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

//! Variations ported from the JWildfire sources (batch w14).

// Some variations use JWildfire's truncated pi literals.
#![allow(clippy::approx_constant)]

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

/// `Hexaplay3DFunc` / `Hexnix3DFunc` segment cycling: JWildfire draws a mode (60 degree steps for
/// `rswtch <= 1`) per instance and redraws it after 6 calls in 60 degree mode or 3 calls in 120
/// degree mode. On the CPU the counters are kept per render thread; returns (mode60, fcycle,
/// bcycle) before the step.
fn hex_lattice_step(d: &[f64], ctx: &mut Ctx) -> (bool, usize, usize) {
    let key = d.as_ptr() as usize;
    let st = match ctx.orbits.get(&key) {
        Some(st) => *st,
        // init: rswtch, fcycle = bcycle = 0
        None => [(ctx.random() * 3.0).trunc(), 0.0],
    };
    let mut rswtch = st[0] as i32;
    let (mut fcycle, mut bcycle) = ((st[1] as usize) % 8, (st[1] as usize) / 8);
    if fcycle > 5 {
        fcycle = 0;
        rswtch = (ctx.random() * 3.0).trunc() as i32;
    }
    if bcycle > 2 {
        bcycle = 0;
        rswtch = (ctx.random() * 3.0).trunc() as i32;
    }
    let mode60 = rswtch <= 1;
    let next = if mode60 { (fcycle + 1, bcycle) } else { (fcycle, bcycle + 1) };
    ctx.orbits.insert(key, [rswtch as f64, (next.0 + 8 * next.1) as f64]);
    (mode60, fcycle, bcycle)
}

/// `HexPrism3DFunc.sdHexPrism`
fn hexprism3d_sd(p: [f64; 3], hx: f64, hy: f64) -> f64 {
    let k = [-0.8660254, 0.5, 0.57735];
    let mut p = p.map(f64::abs);
    let t = 2.0 * java_min(k[0] * p[0] + k[1] * p[1], 0.0);
    p[0] -= k[0] * t;
    p[1] -= k[1] * t;
    let cx = java_min(java_max(p[0], -k[2] * hx), k[2] * hx);
    let (ex, ey) = (p[0] - cx, p[1] - hx);
    let sgn = if p[1] - hx > 0.0 { 1.0 } else if p[1] - hx < 0.0 { -1.0 } else { 0.0 };
    let (dx, dy) = ((ex * ex + ey * ey).sqrt() * sgn, p[2] - hy);
    let (mx, my) = (java_max(dx, 0.0), java_max(dy, 0.0));
    java_min(java_max(dx, dy), 0.0) + (mx * mx + my * my).sqrt()
}

/// `HilbertFunc` turtle path as a polyline: d = [level, show_lines, line_thickness, show_points,
/// point_thickness, line_threshold, point_threshold, line_thickness / 100, point_thickness / 100,
/// segments, x0, y0, x1, y1, ...].
fn hilbert_prepare(p: &[f64]) -> Vec<f64> {
    let level = limit_value(ftoi(p[0]), 1, 10);
    struct Turtle {
        x: f64,
        y: f64,
        angle: f64,
        pts: Vec<f64>,
    }
    impl Turtle {
        fn forward(&mut self) {
            self.x += self.angle.to_radians().cos();
            self.y += self.angle.to_radians().sin();
            self.pts.extend([self.x, self.y]);
        }
    }
    fn draw(t: &mut Turtle, n: i32, sign: f64) {
        if n == 0 {
            return;
        }
        t.angle += 90.0 * sign;
        draw(t, n - 1, -sign);
        t.forward();
        t.angle -= 90.0 * sign;
        draw(t, n - 1, sign);
        t.forward();
        draw(t, n - 1, sign);
        t.angle -= 90.0 * sign;
        t.forward();
        draw(t, n - 1, -sign);
        t.angle += 90.0 * sign;
    }
    let mut t = Turtle { x: 0.0, y: 0.0, angle: 0.0, pts: vec![0.0, 0.0] };
    draw(&mut t, level, 1.0);
    let (show_lines, show_points) = (p[1].trunc(), p[3].trunc());
    let sum = show_lines + show_points;
    let line_fraction = show_lines / sum;
    let mut d = vec![level as f64, show_lines, p[2], show_points, p[4], line_fraction, line_fraction + show_points / sum, p[2] / 100.0, p[4] / 100.0];
    d.push((t.pts.len() / 2 - 1) as f64);
    d.extend_from_slice(&t.pts);
    d
}

/// `HtreeFunc.draw`: three segments per H, d = [level, size, show_lines, line_thickness,
/// show_points, point_thickness, line_threshold, point_threshold, line_thickness / 100,
/// point_thickness / 100, segments, x1, y1, x2, y2, ...].
fn htree_prepare(p: &[f64]) -> Vec<f64> {
    let level = limit_value(ftoi(p[0]), 1, 10);
    fn draw(segs: &mut Vec<f64>, n: i32, x: f64, y: f64, size: f64) {
        if n == 0 {
            return;
        }
        let (x0, x1, y0, y1) = (x - size / 2.0, x + size / 2.0, y - size / 2.0, y + size / 2.0);
        segs.extend([x0, y0, x0, y1, x1, y0, x1, y1, x0, y, x1, y]);
        draw(segs, n - 1, x0, y0, size / 2.0);
        draw(segs, n - 1, x0, y1, size / 2.0);
        draw(segs, n - 1, x1, y0, size / 2.0);
        draw(segs, n - 1, x1, y1, size / 2.0);
    }
    let mut segs = Vec::new();
    draw(&mut segs, level, 0.0, 0.0, p[1]);
    let (show_lines, show_points) = (p[2].trunc(), p[4].trunc());
    let sum = show_lines + show_points;
    let lf = show_lines / sum;
    let mut d = vec![level as f64, p[1], show_lines, p[3], show_points, p[5], lf, lf + show_points / sum, p[3] / 100.0, p[5] / 100.0];
    d.push((segs.len() / 4) as f64);
    d.extend_from_slice(&segs);
    d
}

/// GPU stand-in for `random(Integer.MAX_VALUE) * step`: the number of distinct angles
/// (`n * step` a multiple of 2 pi), or 0 if there are too many (then a uniform angle is used).
fn angle_period(step: f64) -> f64 {
    let turns = step / (2.0 * M_PI);
    (1..=10000).find(|&n| {
        let t = n as f64 * turns;
        (t - t.round()).abs() < 1e-9
    })
    .map_or(0.0, |n| n as f64)
}

/// Shared tail of `hypertile3D2` / `hypertile3D2b`: d[o..] = [pa, period, cx, c2, c2x, s2x, s2y,
/// s2z, h].
fn hypertile3d2_apply(d: &[f64], o: usize, ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let (pa, cx, c2, c2x, s2x, s2y, s2z, h) = (d[o], d[o + 2], d[o + 3], d[o + 4], d[o + 5], d[o + 6], d[o + 7], d[o + 8]);
    let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
    let x = a.x * s2x - cx * (-r2 - 1.0);
    let y = a.y * s2y;
    let vr = amount / (c2 * r2 + c2x * a.x + h);
    let ang = ctx.random_int(i32::MAX as usize) as f64 * pa;
    let (sina, cosa) = ang.sin_cos();
    v.x += vr * (x * cosa + y * sina);
    v.y += vr * (y * cosa - x * sina);
    v.z += vr * (a.z * s2z);
}

/// `InversionFunc` shape point `(r, x, y)` at angle `tin`; d = [scale, rotation, shape, imode,
/// hide_uninverted, a, b, c, d, e, f, ring_mode, ring_scale, pnorm_point, pnorm_shape, pnorm_pmod,
/// pnorm_smod, draw_shape, shape_thickness, passthrough, guides_enabled, color_measure,
/// color_gradient, color_low_threshold, color_high_threshold, rotation (radians), ring_rmin,
/// ring_rmax, period].
fn inversion_curve(d: &[f64], tin: f64) -> (f64, f64, f64) {
    let (scale, a, b, c) = (d[0], d[5], d[6], d[7]);
    let t = tin - d[25];
    let r = match d[2] as i32 {
        1 => (a * b) / ((b * t.cos()) * (b * t.cos()) + (a * t.sin()) * (a * t.sin())).sqrt(),
        2 => ((a * a * b * b) / ((b * b * t.cos() * t.cos()) - (a * a * t.sin() * t.sin()))).sqrt(),
        3 => {
            let theta = ((tin - d[25]) % M_2PI).abs();
            let n = a.floor();
            (M_PI / n).cos() / (theta % (M_2PI / n) - M_PI / n).cos()
        }
        4 => (a / b * t).cos() + c,
        5 => (a * b) / ((a * t.sin()).abs().powf(c) + (b * t.cos()).abs().powf(c)).powf(1.0 / c),
        6 => (((t * c / 4.0).cos() / a).abs().powf(d[9]) + ((t * c / 4.0).sin() / b).abs().powf(d[10])).powf(-1.0 / d[8]),
        _ => 1.0,
    } * scale;
    (r, r * tin.cos(), r * tin.sin())
}

/// `Rhodonea.calcPeriod`
fn inversion_rhodonea_period(a: f64, b: f64, c: f64) -> f64 {
    let k = a / b;
    if k % 1.0 == 0.0 {
        if k % 2.0 == 0.0 || c != 0.0 { M_2PI } else { M_PI }
    } else if a % 1.0 == 0.0 && b % 1.0 == 0.0 {
        let (mut kn, mut kd) = (a, b);
        let (mut x, mut y) = ((kn as i64).abs(), (kd as i32 as i64).abs());
        while y != 0 {
            (x, y) = (y, x % y);
        }
        let gcd = x as f64;
        if gcd != 1.0 {
            kn /= gcd;
            kd /= gcd;
        }
        if kn % 2.0 == 0.0 || kd % 2.0 == 0.0 { kd * M_2PI } else { kd * M_PI }
    } else {
        M_2PI
    }
}

fn inversion_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let (xin, yin) = (a.x, a.y);
    if d[17] > 0.0 && ctx.random() < d[17] {
        let (_, cx, cy) = inversion_curve(d, ctx.random() * d[28]);
        v.x += cx;
        v.y += cy;
        if d[18] != 0.0 {
            v.x += 0.01 * (ctx.random() - 0.5) * d[18];
            v.y += 0.01 * (ctx.random() - 0.5) * d[18];
        }
        return;
    }
    if d[19] > 0.0 && ctx.random() < d[19] {
        v.x += xin;
        v.y += yin;
        return;
    }
    let tin = yin.atan2(xin);
    let rin = (xin * xin + yin * yin).sqrt();
    // getMaxCurvePoint
    let (rc, xc, yc) = if d[2] as i32 == 4 {
        let mut best = (-0.0001, 0.0, 0.0);
        for i in 0..(d[28] / M_2PI) as i32 {
            let (r, x, y) = inversion_curve(d, tin + i as f64 * M_PI);
            if r.abs() > best.0 {
                best = (r.abs(), x, y);
            }
        }
        best
    } else {
        inversion_curve(d, tin)
    };
    let mut invert = match d[3] as i32 {
        1 => rin > rc,
        2 => rin < rc,
        _ => true,
    };
    let (ring_mode, rmin, rmax) = (d[11] as i32, d[26], d[27]);
    if invert && ring_mode != 0 && d[12] != 1.0 {
        if ring_mode == 1 {
            invert = rin >= rmin && rin <= rmax;
        } else if ring_mode == 2 {
            invert = rin <= rmin || rin >= rmax;
        }
    }
    if !invert {
        v.x += xin;
        v.y += yin;
        if ctx.preserve_z {
            v.z += a.z;
        }
        v.do_hide = d[4] == 1.0;
        return;
    }
    let (ps, pp) = (d[14], d[13]);
    let num = if ps == 2.0 && d[16] == 1.0 { rc * rc } else { (xc.abs().powf(ps) + yc.abs().powf(ps)).powf(2.0 / (ps * d[16])) };
    let den = if pp == 2.0 && d[15] == 1.0 { xin * xin + yin * yin } else { (xin.abs().powf(pp) + yin.abs().powf(pp)).powf(2.0 / (pp * d[15])) };
    let iscale = num / den;
    v.x += amount * iscale * xin;
    v.y += amount * iscale * yin;
    ctx.pass_z(a, v, amount);
    v.do_hide = false;
    // setColor
    let val = match d[21] as i32 {
        1 => ((v.x - xc) * (v.x - xc) + (v.y - yc) * (v.y - yc)).sqrt(),
        2 => ((a.x - xc) * (a.x - xc) + (a.y - yc) * (a.y - yc)).sqrt(),
        3 => ((v.x - a.x) * (v.x - a.x) + (v.y - a.y) * (v.y - a.y)).sqrt(),
        4 => {
            let r_shape = (v.x * v.x + v.y * v.y).sqrt();
            let is = (d[0] * d[0]) / (a.x * a.x + a.y * a.y);
            let (cx, cy) = (is * a.x * amount, is * a.y * amount);
            r_shape / (cx * cx + cy * cy).sqrt()
        }
        _ => return,
    };
    let (mut lo, mut hi) = (d[23], d[24]);
    if lo > hi {
        std::mem::swap(&mut lo, &mut hi);
    }
    let base = if d[22] as i32 == 2 {
        let mut w = val;
        if w < lo {
            w = hi - ((lo - w) % (hi - lo));
        } else if w > hi {
            w = lo + ((w - lo) % (hi - lo));
        }
        ((w - lo) / (hi - lo)) * 255.0
    } else if val < lo {
        0.0
    } else if val >= hi {
        255.0
    } else {
        ((val - lo) / (hi - lo)) * 255.0
    };
    v.rgb_color = false;
    v.color = (base / 255.0).clamp(0.0, 1.0);
}

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "harmonograph_js",
        // Defaults: JWildfire's values for seed 1234 (the damping values are random there).
        params: &[
            ParamDef::int("seed", 1234.0, 0.0, 100000.0),
            ParamDef::int("time", 100.0, 1.0, 10000.0),
            ParamDef::int("a1", 128.0, 0.0, 180.0),
            ParamDef::new("f1", 5.0),
            ParamDef::int("p1", 113.0, 0.0, 180.0),
            ParamDef::new("d1", 0.001),
            ParamDef::int("a2", 0.0, 0.0, 180.0),
            ParamDef::new("f2", 3.0),
            ParamDef::int("p2", 33.0, 0.0, 180.0),
            ParamDef::new("d2", 0.001),
            ParamDef::int("a3", 49.0, 0.0, 180.0),
            ParamDef::new("f3", -4.0),
            ParamDef::int("p3", 57.0, 0.0, 90.0),
            ParamDef::new("d3", 0.001),
            ParamDef::int("a4", 97.0, 0.0, 180.0),
            ParamDef::new("f4", 1.0),
            ParamDef::int("p4", 68.0, 0.0, 90.0),
            ParamDef::new("d4", 0.001),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let mo = |t: f64, k: usize, dp: f64| {
                let (pa, pb) = ((d[k + 2] + dp) * M_PI / 180.0, (d[k + 6] + dp) * M_PI / 180.0);
                d[k] * (t * d[k + 1] + pa).sin() * (-d[k + 3] * t).exp() + d[k + 4] * (t * d[k + 5] + pb).sin() * (-d[k + 7] * t).exp()
            };
            let t = ctx.random() * d[1];
            v.x += mo(t, 2, 0.0) / 180.0 * amount;
            v.y += mo(t, 10, 90.0) / 180.0 * amount;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for i in [1, 2, 4, 6, 8, 10, 12, 14, 16] {
                d[i] = p[i].trunc();
            }
            d
        }),
        wgsl: "fn harmonograph_js_mo(base: u32, t: f32, k: u32, dp: f32) -> f32 {
  let pa = (prm[base + k + 2u] + dp) * PI / 180.0;
  let pb = (prm[base + k + 6u] + dp) * PI / 180.0;
  return prm[base + k] * sin(t * prm[base + k + 1u] + pa) * exp(-prm[base + k + 3u] * t)
    + prm[base + k + 4u] * sin(t * prm[base + k + 5u] + pb) * exp(-prm[base + k + 7u] * t);
}
@body
let t = rnd() * D(1);
v.x += harmonograph_js_mo(base, t, 2u, 0.0) / 180.0 * AMOUNT;
v.y += harmonograph_js_mo(base, t, 10u, 90.0) / 180.0 * AMOUNT;",
    },
    VarDef {
        name: "helicoid",
        params: &[ParamDef::new("frequency", 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let range = (a.x * a.x + a.y * a.y).sqrt();
            let ang = a.z * M_2PI * d[0] + a.y.atan2(a.x);
            v.x += amount * ang.cos() * range;
            v.y += amount * ang.sin() * range;
            v.z += amount * a.z;
        },
        prepare: None,
        wgsl: "let range = sqrt(a.x * a.x + a.y * a.y);
let ang = a.z * TWO_PI * D(0) + atan2(a.y, a.x);
v.x += AMOUNT * cos(ang) * range;
v.y += AMOUNT * sin(ang) * range;
v.z += AMOUNT * a.z;",
    },
    VarDef {
        name: "helix",
        params: &[ParamDef::new("frequency", 1.0), ParamDef::new("width", 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let ang = a.z * M_2PI * d[0];
            v.x += amount * (a.x + ang.cos() * d[1]);
            v.y += amount * (a.y + ang.sin() * d[1]);
            v.z += amount * a.z;
        },
        prepare: None,
        wgsl: "let ang = a.z * TWO_PI * D(0);
v.x += AMOUNT * (a.x + cos(ang) * D(1));
v.y += AMOUNT * (a.y + sin(ang) * D(1));
v.z += AMOUNT * a.z;",
    },
    VarDef {
        name: "henon",
        params: &[ParamDef::new("a", 0.5), ParamDef::new("b", 1.0), ParamDef::new("c", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += (d[2] - (d[0] * (a.x * a.x)) + a.y) * amount;
            v.y += d[1] * a.x * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += (D(2) - (D(0) * (a.x * a.x)) + a.y) * AMOUNT;
v.y += D(1) * a.x * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hexagonal",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sc = java_max(d[0].abs(), 0.0001);
            const SQRT3: f64 = 1.732050807568877;
            let q = (2.0 / 3.0) * a.x / sc;
            let r = ((-1.0 / 3.0) * a.x + (1.0 / SQRT3) * a.y) / sc;
            let s = -q - r;
            let (mut rq, mut rr, rs) = ((q + 0.5).floor(), (r + 0.5).floor(), (s + 0.5).floor());
            let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
            if dq > dr && dq > ds {
                rq = -rr - rs;
            } else if dr > ds {
                rr = -rq - rs;
            }
            v.x += amount * (a.x - sc * 1.5 * rq);
            v.y += amount * (a.y - sc * SQRT3 * (rr + rq * 0.5));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sc = max(abs(D(0)), 0.0001);
let q = (2.0 / 3.0) * a.x / sc;
let r = ((-1.0 / 3.0) * a.x + (1.0 / 1.732050807568877) * a.y) / sc;
let s = -q - r;
var rq = floor(q + 0.5);
var rr = floor(r + 0.5);
let rs = floor(s + 0.5);
let dq = abs(rq - q);
let dr = abs(rr - r);
let ds = abs(rs - s);
if (dq > dr && dq > ds) { rq = -rr - rs; } else if (dr > ds) { rr = -rq - rs; }
v.x += AMOUNT * (a.x - sc * 1.5 * rq);
v.y += AMOUNT * (a.y - sc * 1.732050807568877 * (rr + rq * 0.5));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hexaplay3D",
        params: &[ParamDef::new("majp", 1.0), ParamDef::new("scale", 0.25), ParamDef::new("zlift", 0.25)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            let (mode60, fcycle, bcycle) = hex_lattice_step(d, ctx);
            let scale = d[1] * 0.5;
            let pos_neg = if ctx.random() < 0.5 { -1.0 } else { 1.0 };
            let abmajp = d[0].abs();
            if abmajp <= 1.0 {
                v.z += a.z * 0.5 * d[2];
            } else {
                v.z += a.z * 0.5 * d[2] + pos_neg * ((abmajp - 1.0) * 0.5);
            }
            let h = (M_PI / 3.0).sin();
            let (sx, sy) = if mode60 {
                [(1.0, 0.0), (0.5, h), (-0.5, h), (-1.0, 0.0), (-0.5, -h), (0.5, -h)][fcycle]
            } else {
                [(1.0, 0.0), (-0.5, h), (-0.5, -h)][bcycle]
            };
            v.x = ((v.x + a.x) * scale) + amount * sx;
            v.y = ((v.y + a.y) * scale) + amount * sy;
        },
        prepare: None,
        // GPU: no state across calls; the 60 degree mode is drawn with its long-run share (4/5)
        // and a random segment instead of the next one.
        wgsl: "let mode60 = rnd() < 0.8;
let scale = D(1) * 0.5;
var pos_neg = 1.0;
if (rnd() < 0.5) { pos_neg = -1.0; }
let abmajp = abs(D(0));
if (abmajp <= 1.0) {
  v.z += a.z * 0.5 * D(2);
} else {
  v.z += a.z * 0.5 * D(2) + pos_neg * ((abmajp - 1.0) * 0.5);
}
let h = sin(PI / 3.0);
var sg = vec2<f32>(1.0, 0.0);
if (mode60) {
  let k = min(u32(rnd() * 6.0), 5u);
  let ang = f32(k) * PI / 3.0;
  sg = vec2<f32>(cos(ang), sin(ang));
} else {
  let k = min(u32(rnd() * 3.0), 2u);
  if (k == 1u) { sg = vec2<f32>(-0.5, h); } else if (k == 2u) { sg = vec2<f32>(-0.5, -h); }
}
v.x = (v.x + a.x) * scale + AMOUNT * sg.x;
v.y = (v.y + a.y) * scale + AMOUNT * sg.y;",
    },
    VarDef {
        name: "hex_modulus",
        params: &[ParamDef::new("size", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            const SQRT3_2: f64 = 0.866_025_403_784_438_6;
            const SQRT3: f64 = 1.732_050_807_568_877_2;
            let hsize = SQRT3_2 / d[0];
            let weight = amount / SQRT3_2;
            let (xx, yy) = (a.x * hsize, a.y * hsize);
            let x = 0.577_350_269_189_625_7 * xx - yy / 3.0;
            let z = 2.0 * yy / 3.0;
            let y = -x - z;
            let (mut rx, mut ry, mut rz) = (java_round(x), java_round(y), java_round(z));
            let (dx, dy, dz) = ((rx - x).abs(), (ry - y).abs(), (rz - z).abs());
            if dx > dy && dx > dz {
                rx = -ry - rz;
            } else if dy > dz {
                ry = -rx - rz;
            } else {
                rz = -rx - ry;
            }
            let _ = ry;
            v.x += (xx - (SQRT3 * rx + SQRT3_2 * rz)) * weight;
            v.y += (yy - 1.5 * rz) * weight;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let hsize = 0.8660254037844386 / D(0);
let weight = AMOUNT / 0.8660254037844386;
let xx = a.x * hsize;
let yy = a.y * hsize;
let x = 0.5773502691896257 * xx - yy / 3.0;
let z = 2.0 * yy / 3.0;
let y = -x - z;
var rx = java_round(x);
var ry = java_round(y);
var rz = java_round(z);
let dx = abs(rx - x);
let dy = abs(ry - y);
let dz = abs(rz - z);
if (dx > dy && dx > dz) { rx = -ry - rz; } else if (dy > dz) { ry = -rx - rz; } else { rz = -rx - ry; }
v.x += (xx - (1.7320508075688772 * rx + 0.8660254037844386 * rz)) * weight;
v.y += (yy - 1.5 * rz) * weight;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hexnix3D",
        params: &[ParamDef::new("majp", 1.0), ParamDef::new("scale", 0.25), ParamDef::new("zlift", 0.0), ParamDef::new("3side", 0.67)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            let (mode60, _, _) = hex_lattice_step(d, ctx);
            let (majp, scale, zlift, scale3) = (d[0], d[1], d[2], d[3]);
            let smooth = if amount.abs() <= 0.5 { amount * 2.0 } else { 1.0 };
            let pos_neg = if ctx.random() < 0.5 { -1.0 } else { 1.0 };
            let abmajp = majp.abs();
            let (majplane, boost) = if abmajp <= 1.0 {
                (0, 0.0)
            } else if abmajp > 1.0 && abmajp < 2.0 {
                (1, 0.0)
            } else {
                (2, (abmajp - 2.0) * 0.5)
            };
            if majplane == 0 {
                v.z += smooth * a.z * scale * zlift;
            } else if majplane == 1 && majp < 0.0 {
                let gentle = if (-2.0..-1.0).contains(&majp) { abmajp - 1.0 } else { 1.0 };
                if pos_neg < 0.0 {
                    v.z += -2.0 * (v.z * gentle);
                }
            }
            if majplane == 2 && majp < 0.0 {
                if pos_neg > 0.0 {
                    v.z += smooth * (a.z * scale * zlift + boost);
                } else {
                    v.z = (v.z - (2.0 * smooth * v.z)) + (smooth * pos_neg * (a.z * scale * zlift + boost));
                }
            } else {
                v.z += smooth * (a.z * scale * zlift + (pos_neg * boost));
            }
            let h = (M_PI / 3.0).sin();
            let ((sx, sy), tail) = if mode60 {
                ([(1.0, 0.0), (0.5, -h), (-0.5, -h), (-1.0, 0.0), (-0.5, h), (0.5, h)][(ctx.random() * 6.0).trunc() as usize], 1.0)
            } else {
                ([(0.0, -1.0), ((7.0 * M_PI / 6.0).cos(), 0.5), ((11.0 * M_PI / 6.0).cos(), 0.5)][(ctx.random() * 3.0).trunc() as usize], scale3)
            };
            let rx_tp = (smooth * scale * v.x * sx) - (smooth * scale * v.y * sy);
            let ry_tp = (smooth * scale * v.y * sx) + (smooth * scale * v.x * sy);
            let rx_ft = (a.x * smooth * scale * sx) - (a.y * smooth * scale * sy);
            let ry_ft = (a.y * smooth * scale * sx) + (a.x * smooth * scale * sy);
            v.x = v.x * (1.0 - smooth) + rx_tp + rx_ft + smooth * amount * tail * sx;
            v.y = v.y * (1.0 - smooth) + ry_tp + ry_ft + smooth * amount * tail * sy;
        },
        prepare: None,
        // GPU: no state across calls; the 60 degree mode is drawn with its long-run share (4/5).
        wgsl: "let mode60 = rnd() < 0.8;
let majp = D(0);
let scale = D(1);
let zlift = D(2);
var smooth_f = 1.0;
if (abs(AMOUNT) <= 0.5) { smooth_f = AMOUNT * 2.0; }
var pos_neg = 1.0;
if (rnd() < 0.5) { pos_neg = -1.0; }
let abmajp = abs(majp);
var majplane = 2;
var boost = (abmajp - 2.0) * 0.5;
if (abmajp <= 1.0) { majplane = 0; boost = 0.0; } else if (abmajp < 2.0) { majplane = 1; boost = 0.0; }
if (majplane == 0) {
  v.z += smooth_f * a.z * scale * zlift;
} else if (majplane == 1 && majp < 0.0) {
  var gentle = 1.0;
  if (majp < -1.0 && majp >= -2.0) { gentle = abmajp - 1.0; }
  if (pos_neg < 0.0) { v.z += -2.0 * (v.z * gentle); }
}
if (majplane == 2 && majp < 0.0) {
  if (pos_neg > 0.0) {
    v.z += smooth_f * (a.z * scale * zlift + boost);
  } else {
    v.z = (v.z - (2.0 * smooth_f * v.z)) + (smooth_f * pos_neg * (a.z * scale * zlift + boost));
  }
} else {
  v.z += smooth_f * (a.z * scale * zlift + (pos_neg * boost));
}
var sg = vec2<f32>(0.0, -1.0);
var tail = D(3);
if (mode60) {
  let k = min(u32(trunc(rnd() * 6.0)), 5u);
  let ang = -f32(k) * PI / 3.0;
  sg = vec2<f32>(cos(ang), sin(ang));
  tail = 1.0;
} else {
  let k = min(u32(trunc(rnd() * 3.0)), 2u);
  if (k == 1u) { sg = vec2<f32>(cos(7.0 * PI / 6.0), 0.5); } else if (k == 2u) { sg = vec2<f32>(cos(11.0 * PI / 6.0), 0.5); }
}
let k = smooth_f * scale;
let rx_tp = k * v.x * sg.x - k * v.y * sg.y;
let ry_tp = k * v.y * sg.x + k * v.x * sg.y;
let rx_ft = a.x * k * sg.x - a.y * k * sg.y;
let ry_ft = a.y * k * sg.x + a.x * k * sg.y;
v.x = v.x * (1.0 - smooth_f) + rx_tp + rx_ft + smooth_f * AMOUNT * tail * sg.x;
v.y = v.y * (1.0 - smooth_f) + ry_tp + ry_ft + smooth_f * AMOUNT * tail * sg.y;",
    },
    VarDef {
        name: "hexprism3D",
        params: &[ParamDef::range("d", 0.3, 0.0, 0.5), ParamDef::range("H", 0.25, 0.0, 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| {
            for _ in 0..50 {
                let p = [ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5];
                if hexprism3d_sd(p, d[0], d[1]) < 0.0 {
                    v.do_hide = false;
                    v.x += amount * p[0];
                    v.y += amount * p[1];
                    v.z += amount * p[2];
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, 0.5), limit_value(p[1], 0.0, 0.5)]),
        wgsl: "fn hexprism3D_sd(p_in: vec3<f32>, h: vec2<f32>) -> f32 {
  let k = vec3<f32>(-0.8660254, 0.5, 0.57735);
  var p = abs(p_in);
  let t = p.xy - 2.0 * min(dot(k.xy, p.xy), 0.0) * k.xy;
  p = vec3<f32>(t, p.z);
  let e = p.xy - vec2<f32>(min(max(p.x, -k.z * h.x), k.z * h.x), h.x);
  let dd = vec2<f32>(length(e) * sign(p.y - h.x), p.z - h.y);
  return min(max(dd.x, dd.y), 0.0) + length(max(dd, vec2<f32>(0.0)));
}
@body
var found = false;
for (var i = 0; i < 50; i++) {
  let p = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  if (hexprism3D_sd(p, vec2<f32>(D(0), D(1))) < 0.0) {
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
        name: "hilbert_js",
        params: &[
            ParamDef::int("level", 2.0, 1.0, 10.0),
            ParamDef::int("show_lines", 1.0, 0.0, 1.0),
            ParamDef::new("line_thickness", 0.5),
            ParamDef::int("show_points", 0.0, 0.0, 1.0),
            ParamDef::new("point_thickness", 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // Java cycles through the segments; here a random segment is drawn.
            let nseg = d[9] as usize;
            let k = ((ctx.random() * nseg as f64) as usize).min(nseg - 1);
            let (x1, y1, x2, y2) = (d[10 + 2 * k], d[11 + 2 * k], d[12 + 2 * k], d[13 + 2 * k]);
            let rnd = ctx.random();
            let (mut xo, mut yo) = (0.0, 0.0);
            if rnd < d[5] {
                let (xdiff, ydiff) = (x2 - x1, y2 - y1);
                let m = if xdiff == 0.0 { 10000.0 } else { ydiff / xdiff };
                let dd = ctx.random() * (xdiff * xdiff + ydiff * ydiff).sqrt();
                let mut xoffset = dd / (1.0 + m * m).sqrt();
                if x2 < x1 {
                    xoffset = -xoffset;
                }
                let mut yoffset = (m * xoffset).abs();
                if y2 < y1 {
                    yoffset = -yoffset;
                }
                if d[7] != 0.0 {
                    xoffset += (ctx.random() - 0.5) * d[7];
                    yoffset += (ctx.random() - 0.5) * d[7];
                }
                xo = x1 + xoffset;
                yo = y1 + yoffset;
            } else if rnd <= d[6] {
                let (mut xoffset, mut yoffset) = (0.0, 0.0);
                if d[8] != 0.0 {
                    let roffset = ctx.random() * d[8];
                    let rangle = ctx.random() * M_2PI;
                    xoffset = roffset * rangle.cos();
                    yoffset = roffset * rangle.sin();
                }
                xo = x1 + xoffset;
                yo = y1 + yoffset;
            }
            v.x += amount * xo;
            v.y += amount * yo;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| hilbert_prepare(p)),
        wgsl: "let nseg = u32(D(9));
let k = min(u32(rnd() * f32(nseg)), nseg - 1u);
let o = base + 10u + 2u * k;
let x1 = prm[o];
let y1 = prm[o + 1u];
let x2 = prm[o + 2u];
let y2 = prm[o + 3u];
let r0 = rnd();
var q = vec2<f32>(0.0);
if (r0 < D(5)) {
  let xdiff = x2 - x1;
  let ydiff = y2 - y1;
  var m = 10000.0;
  if (xdiff != 0.0) { m = ydiff / xdiff; }
  let dd = rnd() * sqrt(xdiff * xdiff + ydiff * ydiff);
  var xoffset = dd / sqrt(1.0 + m * m);
  if (x2 < x1) { xoffset = -xoffset; }
  var yoffset = abs(m * xoffset);
  if (y2 < y1) { yoffset = -yoffset; }
  if (D(7) != 0.0) {
    xoffset += (rnd() - 0.5) * D(7);
    yoffset += (rnd() - 0.5) * D(7);
  }
  q = vec2<f32>(x1 + xoffset, y1 + yoffset);
} else if (r0 <= D(6)) {
  q = vec2<f32>(x1, y1);
  if (D(8) != 0.0) {
    let roffset = rnd() * D(8);
    let rangle = rnd() * TWO_PI;
    q += roffset * vec2<f32>(cos(rangle), sin(rangle));
  }
}
v.x += AMOUNT * q.x;
v.y += AMOUNT * q.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "ho",
        params: &[ParamDef::new("xpow", 3.0), ParamDef::new("ypow", 3.0), ParamDef::new("zpow", 3.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let (u, w0, w) = (a.x, a.y, a.z);
            let ax = (w0 * w0).atan2(w * w);
            let ay = (u * u).atan2(w * w);
            let (sv, cv, su, cu) = (w0.sin(), w0.cos(), u.sin(), u.cos());
            v.x += amount * ((cu * cv).powf(d[0]) + ((cu * cv) * d[0]) + (0.25 * ax));
            v.y += amount * ((su * cv).powf(d[1]) + ((su * cv) * d[1]) + (0.25 * ay));
            v.z += amount * (sv.powf(d[2]) + sv * d[2]);
        },
        prepare: None,
        wgsl: "let ax = atan2(a.y * a.y, a.z * a.z);
let ay = atan2(a.x * a.x, a.z * a.z);
let sv = sin(a.y);
let cv = cos(a.y);
let su = sin(a.x);
let cu = cos(a.x);
v.x += AMOUNT * (java_pow(cu * cv, D(0)) + cu * cv * D(0) + 0.25 * ax);
v.y += AMOUNT * (java_pow(su * cv, D(1)) + su * cv * D(1) + 0.25 * ay);
v.z += AMOUNT * (java_pow(sv, D(2)) + sv * D(2));",
    },
    VarDef {
        name: "hole",
        params: &[ParamDef::new("a", 1.0), ParamDef::int("inside", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let alpha = a.y.atan2(a.x);
            let delta = (alpha / M_PI + 1.0).powf(d[0]);
            let r = if d[1] != 0.0 { amount * delta / (a.x * a.x + a.y * a.y + delta) } else { amount * (a.x * a.x + a.y * a.y + delta).sqrt() };
            v.x += r * alpha.cos();
            v.y += r * alpha.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], limit_value(ftoi(p[1]), 0, 1) as f64]),
        wgsl: "let alpha = atan2(a.y, a.x);
let delta = java_pow(alpha / PI + 1.0, D(0));
var r = AMOUNT * sqrt(a.x * a.x + a.y * a.y + delta);
if (D(1) != 0.0) { r = AMOUNT * delta / (a.x * a.x + a.y * a.y + delta); }
v.x += r * cos(alpha);
v.y += r * sin(alpha);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hole2",
        params: &[
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 2.0),
            ParamDef::new("c", 1.0),
            ParamDef::new("d", 1.0),
            ParamDef::int("inside", 0.0, 0.0, 1.0),
            ParamDef::int("shape", 0.0, 0.0, 9.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let rhosq = a.sumsq();
            let theta = a.atan_yx() * d[3];
            let delta = (theta / M_PI + 1.0).powf(d[0]) * d[2];
            let b = d[1];
            let r1 = match d[5] as i32 {
                0 => rhosq.sqrt() + delta,
                1 => (rhosq + delta).sqrt(),
                2 => (rhosq + (b * theta).sin() + delta).sqrt(),
                3 => (rhosq + theta.sin() + delta).sqrt(),
                4 => (rhosq + theta * theta - delta + 1.0).sqrt(),
                5 => (rhosq + theta.tan().abs() + delta).sqrt(),
                6 => (rhosq * (1.0 + (b * theta).sin()) + delta).sqrt(),
                7 => (rhosq + (0.5 * b * theta).sin().abs() + delta).sqrt(),
                8 => (rhosq + (M_PI * (b * theta).sin()).sin() + delta).sqrt(),
                _ => (rhosq + ((b * theta).sin() + (2.0 * b * theta + M_PI_2).sin()) / 2.0 + delta).sqrt(),
            };
            let r1 = if d[4] != 0.0 { amount / r1 } else { amount * r1 };
            v.x += r1 * theta.cos();
            v.y += r1 * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = limit_value(ftoi(p[4]), 0, 1) as f64;
            d[5] = limit_value(ftoi(p[5]), 0, 9) as f64;
            d
        }),
        wgsl: "let rhosq = sumsq(a);
let theta = atan_yx(a) * D(3);
let delta = java_pow(theta / PI + 1.0, D(0)) * D(2);
let b = D(1);
var r1 = 1.0;
switch (i32(D(5))) {
  case 0: { r1 = sqrt(rhosq) + delta; }
  case 1: { r1 = sqrt(rhosq + delta); }
  case 2: { r1 = sqrt(rhosq + sin(b * theta) + delta); }
  case 3: { r1 = sqrt(rhosq + sin(theta) + delta); }
  case 4: { r1 = sqrt(rhosq + theta * theta - delta + 1.0); }
  case 5: { r1 = sqrt(rhosq + abs(tan(theta)) + delta); }
  case 6: { r1 = sqrt(rhosq * (1.0 + sin(b * theta)) + delta); }
  case 7: { r1 = sqrt(rhosq + abs(sin(0.5 * b * theta)) + delta); }
  case 8: { r1 = sqrt(rhosq + sin(PI * sin(b * theta)) + delta); }
  default: { r1 = sqrt(rhosq + (sin(b * theta) + sin(2.0 * b * theta + HALF_PI)) / 2.0 + delta); }
}
if (D(4) != 0.0) { r1 = AMOUNT / r1; } else { r1 = AMOUNT * r1; }
v.x += r1 * cos(theta);
v.y += r1 * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "holesq",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |_, ctx, a, v, amount| {
            let (x, y) = (amount * a.x, amount * a.y);
            let (fax, fay) = (x.abs(), y.abs());
            if fax + fay > 1.0 {
                v.x += x;
                v.y += y;
            } else if fax > fay {
                v.x += if x < 0.0 { (x + fay - 1.0) * 0.5 } else { (x - fay + 1.0) * 0.5 };
                v.y += y;
            } else {
                v.x += x;
                v.y += if y < 0.0 { (y + fax - 1.0) * 0.5 } else { (y - fax + 1.0) * 0.5 };
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = AMOUNT * a.x;
let y = AMOUNT * a.y;
let fax = abs(x);
let fay = abs(y);
if (fax + fay > 1.0) {
  v.x += x;
  v.y += y;
} else if (fax > fay) {
  var t = (x - fay + 1.0) * 0.5;
  if (x < 0.0) { t = (x + fay - 1.0) * 0.5; }
  v.x += t;
  v.y += y;
} else {
  var t = (y - fax + 1.0) * 0.5;
  if (y < 0.0) { t = (y + fax - 1.0) * 0.5; }
  v.x += x;
  v.y += t;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "honeycomb",
        params: &[ParamDef::new("scale", 1.0), ParamDef::new("pull", 0.7)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sc = java_max(d[0].abs(), 0.01);
            let pull = java_max(0.0, java_min(1.0, d[1]));
            let q = (2.0 / 3.0) * a.x / sc;
            let r2 = (-1.0 / 3.0) * a.x / sc + 0.5773502691896257 * a.y / sc;
            let (mut rq, mut rr, rs) = (java_round(q), java_round(r2), java_round(-q - r2));
            let (dq, dr, ds) = ((rq - q).abs(), (rr - r2).abs(), (rs - (-q - r2)).abs());
            if dq > dr && dq > ds {
                rq = -rr - rs;
            } else if dr > ds {
                rr = -rq - rs;
            }
            let cx = sc * (rq + 0.5 * rr);
            let cy = sc * 0.8660254037844386 * rr;
            v.x += amount * (cx + (a.x - cx) * pull);
            v.y += amount * (cy + (a.y - cy) * pull);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sc = max(abs(D(0)), 0.01);
let pull = clamp(D(1), 0.0, 1.0);
let q = (2.0 / 3.0) * a.x / sc;
let r2 = (-1.0 / 3.0) * a.x / sc + 0.5773502691896257 * a.y / sc;
var rq = java_round(q);
var rr = java_round(r2);
let rs = java_round(-q - r2);
let dq = abs(rq - q);
let dr = abs(rr - r2);
let ds = abs(rs - (-q - r2));
if (dq > dr && dq > ds) { rq = -rr - rs; } else if (dr > ds) { rr = -rq - rs; }
let cx = sc * (rq + 0.5 * rr);
let cy = sc * 0.8660254037844386 * rr;
v.x += AMOUNT * (cx + (a.x - cx) * pull);
v.y += AMOUNT * (cy + (a.y - cy) * pull);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hopalong",
        params: &[
            ParamDef::int("random", 1.0, 0.0, 10000000.0),
            ParamDef::range("a", 0.75, -1.0, 1.0),
            ParamDef::range("b", 0.5, -1.0, 1.0),
            ParamDef::range("c", 0.25, -1.0, 1.0),
            ParamDef::new("startx", 0.0),
            ParamDef::new("starty", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            // Java follows one endless orbit per instance; here one per render thread.
            let p = ctx.orbits.entry(d.as_ptr() as usize).or_insert([d[4], d[5]]);
            let (x0, y0) = (p[0], p[1]);
            // Math.signum
            let sg = if x0 > 0.0 { 1.0 } else if x0 < 0.0 { -1.0 } else { x0 };
            let x1 = y0 - sg * (d[2] * x0 - d[3]).abs().sqrt();
            let y1 = d[1] - x0;
            *p = [x1, y1];
            v.x += amount * x1;
            v.y += amount * y1;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for i in 1..4 {
                d[i] = limit_value(p[i], -1.0, 1.0);
            }
            d
        }),
        // GPU: no state across calls, every call iterates a slightly perturbed start point.
        wgsl: "var x0 = D(4) + 1.0e-2 * (rnd() - 0.5);
var y0 = D(5) + 1.0e-2 * (rnd() - 0.5);
let n = 32 + min(i32(rnd() * 1024.0), 1023);
for (var i = 0; i < n; i++) {
  let x1 = y0 - sign(x0) * sqrt(abs(D(2) * x0 - D(3)));
  y0 = D(1) - x0;
  x0 = x1;
}
v.x += AMOUNT * x0;
v.y += AMOUNT * y0;",
    },
    VarDef {
        name: "hourglass3D",
        params: &[
            ParamDef::new("kx", 1.0),
            ParamDef::new("ky", 1.0),
            ParamDef::new("kz", 1.0),
            // JWildfire: random in 3..6
            ParamDef::new("maxheight", 4.5),
            ParamDef::int("limit", 1.0, 0.0, 1.0),
            ParamDef::int("zero", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [kx, ky, kz, maxheight, limit, zero, h, hb]
        apply: |d, _, a, v, amount| {
            let (vkx, vky, vkz, h) = (amount * d[0], amount * d[1], amount * d[2], d[6]);
            let (sn, cn) = (a.x.sin(), a.x.cos());
            let (sh, ch) = (a.y.sinh(), a.y.cosh());
            if d[4] == 0.0 || ch <= h {
                v.x += vkx * ch * sn;
                v.y += vky * sh;
                v.z += vkz * ch * cn;
            } else if d[5] == 0.0 {
                let sg = if sh > 0.0 { 1.0 } else if sh < 0.0 { -1.0 } else { 0.0 };
                v.x += vkx * h * sn;
                v.y += vky * sg * d[7];
                v.z += vkz * h * cn;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for k in &mut d[0..3] {
                if *k < EPSILON {
                    *k = EPSILON;
                }
            }
            if d[3] < 0.1 {
                d[3] = 0.1;
            }
            d[4] = if p[4] <= EPSILON { 0.0 } else { 1.0 };
            d[5] = if p[5] <= EPSILON { 0.0 } else { 1.0 };
            let h = (1.0 + (d[3] / d[1]) * (d[3] / d[1])).sqrt();
            d.extend([h, (h * h - 1.0).sqrt()]);
            d
        }),
        wgsl: "let vk = AMOUNT * vec3<f32>(D(0), D(1), D(2));
let h = D(6);
let sn = sin(a.x);
let cn = cos(a.x);
let sh = sinh(a.y);
let ch = cosh(a.y);
if (D(4) == 0.0 || ch <= h) {
  v.x += vk.x * ch * sn;
  v.y += vk.y * sh;
  v.z += vk.z * ch * cn;
} else if (D(5) == 0.0) {
  v.x += vk.x * h * sn;
  v.y += vk.y * sign(sh) * D(7);
  v.z += vk.z * h * cn;
}",
    },
    VarDef {
        name: "htree_js",
        params: &[
            ParamDef::int("level", 2.0, 1.0, 10.0),
            ParamDef::new("size", 2.0),
            ParamDef::int("show_lines", 1.0, 0.0, 1.0),
            ParamDef::new("line_thickness", 0.5),
            ParamDef::int("show_points", 0.0, 0.0, 1.0),
            ParamDef::new("point_thickness", 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // Java cycles through the segments; here a random segment is drawn.
            let nseg = d[10] as usize;
            let k = ((ctx.random() * nseg as f64) as usize).min(nseg - 1);
            let o = 11 + 4 * k;
            let (x1, y1, x2, y2) = (d[o], d[o + 1], d[o + 2], d[o + 3]);
            let rnd = ctx.random();
            let (mut xo, mut yo) = (0.0, 0.0);
            if rnd < d[6] {
                let (xdiff, ydiff) = (x2 - x1, y2 - y1);
                let m = if xdiff == 0.0 { 10000.0 } else { ydiff / xdiff };
                let dd = ctx.random() * (xdiff * xdiff + ydiff * ydiff).sqrt();
                let mut xoffset = dd / (1.0 + m * m).sqrt();
                if x2 < x1 {
                    xoffset = -xoffset;
                }
                let mut yoffset = (m * xoffset).abs();
                if y2 < y1 {
                    yoffset = -yoffset;
                }
                if d[8] != 0.0 {
                    xoffset += (ctx.random() - 0.5) * d[8];
                    yoffset += (ctx.random() - 0.5) * d[8];
                }
                xo = x1 + xoffset;
                yo = y1 + yoffset;
            } else if rnd <= d[7] {
                let (mut xoffset, mut yoffset) = (0.0, 0.0);
                if d[9] != 0.0 {
                    let roffset = ctx.random() * d[9];
                    let rangle = ctx.random() * M_2PI;
                    xoffset = roffset * rangle.cos();
                    yoffset = roffset * rangle.sin();
                }
                xo = x1 + xoffset;
                yo = y1 + yoffset;
            }
            v.x += amount * xo;
            v.y += amount * yo;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| htree_prepare(p)),
        wgsl: "let nseg = u32(D(10));
let k = min(u32(rnd() * f32(nseg)), nseg - 1u);
let o = base + 11u + 4u * k;
let x1 = prm[o];
let y1 = prm[o + 1u];
let x2 = prm[o + 2u];
let y2 = prm[o + 3u];
let r0 = rnd();
var q = vec2<f32>(0.0);
if (r0 < D(6)) {
  let xdiff = x2 - x1;
  let ydiff = y2 - y1;
  var m = 10000.0;
  if (xdiff != 0.0) { m = ydiff / xdiff; }
  let dd = rnd() * sqrt(xdiff * xdiff + ydiff * ydiff);
  var xoffset = dd / sqrt(1.0 + m * m);
  if (x2 < x1) { xoffset = -xoffset; }
  var yoffset = abs(m * xoffset);
  if (y2 < y1) { yoffset = -yoffset; }
  if (D(8) != 0.0) {
    xoffset += (rnd() - 0.5) * D(8);
    yoffset += (rnd() - 0.5) * D(8);
  }
  q = vec2<f32>(x1 + xoffset, y1 + yoffset);
} else if (r0 <= D(7)) {
  q = vec2<f32>(x1, y1);
  if (D(9) != 0.0) {
    let roffset = rnd() * D(9);
    let rangle = rnd() * TWO_PI;
    q += roffset * vec2<f32>(cos(rangle), sin(rangle));
  }
}
v.x += AMOUNT * q.x;
v.y += AMOUNT * q.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hyperbolicellipse",
        params: &[ParamDef::new("a", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, _, a, v, amount| {
            let (x, y) = (a.x, a.y);
            v.x = ((x.exp() - (-x).exp()) / 2.0) * (d[0] * y).cos() * amount;
            v.y = ((x.exp() + (-x).exp()) / 2.0) * (d[0] * y).sin() * amount;
        },
        prepare: None,
        wgsl: "v.x = ((exp(a.x) - exp(-a.x)) / 2.0) * cos(D(0) * a.y) * AMOUNT;
v.y = ((exp(a.x) + exp(-a.x)) / 2.0) * sin(D(0) * a.y) * AMOUNT;",
    },
    VarDef {
        name: "hypercrop",
        params: &[ParamDef::int("n", 4.0, 1.0, 100.0), ParamDef::new("rad", 1.0), ParamDef::new("zero", 0.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let n = d[0];
            let coef = n * 0.5 / M_PI;
            let (mut fx, mut fy, mut fz) = (a.x, a.y, a.z);
            let a0 = M_PI / n;
            let len = 1.0 / a0.cos();
            let dd = d[1] * a0.sin() * len;
            let angle = (a.y.atan2(a.x) * coef).floor() / coef + M_PI / n;
            let (x0, y0) = (angle.cos() * len, angle.sin() * len);
            if ((a.x - x0) * (a.x - x0) + (a.y - y0) * (a.y - y0)).sqrt() < dd {
                if d[2] > 1.5 {
                    (fx, fy, fz) = (x0, y0, 0.0);
                } else if d[2] > 0.5 {
                    (fx, fy, fz) = (0.0, 0.0, 0.0);
                } else {
                    let rangle = (a.y - y0).atan2(a.x - x0);
                    (fx, fy, fz) = (x0 + rangle.cos() * dd, y0 + rangle.sin() * dd, 0.0);
                }
            }
            v.x += fx * amount;
            v.y += fy * amount;
            v.z += fz * amount;
        },
        prepare: Some(|p, _| vec![ftoi(p[0]) as f64, p[1], p[2]]),
        wgsl: "let n = D(0);
let coef = n * 0.5 / PI;
var f = vec3<f32>(a.x, a.y, a.z);
let a0 = PI / n;
let len = 1.0 / cos(a0);
let dd = D(1) * sin(a0) * len;
let angle = floor(atan2(a.y, a.x) * coef) / coef + PI / n;
let c0 = vec2<f32>(cos(angle), sin(angle)) * len;
if (length(vec2<f32>(a.x, a.y) - c0) < dd) {
  if (D(2) > 1.5) {
    f = vec3<f32>(c0, 0.0);
  } else if (D(2) > 0.5) {
    f = vec3<f32>(0.0);
  } else {
    let rangle = atan2(a.y - c0.y, a.x - c0.x);
    f = vec3<f32>(c0 + dd * vec2<f32>(cos(rangle), sin(rangle)), 0.0);
  }
}
v.x += f.x * AMOUNT;
v.y += f.y * AMOUNT;
v.z += f.z * AMOUNT;",
    },
    VarDef {
        name: "hypershift",
        params: &[ParamDef::new("shift", 2.0), ParamDef::new("stretch", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (shift, stretch) = (d[0], d[1]);
            let scale = 1.0 - shift * shift;
            let rad = 1.0 / (a.x * a.x + a.y * a.y);
            let (x, y) = (rad * a.x + shift, rad * a.y);
            let rad = amount * scale / (x * x + y * y);
            v.x += rad * x + shift;
            v.y += rad * y * stretch;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let shift = D(0);
let scale = 1.0 - shift * shift;
let rad0 = 1.0 / (a.x * a.x + a.y * a.y);
let x = rad0 * a.x + shift;
let y = rad0 * a.y;
let rad = AMOUNT * scale / (x * x + y * y);
v.x += rad * x + shift;
v.y += rad * y * D(1);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hypershift2",
        params: &[ParamDef::int("p", 3.0, 1.0, 100.0), ParamDef::int("q", 7.0, 1.0, 100.0)],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [p, q, shift, scale2]
        apply: |d, ctx, a, v, amount| {
            let (p, shift, scale2) = (d[0], d[2], d[3]);
            let scale = 1.0 - shift * shift;
            let (fx, fy) = (a.x * scale2, a.y * scale2);
            let rad = 1.0 / (fx * fx + fy * fy);
            let (x, y) = (rad * fx + shift, rad * fy);
            let rad = amount * scale / (x * x + y * y);
            let angle = ((ctx.random_int(i32::MAX as usize) as i64 % p as i64) as f64 * 2.0 + 1.0) * M_PI / p;
            let (xx, yy) = (rad * x + shift, rad * y);
            let (sina, cosa) = angle.sin_cos();
            v.x = cosa * xx - sina * yy;
            v.y = sina * xx + cosa * yy;
            v.z = a.z * rad;
        },
        prepare: Some(|p, _| {
            let (pp, qq) = (ftoi(p[0]) as f64, ftoi(p[1]) as f64);
            let (pq, ppa) = (M_PI / qq, M_PI / pp);
            let (spq, spp) = (pq.sin(), ppa.sin());
            let shift = (M_PI * 0.5 - pq - ppa).sin() / (1.0 - spq * spq - spp * spp).sqrt();
            let s = (M_PI / 2.0 + ppa).sin();
            let scale2 = (1.0 / ((s * s) / (spq * spq) - 1.0).sqrt()) * (s / spq - 1.0);
            vec![pp, qq, shift, scale2]
        }),
        wgsl: "let p = D(0);
let shift = D(2);
let scale = 1.0 - shift * shift;
let f = vec2<f32>(a.x, a.y) * D(3);
let rad0 = 1.0 / dot(f, f);
let x = rad0 * f.x + shift;
let y = rad0 * f.y;
let rad = AMOUNT * scale / (x * x + y * y);
let k = floor(rnd() * p);
let angle = (k * 2.0 + 1.0) * PI / p;
let xx = rad * x + shift;
let yy = rad * y;
v.x = cos(angle) * xx - sin(angle) * yy;
v.y = sin(angle) * xx + cos(angle) * yy;
v.z = a.z * rad;",
    },
    VarDef {
        name: "hypertile3D",
        params: &[ParamDef::int("p", 3.0, 3.0, 100.0), ParamDef::int("q", 7.0, 3.0, 100.0), ParamDef::int("n", 0.0, 0.0, 100.0)],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [p, q, n, c2x, c2y, c2, s2x, s2y, s2z, cx, cy]
        apply: |d, _, a, v, amount| {
            let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
            let (x2cx, y2cy) = (d[3] * a.x, d[4] * a.y);
            let dd = amount / (d[5] * r2 + x2cx - y2cy + 1.0);
            v.x += dd * (a.x * d[6] - d[9] * (y2cy - r2 - 1.0));
            v.y += dd * (a.y * d[7] + d[10] * (-x2cx - r2 - 1.0));
            v.z += dd * (a.z * d[8]);
        },
        prepare: Some(|p, _| {
            let (pp, qq, n) = (ftoi(p[0]).max(3) as f64, ftoi(p[1]).max(3) as f64, ftoi(p[2]).max(0) as f64);
            let (pa, qa) = (2.0 * M_PI / pp, 2.0 * M_PI / qq);
            let mut r = -(pa.cos() - 1.0) / (pa.cos() + qa.cos());
            r = if r > 0.0 { 1.0 / (1.0 + r).sqrt() } else { 1.0 };
            let na = n * pa;
            let (cx, cy) = (r * na.cos(), r * na.sin());
            vec![pp, qq, n, 2.0 * cx, 2.0 * cy, cx * cx + cy * cy, 1.0 + cx * cx - cy * cy, 1.0 + cy * cy - cx * cx, 1.0 - cy * cy - cx * cx, cx, cy]
        }),
        wgsl: "let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
let x2cx = D(3) * a.x;
let y2cy = D(4) * a.y;
let dd = AMOUNT / (D(5) * r2 + x2cx - y2cy + 1.0);
v.x += dd * (a.x * D(6) - D(9) * (y2cy - r2 - 1.0));
v.y += dd * (a.y * D(7) + D(10) * (-x2cx - r2 - 1.0));
v.z += dd * (a.z * D(8));",
    },
    VarDef {
        name: "hypertile3D1",
        params: &[ParamDef::int("p", 3.0, 3.0, 100.0), ParamDef::int("q", 7.0, 3.0, 100.0)],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [p, q, pa, r, c2, s2z]
        apply: |d, ctx, a, v, amount| {
            let ang = ctx.random_int(i32::MAX as usize) as f64 * d[2];
            let (cx, cy) = (d[3] * ang.cos(), d[3] * ang.sin());
            let (s2x, s2y) = (1.0 + cx * cx - cy * cy, 1.0 + cy * cy - cx * cx);
            let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
            let (x2cx, y2cy) = (2.0 * cx * a.x, 2.0 * cy * a.y);
            let dd = amount / (d[4] * r2 + x2cx - y2cy + 1.0);
            v.x += dd * (a.x * s2x - cx * (y2cy - r2 - 1.0));
            v.y += dd * (a.y * s2y + cy * (-x2cx - r2 - 1.0));
            v.z += dd * (a.z * d[5]);
        },
        prepare: Some(|p, _| {
            let (pp, qq) = (ftoi(p[0]).max(3) as f64, ftoi(p[1]).max(3) as f64);
            let (pa, qa) = (2.0 * M_PI / pp, 2.0 * M_PI / qq);
            let mut r = -(pa.cos() - 1.0) / (pa.cos() + qa.cos());
            r = if r > 0.0 { 1.0 / (1.0 + r).sqrt() } else { 1.0 };
            vec![pp, qq, pa, r, r * r, 1.0 - r * r]
        }),
        // GPU: random multiple of pa reduced modulo p (same angles as Java's random(Integer.MAX_VALUE) * pa)
        wgsl: "let ang = floor(rnd() * D(0)) * D(2);
let cx = D(3) * cos(ang);
let cy = D(3) * sin(ang);
let s2x = 1.0 + cx * cx - cy * cy;
let s2y = 1.0 + cy * cy - cx * cx;
let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
let x2cx = 2.0 * cx * a.x;
let y2cy = 2.0 * cy * a.y;
let dd = AMOUNT / (D(4) * r2 + x2cx - y2cy + 1.0);
v.x += dd * (a.x * s2x - cx * (y2cy - r2 - 1.0));
v.y += dd * (a.y * s2y + cy * (-x2cx - r2 - 1.0));
v.z += dd * (a.z * D(5));",
    },
    VarDef {
        name: "hypertile3D2",
        params: &[ParamDef::int("p", 3.0, 3.0, 100.0), ParamDef::int("q", 7.0, 3.0, 100.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| hypertile3d2_apply(d, 2, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let (pp, qq) = (ftoi(p[0]).max(3) as f64, ftoi(p[1]).max(3) as f64);
            let (pa, qa) = (2.0 * M_PI / pp, 2.0 * M_PI / qq);
            let mut r = -(pa.cos() - 1.0) / (pa.cos() + qa.cos());
            r = if r > 0.0 { 1.0 / (1.0 + r).sqrt() } else { 1.0 };
            vec![pp, qq, pa, angle_period(pa), r, r * r, 2.0 * r, 1.0 + r * r, 1.0 - r * r, 1.0 - r * r, 1.0]
        }),
        wgsl: "let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
let x = a.x * D(7) - D(4) * (-r2 - 1.0);
let y = a.y * D(8);
let vr = AMOUNT / (D(5) * r2 + D(6) * a.x + D(10));
var ang = rnd() * TWO_PI;
if (D(3) > 0.0) { ang = floor(rnd() * D(3)) * D(2); }
v.x += vr * (x * cos(ang) + y * sin(ang));
v.y += vr * (y * cos(ang) - x * sin(ang));
v.z += vr * (a.z * D(9));",
    },
    VarDef {
        name: "hypertile3D2b",
        params: &[
            ParamDef::int("p", 3.0, 3.0, 100.0),
            ParamDef::int("q", 7.0, 3.0, 100.0),
            ParamDef::new("b", 2.0),
            ParamDef::new("c", 2.0),
            ParamDef::new("d", 1.0),
            ParamDef::new("e", 1.0),
            ParamDef::new("f", 1.0),
            ParamDef::new("g", 2.0),
            ParamDef::new("h", 1.0),
            ParamDef::new("i", 1.0),
            ParamDef::new("j", 1.0),
            ParamDef::new("k", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| hypertile3d2_apply(d, 12, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = ftoi(p[0]).max(3) as f64;
            d[1] = ftoi(p[1]).max(3) as f64;
            let (b, c, dd, e, f, g, h, i, j, k) = (p[2], p[3], p[4], p[5], p[6], p[7], p[8], p[9], p[10], p[11]);
            let (pa, qa) = (b * M_PI / d[0], c * M_PI / d[1]);
            let mut r = -(pa.cos() - dd) / (pa.cos() + qa.cos());
            r = if r > 0.0 { e / (f + r).sqrt() } else { 1.0 };
            d.extend([pa, angle_period(pa), r, r * r, g * r, i + r * r, j - r * r, k - r * r, h]);
            d
        }),
        wgsl: "let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
let x = a.x * D(17) - D(14) * (-r2 - 1.0);
let y = a.y * D(18);
let vr = AMOUNT / (D(15) * r2 + D(16) * a.x + D(20));
var ang = rnd() * TWO_PI;
if (D(13) > 0.0) { ang = floor(rnd() * D(13)) * D(12); }
v.x += vr * (x * cos(ang) + y * sin(ang));
v.y += vr * (y * cos(ang) - x * sin(ang));
v.z += vr * (a.z * D(19));",
    },
    VarDef {
        name: "hypertile3Db",
        params: &[
            ParamDef::int("p", 3.0, 3.0, 100.0),
            ParamDef::int("q", 7.0, 3.0, 100.0),
            ParamDef::int("n", 0.0, 0.0, 100.0),
            ParamDef::new("b", 2.0),
            ParamDef::new("c", 2.0),
            ParamDef::new("d", 1.0),
            ParamDef::new("e", 1.0),
            ParamDef::new("f", 1.0),
            ParamDef::new("g", 2.0),
            ParamDef::new("h", 2.0),
            ParamDef::new("i", 1.0),
            ParamDef::new("j", 1.0),
            ParamDef::new("k", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [<params>, c2x, c2y, c2, s2x, s2y, s2z, cx, cy] (13..)
        apply: |d, _, a, v, amount| {
            let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
            let (x2cx, y2cy) = (d[13] * a.x, d[14] * a.y);
            let dd = amount / (d[15] * r2 + x2cx - y2cy + 1.0);
            v.x += dd * (a.x * d[16] - d[19] * (y2cy - r2 - 1.0));
            v.y += dd * (a.y * d[17] + d[20] * (-x2cx - r2 - 1.0));
            v.z += dd * (a.z * d[18]);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = ftoi(p[0]).max(3) as f64;
            d[1] = ftoi(p[1]).max(3) as f64;
            d[2] = ftoi(p[2]).max(0) as f64;
            let (b, c, dd, e, f, g, h, i, j, k) = (p[3], p[4], p[5], p[6], p[7], p[8], p[9], p[10], p[11], p[12]);
            let (pa, qa) = (b * M_PI / d[0], c * M_PI / d[1]);
            let mut r = -(pa.cos() - dd) / (pa.cos() + qa.cos());
            r = if r > 0.0 { e / (f + r).sqrt() } else { 1.0 };
            let na = d[2] * pa;
            let (cx, cy) = (r * na.cos(), r * na.sin());
            d.extend([g * cx, h * cy, cx * cx + cy * cy, i + cx * cx - cy * cy, j + cy * cy - cx * cx, k - cy * cy - cx * cx, cx, cy]);
            d
        }),
        wgsl: "let r2 = a.x * a.x + a.y * a.y + a.z * a.z;
let x2cx = D(13) * a.x;
let y2cy = D(14) * a.y;
let dd = AMOUNT / (D(15) * r2 + x2cx - y2cy + 1.0);
v.x += dd * (a.x * D(16) - D(19) * (y2cy - r2 - 1.0));
v.y += dd * (a.y * D(17) + D(20) * (-x2cx - r2 - 1.0));
v.z += dd * (a.z * D(18));",
    },
    VarDef {
        name: "iconattractor_js",
        // Defaults: preset 0 (JWildfire picks a random preset for new instances)
        params: &[
            ParamDef::int("presetId", 0.0, 0.0, 16.0),
            ParamDef::int("degree", 5.0, 3.0, 50.0),
            ParamDef::new("a", 5.0),
            ParamDef::new("b", -1.9),
            ParamDef::new("g", 1.0),
            ParamDef::new("o", 0.188),
            ParamDef::new("l", -2.5),
            ParamDef::new("centerx", 0.0),
            ParamDef::new("centery", 0.0),
            ParamDef::new("scale", 5.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [presetId, degree, a, b, g, o, l, centerx, centery, scale, bdcs]
        apply: |d, _, a, v, amount| {
            let (pa, pb, pg, po, pl) = (d[2], d[3], d[4], d[5], d[6]);
            let zzbar = a.x * a.x + a.y * a.y;
            let mut p = pa * zzbar + pl;
            let (mut zr, mut zi) = (a.x, a.y);
            for _ in 1..=(d[1] as i32 - 2) {
                let za = zr * a.x - zi * a.y;
                let zb = zi * a.x + zr * a.y;
                zr = za;
                zi = zb;
            }
            let zn = a.x * zr - a.y * zi;
            p += pb * zn;
            v.x = (p * a.x + pg * zr - po * a.y) * amount;
            v.y = (p * a.y - pg * zi + po * a.x) * amount;
            v.color = fmod((d[10] * ((v.x + d[7]) * (v.x + d[7]) + (v.y + d[8]) * (v.y + d[8]))).abs(), 1.0);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(p[0] as i32, 0, 16) as f64;
            d[1] = limit_value(p[1] as i32, 3, 50) as f64;
            d.push(1.0 / if p[9] == 0.0 { 10E-6 } else { p[9] });
            d
        }),
        wgsl: "let zzbar = a.x * a.x + a.y * a.y;
var p = D(2) * zzbar + D(6);
var z = vec2<f32>(a.x, a.y);
let n = i32(D(1)) - 2;
for (var i = 1; i <= n; i++) {
  z = vec2<f32>(z.x * a.x - z.y * a.y, z.y * a.x + z.x * a.y);
}
p += D(3) * (a.x * z.x - a.y * z.y);
v.x = (p * a.x + D(4) * z.x - D(5) * a.y) * AMOUNT;
v.y = (p * a.y - D(4) * z.y + D(5) * a.x) * AMOUNT;
v.color = abs(D(10) * ((v.x + D(7)) * (v.x + D(7)) + (v.y + D(8)) * (v.y + D(8)))) % 1.0;",
    },
    simple(
        "idisc",
        false,
        |_, ctx, a, v, amount| {
            let ang = M_PI / ((a.x * a.x + a.y * a.y).sqrt() + 1.0);
            let r = a.y.atan2(a.x) * (amount * M_1_PI);
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        "let ang = PI / (sqrt(a.x * a.x + a.y * a.y) + 1.0);
let r = atan2(a.y, a.x) * (AMOUNT / PI);
v.x += r * cos(ang);
v.y += r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "ikeda",
        params: &[ParamDef::new("u", 0.9)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let t = 0.4 - 6.0 / (1.0 + a.x * a.x + a.y * a.y);
            let (st, ct) = (t.sin(), t.cos());
            v.x += amount * (1.0 + d[0] * (a.x * ct - a.y * st));
            v.y += amount * (d[0] * (a.x * st + a.y * ct));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let t = 0.4 - 6.0 / (1.0 + a.x * a.x + a.y * a.y);
v.x += AMOUNT * (1.0 + D(0) * (a.x * cos(t) - a.y * sin(t)));
v.y += AMOUNT * (D(0) * (a.x * sin(t) + a.y * cos(t)));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "inflate",
        params: &[ParamDef::new("strength", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let nr = r + d[0] * (-r).exp();
            let scale = if r > 0.001 { nr / r } else { 1.0 };
            v.x += amount * a.x * scale;
            v.y += amount * a.y * scale;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
var scale = 1.0;
if (r > 0.001) { scale = (r + D(0) * exp(-r)) / r; }
v.x += AMOUNT * a.x * scale;
v.y += AMOUNT * a.y * scale;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "inflateZ_1",
        true,
        |_, _, a, v, amount| {
            v.z += amount * (a.y.atan2(a.x).sin() - a.y * 2.0);
        },
        "v.z += AMOUNT * (sin(atan2(a.y, a.x)) - a.y * 2.0);",
    ),
    simple(
        "inflateZ_2",
        true,
        |_, _, a, v, amount| {
            v.z += amount * (0.25 - (a.y * 2.0 + a.x * 2.0) * 0.333333);
        },
        "v.z += AMOUNT * (0.25 - (a.y * 2.0 + a.x * 2.0) * 0.333333);",
    ),
    simple(
        "inflateZ_3",
        true,
        |_, _, a, v, amount| {
            let ang = a.y.atan2(a.x);
            v.z += amount * (0.2 * (M_PI - ang) * (3.0 * ang + (a.y - a.x)).cos());
        },
        "let ang = atan2(a.y, a.x);
v.z += AMOUNT * (0.2 * (PI - ang) * cos(3.0 * ang + (a.y - a.x)));",
    ),
    simple(
        "inflateZ_4",
        true,
        |_, ctx, a, v, amount| {
            let mut val = M_PI_2 - a.y.atan2(a.x);
            if ctx.random() < 0.5 {
                val = -val;
            }
            v.z += amount * val * 0.25;
        },
        "var val = HALF_PI - atan2(a.y, a.x);
if (rnd() < 0.5) { val = -val; }
v.z += AMOUNT * val * 0.25;",
    ),
    simple(
        "inflateZ_5",
        true,
        |_, _, a, v, amount| {
            v.z += amount * ((M_PI_2 - a.y.atan2(a.x)).cos() / 2.0);
        },
        "v.z += AMOUNT * (cos(HALF_PI - atan2(a.y, a.x)) / 2.0);",
    ),
    simple(
        "inflateZ_6",
        true,
        |_, _, a, v, amount| {
            let ang = a.y.atan2(a.x);
            let kik = ang * (a.y - a.x).sin();
            v.z += amount * (1.5 - (ang.sin() * kik * 0.5).acos());
        },
        "let ang = atan2(a.y, a.x);
let kik = ang * sin(a.y - a.x);
v.z += AMOUNT * (1.5 - acos(sin(ang) * kik * 0.5));",
    ),
    VarDef {
        name: "intersection",
        params: &[
            ParamDef::new("xwidth", 5.0),
            ParamDef::new("xtilesize", 0.5),
            ParamDef::new("xmod1", 0.3),
            ParamDef::new("xmod2", 1.0),
            ParamDef::new("xheight", 0.5),
            ParamDef::new("yheight", 5.0),
            ParamDef::new("ytilesize", 0.5),
            ParamDef::new("ymod1", 0.3),
            ParamDef::new("ymod2", 1.0),
            ParamDef::new("ywidth", 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [<params>, xr1, yr1]
        apply: |d, ctx, a, v, amount| {
            // fold of the coordinate `c` with mod1 `m` and period `r`
            let fold = |c: f64, m: f64, r: f64| if c > m { -m + fmod(c + m, r) } else if c < -m { m - fmod(m - c, r) } else { c };
            if ctx.random() < 0.5 {
                let x = if ctx.random() < 0.5 { d[0] } else { -d[0] };
                v.x += d[1] * (a.x + java_round(x * ctx.random().ln()));
                v.y += d[4] * fold(a.y, d[2], d[10]);
            } else {
                let y = if ctx.random() < 0.5 { d[5] } else { -d[5] };
                v.y += d[6] * (a.y + java_round(y * ctx.random().ln()));
                v.x += d[9] * fold(a.x, d[7], d[11]);
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d.extend([p[3] * p[2], p[8] * p[7]]);
            d
        }),
        wgsl: "fn intersection_fold(c: f32, m: f32, r: f32) -> f32 {
  if (c > m) { return -m + (c + m) % r; }
  if (c < -m) { return m - (m - c) % r; }
  return c;
}
@body
if (rnd() < 0.5) {
  var x = -D(0);
  if (rnd() < 0.5) { x = D(0); }
  v.x += D(1) * (a.x + java_round(x * log(rnd())));
  v.y += D(4) * intersection_fold(a.y, D(2), D(10));
} else {
  var y = -D(5);
  if (rnd() < 0.5) { y = D(5); }
  v.y += D(6) * (a.y + java_round(y * log(rnd())));
  v.x += D(9) * intersection_fold(a.x, D(7), D(11));
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "inversion",
        params: &[
            ParamDef::new("scale", 1.0),
            ParamDef::new("rotation", 0.0),
            ParamDef::int("shape", 0.0, 0.0, 6.0),
            ParamDef::int("imode", 0.0, 0.0, 4.0),
            ParamDef::int("hide_uninverted", 0.0, 0.0, 1.0),
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 1.0),
            ParamDef::new("c", 0.0),
            ParamDef::new("d", 0.0),
            ParamDef::new("e", 0.0),
            ParamDef::new("f", 0.0),
            ParamDef::int("ring_mode", 0.0, 0.0, 2.0),
            ParamDef::new("ring_scale", 1.0),
            ParamDef::new("pnorm_point", 2.0),
            ParamDef::new("pnorm_shape", 2.0),
            ParamDef::new("pnorm_pmod", 1.0),
            ParamDef::new("pnorm_smod", 1.0),
            ParamDef::new("draw_shape", 0.0),
            ParamDef::new("shape_thickness", 0.0),
            ParamDef::new("passthrough", 0.0),
            ParamDef::int("guides_enabled", 1.0, 0.0, 1.0),
            ParamDef::int("color_measure", 0.0, 0.0, 4.0),
            ParamDef::int("color_gradient", 1.0, 0.0, 2.0),
            ParamDef::new("color_low_threshold", 0.0),
            ParamDef::new("color_high_threshold", 1.0)
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| inversion_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for i in [2, 3, 11, 21, 22] {
                d[i] = p[i].trunc();
            }
            let (scale, ring_scale) = (p[0], p[12]);
            let (rmin, rmax) = if ring_scale <= scale {
                let rmin = ring_scale * scale;
                (rmin, scale * scale / rmin)
            } else {
                let rmax = ring_scale * scale;
                (scale * scale / rmax, rmax)
            };
            let period = if d[2] as i32 == 4 { inversion_rhodonea_period(p[5], p[6], p[7]) } else { M_2PI };
            d.extend([M_PI * p[1], rmin, rmax, period]);
            d
        }),
        wgsl: "fn inversion_curve(base: u32, tin: f32) -> vec3<f32> {
  let scale = prm[base];
  let a = prm[base + 5u];
  let b = prm[base + 6u];
  let c = prm[base + 7u];
  let t = tin - prm[base + 25u];
  var r = 1.0;
  switch (i32(prm[base + 2u])) {
    case 1: { r = (a * b) / sqrt((b * cos(t)) * (b * cos(t)) + (a * sin(t)) * (a * sin(t))); }
    case 2: { r = sqrt((a * a * b * b) / ((b * b * cos(t) * cos(t)) - (a * a * sin(t) * sin(t)))); }
    case 3: {
      let theta = abs(t % TWO_PI);
      let n = floor(a);
      r = cos(PI / n) / cos(theta % (TWO_PI / n) - PI / n);
    }
    case 4: { r = cos(a / b * t) + c; }
    case 5: { r = (a * b) / java_pow(java_pow(abs(a * sin(t)), c) + java_pow(abs(b * cos(t)), c), 1.0 / c); }
    case 6: {
      r = java_pow(java_pow(abs(cos(t * c / 4.0) / a), prm[base + 9u]) + java_pow(abs(sin(t * c / 4.0) / b), prm[base + 10u]), -1.0 / prm[base + 8u]);
    }
    default: {}
  }
  r *= scale;
  return vec3<f32>(r, r * cos(tin), r * sin(tin));
}
@body
let xin = a.x;
let yin = a.y;
var done = false;
if (D(17) > 0.0 && rnd() < D(17)) {
  let cp = inversion_curve(base, rnd() * D(28));
  v.x += cp.y;
  v.y += cp.z;
  if (D(18) != 0.0) {
    v.x += 0.01 * (rnd() - 0.5) * D(18);
    v.y += 0.01 * (rnd() - 0.5) * D(18);
  }
  done = true;
}
if (!done && D(19) > 0.0 && rnd() < D(19)) {
  v.x += xin;
  v.y += yin;
  done = true;
}
if (!done) {
  let tin = atan2(yin, xin);
  let rin = sqrt(xin * xin + yin * yin);
  var cp = vec3<f32>(-0.0001, 0.0, 0.0);
  if (i32(D(2)) == 4) {
    let tc = i32(D(28) / TWO_PI);
    for (var i = 0; i < tc; i++) {
      let q = inversion_curve(base, tin + f32(i) * PI);
      if (abs(q.x) > cp.x) { cp = vec3<f32>(abs(q.x), q.y, q.z); }
    }
  } else {
    cp = inversion_curve(base, tin);
  }
  var invert = true;
  let imode = i32(D(3));
  if (imode == 1) { invert = rin > cp.x; } else if (imode == 2) { invert = rin < cp.x; }
  let ring_mode = i32(D(11));
  if (invert && ring_mode != 0 && D(12) != 1.0) {
    if (ring_mode == 1) { invert = rin >= D(26) && rin <= D(27); }
    else if (ring_mode == 2) { invert = rin <= D(26) || rin >= D(27); }
  }
  if (!invert) {
    v.x += xin;
    v.y += yin;
    if (PRESERVE_Z) { v.z += a.z; }
    v.hide = select(0.0, 1.0, D(4) == 1.0);
  } else {
    let ps = D(14);
    let pp = D(13);
    var num = cp.x * cp.x;
    if (!(ps == 2.0 && D(16) == 1.0)) { num = java_pow(java_pow(abs(cp.y), ps) + java_pow(abs(cp.z), ps), 2.0 / (ps * D(16))); }
    var den = xin * xin + yin * yin;
    if (!(pp == 2.0 && D(15) == 1.0)) { den = java_pow(java_pow(abs(xin), pp) + java_pow(abs(yin), pp), 2.0 / (pp * D(15))); }
    let iscale = num / den;
    v.x += AMOUNT * iscale * xin;
    v.y += AMOUNT * iscale * yin;
    if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
    v.hide = 0.0;
    let measure = i32(D(21));
    if (measure >= 1 && measure <= 4) {
      var val = 0.0;
      if (measure == 1) { val = length(vec2<f32>(v.x, v.y) - cp.yz); }
      else if (measure == 2) { val = length(vec2<f32>(a.x, a.y) - cp.yz); }
      else if (measure == 3) { val = length(vec2<f32>(v.x - a.x, v.y - a.y)); }
      else {
        let is = (D(0) * D(0)) / (a.x * a.x + a.y * a.y);
        val = length(vec2<f32>(v.x, v.y)) / length(vec2<f32>(a.x, a.y) * is * AMOUNT);
      }
      let lo = min(D(23), D(24));
      let hi = max(D(23), D(24));
      var bc = 0.0;
      if (i32(D(22)) == 2) {
        var w = val;
        if (w < lo) { w = hi - ((lo - w) % (hi - lo)); } else if (w > hi) { w = lo + ((w - lo) % (hi - lo)); }
        bc = (w - lo) / (hi - lo) * 255.0;
      } else if (val >= hi) {
        bc = 255.0;
      } else if (val >= lo) {
        bc = (val - lo) / (hi - lo) * 255.0;
      }
      v.rgbc = 0.0;
      v.color = clamp(bc / 255.0, 0.0, 1.0);
    }
  }
}",
    },
    VarDef {
        name: "inverted_julia",
        params: &[
            ParamDef::new("power", 0.25),
            ParamDef::new("y2_mult", 1.0),
            ParamDef::new("a2x_mult", 1.0),
            ParamDef::new("a2y_mult", 1.0),
            ParamDef::new("a2y_add", 0.0),
            ParamDef::new("cos_mult", 0.0),
            ParamDef::new("y_mult", 1.0),
            ParamDef::new("center", 3.14),
            ParamDef::new("x2y2_add", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let z = (a.x * a.x + (a.y * a.y * d[1])).powf(d[0]) + d[8];
            let q = (a.x * d[2]).atan2(a.y * d[3] + d[4]) * 0.5 + M_PI * ((2.0 * ctx.random()) as i32) as f64;
            let cz = (z * d[5]).cos();
            v.x += amount * cz * (q.sin() / z / d[7]);
            v.y += amount * cz * (q.cos() / z / d[7]) * d[6];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let z = java_pow(a.x * a.x + a.y * a.y * D(1), D(0)) + D(8);
let q = atan2(a.x * D(2), a.y * D(3) + D(4)) * 0.5 + PI * floor(2.0 * rnd());
let cz = cos(z * D(5));
v.x += AMOUNT * cz * (sin(q) / z / D(7));
v.y += AMOUNT * cz * (cos(q) / z / D(7)) * D(6);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "involute",
        params: &[ParamDef::new("r", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let t = a.y.atan2(a.x);
            v.x += amount * d[0] * (t.cos() + t * t.sin());
            v.y += amount * d[0] * (t.sin() - t * t.cos());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let t = atan2(a.y, a.x);
v.x += AMOUNT * D(0) * (cos(t) + t * sin(t));
v.y += AMOUNT * D(0) * (sin(t) - t * cos(t));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "invpolar",
        false,
        |_, ctx, a, v, amount| {
            let ny = 1.0 + a.y;
            v.x += amount * ny * (a.x * M_PI).sin();
            v.y += amount * ny * (a.x * M_PI).cos();
            ctx.pass_z(a, v, amount);
        },
        "let ny = 1.0 + a.y;
v.x += AMOUNT * ny * sin(a.x * PI);
v.y += AMOUNT * ny * cos(a.x * PI);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "invsquircular",
        false,
        |_, ctx, a, v, amount| {
            if amount != 0.0 {
                let (u, w) = (a.x, a.y);
                let r = u * u + w * w;
                let r2 = (r * (amount * amount * r - 4.0 * u * u * w * w) / amount).sqrt();
                let r = (r - r2).sqrt() / M_SQRT2;
                v.x += r / u;
                v.y += r / w;
            }
            ctx.pass_z(a, v, amount);
        },
        "if (AMOUNT != 0.0) {
  let r = a.x * a.x + a.y * a.y;
  let r2 = sqrt(r * (AMOUNT * AMOUNT * r - 4.0 * a.x * a.x * a.y * a.y) / AMOUNT);
  let rr = sqrt(r - r2) / sqrt(2.0);
  v.x += rr / a.x;
  v.y += rr / a.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "invtree_js",
        false,
        |_, ctx, a, v, amount| {
            let (x, y) = if ctx.random() < 0.333 {
                (a.x / 2.0, a.y / 2.0)
            } else if ctx.random() < 0.666 {
                (1.0 / (a.x + 1.0), a.y / (a.y + 1.0))
            } else {
                (a.x / (a.x + 1.0), 1.0 / (a.y + 1.0))
            };
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        "var q = vec2<f32>(a.x / (a.x + 1.0), 1.0 / (a.y + 1.0));
if (rnd() < 0.333) {
  q = vec2<f32>(a.x / 2.0, a.y / 2.0);
} else if (rnd() < 0.666) {
  q = vec2<f32>(1.0 / (a.x + 1.0), a.y / (a.y + 1.0));
}
v.x += q.x * AMOUNT;
v.y += q.y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "isobar",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("amp", 0.3), ParamDef::new("spin", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y + 1.0e-10).sqrt();
            let theta = a.y.atan2(a.x);
            let band = (d[0] * r).sin() * d[1];
            let (nr, nt) = (r + band, theta + d[2] * band);
            v.x += amount * nr * nt.cos();
            v.y += amount * nr * nt.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y + 1.0e-10);
let theta = atan2(a.y, a.x);
let band = sin(D(0) * r) * D(1);
let nt = theta + D(2) * band;
v.x += AMOUNT * (r + band) * cos(nt);
v.y += AMOUNT * (r + band) * sin(nt);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];
