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

//! Variations ported from the JWildfire sources (batch w08).
//!
//! Mostly `DC_BaseFunc` subclasses (`dc_*`), plus `dc_triangle` and `dc_triTile`.
//!
//! Notes on the `DC_*` ports:
//! * The `seed` (`randomize`) parameters only re-derive `time` from the wall clock in JWildfire
//!   (`setParameter`); they are kept as inert parameters here, `time` is used as given.
//! * The `setParameter` limits (`Tools.limitValue`, `(int)` casts) are applied in `prepare`.

use super::glsl::*;
use super::{Ctx, ParamDef, Pt, VarDef, VarKind};
use crate::math::*;
use crate::model::Affine;

// ---------------------------------------------------------------------------------------------
// DC_BaseFunc plumbing
// ---------------------------------------------------------------------------------------------

/// Own parameters followed by the `DC_BaseFunc` parameters
/// (`ColorOnly, Gradient, scale_z, offset_z, reset_z`).
macro_rules! dc_params {
    ($($p:expr),* $(,)?) => {
        &[
            $($p,)*
            ParamDef::int("ColorOnly", 0.0, 0.0, 1.0),
            ParamDef::int("Gradient", 0.0, 0.0, 2.0),
            ParamDef::new("scale_z", 0.0),
            ParamDef::new("offset_z", 0.0),
            ParamDef::int("reset_z", 1.0, 0.0, 1.0),
        ]
    };
}

/// A `DC_BaseFunc` subclass that only overrides `getRGBColor`: `$n` is the index of `ColorOnly`.
macro_rules! dc_def {
    ($name:expr, $n:expr, $params:expr, $prep:expr, $rgb:path, $wgsl:expr) => {
        VarDef {
            name: $name,
            params: $params,
            kind: VarKind::Normal,
            is_3d: false,
            apply: |d, ctx, a, v, amount| {
                let uv = dcb_uv(d, $n, ctx, a);
                let color = $rgb(d, uv);
                dcb_finish(d, $n, ctx, v, amount, uv, color);
            },
            prepare: Some($prep),
            wgsl: $wgsl,
        }
    };
}

/// `Tools.limitValue(v, lo, hi)`
#[inline]
fn lim(v: f64, lo: f64, hi: f64) -> f64 {
    limit_value(v, lo, hi)
}

/// `(int) Tools.limitValue(v, lo, hi)`
#[inline]
fn limi(v: f64, lo: f64, hi: f64) -> f64 {
    limit_value(v, lo, hi) as i32 as f64
}

/// Copies the parameters and applies the `DC_BaseFunc.setParameter` limits (base params at `n`).
fn dcb_prep(p: &[f64], n: usize) -> Vec<f64> {
    let mut d = p.to_vec();
    d[n] = limi(d[n], 0.0, 1.0);
    d[n + 1] = limi(d[n + 1], 0.0, 2.0);
    d[n + 4] = limi(d[n + 4], 0.0, 1.0);
    d
}

/// `DC_BaseFunc.transform`: the sampled point (affine point or a random point in [-0.5, 0.5)).
#[inline]
fn dcb_uv(d: &[f64], n: usize, ctx: &mut Ctx, a: &Pt) -> Vec2 {
    if d[n] as i32 == 1 {
        vec2(a.x, a.y)
    } else {
        let x = ctx.random() - 0.5;
        let y = ctx.random() - 0.5;
        vec2(x, y)
    }
}

/// Tail of `DC_BaseFunc.transform` (see `dc_output`).
#[inline]
fn dcb_finish(d: &[f64], n: usize, ctx: &Ctx, v: &mut Pt, amount: f64, uv: Vec2, color: Vec3) {
    dc_output(ctx, v, amount, uv, color, d[n + 1] as i32, d[n + 2], d[n + 3], d[n + 4] as i32);
}

// ---------------------------------------------------------------------------------------------
// dc_mandelbox2D
// ---------------------------------------------------------------------------------------------

// d = [zoom, seed, time, <dc base>(3..8)]
fn dc_mandelbox2d_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 3);
    d[0] = lim(d[0], 0.1, 50.0);
    d[1] = limi(d[1], 0.0, 10000.0);
    d
}

fn dc_mandelbox2d_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let (zoom, time) = (d[0], d[2]);
    let mut i = vec2(zoom * uv.x, zoom * uv.y);
    let (ox, oy, oz) = (i.x, i.y, -5.0 * (time * 0.1).cos());
    let mut dd = 1.0;
    for _ in 0..20 {
        i = clamp(i, -1.0, 1.0) * 2.0 - i; // boxfold
        let oa = length(i);
        // ballfold
        let b = if oa < 0.5 {
            4.0
        } else if oa < 1.0 {
            1.0 / oa
        } else {
            1.0
        };
        i = i * (oz * b) + vec2(ox, oy);
        dd = b * dd * oz.abs() + 1.0;
    }
    dd = (length(i) / dd).powf(0.1) * 5.0;
    vec3(dd.cos(), (10.0 * dd + 1.0).sin(), (3.0 * dd + 1.0).cos()) * 0.5 + 0.5
}

const DC_MANDELBOX2D_WGSL: &str = r#"
fn dc_mandelbox2D_rgb(uv: vec2<f32>, zoom: f32, time: f32) -> vec3<f32> {
  var i = uv * zoom;
  let o = i;
  let oz = -5.0 * cos(time * 0.1);
  var d = 1.0;
  for (var k = 0; k < 20; k++) {
    i = clamp(i, vec2<f32>(-1.0), vec2<f32>(1.0)) * 2.0 - i;
    let oa = length(i);
    var b = 1.0;
    if (oa < 0.5) { b = 4.0; } else if (oa < 1.0) { b = 1.0 / oa; }
    i = i * (oz * b) + o;
    d = b * d * abs(oz) + 1.0;
  }
  d = pow(length(i) / d, 0.1) * 5.0;
  return vec3<f32>(cos(d), sin(10.0 * d + 1.0), cos(3.0 * d + 1.0)) * 0.5 + 0.5;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_mandelbox2D_rgb(uv, D(0), D(2)), i32(D(4)), D(5), D(6), i32(D(7)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_menger
// ---------------------------------------------------------------------------------------------

// d = [zoom, level, <dc base>(2..7)]
fn dc_menger_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 2);
    d[1] = limi(d[1], 1.0, 5.0);
    d
}

fn dc_menger_sd_box(p: Vec2, s: Vec2) -> f64 {
    let p = abs(p) - s;
    length(max(p, 0.0)) + min(max(p.x, p.y), 0.0)
}

fn dc_menger_sd_cross(p: Vec2) -> f64 {
    let p = abs(p);
    min(p.x, p.y) - 1.0
}

fn dc_menger_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let level = d[1] as i32;
    let p = (uv * d[0]) * 1.3;
    let mut dd = dc_menger_sd_box(p, Vec2::splat(1.0));
    let mut s = 1.0;
    for _ in 0..level {
        let a = glsl_mod(p * s, 2.0) - 1.0;
        let r = Vec2::splat(1.0) - abs(a) * 3.0;
        s *= 3.0;
        let c = dc_menger_sd_cross(r) / s;
        dd = max(dd, c);
    }
    if dd > 0.0 { Vec3::splat(0.0) } else { Vec3::splat(1.0) }
}

const DC_MENGER_WGSL: &str = r#"
fn dc_menger_sd_box(p: vec2<f32>, s: vec2<f32>) -> f32 {
  let q = abs(p) - s;
  return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0);
}
fn dc_menger_sd_cross(p: vec2<f32>) -> f32 {
  let q = abs(p);
  return min(q.x, q.y) - 1.0;
}
fn dc_menger_rgb(uv: vec2<f32>, zoom: f32, level: i32) -> vec3<f32> {
  let p = (uv * zoom) * 1.3;
  var d = dc_menger_sd_box(p, vec2<f32>(1.0));
  var s = 1.0;
  for (var i = 0; i < level; i++) {
    let a = glsl_mod2(p * s, vec2<f32>(2.0)) - 1.0;
    let r = vec2<f32>(1.0) - abs(a) * 3.0;
    s *= 3.0;
    d = max(d, dc_menger_sd_cross(r) / s);
  }
  return select(vec3<f32>(1.0), vec3<f32>(0.0), d > 0.0);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(2)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_menger_rgb(uv, D(0), i32(D(1))), i32(D(3)), D(4), D(5), i32(D(6)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_moebiuslog
// ---------------------------------------------------------------------------------------------

// d = [zoom, seed, time, Log, Moebius, scale, angle, <dc base>(7..12)]
fn dc_moebiuslog_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 7);
    d[0] = lim(d[0], 0.1, 50.0);
    d[1] = limi(d[1], 0.0, 10000.0);
    d[3] = limi(d[3], 0.0, 1.0);
    d[4] = limi(d[4], 0.0, 1.0);
    d[5] = lim(d[5], 1.0, 10.0);
    d[6] = lim(d[6], 1.0, 10.0);
    d
}

fn dc_moebiuslog_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let mut u = uv * d[0];
    let z = u - vec2(-1.0, 0.0);
    if d[4] as i32 == 1 {
        // Moebius transform
        u.x -= 0.5;
        let tmp = Mat2::from_cols(z, vec2(-z.y, z.x));
        u = (u * tmp) / dot(u, u);
    }
    if d[3] as i32 == 1 {
        // spiraling (G.atan2 is the fast approximation)
        u = u + 0.5;
        u = vec2(0.5, -0.5) * length(u).ln() + vec2(d[6], 1.0) * (glsl_atan2(u.y, u.x) / 6.3);
    }
    Vec3::splat(length(fract(u * d[5])))
}

const DC_MOEBIUSLOG_WGSL: &str = r#"
fn dc_moebiuslog_rgb(uv: vec2<f32>, zoom: f32, lg: i32, moeb: i32, scale: f32, angle: f32) -> vec3<f32> {
  var u = uv * zoom;
  let z = u - vec2<f32>(-1.0, 0.0);
  if (moeb == 1) {
    u.x -= 0.5;
    u = (u * mat2x2<f32>(z, vec2<f32>(-z.y, z.x))) / dot(u, u);
  }
  if (lg == 1) {
    u = u + 0.5;
    u = vec2<f32>(0.5, -0.5) * log(length(u)) + vec2<f32>(angle, 1.0) * (glsl_atan2(u.y, u.x) / 6.3);
  }
  return vec3<f32>(length(fract(u * scale)));
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(7)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_moebiuslog_rgb(uv, D(0), i32(D(3)), i32(D(4)), D(5), D(6)), i32(D(8)), D(9), D(10), i32(D(11)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_pentatiles
// ---------------------------------------------------------------------------------------------

// d = [zoom, border, width, <dc base>(3..8)]
fn dc_pentatiles_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 3);
    d[0] = lim(d[0], 1.0, 100.0);
    d[1] = limi(d[1], 0.0, 1.0);
    d[2] = lim(d[2], 0.01, 0.25);
    d
}

fn dc_pentatiles_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let width = d[2];
    // S(v): antialiased line v = 0
    let big_s = |v: f64| smoothstep(width, 0.0, abs(v));
    // l(x, y, a, b): line equation
    let l = |x: f64, y: f64, a: f64, b: f64| dot(vec2(x, y), normalize(vec2(a, b)));
    // P(x, y, a, b): region under the line
    let p = |x: f64, y: f64, a: f64, b: f64| step(l(x, y, a, b), 0.0);

    let mut u = uv * d[0];
    let sq3 = 3f64.sqrt();
    let h = (sq3 + 1.0) / 2.0;
    let c = sq3 - 1.0;
    let r = h + c / 2.0;
    let t = glsl_mod(u, r + r) - r;
    let vv = abs(t);
    // Java writes the O.r/O.g/O.b fields (not x/y/z): plain scalars here.
    let o_r = p(vv.x, vv.y - h, -(c / 2.0 - h), r);
    let o_g = p(vv.x - c / 2.0, vv.y, -sq3, 1.0);
    let o_b = 1.0;
    u = floor(u / (r + r)) * 2.0; // supertile id
    if o_r == 0.0 {
        u = u + sign(t);
    }
    let i = (o_r + 2.0 * o_g + 4.0 * o_b) + 8.0 * (u.x + 12.0 * u.y); // tile id
    let mut o = cos(vec3(0.0, 23.0, 21.0) + i) * 0.6 + 0.6;
    o = o * (0.4 + 0.6 * fract(1234.0 * (43.0 * i).sin()));
    if d[1] as i32 == 1 {
        let b = big_s(vv.x) * step(h, vv.y)
            + big_s(r - vv.x) * step(vv.y, c / 2.0)
            + big_s(l(vv.x, vv.y - h, -(c / 2.0 - h), r))
            + big_s(vv.y) * step(vv.x, c / 2.0)
            + big_s(r - vv.y) * step(r - vv.x, c / 2.0)
            + big_s(l(vv.x - c / 2.0, vv.y, -sq3, 1.0));
        o = o * (1.0 - b);
    }
    o
}

const DC_PENTATILES_WGSL: &str = r#"
fn dc_pentatiles_s(v: f32, w: f32) -> f32 { return glsl_smoothstep(w, 0.0, abs(v)); }
fn dc_pentatiles_l(x: f32, y: f32, a: f32, b: f32) -> f32 { return dot(vec2<f32>(x, y), normalize(vec2<f32>(a, b))); }
fn dc_pentatiles_rgb(uv: vec2<f32>, zoom: f32, border: i32, w: f32) -> vec3<f32> {
  var u = uv * zoom;
  let sq3 = sqrt(3.0);
  let h = (sq3 + 1.0) / 2.0;
  let c = sq3 - 1.0;
  let r = h + c / 2.0;
  let t = glsl_mod2(u, vec2<f32>(r + r)) - r;
  let vv = abs(t);
  let o_r = step(dc_pentatiles_l(vv.x, vv.y - h, -(c / 2.0 - h), r), 0.0);
  let o_g = step(dc_pentatiles_l(vv.x - c / 2.0, vv.y, -sq3, 1.0), 0.0);
  u = floor(u / (r + r)) * 2.0;
  if (o_r == 0.0) { u = u + sign(t); }
  let i = (o_r + 2.0 * o_g + 4.0) + 8.0 * (u.x + 12.0 * u.y);
  var o = cos(vec3<f32>(0.0, 23.0, 21.0) + i) * 0.6 + 0.6;
  o = o * (0.4 + 0.6 * fract(1234.0 * sin(43.0 * i)));
  if (border == 1) {
    let b = dc_pentatiles_s(vv.x, w) * step(h, vv.y)
      + dc_pentatiles_s(r - vv.x, w) * step(vv.y, c / 2.0)
      + dc_pentatiles_s(dc_pentatiles_l(vv.x, vv.y - h, -(c / 2.0 - h), r), w)
      + dc_pentatiles_s(vv.y, w) * step(vv.x, c / 2.0)
      + dc_pentatiles_s(r - vv.y, w) * step(r - vv.x, c / 2.0)
      + dc_pentatiles_s(dc_pentatiles_l(vv.x - c / 2.0, vv.y, -sq3, 1.0), w);
    o = o * (1.0 - b);
  }
  return o;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_pentatiles_rgb(uv, D(0), i32(D(1)), D(2)), i32(D(4)), D(5), D(6), i32(D(7)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_poincaredisc
// ---------------------------------------------------------------------------------------------

// d = [zoom, MaxIter, N, P, checkers0, checkers1, checkers2, <dc base>(7..12), sx, sr, nfp.x, nfp.y]
fn dc_poincaredisc_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 7);
    d[1] = limi(d[1], 0.0, 20.0);
    d[2] = limi(d[2], 3.0, 20.0);
    d[3] = limi(d[3], 3.0, 20.0);
    for k in 4..7 {
        d[k] = limi(d[k], 0.0, 1.0);
    }
    // init()
    let a = M_PI / d[2];
    let mut b = M_PI / d[3];
    if a + b >= 0.5 * M_PI {
        b = 0.5 * M_PI - a - 0.01;
    }
    let (ca, sa) = (a.cos(), a.sin());
    let sb = b.sin();
    let c1 = (a + b).cos();
    let s = c1 / (1.0 - sa * sa - sb * sb).sqrt();
    let sx = (s * s + 1.0) / (2.0 * s * ca);
    let sr = (sx * sx - 1.0).sqrt();
    d.extend([sx, sr, sa, -ca]);
    d
}

/// Moebius transform preserving the unit circle (`mba = (1, 0)`, `mbb = (0, 0)`).
fn dc_poincaredisc_mbtpc(z: Vec2) -> Vec2 {
    let (mba, mbb) = (vec2(1.0, 0.0), vec2(0.0, 0.0));
    let zn = vec2(z.x * mba.x - z.y * mba.y + mbb.x, z.x * mba.y + z.y * mba.x + mbb.y);
    let zd = vec2(z.x * mbb.x + z.y * mbb.y + mba.x, -z.x * mbb.y + z.y * mbb.x - mba.y);
    let idn2 = 1.0 / dot(zd, zd);
    vec2(dot(zn, zd), dot(zn, vec2(zd.y, zd.x) * vec2(-1.0, 1.0))) * idn2
}

fn dc_poincaredisc_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let maxiter = d[1] as i32;
    let (sx, sr, nfp) = (d[12], d[13], vec2(d[14], d[15]));
    let mut z = uv * d[0];
    if dot(z, z) > 1.0 {
        return Vec3::splat(0.0);
    }
    z = dc_poincaredisc_mbtpc(z);
    let mut fc = Vec3::splat(0.0);
    // Java quirk: `az = z` aliases the same vec2 object, so the first fold is never counted
    // (fc.x stays 0) and the loop condition `az != z` (reference compare) is always true.
    for _ in 0..maxiter {
        z.y = abs(z.y);
        let t = 2.0 * min(0.0, dot(z, nfp));
        if t < 0.0 {
            fc.y += 1.0;
        }
        z = z - nfp * t;
        z.x -= sx;
        let r2 = dot(z, z);
        let k = max(sr * sr / r2, 1.0);
        if k != 1.0 {
            fc.z += 1.0;
        }
        z = z * k;
        z.x += sx;
    }
    let r = abs(length(vec2(z.x - sx, z.y)) - sr).powf(0.25);
    let k = glsl_mod(d[4] * fc.x + d[5] * fc.y + d[6] * fc.z, 2.0);
    Vec3::splat(r) * (0.5 * k + 0.5)
}

const DC_POINCAREDISC_WGSL: &str = r#"
fn dc_poincaredisc_rgb(uv: vec2<f32>, zoom: f32, maxiter: i32, c0: f32, c1: f32, c2: f32,
                       sx: f32, sr: f32, nfp: vec2<f32>) -> vec3<f32> {
  var z = uv * zoom;
  if (dot(z, z) > 1.0) { return vec3<f32>(0.0); }
  let mba = vec2<f32>(1.0, 0.0);
  let mbb = vec2<f32>(0.0, 0.0);
  let zn = vec2<f32>(z.x * mba.x - z.y * mba.y + mbb.x, z.x * mba.y + z.y * mba.x + mbb.y);
  let zd = vec2<f32>(z.x * mbb.x + z.y * mbb.y + mba.x, -z.x * mbb.y + z.y * mbb.x - mba.y);
  z = vec2<f32>(dot(zn, zd), dot(zn, zd.yx * vec2<f32>(-1.0, 1.0))) * (1.0 / dot(zd, zd));
  var fc = vec3<f32>(0.0);
  for (var i = 0; i < maxiter; i++) {
    z.y = abs(z.y);
    let t = 2.0 * min(0.0, dot(z, nfp));
    if (t < 0.0) { fc.y += 1.0; }
    z = z - nfp * t;
    z.x -= sx;
    let k = max(sr * sr / dot(z, z), 1.0);
    if (k != 1.0) { fc.z += 1.0; }
    z = z * k;
    z.x += sx;
  }
  let r = pow(abs(length(vec2<f32>(z.x - sx, z.y)) - sr), 0.25);
  let k = glsl_mod(c0 * fc.x + c1 * fc.y + c2 * fc.z, 2.0);
  return vec3<f32>(r) * (0.5 * k + 0.5);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(7)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_poincaredisc_rgb(uv, D(0), i32(D(1)), D(4), D(5), D(6), D(12), D(13), vec2<f32>(D(14), D(15)));
dc_output(&v, AMOUNT, uv, col, i32(D(8)), D(9), D(10), i32(D(11)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_portal (overrides transform: x/y are assigned, z is passed through)
// ---------------------------------------------------------------------------------------------

// d = [seed, time, waves, zoom, <dc base>(4..9)]
fn dc_portal_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let (time, waves) = (d[1], d[2]);
    let mut dd = 1.0;
    dd += length(uv).sqrt() / waves;
    let t = 1000.0 + time;
    let value = dd * t + (t * 0.125) * uv.x.cos() * uv.y.cos();
    let color = value.sin() * 3.0;
    let med = color.abs() - 1.0;
    let med_high = color.abs() - 1.5;
    let high = color.abs() - 2.0;
    // (the unused `lifeColor` of the Java code is dropped)
    if color > 0.0 { vec3(med, med_high, high) } else { Vec3::splat(med_high) }
}

const DC_PORTAL_WGSL: &str = r#"
fn dc_portal_rgb(uv: vec2<f32>, time: f32, waves: f32) -> vec3<f32> {
  let d = 1.0 + sqrt(length(uv)) / waves;
  let t = 1000.0 + time;
  let value = d * t + (t * 0.125) * cos(uv.x) * cos(uv.y);
  let color = sin(value) * 3.0;
  let med = abs(color) - 1.0;
  let med_high = abs(color) - 1.5;
  let high = abs(color) - 2.0;
  if (color > 0.0) { return vec3<f32>(med, med_high, high); }
  return vec3<f32>(med_high);
}
@body
var p = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(4)) == 1) { p = vec2<f32>(a.x, a.y); }
v.x = 0.0;
v.y = 0.0;
dc_output(&v, AMOUNT, p, dc_portal_rgb(p * D(3), D(1), D(2)), i32(D(5)), D(6), D(7), i32(D(8)));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
"#;

// ---------------------------------------------------------------------------------------------
// dc_quadtree
// ---------------------------------------------------------------------------------------------

// d = [zoom, seed, time, width, levels, <dc base>(5..10)]
fn dc_quadtree_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 5);
    d[0] = lim(d[0], 0.1, 50.0);
    d[1] = limi(d[1], 0.0, 10000.0);
    d[3] = lim(d[3], 100.0, 1000.0);
    d[4] = limi(d[4], 1.0, 50.0);
    d
}

fn dc_quadtree_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let (zoom, time, width, levels) = (d[0], d[2], d[3], d[4] as i32);
    let rnd = |p: f64| 0.5 + 0.5 * (p + time).sin() * (p * 2.17 - time).sin() * (p * 5.7 + time).sin();
    let mut r = 1.0;
    let mut id = 1.0;
    let mut u = uv / zoom + vec2(0.5, 0.5);
    // Java sets the O.b field (not z), so O starts black.
    let mut o = 0.0;
    for _ in 0..levels {
        let fu = min(u, Vec2::splat(1.0) - u);
        if min(fu.x, fu.y) < 0.3 * r / width {
            o -= 1.0; // cell border
            break;
        }
        if rnd(id) > 0.7 {
            break; // cell is out of the shape
        }
        let fu = step(0.5, u); // select child
        id = 4.0 * id + 2.0 * fu.y + fu.x;
        u = u * 2.0 - fu;
        r *= 2.0;
        if r > width {
            break;
        }
        o += 0.13;
    }
    Vec3::splat(o)
}

const DC_QUADTREE_WGSL: &str = r#"
fn dc_quadtree_rnd(p: f32, time: f32) -> f32 {
  return 0.5 + 0.5 * sin(p + time) * sin(p * 2.17 - time) * sin(p * 5.7 + time);
}
fn dc_quadtree_rgb(uv: vec2<f32>, zoom: f32, time: f32, width: f32, levels: i32) -> vec3<f32> {
  var r = 1.0;
  var id = 1.0;
  var u = uv / zoom + vec2<f32>(0.5);
  var o = 0.0;
  for (var i = 0; i < levels; i++) {
    let fu = min(u, vec2<f32>(1.0) - u);
    if (min(fu.x, fu.y) < 0.3 * r / width) { o -= 1.0; break; }
    if (dc_quadtree_rnd(id, time) > 0.7) { break; }
    let su = step(vec2<f32>(0.5), u);
    id = 4.0 * id + 2.0 * su.y + su.x;
    u = u * 2.0 - su;
    r *= 2.0;
    if (r > width) { break; }
    o += 0.13;
  }
  return vec3<f32>(o);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(5)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_quadtree_rgb(uv, D(0), D(2), D(3), i32(D(4))), i32(D(6)), D(7), D(8), i32(D(9)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_randomoctree
// ---------------------------------------------------------------------------------------------

// d = [seed, time, steps, rotLR, rotUD, grid, borders, bBorders, <dc base>(8..13), mouseX, mouseY]
fn dc_randomoctree_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 8);
    d[2] = limi(d[2], 3.0, 100.0);
    for k in 5..8 {
        d[k] = limi(d[k], 0.0, 1.0);
    }
    let (mx, my) = (fmod(d[3], 1.0), fmod(d[4], 1.0));
    d.extend([mx, my]);
    d
}

fn dc_randomoctree_rnd(v: Vec4) -> f64 {
    fract(4e4 * (dot(v, vec4(13.46, 41.74, -73.36, 14.24)) + 17.34).sin())
}

/// 0 is empty, 1 is subdivide and 2 is full.
fn dc_randomoctree_getvoxel(p: Vec3, size: f64) -> i32 {
    if p.x == 0.0 && p.y == 0.0 {
        return 0;
    }
    let val = dc_randomoctree_rnd(Vec4::from_vec3(p, size));
    if val < 0.5 {
        0
    } else if val < 0.95 {
        1
    } else {
        2
    }
}

/// Ray-cube intersection, on the inside of the cube.
fn dc_randomoctree_voxel(ro: Vec3, rd: Vec3, ird: Vec3, size: f64) -> Vec3 {
    let size = size * 0.5;
    (sign(rd) * (ro - size) - size) * ird * -1.0
}

fn dc_randomoctree_rot(spin: f64) -> Mat2 {
    mat2(spin.cos(), spin.sin(), -spin.sin(), spin.cos())
}

fn dc_randomoctree_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    const DETAIL: i32 = 5;
    let time = d[1];
    let steps = d[2] as i32;
    let (grid, borders, black_borders) = (d[5] as i32, d[6] as i32, d[7] as i32);
    let (mouse_x, mouse_y) = (d[13], d[14]);

    let mut size = 1.0;
    let maxdistance = 6.0;
    let ro = vec3(0.5 + time.sin() * 0.4, 0.5 + time.cos() * 0.4, time);
    let mut rd = normalize(Vec3::from_vec2(uv, 1.0));
    if length(vec2(mouse_x * 1000.0, mouse_y * 1000.0)) > 40.0 {
        let t1 = vec2(rd.y, rd.z) * dc_randomoctree_rot(mouse_y * M_PI - M_PI / 2.0);
        rd = vec3(rd.x, t1.x, t1.y);
        let t2 = vec2(rd.x, rd.z) * dc_randomoctree_rot(2.0 * mouse_x * M_PI - M_PI);
        rd = vec3(t2.x, rd.y, t2.y);
    }
    let mut lro = glsl_mod(ro, size);
    let mut fro = ro - lro;
    let ird = Vec3::splat(1.0) / max(abs(rd), Vec3::splat(0.001));
    let mut mask = Vec3::splat(0.0);
    let mut exitoct = false;
    let mut recursions = 0;
    let mut dist = 0.0;
    let mut edge = 1.0;
    let mut lastmask = Vec3::splat(0.0);

    // the octree traverser loop
    for _ in 0..steps {
        if dist > maxdistance {
            break;
        }
        let voxelstate = dc_randomoctree_getvoxel(fro, size);
        if exitoct {
            // go up a level
            let v1 = fro / (size / 0.5) + 0.25;
            let newfro = floor(v1) * (size * 2.0);
            lro = lro + (fro - newfro);
            fro = newfro;
            recursions -= 1;
            size *= 2.0;
            let v0 = glsl_mod(fro / size + 0.5, 2.0);
            let v3 = mask * sign(rd) * 0.5;
            let v1 = v0 - Vec3::splat(1.0) + v3;
            let f1 = abs(dot(v1, mask));
            exitoct = recursions > 0 && f1 < 0.1;
        } else if voxelstate == 1 && recursions <= DETAIL {
            // subdivide
            recursions += 1;
            size *= 0.5;
            let mask2 = step(Vec3::splat(size), lro);
            fro = fro + mask2 * size;
            lro = lro - mask2 * size;
        } else {
            // move forward
            let hit = dc_randomoctree_voxel(lro, rd, ird, size);
            mask = if hit.x < min(hit.y, hit.z) {
                vec3(1.0, 0.0, 0.0)
            } else if hit.y < hit.z {
                vec3(0.0, 1.0, 0.0)
            } else {
                vec3(0.0, 0.0, 1.0)
            };
            let len = dot(hit, mask);
            if voxelstate == 2 {
                break;
            }
            dist += len;
            lro = lro + (rd * len - mask * sign(rd) * size);
            let newfro = fro + mask * sign(rd) * size;
            // Java compares two distinct vec3 objects (`floor(v4) != floor(v4)`): always true.
            exitoct = recursions > 0;
            fro = newfro;
            lastmask = mask;
        }
        if grid == 1 {
            let q = abs(lro / size - 0.5) * (Vec3::splat(1.0) - lastmask);
            edge = min(edge, -(max(max(q.x, q.y), q.z) - 0.5) * 80.0 * size);
        }
    }
    // Java's `i < steps` is always true (i is never incremented).
    let color = if dist < maxdistance {
        let val = fract(dot(fro, vec3(15.23, 754.345, 3.454)));
        let normal = lastmask * sign(rd) * -1.0;
        let mut color = sin(vec3(39.896, 57.3225, 48.25) * val) * 0.5 + 0.5;
        color = color * (normal * 0.25 + 0.75);
        if borders == 1 {
            let q = abs(lro / size - 0.5) * (Vec3::splat(1.0) - lastmask);
            edge = clamp(-(max(max(q.x, q.y), q.z) - 0.5) * 20.0 * size, 0.0, edge);
        }
        if black_borders == 1 { color * edge } else { Vec3::splat(1.0) - (Vec3::splat(1.0) - color) * edge }
    } else if black_borders == 1 {
        Vec3::splat(edge)
    } else {
        Vec3::splat(1.0 - edge)
    };
    sqrt(color)
}

const DC_RANDOMOCTREE_WGSL: &str = r#"
fn dc_randomoctree_rnd(v: vec4<f32>) -> f32 {
  return fract(4e4 * sin(dot(v, vec4<f32>(13.46, 41.74, -73.36, 14.24)) + 17.34));
}
fn dc_randomoctree_getvoxel(p: vec3<f32>, size: f32) -> i32 {
  if (p.x == 0.0 && p.y == 0.0) { return 0; }
  let val = dc_randomoctree_rnd(vec4<f32>(p, size));
  if (val < 0.5) { return 0; }
  if (val < 0.95) { return 1; }
  return 2;
}
fn dc_randomoctree_rot(s: f32) -> mat2x2<f32> { return mat2x2<f32>(cos(s), sin(s), -sin(s), cos(s)); }
fn dc_randomoctree_rgb(uv: vec2<f32>, time: f32, steps: i32, mx: f32, my: f32, grid: i32, borders: i32,
                       bb: i32) -> vec3<f32> {
  var size = 1.0;
  let maxdistance = 6.0;
  let ro = vec3<f32>(0.5 + sin(time) * 0.4, 0.5 + cos(time) * 0.4, time);
  var rd = normalize(vec3<f32>(uv, 1.0));
  if (length(vec2<f32>(mx * 1000.0, my * 1000.0)) > 40.0) {
    let t1 = vec2<f32>(rd.y, rd.z) * dc_randomoctree_rot(my * PI - PI / 2.0);
    rd = vec3<f32>(rd.x, t1.x, t1.y);
    let t2 = vec2<f32>(rd.x, rd.z) * dc_randomoctree_rot(2.0 * mx * PI - PI);
    rd = vec3<f32>(t2.x, rd.y, t2.y);
  }
  let srd = sign(rd);
  var lro = glsl_mod3(ro, vec3<f32>(size));
  var fro = ro - lro;
  let ird = vec3<f32>(1.0) / max(abs(rd), vec3<f32>(0.001));
  var mask = vec3<f32>(0.0);
  var exitoct = false;
  var recursions = 0;
  var dist = 0.0;
  var edge = 1.0;
  var lastmask = vec3<f32>(0.0);
  for (var k = 0; k < steps; k++) {
    if (dist > maxdistance) { break; }
    let vs = dc_randomoctree_getvoxel(fro, size);
    if (exitoct) {
      let newfro = floor(fro / (size / 0.5) + 0.25) * (size * 2.0);
      lro = lro + (fro - newfro);
      fro = newfro;
      recursions -= 1;
      size *= 2.0;
      let v1 = glsl_mod3(fro / size + 0.5, vec3<f32>(2.0)) - vec3<f32>(1.0) + mask * srd * 0.5;
      exitoct = recursions > 0 && abs(dot(v1, mask)) < 0.1;
    } else if (vs == 1 && recursions <= 5) {
      recursions += 1;
      size *= 0.5;
      let mask2 = step(vec3<f32>(size), lro);
      fro = fro + mask2 * size;
      lro = lro - mask2 * size;
    } else {
      let hs = size * 0.5;
      let hit = -((srd * (lro - hs) - hs) * ird);
      if (hit.x < min(hit.y, hit.z)) {
        mask = vec3<f32>(1.0, 0.0, 0.0);
      } else if (hit.y < hit.z) {
        mask = vec3<f32>(0.0, 1.0, 0.0);
      } else {
        mask = vec3<f32>(0.0, 0.0, 1.0);
      }
      let len = dot(hit, mask);
      if (vs == 2) { break; }
      dist += len;
      lro = lro + (rd * len - mask * srd * size);
      fro = fro + mask * srd * size;
      exitoct = recursions > 0;
      lastmask = mask;
    }
    if (grid == 1) {
      let q = abs(lro / size - 0.5) * (vec3<f32>(1.0) - lastmask);
      edge = min(edge, -(max(max(q.x, q.y), q.z) - 0.5) * 80.0 * size);
    }
  }
  var color: vec3<f32>;
  if (dist < maxdistance) {
    let val = fract(dot(fro, vec3<f32>(15.23, 754.345, 3.454)));
    let normal = -(lastmask * srd);
    color = sin(vec3<f32>(39.896, 57.3225, 48.25) * val) * 0.5 + 0.5;
    color = color * (normal * 0.25 + 0.75);
    if (borders == 1) {
      let q = abs(lro / size - 0.5) * (vec3<f32>(1.0) - lastmask);
      // Java clamp = min(max(x, lo), hi), also for hi < lo (WGSL clamp is not defined there)
      edge = min(max(-(max(max(q.x, q.y), q.z) - 0.5) * 20.0 * size, 0.0), edge);
    }
    if (bb == 1) {
      color = color * edge;
    } else {
      color = vec3<f32>(1.0) - (vec3<f32>(1.0) - color) * edge;
    }
  } else {
    if (bb == 1) { color = vec3<f32>(edge); } else { color = vec3<f32>(1.0 - edge); }
  }
  // Java: sqrt(<0) = NaN, which dbl2int maps to 0 -- same as sqrt(0).
  return sqrt(max(color, vec3<f32>(0.0)));
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(8)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_randomoctree_rgb(uv, D(1), i32(D(2)), D(13), D(14), i32(D(5)), i32(D(6)), i32(D(7)));
dc_output(&v, AMOUNT, uv, col, i32(D(9)), D(10), D(11), i32(D(12)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_rotations
// ---------------------------------------------------------------------------------------------

// d = [levels, seed, time, zoom, <dc base>(4..9)]
fn dc_rotations_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 4);
    d[0] = limi(d[0], 1.0, 50.0);
    d
}

fn dc_rotations_hsv2rgb_smooth(c: Vec3) -> Vec3 {
    let t1 = vec3(0.0, 4.0, 2.0) + c.x * 6.0;
    let t2 = glsl_mod(t1, 6.0) - 3.0;
    let mut rgb = clamp(abs(t2) - 1.0, 0.0, 1.0);
    let t0 = Vec3::splat(3.0) - rgb * 2.0;
    rgb = rgb * rgb * t0; // cubic smoothing
    mix(Vec3::splat(1.0), rgb, c.y) * c.z
}

fn dc_rotations_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let (levels, time, zoom) = (d[0] as i32, d[2], d[3]);
    let mut uv = uv * zoom;
    let dd = 1.0 - length(uv);
    let mut s = 0.5;
    let (sa, ca) = (time * 0.1).sin_cos();
    let rot = mat2(ca, -sa, sa, ca);
    for _ in 0..levels {
        uv = (abs(uv) / dot(uv, uv)) - s;
        uv = uv * rot;
        s *= 0.955;
    }
    let an = time + uv.x * 0.05;
    uv = uv * mat2(an.cos(), an.sin(), -an.sin(), an.cos());
    let ch_color = dc_rotations_hsv2rgb_smooth(vec3(time * 0.4 + uv.y * 0.1, 1.0, 1.0));
    let bg_color = vec3(dd * 0.1, dd * 0.2, dd * 0.1);
    mix(ch_color, bg_color, 0.0)
}

const DC_ROTATIONS_WGSL: &str = r#"
fn dc_rotations_hsv(c: vec3<f32>) -> vec3<f32> {
  let t2 = glsl_mod3(vec3<f32>(0.0, 4.0, 2.0) + c.x * 6.0, vec3<f32>(6.0)) - 3.0;
  var rgb = clamp(abs(t2) - 1.0, vec3<f32>(0.0), vec3<f32>(1.0));
  rgb = rgb * rgb * (vec3<f32>(3.0) - rgb * 2.0);
  return mix(vec3<f32>(1.0), rgb, c.y) * c.z;
}
fn dc_rotations_rgb(uv_in: vec2<f32>, levels: i32, time: f32, zoom: f32) -> vec3<f32> {
  var uv = uv_in * zoom;
  let dd = 1.0 - length(uv);
  var s = 0.5;
  let sa = sin(time * 0.1);
  let ca = cos(time * 0.1);
  let m = mat2x2<f32>(ca, -sa, sa, ca);
  for (var k = 0; k < levels; k++) {
    uv = abs(uv) / dot(uv, uv) - s;
    uv = uv * m;
    s *= 0.955;
  }
  let an = time + uv.x * 0.05;
  uv = uv * mat2x2<f32>(cos(an), sin(an), -sin(an), cos(an));
  let ch = dc_rotations_hsv(vec3<f32>(time * 0.4 + uv.y * 0.1, 1.0, 1.0));
  let bg = vec3<f32>(dd * 0.1, dd * 0.2, dd * 0.1);
  return mix(ch, bg, 0.0);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(4)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_rotations_rgb(uv, i32(D(0)), D(2), D(3)), i32(D(5)), D(6), D(7), i32(D(8)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_spacefold
// ---------------------------------------------------------------------------------------------

// d = [randomize, time, zoom, <dc base>(3..8)]
fn dc_spacefold_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 3);
    d[0] = limi(d[0], 0.0, 10000.0);
    d
}

fn dc_spacefold_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let (time, zoom) = (d[1], d[2]);
    let mut uv = vec2(uv.x * zoom, uv.y * zoom);
    let an = time / 30.0;
    let rot = mat2(an.cos(), -an.sin(), an.sin(), an.cos());
    for _ in 0..64 {
        uv = abs(uv);
        uv = uv * rot;
        uv = uv - vec2(0.5, 0.5);
        uv = uv * 1.03;
    }
    uv = pow(abs(sin(uv)), Vec2::splat(0.3));
    // iq palette
    let t = uv.x * uv.y * 1.9;
    Vec3::splat(0.5) + Vec3::splat(0.5) * cos((vec3(1.0, 1.0, 0.5) * t + vec3(0.8, 0.9, 0.3)) * 6.28318)
}

const DC_SPACEFOLD_WGSL: &str = r#"
fn dc_spacefold_rgb(uv_in: vec2<f32>, time: f32, zoom: f32) -> vec3<f32> {
  var uv = uv_in * zoom;
  let an = time / 30.0;
  let m = mat2x2<f32>(cos(an), -sin(an), sin(an), cos(an));
  for (var i = 0; i < 64; i++) {
    uv = abs(uv);
    uv = uv * m;
    uv = uv - vec2<f32>(0.5);
    uv = uv * 1.03;
  }
  uv = pow(abs(sin(uv)), vec2<f32>(0.3));
  let t = uv.x * uv.y * 1.9;
  return vec3<f32>(0.5) + vec3<f32>(0.5) * cos((vec3<f32>(1.0, 1.0, 0.5) * t + vec3<f32>(0.8, 0.9, 0.3)) * 6.28318);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_spacefold_rgb(uv, D(1), D(2)), i32(D(4)), D(5), D(6), i32(D(7)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_squares
// ---------------------------------------------------------------------------------------------

// d = [seed, time, N, direction, redF, greenF, blueF, <dc base>(7..12)]
fn dc_squares_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 7);
    d[1] = lim(d[1], 0.0, 1000.0);
    d[2] = limi(d[2], 1.0, 20.0);
    d[3] = limi(d[3], 0.0, 1.0);
    for k in 4..7 {
        d[k] = lim(d[k], 0.0, 5.0);
    }
    d
}

fn dc_squares_hit(p: Vec2, n: i32, scale: f64) -> bool {
    let mut coord_iter = p / scale;
    for _ in 0..n {
        let sectors = floor(coord_iter * 3.0);
        if sectors.x == 1.0 && sectors.y == 1.0 {
            return false; // make a hole
        }
        coord_iter = coord_iter * 3.0 - sectors; // map current sector to whole carpet
    }
    true
}

fn dc_squares_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let (time, n) = (d[1], d[2] as i32);
    let direction = if d[3] as i32 == 1 { 0.1 } else { -1.0 };
    let scale = 0.5f64.powf(glsl_mod(direction * time, 1.0));
    let coord_orig = glsl_mod(vec2(uv.x.abs(), uv.y.abs()), 1.0);
    let mut color = vec3(time.cos(), time.tan(), time.sin());
    for i in 0..4 {
        if dc_squares_hit(Vec2::splat(i as f64 * 0.1) + coord_orig, n, scale) {
            color = Vec3::splat(1.0) - color;
        }
    }
    vec3((color.x * d[4]).cos(), (color.y * d[5]).cos(), (color.z * d[6]).cos())
}

const DC_SQUARES_WGSL: &str = r#"
fn dc_squares_hit(p: vec2<f32>, n: i32, scale: f32) -> bool {
  var ci = p / scale;
  for (var i = 0; i < n; i++) {
    let sec = floor(ci * 3.0);
    if (sec.x == 1.0 && sec.y == 1.0) { return false; }
    ci = ci * 3.0 - sec;
  }
  return true;
}
fn dc_squares_rgb(uv: vec2<f32>, time: f32, n: i32, dir: i32, fr: f32, fg: f32, fb: f32) -> vec3<f32> {
  var direction = -1.0;
  if (dir == 1) { direction = 0.1; }
  let scale = pow(0.5, glsl_mod(direction * time, 1.0));
  let co = glsl_mod2(abs(uv), vec2<f32>(1.0));
  var color = vec3<f32>(cos(time), tan(time), sin(time));
  for (var i = 0; i < 4; i++) {
    if (dc_squares_hit(vec2<f32>(f32(i) * 0.1) + co, n, scale)) { color = vec3<f32>(1.0) - color; }
  }
  return vec3<f32>(cos(color.x * fr), cos(color.y * fg), cos(color.z * fb));
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(7)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_squares_rgb(uv, D(1), i32(D(2)), i32(D(3)), D(4), D(5), D(6));
dc_output(&v, AMOUNT, uv, col, i32(D(8)), D(9), D(10), i32(D(11)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_starsfield
// ---------------------------------------------------------------------------------------------

// d = [seed, time, zdistance, glow, <dc base>(4..9)]
fn dc_starsfield_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 4);
    d[2] = lim(d[2], 2.0, 150.0);
    d[3] = lim(d[3], 0.0, 1.0);
    d
}

fn dc_starsfield_rotate(a: f64) -> Mat2 {
    let (s, c) = a.sin_cos();
    mat2(c, s, -s, c)
}

fn dc_starsfield_hash11(p: f64) -> f64 {
    let mut p = fract(p * 35.35);
    p += dot(p, p + 45.85);
    fract(p * 7858.58)
}

fn dc_starsfield_hash21(p: Vec2) -> f64 {
    let mut p = fract(p * vec2(451.45, 231.95));
    p = p + dot(p, p + 78.78);
    fract(p.x * p.y)
}

fn dc_starsfield_hash22(p: Vec2) -> Vec2 {
    let t1 = vec3(p.x, p.y, p.x) * vec3(451.45, 231.95, 7878.5);
    let mut q = fract(t1);
    q = q + dot(q, q + 78.78);
    fract(vec2(q.x, q.z) * q.y)
}

fn dc_starsfield_layer(uv: Vec2, zdistance: f64) -> f64 {
    let uv = uv * 5.0;
    let i = floor(uv);
    let f = fract(uv) * zdistance - 1.0;
    let p = dc_starsfield_hash22(i) * 0.3;
    let d = length(f - p);
    let mut c = smoothstep(0.1 + 0.8 * dc_starsfield_hash21(i), 0.01, d);
    c *= (1.0 / d) * 0.2;
    c
}

fn dc_starsfield_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let (time, zdistance, glow) = (d[1], d[2], d[3]);
    let mut col = Vec3::splat(0.0);
    let mut uv = uv * dc_starsfield_rotate(time);
    uv = uv + vec2(time.cos(), time.sin()) * 2.0;
    // Java steps a double by 0.1: 11 iterations, the last one at 0.9999999999999999.
    let mut i = 0.0;
    while i < 1.0 {
        uv = uv * dc_starsfield_rotate(dc_starsfield_hash11(i) * 6.28);
        let t = fract(i - time);
        let s = smoothstep(0.0, 1.0, t);
        let mut f = smoothstep(0.0, 1.0, t);
        f *= smoothstep(1.0, 0.0, t);
        let k = dc_starsfield_hash22(vec2(i, i * 5.0)) * 0.1;
        let l = dc_starsfield_layer((uv - k) * s, zdistance);
        col = col + mix(Vec3::splat(0.0), Vec3::splat(1.0), l) * f;
        i += 0.1;
    }
    let t1 = glow * dc_starsfield_hash21(uv + time);
    col + Vec3::splat(t1)
}

const DC_STARSFIELD_WGSL: &str = r#"
fn dc_starsfield_rotate(a: f32) -> mat2x2<f32> { return mat2x2<f32>(cos(a), sin(a), -sin(a), cos(a)); }
fn dc_starsfield_hash11(p_in: f32) -> f32 {
  var p = fract(p_in * 35.35);
  p += p * (p + 45.85);
  return fract(p * 7858.58);
}
fn dc_starsfield_hash21(p_in: vec2<f32>) -> f32 {
  var p = fract(p_in * vec2<f32>(451.45, 231.95));
  p = p + dot(p, p + 78.78);
  return fract(p.x * p.y);
}
fn dc_starsfield_hash22(p: vec2<f32>) -> vec2<f32> {
  var q = fract(vec3<f32>(p.x, p.y, p.x) * vec3<f32>(451.45, 231.95, 7878.5));
  q = q + dot(q, q + 78.78);
  return fract(vec2<f32>(q.x, q.z) * q.y);
}
fn dc_starsfield_layer(uv_in: vec2<f32>, zd: f32) -> f32 {
  let uv = uv_in * 5.0;
  let i = floor(uv);
  let f = fract(uv) * zd - 1.0;
  let p = dc_starsfield_hash22(i) * 0.3;
  let d = length(f - p);
  var c = glsl_smoothstep(0.1 + 0.8 * dc_starsfield_hash21(i), 0.01, d);
  c *= (1.0 / d) * 0.2;
  return c;
}
fn dc_starsfield_rgb(uv_in: vec2<f32>, time: f32, zd: f32, glow: f32) -> vec3<f32> {
  var col = vec3<f32>(0.0);
  var uv = uv_in * dc_starsfield_rotate(time);
  uv = uv + vec2<f32>(cos(time), sin(time)) * 2.0;
  // 11 iterations like the Java double loop (last i just below 1).
  for (var k = 0; k < 11; k++) {
    let i = min(f32(k) * 0.1, 0.99999994);
    uv = uv * dc_starsfield_rotate(dc_starsfield_hash11(i) * 6.28);
    let t = fract(i - time);
    let s = glsl_smoothstep(0.0, 1.0, t);
    let f = s * glsl_smoothstep(1.0, 0.0, t);
    let kk = dc_starsfield_hash22(vec2<f32>(i, i * 5.0)) * 0.1;
    let l = dc_starsfield_layer((uv - kk) * s, zd);
    col = col + mix(vec3<f32>(0.0), vec3<f32>(1.0), l) * f;
  }
  return col + vec3<f32>(glow * dc_starsfield_hash21(uv + time));
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(4)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_starsfield_rgb(uv, D(1), D(2), D(3)), i32(D(5)), D(6), D(7), i32(D(8)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_sunflower
// ---------------------------------------------------------------------------------------------

// d = [zoom, step, N, Polar, Dots, GridX, GridY, <dc base>(7..12)]
fn dc_sunflower_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 7);
    d[0] = lim(d[0], 0.1, 50.0);
    d[1] = lim(d[1], -1.0, 1.0);
    d[2] = d[2] as i32 as f64;
    for k in 3..7 {
        d[k] = limi(d[k], 0.0, 1.0);
    }
    d
}

/// Distance to the segment a-b.
fn dc_sunflower_line(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let p = p - a;
    let b = b - a;
    let h = clamp(dot(p, b) / dot(b, b), 0.0, 1.0);
    length(p - b * h)
}

fn dc_sunflower_l(u: Vec2, x: f64, y: f64) -> f64 {
    dc_sunflower_line(u, floor(u + 0.5) - vec2(x, y), floor(u + 0.5) + vec2(x, y))
}

fn dc_sunflower_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let (zoom, r, n) = (d[0], d[1], d[2]);
    let (polar, dots, grid_x, grid_y) = (d[3] as i32, d[4] as i32, d[5] as i32, d[6] as i32);
    let s = |dd: f64, rr: f64| smoothstep(rr, 0.0, dd);
    let mut u = uv * zoom;
    let mut j = Vec2::splat(1.0);
    let mut o = Vec3::splat(0.0);
    let l = length(u);
    j.x = l * 6.28; // jacobian for dots
    u = vec2(glsl_atan2(u.y, u.x) / 6.28, l) * n; // polar coords
    let w = 0.1;
    u.x += r * (u.y + 0.5).floor(); // dot offsetting
    if grid_x == 1 {
        o.x = s(dc_sunflower_l(u, (if r < 0.0 { -1.0 } else { 1.0 }) - r, 1.0), w);
    }
    if grid_y == 1 {
        o.y = s(dc_sunflower_l(u, -r, 1.0), w);
    }
    if grid_x == 0 && grid_y == 0 {
        let hx = (if r < 0.0 { -0.5 } else { 0.5 }) - r;
        o.z = s(dc_sunflower_l(u, hx, 1.0), w);
        u.x += 0.5;
        o.z += s(dc_sunflower_l(u, hx, 1.0), w);
        u.x -= 0.5;
    }
    if polar == 1 {
        o = o + 0.5 * s(dc_sunflower_l(u, 1.0, 0.0), w);
    }
    if dots == 1 {
        u = fract(u + 0.5) - 0.5; // draw dots
        o = o + s(length(u * j) - 0.3, n * 2.0 / 2000.0);
    }
    o
}

const DC_SUNFLOWER_WGSL: &str = r#"
fn dc_sunflower_line(p_in: vec2<f32>, a: vec2<f32>, b_in: vec2<f32>) -> f32 {
  let p = p_in - a;
  let b = b_in - a;
  let h = clamp(dot(p, b) / dot(b, b), 0.0, 1.0);
  return length(p - b * h);
}
fn dc_sunflower_l(u: vec2<f32>, x: f32, y: f32) -> f32 {
  let c = floor(u + 0.5);
  return dc_sunflower_line(u, c - vec2<f32>(x, y), c + vec2<f32>(x, y));
}
fn dc_sunflower_rgb(uv: vec2<f32>, zoom: f32, r: f32, n: f32, polar: i32, dots: i32, gx: i32, gy: i32) -> vec3<f32> {
  var u = uv * zoom;
  var j = vec2<f32>(1.0);
  var o = vec3<f32>(0.0);
  let l = length(u);
  j.x = l * 6.28;
  u = vec2<f32>(glsl_atan2(u.y, u.x) / 6.28, l) * n;
  let w = 0.1;
  u.x += r * floor(u.y + 0.5);
  if (gx == 1) { o.x = glsl_smoothstep(w, 0.0, dc_sunflower_l(u, select(1.0, -1.0, r < 0.0) - r, 1.0)); }
  if (gy == 1) { o.y = glsl_smoothstep(w, 0.0, dc_sunflower_l(u, -r, 1.0)); }
  if (gx == 0 && gy == 0) {
    let hx = select(0.5, -0.5, r < 0.0) - r;
    o.z = glsl_smoothstep(w, 0.0, dc_sunflower_l(u, hx, 1.0));
    u.x += 0.5;
    o.z += glsl_smoothstep(w, 0.0, dc_sunflower_l(u, hx, 1.0));
    u.x -= 0.5;
  }
  if (polar == 1) { o = o + 0.5 * glsl_smoothstep(w, 0.0, dc_sunflower_l(u, 1.0, 0.0)); }
  if (dots == 1) {
    u = fract(u + 0.5) - 0.5;
    o = o + glsl_smoothstep(n * 2.0 / 2000.0, 0.0, length(u * j) - 0.3);
  }
  return o;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(7)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_sunflower_rgb(uv, D(0), D(1), D(2), i32(D(3)), i32(D(4)), i32(D(5)), i32(D(6)));
dc_output(&v, AMOUNT, uv, col, i32(D(8)), D(9), D(10), i32(D(11)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_tesla
// ---------------------------------------------------------------------------------------------

// d = [levels, style, seed, time, zoom, <dc base>(5..10)]
fn dc_tesla_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 5);
    d[0] = limi(d[0], 1.0, 25.0);
    d[1] = lim(d[1], 5.0, 100.0);
    d
}

fn dc_tesla_field21(mut p: Vec3, s: f64, levels: i32, style: f64, time: f64) -> f64 {
    let strength = 20.0 + 0.03 * (1.0e-6 + fract((time * 10.0).sin() * 4500.0)).ln();
    let mut accum = s * 3.0;
    let mut prev = 0.0;
    let mut tw = 0.0;
    for i in 0..levels {
        let mag = dot(p, p) * s.powf((time / 8.0).sin().abs());
        p = abs(p) / mag + vec3(-0.9, -0.23456 * time.sin().abs(), -1.0);
        let w = (-(i as f64) / style).exp();
        // first level: mag / 0 = inf -> exp(-inf) = 0 (like Java)
        accum += w * (-strength * (mag / prev).abs().powf(1.2)).exp();
        tw += w * w;
        prev = mag;
    }
    max(0.0, 6.0 * accum / tw - 2.0)
}

fn dc_tesla_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let pos = uv * d[4];
    Vec3::splat(dc_tesla_field21(Vec3::from_vec2(pos, 1.0), 0.5, d[0] as i32, d[1], d[3]))
}

const DC_TESLA_WGSL: &str = r#"
fn dc_tesla_field(p_in: vec3<f32>, s: f32, levels: i32, style: f32, time: f32) -> f32 {
  var p = p_in;
  let strength = 20.0 + 0.03 * log(1.0e-6 + fract(sin(time * 10.0) * 4500.0));
  var accum = s * 3.0;
  var prev = 0.0;
  var tw = 0.0;
  for (var i = 0; i < levels; i++) {
    let mag = dot(p, p) * pow(s, abs(sin(time / 8.0)));
    p = abs(p) / mag + vec3<f32>(-0.9, -0.23456 * abs(sin(time)), -1.0);
    let w = exp(-f32(i) / style);
    var e = 0.0;
    if (prev != 0.0) { e = exp(-strength * pow(abs(mag / prev), 1.2)); }
    accum += w * e;
    tw += w * w;
    prev = mag;
  }
  return max(0.0, 6.0 * accum / tw - 2.0);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(5)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = vec3<f32>(dc_tesla_field(vec3<f32>(uv * D(4), 1.0), 0.5, i32(D(0)), D(1), D(3)));
dc_output(&v, AMOUNT, uv, col, i32(D(6)), D(7), D(8), i32(D(9)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_tree
// ---------------------------------------------------------------------------------------------

// d = [levels, thicknes, style, shift, seed, time, zoom, <dc base>(7..12)]
fn dc_tree_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 7);
    d[0] = limi(d[0], 1.0, 25.0);
    d[1] = limi(d[1], -1500.0, 1500.0);
    d[2] = lim(d[2], 1.0, 100.0);
    d[3] = lim(d[3], -100.0, 100.0);
    d
}

fn dc_tree_po(v: Vec2) -> Vec2 {
    vec2(length(v), glsl_atan2(v.y, v.x))
}

fn dc_tree_ca(u: Vec2) -> Vec2 {
    vec2(u.y.cos(), u.y.sin()) * u.x
}

/// Java's `ln` modifies `p.x` of the vector passed in (the caller's `U`).
fn dc_tree_ln(p: &mut Vec2, a: Vec2, b: Vec2, time: f64) -> f64 {
    let mut r = dot(*p - a, b - a) / dot(b - a, b - a);
    r = clamp(r, 0.0, 1.0);
    p.x += (0.7 + 0.5 * (0.1 * time).sin())
        * 0.2
        * smoothstep(1.0, 0.0, abs(r * 2.0 - 1.0))
        * (3.14159 * (r - 4.0 * time)).sin();
    let t0 = *p - a;
    let t1 = (b - a) * r;
    (1.0 + 0.5 * r) * length(t0 - t1)
}

fn dc_tree_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let (levels, thickness, style, shift, time, zoom) = (d[0] as i32, d[1], d[2], d[3], d[5], d[6]);
    let mut u = uv * zoom;
    let mut r = 1e9;
    u.y += shift;
    let mut col = Vec3::splat(0.0);
    for k in 1..levels {
        let kf = k as f64;
        let s1 = 0.3 * ((2.0 * time).sin() + 0.5 * (4.53 * time).sin() + 0.1 * (12.2 * time).cos());
        let t = dc_tree_po(u) + vec2(0.0, 1.0) * s1;
        u = dc_tree_ca(t);
        r = min(r, dc_tree_ln(&mut u, Vec2::splat(0.0), vec2(0.0, 1.0), time));
        u.y -= 1.0;
        u.x = abs(u.x);
        u = u * (1.4 + 0.1 * time.sin() + 0.05 * (0.2455 * time).sin() * kf);
        u = dc_tree_po(u);
        u.y += 1.0
            + 0.5 * (0.553 * time).sin() * (time.sin() * kf).sin()
            + 0.1 * (0.4 * time).sin()
            + 0.05 * (0.554 * time).sin();
        u = dc_tree_ca(u);
        let s0 = (-thickness * r * r).exp() * style;
        col = col + sin(Vec3::splat(1.0) * s0);
    }
    col
}

const DC_TREE_WGSL: &str = r#"
fn dc_tree_po(v: vec2<f32>) -> vec2<f32> { return vec2<f32>(length(v), glsl_atan2(v.y, v.x)); }
fn dc_tree_ca(u: vec2<f32>) -> vec2<f32> { return vec2<f32>(cos(u.y), sin(u.y)) * u.x; }
fn dc_tree_ln(p: ptr<function, vec2<f32>>, a: vec2<f32>, b: vec2<f32>, time: f32) -> f32 {
  let r = clamp(dot(*p - a, b - a) / dot(b - a, b - a), 0.0, 1.0);
  (*p).x += (0.7 + 0.5 * sin(0.1 * time)) * 0.2 * glsl_smoothstep(1.0, 0.0, abs(r * 2.0 - 1.0))
    * sin(3.14159 * (r - 4.0 * time));
  return (1.0 + 0.5 * r) * length((*p - a) - (b - a) * r);
}
fn dc_tree_rgb(uv: vec2<f32>, levels: i32, thickness: f32, style: f32, shift: f32, time: f32, zoom: f32) -> vec3<f32> {
  var u = uv * zoom;
  var r = 1.0e9;
  u.y += shift;
  var col = vec3<f32>(0.0);
  for (var k = 1; k < levels; k++) {
    let kf = f32(k);
    let s1 = 0.3 * (sin(2.0 * time) + 0.5 * sin(4.53 * time) + 0.1 * cos(12.2 * time));
    u = dc_tree_ca(dc_tree_po(u) + vec2<f32>(0.0, s1));
    r = min(r, dc_tree_ln(&u, vec2<f32>(0.0), vec2<f32>(0.0, 1.0), time));
    u.y -= 1.0;
    u.x = abs(u.x);
    u = u * (1.4 + 0.1 * sin(time) + 0.05 * sin(0.2455 * time) * kf);
    u = dc_tree_po(u);
    u.y += 1.0 + 0.5 * sin(0.553 * time) * sin(sin(time) * kf) + 0.1 * sin(0.4 * time) + 0.05 * sin(0.554 * time);
    u = dc_tree_ca(u);
    let s0 = exp(-thickness * r * r) * style;
    col = col + sin(vec3<f32>(s0));
  }
  return col;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(7)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_tree_rgb(uv, i32(D(0)), D(1), D(2), D(3), D(5), D(6));
dc_output(&v, AMOUNT, uv, col, i32(D(8)), D(9), D(10), i32(D(11)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_truchet
// ---------------------------------------------------------------------------------------------

// d = [type, zoom, <dc base>(2..7)]
fn dc_truchet_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = dcb_prep(p, 2);
    d[0] = limi(d[0], 0.0, 2.0);
    d
}

fn dc_truchet_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let st = uv * d[1];
    let ipos = floor(st);
    let fpos = fract(st);
    let tile = glsl_truchet_pattern(fpos, glsl_random(ipos));
    let icolor = match d[0] as i32 {
        // Maze
        0 => smoothstep(tile.x - 0.3, tile.x, tile.y) - smoothstep(tile.x, tile.x + 0.3, tile.y),
        // Circles
        1 => {
            let l0 = length(tile);
            let l1 = length(tile - Vec2::splat(1.0));
            (step(l0, 0.6) - step(l0, 0.4)) + (step(l1, 0.6) - step(l1, 0.4))
        }
        // Truchet (2 triangles)
        2 => step(tile.x, tile.y),
        _ => 0.0,
    };
    Vec3::splat(icolor)
}

const DC_TRUCHET_WGSL: &str = r#"
fn dc_truchet_rgb(uv: vec2<f32>, ty: i32, zoom: f32) -> vec3<f32> {
  let st = uv * zoom;
  let tile = glsl_truchet_pattern(fract(st), glsl_random(floor(st)));
  var ic = 0.0;
  if (ty == 0) {
    ic = glsl_smoothstep(tile.x - 0.3, tile.x, tile.y) - glsl_smoothstep(tile.x, tile.x + 0.3, tile.y);
  } else if (ty == 1) {
    let l0 = length(tile);
    let l1 = length(tile - vec2<f32>(1.0));
    ic = (step(l0, 0.6) - step(l0, 0.4)) + (step(l1, 0.6) - step(l1, 0.4));
  } else if (ty == 2) {
    ic = step(tile.x, tile.y);
  }
  return vec3<f32>(ic);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(2)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_truchet_rgb(uv, i32(D(0)), D(1)), i32(D(3)), D(4), D(5), i32(D(6)));
"#;

// ---------------------------------------------------------------------------------------------
// dc_triangle (Xyrus02)
// ---------------------------------------------------------------------------------------------

// d = [scatter_area, zero_edges, A, xx, xy, yx, yy, ox, oy]
fn dc_triangle_prep(p: &[f64], c: &Affine) -> Vec<f64> {
    let zero_edges = limit_value(ftoi(p[1]), 0, 1) as f64;
    let a = if p[0] < -1.0 {
        -1.0
    } else if p[0] > 1.0 {
        1.0
    } else {
        p[0]
    };
    vec![p[0], zero_edges, a, c.c00, c.c10, -c.c01, -c.c11, c.c20, c.c21]
}

fn dc_triangle_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let big_a = d[2];
    // set up triangle
    let (xx, xy) = (d[3], d[4]); // X
    let (yx, yy) = (d[5], d[6]); // Y
    let (ox, oy) = (d[7], d[8]); // O
    let (px, py) = (a.x - ox, a.y - oy); // P

    // dot products
    let dot00 = xx * xx + xy * xy;
    let dot01 = xx * yx + xy * yy;
    let dot02 = xx * px + xy * py;
    let dot11 = yx * yx + yy * yy;
    let dot12 = yx * px + yy * py;

    // barycentric coordinates
    let denom = dot00 * dot11 - dot01 * dot01;
    let num_u = dot11 * dot02 - dot01 * dot12;
    let num_v = dot00 * dot12 - dot01 * dot02;
    let mut u = num_u / denom;
    let mut w = num_v / denom;
    let mut inside = false;
    let mut f = 1.0;

    if u + w > 1.0 {
        // case A - point escapes edge XY
        f = -1.0;
        if u > w {
            u = if u > 1.0 { 1.0 } else { u };
            w = 1.0 - u;
        } else {
            w = if w > 1.0 { 1.0 } else { w };
            u = 1.0 - w;
        }
    } else if u < 0.0 || w < 0.0 {
        // case B - point escapes either edge OX or OY
        u = if u < 0.0 {
            0.0
        } else if u > 1.0 {
            1.0
        } else {
            u
        };
        w = if w < 0.0 {
            0.0
        } else if w > 1.0 {
            1.0
        } else {
            w
        };
    } else {
        // case C - point is in triangle
        inside = true;
    }

    if d[1] as i32 == 1 && !inside {
        u = 0.0;
        w = 0.0;
    } else if inside {
        u += ctx.random() * big_a * f;
        w += ctx.random() * big_a * f;
        u = u.clamp(-1.0, 1.0);
        w = w.clamp(-1.0, 1.0);
        if u + w > 1.0 && big_a > 0.0 {
            if u > w {
                u = if u > 1.0 { 1.0 } else { u };
                w = 1.0 - u;
            } else {
                w = if w > 1.0 { 1.0 } else { w };
                u = 1.0 - w;
            }
        }
    }

    v.x += amount * (ox + u * xx + w * yx);
    v.y += amount * (oy + u * xy + w * yy);
    ctx.pass_z(a, v, amount);
    v.color = fmod((u + w).abs(), 1.0);
}

const DC_TRIANGLE_WGSL: &str = r#"
let xx = D(3);
let xy = D(4);
let yx = D(5);
let yy = D(6);
let ox = D(7);
let oy = D(8);
let px = a.x - ox;
let py = a.y - oy;
let dot00 = xx * xx + xy * xy;
let dot01 = xx * yx + xy * yy;
let dot02 = xx * px + xy * py;
let dot11 = yx * yx + yy * yy;
let dot12 = yx * px + yy * py;
let denom = dot00 * dot11 - dot01 * dot01;
var u = (dot11 * dot02 - dot01 * dot12) / denom;
var w = (dot00 * dot12 - dot01 * dot02) / denom;
var inside = false;
var f = 1.0;
if (u + w > 1.0) {
  f = -1.0;
  if (u > w) { u = min(u, 1.0); w = 1.0 - u; } else { w = min(w, 1.0); u = 1.0 - w; }
} else if (u < 0.0 || w < 0.0) {
  u = clamp(u, 0.0, 1.0);
  w = clamp(w, 0.0, 1.0);
} else {
  inside = true;
}
let big_a = D(2);
if (i32(D(1)) == 1 && !inside) {
  u = 0.0;
  w = 0.0;
} else if (inside) {
  u = u + rnd() * big_a * f;
  w = w + rnd() * big_a * f;
  u = clamp(u, -1.0, 1.0);
  w = clamp(w, -1.0, 1.0);
  if (u + w > 1.0 && big_a > 0.0) {
    if (u > w) { u = min(u, 1.0); w = 1.0 - u; } else { w = min(w, 1.0); u = 1.0 - w; }
  }
}
v.x += AMOUNT * (ox + u * xx + w * yx);
v.y += AMOUNT * (oy + u * xy + w * yy);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
v.color = fmod(abs(u + w), 1.0);
"#;

// ---------------------------------------------------------------------------------------------
// dc_triTile (eralex61 FiveFold extension)
// ---------------------------------------------------------------------------------------------

const DC_TRITILE_PHI: f64 = 1.618_033_988_749_895;
const DC_TRITILE_1_PHI: f64 = 1.0 / DC_TRITILE_PHI;

#[derive(Clone, Copy, Debug, Default)]
struct DcTriTileTri {
    ty: i32,
    col: f64,
    p1: Vec2,
    p2: Vec2,
    p3: Vec2,
}

const fn dc_tritile_tri(ty: i32, col: f64, p1: Vec2, p2: Vec2, p3: Vec2) -> DcTriTileTri {
    DcTriTileTri { ty, col, p1, p2, p3 }
}

/// The `*Init` functions: start triangle (type 0 / all zero if `t` is not 1 or 2) and `alfa`.
fn dc_tritile_init(tiling: i32, t: i32, col1: f64, col2: f64) -> (DcTriTileTri, f64) {
    let z = Vec2::splat(0.0);
    let pick = |a: DcTriTileTri, b: DcTriTileTri| match t {
        1 => a,
        2 => b,
        _ => DcTriTileTri::default(),
    };
    let base = |ty: i32, col: f64, p2: Vec2| dc_tritile_tri(ty, col, z, p2, vec2(2.0 * p2.x, 0.0));
    match tiling {
        1 => {
            // fiveFold
            let a = base(1, col1, vec2((M_2PI / 5.0).cos(), (M_2PI / 5.0).sin()));
            let b = base(2, col2, vec2((M_PI / 5.0).cos(), (M_PI / 5.0).sin()));
            (pick(a, b), 2.0 * (M_2PI / 5.0).cos())
        }
        2 => {
            // trihex
            let p2 = vec2((M_PI / 3.0).cos(), 0.0);
            let p3 = vec2(0.0, (M_PI / 3.0).sin());
            (pick(dc_tritile_tri(1, col1, z, p2, p3), dc_tritile_tri(2, col2, z, p2, p3)), 0.0)
        }
        3 => {
            // equiThirds
            let a = base(1, col1, vec2((M_PI / 3.0).cos(), (M_PI / 3.0).sin()));
            let b = base(2, col2, vec2((M_PI / 6.0).cos(), (M_PI / 6.0).sin()));
            (pick(a, b), 1.0 / 3.0)
        }
        4 => {
            // ort
            let p2 = vec2((M_2PI / 5.0).cos(), (M_2PI / 5.0).sin());
            (pick(base(1, col1, p2), base(2, col2, p2)), 2.0 * (M_2PI / 5.0).cos())
        }
        5 => {
            // pinwheel
            let a = dc_tritile_tri(1, col1, z, vec2(0.0, 1.0), vec2(2.0, 0.0));
            let b = dc_tritile_tri(2, col2, z, vec2(0.0, 2.0), vec2(1.0, 0.0));
            (pick(a, b), 0.0)
        }
        6 => {
            // gold
            let s = DC_TRITILE_1_PHI.sqrt();
            let a = dc_tritile_tri(1, col1, z, vec2(DC_TRITILE_1_PHI, s), vec2(DC_TRITILE_1_PHI, 0.0));
            let b = dc_tritile_tri(2, col2, z, vec2(0.0, s), vec2(1.0, 0.0));
            (pick(a, b), DC_TRITILE_1_PHI * DC_TRITILE_1_PHI)
        }
        _ => (DcTriTileTri::default(), 0.0),
    }
}

/// d = [type, t, col1, col2, on, itt, eq, depth, on1, itt1, eq1, depth1, on2, itt2, eq2, depth2,
///      orig.type, orig.col, orig.x1, orig.y1, orig.x2, orig.y2, orig.x3, orig.y3, alfa]
fn dc_tritile_prep(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = p.to_vec();
    d[0] = limit_value(p[0] as i32, 1, 6) as f64;
    d[1] = ftoi(p[1]) as f64;
    for k in [4, 8, 12] {
        d[k] = limit_value(p[k] as i32, 0, 1) as f64;
    }
    for k in [5, 7, 9, 11, 13, 15] {
        d[k] = ftoi(p[k]) as f64;
    }
    for k in [6, 10, 14] {
        d[k] = limit_value(p[k] as i32, 0, 2) as f64;
    }
    let (o, alfa) = dc_tritile_init(d[0] as i32, d[1] as i32, d[2], d[3]);
    d.extend([o.ty as f64, o.col, o.p1.x, o.p1.y, o.p2.x, o.p2.y, o.p3.x, o.p3.y, alfa]);
    d
}

/// Java `discretNoise` (int arithmetic wraps).
fn dc_tritile_noise(x: i32) -> f64 {
    let n = x.wrapping_shl(13) ^ x;
    let t = n
        .wrapping_mul(n.wrapping_mul(n).wrapping_mul(15731).wrapping_add(789221))
        .wrapping_add(1376312589);
    (t & 0x7fff_ffff) as f64 / 0x7fff_ffff as f64
}

struct DcTriTileCfg {
    tiling: i32,
    col1: f64,
    col2: f64,
    alfa: f64,
}

/// `devis(in, out)`: one substitution step; `out` is returned unchanged when `in.type` has no
/// rule (Java leaves the object untouched).
fn dc_tritile_devis(c: &DcTriTileCfg, ctx: &mut Ctx, i: &DcTriTileTri, out: DcTriTileTri) -> DcTriTileTri {
    let (c1, c2, alfa) = (c.col1, c.col2, c.alfa);
    let t = dc_tritile_tri;
    match c.tiling {
        // fiveFold
        1 => match i.ty {
            1 => {
                let p4 = i.p2 + (i.p3 - i.p2) * alfa;
                if ctx.random() < alfa * alfa { t(1, c1, i.p3, i.p1, p4) } else { t(2, c2, i.p2, p4, i.p1) }
            }
            2 => {
                let p4 = i.p3 + (i.p1 - i.p3) * alfa;
                let p5 = i.p3 + (i.p2 - i.p3) * alfa;
                let w = ctx.random();
                if w < alfa * alfa {
                    t(2, c2, i.p2, p4, i.p1)
                } else if w < 2.0 * alfa * alfa {
                    t(2, c2, i.p3, p5, p4)
                } else {
                    t(1, c1, i.p2, p4, p5)
                }
            }
            _ => out,
        },
        // trihex
        2 => {
            let p4 = (i.p1 + i.p3) / 2.0;
            let p5 = (i.p2 + i.p3) / 2.0;
            let p6 = (i.p1 + p5) / 2.0;
            let same = if i.ty == 1 { c1 } else { c2 };
            let other = if i.ty == 1 { c2 } else { c1 };
            let w = ctx.random();
            if w < 0.25 {
                t(i.ty, same, p4, p5, i.p3)
            } else if w < 0.5 {
                t(3 - i.ty, other, p4, p5, i.p1)
            } else if w < 0.75 {
                t(3 - i.ty, other, p6, p5, i.p2)
            } else {
                t(i.ty, same, p6, i.p1, i.p2)
            }
        }
        // equiThirds
        3 => match i.ty {
            1 => {
                let p4 = i.p1 + (i.p3 - i.p1) * 0.5;
                let p5 = p4 + (i.p2 - p4) * alfa;
                let w = ctx.random();
                if w < alfa {
                    t(2, c2, i.p2, p5, i.p1)
                } else if w < 2.0 * alfa {
                    t(2, c2, i.p3, p5, i.p2)
                } else {
                    t(2, c2, i.p1, p5, i.p3)
                }
            }
            2 => {
                let p4 = i.p1 + (i.p3 - i.p1) * alfa;
                let p5 = i.p3 + (i.p1 - i.p3) * alfa;
                let w = ctx.random();
                if w < alfa {
                    t(2, c2, i.p2, p4, i.p1)
                } else if w < 2.0 * alfa {
                    t(2, c2, i.p3, p5, i.p2)
                } else {
                    t(1, c1, p4, i.p2, p5)
                }
            }
            _ => out,
        },
        // ort
        4 => {
            let p4 = i.p2 + (i.p1 - i.p2) * alfa;
            let p5 = i.p1 + (i.p2 - i.p1) * alfa;
            let p6 = i.p2 + (i.p3 - i.p2) * alfa;
            let w = ctx.random();
            if w < 0.3 {
                t(2, c2, p4, i.p2, p6)
            } else if w < 0.63 {
                t(3 - i.ty, if i.ty == 1 { c2 } else { c1 }, p6, i.p1, p5)
            } else {
                t(1, c1, i.p3, i.p1, p6)
            }
        }
        // pinwheel
        5 => match i.ty {
            1 => {
                let p4 = i.p2 + (i.p3 - i.p2) * 0.2;
                let p5 = i.p2 + (i.p3 - i.p2) * 0.6;
                let p6 = i.p1 + (i.p3 - i.p1) * 0.5;
                let p7 = i.p1 + (p4 - i.p1) * 0.5;
                match (ctx.random() * 5.0) as i32 {
                    0 => t(2, c2, p4, i.p1, i.p2),
                    1 => t(2, c2, p7, p6, i.p1),
                    2 => t(1, c1, p7, p4, p6),
                    3 => t(1, c1, p5, p6, p4),
                    4 => t(2, c2, p5, i.p3, p6),
                    _ => out,
                }
            }
            2 => {
                let p4 = i.p3 + (i.p2 - i.p3) * 0.2;
                let p5 = i.p3 + (i.p2 - i.p3) * 0.6;
                let p6 = i.p1 + (i.p2 - i.p1) * 0.5;
                let p7 = i.p1 + (p4 - i.p1) * 0.5;
                match (ctx.random() * 5.0) as i32 {
                    0 => t(1, c1, p4, i.p3, i.p1),
                    1 => t(1, c1, p7, i.p1, p6),
                    2 => t(2, c2, p7, p6, p4),
                    3 => t(2, c2, p5, p4, p6),
                    4 => t(1, c1, p5, p6, i.p2),
                    _ => out,
                }
            }
            _ => out,
        },
        // gold
        6 => match i.ty {
            1 => {
                let p4 = i.p1 + (i.p2 - i.p1) * alfa;
                let p5 = i.p2 + (i.p1 - i.p2) * alfa;
                let p6 = i.p3 + (i.p2 - i.p3) * alfa;
                let p7 = i.p1 + (i.p3 - i.p1) * alfa;
                let w = ctx.random();
                if w < 0.146 {
                    t(1, c1, i.p1, p4, p7)
                } else if w < 0.382 {
                    t(2, c2, p4, p7, p6)
                } else if w < 0.618 {
                    t(2, c2, i.p3, p6, p7)
                } else if w < 0.764 {
                    t(1, c1, p4, p6, p5)
                } else {
                    t(2, c2, p5, p6, i.p2)
                }
            }
            2 => {
                let p4 = i.p2 + (i.p1 - i.p2) * alfa;
                let p5 = i.p2 + (i.p3 - i.p2) * alfa;
                let p6 = i.p3 + (i.p2 - i.p3) * alfa;
                let p7 = i.p1 + (i.p3 - i.p1) * alfa;
                let p8 = i.p3 + (i.p1 - i.p3) * alfa;
                let p9 = p4 + (p7 - p4) * alfa;
                let p10 = p7 + (p4 - p7) * alfa;
                let w = ctx.random();
                if w < 0.146 {
                    t(2, c2, p4, i.p2, p5)
                } else if w < 0.236 {
                    t(1, c1, p4, p5, p9)
                } else if w < 0.382 {
                    t(2, c2, p10, i.p1, p4)
                } else if w < 0.472 {
                    t(1, c1, p7, i.p1, p10)
                } else if w < 0.618 {
                    t(2, c2, p9, p5, p7)
                } else if w < 0.764 {
                    t(2, c2, p6, p5, p7)
                } else if w < 0.854 {
                    t(1, c1, p7, p6, p8)
                } else {
                    t(2, c2, p8, p6, i.p3)
                }
            }
            _ => out,
        },
        _ => out,
    }
}

/// `triangle.blur`: a random point inside the triangle.
fn dc_tritile_blur(tr: &DcTriTileTri, ctx: &mut Ctx) -> Vec2 {
    let (q1, q2, q3) = (tr.p1, tr.p2, tr.p3);
    // sort the corners by x
    let (a, b, c) = if q1.x <= q2.x && q1.x <= q3.x {
        if q2.x <= q3.x { (q1, q2, q3) } else { (q1, q3, q2) }
    } else if q2.x <= q1.x && q2.x <= q3.x {
        if q1.x <= q3.x { (q2, q1, q3) } else { (q2, q3, q1) }
    } else if q1.x <= q2.x {
        (q3, q1, q2)
    } else {
        (q3, q2, q1)
    };
    let r1 = ctx.random();
    let r2 = ctx.random();
    let mut res;
    if a.x == b.x && b.x == c.x {
        let (v1, v2) = if a.y <= b.y && a.y <= c.y {
            (a.y, if c.y > b.y { b.y } else { c.y })
        } else if b.y <= a.y && b.y <= c.y {
            (b.y, if c.y > a.y { a.y } else { c.y })
        } else {
            (c.y, if b.y > a.y { a.y } else { b.y })
        };
        res = vec2(a.x, v1 + r1 * (v2 - v1));
    } else if a.x == b.x {
        let p4 = (a + c) / 2.0;
        let p5 = c + b - p4;
        let p6 = a + b - p4;
        let q = c + (p4 - c) * r1;
        let q0 = p5 + (b - p5) * r1;
        res = q + (q0 - q) * r2;
        let w = c.y + (b.y - c.y) / (b.x - c.x) * (res.x - c.x);
        let upper = b.y > c.y + (c.y - a.y) / (c.x - a.x) * (b.x - c.x);
        if (upper && res.y > w) || (!upper && res.y < w) {
            let q = p4 + (a - p4) * r1;
            let q0 = b + (p6 - b) * r1;
            res = q0 + (q - q0) * r2;
        }
    } else {
        let p4 = (a + c) / 2.0;
        let p5 = a + b - p4;
        let p6 = c + b - p4;
        let q = a + (p4 - a) * r1;
        let q0 = p5 + (b - p5) * r1;
        res = q + (q0 - q) * r2;
        let w = a.y + (b.y - a.y) / (b.x - a.x) * (res.x - a.x);
        let upper = b.y > a.y + (c.y - a.y) / (c.x - a.x) * (b.x - a.x);
        if (upper && res.y > w) || (!upper && res.y < w) {
            let q = p4 + (c - p4) * r1;
            let q0 = b + (p6 - b) * r1;
            res = q0 + (q - q0) * r2;
        }
    }
    if a == b {
        res = a + (c - a) * r1;
    }
    if a == c {
        res = a + (b - a) * r1;
    }
    if b == c {
        res = a + (b - a) * r1;
    }
    res
}

fn dc_tritile_apply(d: &[f64], ctx: &mut Ctx, _a: &mut Pt, v: &mut Pt, amount: f64) {
    let cfg = DcTriTileCfg { tiling: d[0] as i32, col1: d[2], col2: d[3], alfa: d[24] };
    let ip = |k: usize| d[k] as i32;
    let orig = dc_tritile_tri(ip(16), d[17], vec2(d[18], d[19]), vec2(d[20], d[21]), vec2(d[22], d[23]));

    let mut t1 = orig;
    t1.col = 0.25;
    let mut t2 = orig;
    t2.col = 0.0;

    let (on, itt, eq, depth) = (ip(4), ip(5), ip(6), ip(7));
    let mut n = itt;
    let mut k = 1;
    while n > 0 {
        t2 = dc_tritile_devis(&cfg, ctx, &t1, t2);
        t1 = t2;
        if on == 0 {
            if k == itt && t1.ty != eq {
                n += depth;
            }
        } else if k >= itt && k < itt + depth && t1.ty != eq {
            n += 1;
        }
        n -= 1;
        k += 1;
    }

    if t1.ty == 1 || t1.ty == 2 {
        let base = if t1.ty == 1 { 8 } else { 12 };
        let (on_x, itt_x, eq_x, depth_x) = (ip(base), ip(base + 1), ip(base + 2), ip(base + 3));
        let mut m: i32 = 0;
        n = itt_x;
        k = 1;
        while n > 0 {
            t2 = dc_tritile_devis(&cfg, ctx, &t1, t2);
            t1 = t2;
            m = m.wrapping_mul(2).wrapping_add(t2.ty - 1);
            if on_x == 0 {
                if k == itt_x && t1.ty != eq_x {
                    n += depth_x;
                }
            } else if k >= itt_x && k < itt_x + depth_x && t1.ty != eq_x {
                n += 1;
            }
            n -= 1;
            k += 1;
        }
        t1.col = dc_tritile_noise(m.wrapping_add(if base == 8 { 43 } else { 31 }));
    }

    let p = dc_tritile_blur(&t2, ctx);
    v.x += amount * p.x;
    v.y += amount * p.y;
    v.color = fmod(t1.col + t2.col, 1.0);
}

const DC_TRITILE_WGSL: &str = r#"
struct dc_triTile_T { ty: i32, col: f32, p1: vec2<f32>, p2: vec2<f32>, p3: vec2<f32> }
fn dc_triTile_noise(x: i32) -> f32 {
  let n = (x << 13u) ^ x;
  return f32((n * (n * n * 15731 + 789221) + 1376312589) & 0x7fffffff) / 2147483647.0;
}
fn dc_triTile_devis(tiling: i32, i: dc_triTile_T, o: dc_triTile_T, c1: f32, c2: f32, alfa: f32) -> dc_triTile_T {
  if (tiling == 1) {
    if (i.ty == 1) {
      let p4 = i.p2 + (i.p3 - i.p2) * alfa;
      if (rnd() < alfa * alfa) { return dc_triTile_T(1, c1, i.p3, i.p1, p4); }
      return dc_triTile_T(2, c2, i.p2, p4, i.p1);
    }
    if (i.ty == 2) {
      let p4 = i.p3 + (i.p1 - i.p3) * alfa;
      let p5 = i.p3 + (i.p2 - i.p3) * alfa;
      let w = rnd();
      if (w < alfa * alfa) { return dc_triTile_T(2, c2, i.p2, p4, i.p1); }
      if (w < 2.0 * alfa * alfa) { return dc_triTile_T(2, c2, i.p3, p5, p4); }
      return dc_triTile_T(1, c1, i.p2, p4, p5);
    }
    return o;
  }
  if (tiling == 2) {
    let p4 = (i.p1 + i.p3) / 2.0;
    let p5 = (i.p2 + i.p3) / 2.0;
    let p6 = (i.p1 + p5) / 2.0;
    let same = select(c2, c1, i.ty == 1);
    let other = select(c1, c2, i.ty == 1);
    let w = rnd();
    if (w < 0.25) { return dc_triTile_T(i.ty, same, p4, p5, i.p3); }
    if (w < 0.5) { return dc_triTile_T(3 - i.ty, other, p4, p5, i.p1); }
    if (w < 0.75) { return dc_triTile_T(3 - i.ty, other, p6, p5, i.p2); }
    return dc_triTile_T(i.ty, same, p6, i.p1, i.p2);
  }
  if (tiling == 3) {
    if (i.ty == 1) {
      let p4 = i.p1 + (i.p3 - i.p1) * 0.5;
      let p5 = p4 + (i.p2 - p4) * alfa;
      let w = rnd();
      if (w < alfa) { return dc_triTile_T(2, c2, i.p2, p5, i.p1); }
      if (w < 2.0 * alfa) { return dc_triTile_T(2, c2, i.p3, p5, i.p2); }
      return dc_triTile_T(2, c2, i.p1, p5, i.p3);
    }
    if (i.ty == 2) {
      let p4 = i.p1 + (i.p3 - i.p1) * alfa;
      let p5 = i.p3 + (i.p1 - i.p3) * alfa;
      let w = rnd();
      if (w < alfa) { return dc_triTile_T(2, c2, i.p2, p4, i.p1); }
      if (w < 2.0 * alfa) { return dc_triTile_T(2, c2, i.p3, p5, i.p2); }
      return dc_triTile_T(1, c1, p4, i.p2, p5);
    }
    return o;
  }
  if (tiling == 4) {
    let p4 = i.p2 + (i.p1 - i.p2) * alfa;
    let p5 = i.p1 + (i.p2 - i.p1) * alfa;
    let p6 = i.p2 + (i.p3 - i.p2) * alfa;
    let w = rnd();
    if (w < 0.3) { return dc_triTile_T(2, c2, p4, i.p2, p6); }
    if (w < 0.63) { return dc_triTile_T(3 - i.ty, select(c1, c2, i.ty == 1), p6, i.p1, p5); }
    return dc_triTile_T(1, c1, i.p3, i.p1, p6);
  }
  if (tiling == 5) {
    if (i.ty == 1) {
      let p4 = i.p2 + (i.p3 - i.p2) * 0.2;
      let p5 = i.p2 + (i.p3 - i.p2) * 0.6;
      let p6 = i.p1 + (i.p3 - i.p1) * 0.5;
      let p7 = i.p1 + (p4 - i.p1) * 0.5;
      let w = i32(rnd() * 5.0);
      if (w == 0) { return dc_triTile_T(2, c2, p4, i.p1, i.p2); }
      if (w == 1) { return dc_triTile_T(2, c2, p7, p6, i.p1); }
      if (w == 2) { return dc_triTile_T(1, c1, p7, p4, p6); }
      if (w == 3) { return dc_triTile_T(1, c1, p5, p6, p4); }
      if (w == 4) { return dc_triTile_T(2, c2, p5, i.p3, p6); }
      return o;
    }
    if (i.ty == 2) {
      let p4 = i.p3 + (i.p2 - i.p3) * 0.2;
      let p5 = i.p3 + (i.p2 - i.p3) * 0.6;
      let p6 = i.p1 + (i.p2 - i.p1) * 0.5;
      let p7 = i.p1 + (p4 - i.p1) * 0.5;
      let w = i32(rnd() * 5.0);
      if (w == 0) { return dc_triTile_T(1, c1, p4, i.p3, i.p1); }
      if (w == 1) { return dc_triTile_T(1, c1, p7, i.p1, p6); }
      if (w == 2) { return dc_triTile_T(2, c2, p7, p6, p4); }
      if (w == 3) { return dc_triTile_T(2, c2, p5, p4, p6); }
      if (w == 4) { return dc_triTile_T(1, c1, p5, p6, i.p2); }
      return o;
    }
    return o;
  }
  if (tiling == 6) {
    if (i.ty == 1) {
      let p4 = i.p1 + (i.p2 - i.p1) * alfa;
      let p5 = i.p2 + (i.p1 - i.p2) * alfa;
      let p6 = i.p3 + (i.p2 - i.p3) * alfa;
      let p7 = i.p1 + (i.p3 - i.p1) * alfa;
      let w = rnd();
      if (w < 0.146) { return dc_triTile_T(1, c1, i.p1, p4, p7); }
      if (w < 0.382) { return dc_triTile_T(2, c2, p4, p7, p6); }
      if (w < 0.618) { return dc_triTile_T(2, c2, i.p3, p6, p7); }
      if (w < 0.764) { return dc_triTile_T(1, c1, p4, p6, p5); }
      return dc_triTile_T(2, c2, p5, p6, i.p2);
    }
    if (i.ty == 2) {
      let p4 = i.p2 + (i.p1 - i.p2) * alfa;
      let p5 = i.p2 + (i.p3 - i.p2) * alfa;
      let p6 = i.p3 + (i.p2 - i.p3) * alfa;
      let p7 = i.p1 + (i.p3 - i.p1) * alfa;
      let p8 = i.p3 + (i.p1 - i.p3) * alfa;
      let p9 = p4 + (p7 - p4) * alfa;
      let p10 = p7 + (p4 - p7) * alfa;
      let w = rnd();
      if (w < 0.146) { return dc_triTile_T(2, c2, p4, i.p2, p5); }
      if (w < 0.236) { return dc_triTile_T(1, c1, p4, p5, p9); }
      if (w < 0.382) { return dc_triTile_T(2, c2, p10, i.p1, p4); }
      if (w < 0.472) { return dc_triTile_T(1, c1, p7, i.p1, p10); }
      if (w < 0.618) { return dc_triTile_T(2, c2, p9, p5, p7); }
      if (w < 0.764) { return dc_triTile_T(2, c2, p6, p5, p7); }
      if (w < 0.854) { return dc_triTile_T(1, c1, p7, p6, p8); }
      return dc_triTile_T(2, c2, p8, p6, i.p3);
    }
    return o;
  }
  return o;
}
fn dc_triTile_blur(t: dc_triTile_T) -> vec2<f32> {
  var a = t.p1;
  var b = t.p2;
  var c = t.p3;
  if (t.p1.x <= t.p2.x && t.p1.x <= t.p3.x) {
    if (t.p2.x > t.p3.x) { b = t.p3; c = t.p2; }
  } else if (t.p2.x <= t.p1.x && t.p2.x <= t.p3.x) {
    a = t.p2;
    if (t.p1.x <= t.p3.x) { b = t.p1; c = t.p3; } else { b = t.p3; c = t.p1; }
  } else {
    a = t.p3;
    if (t.p1.x <= t.p2.x) { b = t.p1; c = t.p2; } else { b = t.p2; c = t.p1; }
  }
  let r1 = rnd();
  let r2 = rnd();
  var res: vec2<f32>;
  if (a.x == b.x && b.x == c.x) {
    var v1: f32;
    var v2: f32;
    if (a.y <= b.y && a.y <= c.y) {
      v1 = a.y;
      v2 = select(c.y, b.y, c.y > b.y);
    } else if (b.y <= a.y && b.y <= c.y) {
      v1 = b.y;
      v2 = select(c.y, a.y, c.y > a.y);
    } else {
      v1 = c.y;
      v2 = select(b.y, a.y, b.y > a.y);
    }
    res = vec2<f32>(a.x, v1 + r1 * (v2 - v1));
  } else if (a.x == b.x) {
    let p4 = (a + c) / 2.0;
    let p5 = c + b - p4;
    let p6 = a + b - p4;
    let q = c + (p4 - c) * r1;
    let q0 = p5 + (b - p5) * r1;
    res = q + (q0 - q) * r2;
    let w = c.y + (b.y - c.y) / (b.x - c.x) * (res.x - c.x);
    let upper = b.y > c.y + (c.y - a.y) / (c.x - a.x) * (b.x - c.x);
    if ((upper && res.y > w) || (!upper && res.y < w)) {
      let qq = p4 + (a - p4) * r1;
      let qq0 = b + (p6 - b) * r1;
      res = qq0 + (qq - qq0) * r2;
    }
  } else {
    let p4 = (a + c) / 2.0;
    let p5 = a + b - p4;
    let p6 = c + b - p4;
    let q = a + (p4 - a) * r1;
    let q0 = p5 + (b - p5) * r1;
    res = q + (q0 - q) * r2;
    let w = a.y + (b.y - a.y) / (b.x - a.x) * (res.x - a.x);
    let upper = b.y > a.y + (c.y - a.y) / (c.x - a.x) * (b.x - a.x);
    if ((upper && res.y > w) || (!upper && res.y < w)) {
      let qq = p4 + (c - p4) * r1;
      let qq0 = b + (p6 - b) * r1;
      res = qq0 + (qq - qq0) * r2;
    }
  }
  if (all(a == b)) { res = a + (c - a) * r1; }
  if (all(a == c)) { res = a + (b - a) * r1; }
  if (all(b == c)) { res = a + (b - a) * r1; }
  return res;
}
@body
let tiling = i32(D(0));
let c1 = D(2);
let c2 = D(3);
let alfa = D(24);
let orig = dc_triTile_T(i32(D(16)), D(17), vec2<f32>(D(18), D(19)), vec2<f32>(D(20), D(21)), vec2<f32>(D(22), D(23)));
var t1 = orig;
t1.col = 0.25;
var t2 = orig;
t2.col = 0.0;
let on = i32(D(4));
let itt = i32(D(5));
let eq = i32(D(6));
let depth = i32(D(7));
var n = itt;
var k = 1;
var guard = 0;
while (n > 0 && guard < 4096) {
  t2 = dc_triTile_devis(tiling, t1, t2, c1, c2, alfa);
  t1 = t2;
  if (on == 0) {
    if (k == itt && t1.ty != eq) { n += depth; }
  } else if (k >= itt && k < itt + depth && t1.ty != eq) {
    n += 1;
  }
  n -= 1;
  k += 1;
  guard += 1;
}
if (t1.ty == 1 || t1.ty == 2) {
  let is1 = t1.ty == 1;
  let on_x = i32(select(D(12), D(8), is1));
  let itt_x = i32(select(D(13), D(9), is1));
  let eq_x = i32(select(D(14), D(10), is1));
  let depth_x = i32(select(D(15), D(11), is1));
  var m = 0;
  n = itt_x;
  k = 1;
  guard = 0;
  while (n > 0 && guard < 4096) {
    t2 = dc_triTile_devis(tiling, t1, t2, c1, c2, alfa);
    t1 = t2;
    m = 2 * m + t2.ty - 1;
    if (on_x == 0) {
      if (k == itt_x && t1.ty != eq_x) { n += depth_x; }
    } else if (k >= itt_x && k < itt_x + depth_x && t1.ty != eq_x) {
      n += 1;
    }
    n -= 1;
    k += 1;
    guard += 1;
  }
  t1.col = dc_triTile_noise(m + select(31, 43, is1));
}
let p = dc_triTile_blur(t2);
v.x += AMOUNT * p.x;
v.y += AMOUNT * p.y;
v.color = fmod(t1.col + t2.col, 1.0);
"#;

// ---------------------------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    dc_def!(
        "dc_mandelbox2D",
        3,
        dc_params![
            ParamDef::range("zoom", 7.0, 0.1, 50.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
        ],
        dc_mandelbox2d_prep,
        dc_mandelbox2d_rgb,
        DC_MANDELBOX2D_WGSL
    ),
    dc_def!(
        "dc_menger",
        2,
        dc_params![ParamDef::range("zoom", 1.0, 0.1, 10.0), ParamDef::int("level", 3.0, 1.0, 5.0)],
        dc_menger_prep,
        dc_menger_rgb,
        DC_MENGER_WGSL
    ),
    dc_def!(
        "dc_moebiuslog",
        7,
        dc_params![
            ParamDef::range("zoom", 7.0, 0.1, 50.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::int("Log", 1.0, 0.0, 1.0),
            ParamDef::int("Moebius", 1.0, 0.0, 1.0),
            ParamDef::range("scale", 5.0, 1.0, 10.0),
            ParamDef::range("angle", 5.0, 1.0, 10.0),
        ],
        dc_moebiuslog_prep,
        dc_moebiuslog_rgb,
        DC_MOEBIUSLOG_WGSL
    ),
    dc_def!(
        "dc_pentatiles",
        3,
        dc_params![
            ParamDef::range("zoom", 20.0, 1.0, 100.0),
            ParamDef::int("border", 1.0, 0.0, 1.0),
            ParamDef::range("width", 0.1, 0.01, 0.25),
        ],
        dc_pentatiles_prep,
        dc_pentatiles_rgb,
        DC_PENTATILES_WGSL
    ),
    dc_def!(
        "dc_poincaredisc",
        7,
        dc_params![
            ParamDef::range("zoom", 2.0, 0.1, 10.0),
            ParamDef::int("MaxIter", 10.0, 0.0, 20.0),
            ParamDef::int("N", 4.0, 3.0, 20.0),
            ParamDef::int("P", 5.0, 3.0, 20.0),
            ParamDef::int("checkers0", 1.0, 0.0, 1.0),
            ParamDef::int("checkers1", 1.0, 0.0, 1.0),
            ParamDef::int("checkers2", 1.0, 0.0, 1.0),
        ],
        dc_poincaredisc_prep,
        dc_poincaredisc_rgb,
        DC_POINCAREDISC_WGSL
    ),
    VarDef {
        name: "dc_portal",
        params: dc_params![
            ParamDef::int("seed", 1000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::range("waves", 5.0, 0.1, 20.0),
            ParamDef::range("zoom", 15.0, 0.1, 50.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let p = dcb_uv(d, 4, ctx, a);
            let color = dc_portal_rgb(d, vec2(p.x * d[3], p.y * d[3]));
            // Java assigns x/y (`pVarTP.x = pAmount * x`) instead of adding.
            v.x = 0.0;
            v.y = 0.0;
            dcb_finish(d, 4, ctx, v, amount, p, color);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| dcb_prep(p, 4)),
        wgsl: DC_PORTAL_WGSL,
    },
    dc_def!(
        "dc_quadtree",
        5,
        dc_params![
            ParamDef::range("zoom", 1.0, 0.1, 50.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::range("width", 1000.0, 100.0, 1000.0),
            ParamDef::int("levels", 30.0, 1.0, 50.0),
        ],
        dc_quadtree_prep,
        dc_quadtree_rgb,
        DC_QUADTREE_WGSL
    ),
    dc_def!(
        "dc_randomoctree",
        8,
        dc_params![
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::int("steps", 10.0, 3.0, 100.0),
            ParamDef::range("rotLR", 0.0, 0.0, 1.0),
            ParamDef::range("rotUD", 0.0, 0.0, 1.0),
            ParamDef::int("grid", 0.0, 0.0, 1.0),
            ParamDef::int("borders", 1.0, 0.0, 1.0),
            ParamDef::int("bBorders", 1.0, 0.0, 1.0),
        ],
        dc_randomoctree_prep,
        dc_randomoctree_rgb,
        DC_RANDOMOCTREE_WGSL
    ),
    dc_def!(
        "dc_rotations",
        4,
        dc_params![
            ParamDef::int("levels", 20.0, 1.0, 50.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::range("zoom", 4.0, 0.1, 20.0),
        ],
        dc_rotations_prep,
        dc_rotations_rgb,
        DC_ROTATIONS_WGSL
    ),
    dc_def!(
        "dc_spacefold",
        3,
        dc_params![
            ParamDef::int("randomize", 1000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::range("zoom", 2.5, 0.1, 20.0),
        ],
        dc_spacefold_prep,
        dc_spacefold_rgb,
        DC_SPACEFOLD_WGSL
    ),
    dc_def!(
        "dc_squares",
        7,
        dc_params![
            ParamDef::int("seed", 5000.0, 0.0, 10000.0),
            ParamDef::range("time", 85.5, 0.0, 1000.0),
            ParamDef::int("N", 3.0, 1.0, 20.0),
            ParamDef::int("direction", 1.0, 0.0, 1.0),
            ParamDef::range("redF", 1.8, 0.0, 5.0),
            ParamDef::range("greenF", 1.9, 0.0, 5.0),
            ParamDef::range("blueF", 2.2, 0.0, 5.0),
        ],
        dc_squares_prep,
        dc_squares_rgb,
        DC_SQUARES_WGSL
    ),
    dc_def!(
        "dc_starsfield",
        4,
        dc_params![
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::range("zdistance", 2.0, 2.0, 150.0),
            ParamDef::range("glow", 1.0, 0.0, 1.0),
        ],
        dc_starsfield_prep,
        dc_starsfield_rgb,
        DC_STARSFIELD_WGSL
    ),
    dc_def!(
        "dc_sunflower",
        7,
        dc_params![
            ParamDef::range("zoom", 7.0, 0.1, 50.0),
            ParamDef::range("step", 0.1, -1.0, 1.0),
            ParamDef::int("N", 30.0, 1.0, 100.0),
            ParamDef::int("Polar", 1.0, 0.0, 1.0),
            ParamDef::int("Dots", 1.0, 0.0, 1.0),
            ParamDef::int("GridX", 1.0, 0.0, 1.0),
            ParamDef::int("GridY", 1.0, 0.0, 1.0),
        ],
        dc_sunflower_prep,
        dc_sunflower_rgb,
        DC_SUNFLOWER_WGSL
    ),
    dc_def!(
        "dc_tesla",
        5,
        dc_params![
            ParamDef::int("levels", 20.0, 1.0, 25.0),
            ParamDef::range("style", 10.0, 5.0, 100.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::range("zoom", 4.0, 0.1, 20.0),
        ],
        dc_tesla_prep,
        dc_tesla_rgb,
        DC_TESLA_WGSL
    ),
    dc_def!(
        "dc_tree",
        7,
        dc_params![
            ParamDef::int("levels", 20.0, 1.0, 25.0),
            ParamDef::int("thicknes", 1000.0, -1500.0, 1500.0),
            ParamDef::range("style", 50.0, 1.0, 100.0),
            ParamDef::range("shift", 1.5, -100.0, 100.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::range("zoom", 4.0, 0.1, 20.0),
        ],
        dc_tree_prep,
        dc_tree_rgb,
        DC_TREE_WGSL
    ),
    dc_def!(
        "dc_truchet",
        2,
        dc_params![ParamDef::int("type", 0.0, 0.0, 2.0), ParamDef::range("zoom", 10.0, 0.1, 50.0)],
        dc_truchet_prep,
        dc_truchet_rgb,
        DC_TRUCHET_WGSL
    ),
    VarDef {
        name: "dc_triangle",
        params: &[ParamDef::range("scatter_area", 0.0, -1.0, 1.0), ParamDef::int("zero_edges", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: dc_triangle_apply,
        prepare: Some(dc_triangle_prep),
        wgsl: DC_TRIANGLE_WGSL,
    },
    // Java initialises col1/col2/on*/itt*/eq*/depth* randomly; typical fixed values are used here.
    VarDef {
        name: "dc_triTile",
        params: &[
            ParamDef::int("type", 1.0, 1.0, 6.0),
            ParamDef::int("t", 2.0, 1.0, 2.0),
            ParamDef::range("col1", 0.25, 0.0, 1.0),
            ParamDef::range("col2", 0.75, 0.0, 1.0),
            ParamDef::int("on", 0.0, 0.0, 1.0),
            ParamDef::int("itt", 3.0, 0.0, 10.0),
            ParamDef::int("eq", 1.0, 0.0, 2.0),
            ParamDef::int("depth", 1.0, 0.0, 5.0),
            ParamDef::int("on1", 0.0, 0.0, 1.0),
            ParamDef::int("itt1", 2.0, 0.0, 10.0),
            ParamDef::int("eq1", 1.0, 0.0, 2.0),
            ParamDef::int("depth1", 1.0, 0.0, 5.0),
            ParamDef::int("on2", 0.0, 0.0, 1.0),
            ParamDef::int("itt2", 2.0, 0.0, 10.0),
            ParamDef::int("eq2", 2.0, 0.0, 2.0),
            ParamDef::int("depth2", 1.0, 0.0, 5.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: dc_tritile_apply,
        prepare: Some(dc_tritile_prep),
        wgsl: DC_TRITILE_WGSL,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "dc_mandelbox2D",
        "dc_menger",
        "dc_moebiuslog",
        "dc_pentatiles",
        "dc_poincaredisc",
        "dc_portal",
        "dc_quadtree",
        "dc_randomoctree",
        "dc_rotations",
        "dc_spacefold",
        "dc_squares",
        "dc_starsfield",
        "dc_sunflower",
        "dc_tesla",
        "dc_tree",
        "dc_triangle",
        "dc_triTile",
        "dc_truchet",
    ];

    fn find(n: &str) -> &'static VarDef {
        DEFS.iter().find(|d| d.name == n).unwrap()
    }

    fn defaults(def: &VarDef) -> Vec<f64> {
        def.params.iter().map(|p| p.default).collect()
    }

    #[test]
    fn all_names_present() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert!(DEFS.iter().any(|d| d.name == *n), "missing {n}");
        }
        assert!(DEFS.iter().all(|d| !d.wgsl.is_empty()));
    }

    fn check(def: &VarDef, params: &[f64], c: &Affine) {
        let data = def.prepare_data(params, c);
        assert!(data.iter().all(|x| x.is_finite()), "{}: prepared data {:?}", def.name, data);
        let pts = [(0.3, -0.7, 0.2), (1.5, 2.0, -0.5), (-0.1, 0.05, 0.0), (-0.2, -0.4, 1.0), (0.01, 0.37, 0.0)];
        for preserve_z in [false, true] {
            let mut ctx = Ctx::new(42, preserve_z);
            for (i, p) in ctx.palette.iter_mut().enumerate() {
                *p = [i as u8, (255 - i) as u8, (i / 2) as u8];
            }
            for &(x, y, z) in &pts {
                for _ in 0..8 {
                    let mut a = Pt { x, y, z, ..Default::default() };
                    let mut v = Pt::default();
                    (def.apply)(&data, &mut ctx, &mut a, &mut v, 0.75);
                    assert!(!a.is_bad() && !v.is_bad(), "{}: ({x},{y},{z}) -> {:?}", def.name, v);
                    assert!(v.color.is_finite(), "{}: color {:?}", def.name, v);
                    if v.rgb_color {
                        for c in [v.red, v.green, v.blue] {
                            assert!((0.0..=255.0).contains(&c), "{}: rgb {:?}", def.name, v);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn finite_on_sample_points() {
        let coefs = [Affine::default(), Affine { c00: 0.8, c01: -0.3, c10: 0.4, c11: 0.9, c20: 0.35, c21: -0.6 }];
        for def in DEFS {
            for c in &coefs {
                check(def, &[], c);
            }
            // DC variations: also with ColorOnly = 1 (pattern sampled at the affine point) and the
            // gradient / greyscale outputs.
            if let Some(n) = def.params.iter().position(|p| p.name == "ColorOnly") {
                for gradient in [0.0, 1.0, 2.0] {
                    let mut p = defaults(def);
                    p[n] = 1.0;
                    p[n + 1] = gradient;
                    p[n + 2] = 0.5;
                    check(def, &p, &coefs[0]);
                }
            }
        }
    }

    #[test]
    fn tritile_all_tilings() {
        let def = find("dc_triTile");
        for tiling in 1..=6 {
            for t in [1.0, 2.0] {
                for on in [0.0, 1.0] {
                    let mut p = defaults(def);
                    p[0] = tiling as f64;
                    p[1] = t;
                    p[4] = on;
                    p[8] = on;
                    p[12] = on;
                    check(def, &p, &Affine::default());
                }
            }
        }
    }

    #[test]
    fn spot_values() {
        let run = |n: &str, params: &[f64], x: f64, y: f64| {
            let def = find(n);
            let data = def.prepare_data(params, &Affine::default());
            let mut ctx = Ctx::new(1, false);
            let mut a = Pt { x, y, ..Default::default() };
            let mut v = Pt::default();
            (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
            v
        };
        // dc_triangle with the identity affine: a point inside the triangle O, X = (1, 0),
        // Y = (0, -1) maps onto itself (scatter_area = 0).
        let v = run("dc_triangle", &[], 0.25, -0.25);
        assert!((v.x - 0.25).abs() < 1e-12 && (v.y + 0.25).abs() < 1e-12, "{v:?}");
        assert!((v.color - 0.5).abs() < 1e-12);

        // dc_menger (ColorOnly = 1): the centre of the carpet is a hole (black); points are passed
        // through.
        let p = [1.0, 3.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let v = run("dc_menger", &p, 0.0, 0.0);
        assert!(v.rgb_color && v.red == 0.0 && v.green == 0.0 && v.blue == 0.0);
        let v = run("dc_menger", &p, 0.5, 0.1);
        assert!(v.rgb_color && v.red == 255.0 && (v.x - 0.5).abs() < 1e-12 && (v.y - 0.1).abs() < 1e-12);

        // dc_portal assigns x/y instead of adding.
        let def = find("dc_portal");
        let mut p = defaults(def);
        p[4] = 1.0;
        let data = def.prepare_data(&p, &Affine::default());
        let mut ctx = Ctx::new(1, false);
        let mut a = Pt { x: 0.2, y: 0.3, ..Default::default() };
        let mut v = Pt { x: 5.0, y: 5.0, ..Default::default() };
        (def.apply)(&data, &mut ctx, &mut a, &mut v, 2.0);
        assert!((v.x - 0.4).abs() < 1e-12 && (v.y - 0.6).abs() < 1e-12);

        // poincaredisc precalc for N = 4, P = 5
        let d = find("dc_poincaredisc").prepare_data(&[], &Affine::default());
        assert!(d[12] > 1.0 && d[13] > 0.0);
        assert!((d[14] - (M_PI / 4.0).sin()).abs() < 1e-12 && (d[15] + (M_PI / 4.0).cos()).abs() < 1e-12);
    }

    #[test]
    fn tritile_noise() {
        assert_eq!(dc_tritile_noise(0), 1376312589.0 / 2147483647.0);
        for x in [-100, -1, 1, 31, 43, 1000, i32::MAX / 3] {
            let n = dc_tritile_noise(x);
            assert!((0.0..=1.0).contains(&n), "{x}: {n}");
        }
    }
}
