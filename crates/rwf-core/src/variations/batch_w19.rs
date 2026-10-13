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

//! Variations ported from the JWildfire sources (batch w19).
//!
//! Note on `prepost_*` and `ringtile` (JWildfire priority 2): JWildfire runs `invtransform` as a
//! pre step and `transform` as a post step. They are registered as `VarKind::PrePost` with the
//! inverse as a separate, unregistered `VarDef` (`*_INV` below).

use super::complex::Complex;
#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

#[inline]
fn ftoi_limit(x: f64, lo: i32, hi: i32) -> f64 {
    limit_value(ftoi(x), lo, hi) as f64
}

/// `java.util.Random` (48-bit LCG), used to rebuild seeded per-instance tables in `prepare`.
struct JavaRandom {
    seed: i64,
}

const JAVA_RANDOM_MULT: i64 = 0x5_DEEC_E66D;
const JAVA_RANDOM_MASK: i64 = (1 << 48) - 1;

impl JavaRandom {
    fn new(seed: i64) -> Self {
        Self { seed: (seed ^ JAVA_RANDOM_MULT) & JAVA_RANDOM_MASK }
    }

    fn next(&mut self, bits: u32) -> i64 {
        self.seed = self.seed.wrapping_mul(JAVA_RANDOM_MULT).wrapping_add(0xB) & JAVA_RANDOM_MASK;
        self.seed >> (48 - bits)
    }

    fn next_double(&mut self) -> f64 {
        ((self.next(26) << 27) + self.next(27)) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

// ---------------------------------------------------------------------------------------------
// pre_falloff3 (AbstractFalloff3Func with v_in = affine point, result written to the affine point)
// d = [blur_type, blur_shape, blur_strength, min_distance, invert_distance,
//      mul_x, mul_y, mul_z, mul_c, center_x, center_y, center_z, alpha, r_max]

#[inline]
fn pre_falloff3_sgnd(x: f64) -> f64 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

#[inline]
fn pre_falloff3_log_scale(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { ((x.abs() + 1.0) * M_E).ln() * pre_falloff3_sgnd(x) / M_E }
}

#[inline]
fn pre_falloff3_log_map(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { (M_E + (x * M_E).ln()) / 4.0 * pre_falloff3_sgnd(x) }
}

fn pre_falloff3_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let strength = if p[2] < 1.0e-6 { 1.0e-6 } else { p[2] };
    let min_dist = if p[3] < 0.0 { 0.0 } else { p[3] };
    vec![
        ftoi_limit(p[0], 0, 2),
        ftoi_limit(p[1], 0, 1),
        strength,
        min_dist,
        ftoi_limit(p[4], 0, 1),
        limit_value(p[5], 0.0, 1.0),
        limit_value(p[6], 0.0, 1.0),
        limit_value(p[7], 0.0, 1.0),
        limit_value(p[8], 0.0, 1.0),
        p[9],
        p[10],
        p[11],
        p[12],
        0.04 * strength,
    ]
}

fn pre_falloff3_vout(d: &[f64], ctx: &mut Ctx, vin: [f64; 4]) -> [f64; 4] {
    let rnd = [ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5];
    let (dx, dy, dz) = (vin[0] - d[9], vin[1] - d[10], vin[2] - d[11]);
    let radius = if d[1] as i32 == 0 {
        (dx * dx + dy * dy + dz * dz).sqrt()
    } else {
        java_max(dx.abs(), java_max(dy.abs(), dz.abs()))
    };
    let r_max = d[13];
    let base = if d[4] as i32 != 0 { java_max(1.0 - radius, 0.0) } else { java_max(radius, 0.0) };
    let dist = java_max((base - d[3]) * r_max, 0.0);
    let mul = [d[5], d[6], d[7], d[8]];
    let alpha = d[12];
    match d[0] as i32 {
        0 => {
            let sigma = dist * rnd[1] * M_2PI;
            let phi = dist * rnd[2] * M_PI;
            let rad = dist * rnd[0];
            let (ss, sc) = sigma.sin_cos();
            let (ps, pc) = phi.sin_cos();
            [
                vin[0] + mul[0] * rad * sc * pc,
                vin[1] + mul[1] * rad * sc * ps,
                vin[2] + mul[2] * rad * ss,
                vin[3] + mul[3] * dist * rnd[3],
            ]
        }
        1 => {
            if vin[0] == 0.0 && vin[1] == 0.0 && vin[2] == 0.0 {
                vin
            } else {
                let r_in = (sqr(vin[0]) + sqr(vin[1]) + sqr(vin[2])).sqrt();
                let aa = mul[1] * dist + alpha * dist / r_max.sqrt();
                let bb = mul[2] * dist;
                let sigma = (vin[2] / r_in).asin() + bb * rnd[2];
                let phi = vin[1].atan2(vin[0]) + aa * rnd[1];
                let r = r_in + mul[0] * rnd[0] * dist;
                let (ss, sc) = sigma.sin_cos();
                let (ps, pc) = phi.sin_cos();
                [r * sc * pc, r * sc * ps, r * ss, vin[3] + mul[3] * rnd[3] * dist]
            }
        }
        _ => {
            let coeff = if r_max <= EPSILON { dist } else { dist + alpha * (pre_falloff3_log_map(dist) - dist) };
            [
                vin[0] + pre_falloff3_log_map(mul[0]) * pre_falloff3_log_scale(rnd[0]) * coeff,
                vin[1] + pre_falloff3_log_map(mul[1]) * pre_falloff3_log_scale(rnd[1]) * coeff,
                vin[2] + pre_falloff3_log_map(mul[2]) * pre_falloff3_log_scale(rnd[2]) * coeff,
                vin[3] + pre_falloff3_log_map(mul[3]) * pre_falloff3_log_scale(rnd[3]) * coeff,
            ]
        }
    }
}

// ---------------------------------------------------------------------------------------------
// prepost_affine / pre_prepost_affine
// d = [scale_x, scale_y, scale_z, yaw, pitch, roll, move_x, move_y, move_z,
//      coefxx, coefxy, coefxz, coefyx, coefyy, coefyz, coefzx, coefzy, coefzz,
//      icoefxx, icoefxy, icoefxz, icoefyx, icoefyy, icoefyz, icoefzx, icoefzy, icoefzz]

const PREPOST_AFFINE_PARAMS: &[ParamDef] = &[
    ParamDef::new("scale_x", 1.0),
    ParamDef::new("scale_y", 1.0),
    ParamDef::new("scale_z", 1.0),
    ParamDef::range("yaw", 0.0, -180.0, 180.0),
    ParamDef::range("pitch", 0.0, -180.0, 180.0),
    ParamDef::range("roll", 0.0, -180.0, 180.0),
    ParamDef::new("move_x", 0.0),
    ParamDef::new("move_y", 0.0),
    ParamDef::new("move_z", 0.0),
];

fn prepost_affine_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = p.to_vec();
    for s in d.iter_mut().take(3) {
        if *s == 0.0 {
            *s = EPSILON;
        }
    }
    let d2r = M_PI / 180.0;
    let (sinyaw, cosyaw) = (p[3] * d2r).sin_cos();
    let (sinpitch, cospitch) = (p[4] * d2r).sin_cos();
    let (sinroll, cosroll) = (p[5] * d2r).sin_cos();
    d.extend_from_slice(&[
        cosyaw * cosroll - sinyaw * sinpitch * sinroll,
        -sinyaw * cosroll - cosyaw * sinpitch * sinroll,
        -cospitch * sinroll,
        sinyaw * cospitch,
        cosyaw * cospitch,
        -sinpitch,
        cosyaw * sinroll + sinyaw * sinpitch * cosroll,
        cosyaw * sinpitch * cosroll - sinyaw * sinroll,
        cospitch * cosroll,
    ]);
    d.extend_from_slice(&[
        cosyaw * cosroll - sinyaw * sinpitch * sinroll,
        sinyaw * cospitch,
        cosyaw * sinroll + sinyaw * sinpitch * cosroll,
        -sinyaw * cosroll - cosyaw * sinpitch * sinroll,
        cosyaw * cospitch,
        cosyaw * sinpitch * cosroll - sinyaw * sinroll,
        -cospitch * sinroll,
        -sinpitch,
        cospitch * cosroll,
    ]);
    d
}


/// `pre_prepost_affine` (JWildfire `InternalPrePostAffinePreWFFunc`), also the inverse of
/// `prepost_affine`.
const PRE_PREPOST_AFFINE: VarDef = VarDef {
    name: "pre_prepost_affine",
    params: PREPOST_AFFINE_PARAMS,
    kind: VarKind::Pre,
    is_3d: true,
    apply: |d, _, a, _, amount| {
        // Inverse of prepost_affine (the GPU code of InternalPrePostAffinePreWFFunc; its Java
        // CPU transform is empty).
        if amount == 0.0 {
            return;
        }
        let x = a.x - d[6];
        let y = a.y - d[7];
        let z = a.z - d[8];
        a.x = (d[18] * x + d[19] * y + d[20] * z) / (amount * d[0]);
        a.y = (d[21] * x + d[22] * y + d[23] * z) / (amount * d[1]);
        a.z = (d[24] * x + d[25] * y + d[26] * z) / (amount * d[2]);
    },
    prepare: Some(prepost_affine_prepare),
    wgsl: "if (AMOUNT != 0.0) {
  let x = a.x - D(6);
  let y = a.y - D(7);
  let z = a.z - D(8);
  a.x = (D(18) * x + D(19) * y + D(20) * z) / (AMOUNT * D(0));
  a.y = (D(21) * x + D(22) * y + D(23) * z) / (AMOUNT * D(1));
  a.z = (D(24) * x + D(25) * y + D(26) * z) / (AMOUNT * D(2));
}",
};

static PREPOST_AFFINE_INV: VarDef = PRE_PREPOST_AFFINE;


// ---------------------------------------------------------------------------------------------
// Inverse (pre slot) halves of the priority 2 variations (`invtransform`)

const PREPOST_BLOB_PARAMS: &[ParamDef] =
    &[ParamDef::new("low", 0.8), ParamDef::new("high", 1.2), ParamDef::range("waves", 6.0, 1.0, 31.0)];

static PREPOST_BLOB_INV: VarDef = VarDef {
    name: "prepost_blob_inv",
    params: PREPOST_BLOB_PARAMS,
    kind: VarKind::Pre,
    is_3d: false,
    apply: |d, _, a, _, amount| {
        let alow = 1.0 - amount * (1.0 - d[0]);
        let ahigh = 1.0 - amount * (1.0 - d[1]);
        let ang = a.x.atan2(a.y);
        let r = (a.x * a.x + a.y * a.y).sqrt() / (alow + (ahigh - alow) * (0.5 + 0.5 * (d[2] * ang).sin()));
        a.x = ang.sin() * r;
        a.y = ang.cos() * r;
    },
    prepare: None,
    wgsl: "let alow = 1.0 - AMOUNT * (1.0 - D(0));
let ahigh = 1.0 - AMOUNT * (1.0 - D(1));
let ang = atan2(a.x, a.y);
let r = sqrt(a.x * a.x + a.y * a.y) / (alow + (ahigh - alow) * (0.5 + 0.5 * sin(D(2) * ang)));
a.x = sin(ang) * r;
a.y = cos(ang) * r;",
};

const PREPOST_CIRCLIZE_PARAMS: &[ParamDef] = &[
    ParamDef::int("n", 4.0, 3.0, 50.0),
    ParamDef::range("rotation", 45.0, -180.0, 180.0),
    ParamDef::int("reverse", 0.0, 0.0, 1.0),
];

// d = [n, rotation, reverse, rot_rad, pi_n, cospi_n]
fn prepost_circlize_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let n = ftoi_limit(p[0], 3, 50);
    let pi_n = M_PI / n;
    vec![n, p[1], ftoi_limit(p[2], 0, 1), p[1] * M_PI / 180.0, pi_n, pi_n.cos()]
}

static PREPOST_CIRCLIZE_INV: VarDef = VarDef {
    name: "prepost_circlize_inv",
    params: PREPOST_CIRCLIZE_PARAMS,
    kind: VarKind::Pre,
    is_3d: false,
    apply: |d, _, a, _, amount| {
        let theta = a.atan_yx();
        let r = a.rad();
        let tr = theta - d[3];
        let factor = d[5] / (tr - d[4] * (2.0 * (d[0] * tr / M_2PI).floor() + 1.0)).cos();
        // invtransform: reverse == 0 -> circlize (r / factor), else uncirclize (r * factor)
        let target = if d[2] as i32 == 0 { r / factor } else { r * factor };
        let l = (1.0 - amount) * r + amount * target;
        a.x = l * theta.cos();
        a.y = l * theta.sin();
    },
    prepare: Some(prepost_circlize_prepare),
    wgsl: "let theta = atan_yx(a);
let r = rad(a);
let tr = theta - D(3);
let factor = D(5) / cos(tr - D(4) * (2.0 * floor(D(0) * tr / TWO_PI) + 1.0));
var tgt = r * factor;
if (i32(D(2)) == 0) { tgt = r / factor; }
let l = (1.0 - AMOUNT) * r + AMOUNT * tgt;
a.x = l * cos(theta);
a.y = l * sin(theta);",
};

const PREPOST_MOBIUS_PARAMS: &[ParamDef] = &[
    ParamDef::new("re_a", 1.0),
    ParamDef::new("re_b", 0.0),
    ParamDef::new("re_c", 0.0),
    ParamDef::new("re_d", 1.0),
    ParamDef::new("im_a", 0.0),
    ParamDef::new("im_b", 0.0),
    ParamDef::new("im_c", 0.0),
    ParamDef::new("im_d", 0.0),
];

static PREPOST_MOBIUS_INV: VarDef = VarDef {
    name: "prepost_mobius_inv",
    params: PREPOST_MOBIUS_PARAMS,
    kind: VarKind::Pre,
    is_3d: false,
    apply: |d, _, a, _, amount| {
        // d = [re_a, re_b, re_c, re_d, im_a, im_b, im_c, im_d]
        if amount == 0.0 {
            return;
        }
        let re_u = d[3] * a.x - d[7] * a.y - d[1];
        let im_u = d[3] * a.y + d[7] * a.x - d[5];
        let re_v = -d[2] * a.x + d[6] * a.y + d[0];
        let im_v = -d[2] * a.y - d[6] * a.x + d[4];
        let den = re_v * re_v + im_v * im_v;
        if den == 0.0 {
            return;
        }
        let rad_v = 1.0 / (amount * den);
        a.x = rad_v * (re_u * re_v + im_u * im_v);
        a.y = rad_v * (im_u * re_v - re_u * im_v);
    },
    prepare: None,
    wgsl: "let re_u = D(3) * a.x - D(7) * a.y - D(1);
let im_u = D(3) * a.y + D(7) * a.x - D(5);
let re_v = -D(2) * a.x + D(6) * a.y + D(0);
let im_v = -D(2) * a.y - D(6) * a.x + D(4);
let den = re_v * re_v + im_v * im_v;
if (AMOUNT != 0.0 && den != 0.0) {
  let rad_v = 1.0 / (AMOUNT * den);
  a.x = rad_v * (re_u * re_v + im_u * im_v);
  a.y = rad_v * (im_u * re_v - re_u * im_v);
}",
};

// ringtile: d = [power, tilesize, width1, width2, outline]
const RINGTILE_PARAMS: &[ParamDef] = &[
    ParamDef::new("power", 5.0),
    ParamDef::new("tilesize", 0.5),
    ParamDef::new("width1", 0.5),
    ParamDef::new("width2", 0.3),
    ParamDef::new("outline", 0.000001),
];

static RINGTILE_INV: VarDef = VarDef {
    name: "ringtile_inv",
    params: RINGTILE_PARAMS,
    kind: VarKind::Pre,
    is_3d: false,
    apply: |d, ctx, a, v, _| {
        let yr1 = d[4] * d[3];
        let yh = if ctx.random() < 0.5 { d[0] } else { -d[0] };
        v.y += d[1] * (a.y + java_round(yh * ctx.random().ln()));
        if a.x > d[3] {
            v.x += d[2] * (-d[3] + fmod(a.x + d[3], yr1));
        } else if a.x < -d[3] {
            v.x += d[2] * (d[3] - fmod(d[3] - a.x, yr1));
        } else {
            v.x += d[2] * a.x;
        }
    },
    prepare: None,
    wgsl: "let yr1 = D(4) * D(3);
var yh = -D(0);
if (rnd() < 0.5) { yh = D(0); }
v.y += D(1) * (a.y + java_round(yh * log(max(rnd(), 1e-30))));
if (a.x > D(3)) {
  v.x += D(2) * (-D(3) + fmod(a.x + D(3), yr1));
} else if (a.x < -D(3)) {
  v.x += D(2) * (D(3) - fmod(D(3) - a.x, yr1));
} else {
  v.x += D(2) * a.x;
}",
};

// ---------------------------------------------------------------------------------------------
// primitives_wf

fn primitives_wf_inside_unit_triangle(px: f64, py: f64) -> bool {
    let (v0x, v0y, v1x, v1y) = (0.0, 1.0, 1.0, 0.0);
    let dot00 = v0x * v0x + v0y * v0y;
    let dot01 = v0x * v1x + v0y * v1y;
    let dot02 = v0x * px + v0y * py;
    let dot11 = v1x * v1x + v1y * v1y;
    let dot12 = v1x * px + v1y * py;
    let inv_denom = 1.0 / (dot00 * dot11 - dot01 * dot01);
    let u = (dot11 * dot02 - dot01 * dot12) * inv_denom;
    let v = (dot00 * dot12 - dot01 * dot02) * inv_denom;
    u >= 0.0 && v >= 0.0 && u + v < 1.0
}

const PRIMITIVES_WF_P_SINA: f64 = 0.866_025_403_784_438_6; // sqrt(3) / 2
const PRIMITIVES_WF_P_COSA: f64 = 0.5;

/// Edge-or-interior coordinate of the square/cube shapes.
#[inline]
fn primitives_wf_edge(ctx: &mut Ctx, size: f64, filled: i32, flag: bool) -> f64 {
    if filled == 0 || ctx.random() < 0.05 {
        size * if flag { -0.5 } else { 0.5 }
    } else {
        size * (ctx.random() - 0.5)
    }
}

fn primitives_wf_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    // d = [shape, a, b, filled]
    let filled = d[3] as i32;
    let size = amount;
    match d[0] as i32 {
        0 => {
            // disc
            let radius = if filled == 0 || ctx.random() < 0.05 { amount } else { amount * ctx.random() };
            let (sina, cosa) = (ctx.random() * 2.0 * M_PI).sin_cos();
            v.x += radius * cosa;
            v.y += radius * sina;
            ctx.pass_z(a, v, amount);
        }
        2 => {
            // cube
            let (mut dx, mut dy, mut dz) = (0.0, 0.0, 0.0);
            match ctx.random_int(3) {
                0 => {
                    let left = ctx.random() < 0.5;
                    dx = primitives_wf_edge(ctx, size, filled, left);
                    dy = size * (ctx.random() - 0.5);
                    dz = size * (ctx.random() - 0.5);
                }
                1 => {
                    let top = ctx.random() < 0.5;
                    dx = size * (ctx.random() - 0.5);
                    dy = primitives_wf_edge(ctx, size, filled, top);
                    dz = size * (ctx.random() - 0.5);
                }
                2 => {
                    let front = ctx.random() < 0.5;
                    dx = size * (ctx.random() - 0.5);
                    dy = size * (ctx.random() - 0.5);
                    dz = primitives_wf_edge(ctx, size, filled, front);
                }
                _ => {}
            }
            v.x += dx;
            v.y += dy;
            v.z += dz;
        }
        3 => {
            // square
            let (mut dx, mut dy) = (0.0, 0.0);
            if filled == 1 && ctx.random() < 0.5 {
                dx = size * (ctx.random() - 0.5);
                dy = size * (ctx.random() - 0.5);
            } else {
                match ctx.random_int(2) {
                    0 => {
                        let left = ctx.random() < 0.5;
                        dx = primitives_wf_edge(ctx, size, filled, left);
                        dy = size * (ctx.random() - 0.5);
                    }
                    1 => {
                        let top = ctx.random() < 0.5;
                        dx = size * (ctx.random() - 0.5);
                        dy = primitives_wf_edge(ctx, size, filled, top);
                    }
                    _ => {}
                }
            }
            v.x += dx;
            v.y += dy;
            ctx.pass_z(a, v, amount);
        }
        4 => {
            // triangle (not scaled by amount, like Java)
            let dx = ctx.random();
            let dy = ctx.random();
            if primitives_wf_inside_unit_triangle(dx, dy) {
                v.x += if ctx.random() < 0.5 { dx } else { -dx };
                v.y += -dy;
            }
            ctx.pass_z(a, v, amount);
        }
        5 => {
            // pyramid
            let mut tx = ctx.random();
            let mut ty = ctx.random();
            if primitives_wf_inside_unit_triangle(tx, ty) {
                tx *= size;
                ty *= size;
                let (mut dx, mut dy, mut dz) = (0.0, 0.0, 0.0);
                match ctx.random_int(5) {
                    0 => {
                        dx = size * (ctx.random() - 0.5);
                        dy = size * 0.5;
                        dz = size * (ctx.random() - 0.5);
                    }
                    1 => {
                        dx = -size * 0.5 + PRIMITIVES_WF_P_COSA * ty;
                        dy = size * 0.5 - PRIMITIVES_WF_P_SINA * ty;
                        dz = (if ctx.random() < 0.5 { tx } else { -tx }) * 0.5;
                    }
                    2 => {
                        dx = size * 0.5 - PRIMITIVES_WF_P_COSA * ty;
                        dy = 0.5 * size - PRIMITIVES_WF_P_SINA * ty;
                        dz = (if ctx.random() < 0.5 { tx } else { -tx }) * 0.5;
                    }
                    3 => {
                        dx = (if ctx.random() < 0.5 { tx } else { -tx }) * 0.5;
                        dy = 0.5 * size - PRIMITIVES_WF_P_SINA * ty;
                        dz = -size * 0.5 + PRIMITIVES_WF_P_COSA * ty;
                    }
                    4 => {
                        dx = (if ctx.random() < 0.5 { tx } else { -tx }) * 0.5;
                        dy = 0.5 * size - PRIMITIVES_WF_P_SINA * ty;
                        dz = size * 0.5 - PRIMITIVES_WF_P_COSA * ty;
                    }
                    _ => {}
                }
                v.x += dx;
                v.y += dy;
                v.z += dz;
            }
        }
        6 => {
            // torus
            let pa = d[1];
            let pb = d[2];
            let radius = if filled == 0 || ctx.random() < 0.05 { pa } else { pa * ctx.random() };
            let alpha = ctx.random() * 2.0 * M_PI;
            let x = radius * alpha.cos();
            let y = radius * alpha.sin();
            let beta = ctx.random() * 2.0 * M_PI;
            v.y += x * amount;
            v.x += (y + pb + pb) * beta.sin() * amount;
            v.z += (y + pb + pb) * beta.cos() * amount;
        }
        _ => {
            // sphere
            let radius = if filled == 0 || ctx.random() < 0.05 { amount } else { amount * ctx.random() };
            let (sina, cosa) = (ctx.random() * 2.0 * M_PI).sin_cos();
            let (sinb, cosb) = (ctx.random() * 2.0 * M_PI).sin_cos();
            v.x += radius * sina * cosb;
            v.y += radius * sina * sinb;
            v.z += radius * cosa;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// pyramid3D

fn pyramid3d_sd_pyramid(px: f64, py: f64, pz: f64, h: f64) -> f64 {
    let m2 = h * h + 0.25;
    let mut x = px.abs();
    let mut z = pz.abs();
    if z > x {
        // Java quirk: `p.x = p.z; p.z = p.x;` (no swap, z stays)
        x = z;
    }
    x -= 0.5;
    z -= 0.5;
    let (qx, qy, qz) = (z, h * py - 0.5 * x, h * x + 0.5 * py);
    let s = java_max(-qx, 0.0);
    let t = limit_value((qy - 0.5 * z) / (m2 + 0.25), 0.0, 1.0);
    let a = m2 * (qx + s) * (qx + s) + qy * qy;
    let b = m2 * (qx + 0.5 * t) * (qx + 0.5 * t) + (qy - m2 * t) * (qy - m2 * t);
    let d2 = if java_min(qy, -qx * m2 - qy * 0.5) > 0.0 { 0.0 } else { java_min(a, b) };
    ((d2 + qz * qz) / m2).sqrt() * sgn(java_max(qz, -py))
}

// ---------------------------------------------------------------------------------------------
// pRose3D
// d = [l, k, c, z1, z2, refSc, opt, optSc, opt3, transp, dist, wagsc, srvsc, f, wigsc, offset, cycle]

fn prose3d_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let mut length = d[0];
    let mut num_petals = d[1];
    let constant = d[2];
    let (scale_z1, scale_z2) = (d[3], d[4]);
    let mut smooth12 = d[6].abs();
    let mut smooth3 = d[8].abs();
    let op_scale = d[7].abs();
    let mut ghost = sqr(d[9]);
    let wag_scale = d[11];
    let curve_scale = d[12];
    let frequency = d[13] * M_2PI;
    let wig_scale = d[14] * 0.5;
    let cycle = d[16];
    let rad = (sqr(a.x) + sqr(a.y)).sqrt();
    let curve1 = rad / length;
    let curve2 = sqr(rad / length);
    let curve3 = (rad - length * 0.5) / length;
    let curve4 = sqr(sqr(rad / length));
    if smooth12 > 2.0 {
        smooth12 = 2.0;
    }
    let anti_opt1 = 2.0 - smooth12;
    let ghost_prep = ctx.random();
    let th = a.y.atan2(a.x);
    let (sth, cth) = th.sin_cos();
    let mut opt_dir = 1.0_f64.copysign(d[6]);
    let petals_sign = 1.0_f64.copysign(d[1]);
    if d[8] < 0.0 {
        opt_dir = -1.0;
    }
    if smooth3 > 1.0 {
        smooth3 = 1.0;
    }
    if length == 0.0 {
        length = 0.000001;
    }
    if num_petals.abs() < 0.0001 {
        num_petals = 0.0001 * petals_sign;
    }
    let pos_neg = if ctx.random() < 0.5 { -1 } else { 1 };
    let pang = if cycle == 0.0 { th / cycle + EPSILON } else { th / cycle };
    let wig = pang * frequency * 0.5 + d[15] * cycle;
    let (wag, wag3) = if opt_dir < 0.0 {
        (
            (curve1 * M_PI * op_scale).sin() + wag_scale * 0.4 * rad + curve_scale * 0.5 * (curve2 * M_PI).sin(),
            (curve4 * M_PI * op_scale).sin() + wag_scale * sqr(rad) * 0.4 + curve_scale * 0.5 * (curve3 * M_PI).cos(),
        )
    } else {
        (
            (curve1 * M_PI * op_scale).sin() + wag_scale * 0.4 * rad + curve_scale * 0.5 * (curve3 * M_PI).cos(),
            (curve4 * M_PI * op_scale).sin() + wag_scale * sqr(rad) * 0.4 + curve_scale * 0.5 * (curve2 * M_PI).sin(),
        )
    };
    let wag2 = (curve2 * M_PI * op_scale).sin() + wag_scale * 0.4 * rad + curve_scale * 0.5 * (curve3 * M_PI).cos();
    let mut wag12 = 0.0;
    if smooth12 <= 1.0 {
        wag12 = wag;
    } else if smooth12 <= 2.0 && smooth12 > 1.0 {
        wag12 = wag2 * (1.0 - anti_opt1) + wag * anti_opt1;
    } else if smooth12 > 2.0 {
        wag12 = wag2;
    }
    let mut waggle = 0.0;
    if smooth3 == 0.0 {
        waggle = wag12;
    } else if smooth3 > 0.0 {
        waggle = wag12 * (1.0 - smooth3) + wag3 * smooth3;
    }
    if opt_dir > 0.0 {
        ghost = 0.0;
    }
    let petal = length * (num_petals * th + constant).cos();
    let wiggle = sqr(rad * 0.5) * wig.sin() * wig_scale;
    if ghost_prep < ghost && pos_neg < 0 {
        // reflection
        v.x += amount * 0.5 * d[5] * petal * cth;
        v.y += amount * 0.5 * d[5] * petal * sth;
        v.z += amount * -0.5 * ((scale_z2 * waggle + wiggle) + d[10]);
    } else {
        v.x += amount * 0.5 * petal * cth;
        v.y += amount * 0.5 * petal * sth;
        v.z += amount * 0.5 * ((scale_z1 * waggle + wiggle) + d[10]);
    }
}

// ---------------------------------------------------------------------------------------------
// pre_recip
// d = [reciprocalpow, dividepow, sqrtpow, asinhpow, acoshpow, atanhpow, asechpow, acosechpow,
//      acothpow, logpow, exppow, zx_mult, zy_mult, zx_add, zy_add]

fn pre_recip_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (zxm, zym, zxa, zya) = (d[11], d[12], d[13], d[14]);
    let (mut xx, mut yy) = (0.0, 0.0);
    let (mut xr, mut yr) = (a.x, a.y);
    let s = amount * M_2_PI;
    if d[0] != 0.0 {
        let mut z = Complex::new(a.x * zxm + zxa, a.y * zym + zya);
        z.recip();
        z.scale(amount);
        xr = d[0] * z.re;
        yr = d[0] * z.im;
    }
    if d[1] != 0.0 {
        let mut z = Complex::new(xr * zxm + zxa, yr * zym + zya);
        let mut z2 = Complex::new(xr, yr);
        z2.dec();
        z.inc();
        z.div(&z2);
        z.scale(s);
        xr = d[1] * z.re;
        yr = d[1] * z.im;
    }
    if d[2] != 0.0 {
        let mut z = Complex::new(xr * zxm + zxa, yr * zym + zya);
        z.sqrt();
        z.scale(amount);
        let sg = if ctx.random() < 0.5 { 1.0 } else { -1.0 };
        xr = d[2] * sg * z.re;
        yr = d[2] * sg * z.im;
    }
    // (power index, operation, random sign)
    let ops: [(usize, fn(&mut Complex), bool); 8] = [
        (3, Complex::asinh, false),
        (4, Complex::acosh, true),
        (5, Complex::atanh, false),
        (6, Complex::asech, false),
        (7, Complex::acosech, true),
        (8, Complex::acoth, false),
        (9, Complex::log, false),
        (10, Complex::exp, false),
    ];
    for (k, op, rnd_sign) in ops {
        if d[k] != 0.0 {
            let mut z = Complex::new(xr * zxm + zxa, yr * zym + zya);
            op(&mut z);
            z.scale(s);
            let sg = if rnd_sign && ctx.random() >= 0.5 { -1.0 } else { 1.0 };
            xx += d[k] * sg * z.re;
            yy += d[k] * sg * z.im;
        }
    }
    if d[3] + d[4] + d[5] + d[6] + d[7] + d[8] + d[9] + d[10] == 0.0 {
        a.x = xr;
        a.y = yr;
    } else {
        a.x = xx;
        a.y = yy;
    }
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "pre_dcztransl",
        params: &[
            ParamDef::range("x0", 0.0, 0.0, 1.0),
            ParamDef::range("x1", 1.0, 0.0, 1.0),
            ParamDef::new("factor", 1.0),
            ParamDef::int("overwrite", 1.0, 0.0, 1.0),
            ParamDef::int("clamp", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Pre,
        is_3d: true,
        apply: |d, _, a, _, amount| {
            // d = [x0, x1, factor, overwrite, clamp, _x0, _x1_m_x0]
            let mut zf = d[2] * (a.color - d[5]) / d[6];
            if d[4] as i32 != 0 {
                zf = if zf < 0.0 { 0.0 } else if zf > 1.0 { 1.0 } else { zf };
            }
            a.x *= amount;
            a.y *= amount;
            a.z = if d[3] as i32 == 0 { amount * a.z * zf } else { amount * zf };
        },
        prepare: Some(|p, _| {
            let x0 = limit_value(p[0], 0.0, 1.0);
            let x1 = limit_value(p[1], 0.0, 1.0);
            let lo = if x0 < x1 { x0 } else { x1 };
            let hi = if x0 > x1 { x0 } else { x1 };
            let den = if hi - lo == 0.0 { SMALL_EPSILON } else { hi - lo };
            vec![x0, x1, p[2], ftoi_limit(p[3], 0, 1), ftoi_limit(p[4], 0, 1), lo, den]
        }),
        wgsl: "var den = D(6);
if (den == 0.0) { den = EPS; }
var zf = D(2) * (a.color - D(5)) / den;
if (i32(D(4)) != 0) { zf = clamp(zf, 0.0, 1.0); }
a.x = AMOUNT * a.x;
a.y = AMOUNT * a.y;
if (i32(D(3)) == 0) { a.z = AMOUNT * a.z * zf; } else { a.z = AMOUNT * zf; }",
    },
    VarDef {
        name: "pre_disc3d",
        params: &[ParamDef::range("pi", M_PI, 0.0, 6.5)],
        kind: VarKind::Pre,
        is_3d: true,
        apply: |d, _, a, _, amount| {
            let pi = d[0];
            let r = (a.y * a.y + a.x * a.x + SMALL_EPSILON).sqrt();
            let (sr, cr) = (pi * r).sin_cos();
            let vv = amount * a.x.atan2(a.y) / (pi + SMALL_EPSILON);
            a.x = vv * sr;
            a.y = vv * cr;
            a.z = vv * (r * a.z.cos());
        },
        prepare: None,
        wgsl: "let r = sqrt(a.y * a.y + a.x * a.x + EPS);
let ang = D(0) * r;
let vv = AMOUNT * atan2(a.x, a.y) / (D(0) + EPS);
a.x = vv * sin(ang);
a.y = vv * cos(ang);
a.z = vv * (r * cos(a.z));",
    },
    VarDef {
        name: "pre_disc",
        params: &[],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |_, _, a, _, amount| {
            let f = amount * a.atan_xy() / M_PI;
            let r = a.rad();
            a.x = f * (M_PI * r).sin();
            a.y = f * (M_PI * r).cos();
        },
        prepare: None,
        wgsl: "let f = AMOUNT * atan_xy(a) / PI;
let r = rad(a);
a.x = f * sin(PI * r);
a.y = f * cos(PI * r);",
    },
    VarDef {
        name: "pre_falloff3",
        params: &[
            ParamDef::int("blur_type", 0.0, 0.0, 2.0),
            ParamDef::int("blur_shape", 0.0, 0.0, 1.0),
            ParamDef::range("blur_strength", 1.0, 0.0, 2.0),
            ParamDef::range("min_distance", 0.5, 0.0, 2.0),
            ParamDef::int("invert_distance", 0.0, 0.0, 1.0),
            ParamDef::range("mul_x", 1.0, 0.0, 1.0),
            ParamDef::range("mul_y", 1.0, 0.0, 1.0),
            ParamDef::range("mul_z", 0.0, 0.0, 1.0),
            ParamDef::range("mul_c", 0.0, 0.0, 1.0),
            ParamDef::new("center_x", 0.0),
            ParamDef::new("center_y", 0.0),
            ParamDef::new("center_z", 0.0),
            ParamDef::new("alpha", 0.0),
        ],
        kind: VarKind::Pre,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            let vout = pre_falloff3_vout(d, ctx, [a.x, a.y, a.z, a.color]);
            a.x = vout[0] * amount;
            a.y = vout[1] * amount;
            a.z = vout[2] * amount;
            v.color = fmod(vout[3], 1.0).abs();
        },
        prepare: Some(pre_falloff3_prepare),
        wgsl: "fn pre_falloff3_sgnd(x: f32) -> f32 { return select(1.0, -1.0, x < 0.0); }
fn pre_falloff3_log_scale(x: f32) -> f32 {
  if (x == 0.0) { return 0.0; }
  return log((abs(x) + 1.0) * 2.718281828459045) * pre_falloff3_sgnd(x) / 2.718281828459045;
}
fn pre_falloff3_log_map(x: f32) -> f32 {
  if (x == 0.0) { return 0.0; }
  return (2.718281828459045 + log(x * 2.718281828459045)) / 4.0 * pre_falloff3_sgnd(x);
}
@body
let rx = rnd() - 0.5;
let ry = rnd() - 0.5;
let rz = rnd() - 0.5;
let rc = rnd() - 0.5;
let vin = vec4<f32>(a.x, a.y, a.z, a.color);
let dc = abs(vin.xyz - vec3<f32>(D(9), D(10), D(11)));
var radius = length(dc);
if (i32(D(1)) != 0) { radius = max(dc.x, max(dc.y, dc.z)); }
var rb = max(radius, 0.0);
if (i32(D(4)) != 0) { rb = max(1.0 - radius, 0.0); }
let r_max = D(13);
let dist = max((rb - D(3)) * r_max, 0.0);
let alpha = D(12);
var vo = vin;
let bt = i32(D(0));
if (bt == 0) {
  let sigma = dist * ry * TWO_PI;
  let phi = dist * rz * PI;
  let rad0 = dist * rx;
  vo = vec4<f32>(vin.x + D(5) * rad0 * cos(sigma) * cos(phi), vin.y + D(6) * rad0 * cos(sigma) * sin(phi),
    vin.z + D(7) * rad0 * sin(sigma), vin.w + D(8) * dist * rc);
} else if (bt == 1) {
  if (!(vin.x == 0.0 && vin.y == 0.0 && vin.z == 0.0)) {
    let r_in = length(vin.xyz);
    let aa = D(6) * dist + alpha * dist / sqrt(r_max);
    let bb = D(7) * dist;
    let sigma = asin(clamp(vin.z / r_in, -1.0, 1.0)) + bb * rz;
    let phi = atan2(vin.y, vin.x) + aa * ry;
    let r = r_in + D(5) * rx * dist;
    vo = vec4<f32>(r * cos(sigma) * cos(phi), r * cos(sigma) * sin(phi), r * sin(sigma), vin.w + D(8) * rc * dist);
  }
} else {
  var coeff = dist;
  if (r_max > EPSILON) { coeff = dist + alpha * (pre_falloff3_log_map(dist) - dist); }
  vo = vin + vec4<f32>(pre_falloff3_log_map(D(5)) * pre_falloff3_log_scale(rx), pre_falloff3_log_map(D(6)) * pre_falloff3_log_scale(ry),
    pre_falloff3_log_map(D(7)) * pre_falloff3_log_scale(rz), pre_falloff3_log_map(D(8)) * pre_falloff3_log_scale(rc)) * coeff;
}
a.x = vo.x * AMOUNT;
a.y = vo.y * AMOUNT;
a.z = vo.z * AMOUNT;
v.color = abs(fmod(vo.w, 1.0));",
    },
    VarDef {
        name: "pre_flatten",
        params: &[],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |_, _, _, v, amount| {
            // Java quirk: priority -1, but it flattens the variation point (not the affine one).
            let f = java_min(amount.abs(), 1.0);
            v.z *= 1.0 - f;
        },
        prepare: None,
        wgsl: "v.z = v.z * (1.0 - min(abs(AMOUNT), 1.0));",
    },
    VarDef {
        name: "prepost_affine",
        params: PREPOST_AFFINE_PARAMS,
        kind: VarKind::PrePost(&PREPOST_AFFINE_INV),
        is_3d: true,
        apply: |d, _, _, v, amount| {
            if amount == 0.0 {
                return;
            }
            let x = amount * d[0] * v.x;
            let y = amount * d[1] * v.y;
            let z = amount * d[2] * v.z;
            v.x = d[9] * x + d[10] * y + d[11] * z + d[6];
            v.y = d[12] * x + d[13] * y + d[14] * z + d[7];
            v.z = d[15] * x + d[16] * y + d[17] * z + d[8];
        },
        prepare: Some(prepost_affine_prepare),
        wgsl: "if (AMOUNT != 0.0) {
  let x = AMOUNT * D(0) * v.x;
  let y = AMOUNT * D(1) * v.y;
  let z = AMOUNT * D(2) * v.z;
  v.x = D(9) * x + D(10) * y + D(11) * z + D(6);
  v.y = D(12) * x + D(13) * y + D(14) * z + D(7);
  v.z = D(15) * x + D(16) * y + D(17) * z + D(8);
}",
    },
    VarDef {
        name: "prepost_blob",
        params: PREPOST_BLOB_PARAMS,
        kind: VarKind::PrePost(&PREPOST_BLOB_INV),
        is_3d: false,
        apply: |d, _, _, v, amount| {
            let alow = 1.0 - amount * (1.0 - d[0]);
            let ahigh = 1.0 - amount * (1.0 - d[1]);
            let a = v.x.atan2(v.y);
            let r = (v.x * v.x + v.y * v.y).sqrt() * (alow + (ahigh - alow) * (0.5 + 0.5 * (d[2] * a).sin()));
            v.x = a.sin() * r;
            v.y = a.cos() * r;
        },
        prepare: None,
        wgsl: "let alow = 1.0 - AMOUNT * (1.0 - D(0));
let ahigh = 1.0 - AMOUNT * (1.0 - D(1));
let ang = atan2(v.x, v.y);
let r = sqrt(v.x * v.x + v.y * v.y) * (alow + (ahigh - alow) * (0.5 + 0.5 * sin(D(2) * ang)));
v.x = sin(ang) * r;
v.y = cos(ang) * r;",
    },
    VarDef {
        name: "prepost_circlize",
        params: PREPOST_CIRCLIZE_PARAMS,
        kind: VarKind::PrePost(&PREPOST_CIRCLIZE_INV),
        is_3d: false,
        apply: |d, _, _, v, amount| {
            // d = [n, rotation, reverse, rot_rad, pi_n, cospi_n]
            let theta = v.atan_yx();
            let r = v.rad();
            let tr = theta - d[3];
            let factor = d[5] / (tr - d[4] * (2.0 * (d[0] * tr / M_2PI).floor() + 1.0)).cos();
            // transform: reverse == 0 -> uncirclize (r * factor), else circlize (r / factor)
            let target = if d[2] as i32 == 0 { r * factor } else { r / factor };
            let l = (1.0 - amount) * r + amount * target;
            v.x = l * theta.cos();
            v.y = l * theta.sin();
        },
        prepare: Some(prepost_circlize_prepare),
        wgsl: "let theta = atan_yx(v);
let r = rad(v);
let tr = theta - D(3);
let factor = D(5) / cos(tr - D(4) * (2.0 * floor(D(0) * tr / TWO_PI) + 1.0));
var tgt = r / factor;
if (i32(D(2)) == 0) { tgt = r * factor; }
let l = (1.0 - AMOUNT) * r + AMOUNT * tgt;
v.x = l * cos(theta);
v.y = l * sin(theta);",
    },
    VarDef {
        name: "prepost_mobius",
        params: PREPOST_MOBIUS_PARAMS,
        kind: VarKind::PrePost(&PREPOST_MOBIUS_INV),
        is_3d: false,
        apply: |d, _, _, v, amount| {
            // d = [re_a, re_b, re_c, re_d, im_a, im_b, im_c, im_d]
            if amount == 0.0 {
                return;
            }
            let re_u = d[0] * v.x - d[4] * v.y + d[1];
            let im_u = d[0] * v.y + d[4] * v.x + d[5];
            let re_v = d[2] * v.x - d[6] * v.y + d[3];
            let im_v = d[2] * v.y + d[6] * v.x + d[7];
            let den = re_v * re_v + im_v * im_v;
            if den == 0.0 {
                return;
            }
            let rad_v = amount / den;
            v.x = rad_v * (re_u * re_v + im_u * im_v);
            v.y = rad_v * (im_u * re_v - re_u * im_v);
        },
        prepare: None,
        wgsl: "let re_u = D(0) * v.x - D(4) * v.y + D(1);
let im_u = D(0) * v.y + D(4) * v.x + D(5);
let re_v = D(2) * v.x - D(6) * v.y + D(3);
let im_v = D(2) * v.y + D(6) * v.x + D(7);
let den = re_v * re_v + im_v * im_v;
if (AMOUNT != 0.0 && den != 0.0) {
  let rad_v = AMOUNT / den;
  v.x = rad_v * (re_u * re_v + im_u * im_v);
  v.y = rad_v * (im_u * re_v - re_u * im_v);
}",
    },
    PRE_PREPOST_AFFINE,
    VarDef {
        name: "ringtile",
        params: RINGTILE_PARAMS,
        kind: VarKind::PrePost(&RINGTILE_INV),
        is_3d: false,
        apply: |_, ctx, a, v, amount| {
            let mut z = Complex::new(v.x, v.y);
            z.exp();
            v.x = amount * z.re;
            v.y = amount * z.im;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let e = exp(v.x);
v.x = AMOUNT * e * cos(v.y);
v.y = AMOUNT * e * sin(v.y);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "pre_recip",
        params: &[
            ParamDef::new("reciprocalpow", 1.0),
            ParamDef::new("dividepow", 0.0),
            ParamDef::new("sqrtpow", 0.0),
            ParamDef::new("asinhpow", 0.0),
            ParamDef::new("acoshpow", 0.0),
            ParamDef::new("atanhpow", 0.0),
            ParamDef::new("asechpow", 0.0),
            ParamDef::new("acosechpow", 0.0),
            ParamDef::new("acothpow", 0.0),
            ParamDef::new("logpow", 0.0),
            ParamDef::new("exppow", 0.0),
            ParamDef::new("zx_mult", 1.0),
            ParamDef::new("zy_mult", 1.0),
            ParamDef::new("zx_add", 0.0),
            ParamDef::new("zy_add", 0.0),
        ],
        kind: VarKind::Pre,
        is_3d: true,
        apply: pre_recip_apply,
        prepare: None,
        wgsl: "let zm = vec2<f32>(D(11), D(12));
let za = vec2<f32>(D(13), D(14));
let s = AMOUNT * 2.0 / PI;
var xr = vec2<f32>(a.x, a.y);
var acc = vec2<f32>(0.0, 0.0);
if (D(0) != 0.0) { xr = D(0) * cx_scale(cx_recip(vec2<f32>(a.x, a.y) * zm + za), AMOUNT); }
if (D(1) != 0.0) { xr = D(1) * cx_scale(cx_div(cx_inc(xr * zm + za), cx_dec(xr)), s); }
if (D(2) != 0.0) {
  var z = cx_scale(cx_sqrt(xr * zm + za), AMOUNT);
  if (rnd() >= 0.5) { z = -z; }
  xr = D(2) * z;
}
let zi = xr * zm + za;
if (D(3) != 0.0) { acc += D(3) * cx_scale(cx_asinh(zi), s); }
if (D(4) != 0.0) {
  var z = cx_scale(cx_acosh(zi), s);
  if (rnd() >= 0.5) { z = -z; }
  acc += D(4) * z;
}
if (D(5) != 0.0) { acc += D(5) * cx_scale(cx_atanh(zi), s); }
if (D(6) != 0.0) { acc += D(6) * cx_scale(cx_asech(zi), s); }
if (D(7) != 0.0) {
  var z = cx_scale(cx_acosech(zi), s);
  if (rnd() >= 0.5) { z = -z; }
  acc += D(7) * z;
}
if (D(8) != 0.0) { acc += D(8) * cx_scale(cx_acoth(zi), s); }
if (D(9) != 0.0) { acc += D(9) * cx_scale(cx_log(zi), s); }
if (D(10) != 0.0) { acc += D(10) * cx_scale(cx_exp(zi), s); }
if (D(3) + D(4) + D(5) + D(6) + D(7) + D(8) + D(9) + D(10) == 0.0) {
  a.x = xr.x;
  a.y = xr.y;
} else {
  a.x = acc.x;
  a.y = acc.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "pre_rect_wf",
        params: &[ParamDef::new("x0", -0.5), ParamDef::new("x1", 0.5), ParamDef::new("y0", -0.5), ParamDef::new("y1", 0.5)],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |d, ctx, a, _, amount| {
            // d = [x0, x1, y0, y1]
            a.x = amount * (d[0] + (d[1] - d[0]) * ctx.random());
            a.y = amount * (d[2] + (d[3] - d[2]) * ctx.random());
        },
        prepare: None,
        wgsl: "a.x = AMOUNT * (D(0) + (D(1) - D(0)) * rnd());
a.y = AMOUNT * (D(2) + (D(3) - D(2)) * rnd());",
    },
    VarDef {
        name: "pre_rotate_y",
        params: &[],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |_, _, a, _, amount| {
            let (sina, cosa) = (amount * M_PI * 0.5).sin_cos();
            let x = cosa * a.x - sina * a.z;
            a.z = sina * a.x + cosa * a.z;
            a.x = x;
        },
        prepare: None,
        wgsl: "let sina = sin(AMOUNT * HALF_PI);
let cosa = cos(AMOUNT * HALF_PI);
let x = cosa * a.x - sina * a.z;
a.z = sina * a.x + cosa * a.z;
a.x = x;",
    },
    VarDef {
        name: "pre_sinusoidal3d",
        params: &[],
        kind: VarKind::Pre,
        is_3d: true,
        apply: |_, _, a, _, amount| {
            a.x = amount * a.x.sin();
            a.y = amount * a.y.sin();
            a.z = amount * (a.x * a.x).atan2(a.y * a.y) * a.z.cos();
        },
        prepare: None,
        wgsl: "a.x = AMOUNT * sin(a.x);
a.y = AMOUNT * sin(a.y);
a.z = AMOUNT * atan2(a.x * a.x, a.y * a.y) * cos(a.z);",
    },
    VarDef {
        name: "pre_spherical",
        params: &[],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |_, _, a, _, amount| {
            let r = amount / (a.x * a.x + a.y * a.y + SMALL_EPSILON);
            a.x *= r;
            a.y *= r;
        },
        prepare: None,
        wgsl: "let r = AMOUNT / (a.x * a.x + a.y * a.y + EPS);
a.x = a.x * r;
a.y = a.y * r;",
    },
    VarDef {
        name: "pre_spin_z",
        params: &[],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |_, _, a, _, amount| {
            let (rz_sin, rz_cos) = (amount * M_PI_2).sin_cos();
            let y = rz_cos * a.y - rz_sin * a.x;
            a.x = rz_sin * a.y + rz_cos * a.x;
            a.y = y;
        },
        prepare: None,
        wgsl: "let rz_sin = sin(AMOUNT * HALF_PI);
let rz_cos = cos(AMOUNT * HALF_PI);
let y = rz_cos * a.y - rz_sin * a.x;
a.x = rz_sin * a.y + rz_cos * a.x;
a.y = y;",
    },
    VarDef {
        name: "pressure_wave",
        params: &[ParamDef::new("x_freq", 1.0), ParamDef::new("y_freq", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x_freq, y_freq, pwx, ipwx, pwy, ipwy]
            v.x += amount * (a.x + d[3] * (d[2] * a.x).sin());
            v.y += amount * (a.y + d[5] * (d[4] * a.y).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let pw = |f: f64| if f == 0.0 { (1.0, 1.0) } else { (f * M_2PI, 1.0 / (f * M_2PI)) };
            let (pwx, ipwx) = pw(p[0]);
            let (pwy, ipwy) = pw(p[1]);
            vec![p[0], p[1], pwx, ipwx, pwy, ipwy]
        }),
        wgsl: "v.x += AMOUNT * (a.x + D(3) * sin(D(2) * a.x));
v.y += AMOUNT * (a.y + D(5) * sin(D(4) * a.y));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "pre_stabilize",
        params: &[
            ParamDef::int("n", 4.0, 1.0, 250.0),
            // Java default: random seed; fixed here.
            ParamDef::int("seed", 12345.0, 0.0, 1000000.0),
            ParamDef::range("p", 0.1, 0.0, 1.0),
            ParamDef::int("dc", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |d, ctx, a, _, _| {
            // JWildfire restarts on the first call of an instance (here: of a render thread) and
            // then with probability p / 1000. The GPU only does the random restarts.
            let started = ctx.orbits.insert(d.as_ptr() as usize, [1.0, 0.0]).is_some();
            if !started || ctx.random() < d[2] / 1000.0 {
                let n = d[0] as usize;
                let i = ctx.random_int(n).min(n - 1);
                let k = 4 + 3 * i;
                a.x = d[k];
                a.y = d[k + 1];
                a.z = 0.0;
                if d[3] as i32 != 0 {
                    a.color = d[k + 2];
                }
            }
        },
        prepare: Some(|p, _| {
            // d = [n, seed, p, dc, (x, y, c) * n]
            let n = ftoi(p[0]).clamp(1, 100_000);
            let seed = ftoi(p[1]);
            let mut d = vec![n as f64, seed as f64, p[2], ftoi_limit(p[3], 0, 1)];
            let mut rand = JavaRandom::new(seed as i64);
            let mut next_color = 0.5;
            let mut color_delta = 1.0;
            for _ in 0..n {
                let x = rand.next_double() * 2.0 - 1.0;
                let y = rand.next_double() * 2.0 - 1.0;
                d.extend_from_slice(&[x, y, next_color]);
                next_color += color_delta;
                if next_color >= 1.0 {
                    color_delta /= 2.0;
                    next_color = color_delta / 2.0;
                }
            }
            d
        }),
        wgsl: "if (rnd() < D(2) / 1000.0) {
  let n = u32(D(0));
  let i = min(u32(rnd() * D(0)), n - 1u);
  let k = base + 4u + 3u * i;
  a.x = prm[k];
  a.y = prm[k + 1u];
  a.z = 0.0;
  if (i32(D(3)) != 0) { a.color = prm[k + 2u]; }
}",
    },
    VarDef {
        name: "pre_ztranslate",
        params: &[],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |_, _, a, _, amount| {
            a.z += amount;
        },
        prepare: None,
        wgsl: "a.z += AMOUNT;",
    },
    VarDef {
        name: "primitives_wf",
        params: &[
            ParamDef::int("shape", 1.0, 0.0, 6.0),
            ParamDef::range("a", 1.0, 0.1, 2.1),
            ParamDef::range("b", 1.0, 0.0, 2.0),
            ParamDef::int("filled", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: primitives_wf_apply,
        prepare: Some(|p, _| vec![ftoi(p[0]) as f64, p[1], p[2], ftoi_limit(p[3], 0, 1)]),
        wgsl: "fn primitives_wf_inside(px: f32, py: f32) -> bool {
  return py >= 0.0 && px >= 0.0 && px + py < 1.0;
}
fn primitives_wf_edge(size: f32, filled: i32, flag: bool) -> f32 {
  if (filled == 0 || rnd() < 0.05) { return select(0.5, -0.5, flag) * size; }
  return size * (rnd() - 0.5);
}
@body
let shape = i32(D(0));
let filled = i32(D(3));
let size = AMOUNT;
if (shape == 0) {
  var radius = AMOUNT;
  if (!(filled == 0 || rnd() < 0.05)) { radius = AMOUNT * rnd(); }
  let ang = rnd() * TWO_PI;
  v.x += radius * cos(ang);
  v.y += radius * sin(ang);
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
} else if (shape == 2) {
  var dx = 0.0;
  var dy = 0.0;
  var dz = 0.0;
  let side = i32(rnd() * 3.0);
  if (side == 0) {
    let left = rnd() < 0.5;
    dx = primitives_wf_edge(size, filled, left);
    dy = size * (rnd() - 0.5);
    dz = size * (rnd() - 0.5);
  } else if (side == 1) {
    let top = rnd() < 0.5;
    dx = size * (rnd() - 0.5);
    dy = primitives_wf_edge(size, filled, top);
    dz = size * (rnd() - 0.5);
  } else if (side == 2) {
    let front = rnd() < 0.5;
    dx = size * (rnd() - 0.5);
    dy = size * (rnd() - 0.5);
    dz = primitives_wf_edge(size, filled, front);
  }
  v.x += dx;
  v.y += dy;
  v.z += dz;
} else if (shape == 3) {
  var dx = 0.0;
  var dy = 0.0;
  if (filled == 1 && rnd() < 0.5) {
    dx = size * (rnd() - 0.5);
    dy = size * (rnd() - 0.5);
  } else {
    let side = i32(rnd() * 2.0);
    if (side == 0) {
      let left = rnd() < 0.5;
      dx = primitives_wf_edge(size, filled, left);
      dy = size * (rnd() - 0.5);
    } else if (side == 1) {
      let top = rnd() < 0.5;
      dx = size * (rnd() - 0.5);
      dy = primitives_wf_edge(size, filled, top);
    }
  }
  v.x += dx;
  v.y += dy;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
} else if (shape == 4) {
  let dx = rnd();
  let dy = rnd();
  if (primitives_wf_inside(dx, dy)) {
    v.x += select(-dx, dx, rnd() < 0.5);
    v.y += -dy;
  }
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
} else if (shape == 5) {
  var tx = rnd();
  var ty = rnd();
  if (primitives_wf_inside(tx, ty)) {
    tx *= size;
    ty *= size;
    let ps = 0.8660254037844386;
    var dx = 0.0;
    var dy = 0.0;
    var dz = 0.0;
    let side = i32(rnd() * 5.0);
    if (side == 0) {
      dx = size * (rnd() - 0.5);
      dy = size * 0.5;
      dz = size * (rnd() - 0.5);
    } else if (side == 1) {
      dx = -size * 0.5 + 0.5 * ty;
      dy = size * 0.5 - ps * ty;
      dz = select(-tx, tx, rnd() < 0.5) * 0.5;
    } else if (side == 2) {
      dx = size * 0.5 - 0.5 * ty;
      dy = 0.5 * size - ps * ty;
      dz = select(-tx, tx, rnd() < 0.5) * 0.5;
    } else if (side == 3) {
      dx = select(-tx, tx, rnd() < 0.5) * 0.5;
      dy = 0.5 * size - ps * ty;
      dz = -size * 0.5 + 0.5 * ty;
    } else if (side == 4) {
      dx = select(-tx, tx, rnd() < 0.5) * 0.5;
      dy = 0.5 * size - ps * ty;
      dz = size * 0.5 - 0.5 * ty;
    }
    v.x += dx;
    v.y += dy;
    v.z += dz;
  }
} else if (shape == 6) {
  var radius = D(1);
  if (!(filled == 0 || rnd() < 0.05)) { radius = D(1) * rnd(); }
  let alpha = rnd() * TWO_PI;
  let x = radius * cos(alpha);
  let y = radius * sin(alpha);
  let beta = rnd() * TWO_PI;
  v.y += x * AMOUNT;
  v.x += (y + D(2) + D(2)) * sin(beta) * AMOUNT;
  v.z += (y + D(2) + D(2)) * cos(beta) * AMOUNT;
} else {
  var radius = AMOUNT;
  if (!(filled == 0 || rnd() < 0.05)) { radius = AMOUNT * rnd(); }
  let ang = rnd() * TWO_PI;
  let b1 = rnd() * TWO_PI;
  v.x += radius * sin(ang) * cos(b1);
  v.y += radius * sin(ang) * sin(b1);
  v.z += radius * cos(ang);
}",
    },
    VarDef {
        name: "projective",
        params: &[
            ParamDef::new("A", 0.0),
            ParamDef::new("B", -0.4),
            ParamDef::new("C", 1.0),
            ParamDef::new("A1", 1.0),
            ParamDef::new("B1", 0.1),
            ParamDef::new("C1", 0.0),
            ParamDef::new("A2", 0.0),
            ParamDef::new("B2", 1.2),
            ParamDef::new("C2", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let u = d[0] * a.x + d[1] * a.y + d[2];
            v.x += amount * (d[3] * a.x + d[4] * a.y + d[5]) / u;
            v.y += amount * (d[6] * a.x + d[7] * a.y + d[8]) / u;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let u = D(0) * a.x + D(1) * a.y + D(2);
v.x += AMOUNT * (D(3) * a.x + D(4) * a.y + D(5)) / u;
v.y += AMOUNT * (D(6) * a.x + D(7) * a.y + D(8)) / u;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "pRose3D",
        params: &[
            ParamDef::range("l", 1.0, 0.5, 2.0),
            // Java default: 3 + random (0..10 or int 0..14); fixed to 5
            ParamDef::range("k", 5.0, 0.0, 18.0),
            ParamDef::range("c", 0.0, -3.2, 3.2),
            ParamDef::new("z1", 1.0),
            ParamDef::new("z2", 1.0),
            ParamDef::new("refSc", 1.0),
            ParamDef::new("opt", 1.0),
            ParamDef::new("optSc", 1.0),
            ParamDef::range("opt3", 0.0, 0.0, 1.0),
            ParamDef::range("transp", 0.5, 0.0, 1.0),
            ParamDef::range("dist", 1.0, 0.0, 3.0),
            ParamDef::range("wagsc", 0.0, -2.5, 2.5),
            ParamDef::range("srvsc", 0.0, 0.0, 3.0),
            ParamDef::range("f", 3.0, 0.0, 6.0),
            ParamDef::range("wigsc", 0.0, -2.5, 2.5),
            ParamDef::new("offset", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: prose3d_apply,
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            let cycle = if p[1] == 0.0 { M_2PI / (p[1] + 0.000001) } else { M_2PI / p[1] };
            d.push(cycle);
            d
        }),
        wgsl: "var length0 = D(0);
var num_petals = D(1);
let smooth12 = min(abs(D(6)), 2.0);
let smooth3 = min(abs(D(8)), 1.0);
let op_scale = abs(D(7));
var ghost = D(9) * D(9);
let wag_scale = D(11);
let curve_scale = D(12);
let frequency = D(13) * TWO_PI;
let wig_scale = D(14) * 0.5;
let cycle = D(16);
let rd = sqrt(a.x * a.x + a.y * a.y);
let curve1 = rd / length0;
let curve2 = sqr(rd / length0);
let curve3 = (rd - length0 * 0.5) / length0;
let curve4 = sqr(sqr(rd / length0));
let anti_opt1 = 2.0 - smooth12;
let ghost_prep = rnd();
let th = atan2(a.y, a.x);
let sth = sin(th);
let cth = cos(th);
var opt_dir = select(-1.0, 1.0, D(6) >= 0.0);
let petals_sign = select(-1.0, 1.0, D(1) >= 0.0);
if (D(8) < 0.0) { opt_dir = -1.0; }
if (length0 == 0.0) { length0 = 0.000001; }
if (abs(num_petals) < 0.0001) { num_petals = 0.0001 * petals_sign; }
let pos_neg = select(1, -1, rnd() < 0.5);
var pang = th / cycle;
if (cycle == 0.0) { pang = pang + EPSILON; }
let wig = pang * frequency * 0.5 + D(15) * cycle;
var wag = sin(curve1 * PI * op_scale) + wag_scale * 0.4 * rd + curve_scale * 0.5 * cos(curve3 * PI);
var wag3 = sin(curve4 * PI * op_scale) + wag_scale * rd * rd * 0.4 + curve_scale * 0.5 * sin(curve2 * PI);
if (opt_dir < 0.0) {
  wag = sin(curve1 * PI * op_scale) + wag_scale * 0.4 * rd + curve_scale * 0.5 * sin(curve2 * PI);
  wag3 = sin(curve4 * PI * op_scale) + wag_scale * rd * rd * 0.4 + curve_scale * 0.5 * cos(curve3 * PI);
}
let wag2 = sin(curve2 * PI * op_scale) + wag_scale * 0.4 * rd + curve_scale * 0.5 * cos(curve3 * PI);
var wag12 = wag;
if (smooth12 > 1.0) { wag12 = wag2 * (1.0 - anti_opt1) + wag * anti_opt1; }
var waggle = wag12;
if (smooth3 > 0.0) { waggle = wag12 * (1.0 - smooth3) + wag3 * smooth3; }
if (opt_dir > 0.0) { ghost = 0.0; }
let petal = length0 * cos(num_petals * th + D(2));
let wiggle = sqr(rd * 0.5) * sin(wig) * wig_scale;
if (ghost_prep < ghost && pos_neg < 0) {
  v.x += AMOUNT * 0.5 * D(5) * petal * cth;
  v.y += AMOUNT * 0.5 * D(5) * petal * sth;
  v.z += AMOUNT * -0.5 * ((D(4) * waggle + wiggle) + D(10));
} else {
  v.x += AMOUNT * 0.5 * petal * cth;
  v.y += AMOUNT * 0.5 * petal * sth;
  v.z += AMOUNT * 0.5 * ((D(3) * waggle + wiggle) + D(10));
}",
    },
    VarDef {
        name: "pTransform",
        params: &[
            ParamDef::new("rotate", 0.30),
            ParamDef::int("power", 2.0, 1.0, 10.0),
            ParamDef::new("move", 0.40),
            ParamDef::new("split", 0.0),
            ParamDef::int("use_log", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [rotate, power, move, split, use_log]
            let use_log = d[4] as i32 != 0;
            let r = a.rad();
            let mut rho = if use_log { r.ln() / d[1] + d[2] } else { r / d[1] + d[2] };
            let theta = a.atan_yx() + d[0];
            if a.x >= 0.0 {
                rho += d[3];
            } else {
                rho -= d[3];
            }
            if use_log {
                rho = rho.exp();
            }
            v.x += amount * rho * theta.cos();
            v.y += amount * rho * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], ftoi_limit(p[1], 1, i32::MAX), p[2], p[3], ftoi_limit(p[4], 0, 1)]),
        wgsl: "let use_log = i32(D(4)) != 0;
let r = rad(a);
var rho = r / D(1) + D(2);
if (use_log) { rho = log(r) / D(1) + D(2); }
let theta = atan_yx(a) + D(0);
if (a.x >= 0.0) { rho += D(3); } else { rho -= D(3); }
if (use_log) { rho = exp(rho); }
v.x += AMOUNT * rho * cos(theta);
v.y += AMOUNT * rho * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "pulse",
        params: &[
            ParamDef::new("freqx", 2.0),
            ParamDef::new("freqy", 2.0),
            ParamDef::new("scalex", 1.0),
            ParamDef::new("scaley", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * (a.x + d[2] * (a.x * d[0]).sin());
            v.y += amount * (a.y + d[3] * (a.y * d[1]).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * (a.x + D(2) * sin(a.x * D(0)));
v.y += AMOUNT * (a.y + D(3) * sin(a.y * D(1)));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "p_wave",
        params: &[ParamDef::new("freq", 4.0), ParamDef::new("amp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let dist = (a.x * a.x + a.y * a.y).sqrt();
            let compress = 1.0 + d[1] * (d[0] * dist).sin();
            v.x += amount * a.x * compress;
            v.y += amount * a.y * compress;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let compress = 1.0 + D(1) * sin(D(0) * sqrt(a.x * a.x + a.y * a.y));
v.x += AMOUNT * a.x * compress;
v.y += AMOUNT * a.y * compress;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "pyramid3D",
        params: &[ParamDef::range("h", 0.25, 0.0, 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            for _ in 0..50 {
                let x = ctx.random() - 0.5;
                let y = ctx.random() - 0.5;
                let z = ctx.random() - 0.5;
                if pyramid3d_sd_pyramid(x, y, z, d[0]) < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, 0.5)]),
        wgsl: "fn pyramid3D_sd(px: f32, py: f32, pz0: f32, h: f32) -> f32 {
  let m2 = h * h + 0.25;
  var x = abs(px);
  var z = abs(pz0);
  if (z > x) { x = z; }
  x = x - 0.5;
  z = z - 0.5;
  let q = vec3<f32>(z, h * py - 0.5 * x, h * x + 0.5 * py);
  let s = max(-q.x, 0.0);
  let t = clamp((q.y - 0.5 * z) / (m2 + 0.25), 0.0, 1.0);
  let aa = m2 * (q.x + s) * (q.x + s) + q.y * q.y;
  let bb = m2 * (q.x + 0.5 * t) * (q.x + 0.5 * t) + (q.y - m2 * t) * (q.y - m2 * t);
  var d2 = min(aa, bb);
  if (min(q.y, -q.x * m2 - q.y * 0.5) > 0.0) { d2 = 0.0; }
  return sqrt((d2 + q.z * q.z) / m2) * sign(max(q.z, -py));
}
@body
var found = false;
for (var i = 0; i < 50; i++) {
  let x = rnd() - 0.5;
  let y = rnd() - 0.5;
  let z = rnd() - 0.5;
  if (pyramid3D_sd(x, y, z, D(0)) < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * x;
    v.y += AMOUNT * y;
    v.z += AMOUNT * z;
    found = true;
    break;
  }
}
if (!found) { v.hide = 1.0; }",
    },
    VarDef {
        name: "pyramid",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |_, _, a, v, amount| {
            let x = a.x * a.x * a.x;
            let y = a.y * a.y * a.y;
            let z = (a.z * a.z * a.z).abs();
            let r = amount / (x.abs() + y.abs() + z + 0.000000001);
            v.x += x * r;
            v.y += y * r;
            v.z += z * r;
        },
        prepare: None,
        wgsl: "let x = a.x * a.x * a.x;
let y = a.y * a.y * a.y;
let z = abs(a.z * a.z * a.z);
let r = AMOUNT / (abs(x) + abs(y) + z + 0.000000001);
v.x += x * r;
v.y += y * r;
v.z += z * r;",
    },
    VarDef {
        name: "q_ode",
        // Java defaults of q_ode03..06 and q_ode09..12 are random in [-1, 1]; fixed values here.
        params: &[
            ParamDef::new("q_ode01", 0.0),
            ParamDef::new("q_ode02", 1.0),
            ParamDef::range("q_ode03", -0.2, -1.0, 1.0),
            ParamDef::range("q_ode04", 0.3, -1.0, 1.0),
            ParamDef::range("q_ode05", -0.4, -1.0, 1.0),
            ParamDef::range("q_ode06", 0.1, -1.0, 1.0),
            ParamDef::new("q_ode07", 0.0),
            ParamDef::new("q_ode08", 1.0),
            ParamDef::range("q_ode09", 0.25, -1.0, 1.0),
            ParamDef::range("q_ode10", -0.15, -1.0, 1.0),
            ParamDef::range("q_ode11", 0.5, -1.0, 1.0),
            ParamDef::range("q_ode12", -0.3, -1.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x, y) = (a.x, a.y);
            v.x += d[0] + amount * d[1] * x + d[2] * x * x;
            v.x += d[3] * x * y + d[4] * y + d[5] * y * y;
            v.y += d[6] + d[7] * x + d[8] * x * x;
            v.y += d[9] * x * y + amount * d[10] * y + d[11] * y * y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = a.x;
let y = a.y;
v.x += D(0) + AMOUNT * D(1) * x + D(2) * x * x;
v.x += D(3) * x * y + D(4) * y + D(5) * y * y;
v.y += D(6) + D(7) * x + D(8) * x * x;
v.y += D(9) * x * y + AMOUNT * D(10) * y + D(11) * y * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "pre_dcztransl",
        "pre_disc3d",
        "pre_disc",
        "pre_falloff3",
        "pre_flatten",
        "prepost_affine",
        "prepost_blob",
        "prepost_circlize",
        "prepost_mobius",
        "pre_prepost_affine",
        "ringtile",
        "pre_recip",
        "pre_rect_wf",
        "pre_rotate_y",
        "pre_sinusoidal3d",
        "pre_spherical",
        "pre_spin_z",
        "pressure_wave",
        "pre_stabilize",
        "pre_ztranslate",
        "primitives_wf",
        "projective",
        "pRose3D",
        "pTransform",
        "pulse",
        "p_wave",
        "pyramid3D",
        "pyramid",
        "q_ode",
    ];

    /// Registered without a GPU implementation.
    const CPU_ONLY: &[&str] = &[];

    const POINTS: &[(f64, f64, f64)] = &[
        (0.3, 0.2, 0.1),
        (-0.7, 0.5, -0.2),
        (1.3, -0.9, 0.4),
        (0.05, -0.02, 0.0),
        (-2.1, -1.7, 1.1),
        (0.6, 0.0, -0.5),
    ];

    fn find(name: &str) -> &'static VarDef {
        DEFS.iter().find(|d| d.name == name).unwrap()
    }

    fn run(def: &VarDef, data: &[f64], ctx: &mut Ctx, p: (f64, f64, f64)) -> (Pt, Pt) {
        let mut a = Pt { x: p.0, y: p.1, z: p.2, color: 0.5, ..Default::default() };
        let mut v = Pt { x: 0.1, y: -0.2, z: 0.05, color: 0.5, ..Default::default() };
        (def.apply)(data, ctx, &mut a, &mut v, 0.8);
        (a, v)
    }

    #[test]
    fn all_names_present_and_unique() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert_eq!(DEFS.iter().filter(|d| d.name == *n).count(), 1, "{n}");
        }
        for def in DEFS {
            assert_eq!(def.supports_gpu(), !CPU_ONLY.contains(&def.name), "{}", def.name);
        }
    }

    #[test]
    fn default_params_give_finite_results() {
        let mut ctx = Ctx::new(42, true);
        for def in DEFS {
            let data = def.prepare_data(&[], &Affine::IDENTITY);
            for &p in POINTS {
                for _ in 0..50 {
                    let (a, v) = run(def, &data, &mut ctx, p);
                    assert!(
                        !a.is_bad() && !v.is_bad() && v.color.is_finite() && a.color.is_finite(),
                        "{} at {:?}: a={:?} v={:?}",
                        def.name,
                        p,
                        a,
                        v
                    );
                }
            }
        }
    }

    #[test]
    fn primitives_wf_all_shapes_finite() {
        let def = find("primitives_wf");
        let mut ctx = Ctx::new(5, false);
        for shape in 0..7 {
            for filled in 0..2 {
                let data = def.prepare_data(&[shape as f64, 1.0, 1.0, filled as f64], &Affine::IDENTITY);
                for &p in POINTS {
                    for _ in 0..50 {
                        let (_, v) = run(def, &data, &mut ctx, p);
                        assert!(!v.is_bad(), "shape {shape} filled {filled}");
                    }
                }
            }
        }
    }

    #[test]
    fn prepost_affine_inverse_roundtrip() {
        let params = [1.5, 0.7, 2.0, 30.0, -20.0, 45.0, 0.3, -0.2, 0.1];
        let fwd = find("prepost_affine");
        let inv = find("pre_prepost_affine");
        let dp = fwd.prepare_data(&params, &Affine::IDENTITY);
        let di = inv.prepare_data(&params, &Affine::IDENTITY);
        let mut ctx = Ctx::new(1, false);
        for &p in POINTS {
            let mut a = Pt { x: p.0, y: p.1, z: p.2, ..Default::default() };
            let mut v = Pt::default();
            (inv.apply)(&di, &mut ctx, &mut a, &mut v, 0.8);
            let mut w = Pt { x: a.x, y: a.y, z: a.z, ..Default::default() };
            let mut dummy = Pt::default();
            (fwd.apply)(&dp, &mut ctx, &mut dummy, &mut w, 0.8);
            assert!((w.x - p.0).abs() < 1e-9 && (w.y - p.1).abs() < 1e-9 && (w.z - p.2).abs() < 1e-9, "{p:?} -> {w:?}");
        }
    }

    /// The pre slot runs `invtransform`, the post slot `transform`: for these parameters the
    /// pair is the identity.
    #[test]
    fn prepost_inverse_then_forward_is_identity() {
        let cases: &[(&str, &[f64])] = &[
            ("prepost_affine", &[1.5, 0.7, 2.0, 30.0, -20.0, 45.0, 0.3, -0.2, 0.1]),
            ("prepost_blob", &[0.6, 1.4, 5.0]),
            ("prepost_circlize", &[5.0, 20.0, 0.0]),
            ("prepost_circlize", &[7.0, -40.0, 1.0]),
            ("prepost_mobius", &[1.2, 0.3, 0.4, 0.9, -0.2, 0.1, 0.25, 0.05]),
        ];
        let mut ctx = Ctx::new(1, false);
        for &(name, params) in cases {
            let fwd = find(name);
            let VarKind::PrePost(inv) = fwd.kind else { panic!("{name} is not prepost") };
            let d = fwd.prepare_data(params, &Affine::IDENTITY);
            for &p in POINTS {
                let mut a = Pt { x: p.0, y: p.1, z: p.2, ..Default::default() };
                let mut v = Pt::default();
                (inv.apply)(&d, &mut ctx, &mut a, &mut v, 1.0);
                let mut w = Pt { x: a.x, y: a.y, z: a.z, ..Default::default() };
                let mut dummy = Pt::default();
                (fwd.apply)(&d, &mut ctx, &mut dummy, &mut w, 1.0);
                let z_ok = !fwd.is_3d || (w.z - p.2).abs() < 1e-9;
                assert!((w.x - p.0).abs() < 1e-9 && (w.y - p.1).abs() < 1e-9 && z_ok, "{name} {p:?} -> {w:?}");
            }
        }
    }

    #[test]
    fn pre_stabilize_table_matches_java_random() {
        let def = find("pre_stabilize");
        let d = def.prepare_data(&[3.0, 0.0, 1000.0, 1.0], &Affine::IDENTITY);
        assert_eq!(d.len(), 4 + 9);
        // new java.util.Random(0).nextDouble() = 0.730967787376657
        assert!((d[4] - (0.730967787376657 * 2.0 - 1.0)).abs() < 1e-12);
        assert_eq!([d[6], d[9], d[12]], [0.5, 0.25, 0.75]);
        // p = 1000 -> always restarts onto a table point
        let mut ctx = Ctx::new(9, false);
        let (a, _) = run(def, &d, &mut ctx, (5.0, 5.0, 5.0));
        assert!(a.x.abs() <= 1.0 && a.y.abs() <= 1.0 && a.z == 0.0);
    }
}
