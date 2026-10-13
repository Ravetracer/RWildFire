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

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;
use super::glsl::{Vec2, Vec3, dc_output, vec2};
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
            // JWildfire sums a sliding window of four randoms (one new per call); four fresh
            // ones give the same distribution per sample
            let rr = d[5] * (ctx.random() + ctx.random() + ctx.random() + ctx.random() - 2.0);
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
        wgsl: "let ang = rnd() * TWO_PI;
let rr = D(5) * (rnd() + rnd() + rnd() + rnd() - 2.0);
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
];
