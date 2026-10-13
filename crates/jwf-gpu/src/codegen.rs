//! WGSL generation for the iteration kernel.
//!
//! The shader *structure* (xforms, variation types, modes) is compiled into code; all numeric
//! values live in the `prm` storage buffer. Changing a value therefore only needs a buffer upload,
//! while structural changes produce a different [`Program::signature`] and a recompile.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use jwf_core::model::{ColorType, DrawMode, Flame, PostSymmetryType};
use jwf_core::render::prepared::{AffineMode, NEXT_XFORM_TABLE_SIZE, PreparedFlame, PreparedXForm, StepMode, VarStep};
use jwf_core::variations::VarDef;
use jwf_core::variations::wgsl::{prelude, var_fn_name, variation_function};

/// Offsets inside an xform parameter block.
mod xo {
    pub const COEFS: usize = 0;
    pub const POST: usize = 6;
    pub const C1: usize = 12;
    pub const C2: usize = 13;
    pub const COLOR_SYM: usize = 14;
    pub const OPACITY: usize = 15;
    pub const TARGET: usize = 16;
    pub const DIST_MULT: usize = 19;
    pub const COLOR: usize = 20;
    pub const YZ: usize = 21;
    pub const YZ_POST: usize = 27;
    pub const ZX: usize = 33;
    pub const ZX_POST: usize = 39;
    /// Material blending (`material1`, `material2`), used by solid rendering.
    pub const MAT1: usize = 45;
    pub const MAT2: usize = 46;
    pub const VARS: usize = 47;
}

/// Offsets inside a layer header block.
mod lo {
    pub const WEIGHT: usize = 0;
    pub const DENSITY: usize = 1;
    pub const SIZE: usize = 2;
}


#[derive(Clone, Debug)]
pub struct LayerLayout {
    pub header: usize,
    pub xforms: Vec<usize>,
    pub finals: Vec<usize>,
    pub next_base: usize,
    pub cmap_base: usize,
}

/// Generated shader + layout for packing parameters.
pub struct Program {
    pub signature: String,
    pub wgsl: String,
    pub layers: Vec<LayerLayout>,
    pub param_len: usize,
    pub next_len: usize,
    pub layer_count: usize,
    /// Variation names without GPU implementation (ignored on the GPU).
    pub unsupported: Vec<String>,
    /// Start of the DOF parameter block in `prm` (if DOF is active).
    pub dof_base: Option<usize>,
    /// Solid rendering: start of the light block in `prm` (13 floats per shadow map:
    /// projection matrix rows, then the map frame).
    pub solid_base: Option<usize>,
    /// Shadow map index of every light (`None` = casts no shadow).
    pub shadow_maps: Vec<Option<u32>>,
    pub shadow_map_size: usize,
}

/// Floats per shadow map in the solid `prm` block.
pub const SOLID_LIGHT_STRIDE: usize = 13;

/// Offsets inside the DOF parameter block.
mod dofo {
    pub const LEGACY: usize = 0;
    pub const CAM_DOF_10: usize = 1;
    pub const SCALE: usize = 2;
    pub const FADE: usize = 3;
    pub const ROT_ON: usize = 4;
    pub const SIN: usize = 5;
    pub const COS: usize = 6;
    pub const CAM_Z: usize = 7;
    pub const FOCUS: usize = 8;
    pub const AREA: usize = 11;
    pub const FADE_DIST: usize = 12;
    pub const AREA_MINUS_FADE: usize = 13;
    pub const EXPONENT: usize = 14;
    pub const WIDTH: usize = 15;
    pub const VAR_DATA: usize = 16;
}

/// WGSL for `dof_distance` / `dof_apply` (mirrors `jwf_core::render::dof`).
fn dof_wgsl(dof: &jwf_core::render::dof::Dof, base: usize) -> String {
    use jwf_core::render::dof::ShapeMode;
    let b = format!("{base}u");
    let p = |o: usize| format!("prm[{b} + {o}u]");
    let mut w = String::new();
    let _ = write!(
        w,
        "fn dof_fade() -> f32 {{\n  let f = {fade};\n  if (f <= EPSILON) {{ return 1.0; }}\n  if (f >= 1.0 - EPSILON) {{ return rnd(); }}\n  if (rnd() <= f) {{ return rnd(); }}\n  return 1.0;\n}}\n\
fn dof_distance(c: vec3<f32>) -> f32 {{\n  if ({legacy} > 0.5) {{ let zd = {camz} - c.z; return select(-1.0, zd, zd > 0.0); }}\n  let d = c - vec3<f32>({fx}, {fy}, {fz});\n  let dist = pow(dot(d, d), 1.0 / {exp});\n  if (dist > {area}) {{ return dist; }}\n  if (dist > {amf}) {{ let t = clamp((dist - {amf}) / {fd}, 0.0, 1.0); return t * t * t * (t * (t * 6.0 - 15.0) + 10.0) * dist; }}\n  return -1.0;\n}}\n\
fn dof_rotate(o: vec2<f32>, d: vec2<f32>) -> vec2<f32> {{\n  if ({rot} < 0.5) {{ return d; }}\n  let dx = d.x - o.x; let dy = d.y - o.y;\n  return vec2<f32>(o.x + dx * {cos} - dy * {sin}, o.y + dx * {sin} + dy * {cos});\n}}\n",
        fade = p(dofo::FADE),
        legacy = p(dofo::LEGACY),
        camz = p(dofo::CAM_Z),
        fx = p(dofo::FOCUS),
        fy = p(dofo::FOCUS + 1),
        fz = p(dofo::FOCUS + 2),
        exp = p(dofo::EXPONENT),
        area = p(dofo::AREA),
        amf = p(dofo::AREA_MINUS_FADE),
        fd = p(dofo::FADE_DIST),
        rot = p(dofo::ROT_ON),
        sin = p(dofo::SIN),
        cos = p(dofo::COS),
    );
    let k = p(dofo::CAM_DOF_10);
    let sc = p(dofo::SCALE);
    let var = dof.var.as_ref().filter(|(d, _)| d.supports_gpu());
    let body = match (dof.mode, var) {
        (Some(mode), Some((def, _))) => {
            let f = var_fn_name(def);
            let vb = format!("{b} + {}u", dofo::VAR_DATA);
            let abs_dr = if def.name == "xheart_blur_wf" { "abs" } else { "" };
            match mode {
                ShapeMode::Offset | ShapeMode::OffsetNoRotate | ShapeMode::Rect => {
                    let mut s = format!(
                        "  let dr = {abs_dr}({k} * zdist * {sc} * dof_fade());\n  let src = Pt(x, y, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);\n  let r = {f}(src, src, {vb}, dr);\n  var d = vec2<f32>(r.v.x, r.v.y);\n"
                    );
                    if mode == ShapeMode::Rect {
                        let _ = writeln!(s, "  d.x = r.a.x + (d.x - r.a.x) * {};", p(dofo::WIDTH));
                    }
                    if mode != ShapeMode::OffsetNoRotate {
                        s.push_str("  d = dof_rotate(vec2<f32>(r.a.x, r.a.y), d);\n");
                    }
                    s.push_str("  return d;\n");
                    s
                }
                ShapeMode::RandomSource { scale_by_fade, dr_factor, refade } => {
                    format!(
                        "  let fade = dof_fade();\n  let drf = {};\n  let dr = abs({k} * zdist * {sc} * drf) * {dr_factor:.6};\n  let fs = {};\n  let s = Pt((1.0 - 2.0 * rnd()) * fs, (1.0 - 2.0 * rnd()) * fs, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);\n  let r = {f}(s, Pt(x, y, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), {vb}, dr);\n  return dof_rotate(vec2<f32>(x, y), vec2<f32>(r.v.x, r.v.y));\n",
                        if refade { "dof_fade()" } else { "fade" },
                        if scale_by_fade { "fade" } else { "1.0" },
                    )
                }
                ShapeMode::Taurus => format!(
                    "  let fade = dof_fade();\n  let dr = abs({k} * zdist * {sc} * fade) * 0.25;\n  let s = Pt((TWO_PI - 4.0 * rnd() * PI) * fade, (TWO_PI - 4.0 * rnd() * PI) * fade, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);\n  let r = {f}(s, Pt(x, y, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), {vb}, dr);\n  return dof_rotate(vec2<f32>(x, y), vec2<f32>(r.v.x, r.v.y));\n"
                ),
            }
        }
        // Bubble (also the fallback for shapes without GPU support)
        _ => format!("  let dr = dof_fade() * {k} * zdist * {sc};\n  let a = TWO_PI * rnd();\n  return vec2<f32>(x + dr * cos(a), y + dr * sin(a));\n"),
    };
    let _ = write!(w, "fn dof_apply(x: f32, y: f32, zdist: f32) -> vec2<f32> {{\n{body}}}\n");
    w
}

fn mode_tag(m: AffineMode) -> &'static str {
    match m {
        AffineMode::None => "n",
        AffineMode::Flat => "f",
        AffineMode::Full => "3",
    }
}

fn step_mode_tag(m: StepMode) -> &'static str {
    match m {
        StepMode::Pre => "P",
        StepMode::EnforcedPre => "p",
        StepMode::Normal => "N",
        StepMode::Enforced => "e",
        StepMode::Post => "Q",
        StepMode::EnforcedPost => "q",
    }
}

fn xform_block_len(xf: &PreparedXForm) -> usize {
    xo::VARS + xf.pre.iter().chain(&xf.normal).chain(&xf.post_vars).map(|s| 1 + s.data.len()).sum::<usize>()
}

fn color_tag(c: ColorType) -> &'static str {
    match c {
        ColorType::Unset | ColorType::None => "N",
        ColorType::Diffusion => "D",
        ColorType::Cyclic => "C",
        ColorType::Target | ColorType::TargetG => "T",
        ColorType::Distance => "X",
    }
}

fn steps(xf: &PreparedXForm) -> impl Iterator<Item = &VarStep> {
    xf.pre.iter().chain(&xf.normal).chain(&xf.post_vars)
}

fn emit_affine(out: &mut String, mode: AffineMode, b: &str, xy: usize, yz: usize, zx: usize, dst: &str, src: &str) {
    match mode {
        AffineMode::None => {
            let _ = writeln!(out, "  {dst}.x = {src}.x; {dst}.y = {src}.y; {dst}.z = {src}.z;");
        }
        AffineMode::Flat => {
            let _ = writeln!(
                out,
                "  {{ let sx = {src}.x; let sy = {src}.y; {dst}.x = prm[{b} + {}u] * sx + prm[{b} + {}u] * sy + prm[{b} + {}u]; {dst}.y = prm[{b} + {}u] * sx + prm[{b} + {}u] * sy + prm[{b} + {}u]; {dst}.z = {src}.z; }}",
                xy,
                xy + 2,
                xy + 4,
                xy + 1,
                xy + 3,
                xy + 5
            );
        }
        AffineMode::Full => {
            let _ = writeln!(
                out,
                "  {{ var x = prm[{b} + {}u] * {src}.x + prm[{b} + {}u] * {src}.y; var y = prm[{b} + {}u] * {src}.x + prm[{b} + {}u] * {src}.y; var z = {src}.z;\n    let ny = prm[{b} + {}u] * y + prm[{b} + {}u] * z; let nz = prm[{b} + {}u] * y + prm[{b} + {}u] * z; y = ny; z = nz;\n    let nx = prm[{b} + {}u] * x + prm[{b} + {}u] * z; let nz2 = prm[{b} + {}u] * x + prm[{b} + {}u] * z; x = nx; z = nz2;\n    {dst}.x = x + prm[{b} + {}u] + prm[{b} + {}u]; {dst}.y = y + prm[{b} + {}u] + prm[{b} + {}u]; {dst}.z = z + prm[{b} + {}u] + prm[{b} + {}u]; }}",
                xy,
                xy + 2,
                xy + 1,
                xy + 3,
                yz,
                yz + 2,
                yz + 1,
                yz + 3,
                zx,
                zx + 2,
                zx + 1,
                zx + 3,
                xy + 4,
                zx + 4,
                xy + 5,
                yz + 4,
                yz + 5,
                zx + 5
            );
        }
    }
}

fn emit_xform(out: &mut String, fname: &str, xf: &PreparedXForm, base: usize, layer: usize, layout_cmap: usize, smooth: bool) {
    let b = format!("{base}u");
    let _ = writeln!(out, "fn {fname}(src: Pt) -> Pt {{");
    let _ = writeln!(out, "  let B = {b};");
    out.push_str("  var a = Pt(0.0, 0.0, 0.0, 0.0, src.r, src.g, src.b, 0.0, src.hide);\n");
    if xf.color_type == ColorType::Cyclic {
        let _ = writeln!(out, "  var c = fmod(src.color + prm[B + {}u], 1.0); if (c < 0.0) {{ c += 1.0; }} a.color = c;", xo::COLOR_SYM);
    } else {
        let _ = writeln!(out, "  a.color = src.color * prm[B + {}u] + prm[B + {}u];", xo::C1, xo::C2);
    }
    emit_affine(out, xf.affine_mode, "B", xo::COEFS, xo::YZ, xo::ZX, "a", "src");
    out.push_str("  var v = Pt(0.0, 0.0, 0.0, a.color, a.r, a.g, a.b, a.rgbc, a.hide);\n");
    let mut off = xo::VARS;
    let mut emit_steps = |out: &mut String, list: &[VarStep]| {
        for s in list {
            let f = var_fn_name(s.def);
            let call_base = off + 1;
            let amt = format!("prm[B + {off}u]");
            match s.mode {
                StepMode::Pre | StepMode::Normal | StepMode::Post => {
                    let _ = writeln!(out, "  {{ let r = {f}(a, v, B + {call_base}u, {amt}); a = r.a; v = r.v; }}");
                }
                StepMode::EnforcedPre => {
                    let _ = writeln!(out, "  {{ let r = {f}(a, a, B + {call_base}u, {amt}); a = r.v; }}");
                }
                StepMode::Enforced | StepMode::EnforcedPost => {
                    let _ = writeln!(out, "  {{ let r = {f}(v, v, B + {call_base}u, {amt}); v = r.v; }}");
                }
            }
            off += 1 + s.data.len();
        }
    };
    emit_steps(out, &xf.pre);
    out.push_str("  v.color = a.color; v.r = a.r; v.g = a.g; v.b = a.b; v.rgbc = a.rgbc; v.hide = a.hide;\n");
    emit_steps(out, &xf.normal);
    emit_steps(out, &xf.post_vars);
    out.push_str("  var d = v;\n");
    emit_affine(out, xf.post_mode, "B", xo::POST, xo::YZ_POST, xo::ZX_POST, "d", "v");
    out.push_str("  if (d.rgbc == 0.0) {\n");
    match xf.color_type {
        ColorType::Diffusion | ColorType::Cyclic => {
            if smooth {
                let _ = writeln!(
                    out,
                    "  {{ let fi = d.color * 254.0; let li = i32(fi); let t = fi - trunc(fi); var lc = vec4<f32>(0.0); var rc = vec4<f32>(0.0);\n    if (li >= 0 && li < 256) {{ lc = cmap[{layout_cmap}u + u32(li)]; }} if (li + 1 >= 0 && li + 1 < 256) {{ rc = cmap[{layout_cmap}u + u32(li + 1)]; }}\n    let c = mix(lc, rc, t); d.r = c.x; d.g = c.y; d.b = c.z; }}"
                );
            } else {
                let _ = writeln!(
                    out,
                    "  {{ let ci = u32(clamp(i32(d.color * 254.0 + 0.5), 0, 255)); let c = cmap[{layout_cmap}u + ci]; d.r = c.x; d.g = c.y; d.b = c.z; }}"
                );
            }
        }
        ColorType::Target | ColorType::TargetG => {
            let _ = writeln!(
                out,
                "  {{ let t = (prm[B + {}u] + 1.0) / 2.0; d.r = mix(prm[B + {}u], d.r, t); d.g = mix(prm[B + {}u], d.g, t); d.b = mix(prm[B + {}u], d.b, t); }}",
                xo::COLOR_SYM,
                xo::TARGET,
                xo::TARGET + 1,
                xo::TARGET + 2
            );
        }
        ColorType::Distance => {
            let _ = writeln!(
                out,
                "  {{ let dist = sqrt(sqr(src.x - d.x) + sqr(src.y - d.y) + sqr(src.z - d.z)); let ci = u32(i32((prm[B + {}u] + dist * prm[B + {}u]) * 254.0 + 0.5) & 255); let c = cmap[{layout_cmap}u + ci]; d.r = c.x; d.g = c.y; d.b = c.z; }}",
                xo::COLOR,
                xo::DIST_MULT
            );
        }
        ColorType::None | ColorType::Unset => {}
    }
    out.push_str("  }\n");
    let _ = layer;
    out.push_str("  return d;\n}\n");
}

impl Program {
    pub fn new(flame: &Flame, prepared: &PreparedFlame) -> Self {
        Self::new_scaled(flame, prepared, 1.0)
    }

    /// `ppu_scale` = output size relative to the flame size (scales the shadow map size).
    pub fn new_scaled(flame: &Flame, prepared: &PreparedFlame, ppu_scale: f64) -> Self {
        let mut sig = String::new();
        let mut unsupported: Vec<String> = prepared.missing_variations.clone();
        let mut used_vars: BTreeMap<&'static str, &'static VarDef> = BTreeMap::new();
        let mut layers = Vec::new();
        let mut param_len = 0usize;
        let mut next_len = 0usize;

        let _ = write!(sig, "pz{}|sym{:?}|3d{}|", prepared.preserve_z, flame.post_symmetry.kind, flame.is_3d_projection_required());
        let solid = flame.solid.enabled;
        let mut shadow_maps = Vec::new();
        let mut map_count = 0u32;
        if solid {
            let lights = jwf_core::render::solid::LightViews::new(&flame.solid);
            for m in &lights.proj {
                shadow_maps.push(m.map(|_| {
                    map_count += 1;
                    map_count - 1
                }));
            }
        }
        let shadow_map_size = if map_count > 0 { jwf_core::render::solid::shadow_map_size(flame, ppu_scale) } else { 0 };
        if solid {
            let _ = write!(sig, "solid{map_count}x{shadow_map_size}|");
        }
        if flame.post_blur_radius > 0 {
            let _ = write!(sig, "pb{}/{}/{}|", flame.post_blur_radius, flame.post_blur_fade, flame.post_blur_falloff);
        }
        for (li, layer) in prepared.layers.iter().enumerate() {
            let header = param_len;
            param_len += lo::SIZE;
            let mut xforms = Vec::new();
            let mut finals = Vec::new();
            let _ = write!(sig, "L{}s{}:", li, layer.smooth_gradient as u8);
            for (k, xf) in layer.xforms.iter().chain(layer.finals.iter()).enumerate() {
                let is_final = k >= layer.xforms.len();
                if is_final {
                    finals.push(param_len);
                } else {
                    xforms.push(param_len);
                }
                param_len += xform_block_len(xf);
                let _ = write!(
                    sig,
                    "{}{}{}{}{}{}[",
                    if is_final { "F" } else { "X" },
                    mode_tag(xf.affine_mode),
                    mode_tag(xf.post_mode),
                    color_tag(xf.color_type),
                    match xf.draw_mode {
                        DrawMode::Normal => "n",
                        DrawMode::Opaque => "o",
                        DrawMode::Hidden => "h",
                    },
                    if xf.next.is_some() { "t" } else { "-" }
                );
                for s in steps(xf) {
                    let _ = write!(sig, "{}{}:{},", s.def.name, step_mode_tag(s.mode), s.data.len());
                    if s.def.supports_gpu() {
                        used_vars.insert(s.def.name, s.def);
                    } else if !unsupported.iter().any(|n| n == s.def.name) {
                        unsupported.push(s.def.name.to_string());
                    }
                }
                sig.push(']');
            }
            let next_base = next_len;
            next_len += layer.xforms.len() * NEXT_XFORM_TABLE_SIZE;
            layers.push(LayerLayout { header, xforms, finals, next_base, cmap_base: li * 256 });
            sig.push('|');
        }

        // ---- WGSL ----
        let mut w = String::with_capacity(32 * 1024);
        w.push_str(&prelude());
        w.push_str(ITER_BINDINGS);
        for def in used_vars.values() {
            w.push_str(&variation_function(def, prepared.preserve_z));
        }
        // Variations without GPU code compile to a no-op.
        w.push_str("fn var_unsupported(a_in: Pt, v_in: Pt, base: u32, AMOUNT: f32) -> PtPair { return PtPair(a_in, v_in); }\n");

        for (li, layer) in prepared.layers.iter().enumerate() {
            let lay = &layers[li];
            for (k, xf) in layer.xforms.iter().enumerate() {
                let mut xf2 = xf.clone();
                strip_unsupported(&mut xf2);
                emit_xform(&mut w, &format!("xf_{li}_{k}"), &xf2, lay.xforms[k], li, lay.cmap_base, layer.smooth_gradient);
            }
            for (k, xf) in layer.finals.iter().enumerate() {
                let mut xf2 = xf.clone();
                strip_unsupported(&mut xf2);
                emit_xform(&mut w, &format!("fxf_{li}_{k}"), &xf2, lay.finals[k], li, lay.cmap_base, layer.smooth_gradient);
            }
            // Dispatcher
            let _ = writeln!(w, "fn apply_xf_{li}(idx: u32, p: Pt) -> Pt {{\n  switch idx {{");
            for k in 0..layer.xforms.len() {
                let _ = writeln!(w, "    case {k}u: {{ return xf_{li}_{k}(p); }}");
            }
            let _ = writeln!(w, "    default: {{ return p; }}\n  }}\n}}");
            if solid {
                let _ = writeln!(w, "fn mat_xf_{li}(idx: u32, m: f32) -> f32 {{\n  switch idx {{");
                for k in 0..layer.xforms.len() {
                    let _ = writeln!(w, "    case {k}u: {{ return m * prm[{b}u + {m1}u] + prm[{b}u + {m2}u]; }}", b = lay.xforms[k], m1 = xo::MAT1, m2 = xo::MAT2);
                }
                let _ = writeln!(w, "    default: {{ return m; }}\n  }}\n}}");
            }
            // Visibility (draw mode)
            let _ = writeln!(w, "fn visible_{li}(idx: u32) -> bool {{\n  switch idx {{");
            for (k, xf) in layer.xforms.iter().enumerate() {
                match xf.draw_mode {
                    DrawMode::Normal => {}
                    DrawMode::Hidden => {
                        let _ = writeln!(w, "    case {k}u: {{ return false; }}");
                    }
                    DrawMode::Opaque => {
                        let _ = writeln!(w, "    case {k}u: {{ return rnd() <= prm[{}u + {}u]; }}", lay.xforms[k], xo::OPACITY);
                    }
                }
            }
            let _ = writeln!(w, "    default: {{ return true; }}\n  }}\n}}");
            // Finals
            let _ = writeln!(w, "fn finals_{li}(p: Pt) -> Pt {{\n  var q = p;");
            for k in 0..layer.finals.len() {
                let _ = writeln!(w, "  q = fxf_{li}_{k}(q);");
            }
            let _ = writeln!(w, "  return q;\n}}");
            let has_table: Vec<bool> = layer.xforms.iter().map(|x| x.next.is_some()).collect();
            let _ = writeln!(w, "fn has_next_{li}(idx: u32) -> bool {{\n  switch idx {{");
            for (k, t) in has_table.iter().enumerate() {
                if !t {
                    let _ = writeln!(w, "    case {k}u: {{ return false; }}");
                }
            }
            let _ = writeln!(w, "    default: {{ return true; }}\n  }}\n}}");
        }

        // Plot with post symmetry
        // Depth of field
        let dof = jwf_core::render::dof::Dof::new(flame);
        let mut dof_base = None;
        if let Some(d) = &dof {
            let base = param_len;
            let data_len = d.var.as_ref().map(|v| v.1.len()).unwrap_or(0);
            param_len += dofo::VAR_DATA + data_len;
            dof_base = Some(base);
            if let Some((def, _)) = d.var.as_ref().filter(|(d, _)| d.supports_gpu())
                && !used_vars.contains_key(def.name)
            {
                w.push_str(&variation_function(def, prepared.preserve_z));
            }
            let _ = write!(sig, "dof{}{:?}{}|", d.legacy, d.mode, d.var.as_ref().map(|v| v.0.name).unwrap_or("-"));
            w.push_str(&dof_wgsl(d, base));
        }
        let mut solid_base = None;
        if solid {
            let base = param_len;
            param_len += map_count as usize * SOLID_LIGHT_STRIDE;
            solid_base = Some(base);
            w.push_str(&solid_plot_wgsl(base, map_count, shadow_map_size, dof.is_some()));
        } else if dof.is_some() {
            w.push_str(&PROJECT_FN.replace(
                "        x = cx / zr;\n        y = cy / zr;",
                "        let dd = dof_distance(vec3<f32>(cx, cy, cz));\n        if (dd >= 0.0) { let bp = dof_apply(cx, cy, dd); x = bp.x / zr; y = bp.y / zr; } else { x = cx / zr; y = cy / zr; }",
            ));
        } else {
            w.push_str(PROJECT_FN);
        }
        if solid {
            // Post blur does not apply to the z-buffer.
        } else if flame.post_blur_radius > 0 {
            w.push_str(
                &SPLAT_POST_BLUR
                    .replace("PB_R", &format!("{}", flame.post_blur_radius.min(64) as i32))
                    .replace("PB_FADE", &format!("{:?}", flame.post_blur_fade.clamp(0.0, 1.0) as f32))
                    .replace("PB_FALLOFF", &format!("{:?}", flame.post_blur_falloff as f32)),
            );
        } else {
            w.push_str(SPLAT_PLAIN);
        }
        match flame.post_symmetry.kind {
            PostSymmetryType::None => w.push_str("fn plot_sym(q: Pt, col: vec3<f32>) { plot(q.x, q.y, q.z, col); }\n"),
            PostSymmetryType::Point => w.push_str(PLOT_POINT_SYM),
            PostSymmetryType::XAxis => w.push_str(&PLOT_AXIS_SYM.replace("AXIS_X", "true")),
            PostSymmetryType::YAxis => w.push_str(&PLOT_AXIS_SYM.replace("AXIS_X", "false")),
        }

        // Main kernel
        w.push_str(ITER_MAIN_HEAD);
        for li in 0..prepared.layers.len() {
            let lay = &layers[li];
            let _ = writeln!(
                w,
                "    case {li}u: {{\n      cmap_base = {cb}u;\n      let density = prm[{h}u + {d}u];\n      let weight = prm[{h}u + {wt}u];\n      for (var i = 0u; i < U.iters; i++) {{\n        if (density < 1.0 && rnd() > density * density) {{ continue; }}\n        if (!has_next_{li}(xf)) {{ break; }}\n        xf = nxt[{nb}u + xf * {ts}u + (rnd_u32() & {mask}u)];\n        p = apply_xf_{li}(xf, p);{mat_step}\n        if (bad(p)) {{ p = fresh_point(); skip = FUSE_ITERS;{mat_reset} continue; }}\n        if (skip > 0u) {{ skip -= 1u; continue; }}\n        if (!visible_{li}(xf)) {{ continue; }}\n        let q = finals_{li}(p);\n        if (q.hide != 0.0) {{ continue; }}{mat_plot}\n        plot_sym(q, vec3<f32>(q.r, q.g, q.b) * weight);\n      }}\n    }}",
                h = lay.header,
                d = lo::DENSITY,
                wt = lo::WEIGHT,
                nb = lay.next_base,
                ts = NEXT_XFORM_TABLE_SIZE,
                mask = NEXT_XFORM_TABLE_SIZE - 1,
                cb = lay.cmap_base,
                mat_step = if solid { format!(" mat = mat_xf_{li}(xf, mat);") } else { String::new() },
                mat_reset = if solid { " mat = rnd();" } else { "" },
                mat_plot = if solid { " cur_mat = mat;" } else { "" },
            );
        }
        w.push_str(ITER_MAIN_TAIL);

        Self { signature: sig, wgsl: w, layer_count: layers.len(), layers, param_len, next_len, unsupported, dof_base, solid_base, shadow_maps, shadow_map_size }
    }

    /// Packs all numeric values into the `prm` buffer layout of this program.
    pub fn pack_params(&self, flame: &Flame, prepared: &PreparedFlame) -> Vec<f32> {
        let mut p = vec![0.0f32; self.param_len.max(1)];
        if let (Some(base), Some(d)) = (self.dof_base, jwf_core::render::dof::Dof::new(flame)) {
            let mut put = |o: usize, v: f64| p[base + o] = v as f32;
            put(dofo::LEGACY, d.legacy as u8 as f64);
            put(dofo::CAM_DOF_10, d.cam_dof_10);
            put(dofo::SCALE, d.scale);
            put(dofo::FADE, d.fade);
            put(dofo::ROT_ON, d.rotate.is_some() as u8 as f64);
            let (s, c) = d.rotate.unwrap_or((0.0, 1.0));
            put(dofo::SIN, s);
            put(dofo::COS, c);
            put(dofo::CAM_Z, d.cam_z);
            for i in 0..3 {
                put(dofo::FOCUS + i, d.focus[i]);
            }
            put(dofo::AREA, d.area);
            put(dofo::FADE_DIST, d.fade_dist);
            put(dofo::AREA_MINUS_FADE, d.area_minus_fade);
            put(dofo::EXPONENT, d.exponent);
            put(dofo::WIDTH, d.width);
            if let Some((_, data)) = &d.var {
                for (i, v) in data.iter().enumerate() {
                    if base + dofo::VAR_DATA + i < p.len() {
                        p[base + dofo::VAR_DATA + i] = *v as f32;
                    }
                }
            }
        }
        for (li, layer) in prepared.layers.iter().enumerate() {
            let lay = &self.layers[li];
            p[lay.header + lo::WEIGHT] = layer.weight as f32;
            p[lay.header + lo::DENSITY] = layer.density as f32;
            for (k, xf) in layer.xforms.iter().enumerate() {
                pack_xform(&mut p, lay.xforms[k], xf);
            }
            for (k, xf) in layer.finals.iter().enumerate() {
                pack_xform(&mut p, lay.finals[k], xf);
            }
        }
        p
    }

    /// Writes the light projections and shadow map frames into `prm`.
    pub fn pack_solid(&self, p: &mut [f32], flame: &Flame, frames: &[Option<jwf_core::render::solid::ShadowFrame>]) {
        let Some(base) = self.solid_base else { return };
        let lights = jwf_core::render::solid::LightViews::new(&flame.solid);
        for (li, map) in self.shadow_maps.iter().enumerate() {
            let (Some(k), Some(Some(m))) = (map, lights.proj.get(li)) else { continue };
            let o = base + *k as usize * SOLID_LIGHT_STRIDE;
            for r in 0..3 {
                for c in 0..3 {
                    p[o + r * 3 + c] = m[r][c] as f32;
                }
            }
            let f = frames.get(li).copied().flatten().unwrap_or_default();
            p[o + 9] = f.scale_x as f32;
            p[o + 10] = f.scale_y as f32;
            p[o + 11] = f.centre_x as f32;
            p[o + 12] = f.centre_y as f32;
        }
    }

    pub fn pack_next(&self, prepared: &PreparedFlame) -> Vec<u32> {
        let mut n = vec![0u32; self.next_len.max(1)];
        for (li, layer) in prepared.layers.iter().enumerate() {
            let base = self.layers[li].next_base;
            for (k, xf) in layer.xforms.iter().enumerate() {
                if let Some(t) = &xf.next {
                    for (i, v) in t.iter().enumerate() {
                        n[base + k * NEXT_XFORM_TABLE_SIZE + i] = *v as u32;
                    }
                }
            }
        }
        n
    }

    pub fn pack_colormaps(&self, prepared: &PreparedFlame) -> Vec<[f32; 4]> {
        let mut c = vec![[0.0f32; 4]; (prepared.layers.len() * 256).max(1)];
        for (li, layer) in prepared.layers.iter().enumerate() {
            for (i, col) in layer.colormap.iter().enumerate().take(256) {
                c[li * 256 + i] = [col[0] as f32, col[1] as f32, col[2] as f32, 1.0];
            }
        }
        c
    }
}

fn strip_unsupported(xf: &mut PreparedXForm) {
    // Keep the parameter layout identical: unsupported steps become no-ops via a
    // dedicated function name, so offsets stay aligned with `pack_params`.
    for s in xf.pre.iter_mut().chain(xf.normal.iter_mut()).chain(xf.post_vars.iter_mut()) {
        if !s.def.supports_gpu() {
            s.def = &UNSUPPORTED_DEF;
        }
    }
}

static UNSUPPORTED_DEF: VarDef = VarDef {
    name: "unsupported",
    params: &[],
    kind: jwf_core::variations::VarKind::Normal,
    is_3d: false,
    apply: |_, _, _, _, _| {},
    prepare: None,
    wgsl: "",
};

fn pack_xform(p: &mut [f32], base: usize, xf: &PreparedXForm) {
    let put6 = |p: &mut [f32], o: usize, a: &jwf_core::model::Affine| {
        for (i, v) in a.as_array().iter().enumerate() {
            p[base + o + i] = *v as f32;
        }
    };
    put6(p, xo::COEFS, &xf.coefs);
    put6(p, xo::POST, &xf.post);
    put6(p, xo::YZ, &xf.yz);
    put6(p, xo::YZ_POST, &xf.yz_post);
    put6(p, xo::ZX, &xf.zx);
    put6(p, xo::ZX_POST, &xf.zx_post);
    p[base + xo::C1] = xf.c1 as f32;
    p[base + xo::C2] = xf.c2 as f32;
    p[base + xo::COLOR_SYM] = xf.color_symmetry as f32;
    p[base + xo::OPACITY] = xf.opacity as f32;
    for i in 0..3 {
        p[base + xo::TARGET + i] = xf.target_color[i] as f32;
    }
    p[base + xo::DIST_MULT] = xf.dist_mult as f32;
    p[base + xo::COLOR] = xf.color as f32;
    p[base + xo::MAT1] = xf.material.0 as f32;
    p[base + xo::MAT2] = xf.material.1 as f32;
    let mut off = base + xo::VARS;
    for s in steps(xf) {
        p[off] = s.amount as f32;
        for (i, v) in s.data.iter().enumerate() {
            p[off + 1 + i] = *v as f32;
        }
        off += 1 + s.data.len();
    }
}

/// Solid rendering plot: z-buffer + point attributes + shadow maps (atomic max of an
/// order-preserving key).
fn solid_plot_wgsl(base: usize, maps: u32, map_size: usize, dof: bool) -> String {
    let mut w = String::from(
        "var<private> cur_mat: f32;\n\
@group(0) @binding(6) var<storage, read_write> zbuf: array<atomic<u32>>;\n\
@group(0) @binding(7) var<storage, read_write> sattr: array<f32>;\n",
    );
    if maps > 0 {
        w.push_str("@group(0) @binding(8) var<storage, read_write> smap: array<atomic<u32>>;\n");
    }
    w.push_str(
        "fn zkey(f: f32) -> u32 { let b = bitcast<u32>(f); if ((b & 0x80000000u) != 0u) { return ~b; } return b | 0x80000000u; }\n\
fn plot(px_in: f32, py_in: f32, pz: f32, col_in: vec3<f32>) {\n  let wp = vec3<f32>(px_in, py_in, pz);\n",
    );
    for k in 0..maps as usize {
        let o = base + k * SOLID_LIGHT_STRIDE;
        let _ = writeln!(
            w,
            "  {{ let lp = vec3<f32>(dot(vec3<f32>(prm[{o}u], prm[{o1}u], prm[{o2}u]), wp), dot(vec3<f32>(prm[{o3}u], prm[{o4}u], prm[{o5}u]), wp), dot(vec3<f32>(prm[{o6}u], prm[{o7}u], prm[{o8}u]), wp));\n    let mx = i32(prm[{o9}u] * lp.x + prm[{o11}u] + 0.5); let my = i32(prm[{o10}u] * lp.y + prm[{o12}u] + 0.5);\n    if (mx >= 0 && my >= 0 && mx < {ms} && my < {ms}) {{ atomicMax(&smap[{off}u + u32(mx) * {ms}u + u32(my)], zkey(lp.z)); }} }}",
            o1 = o + 1,
            o2 = o + 2,
            o3 = o + 3,
            o4 = o + 4,
            o5 = o + 5,
            o6 = o + 6,
            o7 = o + 7,
            o8 = o + 8,
            o9 = o + 9,
            o10 = o + 10,
            o11 = o + 11,
            o12 = o + 12,
            ms = map_size,
            off = k * map_size * map_size,
        );
    }
    w.push_str(
        "  let cx = U.m0.x * px_in + U.m1.x * py_in + U.m2.x * pz + U.cam_pos.x;\n\
  let cy = U.m0.y * px_in + U.m1.y * py_in + U.m2.y * pz + U.cam_pos.y;\n\
  let cz = U.m0.z * px_in + U.m1.z * py_in + U.m2.z * pz + U.cam_pos.z;\n\
  let zr = 1.0 - U.cam_persp * cz + U.cam_pos.z;\n\
  if (zr < EPSILON) { return; }\n\
  var intensity = 1.0;\n\
  if (U.dimish_z > EPSILON) { let zdist = U.dim_z_dist - cz; if (zdist > 0.0) { intensity = exp(-zdist * zdist * U.dimish_z); } }\n\
  var dof = 0.0;\n",
    );
    if dof {
        w.push_str("  let dd = dof_distance(vec3<f32>(cx, cy, cz)); if (dd >= 0.0) { dof = dd; }\n");
    }
    w.push_str(
        "  let x = cx / zr; let y = cy / zr;\n\
  let px = x * U.cosa + y * U.sina + U.rc_x;\n\
  if (px < 0.0 || px > U.cam_w) { return; }\n\
  let py = y * U.cosa - x * U.sina + U.rc_y;\n\
  if (py < 0.0 || py > U.cam_h) { return; }\n\
  let ix = i32(U.bws * px + 0.5);\n\
  let iy = i32(U.bhs * py + 0.5);\n\
  if (ix < 0 || iy < 0 || ix >= i32(U.raster_w) || iy >= i32(U.raster_h)) { return; }\n\
  let idx = u32(iy) * U.raster_w + u32(ix);\n\
  let key = zkey(cz * U.bws);\n\
  let old = atomicMax(&zbuf[idx], key);\n\
  if (key > old) {\n\
    let col = col_in * intensity;\n\
    let b = idx * 8u;\n\
    sattr[b] = px_in; sattr[b + 1u] = py_in; sattr[b + 2u] = pz;\n\
    sattr[b + 3u] = col.x; sattr[b + 4u] = col.y; sattr[b + 5u] = col.z;\n\
    sattr[b + 6u] = cur_mat; sattr[b + 7u] = dof;\n\
  }\n\
}\n\
fn splat(ix: i32, iy: i32, col: vec3<f32>) { }\n",
    );
    w
}

const ITER_BINDINGS: &str = r#"
struct IterUniforms {
    // view
    cosa: f32, sina: f32, rc_x: f32, rc_y: f32,
    bws: f32, bhs: f32, cam_w: f32, cam_h: f32,
    m0: vec4<f32>, m1: vec4<f32>, m2: vec4<f32>,
    cam_pos: vec4<f32>,
    cam_persp: f32, dimish_z: f32, dim_z_dist: f32, project_3d: u32,
    raster_w: u32, raster_h: u32, iters: u32, thread_count: u32,
    aa_amount: f32, aa_radius: f32, color_scale: f32, layer_count: u32,
    sym_cx: f32, sym_cy: f32, sym_halve_dist: f32, sym_order: u32,
    sym_rot_sin: f32, sym_rot_cos: f32, sym_do_rot: u32, _pad0: u32,
}

struct State { x: f32, y: f32, z: f32, color: f32, r: f32, g: f32, b: f32, xf: u32, rng: u32, skip: u32, hide: f32, mat: f32 }

@group(0) @binding(0) var<uniform> U: IterUniforms;
@group(0) @binding(1) var<storage, read> prm: array<f32>;
@group(0) @binding(2) var<storage, read> nxt: array<u32>;
@group(0) @binding(4) var<storage, read_write> states: array<State>;
@group(0) @binding(5) var<storage, read_write> hist: array<atomic<i32>>;

const FUSE_ITERS: u32 = 30u;

fn bad(p: Pt) -> bool {
    let s = p.x + p.y + p.z;
    return !(s == s) || abs(p.x) > 1.0e18 || abs(p.y) > 1.0e18 || abs(p.z) > 1.0e18;
}

fn fresh_point() -> Pt {
    return Pt(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0, 0.0, rnd(), 0.0, 0.0, 0.0, 0.0, 0.0);
}
"#;

const PROJECT_FN: &str = r#"
fn plot(px_in: f32, py_in: f32, pz: f32, col_in: vec3<f32>) {
    var x = px_in;
    var y = py_in;
    var intensity = 1.0;
    if (U.project_3d != 0u) {
        let cx = U.m0.x * x + U.m1.x * y + U.m2.x * pz + U.cam_pos.x;
        let cy = U.m0.y * x + U.m1.y * y + U.m2.y * pz + U.cam_pos.y;
        let cz = U.m0.z * x + U.m1.z * y + U.m2.z * pz + U.cam_pos.z;
        let zr = 1.0 - U.cam_persp * cz + U.cam_pos.z;
        if (zr < EPSILON) { return; }
        if (U.dimish_z > EPSILON) {
            let zdist = U.dim_z_dist - cz;
            if (zdist > 0.0) { intensity = exp(-zdist * zdist * U.dimish_z); }
        }
        x = cx / zr;
        y = cy / zr;
    }
    let px = x * U.cosa + y * U.sina + U.rc_x;
    if (px < 0.0 || px > U.cam_w) { return; }
    let py = y * U.cosa - x * U.sina + U.rc_y;
    if (py < 0.0 || py > U.cam_h) { return; }
    var rx = U.bws * px;
    var ry = U.bhs * py;
    if (U.aa_amount > EPSILON && U.aa_radius > EPSILON && rnd() > 1.0 - U.aa_amount) {
        let dr = exp(U.aa_radius * sqrt(-log(max(rnd(), 1.0e-12)))) - 1.0;
        let da = rnd() * TWO_PI;
        rx += dr * cos(da);
        ry += dr * sin(da);
    }
    let ix = i32(floor(rx + 0.5));
    let iy = i32(floor(ry + 0.5));
    if (ix < 0 || iy < 0 || ix >= i32(U.raster_w) || iy >= i32(U.raster_h)) { return; }
    splat(ix, iy, col_in * intensity);
}

fn hist_add(ix: i32, iy: i32, col: vec3<f32>) {
    let idx = (u32(iy) * U.raster_w + u32(ix)) * 4u;
    atomicAdd(&hist[idx], 1);
    atomicAdd(&hist[idx + 1u], i32(col.x * U.color_scale));
    atomicAdd(&hist[idx + 2u], i32(col.y * U.color_scale));
    atomicAdd(&hist[idx + 3u], i32(col.z * U.color_scale));
}
"#;

const SPLAT_PLAIN: &str = "fn splat(ix: i32, iy: i32, col: vec3<f32>) { hist_add(ix, iy, col); }\n";

/// `PostBlurRenderIterationState`; `PB_R`, `PB_FADE` and `PB_FALLOFF` are substituted.
const SPLAT_POST_BLUR: &str = r#"
fn splat(ix: i32, iy: i32, col: vec3<f32>) {
    if (rnd() <= PB_FADE) { hist_add(ix, iy, col); return; }
    for (var ky = -PB_R; ky <= PB_R; ky++) {
        let y = iy + ky;
        if (y < 0 || y >= i32(U.raster_h)) { continue; }
        for (var kx = -PB_R; kx <= PB_R; kx++) {
            let x = ix + kx;
            if (x < 0 || x >= i32(U.raster_w)) { continue; }
            let r = sqrt(f32(kx * kx + ky * ky)) / f32(PB_R);
            hist_add(x, y, col * exp2(-r * PB_FALLOFF));
        }
    }
}
"#;

const PLOT_POINT_SYM: &str = r#"
fn plot_sym(q: Pt, col: vec3<f32>) {
    let dx = q.x - U.sym_cx;
    let dy = q.y - U.sym_cy;
    plot(q.x, q.y, q.z, col);
    let da = TWO_PI / f32(U.sym_order);
    for (var i = 0u; i < U.sym_order; i++) {
        let s = sin(da * f32(i));
        let c = cos(da * f32(i));
        plot(U.sym_cx + dx * c + dy * s, U.sym_cy + dy * c - dx * s, q.z, col);
    }
}
"#;

const PLOT_AXIS_SYM: &str = r#"
fn plot_sym(q: Pt, col: vec3<f32>) {
    var ax = q.x; var ay = q.y; var bx = q.x; var by = q.y;
    if (AXIS_X) {
        let dx = q.x - U.sym_cx;
        ax = U.sym_cx + dx + U.sym_halve_dist;
        bx = U.sym_cx - dx - U.sym_halve_dist;
    } else {
        let dy = q.y - U.sym_cy;
        ay = U.sym_cy + dy + U.sym_halve_dist;
        by = U.sym_cy - dy - U.sym_halve_dist;
    }
    if (U.sym_do_rot != 0u) {
        let s = U.sym_rot_sin; let c = U.sym_rot_cos;
        var dx = ax - U.sym_cx; var dy = ay - U.sym_cy;
        ax = U.sym_cx + dx * c + dy * s; ay = U.sym_cy + dy * c - dx * s;
        dx = bx - U.sym_cx; dy = by - U.sym_cy;
        bx = U.sym_cx + dx * c - dy * s; by = U.sym_cy + dy * c + dx * s;
    }
    plot(ax, ay, q.z, col);
    plot(bx, by, q.z, col);
}
"#;

const ITER_MAIN_HEAD: &str = r#"
@compute @workgroup_size(64)
fn iterate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let tid = gid.x;
    if (tid >= U.thread_count) { return; }
    var s = states[tid];
    rng_state = s.rng;
    var p = Pt(s.x, s.y, s.z, s.color, s.r, s.g, s.b, 0.0, s.hide);
    var xf = s.xf;
    var skip = s.skip;
    var mat = s.mat;
    let layer = tid % max(U.layer_count, 1u);
    switch layer {
"#;

const ITER_MAIN_TAIL: &str = r#"
    default: {}
    }
    s.x = p.x; s.y = p.y; s.z = p.z; s.color = p.color; s.r = p.r; s.g = p.g; s.b = p.b;
    s.xf = xf; s.skip = skip; s.rng = rng_state; s.hide = p.hide; s.mat = mat;
    states[tid] = s;
}
"#;

/// Validates WGSL with naga (used by tests and debug builds).
pub fn validate_wgsl(src: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(src).map_err(|e| e.emit_to_string(src))?;
    let mut validator = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
    validator.validate(&module).map_err(|e| e.emit_to_string(src))?;
    Ok(())
}

