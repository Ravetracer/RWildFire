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
use super::complex::Complex;
use super::glsl::{mat2, vec2};
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

// `hehl.gl.Complex` operations of `JacAsnFunc` on `(re, im)` pairs.
type Hc = (f64, f64);

fn hc_mul(a: Hc, b: Hc) -> Hc {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

fn hc_add(a: Hc, b: Hc) -> Hc {
    (a.0 + b.0, a.1 + b.1)
}

fn hc_sub(a: Hc, b: Hc) -> Hc {
    (a.0 - b.0, a.1 - b.1)
}

fn hc_scale(a: Hc, s: f64) -> Hc {
    (s * a.0, s * a.1)
}

fn hc_mag(a: Hc) -> f64 {
    (a.0 * a.0 + a.1 * a.1).sqrt()
}

fn hc_sqrt(z: Hc) -> Hc {
    let (r, phi) = (hc_mag(z).sqrt(), z.1.atan2(z.0) / 2.0);
    (r * phi.cos(), r * phi.sin())
}

fn hc_ln(z: Hc) -> Hc {
    (hc_mag(z).ln() / std::f64::consts::E.ln(), z.1.atan2(z.0))
}

fn hc_sin(z: Hc) -> Hc {
    (z.0.sin() * z.1.cosh(), z.0.cos() * z.1.sinh())
}

fn hc_tan(z: Hc) -> Hc {
    let den = (2.0 * z.0).cos() + (2.0 * z.1).cosh();
    ((2.0 * z.0).sin() / den, (2.0 * z.1).sinh() / den)
}

/// `j * z`
fn hc_j(z: Hc) -> Hc {
    (-z.1, z.0)
}

fn hc_asin(z: Hc) -> Hc {
    let zz = hc_add(hc_sqrt(hc_sub((1.0, 0.0), hc_mul(z, z))), hc_j(z));
    hc_scale(hc_j(hc_ln(zz)), -1.0)
}

fn hc_acos(z: Hc) -> Hc {
    let zz = hc_add(z, hc_sqrt(hc_sub(hc_mul(z, z), (1.0, 0.0))));
    hc_scale(hc_j(hc_ln(zz)), -1.0)
}

fn hc_asinh(zz: Hc) -> Hc {
    hc_ln(hc_add(zz, hc_sqrt(hc_add((1.0, 0.0), hc_mul(zz, zz)))))
}

/// `JacElkFunc.RF`: Carlson's elliptic integral of the first kind.
fn jac_elk_rf(mut x: f64, mut y: f64, mut z: f64) -> f64 {
    let (mut a, mut dx, mut dy, mut dz);
    let mut itc = 0;
    loop {
        let lamda = (x * y).sqrt() + (y * z).sqrt() + (z * x).sqrt();
        x = (x + lamda) / 4.0;
        y = (y + lamda) / 4.0;
        z = (z + lamda) / 4.0;
        a = (x + y + z) / 3.0;
        dx = 1.0 - x / a;
        dy = 1.0 - y / a;
        dz = 1.0 - z / a;
        itc += 1;
        if itc >= 25 || java_max(java_max(dx.abs(), dy.abs()), dz.abs()) <= 1e-5 {
            break;
        }
    }
    let e2 = dx * dy + dy * dz + dz * dx;
    let e3 = dy * dx * dz;
    let r = 1.0 - (1.0 / 10.0) * e2 + (1.0 / 14.0) * e3 + (1.0 / 24.0) * (e2 * e2) - (3.0 / 44.0) * e2 * e3 - (5.0 / 208.0) * (e2 * e2 * e2)
        + (3.0 / 104.0) * (e3 * e3)
        + (1.0 / 16.0) * (e2 * e2) * e3;
    r * (1.0 / a.sqrt())
}

/// `Math.signum`
fn jsignum(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// GPU stand-in for `fmod(random(Integer.MAX_VALUE), re)`: the period in which the values repeat
/// (smallest integer multiple of `re`), or 0 if there is none below 10000.
fn juliac_period(re: f64) -> f64 {
    (1..=10000)
        .find(|&n| {
            let t = n as f64 / re;
            t != 0.0 && (t - t.round()).abs() < 1e-9
        })
        .map_or(0.0, |n| n as f64)
}

/// `JuliaScopePlusFunc.transformFunction`; d = [power, dist, cx, cy, mixPowerSign, colorMode,
/// colorSpeed, thetaSignMode, angularOffset, swirlFactor, fixedBranch, radialOffset,
/// altPowerScale, altPowerRotate, rndTermMode, mandelbrotLike, branches].
fn juliascope_plus_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let power = d[0] as i32;
    let nb = d[16] as i32;
    let sign = if power == 0 { 1 } else { power.signum() };
    let (eff, alt, flipped);
    if power != 0 && d[4] >= 0.5 {
        if ctx.random_int(2) == 0 {
            eff = nb * sign;
            alt = false;
        } else {
            eff = -nb * sign;
            alt = true;
        }
        flipped = eff < 0;
    } else {
        eff = power;
        alt = false;
        flipped = eff < 0;
    }
    let (add_x, add_y) = if d[15] >= 1.0 { (a.x, a.y) } else { (d[2], d[3]) };
    if eff == 0 {
        v.x += amount * (a.x + add_x);
        v.y += amount * (a.y + add_y);
        ctx.pass_z(a, v, amount);
        return;
    }
    let cpower = d[1] / eff as f64 * 0.5;
    let fixed = java_round(d[10]) as i32;
    let num = if nb > 0 { nb } else { 1 };
    let rnd = if fixed >= 0 && fixed < num { fixed } else { ctx.random_int(num as usize) as i32 };
    let theta = a.y.atan2(a.x);
    let tsign = match d[7] as i32 {
        0 => if rnd & 1 != 0 { -1.0 } else { 1.0 },
        2 => -1.0,
        3 => if ctx.random_int(2) == 0 { 1.0 } else { -1.0 },
        _ => 1.0,
    };
    let rsign = match d[14] as i32 {
        1 => -1.0,
        2 => if rnd & 1 != 0 { -1.0 } else { 1.0 },
        3 => if ctx.random_int(2) == 0 { 1.0 } else { -1.0 },
        _ => 1.0,
    };
    let mut ang = (rsign * 2.0 * M_PI * rnd as f64 + tsign * theta) / eff as f64;
    if d[9] != 0.0 {
        let r0 = (a.x * a.x + a.y * a.y).sqrt();
        if r0 > 0.00001 {
            ang += d[9] * (1.0 + r0).ln();
        }
    }
    ang += d[8];
    let rmag = java_max(0.0, a.x * a.x + a.y * a.y).powf(cpower) + d[11];
    let (mut dx, mut dy) = (rmag * ang.cos(), rmag * ang.sin());
    if alt {
        dx *= d[12];
        dy *= d[12];
        if d[13] != 0.0 {
            let (sr, cr) = d[13].sin_cos();
            (dx, dy) = (dx * cr - dy * sr, dx * sr + dy * cr);
        }
    }
    v.x += amount * (dx + add_x);
    v.y += amount * (dy + add_y);
    let cm = d[5] as i32;
    if cm != 0 {
        let val = match cm {
            1 => ang / (2.0 * M_PI),
            2 => (1.00001 + java_max(0.0, rmag)).ln(),
            3 => (theta + M_PI) / (2.0 * M_PI),
            4 => rnd as f64 / num as f64,
            _ => {
                if d[4] >= 0.5 && power != 0 {
                    if flipped { 0.75 } else { 0.25 }
                } else if power >= 0 {
                    0.25
                } else {
                    0.75
                }
            }
        };
        v.color = frac(val * d[6]);
    }
    ctx.pass_z(a, v, amount);
}

/// `KIFS3DFunc`; d = [max_iter, bailout_radius, kifs_scale_x, kifs_scale_y, kifs_scale_z,
/// scale_pivot, center_x/y/z, offset_x/y/z, mirror_fold, fold_type, fold_plane1_nx/ny/nz,
/// fold_plane1_dist, fold_plane1_intensity, edge_x/y/z, rot_x/y/z, rot_order, rot_pivot,
/// transform_order, post_symmetry, color_mode, color_scale, bailout_sq, rot_x/y/z (radians),
/// fold_norm1_len_sq] (31..).
fn kifs3d_fold(d: &[f64], ctx: &mut Ctx, p: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = p;
    let (n, dist, inten, len_sq) = ([d[14], d[15], d[16]], d[17], d[18], d[35]);
    let edge = [d[19], d[20], d[21]];
    let plane = |p: [f64; 3]| {
        let dot = p[0] * n[0] + p[1] * n[1] + p[2] * n[2] - dist;
        let sf = (1.0 + inten) * dot / len_sq;
        [p[0] - sf * n[0], p[1] - sf * n[1], p[2] - sf * n[2]]
    };
    if d[12] <= 0.0 {
        match d[13] as i32 {
            1 => [x.abs(), y.abs(), z.abs()],
            2 => if x + y < 0.0 { [-y, -x, z] } else { p },
            3 => {
                if len_sq > 1e-9 && x * n[0] + y * n[1] + z * n[2] - dist < 0.0 {
                    plane(p)
                } else {
                    p
                }
            }
            4 => [0, 1, 2].map(|k| if edge[k] != 0.0 { edge[k] - (edge[k] - p[k]).abs() } else { p[k] }),
            _ => p,
        }
    } else {
        let sg = [0; 3].map(|_| if ctx.random() < 0.5 { -1.0 } else { 1.0 });
        match d[13] as i32 {
            1 => [x * sg[0], y * sg[1], z * sg[2]],
            2 => if sg[0] < 0.0 { [-y, -x, z] } else { p },
            3 => if len_sq > 1e-9 && sg[0] < 0.0 { plane(p) } else { p },
            4 => [0, 1, 2].map(|k| if edge[k] != 0.0 { edge[k] - sg[k] * (edge[k] - p[k]).abs() } else { p[k] }),
            _ => p,
        }
    }
}

fn kifs3d_rotate(d: &[f64], p: [f64; 3], pivot: [f64; 3]) -> [f64; 3] {
    let (ax, ay, az) = (d[32], d[33], d[34]);
    if ax == 0.0 && ay == 0.0 && az == 0.0 {
        return p;
    }
    let (mut rx, mut ry, mut rz) = (p[0] - pivot[0], p[1] - pivot[1], p[2] - pivot[2]);
    let (sx, cx) = ax.sin_cos();
    let (sy, cy) = ay.sin_cos();
    let (sz, cz) = az.sin_cos();
    if d[25] as i32 == 0 {
        (rx, ry) = (rx * cz - ry * sz, rx * sz + ry * cz);
        (rx, rz) = (rx * cy + rz * sy, -rx * sy + rz * cy);
        (ry, rz) = (ry * cx - rz * sx, ry * sx + rz * cx);
    } else {
        (ry, rz) = (ry * cx - rz * sx, ry * sx + rz * cx);
        (rx, rz) = (rx * cy + rz * sy, -rx * sy + rz * cy);
        (rx, ry) = (rx * cz - ry * sz, rx * sz + ry * cz);
    }
    [rx + pivot[0], ry + pivot[1], rz + pivot[2]]
}

fn kifs3d_scale(d: &[f64], p: [f64; 3]) -> [f64; 3] {
    let offset = [d[9], d[10], d[11]];
    let pivot_is_offset = d[5] as i32 == 1;
    let pivot = if pivot_is_offset { offset } else { [d[6], d[7], d[8]] };
    let mut s = [0, 1, 2].map(|k| d[2 + k] * (p[k] - pivot[k]) + pivot[k]);
    if !pivot_is_offset {
        for k in 0..3 {
            s[k] += offset[k];
        }
    }
    s
}

fn kifs3d_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let mut p = [a.x, a.y, a.z];
    let max_iter = d[0] as i32;
    let mut iter_color = 1.0;
    let pivot = match d[26] as i32 {
        1 => [d[6], d[7], d[8]],
        2 => [d[9], d[10], d[11]],
        _ => [0.0; 3],
    };
    for i in 0..max_iter {
        p = match d[27] as i32 {
            0 => {
                let f = kifs3d_fold(d, ctx, p);
                kifs3d_scale(d, kifs3d_rotate(d, f, pivot))
            }
            1 => {
                let r = kifs3d_rotate(d, p, pivot);
                kifs3d_scale(d, kifs3d_fold(d, ctx, r))
            }
            _ => {
                let sc = kifs3d_scale(d, p);
                let f = kifs3d_fold(d, ctx, sc);
                kifs3d_rotate(d, f, pivot)
            }
        };
        if p[0] * p[0] + p[1] * p[1] + p[2] * p[2] > d[31] {
            iter_color = i as f64 / max_iter as f64;
            break;
        }
    }
    let sym = d[28] as i32;
    if sym > 0 {
        let sg = [0; 3].map(|_| if ctx.random() < 0.5 { -1.0 } else { 1.0 });
        for k in 0..3 {
            if sym & (1 << k) != 0 {
                p[k] *= sg[k];
            }
        }
    }
    let cs = d[30];
    let fl = |t: f64| t - t.floor();
    let color = match d[29] as i32 {
        1 => fl((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt() * cs),
        2 => fl(((p[1].atan2(p[0]) + M_PI) / (2.0 * M_PI)) * cs),
        3 => fl(p[0] * cs),
        4 => fl(p[1] * cs),
        5 => fl(p[2] * cs),
        _ => iter_color,
    };
    v.x += p[0] * amount;
    v.y += p[1] * amount;
    v.z += p[2] * amount;
    v.color = color;
}

/// `org.nfunk.jep.type.Complex` (JEP 2.4.1) as used by `KleinGroupFunc`.
#[derive(Clone, Copy)]
struct Jc(f64, f64);

impl Jc {
    fn mul(self, b: Jc) -> Jc {
        Jc(self.0 * b.0 - self.1 * b.1, self.1 * b.0 + self.0 * b.1)
    }
    fn muls(self, s: f64) -> Jc {
        Jc(self.0 * s, self.1 * s)
    }
    fn add(self, b: Jc) -> Jc {
        Jc(self.0 + b.0, self.1 + b.1)
    }
    fn sub(self, b: Jc) -> Jc {
        Jc(self.0 - b.0, self.1 - b.1)
    }
    fn div(self, b: Jc) -> Jc {
        if b.0.abs() >= b.1.abs() {
            let r = b.1 / b.0;
            let den = b.0 + r * b.1;
            Jc((self.0 + r * self.1) / den, (self.1 - r * self.0) / den)
        } else {
            let r = b.0 / b.1;
            let den = b.1 + r * b.0;
            Jc((self.0 * r + self.1) / den, (self.1 * r - self.0) / den)
        }
    }
    fn abs(self) -> f64 {
        let (ar, ai) = (self.0.abs(), self.1.abs());
        if ar == 0.0 && ai == 0.0 {
            0.0
        } else if ar > ai {
            let t = ai / ar;
            ar * (1.0 + t * t).sqrt()
        } else {
            let t = ar / ai;
            ai * (1.0 + t * t).sqrt()
        }
    }
    fn power(self, e: f64) -> Jc {
        let scalar = self.abs().powf(e);
        let factor = if self.1 == 0.0 && self.0 < 0.0 {
            2
        } else if self.0 == 0.0 && self.1 > 0.0 {
            1
        } else if self.0 == 0.0 && self.1 < 0.0 {
            -1
        } else {
            0
        };
        let fe = factor as f64 * e;
        if factor != 0 && fe == (fe as i32) as f64 {
            let mut x = (fe as i32) % 4;
            if x < 0 {
                x += 4;
            }
            const SIN: [f64; 4] = [0.0, 1.0, 0.0, -1.0];
            const COS: [f64; 4] = [1.0, 0.0, -1.0, 0.0];
            return Jc(scalar * COS[x as usize], scalar * SIN[x as usize]);
        }
        let t = e * self.1.atan2(self.0);
        Jc(scalar * t.cos(), scalar * t.sin())
    }
    fn sqrt(self) -> Jc {
        if self.0 == 0.0 && self.1 == 0.0 {
            return Jc(0.0, 0.0);
        }
        let (ar, ai) = (self.0.abs(), self.1.abs());
        let mut w = if ar >= ai {
            let t = ai / ar;
            ar.sqrt() * (0.5 * (1.0 + (1.0 + t * t).sqrt())).sqrt()
        } else {
            let t = ar / ai;
            ai.sqrt() * (0.5 * (t + (1.0 + t * t).sqrt())).sqrt()
        };
        if self.0 >= 0.0 {
            Jc(w, self.1 / (2.0 * w))
        } else {
            if self.1 < 0.0 {
                w = -w;
            }
            Jc(self.1 / (2.0 * w), w)
        }
    }
}

/// `KleinGroupFunc.init`: the generators `a`, `b` for the recipe, as 2x2 complex matrices.
fn klein_group_generators(a_re: f64, a_im: f64, b_re: f64, b_im: f64, recipe: i32) -> [[Jc; 4]; 2] {
    let (zero, re1, re2, re4, im1, im2, im4) = (Jc(0.0, 0.0), Jc(1.0, 0.0), Jc(2.0, 0.0), Jc(4.0, 0.0), Jc(0.0, 1.0), Jc(0.0, 2.0), Jc(0.0, 4.0));
    let trace_ab = |ta: Jc, tb: Jc| {
        let b = ta.mul(tb).muls(-1.0);
        let c = ta.power(2.0).add(tb.power(2.0));
        let bsq = b.power(2.0);
        let ac4 = c.muls(4.0);
        b.muls(-1.0).sub(bsq.sub(ac4).sqrt()).div(re2)
    };
    let maskit_a = |mu: Jc| [mu.muls(-1.0).mul(im1), im1.muls(-1.0), im1.muls(-1.0), zero];
    match recipe {
        1 => [maskit_a(Jc(a_re, a_im)), [re1, re2, zero, re1]],
        2 => {
            let (ta, tb) = (Jc(a_re, a_im), Jc(b_re, b_im));
            let tab = trace_ab(ta, tb);
            [
                [ta.sub(tb.div(tab)), ta.div(tab.power(2.0)), ta, tb.div(tab)],
                [tb.sub(ta.div(tab)), tb.muls(-1.0).div(tab.power(2.0)), tb.muls(-1.0), ta.div(tab)],
            ]
        }
        3 => [[re1, zero, Jc(a_re, a_im), re1], [re1, re2, zero, re1]],
        4 => [[re1, zero, Jc(a_re, a_im), re1], [re1, Jc(b_re, b_im), zero, re1]],
        5 => [maskit_a(Jc(a_re, a_im)), [re1, Jc(b_re, b_im), zero, re1]],
        6 => [maskit_a(Jc(a_re, a_im)), [re1, Jc(2.0 * (M_PI / b_re).cos(), b_im), zero, re1]],
        _ => {
            let (ta, tb) = (Jc(a_re, a_im), Jc(b_re, b_im));
            let tab = trace_ab(ta, tb);
            let z0 = tab.sub(re2).mul(tb).div(tb.mul(tab).sub(ta.mul(re2)).add(tab.mul(im2)));
            let a1 = ta.mul(tab).sub(tb.mul(re2)).add(im4).div(tab.mul(re2).add(re4).mul(z0));
            let a2 = ta.mul(tab).sub(tb.mul(re2)).sub(im4).mul(z0).div(tab.mul(re2).sub(re4));
            [[ta.div(re2), a1, a2, ta.div(re2)], [tb.sub(im2).div(re2), tb.div(re2), tb.div(re2), tb.add(im2).div(re2)]]
        }
    }
}

/// Turtle polylines of `HilbertFunc` / `KochFunc`: d = [level, show_lines, line_thickness,
/// show_points, point_thickness, line_threshold, point_threshold, line_thickness / 100,
/// point_thickness / 100, segments, x0, y0, x1, y1, ...].
fn turtle_polyline_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
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
}

const TURTLE_POLYLINE_WGSL: &str = "let nseg = u32(D(9));
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
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

/// `KochFunc.koch` polyline in the layout of [`turtle_polyline_apply`].
fn koch_prepare(p: &[f64]) -> Vec<f64> {
    let level = limit_value(ftoi(p[0]), 1, 7);
    fn koch(n: i32, angle: &mut f64, pos: &mut (f64, f64), pts: &mut Vec<f64>) {
        if n == 0 {
            pos.0 += 0.5 * angle.to_radians().cos();
            pos.1 += 0.5 * angle.to_radians().sin();
            pts.extend([pos.0, pos.1]);
            return;
        }
        koch(n - 1, angle, pos, pts);
        *angle += 60.0;
        koch(n - 1, angle, pos, pts);
        *angle -= 120.0;
        koch(n - 1, angle, pos, pts);
        *angle += 60.0;
        koch(n - 1, angle, pos, pts);
    }
    let mut pts = vec![0.0, 0.0];
    koch(level, &mut 0.0, &mut (0.0, 0.0), &mut pts);
    let (show_lines, show_points) = (p[1].trunc(), p[3].trunc());
    let sum = show_lines + show_points;
    let lf = show_lines / sum;
    let mut d = vec![level as f64, show_lines, p[2], show_points, p[4], lf, lf + show_points / sum, p[2] / 100.0, p[4] / 100.0];
    d.push((pts.len() / 2 - 1) as f64);
    d.extend_from_slice(&pts);
    d
}

/// `java.awt.Color.HSBtoRGB` (float arithmetic).
fn java_hsb_to_rgb(hue: f32, saturation: f32, brightness: f32) -> [i32; 3] {
    let c = |v: f32| (v * 255.0 + 0.5) as i32;
    if saturation == 0.0 {
        let v = c(brightness);
        return [v, v, v];
    }
    let h = (hue - hue.floor()) * 6.0;
    let f = h - h.floor();
    let p = brightness * (1.0 - saturation);
    let q = brightness * (1.0 - saturation * f);
    let t = brightness * (1.0 - (saturation * (1.0 - f)));
    match h as i32 {
        0 => [c(brightness), c(t), c(p)],
        1 => [c(q), c(brightness), c(p)],
        2 => [c(p), c(brightness), c(t)],
        3 => [c(p), c(q), c(brightness)],
        4 => [c(t), c(p), c(brightness)],
        5 => [c(brightness), c(p), c(q)],
        _ => [0, 0, 0],
    }
}

/// Hue of `java.awt.Color.RGBtoHSB` (float arithmetic).
fn java_rgb_hue(rgb: [i32; 3]) -> f32 {
    let [r, g, b] = rgb;
    let cmax = r.max(g).max(b);
    let cmin = r.min(g).min(b);
    let saturation = if cmax != 0 { (cmax - cmin) as f32 / cmax as f32 } else { 0.0 };
    if saturation == 0.0 {
        return 0.0;
    }
    let span = (cmax - cmin) as f32;
    let (rc, gc, bc) = ((cmax - r) as f32 / span, (cmax - g) as f32 / span, (cmax - b) as f32 / span);
    let mut hue = if r == cmax {
        bc - gc
    } else if g == cmax {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    hue /= 6.0;
    if hue < 0.0 {
        hue += 1.0;
    }
    hue
}

/// `js.mandala.RotatorMap` skew tables.
struct RotatorSkews {
    num: i32,
    denom: i32,
    minsky: bool,
    wobble: f64,
    wrap: f64,
    extra: i32,
    size: i32,
    base: i32,
    min: i32,
    max: i32,
    h1: Vec<i32>,
    v: Vec<i32>,
    h2: Vec<i32>,
}

impl RotatorSkews {
    /// `adjust_skew_maps`
    fn adjust(&mut self, x: i32) -> i32 {
        if self.wrap == 0.0 {
            while self.base + x < 0 || self.base + x >= self.size {
                self.size *= 2;
                self.base = self.size / 2;
                self.h1.clear();
            }
        }
        if self.h1.is_empty() {
            self.min = -self.base;
            self.max = self.min + self.size - 1;
            let n = self.size as usize;
            (self.h1, self.v, self.h2) = (vec![0; n], vec![0; n], vec![0; n]);
            let low = self.num as f64 / ((1.0 + self.wobble) * self.denom as f64);
            let hi = self.num as f64 / ((1.0 - self.wobble) * self.denom as f64);
            let mut i = -self.base;
            while i + self.base < self.size {
                let x = i as f64 * 2.0 * M_PI * (1.0 / 512.0);
                let angle = M_PI * 2.0 * (low - (x.cos() - 1.0) / 2.0 * (hi - low));
                let k = (i + self.base) as usize;
                if self.minsky {
                    let eps = (2.0 * (1.0 - angle.cos())).sqrt();
                    self.h1[k] = (-(java_round(eps * i as f64 + 0.00000001) as i64)) as i32 + self.extra;
                    self.v[k] = java_round(eps * i as f64 + 0.00000001) as i64 as i32;
                    self.h2[k] = 0;
                } else {
                    let sa = angle.sin();
                    let c = (angle.cos() - 1.0) / sa;
                    self.h1[k] = java_round(c * i as f64) as i64 as i32;
                    self.v[k] = java_round(sa * i as f64 + 0.00000001) as i64 as i32;
                    self.h2[k] = self.h1[k] + self.extra;
                }
                i += 1;
            }
        }
        x - self.size * ((self.base + x) as f64 / self.size as f64).floor() as i32
    }

    /// One rotation (`denom` repetitions of the three skews).
    fn rotate(&mut self, px: &mut i32, py: &mut i32) {
        for _ in 0..self.denom {
            *px += self.h1[(*py + self.base) as usize];
            if *px < self.min || *px > self.max {
                *px = self.adjust(*px);
            }
            *py += self.v[(*px + self.base) as usize];
            if *py < self.min || *py > self.max {
                *py = self.adjust(*py);
            }
            *px += self.h2[(*py + self.base) as usize];
            if *px < self.min || *px > self.max {
                *px = self.adjust(*px);
            }
        }
    }
}

const ROTATOR_MAX_STEPS: i32 = 20000;

/// `RotatorMap.drawMap` (`mandala2`: `Mandala2Func.drawMap`, which follows wrapped paths until
/// they cycle). Returns the step counts per cell (`x * width + y`) and the cells in drawing order.
#[allow(clippy::too_many_arguments)]
fn rotator_map(width: i32, num: i32, denom: i32, minsky: bool, wobble: f64, wrap: f64, extra: i32, mandala2: bool) -> (Vec<i32>, Vec<(i32, i32)>) {
    let show_base = -width / 2;
    let mut size = if wrap == 0.0 { width * 2 } else { (width as f64 * wrap) as i32 };
    size |= 1;
    let mut base = size / 2;
    if base + (show_base + width) > size {
        base = -(show_base + width) + size;
    }
    let mut sk = RotatorSkews { num, denom, minsky, wobble, wrap, extra, size, base, min: 0, max: 0, h1: Vec::new(), v: Vec::new(), h2: Vec::new() };
    sk.adjust(0);
    let offs = -show_base;
    let (min_show, max_show) = (show_base, show_base + width - 1);
    let mut counts = vec![0i32; (width * width) as usize];
    let mut order = Vec::new();
    let idx = |x: i32, y: i32| ((x + offs) * width + (y + offs)) as usize;
    for x in min_show..=max_show {
        for y in min_show..=max_show {
            if counts[idx(x, y)] != 0 {
                continue;
            }
            let (mut px, mut py) = (x, y);
            let mut n_steps = 1;
            // Mandala2Func also follows wrapped paths beyond max_n_steps (they cycle); bounded here
            let mut guard = 0u64;
            while n_steps <= ROTATOR_MAX_STEPS || (mandala2 && wrap > 0.0 && guard < 50_000_000) {
                guard += 1;
                sk.rotate(&mut px, &mut py);
                if px == x && py == y {
                    break;
                }
                if mandala2 && n_steps > ROTATOR_MAX_STEPS {
                    n_steps = ROTATOR_MAX_STEPS;
                }
                n_steps += 1;
            }
            let (mut px, mut py) = (x, y);
            for _ in 1..=n_steps {
                if px >= min_show && px <= max_show && py >= min_show && py <= max_show {
                    if counts[idx(px, py)] != 0 {
                        break;
                    }
                    order.push((px + offs, py + offs));
                    counts[idx(px, py)] = n_steps;
                }
                sk.rotate(&mut px, &mut py);
                if px == x && py == y {
                    break;
                }
            }
        }
    }
    (counts, order)
}

/// Rotator map parameters as set when loading a flame: `num` is adjusted against the default
/// `denom` (3) and `denom` against the adjusted `num`; returns (num, denom, wobble, wrap, extra).
fn rotator_params(num: f64, denom: f64, wobble: f64, wrap: f64, hskew: f64) -> (i32, i32, f64, f64, i32) {
    const WRAP: [f64; 9] = [0.0, 12.0, 8.0, 6.0, 4.0, 3.0, 2.0, 1.5, 1.0];
    const SKEW: [i32; 23] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 15, 16, 23, 24, 25, 31, 32, 36, 47, 48, 63, 64];
    let mut n = if num < 1.0 { 1.0 } else { num };
    if (n * 2.0) % 3.0 == 0.0 {
        n += 1.0;
    }
    let n = limit_value(n, 1.0, 15.0) as i32;
    let mut dn = if denom < 3.0 { 3.0 } else { denom };
    if (n as f64 * 2.0) % dn == 0.0 {
        dn += 1.0;
    }
    let dn = limit_value(dn, 3.0, 16.0) as i32;
    let pick = |v: f64, len: usize| (limit_value(v, 0.0, len as f64) as i32 % len as i32) as usize;
    let wp = pick(wobble, 9);
    let wobble = if wp == 0 { 0.0 } else { 10f64.powf(-6.0 + 0.5 * wp as f64) };
    (n, dn, wobble, WRAP[pick(wrap, 9)], SKEW[pick(hskew, 23)])
}

/// Packs an RGB color into one value.
fn pack_rgb(c: [i32; 3]) -> f64 {
    ((c[0] << 16) | (c[1] << 8) | c[2]) as f64
}

/// Cached prepared data of the mandala variations (building the map can take a while).
fn mandala_cached(key: Vec<u64>, build: impl FnOnce() -> Vec<f64>) -> std::sync::Arc<Vec<f64>> {
    use std::sync::{Arc, Mutex, OnceLock};
    type Entry = (Vec<u64>, Arc<Vec<f64>>);
    static CACHE: OnceLock<Mutex<Vec<Entry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, d)) = cache.lock().unwrap().iter().find(|(k, _)| *k == key) {
        return d.clone();
    }
    let d = Arc::new(build());
    let mut c = cache.lock().unwrap();
    if c.len() >= 8 {
        c.remove(0);
    }
    c.push((key, d.clone()));
    d
}

/// `MandalaFunc`: d = [width, size, num, denom, minsky, wobble, wrap_range, hskew, color id,
/// count, (x, y, color) per cell in drawing order]; the color is packed RGB (color id 0) or a
/// gradient index.
fn mandala_prepare(p: &[f64]) -> Vec<f64> {
    let width = limit_value(p[0], 100.0, 1000.0) as i32;
    let (num, denom, wobble, wrap, extra) = rotator_params(p[2], p[3], p[5], p[6], p[7]);
    let minsky = limit_value(p[4], 0.0, 1.0) as i32 == 1;
    let colorid = limit_value(p[8], 0.0, 1.0) as i32;
    let key = vec![width as u64, num as u64, denom as u64, minsky as u64, wobble.to_bits(), wrap.to_bits(), extra as u64, colorid as u64];
    let cells = mandala_cached(key, || {
        let (counts, order) = rotator_map(width, num, denom, minsky, wobble, wrap, extra, false);
        let mut d = Vec::with_capacity(order.len() * 3);
        for (x, y) in order {
            let n = counts[(x * width + y) as usize];
            let rgb = if n == ROTATOR_MAX_STEPS + 1 {
                [0, 0, 0]
            } else {
                let z = (n as f64).ln() as f32;
                java_hsb_to_rgb(z - z.floor(), 0.85, 1.0)
            };
            d.extend([x as f64, y as f64, if colorid == 0 { pack_rgb(rgb) } else { java_rgb_hue(rgb) as f64 }]);
        }
        d
    });
    let mut d = vec![width as f64, limit_value(p[1], 0.0, 1.0), num as f64, denom as f64, minsky as i32 as f64, wobble, wrap, extra as f64, colorid as f64];
    d.push((cells.len() / 3) as f64);
    d.extend_from_slice(&cells);
    d
}

/// `Mandala2Func`: d = [width, num, denom, minsky, wobble, wrap_range, hskew, color id,
/// color per cell (x * width + y)]; packed RGB for color ids 1..3, else a gradient index.
fn mandala2_prepare(p: &[f64]) -> Vec<f64> {
    let width = limit_value(p[0], 100.0, 2000.0) as i32;
    let (num, denom, wobble, wrap, extra) = rotator_params(p[1], p[2], p[4], p[5], p[6]);
    let minsky = limit_value(p[3], 0.0, 1.0) as i32 == 1;
    let colorid = limit_value(p[7], 0.0, 3.0) as i32;
    let key = vec![width as u64, num as u64, denom as u64, minsky as u64, wobble.to_bits(), wrap.to_bits(), extra as u64, colorid as u64, 2];
    let cells = mandala_cached(key, || {
        let (counts, _) = rotator_map(width, num, denom, minsky, wobble, wrap, extra, true);
        counts
            .iter()
            .map(|&n| {
                let rgb = if n == ROTATOR_MAX_STEPS + 1 {
                    [0, 0, 0]
                } else {
                    match colorid {
                        1 => {
                            let z = (n as f64).ln() as f32;
                            java_hsb_to_rgb(z - z.floor(), 0.85, 1.0)
                        }
                        2 => {
                            let z = (n as f64 / 6.333333) as f32;
                            java_hsb_to_rgb(z - z.floor(), 0.85, 1.0)
                        }
                        _ => {
                            let c = |m: i32, dv: f32| ((n % m) as f32 / dv * 255.0 + 0.5) as i32;
                            [c(91, 90.0), c(123, 122.0), c(17, 16.0)]
                        }
                    }
                };
                if (1..=3).contains(&colorid) { pack_rgb(rgb) } else { java_rgb_hue(rgb) as f64 }
            })
            .collect()
    });
    let mut d = vec![width as f64, num as f64, denom as f64, minsky as i32 as f64, wobble, wrap, extra as f64, colorid as f64];
    d.extend_from_slice(&cells);
    d
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
        // GPU: the segment cycling is kept per shader invocation.
        wgsl: "var<private> hexaplay3D_st: vec4<f32>;
fn hexaplay3D_step() -> vec3<f32> {
  // (mode60, fcycle, bcycle) before the step; hexaplay3D_st = (rswtch, fcycle, bcycle, initialized)
  if (hexaplay3D_st.w == 0.0) { hexaplay3D_st = vec4<f32>(trunc(rnd() * 3.0), 0.0, 0.0, 1.0); }
  if (hexaplay3D_st.y > 5.0) { hexaplay3D_st.y = 0.0; hexaplay3D_st.x = trunc(rnd() * 3.0); }
  if (hexaplay3D_st.z > 2.0) { hexaplay3D_st.z = 0.0; hexaplay3D_st.x = trunc(rnd() * 3.0); }
  let r = vec3<f32>(select(0.0, 1.0, hexaplay3D_st.x <= 1.0), hexaplay3D_st.y, hexaplay3D_st.z);
  if (r.x > 0.0) { hexaplay3D_st.y += 1.0; } else { hexaplay3D_st.z += 1.0; }
  return r;
}
@body
let cyc = hexaplay3D_step();
let mode60 = cyc.x > 0.0;
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
  let ang = cyc.y * PI / 3.0;
  sg = vec2<f32>(cos(ang), sin(ang));
} else {
  if (cyc.z == 1.0) { sg = vec2<f32>(-0.5, h); } else if (cyc.z == 2.0) { sg = vec2<f32>(-0.5, -h); }
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
        // GPU: the mode cycling is kept per shader invocation.
        wgsl: "var<private> hexnix3D_st: vec4<f32>;
fn hexnix3D_step() -> vec3<f32> {
  // (mode60, fcycle, bcycle) before the step; hexnix3D_st = (rswtch, fcycle, bcycle, initialized)
  if (hexnix3D_st.w == 0.0) { hexnix3D_st = vec4<f32>(trunc(rnd() * 3.0), 0.0, 0.0, 1.0); }
  if (hexnix3D_st.y > 5.0) { hexnix3D_st.y = 0.0; hexnix3D_st.x = trunc(rnd() * 3.0); }
  if (hexnix3D_st.z > 2.0) { hexnix3D_st.z = 0.0; hexnix3D_st.x = trunc(rnd() * 3.0); }
  let r = vec3<f32>(select(0.0, 1.0, hexnix3D_st.x <= 1.0), hexnix3D_st.y, hexnix3D_st.z);
  if (r.x > 0.0) { hexnix3D_st.y += 1.0; } else { hexnix3D_st.z += 1.0; }
  return r;
}
@body
let mode60 = hexnix3D_step().x > 0.0;
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
        apply: |d, ctx, a, v, amount| turtle_polyline_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| hilbert_prepare(p)),
        wgsl: TURTLE_POLYLINE_WGSL,
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
        // GPU: the orbit is kept per shader invocation; it starts at a slightly perturbed start
        // point after a random number of warm-up steps.
        wgsl: "var<private> hopalong_st: vec3<f32>;
fn hopalong_step(p: vec2<f32>, base: u32) -> vec2<f32> {
  return vec2<f32>(p.y - sign(p.x) * sqrt(abs(prm[base + 2u] * p.x - prm[base + 3u])), prm[base + 1u] - p.x);
}
@body
if (hopalong_st.z == 0.0) {
  var q = vec2<f32>(D(4), D(5)) + 1.0e-2 * (vec2<f32>(rnd(), rnd()) - 0.5);
  let n = min(i32(rnd() * 65536.0), 65535);
  for (var i = 0; i < n; i++) { q = hopalong_step(q, base); }
  hopalong_st = vec3<f32>(q, 1.0);
}
let q = hopalong_step(hopalong_st.xy, base);
hopalong_st = vec3<f32>(q, 1.0);
v.x += AMOUNT * q.x;
v.y += AMOUNT * q.y;",
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
    VarDef {
        name: "jac_asn",
        params: &[ParamDef::new("kr", 0.5), ParamDef::new("ki", 0.0), ParamDef::int("type", 1.0, 0.0, 7.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let mut geo1: Hc = (0.0, 0.0);
            let mut phi: Hc = (a.x, a.y);
            let mut modul: Hc = (d[0], d[1]);
            let t1: Hc = (1.0, 0.0);
            let kind = d[2] as i32;
            phi = match kind {
                0 | 4 => hc_scale(hc_add(phi, hc_asin(hc_mul(modul, hc_sin(phi)))), 0.5),
                1 | 5 => hc_scale(hc_add(hc_asin(phi), hc_asin(hc_mul(modul, phi))), 0.5),
                2 | 6 => hc_scale(hc_add(hc_acos(phi), hc_asin(hc_mul(modul, hc_sqrt(hc_sub(t1, hc_mul(phi, phi)))))), 0.5),
                _ => {
                    let p = hc_j(hc_asinh(phi));
                    hc_scale(hc_add(p, hc_asin(hc_mul(modul, hc_sin(p)))), 0.5)
                }
            };
            if kind > 3 {
                geo1 = phi;
                phi = modul;
                modul = geo1;
            }
            let mut geo: Hc = (amount, 0.0);
            for jj in 0..10 {
                let dm = hc_sub(t1, modul);
                if dm.0 * dm.0 + dm.1 * dm.1 < SMALL_EPSILON {
                    break;
                }
                if jj > 0 {
                    phi = hc_scale(hc_add(phi, hc_asin(hc_mul(modul, hc_sin(phi)))), 0.5);
                }
                // divideBy: unchanged geo1 for a (numerically) zero denominator
                let den = hc_add(t1, modul);
                let rz = hc_mag(den);
                if rz.abs() >= 1.0e-12 {
                    geo1 = ((2.0 * den.0) / (rz * rz), (-2.0 * den.1) / (rz * rz));
                }
                geo = hc_mul(geo, geo1);
                modul = hc_mul(geo1, hc_sqrt(modul));
            }
            let r = hc_mul(geo, hc_ln(hc_tan(hc_add(hc_scale(phi, 0.5), (M_PI_4, 0.0)))));
            v.x += r.0;
            v.y += r.1;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], limit_value(p[2], 0.0, 7.0).trunc()]),
        wgsl: "fn jac_asn_mul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> { return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x); }
fn jac_asn_sqrt(z: vec2<f32>) -> vec2<f32> {
  let r = sqrt(length(z));
  let phi = atan2(z.y, z.x) / 2.0;
  return r * vec2<f32>(cos(phi), sin(phi));
}
fn jac_asn_ln(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(log(length(z)), atan2(z.y, z.x)); }
fn jac_asn_sin(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(sin(z.x) * cosh(z.y), cos(z.x) * sinh(z.y)); }
fn jac_asn_j(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(-z.y, z.x); }
fn jac_asn_asin(z: vec2<f32>) -> vec2<f32> {
  let zz = jac_asn_sqrt(vec2<f32>(1.0, 0.0) - jac_asn_mul(z, z)) + jac_asn_j(z);
  return -jac_asn_j(jac_asn_ln(zz));
}
fn jac_asn_acos(z: vec2<f32>) -> vec2<f32> {
  let zz = z + jac_asn_sqrt(jac_asn_mul(z, z) - vec2<f32>(1.0, 0.0));
  return -jac_asn_j(jac_asn_ln(zz));
}
@body
var geo1 = vec2<f32>(0.0);
var phi = vec2<f32>(a.x, a.y);
var modul = vec2<f32>(D(0), D(1));
let t1 = vec2<f32>(1.0, 0.0);
let kind = i32(D(2));
switch (kind) {
  case 0, 4: { phi = 0.5 * (phi + jac_asn_asin(jac_asn_mul(modul, jac_asn_sin(phi)))); }
  case 1, 5: { phi = 0.5 * (jac_asn_asin(phi) + jac_asn_asin(jac_asn_mul(modul, phi))); }
  case 2, 6: { phi = 0.5 * (jac_asn_acos(phi) + jac_asn_asin(jac_asn_mul(modul, jac_asn_sqrt(t1 - jac_asn_mul(phi, phi))))); }
  default: {
    let p = jac_asn_j(jac_asn_ln(phi + jac_asn_sqrt(t1 + jac_asn_mul(phi, phi))));
    phi = 0.5 * (p + jac_asn_asin(jac_asn_mul(modul, jac_asn_sin(p))));
  }
}
if (kind > 3) {
  geo1 = phi;
  phi = modul;
  modul = geo1;
}
var geo = vec2<f32>(AMOUNT, 0.0);
for (var jj = 0; jj < 10; jj++) {
  let dm = t1 - modul;
  if (dot(dm, dm) < 1e-30) { break; }
  if (jj > 0) { phi = 0.5 * (phi + jac_asn_asin(jac_asn_mul(modul, jac_asn_sin(phi)))); }
  let den = t1 + modul;
  let rz = length(den);
  if (rz >= 1.0e-12) { geo1 = vec2<f32>(2.0 * den.x, -2.0 * den.y) / (rz * rz); }
  geo = jac_asn_mul(geo, geo1);
  modul = jac_asn_mul(geo1, jac_asn_sqrt(modul));
}
let hp = 0.5 * phi + vec2<f32>(PI / 4.0, 0.0);
let tden = cos(2.0 * hp.x) + cosh(2.0 * hp.y);
let r = jac_asn_mul(geo, jac_asn_ln(vec2<f32>(sin(2.0 * hp.x), sinh(2.0 * hp.y)) / tden));
v.x += r.x;
v.y += r.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "jac_cn",
        params: &[ParamDef::range("k", 0.5, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let k = d[0];
            let (xs, xc, xd) = jacobi_elliptic(a.x, k);
            let (ys, yc, yd) = jacobi_elliptic(a.y, 1.0 - k);
            let (num_x, num_y) = (xc * yc, -xd * xs * yd * ys);
            let den = amount / (SMALL_EPSILON + (xs * xs) * (ys * ys) * k + (yc * yc));
            v.x += den * num_x;
            v.y += den * num_y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![limit_value(p[0], -1.0, 1.0)]),
        wgsl: "let k = D(0);
let jx = math_jacobi_elliptic(a.x, k);
let jy = math_jacobi_elliptic(a.y, 1.0 - k);
let den = AMOUNT / (1e-30 + jx.x * jx.x * jy.x * jy.x * k + jy.y * jy.y);
v.x += den * (jx.y * jy.y);
v.y += den * (-jx.z * jx.x * jy.z * jy.x);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "jac_dn",
        params: &[ParamDef::range("k", 0.5, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let k = d[0];
            let (xs, xc, xd) = jacobi_elliptic(a.x, k);
            let (ys, yc, yd) = jacobi_elliptic(a.y, 1.0 - k);
            let (num_x, num_y) = (xd * yc * yd, -xc * xs * ys * k);
            let den = amount / (SMALL_EPSILON + (xs * xs) * (ys * ys) * k + (yc * yc));
            v.x += den * num_x;
            v.y += den * num_y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![limit_value(p[0], -1.0, 1.0)]),
        wgsl: "let k = D(0);
let jx = math_jacobi_elliptic(a.x, k);
let jy = math_jacobi_elliptic(a.y, 1.0 - k);
let den = AMOUNT / (1e-30 + jx.x * jx.x * jy.x * jy.x * k + jy.y * jy.y);
v.x += den * (jx.z * jy.y * jy.z);
v.y += den * (-jx.y * jx.x * jy.x * k);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "jac_elk",
        params: &[ParamDef::range("k", 0.5, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            const EPS: f64 = 1.0e-20;
            let nonz = |nz: f64| if nz.abs() <= EPS { EPS * jsignum(nz) } else { nz };
            let k = d[0];
            let (phi, psi) = (a.x, a.y);
            let cotphi = phi.cos() / nonz(phi.sin());
            let cotphi2 = cotphi * cotphi;
            let sp = psi.sinh() / (EPS + phi.sin());
            let b = -(cotphi2 + k * (sp * sp) - 1.0 + k) / 2.0;
            let c = -(1.0 - k) * cotphi2;
            let c = (b * b - c).abs().sqrt();
            let x1 = java_max(-b + c, -b - c);
            let mu = jsignum(psi) * ((x1 / nonz(cotphi2) - 1.0) / nonz(k)).abs().sqrt();
            let lambda = jsignum(phi) * x1.abs().sqrt();
            let sina = jsignum(lambda) / (lambda * lambda + 1.0).sqrt();
            let cosa = lambda * sina;
            let e_phi = sina * jac_elk_rf(cosa * cosa, 1.0 - k * (sina * sina), 1.0);
            let cosa = 1.0 / (mu * mu + 1.0).sqrt();
            let sina = mu * cosa;
            let e_psi = sina * jac_elk_rf(cosa * cosa, 1.0 - (1.0 - k) * (sina * sina), 1.0);
            v.x += amount * e_phi;
            v.y += amount * e_psi;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![limit_value(p[0], -1.0, 1.0)]),
        wgsl: "fn jac_elk_nonz(nz: f32) -> f32 {
  if (abs(nz) <= 1.0e-20) { return 1.0e-20 * sign(nz); }
  return nz;
}
fn jac_elk_rf(x_in: f32, y_in: f32, z_in: f32) -> f32 {
  var x = x_in;
  var y = y_in;
  var z = z_in;
  var a = 0.0;
  var dx = 0.0;
  var dy = 0.0;
  var dz = 0.0;
  for (var it = 0; it < 25; it++) {
    let lamda = sqrt(x * y) + sqrt(y * z) + sqrt(z * x);
    x = (x + lamda) / 4.0;
    y = (y + lamda) / 4.0;
    z = (z + lamda) / 4.0;
    a = (x + y + z) / 3.0;
    dx = 1.0 - x / a;
    dy = 1.0 - y / a;
    dz = 1.0 - z / a;
    if (max(max(abs(dx), abs(dy)), abs(dz)) <= 1e-5) { break; }
  }
  let e2 = dx * dy + dy * dz + dz * dx;
  let e3 = dy * dx * dz;
  let r = 1.0 - e2 / 10.0 + e3 / 14.0 + e2 * e2 / 24.0 - 3.0 / 44.0 * e2 * e3 - 5.0 / 208.0 * e2 * e2 * e2 + 3.0 / 104.0 * e3 * e3 + e2 * e2 * e3 / 16.0;
  return r / sqrt(a);
}
@body
let k = D(0);
let phi = a.x;
let psi = a.y;
let cotphi = cos(phi) / jac_elk_nonz(sin(phi));
let cotphi2 = cotphi * cotphi;
let sp = sinh(psi) / (1.0e-20 + sin(phi));
let b = -(cotphi2 + k * sp * sp - 1.0 + k) / 2.0;
let c = sqrt(abs(b * b + (1.0 - k) * cotphi2));
let x1 = max(-b + c, -b - c);
let mu = sign(psi) * sqrt(abs((x1 / jac_elk_nonz(cotphi2) - 1.0) / jac_elk_nonz(k)));
let lambda = sign(phi) * sqrt(abs(x1));
var sina = sign(lambda) / sqrt(lambda * lambda + 1.0);
var cosa = lambda * sina;
let e_phi = sina * jac_elk_rf(cosa * cosa, 1.0 - k * sina * sina, 1.0);
cosa = 1.0 / sqrt(mu * mu + 1.0);
sina = mu * cosa;
let e_psi = sina * jac_elk_rf(cosa * cosa, 1.0 - (1.0 - k) * sina * sina, 1.0);
v.x += AMOUNT * e_phi;
v.y += AMOUNT * e_psi;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "jac_sn",
        params: &[ParamDef::range("k", 0.5, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let k = d[0];
            let (xs, xc, xd) = jacobi_elliptic(a.x, k);
            let (ys, yc, yd) = jacobi_elliptic(a.y, 1.0 - k);
            let (num_x, num_y) = (xs * yd, xc * xd * yc * ys);
            let den = amount / (SMALL_EPSILON + (xs * xs) * (ys * ys) * k + (yc * yc));
            v.x += den * num_x;
            v.y += den * num_y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![limit_value(p[0], -1.0, 1.0)]),
        wgsl: "let k = D(0);
let jx = math_jacobi_elliptic(a.x, k);
let jy = math_jacobi_elliptic(a.y, 1.0 - k);
let den = AMOUNT / (1e-30 + jx.x * jx.x * jy.x * jy.x * k + jy.y * jy.y);
v.x += den * (jx.x * jy.z);
v.y += den * (jx.y * jx.z * jy.y * jy.x);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "japanese_maple_leaf",
        params: &[ParamDef::new("filled", 0.15)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let t = a.atan_xy() * 0.5 + M_PI_2;
            let scale = 0.667;
            let r = if d[0] > 0.0 && d[0] > ctx.random() { amount * ctx.random() * scale } else { amount * scale };
            let p = f64::powf;
            let (c8, s8, ct) = ((8.0 * t).cos(), (8.0 * t).sin(), t.cos());
            let lx = (4.0 / 15.0 + p(c8, 2.0) + 1.0 / 5.0 * p(c8 * (24.0 * t).cos(), 10.0) + (1.0 / 6.0 * p((80.0 * t).cos(), 2.0) + 1.0 / 15.0 * p((80.0 * t).cos(), 10.0)))
                * (1.0 - p(s8, 10.0))
                * (2.0 * t).sin()
                * (1.0 + t.sin())
                * (1.0 - p(ct, 10.0) * p((3.0 * t).cos(), 2.0) + 1.0 / 30.0 * p(ct, 9.0) * p((5.0 * t).cos(), 10.0));
            let ly = (8.0 / 30.0
                + p(c8, 2.0)
                + 1.0 / 5.0 * p(c8 * (24.0 * t).cos(), 10.0)
                + (1.0 / 6.0 * p((80.0 * t).cos(), 2.0) + 1.0 / 15.0 * p((80.0 * t).cos(), 10.0)) * (1.0 - p(ct, 10.0) * p((3.0 * t).cos(), 2.0)) * (1.0 - p(s8, 10.0)))
                * (2.0 * t).cos()
                * (1.0 + t.sin() + p(ct * (3.0 * t).cos(), 10.0));
            v.x += r * lx;
            v.y += r * ly;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let t = atan_xy(a) * 0.5 + HALF_PI;
var r = AMOUNT * 0.667;
if (D(0) > 0.0 && D(0) > rnd()) { r = AMOUNT * rnd() * 0.667; }
let c8 = cos(8.0 * t);
let s8 = sin(8.0 * t);
let ct = cos(t);
let c80 = cos(80.0 * t);
let lx = (4.0 / 15.0 + java_pow(c8, 2.0) + 1.0 / 5.0 * java_pow(c8 * cos(24.0 * t), 10.0) + (1.0 / 6.0 * java_pow(c80, 2.0) + 1.0 / 15.0 * java_pow(c80, 10.0)))
  * (1.0 - java_pow(s8, 10.0)) * sin(2.0 * t) * (1.0 + sin(t))
  * (1.0 - java_pow(ct, 10.0) * java_pow(cos(3.0 * t), 2.0) + 1.0 / 30.0 * java_pow(ct, 9.0) * java_pow(cos(5.0 * t), 10.0));
let ly = (8.0 / 30.0 + java_pow(c8, 2.0) + 1.0 / 5.0 * java_pow(c8 * cos(24.0 * t), 10.0)
  + (1.0 / 6.0 * java_pow(c80, 2.0) + 1.0 / 15.0 * java_pow(c80, 10.0)) * (1.0 - java_pow(ct, 10.0) * java_pow(cos(3.0 * t), 2.0)) * (1.0 - java_pow(s8, 10.0)))
  * cos(2.0 * t) * (1.0 + sin(t) + java_pow(ct * cos(3.0 * t), 10.0));
v.x += r * lx;
v.y += r * ly;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "jet_stream",
        params: &[ParamDef::new("speed", 1.0), ParamDef::new("width", 0.3), ParamDef::new("center", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let w = java_max(d[1].abs(), 0.01);
            let dy = a.y - d[2];
            v.x += amount * (a.x + d[0] * (-(dy * dy) / (w * w)).exp());
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let w = max(abs(D(1)), 0.01);
let dy = a.y - D(2);
v.x += AMOUNT * (a.x + D(0) * exp(-(dy * dy) / (w * w)));
v.y += AMOUNT * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "joukowski",
        params: &[ParamDef::new("P1", 2.5), ParamDef::new("P2", 0.0), ParamDef::int("inverse", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (p1, z0) = (d[0], (-d[1], 0.2));
            let mul = |a: Hc, b: Hc| hc_mul(a, b);
            let z = if d[2] as i32 == 0 {
                let w = (a.x + z0.0, a.y + z0.1);
                // cdiv((p1^2, 0), w)
                let dd = w.0 * w.0 + w.1 * w.1;
                let q = ((p1 * p1 * w.0) / dd, (-(p1 * p1) * w.1) / dd);
                mul(((-0.0f64).cos(), (-0.0f64).sin()), (w.0 + q.0, w.1 + q.1))
            } else {
                let z = mul((1.0, 0.0), (a.x, a.y));
                let zz = mul(z, z);
                let arg = (zz.0 / 4.0 - p1 * p1, zz.1 / 4.0);
                // csqrt
                let m = (arg.0 * arg.0 + arg.1 * arg.1).sqrt();
                let sg = if arg.1 > 0.0 { 1.0 } else if arg.1 < 0.0 { -1.0 } else { 0.0 };
                let sq = (((m + arg.0) * 0.5).sqrt(), ((m - arg.0) * 0.5).sqrt() * sg);
                let r1 = (z.0 / 2.0 + sq.0 - z0.0, z.1 / 2.0 + sq.1 - z0.1);
                let r2 = (z.0 / 2.0 - sq.0 - z0.0, z.1 / 2.0 - sq.1 - z0.1);
                if hc_mag(r1) > hc_mag(r2) { r1 } else { r2 }
            };
            v.x += amount * z.0;
            v.y += amount * z.1;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], limit_value(p[2], 0.0, 1.0).trunc()]),
        wgsl: "let p1 = D(0);
let z0 = vec2<f32>(-D(1), 0.2);
var z = vec2<f32>(0.0);
if (i32(D(2)) == 0) {
  let w = vec2<f32>(a.x, a.y) + z0;
  z = w + vec2<f32>(p1 * p1 * w.x, -(p1 * p1) * w.y) / dot(w, w);
} else {
  let q = vec2<f32>(a.x, a.y);
  let arg = vec2<f32>((q.x * q.x - q.y * q.y) / 4.0 - p1 * p1, (2.0 * q.x * q.y) / 4.0);
  let m = length(arg);
  let sq = vec2<f32>(sqrt((m + arg.x) * 0.5), sqrt((m - arg.x) * 0.5) * sign(arg.y));
  let r1 = q / 2.0 + sq - z0;
  let r2 = q / 2.0 - sq - z0;
  z = select(r2, r1, length(r1) > length(r2));
}
v.x += AMOUNT * z.x;
v.y += AMOUNT * z.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "jubiQ",
        // Default power: JWildfire picks a random one (+-2..7) for new instances
        params: &[
            ParamDef::int("power", 3.0, -10.0, 10.0),
            ParamDef::new("dist", 1.0),
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 0.0),
            ParamDef::new("c", 0.0),
            ParamDef::new("d", 1.0),
            ParamDef::new("e", 0.0),
            ParamDef::new("f", 0.0),
            ParamDef::new("qat", 0.0),
            ParamDef::new("qax", 0.0),
            ParamDef::new("qay", 0.0),
            ParamDef::new("qaz", 0.0),
            ParamDef::new("qbt", 0.0),
            ParamDef::new("qbx", 0.0),
            ParamDef::new("qby", 0.0),
            ParamDef::new("qbz", 0.0),
            ParamDef::new("qct", 0.0),
            ParamDef::new("qcx", 0.0),
            ParamDef::new("qcy", 0.0),
            ParamDef::new("qcz", 0.0),
            ParamDef::new("qdt", 1.0),
            ParamDef::new("qdx", 0.0),
            ParamDef::new("qdy", 0.0),
            ParamDef::new("qdz", 0.0),
            ParamDef::int("zmode", 0.0, 0.0, 2.0)
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [power, dist, a..f, qat..qdz, zmode, abs_n, cn, abs_power, cpower] (27..)
        apply: |d, ctx, a, v, amount| {
            let q = &d[8..24];
            let (t2, x2, y2) = (a.x, a.y, a.z);
            let nt = q[0] * t2 - q[1] * x2 - q[2] * y2 + q[4];
            let nx = q[0] * x2 + q[1] * t2 - q[3] * y2 + q[5];
            let ny = q[0] * y2 + q[2] * t2 + q[3] * x2 + q[6];
            let nz = q[3] * t2 + q[1] * y2 - q[2] * x2 + q[7];
            let dt = q[8] * t2 - q[9] * x2 - q[10] * y2 + q[12];
            let dx = q[8] * x2 + q[9] * t2 - q[11] * y2 + q[13];
            let dy = q[8] * y2 + q[10] * t2 + q[11] * x2 + q[14];
            let dz = q[11] * t2 + q[9] * y2 - q[10] * x2 + q[15];
            let ni = amount / (dt * dt + dx * dx + dy * dy + dz * dz);
            let x = d[2] * a.x + d[3] * a.y + (nt * dt + nx * dx + ny * dy + nz * dz) * ni + d[6];
            let y = d[4] * a.x + d[5] * a.y + (nx * dt - nt * dx - ny * dz + nz * dy) * ni + d[7];
            let k = (ctx.random_int(i32::MAX as usize) as i64 % d[25] as i64) as f64;
            let angle = (y.atan2(x) + M_2PI * k) / d[0];
            let r = amount * (x * x + y * y).powf(d[26]);
            v.x += r * angle.cos();
            v.y += r * angle.sin();
            let zq = (ny * dt - nt * dy - nz * dx + nx * dz) * ni;
            match d[24] as i32 {
                0 => {
                    let z = a.z / 2.0;
                    let r3d = (a.x * a.x + a.y * a.y + z * z).sqrt();
                    v.z += zq + amount / (r3d.sqrt() * r3d) * z;
                }
                1 => {
                    let z = a.z / d[27];
                    v.z += zq + amount * (a.x * a.x + a.y * a.y + z * z).powf(d[28]) * z;
                }
                _ => v.z += zq + a.z,
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            let power = ftoi(p[0]) as f64;
            d[0] = power;
            d[24] = limit_value(p[24], 0.0, 2.0).trunc();
            d.extend([power.abs(), p[1] / power * 0.5, power.abs(), (1.0 / power - 1.0) * 0.5]);
            d
        }),
        wgsl: "let t2 = a.x;
let x2 = a.y;
let y2 = a.z;
let nt = D(8) * t2 - D(9) * x2 - D(10) * y2 + D(12);
let nx = D(8) * x2 + D(9) * t2 - D(11) * y2 + D(13);
let ny = D(8) * y2 + D(10) * t2 + D(11) * x2 + D(14);
let nz = D(11) * t2 + D(9) * y2 - D(10) * x2 + D(15);
let dt = D(16) * t2 - D(17) * x2 - D(18) * y2 + D(20);
let dx = D(16) * x2 + D(17) * t2 - D(19) * y2 + D(21);
let dy = D(16) * y2 + D(18) * t2 + D(19) * x2 + D(22);
let dz = D(19) * t2 + D(17) * y2 - D(18) * x2 + D(23);
let ni = AMOUNT / (dt * dt + dx * dx + dy * dy + dz * dz);
let x = D(2) * a.x + D(3) * a.y + (nt * dt + nx * dx + ny * dy + nz * dz) * ni + D(6);
let y = D(4) * a.x + D(5) * a.y + (nx * dt - nt * dx - ny * dz + nz * dy) * ni + D(7);
let angle = (atan2(y, x) + TWO_PI * floor(rnd() * D(25))) / D(0);
let r = AMOUNT * java_pow(x * x + y * y, D(26));
v.x += r * cos(angle);
v.y += r * sin(angle);
let zq = (ny * dt - nt * dy - nz * dx + nx * dz) * ni;
let zmode = i32(D(24));
if (zmode == 0) {
  let z = a.z / 2.0;
  let r3d = sqrt(a.x * a.x + a.y * a.y + z * z);
  v.z += zq + AMOUNT / (sqrt(r3d) * r3d) * z;
} else if (zmode == 1) {
  let z = a.z / D(27);
  v.z += zq + AMOUNT * java_pow(a.x * a.x + a.y * a.y + z * z, D(28)) * z;
} else {
  v.z += zq + a.z;
}",
    },
    VarDef {
        name: "julia3Dq",
        // Default power: JWildfire picks a random one (+-2..7) for new instances
        params: &[ParamDef::int("power", 3.0, -10.0, 10.0), ParamDef::int("divisor", 2.0, -10.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [power, divisor, inv_power, abs_inv_power, half_inv_power, inv_power_2pi]
        apply: |d, ctx, a, v, amount| {
            let ang = a.y.atan2(a.x) * d[2] + ctx.random_int(i32::MAX as usize) as f64 * d[5];
            let z = a.z * d[3];
            let r2d = a.x * a.x + a.y * a.y;
            let r = amount * (r2d + z * z).powf(d[4]);
            v.z += r * z;
            let r = r * r2d.sqrt();
            v.x += r * ang.cos();
            v.y += r * ang.sin();
        },
        prepare: Some(|p, _| {
            let (power, divisor) = (ftoi(p[0]) as f64, ftoi(p[1]) as f64);
            let inv = divisor / power;
            vec![power, divisor, inv, inv.abs(), 0.5 * inv - 0.5, M_2PI / power]
        }),
        // GPU: random multiple of 2 pi / power reduced modulo |power| (same angles)
        wgsl: "let ang = atan2(a.y, a.x) * D(2) + floor(rnd() * abs(D(0))) * D(5);
let z = a.z * D(3);
let r2d = a.x * a.x + a.y * a.y;
let r = AMOUNT * java_pow(r2d + z * z, D(4));
v.z += r * z;
v.x += r * sqrt(r2d) * cos(ang);
v.y += r * sqrt(r2d) * sin(ang);",
    },
    VarDef {
        name: "juliac",
        // Default re: JWildfire picks a random one for new instances
        params: &[ParamDef::new("re", 4.0), ParamDef::new("im", 0.5), ParamDef::new("dist", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [re, im, dist, period]
        apply: |d, ctx, a, v, amount| {
            let re = 1.0 / (d[0] + SMALL_EPSILON);
            let im = d[1] / 100.0;
            let arg = a.y.atan2(a.x) + fmod(ctx.random_int(i32::MAX as usize) as f64, d[0]) * 2.0 * M_PI;
            let lnmod = d[2] * 0.5 * (a.x * a.x + a.y * a.y).ln();
            let ang = arg * re + lnmod * im;
            let mod2 = (lnmod * re - arg * im).exp();
            v.x += amount * mod2 * ang.cos();
            v.y += amount * mod2 * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], juliac_period(p[0])]),
        wgsl: "let re = 1.0 / D(0);
let im = D(1) / 100.0;
var k = floor(rnd() * 1000000.0);
if (D(3) > 0.0) { k = floor(rnd() * D(3)); }
let arg = atan2(a.y, a.x) + (k % D(0)) * TWO_PI;
let lnmod = D(2) * 0.5 * log(a.x * a.x + a.y * a.y);
let ang = arg * re + lnmod * im;
let mod2 = exp(lnmod * re - arg * im);
v.x += AMOUNT * mod2 * cos(ang);
v.y += AMOUNT * mod2 * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "julian2",
        // Default power: JWildfire picks a random one (+-2..7) for new instances
        params: &[
            ParamDef::int("power", 3.0, -10.0, 10.0),
            ParamDef::new("dist", 1.0),
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 0.0),
            ParamDef::new("c", 0.0),
            ParamDef::new("d", 1.0),
            ParamDef::new("e", 0.0),
            ParamDef::new("f", 0.0)
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [power, dist, a..f, abs_n, cn]
        apply: |d, ctx, a, v, amount| {
            if d[0] == 0.0 {
                return;
            }
            let x = d[2] * a.x + d[3] * a.y + d[6];
            let y = d[4] * a.x + d[5] * a.y + d[7];
            let k = (ctx.random_int(i32::MAX as usize) as i64 % d[8] as i64) as f64;
            let angle = (y.atan2(x) + M_2PI * k) / d[0];
            let r = amount * (x * x + y * y).powf(d[9]);
            v.x += r * angle.cos();
            v.y += r * angle.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            let power = ftoi(p[0]) as f64;
            d[0] = power;
            d.extend([power.abs(), p[1] / power * 0.5]);
            d
        }),
        wgsl: "if (D(0) != 0.0) {
  let x = D(2) * a.x + D(3) * a.y + D(6);
  let y = D(4) * a.x + D(5) * a.y + D(7);
  let angle = (atan2(y, x) + TWO_PI * floor(rnd() * D(8))) / D(0);
  let r = AMOUNT * java_pow(x * x + y * y, D(9));
  v.x += r * cos(angle);
  v.y += r * sin(angle);
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    VarDef {
        name: "julian3Dx",
        // Default power: JWildfire picks a random one (+-2..7) for new instances
        params: &[
            ParamDef::int("power", 3.0, -10.0, 10.0),
            ParamDef::new("dist", 1.0),
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 0.0),
            ParamDef::new("c", 0.0),
            ParamDef::new("d", 1.0),
            ParamDef::new("e", 0.0),
            ParamDef::new("f", 0.0)
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [power, dist, a..f, abs_n, cn]
        apply: |d, ctx, a, v, amount| {
            let z = a.x / d[8];
            let rsq = a.x * a.x + a.y * a.y;
            let rout = amount * (rsq + z * z).powf(d[9]);
            v.z += rout * z;
            let x = d[2] * a.x + d[3] * a.y + d[6];
            let y = d[4] * a.x + d[5] * a.y + d[7];
            let rand = ((ctx.random() * d[8]) as i32) as f64;
            let alpha = (y.atan2(x) + M_2PI * rand) / d[0];
            let gamma = rout * rsq.sqrt();
            v.x += gamma * alpha.cos();
            v.y += gamma * alpha.sin();
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            let power = ftoi(p[0]) as f64;
            d[0] = power;
            d.extend([power.abs(), (p[1] / power - 1.0) / 2.0]);
            d
        }),
        wgsl: "let z = a.x / D(8);
let rsq = a.x * a.x + a.y * a.y;
let rout = AMOUNT * java_pow(rsq + z * z, D(9));
v.z += rout * z;
let x = D(2) * a.x + D(3) * a.y + D(6);
let y = D(4) * a.x + D(5) * a.y + D(7);
let alpha = (atan2(y, x) + TWO_PI * trunc(rnd() * D(8))) / D(0);
let gamma = rout * sqrt(rsq);
v.x += gamma * cos(alpha);
v.y += gamma * sin(alpha);",
    },
    VarDef {
        name: "julia_outside",
        params: &[ParamDef::new("re_div", 1.0), ParamDef::new("im_div", 0.0), ParamDef::int("mode", 0.0, 0.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let mode = d[2] as i32;
            let (outer, inner) = (mode == 0 || mode == 2, mode == 0 || mode == 1);
            let mut z = Complex::new(a.x, a.y);
            let mut z2 = Complex::new(a.x, a.y);
            if outer {
                z.sqrt();
            }
            z.inc();
            if outer {
                z.sqr();
                z2.sqrt();
            }
            z2.dec();
            if outer {
                z2.sqr();
            }
            z.div(&z2);
            if inner {
                z.sqrt();
            }
            z.div(&Complex::new(d[0], d[1]));
            let s = if inner && ctx.random() >= 0.5 { -1.0 } else { 1.0 };
            v.x += amount * s * z.re;
            v.y += amount * s * z.im;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], limit_value(p[2], 0.0, 2.0).trunc()]),
        wgsl: "let mode = i32(D(2));
let outer = mode == 0 || mode == 2;
let inner = mode == 0 || mode == 1;
var z = vec2<f32>(a.x, a.y);
var z2 = z;
if (outer) { z = cx_sqrt(z); }
z.x += 1.0;
if (outer) { z = cx_sqr(z); z2 = cx_sqrt(z2); }
z2.x -= 1.0;
if (outer) { z2 = cx_sqr(z2); }
z = cx_div(z, z2);
if (inner) { z = cx_sqrt(z); }
z = cx_div(z, vec2<f32>(D(0), D(1)));
var s = 1.0;
if (inner && rnd() >= 0.5) { s = -1.0; }
v.x += AMOUNT * s * z.x;
v.y += AMOUNT * s * z.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "juliaq",
        // Default power: JWildfire picks a random one (+-2..7) for new instances
        params: &[ParamDef::int("power", 3.0, -10.0, 10.0), ParamDef::int("divisor", 2.0, -10.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [power, divisor, half_inv_power, inv_power, inv_power_2pi]
        apply: |d, ctx, a, v, amount| {
            let ang = a.y.atan2(a.x) * d[3] + ctx.random_int(i32::MAX as usize) as f64 * d[4];
            let r = amount * (a.x * a.x + a.y * a.y).powf(d[2]);
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let (power, divisor) = (ftoi(p[0]) as f64, ftoi(p[1]) as f64);
            vec![power, divisor, 0.5 * divisor / power, divisor / power, M_2PI / power]
        }),
        // GPU: random multiple of 2 pi / power reduced modulo |power| (same angles)
        wgsl: "let ang = atan2(a.y, a.x) * D(3) + floor(rnd() * abs(D(0))) * D(4);
let r = AMOUNT * java_pow(a.x * a.x + a.y * a.y, D(2));
v.x += r * cos(ang);
v.y += r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "juliascope3Db",
        // Default power: JWildfire picks a random one (+-2..7) for new instances
        params: &[
            ParamDef::new("power", 3.0),
            ParamDef::new("dist", 1.0),
            ParamDef::int("type", 0.0, 0.0, 1.0),
            ParamDef::new("warp", 0.0),
            ParamDef::new("za", 1.0),
            ParamDef::new("zb", 1.0),
            ParamDef::new("zamount", 1.0),
            ParamDef::new("zdist", 1.0),
            ParamDef::int("mode", 0.0, 0.0, 1.0),
            ParamDef::new("wave1", 1.0),
            ParamDef::new("wave2", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [<params>, abs_power, cpower]
        apply: |d, ctx, a, v, amount| {
            let (abs_power, cpower) = (d[11], d[12]);
            let z = a.z / abs_power + d[3];
            let r2d = a.x * a.x * d[4] + a.y * a.y * d[5];
            let rz = amount * d[6] * (r2d + z * z).powf(cpower * d[7]);
            let rnd = ctx.random_int(abs_power as usize) as i32;
            let theta = a.y.atan2(a.x);
            let ang = if rnd & 1 == 0 { (2.0 * M_PI * rnd as f64 + theta) / d[0] } else { (2.0 * M_PI * rnd as f64 - theta) / d[0] };
            let rsq = if d[2] != 0.0 { a.x * a.x + a.y * a.y + a.z * a.z } else { a.x * a.x + a.y * a.y };
            let r = amount * rsq.powf(cpower);
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            if d[8] != 0.0 {
                v.z += rz * z * (d[9].sin() * d[10].cos());
            } else {
                v.z += rz * z;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[2] = limit_value(p[2], 0.0, 1.0).trunc();
            d[8] = limit_value(p[8], 0.0, 1.0).trunc();
            d.extend([ftoi(p[0]).abs() as f64, p[1] / p[0] * 0.5]);
            d
        }),
        wgsl: "let ap = D(11);
let cpower = D(12);
let z = a.z / ap + D(3);
let r2d = a.x * a.x * D(4) + a.y * a.y * D(5);
let rz = AMOUNT * D(6) * java_pow(r2d + z * z, cpower * D(7));
let rnd = i32(floor(rnd() * ap));
let theta = atan2(a.y, a.x);
var ang = (TWO_PI * f32(rnd) + theta) / D(0);
if ((rnd & 1) != 0) { ang = (TWO_PI * f32(rnd) - theta) / D(0); }
var rsq = a.x * a.x + a.y * a.y;
if (D(2) != 0.0) { rsq += a.z * a.z; }
let r = AMOUNT * java_pow(rsq, cpower);
v.x += r * cos(ang);
v.y += r * sin(ang);
if (D(8) != 0.0) { v.z += rz * z * (sin(D(9)) * cos(D(10))); } else { v.z += rz * z; }",
    },
    VarDef {
        name: "juliascopePlus",
        // Default power: JWildfire picks a random one (+-2..5) for new instances
        params: &[
            ParamDef::int("power", 3.0, -10.0, 10.0),
            ParamDef::new("dist", 1.0),
            ParamDef::new("cx", 0.0),
            ParamDef::new("cy", 0.0),
            ParamDef::new("mixPowerSign", 0.0),
            ParamDef::int("colorMode", 0.0, 0.0, 5.0),
            ParamDef::new("colorSpeed", 1.0),
            ParamDef::int("thetaSignMode", 0.0, 0.0, 3.0),
            ParamDef::new("angularOffset", 0.0),
            ParamDef::new("swirlFactor", 0.0),
            ParamDef::new("fixedBranch", -1.0),
            ParamDef::new("radialOffset", 0.0),
            ParamDef::new("altPowerScale", 1.0),
            ParamDef::new("altPowerRotate", 0.0),
            ParamDef::int("rndTermMode", 0.0, 0.0, 3.0),
            ParamDef::int("mandelbrotLike", 0.0, 0.0, 1.0)
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| juliascope_plus_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            let power = java_round(p[0]) as i32;
            d[0] = power as f64;
            d[5] = limit_value(ftoi(p[5]), 0, 5) as f64;
            d[7] = limit_value(ftoi(p[7]), 0, 3) as f64;
            d[14] = limit_value(ftoi(p[14]), 0, 3) as f64;
            d[15] = limit_value(ftoi(p[15]), 0, 1) as f64;
            d.push(if power == 0 { 2.0 } else { power.abs().max(1) as f64 });
            d
        }),
        wgsl: "let power = i32(D(0));
let nb = i32(D(16));
var sgn = 1;
if (power < 0) { sgn = -1; }
var eff = power;
var alt = false;
if (power != 0 && D(4) >= 0.5) {
  if (rnd() < 0.5) { eff = nb * sgn; } else { eff = -nb * sgn; alt = true; }
}
let flipped = eff < 0;
var add = vec2<f32>(D(2), D(3));
if (D(15) >= 1.0) { add = vec2<f32>(a.x, a.y); }
if (eff == 0) {
  v.x += AMOUNT * (a.x + add.x);
  v.y += AMOUNT * (a.y + add.y);
} else {
  let cpower = D(1) / f32(eff) * 0.5;
  let fixed = i32(java_round(D(10)));
  let num = max(nb, 1);
  var rnd_i = min(i32(rnd() * f32(num)), num - 1);
  if (fixed >= 0 && fixed < num) { rnd_i = fixed; }
  let theta = atan2(a.y, a.x);
  let tm = i32(D(7));
  var tsign = 1.0;
  if (tm == 0) { if ((rnd_i & 1) != 0) { tsign = -1.0; } }
  else if (tm == 2) { tsign = -1.0; }
  else if (tm == 3) { tsign = select(-1.0, 1.0, rnd() < 0.5); }
  let rm = i32(D(14));
  var rsign = 1.0;
  if (rm == 1) { rsign = -1.0; }
  else if (rm == 2) { if ((rnd_i & 1) != 0) { rsign = -1.0; } }
  else if (rm == 3) { rsign = select(-1.0, 1.0, rnd() < 0.5); }
  var ang = (rsign * TWO_PI * f32(rnd_i) + tsign * theta) / f32(eff);
  if (D(9) != 0.0) {
    let r0 = sqrt(a.x * a.x + a.y * a.y);
    if (r0 > 0.00001) { ang += D(9) * log(1.0 + r0); }
  }
  ang += D(8);
  let rmag = java_pow(max(0.0, a.x * a.x + a.y * a.y), cpower) + D(11);
  var dd = rmag * vec2<f32>(cos(ang), sin(ang));
  if (alt) {
    dd *= D(12);
    if (D(13) != 0.0) {
      let cr = cos(D(13));
      let sr = sin(D(13));
      dd = vec2<f32>(dd.x * cr - dd.y * sr, dd.x * sr + dd.y * cr);
    }
  }
  v.x += AMOUNT * (dd.x + add.x);
  v.y += AMOUNT * (dd.y + add.y);
  let cm = i32(D(5));
  if (cm != 0) {
    var val = 0.25;
    if (cm == 1) { val = ang / TWO_PI; }
    else if (cm == 2) { val = log(1.00001 + max(0.0, rmag)); }
    else if (cm == 3) { val = (theta + PI) / TWO_PI; }
    else if (cm == 4) { val = f32(rnd_i) / f32(num); }
    else if (D(4) >= 0.5 && power != 0) { val = select(0.25, 0.75, flipped); }
    else { val = select(0.75, 0.25, power >= 0); }
    let cv = val * D(6);
    v.color = cv - trunc(cv);
  }
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "kaplan",
        params: &[ParamDef::int("seed", 1000.0, 0.0, 100000.0), ParamDef::int("N", 800.0, 50.0, 1500.0), ParamDef::new("time", 10.0), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let n = d[1];
            let (mut x, mut y) = ((n * ctx.random()) as i32, (n * ctx.random()) as i32);
            let zoom = d[2].floor();
            let (xv, yv) = (zoom * (x as f64 - n / 2.0), zoom * (y as f64 - n / 2.0));
            let r0 = super::glsl::glsl_atan(xv, yv);
            let uv = vec2(xv, yv) * mat2(r0.cos(), -r0.sin(), r0.sin(), r0.cos());
            let value = uv.x * uv.x + uv.y * uv.y;
            let exponent = (value.ln() / 2f64.ln()).floor();
            let mantissa = value * 2f64.powf(-exponent) - 1.0;
            let r = mantissa - (mantissa * 2f64.powf(16.0) + 0.5).floor() / 2f64.powf(16.0);
            let hide = if d[3] == 0.0 { r > 0.0 } else { r <= 0.0 };
            v.do_hide = hide;
            if hide {
                x = 0;
                y = 0;
            }
            v.x = amount * (x as f64 / n - 0.5);
            v.y = amount * (y as f64 / n - 0.5);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], limit_value(p[1], 50.0, 1500.0).trunc(), p[2], limit_value(p[3], 0.0, 1.0).trunc()]),
        wgsl: "let n = D(1);
var x = trunc(n * rnd());
var y = trunc(n * rnd());
let zoom = floor(D(2));
let xv = zoom * (x - n / 2.0);
let yv = zoom * (y - n / 2.0);
let r0 = glsl_atan(xv, yv);
let uv = vec2<f32>(xv, yv) * mat2x2<f32>(cos(r0), -sin(r0), sin(r0), cos(r0));
let value = dot(uv, uv);
let exponent = floor(log2(value));
let mantissa = value * exp2(-exponent) - 1.0;
let r = mantissa - floor(mantissa * 65536.0 + 0.5) / 65536.0;
var hide = r <= 0.0;
if (D(3) == 0.0) { hide = r > 0.0; }
if (hide) { x = 0.0; y = 0.0; v.hide = 1.0; } else { v.hide = 0.0; }
v.x = AMOUNT * (x / n - 0.5);
v.y = AMOUNT * (y / n - 0.5);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "karman_vortex",
        params: &[ParamDef::new("freq", 2.0), ParamDef::new("strength", 0.3), ParamDef::new("sep", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let freq = java_max(d[0].abs(), 0.01);
            let xi = java_round(a.x * freq);
            let mut parity = xi % 2.0;
            if parity < 0.0 {
                parity += 2.0;
            }
            let (vcx, vcy) = (xi / freq, d[2] * 0.5 * (2.0 * parity - 1.0));
            let (dx, dy) = (a.x - vcx, a.y - vcy);
            let spin = d[1] * (2.0 * parity - 1.0) / (dx * dx + dy * dy + 1e-4);
            v.x += amount * (a.x + spin * (-dy));
            v.y += amount * (a.y + spin * dx);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let freq = max(abs(D(0)), 0.01);
let xi = java_round(a.x * freq);
var parity = xi % 2.0;
if (parity < 0.0) { parity += 2.0; }
let dx = a.x - xi / freq;
let dy = a.y - D(2) * 0.5 * (2.0 * parity - 1.0);
let spin = D(1) * (2.0 * parity - 1.0) / (dx * dx + dy * dy + 1e-4);
v.x += AMOUNT * (a.x + spin * (-dy));
v.y += AMOUNT * (a.y + spin * dx);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "kifs3d",
        params: &[
            ParamDef::int("max_iter", 7.0, 1.0, 100.0),
            ParamDef::new("bailout_radius", 1.1),
            ParamDef::new("kifs_scale_x", 1.4),
            ParamDef::new("kifs_scale_y", 1.4),
            ParamDef::new("kifs_scale_z", -1.0),
            ParamDef::int("scale_pivot", 0.0, 0.0, 1.0),
            ParamDef::new("center_x", 0.0),
            ParamDef::new("center_y", 0.0),
            ParamDef::new("center_z", 0.0),
            ParamDef::new("offset_x", -1.0),
            ParamDef::new("offset_y", -1.0),
            ParamDef::new("offset_z", 1.0),
            ParamDef::int("mirror_fold", 0.0, 0.0, 1.0),
            ParamDef::int("fold_type", 1.0, 0.0, 4.0),
            ParamDef::new("fold_plane1_nx", 1.0),
            ParamDef::new("fold_plane1_ny", 0.0),
            ParamDef::new("fold_plane1_nz", 0.0),
            ParamDef::new("fold_plane1_dist", 0.0),
            ParamDef::new("fold_plane1_intensity", 1.0),
            ParamDef::new("edge_x", 0.0),
            ParamDef::new("edge_y", 0.0),
            ParamDef::new("edge_z", 0.0),
            ParamDef::new("rot_x", 5.0),
            ParamDef::new("rot_y", 5.0),
            ParamDef::new("rot_z", 5.0),
            ParamDef::int("rot_order", 0.0, 0.0, 1.0),
            ParamDef::int("rot_pivot", 0.0, 0.0, 2.0),
            ParamDef::int("transform_order", 0.0, 0.0, 2.0),
            ParamDef::int("post_symmetry", 7.0, 0.0, 7.0),
            ParamDef::int("color_mode", 0.0, 0.0, 5.0),
            ParamDef::new("color_scale", 1.0)
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| kifs3d_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = p[0].trunc();
            for (i, hi) in [(5, 1), (12, 1), (13, 4), (25, 1), (26, 2), (27, 2), (28, 7), (29, 5)] {
                d[i] = limit_value(ftoi(p[i]), 0, hi) as f64;
            }
            if d[18] < 0.0 {
                d[18] = 0.0;
            }
            let mut bsq = p[1] * p[1];
            if bsq <= 0.0 {
                bsq = 1e-6;
            }
            d.extend([bsq, p[22].to_radians(), p[23].to_radians(), p[24].to_radians(), p[14] * p[14] + p[15] * p[15] + p[16] * p[16]]);
            d
        }),
        wgsl: "fn kifs3d_fold(base: u32, p: vec3<f32>) -> vec3<f32> {
  let n = vec3<f32>(prm[base + 14u], prm[base + 15u], prm[base + 16u]);
  let dist = prm[base + 17u];
  let len_sq = prm[base + 35u];
  let edge = vec3<f32>(prm[base + 19u], prm[base + 20u], prm[base + 21u]);
  let ft = i32(prm[base + 13u]);
  var sg = vec3<f32>(1.0);
  let mirror = prm[base + 12u] > 0.0;
  if (mirror) {
    sg = select(vec3<f32>(1.0), vec3<f32>(-1.0), vec3<bool>(rnd() < 0.5, rnd() < 0.5, rnd() < 0.5));
  }
  if (ft == 1) {
    if (mirror) { return p * sg; }
    return abs(p);
  }
  if (ft == 2) {
    var flip = p.x + p.y < 0.0;
    if (mirror) { flip = sg.x < 0.0; }
    if (flip) { return vec3<f32>(-p.y, -p.x, p.z); }
    return p;
  }
  if (ft == 3) {
    let dot_n = dot(p, n) - dist;
    var doit = dot_n < 0.0;
    if (mirror) { doit = sg.x < 0.0; }
    if (len_sq > 1e-9 && doit) {
      return p - (1.0 + prm[base + 18u]) * dot_n / len_sq * n;
    }
    return p;
  }
  if (ft == 4) {
    let s = select(vec3<f32>(1.0), sg, mirror);
    return select(p, edge - s * abs(edge - p), edge != vec3<f32>(0.0));
  }
  return p;
}
fn kifs3d_rotate(base: u32, p: vec3<f32>, pivot: vec3<f32>) -> vec3<f32> {
  let ax = prm[base + 32u];
  let ay = prm[base + 33u];
  let az = prm[base + 34u];
  if (ax == 0.0 && ay == 0.0 && az == 0.0) { return p; }
  var r = p - pivot;
  if (i32(prm[base + 25u]) == 0) {
    r = vec3<f32>(r.x * cos(az) - r.y * sin(az), r.x * sin(az) + r.y * cos(az), r.z);
    r = vec3<f32>(r.x * cos(ay) + r.z * sin(ay), r.y, -r.x * sin(ay) + r.z * cos(ay));
    r = vec3<f32>(r.x, r.y * cos(ax) - r.z * sin(ax), r.y * sin(ax) + r.z * cos(ax));
  } else {
    r = vec3<f32>(r.x, r.y * cos(ax) - r.z * sin(ax), r.y * sin(ax) + r.z * cos(ax));
    r = vec3<f32>(r.x * cos(ay) + r.z * sin(ay), r.y, -r.x * sin(ay) + r.z * cos(ay));
    r = vec3<f32>(r.x * cos(az) - r.y * sin(az), r.x * sin(az) + r.y * cos(az), r.z);
  }
  return r + pivot;
}
fn kifs3d_scale(base: u32, p: vec3<f32>) -> vec3<f32> {
  let offset = vec3<f32>(prm[base + 9u], prm[base + 10u], prm[base + 11u]);
  let s = vec3<f32>(prm[base + 2u], prm[base + 3u], prm[base + 4u]);
  if (i32(prm[base + 5u]) == 1) { return s * (p - offset) + offset; }
  let pivot = vec3<f32>(prm[base + 6u], prm[base + 7u], prm[base + 8u]);
  return s * (p - pivot) + pivot + offset;
}
@body
var p = vec3<f32>(a.x, a.y, a.z);
let max_iter = i32(D(0));
var iter_color = 1.0;
var pivot = vec3<f32>(0.0);
let pt = i32(D(26));
if (pt == 1) { pivot = vec3<f32>(D(6), D(7), D(8)); } else if (pt == 2) { pivot = vec3<f32>(D(9), D(10), D(11)); }
let order = i32(D(27));
for (var i = 0; i < max_iter; i++) {
  if (order == 0) {
    p = kifs3d_scale(base, kifs3d_rotate(base, kifs3d_fold(base, p), pivot));
  } else if (order == 1) {
    p = kifs3d_scale(base, kifs3d_fold(base, kifs3d_rotate(base, p, pivot)));
  } else {
    p = kifs3d_rotate(base, kifs3d_fold(base, kifs3d_scale(base, p)), pivot);
  }
  if (dot(p, p) > D(31)) {
    iter_color = f32(i) / f32(max_iter);
    break;
  }
}
let sym = i32(D(28));
if (sym > 0) {
  let sg = select(vec3<f32>(1.0), vec3<f32>(-1.0), vec3<bool>(rnd() < 0.5, rnd() < 0.5, rnd() < 0.5));
  if ((sym & 1) != 0) { p.x *= sg.x; }
  if ((sym & 2) != 0) { p.y *= sg.y; }
  if ((sym & 4) != 0) { p.z *= sg.z; }
}
let cs = D(30);
let cm = i32(D(29));
var col = iter_color;
if (cm == 1) { col = fract(length(p) * cs); }
else if (cm == 2) { col = fract((atan2(p.y, p.x) + PI) / TWO_PI * cs); }
else if (cm == 3) { col = fract(p.x * cs); }
else if (cm == 4) { col = fract(p.y * cs); }
else if (cm == 5) { col = fract(p.z * cs); }
v.x += p.x * AMOUNT;
v.y += p.y * AMOUNT;
v.z += p.z * AMOUNT;
v.color = col;",
    },
    VarDef {
        name: "klein",
        params: &[ParamDef::new("inner_radius", 0.5), ParamDef::new("twist", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let ri = java_max(d[0].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let (nr, nt) = if r < ri { (r, theta + d[1] * r) } else { ((ri * ri) / java_max(r, 1e-6), -theta + d[1] * r) };
            v.x += amount * nr * nt.cos();
            v.y += amount * nr * nt.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ri = max(abs(D(0)), 0.01);
let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
var nr = r;
var nt = theta + D(1) * r;
if (r >= ri) {
  nr = (ri * ri) / max(r, 1e-6);
  nt = -theta + D(1) * r;
}
v.x += AMOUNT * nr * cos(nt);
v.y += AMOUNT * nr * sin(nt);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "klein_group",
        params: &[
            ParamDef::new("a_re", 2.0),
            ParamDef::new("a_im", 0.0),
            ParamDef::new("b_re", 2.0),
            ParamDef::new("b_im", 0.0),
            ParamDef::int("recipe", 0.0, 0.0, 6.0),
            ParamDef::int("avoid_reversal", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [<params>, matrices a, a^-1, b, b^-1 (4 x 8 values, re/im of m0..m3)]
        apply: |d, ctx, a, v, amount| {
            // the inverse of matrix i is i ^ 1; with avoid_reversal the inverse of the previous
            // matrix is left out (the previous matrix is kept per render thread on the CPU)
            let k = if d[5] == 1.0 {
                let prev = ctx.orbits.entry(d.as_ptr() as usize).or_insert([0.0, 0.0]);
                let skip = (prev[0] as usize) ^ 1;
                let mut k = ctx.random_int(3);
                if k >= skip {
                    k += 1;
                }
                k
            } else {
                ctx.random_int(4)
            };
            if d[5] == 1.0 {
                ctx.orbits.insert(d.as_ptr() as usize, [k as f64, 0.0]);
            }
            let m = &d[6 + 8 * k..14 + 8 * k];
            let w = Jc(a.x / amount, a.y / amount);
            let out = w.mul(Jc(m[0], m[1])).add(Jc(m[2], m[3])).div(w.mul(Jc(m[4], m[5])).add(Jc(m[6], m[7])));
            v.x += amount * out.0;
            v.y += amount * out.1;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = p[4].floor();
            d[5] = if p[5] == 1.0 { 1.0 } else { 0.0 };
            let [ma, mb] = klein_group_generators(p[0], p[1], p[2], p[3], d[4] as i32);
            let inv = |m: [Jc; 4]| [m[3], m[1].muls(-1.0), m[2].muls(-1.0), m[0]];
            for m in [ma, inv(ma), mb, inv(mb)] {
                for c in m {
                    d.extend([c.0, c.1]);
                }
            }
            d
        }),
        // GPU: the previous matrix (avoid_reversal) is kept per shader invocation
        wgsl: "var<private> klein_group_prev: u32;
fn klein_group_mul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> { return vec2<f32>(a.x * b.x - a.y * b.y, a.y * b.x + a.x * b.y); }
@body
var k = min(u32(rnd() * 4.0), 3u);
if (D(5) == 1.0) {
  // klein_group_prev holds index + 1 (0: not yet set, i.e. the initial matrix a)
  let prev = max(klein_group_prev, 1u) - 1u;
  k = min(u32(rnd() * 3.0), 2u);
  if (k >= (prev ^ 1u)) { k += 1u; }
  klein_group_prev = k + 1u;
}
let o = base + 6u + 8u * k;
let w = vec2<f32>(a.x, a.y) / AMOUNT;
let num = klein_group_mul(w, vec2<f32>(prm[o], prm[o + 1u])) + vec2<f32>(prm[o + 2u], prm[o + 3u]);
let den = klein_group_mul(w, vec2<f32>(prm[o + 4u], prm[o + 5u])) + vec2<f32>(prm[o + 6u], prm[o + 7u]);
let q = vec2<f32>(num.x * den.x + num.y * den.y, num.y * den.x - num.x * den.y) / dot(den, den);
v.x += AMOUNT * q.x;
v.y += AMOUNT * q.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "koch_js",
        params: &[
            ParamDef::int("level", 2.0, 1.0, 7.0),
            ParamDef::int("show_lines", 1.0, 0.0, 1.0),
            ParamDef::new("line_thickness", 0.5),
            ParamDef::int("show_points", 0.0, 0.0, 1.0),
            ParamDef::new("point_thickness", 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| turtle_polyline_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| koch_prepare(p)),
        wgsl: TURTLE_POLYLINE_WGSL,
    },
    VarDef {
        name: "labyrinth",
        params: &[ParamDef::new("scale", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sc = java_max(d[0].abs(), 0.01);
            let (ri, ci) = ((a.x / sc + 0.5).floor(), (a.y / sc + 0.5).floor());
            let (lx, ly) = (a.x - ri * sc, a.y - ci * sc);
            let fx = if ci.abs() % 2.0 > 0.5 { -lx } else { lx };
            let fy = if ri.abs() % 2.0 > 0.5 { -ly } else { ly };
            v.x += amount * (ri * sc + fx);
            v.y += amount * (ci * sc + fy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sc = max(abs(D(0)), 0.01);
let ri = floor(a.x / sc + 0.5);
let ci = floor(a.y / sc + 0.5);
var fx = a.x - ri * sc;
var fy = a.y - ci * sc;
if (abs(ci) % 2.0 > 0.5) { fx = -fx; }
if (abs(ri) % 2.0 > 0.5) { fy = -fy; }
v.x += AMOUNT * (ri * sc + fx);
v.y += AMOUNT * (ci * sc + fy);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "lace",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("amp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let freq = java_max(d[0].abs(), 0.01);
            v.x += amount * (a.x + d[1] * (freq * a.y).sin());
            v.y += amount * (a.y + d[1] * (freq * a.x).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let freq = max(abs(D(0)), 0.01);
v.x += AMOUNT * (a.x + D(1) * sin(freq * a.y));
v.y += AMOUNT * (a.y + D(1) * sin(freq * a.x));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "lace_js",
        false,
        |_, ctx, a, v, amount| {
            let r0 = (a.x * a.x + a.y * a.y).sqrt();
            let s3 = 3f64.sqrt() / 2.0;
            let weight = ctx.random();
            let (x, y) = if weight > 0.75 {
                let w = a.y.atan2(a.x - 1.0);
                (-r0 * w.sin() / 2.0, -r0 * w.cos() / 2.0 + 1.0)
            } else if weight > 0.50 {
                let w = (a.y - s3).atan2(a.x + 0.5);
                (-r0 * w.sin() / 2.0 + s3, -r0 * w.cos() / 2.0 - 0.5)
            } else if weight > 0.25 {
                let w = (a.y + s3).atan2(a.x + 0.5);
                (-r0 * w.sin() / 2.0 - s3, -r0 * w.cos() / 2.0 - 0.5)
            } else {
                let w = a.y.atan2(a.x);
                (-r0 * w.sin() / 2.0, -r0 * w.cos() / 2.0)
            };
            v.x += x * amount;
            v.y += y * amount;
        },
        "let r0 = sqrt(a.x * a.x + a.y * a.y);
let s3 = sqrt(3.0) / 2.0;
let weight = rnd();
var c = vec2<f32>(0.0);
var w = atan2(a.y, a.x);
if (weight > 0.75) {
  w = atan2(a.y, a.x - 1.0);
  c = vec2<f32>(0.0, 1.0);
} else if (weight > 0.50) {
  w = atan2(a.y - s3, a.x + 0.5);
  c = vec2<f32>(s3, -0.5);
} else if (weight > 0.25) {
  w = atan2(a.y + s3, a.x + 0.5);
  c = vec2<f32>(-s3, -0.5);
}
v.x += (-r0 * sin(w) / 2.0 + c.x) * AMOUNT;
v.y += (-r0 * cos(w) / 2.0 + c.y) * AMOUNT;",
    ),
    VarDef {
        name: "lagrange",
        params: &[ParamDef::new("separation", 0.5), ParamDef::new("strength", 0.2)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sep = java_max(d[0].abs(), 0.01);
            let (dx1, dx2) = (a.x + sep * 0.5, a.x - sep * 0.5);
            let r1 = (dx1 * dx1 + a.y * a.y).sqrt() + 0.1;
            let r2 = (dx2 * dx2 + a.y * a.y).sqrt() + 0.1;
            let px = -d[1] * (dx1 / (r1 * r1 * r1) + dx2 / (r2 * r2 * r2));
            let py = -d[1] * (a.y / (r1 * r1 * r1) + a.y / (r2 * r2 * r2));
            v.x += amount * (a.x + px);
            v.y += amount * (a.y + py);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sep = max(abs(D(0)), 0.01);
let dx1 = a.x - (-sep * 0.5);
let dx2 = a.x - sep * 0.5;
let r1 = sqrt(dx1 * dx1 + a.y * a.y) + 0.1;
let r2 = sqrt(dx2 * dx2 + a.y * a.y) + 0.1;
let px = -D(1) * (dx1 / (r1 * r1 * r1) + dx2 / (r2 * r2 * r2));
let py = -D(1) * (a.y / (r1 * r1 * r1) + a.y / (r2 * r2 * r2));
v.x += AMOUNT * (a.x + px);
v.y += AMOUNT * (a.y + py);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "laminar",
        params: &[ParamDef::new("shear", 0.5), ParamDef::new("amp", 0.1), ParamDef::new("freq", 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * (d[0] * (1.0 - a.y * a.y) + d[1] * (d[2] * a.y).sin());
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * (D(0) * (1.0 - a.y * a.y) + D(1) * sin(D(2) * a.y));
v.y += AMOUNT * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "lattice",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("pull", 0.5), ParamDef::new("shear", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let freq = java_max(d[0].abs(), 0.01);
            let pull = java_max(0.0, java_min(1.0, d[1]));
            let dx = java_round(a.x * freq) / freq - a.x;
            let dy = java_round(a.y * freq) / freq - a.y;
            v.x += amount * (a.x + pull * dx + d[2] * dy);
            v.y += amount * (a.y + pull * dy + d[2] * dx);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let freq = max(abs(D(0)), 0.01);
let pull = clamp(D(1), 0.0, 1.0);
let dx = java_round(a.x * freq) / freq - a.x;
let dy = java_round(a.y * freq) / freq - a.y;
v.x += AMOUNT * (a.x + pull * dx + D(2) * dy);
v.y += AMOUNT * (a.y + pull * dy + D(2) * dx);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "layered_spiral",
        params: &[ParamDef::new("radius", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = a.x * d[0];
            let t = a.x * a.x + a.y * a.y + EPSILON;
            v.x += amount * r * t.cos();
            v.y += amount * r * t.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = a.x * D(0);
let t = a.x * a.x + a.y * a.y + EPSILON;
v.x += AMOUNT * r * cos(t);
v.y += AMOUNT * r * sin(t);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "lazyjess",
        params: &[ParamDef::int("n", 4.0, 2.0, 100.0), ParamDef::new("spin", M_PI), ParamDef::new("space", 0.0), ParamDef::int("corner", 1.0, -100.0, 100.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [n, spin, space, corner, vertex, sin_vertex, pie_slice, half_slice, corner_rotation]
        apply: |d, ctx, a, v, amount| {
            let (spin, space) = (d[1], d[2]);
            let (sin_vertex, vertex, pie, half, crot) = (d[5], d[4], d[6], d[7], d[8]);
            let (mut x, mut y) = (a.x, a.y);
            let mut modulus = (x * x + y * y).sqrt();
            let inside;
            if d[0] as i32 == 2 {
                inside = x.abs() < amount;
                if inside {
                    let th = y.atan2(x) + spin;
                    x = amount * modulus * th.cos();
                    y = amount * modulus * th.sin();
                    if x.abs() < amount {
                        v.x += x;
                        v.y += y;
                    } else {
                        let th = y.atan2(x) - spin + crot;
                        v.x += amount * modulus * th.cos();
                        v.y -= amount * modulus * th.sin();
                    }
                }
            } else {
                let th = y.atan2(x) + M_2PI;
                let diff = fmod(th + half, pie);
                let r = amount * M_SQRT2 * sin_vertex / (M_PI - diff - vertex).sin();
                inside = modulus < r;
                if inside {
                    let th = y.atan2(x) + spin + M_2PI;
                    x = amount * modulus * th.cos();
                    y = amount * modulus * th.sin();
                    let diff = fmod(th + half, pie);
                    let r = amount * M_SQRT2 * sin_vertex / (M_PI - diff - vertex).sin();
                    modulus = (x * x + y * y).sqrt();
                    if modulus < r {
                        v.x += x;
                        v.y += y;
                    } else {
                        let th = y.atan2(x) - spin + crot + M_2PI;
                        v.x += amount * modulus * th.cos();
                        v.y -= amount * modulus * th.sin();
                    }
                }
            }
            if !inside {
                let m = 1.0 + space / modulus;
                v.x += amount * m * x;
                v.y += amount * m * y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let n = ftoi(p[0]).max(2) as f64;
            let corner = ftoi(p[3]) as f64;
            let vertex = M_PI * (n - 2.0) / (2.0 * n);
            let pie = M_2PI / n;
            vec![n, p[1], p[2], corner, vertex, vertex.sin(), pie, pie / 2.0, (corner - 1.0) * pie]
        }),
        wgsl: "let spin = D(1);
let space = D(2);
var x = a.x;
var y = a.y;
var modulus = sqrt(x * x + y * y);
var inside = false;
if (i32(D(0)) == 2) {
  inside = abs(x) < AMOUNT;
  if (inside) {
    let th = atan2(y, x) + spin;
    x = AMOUNT * modulus * cos(th);
    y = AMOUNT * modulus * sin(th);
    if (abs(x) < AMOUNT) {
      v.x += x;
      v.y += y;
    } else {
      let th2 = atan2(y, x) - spin + D(8);
      v.x += AMOUNT * modulus * cos(th2);
      v.y -= AMOUNT * modulus * sin(th2);
    }
  }
} else {
  let th = atan2(y, x) + TWO_PI;
  let diff = (th + D(7)) % D(6);
  let r = AMOUNT * sqrt(2.0) * D(5) / sin(PI - diff - D(4));
  inside = modulus < r;
  if (inside) {
    let th1 = atan2(y, x) + spin + TWO_PI;
    x = AMOUNT * modulus * cos(th1);
    y = AMOUNT * modulus * sin(th1);
    let diff1 = (th1 + D(7)) % D(6);
    let r1 = AMOUNT * sqrt(2.0) * D(5) / sin(PI - diff1 - D(4));
    modulus = sqrt(x * x + y * y);
    if (modulus < r1) {
      v.x += x;
      v.y += y;
    } else {
      let th2 = atan2(y, x) - spin + D(8) + TWO_PI;
      v.x += AMOUNT * modulus * cos(th2);
      v.y -= AMOUNT * modulus * sin(th2);
    }
  }
}
if (!inside) {
  let m = 1.0 + space / modulus;
  v.x += AMOUNT * m * x;
  v.y += AMOUNT * m * y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "lazysensen",
        params: &[ParamDef::new("scale_x", 1.0), ParamDef::new("scale_y", 1.0), ParamDef::new("scale_z", 0.0)],
        kind: VarKind::Normal,
        is_3d: true,
        // flips the coordinates of the affine point itself, as in JWildfire
        apply: |d, _, a, v, amount| {
            let flip = |c: &mut f64, sc: f64| {
                if sc != 0.0 {
                    let nr = (*c * sc).floor() as i32 as f64;
                    if (nr >= 0.0 && nr % 2.0 == 1.0) || (nr < 0.0 && nr % 2.0 == 0.0) {
                        *c = -*c;
                    }
                }
            };
            flip(&mut a.x, d[0]);
            flip(&mut a.y, d[1]);
            flip(&mut a.z, d[2]);
            v.x += amount * a.x;
            v.y += amount * a.y;
            v.z += amount * a.z;
        },
        prepare: None,
        wgsl: "fn lazysensen_flip(c: f32, sc: f32) -> f32 {
  if (sc == 0.0) { return c; }
  let nr = f32(i32(floor(c * sc)));
  if ((nr >= 0.0 && nr % 2.0 == 1.0) || (nr < 0.0 && nr % 2.0 == 0.0)) { return -c; }
  return c;
}
@body
a.x = lazysensen_flip(a.x, D(0));
a.y = lazysensen_flip(a.y, D(1));
a.z = lazysensen_flip(a.z, D(2));
v.x += AMOUNT * a.x;
v.y += AMOUNT * a.y;
v.z += AMOUNT * a.z;",
    },
    VarDef {
        name: "lazyTravis",
        params: &[ParamDef::new("spin_in", 1.0), ParamDef::new("spin_out", 0.5), ParamDef::new("space", M_PI / 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x, y, space) = (a.x.abs(), a.y.abs(), d[2]);
            let outside = x > amount || y > amount;
            let spin = 4.0 * if outside { d[1] } else { d[0] };
            let (s, p) = if x > y {
                (x, if a.x > 0.0 { x + a.y + x * spin } else { 5.0 * x - a.y + x * spin })
            } else {
                (y, if a.y > 0.0 { 3.0 * y - a.x + y * spin } else { 7.0 * y + a.x + y * spin })
            };
            let p = fmod(p, s * 8.0);
            if outside {
                let (x2, y2) = if p <= 2.0 * s {
                    let y2 = -(s - p);
                    (s + space, y2 + y2 / s * space)
                } else if p <= 4.0 * s {
                    let x2 = 3.0 * s - p;
                    (x2 + x2 / s * space, s + space)
                } else if p <= 6.0 * s {
                    let y2 = 5.0 * s - p;
                    (-(s + space), y2 + y2 / s * space)
                } else {
                    let x2 = -(7.0 * s - p);
                    (x2 + x2 / s * space, -(s + space))
                };
                v.x += amount * x2;
                v.y += amount * y2;
            } else if p <= 2.0 * s {
                v.x += amount * s;
                v.y -= amount * (s - p);
            } else if p <= 4.0 * s {
                v.x += amount * (3.0 * s - p);
                v.y += amount * s;
            } else if p <= 6.0 * s {
                v.x -= amount * s;
                v.y += amount * (5.0 * s - p);
            } else {
                v.x -= amount * (7.0 * s - p);
                v.y -= amount * s;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = abs(a.x);
let y = abs(a.y);
let space = D(2);
let outside = x > AMOUNT || y > AMOUNT;
var spin = 4.0 * D(0);
if (outside) { spin = 4.0 * D(1); }
var s = y;
var p = 0.0;
if (x > y) {
  s = x;
  if (a.x > 0.0) { p = x + a.y + x * spin; } else { p = 5.0 * x - a.y + x * spin; }
} else {
  if (a.y > 0.0) { p = 3.0 * y - a.x + y * spin; } else { p = 7.0 * y + a.x + y * spin; }
}
p = p % (s * 8.0);
var q = vec2<f32>(0.0);
if (outside) {
  if (p <= 2.0 * s) {
    let y2 = -(s - p);
    q = vec2<f32>(s + space, y2 + y2 / s * space);
  } else if (p <= 4.0 * s) {
    let x2 = 3.0 * s - p;
    q = vec2<f32>(x2 + x2 / s * space, s + space);
  } else if (p <= 6.0 * s) {
    let y2 = 5.0 * s - p;
    q = vec2<f32>(-(s + space), y2 + y2 / s * space);
  } else {
    let x2 = -(7.0 * s - p);
    q = vec2<f32>(x2 + x2 / s * space, -(s + space));
  }
} else if (p <= 2.0 * s) {
  q = vec2<f32>(s, -(s - p));
} else if (p <= 4.0 * s) {
  q = vec2<f32>(3.0 * s - p, s);
} else if (p <= 6.0 * s) {
  q = vec2<f32>(-s, 5.0 * s - p);
} else {
  q = vec2<f32>(-(7.0 * s - p), -s);
}
v.x += AMOUNT * q.x;
v.y += AMOUNT * q.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "leather",
        params: &[ParamDef::new("scale", 2.0), ParamDef::new("rough", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sc = java_max(d[0].abs(), 0.01);
            let coarse = d[1] * (sc * a.x).sin() * (sc * a.y).cos();
            let fine = d[1] * 0.3 * (sc * 3.0 * a.y).sin() * (sc * 3.0 * a.x).cos();
            v.x += amount * (a.x + coarse);
            v.y += amount * (a.y + fine);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sc = max(abs(D(0)), 0.01);
v.x += AMOUNT * (a.x + D(1) * sin(sc * a.x) * cos(sc * a.y));
v.y += AMOUNT * (a.y + D(1) * 0.3 * sin(sc * 3.0 * a.y) * cos(sc * 3.0 * a.x));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "line",
        params: &[ParamDef::new("delta", 0.0), ParamDef::new("phi", 0.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| {
            let (dl, ph) = (d[0] * M_PI, d[1] * M_PI);
            let (ux, uy, uz) = (dl.cos() * ph.cos(), dl.sin() * ph.cos(), ph.sin());
            let r = (ux * ux + uy * uy + uz * uz).sqrt();
            let rand = ctx.random() * amount;
            v.x += ux / r * rand;
            v.y += uy / r * rand;
            v.z += uz / r * rand;
        },
        prepare: None,
        wgsl: "let dl = D(0) * PI;
let ph = D(1) * PI;
let u = normalize(vec3<f32>(cos(dl) * cos(ph), sin(dl) * cos(ph), sin(ph)));
let rand = rnd() * AMOUNT;
v.x += u.x * rand;
v.y += u.y * rand;
v.z += u.z * rand;",
    },
    VarDef {
        name: "linearT3D",
        params: &[ParamDef::new("powX", 1.35), ParamDef::new("powY", 0.85), ParamDef::new("powZ", 1.15)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let sgn = |x: f64| if x > 0.0 { 1.0 } else { -1.0 };
            v.x += sgn(a.x) * a.x.abs().powf(d[0]) * amount;
            v.y += sgn(a.y) * a.y.abs().powf(d[1]) * amount;
            v.z += sgn(a.z) * a.z.abs().powf(d[2]) * amount;
        },
        prepare: None,
        wgsl: "let p = vec3<f32>(a.x, a.y, a.z);
let sg = select(vec3<f32>(-1.0), vec3<f32>(1.0), p > vec3<f32>(0.0));
v.x += sg.x * java_pow(abs(p.x), D(0)) * AMOUNT;
v.y += sg.y * java_pow(abs(p.y), D(1)) * AMOUNT;
v.z += sg.z * java_pow(abs(p.z), D(2)) * AMOUNT;",
    },
    VarDef {
        name: "liquefaction",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("amp", 0.3), ParamDef::new("phase", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * (a.x + d[1] * (d[0] * a.y + d[2]).sin());
            v.y += amount * (a.y + d[1] * (d[0] * a.x + d[2]).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * (a.x + D(1) * sin(D(0) * a.y + D(2)));
v.y += AMOUNT * (a.y + D(1) * sin(D(0) * a.x + D(2)));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "lituus_var",
        params: &[ParamDef::new("a", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let theta = a.y.atan2(a.x);
            let nr = java_max((d[0] * d[0]) / java_max(theta.abs(), 1e-4), 0.0).sqrt();
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let theta = atan2(a.y, a.x);
let nr = sqrt(max((D(0) * D(0)) / max(abs(theta), 1e-4), 0.0));
v.x += AMOUNT * nr * cos(theta);
v.y += AMOUNT * nr * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "log_apo",
        params: &[ParamDef::new("base", 2.71828182845905)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * (a.x * a.x + a.y * a.y).ln() * d[1];
            v.y += amount * a.y.atan2(a.x);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], 0.5 / p[0].ln()]),
        wgsl: "v.x += AMOUNT * log(a.x * a.x + a.y * a.y) * D(1);
v.y += AMOUNT * atan2(a.y, a.x);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "log_db",
        params: &[ParamDef::new("base", 1.0), ParamDef::new("fix_period", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [base, fix_period, denom (without amount), fixpe]
        apply: |d, ctx, a, v, amount| {
            let mut fix = 0.0;
            for _ in 0..7 {
                let adp = ctx.random_int(10) as i32 - 5;
                if adp.abs() < 3 {
                    fix += adp as f64;
                }
            }
            v.x += d[2] * amount * a.sumsq().ln();
            v.y += amount * (a.atan_yx() + fix * d[3]);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let denom = if p[0] > 1e-20 { 0.5 / (2.71828182845905 * p[0]).ln() } else { 0.5 };
            let fixpe = if p[1] > 1e-20 { M_PI * p[1] } else { M_PI };
            vec![p[0], p[1], denom, fixpe]
        }),
        wgsl: "var fix = 0.0;
for (var i = 0; i < 7; i++) {
  let adp = min(i32(rnd() * 10.0), 9) - 5;
  if (abs(adp) < 3) { fix += f32(adp); }
}
v.x += D(2) * AMOUNT * log(sumsq(a));
v.y += AMOUNT * (atan_yx(a) + fix * D(3));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "log_tile2",
        params: &[ParamDef::new("spreadx", 2.0), ParamDef::new("spready", 2.0), ParamDef::new("spreadz", 0.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            let sx = if ctx.random() < 0.5 { d[0] } else { -d[0] };
            let sy = if ctx.random() < 0.5 { d[1] } else { -d[1] };
            let sz = if ctx.random() < 0.5 { d[2] } else { -d[2] };
            v.x += amount * (a.x + sx * java_round(ctx.random().ln()));
            v.y += amount * (a.y + sy * java_round(ctx.random().ln()));
            v.z += amount * (a.z + sz * java_round(ctx.random().ln()));
        },
        prepare: None,
        wgsl: "let sx = select(-D(0), D(0), rnd() < 0.5);
let sy = select(-D(1), D(1), rnd() < 0.5);
let sz = select(-D(2), D(2), rnd() < 0.5);
v.x += AMOUNT * (a.x + sx * java_round(log(rnd())));
v.y += AMOUNT * (a.y + sy * java_round(log(rnd())));
v.z += AMOUNT * (a.z + sz * java_round(log(rnd())));",
    },
    VarDef {
        name: "loonie2",
        params: &[ParamDef::int("sides", 4.0, 1.0, 50.0), ParamDef::range("star", 0.15, -1.0, 1.0), ParamDef::range("circle", 0.25, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [sides, star, circle, sina, cosa, sins, coss, sinc, cosc]
        apply: |d, ctx, a, v, amount| {
            let (sina, cosa, sins, coss, sinc, cosc) = (d[3], d[4], d[5], d[6], d[7], d[8]);
            let sqrvvar = amount * amount;
            let (mut xrt, mut yrt) = (a.x, a.y);
            let mut r2 = xrt * coss + yrt.abs() * sins;
            let circle = (xrt * xrt + yrt * yrt).sqrt();
            let mut i = 0;
            while i < d[0] as i32 - 1 {
                (xrt, yrt) = (xrt * cosa - yrt * sina, xrt * sina + yrt * cosa);
                r2 = java_max(r2, xrt * coss + yrt.abs() * sins);
                i += 1;
            }
            r2 = r2 * cosc + circle * sinc;
            r2 = if i > 1 { r2 * r2 } else { r2.abs() * r2 };
            let r = if r2 > 0.0 && r2 < sqrvvar {
                amount * (sqrvvar / r2 - 1.0).abs().sqrt()
            } else if r2 < 0.0 {
                amount / ((sqrvvar / r2).abs() - 1.0).sqrt()
            } else {
                amount
            };
            v.x += r * a.x;
            v.y += r * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let sides = limit_value(ftoi(p[0]), 1, 50) as f64;
            let (star, circle) = (limit_value(p[1], -1.0, 1.0), limit_value(p[2], -1.0, 1.0));
            let (sa, ss, sc) = (M_2PI / sides, -M_PI_2 * star, M_PI_2 * circle);
            vec![sides, star, circle, sa.sin(), sa.cos(), ss.sin(), ss.cos(), sc.sin(), sc.cos()]
        }),
        wgsl: "let sqrvvar = AMOUNT * AMOUNT;
var xrt = a.x;
var yrt = a.y;
var r2 = xrt * D(6) + abs(yrt) * D(5);
let circle = sqrt(xrt * xrt + yrt * yrt);
var i = 0;
let n = i32(D(0)) - 1;
loop {
  if (i >= n) { break; }
  let swp = xrt * D(4) - yrt * D(3);
  yrt = xrt * D(3) + yrt * D(4);
  xrt = swp;
  r2 = max(r2, xrt * D(6) + abs(yrt) * D(5));
  i++;
}
r2 = r2 * D(8) + circle * D(7);
if (i > 1) { r2 = r2 * r2; } else { r2 = abs(r2) * r2; }
var r = AMOUNT;
if (r2 > 0.0 && r2 < sqrvvar) { r = AMOUNT * sqrt(abs(sqrvvar / r2 - 1.0)); }
else if (r2 < 0.0) { r = AMOUNT / sqrt(abs(sqrvvar / r2) - 1.0); }
v.x += r * a.x;
v.y += r * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "loonie3",
        false,
        |_, ctx, a, v, amount| {
            let sqrvvar = amount * amount;
            let mut r2 = 2.0 * sqrvvar;
            if a.x > SMALL_EPSILON {
                let t = (a.x * a.x + a.y * a.y) / a.x;
                r2 = t * t;
            }
            let r = if r2 < sqrvvar { amount * (sqrvvar / r2 - 1.0).sqrt() } else { amount };
            v.x += r * a.x;
            v.y += r * a.y;
            ctx.pass_z(a, v, amount);
        },
        "let sqrvvar = AMOUNT * AMOUNT;
var r2 = 2.0 * sqrvvar;
if (a.x > 0.0) {
  let t = (a.x * a.x + a.y * a.y) / a.x;
  r2 = t * t;
}
var r = AMOUNT;
if (r2 < sqrvvar) { r = AMOUNT * sqrt(sqrvvar / r2 - 1.0); }
v.x += r * a.x;
v.y += r * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "loonie_3D",
        true,
        |_, _, a, v, amount| {
            let sqrvvar = amount * amount;
            let ez = if a.z == 0.0 { a.y.atan2(a.x) } else { a.z };
            let r2 = a.x * a.x + a.y * a.y + ez * ez;
            if r2 < EPSILON {
                return;
            }
            let r = if r2 < sqrvvar { amount * (sqrvvar / r2 - 1.0).sqrt() } else { amount };
            v.x += r * a.x;
            v.y += r * a.y;
            v.z += r * ez * 0.5;
        },
        "var ez = a.z;
if (ez == 0.0) { ez = atan2(a.y, a.x); }
let r2 = a.x * a.x + a.y * a.y + ez * ez;
if (r2 >= EPSILON) {
  let sqrvvar = AMOUNT * AMOUNT;
  var r = AMOUNT;
  if (r2 < sqrvvar) { r = AMOUNT * sqrt(sqrvvar / r2 - 1.0); }
  v.x += r * a.x;
  v.y += r * a.y;
  v.z += r * ez * 0.5;
}",
    ),
    VarDef {
        name: "loq",
        params: &[ParamDef::new("base", std::f64::consts::E)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let abs_v = a.y.hypot(a.z);
            let c = amount * abs_v.atan2(a.x) / abs_v;
            v.x += (a.x * a.x + abs_v * abs_v).ln() * (0.5 * amount / d[0].ln());
            v.y += c * a.y;
            v.z += c * a.z;
        },
        prepare: None,
        wgsl: "let abs_v = length(vec2<f32>(a.y, a.z));
let c = AMOUNT * atan2(abs_v, a.x) / abs_v;
v.x += log(a.x * a.x + abs_v * abs_v) * (0.5 * AMOUNT / log(D(0)));
v.y += c * a.y;
v.z += c * a.z;",
    },
    VarDef {
        name: "lorenz_js",
        params: &[
            ParamDef::new("a", 10.0),
            ParamDef::new("b", 28.0),
            ParamDef::new("c", 1.66),
            ParamDef::new("h", 0.00001),
            ParamDef::new("centerx", 0.0),
            ParamDef::new("centery", 0.0),
            ParamDef::new("scale", 1000.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let (pa, pb, pc, h) = (d[0], d[1], d[2], d[3]);
            v.x += (a.x + h * pa * (a.y - a.x)) * amount;
            v.y += (a.y + h * (a.x * (pb - a.z) - a.y)) * amount;
            v.z += (a.z + h * (a.x * a.y - pc * a.z)) * amount;
            let bdcs = 1.0 / if d[6] == 0.0 { 10E-6 } else { d[6] };
            v.color = fmod((bdcs * ((v.x + d[4]) * (v.x + d[4]) + (v.y + d[5]) * (v.y + d[5]))).abs(), 1.0);
        },
        prepare: None,
        wgsl: "let h = D(3);
v.x += (a.x + h * D(0) * (a.y - a.x)) * AMOUNT;
v.y += (a.y + h * (a.x * (D(1) - a.z) - a.y)) * AMOUNT;
v.z += (a.z + h * (a.x * a.y - D(2) * a.z)) * AMOUNT;
var sc = D(6);
if (sc == 0.0) { sc = 10E-6; }
v.color = abs((1.0 / sc) * ((v.x + D(4)) * (v.x + D(4)) + (v.y + D(5)) * (v.y + D(5)))) % 1.0;",
    },
    VarDef {
        name: "lozi",
        params: &[ParamDef::new("a", 0.5), ParamDef::new("b", 1.0), ParamDef::new("c", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += (d[2] - (d[0] * a.x.abs()) + a.y) * amount;
            v.y += d[1] * a.x * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += (D(2) - (D(0) * abs(a.x)) + a.y) * AMOUNT;
v.y += D(1) * a.x * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "macmillan",
        params: &[ParamDef::new("a", 1.6), ParamDef::new("b", 0.4), ParamDef::new("startx", 0.1), ParamDef::new("starty", 0.1)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            // Java follows one endless orbit per instance; here one per render thread.
            let (pa, pb) = (d[0], d[1]);
            let st = ctx.orbits.entry(d.as_ptr() as usize).or_insert([d[2], d[3]]);
            let (mut xa, mut y) = (st[0], st[1]);
            let mut x = y;
            y = -xa + 2.0 * pa * (y / (1.0 + y * y)) + pb * y;
            xa = x;
            x = y;
            y = -xa + 2.0 * pa * (y / (1.0 + y * y)) + pb * y;
            *st = [x, y];
            v.x += amount * x;
            v.y += amount * y;
        },
        prepare: None,
        // GPU: the orbit is kept per shader invocation; it starts at a slightly perturbed start
        // point after a random number of warm-up steps.
        wgsl: "var<private> macmillan_st: vec3<f32>;
@body
if (macmillan_st.z == 0.0) {
  var xa = D(2) + 1.0e-2 * (rnd() - 0.5);
  var y = D(3) + 1.0e-2 * (rnd() - 0.5);
  let n = min(i32(rnd() * 65536.0), 65535);
  for (var i = 0; i < n; i++) {
    let x = y;
    y = -xa + 2.0 * D(0) * (y / (1.0 + y * y)) + D(1) * y;
    xa = x;
  }
  macmillan_st = vec3<f32>(xa, y, 1.0);
}
var xa = macmillan_st.x;
var y = macmillan_st.y;
var x = y;
y = -xa + 2.0 * D(0) * (y / (1.0 + y * y)) + D(1) * y;
xa = x;
x = y;
y = -xa + 2.0 * D(0) * (y / (1.0 + y * y)) + D(1) * y;
macmillan_st = vec3<f32>(x, y, 1.0);
v.x += AMOUNT * x;
v.y += AMOUNT * y;",
    },
    VarDef {
        name: "mandala",
        params: &[
            ParamDef::int("width", 300.0, 100.0, 1000.0),
            ParamDef::range("size", 0.6, 0.0, 1.0),
            ParamDef::int("num", 1.0, 1.0, 15.0),
            ParamDef::int("denom", 3.0, 3.0, 16.0),
            ParamDef::int("minsky", 0.0, 0.0, 1.0),
            ParamDef::int("wobble", 0.0, 0.0, 8.0),
            ParamDef::int("wrap_range", 0.0, 0.0, 8.0),
            ParamDef::int("hskew", 0.0, 0.0, 22.0),
            ParamDef::int("color id", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let n = d[9] as usize;
            if n == 0 {
                return;
            }
            let k = 10 + 3 * ((ctx.random() * n as f64) as usize).min(n - 1);
            let (ox, oy) = super::draw::ngon_point(ctx, 4, 0.0);
            let (size, half) = (d[1], ((d[0] as i32) / 2) as f64);
            v.x += 0.01 * amount * (ox * size + d[k] - half);
            v.y += 0.01 * amount * (oy * size + d[k + 1] - half);
            ctx.pass_z(a, v, amount);
            if d[8] as i32 == 0 {
                let c = d[k + 2] as i32;
                v.rgb_color = true;
                v.red = (c >> 16 & 255) as f64;
                v.green = (c >> 8 & 255) as f64;
                v.blue = (c & 255) as f64;
            } else {
                v.color = d[k + 2];
            }
        },
        prepare: Some(|p, _| mandala_prepare(p)),
        wgsl: "let n = u32(D(9));
if (n > 0u) {
  let k = base + 10u + 3u * min(u32(rnd() * f32(n)), n - 1u);
  let o = res_ngon_point(4, 0.0);
  let half = f32(i32(D(0)) / 2);
  v.x += 0.01 * AMOUNT * (o.x * D(1) + prm[k] - half);
  v.y += 0.01 * AMOUNT * (o.y * D(1) + prm[k + 1u] - half);
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
  if (i32(D(8)) == 0) {
    let c = u32(prm[k + 2u]);
    v.rgbc = 1.0;
    v.r = f32((c >> 16u) & 255u);
    v.g = f32((c >> 8u) & 255u);
    v.b = f32(c & 255u);
  } else {
    v.color = prm[k + 2u];
  }
}",
    },
    VarDef {
        name: "mandala2",
        params: &[
            ParamDef::int("width", 500.0, 100.0, 2000.0),
            ParamDef::int("num", 1.0, 1.0, 15.0),
            ParamDef::int("denom", 3.0, 3.0, 16.0),
            ParamDef::int("minsky", 0.0, 0.0, 1.0),
            ParamDef::int("wobble", 0.0, 0.0, 8.0),
            ParamDef::int("wrap_range", 0.0, 0.0, 8.0),
            ParamDef::int("hskew", 0.0, 0.0, 22.0),
            ParamDef::int("color id", 0.0, 0.0, 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let w = d[0];
            let x = (ctx.random() * w) as i32;
            let y = (ctx.random() * w) as i32;
            let c = d[8 + (x * w as i32 + y) as usize];
            let colorid = d[7] as i32;
            if (1..=3).contains(&colorid) {
                let c = c as i32;
                v.rgb_color = true;
                v.red = (c >> 16 & 255) as f64;
                v.green = (c >> 8 & 255) as f64;
                v.blue = (c & 255) as f64;
            } else {
                v.color = c;
            }
            v.x += amount * (x as f64 / w - 0.5);
            v.y += amount * (y as f64 / w - 0.5);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| mandala2_prepare(p)),
        wgsl: "let w = D(0);
let x = min(trunc(rnd() * w), w - 1.0);
let y = min(trunc(rnd() * w), w - 1.0);
let c = prm[base + 8u + u32(x * w + y)];
let colorid = i32(D(7));
if (colorid >= 1 && colorid <= 3) {
  let ci = u32(c);
  v.rgbc = 1.0;
  v.r = f32((ci >> 16u) & 255u);
  v.g = f32((ci >> 8u) & 255u);
  v.b = f32(ci & 255u);
} else {
  v.color = c;
}
v.x += AMOUNT * (x / w - 0.5);
v.y += AMOUNT * (y / w - 0.5);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];
