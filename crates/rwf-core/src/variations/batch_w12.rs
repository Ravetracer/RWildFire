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

/// Parameters of `QuaternionFunc`: groups `<f>qpow, <f>qx1, <f>qx2, <f>qy1, <f>qy2, <f>qz1, <f>qz2`
/// (estiq without x2, logq with pow and base only).
const QUATERNION_PARAMS: &[ParamDef] = &{
    const NAMES: [&str; 92] = [
        "cosqpow", "cosqx1", "cosqx2", "cosqy1", "cosqy2", "cosqz1", "cosqz2", "coshqpow", "coshqx1", "coshqx2", "coshqy1", "coshqy2", "coshqz1",
        "coshqz2", "cotqpow", "cotqx1", "cotqx2", "cotqy1", "cotqy2", "cotqz1", "cotqz2", "cothqpow", "cothqx1", "cothqx2", "cothqy1", "cothqy2",
        "cothqz1", "cothqz2", "cscqpow", "cscqx1", "cscqx2", "cscqy1", "cscqy2", "cscqz1", "cscqz2", "cschqpow", "cschqx1", "cschqx2", "cschqy1",
        "cschqy2", "cschqz1", "cschqz2", "estiqpow", "estiqx1", "estiqy1", "estiqy2", "estiqz1", "estiqz2", "logqpow", "logqbase", "secqpow",
        "secqx1", "secqx2", "secqy1", "secqy2", "secqz1", "secqz2", "sechqpow", "sechqx1", "sechqx2", "sechqy1", "sechqy2", "sechqz1", "sechqz2",
        "sinqpow", "sinqx1", "sinqx2", "sinqy1", "sinqy2", "sinqz1", "sinqz2", "sinhqpow", "sinhqx1", "sinhqx2", "sinhqy1", "sinhqy2", "sinhqz1",
        "sinhqz2", "tanqpow", "tanqx1", "tanqx2", "tanqy1", "tanqy2", "tanqz1", "tanqz2", "tanhqpow", "tanhqx1", "tanhqx2", "tanhqy1", "tanhqy2",
        "tanhqz1", "tanhqz2",
    ];
    let mut p = [ParamDef::new("", 1.0); 92];
    let mut i = 0;
    while i < 92 {
        // the "pow" weights are 0 except cosqpow; logqbase is e
        let pow = i == 0 || i == 7 || i == 14 || i == 21 || i == 28 || i == 35 || i == 42 || i == 48 || i == 50 || i == 57 || i == 64 || i == 71 || i == 78 || i == 85;
        p[i] = ParamDef::new(NAMES[i], if i == 49 { M_E } else if pow && i != 0 { 0.0 } else { 1.0 });
        i += 1;
    }
    p
};

/// `RecurrencePlotFunc`: signal value of function `id` at sample `i` (`a`, `b`, `k`, `n` = Density).
fn recurrenceplot_y(id: i32, i: i32, a: f64, b: f64, k: i32, n: f64) -> f64 {
    let fi = i as f64;
    let s = |t: f64| t.sin();
    match id {
        0 => 50.0 * s((i * 4) as f64 * M_PI / n) + 25.0 * s((i * 40) as f64 * M_PI / n),
        1 => 10.0 * s((i * 5) as f64 * M_PI / n) * 5.0 * s((i * 15) as f64 * M_PI / n),
        2 => 50.0 * ((i * 24) as f64 * M_PI / n + 5.0 * s((i * 6) as f64 * M_PI / n)).cos(),
        3 => 50.0 * s((i * 20) as f64 * M_PI / n) + 50.0 * s((i * 22) as f64 * M_PI / n + M_PI),
        4 => 50.0 * s((i * 20) as f64 * M_PI / n) + 10.0 * s((i * 22) as f64 * M_PI / n),
        5 => 50.0 * s(fi * a * M_PI / n + s(fi * a * M_PI / n)),
        6 => a / (1.0 + (-b * (fi - n / 2.0) / n).exp()),
        7 => {
            (400.0 / M_PI)
                * (s(fi * M_PI / (0.5 * n)) + s((i * 3) as f64 * M_PI / (0.5 * n)) / 3.0 + s((i * 5) as f64 * M_PI / (0.5 * n)) / 5.0
                    + s((i * 7) as f64 * M_PI / (0.5 * n)) / 7.0)
        }
        8 => {
            (400.0 / M_PI * M_PI)
                * (s(fi * M_PI / (0.5 * n)) - s((i * 3) as f64 * M_PI / (0.5 * n)) / 9.0 + s((i * 5) as f64 * M_PI / (0.5 * n)) / 25.0
                    - s((i * 7) as f64 * M_PI / (0.5 * n)) / 49.0)
        }
        9 => (a * fi * M_PI / n).tanh(),
        10 => (a * fi * M_PI / n).tan(),
        11 => {
            let mut sum = s(fi * M_PI / (0.5 * n));
            for m in 2..=8 {
                sum += s((i * m) as f64 * M_PI / (0.5 * n)) / m as f64;
            }
            (-200.0 / M_PI) * sum
        }
        12 => 50.0 * s((i * 40) as f64 * M_PI / n) + 50.0 * s((i * 44) as f64 * M_PI / n),
        13 => 50.0 * s((i * 40) as f64 * M_PI / n) + 50.0 * s((i * 44) as f64 * M_PI / n + M_PI),
        14 => 100.0 * s((i * 16) as f64 * M_PI / n) - 500.0 + fi * 0.25,
        15 => 100.0 * s((i * 16) as f64 * M_PI / n),
        16 => 1.0 / (fi * a * M_PI / n).cos(),
        17 => 50.0 * ((i * 2) as f64 * M_PI * 25.0 / n + 25.0 * s((i * 2) as f64 * M_PI * 0.50 / n)).cos(),
        18 => a * (-b * (2.0 * a * fi / n - a) * (2.0 * a * fi / n - a)).exp(),
        19 => 500.0 * s((i * 10) as f64 * M_PI / n) * (-5.0 * fi / n).exp(),
        20 => 500.0 * s((i * 20) as f64 * M_PI / n) * (-3.0 * fi / n).exp(),
        21 => 50.0 * ((i * 2) as f64 * M_PI / n).tan(),
        22 => {
            let x = (2.0 * fi) / n - 1.0;
            let p = |e: f64| x.powf(e);
            // Legendre polynomials: a * numerator / denominator
            let (num, den) = match b as i32 {
                0 => return a,
                1 => return a * x,
                2 => (3.0 * p(2.0) - 1.0, 2.0),
                3 => (5.0 * p(3.0) - 3.0 * x, 2.0),
                4 => (35.0 * p(4.0) - 30.0 * p(2.0) + 3.0, 8.0),
                5 => (63.0 * p(5.0) - 70.0 * p(3.0) + 15.0 * x, 8.0),
                6 => (231.0 * p(6.0) - 315.0 * p(4.0) + 105.0 * p(2.0) - 5.0, 16.0),
                7 => (429.0 * p(7.0) - 693.0 * p(5.0) + 315.0 * p(3.0) - 35.0 * x, 16.0),
                8 => (6435.0 * p(8.0) - 12012.0 * p(6.0) + 6930.0 * p(4.0) - 1260.0 * p(2.0) + 35.0, 128.0),
                9 => (12155.0 * p(9.0) - 25740.0 * p(7.0) + 18018.0 * p(5.0) - 4620.0 * p(3.0) + 315.0 * x, 128.0),
                10 => (46189.0 * p(10.0) - 109395.0 * p(8.0) + 90090.0 * p(6.0) - 30030.0 * p(4.0) + 3465.0 * p(2.0) - 63.0, 256.0),
                _ => return 0.0,
            };
            a * num / den
        }
        23 => {
            let mut sum = 0.0;
            for j in 1..=k {
                let o = 2.0 * j as f64 - 1.0;
                sum += s(fi * o * b * 2.0 * M_PI / n) / o;
            }
            sum * (4.0 * a / M_PI)
        }
        24 => {
            let mut sum = 0.0;
            for j in 1..=k {
                sum += s(fi * j as f64 * b * 2.0 * M_PI / n) / j as f64;
            }
            sum * (-2.0 * a / M_PI)
        }
        25 => {
            let mut sum = 0.0;
            for j in 1..=k {
                let j = j as f64;
                sum += (fi * (2.0 * j) * b * 2.0 * M_PI / n).cos() / ((2.0 * j - 1.0) * (2.0 * j + 1.0));
            }
            (4.0 * a / M_PI) * (0.5 - sum)
        }
        26 => {
            let mut sum = 0.0;
            for j in 1..=k {
                let j = j as f64;
                sum += (-1.0f64).powf(j) * (fi * (2.0 * j) * b * 1.0 * M_PI / n).cos() / (j * j);
            }
            (a * M_PI * M_PI / 3.0) + 4.0 * a * sum
        }
        27 => a * s(b * M_PI * (fi - 0.5 * n) / n) / (b * M_PI * (fi - 0.5 * n) / n),
        28 => a * (-b * (((fi - 0.5 * n) / n) * (fi - 0.5 * n) / n)).exp() * (2.0 * k as f64 * M_PI * fi / n).cos(),
        29 => {
            let x = (2.0 * fi) / n - 1.0;
            let p = |e: f64| x.powf(e);
            a * match k {
                0 => 1.0,
                1 => x,
                2 => 2.0 * p(2.0) - 1.0,
                3 => 4.0 * p(3.0) - 3.0 * x,
                4 => 8.0 * p(4.0) - 8.0 * p(2.0) + 1.0,
                5 => 16.0 * p(5.0) - 20.0 * p(3.0) + 5.0 * x,
                6 => 32.0 * p(6.0) - 48.0 * p(4.0) + 18.0 * p(2.0) - 1.0,
                7 => 64.0 * p(7.0) - 112.0 * p(5.0) + 56.0 * p(3.0) - 7.0 * x,
                8 => 128.0 * p(8.0) - 256.0 * p(6.0) + 160.0 * p(4.0) - 32.0 * p(2.0) + 1.0,
                9 => 256.0 * p(9.0) - 576.0 * p(7.0) + 432.0 * p(5.0) - 120.0 * p(3.0) + 9.0 * x,
                10 => 512.0 * p(10.0) - 1280.0 * p(8.0) + 1120.0 * p(6.0) - 400.0 * p(4.0) + 50.0 * p(2.0) - 1.0,
                11 => 1024.0 * p(11.0) - 2816.0 * p(9.0) + 2816.0 * p(7.0) - 1232.0 * p(5.0) + 220.0 * p(3.0) - 11.0 * x,
                12 => 2048.0 * p(12.0) - 6144.0 * p(10.0) + 6912.0 * p(8.0) - 3584.0 * p(6.0) + 840.0 * p(4.0) - 72.0 * p(2.0) + 1.0,
                _ => return 0.0,
            }
        }
        // Java keeps the previous values (initially 0) for unknown ids.
        _ => 0.0,
    }
}

/// `RhodoneaFunc.init`: `(k, cycles, cycles_to_close)` (first initialization, `cycles` = 0).
fn rhodonea_cycles(p: &[f64]) -> (f64, f64, f64) {
    // p = [knumer, kdenom, inner_mode, outer_mode, inner_spread, outer_spread, inner_spread_ratio,
    //      outer_spread_ratio, spread_split, fill, radial_offset, cycles, cycle_offset, metacycles,
    //      metacycle_expansion]
    let (mut kn, mut kd) = (p[0], p[1]);
    let k = kn / kd;
    let cycles_to_close = if k % 1.0 == 0.0 {
        if k % 2.0 == 0.0 || p[10] != 0.0 || p[4] != 0.0 || p[5] != 0.0 || p[9] != 0.0 { 1.0 } else { 0.5 }
    } else if kn % 1.0 == 0.0 && kd % 1.0 == 0.0 {
        let (mut x, mut y) = ((kn as i64).unsigned_abs(), (kd as i32 as i64).unsigned_abs());
        while y != 0 {
            (x, y) = (y, x % y);
        }
        let gcd = x as i32;
        if gcd != 1 {
            kn /= gcd as f64;
            kd /= gcd as f64;
        }
        if kn % 2.0 == 0.0 || kd % 2.0 == 0.0 { kd } else { kd / 2.0 }
    } else {
        // Java compares the not yet computed cycles (0) with 16.
        16.0
    };
    let cycles = if p[11] == 0.0 { cycles_to_close * p[13] } else { p[11] };
    (k, cycles, cycles_to_close)
}

/// `RsquaresFunc`: end points of segment `k` of the recursive squares of depth `level`
/// (squares in the drawing order of `rSquares.draw`, four sides each).
fn rsquares_segment(level: i32, k: i64) -> [f64; 4] {
    let (mut x, mut y, mut size) = (0.5, 0.5, 1.0);
    let mut s = k / 4;
    let mut depth = level;
    while s > 0 {
        s -= 1;
        // number of squares below a child: (4^(depth-1) - 1) / 3
        let sub = ((1i64 << (2 * (depth - 1))) - 1) / 3;
        let child = s / sub;
        s %= sub;
        let (cx, cy) = match child {
            0 => (x - size / 2.0, y - size / 2.0),
            1 => (x - size / 2.0, y + size / 2.0),
            2 => (x + size / 2.0, y - size / 2.0),
            _ => (x + size / 2.0, y + size / 2.0),
        };
        (x, y, size) = (cx, cy, size / 2.2);
        depth -= 1;
    }
    let (x0, x1, y0, y1) = (x - size / 2.0, x + size / 2.0, y - size / 2.0, y + size / 2.0);
    match k % 4 {
        0 => [x0, y0, x0, y1],
        1 => [x0, y1, x1, y1],
        2 => [x1, y1, x1, y0],
        _ => [x1, y0, x0, y0],
    }
}

/// `ScramblyFunc.mx_randflip`: pseudo random shuffle of `mx[idxmin..idxmax]`.
fn scrambly_randflip(idxmin: i32, idxmax: i32, seed: i32, mx: &mut [i32]) {
    let mut prn: i32 = 1;
    let mut j = idxmin;
    loop {
        prn = prn.wrapping_mul(1103515245).wrapping_add(12345);
        prn = (prn as u32 & 0xFFFF0000) as i32 | ((prn << 8) & 0xFF00) | ((prn >> 8) & 0x00FF);
        prn = if prn & 4 != 0 { prn.wrapping_sub(seed) } else { prn ^ seed };
        prn = if prn < 0 { prn.wrapping_neg() } else { prn };
        let mut ridx = 1 + j;
        if idxmax > ridx {
            ridx += prn % (idxmax - ridx);
        } else {
            break;
        }
        // Java would fail with an index error for prn = Integer.MIN_VALUE
        if ridx < 0 || j < 0 || ridx as usize >= mx.len() || j as usize >= mx.len() {
            break;
        }
        mx.swap(ridx as usize, j as usize);
        j += 1;
    }
}

/// `SnowflakeWFFunc.getPoints`: the snowflake points `[x, y, intensity]` (cached by parameters,
/// like Java). `p` = buffer_size, max_iter, bg_freeze_level, fg_freeze_speed, diffusion_speed,
/// diffusion_asymmetry, rnd_bg_noise, threshold, seed, scale, jitter.
fn snowflake_points(p: &[f64]) -> std::sync::Arc<Vec<[f64; 3]>> {
    use std::sync::{Arc, Mutex, OnceLock};
    type Cache = std::collections::HashMap<Vec<u64>, Arc<Vec<[f64; 3]>>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let key: Vec<u64> = p[..11].iter().map(|v| v.to_bits()).collect();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Some(r) = cache.lock().unwrap().get(&key) {
        return r.clone();
    }
    let stretch = 1.5 / 1.7321;
    let (w, max_iter, seed) = (p[0] as i32, p[1] as i32, p[8] as i32);
    let (bg, fg, diff, asym, noise, threshold, scale) = (p[2], p[3], p[4], p[5], p[6], p[7], p[9]);
    let jitter = java_max(java_min(1.0, p[10]), 0.0);
    // Snowflake simulation on a hexagonal grid
    let mut pts = Vec::new();
    if w > 0 {
        let h = w + w - 1;
        let n = (w * h + (w - 1) * (h - 1)) as usize;
        let offs = [-w - (w - 1), -(w - 1), w, w + (w - 1), w - 1, -w];
        let nb = |i: usize, j: usize| -> Option<usize> {
            let k = i as i64 + offs[j] as i64;
            if k >= 0 && (k as usize) < n { Some(k as usize) } else { None }
        };
        let mut rnd = Marsaglia::new(seed as i64);
        let mut flake: Vec<f64> = (0..n).map(|_| bg + (noise - 2.0 * noise * rnd.random())).collect();
        flake[(n as f64 / 2.0).floor() as usize] = 1.0;
        let (t_freeze, t_diff) = (fg / 1000.0, diff / 1000.0 + 1.0);
        let cw = 0.5 * asym / t_diff;
        let nw = (1.0 * t_diff - cw) / 6.0;
        let mut tmp = vec![0.0; n];
        let mut recv = vec![0.0; n];
        let mut nonrecv = vec![0.0; n];
        for _ in 0..max_iter {
            tmp.copy_from_slice(&flake);
            recv.fill(0.0);
            for i in 0..n {
                if flake[i] >= 1.0 {
                    recv[i] = flake[i];
                    for j in 0..6 {
                        if let Some(k) = nb(i, j) {
                            tmp[k] = 0.0;
                            recv[k] = if flake[k] > 0.0 { flake[k] + t_freeze } else { flake[k] };
                        }
                    }
                }
            }
            for i in 0..n {
                nonrecv[i] = tmp[i] * cw;
                for j in 0..6 {
                    if let Some(k) = nb(i, j) {
                        nonrecv[i] += tmp[k] * nw;
                    }
                }
            }
            for i in 0..n {
                flake[i] = nonrecv[i] + recv[i];
            }
        }
        // renderSnowflake (float canvas)
        let cw_px = (2.0 * w as f64 * stretch).floor() as i32;
        let mut canvas = vec![vec![0.0f32; cw_px.max(0) as usize]; h as usize];
        let mut render = |value: f64, x: i32, y: i32| {
            if value > threshold + EPSILON && y >= 0 && y < h && x >= 0 && x < cw_px {
                canvas[y as usize][x as usize] = (value - threshold) as f32;
            }
        };
        for i in 0..h {
            for j in 0..w {
                let base_idx = (w + w - 1) * i + j;
                render(flake[base_idx as usize], ftoi((2 * j) as f64 * stretch), i);
                let (i1, i2) = (base_idx - w + 1, base_idx + w);
                if i1 >= 0 && (i1 as usize) < n && i2 >= 0 && (i2 as usize) < n {
                    render((flake[i1 as usize] + flake[i2 as usize]) * 0.5, ftoi((2 * j + 1) as f64 * stretch), i);
                }
            }
        }
        let mut rnd = Marsaglia::new(seed as i64);
        let rows = canvas.len() as f64;
        for (i, row) in canvas.iter().enumerate() {
            let cols = row.len() as f64;
            for (j, &val) in row.iter().enumerate() {
                let a_rnd = if jitter > EPSILON { rnd.random() } else { 0.0 };
                if val as f64 > EPSILON {
                    let mut x = (i as f64 - rows * 0.5) * scale / rows;
                    let mut y = (j as f64 - cols * 0.5) * scale / cols * stretch;
                    if jitter > EPSILON {
                        let (s, c) = (a_rnd * 2.0 * M_PI).sin_cos();
                        x += jitter * c;
                        y += jitter * s;
                    }
                    pts.push([x, y, val as f64]);
                }
            }
        }
    }
    let res = Arc::new(pts);
    cache.lock().unwrap().insert(key, res.clone());
    res
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
        name: "pow_block",
        // Java defaults are random (numerator 2..7, denominator 1..4).
        params: &[
            ParamDef::new("numerator", 3.0),
            ParamDef::new("denominator", 2.0),
            ParamDef::new("root", 1.0),
            ParamDef::new("correctn", 1.0),
            ParamDef::new("correctd", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [numerator, denominator, root, correctn, correctd, power, deneps]
            let theta = a.atan_yx();
            let r2 = a.sumsq().powf(d[5]) * amount;
            let ran = (theta * d[6] + (d[2] * M_2PI * (ctx.random() * d[1]).floor() * d[6])) * d[0];
            v.x += r2 * ran.cos();
            v.y += r2 * ran.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut power = p[1] * p[3] * 1.0 / (p[4].abs() + SMALL_EPSILON);
            if power.abs() <= SMALL_EPSILON {
                power = SMALL_EPSILON;
            }
            power = (p[0] * 0.5) / power;
            let mut deneps = p[1];
            if deneps.abs() <= SMALL_EPSILON {
                deneps = SMALL_EPSILON;
            }
            let mut d = p.to_vec();
            d.extend([power, 1.0 / deneps]);
            d
        }),
        wgsl: "let theta = atan2(a.y, a.x);
let r2 = pow(a.x * a.x + a.y * a.y, D(5)) * AMOUNT;
let ran = (theta * D(6) + (D(2) * TWO_PI * floor(rnd() * D(1)) * D(6))) * D(0);
v.x += r2 * cos(ran);
v.y += r2 * sin(ran);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "pre_affine3D",
        params: AFFINE3D_PARAMS,
        kind: VarKind::Pre,
        is_3d: true,
        apply: |d, _, a, _v, amount| {
            (a.x, a.y, a.z) = affine3d_xform(d, a.x, a.y, a.z, amount);
        },
        prepare: Some(|p, _| affine3d_prepare(p)),
        wgsl: concat!("let ax = a.x;\nlet ay = a.y;\nlet az = a.z;\n", affine3d_wgsl!(), "a.x = r.x;\na.y = r.y;\na.z = r.z;"),
    },
    VarDef {
        name: "pre_blur3D",
        params: &[],
        kind: VarKind::Pre,
        is_3d: true,
        apply: |d, ctx, a, _v, amount| {
            // d = [six initial random numbers, 0, 0]
            // Java keeps six random numbers (filled in init) and replaces one per call, the index
            // alternating 0, 1 ((n + 1) & 5); state slots 0..3 hold them, slot 3 the index (CPU,
            // per render thread).
            let (sina, cosa) = (ctx.random() * 2.0 * M_PI).sin_cos();
            let g: Vec<f64> = (0..3).flat_map(|k| *state_slot(ctx, d, k, [d[2 * k], d[2 * k + 1]])).collect();
            let r = amount * (g[0] + g[1] + g[2] + g[3] + g[4] + g[5] - 3.0);
            let n = state_slot(ctx, d, 3, [0.0; 2])[0] as usize;
            let rn = ctx.random();
            state_slot(ctx, d, n / 2, [0.0; 2])[n % 2] = rn;
            state_slot(ctx, d, 3, [0.0; 2])[0] = ((n + 1) & 5) as f64;
            let (sinb, cosb) = (ctx.random() * M_PI).sin_cos();
            a.x += r * sinb * cosa;
            a.y += r * sinb * sina;
            a.z += r * cosb;
        },
        // The random numbers of Java's init come from a fixed seed here.
        prepare: Some(|_, _| {
            let mut rng = crate::rng::Rng::new(1);
            let mut d: Vec<f64> = (0..6).map(|_| rng.random()).collect();
            d.extend([0.0, 0.0]);
            d
        }),
        // GPU: only the first two of the six numbers change in Java; they are fresh random
        // numbers here.
        wgsl: "let an = rnd() * TWO_PI;
let r = AMOUNT * (rnd() + rnd() + D(2) + D(3) + D(4) + D(5) - 3.0);
let bn = rnd() * PI;
a.x += r * sin(bn) * cos(an);
a.y += r * sin(bn) * sin(an);
a.z += r * cos(bn);",
    },
    VarDef {
        name: "pre_boarders2",
        params: &[ParamDef::new("c", 0.4), ParamDef::new("left", 0.65), ParamDef::new("right", 0.35)],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |d, ctx, a, _v, amount| {
            // d = [c, left, right, _c, _cl, _cr]
            let (c, cl, cr) = (d[3], d[4], d[5]);
            let (rx, ry) = (rint(a.x), rint(a.y));
            let (ox, oy) = (a.x - rx, a.y - ry);
            if ctx.random() >= cr {
                a.x = amount * (ox * c + rx);
                a.y = amount * (oy * c + ry);
            } else if ox.abs() >= oy.abs() {
                if ox >= 0.0 {
                    a.x = amount * (ox * c + rx + cl);
                    a.y = amount * (oy * c + ry + cl * oy / ox);
                } else {
                    a.x = amount * (ox * c + rx - cl);
                    a.y = amount * (oy * c + ry - cl * oy / ox);
                }
            } else if oy >= 0.0 {
                a.y = amount * (oy * c + ry + cl);
                a.x = amount * (ox * c + rx + ox / oy * cl);
            } else {
                a.y = amount * (oy * c + ry - cl);
                a.x = amount * (ox * c + rx - ox / oy * cl);
            }
        },
        prepare: Some(|p, _| {
            let nz = |v: f64| if v == 0.0 { EPSILON } else { v };
            let (c, cl, cr) = (nz(p[0].abs()), nz(p[1].abs()), nz(p[2].abs()));
            vec![p[0], p[1], p[2], c, c * cl, c + c * cr]
        }),
        wgsl: "let c = D(3);
let cl = D(4);
let rx = round(a.x);
let ry = round(a.y);
let ox = a.x - rx;
let oy = a.y - ry;
if (rnd() >= D(5)) {
  a.x = AMOUNT * (ox * c + rx);
  a.y = AMOUNT * (oy * c + ry);
} else if (abs(ox) >= abs(oy)) {
  let s = select(-1.0, 1.0, ox >= 0.0);
  a.x = AMOUNT * (ox * c + rx + s * cl);
  a.y = AMOUNT * (oy * c + ry + s * cl * oy / ox);
} else {
  let s = select(-1.0, 1.0, oy >= 0.0);
  a.y = AMOUNT * (oy * c + ry + s * cl);
  a.x = AMOUNT * (ox * c + rx + s * ox / oy * cl);
}",
    },
    VarDef {
        name: "pre_bwraps2",
        params: &[
            ParamDef::new("cellsize", 1.0),
            ParamDef::new("space", 0.0),
            ParamDef::new("gain", 2.0),
            ParamDef::new("inner_twist", 0.0),
            ParamDef::new("outer_twist", 0.0),
        ],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |d, _, a, _v, amount| {
            // d = [cellsize, space, gain, inner_twist, outer_twist, g2, r2, rfactor]
            let (vx, vy) = (a.x, a.y);
            let cs = d[0];
            if cs.abs() < SMALL_EPSILON {
                a.x = amount * vx;
                a.y = amount * vy;
                return;
            }
            let cx = ((vx / cs).floor() + 0.5) * cs;
            let cy = ((vy / cs).floor() + 0.5) * cs;
            let (mut lx, mut ly) = (vx - cx, vy - cy);
            if lx * lx + ly * ly > d[6] {
                a.x = amount * vx;
                a.y = amount * vy;
                return;
            }
            lx *= d[5];
            ly *= d[5];
            let r = d[7] / ((lx * lx + ly * ly) / 4.0 + 1.0);
            lx *= r;
            ly *= r;
            let r = (lx * lx + ly * ly) / d[6];
            let (s, c) = (d[3] * (1.0 - r) + d[4] * r).sin_cos();
            a.x = amount * (cx + c * lx + s * ly);
            a.y = amount * (cy - s * lx + c * ly);
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
var cc = vec2<f32>(a.x, a.y);
if (abs(cs) >= EPS) {
  cc = (floor(vec2<f32>(a.x, a.y) / cs) + 0.5) * cs;
  l = vec2<f32>(a.x, a.y) - cc;
  if (dot(l, l) > D(6)) {
    cc = vec2<f32>(a.x, a.y);
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
a.x = AMOUNT * (cc.x + l.x);
a.y = AMOUNT * (cc.y + l.y);",
    },
    VarDef {
        name: "pre_circlecrop",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::new("x", 0.0),
            ParamDef::new("y", 0.0),
            ParamDef::range("scatter_area", 0.0, -1.0, 1.0),
            ParamDef::int("zero", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |d, ctx, a, _v, amount| {
            // d = [radius, x, y, scatter_area, zero, cA]
            let (x0, y0, cr) = (d[1], d[2], d[0]);
            a.x -= x0;
            a.y -= y0;
            let rad = (a.x * a.x + a.y * a.y).sqrt();
            let ang = a.y.atan2(a.x);
            let rdc = cr + ctx.random() * 0.5 * d[5];
            let esc = rad > cr;
            let cr0 = d[4] as i32 == 1;
            let (s, c) = ang.sin_cos();
            a.do_hide = false;
            if cr0 && esc {
                a.x = 0.0;
                a.y = 0.0;
                a.do_hide = true;
            } else if !cr0 && esc {
                a.x = amount * rdc * c + x0;
                a.y = amount * rdc * s + y0;
            } else {
                a.x = amount * a.x + x0;
                a.y = amount * a.y + y0;
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
let px = a.x - x0;
let py = a.y - y0;
let rad = sqrt(px * px + py * py);
let ang = atan2(py, px);
let rdc = cr + rnd() * 0.5 * D(5);
let esc = rad > cr;
let cr0 = i32(D(4)) == 1;
a.hide = 0.0;
if (cr0 && esc) {
  a.x = 0.0;
  a.y = 0.0;
  a.hide = 1.0;
} else if (!cr0 && esc) {
  a.x = AMOUNT * rdc * cos(ang) + x0;
  a.y = AMOUNT * rdc * sin(ang) + y0;
} else {
  a.x = AMOUNT * px + x0;
  a.y = AMOUNT * py + y0;
}",
    },
    VarDef {
        name: "pre_crop",
        params: &[
            ParamDef::new("left", -1.0),
            ParamDef::new("right", 1.0),
            ParamDef::new("top", -1.0),
            ParamDef::new("bottom", 1.0),
            ParamDef::range("scatter_area", 0.0, -1.0, 1.0),
            ParamDef::int("zero", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |d, ctx, a, _v, amount| {
            // d = [left, right, top, bottom, scatter_area, zero, xmin 6, xmax 7, ymin 8, ymax 9, w 10, h 11]
            let (mut x, mut y) = (a.x, a.y);
            if (x < d[6] || x > d[7] || y < d[8] || y > d[9]) && d[5] != 0.0 {
                a.x = 0.0;
                a.y = 0.0;
                a.do_hide = true;
                return;
            }
            a.do_hide = false;
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
            a.x = amount * x;
            a.y = amount * y;
        },
        prepare: Some(|p, _| {
            let scatter = limit_value(p[4], -1.0, 1.0);
            let zero = limit_value(ftoi(p[5]), 0, 1) as f64;
            let (xmin, xmax) = (java_min(p[0], p[1]), java_max(p[0], p[1]));
            let (ymin, ymax) = (java_min(p[2], p[3]), java_max(p[2], p[3]));
            vec![p[0], p[1], p[2], p[3], scatter, zero, xmin, xmax, ymin, ymax, (xmax - xmin) * 0.5 * scatter, (ymax - ymin) * 0.5 * scatter]
        }),
        wgsl: "var x = a.x;
var y = a.y;
if ((x < D(6) || x > D(7) || y < D(8) || y > D(9)) && D(5) != 0.0) {
  a.x = 0.0;
  a.y = 0.0;
  a.hide = 1.0;
  return PtPair(a, v);
}
a.hide = 0.0;
if (x < D(6)) { x = D(6) + rnd() * D(10); } else if (x > D(7)) { x = D(7) - rnd() * D(10); }
if (y < D(8)) { y = D(8) + rnd() * D(11); } else if (y > D(9)) { y = D(9) - rnd() * D(11); }
a.x = AMOUNT * x;
a.y = AMOUNT * y;",
    },
    VarDef {
        name: "pre_c_symmetry",
        params: &[ParamDef::new("p1", 3.0), ParamDef::new("p2", 3.0)],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |d, ctx, a, _v, amount| {
            let t = d[0] * a.y.atan2(a.x);
            let l = (a.x * a.x + a.y * a.y).sqrt().powf(d[1]);
            a.x = amount * (t.cos() * l);
            a.y = amount * (t.sin() * l);
            if ctx.preserve_z {
                a.z *= amount;
            }
        },
        prepare: None,
        wgsl: "let t = D(0) * atan2(a.y, a.x);
let l = pow(length(vec2<f32>(a.x, a.y)), D(1));
a.x = AMOUNT * (cos(t) * l);
a.y = AMOUNT * (sin(t) * l);
if (PRESERVE_Z) { a.z = AMOUNT * a.z; }",
    },
    VarDef {
        name: "pre_curl",
        params: &[ParamDef::new("c1", 0.0), ParamDef::new("c2", 0.0)],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |d, _, a, _v, amount| {
            let (x, y) = (a.x, a.y);
            let re = 1.0 + d[0] * x + d[1] * (x * x - y * y);
            let im = d[0] * y + 2.0 * d[1] * x * y;
            let r = amount / (re * re + im * im);
            a.x = (x * re + y * im) * r;
            a.y = (y * re - x * im) * r;
        },
        prepare: None,
        wgsl: "let x = a.x;
let y = a.y;
let re = 1.0 + D(0) * x + D(1) * (x * x - y * y);
let im = D(0) * y + 2.0 * D(1) * x * y;
let r = AMOUNT / (re * re + im * im);
a.x = (x * re + y * im) * r;
a.y = (y * re - x * im) * r;",
    },
    VarDef {
        name: "quadrupole",
        params: &[ParamDef::new("strength", 0.3), ParamDef::new("smooth", 0.1)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r2 = a.x * a.x + a.y * a.y;
            let qr2 = r2 + d[1];
            let theta = a.y.atan2(a.x);
            let fr = d[0] * (2.0 * theta).cos() / qr2;
            let ft = d[0] * (2.0 * theta).sin() / qr2;
            let nr = (r2 + 1.0e-10).sqrt() + fr;
            let nt = theta + ft;
            v.x += amount * nr * nt.cos();
            v.y += amount * nr * nt.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r2 = a.x * a.x + a.y * a.y;
let qr2 = r2 + D(1);
let theta = atan2(a.y, a.x);
let nr = sqrt(r2 + 1.0e-10) + D(0) * cos(2.0 * theta) / qr2;
let nt = theta + D(0) * sin(2.0 * theta) / qr2;
v.x += AMOUNT * nr * cos(nt);
v.y += AMOUNT * nr * sin(nt);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "quasicrystal",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("amp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (mut sx, mut sy) = (0.0, 0.0);
            for k in 0..5 {
                let (s, c) = (k as f64 * 1.2566370614359172).sin_cos();
                let wave = (d[0] * (a.x * c + a.y * s)).cos();
                sx += wave * c;
                sy += wave * s;
            }
            v.x += amount * (a.x + d[1] * sx * 0.2);
            v.y += amount * (a.y + d[1] * sy * 0.2);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "var s = vec2<f32>(0.0);
for (var k = 0; k < 5; k++) {
  let an = f32(k) * 1.2566370614359172;
  let cs = vec2<f32>(cos(an), sin(an));
  s += cos(D(0) * (a.x * cs.x + a.y * cs.y)) * cs;
}
v.x += AMOUNT * (a.x + D(1) * s.x * 0.2);
v.y += AMOUNT * (a.y + D(1) * s.y * 0.2);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "quilt",
        params: &[ParamDef::new("scale", 3.0), ParamDef::new("puff", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (cx, cy) = (a.x * d[0], a.y * d[0]);
            let (col, row) = (java_round(cx), java_round(cy));
            let (lx, ly) = (cx - col, cy - row);
            let bulge = d[1] * (1.0 - (lx * lx + ly * ly).sqrt() * 2.0);
            v.x += amount * (col + lx * (1.0 - bulge)) / d[0];
            v.y += amount * (row + ly * (1.0 - bulge)) / d[0];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let cx = a.x * D(0);
let cy = a.y * D(0);
let col = java_round(cx);
let row = java_round(cy);
let lx = cx - col;
let ly = cy - row;
let bulge = D(1) * (1.0 - sqrt(lx * lx + ly * ly) * 2.0);
v.x += AMOUNT * (col + lx * (1.0 - bulge)) / D(0);
v.y += AMOUNT * (row + ly * (1.0 - bulge)) / D(0);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "quintessence",
        params: &[ParamDef::new("scale", 1.0), ParamDef::new("power", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y + 1e-10).powf(d[1] * 0.5);
            let an = a.y.atan2(a.x) * 5.0;
            v.x += amount * r * an.cos() * d[0];
            v.y += amount * r * an.sin() * d[0];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = pow(a.x * a.x + a.y * a.y + 1e-10, D(1) * 0.5);
let an = atan2(a.y, a.x) * 5.0;
v.x += AMOUNT * r * cos(an) * D(0);
v.y += AMOUNT * r * sin(an) * D(0);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "quaternion",
        params: QUATERNION_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            // d: see QUATERNION_PARAMS (cosq 0, coshq 7, cotq 14, cothq 21, cscq 28, cschq 35, estiq 42,
            // logq 48, secq 50, sechq 57, sinq 64, sinhq 71, tanq 78, tanhq 85)
            let (ax, ay, az) = (a.x, a.y, a.z);
            let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
            let hyp = ay.hypot(az);
            let r2 = sqr(ax) + sqr(ay) + sqr(az);
            if d[0] != 0.0 {
                let p = &d[0..7];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((ax * p[1]).sin(), (ax * p[2]).cos(), (av * p[3]).sinh(), (av * p[4]).cosh());
                let cc = amount * s * sh / av * p[6];
                x += p[0] * amount * c * ch;
                y += p[0] * cc * ay;
                z += p[0] * cc * az;
            }
            if d[7] != 0.0 {
                let p = &d[7..14];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((av * p[3]).sin(), (av * p[4]).cos(), (ax * p[1]).sinh(), (ax * p[2]).cosh());
                let cc = amount * sh * s / av * p[6];
                x += p[0] * amount * ch * c;
                y += p[0] * cc * ay;
                z += p[0] * cc * az;
            }
            if d[14] != 0.0 {
                let p = &d[14..21];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((ax * p[1]).sin(), (ax * p[2]).cos(), (av * p[3]).sinh(), (av * p[4]).cosh());
                let sysz = sqr(ay) + sqr(az);
                let ni = amount / (sqr(ax) + sysz);
                let cc = c * sh / av * p[6];
                let b = -s * sh / av;
                let stcv = s * ch;
                let nstcv = -stcv;
                let ctcv = c * ch;
                x += p[0] * (stcv * ctcv + cc * b * sysz) * ni;
                y -= p[0] * (nstcv * b * ay + cc * ay * ctcv) * ni;
                z -= p[0] * (nstcv * b * az + cc * az * ctcv) * ni;
            }
            if d[21] != 0.0 {
                let p = &d[21..28];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((av * p[3]).sin(), (av * p[4]).cos(), (ax * p[1]).sinh(), (ax * p[2]).cosh());
                let sysz = sqr(ay) + sqr(az);
                let ni = amount / (sqr(ax) + sysz);
                let cc = ch * s / av * p[6];
                let b = sh * s / av;
                let stcv = sh * c;
                let nstcv = -stcv;
                let ctcv = ch * c;
                x += p[0] * (stcv * ctcv + cc * b * sysz) * ni;
                y += p[0] * (nstcv * b * ay + cc * ay * ctcv) * ni;
                z += p[0] * (nstcv * b * az + cc * az * ctcv) * ni;
            }
            if d[28] != 0.0 {
                let p = &d[28..35];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((ax * p[1]).sin(), (ax * p[2]).cos(), (av * p[3]).sinh(), (av * p[4]).cosh());
                let ni = amount / r2;
                let cc = ni * c * sh / av * p[6];
                x += p[0] * s * ch * ni;
                y -= p[0] * cc * ay;
                z -= p[0] * cc * az;
            }
            if d[35] != 0.0 {
                let p = &d[35..42];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((av * p[3]).sin(), (av * p[4]).cos(), (ax * p[1]).sinh(), (ax * p[2]).cosh());
                let ni = amount / r2;
                let cc = ni * ch * s / av * p[6];
                x += p[0] * sh * c * ni;
                y -= p[0] * cc * ay;
                z -= p[0] * cc * az;
            }
            if d[42] != 0.0 {
                // [pow, x1, y1, y2, z1, z2]
                let p = &d[42..48];
                let e = (ax * p[1]).exp();
                let av = hyp * p[4];
                let (s, c) = ((av * p[2]).sin(), (av * p[3]).cos());
                let ea = e * s / av * p[5];
                x += p[0] * amount * e * c;
                y += p[0] * amount * ea * ay;
                z += p[0] * amount * ea * az;
            }
            if d[48] != 0.0 {
                let av = hyp;
                let cc = amount * av.atan2(ax) / av;
                x += d[48] * (sqr(ax) + sqr(av)).ln() * (0.5 * amount / d[49].ln());
                y += d[48] * cc * ay;
                z += d[48] * cc * az;
            }
            if d[50] != 0.0 {
                let p = &d[50..57];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((-ax * p[1]).sin(), (-ax * p[2]).cos(), (av * p[3]).sinh(), (av * p[4]).cosh());
                let ni = amount / r2;
                let cc = ni * s * sh / av * p[6];
                x += p[0] * c * ch * ni;
                y -= p[0] * cc * ay;
                z -= p[0] * cc * az;
            }
            if d[57] != 0.0 {
                let p = &d[57..64];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((av * p[3]).sin(), (av * p[4]).cos(), (ax * p[1]).sinh(), (ax * p[2]).cosh());
                let ni = amount / r2;
                let cc = ni * sh * s / av * p[6];
                x += p[0] * ch * c * ni;
                y -= p[0] * cc * ay;
                z -= p[0] * cc * az;
            }
            if d[78] != 0.0 {
                let p = &d[78..85];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((ax * p[1]).sin(), (ax * p[2]).cos(), (av * p[3]).sinh(), (av * p[4]).cosh());
                let sysz = sqr(ay) + sqr(az);
                let ni = amount / (sqr(ax) + sysz);
                let cc = c * sh / av * p[6];
                let b = -s * sh / av;
                let stcv = s * ch;
                let nstcv = -stcv;
                let ctcv = c * ch;
                x += p[0] * (stcv * ctcv + cc * b * sysz) * ni;
                y += p[0] * (nstcv * b * ay + cc * ay * ctcv) * ni;
                z += p[0] * (nstcv * b * az + cc * az * ctcv) * ni;
            }
            if d[85] != 0.0 {
                let p = &d[85..92];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((av * p[3]).sin(), (av * p[4]).cos(), (ax * p[1]).sinh(), (ax * p[2]).cosh());
                let sysz = sqr(ay) + sqr(az);
                let ni = amount / (sqr(ax) + sysz);
                let cc = ch * s / av * p[6];
                let b = sh * s / av;
                let stcv = sh * c;
                let nstcv = -stcv;
                let ctcv = c * ch;
                x += p[0] * (stcv * ctcv + cc * b * sysz) * ni;
                y += p[0] * (nstcv * b * ay + cc * ay * ctcv) * ni;
                z += p[0] * (nstcv * b * az + cc * az * ctcv) * ni;
            }
            if d[64] != 0.0 {
                let p = &d[64..71];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((ax * p[1]).sin(), (ax * p[2]).cos(), (av * p[3]).sinh(), (av * p[4]).cosh());
                let cc = amount * c * sh / av * p[6];
                x += p[0] * amount * s * ch;
                y += p[0] * cc * ay;
                z += p[0] * cc * az;
            }
            if d[71] != 0.0 {
                let p = &d[71..78];
                let av = hyp * p[5];
                let (s, c, sh, ch) = ((av * p[3]).sin(), (av * p[4]).cos(), (ax * p[1]).sinh(), (ax * p[2]).cosh());
                let cc = amount * ch * s / av * p[6];
                x += p[0] * amount * sh * c;
                y += p[0] * cc * ay;
                z += p[0] * cc * az;
            }
            v.x += x;
            v.y += y;
            v.z += z;
        },
        prepare: None,
        wgsl: "fn quaternion_trig(base: u32, ax: f32, hyp: f32, hyp_trig: bool) -> vec4<f32> {
  // (s, c, sh, ch): trigonometric functions of x and hyperbolic ones of |v|, or swapped
  let av = hyp * prm[base + 5u];
  if (hyp_trig) {
    return vec4<f32>(sin(av * prm[base + 3u]), cos(av * prm[base + 4u]), sinh(ax * prm[base + 1u]), cosh(ax * prm[base + 2u]));
  }
  return vec4<f32>(sin(ax * prm[base + 1u]), cos(ax * prm[base + 2u]), sinh(av * prm[base + 3u]), cosh(av * prm[base + 4u]));
}
@body
let ax = a.x;
let ay = a.y;
let az = a.z;
var q = vec3<f32>(0.0);
let hyp = length(vec2<f32>(ay, az));
let r2 = ax * ax + ay * ay + az * az;
let sysz = ay * ay + az * az;
let yz = vec2<f32>(ay, az);
if (D(0) != 0.0) {
  let t = quaternion_trig(base, ax, hyp, false);
  let av = hyp * D(5);
  let cc = AMOUNT * t.x * t.z / av * D(6);
  q += D(0) * vec3<f32>(AMOUNT * t.y * t.w, cc * yz);
}
if (D(7) != 0.0) {
  let t = quaternion_trig(base + 7u, ax, hyp, true);
  let av = hyp * D(12);
  let cc = AMOUNT * t.z * t.x / av * D(13);
  q += D(7) * vec3<f32>(AMOUNT * t.w * t.y, cc * yz);
}
if (D(14) != 0.0) {
  let t = quaternion_trig(base + 14u, ax, hyp, false);
  let av = hyp * D(19);
  let ni = AMOUNT / (ax * ax + sysz);
  let cc = t.y * t.z / av * D(20);
  let b = -t.x * t.z / av;
  let stcv = t.x * t.w;
  let ctcv = t.y * t.w;
  q += D(14) * vec3<f32>(stcv * ctcv + cc * b * sysz, -(-stcv * b * yz + cc * yz * ctcv)) * ni;
}
if (D(21) != 0.0) {
  let t = quaternion_trig(base + 21u, ax, hyp, true);
  let av = hyp * D(26);
  let ni = AMOUNT / (ax * ax + sysz);
  let cc = t.w * t.x / av * D(27);
  let b = t.z * t.x / av;
  let stcv = t.z * t.y;
  let ctcv = t.w * t.y;
  q += D(21) * vec3<f32>(stcv * ctcv + cc * b * sysz, -stcv * b * yz + cc * yz * ctcv) * ni;
}
if (D(28) != 0.0) {
  let t = quaternion_trig(base + 28u, ax, hyp, false);
  let av = hyp * D(33);
  let ni = AMOUNT / r2;
  let cc = ni * t.y * t.z / av * D(34);
  q += D(28) * vec3<f32>(t.x * t.w * ni, -cc * yz);
}
if (D(35) != 0.0) {
  let t = quaternion_trig(base + 35u, ax, hyp, true);
  let av = hyp * D(40);
  let ni = AMOUNT / r2;
  let cc = ni * t.w * t.x / av * D(41);
  q += D(35) * vec3<f32>(t.z * t.y * ni, -cc * yz);
}
if (D(42) != 0.0) {
  let e = exp(ax * D(43));
  let av = hyp * D(46);
  let ea = e * sin(av * D(44)) / av * D(47);
  q += D(42) * AMOUNT * vec3<f32>(e * cos(av * D(45)), ea * yz);
}
if (D(48) != 0.0) {
  let cc = AMOUNT * atan2(hyp, ax) / hyp;
  q += D(48) * vec3<f32>(log(ax * ax + hyp * hyp) * (0.5 * AMOUNT / log(D(49))), cc * yz);
}
if (D(50) != 0.0) {
  let t = quaternion_trig(base + 50u, -ax, hyp, false);
  let av = hyp * D(55);
  let ni = AMOUNT / r2;
  let cc = ni * t.x * t.z / av * D(56);
  q += D(50) * vec3<f32>(t.y * t.w * ni, -cc * yz);
}
if (D(57) != 0.0) {
  let t = quaternion_trig(base + 57u, ax, hyp, true);
  let av = hyp * D(62);
  let ni = AMOUNT / r2;
  let cc = ni * t.z * t.x / av * D(63);
  q += D(57) * vec3<f32>(t.w * t.y * ni, -cc * yz);
}
if (D(78) != 0.0) {
  let t = quaternion_trig(base + 78u, ax, hyp, false);
  let av = hyp * D(83);
  let ni = AMOUNT / (ax * ax + sysz);
  let cc = t.y * t.z / av * D(84);
  let b = -t.x * t.z / av;
  let stcv = t.x * t.w;
  let ctcv = t.y * t.w;
  q += D(78) * vec3<f32>(stcv * ctcv + cc * b * sysz, -stcv * b * yz + cc * yz * ctcv) * ni;
}
if (D(85) != 0.0) {
  let t = quaternion_trig(base + 85u, ax, hyp, true);
  let av = hyp * D(90);
  let ni = AMOUNT / (ax * ax + sysz);
  let cc = t.w * t.x / av * D(91);
  let b = t.z * t.x / av;
  let stcv = t.z * t.y;
  let ctcv = t.y * t.w;
  q += D(85) * vec3<f32>(stcv * ctcv + cc * b * sysz, -stcv * b * yz + cc * yz * ctcv) * ni;
}
if (D(64) != 0.0) {
  let t = quaternion_trig(base + 64u, ax, hyp, false);
  let av = hyp * D(69);
  let cc = AMOUNT * t.y * t.z / av * D(70);
  q += D(64) * vec3<f32>(AMOUNT * t.x * t.w, cc * yz);
}
if (D(71) != 0.0) {
  let t = quaternion_trig(base + 71u, ax, hyp, true);
  let av = hyp * D(76);
  let cc = AMOUNT * t.w * t.x / av * D(77);
  q += D(71) * vec3<f32>(AMOUNT * t.z * t.y, cc * yz);
}
v.x += q.x;
v.y += q.y;
v.z += q.z;",
    },
    VarDef {
        name: "rainbow_arc",
        params: &[ParamDef::new("radius", 0.5), ParamDef::new("width", 0.2), ParamDef::new("arc", 3.14159)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let radius = java_max(d[0].abs(), 0.01);
            let width = java_max(d[1].abs(), 0.001);
            let arc = java_max(d[2].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let band_r = java_round((r - radius) / width) * width + radius;
            let new_r = r + 0.6 * (java_max(band_r, 1e-4) - r);
            let new_theta = java_max(-arc, java_min(arc, theta));
            v.x += amount * new_r * new_theta.cos();
            v.y += amount * new_r * new_theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let radius = max(abs(D(0)), 0.01);
let width = max(abs(D(1)), 0.001);
let arc = max(abs(D(2)), 0.01);
let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
let band_r = java_round((r - radius) / width) * width + radius;
let new_r = r + 0.6 * (max(band_r, 1e-4) - r);
let new_theta = max(-arc, min(arc, theta));
v.x += AMOUNT * new_r * cos(new_theta);
v.y += AMOUNT * new_r * sin(new_theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "rational3",
        params: &[
            ParamDef::new("a", 0.5),
            ParamDef::new("b", 0.0),
            ParamDef::new("c", 0.25),
            ParamDef::new("d", 1.0),
            ParamDef::new("e", 0.0),
            ParamDef::new("f", 0.9),
            ParamDef::new("g", 0.0),
            ParamDef::new("h", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (xsqr, ysqr) = (a.x * a.x, a.y * a.y);
            let (xcb, ycb) = (xsqr * a.x, ysqr * a.y);
            let zt3 = xcb - 3.0 * a.x * ysqr;
            let zt2 = xsqr - ysqr;
            let zb3 = 3.0 * xsqr * a.y - ycb;
            let zb2 = 2.0 * a.x * a.y;
            let tr = (d[0] * zt3) + (d[1] * zt2) + (d[2] * a.x) + d[3];
            let ti = (d[0] * zb3) + (d[1] * zb2) + (d[2] * a.y);
            let br = (d[4] * zt3) + (d[5] * zt2) + (d[6] * a.x) + d[7];
            let bi = (d[4] * zb3) + (d[5] * zb2) + (d[6] * a.y);
            let r3den = 1.0 / (br * br + bi * bi);
            v.x += amount * (tr * br + ti * bi) * r3den;
            v.y += amount * (ti * br - tr * bi) * r3den;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let xsqr = a.x * a.x;
let ysqr = a.y * a.y;
let zt3 = xsqr * a.x - 3.0 * a.x * ysqr;
let zt2 = xsqr - ysqr;
let zb3 = 3.0 * xsqr * a.y - ysqr * a.y;
let zb2 = 2.0 * a.x * a.y;
let tr = D(0) * zt3 + D(1) * zt2 + D(2) * a.x + D(3);
let ti = D(0) * zb3 + D(1) * zb2 + D(2) * a.y;
let br = D(4) * zt3 + D(5) * zt2 + D(6) * a.x + D(7);
let bi = D(4) * zb3 + D(5) * zb2 + D(6) * a.y;
let r3den = 1.0 / (br * br + bi * bi);
v.x += AMOUNT * (tr * br + ti * bi) * r3den;
v.y += AMOUNT * (ti * br - tr * bi) * r3den;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "rays1",
        false,
        |_, ctx, a, v, amount| {
            let t = sqr(a.x) + sqr(a.y);
            let u = 1.0 / t.sqrt().tan() + (amount * sqr(M_2_PI));
            v.x = amount * u * t / a.x;
            v.y = amount * u * t / a.y;
            ctx.pass_z(a, v, amount);
        },
        "let t = a.x * a.x + a.y * a.y;
let u = 1.0 / tan(sqrt(t)) + AMOUNT * (4.0 / (PI * PI));
v.x = AMOUNT * u * t / a.x;
v.y = AMOUNT * u * t / a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "rays2",
        false,
        |_, ctx, a, v, amount| {
            let t = sqr(a.x) + sqr(a.y);
            let u = 1.0 / ((t + SMALL_EPSILON) * (1.0 / t + SMALL_EPSILON).tan()).cos();
            v.x = (amount / 10.0) * u * t / a.x;
            v.y = (amount / 10.0) * u * t / a.y;
            ctx.pass_z(a, v, amount);
        },
        "let t = a.x * a.x + a.y * a.y;
let u = 1.0 / cos((t + EPS) * tan(1.0 / t + EPS));
v.x = (AMOUNT / 10.0) * u * t / a.x;
v.y = (AMOUNT / 10.0) * u * t / a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "rays3",
        false,
        |_, ctx, a, v, amount| {
            let t = sqr(a.x) + sqr(a.y);
            let u = 1.0 / ((sqr(t) + SMALL_EPSILON).sin() * (1.0 / sqr(t) + SMALL_EPSILON).sin()).cos().sqrt();
            v.x = (amount / 10.0) * u * t.cos() * t / a.x;
            v.y = (amount / 10.0) * u * t.tan() * t / a.y;
            ctx.pass_z(a, v, amount);
        },
        "let t = a.x * a.x + a.y * a.y;
let u = 1.0 / sqrt(cos(sin(t * t + EPS) * sin(1.0 / (t * t) + EPS)));
v.x = (AMOUNT / 10.0) * u * cos(t) * t / a.x;
v.y = (AMOUNT / 10.0) * u * tan(t) * t / a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "rbox3D",
        params: &[ParamDef::range("dx", 0.5, 0.0, 0.5), ParamDef::range("dy", 0.5, 0.0, 0.5), ParamDef::range("dz", 0.5, 0.0, 0.5), ParamDef::range("r", 0.05, 0.0, 0.25)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [dx, dy, dz, r]
            for _ in 0..50 {
                let (x, y, z) = (ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5);
                let q = [x.abs() - d[0], y.abs() - d[1], z.abs() - d[2]];
                let l = (sqr(q[0].max(0.0)) + sqr(q[1].max(0.0)) + sqr(q[2].max(0.0))).sqrt();
                let dist = l + java_min(java_max(q[0], java_max(q[1], q[2])), 0.0) - d[3];
                if dist < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, 0.5), limit_value(p[1], 0.0, 0.5), limit_value(p[2], 0.0, 0.5), limit_value(p[3], 0.0, 0.25)]),
        wgsl: "var found = false;
for (var i = 0; i < 50; i++) {
  let p = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  let q = abs(p) - vec3<f32>(D(0), D(1), D(2));
  let dist = length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0) - D(3);
  if (dist < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * p.x;
    v.y += AMOUNT * p.y;
    v.z += AMOUNT * p.z;
    found = true;
    break;
  }
}
if (!found) { v.hide = 1.0; }",
    },
    VarDef {
        name: "r_circleblur",
        params: &[ParamDef::new("n", 1.0), ParamDef::new("seed", 0.0), ParamDef::new("dist", 0.5), ParamDef::new("min", 0.1), ParamDef::new("max", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [n, seed, dist, min, max]
            let angle = a.y.atan2(a.x);
            let rad = fmod((sqr(a.x) + sqr(a.y)).sqrt(), d[0].abs());
            let by = java_round((angle + rad).sin() * rad);
            let bx = java_round((angle + rad).cos() * rad);
            let mut rad2 = ctx.random().sqrt() * 0.5;
            let angle2 = ctx.random() * M_2PI;
            let h = |t: f64| {
                let s = (t + d[1]).sin() * 43758.5453;
                s - s.trunc()
            };
            let a1 = h(bx * 127.1 + by * 311.7);
            let a2 = h(bx * 269.5 + by * 183.3);
            let a3 = h(by * 12.9898 + bx * 78.233) * (d[4] - d[3]) + d[3];
            rad2 *= a3;
            v.x += amount * (bx + rad2 * angle2.cos() + a2 * d[2]);
            v.y += amount * (by + rad2 * angle2.sin() + a1 * d[2]);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        // GPU: the sin based hash runs in f32 (different cell offsets).
        wgsl: "let angle = atan2(a.y, a.x);
let rad = fmod(sqrt(a.x * a.x + a.y * a.y), abs(D(0)));
let by = java_round(sin(angle + rad) * rad);
let bx = java_round(cos(angle + rad) * rad);
var rad2 = sqrt(rnd()) * 0.5;
let angle2 = rnd() * TWO_PI;
let h = sin(vec3<f32>(bx * 127.1 + by * 311.7, bx * 269.5 + by * 183.3, by * 12.9898 + bx * 78.233) + D(1)) * 43758.5453;
let a1 = h.x - trunc(h.x);
let a2 = h.y - trunc(h.y);
let a3 = h.z - trunc(h.z);
rad2 *= a3 * (D(4) - D(3)) + D(3);
v.x += AMOUNT * (bx + rad2 * cos(angle2) + a2 * D(2));
v.y += AMOUNT * (by + rad2 * sin(angle2) + a1 * D(2));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "recurrenceplot",
        params: &[
            ParamDef::int("func_id", 0.0, 0.0, 29.0),
            ParamDef::new("a", 0.5),
            ParamDef::new("b", 10.0),
            ParamDef::int("k", 5.0, 0.0, 20.0),
            ParamDef::int("Density", 700.0, 1.0, 5000.0),
            ParamDef::int("dc", 0.0, 0.0, 2.0),
            ParamDef::new("scale", 1.0),
            ParamDef::new("angle", 0.0),
            ParamDef::new("offset", 0.0),
            ParamDef::new("tolerance", 5.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            // d = [func_id, a, b, k, Density, dc, scale, angle, offset, tolerance, ldcs]
            let n = d[4];
            let i = (n * ctx.random()) as i32;
            let j = (n * ctx.random()) as i32;
            let (id, k) = (d[0] as i32, d[3] as i32);
            let y1 = recurrenceplot_y(id, i, d[1], d[2], k, n);
            let y2 = recurrenceplot_y(id, j, d[1], d[2], k, n);
            // The last plotted point is kept across calls (per render thread here).
            if ((y1 - y2) * (y1 - y2)).sqrt() < d[9] {
                v.x += amount * (i as f64 / n - 0.5);
                v.y += amount * (j as f64 / n - 0.5);
                *state_slot(ctx, d, 0, [0.0; 2]) = [v.x, v.y];
                let dc = d[5] as i32;
                if dc == 1 {
                    v.color = fmod((0.5 * d[10] * v.x * v.y + d[8]).abs(), 1.0);
                } else if dc == 2 {
                    let (s, c) = (d[7].sin(), d[7].cos());
                    v.color = fmod((0.5 * (d[10] * (c * v.x + s * v.y + d[8]) + 1.0)).abs(), 1.0);
                }
            } else {
                [v.x, v.y] = *state_slot(ctx, d, 0, [0.0; 2]);
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for k in [0, 3, 4, 5] {
                d[k] = (p[k] as i32) as f64;
            }
            d.push(1.0 / if p[6] == 0.0 { 10E-6 } else { p[6] });
            d
        }),
        // GPU: no memory of the last plotted point; the sample pair is redrawn (up to 32 times)
        // until it lies on the plot, else the point moves to the origin.
        wgsl: "fn recurrenceplot_pow(x: f32, e: i32) -> f32 {
  var r = 1.0;
  for (var m = 0; m < e; m++) { r *= x; }
  return r;
}
fn recurrenceplot_y(id: i32, ii: i32, a: f32, b: f32, k: i32, n: f32) -> f32 {
  let i = f32(ii);
  switch (id) {
    case 0: { return 50.0 * sin(i * 4.0 * PI / n) + 25.0 * sin(i * 40.0 * PI / n); }
    case 1: { return 10.0 * sin(i * 5.0 * PI / n) * 5.0 * sin(i * 15.0 * PI / n); }
    case 2: { return 50.0 * cos(i * 24.0 * PI / n + 5.0 * sin(i * 6.0 * PI / n)); }
    case 3: { return 50.0 * sin(i * 20.0 * PI / n) + 50.0 * sin(i * 22.0 * PI / n + PI); }
    case 4: { return 50.0 * sin(i * 20.0 * PI / n) + 10.0 * sin(i * 22.0 * PI / n); }
    case 5: { return 50.0 * sin(i * a * PI / n + sin(i * a * PI / n)); }
    case 6: { return a / (1.0 + exp(-b * (i - n / 2.0) / n)); }
    case 7: {
      let h = 0.5 * n;
      return (400.0 / PI) * (sin(i * PI / h) + sin(i * 3.0 * PI / h) / 3.0 + sin(i * 5.0 * PI / h) / 5.0 + sin(i * 7.0 * PI / h) / 7.0);
    }
    case 8: {
      let h = 0.5 * n;
      return 400.0 * (sin(i * PI / h) - sin(i * 3.0 * PI / h) / 9.0 + sin(i * 5.0 * PI / h) / 25.0 - sin(i * 7.0 * PI / h) / 49.0);
    }
    case 9: { return tanh(a * i * PI / n); }
    case 10: { return tan(a * i * PI / n); }
    case 11: {
      var s = 0.0;
      for (var m = 1; m <= 8; m++) { s += sin(i * f32(m) * PI / (0.5 * n)) / f32(m); }
      return (-200.0 / PI) * s;
    }
    case 12: { return 50.0 * sin(i * 40.0 * PI / n) + 50.0 * sin(i * 44.0 * PI / n); }
    case 13: { return 50.0 * sin(i * 40.0 * PI / n) + 50.0 * sin(i * 44.0 * PI / n + PI); }
    case 14: { return 100.0 * sin(i * 16.0 * PI / n) - 500.0 + i * 0.25; }
    case 15: { return 100.0 * sin(i * 16.0 * PI / n); }
    case 16: { return 1.0 / cos(i * a * PI / n); }
    case 17: { return 50.0 * cos(i * 2.0 * PI * 25.0 / n + 25.0 * sin(i * 2.0 * PI * 0.5 / n)); }
    case 18: {
      let t = 2.0 * a * i / n - a;
      return a * exp(-b * t * t);
    }
    case 19: { return 500.0 * sin(i * 10.0 * PI / n) * exp(-5.0 * i / n); }
    case 20: { return 500.0 * sin(i * 20.0 * PI / n) * exp(-3.0 * i / n); }
    case 21: { return 50.0 * tan(i * 2.0 * PI / n); }
    case 22: {
      let x = 2.0 * i / n - 1.0;
      switch (i32(b)) {
        case 0: { return a; }
        case 1: { return a * x; }
        case 2: { return a * (3.0 * x * x - 1.0) / 2.0; }
        case 3: { return a * (5.0 * recurrenceplot_pow(x, 3) - 3.0 * x) / 2.0; }
        case 4: { return a * (35.0 * recurrenceplot_pow(x, 4) - 30.0 * x * x + 3.0) / 8.0; }
        case 5: { return a * (63.0 * recurrenceplot_pow(x, 5) - 70.0 * recurrenceplot_pow(x, 3) + 15.0 * x) / 8.0; }
        case 6: { return a * (231.0 * recurrenceplot_pow(x, 6) - 315.0 * recurrenceplot_pow(x, 4) + 105.0 * x * x - 5.0) / 16.0; }
        case 7: { return a * (429.0 * recurrenceplot_pow(x, 7) - 693.0 * recurrenceplot_pow(x, 5) + 315.0 * recurrenceplot_pow(x, 3) - 35.0 * x) / 16.0; }
        case 8: { return a * (6435.0 * recurrenceplot_pow(x, 8) - 12012.0 * recurrenceplot_pow(x, 6) + 6930.0 * recurrenceplot_pow(x, 4) - 1260.0 * x * x + 35.0) / 128.0; }
        case 9: { return a * (12155.0 * recurrenceplot_pow(x, 9) - 25740.0 * recurrenceplot_pow(x, 7) + 18018.0 * recurrenceplot_pow(x, 5) - 4620.0 * recurrenceplot_pow(x, 3) + 315.0 * x) / 128.0; }
        case 10: { return a * (46189.0 * recurrenceplot_pow(x, 10) - 109395.0 * recurrenceplot_pow(x, 8) + 90090.0 * recurrenceplot_pow(x, 6) - 30030.0 * recurrenceplot_pow(x, 4) + 3465.0 * x * x - 63.0) / 256.0; }
        default: { return 0.0; }
      }
    }
    case 23: {
      var s = 0.0;
      for (var j = 1; j <= k; j++) {
        let o = 2.0 * f32(j) - 1.0;
        s += sin(i * o * b * 2.0 * PI / n) / o;
      }
      return s * (4.0 * a / PI);
    }
    case 24: {
      var s = 0.0;
      for (var j = 1; j <= k; j++) { s += sin(i * f32(j) * b * 2.0 * PI / n) / f32(j); }
      return s * (-2.0 * a / PI);
    }
    case 25: {
      var s = 0.0;
      for (var j = 1; j <= k; j++) {
        let fj = f32(j);
        s += cos(i * (2.0 * fj) * b * 2.0 * PI / n) / ((2.0 * fj - 1.0) * (2.0 * fj + 1.0));
      }
      return (4.0 * a / PI) * (0.5 - s);
    }
    case 26: {
      var s = 0.0;
      for (var j = 1; j <= k; j++) {
        let fj = f32(j);
        s += select(1.0, -1.0, j % 2 == 1) * cos(i * (2.0 * fj) * b * PI / n) / (fj * fj);
      }
      return (a * PI * PI / 3.0) + 4.0 * a * s;
    }
    case 27: {
      let t = b * PI * (i - 0.5 * n) / n;
      return a * sin(t) / t;
    }
    case 28: {
      let t = (i - 0.5 * n) / n;
      return a * exp(-b * (t * t)) * cos(2.0 * f32(k) * PI * i / n);
    }
    case 29: {
      let x = 2.0 * i / n - 1.0;
      var t0 = 1.0;
      var t1 = x;
      if (k < 0 || k > 12) { return 0.0; }
      if (k == 0) { return a; }
      for (var m = 1; m < k; m++) {
        let t2 = 2.0 * x * t1 - t0;
        t0 = t1;
        t1 = t2;
      }
      return a * t1;
    }
    default: { return 0.0; }
  }
}
@body
let n = D(4);
let id = i32(D(0));
let k = i32(D(3));
var found = false;
var i = 0;
var j = 0;
for (var t = 0; t < 32; t++) {
  i = i32(n * rnd());
  j = i32(n * rnd());
  let y1 = recurrenceplot_y(id, i, D(1), D(2), k, n);
  let y2 = recurrenceplot_y(id, j, D(1), D(2), k, n);
  if (abs(y1 - y2) < D(9)) {
    found = true;
    break;
  }
}
if (found) {
  v.x += AMOUNT * (f32(i) / n - 0.5);
  v.y += AMOUNT * (f32(j) / n - 0.5);
  let dc = i32(D(5));
  if (dc == 1) {
    v.color = fmod(abs(0.5 * D(10) * v.x * v.y + D(8)), 1.0);
  } else if (dc == 2) {
    v.color = fmod(abs(0.5 * (D(10) * (cos(D(7)) * v.x + sin(D(7)) * v.y + D(8)) + 1.0)), 1.0);
  }
} else {
  v.x = 0.0;
  v.y = 0.0;
}",
    },
    VarDef {
        name: "resonance",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("decay", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let freq = java_max(d[0].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let ring = java_round(r * freq) / freq;
            let dist = r - ring;
            let pull = d[1] * (-dist * dist * freq * freq * 4.0).exp();
            let nr = r - pull * dist;
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let freq = max(abs(D(0)), 0.01);
let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
let dist = r - java_round(r * freq) / freq;
let nr = r - D(1) * exp(-dist * dist * freq * freq * 4.0) * dist;
v.x += AMOUNT * nr * cos(theta);
v.y += AMOUNT * nr * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "rewind",
        params: &[ParamDef::new("speed", 1.0), ParamDef::new("radius", 0.5), ParamDef::new("decay", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y + 1.0e-10).sqrt();
            let theta = a.y.atan2(a.x);
            let nr = r * (1.0 - d[2] * 0.05);
            let nt = theta + d[0] / (r + d[1]);
            v.x += amount * nr * nt.cos();
            v.y += amount * nr * nt.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y + 1.0e-10);
let nr = r * (1.0 - D(2) * 0.05);
let nt = atan2(a.y, a.x) + D(0) / (r + D(1));
v.x += AMOUNT * nr * cos(nt);
v.y += AMOUNT * nr * sin(nt);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "rhodonea",
        params: &[
            ParamDef::new("knumer", 3.0),
            ParamDef::new("kdenom", 4.0),
            ParamDef::int("inner_mode", 1.0, 0.0, 6.0),
            ParamDef::int("outer_mode", 1.0, 0.0, 6.0),
            ParamDef::new("inner_spread", 0.0),
            ParamDef::new("outer_spread", 0.0),
            ParamDef::new("inner_spread_ratio", 1.0),
            ParamDef::new("outer_spread_ratio", 1.0),
            ParamDef::new("spread_split", 1.0),
            ParamDef::new("fill", 0.0),
            ParamDef::new("radial_offset", 0.0),
            ParamDef::new("cycles", 0.0),
            ParamDef::new("cycle_offset", 0.0),
            ParamDef::new("metacycles", 1.0),
            ParamDef::new("metacycle_expansion", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [knumer, kdenom, inner_mode, outer_mode, inner_spread, outer_spread, inner_spread_ratio,
            //      outer_spread_ratio, spread_split, fill, radial_offset, cycles, cycle_offset, metacycles,
            //      metacycle_expansion, k 15, cycles 16, cycles_to_close 17]
            let (k, cycles, ctc) = (d[15], d[16], d[17]);
            let rin = d[8] * ((a.x * a.x) + (a.y * a.y)).sqrt();
            let tin = a.y.atan2(a.x);
            let t = cycles * (tin + (d[12] * 2.0 * M_PI));
            let mut r = (k * t).cos() + d[10];
            if d[9] != 0.0 {
                r += d[9] * (ctx.random() - 0.5);
            }
            let (x, y) = (r * t.cos(), r * t.sin());
            let expansion = ((cycles * (tin + M_PI)) / (ctc * 2.0 * M_PI)).floor();
            let am = amount + (expansion * d[14]);
            let signed = |va: f64, w: f64| if w < 0.0 { -va.abs() } else { va.abs() };
            let outer = rin.abs() > r.abs();
            let (mode, spread, ratio) = if outer { (d[3] as i32, d[5], d[7]) } else { (d[2] as i32, d[4], d[6]) };
            // modes 5 and 6 are swapped between inside and outside
            let mode = if !outer && (mode == 5 || mode == 6) { 11 - mode } else { mode };
            match mode {
                1 => {
                    let rinx = (rin * spread * ratio) - (spread * ratio) + 1.0;
                    let riny = (rin * spread) - spread + 1.0;
                    v.x += am * rinx * x;
                    v.y += am * riny * y;
                    if v.y == 0.0 {
                        v.x = 0.0;
                    }
                }
                2 => {
                    let (xin, yin) = (signed(a.x, x), signed(a.y, y));
                    if outer {
                        v.x += am * (x + (spread * ratio * (xin - x)));
                        v.y += am * (y + (spread * (yin - y)));
                    } else {
                        v.x += am * (x - (spread * ratio * (x - xin)));
                        v.y += am * (y - (spread * (y - yin)));
                    }
                }
                3 => {
                    let (xin, yin) = (signed(a.x, x), signed(a.y, y));
                    if outer {
                        v.x += am * (x + (spread * ratio * xin));
                        v.y += am * (y + (spread * yin));
                    } else {
                        v.x += am * (x - (spread * ratio * xin));
                        v.y += am * (y - (spread * yin));
                    }
                }
                4 => {
                    let rinx = (0.5 * rin) + (spread * ratio);
                    let riny = (0.5 * rin) + spread;
                    v.x += am * rinx * x;
                    v.y += am * riny * y;
                }
                5 => {
                    v.x += a.x;
                    v.y += a.y;
                }
                6 => v.do_hide = true,
                _ => {
                    v.x += am * x;
                    v.y += am * y;
                }
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for k in [2, 3] {
                let m = p[k].floor() as i32;
                d[k] = if !(0..=6).contains(&m) { 0.0 } else { m as f64 };
            }
            let (k, cycles, ctc) = rhodonea_cycles(&d);
            d.extend([k, cycles, ctc]);
            d
        }),
        wgsl: "let rin = D(8) * sqrt(a.x * a.x + a.y * a.y);
let tin = atan2(a.y, a.x);
let cycles = D(16);
let t = cycles * (tin + (D(12) * 2.0 * PI));
var r = cos(D(15) * t) + D(10);
if (D(9) != 0.0) { r += D(9) * (rnd() - 0.5); }
let x = r * cos(t);
let y = r * sin(t);
let am = AMOUNT + floor((cycles * (tin + PI)) / (D(17) * 2.0 * PI)) * D(14);
let outer = abs(rin) > abs(r);
var mode = i32(D(2));
var spread = D(4);
var ratio = D(6);
var sg = -1.0;
if (outer) {
  mode = i32(D(3));
  spread = D(5);
  ratio = D(7);
  sg = 1.0;
} else if (mode == 5 || mode == 6) {
  mode = 11 - mode;
}
let xin = select(abs(a.x), -abs(a.x), x < 0.0);
let yin = select(abs(a.y), -abs(a.y), y < 0.0);
if (mode == 1) {
  v.x += am * ((rin * spread * ratio) - (spread * ratio) + 1.0) * x;
  v.y += am * ((rin * spread) - spread + 1.0) * y;
  if (v.y == 0.0) { v.x = 0.0; }
} else if (mode == 2) {
  v.x += am * (x + sg * (spread * ratio * (sg * (xin - x))));
  v.y += am * (y + sg * (spread * (sg * (yin - y))));
} else if (mode == 3) {
  v.x += am * (x + sg * (spread * ratio * xin));
  v.y += am * (y + sg * (spread * yin));
} else if (mode == 4) {
  v.x += am * ((0.5 * rin) + (spread * ratio)) * x;
  v.y += am * ((0.5 * rin) + spread) * y;
} else if (mode == 5) {
  v.x += a.x;
  v.y += a.y;
} else if (mode == 6) {
  v.hide = 1.0;
} else {
  v.x += am * x;
  v.y += am * y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "rhombus3D",
        params: &[ParamDef::range("la", 0.5, 0.0, 0.5), ParamDef::range("lb", 0.5, 0.0, 0.5), ParamDef::range("h", 0.5, 0.0, 0.5), ParamDef::range("r", 0.05, 0.0, 0.25)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [la, lb, h, r]
            let b = vec2(d[0], d[1]);
            for _ in 0..50 {
                let (x, y, z) = (ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5);
                let (px, py, pz) = (x.abs(), y.abs(), z.abs());
                let pxz = vec2(px, pz);
                let w = b - pxz * 2.0;
                let f = clamp((b.x * w.x - b.y * w.y) / dot(b, b), -1.0, 1.0);
                let qx = length(pxz - b * vec2(1.0 - f, 1.0 + f) * 0.5) * sign(px * b.y + pz * b.x - b.x * b.y) - d[3];
                let q = vec2(qx, py - d[2]);
                let dist = min(max(q.x, q.y), 0.0) + length(max(q, 0.0));
                if dist < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, 0.5), limit_value(p[1], 0.0, 0.5), limit_value(p[2], 0.0, 0.5), limit_value(p[3], 0.0, 0.25)]),
        wgsl: "let b = vec2<f32>(D(0), D(1));
var found = false;
for (var i = 0; i < 50; i++) {
  let p = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  let ap = abs(p);
  let w = b - ap.xz * 2.0;
  let f = clamp((b.x * w.x - b.y * w.y) / dot(b, b), -1.0, 1.0);
  let q = vec2<f32>(length(ap.xz - b * vec2<f32>(1.0 - f, 1.0 + f) * 0.5) * sign(ap.x * b.y + ap.z * b.x - b.x * b.y) - D(3), ap.y - D(2));
  let dist = min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0)));
  if (dist < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * p.x;
    v.y += AMOUNT * p.y;
    v.z += AMOUNT * p.z;
    found = true;
    break;
  }
}
if (!found) { v.hide = 1.0; }",
    },
    VarDef {
        name: "ridged",
        params: &[ParamDef::new("freq", 2.0), ParamDef::new("amp", 0.4)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x, y) = (d[0] * a.x, d[0] * a.y);
            let v1 = 1.0 - (x + y * 0.3).sin().abs();
            let v2 = (1.0 - (2.1 * x - y * 0.7).sin().abs()) * 0.5;
            let val = d[1] * (v1 + v2);
            let theta = a.y.atan2(a.x);
            v.x += amount * val * theta.cos();
            v.y += amount * val * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = D(0) * a.x;
let y = D(0) * a.y;
let val = D(1) * ((1.0 - abs(sin(x + y * 0.3))) + (1.0 - abs(sin(2.1 * x - y * 0.7))) * 0.5);
let theta = atan2(a.y, a.x);
v.x += AMOUNT * val * cos(theta);
v.y += AMOUNT * val * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "ringer",
        params: &[
            ParamDef::int("hide_inner", 0.0, 0.0, 1.0),
            ParamDef::new("innerradius", 0.5),
            ParamDef::new("outerradius", 1.0),
            ParamDef::range("thickness", 0.0, 0.0, 1.0),
            ParamDef::range("contrast", 0.0, 0.0, 1.0),
            ParamDef::new("pow", 1.0),
            ParamDef::range("scatter_area", 0.0, -1.0, 1.0),
            ParamDef::int("hide_outer", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [hide_inner, innerradius, outerradius, thickness, contrast, pow, scatter_area, hide_outer,
            //      radius1, radius2, gamma, abs_pow, cA]
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let alpha = d[1] / r;
            if r < d[8] {
                if d[0] as i32 == 0 {
                    let (sp, cp) = (2.0 * M_PI * ctx.random()).sin_cos();
                    let rr = if ctx.random() < d[10] { d[8] } else { d[9] };
                    v.x += amount * (rr * cp);
                    v.y += amount * (rr * sp);
                } else {
                    // The last point is kept across calls (per render thread here).
                    [v.x, v.y] = *state_slot(ctx, d, 0, [0.0; 2]);
                }
            } else if ctx.random() > d[4] * alpha.powf(d[11]) {
                v.x += amount * a.x;
                v.y += amount * a.y;
            } else {
                v.x += amount * alpha * alpha * a.x;
                v.y += amount * alpha * alpha * a.y;
            }
            *state_slot(ctx, d, 0, [0.0; 2]) = [v.x, v.y];
            let cr = d[2];
            let rad = (v.x * v.x + v.y * v.y).sqrt();
            let ang = v.y.atan2(v.x);
            let rdc = cr + ctx.random() * 0.5 * d[12];
            let esc = rad > cr;
            let cr0 = d[7] as i32 == 1;
            v.do_hide = false;
            if cr0 && esc {
                v.x = 0.0;
                v.y = 0.0;
                v.do_hide = true;
            } else if !cr0 && esc {
                v.x = amount * rdc * ang.cos();
                v.y = amount * rdc * ang.sin();
            } else {
                v.x *= amount;
                v.y *= amount;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(p[0], 0.0, 1.0) as i32 as f64;
            d[3] = limit_value(p[3], 0.0, 1.0);
            d[4] = limit_value(p[4], 0.0, 1.0);
            d[7] = limit_value(p[7], 0.0, 1.0) as i32 as f64;
            let r1 = d[1] + d[3];
            let r2 = sqr(d[1]) / r1;
            d.extend([r1, r2, r1 / (r1 + r2), p[5].abs(), java_max(-1.0, java_min(p[6], 1.0))]);
            d
        }),
        // GPU: no memory of the last point; hidden inner points restart at the origin.
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let alpha = D(1) / r;
if (r < D(8)) {
  if (i32(D(0)) == 0) {
    let phi = 2.0 * PI * rnd();
    var rr = D(9);
    if (rnd() < D(10)) { rr = D(8); }
    v.x += AMOUNT * (rr * cos(phi));
    v.y += AMOUNT * (rr * sin(phi));
  } else {
    v.x = 0.0;
    v.y = 0.0;
  }
} else if (rnd() > D(4) * pow(alpha, D(11))) {
  v.x += AMOUNT * a.x;
  v.y += AMOUNT * a.y;
} else {
  v.x += AMOUNT * alpha * alpha * a.x;
  v.y += AMOUNT * alpha * alpha * a.y;
}
let cr = D(2);
let rad = sqrt(v.x * v.x + v.y * v.y);
let ang = atan2(v.y, v.x);
let rdc = cr + rnd() * 0.5 * D(12);
let esc = rad > cr;
let cr0 = i32(D(7)) == 1;
v.hide = 0.0;
if (cr0 && esc) {
  v.x = 0.0;
  v.y = 0.0;
  v.hide = 1.0;
} else if (!cr0 && esc) {
  v.x = AMOUNT * rdc * cos(ang);
  v.y = AMOUNT * rdc * sin(ang);
} else {
  v.x *= AMOUNT;
  v.y *= AMOUNT;
}",
    },
    VarDef {
        name: "rings3",
        params: &[ParamDef::new("val", 0.01), ParamDef::new("n", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [val, n, dx, c]
            let l = a.rad();
            let (dx, c) = (d[2], d[3]);
            if dx == 0.0 || l == 0.0 {
                return;
            }
            let k = (((l / dx + 1.0) as i32) / 2) as f64;
            let r = amount * (2.0 - dx * (k * 2.0 / l + 1.0) - d[1] * (k * c - 1.0) / l);
            v.x += r * a.x;
            v.y += r * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let dx = p[0] * p[0] + EPSILON;
            vec![p[0], p[1], dx, 2.0 * (dx - dx * dx)]
        }),
        wgsl: "let l = sqrt(a.x * a.x + a.y * a.y);
let dx = D(2);
if (dx != 0.0 && l != 0.0) {
  let k = f32(i32(l / dx + 1.0) / 2);
  let r = AMOUNT * (2.0 - dx * (k * 2.0 / l + 1.0) - D(1) * (k * D(3) - 1.0) / l);
  v.x += r * a.x;
  v.y += r * a.y;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    VarDef {
        name: "ripple",
        params: &[
            ParamDef::new("frequency", 2.0),
            ParamDef::new("velocity", 1.0),
            ParamDef::new("amplitude", 0.5),
            ParamDef::new("centerx", 0.0),
            ParamDef::new("centery", 0.0),
            ParamDef::new("phase", 0.0),
            ParamDef::new("scale", 1.0),
            ParamDef::int("fixed_dist_calc", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [frequency, velocity, amplitude, centerx, centery, phase, scale, fixed_dist_calc,
            //      f, s, p, is, pxa, pixa, vxp]
            let (f, s, p, is, pxa, pixa, vxp) = (d[8], d[9], d[10], d[11], d[12], d[13], d[14]);
            let (x, y) = ((a.x * s) - d[3], (a.y * s) + d[4]);
            let mut dd = if d[7] != 0.0 { (x * x + y * y).sqrt() } else { (x * x * y * y).sqrt() };
            if dd < SMALL_EPSILON {
                dd = SMALL_EPSILON;
            }
            let (nx, ny) = (x / dd, y / dd);
            let wave = (f * dd - vxp).cos();
            let (d1, d2) = (wave * pxa + dd, wave * pixa + dd);
            let (u1, v1) = (d[3] + nx * d1, -d[4] + ny * d1);
            let (u2, v2) = (d[3] + nx * d2, -d[4] + ny * d2);
            v.x = amount * (u1 + (u2 - u1) * p) * is;
            v.y = amount * (v1 + (v2 - v1) * p) * is;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[7] = ftoi(p[7]) as f64;
            let amp = p[2] * 0.01;
            let ph = p[5] * M_2PI - M_PI;
            let s = if p[6] == 0.0 { SMALL_EPSILON } else { p[6] };
            d.extend([p[0] * 5.0, s, ph, 1.0 / s, ph * amp, (M_PI - ph) * amp, p[1] * ph]);
            d
        }),
        wgsl: "let x = a.x * D(9) - D(3);
let y = a.y * D(9) + D(4);
var dd = sqrt(x * x * y * y);
if (D(7) != 0.0) { dd = sqrt(x * x + y * y); }
dd = max(dd, EPS);
let n = vec2<f32>(x, y) / dd;
let wave = cos(D(8) * dd - D(14));
let d1 = wave * D(12) + dd;
let d2 = wave * D(13) + dd;
let c = vec2<f32>(D(3), -D(4));
let p1 = c + n * d1;
let p2 = c + n * d2;
let q = mix(p1, p2, D(10));
v.x = AMOUNT * q.x * D(11);
v.y = AMOUNT * q.y * D(11);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "rippled",
        false,
        |_, ctx, a, v, amount| {
            let d = sqr(a.x) + sqr(a.y);
            v.x += amount / 2.0 * ((d + SMALL_EPSILON).tanh() * (2.0 * a.x));
            v.y += amount / 2.0 * ((d + SMALL_EPSILON).cos() * (2.0 * a.y));
            ctx.pass_z(a, v, amount);
        },
        "let d = a.x * a.x + a.y * a.y;
v.x += AMOUNT / 2.0 * (tanh(d + EPS) * (2.0 * a.x));
v.y += AMOUNT / 2.0 * (cos(d + EPS) * (2.0 * a.y));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "robinson",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let lat = java_max(-1.5707963267948966, java_min(1.5707963267948966, a.y * d[0]));
            let t = lat.abs() / 1.5707963267948966;
            let xs = 1.0 - 0.42 * t * t;
            let ys = 1.0 + 0.12 * t;
            v.x += amount * a.x * java_max(xs, 0.05) / d[0];
            v.y += amount * lat * ys / d[0];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let lat = clamp(a.y * D(0), -HALF_PI, HALF_PI);
let t = abs(lat) / HALF_PI;
v.x += AMOUNT * a.x * max(1.0 - 0.42 * t * t, 0.05) / D(0);
v.y += AMOUNT * lat * (1.0 + 0.12 * t) / D(0);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "romanesco",
        params: &[
            ParamDef::new("size", 0.5),
            ParamDef::int("recursion_depth", 7.0, 0.0, 20.0),
            ParamDef::int("num_arms", 1.0, 1.0, 12.0),
            ParamDef::new("arm_spread", 1.0),
            ParamDef::range("arm_elevation", 45.0, -90.0, 90.0),
            ParamDef::new("arm_twist", 0.0),
            ParamDef::int("floret_count", 100.0, 1.0, 1000.0),
            ParamDef::new("floret_scale", 0.28),
            ParamDef::new("pattern_spread", 0.4),
            ParamDef::new("spiral_twist", 1.0),
            ParamDef::new("cone_steepness", 1.0),
            ParamDef::new("floret_detail_size", 0.1),
            ParamDef::int("floret_shape", 0.0, 0.0, 3.0),
            ParamDef::range("pitch", 180.0, -180.0, 180.0),
            ParamDef::range("yaw", 0.0, -180.0, 180.0),
            ParamDef::range("roll", 0.0, -180.0, 180.0),
            ParamDef::int("color_mode", 0.0, 0.0, 3.0),
            ParamDef::range("solid_color_idx", 0.5, 0.0, 1.0),
            ParamDef::new("color_range_min", 0.0),
            ParamDef::new("color_range_max", 9.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [size, recursion_depth, num_arms, arm_spread, arm_elevation, arm_twist, floret_count,
            //      floret_scale, pattern_spread, spiral_twist, cone_steepness, floret_detail_size,
            //      floret_shape, pitch, yaw, roll, color_mode, solid_color_idx, color_range_min, color_range_max]
            type V3 = [f64; 3];
            let cross = |a: V3, b: V3| -> V3 { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] };
            let normalize = |v: V3| -> V3 {
                let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                if len > 1e-9 { [v[0] / len, v[1] / len, v[2] / len] } else { v }
            };
            let golden = M_PI * (3.0 - 5.0f64.sqrt());
            let mut scale = d[0];
            let mut pos = [0.0; 3];
            let (mut ax, mut ay, mut az) = ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]);
            let arms = d[2] as i32;
            if arms > 1 {
                let idx = (ctx.random() * arms as f64) as i32;
                let base_angle = idx as f64 / arms as f64 * 2.0 * M_PI;
                pos = [d[3] * base_angle.cos(), d[3] * base_angle.sin(), 0.0];
                let elev = d[4] * M_PI / 180.0;
                let arm_angle = base_angle + pos[1] * d[5];
                az = [arm_angle.cos() * elev.cos(), arm_angle.sin() * elev.cos(), elev.sin()];
                let up = if az[2].abs() > 0.999 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] };
                ax = normalize(cross(az, up));
                ay = cross(az, ax);
            }
            for _ in 0..d[1] as i32 {
                let fi = (ctx.random().powf(1.5) * d[6]) as i32;
                let sa = fi as f64 * golden * d[9];
                let r = d[8] * (fi as f64).sqrt();
                let (lx, ly, lz) = (r * sa.cos(), r * sa.sin(), r * d[10]);
                for k in 0..3 {
                    pos[k] += (ax[k] * lx + ay[k] * ly + az[k] * lz) * scale;
                }
                let n = normalize([-d[10] * sa.cos(), -d[10] * sa.sin(), 1.0]);
                az = normalize([
                    ax[0] * n[0] + ay[0] * n[1] + az[0] * n[2],
                    ax[1] * n[0] + ay[1] * n[1] + az[1] * n[2],
                    ax[2] * n[0] + ay[2] * n[1] + az[2] * n[2],
                ]);
                let up = if az[1].abs() > 0.999 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
                ax = normalize(cross(up, az));
                ay = cross(az, ax);
                scale *= d[7];
            }
            let ss = d[11] * scale;
            let mut l = [0.0; 3];
            match d[12] as i32 {
                0 => {
                    let r = [ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5];
                    let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
                    if rl > 1e-6 {
                        let br = ss * ctx.random();
                        l = [(r[0] / rl) * br, (r[1] / rl) * br, (r[2] / rl) * br];
                    }
                }
                1 => l = [(ctx.random() - 0.5) * ss, (ctx.random() - 0.5) * ss, (ctx.random() - 0.5) * ss],
                2 => l[2] = ss * ctx.random(),
                3 => {
                    let ra = ctx.random() * 2.0 * M_PI;
                    l = [ss * ra.cos(), ss * ra.sin(), 0.0];
                }
                _ => {}
            }
            let mut f = [0.0; 3];
            for k in 0..3 {
                f[k] = pos[k] + (ax[k] * l[0] + ay[k] * l[1] + az[k] * l[2]);
            }
            if d[13] != 0.0 || d[14] != 0.0 || d[15] != 0.0 {
                let (sp, cp) = (d[13].to_radians().sin(), d[13].to_radians().cos());
                let (sy, cy) = (d[14].to_radians().sin(), d[14].to_radians().cos());
                let (sr, cr) = (d[15].to_radians().sin(), d[15].to_radians().cos());
                (f[0], f[2]) = (f[0] * cy - f[2] * sy, f[0] * sy + f[2] * cy);
                (f[1], f[2]) = (f[1] * cp - f[2] * sp, f[1] * sp + f[2] * cp);
                (f[0], f[1]) = (f[0] * cr - f[1] * sr, f[0] * sr + f[1] * cr);
            }
            let range = d[19] - d[18];
            let mut ci = 0.5;
            match d[16] as i32 {
                0 => ci = d[17],
                1 if range > 1e-6 => ci = ((f[0] * f[0] + f[1] * f[1] + f[2] * f[2]).sqrt() - d[18]) / range,
                2 if range > 1e-6 => ci = ((f[0] * f[0] + f[1] * f[1]).sqrt() - d[18]) / range,
                3 if range > 1e-6 => ci = (f[2] - d[18]) / range,
                _ => {}
            }
            v.color = java_max(0.0, java_min(1.0, ci));
            v.x += f[0] * amount;
            v.y += f[1] * amount;
            v.z += f[2] * amount;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for k in [1, 2, 6] {
                d[k] = (p[k] as i32) as f64;
            }
            d[12] = limit_value(ftoi(p[12]), 0, 3) as f64;
            d[16] = limit_value(ftoi(p[16]), 0, 3) as f64;
            d
        }),
        wgsl: "fn romanesco_norm(v: vec3<f32>) -> vec3<f32> {
  let len = length(v);
  if (len > 1e-9) { return v / len; }
  return v;
}
@body
let golden = PI * (3.0 - sqrt(5.0));
var scale = D(0);
var pos = vec3<f32>(0.0);
var ax = vec3<f32>(1.0, 0.0, 0.0);
var ay = vec3<f32>(0.0, 1.0, 0.0);
var az = vec3<f32>(0.0, 0.0, 1.0);
let arms = i32(D(2));
if (arms > 1) {
  let idx = i32(rnd() * f32(arms));
  let base_angle = f32(idx) / f32(arms) * 2.0 * PI;
  pos = vec3<f32>(D(3) * cos(base_angle), D(3) * sin(base_angle), 0.0);
  let elev = D(4) * PI / 180.0;
  let arm_angle = base_angle + pos.y * D(5);
  az = vec3<f32>(cos(arm_angle) * cos(elev), sin(arm_angle) * cos(elev), sin(elev));
  var up = vec3<f32>(0.0, 0.0, 1.0);
  if (abs(az.z) > 0.999) { up = vec3<f32>(1.0, 0.0, 0.0); }
  ax = romanesco_norm(cross(az, up));
  ay = cross(az, ax);
}
let depth = i32(D(1));
for (var i = 0; i < depth; i++) {
  let fi = f32(i32(pow(rnd(), 1.5) * D(6)));
  let sa = fi * golden * D(9);
  let r = D(8) * sqrt(fi);
  pos += (ax * (r * cos(sa)) + ay * (r * sin(sa)) + az * (r * D(10))) * scale;
  let n = romanesco_norm(vec3<f32>(-D(10) * cos(sa), -D(10) * sin(sa), 1.0));
  az = romanesco_norm(ax * n.x + ay * n.y + az * n.z);
  var up = vec3<f32>(0.0, 1.0, 0.0);
  if (abs(az.y) > 0.999) { up = vec3<f32>(1.0, 0.0, 0.0); }
  ax = romanesco_norm(cross(up, az));
  ay = cross(az, ax);
  scale *= D(7);
}
let ss = D(11) * scale;
var l = vec3<f32>(0.0);
let shape = i32(D(12));
if (shape == 0) {
  let r = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  let rl = length(r);
  if (rl > 1e-6) { l = r / rl * (ss * rnd()); }
} else if (shape == 1) {
  l = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5) * ss;
} else if (shape == 2) {
  l.z = ss * rnd();
} else if (shape == 3) {
  let ra = rnd() * 2.0 * PI;
  l = vec3<f32>(ss * cos(ra), ss * sin(ra), 0.0);
}
var f = pos + ax * l.x + ay * l.y + az * l.z;
if (D(13) != 0.0 || D(14) != 0.0 || D(15) != 0.0) {
  let p = radians(D(13));
  let yw = radians(D(14));
  let rl = radians(D(15));
  f = vec3<f32>(f.x * cos(yw) - f.z * sin(yw), f.y, f.x * sin(yw) + f.z * cos(yw));
  f = vec3<f32>(f.x, f.y * cos(p) - f.z * sin(p), f.y * sin(p) + f.z * cos(p));
  f = vec3<f32>(f.x * cos(rl) - f.y * sin(rl), f.x * sin(rl) + f.y * cos(rl), f.z);
}
let range = D(19) - D(18);
var ci = 0.5;
let cm = i32(D(16));
if (cm == 0) {
  ci = D(17);
} else if (range > 1e-6) {
  if (cm == 1) { ci = (length(f) - D(18)) / range; }
  if (cm == 2) { ci = (length(f.xy) - D(18)) / range; }
  if (cm == 3) { ci = (f.z - D(18)) / range; }
}
v.color = clamp(ci, 0.0, 1.0);
v.x += f.x * AMOUNT;
v.y += f.y * AMOUNT;
v.z += f.z * AMOUNT;",
    },
    VarDef {
        name: "rose_wf",
        params: &[ParamDef::new("amp", 0.5), ParamDef::int("waves", 4.0, 0.0, 32.0), ParamDef::range("filled", 0.85, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let an = a.atan_xy();
            let mut r = d[0] * (d[1] * an).cos();
            if d[2] > 0.0 && d[2] > ctx.random() {
                r *= ctx.random();
            }
            v.x += amount * an.sin() * r;
            v.y += amount * an.cos() * r;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], ftoi(p[1]) as f64, p[2]]),
        wgsl: "let an = atan2(a.x, a.y);
var r = D(0) * cos(D(1) * an);
if (D(2) > 0.0 && D(2) > rnd()) { r *= rnd(); }
v.x += AMOUNT * sin(an) * r;
v.y += AMOUNT * cos(an) * r;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "rosoni",
        params: &[
            ParamDef::int("maxiter", 25.0, 1.0, 1024.0),
            ParamDef::int("sweetiter", 3.0, 0.0, 1023.0),
            ParamDef::int("altshapes", 0.0, 0.0, 1.0),
            ParamDef::new("cutoff", 1.0),
            ParamDef::new("radius", 0.4),
            ParamDef::new("dx", 0.6),
            ParamDef::new("dy", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [maxiter, sweetiter, altshapes, cutoff, radius, dx, dy, sina, cosa]
            let (x, y) = (a.x, a.y);
            let (cutoff, radius, dx, dy) = (d[3], d[4], d[5], d[6]);
            let r = if cutoff < 0.0 { java_max(x.abs(), y.abs()) + cutoff } else { (sqr(x) + sqr(y)).sqrt() - cutoff };
            if r > 0.0 {
                v.x += amount * x;
                v.y += amount * y;
                ctx.pass_z(a, v, amount);
                return;
            }
            let mut cerc = dx > 0.0;
            let (mut xrt, mut yrt) = (x, y);
            let (mut sweetx, mut sweety) = (xrt, yrt);
            let sweetiter = d[1] as i32;
            for i in 0..d[0] as i32 {
                let r2 = if d[2] as i32 == 0 {
                    if radius < 0.0 { java_max((xrt - dx).abs(), (yrt - dy).abs()) + radius } else { sqr(xrt - dx) + sqr(yrt - dy) - sqr(radius) }
                } else if radius < 0.0 {
                    (yrt - dy).atan2(xrt - dx).abs() * M_1_PI + radius
                } else if (xrt - dx) < 0.0 {
                    -(xrt - dx)
                } else {
                    sqr(yrt - dy) - sqr(xrt - dx) * (sqr(radius) - sqr(xrt - dx))
                };
                cerc ^= r2 <= 0.0;
                if i == sweetiter {
                    sweetx = xrt;
                    sweety = yrt;
                }
                (xrt, yrt) = (xrt * d[8] - yrt * d[7], xrt * d[7] + yrt * d[8]);
            }
            if cerc {
                if sweetiter == 0 {
                    if dy != 0.0 {
                        v.x -= amount * sweetx;
                    } else {
                        v.x += amount * sweetx;
                    }
                    v.y -= amount * sweety;
                } else {
                    v.x += amount * sweetx;
                    v.y += amount * sweety;
                }
            } else {
                v.x += amount * x;
                v.y += amount * y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(ftoi(p[0]), 1, 1024) as f64;
            d[1] = limit_value(ftoi(p[1]), 0, 1023) as f64;
            d[2] = ftoi(p[2]) as f64;
            let phi = M_2PI / d[0];
            d.extend([phi.sin(), phi.cos()]);
            d
        }),
        wgsl: "let x = a.x;
let y = a.y;
let cutoff = D(3);
let radius = D(4);
let dx = D(5);
let dy = D(6);
var r = sqrt(x * x + y * y) - cutoff;
if (cutoff < 0.0) { r = max(abs(x), abs(y)) + cutoff; }
var res = vec2<f32>(x, y);
if (r <= 0.0) {
  var cerc = dx > 0.0;
  var xrt = x;
  var yrt = y;
  var sweet = vec2<f32>(x, y);
  let sweetiter = i32(D(1));
  let n = i32(D(0));
  for (var i = 0; i < n; i++) {
    var r2 = 0.0;
    if (i32(D(2)) == 0) {
      r2 = (xrt - dx) * (xrt - dx) + (yrt - dy) * (yrt - dy) - radius * radius;
      if (radius < 0.0) { r2 = max(abs(xrt - dx), abs(yrt - dy)) + radius; }
    } else if (radius < 0.0) {
      r2 = abs(atan2(yrt - dy, xrt - dx)) / PI + radius;
    } else if ((xrt - dx) < 0.0) {
      r2 = -(xrt - dx);
    } else {
      r2 = (yrt - dy) * (yrt - dy) - (xrt - dx) * (xrt - dx) * (radius * radius - (xrt - dx) * (xrt - dx));
    }
    cerc = cerc != (r2 <= 0.0);
    if (i == sweetiter) { sweet = vec2<f32>(xrt, yrt); }
    let swp = xrt * D(8) - yrt * D(7);
    yrt = xrt * D(7) + yrt * D(8);
    xrt = swp;
  }
  if (cerc) {
    res = sweet;
    if (sweetiter == 0) {
      res.y = -sweet.y;
      if (dy != 0.0) { res.x = -sweet.x; }
    }
  }
}
v.x += AMOUNT * res.x;
v.y += AMOUNT * res.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "roundspher",
        false,
        |_, ctx, a, v, amount| {
            let d = sqr(a.x) + sqr(a.y);
            let e = 1.0 / d + sqr(M_2_PI);
            v.x += amount * (amount / d * a.x / e);
            v.y += amount * (amount / d * a.y / e);
            ctx.pass_z(a, v, amount);
        },
        "let d = a.x * a.x + a.y * a.y;
let e = 1.0 / d + 4.0 / (PI * PI);
v.x += AMOUNT * (AMOUNT / d * a.x / e);
v.y += AMOUNT * (AMOUNT / d * a.y / e);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "roundspher3D",
        true,
        |_, _, a, v, amount| {
            let f = (sqr(a.x) + sqr(a.y)).sqrt();
            let ttz = if a.z == 0.0 { f.cos() } else { a.z };
            let tpz = if v.z == 0.0 { f.cos() } else { v.z };
            let d = sqr(a.x) + sqr(a.y) + sqr(ttz);
            let e = 1.0 / d + sqr(M_2_PI);
            v.x += amount * (amount / d * a.x / e);
            v.y += amount * (amount / d * a.y / e);
            v.z = tpz + amount * (amount / d * ttz / e);
        },
        "let f = sqrt(a.x * a.x + a.y * a.y);
var ttz = a.z;
if (a.z == 0.0) { ttz = cos(f); }
var tpz = v.z;
if (v.z == 0.0) { tpz = cos(f); }
let d = a.x * a.x + a.y * a.y + ttz * ttz;
let e = 1.0 / d + 4.0 / (PI * PI);
v.x += AMOUNT * (AMOUNT / d * a.x / e);
v.y += AMOUNT * (AMOUNT / d * a.y / e);
v.z = tpz + AMOUNT * (AMOUNT / d * ttz / e);",
    ),
    VarDef {
        name: "rsquares_js",
        params: &[
            ParamDef::int("level", 2.0, 1.0, 9.0),
            ParamDef::int("show_lines", 1.0, 0.0, 1.0),
            ParamDef::new("line_thickness", 0.5),
            ParamDef::int("show_points", 0.0, 0.0, 1.0),
            ParamDef::new("point_thickness", 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [level, show_lines, line_thickness, show_points, point_thickness, segments,
            //      line_thickness / 100, point_thickness / 100, line_threshold, point_threshold]
            // Java cycles through the segments; the position is kept per render thread here.
            let slot = state_slot(ctx, d, 0, [0.0; 2]);
            let k = slot[0] as i64;
            slot[0] = ((k + 1) % d[5] as i64) as f64;
            let [x1, y1, x2, y2] = rsquares_segment(d[0] as i32, k);
            let rnd = ctx.random();
            let (mut ox, mut oy) = (0.0, 0.0);
            if rnd < d[8] {
                let (xdiff, ydiff) = (x2 - x1, y2 - y1);
                let m = if xdiff == 0.0 { 10000.0 } else { ydiff / xdiff };
                let len = ((xdiff * xdiff) + (ydiff * ydiff)).sqrt();
                let dd = ctx.random() * len;
                let mut xo = dd / (1.0 + m * m).sqrt();
                if x2 < x1 {
                    xo = -xo;
                }
                let mut yo = (m * xo).abs();
                if y2 < y1 {
                    yo = -yo;
                }
                if d[6] != 0.0 {
                    xo += (ctx.random() - 0.5) * d[6];
                    yo += (ctx.random() - 0.5) * d[6];
                }
                (ox, oy) = (x1 + xo, y1 + yo);
            } else if rnd <= d[9] {
                let (mut xo, mut yo) = (0.0, 0.0);
                if d[7] != 0.0 {
                    let ro = ctx.random() * d[7];
                    let ra = ctx.random() * M_2PI;
                    (xo, yo) = (ro * ra.cos(), ro * ra.sin());
                }
                (ox, oy) = (x1 + xo, y1 + yo);
            }
            v.x += amount * ox;
            v.y += amount * oy;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let level = limit_value(ftoi(p[0]), 1, 9);
            let (sl, sp) = ((p[1] as i32) as f64, (p[3] as i32) as f64);
            let sum = sl + sp;
            let (lf, pf) = (sl / sum, sp / sum);
            let nseg = 4 * ((1i64 << (2 * level)) - 1) / 3;
            vec![level as f64, sl, p[2], sp, p[4], nseg as f64, p[2] / 100.0, p[4] / 100.0, lf, lf + pf]
        }),
        // GPU: a random segment per call instead of the cycling order.
        wgsl: "fn rsquares_js_segment(level: i32, k: i32) -> vec4<f32> {
  var c = vec2<f32>(0.5, 0.5);
  var size = 1.0;
  var s = k / 4;
  var depth = level;
  while (s > 0) {
    s -= 1;
    let sub = ((1 << u32(2 * (depth - 1))) - 1) / 3;
    let child = s / sub;
    s = s % sub;
    let h = size / 2.0;
    if (child == 0) { c = c + vec2<f32>(-h, -h); } else if (child == 1) { c = c + vec2<f32>(-h, h); }
    else if (child == 2) { c = c + vec2<f32>(h, -h); } else { c = c + vec2<f32>(h, h); }
    size = size / 2.2;
    depth -= 1;
  }
  let h = size / 2.0;
  let x0 = c.x - h;
  let x1 = c.x + h;
  let y0 = c.y - h;
  let y1 = c.y + h;
  let side = k % 4;
  if (side == 0) { return vec4<f32>(x0, y0, x0, y1); }
  if (side == 1) { return vec4<f32>(x0, y1, x1, y1); }
  if (side == 2) { return vec4<f32>(x1, y1, x1, y0); }
  return vec4<f32>(x1, y0, x0, y0);
}
@body
let nseg = i32(D(5));
let seg = rsquares_js_segment(i32(D(0)), min(i32(rnd() * f32(nseg)), nseg - 1));
let x1 = seg.x;
let y1 = seg.y;
let x2 = seg.z;
let y2 = seg.w;
let rn = rnd();
var o = vec2<f32>(0.0);
if (rn < D(8)) {
  let xdiff = x2 - x1;
  let ydiff = y2 - y1;
  var m = 10000.0;
  if (xdiff != 0.0) { m = ydiff / xdiff; }
  let dd = rnd() * sqrt(xdiff * xdiff + ydiff * ydiff);
  var xo = dd / sqrt(1.0 + m * m);
  if (x2 < x1) { xo = -xo; }
  var yo = abs(m * xo);
  if (y2 < y1) { yo = -yo; }
  if (D(6) != 0.0) {
    xo += (rnd() - 0.5) * D(6);
    yo += (rnd() - 0.5) * D(6);
  }
  o = vec2<f32>(x1 + xo, y1 + yo);
} else if (rn <= D(9)) {
  var off = vec2<f32>(0.0);
  if (D(7) != 0.0) {
    let ro = rnd() * D(7);
    let ra = rnd() * TWO_PI;
    off = vec2<f32>(ro * cos(ra), ro * sin(ra));
  }
  o = vec2<f32>(x1, y1) + off;
}
v.x += AMOUNT * o.x;
v.y += AMOUNT * o.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "ruffle",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("amp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let fa = java_max(d[0].abs(), 0.01);
            let cell = (a.x * fa).floor();
            let lx = a.x * fa - cell;
            let dir = if cell - 2.0 * (cell / 2.0).floor() > 0.5 { 1.0 } else { -1.0 };
            let wave = d[1] * (M_PI * lx).sin() * dir;
            v.x = amount * a.x;
            v.y = amount * (a.y + wave);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let fa = max(abs(D(0)), 0.01);
let cell = floor(a.x * fa);
let lx = a.x * fa - cell;
var dir = -1.0;
if (cell - 2.0 * floor(cell / 2.0) > 0.5) { dir = 1.0; }
v.x = AMOUNT * a.x;
v.y = AMOUNT * (a.y + D(1) * sin(PI * lx) * dir);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "satin",
        params: &[ParamDef::new("freq", 4.0), ParamDef::new("sheen", 0.2)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let f = java_max(d[0].abs(), 0.01);
            let d1 = (a.x + a.y) * 0.7071067811865476;
            let d2 = (a.x - a.y) * 0.7071067811865476;
            v.x += amount * (a.x + d[1] * (f * d1).sin());
            v.y += amount * (a.y + d[1] * (f * d2).cos());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let f = max(abs(D(0)), 0.01);
let d1 = (a.x + a.y) * 0.7071067811865476;
let d2 = (a.x - a.y) * 0.7071067811865476;
v.x += AMOUNT * (a.x + D(1) * sin(f * d1));
v.y += AMOUNT * (a.y + D(1) * cos(f * d2));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sattractor_js",
        params: &[ParamDef::int("m", 10.0, 2.0, 12.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [m]
            let m = d[0];
            let l = (ctx.random() * m) as i32 + 1;
            let an = 2.0 * M_PI * l as f64 / m;
            let (bl, al) = (an.sin(), an.cos());
            let (mut x, mut y) = if ctx.random() < 0.5 {
                (a.x / 2.0 + al, a.y / 2.0 + bl)
            } else {
                (a.x * al + a.y * bl + a.x * a.x * bl, a.y * al - a.x * bl + a.x * a.x * al)
            };
            x /= 2.0;
            y /= 2.0;
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![((p[0] as i32).clamp(2, 12)) as f64]),
        wgsl: "let m = D(0);
let l = f32(i32(rnd() * m) + 1);
let an = 2.0 * PI * l / m;
let al = cos(an);
let bl = sin(an);
var x = a.x * al + a.y * bl + a.x * a.x * bl;
var y = a.y * al - a.x * bl + a.x * a.x * al;
if (rnd() < 0.5) {
  x = a.x / 2.0 + al;
  y = a.y / 2.0 + bl;
}
v.x += x / 2.0 * AMOUNT;
v.y += y / 2.0 * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "schwarz_christoffel",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let s = java_max(d[0].abs(), 0.01);
            let ex = (M_PI * a.x / s).exp();
            v.x += amount * ex * (M_PI * a.y / s).cos();
            v.y += amount * ex * (M_PI * a.y / s).sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let s = max(abs(D(0)), 0.01);
let ex = exp(PI * a.x / s);
v.x += AMOUNT * ex * cos(PI * a.y / s);
v.y += AMOUNT * ex * sin(PI * a.y / s);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "scrambly",
        params: &[ParamDef::int("l", 3.0, 3.0, 25.0), ParamDef::int("seed", 51.0, 0.0, 1000.0), ParamDef::int("byrows", 0.0, 0.0, 1.0), ParamDef::new("cellsize", 0.1)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [l, seed, byrows, cellsize, current_l, cella, cellainv, mzcella, permutation...]
            let cl = d[4] as i32;
            let (mut vx, mut vy) = ((a.x + d[7]) * d[6], (a.y + d[7]) * d[6]);
            let (ix, iy) = (vx.floor() as i32, vy.floor() as i32);
            if cl <= 0 || ix < 0 || ix >= cl || iy < 0 || iy >= cl {
                v.x += amount * a.x;
                v.y += amount * a.y;
            } else {
                vx -= ix as f64;
                vy -= iy as f64;
                let swp = d[8 + (ix + cl * iy) as usize] as i32;
                vx += (swp % cl) as f64;
                vy += (swp / cl) as f64;
                vx = vx * d[5] - d[7];
                vy = vy * d[5] - d[7];
                v.x += amount * vx;
                v.y += amount * vy;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let (l, seed, byrows) = (ftoi(p[0]), ftoi(p[1]), ftoi(p[2]));
            let cellsize = limit_value(p[3], 1.0e-20, 1.0e20);
            let ll = (l * l.signum()).clamp(3, 25);
            let ll2 = ll * ll;
            let mut mx = vec![0i32; 626];
            if (0..=50).contains(&seed) {
                for j in 0..ll2 {
                    mx[j as usize] = (seed + j + 1) % ll2;
                }
            } else {
                for j in 0..ll2 {
                    mx[j as usize] = j;
                }
                if byrows == 0 {
                    scrambly_randflip(0, ll2, seed, &mut mx);
                } else {
                    for j in 0..ll {
                        scrambly_randflip(ll * j, ll * (1 + j), seed.wrapping_add(j), &mut mx);
                    }
                }
            }
            let cella = cellsize / ll as f64;
            let mut d = vec![l as f64, seed as f64, byrows as f64, cellsize, ll as f64, cella, 1.0 / cella, 0.5 * cellsize];
            d.extend(mx[..ll2 as usize].iter().map(|&m| m as f64));
            d
        }),
        wgsl: "let cl = i32(D(4));
var vx = (a.x + D(7)) * D(6);
var vy = (a.y + D(7)) * D(6);
let ix = i32(floor(vx));
let iy = i32(floor(vy));
if (cl <= 0 || ix < 0 || ix >= cl || iy < 0 || iy >= cl) {
  v.x += AMOUNT * a.x;
  v.y += AMOUNT * a.y;
} else {
  vx -= f32(ix);
  vy -= f32(iy);
  let swp = i32(prm[base + 8u + u32(ix + cl * iy)]);
  vx += f32(swp % cl);
  vy += f32(swp / cl);
  v.x += AMOUNT * (vx * D(5) - D(7));
  v.y += AMOUNT * (vy * D(5) - D(7));
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "screw",
        params: &[ParamDef::new("pitch", 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let nt = a.y.atan2(a.x) + r * d[0];
            v.x += amount * r * nt.cos();
            v.y += amount * r * nt.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let nt = atan2(a.y, a.x) + r * D(0);
v.x += AMOUNT * r * cos(nt);
v.y += AMOUNT * r * sin(nt);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "scry2",
        params: &[ParamDef::int("sides", 4.0, 1.0, 50.0), ParamDef::range("star", 0.0, -1.0, 1.0), ParamDef::range("circle", 0.0, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [sides, star, circle, sina, cosa, sins, coss, sinc, cosc]
            let (mut xrt, mut yrt) = (a.x, a.y);
            let mut r2 = xrt * d[6] + yrt.abs() * d[5];
            let circle = (sqr(xrt) + sqr(yrt)).sqrt();
            let n = (d[0] as i32 - 1).max(0);
            for _ in 0..n {
                (xrt, yrt) = (xrt * d[4] - yrt * d[3], xrt * d[3] + yrt * d[4]);
                r2 = java_max(r2, xrt * d[6] + yrt.abs() * d[5]);
            }
            r2 = r2 * d[8] + circle * d[7];
            let r1 = r2;
            r2 = if n > 1 { sqr(r2) } else { r2.abs() * r2 };
            let dd = r1 * (r2 + 1.0 / amount);
            if dd == 0.0 {
                return;
            }
            let r = 1.0 / dd;
            v.x += a.x * r;
            v.y += a.y * r;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let sides = limit_value(ftoi(p[0]), 1, 50) as f64;
            let (star, circle) = (limit_value(p[1], -1.0, 1.0), limit_value(p[2], -1.0, 1.0));
            let a = M_2PI / sides;
            let s = -M_PI_2 * star;
            let c = M_PI_2 * circle;
            vec![sides, star, circle, a.sin(), a.cos(), s.sin(), s.cos(), c.sin(), c.cos()]
        }),
        wgsl: "var xrt = a.x;
var yrt = a.y;
var r2 = xrt * D(6) + abs(yrt) * D(5);
let circle = sqrt(xrt * xrt + yrt * yrt);
let n = max(i32(D(0)) - 1, 0);
for (var i = 0; i < n; i++) {
  let swp = xrt * D(4) - yrt * D(3);
  yrt = xrt * D(3) + yrt * D(4);
  xrt = swp;
  r2 = max(r2, xrt * D(6) + abs(yrt) * D(5));
}
r2 = r2 * D(8) + circle * D(7);
let r1 = r2;
if (n > 1) { r2 = r2 * r2; } else { r2 = abs(r2) * r2; }
let dd = r1 * (r2 + 1.0 / AMOUNT);
if (dd != 0.0) {
  v.x += a.x / dd;
  v.y += a.y / dd;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    simple(
        "scry_3D",
        true,
        |_, _, a, v, amount| {
            let inv = 1.0 / (amount + SMALL_EPSILON);
            let t = sqr(a.x) + sqr(a.y) + sqr(a.z);
            let r = 1.0 / (t.sqrt() * (t + inv));
            v.x += a.x * r;
            v.y += a.y * r;
            let fz = if a.z != 0.0 { a.z } else { a.y.atan2(a.x) };
            v.z += fz * r;
        },
        "let inv = 1.0 / (AMOUNT + EPS);
let t = a.x * a.x + a.y * a.y + a.z * a.z;
let r = 1.0 / (sqrt(t) * (t + inv));
v.x += a.x * r;
v.y += a.y * r;
var fz = a.z;
if (fz == 0.0) { fz = atan2(a.y, a.x); }
v.z += fz * r;",
    ),
    VarDef {
        name: "seashell3D",
        params: &[ParamDef::new("final_radius", 0.25), ParamDef::new("height", 3.5), ParamDef::new("inner_radius", 0.4), ParamDef::int("nSpirals", 3.0, 1.0, 20.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            let t = M_2PI * ctx.random();
            let s = M_2PI * ctx.random();
            let n = (d[3] as i32) as f64;
            let f = d[0] * (1.0 - t / M_2PI);
            let x = f * (n * t).cos() * (1.0 + s.cos()) + d[2] * (n * t).cos();
            let y = f * (n * t).sin() * (1.0 + s.cos()) + d[2] * (n * t).sin();
            let z = d[1] * t / M_2PI + f * s.sin();
            v.x += x * amount;
            v.y += y * amount;
            v.z += z * amount;
        },
        prepare: None,
        wgsl: "let t = TWO_PI * rnd();
let s = TWO_PI * rnd();
let n = f32(i32(D(3)));
let f = D(0) * (1.0 - t / TWO_PI);
v.x += (f * cos(n * t) * (1.0 + cos(s)) + D(2) * cos(n * t)) * AMOUNT;
v.y += (f * sin(n * t) * (1.0 + cos(s)) + D(2) * sin(n * t)) * AMOUNT;
v.z += (D(1) * t / TWO_PI + f * sin(s)) * AMOUNT;",
    },
    VarDef {
        name: "sec2_bs",
        params: &[ParamDef::new("x1", 1.25), ParamDef::new("x2", 0.75), ParamDef::new("y1", 1.5), ParamDef::new("y2", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (s, c) = ((a.x * d[0]).sin(), (a.x * d[1]).cos());
            let (sh, ch) = ((a.y * d[2]).sinh(), (a.y * d[3]).cosh());
            let dd = (2.0 * a.x).cos() + (2.0 * a.y).cosh();
            if dd == 0.0 {
                return;
            }
            let den = 2.0 / dd;
            v.x += amount * den * c * ch;
            v.y += amount * den * s * sh;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dd = cos(2.0 * a.x) + cosh(2.0 * a.y);
if (dd != 0.0) {
  let den = 2.0 / dd;
  v.x += AMOUNT * den * cos(a.x * D(1)) * cosh(a.y * D(3));
  v.y += AMOUNT * den * sin(a.x * D(0)) * sinh(a.y * D(2));
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    VarDef {
        name: "secant",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |_, ctx, a, v, amount| {
            let cr = (amount * (a.x * a.x + a.y * a.y).sqrt()).cos();
            if cr.abs() > 1.0e-6 {
                v.x += amount * a.x;
                v.y += amount / cr;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let cr = cos(AMOUNT * sqrt(a.x * a.x + a.y * a.y));
if (abs(cr) > 1.0e-6) {
  v.x += AMOUNT * a.x;
  v.y += AMOUNT / cr;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sech2_bs",
        params: &[ParamDef::new("x1", 1.25), ParamDef::new("x2", 0.75), ParamDef::new("y1", 1.5), ParamDef::new("y2", 0.75)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (s, c) = ((a.y * d[2]).sin(), (a.y * d[3]).cos());
            let (sh, ch) = ((a.x * d[0]).sinh(), (a.x * d[1]).cosh());
            let dd = (2.0 * a.y).cos() + (2.0 * a.x).cosh();
            if dd == 0.0 {
                return;
            }
            let den = 2.0 / dd;
            v.x += amount * den * c * ch;
            v.y -= amount * den * s * sh;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dd = cos(2.0 * a.y) + cosh(2.0 * a.x);
if (dd != 0.0) {
  let den = 2.0 / dd;
  v.x += AMOUNT * den * cos(a.y * D(3)) * cosh(a.x * D(1));
  v.y -= AMOUNT * den * sin(a.y * D(2)) * sinh(a.x * D(0));
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    simple(
        "sechq",
        true,
        |_, _, a, v, amount| {
            let av = a.y.hypot(a.z);
            let (s, c) = (av.sin(), av.cos());
            let (sh, ch) = (a.x.sinh(), a.x.cosh());
            let ni = amount / (sqr(a.x) + sqr(a.y) + sqr(a.z));
            let cc = ni * sh * s / av;
            v.x += ch * c * ni;
            v.y -= cc * a.y;
            v.z -= cc * a.z;
        },
        "let av = length(vec2<f32>(a.y, a.z));
let ni = AMOUNT / (a.x * a.x + a.y * a.y + a.z * a.z);
let cc = ni * sinh(a.x) * sin(av) / av;
v.x += cosh(a.x) * cos(av) * ni;
v.y -= cc * a.y;
v.z -= cc * a.z;",
    ),
    simple(
        "secq",
        true,
        |_, _, a, v, amount| {
            let av = a.y.hypot(a.z);
            let (s, c) = ((-a.x).sin(), (-a.x).cos());
            let (sh, ch) = (av.sinh(), av.cosh());
            let ni = amount / (sqr(a.x) + sqr(a.y) + sqr(a.z));
            let cc = ni * s * sh / av;
            v.x += c * ch * ni;
            v.y -= cc * a.y;
            v.z -= cc * a.z;
        },
        "let av = length(vec2<f32>(a.y, a.z));
let ni = AMOUNT / (a.x * a.x + a.y * a.y + a.z * a.z);
let cc = ni * sin(-a.x) * sinh(av) / av;
v.x += cos(-a.x) * cosh(av) * ni;
v.y -= cc * a.y;
v.z -= cc * a.z;",
    ),
    VarDef {
        name: "shape_warp",
        params: &[
            ParamDef::int("shape", 0.0, 0.0, 9.0),
            ParamDef::new("size", 1.0),
            ParamDef::new("aspect", 1.0),
            ParamDef::new("centerX", 0.0),
            ParamDef::new("centerY", 0.0),
            ParamDef::new("innerRadius", 0.0),
            ParamDef::new("outerRadius", 1.0),
            ParamDef::int("warpMode", 0.0, 0.0, 5.0),
            ParamDef::new("warpAmount", M_PI),
            ParamDef::new("warpAmountX", 1.0),
            ParamDef::new("warpAmountY", 1.0),
            ParamDef::new("warpCenterX", 0.0),
            ParamDef::new("warpCenterY", 0.0),
            ParamDef::new("warpCurve", 1.0),
            ParamDef::int("warpInvert", 0.0, 0.0, 1.0),
            ParamDef::int("flowerPetals", 5.0, 2.0, 20.0),
            ParamDef::int("starPoints", 5.0, 3.0, 20.0),
            ParamDef::range("starDepth", 0.5, 0.0, 1.0),
            ParamDef::new("cloudAmplitude", 0.2),
            ParamDef::new("cloudFrequency", 5.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [shape, size, aspect, centerX, centerY, innerRadius, outerRadius, warpMode, warpAmount,
            //      warpAmountX, warpAmountY, warpCenterX, warpCenterY, warpCurve, warpInvert, flowerPetals,
            //      starPoints, starDepth, cloudAmplitude, cloudFrequency]
            let shape = d[0] as i32;
            let size = d[1];
            let aspect = if shape == 1 || shape == 2 || shape == 3 { d[2] } else { 1.0 };
            let inner = java_max(0.0, d[5]);
            let outer = java_max(inner + 1e-9, d[6]);
            let curve = java_max(1e-6, d[13]);
            let (rx, ry) = (a.x - d[3], a.y - d[4]);
            let hw = java_max(1e-9, size * aspect * 0.5);
            let hh = java_max(1e-9, size * 0.5);
            let rr = java_max(1e-9, size * 0.5);
            let radius = (rx * rx + ry * ry).sqrt();
            let mut angle = 0.0;
            if radius > 1e-9 {
                angle = ry.atan2(rx);
                while angle < 0.0 {
                    angle += M_2PI;
                }
            }
            let polygon = |n: f64, half: f64| {
                if radius < 1e-9 {
                    return 0.0;
                }
                let sector = M_2PI / n;
                let diff = fmod(angle, sector) - half;
                let boundary = rr * half.cos() / java_max(1e-9, diff.cos().abs());
                radius / java_max(1e-9, boundary)
            };
            let dist = match shape {
                2 => ((rx / hw) * (rx / hw) + (ry / hh) * (ry / hh)).sqrt(),
                3 => (rx / hw).abs() + (ry / hh).abs(),
                4 => polygon(3.0, M_PI / 3.0),
                5 => polygon(5.0, M_PI / 5.0),
                6 => polygon(6.0, M_PI / 6.0),
                7 => {
                    if radius < 1e-9 {
                        0.0
                    } else {
                        radius / java_max(1e-9, rr * (d[15] * angle).cos().abs())
                    }
                }
                8 => {
                    if radius < 1e-9 {
                        0.0
                    } else {
                        let k = d[16];
                        let r_inner = rr * d[17];
                        let apv = M_PI / k;
                        let si = (angle / apv).floor() as i32;
                        let t = (angle - si as f64 * apv) / apv;
                        let tn = 2.0 * (t - 0.5).abs();
                        let b = if si % 2 == 0 { r_inner + (rr - r_inner) * (1.0 - tn) } else { r_inner + (rr - r_inner) * tn };
                        radius / java_max(1e-9, b)
                    }
                }
                9 => {
                    if radius < 1e-9 {
                        0.0
                    } else {
                        let f = d[19];
                        let noise = 0.6 * (f * angle).sin() + 0.3 * (2.1 * f * angle + 1.23).sin() + 0.1 * (4.3 * f * angle + 4.56).sin();
                        let b = java_max(rr * 0.1, rr * (1.0 + d[18] * noise));
                        radius / b
                    }
                }
                _ => java_max((rx / hw).abs(), (ry / hh).abs()),
            };
            let range = outer - inner;
            let mut wf = if dist <= inner {
                0.0
            } else if dist >= outer {
                1.0
            } else if range > 1e-9 {
                (dist - inner) / range
            } else {
                1.0
            };
            wf = wf.powf(curve);
            if d[14] != 0.0 {
                wf = 1.0 - wf;
            }
            let (ew, ewx, ewy) = (wf * d[8], wf * d[9], wf * d[10]);
            let (wx, wy) = (a.x - d[11], a.y - d[12]);
            let (mut ox, mut oy) = (wx, wy);
            match d[7] as i32 {
                0 => {
                    let (s, c) = (ew.sin(), ew.cos());
                    (ox, oy) = (wx * c - wy * s, wx * s + wy * c);
                }
                1 => {
                    let f = java_max(1e-9, 1.0 + ew);
                    (ox, oy) = (wx * f, wy * f);
                }
                2 => {
                    let an = wy.atan2(wx) + ew;
                    let r = (wx * wx + wy * wy).sqrt();
                    (ox, oy) = (r * an.cos(), r * an.sin());
                }
                3 => (ox, oy) = (wx * java_max(1e-9, 1.0 + ewx), wy * java_max(1e-9, 1.0 + ewy)),
                4 => {
                    let an = wy.atan2(wx);
                    let r = (wx * wx + wy * wy).sqrt();
                    if r > 1e-9 {
                        let p = java_max(-5.0, java_min(5.0, 1.0 - ew));
                        if (p - 1.0).abs() > 1e-9 {
                            let nr = r.powf(p);
                            (ox, oy) = (nr * an.cos(), nr * an.sin());
                        }
                    }
                }
                5 => (ox, oy) = (wx + ewx * wy, wy + ewy * wx),
                _ => {}
            }
            v.x = (ox + d[11]) * amount;
            v.y = (oy + d[12]) * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = (p[0] as i32).clamp(0, 9) as f64;
            d[1] = java_max(1e-9, p[1]);
            d[2] = java_max(1e-9, p[2]);
            d[5] = java_max(0.0, p[5]);
            d[6] = java_max(d[5], p[6]);
            d[7] = (p[7] as i32).clamp(0, 5) as f64;
            d[13] = java_max(1e-6, p[13]);
            d[14] = if p[14] >= 0.5 { 1.0 } else { 0.0 };
            d[15] = (p[15] as i32).max(2) as f64;
            d[16] = (p[16] as i32).max(3) as f64;
            d[17] = java_max(0.0, java_min(1.0, p[17]));
            d[18] = java_max(0.0, p[18]);
            d[19] = java_max(1e-6, p[19]);
            d
        }),
        wgsl: "fn shape_warp_polygon(radius: f32, angle: f32, rr: f32, n: f32, half: f32) -> f32 {
  if (radius < 1e-9) { return 0.0; }
  let diff = fmod(angle, TWO_PI / n) - half;
  return radius / max(1e-9, rr * cos(half) / max(1e-9, abs(cos(diff))));
}
@body
let shape = i32(D(0));
let size = D(1);
var aspect = 1.0;
if (shape == 1 || shape == 2 || shape == 3) { aspect = D(2); }
let inner = max(0.0, D(5));
let outer = max(inner + 1e-9, D(6));
let rel = vec2<f32>(a.x - D(3), a.y - D(4));
let hw = max(1e-9, size * aspect * 0.5);
let hh = max(1e-9, size * 0.5);
let rr = max(1e-9, size * 0.5);
let radius = length(rel);
var angle = 0.0;
if (radius > 1e-9) {
  angle = atan2(rel.y, rel.x);
  if (angle < 0.0) { angle += TWO_PI; }
}
var dist = max(abs(rel.x / hw), abs(rel.y / hh));
if (shape == 2) {
  dist = length(vec2<f32>(rel.x / hw, rel.y / hh));
} else if (shape == 3) {
  dist = abs(rel.x / hw) + abs(rel.y / hh);
} else if (shape == 4) {
  dist = shape_warp_polygon(radius, angle, rr, 3.0, PI / 3.0);
} else if (shape == 5) {
  dist = shape_warp_polygon(radius, angle, rr, 5.0, PI / 5.0);
} else if (shape == 6) {
  dist = shape_warp_polygon(radius, angle, rr, 6.0, PI / 6.0);
} else if (shape == 7) {
  dist = 0.0;
  if (radius >= 1e-9) { dist = radius / max(1e-9, rr * abs(cos(D(15) * angle))); }
} else if (shape == 8) {
  dist = 0.0;
  if (radius >= 1e-9) {
    let r_inner = rr * D(17);
    let apv = PI / D(16);
    let si = i32(floor(angle / apv));
    let tn = 2.0 * abs((angle - f32(si) * apv) / apv - 0.5);
    var b = r_inner + (rr - r_inner) * tn;
    if (si % 2 == 0) { b = r_inner + (rr - r_inner) * (1.0 - tn); }
    dist = radius / max(1e-9, b);
  }
} else if (shape == 9) {
  dist = 0.0;
  if (radius >= 1e-9) {
    let f = D(19);
    let noise = 0.6 * sin(f * angle) + 0.3 * sin(2.1 * f * angle + 1.23) + 0.1 * sin(4.3 * f * angle + 4.56);
    dist = radius / max(rr * 0.1, rr * (1.0 + D(18) * noise));
  }
}
var wf = 1.0;
if (dist <= inner) { wf = 0.0; } else if (dist < outer && outer - inner > 1e-9) { wf = (dist - inner) / (outer - inner); }
wf = pow(wf, max(1e-6, D(13)));
if (D(14) != 0.0) { wf = 1.0 - wf; }
let ew = wf * D(8);
let ewx = wf * D(9);
let ewy = wf * D(10);
let w = vec2<f32>(a.x - D(11), a.y - D(12));
var o = w;
let mode = i32(D(7));
if (mode == 0) {
  o = vec2<f32>(w.x * cos(ew) - w.y * sin(ew), w.x * sin(ew) + w.y * cos(ew));
} else if (mode == 1) {
  o = w * max(1e-9, 1.0 + ew);
} else if (mode == 2) {
  let an = atan2(w.y, w.x) + ew;
  o = length(w) * vec2<f32>(cos(an), sin(an));
} else if (mode == 3) {
  o = vec2<f32>(w.x * max(1e-9, 1.0 + ewx), w.y * max(1e-9, 1.0 + ewy));
} else if (mode == 4) {
  let r = length(w);
  if (r > 1e-9) {
    let p = clamp(1.0 - ew, -5.0, 5.0);
    if (abs(p - 1.0) > 1e-9) {
      let an = atan2(w.y, w.x);
      o = pow(r, p) * vec2<f32>(cos(an), sin(an));
    }
  }
} else if (mode == 5) {
  o = vec2<f32>(w.x + ewx * w.y, w.y + ewy * w.x);
}
v.x = (o.x + D(11)) * AMOUNT;
v.y = (o.y + D(12)) * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "shift",
        params: &[ParamDef::new("shift_x", 0.10), ParamDef::new("shift_y", 0.06), ParamDef::range("angle", 12.25, -180.0, 180.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let ang = d[2] / 180.0 * M_PI;
            let (sn, cs) = (ang.sin(), ang.cos());
            v.x += amount * (a.x + cs * d[0] - sn * d[1]);
            v.y += amount * (a.y - cs * d[1] - sn * d[0]);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ang = D(2) / 180.0 * PI;
let sn = sin(ang);
let cs = cos(ang);
v.x += AMOUNT * (a.x + cs * D(0) - sn * D(1));
v.y += AMOUNT * (a.y - cs * D(1) - sn * D(0));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "shredded",
        params: &[
            ParamDef::new("x1", 1.0),
            ParamDef::new("x2", 3.0),
            ParamDef::new("x3", 1.0),
            ParamDef::new("x_blur_amount", 0.1),
            ParamDef::new("y1", 1.0),
            ParamDef::new("y2", 3.0),
            ParamDef::new("y3", 1.0),
            ParamDef::new("y_blur_amount", 0.1),
            ParamDef::new("z1", 1.0),
            ParamDef::new("z2", 3.0),
            ParamDef::new("z3", 1.0),
            ParamDef::new("z_blur_amount", 0.1),
            ParamDef::int("type", 0.0, 0.0, 2.0),
            ParamDef::int("blur", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [x1, x2, x3, x_blur, y1, y2, y3, y_blur, z1, z2, z3, z_blur, type, blur]
            let ty = d[12] as i32;
            let sx = (d[0] * (d[1] * a.x).floor()).sin();
            let cy = (d[4] * (d[5] * a.y).floor()).cos();
            let sz = (d[8] * (d[9] * a.z).floor()).sin();
            let (fx, fy, fz) = match ty {
                0 => (sx + (d[0] * (d[1] * a.y).floor()).cos(), cy + (d[4] * (d[5] * a.x).floor()).sin(), sz + (d[8] * (d[9] * a.z).floor()).cos()),
                _ => (sx, cy, sz),
            };
            // multipliers of the x / y terms and of the z blur term
            let (mx, my, mz) = match ty {
                2 => (a.x, a.y, 1.0),
                1 => (a.y, a.x, 1.0),
                _ => (a.y, a.x, a.z),
            };
            if d[13] != 0.0 {
                v.x += amount * d[2] * fx + (d[3] * (M_2PI * ctx.random()).sin()) * mx;
                v.y += amount * d[6] * fy + (d[7] * (M_2PI * ctx.random()).sin()) * my;
                v.z += amount * d[10] * fz + (d[11] * (M_2PI * ctx.random()).sin()) * mz;
            } else {
                v.x += amount * d[2] * fx * mx;
                v.y += amount * d[6] * fy * my;
                v.z += amount * d[10] * fz * a.z;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[12] = limit_value(p[12], 0.0, 2.0) as i32 as f64;
            d[13] = limit_value(p[13], 0.0, 1.0) as i32 as f64;
            d
        }),
        wgsl: "let ty = i32(D(12));
let sx = sin(D(0) * floor(D(1) * a.x));
let cy = cos(D(4) * floor(D(5) * a.y));
let sz = sin(D(8) * floor(D(9) * a.z));
var f = vec3<f32>(sx, cy, sz);
var m = vec3<f32>(a.y, a.x, a.z);
if (ty == 0) {
  f += vec3<f32>(cos(D(0) * floor(D(1) * a.y)), sin(D(4) * floor(D(5) * a.x)), cos(D(8) * floor(D(9) * a.z)));
} else if (ty == 1) {
  m = vec3<f32>(a.y, a.x, 1.0);
} else {
  m = vec3<f32>(a.x, a.y, 1.0);
}
if (D(13) != 0.0) {
  v.x += AMOUNT * D(2) * f.x + (D(3) * sin(TWO_PI * rnd())) * m.x;
  v.y += AMOUNT * D(6) * f.y + (D(7) * sin(TWO_PI * rnd())) * m.y;
  v.z += AMOUNT * D(10) * f.z + (D(11) * sin(TWO_PI * rnd())) * m.z;
} else {
  v.x += AMOUNT * D(2) * f.x * m.x;
  v.y += AMOUNT * D(6) * f.y * m.y;
  v.z += AMOUNT * D(10) * f.z * a.z;
}",
    },
    VarDef {
        name: "shredlin",
        params: &[ParamDef::new("xdistance", 1.0), ParamDef::range("xwidth", 0.5, -1.0, 1.0), ParamDef::new("ydistance", 1.0), ParamDef::range("ywidth", 0.5, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (sxd, sxw, syd, syw) = (d[0], d[1], d[2], d[3]);
            let xpos = if a.x < 0.0 { 1.0 } else { 0.0 };
            let ypos = if a.y < 0.0 { 1.0 } else { 0.0 };
            let (xr, yr) = (a.x / sxd, a.y / syd);
            let (xi, yi) = ((xr as i32) as f64, (yr as i32) as f64);
            v.x = amount * sxd * ((xr - xi) * sxw + xi + (0.5 - xpos) * (1.0 - sxw));
            v.y = amount * syd * ((yr - yi) * syw + yi + (0.5 - ypos) * (1.0 - syw));
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], limit_value(p[1], -1.0, 1.0), p[2], limit_value(p[3], -1.0, 1.0)]),
        wgsl: "var xpos = 0.0;
if (a.x < 0.0) { xpos = 1.0; }
var ypos = 0.0;
if (a.y < 0.0) { ypos = 1.0; }
let xr = a.x / D(0);
let yr = a.y / D(2);
let xi = trunc(xr);
let yi = trunc(yr);
v.x = AMOUNT * D(0) * ((xr - xi) * D(1) + xi + (0.5 - xpos) * (1.0 - D(1)));
v.y = AMOUNT * D(2) * ((yr - yi) * D(3) + yi + (0.5 - ypos) * (1.0 - D(3)));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "shredrad",
        params: &[ParamDef::new("n", 4.0), ParamDef::range("width", 0.5, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [n, width, alpha]
            let (sa, sw) = (d[2], d[1]);
            let ang = a.atan_yx();
            let rad = a.rad();
            let xang = (ang + 3.0 * M_PI + sa / 2.0) / sa;
            let xi = (xang as i32) as f64;
            let zang = ((xang - xi) * sw + xi) * sa - M_PI - sa / 2.0 * sw;
            v.x += amount * rad * zang.cos();
            v.y += amount * rad * zang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let n = if p[0] == 0.0 { EPSILON } else { p[0] };
            vec![n, limit_value(p[1], -1.0, 1.0), M_2PI / n]
        }),
        wgsl: "let sa = D(2);
let sw = D(1);
let rad = sqrt(a.x * a.x + a.y * a.y);
let xang = (atan2(a.y, a.x) + 3.0 * PI + sa / 2.0) / sa;
let xi = trunc(xang);
let zang = ((xang - xi) * sw + xi) * sa - PI - sa / 2.0 * sw;
v.x += AMOUNT * rad * cos(zang);
v.y += AMOUNT * rad * sin(zang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "siercarpet_js",
        params: &[ParamDef::int("m", 3.0, 3.0, 12.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [m]
            let m = d[0];
            let l = 1 + (2.0 * m * ctx.random()) as i32;
            let (al, bl) = if l % 2 == 1 {
                let an = 2.0 * M_PI * ((l + 1) / 2) as f64 / m;
                (an.cos(), an.sin())
            } else {
                let i = (l / 2) as f64;
                let (a0, a1) = (2.0 * M_PI * i / m, 2.0 * M_PI * (i - 1.0) / m);
                ((a0.cos() + a1.cos()) / 2.0, (a0.sin() + a1.sin()) / 2.0)
            };
            v.x += (a.x / 3.0 + (al - bl) / 2.0f64.sqrt()) * amount;
            v.y += (a.y / 3.0 + (al + bl) / 2.0f64.sqrt()) * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![((p[0] as i32).clamp(3, 12)) as f64]),
        wgsl: "let m = D(0);
let l = 1 + i32(2.0 * m * rnd());
var ab = vec2<f32>(0.0);
if (l % 2 == 1) {
  let an = 2.0 * PI * f32((l + 1) / 2) / m;
  ab = vec2<f32>(cos(an), sin(an));
} else {
  let i = f32(l / 2);
  let a0 = 2.0 * PI * i / m;
  let a1 = 2.0 * PI * (i - 1.0) / m;
  ab = vec2<f32>(cos(a0) + cos(a1), sin(a0) + sin(a1)) / 2.0;
}
v.x += (a.x / 3.0 + (ab.x - ab.y) / sqrt(2.0)) * AMOUNT;
v.y += (a.y / 3.0 + (ab.x + ab.y) / sqrt(2.0)) * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sigmoid",
        params: &[ParamDef::new("shiftx", 1.0), ParamDef::new("shifty", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let adjust = |s: f64| {
                if s < 1.0 && s > -1.0 {
                    if s == 0.0 { (SMALL_EPSILON, 1.0) } else { (1.0 / s, if s < 0.0 { -1.0 } else { 1.0 }) }
                } else {
                    (s, 1.0)
                }
            };
            let (sx, ax) = adjust(d[0]);
            let (sy, ay) = adjust(d[1]);
            let vv = amount.abs();
            let c0 = ax / (1.0 + (sx * -5.0 * a.x).exp());
            let c1 = ay / (1.0 + (sy * -5.0 * a.y).exp());
            v.x += vv * (2.0 * (c0 - 0.5));
            v.y += vv * (2.0 * (c1 - 0.5));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "var sx = D(0);
var sy = D(1);
var ax = 1.0;
var ay = 1.0;
if (sx < 1.0 && sx > -1.0) {
  if (sx == 0.0) { sx = EPS; } else { ax = sign(sx); sx = 1.0 / sx; }
}
if (sy < 1.0 && sy > -1.0) {
  if (sy == 0.0) { sy = EPS; } else { ay = sign(sy); sy = 1.0 / sy; }
}
let vv = abs(AMOUNT);
let c0 = ax / (1.0 + exp(sx * -5.0 * a.x));
let c1 = ay / (1.0 + exp(sy * -5.0 * a.y));
v.x += vv * (2.0 * (c0 - 0.5));
v.y += vv * (2.0 * (c1 - 0.5));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "simplex_warp",
        params: &[ParamDef::new("freq", 2.0), ParamDef::new("amp", 0.4)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x, y) = (d[0] * a.x, d[0] * a.y);
            let v1 = (x * 1.3 + y * 0.7 + (x * 0.9 - y * 1.1).sin() * 2.0).sin();
            let v2 = (y * 1.1 + x * 0.8 + (y * 1.2 + x * 0.6).sin() * 2.0).sin();
            v.x += amount * d[1] * v1;
            v.y += amount * d[1] * v2;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = D(0) * a.x;
let y = D(0) * a.y;
v.x += AMOUNT * D(1) * sin(x * 1.3 + y * 0.7 + sin(x * 0.9 - y * 1.1) * 2.0);
v.y += AMOUNT * D(1) * sin(y * 1.1 + x * 0.8 + sin(y * 1.2 + x * 0.6) * 2.0);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sin2_bs",
        params: &[ParamDef::new("x1", 1.0), ParamDef::new("x2", 1.0), ParamDef::new("y1", 1.0), ParamDef::new("y2", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * (a.x * d[0]).sin() * (a.y * d[3]).cosh();
            v.y += amount * (a.x * d[1]).cos() * (a.y * d[2]).sinh();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * sin(a.x * D(0)) * cosh(a.y * D(3));
v.y += AMOUNT * cos(a.x * D(1)) * sinh(a.y * D(2));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sineblur",
        params: &[ParamDef::new("power", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let power = if d[0] < 0.0 { 0.0 } else { d[0] };
            let ang = ctx.random() * M_2PI;
            let r = amount
                * if power == 1.0 {
                    (ctx.random() * 2.0 - 1.0).acos() / M_PI
                } else {
                    ((ctx.random().ln() * power).exp() * 2.0 - 1.0).acos() / M_PI
                };
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let power = max(D(0), 0.0);
let ang = rnd() * TWO_PI;
var t = rnd();
if (power != 1.0) { t = exp(log(t) * power); }
let r = AMOUNT * acos(clamp(t * 2.0 - 1.0, -1.0, 1.0)) / PI;
v.x += r * cos(ang);
v.y += r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sinh2_bs",
        params: &[ParamDef::new("x1", 1.0), ParamDef::new("x2", 1.0), ParamDef::new("y1", 1.0), ParamDef::new("y2", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * (a.x * d[0]).sinh() * (a.y * d[3]).cos();
            v.y += amount * (a.x * d[1]).cosh() * (a.y * d[2]).sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * sinh(a.x * D(0)) * cos(a.y * D(3));
v.y += AMOUNT * cosh(a.x * D(1)) * sin(a.y * D(2));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "sinhq",
        true,
        |_, _, a, v, amount| {
            let av = a.y.hypot(a.z);
            let cc = amount * a.x.cosh() * av.sin() / av;
            v.x += amount * a.x.sinh() * av.cos();
            v.y += cc * a.y;
            v.z += cc * a.z;
        },
        "let av = length(vec2<f32>(a.y, a.z));
let cc = AMOUNT * cosh(a.x) * sin(av) / av;
v.x += AMOUNT * sinh(a.x) * cos(av);
v.y += cc * a.y;
v.z += cc * a.z;",
    ),
    simple(
        "sinq",
        true,
        |_, _, a, v, amount| {
            let av = a.y.hypot(a.z);
            let cc = amount * a.x.cos() * av.sinh() / av;
            v.x += amount * a.x.sin() * av.cosh();
            v.y += cc * a.y;
            v.z += cc * a.z;
        },
        "let av = length(vec2<f32>(a.y, a.z));
let cc = AMOUNT * cos(a.x) * sinh(av) / av;
v.x += AMOUNT * sin(a.x) * cosh(av);
v.y += cc * a.y;
v.z += cc * a.z;",
    ),
    VarDef {
        name: "sintrange",
        params: &[ParamDef::new("w", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let w = d[0];
            let t = (sqr(a.x) + sqr(a.y)) * w;
            v.x = amount * a.x.sin() * (a.x * a.x + w - t);
            v.y = amount * a.y.sin() * (a.y * a.y + w - t);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let t = (a.x * a.x + a.y * a.y) * D(0);
v.x = AMOUNT * sin(a.x) * (a.x * a.x + D(0) - t);
v.y = AMOUNT * sin(a.y) * (a.y * a.y + D(0) - t);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sinusgrid",
        params: &[ParamDef::new("ampx", 0.5), ParamDef::new("ampy", 0.6), ParamDef::new("freqx", 1.2), ParamDef::new("freqy", 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            // d = [ampx, ampy, freqx, freqy, fx, fy]
            let sx = -(a.x * d[4]).cos();
            let sy = -(a.y * d[5]).cos();
            v.x += amount * (a.x + d[0] * (sx - a.x));
            v.y += amount * (a.y + d[1] * (sy - a.y));
            v.z += amount * a.z;
        },
        prepare: Some(|p, _| {
            let nz = |f: f64| if f == 0.0 { EPSILON } else { f };
            vec![p[0], p[1], p[2], p[3], nz(p[2] * M_2PI), nz(p[3] * M_2PI)]
        }),
        wgsl: "let sx = -cos(a.x * D(4));
let sy = -cos(a.y * D(5));
v.x += AMOUNT * (a.x + D(0) * (sx - a.x));
v.y += AMOUNT * (a.y + D(1) * (sy - a.y));
v.z += AMOUNT * a.z;",
    },
    simple(
        "sinusoidal3d",
        true,
        |_, _, a, v, amount| {
            v.x += amount * a.x.sin();
            v.y += amount * a.y.sin();
            v.z += amount * ((a.x * a.x).atan2(a.y * a.y) * a.z.cos());
        },
        "v.x += AMOUNT * sin(a.x);
v.y += AMOUNT * sin(a.y);
v.z += AMOUNT * (atan2(a.x * a.x, a.y * a.y) * cos(a.z));",
    ),
    VarDef {
        name: "sinusoidal_proj",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * a.x * (a.y * d[0]).cos();
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * a.x * cos(a.y * D(0));
v.y += AMOUNT * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "_slice_indicator_wf",
        params: &[
            ParamDef::new("thickness", 0.01),
            ParamDef::new("position_1", 0.0),
            ParamDef::int("dc_red_1", 255.0, 0.0, 255.0),
            ParamDef::int("dc_green_1", 0.0, 0.0, 255.0),
            ParamDef::int("dc_blue_1", 0.0, 0.0, 255.0),
            ParamDef::new("position_2", 0.0),
            ParamDef::int("dc_red_2", 255.0, 0.0, 255.0),
            ParamDef::int("dc_green_2", 0.0, 0.0, 255.0),
            ParamDef::int("dc_blue_2", 0.0, 0.0, 255.0),
            ParamDef::int("random_fill", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Post,
        is_3d: true,
        apply: |d, ctx, _a, v, _amount| {
            // d = [thickness, position_1, r1, g1, b1, position_2, r2, g2, b2, random_fill, min1, max1, min2, max2]
            let z = v.z;
            for (k, lo, hi) in [(2, d[10], d[11]), (6, d[12], d[13])] {
                if z >= lo && z <= hi {
                    v.rgb_color = true;
                    v.red = d[k] * 4.0;
                    v.green = d[k + 1] * 4.0;
                    v.blue = d[k + 2] * 4.0;
                    return;
                }
            }
            for (k, lo) in [(2, d[10]), (6, d[12])] {
                if d[9] as i32 == 1 && ctx.random() > 0.75 {
                    v.z = lo;
                    let (s, c) = (M_2PI * ctx.random()).sin_cos();
                    (v.x, v.y) = (s * v.x + c * v.y, c * v.x - s * v.y);
                    v.rgb_color = true;
                    v.red = d[k];
                    v.green = d[k + 1];
                    v.blue = d[k + 2];
                    return;
                }
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for k in [2, 3, 4, 6, 7, 8] {
                d[k] = ftoi(p[k]).max(0) as f64;
            }
            d[9] = limit_value(ftoi(p[9]), 0, 1) as f64;
            let ath = p[0].abs();
            d.extend([p[1] - 0.5 * ath, p[1] + 0.5 * ath, p[5] - 0.5 * ath, p[5] + 0.5 * ath]);
            d
        }),
        wgsl: "let z = v.z;
if (z >= D(10) && z <= D(11)) {
  v.rgbc = 1.0;
  v.r = D(2) * 4.0;
  v.g = D(3) * 4.0;
  v.b = D(4) * 4.0;
} else if (z >= D(12) && z <= D(13)) {
  v.rgbc = 1.0;
  v.r = D(6) * 4.0;
  v.g = D(7) * 4.0;
  v.b = D(8) * 4.0;
} else if (i32(D(9)) == 1) {
  var k = -1;
  if (rnd() > 0.75) { k = 0; } else if (rnd() > 0.75) { k = 1; }
  if (k >= 0) {
    let o = u32(k) * 4u;
    v.z = select(D(12), D(10), k == 0);
    let an = TWO_PI * rnd();
    let s = sin(an);
    let c = cos(an);
    let nx = s * v.x + c * v.y;
    v.y = c * v.x - s * v.y;
    v.x = nx;
    v.rgbc = 1.0;
    v.r = prm[base + 2u + o];
    v.g = prm[base + 3u + o];
    v.b = prm[base + 4u + o];
  }
}",
    },
    VarDef {
        name: "snowflake_wf",
        params: &[
            ParamDef::int("buffer_size", 128.0, 8.0, 512.0),
            ParamDef::int("max_iter", 500.0, 1.0, 5000.0),
            ParamDef::new("bg_freeze_level", 0.5),
            ParamDef::new("fg_freeze_speed", 0.0005),
            ParamDef::new("diffusion_speed", 0.01),
            ParamDef::new("diffusion_asymmetry", 1.0),
            ParamDef::new("rnd_bg_noise", 0.25),
            ParamDef::new("threshold", 0.65),
            ParamDef::int("seed", 12345.0, 0.0, 100000.0),
            ParamDef::new("scale", 1.0),
            ParamDef::range("jitter", 0.001, 0.0, 1.0),
            ParamDef::int("dc_color", 1.0, 0.0, 1.0),
            ParamDef::new("dc_color_scale", 2.0),
            ParamDef::new("dc_color_offset", 0.1),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [14 params, psize (points - 1), points as (x, y, intensity)...]
            let psize = d[14];
            let (px, py, pi) = if psize > 0.0 {
                let o = 15 + 3 * ftoi(ctx.random() * psize) as usize;
                (d[o], d[o + 1], d[o + 2])
            } else {
                (0.0, 0.0, 0.0)
            };
            v.x += amount * px;
            v.y += amount * py;
            ctx.pass_z(a, v, amount);
            if d[11] > 0.0 {
                v.color = pi * d[12] + d[13];
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for k in [0, 1, 8, 11] {
                d[k] = ftoi(p[k]) as f64;
            }
            let pts = snowflake_points(&d);
            d.push(pts.len() as f64 - 1.0);
            d.extend(pts.iter().flatten());
            d
        }),
        wgsl: "let psize = D(14);
var p = vec3<f32>(0.0);
if (psize > 0.0) {
  let o = base + 15u + 3u * u32(ftoi(rnd() * psize));
  p = vec3<f32>(prm[o], prm[o + 1u], prm[o + 2u]);
}
v.x += AMOUNT * p.x;
v.y += AMOUNT * p.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
if (D(11) > 0.0) { v.color = p.z * D(12) + D(13); }",
    },
    VarDef {
        name: "solidangle3D",
        params: &[ParamDef::range("p1", 0.25, 0.0, 0.5), ParamDef::range("p2", 0.5, 0.0, 0.5), ParamDef::range("p3", 0.5, 0.0, 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [p1, p2, p3]
            let c = vec2(d[0], d[1]);
            for _ in 0..50 {
                let (x, y, z) = (ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5);
                let p = vec2((x * x + z * z).sqrt(), y);
                let l = length(p) - d[2];
                let m = length(p - c * clamp(dot(p, c), 0.0, d[2]));
                if java_max(l, m * sign(c.y * p.x - c.x * p.y)) < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, 0.5), limit_value(p[1], 0.0, 0.5), limit_value(p[2], 0.0, 0.5)]),
        wgsl: "let c = vec2<f32>(D(0), D(1));
var found = false;
for (var i = 0; i < 50; i++) {
  let q = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  let p = vec2<f32>(length(q.xz), q.y);
  let l = length(p) - D(2);
  let m = length(p - c * clamp(dot(p, c), 0.0, D(2)));
  if (max(l, m * sign(c.y * p.x - c.x * p.y)) < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * q.x;
    v.y += AMOUNT * q.y;
    v.z += AMOUNT * q.z;
    found = true;
    break;
  }
}
if (!found) { v.hide = 1.0; }",
    },
    VarDef {
        name: "sph3D",
        params: &[ParamDef::new("x", 0.75), ParamDef::new("y", 1.0), ParamDef::new("z", 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            // Java scales z with the x parameter.
            let (xx, yy, zz) = (d[0] * a.x, d[1] * a.y, d[0] * a.z);
            let r = amount / (xx * xx + yy * yy + zz * zz + 10.000e-30);
            v.x += a.x * r;
            v.y += a.y * r;
            v.z += a.z * r;
        },
        prepare: None,
        wgsl: "let xx = D(0) * a.x;
let yy = D(1) * a.y;
let zz = D(0) * a.z;
let r = AMOUNT / (xx * xx + yy * yy + zz * zz + 1e-29);
v.x += a.x * r;
v.y += a.y * r;
v.z += a.z * r;",
    },
    VarDef {
        name: "spherecrop",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::new("x", 0.0),
            ParamDef::new("y", 0.0),
            ParamDef::new("z", 0.0),
            ParamDef::range("scatter_area", 0.0, -1.0, 1.0),
            ParamDef::int("zero", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [radius, x, y, z, scatter_area, zero, cA]
            let (x0, y0, z0, cr) = (d[1], d[2], d[3], d[0]);
            // Java moves the affine point itself.
            a.x -= x0;
            a.y -= y0;
            a.z -= z0;
            let rad = (a.x * a.x + a.y * a.y + a.z * a.z).sqrt();
            let ang = a.y.atan2(a.x);
            let az = (a.z / rad).acos();
            let rdc = cr + ctx.random() * 0.5 * d[6];
            let esc = rad > cr;
            let cr0 = d[5] as i32 == 1;
            v.do_hide = false;
            if cr0 && esc {
                v.x = 0.0;
                v.y = 0.0;
                v.z = 0.0;
                v.do_hide = true;
            } else if !cr0 && esc {
                v.x += amount * rdc * ang.cos() * az.sin() + x0;
                v.y += amount * rdc * ang.sin() * az.sin() + y0;
                v.z += amount * rdc * az.cos() + z0;
            } else {
                v.x += amount * a.x + x0;
                v.y += amount * a.y + y0;
                v.z += amount * a.z + z0;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[5] = ftoi(p[5]) as f64;
            d.push(java_max(-1.0, java_min(p[4], 1.0)));
            d
        }),
        wgsl: "let o = vec3<f32>(D(1), D(2), D(3));
let cr = D(0);
a.x -= o.x;
a.y -= o.y;
a.z -= o.z;
let rad = sqrt(a.x * a.x + a.y * a.y + a.z * a.z);
let ang = atan2(a.y, a.x);
let az = acos(clamp(a.z / rad, -1.0, 1.0));
let rdc = cr + rnd() * 0.5 * D(6);
let esc = rad > cr;
let cr0 = i32(D(5)) == 1;
v.hide = 0.0;
if (cr0 && esc) {
  v.x = 0.0;
  v.y = 0.0;
  v.z = 0.0;
  v.hide = 1.0;
} else if (!cr0 && esc) {
  v.x += AMOUNT * rdc * cos(ang) * sin(az) + o.x;
  v.y += AMOUNT * rdc * sin(ang) * sin(az) + o.y;
  v.z += AMOUNT * rdc * cos(az) + o.z;
} else {
  v.x += AMOUNT * a.x + o.x;
  v.y += AMOUNT * a.y + o.y;
  v.z += AMOUNT * a.z + o.z;
}",
    },
    VarDef {
        name: "sphere_nja",
        params: &[
            ParamDef::new("circle_a", 1.0),
            ParamDef::new("circle_b", 1.0),
            ParamDef::new("shift_x", 0.0),
            ParamDef::new("shift_y", 0.0),
            ParamDef::new("shift_z", 0.0),
            ParamDef::new("stretch", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            // d = [circle_a, circle_b, shift_x, shift_y, shift_z, stretch]
            let (x, y, z) = (a.x - d[2], a.y - d[3], a.z - d[4]);
            let sq = (x * x + y * y).sqrt() + SMALL_EPSILON;
            let t = sq / d[5] - M_PI / 2.0;
            let r = t.cos() * amount;
            let zz = t.sin() * amount;
            v.x += r * (x / sq);
            v.y += r * (y / sq);
            v.z += zz;
            v.x += r * z * (x / sq);
            v.y += r * z * (y / sq);
            v.z += z * zz / (r * r + zz * zz);
            v.x += d[2];
            v.y += d[3];
            v.z += d[4];
        },
        prepare: None,
        wgsl: "let x = a.x - D(2);
let y = a.y - D(3);
let z = a.z - D(4);
let sq = sqrt(x * x + y * y) + EPS;
let t = sq / D(5) - PI / 2.0;
let r = cos(t) * AMOUNT;
let zz = sin(t) * AMOUNT;
v.x += r * (x / sq) + r * z * (x / sq) + D(2);
v.y += r * (y / sq) + r * z * (y / sq) + D(3);
v.z += zz + z * zz / (r * r + zz * zz) + D(4);",
    },
    VarDef {
        name: "spherical3D_wf",
        params: &[ParamDef::int("invert", 0.0, 0.0, 1.0), ParamDef::new("exponent", 2.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            // d = [invert, exponent, regular_form]
            let r2 = a.x * a.x + a.y * a.y + a.z * a.z + SMALL_EPSILON;
            let r = if d[2] != 0.0 { amount / r2 } else { amount / r2.powf(d[1] / 2.0) };
            if d[0] as i32 == 0 {
                v.x += a.x * r;
                v.y += a.y * r;
                v.z += a.z * r;
            } else {
                v.x -= a.x * r;
                v.y -= a.y * r;
                v.z -= a.z * r;
            }
        },
        prepare: Some(|p, _| vec![ftoi(p[0]) as f64, p[1], if (p[1] - 2.0).abs() < EPSILON { 1.0 } else { 0.0 }]),
        wgsl: "let r2 = a.x * a.x + a.y * a.y + a.z * a.z + EPS;
var r = AMOUNT / r2;
if (D(2) == 0.0) { r = AMOUNT / pow(r2, D(1) / 2.0); }
if (i32(D(0)) != 0) { r = -r; }
v.x += a.x * r;
v.y += a.y * r;
v.z += a.z * r;",
    },
    VarDef {
        name: "sphericalN",
        params: &[ParamDef::new("power", 3.0), ParamDef::new("dist", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (sqr(a.x) + sqr(a.y)).sqrt().powf(d[1]);
            let n = (d[0] * ctx.random()).floor() as i32;
            let alpha = a.y.atan2(a.x) + n as f64 * M_2PI / d[0].floor();
            if r > SMALL_EPSILON {
                v.x += amount * alpha.cos() / r;
                v.y += amount * alpha.sin() / r;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = pow(sqrt(a.x * a.x + a.y * a.y), D(1));
let n = floor(D(0) * rnd());
let alpha = atan2(a.y, a.x) + n * TWO_PI / floor(D(0));
if (r > EPS) {
  v.x += AMOUNT * cos(alpha) / r;
  v.y += AMOUNT * sin(alpha) / r;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sph_harmonic",
        params: &[ParamDef::int("l", 2.0, 0.0, 12.0), ParamDef::int("m", 1.0, 0.0, 12.0), ParamDef::new("strength", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let theta = a.y.atan2(a.x);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let harm = (d[0] * theta).cos() * (d[1] * theta).cos();
            let nr = r * (1.0 + d[2] * harm);
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![(p[0] as i32) as f64, (p[1] as i32) as f64, p[2]]),
        wgsl: "let theta = atan2(a.y, a.x);
let nr = sqrt(a.x * a.x + a.y * a.y) * (1.0 + D(2) * cos(D(0) * theta) * cos(D(1) * theta));
v.x += AMOUNT * nr * cos(theta);
v.y += AMOUNT * nr * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sphtiling3v2",
        params: &[
            ParamDef::new("a", 0.5),
            ParamDef::new("b", 1.0),
            ParamDef::new("c", 1.0),
            ParamDef::new("d", 0.5),
            ParamDef::new("e", 1.0),
            ParamDef::new("f", 0.5),
            ParamDef::int("dc", 1.0, 0.0, 1.0),
            ParamDef::new("dc1", 0.3),
            ParamDef::new("dc2", 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [a, b, c, d, e, f, dc, dc1, dc2]
            const M_SQRT3_2: f64 = 0.8660254037844386;
            // Java iterates one internal point across calls (per render thread here).
            let [mut x, mut y] = *state_slot(ctx, d, 0, [0.0; 2]);
            let (ux, uy);
            if ctx.random() < d[0] {
                let n = (3.0 * ctx.random()).floor();
                let r = d[1] / M_SQRT3_2;
                let (x0, y0) = (r * (n * M_2PI / 3.0).cos(), r * (n * M_2PI / 3.0).sin());
                x -= x0;
                y -= y0;
                let rr = d[2] / (x * x + y * y + SMALL_EPSILON);
                (ux, uy) = (x * rr + x0, y * rr + y0);
            } else {
                let rr = d[4] / (x * x + y * y + SMALL_EPSILON);
                let (sx, sy) = (x * rr, y * rr);
                (ux, uy) = (0.0411259 * (sx * d[5] - sy * M_SQRT3_2), 0.0411259 * (sx * M_SQRT3_2 + sy * d[3]));
            }
            if d[6] as i32 == 1 {
                v.color = if a.x < 0.0 { d[8] } else { d[7] };
            }
            *state_slot(ctx, d, 0, [0.0; 2]) = [ux, uy];
            v.x += amount * ux;
            v.y += amount * uy;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[6] = limit_value(ftoi(p[6]), 0, 1) as f64;
            d
        }),
        // GPU: no state across calls; the internal point is iterated 16 times from the origin.
        wgsl: "let s32 = 0.8660254037844386;
var p = vec2<f32>(0.0);
for (var i = 0; i < 16; i++) {
  if (rnd() < D(0)) {
    let n = floor(3.0 * rnd());
    let r = D(1) / s32;
    let c0 = r * vec2<f32>(cos(n * TWO_PI / 3.0), sin(n * TWO_PI / 3.0));
    let q = p - c0;
    p = q * (D(2) / (dot(q, q) + EPS)) + c0;
  } else {
    let s = p * (D(4) / (dot(p, p) + EPS));
    p = 0.0411259 * vec2<f32>(s.x * D(5) - s.y * s32, s.x * s32 + s.y * D(3));
  }
}
if (i32(D(6)) == 1) { v.color = select(D(7), D(8), a.x < 0.0); }
v.x += AMOUNT * p.x;
v.y += AMOUNT * p.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "spiralwing",
        false,
        |_, ctx, a, v, amount| {
            let c1 = sqr(a.x);
            let c2 = sqr(a.y);
            let dd = amount / (c1 + c2 + SMALL_EPSILON);
            let s2 = c2.sin();
            v.x += dd * c1.cos() * s2;
            v.y += dd * c1.sin() * s2;
            ctx.pass_z(a, v, amount);
        },
        "let c1 = a.x * a.x;
let c2 = a.y * a.y;
let dd = AMOUNT / (c1 + c2 + EPS);
v.x += dd * cos(c1) * sin(c2);
v.y += dd * sin(c1) * sin(c2);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "spirograph",
        params: &[
            ParamDef::new("a", 3.0),
            ParamDef::new("b", 2.0),
            ParamDef::new("d", 0.0),
            ParamDef::new("tmin", -1.0),
            ParamDef::new("tmax", 1.0),
            ParamDef::new("ymin", -1.0),
            ParamDef::new("ymax", 1.0),
            ParamDef::new("c1", 0.0),
            ParamDef::new("c2", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [a, b, d, tmin, tmax, ymin, ymax, c1, c2]
            let t = (d[4] - d[3]) * ctx.random() + d[3];
            let y = (d[6] - d[5]) * ctx.random() + d[5];
            let ab = d[0] + d[1];
            let x1 = ab * t.cos() - d[7] * (ab / d[1] * t).cos();
            let y1 = ab * t.sin() - d[8] * (ab / d[1] * t).sin();
            v.x += amount * (x1 + d[2] * t.cos() + y);
            v.y += amount * (y1 + d[2] * t.sin() + y);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let t = (D(4) - D(3)) * rnd() + D(3);
let y = (D(6) - D(5)) * rnd() + D(5);
let ab = D(0) + D(1);
let x1 = ab * cos(t) - D(7) * cos(ab / D(1) * t);
let y1 = ab * sin(t) - D(8) * sin(ab / D(1) * t);
v.x += AMOUNT * (x1 + D(2) * cos(t) + y);
v.y += AMOUNT * (y1 + D(2) * sin(t) + y);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "roche",
        params: &[ParamDef::new("sep", 0.5), ParamDef::range("mass", 0.5, 0.01, 0.99)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sep = java_max(d[0].abs(), 0.01);
            let mass = java_max(0.01, java_min(0.99, d[1]));
            let mx = sep * 0.5;
            let (dx1, dx2) = (a.x - mx, a.x + mx);
            let r1 = java_max((dx1 * dx1 + a.y * a.y).sqrt(), 0.01);
            let r2 = java_max((dx2 * dx2 + a.y * a.y).sqrt(), 0.01);
            let (r13, r23) = (r1 * r1 * r1, r2 * r2 * r2);
            let px = -mass * dx1 / r13 - (1.0 - mass) * dx2 / r23;
            let py = -mass * a.y / r13 - (1.0 - mass) * a.y / r23;
            v.x += amount * (a.x - py * 0.5);
            v.y += amount * (a.y + px * 0.5);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sep = max(abs(D(0)), 0.01);
let mass = clamp(D(1), 0.01, 0.99);
let mx = sep * 0.5;
let dx1 = a.x - mx;
let dx2 = a.x + mx;
let r1 = max(sqrt(dx1 * dx1 + a.y * a.y), 0.01);
let r2 = max(sqrt(dx2 * dx2 + a.y * a.y), 0.01);
let r13 = r1 * r1 * r1;
let r23 = r2 * r2 * r2;
let px = -mass * dx1 / r13 - (1.0 - mass) * dx2 / r23;
let py = -mass * a.y / r13 - (1.0 - mass) * a.y / r23;
v.x += AMOUNT * (a.x - py * 0.5);
v.y += AMOUNT * (a.y + px * 0.5);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `rSquares.draw` of JWildfire: the point list of the recursive squares.
    fn rsquares_points(n: i32, x: f64, y: f64, size: f64, pts: &mut Vec<[f64; 2]>) {
        if n == 0 {
            return;
        }
        let (h0, h1) = (x - size / 2.0, x + size / 2.0);
        let (v0, v1) = (y - size / 2.0, y + size / 2.0);
        pts.extend([[h0, v0], [h0, v1], [h0, v1], [h1, v1], [h1, v1], [h1, v0], [h1, v0], [h0, v0]]);
        for (cx, cy) in [(h0, v0), (h0, v1), (h1, v0), (h1, v1)] {
            rsquares_points(n - 1, cx, cy, size / 2.2, pts);
        }
    }

    #[test]
    fn rsquares_segments_follow_the_drawing_order() {
        for level in 1..=5 {
            let mut pts = Vec::new();
            rsquares_points(level, 0.5, 0.5, 1.0, &mut pts);
            for k in 0..pts.len() / 2 {
                let s = rsquares_segment(level, k as i64);
                assert_eq!(s, [pts[2 * k][0], pts[2 * k][1], pts[2 * k + 1][0], pts[2 * k + 1][1]], "level {level} segment {k}");
            }
        }
    }
}
