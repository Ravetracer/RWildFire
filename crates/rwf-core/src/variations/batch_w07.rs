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

//! Variations ported from the JWildfire sources (batch w07: `dc_*` variations, part 1).
//!
//! Notes valid for the whole batch:
//! * The `DC_BaseFunc` parameters (`ColorOnly, Gradient, scale_z, offset_z, reset_z`) follow the
//!   variation's own parameters (Java `joinArrays(additionalParamNames, paramNames)`).
//! * `seed` / `Seed` / `randomize` parameters only re-seed an unused `java.util.Random` and set
//!   `time` from the wall clock in Java (not reproducible); they are kept for file compatibility
//!   but have no effect here.
//! * `Tools.limitValue` clamps of `setParameter` are applied in `prepare`.

#[allow(unused_imports)]
use super::glsl::*;
use super::noise;
#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

// ---------------------------------------------------------------------------------------------
// DC_BaseFunc helpers
// ---------------------------------------------------------------------------------------------

/// Own parameters followed by the five `DC_BaseFunc` parameters.
macro_rules! dc_params {
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

/// Copies the parameters, applying `Tools.limitValue` to the listed ones and to the
/// `DC_BaseFunc` parameters starting at `n` (pass `usize::MAX` for non-DC variations).
fn dc_prep(p: &[f64], n: usize, lims: &[(usize, f64, f64)]) -> Vec<f64> {
    let mut d = p.to_vec();
    for &(i, lo, hi) in lims {
        d[i] = limit_value(d[i], lo, hi);
    }
    if n != usize::MAX {
        d[n] = limit_value(d[n], 0.0, 1.0);
        d[n + 1] = limit_value(d[n + 1], 0.0, 2.0);
        d[n + 4] = limit_value(d[n + 4], 0.0, 1.0);
    }
    d
}

/// `uV` of `DC_BaseFunc.transform`: the affine point (`ColorOnly == 1`), else random in
/// [-0.5, 0.5) (`full == false`, base class) or [-1, 1) (`full == true`, overridden transforms).
fn dc_uv(color_only: f64, ctx: &mut Ctx, a: &Pt, full: bool) -> Vec2 {
    if color_only as i32 == 1 {
        vec2(a.x, a.y)
    } else {
        let x = ctx.random();
        let y = ctx.random();
        if full { vec2(2.0 * x - 1.0, 2.0 * y - 1.0) } else { vec2(x - 0.5, y - 0.5) }
    }
}

/// Tail of `DC_BaseFunc.transform`; `n` is the index of `ColorOnly` in `d`.
fn dc_out(d: &[f64], n: usize, ctx: &Ctx, v: &mut Pt, amount: f64, uv: Vec2, color: Vec3) {
    dc_output(ctx, v, amount, uv, color, d[n + 1] as i32, d[n + 2], d[n + 3], d[n + 4] as i32);
}

// ---------------------------------------------------------------------------------------------
// dc_fingerprint
// ---------------------------------------------------------------------------------------------

fn dc_fingerprint_hash2(p: Vec2) -> Vec2 {
    let p = vec2(dot(p, vec2(63.31, 127.63)), dot(p, vec2(395.467, 213.799)));
    fract(sin(p) * 43141.59265) * 2.0 - 1.0
}

const DC_FINGERPRINT_TAB: usize = 9;

/// d = [zoom, seed, time, width, dc(5), 50 x (sign, hx, hy), end_x, end_y]
fn dc_fingerprint_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let mut uv = vec2(xp, yp) * d[0];
    let bounds = smoothstep(9.0, 10.0, length(uv * vec2(0.7, 0.5)));
    let mut a = 0.0;
    for i in 0..50 {
        let k = DC_FINGERPRINT_TAB + 3 * i;
        a += d[k] * glsl_atan2(uv.x - d[k + 1], uv.y - d[k + 2]);
    }
    uv = uv + vec2(d[DC_FINGERPRINT_TAB + 150], d[DC_FINGERPRINT_TAB + 151]);
    a += glsl_atan2(uv.y, uv.x);
    let p = (1.0 - bounds) * d[3];
    let s = min(0.3, p);
    let l = length(uv) + 0.319 * a;
    let m = glsl_mod(l, 2.0);
    let v = (1.0 - smoothstep(2.0 - s, 2.0, m)) * smoothstep(p, p + s, m);
    Vec3::splat(v)
}

// ---------------------------------------------------------------------------------------------
// dc_fractaldots
// ---------------------------------------------------------------------------------------------

/// Java computes `circleSize` once from the field defaults (dotsize 400, maxiterations 10) and
/// never updates it; reproduced.
const DC_FRACTALDOTS_CIRCLE: f64 = 400.0 / (3.0 * 1024.0);

fn dc_fractaldots_rot(uv: Vec2, a: f64) -> Vec2 {
    vec2(uv.x * a.cos() - uv.y * a.sin(), uv.y * a.cos() + uv.x * a.sin())
}

/// d = [iterations, dotsize, maxiterations, complexity, pattern, spacing, rotate1, rotate2, zoom, dc(5)]
fn dc_fractaldots_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let iterations = d[0] as i32;
    let mut uv = dc_fractaldots_rot(vec2(xp, yp), d[6]);
    uv = uv * d[8];
    let mut s = d[5];
    let mut i = 0;
    while (i as f64) < d[2] {
        uv = abs(uv) - s;
        uv = uv - d[3];
        uv = dc_fractaldots_rot(uv, d[7]);
        s /= d[4];
        if iterations < i {
            break;
        }
        i += 1;
    }
    let c = if length(uv) > DC_FRACTALDOTS_CIRCLE { 0.0 } else { 1.0 };
    Vec3::splat(c)
}

// ---------------------------------------------------------------------------------------------
// dc_fractcolor
// ---------------------------------------------------------------------------------------------

/// d = [randomize, time, xpar, ypar, iters, zoom, dc(5)]
fn dc_fractcolor_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let u = vec2(xp * d[5], yp * d[5]);
    let blue = 1.5 - d[2] + 0.04 * d[1].sin();
    let mut o = Vec3::from_vec2(u, blue);
    for _ in 0..(d[4] as i32) {
        let tmp = abs(o / dot(o, o) - vec3(1.0, 1.0, d[3] * 0.3)) * vec3(1.3, 1.0, 0.777);
        o = vec3(tmp.x, tmp.z, tmp.y);
    }
    o
}

// ---------------------------------------------------------------------------------------------
// dc_gabornoise
// ---------------------------------------------------------------------------------------------

const DC_GABORNOISE_TAB: usize = 8;

fn dc_gabornoise_hash(p: f64) -> Vec3 {
    fract(sin(vec3(63.31, 395.467, 1.0) * p) * 43141.59265) * 2.0 - 1.0
}

/// d = [seed, time, zoom, dc(5), 100 x (hx, hy)]
fn dc_gabornoise_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let vv = vec2(xp, yp) * d[2];
    let u = vec2(vv.x.abs(), vv.y) * 45.0;
    let mut a = 0.0;
    for i in 0..100 {
        let k = DC_GABORNOISE_TAB + 2 * i;
        a += glsl_atan2(u.x - d[k], u.y - d[k + 1]);
    }
    cos(vec3(0.0, 23.0, 21.0) + a) * 0.6 + 0.6
}

// ---------------------------------------------------------------------------------------------
// dc_glypho
// ---------------------------------------------------------------------------------------------

/// d = [zoom, seed, time, f1, f2, f3, dc(5)]
fn dc_glypho_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let (time, f1, f2, f3) = (d[2], d[3], d[4], d[5]);
    let uv = vec2(xp, yp) * d[0];
    let mut m = 0.0;
    let stp = M_PI / 5.0;
    let mut odd = -1.0;
    let mut n = 1.0;
    while n < 2.5 {
        odd *= -1.0;
        let t = time * odd * 0.3;
        let mut i = 0.0001;
        while i < 2.0 * M_PI {
            let ang = i + n * stp * 0.5 + t;
            let uvi = uv * n + vec2(ang.cos() * 0.4, ang.sin() * 0.4);
            m += smoothstep(f1 * n, 0.0, length(uvi)) * f3;
            i += stp;
        }
        n += 0.5;
    }
    Vec3::splat(step(f2, fract(m * f3)))
}

// ---------------------------------------------------------------------------------------------
// dc_gnarly
// ---------------------------------------------------------------------------------------------

fn dc_gnarly_asinh(x: f64) -> f64 {
    (x + (x * x + 1.0).sqrt()).ln()
}

// ---------------------------------------------------------------------------------------------
// dc_grid3D
// ---------------------------------------------------------------------------------------------

fn dc_grid3d_field(p: Vec3) -> Vec3 {
    let mut p = p * 0.1;
    let mut f = 0.1;
    for _ in 0..3 {
        p = vec3(p.y, p.z, p.x);
        p = abs(fract(p) - 0.5);
        p = p * 2.0;
        f *= 2.0;
    }
    p = p * p;
    sqrt(p + vec3(p.y, p.z, p.x)) / f - 0.05
}

/// d = [zoom, Seed, time, dc(5)]
fn dc_grid3d_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    const MAXITER: i32 = 30;
    let time = d[2];
    let p = vec2(xp, yp) * d[0];
    let mut dir = normalize(vec3(p.x, p.y, 1.0));
    let a = time * 0.021;
    let (sa, ca) = a.sin_cos();
    let mut pos = vec3(0.0, time * 0.1, 0.0);
    dir = dir * mat3(1.0, 0.0, 0.0, 0.0, ca, -sa, 0.0, sa, ca);
    dir = dir * mat3(ca, 0.0, -sa, 0.0, 1.0, 0.0, sa, 0.0, ca);
    let mut color = Vec3::splat(0.0);
    for i in 0..MAXITER {
        let f2 = dc_grid3d_field(pos);
        let f = min(min(f2.x, f2.y), f2.z);
        pos = pos + dir * f;
        color = color + Vec3::splat((MAXITER - i) as f64) / (f2 + 0.1);
    }
    let c3 = Vec3::splat(1.0) - Vec3::splat(1.0) / (Vec3::splat(1.0) + color * (0.09 / (MAXITER * MAXITER) as f64));
    Vec3::splat(c3.x + c3.y + c3.z)
}

// ---------------------------------------------------------------------------------------------
// dc_hexagons
// ---------------------------------------------------------------------------------------------

fn dc_hexagons_hex(mut p: Vec2) -> f64 {
    p.x *= 0.57735 * 2.0;
    p.y += glsl_mod(p.x.floor(), 2.0) * 0.5;
    p = abs(glsl_mod(p, 1.0) - 0.5);
    (max(p.x * 1.5 + p.y, p.y * 2.0) - 1.0).abs()
}

/// d = [shape, seed, time, zoom, dc(5), cos_t, sin_t]
fn dc_hexagons_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let time = d[2];
    let mut p = vec2(xp, yp) * d[3];
    let mut s = (dot(p, p) / -64.0 + time * 4.0).sin();
    s = s.abs().powf(0.5) * sign(s);
    let r = 0.35 + 0.25 * s;
    let (ct, st) = (d[9], d[10]);
    p = p * mat2(ct, -st, st, ct);
    Vec3::splat(smoothstep(r - 0.1, r + 0.1, dc_hexagons_hex(p)))
}

// ---------------------------------------------------------------------------------------------
// dc_hexes_wf (HexesFunc with color)
// ---------------------------------------------------------------------------------------------

const SQRT3: f64 = 1.7320508075688772935;
const HEX_A: f64 = 1.0 / 3.0;
const HEX_B: f64 = SQRT3 / 3.0;
const HEX_C: f64 = -1.0 / 3.0;
const HEX_D: f64 = SQRT3 / 3.0;
const CART_A: f64 = 1.5;
const CART_B: f64 = -1.5;
const CART_C: f64 = SQRT3 / 2.0;
const CART_D: f64 = SQRT3 / 2.0;
const DC_HEXES_CELLS: [(i32, i32); 9] = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 0), (0, 1), (1, -1), (1, 0), (1, 1)];

fn dc_hexes_wf_centre(x: i32, y: i32, s: f64) -> [f64; 2] {
    let (xf, yf) = (x as f64, y as f64);
    [(CART_A * xf + CART_B * yf) * s, (CART_C * xf + CART_D * yf) * s]
}

// ---------------------------------------------------------------------------------------------
// dc_hoshi
// ---------------------------------------------------------------------------------------------

const DC_HOSHI_TAB: usize = 10;

/// d = [seed, time, steps, scale, translate, dc(5), 60 x (cos, sin) of the step rotation]
fn dc_hoshi_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let (time, steps, scale, translate) = (d[1], d[2] as i32, d[3], d[4]);
    let fold = vec2(-0.5, -0.5);
    let mut p = vec2(xp, yp) * 0.182;
    let x = p.y;
    p = abs(glsl_mod(p, 4.0) - 2.0);
    let mut i = steps;
    while i > 0 {
        p = abs(p - fold) + fold;
        p = p * scale - translate;
        let k = DC_HOSHI_TAB + 2 * (i - 1) as usize;
        let (c, s) = (d[k], d[k + 1]);
        p = vec2(p.x * c - p.y * s, p.x * s + p.y * c);
        i -= 1;
    }
    let ii = x * x + glsl_atan2(p.y, p.x) + time * 0.02;
    let mut h = (ii * 4.0).floor() / 8.0 + 1.107;
    h += smoothstep(-0.1, 0.8, glsl_mod(ii * 2.0 / 5.0, 1.0 / 4.0) * 900.0) / 0.010 - 0.5;
    let val = smoothstep(-3.0, 3.0, length(p) * 1.0);
    // hsv(h, 1.0, val)
    let t1 = (vec3(3.0, 2.0, 1.0) + h) / 3.0;
    let t2 = clamp(abs(fract(t1) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
    mix(Vec3::splat(3.1), t2, 1.0) * val
}

// ---------------------------------------------------------------------------------------------
// dc_hyperbolictile
// ---------------------------------------------------------------------------------------------

fn dc_hyperbolictile_tocart(p: Vec2) -> Vec2 {
    vec2(p.x * p.y.cos(), p.x * p.y.sin())
}

/// Uses the `G.atan2` approximation like the Java code.
fn dc_hyperbolictile_topolar(c: Vec2) -> Vec2 {
    vec2((c.x * c.x + c.y * c.y).sqrt(), glsl_atan2(c.y, c.x))
}

fn dc_hyperbolictile_mirror(line: Vec2, point: Vec2) -> Vec2 {
    if line.x < 100.0 {
        let lc = dc_hyperbolictile_tocart(line);
        let cart = dc_hyperbolictile_tocart(point);
        let dx = cart.x - lc.x;
        let dy = cart.y - lc.y;
        let r1 = line.x * line.x - 1.0;
        let r2 = dx * dx + dy * dy;
        let rr = r1 / r2;
        dc_hyperbolictile_topolar(lc + vec2(dx * rr, dy * rr))
    } else {
        vec2(point.x, -point.y + 2.0 * line.y + M_PI)
    }
}

/// d = [zoom, dc(5)]
fn dc_hyperbolictile_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let position = vec2(xp, yp) * d[0];
    let l1 = vec2(1000.0, 0.0);
    let l2 = vec2(1000.0, M_PI / 8.0);
    let l3 = vec2((2f64.sqrt() + 1.0).sqrt(), M_PI / 2.0);
    let mut p = dc_hyperbolictile_topolar(position);
    let in_wedge = |q: Vec2| q.y >= M_PI / 2.0 && q.y < M_PI * 0.75;
    let color = if p.x > 1.0 {
        0.0
    } else {
        let mut g2 = 0;
        let mut k1 = 1;
        for _ in 0..7 {
            if k1 == 0 {
                continue;
            }
            let mut k = 1;
            for _ in 1..8 {
                if k == 0 || in_wedge(p) {
                    k = 0;
                } else {
                    g2 += 1;
                    p = dc_hyperbolictile_mirror(l2, dc_hyperbolictile_mirror(l1, p));
                }
            }
            let p1 = dc_hyperbolictile_mirror(l3, p);
            if in_wedge(p1) {
                k1 = 0;
            }
            p = p1;
        }
        glsl_mod(g2 as f64, 2.0) / 1.0
    };
    Vec3::splat(color)
}

// ---------------------------------------------------------------------------------------------
// dc_inversion
// ---------------------------------------------------------------------------------------------

fn dc_inversion_circle(v_pos: Vec2, v_origin: Vec2, r: f64) -> Vec2 {
    let vop = v_pos - v_origin;
    let mut o = v_origin - vop * r * (r / dot(vop, vop));
    o.x += (o.x * 0.001).sin() / (o.y * 0.001).cos();
    o.y += (o.x * 0.001).sin() * (o.y * 0.001).cos();
    o
}

/// d = [seed, time, iterations, scaleX, scaleY, dc(5), off_x, off_y, base_r, base_g, base_b]
fn dc_inversion_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let mut v = vec2(xp, yp);
    let scale = vec2(d[3], d[4]);
    let off = vec2(d[10], d[11]);
    let mut l = 0.0;
    let mut minl = 10000.0;
    for _ in 0..(d[2] as i32) {
        v.x = v.x.abs();
        v = v * scale + off;
        v = dc_inversion_circle(v, vec2(0.5, 0.5), 0.9);
        l = length(v * v);
        minl = min(l, minl);
    }
    let base = vec3(d[12], d[13], d[14]);
    let mut col = base * (l * l * 11.0);
    minl = (3.0 * minl * (1.0 - minl)).powf(5.0);
    col = col * (minl + 0.14);
    Vec3::splat(1.0) - exp(col * -1.0)
}

// ---------------------------------------------------------------------------------------------
// dc_kaleidocomplex
// ---------------------------------------------------------------------------------------------

fn dc_kaleidocomplex_cmult(a: Vec2, b: Vec2) -> Vec2 {
    vec2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x)
}

/// d = [seed, time, iMax, color, redF, greenF, blueF, dc(5)]
fn dc_kaleidocomplex_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let (time, imax, fc) = (d[1], d[2] as i32, d[3]);
    let mut z = vec4(xp.abs() / 0.125, yp.abs() / 0.125, 0.0, 0.0);
    let mut col = Vec3::splat(0.0);
    let mut ii = -1;
    let mut r = 1.0;
    let rot = vec2(time.sin(), time.cos());
    let mut i = -1;
    while i < imax {
        r *= -1.0;
        ii += 1;
        let mut t1 = dc_kaleidocomplex_cmult(vec2(z.x, z.y), vec2(1.0, 1.0));
        t1 = abs(t1) + (r - 10.5);
        t1 = dc_kaleidocomplex_cmult(t1, rot);
        z.x = t1.x;
        z.y = t1.y;
        z.z = fc * (z.x * z.z - z.y * z.w);
        z.w = fc * (z.y * z.z - z.x * z.w);
        let dist_x = z.x * z.x + z.y * z.y;
        let dist_y = z.z * z.z + z.w * z.w;
        if ii > 0 && ((z.x * z.x).sqrt() < 0.51 || (z.y * z.y).sqrt() < 0.51) {
            col.x = (-(z.x * z.x * z.y).abs()).exp();
            col.y = (-(z.y * z.x * z.y).abs()).exp();
            col.z = (-min(z.x, z.y).abs()).exp();
            break;
        }
        let fi = ii as f64;
        col.x += fc * (-(-z.x / fi + fi / z.x).abs()).exp();
        col.y += fc * (-(-z.y / fi + fi / z.y).abs()).exp();
        col.z += fc * (-((-z.x - z.y) / fi + fi / (z.x + z.y)).abs()).exp();
        if dist_x > 10000000.0 || dist_y > 100000000000.0 {
            break;
        }
        i += 1;
    }
    vec3((col.x * d[4]).cos(), (col.y * d[5]).cos(), (col.z * d[6]).cos())
}

// ---------------------------------------------------------------------------------------------
// dc_kaleidoscopic
// ---------------------------------------------------------------------------------------------

/// d = [seed, time, sides, zoom, p1, radial, dc(5), KA]
fn dc_kaleidoscopic_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let (time, zoom, p1, ka) = (d[1], d[3], d[4], d[11]);
    let mut uv = vec2(xp, yp) * zoom;
    if d[5] as i32 == 1 {
        let angle = (glsl_mod(glsl_atan2(uv.y, uv.x), 2.0 * ka) - ka).abs() + 0.1 * time;
        uv = vec2(angle.cos(), angle.sin()) * length(uv);
    }
    let mut p = Vec3::from_vec2(uv, zoom);
    for _ in 0..44 {
        let t1 = abs(p) / dot(p, p);
        let t0 = t1 - vec3(1.0, 1.02, p1 * 0.4);
        let t2 = vec3(1.3, 0.999, 0.678) * abs(t0);
        p = vec3(t2.x, t2.z, t2.y);
    }
    p
}

// ---------------------------------------------------------------------------------------------
// dc_kaleidotile
// ---------------------------------------------------------------------------------------------

/// Java keeps the color in a per-instance chain (`oldColor`, starting at 0.5, advanced by every
/// call with the branch chosen for that call). The port is stateless: the chain state is
/// reconstructed from this many random earlier steps, starting at 0.5.
const DC_KALEIDOTILE_HISTORY: usize = 16;

// ---------------------------------------------------------------------------------------------
// dc_kaliset2 / dc_kaliset
// ---------------------------------------------------------------------------------------------

/// d = [seed, time, zoom, N, radio, shiftX, shiftY, redF, greenF, blueF, dc(5), C, S, shift_y, zoom2]
fn dc_kaliset2_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let rad2 = d[4];
    let n = d[3] as i32;
    let mut v = vec2(xp, yp) * d[2];
    let (c, s) = (d[15], d[16]);
    let shift = vec2(0.0, d[17]);
    let zoom = d[18];
    let mut rsum = 0.0;
    for _ in 0..n {
        let mut rr = v.x * v.x + v.y * v.y;
        if rr > rad2 {
            rr = rad2 / rr;
            v.x *= rr;
            v.y *= rr;
        }
        rsum = max(rsum, rr);
        v = vec2(c * v.x - s * v.y, s * v.x + c * v.y) * zoom + shift;
    }
    let col = rsum * rsum * (500.0 / n as f64 / rad2);
    vec3((col * d[7]).cos(), (col * d[8]).cos(), (col * d[9]).cos())
}

/// d = [Seed, time, zoom, N, shiftX, shiftY, dc(5), C, S, zoom2]
fn dc_kaliset_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let mut v = vec2(xp, yp) * d[2];
    let (c, s, zoom) = (d[11], d[12], d[13]);
    let shift = vec2(d[4], d[5]);
    let mut rsum = 0.0;
    for _ in 0..(d[3] as i32) {
        let mut rr = v.x * v.x + v.y * v.y;
        if rr > 1.0 {
            rr = 1.0 / rr;
            v.x *= rr;
            v.y *= rr;
        }
        rsum *= 0.99;
        rsum += rr;
        v = vec2(c * v.x - s * v.y, s * v.x + c * v.y) * zoom + shift;
    }
    let col = rsum * 0.5;
    vec3(col.cos(), (col * 2.0).cos(), (col * 4.0).cos())
}

// ---------------------------------------------------------------------------------------------
// dc_layers
// ---------------------------------------------------------------------------------------------

fn dc_layers_snoise(v: Vec3) -> f64 {
    (v.x * 4.0).sin() * (v.y * 4.0).cos() - (v.z * 4.0).sin()
}

fn dc_layers_rotate(v: Vec3, r: Vec2) -> Vec3 {
    let rx = mat3(1.0, 0.0, 0.0, 0.0, r.y.cos(), -r.y.sin(), 0.0, r.y.sin(), r.y.cos());
    let ry = mat3(r.x.cos(), 0.0, -r.x.sin(), 0.0, 1.0, 0.0, r.x.sin(), 0.0, r.x.cos());
    v * rx * ry
}

/// d = [N, Amplitude, seed, time, zoom, dc(5)]
fn dc_layers_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let p = vec2(xp, yp) * d[4];
    let m = vec2(2.0, 1.0) * (d[1] * M_PI);
    let pos = normalize(dc_layers_rotate(Vec3::from_vec2(p, 1.0), m));
    let mut dist = 0.0;
    for k in 1..=(d[0] as i32) {
        let kf = k as f64;
        let mut shell = dc_layers_snoise(pos * kf + vec3(d[3], 0.0, 0.0) * 0.13).abs();
        shell = smoothstep(0.25, 0.2, shell);
        dist = max(dist, shell * (1.0 - kf / 8.0));
    }
    mix(Vec3::splat(1.0), Vec3::splat(0.0), 1.0 - dist)
}

// ---------------------------------------------------------------------------------------------
// dc_mandala
// ---------------------------------------------------------------------------------------------

/// d = [mX, mY, scale, sides, multiply, loops, iR, iG, iB, dc(5)]
fn dc_mandala_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    const RCPI: f64 = 0.318309886183791;
    let (mx, my, scale) = (d[0], d[1], d[2]);
    let (sides, multiply, loops) = (d[3] as i32, d[4] as i32, d[5] as i32);
    let uv = vec2((5.5 - scale) * xp, (5.5 - scale) * yp);
    let k = M_PI / sides as f64;
    let s = glsl_kscope(uv, k);
    let t = glsl_kscope(s, k);
    let v = dot(t, s);
    let mut u = mix(s, t, v.cos());
    if multiply > 0 {
        let t1 = vec2(u.y, u.x);
        let t2 = glsl_mod(t1, multiply as f64);
        let t3 = vec2(-u.x, -u.y);
        let t4 = vec2(u.y, u.x);
        let t5 = t4 + glsl_mod(t2, t3);
        u = vec2(t5.y, t5.x);
    }
    let mut p = Vec3::from_vec2(u, mx * v);
    for l in 0..73 {
        if l > loops {
            break;
        }
        let t2 = abs(p) / dot(p, p) - vec3(1.0, 1.02, my * RCPI);
        let t4 = vec3(1.3, 0.999, 0.678) * abs(t2);
        p = vec3(t4.x, t4.z, t4.y);
    }
    if d[6] as i32 == 1 {
        p.x = 1.0 - p.x;
    }
    if d[7] as i32 == 1 {
        p.y = 1.0 - p.y;
    }
    if d[8] as i32 == 1 {
        p.z = 1.0 - p.z;
    }
    p
}

// ---------------------------------------------------------------------------------------------
// dc_mandbrot
// ---------------------------------------------------------------------------------------------

/// d = [nIters, N, Complexity, seed, time, zoom, red, green, blue, dc(5), scale1, cos_t, sin_t, scale2]
fn dc_mandbrot_rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let mut p = vec2(xp, yp) * d[5];
    p = p * d[14];
    // FRACTALIZE2
    let mut s = 0.5;
    let rot = mat2(d[15], d[16], -d[16], d[15]);
    for _ in 0..(d[1] as i32) {
        p = abs(p) / dot(p, p) - s;
        p = p * rot;
        s *= d[2];
    }
    p = p * d[17];
    p = p + vec2(-1.002029, 0.303864);
    // mandelbrot(p)
    let mut col = Vec3::splat(0.0);
    let mut z = p;
    let mut dd = 0.0;
    for _ in 0..(d[0] as i32) {
        z = vec2(z.x * z.x - z.y * z.y + p.x, 2.0 * z.x * z.y + p.y);
        let l = length(z);
        dd += l + 0.5;
        if l > 2.0 {
            col = vec3((dd * d[6]).sin(), (dd * d[7]).sin(), (dd * d[8]).sin());
            break;
        }
    }
    Vec3::splat(1.0) - col
}

// ---------------------------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "dc_fingerprint",
        params: dc_params!(
            ParamDef::range("zoom", 50.0, 1.0, 100.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("width", 0.8, 0.0, 1.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[4], ctx, a, false);
            let color = dc_fingerprint_rgb(d, uv.x, uv.y);
            dc_out(d, 4, ctx, v, amount, uv, color);
        },
        // The 50 hash steps only depend on `time`: precomputed (exact f64 values for the GPU).
        prepare: Some(|p, _| {
            let mut d = dc_prep(p, 4, &[(0, 1.0, 100.0), (1, 0.0, 10000.0), (3, 0.0, 1.0)]);
            let mut h = vec2((7.0 * d[2]).floor(), 0.0);
            for _ in 0..50 {
                let s = sign(h.x);
                h = dc_fingerprint_hash2(h) * vec2(15.0, 20.0);
                d.extend_from_slice(&[s, h.x, h.y]);
            }
            let e = abs(dc_fingerprint_hash2(h));
            d.extend_from_slice(&[e.x, e.y]);
            d
        }),
        wgsl: "fn dc_fingerprint_rgb(xp: f32, yp: f32, zoom: f32, width: f32, base: u32) -> vec3<f32> {
  var uv = vec2<f32>(xp, yp) * zoom;
  let bounds = smoothstep(9.0, 10.0, length(uv * vec2<f32>(0.7, 0.5)));
  var a = 0.0;
  for (var i = 0u; i < 50u; i++) {
    let k = base + 9u + 3u * i;
    a += prm[k] * glsl_atan2(uv.x - prm[k + 1u], uv.y - prm[k + 2u]);
  }
  uv += vec2<f32>(prm[base + 159u], prm[base + 160u]);
  a += glsl_atan2(uv.y, uv.x);
  let p = (1.0 - bounds) * width;
  let s = min(0.3, p);
  let l = length(uv) + 0.319 * a;
  let m = glsl_mod(l, 2.0);
  let c = (1.0 - glsl_smoothstep(2.0 - s, 2.0, m)) * glsl_smoothstep(p, p + s, m);
  return vec3<f32>(c);
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(4)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_fingerprint_rgb(uv.x, uv.y, D(0), D(3), base);
dc_output(&v, AMOUNT, uv, col, i32(D(5)), D(6), D(7), i32(D(8)));",
    },
    VarDef {
        name: "dc_fractaldots",
        params: dc_params!(
            ParamDef::int("iterations", 9.0, 1.0, 100.0),
            ParamDef::range("dotsize", 400.0, 1.0, 1000.0),
            ParamDef::range("maxiterations", 10.0, 1.0, 50.0),
            ParamDef::range("complexity", 0.001245675, -1.0, 1.0),
            ParamDef::range("pattern", 2.5, 0.5, 5.0),
            ParamDef::range("spacing", 12.0, 0.0, 50.0),
            ParamDef::range("rotate1", 1.5, -6.3, 6.3),
            ParamDef::range("rotate2", 64.0, -100.0, 100.0),
            ParamDef::range("zoom", 64.0, 1.0, 200.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[9], ctx, a, true);
            let color = dc_fractaldots_rgb(d, uv.x, uv.y);
            dc_out(d, 9, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| dc_prep(p, 9, &[(0, 1.0, 100.0)])),
        wgsl: "fn dc_fractaldots_rot(uv: vec2<f32>, a: f32) -> vec2<f32> {
  return vec2<f32>(uv.x * cos(a) - uv.y * sin(a), uv.y * cos(a) + uv.x * sin(a));
}
fn dc_fractaldots_rgb(xp: f32, yp: f32, iterations: i32, maxit: f32, complexity: f32, pattern: f32,
                      spacing: f32, rotate1: f32, rotate2: f32, zoom: f32) -> vec3<f32> {
  var uv = dc_fractaldots_rot(vec2<f32>(xp, yp), rotate1) * zoom;
  var s = spacing;
  for (var i = 0; f32(i) < maxit; i++) {
    uv = abs(uv) - s;
    uv = uv - complexity;
    uv = dc_fractaldots_rot(uv, rotate2);
    s = s / pattern;
    if (iterations < i) { break; }
  }
  var c = 1.0;
  if (length(uv) > 0.130208333333) { c = 0.0; }
  return vec3<f32>(c);
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(9)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_fractaldots_rgb(uv.x, uv.y, i32(D(0)), D(2), D(3), D(4), D(5), D(6), D(7), D(8));
dc_output(&v, AMOUNT, uv, col, i32(D(10)), D(11), D(12), i32(D(13)));",
    },
    VarDef {
        name: "dc_fractcolor",
        params: dc_params!(
            ParamDef::int("randomize", 0.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("xpar", 0.5, 0.0, 1.0),
            ParamDef::range("ypar", 0.5, 0.0, 1.0),
            ParamDef::int("iters", 88.0, 1.0, 200.0),
            ParamDef::range("zoom", 1.0, 0.1, 10.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[6], ctx, a, false);
            let color = dc_fractcolor_rgb(d, uv.x, uv.y);
            dc_out(d, 6, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| dc_prep(p, 6, &[(2, 0.0, 1.0), (3, 0.0, 1.0)])),
        wgsl: "fn dc_fractcolor_rgb(xp: f32, yp: f32, time: f32, xpar: f32, ypar: f32, iters: i32, zoom: f32) -> vec3<f32> {
  let u = vec2<f32>(xp * zoom, yp * zoom);
  let blue = 1.5 - xpar + 0.04 * sin(time);
  var o = vec3<f32>(u, blue);
  for (var i = 0; i < iters; i++) {
    let tmp = abs(o / dot(o, o) - vec3<f32>(1.0, 1.0, ypar * 0.3)) * vec3<f32>(1.3, 1.0, 0.777);
    o = tmp.xzy;
  }
  return o;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(6)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_fractcolor_rgb(uv.x, uv.y, D(1), D(2), D(3), i32(D(4)), D(5));
dc_output(&v, AMOUNT, uv, col, i32(D(7)), D(8), D(9), i32(D(10)));",
    },
    VarDef {
        name: "dc_gabornoise",
        params: dc_params!(
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("zoom", 20.0, 1.0, 100.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[3], ctx, a, false);
            let color = dc_gabornoise_rgb(d, uv.x, uv.y);
            dc_out(d, 3, ctx, v, amount, uv, color);
        },
        // The 100 kernel centres only depend on `time`: precomputed.
        prepare: Some(|p, _| {
            let mut d = dc_prep(p, 3, &[(0, 0.0, 10000.0), (2, 1.0, 100.0)]);
            let time = d[1];
            for i in 0..100 {
                let h = dc_gabornoise_hash(i as f64) * 30.0;
                let m = Mat2::from_vec4(vec4(0.0, 33.0, 11.0, 0.0) + (h.z / 1e2 * time).cos());
                let t = vec2(h.x, h.y) * m;
                d.extend_from_slice(&[t.x, t.y]);
            }
            d
        }),
        wgsl: "fn dc_gabornoise_rgb(xp: f32, yp: f32, zoom: f32, base: u32) -> vec3<f32> {
  let vv = vec2<f32>(xp, yp) * zoom;
  let u = vec2<f32>(abs(vv.x), vv.y) * 45.0;
  var a = 0.0;
  for (var i = 0u; i < 100u; i++) {
    let k = base + 8u + 2u * i;
    a += glsl_atan2(u.x - prm[k], u.y - prm[k + 1u]);
  }
  return cos(vec3<f32>(0.0, 23.0, 21.0) + a) * 0.6 + 0.6;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_gabornoise_rgb(uv.x, uv.y, D(2), base);
dc_output(&v, AMOUNT, uv, col, i32(D(4)), D(5), D(6), i32(D(7)));",
    },
    VarDef {
        name: "dc_glypho",
        params: dc_params!(
            ParamDef::range("zoom", 7.0, 0.1, 50.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("f1", 0.115, 0.0, 1.0),
            ParamDef::range("f2", 0.75, 0.0, 1.0),
            ParamDef::range("f3", 1.5, 0.0, 5.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[6], ctx, a, false);
            let color = dc_glypho_rgb(d, uv.x, uv.y);
            dc_out(d, 6, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| dc_prep(p, 6, &[(0, 0.1, 50.0), (1, 0.0, 10000.0)])),
        // The Java float loops run 3 x 10 times (n = 1, 1.5, 2; i = 0.0001 + k * PI / 5).
        wgsl: "fn dc_glypho_rgb(xp: f32, yp: f32, zoom: f32, time: f32, f1: f32, f2: f32, f3: f32) -> vec3<f32> {
  let uv = vec2<f32>(xp, yp) * zoom;
  var m = 0.0;
  let stp = PI / 5.0;
  var odd = -1.0;
  for (var ni = 0; ni < 3; ni++) {
    let n = 1.0 + 0.5 * f32(ni);
    odd *= -1.0;
    let t = time * odd * 0.3;
    for (var k = 0; k < 10; k++) {
      let ang = 0.0001 + f32(k) * stp + n * stp * 0.5 + t;
      let uvi = uv * n + vec2<f32>(cos(ang) * 0.4, sin(ang) * 0.4);
      m += glsl_smoothstep(f1 * n, 0.0, length(uvi)) * f3;
    }
  }
  return vec3<f32>(step(f2, fract(m * f3)));
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(6)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_glypho_rgb(uv.x, uv.y, D(0), D(2), D(3), D(4), D(5));
dc_output(&v, AMOUNT, uv, col, i32(D(7)), D(8), D(9), i32(D(10)));",
    },
    VarDef {
        name: "dc_gnarly",
        params: &[
            ParamDef::int("mode", 1.0, 0.0, 8.0),
            ParamDef::range("scalex", 0.05, -1.0, 1.0),
            ParamDef::range("scaley", 0.05, -1.0, 1.0),
            ParamDef::range("freqx1", 3.5, -10.0, 10.0),
            ParamDef::range("freqy1", 3.5, -10.0, 10.0),
            ParamDef::range("freqx2", 2.0, -10.0, 10.0),
            ParamDef::range("freqy2", 2.0, -10.0, 10.0),
            ParamDef::range("freqx3", 5.0, -10.0, 10.0),
            ParamDef::range("freqy3", 5.0, -10.0, 10.0),
            ParamDef::int("dc", 1.0, 0.0, 1.0),
            ParamDef::range("color1", 5.0, -10.0, 10.0),
            ParamDef::range("color2", 5.0, -10.0, 10.0),
            ParamDef::range("fmag", 1.0, 0.05, 1.0),
            ParamDef::range("distort", 0.0, -2.0, 2.0),
            ParamDef::range("blur", 5.0, -10.0, 10.0),
            ParamDef::range("scale_z", 0.0, -2.0, 2.0),
            ParamDef::range("offset_z", 0.0, -2.0, 2.0),
            ParamDef::int("reset_z", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let mut x0 = a.x;
            let mut y0 = a.y;
            let blur = d[14];
            if blur > 0.0 {
                let ang = ctx.random() * 2.0 * M_PI;
                let (sina, cosa) = ang.sin_cos();
                // Java sums a per-instance cache of 6 randoms (one replaced per call); stateless
                // here: 6 fresh randoms.
                let mut g = 0.0;
                for _ in 0..6 {
                    g += ctx.random();
                }
                let r = blur * (g - 3.0);
                x0 += r * cosa;
                y0 += r * sina;
            } else if blur < 0.0 {
                x0 += ctx.random() * blur * 2.0 - blur;
                y0 += ctx.random() * blur * 2.0 - blur;
            }
            let (fx1, fy1, fx2, fy2, fx3, fy3) = (d[3], d[4], d[5], d[6], d[7], d[8]);
            let (x1, y1) = match d[0] as i32 {
                1 => (
                    (fx1 * y0 + (fx2 * (y0 + (fx3 * y0).sin())).sin()).cos(),
                    (fy1 * x0 + (fy2 * (x0 + (fy3 * x0).sin())).sin()).cos(),
                ),
                2 => (
                    (fx1 * y0 + (fx2 * (y0 + (fx3 * y0).cos())).sin()).sin(),
                    (fy1 * x0 + (fy2 * (x0 + (fy3 * x0).cos())).sin()).sin(),
                ),
                3 => (
                    (fx1 * y0 + (fx2 * (x0 + (fx3 * y0).sin())).sin()).cos(),
                    (fy1 * x0 + (fy2 * (y0 + (fy3 * x0).sin())).sin()).cos(),
                ),
                4 => (
                    (fx1 * y0 + (fx2 * (y0 + (fx3 * y0).cos())).abs().sqrt().sin()).sin(),
                    (fy1 * x0 + (fy2 * (x0 + (fy3 * x0).cos())).abs().sqrt().sin()).sin(),
                ),
                5 => (
                    (fx1 * y0 + dc_gnarly_asinh(fx2 * (y0 + (fx3 * y0).sin()))).cos(),
                    (fy1 * x0 + dc_gnarly_asinh(fy2 * (x0 + (fy3 * x0).sin()))).cos(),
                ),
                6 => (
                    (fx1 * y0 + (fx2 * (y0 + (fx3 * y0).sin())).tan()).cos(),
                    (fy1 * x0 + (fy2 * (x0 + (fy3 * x0).sin())).tan()).cos(),
                ),
                7 => (
                    (fx1 * y0 + sqr(fx2 * (y0 + (fx3 * y0).cos())).sin()).sin(),
                    (fy1 * x0 + sqr(fy2 * (x0 + (fy3 * x0).cos())).sin()).sin(),
                ),
                8 => (
                    (fx1 * y0 + sqr(fx2 * (x0 + (fx3 * y0).cos())).sin()).sin(),
                    (fy1 * x0 + sqr(fy2 * (y0 + (fy3 * x0).cos())).sin()).sin(),
                ),
                _ => ((fx1 * y0).sin(), (fy1 * x0).sin()),
            };
            let x1 = x1 * d[1];
            let y1 = y1 * d[2];
            let s1 = (sqr(x1) + sqr(y1)).sqrt().ln() * d[13];
            v.x += amount * (x0 + x1) / 2.0;
            v.y += amount * (y0 + y1) / 2.0;
            if d[9] as i32 != 0 {
                v.color = fmod(((x0 + x1 * d[10] + s1).cos() + (y0 + y1 * d[11] + s1).sin()).abs(), d[12]);
            }
            let dz = v.color * d[15] + d[16];
            if d[17] as i32 > 0 {
                v.z = dz;
            } else {
                v.z += dz;
            }
        },
        prepare: Some(|p, _| {
            let mut d = dc_prep(p, usize::MAX, &[(12, 0.05, 1.0)]);
            d[0] = limit_value(ftoi(p[0]), 0, 8) as f64;
            d[9] = limit_value(ftoi(p[9]), 0, 1) as f64;
            d[17] = limit_value(ftoi(p[17]), 0, 1) as f64;
            d
        }),
        wgsl: "fn dc_gnarly_asinh(x: f32) -> f32 { return log(x + sqrt(x * x + 1.0)); }
@body
var x0 = a.x;
var y0 = a.y;
let blur = D(14);
if (blur > 0.0) {
  let ang = rnd() * TWO_PI;
  let r = blur * (rnd() + rnd() + rnd() + rnd() + rnd() + rnd() - 3.0);
  x0 += r * cos(ang);
  y0 += r * sin(ang);
} else if (blur < 0.0) {
  x0 += rnd() * blur * 2.0 - blur;
  y0 += rnd() * blur * 2.0 - blur;
}
let fx1 = D(3); let fy1 = D(4); let fx2 = D(5); let fy2 = D(6); let fx3 = D(7); let fy3 = D(8);
let mode = i32(D(0));
var x1 = 0.0;
var y1 = 0.0;
if (mode == 1) {
  x1 = cos(fx1 * y0 + sin(fx2 * (y0 + sin(fx3 * y0))));
  y1 = cos(fy1 * x0 + sin(fy2 * (x0 + sin(fy3 * x0))));
} else if (mode == 2) {
  x1 = sin(fx1 * y0 + sin(fx2 * (y0 + cos(fx3 * y0))));
  y1 = sin(fy1 * x0 + sin(fy2 * (x0 + cos(fy3 * x0))));
} else if (mode == 3) {
  x1 = cos(fx1 * y0 + sin(fx2 * (x0 + sin(fx3 * y0))));
  y1 = cos(fy1 * x0 + sin(fy2 * (y0 + sin(fy3 * x0))));
} else if (mode == 4) {
  x1 = sin(fx1 * y0 + sin(sqrt(abs(fx2 * (y0 + cos(fx3 * y0))))));
  y1 = sin(fy1 * x0 + sin(sqrt(abs(fy2 * (x0 + cos(fy3 * x0))))));
} else if (mode == 5) {
  x1 = cos(fx1 * y0 + dc_gnarly_asinh(fx2 * (y0 + sin(fx3 * y0))));
  y1 = cos(fy1 * x0 + dc_gnarly_asinh(fy2 * (x0 + sin(fy3 * x0))));
} else if (mode == 6) {
  x1 = cos(fx1 * y0 + tan(fx2 * (y0 + sin(fx3 * y0))));
  y1 = cos(fy1 * x0 + tan(fy2 * (x0 + sin(fy3 * x0))));
} else if (mode == 7) {
  x1 = sin(fx1 * y0 + sin(sqr(fx2 * (y0 + cos(fx3 * y0)))));
  y1 = sin(fy1 * x0 + sin(sqr(fy2 * (x0 + cos(fy3 * x0)))));
} else if (mode == 8) {
  x1 = sin(fx1 * y0 + sin(sqr(fx2 * (x0 + cos(fx3 * y0)))));
  y1 = sin(fy1 * x0 + sin(sqr(fy2 * (y0 + cos(fy3 * x0)))));
} else {
  x1 = sin(fx1 * y0);
  y1 = sin(fy1 * x0);
}
x1 *= D(1);
y1 *= D(2);
let s1 = log(sqrt(x1 * x1 + y1 * y1)) * D(13);
v.x += AMOUNT * (x0 + x1) / 2.0;
v.y += AMOUNT * (y0 + y1) / 2.0;
if (i32(D(9)) != 0) {
  v.color = fmod(abs(cos(x0 + x1 * D(10) + s1) + sin(y0 + y1 * D(11) + s1)), D(12));
}
let dz = v.color * D(15) + D(16);
if (i32(D(17)) > 0) { v.z = dz; } else { v.z += dz; }",
    },
    VarDef {
        name: "dc_grid3D",
        params: dc_params!(
            ParamDef::range("zoom", 1.0, 0.01, 10.0),
            ParamDef::int("Seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 200.0, 1.0, 1000.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[3], ctx, a, true);
            let color = dc_grid3d_rgb(d, uv.x, uv.y);
            dc_out(d, 3, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| dc_prep(p, 3, &[(0, 0.01, 1000.0), (2, 1.0, 1000.0)])),
        wgsl: "fn dc_grid3D_field(p_in: vec3<f32>) -> vec3<f32> {
  var p = p_in * 0.1;
  var f = 0.1;
  for (var i = 0; i < 3; i++) {
    p = p.yzx;
    p = abs(fract(p) - 0.5);
    p = p * 2.0;
    f *= 2.0;
  }
  p = p * p;
  return sqrt(p + p.yzx) / f - 0.05;
}
fn dc_grid3D_rgb(xp: f32, yp: f32, zoom: f32, time: f32) -> vec3<f32> {
  let p = vec2<f32>(xp, yp) * zoom;
  var dir = normalize(vec3<f32>(p.x, p.y, 1.0));
  let a = time * 0.021;
  let ca = cos(a);
  let sa = sin(a);
  var pos = vec3<f32>(0.0, time * 0.1, 0.0);
  dir = dir * mat3x3<f32>(1.0, 0.0, 0.0, 0.0, ca, -sa, 0.0, sa, ca);
  dir = dir * mat3x3<f32>(ca, 0.0, -sa, 0.0, 1.0, 0.0, sa, 0.0, ca);
  var color = vec3<f32>(0.0);
  for (var i = 0; i < 30; i++) {
    let f2 = dc_grid3D_field(pos);
    let f = min(min(f2.x, f2.y), f2.z);
    pos += dir * f;
    color += vec3<f32>(f32(30 - i)) / (f2 + 0.1);
  }
  let c3 = vec3<f32>(1.0) - vec3<f32>(1.0) / (vec3<f32>(1.0) + color * (0.09 / 900.0));
  return vec3<f32>(c3.x + c3.y + c3.z);
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(3)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_grid3D_rgb(uv.x, uv.y, D(0), D(2));
dc_output(&v, AMOUNT, uv, col, i32(D(4)), D(5), D(6), i32(D(7)));",
    },
    VarDef {
        name: "dc_hexagons",
        params: dc_params!(
            ParamDef::int("shape", 0.0, 0.0, 1.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("zoom", 4.0, 0.1, 20.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = if d[4] as i32 == 1 {
                vec2(a.x, a.y)
            } else {
                match d[0] as i32 {
                    0 => {
                        let x = 2.0 * ctx.random() - 1.0;
                        let y = 2.0 * ctx.random() - 1.0;
                        vec2(x, y)
                    }
                    1 => {
                        let mut r = 2.0 * ctx.random() + 2.0 * ctx.random();
                        r = if r > 2.0 { 4.0 - r } else { r };
                        let theta = ctx.random() * M_2PI;
                        vec2(0.5 * r * theta.sin(), 0.5 * r * theta.cos())
                    }
                    _ => vec2(0.0, 0.0),
                }
            };
            let color = dc_hexagons_rgb(d, uv.x, uv.y);
            dc_out(d, 4, ctx, v, amount, uv, color);
        },
        // The rotation angle only depends on `time`: precomputed (avoids pow(0, 0.2) on the GPU).
        prepare: Some(|p, _| {
            let mut d = dc_prep(p, 4, &[(0, 0.0, 1.0)]);
            let s4 = (d[2] * 4.0).sin();
            let t = s4.abs().powf(0.2) * sign(s4) * 10.25;
            d.extend_from_slice(&[t.cos(), t.sin()]);
            d
        }),
        wgsl: "fn dc_hexagons_hex(p_in: vec2<f32>) -> f32 {
  var p = p_in;
  p.x *= 0.57735 * 2.0;
  p.y += glsl_mod(floor(p.x), 2.0) * 0.5;
  p = abs(glsl_mod2(p, vec2<f32>(1.0)) - 0.5);
  return abs(max(p.x * 1.5 + p.y, p.y * 2.0) - 1.0);
}
fn dc_hexagons_rgb(xp: f32, yp: f32, zoom: f32, time: f32, ct: f32, st: f32) -> vec3<f32> {
  var p = vec2<f32>(xp, yp) * zoom;
  var s = sin(dot(p, p) / -64.0 + time * 4.0);
  s = sqrt(abs(s)) * sign(s);
  let r = 0.35 + 0.25 * s;
  p = p * mat2x2<f32>(ct, -st, st, ct);
  return vec3<f32>(smoothstep(r - 0.1, r + 0.1, dc_hexagons_hex(p)));
}
@body
var uv = vec2<f32>(0.0);
if (i32(D(4)) == 1) {
  uv = vec2<f32>(a.x, a.y);
} else if (i32(D(0)) == 0) {
  let x = 2.0 * rnd() - 1.0;
  uv = vec2<f32>(x, 2.0 * rnd() - 1.0);
} else if (i32(D(0)) == 1) {
  var r = 2.0 * rnd() + 2.0 * rnd();
  if (r > 2.0) { r = 4.0 - r; }
  let th = rnd() * TWO_PI;
  uv = vec2<f32>(0.5 * r * sin(th), 0.5 * r * cos(th));
}
let col = dc_hexagons_rgb(uv.x, uv.y, D(3), D(2), D(9), D(10));
dc_output(&v, AMOUNT, uv, col, i32(D(5)), D(6), D(7), i32(D(8)));",
    },
    VarDef {
        name: "dc_hexes_wf",
        params: &[
            ParamDef::range("cellsize", 1.0, 0.01, 4.0),
            ParamDef::range("power", 1.0, -2.0, 4.0),
            ParamDef::range("rotate", 0.166, -1.0, 1.0),
            ParamDef::range("scale", 1.0, -2.0, 2.0),
            ParamDef::range("color_scale", 0.5, -2.0, 2.0),
            ParamDef::range("color_offset", 0.0, -1.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, _, a, v, amount| {
            // d = [cellsize, power, rotate, scale, color_scale, color_offset, rot_sin, rot_cos]
            let s = d[0];
            if s == 0.0 {
                return;
            }
            let (rot_sin, rot_cos) = (d[6], d[7]);
            let u = [a.x, a.y];
            let mut hx = ((HEX_A * u[0] + HEX_B * u[1]) / s).floor() as i32;
            let mut hy = ((HEX_C * u[0] + HEX_D * u[1]) / s).floor() as i32;
            let mut p9 = [[0.0; 2]; 9];
            for (i, (di, dj)) in DC_HEXES_CELLS.iter().enumerate() {
                p9[i] = dc_hexes_wf_centre(hx + di, hy + dj, s);
            }
            let q = noise::closest(&p9, u);
            hx += DC_HEXES_CELLS[q].0;
            hy += DC_HEXES_CELLS[q].1;
            let p = [
                dc_hexes_wf_centre(hx, hy, s),
                dc_hexes_wf_centre(hx, hy + 1, s),
                dc_hexes_wf_centre(hx + 1, hy + 1, s),
                dc_hexes_wf_centre(hx + 1, hy, s),
                dc_hexes_wf_centre(hx, hy - 1, s),
                dc_hexes_wf_centre(hx - 1, hy - 1, s),
                dc_hexes_wf_centre(hx - 1, hy, s),
            ];
            let l1 = noise::voronoi(&p, 0, u);
            let dxo = u[0] - p[0][0];
            let dyo = u[1] - p[0][1];
            let trg_l = (l1 + 1e-100).powf(d[1]) * d[3];
            let mut vx = dxo * rot_cos + dyo * rot_sin;
            let mut vy = -dxo * rot_sin + dyo * rot_cos;
            let l2 = noise::voronoi(&p, 0, [vx + p[0][0], vy + p[0][1]]);
            let l = if l1 > l2 { l1 } else { l2 };
            let r = if l < 0.5 {
                trg_l / l1
            } else if l > 0.8 {
                trg_l / l2
            } else {
                ((trg_l / l1) * (0.8 - l) + (trg_l / l2) * (l - 0.5)) / 0.3
            };
            vx = vx * r + p[0][0];
            vy = vy * r + p[0][1];
            // DCHexesWFFunc.applyCellCalculation
            let c = l * d[4] + d[5];
            v.color = if c < 0.0 {
                0.0
            } else if c > 1.0 {
                1.0
            } else {
                c
            };
            v.x += amount * vx;
            v.y += amount * vy;
        },
        prepare: Some(|p, _| {
            let ang = p[2] * 2.0 * M_PI;
            let mut d = p.to_vec();
            d.extend_from_slice(&[ang.sin(), ang.cos()]);
            d
        }),
        wgsl: "let s = D(0);
if (s != 0.0) {
  let U = vec2<f32>(a.x, a.y);
  var hx = i32(floor((0.333333333333 * U.x + 0.577350269190 * U.y) / s));
  var hy = i32(floor((-0.333333333333 * U.x + 0.577350269190 * U.y) / s));
  var q = 0;
  var d2min = 1.0e30;
  var i = 0;
  for (var di = -1; di < 2; di++) {
    for (var dj = -1; dj < 2; dj++) {
      let cx = f32(hx + di);
      let cy = f32(hy + dj);
      let c = vec2<f32>((1.5 * cx - 1.5 * cy) * s, (0.866025403784 * cx + 0.866025403784 * cy) * s);
      let dd = c - U;
      let d2 = dot(dd, dd);
      if (d2 < d2min) { d2min = d2; q = i; }
      i++;
    }
  }
  hx += q / 3 - 1;
  hy += q % 3 - 1;
  var OX = array<i32, 7>(0, 0, 1, 1, 0, -1, -1);
  var OY = array<i32, 7>(0, 1, 1, 0, -1, -1, 0);
  var P: array<vec2<f32>, 7>;
  for (var k = 0; k < 7; k++) {
    let cx = f32(hx + OX[k]);
    let cy = f32(hy + OY[k]);
    P[k] = vec2<f32>((1.5 * cx - 1.5 * cy) * s, (0.866025403784 * cx + 0.866025403784 * cy) * s);
  }
  let C = P[0];
  var L1 = -1.0e30;
  for (var k = 1; k < 7; k++) {
    let pmq = P[k] - C;
    let pp = dot(pmq, pmq);
    var ratio = 1.0;
    if (pp != 0.0) { ratio = 2.0 * dot(U - C, pmq) / pp; }
    L1 = max(L1, ratio);
  }
  let dxo = U.x - C.x;
  let dyo = U.y - C.y;
  let trg_l = pow(L1 + 1.0e-30, D(1)) * D(3);
  var vx = dxo * D(7) + dyo * D(6);
  var vy = -dxo * D(6) + dyo * D(7);
  let U2 = vec2<f32>(vx, vy) + C;
  var L2 = -1.0e30;
  for (var k = 1; k < 7; k++) {
    let pmq = P[k] - C;
    let pp = dot(pmq, pmq);
    var ratio = 1.0;
    if (pp != 0.0) { ratio = 2.0 * dot(U2 - C, pmq) / pp; }
    L2 = max(L2, ratio);
  }
  let L = max(L1, L2);
  var R = 0.0;
  if (L < 0.5) {
    R = trg_l / L1;
  } else if (L > 0.8) {
    R = trg_l / L2;
  } else {
    R = ((trg_l / L1) * (0.8 - L) + (trg_l / L2) * (L - 0.5)) / 0.3;
  }
  vx = vx * R + C.x;
  vy = vy * R + C.y;
  v.color = clamp(L * D(4) + D(5), 0.0, 1.0);
  v.x += AMOUNT * vx;
  v.y += AMOUNT * vy;
}",
    },
    VarDef {
        name: "dc_hoshi",
        params: dc_params!(
            ParamDef::int("seed", 100000.0, 0.0, 100000.0),
            ParamDef::range("time", 10.0, 0.0, 100.0),
            ParamDef::int("steps", 28.0, 1.0, 60.0),
            ParamDef::range("scale", 1.25, 1.0, 5.0),
            ParamDef::range("translate", 1.5, 0.0, 5.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[5], ctx, a, true);
            let color = dc_hoshi_rgb(d, uv.x, uv.y);
            dc_out(d, 5, ctx, v, amount, uv, color);
        },
        // The per-step rotation angles only depend on `time` and the step: precomputed.
        prepare: Some(|p, _| {
            let mut d = dc_prep(p, 5, &[(2, 1.0, 60.0), (3, 1.0, 5.0), (4, 0.0, 5.0)]);
            let time = d[1];
            for i in 1..=60 {
                let ang = 3.14159
                    / (0.10 + (time * 0.0005 + i as f64 * 0.5000001).sin() * 0.4999 + 0.5 + (10. / time) + time.sin() / 100.);
                d.extend_from_slice(&[ang.cos(), ang.sin()]);
            }
            d
        }),
        wgsl: "fn dc_hoshi_rgb(xp: f32, yp: f32, time: f32, steps: i32, scale: f32, translate: f32, base: u32) -> vec3<f32> {
  let fold = vec2<f32>(-0.5, -0.5);
  var p = vec2<f32>(xp, yp) * 0.182;
  let x = p.y;
  p = abs(glsl_mod2(p, vec2<f32>(4.0)) - 2.0);
  for (var i = steps; i > 0; i--) {
    p = abs(p - fold) + fold;
    p = p * scale - translate;
    let k = base + 10u + 2u * u32(i - 1);
    let c = prm[k];
    let s = prm[k + 1u];
    p = vec2<f32>(p.x * c - p.y * s, p.x * s + p.y * c);
  }
  let ii = x * x + glsl_atan2(p.y, p.x) + time * 0.02;
  var h = floor(ii * 4.0) / 8.0 + 1.107;
  h += smoothstep(-0.1, 0.8, glsl_mod(ii * 2.0 / 5.0, 1.0 / 4.0) * 900.0) / 0.010 - 0.5;
  let val = smoothstep(-3.0, 3.0, length(p));
  let t1 = (vec3<f32>(3.0, 2.0, 1.0) + h) / 3.0;
  let t2 = clamp(abs(fract(t1) * 6.0 - 3.0) - 1.0, vec3<f32>(0.0), vec3<f32>(1.0));
  return mix(vec3<f32>(3.1), t2, 1.0) * val;
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(5)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_hoshi_rgb(uv.x, uv.y, D(1), i32(D(2)), D(3), D(4), base);
dc_output(&v, AMOUNT, uv, col, i32(D(6)), D(7), D(8), i32(D(9)));",
    },
    VarDef {
        name: "dc_hyperbolictile",
        params: dc_params!(ParamDef::range("zoom", 1.0, 0.01, 50.0)),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[1], ctx, a, true);
            let color = dc_hyperbolictile_rgb(d, uv.x, uv.y);
            dc_out(d, 1, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| dc_prep(p, 1, &[(0, 0.01, 50.0)])),
        wgsl: "fn dc_hyperbolictile_tocart(p: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(p.x * cos(p.y), p.x * sin(p.y));
}
fn dc_hyperbolictile_topolar(c: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(sqrt(c.x * c.x + c.y * c.y), glsl_atan2(c.y, c.x));
}
fn dc_hyperbolictile_mirror(line: vec2<f32>, point: vec2<f32>) -> vec2<f32> {
  if (line.x < 100.0) {
    let lc = dc_hyperbolictile_tocart(line);
    let cart = dc_hyperbolictile_tocart(point);
    let dx = cart.x - lc.x;
    let dy = cart.y - lc.y;
    let rr = (line.x * line.x - 1.0) / (dx * dx + dy * dy);
    return dc_hyperbolictile_topolar(lc + vec2<f32>(dx * rr, dy * rr));
  }
  return vec2<f32>(point.x, -point.y + 2.0 * line.y + PI);
}
fn dc_hyperbolictile_rgb(xp: f32, yp: f32, zoom: f32) -> vec3<f32> {
  let l1 = vec2<f32>(1000.0, 0.0);
  let l2 = vec2<f32>(1000.0, PI / 8.0);
  let l3 = vec2<f32>(1.553773974, HALF_PI);
  var p = dc_hyperbolictile_topolar(vec2<f32>(xp, yp) * zoom);
  if (p.x > 1.0) { return vec3<f32>(0.0); }
  var g2 = 0;
  for (var i = 0; i < 7; i++) {
    var k = 1;
    for (var j = 1; j < 8; j++) {
      if (k == 0 || (p.y >= HALF_PI && p.y < PI * 0.75)) {
        k = 0;
      } else {
        g2++;
        p = dc_hyperbolictile_mirror(l2, dc_hyperbolictile_mirror(l1, p));
      }
    }
    let p1 = dc_hyperbolictile_mirror(l3, p);
    p = p1;
    if (p1.y >= HALF_PI && p1.y < PI * 0.75) { break; }
  }
  return vec3<f32>(f32(g2 % 2));
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(1)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_hyperbolictile_rgb(uv.x, uv.y, D(0));
dc_output(&v, AMOUNT, uv, col, i32(D(2)), D(3), D(4), i32(D(5)));",
    },
    VarDef {
        name: "dc_inversion",
        params: dc_params!(
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("iterations", 50.0, 0.0, 100.0),
            ParamDef::range("scaleX", 1.5, 1.1, 3.0),
            ParamDef::range("scaleY", 1.5, 1.1, 3.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[5], ctx, a, true);
            let color = dc_inversion_rgb(d, uv.x, uv.y);
            dc_out(d, 5, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = dc_prep(p, 5, &[(2, 0.0, 100.0), (3, 1.1, 3.0), (4, 1.1, 3.0)]);
            let time = d[1];
            let t = 2.1 + time * 0.025;
            let base = normalize(vec3((t * 1.790).sin(), (t * 1.345).sin(), (t * 1.123).sin()) * 0.5 + 0.5);
            d.extend_from_slice(&[(time * 0.123).sin(), (time * 0.0567).atan(), base.x, base.y, base.z]);
            d
        }),
        wgsl: "fn dc_inversion_rgb(xp: f32, yp: f32, iters: i32, sx: f32, sy: f32, base: u32) -> vec3<f32> {
  var v = vec2<f32>(xp, yp);
  let off = vec2<f32>(prm[base + 10u], prm[base + 11u]);
  var l = 0.0;
  var minl = 10000.0;
  for (var i = 0; i < iters; i++) {
    v.x = abs(v.x);
    v = v * vec2<f32>(sx, sy) + off;
    let vop = v - vec2<f32>(0.5);
    var o = vec2<f32>(0.5) - vop * 0.9 * (0.9 / dot(vop, vop));
    o.x += sin(o.x * 0.001) / cos(o.y * 0.001);
    o.y += sin(o.x * 0.001) * cos(o.y * 0.001);
    v = o;
    l = length(v * v);
    minl = min(l, minl);
  }
  let bc = vec3<f32>(prm[base + 12u], prm[base + 13u], prm[base + 14u]);
  var col = bc * (l * l * 11.0);
  minl = java_pow(3.0 * minl * (1.0 - minl), 5.0);
  col = col * (minl + 0.14);
  return vec3<f32>(1.0) - exp(col * -1.0);
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(5)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_inversion_rgb(uv.x, uv.y, i32(D(2)), D(3), D(4), base);
dc_output(&v, AMOUNT, uv, col, i32(D(6)), D(7), D(8), i32(D(9)));",
    },
    VarDef {
        name: "dc_kaleidocomplex",
        params: dc_params!(
            ParamDef::int("seed", 1000000.0, 0.0, 1000000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("iMax", 12.0, 1.0, 50.0),
            ParamDef::range("color", 2.0, -5.0, 5.0),
            ParamDef::range("redF", 1.0, 0.0, 5.0),
            ParamDef::range("greenF", 1.0, 0.0, 5.0),
            ParamDef::range("blueF", 1.0, 0.0, 5.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[7], ctx, a, false);
            let color = dc_kaleidocomplex_rgb(d, uv.x, uv.y);
            dc_out(d, 7, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| dc_prep(p, 7, &[(2, 1.0, 50.0), (4, 0.0, 5.0), (5, 0.0, 5.0), (6, 0.0, 5.0)])),
        // First pass (ii == 0): Java adds exp(-|x/0 + 0/x|) = 0; skipped on the GPU (no f32 division by 0).
        wgsl: "fn dc_kaleidocomplex_rgb(xp: f32, yp: f32, time: f32, imax: i32, fc: f32, fr: f32, fg: f32, fb: f32) -> vec3<f32> {
  var z = vec4<f32>(abs(xp) / 0.125, abs(yp) / 0.125, 0.0, 0.0);
  var col = vec3<f32>(0.0);
  var ii = -1;
  var r = 1.0;
  let st = sin(time);
  let ct = cos(time);
  for (var i = -1; i < imax; i++) {
    r *= -1.0;
    ii++;
    var t1 = vec2<f32>(z.x - z.y, z.x + z.y);
    t1 = abs(t1) + (r - 10.5);
    t1 = vec2<f32>(t1.x * st - t1.y * ct, t1.x * ct + t1.y * st);
    z.x = t1.x;
    z.y = t1.y;
    z.z = fc * (z.x * z.z - z.y * z.w);
    z.w = fc * (z.y * z.z - z.x * z.w);
    let dist_x = z.x * z.x + z.y * z.y;
    let dist_y = z.z * z.z + z.w * z.w;
    if (ii > 0 && (abs(z.x) < 0.51 || abs(z.y) < 0.51)) {
      col = vec3<f32>(exp(-abs(z.x * z.x * z.y)), exp(-abs(z.y * z.x * z.y)), exp(-abs(min(z.x, z.y))));
      break;
    }
    if (ii > 0) {
      let fi = f32(ii);
      col.x += fc * exp(-abs(-z.x / fi + fi / z.x));
      col.y += fc * exp(-abs(-z.y / fi + fi / z.y));
      col.z += fc * exp(-abs((-z.x - z.y) / fi + fi / (z.x + z.y)));
    }
    if (dist_x > 10000000.0 || dist_y > 100000000000.0) { break; }
  }
  return vec3<f32>(cos(col.x * fr), cos(col.y * fg), cos(col.z * fb));
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(7)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_kaleidocomplex_rgb(uv.x, uv.y, D(1), i32(D(2)), D(3), D(4), D(5), D(6));
dc_output(&v, AMOUNT, uv, col, i32(D(8)), D(9), D(10), i32(D(11)));",
    },
    VarDef {
        name: "dc_kaleidoscopic",
        params: dc_params!(
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("sides", 8.0, 2.0, 20.0),
            ParamDef::range("zoom", 1.0, 0.0, 1.0),
            ParamDef::range("p1", 0.0, 0.0, 1.0),
            ParamDef::int("radial", 1.0, 0.0, 1.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[6], ctx, a, true);
            let color = dc_kaleidoscopic_rgb(d, uv.x, uv.y);
            dc_out(d, 6, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = dc_prep(p, 6, &[(2, 2.0, 20.0), (3, 0.0, 1.0), (4, 0.0, 1.0), (5, 0.0, 1.0)]);
            let sides = d[2] as i32;
            d.push(M_PI / sides as f64);
            d
        }),
        wgsl: "fn dc_kaleidoscopic_rgb(xp: f32, yp: f32, time: f32, ka: f32, zoom: f32, p1: f32, radial: i32) -> vec3<f32> {
  var uv = vec2<f32>(xp, yp) * zoom;
  if (radial == 1) {
    let angle = abs(glsl_mod(glsl_atan2(uv.y, uv.x), 2.0 * ka) - ka) + 0.1 * time;
    uv = vec2<f32>(cos(angle), sin(angle)) * length(uv);
  }
  var p = vec3<f32>(uv, zoom);
  for (var i = 0; i < 44; i++) {
    let t1 = abs(p) / dot(p, p);
    let t0 = t1 - vec3<f32>(1.0, 1.02, p1 * 0.4);
    let t2 = vec3<f32>(1.3, 0.999, 0.678) * abs(t0);
    p = t2.xzy;
  }
  return p;
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(6)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_kaleidoscopic_rgb(uv.x, uv.y, D(1), D(11), D(3), D(4), i32(D(5)));
dc_output(&v, AMOUNT, uv, col, i32(D(7)), D(8), D(9), i32(D(10)));",
    },
    VarDef {
        name: "dc_kaleidotile",
        params: &[
            ParamDef::range("pull", -1.5, -5.0, 5.0),
            ParamDef::range("rotate", 0.0, -2.0, 2.0),
            ParamDef::range("line_up", 1.0, -2.0, 2.0),
            ParamDef::range("x", 0.0, -2.0, 2.0),
            ParamDef::range("y", 0.0, -2.0, 2.0),
            ParamDef::range("color1", 0.0, 0.0, 1.0),
            ParamDef::range("speed1", 0.0, -1.0, 1.0),
            ParamDef::range("color2", 1.0, 0.0, 1.0),
            ParamDef::range("speed2", 0.0, -1.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [pull, rotate, line_up, x, y, color1, speed1, color2, speed2, c11, c12, c21, c22]
            let (c11, c12, c21, c22) = (d[9], d[10], d[11], d[12]);
            let first = ctx.random() < 0.5;
            let mut old_color = 0.5;
            for _ in 0..DC_KALEIDOTILE_HISTORY {
                old_color = if ctx.random() < 0.5 { old_color * c11 + c12 } else { old_color * c21 + c22 };
            }
            let (c45, s45) = (45f64.cos(), 45f64.sin());
            let xin = a.x * 0.831;
            let yin = a.y * 0.831;
            let new_color;
            if first {
                let (q, w, e, r, t) = (d[0], d[1], d[2], d[3], d[4]);
                v.x += ((w * xin) * c45 - yin * s45 + e) + r;
                if yin > 0.0 {
                    v.y += ((w * yin) * c45 + xin * s45 + q + e) + t;
                } else {
                    v.y += (w * yin) * c45 + xin * s45 - q - e;
                }
                new_color = old_color * c11 + c12;
            } else {
                let (q, w, e, r, t) = (-d[0], -d[1], -d[2], -d[3], -d[4]);
                v.x += ((w * xin) * c45 - yin * s45 + e) + r;
                if yin > 0.0 {
                    v.y += ((w * yin) * c45 + xin * s45 + q + e) + t;
                } else {
                    v.y += (w * yin) * c45 + xin * s45 - q - e;
                }
                new_color = old_color * c21 + c22;
            }
            v.color = fmod(new_color, 1.0);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d.extend_from_slice(&[
                (1.0 + p[6]) / 2.0,
                p[5] * (1.0 - p[6]) / 2.0,
                (1.0 + p[8]) / 2.0,
                p[7] * (1.0 - p[8]) / 2.0,
            ]);
            d
        }),
        wgsl: "let first = rnd() < 0.5;
var old_color = 0.5;
for (var i = 0; i < 16; i++) {
  if (rnd() < 0.5) { old_color = old_color * D(9) + D(10); } else { old_color = old_color * D(11) + D(12); }
}
let c45 = 0.525321988818;
let s45 = 0.850903524534;
let xin = a.x * 0.831;
let yin = a.y * 0.831;
var sg = -1.0;
var new_color = old_color * D(11) + D(12);
if (first) {
  sg = 1.0;
  new_color = old_color * D(9) + D(10);
}
let q = sg * D(0);
let w = sg * D(1);
let e = sg * D(2);
v.x += ((w * xin) * c45 - yin * s45 + e) + sg * D(3);
if (yin > 0.0) {
  v.y += ((w * yin) * c45 + xin * s45 + q + e) + sg * D(4);
} else {
  v.y += (w * yin) * c45 + xin * s45 - q - e;
}
v.color = fmod(new_color, 1.0);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "dc_kaliset2",
        params: dc_params!(
            ParamDef::int("seed", 5000.0, 0.0, 10000.0),
            ParamDef::range("time", 85.5, 1.0, 1000.0),
            ParamDef::range("zoom", 20.0, 1.0, 1000.0),
            ParamDef::int("N", 100.0, 1.0, 100.0),
            ParamDef::range("radio", 1.0, 0.1, 10.0),
            ParamDef::range("shiftX", -0.356, -1.0, 1.0),
            ParamDef::range("shiftY", 0.686, -1.0, 1.0),
            ParamDef::range("redF", 1.8, 0.0, 5.0),
            ParamDef::range("greenF", 1.9, 0.0, 5.0),
            ParamDef::range("blueF", 2.2, 0.0, 5.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[10], ctx, a, true);
            let color = dc_kaliset2_rgb(d, uv.x, uv.y);
            dc_out(d, 10, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = dc_prep(
                p,
                10,
                &[
                    (1, 1.0, 1000.0),
                    (2, 1.0, 1000.0),
                    (3, 1.0, 100.0),
                    (4, 0.1, 10.0),
                    (5, -1.0, 1.0),
                    (6, -1.0, 1.0),
                    (7, 0.0, 5.0),
                    (8, 0.0, 5.0),
                    (9, 0.0, 5.0),
                ],
            );
            let angle = 6.2831853070 * d[5];
            let shift_y = 2.0 + 2. * (0.03 * d[1]).sin();
            let zoom = 2.0 + 5.0 * d[6];
            d.extend_from_slice(&[angle.cos(), angle.sin(), shift_y, zoom]);
            d
        }),
        wgsl: "fn dc_kaliset2_rgb(xp: f32, yp: f32, zoom0: f32, n: i32, rad2: f32, c: f32, s: f32, shift_y: f32,
                   zoom: f32, fr: f32, fg: f32, fb: f32) -> vec3<f32> {
  var v = vec2<f32>(xp, yp) * zoom0;
  var rsum = 0.0;
  for (var i = 0; i < n; i++) {
    var rr = v.x * v.x + v.y * v.y;
    if (rr > rad2) {
      rr = rad2 / rr;
      v = v * rr;
    }
    rsum = max(rsum, rr);
    v = vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y) * zoom + vec2<f32>(0.0, shift_y);
  }
  let col = rsum * rsum * (500.0 / f32(n) / rad2);
  return vec3<f32>(cos(col * fr), cos(col * fg), cos(col * fb));
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(10)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_kaliset2_rgb(uv.x, uv.y, D(2), i32(D(3)), D(4), D(15), D(16), D(17), D(18), D(7), D(8), D(9));
dc_output(&v, AMOUNT, uv, col, i32(D(11)), D(12), D(13), i32(D(14)));",
    },
    VarDef {
        name: "dc_kaliset",
        params: dc_params!(
            ParamDef::int("Seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 200.0, 1.0, 1000.0),
            ParamDef::range("zoom", 0.5, 0.01, 10.0),
            ParamDef::int("N", 60.0, 1.0, 100.0),
            ParamDef::range("shiftX", -0.22, -5.0, 5.0),
            ParamDef::range("shiftY", -0.21, -5.0, 5.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[6], ctx, a, true);
            let color = dc_kaliset_rgb(d, uv.x, uv.y);
            dc_out(d, 6, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = dc_prep(p, 6, &[(1, 1.0, 1000.0), (2, 0.01, 1000.0), (3, 1.0, 100.0), (4, -5.0, 5.0), (5, -5.0, 5.0)]);
            let time = d[1];
            let pi2 = 3.6 * 2.0;
            d.extend_from_slice(&[(time / 603. * pi2).cos(), (time / 407.4 * pi2).sin(), time / 184.0 + 1.0]);
            d
        }),
        wgsl: "fn dc_kaliset_rgb(xp: f32, yp: f32, zoom0: f32, n: i32, shift: vec2<f32>, c: f32, s: f32, zoom: f32) -> vec3<f32> {
  var v = vec2<f32>(xp, yp) * zoom0;
  var rsum = 0.0;
  for (var i = 0; i < n; i++) {
    var rr = v.x * v.x + v.y * v.y;
    if (rr > 1.0) {
      rr = 1.0 / rr;
      v = v * rr;
    }
    rsum *= 0.99;
    rsum += rr;
    v = vec2<f32>(c * v.x - s * v.y, s * v.x + c * v.y) * zoom + shift;
  }
  let col = rsum * 0.5;
  return vec3<f32>(cos(col), cos(col * 2.0), cos(col * 4.0));
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(6)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_kaliset_rgb(uv.x, uv.y, D(2), i32(D(3)), vec2<f32>(D(4), D(5)), D(11), D(12), D(13));
dc_output(&v, AMOUNT, uv, col, i32(D(7)), D(8), D(9), i32(D(10)));",
    },
    VarDef {
        name: "dc_layers",
        params: dc_params!(
            ParamDef::int("N", 8.0, 1.0, 10.0),
            ParamDef::range("Amplitude", 0.995, 0.0, 10.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("zoom", 8.0, 0.1, 50.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[5], ctx, a, true);
            let color = dc_layers_rgb(d, uv.x, uv.y);
            dc_out(d, 5, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| dc_prep(p, 5, &[(0, 1.0, 10.0), (1, 0.0, 10.0)])),
        wgsl: "fn dc_layers_rgb(xp: f32, yp: f32, n: i32, amp: f32, time: f32, zoom: f32) -> vec3<f32> {
  let p = vec2<f32>(xp, yp) * zoom;
  let m = vec2<f32>(2.0, 1.0) * (amp * PI);
  let rx = mat3x3<f32>(1.0, 0.0, 0.0, 0.0, cos(m.y), -sin(m.y), 0.0, sin(m.y), cos(m.y));
  let ry = mat3x3<f32>(cos(m.x), 0.0, -sin(m.x), 0.0, 1.0, 0.0, sin(m.x), 0.0, cos(m.x));
  let pos = normalize(vec3<f32>(p, 1.0) * rx * ry);
  var dist = 0.0;
  for (var k = 1; k <= n; k++) {
    let q = pos * f32(k) + vec3<f32>(time, 0.0, 0.0) * 0.13;
    var shell = abs(sin(q.x * 4.0) * cos(q.y * 4.0) - sin(q.z * 4.0));
    shell = glsl_smoothstep(0.25, 0.2, shell);
    dist = max(dist, shell * (1.0 - f32(k) / 8.0));
  }
  return mix(vec3<f32>(1.0), vec3<f32>(0.0), 1.0 - dist);
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(5)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_layers_rgb(uv.x, uv.y, i32(D(0)), D(1), D(3), D(4));
dc_output(&v, AMOUNT, uv, col, i32(D(6)), D(7), D(8), i32(D(9)));",
    },
    VarDef {
        name: "dc_linear",
        params: &[
            ParamDef::range("offset", 0.0, -2.0, 2.0),
            ParamDef::range("angle", 0.3, -3.2, 3.2),
            ParamDef::range("scale", 0.8, -2.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [offset, angle, scale, ldcs, sin(angle), cos(angle)]
            v.x += amount * a.x;
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
            v.color = fmod((0.5 * (d[3] * (d[5] * v.x + d[4] * v.y + d[0]) + 1.0)).abs(), 1.0);
        },
        prepare: Some(|p, _| {
            let ldcs = 1.0 / if p[2] == 0.0 { 10E-6 } else { p[2] };
            vec![p[0], p[1], p[2], ldcs, p[1].sin(), p[1].cos()]
        }),
        wgsl: "v.x += AMOUNT * a.x;
v.y += AMOUNT * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
v.color = fmod(abs(0.5 * (D(3) * (D(5) * v.x + D(4) * v.y + D(0)) + 1.0)), 1.0);",
    },
    VarDef {
        name: "dc_mandala",
        params: dc_params!(
            ParamDef::range("mX", 0.025, -1.0, 1.0),
            ParamDef::range("mY", -0.001245675, -1.0, 1.0),
            ParamDef::range("scale", 2.0, 0.0, 5.0),
            ParamDef::int("sides", 12.0, 1.0, 50.0),
            ParamDef::int("multiply", 1.0, 0.0, 10.0),
            ParamDef::int("loops", 64.0, 0.0, 72.0),
            ParamDef::int("iR", 0.0, 0.0, 1.0),
            ParamDef::int("iG", 0.0, 0.0, 1.0),
            ParamDef::int("iB", 1.0, 0.0, 1.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[9], ctx, a, true);
            let color = dc_mandala_rgb(d, uv.x, uv.y);
            dc_out(d, 9, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| dc_prep(p, 9, &[(6, 0.0, 1.0), (7, 0.0, 1.0), (8, 0.0, 1.0)])),
        wgsl: "fn dc_mandala_rgb(xp: f32, yp: f32, mx: f32, my: f32, scale: f32, sides: i32, mult: i32, loops: i32,
                  ir: i32, ig: i32, ib: i32) -> vec3<f32> {
  let uv = vec2<f32>((5.5 - scale) * xp, (5.5 - scale) * yp);
  let k = PI / f32(sides);
  let s = glsl_kscope(uv, k);
  let t = glsl_kscope(s, k);
  let v = dot(t, s);
  var u = mix(s, t, cos(v));
  if (mult > 0) {
    let t2 = glsl_mod2(u.yx, vec2<f32>(f32(mult)));
    let t5 = u.yx + glsl_mod2(t2, -u);
    u = t5.yx;
  }
  var p = vec3<f32>(u, mx * v);
  for (var l = 0; l < 73; l++) {
    if (l > loops) { break; }
    let t2 = abs(p) / dot(p, p) - vec3<f32>(1.0, 1.02, my * 0.318309886183791);
    let t4 = vec3<f32>(1.3, 0.999, 0.678) * abs(t2);
    p = t4.xzy;
  }
  if (ir == 1) { p.x = 1.0 - p.x; }
  if (ig == 1) { p.y = 1.0 - p.y; }
  if (ib == 1) { p.z = 1.0 - p.z; }
  return p;
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(9)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_mandala_rgb(uv.x, uv.y, D(0), D(1), D(2), i32(D(3)), i32(D(4)), i32(D(5)), i32(D(6)), i32(D(7)), i32(D(8)));
dc_output(&v, AMOUNT, uv, col, i32(D(10)), D(11), D(12), i32(D(13)));",
    },
    VarDef {
        name: "dc_mandbrot",
        params: dc_params!(
            ParamDef::int("nIters", 250.0, 10.0, 1000.0),
            ParamDef::int("N", 8.0, 2.0, 10.0),
            ParamDef::range("Complexity", 0.995, 0.0, 1.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("zoom", 4.0, 0.1, 20.0),
            ParamDef::range("red", 0.0314, 0.0, 1.0),
            ParamDef::range("green", 0.02, 0.0, 1.0),
            ParamDef::range("blue", 0.011, 0.0, 1.0),
        ),
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = dc_uv(d[9], ctx, a, false);
            let color = dc_mandbrot_rgb(d, uv.x, uv.y);
            dc_out(d, 9, ctx, v, amount, uv, color);
        },
        prepare: Some(|p, _| {
            let mut d = dc_prep(
                p,
                9,
                &[(0, 10.0, 1000.0), (1, 2.0, 10.0), (2, 0.0, 1.0), (6, 0.0, 1.0), (7, 0.0, 1.0), (8, 0.0, 1.0)],
            );
            let t = d[4] / 65. * 10.;
            let f = (t * 0.10 + 99.0).sin() * 0.5 + 0.5;
            d.extend_from_slice(&[fract(t * 0.0001) * 100. + 1., t.cos(), t.sin(), 1.5f64.powf(f * (-31.0))]);
            d
        }),
        wgsl: "fn dc_mandbrot_rgb(xp: f32, yp: f32, zoom: f32, n: i32, cplx: f32, niters: i32, rgb: vec3<f32>,
                   sc1: f32, cs: f32, sn: f32, sc2: f32) -> vec3<f32> {
  var p = vec2<f32>(xp, yp) * zoom * sc1;
  var s = 0.5;
  let rot = mat2x2<f32>(cs, sn, -sn, cs);
  for (var i = 0; i < n; i++) {
    p = abs(p) / dot(p, p) - s;
    p = p * rot;
    s *= cplx;
  }
  p = p * sc2 + vec2<f32>(-1.002029, 0.303864);
  var col = vec3<f32>(0.0);
  var z = p;
  var d = 0.0;
  for (var i = 0; i < niters; i++) {
    z = vec2<f32>(z.x * z.x - z.y * z.y + p.x, 2.0 * z.x * z.y + p.y);
    let l = length(z);
    d += l + 0.5;
    if (l > 2.0) {
      col = sin(rgb * d);
      break;
    }
  }
  return vec3<f32>(1.0) - col;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(9)) == 1) { uv = vec2<f32>(a.x, a.y); }
let col = dc_mandbrot_rgb(uv.x, uv.y, D(5), i32(D(1)), D(2), i32(D(0)), vec3<f32>(D(6), D(7), D(8)),
                          D(14), D(15), D(16), D(17));
dc_output(&v, AMOUNT, uv, col, i32(D(10)), D(11), D(12), i32(D(13)));",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "dc_fingerprint",
        "dc_fractaldots",
        "dc_fractcolor",
        "dc_gabornoise",
        "dc_glypho",
        "dc_gnarly",
        "dc_grid3D",
        "dc_hexagons",
        "dc_hexes_wf",
        "dc_hoshi",
        "dc_hyperbolictile",
        "dc_inversion",
        "dc_kaleidocomplex",
        "dc_kaleidoscopic",
        "dc_kaleidotile",
        "dc_kaliset2",
        "dc_kaliset",
        "dc_layers",
        "dc_linear",
        "dc_mandala",
        "dc_mandbrot",
    ];

    const POINTS: &[(f64, f64, f64)] = &[
        (0.3, 0.2, 0.1),
        (-0.7, 0.5, -0.2),
        (1.3, -0.9, 0.4),
        (0.05, -0.02, 0.0),
        (-2.1, -1.7, 1.1),
        (0.6, 0.0, -0.5),
    ];

    fn run(def: &VarDef, data: &[f64], ctx: &mut Ctx, p: (f64, f64, f64)) -> (Pt, Pt) {
        let mut a = Pt { x: p.0, y: p.1, z: p.2, color: 0.5, ..Default::default() };
        let mut v = Pt { x: 0.1, y: -0.2, z: 0.05, color: 0.5, ..Default::default() };
        (def.apply)(data, ctx, &mut a, &mut v, 0.8);
        (a, v)
    }

    fn find(name: &str) -> &'static VarDef {
        DEFS.iter().find(|d| d.name == name).unwrap()
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
        for (i, c) in ctx.palette.iter_mut().enumerate() {
            *c = [i as u8, (255 - i) as u8, (i / 2) as u8];
        }
        for def in DEFS {
            let data = def.prepare_data(&[], &Affine::IDENTITY);
            assert!(def.supports_gpu(), "{} has no WGSL", def.name);
            for &p in POINTS {
                for _ in 0..20 {
                    let (a, v) = run(def, &data, &mut ctx, p);
                    assert!(
                        !a.is_bad() && !v.is_bad() && v.color.is_finite(),
                        "{} at {:?}: a={:?} v={:?}",
                        def.name,
                        p,
                        a,
                        v
                    );
                    assert!((0.0..=255.0).contains(&v.red) && (0.0..=255.0).contains(&v.blue), "{}", def.name);
                }
            }
        }
    }

    #[test]
    fn color_only_and_gradient_modes_are_finite() {
        let mut ctx = Ctx::new(5, false);
        for def in DEFS.iter().filter(|d| d.params.iter().any(|p| p.name == "ColorOnly")) {
            let n = def.params.iter().position(|p| p.name == "ColorOnly").unwrap();
            for gradient in [0.0, 1.0, 2.0] {
                let mut params: Vec<f64> = def.params.iter().map(|p| p.default).collect();
                params[n] = 1.0;
                params[n + 1] = gradient;
                let data = def.prepare_data(&params, &Affine::IDENTITY);
                for &p in POINTS {
                    let (_, v) = run(def, &data, &mut ctx, p);
                    // ColorOnly: the affine point is passed through (x/y += amount * uv)
                    assert!((v.x - (0.1 + 0.8 * p.0)).abs() < 1e-12, "{}", def.name);
                    assert!(v.color.is_finite() && !v.is_bad(), "{} gradient {gradient}", def.name);
                    assert_eq!(v.rgb_color, gradient < 2.0, "{}", def.name);
                }
            }
        }
    }

    #[test]
    fn gnarly_modes_are_finite() {
        let def = find("dc_gnarly");
        let mut ctx = Ctx::new(9, false);
        for mode in 0..=8 {
            for blur in [-1.0, 0.0, 5.0] {
                let mut params: Vec<f64> = def.params.iter().map(|p| p.default).collect();
                params[0] = mode as f64;
                params[14] = blur;
                params[13] = 0.3;
                let data = def.prepare_data(&params, &Affine::IDENTITY);
                for &p in POINTS {
                    let (_, v) = run(def, &data, &mut ctx, p);
                    assert!(!v.is_bad() && v.color.is_finite(), "mode {mode} blur {blur}: {v:?}");
                }
            }
        }
    }

    #[test]
    fn hexes_wf_identity_like() {
        // power 1, scale 1, no rotation: hexes maps a point onto itself
        let def = find("dc_hexes_wf");
        let data = def.prepare_data(&[1.0, 1.0, 0.0, 1.0, 0.5, 0.0], &Affine::IDENTITY);
        let mut ctx = Ctx::new(1, false);
        let mut a = Pt { x: 0.37, y: -0.21, ..Default::default() };
        let mut v = Pt::default();
        (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
        assert!((v.x - 0.37).abs() < 1e-9 && (v.y + 0.21).abs() < 1e-9, "{v:?}");
        assert!((0.0..=1.0).contains(&v.color));
    }

    #[test]
    fn linear_color() {
        let def = find("dc_linear");
        let data = def.prepare_data(&[], &Affine::IDENTITY);
        let mut ctx = Ctx::new(1, false);
        let mut a = Pt { x: 0.5, y: 0.25, ..Default::default() };
        let mut v = Pt::default();
        (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
        let (s, c) = 0.3f64.sin_cos();
        let expect = fmod((0.5 * ((1.0 / 0.8) * (c * 0.5 + s * 0.25) + 1.0)).abs(), 1.0);
        assert!((v.color - expect).abs() < 1e-12);
    }

    #[test]
    fn glypho_float_loop_count() {
        // The GPU snippet assumes 10 inner iterations of the Java float loop.
        let stp = M_PI / 5.0;
        let mut i = 0.0001;
        let mut count = 0;
        while i < 2.0 * M_PI {
            count += 1;
            i += stp;
        }
        assert_eq!(count, 10);
    }

    #[test]
    fn kaleidotile_color_in_range() {
        let def = find("dc_kaleidotile");
        let data = def.prepare_data(&[], &Affine::IDENTITY);
        let mut ctx = Ctx::new(3, false);
        let mut sum = 0.0;
        for _ in 0..1000 {
            let (_, v) = run(def, &data, &mut ctx, (0.3, 0.4, 0.0));
            assert!((0.0..1.0).contains(&v.color));
            sum += v.color;
        }
        // default speeds: the color chain is uniform-ish on [0, 1)
        assert!((sum / 1000.0 - 0.5).abs() < 0.1);
    }
}
