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

//! Variations ported from the JWildfire sources (batch w04).
//!
//! Most of them are the `cut_*` family: a pattern is evaluated at the input point (mode 0) or at a
//! random point (mode 1) and the point is hidden (`doHide`, GPU: `v.hide`) inside or outside the
//! pattern.

#[allow(unused_imports)]
use super::glsl::*;
#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

/// `java.util.Random` (48-bit LCG), used by the `init()` methods seeded with a parameter.
struct JavaRandom(u64);

impl JavaRandom {
    const MULT: u64 = 0x5_DEEC_E66D;
    const MASK: u64 = (1 << 48) - 1;

    fn new(seed: i64) -> Self {
        Self((seed as u64 ^ Self::MULT) & Self::MASK)
    }

    fn next(&mut self, bits: u32) -> i64 {
        self.0 = self.0.wrapping_mul(Self::MULT).wrapping_add(0xB) & Self::MASK;
        (self.0 >> (48 - bits)) as i64
    }

    fn next_double(&mut self) -> f64 {
        ((self.next(26) << 27) + self.next(27)) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

/// `x0 = seed * r.nextDouble(); y0 = seed * r.nextDouble()` with `r = new Random(seed)`.
fn cut_seed_offset(seed: f64) -> [f64; 2] {
    let seed = seed as i32;
    let mut r = JavaRandom::new(seed as i64);
    let x0 = seed as f64 * r.next_double();
    let y0 = seed as f64 * r.next_double();
    [x0, y0]
}

/// `(int) Tools.limitValue(v, lo, hi)` as f64.
#[inline]
fn limit_int(v: f64, lo: f64, hi: f64) -> f64 {
    limit_value(v, lo, hi) as i32 as f64
}

/// Input point of the cut_* variations: the affine point (mode 0) or `s * random + o` (mode 1).
#[inline]
fn cut_input(ctx: &mut Ctx, a: &Pt, mode: f64, s: f64, o: f64) -> (f64, f64) {
    if mode as i32 == 0 {
        (a.x, a.y)
    } else {
        let x = s * ctx.random() + o;
        let y = s * ctx.random() + o;
        (x, y)
    }
}

/// Shared tail of the cut_* variations: `doHide`, `pVarTP.x = amount * (x - c)`.
#[inline]
#[allow(clippy::too_many_arguments)]
fn cut_output(ctx: &Ctx, a: &Pt, v: &mut Pt, amount: f64, x: f64, y: f64, c: f64, hide: bool) {
    v.do_hide = hide;
    let (x, y) = if hide { (0.0, 0.0) } else { (x, y) };
    v.x = amount * (x - c);
    v.y = amount * (y - c);
    ctx.pass_z(a, v, amount);
}

/// `invert == 0 ? color > t : color <= t`
#[inline]
fn cut_hide(invert: f64, color: f64, t: f64) -> bool {
    if invert as i32 == 0 { color > t } else { color <= t }
}

/// WGSL prologue of the cut_* variations: `x`, `y` = affine point (mode 0) or `S * rnd() + O`
/// (mode 1); `cx` = output centre (`C` in mode 1, 0 otherwise).
macro_rules! cut_wgsl_head {
    ($mode:literal, $s:literal, $o:literal, $c:literal) => {
        concat!(
            "  var x = a.x;\n  var y = a.y;\n  var cx = 0.0;\n  if (i32(D(",
            $mode,
            ")) != 0) {\n    x = ",
            $s,
            " * rnd() + ",
            $o,
            ";\n    y = ",
            $s,
            " * rnd() + ",
            $o,
            ";\n    cx = ",
            $c,
            ";\n  }\n"
        )
    };
}

/// WGSL epilogue of the cut_* variations (expects `hide: bool`).
macro_rules! cut_wgsl_tail {
    () => {
        "
  v.hide = select(0.0, 1.0, hide);
  if (hide) { x = 0.0; y = 0.0; }
  v.x = AMOUNT * (x - cx);
  v.y = AMOUNT * (y - cx);
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
"
    };
}

/// `invert == 0 ? color > T : color <= T` in WGSL (`color`, invert index, threshold).
macro_rules! cut_wgsl_hide {
    ($inv:literal, $t:literal) => {
        concat!("  let hide = select(color > ", $t, ", color <= ", $t, ", i32(D(", $inv, ")) != 0);\n")
    };
}

// ---------------------------------------------------------------------------------------------
// curliecue (DrawFunc: random primitive of a precalculated curlicue curve)
// ---------------------------------------------------------------------------------------------

/// Symmetry images: 0 (x,-y) 1 (-x,y) 2 (y,x) 3 (-y,-x) 4 (x,y) 5 (-x,-y) 6 (y,-x) 7 (-y,x).
#[inline]
fn curliecue_tf(t: usize, x: f64, y: f64) -> (f64, f64) {
    match t {
        0 => (x, -y),
        1 => (-x, y),
        2 => (y, x),
        3 => (-y, -x),
        4 => (x, y),
        5 => (-x, -y),
        6 => (y, -x),
        _ => (-y, x),
    }
}

/// Images used per symmetry (1..4); the line images are the same sets.
const CURLIECUE_SYM: [[usize; 8]; 4] =
    [[0, 0, 0, 0, 0, 0, 0, 0], [0, 1, 2, 3, 0, 0, 0, 0], [0, 1, 5, 4, 0, 0, 0, 0], [0, 1, 2, 3, 4, 5, 6, 7]];
const CURLIECUE_NT: [usize; 4] = [1, 4, 4, 8];
const CURLIECUE_HDR: usize = 10;

#[inline]
fn f32r(v: f64) -> f64 {
    v as f32 as f64
}

/// `DrawFunc.plotLine` (line_thickness is 0: curliecue does not call `super.init`).
fn curliecue_line(ctx: &mut Ctx, x1: f64, y1: f64, x2: f64, y2: f64) -> (f64, f64) {
    let ydiff = y2 - y1;
    let xdiff = x2 - x1;
    let m = if xdiff == 0.0 { 10000.0 } else { ydiff / xdiff };
    let line_length = (xdiff * xdiff + ydiff * ydiff).sqrt();
    let d = ctx.random() * line_length;
    let mut xoffset = d / (1.0 + m * m).sqrt();
    if x2 < x1 {
        xoffset = -xoffset;
    }
    let mut yoffset = (m * xoffset).abs();
    if y2 < y1 {
        yoffset = -yoffset;
    }
    (x1 + xoffset, y1 + yoffset)
}

/// `DrawFunc.plotPolygon` / `randXY` (nBlur) for an Ngon with `fill = 0`.
fn curliecue_ngon(ctx: &mut Ctx, sides: f64) -> (f64, f64) {
    let n_edges = sides.max(3.0);
    let mid_angle = M_2PI / n_edges;
    let tan90_m_2 = (M_PI_2 + mid_angle / 2.0).tan();
    let arc_tan1 = 13.0 / n_edges.powf(1.3);
    let arc_tan2 = 2.0 * (arc_tan1 / (-2.0)).atan();
    let r1 = ctx.random();
    let ri = (ctx.random_int(32767) % n_edges as usize) as f64;
    let mut ang_xy = ((arc_tan1 * (r1 - 0.5)).atan() / arc_tan2 + 0.5 + ri) * mid_angle;
    let x = ang_xy.sin();
    let y = ang_xy.cos();
    while ang_xy > mid_angle {
        ang_xy -= mid_angle;
    }
    let x_tmp = tan90_m_2 / (tan90_m_2 - ang_xy.tan());
    let y_tmp = x_tmp * ang_xy.tan();
    let len_outer = (x_tmp * x_tmp + y_tmp * y_tmp).sqrt();
    let _ = ctx.random(); // Java draws a first radius and overwrites it
    let ran = ctx.random().sqrt() * len_outer;
    let (x, y) = (x * ran, y * ran);
    let (sa, ca) = (mid_angle / 2.0).sin_cos();
    (ca * x - sa * y, sa * x + ca * y)
}

fn curliecue_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    // d = [Points, seed, type, sides, scale, symmetry, eccentricity, showlines, nt, per_step,
    //      x_0, y_0, x_1, y_1, ... x_n, y_n]
    let size = limit_int(p[0], 1.0, 10000.0);
    let seed = p[1] as i32;
    let typ = limit_int(p[2], 0.0, 1.0);
    let sides = limit_int(p[3], 3.0, 20.0);
    let scale = limit_value(p[4], 0.0, 1.0);
    let sym = limit_int(p[5], 1.0, 4.0);
    let ecce = limit_value(p[6], 0.0, 1.0);
    let showlines = limit_int(p[7], 0.0, 1.0);
    let nt = CURLIECUE_NT[sym as usize - 1];
    let per_step = 2 * nt + if showlines as i32 == 1 { nt } else { 0 };
    let s = JavaRandom::new(seed as i64).next_double();
    let n = size as usize;
    let mut d = Vec::with_capacity(CURLIECUE_HDR + 2 * n + 2);
    d.extend_from_slice(&[size, seed as f64, typ, sides, scale, sym, ecce, showlines, nt as f64, per_step as f64]);
    let (mut theta, mut phi, mut x0, mut y0) = (0.0f64, 0.0f64, ecce, ecce);
    d.push(x0);
    d.push(y0);
    for _ in 0..n {
        let x1 = x0 + 0.01 * phi.cos();
        let y1 = y0 + 0.01 * phi.sin();
        d.push(x1);
        d.push(y1);
        x0 = x1;
        y0 = y1;
        phi = (theta + phi) % M_2PI;
        theta = (theta + M_2PI * s) % M_2PI;
    }
    d
}

fn curliecue_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let size = d[0];
    let typ = d[2] as i32;
    let scale = d[4];
    let sym = d[5] as usize;
    let nt = d[8] as usize;
    let per = d[9] as usize;
    let total = size as usize * per;
    let idx = ((ctx.random() * total as f64) as usize).min(total - 1);
    let step = idx / per;
    let k = idx % per;
    let o = CURLIECUE_HDR + 2 * step;
    let (x0, y0, x1, y1) = (d[o], d[o + 1], d[o + 2], d[o + 3]);
    if k < 2 * nt {
        let t = CURLIECUE_SYM[sym - 1][k / 2];
        let (px, py) = if k % 2 == 0 { curliecue_tf(t, x0, y0) } else { curliecue_tf(t, x1, y1) };
        if typ == 0 {
            // Points (symmetry 1 stores them as float)
            let (px, py) = if sym == 1 { (f32r(px), f32r(py)) } else { (px, py) };
            let r = ctx.random() * M_2PI;
            let r2 = scale * ctx.random();
            v.x += amount * (px + r2 * r.cos());
            v.y += amount * (py + r2 * r.sin());
        } else {
            // Ngon(sides, scale, angle 0, pos, color, fill 0)
            let (ox, oy) = curliecue_ngon(ctx, d[3]);
            v.x += amount * (ox * scale + px);
            v.y += amount * (oy * scale + py);
        }
    } else {
        let t = CURLIECUE_SYM[sym - 1][k - 2 * nt];
        let (ax, ay) = curliecue_tf(t, x0, y0);
        let (bx, by) = curliecue_tf(t, x1, y1);
        let (ox, oy) = curliecue_line(ctx, f32r(ax), f32r(ay), f32r(bx), f32r(by));
        v.x += amount * ox;
        v.y += amount * oy;
    }
    v.color = step as f64 / size;
    ctx.pass_z(a, v, amount);
}

const CURLIECUE_WGSL: &str = "fn curliecue_tf(t: i32, p: vec2<f32>) -> vec2<f32> {
  switch t {
    case 0: { return vec2<f32>(p.x, -p.y); }
    case 1: { return vec2<f32>(-p.x, p.y); }
    case 2: { return vec2<f32>(p.y, p.x); }
    case 3: { return vec2<f32>(-p.y, -p.x); }
    case 4: { return vec2<f32>(p.x, p.y); }
    case 5: { return vec2<f32>(-p.x, -p.y); }
    case 6: { return vec2<f32>(p.y, -p.x); }
    default: { return vec2<f32>(-p.y, p.x); }
  }
}
fn curliecue_tid(sym: i32, k: i32) -> i32 {
  if (sym == 1) { return 0; }
  if (sym == 3) {
    if (k == 2) { return 5; }
    if (k == 3) { return 4; }
  }
  return k;
}
fn curliecue_line(p1: vec2<f32>, p2: vec2<f32>) -> vec2<f32> {
  let diff = p2 - p1;
  var m = 10000.0;
  if (diff.x != 0.0) { m = diff.y / diff.x; }
  let dd = rnd() * length(diff);
  var xo = dd / sqrt(1.0 + m * m);
  if (p2.x < p1.x) { xo = -xo; }
  var yo = abs(m * xo);
  if (p2.y < p1.y) { yo = -yo; }
  return p1 + vec2<f32>(xo, yo);
}
fn curliecue_ngon(sides: f32) -> vec2<f32> {
  let ne = max(sides, 3.0);
  let mid = TWO_PI / ne;
  let tan90_m_2 = tan(HALF_PI + mid / 2.0);
  let at1 = 13.0 / pow(ne, 1.3);
  let at2 = 2.0 * atan(at1 / -2.0);
  let r1 = rnd();
  let ri = f32(i32(rnd() * 32767.0) % i32(ne));
  var ang = (atan(at1 * (r1 - 0.5)) / at2 + 0.5 + ri) * mid;
  let x = sin(ang);
  let y = cos(ang);
  for (var i = 0; i < 64 && ang > mid; i++) { ang -= mid; }
  let xt = tan90_m_2 / (tan90_m_2 - tan(ang));
  let yt = xt * tan(ang);
  let lo = sqrt(xt * xt + yt * yt);
  _ = rnd();
  let ran = sqrt(rnd()) * lo;
  let sa = sin(mid / 2.0);
  let ca = cos(mid / 2.0);
  return vec2<f32>(ca * x * ran - sa * y * ran, sa * x * ran + ca * y * ran);
}
@body
  let size = D(0);
  let sym = i32(D(5));
  let scale = D(4);
  let nt = i32(D(8));
  let per = i32(D(9));
  let total = i32(size) * per;
  let idx = clamp(i32(rnd() * f32(total)), 0, total - 1);
  let st = idx / per;
  let k = idx - st * per;
  let o = base + 10u + 2u * u32(st);
  let p0 = vec2<f32>(prm[o], prm[o + 1u]);
  let p1 = vec2<f32>(prm[o + 2u], prm[o + 3u]);
  if (k < 2 * nt) {
    let p = curliecue_tf(curliecue_tid(sym, k / 2), select(p1, p0, k % 2 == 0));
    if (i32(D(2)) == 0) {
      let r = rnd() * TWO_PI;
      let r2 = scale * rnd();
      v.x += AMOUNT * (p.x + r2 * cos(r));
      v.y += AMOUNT * (p.y + r2 * sin(r));
    } else {
      let q = curliecue_ngon(D(3));
      v.x += AMOUNT * (q.x * scale + p.x);
      v.y += AMOUNT * (q.y * scale + p.y);
    }
  } else {
    let t = curliecue_tid(sym, k - 2 * nt);
    let q = curliecue_line(curliecue_tf(t, p0), curliecue_tf(t, p1));
    v.x += AMOUNT * q.x;
    v.y += AMOUNT * q.y;
  }
  v.color = f32(st) / size;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

// ---------------------------------------------------------------------------------------------
// curl_sp helpers
// ---------------------------------------------------------------------------------------------

#[inline]
fn curl_sp_spread(a: f64, b: f64) -> f64 {
    (sqr(a) + sqr(b)).sqrt() * if a > 0.0 { 1.0 } else { -1.0 }
}

#[inline]
fn curl_sp_powq4c(x: f64, y: f64) -> f64 {
    if y == 1.0 { x } else { x.abs().powf(y) * sgn(x) }
}

// ---------------------------------------------------------------------------------------------
// cut_2ewangtile helpers
// ---------------------------------------------------------------------------------------------

const WANG_K: f64 = 0.1;
const WANG_K2: f64 = (1.0 - WANG_K) / 2.0;

fn cut_2ewangtile_tile0(uv: Vec2) -> f64 {
    let k3 = 2f64.sqrt() * 0.5 - WANG_K2;
    let v = length(uv) - k3;
    let w = WANG_K2 - length(vec2(uv.x.abs() - 0.5, uv.y - 0.5));
    mix(v, w, step(uv.x.abs(), uv.y))
}

fn cut_2ewangtile_tile1(uv: Vec2) -> f64 {
    (length(uv - 0.5) - 0.5).abs() - WANG_K * 0.5
}

fn cut_2ewangtile_tile2(uv: Vec2) -> f64 {
    uv.x.abs() - WANG_K * 0.5
}

fn cut_2ewangtile_tile3(uv: Vec2) -> f64 {
    java_max(-uv.x - WANG_K * 0.5, WANG_K2 - length(vec2(uv.x - 0.5, uv.y.abs() - 0.5)))
}

fn cut_2ewangtile_tile4(uv: Vec2) -> f64 {
    WANG_K2 - length(vec2(uv.x.abs() - 0.5, uv.y.abs() - 0.5))
}

fn cut_2ewangtile_tile(uv: Vec2, tile: i32) -> f64 {
    match tile {
        1 => java_max(cut_2ewangtile_tile0(uv), 0.15 - length(uv)),
        2 => cut_2ewangtile_tile0(vec2(uv.y, uv.x)),
        3 => cut_2ewangtile_tile1(uv),
        4 => cut_2ewangtile_tile0(vec2(uv.x, -uv.y)),
        5 => cut_2ewangtile_tile2(uv),
        6 => cut_2ewangtile_tile1(vec2(uv.x, -uv.y)),
        7 => cut_2ewangtile_tile3(uv),
        8 => cut_2ewangtile_tile0(vec2(uv.y, -uv.x)),
        9 => cut_2ewangtile_tile1(vec2(-uv.x, uv.y)),
        10 => cut_2ewangtile_tile2(vec2(uv.y, uv.x)),
        11 => cut_2ewangtile_tile3(vec2(uv.y, uv.x)),
        12 => cut_2ewangtile_tile1(vec2(-uv.x, -uv.y)),
        13 => cut_2ewangtile_tile3(vec2(-uv.x, uv.y)),
        14 => cut_2ewangtile_tile3(vec2(-uv.y, uv.x)),
        15 => cut_2ewangtile_tile4(uv),
        _ => 1.414,
    }
}

fn cut_2ewangtile_hash(p: Vec2) -> f64 {
    let mut p3 = fract(vec3(p.x, p.y, p.x) * 0.1031);
    p3 = p3 + dot(p3, p3.yzx() + 19.19);
    fract((p3.x + p3.y) * p3.z)
}

fn cut_2ewangtile_map(uv: Vec2) -> f64 {
    let mut b = 0;
    let uv = uv + 0.5;
    let id = floor(uv);
    if cut_2ewangtile_hash(id) >= 0.5 {
        b += 1;
    }
    if cut_2ewangtile_hash(id * -1.0) >= 0.5 {
        b += 8;
    }
    if cut_2ewangtile_hash(id - vec2(0.0, 1.0)) >= 0.5 {
        b += 4;
    }
    if cut_2ewangtile_hash((id + vec2(1.0, 0.0)) * -1.0) >= 0.5 {
        b += 2;
    }
    cut_2ewangtile_tile(fract(uv) - 0.5, b)
}

// ---------------------------------------------------------------------------------------------
// misc cut_* helpers
// ---------------------------------------------------------------------------------------------

fn cut_alientext_random2d(n: Vec2) -> f64 {
    fract(dot(n, vec2(129.9898, 4.1414)).sin() * 2398.5453)
}

fn cut_alientext_letter(coord: Vec2, size: f64) -> f64 {
    let gp = floor(coord / size * 7.0);
    let rp = floor(fract(coord / size) * 7.0);
    let odd = fract(rp * 0.5) * 2.0;
    let rnd = cut_alientext_random2d(gp);
    let mut c = java_max(odd.x, odd.y) * step(0.5, rnd);
    c += java_min(odd.x, odd.y);
    c *= rp.x * (6.0 - rp.x);
    c *= rp.y * (6.0 - rp.y);
    clamp(c, 0.0, 1.0)
}

fn cut_btruchet_hash1(n: i32) -> f64 {
    let n = (n << 13) ^ n;
    let n = n.wrapping_mul(n.wrapping_mul(n).wrapping_mul(15731).wrapping_add(789221)).wrapping_add(1376312589);
    n as f64 / 0x7fff_ffff as f64
}

fn cut_btruchet_tile2(mut uv: Vec2) -> f64 {
    if uv.y < -uv.x {
        uv.x = -uv.x;
        uv.y = -uv.y;
    }
    (distance(uv, vec2(1.0, 1.0)) - 1.0).abs()
}

fn cut_celtic_circ(uv: Vec2, r: f64) -> f64 {
    let d = length(uv);
    smoothstep(d, d + 0.02, r)
}

fn cut_celtic_shape(uv: Vec2) -> f64 {
    let c1 = cut_celtic_circ(uv, 0.38);
    let c5 = cut_celtic_circ(uv, 0.45);
    let c2 = cut_celtic_circ(uv + vec2(0.5, -0.288675), 0.45);
    let c3 = cut_celtic_circ(uv + vec2(-0.5, -0.288675), 0.45);
    let c4 = cut_celtic_circ(uv + vec2(0.0, 0.57735), 0.45);
    let d = c5 - c2 - c3 - c4;
    mix(0.0, d, c1)
}

/// One of the three hexagonal cell coordinates (`uv1/uv2/uv3`), offset (ox, oy) after zooming.
fn cut_celtic_cell(x: f64, y: f64, zoom: f64, ox: f64, oy: f64) -> Vec2 {
    let mut uv = vec2(x * 1.0, y * 1.155) * zoom;
    uv.x += ox;
    uv.y += oy;
    let m = glsl_mod(uv.y, 2.0);
    uv.x += step(1.0, m) * 0.5;
    uv = fract(uv);
    uv.y /= 1.155;
    uv.x -= 1.155 / 2.0;
    uv.y -= 0.5;
    uv
}

fn cut_fingerprint_hash2(p: Vec2) -> Vec2 {
    let q = vec2(dot(p, vec2(63.31, 127.63)), dot(p, vec2(395.467, 213.799)));
    fract(sin(q) * 43141.59265) * 2.0 - 1.0
}

const FINGERPRINT_N: usize = 50;
const FINGERPRINT_HDR: usize = 7;

fn cut_fun_pix(u: Vec2) -> f64 {
    let co = floor(u);
    step(0.5, fract(dot(co, vec2(12.9898, 78.233)).sin() * 43758.5453))
}

fn cut_fun_hm(uv: Vec2) -> f64 {
    let nuv = fract(uv) * 2.0 - 1.0;
    let pos = sign(nuv) * 0.9;
    let d = length(nuv);
    let mut v = cut_fun_pix(uv);
    if d < 1.0 {
        return v;
    }
    let v0 = v;
    let v1 = cut_fun_pix(vec2(pos.x, 0.0) + uv);
    let v2 = cut_fun_pix(vec2(0.0, pos.y) + uv);
    if v1 == v2 {
        v = if v == cut_fun_pix(uv + pos) && v != v1 { 0.0 } else { v1 };
    }
    mix(v0, v, smoothstep(d, 1.0, 1.01))
}

const HEX_SQRT3: f64 = 1.73205;

fn cut_hextruchetflow_pix_to_hex(p: Vec2) -> Vec2 {
    let t0 = vec2((1.0 / HEX_SQRT3) * p.x - (1.0 / 3.0) * p.y, (2.0 / 3.0) * p.y);
    let c = vec3(t0.x, -t0.x - t0.y, t0.y);
    let mut r = floor(c + 0.5);
    let dr = abs(r - c);
    r = r - step(dr.yzx(), dr) * step(dr.zxy(), dr) * dot(r, Vec3::splat(1.0));
    vec2(r.x, r.z)
}

fn cut_hextruchetflow_scene(p: Vec2, grid: i32) -> f64 {
    let mut col = 0.0;
    let cid = cut_hextruchetflow_pix_to_hex(p);
    let pc = vec2(HEX_SQRT3 * (cid.x + 0.5 * cid.y), 1.5 * cid.y);
    let hash = fract(dot(cid, vec2(37.0, 39.0)).sin() * 43758.54);
    let dir = 2.0 * step(hash, 0.5) - 1.0;
    let t0 = pc + vec2(0.0, -dir);
    let mut w = vec3(t0.x, t0.y, dot(t0 - p, t0 - p));
    let mut q = pc + vec2(HEX_SQRT3 / 2.0, 0.5 * dir);
    let mut d = dot(q - p, q - p);
    if d < w.z {
        w = Vec3::from_vec2(q, d);
    }
    q = pc + vec2(-HEX_SQRT3 / 2.0, 0.5 * dir);
    d = dot(q - p, q - p);
    if d < w.z {
        w = Vec3::from_vec2(q, d);
    }
    w.z = (w.z.sqrt() - 0.5).abs();
    let pe = abs(p - pc);
    d = (HEX_SQRT3 / 2.0) - pe.x + 0.5 * min(pe.x - HEX_SQRT3 * pe.y, 0.0);
    if grid == 1 {
        col = mix(1.0, smoothstep(1.0, 1.0, d), smoothstep(0.01, 0.02, d));
    }
    if w.z < 0.25 {
        col = 1.0;
    }
    col
}

fn cut_kleinian_wrap(x: f64, a: f64, s: f64) -> f64 {
    let x = x - s;
    (x - a * (x / a).floor()) + s
}

/// `JosKleinian`; reproduces the Java aliasing of `lz` with `z` (in-place `z.x` updates).
fn cut_kleinian_hit(mut z: Vec2, a: f64, b: f64, box_size: f64, iters: i32) -> i32 {
    let mut lz = z + 1.0;
    let mut llz = z + -1.0;
    let f = sign(b);
    for i in 0..iters {
        z.x += f * b / a * z.y;
        z.x = cut_kleinian_wrap(z.x, 2.0 * box_size, -box_size);
        z.x -= f * b / a * z.y;
        if i > 0 {
            lz = z;
        }
        let zb = z.x + b * 0.5;
        if z.y >= a * 0.5 + f * (2.0 * a - 1.95) / 4.0 * sign(zb) * (1.0 - (-(7.2 - (1.95 - a) * 15.0) * zb.abs()).exp()) {
            z = vec2(-b, a) - z;
        }
        // TransA
        let ir = 1.0 / dot(z, z);
        z = z * -ir;
        z.x = -b - z.x;
        z.y += a;
        if dot(z - llz, z - llz) < 1.0e-6 {
            break;
        }
        if z.y < 0.0 || z.y > a {
            return 1;
        }
        llz = lz;
        lz = z;
    }
    0
}

const MAGFIELD_HDR: usize = 7;

fn cut_magfield_velocity(d: &[f64], p: Vec2) -> Vec2 {
    let mut v = Vec2::splat(0.0);
    let mut s = 1.0;
    for i in 0..15 {
        let a = vec2(d[MAGFIELD_HDR + 2 * i], d[MAGFIELD_HDR + 2 * i + 1]);
        s = -s;
        let q = p - a;
        v = v - q / dot(q, q) * s;
    }
    normalize(v)
}

// ---------------------------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    // curliecue: `seed` default 1000 (deterministic in Java as well).
    VarDef {
        name: "curliecue",
        params: &[
            ParamDef::int("Points", 500.0, 1.0, 10000.0),
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::int("type", 0.0, 0.0, 1.0),
            ParamDef::int("sides", 4.0, 3.0, 20.0),
            ParamDef::range("scale", 0.02, 0.0, 1.0),
            ParamDef::int("symmetry", 1.0, 1.0, 4.0),
            ParamDef::range("eccentricity", 0.0, 0.0, 1.0),
            ParamDef::int("showlines", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: curliecue_apply,
        prepare: Some(curliecue_prepare),
        wgsl: CURLIECUE_WGSL,
    },
    VarDef {
        name: "curl_noise",
        params: &[ParamDef::new("freq", 2.0), ParamDef::new("strength", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [freq, strength]
            let dx = d[1] * d[0] * (d[0] * a.x).sin() * (d[0] * a.y).cos();
            let dy = -d[1] * d[0] * (d[0] * a.x).cos() * (d[0] * a.y).sin();
            v.x += amount * (a.x + dx);
            v.y += amount * (a.y + dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dx = D(1) * D(0) * sin(D(0) * a.x) * cos(D(0) * a.y);
  let dy = -D(1) * D(0) * cos(D(0) * a.x) * sin(D(0) * a.y);
  v.x += AMOUNT * (a.x + dx);
  v.y += AMOUNT * (a.y + dy);
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "curl_sp",
        params: &[
            ParamDef::new("pow", 1.0),
            ParamDef::new("c1", -0.01),
            ParamDef::new("c2", 0.03),
            ParamDef::new("sx", 0.0),
            ParamDef::new("sy", 0.0),
            ParamDef::new("dc", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            // d = [pow, c1, c2, sx, sy, dc, power, c2_x2, dc_adjust, power_inv]
            let power = d[6];
            let x = curl_sp_powq4c(a.x, power);
            let y = curl_sp_powq4c(a.y, power);
            let z = curl_sp_powq4c(a.z, power);
            let dd = sqr(x) - sqr(y);
            let re = curl_sp_spread(d[1] * x + d[2] * dd, d[3]) + 1.0;
            let im = curl_sp_spread(d[1] * y + d[7] * x * y, d[4]);
            let c = curl_sp_powq4c(sqr(re) + sqr(im), d[9]);
            let r = amount / c;
            v.x += (x * re + y * im) * r;
            v.y += (y * re - x * im) * r;
            v.z += (z * amount) / c;
            let col = v.color + d[8] * c;
            v.color = if col < 0.0 {
                0.0
            } else if col > 1.0 {
                1.0
            } else {
                col
            };
        },
        prepare: Some(|p, _| {
            let mut power = p[0];
            let power_inv = 1.0 / if power == 0.0 { SMALL_EPSILON } else { power };
            if power == 0.0 {
                power = SMALL_EPSILON;
            }
            let mut d = p.to_vec();
            d.extend_from_slice(&[power, 2.0 * p[2], 0.1 * p[5], power_inv]);
            d
        }),
        wgsl: "fn curl_sp_spread(a: f32, b: f32) -> f32 {
  return sqrt(a * a + b * b) * select(-1.0, 1.0, a > 0.0);
}
fn curl_sp_powq4c(x: f32, y: f32) -> f32 {
  if (y == 1.0) { return x; }
  return pow(abs(x), y) * sign(x);
}
@body
  let power = D(6);
  let x = curl_sp_powq4c(a.x, power);
  let y = curl_sp_powq4c(a.y, power);
  let z = curl_sp_powq4c(a.z, power);
  let dd = x * x - y * y;
  let re = curl_sp_spread(D(1) * x + D(2) * dd, D(3)) + 1.0;
  let im = curl_sp_spread(D(1) * y + D(7) * x * y, D(4));
  let c = curl_sp_powq4c(re * re + im * im, D(9));
  let r = AMOUNT / c;
  v.x += (x * re + y * im) * r;
  v.y += (y * re - x * im) * r;
  v.z += (z * AMOUNT) / c;
  let col = v.color + D(8) * c;
  v.color = select(select(col, 1.0, col > 1.0), 0.0, col < 0.0);",
    },
    // cut_2ewangtile: unused Java height/normal computations are omitted (no side effects).
    VarDef {
        name: "cut_2ewangtile",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 25.0, 3.0, 50.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, zoom, invert, x0, y0]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, 0.0);
            let c = if d[1] as i32 == 0 { 0.0 } else { 0.5 };
            let zoom = d[2];
            let uv = vec2(x * zoom, y * zoom) + vec2(d[4], d[5]);
            let vv = cut_2ewangtile_map(uv);
            let color = mix(1.0, 0.0, smoothstep(0.0, 2.0 / 2000.0, java_min(vv, (vv - 0.1).abs() - 0.1) / zoom));
            let color = color.sqrt();
            cut_output(ctx, a, v, amount, x, y, c, cut_hide(d[3], color, 0.30));
        },
        prepare: Some(|p, _| {
            let mut d = vec![p[0] as i32 as f64, limit_int(p[1], 0.0, 1.0), p[2], limit_int(p[3], 0.0, 1.0)];
            d.extend_from_slice(&cut_seed_offset(p[0]));
            d
        }),
        wgsl: concat!(
            "fn cut_2ewangtile_tile0(uv: vec2<f32>) -> f32 {
  let vv = length(uv) - (sqrt(2.0) * 0.5 - 0.45);
  let w = 0.45 - length(vec2<f32>(abs(uv.x) - 0.5, uv.y - 0.5));
  return mix(vv, w, step(abs(uv.x), uv.y));
}
fn cut_2ewangtile_tile1(uv: vec2<f32>) -> f32 { return abs(length(uv - 0.5) - 0.5) - 0.05; }
fn cut_2ewangtile_tile2(uv: vec2<f32>) -> f32 { return abs(uv.x) - 0.05; }
fn cut_2ewangtile_tile3(uv: vec2<f32>) -> f32 {
  return max(-uv.x - 0.05, 0.45 - length(vec2<f32>(uv.x - 0.5, abs(uv.y) - 0.5)));
}
fn cut_2ewangtile_tile4(uv: vec2<f32>) -> f32 {
  return 0.45 - length(vec2<f32>(abs(uv.x) - 0.5, abs(uv.y) - 0.5));
}
fn cut_2ewangtile_tile(uv: vec2<f32>, t: i32) -> f32 {
  switch t {
    case 1: { return max(cut_2ewangtile_tile0(uv), 0.15 - length(uv)); }
    case 2: { return cut_2ewangtile_tile0(uv.yx); }
    case 3: { return cut_2ewangtile_tile1(uv); }
    case 4: { return cut_2ewangtile_tile0(vec2<f32>(uv.x, -uv.y)); }
    case 5: { return cut_2ewangtile_tile2(uv); }
    case 6: { return cut_2ewangtile_tile1(vec2<f32>(uv.x, -uv.y)); }
    case 7: { return cut_2ewangtile_tile3(uv); }
    case 8: { return cut_2ewangtile_tile0(vec2<f32>(uv.y, -uv.x)); }
    case 9: { return cut_2ewangtile_tile1(vec2<f32>(-uv.x, uv.y)); }
    case 10: { return cut_2ewangtile_tile2(uv.yx); }
    case 11: { return cut_2ewangtile_tile3(uv.yx); }
    case 12: { return cut_2ewangtile_tile1(-uv); }
    case 13: { return cut_2ewangtile_tile3(vec2<f32>(-uv.x, uv.y)); }
    case 14: { return cut_2ewangtile_tile3(vec2<f32>(-uv.y, uv.x)); }
    case 15: { return cut_2ewangtile_tile4(uv); }
    default: { return 1.414; }
  }
}
fn cut_2ewangtile_hash(p: vec2<f32>) -> f32 {
  var p3 = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
  p3 = p3 + dot(p3, p3.yzx + 19.19);
  return fract((p3.x + p3.y) * p3.z);
}
fn cut_2ewangtile_map(uv_in: vec2<f32>) -> f32 {
  var b = 0;
  let uv = uv_in + 0.5;
  let id = floor(uv);
  if (cut_2ewangtile_hash(id) >= 0.5) { b += 1; }
  if (cut_2ewangtile_hash(-id) >= 0.5) { b += 8; }
  if (cut_2ewangtile_hash(id - vec2<f32>(0.0, 1.0)) >= 0.5) { b += 4; }
  if (cut_2ewangtile_hash(-(id + vec2<f32>(1.0, 0.0))) >= 0.5) { b += 2; }
  return cut_2ewangtile_tile(fract(uv) - 0.5, b);
}
@body
",
            cut_wgsl_head!(1, "1.0", "0.0", "0.5"),
            "  let zoom = D(2);
  let uv = vec2<f32>(x * zoom, y * zoom) + vec2<f32>(D(4), D(5));
  let vv = cut_2ewangtile_map(uv);
  let color = sqrt(mix(1.0, 0.0, smoothstep(0.0, 2.0 / 2000.0, min(vv, abs(vv - 0.1) - 0.1) / zoom)));
",
            cut_wgsl_hide!(3, "0.30"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_alientext",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::int("subdivisions", 3.0, 0.0, 4.0),
            ParamDef::range("zoom", 1.0, 0.1, 4.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, subdivisions, zoom, invert, x0, y0]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, 0.0);
            let c = if d[1] as i32 == 0 { 0.0 } else { 0.5 };
            let mut uv = vec2(x * d[3], y * d[3]) + vec2(d[5], d[6]);
            uv.y -= d[6];
            let mut dims = 2.0;
            for _ in 0..=(d[2] as i32) {
                let ij = floor(uv * dims) / dims;
                dims *= 2.0;
                if cut_alientext_random2d(ij + 454.4543) > 0.3 {
                    break;
                }
            }
            let color = cut_alientext_letter(uv, 1.0 / dims);
            cut_output(ctx, a, v, amount, x, y, c, cut_hide(d[4], color, 0.0));
        },
        prepare: Some(|p, _| {
            let mut d =
                vec![p[0] as i32 as f64, limit_int(p[1], 0.0, 1.0), limit_int(p[2], 0.0, 4.0), p[3], limit_int(p[4], 0.0, 1.0)];
            d.extend_from_slice(&cut_seed_offset(p[0]));
            d
        }),
        wgsl: concat!(
            "fn cut_alientext_r2d(n: vec2<f32>) -> f32 {
  return fract(sin(dot(n, vec2<f32>(129.9898, 4.1414))) * 2398.5453);
}
fn cut_alientext_letter(coord: vec2<f32>, size: f32) -> f32 {
  let gp = floor(coord / size * 7.0);
  let rp = floor(fract(coord / size) * 7.0);
  let odd = fract(rp * 0.5) * 2.0;
  var c = max(odd.x, odd.y) * step(0.5, cut_alientext_r2d(gp));
  c += min(odd.x, odd.y);
  c *= rp.x * (6.0 - rp.x);
  c *= rp.y * (6.0 - rp.y);
  return clamp(c, 0.0, 1.0);
}
@body
",
            cut_wgsl_head!(1, "1.0", "0.0", "0.5"),
            "  var uv = vec2<f32>(x * D(3), y * D(3)) + vec2<f32>(D(5), D(6));
  uv.y -= D(6);
  var dims = 2.0;
  for (var i = 0; i <= i32(D(2)); i++) {
    let ij = floor(uv * dims) / dims;
    dims *= 2.0;
    if (cut_alientext_r2d(ij + 454.4543) > 0.3) { break; }
  }
  let color = cut_alientext_letter(uv, 1.0 / dims);
",
            cut_wgsl_hide!(4, "0.0"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_apollonian",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::int("levels", 4.0, 1.0, 10.0),
            ParamDef::range("zoom", 2.0, 0.1, 50.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, levels, zoom, invert]
            let (x, y) = cut_input(ctx, a, d[0], 1.0, -0.5);
            let mut p = vec2(x * d[2], y * d[2]);
            let mut scale = 1.0;
            for _ in 0..(d[1] as i32) {
                p = fract(p * 0.5 + 0.5) * 2.0 - 1.0;
                let k = 1.34 / dot(p, p);
                p = p * k;
                scale *= k;
            }
            let col = smoothstep(0.001, 0.002, 0.25 * p.y.abs() / scale);
            cut_output(ctx, a, v, amount, x, y, 0.0, cut_hide(d[3], col, 0.0));
        },
        prepare: Some(|p, _| {
            vec![limit_int(p[0], 0.0, 1.0), limit_int(p[1], 1.0, 10.0), limit_value(p[2], 0.1, 50.0), limit_int(p[3], 0.0, 1.0)]
        }),
        wgsl: concat!(
            cut_wgsl_head!(0, "1.0", "-0.5", "0.0"),
            "  var p = vec2<f32>(x * D(2), y * D(2));
  var scale = 1.0;
  for (var i = 0; i < i32(D(1)); i++) {
    p = fract(p * 0.5 + 0.5) * 2.0 - 1.0;
    let k = 1.34 / dot(p, p);
    p = p * k;
    scale *= k;
  }
  let color = smoothstep(0.001, 0.002, 0.25 * abs(p.y) / scale);
",
            cut_wgsl_hide!(3, "0.0"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_booleans",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::int("type", 0.0, 0.0, 2.0),
            ParamDef::range("zoom", 250.0, 50.0, 500.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, type, zoom, invert, x0, y0]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, 0.0);
            let c = if d[1] as i32 == 0 { 0.0 } else { 0.5 };
            let u = vec2(x * d[3], y * d[3]) + vec2(d[5], d[6]);
            let (ix, iy) = (u.x as i32, u.y as i32);
            let temp = match d[2] as i32 {
                1 => ix & iy,
                2 => ix | iy,
                _ => ix ^ iy,
            };
            let color = (temp as f64).sin();
            cut_output(ctx, a, v, amount, x, y, c, cut_hide(d[4], color, 0.0));
        },
        prepare: Some(|p, _| {
            let mut d =
                vec![p[0] as i32 as f64, limit_int(p[1], 0.0, 1.0), limit_int(p[2], 0.0, 2.0), p[3], limit_int(p[4], 0.0, 1.0)];
            d.extend_from_slice(&cut_seed_offset(p[0]));
            d
        }),
        wgsl: concat!(
            cut_wgsl_head!(1, "1.0", "0.0", "0.5"),
            "  let u = vec2<f32>(x * D(3), y * D(3)) + vec2<f32>(D(5), D(6));
  let ix = i32(u.x);
  let iy = i32(u.y);
  let ty = i32(D(2));
  var t = ix ^ iy;
  if (ty == 1) { t = ix & iy; } else if (ty == 2) { t = ix | iy; }
  let color = sin(f32(t));
",
            cut_wgsl_hide!(4, "0.0"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_bricks",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("size", 0.9, 0.1, 0.9999),
            ParamDef::range("zoom", 3.0, 0.5, 20.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, size, zoom, invert, x0, y0]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, 0.0);
            let c = if d[1] as i32 == 0 { 0.0 } else { 0.5 };
            let mut u = vec2(x * d[3], y * d[3]) + vec2(d[5], d[6]);
            // brickTile
            u.x += step(1.0, glsl_mod(u.y, 2.0)) * 0.5;
            u = fract(u);
            // box
            let s = Vec2::splat(0.5) - Vec2::splat(d[2]) * 0.5;
            let mut uv = smoothstep(s, s + Vec2::splat(1e-4), u);
            uv = uv * smoothstep(s, s + Vec2::splat(1e-4), Vec2::splat(1.0) - u);
            let color = uv.x * uv.y;
            cut_output(ctx, a, v, amount, x, y, c, cut_hide(d[4], color, 0.0));
        },
        prepare: Some(|p, _| {
            let mut d = vec![
                p[0] as i32 as f64,
                limit_int(p[1], 0.0, 1.0),
                limit_value(p[2], 0.1, 0.9999),
                p[3],
                limit_int(p[4], 0.0, 1.0),
            ];
            d.extend_from_slice(&cut_seed_offset(p[0]));
            d
        }),
        wgsl: concat!(
            cut_wgsl_head!(1, "1.0", "0.0", "0.5"),
            "  var u = vec2<f32>(x * D(3), y * D(3)) + vec2<f32>(D(5), D(6));
  u.x += step(1.0, glsl_mod(u.y, 2.0)) * 0.5;
  u = fract(u);
  let s = vec2<f32>(0.5) - vec2<f32>(D(2)) * 0.5;
  var uv = smoothstep(s, s + vec2<f32>(1e-4), u);
  uv = uv * smoothstep(s, s + vec2<f32>(1e-4), vec2<f32>(1.0) - u);
  let color = uv.x * uv.y;
",
            cut_wgsl_hide!(4, "0.0"),
            cut_wgsl_tail!()
        ),
    },
    // cut_btree: Java resets `time` to the elapsed wall-clock seconds whenever `randomize` is
    // set; here `time` is an ordinary parameter.
    VarDef {
        name: "cut_btree",
        params: &[
            ParamDef::int("randomize", 0.0, 0.0, 1000000.0),
            ParamDef::range("time", 1.0, 0.0, 100.0),
            ParamDef::range("thickness", 5.0, 1.0, 10.0),
            ParamDef::range("zoom", 1.0, 0.1, 5.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::int("invert", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [randomize, time, thickness, zoom, mode, invert]
            let (x, y) = cut_input(ctx, a, d[4], 2.0, -1.0);
            let mut w = vec2(x * d[3], y * d[3]);
            let mut t = w / w * fract(d[1]);
            let l = w * w * 5.0 - t;
            t = t + 1.0;
            let m = exp2(ceil(l)) * t;
            w = fract(m * w.x) - 0.5;
            let t0 = sign(w) / 4.0;
            let t1 = abs(smoothstep(0.0, 1.0, fract(l)) * t0 - w);
            let t2 = m / t1 / (100.0 * d[2]);
            let color = t2.y;
            cut_output(ctx, a, v, amount, x, y, 0.0, cut_hide(d[5], color, 0.5));
        },
        prepare: Some(|p, _| {
            vec![p[0] as i32 as f64, p[1], limit_value(p[2], 1.0, 10.0), p[3], limit_int(p[4], 0.0, 1.0), limit_int(p[5], 0.0, 1.0)]
        }),
        wgsl: concat!(
            cut_wgsl_head!(4, "2.0", "-1.0", "0.0"),
            "  var w = vec2<f32>(x * D(3), y * D(3));
  var t = w / w * fract(D(1));
  let l = w * w * 5.0 - t;
  t = t + 1.0;
  let m = exp2(ceil(l)) * t;
  w = fract(m * w.x) - 0.5;
  let t0 = sign(w) / 4.0;
  let t1 = abs(smoothstep(vec2<f32>(0.0), vec2<f32>(1.0), fract(l)) * t0 - w);
  let t2 = m / t1 / (100.0 * D(2));
  let color = t2.y;
",
            cut_wgsl_hide!(5, "0.5"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_btruchet",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("width", 2.0, 0.5, 5.0),
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::range("zoom", 10.0, 1.0, 50.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, width, seed, zoom, invert, x0, y0]
            let (x, y) = cut_input(ctx, a, d[0], 1.0, -0.5);
            let zoom = d[3];
            let mut uv = vec2(x * zoom, y * zoom) + vec2(d[5], d[6]);
            let e = zoom * d[1] / 1000.0;
            let tile = floor(uv);
            let id = tile.x * 10000.0 + tile.y;
            uv = (uv - floor(uv) - Vec2::splat(0.5)) * 2.0;
            let t = cut_btruchet_hash1(id as i32);
            let dd = if t < 0.33 {
                java_min(uv.x.abs(), uv.y.abs())
            } else if t < 0.67 {
                cut_btruchet_tile2(uv)
            } else {
                cut_btruchet_tile2(vec2(-uv.x, uv.y))
            };
            let color = smoothstep(e * 5.0, e, dd);
            let hide = if d[4] as i32 == 0 { color == 0.0 } else { color > 0.0 };
            cut_output(ctx, a, v, amount, x, y, 0.0, hide);
        },
        prepare: Some(|p, _| {
            let mut d = vec![
                limit_int(p[0], 0.0, 1.0),
                limit_value(p[1], 0.5, 5.0),
                p[2] as i32 as f64,
                p[3],
                limit_int(p[4], 0.0, 1.0),
            ];
            d.extend_from_slice(&cut_seed_offset(p[2]));
            d
        }),
        wgsl: concat!(
            "fn cut_btruchet_hash1(n_in: i32) -> f32 {
  var n = (n_in << 13u) ^ n_in;
  n = n * (n * n * 15731 + 789221) + 1376312589;
  return f32(n) / 2147483647.0;
}
fn cut_btruchet_tile2(uv_in: vec2<f32>) -> f32 {
  var uv = uv_in;
  if (uv.y < -uv.x) { uv = -uv; }
  return abs(distance(uv, vec2<f32>(1.0, 1.0)) - 1.0);
}
@body
",
            cut_wgsl_head!(0, "1.0", "-0.5", "0.0"),
            "  let zoom = D(3);
  var uv = vec2<f32>(x * zoom, y * zoom) + vec2<f32>(D(5), D(6));
  let e = zoom * D(1) / 1000.0;
  let tile = floor(uv);
  let id = i32(tile.x) * 10000 + i32(tile.y);
  uv = (uv - tile - vec2<f32>(0.5)) * 2.0;
  let t = cut_btruchet_hash1(id);
  var dd = 0.0;
  if (t < 0.33) {
    dd = min(abs(uv.x), abs(uv.y));
  } else if (t < 0.67) {
    dd = cut_btruchet_tile2(uv);
  } else {
    dd = cut_btruchet_tile2(vec2<f32>(-uv.x, uv.y));
  }
  let color = glsl_smoothstep(e * 5.0, e, dd);
  let hide = select(color == 0.0, color > 0.0, i32(D(4)) != 0);
",
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_celtic",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 5.0, 0.5, 20.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, zoom, invert]
            let (x, y) = cut_input(ctx, a, d[0], 1.0, 0.0);
            let c = if d[0] as i32 == 0 { 0.0 } else { 0.5 };
            let uv1 = cut_celtic_cell(x, y, d[1], 0.0, 0.0);
            let uv2 = cut_celtic_cell(x, y, d[1], -0.5, -0.288675);
            let uv3 = cut_celtic_cell(x, y, d[1], 1.0, -0.65);
            let color = cut_celtic_shape(uv1) + cut_celtic_shape(uv2) + cut_celtic_shape(uv3);
            cut_output(ctx, a, v, amount, x, y, c, cut_hide(d[2], color, 0.0));
        },
        prepare: Some(|p, _| vec![limit_int(p[0], 0.0, 1.0), p[1], limit_int(p[2], 0.0, 1.0)]),
        wgsl: concat!(
            "fn cut_celtic_circ(uv: vec2<f32>, r: f32) -> f32 {
  let d = length(uv);
  return smoothstep(d, d + 0.02, r);
}
fn cut_celtic_shape(uv: vec2<f32>) -> f32 {
  let c1 = cut_celtic_circ(uv, 0.38);
  let c5 = cut_celtic_circ(uv, 0.45);
  let c2 = cut_celtic_circ(uv + vec2<f32>(0.5, -0.288675), 0.45);
  let c3 = cut_celtic_circ(uv + vec2<f32>(-0.5, -0.288675), 0.45);
  let c4 = cut_celtic_circ(uv + vec2<f32>(0.0, 0.57735), 0.45);
  return mix(0.0, c5 - c2 - c3 - c4, c1);
}
fn cut_celtic_cell(x: f32, y: f32, zoom: f32, ox: f32, oy: f32) -> vec2<f32> {
  var uv = vec2<f32>(x, y * 1.155) * zoom + vec2<f32>(ox, oy);
  uv.x += step(1.0, glsl_mod(uv.y, 2.0)) * 0.5;
  uv = fract(uv);
  uv.y /= 1.155;
  uv.x -= 1.155 / 2.0;
  uv.y -= 0.5;
  return uv;
}
@body
",
            cut_wgsl_head!(0, "1.0", "0.0", "0.5"),
            "  let color = cut_celtic_shape(cut_celtic_cell(x, y, D(1), 0.0, 0.0))
    + cut_celtic_shape(cut_celtic_cell(x, y, D(1), -0.5, -0.288675))
    + cut_celtic_shape(cut_celtic_cell(x, y, D(1), 1.0, -0.65));
",
            cut_wgsl_hide!(2, "0.0"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_chains",
        params: &[
            ParamDef::new("shiftX", 0.0),
            ParamDef::new("shiftY", 0.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 2.5, 0.5, 20.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [shiftX, shiftY, mode, zoom, invert]
            let (x, y) = cut_input(ctx, a, d[2], 1.0, 0.0);
            let c = if d[2] as i32 == 0 { 0.0 } else { 0.5 };
            let u = vec2(x * d[3], y * d[3]) + vec2(d[0], d[1]);
            let wy = (u.y * 12.0).cos();
            let wx = (u.x * 15.0).sin();
            let w = (u.x * 30.0).cos();
            let color = smoothstep(-0.25, 0.25, mix(wy + wx, wx * 1.4, w * 3.0));
            cut_output(ctx, a, v, amount, x, y, c, cut_hide(d[4], color, 0.0));
        },
        prepare: Some(|p, _| vec![p[0], p[1], limit_int(p[2], 0.0, 1.0), p[3], limit_int(p[4], 0.0, 1.0)]),
        wgsl: concat!(
            cut_wgsl_head!(2, "1.0", "0.0", "0.5"),
            "  let u = vec2<f32>(x * D(3), y * D(3)) + vec2<f32>(D(0), D(1));
  let wy = cos(u.y * 12.0);
  let wx = sin(u.x * 15.0);
  let w = cos(u.x * 30.0);
  let color = smoothstep(-0.25, 0.25, mix(wy + wx, wx * 1.4, w * 3.0));
",
            cut_wgsl_hide!(4, "0.0"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_circdes",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("zoom", 10.0, 1.0, 50.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, time, zoom, invert]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, 0.0);
            let c = if d[1] as i32 == 0 { 0.0 } else { 0.5 };
            let u = fract(vec2(x * d[3], y * d[3])) - 0.5;
            let mut r = 0.8;
            let mut dd = step(length(u), r);
            for i in 0..9 {
                r *= 0.5 + 0.1 * (i as f64 + d[2]).sin();
                let s = step(length(u + vec2(0.0, r)), r)
                    + step(length(u + vec2(r, 0.0)), r)
                    + step(length(u - vec2(0.0, r)), r)
                    + step(length(u - vec2(r, 0.0)), r);
                if i % 2 == 0 {
                    dd -= s;
                } else {
                    dd += s;
                }
            }
            cut_output(ctx, a, v, amount, x, y, c, cut_hide(d[4], dd, 0.5));
        },
        prepare: Some(|p, _| vec![p[0] as i32 as f64, limit_int(p[1], 0.0, 1.0), p[2], p[3], limit_int(p[4], 0.0, 1.0)]),
        wgsl: concat!(
            cut_wgsl_head!(1, "1.0", "0.0", "0.5"),
            "  let u = fract(vec2<f32>(x * D(3), y * D(3))) - 0.5;
  var r = 0.8;
  var color = step(length(u), r);
  for (var i = 0; i < 9; i++) {
    r *= 0.5 + 0.1 * sin(f32(i) + D(2));
    let s = step(length(u + vec2<f32>(0.0, r)), r) + step(length(u + vec2<f32>(r, 0.0)), r)
      + step(length(u - vec2<f32>(0.0, r)), r) + step(length(u - vec2<f32>(r, 0.0)), r);
    if (i % 2 == 0) { color -= s; } else { color += s; }
  }
",
            cut_wgsl_hide!(4, "0.5"),
            cut_wgsl_tail!()
        ),
    },
    // cut_fingerprint: the 50 hash points only depend on the seed and are precalculated.
    VarDef {
        name: "cut_fingerprint",
        params: &[
            ParamDef::int("seed", 10000.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 20.0, 1.0, 100.0),
            ParamDef::range("width", 0.8, 0.02, 1.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, zoom, width, invert, off_x, off_y, (s, hx, hy) * 50]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, -0.5);
            let mut uv = vec2(x, y) * d[2];
            let bounds = smoothstep(9.0, 10.0, length(uv * vec2(0.7, 0.5)));
            let mut ang = 0.0;
            for i in 0..FINGERPRINT_N {
                let o = FINGERPRINT_HDR + 3 * i;
                ang += d[o] * glsl_atan2(uv.x - d[o + 1], uv.y - d[o + 2]);
            }
            uv = uv + vec2(d[5], d[6]);
            ang += glsl_atan2(uv.y, uv.x);
            let p = (1.0 - bounds) * d[3];
            let s = min(0.3, p);
            let l = length(uv) + 0.319 * ang;
            let m = glsl_mod(l, 2.0);
            let color = (1.0 - smoothstep(2.0 - s, 2.0, m)) * smoothstep(p, p + s, m);
            let hide = if d[4] as i32 == 0 { color == 0.0 } else { color > 0.0 };
            v.do_hide = hide;
            if hide {
                v.x = 0.0;
                v.y = 0.0;
                return;
            }
            v.x = amount * x;
            v.y = amount * y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let seed = p[0] as i32 as f64;
            let mut d = vec![
                seed,
                limit_int(p[1], 0.0, 1.0),
                limit_value(p[2], 1.0, 100.0),
                limit_value(p[3], 0.02, 1.0).abs(),
                limit_int(p[4], 0.0, 1.0),
                0.0,
                0.0,
            ];
            let mut h = vec2((7.0 * seed).floor(), 0.0);
            for _ in 0..FINGERPRINT_N {
                let s = sign(h.x);
                h = cut_fingerprint_hash2(h) * vec2(15.0, 20.0);
                d.extend_from_slice(&[s, h.x, h.y]);
            }
            let off = abs(cut_fingerprint_hash2(h));
            d[5] = off.x;
            d[6] = off.y;
            d
        }),
        wgsl: concat!(
            cut_wgsl_head!(1, "1.0", "-0.5", "0.0"),
            "  var uv = vec2<f32>(x, y) * D(2);
  let bounds = smoothstep(9.0, 10.0, length(uv * vec2<f32>(0.7, 0.5)));
  var ang = 0.0;
  for (var i = 0u; i < 50u; i++) {
    let o = base + 7u + 3u * i;
    ang += prm[o] * glsl_atan2(uv.x - prm[o + 1u], uv.y - prm[o + 2u]);
  }
  uv = uv + vec2<f32>(D(5), D(6));
  ang += glsl_atan2(uv.y, uv.x);
  let p = (1.0 - bounds) * D(3);
  let s = min(0.3, p);
  let m = glsl_mod(length(uv) + 0.319 * ang, 2.0);
  let color = (1.0 - glsl_smoothstep(2.0 - s, 2.0, m)) * glsl_smoothstep(p, p + s, m);
  let hide = select(color == 0.0, color > 0.0, i32(D(4)) != 0);
  v.hide = select(0.0, 1.0, hide);
  if (hide) {
    v.x = 0.0;
    v.y = 0.0;
  } else {
    v.x = AMOUNT * x;
    v.y = AMOUNT * y;
    if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
  }
"
        ),
    },
    VarDef {
        name: "cut_fractal",
        params: &[
            ParamDef::int("seed", 0.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("iters", 30.0, 1.0, 100.0),
            ParamDef::range("zoom", 1.0, 0.1, 10.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, time, iters, zoom, invert, cx, cy]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, -0.5);
            let c = vec2(d[6], d[7]);
            let mut u = vec2(y * d[4], x * d[4]);
            for _ in 0..(d[3] as i32) {
                u = abs(u) * (0.5 / dot(u, u)) + c;
            }
            let color = (dot(u, u) - 0.01).abs() * 2.0;
            cut_output(ctx, a, v, amount, x, y, 0.0, cut_hide(d[5], color, 0.5));
        },
        prepare: Some(|p, _| {
            let c = vec2((p[2] + 1.5).cos(), (p[2] + 1.8).sin()) * 0.15 - 0.25;
            vec![p[0] as i32 as f64, limit_int(p[1], 0.0, 1.0), p[2], limit_int(p[3], 1.0, 100.0), p[4], limit_int(p[5], 0.0, 1.0), c.x, c.y]
        }),
        wgsl: concat!(
            cut_wgsl_head!(1, "1.0", "-0.5", "0.0"),
            "  let c = vec2<f32>(D(6), D(7));
  var u = vec2<f32>(y * D(4), x * D(4));
  for (var i = 0; i < i32(D(3)); i++) {
    u = abs(u) * (0.5 / dot(u, u)) + c;
  }
  let color = abs(dot(u, u) - 0.01) * 2.0;
",
            cut_wgsl_hide!(5, "0.5"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_fun",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 10.0, 1.0, 50.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, zoom, invert, x0, y0]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, 0.0);
            let c = if d[1] as i32 == 0 { 0.0 } else { 0.5 };
            let uv = vec2(x * d[2], y * d[2]);
            let color = cut_fun_hm(vec2(uv.x + d[4], uv.y + d[5]));
            cut_output(ctx, a, v, amount, x, y, c, cut_hide(d[3], color, 0.0));
        },
        prepare: Some(|p, _| {
            let mut d = vec![p[0] as i32 as f64, limit_int(p[1], 0.0, 1.0), p[2], limit_int(p[3], 0.0, 1.0)];
            d.extend_from_slice(&cut_seed_offset(p[0]));
            d
        }),
        wgsl: concat!(
            "fn cut_fun_pix(u: vec2<f32>) -> f32 {
  return step(0.5, fract(sin(dot(floor(u), vec2<f32>(12.9898, 78.233))) * 43758.5453));
}
fn cut_fun_hm(uv: vec2<f32>) -> f32 {
  let nuv = fract(uv) * 2.0 - 1.0;
  let pos = sign(nuv) * 0.9;
  let d = length(nuv);
  var vv = cut_fun_pix(uv);
  if (d < 1.0) { return vv; }
  let v0 = vv;
  let v1 = cut_fun_pix(vec2<f32>(pos.x, 0.0) + uv);
  let v2 = cut_fun_pix(vec2<f32>(0.0, pos.y) + uv);
  if (v1 == v2) { vv = select(v1, 0.0, vv == cut_fun_pix(uv + pos) && vv != v1); }
  return mix(v0, vv, glsl_smoothstep(d, 1.0, 1.01));
}
@body
",
            cut_wgsl_head!(1, "1.0", "0.0", "0.5"),
            "  let color = cut_fun_hm(vec2<f32>(x * D(2) + D(4), y * D(2) + D(5)));
",
            cut_wgsl_hide!(3, "0.0"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_glypho",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 1.0, 0.1, 50.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
            ParamDef::new("f1", 0.115),
            ParamDef::new("f2", 0.75),
            ParamDef::new("f3", 1.5),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, zoom, invert, f1, f2, f3]
            let (x, y) = cut_input(ctx, a, d[0], 1.0, -0.5);
            let uv = vec2(x, y) * d[1];
            let mut m = 0.0;
            let stp = M_PI / 5.0;
            let mut n = 1.0;
            while n < 2.5 {
                let mut i = 0.0001;
                while i < 2.0 * M_PI {
                    let uvi = uv * n + vec2((i + n * stp * 0.5).cos() * 0.4, (i + n * stp * 0.5).sin() * 0.4);
                    m += smoothstep(d[3] * n, 0.0, length(uvi)) * d[5];
                    i += stp;
                }
                n += 0.5;
            }
            let color = step(d[4], fract(m * d[5]));
            let hide = if d[2] as i32 == 0 { color == 0.0 } else { color > 0.0 };
            v.do_hide = hide;
            if hide {
                v.x = 0.0;
                v.y = 0.0;
                return;
            }
            v.x = amount * x;
            v.y = amount * y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![limit_int(p[0], 0.0, 1.0), limit_value(p[1], 0.1, 50.0), limit_int(p[2], 0.0, 1.0), p[3], p[4], p[5]]),
        wgsl: concat!(
            cut_wgsl_head!(0, "1.0", "-0.5", "0.0"),
            "  let uv = vec2<f32>(x, y) * D(1);
  var m = 0.0;
  let stp = PI / 5.0;
  for (var n = 1.0; n < 2.5; n += 0.5) {
    for (var i = 0.0001; i < 2.0 * PI; i += stp) {
      let uvi = uv * n + vec2<f32>(cos(i + n * stp * 0.5) * 0.4, sin(i + n * stp * 0.5) * 0.4);
      m += glsl_smoothstep(D(3) * n, 0.0, length(uvi)) * D(5);
    }
  }
  let color = step(D(4), fract(m * D(5)));
  let hide = select(color == 0.0, color > 0.0, i32(D(2)) != 0);
  v.hide = select(0.0, 1.0, hide);
  if (hide) {
    v.x = 0.0;
    v.y = 0.0;
  } else {
    v.x = AMOUNT * x;
    v.y = AMOUNT * y;
    if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
  }
"
        ),
    },
    VarDef {
        name: "cut_hexdots",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("size", 0.5, 0.01, 1.0),
            ParamDef::range("zoom", 4.0, 0.1, 50.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, size, zoom, invert]
            let (x, y) = cut_input(ctx, a, d[0], 1.0, -0.5);
            let u = vec2(x * d[2], y * d[2]);
            let s = vec2(1.0, 1.732);
            let ua = glsl_mod(u, s) * 2.0 - s;
            let ub = glsl_mod(u + s * 0.5, s) * 2.0 - s;
            let col = 0.8 * min(dot(ua, ua), dot(ub, ub));
            cut_output(ctx, a, v, amount, x, y, 0.0, cut_hide(d[3], col, d[1]));
        },
        prepare: Some(|p, _| vec![limit_int(p[0], 0.0, 1.0), limit_value(p[1], 0.01, 1.0), limit_value(p[2], 0.1, 50.0), limit_int(p[3], 0.0, 1.0)]),
        wgsl: concat!(
            cut_wgsl_head!(0, "1.0", "-0.5", "0.0"),
            "  let u = vec2<f32>(x * D(2), y * D(2));
  let s = vec2<f32>(1.0, 1.732);
  let ua = glsl_mod2(u, s) * 2.0 - s;
  let ub = glsl_mod2(u + s * 0.5, s) * 2.0 - s;
  let color = 0.8 * min(dot(ua, ua), dot(ub, ub));
  let hide = select(color > D(1), color <= D(1), i32(D(3)) != 0);
",
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_hextruchetflow",
        params: &[
            ParamDef::int("randomize", 0.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::int("grid", 0.0, 0.0, 1.0),
            ParamDef::range("zoom", 10.0, 0.1, 50.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [randomize, mode, grid, zoom, invert, uv_offset]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, -0.5);
            // `uv.plus(G.step(1.5, 1.0))` adds 0
            let uv = vec2(x * d[3] - d[5], y * d[3] - d[5]);
            let col = cut_hextruchetflow_scene(uv, d[2] as i32);
            cut_output(ctx, a, v, amount, x, y, 0.0, cut_hide(d[4], col, 0.9));
        },
        prepare: Some(|p, _| {
            let seed = p[0] as i32 as f64;
            let uvx = seed * (seed * 180.0 / M_PI).sin();
            vec![seed, limit_int(p[1], 0.0, 1.0), limit_int(p[2], 0.0, 1.0), limit_value(p[3], 0.1, 50.0), limit_int(p[4], 0.0, 1.0), uvx]
        }),
        wgsl: concat!(
            "fn cut_hextruchetflow_pix_to_hex(p: vec2<f32>) -> vec2<f32> {
  let sqrt3 = 1.73205;
  let t0 = vec2<f32>((1.0 / sqrt3) * p.x - (1.0 / 3.0) * p.y, (2.0 / 3.0) * p.y);
  let c = vec3<f32>(t0.x, -t0.x - t0.y, t0.y);
  var r = floor(c + 0.5);
  let dr = abs(r - c);
  r = r - step(dr.yzx, dr) * step(dr.zxy, dr) * dot(r, vec3<f32>(1.0));
  return vec2<f32>(r.x, r.z);
}
fn cut_hextruchetflow_scene(p: vec2<f32>, grid: i32) -> f32 {
  let sqrt3 = 1.73205;
  var col = 0.0;
  let cid = cut_hextruchetflow_pix_to_hex(p);
  let pc = vec2<f32>(sqrt3 * (cid.x + 0.5 * cid.y), 1.5 * cid.y);
  let dir = 2.0 * step(fract(sin(dot(cid, vec2<f32>(37.0, 39.0))) * 43758.54), 0.5) - 1.0;
  let t0 = pc + vec2<f32>(0.0, -dir);
  var w = vec3<f32>(t0, dot(t0 - p, t0 - p));
  var q = pc + vec2<f32>(sqrt3 / 2.0, 0.5 * dir);
  var d = dot(q - p, q - p);
  if (d < w.z) { w = vec3<f32>(q, d); }
  q = pc + vec2<f32>(-sqrt3 / 2.0, 0.5 * dir);
  d = dot(q - p, q - p);
  if (d < w.z) { w = vec3<f32>(q, d); }
  w.z = abs(sqrt(w.z) - 0.5);
  let pe = abs(p - pc);
  d = sqrt3 / 2.0 - pe.x + 0.5 * min(pe.x - sqrt3 * pe.y, 0.0);
  if (grid == 1) { col = mix(1.0, glsl_smoothstep(1.0, 1.0, d), smoothstep(0.01, 0.02, d)); }
  if (w.z < 0.25) { col = 1.0; }
  return col;
}
@body
",
            cut_wgsl_head!(1, "1.0", "-0.5", "0.0"),
            "  let uv = vec2<f32>(x * D(3) - D(5), y * D(3) - D(5));
  let color = cut_hextruchetflow_scene(uv, i32(D(2)));
",
            cut_wgsl_hide!(4, "0.9"),
            cut_wgsl_tail!()
        ),
    },
    // cut_jigsaw: Java computes a seed offset in init() but never uses it.
    VarDef {
        name: "cut_jigsaw",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 4.0, 0.5, 20.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, time, mode, zoom, invert]
            let (x, y) = cut_input(ctx, a, d[2], 1.0, -0.5);
            let mut u = vec2(x * d[3], y * d[3]) + vec2(1.0, 0.0) * d[1];
            let i = ceil(u);
            let m = glsl_mod(i, 2.0) - 0.5;
            u = u - (i - 0.5);
            let dd = if dot(m, u) < 0.0 { m * -1.0 } else { m };
            let f = dot(abs(u - dd), Vec2::splat(1.0)) * 0.7 - 0.65;
            let c = length(u - dd * 0.2) - 0.2;
            let h = dot(i, vec2(27.0, 57.0)).sin() * 1e5;
            let q = (fract(h) - 0.5) * dd.x < 0.0;
            let t = if q { java_max(f, 0.1 - c) } else { f };
            let s = java_min(t, c);
            let color = 1.0 - s * 1e4 / 8.0;
            cut_output(ctx, a, v, amount, x, y, 0.0, cut_hide(d[4], color, 0.0));
        },
        prepare: Some(|p, _| vec![p[0] as i32 as f64, p[1], limit_int(p[2], 0.0, 1.0), p[3], limit_int(p[4], 0.0, 1.0)]),
        wgsl: concat!(
            cut_wgsl_head!(2, "1.0", "-0.5", "0.0"),
            "  var u = vec2<f32>(x * D(3), y * D(3)) + vec2<f32>(1.0, 0.0) * D(1);
  let ii = ceil(u);
  let m = glsl_mod2(ii, vec2<f32>(2.0)) - 0.5;
  u = u - (ii - 0.5);
  let dd = select(m, -m, dot(m, u) < 0.0);
  let f = dot(abs(u - dd), vec2<f32>(1.0)) * 0.7 - 0.65;
  let c = length(u - dd * 0.2) - 0.2;
  let h = sin(dot(ii, vec2<f32>(27.0, 57.0))) * 1e5;
  let q = (fract(h) - 0.5) * dd.x < 0.0;
  let t = select(f, max(f, 0.1 - c), q);
  let s = min(t, c);
  let color = 1.0 - s * 1e4 / 8.0;
",
            cut_wgsl_hide!(4, "0.0"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_kaleido",
        params: &[
            ParamDef::int("seed", 1000.0, 0.0, 1000000.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("n", 6.0, 2.0, 20.0),
            ParamDef::range("zoom", 0.5, 0.1, 5.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, time, n, zoom, invert]
            let (x, y) = cut_input(ctx, a, d[1], 1.0, -0.5);
            let u = vec2(x * d[4], y * d[4]);
            let time = d[2];
            let n = d[3] as i32;
            let mut dd = 1.0;
            let k = 100.0 + 10.0 * (time / 5.0).sin();
            for i in 0..n {
                let fi = 2.0 * 3.14 * (i as f64 + 0.02 * time) / n as f64;
                let c = vec2(0.5 * fi.sin(), 0.5 * fi.cos());
                dd += (k * distance(u, c)).sin();
            }
            let color = 0.0 - (dd / n as f64 * 13.0).cos();
            cut_output(ctx, a, v, amount, x, y, 0.0, cut_hide(d[5], color, 0.0));
        },
        prepare: Some(|p, _| vec![p[0] as i32 as f64, limit_int(p[1], 0.0, 1.0), p[2], limit_int(p[3], 2.0, 20.0), p[4], limit_int(p[5], 0.0, 1.0)]),
        wgsl: concat!(
            cut_wgsl_head!(1, "1.0", "-0.5", "0.0"),
            "  let u = vec2<f32>(x * D(4), y * D(4));
  let time = D(2);
  let n = i32(D(3));
  var dd = 1.0;
  let k = 100.0 + 10.0 * sin(time / 5.0);
  for (var i = 0; i < n; i++) {
    let fi = 2.0 * 3.14 * (f32(i) + 0.02 * time) / f32(n);
    dd += sin(k * distance(u, vec2<f32>(0.5 * sin(fi), 0.5 * cos(fi))));
  }
  let color = 0.0 - cos(dd / f32(n) * 13.0);
",
            cut_wgsl_hide!(5, "0.0"),
            cut_wgsl_tail!()
        ),
    },
    VarDef {
        name: "cut_kleinian",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 2.0, 0.1, 10.0),
            ParamDef::new("boxSize", 1.0),
            ParamDef::range("time", 0.0, 0.0, 1.0),
            ParamDef::int("NIters", 150.0, 0.0, 150.0),
            ParamDef::new("Dx", 0.0),
            ParamDef::new("Dy", -0.955),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, zoom, boxSize, time, NIters, Dx, Dy, invert, klein_r, klein_i]
            let (x, y) = cut_input(ctx, a, d[0], 1.0, -0.5);
            let uv = vec2(x * d[1], y * d[1]) - vec2(d[5], d[6]);
            let hit = cut_kleinian_hit(uv, d[8], d[9], d[2], d[4] as i32);
            let hide = if d[7] as i32 == 0 { hit == 0 } else { hit == 1 };
            cut_output(ctx, a, v, amount, x, y, 0.0, hide);
        },
        prepare: Some(|p, _| {
            let time = limit_value(p[3], 0.0, 1.0);
            let klein_r = 1.8462756 + (1.958591 - 1.8462756) * 0.5 + 0.5 * (1.958591 - 1.8462756) * time;
            let klein_i = 0.09627581 + (0.0112786 - 0.09627581) * 0.5 + 0.5 * (0.0112786 - 0.09627581) * time;
            vec![limit_int(p[0], 0.0, 1.0), p[1], p[2], time, limit_int(p[4], 0.0, 150.0), p[5], p[6], limit_int(p[7], 0.0, 1.0), klein_r, klein_i]
        }),
        wgsl: concat!(
            "fn cut_kleinian_wrap(x: f32, a: f32, s: f32) -> f32 {
  let y = x - s;
  return (y - a * floor(y / a)) + s;
}
fn cut_kleinian_hit(z_in: vec2<f32>, a: f32, b: f32, box_size: f32, iters: i32) -> i32 {
  var z = z_in;
  var lz = z + vec2<f32>(1.0);
  var llz = z + vec2<f32>(-1.0);
  let f = sign(b);
  for (var i = 0; i < iters; i++) {
    z.x = z.x + f * b / a * z.y;
    z.x = cut_kleinian_wrap(z.x, 2.0 * box_size, -box_size);
    z.x = z.x - f * b / a * z.y;
    // Java aliasing: lz is the same object as z here
    if (i > 0) { lz = z; }
    let zb = z.x + b * 0.5;
    if (z.y >= a * 0.5 + f * (2.0 * a - 1.95) / 4.0 * sign(zb) * (1.0 - exp(-(7.2 - (1.95 - a) * 15.0) * abs(zb)))) {
      z = vec2<f32>(-b, a) - z;
    }
    let ir = 1.0 / dot(z, z);
    z = z * (-ir);
    z.x = -b - z.x;
    z.y = a + z.y;
    let dz = z - llz;
    if (dot(dz, dz) < 1.0e-6) { break; }
    if (z.y < 0.0 || z.y > a) { return 1; }
    llz = lz;
    lz = z;
  }
  return 0;
}
@body
",
            cut_wgsl_head!(0, "1.0", "-0.5", "0.0"),
            "  let uv = vec2<f32>(x * D(1), y * D(1)) - vec2<f32>(D(5), D(6));
  let hit = cut_kleinian_hit(uv, D(8), D(9), D(2), i32(D(4)));
  let hide = select(hit == 0, hit == 1, i32(D(7)) != 0);
",
            cut_wgsl_tail!()
        ),
    },
    // cut_magfield: the 15 field sources only depend on `time` and are precalculated.
    VarDef {
        name: "cut_magfield",
        params: &[
            ParamDef::int("randomize", 0.0, 0.0, 1000000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("density", 3.0, 1.0, 10.0),
            ParamDef::range("zoom", 3.0, 0.1, 10.0),
            ParamDef::int("mode", 1.0, 0.0, 1.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [randomize, time, density, zoom, mode, invert, spacing, (ax, ay) * 15]
            let (x, y) = cut_input(ctx, a, d[4], 2.0, -1.0);
            let p = vec2(x * d[3], y * d[3]);
            let spacing = d[6];
            let vel = cut_magfield_velocity(d, p);
            let ang = glsl_atan2(vel.x, vel.y) / M_PI / 2.0;
            let mut lines = fract(ang / spacing);
            lines = java_min(lines, 1.0 - lines) * 2.0;
            let dd = 2.0 / 600.0;
            let der = (length(vel - cut_magfield_velocity(d, p + vec2(0.0, dd)))
                + length(vel - cut_magfield_velocity(d, p + vec2(dd, 0.0)))
                + length(vel - cut_magfield_velocity(d, p + vec2(dd, dd)))
                + length(vel - cut_magfield_velocity(d, p + vec2(dd, -dd))))
                / 4.0;
            lines /= der / spacing;
            lines -= 1000.0 * 0.0005;
            lines = clamp(lines, 0.0, 1.0);
            let disc = clamp((length(p) - 1.0) / 0.001, 0.0, 1.0);
            lines = mix(1.0 - lines, lines, disc);
            let color = lines.powf(1.0 / 2.2);
            let hide = if d[5] as i32 == 0 { color == 0.0 } else { color > 0.0 };
            cut_output(ctx, a, v, amount, x, y, 0.0, hide);
        },
        prepare: Some(|p, _| {
            let time = p[1];
            let density = limit_value(p[2], 1.0, 10.0);
            let rand = |n: f64| fract(n.sin() * 43758.5453123);
            let mut d = vec![p[0] as i32 as f64, time, density, p[3], limit_int(p[4], 0.0, 1.0), limit_int(p[5], 0.0, 1.0), 1.0 / (10.0 * density)];
            let limit = 15.0;
            for i in 0..15 {
                let i = i as f64;
                let r = rand(i / limit) - 0.5;
                let mut m = rand(i + 1.0) - 0.5;
                m *= (time + (23.78 * 1000.0)) * 2.0;
                let o = i + r + m;
                d.push((o / limit * M_PI * 2.0).sin());
                d.push((o / limit * M_PI * 2.0).cos());
            }
            d
        }),
        wgsl: concat!(
            "fn cut_magfield_velocity(p: vec2<f32>, base: u32) -> vec2<f32> {
  var vel = vec2<f32>(0.0);
  var s = 1.0;
  for (var i = 0u; i < 15u; i++) {
    let src = vec2<f32>(prm[base + 7u + 2u * i], prm[base + 8u + 2u * i]);
    s = -s;
    let q = p - src;
    vel = vel - q / dot(q, q) * s;
  }
  return normalize(vel);
}
@body
",
            cut_wgsl_head!(4, "2.0", "-1.0", "0.0"),
            "  let p = vec2<f32>(x * D(3), y * D(3));
  let spacing = D(6);
  let vel = cut_magfield_velocity(p, base);
  let ang = glsl_atan2(vel.x, vel.y) / PI / 2.0;
  var lines = fract(ang / spacing);
  lines = min(lines, 1.0 - lines) * 2.0;
  let dd = 2.0 / 600.0;
  let der = (length(vel - cut_magfield_velocity(p + vec2<f32>(0.0, dd), base))
    + length(vel - cut_magfield_velocity(p + vec2<f32>(dd, 0.0), base))
    + length(vel - cut_magfield_velocity(p + vec2<f32>(dd, dd), base))
    + length(vel - cut_magfield_velocity(p + vec2<f32>(dd, -dd), base))) / 4.0;
  lines /= der / spacing;
  lines -= 1000.0 * 0.0005;
  lines = clamp(lines, 0.0, 1.0);
  let disc = clamp((length(p) - 1.0) / 0.001, 0.0, 1.0);
  lines = mix(1.0 - lines, lines, disc);
  let color = pow(lines, 1.0 / 2.2);
  let hide = select(color == 0.0, color > 0.0, i32(D(5)) != 0);
",
            cut_wgsl_tail!()
        ),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "curliecue",
        "curl_noise",
        "curl_sp",
        "cut_2ewangtile",
        "cut_alientext",
        "cut_apollonian",
        "cut_booleans",
        "cut_bricks",
        "cut_btree",
        "cut_btruchet",
        "cut_celtic",
        "cut_chains",
        "cut_circdes",
        "cut_fingerprint",
        "cut_fractal",
        "cut_fun",
        "cut_glypho",
        "cut_hexdots",
        "cut_hextruchetflow",
        "cut_jigsaw",
        "cut_kaleido",
        "cut_kleinian",
        "cut_magfield",
    ];

    #[test]
    fn all_names_present() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert!(DEFS.iter().any(|d| d.name == *n), "missing {n}");
            assert!(crate::variations::find(n).is_some(), "not registered: {n}");
        }
    }

    #[test]
    fn java_random_matches_java() {
        let mut r = JavaRandom::new(1000);
        assert_eq!(r.next_double(), 0.7101849056320707);
        assert_eq!(r.next_double(), 0.574836350385667);
        assert_eq!(JavaRandom::new(-5).next_double(), 0.26930095796932485);
    }

    fn run(def: &VarDef, params: &[f64], mut check: impl FnMut(&Pt)) {
        let coefs = [Affine::default(), Affine { c00: 0.8, c01: -0.3, c10: 0.4, c11: 0.9, c20: 0.35, c21: -0.6 }];
        let pts = [(0.3, -0.7, 0.2), (1.5, 2.0, -0.5), (-0.1, 0.05, 0.0), (-2.2, -0.4, 1.0)];
        for c in &coefs {
            let data = def.prepare_data(params, c);
            for preserve_z in [false, true] {
                let mut ctx = Ctx::new(42, preserve_z);
                for &(x, y, z) in &pts {
                    for _ in 0..16 {
                        let mut a = Pt { x, y, z, color: 0.5, ..Default::default() };
                        let mut v = Pt { color: 0.5, ..Default::default() };
                        (def.apply)(&data, &mut ctx, &mut a, &mut v, 0.75);
                        assert!(!a.is_bad() && !v.is_bad(), "{}: ({x},{y},{z}) -> {:?}", def.name, v);
                        assert!(v.color.is_finite(), "{}: color {:?}", def.name, v.color);
                        check(&v);
                    }
                }
            }
        }
    }

    #[test]
    fn finite_on_sample_points() {
        for def in DEFS {
            run(def, &[], |_| {});
        }
    }

    #[test]
    fn finite_with_variants() {
        let find = |n: &str| DEFS.iter().find(|d| d.name == n).unwrap();
        // curliecue: all types / symmetries, with and without lines
        for typ in [0.0, 1.0] {
            for sym in 1..=4 {
                for lines in [0.0, 1.0] {
                    run(find("curliecue"), &[300.0, 7.0, typ, 5.0, 0.05, sym as f64, 0.2, lines], |v| {
                        assert!((0.0..=1.0).contains(&v.color));
                    });
                }
            }
        }
        // cut_* in affine mode and inverted
        for def in DEFS.iter().filter(|d| d.name.starts_with("cut_")) {
            let mut p: Vec<f64> = def.params.iter().map(|p| p.default).collect();
            for (i, pd) in def.params.iter().enumerate() {
                match pd.name {
                    "mode" => p[i] = 0.0,
                    "invert" => p[i] = 1.0 - p[i],
                    _ => {}
                }
            }
            run(def, &p, |_| {});
            for (i, pd) in def.params.iter().enumerate() {
                if pd.name == "mode" {
                    p[i] = 1.0;
                }
            }
            run(def, &p, |_| {});
        }
        run(find("cut_hextruchetflow"), &[3.0, 1.0, 1.0, 10.0, 0.0], |_| {});
        run(find("cut_booleans"), &[1000.0, 1.0, 2.0, 250.0, 0.0], |_| {});
        run(find("curl_sp"), &[2.0, -0.5, 0.3, 0.2, 0.1, 1.0], |_| {});
    }

    #[test]
    fn cut_variations_hide_some_points() {
        // In random mode each pattern must hide part of the samples and keep the rest.
        for def in DEFS.iter().filter(|d| d.name.starts_with("cut_")) {
            let data = def.prepare_data(&[], &Affine::default());
            let mut ctx = Ctx::new(7, false);
            let mut hidden = 0;
            let n = 4000;
            for _ in 0..n {
                let mut a = Pt::default();
                let mut v = Pt::default();
                (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
                if v.do_hide {
                    hidden += 1;
                }
            }
            assert!(hidden > 0 && hidden < n, "{}: hidden {hidden} of {n}", def.name);
        }
    }
}
