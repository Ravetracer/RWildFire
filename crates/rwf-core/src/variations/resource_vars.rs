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

//! Variations with external resources (images, meshes): `plane_wf`, `obj_mesh_wf`,
//! `obj_mesh_primitive_wf`.
//!
//! Resources are packed into the prepared data after the parameters (see [`super::LoadFn`]).
//! An *image block* is three values `[width, height, start]`, `start` being the index in `d` of
//! the `width * height` packed `r << 16 | g << 8 | b` pixels; width 0 means "no image". The WGSL
//! counterparts (`res_*`) live in `wgsl/resource.wgsl`.

use std::sync::{Arc, OnceLock};

use super::{Ctx, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::math::*;
use crate::mesh::{self, Mesh, MeshSource, Refinement};
use crate::resources;

/// `limitIntVal(Tools.FTOI(x), lo, hi)` as `f64`.
fn ftoi_limit(x: f64, lo: i32, hi: i32) -> f64 {
    ftoi(x).clamp(lo, hi) as f64
}

/// Values per image block.
const IMG: usize = 3;

/// Fills the image block at `d[o..o + 3]` and appends the pixels (no-op if the image is missing).
fn load_image(d: &mut Vec<f64>, o: usize, path: &str) {
    if let Some(img) = resources::image(path) {
        d[o] = img.width as f64;
        d[o + 1] = img.height as f64;
        d[o + 2] = d.len() as f64;
        d.extend(img.pixels.iter().map(|&p| p as f64));
    }
}

#[inline]
fn pixel(d: &[f64], o: usize, ix: i64, iy: i64) -> [f64; 3] {
    let p = d[d[o + 2] as usize + (iy * d[o] as i64 + ix) as usize] as u32;
    [(p >> 16 & 255) as f64, (p >> 8 & 255) as f64, (p & 255) as f64]
}

/// Pixel at `(ix, iy)`, optionally blended with its right/lower neighbours (wrapping) by the
/// fractional part of `(iu, iv)`. `None` outside the image.
fn sample(d: &[f64], o: usize, blend: bool, ix: i64, iy: i64, iu: f64, iv: f64) -> Option<[f64; 3]> {
    let (w, h) = (d[o] as i64, d[o + 1] as i64);
    if ix < 0 || ix >= w || iy < 0 || iy >= h {
        return None;
    }
    if !blend {
        return Some(pixel(d, o, ix, iy));
    }
    let ix1 = if ix + 1 >= w { 0 } else { ix + 1 };
    let iy1 = if iy + 1 >= h { 0 } else { iy + 1 };
    let (fu, fv) = (frac(iu), frac(iv));
    let (lu, ru, lb, rb) = (pixel(d, o, ix, iy), pixel(d, o, ix1, iy), pixel(d, o, ix, iy1), pixel(d, o, ix1, iy1));
    Some([0, 1, 2].map(|k| blerp(lu[k], ru[k], lb[k], rb[k], fu, fv)))
}

/// `ColorMapHolder.applyImageColor`: true color from the image, black outside of it.
fn apply_image_color(d: &[f64], o: usize, blend: bool, v: &mut Pt, iu: f64, iv: f64) {
    let c = sample(d, o, blend, iu.trunc() as i64, iv.trunc() as i64, iu, iv).unwrap_or([0.0; 3]);
    v.rgb_color = true;
    v.red = c[0];
    v.green = c[1];
    v.blue = c[2];
}

/// `DisplacementMapHolder.calculateImageDisplacement`: green channel, 0..1.
fn image_displacement(d: &[f64], o: usize, blend: bool, iu: f64, iv: f64) -> f64 {
    sample(d, o, blend, iu.trunc() as i64, iv.trunc() as i64, iu, iv).map_or(0.0, |c| c[1] / 255.0)
}

// ---------------------------------------------------------------------------------------------
// plane_wf

/// `plane_wf` data: 10 parameters, colormap image, displacement image.
const PW_CM: usize = 10;
const PW_DM: usize = PW_CM + IMG;

fn plane_wf_apply(d: &[f64], ctx: &mut Ctx, _: &mut Pt, v: &mut Pt, amount: f64) {
    // d = [position, size, axis, direct_color, color_mode, blend_colormap, displ_amount,
    //      blend_displ_map, calc_color_idx, receive_only_shadows, colormap (3), displ map (3), ..]
    let r1 = 0.5 - ctx.random();
    let r2 = 0.5 - ctx.random();
    let axis = d[2] as i32;
    // (u, v) of setColor / getDisplacement: (x, y), (y, z) or (z, x)
    let (cu, cv) = if axis == 2 { (r2, r1) } else { (r1, r2) };
    let displ = if d[PW_DM] > 0.0 && d[6].abs() > EPSILON {
        image_displacement(d, PW_DM, d[7] > 0.0, (cu + 0.5) * d[PW_DM], (cv + 0.5) * d[PW_DM + 1]) * d[6]
    } else {
        0.0
    };
    let (pos, size) = (d[0] + displ, d[1]);
    let (x, y, z) = match axis {
        0 => (r1 * size, r2 * size, pos),
        1 => (pos, r1 * size, r2 * size),
        _ => (r1 * size, pos, r2 * size),
    };
    if d[3] as i32 > 0 {
        match d[4] as i32 {
            0 => {
                apply_image_color(d, PW_CM, d[5] > 0.0, v, (cu + 0.5) * d[PW_CM], (cv + 0.5) * d[PW_CM + 1]);
                if d[8] as i32 == 1 {
                    v.color = ctx.uv_color_idx(ftoi(v.red), ftoi(v.green), ftoi(v.blue));
                }
            }
            2 => v.color = cv + 0.5,
            3 => v.color = (cv + 0.5) * (cu + 0.5),
            _ => v.color = cu + 0.5,
        }
        v.color = v.color.clamp(0.0, 1.0);
    }
    if d[9] as i32 == 1 {
        v.receive_only_shadows = true;
    }
    v.x += amount * x;
    v.y += amount * y;
    v.z += amount * z;
}

fn plane_wf_load(d: &mut Vec<f64>, res: &[String]) {
    load_image(d, PW_CM, &res[0]);
    load_image(d, PW_DM, &res[1]);
}

// ---------------------------------------------------------------------------------------------
// obj_mesh_wf / obj_mesh_primitive_wf (AbstractOBJMeshWFFunc)

/// Parameters shared by both mesh variations (after `primitive` for the primitive one).
const MESH_PARAMS: [ParamDef; 14] = [
    ParamDef::range("scale_x", 1.0, 0.0, 4.0),
    ParamDef::range("scale_y", 1.0, 0.0, 4.0),
    ParamDef::range("scale_z", 1.0, 0.0, 4.0),
    ParamDef::new("offset_x", 0.0),
    ParamDef::new("offset_y", 0.0),
    ParamDef::new("offset_z", 0.0),
    ParamDef::int("subdiv_level", 0.0, 0.0, 6.0),
    ParamDef::int("subdiv_smooth_passes", 12.0, 0.0, 24.0),
    ParamDef::range("subdiv_smooth_lambda", 0.42, 0.0, 1.0),
    ParamDef::range("subdiv_smooth_mu", -0.45, -1.0, 0.0),
    ParamDef::int("blend_colormap", 1.0, 0.0, 1.0),
    ParamDef::new("displ_amount", 0.1),
    ParamDef::int("blend_displ_map", 1.0, 0.0, 1.0),
    ParamDef::int("receive_only_shadows", 0.0, 0.0, 1.0),
];

const fn primitive_params() -> [ParamDef; 15] {
    let mut p = [ParamDef::int("primitive", 0.0, 0.0, 25.0); 15];
    let mut i = 0;
    while i < 14 {
        p[i + 1] = MESH_PARAMS[i];
        i += 1;
    }
    p
}
static PRIMITIVE_PARAMS: [ParamDef; 15] = primitive_params();

// Mesh data after the parameters (relative to `o`, the colormap image block).
const M_DISPL: usize = IMG;
const M_NV: usize = 2 * IMG;
const M_NF: usize = M_NV + 1;
const M_TOTAL: usize = M_NV + 2;
const M_VSTART: usize = M_NV + 3;
const M_FSTART: usize = M_NV + 4;
const M_HEADER: usize = M_NV + 5;
/// Values per vertex `[x, y, z, u, v, has_uv]` and per face `[v1, v2, v3, cumulative count]`.
const VREC: usize = 6;
const FREC: usize = 4;

/// Limits the shared parameters (starting at `p`) and reserves the resource header.
fn mesh_prepare(params: &[f64], p: usize) -> Vec<f64> {
    let mut d = params.to_vec();
    d[p + 6] = ftoi_limit(params[p + 6], 0, 6);
    d[p + 7] = ftoi_limit(params[p + 7], 0, 24);
    d[p + 10] = ftoi_limit(params[p + 10], 0, 1);
    d[p + 12] = ftoi_limit(params[p + 12], 0, 1);
    d[p + 13] = ftoi_limit(params[p + 13], 0, 1);
    d.extend([0.0; M_HEADER]);
    d
}

fn default_mesh() -> Arc<Mesh> {
    static CUBE: OnceLock<Arc<Mesh>> = OnceLock::new();
    CUBE.get_or_init(|| Arc::new(Mesh::default_cube())).clone()
}

/// Loads the images and the mesh (the cube if `src` is unavailable) and packs them into `d`.
fn mesh_load(d: &mut Vec<f64>, p: usize, src: Option<MeshSource>, colormap: &str, displ_map: &str) {
    let o = p + MESH_PARAMS.len();
    load_image(d, o, colormap);
    load_image(d, o + M_DISPL, displ_map);
    let r = Refinement { level: d[p + 6] as i32, smooth_passes: d[p + 7] as i32, lambda: d[p + 8], mu: d[p + 9] };
    let m = src.and_then(|s| mesh::load(&s, r)).unwrap_or_else(default_mesh);
    d[o + M_NV] = m.vertices.len() as f64;
    d[o + M_NF] = m.faces.len() as f64;
    d[o + M_TOTAL] = m.face_count() as f64;
    d[o + M_VSTART] = d.len() as f64;
    d.reserve(m.vertices.len() * VREC + m.faces.len() * FREC);
    for v in &m.vertices {
        let (u, w) = v.uv.unwrap_or((0.0, 0.0));
        d.extend([v.x as f64, v.y as f64, v.z as f64, u as f64, w as f64, if v.uv.is_some() { 1.0 } else { 0.0 }]);
    }
    d[o + M_FSTART] = d.len() as f64;
    for (i, f) in m.faces.iter().enumerate() {
        let cum = m.cum_counts.as_ref().map_or(i as u64 + 1, |c| c[i]);
        d.extend([f[0] as f64, f[1] as f64, f[2] as f64, cum as f64]);
    }
}

/// `AbstractOBJMeshWFFunc.transform`; `p` = index of `scale_x`.
fn mesh_apply(d: &[f64], p: usize, ctx: &mut Ctx, v: &mut Pt, amount: f64) {
    let o = p + MESH_PARAMS.len();
    let total = d[o + M_TOTAL] as usize;
    if total == 0 {
        return;
    }
    // Face `k` of the (virtually) expanded face list.
    let k = ctx.random_int(total) as f64;
    let fs = d[o + M_FSTART] as usize;
    let (mut lo, mut hi) = (0, d[o + M_NF] as usize - 1);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if d[fs + mid * FREC + 3] > k {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let f = fs + lo * FREC;
    let vs = d[o + M_VSTART] as usize;
    let vi = [0, 1, 2].map(|i| vs + d[f + i] as usize * VREC);
    // Vertex.transform: float precision
    let pt = vi.map(|b| [0, 1, 2].map(|c| (d[b + c] * d[p + c] + d[p + 3 + c]) as f32 as f64));

    let sqrt_r1 = ctx.random().sqrt();
    let r2 = ctx.random();
    let (a, b, c) = (1.0 - sqrt_r1, sqrt_r1 * (1.0 - r2), r2 * sqrt_r1);
    v.x += amount * (a * pt[0][0] + b * pt[1][0] + c * pt[2][0]);
    v.y += amount * (a * pt[0][1] + b * pt[1][1] + c * pt[2][1]);
    v.z += amount * (a * pt[0][2] + b * pt[1][2] + c * pt[2][2]);

    let color_on = d[o] > 0.0;
    let displ_amount = d[p + 11];
    let displ_on = d[o + M_DISPL] > 0.0 && displ_amount.abs() > EPSILON;
    if (color_on || displ_on) && d[vi[0] + 5] != 0.0 {
        let u = a * d[vi[0] + 3] + b * d[vi[1] + 3] + c * d[vi[2] + 3];
        let w = a * d[vi[0] + 4] + b * d[vi[1] + 4] + c * d[vi[2] + 4];
        // texture v runs bottom-up
        let uv = |io: usize| {
            let (iw, ih) = (d[io] - 1.0, d[io + 1] - 1.0);
            ((u * iw).clamp(0.0, iw), (ih - w * ih).clamp(0.0, ih))
        };
        if color_on {
            let (iu, iv) = uv(o);
            apply_image_color(d, o, d[p + 10] > 0.0, v, iu, iv);
            v.color = ctx.uv_color_idx(ftoi(v.red), ftoi(v.green), ftoi(v.blue));
        }
        if displ_on {
            // As in JWildfire, the z components of the edge vectors repeat the y difference.
            let av = [pt[1][0] - pt[0][0], pt[1][1] - pt[0][1], pt[1][1] - pt[0][1]];
            let bv = [pt[2][0] - pt[0][0], pt[2][1] - pt[0][1], pt[2][1] - pt[0][1]];
            let n = [av[1] * bv[2] - av[2] * bv[1], av[2] * bv[0] - av[0] * bv[2], av[0] * bv[1] - av[1] * bv[0]];
            let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() + 1.0e-16;
            let (iu, iv) = uv(o + M_DISPL);
            let dd = image_displacement(d, o + M_DISPL, d[p + 12] > 0.0, iu, iv) * displ_amount;
            v.x += amount * n[0] / l * dd;
            v.y += amount * n[1] / l * dd;
            v.z += amount * n[2] / l * dd;
        }
    }
    if d[p + 13] as i32 == 1 {
        v.receive_only_shadows = true;
    }
}

// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "plane_wf",
        params: &[
            ParamDef::new("position", 3.0),
            ParamDef::new("size", 10.0),
            ParamDef::int("axis", 2.0, 0.0, 2.0),
            ParamDef::int("direct_color", 1.0, 0.0, 1.0),
            ParamDef::int("color_mode", 3.0, 0.0, 3.0),
            ParamDef::int("blend_colormap", 1.0, 0.0, 1.0),
            ParamDef::new("displ_amount", 0.1),
            ParamDef::int("blend_displ_map", 1.0, 0.0, 1.0),
            ParamDef::int("calc_color_idx", 0.0, 0.0, 1.0),
            ParamDef::int("receive_only_shadows", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: plane_wf_apply,
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[2] = ftoi_limit(p[2], 0, 2);
            d[3] = ftoi_limit(p[3], 0, 1);
            d[4] = ftoi_limit(p[4], 0, 3);
            d[5] = ftoi_limit(p[5], 0, 1);
            d[7] = ftoi_limit(p[7], 0, 1);
            d[8] = ftoi_limit(p[8], 0, 1);
            d[9] = ftoi_limit(p[9], 0, 1);
            d.extend([0.0; 2 * IMG]);
            d
        }),
        wgsl: "let r1 = 0.5 - rnd();
let r2 = 0.5 - rnd();
let axis = i32(D(2));
var cu = r1; var cv = r2;
if (axis == 2) { cu = r2; cv = r1; }
var pos = D(0);
if (D(13) > 0.0 && abs(D(6)) > EPSILON) {
  let iu = (cu + 0.5) * D(13); let iv = (cv + 0.5) * D(14);
  pos += res_image_displ(base, 13u, D(7) > 0.0, iu, iv) * D(6);
}
var x = r1 * D(1); var y = pos; var z = r2 * D(1);
if (axis == 0) { x = r1 * D(1); y = r2 * D(1); z = pos; }
else if (axis == 1) { x = pos; y = r1 * D(1); z = r2 * D(1); }
if (i32(D(3)) > 0) {
  let mode = i32(D(4));
  if (mode == 0) {
    let rgb = res_image_color(base, 10u, D(5) > 0.0, (cu + 0.5) * D(10), (cv + 0.5) * D(11));
    v.rgbc = 1.0; v.r = rgb.x; v.g = rgb.y; v.b = rgb.z;
    if (i32(D(8)) == 1) { v.color = res_uv_color_idx(rgb); }
  }
  else if (mode == 2) { v.color = cv + 0.5; }
  else if (mode == 3) { v.color = (cv + 0.5) * (cu + 0.5); }
  else { v.color = cu + 0.5; }
  v.color = clamp(v.color, 0.0, 1.0);
}
if (i32(D(9)) == 1) { v.rso = 1.0; }
v.x += AMOUNT * x;
v.y += AMOUNT * y;
v.z += AMOUNT * z;",
    },
    VarDef {
        name: "obj_mesh_wf",
        params: &MESH_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| mesh_apply(d, 0, ctx, v, amount),
        prepare: Some(|p, _| mesh_prepare(p, 0)),
        wgsl: "v = res_mesh(base, 0u, v, AMOUNT);",
    },
    VarDef {
        name: "obj_mesh_primitive_wf",
        params: &PRIMITIVE_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| mesh_apply(d, 1, ctx, v, amount),
        prepare: Some(|p, _| {
            let mut d = mesh_prepare(p, 1);
            d[0] = ftoi(p[0]) as f64;
            d
        }),
        wgsl: "v = res_mesh(base, 1u, v, AMOUNT);",
    },
];

const IMAGE_RESOURCES: [ResDef; 2] =
    [ResDef { name: "colormap_filename", kind: ResKind::ImageFile }, ResDef { name: "displ_map_filename", kind: ResKind::ImageFile }];

pub static RESOURCES: &[ResourceVar] = &[
    ResourceVar { name: "plane_wf", resources: &IMAGE_RESOURCES, load: plane_wf_load },
    ResourceVar {
        name: "obj_mesh_wf",
        resources: &[
            ResDef { name: "obj_filename", kind: ResKind::ObjFile },
            ResDef { name: "colormap_filename", kind: ResKind::ImageFile },
            ResDef { name: "displ_map_filename", kind: ResKind::ImageFile },
        ],
        load: |d, res| {
            let src = (!res[0].is_empty()).then(|| MeshSource::File(res[0].clone()));
            mesh_load(d, 0, src, &res[1], &res[2]);
        },
    },
    ResourceVar {
        name: "obj_mesh_primitive_wf",
        resources: &IMAGE_RESOURCES,
        load: |d, res| {
            let i = d[0] as i64;
            let src = (0..mesh::PRIMITIVES.len() as i64).contains(&i).then_some(MeshSource::Primitive(i as usize));
            mesh_load(d, 1, src, &res[0], &res[1]);
        },
    },
];

#[cfg(test)]
mod tests {
    use super::super::{find, prepare_instance};
    use super::*;
    use crate::model::{Affine, VariationInstance};

    fn write_png(name: &str, w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> String {
        let path = std::env::temp_dir().join(format!("rwf_res_{}_{name}.png", std::process::id()));
        image::RgbImage::from_fn(w, h, |x, y| image::Rgb(f(x, y))).save(&path).unwrap();
        path.display().to_string()
    }

    fn run(inst: &VariationInstance, n: usize) -> Vec<Pt> {
        let def = find(&inst.name).unwrap();
        let d = prepare_instance(def, inst, &Affine::IDENTITY);
        let mut ctx = Ctx::new(7, false);
        (0..n)
            .map(|_| {
                let mut a = Pt::default();
                let mut v = Pt::default();
                (def.apply)(&d, &mut ctx, &mut a, &mut v, 1.0);
                v
            })
            .collect()
    }

    #[test]
    fn plane_wf_colormap_and_displacement() {
        // Left half red, right half blue; displacement: green 255 everywhere.
        let cm = write_png("cm", 4, 4, |x, _| if x < 2 { [255, 0, 0] } else { [0, 0, 255] });
        let dm = write_png("dm", 2, 2, |_, _| [0, 255, 0]);
        let mut inst = VariationInstance::new("plane_wf", 1.0);
        inst.set_param("axis", 0.0); // XY plane at z = position
        inst.set_param("color_mode", 0.0);
        inst.set_param("blend_colormap", 0.0);
        inst.set_param("displ_amount", 0.5);
        inst.set_resource("colormap_filename", &cm);
        inst.set_resource("displ_map_filename", &dm);
        for v in run(&inst, 200) {
            assert!(v.rgb_color);
            // x = u * size, u in (-0.5, 0.5]: left half of the image for negative u
            if v.x < -0.1 {
                assert_eq!([v.red, v.green, v.blue], [255.0, 0.0, 0.0]);
            } else if v.x > 0.1 {
                assert_eq!([v.red, v.green, v.blue], [0.0, 0.0, 255.0]);
            }
            assert_eq!(v.z, 3.5);
        }
    }

    #[test]
    fn plane_wf_without_image_keeps_flat_plane() {
        let inst = VariationInstance::new("plane_wf", 1.0);
        for v in run(&inst, 50) {
            assert_eq!(v.y, 3.0);
            assert!(!v.rgb_color && (0.0..=1.0).contains(&v.color));
        }
    }

    #[test]
    fn mesh_primitive_points_lie_on_box() {
        let mut inst = VariationInstance::new("obj_mesh_primitive_wf", 1.0);
        inst.set_param("primitive", 5.0); // box
        let pts = run(&inst, 500);
        let ext = pts.iter().fold(0.0f64, |m, v| m.max(v.x.abs()).max(v.y.abs()).max(v.z.abs()));
        assert!(ext > 0.1);
        for v in &pts {
            // on the surface: at least one coordinate at the extent
            let on = [v.x, v.y, v.z].iter().any(|c| (c.abs() - ext).abs() < 1e-4 * ext.max(1.0));
            assert!(on, "{v:?} ext {ext}");
        }
    }

    #[test]
    fn mesh_obj_file_with_colormap() {
        let obj = std::env::temp_dir().join(format!("rwf_res_{}_quad.obj", std::process::id()));
        std::fs::write(&obj, "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3 4/4\n").unwrap();
        let cm = write_png("mesh_cm", 2, 2, |_, _| [10, 200, 30]);
        let mut inst = VariationInstance::new("obj_mesh_wf", 1.0);
        inst.set_resource("obj_filename", &obj.display().to_string());
        inst.set_resource("colormap_filename", &cm);
        for v in run(&inst, 100) {
            assert!((0.0..=1.0).contains(&v.x) && (0.0..=1.0).contains(&v.y) && v.z == 0.0);
            // blended: identical neighbours up to rounding
            assert!(v.rgb_color && (v.red - 10.0).abs() < 1e-9 && (v.green - 200.0).abs() < 1e-9 && (v.blue - 30.0).abs() < 1e-9, "{v:?}");
        }
        // Missing file: the default cube.
        inst.set_resource("obj_filename", "/nonexistent.obj");
        assert!(run(&inst, 20).iter().all(|v| v.x.abs() <= 1.0 && v.y.abs() <= 1.0 && v.z.abs() <= 1.0));
    }
}
