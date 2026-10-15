// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// Ported from JWildfire - an image and animation processor written in Java
// Copyright (C) 1995-2026 Andreas Maschke
//
// This program is free software: you can redistribute it and/or modify it under the terms of
// the GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: LGPL-3.0-or-later

//! WGSL generation for the iteration kernel.
//!
//! The shader *structure* (xforms, variation types, modes) is compiled into code; all numeric
//! values live in the `prm` storage buffer. Changing a value therefore only needs a buffer upload,
//! while structural changes produce a different [`Program::signature`] and a recompile.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use rwf_core::model::{ColorType, DrawMode, Flame, PostSymmetryType};
use rwf_core::render::prepared::{AffineMode, NEXT_XFORM_TABLE_SIZE, PreparedFlame, PreparedXForm, StepMode, VarStep};
use rwf_core::variations::wgsl::{prelude, var_fn_name, variation_function, variation_function_weighted};
use rwf_core::variations::{NOISE_TABLES_LEN, VarDef, noise_tables, subflame};
use rwf_core::weighting::{Field, FieldInput};

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
/// Length of a gradient map block's header (see [`LayerLayout::gmap`]).
const GMAP_HEADER: usize = 8;

/// `TransformationGradientMapColorStep` reading the block at `g`.
const GMAP_WGSL: &str = r#"
fn gmap_px(g: u32, iw: i32, ih: i32, x: i32, y: i32) -> vec3<f32> {
    if (x < 0 || y < 0 || x >= iw || y >= ih) { return vec3<f32>(0.0); }
    let c = u32(prm[g + 8u + u32(y * iw + x)]);
    return vec3<f32>(f32((c >> 16u) & 255u), f32((c >> 8u) & 255u), f32(c & 255u));
}

fn gmap_color(g: u32, px: f32, py: f32, color: f32) -> vec3<f32> {
    let lca = prm[g + 4u];
    let lcm = prm[g + 5u];
    let x = (px * (1.0 - lcm) + px * lcm * color + lca * color) * prm[g] + prm[g + 1u];
    let y = (py * (1.0 - lcm) + py * lcm * color + lca * color) * prm[g + 2u] + prm[g + 3u];
    let iw = i32(prm[g + 6u]);
    let ih = i32(prm[g + 7u]);
    let w = f32(max(iw - 2, 1));
    let h = f32(max(ih - 2, 1));
    let fx = abs(x);
    var ix = fx * w - w * trunc(fx * w / w);
    if (i32(fx) % 2 != 0) { ix = w - ix; }
    let fy = abs(y);
    var iy = fy * h - h * trunc(fy * h / h);
    if (i32(fy) % 2 != 0) { iy = h - iy; }
    let x0 = i32(ix);
    let y0 = i32(iy);
    let tx = ix - trunc(ix);
    let ty = iy - trunc(iy);
    let top = mix(gmap_px(g, iw, ih, x0, y0), gmap_px(g, iw, ih, x0 + 1, y0), tx);
    let bottom = mix(gmap_px(g, iw, ih, x0, y0 + 1), gmap_px(g, iw, ih, x0 + 1, y0 + 1), tx);
    return mix(top, bottom, ty);
}
"#;

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
    /// Gradient map block in `prm`: [h_scale, h_offset, v_scale, v_offset, local color add,
    /// local color scale, width, height, packed pixels...].
    pub gmap: Option<usize>,
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
    /// Start of the weighting field tables in `prm` (if any xform has a weighting field).
    pub wf_tables: Option<usize>,
    /// Classic rendering records depths into a z-buffer (binding 6, see [`ZBUFFER_WGSL`]).
    pub zbuffer: bool,
    /// Sub-flame variations read the sub-flame points (binding 9, see `subflame.wgsl`).
    pub uses_subflames: bool,
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

/// WGSL for `dof_distance` / `dof_apply` (mirrors `rwf_core::render::dof`).
fn dof_wgsl(dof: &rwf_core::render::dof::Dof, base: usize) -> String {
    use rwf_core::render::dof::ShapeMode;
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
                        "  let dr = {abs_dr}({k} * zdist * {sc} * dof_fade());\n  let src = Pt(x, y, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);\n  let r = {f}(src, src, {vb}, dr);\n  var d = vec2<f32>(r.v.x, r.v.y);\n"
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
                        "  let fade = dof_fade();\n  let drf = {};\n  let dr = abs({k} * zdist * {sc} * drf) * {dr_factor:.6};\n  let fs = {};\n  let s = Pt((1.0 - 2.0 * rnd()) * fs, (1.0 - 2.0 * rnd()) * fs, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);\n  let r = {f}(s, Pt(x, y, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), {vb}, dr);\n  return dof_rotate(vec2<f32>(x, y), vec2<f32>(r.v.x, r.v.y));\n",
                        if refade { "dof_fade()" } else { "fade" },
                        if scale_by_fade { "fade" } else { "1.0" },
                    )
                }
                ShapeMode::Taurus => format!(
                    "  let fade = dof_fade();\n  let dr = abs({k} * zdist * {sc} * fade) * 0.25;\n  let s = Pt((TWO_PI - 4.0 * rnd() * PI) * fade, (TWO_PI - 4.0 * rnd() * PI) * fade, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);\n  let r = {f}(s, Pt(x, y, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), {vb}, dr);\n  return dof_rotate(vec2<f32>(x, y), vec2<f32>(r.v.x, r.v.y));\n"
                ),
            }
        }
        // Bubble (also the fallback for shapes without GPU support)
        _ => {
            format!("  let dr = dof_fade() * {k} * zdist * {sc};\n  let a = TWO_PI * rnd();\n  return vec2<f32>(x + dr * cos(a), y + dr * sin(a));\n")
        }
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
    wf_offset(xf) + wf_len(xf)
}

/// Start of the weighting field block inside an xform block (after the variation data).
fn wf_offset(xf: &PreparedXForm) -> usize {
    xo::VARS + xf.pre.iter().chain(&xf.normal).chain(&xf.post_vars).map(|s| 1 + s.data.len()).sum::<usize>()
}

/// Offsets inside a weighting field block (see `weighting.wgsl`), followed by one amount
/// intensity per variation step and the image pixels (image maps).
mod wfo {
    pub const TYPE: usize = 0;
    pub const SEED: usize = 1;
    pub const FREQUENCY: usize = 2;
    pub const OCTAVES: usize = 3;
    pub const LACUNARITY: usize = 4;
    pub const GAIN: usize = 5;
    pub const BOUNDING: usize = 6;
    pub const FRACTAL: usize = 7;
    pub const CELL_DIST: usize = 8;
    pub const CELL_RET: usize = 9;
    pub const IMG: usize = 10;
    pub const PIXELS: usize = 16;
    pub const VAR_AMOUNT: usize = 17;
    pub const COLOR_SCALE: usize = 18;
    pub const JITTER: usize = 19;
    /// Per variation step: amount intensity, then the intensities of up to 3 parameters.
    pub const STEPS: usize = 20;
    pub const STEP_LEN: usize = 4;
}

/// Field type code of the GPU (see `weighting.wgsl`): 0 none, 1 cellular, 2 cubic, 3 cubic
/// fractal, 4 perlin, 5 perlin fractal, 6 simplex, 7 simplex fractal, 8 value, 9 value fractal,
/// 10 white noise, 11 image map.
fn field_code(field: &Field) -> u32 {
    use rwf_core::fastnoise::NoiseType;
    match field {
        Field::Empty => 0,
        Field::Noise(n) => match n.noise_type {
            NoiseType::Cellular => 1,
            NoiseType::Cubic => 2,
            NoiseType::CubicFractal => 3,
            NoiseType::Perlin => 4,
            NoiseType::PerlinFractal => 5,
            NoiseType::Simplex => 6,
            NoiseType::SimplexFractal => 7,
            NoiseType::Value => 8,
            NoiseType::ValueFractal => 9,
            NoiseType::WhiteNoise => 10,
        },
        Field::Image { .. } => 11,
    }
}

/// `wf_value_<code>(b, x, y, z)`: the value of a field of one type. Specialized per type, so a
/// call site inlines one noise function and not all of them.
fn wf_value_fn(code: u32) -> String {
    let noise = match code {
        1 => "return wf_cellular(b, x, y, z);",
        2 => "return wf_cubic3(seed, x, y, z);",
        3 => "return wf_fractal(b, 3, x, y, z);",
        4 => "return wf_perlin3(seed, x, y, z);",
        5 => "return wf_fractal(b, 1, x, y, z);",
        6 => "return wf_simplex3(seed, x, y, z);",
        7 => "return wf_fractal(b, 2, x, y, z);",
        8 => "return wf_value3(seed, x, y, z);",
        9 => "return wf_fractal(b, 0, x, y, z);",
        10 => {
            "let xb = bitcast<i32>(x);\n  let yb = bitcast<i32>(y);\n  let zb = bitcast<i32>(z);\n  return wf_val_coord3(seed, xb ^ (xb >> 16u), yb ^ (yb >> 16u), zb ^ (zb >> 16u));"
        }
        11 => return format!("fn wf_value_{code}(b: u32, x_in: f32, y_in: f32, z_in: f32) -> f32 {{\n  return wf_image(b, x_in, y_in);\n}}\n"),
        _ => return format!("fn wf_value_{code}(b: u32, x_in: f32, y_in: f32, z_in: f32) -> f32 {{\n  return 0.0;\n}}\n"),
    };
    format!(
        "fn wf_value_{code}(b: u32, x_in: f32, y_in: f32, z_in: f32) -> f32 {{\n  let f = prm[b + 2u];\n  let x = x_in * f;\n  let y = y_in * f;\n  let z = z_in * f;\n  let seed = i32(prm[b + 1u]);\n  {noise}\n}}\n"
    )
}

fn wf_len(xf: &PreparedXForm) -> usize {
    match &xf.weighting {
        Some(w) => {
            let pixels = match &w.field {
                Field::Image { img, .. } => (img.width * img.height) as usize,
                _ => 0,
            };
            wfo::STEPS + wfo::STEP_LEN * steps(xf).count() + pixels + step_tables(xf).iter().flatten().map(StepTable::len).sum::<usize>()
        }
        None => 0,
    }
}

/// Weighting field modulated parameters of a variation with a `prepare` step, whose data
/// cannot be derived on the GPU: the prepared data at `levels` field values from `lo` to `hi`.
/// The shader picks one per sample, randomly between the two nearest levels.
struct StepTable {
    levels: usize,
    stride: usize,
    lo: f64,
    hi: f64,
    data: Vec<f32>,
}

impl StepTable {
    /// Header (`lo`, levels per field unit) and the data blocks.
    fn len(&self) -> usize {
        2 + self.levels * self.stride
    }
}

const STEP_TABLE_LEVELS: usize = 64;
const STEP_TABLE_MIN_LEVELS: usize = 8;
/// Maximum size of one table in floats.
const STEP_TABLE_BUDGET: usize = 1 << 14;

fn step_table(xf: &PreparedXForm, s: &VarStep) -> Option<StepTable> {
    let w = xf.weighting.as_ref()?;
    let sw = s.weighting.as_ref()?;
    if s.def.prepare.is_none() || !s.def.supports_gpu() || matches!(s.mode, StepMode::Enforced | StepMode::EnforcedPost) || sw.params.iter().all(|q| q.param.is_none())
    {
        return None;
    }
    let (lo, hi) = w.field.value_range();
    if hi - lo < 1e-9 {
        return None;
    }
    let stride = [lo, hi].iter().map(|v| sw.modulated_data(s.def, *v).len()).max()?.max(s.data.len()).max(1);
    let levels = STEP_TABLE_LEVELS.min(STEP_TABLE_BUDGET / stride);
    if levels < STEP_TABLE_MIN_LEVELS {
        return None;
    }
    let mut data = vec![0.0f32; levels * stride];
    for l in 0..levels {
        let wmv = lo + (hi - lo) * l as f64 / (levels - 1) as f64;
        for (i, v) in sw.modulated_data(s.def, wmv).iter().take(stride).enumerate() {
            data[l * stride + i] = *v as f32;
        }
    }
    Some(StepTable { levels, stride, lo, hi, data })
}

/// The [`step_table`] of every step of an xform (in [`steps`] order).
fn step_tables(xf: &PreparedXForm) -> Vec<Option<StepTable>> {
    if xf.weighting.is_none() {
        return Vec::new();
    }
    steps(xf).map(|s| step_table(xf, s)).collect()
}

/// Start of the step tables relative to the weighting field block.
fn step_tables_offset(xf: &PreparedXForm) -> usize {
    let pixels = match xf.weighting.as_ref().map(|w| &w.field) {
        Some(Field::Image { img, .. }) => (img.width * img.height) as usize,
        _ => 0,
    };
    wfo::STEPS + wfo::STEP_LEN * steps(xf).count() + pixels
}

/// Parameters of a step a weighting field scales on the GPU (`(index, integer)`): only for
/// variations without a `prepare` step, whose prepared data are the parameters.
fn gpu_weighted_params(s: &VarStep) -> Vec<(usize, bool)> {
    match &s.weighting {
        Some(w) if s.def.prepare.is_none() && !matches!(s.mode, StepMode::Enforced | StepMode::EnforcedPost) => {
            w.params.iter().filter_map(|q| q.param.map(|k| (k, q.integer))).collect()
        }
        _ => Vec::new(),
    }
}

fn weighted_fn_name(def: &VarDef, params: &[(usize, bool)]) -> String {
    let mut n = format!("{}_wf", var_fn_name(def));
    for (k, int) in params {
        let _ = write!(n, "_{k}{}", if *int { "i" } else { "" });
    }
    n
}

/// Weighting field function tables (`GRAD_3D`, `CELL_3D`) in `prm`.
const WF_TABLES_LEN: usize = 16 * 3 + 256 * 3;

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

#[allow(clippy::too_many_arguments)]
fn emit_xform(out: &mut String, fname: &str, xf: &PreparedXForm, base: usize, layer: usize, layout_cmap: usize, smooth: bool, gmap: Option<usize>) {
    let b = format!("{base}u");
    let _ = writeln!(out, "fn {fname}(src: Pt) -> Pt {{");
    let _ = writeln!(out, "  let B = {b};");
    out.push_str("  var a = Pt(0.0, 0.0, 0.0, 0.0, src.r, src.g, src.b, 0.0, src.hide, 0.0);\n");
    if xf.color_type == ColorType::Cyclic {
        let _ = writeln!(out, "  var c = fmod(src.color + prm[B + {}u], 1.0); if (c < 0.0) {{ c += 1.0; }} a.color = c;", xo::COLOR_SYM);
    } else {
        let _ = writeln!(out, "  a.color = src.color * prm[B + {}u] + prm[B + {}u];", xo::C1, xo::C2);
    }
    emit_affine(out, xf.affine_mode, "B", xo::COEFS, xo::YZ, xo::ZX, "a", "src");
    let wf = wf_offset(xf);
    let wf_value = format!("wf_value_{}", xf.weighting.as_ref().map_or(0, |w| field_code(&w.field)));
    if let Some(w) = &xf.weighting {
        let p = if w.input == FieldInput::Position { "src" } else { "a" };
        let _ = writeln!(out, "  let wmv = {wf_value}(B + {wf}u, {p}.x, {p}.y, {p}.z);");
        let _ = writeln!(out, "  let vai = prm[B + {}u];", wf + wfo::VAR_AMOUNT);
        out.push_str("  let wamt = select(1.0, 1.0 + wmv * vai, abs(wmv) > EPSILON && abs(vai) > EPSILON);\n");
    }
    out.push_str("  var v = Pt(0.0, 0.0, 0.0, a.color, a.r, a.g, a.b, a.rgbc, a.hide, a.rso);\n");
    let mut off = xo::VARS;
    let mut si = 0usize;
    let weighted = xf.weighting.is_some();
    let tables = step_tables(xf);
    let mut table_off = wf + step_tables_offset(xf);
    let mut emit_steps = |out: &mut String, list: &[VarStep]| {
        for s in list {
            let f = var_fn_name(s.def);
            let call_base = off + 1;
            // Weighting field: amount scaled by the field value (not for the enforced steps,
            // which see no field value in JWildfire). Modulated parameters are scaled in the
            // shader, or come from a step table for variations with a `prepare` step.
            let so = wf + wfo::STEPS + wfo::STEP_LEN * si;
            let amt = if weighted && !matches!(s.mode, StepMode::Enforced | StepMode::EnforcedPost) {
                format!("(prm[B + {off}u] * wamt * (1.0 + wmv * prm[B + {so}u]))")
            } else {
                format!("prm[B + {off}u]")
            };
            let wp = gpu_weighted_params(s);
            let (f, amt) = if wp.is_empty() || !s.def.supports_gpu() {
                (f, amt)
            } else {
                let extra: String = (0..wp.len()).map(|n| format!(", 1.0 + wmv * prm[B + {}u]", so + 1 + n)).collect();
                (weighted_fn_name(s.def, &wp), format!("{amt}{extra}"))
            };
            // Prepared data at the field value from a step table.
            let call_base = match tables.get(si).and_then(Option::as_ref) {
                Some(t) => {
                    let base = format!("B + {}u + {}u * wf_level(wmv, B + {table_off}u, {}u)", table_off + 2, t.stride, t.levels);
                    table_off += t.len();
                    base
                }
                None => format!("B + {call_base}u"),
            };
            si += 1;
            match s.mode {
                StepMode::Pre | StepMode::Normal | StepMode::Post => {
                    let _ = writeln!(out, "  {{ let r = {f}(a, v, {call_base}, {amt}); a = r.a; v = r.v; }}");
                }
                StepMode::EnforcedPre => {
                    let _ = writeln!(out, "  {{ let r = {f}(a, a, {call_base}, {amt}); a = r.v; }}");
                }
                StepMode::Enforced | StepMode::EnforcedPost => {
                    let _ = writeln!(out, "  {{ let r = {f}(v, v, {call_base}, {amt}); v = r.v; }}");
                }
            }
            off += 1 + s.data.len();
        }
    };
    emit_steps(out, &xf.pre);
    out.push_str("  v.color = a.color; v.r = a.r; v.g = a.g; v.b = a.b; v.rgbc = a.rgbc; v.hide = a.hide; v.rso = a.rso;\n");
    emit_steps(out, &xf.normal);
    emit_steps(out, &xf.post_vars);
    out.push_str("  var d = v;\n");
    emit_affine(out, xf.post_mode, "B", xo::POST, xo::YZ_POST, xo::ZX_POST, "d", "v");
    if xf.weighting.is_some() {
        let _ = writeln!(
            out,
            "  {{ let j = prm[B + {j}u]; if (abs(j) > EPSILON) {{ d.x += {wf_value}(B + {wf}u, d.x, d.y, d.z) * j; d.y += {wf_value}(B + {wf}u, d.y, d.x, d.z) * j; d.z += {wf_value}(B + {wf}u, d.z, d.x, d.y) * j; }}\n    let cs = prm[B + {c}u]; if (abs(cs) > EPSILON) {{ d.color *= 1.0 + wmv * cs; }} }}",
            j = wf + wfo::JITTER,
            c = wf + wfo::COLOR_SCALE
        );
    }
    out.push_str("  if (d.rgbc == 0.0) {\n");
    match xf.color_type {
        ColorType::Diffusion | ColorType::Cyclic if gmap.is_some() => {
            let _ = writeln!(out, "  {{ let c = gmap_color({}u, d.x, d.y, d.color); d.r = c.x; d.g = c.y; d.b = c.z; }}", gmap.unwrap());
        }
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

/// Variation functions with weighting field modulated parameters, by function name.
type WeightedFns = BTreeMap<String, (&'static VarDef, Vec<(usize, bool)>)>;

/// Parameter layout and the variation functions of a prepared flame (shared by the iterate
/// and the sub-flame kernels).
struct Layout {
    sig: String,
    unsupported: Vec<String>,
    used_vars: BTreeMap<&'static str, &'static VarDef>,
    weighted_fns: WeightedFns,
    layers: Vec<LayerLayout>,
    param_len: usize,
    next_len: usize,
    wf_tables: Option<usize>,
    /// Weighting field type codes in use (see [`field_code`]).
    wf_types: std::collections::BTreeSet<u32>,
}

impl Layout {
    fn new(prepared: &PreparedFlame, mut sig: String) -> Self {
        let mut unsupported: Vec<String> = prepared.missing_variations.clone();
        let mut used_vars: BTreeMap<&'static str, &'static VarDef> = BTreeMap::new();
        let mut layers = Vec::new();
        // The noise tables come first (see `wgsl/noise.wgsl`).
        let mut param_len = NOISE_TABLES_LEN;
        let mut next_len = 0usize;
        let mut any_weighting = false;
        let mut wf_types = std::collections::BTreeSet::new();
        let mut weighted_fns: WeightedFns = BTreeMap::new();
        for (li, layer) in prepared.layers.iter().enumerate() {
            let header = param_len;
            param_len += lo::SIZE;
            let gmap = layer.gradient_map.as_ref().map(|g| {
                let _ = write!(sig, "gm{}x{}:", g.image.width, g.image.height);
                param_len += GMAP_HEADER + (g.image.width * g.image.height) as usize;
                param_len - GMAP_HEADER - (g.image.width * g.image.height) as usize
            });
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
                if let Some(w) = &xf.weighting {
                    let code = field_code(&w.field);
                    let _ = write!(sig, "w{}{}t{code},", wf_len(xf), w.input == FieldInput::Position);
                    wf_types.insert(code);
                    any_weighting = true;
                }
                for t in step_tables(xf).iter().flatten() {
                    let _ = write!(sig, "T{}x{},", t.levels, t.stride);
                }
                for s in steps(xf) {
                    let wp = gpu_weighted_params(s);
                    if !wp.is_empty() && s.def.supports_gpu() {
                        let name = weighted_fn_name(s.def, &wp);
                        let _ = write!(sig, "{name},");
                        weighted_fns.insert(name, (s.def, wp));
                    }
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
            layers.push(LayerLayout { header, xforms, finals, next_base, cmap_base: li * 256, gmap });
            sig.push('|');
        }
        let wf_tables = any_weighting.then(|| {
            param_len += WF_TABLES_LEN;
            param_len - WF_TABLES_LEN
        });
        if wf_tables.is_some() {
            sig.push_str("wf|");
        }
        Self { sig, unsupported, used_vars, weighted_fns, layers, param_len, next_len, wf_tables, wf_types }
    }
}

/// Variation, xform, dispatcher, visibility and finals functions of every layer.
fn emit_functions(w: &mut String, prepared: &PreparedFlame, layout: &Layout, solid: bool) {
    let Layout { layers, used_vars, weighted_fns, wf_tables, wf_types, .. } = layout;
    if let Some(t) = wf_tables {
        let _ = writeln!(w, "const WF_TABLES: u32 = {t}u;");
        w.push_str(include_str!("weighting.wgsl"));
        for code in wf_types {
            w.push_str(&wf_value_fn(*code));
        }
    }
    for def in used_vars.values() {
        w.push_str(&variation_function(def, prepared.preserve_z));
    }
    for (name, (def, wp)) in weighted_fns {
        w.push_str(&variation_function_weighted(def, prepared.preserve_z, name, wp));
    }
    if layers.iter().any(|l| l.gmap.is_some()) {
        w.push_str(GMAP_WGSL);
    }
    // Variations without GPU code compile to a no-op.
    w.push_str("fn var_unsupported(a_in: Pt, v_in: Pt, base: u32, AMOUNT: f32) -> PtPair { return PtPair(a_in, v_in); }\n");

    for (li, layer) in prepared.layers.iter().enumerate() {
        let lay = &layers[li];
        for (k, xf) in layer.xforms.iter().enumerate() {
            let mut xf2 = xf.clone();
            strip_unsupported(&mut xf2);
            emit_xform(w, &format!("xf_{li}_{k}"), &xf2, lay.xforms[k], li, lay.cmap_base, layer.smooth_gradient, lay.gmap);
        }
        for (k, xf) in layer.finals.iter().enumerate() {
            let mut xf2 = xf.clone();
            strip_unsupported(&mut xf2);
            emit_xform(w, &format!("fxf_{li}_{k}"), &xf2, lay.finals[k], li, lay.cmap_base, layer.smooth_gradient, lay.gmap);
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
                let _ = writeln!(
                    w,
                    "    case {k}u: {{ return m * prm[{b}u + {m1}u] + prm[{b}u + {m2}u]; }}",
                    b = lay.xforms[k],
                    m1 = xo::MAT1,
                    m2 = xo::MAT2
                );
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
}

impl Program {
    pub fn new(flame: &Flame, prepared: &PreparedFlame) -> Self {
        Self::new_scaled(flame, prepared, 1.0, false)
    }

    /// `ppu_scale` = output size relative to the flame size (scales the shadow map size).
    /// `zbuffer`: record depths (solid rendering always has a z-buffer).
    pub fn new_scaled(flame: &Flame, prepared: &PreparedFlame, ppu_scale: f64, zbuffer: bool) -> Self {
        let mut sig = String::new();
        let _ = write!(sig, "pz{}|sym{:?}|3d{}|", prepared.preserve_z, flame.post_symmetry.kind, flame.is_3d_projection_required());
        let solid = flame.solid.enabled;
        let mut shadow_maps = Vec::new();
        let mut map_count = 0u32;
        if solid {
            let lights = rwf_core::render::solid::LightViews::new(&flame.solid);
            for m in &lights.proj {
                shadow_maps.push(m.map(|_| {
                    map_count += 1;
                    map_count - 1
                }));
            }
        }
        let shadow_map_size = if map_count > 0 { rwf_core::render::solid::shadow_map_size(flame, ppu_scale) } else { 0 };
        if solid {
            let _ = write!(sig, "solid{map_count}x{shadow_map_size}|");
        }
        let zbuffer = zbuffer && !solid;
        if zbuffer {
            sig.push_str("zb|");
        }
        if flame.post_blur_radius > 0 {
            let _ = write!(sig, "pb{}/{}/{}|", flame.post_blur_radius, flame.post_blur_fade, flame.post_blur_falloff);
        }
        let layout = Layout::new(prepared, sig);

        // ---- WGSL ----
        let mut w = String::with_capacity(32 * 1024);
        w.push_str(&prelude());
        w.push_str(ITER_BINDINGS);
        w.push_str(ITER_COMMON);
        emit_functions(&mut w, prepared, &layout, solid);
        let Layout { mut sig, unsupported, used_vars, layers, mut param_len, next_len, wf_tables, .. } = layout;

        // Plot with post symmetry
        // Depth of field
        let dof = rwf_core::render::dof::Dof::new(flame);
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
        } else {
            let mut project = PROJECT_FN.to_string();
            if dof.is_some() {
                project = project.replace(
                    "        x = cx / zr;\n        y = cy / zr;",
                    "        let dd = dof_distance(vec3<f32>(cx, cy, cz));\n        if (dd >= 0.0) { let bp = dof_apply(cx, cy, dd); x = bp.x / zr; y = bp.y / zr; } else { x = cx / zr; y = cy / zr; }",
                );
            }
            if zbuffer {
                w.push_str(ZBUFFER_WGSL);
                project = project
                    .replace("    var intensity = 1.0;\n", "    var intensity = 1.0;\n    var cam_z = 0.0;\n")
                    .replace("        if (zr < EPSILON) { return; }\n", "        if (zr < EPSILON) { return; }\n        cam_z = cz;\n")
                    .replace(
                        "    splat(ix, iy, dim_col(col_in, intensity));",
                        "    zbuf_add(ix, iy, cam_z * U.bws, dim_col(col_in, intensity));\n    splat(ix, iy, dim_col(col_in, intensity));",
                    );
            }
            w.push_str(&project);
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
                "    case {li}u: {{\n      cmap_base = {cb}u;\n      let density = prm[{h}u + {d}u];\n      let weight = prm[{h}u + {wt}u];\n      for (var i = 0u; i < U.iters; i++) {{\n        if (density < 1.0 && rnd() > density * density) {{ continue; }}\n        if (!has_next_{li}(xf)) {{ break; }}\n        xf = nxt[{nb}u + xf * {ts}u + (rnd_u32() & {mask}u)];\n        p = apply_xf_{li}(xf, p);{mat_step}\n        if (bad(p)) {{ p = fresh_point(); skip = FUSE_ITERS;{mat_reset} continue; }}\n        if (skip > 0u) {{ skip -= 1u; continue; }}\n        if (!visible_{li}(xf)) {{ continue; }}\n        let q = finals_{li}(p);\n        if (q.hide != 0.0) {{ continue; }}{mat_plot}\n        cur_w = weight;\n        plot_sym(q, vec3<f32>(q.r, q.g, q.b) * weight);\n      }}\n    }}",
                h = lay.header,
                d = lo::DENSITY,
                wt = lo::WEIGHT,
                nb = lay.next_base,
                ts = NEXT_XFORM_TABLE_SIZE,
                mask = NEXT_XFORM_TABLE_SIZE - 1,
                cb = lay.cmap_base,
                mat_step = if solid { format!(" mat = mat_xf_{li}(xf, mat);") } else { String::new() },
                mat_reset = if solid { " mat = rnd();" } else { "" },
                mat_plot = if solid { " cur_rso = p.rso; cur_mat = select(mat, -1.0, p.rso != 0.0);" } else { "" },
            );
        }
        w.push_str(ITER_MAIN_TAIL);

        let uses_subflames = used_vars.keys().any(|n| subflame::uses_subflames(n));
        Self {
            signature: sig,
            wgsl: w,
            layer_count: layers.len(),
            layers,
            param_len,
            next_len,
            unsupported,
            dof_base,
            solid_base,
            shadow_maps,
            shadow_map_size,
            wf_tables,
            zbuffer,
            uses_subflames,
        }
    }

    /// The sub-flame kernel `sub_iterate` of a sub-flame (layer 0; sub-flame variations inside
    /// it are ignored).
    pub fn new_sub(sub: &PreparedFlame) -> Self {
        let mut prepared = sub.clone();
        prepared.layers.truncate(1);
        for layer in &mut prepared.layers {
            for xf in layer.xforms.iter_mut().chain(layer.finals.iter_mut()) {
                for s in xf.pre.iter_mut().chain(xf.normal.iter_mut()).chain(xf.post_vars.iter_mut()) {
                    if subflame::uses_subflames(s.def.name) {
                        s.def = &UNSUPPORTED_DEF;
                    }
                }
            }
        }
        let layout = Layout::new(&prepared, format!("sub|pz{}|", prepared.preserve_z));
        let mut w = String::with_capacity(32 * 1024);
        w.push_str(&prelude());
        w.push_str(ITER_COMMON);
        emit_functions(&mut w, &prepared, &layout, false);
        let Layout { sig, unsupported, layers, param_len, next_len, wf_tables, .. } = layout;
        if let Some(lay) = layers.first() {
            w.push_str(
                &SUB_KERNEL
                    .replace("NBu", &format!("{}u", lay.next_base))
                    .replace("TSu", &format!("{}u", NEXT_XFORM_TABLE_SIZE))
                    .replace("MASKu", &format!("{}u", NEXT_XFORM_TABLE_SIZE - 1)),
            );
        }
        Self {
            signature: sig,
            wgsl: w,
            layer_count: layers.len(),
            layers,
            param_len,
            next_len,
            unsupported,
            dof_base: None,
            solid_base: None,
            shadow_maps: Vec::new(),
            shadow_map_size: 0,
            wf_tables,
            zbuffer: false,
            uses_subflames: false,
        }
    }

    /// Packs all numeric values into the `prm` buffer layout of this program.
    pub fn pack_params(&self, flame: &Flame, prepared: &PreparedFlame) -> Vec<f32> {
        let mut p = self.pack_layers(prepared);
        if let (Some(base), Some(d)) = (self.dof_base, rwf_core::render::dof::Dof::new(flame)) {
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
        p
    }

    /// Packs the layers and weighting field tables (everything but the flame-wide blocks).
    pub fn pack_layers(&self, prepared: &PreparedFlame) -> Vec<f32> {
        let mut p = vec![0.0f32; self.param_len.max(1)];
        p[..NOISE_TABLES_LEN].copy_from_slice(&noise_tables());
        if let Some(t) = self.wf_tables {
            p[t..t + WF_TABLES_LEN].copy_from_slice(&rwf_core::fastnoise::tables());
        }
        for (li, layer) in prepared.layers.iter().take(self.layers.len()).enumerate() {
            let lay = &self.layers[li];
            p[lay.header + lo::WEIGHT] = layer.weight as f32;
            p[lay.header + lo::DENSITY] = layer.density as f32;
            if let (Some(g), Some(gm)) = (lay.gmap, &layer.gradient_map) {
                let m = &gm.map;
                let head = [m.h_scale, m.h_offset, m.v_scale, m.v_offset, m.local_color_add, m.local_color_scale, gm.image.width as f64, gm.image.height as f64];
                for (i, v) in head.iter().enumerate() {
                    p[g + i] = *v as f32;
                }
                // packed 0xRRGGBB: exact in f32
                for (i, px) in gm.image.pixels.iter().enumerate() {
                    p[g + GMAP_HEADER + i] = *px as f32;
                }
            }
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
    pub fn pack_solid(&self, p: &mut [f32], flame: &Flame, frames: &[Option<rwf_core::render::solid::ShadowFrame>]) {
        let Some(base) = self.solid_base else { return };
        let lights = rwf_core::render::solid::LightViews::new(&flame.solid);
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
        for (li, layer) in prepared.layers.iter().take(self.layers.len()).enumerate() {
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
    kind: rwf_core::variations::VarKind::Normal,
    is_3d: false,
    apply: |_, _, _, _, _| {},
    prepare: None,
    wgsl: "",
};

fn pack_xform(p: &mut [f32], base: usize, xf: &PreparedXForm) {
    let put6 = |p: &mut [f32], o: usize, a: &rwf_core::model::Affine| {
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
    if let Some(w) = &xf.weighting {
        let b = base + wf_offset(xf);
        let mut put = |o: usize, v: f64| p[b + o] = v as f32;
        put(wfo::VAR_AMOUNT, w.var_amount_intensity);
        put(wfo::COLOR_SCALE, w.color_scale);
        put(wfo::JITTER, w.jitter);
        match &w.field {
            Field::Empty => put(wfo::TYPE, 0.0),
            Field::Noise(n) => {
                use rwf_core::fastnoise::{CellularDistance, CellularReturn, FractalType};
                put(wfo::TYPE, field_code(&w.field) as f64);
                put(wfo::SEED, n.seed as f64);
                put(wfo::FREQUENCY, n.frequency as f64);
                put(wfo::OCTAVES, n.octaves as f64);
                put(wfo::LACUNARITY, n.lacunarity as f64);
                put(wfo::GAIN, n.gain as f64);
                put(wfo::BOUNDING, n.fractal_bounding() as f64);
                put(
                    wfo::FRACTAL,
                    match n.fractal_type {
                        FractalType::Fbm => 0.0,
                        FractalType::Billow => 1.0,
                        FractalType::RigidMulti => 2.0,
                    },
                );
                put(
                    wfo::CELL_DIST,
                    match n.cellular_distance {
                        CellularDistance::Euclidean => 0.0,
                        CellularDistance::Manhattan => 1.0,
                        CellularDistance::Natural => 2.0,
                    },
                );
                put(
                    wfo::CELL_RET,
                    match n.cellular_return {
                        CellularReturn::CellValue => 0.0,
                        CellularReturn::Distance => 1.0,
                        CellularReturn::Distance2 => 2.0,
                        CellularReturn::Distance2Add => 3.0,
                        CellularReturn::Distance2Sub => 4.0,
                        CellularReturn::Distance2Mul => 5.0,
                        CellularReturn::Distance2Div => 6.0,
                    },
                );
            }
            Field::Image { img, x_centre, y_centre, x_size, y_size } => {
                put(wfo::TYPE, 11.0);
                put(wfo::IMG, *x_centre);
                put(wfo::IMG + 1, *y_centre);
                put(wfo::IMG + 2, *x_size);
                put(wfo::IMG + 3, *y_size);
                put(wfo::IMG + 4, img.width as f64);
                put(wfo::IMG + 5, img.height as f64);
                // pixel start relative to the block (stays exact in f32)
                let rel = wfo::STEPS + wfo::STEP_LEN * steps(xf).count();
                put(wfo::PIXELS, rel as f64);
                for (i, px) in img.pixels.iter().enumerate() {
                    p[b + rel + i] = (px >> 16 & 255) as f32;
                }
            }
        }
        let mut t_off = b + step_tables_offset(xf);
        for t in step_tables(xf).iter().flatten() {
            p[t_off] = t.lo as f32;
            p[t_off + 1] = ((t.levels - 1) as f64 / (t.hi - t.lo)) as f32;
            p[t_off + 2..t_off + t.len()].copy_from_slice(&t.data);
            t_off += t.len();
        }
        for (si, s) in steps(xf).enumerate() {
            let o = b + wfo::STEPS + wfo::STEP_LEN * si;
            if let Some(sw) = &s.weighting {
                p[o] = sw.params.iter().find(|q| q.param.is_none()).map_or(0.0, |q| q.intensity) as f32;
                for (n, q) in sw.params.iter().filter(|q| q.param.is_some()).enumerate() {
                    p[o + 1 + n] = q.intensity as f32;
                }
            }
        }
    }
}

/// Solid rendering plot: z-buffer + point attributes + shadow maps (atomic max of an
/// order-preserving key).
fn solid_plot_wgsl(base: usize, maps: u32, map_size: usize, dof: bool) -> String {
    let mut w = String::from(
        "var<private> cur_mat: f32;\n\
var<private> cur_rso: f32;\n\
@group(0) @binding(6) var<storage, read_write> zbuf: array<atomic<u32>>;\n\
@group(0) @binding(7) var<storage, read_write> sattr: array<f32>;\n",
    );
    if maps > 0 {
        w.push_str("@group(0) @binding(8) var<storage, read_write> smap: array<atomic<u32>>;\n");
    }
    w.push_str(
        "fn zkey(f: f32) -> u32 { let b = bitcast<u32>(f); if ((b & 0x80000000u) != 0u) { return ~b; } return b | 0x80000000u; }\n\
fn plot(px_in: f32, py_in: f32, pz: f32, col_in: vec3<f32>) {\n  let wp = vec3<f32>(px_in, py_in, pz);\n  if (cur_rso == 0.0) {\n",
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
    w.push_str("  }\n");
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
    let col = dim_col(col_in, intensity);\n\
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
    sym_rot_sin: f32, sym_rot_cos: f32, sym_do_rot: u32, dim_r: f32,
    dim_g: f32, dim_b: f32, _pad1: u32, _pad2: u32,
}

@group(0) @binding(0) var<uniform> U: IterUniforms;

// Weight of the layer being plotted (for the depth fade color).
var<private> cur_w: f32 = 1.0;

// Depth fade: blends the (weighted) point color into the fade color, both scaled by the layer
// weight (`plotPoint`: lerp(dimColor, color, intensity) * weight).
fn dim_col(col: vec3<f32>, intensity: f32) -> vec3<f32> {
    return col * intensity + vec3<f32>(U.dim_r, U.dim_g, U.dim_b) * (cur_w * (1.0 - intensity));
}
@group(0) @binding(5) var<storage, read_write> hist: array<atomic<i32>>;
"#;

/// Parameters, xform tables and chaos-game states (iterate and sub-flame kernels).
const ITER_COMMON: &str = r#"
struct State { x: f32, y: f32, z: f32, color: f32, r: f32, g: f32, b: f32, xf: u32, rng: u32, skip: u32, hide: f32, mat: f32 }

@group(0) @binding(1) var<storage, read> prm: array<f32>;
@group(0) @binding(2) var<storage, read> nxt: array<u32>;
@group(0) @binding(4) var<storage, read_write> states: array<State>;

const FUSE_ITERS: u32 = 30u;

fn bad(p: Pt) -> bool {
    let s = p.x + p.y + p.z;
    return !(s == s) || abs(p.x) > 1.0e18 || abs(p.y) > 1.0e18 || abs(p.z) > 1.0e18;
}

fn fresh_point() -> Pt {
    return Pt(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0, 0.0, rnd(), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
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
    splat(ix, iy, dim_col(col_in, intensity));
}

fn hist_add(ix: i32, iy: i32, col: vec3<f32>) {
    let idx = (u32(iy) * U.raster_w + u32(ix)) * 4u;
    atomicAdd(&hist[idx], 1);
    atomicAdd(&hist[idx + 1u], i32(col.x * U.color_scale));
    atomicAdd(&hist[idx + 2u], i32(col.y * U.color_scale));
    atomicAdd(&hist[idx + 3u], i32(col.z * U.color_scale));
}
"#;

/// Z-buffer of classic rendering (`RasterFloatIntWithZBuffer`): the maximum camera depth of
/// bright samples per cell, as an order preserving key (0 = empty).
const ZBUFFER_WGSL: &str = r#"
@group(0) @binding(6) var<storage, read_write> zbuf: array<atomic<u32>>;
fn zkey(f: f32) -> u32 { let b = bitcast<u32>(f); if ((b & 0x80000000u) != 0u) { return ~b; } return b | 0x80000000u; }
fn zbuf_add(ix: i32, iy: i32, z: f32, col: vec3<f32>) {
    if (0.299 * col.x + 0.588 * col.y + 0.113 * col.z > 8.0) {
        atomicMax(&zbuf[u32(iy) * U.raster_w + u32(ix)], zkey(z));
    }
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
    // Keep the gradient binding in the layout, even if no xform reads it (gradient maps).
    _ = cmap[0];
    var s = states[tid];
    rng_state = s.rng;
    var p = Pt(s.x, s.y, s.z, s.color, s.r, s.g, s.b, 0.0, s.hide, 0.0);
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

/// Sub-flame kernel: every thread writes `per_thread` visible points (after the finals) of
/// layer 0 to `outpts`, three vec4 per point (see `subflame.wgsl`). `NB`, `TS` and `MASK` are
/// substituted.
const SUB_KERNEL: &str = r#"
struct SubUniforms { thread_count: u32, per_thread: u32, out_base: u32, max_iter: u32 }
@group(0) @binding(0) var<uniform> SU: SubUniforms;
@group(0) @binding(5) var<storage, read_write> outpts: array<vec4<f32>>;

@compute @workgroup_size(64)
fn sub_iterate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let tid = gid.x;
    if (tid >= SU.thread_count) { return; }
    // Keep every binding in the layout, even if no xform reads the gradient.
    _ = cmap[0];
    _ = prm[0];
    var s = states[tid];
    rng_state = s.rng;
    cmap_base = 0u;
    var p = Pt(s.x, s.y, s.z, s.color, s.r, s.g, s.b, 0.0, s.hide, 0.0);
    var xf = s.xf;
    var skip = s.skip;
    for (var k = 0u; k < SU.per_thread; k++) {
        var q = p;
        var ok = false;
        for (var i = 0u; i < SU.max_iter; i++) {
            if (!has_next_0(xf)) { break; }
            xf = nxt[NBu + xf * TSu + (rnd_u32() & MASKu)];
            p = apply_xf_0(xf, p);
            if (bad(p)) { p = fresh_point(); skip = FUSE_ITERS; continue; }
            if (skip > 0u) { skip -= 1u; continue; }
            if (!visible_0(xf)) { continue; }
            q = finals_0(p);
            ok = true;
            break;
        }
        let o = (SU.out_base + tid * SU.per_thread + k) * 3u;
        outpts[o] = vec4<f32>(q.x, q.y, q.z, q.color);
        outpts[o + 1u] = vec4<f32>(q.r, q.g, q.b, q.rgbc);
        outpts[o + 2u] = vec4<f32>(q.hide, select(0.0, 1.0, ok && !bad(q)), 0.0, 0.0);
    }
    s.x = p.x; s.y = p.y; s.z = p.z; s.color = p.color; s.r = p.r; s.g = p.g; s.b = p.b;
    s.xf = xf; s.skip = skip; s.rng = rng_state; s.hide = p.hide;
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
