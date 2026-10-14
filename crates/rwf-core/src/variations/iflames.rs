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

//! `iflames_wf` (Andreas Maschke): an image drawn with up to six embedded flames. Flames are
//! placed on the edges of the image (or a grid) and colored by it; each one runs as a
//! sub-flame, sized, rotated and moved by a particle simulation (`time`). Points not spent on
//! the flames draw the image itself.
//!
//! The placement uses JWildfire's seeded random numbers, image filters and draw order, so the
//! same parameters place the same flames. On the GPU the embedded flames run as sub-flame
//! kernels; flames with parameter maps (a different parameter value per placed flame) share
//! at most [`MAX_VARIANTS`] sub-flames there (values quantized), while the CPU uses the exact
//! values. Motion curves of the forces are not supported (the static force values apply).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use super::resource_vars::{push_image, text};
use super::subflame::{default_block, next_point, register_flame};
use super::{Ctx, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::math::{EPSILON, M_2PI, blerp, frac, ftoi};
use crate::resources::{self, ImageMap};
use crate::rng::JavaRandom;

include!("iflames_params.rs");

const FLAMES: usize = 6;
const FLAME_LEN: usize = 46;
const NP: usize = 28 + 13 + FLAMES * FLAME_LEN;

// image parameters
const P_THRESHOLD: usize = 0;
const P_DENSITY: usize = 1;
const P_IFLAME_DENSITY: usize = 2;
const P_IFLAME_BRIGHTNESS: usize = 3;
const P_IMAGE_BRIGHTNESS: usize = 4;
const P_SHAPE: usize = 5;
const P_ERODE: usize = 6;
const P_ERODE_SIZE: usize = 7;
const P_CONV: usize = 8;
const P_SCALE: usize = 12;
const P_OFFSET: usize = 15;
const P_MAX_W: usize = 18;
const P_SEED: usize = 21;
const P_CHG: usize = 22;
// motion parameters
const M_TIME: usize = 28;
const M_PREVIEW: usize = 29;
const M_LIFE: usize = 30;
const M_LIFE_VAR: usize = 31;
const M_FORCE: usize = 32;
const M_CENTRE: usize = 35;
// flame parameters (relative to the flame's block)
const FB: usize = 41;
const F_SIZE: usize = 0;
const F_ALPHA: usize = 2;
const F_BETA: usize = 6;
const F_GAMMA: usize = 10;
const F_SPEED: usize = 14;
const F_RADIAL: usize = 20;
const F_TANGENTIAL: usize = 22;
const F_CENTRE: usize = 24;
const F_MIN: usize = 26;
const F_MAX: usize = 27;
const F_PARAM: usize = 29;
const F_PREVIEW: usize = 35;
const F_GRID: usize = 38;
const F_BRIGHT: usize = 42;
const F_INSTANCING: usize = 45;

// header after the parameters
const H_COUNT: usize = NP;
const H_START: usize = NP + 1;
const H_IMG: usize = NP + 2;
const H_VARIANTS: usize = NP + 5;
const H_TABLE: usize = NP + 6;
/// Sub-flames per variation on the GPU.
pub const MAX_VARIANTS: usize = 24;
const BLOCK: usize = 13;
// placed flame record
const REC: usize = 18;
const R_VARIANT: usize = 0;
const R_SIZE: usize = 1;
const R_ROT: usize = 2;
const R_POS: usize = 5;
const R_RGB: usize = 8;
const R_BRIGHT: usize = 11;
const R_CENTRE: usize = 12;
const R_PREVIEW: usize = 14;
const R_CPU_ID: usize = 17;

fn fp(k: usize, field: usize) -> usize {
    FB + k * FLAME_LEN + field
}

/// The resource indices of flame `k`: XML, then (name, map) for the three parameters.
fn res_flame(k: usize) -> usize {
    2 + 7 * k
}

/// The data indices of the GPU sub-flame ids (see `subflame::id_indices`).
pub(crate) fn subflame_id_indices(d: &[f64]) -> Vec<usize> {
    let n = d.get(H_VARIANTS).map_or(0, |v| *v as usize).min(MAX_VARIANTS);
    (0..n).map(|k| H_TABLE + BLOCK * k + BLOCK - 1).collect()
}

// ---- images (`SimpleImage` and the JWildfire transformers used here) --------------------

#[derive(Clone)]
struct Img {
    w: usize,
    h: usize,
    px: Vec<[i32; 3]>,
}

impl Img {
    fn from_map(m: &ImageMap) -> Self {
        let px = m.pixels.iter().map(|&p| [(p >> 16 & 255) as i32, (p >> 8 & 255) as i32, (p & 255) as i32]).collect();
        Img { w: m.width as usize, h: m.height as usize, px }
    }

    fn get(&self, x: usize, y: usize) -> [i32; 3] {
        self.px[y * self.w + x]
    }

    /// `getARGBValueIgnoreBounds`: black outside.
    fn get_or_black(&self, x: i64, y: i64) -> [i32; 3] {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 { [0; 3] } else { self.get(x as usize, y as usize) }
    }
}

fn round_color(v: f64) -> i32 {
    ((v + 0.5) as i32).clamp(0, 255)
}

/// `ColorToGrayTransformer` (luminance weights in 10 bit fixed point).
fn grey(img: &Img) -> Img {
    let (rs, gs, bs) = ((2990i64 * 1024) / 10000, (5880i64 * 1024) / 10000, (1130i64 * 1024) / 10000);
    let px = img
        .px
        .iter()
        .map(|p| {
            let l = ((rs * p[0] as i64 + gs * p[1] as i64 + bs * p[2] as i64) >> 10).clamp(0, 255) as i32;
            [l; 3]
        })
        .collect();
    Img { w: img.w, h: img.h, px }
}

/// `ConvolveTransformer` with the Sobel kernel in grey mode (mirrored edges).
fn sobel_grey(src: &Img, direction: usize) -> Img {
    const K: [[i32; 3]; 3] = [[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]];
    let kernel: [[i32; 3]; 3] = std::array::from_fn(|j| {
        std::array::from_fn(|i| match direction {
            // north, east, south, west (`rotKernel[j][i]`)
            1 => K[2 - i][j],
            2 => K[2 - j][2 - i],
            3 => K[i][2 - j],
            _ => K[j][i],
        })
    });
    let g = grey(src);
    let (w, h) = (src.w as i64, src.h as i64);
    let mirror = |v: i64, n: i64| {
        (if v < 0 {
            -v
        } else if v >= n {
            2 * n - v - 1
        } else {
            v
        })
        .clamp(0, n - 1)
    };
    let mut px = Vec::with_capacity(src.px.len());
    for i in 0..h {
        for j in 0..w {
            let mut sum = 0i32;
            for (k, row) in kernel.iter().enumerate() {
                for (l, kv) in row.iter().enumerate() {
                    let x = mirror(j - 1 + l as i64, w);
                    let y = mirror(i - 1 + k as i64, h);
                    sum += g.get(x as usize, y as usize)[0] * kv;
                }
            }
            px.push([sum.clamp(0, 255); 3]);
        }
    }
    Img { w: src.w, h: src.h, px }
}

/// `ErodeTransformer.generateDisk` (taken from JWildfire, which draws it with AWT).
fn disk(size: i32) -> &'static [(i64, i64)] {
    const D3: &[(i64, i64)] = &[(0, -1), (-1, 0), (0, 0), (1, 0), (0, 1)];
    const D5: &[(i64, i64)] = &[
        (-1, -2),
        (0, -2),
        (1, -2),
        (-2, -1),
        (-1, -1),
        (0, -1),
        (1, -1),
        (2, -1),
        (-2, 0),
        (-1, 0),
        (0, 0),
        (1, 0),
        (2, 0),
        (-2, 1),
        (-1, 1),
        (0, 1),
        (1, 1),
        (2, 1),
        (-1, 2),
        (0, 2),
        (1, 2),
    ];
    const D7: &[(i64, i64)] = &[
        (-1, -3),
        (0, -3),
        (1, -3),
        (-2, -2),
        (-1, -2),
        (0, -2),
        (1, -2),
        (2, -2),
        (-3, -1),
        (-2, -1),
        (-1, -1),
        (0, -1),
        (1, -1),
        (2, -1),
        (3, -1),
        (-3, 0),
        (-2, 0),
        (-1, 0),
        (0, 0),
        (1, 0),
        (2, 0),
        (3, 0),
        (-3, 1),
        (-2, 1),
        (-1, 1),
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (-2, 2),
        (-1, 2),
        (0, 2),
        (1, 2),
        (2, 2),
        (-1, 3),
        (0, 3),
        (1, 3),
    ];
    const D9: &[(i64, i64)] = &[
        (-1, -4),
        (0, -4),
        (1, -4),
        (-3, -3),
        (-2, -3),
        (-1, -3),
        (0, -3),
        (1, -3),
        (2, -3),
        (3, -3),
        (-3, -2),
        (-2, -2),
        (-1, -2),
        (0, -2),
        (1, -2),
        (2, -2),
        (3, -2),
        (-4, -1),
        (-3, -1),
        (-2, -1),
        (-1, -1),
        (0, -1),
        (1, -1),
        (2, -1),
        (3, -1),
        (4, -1),
        (-4, 0),
        (-3, 0),
        (-2, 0),
        (-1, 0),
        (0, 0),
        (1, 0),
        (2, 0),
        (3, 0),
        (4, 0),
        (-4, 1),
        (-3, 1),
        (-2, 1),
        (-1, 1),
        (0, 1),
        (1, 1),
        (2, 1),
        (3, 1),
        (4, 1),
        (-3, 2),
        (-2, 2),
        (-1, 2),
        (0, 2),
        (1, 2),
        (2, 2),
        (3, 2),
        (-3, 3),
        (-2, 3),
        (-1, 3),
        (0, 3),
        (1, 3),
        (2, 3),
        (3, 3),
        (-1, 4),
        (0, 4),
        (1, 4),
    ];
    match size {
        3 => D3,
        5 => D5,
        7 => D7,
        _ => D9,
    }
}

/// `ErodeTransformer` in DILATE mode with a disk: the brightest pixel of the neighbourhood.
fn dilate(src: &Img, size: i32) -> Img {
    if size <= 1 {
        return src.clone();
    }
    let g = grey(src);
    let shape = disk(size);
    let (w, h) = (src.w as i64, src.h as i64);
    let mut px = Vec::with_capacity(src.px.len());
    for i in 0..h {
        for j in 0..w {
            let (mut best, mut bx, mut by) = (g.get(j as usize, i as usize)[0], j, i);
            for &(dx, dy) in shape {
                let (x, y) = (j + dx, i + dy);
                if x >= 0 && x < w && y >= 0 && y < h {
                    let l = g.get(x as usize, y as usize)[0];
                    if l > best {
                        (best, bx, by) = (l, x, y);
                    }
                }
            }
            px.push(src.get(bx as usize, by as usize));
        }
    }
    Img { w: src.w, h: src.h, px }
}

/// `ScaleTransformer` (bilinear up, box down; JWildfire's integer quirks kept).
fn scale(src: &Img, dw: usize, dh: usize) -> Img {
    let (sw, sh) = (src.w as i64, src.h as i64);
    let (w, h) = (dw.max(1) as i64, dh.max(1) as i64);
    let scl_x = if w > 1 { (sw - 1) as f64 / (w - 1) as f64 } else { 1.0 };
    let scl_y = if h > 1 { (sh - 1) as f64 / (h - 1) as f64 } else { 1.0 };
    let (cxd, cyd, cxs, cys) = ((w - 1) as f64 * 0.5, (h - 1) as f64 * 0.5, (sw - 1) as f64 * 0.5, (sh - 1) as f64 * 0.5);
    // readSrcPixels: P, Q (right), R (below), S (diagonal)
    let read = |x: f64, y: f64| -> [[f64; 3]; 4] {
        let (ix, iy) = ((x + 0.5) as i64, (y + 0.5) as i64);
        if ix < 0 || ix >= sw || iy < 0 || iy >= sh {
            return [[0.0; 3]; 4];
        }
        let p = |x: i64, y: i64| src.get(x as usize, y as usize).map(|c| c as f64);
        let pp = p(ix, iy);
        let q = if ix < sw - 1 { p(ix + 1, iy) } else { pp };
        let (r, s) = if iy < sh - 1 { (p(ix, iy + 1), if ix < sw - 1 { p(ix + 1, iy + 1) } else { q }) } else { (pp, q) };
        [pp, q, r, s]
    };
    let bilinear = |c: &[[f64; 3]; 4], xi: f64, yi: f64, k: usize| {
        round_color((1.0 - yi) * ((1.0 - xi) * c[0][k] + xi * c[1][k]) + yi * ((1.0 - xi) * c[2][k] + xi * c[3][k]))
    };
    let (down_x, down_y) = (w < sw, h < sh);
    let mut px = Vec::with_capacity((w * h) as usize);
    for i in 0..h {
        let y0 = i as f64 - cyd;
        for j in 0..w {
            let x0 = j as f64 - cxd;
            let x = x0 * scl_x + cxs;
            let y = y0 * scl_y + cys;
            if (x as i64) < 0 || (x as i64) >= sw || (y as i64) < 0 || (y as i64) >= sh {
                px.push([0; 3]);
                continue;
            }
            let (xi, yi) = (x - x.trunc(), y - y.trunc());
            let c = read(x, y);
            let mut cv: [i32; 3] = std::array::from_fn(|k| bilinear(&c, xi, yi, k));
            let mut cnt = 1;
            let xn = ((x0 + 1.0) * scl_x + cxs).min(if (x0 + 1.0) * scl_x + cxs >= sw as f64 - 0.5 { sw as f64 - 1.0 } else { f64::MAX });
            let yn = ((y0 + 1.0) * scl_y + cys).min(if (y0 + 1.0) * scl_y + cys >= sh as f64 - 0.5 { sh as f64 - 1.0 } else { f64::MAX });
            // Java: int += double (truncated).
            let add = |cv: &mut [i32; 3], v: [f64; 3]| {
                for k in 0..3 {
                    cv[k] = (cv[k] as f64 + v[k]) as i32;
                }
            };
            match (down_x, down_y) {
                (true, false) => {
                    let mut xc = x - xi + 1.0;
                    while xc < xn {
                        let c = read(xc, y);
                        add(&mut cv, std::array::from_fn(|k| (1.0 - yi) * c[0][k] + yi * c[2][k]));
                        xc += 1.0;
                        cnt += 1;
                    }
                }
                (false, true) => {
                    let mut yc = y - yi + 1.0;
                    while yc < yn {
                        let c = read(x, yc);
                        add(&mut cv, std::array::from_fn(|k| (1.0 - xi) * c[0][k] + xi * c[1][k]));
                        yc += 1.0;
                        cnt += 1;
                    }
                }
                (true, true) => {
                    // JWildfire reads the pixel at (x, y) in the inner loops (not at xc).
                    let mut xc = x - xi + 1.0;
                    while xc < xn {
                        let c = read(x, y);
                        add(&mut cv, std::array::from_fn(|k| (1.0 - yi) * c[0][k] + yi * c[2][k]));
                        xc += 1.0;
                        cnt += 1;
                    }
                    let mut yc = y - yi + 1.0;
                    while yc < yn {
                        let c = read(x, yc);
                        add(&mut cv, std::array::from_fn(|k| (1.0 - xi) * c[0][k] + xi * c[1][k]));
                        xc = x - xi + 1.0;
                        cnt += 1;
                        while xc < xn {
                            let c = read(x, yc);
                            add(&mut cv, c[0]);
                            xc += 1.0;
                            cnt += 1;
                        }
                        yc += 1.0;
                    }
                }
                (false, false) => {}
            }
            if cnt > 1 {
                cv = cv.map(|v| round_color(v as f64 / cnt as f64));
            }
            px.push(cv);
        }
    }
    Img { w: w as usize, h: h as usize, px }
}

/// The default image (`GradientCreator`, 128 × 128).
fn default_image() -> Arc<ImageMap> {
    static IMG: OnceLock<Arc<ImageMap>> = OnceLock::new();
    IMG.get_or_init(|| Arc::new(ImageMap::gradient(128))).clone()
}

fn load_image(filename: &[u8], inlined: &[u8]) -> Option<Arc<ImageMap>> {
    if !inlined.is_empty() {
        resources::image_from_bytes(inlined)
    } else if !filename.is_empty() {
        resources::image(&text(filename))
    } else {
        None
    }
}

// ---- placed flames (`BaseFlameListCreator`) ----------------------------------------------

fn intensity(r: f64, g: f64, b: f64) -> f64 {
    0.299 * r + 0.588 * g + 0.113 * b
}

/// A motion state (`Particle`), in float precision as in JWildfire.
#[derive(Clone, Copy)]
struct Particle {
    start: [f32; 15],
    life: f32,
    pos: [f32; 3],
    speed: [f32; 3],
    rot: [f32; 3],
    rot_speed: [f32; 3],
    radial: f32,
    tangential: f32,
}

impl Particle {
    /// `DynamicProperties`: x0, y0, z0, vx0, vy0, vz0, alpha0, beta0, gamma0, alpha speed,
    /// beta speed, gamma speed, radial and tangential acceleration, life.
    fn new(start: [f32; 15]) -> Self {
        let mut p = Particle { start, life: 0.0, pos: [0.0; 3], speed: [0.0; 3], rot: [0.0; 3], rot_speed: [0.0; 3], radial: 0.0, tangential: 0.0 };
        p.reset();
        p
    }

    fn reset(&mut self) {
        let s = self.start;
        self.life = s[14];
        self.pos = [s[0], s[1], s[2]];
        self.speed = [s[3], s[4], s[5]];
        self.rot = [s[6], s[7], s[8]];
        self.rot_speed = [s[9], s[10], s[11]];
        self.radial = s[12];
        self.tangential = s[13];
    }
}

fn len3(v: [f32; 3]) -> f32 {
    ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) as f64).sqrt() as f32
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let r = len3(v);
    if (r as f64) > EPSILON { v.map(|c| c / r) } else { v }
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// `IFlamesAnimator.animate` (static forces).
fn animate(particles: &mut [Particle], d: &[f64], time: f32) {
    const DT: f32 = 0.0025;
    let force = [d[M_FORCE] as f32, d[M_FORCE + 1] as f32, d[M_FORCE + 2] as f32];
    let centre = [d[M_CENTRE] as f32, d[M_CENTRE + 1] as f32, d[M_CENTRE + 2] as f32];
    let mut t = 0.0f32;
    let t_max = time.max(0.0);
    let add = |a: &mut [f32; 3], s: f32, v: [f32; 3]| {
        for k in 0..3 {
            a[k] += s * v[k];
        }
    };
    while t < t_max {
        for p in particles.iter_mut() {
            add(&mut p.speed, DT, force);
            let radial = p.radial * DT;
            let tangent = p.tangential * DT;
            if (radial.abs() as f64) > EPSILON || (tangent.abs() as f64) > EPSILON {
                let r = [p.pos[0] - centre[0], p.pos[1] - centre[1], p.pos[2] - centre[2]];
                let normal = normalize3(r);
                if (radial.abs() as f64) > EPSILON {
                    add(&mut p.speed, radial, normal);
                }
                if (tangent.abs() as f64) > EPSILON {
                    let (c1, c2) = (cross3(r, [0.0, 0.0, 1.0]), cross3(r, [0.0, 1.0, 0.0]));
                    let tv = normalize3(if len3(c1) > len3(c2) { c1 } else { c2 });
                    add(&mut p.speed, tangent, tv);
                }
            }
            let (speed, rot_speed) = (p.speed, p.rot_speed);
            add(&mut p.pos, DT, speed);
            add(&mut p.rot, DT, rot_speed);
            p.life -= DT;
            if p.life <= 0.0 {
                p.reset();
            }
        }
        t += DT;
    }
}

/// The placed flames: records, and the GPU sub-flame ids (variants).
struct Built {
    records: Vec<f64>,
    variants: Vec<usize>,
}

/// JWildfire property path (`layer1.transform2.julia3D.power`, `brightness`, ...) as an
/// animation path of RWildFire.
fn property_path(flame: &crate::model::Flame, path: &str) -> Option<String> {
    use crate::animation::{jwf_flame_curve_path, jwf_xform_curve_suffix};
    let mut comps: Vec<&str> = path.split('.').collect();
    if comps.first() == Some(&"flame") {
        comps.remove(0);
    }
    match comps.as_slice() {
        [name] => jwf_flame_curve_path(name).map(str::to_string),
        [layer, xf, rest @ ..] => {
            let l = layer.strip_prefix("layer")?.parse::<usize>().ok()?.checked_sub(1)?;
            let (kind, n) = if let Some(n) = xf.strip_prefix("finalTransform") { ("final", n) } else { ("xf", xf.strip_prefix("transform")?) };
            let x = n.parse::<usize>().ok()?.checked_sub(1)?;
            let layer = flame.layers.get(l)?;
            let xform = if kind == "final" { layer.final_xforms.get(x)? } else { layer.xforms.get(x)? };
            let base = format!("layer{l}.{kind}{x}");
            match rest {
                [field] => jwf_xform_curve_suffix(field).map(|s| format!("{base}.{s}")),
                [var, param] => {
                    let v = xform.variations.iter().position(|v| v.name == *var)?;
                    Some(if *param == "amount" { format!("{base}.var{v}.amount") } else { format!("{base}.var{v}.p.{param}") })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Registers flame XML `xml` with the parameter values `mods` as a sub-flame.
fn register(xml: &str, mods: &[(String, f64)]) -> usize {
    let key = format!("iflames#{mods:?}#{xml}");
    register_flame(key, || {
        let mut flame = crate::io::read_flames(xml).ok()?.into_iter().next()?;
        for (path, value) in mods {
            if let Some(p) = property_path(&flame, path) {
                crate::animation::set_prop(&mut flame, &p, *value);
            }
        }
        Some(flame)
    })
}

fn build(d: &[f64], res: &[Vec<u8>], color_map: &ImageMap) -> Built {
    let mut rnd = JavaRandom::new(d[P_SEED] as i64);
    let img = Img::from_map(color_map);
    let (img_w, img_h) = (img.w, img.h);
    let max_w = d[P_MAX_W] as usize;
    let scale_cm = if img_w > max_w { img_w as f64 / max_w as f64 } else { 1.0 };
    // structure map
    let mut structure = img.clone();
    if structure.w > max_w {
        let h = structure.h as f64 * (max_w as f64 / structure.w as f64);
        structure = scale(&structure, max_w, ((h + 0.5) as usize).max(1));
    }
    if d[P_ERODE] as i32 == 1 {
        structure = dilate(&structure, d[P_ERODE_SIZE] as i32);
    }
    let convs: Vec<Img> = (0..4).filter(|&k| d[P_CONV + k] as i32 == 1).map(|k| sobel_grey(&structure, k)).collect();
    if convs.len() == 1 {
        structure = convs[0].clone();
    } else if convs.len() > 1 {
        for (i, p) in structure.px.iter_mut().enumerate() {
            *p = std::array::from_fn(|k| round_color(convs.iter().map(|c| c.px[i][k]).sum::<i32>() as f64 / convs.len() as f64));
        }
    }
    let xml = |k: usize| text(&res[res_flame(k)]);
    let has_flame: Vec<bool> = (0..FLAMES).map(|k| !xml(k).is_empty()).collect();
    // parameter maps: JWildfire only uses maps it had to rescale.
    let param_maps: Vec<[Option<Img>; 3]> = (0..FLAMES)
        .map(|k| {
            std::array::from_fn(|p| {
                let (name, file) = (&res[res_flame(k) + 1 + 2 * p], &res[res_flame(k) + 2 + 2 * p]);
                if name.is_empty() || file.is_empty() {
                    return None;
                }
                let m = Img::from_map(&*resources::image(&text(file))?);
                if m.w < 8 || m.h < 8 {
                    return None;
                }
                (m.w != structure.w || m.h != structure.h).then(|| scale(&m, structure.w, structure.h))
            })
        })
        .collect();
    let threshold = d[P_THRESHOLD];
    let preview = d[M_PREVIEW] as i32 == 1;
    let mut shared: HashMap<usize, usize> = HashMap::new();
    let mut records: Vec<f64> = Vec::new();
    let mut gpu_keys: Vec<(usize, Vec<i64>)> = Vec::new();
    let mut variants: Vec<usize> = Vec::new();
    let mut particles: Vec<Particle> = Vec::new();
    let mut tool: [i32; 3];
    let shape = match ftoi(d[P_SHAPE]) {
        s @ 0..=4 => s,
        _ => 1,
    };
    let flame_index = |v: f64| -> i32 {
        let mut dx = f64::MAX;
        let mut res = -1;
        for k in 0..FLAMES {
            if has_flame[k] && v >= d[fp(k, F_MIN)] && v <= d[fp(k, F_MAX)] {
                let dist = d[fp(k, F_MAX)] - d[fp(k, F_MIN)];
                if dist < dx {
                    res = k as i32;
                    dx = dist;
                }
            }
        }
        res
    };
    // Parameter names in use per flame.
    let names: Vec<[String; 3]> = (0..FLAMES).map(|k| std::array::from_fn(|p| text(&res[res_flame(k) + 1 + 2 * p]))).collect();
    // GPU quantization levels per flame with parameters.
    let plain_types = (0..FLAMES).filter(|&k| has_flame[k] && (d[fp(k, F_INSTANCING)] as i32 == 1 || names[k].iter().all(String::is_empty))).count();
    let param_types = (0..FLAMES).filter(|&k| has_flame[k] && d[fp(k, F_INSTANCING)] as i32 != 1 && names[k].iter().any(|n| !n.is_empty())).count();
    let budget = (MAX_VARIANTS - plain_types.min(MAX_VARIANTS)).checked_div(param_types).unwrap_or(1).max(1);

    let mut add = |i: usize, j: usize, inten: f64, k: usize, tool: &mut [i32; 3], rnd: &mut JavaRandom| {
        let p_int = inten / (1.0 - threshold);
        let (mut rgb, mut brightness, mut cpu_id, mut variant_key) = ([0.0; 3], 1.0, 0usize, (k, Vec::new()));
        if !preview {
            // createFlameColor
            let mut c = tool.map(|v| v as f64 / 255.0);
            for ch in 0..3 {
                if d[P_CHG + ch].abs() > EPSILON {
                    c[ch] = (c[ch] + d[P_CHG + ch]).clamp(0.0, 1.0);
                }
            }
            if (0..3).any(|q| d[P_CHG + 3 + q].abs() > EPSILON) {
                let (h, s, l) = crate::random::gradients::hsl_of(c[0], c[1], c[2]);
                let (r, g, b) = crate::random::gradients::converter_from_hsl(h + d[P_CHG + 3], s + d[P_CHG + 4], l + d[P_CHG + 5]);
                c = [r, g, b];
            }
            rgb = c.map(|v| v * 255.0);
            let (bmin, bmax, change) = (d[fp(k, F_BRIGHT)], d[fp(k, F_BRIGHT + 1)], d[fp(k, F_BRIGHT + 2)]);
            brightness = if change > EPSILON && (rnd.next_double() < change || (change - EPSILON) >= 1.0) && bmin < bmax {
                bmin + rnd.next_double() * (bmax - bmin)
            } else {
                bmin
            };
            let base_xml = xml(k);
            let instancing = d[fp(k, F_INSTANCING)] as i32 == 1;
            if !instancing && names[k].iter().any(|n| !n.is_empty()) {
                let mut mods = Vec::new();
                let mut levels = Vec::new();
                let used = names[k].iter().filter(|n| !n.is_empty()).count() as f64;
                let per_param = (budget as f64).powf(1.0 / used).floor().max(1.0) as i64;
                for p in 0..3 {
                    if names[k][p].is_empty() {
                        continue;
                    }
                    let (min, max) = (d[fp(k, F_PARAM + 2 * p)], d[fp(k, F_PARAM + 2 * p + 1)]);
                    let value = match &param_maps[k][p] {
                        Some(m) => {
                            *tool = m.get_or_black(j as i64, i as i64);
                            let pi = intensity(tool[0] as f64, tool[1] as f64, tool[2] as f64);
                            min + (pi / 255.0) * (max - min)
                        }
                        None => min + rnd.next_double() * (max - min),
                    };
                    mods.push((names[k][p].clone(), value));
                    let t = if max > min { ((value - min) / (max - min)).clamp(0.0, 1.0) } else { 0.0 };
                    levels.push(if per_param > 1 { (t * (per_param - 1) as f64).round() as i64 } else { 0 });
                }
                cpu_id = register(&base_xml, &mods);
                variant_key = (k, levels);
            } else {
                cpu_id = *shared.entry(k).or_insert_with(|| register(&base_xml, &[]));
            }
        }
        // calculateMotionProperties
        let start_rot = |base: f64, var: f64, rnd: &mut JavaRandom| {
            let rad = M_2PI * base / 180.0;
            let v = if var >= 0.0 { var / 100.0 } else { 0.0 };
            if v > EPSILON { rad * (1.0 + v * (0.5 - rnd.next_double())) } else { rad }
        };
        let start_trans = |base: f64, var: f64, rnd: &mut JavaRandom| {
            let v = if var >= 0.0 { var / 100.0 } else { 0.0 };
            if v > EPSILON { base * (1.0 + v * (0.5 - rnd.next_double())) } else { base }
        };
        let pos_z = (0.1 * (0.5 - p_int)) as f32;
        let x0 = ((scale_cm * j as f64) * d[P_SCALE] / (img_w as f32 - 1.0) as f64 + d[P_OFFSET]) as f32;
        let y0 = ((scale_cm * i as f64) * d[P_SCALE + 1] / (img_h as f32 - 1.0) as f64 + d[P_OFFSET + 1]) as f32;
        let z0 = (pos_z as f64 * d[P_SCALE + 2] + d[P_OFFSET + 2]) as f32;
        let mut s = [0f32; 15];
        (s[0], s[1], s[2]) = (x0, y0, z0);
        for (n, f) in [F_ALPHA, F_BETA, F_GAMMA].into_iter().enumerate() {
            s[6 + n] = start_rot(d[fp(k, f)], d[fp(k, f + 1)], rnd) as f32;
        }
        for (n, f) in [F_ALPHA, F_BETA, F_GAMMA].into_iter().enumerate() {
            s[9 + n] = start_rot(d[fp(k, f + 2)], d[fp(k, f + 3)], rnd) as f32;
        }
        for n in 0..3 {
            s[3 + n] = start_trans(d[fp(k, F_SPEED + 2 * n)], d[fp(k, F_SPEED + 2 * n + 1)], rnd) as f32;
        }
        s[12] = start_trans(d[fp(k, F_RADIAL)], d[fp(k, F_RADIAL + 1)], rnd) as f32;
        s[13] = start_trans(d[fp(k, F_TANGENTIAL)], d[fp(k, F_TANGENTIAL + 1)], rnd) as f32;
        let base_life = d[M_LIFE] as f32;
        let life_var = (if d[M_LIFE_VAR] >= 0.0 { d[M_LIFE_VAR] / 100.0 } else { 0.0 }) as f32;
        s[14] = if life_var as f64 > EPSILON { (base_life as f64 * (1.0 + life_var as f64 * (0.5 - rnd.next_double()))) as f32 } else { base_life };
        // BaseFlame: size
        let size_var = if d[fp(k, F_SIZE + 1)] >= 0.0 { d[fp(k, F_SIZE + 1)] / 100.0 } else { 0.0 };
        let base_size = d[fp(k, F_SIZE)];
        let size = 0.01 * if size_var > EPSILON { base_size * (1.0 + size_var * (0.5 - rnd.next_double())) } else { base_size };
        let variant = gpu_keys.iter().position(|g| *g == variant_key).unwrap_or_else(|| {
            gpu_keys.push(variant_key.clone());
            gpu_keys.len() - 1
        });
        let mut r = [0.0; REC];
        r[R_VARIANT] = variant as f64;
        r[R_SIZE] = size;
        r[R_RGB..R_RGB + 3].copy_from_slice(&rgb);
        r[R_BRIGHT] = brightness;
        r[R_CENTRE] = d[fp(k, F_CENTRE)];
        r[R_CENTRE + 1] = d[fp(k, F_CENTRE + 1)];
        for c in 0..3 {
            r[R_PREVIEW + c] = d[fp(k, F_PREVIEW + c)];
        }
        r[R_CPU_ID] = cpu_id as f64;
        records.extend_from_slice(&r);
        particles.push(Particle::new(s));
    };

    let (sw, sh) = (structure.w, structure.h);
    if shape == 4 {
        // GRID
        for i in 0..sh {
            for j in 0..sw {
                tool = structure.get(j, i);
                let inten = intensity(tool[0] as f64 / 255.0, tool[1] as f64 / 255.0, tool[2] as f64 / 255.0);
                for k in 0..FLAMES {
                    if !has_flame[k] {
                        continue;
                    }
                    let (xoff, yoff, xsize, ysize) =
                        (d[fp(k, F_GRID)] as i64, d[fp(k, F_GRID + 1)] as i64, d[fp(k, F_GRID + 2)] as i64, d[fp(k, F_GRID + 3)] as i64);
                    if xsize > 0 && (xoff + j as i64) % xsize == 0 && ysize > 0 && (yoff + i as i64) % ysize == 0 {
                        add(i, j, inten, k, &mut tool, &mut rnd);
                    }
                }
            }
        }
    } else {
        for i in 0..sh {
            for j in 0..sw {
                let p = structure.get(j, i);
                let inten = intensity(p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0);
                if inten > threshold && d[P_DENSITY] > rnd.next_double() {
                    tool = img.get_or_black(ftoi(j as f64 * scale_cm) as i64, ftoi(i as f64 * scale_cm) as i64);
                    let c = tool.map(|v| v as f64 / 255.0);
                    let k = match shape {
                        0 => {
                            let list: Vec<usize> = (0..FLAMES).filter(|&k| has_flame[k]).collect();
                            if list.is_empty() { 0 } else { list[(rnd.next_double() * list.len() as f64) as usize] as i32 }
                        }
                        1 => flame_index(crate::random::gradients::hsl_of(c[0], c[1], c[2]).0),
                        2 => flame_index(crate::random::gradients::hsl_of(c[0], c[1], c[2]).2),
                        3 => flame_index(intensity(c[0], c[1], c[2])),
                        _ => -1,
                    };
                    if k >= 0 && (k as usize) < FLAMES && has_flame[k as usize] {
                        add(i, j, inten, k as usize, &mut tool, &mut rnd);
                    }
                }
            }
        }
    }
    // The sub-flames of the GPU variants (quantized parameter values).
    for (k, levels) in &gpu_keys {
        let id = if levels.is_empty() {
            shared.get(k).copied().unwrap_or_else(|| register(&xml(*k), &[]))
        } else {
            let used = names[*k].iter().filter(|n| !n.is_empty()).count() as f64;
            let per_param = (budget as f64).powf(1.0 / used).floor().max(1.0) as i64;
            let mut mods = Vec::new();
            let mut li = levels.iter();
            for p in 0..3 {
                if names[*k][p].is_empty() {
                    continue;
                }
                let (min, max) = (d[fp(*k, F_PARAM + 2 * p)], d[fp(*k, F_PARAM + 2 * p + 1)]);
                let l = *li.next().unwrap_or(&0);
                let t = if per_param > 1 { l as f64 / (per_param - 1) as f64 } else { 0.5 };
                mods.push((names[*k][p].clone(), min + t * (max - min)));
            }
            register(&xml(*k), &mods)
        };
        variants.push(id);
    }
    variants.truncate(MAX_VARIANTS);
    let time = d[M_TIME] as f32;
    if time as f64 > EPSILON {
        animate(&mut particles, d, time);
    }
    for (n, p) in particles.iter().enumerate() {
        let r = &mut records[n * REC..(n + 1) * REC];
        for c in 0..3 {
            r[R_ROT + c] = p.rot[c] as f64;
            r[R_POS + c] = p.pos[c] as f64;
        }
        r[R_VARIANT] = r[R_VARIANT].min((MAX_VARIANTS - 1) as f64);
    }
    Built { records, variants }
}

fn load(d: &mut Vec<f64>, res: &[Vec<u8>]) {
    let color_map = load_image(&res[0], &res[1]).unwrap_or_else(default_image);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    d[..NP].iter().for_each(|v| v.to_bits().hash(&mut hasher));
    res.hash(&mut hasher);
    let key = hasher.finish();
    type Cache = Mutex<Vec<(u64, Arc<Built>)>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let cached = cache.lock().expect("iflames cache").iter().find(|(k, _)| *k == key).map(|(_, b)| b.clone());
    let built = cached.unwrap_or_else(|| {
        let b = Arc::new(build(d, res, &color_map));
        let mut c = cache.lock().expect("iflames cache");
        if c.len() >= 8 {
            c.remove(0);
        }
        c.push((key, b.clone()));
        b
    });
    d[H_COUNT] = (built.records.len() / REC) as f64;
    d[H_VARIANTS] = built.variants.len() as f64;
    for (k, id) in built.variants.iter().enumerate() {
        d[H_TABLE + BLOCK * k..H_TABLE + BLOCK * (k + 1)].copy_from_slice(&default_block(*id));
    }
    push_image(d, H_IMG, &color_map);
    d[H_START] = d.len() as f64;
    d.extend_from_slice(&built.records);
}

// ---- transform -----------------------------------------------------------------------------

/// `IFlamesIterator.iterate`: the sub-flame point `q` sized, rotated around the flame's centre.
fn place(q: (f64, f64, f64), r: &[f64]) -> [f64; 3] {
    let (alpha, beta, gamma) = (r[R_ROT], r[R_ROT + 1], r[R_ROT + 2]);
    let (dr, cx, cy, cz) = (r[R_SIZE], r[R_CENTRE], r[R_CENTRE + 1], 0.0);
    let (sina, cosa) = alpha.sin_cos();
    if beta.abs() < EPSILON && gamma.abs() < EPSILON {
        let (dx, dy) = (q.0 - cx, q.1 - cy);
        [(dx * cosa - dy * sina) * dr + cx, (dx * sina + dy * cosa) * dr + cy, q.2]
    } else {
        let (sinb, cosb) = beta.sin_cos();
        let (sing, cosg) = gamma.sin_cos();
        let m = [
            [cosb * cosa - sina * sinb * cosg, cosb * sina + cosa * sinb * cosg, sinb * sing],
            [-sinb * cosa - cosb * cosg * sina, -sinb * sina + cosb * cosg * cosa, cosb * sing],
            [sina * sing, -cosa * sing, cosg],
        ];
        let (dx, dy, dz) = ((q.0 - cx) * dr, (q.1 - cy) * dr, (q.2 - cz) * dr);
        [
            m[0][0] * dx + m[1][0] * dy + m[2][0] * dz + cx,
            m[0][1] * dx + m[1][1] * dy + m[2][1] * dz + cy,
            m[0][2] * dx + m[1][2] * dy + m[2][2] * dz + cz,
        ]
    }
}

/// `addImage`: a point of the image, colored by it (black pixels are hidden).
fn add_image(d: &[f64], ctx: &mut Ctx, v: &mut Pt) {
    let (w, h, start) = (d[H_IMG] as i64, d[H_IMG + 1] as i64, d[H_IMG + 2] as usize);
    if w < 2 || h < 2 {
        return;
    }
    let x = ctx.random_int(w as usize) as f64 + (0.5 - ctx.random());
    let y = ctx.random_int(h as usize) as f64 + (0.5 - ctx.random());
    v.x += x * d[P_SCALE] / (w - 1) as f64 + d[P_OFFSET];
    v.y += y * d[P_SCALE + 1] / (h - 1) as f64 + d[P_OFFSET + 1];
    v.z += d[P_OFFSET + 2];
    let px = |ix: i64, iy: i64| -> [f64; 3] {
        if ix < 0 || iy < 0 || ix >= w || iy >= h {
            return [0.0; 3];
        }
        let p = d[start + (iy * w + ix) as usize] as u32;
        [(p >> 16 & 255) as f64, (p >> 8 & 255) as f64, (p & 255) as f64]
    };
    let (ix, iy) = (x as i64, y as i64);
    let lu = px(ix, iy);
    v.do_hide = false;
    if lu == [0.0; 3] {
        v.do_hide = true;
        return;
    }
    let (ru, lb, rb) = (px(ix + 1, iy), px(ix, iy + 1), px(ix + 1, iy + 1));
    let (fx, fy) = (frac(x), frac(y));
    let c: [f64; 3] = std::array::from_fn(|k| blerp(lu[k], ru[k], lb[k], rb[k], fx, fy));
    v.rgb_color = true;
    v.red = c[0] * d[P_IMAGE_BRIGHTNESS];
    v.green = c[1] * d[P_IMAGE_BRIGHTNESS];
    v.blue = c[2] * d[P_IMAGE_BRIGHTNESS];
}

fn apply(d: &[f64], ctx: &mut Ctx, v: &mut Pt) {
    if !(d[P_IMAGE_BRIGHTNESS] < EPSILON || ctx.random() < d[P_IFLAME_DENSITY]) {
        return add_image(d, ctx, v);
    }
    let n = d[H_COUNT] as usize;
    if n == 0 {
        return;
    }
    let o = d[H_START] as usize + REC * ctx.random_int(n);
    let r = &d[o..o + REC];
    let pos = [r[R_POS], r[R_POS + 1], r[R_POS + 2]];
    let scale = d[P_IFLAME_BRIGHTNESS] * r[R_BRIGHT];
    if d[M_PREVIEW] as i32 == 1 {
        v.x += pos[0];
        v.y += pos[1];
        v.z += pos[2];
        v.rgb_color = true;
        (v.red, v.green, v.blue) = (r[R_PREVIEW] * scale, r[R_PREVIEW + 1] * scale, r[R_PREVIEW + 2] * scale);
        return;
    }
    let q = next_point(&default_block(r[R_CPU_ID] as usize), ctx).map_or((0.0, 0.0, 0.0), |q| (q.x, q.y, q.z));
    let p = place(q, r);
    v.x += p[0] + pos[0];
    v.y += p[1] + pos[1];
    v.z += p[2] + pos[2];
    // The sub-flame's color mode -1 leaves the color index at 0.
    v.color = 0.0;
    v.rgb_color = true;
    (v.red, v.green, v.blue) = (r[R_RGB] * scale, r[R_RGB + 1] * scale, r[R_RGB + 2] * scale);
}

pub static DEFS: &[VarDef] = &[VarDef {
    name: "iflames_wf",
    params: &PARAMS,
    kind: VarKind::Normal,
    is_3d: true,
    apply: |d, ctx, _, v, _| apply(d, ctx, v),
    prepare: Some(|p, _| {
        // The conversions of JWildfire's setParameter.
        let mut d = p.to_vec();
        let int = |v: f64, lo: i32, hi: i32| ftoi(v).clamp(lo, hi) as f64;
        d[P_THRESHOLD] = p[P_THRESHOLD].clamp(0.0, 0.99999);
        d[P_DENSITY] = p[P_DENSITY].clamp(0.0, 1.0);
        d[P_IFLAME_DENSITY] = p[P_IFLAME_DENSITY].clamp(0.0, 1.0);
        d[P_SHAPE] = int(p[P_SHAPE], i32::MIN, i32::MAX);
        d[P_ERODE] = int(p[P_ERODE], 0, 1);
        let mut es = ftoi(p[P_ERODE_SIZE]).max(3);
        if es % 2 == 0 {
            es -= 1;
        }
        d[P_ERODE_SIZE] = es.min(9) as f64;
        for k in 0..4 {
            d[P_CONV + k] = int(p[P_CONV + k], 0, 1);
        }
        d[P_MAX_W] = int(p[P_MAX_W], 16, 64000);
        for i in [19, 20, 21] {
            d[i] = ftoi(p[i]) as f64;
        }
        for k in 0..6 {
            d[P_CHG + k] = p[P_CHG + k].clamp(-1.0, 1.0);
        }
        d[M_PREVIEW] = int(p[M_PREVIEW], 0, 1);
        d[M_LIFE_VAR] = p[M_LIFE_VAR].max(0.0);
        for k in 0..FLAMES {
            for f in [F_MIN, F_MAX, 28] {
                d[fp(k, f)] = p[fp(k, f)].clamp(0.0, 100.0);
            }
            for c in 0..3 {
                d[fp(k, F_PREVIEW + c)] = int(p[fp(k, F_PREVIEW + c)], 0, 255);
            }
            for g in 0..4 {
                d[fp(k, F_GRID + g)] = ftoi(p[fp(k, F_GRID + g)]) as f64;
            }
            d[fp(k, F_INSTANCING)] = if ftoi(p[fp(k, F_INSTANCING)]) == 1 { 1.0 } else { 0.0 };
        }
        d.extend(std::iter::repeat_n(0.0, 6 + MAX_VARIANTS * BLOCK));
        d
    }),
    wgsl: "fn iflames_px(base: u32, w: i32, h: i32, x: i32, y: i32) -> vec3<f32> {
  if (x < 0 || y < 0 || x >= w || y >= h) { return vec3<f32>(0.0); }
  let p = u32(prm[base + u32(prm[base + 321u]) + u32(y * w + x)]);
  return vec3<f32>(f32((p >> 16u) & 255u), f32((p >> 8u) & 255u), f32(p & 255u));
}
@body
if (D(4) < EPSILON || rnd() < D(2)) {
  let n = u32(D(317));
  if (n > 0u) {
    let o = base + u32(D(318)) + 18u * (rnd_u32() % n);
    let pos = vec3<f32>(prm[o + 5u], prm[o + 6u], prm[o + 7u]);
    let scl = D(3) * prm[o + 11u];
    if (D(29) == 1.0) {
      v.x += pos.x; v.y += pos.y; v.z += pos.z;
      v.rgbc = 1.0; v.r = prm[o + 14u] * scl; v.g = prm[o + 15u] * scl; v.b = prm[o + 16u] * scl;
    } else {
      let s = subflame_point(base, 323u + 13u * u32(prm[o]));
      var q = vec3<f32>(0.0);
      if (s.valid) { q = vec3<f32>(s.p.x, s.p.y, s.p.z); }
      let alpha = prm[o + 2u]; let beta = prm[o + 3u]; let gamma = prm[o + 4u];
      let dr = prm[o + 1u]; let cx = prm[o + 12u]; let cy = prm[o + 13u];
      let sina = sin(alpha); let cosa = cos(alpha);
      var p = vec3<f32>(0.0);
      if (abs(beta) < EPSILON && abs(gamma) < EPSILON) {
        let dx = q.x - cx; let dy = q.y - cy;
        p = vec3<f32>((dx * cosa - dy * sina) * dr + cx, (dx * sina + dy * cosa) * dr + cy, q.z);
      } else {
        let sinb = sin(beta); let cosb = cos(beta); let sing = sin(gamma); let cosg = cos(gamma);
        let dx = (q.x - cx) * dr; let dy = (q.y - cy) * dr; let dz = q.z * dr;
        p.x = (cosb * cosa - sina * sinb * cosg) * dx + (-sinb * cosa - cosb * cosg * sina) * dy + sina * sing * dz + cx;
        p.y = (cosb * sina + cosa * sinb * cosg) * dx + (-sinb * sina + cosb * cosg * cosa) * dy - cosa * sing * dz + cy;
        p.z = sinb * sing * dx + cosb * sing * dy + cosg * dz;
      }
      v.x += p.x + pos.x; v.y += p.y + pos.y; v.z += p.z + pos.z;
      v.color = 0.0;
      v.rgbc = 1.0; v.r = prm[o + 8u] * scl; v.g = prm[o + 9u] * scl; v.b = prm[o + 10u] * scl;
    }
  }
} else {
  let w = i32(D(319)); let h = i32(D(320));
  if (w >= 2 && h >= 2) {
    let x = f32(min(i32(rnd() * f32(w)), w - 1)) + (0.5 - rnd());
    let y = f32(min(i32(rnd() * f32(h)), h - 1)) + (0.5 - rnd());
    v.x += x * D(12) / f32(w - 1) + D(15);
    v.y += y * D(13) / f32(h - 1) + D(16);
    v.z += D(17);
    let ix = i32(x); let iy = i32(y);
    let lu = iflames_px(base, w, h, ix, iy);
    if (lu.x == 0.0 && lu.y == 0.0 && lu.z == 0.0) {
      v.hide = 1.0;
    } else {
      v.hide = 0.0;
      let ru = iflames_px(base, w, h, ix + 1, iy);
      let lb = iflames_px(base, w, h, ix, iy + 1);
      let rb = iflames_px(base, w, h, ix + 1, iy + 1);
      let fx = x - trunc(x); let fy = y - trunc(y);
      let c = mix(mix(lu, ru, fx), mix(lb, rb, fx), fy);
      v.rgbc = 1.0; v.r = c.x * D(4); v.g = c.y * D(4); v.b = c.z * D(4);
    }
  }
}",
}];

pub static RESOURCES: &[ResourceVar] = &[ResourceVar { name: "iflames_wf", resources: &RESOURCES_DEFS, load: |d, res, _| load(d, res) }];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_constants_match_wgsl() {
        assert_eq!(NP, 317);
        assert_eq!((H_COUNT, H_START, H_IMG, H_IMG + 1, H_IMG + 2, H_TABLE), (317, 318, 319, 320, 321, 323));
        assert_eq!((P_IFLAME_DENSITY, P_IFLAME_BRIGHTNESS, P_IMAGE_BRIGHTNESS, M_PREVIEW, P_SCALE, P_OFFSET), (2, 3, 4, 29, 12, 15));
        assert_eq!(PARAMS.len(), NP);
        assert_eq!(RESOURCES_DEFS.len(), 2 + 7 * FLAMES);
        assert_eq!(PARAMS[fp(5, F_INSTANCING)].name, "instancing006");
    }

    #[test]
    fn property_paths() {
        let flame = crate::io::read_flames(include_str!("../../assets/iflames/example1.flame")).unwrap().remove(0);
        assert_eq!(property_path(&flame, "layer1.transform1.waves2_wf.scalex").as_deref(), Some("layer0.xf0.var0.p.scalex"));
        assert_eq!(property_path(&flame, "layer1.transform2.spherical3D.amount").as_deref(), Some("layer0.xf1.var0.amount"));
        assert_eq!(property_path(&flame, "layer1.transform3.weight").as_deref(), Some("layer0.xf2.weight"));
        assert_eq!(property_path(&flame, "layer1.transform9.weight"), None);
        let mut f = flame.clone();
        assert!(crate::animation::set_prop(&mut f, &property_path(&flame, "layer1.transform1.waves2_wf.scalex").unwrap(), 0.25));
        assert_eq!(crate::animation::get_prop(&f, "layer0.xf0.var0.p.scalex"), Some(0.25));
    }

    #[test]
    fn sobel_finds_edges() {
        // Left half black, right half white: the north kernel (x gradient) lights the edge.
        let mut img = Img { w: 8, h: 4, px: vec![[0; 3]; 32] };
        for y in 0..4 {
            for x in 4..8 {
                img.px[y * 8 + x] = [255; 3];
            }
        }
        let e = sobel_grey(&img, 0);
        let lit: Vec<usize> = (0..8).filter(|&x| e.get(x, 1)[0] > 0).collect();
        assert_eq!(lit, vec![3, 4]);
    }

    #[test]
    fn default_flames_are_placed() {
        let def = &DEFS[0];
        let mut params: Vec<f64> = def.params.iter().map(|p| p.default).collect();
        params[P_DENSITY] = 0.5;
        let mut d = (def.prepare.unwrap())(&params, &crate::model::Affine::IDENTITY);
        let mut res: Vec<Vec<u8>> = RESOURCES_DEFS.iter().map(|r| r.default.as_bytes().to_vec()).collect();
        // The default gradient has no edges above the threshold (none placed, as in JWildfire):
        // use colored blocks.
        let img = image::RgbaImage::from_fn(96, 64, |x, y| {
            let k = (x / 16 + y / 16) as u8;
            image::Rgba([k.wrapping_mul(70), 255 - k.wrapping_mul(40), k.wrapping_mul(110), 255])
        });
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        res[1] = png.into_inner();
        load(&mut d, &res);
        let n = d[H_COUNT] as usize;
        assert!(n > 10, "{n} flames placed");
        assert!((1..=6).contains(&(d[H_VARIANTS] as usize)));
        let mut ctx = Ctx::new(1, false);
        let mut spread = 0.0f64;
        for _ in 0..2000 {
            let mut v = Pt::default();
            apply(&d, &mut ctx, &mut v);
            assert!(!v.is_bad());
            spread = spread.max(v.x.abs());
        }
        assert!(spread > 1.0, "{spread}");
    }
}
