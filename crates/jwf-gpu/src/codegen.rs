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
    pub const VARS: usize = 45;
}

/// Offsets inside a layer header block.
mod lo {
    pub const WEIGHT: usize = 0;
    pub const DENSITY: usize = 1;
    pub const SIZE: usize = 2;
}

/// Shared WGSL prelude: types, RNG and math helpers available to variation snippets.
pub const PRELUDE: &str = r#"
const PI: f32 = 3.14159265358979;
const TWO_PI: f32 = 6.28318530717959;
const HALF_PI: f32 = 1.5707963267949;
const EPS: f32 = 1.0e-20;
const EPSILON: f32 = 1.0e-8;

struct Pt { x: f32, y: f32, z: f32, color: f32, r: f32, g: f32, b: f32 }
struct PtPair { a: Pt, v: Pt }

var<private> rng_state: u32;

fn rnd_u32() -> u32 {
    rng_state = rng_state * 747796405u + 2891336453u;
    let w = ((rng_state >> ((rng_state >> 28u) + 4u)) ^ rng_state) * 277803737u;
    return (w >> 22u) ^ w;
}
fn rnd() -> f32 { return f32(rnd_u32() >> 8u) * (1.0 / 16777216.0); }

fn sqr(x: f32) -> f32 { return x * x; }
fn sumsq(p: Pt) -> f32 { return p.x * p.x + p.y * p.y; }
fn rad(p: Pt) -> f32 { return sqrt(p.x * p.x + p.y * p.y) + EPS; }
fn atan_xy(p: Pt) -> f32 { return atan2(p.x, p.y); }
fn atan_yx(p: Pt) -> f32 { return atan2(p.y, p.x); }
fn log10(x: f32) -> f32 { return log(x) * 0.434294481903252; }
fn fmod(a: f32, b: f32) -> f32 { return a - b * trunc(a / b); }
fn sgn(x: f32) -> f32 { return sign(x); }
"#;

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

/// Replaces `D(k)` with `prm[base + k]` in a snippet.
fn expand_snippet(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 64);
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let is_ident_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if !is_ident_before && b[i] == b'D' && i + 1 < b.len() && b[i + 1] == b'(' {
            if let Some(end) = src[i + 2..].find(')') {
                let inner = src[i + 2..i + 2 + end].trim();
                if !inner.is_empty() && inner.bytes().all(|c| c.is_ascii_digit()) {
                    let _ = write!(out, "prm[base + {inner}u]");
                    i += 2 + end + 1;
                    continue;
                }
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

/// WGSL function name for a variation.
fn var_fn_name(def: &VarDef) -> String {
    let clean: String = def.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("var_{clean}")
}

/// Emits a variation as a WGSL function `fn var_x(a_in, v_in, base, AMOUNT) -> PtPair`.
pub fn variation_function(def: &VarDef, preserve_z: bool) -> String {
    let body = expand_snippet(def.wgsl).replace("PRESERVE_Z", if preserve_z { "true" } else { "false" });
    format!(
        "fn {}(a_in: Pt, v_in: Pt, base: u32, AMOUNT: f32) -> PtPair {{\n  var a = a_in;\n  var v = v_in;\n  {{\n{}\n  }}\n  return PtPair(a, v);\n}}\n",
        var_fn_name(def),
        body
    )
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
    out.push_str("  var a = Pt(0.0, 0.0, 0.0, 0.0, src.r, src.g, src.b);\n");
    if xf.color_type == ColorType::Cyclic {
        let _ = writeln!(out, "  var c = fmod(src.color + prm[B + {}u], 1.0); if (c < 0.0) {{ c += 1.0; }} a.color = c;", xo::COLOR_SYM);
    } else {
        let _ = writeln!(out, "  a.color = src.color * prm[B + {}u] + prm[B + {}u];", xo::C1, xo::C2);
    }
    emit_affine(out, xf.affine_mode, "B", xo::COEFS, xo::YZ, xo::ZX, "a", "src");
    out.push_str("  var v = Pt(0.0, 0.0, 0.0, a.color, a.r, a.g, a.b);\n");
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
    out.push_str("  v.color = a.color; v.r = a.r; v.g = a.g; v.b = a.b;\n");
    emit_steps(out, &xf.normal);
    emit_steps(out, &xf.post_vars);
    out.push_str("  var d = v;\n");
    emit_affine(out, xf.post_mode, "B", xo::POST, xo::YZ_POST, xo::ZX_POST, "d", "v");
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
    let _ = layer;
    out.push_str("  return d;\n}\n");
}

impl Program {
    pub fn new(flame: &Flame, prepared: &PreparedFlame) -> Self {
        let mut sig = String::new();
        let mut unsupported: Vec<String> = prepared.missing_variations.clone();
        let mut used_vars: BTreeMap<&'static str, &'static VarDef> = BTreeMap::new();
        let mut layers = Vec::new();
        let mut param_len = 0usize;
        let mut next_len = 0usize;

        let _ = write!(sig, "pz{}|sym{:?}|3d{}|", prepared.preserve_z, flame.post_symmetry.kind, flame.is_3d_projection_required());
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
        w.push_str(PRELUDE);
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
        w.push_str(PROJECT_FN);
        match flame.post_symmetry.kind {
            PostSymmetryType::None => w.push_str("fn plot_sym(q: Pt, col: vec3<f32>) { plot(q.x, q.y, q.z, col); }\n"),
            PostSymmetryType::Point => w.push_str(PLOT_POINT_SYM),
            PostSymmetryType::XAxis => w.push_str(&PLOT_AXIS_SYM.replace("AXIS_X", "true")),
            PostSymmetryType::YAxis => w.push_str(&PLOT_AXIS_SYM.replace("AXIS_X", "false")),
        }

        // Main kernel
        w.push_str(ITER_MAIN_HEAD);
        for (li, layer) in prepared.layers.iter().enumerate() {
            let lay = &layers[li];
            let _ = writeln!(
                w,
                "    case {li}u: {{\n      let density = prm[{h}u + {d}u];\n      let weight = prm[{h}u + {wt}u];\n      for (var i = 0u; i < U.iters; i++) {{\n        if (density < 1.0 && rnd() > density * density) {{ continue; }}\n        if (!has_next_{li}(xf)) {{ break; }}\n        xf = nxt[{nb}u + xf * {ts}u + (rnd_u32() & {mask}u)];\n        p = apply_xf_{li}(xf, p);\n        if (bad(p)) {{ p = fresh_point(); skip = FUSE_ITERS; continue; }}\n        if (skip > 0u) {{ skip -= 1u; continue; }}\n        if (!visible_{li}(xf)) {{ continue; }}\n        let q = finals_{li}(p);\n        plot_sym(q, vec3<f32>(q.r, q.g, q.b) * weight);\n      }}\n    }}",
                h = lay.header,
                d = lo::DENSITY,
                wt = lo::WEIGHT,
                nb = lay.next_base,
                ts = NEXT_XFORM_TABLE_SIZE,
                mask = NEXT_XFORM_TABLE_SIZE - 1,
            );
        }
        w.push_str(ITER_MAIN_TAIL);

        Self { signature: sig, wgsl: w, layer_count: layers.len(), layers, param_len, next_len, unsupported }
    }

    /// Packs all numeric values into the `prm` buffer layout of this program.
    pub fn pack_params(&self, prepared: &PreparedFlame) -> Vec<f32> {
        let mut p = vec![0.0f32; self.param_len.max(1)];
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
    let mut off = base + xo::VARS;
    for s in steps(xf) {
        p[off] = s.amount as f32;
        for (i, v) in s.data.iter().enumerate() {
            p[off + 1 + i] = *v as f32;
        }
        off += 1 + s.data.len();
    }
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

struct State { x: f32, y: f32, z: f32, color: f32, r: f32, g: f32, b: f32, xf: u32, rng: u32, skip: u32, _p0: u32, _p1: u32 }

@group(0) @binding(0) var<uniform> U: IterUniforms;
@group(0) @binding(1) var<storage, read> prm: array<f32>;
@group(0) @binding(2) var<storage, read> nxt: array<u32>;
@group(0) @binding(3) var<storage, read> cmap: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> states: array<State>;
@group(0) @binding(5) var<storage, read_write> hist: array<atomic<i32>>;

const FUSE_ITERS: u32 = 30u;

fn bad(p: Pt) -> bool {
    let s = p.x + p.y + p.z;
    return !(s == s) || abs(p.x) > 1.0e18 || abs(p.y) > 1.0e18 || abs(p.z) > 1.0e18;
}

fn fresh_point() -> Pt {
    return Pt(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0, 0.0, rnd(), 0.0, 0.0, 0.0);
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
    let col = col_in * intensity;
    let idx = (u32(iy) * U.raster_w + u32(ix)) * 4u;
    atomicAdd(&hist[idx], 1);
    atomicAdd(&hist[idx + 1u], i32(col.x * U.color_scale));
    atomicAdd(&hist[idx + 2u], i32(col.y * U.color_scale));
    atomicAdd(&hist[idx + 3u], i32(col.z * U.color_scale));
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
    var p = Pt(s.x, s.y, s.z, s.color, s.r, s.g, s.b);
    var xf = s.xf;
    var skip = s.skip;
    let layer = tid % max(U.layer_count, 1u);
    switch layer {
"#;

const ITER_MAIN_TAIL: &str = r#"
    default: {}
    }
    s.x = p.x; s.y = p.y; s.z = p.z; s.color = p.color; s.r = p.r; s.g = p.g; s.b = p.b;
    s.xf = xf; s.skip = skip; s.rng = rng_state;
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

/// A tiny shader exercising one variation, for validating snippets in isolation.
pub fn variation_test_shader(def: &VarDef) -> String {
    let mut w = String::new();
    w.push_str(PRELUDE);
    w.push_str("@group(0) @binding(0) var<storage, read_write> prm: array<f32>;\n");
    w.push_str(&variation_function(def, true));
    let _ = write!(
        w,
        "@compute @workgroup_size(1)\nfn main() {{\n  rng_state = 1u;\n  let a = Pt(prm[0], prm[1], prm[2], 0.5, 0.0, 0.0, 0.0);\n  let r = {}(a, Pt(0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0), 4u, prm[3]);\n  prm[0] = r.v.x + r.v.y + r.v.z + r.a.x;\n}}\n",
        var_fn_name(def)
    );
    w
}
