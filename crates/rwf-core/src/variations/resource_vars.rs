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

use super::{Ctx, LoadEnv, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::math::*;
use crate::mesh::{self, Mesh, MeshSource, Refinement};
use crate::resources::{self, ImageMap};

/// `limitIntVal(Tools.FTOI(x), lo, hi)` as `f64`.
fn ftoi_limit(x: f64, lo: i32, hi: i32) -> f64 {
    ftoi(x).clamp(lo, hi) as f64
}

/// Values per image block.
const IMG: usize = 3;

/// Fills the image block at `d[o..o + 3]` and appends the pixels (no-op if the image is missing).
fn load_image(d: &mut Vec<f64>, o: usize, path: &[u8]) {
    if let Some(img) = resources::image(&text(path)) {
        push_image(d, o, &img);
    }
}

fn push_image(d: &mut Vec<f64>, o: usize, img: &ImageMap) {
    d[o] = img.width as f64;
    d[o + 1] = img.height as f64;
    d[o + 2] = d.len() as f64;
    d.extend(img.pixels.iter().map(|&p| p as f64));
}

/// Image block of the alpha plane (alpha in the blue channel of the packed pixels).
fn push_alpha(d: &mut Vec<f64>, o: usize, img: &ImageMap) {
    d[o] = img.width as f64;
    d[o + 1] = img.height as f64;
    d[o + 2] = d.len() as f64;
    d.extend(img.alpha.iter().map(|&a| a as f64));
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
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

fn plane_wf_load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
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
fn mesh_load(d: &mut Vec<f64>, p: usize, src: Option<MeshSource>, colormap: &[u8], displ_map: &[u8]) {
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
// colormap_wf / post_colormap_wf (AbstractColorMapWFFunc),
// displacemap_wf / post_displacemap_wf (AbstractDisplacementMapWFFunc)

/// `getCurrImageFilename`: the frame's file of an image sequence (`name0001.png`, ...).
fn sequence_filename(name: &str, frame: u32, start: i32, digits: i32) -> String {
    let chars: Vec<char> = name.chars().collect();
    let Some(p) = chars.iter().rposition(|&c| c == '.') else {
        return name.to_string();
    };
    if p as i32 <= digits || p == chars.len() - 1 {
        return name.to_string();
    }
    let base: String = chars[..p - digits as usize].iter().collect();
    let ext: String = chars[p..].iter().collect();
    let mut number = (frame as i32 - 1 + start).to_string();
    while (number.chars().count() as i32) < digits {
        number.insert(0, '0');
    }
    format!("{base}{number}{ext}")
}

/// The image of an image-based variation: the embedded image, else the (sequence) file, else
/// the default gradient.
fn source_image(filename: &[u8], inlined: &[u8], sequence: Option<(i32, i32)>, frame: u32) -> Arc<ImageMap> {
    let img = if !inlined.is_empty() {
        resources::image_from_bytes(inlined)
    } else if !filename.is_empty() {
        let name = text(filename);
        resources::image(&match sequence {
            Some((start, digits)) => sequence_filename(&name, frame, start, digits),
            None => name,
        })
    } else {
        None
    };
    img.unwrap_or_else(ImageMap::default_gradient)
}

/// Tiling of an image index as in JWildfire (note: `-n` maps to `n`, outside the image).
fn wrap_tile(i: i32, n: i32) -> i32 {
    if i < 0 {
        i.wrapping_sub((i / n - 1).wrapping_mul(n))
    } else if i >= n {
        i - (i / n) * n
    } else {
        i
    }
}

/// `colormap_wf` data: 15 parameters, image, alpha plane (only with `with_alpha`).
const CM_IMG: usize = 15;
const CM_ALPHA: usize = CM_IMG + IMG;

const COLORMAP_PARAMS: [ParamDef; 15] = [
    ParamDef::new("scale_x", 1.0),
    ParamDef::new("scale_y", 1.0),
    ParamDef::new("scale_z", 0.0),
    ParamDef::new("offset_x", 0.0),
    ParamDef::new("offset_y", 0.0),
    ParamDef::new("offset_z", 0.0),
    ParamDef::int("tile_x", 1.0, 0.0, 1.0),
    ParamDef::int("tile_y", 1.0, 0.0, 1.0),
    ParamDef::int("reset_z", 1.0, 0.0, 1.0),
    ParamDef::int("dc_color", 1.0, 0.0, 1.0),
    ParamDef::int("blend_colormap", 0.0, 0.0, 1.0),
    ParamDef::int("with_alpha", 0.0, 0.0, 1.0),
    ParamDef::int("is_sequence", 0.0, 0.0, 1.0),
    ParamDef::int("sequence_start", 1.0, 0.0, 9999.0),
    ParamDef::int("sequence_digits", 4.0, 1.0, 8.0),
];

fn colormap_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    for v in &mut d[6..15] {
        *v = ftoi(*v) as f64;
    }
    d.extend([0.0; 2 * IMG]);
    d
}

fn colormap_load(d: &mut Vec<f64>, res: &[Vec<u8>], env: &LoadEnv) {
    let sequence = (d[12] > 0.0).then_some((d[13] as i32, d[14] as i32));
    let img = source_image(&res[0], &res[1], sequence, env.frame);
    push_image(d, CM_IMG, &img);
    if d[11] > 0.0 {
        push_alpha(d, CM_ALPHA, &img);
    }
}

/// `AbstractColorMapWFFunc.transform`: colors (and optionally lifts or hides) the point by the
/// image pixel at `(inx, iny)`.
fn colormap_apply(d: &[f64], ctx: &mut Ctx, v: &mut Pt, inx: f64, iny: f64) {
    // d = [scale_x, scale_y, scale_z, offset_x, offset_y, offset_z, tile_x, tile_y, reset_z,
    //      dc_color, blend_colormap, with_alpha, is_sequence, sequence_start, sequence_digits,
    //      image (3), alpha (3), ..]
    let (w, h) = (d[CM_IMG] as i32, d[CM_IMG + 1] as i32);
    let x = (inx - (d[3] + 0.5) + 1.0) / d[0] * (w - 2) as f64;
    let y = (iny - (d[4] + 0.5) + 1.0) / d[1] * (h - 2) as f64;
    let blend = d[10] > 0.0;
    let (mut ix, mut iy) = if blend { (x.trunc() as i32, y.trunc() as i32) } else { (ftoi(x), ftoi(y)) };
    if d[6] as i32 == 1 {
        ix = wrap_tile(ix, w);
    }
    if d[7] as i32 == 1 {
        iy = wrap_tile(iy, h);
    }
    let with_alpha = d[11] > 0.0;
    let rgba = |ix: i32, iy: i32| {
        let c = pixel(d, CM_IMG, ix as i64, iy as i64);
        let a = if with_alpha { pixel(d, CM_ALPHA, ix as i64, iy as i64)[2] } else { 255.0 };
        [c[0], c[1], c[2], a]
    };
    let c = if ix >= 0 && ix < w && iy >= 0 && iy < h {
        if blend {
            // neighbours beyond the border are transparent black
            let at = |ok: bool, ix: i32, iy: i32| if ok { rgba(ix, iy) } else { [0.0; 4] };
            let (lu, ru, lb, rb) = (rgba(ix, iy), at(ix < w - 1, ix + 1, iy), at(iy < h - 1, ix, iy + 1), at(ix < w - 1 && iy < h - 1, ix + 1, iy + 1));
            let (fu, fv) = (frac(x), frac(y));
            [0, 1, 2, 3].map(|k| blerp(lu[k], ru[k], lb[k], rb[k], fu, fv))
        } else {
            rgba(ix, iy)
        }
    } else {
        [0.0; 4]
    };
    let dc_color = d[9] as i32;
    if dc_color == 0 {
        v.rgb_color = true;
        v.red = c[0];
        v.green = c[1];
        v.blue = c[2];
    }
    v.do_hide = with_alpha && c[3] <= 0.1;
    let mut dz = d[5];
    if d[2].abs() > EPSILON {
        dz += d[2] * (0.299 * c[0] + 0.588 * c[1] + 0.113 * c[2]) / 255.0;
    }
    if d[8] as i32 != 0 {
        v.z = dz;
    } else {
        v.z += dz;
    }
    if dc_color > 0 {
        v.color = ctx.uv_color_idx(ftoi(c[0]), ftoi(c[1]), ftoi(c[2]));
    }
}

/// `displacemap_wf` data: 9 parameters, image.
const DM_IMG: usize = 9;

const DISPLACEMAP_PARAMS: [ParamDef; 9] = [
    ParamDef::int("mode", 0.0, 0.0, 3.0),
    ParamDef::int("color_mode", 0.0, 0.0, 1.0),
    ParamDef::range("bias", 0.0, 0.0, 1.0),
    ParamDef::new("scale_x", 1.0),
    ParamDef::new("scale_y", 1.0),
    ParamDef::new("offset_x", 0.0),
    ParamDef::new("offset_y", 0.0),
    ParamDef::int("tile_x", 1.0, 0.0, 1.0),
    ParamDef::int("tile_y", 1.0, 0.0, 1.0),
];

fn displacemap_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    for i in [0, 1, 7, 8] {
        d[i] = ftoi(p[i]) as f64;
    }
    d.extend([0.0; IMG]);
    d
}

fn displacemap_load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
    push_image(d, DM_IMG, &source_image(&res[0], &res[1], None, 1));
}

/// `AbstractDisplacementMapWFFunc.transform`: rotates, translates, scales or shears the
/// accumulated point by the image pixel at `(inx, iny)`.
fn displacemap_apply(d: &[f64], ctx: &mut Ctx, v: &mut Pt, amount: f64, inx: f64, iny: f64) {
    // d = [mode, color_mode, bias, scale_x, scale_y, offset_x, offset_y, tile_x, tile_y, image (3), ..]
    let (w, h) = (d[DM_IMG] as i32, d[DM_IMG + 1] as i32);
    let mut ix = ftoi((inx - (d[5] + 0.5) + 1.0) / d[3] * (w - 1) as f64);
    let mut iy = ftoi((iny - (d[6] + 0.5) + 1.0) / d[4] * (h - 1) as f64);
    if d[7] as i32 == 1 {
        ix = wrap_tile(ix, w);
    }
    if d[8] as i32 == 1 {
        iy = wrap_tile(iy, h);
    }
    if ix < 0 || ix >= w || iy < 0 || iy >= h {
        return;
    }
    let c = pixel(d, DM_IMG, ix as i64, iy as i64).map(|c| c / 255.0);
    let intensity = 0.299 * c[0] + 0.588 * c[1] + 0.113 * c[2] - d[2];
    match d[0] as i32 {
        1 => {
            v.x += (c[0] - 0.5) * amount;
            v.y += (c[1] - 0.5) * amount;
        }
        2 => {
            if intensity > 0.0 {
                let scl = 1.0 + (intensity - 0.5) * amount;
                v.x *= scl;
                v.y *= scl;
            }
        }
        3 => {
            let (ax, ay) = ((c[0] - 0.5) * amount, (c[1] - 0.5) * amount);
            let nx = v.x * ax * ay + v.y * ay;
            let ny = v.x * ay - v.y * ax * ay;
            v.x = nx;
            v.y = ny;
        }
        _ => {
            if intensity > 0.0 {
                let (sa, ca) = (intensity * M_2PI * amount).sin_cos();
                let nx = v.x * ca - v.y * sa;
                let ny = v.x * sa + v.y * ca;
                v.x = nx;
                v.y = ny;
            }
        }
    }
    if d[1] as i32 == 1 {
        v.rgb_color = true;
        v.red = c[0] * 255.0;
        v.green = c[1] * 255.0;
        v.blue = c[2] * 255.0;
        // as in JWildfire: 0..1 components against the 0..200 render palette
        v.color = ctx.unit_color_idx(c);
    }
}

/// `post_bumpmap_wf` data: 7 parameters, image (black 320x256 if none, as in JWildfire).
const BM_IMG: usize = 7;

fn bumpmap_load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
    static BLACK: OnceLock<Arc<ImageMap>> = OnceLock::new();
    let img = if !res[1].is_empty() {
        resources::image_from_bytes(&res[1])
    } else if !res[0].is_empty() {
        resources::image(&text(&res[0]))
    } else {
        None
    };
    let img = img.unwrap_or_else(|| {
        BLACK.get_or_init(|| Arc::new(ImageMap { width: 320, height: 256, pixels: vec![0; 320 * 256], alpha: vec![255; 320 * 256] })).clone()
    });
    push_image(d, BM_IMG, &img);
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
    VarDef {
        name: "colormap_wf",
        params: &COLORMAP_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, _| colormap_apply(d, ctx, v, a.x, a.y),
        prepare: Some(|p, _| colormap_prepare(p)),
        wgsl: "v = res_colormap(base, a.x, a.y, v);",
    },
    VarDef {
        name: "post_colormap_wf",
        params: &COLORMAP_PARAMS,
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, _, v, _| {
            let (x, y) = (v.x, v.y);
            colormap_apply(d, ctx, v, x, y)
        },
        prepare: Some(|p, _| colormap_prepare(p)),
        wgsl: "v = res_colormap(base, v.x, v.y, v);",
    },
    VarDef {
        name: "displacemap_wf",
        params: &DISPLACEMAP_PARAMS,
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| displacemap_apply(d, ctx, v, amount, a.x, a.y),
        prepare: Some(|p, _| displacemap_prepare(p)),
        wgsl: "v = res_displacemap(base, a.x, a.y, AMOUNT, v);",
    },
    VarDef {
        name: "post_displacemap_wf",
        params: &DISPLACEMAP_PARAMS,
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let (x, y) = (v.x, v.y);
            displacemap_apply(d, ctx, v, amount, x, y)
        },
        prepare: Some(|p, _| displacemap_prepare(p)),
        wgsl: "v = res_displacemap(base, v.x, v.y, AMOUNT, v);",
    },
    VarDef {
        name: "post_bumpmap_wf",
        params: &[
            ParamDef::new("scale_x", 1.0),
            ParamDef::new("scale_y", 1.0),
            ParamDef::new("scale_z", 0.2),
            ParamDef::new("offset_x", 0.0),
            ParamDef::new("offset_y", 0.0),
            ParamDef::new("offset_z", 0.0),
            ParamDef::int("reset_z", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, a, v, _| {
            // d = [scale_x, scale_y, scale_z, offset_x, offset_y, offset_z, reset_z, image (3), ..]
            let (w, h) = (d[BM_IMG] as i32, d[BM_IMG + 1] as i32);
            let ix = ftoi((a.x - d[3] + 1.0) / d[0] * 0.5 * (w - 1) as f64);
            let iy = ftoi((a.y - d[4] + 1.0) / d[1] * 0.5 * (h - 1) as f64);
            if ix >= 0 && ix < w && iy >= 0 && iy < h {
                let c = pixel(d, BM_IMG, ix as i64, iy as i64);
                let dz = d[5] + d[2] * (0.299 * c[0] + 0.588 * c[1] + 0.113 * c[2]) / 255.0;
                if d[6] as i32 != 0 {
                    v.z = dz;
                } else {
                    v.z += dz;
                }
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[6] = ftoi(p[6]) as f64;
            d.extend([0.0; IMG]);
            d
        }),
        wgsl: "let w = i32(D(7));
let h = i32(D(8));
let ix = ftoi((a.x - D(3) + 1.0) / D(0) * 0.5 * f32(w - 1));
let iy = ftoi((a.y - D(4) + 1.0) / D(1) * 0.5 * f32(h - 1));
if (ix >= 0 && ix < w && iy >= 0 && iy < h) {
  let c = res_pixel(base, 7u, ix, iy);
  let dz = D(5) + D(2) * (0.299 * c.x + 0.588 * c.y + 0.113 * c.z) / 255.0;
  if (i32(D(6)) != 0) { v.z = dz; } else { v.z += dz; }
}",
    },
];

const COLORMAP_RESOURCES: [ResDef; 4] = [
    ResDef { name: "image_filename", kind: ResKind::ImageFile },
    ResDef { name: "inlined_image", kind: ResKind::ImageData },
    ResDef { name: "image_desc_src", kind: ResKind::Href },
    ResDef { name: "image_src", kind: ResKind::Href },
];

const DISPLACEMAP_RESOURCES: [ResDef; 2] =
    [ResDef { name: "image_filename", kind: ResKind::ImageFile }, ResDef { name: "inlined_image", kind: ResKind::ImageData }];

const IMAGE_RESOURCES: [ResDef; 2] =
    [ResDef { name: "colormap_filename", kind: ResKind::ImageFile }, ResDef { name: "displ_map_filename", kind: ResKind::ImageFile }];

pub static RESOURCES: &[ResourceVar] = &[
    ResourceVar { name: "colormap_wf", resources: &COLORMAP_RESOURCES, load: colormap_load },
    ResourceVar { name: "post_colormap_wf", resources: &COLORMAP_RESOURCES, load: colormap_load },
    ResourceVar { name: "displacemap_wf", resources: &DISPLACEMAP_RESOURCES, load: displacemap_load },
    ResourceVar { name: "post_displacemap_wf", resources: &DISPLACEMAP_RESOURCES, load: displacemap_load },
    ResourceVar { name: "post_bumpmap_wf", resources: &DISPLACEMAP_RESOURCES, load: bumpmap_load },
    ResourceVar { name: "plane_wf", resources: &IMAGE_RESOURCES, load: plane_wf_load },
    ResourceVar {
        name: "obj_mesh_wf",
        resources: &[
            ResDef { name: "obj_filename", kind: ResKind::ObjFile },
            ResDef { name: "colormap_filename", kind: ResKind::ImageFile },
            ResDef { name: "displ_map_filename", kind: ResKind::ImageFile },
        ],
        load: |d, res, _| {
            let src = (!res[0].is_empty()).then(|| MeshSource::File(text(&res[0])));
            mesh_load(d, 0, src, &res[1], &res[2]);
        },
    },
    ResourceVar {
        name: "obj_mesh_primitive_wf",
        resources: &IMAGE_RESOURCES,
        load: |d, res, _| {
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
        let d = prepare_instance(def, inst, &Affine::IDENTITY, 1);
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

    #[test]
    fn sequence_file_names() {
        assert_eq!(sequence_filename("/x/img0001.png", 1, 1, 4), "/x/img0001.png");
        assert_eq!(sequence_filename("/x/img0001.png", 12, 1, 4), "/x/img0012.png");
        assert_eq!(sequence_filename("/x/img0001.png", 3, 100, 4), "/x/img0102.png");
        assert_eq!(sequence_filename("a.png", 3, 1, 4), "a.png");
    }

    #[test]
    fn colormap_wf_default_image() {
        let mut inst = VariationInstance::new("colormap_wf", 1.0);
        inst.set_param("dc_color", 0.0);
        let def = find("colormap_wf").unwrap();
        let d = prepare_instance(def, &inst, &Affine::IDENTITY, 1);
        let mut ctx = Ctx::new(1, false);
        // (0, 0) maps to pixel (127, 127) of the default gradient
        let mut a = Pt::default();
        let mut v = Pt { z: 5.0, ..Pt::default() };
        (def.apply)(&d, &mut ctx, &mut a, &mut v, 1.0);
        let img = ImageMap::default_gradient();
        let p = img.pixels[127 * 256 + 127];
        assert!(v.rgb_color && [v.red, v.green, v.blue] == [(p >> 16 & 255) as f64, (p >> 8 & 255) as f64, (p & 255) as f64]);
        assert_eq!(v.z, 0.0); // reset_z
        assert!(!v.do_hide);
    }

    #[test]
    fn colormap_wf_alpha_hides_transparent_pixels() {
        let path = std::env::temp_dir().join(format!("rwf_res_{}_alpha.png", std::process::id()));
        image::RgbaImage::from_fn(8, 8, |x, _| image::Rgba([200, 10, 10, if x < 4 { 0 } else { 255 }])).save(&path).unwrap();
        let mut inst = VariationInstance::new("colormap_wf", 1.0);
        inst.set_param("with_alpha", 1.0);
        inst.set_param("tile_x", 0.0);
        inst.set_resource("image_filename", &path.display().to_string());
        let def = find("colormap_wf").unwrap();
        let d = prepare_instance(def, &inst, &Affine::IDENTITY, 1);
        let mut ctx = Ctx::new(1, false);
        // x = (in + 0.5) * 6: in = -0.3 -> 1.2 (transparent), in = 0.3 -> 4.8 (opaque)
        for (inx, hidden) in [(-0.3, true), (0.3, false)] {
            let mut a = Pt { x: inx, ..Pt::default() };
            let mut v = Pt::default();
            (def.apply)(&d, &mut ctx, &mut a, &mut v, 1.0);
            assert_eq!(v.do_hide, hidden, "{inx}");
        }
    }

    #[test]
    fn displacemap_wf_translates_by_red_green() {
        let png = write_png("dmap", 4, 4, |_, _| [255, 0, 0]);
        let mut inst = VariationInstance::new("post_displacemap_wf", 1.0);
        inst.set_param("mode", 1.0);
        inst.set_resource("image_filename", &png);
        let def = find("post_displacemap_wf").unwrap();
        let d = prepare_instance(def, &inst, &Affine::IDENTITY, 1);
        let mut ctx = Ctx::new(1, false);
        let mut a = Pt::default();
        let mut v = Pt { x: 0.1, y: 0.2, ..Pt::default() };
        (def.apply)(&d, &mut ctx, &mut a, &mut v, 2.0);
        assert!((v.x - 1.1).abs() < 1e-12 && (v.y - (0.2 - 1.0)).abs() < 1e-12, "{v:?}");
    }
}
