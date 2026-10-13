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

//! Variations ported from the JWildfire sources (batch w12).

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;
use super::complex::Complex;
use crate::brush::Marsaglia;
use std::cell::RefCell;
use std::collections::HashMap;

/// `MarsagliaRandomGenerator.randomize(seed)` followed by one `random()`.
fn marsaglia1(seed: i32) -> f64 {
    Marsaglia::new(seed as i64).random()
}

/// Point buffer of `PointMirrorSymmetryFunc` (one per prepared instance and render thread).
struct PmsBuffer {
    pts: Vec<[f64; 4]>,
    index: usize,
    valid: bool,
}

thread_local! {
    static PMS_BUFFERS: RefCell<HashMap<usize, PmsBuffer>> = RefCell::new(HashMap::new());
}

/// `PolylogarithmFunc.RiemannInt.Z` (table literals as in Java)
#[allow(clippy::excessive_precision)]
fn riemann_z(n: i32) -> f64 {
    const NEG_ODD: [f64; 100] = [
        -0.08333333333333333, 0.008333333333333333, -0.003968253968253968, 0.004166666666666667, -0.007575757575757576, 0.021092796092796094,
        -0.08333333333333333, 0.4432598039215686, -3.0539543302701198, 26.456212121212122, -281.46014492753625, 3607.5105463980462,
        -54827.583333333336, 974936.8238505747, -2.005269579668808e+7, 4.723848677216299e+8, -1.2635724795916667e+10, 3.8087931125245367e+11,
        -1.2850850499305083e+13, 4.824144835485017e+14, -2.0040310656516253e+16, 9.167743603195331e+17, -4.5979888343656505e+19,
        2.518047192145109e+21, -1.500173349215393e+23, 9.689957887463594e+24, -6.764588237929281e+26, 5.089065946866229e+28,
        -4.114728879255798e+30, 3.5666582095375554e+32, -3.306608987657758e+34, 3.2715634236478713e+36, -3.4473782558278054e+38,
        3.861427983270526e+40, -4.589297443245433e+42, 5.777538634277042e+44, -7.691985875950713e+46, 1.0813635449971654e+49,
        -1.6029364522008965e+51, 2.501947904156046e+53, -4.106705233581021e+55, 7.079877440849458e+57, -1.280454688793951e+60,
        2.4267340392333522e+62, -4.8143218874045757e+64, 9.987557417572751e+66, -2.1645634868435182e+69, 4.8962327039620546e+71,
        -1.1549023923963518e+74, 2.83822495706937e+76, -7.26120088036067e+78, 1.932351423341981e+81, -5.345016042528861e+83,
        1.535602884642242e+86, -4.5789872682265786e+88, 1.4162025212194806e+91, -4.540065229609264e+93, 1.5076656758807855e+96,
        -5.183094914826456e+98, 1.843564742725653e+101, -6.780555475309095e+103, 2.57733267027546e+106, -1.01191128757046e+109,
        4.101634616154228e+111, -1.715524453403202e+114, 7.400342570526908e+116, -3.290922535705444e+119, 1.507983153416477e+122,
        -7.116987918825454e+124, 3.458042914157776e+127, -1.729090760667674e+130, 8.893699169503295e+132, -4.7038470619636e+135,
        2.55719382310602e+138, -1.428406750044352e+141, 8.195215221831376e+143, -4.827648542272736e+146, 2.918961237477031e+149,
        -1.81089321625689e+152, 1.152357722002117e+155, -7.519231195198174e+157, 5.029401657641103e+160, -3.447342044447767e+163,
        2.420745864586851e+166, -1.740946592037767e+169, 1.281948986348224e+172, -9.662412110856088e+174, 7.452691030430086e+177,
        -5.880839331167436e+180, 4.74627186549076e+183, -3.916913259477282e+186, 3.304507144322603e+189, -2.849289055099457e+192,
        2.510332934507758e+195, -2.259390199547524e+198, 2.07691380042876e+201, -1.949473217492725e+204, 1.868073147126591e+207,
        -1.827075266281457e+210, 1.823538632259567e+213,
    ];
    const POS_INT: [f64; 34] = [
        -0.5, 0.0, 1.64493406684822643647241516665, 1.20205690315959428539973816151, 1.08232323371113819151600369654,
        1.03692775514336992633136548646, 1.01734306198444913971451792979, 1.00834927738192282683979754985, 1.00407735619794433937868523851,
        1.00200839282608221441785276923, 1.00099457512781808533714595890, 1.00049418860411946455870228253, 1.00024608655330804829863799805,
        1.00012271334757848914675183653, 1.00006124813505870482925854511, 1.00003058823630702049355172851, 1.00001528225940865187173257149,
        1.00000763719763789976227360029, 1.00000381729326499983985646164, 1.00000190821271655393892565696, 1.00000095396203387279611315204,
        1.00000047693298678780646311672, 1.00000023845050272773299000365, 1.00000011921992596531107306779, 1.00000005960818905125947961244,
        1.00000002980350351465228018606, 1.00000001490155482836504123466, 1.00000000745071178983542949198, 1.00000000372533402478845705482,
        1.00000000186265972351304900640, 1.00000000093132743241966818287, 1.00000000046566290650337840730, 1.00000000023283118336765054920,
        1.00000000011641550172700519776,
    ];
    if n > 33 {
        return 1.0;
    }
    if n < -200 {
        return 1e250;
    }
    if n < 0 {
        if (-n) % 2 == 0 {
            return 0.0;
        }
        return NEG_ODD[((-n - 1) / 2) as usize];
    }
    POS_INT[n as usize]
}

/// `cern.jet.math.Arithmetic.longFactorial` as a double (`k` in 0..=20).
fn long_factorial(k: i32) -> f64 {
    (1..=k.max(0) as i64).product::<i64>() as f64
}

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "pleat",
        params: &[ParamDef::new("width", 0.5), ParamDef::new("depth", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let w = java_max(d[0].abs(), 0.01);
            let cell = (a.x / w).floor();
            let lx = a.x / w - cell;
            let crease = d[1] * (1.0 - (2.0 * lx - 1.0).abs());
            let mut row = fmod(cell, 2.0);
            if row < 0.0 {
                row += 2.0;
            }
            let dir = if row > 0.5 { 1.0 } else { -1.0 };
            v.x += amount * a.x;
            v.y += amount * (a.y + crease * dir);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let w = max(abs(D(0)), 0.01);
let cell = floor(a.x / w);
let lx = a.x / w - cell;
let crease = D(1) * (1.0 - abs(2.0 * lx - 1.0));
var row = fmod(cell, 2.0);
if (row < 0.0) { row += 2.0; }
var dir = -1.0;
if (row > 0.5) { dir = 1.0; }
v.x += AMOUNT * a.x;
v.y += AMOUNT * (a.y + crease * dir);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "plusrecip",
        params: &[ParamDef::new("ar", 4.0), ParamDef::new("ai", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let mut z = Complex::new(a.x, a.y);
            let mut k = Complex::copy_of(&z);
            let mut ca = Complex::new(d[0], d[1]);
            let aa = ca.mag2eps().sqrt();
            k.sqr();
            k.sub(&ca);
            k.sqrt();
            k.add(&z);
            z.copy(&k);
            z.sqr();
            if z.mag2eps().sqrt() < aa {
                k.conj();
                ca.scale(-1.0 / aa);
                k.mul(&ca);
            }
            if k.re < 0.0 {
                k.neg();
            }
            k.scale(amount);
            v.x += k.re;
            v.y += k.im;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let z = vec2<f32>(a.x, a.y);
let ca = vec2<f32>(D(0), D(1));
let aa = sqrt(cx_mag2eps(ca));
var k = cx_add(cx_sqrt(cx_sub(cx_sqr(z), ca)), z);
if (sqrt(cx_mag2eps(cx_sqr(k))) < aa) {
  k = cx_mul(cx_conj(k), cx_scale(ca, -1.0 / aa));
}
if (k.x < 0.0) { k = cx_neg(k); }
k = cx_scale(k, AMOUNT);
v.x += k.x;
v.y += k.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "poincare3D",
        params: &[ParamDef::new("r", 0.0), ParamDef::new("a", 0.0), ParamDef::new("b", 0.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            // d = [r, a, b, cx, cy, cz, c2, c2x, c2y, c2z, s2x, s2y, s2z]
            let r2 = sqr(a.x) + sqr(a.y) + sqr(a.z);
            let (x2cx, y2cy, z2cz) = (d[7] * a.x, d[8] * a.y, d[9] * a.z);
            let dd = amount / (d[6] * r2 - x2cx - y2cy - z2cz + 1.0);
            v.x += dd * (a.x * d[10] + d[3] * (y2cy + z2cz - r2 - 1.0));
            v.y += dd * (a.y * d[11] + d[4] * (x2cx + z2cz - r2 - 1.0));
            v.z += dd * (a.z * d[12] + d[5] * (y2cy + x2cx - r2 - 1.0));
        },
        prepare: Some(|p, _| {
            let cx = -p[0] * (p[1] * M_PI_2).cos() * (p[2] * M_PI_2).cos();
            let cy = p[0] * (p[1] * M_PI_2).sin() * (p[2] * M_PI_2).cos();
            let cz = -p[0] * (p[2] * M_PI_2).sin();
            let c2 = sqr(cx) + sqr(cy) + sqr(cz);
            let s2x = sqr(cx) - sqr(cy) - sqr(cz) + 1.0;
            let s2y = sqr(cy) - sqr(cx) - sqr(cz) + 1.0;
            let s2z = sqr(cz) - sqr(cy) - sqr(cx) + 1.0;
            vec![p[0], p[1], p[2], cx, cy, cz, c2, 2.0 * cx, 2.0 * cy, 2.0 * cz, s2x, s2y, s2z]
        }),
        wgsl: "let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
let x2cx = D(7) * a.x;
let y2cy = D(8) * a.y;
let z2cz = D(9) * a.z;
let dd = AMOUNT / (D(6) * r2 - x2cx - y2cy - z2cz + 1.0);
v.x += dd * (a.x * D(10) + D(3) * (y2cy + z2cz - r2 - 1.0));
v.y += dd * (a.y * D(11) + D(4) * (x2cx + z2cz - r2 - 1.0));
v.z += dd * (a.z * D(12) + D(5) * (y2cy + x2cx - r2 - 1.0));",
    },
    VarDef {
        name: "pointgrid3d_wf",
        params: &[
            ParamDef::new("xmin", -3.0),
            ParamDef::new("xmax", 3.0),
            ParamDef::int("xcount", 10.0, 1.0, 100.0),
            ParamDef::new("ymin", -3.0),
            ParamDef::new("ymax", 3.0),
            ParamDef::int("ycount", 10.0, 1.0, 100.0),
            ParamDef::new("zmin", -1.0),
            ParamDef::new("zmax", 1.0),
            ParamDef::int("zcount", 10.0, 1.0, 100.0),
            ParamDef::new("distortion", 2.3),
            ParamDef::int("seed", 1234.0, 0.0, 10000.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [xmin, xmax, xcount, ymin, ymax, ycount, zmin, zmax, zcount, distortion, seed, dx, dy, dz]
            let xi = ctx.random_int(d[2] as usize) as i32;
            let yi = ctx.random_int(d[5] as usize) as i32;
            let zi = ctx.random_int(d[8] as usize) as i32;
            let mut x = d[0] + d[11] * xi as f64;
            let mut y = d[3] + d[12] * yi as f64;
            let mut z = d[6] + d[13] * zi as f64;
            if d[9] > 0.0 {
                let seed = d[10] as i32;
                x += (0.5 - marsaglia1(seed.wrapping_add(1563).wrapping_mul(xi).wrapping_add(zi))) * d[9];
                y += (0.5 - marsaglia1(seed.wrapping_add(6715).wrapping_mul(yi).wrapping_add(xi))) * d[9];
                z += (0.5 - marsaglia1(seed.wrapping_add(4761).wrapping_mul(zi).wrapping_add(yi))) * d[9];
            }
            v.x += x * amount;
            v.y += y * amount;
            v.z += z * amount;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for k in [2, 5, 8, 10] {
                d[k] = ftoi(p[k]) as f64;
            }
            d.extend([(p[1] - p[0]) / d[2], (p[4] - p[3]) / d[5], (p[7] - p[6]) / d[8]]);
            d
        }),
        wgsl: "fn pointgrid3d_wf_marsaglia(seed: i32) -> f32 {
  var u = seed << 16u;
  var v = (seed << 16u) >> 16u;
  v = 36969 * (v & 65535) + (v >> 16u);
  u = 18000 * (u & 65535) + (u >> 16u);
  return abs(f32((v << 16u) + u) / 2147483647.0);
}
@body
let xi = i32(rnd() * D(2));
let yi = i32(rnd() * D(5));
let zi = i32(rnd() * D(8));
var x = D(0) + D(11) * f32(xi);
var y = D(3) + D(12) * f32(yi);
var z = D(6) + D(13) * f32(zi);
if (D(9) > 0.0) {
  let seed = i32(D(10));
  x += (0.5 - pointgrid3d_wf_marsaglia((seed + 1563) * xi + zi)) * D(9);
  y += (0.5 - pointgrid3d_wf_marsaglia((seed + 6715) * yi + xi)) * D(9);
  z += (0.5 - pointgrid3d_wf_marsaglia((seed + 4761) * zi + yi)) * D(9);
}
v.x += x * AMOUNT;
v.y += y * AMOUNT;
v.z += z * AMOUNT;",
    },
    VarDef {
        name: "pointgrid_wf",
        params: &[
            ParamDef::new("xmin", -3.0),
            ParamDef::new("xmax", 3.0),
            ParamDef::int("xcount", 32.0, 1.0, 200.0),
            ParamDef::new("ymin", -3.0),
            ParamDef::new("ymax", 3.0),
            ParamDef::int("ycount", 32.0, 1.0, 200.0),
            ParamDef::new("distortion", 2.3),
            ParamDef::int("seed", 1234.0, 0.0, 10000.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [xmin, xmax, xcount, ymin, ymax, ycount, distortion, seed, dx, dy]
            let xi = ctx.random_int(d[2] as usize) as i32;
            let yi = ctx.random_int(d[5] as usize) as i32;
            let mut x = d[0] + d[8] * xi as f64;
            let mut y = d[3] + d[9] * yi as f64;
            if d[6] > 0.0 {
                let seed = d[7] as i32;
                x += (0.5 - marsaglia1(seed.wrapping_add(1563).wrapping_mul(xi).wrapping_add(yi))) * d[6];
                y += (0.5 - marsaglia1(seed.wrapping_add(6715).wrapping_mul(yi).wrapping_add(xi))) * d[6];
            }
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for k in [2, 5, 7] {
                d[k] = ftoi(p[k]) as f64;
            }
            d.extend([(p[1] - p[0]) / d[2], (p[4] - p[3]) / d[5]]);
            d
        }),
        wgsl: "fn pointgrid_wf_marsaglia(seed: i32) -> f32 {
  var u = seed << 16u;
  var v = (seed << 16u) >> 16u;
  v = 36969 * (v & 65535) + (v >> 16u);
  u = 18000 * (u & 65535) + (u >> 16u);
  return abs(f32((v << 16u) + u) / 2147483647.0);
}
@body
let xi = i32(rnd() * D(2));
let yi = i32(rnd() * D(5));
var x = D(0) + D(8) * f32(xi);
var y = D(3) + D(9) * f32(yi);
if (D(6) > 0.0) {
  let seed = i32(D(7));
  x += (0.5 - pointgrid_wf_marsaglia((seed + 1563) * xi + yi)) * D(6);
  y += (0.5 - pointgrid_wf_marsaglia((seed + 6715) * yi + xi)) * D(6);
}
v.x += x * AMOUNT;
v.y += y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "point_mirror_symmetry",
        params: &[
            ParamDef::int("symmetry", 3.0, 1.0, 99.0),
            ParamDef::int("side crop enable", 1.0, 0.0, 1.0),
            ParamDef::int("crop end enable", 1.0, 0.0, 1.0),
            ParamDef::new("crop end distance", 1.0),
            ParamDef::new("zx Multiply", 1.0),
            ParamDef::new("zy Multiply", 0.0),
            ParamDef::int("collidoscopic", 1.0, 0.0, 1.0),
            ParamDef::int("color_preserve", 0.0, 0.0, 1.0),
            ParamDef::int("buffer_size", 12.0, 1.0, 1000.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [symmetry, side_crop, crop_end, crop_end_dist, zx, zy, collidoscopic, color_preserve, buffer_size]
            // Java draws the symmetry index from an unseeded java.util.Random; here from ctx
            // (no draw for symmetry 1, where the index is always 0).
            let sym = d[0] as i32;
            let rand_sym = if sym > 1 { ctx.random_int(sym as usize) as i32 } else { 0 };
            let rot_sym = M_PI / sym as f64;
            let rot_rs = rot_sym * rand_sym as f64 * 2.0;
            let mut x = a.x * d[4] - a.y * d[5];
            let mut y = a.x * d[5] + a.y * d[4];
            let mut z = a.z;
            if d[1] as i32 == 1 {
                let r = (x * x + y * y).sqrt();
                let x_line = r * rot_sym.cos();
                if d[6] as i32 == 1 && y < 0.0 {
                    x = 0.0;
                    y = 0.0;
                }
                if x < x_line {
                    x = 0.0;
                    y = 0.0;
                }
                let crop_end = d[2] as i32 == 1;
                if crop_end && x * rot_sym.cos() + y * rot_sym.sin() > d[3] {
                    x = 0.0;
                    y = 0.0;
                }
                if crop_end && x * (-rot_sym).cos() + y * (-rot_sym).sin() > d[3] {
                    x = 0.0;
                    y = 0.0;
                }
                // The point buffer lives across calls (per render thread here).
                let size = d[8] as usize;
                PMS_BUFFERS.with(|m| {
                    let mut m = m.borrow_mut();
                    let b = m.entry(d.as_ptr() as usize).or_insert_with(|| PmsBuffer { pts: vec![[0.1, 0.1, 0.0, 0.5]; size], index: 0, valid: false });
                    if x == 0.0 && y == 0.0 {
                        if b.valid {
                            let p = b.pts[ctx.random_int(size)];
                            x = p[0];
                            y = p[1];
                            z = p[2];
                            if d[7] as i32 == 0 {
                                v.color = p[3];
                            }
                        }
                    } else {
                        b.pts[b.index] = [x, y, z, v.color];
                        b.valid = true;
                        b.index = (b.index + 1) % size;
                    }
                });
            }
            if d[6] as i32 == 1 && ctx.random() < 0.5 {
                y = -y;
            }
            if rand_sym % 2 == 0 {
                y = -y;
            }
            let (s, c) = rot_rs.sin_cos();
            v.x = amount * (x * c - y * s);
            v.y = amount * (y * c + x * s);
            v.z = amount * z;
            if ctx.preserve_z {
                v.z = amount * a.z;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for (k, lo, hi) in [(0, 1, 99), (1, 0, 1), (2, 0, 1), (6, 0, 1), (7, 0, 1), (8, 1, 1000)] {
                d[k] = limit_value(ftoi(p[k]), lo, hi) as f64;
            }
            d
        }),
        // GPU: no point buffer; cropped points stay at the origin (as in Java before the first
        // uncropped point).
        wgsl: "let sym = i32(D(0));
var rand_sym = 0;
if (sym > 1) { rand_sym = i32(rnd() * f32(sym)); }
let rot_sym = PI / f32(sym);
let rot_rs = rot_sym * f32(rand_sym) * 2.0;
var x = a.x * D(4) - a.y * D(5);
var y = a.x * D(5) + a.y * D(4);
if (i32(D(1)) == 1) {
  let x_line = sqrt(x * x + y * y) * cos(rot_sym);
  if (i32(D(6)) == 1 && y < 0.0) { x = 0.0; y = 0.0; }
  if (x < x_line) { x = 0.0; y = 0.0; }
  let crop_end = i32(D(2)) == 1;
  if (crop_end && x * cos(rot_sym) + y * sin(rot_sym) > D(3)) { x = 0.0; y = 0.0; }
  if (crop_end && x * cos(-rot_sym) + y * sin(-rot_sym) > D(3)) { x = 0.0; y = 0.0; }
}
if (i32(D(6)) == 1 && rnd() < 0.5) { y = -y; }
if (rand_sym % 2 == 0) { y = -y; }
v.x = AMOUNT * (x * cos(rot_rs) - y * sin(rot_rs));
v.y = AMOUNT * (y * cos(rot_rs) + x * sin(rot_rs));
v.z = AMOUNT * a.z;",
    },
    VarDef {
        name: "polylogarithm",
        params: &[ParamDef::int("n", 2.0, -10.0, 19.0), ParamDef::new("zpow", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [n, zpow, harmonic(n - 1), 1 / (n - 1)!, Z(n - i) / i! for i in 0..20]
            let n = d[0];
            let mut z = Complex::new(a.x, a.y);
            z.pow(d[1]);
            z.save();
            if z.mag2() > 250000.0 || n >= 20.0 {
                v.x += amount * z.re;
                v.y += amount * z.im;
            } else {
                let mut lin = Complex::new(0.0, 0.0);
                let mut zpl1 = Complex::copy_of(&z);
                if z.mag2() < 0.07 {
                    for i in 1..20 {
                        let mut t = Complex::new((i as f64).powf(n), 0.0);
                        t.div_r(&z);
                        lin.add(&t);
                        z.next_pow();
                    }
                } else {
                    z.log();
                    z.save();
                    z.one();
                    for i in 0..20 {
                        let c = d[4 + i];
                        if c != 0.0 {
                            let mut t = Complex::copy_of(&z);
                            t.scale(c);
                            lin.add(&t);
                        }
                        if i as f64 == n - 1.0 {
                            zpl1.copy(&z);
                        }
                        z.next_pow();
                    }
                    z.restore();
                    z.neg();
                    z.log();
                    z.neg();
                    z.re += d[2];
                    let t = Complex::copy_of(&z);
                    z.copy(&zpl1);
                    z.scale(d[3]);
                    z.mul(&t);
                    lin.add(&z);
                }
                v.x += amount * lin.re;
                v.y += amount * lin.im;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let n = ftoi(p[0]);
            let hs = (1..n).map(|i| 1.0 / i as f64).sum::<f64>();
            let mut d = vec![n as f64, p[1], hs, 1.0 / long_factorial(n - 1)];
            d.extend((0..20).map(|i| riemann_z(n - i) / long_factorial(i)));
            d
        }),
        wgsl: "let n = D(0);
var z = cx_pow(vec2<f32>(a.x, a.y), D(1));
let s = z;
if (cx_mag2(z) > 250000.0 || n >= 20.0) {
  v.x += AMOUNT * z.x;
  v.y += AMOUNT * z.y;
} else {
  var lin = vec2<f32>(0.0);
  var zpl1 = z;
  if (cx_mag2(z) < 0.07) {
    for (var i = 1; i < 20; i++) {
      lin += cx_div_r(vec2<f32>(pow(f32(i), n), 0.0), z);
      z = cx_mul(z, s);
    }
  } else {
    let lz = cx_log(z);
    z = vec2<f32>(1.0, 0.0);
    for (var i = 0; i < 20; i++) {
      let c = prm[base + 4u + u32(i)];
      if (c != 0.0) { lin += z * c; }
      if (f32(i) == n - 1.0) { zpl1 = z; }
      z = cx_mul(z, lz);
    }
    var w = -cx_log(-lz);
    w.x += D(2);
    lin += cx_mul(zpl1 * D(3), w);
  }
  v.x += AMOUNT * lin.x;
  v.y += AMOUNT * lin.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "polySurf",
        params: &[
            ParamDef::new("c", 0.3),
            ParamDef::int("mirrorX", 1.0, 0.0, 1.0),
            ParamDef::int("mirrorY", 1.0, 0.0, 1.0),
            ParamDef::int("mirrorZ", 0.0, 0.0, 1.0),
            ParamDef::int("julia", 0.0, 0.0, 1.0),
            ParamDef::new("juliaX", 0.0),
            ParamDef::new("juliaY", 0.0),
            ParamDef::new("juliaZ", 0.0),
            ParamDef::int("enableFold", 0.0, 0.0, 1.0),
            ParamDef::new("foldX", 1.0),
            ParamDef::new("foldY", 1.0),
            ParamDef::range("rotX", 0.0, -180.0, 180.0),
            ParamDef::range("rotY", 0.0, -180.0, 180.0),
            ParamDef::range("rotZ", 0.0, -180.0, 180.0),
            ParamDef::int("enableOffset", 1.0, 0.0, 1.0),
            ParamDef::new("offsetX", 1.0),
            ParamDef::new("offsetY", 1.0),
            ParamDef::new("offsetZ", 0.0),
            ParamDef::int("enableInvert", 0.0, 0.0, 1.0),
            ParamDef::new("invertRadius", 1.0),
            ParamDef::int("colorMode", 0.0, 0.0, 2.0),
            ParamDef::new("colorSpeed", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [c, mirrorX, mirrorY, mirrorZ, julia, juliaX, juliaY, juliaZ, enableFold, foldX, foldY,
            //      rotX, rotY, rotZ, enableOffset, offsetX, offsetY, offsetZ, enableInvert, invertRadius,
            //      colorMode, colorSpeed]
            let sign_x = if a.x >= 0.0 { 1.0 } else { -1.0 };
            let sign_y = if a.y >= 0.0 { 1.0 } else { -1.0 };
            let (mut x, mut y, mut z) = (a.x, a.y, a.z);
            if d[4] > 0.0 {
                x += d[5];
                y += d[6];
                z += d[7];
            }
            if d[8] > 0.0 {
                x = (x + d[9]).abs() - (x - d[9]).abs() - x;
                y = (y + d[10]).abs() - (y - d[10]).abs() - y;
            }
            if d[14] > 0.0 {
                x += d[15];
                y += d[16];
                z += d[17];
            }
            let (xa, ya) = (x.abs(), y.abs());
            let c = d[0];
            let r2 = sqr(xa) + sqr(ya) + sqr(z);
            let r = amount / (r2 * sqr(c) + 2.0 * c * z + 1.0);
            let (mut dx, mut dy, mut dz) = (r * xa, r * ya, r * (z + c * r2));
            if d[11] != 0.0 || d[12] != 0.0 || d[13] != 0.0 {
                let (sx, cx) = d[11].to_radians().sin_cos();
                let (sy, cy) = d[12].to_radians().sin_cos();
                let (sz, cz) = d[13].to_radians().sin_cos();
                (dy, dz) = (dy * cx - dz * sx, dy * sx + dz * cx);
                (dx, dz) = (dx * cy + dz * sy, -dx * sy + dz * cy);
                (dx, dy) = (dx * cz - dy * sz, dx * sz + dy * cz);
            }
            if d[18] > 0.0 {
                let r2i = sqr(dx) + sqr(dy) + sqr(dz);
                if r2i > 1.0e-9 {
                    let f = sqr(d[19]) / r2i;
                    dx *= f;
                    dy *= f;
                    dz *= f;
                }
            }
            v.x += (if d[1] > 0.0 && ctx.random() < 0.5 { -1.0 } else { 1.0 }) * sign_x * dx;
            v.y += (if d[2] > 0.0 && ctx.random() < 0.5 { -1.0 } else { 1.0 }) * sign_y * dy;
            v.z += (if d[3] > 0.0 && ctx.random() < 0.5 { -1.0 } else { 1.0 }) * dz;
            let mode = d[20] as i32;
            if mode > 0 {
                let col = match mode {
                    1 => (sqr(dx) + sqr(dy) + sqr(dz)).sqrt(),
                    2 => dy.atan2(dx) / (2.0 * M_PI) + 0.5,
                    _ => dz,
                } * d[21];
                let col = fmod(fmod(col, 1.0) + 1.0, 1.0);
                v.color += (col - v.color) * amount;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for (k, hi) in [(1, 1), (2, 1), (3, 1), (4, 1), (8, 1), (14, 1), (18, 1), (20, 2)] {
                d[k] = limit_value(ftoi(p[k]), 0, hi) as f64;
            }
            d
        }),
        wgsl: "let sign_x = select(-1.0, 1.0, a.x >= 0.0);
let sign_y = select(-1.0, 1.0, a.y >= 0.0);
var x = a.x;
var y = a.y;
var z = a.z;
if (D(4) > 0.0) { x += D(5); y += D(6); z += D(7); }
if (D(8) > 0.0) {
  x = abs(x + D(9)) - abs(x - D(9)) - x;
  y = abs(y + D(10)) - abs(y - D(10)) - y;
}
if (D(14) > 0.0) { x += D(15); y += D(16); z += D(17); }
let xa = abs(x);
let ya = abs(y);
let c = D(0);
let r2 = xa * xa + ya * ya + z * z;
let r = AMOUNT / (r2 * c * c + 2.0 * c * z + 1.0);
var dx = r * xa;
var dy = r * ya;
var dz = r * (z + c * r2);
if (D(11) != 0.0 || D(12) != 0.0 || D(13) != 0.0) {
  let rx = radians(D(11));
  let ry = radians(D(12));
  let rz = radians(D(13));
  let ty = dy * cos(rx) - dz * sin(rx);
  dz = dy * sin(rx) + dz * cos(rx);
  dy = ty;
  let tx = dx * cos(ry) + dz * sin(ry);
  dz = -dx * sin(ry) + dz * cos(ry);
  dx = tx;
  let tx2 = dx * cos(rz) - dy * sin(rz);
  dy = dx * sin(rz) + dy * cos(rz);
  dx = tx2;
}
if (D(18) > 0.0) {
  let r2i = dx * dx + dy * dy + dz * dz;
  if (r2i > 1.0e-9) {
    let f = D(19) * D(19) / r2i;
    dx *= f;
    dy *= f;
    dz *= f;
  }
}
var mx = 1.0;
if (D(1) > 0.0 && rnd() < 0.5) { mx = -1.0; }
v.x += mx * sign_x * dx;
var my = 1.0;
if (D(2) > 0.0 && rnd() < 0.5) { my = -1.0; }
v.y += my * sign_y * dy;
var mz = 1.0;
if (D(3) > 0.0 && rnd() < 0.5) { mz = -1.0; }
v.z += mz * dz;
let mode = i32(D(20));
if (mode > 0) {
  var col = dz;
  if (mode == 1) { col = sqrt(dx * dx + dy * dy + dz * dz); }
  if (mode == 2) { col = atan2(dy, dx) / (2.0 * PI) + 0.5; }
  col *= D(21);
  col = fmod(fmod(col, 1.0) + 1.0, 1.0);
  v.color += (col - v.color) * AMOUNT;
}",
    },
    VarDef {
        name: "popcorn2_3D",
        params: &[ParamDef::new("x", 0.1), ParamDef::new("y", 0.1), ParamDef::new("z", 0.1), ParamDef::new("c", 3.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let tmp_vv = if amount.abs() <= 1.0 { amount.abs() * amount } else { amount };
            let stc = d[3].tan().sin();
            let tpz = if v.z == 0.0 { tmp_vv * stc * a.y.atan2(a.x) } else { v.z };
            let ttz = if a.z == 0.0 { tmp_vv * stc * a.y.atan2(a.x) } else { a.z };
            v.x += amount * 0.5 * (a.x + d[0] * (d[3] * a.y).tan().sin());
            v.y += amount * 0.5 * (a.y + d[1] * (d[3] * a.x).tan().sin());
            v.z = tpz + tmp_vv * (d[2] * stc * ttz);
        },
        prepare: None,
        wgsl: "var tmp_vv = AMOUNT;
if (abs(AMOUNT) <= 1.0) { tmp_vv = abs(AMOUNT) * AMOUNT; }
let stc = sin(tan(D(3)));
var tpz = v.z;
if (v.z == 0.0) { tpz = tmp_vv * stc * atan2(a.y, a.x); }
var ttz = a.z;
if (a.z == 0.0) { ttz = tmp_vv * stc * atan2(a.y, a.x); }
v.x += AMOUNT * 0.5 * (a.x + D(0) * sin(tan(D(3) * a.y)));
v.y += AMOUNT * 0.5 * (a.y + D(1) * sin(tan(D(3) * a.x)));
v.z = tpz + tmp_vv * (D(2) * stc * ttz);",
    },
    VarDef {
        name: "post_aexion_crop",
        params: &[
            ParamDef::int("Iters", 100.0, 1.0, 250.0),
            ParamDef::new("cadd", -0.8),
            ParamDef::new("bailout", 10.0),
            ParamDef::new("scaleX", 1.0),
            ParamDef::new("scaleY", 1.0),
            ParamDef::new("ScaleZ", 1.0),
            ParamDef::range("color", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            // d = [iters, cadd, bailout, scaleX, scaleY, scaleZ, color]
            let cadd = d[1];
            let (x, y, z) = (v.x, v.y, v.z);
            let p0 = [(x + y + z).abs() + cadd, (-x - y + z).abs() + cadd, (-x + y - z).abs() + cadd, (x - y - z).abs() + cadd];
            let auxc = [p0[0] * d[3], p0[1] * d[4], p0[2] * d[5]];
            let tw = (x - y - z).abs() + cadd;
            let mut q = [((x + y + z).abs() + cadd) * p0[0], ((-x - y + z).abs() + cadd) * p0[1], ((-x + y - z).abs() + cadd) * p0[2], tw * p0[3]];
            for _ in 0..d[0] as i32 {
                let tx = q[0] * q[0] - q[1] * q[1] + 2.0 * q[3] * q[2] + auxc[0];
                let ty = q[1] * q[1] - q[0] * q[0] + 2.0 * q[3] * q[2] + auxc[1];
                let tz = q[2] * q[2] - q[3] * q[3] + 2.0 * q[0] * q[1] + auxc[2];
                q = [tx + p0[0], ty + p0[1], tz + p0[2], tw + p0[3]];
                let r = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
                // Java scales the point again in every iteration below the bailout.
                if r < d[2] {
                    v.x *= amount;
                    v.y *= amount;
                    v.z *= amount;
                    v.color = r.sin() * d[6];
                } else {
                    v.x = 0.0;
                    v.y = 0.0;
                    v.z = 0.0;
                }
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(ftoi(p[0]), 1, 250) as f64;
            d[6] = limit_value(p[6], 0.0, 1.0);
            d
        }),
        wgsl: "let cadd = D(1);
let x = v.x;
let y = v.y;
let z = v.z;
let p0 = vec4<f32>(abs(x + y + z) + cadd, abs(-x - y + z) + cadd, abs(-x + y - z) + cadd, abs(x - y - z) + cadd);
let auxc = vec3<f32>(p0.x * D(3), p0.y * D(4), p0.z * D(5));
let tw = abs(x - y - z) + cadd;
var q = p0 * p0;
let n = i32(D(0));
for (var i = 0; i < n; i++) {
  let tx = q.x * q.x - q.y * q.y + 2.0 * q.w * q.z + auxc.x;
  let ty = q.y * q.y - q.x * q.x + 2.0 * q.w * q.z + auxc.y;
  let tz = q.z * q.z - q.w * q.w + 2.0 * q.x * q.y + auxc.z;
  q = vec4<f32>(tx, ty, tz, tw) + p0;
  let r = length(q);
  if (r < D(2)) {
    v.x *= AMOUNT;
    v.y *= AMOUNT;
    v.z *= AMOUNT;
    v.color = sin(r) * D(6);
  } else {
    v.x = 0.0;
    v.y = 0.0;
    v.z = 0.0;
  }
}",
    },
];
