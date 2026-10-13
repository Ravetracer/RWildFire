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

// kaleidoimg uses JWildfire's truncated pi values.
#![allow(clippy::approx_constant)]

use std::sync::{Arc, OnceLock};

use super::glsl::{glsl_atan, glsl_atan2};
use super::{Ctx, LoadEnv, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::math::*;
use crate::mesh::{self, Mesh, MeshSource, Refinement};
use crate::resources::{self, ImageMap};

/// `limitIntVal(Tools.FTOI(x), lo, hi)` as `f64`.
pub(super) fn ftoi_limit(x: f64, lo: i32, hi: i32) -> f64 {
    ftoi(x).clamp(lo, hi) as f64
}

/// Values per image block.
pub(super) const IMG: usize = 3;

/// Fills the image block at `d[o..o + 3]` and appends the pixels (no-op if the image is missing).
pub(super) fn load_image(d: &mut Vec<f64>, o: usize, path: &[u8]) {
    if let Some(img) = resources::image(&text(path)) {
        push_image(d, o, &img);
    }
}

pub(super) fn push_image(d: &mut Vec<f64>, o: usize, img: &ImageMap) {
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

pub(super) fn text(b: &[u8]) -> String {
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
pub(super) fn apply_image_color(d: &[f64], o: usize, blend: bool, v: &mut Pt, iu: f64, iv: f64) {
    let c = sample(d, o, blend, iu.trunc() as i64, iv.trunc() as i64, iu, iv).unwrap_or([0.0; 3]);
    v.rgb_color = true;
    v.red = c[0];
    v.green = c[1];
    v.blue = c[2];
}

/// `DisplacementMapHolder.calculateImageDisplacement`: green channel, 0..1.
pub(super) fn image_displacement(d: &[f64], o: usize, blend: bool, iu: f64, iv: f64) -> f64 {
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
pub(super) const M_NV: usize = 2 * IMG;
pub(super) const M_NF: usize = M_NV + 1;
pub(super) const M_TOTAL: usize = M_NV + 2;
pub(super) const M_VSTART: usize = M_NV + 3;
pub(super) const M_FSTART: usize = M_NV + 4;
pub(super) const M_HEADER: usize = M_NV + 5;
/// Values per vertex `[x, y, z, u, v, has_uv]` and per face `[v1, v2, v3, cumulative count]`.
pub(super) const VREC: usize = 6;
pub(super) const FREC: usize = 4;

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
    pack_mesh(d, o, &m);
}

/// Appends a mesh and fills the mesh header at `o + M_NV`.
pub(super) fn pack_mesh(d: &mut Vec<f64>, o: usize, m: &Mesh) {
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
// brush_stroke_wf / post_brush_stroke_wf (AbstractBrushStrokeWFFunc)

const BRUSH_PARAMS: [ParamDef; 20] = [
    ParamDef::range("blend", 0.25, 0.0, 1.0),
    ParamDef::range("grid_size", 0.025, 0.0, 0.25),
    ParamDef::new("grid_deform", 0.05),
    ParamDef::range("grid_brush_rotate", M_2PI, -M_2PI, M_2PI),
    ParamDef::new("grid_brush_scale", 0.35),
    ParamDef::range("grid_brush_blend", 0.15, 0.0, 1.0),
    ParamDef::int("color_channel", 0.0, 0.0, 3.0),
    ParamDef::range("threshold", 0.1, 0.0, 1.0),
    ParamDef::int("invert", 1.0, 0.0, 1.0),
    ParamDef::int("variation_batch_size", 12.0, 1.0, 50.0),
    ParamDef::new("variation_zoom", 0.8),
    ParamDef::new("variation_position", 0.5),
    ParamDef::range("variation_rotation", M_2PI, -M_2PI, M_2PI),
    ParamDef::range("variation_erosion", 0.1, 0.0, 1.0),
    ParamDef::int("variation_seed", 12345.0, 0.0, 99999.0),
    ParamDef::int("image_max_size", 320.0, 16.0, 2048.0),
    ParamDef::range("antialias_radius", 0.75, 0.0, 2.0),
    ParamDef::int("rnd_brush_seed", 0.0, 0.0, 99999.0),
    ParamDef::int("rnd_brush_min_count", 1.0, 1.0, 10.0),
    ParamDef::int("rnd_brush_max_count", 5.0, 1.0, 10.0),
];

/// Point set header after the parameters: `[sets, table start]`; the table holds
/// `[points start, points, total count]` per set, a point is `[x, y, cumulative count]`.
const BR_HDR: usize = 20;

fn brush_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    for i in [6, 8, 9, 14, 15, 17, 18, 19] {
        d[i] = ftoi(p[i]) as f64;
    }
    d.extend([0.0; 2]);
    d
}

fn brush_load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
    let s = crate::brush::BrushSettings {
        color_channel: d[6] as i32,
        threshold: d[7],
        invert: d[8] as i32,
        batch_size: d[9] as i32,
        zoom: d[10],
        position: d[11],
        rotation: d[12],
        erosion: d[13],
        seed: d[14] as i32,
        image_max_size: d[15] as i32,
        rnd_brush_seed: d[17] as i32,
        rnd_brush_min_count: d[18] as i32,
        rnd_brush_max_count: d[19] as i32,
        brush_list: text(&res[0]),
        filenames: res[1..7].iter().map(|r| text(r)).collect(),
    };
    let sets = crate::brush::point_sets(&s);
    d[BR_HDR] = sets.len() as f64;
    d[BR_HDR + 1] = d.len() as f64;
    let table = d.len();
    d.resize(table + 3 * sets.len(), 0.0);
    for (i, set) in sets.iter().enumerate() {
        d[table + 3 * i] = d.len() as f64;
        d[table + 3 * i + 1] = set.points.len() as f64;
        d[table + 3 * i + 2] = set.total() as f64;
        for (&(x, y), &c) in set.points.iter().zip(&set.cum) {
            d.extend([x, y, c as f64]);
        }
    }
}

/// `AbstractBrushStrokeWFFunc.discretNoise`
fn discret_noise(x: i32) -> f64 {
    let n = (x << 13) ^ x;
    let v = n.wrapping_mul(n.wrapping_mul(n).wrapping_mul(15731).wrapping_add(789221)).wrapping_add(1376312589);
    (v & 0x7fffffff) as f64 * (1.0 / 2147483647.0)
}

/// `AbstractBrushStrokeWFFunc.transform`: draws brush points on a grid, the brush, rotation
/// and scale per grid cell chosen by noise.
fn brush_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64, inx: f64, iny: f64) {
    // d = [blend, grid_size, grid_deform, grid_brush_rotate, grid_brush_scale, grid_brush_blend,
    //      color_channel, threshold, invert, variation_batch_size, ..(generation params)..,
    //      antialias_radius (16), .., sets, table start, ..]
    const NSG: f64 = 32768.0 * 0.666;
    const NSX: f64 = 32768.0;
    const NSY: f64 = NSX * 0.42 + M_PI;
    let (blend, grid) = (d[0], d[1]);
    let mut gx = inx - fmod(inx, grid);
    let mut gy = iny - fmod(iny, grid);
    let batch = d[9] as i32;
    let generation = ftoi((batch + 1) as f64 * 0.5 * (a.color + discret_noise(ftoi(NSG * (gx + 1235.767 * gy))))).min(batch).max(0) as usize;
    if generation < d[BR_HDR] as usize {
        let t = d[BR_HDR + 1] as usize + 3 * generation;
        let (start, n, total) = (d[t] as usize, d[t + 1] as usize, d[t + 2] as usize);
        let total_blend = blend * (1.0 + (0.5 - discret_noise(ftoi(NSY * (gy + 17.85263 * gx)))) * d[5]);
        if total > 1 && (blend < EPSILON || ctx.random() > total_blend) {
            let k = ctx.random_int(total) as f64;
            let (mut lo, mut hi) = (0, n - 1);
            while lo < hi {
                let mid = (lo + hi) / 2;
                if d[start + 3 * mid + 2] > k {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            let (px, py) = (d[start + 3 * lo], d[start + 3 * lo + 1]);
            let alpha = discret_noise(ftoi(NSX * (gx - 73.547321 * gy))) * d[3];
            let scl = 1.0 + (0.5 - discret_noise(ftoi(NSY * (gy + 31.67891 * gx)))) * d[4];
            let (sa, ca) = alpha.sin_cos();
            let mut rx = (ca * px - sa * py) * scl;
            let mut ry = (sa * px + ca * py) * scl;
            if d[16] > 0.01 {
                let dr = ((d[16] * (-ctx.random().ln()).sqrt()).exp() - 1.0) * 0.001;
                let da = ctx.random() * 2.0 * M_PI;
                rx += dr * da.cos();
                ry += dr * da.sin();
            }
            if grid <= EPSILON {
                v.x = amount * rx;
                v.y = amount * ry;
            } else {
                gx += (0.5 - discret_noise(ftoi(NSX * (gx - 17.236612 * gy)))) * d[2];
                gy += (0.5 - discret_noise(ftoi(NSY * (gy + 76.32561 * gx)))) * d[2];
                v.x = gx + amount * rx;
                v.y = gy + amount * ry;
            }
        }
    }
    v.do_hide = v.x.abs() < EPSILON && v.y.abs() < EPSILON;
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// metaballs3d_wf (Metaballs3DWFFunc)

/// `metaballs3d_wf` data: 18 parameters, bounding box `[xmin, dx, ymin, dy, zmin, dz]`,
/// colormap image, ball count, balls `[x, y, z, radius]` (negative radius = negative ball).
const MB_BOX: usize = 18;
const MB_IMG: usize = MB_BOX + 6;
const MB_N: usize = MB_IMG + IMG;
const MB_BALLS: usize = MB_N + 1;

fn metaballs_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    d[0] = ftoi_limit(p[0], 0, 2);
    for i in [1, 5, 15, 16] {
        d[i] = ftoi(p[i]) as f64;
    }
    d[14] = ftoi(p[14]).max(1) as f64;
    d[17] = ftoi_limit(p[17], 0, 1);
    // updateBoundingBoxSettings
    let sorted = |a: f64, b: f64| if a > b { (b, a) } else { (a, b) };
    let ((x0, x1), (y0, y1), (z0, z1)) = (sorted(p[8], p[9]), sorted(p[10], p[11]), sorted(p[12], p[13]));
    let (dx, dy, dz) = (x1 - x0, y1 - y0, z1 - z0);
    d.extend([x0, dx, y0, dy, z0, dz]);
    d.extend([0.0; IMG]);
    // init: random balls
    let (rmin, rmax) = (p[2].abs().min(p[3].abs()), p[2].abs().max(p[3].abs()));
    let dr = rmax - rmin;
    let border = p[7];
    let (bx0, bdx) = (x0 + border, (x1 - border) - (x0 + border));
    let (by0, bdy) = (y0 + border, (y1 - border) - (y0 + border));
    let (bz0, bdz) = (z0 + border, (z1 - border) - (z0 + border));
    let falloff = |x: f64| 1.0 / (x + 0.63) - 0.60;
    let mut rnd = crate::rng::Marsaglia::new(d[5] as i64);
    let count = (d[1] as i32).max(0);
    d.push(count as f64);
    for _ in 0..count {
        let (mut r, x, y, z) = match d[0] as i32 {
            1 => {
                let r = rmin + dr * falloff(rnd.random()) * 1.42;
                let x = bx0 + bdx * rnd.random();
                let y = (y1 - border) - bdy * falloff(rnd.random());
                (r, x, y, bz0 + bdz * rnd.random())
            }
            2 => {
                let r = rmin + dr * falloff(rnd.random());
                // as in JWildfire, the y extent without the border enters the minimum
                let radius = falloff(rnd.random()) * bdx.min(dy).min(bdz) * 0.85;
                let (sa, ca) = (rnd.random() * 2.0 * M_PI).sin_cos();
                let (sb, cb) = (rnd.random() * M_PI).sin_cos();
                (r, bx0 + bdx * 0.5 + radius * sb * ca, by0 + bdy * 0.5 + radius * sb * sa, bz0 + bdz * 0.5 + radius * cb)
            }
            _ => {
                let r = rmin + dr * rnd.random();
                let x = bx0 + bdx * rnd.random();
                let y = by0 + bdy * rnd.random();
                (r, x, y, bz0 + bdz * rnd.random())
            }
        };
        if rnd.random() < p[4] {
            r = -r;
        }
        d.extend([x, y, z, r]);
    }
    d
}

fn metaballs_apply(d: &[f64], ctx: &mut Ctx, v: &mut Pt, amount: f64) {
    // d = [mb_mode, mb_count, mb_min_radius, mb_max_radius, mb_negative, mb_seed, mb_sharpness,
    //      border_size, xmin, xmax, ymin, ymax, zmin, zmax, max_iter, direct_color, color_mode,
    //      blend_colormap, box (6), colormap (3), balls, ..]
    let (x0, dx, y0, dy, z0, dz) = (d[MB_BOX], d[MB_BOX + 1], d[MB_BOX + 2], d[MB_BOX + 3], d[MB_BOX + 4], d[MB_BOX + 5]);
    let balls = &d[MB_BALLS..MB_BALLS + 4 * d[MB_N] as usize];
    v.do_hide = true;
    let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
    for _ in 0..d[14] as i32 {
        x = x0 + ctx.random() * dx;
        y = y0 + ctx.random() * dy;
        z = z0 + ctx.random() * dz;
        let e: f64 = balls
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| {
                let q = b[3] * b[3] / ((x - b[0]) * (x - b[0]) + (y - b[1]) * (y - b[1]) + (z - b[2]) * (z - b[2]));
                if b[3] < 0.0 { -q } else { q }
            })
            .sum();
        if e > d[6] {
            v.x += amount * x;
            v.y += amount * y;
            v.z += amount * z;
            v.do_hide = false;
            break;
        }
    }
    if v.do_hide || d[15] <= 0.0 {
        return;
    }
    let (fx, fy, fz) = ((x - x0) / dx, (y - y0) / dy, (z - z0) / dz);
    match d[16] as i32 {
        3 => v.color = fx,
        4 => v.color = fy,
        5 => v.color = fz,
        mode @ 0..=2 => {
            if d[MB_IMG] > 0.0 {
                let (iw, ih) = (d[MB_IMG] - 1.0, d[MB_IMG + 1] - 1.0);
                let (u, w) = match mode {
                    0 => (fy, fz),
                    1 => (fz, fx),
                    _ => (fx, fy),
                };
                apply_image_color(d, MB_IMG, d[17] > 0.0, v, (u * iw).clamp(0.0, iw), (ih - w * ih).clamp(0.0, ih));
                v.color = ctx.uv_color_idx(ftoi(v.red), ftoi(v.green), ftoi(v.blue));
            }
        }
        6 => v.color = fx * fy,
        7 => v.color = fy * fz,
        8 => v.color = fz * fx,
        _ => v.color = fx * fy * fz,
    }
    v.color = v.color.clamp(0.0, 1.0);
}

// ---------------------------------------------------------------------------------------------
// kaleidoimg (KaleidoImgFunc)

/// `kaleidoimg` data: 18 parameters, `[cos(Angle), sin(Angle)]`, the 50 rotation angles of
/// mode 0 (precalculated, they divide by `time`), image.
const KI_CS: usize = 18;
const KI_ANG: usize = 20;
const KI_IMG: usize = KI_ANG + 50;

fn kaleido_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    let lim = |v: f64, lo: f64, hi: f64| limit_value(v, lo, hi).trunc();
    d[2] = lim(p[2], 0.0, 1.0);
    d[3] = lim(p[3], 0.0, 1.0);
    for i in [11, 12, 13] {
        d[i] = lim(p[i], 0.0, 1.0);
    }
    d[14] = lim(p[14], -1.0, 13.0);
    d[15] = lim(p[15], 1.0, 50.0);
    d.extend([p[4].cos(), p[4].sin()]);
    let time = p[1];
    d.extend((1..=50).map(|i| 3.14159 / (0.10 + (time * 0.0005 + i as f64 * 0.5000001).sin() * 0.4999 + 0.5 + (10. / time) + time.sin() / 100.)));
    d.extend([0.0; IMG]);
    d
}

/// `G.texture`: blended pixel (0..255 RGB) at `uv` (0..1, tiled).
fn kaleido_texture(d: &[f64], u: f64, v: f64) -> [f64; 3] {
    let (w, h) = (d[KI_IMG] as i32, d[KI_IMG + 1] as i32);
    let x = (u - 0.5) * (w - 2) as f64;
    let y = (v - 0.5) * (h - 2) as f64;
    let ix = wrap_tile(x.trunc() as i32, w);
    let iy = wrap_tile(y.trunc() as i32, h);
    if ix >= 0 && ix < w - 1 && iy >= 0 && iy < h - 1 {
        let px = |dx: i32, dy: i32| pixel(d, KI_IMG, (ix + dx) as i64, (iy + dy) as i64);
        let (lu, ru, lb, rb) = (px(0, 0), px(1, 0), px(0, 1), px(1, 1));
        let (fu, fv) = (frac(x), frac(y));
        [0, 1, 2].map(|k| blerp(lu[k], ru[k], lb[k], rb[k], fu, fv))
    } else {
        [0.0; 3]
    }
}

type V2 = (f64, f64);

fn len2(p: V2) -> f64 {
    (p.0 * p.0 + p.1 * p.1).sqrt()
}

fn rot2(p: V2, a: f64) -> V2 {
    let (s, c) = a.sin_cos();
    (p.0 * c - p.1 * s, p.0 * s + p.1 * c)
}

fn gmod(x: f64, y: f64) -> f64 {
    x - y * (x / y).floor()
}

fn smoothstep1(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn kaleido_uv_mode(d: &[f64], mode: i32, uv: V2) -> V2 {
    let (time, nr, pa, pb) = (d[1], d[15] as i32, d[16], d[17]);
    match mode {
        1 => {
            let (s, c) = (time * 0.1).sin_cos();
            let mut uv = uv;
            for _ in 0..32 {
                let a = (uv.0.abs() - 0.25, uv.1.abs() - 0.25);
                uv = (a.0 * c + a.1 * s, a.1 * c - a.0 * s);
            }
            uv
        }
        2 => {
            let t = time * 0.3 + 10.0;
            let (s, c) = (t * 0.1).sin_cos();
            let q = t * 0.2 / 6.2831853;
            let mut uv = uv;
            for _ in 0..30 {
                let mut a = glsl_atan(uv.0, uv.1) * q;
                a = ((a * 0.5 + 0.5).fract_glsl() * 2.0 - 1.0).abs() / q;
                let l = len2(uv);
                let p = (a.sin() * l - 0.7, a.cos() * l - 0.7);
                uv = (p.0 * c + p.1 * s, p.1 * c - p.0 * s);
            }
            uv
        }
        3 => {
            let t = nr as f64 * glsl_atan(uv.1, uv.0);
            let r3 = len2(uv).powf(3.0);
            ((1.0 + t.cos() * r3) / 2.0, (1.0 + t.sin() * r3) / 2.0)
        }
        4 => {
            // 3D mirror folds around the axis (0, 0, 1)
            let rot3 = |v: [f64; 3], a: f64| {
                let (s, c) = a.sin_cos();
                [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]]
            };
            let mut huv = rot3([uv.0, uv.1, 0.0], time * 0.2);
            for i in 0..nr {
                let ar = 3.1415 * 2.0 / nr as f64 * i as f64;
                let t = rot3([huv[0], huv[1], huv[2] - 1.0], -ar);
                if t[1] < 0.0 {
                    let inv = rot3([t[0], -t[1], t[2]], ar);
                    huv = [inv[0], inv[1], inv[2] + 1.0];
                }
            }
            (huv[0], huv[1])
        }
        5 => {
            let a: f64 = 5. / 6. * 3.1415;
            let mut uv = (uv.0.abs(), uv.1 + a.tan() * 0.5);
            let n = (a.sin(), a.cos());
            let dd = (uv.0 - 0.5) * n.0 + uv.1 * n.1;
            uv = (uv.0 - n.0 * dd.max(0.0) * 2.0, uv.1 - n.1 * dd.max(0.0) * 2.0);
            let mut scale = 1.0;
            let b: f64 = 2. / 3. * 3.1415;
            let n = (b.sin(), b.cos());
            uv.0 += 1.5 / 3.0;
            for _ in 0..4 {
                uv = (uv.0 * 3.0, uv.1 * 3.0);
                scale *= 3.0;
                uv.0 = (uv.0 - 1.5).abs() - 0.5;
                let m = 2.0 * (uv.0 * n.0 + uv.1 * n.1).min(0.0);
                uv = (uv.0 - n.0 * m, uv.1 - n.1 * m);
            }
            (uv.0 / scale, uv.1 / scale)
        }
        6 => {
            let t2 = 2f64.powf(-len2(uv));
            (5.0 * (glsl_atan2(uv.0, uv.1) / 6.28 + t2), 8.0 * (glsl_atan2(uv.1, uv.0) / 6.28 + t2))
        }
        9 => {
            let l = len2(uv);
            let f = 0.7 * l.ln() / l;
            (uv.0 + uv.0 * f, uv.1 + uv.1 * f)
        }
        10 => {
            let r = len2(uv);
            let rn = pa * r.powf(pb);
            // as in JWildfire: y divided by rn
            (rn * uv.0 / r, rn * uv.1 / rn)
        }
        11 => {
            let w = |k: f64| (time * k).sin() * 0.5 + 0.5;
            let (a, b, c, dd) = (w(1.0), w(1.5), w(2.0), w(2.5));
            let mix = |p: f64, q: f64, t: f64| p * (1.0 - t) + q * t;
            let (y0, y1, x0, x1) = (mix(a, b, uv.0), mix(c, dd, uv.0), mix(a, c, uv.1), mix(b, dd, uv.1));
            let hermite = |m0: f64, m1: f64, x: f64| {
                let (x2, x3) = (x * x, x * x * x);
                (3. * x2 - 2. * x3) + m0 * (x3 - 2. * x2 + x) + m1 * (x3 - x2)
            };
            (hermite(3. * x0, 3. * x1, uv.0), 1.0 - hermite(3. * y0, 3. * y1, uv.1))
        }
        12 => {
            let r = (len2(uv) - time * 0.05) * pa;
            let r = (r - r.floor()) / pb;
            ((uv.0 * 2.0 - 1.0) * r * 0.5 + 0.5, (uv.1 * 2.0 - 1.0) * r * 0.5 + 0.5)
        }
        13 => {
            let ad = len2((uv.0 + pa / 2.0, uv.1));
            let bd = len2((uv.0 - pa / 2.0, uv.1));
            ((((ad - time / 20.0) * 10.).sin() + 1.0) / 2.0, (((bd - time / 20.0) * 10.).sin() + 1.0) / 2.0)
        }
        _ => uv,
    }
}

trait FractGlsl {
    fn fract_glsl(self) -> f64;
}
impl FractGlsl for f64 {
    /// `G.fract`: `x - floor(x)`
    fn fract_glsl(self) -> f64 {
        self - self.floor()
    }
}

/// `KaleidoImgFunc.transform`
fn kaleido_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    // d = [zoom, time, blendcolor, dc_color, Angle, scaleX, scaleY, scale_z, offsetX, offsetY,
    //      offset_z, tile_x, tile_y, reset_z, type, nr, a, b, cos, sin, mode 0 angles (50), image (3), ..]
    let x = ctx.random() - 0.5;
    let y = ctx.random() - 0.5;
    let uv = (x * d[0], y * d[0]);
    let time = d[1];
    let (c, s) = (d[KI_CS], d[KI_CS + 1]);
    // rotate (uv * mat2(c, s, -s, c)), scale, shift
    let place = |p: V2| ((c * p.0 + s * p.1) * d[5] + d[8], (-s * p.0 + c * p.1) * d[6] + d[9]);
    let tex = |p: V2| kaleido_texture(d, p.0, p.1);
    let pixel = match d[14] as i32 {
        -1 => tex(place(uv)),
        0 => {
            let fold = -0.15;
            let mut p = (uv.0 * 0.182, uv.1 * 0.182);
            let x0 = p.1;
            p = ((gmod(p.0, 4.0) - 2.0).abs(), (gmod(p.1, 4.0) - 2.0).abs());
            for i in (1..=d[15] as usize).rev() {
                p = ((p.0 - fold).abs() + fold, (p.1 - fold).abs() + fold);
                p = (p.0 * 1.25 - 1.5, p.1 * 1.25 - 1.5);
                p = rot2(p, d[KI_ANG + i - 1]);
            }
            let i = x0 * x0 + glsl_atan2(p.1, p.0) + time * 0.02;
            let mut h = (i * 4.0).floor() / 8.0 + 1.107;
            h += smoothstep1(-0.1, 0.8, gmod(i * 2.0 / 5.0, 1.0 / 4.0) * 900.0) / 0.010 - 0.5;
            // hsv(h, 1, value) and dbl2int
            let value = smoothstep1(-3.0, 3.0, len2(uv));
            [3.0, 2.0, 1.0].map(|k: f64| {
                let t = (((k + h) / 3.0).fract_glsl() * 6.0 - 3.0).abs() - 1.0;
                (t.clamp(0.0, 1.0) * value * 256.0).floor().clamp(0.0, 255.0)
            })
        }
        5 => {
            let p = place(kaleido_uv_mode(d, 5, uv));
            tex((p.0 * 2.0 + time * 0.05, p.1 * 2.0 + time * 0.05))
        }
        7 => {
            let p = place(uv);
            let r = len2(p);
            let c = tex((0.3 / r + 0.2 * time, glsl_atan2(p.1, p.0) / 3.1415927));
            c.map(|v| v * r)
        }
        8 => {
            // distortUV chain; Java aliases the uv vector, so the first call moves it by
            // time * 0.25 before it is used anywhere
            let ts = time * 0.25;
            let p = place(uv);
            let moved = (p.0 + ts, p.1 + ts);
            let distort = |q: V2, n: V2| {
                let t = tex((n.0 * 0.01, n.1 * 0.01));
                (q.0 + (t[0] * 2.0 - 1.0) * 0.01, q.1 + (t[1] * 2.0 - 1.0) * 0.01)
            };
            let mut q = distort(moved, moved);
            for o in [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.15, 0.25, 0.35, 0.45, 0.55, 0.65, 0.75, 0.85, 0.95] {
                q = distort(q, (moved.0 + o + ts, moved.1 + o + ts));
            }
            tex((moved.0 + q.0, moved.1 + q.1))
        }
        t @ 1..=6 => tex(place(kaleido_uv_mode(d, t, uv))),
        t => tex(kaleido_uv_mode(d, t, place(uv))),
    };
    v.x = amount * x;
    v.y = amount * y;
    ctx.pass_z(a, v, amount);
    let mut dz = d[10];
    if d[7].abs() > EPSILON {
        dz += d[7] * (0.299 * pixel[0] + 0.588 * pixel[1] + 0.113 * pixel[2]) / 255.0;
    }
    if d[13] as i32 != 0 {
        v.z = dz;
    } else {
        v.z += dz;
    }
    if d[3] as i32 == 0 {
        v.rgb_color = true;
        v.red = pixel[0];
        v.green = pixel[1];
        v.blue = pixel[2];
    } else {
        v.rgb_color = false;
        v.color = ctx.uv_color_idx(ftoi(pixel[0]), ftoi(pixel[1]), ftoi(pixel[2]));
    }
}

// ---------------------------------------------------------------------------------------------
// knots3D (Knots3DFunc), sattractor3D (Strange3DFunc): tubes around formula curves, sampled like
// the OBJ meshes

const fn curve_params<const H: usize, const N: usize>(head: [ParamDef; H], scale: f64) -> [ParamDef; N] {
    let mut out = [ParamDef::new("", 0.0); N];
    let mut i = 0;
    while i < H {
        out[i] = head[i];
        i += 1;
    }
    let names = ["param_a", "param_b", "param_c", "param_d", "param_e", "param_f", "param_g", "param_h"];
    let mut k = 0;
    while k < 8 {
        out[i] = ParamDef::new(names[k], 1.0);
        i += 1;
        k += 1;
    }
    let mut k = 0;
    while k < MESH_PARAMS.len() {
        out[i] = MESH_PARAMS[k];
        if k < 3 {
            out[i].default = scale;
        }
        i += 1;
        k += 1;
    }
    out
}

static KNOTS_PARAMS: [ParamDef; 26] = curve_params(
    [
        ParamDef::int("presetId", 0.0, -1.0, 200.0),
        ParamDef::int("steps", 1500.0, 3.0, 20000.0),
        ParamDef::range("radius", 2.0, 0.0, 50.0),
        ParamDef::int("facets", 4.0, 2.0, 24.0),
    ],
    0.02,
);

static SATTRACTOR_PARAMS: [ParamDef; 31] = curve_params(
    [
        ParamDef::int("presetId", 0.0, -1.0, 200.0),
        ParamDef::int("steps", 1.0, 1.0, 50.0),
        ParamDef::range("radius", 0.005, 0.0, 0.1),
        ParamDef::range("stepTime", 0.05, 0.0, 0.5),
        ParamDef::int("facets", 3.0, 1.0, 24.0),
        ParamDef::new("start_x", 0.1),
        ParamDef::new("start_y", 1.0),
        ParamDef::new("start_z", 1.0),
        ParamDef::int("warmup", 1000.0, 0.0, 100000.0),
    ],
    1.0,
);

/// Compiles the three curve formulas (`param_a` .. `param_h` from `d[pa..]`).
fn curve_formulas(res: &[Vec<u8>], vars: &[&str], d: &[f64], pa: usize) -> Option<[crate::formula::Program; 3]> {
    let mut consts = std::collections::HashMap::new();
    consts.insert("pi".to_string(), M_PI);
    for (k, name) in ["param_a", "param_b", "param_c", "param_d", "param_e", "param_f", "param_g", "param_h"].iter().enumerate() {
        consts.insert(name.to_string(), d[pa + k]);
    }
    let f = |i: usize| crate::formula::compile(&text(&res[i]), vars, &consts).ok();
    Some([f(0)?, f(1)?, f(2)?])
}

fn curve_key(name: &str, res: &[Vec<u8>], d: &[f64], n: usize) -> String {
    format!("{name}#{}#{}#{}#{:?}", text(&res[0]), text(&res[1]), text(&res[2]), &d[1..n])
}

fn knots_load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
    let p = 12;
    let o = p + MESH_PARAMS.len();
    let mesh = match curve_formulas(res, &["t"], d, 4) {
        Some(f) => {
            let (steps, radius, facets) = (d[1] as i32, d[2], d[3] as i32);
            mesh::cached(curve_key("knots3D", res, d, p), || {
                let pt = |step: i32| {
                    let t = 2.0 * M_PI * step as f64 / steps as f64;
                    [0, 1, 2].map(|k| crate::formula::eval(&f[k].code, &[t]))
                };
                crate::curves::knot(pt, steps, radius, facets)
            })
        }
        None => Arc::new(Mesh::default()),
    };
    pack_mesh(d, o, &mesh);
}

/// `terrain3D` parameters: roughness, z_exaggeration, size, seed, dc, then the mesh parameters.
static TERRAIN_PARAMS: [ParamDef; 19] = {
    let mut p = [ParamDef::new("roughness", 0.5); 19];
    p[1] = ParamDef::new("z_exaggeration", 0.7);
    p[2] = ParamDef::int("Size (N PowersOf 2)", 5.0, 1.0, 8.0);
    // JWildfire draws a random default seed (0..999)
    p[3] = ParamDef::int("seed", 42.0, 0.0, 10000.0);
    p[4] = ParamDef::int("dc", 1.0, 0.0, 1.0);
    let mut i = 0;
    while i < 14 {
        p[5 + i] = MESH_PARAMS[i];
        i += 1;
    }
    p
};

/// `FracTerrain3DFunc.FractalTerrain`: diamond-square height field as a mesh.
fn terrain_mesh(lod: i32, roughness: f64, seed: i32, exaggeration: f64) -> Mesh {
    let n = 1usize << lod;
    let mut rng = crate::rng::JavaRandom::new(seed as i64);
    let mut rnd = move || 2.0 * rng.next_double() - 1.0;
    let mut t = vec![vec![0.0f64; n + 1]; n + 1];
    t[0][0] = rnd();
    t[0][n] = rnd();
    t[n][n] = rnd();
    t[n][0] = rnd();
    let mut rough = roughness;
    for i in 0..lod {
        let r = 1i64 << (lod - i);
        let s = r >> 1;
        let div = n as i64;
        for j in (0..div).step_by(r as usize) {
            for k in (0..div).step_by(r as usize) {
                // diamond
                if r > 1 {
                    let (x, y, side) = (j as usize, k as usize, r as usize);
                    let half = side / 2;
                    let avg = (t[x][y] + t[x + side][y] + t[x + side][y + side] + t[x][y + side]) * 0.25;
                    t[x + half][y + half] = avg + rnd() * rough;
                }
            }
        }
        if s > 0 {
            let mut j = 0;
            while j <= div {
                let mut k = (j + s) % r;
                while k <= div {
                    // square
                    let (x, y, side) = (j - s, k - s, r);
                    let half = side / 2;
                    let (mut avg, mut sum) = (0.0, 0.0);
                    if x >= 0 {
                        avg += t[x as usize][(y + half) as usize];
                        sum += 1.0;
                    }
                    if y >= 0 {
                        avg += t[(x + half) as usize][y as usize];
                        sum += 1.0;
                    }
                    if x + side <= div {
                        avg += t[(x + side) as usize][(y + half) as usize];
                        sum += 1.0;
                    }
                    if y + side <= div {
                        avg += t[(x + half) as usize][(y + side) as usize];
                        sum += 1.0;
                    }
                    t[(x + half) as usize][(y + half) as usize] = avg / sum + rnd() * rough;
                    k += r;
                }
                j += s;
            }
        }
        rough *= roughness;
    }
    let (mut min, mut max) = (t[0][0], t[0][0]);
    for row in &t {
        for &h in row {
            if h < min {
                min = h;
            } else if h > max {
                max = h;
            }
        }
    }
    let mut m = Mesh::default();
    for i in 0..=n {
        for j in 0..=n {
            let (x, z) = (i as f64 / n as f64, j as f64 / n as f64);
            let alt = (t[(x * n as f64) as usize][(z * n as f64) as usize] - min) / (max - min);
            m.add_vertex(x - 0.5, alt * exaggeration, z - 0.5, None);
        }
    }
    let w = (n + 1) as u32;
    for i in 0..n as u32 {
        for j in 0..n as u32 {
            m.add_face(w * i + j, w * (i + 1) + j, w * i + j + 1);
            m.add_face(w * (i + 1) + j, w * (i + 1) + j + 1, w * i + j + 1);
        }
    }
    m
}

fn terrain_load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
    let p = 5;
    let o = p + MESH_PARAMS.len();
    load_image(d, o, &res[0]);
    load_image(d, o + M_DISPL, &res[1]);
    let (lod, roughness, seed, exaggeration) = (d[2] as i32, d[0], d[3] as i32, d[1]);
    let (level, passes, lambda, mu) = (d[p + 6] as i32, d[p + 7] as i32, d[p + 8], d[p + 9]);
    let key = format!("terrain3D#{lod}#{roughness}#{seed}#{exaggeration}#{level}#{passes}#{lambda}#{mu}");
    let mesh = mesh::cached(key, || {
        let mut m = terrain_mesh(lod, roughness, seed, exaggeration);
        for _ in 0..level.min(2) {
            m = m.interpolate();
            m.taubin_smooth(passes, lambda, mu);
        }
        m
    });
    pack_mesh(d, o, &mesh);
}

fn sattractor_load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
    let p = 17;
    let o = p + MESH_PARAMS.len();
    let mesh = match curve_formulas(res, &["x", "y", "z", "delta_t"], d, 9) {
        Some(f) => {
            let count = (d[1] as i32).wrapping_mul(1000);
            let (radius, step_time, facets) = (d[2], d[3], d[4] as i32);
            let (start, warmup) = ([d[5], d[6], d[7]], d[8] as i32);
            mesh::cached(curve_key("sattractor3D", res, d, p), || {
                let eval = |k: usize, q: [f64; 3], dt: f64| q[k] + crate::formula::eval(&f[k].code, &[q[0], q[1], q[2], dt]) * dt;
                crate::curves::attractor(eval, start, count, warmup, step_time, radius, facets)
            })
        }
        None => Arc::new(Mesh::default()),
    };
    pack_mesh(d, o, &mesh);
}

// ---------------------------------------------------------------------------------------------
// svg_wf (SVGWFFunc), text_wf (TextWFFunc): point sets of rendered shapes

/// Point header after the parameters: `[count, start]`; a point is `[x, y, packed rgb]`.
fn push_shape_points(d: &mut Vec<f64>, o: usize, pts: &[crate::shapes::ShapePoint]) {
    d[o] = pts.len() as f64;
    d[o + 1] = d.len() as f64;
    d.reserve(3 * pts.len());
    for p in pts {
        d.extend([p.x, p.y, ((p.rgb[0] as u32) << 16 | (p.rgb[1] as u32) << 8 | p.rgb[2] as u32) as f64]);
    }
}

/// Picks a point and adds the antialiasing jitter of JWildfire's shape variations; `None` if
/// the set is empty (then JWildfire adds a random black point).
fn shape_point(d: &[f64], o: usize, aa_radius: f64, ctx: &mut Ctx) -> Option<(f64, f64, u32)> {
    let n = d[o] as usize;
    if n <= 1 {
        return None;
    }
    let p = d[o + 1] as usize + 3 * ctx.random_int(n);
    let (mut x, mut y) = (d[p], d[p + 1]);
    if aa_radius > 0.01 {
        let dr = ((aa_radius * (-ctx.random().ln()).sqrt()).exp() - 1.0) * 0.001;
        let da = ctx.random() * 2.0 * M_PI;
        x += dr * da.cos();
        y += dr * da.sin();
    }
    Some((x, y, d[p + 2] as u32))
}

fn random_black(ctx: &mut Ctx, v: &mut Pt) {
    v.x += ctx.random();
    v.y += ctx.random();
    v.rgb_color = true;
    v.red = 0.0;
    v.green = 0.0;
    v.blue = 0.0;
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
    VarDef {
        name: "brush_stroke_wf",
        params: &BRUSH_PARAMS,
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x, y) = (a.x, a.y);
            brush_apply(d, ctx, a, v, amount, x, y)
        },
        prepare: Some(|p, _| brush_prepare(p)),
        wgsl: "v = res_brush(base, a, v, AMOUNT, a.x, a.y, PRESERVE_Z);",
    },
    VarDef {
        name: "post_brush_stroke_wf",
        params: &BRUSH_PARAMS,
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x, y) = (v.x, v.y);
            brush_apply(d, ctx, a, v, amount, x, y)
        },
        prepare: Some(|p, _| brush_prepare(p)),
        wgsl: "v = res_brush(base, a, v, AMOUNT, v.x, v.y, PRESERVE_Z);",
    },
    VarDef {
        name: "metaballs3d_wf",
        params: &[
            ParamDef::int("mb_mode", 0.0, 0.0, 2.0),
            ParamDef::int("mb_count", 64.0, 1.0, 500.0),
            ParamDef::range("mb_min_radius", 0.35, 0.0, 2.0),
            ParamDef::range("mb_max_radius", 0.75, 0.0, 2.0),
            ParamDef::range("mb_negative", 0.3, 0.0, 1.0),
            ParamDef::int("mb_seed", 12345.0, 0.0, 1234567.0),
            ParamDef::range("mb_sharpness", 2.75, 0.0, 10.0),
            ParamDef::range("border_size", 0.42, 0.0, 2.0),
            ParamDef::new("xmin", -M_PI),
            ParamDef::new("xmax", M_PI),
            ParamDef::new("ymin", -M_PI),
            ParamDef::new("ymax", M_PI),
            ParamDef::new("zmin", -M_PI),
            ParamDef::new("zmax", M_PI),
            ParamDef::int("max_iter", 160.0, 1.0, 1000.0),
            ParamDef::int("direct_color", 1.0, 0.0, 1.0),
            ParamDef::int("color_mode", 9.0, 0.0, 9.0),
            ParamDef::int("blend_colormap", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| metaballs_apply(d, ctx, v, amount),
        prepare: Some(|p, _| metaballs_prepare(p)),
        wgsl: "v = res_metaballs(base, v, AMOUNT);",
    },
    VarDef {
        name: "kaleidoimg",
        params: &[
            ParamDef::new("zoom", 2.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("blendcolor", 0.0, 0.0, 1.0),
            ParamDef::int("dc_color", 0.0, 0.0, 1.0),
            ParamDef::range("Angle", 0.0, -M_PI, M_PI),
            ParamDef::new("scaleX", 1.0),
            ParamDef::new("scaleY", 1.0),
            ParamDef::new("scale_z", 0.0),
            ParamDef::new("offsetX", 0.0),
            ParamDef::new("offsetY", 0.0),
            ParamDef::new("offset_z", 0.0),
            ParamDef::int("tile_x", 1.0, 0.0, 1.0),
            ParamDef::int("tile_y", 1.0, 0.0, 1.0),
            ParamDef::int("reset_z", 1.0, 0.0, 1.0),
            ParamDef::int("type", 0.0, -1.0, 13.0),
            ParamDef::int("nr", 12.0, 1.0, 50.0),
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| kaleido_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| kaleido_prepare(p)),
        wgsl: "fn kaleidoimg_tex(base: u32, u: f32, v: f32) -> vec3<f32> {
  let w = i32(prm[base + 70u]);
  let h = i32(prm[base + 71u]);
  let x = (u - 0.5) * f32(w - 2);
  let y = (v - 0.5) * f32(h - 2);
  let ix = res_wrap_tile(i32(trunc(x)), w);
  let iy = res_wrap_tile(i32(trunc(y)), h);
  if (ix >= 0 && ix < w - 1 && iy >= 0 && iy < h - 1) {
    let fu = x - trunc(x);
    let fv = y - trunc(y);
    let top = mix(res_pixel(base, 70u, ix, iy), res_pixel(base, 70u, ix + 1, iy), fu);
    let bottom = mix(res_pixel(base, 70u, ix, iy + 1), res_pixel(base, 70u, ix + 1, iy + 1), fu);
    return mix(top, bottom, fv);
  }
  return vec3<f32>(0.0);
}
fn kaleidoimg_rot3(v: vec3<f32>, a: f32) -> vec3<f32> {
  return vec3<f32>(v.x * cos(a) - v.y * sin(a), v.x * sin(a) + v.y * cos(a), v.z);
}
fn kaleidoimg_mode(base: u32, mode: i32, uv_in: vec2<f32>) -> vec2<f32> {
  var uv = uv_in;
  let time = prm[base + 1u];
  let nr = i32(prm[base + 15u]);
  let pa = prm[base + 16u];
  let pb = prm[base + 17u];
  if (mode == 1) {
    let c = cos(time * 0.1);
    let s = sin(time * 0.1);
    for (var i = 0; i < 32; i++) {
      let a = abs(uv) - 0.25;
      uv = vec2<f32>(a.x * c + a.y * s, a.y * c - a.x * s);
    }
  } else if (mode == 2) {
    let t = time * 0.3 + 10.0;
    let c = cos(t * 0.1);
    let s = sin(t * 0.1);
    let q = t * 0.2 / 6.2831853;
    for (var i = 0; i < 30; i++) {
      var a = glsl_atan(uv.x, uv.y) * q;
      a = abs(fract(a * 0.5 + 0.5) * 2.0 - 1.0) / q;
      let p = vec2<f32>(sin(a), cos(a)) * length(uv) - 0.7;
      uv = vec2<f32>(p.x * c + p.y * s, p.y * c - p.x * s);
    }
  } else if (mode == 3) {
    let t = f32(nr) * glsl_atan(uv.y, uv.x);
    uv = (vec2<f32>(1.0) + vec2<f32>(cos(t), sin(t)) * pow(length(uv), 3.0)) / 2.0;
  } else if (mode == 4) {
    var huv = kaleidoimg_rot3(vec3<f32>(uv, 0.0), time * 0.2);
    for (var i = 0; i < nr; i++) {
      let ar = 3.1415 * 2.0 / f32(nr) * f32(i);
      let t = kaleidoimg_rot3(huv - vec3<f32>(0.0, 0.0, 1.0), -ar);
      if (t.y < 0.0) {
        huv = kaleidoimg_rot3(vec3<f32>(t.x, -t.y, t.z), ar) + vec3<f32>(0.0, 0.0, 1.0);
      }
    }
    uv = huv.xy;
  } else if (mode == 5) {
    let a = 5.0 / 6.0 * 3.1415;
    uv = vec2<f32>(abs(uv.x), uv.y + tan(a) * 0.5);
    var n = vec2<f32>(sin(a), cos(a));
    let dd = dot(uv - vec2<f32>(0.5, 0.0), n);
    uv = uv - n * max(dd, 0.0) * 2.0;
    var scale = 1.0;
    n = vec2<f32>(sin(2.0 / 3.0 * 3.1415), cos(2.0 / 3.0 * 3.1415));
    uv.x += 1.5 / 3.0;
    for (var i = 0; i < 4; i++) {
      uv *= 3.0;
      scale *= 3.0;
      uv.x = abs(uv.x - 1.5) - 0.5;
      uv = uv - n * (2.0 * min(dot(uv, n), 0.0));
    }
    uv /= scale;
  } else if (mode == 6) {
    let t2 = pow(2.0, -length(uv));
    uv = vec2<f32>(5.0 * (glsl_atan2(uv.x, uv.y) / 6.28 + t2), 8.0 * (glsl_atan2(uv.y, uv.x) / 6.28 + t2));
  } else if (mode == 9) {
    let l = length(uv);
    uv = uv + uv * (0.7 * log(l) / l);
  } else if (mode == 10) {
    let r = length(uv);
    let rn = pa * pow(r, pb);
    uv = vec2<f32>(rn * uv.x / r, rn * uv.y / rn);
  } else if (mode == 11) {
    let a = sin(time) * 0.5 + 0.5;
    let b = sin(time * 1.5) * 0.5 + 0.5;
    let c = sin(time * 2.0) * 0.5 + 0.5;
    let d = sin(time * 2.5) * 0.5 + 0.5;
    let y0 = mix(a, b, uv.x);
    let y1 = mix(c, d, uv.x);
    let x0 = mix(a, c, uv.y);
    let x1 = mix(b, d, uv.y);
    let hx = uv.x;
    let hy = uv.y;
    let ux = (3.0 * hx * hx - 2.0 * hx * hx * hx) + 3.0 * x0 * (hx * hx * hx - 2.0 * hx * hx + hx) + 3.0 * x1 * (hx * hx * hx - hx * hx);
    let uy = (3.0 * hy * hy - 2.0 * hy * hy * hy) + 3.0 * y0 * (hy * hy * hy - 2.0 * hy * hy + hy) + 3.0 * y1 * (hy * hy * hy - hy * hy);
    uv = vec2<f32>(ux, 1.0 - uy);
  } else if (mode == 12) {
    let r = fract((length(uv) - time * 0.05) * pa) / pb;
    uv = (uv * 2.0 - 1.0) * r * 0.5 + 0.5;
  } else if (mode == 13) {
    let ad = length(uv + vec2<f32>(pa / 2.0, 0.0));
    let bd = length(uv - vec2<f32>(pa / 2.0, 0.0));
    uv = vec2<f32>((sin((ad - time / 20.0) * 10.0) + 1.0) / 2.0, (sin((bd - time / 20.0) * 10.0) + 1.0) / 2.0);
  }
  return uv;
}
fn kaleidoimg_place(base: u32, p: vec2<f32>) -> vec2<f32> {
  let c = prm[base + 18u];
  let s = prm[base + 19u];
  return vec2<f32>((c * p.x + s * p.y) * prm[base + 5u] + prm[base + 8u], (-s * p.x + c * p.y) * prm[base + 6u] + prm[base + 9u]);
}
@body
let x = rnd() - 0.5;
let y = rnd() - 0.5;
let uv = vec2<f32>(x, y) * D(0);
let time = D(1);
let kind = i32(D(14));
var pixel = vec3<f32>(0.0);
if (kind == -1) {
  let p = kaleidoimg_place(base, uv);
  pixel = kaleidoimg_tex(base, p.x, p.y);
} else if (kind == 0) {
  var p = abs(glsl_mod2(uv * 0.182, vec2<f32>(4.0)) - 2.0);
  let x0 = uv.y * 0.182;
  for (var i = i32(D(15)); i > 0; i--) {
    p = abs(p + 0.15) - 0.15;
    p = p * 1.25 - 1.5;
    let ang = prm[base + 19u + u32(i)];
    p = vec2<f32>(p.x * cos(ang) - p.y * sin(ang), p.x * sin(ang) + p.y * cos(ang));
  }
  let ii = x0 * x0 + glsl_atan2(p.y, p.x) + time * 0.02;
  var h = floor(ii * 4.0) / 8.0 + 1.107;
  h += glsl_smoothstep(-0.1, 0.8, glsl_mod(ii * 2.0 / 5.0, 1.0 / 4.0) * 900.0) / 0.010 - 0.5;
  let value = glsl_smoothstep(-3.0, 3.0, length(uv));
  let t = clamp(abs(fract((vec3<f32>(3.0, 2.0, 1.0) + h) / 3.0) * 6.0 - 3.0) - 1.0, vec3<f32>(0.0), vec3<f32>(1.0));
  pixel = clamp(floor(t * value * 256.0), vec3<f32>(0.0), vec3<f32>(255.0));
} else if (kind == 5) {
  let p = kaleidoimg_place(base, kaleidoimg_mode(base, 5, uv)) * 2.0 + time * 0.05;
  pixel = kaleidoimg_tex(base, p.x, p.y);
} else if (kind == 7) {
  let p = kaleidoimg_place(base, uv);
  let r = length(p);
  pixel = kaleidoimg_tex(base, 0.3 / r + 0.2 * time, glsl_atan2(p.y, p.x) / 3.1415927) * r;
} else if (kind == 8) {
  let ts = time * 0.25;
  let moved = kaleidoimg_place(base, uv) + ts;
  var q = moved;
  for (var k = 0; k < 19; k++) {
    var o = 0.0;
    if (k > 0 && k < 10) { o = f32(k) * 0.1; }
    else if (k >= 10) { o = f32(k - 10) * 0.1 + 0.15; }
    var n = moved;
    if (k > 0) { n = moved + o + ts; }
    let t = kaleidoimg_tex(base, n.x * 0.01, n.y * 0.01);
    q = q + (t.xy * 2.0 - 1.0) * 0.01;
  }
  let p = moved + q;
  pixel = kaleidoimg_tex(base, p.x, p.y);
} else if (kind >= 1 && kind <= 6) {
  let p = kaleidoimg_place(base, kaleidoimg_mode(base, kind, uv));
  pixel = kaleidoimg_tex(base, p.x, p.y);
} else {
  let p = kaleidoimg_mode(base, kind, kaleidoimg_place(base, uv));
  pixel = kaleidoimg_tex(base, p.x, p.y);
}
v.x = AMOUNT * x;
v.y = AMOUNT * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
var dz = D(10);
if (abs(D(7)) > EPSILON) { dz += D(7) * (0.299 * pixel.x + 0.588 * pixel.y + 0.113 * pixel.z) / 255.0; }
if (i32(D(13)) != 0) { v.z = dz; } else { v.z += dz; }
if (i32(D(3)) == 0) {
  v.rgbc = 1.0; v.r = pixel.x; v.g = pixel.y; v.b = pixel.z;
} else {
  v.rgbc = 0.0;
  v.color = res_uv_color_idx(pixel);
}",
    },
    VarDef {
        name: "knots3D",
        params: &KNOTS_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| mesh_apply(d, 12, ctx, v, amount),
        prepare: Some(|p, _| {
            let mut d = mesh_prepare(p, 12);
            d[0] = ftoi(p[0]) as f64;
            d[1] = p[1].trunc();
            d[3] = p[3].trunc();
            d
        }),
        wgsl: "v = res_mesh(base, 12u, v, AMOUNT);",
    },
    VarDef {
        name: "terrain3D",
        params: &TERRAIN_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| {
            mesh_apply(d, 5, ctx, v, amount);
            if d[4] as i32 == 1 {
                v.color = v.y % 1.0;
            }
        },
        prepare: Some(|p, _| {
            let mut d = mesh_prepare(p, 5);
            d[2] = (limit_value(p[2], 1.0, 8.0) as i32) as f64;
            d[3] = ftoi(p[3]) as f64;
            d[4] = (limit_value(p[4], 0.0, 1.0) as i32) as f64;
            d
        }),
        wgsl: "v = res_mesh(base, 5u, v, AMOUNT);
if (i32(D(4)) == 1) { v.color = v.y % 1.0; }",
    },
    VarDef {
        name: "sattractor3D",
        params: &SATTRACTOR_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| mesh_apply(d, 17, ctx, v, amount),
        prepare: Some(|p, _| {
            let mut d = mesh_prepare(p, 17);
            d[0] = ftoi(p[0]) as f64;
            for i in [1, 4, 8] {
                d[i] = p[i].trunc();
            }
            d
        }),
        wgsl: "v = res_mesh(base, 17u, v, AMOUNT);",
    },
    VarDef {
        name: "svg_wf",
        params: &[
            ParamDef::range("antialias_radius", 0.5, 0.0, 2.0),
            ParamDef::range("resolution_multiplier", 2.0, 0.1, 8.0),
            ParamDef::int("true_color", 1.0, 0.0, 1.0),
            ParamDef::int("pre_antialias", 1.0, 0.0, 1.0),
            ParamDef::new("scale_x", 1.0),
            ParamDef::new("scale_y", 1.0),
            ParamDef::new("offset_x", 0.0),
            ParamDef::new("offset_y", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, _| {
            // d = [antialias_radius, resolution_multiplier, true_color, pre_antialias, scale_x,
            //      scale_y, offset_x, offset_y, count, start, points [x, y, rgb] ..]
            let Some((x, y, rgb)) = shape_point(d, 8, d[0], ctx) else {
                return random_black(ctx, v);
            };
            v.x += x * d[4] + d[6];
            v.y += y * d[5] + d[7];
            let c = [(rgb >> 16 & 255) as i32, (rgb >> 8 & 255) as i32, (rgb & 255) as i32];
            if d[2] as i32 == 1 {
                v.rgb_color = true;
                v.red = c[0] as f64;
                v.green = c[1] as f64;
                v.blue = c[2] as f64;
            }
            v.color = ctx.uv_color_idx(c[0], c[1], c[2]);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[2] = ftoi(p[2]) as f64;
            d[3] = ftoi(p[3]) as f64;
            d.extend([0.0; 2]);
            d
        }),
        wgsl: "let q = res_shape_point(base, 8u, D(0));
if (q.w < 0.0) {
  v.x += rnd(); v.y += rnd();
  v.rgbc = 1.0; v.r = 0.0; v.g = 0.0; v.b = 0.0;
} else {
  v.x += q.x * D(4) + D(6);
  v.y += q.y * D(5) + D(7);
  let p = u32(q.z);
  let c = vec3<f32>(f32((p >> 16u) & 255u), f32((p >> 8u) & 255u), f32(p & 255u));
  if (i32(D(2)) == 1) { v.rgbc = 1.0; v.r = c.x; v.g = c.y; v.b = c.z; }
  v.color = res_nearest_color_idx(c);
}",
    },
    VarDef {
        name: "text_wf",
        params: &[
            ParamDef::int("font_size", 320.0, 4.0, 1000.0),
            ParamDef::range("antialias", 0.5, 0.0, 2.0),
            ParamDef::new("scale_x", 1.0),
            ParamDef::new("scale_y", 1.0),
            ParamDef::new("offset_x", 0.0),
            ParamDef::new("offset_y", 0.0),
            ParamDef::int("baseline", 0.0, -100.0, 100.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, _| {
            // d = [font_size, antialias, scale_x, scale_y, offset_x, offset_y, baseline, count,
            //      start, points ..]
            const DFLT_SCALE: f64 = 3.0;
            let Some((x, y, _)) = shape_point(d, 7, d[1], ctx) else {
                return random_black(ctx, v);
            };
            v.x += (x * d[2] + d[4]) * DFLT_SCALE;
            v.y += (y * d[3] + d[5]) * DFLT_SCALE;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = ftoi(p[0]) as f64;
            d[6] = ftoi(p[6]) as f64;
            d.extend([0.0; 2]);
            d
        }),
        wgsl: "let q = res_shape_point(base, 7u, D(1));
if (q.w < 0.0) {
  v.x += rnd(); v.y += rnd();
  v.rgbc = 1.0; v.r = 0.0; v.g = 0.0; v.b = 0.0;
} else {
  v.x += (q.x * D(2) + D(4)) * 3.0;
  v.y += (q.y * D(3) + D(5)) * 3.0;
}",
    },
];

const COLORMAP_RESOURCES: [ResDef; 4] = [
    ResDef { name: "image_filename", kind: ResKind::ImageFile, default: "" },
    ResDef { name: "inlined_image", kind: ResKind::ImageData, default: "" },
    ResDef { name: "image_desc_src", kind: ResKind::Text, default: "" },
    ResDef { name: "image_src", kind: ResKind::Text, default: "" },
];

const DISPLACEMAP_RESOURCES: [ResDef; 2] =
    [ResDef { name: "image_filename", kind: ResKind::ImageFile, default: "" }, ResDef { name: "inlined_image", kind: ResKind::ImageData, default: "" }];

const BRUSH_RESOURCES: [ResDef; 7] = [
    ResDef { name: "brush_list", kind: ResKind::Text, default: "001, 005, 012" },
    ResDef { name: "image_filename1", kind: ResKind::ImageFile, default: "" },
    ResDef { name: "image_filename2", kind: ResKind::ImageFile, default: "" },
    ResDef { name: "image_filename3", kind: ResKind::ImageFile, default: "" },
    ResDef { name: "image_filename4", kind: ResKind::ImageFile, default: "" },
    ResDef { name: "image_filename5", kind: ResKind::ImageFile, default: "" },
    ResDef { name: "image_filename6", kind: ResKind::ImageFile, default: "" },
];

const CURVE_RESOURCES: [ResDef; 3] = [
    ResDef { name: "xformula", kind: ResKind::Formula, default: "" },
    ResDef { name: "yformula", kind: ResKind::Formula, default: "" },
    ResDef { name: "zformula", kind: ResKind::Formula, default: "" },
];

const IMAGE_RESOURCES: [ResDef; 2] =
    [ResDef { name: "colormap_filename", kind: ResKind::ImageFile, default: "" }, ResDef { name: "displ_map_filename", kind: ResKind::ImageFile, default: "" }];

pub static RESOURCES: &[ResourceVar] = &[
    ResourceVar { name: "colormap_wf", resources: &COLORMAP_RESOURCES, load: colormap_load },
    ResourceVar { name: "post_colormap_wf", resources: &COLORMAP_RESOURCES, load: colormap_load },
    ResourceVar { name: "displacemap_wf", resources: &DISPLACEMAP_RESOURCES, load: displacemap_load },
    ResourceVar { name: "post_displacemap_wf", resources: &DISPLACEMAP_RESOURCES, load: displacemap_load },
    ResourceVar { name: "post_bumpmap_wf", resources: &DISPLACEMAP_RESOURCES, load: bumpmap_load },
    ResourceVar {
        name: "metaballs3d_wf",
        resources: &[ResDef { name: "colormap_filename", kind: ResKind::ImageFile, default: "" }],
        load: |d, res, _| load_image(d, MB_IMG, &res[0]),
    },
    ResourceVar {
        name: "kaleidoimg",
        resources: &COLORMAP_RESOURCES,
        load: |d, res, _| push_image(d, KI_IMG, &source_image(&res[0], &res[1], None, 1)),
    },
    ResourceVar { name: "knots3D", resources: &CURVE_RESOURCES, load: knots_load },
    ResourceVar { name: "terrain3D", resources: &IMAGE_RESOURCES, load: terrain_load },
    ResourceVar { name: "sattractor3D", resources: &CURVE_RESOURCES, load: sattractor_load },
    ResourceVar {
        name: "svg_wf",
        resources: &[ResDef { name: "svg", kind: ResKind::Svg, default: include_str!("../../assets/svg_wf_default.svg") }],
        load: |d, res, _| {
            let pts = crate::shapes::svg_points(&text(&res[0]), d[1], d[3] != 0.0);
            push_shape_points(d, 8, &pts);
        },
    },
    ResourceVar {
        name: "text_wf",
        resources: &[
            ResDef { name: "text", kind: ResKind::Text, default: "JWildfire" },
            ResDef { name: "font_name", kind: ResKind::Font, default: "Arial" },
        ],
        load: |d, res, _| {
            let pts = crate::shapes::text_points(&text(&res[0]), &text(&res[1]), d[0] as i32, d[6] as i32);
            push_shape_points(d, 7, &pts);
        },
    },
    ResourceVar { name: "brush_stroke_wf", resources: &BRUSH_RESOURCES, load: brush_load },
    ResourceVar { name: "post_brush_stroke_wf", resources: &BRUSH_RESOURCES, load: brush_load },
    ResourceVar { name: "plane_wf", resources: &IMAGE_RESOURCES, load: plane_wf_load },
    ResourceVar {
        name: "obj_mesh_wf",
        resources: &[
            ResDef { name: "obj_filename", kind: ResKind::ObjFile, default: "" },
            ResDef { name: "colormap_filename", kind: ResKind::ImageFile, default: "" },
            ResDef { name: "displ_map_filename", kind: ResKind::ImageFile, default: "" },
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
