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

//! Variations ported from the JWildfire sources (batch w06).
//!
//! GLSL-style hashes (`fract(sin(..) * big)`) are evaluated in f32 on the GPU and therefore
//! give a different (but equally distributed) pattern than the f64 CPU code.

// The Java sources use truncated constants (3.14159, 0.7071, 6.283, ...); kept for fidelity.
#![allow(clippy::approx_constant)]

use super::glsl::*;
use super::noise;
#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

/// `java.util.Random(seed)`: the first `N` values of `nextDouble()`.
fn w06_java_random_doubles<const N: usize>(seed: i32) -> [f64; N] {
    let mut r = crate::rng::JavaRandom::new(seed as i64);
    std::array::from_fn(|_| r.next_double())
}

/// `Tools.limitValue` of the Java `setParameter` methods: `(index, min, max, integer)`;
/// integer parameters are truncated like the Java `(int)` cast.
fn w06_limit(p: &[f64], lim: &[(usize, f64, f64, bool)]) -> Vec<f64> {
    let mut d = p.to_vec();
    for &(i, lo, hi, int) in lim {
        let x = limit_value(d[i], lo, hi);
        d[i] = if int { x.trunc() } else { x };
    }
    d
}

/// Limits of the `DC_BaseFunc` parameters starting at `n`
/// (`ColorOnly, Gradient, scale_z, offset_z, reset_z`).
fn w06_dc_limit(d: &mut [f64], n: usize) {
    for (i, hi) in [(n, 1.0), (n + 1, 2.0), (n + 4, 1.0)] {
        d[i] = limit_value(d[i], 0.0, hi).trunc();
    }
}

/// `DC_BaseFunc` sample position: the affine point (`ColorOnly == 1`) or a random point in
/// `[-half, half)^2`. `full == false`: `random() - 0.5`, `full == true`: `2 * random() - 1`.
fn w06_dc_uv(d: &[f64], n: usize, ctx: &mut Ctx, a: &Pt, full: bool) -> Vec2 {
    if d[n] as i32 == 1 {
        vec2(a.x, a.y)
    } else if full {
        let x = 2.0 * ctx.random() - 1.0;
        let y = 2.0 * ctx.random() - 1.0;
        vec2(x, y)
    } else {
        let x = ctx.random() - 0.5;
        let y = ctx.random() - 0.5;
        vec2(x, y)
    }
}

/// The `DC_BaseFunc` tail with the base parameters starting at `n`.
fn w06_dc_tail(d: &[f64], n: usize, ctx: &Ctx, v: &mut Pt, amount: f64, uv: Vec2, color: Vec3) {
    dc_output(
        ctx,
        v,
        amount,
        uv,
        color,
        d[n + 1] as i32,
        d[n + 2],
        d[n + 3],
        d[n + 4] as i32,
    );
}

/// Common tail of the `cut_*` variations: hidden samples are moved to `(0, 0)` and flagged.
#[allow(clippy::too_many_arguments)]
fn w06_cut_out(
    ctx: &Ctx,
    a: &Pt,
    v: &mut Pt,
    amount: f64,
    x: f64,
    y: f64,
    center: f64,
    hide: bool,
) {
    v.do_hide = hide;
    let (x, y) = if hide { (0.0, 0.0) } else { (x, y) };
    v.x = amount * (x - center);
    v.y = amount * (y - center);
    ctx.pass_z(a, v, amount);
}

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

// ---------------------------------------------------------------------------------------------
// cut_tstruchet (CutTriScaleTruchetFunc)
// ---------------------------------------------------------------------------------------------

fn tstruchet_hash22(p: Vec2) -> Vec2 {
    let n = dot(p, vec2(57.0, 27.0));
    fract(vec2(262144.0, 32768.0) * n.sin())
}

fn tstruchet_dist(p: Vec2, shape: i32) -> f64 {
    if shape == 0 {
        return length(p);
    }
    let p = abs(p);
    match shape {
        1 => p.x.max(p.y).max((p.x + p.y) * 0.7071),
        2 => {
            let p2 = p * 0.8660254 + p.yx() * 0.5;
            p2.x.max(p2.y).max(p.x.max(p.y))
        }
        _ => {
            let (s, c) = (3.14159f64 / 8.0).sin_cos();
            let p2 = p * mat2(c, -s, s, c);
            let c = p2.x.max(p2.y).max((p2.x + p2.y) * 0.7071);
            c.max(p.x.max(p.y).max((p.x + p.y) * 0.7071))
        }
    }
}

fn tstruchet_color(op: Vec2, shape: i32, levels: i32, arcs: i32) -> f64 {
    // Java quirk: `if (levels==1) rndTh[0].x = .5; rndTh[1].x = 1.;` -- the second assignment is
    // unconditional, so the "mid border holes" branch (`rndTh[1].x < .99`) is dead code.
    let th0 = if levels == 1 { 0.5 } else { 0.35 };
    let rnd_th = [th0, 1.0, 1.0];
    let mut dx = 1e5;
    let mut dim = 2.0;
    for (k, &th) in rnd_th.iter().enumerate() {
        let ip = floor(op * dim);
        let mut rnd = tstruchet_hash22(ip);
        if rnd.x < th {
            let mut p = op - (ip + 0.5) / dim;
            rnd = fract(rnd * 27.63 + (k * 57 + 1) as f64);
            if rnd.y >= 0.5 {
                p.y = -p.y;
            }
            let aw = 0.5 / 3.0 / dim;
            let c1 = (tstruchet_dist(p - Vec2::splat(0.5) / dim, shape) - 0.5 / dim).abs() - aw;
            let c2 = if arcs == 1 {
                (tstruchet_dist(p - Vec2::splat(-0.5) / dim, shape) - 0.5 / dim).abs() - aw
            } else if fract(rnd.y * 57.53 + 0.47) < 0.35 {
                let c = tstruchet_dist(p - vec2(-0.5, 0.0) / dim, shape) - aw;
                c.min(tstruchet_dist(p - vec2(0.0, -0.5) / dim, shape) - aw)
            } else {
                (tstruchet_dist(p - Vec2::splat(-0.5) / dim, shape) - 0.5 / dim).abs() - aw
            };
            dx = c1.min(c2);
            if k == 1 {
                dx = -dx;
            }
            p = abs(p) - 0.5 / dim;
            if k < 2 && th0 < 0.99 {
                dx = dx.min(tstruchet_dist(p, shape) - aw);
            }
            dx -= 0.01;
            break;
        }
        dim *= 2.0;
    }
    let fo = 10.0 / 1e3 / dim.sqrt();
    let mut sh = (0.75 - dx * 10.0).max(0.0);
    sh *= clamp(-(dx * 6.283 * 18.0).sin() + 0.75, -0.25, 1.0) + 0.25;
    // background and drop shadow are black, the pattern colour is white: one channel suffices
    let mut col = 0.0;
    col = mix(col, 0.0, (1.0 - smoothstep(0.0, fo * 5.0, dx)) * 0.75);
    col = mix(col, 0.0, 1.0 - smoothstep(0.0, fo, dx));
    col = mix(col, sh, 1.0 - smoothstep(0.0, fo, dx + 0.015));
    mix(
        col,
        0.0,
        1.0 - smoothstep(0.0, fo, (dx + 0.1).max(-(dx + 0.14))),
    )
}

const CUT_TSTRUCHET_WGSL: &str = "fn cut_tstruchet_hash22(p: vec2<f32>) -> vec2<f32> {
  let n = dot(p, vec2<f32>(57.0, 27.0));
  return fract(vec2<f32>(262144.0, 32768.0) * sin(n));
}
fn cut_tstruchet_dist(p_in: vec2<f32>, shape: i32) -> f32 {
  if (shape == 0) { return length(p_in); }
  let p = abs(p_in);
  if (shape == 1) { return max(max(p.x, p.y), (p.x + p.y) * 0.7071); }
  if (shape == 2) {
    let p2 = p * 0.8660254 + p.yx * 0.5;
    return max(max(p2.x, p2.y), max(p.x, p.y));
  }
  let ang = 3.14159 / 8.0;
  let p2 = p * mat2x2<f32>(cos(ang), -sin(ang), sin(ang), cos(ang));
  let c = max(max(p2.x, p2.y), (p2.x + p2.y) * 0.7071);
  return max(c, max(max(p.x, p.y), (p.x + p.y) * 0.7071));
}
fn cut_tstruchet_color(op: vec2<f32>, shape: i32, levels: i32, arcs: i32) -> f32 {
  var th0 = 0.35;
  if (levels == 1) { th0 = 0.5; }
  var dx = 1.0e5;
  var dim = 2.0;
  for (var k = 0; k < 3; k++) {
    let ip = floor(op * dim);
    var r = cut_tstruchet_hash22(ip);
    var th = 1.0;
    if (k == 0) { th = th0; }
    if (r.x < th) {
      var p = op - (ip + 0.5) / dim;
      r = fract(r * 27.63 + f32(k * 57 + 1));
      if (r.y >= 0.5) { p.y = -p.y; }
      let aw = 0.5 / 3.0 / dim;
      let c1 = abs(cut_tstruchet_dist(p - vec2<f32>(0.5) / dim, shape) - 0.5 / dim) - aw;
      var c2: f32;
      if (arcs == 1) {
        c2 = abs(cut_tstruchet_dist(p - vec2<f32>(-0.5) / dim, shape) - 0.5 / dim) - aw;
      } else if (fract(r.y * 57.53 + 0.47) < 0.35) {
        c2 = cut_tstruchet_dist(p - vec2<f32>(-0.5, 0.0) / dim, shape) - aw;
        c2 = min(c2, cut_tstruchet_dist(p - vec2<f32>(0.0, -0.5) / dim, shape) - aw);
      } else {
        c2 = abs(cut_tstruchet_dist(p - vec2<f32>(-0.5) / dim, shape) - 0.5 / dim) - aw;
      }
      dx = min(c1, c2);
      if (k == 1) { dx = -dx; }
      p = abs(p) - 0.5 / dim;
      if (k < 2 && th0 < 0.99) { dx = min(dx, cut_tstruchet_dist(p, shape) - aw); }
      dx -= 0.01;
      break;
    }
    dim *= 2.0;
  }
  let fo = 10.0 / 1.0e3 / sqrt(dim);
  var sh = max(0.75 - dx * 10.0, 0.0);
  sh *= clamp(-sin(dx * 6.283 * 18.0) + 0.75, -0.25, 1.0) + 0.25;
  var col = 0.0;
  col = mix(col, 0.0, (1.0 - glsl_smoothstep(0.0, fo * 5.0, dx)) * 0.75);
  col = mix(col, 0.0, 1.0 - glsl_smoothstep(0.0, fo, dx));
  col = mix(col, sh, 1.0 - glsl_smoothstep(0.0, fo, dx + 0.015));
  return mix(col, 0.0, 1.0 - glsl_smoothstep(0.0, fo, max(dx + 0.1, -(dx + 0.14))));
}
@body
let mode = i32(D(1));
var x = 0.0;
var y = 0.0;
var pc = 0.0;
var hide = true;
{
  if (mode == 0) { x = a.x; y = a.y; pc = 0.0; } else { x = rnd(); y = rnd(); pc = 0.5; }
  let op = vec2<f32>(x * D(5), y * D(5)) + vec2<f32>(D(7), D(8));
  let color = cut_tstruchet_color(op, i32(D(2)), i32(D(3)), i32(D(4)));
  if (i32(D(6)) == 0) { hide = color > 0.30; } else { hide = color <= 0.30; }
}
v.hide = 0.0;
if (hide) { x = 0.0; y = 0.0; v.hide = 1.0; }
v.x = AMOUNT * (x - pc);
v.y = AMOUNT * (y - pc);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

// ---------------------------------------------------------------------------------------------
// cut_vasarely
// ---------------------------------------------------------------------------------------------

fn vasarely_color(x: f64, y: f64, zoom: f64, size: f64) -> f64 {
    let uv = vec2(x, y) * zoom;
    let l = length(uv);
    let b = (2.0 - l / size).max(0.0);
    let term = |color: f64| {
        color - 0.5 - (l * (glsl_atan2(uv.y, uv.x) + 1.57 + 4.0 * b * b).sin() * 150.0).sin()
    };
    let mut color = 0.0;
    color = if uv.x.abs() < 1.0 { term(color) } else { color };
    color -= if uv.x.abs() < 1.0 { term(color) } else { color };
    color
}

const CUT_VASARELY_WGSL: &str =
    "fn cut_vasarely_color(x: f32, y: f32, zoom: f32, size: f32) -> f32 {
  let uv = vec2<f32>(x, y) * zoom;
  let l = length(uv);
  let b = max(0.0, 2.0 - l / size);
  let s = sin(l * sin(glsl_atan2(uv.y, uv.x) + 1.57 + 4.0 * b * b) * 150.0);
  var color = 0.0;
  if (abs(uv.x) < 1.0) { color = color - 0.5 - s; }
  if (abs(uv.x) < 1.0) { color -= color - 0.5 - s; } else { color -= color; }
  return color;
}
@body
let mode = i32(D(0));
var x = 0.0;
var y = 0.0;
var hide = true;
{
  if (mode == 0) { x = a.x; y = a.y; } else { x = 2.0 * rnd() - 1.0; y = 2.0 * rnd() - 1.0; }
  let color = cut_vasarely_color(x, y, D(1), D(3));
  if (i32(D(2)) == 0) { hide = color < 0.1; } else { hide = color >= 0.1; }
}
v.hide = 0.0;
if (hide) { x = 0.0; y = 0.0; v.hide = 1.0; }
v.x = AMOUNT * x;
v.y = AMOUNT * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

// ---------------------------------------------------------------------------------------------
// cut_web
// ---------------------------------------------------------------------------------------------

fn web_colour(p: Vec2, time: f64, thick: f64) -> f64 {
    let fac = 10.0 / (2.0 * M_PI);
    let mut ep = vec2(length(p).ln(), glsl_atan2(p.y, p.x));
    let d = ep.x * 0.05 * fac;
    ep = ep + vec2(-ep.y, ep.x) * 4.0;
    let si = smoothstep(
        Vec2::splat(-0.5 * d),
        Vec2::splat(0.5 * d),
        abs(glsl_mod(ep * fac + time, 2.0) - 1.0) - thick,
    );
    1.0 - (1.0 - (si.x + si.y))
}

const CUT_WEB_WGSL: &str = "fn cut_web_colour(p: vec2<f32>, time: f32, thick: f32) -> f32 {
  let fac = 10.0 / (2.0 * PI);
  var ep = vec2<f32>(log(length(p)), glsl_atan2(p.y, p.x));
  let d = ep.x * 0.05 * fac;
  ep = ep + vec2<f32>(-ep.y, ep.x) * 4.0;
  let si = glsl_smoothstep2(vec2<f32>(-0.5 * d), vec2<f32>(0.5 * d),
                            abs(glsl_mod2(ep * fac + time, vec2<f32>(2.0)) - 1.0) - thick);
  return 1.0 - (1.0 - (si.x + si.y));
}
@body
let mode = i32(D(2));
var x = 0.0;
var y = 0.0;
var hide = true;
{
  if (mode == 0) { x = a.x; y = a.y; } else { x = 2.0 * rnd() - 1.0; y = 2.0 * rnd() - 1.0; }
  let color = cut_web_colour(vec2<f32>(x * 0.5, y * 0.5), D(1), D(3));
  if (i32(D(4)) == 0) { hide = color > 0.0; } else { hide = color <= 0.0; }
}
v.hide = 0.0;
if (hide) { x = 0.0; y = 0.0; v.hide = 1.0; }
v.x = AMOUNT * x;
v.y = AMOUNT * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

// ---------------------------------------------------------------------------------------------
// cut_wood
// ---------------------------------------------------------------------------------------------

fn wood_random(st: Vec2) -> f64 {
    fract((dot(st, vec2(12.9898, 78.233))).sin() * 43758.5453123)
}

fn wood_noise(c: Vec2, density: f64) -> f64 {
    let cu = fract(c * density);
    let ci = floor(c * density);
    let bl = wood_random(ci);
    let br = wood_random(ci + vec2(1.0, 0.0));
    let tl = wood_random(ci + vec2(0.0, 1.0));
    let tr = wood_random(ci + vec2(1.0, 1.0));
    let cu = smoothstep(0.0, 1.0, cu);
    let b = mix(bl, br, cu.x);
    let t = mix(tl, tr, cu.x);
    mix(b, t, cu.y)
}

fn wood_line(c: Vec2, width: f64) -> f64 {
    let smoothing = 0.04;
    smoothstep(0.5 - width, 0.5 - width + smoothing, c.y)
        - smoothstep(0.5 + width - smoothing, 0.5 + width, c.y)
}

// d = [shiftX, shiftY, freq, smooth, LineCount, LineWidth, mode, zoom, invert]
fn wood_color(d: &[f64], x: f64, y: f64) -> f64 {
    let st = vec2(x * d[7], y * d[7]) + vec2(d[0], d[1]);
    let angle = wood_noise(st, d[2]).powf(d[3]);
    let (s, c) = angle.sin_cos();
    let st = (st - 0.5) * mat2(c, -s, s, c) + 0.5;
    let c = fract(st * vec2(1.0, d[4]));
    mix(0.0, 1.0, wood_line(c, d[5]))
}

const CUT_WOOD_WGSL: &str = "fn cut_wood_random(st: vec2<f32>) -> f32 {
  return fract(sin(dot(st, vec2<f32>(12.9898, 78.233))) * 43758.5453123);
}
fn cut_wood_noise(c: vec2<f32>, density: f32) -> f32 {
  var cu = fract(c * density);
  let ci = floor(c * density);
  let bl = cut_wood_random(ci);
  let br = cut_wood_random(ci + vec2<f32>(1.0, 0.0));
  let tl = cut_wood_random(ci + vec2<f32>(0.0, 1.0));
  let tr = cut_wood_random(ci + vec2<f32>(1.0, 1.0));
  cu = glsl_smoothstep2(vec2<f32>(0.0), vec2<f32>(1.0), cu);
  let b = mix(bl, br, cu.x);
  let t = mix(tl, tr, cu.x);
  return mix(b, t, cu.y);
}
fn cut_wood_line(c: vec2<f32>, width: f32) -> f32 {
  let sm = 0.04;
  return glsl_smoothstep(0.5 - width, 0.5 - width + sm, c.y) - glsl_smoothstep(0.5 + width - sm, 0.5 + width, c.y);
}
fn cut_wood_color(st_in: vec2<f32>, freq: f32, smoothing: f32, line_count: f32, line_width: f32) -> f32 {
  let ang = pow(cut_wood_noise(st_in, freq), smoothing);
  let m = mat2x2<f32>(cos(ang), -sin(ang), sin(ang), cos(ang));
  let st = (st_in - 0.5) * m + 0.5;
  let c = fract(st * vec2<f32>(1.0, line_count));
  return mix(0.0, 1.0, cut_wood_line(c, line_width));
}
@body
let mode = i32(D(6));
var x = 0.0;
var y = 0.0;
var pc = 0.0;
var hide = true;
{
  if (mode == 0) { x = a.x; y = a.y; pc = 0.0; } else { x = rnd(); y = rnd(); pc = 0.5; }
  let st = vec2<f32>(x * D(7), y * D(7)) + vec2<f32>(D(0), D(1));
  let color = cut_wood_color(st, D(2), D(3), D(4), D(5));
  if (i32(D(8)) == 0) { hide = color > 0.0; } else { hide = color <= 0.0; }
}
v.hide = 0.0;
if (hide) { x = 0.0; y = 0.0; v.hide = 1.0; }
v.x = AMOUNT * (x - pc);
v.y = AMOUNT * (y - pc);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

// ---------------------------------------------------------------------------------------------
// cut_x
// ---------------------------------------------------------------------------------------------

fn cutx_color(x: f64, y: f64, zoom: f64, size: f64) -> f64 {
    let uv = vec2(x, y) * zoom;
    let mut st = abs(uv);
    let a = 3.14159265359f64 / 4.0;
    st = mat2(a.cos(), -a.sin(), a.sin(), a.cos()) * st;
    st = abs(st);
    st.y -= size;
    let line = smoothstep(0.0, 0.009, st.y);
    mix(0.0, 1.0, line)
}

const CUT_X_WGSL: &str = "fn cut_x_color(x: f32, y: f32, zoom: f32, size: f32) -> f32 {
  var st = abs(vec2<f32>(x, y) * zoom);
  let ang = 3.14159265359 / 4.0;
  st = mat2x2<f32>(cos(ang), -sin(ang), sin(ang), cos(ang)) * st;
  st = abs(st);
  st.y -= size;
  return mix(0.0, 1.0, glsl_smoothstep(0.0, 0.009, st.y));
}
@body
let mode = i32(D(0));
var x = 0.0;
var y = 0.0;
var hide = true;
{
  if (mode == 0) { x = a.x; y = a.y; } else { x = rnd() - 0.5; y = rnd() - 0.5; }
  let color = cut_x_color(x, y, D(1), D(3));
  if (i32(D(2)) == 0) { hide = color < 0.1; } else { hide = color >= 0.1; }
}
v.hide = 0.0;
if (hide) { x = 0.0; y = 0.0; v.hide = 1.0; }
v.x = AMOUNT * x;
v.y = AMOUNT * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

// ---------------------------------------------------------------------------------------------
// cut_yuebing
// ---------------------------------------------------------------------------------------------

const CUT_YUEBING_WGSL: &str = "let mode = i32(D(0));
let duration = sin(D(3) / 2.0) * D(4);
var x = 0.0;
var y = 0.0;
var hide = true;
{
  if (mode == 0) { x = a.x; y = a.y; } else { x = rnd() - 0.5; y = rnd() - 0.5; }
  let px = x * D(1);
  let py = y * D(1);
  let color = sin(px * py * 10000.0 * duration);
  if (i32(D(2)) == 0) { hide = color < 0.3; } else { hide = color >= 0.3; }
}
v.hide = 0.0;
if (hide) { x = 0.0; y = 0.0; v.hide = 1.0; }
v.x = AMOUNT * x;
v.y = AMOUNT * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

// ---------------------------------------------------------------------------------------------
// cut_zigzag
// ---------------------------------------------------------------------------------------------

fn zigzag_color(xp: f64, yp: f64, xpar: f64, ypar: f64, zoom: f64) -> f64 {
    let mut st = vec2(xp, yp) * vec2(xpar, ypar) * zoom;
    // mirrorTile
    if fract(st.y * 0.5) > 0.5 {
        st.x += 0.5;
        st.y = 1.0 - st.y;
    }
    let st = fract(st);
    let x = st.x * 2.0;
    let a = (1.0 + (x * M_PI).sin()).floor();
    let b = (1.0 + ((x + 1.0) * M_PI).sin()).floor();
    let f = fract(x);
    let pct = mix(a, b, f);
    smoothstep(pct - 0.01, pct, st.y)
}

const CUT_ZIGZAG_WGSL: &str =
    "fn cut_zigzag_color(xp: f32, yp: f32, xpar: f32, ypar: f32, zoom: f32) -> f32 {
  var st = vec2<f32>(xp, yp) * vec2<f32>(xpar, ypar) * zoom;
  if (fract(st.y * 0.5) > 0.5) {
    st.x = st.x + 0.5;
    st.y = 1.0 - st.y;
  }
  st = fract(st);
  let x = st.x * 2.0;
  let a = floor(1.0 + sin(x * PI));
  let b = floor(1.0 + sin((x + 1.0) * PI));
  let pct = mix(a, b, fract(x));
  return glsl_smoothstep(pct - 0.01, pct, st.y);
}
@body
let mode = i32(D(0));
var x = 0.0;
var y = 0.0;
var hide = true;
{
  if (mode == 0) { x = a.x; y = a.y; } else { x = rnd() - 0.5; y = rnd() - 0.5; }
  let color = cut_zigzag_color(x, y, D(1), D(2), D(3));
  if (i32(D(4)) == 0) { hide = color >= 0.5; } else { hide = color < 0.5; }
}
v.hide = 0.0;
if (hide) { x = 0.0; y = 0.0; v.hide = 1.0; }
v.x = AMOUNT * x;
v.y = AMOUNT * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

// ---------------------------------------------------------------------------------------------
// cylinder3D
// ---------------------------------------------------------------------------------------------

fn cylinder3d_sd(p: Vec3, h: Vec2) -> f64 {
    let d = abs(vec2(length(vec2(p.x, p.z)), p.y)) - h;
    d.x.max(d.y).min(0.0) + length(max(d, 0.0))
}

// ---------------------------------------------------------------------------------------------
// dc_acrilic
// ---------------------------------------------------------------------------------------------

// d = [seed, time, zoom, steps, p1, p2, p3, p4, p5, p6, redf, greenf, bluef, <dc base>(13..)]
fn acrilic_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let time = d[1];
    let steps = d[3] as i32;
    let mut p = uv * d[2];
    let mut col = [0.0; 3];
    for (j, c) in col.iter_mut().enumerate() {
        let fj = j as f64;
        for i in 1..steps {
            let fi = i as f64;
            let w = 0.1 / (i + j as i32) as f64;
            p.x += w * (fi * d[4] * p.y + time + ((time / (d[5] * fi)) * fi + fj).cos()).sin();
            p.y += w * (fi * d[6] * p.x + time + ((time / (d[7] * fi)) * fi + fj).sin()).cos();
        }
        *c = (d[8] * sqr(p.x)).sin() + (d[9] * sqr(p.y)).sin();
    }
    vec3(
        (col[0] * d[10]).cos(),
        (col[1] * d[11]).cos(),
        (col[2] * d[12]).cos(),
    )
}

// ---------------------------------------------------------------------------------------------
// dc_booleans
// ---------------------------------------------------------------------------------------------

// d = [randomize, type, zoom, <dc base>(3..8), x0, y0]
fn booleans_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let u = uv * d[2] + vec2(d[8], d[9]);
    let (ix, iy) = (u.x as i32, u.y as i32);
    let temp = match d[1] as i32 {
        0 => (ix ^ iy) as f64,
        1 => (ix & iy) as f64,
        2 => (ix | iy) as f64,
        _ => 0.0,
    };
    let c = vec3(temp.sin(), 1.0, temp.cos());
    let k = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(Vec3::splat(c.x) + vec3(k.x, k.y, k.z)) * 6.0 - Vec3::splat(k.w));
    mix(Vec3::splat(k.x), clamp(p - Vec3::splat(k.x), 0.0, 1.0), c.y) * c.z
}

// ---------------------------------------------------------------------------------------------
// dc_butterflies
// ---------------------------------------------------------------------------------------------

fn butterflies_cpow(z: Vec2, n: f64) -> Vec2 {
    let r = length(z);
    let a = glsl_atan2(z.y, z.x);
    vec2((a * n).cos(), (a * n).sin()) * r.powf(n)
}

fn butterflies_csin(z: Vec2) -> Vec2 {
    let r = z.y.exp();
    vec2((r + 1.0 / r) * z.x.sin(), (r - 1.0 / r) * z.x.cos()) * 0.5
}

// d = [seed, time, zoom, red, green, blue, ColorOnly, Gradient, scale_z, offset_z, reset_z]
fn butterflies_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let p = vec2(xp * d[2], yp * d[2]);
    let z0 = p;
    let fk = 0.9 * (1.0 + 2.0 * (0.1 * d[1]).sin());
    let mut z = p;
    let mut g = 1e10;
    for _ in 0..100 {
        let prevz = z;
        z = butterflies_csin(butterflies_cpow(z + z0, -4.0)) - z * fk + z0;
        g = java_min(g, dot(z - 1.0, z - 1.0));
        let dz = dot(z - prevz, z - prevz);
        if dz < 0.00001 {
            let k = dz / 0.00001;
            z = z * k + prevz * (1.0 - k);
            break;
        }
        if dz > 10000.0 {
            let k = 10000.0 / dz;
            z = z * k + prevz * (1.0 - k);
            break;
        }
    }
    sin(vec3(d[3], d[4], d[5]) + (2.3 + (g * (z.y * z.x).abs()).ln())) * 0.5 + 0.5
}

// ---------------------------------------------------------------------------------------------
// dc_cairotiles
// ---------------------------------------------------------------------------------------------

// d = [zoom, seed, time, <dc base>(3..)]
fn cairotiles_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let pi = (-1.0f64).acos();
    let zoom = d[0];
    let mut p = uv * zoom;
    let th = glsl_mod(d[2] * pi / 5.0, pi * 2.0);
    let gridsize = (0.5 + (th * 2.0).sin().abs() * (2.0f64.sqrt() / 2.0 - 0.5)) * 2.0;
    let mut flip = 0;
    if fract(th / pi + 0.25) > 0.5 {
        p -= 0.5;
        flip = 1;
    }
    p *= gridsize;
    let cp = floor(p / gridsize);
    p = glsl_mod(p, gridsize) - gridsize / 2.0;
    p *= glsl_mod(cp, 2.0) * 2.0 - 1.0;
    p = p * mat2(th.cos(), th.sin(), -th.sin(), th.cos());
    let w = zoom / 2000.0 * 1.5;
    let mut a = smoothstep(-w, w, p.x.abs().max(p.y.abs()) - 0.5);
    if flip == 1 {
        a = 1.0 - a;
    }
    if flip == 1 && a < 0.5 && (p.x.abs() - p.y.abs()) * sign(fract(th / pi) - 0.5) > 0.0 {
        a = 0.4;
    }
    if flip == 0 && a < 0.5 && (glsl_mod(cp.x + cp.y, 2.0) - 0.5) > 0.0 {
        a = 0.4;
    }
    pow(Vec3::splat(a), Vec3::splat(1.0 / 2.2))
}

// ---------------------------------------------------------------------------------------------
// dc_circlesblue
// ---------------------------------------------------------------------------------------------

// d = [seed, time, zoom, radius, bubles, <dc base>(5..)]
fn circlesblue_rgb(d: &[f64], uv: Vec2) -> Vec3 {
    let time = d[1];
    let uv = uv * d[2];
    let mut color = Vec3::splat(0.0);
    for i in 0..d[4] as i32 {
        let fi = i as f64;
        let pha = (fi * 6.0 + 1.0).tan() * 0.5 + 0.5;
        let siz = ((fi * 2.4 + 5.0).cos() * 0.5 + 0.5).powf(4.0);
        let pox = (fi * 3.55 + 4.1).cos();
        let rad = d[3] + fi.sin() * 0.12 + 0.08;
        let pos = vec2(
            pox + (time / 15.0 + pha + siz).sin(),
            -1.0 - rad
                + (2.0 + 2.0 * rad) * glsl_mod(pha + 0.1 * (time / 5.0) * (0.2 + 0.8 * siz), 1.0),
        );
        let dis = length(uv - pos);
        let col = mix(
            vec3(0.1, 0.2, 0.8),
            vec3(0.2, 0.8, 0.6),
            0.5 + 0.5 * (fi * (time * pox * 0.03).sin() + 1.9).sin(),
        );
        color += col
            * (1.0 - smoothstep(rad * (0.65 + 0.20 * (pox * time).sin()), rad, dis))
            * (1.0 - (pox * time).cos());
    }
    color * 0.3
}

// ---------------------------------------------------------------------------------------------
// dc_circuits
// ---------------------------------------------------------------------------------------------

// d = [Seed, time, rate, intensity, focus, pulse, glow, loops, zoom, <dc base>(9..)]
/// Returns the colour and the increment of the Java field `S`.
fn circuits_formula(d: &[f64], z: Vec2, t: f64) -> (Vec3, f64) {
    let (intensity, pulse, loops, zoom) = (d[3], d[5], d[7], d[8]);
    let mut z = z;
    let mut m: f64 = 0.0;
    let mut ot: f64 = 1000.0;
    let mut ot2: f64 = 1000.0;
    let mut k = (loops / 4.0 + (5.0 * zoom).floor()).floor();
    for i in 0..11 {
        z = abs(z) / dot(z, z).clamp(0.1, 0.5) - t;
        let l = length(z);
        let o = z.x.min(z.y).abs().max(-l + 0.25).min((l - 0.25).abs());
        ot = ot.min(o);
        ot2 = (l * 0.1).min(ot2);
        m = m.max(i as f64 * (1.0 - sign(ot - o).abs()));
        if k <= 0.0 {
            break;
        }
        k -= 1.0;
    }
    m += 1.0;
    let w = (intensity * zoom) * m;
    let circ = ((w - ot2).max(0.0) / w).powf(6.0);
    let ds = (((w - ot).max(0.0) / w).powf(0.25)).max(circ);
    let col = normalize(Vec3::splat(0.1) + vec3(0.45, 0.75, m * 0.1));
    let t2 = Vec3::splat(0.4 + glsl_mod(m / 9.0 - t * pulse + ot2 * 2.0, 1.0));
    (
        col * t2 + vec3(1.0, 0.7, 0.3) * (circ * (10.0 - m) * 3.0),
        ds,
    )
}

/// `DCCircuitsFunc.getRGBColor`; `s` is JWildfire's `S` field, which is carried across samples.
fn circuits_rgb(d: &[f64], pos: Vec2, s: &mut f64) -> Vec3 {
    let (time, rate, focus, loops, zoom) = (d[1], d[2], d[4], d[7], d[8]);
    let mut r = 0.0;
    let mut n = time * 0.01 * rate;
    let mut t = 2.0 * rate;
    if n > 6.0 * rate {
        r += 1.0;
        n -= r * 8.0 * rate;
    }
    if n < 4.0 * rate {
        t += n;
    } else {
        t = 8.0 * rate - n;
    }
    let zz = 1.05 - zoom;
    let mut uv = pos;
    let sph = length(uv) * 0.1;
    let sph = (1.0 - sph * sph).sqrt() * 2.0;
    let a = t * M_PI;
    let b = a + t;
    let c = a.cos() + b.sin();
    uv = uv * mat2(b.cos(), b.sin(), -b.sin(), b.cos());
    uv = uv * mat2(a.cos(), -a.sin(), a.sin(), a.cos());
    uv -= vec2(c.sin(), c.cos()) / M_PI;
    uv *= zz;
    let pix = 0.5 / 1000.0 * zz / sph;
    let dof = zoom * focus + t * 0.25;
    let mut l = loops.floor();
    let mut color = Vec3::splat(0.0);
    for aa in 0..24 {
        let aauv = floor(vec2(aa as f64 / 6.0, glsl_mod(aa as f64, 6.0)));
        let (col, ds) = circuits_formula(d, uv + aauv * pix * dof, t);
        color = col;
        *s += ds;
        if l <= 0.0 {
            break;
        }
        l -= 1.0;
    }
    *s /= loops.floor();
    color /= loops.floor();
    mix(Vec3::splat(0.15), color, *s) * (1.0 - length(pos)) * vec3(1.2, 1.1, 1.0)
}

// ---------------------------------------------------------------------------------------------
// dc_crackle_wf / dc_cracklep_wf (CrackleFunc + DCCrackleWFFunc.applyCellCalculation)
// ---------------------------------------------------------------------------------------------

fn crackle_wf_position(x: i32, y: i32, z: f64, s: f64, d: f64) -> [f64; 2] {
    if d == 0.0 {
        return [x as f64 * s, y as f64 * s];
    }
    let e = [x as f64 * 2.5, y as f64 * 2.5, z * 2.5];
    let f = [y as f64 * 2.5 + 30.2, x as f64 * 2.5 - 12.1, z * 2.5 + 19.8];
    [
        (x as f64 + d * noise::simplex_noise3d(e)) * s,
        (y as f64 + d * noise::simplex_noise3d(f)) * s,
    ]
}

/// `d` = [cellsize, power, distort, scale, z, color_scale, color_offset]. The Java cell cache
/// holds exactly the values `position()` computes, so positions are computed directly.
fn crackle_wf_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let (cellsize, power, distort, scale, z) = (d[0], d[1], d[2], d[3], d[4]);
    if cellsize == 0.0 {
        return;
    }
    let s = cellsize / 2.0;
    let u = if amount != 0.0 {
        let blurr = (ctx.random() + ctx.random()) / 2.0 + (ctx.random() - 0.5) / 4.0;
        let theta = 2.0 * M_PI * ctx.random();
        [blurr * theta.sin(), blurr * theta.cos()]
    } else {
        [a.x, a.y]
    };
    let mut xcv = (u[0] / s).floor() as i32;
    let mut ycv = (u[1] / s).floor() as i32;
    let fill = |xcv: i32, ycv: i32| {
        let mut p = [[0.0; 2]; 9];
        for (i, pt) in p.iter_mut().enumerate() {
            *pt = crackle_wf_position(
                xcv + i as i32 / 3 - 1,
                ycv + i as i32 % 3 - 1,
                z,
                s,
                distort,
            );
        }
        p
    };
    let p = fill(xcv, ycv);
    let q = noise::closest(&p, u) as i32;
    xcv += q / 3 - 1;
    ycv += q % 3 - 1;
    let p = fill(xcv, ycv);
    let l = noise::voronoi(&p, 4, u);
    let trg_l = (l + 1e-100).powf(power) * scale;
    let r = trg_l / (l + 1e-100);
    let dxo = (u[0] - p[4][0]) * r + p[4][0];
    let dyo = (u[1] - p[4][1]) * r + p[4][1];
    v.color = (l * d[5] + d[6]).clamp(0.0, 1.0);
    v.x += amount * dxo;
    v.y += amount * dyo;
}

/// WGSL for the crackle based DC variations; `$o0..$o6` are the `D()` indices of
/// cellsize, power, distort, scale, z, color_scale, color_offset.
macro_rules! crackle_wf_wgsl {
    ($n:literal, $o0:literal, $o1:literal, $o2:literal, $o3:literal, $o4:literal, $o5:literal, $o6:literal) => {
        concat!(
            "fn ", $n, "_pos(x: i32, y: i32, z: f32, s: f32, d: f32) -> vec2<f32> {
  var px = f32(x);
  var py = f32(y);
  if (d != 0.0) {
    px += d * noise_simplex3d(vec3<f32>(f32(x) * 2.5, f32(y) * 2.5, z * 2.5));
    py += d * noise_simplex3d(vec3<f32>(f32(y) * 2.5 + 30.2, f32(x) * 2.5 - 12.1, z * 2.5 + 19.8));
  }
  return vec2<f32>(px, py) * s;
}
fn ", $n, "_cell(u: vec2<f32>, cellsize: f32, power: f32, distort: f32, scale: f32, z: f32) -> vec3<f32> {
  let s = cellsize / 2.0;
  var xcv = i32(floor(u.x / s));
  var ycv = i32(floor(u.y / s));
  var P: array<vec2<f32>, 25>;
  for (var i = 0; i < 9; i++) { P[i] = ", $n, "_pos(xcv + i / 3 - 1, ycv + i % 3 - 1, z, s, distort); }
  let q = noise_closest(P, 9, u);
  xcv += q / 3 - 1;
  ycv += q % 3 - 1;
  for (var i = 0; i < 9; i++) { P[i] = ", $n, "_pos(xcv + i / 3 - 1, ycv + i % 3 - 1, z, s, distort); }
  let l = noise_voronoi(P, 9, 4, u);
  let c = P[4];
  let trg_l = java_pow(l + 1.0e-30, power) * scale;
  let r = trg_l / (l + 1.0e-30);
  return vec3<f32>((u - c) * r + c, l);
}
@body
if (D(", $o0, ") != 0.0) {
  var u = vec2<f32>(a.x, a.y);
  if (AMOUNT != 0.0) {
    let blurr = (rnd() + rnd()) / 2.0 + (rnd() - 0.5) / 4.0;
    let theta = TWO_PI * rnd();
    u = vec2<f32>(blurr * sin(theta), blurr * cos(theta));
  }
  let r = ", $n, "_cell(u, D(", $o0, "), D(", $o1, "), D(", $o2, "), D(", $o3, "), D(", $o4, "));
  var c = r.z * D(", $o5, ") + D(", $o6, ");
  if (c < 0.0) { c = 0.0; } else if (c > 1.0) { c = 1.0; }
  v.color = c;
  v.x += AMOUNT * r.x;
  v.y += AMOUNT * r.y;
}"
        )
    };
}

// ---------------------------------------------------------------------------------------------
// dc_dmodulus
// ---------------------------------------------------------------------------------------------

fn dmodulus_wrap(t: f64, e: f64, r: f64) -> f64 {
    if t > e {
        -e + fmod(t + e, r)
    } else if t < -e {
        e - fmod(e - t, r)
    } else {
        t
    }
}

// d = [x, y, angle, shiftx, shifty, color1, speed1, color2, speed2, square,
//      _xr, _yr, c11, c12, c21, c22, coscos, sinsin, sincos]
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn dmodulus_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (x, y, angle) = (d[0], d[1], d[2]);
    let (xr, yr) = (d[10], d[11]);
    let (c11, c12, c21, c22) = (d[12], d[13], d[14], d[15]);
    let (coscos, sinsin, sincos) = (d[16], d[17], d[18]);
    let p2 = coscos / (coscos + sinsin);
    let p1 = sinsin / (coscos + sinsin);
    let xnew = dmodulus_wrap(a.x, x, xr);
    let ynew = dmodulus_wrap(a.y, y, yr);
    // true: the "coscos" branch (colour coefficients c11/c12)
    let branch_a = |p: f64| if angle <= 45.0 { p < p2 } else { !(p < p1) };
    // Java keeps the colour recurrence `newColor = oldColor * c + c'` in an instance field
    // (starting at 0.5); here per render thread.
    let key = d.as_ptr() as usize;
    let old = ctx.orbits.get(&key).map_or(0.5, |st| st[0]);
    let (mut xt, mut yt, new_color) = if branch_a(ctx.random()) {
        (
            xnew * coscos + ynew * sincos + x,
            -xnew * sincos + ynew * coscos - y,
            old * c11 + c12,
        )
    } else {
        (
            xnew * sincos + ynew * sinsin,
            -xnew * sinsin + ynew * sincos,
            old * c21 + c22,
        )
    };
    xt += d[3];
    yt += d[4];
    if d[9] as i32 == 0 {
        v.x += amount * xt;
        v.y += amount * yt;
    } else {
        v.x += amount * dmodulus_wrap(xt, x, xr);
        v.y += amount * dmodulus_wrap(yt, y, yr);
    }
    ctx.pass_z(a, v, amount);
    v.color = fmod(new_color, 1.0);
    ctx.orbits.insert(key, [new_color, 0.0]);
}

// ---------------------------------------------------------------------------------------------
// dc_ducks
// ---------------------------------------------------------------------------------------------

fn ducks_f(e: Vec2, time: f64) -> Vec3 {
    let mut e_ = e;
    let mut c = 0.0;
    let add = vec2(
        0.1 * (time / 3.0).sin() - 0.1,
        5.0 + 0.1 * (time / 5.0).cos(),
    );
    for _ in 0..50 {
        let a = vec2(e_.x, e_.y.abs());
        e_ = vec2(length(a).ln(), glsl_atan2(a.y, a.x) - 6.2) + add;
        c += length(e_);
    }
    let d = log2(log2(c * 0.05)) * 6.0;
    let flori = vec3(
        0.1 + 0.7 * d.sin(),
        0.1 + 0.5 * (d - 0.7).sin(),
        0.7 + 0.7 * (d - 0.7).cos(),
    );
    floor(Vec3::splat(0.5) + flori * 1.1) / 1.1
}

// ---------------------------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "cut_tstruchet",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::int("shape", 0.0, 0.0, 3.0),
            ParamDef::int("levels", 0.0, 0.0, 1.0),
            ParamDef::int("arcs", 0.0, 0.0, 1.0),
            ParamDef::range("zoom", 5.0, 0.2, 8.2),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [seed, mode, shape, levels, arcs, zoom, invert, x0, y0]
        apply: |d, ctx, a, v, amount| {
            let (x, y, center) = if d[1] as i32 == 0 { (a.x, a.y, 0.0) } else { (ctx.random(), ctx.random(), 0.5) };
            let op = vec2(x * d[5], y * d[5]) + vec2(d[7], d[8]);
            let color = tstruchet_color(op, d[2] as i32, d[3] as i32, d[4] as i32);
            let hide = if d[6] as i32 == 0 { color > 0.30 } else { color <= 0.30 };
            w06_cut_out(ctx, a, v, amount, x, y, center, hide);
        },
        prepare: Some(|p, _| {
            let mut d = w06_limit(p, &[(1, 0.0, 1.0, true), (2, 0.0, 3.0, true), (3, 0.0, 1.0, true), (4, 0.0, 1.0, true), (6, 0.0, 1.0, true)]);
            let seed = p[0] as i32;
            let r = w06_java_random_doubles::<2>(seed);
            d.push(seed as f64 * r[0]);
            d.push(seed as f64 * r[1]);
            d
        }),
        wgsl: CUT_TSTRUCHET_WGSL,
    },
    VarDef {
        name: "cut_vasarely",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 1.0, 0.01, 2.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
            ParamDef::range("size", 0.5, 0.0, 10.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [mode, zoom, invert, size]
        apply: |d, ctx, a, v, amount| {
            let (x, y) = if d[0] as i32 == 0 {
                (a.x, a.y)
            } else {
                let x = 2.0 * ctx.random() - 1.0;
                (x, 2.0 * ctx.random() - 1.0)
            };
            let color = vasarely_color(x, y, d[1], d[3]);
            let hide = if d[2] as i32 == 0 { color < 0.1 } else { color >= 0.1 };
            w06_cut_out(ctx, a, v, amount, x, y, 0.0, hide);
        },
        prepare: Some(|p, _| w06_limit(p, &[(0, 0.0, 1.0, true), (2, 0.0, 1.0, true)])),
        wgsl: CUT_VASARELY_WGSL,
    },
    VarDef {
        name: "cut_web",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("thick", 0.05, 0.0, 0.9),
            ParamDef::int("invert", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [seed, time, mode, thick, invert]; `seed` only advanced `time` by the elapsed
        // wall-clock time in Java (non-deterministic) and has no effect here.
        apply: |d, ctx, a, v, amount| {
            let (x, y) = if d[2] as i32 == 0 {
                (a.x, a.y)
            } else {
                let x = 2.0 * ctx.random() - 1.0;
                (x, 2.0 * ctx.random() - 1.0)
            };
            let color = web_colour(vec2(x * 0.5, y * 0.5), d[1], d[3]);
            let hide = if d[4] as i32 == 0 { color > 0.0 } else { color <= 0.0 };
            w06_cut_out(ctx, a, v, amount, x, y, 0.0, hide);
        },
        prepare: Some(|p, _| w06_limit(p, &[(2, 0.0, 1.0, true), (3, 0.0, 0.9, false), (4, 0.0, 1.0, true)])),
        wgsl: CUT_WEB_WGSL,
    },
    VarDef {
        name: "cut_wood",
        params: &[
            ParamDef::range("shiftX", 0.0, -5.0, 5.0),
            ParamDef::range("shiftY", 0.0, -5.0, 5.0),
            ParamDef::range("freq", 1.8, 0.1, 2.0),
            ParamDef::range("smooth", 1.0, 0.0, 1.0),
            ParamDef::range("LineCount", 20.0, 1.0, 50.0),
            ParamDef::range("LineWidth", 0.2, 0.1, 0.5),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 2.5, 0.0, 20.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [shiftX, shiftY, freq, smooth, LineCount, LineWidth, mode, zoom, invert]
        apply: |d, ctx, a, v, amount| {
            let (x, y, center) = if d[6] as i32 == 0 { (a.x, a.y, 0.0) } else { (ctx.random(), ctx.random(), 0.5) };
            let color = wood_color(d, x, y);
            let hide = if d[8] as i32 == 0 { color > 0.0 } else { color <= 0.0 };
            w06_cut_out(ctx, a, v, amount, x, y, center, hide);
        },
        prepare: Some(|p, _| {
            w06_limit(
                p,
                &[
                    (2, 0.1, 2.0, false),
                    (3, 0.0, 1.0, false),
                    (4, 1.0, 50.0, false),
                    (5, 0.1, 0.5, false),
                    (6, 0.0, 1.0, true),
                    (8, 0.0, 1.0, true),
                ],
            )
        }),
        wgsl: CUT_WOOD_WGSL,
    },
    VarDef {
        name: "cut_x",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 1.0, 0.5, 10.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
            ParamDef::range("size", 0.1, 0.0, 0.25),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [mode, zoom, invert, size]
        apply: |d, ctx, a, v, amount| {
            let (x, y) = if d[0] as i32 == 0 {
                (a.x, a.y)
            } else {
                let x = ctx.random() - 0.5;
                (x, ctx.random() - 0.5)
            };
            let color = cutx_color(x, y, d[1], d[3]);
            let hide = if d[2] as i32 == 0 { color < 0.1 } else { color >= 0.1 };
            w06_cut_out(ctx, a, v, amount, x, y, 0.0, hide);
        },
        prepare: Some(|p, _| w06_limit(p, &[(0, 0.0, 1.0, true), (2, 0.0, 1.0, true)])),
        wgsl: CUT_X_WGSL,
    },
    VarDef {
        name: "cut_yuebing",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 0.25, 0.01, 0.26),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
            ParamDef::range("p1", 2.0, -3.14159, 3.14159),
            ParamDef::range("p2", 25.0, 1.0, 10.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [mode, zoom, invert, p1, p2]
        apply: |d, ctx, a, v, amount| {
            let (x, y) = if d[0] as i32 == 0 {
                (a.x, a.y)
            } else {
                let x = ctx.random() - 0.5;
                (x, ctx.random() - 0.5)
            };
            let (px, py) = (x * d[1], y * d[1]);
            let duration = (d[3] / 2.0).sin() * d[4];
            let color = (px * py * 10000.0 * duration).sin();
            let hide = if d[2] as i32 == 0 { color < 0.3 } else { color >= 0.3 };
            w06_cut_out(ctx, a, v, amount, x, y, 0.0, hide);
        },
        prepare: Some(|p, _| w06_limit(p, &[(0, 0.0, 1.0, true), (2, 0.0, 1.0, true)])),
        wgsl: CUT_YUEBING_WGSL,
    },
    VarDef {
        name: "cut_zigzag",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("xpar", 1.0, 0.5, 5.0),
            ParamDef::range("ypar", 2.0, 0.5, 5.0),
            ParamDef::range("zoom", 2.0, 0.5, 15.5),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [mode, xpar, ypar, zoom, invert]
        apply: |d, ctx, a, v, amount| {
            let (x, y) = if d[0] as i32 == 0 {
                (a.x, a.y)
            } else {
                let x = ctx.random() - 0.5;
                (x, ctx.random() - 0.5)
            };
            let color = zigzag_color(x, y, d[1], d[2], d[3]);
            let hide = if d[4] as i32 == 0 { color >= 0.5 } else { color < 0.5 };
            w06_cut_out(ctx, a, v, amount, x, y, 0.0, hide);
        },
        prepare: Some(|p, _| w06_limit(p, &[(0, 0.0, 1.0, true), (4, 0.0, 1.0, true)])),
        wgsl: CUT_ZIGZAG_WGSL,
    },
    VarDef {
        name: "cyclone",
        params: &[ParamDef::new("strength", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let theta = a.y.atan2(a.x);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let new_theta = theta + d[0] / (r + 0.01);
            v.x += amount * r * new_theta.cos();
            v.y += amount * r * new_theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let theta = atan2(a.y, a.x);
         let r = sqrt(a.x * a.x + a.y * a.y);
         let new_theta = theta + D(0) / (r + 0.01);
         v.x += AMOUNT * r * cos(new_theta);
         v.y += AMOUNT * r * sin(new_theta);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cylinder3D",
        params: &[ParamDef::range("d", 0.30, 0.0, 0.5), ParamDef::range("H", 0.25, 0.0, 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [d, H]
        apply: |d, ctx, _a, v, amount| {
            for _ in 0..50 {
                let x = ctx.random() - 0.5;
                let y = ctx.random() - 0.5;
                let z = ctx.random() - 0.5;
                if cylinder3d_sd(vec3(x, y, z), vec2(d[0], d[1])) < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| w06_limit(p, &[(0, 0.0, 0.5, false), (1, 0.0, 0.5, false)])),
        wgsl: "v.hide = 1.0;
         for (var i = 0; i < 50; i++) {
           let p = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
           let dd = abs(vec2<f32>(length(p.xz), p.y)) - vec2<f32>(D(0), D(1));
           let dist = min(max(dd.x, dd.y), 0.0) + length(max(dd, vec2<f32>(0.0)));
           if (dist < 0.0) {
             v.hide = 0.0;
             v.x += AMOUNT * p.x;
             v.y += AMOUNT * p.y;
             v.z += AMOUNT * p.z;
             break;
           }
         }",
    },
    simple(
        "cylinder_apo",
        true,
        |_, _, a, v, amount| {
            v.x += amount * a.x.sin();
            v.y += amount * a.y;
            v.z += amount * a.x.cos();
        },
        "v.x += AMOUNT * sin(a.x);
         v.y += AMOUNT * a.y;
         v.z += AMOUNT * cos(a.x);",
    ),
    VarDef {
        name: "dc_acrilic",
        params: dc_params![
            ParamDef::int("seed", 10000.0, 0.0, 100000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("zoom", 0.7, 0.1, 5.0),
            ParamDef::int("steps", 10.0, 3.0, 100.0),
            ParamDef::range("p1", 12.0, 1.0, 100.0),
            ParamDef::range("p2", 12.0, 1.0, 100.0),
            ParamDef::range("p3", 12.0, 1.0, 100.0),
            ParamDef::range("p4", 12.0, 1.0, 100.0),
            ParamDef::range("p5", 75.0, 1.0, 100.0),
            ParamDef::range("p6", 75.0, 1.0, 100.0),
            ParamDef::range("redf", 1.0, 0.0, 5.0),
            ParamDef::range("greenf", 1.0, 0.0, 5.0),
            ParamDef::range("bluef", 1.0, 0.0, 5.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = w06_dc_uv(d, 13, ctx, a, false);
            let color = acrilic_rgb(d, uv);
            w06_dc_tail(d, 13, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut lim = vec![(3, 3.0, 100.0, true)];
            lim.extend((4..10).map(|i| (i, 1.0, 100.0, false)));
            lim.extend((10..13).map(|i| (i, 0.0, 5.0, false)));
            let mut d = w06_limit(p, &lim);
            w06_dc_limit(&mut d, 13);
            d
        }),
        wgsl: "var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
         if (i32(D(13)) == 1) { uv = vec2<f32>(a.x, a.y); }
         let tm = D(1);
         let steps = i32(D(3));
         var p = uv * D(2);
         var col = vec3<f32>(0.0);
         for (var j = 0; j < 3; j++) {
           let fj = f32(j);
           for (var i = 1; i < steps; i++) {
             let fi = f32(i);
             let w = 0.1 / f32(i + j);
             p.x += w * sin(fi * D(4) * p.y + tm + cos((tm / (D(5) * fi)) * fi + fj));
             p.y += w * cos(fi * D(6) * p.x + tm + sin((tm / (D(7) * fi)) * fi + fj));
           }
           let cv = sin(D(8) * p.x * p.x) + sin(D(9) * p.y * p.y);
           if (j == 0) { col.x = cv; } else if (j == 1) { col.y = cv; } else { col.z = cv; }
         }
         col = vec3<f32>(cos(col.x * D(10)), cos(col.y * D(11)), cos(col.z * D(12)));
         dc_output(&v, AMOUNT, uv, col, i32(D(14)), D(15), D(16), i32(D(17)));",
    },
    VarDef {
        name: "dc_apollonian",
        params: dc_params![ParamDef::int("seed", 10000.0, 0.0, 100000.0), ParamDef::range("time", 0.0, 0.0, 100.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [seed, time, <dc base>(2..)]
        apply: |d, ctx, a, v, amount| {
            let uv = w06_dc_uv(d, 2, ctx, a, true);
            let time = d[1];
            let t = 0.05 * time;
            let m = glsl_rot(Vec3::splat(t) + vec3(1.0, 2.0, 3.0));
            let k = 1.2 + 0.1 * (0.1 * time).sin();
            let f1 = 2.0 + 0.25 * (0.3 * time).sin();
            let p = (m * f1) * Vec3::from_vec2(uv * 2.0, 0.0);
            let color = sin(glsl_app(p, k, m)) * 0.6 + 0.5;
            w06_dc_tail(d, 2, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            w06_dc_limit(&mut d, 2);
            d
        }),
        wgsl: "var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
         if (i32(D(2)) == 1) { uv = vec2<f32>(a.x, a.y); }
         let tm = D(1);
         let t = 0.05 * tm;
         let m = glsl_rot(vec3<f32>(t) + vec3<f32>(1.0, 2.0, 3.0));
         let k = 1.2 + 0.1 * sin(0.1 * tm);
         let f1 = 2.0 + 0.25 * sin(0.3 * tm);
         let p = (m * f1) * vec3<f32>(uv * 2.0, 0.0);
         let col = sin(glsl_app(p, k, m)) * 0.6 + 0.5;
         dc_output(&v, AMOUNT, uv, col, i32(D(3)), D(4), D(5), i32(D(6)));",
    },
    VarDef {
        name: "dc_booleans",
        params: dc_params![
            ParamDef::int("randomize", 1000.0, 0.0, 1000000.0),
            ParamDef::int("type", 1.0, 0.0, 2.0),
            ParamDef::range("zoom", 1000.0, 1.0, 2000.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [randomize, type, zoom, <dc base>(3..8), x0, y0]
        apply: |d, ctx, a, v, amount| {
            let uv = w06_dc_uv(d, 3, ctx, a, false);
            let color = booleans_rgb(d, uv);
            w06_dc_tail(d, 3, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = w06_limit(p, &[(1, 0.0, 2.0, true)]);
            w06_dc_limit(&mut d, 3);
            let seed = p[0] as i32;
            let r = w06_java_random_doubles::<2>(seed);
            d.push(seed as f64 * r[0]);
            d.push(seed as f64 * r[1]);
            d
        }),
        wgsl: "var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
         if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
         let u = uv * D(2) + vec2<f32>(D(8), D(9));
         let ix = i32(u.x);
         let iy = i32(u.y);
         let ty = i32(D(1));
         var temp = 0.0;
         if (ty == 0) { temp = f32(ix ^ iy); } else if (ty == 1) { temp = f32(ix & iy); } else if (ty == 2) { temp = f32(ix | iy); }
         let c = vec3<f32>(sin(temp), 1.0, cos(temp));
         let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
         let p = abs(fract(vec3<f32>(c.x) + K.xyz) * 6.0 - vec3<f32>(K.w));
         let col = mix(vec3<f32>(K.x), clamp(p - vec3<f32>(K.x), vec3<f32>(0.0), vec3<f32>(1.0)), c.y) * c.z;
         dc_output(&v, AMOUNT, uv, col, i32(D(4)), D(5), D(6), i32(D(7)));",
    },
    VarDef {
        name: "dc_bubble",
        params: &[
            ParamDef::new("centerx", 0.0),
            ParamDef::new("centery", 0.0),
            ParamDef::new("scale", 1.0),
            ParamDef::int("invert", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [centerx, centery, scale, invert, bdcs]
        apply: |d, _, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y) / 4.0 + 1.0;
            let t = amount / r;
            v.x += t * a.x;
            v.y += t * a.y;
            v.z += amount * (2.0 / (if d[3] as i32 == 1 { t } else { r }) - 1.0);
            v.color = fmod((d[4] * (sqr(v.x + d[0]) + sqr(v.y + d[1]))).abs(), 1.0);
        },
        prepare: Some(|p, _| {
            let mut d = w06_limit(p, &[(3, 0.0, 1.0, true)]);
            d.push(1.0 / if p[2] == 0.0 { 10e-6 } else { p[2] });
            d
        }),
        wgsl: "let r = (a.x * a.x + a.y * a.y) / 4.0 + 1.0;
         let t = AMOUNT / r;
         v.x += t * a.x;
         v.y += t * a.y;
         var den = r;
         if (i32(D(3)) == 1) { den = t; }
         v.z += AMOUNT * (2.0 / den - 1.0);
         v.color = fmod(abs(D(4) * (sqr(v.x + D(0)) + sqr(v.y + D(1)))), 1.0);",
    },
    VarDef {
        name: "dc_butterflies",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("zoom", 1.0, 0.1, 5.0),
            ParamDef::range("red", 1.0, -1.0, 1.0),
            ParamDef::range("green", 1.0, -1.0, 1.0),
            ParamDef::range("blue", 1.0, -1.0, 1.0),
            ParamDef::int("ColorOnly", 0.0, 0.0, 1.0),
            ParamDef::int("Gradient", 0.0, 0.0, 2.0),
            ParamDef::new("scale_z", 0.0),
            ParamDef::new("offset_z", 0.0),
            ParamDef::int("reset_z", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [seed, time, zoom, red, green, blue, ColorOnly, Gradient, scale_z, offset_z, reset_z]
        // Not a DC_BaseFunc subclass, but the same tail except that x/y are assigned.
        apply: |d, ctx, a, v, amount| {
            let uv = w06_dc_uv(d, 6, ctx, a, false);
            let color = butterflies_rgb(d, uv.x, uv.y);
            v.x = 0.0;
            v.y = 0.0;
            w06_dc_tail(d, 6, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = w06_limit(p, &[(3, -1.0, 1.0, false), (4, -1.0, 1.0, false), (5, -1.0, 1.0, false)]);
            w06_dc_limit(&mut d, 6);
            d
        }),
        wgsl: "fn dc_butterflies_cpow(z: vec2<f32>, n: f32) -> vec2<f32> {
  let r = length(z);
  let ang = glsl_atan2(z.y, z.x);
  return vec2<f32>(cos(ang * n), sin(ang * n)) * pow(r, n);
}
fn dc_butterflies_csin(z: vec2<f32>) -> vec2<f32> {
  let r = exp(z.y);
  return vec2<f32>((r + 1.0 / r) * sin(z.x), (r - 1.0 / r) * cos(z.x)) * 0.5;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(6)) == 1) { uv = vec2<f32>(a.x, a.y); }
let p = uv * D(2);
let z0 = p;
let fk = 0.9 * (1.0 + 2.0 * sin(0.1 * D(1)));
var z = p;
var g = 1.0e10;
for (var i = 0; i < 100; i++) {
  let prevz = z;
  z = dc_butterflies_csin(dc_butterflies_cpow(z + z0, -4.0)) - z * fk + z0;
  let zm = z - 1.0;
  g = min(g, dot(zm, zm));
  let dz = dot(z - prevz, z - prevz);
  if (dz < 0.00001) {
    let k = dz / 0.00001;
    z = z * k + prevz * (1.0 - k);
    break;
  }
  if (dz > 10000.0) {
    let k = 10000.0 / dz;
    z = z * k + prevz * (1.0 - k);
    break;
  }
}
let col = sin(vec3<f32>(D(3), D(4), D(5)) + (2.3 + log(g * abs(z.y * z.x)))) * 0.5 + 0.5;
v.x = 0.0;
v.y = 0.0;
dc_output(&v, AMOUNT, uv, col, i32(D(7)), D(8), D(9), i32(D(10)));",
    },
    VarDef {
        name: "dc_cairotiles",
        params: dc_params![
            ParamDef::range("zoom", 7.0, 0.1, 50.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [zoom, seed, time, <dc base>(3..)]
        apply: |d, ctx, a, v, amount| {
            let uv = w06_dc_uv(d, 3, ctx, a, false);
            let color = cairotiles_rgb(d, uv);
            w06_dc_tail(d, 3, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = w06_limit(p, &[(0, 0.1, 50.0, false), (1, 0.0, 10000.0, true)]);
            w06_dc_limit(&mut d, 3);
            d
        }),
        wgsl: "var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
         if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
         var p = uv * D(0);
         let th = glsl_mod(D(2) * PI / 5.0, PI * 2.0);
         let gs = (0.5 + abs(sin(th * 2.0)) * (sqrt(2.0) / 2.0 - 0.5)) * 2.0;
         var flip = 0;
         if (fract(th / PI + 0.25) > 0.5) {
           p = p - 0.5;
           flip = 1;
         }
         p = p * gs;
         let cp = floor(p / gs);
         p = glsl_mod2(p, vec2<f32>(gs)) - gs / 2.0;
         p = p * (glsl_mod2(cp, vec2<f32>(2.0)) * 2.0 - 1.0);
         p = p * mat2x2<f32>(cos(th), sin(th), -sin(th), cos(th));
         let w = D(0) / 2000.0 * 1.5;
         var av = glsl_smoothstep(-w, w, max(abs(p.x), abs(p.y)) - 0.5);
         if (flip == 1) { av = 1.0 - av; }
         if (flip == 1 && av < 0.5 && (abs(p.x) - abs(p.y)) * sign(fract(th / PI) - 0.5) > 0.0) { av = 0.4; }
         if (flip == 0 && av < 0.5 && (glsl_mod(cp.x + cp.y, 2.0) - 0.5) > 0.0) { av = 0.4; }
         let col = pow(vec3<f32>(av), vec3<f32>(1.0 / 2.2));
         dc_output(&v, AMOUNT, uv, col, i32(D(4)), D(5), D(6), i32(D(7)));",
    },
    VarDef {
        name: "dc_carpet3D",
        params: &[
            ParamDef::new("origin", 0.5),
            ParamDef::new("color_a", 0.5),
            ParamDef::new("color_b", 1.0),
            ParamDef::new("color_c", 1.0),
            ParamDef::new("color_d", 1.0),
            ParamDef::new("color_e", 0.5),
            ParamDef::new("color_f", 1.0),
            ParamDef::new("stretch_x", 1.0),
            ParamDef::new("stretch_y", 1.0),
            ParamDef::new("scale_x", 1.0),
            ParamDef::new("scale_y", 1.0),
            ParamDef::new("scale_z", 1.0),
            ParamDef::new("offset_z", 0.0),
            ParamDef::new("reset_z", 0.0),
            ParamDef::new("sides", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [origin, color_a..color_f (1..6), stretch_x, stretch_y, scale_x, scale_y, scale_z,
        //      offset_z, reset_z, sides, H, c00, c01, c10, c11, c20, c21]
        apply: |d, ctx, a, v, amount| {
            let x0: i32 = if ctx.random() < 0.5 { -1 } else { 1 };
            let y0: i32 = if ctx.random() > 0.5 { -1 } else { 1 };
            let x = a.x + x0 as f64 * d[7];
            let y = a.y + y0 as f64 * d[8];
            let xor = (x0 ^ y0) as f64;
            let h = d[15];
            let hh = -h + (d[2] - xor) * h;
            v.x += amount * (d[16] * x + d[18] * y + d[20]) * d[9];
            v.y += amount * (d[17] * x + d[19] * y + d[21]) * d[10];
            v.color = fmod((v.color * d[1] * (d[3] + hh) + xor * (d[4] - hh) * d[5]).abs(), d[6]);
            ctx.pass_z(a, v, amount);
            let dz = v.color * d[11] + d[12];
            if d[13] > 0.0 {
                v.z = dz;
            } else {
                v.z += dz;
                if d[14] > 0.0 {
                    v.z += dz * ctx.random();
                } else {
                    v.z += dz;
                }
            }
        },
        prepare: Some(|p, c| {
            let mut d = p.to_vec();
            d.extend([0.1 * p[0], c.c00, c.c01, c.c10, c.c11, c.c20, c.c21]);
            d
        }),
        wgsl: "var x0 = 1;
         if (rnd() < 0.5) { x0 = -1; }
         var y0 = 1;
         if (rnd() > 0.5) { y0 = -1; }
         let x = a.x + f32(x0) * D(7);
         let y = a.y + f32(y0) * D(8);
         let xr = f32(x0 ^ y0);
         let hh = -D(15) + (D(2) - xr) * D(15);
         v.x += AMOUNT * (D(16) * x + D(18) * y + D(20)) * D(9);
         v.y += AMOUNT * (D(17) * x + D(19) * y + D(21)) * D(10);
         v.color = fmod(abs(v.color * D(1) * (D(3) + hh) + xr * (D(4) - hh) * D(5)), D(6));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         let dz = v.color * D(11) + D(12);
         if (D(13) > 0.0) {
           v.z = dz;
         } else {
           v.z += dz;
           if (D(14) > 0.0) { v.z += dz * rnd(); } else { v.z += dz; }
         }",
    },
    VarDef {
        name: "dc_carpet",
        params: &[ParamDef::new("origin", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [origin, H, c00, c01, c10, c11, c20, c21]
        apply: |d, ctx, a, v, amount| {
            let x0: i32 = if ctx.random() < 0.5 { -1 } else { 1 };
            let y0: i32 = if ctx.random() > 0.5 { -1 } else { 1 };
            let x = a.x + x0 as f64;
            let y = a.y + y0 as f64;
            let xor = (x0 ^ y0) as f64;
            let h = -d[1] + (1.0 - xor) * d[1];
            v.x += amount * (d[2] * x + d[4] * y + d[6]);
            v.y += amount * (d[3] * x + d[5] * y + d[7]);
            v.color = fmod((v.color * 0.5 * (1.0 + h) + xor * (1.0 - h) * 0.5).abs(), 1.0);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, c| vec![p[0], 0.1 * p[0], c.c00, c.c01, c.c10, c.c11, c.c20, c.c21]),
        wgsl: "var x0 = 1;
         if (rnd() < 0.5) { x0 = -1; }
         var y0 = 1;
         if (rnd() > 0.5) { y0 = -1; }
         let x = a.x + f32(x0);
         let y = a.y + f32(y0);
         let xr = f32(x0 ^ y0);
         let h = -D(1) + (1.0 - xr) * D(1);
         v.x += AMOUNT * (D(2) * x + D(4) * y + D(6));
         v.y += AMOUNT * (D(3) * x + D(5) * y + D(7));
         v.color = fmod(abs(v.color * 0.5 * (1.0 + h) + xr * (1.0 - h) * 0.5), 1.0);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "dc_circlesblue",
        params: dc_params![
            ParamDef::int("seed", 10000.0, 0.0, 100000.0),
            ParamDef::range("time", 1.0, 1.0, 1000.0),
            ParamDef::range("zoom", 1.0, 0.01, 100.0),
            ParamDef::range("radius", 0.04, 0.0, 1.0),
            ParamDef::int("bubles", 40.0, 1.0, 100.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [seed, time, zoom, radius, bubles, <dc base>(5..)]
        apply: |d, ctx, a, v, amount| {
            let uv = w06_dc_uv(d, 5, ctx, a, true);
            let color = circlesblue_rgb(d, uv);
            w06_dc_tail(d, 5, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = w06_limit(
                p,
                &[(1, 1.0, 1000.0, false), (2, 0.01, 100.0, false), (3, 0.0, 1.0, false), (4, 1.0, 100.0, true)],
            );
            w06_dc_limit(&mut d, 5);
            d
        }),
        wgsl: "var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
         if (i32(D(5)) == 1) { uv = vec2<f32>(a.x, a.y); }
         let tm = D(1);
         let q = uv * D(2);
         var color = vec3<f32>(0.0);
         let nb = i32(D(4));
         for (var i = 0; i < nb; i++) {
           let fi = f32(i);
           let pha = tan(fi * 6.0 + 1.0) * 0.5 + 0.5;
           let siz = pow(cos(fi * 2.4 + 5.0) * 0.5 + 0.5, 4.0);
           let pox = cos(fi * 3.55 + 4.1);
           let rad = D(3) + sin(fi) * 0.12 + 0.08;
           let pos = vec2<f32>(pox + sin(tm / 15.0 + pha + siz),
                               -1.0 - rad + (2.0 + 2.0 * rad) * glsl_mod(pha + 0.1 * (tm / 5.0) * (0.2 + 0.8 * siz), 1.0));
           let dis = length(q - pos);
           let col = mix(vec3<f32>(0.1, 0.2, 0.8), vec3<f32>(0.2, 0.8, 0.6), 0.5 + 0.5 * sin(fi * sin(tm * pox * 0.03) + 1.9));
           color += col * (1.0 - glsl_smoothstep(rad * (0.65 + 0.20 * sin(pox * tm)), rad, dis)) * (1.0 - cos(pox * tm));
         }
         dc_output(&v, AMOUNT, uv, color * 0.3, i32(D(6)), D(7), D(8), i32(D(9)));",
    },
    VarDef {
        name: "dc_circuits",
        params: dc_params![
            ParamDef::int("Seed", 1000000.0, 0.0, 10000000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("rate", 0.8, 0.0, 2.0),
            ParamDef::range("intensity", 0.9, 0.0, 2.0),
            ParamDef::range("focus", 1.5, 0.0, 6.28),
            ParamDef::range("pulse", 10.0, 0.0, 20.0),
            ParamDef::range("glow", 2.0, 0.0, 10.0),
            ParamDef::range("loops", 15.0, 1.0, 24.0),
            ParamDef::range("zoom", 1.0, 0.1, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [Seed, time, rate, intensity, focus, pulse, glow, loops, zoom, <dc base>(9..)]
        apply: |d, ctx, a, v, amount| {
            let uv = w06_dc_uv(d, 9, ctx, a, true);
            // JWildfire's `S` starts at (101 + glow) * intensity of the default parameters
            let key = d.as_ptr() as usize;
            let mut sv = ctx.orbits.get(&key).map_or((101.0 + 2.0) * 0.9, |st| st[0]);
            let color = circuits_rgb(d, uv, &mut sv);
            ctx.orbits.insert(key, [sv, 0.0]);
            w06_dc_tail(d, 9, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            w06_dc_limit(&mut d, 9);
            d
        }),
        wgsl: "fn dc_circuits_formula(z_in: vec2<f32>, t: f32, loops: f32, zoom: f32, intensity: f32, pulse: f32) -> vec4<f32> {
  var z = z_in;
  var m = 0.0;
  var ot = 1000.0;
  var ot2 = 1000.0;
  var k = floor(loops / 4.0 + floor(5.0 * zoom));
  for (var i = 0; i < 11; i++) {
    z = abs(z) / clamp(dot(z, z), 0.1, 0.5) - t;
    let l = length(z);
    let o = min(max(abs(min(z.x, z.y)), -l + 0.25), abs(l - 0.25));
    ot = min(ot, o);
    ot2 = min(l * 0.1, ot2);
    m = max(m, f32(i) * (1.0 - abs(sign(ot - o))));
    if (k <= 0.0) { break; }
    k -= 1.0;
  }
  m += 1.0;
  let w = (intensity * zoom) * m;
  let circ = java_pow(max(0.0, w - ot2) / w, 6.0);
  let ds = max(java_pow(max(0.0, w - ot) / w, 0.25), circ);
  let col = normalize(vec3<f32>(0.1) + vec3<f32>(0.45, 0.75, m * 0.1));
  let t2 = vec3<f32>(0.4 + glsl_mod(m / 9.0 - t * pulse + ot2 * 2.0, 1.0));
  return vec4<f32>(col * t2 + vec3<f32>(1.0, 0.7, 0.3) * (circ * (10.0 - m) * 3.0), ds);
}
// JWildfire carries `S` across samples; here per shader invocation (starting at 0, close to
// its steady state)
var<private> dc_circuits_s: f32;
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(9)) == 1) { uv = vec2<f32>(a.x, a.y); }
let rate = D(2);
let loops = D(7);
let zoom = D(8);
var rr = 0.0;
var nn = D(1) * 0.01 * rate;
var tt = 2.0 * rate;
if (nn > 6.0 * rate) {
  rr += 1.0;
  nn -= rr * 8.0 * rate;
}
if (nn < 4.0 * rate) { tt += nn; } else { tt = 8.0 * rate - nn; }
let zz = 1.05 - zoom;
var q = uv;
var sph = length(q) * 0.1;
sph = sqrt(1.0 - sph * sph) * 2.0;
let ca = tt * PI;
let cb = ca + tt;
let cc = cos(ca) + sin(cb);
q = q * mat2x2<f32>(cos(cb), sin(cb), -sin(cb), cos(cb));
q = q * mat2x2<f32>(cos(ca), -sin(ca), sin(ca), cos(ca));
q = q - vec2<f32>(sin(cc), cos(cc)) / PI;
q = q * zz;
let pix = 0.5 / 1000.0 * zz / sph;
let dof = zoom * D(4) + tt * 0.25;
var ll = floor(loops);
var s = dc_circuits_s;
var color = vec3<f32>(0.0);
for (var ai = 0; ai < 24; ai++) {
  let aauv = floor(vec2<f32>(f32(ai) / 6.0, glsl_mod(f32(ai), 6.0)));
  let r = dc_circuits_formula(q + aauv * pix * dof, tt, loops, zoom, D(3), D(5));
  color = r.xyz;
  s += r.w;
  if (ll <= 0.0) { break; }
  ll -= 1.0;
}
s /= floor(loops);
dc_circuits_s = s;
color = color / floor(loops);
let colo = mix(vec3<f32>(0.15), color, s) * (1.0 - length(uv)) * vec3<f32>(1.2, 1.1, 1.0);
dc_output(&v, AMOUNT, uv, colo, i32(D(10)), D(11), D(12), i32(D(13)));",
    },
    VarDef {
        name: "dc_cracklep_wf",
        params: &[
            ParamDef::int("preset", 0.0, 0.0, 20.0),
            ParamDef::range("cellsize", 1.0, 0.01, 4.0),
            ParamDef::range("power", 0.2, -2.0, 2.0),
            ParamDef::range("distort", 0.0, 0.0, 2.0),
            ParamDef::range("scale", 1.0, -2.0, 2.0),
            ParamDef::new("z", 0.0),
            ParamDef::new("color_scale", 0.5),
            ParamDef::new("color_offset", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [preset, cellsize, power, distort, scale, z, color_scale, color_offset]
        // `preset` only overwrites the other parameters when it is set (JWildfire applies the
        // stored values afterwards when loading a flame), so it has no effect here.
        apply: |d, ctx, a, v, amount| crackle_wf_apply(&d[1..], ctx, a, v, amount),
        prepare: None,
        wgsl: crackle_wf_wgsl!("dc_cracklep_wf", "1", "2", "3", "4", "5", "6", "7"),
    },
    VarDef {
        name: "dc_crackle_wf",
        params: &[
            ParamDef::range("cellsize", 1.0, 0.01, 4.0),
            ParamDef::range("power", 0.2, -2.0, 2.0),
            ParamDef::range("distort", 0.0, 0.0, 2.0),
            ParamDef::range("scale", 1.0, -2.0, 2.0),
            ParamDef::new("z", 0.0),
            ParamDef::new("color_scale", 0.5),
            ParamDef::new("color_offset", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [cellsize, power, distort, scale, z, color_scale, color_offset]
        apply: |d, ctx, a, v, amount| crackle_wf_apply(d, ctx, a, v, amount),
        prepare: None,
        wgsl: crackle_wf_wgsl!("dc_crackle_wf", "0", "1", "2", "3", "4", "5", "6"),
    },
    VarDef {
        name: "dc_cube",
        params: &[
            ParamDef::range("c1", 0.1, 0.0, 1.0),
            ParamDef::range("c2", 0.2, 0.0, 1.0),
            ParamDef::range("c3", 0.3, 0.0, 1.0),
            ParamDef::range("c4", 0.4, 0.0, 1.0),
            ParamDef::range("c5", 0.5, 0.0, 1.0),
            ParamDef::range("c6", 0.6, 0.0, 1.0),
            ParamDef::range("x", 1.0, 0.5, 2.5),
            ParamDef::range("y", 1.0, 0.5, 2.5),
            ParamDef::range("z", 1.0, 0.5, 2.5),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [c1, c2, c3, c4, c5, c6, x, y, z]
        apply: |d, ctx, _a, v, amount| {
            let p = 2.0 * ctx.random() - 1.0;
            let q = 2.0 * ctx.random() - 1.0;
            let i = ctx.random_int(i32::MAX as usize) % 3;
            let j = (ctx.random_int(i32::MAX as usize) & 1) == 1;
            let sg = if j { -1.0 } else { 1.0 };
            let (x, y, z) = match i {
                0 => {
                    v.color = if j { d[0] } else { d[1] };
                    (amount * sg, amount * p, amount * q)
                }
                1 => {
                    v.color = if j { d[2] } else { d[3] };
                    (amount * p, amount * sg, amount * q)
                }
                _ => {
                    v.color = if j { d[4] } else { d[5] };
                    (amount * p, amount * q, amount * sg)
                }
            };
            v.x += x * d[6];
            v.y += y * d[7];
            v.z += z * d[8];
        },
        prepare: Some(|p, _| w06_limit(p, &(0..6).map(|i| (i, 0.0, 1.0, false)).collect::<Vec<_>>())),
        wgsl: "let p = 2.0 * rnd() - 1.0;
         let q = 2.0 * rnd() - 1.0;
         let i = min(i32(rnd() * 3.0), 2);
         let j = rnd() < 0.5;
         var sg = 1.0;
         if (j) { sg = -1.0; }
         var x = AMOUNT * p;
         var y = AMOUNT * q;
         var z = AMOUNT * sg;
         if (i == 0) {
           x = AMOUNT * sg; y = AMOUNT * p; z = AMOUNT * q;
           if (j) { v.color = D(0); } else { v.color = D(1); }
         } else if (i == 1) {
           x = AMOUNT * p; y = AMOUNT * sg; z = AMOUNT * q;
           if (j) { v.color = D(2); } else { v.color = D(3); }
         } else {
           if (j) { v.color = D(4); } else { v.color = D(5); }
         }
         v.x += x * D(6);
         v.y += y * D(7);
         v.z += z * D(8);",
    },
    VarDef {
        name: "dc_cylinder2",
        params: &[
            ParamDef::new("offset", 0.0),
            ParamDef::new("angle", 0.0),
            ParamDef::new("scale", 0.5),
            ParamDef::new("x", 0.125),
            ParamDef::new("y", 0.125),
            ParamDef::new("blur", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [offset, angle, scale, x, y, blur, ldcs, cos(angle), sin(angle)]
        apply: |d, ctx, a, v, amount| {
            let ang = ctx.random() * M_2PI;
            let (sr, cr) = ang.sin_cos();
            let rr = d[5] * (ctx.gauss_ring(d.as_ptr() as usize, 4, 3) - 2.0);
            v.x += amount * (a.x + rr * sr).sin() * d[3];
            v.y += rr * a.y * d[4];
            v.z += amount * (a.x + rr * cr).cos();
            v.color = fmod((0.5 * (d[6] * (d[7] * v.x + d[8] * v.y + d[0]) + 1.0)).abs(), 1.0);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d.push(1.0 / if p[2] == 0.0 { 10e-6 } else { p[2] });
            d.push(p[1].cos());
            d.push(p[1].sin());
            d
        }),
        wgsl: "var<private> dc_cylinder2_ring: array<f32, 4>;
var<private> dc_cylinder2_idx: u32;
var<private> dc_cylinder2_init: bool;
fn dc_cylinder2_gauss() -> f32 {
  if (!dc_cylinder2_init) {
    dc_cylinder2_init = true;
    for (var i = 0; i < 4; i++) { dc_cylinder2_ring[i] = rnd(); }
  }
  let s = dc_cylinder2_ring[0] + dc_cylinder2_ring[1] + dc_cylinder2_ring[2] + dc_cylinder2_ring[3];
  dc_cylinder2_ring[dc_cylinder2_idx] = rnd();
  dc_cylinder2_idx = (dc_cylinder2_idx + 1u) & 3u;
  return s;
}
@body
let ang = rnd() * TWO_PI;
         let sr = sin(ang);
         let cr = cos(ang);
         let rr = D(5) * (dc_cylinder2_gauss() - 2.0);
         v.x += AMOUNT * sin(a.x + rr * sr) * D(3);
         v.y += rr * a.y * D(4);
         v.z += AMOUNT * cos(a.x + rr * cr);
         v.color = fmod(abs(0.5 * (D(6) * (D(7) * v.x + D(8) * v.y + D(0)) + 1.0)), 1.0);",
    },
    VarDef {
        name: "dc_dmodulus",
        params: &[
            ParamDef::range("x", 2.0, 0.5, 5.0),
            ParamDef::range("y", 2.0, 0.5, 5.0),
            ParamDef::range("angle", 45.0, 0.0, 90.0),
            ParamDef::new("shiftx", 0.0),
            ParamDef::new("shifty", 0.0),
            ParamDef::range("color1", 0.0, 0.0, 1.0),
            ParamDef::range("speed1", 0.0, 0.0, 1.0),
            ParamDef::range("color2", 1.0, 0.0, 1.0),
            ParamDef::range("speed2", 0.0, 0.0, 1.0),
            ParamDef::int("square", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: dmodulus_apply,
        // coscos/sinsin/sincos are computed by the Java `setParameter("angle")`, i.e. from the
        // stored angle when a flame is loaded.
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[9] = ftoi(p[9]).clamp(0, 1) as f64;
            let rangle = M_PI * p[2] / 180.0;
            let (s, c) = rangle.sin_cos();
            d.extend([
                2.0 * p[0],
                2.0 * p[1],
                (1.0 + p[6]) / 2.0,
                p[5] * (1.0 - p[6]) / 2.0,
                (1.0 + p[8]) / 2.0,
                p[7] * (1.0 - p[8]) / 2.0,
                c * c,
                s * s,
                s * c,
            ]);
            d
        }),
        wgsl: "fn dc_dmodulus_wrap(t: f32, e: f32, r: f32) -> f32 {
  if (t > e) { return -e + fmod(t + e, r); }
  if (t < -e) { return e - fmod(e - t, r); }
  return t;
}
@body
let x = D(0);
let y = D(1);
let cc = D(16);
let ss = D(17);
let sc = D(18);
let p2 = cc / (cc + ss);
let p1 = ss / (cc + ss);
let le45 = D(2) <= 45.0;
let xnew = dc_dmodulus_wrap(a.x, x, D(10));
let ynew = dc_dmodulus_wrap(a.y, y, D(11));
// GPU: the colour recurrence is simulated over 16 random branch choices from 0.5
var old = 0.5;
for (var i = 0; i < 16; i++) {
  let pp = rnd();
  var ba = !(pp < p1);
  if (le45) { ba = pp < p2; }
  if (ba) { old = old * D(12) + D(13); } else { old = old * D(14) + D(15); }
}
let pp = rnd();
var ba = !(pp < p1);
if (le45) { ba = pp < p2; }
var xt: f32;
var yt: f32;
var nc: f32;
if (ba) {
  xt = xnew * cc + ynew * sc + x;
  yt = -xnew * sc + ynew * cc - y;
  nc = old * D(12) + D(13);
} else {
  xt = xnew * sc + ynew * ss;
  yt = -xnew * ss + ynew * sc;
  nc = old * D(14) + D(15);
}
xt += D(3);
yt += D(4);
if (i32(D(9)) == 0) {
  v.x += AMOUNT * xt;
  v.y += AMOUNT * yt;
} else {
  v.x += AMOUNT * dc_dmodulus_wrap(xt, x, D(10));
  v.y += AMOUNT * dc_dmodulus_wrap(yt, y, D(11));
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
v.color = fmod(nc, 1.0);",
    },
    VarDef {
        name: "dc_ducks",
        params: dc_params![
            ParamDef::int("seed", 10000.0, 0.0, 100000.0),
            ParamDef::range("time", 10.0, 0.0, 100.0),
            ParamDef::range("zoom", 1.0, 0.1, 5.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [seed, time, zoom, <dc base>(3..)]
        apply: |d, ctx, a, v, amount| {
            let uv = w06_dc_uv(d, 3, ctx, a, true);
            let t1 = ducks_f(uv * d[2], d[1] / 65.0 * 30.0);
            w06_dc_tail(d, 3, ctx, v, amount, uv, vec3(t1.z, t1.x, t1.y));
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            w06_dc_limit(&mut d, 3);
            d
        }),
        wgsl: "fn dc_ducks_f(e_in: vec2<f32>, t: f32) -> vec3<f32> {
  var e = e_in;
  var c = 0.0;
  let add = vec2<f32>(0.1 * sin(t / 3.0) - 0.1, 5.0 + 0.1 * cos(t / 5.0));
  for (var i = 0; i < 50; i++) {
    let a2 = vec2<f32>(e.x, abs(e.y));
    e = vec2<f32>(log(length(a2)), glsl_atan2(a2.y, a2.x) - 6.2) + add;
    c += length(e);
  }
  let d = log2(log2(c * 0.05)) * 6.0;
  let flori = vec3<f32>(0.1 + 0.7 * sin(d), 0.1 + 0.5 * sin(d - 0.7), 0.7 + 0.7 * cos(d - 0.7));
  return floor(vec3<f32>(0.5) + flori * 1.1) / 1.1;
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
let t1 = dc_ducks_f(uv * D(2), D(1) / 65.0 * 30.0);
dc_output(&v, AMOUNT, uv, vec3<f32>(t1.z, t1.x, t1.y), i32(D(4)), D(5), D(6), i32(D(7)));",
    },
];

#[cfg(test)]
mod tests {
    use super::super::{Ctx, Pt};
    use super::*;
    use crate::model::Affine;

    const NAMES: &[&str] = &[
        "cut_tstruchet",
        "cut_vasarely",
        "cut_web",
        "cut_wood",
        "cut_x",
        "cut_yuebing",
        "cut_zigzag",
        "cyclone",
        "cylinder3D",
        "cylinder_apo",
        "dc_acrilic",
        "dc_apollonian",
        "dc_booleans",
        "dc_bubble",
        "dc_butterflies",
        "dc_cairotiles",
        "dc_carpet3D",
        "dc_carpet",
        "dc_circlesblue",
        "dc_circuits",
        "dc_cracklep_wf",
        "dc_crackle_wf",
        "dc_cube",
        "dc_cylinder2",
        "dc_dmodulus",
        "dc_ducks",
    ];

    const POINTS: &[(f64, f64, f64)] = &[
        (0.3, 0.2, 0.1),
        (-0.7, 0.5, -0.2),
        (1.3, -0.9, 0.4),
        (0.05, -0.02, 0.0),
        (-2.1, -1.7, 1.1),
        (0.6, 0.0, -0.5),
    ];

    fn def(name: &str) -> &'static VarDef {
        DEFS.iter().find(|d| d.name == name).unwrap()
    }

    fn run(def: &VarDef, data: &[f64], ctx: &mut Ctx, p: (f64, f64, f64)) -> (Pt, Pt) {
        let mut a = Pt {
            x: p.0,
            y: p.1,
            z: p.2,
            color: 0.5,
            ..Default::default()
        };
        let mut v = Pt {
            x: 0.1,
            y: -0.2,
            z: 0.05,
            color: 0.5,
            ..Default::default()
        };
        (def.apply)(data, ctx, &mut a, &mut v, 0.8);
        (a, v)
    }

    #[test]
    fn all_names_present_and_unique() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert_eq!(DEFS.iter().filter(|d| d.name == *n).count(), 1, "{n}");
        }
    }

    #[test]
    fn default_params_give_finite_results() {
        let mut ctx = Ctx::new(42, true);
        for def in DEFS {
            let data = def.prepare_data(&[], &Affine::IDENTITY);
            assert!(def.supports_gpu(), "{} has no WGSL", def.name);
            for &p in POINTS {
                for _ in 0..30 {
                    let (a, v) = run(def, &data, &mut ctx, p);
                    assert!(
                        !a.is_bad() && !v.is_bad() && v.color.is_finite(),
                        "{} at {:?}: a={:?} v={:?}",
                        def.name,
                        p,
                        a,
                        v
                    );
                    if v.rgb_color {
                        assert!(
                            [v.red, v.green, v.blue]
                                .iter()
                                .all(|c| (0.0..=255.0).contains(c)),
                            "{}",
                            def.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn color_only_mode_is_finite() {
        // DC variations sampling the affine point instead of random positions
        let mut ctx = Ctx::new(5, false);
        for (name, idx) in [
            ("dc_acrilic", 13),
            ("dc_apollonian", 2),
            ("dc_booleans", 3),
            ("dc_butterflies", 6),
            ("dc_cairotiles", 3),
            ("dc_circlesblue", 5),
            ("dc_circuits", 9),
            ("dc_ducks", 3),
        ] {
            let def = def(name);
            let mut params: Vec<f64> = def.params.iter().map(|p| p.default).collect();
            params[idx] = 1.0;
            params[idx + 1] = 2.0;
            let data = def.prepare_data(&params, &Affine::IDENTITY);
            for &p in POINTS {
                let (_, v) = run(def, &data, &mut ctx, p);
                assert!(!v.is_bad() && v.color.is_finite(), "{name} {p:?} {v:?}");
                assert!(!v.rgb_color, "{name}");
                // dc_butterflies assigns x/y instead of adding
                let x0 = if name == "dc_butterflies" { 0.0 } else { 0.1 };
                assert!((p.0 * 0.8 + x0 - v.x).abs() < 1e-12, "{name}");
            }
        }
    }

    #[test]
    fn java_random_matches_java_util_random() {
        // new java.util.Random(0).nextDouble() / new Random(42).nextDouble()
        assert!((w06_java_random_doubles::<1>(0)[0] - 0.730967787376657).abs() < 1e-15);
        assert!((w06_java_random_doubles::<1>(42)[0] - 0.7275636800328681).abs() < 1e-15);
    }

    #[test]
    fn cut_variations_hide_part_of_the_samples() {
        let mut ctx = Ctx::new(9, false);
        for name in [
            "cut_tstruchet",
            "cut_vasarely",
            "cut_web",
            "cut_wood",
            "cut_x",
            "cut_yuebing",
            "cut_zigzag",
        ] {
            let def = def(name);
            let data = def.prepare_data(&[], &Affine::IDENTITY);
            let mut hidden = 0;
            for _ in 0..2000 {
                let (_, v) = run(def, &data, &mut ctx, (0.1, 0.2, 0.0));
                if v.do_hide {
                    hidden += 1;
                }
            }
            assert!(hidden > 0 && hidden < 2000, "{name}: {hidden}");
        }
    }

    #[test]
    fn cylinder3d_points_lie_inside_the_cylinder() {
        let def = def("cylinder3D");
        let data = def.prepare_data(&[], &Affine::IDENTITY);
        let mut ctx = Ctx::new(3, false);
        for _ in 0..500 {
            let mut a = Pt::default();
            let mut v = Pt::default();
            (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
            if !v.do_hide {
                assert!((v.x * v.x + v.z * v.z).sqrt() < 0.3 + 1e-12 && v.y.abs() < 0.25 + 1e-12);
            }
        }
    }
}
