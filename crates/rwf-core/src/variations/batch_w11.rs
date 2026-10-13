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

//! Variations ported from the JWildfire sources (batch w11).

// The glsl_* shaders use JWildfire's truncated pi literals.
#![allow(clippy::approx_constant)]

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;
use super::glsl::{Vec2, Vec3, dc_output, fract, glsl_app, glsl_atan2, mat3, glsl_dbl2int, glsl_kscope, glsl_mod, glsl_rot, length, mat2, mix, normalize, sign, smoothstep, vec2, vec3};
use super::noise;

/// `Cubic_3DFunc` / `CubicLattice3DFunc` node: sign of the lattice offset per axis
/// (x flips with bit 2, y with bit 0, z with bit 1).
fn lattice_signs(node: i32) -> [f64; 3] {
    let s = |bit: i32| if node & bit != 0 { -1.0 } else { 1.0 };
    [s(4), s(1), s(2)]
}

/// `CactusGlobeFunc.applySpikeRealism`
fn cactus_spike(d: &[f64], ctx: &mut Ctx, p: [f64; 3]) -> [f64; 3] {
    // d[13] spike_length, d[14] spike_angle_rand, d[15] spike_droop
    let len = d[13] * ctx.random();
    let body = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    if body < 1e-6 {
        return [0.0; 3];
    }
    let mut s = [p[0] / body * len, p[1] / body * len, p[2] / body * len];
    s[2] -= d[15] * len;
    if d[14] > 0.0 {
        let r = [ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5];
        let rl = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt();
        if rl > 1e-6 {
            let m = d[14] * len * ctx.random();
            for k in 0..3 {
                s[k] += r[k] / rl * m;
            }
        }
    }
    s
}

/// `CactusGlobeFunc.calculateCactusPoint`: `(point, has feature)`
fn cactus_point(d: &[f64], ctx: &mut Ctx, a: &Pt, size: f64) -> ([f64; 3], bool) {
    // d = [pattern_mode, generator_mode, size, ribs_1, ribDepth_1, spiral_1, ribs_2, ribDepth_2,
    //      spiral_2, areoles_per_rib, areole_density, areole_focus, areole_size, ...]
    let (rho, theta, phi) = if d[1] as i32 == 1 {
        let rho = size * ctx.random().powf(1.0 / 3.0);
        let theta = ctx.random() * 2.0 * M_PI;
        (rho, theta, (2.0 * ctx.random() - 1.0).acos())
    } else {
        let s = if d[2] > 0.0 { size / d[2] } else { size };
        let (x, y, z) = (a.x * s, a.y * s, a.z * s);
        let rho = (x * x + y * y + z * z).sqrt();
        if rho == 0.0 {
            return ([0.0; 3], false);
        }
        (rho, y.atan2(x), (z / rho).acos())
    };
    let (sp, cp) = phi.sin_cos();
    let mut feature = false;
    let mut q;
    if d[0] as i32 == 0 {
        let rib_cos = (theta * d[3]).cos();
        let r = rho + d[4] * sp * rib_cos;
        let t = theta + phi * d[5];
        q = [r * sp * t.cos(), r * sp * t.sin(), r * cp];
        if d[9] > 0.0 {
            let rib_width = 2.0 * M_PI / d[3];
            let rib_center = (theta / rib_width + 0.5).floor() * rib_width;
            let cell = M_PI / d[9];
            let ac = (phi / cell).floor() * cell + cell * 0.5;
            let dist = (phi.sin() * ac.sin() * (theta - rib_center).cos() + phi.cos() * ac.cos()).acos();
            if dist < d[12] {
                feature = true;
                let s = cactus_spike(d, ctx, q);
                q = [q[0] + s[0], q[1] + s[1], q[2] + s[2]];
            }
        }
    } else {
        let rc1 = ((theta + phi * d[5]) * d[3]).cos();
        let rc2 = ((theta + phi * d[8]) * d[6]).cos();
        let r = rho + d[4] * sp * rc1 + d[7] * sp * rc2;
        q = [r * sp * theta.cos(), r * sp * theta.sin(), r * cp];
        if d[10] > 0.0 && ctx.random() < d[10] {
            let f = (((rc1 + 1.0) / 2.0) * ((rc2 + 1.0) / 2.0)).powf(d[11]);
            if ctx.random() < f {
                feature = true;
                let s = cactus_spike(d, ctx, q);
                q = [q[0] + s[0], q[1] + s[1], q[2] + s[2]];
            }
        }
    }
    (q, feature)
}

fn rot_x(p: &mut [f64; 3], ang: f64) {
    let (s, c) = ang.sin_cos();
    let (y, z) = (p[1], p[2]);
    p[1] = c * y - s * z;
    p[2] = s * y + c * z;
}
fn rot_y(p: &mut [f64; 3], ang: f64) {
    let (s, c) = ang.sin_cos();
    let (x, z) = (p[0], p[2]);
    p[0] = c * x + s * z;
    p[2] = -s * x + c * z;
}
fn rot_z(p: &mut [f64; 3], ang: f64) {
    let (s, c) = ang.sin_cos();
    let (x, y) = (p[0], p[1]);
    p[0] = c * x - s * y;
    p[1] = s * x + c * y;
}

/// `ChaosCubesFunc` cube selection: is cell `(ix, iy, iz)` part of the fractal in `mode`?
fn chaos_cube_valid(mode: i32, invert: bool, ix: i32, iy: i32, iz: i32) -> bool {
    let zeros = (ix == 0) as i32 + (iy == 0) as i32 + (iz == 0) as i32;
    let manhattan = ix.abs() + iy.abs() + iz.abs();
    let centre = ix == 0 && iy == 0 && iz == 0;
    let v = match mode {
        0 => zeros < 2,
        1 => manhattan <= 1,
        2 => !centre,
        3 => return if invert { zeros >= 2 && !centre } else { zeros < 2 || centre },
        4 => manhattan == 3,
        _ => 3 - zeros == 2,
    };
    v != invert
}

/// `Cell3DFunc.rotatePoint3D` (degrees; z, then y, then x)
fn cell3d_rotate(p: &mut [f64; 3], ax: f64, ay: f64, az: f64) {
    let [mut x, mut y, mut z] = *p;
    if az.abs() > 1e-9 {
        let (s, c) = az.to_radians().sin_cos();
        (x, y) = (x * c - y * s, x * s + y * c);
    }
    if ay.abs() > 1e-9 {
        let (s, c) = ay.to_radians().sin_cos();
        (x, z) = (x * c + z * s, -x * s + z * c);
    }
    if ax.abs() > 1e-9 {
        let (s, c) = ax.to_radians().sin_cos();
        (y, z) = (y * c - z * s, y * s + z * c);
    }
    *p = [x, y, z];
}

fn cell3d_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let mut p = [a.x, a.y, a.z];
    if d[2].abs() > 1e-9 || d[3].abs() > 1e-9 || d[4].abs() > 1e-9 {
        cell3d_rotate(&mut p, d[2], d[3], d[4]);
    }
    let cs = if d[1].abs() < 1e-9 { if d[0].abs() < 1e-9 { 1.0 } else { d[0] } } else { d[0] / d[1] };
    if cs.abs() < 1e-9 {
        v.x += amount * p[0];
        v.y += amount * p[1];
        v.z += amount * p[2];
        return;
    }
    let inv = 1.0 / cs;
    let raw = p.map(|c| (c * inv).floor());
    let proc = [0, 1, 2].map(|k| {
        let pw = d[5 + k];
        if (pw - 1.0).abs() > 1e-9 { raw[k].signum() * raw[k].abs().powf(pw) * if raw[k] == 0.0 { 0.0 } else { 1.0 } } else { raw[k] }
    });
    let mut dd = [0, 1, 2].map(|k| p[k] - raw[k] * cs);
    if d[8] as i32 == 1 {
        dd = dd.map(|c| cs / 2.0 - (c - cs / 2.0).abs());
    }
    if d[62] as i32 == 1 {
        // (color_mode is limited to 0..1 in JWildfire, so only the cell index mode exists)
        let c = proc[0] * d[63] + proc[1] * d[64] + proc[2] * d[65] + d[66];
        v.color = c.abs() - c.abs().floor();
    }
    for k in 0..3 {
        let pw = d[12 + k];
        if (pw - 1.0).abs() > 1e-9 {
            dd[k] = if dd[k].abs() < 1e-9 { 0.0 } else { dd[k].signum() * (dd[k] * inv).abs().powf(pw) * cs };
        }
        dd[k] *= d[9 + k];
    }
    if d[15].abs() > 1e-9 || d[16].abs() > 1e-9 || d[17].abs() > 1e-9 {
        let h = cs / 2.0;
        let mut q = dd.map(|c| c - h);
        cell3d_rotate(&mut q, d[15], d[16], d[17]);
        dd = q.map(|c| c + h);
    }
    let mut alt = false;
    if d[35] as i32 == 1 {
        let odd = (raw[0] as i64 + raw[1] as i64) % 2 != 0;
        alt = odd == (d[36] as i32 == 0);
    }
    let g = |k: usize| d[if alt { 37 } else { 18 } + k];
    let (px, py) = (proc[0], proc[1]);
    let (mx, my) = if py >= 0.0 {
        if px >= 0.0 { (g(0) * px + g(12), g(1) * py + g(13)) } else { (-(g(2) * px + g(4)), g(3) * py) }
    } else if px >= 0.0 {
        (g(5) * px + g(14), -(g(6) * py + g(7)))
    } else {
        (-(g(8) * px + g(10)), -(g(9) * py + g(11)))
    };
    let mz = proc[2] * g(15) + g(16);
    let (fx, fy, fz) = (dd[0] + mx * cs, dd[1] + my * cs, dd[2] + mz * cs);
    v.x += amount * fx;
    v.y += amount * if d[57] as i32 == 1 { -fy } else { fy };
    v.z += amount * if d[58] as i32 == 1 { -fz } else { fz };
    if d[54] as i32 > 0 && ctx.random() < 0.5 {
        v.x = -v.x;
    }
    if d[55] as i32 > 0 && ctx.random() < 0.5 {
        v.y = -v.y;
    }
    if d[56] as i32 > 0 && ctx.random() < 0.5 {
        v.z = -v.z;
    }
    v.x += d[59];
    v.y += d[60];
    v.z += d[61];
}

/// `DC_GenericMandelbrootFunc.gen_mandelbroot`
fn gmandel_color(d: &[f64], p: Vec2) -> Vec3 {
    // d = [randomize, colormode, iters, exp_re, exp_im, n, m, burnigship, conjugate, julia, jre,
    //      jim, panX, panY, zoom, ...]
    let (iters, dexp, k, n, m) = (d[2] as i32, d[3], d[4], d[5], d[6]);
    let (bship, conj, julia) = (d[7] as i32 == 1, d[8] as i32 == 1, d[9] as i32 == 1);
    let (mut ii, mut r) = (0.0f64, 0.0f64);
    let mut z = p;
    for i in 0..iters {
        if bship {
            z = vec2(z.x.abs(), z.y.abs());
        }
        if conj {
            z.y = -z.y;
        }
        let q1 = z.y.atan2(z.x);
        r = z.x * z.x + z.y * z.y;
        let q2 = q1 * dexp + (k / 2.0) * (r + m).ln() + 2.0 * M_PI * n;
        let q3 = r.powf(dexp / 2.0) / (k * q1).exp();
        let base = if julia { vec2(d[10], d[11]) } else { p };
        z = vec2(q3 * q2.cos() + base.x, q3 * q2.sin() + base.y);
        if m == 0.0 && !julia && i == 0 {
            r = 0.0;
        }
        if r > iters as f64 {
            ii = i as f64;
            break;
        }
    }
    let q = if dexp == 0.0 { ii * r.powf(0.5) * ii.powf(0.75) / 255.0 } else { ii * r.powf(dexp.abs() / dexp * 0.5) * ii.powf(0.75) / 255.0 };
    let jround = |x: f64| (x + 0.5).floor();
    let (mut rrr, mut ggg, mut bbb) = (0.0, 0.0, 0.0);
    match d[1] as i32 {
        0 => {
            let s = 0.125662 * ii;
            Vec3::new((s + 0.9).cos() * 0.4 + 0.6, (s + 0.3).cos() * 0.4 + 0.6, (s + 0.2).cos() * 0.4 + 0.6)
        }
        1 => {
            let ss = jround((1.12f64.powf(ii) - 1.0) / (1.12 - 1.0));
            rrr = jround(ss / 256.0 / 256.0);
            ggg = jround((ss - rrr * 256.0 * 256.0) / 256.0);
            bbb = jround(ss - rrr * 256.0 * 256.0 - ggg * 256.0);
            let _ = (ggg, bbb);
            let hue = (255.0 * ii / iters as f64).floor();
            let value = if ii <= iters as f64 { 255.0 } else { 0.0 };
            // hsv(h, s = 1, v)
            let (h, vv) = (hue / 255.0, value / 255.0);
            let c = |o: f64| ((((o + h) / 3.0) - ((o + h) / 3.0).floor()) * 6.0 - 3.0).abs() - 1.0;
            Vec3::new(c(3.0).clamp(0.0, 1.0) * vv, c(2.0).clamp(0.0, 1.0) * vv, c(1.0).clamp(0.0, 1.0) * vv)
        }
        2 => Vec3::new((rrr * q + z.x * z.x * ii / 255.0) / 255.0, (ggg * q - z.y * z.y * ii / 255.0) / 255.0, (bbb * q - ii * ii * ii / 255.0) / 255.0),
        3 => Vec3::new(
            (rrr * q + (z.x * z.x * ii).powf(0.5) / 255.0) / 255.0,
            (ggg * q + (z.y * z.y * ii).powf(0.5) / 255.0) / 255.0,
            bbb * q + (ii * ii * ii).powf(3.0) / 255.0,
        ),
        4 => {
            let t = r * ii / iters as f64;
            Vec3::new(ii * t.powf(0.45) / 255.0, ii * t.powf(0.35) / 255.0, ii * t.powf(0.25) / 255.0)
        }
        _ => {
            let f = (1e20f64).min(z.x * z.x + z.y * z.y);
            let f = 1.0 + f.log10() / 2.0f64.log10() / 16.0;
            Vec3::new(f, f * f, f * f * f)
        }
    }
}

/// `FloraFunc` leaf outline for the angle `t0 = atan2(y, x)`: `[lx, ly]`.
fn flora_leaf(kind: i32, sm: f64, t0: f64) -> (f64, f64) {
    let t = t0 + M_PI_2;
    match kind {
        0 => {
            let mut r = 0.0;
            if t.sin() > 0.0 {
                r = 1.0;
                r -= (sm * 0.8) * t.cos().powf(50.0);
                r *= 1.0 + 0.05 * (8.0 * t).cos();
            }
            (r * t.cos(), r * t.sin())
        }
        1 => {
            let r = (1.0 + (0.5 + sm * 0.8) * (8.0 * t).cos()) * (1.0 + 0.1 * (24.0 * t).cos()) * (0.9 + 0.05 * (200.0 * t).cos()) * (1.0 + t.sin());
            (r * t.sin(), -r * t.cos())
        }
        2 => {
            let cleft = 0.5 + (-6.0 * sm + 0.5) * 0.5;
            let t = t0 + M_PI / 4.0;
            let r = (1.0 + 0.8 * (4.0 * t).cos()) * (1.0 - cleft * (4.0 * t).sin().powf(2.0));
            (r * t.cos(), r * t.sin())
        }
        3 => {
            let mut r = t.sin();
            if r < 0.0 {
                r = 0.0;
            }
            r *= 1.0 + (sm * 0.1) * (40.0 * t).cos();
            (r * t.cos() * 0.7, r * t.sin())
        }
        4 => {
            let r = 0.6 * (1.2 + ((8.0 + sm * 16.0) * t).cos());
            (r * t.cos(), r * t.sin())
        }
        5 => {
            let r = (t.cos().exp() - 2.0 * (4.0 * t).cos() + (t / 12.0).sin().powf(5.0)) * (0.7 + sm * 0.6);
            (r * t.sin(), -r * t.cos())
        }
        6 => {
            let r = (1.0 + 0.8 * t.sin()) * (1.0 + (0.1 + sm * 0.2) * (14.0 * t).cos());
            (r * t.sin(), -r * t.cos())
        }
        7 => {
            let r = (1.0 - 0.9 * t.sin()) * (1.0 + 0.02 * (60.0 * t).cos());
            (r * t.cos() * (0.6 + sm * 0.4), r * t.sin())
        }
        8 => {
            let r = 1.0 + 0.05 * (12.0 * t0).cos();
            (r * t0.cos(), r * t0.sin() + (0.1 + sm * 0.4) * (1.0 - r))
        }
        9 => {
            let r = (1.0 - 0.5 * t.sin()) * (1.0 + (0.1 + sm * 0.4) * (5.0 * t).cos()) * (1.0 + 0.05 * (25.0 * t).cos());
            (r * t.sin(), -r * t.cos())
        }
        10 => {
            let r = if t.sin() > 0.0 { t.sin() * (1.0 + (0.05 + sm * 0.3) * (30.0 * t).cos()) } else { 0.0 };
            (r * t.cos() * 0.6, r * t.sin())
        }
        11 => {
            let mut base = 1.0 - 0.9 * t.sin();
            if base < 0.0 {
                base = 0.0;
            }
            let mut fen = 1.0 - (sm * 0.8) * (t * 2.5).sin().powf(10.0);
            fen *= 1.0 - (sm * 0.7) * (t * 3.5).cos().powf(10.0);
            let r = base * fen;
            (r * t.cos(), r * t.sin())
        }
        12 => {
            let r = 1.0 + (0.2 + sm * 0.4) * (8.0 * t).cos();
            (r * t.cos(), r * t.sin())
        }
        13 => {
            let r = 1.0 - (0.15 + sm * 0.2) * (t * 10.0 / 2.0).sin().abs();
            (r * t.cos() * 0.7, r * t.sin())
        }
        14 => {
            let r = (1.0 - (0.3 + sm * 0.4) * (t * 5.0 / 2.0).sin().abs()) * (1.0 + 0.04 * (40.0 * t).cos());
            (r * t.cos(), r * t.sin())
        }
        15 => {
            let lobe = 0.1 + sm * 0.2;
            let base = 1.0 + lobe * (3.0 * t).cos() - (lobe * 0.5) * (5.0 * t).cos();
            let r = base * (1.0 + 0.03 * (20.0 * t).cos() + 0.02 * (35.0 * t).cos());
            (r * t.cos(), r * t.sin())
        }
        16 => {
            let r = (1.0 + (0.3 + sm * 0.5) * (7.0 * t).cos()) * (1.0 + 0.05 * (42.0 * t).cos());
            (r * t.cos(), r * t.sin())
        }
        17 => {
            let r = (1.0 - 0.9 * t.sin()) * (1.0 + (0.02 + sm * 0.05) * (10.0 * t).sin());
            (r * t.cos() * 1.2, r * t.sin())
        }
        18 => {
            let t = t0;
            let r = (1.0 - t.sin()) * (1.0 - (0.1 + sm * 0.4) * t.cos().powf(2.0)) * (1.0 + 0.03 * (15.0 * t).sin());
            (r * t.cos() * 0.7, r * t.sin())
        }
        19 => {
            let r = 0.8 * (1.1 + ((20.0 + sm * 40.0) * t).cos() * 0.3);
            (r * t.cos(), r * t.sin())
        }
        20 => {
            let r = 1.0 + t.sin() + (0.2 + sm * 0.6) * (5.0 * t - M_PI_2).sin();
            (r * t.cos(), r * t.sin())
        }
        21 => {
            let t = t0;
            let r = (1.0 - 0.9 * t.sin()) * (1.0 + 0.05 * (20.0 * t).cos()) * (1.0 + (0.02 + sm * 0.03) * (40.0 * t).cos());
            (r * t.cos() * 0.8, r * t.sin())
        }
        22 => {
            let r = t.cos().abs().powf(0.3) + t.sin().abs().powf(2.0 + sm * 2.0);
            (r * t.sin(), -r * t.cos() * 0.6)
        }
        23 => {
            let r = 1.0 - 0.9 * (t + (sm - 0.5) * 0.2).sin();
            (r * t.sin() * 1.1, -r * t.cos())
        }
        _ => {
            let t = t0 * (2.0 + sm * 4.0) * M_PI;
            let r = 0.05 * t;
            (r * t.cos(), r * t.sin())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// glsl_* (GLSLFunc): a random pixel of a resolutionX x 1000000 grid (resolutionY keeps its
// initial value in JWildfire) is colored by a shader and placed into the unit square.
// ---------------------------------------------------------------------------------------------

/// JWildfire's `resolutionY` (never updated from the "Density Pixels" parameter).
const GLSL_RES_Y: f64 = 1000000.0;

/// Random pixel `(i, j)` for the resolution `res_x`.
fn glsl_pixel(ctx: &mut Ctx, res_x: f64) -> (f64, f64) {
    let i = (ctx.random() * res_x) as i32;
    let j = (ctx.random() * GLSL_RES_Y) as i32;
    (i as f64, j as f64)
}

/// Output of a pixel: true color (`gradient == 0`) or the gradient index `idx`, and the position.
fn glsl_out(v: &mut Pt, amount: f64, (i, j): (f64, f64), res_x: f64, color: Vec3, gradient: f64, idx: impl FnOnce(Vec3) -> f64) {
    if gradient as i32 == 0 {
        let t = glsl_dbl2int(color);
        v.rgb_color = true;
        v.red = t[0] as f64;
        v.green = t[1] as f64;
        v.blue = t[2] as f64;
    } else {
        v.color = idx(color);
    }
    v.x += amount * (i / res_x - 0.5);
    v.y += amount * (j / GLSL_RES_Y - 0.5);
}

/// `sin(r / (r + g + b))`, the gradient index of several glsl_* variations.
fn glsl_red_ratio(c: Vec3) -> f64 {
    (c.x / (c.x + c.y + c.z)).sin()
}

/// "Density Pixels": `(int) limitValue(v, 100, 10000000)`.
fn glsl_density(v: f64) -> f64 {
    limit_value(v, 100.0, 10000000.0).trunc()
}

fn glsl_circuits_formula(d: &[f64], mut z: Vec2, t: f64) -> (Vec3, f64) {
    let (intensity, pulse, loops, zoom) = (d[3], d[5], d[7], d[8]);
    let mut m = 0.0f64;
    let (mut ot, mut ot2) = (1000.0f64, 1000.0f64);
    let mut k = (loops / 4.0 + (5.0 * zoom).floor()).floor();
    for i in 0..11 {
        z = super::glsl::abs(z) / (z.x * z.x + z.y * z.y).clamp(0.1, 0.5) - t;
        let l = length(z);
        let o = java_min(java_max(java_min(z.x, z.y).abs(), -l + 0.25), (l - 0.25).abs());
        ot = java_min(ot, o);
        ot2 = java_min(l * 0.1, ot2);
        m = java_max(m, i as f64 * (1.0 - sign(ot - o).abs()));
        if k <= 0.0 {
            break;
        }
        k -= 1.0;
    }
    m += 1.0;
    let w = (intensity * zoom) * m;
    let circ = (java_max(0.0, w - ot2) / w).powf(6.0);
    let ds = java_max((java_max(0.0, w - ot) / w).powf(0.25), circ);
    let col = normalize(Vec3::splat(0.1) + vec3(0.45, 0.75, m * 0.1));
    let t2 = Vec3::splat(0.4 + glsl_mod(m / 9.0 - t * pulse + ot2 * 2.0, 1.0));
    (col * t2 + vec3(1.0, 0.7, 0.3) * (circ * (10.0 - m) * 3.0), ds)
}

/// `GLSLCircuitsFunc.getRGBColor`; `s` is JWildfire's `S` field, which is carried across
/// samples.
fn glsl_circuits_rgb(d: &[f64], (xp, yp): (f64, f64), s: &mut f64) -> Vec3 {
    let (time, rate, focus, loops, zoom) = (d[1], d[2], d[4], d[7], d[8]);
    let pos = vec2((xp + 0.5) / d[0] - 0.5, (yp + 0.5) / GLSL_RES_Y - 0.5);
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
    let pix = 0.5 / d[0] * zz / sph;
    let dof = zoom * focus + t * 0.25;
    let mut l = loops.floor();
    let mut color = Vec3::splat(0.0);
    for aa in 0..24 {
        let aauv = vec2((aa as f64 / 6.0).floor(), glsl_mod(aa as f64, 6.0).floor());
        let (col, ds) = glsl_circuits_formula(d, uv + aauv * pix * dof, t);
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

/// `GLSLGrid3DFunc.field`
fn glsl_grid3d_field(p: Vec3) -> Vec3 {
    let mut p = p * 0.1;
    let mut f = 0.1;
    for _ in 0..3 {
        p = vec3(p.y, p.z, p.x);
        p = super::glsl::abs(fract(p) - 0.5) * 2.0;
        f *= 2.0;
    }
    p = p * p;
    super::glsl::sqrt(p + vec3(p.y, p.z, p.x)) / f - 0.05
}

/// `GLSLHyperbolicFunc` helpers.
fn glsl_hyp_tocart(p: Vec2) -> Vec2 {
    vec2(p.x * p.y.cos(), p.x * p.y.sin())
}

fn glsl_hyp_topolar(c: Vec2) -> Vec2 {
    vec2((c.x * c.x + c.y * c.y).sqrt(), glsl_atan2(c.y, c.x))
}

fn glsl_hyp_mirror(line: Vec2, point: Vec2) -> Vec2 {
    if line.x < 100.0 {
        let lc = glsl_hyp_tocart(line);
        let cart = glsl_hyp_tocart(point);
        let (dx, dy) = (cart.x - lc.x, cart.y - lc.y);
        let rr = (line.x * line.x - 1.0) / (dx * dx + dy * dy);
        glsl_hyp_topolar(lc + vec2(dx * rr, dy * rr))
    } else {
        vec2(point.x, -point.y + 2.0 * line.y + M_PI)
    }
}

/// `GLSLStarsFieldFunc` hashes and star layer.
fn glsl_stars_hash11(p: f64) -> f64 {
    let mut p = fract(p * 35.35);
    p += p * (p + 45.85);
    fract(p * 7858.58)
}

fn glsl_stars_hash21(p: Vec2) -> f64 {
    let mut p = fract(p * vec2(451.45, 231.95));
    p = p + super::glsl::dot(p, p + 78.78);
    fract(p.x * p.y)
}

fn glsl_stars_hash22(p: Vec2) -> Vec2 {
    let mut q = fract(vec3(p.x, p.y, p.x) * vec3(451.45, 231.95, 7878.5));
    q = q + super::glsl::dot(q, q + 78.78);
    fract(vec2(q.x, q.z) * q.y)
}

fn glsl_stars_layer(uv: Vec2, zdistance: f64) -> f64 {
    let uv = uv * 5.0;
    let i = super::glsl::floor(uv);
    let f = fract(uv) * zdistance - 1.0;
    let p = glsl_stars_hash22(i) * 0.3;
    let d = length(f - p);
    let mut c = smoothstep(0.1 + 0.8 * glsl_stars_hash21(i), 0.01, d);
    c *= (1.0 / d) * 0.2;
    c
}

/// `GLSLSquaresFunc.hit`
fn glsl_squares_hit(d: &[f64], p: Vec2) -> bool {
    let direction = if d[4] as i32 == 1 { 0.1 } else { -1.0 };
    let mut c = p / 0.5f64.powf(glsl_mod(direction * d[2], 1.0));
    for _ in 0..d[3] as i32 {
        let sectors = super::glsl::floor(c * 3.0);
        if sectors.x == 1.0 && sectors.y == 1.0 {
            return false;
        }
        c = c * 3.0 - sectors;
    }
    true
}

/// `GLSLRandomOctreeFunc.getRGBColor`; d = [res_x, seed, time, steps, mouse_x, mouse_y, grid,
/// borders, black_borders, gradient].
fn glsl_randomoctree_rgb(d: &[f64], (xp, yp): (f64, f64)) -> Vec3 {
    use super::glsl::{abs, dot, floor, sign, step};
    let (time, steps, mouse_x, mouse_y) = (d[2], d[3] as i32, d[4], d[5]);
    const DETAIL: i32 = 5;
    let getvoxel = |p: Vec3, size: f64| -> i32 {
        if p.x == 0.0 && p.y == 0.0 {
            return 0;
        }
        let val = fract(4e4 * (p.x * 13.46 + p.y * 41.74 + p.z * -73.36 + size * 14.24 + 17.34).sin());
        if val < 0.5 {
            0
        } else if val < 0.95 {
            1
        } else {
            2
        }
    };
    let rot = |a: f64| mat2(a.cos(), a.sin(), -a.sin(), a.cos());
    let uv = vec2(2.0 * (xp + 0.5) / d[0] - 1.0, 2.0 * (yp + 0.5) / GLSL_RES_Y - 1.0);
    let mut size = 1.0;
    let maxdistance = 6.0;
    let ro = vec3(0.5 + time.sin() * 0.4, 0.5 + time.cos() * 0.4, time);
    let mut rd = normalize(Vec3::from_vec2(uv, 1.0));
    if length(vec2(mouse_x * d[0], mouse_y * GLSL_RES_Y)) > 40.0 {
        let t1 = vec2(rd.y, rd.z) * rot(mouse_y * M_PI - M_PI / 2.0);
        rd = vec3(rd.x, t1.x, t1.y);
        let t2 = vec2(rd.x, rd.z) * rot(2.0 * mouse_x * M_PI - M_PI);
        rd = vec3(t2.x, rd.y, t2.y);
    }
    let mut lro = glsl_mod(ro, size);
    let mut fro = ro - lro;
    let ird = Vec3::splat(1.0) / super::glsl::max(abs(rd), Vec3::splat(0.001));
    let mut mask = Vec3::splat(0.0);
    let mut exitoct = false;
    let mut recursions = 0;
    let mut dist = 0.0;
    let mut edge = 1.0;
    let mut lastmask = Vec3::splat(0.0);
    for _ in 0..steps {
        if dist > maxdistance {
            break;
        }
        let voxelstate = getvoxel(fro, size);
        if exitoct {
            let newfro = floor(fro / (size / 0.5) + 0.25) * (size * 2.0);
            lro += fro - newfro;
            fro = newfro;
            recursions -= 1;
            size *= 2.0;
            let v0 = glsl_mod(fro / size + 0.5, 2.0);
            let v1 = v0 - Vec3::splat(1.0) + mask * sign(rd) * 0.5;
            exitoct = recursions > 0 && dot(v1, mask).abs() < 0.1;
        } else if voxelstate == 1 && recursions <= DETAIL {
            recursions += 1;
            size *= 0.5;
            let mask2 = step(Vec3::splat(size), lro);
            fro += mask2 * size;
            lro -= mask2 * size;
        } else if voxelstate == 0 || voxelstate == 2 || recursions > DETAIL {
            // voxel(lro, rd, ird, size)
            let hs = size * 0.5;
            let hit = (sign(rd) * (lro - hs) - hs) * ird * -1.0;
            mask = if hit.x < java_min(hit.y, hit.z) {
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
            lro += rd * len - mask * sign(rd) * size;
            fro += mask * sign(rd) * size;
            // JWildfire compares two new vec3 objects by reference here (always unequal)
            exitoct = recursions > 0;
            lastmask = mask;
        }
        if d[6] as i32 == 1 {
            let q = abs(lro / size - 0.5) * (Vec3::splat(1.0) - lastmask);
            edge = java_min(edge, -(java_max(java_max(q.x, q.y), q.z) - 0.5) * 80.0 * size);
        }
    }
    let mut color;
    if dist < maxdistance {
        let val = fract(dot(fro, vec3(15.23, 754.345, 3.454)));
        let normal = lastmask * sign(rd) * -1.0;
        color = super::glsl::sin(vec3(39.896, 57.3225, 48.25) * val) * 0.5 + 0.5;
        color *= normal * 0.25 + 0.75;
        if d[7] as i32 == 1 {
            let q = abs(lro / size - 0.5) * (Vec3::splat(1.0) - lastmask);
            edge = super::glsl::clamp(-(java_max(java_max(q.x, q.y), q.z) - 0.5) * 20.0 * size, 0.0, edge);
        }
        if d[8] as i32 == 1 {
            color *= edge;
        } else {
            color = Vec3::splat(1.0) - (Vec3::splat(1.0) - color) * edge;
        }
    } else if d[8] as i32 == 1 {
        color = Vec3::splat(edge);
    } else {
        color = Vec3::splat(1.0 - edge);
    }
    super::glsl::sqrt(color)
}

/// `GlynnSim2Func.init`: limits and appends [phi10, gamma, delta, abs_pow] to
/// [radius, thickness, contrast, pow, phi1, phi2].
fn glynn_sim2_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    d[1] = limit_value(p[1], 0.0, 1.0);
    d[2] = limit_value(p[2], 0.0, 1.0);
    let (radius, thickness) = (d[0], d[1]);
    let (phi10, phi20) = (M_PI * p[4] / 180.0, M_PI * p[5] / 180.0);
    d.extend([phi10, thickness * (2.0 * radius + thickness) / (radius + thickness), phi20 - phi10, p[3].abs()]);
    d
}

/// Shared tail of `glynnspiro` / `glynnSShape`: `g` = [radius, radius1, thickness, contrast, x1,
/// y1, abs_pow], `c` = curve point (already scaled, before `* radius1 + (x1, y1)`).
fn glynn_curve(g: [f64; 7], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64, c: (f64, f64)) {
    let [radius, radius1, thickness, contrast, x1, y1, abs_pow] = g;
    let r = (a.x * a.x + a.y * a.y).sqrt();
    if r < radius.abs() {
        if radius1 >= 0.0 {
            v.x += amount * (c.0 * radius1 + x1);
            v.y += amount * (c.1 * radius1 + y1);
        } else {
            let cr = radius1 * (thickness + (1.0 - thickness) * ctx.random());
            let phi = 2.0 * M_PI * ctx.random();
            v.x += amount * (cr * phi.cos() + x1);
            v.y += amount * (cr * phi.sin() + y1);
        }
    } else {
        let alpha = radius.abs() / r;
        let (xi, yi) = if ctx.random() > contrast * alpha.powf(abs_pow) { (a.x, a.y) } else { (alpha * alpha * a.x, alpha * alpha * a.y) };
        if (xi - x1) * (xi - x1) + (yi - y1) * (yi - y1) < radius1 * radius1 {
            v.x += amount * (c.0 * radius1 + x1);
            v.y += amount * (c.1 * radius1 + y1);
        } else {
            v.x += amount * xi;
            v.y += amount * yi;
        }
    }
    ctx.pass_z(a, v, amount);
}

/// Appends [x1, y1, abs_pow] of the glynn* variations.
fn glynn_prepare(p: &[f64], radius: f64, phi1: f64, pow: f64, contrast_idx: usize) -> Vec<f64> {
    let mut d = p.to_vec();
    d[contrast_idx] = limit_value(p[contrast_idx], 0.0, 1.0);
    let ph = M_PI * phi1 / 180.0;
    d.extend([radius * ph.cos(), radius * ph.sin(), pow.abs()]);
    d
}

/// `GosperIslandFunc.GosperIsland`: polyline of the turtle path, d = [level, line_thickness,
/// line_thickness / 100, segments, x0, y0, x1, y1, ...].
fn gosperisland_prepare(p: &[f64]) -> Vec<f64> {
    // JWildfire allows level 10 (6 * 7^10 segments); capped to keep the point list manageable.
    let level = limit_value(ftoi(p[0]), 1, 6);
    const ANGLE: f64 = 19.106605350869096;
    struct Turtle {
        x: f64,
        y: f64,
        angle: f64,
        pts: Vec<f64>,
    }
    fn gosper(t: &mut Turtle, n: i32) {
        if n == 0 {
            t.x += t.angle.to_radians().cos();
            t.y += t.angle.to_radians().sin();
            t.pts.extend([t.x, t.y]);
        } else {
            t.angle -= ANGLE;
            gosper(t, n - 1);
            t.angle += 60.0;
            gosper(t, n - 1);
            t.angle -= 60.0;
            gosper(t, n - 1);
            t.angle += ANGLE;
        }
    }
    let mut t = Turtle { x: 9.0 / 32.0, y: 1.0 / 8.0, angle: 0.0, pts: vec![9.0 / 32.0, 1.0 / 8.0] };
    for _ in 0..6 {
        gosper(&mut t, level);
        t.angle += 60.0;
    }
    let nseg = t.pts.len() / 2 - 1;
    let mut d = vec![level as f64, p[1], p[1] / 100.0, nseg as f64];
    d.extend_from_slice(&t.pts);
    d
}

/// `GreeblesFunc.transform`; d = [mode, base_shape, size, subdivisions, greeble_shape,
/// greeble_height, seed].
fn greebles_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let (mode, base_shape, size, subdivisions, shape, gheight, seed) = (d[0] as i32, d[1] as i32, d[2], d[3] as i32, d[4] as i32, d[5], d[6] as i32);
    let norm = |p: [f64; 3]| {
        let len = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        if len > 1e-9 { [p[0] / len, p[1] / len, p[2] / len] } else { p }
    };
    let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let frac = |n: f64| n - n.floor();
    let base;
    let normal;
    let gsize;
    if base_shape == 0 {
        let half = size * 0.5;
        let (ua, va) = if mode == 1 {
            (a.y.atan2(a.x), a.z.atan2((a.x * a.x + a.y * a.y).sqrt()))
        } else {
            let ua = ctx.random() * 2.0 * M_PI;
            (ua, (2.0 * ctx.random() - 1.0).acos())
        };
        let ucell = (2.0 * M_PI) / (subdivisions * 2).max(1) as f64;
        let vcell = M_PI / subdivisions.max(1) as f64;
        let uq = (ua / ucell).floor() * ucell + ucell * 0.5;
        let vq = (va / vcell).floor() * vcell + vcell * 0.5;
        base = [half * vq.sin() * uq.cos(), half * vq.sin() * uq.sin(), half * vq.cos()];
        normal = norm(base);
        gsize = (M_PI * size) / subdivisions as f64;
    } else {
        let mut p = if mode == 1 {
            [a.x, a.y, a.z]
        } else if base_shape == 2 {
            [(ctx.random() - 0.5) * size, (ctx.random() - 0.5) * size, 0.0]
        } else {
            let face = (ctx.random() * 6.0) as i32;
            let (ur, vr) = ((ctx.random() - 0.5) * size, (ctx.random() - 0.5) * size);
            let hs = size * 0.5;
            match face {
                0 => [hs, ur, vr],
                1 => [-hs, ur, vr],
                2 => [ur, hs, vr],
                3 => [ur, -hs, vr],
                4 => [ur, vr, hs],
                _ => [ur, vr, -hs],
            }
        };
        let half = size * 0.5;
        let sgn = |c: f64| if c > 0.0 { 1.0 } else { -1.0 };
        if base_shape == 2 {
            p[2] = 0.0;
            normal = [0.0, 0.0, 1.0];
        } else {
            let (ax, ay, az) = (p[0].abs(), p[1].abs(), p[2].abs());
            let k = if ax >= ay && ax >= az { 0 } else if ay >= ax && ay >= az { 1 } else { 2 };
            p[k] = sgn(p[k]) * half;
            let mut n = [0.0; 3];
            n[k] = sgn(p[k]);
            normal = n;
        }
        gsize = if subdivisions > 0 { size / subdivisions as f64 } else { size };
        if gsize < 1.0e-6 {
            v.do_hide = true;
            return;
        }
        base = p.map(|c| (c / gsize).floor() as i32 as f64 * gsize + gsize * 0.5);
    }
    let cell_seed = (seed as i64).wrapping_add(((base[0] * 100.0) as i64).wrapping_mul(73856093))
        ^ ((base[1] * 100.0) as i64).wrapping_mul(19349663)
        ^ ((base[2] * 100.0) as i64).wrapping_mul(83492791);
    let height = gheight * crate::brush::Marsaglia::new(cell_seed).random();
    let up = if normal[1].abs() > 0.999 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let tangent = norm(cross(normal, up));
    let bitangent = norm(cross(normal, tangent));
    let (mut u, mut w2, mut w) = (0.0, 0.0, 0.0);
    if mode == 0 {
        match shape {
            1 => {
                let ang = ctx.random() * 2.0 * M_PI;
                let rad = (gsize * 0.5) * ctx.random().sqrt();
                u = ang.cos() * rad;
                w2 = ang.sin() * rad;
                w = ctx.random() * height;
            }
            2 => {
                let (rx, ry, rz) = (ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5);
                let rl = (rx * rx + ry * ry + rz * rz).sqrt();
                if rl > 1e-9 {
                    let rad = ctx.random().powf(1.0 / 3.0);
                    u = (rx / rl) * rad * (gsize * 0.5);
                    w2 = (ry / rl) * rad * (gsize * 0.5);
                    w = (rz / rl) * rad * height;
                }
            }
            3 => {
                let ang = ctx.random() * 2.0 * M_PI;
                w = ctx.random() * height;
                let taper = if height > 1e-9 { 1.0 - w / height } else { 1.0 };
                let rad = (gsize * 0.5) * ctx.random().sqrt() * taper;
                u = ang.cos() * rad;
                w2 = ang.sin() * rad;
            }
            _ => {
                u = (ctx.random() - 0.5) * gsize;
                w2 = (ctx.random() - 0.5) * gsize;
                w = ctx.random() * height;
            }
        }
    } else {
        let r = [a.x - base[0], a.y - base[1], a.z - base[2]];
        let dot = |b: [f64; 3]| r[0] * b[0] + r[1] * b[1] + r[2] * b[2];
        let (lu, lv, lw) = (dot(tangent), dot(bitangent), dot(normal));
        match shape {
            1 | 3 => {
                let ang = lv.atan2(lu);
                if shape == 1 {
                    let rad = (gsize * 0.5) * frac((lu * lu + lv * lv).sqrt());
                    u = ang.cos() * rad;
                    w2 = ang.sin() * rad;
                    w = frac(lw) * height;
                } else {
                    w = frac(lw) * height;
                    let taper = if height > 1e-9 { 1.0 - w / height } else { 1.0 };
                    let rad = (gsize * 0.5) * frac((lu * lu + lv * lv).sqrt()) * taper;
                    u = ang.cos() * rad;
                    w2 = ang.sin() * rad;
                }
            }
            2 => {
                let rl = (lu * lu + lv * lv + lw * lw).sqrt();
                let rad = (gsize * 0.5) * frac(rl);
                if rl > 1e-9 {
                    u = (lu / rl) * rad;
                    w2 = (lv / rl) * rad;
                    w = (lw / rl) * rad;
                }
            }
            _ => {
                u = (frac(lu) - 0.5) * gsize;
                w2 = (frac(lv) - 0.5) * gsize;
                w = frac(lw) * height;
            }
        }
    }
    for k in 0..3 {
        let f = base[k] + tangent[k] * u + bitangent[k] * w2 + normal[k] * w;
        match k {
            0 => v.x += f * amount,
            1 => v.y += f * amount,
            _ => v.z += f * amount,
        }
    }
}

/// Deterministic 0..1 value per grid cell (`Grid3DWFFunc` draws the spreads from a
/// java.util.Random in the order the cells are first visited).
fn grid3d_cell_rnd(x: i32, y: i32, z: i32, k: u32) -> f64 {
    let mut h = (x as u32).wrapping_mul(0x8da6b343) ^ (y as u32).wrapping_mul(0xd8163841) ^ (z as u32).wrapping_mul(0xcb1ab31f) ^ k.wrapping_mul(0x165667b1);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846ca68b);
    h ^= h >> 16;
    h as f64 / 4294967296.0
}

/// `GridoutFunc` / `Gridout2Func` / `Gridout3DFunc` quadrant selection: 0..7 = cases a..h.
fn gridout_case(x: f64, y: f64) -> usize {
    if y <= 0.0 {
        if x > 0.0 {
            if -y >= x { 0 } else { 1 }
        } else if y <= x {
            2
        } else {
            3
        }
    } else if x > 0.0 {
        if y >= x { 4 } else { 5 }
    } else if y > -x {
        6
    } else {
        7
    }
}

/// `HamidFunc` preset curves: line presets (0..9) give `[x1, y1, x2, y2]`, circle presets
/// (10..19) `[x, y, radius, 0]`; d = [presetId, npoints, filled, a..p].
fn hamid_curve(d: &[f64], fi: f64) -> [f64; 4] {
    let hs = f64::sin;
    let hc = f64::cos;
    let hp = f64::powf;
    let np = d[1];
    let [pa, pb, pc, pd, pe, pf, pg, ph, pi, pj, pk, pl, pm, pn, po, pp] = [d[3], d[4], d[5], d[6], d[7], d[8], d[9], d[10], d[11], d[12], d[13], d[14], d[15], d[16], d[17], d[18]];
    match d[0] as i32 {
        0 => [-hs(4.0 * pa * M_PI * fi / np), -hc(2.0 * pb * M_PI * fi / np), -0.5 * pe * hs(8.0 * pc * M_PI * fi / np), -0.5 * pf * hc(4.0 * pd * M_PI * fi / np)],
        1 => [-hs(2.0 * pa * M_PI * fi / np), -hc(2.0 * pb * M_PI * fi / np), -0.5 * pe * hs(8.0 * pc * M_PI * fi / np), -0.5 * pf * hc(12.0 * pd * M_PI * fi / np)],
        2 => [-hs(8.0 * pa * M_PI * fi / np), -hc(2.0 * pb * M_PI * fi / np), -0.5 * pe * hs(6.0 * pc * M_PI * fi / np), -0.5 * pf * hc(2.0 * pd * M_PI * fi / np)],
        3 => [-hs(10.0 * pa * M_PI * fi / np), -hc(2.0 * pb * M_PI * fi / np), -0.5 * pe * hs(12.0 * pc * M_PI * fi / np), -0.5 * pf * hc(2.0 * pd * M_PI * fi / np)],
        4 => [hs(10.0 * pa * M_PI * (fi + 699.0) / np), hc(8.0 * pb * M_PI * (fi + 699.0) / np), hs(12.0 * pc * M_PI * (fi + 699.0) / np), hc(10.0 * pd * M_PI * (fi + 699.0) / np)],
        5 => [-2.0 * pa * hc(4.0 * pb * M_PI * fi / 1000.0), 0.5 * pc * hp(hc(6.0 * pd * M_PI * fi / np), 3.0 * pi), -2.0 * pe * hs(6.0 * pf * M_PI * fi / np) / 15.0 * pj, (4.0 * pg * hs(2.0 * ph * M_PI * fi / np) / 15.0 * pk)],
        6 => [3.0 * pa * hp(hs(2.0 * pb * M_PI * fi / np), 3.0 * ph), -hc(8.0 * pc * M_PI * fi / np), 1.5 * pd * hp(hs(2.0 * pe * M_PI * fi / np), 3.0 * pi), -0.5 * pf * hc(6.0 * pg * M_PI * fi / np)],
        7 => [hc(6.0 * pa * M_PI * fi / np), -hc(8.0 * pb * M_PI * fi / np), 1.5 * pc * hp(hs(2.0 * M_PI * fi / np), 3.0 * pf), -0.5 * pd * hc(6.0 * pe * M_PI * fi / np)],
        8 => [hp(hs(20.0 * pa * M_PI * fi / np), 3.0), hp(hc(18.0 * pb * M_PI * fi / np), 3.0 * pe), hp(hs(16.0 * pc * M_PI * fi / np), 3.0), hp(hc(14.0 * pd * M_PI * fi / np), 3.0 * pf)],
        9 => {
            let side = |mut n: i32| loop {
                if n < 100 {
                    break [n as f64 / 100.0, 0.0];
                } else if n < 200 {
                    break [1.0, (n as f64 - 100.0) / 100.0];
                } else if n < 300 {
                    break [(300 - n) as f64 / 100.0, 1.0];
                } else if n < 400 {
                    break [0.0, (400 - n) as f64 / 100.0];
                }
                n -= 400;
            };
            let (p1, p2) = (side(fi as i32), side(fi as i32 + 199));
            [p1[0], p1[1], p2[0], p2[1]]
        }
        10 => [2.0 * pa * hs(2.0 * pb * M_PI * fi / np) / 3.0 + hs(36.0 * pc * M_PI * fi / np) * hc(30.0 * pd * M_PI * fi / np) / 3.0 * pk + hp(hs(14.0 * M_PI * fi / np), 3.0 * pl) / 3.0 * pm, 2.0 * pe * hc(2.0 * pf * M_PI * fi / np) / 3.0 * pn + hc(36.0 * pg * M_PI * fi / np) * hc(30.0 * ph * M_PI * fi / np) / 3.0 * po + hp(hc(14.0 * pi * M_PI * fi / np), 3.0) / 5.0 * pp, 1.0 / 150.0 + hp(hs(36.0 * pj * M_PI * fi / np), 4.0) / 9.0, 0.0],
        11 => [hs(14.0 * pa * M_PI * fi / np), hc(32.0 * pb * M_PI * fi / np), hp(hs(20.0 * pc * M_PI * fi / np), 2.0) / 3.0 * pd, 0.0],
        12 => [2.0 * pa * hs(6.0 * pb * M_PI * fi / np) / 3.0 + hs(36.0 * pc * M_PI * fi / np) * hc(30.0 * pd * M_PI * fi / np) / 4.0 + hp(hs(14.0 * pe * M_PI * fi / np), 3.0 * pk) / 4.0 * pl, 2.0 * pf * hc(6.0 * pg * M_PI * fi / np) / 3.0 + hc(28.0 * ph * M_PI * fi / np) * hc(30.0 * pi * M_PI * fi / np) / 3.0 + hp(hc(14.0 * M_PI * fi / np), 3.0 * pm) / 5.0 * pn, 1.0 / 150.0 + hp(hs(36.0 * pj * M_PI * fi / np), 4.0 * po) / 20.0 * pp, 0.0],
        13 => [hs(10.0 * pa * M_PI * fi / np), hp(hc(28.0 * pb * M_PI * fi / np), 3.0 * pe), hp(hs(18.0 * pc * M_PI * fi / np), 2.0) / 3.0 * pd, 0.0],
        14 => [2.0 * pa * hs(18.0 * pb * M_PI * fi / np) / 3.0 + hp(hs(40.0 * pc * M_PI * fi / np), 3.0) * hc(30.0 * pd * M_PI * fi / np) / 3.0 + hp(hs(34.0 * pe * M_PI * fi / np), 3.0 * pk) / 4.0 * pl, 2.0 * pf * hc(18.0 * pg * M_PI * fi / np) / 3.0 + hp(hc(28.0 * ph * M_PI * fi / np), 3.0) * hc(40.0 * M_PI * fi / np) / 3.0 + hp(hc(34.0 * pi * M_PI * fi / np), 3.0 * pm) / 3.0 * pn, 1.0 / 150.0 + hp(hs(56.0 * pj * M_PI * fi / np), 4.0 * po) / 8.0 * pp, 0.0],
        15 => [2.0 * pa * hs(14.0 * pb * M_PI * fi / np) / 3.0 + hp(hs(10.0 * pc * M_PI * fi / np), 3.0) * hc(28.0 * pd * M_PI * fi / np) / 3.0 + hp(hs(46.0 * pe * M_PI * fi / np), 3.0 * pm) / 3.0 * pn, 2.0 * hc(14.0 * pf * M_PI * fi / np) / 3.0 + hp(hc(10.0 * pg * M_PI * fi / np), 3.0) * hc(28.0 * ph * M_PI * fi / np) / 3.0 + hp(hc(34.0 * pj * M_PI * fi / np), 3.0 * po) / 3.0, 1.0 / 150.0 + hp(hs(72.0 * pk * M_PI * fi / np), 6.0) / 9.0 + hp(hs(12.0 * pl * M_PI * fi / np), 4.0) / 20.0 * pp, 0.0],
        16 => [hs(38.0 * pa * M_PI * fi / np) * hc(26.0 * pb * M_PI * fi / np) / 2.0 + hp(hs(18.0 * pc * M_PI * fi / np), 3.0 * pl) / 2.0 * pm, 2.0 * hc(18.0 * pd * M_PI * fi / np) / 3.0 + hc(38.0 * pe * M_PI * fi / np) * hc(36.0 * pf * M_PI * fi / np) / 3.0 + hp(hc(18.0 * pg * M_PI * fi / np), 3.0 * pn) / 4.0 * po, 1.0 / 150.0 + hp(hs(18.0 * ph * M_PI * fi / np), 10.0) * hp(hs(100.0 * pi * M_PI * fi / np), 6.0) / 100.0 + hp(hs(72.0 * pj * M_PI * fi / np), 8.0) * hp(hc(18.0 * pk * M_PI * fi / np), 6.0) / 20.0 * pp, 0.0],
        17 => [2.0 * pa * hs(10.0 * pb * M_PI * fi / np) + hp(hs(6.0 * pc * M_PI * fi / np), 3.0) * hc(28.0 * pd * M_PI * fi / np) / 3.0 + hs(126.0 * pe * M_PI * fi / np) / 10.0 * pl, 2.0 * hc(6.0 * pf * M_PI * fi / np) / 3.0 + hp(hc(18.0 * pg * M_PI * fi / np), 3.0) * hc(36.0 * ph * M_PI * fi / np) / 3.0 + hc(126.0 * pi * M_PI * fi / np) / 10.0 * pm, 1.0 / 150.0 + hp(hs(48.0 * pj * M_PI * fi / np), 4.0) * (1.0 + hp(hs(12.0 * pk * M_PI * fi / np), 2.0 * pn)) / 18.0 * po, 0.0],
        18 => [2.0 * pa * hs(18.0 * pb * M_PI * fi / np) / 5.0 + hp(hs(34.0 * pc * M_PI * fi / np), 3.0 * pj) / 3.0 * pk + hp(hs(50.0 * pd * M_PI * fi / np), 3.0 * pl) / 3.0 * pm, 2.0 * hc(18.0 * pe * M_PI * fi / np) / 3.0 + hp(hc(34.0 * pf * M_PI * fi / np), 3.0) / 3.0 + hp(hc(50.0 * pg * M_PI * fi / np), 3.0 * pn) / 10.0 * po, 1.0 / 150.0 + hp(hs(58.0 * pi * M_PI * fi / np), 4.0) / 10.0 * pp, 0.0],
        _ => [hc(10.0 * pa * M_PI * fi / np) * (1.0 * pc - hp(hs(16.0 * pb * M_PI * fi / np), 2.0 * ph) / 2.0 * pi), hs(10.0 * pd * M_PI * fi / np) * (1.0 * pf - hp(hc(34.0 * pe * M_PI * fi / np), 2.0 * pj) / 2.0 * pk), 1.0 / 200.0 + hp(hs(52.0 * pg * M_PI * fi / np), 4.0 * pl) / 10.0 * pm, 0.0],
    }
}

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "barycentroid",
        params: &[ParamDef::new("a", 1.0), ParamDef::new("b", 0.0), ParamDef::new("c", 0.0), ParamDef::new("d", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (dot00, dot01) = (d[0] * d[0] + d[1] * d[1], d[0] * d[2] + d[1] * d[3]);
            let (dot02, dot11) = (d[0] * a.x + d[1] * a.y, d[2] * d[2] + d[3] * d[3]);
            let dot12 = d[2] * a.x + d[3] * a.y;
            let inv = 1.0 / (dot00 * dot11 - dot01 * dot01);
            let u = (dot11 * dot02 - dot01 * dot12) * inv;
            let w = (dot00 * dot12 - dot01 * dot02) * inv;
            let sgn = |x: f64| if x < 0.0 { -1.0 } else if x > 0.0 { 1.0 } else { 0.0 };
            v.x += amount * (u * u + a.x * a.x).sqrt() * sgn(u);
            v.y += amount * (w * w + a.y * a.y).sqrt() * sgn(w);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dot00 = D(0) * D(0) + D(1) * D(1);
let dot01 = D(0) * D(2) + D(1) * D(3);
let dot02 = D(0) * a.x + D(1) * a.y;
let dot11 = D(2) * D(2) + D(3) * D(3);
let dot12 = D(2) * a.x + D(3) * a.y;
let inv = 1.0 / (dot00 * dot11 - dot01 * dot01);
let u = (dot11 * dot02 - dot01 * dot12) * inv;
let w = (dot00 * dot12 - dot01 * dot02) * inv;
v.x += AMOUNT * sqrt(u * u + a.x * a.x) * sign(u);
v.y += AMOUNT * sqrt(w * w + a.y * a.y) * sign(w);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cubic3D",
        params: &[ParamDef::new("xpand", 0.25), ParamDef::new("style", 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [xpand, style, fill, smoothStyle]
            let node = (ctx.random() * 8.0).trunc() as i32;
            let lattd = amount * 0.5;
            let smooth = if amount.abs() <= 0.5 { amount * 2.0 } else { 1.0 };
            let (fill, ss) = (d[2], d[3]);
            let ex = 1.0 - ss * (1.0 - a.x.atan2(a.z).cos());
            let wy = 1.0 - ss * (1.0 - a.y.atan2(a.z).sin());
            let zn = if ss > 1.0 { 1.0 - ss * (1.0 - (ex + wy) / 2.0 * ss) } else { 1.0 - ss * (1.0 - (ex + wy) / 2.0) };
            let s = lattice_signs(node);
            v.x = (v.x - smooth * (1.0 - fill) * v.x * ex) + a.x * smooth * fill * ex + s[0] * lattd;
            v.y = (v.y - smooth * (1.0 - fill) * v.y * wy) + a.y * smooth * fill * wy + s[1] * lattd;
            v.z = (v.z - smooth * (1.0 - fill) * v.z * zn) + a.z * smooth * fill * zn + s[2] * lattd;
        },
        prepare: Some(|p, _| {
            let fill = if p[0].abs() <= 1.0 { p[0] * 0.5 } else { p[0].sqrt() * 0.5 };
            let ss = if p[1].abs() <= 1.0 {
                p[1]
            } else if p[1] > 1.0 {
                1.0 + (p[1] - 1.0) * 0.25
            } else {
                (p[1] + 1.0) * 0.25 - 1.0
            };
            vec![p[0], p[1], fill, ss]
        }),
        wgsl: "let node = i32(trunc(rnd() * 8.0));
let lattd = AMOUNT * 0.5;
var smth = 1.0;
if (abs(AMOUNT) <= 0.5) { smth = AMOUNT * 2.0; }
let fill = D(2);
let ss = D(3);
let ex = 1.0 - ss * (1.0 - cos(atan2(a.x, a.z)));
let wy = 1.0 - ss * (1.0 - sin(atan2(a.y, a.z)));
var zn = 1.0 - ss * (1.0 - (ex + wy) / 2.0);
if (ss > 1.0) { zn = 1.0 - ss * (1.0 - (ex + wy) / 2.0 * ss); }
let s = vec3<f32>(select(1.0, -1.0, (node & 4) != 0), select(1.0, -1.0, (node & 1) != 0), select(1.0, -1.0, (node & 2) != 0));
v.x = (v.x - smth * (1.0 - fill) * v.x * ex) + a.x * smth * fill * ex + s.x * lattd;
v.y = (v.y - smth * (1.0 - fill) * v.y * wy) + a.y * smth * fill * wy + s.y * lattd;
v.z = (v.z - smth * (1.0 - fill) * v.z * zn) + a.z * smth * fill * zn + s.z * lattd;",
    },
    VarDef {
        name: "cubicLattice_3D",
        params: &[ParamDef::new("xpand", 0.2), ParamDef::new("style", 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [xpand, style, fill, style 2]
            let (ex, wy, zn) = if d[3] != 0.0 {
                let ex = a.x.atan2(a.z).cos();
                let wy = a.y.atan2(a.z).sin();
                (ex, wy, (ex + wy) / 2.0)
            } else {
                (1.0, 1.0, 1.0)
            };
            let s = lattice_signs((ctx.random() * 8.0).trunc() as i32);
            let fill = d[2];
            v.x = (v.x + a.x) * fill * ex + s[0] * amount;
            v.y = (v.y + a.y) * fill * wy + s[1] * amount;
            v.z = (v.z + a.z) * fill * zn + s[2] * amount;
        },
        prepare: Some(|p, _| {
            let fill = if p[0].abs() <= 1.0 { p[0] * 0.5 } else { p[0].sqrt() * 0.5 };
            vec![p[0], p[1], fill, if p[1].abs() >= 2.0 { 1.0 } else { 0.0 }]
        }),
        wgsl: "var ex = 1.0;
var wy = 1.0;
var zn = 1.0;
if (D(3) != 0.0) {
  ex = cos(atan2(a.x, a.z));
  wy = sin(atan2(a.y, a.z));
  zn = (ex + wy) / 2.0;
}
let node = i32(trunc(rnd() * 8.0));
let s = vec3<f32>(select(1.0, -1.0, (node & 4) != 0), select(1.0, -1.0, (node & 1) != 0), select(1.0, -1.0, (node & 2) != 0));
v.x = (v.x + a.x) * D(2) * ex + s.x * AMOUNT;
v.y = (v.y + a.y) * D(2) * wy + s.y * AMOUNT;
v.z = (v.z + a.z) * D(2) * zn + s.z * AMOUNT;",
    },
    VarDef {
        name: "cactusGlobe",
        params: &[
            ParamDef::int("pattern_mode", 0.0, 0.0, 1.0),
            ParamDef::int("generator_mode", 1.0, 0.0, 1.0),
            ParamDef::new("size", 1.0),
            ParamDef::range("ribs_1", 12.0, 0.0, 40.0),
            ParamDef::new("ribDepth_1", 0.1),
            ParamDef::new("spiral_1", 0.5),
            ParamDef::range("ribs_2", 0.0, 0.0, 40.0),
            ParamDef::new("ribDepth_2", 0.0),
            ParamDef::new("spiral_2", 0.0),
            ParamDef::range("areoles_per_rib", 10.0, 0.0, 40.0),
            ParamDef::range("areole_density", 1.0, 0.0, 1.0),
            ParamDef::range("areole_focus", 16.0, 0.0, 64.0),
            ParamDef::new("areole_size", 0.05),
            ParamDef::new("spike_length", 0.1),
            ParamDef::new("spike_angle_rand", 0.0),
            ParamDef::new("spike_droop", 0.0),
            ParamDef::int("recursion_depth", 3.0, 0.0, 8.0),
            ParamDef::range("pup_chance", 0.3, 0.0, 1.0),
            ParamDef::int("pups_count", 3.0, 1.0, 12.0),
            ParamDef::new("pup_size", 0.5),
            ParamDef::new("pups_spread", 1.0),
            ParamDef::new("pups_vertical_offset", -0.5),
            ParamDef::range("feature_color", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            let mut size = d[2];
            let mut off = [0.0; 3];
            for _ in 0..d[16] as i32 {
                if ctx.random() < d[17] {
                    let idx = (ctx.random() * d[18]).trunc();
                    let ang = idx / d[18] * 2.0 * M_PI;
                    off[0] += size * d[20] * ang.cos();
                    off[1] += size * d[20] * ang.sin();
                    off[2] += size * d[21];
                    size *= d[19];
                } else {
                    break;
                }
            }
            let (q, feature) = cactus_point(d, ctx, a, size);
            if feature {
                v.color = d[22];
            }
            v.x += (q[0] + off[0]) * amount;
            v.y += (q[1] + off[1]) * amount;
            v.z += (q[2] + off[2]) * amount;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = ftoi(p[0]).clamp(0, 1) as f64;
            d[1] = ftoi(p[1]).clamp(0, 1) as f64;
            d[10] = limit_value(p[10], 0.0, 1.0);
            d[16] = ftoi(p[16]).clamp(0, 8) as f64;
            d[17] = limit_value(p[17], 0.0, 1.0);
            d[18] = p[18].trunc();
            d[22] = limit_value(p[22], 0.0, 1.0);
            d
        }),
        wgsl: "fn cactusGlobe_spike(base: u32, p: vec3<f32>) -> vec3<f32> {
  let len = prm[base + 13u] * rnd();
  let body = length(p);
  if (body < 1e-6) { return vec3<f32>(0.0); }
  var s = p / body * len;
  s.z -= prm[base + 15u] * len;
  if (prm[base + 14u] > 0.0) {
    let r = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
    let rl = length(r);
    if (rl > 1e-6) { s += r / rl * (prm[base + 14u] * len * rnd()); }
  }
  return s;
}
// (point, feature flag)
fn cactusGlobe_point(base: u32, a: Pt, size: f32) -> vec4<f32> {
  var rho = 0.0;
  var theta = 0.0;
  var phi = 0.0;
  if (i32(prm[base + 1u]) == 1) {
    rho = size * pow(rnd(), 1.0 / 3.0);
    theta = rnd() * TWO_PI;
    phi = acos(2.0 * rnd() - 1.0);
  } else {
    var s = size;
    if (prm[base + 2u] > 0.0) { s = size / prm[base + 2u]; }
    let q = vec3<f32>(a.x, a.y, a.z) * s;
    rho = length(q);
    if (rho == 0.0) { return vec4<f32>(0.0, 0.0, 0.0, -1.0); }
    theta = atan2(q.y, q.x);
    phi = acos(q.z / rho);
  }
  let sp = sin(phi);
  let cp = cos(phi);
  var feature = -1.0;
  var q = vec3<f32>(0.0);
  if (i32(prm[base]) == 0) {
    let r = rho + prm[base + 4u] * sp * cos(theta * prm[base + 3u]);
    let t = theta + phi * prm[base + 5u];
    q = vec3<f32>(r * sp * cos(t), r * sp * sin(t), r * cp);
    if (prm[base + 9u] > 0.0) {
      let rib_width = TWO_PI / prm[base + 3u];
      let rib_center = floor(theta / rib_width + 0.5) * rib_width;
      let cell = PI / prm[base + 9u];
      let ac = floor(phi / cell) * cell + cell * 0.5;
      let dist = acos(sin(phi) * sin(ac) * cos(theta - rib_center) + cos(phi) * cos(ac));
      if (dist < prm[base + 12u]) { feature = 1.0; q += cactusGlobe_spike(base, q); }
    }
  } else {
    let rc1 = cos((theta + phi * prm[base + 5u]) * prm[base + 3u]);
    let rc2 = cos((theta + phi * prm[base + 8u]) * prm[base + 6u]);
    let r = rho + prm[base + 4u] * sp * rc1 + prm[base + 7u] * sp * rc2;
    q = vec3<f32>(r * sp * cos(theta), r * sp * sin(theta), r * cp);
    if (prm[base + 10u] > 0.0 && rnd() < prm[base + 10u]) {
      let f = pow(((rc1 + 1.0) / 2.0) * ((rc2 + 1.0) / 2.0), prm[base + 11u]);
      if (rnd() < f) { feature = 1.0; q += cactusGlobe_spike(base, q); }
    }
  }
  return vec4<f32>(q, feature);
}
@body
var size = D(2);
var off = vec3<f32>(0.0);
for (var i = 0; i < i32(D(16)); i++) {
  if (rnd() < D(17)) {
    let ang = trunc(rnd() * D(18)) / D(18) * TWO_PI;
    off += vec3<f32>(size * D(20) * cos(ang), size * D(20) * sin(ang), size * D(21));
    size *= D(19);
  } else { break; }
}
let q = cactusGlobe_point(base, a, size);
if (q.w > 0.0) { v.color = D(22); }
v.x += (q.x + off.x) * AMOUNT;
v.y += (q.y + off.y) * AMOUNT;
v.z += (q.z + off.z) * AMOUNT;",
    },
    VarDef {
        name: "chaosCubes",
        params: &[
            ParamDef::int("mode", 0.0, 0.0, 7.0),
            ParamDef::int("mode7_A", 0.0, 0.0, 5.0),
            ParamDef::int("mode7_B", 1.0, 0.0, 5.0),
            ParamDef::int("depth", 5.0, 1.0, 15.0),
            ParamDef::new("twistX", 0.0),
            ParamDef::new("twistY", 0.0),
            ParamDef::new("twistZ", 0.0),
            ParamDef::range("scaleX", 0.3333333, 0.0, 1.0),
            ParamDef::range("scaleY", 0.3333333, 0.0, 1.0),
            ParamDef::range("scaleZ", 0.3333333, 0.0, 1.0),
            ParamDef::new("offset", 1.0),
            ParamDef::range("rotX", 0.0, -180.0, 180.0),
            ParamDef::range("rotY", 0.0, -180.0, 180.0),
            ParamDef::range("rotZ", 0.0, -180.0, 180.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
            ParamDef::int("julia", 0.0, 0.0, 1.0),
            ParamDef::int("colorMode", 0.0, 0.0, 5.0),
            ParamDef::new("colorSpeed", 1.0),
            ParamDef::int("sphereInvert", 0.0, 0.0, 1.0),
            ParamDef::new("sphereRadius", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            let mut p = [a.x, a.y, a.z];
            let (rx, ry, rz) = (d[11].to_radians(), d[12].to_radians(), d[13].to_radians());
            if rx != 0.0 {
                rot_x(&mut p, rx);
            }
            if ry != 0.0 {
                rot_y(&mut p, ry);
            }
            if rz != 0.0 {
                rot_z(&mut p, rz);
            }
            let c = p;
            let (mode, off, invert) = (d[0] as i32, d[10], d[14] != 0.0);
            let scale = [d[7], d[8], d[9]];
            let (mut last, mut last_v, mut path) = ([0i32; 3], 0, 0.0);
            for _ in 0..d[3] as i32 {
                let m = if mode == 7 { if ctx.random() < 0.5 { d[1] as i32 } else { d[2] as i32 } } else { mode };
                if m <= 5 {
                    let cell = loop {
                        let ix = (ctx.random() * 3.0) as i32 - 1;
                        let iy = (ctx.random() * 3.0) as i32 - 1;
                        let iz = (ctx.random() * 3.0) as i32 - 1;
                        if chaos_cube_valid(m, invert, ix, iy, iz) {
                            break [ix, iy, iz];
                        }
                    };
                    last = cell;
                    for k in 0..3 {
                        p[k] = p[k] * scale[k] - cell[k] as f64 * off * (1.0 - scale[k]);
                        if d[15] != 0.0 {
                            p[k] += c[k];
                        }
                    }
                } else {
                    last_v = (ctx.random() * 5.0) as i32;
                    let vt = [[off, off, 0.0], [off, -off, 0.0], [-off, -off, 0.0], [-off, off, 0.0], [0.0, 0.0, off * scale[1] * 0.5]][last_v as usize];
                    for k in 0..3 {
                        p[k] = (p[k] + vt[k]) * 0.5;
                    }
                }
                if d[4] != 0.0 {
                    let ang = p[0] * d[4];
                    rot_x(&mut p, ang);
                }
                if d[5] != 0.0 {
                    let ang = p[1] * d[5];
                    rot_y(&mut p, ang);
                }
                if d[6] != 0.0 {
                    let ang = p[2] * d[6];
                    rot_z(&mut p, ang);
                }
                path += (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            }
            if d[18] != 0.0 {
                let r2 = p[0] * p[0] + p[1] * p[1] + p[2] * p[2];
                if r2 > 1.0e-6 {
                    let f = d[19] * d[19] / r2;
                    p = [p[0] * f, p[1] * f, p[2] * f];
                }
            }
            v.x += p[0] * amount;
            v.y += p[1] * amount;
            v.z += p[2] * amount;
            if d[16] as i32 > 0 {
                let col = match d[16] as i32 {
                    1 => (p[0] / off + p[1] / off + p[2] / off) / 3.0,
                    2 => {
                        if mode == 6 {
                            last_v as f64 / 4.0
                        } else {
                            (last[0] + 1 + (last[1] + 1) * 3 + (last[2] + 1) * 9) as f64 / 26.0
                        }
                    }
                    3 => (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() / off,
                    4 => path / (d[3] * off),
                    _ => (p[1].atan2(p[0]) + p[2].atan2(p[0]) + 2.0 * M_PI) / (4.0 * M_PI),
                } * d[17];
                let col = ((col % 1.0) + 1.0) % 1.0;
                v.color += (col - v.color) * amount;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            let clamp = |v: f64, lo: f64, hi: f64| v.min(hi).max(lo).trunc();
            d[0] = clamp(p[0], 0.0, 7.0);
            d[1] = clamp(p[1], 0.0, 5.0);
            d[2] = clamp(p[2], 0.0, 5.0);
            d[3] = clamp(p[3], 1.0, 15.0);
            for i in [14, 15, 18] {
                d[i] = if p[i] > 0.5 { 1.0 } else { 0.0 };
            }
            d[16] = clamp(p[16], 0.0, 5.0);
            d
        }),
        wgsl: "fn chaosCubes_rot(p: vec3<f32>, axis: i32, ang: f32) -> vec3<f32> {
  let s = sin(ang);
  let c = cos(ang);
  if (axis == 0) { return vec3<f32>(p.x, c * p.y - s * p.z, s * p.y + c * p.z); }
  if (axis == 1) { return vec3<f32>(c * p.x + s * p.z, p.y, -s * p.x + c * p.z); }
  return vec3<f32>(c * p.x - s * p.y, s * p.x + c * p.y, p.z);
}
fn chaosCubes_valid(mode: i32, invert: bool, c: vec3<i32>) -> bool {
  let zeros = select(0, 1, c.x == 0) + select(0, 1, c.y == 0) + select(0, 1, c.z == 0);
  let manhattan = abs(c.x) + abs(c.y) + abs(c.z);
  let centre = c.x == 0 && c.y == 0 && c.z == 0;
  var v = false;
  if (mode == 0) { v = zeros < 2; }
  else if (mode == 1) { v = manhattan <= 1; }
  else if (mode == 2) { v = !centre; }
  else if (mode == 3) { if (invert) { return zeros >= 2 && !centre; } return zeros < 2 || centre; }
  else if (mode == 4) { v = manhattan == 3; }
  else { v = 3 - zeros == 2; }
  return v != invert;
}
@body
var p = vec3<f32>(a.x, a.y, a.z);
let rx = radians(D(11));
let ry = radians(D(12));
let rz = radians(D(13));
if (rx != 0.0) { p = chaosCubes_rot(p, 0, rx); }
if (ry != 0.0) { p = chaosCubes_rot(p, 1, ry); }
if (rz != 0.0) { p = chaosCubes_rot(p, 2, rz); }
let c = p;
let mode = i32(D(0));
let off = D(10);
let invert = D(14) != 0.0;
let scale = vec3<f32>(D(7), D(8), D(9));
var last = vec3<i32>(0);
var last_v = 0;
var path = 0.0;
for (var i = 0; i < i32(D(3)); i++) {
  var m = mode;
  if (mode == 7) { m = select(i32(D(2)), i32(D(1)), rnd() < 0.5); }
  if (m <= 5) {
    var cell = vec3<i32>(0);
    for (var t = 0; t < 256; t++) {
      cell = vec3<i32>(i32(rnd() * 3.0) - 1, i32(rnd() * 3.0) - 1, i32(rnd() * 3.0) - 1);
      if (chaosCubes_valid(m, invert, cell)) { break; }
    }
    last = cell;
    p = p * scale - vec3<f32>(cell) * off * (1.0 - scale);
    if (D(15) != 0.0) { p += c; }
  } else {
    last_v = i32(rnd() * 5.0);
    var vt = vec3<f32>(0.0, 0.0, off * scale.y * 0.5);
    if (last_v == 0) { vt = vec3<f32>(off, off, 0.0); }
    else if (last_v == 1) { vt = vec3<f32>(off, -off, 0.0); }
    else if (last_v == 2) { vt = vec3<f32>(-off, -off, 0.0); }
    else if (last_v == 3) { vt = vec3<f32>(-off, off, 0.0); }
    p = (p + vt) * 0.5;
  }
  if (D(4) != 0.0) { p = chaosCubes_rot(p, 0, p.x * D(4)); }
  if (D(5) != 0.0) { p = chaosCubes_rot(p, 1, p.y * D(5)); }
  if (D(6) != 0.0) { p = chaosCubes_rot(p, 2, p.z * D(6)); }
  path += length(p);
}
if (D(18) != 0.0) {
  let r2 = dot(p, p);
  if (r2 > 1.0e-6) { p *= D(19) * D(19) / r2; }
}
v.x += p.x * AMOUNT;
v.y += p.y * AMOUNT;
v.z += p.z * AMOUNT;
let cm = i32(D(16));
if (cm > 0) {
  var col = 0.0;
  if (cm == 1) { col = (p.x / off + p.y / off + p.z / off) / 3.0; }
  else if (cm == 2) {
    if (mode == 6) { col = f32(last_v) / 4.0; } else { col = f32(last.x + 1 + (last.y + 1) * 3 + (last.z + 1) * 9) / 26.0; }
  }
  else if (cm == 3) { col = length(p) / off; }
  else if (cm == 4) { col = path / (D(3) * off); }
  else { col = (atan2(p.y, p.x) + atan2(p.z, p.x) + TWO_PI) / (4.0 * PI); }
  col *= D(17);
  col = fmod(fmod(col, 1.0) + 1.0, 1.0);
  v.color += (col - v.color) * AMOUNT;
}",
    },
    VarDef {
        name: "cell3D",
        params: &[
            ParamDef::new("size", 0.6),
            ParamDef::new("a", 1.0),
            ParamDef::new("grid_rot_x", 0.0),
            ParamDef::new("grid_rot_y", 0.0),
            ParamDef::new("grid_rot_z", 0.0),
            ParamDef::new("cell_idx_power_x", 1.0),
            ParamDef::new("cell_idx_power_y", 1.0),
            ParamDef::new("cell_idx_power_z", 1.0),
            ParamDef::int("dxdydz_mode", 0.0, 0.0, 1.0),
            ParamDef::new("cell_scale_x", 1.0),
            ParamDef::new("cell_scale_y", 1.0),
            ParamDef::new("cell_scale_z", 1.0),
            ParamDef::new("cell_power_x", 1.0),
            ParamDef::new("cell_power_y", 1.0),
            ParamDef::new("cell_power_z", 1.0),
            ParamDef::new("cell_rot_x", 0.0),
            ParamDef::new("cell_rot_y", 0.0),
            ParamDef::new("cell_rot_z", 0.0),
            ParamDef::new("space_xa", 2.0),
            ParamDef::new("space_ya", 2.0),
            ParamDef::new("space_xb", 2.0),
            ParamDef::new("space_yb", 2.0),
            ParamDef::new("move_xa", 1.0),
            ParamDef::new("space_xc", 2.0),
            ParamDef::new("space_yc", 2.0),
            ParamDef::new("move_ya", 1.0),
            ParamDef::new("space_xd", 2.0),
            ParamDef::new("space_yd", 2.0),
            ParamDef::new("move_xb", 1.0),
            ParamDef::new("move_yb", 1.0),
            ParamDef::new("move_tr_x", 0.0),
            ParamDef::new("move_tr_y", 0.0),
            ParamDef::new("move_br_x", 0.0),
            ParamDef::new("space_z", 1.0),
            ParamDef::new("move_z", 0.0),
            ParamDef::int("checker_mode", 0.0, 0.0, 1.0),
            ParamDef::int("checker_invert", 0.0, 0.0, 1.0),
            ParamDef::new("space_xa2", 2.0),
            ParamDef::new("space_ya2", 2.0),
            ParamDef::new("space_xb2", 2.0),
            ParamDef::new("space_yb2", 2.0),
            ParamDef::new("move_xa2", 1.0),
            ParamDef::new("space_xc2", 2.0),
            ParamDef::new("space_yc2", 2.0),
            ParamDef::new("move_ya2", 1.0),
            ParamDef::new("space_xd2", 2.0),
            ParamDef::new("space_yd2", 2.0),
            ParamDef::new("move_xb2", 1.0),
            ParamDef::new("move_yb2", 1.0),
            ParamDef::new("move_tr_x2", 0.0),
            ParamDef::new("move_tr_y2", 0.0),
            ParamDef::new("move_br_x2", 0.0),
            ParamDef::new("space_z2", 1.0),
            ParamDef::new("move_z2", 0.0),
            ParamDef::int("mirror_x", 0.0, 0.0, 1.0),
            ParamDef::int("mirror_y", 0.0, 0.0, 1.0),
            ParamDef::int("mirror_z", 0.0, 0.0, 1.0),
            ParamDef::int("invert_y", 1.0, 0.0, 1.0),
            ParamDef::int("invert_z", 0.0, 0.0, 1.0),
            ParamDef::new("offset_x", 0.0),
            ParamDef::new("offset_y", 0.0),
            ParamDef::new("offset_z", 0.0),
            ParamDef::int("color_mode", 0.0, 0.0, 1.0),
            ParamDef::new("cidx_scale_x", 0.05),
            ParamDef::new("cidx_scale_y", 0.05),
            ParamDef::new("cidx_scale_z", 0.05),
            ParamDef::new("cidx_offset", 0.0),
            ParamDef::new("cdist_scale", 1.0),
            ParamDef::new("color_q1", 0.0),
            ParamDef::new("color_q2", 0.25),
            ParamDef::new("color_q3", 0.5),
            ParamDef::new("color_q4", 0.75),
            ParamDef::int("color_use_z_sign", 0.0, 0.0, 1.0),
            ParamDef::new("color_z_neg_offset", 0.1),
            ParamDef::new("cz_scale", 0.05),
            ParamDef::new("cz_offset", 0.0)
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| cell3d_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for i in [8, 35, 36, 54, 55, 56, 57, 58, 62, 72] {
                d[i] = ftoi(p[i]).clamp(0, 1) as f64;
            }
            for i in 68..72 {
                d[i] = p[i].abs() - p[i].abs().floor();
            }
            d
        }),
        wgsl: "fn cell3D_rot(p: vec3<f32>, ax: f32, ay: f32, az: f32) -> vec3<f32> {
  var q = p;
  if (abs(az) > 1e-9) { let s = sin(radians(az)); let c = cos(radians(az)); q = vec3<f32>(q.x * c - q.y * s, q.x * s + q.y * c, q.z); }
  if (abs(ay) > 1e-9) { let s = sin(radians(ay)); let c = cos(radians(ay)); q = vec3<f32>(q.x * c + q.z * s, q.y, -q.x * s + q.z * c); }
  if (abs(ax) > 1e-9) { let s = sin(radians(ax)); let c = cos(radians(ax)); q = vec3<f32>(q.x, q.y * c - q.z * s, q.y * s + q.z * c); }
  return q;
}
fn cell3D_pow(x: f32, pw: f32) -> f32 {
  if (abs(pw - 1.0) > 1e-9) { if (x == 0.0) { return 0.0; } return sign(x) * pow(abs(x), pw); }
  return x;
}
@body
var p = vec3<f32>(a.x, a.y, a.z);
if (abs(D(2)) > 1e-9 || abs(D(3)) > 1e-9 || abs(D(4)) > 1e-9) { p = cell3D_rot(p, D(2), D(3), D(4)); }
var cs = D(0) / D(1);
if (abs(D(1)) < 1e-9) { cs = select(D(0), 1.0, abs(D(0)) < 1e-9); }
if (abs(cs) < 1e-9) {
  v.x += AMOUNT * p.x; v.y += AMOUNT * p.y; v.z += AMOUNT * p.z;
} else {
  let inv = 1.0 / cs;
  let raw = floor(p * inv);
  let pr = vec3<f32>(cell3D_pow(raw.x, D(5)), cell3D_pow(raw.y, D(6)), cell3D_pow(raw.z, D(7)));
  var dd = p - raw * cs;
  if (i32(D(8)) == 1) { dd = cs / 2.0 - abs(dd - cs / 2.0); }
  if (i32(D(62)) == 1) {
    let c = abs(pr.x * D(63) + pr.y * D(64) + pr.z * D(65) + D(66));
    v.color = c - floor(c);
  }
  let pw = vec3<f32>(D(12), D(13), D(14));
  for (var k = 0; k < 3; k++) {
    if (abs(pw[k] - 1.0) > 1e-9) {
      if (abs(dd[k]) < 1e-9) { dd[k] = 0.0; } else { dd[k] = sign(dd[k]) * pow(abs(dd[k] * inv), pw[k]) * cs; }
    }
  }
  dd *= vec3<f32>(D(9), D(10), D(11));
  if (abs(D(15)) > 1e-9 || abs(D(16)) > 1e-9 || abs(D(17)) > 1e-9) { dd = cell3D_rot(dd - cs / 2.0, D(15), D(16), D(17)) + cs / 2.0; }
  var g = base + 18u;
  if (i32(D(35)) == 1) {
    let odd = (i32(raw.x) + i32(raw.y)) % 2 != 0;
    if (odd == (i32(D(36)) == 0)) { g = base + 37u; }
  }
  var m = vec2<f32>(0.0);
  if (pr.y >= 0.0) {
    if (pr.x >= 0.0) { m = vec2<f32>(prm[g] * pr.x + prm[g + 12u], prm[g + 1u] * pr.y + prm[g + 13u]); }
    else { m = vec2<f32>(-(prm[g + 2u] * pr.x + prm[g + 4u]), prm[g + 3u] * pr.y); }
  } else {
    if (pr.x >= 0.0) { m = vec2<f32>(prm[g + 5u] * pr.x + prm[g + 14u], -(prm[g + 6u] * pr.y + prm[g + 7u])); }
    else { m = vec2<f32>(-(prm[g + 8u] * pr.x + prm[g + 10u]), -(prm[g + 9u] * pr.y + prm[g + 11u])); }
  }
  let mz = pr.z * prm[g + 15u] + prm[g + 16u];
  let f = vec3<f32>(dd.x + m.x * cs, dd.y + m.y * cs, dd.z + mz * cs);
  v.x += AMOUNT * f.x;
  v.y += AMOUNT * select(f.y, -f.y, i32(D(57)) == 1);
  v.z += AMOUNT * select(f.z, -f.z, i32(D(58)) == 1);
  if (i32(D(54)) > 0 && rnd() < 0.5) { v.x = -v.x; }
  if (i32(D(55)) > 0 && rnd() < 0.5) { v.y = -v.y; }
  if (i32(D(56)) > 0 && rnd() < 0.5) { v.z = -v.z; }
  v.x += D(59); v.y += D(60); v.z += D(61);
}",
    },
    VarDef {
        name: "dc_cylinder",
        params: &[
            ParamDef::new("offset", 0.0),
            ParamDef::new("angle", 0.0),
            ParamDef::new("scale", 0.5),
            ParamDef::new("x", 0.125),
            ParamDef::new("y", 0.125),
            ParamDef::new("blur", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [offset, angle, scale, x, y, blur, 1 / scale, cos(angle), sin(angle)]
            let (sr, cr) = (ctx.random() * M_2PI).sin_cos();
            let rr = d[5] * (ctx.gauss_ring(d.as_ptr() as usize, 4, 3) - 2.0);
            v.x += amount * (a.x + rr * sr).sin() * d[3];
            v.y += rr + a.y * d[4];
            v.z += amount * (a.x + rr * cr).cos();
            v.color = (0.5 * (d[6] * (d[7] * v.x + d[8] * v.y + d[0]) + 1.0)).abs() % 1.0;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d.extend([1.0 / if p[2] == 0.0 { 10e-6 } else { p[2] }, p[1].cos(), p[1].sin()]);
            d
        }),
        wgsl: "var<private> dc_cylinder_ring: array<f32, 4>;
var<private> dc_cylinder_idx: u32;
var<private> dc_cylinder_init: bool;
fn dc_cylinder_gauss() -> f32 {
  if (!dc_cylinder_init) {
    dc_cylinder_init = true;
    for (var i = 0; i < 4; i++) { dc_cylinder_ring[i] = rnd(); }
  }
  let s = dc_cylinder_ring[0] + dc_cylinder_ring[1] + dc_cylinder_ring[2] + dc_cylinder_ring[3];
  dc_cylinder_ring[dc_cylinder_idx] = rnd();
  dc_cylinder_idx = (dc_cylinder_idx + 1u) & 3u;
  return s;
}
@body
let ang = rnd() * TWO_PI;
let rr = D(5) * (dc_cylinder_gauss() - 2.0);
v.x += AMOUNT * sin(a.x + rr * sin(ang)) * D(3);
v.y += rr + a.y * D(4);
v.z += AMOUNT * cos(a.x + rr * cos(ang));
v.color = fmod(abs(0.5 * (D(6) * (D(7) * v.x + D(8) * v.y + D(0)) + 1.0)), 1.0);",
    },
    VarDef {
        name: "dc_gmandelbroot",
        params: &[
            ParamDef::int("randomize", 10000.0, 0.0, 10000000.0),
            ParamDef::int("colormode", 0.0, 0.0, 5.0),
            ParamDef::int("iters", 128.0, 1.0, 255.0),
            ParamDef::new("exp_re", 2.0),
            ParamDef::new("exp_im", 0.0),
            ParamDef::new("n", 0.0),
            ParamDef::new("m", 0.0),
            ParamDef::int("burnigship", 0.0, 0.0, 1.0),
            ParamDef::int("conjugate", 0.0, 0.0, 1.0),
            ParamDef::int("julia", 0.0, 0.0, 1.0),
            ParamDef::new("jre", 0.0),
            ParamDef::new("jim", 0.0),
            ParamDef::range("panX", 0.5, -2.0, 2.0),
            ParamDef::range("panY", 0.0, -2.0, 2.0),
            ParamDef::range("zoom", 4.0, 0.0, 100.0),
            ParamDef::int("ColorOnly", 0.0, 0.0, 1.0),
            ParamDef::int("Gradient", 0.0, 0.0, 2.0),
            ParamDef::range("scale_z", 0.0, -2.0, 2.0),
            ParamDef::range("offset_z", 0.0, -2.0, 2.0),
            ParamDef::int("reset_z", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let uv = if d[15] as i32 == 1 { vec2(a.x, a.y) } else { vec2(2.0 * ctx.random() - 1.0, 2.0 * ctx.random() - 1.0) };
            let color = gmandel_color(d, vec2(uv.x * d[14] - d[12], uv.y * d[14] - d[13]));
            v.do_hide = color.x == 0.0 && color.y == 0.0 && color.z == 0.0;
            dc_output(ctx, v, amount, uv, color, d[16] as i32, d[17], d[18], d[19] as i32);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            let lim = |v: f64, lo: f64, hi: f64| limit_value(v, lo, hi);
            d[0] = lim(p[0], 0.0, 10000000.0).trunc();
            d[1] = lim(p[1], 0.0, 5.0).trunc();
            d[2] = lim(p[2], 1.0, 255.0).trunc();
            for i in 7..10 {
                d[i] = lim(p[i], 0.0, 1.0).trunc();
            }
            d[12] = lim(p[12], -2.0, 2.0);
            d[13] = lim(p[13], -2.0, 2.0);
            d[14] = lim(p[14], 0.0, 100.0);
            d
        }),
        wgsl: "fn dc_gmandelbroot_color(base: u32, p: vec2<f32>) -> vec3<f32> {
  let iters = i32(prm[base + 2u]);
  let dexp = prm[base + 3u];
  let k = prm[base + 4u];
  let n = prm[base + 5u];
  let m = prm[base + 6u];
  let bship = i32(prm[base + 7u]) == 1;
  let conj = i32(prm[base + 8u]) == 1;
  let julia = i32(prm[base + 9u]) == 1;
  var ii = 0.0;
  var r = 0.0;
  var z = p;
  for (var i = 0; i < iters; i++) {
    if (bship) { z = abs(z); }
    if (conj) { z.y = -z.y; }
    let q1 = atan2(z.y, z.x);
    r = z.x * z.x + z.y * z.y;
    let q2 = q1 * dexp + (k / 2.0) * log(r + m) + TWO_PI * n;
    let q3 = java_pow(r, dexp / 2.0) / exp(k * q1);
    var b = p;
    if (julia) { b = vec2<f32>(prm[base + 10u], prm[base + 11u]); }
    z = vec2<f32>(q3 * cos(q2), q3 * sin(q2)) + b;
    if (m == 0.0 && !julia && i == 0) { r = 0.0; }
    if (r > f32(iters)) { ii = f32(i); break; }
  }
  var q = ii * java_pow(r, 0.5) * java_pow(ii, 0.75) / 255.0;
  if (dexp != 0.0) { q = ii * java_pow(r, abs(dexp) / dexp * 0.5) * java_pow(ii, 0.75) / 255.0; }
  let mode = i32(prm[base + 1u]);
  if (mode == 0) {
    let s = 0.125662 * ii;
    return vec3<f32>(cos(s + 0.9), cos(s + 0.3), cos(s + 0.2)) * 0.4 + 0.6;
  } else if (mode == 1) {
    let h = floor(255.0 * ii / f32(iters)) / 255.0;
    let vv = select(0.0, 1.0, ii <= f32(iters));
    return clamp(abs(fract((vec3<f32>(3.0, 2.0, 1.0) + h) / 3.0) * 6.0 - 3.0) - 1.0, vec3<f32>(0.0), vec3<f32>(1.0)) * vv;
  } else if (mode == 2) {
    return vec3<f32>((z.x * z.x * ii / 255.0) / 255.0, (-z.y * z.y * ii / 255.0) / 255.0, (-ii * ii * ii / 255.0) / 255.0);
  } else if (mode == 3) {
    return vec3<f32>((sqrt(z.x * z.x * ii) / 255.0) / 255.0, (sqrt(z.y * z.y * ii) / 255.0) / 255.0, java_pow(ii * ii * ii, 3.0) / 255.0);
  } else if (mode == 4) {
    let t = r * ii / f32(iters);
    return vec3<f32>(ii * java_pow(t, 0.45) / 255.0, ii * java_pow(t, 0.35) / 255.0, ii * java_pow(t, 0.25) / 255.0);
  }
  let f = 1.0 + log2(min(1e20, dot(z, z))) / 16.0;
  return vec3<f32>(f, f * f, f * f * f);
}
@body
var uv = vec2<f32>(2.0 * rnd() - 1.0, 2.0 * rnd() - 1.0);
if (i32(D(15)) == 1) { uv = vec2<f32>(a.x, a.y); }
let color = dc_gmandelbroot_color(base, uv * D(14) - vec2<f32>(D(12), D(13)));
if (color.x == 0.0 && color.y == 0.0 && color.z == 0.0) { v.hide = 1.0; } else { v.hide = 0.0; }
dc_output(&v, AMOUNT, uv, color, i32(D(16)), D(17), D(18), i32(D(19)));",
    },
    VarDef {
        name: "dc_perlin",
        params: &[
            ParamDef::int("shape", 0.0, 0.0, 2.0),
            ParamDef::int("map", 0.0, 0.0, 5.0),
            ParamDef::range("select_centre", 0.0, -1.0, 1.0),
            ParamDef::range("select_range", 1.0, 0.1, 2.0),
            ParamDef::new("centre", 0.25),
            ParamDef::new("range", 0.25),
            ParamDef::new("edge", 0.0),
            ParamDef::new("scale", 1.0),
            ParamDef::int("octaves", 2.0, 1.0, 5.0),
            ParamDef::new("amps", 2.0),
            ParamDef::new("freqs", 2.0),
            ParamDef::new("z", 0.0),
            ParamDef::int("select_bailout", 10.0, 2.0, 1000.0),
            ParamDef::int("color_only", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [shape, map, select_centre, select_range, centre, range, edge, scale, octaves, amps,
            //      freqs, z, select_bailout, color_only, notch_bottom, notch_top]
            let (edge, scale, z) = (d[6], d[7], d[11]);
            let (mut vx, mut vy, mut p): (f64, f64, f64);
            let mut t = 0;
            loop {
                let mut e = 0.0;
                if amount == 0.0 {
                    vx = a.x;
                    vy = a.y;
                } else {
                    match d[0] as i32 {
                        0 => {
                            vx = (1.0 + edge) * ((if d[13] > 0.0 { a.x } else { ctx.random() }) - 0.5);
                            vy = (1.0 + edge) * ((if d[13] > 0.0 { a.y } else { ctx.random() }) - 0.5);
                            let r = if vx * vx > vy * vy { vx } else { vy };
                            if r > 1.0 - edge {
                                e = 0.5 * (r - 1.0 + edge) / edge;
                            }
                        }
                        shape => {
                            let mut r = if shape == 1 {
                                let r = ctx.random() + ctx.random();
                                (if r > 1.0 { 2.0 - r } else { r }) * (1.0 + edge)
                            } else {
                                (1.0 + edge) * ctx.random()
                            };
                            if r > 1.0 - edge {
                                e = 0.5 * (r - 1.0 + edge) / edge;
                            }
                            let theta = ctx.random() * M_2PI;
                            r *= 0.5;
                            vx = r * theta.sin();
                            vy = r * theta.cos();
                        }
                    }
                }
                let rr = vx * vx + vy * vy;
                let bubble = || {
                    let r = 0.25 - rr;
                    if r < 0.0 { (-r).sqrt() } else { r.sqrt() }
                };
                let vv = match d[1] as i32 {
                    0 => [scale * vx, scale * vy, scale * z],
                    1 => [scale * vx / (rr + SMALL_EPSILON), scale * vy / (rr + SMALL_EPSILON), scale * z],
                    2 => [scale * vx / (rr + 0.5), scale * vy / (rr + 0.5), scale * z],
                    3 => [scale * vx / (rr + 0.25), scale * vy / (rr + 0.25), scale * z],
                    4 => [scale * vx, scale * vy, scale * (bubble() + z)],
                    _ => [scale * vx, scale * vy, scale * (2.0 * bubble() + z)],
                };
                p = noise::perlin_noise3d(vv, d[9], d[10], d[8] as i32);
                e = if p > 0.0 { p * (1.0 + e * e * 20.0) + 2.0 * e } else { p * (1.0 + e * e * 20.0) - 2.0 * e };
                let again = (e < d[14] || e > d[15]) && t < d[12] as i32;
                t += 1;
                if !again {
                    break;
                }
            }
            v.x += amount * vx;
            v.y += amount * vy;
            if ctx.preserve_z {
                v.z += amount * a.z;
            }
            let col = d[4] + d[5] * p;
            v.color = col - col.floor();
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(ftoi(p[0]), 0, 2) as f64;
            d[1] = limit_value(ftoi(p[1]), 0, 5) as f64;
            d[2] = limit_value(p[2], -1.0, 1.0);
            d[3] = limit_value(p[3], 0.1, 2.0);
            d[8] = limit_value(ftoi(p[8]), 1, 5) as f64;
            d[12] = limit_value(ftoi(p[12]), 2, 1000) as f64;
            d[13] = limit_value(ftoi(p[13]), 0, 1) as f64;
            let mut bottom = d[2] - d[3];
            bottom = if bottom > 0.75 { 0.75 } else { bottom };
            bottom = if bottom < -2.0 { -3.0 } else { bottom };
            let mut top = d[2] + d[3];
            top = if top < -0.75 { -0.75 } else { top };
            top = if top > 3.0 { 3.0 } else { top };
            d.push(bottom);
            d.push(top);
            d
        }),
        wgsl: "let edge = D(6);
let scale = D(7);
let zz = D(11);
var vx = 0.0;
var vy = 0.0;
var p = 0.0;
var t = 0;
loop {
  var e = 0.0;
  if (AMOUNT == 0.0) {
    vx = a.x;
    vy = a.y;
  } else if (i32(D(0)) == 0) {
    var ux = rnd();
    var uy = rnd();
    if (D(13) > 0.0) { ux = a.x; uy = a.y; }
    vx = (1.0 + edge) * (ux - 0.5);
    vy = (1.0 + edge) * (uy - 0.5);
    let r = select(vy, vx, vx * vx > vy * vy);
    if (r > 1.0 - edge) { e = 0.5 * (r - 1.0 + edge) / edge; }
  } else {
    var r = 0.0;
    if (i32(D(0)) == 1) {
      r = rnd() + rnd();
      if (r > 1.0) { r = 2.0 - r; }
      r *= 1.0 + edge;
    } else {
      r = (1.0 + edge) * rnd();
    }
    if (r > 1.0 - edge) { e = 0.5 * (r - 1.0 + edge) / edge; }
    let theta = rnd() * TWO_PI;
    vx = 0.5 * r * sin(theta);
    vy = 0.5 * r * cos(theta);
  }
  let rr = vx * vx + vy * vy;
  let bub = sqrt(abs(0.25 - rr));
  let map = i32(D(1));
  var vv = vec3<f32>(scale * vx, scale * vy, scale * zz);
  if (map == 1) { vv = vec3<f32>(vv.xy / (rr + 1e-30), vv.z); }
  else if (map == 2) { vv = vec3<f32>(vv.xy / (rr + 0.5), vv.z); }
  else if (map == 3) { vv = vec3<f32>(vv.xy / (rr + 0.25), vv.z); }
  else if (map == 4) { vv.z = scale * (bub + zz); }
  else if (map == 5) { vv.z = scale * (2.0 * bub + zz); }
  p = noise_perlin3d(vv, D(9), D(10), i32(D(8)));
  if (p > 0.0) { e = p * (1.0 + e * e * 20.0) + 2.0 * e; } else { e = p * (1.0 + e * e * 20.0) - 2.0 * e; }
  let again = (e < D(14) || e > D(15)) && t < i32(D(12));
  t++;
  if (!again) { break; }
}
v.x += AMOUNT * vx;
v.y += AMOUNT * vy;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
let col = D(4) + D(5) * p;
v.color = col - floor(col);",
    },
    VarDef {
        name: "flora",
        params: &[
            ParamDef::int("leafType", 0.0, 0.0, 24.0),
            ParamDef::range("filled", 1.0, 0.0, 1.0),
            ParamDef::new("scale", 1.0),
            ParamDef::new("distort", 0.1),
            ParamDef::new("shapeMod", 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = if d[1] > 0.0 && d[1] > ctx.random() { amount * ctx.random() } else { amount };
            let (mut lx, mut ly) = flora_leaf(d[0] as i32, d[4], a.atan_yx());
            if d[3] != 0.0 {
                lx += d[3] * (a.y * 5.0).sin();
                ly += d[3] * (a.x * 5.0).cos();
            }
            v.x += r * lx * d[2];
            v.y += r * ly * d[2];
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(ftoi(p[0]), 0, 24) as f64;
            d[1] = limit_value(p[1], 0.0, 1.0);
            d
        }),
        wgsl: "fn flora_leaf(kind: i32, sm: f32, t0: f32) -> vec2<f32> {
  let t = t0 + HALF_PI;
  let st = sin(t);
  let ct = cos(t);
  switch (kind) {
    case 0: {
      var r = 0.0;
      if (st > 0.0) { r = (1.0 - (sm * 0.8) * java_pow(ct, 50.0)) * (1.0 + 0.05 * cos(8.0 * t)); }
      return r * vec2<f32>(ct, st);
    }
    case 1: {
      let r = (1.0 + (0.5 + sm * 0.8) * cos(8.0 * t)) * (1.0 + 0.1 * cos(24.0 * t)) * (0.9 + 0.05 * cos(200.0 * t)) * (1.0 + st);
      return r * vec2<f32>(st, -ct);
    }
    case 2: {
      let cleft = 0.5 + (-6.0 * sm + 0.5) * 0.5;
      let u = t0 + PI / 4.0;
      let s4 = sin(4.0 * u);
      let r = (1.0 + 0.8 * cos(4.0 * u)) * (1.0 - cleft * s4 * s4);
      return r * vec2<f32>(cos(u), sin(u));
    }
    case 3: {
      let r = max(st, 0.0) * (1.0 + (sm * 0.1) * cos(40.0 * t));
      return r * vec2<f32>(ct * 0.7, st);
    }
    case 4: {
      let r = 0.6 * (1.2 + cos((8.0 + sm * 16.0) * t));
      return r * vec2<f32>(ct, st);
    }
    case 5: {
      let r = (exp(ct) - 2.0 * cos(4.0 * t) + java_pow(sin(t / 12.0), 5.0)) * (0.7 + sm * 0.6);
      return r * vec2<f32>(st, -ct);
    }
    case 6: {
      let r = (1.0 + 0.8 * st) * (1.0 + (0.1 + sm * 0.2) * cos(14.0 * t));
      return r * vec2<f32>(st, -ct);
    }
    case 7: {
      let r = (1.0 - 0.9 * st) * (1.0 + 0.02 * cos(60.0 * t));
      return r * vec2<f32>(ct * (0.6 + sm * 0.4), st);
    }
    case 8: {
      let r = 1.0 + 0.05 * cos(12.0 * t0);
      return vec2<f32>(r * cos(t0), r * sin(t0) + (0.1 + sm * 0.4) * (1.0 - r));
    }
    case 9: {
      let r = (1.0 - 0.5 * st) * (1.0 + (0.1 + sm * 0.4) * cos(5.0 * t)) * (1.0 + 0.05 * cos(25.0 * t));
      return r * vec2<f32>(st, -ct);
    }
    case 10: {
      var r = 0.0;
      if (st > 0.0) { r = st * (1.0 + (0.05 + sm * 0.3) * cos(30.0 * t)); }
      return r * vec2<f32>(ct * 0.6, st);
    }
    case 11: {
      let base = max(1.0 - 0.9 * st, 0.0);
      let fen = (1.0 - (sm * 0.8) * java_pow(sin(t * 2.5), 10.0)) * (1.0 - (sm * 0.7) * java_pow(cos(t * 3.5), 10.0));
      return base * fen * vec2<f32>(ct, st);
    }
    case 12: {
      let r = 1.0 + (0.2 + sm * 0.4) * cos(8.0 * t);
      return r * vec2<f32>(ct, st);
    }
    case 13: {
      let r = 1.0 - (0.15 + sm * 0.2) * abs(sin(t * 5.0));
      return r * vec2<f32>(ct * 0.7, st);
    }
    case 14: {
      let r = (1.0 - (0.3 + sm * 0.4) * abs(sin(t * 2.5))) * (1.0 + 0.04 * cos(40.0 * t));
      return r * vec2<f32>(ct, st);
    }
    case 15: {
      let lobe = 0.1 + sm * 0.2;
      let r = (1.0 + lobe * cos(3.0 * t) - (lobe * 0.5) * cos(5.0 * t)) * (1.0 + 0.03 * cos(20.0 * t) + 0.02 * cos(35.0 * t));
      return r * vec2<f32>(ct, st);
    }
    case 16: {
      let r = (1.0 + (0.3 + sm * 0.5) * cos(7.0 * t)) * (1.0 + 0.05 * cos(42.0 * t));
      return r * vec2<f32>(ct, st);
    }
    case 17: {
      let r = (1.0 - 0.9 * st) * (1.0 + (0.02 + sm * 0.05) * sin(10.0 * t));
      return r * vec2<f32>(ct * 1.2, st);
    }
    case 18: {
      let c0 = cos(t0);
      let r = (1.0 - sin(t0)) * (1.0 - (0.1 + sm * 0.4) * c0 * c0) * (1.0 + 0.03 * sin(15.0 * t0));
      return r * vec2<f32>(c0 * 0.7, sin(t0));
    }
    case 19: {
      let r = 0.8 * (1.1 + cos((20.0 + sm * 40.0) * t) * 0.3);
      return r * vec2<f32>(ct, st);
    }
    case 20: {
      let r = 1.0 + st + (0.2 + sm * 0.6) * sin(5.0 * t - HALF_PI);
      return r * vec2<f32>(ct, st);
    }
    case 21: {
      let r = (1.0 - 0.9 * sin(t0)) * (1.0 + 0.05 * cos(20.0 * t0)) * (1.0 + (0.02 + sm * 0.03) * cos(40.0 * t0));
      return r * vec2<f32>(cos(t0) * 0.8, sin(t0));
    }
    case 22: {
      let r = java_pow(abs(ct), 0.3) + java_pow(abs(st), 2.0 + sm * 2.0);
      return r * vec2<f32>(st, -ct * 0.6);
    }
    case 23: {
      let r = 1.0 - 0.9 * sin(t + (sm - 0.5) * 0.2);
      return r * vec2<f32>(st * 1.1, -ct);
    }
    default: {
      let u = t0 * (2.0 + sm * 4.0) * PI;
      return 0.05 * u * vec2<f32>(cos(u), sin(u));
    }
  }
}
@body
var r = AMOUNT;
if (D(1) > 0.0 && D(1) > rnd()) { r = AMOUNT * rnd(); }
var l = flora_leaf(i32(D(0)), D(4), atan2(a.y, a.x));
if (D(3) != 0.0) { l += D(3) * vec2<f32>(sin(a.y * 5.0), cos(a.x * 5.0)); }
v.x += r * l.x * D(2);
v.y += r * l.y * D(2);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "glsl_acrilic",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 10000.0, 0.0, 100000.0),
            ParamDef::new("time", 0.0),
            ParamDef::int("Steps", 10.0, 3.0, 100.0),
            ParamDef::range("p1", 12.0, 1.0, 100.0),
            ParamDef::range("p2", 12.0, 1.0, 100.0),
            ParamDef::range("p3", 12.0, 1.0, 100.0),
            ParamDef::range("p4", 12.0, 1.0, 100.0),
            ParamDef::range("p5", 75.0, 1.0, 100.0),
            ParamDef::range("p6", 75.0, 1.0, 100.0),
            ParamDef::range("Red Fac.", 1.0, 0.0, 5.0),
            ParamDef::range("Green Fac.", 1.0, 0.0, 5.0),
            ParamDef::range("Blue Fac.", 1.0, 0.0, 5.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let (time, steps) = (d[2], d[3] as i32);
            let mut p = vec2(0.7 * (px.0 + 0.5) / d[0], 0.7 * (px.1 + 0.5) / GLSL_RES_Y);
            let mut col = [0.0; 3];
            for (j, c) in col.iter_mut().enumerate() {
                let j = j as i32;
                for i in 1..steps {
                    let fi = i as f64;
                    p.x += 0.1 / (i + j) as f64 * (fi * d[4] * p.y + time + ((time / (d[5] * fi)) * fi + j as f64).cos()).sin();
                    p.y += 0.1 / (i + j) as f64 * (fi * d[6] * p.x + time + ((time / (d[7] * fi)) * fi + j as f64).sin()).cos();
                }
                *c = (d[8] * (p.x * p.x)).sin() + (d[9] * (p.y * p.y)).sin();
            }
            let color = vec3((col[0] * d[10]).cos(), (col[1] * d[11]).cos(), (col[2] * d[12]).cos());
            glsl_out(v, amount, px, d[0], color, d[13], glsl_red_ratio);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[3] = limit_value(p[3], 3.0, 100.0).trunc();
            for i in 4..10 {
                d[i] = limit_value(p[i], 1.0, 100.0);
            }
            for i in 10..13 {
                d[i] = limit_value(p[i], 0.0, 5.0);
            }
            d[13] = limit_value(p[13], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let tm = D(2);
let steps = i32(D(3));
var p = vec2<f32>(0.7 * (gi + 0.5) / D(0), 0.7 * (gj + 0.5) / 1000000.0);
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
let color = vec3<f32>(cos(col.x * D(10)), cos(col.y * D(11)), cos(col.z * D(12)));
if (i32(D(13)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color.x / (color.x + color.y + color.z));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_apollonian",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("seed", 10000.0, 0.0, 100000.0),
            ParamDef::new("time", 0.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let uv = vec2(2.0 * (px.0 + 0.5) / d[0] - 1.0, 2.0 * (px.1 + 0.5) / GLSL_RES_Y - 1.0);
            let time = d[2];
            let m = glsl_rot(Vec3::splat(0.05 * time) + vec3(1.0, 2.0, 3.0));
            let k = 1.2 + 0.1 * (0.1 * time).sin();
            let f1 = 2.0 + 0.25 * (0.3 * time).sin();
            let p = (m * f1) * Vec3::from_vec2(uv * 2.0, 0.0);
            let color = super::glsl::sin(glsl_app(p, k, m)) * 0.6 + 0.5;
            glsl_out(v, amount, px, d[0], color, d[3], |c| c.x * c.y);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[3] = limit_value(p[3], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let uv = vec2<f32>(2.0 * (gi + 0.5) / D(0) - 1.0, 2.0 * (gj + 0.5) / 1000000.0 - 1.0);
let tm = D(2);
let m = glsl_rot(vec3<f32>(0.05 * tm) + vec3<f32>(1.0, 2.0, 3.0));
let k = 1.2 + 0.1 * sin(0.1 * tm);
let f1 = 2.0 + 0.25 * sin(0.3 * tm);
let p = (m * f1) * vec3<f32>(uv * 2.0, 0.0);
let color = sin(glsl_app(p, k, m)) * 0.6 + 0.5;
if (i32(D(3)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = color.x * color.y;
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_circlesblue",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 10000.0, 0.0, 100000.0),
            ParamDef::range("time", 1.0, 1.0, 1000.0),
            ParamDef::range("Radiusy", 0.04, 0.0, 1.0),
            ParamDef::int("Bubles", 40.0, 1.0, 100.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let uv = vec2(2.0 * (px.0 + 0.5) / d[0] - 1.0, 2.0 * (px.1 + 0.5) / GLSL_RES_Y - 1.0);
            let time = d[2];
            let mut color = Vec3::splat(0.0);
            for i in 0..d[4] as i32 {
                let fi = i as f64;
                let pha = (fi * 6.0 + 1.0).tan() * 0.5 + 0.5;
                let siz = ((fi * 2.4 + 5.0).cos() * 0.5 + 0.5).powf(4.0);
                let pox = (fi * 3.55 + 4.1).cos();
                let rad = d[3] + fi.sin() * 0.12 + 0.08;
                let pos = vec2(pox + (time / 15.0 + pha + siz).sin(), -1.0 - rad + (2.0 + 2.0 * rad) * glsl_mod(pha + 0.1 * (time / 5.0) * (0.2 + 0.8 * siz), 1.0));
                let dis = length(uv - pos);
                let col = mix(vec3(0.1, 0.2, 0.8), vec3(0.2, 0.8, 0.6), 0.5 + 0.5 * (fi * (time * pox * 0.03).sin() + 1.9).sin());
                color += col * (1.0 - smoothstep(rad * (0.65 + 0.20 * (pox * time).sin()), rad, dis)) * (1.0 - (pox * time).cos());
            }
            glsl_out(v, amount, px, d[0], color * 0.3, d[5], glsl_red_ratio);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[2] = limit_value(p[2], 1.0, 1000.0);
            d[3] = limit_value(p[3], 0.0, 1.0);
            d[4] = limit_value(p[4], 1.0, 100.0).trunc();
            d[5] = limit_value(p[5], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let uv = vec2<f32>(2.0 * (gi + 0.5) / D(0) - 1.0, 2.0 * (gj + 0.5) / 1000000.0 - 1.0);
let tm = D(2);
var color = vec3<f32>(0.0);
let nb = i32(D(4));
for (var i = 0; i < nb; i++) {
  let fi = f32(i);
  let pha = tan(fi * 6.0 + 1.0) * 0.5 + 0.5;
  let siz = pow(cos(fi * 2.4 + 5.0) * 0.5 + 0.5, 4.0);
  let pox = cos(fi * 3.55 + 4.1);
  let rad = D(3) + sin(fi) * 0.12 + 0.08;
  let pos = vec2<f32>(pox + sin(tm / 15.0 + pha + siz), -1.0 - rad + (2.0 + 2.0 * rad) * glsl_mod(pha + 0.1 * (tm / 5.0) * (0.2 + 0.8 * siz), 1.0));
  let dis = length(uv - pos);
  let col = mix(vec3<f32>(0.1, 0.2, 0.8), vec3<f32>(0.2, 0.8, 0.6), 0.5 + 0.5 * sin(fi * sin(tm * pox * 0.03) + 1.9));
  color += col * (1.0 - glsl_smoothstep(rad * (0.65 + 0.20 * sin(pox * tm)), rad, dis)) * (1.0 - cos(pox * tm));
}
color *= 0.3;
if (i32(D(5)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color.x / (color.x + color.y + color.z));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_circuits",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::new("time", 0.0),
            ParamDef::new("rate", 0.8),
            ParamDef::new("intensity", 0.9),
            ParamDef::new("focus", 1.5),
            ParamDef::new("pulse", 10.0),
            ParamDef::new("glow", 2.0),
            ParamDef::new("loops", 15.0),
            ParamDef::new("zoom", 1.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            // JWildfire's `S` starts at (101 + glow) * intensity of the default parameters
            let key = d.as_ptr() as usize;
            let mut sv = ctx.orbits.get(&key).map_or((101.0 + 2.0) * 0.9, |st| st[0]);
            let color = glsl_circuits_rgb(d, px, &mut sv);
            ctx.orbits.insert(key, [sv, 0.0]);
            glsl_out(v, amount, px, d[0], color, d[9], |c| c.x * c.y);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[9] = limit_value(p[9], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "fn glsl_circuits_formula(z_in: vec2<f32>, t: f32, loops: f32, zoom: f32, intensity: f32, pulse: f32) -> vec4<f32> {
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
var<private> glsl_circuits_s: f32;
@body
let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let pos = vec2<f32>((gi + 0.5) / D(0) - 0.5, (gj + 0.5) / 1000000.0 - 0.5);
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
var q = pos;
var sph = length(q) * 0.1;
sph = sqrt(1.0 - sph * sph) * 2.0;
let ca = tt * PI;
let cb = ca + tt;
let cc = cos(ca) + sin(cb);
q = q * mat2x2<f32>(cos(cb), sin(cb), -sin(cb), cos(cb));
q = q * mat2x2<f32>(cos(ca), -sin(ca), sin(ca), cos(ca));
q = q - vec2<f32>(sin(cc), cos(cc)) / PI;
q = q * zz;
let pix = 0.5 / D(0) * zz / sph;
let dof = zoom * D(4) + tt * 0.25;
var ll = floor(loops);
var s = glsl_circuits_s;
var color = vec3<f32>(0.0);
for (var ai = 0; ai < 24; ai++) {
  let aauv = floor(vec2<f32>(f32(ai) / 6.0, glsl_mod(f32(ai), 6.0)));
  let r = glsl_circuits_formula(q + aauv * pix * dof, tt, loops, zoom, D(3), D(5));
  color = r.xyz;
  s += r.w;
  if (ll <= 0.0) { break; }
  ll -= 1.0;
}
s /= floor(loops);
glsl_circuits_s = s;
color = color / floor(loops);
let colo = mix(vec3<f32>(0.15), color, s) * (1.0 - length(pos)) * vec3<f32>(1.2, 1.1, 1.0);
if (i32(D(9)) == 0) {
  let t = glsl_dbl2int(colo);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = colo.x * colo.y;
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_fractaldots",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("iterations", 9.0, 1.0, 100.0),
            ParamDef::new("DotSize", 400.0),
            ParamDef::new("MaxIterations", 10.0),
            ParamDef::new("complexity", 0.001245675),
            ParamDef::new("pattern", 2.5),
            ParamDef::new("spacing", 12.0),
            ParamDef::new("rotate1", 1.5),
            ParamDef::new("rotate2", 64.0),
            ParamDef::new("zoom", 64.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let rot = |uv: Vec2, a: f64| vec2(uv.x * a.cos() - uv.y * a.sin(), uv.y * a.cos() + uv.x * a.sin());
            let mut uv = vec2((px.0 + 0.5) / d[0] - 0.5, (px.1 + 0.5) / GLSL_RES_Y - 0.5);
            uv = rot(uv, d[7]) * d[9];
            let mut s = d[6];
            let mut i = 0;
            while (i as f64) < d[3] {
                uv = super::glsl::abs(uv) - s;
                uv -= d[4];
                uv = rot(uv, d[8]);
                s /= d[5];
                if (d[1] as i32) < i {
                    break;
                }
                i += 1;
            }
            // circleSize keeps its initial value (DotSize 400, MaxIterations 10) in JWildfire
            let c = if length(uv) > 400.0 / (3.0 * 2f64.powf(10.0)) { 0.0 } else { 1.0 };
            glsl_out(v, amount, px, d[0], Vec3::splat(c), d[10], |c| c.x * c.y);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[1] = limit_value(p[1], 1.0, 100.0).trunc();
            d[10] = limit_value(p[10], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
var uv = vec2<f32>((gi + 0.5) / D(0) - 0.5, (gj + 0.5) / 1000000.0 - 0.5);
let r1 = D(7);
uv = vec2<f32>(uv.x * cos(r1) - uv.y * sin(r1), uv.y * cos(r1) + uv.x * sin(r1)) * D(9);
var s = D(6);
let r2 = D(8);
var i = 0;
loop {
  if (f32(i) >= D(3)) { break; }
  uv = abs(uv) - s - D(4);
  uv = vec2<f32>(uv.x * cos(r2) - uv.y * sin(r2), uv.y * cos(r2) + uv.x * sin(r2));
  s /= D(5);
  if (i32(D(1)) < i) { break; }
  i++;
}
let c = select(1.0, 0.0, length(uv) > 400.0 / 3072.0);
if (i32(D(10)) == 0) {
  let t = glsl_dbl2int(vec3<f32>(c));
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = c * c;
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_grid3D",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 10000.0, 0.0, 100000.0),
            ParamDef::range("time", 200.0, 1.0, 1000.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            const MAXITER: i32 = 30;
            let time = d[2];
            let mut dir = normalize(vec3((px.0 + 0.5) / d[0] - 0.5, (px.1 + 0.5) / GLSL_RES_Y - 0.5, 1.0));
            let a = time * 0.021;
            let mut pos = vec3(0.0, time * 0.1, 0.0);
            dir = dir * mat3(1.0, 0.0, 0.0, 0.0, a.cos(), -a.sin(), 0.0, a.sin(), a.cos());
            dir = dir * mat3(a.cos(), 0.0, -a.sin(), 0.0, 1.0, 0.0, a.sin(), 0.0, a.cos());
            let mut color = Vec3::splat(0.0);
            for i in 0..MAXITER {
                let f2 = glsl_grid3d_field(pos);
                let f = java_min(java_min(f2.x, f2.y), f2.z);
                pos += dir * f;
                color += Vec3::splat((MAXITER - i) as f64) / (f2 + 0.1);
            }
            let c3 = Vec3::splat(1.0) - Vec3::splat(1.0) / (Vec3::splat(1.0) + color * (0.09 / (MAXITER * MAXITER) as f64));
            glsl_out(v, amount, px, d[0], Vec3::splat(c3.x + c3.y + c3.z), 0.0, |_| 0.0);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[2] = limit_value(p[2], 1.0, 1000.0);
            d
        }),
        wgsl: "fn glsl_grid3D_field(p_in: vec3<f32>) -> vec3<f32> {
  var p = p_in * 0.1;
  var f = 0.1;
  for (var i = 0; i < 3; i++) {
    p = abs(fract(p.yzx) - 0.5) * 2.0;
    f *= 2.0;
  }
  p = p * p;
  return sqrt(p + p.yzx) / f - 0.05;
}
@body
let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let tm = D(2);
var dir = normalize(vec3<f32>((gi + 0.5) / D(0) - 0.5, (gj + 0.5) / 1000000.0 - 0.5, 1.0));
let a = tm * 0.021;
var pos = vec3<f32>(0.0, tm * 0.1, 0.0);
dir = dir * mat3x3<f32>(1.0, 0.0, 0.0, 0.0, cos(a), -sin(a), 0.0, sin(a), cos(a));
dir = dir * mat3x3<f32>(cos(a), 0.0, -sin(a), 0.0, 1.0, 0.0, sin(a), 0.0, cos(a));
var color = vec3<f32>(0.0);
for (var i = 0; i < 30; i++) {
  let f2 = glsl_grid3D_field(pos);
  pos += dir * min(min(f2.x, f2.y), f2.z);
  color += vec3<f32>(f32(30 - i)) / (f2 + 0.1);
}
let c3 = vec3<f32>(1.0) - vec3<f32>(1.0) / (vec3<f32>(1.0) + color * (0.09 / 900.0));
let t = glsl_dbl2int(vec3<f32>(c3.x + c3.y + c3.z));
v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_hoshi",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 100000.0, 0.0, 10000.0),
            ParamDef::new("time", 10.0),
            ParamDef::int("Steps", 28.0, 1.0, 60.0),
            ParamDef::range("Scale", 1.25, 1.0, 5.0),
            ParamDef::range("Translate", 1.5, 0.0, 5.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let time = d[2];
            let fold = vec2(-0.5, -0.5);
            let mut p = vec2(2.0 * px.0 / d[0] - 1.0, 2.0 * px.1 / GLSL_RES_Y - 1.0) * 0.182;
            let x = p.y;
            p = super::glsl::abs(glsl_mod(p, 4.0) - 2.0);
            let mut i = d[3] as i32;
            while i > 0 {
                p = super::glsl::abs(p - fold) + fold;
                p = p * d[4] - d[5];
                let a = 3.14159 / (0.10 + (time * 0.0005 + i as f64 * 0.5000001).sin() * 0.4999 + 0.5 + (10.0 / time) + time.sin() / 100.0);
                p = vec2(p.x * a.cos() - p.y * a.sin(), p.x * a.sin() + p.y * a.cos());
                i -= 1;
            }
            let i = x * x + glsl_atan2(p.y, p.x) + time * 0.02;
            let mut h = (i * 4.0).floor() / 8.0 + 1.107;
            h += smoothstep(-0.1, 0.8, glsl_mod(i * 2.0 / 5.0, 1.0 / 4.0) * 900.0) / 0.010 - 0.5;
            // hsv(h, 1, value)
            let value = smoothstep(-3.0, 3.0, length(p) * 1.0);
            let t2 = super::glsl::clamp(super::glsl::abs(fract((vec3(3.0, 2.0, 1.0) + h) / 3.0) * 6.0 - 3.0) - 1.0, 0.0, 1.0);
            let color = mix(Vec3::splat(3.1), t2, 1.0) * value;
            glsl_out(v, amount, px, d[0], color, d[6], |c| c.x * c.y);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[3] = limit_value(p[3], 1.0, 60.0).trunc();
            d[4] = limit_value(p[4], 1.0, 5.0);
            d[5] = limit_value(p[5], 0.0, 5.0);
            d[6] = limit_value(p[6], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let tm = D(2);
var p = vec2<f32>(2.0 * gi / D(0) - 1.0, 2.0 * gj / 1000000.0 - 1.0) * 0.182;
let x = p.y;
p = abs(glsl_mod2(p, vec2<f32>(4.0)) - 2.0);
for (var i = i32(D(3)); i > 0; i--) {
  p = abs(p + 0.5) - 0.5;
  p = p * D(4) - D(5);
  let a = 3.14159 / (0.10 + sin(tm * 0.0005 + f32(i) * 0.5000001) * 0.4999 + 0.5 + (10.0 / tm) + sin(tm) / 100.0);
  p = vec2<f32>(p.x * cos(a) - p.y * sin(a), p.x * sin(a) + p.y * cos(a));
}
let ii = x * x + glsl_atan2(p.y, p.x) + tm * 0.02;
var h = floor(ii * 4.0) / 8.0 + 1.107;
h += glsl_smoothstep(-0.1, 0.8, glsl_mod(ii * 2.0 / 5.0, 0.25) * 900.0) / 0.010 - 0.5;
let value = glsl_smoothstep(-3.0, 3.0, length(p));
let color = clamp(abs(fract((vec3<f32>(3.0, 2.0, 1.0) + h) / 3.0) * 6.0 - 3.0) - 1.0, vec3<f32>(0.0), vec3<f32>(1.0)) * value;
if (i32(D(6)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = color.x * color.y;
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_hyperbolictile",
        params: &[ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0), ParamDef::int("Gradient", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let position = vec2(2.0 * (px.0 + 0.5) / d[0] - 1.0, 2.0 * (px.1 + 0.5) / GLSL_RES_Y - 1.0);
            let l1 = vec2(1000.0, 0.0);
            let l2 = vec2(1000.0, M_PI / 8.0);
            let l3 = vec2((2f64.sqrt() + 1.0).sqrt(), M_PI / 2.0);
            let mut p = glsl_hyp_topolar(position);
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
                        if k == 0 || (p.y >= M_PI / 2.0 && p.y < M_PI * 0.75) {
                            k = 0;
                        } else {
                            g2 += 1;
                            p = glsl_hyp_mirror(l2, glsl_hyp_mirror(l1, p));
                        }
                    }
                    let p1 = glsl_hyp_mirror(l3, p);
                    if p1.y >= M_PI / 2.0 && p1.y < M_PI * 0.75 {
                        k1 = 0;
                    }
                    p = p1;
                }
                glsl_mod(g2 as f64, 2.0)
            };
            glsl_out(v, amount, px, d[0], Vec3::splat(color), d[1], glsl_red_ratio);
        },
        prepare: Some(|p, _| vec![glsl_density(p[0]), limit_value(p[1], 0.0, 1.0).trunc()]),
        wgsl: "fn glsl_hyperbolictile_topolar(c: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(length(c), glsl_atan2(c.y, c.x));
}
fn glsl_hyperbolictile_mirror(line: vec2<f32>, point: vec2<f32>) -> vec2<f32> {
  if (line.x < 100.0) {
    let lc = line.x * vec2<f32>(cos(line.y), sin(line.y));
    let cart = point.x * vec2<f32>(cos(point.y), sin(point.y));
    let dd = cart - lc;
    return glsl_hyperbolictile_topolar(lc + dd * ((line.x * line.x - 1.0) / dot(dd, dd)));
  }
  return vec2<f32>(point.x, -point.y + 2.0 * line.y + PI);
}
@body
let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let position = vec2<f32>(2.0 * (gi + 0.5) / D(0) - 1.0, 2.0 * (gj + 0.5) / 1000000.0 - 1.0);
let l1 = vec2<f32>(1000.0, 0.0);
let l2 = vec2<f32>(1000.0, PI / 8.0);
let l3 = vec2<f32>(sqrt(sqrt(2.0) + 1.0), PI / 2.0);
var p = glsl_hyperbolictile_topolar(position);
var color = 0.0;
if (p.x <= 1.0) {
  var g2 = 0;
  var k1 = 1;
  for (var i = 0; i < 7; i++) {
    if (k1 == 0) { continue; }
    var k = 1;
    for (var j = 1; j < 8; j++) {
      if (k == 0 || (p.y >= PI / 2.0 && p.y < PI * 0.75)) {
        k = 0;
      } else {
        g2 += 1;
        p = glsl_hyperbolictile_mirror(l2, glsl_hyperbolictile_mirror(l1, p));
      }
    }
    let p1 = glsl_hyperbolictile_mirror(l3, p);
    if (p1.y >= PI / 2.0 && p1.y < PI * 0.75) { k1 = 0; }
    p = p1;
  }
  color = f32(g2 % 2);
}
if (i32(D(1)) == 0) {
  let t = glsl_dbl2int(vec3<f32>(color));
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color / (color + color + color));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_kaleidocomplex",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::int("iMax", 12.0, 1.0, 50.0),
            ParamDef::new("Color", 2.0),
            ParamDef::range("Red Fac.", 1.0, 0.0, 5.0),
            ParamDef::range("Green Fac.", 1.0, 0.0, 5.0),
            ParamDef::range("Blue Fac.", 1.0, 0.0, 5.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let (time, fc) = (d[2], d[4]);
            let uv = vec2((px.0 + 0.5) / d[0] - 0.5, (px.1 + 0.5) / GLSL_RES_Y - 0.5);
            let (mut zx, mut zy, mut zz, mut zw) = (uv.x.abs() / 0.125 * 1.0, uv.y.abs() / 0.125 * 1.0, 0.0f64, 0.0f64);
            let mut col = [0.0f64; 3];
            let mut ii = -1i32;
            let mut r = 1.0;
            let (st, ct) = (time.sin(), time.cos());
            for _ in -1..d[3] as i32 {
                r *= -1.0;
                ii += 1;
                let (tx, ty) = (zx * 1.0 - zy * 1.0, zx * 1.0 + zy * 1.0);
                let (tx, ty) = (tx.abs() + (r - 10.5), ty.abs() + (r - 10.5));
                zx = tx * st - ty * ct;
                zy = tx * ct + ty * st;
                zz = fc * (zx * zz - zy * zw);
                zw = fc * (zy * zz - zx * zw);
                let dist = (zx * zx + zy * zy, zz * zz + zw * zw);
                if ii as f64 > 0.0 && ((zx * zx).sqrt() < 0.51 || (zy * zy).sqrt() < 0.51) {
                    col[0] = (-(zx * zx * zy).abs()).exp();
                    col[1] = (-(zy * zx * zy).abs()).exp();
                    col[2] = (-java_min(zx, zy).abs()).exp();
                    break;
                }
                let fi = ii as f64;
                col[0] += fc * (-(-zx / fi + fi / zx).abs()).exp();
                col[1] += fc * (-(-zy / fi + fi / zy).abs()).exp();
                col[2] += fc * (-((-zx - zy) / fi + fi / (zx + zy)).abs()).exp();
                if dist.0 > 10000000.0 || dist.1 > 100000000000.0 {
                    break;
                }
            }
            let color = vec3((col[0] * d[5]).cos(), (col[1] * d[6]).cos(), (col[2] * d[7]).cos());
            glsl_out(v, amount, px, d[0], color, d[8], glsl_red_ratio);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[3] = limit_value(p[3], 1.0, 50.0).trunc();
            for i in 5..8 {
                d[i] = limit_value(p[i], 0.0, 5.0);
            }
            d[8] = limit_value(p[8], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let fc = D(4);
let uv = vec2<f32>((gi + 0.5) / D(0) - 0.5, (gj + 0.5) / 1000000.0 - 0.5);
var z = vec4<f32>(abs(uv.x) / 0.125, abs(uv.y) / 0.125, 0.0, 0.0);
var col = vec3<f32>(0.0);
var ii = -1;
var r = 1.0;
let st = sin(D(2));
let ct = cos(D(2));
let imax = i32(D(3));
for (var i = -1; i < imax; i++) {
  r *= -1.0;
  ii += 1;
  let t = abs(vec2<f32>(z.x - z.y, z.x + z.y)) + (r - 10.5);
  z.x = t.x * st - t.y * ct;
  z.y = t.x * ct + t.y * st;
  z.z = fc * (z.x * z.z - z.y * z.w);
  z.w = fc * (z.y * z.z - z.x * z.w);
  let d1 = dot(z.xy, z.xy);
  let d2 = dot(z.zw, z.zw);
  if (ii > 0 && (abs(z.x) < 0.51 || abs(z.y) < 0.51)) {
    col = vec3<f32>(exp(-abs(z.x * z.x * z.y)), exp(-abs(z.y * z.x * z.y)), exp(-abs(min(z.x, z.y))));
    break;
  }
  let fi = f32(ii);
  col.x += fc * exp(-abs(-z.x / fi + fi / z.x));
  col.y += fc * exp(-abs(-z.y / fi + fi / z.y));
  col.z += fc * exp(-abs((-z.x - z.y) / fi + fi / (z.x + z.y)));
  if (d1 > 10000000.0 || d2 > 100000000000.0) { break; }
}
let color = cos(col * vec3<f32>(D(5), D(6), D(7)));
if (i32(D(8)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color.x / (color.x + color.y + color.z));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_kaleidoscopic",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 10000.0, 0.0, 100000.0),
            ParamDef::new("time", 0.0),
            ParamDef::int("Sides", 8.0, 2.0, 20.0),
            ParamDef::range("zoom", 0.0, 0.0, 1.0),
            ParamDef::range("P1", 0.0, 0.0, 1.0),
            ParamDef::int("Radial", 0.0, 0.0, 1.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let (time, zoom, p1) = (d[2], d[4], d[5]);
            let ka = M_PI / d[3];
            let mut uv = vec2(2.0 * (px.0 + 0.5) / d[0] - 1.0, 2.0 * (px.1 + 0.5) / GLSL_RES_Y - 1.0) * 12.0;
            uv *= 0.1 + zoom;
            if d[6] as i32 == 1 {
                let angle = (glsl_mod(glsl_atan2(uv.y, uv.x), 2.0 * ka) - ka).abs() + 0.1 * time;
                uv = vec2(angle.cos(), angle.sin()) * length(uv);
            }
            let mut p = Vec3::from_vec2(uv, zoom);
            for _ in 0..44 {
                let t1 = super::glsl::abs(p) / (p.x * p.x + p.y * p.y + p.z * p.z);
                let t0 = t1 - vec3(1.0, 1.02, p1 * 0.4);
                let t2 = vec3(1.3, 0.999, 0.678) * super::glsl::abs(t0);
                p = vec3(t2.x, t2.z, t2.y);
            }
            glsl_out(v, amount, px, d[0], p, d[7], glsl_red_ratio);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[3] = limit_value(p[3], 2.0, 20.0).trunc();
            d[4] = limit_value(p[4], 0.0, 1.0);
            d[5] = limit_value(p[5], 0.0, 1.0);
            d[6] = limit_value(p[6], 0.0, 1.0).trunc();
            d[7] = limit_value(p[7], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let ka = PI / D(3);
var uv = vec2<f32>(2.0 * (gi + 0.5) / D(0) - 1.0, 2.0 * (gj + 0.5) / 1000000.0 - 1.0) * 12.0;
uv = uv * (0.1 + D(4));
if (i32(D(6)) == 1) {
  let angle = abs(glsl_mod(glsl_atan2(uv.y, uv.x), 2.0 * ka) - ka) + 0.1 * D(2);
  uv = vec2<f32>(cos(angle), sin(angle)) * length(uv);
}
var p = vec3<f32>(uv, D(4));
for (var i = 0; i < 44; i++) {
  let t2 = vec3<f32>(1.3, 0.999, 0.678) * abs(abs(p) / dot(p, p) - vec3<f32>(1.0, 1.02, D(5) * 0.4));
  p = t2.xzy;
}
let color = p;
if (i32(D(7)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color.x / (color.x + color.y + color.z));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_kaliset",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 10000.0, 0.0, 100000.0),
            ParamDef::range("time", 200.0, 1.0, 1000.0),
            ParamDef::int("N", 60.0, 1.0, 100.0),
            ParamDef::range("ShiftX", -0.22, -5.0, 5.0),
            ParamDef::range("ShiftY", -0.21, -5.0, 5.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let time = d[2];
            let mut w = vec2((px.0 + 0.5) / d[0] - 0.5, (px.1 + 0.5) / GLSL_RES_Y - 0.5) * 20.0;
            let mut rsum = 0.0;
            let pi2 = 3.6 * 2.0;
            let c = (time / 603.0 * pi2).cos();
            let s = (time / 407.4 * pi2).sin();
            let zoom = time / 184.0 + 1.0;
            for _ in 0..d[3] as i32 {
                let mut rr = w.x * w.x + w.y * w.y;
                if rr > 1.0 {
                    rr = 1.0 / rr;
                    w.x *= rr;
                    w.y *= rr;
                }
                rsum *= 0.99;
                rsum += rr;
                w = vec2(c * w.x - s * w.y, s * w.x + c * w.y) * zoom + vec2(d[4], d[5]);
            }
            let col = rsum * 0.5;
            glsl_out(v, amount, px, d[0], vec3(col.cos(), (col * 2.0).cos(), (col * 4.0).cos()), d[6], glsl_red_ratio);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[2] = limit_value(p[2], 1.0, 1000.0);
            d[3] = limit_value(p[3], 1.0, 100.0).trunc();
            d[4] = limit_value(p[4], -5.0, 5.0);
            d[5] = limit_value(p[5], -5.0, 5.0);
            d[6] = limit_value(p[6], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let tm = D(2);
var w = vec2<f32>((gi + 0.5) / D(0) - 0.5, (gj + 0.5) / 1000000.0 - 0.5) * 20.0;
var rsum = 0.0;
let c = cos(tm / 603.0 * 7.2);
let s = sin(tm / 407.4 * 7.2);
let zoom = tm / 184.0 + 1.0;
let n = i32(D(3));
for (var i = 0; i < n; i++) {
  var rr = dot(w, w);
  if (rr > 1.0) {
    rr = 1.0 / rr;
    w *= rr;
  }
  rsum = rsum * 0.99 + rr;
  w = vec2<f32>(c * w.x - s * w.y, s * w.x + c * w.y) * zoom + vec2<f32>(D(4), D(5));
}
let col = rsum * 0.5;
let color = cos(vec3<f32>(col, col * 2.0, col * 4.0));
if (i32(D(6)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color.x / (color.x + color.y + color.z));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_kaliset2",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 5000.0, 0.0, 100000.0),
            ParamDef::range("time", 85.5, 1.0, 1000.0),
            ParamDef::int("N", 100.0, 1.0, 100.0),
            ParamDef::range("radio", 1.0, 0.1, 10.0),
            ParamDef::range("ShiftX", -0.356, -1.0, 1.0),
            ParamDef::range("ShiftY", 0.686, -1.0, 1.0),
            ParamDef::range("Red Fac.", 1.8, 0.0, 5.0),
            ParamDef::range("Green Fac.", 1.9, 0.0, 5.0),
            ParamDef::range("Blue Fac.", 2.2, 0.0, 5.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let (time, n, rad2) = (d[2], d[3], d[4]);
            let mut w = vec2((px.0 + 0.5) / d[0] - 0.5, (px.1 + 0.5) / GLSL_RES_Y - 0.5) * 20.0;
            let mut rsum = 0.0;
            let angle = 6.2831853070 * d[5];
            let (c, s) = (angle.cos(), angle.sin());
            let shift = vec2(0.0, 2.0 + 2.0 * (0.03 * time).sin());
            let zoom = 2.0 + 5.0 * d[6];
            for _ in 0..n as i32 {
                let mut rr = w.x * w.x + w.y * w.y;
                if rr > rad2 {
                    rr = rad2 / rr;
                    w.x *= rr;
                    w.y *= rr;
                }
                rsum = java_max(rsum, rr);
                w = vec2(c * w.x - s * w.y, s * w.x + c * w.y) * zoom + shift;
            }
            let col = rsum * rsum * (500.0 / n / rad2);
            glsl_out(v, amount, px, d[0], vec3((col * d[7]).cos(), (col * d[8]).cos(), (col * d[9]).cos()), d[10], glsl_red_ratio);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[2] = limit_value(p[2], 1.0, 1000.0);
            d[3] = limit_value(p[3], 1.0, 100.0).trunc();
            d[4] = limit_value(p[4], 0.1, 10.0);
            d[5] = limit_value(p[5], -1.0, 1.0);
            d[6] = limit_value(p[6], -1.0, 1.0);
            for i in 7..10 {
                d[i] = limit_value(p[i], 0.0, 5.0);
            }
            d[10] = limit_value(p[10], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let rad2 = D(4);
var w = vec2<f32>((gi + 0.5) / D(0) - 0.5, (gj + 0.5) / 1000000.0 - 0.5) * 20.0;
var rsum = 0.0;
let angle = 6.2831853070 * D(5);
let c = cos(angle);
let s = sin(angle);
let shift = vec2<f32>(0.0, 2.0 + 2.0 * sin(0.03 * D(2)));
let zoom = 2.0 + 5.0 * D(6);
let n = i32(D(3));
for (var i = 0; i < n; i++) {
  var rr = dot(w, w);
  if (rr > rad2) {
    rr = rad2 / rr;
    w *= rr;
  }
  rsum = max(rsum, rr);
  w = vec2<f32>(c * w.x - s * w.y, s * w.x + c * w.y) * zoom + shift;
}
let col = rsum * rsum * (500.0 / D(3) / rad2);
let color = cos(col * vec3<f32>(D(7), D(8), D(9)));
if (i32(D(10)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color.x / (color.x + color.y + color.z));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glsl_mandala",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::new("mX", 0.025),
            ParamDef::new("mY", -0.001245675),
            ParamDef::new("scale", 2.0),
            ParamDef::new("sides", 12.0),
            ParamDef::new("multiply", 1.5),
            ParamDef::new("loops", 64.0),
            ParamDef::int("iR", 0.0, 0.0, 1.0),
            ParamDef::int("iG", 0.0, 0.0, 1.0),
            ParamDef::int("iB", 1.0, 0.0, 1.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let rcpi = 0.318309886183791;
            let uv = vec2((5.5 - d[3]) * (2.0 * (px.0 + 0.5) / d[0] - 1.0), (5.5 - d[3]) * (2.0 * (px.1 + 0.5) / GLSL_RES_Y - 1.0));
            let k = M_PI / d[4].floor();
            let s = glsl_kscope(uv, k);
            let t = glsl_kscope(s, k);
            let w = super::glsl::dot(t, s);
            let mut u = mix(s, t, w.cos());
            if d[5] > 0.001 {
                let t2 = glsl_mod(vec2(u.y, u.x), d[5].floor());
                let t5 = vec2(u.y, u.x) + glsl_mod(t2, vec2(-u.x, -u.y));
                u = vec2(t5.y, t5.x);
            }
            let mut p = Vec3::from_vec2(u, d[1] * w);
            for l in 0..73 {
                if l as f64 > d[6].floor() {
                    break;
                }
                let t2 = super::glsl::abs(p) / super::glsl::dot(p, p) - vec3(1.0, 1.02, d[2] * rcpi);
                let t4 = vec3(1.3, 0.999, 0.678) * super::glsl::abs(t2);
                p = vec3(t4.x, t4.z, t4.y);
            }
            // vec3.r/g/b keep the values of construction time, before the inversions
            let idx = p.x * p.y;
            if d[7] as i32 == 1 {
                p.x = 1.0 - p.x;
            }
            if d[8] as i32 == 1 {
                p.y = 1.0 - p.y;
            }
            if d[9] as i32 == 1 {
                p.z = 1.0 - p.z;
            }
            glsl_out(v, amount, px, d[0], p, d[10], |_| idx);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            for i in 7..11 {
                d[i] = limit_value(p[i], 0.0, 1.0).trunc();
            }
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let uv = (5.5 - D(3)) * vec2<f32>(2.0 * (gi + 0.5) / D(0) - 1.0, 2.0 * (gj + 0.5) / 1000000.0 - 1.0);
let k = PI / floor(D(4));
let s = glsl_kscope(uv, k);
let t = glsl_kscope(s, k);
let w = dot(t, s);
var u = mix(s, t, cos(w));
if (D(5) > 0.001) {
  let t2 = glsl_mod2(u.yx, vec2<f32>(floor(D(5))));
  let t5 = u.yx + glsl_mod2(t2, -u);
  u = t5.yx;
}
var p = vec3<f32>(u, D(1) * w);
for (var l = 0; l < 73; l++) {
  if (f32(l) > floor(D(6))) { break; }
  let t2 = abs(p) / dot(p, p) - vec3<f32>(1.0, 1.02, D(2) * 0.318309886183791);
  p = (vec3<f32>(1.3, 0.999, 0.678) * abs(t2)).xzy;
}
let idx = p.x * p.y;
if (i32(D(7)) == 1) { p.x = 1.0 - p.x; }
if (i32(D(8)) == 1) { p.y = 1.0 - p.y; }
if (i32(D(9)) == 1) { p.z = 1.0 - p.z; }
let color = p;
if (i32(D(10)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = idx;
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);"
    },
    VarDef {
        name: "glsl_mandelbox2D",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("seed", 10000.0, 0.0, 10000.0),
            ParamDef::new("time", 0.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let (x, y) = (px.0 + 0.5, px.1 + 0.5);
            let mut ip = vec2(7.0 * (x + x - d[0]) / d[0], 7.0 * (y + y - GLSL_RES_Y) / GLSL_RES_Y);
            let (ox, oy, oz) = (ip.x, ip.y, -5.0 * (d[2] * 0.1).cos());
            let mut dd = 1.0;
            for _ in 0..20 {
                ip = super::glsl::clamp(ip, -1.0, 1.0) * 2.0 - ip;
                let oa = length(ip);
                let b = if oa < 0.5 { 4.0 } else if oa < 1.0 { 1.0 / oa } else { 1.0 };
                ip = ip * (oz * b) + vec2(ox, oy);
                dd = b * dd * oz.abs() + 1.0;
            }
            let dd = (length(ip) / dd).powf(0.1) * 5.0;
            let color = vec3(dd.cos(), (10.0 * dd + 1.0).sin(), (3.0 * dd + 1.0).cos()) * 0.5 + 0.5;
            glsl_out(v, amount, px, d[0], color, d[3], |c| c.x * c.y);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[3] = limit_value(p[3], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let x = gi + 0.5;
let y = gj + 0.5;
var ip = vec2<f32>(7.0 * (x + x - D(0)) / D(0), 7.0 * (y + y - 1000000.0) / 1000000.0);
let o = ip;
let oz = -5.0 * cos(D(2) * 0.1);
var dd = 1.0;
for (var i = 0; i < 20; i++) {
  ip = clamp(ip, vec2<f32>(-1.0), vec2<f32>(1.0)) * 2.0 - ip;
  let oa = length(ip);
  var b = 1.0;
  if (oa < 0.5) { b = 4.0; } else if (oa < 1.0) { b = 1.0 / oa; }
  ip = ip * (oz * b) + o;
  dd = b * dd * abs(oz) + 1.0;
}
let e = java_pow(length(ip) / dd, 0.1) * 5.0;
let color = vec3<f32>(cos(e), sin(10.0 * e + 1.0), cos(3.0 * e + 1.0)) * 0.5 + 0.5;
if (i32(D(3)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = color.x * color.y;
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);"
    },
    VarDef {
        name: "glsl_squares",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 5000.0, 0.0, 100000.0),
            ParamDef::range("time", 85.5, 0.0, 1000.0),
            ParamDef::int("N", 3.0, 1.0, 20.0),
            ParamDef::int("Direction", 1.0, 0.0, 1.0),
            ParamDef::range("Red Fac.", 1.8, 0.0, 5.0),
            ParamDef::range("Green Fac.", 1.9, 0.0, 5.0),
            ParamDef::range("Blue Fac.", 2.2, 0.0, 5.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let time = d[2];
            let c0 = glsl_mod(vec2(((px.0 + 0.5) / d[0] - 0.5).abs(), ((px.1 + 0.5) / GLSL_RES_Y - 0.5).abs()), 1.0);
            let mut color = vec3(time.cos(), time.tan(), time.sin());
            for i in 0..4 {
                if glsl_squares_hit(d, Vec2::splat(i as f64 * 0.1) + c0) {
                    color = Vec3::splat(1.0) - color;
                }
            }
            let color = vec3((color.x * d[5]).cos(), (color.y * d[6]).cos(), (color.z * d[7]).cos());
            glsl_out(v, amount, px, d[0], color, d[8], glsl_red_ratio);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[2] = limit_value(p[2], 0.0, 1000.0);
            d[3] = limit_value(p[3], 1.0, 20.0).trunc();
            d[4] = limit_value(p[4], 0.0, 1.0).trunc();
            for i in 5..8 {
                d[i] = limit_value(p[i], 0.0, 5.0);
            }
            d[8] = limit_value(p[8], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "fn glsl_squares_hit(base: u32, p: vec2<f32>) -> bool {
  var direction = -1.0;
  if (i32(prm[base + 4u]) == 1) { direction = 0.1; }
  var c = p / java_pow(0.5, glsl_mod(direction * prm[base + 2u], 1.0));
  let n = i32(prm[base + 3u]);
  for (var i = 0; i < n; i++) {
    let sectors = floor(c * 3.0);
    if (sectors.x == 1.0 && sectors.y == 1.0) { return false; }
    c = c * 3.0 - sectors;
  }
  return true;
}
@body
let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let tm = D(2);
let c0 = glsl_mod2(abs(vec2<f32>((gi + 0.5) / D(0) - 0.5, (gj + 0.5) / 1000000.0 - 0.5)), vec2<f32>(1.0));
var col = vec3<f32>(cos(tm), tan(tm), sin(tm));
for (var i = 0; i < 4; i++) {
  if (glsl_squares_hit(base, vec2<f32>(f32(i) * 0.1) + c0)) { col = vec3<f32>(1.0) - col; }
}
let color = cos(col * vec3<f32>(D(5), D(6), D(7)));
if (i32(D(8)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color.x / (color.x + color.y + color.z));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);"
    },
    VarDef {
        name: "glsl_starsfield",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 10000.0, 0.0, 100000.0),
            ParamDef::new("time", 0.0),
            ParamDef::range("Z distance", 2.0, 2.0, 150.0),
            // JWildfire starts at 2.0 but clamps every set value (e.g. when loading a flame) to 0..1
            ParamDef::range("Glow", 1.0, 0.0, 1.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let time = d[2];
            let rotate = |a: f64| mat2(a.cos(), a.sin(), -a.sin(), a.cos());
            let mut uv = vec2((px.0 + 0.5) / d[0] - 0.5, (px.1 + 0.5) / GLSL_RES_Y - 0.5);
            let mut col = Vec3::splat(0.0);
            uv = uv * rotate(time);
            uv += vec2(time.cos(), time.sin()) * 2.0;
            let mut i = 0.0;
            while i < 1.0 {
                uv = uv * rotate(glsl_stars_hash11(i) * 6.28);
                let t = fract(i - time);
                let s = smoothstep(0.0, 1.0, t);
                let f = smoothstep(0.0, 1.0, t) * smoothstep(1.0, 0.0, t);
                let k = glsl_stars_hash22(vec2(i, i * 5.0)) * 0.1;
                let l = glsl_stars_layer((uv - k) * s, d[3]);
                col += mix(Vec3::splat(0.0), Vec3::splat(1.0), l) * f;
                i += 0.1;
            }
            col += Vec3::splat(d[4] * glsl_stars_hash21(uv + time));
            glsl_out(v, amount, px, d[0], col, d[5], glsl_red_ratio);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[3] = limit_value(p[3], 2.0, 150.0);
            d[4] = limit_value(p[4], 0.0, 1.0);
            d[5] = limit_value(p[5], 0.0, 1.0).trunc();
            d
        }),
        wgsl: "fn glsl_starsfield_hash21(p_in: vec2<f32>) -> f32 {
  var p = fract(p_in * vec2<f32>(451.45, 231.95));
  p = p + dot(p, p + 78.78);
  return fract(p.x * p.y);
}
fn glsl_starsfield_hash22(p: vec2<f32>) -> vec2<f32> {
  var q = fract(p.xyx * vec3<f32>(451.45, 231.95, 7878.5));
  q = q + dot(q, q + 78.78);
  return fract(q.xz * q.y);
}
fn glsl_starsfield_rot(a: f32) -> mat2x2<f32> {
  return mat2x2<f32>(cos(a), sin(a), -sin(a), cos(a));
}
@body
let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let tm = D(2);
var uv = vec2<f32>((gi + 0.5) / D(0) - 0.5, (gj + 0.5) / 1000000.0 - 0.5);
var col = vec3<f32>(0.0);
uv = uv * glsl_starsfield_rot(tm);
uv = uv + vec2<f32>(cos(tm), sin(tm)) * 2.0;
// Java steps i by 0.1 in double precision: 11 layers (the last at 0.9999999999999999)
for (var n = 0; n < 11; n++) {
  let i = min(f32(n) * 0.1, 0.99999994);
  var h = fract(i * 35.35);
  h += h * (h + 45.85);
  uv = uv * glsl_starsfield_rot(fract(h * 7858.58) * 6.28);
  let t = fract(i - tm);
  let s = glsl_smoothstep(0.0, 1.0, t);
  let f = glsl_smoothstep(0.0, 1.0, t) * glsl_smoothstep(1.0, 0.0, t);
  let k = glsl_starsfield_hash22(vec2<f32>(i, i * 5.0)) * 0.1;
  // layer
  let lu = (uv - k) * s * 5.0;
  let li = floor(lu);
  let lf = fract(lu) * D(3) - 1.0;
  let ld = length(lf - glsl_starsfield_hash22(li) * 0.3);
  let l = glsl_smoothstep(0.1 + 0.8 * glsl_starsfield_hash21(li), 0.01, ld) * (1.0 / ld) * 0.2;
  col += vec3<f32>(l) * f;
}
let color = col + vec3<f32>(D(4) * glsl_starsfield_hash21(uv + tm));
if (i32(D(5)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color.x / (color.x + color.y + color.z));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);"
    },
    VarDef {
        name: "glsl_randomoctree",
        params: &[
            ParamDef::int("Density Pixels", 1000000.0, 100.0, 10000000.0),
            ParamDef::int("Seed", 10000.0, 0.0, 100000.0),
            ParamDef::new("time", 0.0),
            ParamDef::int("Steps", 10.0, 3.0, 100.0),
            ParamDef::range("Rot. L-R", 0.0, 0.0, 1.0),
            ParamDef::range("Rot. U-D", 0.0, 0.0, 1.0),
            ParamDef::int("Grid", 0.0, 0.0, 1.0),
            ParamDef::int("Borders", 1.0, 0.0, 1.0),
            ParamDef::int("Black Borders", 1.0, 0.0, 1.0),
            ParamDef::int("Gradient", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let px = glsl_pixel(ctx, d[0]);
            let color = glsl_randomoctree_rgb(d, px);
            glsl_out(v, amount, px, d[0], color, d[9], glsl_red_ratio);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = glsl_density(p[0]);
            d[3] = limit_value(p[3], 3.0, 100.0).trunc();
            d[4] = limit_value(p[4], 0.0, 1.0);
            d[5] = limit_value(p[5], 0.0, 1.0);
            for i in 6..10 {
                d[i] = limit_value(p[i], 0.0, 1.0).trunc();
            }
            d
        }),
        wgsl: "fn glsl_randomoctree_voxel(p: vec3<f32>, size: f32) -> i32 {
  if (p.x == 0.0 && p.y == 0.0) { return 0; }
  let val = fract(4e4 * sin(dot(vec4<f32>(p, size), vec4<f32>(13.46, 41.74, -73.36, 14.24)) + 17.34));
  if (val < 0.5) { return 0; }
  if (val < 0.95) { return 1; }
  return 2;
}
fn glsl_randomoctree_rot(a: f32) -> mat2x2<f32> {
  return mat2x2<f32>(cos(a), sin(a), -sin(a), cos(a));
}
@body
let gi = floor(rnd() * D(0));
let gj = floor(rnd() * 1000000.0);
let tm = D(2);
let steps = i32(D(3));
let uv = vec2<f32>(2.0 * (gi + 0.5) / D(0) - 1.0, 2.0 * (gj + 0.5) / 1000000.0 - 1.0);
var size = 1.0;
let ro = vec3<f32>(0.5 + sin(tm) * 0.4, 0.5 + cos(tm) * 0.4, tm);
var rd = normalize(vec3<f32>(uv, 1.0));
if (length(vec2<f32>(D(4) * D(0), D(5) * 1000000.0)) > 40.0) {
  let t1 = rd.yz * glsl_randomoctree_rot(D(5) * PI - PI / 2.0);
  rd = vec3<f32>(rd.x, t1.x, t1.y);
  let t2 = rd.xz * glsl_randomoctree_rot(2.0 * D(4) * PI - PI);
  rd = vec3<f32>(t2.x, rd.y, t2.y);
}
var lro = ro - floor(ro);
var fro = ro - lro;
let ird = vec3<f32>(1.0) / max(abs(rd), vec3<f32>(0.001));
let srd = sign(rd);
var mask = vec3<f32>(0.0);
var exitoct = false;
var recursions = 0;
var dist = 0.0;
var edge = 1.0;
var lastmask = vec3<f32>(0.0);
for (var k = 0; k < steps; k++) {
  if (dist > 6.0) { break; }
  let vs = glsl_randomoctree_voxel(fro, size);
  if (exitoct) {
    let newfro = floor(fro / (size / 0.5) + 0.25) * (size * 2.0);
    lro += fro - newfro;
    fro = newfro;
    recursions -= 1;
    size *= 2.0;
    let v0 = glsl_mod3(fro / size + 0.5, vec3<f32>(2.0));
    let v1 = v0 - 1.0 + mask * srd * 0.5;
    exitoct = recursions > 0 && abs(dot(v1, mask)) < 0.1;
  } else if (vs == 1 && recursions <= 5) {
    recursions += 1;
    size *= 0.5;
    let mask2 = step(vec3<f32>(size), lro);
    fro += mask2 * size;
    lro -= mask2 * size;
  } else {
    let hs = size * 0.5;
    let hit = -((srd * (lro - hs) - hs) * ird);
    if (hit.x < min(hit.y, hit.z)) { mask = vec3<f32>(1.0, 0.0, 0.0); }
    else if (hit.y < hit.z) { mask = vec3<f32>(0.0, 1.0, 0.0); }
    else { mask = vec3<f32>(0.0, 0.0, 1.0); }
    let len = dot(hit, mask);
    if (vs == 2) { break; }
    dist += len;
    lro += rd * len - mask * srd * size;
    fro += mask * srd * size;
    exitoct = recursions > 0;
    lastmask = mask;
  }
  if (i32(D(6)) == 1) {
    let q = abs(lro / size - 0.5) * (vec3<f32>(1.0) - lastmask);
    edge = min(edge, -(max(max(q.x, q.y), q.z) - 0.5) * 80.0 * size);
  }
}
var color = vec3<f32>(0.0);
if (dist < 6.0) {
  let val = fract(dot(fro, vec3<f32>(15.23, 754.345, 3.454)));
  let normal = -(lastmask * srd);
  color = (sin(vec3<f32>(39.896, 57.3225, 48.25) * val) * 0.5 + 0.5) * (normal * 0.25 + 0.75);
  if (i32(D(7)) == 1) {
    let q = abs(lro / size - 0.5) * (vec3<f32>(1.0) - lastmask);
    edge = clamp(-(max(max(q.x, q.y), q.z) - 0.5) * 20.0 * size, 0.0, edge);
  }
  if (i32(D(8)) == 1) { color *= edge; } else { color = vec3<f32>(1.0) - (vec3<f32>(1.0) - color) * edge; }
} else if (i32(D(8)) == 1) {
  color = vec3<f32>(edge);
} else {
  color = vec3<f32>(1.0 - edge);
}
color = sqrt(color);
if (i32(D(9)) == 0) {
  let t = glsl_dbl2int(color);
  v.rgbc = 1.0; v.r = t.x; v.g = t.y; v.b = t.z;
} else {
  v.color = sin(color.x / (color.x + color.y + color.z));
}
v.x += AMOUNT * (gi / D(0) - 0.5);
v.y += AMOUNT * (gj / 1000000.0 - 0.5);",
    },
    VarDef {
        name: "glynnia3",
        params: &[ParamDef::new("rscale", 1.0), ParamDef::new("dscale", 1.0), ParamDef::new("rthresh", 0.0), ParamDef::new("ythresh", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let vvar2 = amount * 2f64.sqrt() / 2.0;
            let r = d[0] * (a.x * a.x + a.y * a.y).sqrt();
            // sign of the x offset
            let sx = if r > d[2] && a.y > d[3] { 1.0 } else { -1.0 };
            if ctx.random() > 0.5 {
                let dd = d[1] * (r + a.x).sqrt();
                if dd == 0.0 {
                    return;
                }
                v.x += sx * vvar2 * dd;
                v.y -= vvar2 / dd * a.y;
            } else {
                let dd = d[1] * (r + a.x);
                let dx = (r * (a.y * a.y + dd * dd)).sqrt();
                if dx == 0.0 {
                    return;
                }
                let r = amount / dx;
                v.x += sx * r * dd;
                v.y += r * a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let vvar2 = AMOUNT * sqrt(2.0) / 2.0;
let r = D(0) * sqrt(a.x * a.x + a.y * a.y);
var sx = -1.0;
if (r > D(2) && a.y > D(3)) { sx = 1.0; }
var ok = true;
if (rnd() > 0.5) {
  let dd = D(1) * sqrt(r + a.x);
  if (dd == 0.0) {
    ok = false;
  } else {
    v.x += sx * vvar2 * dd;
    v.y -= vvar2 / dd * a.y;
  }
} else {
  let dd = D(1) * (r + a.x);
  let dx = sqrt(r * (a.y * a.y + dd * dd));
  if (dx == 0.0) {
    ok = false;
  } else {
    let rr = AMOUNT / dx;
    v.x += sx * rr * dd;
    v.y += rr * a.y;
  }
}
if (ok && PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "glynnlissa",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::new("radius1", 0.5),
            ParamDef::new("thickness", 1.0),
            ParamDef::new("phi1", 0.0),
            ParamDef::new("a", 3.0),
            ParamDef::new("b", 2.0),
            ParamDef::new("width", 0.0),
            ParamDef::new("phase", 0.0),
            ParamDef::new("scale", 0.71),
            ParamDef::new("pow", 1.5),
            ParamDef::range("contrast", 0.5, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [radius, radius1, thickness, phi1, a, b, width, phase, scale, pow, contrast, x1, y1, abs_pow]
        apply: |d, ctx, a, v, amount| {
            let (radius, radius1, scale, width) = (d[0], d[1], d[8], d[6]);
            let (x1, y1) = (d[11], d[12]);
            let t = ctx.random() * (M_PI * 2.0);
            let (px, py) = ((d[4] * t + d[7]).sin(), (d[5] * t).sin());
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let y = ctx.random() - 0.5;
            if r < radius.abs() {
                if radius1 >= 0.0 {
                    v.x += amount * ((px * scale + width * y) * radius1 + x1);
                    v.y += amount * ((py * scale + width * y) * radius1 + y1);
                } else {
                    // circle
                    let cr = radius1 * (d[2] + (1.0 - d[2]) * ctx.random());
                    let phi = 2.0 * M_PI * ctx.random();
                    v.x += amount * (cr * phi.cos() + x1);
                    v.y += amount * (cr * phi.sin() + y1);
                }
            } else {
                let alpha = radius.abs() / r;
                let (xi, yi) = if ctx.random() > d[10] * alpha.powf(d[13]) { (a.x, a.y) } else { (alpha * alpha * a.x, alpha * alpha * a.y) };
                let z = (xi - x1) * (xi - x1) + (yi - y1) * (yi - y1);
                if z < radius1 * radius1 {
                    v.x += amount * ((px * scale + width * y) * radius1 + x1);
                    v.y += amount * ((py * scale + width * y) * radius1 + y1);
                } else {
                    v.x += amount * xi;
                    v.y += amount * yi;
                }
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[10] = limit_value(p[10], 0.0, 1.0);
            let ph = M_PI * p[3] / 180.0;
            d.extend([p[0] * ph.cos(), p[0] * ph.sin(), p[9].abs()]);
            d
        }),
        wgsl: "let radius = D(0);
let radius1 = D(1);
let x1 = D(11);
let y1 = D(12);
let t = rnd() * TWO_PI;
let lp = vec2<f32>(sin(D(4) * t + D(7)), sin(D(5) * t));
let r = sqrt(a.x * a.x + a.y * a.y);
let y = rnd() - 0.5;
let liss = (lp * D(8) + D(6) * y) * radius1 + vec2<f32>(x1, y1);
if (r < abs(radius)) {
  if (radius1 >= 0.0) {
    v.x += AMOUNT * liss.x;
    v.y += AMOUNT * liss.y;
  } else {
    let cr = radius1 * (D(2) + (1.0 - D(2)) * rnd());
    let phi = TWO_PI * rnd();
    v.x += AMOUNT * (cr * cos(phi) + x1);
    v.y += AMOUNT * (cr * sin(phi) + y1);
  }
} else {
  let alpha = abs(radius) / r;
  var xi = a.x;
  var yi = a.y;
  if (rnd() <= D(10) * java_pow(alpha, D(13))) {
    xi = alpha * alpha * a.x;
    yi = alpha * alpha * a.y;
  }
  let z = (xi - x1) * (xi - x1) + (yi - y1) * (yi - y1);
  if (z < radius1 * radius1) {
    v.x += AMOUNT * liss.x;
    v.y += AMOUNT * liss.y;
  } else {
    v.x += AMOUNT * xi;
    v.y += AMOUNT * yi;
  }
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "glynnSim1",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::new("radius1", 0.1),
            ParamDef::new("phi1", 110.0),
            ParamDef::range("thickness", 0.1, 0.0, 1.0),
            ParamDef::new("pow", 1.5),
            ParamDef::range("contrast", 0.5, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [radius, radius1, phi1, thickness, pow, contrast, x1, y1, abs_pow]
        apply: |d, ctx, a, v, amount| {
            let (radius, radius1, thickness, x1, y1) = (d[0], d[1], d[3], d[6], d[7]);
            let circle = |ctx: &mut Ctx| {
                let r = radius1 * (thickness + (1.0 - thickness) * ctx.random());
                let phi = 2.0 * M_PI * ctx.random();
                (r * phi.cos() + x1, r * phi.sin() + y1)
            };
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let alpha = radius / r;
            let (tx, ty) = if r < radius {
                circle(ctx)
            } else {
                let (tx, ty) = if ctx.random() > d[5] * alpha.powf(d[8]) { (a.x, a.y) } else { (alpha * alpha * a.x, alpha * alpha * a.y) };
                if (tx - x1) * (tx - x1) + (ty - y1) * (ty - y1) < radius1 * radius1 { circle(ctx) } else { (tx, ty) }
            };
            v.x += amount * tx;
            v.y += amount * ty;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[3] = limit_value(p[3], 0.0, 1.0);
            d[5] = limit_value(p[5], 0.0, 1.0);
            let ph = M_PI * p[2] / 180.0;
            d.extend([p[0] * ph.cos(), p[0] * ph.sin(), p[4].abs()]);
            d
        }),
        wgsl: "let radius = D(0);
let radius1 = D(1);
let c1 = vec2<f32>(D(6), D(7));
let r = sqrt(a.x * a.x + a.y * a.y);
let alpha = radius / r;
var t = vec2<f32>(a.x, a.y);
var circ = r < radius;
if (!circ) {
  if (rnd() <= D(5) * java_pow(alpha, D(8))) { t = alpha * alpha * t; }
  circ = dot(t - c1, t - c1) < radius1 * radius1;
}
if (circ) {
  let cr = radius1 * (D(3) + (1.0 - D(3)) * rnd());
  let phi = TWO_PI * rnd();
  t = cr * vec2<f32>(cos(phi), sin(phi)) + c1;
}
v.x += AMOUNT * t.x;
v.y += AMOUNT * t.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "glynnSim2",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::range("thickness", 0.1, 0.0, 1.0),
            ParamDef::range("contrast", 0.5, 0.0, 1.0),
            ParamDef::new("pow", 1.5),
            ParamDef::new("phi1", 110.0),
            ParamDef::new("phi2", 150.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [radius, thickness, contrast, pow, phi1, phi2, phi10, gamma, delta, abs_pow]
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let alpha = d[0] / r;
            if r < d[0] {
                let cr = d[0] + d[1] - d[7] * ctx.random();
                let phi = d[6] + d[8] * ctx.random();
                v.x += amount * cr * phi.cos();
                v.y += amount * cr * phi.sin();
            } else if ctx.random() > d[2] * alpha.powf(d[9]) {
                v.x += amount * a.x;
                v.y += amount * a.y;
            } else {
                v.x += amount * alpha * alpha * a.x;
                v.y += amount * alpha * alpha * a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| glynn_sim2_prepare(p)),
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let alpha = D(0) / r;
if (r < D(0)) {
  let cr = D(0) + D(1) - D(7) * rnd();
  let phi = D(6) + D(8) * rnd();
  v.x += AMOUNT * cr * cos(phi);
  v.y += AMOUNT * cr * sin(phi);
} else if (rnd() > D(2) * java_pow(alpha, D(9))) {
  v.x += AMOUNT * a.x;
  v.y += AMOUNT * a.y;
} else {
  v.x += AMOUNT * alpha * alpha * a.x;
  v.y += AMOUNT * alpha * alpha * a.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "glynnSim2B",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::range("thickness", 0.1, 0.0, 1.0),
            ParamDef::range("contrast", 0.5, 0.0, 1.0),
            ParamDef::new("pow", 1.5),
            ParamDef::new("phi1", 110.0),
            ParamDef::new("phi2", 150.0),
            ParamDef::new("rotX", 0.0),
            ParamDef::new("rotY", 0.0),
            ParamDef::new("rotZ", 0.0),
            ParamDef::new("offsetZ", 0.0),
            ParamDef::new("scaleX", 1.0),
            ParamDef::new("scaleY", 1.0),
            ParamDef::new("scaleZ", 1.0),
            ParamDef::new("shearXY", 0.0),
            ParamDef::new("shearXZ", 0.0),
            ParamDef::new("shearYX", 0.0),
            ParamDef::new("shearYZ", 0.0),
            ParamDef::new("shearZX", 0.0),
            ParamDef::new("shearZY", 0.0),
            ParamDef::new("circleRotX", 0.0),
            ParamDef::new("circleRotY", 0.0),
            ParamDef::new("circleThicknessZ", 0.0),
            ParamDef::int("colorMode", 0.0, 0.0, 4.0),
            ParamDef::range("fixedHue", 0.0, 0.0, 1.0),
            ParamDef::new("zHueScale", 1.0),
            ParamDef::range("randomHueAmount", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [<params>, phi10, gamma, delta, abs_pow (26..30), rotX, rotY, rotZ, circleRotX, circleRotY (radians, 30..35)]
        apply: |d, ctx, a, v, amount| {
            let (mut x, mut y, mut z) = (a.x, a.y, a.z);
            let r = (x * x + y * y).sqrt();
            let alpha = d[0] / r;
            if r < d[0] {
                let cr = d[0] + d[1] - d[27] * ctx.random();
                let phi = d[26] + d[28] * ctx.random();
                let (cx, cy) = (cr * phi.cos(), cr * phi.sin());
                let cz = (ctx.random() - 0.5) * 2.0 * d[21];
                let (s, c) = d[33].sin_cos();
                let (cy, cz) = (cy * c - cz * s, cy * s + cz * c);
                let (s, c) = d[34].sin_cos();
                x = cx * c + cz * s;
                y = cy;
                z = -cx * s + cz * c;
            } else if ctx.random() <= d[2] * alpha.powf(d[29]) {
                x *= alpha * alpha;
                y *= alpha * alpha;
            }
            x *= d[10];
            y *= d[11];
            z *= d[12];
            let (tx, ty, tz) = (x + d[15] * y + d[17] * z, y + d[13] * x + d[18] * z, z + d[14] * x + d[16] * y);
            let (s, c) = d[30].sin_cos();
            let (y1, z1) = (ty * c - tz * s, ty * s + tz * c);
            let (s, c) = d[31].sin_cos();
            let (x1, z2) = (tx * c + z1 * s, -tx * s + z1 * c);
            let (s, c) = d[32].sin_cos();
            let (x2, y2) = (x1 * c - y1 * s, x1 * s + y1 * c);
            v.x += amount * x2;
            v.y += amount * y2;
            v.z += amount * (z2 + d[9]);
            let mode = d[22] as i32;
            if mode != 0 {
                let mut hue = 0.0;
                if mode == 1 || mode == 3 {
                    hue = fmod((z2 + d[9]) * d[24], 1.0);
                    if hue < 0.0 {
                        hue += 1.0;
                    }
                }
                if mode == 2 || mode == 3 {
                    let shift = (ctx.random() - 0.5) * 2.0 * d[25];
                    hue = fmod(hue + shift, 1.0);
                    if hue < 0.0 {
                        hue += 1.0;
                    }
                }
                if mode == 4 {
                    hue = d[23];
                }
                v.color = hue;
            }
        },
        prepare: Some(|p, _| {
            let mut d = glynn_sim2_prepare(&p[..6]);
            // glynn_sim2_prepare appends [phi10, gamma, delta, abs_pow] after the first 6 values
            let derived = d.split_off(6);
            d.extend_from_slice(&p[6..]);
            d[22] = p[22].trunc();
            d[23] = limit_value(p[23], 0.0, 1.0);
            d[25] = limit_value(p[25], 0.0, 1.0);
            d.extend(&derived[..]);
            d.extend([p[6], p[7], p[8], p[19], p[20]].map(|deg| M_PI * deg / 180.0));
            d
        }),
        wgsl: "var q = vec3<f32>(a.x, a.y, a.z);
let r = sqrt(q.x * q.x + q.y * q.y);
let alpha = D(0) / r;
if (r < D(0)) {
  let cr = D(0) + D(1) - D(27) * rnd();
  let phi = D(26) + D(28) * rnd();
  let cx = cr * cos(phi);
  var cy = cr * sin(phi);
  var cz = (rnd() - 0.5) * 2.0 * D(21);
  let t = cy * cos(D(33)) - cz * sin(D(33));
  cz = cy * sin(D(33)) + cz * cos(D(33));
  cy = t;
  q = vec3<f32>(cx * cos(D(34)) + cz * sin(D(34)), cy, -cx * sin(D(34)) + cz * cos(D(34)));
} else if (rnd() <= D(2) * java_pow(alpha, D(29))) {
  q = vec3<f32>(q.xy * alpha * alpha, q.z);
}
q *= vec3<f32>(D(10), D(11), D(12));
let s = vec3<f32>(q.x + D(15) * q.y + D(17) * q.z, q.y + D(13) * q.x + D(18) * q.z, q.z + D(14) * q.x + D(16) * q.y);
let y1 = s.y * cos(D(30)) - s.z * sin(D(30));
let z1 = s.y * sin(D(30)) + s.z * cos(D(30));
let x1 = s.x * cos(D(31)) + z1 * sin(D(31));
let z2 = -s.x * sin(D(31)) + z1 * cos(D(31)) + D(9);
v.x += AMOUNT * (x1 * cos(D(32)) - y1 * sin(D(32)));
v.y += AMOUNT * (x1 * sin(D(32)) + y1 * cos(D(32)));
v.z += AMOUNT * z2;
let mode = i32(D(22));
if (mode != 0) {
  var hue = 0.0;
  if (mode == 1 || mode == 3) {
    hue = (z2 * D(24)) % 1.0;
    if (hue < 0.0) { hue += 1.0; }
  }
  if (mode == 2 || mode == 3) {
    hue = (hue + (rnd() - 0.5) * 2.0 * D(25)) % 1.0;
    if (hue < 0.0) { hue += 1.0; }
  }
  if (mode == 4) { hue = D(23); }
  v.color = hue;
}",
    },
    VarDef {
        name: "glynnSim3",
        params: &[ParamDef::new("radius", 1.0), ParamDef::new("thickness", 0.1), ParamDef::range("contrast", 0.5, 0.0, 1.0), ParamDef::new("pow", 1.5)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [radius, thickness, contrast, pow, radius1, radius2, gamma, abs_pow]
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let alpha = d[0] / r;
            if r < d[4] {
                let phi = 2.0 * M_PI * ctx.random();
                let cr = if ctx.random() < d[6] { d[4] } else { d[5] };
                v.x += amount * cr * phi.cos();
                v.y += amount * cr * phi.sin();
            } else if ctx.random() > d[2] * alpha.powf(d[7]) {
                v.x += amount * a.x;
                v.y += amount * a.y;
            } else {
                v.x += amount * alpha * alpha * a.x;
                v.y += amount * alpha * alpha * a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[2] = limit_value(p[2], 0.0, 1.0);
            let radius1 = p[0] + p[1];
            let radius2 = p[0] * p[0] / radius1;
            d.extend([radius1, radius2, radius1 / (radius1 + radius2), p[3].abs()]);
            d
        }),
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let alpha = D(0) / r;
if (r < D(4)) {
  let phi = TWO_PI * rnd();
  var cr = D(5);
  if (rnd() < D(6)) { cr = D(4); }
  v.x += AMOUNT * cr * cos(phi);
  v.y += AMOUNT * cr * sin(phi);
} else if (rnd() > D(2) * java_pow(alpha, D(7))) {
  v.x += AMOUNT * a.x;
  v.y += AMOUNT * a.y;
} else {
  v.x += AMOUNT * alpha * alpha * a.x;
  v.y += AMOUNT * alpha * alpha * a.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "glynnspiro",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::new("radius1", 0.5),
            ParamDef::new("thickness", 1.0),
            ParamDef::new("phi1", 0.0),
            ParamDef::new("a", 1.0),
            ParamDef::new("b", -0.3),
            ParamDef::new("c", 0.4),
            ParamDef::new("time", 1000.0),
            ParamDef::int("mode", 0.0, 0.0, 4.0),
            ParamDef::new("width", 0.0),
            ParamDef::new("scale", 0.71),
            ParamDef::new("pow", 1.5),
            ParamDef::range("contrast", 0.5, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [radius, radius1, thickness, phi1, a, b, c, time, mode, width, scale, pow, contrast, x1, y1, abs_pow]
        apply: |d, ctx, a, v, amount| {
            // The width offsets w1..w3 are unused in JWildfire, only their random numbers are drawn.
            let draws = [1, 1, 3, 12, 1][d[8] as usize] + 3;
            for _ in 0..3 {
                ctx.random();
            }
            let t = ctx.random() * d[7];
            for _ in 3..draws {
                ctx.random();
            }
            let (sa, sb, sc) = (d[4], d[5], d[6]);
            let px = (sa + sb) * t.cos() - sc * ((sa + sb) / sb * t).cos();
            let py = (sa + sb) * t.sin() - sc * ((sa + sb) / sb * t).sin();
            let y = ctx.random() - 0.5;
            let c = (px * d[10] + d[9] * y, py * d[10] + d[9] * y);
            glynn_curve([d[0], d[1], d[2], d[12], d[13], d[14], d[15]], ctx, a, v, amount, c);
        },
        prepare: Some(|p, _| {
            let mut d = glynn_prepare(p, p[0], p[3], p[11], 12);
            d[8] = limit_value(p[8], 0.0, 4.0).trunc();
            d
        }),
        wgsl: "let radius = D(0);
let radius1 = D(1);
rnd(); rnd(); rnd();
let t = rnd() * D(7);
let mode = i32(D(8));
if (mode == 2) { rnd(); rnd(); rnd(); }
else if (mode == 3) { for (var i = 0; i < 12; i++) { rnd(); } }
else { rnd(); }
let sab = D(4) + D(5);
let k = sab / D(5) * t;
let pc = vec2<f32>(sab * cos(t) - D(6) * cos(k), sab * sin(t) - D(6) * sin(k));
let y = rnd() - 0.5;
let cv = pc * D(10) + D(9) * y;
let r = sqrt(a.x * a.x + a.y * a.y);
let x1 = D(13);
let y1 = D(14);
if (r < abs(radius)) {
  if (radius1 >= 0.0) {
    v.x += AMOUNT * (cv.x * radius1 + x1);
    v.y += AMOUNT * (cv.y * radius1 + y1);
  } else {
    let cr = radius1 * (D(2) + (1.0 - D(2)) * rnd());
    let phi = TWO_PI * rnd();
    v.x += AMOUNT * (cr * cos(phi) + x1);
    v.y += AMOUNT * (cr * sin(phi) + y1);
  }
} else {
  let alpha = abs(radius) / r;
  var xi = a.x;
  var yi = a.y;
  if (rnd() <= D(12) * java_pow(alpha, D(15))) {
    xi = alpha * alpha * a.x;
    yi = alpha * alpha * a.y;
  }
  if ((xi - x1) * (xi - x1) + (yi - y1) * (yi - y1) < radius1 * radius1) {
    v.x += AMOUNT * (cv.x * radius1 + x1);
    v.y += AMOUNT * (cv.y * radius1 + y1);
  } else {
    v.x += AMOUNT * xi;
    v.y += AMOUNT * yi;
  }
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "glynnSShape",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::new("radius1", 0.5),
            ParamDef::new("thickness", 1.0),
            ParamDef::new("phi1", 0.0),
            ParamDef::new("m", 5.0),
            ParamDef::new("n1", 1.7),
            ParamDef::new("n2", 1.7),
            ParamDef::new("n3", 1.7),
            ParamDef::new("scale", 0.71),
            ParamDef::new("pow", 1.5),
            ParamDef::range("contrast", 0.5, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [radius, radius1, thickness, phi1, m, n1, n2, n3, scale, pow, contrast, x1, y1, abs_pow]
        apply: |d, ctx, a, v, amount| {
            let phi = ctx.random() * (M_PI * 2.0);
            let t1 = (d[4] * phi / 4.0).cos().abs().powf(d[6]);
            let t2 = (d[4] * phi / 4.0).sin().abs().powf(d[7]);
            let r = (t1 + t2).powf(1.0 / d[5]);
            let (px, py) = if r.abs() == 0.0 { (0.0, 0.0) } else { (phi.cos() / r, phi.sin() / r) };
            glynn_curve([d[0], d[1], d[2], d[10], d[11], d[12], d[13]], ctx, a, v, amount, (px * d[8], py * d[8]));
        },
        prepare: Some(|p, _| glynn_prepare(p, p[0], p[3], p[9], 10)),
        wgsl: "let radius = D(0);
let radius1 = D(1);
let phi = rnd() * TWO_PI;
let t1 = java_pow(abs(cos(D(4) * phi / 4.0)), D(6));
let t2 = java_pow(abs(sin(D(4) * phi / 4.0)), D(7));
let rs = java_pow(t1 + t2, 1.0 / D(5));
var pc = vec2<f32>(0.0);
if (abs(rs) != 0.0) { pc = vec2<f32>(cos(phi), sin(phi)) / rs; }
let cv = pc * D(8);
let r = sqrt(a.x * a.x + a.y * a.y);
let x1 = D(11);
let y1 = D(12);
if (r < abs(radius)) {
  if (radius1 >= 0.0) {
    v.x += AMOUNT * (cv.x * radius1 + x1);
    v.y += AMOUNT * (cv.y * radius1 + y1);
  } else {
    let cr = radius1 * (D(2) + (1.0 - D(2)) * rnd());
    let phi = TWO_PI * rnd();
    v.x += AMOUNT * (cr * cos(phi) + x1);
    v.y += AMOUNT * (cr * sin(phi) + y1);
  }
} else {
  let alpha = abs(radius) / r;
  var xi = a.x;
  var yi = a.y;
  if (rnd() <= D(10) * java_pow(alpha, D(13))) {
    xi = alpha * alpha * a.x;
    yi = alpha * alpha * a.y;
  }
  if ((xi - x1) * (xi - x1) + (yi - y1) * (yi - y1) < radius1 * radius1) {
    v.x += AMOUNT * (cv.x * radius1 + x1);
    v.y += AMOUNT * (cv.y * radius1 + y1);
  } else {
    v.x += AMOUNT * xi;
    v.y += AMOUNT * yi;
  }
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "gnomonic_proj",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let nr = if r > 0.001 { (r * d[0]).tan() / d[0] } else { r };
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
var nr = r;
if (r > 0.001) { nr = tan(r * D(0)) / D(0); }
v.x += AMOUNT * nr * cos(theta);
v.y += AMOUNT * nr * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "gosperisland_js",
        params: &[ParamDef::int("level", 2.0, 1.0, 6.0), ParamDef::new("line_thickness", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // Java cycles through the segments in order; here per render thread (the GPU draws a
            // random segment).
            let nseg = d[3] as usize;
            let k = ctx.cycle_index(d.as_ptr() as usize, nseg);
            let (x1, y1, x2, y2) = (d[4 + 2 * k], d[5 + 2 * k], d[6 + 2 * k], d[7 + 2 * k]);
            let (xdiff, ydiff) = (x2 - x1, y2 - y1);
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
            if d[2] != 0.0 {
                xoffset += (ctx.random() - 0.5) * d[2];
                yoffset += (ctx.random() - 0.5) * d[2];
            }
            v.x += amount * (x1 + xoffset);
            v.y += amount * (y1 + yoffset);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| gosperisland_prepare(p)),
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
let dd = rnd() * sqrt(xdiff * xdiff + ydiff * ydiff);
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
        name: "gothic_arch",
        params: &[ParamDef::new("height", 1.0), ParamDef::new("sharpness", 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let h = java_max(d[0].abs(), 0.1);
            let sharp = java_max(d[1].abs(), 0.1);
            let t = java_max(-2.0, java_min(2.0, a.y / h));
            let profile = java_max(1.0 - t.abs().powf(sharp), 0.01);
            v.x += amount * a.x * profile;
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let h = max(abs(D(0)), 0.1);
let sharp = max(abs(D(1)), 0.1);
let t = clamp(a.y / h, -2.0, 2.0);
let profile = max(1.0 - java_pow(abs(t), sharp), 0.01);
v.x += AMOUNT * a.x * profile;
v.y += AMOUNT * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "gradient_warp",
        params: &[ParamDef::new("freq", 2.0), ParamDef::new("amp", 0.5), ParamDef::new("seed", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let px = d[0] * a.x + d[2];
            let py = d[0] * a.y + d[2] * 1.3;
            v.x += amount * (d[1] * px.cos() * py.sin());
            v.y += amount * (d[1] * px.sin() * py.cos());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let px = D(0) * a.x + D(2);
let py = D(0) * a.y + D(2) * 1.3;
v.x += AMOUNT * (D(1) * cos(px) * sin(py));
v.y += AMOUNT * (D(1) * sin(px) * cos(py));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "greebles",
        params: &[
            ParamDef::int("mode", 0.0, 0.0, 1.0),
            ParamDef::int("base_shape", 1.0, 0.0, 2.0),
            ParamDef::new("size", 2.0),
            ParamDef::int("subdivisions", 10.0, 0.0, 100.0),
            ParamDef::int("greeble_shape", 0.0, 0.0, 3.0),
            ParamDef::new("greeble_height", 0.2),
            ParamDef::int("seed", 12345.0, 0.0, 100000.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| greebles_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for i in [0, 1, 3, 4, 6] {
                d[i] = p[i].trunc();
            }
            d
        }),
        wgsl: "fn greebles_norm(p: vec3<f32>) -> vec3<f32> {
  let len = length(p);
  if (len > 1e-9) { return p / len; }
  return p;
}
// MarsagliaRandomGenerator.randomize(seed) + random(); only the low 16 bits of the seed matter.
fn greebles_marsaglia(seed: i32) -> f32 {
  var u = seed << 16u;
  var v = (seed << 16u) >> 16u;
  v = 36969 * (v & 65535) + (v >> 16u);
  u = 18000 * (u & 65535) + (u >> 16u);
  return abs(f32((v << 16u) + u) / 2147483647.0);
}
@body
let mode = i32(D(0));
let bshape = i32(D(1));
let size = D(2);
let subdiv = i32(D(3));
let shape = i32(D(4));
var gb = vec3<f32>(0.0);
var normal = vec3<f32>(0.0);
var gsize = 0.0;
var hide = false;
if (bshape == 0) {
  var ua = atan2(a.y, a.x);
  var va = atan2(a.z, sqrt(a.x * a.x + a.y * a.y));
  if (mode != 1) {
    ua = rnd() * TWO_PI;
    va = acos(2.0 * rnd() - 1.0);
  }
  let ucell = TWO_PI / f32(max(1, subdiv * 2));
  let vcell = PI / f32(max(1, subdiv));
  let uq = floor(ua / ucell) * ucell + ucell * 0.5;
  let vq = floor(va / vcell) * vcell + vcell * 0.5;
  gb = size * 0.5 * vec3<f32>(sin(vq) * cos(uq), sin(vq) * sin(uq), cos(vq));
  normal = greebles_norm(gb);
  gsize = PI * size / f32(subdiv);
} else {
  var p = vec3<f32>(a.x, a.y, a.z);
  if (mode != 1) {
    if (bshape == 2) {
      p = vec3<f32>((rnd() - 0.5) * size, (rnd() - 0.5) * size, 0.0);
    } else {
      let face = i32(rnd() * 6.0);
      let ur = (rnd() - 0.5) * size;
      let vr = (rnd() - 0.5) * size;
      let hs = size * 0.5;
      switch (face) {
        case 0: { p = vec3<f32>(hs, ur, vr); }
        case 1: { p = vec3<f32>(-hs, ur, vr); }
        case 2: { p = vec3<f32>(ur, hs, vr); }
        case 3: { p = vec3<f32>(ur, -hs, vr); }
        case 4: { p = vec3<f32>(ur, vr, hs); }
        default: { p = vec3<f32>(ur, vr, -hs); }
      }
    }
  }
  let half = size * 0.5;
  if (bshape == 2) {
    p.z = 0.0;
    normal = vec3<f32>(0.0, 0.0, 1.0);
  } else {
    let ap = abs(p);
    if (ap.x >= ap.y && ap.x >= ap.z) {
      p.x = select(-half, half, p.x > 0.0);
      normal = vec3<f32>(select(-1.0, 1.0, p.x > 0.0), 0.0, 0.0);
    } else if (ap.y >= ap.x && ap.y >= ap.z) {
      p.y = select(-half, half, p.y > 0.0);
      normal = vec3<f32>(0.0, select(-1.0, 1.0, p.y > 0.0), 0.0);
    } else {
      p.z = select(-half, half, p.z > 0.0);
      normal = vec3<f32>(0.0, 0.0, select(-1.0, 1.0, p.z > 0.0));
    }
  }
  gsize = size;
  if (subdiv > 0) { gsize = size / f32(subdiv); }
  hide = gsize < 1.0e-6;
  gb = floor(p / gsize) * gsize + gsize * 0.5;
}
if (hide) {
  v.hide = 1.0;
} else {
  let ib = vec3<i32>(gb * 100.0);
  let cseed = (i32(D(6)) + ib.x * 73856093) ^ (ib.y * 19349663) ^ (ib.z * 83492791);
  let height = D(5) * greebles_marsaglia(cseed);
  var up = vec3<f32>(0.0, 1.0, 0.0);
  if (abs(normal.y) > 0.999) { up = vec3<f32>(1.0, 0.0, 0.0); }
  let tangent = greebles_norm(cross(normal, up));
  let bitangent = greebles_norm(cross(normal, tangent));
  var u = 0.0;
  var w2 = 0.0;
  var w = 0.0;
  if (mode == 0) {
    if (shape == 1) {
      let ang = rnd() * TWO_PI;
      let rad = gsize * 0.5 * sqrt(rnd());
      u = cos(ang) * rad;
      w2 = sin(ang) * rad;
      w = rnd() * height;
    } else if (shape == 2) {
      let rv = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
      let rl = length(rv);
      if (rl > 1e-9) {
        let rad = java_pow(rnd(), 1.0 / 3.0);
        u = rv.x / rl * rad * gsize * 0.5;
        w2 = rv.y / rl * rad * gsize * 0.5;
        w = rv.z / rl * rad * height;
      }
    } else if (shape == 3) {
      let ang = rnd() * TWO_PI;
      w = rnd() * height;
      var taper = 1.0;
      if (height > 1e-9) { taper = 1.0 - w / height; }
      let rad = gsize * 0.5 * sqrt(rnd()) * taper;
      u = cos(ang) * rad;
      w2 = sin(ang) * rad;
    } else {
      u = (rnd() - 0.5) * gsize;
      w2 = (rnd() - 0.5) * gsize;
      w = rnd() * height;
    }
  } else {
    let r = vec3<f32>(a.x, a.y, a.z) - gb;
    let lu = dot(r, tangent);
    let lv = dot(r, bitangent);
    let lw = dot(r, normal);
    if (shape == 1 || shape == 3) {
      let ang = atan2(lv, lu);
      w = fract(lw) * height;
      var taper = 1.0;
      if (shape == 3 && height > 1e-9) { taper = 1.0 - w / height; }
      let rad = gsize * 0.5 * fract(sqrt(lu * lu + lv * lv)) * taper;
      u = cos(ang) * rad;
      w2 = sin(ang) * rad;
    } else if (shape == 2) {
      let rl = sqrt(lu * lu + lv * lv + lw * lw);
      let rad = gsize * 0.5 * fract(rl);
      if (rl > 1e-9) {
        u = lu / rl * rad;
        w2 = lv / rl * rad;
        w = lw / rl * rad;
      }
    } else {
      u = (fract(lu) - 0.5) * gsize;
      w2 = (fract(lv) - 0.5) * gsize;
      w = fract(lw) * height;
    }
  }
  let f = gb + tangent * u + bitangent * w2 + normal * w;
  v.x += f.x * AMOUNT;
  v.y += f.y * AMOUNT;
  v.z += f.z * AMOUNT;
}",
    },
    VarDef {
        name: "grid3d_wf",
        params: &[
            ParamDef::new("size", 0.1),
            ParamDef::new("size_spread", 0.0),
            ParamDef::new("spacing", 0.75),
            ParamDef::new("alpha", 0.0),
            ParamDef::new("alpha_spread", 0.0),
            ParamDef::new("beta", 0.0),
            ParamDef::new("beta_spread", 0.0),
            ParamDef::new("gamma", 0.0),
            ParamDef::new("gamma_spread", 0.0),
            ParamDef::range("c1", 0.1, 0.0, 1.0),
            ParamDef::range("c2", 0.2, 0.0, 1.0),
            ParamDef::range("c3", 0.3, 0.0, 1.0),
            ParamDef::range("c4", 0.4, 0.0, 1.0),
            ParamDef::range("c5", 0.5, 0.0, 1.0),
            ParamDef::range("c6", 0.6, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [<params>, do_rotate]
        apply: |d, ctx, a, v, _| {
            let size = d[0];
            let (cx, cy, cz) = ((a.x / size) as i32, (a.y / size) as i32, (a.z / size) as i32);
            // spread values per cell: spread * (1 - rnd)
            let spread = |i: usize, k: u32| if d[i] > EPSILON { d[i] * (1.0 - grid3d_cell_rnd(cx, cy, cz, k)) } else { 0.0 };
            let sizescl = d[2] * (size + spread(1, 0) * 0.1);
            let (mut dx, mut dy, mut dz);
            match ctx.random_int(3) {
                0 => {
                    let left = ctx.random() < 0.5;
                    dx = sizescl * if left { -0.5 } else { 0.5 };
                    dy = sizescl * (ctx.random() - 0.5);
                    dz = sizescl * (ctx.random() - 0.5);
                    v.color = if left { d[9] } else { d[10] };
                }
                1 => {
                    let top = ctx.random() < 0.5;
                    dx = sizescl * (ctx.random() - 0.5);
                    dy = sizescl * if top { -0.5 } else { 0.5 };
                    dz = sizescl * (ctx.random() - 0.5);
                    v.color = if top { d[11] } else { d[12] };
                }
                _ => {
                    let front = ctx.random() < 0.5;
                    dx = sizescl * (ctx.random() - 0.5);
                    dy = sizescl * (ctx.random() - 0.5);
                    dz = sizescl * if front { -0.5 } else { 0.5 };
                    v.color = if front { d[13] } else { d[14] };
                }
            }
            if d[15] != 0.0 {
                let (sina, cosa) = (d[3] + spread(4, 1)).sin_cos();
                let (sinb, cosb) = (d[5] + spread(6, 2)).sin_cos();
                let (sing, cosg) = (d[7] + spread(8, 3)).sin_cos();
                let dxr = dx * (cosb * cosg) + dy * (cosg * sina * sinb - cosa * sing) + dz * (cosa * cosg * sinb + sina * sing);
                let dyr = dx * (cosb * sing) + dy * (cosa * cosg + sina * sinb * sing) + dz * (-cosg * sina + cosa * sinb * sing);
                let dzr = dx * (-sinb) + dy * (cosb * sina) + dz * (cosa * cosb);
                dx = dxr;
                dy = dyr;
                dz = dzr;
            }
            v.x += cx as f64 * size + dx;
            v.y += cy as f64 * size + dy;
            v.z += cz as f64 * size + dz;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for i in 9..15 {
                d[i] = limit_value(p[i], 0.0, 1.0);
            }
            let rot = [3, 5, 7, 4, 6, 8].iter().any(|&i| p[i].abs() > EPSILON);
            d.push(if rot { 1.0 } else { 0.0 });
            d
        }),
        wgsl: "fn grid3d_wf_rnd(c: vec3<i32>, k: u32) -> f32 {
  var h = (bitcast<u32>(c.x) * 0x8da6b343u) ^ (bitcast<u32>(c.y) * 0xd8163841u) ^ (bitcast<u32>(c.z) * 0xcb1ab31fu) ^ (k * 0x165667b1u);
  h ^= h >> 16u;
  h *= 0x7feb352du;
  h ^= h >> 15u;
  h *= 0x846ca68bu;
  h ^= h >> 16u;
  return f32(h) / 4294967296.0;
}
fn grid3d_wf_spread(s: f32, c: vec3<i32>, k: u32) -> f32 {
  if (s > EPSILON) { return s * (1.0 - grid3d_wf_rnd(c, k)); }
  return 0.0;
}
@body
let size = D(0);
let c = vec3<i32>(vec3<f32>(a.x, a.y, a.z) / size);
let sizescl = D(2) * (size + grid3d_wf_spread(D(1), c, 0u) * 0.1);
var dd = vec3<f32>(0.0);
let face = min(u32(rnd() * 3.0), 2u);
let side = rnd() < 0.5;
let h = select(0.5, -0.5, side);
if (face == 0u) {
  dd = sizescl * vec3<f32>(h, rnd() - 0.5, rnd() - 0.5);
  v.color = select(D(10), D(9), side);
} else if (face == 1u) {
  dd.x = sizescl * (rnd() - 0.5);
  dd.y = sizescl * h;
  dd.z = sizescl * (rnd() - 0.5);
  v.color = select(D(12), D(11), side);
} else {
  dd.x = sizescl * (rnd() - 0.5);
  dd.y = sizescl * (rnd() - 0.5);
  dd.z = sizescl * h;
  v.color = select(D(14), D(13), side);
}
if (D(15) != 0.0) {
  let al = D(3) + grid3d_wf_spread(D(4), c, 1u);
  let be = D(5) + grid3d_wf_spread(D(6), c, 2u);
  let ga = D(7) + grid3d_wf_spread(D(8), c, 3u);
  let sina = sin(al); let cosa = cos(al);
  let sinb = sin(be); let cosb = cos(be);
  let sing = sin(ga); let cosg = cos(ga);
  dd = vec3<f32>(
    dd.x * (cosb * cosg) + dd.y * (cosg * sina * sinb - cosa * sing) + dd.z * (cosa * cosg * sinb + sina * sing),
    dd.x * (cosb * sing) + dd.y * (cosa * cosg + sina * sinb * sing) + dd.z * (-cosg * sina + cosa * sinb * sing),
    dd.x * (-sinb) + dd.y * (cosb * sina) + dd.z * (cosa * cosb));
}
v.x += f32(c.x) * size + dd.x;
v.y += f32(c.y) * size + dd.y;
v.z += f32(c.z) * size + dd.z;",
    },
    VarDef {
        name: "gridout",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |_, ctx, a, v, amount| {
            // cases a..h: (dx, dy)
            const OFF: [(f64, f64); 8] = [(1.0, 0.0), (0.0, 1.0), (1.0, 0.0), (0.0, -1.0), (-1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)];
            let (dx, dy) = OFF[gridout_case(a.x.round_ties_even(), a.y.round_ties_even())];
            v.x += amount * (a.x + dx);
            v.y += amount * (a.y + dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "fn gridout_case(x: f32, y: f32) -> i32 {
  if (y <= 0.0) {
    if (x > 0.0) { return select(1, 0, -y >= x); }
    return select(3, 2, y <= x);
  }
  if (x > 0.0) { return select(5, 4, y >= x); }
  return select(7, 6, y > -x);
}
@body
let k = gridout_case(round(a.x), round(a.y));
var off = vec2<f32>(0.0, -1.0);
switch (k) {
  case 0, 2: { off = vec2<f32>(1.0, 0.0); }
  case 1, 5: { off = vec2<f32>(0.0, 1.0); }
  case 4, 6: { off = vec2<f32>(-1.0, 0.0); }
  default: {}
}
v.x += AMOUNT * (a.x + off.x);
v.y += AMOUNT * (a.y + off.y);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "gridout2",
        params: &[ParamDef::new("a", 1.0), ParamDef::new("b", 1.0), ParamDef::new("c", 1.0), ParamDef::new("d", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (pa, pb) = (d[0], d[1]);
            let off = [(pa, 0.0), (0.0, pb), (pa, 0.0), (0.0, -pb), (-pa, 0.0), (0.0, pb), (-pa, 0.0), (0.0, -pb)];
            let (dx, dy) = off[gridout_case(a.x.round_ties_even() * d[2], a.y.round_ties_even() * d[3])];
            v.x += amount * (a.x + dx);
            v.y += amount * (a.y + dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "fn gridout2_case(x: f32, y: f32) -> i32 {
  if (y <= 0.0) {
    if (x > 0.0) { return select(1, 0, -y >= x); }
    return select(3, 2, y <= x);
  }
  if (x > 0.0) { return select(5, 4, y >= x); }
  return select(7, 6, y > -x);
}
@body
let k = gridout2_case(round(a.x) * D(2), round(a.y) * D(3));
var off = vec2<f32>(0.0, -D(1));
switch (k) {
  case 0, 2: { off = vec2<f32>(D(0), 0.0); }
  case 1, 5: { off = vec2<f32>(0.0, D(1)); }
  case 4, 6: { off = vec2<f32>(-D(0), 0.0); }
  default: {}
}
v.x += AMOUNT * (a.x + off.x);
v.y += AMOUNT * (a.y + off.y);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "gridout3D",
        params: &[
            ParamDef::new("xx", 1.0),
            ParamDef::new("yy", 1.0),
            ParamDef::new("xa", 1.0),
            ParamDef::new("xb", 0.0),
            ParamDef::new("xc", 1.0),
            ParamDef::new("xd", 0.0),
            ParamDef::new("xe", 1.0),
            ParamDef::new("xf", 0.0),
            ParamDef::new("xg", 1.0),
            ParamDef::new("xh", 0.0),
            ParamDef::new("ya", 0.0),
            ParamDef::new("yb", 1.0),
            ParamDef::new("yc", 0.0),
            ParamDef::new("yd", 1.0),
            ParamDef::new("ye", 0.0),
            ParamDef::new("yf", 1.0),
            ParamDef::new("yg", 0.0),
            ParamDef::new("yh", 1.0),
            ParamDef::new("za", 0.0),
            ParamDef::new("zb", 0.0),
            ParamDef::new("zc", 0.0),
            ParamDef::new("zd", 0.0),
            ParamDef::new("ze", 1.0),
            ParamDef::new("zf", 1.0),
            ParamDef::new("zg", 1.0),
            ParamDef::new("zh", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let k = gridout_case(a.x.round_ties_even() * d[0], a.y.round_ties_even() * d[1]);
            // x: +xa +xb +xc +xd -xe +xf -xg +xh; y: +ya +yb +yc -yd +ye +yf +yg -yh;
            // z: + for cases a..d, * for e..h
            let sx = [1.0, 1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0][k];
            let sy = [1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0][k];
            v.x += amount * (a.x + sx * d[2 + k]);
            v.y += amount * (a.y + sy * d[10 + k]);
            v.z += amount * if k < 4 { a.z + d[18 + k] } else { a.z * d[18 + k] };
        },
        prepare: None,
        wgsl: "fn gridout3D_case(x: f32, y: f32) -> i32 {
  if (y <= 0.0) {
    if (x > 0.0) { return select(1, 0, -y >= x); }
    return select(3, 2, y <= x);
  }
  if (x > 0.0) { return select(5, 4, y >= x); }
  return select(7, 6, y > -x);
}
@body
let k = u32(gridout3D_case(round(a.x) * D(0), round(a.y) * D(1)));
var sx = 1.0;
if (k == 4u || k == 6u) { sx = -1.0; }
var sy = 1.0;
if (k == 3u || k == 7u) { sy = -1.0; }
v.x += AMOUNT * (a.x + sx * prm[base + 2u + k]);
v.y += AMOUNT * (a.y + sy * prm[base + 10u + k]);
if (k < 4u) { v.z += AMOUNT * (a.z + prm[base + 18u + k]); } else { v.z += AMOUNT * (a.z * prm[base + 18u + k]); }",
    },
    VarDef {
        name: "gumowski_mira",
        params: &[ParamDef::int("random", 1.0, 0.0, 10000000.0), ParamDef::range("step", 0.001, 0.0, 1.0), ParamDef::new("a", 0.000001), ParamDef::new("b", 0.05)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [random, step, a, b, m, a_eff]
        apply: |d, ctx, a, v, amount| {
            // Java follows one endless orbit from (5, 5) per instance; here one per render thread.
            let (m, aa, b) = (d[4], d[5], d[3]);
            let mira = |x: f64| m * x + (2.0 * (1.0 - m) * x * x) / (1.0 + x * x);
            let p = ctx.orbits.entry(d.as_ptr() as usize).or_insert([5.0, 5.0]);
            let (x, y) = (p[0], p[1]);
            let x0 = y + (aa * (1.0 - (b * y * y)) * y) + mira(x);
            let y0 = -x + mira(x0);
            *p = [x0, y0];
            v.x = x0 * amount;
            v.y = y0 * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            // Effect of setting the parameters in order on a new JWildfire instance (as when a
            // flame is loaded): "random" and "step" shift m, "a" draws a from the seed.
            let mut d = p.to_vec();
            let seed = limit_value(p[0], 0.0, 10000000.0) as i32;
            let step = limit_value(p[1], 0.0, 1.0);
            let m = -0.080 + if seed > 1 { 0.001 * 2.0 } else { 0.001 / 2.0 } + step;
            let aa = 1.0e-6 + (1e-6 - 1.0e-8) * crate::rng::JavaRandom::new(seed as i64).next_double();
            d[0] = seed as f64;
            d[1] = step;
            d.extend([m, aa]);
            d
        }),
        // GPU: the orbit is kept per shader invocation; it starts at a slightly perturbed start
        // point after a random number of warm-up steps.
        wgsl: "var<private> gumowski_mira_st: vec3<f32>;
fn gumowski_mira_f(x: f32, m: f32) -> f32 { return m * x + (2.0 * (1.0 - m) * x * x) / (1.0 + x * x); }
fn gumowski_mira_step(p: vec2<f32>, base: u32) -> vec2<f32> {
  let m = prm[base + 4u];
  let xn = p.y + (prm[base + 5u] * (1.0 - (prm[base + 3u] * p.y * p.y)) * p.y) + gumowski_mira_f(p.x, m);
  return vec2<f32>(xn, -p.x + gumowski_mira_f(xn, m));
}
@body
if (gumowski_mira_st.z == 0.0) {
  var q = vec2<f32>(5.0) + 1.0e-2 * (vec2<f32>(rnd(), rnd()) - 0.5);
  let n = min(i32(rnd() * 65536.0), 65535);
  for (var i = 0; i < n; i++) { q = gumowski_mira_step(q, base); }
  gumowski_mira_st = vec3<f32>(q, 1.0);
}
let q = gumowski_mira_step(gumowski_mira_st.xy, base);
gumowski_mira_st = vec3<f32>(q, 1.0);
v.x = q.x * AMOUNT;
v.y = q.y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hadamard_js",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |_, ctx, a, v, amount| {
            let (x, y) = if ctx.random() < 0.333 {
                (a.x / 2.0, a.y / 2.0)
            } else if ctx.random() < 0.666 {
                (a.y / 2.0, -a.x / 2.0 - 0.5)
            } else {
                (-a.y / 2.0 - 0.5, a.x / 2.0)
            };
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "var q = vec2<f32>(a.y / 2.0, -a.x / 2.0 - 0.5);
if (rnd() < 0.333) {
  q = vec2<f32>(a.x / 2.0, a.y / 2.0);
} else if (rnd() >= 0.666) {
  q = vec2<f32>(-a.y / 2.0 - 0.5, a.x / 2.0);
}
v.x += q.x * AMOUNT;
v.y += q.y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "halo",
        params: &[ParamDef::new("radius", 0.5), ParamDef::new("width", 0.2), ParamDef::new("strength", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let rad = java_max(d[0].abs(), 0.01);
            let w = java_max(d[1].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let ring = r - rad;
            let env = (-(ring * ring) / (w * w)).exp();
            let scale = 1.0 + d[2] * env / (r + 1e-6);
            v.x += amount * a.x * scale;
            v.y += amount * a.y * scale;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let rad = max(abs(D(0)), 0.01);
let w = max(abs(D(1)), 0.01);
let r = sqrt(a.x * a.x + a.y * a.y);
let ring = r - rad;
let scale = 1.0 + D(2) * exp(-(ring * ring) / (w * w)) / (r + 1e-6);
v.x += AMOUNT * a.x * scale;
v.y += AMOUNT * a.y * scale;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hamid_js",
        params: &[
            ParamDef::int("presetId", 0.0, 0.0, 19.0),
            ParamDef::int("number_lines/circles", 2000.0, 1.0, 100000.0),
            ParamDef::int("filled circles", 1.0, 0.0, 1.0),
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 1.0),
            ParamDef::new("c", 1.0),
            ParamDef::new("d", 1.0),
            ParamDef::new("e", 1.0),
            ParamDef::new("f", 1.0),
            ParamDef::new("g", 1.0),
            ParamDef::new("h", 1.0),
            ParamDef::new("i", 1.0),
            ParamDef::new("j", 1.0),
            ParamDef::new("k", 1.0),
            ParamDef::new("l", 1.0),
            ParamDef::new("m", 1.0),
            ParamDef::new("n", 1.0),
            ParamDef::new("o", 1.0),
            ParamDef::new("p", 1.0)
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let fi = ((ctx.random() * d[1] + 1.0) as i32) as f64;
            let c = hamid_curve(d, fi);
            let (xout, yout) = if (d[0] as i32) < 10 {
                let [x1, y1, x2, y2] = c;
                let (xdiff, ydiff) = (x2 - x1, y2 - y1);
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
                (x1 + xoffset, y1 + yoffset)
            } else {
                let mut radius = c[2];
                if d[2] as i32 == 1 && ctx.random() > 0.05 {
                    radius *= ctx.random();
                }
                let ang = ctx.random() * 2.0 * M_PI;
                (c[0] + radius * ang.cos(), c[1] + radius * ang.sin())
            };
            v.x += amount * xout;
            v.y += amount * yout;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(ftoi(p[0]), 0, 19) as f64;
            d[1] = p[1].trunc();
            d[2] = p[2].trunc();
            d
        }),
        wgsl: "fn hamid_js_square(n_in: i32) -> vec2<f32> {
  var n = n_in % 400;
  if (n < 100) { return vec2<f32>(f32(n) / 100.0, 0.0); }
  if (n < 200) { return vec2<f32>(1.0, (f32(n) - 100.0) / 100.0); }
  if (n < 300) { return vec2<f32>(f32(300 - n) / 100.0, 1.0); }
  return vec2<f32>(0.0, f32(400 - n) / 100.0);
}
fn hamid_js_curve(base: u32, fi: f32) -> vec4<f32> {
  let np = prm[base + 1u];
  let pa = prm[base + 3u];
  let pb = prm[base + 4u];
  let pc = prm[base + 5u];
  let pd = prm[base + 6u];
  let pe = prm[base + 7u];
  let pf = prm[base + 8u];
  let pg = prm[base + 9u];
  let ph = prm[base + 10u];
  let pi = prm[base + 11u];
  let pj = prm[base + 12u];
  let pk = prm[base + 13u];
  let pl = prm[base + 14u];
  let pm = prm[base + 15u];
  let pn = prm[base + 16u];
  let po = prm[base + 17u];
  let pp = prm[base + 18u];
  switch (i32(prm[base])) {
    case 0: { return vec4<f32>(-sin(4.0 * pa * PI * fi / np), -cos(2.0 * pb * PI * fi / np), -0.5 * pe * sin(8.0 * pc * PI * fi / np), -0.5 * pf * cos(4.0 * pd * PI * fi / np)); }
    case 1: { return vec4<f32>(-sin(2.0 * pa * PI * fi / np), -cos(2.0 * pb * PI * fi / np), -0.5 * pe * sin(8.0 * pc * PI * fi / np), -0.5 * pf * cos(12.0 * pd * PI * fi / np)); }
    case 2: { return vec4<f32>(-sin(8.0 * pa * PI * fi / np), -cos(2.0 * pb * PI * fi / np), -0.5 * pe * sin(6.0 * pc * PI * fi / np), -0.5 * pf * cos(2.0 * pd * PI * fi / np)); }
    case 3: { return vec4<f32>(-sin(10.0 * pa * PI * fi / np), -cos(2.0 * pb * PI * fi / np), -0.5 * pe * sin(12.0 * pc * PI * fi / np), -0.5 * pf * cos(2.0 * pd * PI * fi / np)); }
    case 4: { return vec4<f32>(sin(10.0 * pa * PI * (fi + 699.0) / np), cos(8.0 * pb * PI * (fi + 699.0) / np), sin(12.0 * pc * PI * (fi + 699.0) / np), cos(10.0 * pd * PI * (fi + 699.0) / np)); }
    case 5: { return vec4<f32>(-2.0 * pa * cos(4.0 * pb * PI * fi / 1000.0), 0.5 * pc * java_pow(cos(6.0 * pd * PI * fi / np), 3.0 * pi), -2.0 * pe * sin(6.0 * pf * PI * fi / np) / 15.0 * pj, (4.0 * pg * sin(2.0 * ph * PI * fi / np) / 15.0 * pk)); }
    case 6: { return vec4<f32>(3.0 * pa * java_pow(sin(2.0 * pb * PI * fi / np), 3.0 * ph), -cos(8.0 * pc * PI * fi / np), 1.5 * pd * java_pow(sin(2.0 * pe * PI * fi / np), 3.0 * pi), -0.5 * pf * cos(6.0 * pg * PI * fi / np)); }
    case 7: { return vec4<f32>(cos(6.0 * pa * PI * fi / np), -cos(8.0 * pb * PI * fi / np), 1.5 * pc * java_pow(sin(2.0 * PI * fi / np), 3.0 * pf), -0.5 * pd * cos(6.0 * pe * PI * fi / np)); }
    case 8: { return vec4<f32>(java_pow(sin(20.0 * pa * PI * fi / np), 3.0), java_pow(cos(18.0 * pb * PI * fi / np), 3.0 * pe), java_pow(sin(16.0 * pc * PI * fi / np), 3.0), java_pow(cos(14.0 * pd * PI * fi / np), 3.0 * pf)); }
    case 9: { return vec4<f32>(hamid_js_square(i32(fi)), hamid_js_square(i32(fi) + 199)); }
    case 10: { return vec4<f32>(2.0 * pa * sin(2.0 * pb * PI * fi / np) / 3.0 + sin(36.0 * pc * PI * fi / np) * cos(30.0 * pd * PI * fi / np) / 3.0 * pk + java_pow(sin(14.0 * PI * fi / np), 3.0 * pl) / 3.0 * pm, 2.0 * pe * cos(2.0 * pf * PI * fi / np) / 3.0 * pn + cos(36.0 * pg * PI * fi / np) * cos(30.0 * ph * PI * fi / np) / 3.0 * po + java_pow(cos(14.0 * pi * PI * fi / np), 3.0) / 5.0 * pp, 1.0 / 150.0 + java_pow(sin(36.0 * pj * PI * fi / np), 4.0) / 9.0, 0.0); }
    case 11: { return vec4<f32>(sin(14.0 * pa * PI * fi / np), cos(32.0 * pb * PI * fi / np), java_pow(sin(20.0 * pc * PI * fi / np), 2.0) / 3.0 * pd, 0.0); }
    case 12: { return vec4<f32>(2.0 * pa * sin(6.0 * pb * PI * fi / np) / 3.0 + sin(36.0 * pc * PI * fi / np) * cos(30.0 * pd * PI * fi / np) / 4.0 + java_pow(sin(14.0 * pe * PI * fi / np), 3.0 * pk) / 4.0 * pl, 2.0 * pf * cos(6.0 * pg * PI * fi / np) / 3.0 + cos(28.0 * ph * PI * fi / np) * cos(30.0 * pi * PI * fi / np) / 3.0 + java_pow(cos(14.0 * PI * fi / np), 3.0 * pm) / 5.0 * pn, 1.0 / 150.0 + java_pow(sin(36.0 * pj * PI * fi / np), 4.0 * po) / 20.0 * pp, 0.0); }
    case 13: { return vec4<f32>(sin(10.0 * pa * PI * fi / np), java_pow(cos(28.0 * pb * PI * fi / np), 3.0 * pe), java_pow(sin(18.0 * pc * PI * fi / np), 2.0) / 3.0 * pd, 0.0); }
    case 14: { return vec4<f32>(2.0 * pa * sin(18.0 * pb * PI * fi / np) / 3.0 + java_pow(sin(40.0 * pc * PI * fi / np), 3.0) * cos(30.0 * pd * PI * fi / np) / 3.0 + java_pow(sin(34.0 * pe * PI * fi / np), 3.0 * pk) / 4.0 * pl, 2.0 * pf * cos(18.0 * pg * PI * fi / np) / 3.0 + java_pow(cos(28.0 * ph * PI * fi / np), 3.0) * cos(40.0 * PI * fi / np) / 3.0 + java_pow(cos(34.0 * pi * PI * fi / np), 3.0 * pm) / 3.0 * pn, 1.0 / 150.0 + java_pow(sin(56.0 * pj * PI * fi / np), 4.0 * po) / 8.0 * pp, 0.0); }
    case 15: { return vec4<f32>(2.0 * pa * sin(14.0 * pb * PI * fi / np) / 3.0 + java_pow(sin(10.0 * pc * PI * fi / np), 3.0) * cos(28.0 * pd * PI * fi / np) / 3.0 + java_pow(sin(46.0 * pe * PI * fi / np), 3.0 * pm) / 3.0 * pn, 2.0 * cos(14.0 * pf * PI * fi / np) / 3.0 + java_pow(cos(10.0 * pg * PI * fi / np), 3.0) * cos(28.0 * ph * PI * fi / np) / 3.0 + java_pow(cos(34.0 * pj * PI * fi / np), 3.0 * po) / 3.0, 1.0 / 150.0 + java_pow(sin(72.0 * pk * PI * fi / np), 6.0) / 9.0 + java_pow(sin(12.0 * pl * PI * fi / np), 4.0) / 20.0 * pp, 0.0); }
    case 16: { return vec4<f32>(sin(38.0 * pa * PI * fi / np) * cos(26.0 * pb * PI * fi / np) / 2.0 + java_pow(sin(18.0 * pc * PI * fi / np), 3.0 * pl) / 2.0 * pm, 2.0 * cos(18.0 * pd * PI * fi / np) / 3.0 + cos(38.0 * pe * PI * fi / np) * cos(36.0 * pf * PI * fi / np) / 3.0 + java_pow(cos(18.0 * pg * PI * fi / np), 3.0 * pn) / 4.0 * po, 1.0 / 150.0 + java_pow(sin(18.0 * ph * PI * fi / np), 10.0) * java_pow(sin(100.0 * pi * PI * fi / np), 6.0) / 100.0 + java_pow(sin(72.0 * pj * PI * fi / np), 8.0) * java_pow(cos(18.0 * pk * PI * fi / np), 6.0) / 20.0 * pp, 0.0); }
    case 17: { return vec4<f32>(2.0 * pa * sin(10.0 * pb * PI * fi / np) + java_pow(sin(6.0 * pc * PI * fi / np), 3.0) * cos(28.0 * pd * PI * fi / np) / 3.0 + sin(126.0 * pe * PI * fi / np) / 10.0 * pl, 2.0 * cos(6.0 * pf * PI * fi / np) / 3.0 + java_pow(cos(18.0 * pg * PI * fi / np), 3.0) * cos(36.0 * ph * PI * fi / np) / 3.0 + cos(126.0 * pi * PI * fi / np) / 10.0 * pm, 1.0 / 150.0 + java_pow(sin(48.0 * pj * PI * fi / np), 4.0) * (1.0 + java_pow(sin(12.0 * pk * PI * fi / np), 2.0 * pn)) / 18.0 * po, 0.0); }
    case 18: { return vec4<f32>(2.0 * pa * sin(18.0 * pb * PI * fi / np) / 5.0 + java_pow(sin(34.0 * pc * PI * fi / np), 3.0 * pj) / 3.0 * pk + java_pow(sin(50.0 * pd * PI * fi / np), 3.0 * pl) / 3.0 * pm, 2.0 * cos(18.0 * pe * PI * fi / np) / 3.0 + java_pow(cos(34.0 * pf * PI * fi / np), 3.0) / 3.0 + java_pow(cos(50.0 * pg * PI * fi / np), 3.0 * pn) / 10.0 * po, 1.0 / 150.0 + java_pow(sin(58.0 * pi * PI * fi / np), 4.0) / 10.0 * pp, 0.0); }
    default: { return vec4<f32>(cos(10.0 * pa * PI * fi / np) * (1.0 * pc - java_pow(sin(16.0 * pb * PI * fi / np), 2.0 * ph) / 2.0 * pi), sin(10.0 * pd * PI * fi / np) * (1.0 * pf - java_pow(cos(34.0 * pe * PI * fi / np), 2.0 * pj) / 2.0 * pk), 1.0 / 200.0 + java_pow(sin(52.0 * pg * PI * fi / np), 4.0 * pl) / 10.0 * pm, 0.0); }
  }
}
@body
let fi = f32(i32(rnd() * D(1) + 1.0));
let c = hamid_js_curve(base, fi);
var q = vec2<f32>(0.0);
if (i32(D(0)) < 10) {
  let xdiff = c.z - c.x;
  let ydiff = c.w - c.y;
  var m = 10000.0;
  if (xdiff != 0.0) { m = ydiff / xdiff; }
  let dd = rnd() * sqrt(xdiff * xdiff + ydiff * ydiff);
  var xoffset = dd / sqrt(1.0 + m * m);
  if (c.z < c.x) { xoffset = -xoffset; }
  var yoffset = abs(m * xoffset);
  if (c.w < c.y) { yoffset = -yoffset; }
  q = c.xy + vec2<f32>(xoffset, yoffset);
} else {
  var radius = c.z;
  if (i32(D(2)) == 1 && rnd() > 0.05) { radius *= rnd(); }
  let ang = rnd() * TWO_PI;
  q = c.xy + radius * vec2<f32>(cos(ang), sin(ang));
}
v.x += AMOUNT * q.x;
v.y += AMOUNT * q.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hammer",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let lat = a.y * d[0];
            let lon = a.x * d[0];
            let cl = lat.cos();
            let dd = (1.0 + cl * (lon * 0.5).cos()).sqrt() + 0.001;
            v.x += amount * 2.82842712474619 * cl * (lon * 0.5).sin() / dd;
            v.y += amount * 1.414213562373095 * lat.sin() / dd;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let lat = a.y * D(0);
let lon = a.x * D(0);
let cl = cos(lat);
let dd = sqrt(1.0 + cl * cos(lon * 0.5)) + 0.001;
v.x += AMOUNT * 2.82842712474619 * cl * sin(lon * 0.5) / dd;
v.y += AMOUNT * 1.414213562373095 * sin(lat) / dd;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];
