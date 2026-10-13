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

//! Variations ported from the JWildfire sources (batch w09): dc_turbulence .. eSwirl.

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

/// The five `DC_BaseFunc` parameters, appended after the variation's own parameters.
macro_rules! dc_base_params {
    ($($p:expr),* $(,)?) => {
        &[
            $($p,)*
            ParamDef::int("ColorOnly", 0.0, 0.0, 1.0),
            ParamDef::int("Gradient", 0.0, 0.0, 2.0),
            ParamDef::range("scale_z", 0.0, -2.0, 2.0),
            ParamDef::range("offset_z", 0.0, -2.0, 2.0),
            ParamDef::int("reset_z", 1.0, 0.0, 1.0),
        ]
    };
}

/// `DC_BaseFunc.setParameter` limits of the base parameters starting at `n`.
fn dc_limit_base(d: &mut [f64], n: usize) {
    d[n] = limit_value(d[n], 0.0, 1.0).trunc();
    d[n + 1] = limit_value(d[n + 1], 0.0, 2.0).trunc();
    d[n + 4] = limit_value(d[n + 4], 0.0, 1.0).trunc();
}

/// `DC_BaseFunc.transform` sampling: the affine point (`ColorOnly == 1`) or a random point.
fn dc_uv(ctx: &mut Ctx, a: &Pt, color_only: f64) -> Vec2 {
    if color_only as i32 == 1 {
        vec2(a.x, a.y)
    } else {
        let x = ctx.random() - 0.5;
        let y = ctx.random() - 0.5;
        vec2(x, y)
    }
}

/// `DC_BaseFunc.transform` tail for the base parameters `d[n..n + 5]`.
fn dc_finish(ctx: &Ctx, v: &mut Pt, amount: f64, uv: Vec2, color: Vec3, d: &[f64], n: usize) {
    dc_output(ctx, v, amount, uv, color, d[n + 1] as i32, d[n + 2], d[n + 3], d[n + 4] as i32);
}

/// `org.jwildfire.create.tina.random.MarsagliaRandomGenerator`.
struct Marsaglia {
    u: i32,
    v: i32,
}

impl Marsaglia {
    fn new(seed: i64) -> Self {
        let mut r = Marsaglia { u: 12244355, v: 34384 };
        r.randomize(seed);
        r
    }
    fn randomize(&mut self, seed: i64) {
        self.u = (seed << 16) as i32;
        self.v = ((seed << 16) as i32) >> 16;
    }
    fn random(&mut self) -> f64 {
        self.v = 36969i32.wrapping_mul(self.v & 65535).wrapping_add(self.v >> 16);
        self.u = 18000i32.wrapping_mul(self.u & 65535).wrapping_add(self.u >> 16);
        let rnd = (self.v << 16).wrapping_add(self.u);
        let res = rnd as f64 * (1.0 / 0x7fff_ffff as f64);
        if res < 0.0 { -res } else { res }
    }
}

/// `java.util.Random` (48-bit LCG): `new Random(seed)` / `setSeed`.
fn drunken_tiles_set_seed(seed: i64) -> i64 {
    (seed ^ 0x5_DEEC_E66D) & ((1i64 << 48) - 1)
}

fn drunken_tiles_next(seed: &mut i64, bits: u32) -> i64 {
    *seed = seed.wrapping_mul(0x5_DEEC_E66D).wrapping_add(0xB) & ((1i64 << 48) - 1);
    (*seed >> (48 - bits)) as i32 as i64
}

/// `java.util.Random.nextDouble()`.
fn drunken_tiles_next_double(seed: &mut i64) -> f64 {
    let hi = drunken_tiles_next(seed, 26);
    let lo = drunken_tiles_next(seed, 27);
    ((hi << 27) + lo) as f64 * (1.0 / (1i64 << 53) as f64)
}

/// eSeries `sqrt_safe`.
#[inline]
fn e_sqrt_safe(x: f64) -> f64 {
    if x <= 0.0 { 0.0 } else { x.sqrt() }
}

/// eSeries elliptic coordinates: `xmax` (>= 1).
fn e_xmax(x: f64, y: f64) -> f64 {
    let tmp = y * y + x * x + 1.0;
    let tmp2 = 2.0 * x;
    let xmax = (e_sqrt_safe(tmp + tmp2) + e_sqrt_safe(tmp - tmp2)) * 0.5;
    if xmax < 1.0 { 1.0 } else { xmax }
}

/// eSeries elliptic coordinates: `acos(clamp(x / xmax, -1, 1))`.
fn e_nu(x: f64, xmax: f64) -> f64 {
    let mut t = x / xmax;
    if t > 1.0 {
        t = 1.0;
    } else if t < -1.0 {
        t = -1.0;
    }
    t.acos()
}

/// WGSL prologue shared by the eSeries snippets (defines `xmax`, `mu`, `nu`).
macro_rules! e_wgsl_head {
    () => {
        "let e_tmp = a.y * a.y + a.x * a.x + 1.0;
         let e_tmp2 = 2.0 * a.x;
         var xmax = (sqrt(max(e_tmp + e_tmp2, 0.0)) + sqrt(max(e_tmp - e_tmp2, 0.0))) * 0.5;
         if (xmax < 1.0) { xmax = 1.0; }
         var mu = acosh(xmax);
         var nu = acos(clamp(a.x / xmax, -1.0, 1.0));
         "
    };
}

// ---------------------------------------------------------------------------------------------
// DC color functions (`getRGBColor`)
// ---------------------------------------------------------------------------------------------

fn dc_turbulence_rgb(uv: Vec2, time: f64, zoom: f64, level: i32) -> Vec3 {
    let p = uv * zoom - 20.0;
    let mut ik = p;
    let mut c = 1.0;
    let inten = 0.05;
    for n in 0..level {
        let t = time * (1.0 - (3.0 / (n + 1) as f64));
        ik = p + vec2((t - ik.x).cos() + (t + ik.y).sin(), (t - ik.y).sin() + (t + ik.x).cos());
        c += 1.0 / length(vec2(p.x / ((ik.x + t).sin() / inten), p.y / ((ik.y + t).cos() / inten)));
    }
    c /= level as f64;
    c = 1.5 - c.sqrt();
    Vec3::splat(c * c * c * c)
}

fn dc_vortex_hash4(p: f64) -> Vec4 {
    fract(sin(vec4(0.0, 1.0, 57.0, 58.0) + p) * 43758.54)
}

fn dc_vortex_noise(p: Vec2) -> f64 {
    let i = floor(p);
    let mut f = fract(p);
    f = f * f * (Vec2::splat(3.0) - f * 2.0);
    let t = dc_vortex_hash4(dot(i, vec2(1.0, 57.0)));
    mix(mix(t.x, t.y, f.x), mix(t.z, t.w, f.x), f.y)
}

fn dc_vortex_fbm(p: Vec2) -> f64 {
    let mut p = p;
    let mut s = 0.0;
    let mut a = 1.0;
    for _ in 0..6 {
        s += a * dc_vortex_noise(p);
        a *= 0.5;
        p = p * 2.0;
    }
    s
}

fn dc_vortex_flow(q: Vec2, time: f64) -> Vec2 {
    let mut dir = 1.0;
    let mut c = vec2(glsl_mod(time, 10.0) - 20.0, 0.6 * dir);
    let mut vr = Vec2::splat(0.0);
    for _ in 0..30 {
        let dd = q * 4.0 - c;
        vr = vr + vec2(dd.y, -dd.x) / (dot(dd, dd) + 0.05) * -0.25 * dir;
        c = vec2(c.x + 1.0, -c.y);
        dir = -dir;
    }
    vr
}

fn dc_vortex_rgb(xp: f64, yp: f64, time: f64, strength: i32, softness: f64, zoom: f64) -> Vec3 {
    let mut p = vec2(xp * zoom, yp * zoom);
    for _ in 0..strength {
        p = p - dc_vortex_flow(p, time) * 0.03;
    }
    vec3(0.5, 0.5, 0.5) * dc_vortex_fbm(p * softness + vec2(-0.1 * time, 0.0))
}

fn dc_warping_hash(p: Vec2) -> f64 {
    let h = dot(p, vec2(127.1, 311.7));
    -1.0 + 2.0 * fract(h.sin() * 43758.5453123)
}

fn dc_warping_noise(p: Vec2) -> f64 {
    let i = floor(p);
    let f = fract(p);
    // Java: (vec2(3) - vec2(2)) * f, i.e. u = f^3 (quirk kept)
    let u = f * f * ((Vec2::splat(3.0) - Vec2::splat(2.0)) * f);
    mix(
        mix(dc_warping_hash(i), dc_warping_hash(i + vec2(1.0, 0.0)), u.x),
        mix(dc_warping_hash(i + vec2(0.0, 1.0)), dc_warping_hash(i + vec2(1.0, 1.0)), u.x),
        u.y,
    )
}

fn dc_warping_fbm(p: Vec2) -> f64 {
    let m = mat2(0.80, 0.60, -0.60, 0.80);
    let mut p = p;
    let mut f = 0.0;
    f += 0.5000 * dc_warping_noise(p);
    p = m * (p * Vec2::splat(2.02));
    f += 0.2500 * dc_warping_noise(p);
    p = m * (p * Vec2::splat(2.03));
    f += 0.1250 * dc_warping_noise(p);
    p = m * (p * Vec2::splat(2.01));
    f += 0.0625 * dc_warping_noise(p);
    f / 0.9375
}

fn dc_warping_fbm2(p: Vec2) -> Vec2 {
    vec2(dc_warping_fbm(p), dc_warping_fbm(p.yx()))
}

fn dc_warping_map(p: Vec2) -> Vec3 {
    let p = p * Vec2::splat(0.7);
    let t1 = p * 4.0;
    let t2 = p + dc_warping_fbm2((p + dc_warping_fbm2(t1)) * 2.0);
    let t0 = dc_warping_fbm2(t2);
    let f = dot(t0, vec2(1.0, -1.0));
    let bl = smoothstep(-0.8, 0.8, f);
    let ti = smoothstep(-1.0, 1.0, dc_warping_fbm(p));
    mix(mix(vec3(0.50, 0.00, 0.00), vec3(1.00, 0.75, 0.35), ti), vec3(0.00, 0.00, 0.02), bl)
}

fn dc_warping_rgb(xp: f64, yp: f64, zoom: f64, x0: f64) -> Vec3 {
    let p = vec2(xp * zoom, yp * zoom) + vec2(x0, x0);
    let e = 0.0045;
    let colc = dc_warping_map(p);
    let gc = dot(colc, Vec3::splat(0.333));
    let cola = dc_warping_map(p + vec2(e, 0.0));
    let ga = dot(cola, Vec3::splat(0.333));
    let colb = dc_warping_map(p + vec2(0.0, e));
    let gb = dot(colb, Vec3::splat(0.333));
    let nor = normalize(vec3(ga - gc, e, gb - gc));
    let mut col = colc;
    col = col + vec3(1.0, 0.7, 0.6) * (8.0 * (2.0 * gc - ga - gb).abs());
    col = col * (1.0 + 0.2 * nor.y * nor.y);
    col = col + 0.05 * nor.y * nor.y * nor.y;
    let q = vec2((xp + 1.0) / 2.0, (yp + 1.0) / 2.0);
    col * (16.0 * q.x * q.y * (1.0 - q.x) * (1.0 - q.y)).powf(0.1)
}

fn dc_worley_h(n: Vec2, pattern: f64, distort: f64) -> Vec2 {
    fract(vec2(1.0, 12.34) + 1e4 * (n.x + n.y / (pattern * 0.01)).sin()) * (distort * 0.75) + 0.3
}

fn dc_worley_rgb(uv: Vec2, zoom: f64, pattern: f64, distort: f64) -> Vec3 {
    let u = uv * zoom;
    let mut o = Vec3::splat(1e9);
    for k in 0..9 {
        let p = ceil(u) + vec2((k - k / 3 * 3) as f64, (k / 3) as f64) - 2.0;
        let c = dc_worley_h(p, pattern, distort) + p - u;
        let l = dot(c, c);
        if l < o.x {
            // Java quirk: z receives the new y (= old x)
            o.y = o.x;
            o.z = o.y;
            o.x = l;
        } else if l < o.y {
            o.z = o.y;
            o.y = l;
        } else if l < o.z {
            o.z = l;
        }
    }
    o = sqrt(o) * 5.0;
    o = o - o.x;
    o + 4.0 * (o.y / (o.y / o.z + 1.0) - 0.5) - o
}

// ---------------------------------------------------------------------------------------------
// de_stijl / dla_wf / dragon_js / drunken_tiles helpers
// ---------------------------------------------------------------------------------------------

fn de_stijl_hash(seed: i32, n: i32) -> f64 {
    let mut h = seed.wrapping_add(n);
    h = (h ^ 61) ^ (h >> 16);
    h = h.wrapping_add(h << 3);
    h ^= h >> 4;
    h = h.wrapping_mul(0x27d4_eb2d);
    h ^= h >> 15;
    (h & 0x7fff_ffff) as f64 / 0x7fff_ffff as f64
}

/// `DLAWFFunc.calculate`: diffusion limited aggregation on a `size` x `size` grid.
fn dla_wf_calculate(size: i32, max_iter: i32, seed: i32) -> Vec<u8> {
    let n = size as usize;
    let mut q = vec![0u8; n * n];
    let idx = |i: i32, j: i32| i as usize * n + j as usize;
    let centre = size / 2;
    let size2 = size - 2;
    let pi2 = 2.0 * std::f64::consts::PI;
    let mut rg = Marsaglia::new(seed as i64);
    q[idx(centre, centre)] = 1;
    let mut r1 = 3.0f64;
    let mut r2 = 3.0 * r1;
    let mut i = 0;
    // Safety net (not in Java, which would hang): stop when walkers keep escaping.
    let mut failed_in_a_row = 0u32;
    while i < max_iter {
        let phi = pi2 * rg.random();
        let ri = r1 * phi.cos();
        let rj = r1 * phi.sin();
        let mut ci = centre + (ri + 0.5) as i32;
        let mut cj = centre + (rj + 0.5) as i32;
        loop {
            let mut rr = rg.random();
            rr += rr;
            rr += rr;
            match rr as i32 {
                0 => ci += 1,
                1 => cj -= 1,
                2 => ci -= 1,
                _ => cj += 1,
            }
            if ci < 1 || ci > size2 || cj < 1 || cj > size2 {
                i -= 1;
                failed_in_a_row += 1;
                break;
            }
            let sum = q[idx(ci - 1, cj)] as i32
                + q[idx(ci + 1, cj)] as i32
                + q[idx(ci, cj - 1)] as i32
                + q[idx(ci, cj + 1)] as i32;
            let r3 = (((ci - centre) as f64).powi(2) + ((cj - centre) as f64).powi(2)).sqrt();
            if sum != 0 {
                q[idx(ci, cj)] = 1;
                failed_in_a_row = 0;
                if r3 > r1 {
                    r1 = r3;
                    r2 = 2.1 * r1;
                }
                break;
            } else if r3 > r2 {
                i -= 1;
                failed_in_a_row += 1;
                break;
            }
        }
        if failed_in_a_row > 200_000 {
            break;
        }
        i += 1;
    }
    q
}

/// d = [buffer_size, max_iter, seed, scale, jitter, n, x0, y0, x1, y1, ...]
fn dla_wf_prepare(p: &[f64]) -> Vec<f64> {
    let buffer_size = ftoi(p[0]);
    let max_iter = ftoi(p[1]);
    let seed = ftoi(p[2]);
    let scale = p[3];
    let jitter = if p[4] < 0.01 { 0.01 } else { p[4] };
    let mut d = vec![buffer_size as f64, max_iter as f64, seed as f64, scale, jitter, 0.0];
    // Java throws for tiny buffers; huge buffers are rejected to keep memory bounded.
    if !(3..=8192).contains(&buffer_size) {
        return d;
    }
    let q = dla_wf_calculate(buffer_size, max_iter, seed);
    let jitter_radius = java_max(java_min(1.0, jitter), 0.0);
    let mut rg = Marsaglia::new(seed as i64);
    let n = buffer_size as usize;
    let half = buffer_size / 2;
    let mut count = 0usize;
    for i in 0..n {
        for j in 0..n {
            let a_rnd = if jitter_radius > EPSILON { rg.random() } else { 0.0 };
            if q[i * n + j] != 0 {
                let mut x = (i as i32 - half) as f64 / buffer_size as f64 * scale;
                let mut y = (j as i32 - half) as f64 / buffer_size as f64 * scale;
                if jitter_radius > EPSILON {
                    let (sa, ca) = (a_rnd * 2.0 * M_PI).sin_cos();
                    x += jitter_radius * ca;
                    y += jitter_radius * sa;
                }
                d.push(x);
                d.push(y);
                count += 1;
            }
        }
    }
    d[5] = count as f64;
    d
}

struct DragonTurtle {
    x: f64,
    y: f64,
    angle: f64,
    pts: Vec<f64>,
}

impl DragonTurtle {
    fn go_forward(&mut self) {
        let rad = self.angle.to_radians();
        self.x += rad.cos();
        self.y += rad.sin();
        self.pts.push(self.x);
        self.pts.push(self.y);
    }
}

fn dragon_js_dragon(t: &mut DragonTurtle, n: i32) {
    if n == 0 {
        t.go_forward();
    } else {
        dragon_js_dragon(t, n - 1);
        t.angle += 90.0;
        dragon_js_nogard(t, n - 1);
    }
}

fn dragon_js_nogard(t: &mut DragonTurtle, n: i32) {
    if n == 0 {
        t.go_forward();
    } else {
        dragon_js_dragon(t, n - 1);
        t.angle -= 90.0;
        dragon_js_nogard(t, n - 1);
    }
}

/// d = [level, line_thickness, line_thickness / 100, nseg, x0, y0, x1, y1, ...] (turtle positions)
fn dragon_js_prepare(p: &[f64]) -> Vec<f64> {
    let level = limit_value(ftoi(p[0]), 1, 21);
    let mut t = DragonTurtle { x: 0.0, y: 0.0, angle: 0.0, pts: vec![0.0, 0.0] };
    dragon_js_dragon(&mut t, level);
    let nseg = t.pts.len() / 2 - 1;
    let mut d = vec![level as f64, p[1], p[1] / 100.0, nseg as f64];
    d.extend_from_slice(&t.pts);
    d
}

fn drunken_tiles_point_sign(p1x: f64, p1y: f64, p2x: f64, p2y: f64, p3x: f64, p3y: f64) -> f64 {
    (p1x - p3x) * (p2y - p3y) - (p2x - p3x) * (p1y - p3y)
}

/// Boundary test of `DrunkenTilesFunc`: (apply_effect, localDistSq).
fn drunken_tiles_inside(d: &[f64], lx: f64, ly: f64) -> (bool, f64) {
    let shape = d[4] as i32;
    let radius = d[15];
    let r2 = d[16];
    let dsq = lx * lx + ly * ly;
    // hexagon (6 vertices) / star5 (10 vertices alternating outer and inner radius)
    let poly = |n: i32, star: bool| -> bool {
        let step = M_PI / if star { 5.0 } else { 3.0 };
        let r_inner = radius * java_max(0.01, java_min(1.0, d[6]));
        let (mut v2x, mut v2y) = (0.0, radius);
        for i in 0..n {
            let (v1x, v1y) = (v2x, v2y);
            let angle = M_PI / 2.0 + (i + 1) as f64 * step;
            let cr = if !star || (i + 1) % 2 == 0 { radius } else { r_inner };
            v2x = cr * angle.cos();
            v2y = cr * angle.sin();
            if (v2x - v1x) * (ly - v1y) - (v2y - v1y) * (lx - v1x) < 0.0 {
                return false;
            }
        }
        true
    };
    match shape {
        1 => (lx.abs() <= radius && ly.abs() <= radius, dsq),
        2 => {
            let ar = java_max(1e-9, d[5].abs());
            let rx_sq = radius * radius;
            let ry = radius / ar;
            let ry_sq = ry * ry;
            (rx_sq > 1e-12 && ry_sq > 1e-12 && (lx * lx) / rx_sq + (ly * ly) / ry_sq <= 1.0, dsq)
        }
        3 => {
            let r = radius;
            let s32 = 3.0f64.sqrt() / 2.0;
            let (v1x, v1y) = (0.0, r);
            let (v2x, v2y) = (r * s32, -r / 2.0);
            let (v3x, v3y) = (-r * s32, -r / 2.0);
            let d1 = drunken_tiles_point_sign(lx, ly, v1x, v1y, v2x, v2y);
            let d2 = drunken_tiles_point_sign(lx, ly, v2x, v2y, v3x, v3y);
            let d3 = drunken_tiles_point_sign(lx, ly, v3x, v3y, v1x, v1y);
            let hn = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
            let hp = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
            (!(hn && hp), dsq)
        }
        4 => {
            let (s, c) = (-M_PI / 4.0).sin_cos();
            let rlx = c * lx - s * ly;
            let rly = s * lx + c * ly;
            (rlx.abs() <= radius && rly.abs() <= radius, dsq)
        }
        5 => (poly(6, false), dsq),
        6 => (poly(10, true), dsq),
        7 => {
            let arm = radius * java_max(0.01, java_min(1.0, d[7]));
            ((lx.abs() <= arm && ly.abs() <= radius) || (lx.abs() <= radius && ly.abs() <= arm), dsq)
        }
        8 => {
            let ri = radius * java_max(0.0, java_min(0.99, d[6]));
            (dsq >= ri * ri && dsq <= r2, dsq)
        }
        9..=11 => {
            let cr = dsq.sqrt();
            let max_r = if shape == 11 { radius * (1.0 + d[13].abs()) } else { radius };
            if cr > max_r {
                return (false, -1.0);
            }
            if cr < 1e-9 {
                return (true, 0.0);
            }
            let mut angle = ly.atan2(lx);
            let boundary = match shape {
                9 => radius * (java_max(2.0, d[11]) * angle).cos().abs(),
                10 => {
                    while angle < 0.0 {
                        angle += 2.0 * M_PI;
                    }
                    let k = java_max(3.0, d[12]);
                    let depth = java_max(0.0, java_min(1.0, d[6]));
                    let r_inner = radius * depth;
                    let apv = M_PI / k;
                    let sector = (angle / apv).floor() as i32;
                    let t = (angle - sector as f64 * apv) / apv;
                    let t_norm = 2.0 * (t - 0.5).abs();
                    if sector % 2 == 0 {
                        r_inner + (radius - r_inner) * (1.0 - t_norm)
                    } else {
                        r_inner + (radius - r_inner) * t_norm
                    }
                }
                _ => {
                    let amp = java_max(0.0, d[13]);
                    let freq = java_max(1e-6, d[14]);
                    let noise = 0.6 * (freq * angle).sin()
                        + 0.3 * (2.1 * freq * angle + 1.23).sin()
                        + 0.1 * (4.3 * freq * angle + 4.56).sin();
                    radius * (1.0 + amp * noise)
                }
            };
            if cr <= boundary + 1e-9 { (true, cr * cr) } else { (false, -1.0) }
        }
        _ => (dsq <= r2, dsq),
    }
}

// ---------------------------------------------------------------------------------------------
// Variations
// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "dc_turbulence",
        params: dc_base_params![
            ParamDef::int("seed", 10000.0, 0.0, 100000.0),
            ParamDef::range("time", 0.0, 1.0, 1000.0),
            ParamDef::range("zoom", 8.0, 0.1, 50.0),
            ParamDef::int("level", 3.0, 1.0, 5.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, time, zoom, level, ColorOnly, Gradient, scale_z, offset_z, reset_z]
            let uv = dc_uv(ctx, a, d[4]);
            let color = dc_turbulence_rgb(uv, d[1], d[2], d[3] as i32);
            dc_finish(ctx, v, amount, uv, color, d, 4);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            // setParameter limits (time is limited to 1..1000 when set: the default 0 becomes 1)
            d[1] = limit_value(d[1], 1.0, 1000.0);
            d[3] = limit_value(d[3], 1.0, 5.0).trunc();
            dc_limit_base(&mut d, 4);
            d
        }),
        wgsl: "fn dc_turbulence_rgb(uv: vec2<f32>, time: f32, zoom: f32, level: i32) -> vec3<f32> {
  let p = uv * zoom - 20.0;
  var ik = p;
  var c = 1.0;
  let inten = 0.05;
  for (var n = 0; n < level; n++) {
    let t = time * (1.0 - 3.0 / f32(n + 1));
    ik = p + vec2<f32>(cos(t - ik.x) + sin(t + ik.y), sin(t - ik.y) + cos(t + ik.x));
    c += 1.0 / length(vec2<f32>(p.x / (sin(ik.x + t) / inten), p.y / (cos(ik.y + t) / inten)));
  }
  c /= f32(level);
  c = 1.5 - sqrt(c);
  return vec3<f32>(c * c * c * c);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(4)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_turbulence_rgb(uv, D(1), D(2), i32(D(3))), i32(D(5)), D(6), D(7), i32(D(8)));",
    },
    VarDef {
        name: "dc_voronoise",
        params: dc_base_params![
            ParamDef::range("zoom", 8.0, 0.1, 50.0),
            ParamDef::range("deltaX", 0.5, 0.0, 1.0),
            ParamDef::range("deltaY", 0.5, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [zoom, deltaX, deltaY, ColorOnly, Gradient, scale_z, offset_z, reset_z]
            let uv = dc_uv(ctx, a, d[3]);
            let color = Vec3::splat(glsl_iqnoise(uv * d[0], d[1], d[2]));
            dc_finish(ctx, v, amount, uv, color, d, 3);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[1] = limit_value(d[1], 0.0, 1.0);
            d[2] = limit_value(d[2], 0.0, 1.0);
            dc_limit_base(&mut d, 3);
            d
        }),
        wgsl: "var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
         if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
         let col = vec3<f32>(glsl_iqnoise(uv * D(0), D(1), D(2)));
         dc_output(&v, AMOUNT, uv, col, i32(D(4)), D(5), D(6), i32(D(7)));",
    },
    VarDef {
        name: "dc_vortex",
        params: dc_base_params![
            ParamDef::int("randomize", 0.0, 0.0, 100000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("flowStrength", 10.0, 1.0, 15.0),
            ParamDef::range("Blending", 5.0, 1.0, 5.0),
            ParamDef::range("zoom", 1.0, 0.1, 10.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [randomize, time, flowStrength, Blending, zoom, ColorOnly, Gradient, scale_z, offset_z, reset_z]
            let uv = dc_uv(ctx, a, d[5]);
            let color = dc_vortex_rgb(uv.x, uv.y, d[1], d[2] as i32, d[3], d[4]);
            dc_finish(ctx, v, amount, uv, color, d, 5);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[2] = limit_value(d[2], 1.0, 15.0).trunc();
            d[3] = limit_value(d[3], 1.0, 5.0);
            dc_limit_base(&mut d, 5);
            d
        }),
        wgsl: "fn dc_vortex_hash4(p: f32) -> vec4<f32> {
  return fract(sin(vec4<f32>(0.0, 1.0, 57.0, 58.0) + p) * 43758.54);
}
fn dc_vortex_noise(p: vec2<f32>) -> f32 {
  let i = floor(p);
  var f = fract(p);
  f = f * f * (vec2<f32>(3.0) - f * 2.0);
  let t = dc_vortex_hash4(dot(i, vec2<f32>(1.0, 57.0)));
  return mix(mix(t.x, t.y, f.x), mix(t.z, t.w, f.x), f.y);
}
fn dc_vortex_fbm(p_in: vec2<f32>) -> f32 {
  var p = p_in;
  var s = 0.0;
  var amp = 1.0;
  for (var i = 0; i < 6; i++) {
    s += amp * dc_vortex_noise(p);
    amp *= 0.5;
    p = p * 2.0;
  }
  return s;
}
fn dc_vortex_flow(q: vec2<f32>, time: f32) -> vec2<f32> {
  var dir = 1.0;
  var c = vec2<f32>(glsl_mod(time, 10.0) - 20.0, 0.6 * dir);
  var vr = vec2<f32>(0.0);
  for (var k = 0; k < 30; k++) {
    let dd = q * 4.0 - c;
    vr += vec2<f32>(dd.y, -dd.x) / (dot(dd, dd) + 0.05) * -0.25 * dir;
    c = vec2<f32>(c.x + 1.0, -c.y);
    dir = -dir;
  }
  return vr;
}
fn dc_vortex_rgb(xp: f32, yp: f32, time: f32, strength: i32, softness: f32, zoom: f32) -> vec3<f32> {
  var p = vec2<f32>(xp * zoom, yp * zoom);
  for (var i = 0; i < strength; i++) {
    p -= dc_vortex_flow(p, time) * 0.03;
  }
  return vec3<f32>(0.5) * dc_vortex_fbm(p * softness + vec2<f32>(-0.1 * time, 0.0));
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(5)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_vortex_rgb(uv.x, uv.y, D(1), i32(D(2)), D(3), D(4));
dc_output(&v, AMOUNT, uv, col, i32(D(6)), D(7), D(8), i32(D(9)));",
    },
    VarDef {
        name: "dc_warping",
        params: dc_base_params![
            ParamDef::int("randomize", 0.0, 0.0, 100000.0),
            ParamDef::range("zoom", 1.0, 0.1, 10.0),
            ParamDef::range("x0", 0.0, -1.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [randomize, zoom, x0, ColorOnly, Gradient, scale_z, offset_z, reset_z]
            // (`randomize` only re-rolls x0 in the Java editor; x0 is the effective value)
            let uv = dc_uv(ctx, a, d[3]);
            let color = dc_warping_rgb(uv.x, uv.y, d[1], d[2]);
            dc_finish(ctx, v, amount, uv, color, d, 3);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            dc_limit_base(&mut d, 3);
            d
        }),
        wgsl: "fn dc_warping_hash(p: vec2<f32>) -> f32 {
  let h = dot(p, vec2<f32>(127.1, 311.7));
  return -1.0 + 2.0 * fract(sin(h) * 43758.5453123);
}
fn dc_warping_noise(p: vec2<f32>) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let u = f * f * f;
  return mix(mix(dc_warping_hash(i), dc_warping_hash(i + vec2<f32>(1.0, 0.0)), u.x),
             mix(dc_warping_hash(i + vec2<f32>(0.0, 1.0)), dc_warping_hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}
fn dc_warping_fbm(p_in: vec2<f32>) -> f32 {
  let m = mat2x2<f32>(0.80, 0.60, -0.60, 0.80);
  var p = p_in;
  var f = 0.0;
  f += 0.5000 * dc_warping_noise(p);
  p = m * (p * 2.02);
  f += 0.2500 * dc_warping_noise(p);
  p = m * (p * 2.03);
  f += 0.1250 * dc_warping_noise(p);
  p = m * (p * 2.01);
  f += 0.0625 * dc_warping_noise(p);
  return f / 0.9375;
}
fn dc_warping_fbm2(p: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(dc_warping_fbm(p), dc_warping_fbm(p.yx));
}
fn dc_warping_map(p_in: vec2<f32>) -> vec3<f32> {
  let p = p_in * 0.7;
  let t1 = p * 4.0;
  let t2 = p + dc_warping_fbm2((p + dc_warping_fbm2(t1)) * 2.0);
  let t0 = dc_warping_fbm2(t2);
  let f = dot(t0, vec2<f32>(1.0, -1.0));
  let bl = smoothstep(-0.8, 0.8, f);
  let ti = smoothstep(-1.0, 1.0, dc_warping_fbm(p));
  return mix(mix(vec3<f32>(0.50, 0.00, 0.00), vec3<f32>(1.00, 0.75, 0.35), ti), vec3<f32>(0.00, 0.00, 0.02), bl);
}
fn dc_warping_rgb(xp: f32, yp: f32, zoom: f32, x0: f32) -> vec3<f32> {
  let p = vec2<f32>(xp * zoom, yp * zoom) + vec2<f32>(x0, x0);
  let e = 0.0045;
  let colc = dc_warping_map(p);
  let gc = dot(colc, vec3<f32>(0.333));
  let cola = dc_warping_map(p + vec2<f32>(e, 0.0));
  let ga = dot(cola, vec3<f32>(0.333));
  let colb = dc_warping_map(p + vec2<f32>(0.0, e));
  let gb = dot(colb, vec3<f32>(0.333));
  let nor = normalize(vec3<f32>(ga - gc, e, gb - gc));
  var col = colc + vec3<f32>(1.0, 0.7, 0.6) * (8.0 * abs(2.0 * gc - ga - gb));
  col = col * (1.0 + 0.2 * nor.y * nor.y);
  col = col + 0.05 * nor.y * nor.y * nor.y;
  let q = vec2<f32>((xp + 1.0) / 2.0, (yp + 1.0) / 2.0);
  return col * java_pow(16.0 * q.x * q.y * (1.0 - q.x) * (1.0 - q.y), 0.1);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_warping_rgb(uv.x, uv.y, D(1), D(2));
dc_output(&v, AMOUNT, uv, col, i32(D(4)), D(5), D(6), i32(D(7)));",
    },
    VarDef {
        name: "dc_worley",
        params: dc_base_params![
            ParamDef::range("zoom", 7.0, 0.1, 50.0),
            ParamDef::range("pattern", 70.0, 0.1, 100.0),
            ParamDef::range("distort", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [zoom, pattern, distort, ColorOnly, Gradient, scale_z, offset_z, reset_z]
            let uv = dc_uv(ctx, a, d[3]);
            let color = dc_worley_rgb(uv, d[0], d[1], d[2]);
            dc_finish(ctx, v, amount, uv, color, d, 3);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(d[0], 0.1, 50.0);
            d[1] = limit_value(d[1], 0.1, 100.0);
            d[2] = limit_value(d[2], 0.0, 1.0);
            dc_limit_base(&mut d, 3);
            d
        }),
        wgsl: "fn dc_worley_h(n: vec2<f32>, pattern: f32, distort: f32) -> vec2<f32> {
  return fract(vec2<f32>(1.0, 12.34) + 1e4 * sin(n.x + n.y / (pattern * 0.01))) * (distort * 0.75) + 0.3;
}
fn dc_worley_rgb(uv: vec2<f32>, zoom: f32, pattern: f32, distort: f32) -> vec3<f32> {
  let u = uv * zoom;
  var o = vec3<f32>(1e9);
  for (var k = 0; k < 9; k++) {
    let p = ceil(u) + vec2<f32>(f32(k % 3), f32(k / 3)) - 2.0;
    let c = dc_worley_h(p, pattern, distort) + p - u;
    let l = dot(c, c);
    if (l < o.x) {
      o.y = o.x;
      o.z = o.y;
      o.x = l;
    } else if (l < o.y) {
      o.z = o.y;
      o.y = l;
    } else if (l < o.z) {
      o.z = l;
    }
  }
  o = sqrt(o) * 5.0;
  o = o - o.x;
  return o + 4.0 * (o.y / (o.y / o.z + 1.0) - 0.5) - o;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_worley_rgb(uv, D(0), D(1), D(2)), i32(D(4)), D(5), D(6), i32(D(7)));",
    },
    VarDef {
        name: "dc_ztransl",
        params: &[
            ParamDef::range("x0", 0.0, 0.0, 1.0),
            ParamDef::range("x1", 1.0, 0.0, 1.0),
            ParamDef::range("factor", 1.0, -2.0, 2.0),
            ParamDef::int("overwrite", 1.0, 0.0, 1.0),
            ParamDef::int("clamp", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            // d = [x0, x1, factor, overwrite, clamp, _x0, _x1, _x1_m_x0]
            let mut zf = d[2] * (a.color - d[5]) / d[7];
            if d[4] != 0.0 {
                zf = if zf < 0.0 {
                    0.0
                } else if zf > 1.0 {
                    1.0
                } else {
                    zf
                };
            }
            v.x += amount * a.x;
            v.y += amount * a.y;
            if d[3] == 0.0 {
                v.z += amount * a.z * zf;
            } else {
                v.z += amount * zf;
            }
        },
        prepare: Some(|p, _| {
            let x0 = limit_value(p[0], 0.0, 1.0);
            let x1 = limit_value(p[1], 0.0, 1.0);
            let overwrite = limit_value(ftoi(p[3]), 0, 1) as f64;
            let clamp = limit_value(ftoi(p[4]), 0, 1) as f64;
            let lo = if x0 < x1 { x0 } else { x1 };
            let hi = if x0 > x1 { x0 } else { x1 };
            let diff = if hi - lo == 0.0 { SMALL_EPSILON } else { hi - lo };
            vec![x0, x1, p[2], overwrite, clamp, lo, hi, diff]
        }),
        wgsl: "var den = D(7);
         if (den == 0.0) { den = EPS; }
         var zf = D(2) * (a.color - D(5)) / den;
         if (D(4) != 0.0) { zf = clamp(zf, 0.0, 1.0); }
         v.x += AMOUNT * a.x;
         v.y += AMOUNT * a.y;
         if (D(3) == 0.0) {
           v.z += AMOUNT * a.z * zf;
         } else {
           v.z += AMOUNT * zf;
         }",
    },
    simple(
        "deltaA",
        false,
        |_, ctx, a, v, amount| {
            let avgr = amount * ((sqr(a.y) + sqr(a.x + 1.0)).sqrt() / (sqr(a.y) + sqr(a.x - 1.0)).sqrt());
            let avga = (a.y.atan2(a.x - 1.0) - a.y.atan2(a.x + 1.0)) / 2.0;
            let (sa, ca) = avga.sin_cos();
            v.x += avgr * ca;
            v.y += avgr * sa;
            ctx.pass_z(a, v, amount);
        },
        "let avgr = AMOUNT * (sqrt(a.y * a.y + sqr(a.x + 1.0)) / sqrt(a.y * a.y + sqr(a.x - 1.0)));
         let avga = (atan2(a.y, a.x - 1.0) - atan2(a.y, a.x + 1.0)) / 2.0;
         v.x += avgr * cos(avga);
         v.y += avgr * sin(avga);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "deltoid",
        params: &[ParamDef::range("r", 0.33, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = d[0];
            let theta = a.y.atan2(a.x);
            v.x += amount * (2.0 * r * theta.cos() + r * (2.0 * theta).cos());
            v.y += amount * (2.0 * r * theta.sin() - r * (2.0 * theta).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let theta = atan2(a.y, a.x);
         v.x += AMOUNT * (2.0 * D(0) * cos(theta) + D(0) * cos(2.0 * theta));
         v.y += AMOUNT * (2.0 * D(0) * sin(theta) - D(0) * sin(2.0 * theta));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "de_stijl",
        params: &[
            ParamDef::range("max_size", 0.5, 0.0, 3.0),
            ParamDef::range("min_size", 0.1, 0.0, 3.0),
            ParamDef::range("chaos", 5.0, 0.1, 10.0),
            ParamDef::range("line_thickness", 0.02, 0.0, 3.0),
            ParamDef::int("seed", 12345.0, 0.0, 1000000.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [max_size, min_size, chaos, line_thickness, seed]
            let seed = d[4] as i32;
            let (x, y) = (a.x, a.y);
            let region_ix = (x / d[2]).floor() as i32;
            let region_iy = (y / d[2]).floor() as i32;
            let size_hash = de_stijl_hash(seed, region_ix.wrapping_mul(31337) ^ region_iy.wrapping_mul(8191));
            let mut lgs = d[1] + (d[0] - d[1]) * size_hash;
            if lgs < 1.0e-6 {
                lgs = 1.0e-6;
            }
            let snapped_x = java_round(x / lgs) * lgs;
            let snapped_y = java_round(y / lgs) * lgs;
            let half_thickness = d[3] * 0.025;
            if (x - snapped_x).abs() < half_thickness || (y - snapped_y).abs() < half_thickness {
                // on a line
                v.color = 0.0;
            } else {
                let ix = java_round(x / lgs) as i64 as i32;
                let iy = java_round(y / lgs) as i64 as i32;
                let cell_seed = (seed as i64).wrapping_add((ix as i64).wrapping_mul(73856093)) ^ (iy as i64).wrapping_mul(19349663);
                v.color = Marsaglia::new(cell_seed).random();
            }
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], p[3], p[4].trunc()]),
        wgsl: "fn de_stijl_hash(seed: i32, n: i32) -> f32 {
  var h = seed + n;
  h = (h ^ 61) ^ (h >> 16u);
  h = h + (h << 3u);
  h = h ^ (h >> 4u);
  h = h * 0x27d4eb2d;
  h = h ^ (h >> 15u);
  return f32(h & 0x7fffffff) / 2147483647.0;
}
@body
let seed = i32(D(4));
let x = a.x;
let y = a.y;
let region_ix = i32(floor(x / D(2)));
let region_iy = i32(floor(y / D(2)));
let size_hash = de_stijl_hash(seed, (region_ix * 31337) ^ (region_iy * 8191));
let lgs = max(D(1) + (D(0) - D(1)) * size_hash, 1.0e-6);
let gx = java_round(x / lgs);
let gy = java_round(y / lgs);
let half_thickness = D(3) * 0.025;
if (abs(x - gx * lgs) < half_thickness || abs(y - gy * lgs) < half_thickness) {
  v.color = 0.0;
} else {
  let cs = (seed + i32(gx) * 73856093) ^ (i32(gy) * 19349663);
  var mu = cs << 16u;
  var mv = mu >> 16u;
  mv = 36969 * (mv & 65535) + (mv >> 16u);
  mu = 18000 * (mu & 65535) + (mu >> 16u);
  let rn = (mv << 16u) + mu;
  v.color = abs(f32(rn) * (1.0 / 2147483647.0));
}
v.x += x * AMOUNT;
v.y += y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "devil_warp",
        params: &[
            ParamDef::range("a", 2.0, 0.0, 3.0),
            ParamDef::range("b", 1.0, 0.0, 3.0),
            ParamDef::range("effect", 1.0, -3.0, 3.0),
            ParamDef::range("warp", 0.5, -3.0, 3.0),
            ParamDef::range("rmin", -0.24, -5.0, 2.0),
            ParamDef::range("rmax", 100.0, -5.0, 100.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [a, b, effect, warp, rmin, rmax]; Java does not scale x/y by the amount
            let (xx, yy) = (a.x, a.y);
            let r2 = 1.0 / (xx * xx + yy * yy);
            let mut r = (xx * xx + r2 * d[1] * yy * yy).powf(d[3]) - (yy * yy + r2 * d[0] * xx * xx).powf(d[3]);
            if r > d[5] {
                r = d[5];
            } else if r < d[4] {
                r = d[4];
            }
            r *= d[2];
            v.x += xx * (1.0 + r);
            v.y += yy * (1.0 + r);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let xx = a.x;
         let yy = a.y;
         let r2 = 1.0 / (xx * xx + yy * yy);
         var r = java_pow(xx * xx + r2 * D(1) * yy * yy, D(3)) - java_pow(yy * yy + r2 * D(0) * xx * xx, D(3));
         if (r > D(5)) { r = D(5); } else if (r < D(4)) { r = D(4); }
         r = D(2) * r;
         v.x += xx * (1.0 + r);
         v.y += yy * (1.0 + r);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "diamond_lattice",
        params: &[ParamDef::range("scale", 2.0, 0.1, 10.0), ParamDef::range("pull", 0.5, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            const S: f64 = 0.7071067811865475;
            let ax = a.x * d[0];
            let ay = a.y * d[0];
            let u = (ax + ay) * S;
            let w = (ax - ay) * S;
            let nu = java_round(u);
            let nv = java_round(w);
            let bx = (nu + nv) * S;
            let by = (nu - nv) * S;
            v.x += amount * (bx + (u - nu) * (1.0 - d[1])) / d[0];
            v.y += amount * (by + (w - nv) * (1.0 - d[1])) / d[0];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let s = 0.7071067811865475;
         let ax = a.x * D(0);
         let ay = a.y * D(0);
         let u = (ax + ay) * s;
         let w = (ax - ay) * s;
         let nu = java_round(u);
         let nv = java_round(w);
         let bx = (nu + nv) * s;
         let by = (nu - nv) * s;
         v.x += AMOUNT * (bx + (u - nu) * (1.0 - D(1))) / D(0);
         v.y += AMOUNT * (by + (w - nv) * (1.0 - D(1))) / D(0);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "diatom",
        params: &[ParamDef::int("spokes", 8.0, 1.0, 20.0), ParamDef::range("amplitude", 0.3, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y + 1.0e-10).sqrt();
            let theta = a.y.atan2(a.x);
            let nr = r * (1.0 + d[1] * (d[0] * theta).cos());
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0].trunc(), p[1]]),
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y + 1.0e-10);
         let theta = atan2(a.y, a.x);
         let nr = r * (1.0 + D(1) * cos(D(0) * theta));
         v.x += AMOUNT * nr * cos(theta);
         v.y += AMOUNT * nr * sin(theta);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "dinis_surface_wf",
        params: &[ParamDef::range("a", 0.8, -2.0, 2.0), ParamDef::range("b", 0.2, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            let (u, w) = (a.x, a.y);
            let sinv = w.sin();
            v.x += amount * d[0] * u.cos() * sinv;
            v.y += amount * d[0] * u.sin() * sinv;
            if w.abs() > EPSILON {
                v.z += -amount * (d[0] * (w.cos() + (w.abs() / 2.0).tan().ln()) + d[1] * u);
                v.do_hide = false;
            } else {
                v.do_hide = true;
            }
        },
        prepare: None,
        wgsl: "let u = a.x;
         let w = a.y;
         let sinv = sin(w);
         v.x += AMOUNT * D(0) * cos(u) * sinv;
         v.y += AMOUNT * D(0) * sin(u) * sinv;
         if (abs(w) > EPSILON) {
           v.z += -AMOUNT * (D(0) * (cos(w) + log(tan(abs(w) / 2.0))) + D(1) * u);
           v.hide = 0.0;
         } else {
           v.hide = 1.0;
         }",
    },
    VarDef {
        name: "dipole",
        params: &[ParamDef::range("separation", 0.5, 0.0, 2.0), ParamDef::range("strength", 0.2, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let dx1 = a.x + d[0];
            let dx2 = a.x - d[0];
            let r1 = java_max((dx1 * dx1 + a.y * a.y).sqrt(), 1e-4);
            let r2 = java_max((dx2 * dx2 + a.y * a.y).sqrt(), 1e-4);
            let r13 = r1 * r1 * r1;
            let r23 = r2 * r2 * r2;
            let ex = dx1 / r13 - dx2 / r23;
            let ey = a.y / r13 - a.y / r23;
            v.x += amount * (a.x + d[1] * ex);
            v.y += amount * (a.y + d[1] * ey);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dx1 = a.x + D(0);
         let dx2 = a.x - D(0);
         let r1 = max(sqrt(dx1 * dx1 + a.y * a.y), 1e-4);
         let r2 = max(sqrt(dx2 * dx2 + a.y * a.y), 1e-4);
         let r13 = r1 * r1 * r1;
         let r23 = r2 * r2 * r2;
         let ex = dx1 / r13 - dx2 / r23;
         let ey = a.y / r13 - a.y / r23;
         v.x += AMOUNT * (a.x + D(1) * ex);
         v.y += AMOUNT * (a.y + D(1) * ey);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "disc3d",
        params: &[ParamDef::range("pi", M_PI, 0.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            let r = (a.y * a.y + a.x * a.x + SMALL_EPSILON).sqrt();
            let (sr, cr) = (d[0] * r).sin_cos();
            let vv = amount * a.x.atan2(a.y) / (d[0] + SMALL_EPSILON);
            v.x += vv * sr;
            v.y += vv * cr;
            v.z += vv * (r * a.z.cos());
        },
        prepare: None,
        wgsl: "let r = sqrt(a.y * a.y + a.x * a.x + EPS);
         let ang = D(0) * r;
         let vv = AMOUNT * atan2(a.x, a.y) / (D(0) + EPS);
         v.x += vv * sin(ang);
         v.y += vv * cos(ang);
         v.z += vv * (r * cos(a.z));",
    },
    VarDef {
        name: "disc3",
        params: &[
            ParamDef::range("a", 1.0, -2.0, 2.0),
            ParamDef::range("b", 1.0, -2.0, 2.0),
            ParamDef::range("c", 1.0, -2.0, 2.0),
            ParamDef::range("d", 1.0, -2.0, 2.0),
            ParamDef::range("e", 1.0, -2.0, 2.0),
            ParamDef::range("f", 1.0, -2.0, 2.0),
            ParamDef::range("g", 1.0, -2.0, 2.0),
            ParamDef::range("h", 1.0, -2.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [a, b, c, d, e, f, g, h]
            let r_pi = M_PI * (a.x * d[3] * a.x * d[4] + a.y * d[5] * a.y * d[6]).sqrt();
            let sinr = r_pi.sin() * d[0];
            let cosr = r_pi.cos() * d[1];
            let r = amount * a.atan_xy() / M_PI * d[2];
            v.x += sinr * d[7] * r;
            v.y += cosr * d[7] * r;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r_pi = PI * sqrt(a.x * D(3) * a.x * D(4) + a.y * D(5) * a.y * D(6));
         let sinr = sin(r_pi) * D(0);
         let cosr = cos(r_pi) * D(1);
         let r = AMOUNT * atan_xy(a) / PI * D(2);
         v.x += sinr * D(7) * r;
         v.y += cosr * D(7) * r;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "dla_wf",
        params: &[
            ParamDef::int("buffer_size", 800.0, 100.0, 2000.0),
            ParamDef::int("max_iter", 6000.0, 100.0, 25000.0),
            ParamDef::int("seed", 666.0, 0.0, 10000.0),
            ParamDef::range("scale", 10.0, 0.1, 20.0),
            ParamDef::range("jitter", 0.01, 0.01, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // Java draws from the cached point list; here a random point of the precalculated cluster.
            let n = d[5] as usize;
            if n > 0 {
                let i = ((ctx.random() * n as f64) as usize).min(n - 1);
                v.x += amount * d[6 + 2 * i];
                v.y += amount * d[7 + 2 * i];
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| dla_wf_prepare(p)),
        wgsl: "let n = u32(D(5));
         if (n > 0u) {
           let i = min(u32(rnd() * f32(n)), n - 1u);
           v.x += AMOUNT * prm[base + 6u + 2u * i];
           v.y += AMOUNT * prm[base + 7u + 2u * i];
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "domain_fold",
        params: &[ParamDef::range("fold_x", 1.0, -2.0, 2.0), ParamDef::range("fold_y", 1.0, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let fx = java_max(d[0].abs(), 1e-4);
            let fy = java_max(d[1].abs(), 1e-4);
            let mut nx = a.x;
            let mut ny = a.y;
            if nx > fx {
                nx = 2.0 * fx - nx;
            } else if nx < -fx {
                nx = -2.0 * fx - nx;
            }
            if ny > fy {
                ny = 2.0 * fy - ny;
            } else if ny < -fy {
                ny = -2.0 * fy - ny;
            }
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let fx = max(abs(D(0)), 1e-4);
         let fy = max(abs(D(1)), 1e-4);
         var nx = a.x;
         var ny = a.y;
         if (nx > fx) { nx = 2.0 * fx - nx; } else if (nx < -fx) { nx = -2.0 * fx - nx; }
         if (ny > fy) { ny = 2.0 * fy - ny; } else if (ny < -fy) { ny = -2.0 * fy - ny; }
         v.x += AMOUNT * nx;
         v.y += AMOUNT * ny;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "doppler",
        params: &[ParamDef::range("dir", 0.0, -M_PI, M_PI), ParamDef::range("speed", 0.5, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let spd = java_max(-0.99, java_min(0.99, d[1]));
            let (sa, ca) = d[0].sin_cos();
            let along = a.x * ca + a.y * sa;
            let perp = -a.x * sa + a.y * ca;
            let r = along * (1.0 / (1.0 + spd * along));
            v.x += amount * (r * ca - perp * sa);
            v.y += amount * (r * sa + perp * ca);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let spd = clamp(D(1), -0.99, 0.99);
         let ca = cos(D(0));
         let sa = sin(D(0));
         let along = a.x * ca + a.y * sa;
         let perp = -a.x * sa + a.y * ca;
         let r = along * (1.0 / (1.0 + spd * along));
         v.x += AMOUNT * (r * ca - perp * sa);
         v.y += AMOUNT * (r * sa + perp * ca);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "dragon_js",
        params: &[ParamDef::int("level", 2.0, 1.0, 21.0), ParamDef::range("line_thickness", 0.5, 0.0, 200.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // Java cycles through the segments; here a random segment is drawn.
            let nseg = d[3] as usize;
            let k = ((ctx.random() * nseg as f64) as usize).min(nseg - 1);
            let (x1, y1, x2, y2) = (d[4 + 2 * k], d[5 + 2 * k], d[6 + 2 * k], d[7 + 2 * k]);
            let ydiff = y2 - y1;
            let xdiff = x2 - x1;
            let m = if xdiff == 0.0 { 10000.0 } else { ydiff / xdiff };
            let line_length = (xdiff * xdiff + ydiff * ydiff).sqrt();
            let dd = ctx.random() * line_length;
            let mut xoffset = dd / (1.0 + m * m).sqrt();
            if x2 < x1 {
                xoffset = -xoffset;
            }
            let mut yoffset = (m * xoffset).abs();
            if y2 < y1 {
                yoffset = -yoffset;
            }
            let lt = d[2];
            if lt != 0.0 {
                xoffset += (ctx.random() - 0.5) * lt;
                yoffset += (ctx.random() - 0.5) * lt;
            }
            v.x += amount * (x1 + xoffset);
            v.y += amount * (y1 + yoffset);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| dragon_js_prepare(p)),
        wgsl: "let nseg = u32(D(3));
         let k = min(u32(rnd() * f32(nseg)), nseg - 1u);
         let o = base + 4u + 2u * k;
         let x1 = prm[o];
         let y1 = prm[o + 1u];
         let x2 = prm[o + 2u];
         let y2 = prm[o + 3u];
         let ydiff = y2 - y1;
         let xdiff = x2 - x1;
         var m = 10000.0;
         if (xdiff != 0.0) { m = ydiff / xdiff; }
         let line_length = sqrt(xdiff * xdiff + ydiff * ydiff);
         let dd = rnd() * line_length;
         var xoffset = dd / sqrt(1.0 + m * m);
         if (x2 < x1) { xoffset = -xoffset; }
         var yoffset = abs(m * xoffset);
         if (y2 < y1) { yoffset = -yoffset; }
         if (D(2) != 0.0) {
           xoffset += (rnd() - 0.5) * D(2);
           yoffset += (rnd() - 0.5) * D(2);
         }
         v.x += AMOUNT * (x1 + xoffset);
         v.y += AMOUNT * (y1 + yoffset);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "dragon_scale",
        params: &[ParamDef::range("freq", 3.0, 0.0, 10.0), ParamDef::range("relief", 0.4, -5.0, 5.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let freq = java_max(d[0].abs(), 0.01);
            let cx = java_round(a.x * freq) / freq;
            let cy = java_round(a.y * freq) / freq;
            let dx = a.x - cx;
            let dy = a.y - cy;
            let r2 = dx * dx + dy * dy;
            let cell_r = 0.5 / freq;
            let bump = java_max(0.0, java_min(1.0, d[1] * (1.0 - r2 / (cell_r * cell_r + 1e-6))));
            v.x += amount * (a.x + bump * dx);
            v.y += amount * (a.y + bump * dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let freq = max(abs(D(0)), 0.01);
         let dx = a.x - java_round(a.x * freq) / freq;
         let dy = a.y - java_round(a.y * freq) / freq;
         let r2 = dx * dx + dy * dy;
         let cell_r = 0.5 / freq;
         let bump = clamp(D(1) * (1.0 - r2 / (cell_r * cell_r + 1e-6)), 0.0, 1.0);
         v.x += AMOUNT * (a.x + bump * dx);
         v.y += AMOUNT * (a.y + bump * dy);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "drunken_tiles",
        params: &[
            ParamDef::int("seed", 1.0, 0.0, 1000000.0),
            ParamDef::range("cellsize", 0.5, 0.01, 5.0),
            ParamDef::range("radius_factor", 1.0, 0.1, 4.0),
            ParamDef::range("offset_strength", 0.25, -1.5, 1.5),
            ParamDef::int("shape_boundary_type", 0.0, 0.0, 11.0),
            ParamDef::range("aspect_ratio", 1.0, 0.25, 4.0),
            ParamDef::range("inner_radius_factor", 0.5, 0.0, 1.0),
            ParamDef::range("arm_width_factor", 0.3, 0.0, 1.0),
            ParamDef::range("spacing", 0.1, 0.0, 1.0),
            ParamDef::range("inner_twist", 0.0, -M_PI, M_PI),
            ParamDef::range("outer_twist", M_PI, -M_PI, M_PI),
            ParamDef::int("flowerPetals", 5.0, 2.0, 10.0),
            ParamDef::int("starPoints", 5.0, 3.0, 10.0),
            ParamDef::range("cloudAmplitude", 0.2, 0.0, 2.0),
            ParamDef::range("cloudFrequency", 5.0, 0.1, 20.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, cellsize, radius_factor, offset_strength, shape_boundary_type, aspect_ratio,
            //      inner_radius_factor, arm_width_factor, spacing, inner_twist, outer_twist,
            //      flowerPetals, starPoints, cloudAmplitude, cloudFrequency, radius, r2]
            let cellsize = d[1];
            let radius = d[15];
            if cellsize <= 1e-9 || radius <= 1e-9 {
                v.x += amount * a.x;
                v.y += amount * a.y;
                ctx.pass_z(a, v, amount);
                return;
            }
            let (mut fx, mut fy) = (a.x, a.y);
            let inv = 1.0 / cellsize;
            let ix = (a.x * inv).floor() as i32;
            let iy = (a.y * inv).floor() as i32;
            let cx = (ix as f64 + 0.5) * cellsize;
            let cy = (iy as f64 + 0.5) * cellsize;
            let cell_seed = (d[0] as i32) ^ ix.wrapping_mul(31) ^ iy;
            let mut s = drunken_tiles_set_seed(cell_seed as i64);
            let ox = (drunken_tiles_next_double(&mut s) * 2.0 - 1.0) * d[3] * cellsize;
            let oy = (drunken_tiles_next_double(&mut s) * 2.0 - 1.0) * d[3] * cellsize;
            let (cpx, cpy) = (cx + ox, cy + oy);
            let lx = a.x - cpx;
            let ly = a.y - cpy;
            let (inside, mut lds) = drunken_tiles_inside(d, lx, ly);
            if inside {
                let shape = d[4] as i32;
                let scale = 1.0 / radius;
                let mut slx = lx * scale;
                let mut sly = ly * scale;
                if slx > 1.0 {
                    slx -= 2.0;
                }
                if slx < -1.0 {
                    slx += 2.0;
                }
                if sly > 1.0 {
                    sly -= 2.0;
                }
                if sly < -1.0 {
                    sly += 2.0;
                }
                let k = 1.0 + d[8];
                let ljx = slx * k * radius;
                let ljy = sly * k * radius;
                let twist_norm = if shape == 1 {
                    java_min(1.0, java_max(lx.abs(), ly.abs()) / radius)
                } else if shape == 4 {
                    let (s, c) = (-M_PI / 4.0).sin_cos();
                    let rlx = c * lx - s * ly;
                    let rly = s * lx + c * ly;
                    java_min(1.0, java_max(rlx.abs(), rly.abs()) / radius)
                } else {
                    if lds < 0.0 {
                        lds = lx * lx + ly * ly;
                    }
                    java_min(1.0, lds.sqrt() / radius)
                };
                let theta = d[9] * (1.0 - twist_norm) + d[10] * twist_norm;
                let (s, c) = theta.sin_cos();
                fx = cpx + (c * ljx - s * ljy);
                fy = cpy + (s * ljx + c * ljy);
            }
            v.x += amount * fx;
            v.y += amount * fy;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = d[0].trunc();
            d[1] = java_max(1e-9, d[1]);
            d[2] = java_max(1e-9, d[2]);
            d[4] = java_max(0.0, java_min(11.0, d[4].trunc()));
            d[6] = java_max(0.0, java_min(1.0, d[6]));
            d[7] = java_max(0.0, java_min(1.0, d[7]));
            d[11] = java_max(2.0, d[11].trunc());
            d[12] = java_max(3.0, d[12].trunc());
            d[13] = java_max(0.0, d[13]);
            d[14] = java_max(1e-6, d[14]);
            let mut radius = (d[1] * 0.5 * d[2]).abs();
            if radius <= 1e-9 {
                radius = 1e-9;
            }
            d.push(radius);
            d.push(radius * radius);
            d
        }),
        wgsl: "fn drunken_tiles_seed(cs: i32) -> vec3<u32> {
  let su = bitcast<u32>(cs);
  var hi = 0u;
  if (cs < 0) { hi = 0xFFFFu; }
  return vec3<u32>((su & 0xFFFFu) ^ 0xE66Du, (su >> 16u) ^ 0xDEECu, hi ^ 0x5u);
}
// one step of the java.util.Random LCG on a 48-bit state held as three 16-bit limbs
fn drunken_tiles_lcg(s: vec3<u32>) -> vec3<u32> {
  let r0 = s.x * 0xE66Du + 0xBu;
  let p1 = s.x * 0xDEECu;
  let p2 = s.y * 0xE66Du;
  let r1 = (p1 & 0xFFFFu) + (p2 & 0xFFFFu) + (r0 >> 16u);
  let c1 = (p1 >> 16u) + (p2 >> 16u) + (r1 >> 16u);
  let r2 = s.x * 0x5u + s.y * 0xDEECu + s.z * 0xE66Du + c1;
  return vec3<u32>(r0 & 0xFFFFu, r1 & 0xFFFFu, r2 & 0xFFFFu);
}
fn drunken_tiles_sign(p1x: f32, p1y: f32, p2x: f32, p2y: f32, p3x: f32, p3y: f32) -> f32 {
  return (p1x - p3x) * (p2y - p3y) - (p2x - p3x) * (p1y - p3y);
}
fn drunken_tiles_poly(lx: f32, ly: f32, radius: f32, r_inner: f32, n: i32, star: bool) -> bool {
  var astep = PI / 3.0;
  if (star) { astep = PI / 5.0; }
  var v2x = 0.0;
  var v2y = radius;
  for (var i = 0; i < n; i++) {
    let v1x = v2x;
    let v1y = v2y;
    let ang = PI / 2.0 + f32(i + 1) * astep;
    var cr = radius;
    if (star && (i + 1) % 2 != 0) { cr = r_inner; }
    v2x = cr * cos(ang);
    v2y = cr * sin(ang);
    if ((v2x - v1x) * (ly - v1y) - (v2y - v1y) * (lx - v1x) < 0.0) { return false; }
  }
  return true;
}
// returns (inside, localDistSq)
fn drunken_tiles_inside(shape: i32, lx: f32, ly: f32, radius: f32, r2: f32, aspect: f32, irf: f32, awf: f32,
                        petals: f32, npoints: f32, camp: f32, cfreq: f32) -> vec2<f32> {
  let dsq = lx * lx + ly * ly;
  if (shape == 1) {
    return vec2<f32>(select(0.0, 1.0, abs(lx) <= radius && abs(ly) <= radius), dsq);
  } else if (shape == 2) {
    let ar = max(1e-9, abs(aspect));
    let rx_sq = radius * radius;
    let ry = radius / ar;
    let ry_sq = ry * ry;
    return vec2<f32>(select(0.0, 1.0, rx_sq > 1e-12 && ry_sq > 1e-12 && (lx * lx) / rx_sq + (ly * ly) / ry_sq <= 1.0), dsq);
  } else if (shape == 3) {
    let s32 = sqrt(3.0) / 2.0;
    let d1 = drunken_tiles_sign(lx, ly, 0.0, radius, radius * s32, -radius / 2.0);
    let d2 = drunken_tiles_sign(lx, ly, radius * s32, -radius / 2.0, -radius * s32, -radius / 2.0);
    let d3 = drunken_tiles_sign(lx, ly, -radius * s32, -radius / 2.0, 0.0, radius);
    let hn = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let hp = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    return vec2<f32>(select(0.0, 1.0, !(hn && hp)), dsq);
  } else if (shape == 4) {
    let c = cos(-PI / 4.0);
    let s = sin(-PI / 4.0);
    let rlx = c * lx - s * ly;
    let rly = s * lx + c * ly;
    return vec2<f32>(select(0.0, 1.0, abs(rlx) <= radius && abs(rly) <= radius), dsq);
  } else if (shape == 5) {
    return vec2<f32>(select(0.0, 1.0, drunken_tiles_poly(lx, ly, radius, radius, 6, false)), dsq);
  } else if (shape == 6) {
    let ri = radius * max(0.01, min(1.0, irf));
    return vec2<f32>(select(0.0, 1.0, drunken_tiles_poly(lx, ly, radius, ri, 10, true)), dsq);
  } else if (shape == 7) {
    let arm = radius * max(0.01, min(1.0, awf));
    return vec2<f32>(select(0.0, 1.0, (abs(lx) <= arm && abs(ly) <= radius) || (abs(lx) <= radius && abs(ly) <= arm)), dsq);
  } else if (shape == 8) {
    let ri = radius * max(0.0, min(0.99, irf));
    return vec2<f32>(select(0.0, 1.0, dsq >= ri * ri && dsq <= r2), dsq);
  } else if (shape >= 9 && shape <= 11) {
    let cr = sqrt(dsq);
    var max_r = radius;
    if (shape == 11) { max_r = radius * (1.0 + abs(camp)); }
    if (cr > max_r) { return vec2<f32>(0.0, -1.0); }
    if (cr < 1e-9) { return vec2<f32>(1.0, 0.0); }
    var ang = atan2(ly, lx);
    var boundary = 0.0;
    if (shape == 9) {
      boundary = radius * abs(cos(max(2.0, petals) * ang));
    } else if (shape == 10) {
      if (ang < 0.0) { ang += TWO_PI; }
      let k = max(3.0, npoints);
      let r_inner = radius * clamp(irf, 0.0, 1.0);
      let apv = PI / k;
      let sector = i32(floor(ang / apv));
      let t = (ang - f32(sector) * apv) / apv;
      let t_norm = 2.0 * abs(t - 0.5);
      if (sector % 2 == 0) {
        boundary = r_inner + (radius - r_inner) * (1.0 - t_norm);
      } else {
        boundary = r_inner + (radius - r_inner) * t_norm;
      }
    } else {
      let amp = max(0.0, camp);
      let freq = max(1e-6, cfreq);
      let nz = 0.6 * sin(freq * ang) + 0.3 * sin(2.1 * freq * ang + 1.23) + 0.1 * sin(4.3 * freq * ang + 4.56);
      boundary = radius * (1.0 + amp * nz);
    }
    if (cr <= boundary + 1e-9) { return vec2<f32>(1.0, cr * cr); }
    return vec2<f32>(0.0, -1.0);
  }
  return vec2<f32>(select(0.0, 1.0, dsq <= r2), dsq);
}
@body
let cellsize = D(1);
let radius = D(15);
var fx = a.x;
var fy = a.y;
if (cellsize > 1e-9 && radius > 1e-9) {
  let ix = i32(floor(a.x / cellsize));
  let iy = i32(floor(a.y / cellsize));
  let cx = (f32(ix) + 0.5) * cellsize;
  let cy = (f32(iy) + 0.5) * cellsize;
  // nextDouble() ~ next(26) / 2^26 (the low next(27) part is below f32 precision)
  var s = drunken_tiles_lcg(drunken_tiles_seed(i32(D(0)) ^ (ix * 31) ^ iy));
  let ox = (f32((s.z << 10u) | (s.y >> 6u)) / 67108864.0 * 2.0 - 1.0) * D(3) * cellsize;
  s = drunken_tiles_lcg(drunken_tiles_lcg(s));
  let oy = (f32((s.z << 10u) | (s.y >> 6u)) / 67108864.0 * 2.0 - 1.0) * D(3) * cellsize;
  let cpx = cx + ox;
  let cpy = cy + oy;
  let lx = a.x - cpx;
  let ly = a.y - cpy;
  let shape = i32(D(4));
  let ins = drunken_tiles_inside(shape, lx, ly, radius, D(16), D(5), D(6), D(7), D(11), D(12), D(13), D(14));
  if (ins.x > 0.5) {
    var lds = ins.y;
    var slx = lx / radius;
    var sly = ly / radius;
    if (slx > 1.0) { slx -= 2.0; }
    if (slx < -1.0) { slx += 2.0; }
    if (sly > 1.0) { sly -= 2.0; }
    if (sly < -1.0) { sly += 2.0; }
    let k = 1.0 + D(8);
    let ljx = slx * k * radius;
    let ljy = sly * k * radius;
    var twist_norm = 0.0;
    if (shape == 1) {
      twist_norm = min(1.0, max(abs(lx), abs(ly)) / radius);
    } else if (shape == 4) {
      let c4 = cos(-PI / 4.0);
      let s4 = sin(-PI / 4.0);
      twist_norm = min(1.0, max(abs(c4 * lx - s4 * ly), abs(s4 * lx + c4 * ly)) / radius);
    } else {
      if (lds < 0.0) { lds = lx * lx + ly * ly; }
      twist_norm = min(1.0, sqrt(lds) / radius);
    }
    let theta = D(9) * (1.0 - twist_norm) + D(10) * twist_norm;
    fx = cpx + (cos(theta) * ljx - sin(theta) * ljy);
    fy = cpy + (sin(theta) * ljx + cos(theta) * ljy);
  }
}
v.x += AMOUNT * fx;
v.y += AMOUNT * fy;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "d_spherical",
        params: &[ParamDef::range("d_spher_weight", 0.5, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            if ctx.random() < d[0] {
                let r = amount / (sqr(a.x) + sqr(a.y));
                v.x += a.x * r;
                v.y += a.y * r;
            } else {
                v.x += a.x * amount;
                v.y += a.y * amount;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "if (rnd() < D(0)) {
           let r = AMOUNT / (a.x * a.x + a.y * a.y);
           v.x += a.x * r;
           v.y += a.y * r;
         } else {
           v.x += a.x * AMOUNT;
           v.y += a.y * AMOUNT;
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "duffing",
        params: &[ParamDef::range("a", 2.75, -5.0, 5.0), ParamDef::range("b", 0.2, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let nx = a.y;
            let ny = -d[1] * a.x + d[0] * a.y - a.y * a.y * a.y;
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * a.y;
         v.y += AMOUNT * (-D(1) * a.x + D(0) * a.y - a.y * a.y * a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "dustpoint",
        false,
        |_, ctx, a, v, amount| {
            let p = if ctx.random() < 0.5 { 1.0 } else { -1.0 };
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let w = ctx.random();
            let (x, y) = if w < 0.50 {
                (a.x / r - 1.0, p * a.y / r)
            } else if w < 0.75 {
                (a.x / 3.0, a.y / 3.0)
            } else {
                (a.x / 3.0 + 2.0 / 3.0, a.y / 3.0)
            };
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        "var p = -1.0;
         if (rnd() < 0.5) { p = 1.0; }
         let r = sqrt(a.x * a.x + a.y * a.y);
         let w = rnd();
         var x = a.x / 3.0 + 2.0 / 3.0;
         var y = a.y / 3.0;
         if (w < 0.50) {
           x = a.x / r - 1.0;
           y = p * a.y / r;
         } else if (w < 0.75) {
           x = a.x / 3.0;
         }
         v.x += x * AMOUNT;
         v.y += y * AMOUNT;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "eCollide",
        params: &[ParamDef::int("num", 1.0, 1.0, 10.0), ParamDef::range("a", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [num, a, eCn_pi, pi_eCn, eCa, eCa_eCn]
            let xmax = e_xmax(a.x, a.y);
            let mut nu = e_nu(a.x, xmax);
            let alt = (nu * d[2]) as i32;
            if alt % 2 == 0 {
                nu = alt as f64 * d[3] + fmod(nu + d[5], d[3]);
            } else {
                nu = alt as f64 * d[3] + fmod(nu - d[5], d[3]);
            }
            if a.y <= 0.0 {
                nu = -nu;
            }
            v.x += amount * xmax * nu.cos();
            v.y += amount * (xmax - 1.0).sqrt() * (xmax + 1.0).sqrt() * nu.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let num = limit_value(ftoi(p[0]), 1, i32::MAX) as f64;
            let a = limit_value(p[1], 0.0, 1.0);
            let eca = M_PI * a;
            vec![num, a, num * M_1_PI, M_PI / num, eca, eca / num]
        }),
        wgsl: concat!(
            e_wgsl_head!(),
            "let alt = i32(nu * D(2));
             if (alt % 2 == 0) {
               nu = f32(alt) * D(3) + fmod(nu + D(5), D(3));
             } else {
               nu = f32(alt) * D(3) + fmod(nu - D(5), D(3));
             }
             if (a.y <= 0.0) { nu = -nu; }
             v.x += AMOUNT * xmax * cos(nu);
             v.y += AMOUNT * sqrt(xmax - 1.0) * sqrt(xmax + 1.0) * sin(nu);
             if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        ),
    },
    VarDef {
        name: "eJulia",
        params: &[ParamDef::int("power", 2.0, -10.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [power, sign]
            let power = d[0];
            let mut r2 = a.y * a.y + a.x * a.x;
            let x = if d[1] == 1.0 {
                a.x
            } else {
                r2 = 1.0 / r2;
                a.x * r2
            };
            let tmp = r2 + 1.0;
            let tmp2 = 2.0 * x;
            let mut xmax = (e_sqrt_safe(tmp + tmp2) + e_sqrt_safe(tmp - tmp2)) * 0.5;
            if xmax < 1.0 {
                xmax = 1.0;
            }
            let mut mu = acosh(xmax);
            let mut nu = e_nu(x, xmax);
            if a.y < 0.0 {
                nu = -nu;
            }
            nu = nu / power + M_2PI / power * (ctx.random() * power).floor();
            mu /= power;
            v.x += amount * mu.cosh() * nu.cos();
            v.y += amount * mu.sinh() * nu.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let power = ftoi(p[0]);
            vec![power as f64, if power < 0 { -1.0 } else { 1.0 }]
        }),
        wgsl: "let power = D(0);
         var r2 = a.y * a.y + a.x * a.x;
         var x = a.x;
         if (D(1) != 1.0) {
           r2 = 1.0 / r2;
           x = a.x * r2;
         }
         let tmp = r2 + 1.0;
         let tmp2 = 2.0 * x;
         let xmax = max((sqrt(max(tmp + tmp2, 0.0)) + sqrt(max(tmp - tmp2, 0.0))) * 0.5, 1.0);
         let mu = acosh(xmax) / power;
         var nu = acos(clamp(x / xmax, -1.0, 1.0));
         if (a.y < 0.0) { nu = -nu; }
         nu = nu / power + TWO_PI / power * floor(rnd() * power);
         v.x += AMOUNT * cosh(mu) * cos(nu);
         v.y += AMOUNT * sinh(mu) * sin(nu);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "ellipsoid3D",
        params: &[
            ParamDef::range("dx", 0.5, 0.0, 0.5),
            ParamDef::range("dy", 0.5, 0.0, 0.5),
            ParamDef::range("dz", 0.5, 0.0, 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            let r = vec3(d[0], d[1], d[2]);
            for _ in 0..50 {
                let x = ctx.random() - 0.5;
                let y = ctx.random() - 0.5;
                let z = ctx.random() - 0.5;
                let p = vec3(x, y, z);
                let k0 = length(p / r);
                let k1 = length(p / (r * r));
                if k0 * (k0 - 1.0) / k1 < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| p.iter().map(|&x| limit_value(x, 0.0, 0.5)).collect()),
        wgsl: "let r = vec3<f32>(D(0), D(1), D(2));
         v.hide = 1.0;
         for (var i = 0; i < 50; i++) {
           let p = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
           let k0 = length(p / r);
           let k1 = length(p / (r * r));
           if (k0 * (k0 - 1.0) / k1 < 0.0) {
             v.hide = 0.0;
             v.x += AMOUNT * p.x;
             v.y += AMOUNT * p.y;
             v.z += AMOUNT * p.z;
             break;
           }
         }",
    },
    VarDef {
        name: "elliptic2",
        params: &[
            ParamDef::range("a1", 1.0, 0.0, 1.0),
            ParamDef::range("a2", 1.0, 0.75, 1.25),
            ParamDef::range("a3", 0.0, -1.0, 1.0),
            ParamDef::range("b1", 2.0, 1.0, 2.0),
            ParamDef::range("b2", 1.0, 0.5, 2.0),
            ParamDef::range("c", 0.5, 0.25, 0.75),
            ParamDef::range("d", 1.0, 0.25, 1.75),
            ParamDef::range("e", 0.5, 0.0, 1.0),
            ParamDef::range("f", 1.0, 0.5, 2.0),
            ParamDef::range("g", 1.0, 0.5, 2.0),
            ParamDef::range("h", 2.0, 1.0, 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [a1, a2, a3, b1, b2, c, d, e, f, g, h]
            let sqrt_safe = |x: f64| if x < SMALL_EPSILON { 0.0 } else { x.sqrt() };
            let vv = amount / (M_PI / d[10]);
            let tmp = a.y * a.y + a.x * a.x + d[0];
            let x2 = d[3] * a.x;
            let xmax = d[5] * ((tmp + x2).sqrt() + (tmp - x2).sqrt());
            let aa = a.x / xmax * d[1];
            let bb = sqrt_safe(d[6] - aa * aa) * d[4];
            let ps = -M_PI_2 * d[2];
            v.x += vv * aa.atan2(bb) + ps;
            if ctx.random() < d[7] {
                v.y += vv * (xmax + sqrt_safe(xmax - d[8])).ln();
            } else {
                v.y -= vv * (xmax + sqrt_safe(xmax - d[9])).ln();
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let vv = AMOUNT / (PI / D(10));
         let tmp = a.y * a.y + a.x * a.x + D(0);
         let x2 = D(3) * a.x;
         let xmax = D(5) * (sqrt(tmp + x2) + sqrt(tmp - x2));
         let aa = a.x / xmax * D(1);
         let bb = sqrt(max(D(6) - aa * aa, 0.0)) * D(4);
         v.x += vv * atan2(aa, bb) - HALF_PI * D(2);
         if (rnd() < D(7)) {
           v.y += vv * log(xmax + sqrt(max(xmax - D(8), 0.0)));
         } else {
           v.y -= vv * log(xmax + sqrt(max(xmax - D(9), 0.0)));
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "embroidery",
        params: &[ParamDef::range("scale", 3.0, 0.1, 10.0), ParamDef::range("depth", 0.2, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let cx = a.x * d[0];
            let cy = a.y * d[0];
            let mut m = fmod(cy.floor() + cx.floor(), 2.0);
            if m < 0.0 {
                m += 2.0;
            }
            let phase = m * M_PI;
            let wx = (cx * 6.283185307179586 + phase).sin() * d[1];
            let wy = (cy * 6.283185307179586 + phase).sin() * d[1];
            v.x += amount * (a.x + wx / d[0]);
            v.y += amount * (a.y + wy / d[0]);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let cx = a.x * D(0);
         let cy = a.y * D(0);
         var m = fmod(floor(cy) + floor(cx), 2.0);
         if (m < 0.0) { m += 2.0; }
         let phase = m * PI;
         let wx = sin(cx * TWO_PI + phase) * D(1);
         let wy = sin(cy * TWO_PI + phase) * D(1);
         v.x += AMOUNT * (a.x + wx / D(0));
         v.y += AMOUNT * (a.y + wy / D(0));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "eMod",
        params: &[ParamDef::range("radius", 1.0, 0.0, 5.0), ParamDef::range("distance", 0.0, 0.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (radius, distance) = (d[0], d[1]);
            let xmax = e_xmax(a.x, a.y);
            let mut mu = acosh(xmax);
            let mut nu = e_nu(a.x, xmax);
            if a.y < 0.0 {
                nu = -nu;
            }
            if mu < radius && -mu < radius {
                if nu > 0.0 {
                    mu = fmod(mu + radius + distance * radius, 2.0 * radius) - radius;
                } else {
                    mu = fmod(mu - radius - distance * radius, 2.0 * radius) + radius;
                }
            }
            v.x += amount * mu.cosh() * nu.cos();
            v.y += amount * mu.sinh() * nu.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, f64::MAX), limit_value(p[1], 0.0, 2.0)]),
        wgsl: concat!(
            e_wgsl_head!(),
            "if (a.y < 0.0) { nu = -nu; }
             let radius = D(0);
             if (mu < radius && -mu < radius) {
               if (nu > 0.0) {
                 mu = fmod(mu + radius + D(1) * radius, 2.0 * radius) - radius;
               } else {
                 mu = fmod(mu - radius - D(1) * radius, 2.0 * radius) + radius;
               }
             }
             v.x += AMOUNT * cosh(mu) * cos(nu);
             v.y += AMOUNT * sinh(mu) * sin(nu);
             if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        ),
    },
    VarDef {
        name: "eMotion",
        params: &[ParamDef::range("move", 0.0, -2.0, 2.0), ParamDef::range("rotate", 0.0, -M_PI, M_PI)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let xmax = e_xmax(a.x, a.y);
            let mut mu = acosh(xmax);
            let mut nu = e_nu(a.x, xmax);
            if a.y < 0.0 {
                nu = -nu;
            }
            if nu < 0.0 {
                mu += d[0];
            } else {
                mu -= d[0];
            }
            if mu <= 0.0 {
                mu = -mu;
                nu = -nu;
            }
            nu += d[1];
            v.x += amount * mu.cosh() * nu.cos();
            v.y += amount * mu.sinh() * nu.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: concat!(
            e_wgsl_head!(),
            "if (a.y < 0.0) { nu = -nu; }
             if (nu < 0.0) { mu += D(0); } else { mu -= D(0); }
             if (mu <= 0.0) {
               mu = -mu;
               nu = -nu;
             }
             nu += D(1);
             v.x += AMOUNT * cosh(mu) * cos(nu);
             v.y += AMOUNT * sinh(mu) * sin(nu);
             if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        ),
    },
    VarDef {
        name: "ennepers2",
        params: &[
            ParamDef::range("a", 1.0, -2.0, 2.0),
            ParamDef::range("b", 0.3333, -2.0, 2.0),
            ParamDef::range("c", 0.075, -1.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            let (xx, yy) = (a.x, a.y);
            let r2 = 1.0 / (sqr(xx) + sqr(yy));
            let dxy = sqr(d[0] * xx) - sqr(d[1] * yy);
            v.x += amount * xx * (sqr(d[0]) - dxy * r2 - d[2] * xx.abs().sqrt());
            v.y += amount * yy * (sqr(d[1]) - dxy * r2 - d[2] * yy.abs().sqrt());
            v.z += amount * dxy * 0.5 * r2.sqrt();
        },
        prepare: None,
        wgsl: "let xx = a.x;
         let yy = a.y;
         let r2 = 1.0 / (xx * xx + yy * yy);
         let dxy = sqr(D(0) * xx) - sqr(D(1) * yy);
         v.x += AMOUNT * xx * (D(0) * D(0) - dxy * r2 - D(2) * sqrt(abs(xx)));
         v.y += AMOUNT * yy * (D(1) * D(1) - dxy * r2 - D(2) * sqrt(abs(yy)));
         v.z += AMOUNT * dxy * 0.5 * sqrt(r2);",
    },
    simple(
        "ennepers",
        false,
        |_, ctx, a, v, amount| {
            // Java assigns (=) instead of adding to x/y
            v.x = amount * (a.x - ((sqr(a.x) * a.x) / 3.0)) + a.x * sqr(a.y);
            v.y = amount * (a.y - ((sqr(a.y) * a.y) / 3.0)) + a.y * sqr(a.x);
            ctx.pass_z(a, v, amount);
        },
        "v.x = AMOUNT * (a.x - ((a.x * a.x * a.x) / 3.0)) + a.x * a.y * a.y;
         v.y = AMOUNT * (a.y - ((a.y * a.y * a.y) / 3.0)) + a.y * a.x * a.x;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "epispiral_wf",
        params: &[ParamDef::range("waves", 4.0, 0.0, 20.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let ang = a.x.atan2(a.y);
            let dd = (d[0] * ang).cos();
            if dd == 0.0 {
                return;
            }
            let r = 0.5 / dd;
            v.x += amount * ang.sin() * r;
            v.y += amount * ang.cos() * r;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ang = atan2(a.x, a.y);
         let dd = cos(D(0) * ang);
         if (dd != 0.0) {
           let r = 0.5 / dd;
           v.x += AMOUNT * sin(ang) * r;
           v.y += AMOUNT * cos(ang) * r;
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    },
    VarDef {
        name: "epitrochoid",
        params: &[
            ParamDef::range("r_big", 3.0, 1.0, 5.0),
            ParamDef::range("r_small", 1.0, 0.0, 1.0),
            ParamDef::range("d", 0.5, 0.0, 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let ep_r = java_max(d[1].abs(), 1e-4);
            let theta = a.y.atan2(a.x);
            let ratio = (d[0] + ep_r) / ep_r;
            v.x += amount * ((d[0] + ep_r) * theta.cos() - d[2] * (ratio * theta).cos());
            v.y += amount * ((d[0] + ep_r) * theta.sin() - d[2] * (ratio * theta).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ep_r = max(abs(D(1)), 1e-4);
         let theta = atan2(a.y, a.x);
         let ratio = (D(0) + ep_r) / ep_r;
         v.x += AMOUNT * ((D(0) + ep_r) * cos(theta) - D(2) * cos(ratio * theta));
         v.y += AMOUNT * ((D(0) + ep_r) * sin(theta) - D(2) * sin(ratio * theta));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "ePush",
        params: &[
            ParamDef::range("push", 0.0, -2.0, 2.0),
            ParamDef::range("dist", 1.0, 0.0, 2.0),
            ParamDef::range("rotate", 0.0, -M_PI, M_PI),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let xmax = e_xmax(a.x, a.y);
            let mut mu = acosh(xmax);
            let mut nu = e_nu(a.x, xmax);
            if a.y < 0.0 {
                nu = -nu;
            }
            nu += d[2];
            mu *= d[1];
            mu += d[0];
            v.x += amount * mu.cosh() * nu.cos();
            v.y += amount * mu.sinh() * nu.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: concat!(
            e_wgsl_head!(),
            "if (a.y < 0.0) { nu = -nu; }
             nu += D(2);
             mu = mu * D(1) + D(0);
             v.x += AMOUNT * cosh(mu) * cos(nu);
             v.y += AMOUNT * sinh(mu) * sin(nu);
             if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        ),
    },
    simple(
        "erf3D",
        true,
        |_, _ctx, a, v, amount| {
            v.x += erf(a.x) * amount;
            v.y += erf(a.y) * amount;
            v.z += erf(a.z) * amount;
        },
        "v.x += erf(a.x) * AMOUNT;
         v.y += erf(a.y) * AMOUNT;
         v.z += erf(a.z) * AMOUNT;",
    ),
    simple(
        "erf",
        false,
        |_, ctx, a, v, amount| {
            v.x += erf(a.x) * amount;
            v.y += erf(a.y) * amount;
            ctx.pass_z(a, v, amount);
        },
        "v.x += erf(a.x) * AMOUNT;
         v.y += erf(a.y) * AMOUNT;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "erode",
        params: &[
            ParamDef::range("strength", 0.3, -2.0, 2.0),
            ParamDef::range("scale", 2.0, 0.0, 10.0),
            ParamDef::range("rough", 0.5, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (strength, scale) = (d[0], d[1]);
            let rough = java_max(0.0, java_min(1.0, d[2]));
            let dx0 = strength * (scale * a.x + 0.7 * (scale * a.y).sin()).sin();
            let dy0 = strength * (scale * a.y + 0.7 * (scale * a.x).cos()).cos();
            let dx1 = rough * strength * 0.5 * (scale * 2.1 * a.x - scale * a.y * 0.4).sin();
            let dy1 = rough * strength * 0.5 * (scale * 2.1 * a.y + scale * a.x * 0.4).cos();
            v.x += amount * (a.x + dx0 + dx1);
            v.y += amount * (a.y + dy0 + dy1);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let strength = D(0);
         let scale = D(1);
         let rough = clamp(D(2), 0.0, 1.0);
         let dx0 = strength * sin(scale * a.x + 0.7 * sin(scale * a.y));
         let dy0 = strength * cos(scale * a.y + 0.7 * cos(scale * a.x));
         let dx1 = rough * strength * 0.5 * sin(scale * 2.1 * a.x - scale * a.y * 0.4);
         let dy1 = rough * strength * 0.5 * cos(scale * 2.1 * a.y + scale * a.x * 0.4);
         v.x += AMOUNT * (a.x + dx0 + dx1);
         v.y += AMOUNT * (a.y + dy0 + dy1);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "eRotate",
        params: &[ParamDef::range("rotate", 0.0, -M_PI, M_PI)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let xmax = e_xmax(a.x, a.y);
            let mut nu = e_nu(a.x, xmax);
            if a.y < 0.0 {
                nu = -nu;
            }
            nu = fmod(nu + d[0] + M_PI, M_2PI) - M_PI;
            v.x += amount * xmax * nu.cos();
            v.y += amount * (xmax - 1.0).sqrt() * (xmax + 1.0).sqrt() * nu.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: concat!(
            e_wgsl_head!(),
            "if (a.y < 0.0) { nu = -nu; }
             nu = fmod(nu + D(0) + PI, TWO_PI) - PI;
             v.x += AMOUNT * xmax * cos(nu);
             v.y += AMOUNT * sqrt(xmax - 1.0) * sqrt(xmax + 1.0) * sin(nu);
             if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        ),
    },
    VarDef {
        name: "eScale",
        params: &[ParamDef::range("scale", 1.0, -2.0, 2.0), ParamDef::range("angle", 0.0, -M_PI, M_PI)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (scale, angle) = (d[0], d[1]);
            let xmax = e_xmax(a.x, a.y);
            let mut mu = acosh(xmax);
            let mut nu = e_nu(a.x, xmax);
            if a.y < 0.0 {
                nu = -nu;
            }
            mu *= scale;
            nu = fmod(fmod(scale * (nu + M_PI + angle), M_2PI * scale) - angle - scale * M_PI, M_2PI);
            if nu > M_PI {
                nu -= M_2PI;
            }
            if nu < -M_PI {
                nu += M_2PI;
            }
            v.x += amount * mu.cosh() * nu.cos();
            v.y += amount * mu.sinh() * nu.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: concat!(
            e_wgsl_head!(),
            "if (a.y < 0.0) { nu = -nu; }
             let scale = D(0);
             let angle = D(1);
             mu *= scale;
             nu = fmod(fmod(scale * (nu + PI + angle), TWO_PI * scale) - angle - scale * PI, TWO_PI);
             if (nu > PI) { nu -= TWO_PI; }
             if (nu < -PI) { nu += TWO_PI; }
             v.x += AMOUNT * cosh(mu) * cos(nu);
             v.y += AMOUNT * sinh(mu) * sin(nu);
             if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        ),
    },
    simple(
        "estiq",
        true,
        |_, _ctx, a, v, amount| {
            let e = a.x.exp();
            let abs_v = a.y.hypot(a.z);
            let (s, c) = abs_v.sin_cos();
            let aa = e * s / abs_v;
            v.x += amount * e * c;
            v.y += amount * aa * a.y;
            v.z += amount * aa * a.z;
        },
        "let e = exp(a.x);
         let abs_v = length(vec2<f32>(a.y, a.z));
         let aa = e * sin(abs_v) / abs_v;
         v.x += AMOUNT * e * cos(abs_v);
         v.y += AMOUNT * aa * a.y;
         v.z += AMOUNT * aa * a.z;",
    ),
    VarDef {
        name: "eSwirl",
        params: &[ParamDef::range("in", 1.2, -2.0, 2.0), ParamDef::range("out", 0.2, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let xmax = e_xmax(a.x, a.y);
            let mu = acosh(xmax);
            let mut nu = e_nu(a.x, xmax);
            if a.y < 0.0 {
                nu = -nu;
            }
            nu = nu + mu * d[1] + d[0] / mu;
            v.x += amount * mu.cosh() * nu.cos();
            v.y += amount * mu.sinh() * nu.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: concat!(
            e_wgsl_head!(),
            "if (a.y < 0.0) { nu = -nu; }
             nu = nu + mu * D(1) + D(0) / mu;
             v.x += AMOUNT * cosh(mu) * cos(nu);
             v.y += AMOUNT * sinh(mu) * sin(nu);
             if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        ),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "dc_turbulence", "dc_voronoise", "dc_vortex", "dc_warping", "dc_worley", "dc_ztransl", "deltaA", "deltoid",
        "de_stijl", "devil_warp", "diamond_lattice", "diatom", "dinis_surface_wf", "dipole", "disc3d", "disc3", "dla_wf",
        "domain_fold", "doppler", "dragon_js", "dragon_scale", "drunken_tiles", "d_spherical", "duffing", "dustpoint",
        "eCollide", "eJulia", "ellipsoid3D", "elliptic2", "embroidery", "eMod", "eMotion", "ennepers2", "ennepers",
        "epispiral_wf", "epitrochoid", "ePush", "erf3D", "erf", "erode", "eRotate", "eScale", "estiq", "eSwirl",
    ];

    #[test]
    fn all_names_present_and_have_gpu() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            let def = DEFS.iter().find(|d| d.name == *n).unwrap_or_else(|| panic!("missing {n}"));
            assert!(def.supports_gpu(), "{n} has no wgsl");
            assert!(super::super::find(n).is_some(), "{n} not registered");
        }
    }

    #[test]
    fn defaults_produce_finite_results() {
        let pts = [(0.3, -0.7, 0.2), (1.2, 0.4, -0.5), (-0.9, -1.1, 1.0), (-0.05, 0.6, 0.1), (0.7, 0.45, -0.3)];
        let mut ctx = Ctx::new(42, true);
        for def in DEFS {
            let data = def.prepare_data(&[], &Affine::IDENTITY);
            for &(x, y, z) in &pts {
                for amount in [1.0, 0.5] {
                    for _ in 0..8 {
                        let mut a = Pt { x, y, z, color: 0.4, ..Default::default() };
                        let mut v = Pt::default();
                        (def.apply)(&data, &mut ctx, &mut a, &mut v, amount);
                        assert!(!v.is_bad(), "{} produced non-finite output at ({x},{y},{z})", def.name);
                        assert!(v.color.is_finite(), "{} produced a non-finite color", def.name);
                        assert!(!a.is_bad(), "{} corrupted the affine point", def.name);
                    }
                }
            }
        }
    }

    #[test]
    fn dc_variations_with_color_only_and_gradient_modes() {
        let mut ctx = Ctx::new(3, false);
        for name in ["dc_turbulence", "dc_voronoise", "dc_vortex", "dc_warping", "dc_worley"] {
            let def = DEFS.iter().find(|d| d.name == name).unwrap();
            let n = def.params.len() - 5;
            for gradient in 0..3 {
                let mut params: Vec<f64> = def.params.iter().map(|p| p.default).collect();
                params[n] = 1.0;
                params[n + 1] = gradient as f64;
                let data = def.prepare_data(&params, &Affine::IDENTITY);
                let mut a = Pt { x: 0.21, y: -0.33, ..Default::default() };
                let mut v = Pt::default();
                (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
                assert!(!v.is_bad() && v.color.is_finite(), "{name} gradient {gradient}");
                assert!((v.x - 0.21).abs() < 1e-12 && (v.y + 0.33).abs() < 1e-12, "{name}");
                assert_eq!(v.rgb_color, gradient < 2, "{name}");
            }
        }
    }

    #[test]
    fn drunken_tiles_shapes_are_finite() {
        let def = DEFS.iter().find(|d| d.name == "drunken_tiles").unwrap();
        let mut ctx = Ctx::new(7, false);
        for shape in 0..12 {
            let mut params: Vec<f64> = def.params.iter().map(|p| p.default).collect();
            params[4] = shape as f64;
            let data = def.prepare_data(&params, &Affine::IDENTITY);
            for i in 0..200 {
                let t = i as f64 * 0.137;
                let mut a = Pt { x: t.sin() * 1.3, y: (1.7 * t).cos() * 1.1, ..Default::default() };
                let mut v = Pt::default();
                (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
                assert!(!v.is_bad(), "drunken_tiles shape {shape} produced non-finite output");
            }
        }
    }

    #[test]
    fn java_random_matches_reference() {
        // new java.util.Random(42).nextDouble() == 0.7275636800328681
        let mut s = drunken_tiles_set_seed(42);
        assert!((drunken_tiles_next_double(&mut s) - 0.7275636800328681).abs() < 1e-15);
        // new java.util.Random(0).nextDouble() == 0.730967787376657
        let mut s = drunken_tiles_set_seed(0);
        assert!((drunken_tiles_next_double(&mut s) - 0.730967787376657).abs() < 1e-15);
    }

    #[test]
    fn dragon_js_segments_connect() {
        let def = DEFS.iter().find(|d| d.name == "dragon_js").unwrap();
        let data = def.prepare_data(&[3.0], &Affine::IDENTITY);
        assert_eq!(data[3] as usize, 8);
        // level 3: E, N, W, S, W, N, W, N -> ends at (-2, 2)
        let n = data.len();
        assert!((data[n - 2] + 2.0).abs() < 1e-9 && (data[n - 1] - 2.0).abs() < 1e-9, "{:?}", &data[n - 2..]);
    }

    #[test]
    fn dla_wf_builds_a_cluster() {
        let def = DEFS.iter().find(|d| d.name == "dla_wf").unwrap();
        let data = def.prepare_data(&[200.0, 500.0, 666.0, 10.0, 0.01], &Affine::IDENTITY);
        assert_eq!(data[5] as usize * 2 + 6, data.len());
        assert!(data[5] > 400.0, "cluster too small: {}", data[5]);
    }
}
