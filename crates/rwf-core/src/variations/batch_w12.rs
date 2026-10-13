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

// Some shaders use JWildfire's truncated pi literals.
#![allow(clippy::approx_constant)]

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;
use super::complex::Complex;
use super::glsl::{Vec2, abs, clamp, dot, glsl_atan2, glsl_mod, length, max, min, sign, vec2};
use crate::brush::Marsaglia;

/// `MarsagliaRandomGenerator.randomize(seed)` followed by one `random()`.
fn marsaglia1(seed: i32) -> f64 {
    Marsaglia::new(seed as i64).random()
}

/// Per-thread state slot `k` (two values) of the variation instance with prepared data `d`
/// (CPU only, for JWildfire variations that keep values across calls). The key is the address
/// of `d[k]`, so `k < d.len()` keeps the slots of different instances apart.
fn state_slot<'a>(ctx: &'a mut Ctx, d: &[f64], k: usize, init: [f64; 2]) -> &'a mut [f64; 2] {
    debug_assert!(k < d.len());
    ctx.orbits.entry(d.as_ptr().wrapping_add(k) as usize).or_insert(init)
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

const AFFINE3D_PARAMS: &[ParamDef] = &[
    ParamDef::new("translateX", 0.0),
    ParamDef::new("translateY", 0.0),
    ParamDef::new("translateZ", 0.0),
    ParamDef::new("scaleX", 1.0),
    ParamDef::new("scaleY", 1.0),
    ParamDef::new("scaleZ", 1.0),
    ParamDef::range("rotateX", 0.0, -180.0, 180.0),
    ParamDef::range("rotateY", 0.0, -180.0, 180.0),
    ParamDef::range("rotateZ", 0.0, -180.0, 180.0),
    ParamDef::new("shearXY", 0.0),
    ParamDef::new("shearXZ", 0.0),
    ParamDef::new("shearYX", 0.0),
    ParamDef::new("shearYZ", 0.0),
    ParamDef::new("shearZX", 0.0),
    ParamDef::new("shearZY", 0.0),
];

// d = [translateX, translateY, translateZ, scaleX, scaleY, scaleZ, rotateX, rotateY, rotateZ,
//      shearXY, shearXZ, shearYX, shearYZ, shearZX, shearZY, sinX, cosX, sinY, cosY, sinZ, cosZ, has_shear]
fn affine3d_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    for &rot in &p[6..9] {
        let ang = rot * M_PI / 180.0;
        d.extend([ang.sin(), ang.cos()]);
    }
    d.push(if p[9..15].iter().any(|s| s.abs() > EPSILON) { 1.0 } else { 0.0 });
    d
}

/// `AbstractAffine3DFunc.affineTransform`
fn affine3d_xform(d: &[f64], x: f64, y: f64, z: f64, amount: f64) -> (f64, f64, f64) {
    let (sx, sy, sz) = (d[3], d[4], d[5]);
    let (sin_x, cos_x, sin_y, cos_y, sin_z, cos_z) = (d[15], d[16], d[17], d[18], d[19], d[20]);
    if d[21] != 0.0 {
        let u = d[9] * sy * y + d[10] * sz * z + sx * x;
        let w = d[11] * sx * x + d[12] * sz * z + sy * y;
        let t = d[13] * sx * x + d[14] * sy * y + sz * z;
        (
            amount * (cos_z * (cos_y * u + sin_y * (sin_x * w + cos_x * t)) - sin_z * (cos_x * w - sin_x * t) + d[0]),
            amount * (sin_z * (cos_y * u + sin_y * (sin_x * w + cos_x * t)) + cos_z * (cos_x * w - sin_x * t) + d[1]),
            amount * (-sin_y * u + cos_y * (sin_x * w + cos_x * t) + d[2]),
        )
    } else {
        (
            amount * (cos_z * (cos_y * sx * x + sin_y * (cos_x * sz * z + sin_x * sy * y)) - sin_z * (cos_x * sy * y - sin_x * sz * z) + d[0]),
            amount * (sin_z * (cos_y * sx * x + sin_y * (cos_x * sz * z + sin_x * sy * y)) + cos_z * (cos_x * sy * y - sin_x * sz * z) + d[1]),
            amount * (-sin_y * sx * x + cos_y * (cos_x * sz * z + sin_x * sy * y) + d[2]),
        )
    }
}

/// WGSL body of [`affine3d_xform`] (input `ax, ay, az`, output `r`).
macro_rules! affine3d_wgsl {
    () => {
        "let sx = D(3);
let sy = D(4);
let sz = D(5);
let sin_x = D(15);
let cos_x = D(16);
let sin_y = D(17);
let cos_y = D(18);
let sin_z = D(19);
let cos_z = D(20);
var u = sx * ax;
var w = sy * ay;
var t = sz * az;
if (D(21) != 0.0) {
  u = D(9) * sy * ay + D(10) * sz * az + sx * ax;
  w = D(11) * sx * ax + D(12) * sz * az + sy * ay;
  t = D(13) * sx * ax + D(14) * sy * ay + sz * az;
}
let m = cos_y * u + sin_y * (sin_x * w + cos_x * t);
let n = cos_x * w - sin_x * t;
let r = AMOUNT * vec3<f32>(cos_z * m - sin_z * n + D(0), sin_z * m + cos_z * n + D(1), -sin_y * u + cos_y * (sin_x * w + cos_x * t) + D(2));
"
    };
}

/// Java `js.glsl.mat3` in constructor order `[a00, a10, a20, a01, a11, a21, a02, a12, a22]`.
type JMat3 = [f64; 9];

/// `multiply(A, B)` of the `*_crop` variations: passes the row-major product to the
/// column-major `mat3` constructor (i.e. returns the transposed product).
fn jmat3_multiply(a: &JMat3, b: &JMat3) -> JMat3 {
    let g = |m: &JMat3, r: usize, c: usize| m[c * 3 + r];
    let mut res = [0.0; 9];
    for (i, e) in res.iter_mut().enumerate() {
        let (r, c) = (i / 3, i % 3);
        *e = g(a, r, 0) * g(b, 0, c) + g(a, r, 1) * g(b, 1, c) + g(a, r, 2) * g(b, 2, c);
    }
    res
}

/// `euler_rotation(rot)` of the `*_crop` variations (angles in degrees).
fn jmat3_euler(rx: f64, ry: f64, rz: f64) -> JMat3 {
    let (sx, cx) = (-rx * M_PI / 180.0).sin_cos();
    let (sy, cy) = (-ry * M_PI / 180.0).sin_cos();
    let (sz, cz) = (-rz * M_PI / 180.0).sin_cos();
    let mx = [1.0, 0.0, 0.0, 0.0, cx, -sx, 0.0, sx, cx];
    let my = [cy, 0.0, sy, 0.0, 1.0, 0.0, -sy, 0.0, cy];
    let mz = [cz, -sz, 0.0, sz, cz, 0.0, 0.0, 0.0, 1.0];
    let id = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    jmat3_multiply(&jmat3_multiply(&jmat3_multiply(&id, &mx), &my), &mz)
}

/// `vec3.times(mat3)`
fn jmat3_vtimes(m: &[f64], x: f64, y: f64, z: f64) -> (f64, f64, f64) {
    (x * m[0] + y * m[1] + z * m[2], x * m[3] + y * m[4] + z * m[5], x * m[6] + y * m[7] + z * m[8])
}

/// Bailout test at the end of every iteration of the `*_crop` variations: scales the point
/// again while inside, zeroes it otherwise.
fn crop_step(v: &mut Pt, amount: f64, r: f64, bailout: f64, pcolor: f64) {
    if r < bailout {
        v.x *= amount;
        v.y *= amount;
        v.z *= amount;
        v.color = r.sin() * pcolor;
    } else {
        v.x = 0.0;
        v.y = 0.0;
        v.z = 0.0;
    }
}

/// WGSL of [`crop_step`] (uses `r`, bailout `$b`, color `$c`).
macro_rules! crop_step_wgsl {
    ($b:literal, $c:literal) => {
        concat!(
            "  if (r < ",
            $b,
            ") {
    v.x *= AMOUNT;
    v.y *= AMOUNT;
    v.z *= AMOUNT;
    v.color = sin(r) * ",
            $c,
            ";
  } else {
    v.x = 0.0;
    v.y = 0.0;
    v.z = 0.0;
  }
"
        )
    };
}

/// Prepared data of the `*_crop` variations: iterations clamped to 0..250 and, if `color` is
/// given, the color factor to 0..1.
fn crop_prepare(p: &[f64], iters: usize, color: Option<usize>) -> Vec<f64> {
    let mut d = p.to_vec();
    d[iters] = limit_value(ftoi(p[iters]), 0, 250) as f64;
    if let Some(c) = color {
        d[c] = limit_value(p[c], 0.0, 1.0);
    }
    d
}

/// `(int) Tools.limitValue(v, lo, hi)` as used by the `post_crop_*` parameter setters.
fn crop_int_param(v: f64, lo: f64, hi: f64) -> f64 {
    limit_value(v, lo, hi) as i32 as f64
}

/// Shared tail of the SDF based `post_crop_*` variations: points outside the shape (inside when
/// `invert`) are hidden and moved to the origin.
fn post_crop_output(ctx: &Ctx, a: &Pt, v: &mut Pt, amount: f64, d: f64, invert: f64) {
    let hide = if invert == 0.0 { d > 0.0 } else { d <= 0.0 };
    v.do_hide = hide;
    let (x, y) = if hide { (0.0, 0.0) } else { (v.x, v.y) };
    v.x = amount * x;
    v.y = amount * y;
    ctx.pass_z(a, v, amount);
}

/// WGSL of [`post_crop_output`] (`sd` = signed distance, `D(inv)` = invert flag).
macro_rules! post_crop_sdf_tail {
    ($inv:literal) => {
        concat!(
            "var hide = sd > 0.0;
if (D(",
            $inv,
            ") != 0.0) { hide = sd <= 0.0; }
if (hide) {
  v.hide = 1.0;
  v.x = 0.0;
  v.y = 0.0;
} else {
  v.hide = 0.0;
  v.x = AMOUNT * v.x;
  v.y = AMOUNT * v.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        )
    };
}

fn post_crop_box_sd(p: Vec2, b: Vec2) -> f64 {
    let d = abs(p) - b;
    length(max(d, Vec2::splat(0.0))) + min(max(d.x, d.y), 0.0)
}

fn post_crop_cross_sd(p: Vec2, b: Vec2, r: f64) -> f64 {
    let mut p = abs(p);
    p = if p.y > p.x { vec2(p.y, p.x) } else { vec2(p.x, p.y) };
    let q = p - b;
    let k = max(q.y, q.x);
    let w = if k > 0.0 { q } else { vec2(b.y - p.x, -k) };
    sign(k) * length(max(w, 0.0)) + r
}

fn post_crop_polygon_hexagram(p: Vec2, r: f64) -> f64 {
    let k = [-0.5, 0.86602540378, 0.57735026919, 1.73205080757];
    let mut p = abs(p);
    let kxy = vec2(k[0], k[1]);
    let t0 = dot(kxy, p);
    p -= kxy * Vec2::splat(2.0) * min(t0, 0.0);
    let kyx = vec2(k[1], k[0]);
    let t0 = dot(kyx, p);
    p -= kyx * 2.0 * min(t0, 0.0);
    p = p - vec2(clamp(p.x, r * k[2], r * k[3]), r);
    length(p) * sign(p.y)
}

fn post_crop_polygon_octogon(p: Vec2, r: f64) -> f64 {
    let k = [-0.9238795325, 0.3826834323, 0.4142135623];
    let mut p = abs(p);
    let k1 = vec2(k[0], k[1]);
    p = p - k1 * (2.0 * min(dot(k1, p), 0.0));
    let k2 = vec2(-k[0], k[1]);
    p = p - k2 * (2.0 * min(dot(k2, p), 0.0));
    p = p - vec2(clamp(p.x, -k[2] * r, k[2] * r), r);
    length(p) * sign(p.y)
}

fn post_crop_polygon_hexagon(p: Vec2, r: f64) -> f64 {
    let k = [-0.866025404, 0.5, 0.577350269];
    let mut p = abs(p);
    let k1 = vec2(k[0], k[1]);
    p = p - k1 * (2.0 * min(dot(k1, p), 0.0));
    p = p - vec2(clamp(p.x, -k[2] * r, k[2] * r), r);
    length(p) * sign(p.y)
}

fn post_crop_polygon_pentagon(p: Vec2, r: f64) -> f64 {
    let k = [0.809016994, 0.587785252, 0.726542528];
    let mut p = vec2(p.x.abs(), -p.y);
    let k1 = vec2(-k[0], k[1]);
    p = p - k1 * (2.0 * min(dot(k1, p), 0.0));
    let k2 = vec2(k[0], k[1]);
    p = p - k2 * (2.0 * min(dot(k2, p), 0.0));
    p = p - vec2(clamp(p.x, -r * k[2], r * k[2]), r);
    length(p) * sign(p.y)
}

fn post_crop_rhombus_ndot(a: Vec2, b: Vec2) -> f64 {
    a.x * b.x - a.y * b.y
}

fn post_crop_rhombus_sd(p: Vec2, b: Vec2) -> f64 {
    let q = abs(p);
    let h = clamp((-2.0 * post_crop_rhombus_ndot(q, b) + post_crop_rhombus_ndot(b, b)) / dot(b, b), -1.0, 1.0);
    let d = length(q - b * vec2(1.0 - h, 1.0 + h) * 0.5);
    d * sign(q.x * b.y + q.y * b.x - b.x * b.y)
}

fn post_crop_stars_sd(p: Vec2, r: f64, n: i32, m: f64) -> f64 {
    let an = 3.141593 / n as f64;
    let en = 6.283185 / m;
    let acs = vec2(an.cos(), an.sin());
    let ecs = vec2(en.cos(), en.sin());
    let bn = glsl_mod(glsl_atan2(p.x, p.y), 2.0 * an) - an;
    let mut p = vec2(bn.cos(), bn.sin().abs()) * length(p);
    p -= acs * r;
    p = p + ecs * clamp(-dot(p, ecs), 0.0, r * acs.y / ecs.y);
    length(p) * sign(p.x)
}

fn post_crop_trapezoid_sd(p: Vec2, r1: f64, r2: f64, he: f64) -> f64 {
    let k1 = vec2(r2, he);
    let k2 = vec2(r2 - r1, 2.0 * he);
    let p = vec2(p.x.abs(), p.y);
    let ca = vec2(max(0.0, p.x - if p.y < 0.0 { r1 } else { r2 }), p.y.abs() - he);
    let t0 = dot(k1 - p, k2) / dot(k2, k2);
    let cb = p - k1 + k2 * clamp(t0, 0.0, 1.0);
    let s = if cb.x < 0.0 && ca.y < 0.0 { -1.0 } else { 1.0 };
    s * java_min(dot(ca, ca), dot(cb, cb)).sqrt()
}

fn post_crop_triangle_area(x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) -> f64 {
    ((x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2)) / 2.0).abs()
}

fn post_crop_vesica_sd(p: Vec2, r: f64, d: f64) -> f64 {
    let p = abs(p);
    let b = (r * r - d * d).sqrt();
    if (p.y - b) * d > p.x * b { length(p - vec2(0.0, b)) } else { length(p - vec2(-d, 0.0)) - r }
}

fn falloff3_sgnd(x: f64) -> f64 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

fn falloff3_log_scale(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { ((x.abs() + 1.0) * M_E).ln() * falloff3_sgnd(x) / M_E }
}

fn falloff3_log_map(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { (M_E + (x * M_E).ln()) / 4.0 * falloff3_sgnd(x) }
}

/// `AbstractFalloff3Func.transform` for the input `vin = [x, y, z, color]`: the output vector.
// d = [blur_type, blur_shape, blur_strength, min_distance, invert_distance, mul_x, mul_y, mul_z,
//      mul_c, center_x, center_y, center_z, alpha, r_max]
fn falloff3_vout(d: &[f64], ctx: &mut Ctx, vin: [f64; 4]) -> [f64; 4] {
    let rnd = [ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5];
    let (dx, dy, dz) = (vin[0] - d[9], vin[1] - d[10], vin[2] - d[11]);
    let radius = if d[1] as i32 == 0 { (dx * dx + dy * dy + dz * dz).sqrt() } else { java_max(dx.abs(), java_max(dy.abs(), dz.abs())) };
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
            [vin[0] + mul[0] * rad * sc * pc, vin[1] + mul[1] * rad * sc * ps, vin[2] + mul[2] * rad * ss, vin[3] + mul[3] * dist * rnd[3]]
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
            let coeff = if r_max <= EPSILON { dist } else { dist + alpha * (falloff3_log_map(dist) - dist) };
            [
                vin[0] + falloff3_log_map(mul[0]) * falloff3_log_scale(rnd[0]) * coeff,
                vin[1] + falloff3_log_map(mul[1]) * falloff3_log_scale(rnd[1]) * coeff,
                vin[2] + falloff3_log_map(mul[2]) * falloff3_log_scale(rnd[2]) * coeff,
                vin[3] + falloff3_log_map(mul[3]) * falloff3_log_scale(rnd[3]) * coeff,
            ]
        }
    }
}

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

// d = [scale_x, scale_y, scale_z, yaw, pitch, roll, move_x, move_y, move_z,
//      coefxx, coefxy, coefxz, coefyx, coefyy, coefyz, coefzx, coefzy, coefzz,
//      icoefxx, icoefxy, icoefxz, icoefyx, icoefyy, icoefyz, icoefzx, icoefzy, icoefzz]
fn prepost_affine_prepare(p: &[f64]) -> Vec<f64> {
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
    let m = [
        cosyaw * cosroll - sinyaw * sinpitch * sinroll,
        -sinyaw * cosroll - cosyaw * sinpitch * sinroll,
        -cospitch * sinroll,
        sinyaw * cospitch,
        cosyaw * cospitch,
        -sinpitch,
        cosyaw * sinroll + sinyaw * sinpitch * cosroll,
        cosyaw * sinpitch * cosroll - sinyaw * sinroll,
        cospitch * cosroll,
    ];
    d.extend_from_slice(&m);
    d.extend_from_slice(&[m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]]);
    d
}

/// In-place `Complex` operation.
type ComplexOp = fn(&mut Complex);

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
                // The point buffer lives across calls (per render thread here): state slot 0 =
                // [index, valid], slots 1 + 2i / 2 + 2i = [x, y] / [z, color] of point i.
                let size = d[8] as usize;
                let [index, valid] = *state_slot(ctx, d, 0, [0.0, 0.0]);
                if x == 0.0 && y == 0.0 {
                    if valid != 0.0 {
                        let i = ctx.random_int(size);
                        [x, y] = *state_slot(ctx, d, 1 + 2 * i, [0.1, 0.1]);
                        let [pz, pc] = *state_slot(ctx, d, 2 + 2 * i, [0.0, 0.5]);
                        z = pz;
                        if d[7] as i32 == 0 {
                            v.color = pc;
                        }
                    }
                } else {
                    let i = index as usize;
                    *state_slot(ctx, d, 1 + 2 * i, [0.0; 2]) = [x, y];
                    *state_slot(ctx, d, 2 + 2 * i, [0.0; 2]) = [z, v.color];
                    *state_slot(ctx, d, 0, [0.0; 2]) = [((i + 1) % size) as f64, 1.0];
                }
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
            // Room for the state slots of the point buffer.
            d.resize(d.len().max(2 * d[8] as usize + 1), 0.0);
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
    VarDef {
        name: "post_affine3D",
        params: AFFINE3D_PARAMS,
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            (v.x, v.y, v.z) = affine3d_xform(d, v.x, v.y, v.z, amount);
        },
        prepare: Some(|p, _| affine3d_prepare(p)),
        wgsl: concat!("let ax = v.x;\nlet ay = v.y;\nlet az = v.z;\n", affine3d_wgsl!(), "v.x = r.x;\nv.y = r.y;\nv.z = r.z;"),
    },
    VarDef {
        name: "post_asurf_crop",
        params: &[
            ParamDef::int("iterations", 12.0, 0.0, 250.0),
            ParamDef::new("bailout", 16.0),
            ParamDef::new("scaleX", 1.0),
            ParamDef::new("scaleY", 1.0),
            ParamDef::new("scaleZ", 1.0),
            ParamDef::range("color", 1.0, 0.0, 1.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("angle", 0.0, -180.0, 180.0),
            ParamDef::new("rot_x", 0.0),
            ParamDef::new("rot_y", 0.0),
            ParamDef::new("rot_z", 1.0),
            ParamDef::new("minR", 0.25),
            ParamDef::new("maxR", 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            // d = [iterations, bailout, scaleX, scaleY, scaleZ, color, mode, angle, rot_x, rot_y, rot_z,
            //      minR, maxR, rot (mat3, 9)]
            let (x1, y1, z1) = (v.x * d[2], v.y * d[3], v.z * d[4]);
            let (mut px, mut py, mut pz) = (x1, y1, z1);
            let (min_r, max_r) = (d[11], d[12]);
            for _ in 0..d[0] as i32 {
                px = (px + 1.0).abs() - (px - 1.0).abs() - px;
                py = (py + 1.0).abs() - (py - 1.0).abs() - py;
                if d[6] as i32 == 0 {
                    pz = (pz + 1.0).abs() - (pz - 1.0).abs() - pz;
                }
                let rr = (px * px + py * py + pz * pz).sqrt();
                if rr < min_r {
                    let f = max_r / min_r;
                    (px, py, pz) = (px * f, py * f, pz * f);
                } else if rr < max_r {
                    let f = max_r / rr;
                    (px, py, pz) = (px * f, py * f, pz * f);
                }
                (px, py, pz) = jmat3_vtimes(&d[13..22], px, py, pz);
                (px, py, pz) = (px + x1, py + y1, pz + z1);
                crop_step(v, amount, px * px + py * py + pz * pz, d[1], d[5]);
            }
        },
        prepare: Some(|p, _| {
            let mut d = crop_prepare(p, 0, Some(5));
            d[6] = limit_value(ftoi(p[6]), 0, 1) as f64;
            let len = (p[8] * p[8] + p[9] * p[9] + p[10] * p[10]).sqrt();
            let (x, y, z) = (p[8] / len, p[9] / len, p[10] / len);
            let (s, c) = (p[7] * M_PI / 180.0).sin_cos();
            d.extend([
                c + (1.0 - c) * x * x,
                (1.0 - c) * x * y - s * z,
                (1.0 - c) * x * z + s * y,
                (1.0 - c) * x * y + s * z,
                c + (1.0 - c) * y * y,
                (1.0 - c) * y * z - s * x,
                (1.0 - c) * x * z - s * y,
                (1.0 - c) * y * z + s * x,
                c + (1.0 - c) * z * z,
            ]);
            d
        }),
        wgsl: concat!(
            "let p1 = vec3<f32>(v.x * D(2), v.y * D(3), v.z * D(4));
var p = p1;
let min_r = D(11);
let max_r = D(12);
let n = i32(D(0));
for (var i = 0; i < n; i++) {
  p.x = abs(p.x + 1.0) - abs(p.x - 1.0) - p.x;
  p.y = abs(p.y + 1.0) - abs(p.y - 1.0) - p.y;
  if (i32(D(6)) == 0) { p.z = abs(p.z + 1.0) - abs(p.z - 1.0) - p.z; }
  let rr = length(p);
  if (rr < min_r) { p = p * (max_r / min_r); } else if (rr < max_r) { p = p * (max_r / rr); }
  p = vec3<f32>(p.x * D(13) + p.y * D(14) + p.z * D(15), p.x * D(16) + p.y * D(17) + p.z * D(18), p.x * D(19) + p.y * D(20) + p.z * D(21));
  p = p + p1;
  let r = dot(p, p);
",
            crop_step_wgsl!("D(1)", "D(5)"),
            "}"
        ),
    },
    VarDef {
        name: "post_axis_symmetry_wf",
        params: &[
            ParamDef::int("axis", 0.0, 0.0, 2.0),
            ParamDef::new("centre_x", 0.25),
            ParamDef::new("centre_y", 0.5),
            ParamDef::new("centre_z", 0.5),
            ParamDef::range("rotation", 30.0, -360.0, 360.0),
            ParamDef::new("x1colorshift", 0.0),
            ParamDef::new("y1colorshift", 0.0),
            ParamDef::new("z1colorshift", 0.0),
            ParamDef::new("x2colorshift", 0.0),
            ParamDef::new("y2colorshift", 0.0),
            ParamDef::new("z2colorshift", 0.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [axis, centre_x, centre_y, centre_z, rotation, x1, y1, z1, x2, y2, z2 colorshift,
            //      sin, cos, do_rotate]
            if amount.abs() <= EPSILON {
                return;
            }
            let (cx, cy, cz) = (d[1], d[2], d[3]);
            let (sina, cosa, rot) = (d[11], d[12], d[13] != 0.0);
            let halve = amount / 2.0;
            match d[0] as i32 {
                0 => {
                    let dx = v.x - cx;
                    if ctx.random() < 0.5 {
                        let (mut ax, mut ay) = (cx + dx + halve, v.y);
                        if rot {
                            let (dx, dy) = (ax - cx, ay - cy);
                            (ax, ay) = (cx + dx * cosa + dy * sina, cy + dy * cosa - dx * sina);
                        }
                        (v.x, v.y) = (ax, ay);
                        v.color = fmod(v.color + d[5], 1.0);
                    } else {
                        let (mut bx, mut by) = (cx - dx - halve, v.y);
                        if rot {
                            let (dx, dy) = (bx - cx, by - cy);
                            (bx, by) = (cx + dx * cosa - dy * sina, cy + dy * cosa + dx * sina);
                        }
                        (v.x, v.y) = (bx, by);
                        v.color = fmod(v.color + d[8], 1.0);
                    }
                }
                1 => {
                    let dy = v.y - cy;
                    if ctx.random() < 0.5 {
                        let (mut ax, mut ay) = (v.x, cy + dy + halve);
                        if rot {
                            let (dx, dy) = (ax - cx, ay - cy);
                            (ax, ay) = (cx + dx * cosa + dy * sina, cy + dy * cosa - dx * sina);
                        }
                        (v.x, v.y) = (ax, ay);
                        v.color = fmod(v.color + d[6], 1.0);
                    } else {
                        let (mut bx, mut by) = (v.x, cy - dy - halve);
                        // Java applies the second color shift only when rotating.
                        if rot {
                            let (dx, dy) = (bx - cx, by - cy);
                            (bx, by) = (cx + dx * cosa - dy * sina, cy + dy * cosa + dx * sina);
                            v.color = fmod(v.color + d[9], 1.0);
                        }
                        (v.x, v.y) = (bx, by);
                    }
                }
                _ => {
                    // Java rotates around (centre_x, centre_y) in the xz plane.
                    let dz = v.z - cz;
                    if ctx.random() < 0.5 {
                        let (mut ax, mut az) = (v.x, cz + dz + halve);
                        if rot {
                            let (dx, dz) = (ax - cx, az - cz);
                            (ax, az) = (cx + dx * cosa + dz * sina, cy + dz * cosa - dx * sina);
                        }
                        (v.x, v.z) = (ax, az);
                        v.color = fmod(v.color + d[7], 1.0);
                    } else {
                        let (mut bx, mut bz) = (v.x, cz - dz - halve);
                        if rot {
                            let (dx, dz) = (bx - cx, bz - cz);
                            (bx, bz) = (cx + dx * cosa - dz * sina, cy + dz * cosa + dx * sina);
                        }
                        (v.x, v.z) = (bx, bz);
                        v.color = fmod(v.color + d[10], 1.0);
                    }
                }
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(ftoi(p[0]), 0, 2) as f64;
            let a = p[4] * M_2PI / 180.0 / 2.0;
            d.extend([a.sin(), a.cos(), if a.abs() > EPSILON { 1.0 } else { 0.0 }]);
            d
        }),
        wgsl: "if (abs(AMOUNT) > EPSILON) {
  let c = vec3<f32>(D(1), D(2), D(3));
  let sina = D(11);
  let cosa = D(12);
  let rot = D(13) != 0.0;
  let halve = AMOUNT / 2.0;
  let axis = i32(D(0));
  let first = rnd() < 0.5;
  var sgn = -1.0;
  if (first) { sgn = 1.0; }
  if (axis == 0) {
    var p = vec2<f32>(c.x + sgn * (v.x - c.x + halve), v.y);
    if (rot) {
      let q = p - c.xy;
      p = vec2<f32>(c.x + q.x * cosa + sgn * q.y * sina, c.y + q.y * cosa - sgn * q.x * sina);
    }
    v.x = p.x;
    v.y = p.y;
    v.color = fmod(v.color + select(D(8), D(5), first), 1.0);
  } else if (axis == 1) {
    var p = vec2<f32>(v.x, c.y + sgn * (v.y - c.y + halve));
    if (rot) {
      let q = p - c.xy;
      p = vec2<f32>(c.x + q.x * cosa + sgn * q.y * sina, c.y + q.y * cosa - sgn * q.x * sina);
      if (!first) { v.color = fmod(v.color + D(9), 1.0); }
    }
    v.x = p.x;
    v.y = p.y;
    if (first) { v.color = fmod(v.color + D(6), 1.0); }
  } else {
    var p = vec2<f32>(v.x, c.z + sgn * (v.z - c.z + halve));
    if (rot) {
      let q = p - c.xz;
      p = vec2<f32>(c.x + q.x * cosa + sgn * q.y * sina, c.y + q.y * cosa - sgn * q.x * sina);
    }
    v.x = p.x;
    v.z = p.y;
    v.color = fmod(v.color + select(D(10), D(7), first), 1.0);
  }
}",
    },
    VarDef {
        name: "post_benesi_crop",
        params: &[
            ParamDef::int("iterations", 12.0, 0.0, 250.0),
            ParamDef::new("bailout", 16.0),
            ParamDef::new("scaleX", 1.0),
            ParamDef::new("scaleY", 1.0),
            ParamDef::new("ScaleZ", 1.0),
            ParamDef::range("color", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            // d = [iterations, bailout, scaleX, scaleY, scaleZ, color]
            let (x1, y1, z1) = (v.x * d[2], v.y * d[3], v.z * d[4]);
            let (mut x, mut y, mut z) = (x1, y1, z1);
            for _ in 0..d[0] as i32 {
                let r1 = y * y + z * z;
                let nx = if x1 < 0.0 || x < r1.sqrt() { x * x - r1 } else { -x * x + r1 };
                let r1 = -1.0 / r1.sqrt() * 2.0 * x.abs();
                let ny = r1 * (y * y - z * z);
                let nz = r1 * 2.0 * y * z;
                (x, y, z) = (nx + x1, ny + y1, nz + z1);
                crop_step(v, amount, (x * x + y * y + z * z).sqrt(), d[1], d[5]);
            }
        },
        prepare: Some(|p, _| crop_prepare(p, 0, Some(5))),
        wgsl: concat!(
            "let p1 = vec3<f32>(v.x * D(2), v.y * D(3), v.z * D(4));
var x = p1.x;
var y = p1.y;
var z = p1.z;
let n = i32(D(0));
for (var i = 0; i < n; i++) {
  var r1 = y * y + z * z;
  var nx = -x * x + r1;
  if (p1.x < 0.0 || x < sqrt(r1)) { nx = x * x - r1; }
  r1 = -1.0 / sqrt(r1) * 2.0 * abs(x);
  let ny = r1 * (y * y - z * z);
  let nz = r1 * 2.0 * y * z;
  x = nx + p1.x;
  y = ny + p1.y;
  z = nz + p1.z;
  let r = sqrt(x * x + y * y + z * z);
",
            crop_step_wgsl!("D(1)", "D(5)"),
            "}"
        ),
    },
    VarDef {
        name: "post_bristorbrot_crop",
        params: &[
            ParamDef::int("iterations", 12.0, 0.0, 250.0),
            ParamDef::new("bailout", 16.0),
            ParamDef::new("scaleX", 1.0),
            ParamDef::new("scaleY", 1.0),
            ParamDef::new("ScaleZ", 1.0),
            ParamDef::range("color", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            // d = [iterations, bailout, scaleX, scaleY, scaleZ, color]
            let (x1, y1, z1) = (v.x * d[2], v.y * d[3], v.z * d[4]);
            let (mut x, mut y, mut z) = (x1, y1, z1);
            for _ in 0..d[0] as i32 {
                let nx = x * x - y * y - z * z;
                let ny = y * (2.0 * x - z);
                let nz = z * (2.0 * x + y);
                (x, y, z) = (nx + x1, ny + y1, nz + z1);
                crop_step(v, amount, (x * x + y * y + z * z).sqrt(), d[1], d[5]);
            }
        },
        prepare: Some(|p, _| crop_prepare(p, 0, Some(5))),
        wgsl: concat!(
            "let p1 = vec3<f32>(v.x * D(2), v.y * D(3), v.z * D(4));
var p = p1;
let n = i32(D(0));
for (var i = 0; i < n; i++) {
  p = vec3<f32>(p.x * p.x - p.y * p.y - p.z * p.z, p.y * (2.0 * p.x - p.z), p.z * (2.0 * p.x + p.y)) + p1;
  let r = length(p);
",
            crop_step_wgsl!("D(1)", "D(5)"),
            "}"
        ),
    },
    VarDef {
        name: "post_bulbtorus_crop",
        params: &[
            ParamDef::new("power1", 9.0),
            ParamDef::new("power2", 9.0),
            ParamDef::int("iterations", 12.0, 0.0, 250.0),
            ParamDef::new("bailout", 16.0),
            ParamDef::new("scaleX", 1.0),
            ParamDef::new("scaleY", 1.0),
            ParamDef::new("ScaleZ", 1.0),
            ParamDef::range("color", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            // d = [power1, power2, iterations, bailout, scaleX, scaleY, scaleZ, color]
            let (x1, y1, z1) = (v.x * d[4], v.y * d[5], v.z * d[6]);
            let (mut x, mut y, mut z) = (x1, y1, z1);
            for _ in 0..d[2] as i32 {
                let rh = (x * x + y * y).sqrt();
                let phi = y.atan2(x);
                let phipow = phi * d[0];
                let theta = rh.atan2(z);
                let px = x - phi.cos() * 1.5;
                let py = y - phi.sin() * 1.5;
                let rhrad = (px * px + py * py + z * z).sqrt();
                let rh1 = rhrad.powf(d[1]);
                let rh2 = rhrad.powf(d[0]);
                let thetapow = theta * d[1];
                let st = thetapow.sin() * rh2;
                (x, y, z) = (st * phipow.cos() + x1, st * phipow.sin() + y1, thetapow.cos() * rh1 + z1);
                crop_step(v, amount, (x * x + y * y + z * z).sqrt(), d[3], d[7]);
            }
        },
        prepare: Some(|p, _| crop_prepare(p, 2, None)),
        wgsl: concat!(
            "let p1 = vec3<f32>(v.x * D(4), v.y * D(5), v.z * D(6));
var p = p1;
let n = i32(D(2));
for (var i = 0; i < n; i++) {
  let rh = sqrt(p.x * p.x + p.y * p.y);
  let phi = atan2(p.y, p.x);
  let phipow = phi * D(0);
  let theta = atan2(rh, p.z);
  let px = p.x - cos(phi) * 1.5;
  let py = p.y - sin(phi) * 1.5;
  let rhrad = sqrt(px * px + py * py + p.z * p.z);
  let thetapow = theta * D(1);
  let st = sin(thetapow) * java_pow(rhrad, D(0));
  p = vec3<f32>(st * cos(phipow), st * sin(phipow), cos(thetapow) * java_pow(rhrad, D(1))) + p1;
  let r = length(p);
",
            crop_step_wgsl!("D(3)", "D(7)"),
            "}"
        ),
    },
    VarDef {
        name: "post_bwraps2",
        params: &[
            ParamDef::new("cellsize", 1.0),
            ParamDef::new("space", 0.0),
            ParamDef::new("gain", 2.0),
            ParamDef::new("inner_twist", 0.0),
            ParamDef::new("outer_twist", 0.0),
        ],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, _, _a, v, amount| {
            // d = [cellsize, space, gain, inner_twist, outer_twist, g2, r2, rfactor]
            let (vx, vy) = (v.x, v.y);
            let cs = d[0];
            if cs.abs() < SMALL_EPSILON {
                v.x = amount * vx;
                v.y = amount * vy;
                return;
            }
            let cx = ((vx / cs).floor() + 0.5) * cs;
            let cy = ((vy / cs).floor() + 0.5) * cs;
            let (mut lx, mut ly) = (vx - cx, vy - cy);
            if lx * lx + ly * ly > d[6] {
                v.x = amount * vx;
                v.y = amount * vy;
                return;
            }
            lx *= d[5];
            ly *= d[5];
            let r = d[7] / ((lx * lx + ly * ly) / 4.0 + 1.0);
            lx *= r;
            ly *= r;
            let r = (lx * lx + ly * ly) / d[6];
            let (s, c) = (d[3] * (1.0 - r) + d[4] * r).sin_cos();
            v.x = amount * (cx + c * lx + s * ly);
            v.y = amount * (cy - s * lx + c * ly);
        },
        prepare: Some(|p, _| {
            let radius = 0.5 * (p[0] / (1.0 + p[1] * p[1]));
            let g2 = p[2] * p[2] / p[0] + 1.0e-6;
            let mut max_bubble = g2 * radius;
            if max_bubble > 2.0 {
                max_bubble = 1.0;
            } else {
                max_bubble *= 1.0 / ((max_bubble * max_bubble) / 4.0 + 1.0);
            }
            let mut d = p.to_vec();
            d.extend([g2, radius * radius, radius / max_bubble]);
            d
        }),
        wgsl: "let cs = D(0);
var l = vec2<f32>(0.0);
var cc = vec2<f32>(v.x, v.y);
if (abs(cs) >= EPS) {
  cc = (floor(vec2<f32>(v.x, v.y) / cs) + 0.5) * cs;
  l = vec2<f32>(v.x, v.y) - cc;
  if (dot(l, l) > D(6)) {
    cc = vec2<f32>(v.x, v.y);
    l = vec2<f32>(0.0);
  } else {
    l *= D(5);
    l *= D(7) / (dot(l, l) / 4.0 + 1.0);
    let r = dot(l, l) / D(6);
    let theta = D(3) * (1.0 - r) + D(4) * r;
    let s = sin(theta);
    let c = cos(theta);
    l = vec2<f32>(c * l.x + s * l.y, -s * l.x + c * l.y);
  }
}
v.x = AMOUNT * (cc.x + l.x);
v.y = AMOUNT * (cc.y + l.y);",
    },
    VarDef {
        name: "post_circlecrop",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::new("x", 0.0),
            ParamDef::new("y", 0.0),
            ParamDef::range("scatter_area", 0.0, -1.0, 1.0),
            ParamDef::int("zero", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            // d = [radius, x, y, scatter_area, zero, cA]
            let (x0, y0, cr) = (d[1], d[2], d[0]);
            v.x -= x0;
            v.y -= y0;
            let rad = (v.x * v.x + v.y * v.y).sqrt();
            let ang = v.y.atan2(v.x);
            let rdc = cr + ctx.random() * 0.5 * d[5];
            let esc = rad > cr;
            let cr0 = d[4] as i32 == 1;
            let (s, c) = ang.sin_cos();
            v.do_hide = false;
            if cr0 && esc {
                v.x = 0.0;
                v.y = 0.0;
                v.do_hide = true;
            } else if !cr0 && esc {
                v.x = amount * rdc * c + x0;
                v.y = amount * rdc * s + y0;
            } else {
                v.x = amount * v.x + x0;
                v.y = amount * v.y + y0;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = ftoi(p[4]) as f64;
            d.push(java_max(-1.0, java_min(p[3], 1.0)));
            d
        }),
        wgsl: "let x0 = D(1);
let y0 = D(2);
let cr = D(0);
let px = v.x - x0;
let py = v.y - y0;
let rad = sqrt(px * px + py * py);
let ang = atan2(py, px);
let rdc = cr + rnd() * 0.5 * D(5);
let esc = rad > cr;
let cr0 = i32(D(4)) == 1;
v.hide = 0.0;
if (cr0 && esc) {
  v.x = 0.0;
  v.y = 0.0;
  v.hide = 1.0;
} else if (!cr0 && esc) {
  v.x = AMOUNT * rdc * cos(ang) + x0;
  v.y = AMOUNT * rdc * sin(ang) + y0;
} else {
  v.x = AMOUNT * px + x0;
  v.y = AMOUNT * py + y0;
}",
    },
    VarDef {
        name: "post_coastalbrot_crop",
        params: &[
            ParamDef::int("iterations", 12.0, 0.0, 250.0),
            ParamDef::new("bailout", 16.0),
            ParamDef::new("scaleX", 1.0),
            ParamDef::new("scaleY", 1.0),
            ParamDef::new("ScaleZ", 1.0),
            ParamDef::range("color", 1.0, 0.0, 1.0),
            ParamDef::range("alpha", 0.0, -180.0, 180.0),
            ParamDef::range("beta", 0.0, -180.0, 180.0),
            ParamDef::range("gamma", 0.0, -180.0, 180.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            // d = [iterations, bailout, scaleX, scaleY, scaleZ, color, alpha, beta, gamma, rot (mat3, 9)]
            let (x1, y1, z1) = (v.x * d[2], v.y * d[3], v.z * d[4]);
            let (mut x, mut y, mut z) = (x1, y1, z1);
            let mut r = (x * x + y * y + z * z).sqrt();
            let s3 = |t: f64| (M_PI / 3.0 + t * M_PI).sin().sin().sin();
            for _ in 0..d[0] as i32 {
                let temp = r.powf(7.7) * r;
                x = s3(x) + x1;
                y = s3(y) + y1;
                z = s3(z) * temp + z1;
                let (zx, zy, zz) = jmat3_vtimes(&d[9..18], x, y, z);
                r = (zx * zx + zy * zy + zz * zz).sqrt();
                crop_step(v, amount, r, d[1], d[5]);
            }
        },
        prepare: Some(|p, _| {
            let mut d = crop_prepare(p, 0, Some(5));
            d.extend(jmat3_euler(p[6], p[7], p[8]));
            d
        }),
        wgsl: concat!(
            "let p1 = vec3<f32>(v.x * D(2), v.y * D(3), v.z * D(4));
var p = p1;
var r = length(p);
let n = i32(D(0));
for (var i = 0; i < n; i++) {
  let temp = pow(r, 7.7) * r;
  p = sin(sin(sin(PI / 3.0 + p * PI)));
  p = vec3<f32>(p.x, p.y, p.z * temp) + p1;
  let q = vec3<f32>(p.x * D(9) + p.y * D(10) + p.z * D(11), p.x * D(12) + p.y * D(13) + p.z * D(14), p.x * D(15) + p.y * D(16) + p.z * D(17));
  r = length(q);
",
            crop_step_wgsl!("D(1)", "D(5)"),
            "}"
        ),
    },
    VarDef {
        name: "post_colorscale_wf",
        params: &[
            ParamDef::new("scale_x", 0.0),
            ParamDef::new("scale_y", 0.0),
            ParamDef::new("scale_z", 0.5),
            ParamDef::new("offset_z", 0.0),
            ParamDef::new("reset_z", 0.0),
            ParamDef::new("sides", 0.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            v.x += amount * d[0] * v.x;
            v.y += amount * d[1] * v.y;
            let dz = v.color * d[2] * amount + d[3];
            if d[4] > 0.0 {
                v.z = dz;
            } else if d[5] > 0.0 {
                v.z += dz * ctx.random();
            } else {
                v.z += dz;
            }
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * D(0) * v.x;
v.y += AMOUNT * D(1) * v.y;
let dz = v.color * D(2) * AMOUNT + D(3);
if (D(4) > 0.0) {
  v.z = dz;
} else if (D(5) > 0.0) {
  v.z += dz * rnd();
} else {
  v.z += dz;
}",
    },
    VarDef {
        name: "post_crop",
        params: &[
            ParamDef::new("left", -1.0),
            ParamDef::new("right", 1.0),
            ParamDef::new("top", -1.0),
            ParamDef::new("bottom", 1.0),
            ParamDef::range("scatter_area", 0.0, -1.0, 1.0),
            ParamDef::int("zero", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            // d = [left, right, top, bottom, scatter_area, zero, xmin 6, xmax 7, ymin 8, ymax 9, w 10, h 11]
            let (mut x, mut y) = (v.x, v.y);
            if (x < d[6] || x > d[7] || y < d[8] || y > d[9]) && d[5] != 0.0 {
                v.x = 0.0;
                v.y = 0.0;
                v.do_hide = true;
                return;
            }
            v.do_hide = false;
            if x < d[6] {
                x = d[6] + ctx.random() * d[10];
            } else if x > d[7] {
                x = d[7] - ctx.random() * d[10];
            }
            if y < d[8] {
                y = d[8] + ctx.random() * d[11];
            } else if y > d[9] {
                y = d[9] - ctx.random() * d[11];
            }
            v.x = amount * x;
            v.y = amount * y;
        },
        prepare: Some(|p, _| {
            let scatter = limit_value(p[4], -1.0, 1.0);
            let zero = limit_value(ftoi(p[5]), 0, 1) as f64;
            let (xmin, xmax) = (java_min(p[0], p[1]), java_max(p[0], p[1]));
            let (ymin, ymax) = (java_min(p[2], p[3]), java_max(p[2], p[3]));
            vec![p[0], p[1], p[2], p[3], scatter, zero, xmin, xmax, ymin, ymax, (xmax - xmin) * 0.5 * scatter, (ymax - ymin) * 0.5 * scatter]
        }),
        wgsl: "var x = v.x;
var y = v.y;
if ((x < D(6) || x > D(7) || y < D(8) || y > D(9)) && D(5) != 0.0) {
  v.x = 0.0;
  v.y = 0.0;
  v.hide = 1.0;
  return PtPair(a, v);
}
v.hide = 0.0;
if (x < D(6)) { x = D(6) + rnd() * D(10); } else if (x > D(7)) { x = D(7) - rnd() * D(10); }
if (y < D(8)) { y = D(8) + rnd() * D(11); } else if (y > D(9)) { y = D(9) - rnd() * D(11); }
v.x = AMOUNT * x;
v.y = AMOUNT * y;",
    },
    VarDef {
        name: "post_crop_box",
        params: &[ParamDef::new("width", 0.5), ParamDef::new("height", 0.5), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [width, height, invert]
            let sd = post_crop_box_sd(vec2(v.x, v.y), vec2(d[0], d[1]));
            post_crop_output(ctx, a, v, amount, sd, d[2]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: concat!(
            "fn post_crop_box_sd(p: vec2<f32>, b: vec2<f32>) -> f32 {
  let d = abs(p) - b;
  return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}
@body
let sd = post_crop_box_sd(vec2<f32>(v.x, v.y), vec2<f32>(D(0), D(1)));
",
            post_crop_sdf_tail!("2")
        ),
    },
    VarDef {
        name: "post_crop_cross",
        params: &[
            ParamDef::new("width", 0.5),
            ParamDef::new("Size", 0.5),
            ParamDef::new("round", 0.001),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [width, Size, round, invert]
            let mut si = vec2(d[0], d[1]);
            if si.x < si.y {
                si = vec2(si.y, si.x);
            }
            let ra = 0.1 * (d[2] * 1.2).sin();
            let sd = post_crop_cross_sd(vec2(v.x, v.y), si, ra);
            post_crop_output(ctx, a, v, amount, sd, d[3]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], crop_int_param(p[3], 0.0, 1.0)]),
        wgsl: concat!(
            "fn post_crop_cross_sd(p_in: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
  var p = abs(p_in);
  if (p.y > p.x) { p = p.yx; }
  let q = p - b;
  let k = max(q.y, q.x);
  var w = q;
  if (!(k > 0.0)) { w = vec2<f32>(b.y - p.x, -k); }
  return sign(k) * length(max(w, vec2<f32>(0.0))) + r;
}
@body
var si = vec2<f32>(D(0), D(1));
if (si.x < si.y) { si = si.yx; }
let sd = post_crop_cross_sd(vec2<f32>(v.x, v.y), si, 0.1 * sin(D(2) * 1.2));
",
            post_crop_sdf_tail!("3")
        ),
    },
    VarDef {
        name: "post_crop_polygon",
        params: &[ParamDef::int("type", 4.0, 0.0, 4.0), ParamDef::new("radius", 0.35), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [type, radius, invert]
            let p = vec2(v.x, v.y);
            let sd = match d[0] as i32 {
                4 => post_crop_polygon_hexagram(p, d[1]),
                3 => post_crop_polygon_octogon(p, d[1]),
                2 => post_crop_polygon_hexagon(p, d[1]),
                1 => post_crop_polygon_pentagon(p, d[1]),
                _ => length(p) - d[1],
            };
            post_crop_output(ctx, a, v, amount, sd, d[2]);
        },
        prepare: Some(|p, _| vec![crop_int_param(p[0], 0.0, 4.0), p[1], crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: concat!(
            "fn post_crop_polygon_hexagram(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec4<f32>(-0.5, 0.86602540378, 0.57735026919, 1.73205080757);
  var p = abs(p_in);
  p -= k.xy * 2.0 * min(dot(k.xy, p), 0.0);
  p -= k.yx * 2.0 * min(dot(k.yx, p), 0.0);
  p -= vec2<f32>(clamp(p.x, r * k.z, r * k.w), r);
  return length(p) * sign(p.y);
}
fn post_crop_polygon_octogon(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec3<f32>(-0.9238795325, 0.3826834323, 0.4142135623);
  var p = abs(p_in);
  p -= k.xy * (2.0 * min(dot(k.xy, p), 0.0));
  let k2 = vec2<f32>(-k.x, k.y);
  p -= k2 * (2.0 * min(dot(k2, p), 0.0));
  p -= vec2<f32>(clamp(p.x, -k.z * r, k.z * r), r);
  return length(p) * sign(p.y);
}
fn post_crop_polygon_hexagon(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec3<f32>(-0.866025404, 0.5, 0.577350269);
  var p = abs(p_in);
  p -= k.xy * (2.0 * min(dot(k.xy, p), 0.0));
  p -= vec2<f32>(clamp(p.x, -k.z * r, k.z * r), r);
  return length(p) * sign(p.y);
}
fn post_crop_polygon_pentagon(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec3<f32>(0.809016994, 0.587785252, 0.726542528);
  var p = vec2<f32>(abs(p_in.x), -p_in.y);
  let k1 = vec2<f32>(-k.x, k.y);
  p -= k1 * (2.0 * min(dot(k1, p), 0.0));
  p -= k.xy * (2.0 * min(dot(k.xy, p), 0.0));
  p -= vec2<f32>(clamp(p.x, -r * k.z, r * k.z), r);
  return length(p) * sign(p.y);
}
@body
let p = vec2<f32>(v.x, v.y);
let ty = i32(D(0));
var sd = length(p) - D(1);
if (ty == 4) {
  sd = post_crop_polygon_hexagram(p, D(1));
} else if (ty == 3) {
  sd = post_crop_polygon_octogon(p, D(1));
} else if (ty == 2) {
  sd = post_crop_polygon_hexagon(p, D(1));
} else if (ty == 1) {
  sd = post_crop_polygon_pentagon(p, D(1));
}
",
            post_crop_sdf_tail!("2")
        ),
    },
    VarDef {
        name: "post_crop_rhombus",
        params: &[ParamDef::new("width", 0.5), ParamDef::new("height", 0.5), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [width, height, invert]
            let sd = post_crop_rhombus_sd(vec2(v.x, v.y), vec2(d[0], d[1]));
            post_crop_output(ctx, a, v, amount, sd, d[2]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: concat!(
            "fn post_crop_rhombus_ndot(a: vec2<f32>, b: vec2<f32>) -> f32 { return a.x * b.x - a.y * b.y; }
fn post_crop_rhombus_sd(p: vec2<f32>, b: vec2<f32>) -> f32 {
  let q = abs(p);
  let h = clamp((-2.0 * post_crop_rhombus_ndot(q, b) + post_crop_rhombus_ndot(b, b)) / dot(b, b), -1.0, 1.0);
  let d = length(q - b * vec2<f32>(1.0 - h, 1.0 + h) * 0.5);
  return d * sign(q.x * b.y + q.y * b.x - b.x * b.y);
}
@body
let sd = post_crop_rhombus_sd(vec2<f32>(v.x, v.y), vec2<f32>(D(0), D(1)));
",
            post_crop_sdf_tail!("2")
        ),
    },
    VarDef {
        name: "post_crop_stars",
        params: &[
            ParamDef::new("radius", 0.35),
            ParamDef::int("n", 3.0, 3.0, 9.0),
            ParamDef::range("r2", 0.333, 0.0, 1.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [radius, n, r2, invert]
            let n = d[1] as i32;
            let m = 4.0 + d[2] * d[2] * (n as f64 * 2.0 - 4.0);
            let sd = post_crop_stars_sd(vec2(v.x, v.y), d[0], n, m);
            post_crop_output(ctx, a, v, amount, sd, d[3]);
        },
        prepare: Some(|p, _| vec![p[0], crop_int_param(p[1], 3.0, 9.0), limit_value(p[2], 0.0, 1.0), crop_int_param(p[3], 0.0, 1.0)]),
        wgsl: concat!(
            "fn post_crop_stars_sd(p_in: vec2<f32>, r: f32, n: f32, m: f32) -> f32 {
  let an = 3.141593 / n;
  let en = 6.283185 / m;
  let acs = vec2<f32>(cos(an), sin(an));
  let ecs = vec2<f32>(cos(en), sin(en));
  let bn = glsl_mod(glsl_atan2(p_in.x, p_in.y), 2.0 * an) - an;
  var p = vec2<f32>(cos(bn), abs(sin(bn))) * length(p_in);
  p -= acs * r;
  p += ecs * clamp(-dot(p, ecs), 0.0, r * acs.y / ecs.y);
  return length(p) * sign(p.x);
}
@body
let m = 4.0 + D(2) * D(2) * (D(1) * 2.0 - 4.0);
let sd = post_crop_stars_sd(vec2<f32>(v.x, v.y), D(0), D(1), m);
",
            post_crop_sdf_tail!("3")
        ),
    },
    VarDef {
        name: "post_crop_trapezoid",
        params: &[ParamDef::new("LMay", 0.5), ParamDef::new("LMin", 0.2), ParamDef::new("height", 0.5), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [LMay, LMin, height, invert]
            let sd = post_crop_trapezoid_sd(vec2(v.x, v.y), d[0], d[1], d[2]);
            post_crop_output(ctx, a, v, amount, sd, d[3]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], crop_int_param(p[3], 0.0, 1.0)]),
        wgsl: concat!(
            "fn post_crop_trapezoid_sd(p_in: vec2<f32>, r1: f32, r2: f32, he: f32) -> f32 {
  let k1 = vec2<f32>(r2, he);
  let k2 = vec2<f32>(r2 - r1, 2.0 * he);
  let p = vec2<f32>(abs(p_in.x), p_in.y);
  var rr = r2;
  if (p.y < 0.0) { rr = r1; }
  let ca = vec2<f32>(max(0.0, p.x - rr), abs(p.y) - he);
  let t0 = dot(k1 - p, k2) / dot(k2, k2);
  let cb = p - k1 + k2 * clamp(t0, 0.0, 1.0);
  var s = 1.0;
  if (cb.x < 0.0 && ca.y < 0.0) { s = -1.0; }
  return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}
@body
let sd = post_crop_trapezoid_sd(vec2<f32>(v.x, v.y), D(0), D(1), D(2));
",
            post_crop_sdf_tail!("3")
        ),
    },
    VarDef {
        name: "post_crop_triangle",
        params: &[
            ParamDef::new("x1", 0.0),
            ParamDef::new("y1", 0.5),
            ParamDef::new("x2", 0.5),
            ParamDef::new("y2", 0.5),
            ParamDef::new("x3", 0.0),
            ParamDef::new("y3", -0.5),
        ],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x1, y1, x2, y2, x3, y3]
            let (x1, y1, x2, y2, x3, y3) = (d[0], d[1], d[2], d[3], d[4], d[5]);
            let (x, y) = (v.x, v.y);
            let area = post_crop_triangle_area(x1, y1, x2, y2, x3, y3);
            let a1 = post_crop_triangle_area(x, y, x2, y2, x3, y3);
            let a2 = post_crop_triangle_area(x1, y1, x, y, x3, y3);
            let a3 = post_crop_triangle_area(x1, y1, x2, y2, x, y);
            // Java quirk: exact floating point comparison.
            v.do_hide = area != a1 + a2 + a3;
            v.x = amount * x;
            v.y = amount * y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "fn post_crop_triangle_area(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) -> f32 {
  return abs((x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2)) / 2.0);
}
@body
let x = v.x;
let y = v.y;
let area = post_crop_triangle_area(D(0), D(1), D(2), D(3), D(4), D(5));
let a1 = post_crop_triangle_area(x, y, D(2), D(3), D(4), D(5));
let a2 = post_crop_triangle_area(D(0), D(1), x, y, D(4), D(5));
let a3 = post_crop_triangle_area(D(0), D(1), D(2), D(3), x, y);
v.hide = 1.0;
if (area == a1 + a2 + a3) { v.hide = 0.0; }
v.x = AMOUNT * x;
v.y = AMOUNT * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "post_crop_vesica",
        params: &[ParamDef::new("width", 0.2), ParamDef::new("height", 0.1), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [width, height, invert]
            let sd = post_crop_vesica_sd(vec2(v.x, v.y), 1.0 + d[1], 0.8 + d[0]);
            post_crop_output(ctx, a, v, amount, sd, d[2]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: concat!(
            "fn post_crop_vesica_sd(p_in: vec2<f32>, r: f32, d: f32) -> f32 {
  let p = abs(p_in);
  let b = sqrt(r * r - d * d);
  if ((p.y - b) * d > p.x * b) {
    return length(p - vec2<f32>(0.0, b));
  }
  return length(p - vec2<f32>(-d, 0.0)) - r;
}
@body
let sd = post_crop_vesica_sd(vec2<f32>(v.x, v.y), 1.0 + D(1), 0.8 + D(0));
",
            post_crop_sdf_tail!("2")
        ),
    },
    VarDef {
        name: "post_crop_x",
        params: &[ParamDef::new("radius", 0.1), ParamDef::range("width", 0.5, 0.0, 1.0), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [radius, width, invert]
            let p = abs(vec2(v.x, v.y));
            let sd = length(p - Vec2::splat(java_min(p.x + p.y, d[1]) * 0.5)) - d[0];
            post_crop_output(ctx, a, v, amount, sd, d[2]);
        },
        prepare: Some(|p, _| vec![p[0], limit_value(p[1], 0.0, 1.0), crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: concat!(
            "let p = abs(vec2<f32>(v.x, v.y));
let sd = length(p - vec2<f32>(min(p.x + p.y, D(1)) * 0.5)) - D(0);
",
            post_crop_sdf_tail!("2")
        ),
    },
    VarDef {
        name: "post_crosscrop",
        params: &[
            ParamDef::new("x", 1.0),
            ParamDef::new("y", 1.0),
            ParamDef::new("z", 1.0),
            ParamDef::range("rotation", 0.0, -180.0, 180.0),
            ParamDef::int("reverse", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [x, y, z, rotation, reverse]
            let rot = d[3] * M_PI / 180.0;
            let rx = v.x * rot.cos() - v.y * rot.sin();
            let ry = v.y * rot.cos() + v.x * rot.sin();
            let inside = (rx <= d[0] && rx >= -d[0] || ry <= d[1] && ry >= -d[1]) && v.z <= d[2] && v.z >= -d[2];
            if inside == (d[4] as i32 == 1) {
                v.x = 0.0;
                v.y = 0.0;
                v.z = 0.0;
                v.do_hide = true;
                return;
            }
            v.do_hide = false;
            v.x *= amount;
            v.y *= amount;
            v.z *= amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], p[3], crop_int_param(p[4], 0.0, 1.0)]),
        wgsl: "let rot = D(3) * PI / 180.0;
let rx = v.x * cos(rot) - v.y * sin(rot);
let ry = v.y * cos(rot) + v.x * sin(rot);
let inside = (rx <= D(0) && rx >= -D(0) || ry <= D(1) && ry >= -D(1)) && v.z <= D(2) && v.z >= -D(2);
if (inside == (i32(D(4)) == 1)) {
  v.x = 0.0;
  v.y = 0.0;
  v.z = 0.0;
  v.hide = 1.0;
} else {
  v.hide = 0.0;
  v.x *= AMOUNT;
  v.y *= AMOUNT;
  v.z *= AMOUNT;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    VarDef {
        name: "post_c_symmetry",
        params: &[ParamDef::new("p1", 3.0), ParamDef::new("p2", 3.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let t = d[0] * v.y.atan2(v.x);
            let l = (v.x * v.x + v.y * v.y).sqrt().powf(d[1]);
            v.x = amount * (t.cos() * l);
            v.y = amount * (t.sin() * l);
            if ctx.preserve_z {
                v.z = amount * a.z;
            }
        },
        prepare: None,
        wgsl: "let t = D(0) * atan2(v.y, v.x);
let l = pow(length(vec2<f32>(v.x, v.y)), D(1));
v.x = AMOUNT * (cos(t) * l);
v.y = AMOUNT * (sin(t) * l);
if (PRESERVE_Z) { v.z = AMOUNT * a.z; }",
    },
    VarDef {
        name: "post_curl",
        params: &[ParamDef::new("c1", 0.0), ParamDef::new("c2", 0.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, _, _a, v, amount| {
            let (c1, c2) = (d[0] * amount, d[1] * amount);
            let (x, y) = (v.x, v.y);
            let re = 1.0 + c1 * x + c2 * (sqr(x) - sqr(y));
            let im = c1 * y + 2.0 * c2 * x * y;
            let r = sqr(re) + sqr(im) + SMALL_EPSILON;
            v.x = (x * re + y * im) / r;
            v.y = (y * re - x * im) / r;
        },
        prepare: None,
        wgsl: "let c1 = D(0) * AMOUNT;
let c2 = D(1) * AMOUNT;
let x = v.x;
let y = v.y;
let re = 1.0 + c1 * x + c2 * (x * x - y * y);
let im = c1 * y + 2.0 * c2 * x * y;
let r = re * re + im * im + EPS;
v.x = (x * re + y * im) / r;
v.y = (y * re - x * im) / r;",
    },
    VarDef {
        name: "post_curl3D",
        params: &[ParamDef::new("cx", 0.0), ParamDef::new("cy", 0.0), ParamDef::new("cz", 0.0)],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            let (cx, cy, cz) = (amount * d[0], amount * d[1], amount * d[2]);
            let c_2 = sqr(cx) + sqr(cy) + sqr(cz);
            let r2 = sqr(v.x) + sqr(v.y) + sqr(v.z);
            // Java quirk: the y term is subtracted.
            let r = 1.0 / (r2 * c_2 + 2.0 * cx * v.x - 2.0 * cy * v.y + 2.0 * cz * v.z + 1.0);
            v.x = r * (v.x + cx * r2);
            v.y = r * (v.y + cy * r2);
            v.z = r * (v.z + cz * r2);
        },
        prepare: None,
        wgsl: "let c = AMOUNT * vec3<f32>(D(0), D(1), D(2));
let c_2 = dot(c, c);
let r2 = v.x * v.x + v.y * v.y + v.z * v.z;
let r = 1.0 / (r2 * c_2 + 2.0 * c.x * v.x - 2.0 * c.y * v.y + 2.0 * c.z * v.z + 1.0);
v.x = r * (v.x + c.x * r2);
v.y = r * (v.y + c.y * r2);
v.z = r * (v.z + c.z * r2);",
    },
    VarDef {
        name: "post_dcztransl",
        params: &[
            ParamDef::range("x0", 0.0, 0.0, 1.0),
            ParamDef::range("x1", 1.0, 0.0, 1.0),
            ParamDef::new("factor", 1.0),
            ParamDef::int("overwrite", 1.0, 0.0, 1.0),
            ParamDef::int("clamp", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            // d = [x0, x1, factor, overwrite, clamp, x0 (min), x1 - x0]
            let mut zf = d[2] * (a.color - d[5]) / d[6];
            if d[4] != 0.0 {
                zf = zf.clamp(0.0, 1.0);
            }
            v.x *= amount;
            v.y *= amount;
            v.z = if d[3] == 0.0 { amount * v.z * zf } else { amount * zf };
        },
        prepare: Some(|p, _| {
            let (x0, x1) = (limit_value(p[0], 0.0, 1.0), limit_value(p[1], 0.0, 1.0));
            let lo = if x0 < x1 { x0 } else { x1 };
            let hi = if x0 > x1 { x0 } else { x1 };
            let diff = if hi - lo == 0.0 { SMALL_EPSILON } else { hi - lo };
            vec![x0, x1, p[2], limit_value(ftoi(p[3]), 0, 1) as f64, limit_value(ftoi(p[4]), 0, 1) as f64, lo, diff]
        }),
        wgsl: "var zf = D(2) * (a.color - D(5)) / max(D(6), EPS);
if (D(4) != 0.0) { zf = clamp(zf, 0.0, 1.0); }
v.x *= AMOUNT;
v.y *= AMOUNT;
if (D(3) == 0.0) { v.z = AMOUNT * v.z * zf; } else { v.z = AMOUNT * zf; }",
    },
    VarDef {
        name: "post_depth",
        params: &[ParamDef::new("power", 1.0)],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let mut coeff = v.z.abs();
            if coeff != 0.0 && d[0] != 1.0 {
                coeff = (coeff.ln() * d[0]).exp();
            }
            v.x += amount * (a.x + v.x * coeff);
            v.y += amount * (a.y + v.y * coeff);
            v.z += amount * (a.z + v.z * coeff);
        },
        prepare: None,
        wgsl: "var coeff = abs(v.z);
if (coeff != 0.0 && D(0) != 1.0) { coeff = exp(log(coeff) * D(0)); }
v.x += AMOUNT * (a.x + v.x * coeff);
v.y += AMOUNT * (a.y + v.y * coeff);
v.z += AMOUNT * (a.z + v.z * coeff);",
    },
    VarDef {
        name: "post_falloff2",
        params: &[
            ParamDef::range("scatter", 1.0, 0.0, 2.0),
            ParamDef::range("mindist", 0.5, 0.0, 2.0),
            ParamDef::range("mul_x", 1.0, 0.0, 1.0),
            ParamDef::range("mul_y", 1.0, 0.0, 1.0),
            ParamDef::range("mul_z", 0.0, 0.0, 1.0),
            ParamDef::range("mul_c", 0.0, 0.0, 1.0),
            ParamDef::new("x0", 0.0),
            ParamDef::new("y0", 0.0),
            ParamDef::new("z0", 0.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
            ParamDef::int("type", 0.0, 0.0, 2.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [scatter, mindist, mul_x, mul_y, mul_z, mul_c, x0, y0, z0, invert, type, rmax]
            let (x, y, z) = (v.x, v.y, v.z);
            let mut dist = (sqr(x - d[6]) + sqr(y - d[7]) + sqr(z - d[8])).sqrt();
            if d[9] as i32 != 0 {
                dist = 1.0 - dist;
            }
            if dist < 0.0 {
                dist = 0.0;
            }
            dist = (dist - d[1]) * d[11];
            if dist < 0.0 {
                dist = 0.0;
            }
            match d[10] as i32 {
                1 => {
                    let r_in = (x * x + y * y + z * z).sqrt() + 1e-6;
                    let sigma = (z / r_in).asin() + d[4] * ctx.random() * dist;
                    let phi = y.atan2(x) + d[3] * ctx.random() * dist;
                    let r = r_in + d[2] * ctx.random() * dist;
                    let (sins, coss) = sigma.sin_cos();
                    let (sinp, cosp) = phi.sin_cos();
                    v.x += amount * (r * coss * cosp);
                    v.y += amount * (r * coss * sinp);
                    v.z += amount * sins;
                }
                2 => {
                    let sigma = dist * ctx.random() * 2.0 * M_PI;
                    let phi = dist * ctx.random() * M_PI;
                    let r = dist * ctx.random();
                    let (sins, coss) = sigma.sin_cos();
                    let (sinp, cosp) = phi.sin_cos();
                    v.x += amount * (x + d[2] * r * coss * cosp);
                    v.y += amount * (y + d[3] * r * coss * sinp);
                    v.z += amount * (z + d[4] * r * sins);
                }
                _ => {
                    v.x += amount * (x + d[2] * ctx.random() * dist);
                    v.y += amount * (y + d[3] * ctx.random() * dist);
                    v.z += amount * (z + d[4] * ctx.random() * dist);
                }
            }
            v.color = frac(v.color + d[5] * ctx.random() * dist).abs();
        },
        prepare: Some(|p, _| {
            let scatter = if p[0] < 1.0e-6 { 1.0e-6 } else { p[0] };
            let mindist = if p[1] < 0.0 { 0.0 } else { p[1] };
            vec![
                scatter,
                mindist,
                limit_value(p[2], 0.0, 1.0),
                limit_value(p[3], 0.0, 1.0),
                limit_value(p[4], 0.0, 1.0),
                limit_value(p[5], 0.0, 1.0),
                p[6],
                p[7],
                p[8],
                limit_value(ftoi(p[9]), 0, 1) as f64,
                limit_value(ftoi(p[10]), 0, 2) as f64,
                0.04 * scatter,
            ]
        }),
        wgsl: "let x = v.x;
let y = v.y;
let z = v.z;
var dist = length(vec3<f32>(x - D(6), y - D(7), z - D(8)));
if (i32(D(9)) != 0) { dist = 1.0 - dist; }
dist = max((max(dist, 0.0) - D(1)) * D(11), 0.0);
let ty = i32(D(10));
if (ty == 1) {
  let r_in = sqrt(x * x + y * y + z * z) + 1e-6;
  let sigma = asin(clamp(z / r_in, -1.0, 1.0)) + D(4) * rnd() * dist;
  let phi = atan2(y, x) + D(3) * rnd() * dist;
  let r = r_in + D(2) * rnd() * dist;
  v.x += AMOUNT * (r * cos(sigma) * cos(phi));
  v.y += AMOUNT * (r * cos(sigma) * sin(phi));
  v.z += AMOUNT * sin(sigma);
} else if (ty == 2) {
  let sigma = dist * rnd() * TWO_PI;
  let phi = dist * rnd() * PI;
  let r = dist * rnd();
  v.x += AMOUNT * (x + D(2) * r * cos(sigma) * cos(phi));
  v.y += AMOUNT * (y + D(3) * r * cos(sigma) * sin(phi));
  v.z += AMOUNT * (z + D(4) * r * sin(sigma));
} else {
  v.x += AMOUNT * (x + D(2) * rnd() * dist);
  v.y += AMOUNT * (y + D(3) * rnd() * dist);
  v.z += AMOUNT * (z + D(4) * rnd() * dist);
}
let c = v.color + D(5) * rnd() * dist;
v.color = abs(c - trunc(c));",
    },
    VarDef {
        name: "post_falloff3",
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
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            let vout = falloff3_vout(d, ctx, [v.x, v.y, v.z, v.color]);
            v.x = vout[0] * amount;
            v.y = vout[1] * amount;
            v.z = vout[2] * amount;
            v.color = fmod(vout[3], 1.0).abs();
        },
        prepare: Some(|p, _| {
            let strength = if p[2] < 1.0e-6 { 1.0e-6 } else { p[2] };
            let min_dist = if p[3] < 0.0 { 0.0 } else { p[3] };
            let fl = |x: f64, hi: i32| limit_value(ftoi(x), 0, hi) as f64;
            vec![
                fl(p[0], 2),
                fl(p[1], 1),
                strength,
                min_dist,
                fl(p[4], 1),
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
        }),
        wgsl: "fn post_falloff3_sgnd(x: f32) -> f32 { return select(1.0, -1.0, x < 0.0); }
fn post_falloff3_log_scale(x: f32) -> f32 {
  if (x == 0.0) { return 0.0; }
  return log((abs(x) + 1.0) * 2.718281828459045) * post_falloff3_sgnd(x) / 2.718281828459045;
}
fn post_falloff3_log_map(x: f32) -> f32 {
  if (x == 0.0) { return 0.0; }
  return (2.718281828459045 + log(x * 2.718281828459045)) / 4.0 * post_falloff3_sgnd(x);
}
@body
let rx = rnd() - 0.5;
let ry = rnd() - 0.5;
let rz = rnd() - 0.5;
let rc = rnd() - 0.5;
let vin = vec4<f32>(v.x, v.y, v.z, v.color);
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
  if (r_max > EPSILON) { coeff = dist + alpha * (post_falloff3_log_map(dist) - dist); }
  vo = vin + vec4<f32>(post_falloff3_log_map(D(5)) * post_falloff3_log_scale(rx), post_falloff3_log_map(D(6)) * post_falloff3_log_scale(ry),
    post_falloff3_log_map(D(7)) * post_falloff3_log_scale(rz), post_falloff3_log_map(D(8)) * post_falloff3_log_scale(rc)) * coeff;
}
v.x = vo.x * AMOUNT;
v.y = vo.y * AMOUNT;
v.z = vo.z * AMOUNT;
v.color = abs(fmod(vo.w, 1.0));",
    },
    VarDef {
        name: "post_flatten",
        params: &[],
        kind: VarKind::Post,
        is_3d: true,
        apply: |_, _, _a, v, amount| {
            v.z *= 1.0 - java_min(amount.abs(), 1.0);
        },
        prepare: None,
        wgsl: "v.z = v.z * (1.0 - min(abs(AMOUNT), 1.0));",
    },
    VarDef {
        name: "post_heat",
        params: &[
            ParamDef::new("theta_period", 0.0),
            ParamDef::new("theta_phase", 0.0),
            ParamDef::new("theta_amp", 0.0),
            ParamDef::new("phi_period", 0.0),
            ParamDef::new("phi_phase", 0.0),
            ParamDef::new("phi_amp", 0.0),
            ParamDef::new("r_period", 0.0),
            ParamDef::new("r_phase", 0.0),
            ParamDef::new("r_amp", 0.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            let inv = |p: f64| if p == 0.0 { 0.0 } else { 1.0 / p };
            let (tx, px, rx) = (inv(d[0]), inv(d[3]), inv(d[6]));
            let (at, bt, ct) = (amount * d[2], M_2PI * tx, d[1] * tx);
            let (ap, bp, cp) = (amount * d[5], M_2PI * px, d[4] * px);
            let (ar, br, cr) = (amount * d[8], M_2PI * rx, d[7] * rx);
            let mut r = (sqr(v.x) + sqr(v.y) + sqr(v.z)).sqrt();
            let atant = v.y.atan2(v.x);
            r += ar * (br * r + cr).sin();
            let (sint, cost) = (at * (bt * r + ct).sin() + atant).sin_cos();
            let acosp = if r != 0.0 { v.z / r } else { 0.0 };
            let (sinp, cosp) = (ap * (bp * r + cp).sin() + acosp.acos()).sin_cos();
            v.x = r * cost * sinp;
            v.y = r * sint * sinp;
            v.z = r * cosp;
        },
        prepare: None,
        wgsl: "var tx = 0.0;
if (D(0) != 0.0) { tx = 1.0 / D(0); }
var px = 0.0;
if (D(3) != 0.0) { px = 1.0 / D(3); }
var rx = 0.0;
if (D(6) != 0.0) { rx = 1.0 / D(6); }
var r = sqrt(v.x * v.x + v.y * v.y + v.z * v.z);
let atant = atan2(v.y, v.x);
r += AMOUNT * D(8) * sin(TWO_PI * rx * r + D(7) * rx);
let th = AMOUNT * D(2) * sin(TWO_PI * tx * r + D(1) * tx) + atant;
var acosp = 0.0;
if (r != 0.0) { acosp = v.z / r; }
let ph = AMOUNT * D(5) * sin(TWO_PI * px * r + D(4) * px) + acos(acosp);
v.x = r * cos(th) * sin(ph);
v.y = r * sin(th) * sin(ph);
v.z = r * cos(ph);",
    },
    VarDef {
        name: "post_julia3Dq",
        // Java default power is random (+-2..7); 3 is used here.
        params: &[ParamDef::int("power", 3.0, -12.0, 12.0), ParamDef::int("divisor", 2.0, -12.0, 12.0)],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [power, divisor, inv_power, abs_inv_power, half_inv_power, inv_power_2pi]
            let a = v.y.atan2(v.x) * d[2] + ctx.random_int(i32::MAX as usize) as f64 * d[5];
            let (sina, cosa) = a.sin_cos();
            let z = v.z * d[3];
            let r2d = sqr(v.x) + sqr(v.y);
            let r = amount * (r2d + sqr(z)).powf(d[4]);
            v.z = r * z;
            let r = r * r2d.sqrt();
            v.x = r * cosa;
            v.y = r * sina;
        },
        prepare: Some(|p, _| {
            let (power, divisor) = (ftoi(p[0]) as f64, ftoi(p[1]) as f64);
            let inv_power = divisor / power;
            vec![power, divisor, inv_power, inv_power.abs(), 0.5 * inv_power - 0.5, M_2PI / power]
        }),
        // GPU: the random multiple of 2pi/power is drawn from 0..|power| (same angles).
        wgsl: "let k = floor(rnd() * abs(D(0)));
let an = atan2(v.y, v.x) * D(2) + k * D(5);
let z = v.z * D(3);
let r2d = v.x * v.x + v.y * v.y;
let r = AMOUNT * pow(r2d + z * z, D(4));
v.z = r * z;
let r1 = r * sqrt(r2d);
v.x = r1 * cos(an);
v.y = r1 * sin(an);",
    },
    VarDef {
        name: "post_juliaq",
        // Java default power is random (+-2..7); 3 is used here.
        params: &[ParamDef::int("power", 3.0, -12.0, 12.0), ParamDef::int("divisor", 2.0, -12.0, 12.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            // d = [power, divisor, half_inv_power, inv_power, inv_power_2pi]
            let a = v.y.atan2(v.x) * d[3] + ctx.random_int(i32::MAX as usize) as f64 * d[4];
            let (sina, cosa) = a.sin_cos();
            let r = amount * (sqr(v.x) + sqr(v.y)).powf(d[2]);
            v.x = r * cosa;
            v.y = r * sina;
        },
        prepare: Some(|p, _| {
            let (power, divisor) = (ftoi(p[0]) as f64, ftoi(p[1]) as f64);
            vec![power, divisor, 0.5 * divisor / power, divisor / power, M_2PI / power]
        }),
        // GPU: the random multiple of 2pi/power is drawn from 0..|power| (same angles).
        wgsl: "let k = floor(rnd() * abs(D(0)));
let an = atan2(v.y, v.x) * D(3) + k * D(4);
let r = AMOUNT * pow(v.x * v.x + v.y * v.y, D(2));
v.x = r * cos(an);
v.y = r * sin(an);",
    },
    VarDef {
        name: "post_log_tile2",
        params: &[ParamDef::new("spreadx", 2.0), ParamDef::new("spready", 2.0), ParamDef::new("spreadz", 0.0)],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            let sx = if ctx.random() < 0.5 { d[0] } else { -d[0] };
            let sy = if ctx.random() < 0.5 { d[1] } else { -d[1] };
            let sz = if ctx.random() < 0.5 { d[2] } else { -d[2] };
            v.x = amount * (v.x + sx * java_round(ctx.random().ln()));
            v.y = amount * (v.y + sy * java_round(ctx.random().ln()));
            v.z = amount * (v.z + sz * java_round(ctx.random().ln()));
        },
        prepare: None,
        wgsl: "let sx = select(-D(0), D(0), rnd() < 0.5);
let sy = select(-D(1), D(1), rnd() < 0.5);
let sz = select(-D(2), D(2), rnd() < 0.5);
v.x = AMOUNT * (v.x + sx * java_round(log(rnd())));
v.y = AMOUNT * (v.y + sy * java_round(log(rnd())));
v.z = AMOUNT * (v.z + sz * java_round(log(rnd())));",
    },
    VarDef {
        name: "post_mandelbox3d_crop",
        params: &[
            ParamDef::new("scale", 2.0),
            ParamDef::new("radius", 0.5),
            ParamDef::new("f", 1.0),
            ParamDef::int("iterations", 12.0, 0.0, 250.0),
            ParamDef::new("bailout", 16.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            // d = [scale, radius, f, iterations, bailout]
            let fold = |t: f64| if t > 1.0 { 2.0 - t } else if t < -1.0 { -2.0 - t } else { t };
            let (x1, y1, z1) = (v.x * 6.0, v.y * 6.0, v.z * 6.0);
            let (mut x, mut y, mut z) = (x1, y1, z1);
            let (scale, r, f) = (d[0], d[1], d[2]);
            for _ in 0..d[3] as i32 {
                x = fold(x) * f;
                y = fold(y) * f;
                z = fold(z) * f;
                let mag = (x * x + y * y + z * z).sqrt();
                if mag < r {
                    (x, y, z) = (x / (r * r), y / (r * r), z / (r * r));
                } else if mag < 1.0 {
                    (x, y, z) = (x / (mag * mag), y / (mag * mag), z / (mag * mag));
                }
                (x, y, z) = (x * scale + x1, y * scale + y1, z * scale + z1);
                if mag < d[4] {
                    v.do_hide = false;
                    v.x *= amount;
                    v.y *= amount;
                    v.z *= amount;
                } else {
                    v.do_hide = true;
                    v.x = 0.0;
                    v.y = 0.0;
                    v.z = 0.0;
                }
            }
        },
        prepare: Some(|p, _| crop_prepare(p, 3, None)),
        wgsl: "let p1 = vec3<f32>(v.x, v.y, v.z) * 6.0;
var p = p1;
let r = D(1);
let n = i32(D(3));
for (var i = 0; i < n; i++) {
  p = select(select(p, -2.0 - p, p < vec3<f32>(-1.0)), 2.0 - p, p > vec3<f32>(1.0));
  p *= D(2);
  let mag = length(p);
  if (mag < r) { p = p / (r * r); } else if (mag < 1.0) { p = p / (mag * mag); }
  p = p * D(0) + p1;
  if (mag < D(4)) {
    v.hide = 0.0;
    v.x *= AMOUNT;
    v.y *= AMOUNT;
    v.z *= AMOUNT;
  } else {
    v.hide = 1.0;
    v.x = 0.0;
    v.y = 0.0;
    v.z = 0.0;
  }
}",
    },
    VarDef {
        name: "post_mandelbulb3d_crop",
        params: &[
            ParamDef::new("power", 9.0),
            ParamDef::new("xz", 0.0),
            ParamDef::int("iterations", 12.0, 0.0, 250.0),
            ParamDef::new("bailout", 2.0),
            ParamDef::int("filled", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [power, xz, iterations, bailout, filled]
            if d[4] as i32 == 1 {
                let fill = (amount + 1.0) * 2.0;
                v.x = ctx.random() * fill - fill / 2.0;
                v.y = ctx.random() * fill - fill / 2.0;
                v.z = ctx.random() * fill - fill / 2.0;
            }
            let (mut x, mut y, mut z) = (v.x, v.y, v.z);
            let pw = d[0];
            for i in 0..d[2] as i32 {
                let sq_r = (x * x + y * y + z * z).sqrt();
                let sq_xz = (x * x + z * z).sqrt();
                let r = sq_r.powf(pw);
                let theta = (sq_xz + d[1]).atan2(y) * pw;
                let zangle = x.atan2(z) * pw;
                let xf = zangle.sin() * theta.sin() * r + x;
                let yf = theta.cos() * r + y;
                let zf = theta.sin() * zangle.cos() * r + z;
                (x, y, z) = (xf, yf, zf);
                v.color = java_min(1.0, 8.0 / (i + 1) as f64);
                if (xf * xf + yf * yf + zf * zf).sqrt() < d[3] {
                    v.do_hide = false;
                    v.x *= amount;
                    v.y *= amount;
                    v.z *= amount;
                } else {
                    v.do_hide = true;
                    v.x = 0.0;
                    v.y = 0.0;
                    v.z = 0.0;
                }
            }
        },
        prepare: Some(|p, _| {
            let mut d = crop_prepare(p, 2, None);
            d[4] = limit_value(ftoi(p[4]), 0, 1) as f64;
            d
        }),
        wgsl: "if (i32(D(4)) == 1) {
  let fill = (AMOUNT + 1.0) * 2.0;
  v.x = rnd() * fill - fill / 2.0;
  v.y = rnd() * fill - fill / 2.0;
  v.z = rnd() * fill - fill / 2.0;
}
var p = vec3<f32>(v.x, v.y, v.z);
let pw = D(0);
let n = i32(D(2));
for (var i = 0; i < n; i++) {
  let r = java_pow(length(p), pw);
  let theta = atan2(length(p.xz) + D(1), p.y) * pw;
  let zangle = atan2(p.x, p.z) * pw;
  p = vec3<f32>(sin(zangle) * sin(theta) * r, cos(theta) * r, sin(theta) * cos(zangle) * r) + p;
  v.color = min(1.0, 8.0 / f32(i + 1));
  if (length(p) < D(3)) {
    v.hide = 0.0;
    v.x *= AMOUNT;
    v.y *= AMOUNT;
    v.z *= AMOUNT;
  } else {
    v.hide = 1.0;
    v.x = 0.0;
    v.y = 0.0;
    v.z = 0.0;
  }
}",
    },
    VarDef {
        name: "post_point_crop",
        params: &[
            ParamDef::new("x_offset", 0.0),
            ParamDef::new("y_offset", 0.0),
            ParamDef::new("z_offset", 0.0),
            ParamDef::int("point_hide_enable", 1.0, 0.0, 1.0),
            ParamDef::int("point_crop_enable", 1.0, 0.0, 1.0),
            ParamDef::new("x_width", 0.0),
            ParamDef::new("y_width", 0.0),
            ParamDef::new("z_width", 0.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [x_offset, y_offset, z_offset, hide, crop, x_width, y_width, z_width]
            let (xo, yo, zo, xw, yw, zw) = (d[0], d[1], d[2], d[5], d[6], d[7]);
            let boundaries = if xw + yw + zw != 0.0 {
                let bx = v.x - xo <= xw && v.x - xo >= -xw;
                let by = v.y - yo <= yw && v.y - yo >= -yw;
                let bz = v.z - zo <= zw && v.z - zo >= -zw && v.z != 0.0;
                bx || by || bz
            } else {
                v.x == xo && v.y == yo && v.z == zo
            };
            let (hide, crop) = (d[3] as i32 == 1, d[4] as i32 == 1);
            // The last uncropped point lives across calls (per render thread here).
            if boundaries {
                if crop {
                    [v.x, v.y] = *state_slot(ctx, d, 0, [0.0; 2]);
                    [v.z, v.color] = *state_slot(ctx, d, 1, [0.0; 2]);
                }
                if hide {
                    v.do_hide = true;
                }
            } else {
                if hide {
                    v.do_hide = false;
                }
                if crop {
                    *state_slot(ctx, d, 0, [0.0; 2]) = [v.x, v.y];
                    *state_slot(ctx, d, 1, [0.0; 2]) = [v.z, v.color];
                }
                ctx.pass_z(a, v, amount);
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[3] = crop_int_param(p[3], 0.0, 1.0);
            d[4] = crop_int_param(p[4], 0.0, 1.0);
            d
        }),
        // GPU: no memory of the last uncropped point; cropped points move to the origin.
        wgsl: "let o = vec3<f32>(D(0), D(1), D(2));
let w = vec3<f32>(D(5), D(6), D(7));
let q = vec3<f32>(v.x, v.y, v.z) - o;
var boundaries = v.x == o.x && v.y == o.y && v.z == o.z;
if (w.x + w.y + w.z != 0.0) {
  boundaries = (q.x <= w.x && q.x >= -w.x) || (q.y <= w.y && q.y >= -w.y) || (q.z <= w.z && q.z >= -w.z && v.z != 0.0);
}
let hide = i32(D(3)) == 1;
let crop = i32(D(4)) == 1;
if (boundaries) {
  if (crop) {
    v.x = 0.0;
    v.y = 0.0;
    v.z = 0.0;
    v.color = 0.0;
  }
  if (hide) { v.hide = 1.0; }
} else {
  if (hide) { v.hide = 0.0; }
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    VarDef {
        name: "post_point_symmetry_wf",
        params: &[
            ParamDef::new("centre_x", 0.25),
            ParamDef::new("centre_y", 0.5),
            ParamDef::int("order", 3.0, 1.0, 36.0),
            ParamDef::new("colorshift", 0.0),
        ],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            // d = [centre_x, centre_y, order, colorshift]
            let dx = (v.x - d[0]) * amount;
            let dy = (v.y - d[1]) * amount;
            let idx = ctx.random_int(d[2] as usize);
            // Java sums the angle step idx times.
            let da = M_2PI / d[2];
            let mut angle = 0.0;
            for _ in 0..idx {
                angle += da;
            }
            let (sina, cosa) = (angle.sin(), angle.cos());
            v.x = d[0] + dx * cosa + dy * sina;
            v.y = d[1] + dy * cosa - dx * sina;
            v.color = fmod(v.color + idx as f64 * d[3], 1.0);
        },
        prepare: Some(|p, _| vec![p[0], p[1], limit_value(ftoi(p[2]), 1, i32::MAX) as f64, p[3]]),
        wgsl: "let dx = (v.x - D(0)) * AMOUNT;
let dy = (v.y - D(1)) * AMOUNT;
let idx = floor(rnd() * D(2));
let angle = idx * (TWO_PI / D(2));
v.x = D(0) + dx * cos(angle) + dy * sin(angle);
v.y = D(1) + dy * cos(angle) - dx * sin(angle);
v.color = fmod(v.color + idx * D(3), 1.0);",
    },
    VarDef {
        name: "post_prepost_affine",
        params: PREPOST_AFFINE_PARAMS,
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            // The forward transform of prepost_affine (the GPU code of
            // InternalPrePostAffinePostWFFunc; its Java CPU transform is empty).
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
        prepare: Some(|p, _| prepost_affine_prepare(p)),
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
        name: "post_rblur",
        params: &[ParamDef::new("strength", 1.0), ParamDef::new("offset", 1.0), ParamDef::new("center_x", 0.0), ParamDef::new("center_y", 1.0)],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            let r = java_max((sqr(v.x - d[2]) + sqr(v.y - d[3])).sqrt() - d[1], 0.0) * (2.0 * d[0]);
            v.x = amount * (v.x + (ctx.random() - 0.5) * r);
            v.y = amount * (v.y + (ctx.random() - 0.5) * r);
        },
        prepare: None,
        wgsl: "let r = max(length(vec2<f32>(v.x - D(2), v.y - D(3))) - D(1), 0.0) * (2.0 * D(0));
v.x = AMOUNT * (v.x + (rnd() - 0.5) * r);
v.y = AMOUNT * (v.y + (rnd() - 0.5) * r);",
    },
    VarDef {
        name: "post_rotate_y",
        params: &[],
        kind: VarKind::Post,
        is_3d: true,
        apply: |_, _, _a, v, amount| {
            let (sina, cosa) = (amount * M_PI * 0.5).sin_cos();
            let x = cosa * v.x - sina * v.z;
            v.z = sina * v.x + cosa * v.z;
            v.x = x;
        },
        prepare: None,
        wgsl: "let sina = sin(AMOUNT * PI * 0.5);
let cosa = cos(AMOUNT * PI * 0.5);
let x = cosa * v.x - sina * v.z;
v.z = sina * v.x + cosa * v.z;
v.x = x;",
    },
    VarDef {
        name: "post_rotate_z",
        params: &[],
        kind: VarKind::Post,
        is_3d: false,
        apply: |_, _, _a, v, amount| {
            let (sina, cosa) = (amount * M_PI * 0.5).sin_cos();
            let y = cosa * v.y - sina * v.x;
            v.x = sina * v.y + cosa * v.x;
            v.y = y;
        },
        prepare: None,
        wgsl: "let sina = sin(AMOUNT * PI * 0.5);
let cosa = cos(AMOUNT * PI * 0.5);
let y = cosa * v.y - sina * v.x;
v.x = sina * v.y + cosa * v.x;
v.y = y;",
    },
    VarDef {
        name: "post_smartcrop",
        params: &[
            ParamDef::new("power", 4.0),
            ParamDef::new("radius", 1.0),
            ParamDef::new("roundstr", 0.0),
            ParamDef::new("roundwidth", 1.0),
            ParamDef::new("distortion", 1.0),
            ParamDef::new("edge", 0.0),
            ParamDef::new("scatter", 0.0),
            ParamDef::new("offset", 0.0),
            ParamDef::int("cropmode", 1.0, 0.0, 2.0),
            ParamDef::int("static", 2.0, -1.0, 2.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [power, radius, roundstr, roundwidth, distortion, edge, scatter, offset, cropmode, static,
            //      mode 10, radial 11, workradius 12, workpower 13, alpha 14, roundcoeff 15]
            let (cropmode, stat) = (d[8] as i32, d[9] as i32);
            let (xi, yi, zi) = if stat == 2 { (v.x, v.y, v.z) } else { (a.x, a.y, a.z) };
            let ang = yi.atan2(xi);
            let rad = (xi * xi + yi * yi).sqrt();
            let mode = d[10] != 0.0;
            let (scatter, offset) = (d[6], d[7]);
            let assign = stat == 2 || stat == -1;
            // The last uncropped point lives across calls (per render thread here).
            let restore = |ctx: &mut Ctx, v: &mut Pt| {
                let [sx, sy] = *state_slot(ctx, d, 0, [0.0; 2]);
                let [sz, sc] = *state_slot(ctx, d, 1, [0.0; 2]);
                if assign {
                    (v.x, v.y, v.z) = (sx, sy, sz);
                } else {
                    v.x += sx;
                    v.y += sy;
                    v.z += sz;
                }
                v.color = sc;
            };
            let place = |v: &mut Pt, r: f64, an: f64| {
                let (s, c) = an.sin_cos();
                if assign {
                    (v.x, v.y, v.z) = (amount * r * c, amount * r * s, amount * zi);
                } else {
                    v.x += amount * r * c;
                    v.y += amount * r * s;
                    v.z += amount * zi;
                }
            };
            if d[11] != 0.0 {
                let edge = d[5] * (ctx.random() - 0.5);
                let mut xang = ang / M_2PI + 1.0 + edge;
                xang = (xang - (xang as i32) as f64) * M_2PI;
                if (xang > d[13]) == mode {
                    if cropmode == 2 {
                        restore(ctx, v);
                    } else {
                        let an = if (ctx.random() * 2.0) as i32 > 0 {
                            d[13] + (ctx.random() * scatter + offset + edge) * M_PI
                        } else {
                            -(ctx.random() * scatter + offset + edge) * M_PI
                        };
                        place(v, rad, an);
                    }
                }
            } else {
                let mut coeff = 1.0;
                if d[4] != 0.0 {
                    let mut xang = (ang + M_PI) / d[14];
                    xang -= (xang as i32) as f64;
                    xang = if xang < 0.5 { xang } else { 1.0 - xang };
                    coeff = 1.0 / (xang * d[14]).cos();
                    if d[2] != 0.0 {
                        let w = (if d[3] != 1.0 { ((xang * 2.0).ln() * d[3]).exp() } else { xang * 2.0 }) * d[15];
                        coeff = ((1.0 - w) * coeff + w).abs();
                    }
                    if d[4] != 1.0 {
                        coeff = (coeff.ln() * d[4]).exp();
                    }
                }
                let xr = coeff * if d[5] != 0.0 { d[12] + d[5] * (ctx.random() - 0.5) } else { d[12] };
                if (rad > xr) == mode {
                    if cropmode == 2 {
                        restore(ctx, v);
                    } else {
                        let rdc = (if cropmode > 0 { 1.0 } else { 0.0 }) * xr + coeff * (ctx.random() * scatter + offset);
                        place(v, rdc, ang);
                    }
                } else {
                    let (sx, sy, sz) = (amount * xi, amount * yi, amount * zi);
                    *state_slot(ctx, d, 0, [0.0; 2]) = [sx, sy];
                    let slot = state_slot(ctx, d, 1, [0.0; 2]);
                    slot[0] = sz;
                    if cropmode == 2 {
                        slot[1] = v.color;
                    }
                    if stat > 0 {
                        (v.x, v.y, v.z) = (sx, sy, sz);
                    } else {
                        v.x += sx;
                        v.y += sy;
                        v.z += sz;
                    }
                }
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[8] = limit_value(ftoi(p[8]), 0, 2) as f64;
            d[9] = limit_value(ftoi(p[9]), -1, 2) as f64;
            let mode = (p[0] > 0.0) == (p[1] > 0.0);
            let workradius = p[1].abs();
            let mut workpower = p[0].abs();
            let (radial, alpha, roundcoeff) = if workpower < 2.0 {
                workpower *= M_PI;
                (true, 0.0, 0.0)
            } else {
                let alpha = M_2PI / workpower;
                (false, alpha, p[2] / (alpha / 2.0).sin() / workpower * 2.0)
            };
            d.extend([if mode { 1.0 } else { 0.0 }, if radial { 1.0 } else { 0.0 }, workradius, workpower, alpha, roundcoeff]);
            d
        }),
        // GPU: no memory of the last uncropped point (cropmode 2 uses the origin and color 0).
        wgsl: "let cropmode = i32(D(8));
let stat = i32(D(9));
var pin = vec3<f32>(a.x, a.y, a.z);
if (stat == 2) { pin = vec3<f32>(v.x, v.y, v.z); }
let ang = atan2(pin.y, pin.x);
let rad = length(pin.xy);
let mode = D(10) != 0.0;
let assign = stat == 2 || stat == -1;
var crop = false;
var rr = rad;
var an = ang;
if (D(11) != 0.0) {
  let edge = D(5) * (rnd() - 0.5);
  var xang = ang / TWO_PI + 1.0 + edge;
  xang = (xang - trunc(xang)) * TWO_PI;
  if ((xang > D(13)) == mode) {
    crop = true;
    if (cropmode != 2) {
      if (i32(rnd() * 2.0) > 0) {
        an = D(13) + (rnd() * D(6) + D(7) + edge) * PI;
      } else {
        an = -(rnd() * D(6) + D(7) + edge) * PI;
      }
    }
  }
} else {
  var coeff = 1.0;
  if (D(4) != 0.0) {
    var xang = (ang + PI) / D(14);
    xang = xang - trunc(xang);
    if (xang >= 0.5) { xang = 1.0 - xang; }
    coeff = 1.0 / cos(xang * D(14));
    if (D(2) != 0.0) {
      var w = xang * 2.0;
      if (D(3) != 1.0) { w = exp(log(xang * 2.0) * D(3)); }
      w *= D(15);
      coeff = abs((1.0 - w) * coeff + w);
    }
    if (D(4) != 1.0) { coeff = exp(log(coeff) * D(4)); }
  }
  var xr = coeff * D(12);
  if (D(5) != 0.0) { xr = coeff * (D(12) + D(5) * (rnd() - 0.5)); }
  if ((rad > xr) == mode) {
    crop = true;
    if (cropmode != 2) {
      var cm = 0.0;
      if (cropmode > 0) { cm = 1.0; }
      rr = cm * xr + coeff * (rnd() * D(6) + D(7));
    }
  } else {
    let sp = AMOUNT * pin;
    if (stat > 0) {
      v.x = sp.x;
      v.y = sp.y;
      v.z = sp.z;
    } else {
      v.x += sp.x;
      v.y += sp.y;
      v.z += sp.z;
    }
  }
}
if (crop) {
  if (cropmode == 2) {
    if (assign) {
      v.x = 0.0;
      v.y = 0.0;
      v.z = 0.0;
    }
    v.color = 0.0;
  } else if (assign) {
    v.x = AMOUNT * rr * cos(an);
    v.y = AMOUNT * rr * sin(an);
    v.z = AMOUNT * pin.z;
  } else {
    v.x += AMOUNT * rr * cos(an);
    v.y += AMOUNT * rr * sin(an);
    v.z += AMOUNT * pin.z;
  }
}",
    },
    VarDef {
        name: "post_spherical",
        params: &[],
        kind: VarKind::Post,
        is_3d: false,
        apply: |_, _, _a, v, amount| {
            let r = amount / (v.x * v.x + v.y * v.y + SMALL_EPSILON);
            v.x *= r;
            v.y *= r;
        },
        prepare: None,
        wgsl: "let r = AMOUNT / (v.x * v.x + v.y * v.y + EPS);
v.x = v.x * r;
v.y = v.y * r;",
    },
    VarDef {
        name: "post_spin_z",
        params: &[],
        kind: VarKind::Post,
        is_3d: false,
        apply: |_, _, _a, v, amount| {
            let (s, c) = (amount * M_PI_2).sin_cos();
            let y = c * v.y - s * v.x;
            v.x = s * v.y + c * v.x;
            v.y = y;
        },
        prepare: None,
        wgsl: "let s = sin(AMOUNT * HALF_PI);
let c = cos(AMOUNT * HALF_PI);
let y = c * v.y - s * v.x;
v.x = s * v.y + c * v.x;
v.y = y;",
    },
    VarDef {
        name: "post_trig",
        params: &[
            ParamDef::new("reciprocalpow", 1.0),
            ParamDef::new("log_dividepow", 0.0),
            ParamDef::new("sqrtpow", 0.0),
            ParamDef::new("exppow", 0.0),
            ParamDef::new("logpow", 0.0),
            ParamDef::new("asinhpow", 0.0),
            ParamDef::new("acoshpow", 0.0),
            ParamDef::new("atanhpow", 0.0),
            ParamDef::new("asinpow", 0.0),
            ParamDef::new("acospow", 0.0),
            ParamDef::new("atanpow", 0.0),
            ParamDef::new("sinpow", 0.0),
            ParamDef::new("cospow", 0.0),
            ParamDef::new("tanpow", 0.0),
            ParamDef::new("sinhpow", 0.0),
            ParamDef::new("coshpow", 0.0),
            ParamDef::new("tanhpow", 0.0),
        ],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [reciprocal, log_divide, sqrt, exp, log, asinh, acosh, atanh, asin, acos, atan, sin, cos,
            //      tan, sinh, cosh, tanh] weights
            let (mut xr, mut yr) = (v.x, v.y);
            let (mut xx, mut yy) = (0.0, 0.0);
            let f = |xr: f64, yr: f64, op: fn(&mut Complex), scale: f64| {
                let mut z = Complex::new(xr, yr);
                op(&mut z);
                z.scale(scale);
                z
            };
            if d[0] != 0.0 {
                let z = f(xr, yr, Complex::recip, amount);
                (xr, yr) = (d[0] * z.re, d[0] * z.im);
            }
            if d[1] != 0.0 {
                let mut z = Complex::new(xr, yr);
                let mut z2 = Complex::new(xr, yr);
                z2.dec();
                z.inc();
                z.div(&z2);
                z.log();
                z.scale(amount * M_2_PI);
                (xr, yr) = (d[1] * z.re, d[1] * z.im);
            }
            if d[2] != 0.0 {
                let z = f(xr, yr, Complex::sqrt, amount);
                (xr, yr) = if ctx.random() < 0.5 { (d[2] * z.re, d[2] * z.im) } else { (d[2] * -z.re, d[2] * -z.im) };
            }
            if d[3] != 0.0 {
                let z = f(xr, yr, Complex::exp, amount);
                (xr, yr) = (d[3] * z.re, d[3] * z.im);
            }
            if d[4] != 0.0 {
                let z = f(xr, yr, Complex::log, amount);
                (xr, yr) = (d[4] * z.re, d[4] * z.im);
            }
            let ops: [(usize, ComplexOp, f64); 12] = [
                (5, Complex::asinh, M_2_PI),
                (6, Complex::acosh, M_2_PI),
                (7, Complex::atanh, M_2_PI),
                (8, Complex::asin, M_2_PI),
                (9, Complex::acos, M_2_PI),
                (10, Complex::atan, M_2_PI),
                (11, Complex::sin, 1.0),
                (12, Complex::cos, 1.0),
                (13, |z| {
                    let mut z2 = Complex::copy_of(z);
                    z.sin();
                    z2.cos();
                    z.div(&z2);
                }, 1.0),
                (14, Complex::sinh, 1.0),
                (15, Complex::cosh, 1.0),
                (16, |z| {
                    let mut z2 = Complex::copy_of(z);
                    z.sinh();
                    z2.cosh();
                    z.div(&z2);
                }, 1.0),
            ];
            for (k, op, s) in ops {
                if d[k] != 0.0 {
                    let z = f(xr, yr, op, amount * s);
                    if k == 6 && ctx.random() >= 0.5 {
                        xx += d[k] * -z.re;
                        yy += d[k] * -z.im;
                    } else {
                        xx += d[k] * z.re;
                        yy += d[k] * z.im;
                    }
                }
            }
            if d[5] + d[6] + d[7] + d[11] + d[12] + d[13] + d[14] + d[15] + d[16] + d[8] + d[9] + d[10] == 0.0 {
                v.x = xr;
                v.y = yr;
            } else {
                v.x = xx;
                v.y = yy;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "var zr = vec2<f32>(v.x, v.y);
var acc = vec2<f32>(0.0);
let sc = AMOUNT * 2.0 / PI;
if (D(0) != 0.0) { zr = D(0) * cx_scale(cx_recip(zr), AMOUNT); }
if (D(1) != 0.0) { zr = D(1) * cx_scale(cx_log(cx_div(cx_inc(zr), cx_dec(zr))), sc); }
if (D(2) != 0.0) {
  var z = cx_scale(cx_sqrt(zr), AMOUNT);
  if (!(rnd() < 0.5)) { z = -z; }
  zr = D(2) * z;
}
if (D(3) != 0.0) { zr = D(3) * cx_scale(cx_exp(zr), AMOUNT); }
if (D(4) != 0.0) { zr = D(4) * cx_scale(cx_log(zr), AMOUNT); }
if (D(5) != 0.0) { acc += D(5) * cx_scale(cx_asinh(zr), sc); }
if (D(6) != 0.0) {
  var z = cx_scale(cx_acosh(zr), sc);
  if (!(rnd() < 0.5)) { z = -z; }
  acc += D(6) * z;
}
if (D(7) != 0.0) { acc += D(7) * cx_scale(cx_atanh(zr), sc); }
if (D(8) != 0.0) { acc += D(8) * cx_scale(cx_asin(zr), sc); }
if (D(9) != 0.0) { acc += D(9) * cx_scale(cx_acos(zr), sc); }
if (D(10) != 0.0) { acc += D(10) * cx_scale(cx_atan(zr), sc); }
if (D(11) != 0.0) { acc += D(11) * cx_scale(cx_sin(zr), AMOUNT); }
if (D(12) != 0.0) { acc += D(12) * cx_scale(cx_cos(zr), AMOUNT); }
if (D(13) != 0.0) { acc += D(13) * cx_scale(cx_div(cx_sin(zr), cx_cos(zr)), AMOUNT); }
if (D(14) != 0.0) { acc += D(14) * cx_scale(cx_sinh(zr), AMOUNT); }
if (D(15) != 0.0) { acc += D(15) * cx_scale(cx_cosh(zr), AMOUNT); }
if (D(16) != 0.0) { acc += D(16) * cx_scale(cx_div(cx_sinh(zr), cx_cosh(zr)), AMOUNT); }
if (D(5) + D(6) + D(7) + D(11) + D(12) + D(13) + D(14) + D(15) + D(16) + D(8) + D(9) + D(10) == 0.0) {
  v.x = zr.x;
  v.y = zr.y;
} else {
  v.x = acc.x;
  v.y = acc.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "post_zscale_wf",
        params: &[ParamDef::new("ztranslate", 0.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, _a, v, amount| {
            v.z = amount * v.z + d[0];
        },
        prepare: None,
        wgsl: "v.z = AMOUNT * v.z + D(0);",
    },
    VarDef {
        name: "post_ztranslate_wf",
        params: &[],
        kind: VarKind::Post,
        is_3d: true,
        apply: |_, _, _a, v, amount| {
            v.z += amount;
        },
        prepare: None,
        wgsl: "v.z += AMOUNT;",
    },
];
