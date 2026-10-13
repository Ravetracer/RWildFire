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

//! Variations ported from the JWildfire sources (batch B).

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

/// `sqrt(x)` or 0 for (nearly) non-positive input (EllipticFunc.sqrt_safe).
fn elliptic_sqrt_safe(x: f64) -> f64 {
    if x < SMALL_EPSILON { 0.0 } else { x.sqrt() }
}

/// `sqrt(1 + x) - 1` with a Pade approximant near 0 (EllipticFunc.sqrt1pm1).
fn elliptic_sqrt1pm1(x: f64) -> f64 {
    if -0.0625 < x && x < 0.0625 {
        let num = (((x / 32.0 + 5.0 / 16.0) * x + 3.0 / 4.0) * x + 1.0 / 2.0) * x;
        let den = (((x / 256.0 + 5.0 / 32.0) * x + 15.0 / 16.0) * x + 7.0 / 4.0) * x + 1.0;
        return num / den;
    }
    (1.0 + x).sqrt() - 1.0
}

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "pie",
        params: &[
            ParamDef::range("slices", 6.0, 1.0, 50.0),
            ParamDef::range("rotation", 0.0, -M_PI, M_PI),
            ParamDef::range("thickness", 0.5, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sl = (ctx.random() * d[0] + 0.5) as i32 as f64;
            let ang = d[1] + M_2PI * (sl + ctx.random() * d[2]) / d[0];
            let r = amount * ctx.random();
            let (sina, cosa) = ang.sin_cos();
            v.x += r * cosa;
            v.y += r * sina;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sl = trunc(rnd() * D(0) + 0.5);
         let ang = D(1) + TWO_PI * (sl + rnd() * D(2)) / D(0);
         let r = AMOUNT * rnd();
         v.x += r * cos(ang);
         v.y += r * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "ngon",
        params: &[
            ParamDef::range("circle", 1.0, -2.0, 2.0),
            ParamDef::range("corners", 2.0, -4.0, 4.0),
            ParamDef::range("power", 3.0, -6.0, 6.0),
            ParamDef::range("sides", 5.0, 1.0, 12.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [circle, corners, power, sides, power / 2, 2pi / sides]
            let r_factor = a.sumsq().powf(d[4]);
            let theta = a.atan_yx();
            let b = d[5];
            let mut phi = theta - b * (theta / b).floor();
            if phi > b / 2.0 {
                phi -= b;
            }
            let mut amp = d[1] * (1.0 / (phi.cos() + SMALL_EPSILON) - 1.0) + d[0];
            amp /= r_factor + SMALL_EPSILON;
            v.x += amount * a.x * amp;
            v.y += amount * a.y * amp;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], p[3], p[2] / 2.0, 2.0 * M_PI / p[3]]),
        wgsl: "let r_factor = pow(sumsq(a), D(4));
         let theta = atan_yx(a);
         let b = D(5);
         var phi = theta - b * floor(theta / b);
         if (phi > b / 2.0) { phi -= b; }
         var amp = D(1) * (1.0 / (cos(phi) + EPS) - 1.0) + D(0);
         amp /= r_factor + EPS;
         v.x += AMOUNT * a.x * amp;
         v.y += AMOUNT * a.y * amp;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "curl",
        params: &[ParamDef::range("c1", 0.1, -2.0, 2.0), ParamDef::range("c2", 0.0, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let re = 1.0 + d[0] * a.x + d[1] * (sqr(a.x) - sqr(a.y));
            let im = d[0] * a.y + d[1] * 2.0 * a.x * a.y;
            let r = amount / (sqr(re) + sqr(im));
            v.x += (a.x * re + a.y * im) * r;
            v.y += (a.y * re - a.x * im) * r;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let re = 1.0 + D(0) * a.x + D(1) * (a.x * a.x - a.y * a.y);
         let im = D(0) * a.y + D(1) * 2.0 * a.x * a.y;
         let r = AMOUNT / (re * re + im * im);
         v.x += (a.x * re + a.y * im) * r;
         v.y += (a.y * re - a.x * im) * r;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "rectangles",
        params: &[ParamDef::range("x", 0.3, -2.0, 2.0), ParamDef::range("y", 0.2, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            if d[0].abs() < EPSILON {
                v.x += amount * a.x;
            } else {
                v.x += amount * ((2.0 * (a.x / d[0]).floor() + 1.0) * d[0] - a.x);
            }
            if d[1].abs() < EPSILON {
                v.y += amount * a.y;
            } else {
                v.y += amount * ((2.0 * (a.y / d[1]).floor() + 1.0) * d[1] - a.y);
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "if (abs(D(0)) < EPSILON) {
           v.x += AMOUNT * a.x;
         } else {
           v.x += AMOUNT * ((2.0 * floor(a.x / D(0)) + 1.0) * D(0) - a.x);
         }
         if (abs(D(1)) < EPSILON) {
           v.y += AMOUNT * a.y;
         } else {
           v.y += AMOUNT * ((2.0 * floor(a.y / D(1)) + 1.0) * D(1) - a.y);
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "arch",
        false,
        |_, ctx, a, v, amount| {
            let ang = ctx.random() * amount * M_PI;
            let (sinr, cosr) = ang.sin_cos();
            if cosr == 0.0 {
                return;
            }
            v.x += amount * sinr;
            v.y += amount * (sinr * sinr) / cosr;
            ctx.pass_z(a, v, amount);
        },
        "let ang = rnd() * AMOUNT * PI;
         let sinr = sin(ang);
         let cosr = cos(ang);
         if (cosr != 0.0) {
           v.x += AMOUNT * sinr;
           v.y += AMOUNT * (sinr * sinr) / cosr;
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    simple(
        "tangent",
        false,
        |_, ctx, a, v, amount| {
            let d = a.y.cos();
            if d == 0.0 {
                return;
            }
            v.x += amount * a.x.sin() / d;
            v.y += amount * a.y.tan();
            ctx.pass_z(a, v, amount);
        },
        "let dd = cos(a.y);
         if (dd != 0.0) {
           v.x += AMOUNT * sin(a.x) / dd;
           v.y += AMOUNT * tan(a.y);
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    simple(
        "square",
        false,
        |_, ctx, a, v, amount| {
            v.x += amount * (ctx.random() - 0.5);
            v.y += amount * (ctx.random() - 0.5);
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * (rnd() - 0.5);
         v.y += AMOUNT * (rnd() - 0.5);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "rays",
        false,
        |_, ctx, a, v, amount| {
            let ang = amount * ctx.random() * M_PI;
            let r = amount / (a.sumsq() + SMALL_EPSILON);
            let tanr = amount * ang.tan() * r;
            v.x += tanr * a.x.cos();
            v.y += tanr * a.y.sin();
            ctx.pass_z(a, v, amount);
        },
        "let ang = AMOUNT * rnd() * PI;
         let r = AMOUNT / (sumsq(a) + EPS);
         let tanr = AMOUNT * tan(ang) * r;
         v.x += tanr * cos(a.x);
         v.y += tanr * sin(a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "blade",
        false,
        |_, ctx, a, v, amount| {
            let r = ctx.random() * amount * (a.x * a.x + a.y * a.y).sqrt();
            let (sinr, cosr) = r.sin_cos();
            v.x += amount * a.x * (cosr + sinr);
            v.y += amount * a.x * (cosr - sinr);
            ctx.pass_z(a, v, amount);
        },
        "let r = rnd() * AMOUNT * sqrt(a.x * a.x + a.y * a.y);
         let sinr = sin(r);
         let cosr = cos(r);
         v.x += AMOUNT * a.x * (cosr + sinr);
         v.y += AMOUNT * a.x * (cosr - sinr);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "secant2",
        false,
        |_, ctx, a, v, amount| {
            let r = amount * a.rad();
            let cr = r.cos();
            if cr == 0.0 {
                return;
            }
            let icr = 1.0 / cr;
            v.x += amount * a.x;
            if cr < 0.0 {
                v.y += amount * (icr + 1.0);
            } else {
                v.y += amount * (icr - 1.0);
            }
            ctx.pass_z(a, v, amount);
        },
        "let r = AMOUNT * rad(a);
         let cr = cos(r);
         if (cr != 0.0) {
           let icr = 1.0 / cr;
           v.x += AMOUNT * a.x;
           if (cr < 0.0) {
             v.y += AMOUNT * (icr + 1.0);
           } else {
             v.y += AMOUNT * (icr - 1.0);
           }
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    simple(
        "twintrian",
        false,
        |_, ctx, a, v, amount| {
            let r = ctx.random() * amount * a.rad();
            let (sinr, cosr) = r.sin_cos();
            let mut diff = (sinr * sinr).log10() + cosr;
            if diff.abs() < SMALL_EPSILON {
                diff = -30.0;
            }
            v.x += amount * a.x * diff;
            v.y += amount * a.x * (diff - sinr * M_PI);
            ctx.pass_z(a, v, amount);
        },
        "let r = rnd() * AMOUNT * rad(a);
         let sinr = sin(r);
         let cosr = cos(r);
         var diff = log10(sinr * sinr) + cosr;
         if (abs(diff) < EPS) { diff = -30.0; }
         v.x += AMOUNT * a.x * diff;
         v.y += AMOUNT * a.x * (diff - sinr * PI);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "cross",
        false,
        |_, ctx, a, v, amount| {
            let s = a.x * a.x - a.y * a.y;
            let r = amount * (1.0 / (s * s + SMALL_EPSILON)).sqrt();
            v.x += a.x * r;
            v.y += a.y * r;
            ctx.pass_z(a, v, amount);
        },
        "let s = a.x * a.x - a.y * a.y;
         let r = AMOUNT * sqrt(1.0 / (s * s + EPS));
         v.x += a.x * r;
         v.y += a.y * r;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "disc2",
        params: &[ParamDef::range("rot", 2.0, -4.0, 4.0), ParamDef::range("twist", 0.5, -M_2PI, M_2PI)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [rot, twist, timespi, sinadd, cosadd]
            let t = d[2] * (a.x + a.y);
            let (sinr, cosr) = t.sin_cos();
            let r = amount * a.atan_xy() / M_PI;
            v.x += (sinr + d[4]) * r;
            v.y += (cosr + d[3]) * r;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let add = p[1];
            let timespi = p[0] * M_PI;
            let mut sinadd = add.sin();
            let mut cosadd = add.cos() - 1.0;
            if add > 2.0 * M_PI {
                let k = 1.0 + add - 2.0 * M_PI;
                cosadd *= k;
                sinadd *= k;
            }
            if add < -2.0 * M_PI {
                let k = 1.0 + add + 2.0 * M_PI;
                cosadd *= k;
                sinadd *= k;
            }
            vec![p[0], p[1], timespi, sinadd, cosadd]
        }),
        wgsl: "let t = D(2) * (a.x + a.y);
         let r = AMOUNT * atan_xy(a) / PI;
         v.x += (sin(t) + D(4)) * r;
         v.y += (cos(t) + D(3)) * r;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "super_shape",
        params: &[
            ParamDef::range("rnd", 3.0, 0.0, 4.0),
            ParamDef::range("m", 1.0, 0.0, 20.0),
            ParamDef::range("n1", 1.0, -5.0, 5.0),
            ParamDef::range("n2", 1.0, -5.0, 5.0),
            ParamDef::range("n3", 1.0, -5.0, 5.0),
            ParamDef::range("holes", 0.0, -1.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [rnd, m, n1, n2, n3, holes, m / 4, -1 / n1]
            let theta = d[6] * a.atan_yx() + M_PI_4;
            let (st, ct) = theta.sin_cos();
            let t1 = ct.abs().powf(d[3]);
            let t2 = st.abs().powf(d[4]);
            let rr = a.rad();
            let r = amount * ((d[0] * ctx.random() + (1.0 - d[0]) * rr) - d[5]) * (t1 + t2).powf(d[7]) / rr;
            v.x += r * a.x;
            v.y += r * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], p[3], p[4], p[5], p[1] / 4.0, -1.0 / p[2]]),
        wgsl: "let theta = D(6) * atan_yx(a) + PI * 0.25;
         let t1 = pow(abs(cos(theta)), D(3));
         let t2 = pow(abs(sin(theta)), D(4));
         let rr = rad(a);
         let r = AMOUNT * ((D(0) * rnd() + (1.0 - D(0)) * rr) - D(5)) * pow(t1 + t2, D(7)) / rr;
         v.x += r * a.x;
         v.y += r * a.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "flower",
        params: &[ParamDef::range("holes", 0.4, -2.0, 2.0), ParamDef::range("petals", 7.0, 0.0, 20.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let theta = a.atan_yx();
            let dd = a.rad();
            if dd == 0.0 {
                return;
            }
            let r = amount * (ctx.random() - d[0]) * (d[1] * theta).cos() / dd;
            v.x += r * a.x;
            v.y += r * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let theta = atan_yx(a);
         let dd = rad(a);
         if (dd != 0.0) {
           let r = AMOUNT * (rnd() - D(0)) * cos(D(1) * theta) / dd;
           v.x += r * a.x;
           v.y += r * a.y;
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    },
    VarDef {
        name: "conic",
        params: &[ParamDef::range("eccentricity", 1.0, 0.0, 2.0), ParamDef::range("holes", 0.0, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let rr = a.rad();
            let ct = a.x / rr;
            let r = amount * (ctx.random() - d[1]) * d[0] / (1.0 + d[0] * ct) / rr;
            v.x += r * a.x;
            v.y += r * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let rr = rad(a);
         let ct = a.x / rr;
         let r = AMOUNT * (rnd() - D(1)) * D(0) / (1.0 + D(0) * ct) / rr;
         v.x += r * a.x;
         v.y += r * a.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "parabola",
        params: &[ParamDef::range("width", 1.0, -2.0, 2.0), ParamDef::range("height", 0.5, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = a.rad();
            let (sr, cr) = r.sin_cos();
            v.x += d[1] * amount * sr * sr * ctx.random();
            v.y += d[0] * amount * cr * ctx.random();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = rad(a);
         let sr = sin(r);
         let cr = cos(r);
         v.x += D(1) * AMOUNT * sr * sr * rnd();
         v.y += D(0) * AMOUNT * cr * rnd();
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bent2",
        params: &[ParamDef::range("x", 1.0, -2.0, 2.0), ParamDef::range("y", 1.0, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let mut nx = a.x;
            let mut ny = a.y;
            if nx < 0.0 {
                nx *= d[0];
            }
            if ny < 0.0 {
                ny *= d[1];
            }
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let nx = select(a.x, a.x * D(0), a.x < 0.0);
         let ny = select(a.y, a.y * D(1), a.y < 0.0);
         v.x += AMOUNT * nx;
         v.y += AMOUNT * ny;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bipolar",
        params: &[ParamDef::range("shift", 0.0, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let x2y2 = a.x * a.x + a.y * a.y;
            let t = x2y2 + 1.0;
            let x2 = 2.0 * a.x;
            let ps = -M_PI_2 * d[0];
            let mut y = 0.5 * (2.0 * a.y).atan2(x2y2 - 1.0) + ps;
            if y > M_PI_2 {
                y = -M_PI_2 + fmod(y + M_PI_2, M_PI);
            } else if y < -M_PI_2 {
                y = M_PI_2 - fmod(M_PI_2 - y, M_PI);
            }
            let f = t + x2;
            let g = t - x2;
            if g == 0.0 || f / g <= 0.0 {
                return;
            }
            v.x += amount * 0.25 * M_2_PI * ((t + x2) / (t - x2)).ln();
            v.y += amount * M_2_PI * y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x2y2 = a.x * a.x + a.y * a.y;
         let t = x2y2 + 1.0;
         let x2 = 2.0 * a.x;
         let ps = -HALF_PI * D(0);
         var y = 0.5 * atan2(2.0 * a.y, x2y2 - 1.0) + ps;
         if (y > HALF_PI) {
           y = -HALF_PI + fmod(y + HALF_PI, PI);
         } else if (y < -HALF_PI) {
           y = HALF_PI - fmod(HALF_PI - y, PI);
         }
         let f = t + x2;
         let g = t - x2;
         if (g != 0.0 && f / g > 0.0) {
           v.x += AMOUNT * 0.25 * (2.0 / PI) * log(f / g);
           v.y += AMOUNT * (2.0 / PI) * y;
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    },
    simple(
        "boarders",
        false,
        |_, ctx, a, v, amount| {
            let round_x = a.x.round_ties_even();
            let round_y = a.y.round_ties_even();
            let offset_x = a.x - round_x;
            let offset_y = a.y - round_y;
            if ctx.random() >= 0.75 {
                v.x += amount * (offset_x * 0.5 + round_x);
                v.y += amount * (offset_y * 0.5 + round_y);
            } else if offset_x.abs() >= offset_y.abs() {
                if offset_x >= 0.0 {
                    v.x += amount * (offset_x * 0.5 + round_x + 0.25);
                    v.y += amount * (offset_y * 0.5 + round_y + 0.25 * offset_y / offset_x);
                } else {
                    v.x += amount * (offset_x * 0.5 + round_x - 0.25);
                    v.y += amount * (offset_y * 0.5 + round_y - 0.25 * offset_y / offset_x);
                }
            } else if offset_y >= 0.0 {
                v.y += amount * (offset_y * 0.5 + round_y + 0.25);
                v.x += amount * (offset_x * 0.5 + round_x + offset_x / offset_y * 0.25);
            } else {
                v.y += amount * (offset_y * 0.5 + round_y - 0.25);
                v.x += amount * (offset_x * 0.5 + round_x - offset_x / offset_y * 0.25);
            }
            ctx.pass_z(a, v, amount);
        },
        "let round_x = round(a.x);
         let round_y = round(a.y);
         let offset_x = a.x - round_x;
         let offset_y = a.y - round_y;
         if (rnd() >= 0.75) {
           v.x += AMOUNT * (offset_x * 0.5 + round_x);
           v.y += AMOUNT * (offset_y * 0.5 + round_y);
         } else if (abs(offset_x) >= abs(offset_y)) {
           let sx = select(-1.0, 1.0, offset_x >= 0.0);
           v.x += AMOUNT * (offset_x * 0.5 + round_x + sx * 0.25);
           v.y += AMOUNT * (offset_y * 0.5 + round_y + sx * 0.25 * offset_y / offset_x);
         } else {
           let sy = select(-1.0, 1.0, offset_y >= 0.0);
           v.y += AMOUNT * (offset_y * 0.5 + round_y + sy * 0.25);
           v.x += AMOUNT * (offset_x * 0.5 + round_x + sy * offset_x / offset_y * 0.25);
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "butterfly",
        false,
        |_, ctx, a, v, amount| {
            // wx is weight*4/sqrt(3*pi)
            let wx = amount * 1.302_940_031_741_119_8;
            let y2 = a.y * 2.0;
            let r = wx * ((a.y * a.x).abs() / (SMALL_EPSILON + a.x * a.x + y2 * y2)).sqrt();
            v.x += r * a.x;
            v.y += r * y2;
            ctx.pass_z(a, v, amount);
        },
        "let wx = AMOUNT * 1.30294003174112;
         let y2 = a.y * 2.0;
         let r = wx * sqrt(abs(a.y * a.x) / (EPS + a.x * a.x + y2 * y2));
         v.x += r * a.x;
         v.y += r * y2;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "cell",
        params: &[ParamDef::range("size", 0.6, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let size = d[0];
            let inv_cell_size = 1.0 / size;
            // calculate input cell
            let mut x = (a.x * inv_cell_size).floor() as i32;
            let mut y = (a.y * inv_cell_size).floor() as i32;
            // offset from cell origin
            let dx = a.x - x as f64 * size;
            let dy = a.y - y as f64 * size;
            // interleave cells (Java int arithmetic wraps on overflow)
            y = if y >= 0 { y.wrapping_mul(2) } else { y.wrapping_mul(2).wrapping_add(1).wrapping_neg() };
            x = if x >= 0 { x.wrapping_mul(2) } else { x.wrapping_mul(2).wrapping_add(1).wrapping_neg() };
            v.x += amount * (dx + x as f64 * size);
            v.y -= amount * (dy + y as f64 * size);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let size = D(0);
         let inv_cell_size = 1.0 / size;
         var cx = i32(floor(a.x * inv_cell_size));
         var cy = i32(floor(a.y * inv_cell_size));
         let dx = a.x - f32(cx) * size;
         let dy = a.y - f32(cy) * size;
         if (cy >= 0) { cy = cy * 2; } else { cy = -(2 * cy + 1); }
         if (cx >= 0) { cx = cx * 2; } else { cx = -(2 * cx + 1); }
         v.x += AMOUNT * (dx + f32(cx) * size);
         v.y -= AMOUNT * (dy + f32(cy) * size);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cpow",
        params: &[
            ParamDef::range("r", 1.0, -4.0, 4.0),
            ParamDef::range("i", 0.1, -4.0, 4.0),
            ParamDef::range("power", 1.5, -10.0, 10.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [r, i, power, va, vc, vd]
            let ang0 = a.atan_yx();
            let lnr = 0.5 * a.sumsq().ln();
            let ang = d[4] * ang0 + d[5] * lnr + d[3] * (d[2] * ctx.random()).floor();
            let m = amount * (d[4] * lnr - d[5] * ang0).exp();
            let (sa, ca) = ang.sin_cos();
            v.x += m * ca;
            v.y += m * sa;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], 2.0 * M_PI / p[2], p[0] / p[2], p[1] / p[2]]),
        wgsl: "let ang0 = atan_yx(a);
         let lnr = 0.5 * log(sumsq(a));
         let ang = D(4) * ang0 + D(5) * lnr + D(3) * floor(D(2) * rnd());
         let m = AMOUNT * exp(D(4) * lnr - D(5) * ang0);
         v.x += m * cos(ang);
         v.y += m * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "curve",
        params: &[
            ParamDef::range("xamp", 0.25, -2.0, 2.0),
            ParamDef::range("yamp", 0.5, -2.0, 2.0),
            ParamDef::range("xlength", 1.0, 0.0, 4.0),
            ParamDef::range("ylength", 1.0, 0.0, 4.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [xamp, yamp, xlength, ylength, pc_xlen, pc_ylen]
            v.x += amount * (a.x + d[0] * (-a.y * a.y / d[4]).exp());
            v.y += amount * (a.y + d[1] * (-a.x * a.x / d[5]).exp());
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let pc_xlen = (p[2] * p[2]).max(1e-20);
            let pc_ylen = (p[3] * p[3]).max(1e-20);
            vec![p[0], p[1], p[2], p[3], pc_xlen, pc_ylen]
        }),
        wgsl: "v.x += AMOUNT * (a.x + D(0) * exp(-a.y * a.y / D(4)));
         v.y += AMOUNT * (a.y + D(1) * exp(-a.x * a.x / D(5)));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "edisc",
        false,
        |_, ctx, a, v, amount| {
            let tmp = a.sumsq() + 1.0;
            let tmp2 = 2.0 * a.x;
            let r1 = (tmp + tmp2).sqrt();
            let r2 = (tmp - tmp2).sqrt();
            let xmax = (r1 + r2) * 0.5;
            let a1 = (xmax + (xmax - 1.0).sqrt()).ln();
            let a2 = -(a.x / xmax).acos();
            let w = amount / 11.57034632;
            let (mut snv, csv) = a1.sin_cos();
            let snhu = a2.sinh();
            let cshu = a2.cosh();
            if a.y > 0.0 {
                snv = -snv;
            }
            v.x += w * cshu * csv;
            v.y += w * snhu * snv;
            ctx.pass_z(a, v, amount);
        },
        // f32: xmax - 1 and x / xmax are clamped to their mathematical domain to avoid NaN
        // from rounding (xmax >= 1 and |x| <= xmax hold exactly).
        "let tmp = sumsq(a) + 1.0;
         let tmp2 = 2.0 * a.x;
         let r1 = sqrt(tmp + tmp2);
         let r2 = sqrt(tmp - tmp2);
         let xmax = (r1 + r2) * 0.5;
         let a1 = log(xmax + sqrt(max(xmax - 1.0, 0.0)));
         let a2 = -acos(clamp(a.x / xmax, -1.0, 1.0));
         let w = AMOUNT / 11.57034632;
         var snv = sin(a1);
         let csv = cos(a1);
         let snhu = sinh(a2);
         let cshu = cosh(a2);
         if (a.y > 0.0) { snv = -snv; }
         v.x += w * cshu * csv;
         v.y += w * snhu * snv;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "elliptic",
        params: &[ParamDef::int("mode", 1.0, 0.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let vv = amount * M_2_PI;
            let mode = d[0] as i32;
            if mode == 2 {
                let sq = a.y * a.y + a.x * a.x;
                let x2 = 2.0 * a.x;
                let xmaxm1 = 0.5 * (elliptic_sqrt1pm1(sq + x2) + elliptic_sqrt1pm1(sq - x2));
                let ssx = if xmaxm1 < 0.0 { 0.0 } else { xmaxm1.sqrt() };
                let q = a.x / (1.0 + xmaxm1);
                let sign = if a.y > 0.0 { 1.0 } else { -1.0 };
                v.x += vv * q.clamp(-1.0, 1.0).asin();
                v.y += sign * vv * (xmaxm1 + ssx).ln_1p();
            } else {
                let tmp = a.y * a.y + a.x * a.x + 1.0;
                let x2 = 2.0 * a.x;
                let xmax = 0.5 * ((tmp + x2).sqrt() + (tmp - x2).sqrt());
                let q = a.x / xmax;
                let b = elliptic_sqrt_safe(1.0 - q * q);
                let mut sign = if a.y > 0.0 { 1.0 } else { -1.0 };
                if mode == 1 {
                    sign = if ctx.random() < 0.5 { 1.0 } else { -1.0 };
                }
                v.x += vv * q.atan2(b);
                v.y += sign * vv * (xmax + elliptic_sqrt_safe(xmax - 1.0)).ln();
            }
            ctx.pass_z(a, v, amount);
        },
        // mode is rounded (Tools.FTOI) and limited to 0..2
        prepare: Some(|p, _| vec![p[0].round().clamp(0.0, 2.0)]),
        wgsl: "let vv = AMOUNT * (2.0 / PI);
         if (D(0) == 2.0) {
           let sq = a.y * a.y + a.x * a.x;
           let x2 = 2.0 * a.x;
           let u1 = sq + x2;
           var s1 = sqrt(1.0 + u1) - 1.0;
           if (-0.0625 < u1 && u1 < 0.0625) {
             s1 = (((u1 / 32.0 + 5.0 / 16.0) * u1 + 0.75) * u1 + 0.5) * u1
                / ((((u1 / 256.0 + 5.0 / 32.0) * u1 + 15.0 / 16.0) * u1 + 1.75) * u1 + 1.0);
           }
           let u2 = sq - x2;
           var s2 = sqrt(1.0 + u2) - 1.0;
           if (-0.0625 < u2 && u2 < 0.0625) {
             s2 = (((u2 / 32.0 + 5.0 / 16.0) * u2 + 0.75) * u2 + 0.5) * u2
                / ((((u2 / 256.0 + 5.0 / 32.0) * u2 + 15.0 / 16.0) * u2 + 1.75) * u2 + 1.0);
           }
           let xmaxm1 = 0.5 * (s1 + s2);
           let ssx = select(sqrt(xmaxm1), 0.0, xmaxm1 < 0.0);
           let q = a.x / (1.0 + xmaxm1);
           let sg = select(-1.0, 1.0, a.y > 0.0);
           v.x += vv * asin(clamp(q, -1.0, 1.0));
           v.y += sg * vv * log(1.0 + xmaxm1 + ssx);
         } else {
           let tmp = a.y * a.y + a.x * a.x + 1.0;
           let x2 = 2.0 * a.x;
           let xmax = 0.5 * (sqrt(tmp + x2) + sqrt(tmp - x2));
           let q = a.x / xmax;
           let w1 = 1.0 - q * q;
           let b = select(sqrt(w1), 0.0, w1 < EPS);
           var sg = select(-1.0, 1.0, a.y > 0.0);
           if (D(0) == 1.0) { sg = select(-1.0, 1.0, rnd() < 0.5); }
           let w2 = xmax - 1.0;
           v.x += vv * atan2(q, b);
           v.y += sg * vv * log(xmax + select(sqrt(w2), 0.0, w2 < EPS));
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "escher",
        params: &[ParamDef::range("beta", 0.3, -M_PI, M_PI)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [beta, vc, vd]
            let ang = a.atan_yx();
            let lnr = 0.5 * a.sumsq().ln();
            let m = amount * (d[1] * lnr - d[2] * ang).exp();
            let n = d[1] * ang + d[2] * lnr;
            let (sn, cn) = n.sin_cos();
            v.x += m * cn;
            v.y += m * sn;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let (seb, ceb) = p[0].sin_cos();
            vec![p[0], 0.5 * (1.0 + ceb), 0.5 * seb]
        }),
        wgsl: "let ang = atan_yx(a);
         let lnr = 0.5 * log(sumsq(a));
         let m = AMOUNT * exp(D(1) * lnr - D(2) * ang);
         let n = D(1) * ang + D(2) * lnr;
         v.x += m * cos(n);
         v.y += m * sin(n);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "foci",
        false,
        |_, ctx, a, v, amount| {
            let expx = a.x.exp() * 0.5;
            let expnx = 0.25 / expx;
            if expx <= SMALL_EPSILON || expnx <= SMALL_EPSILON {
                return;
            }
            let (siny, cosy) = a.y.sin_cos();
            let mut tmp = expx + expnx - cosy;
            if tmp == 0.0 {
                return;
            }
            tmp = amount / tmp;
            v.x += (expx - expnx) * tmp;
            v.y += siny * tmp;
            ctx.pass_z(a, v, amount);
        },
        "let expx = exp(a.x) * 0.5;
         let expnx = 0.25 / expx;
         if (expx > EPS && expnx > EPS) {
           let t0 = expx + expnx - cos(a.y);
           if (t0 != 0.0) {
             let tmp = AMOUNT / t0;
             v.x += (expx - expnx) * tmp;
             v.y += sin(a.y) * tmp;
             if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
           }
         }",
    ),
    VarDef {
        name: "lazysusan",
        params: &[
            ParamDef::range("space", 0.4, -2.0, 2.0),
            ParamDef::range("twist", 0.2, -2.0, 2.0),
            ParamDef::range("spin", 0.1, -M_PI, M_PI),
            ParamDef::range("x", 0.1, -2.0, 2.0),
            ParamDef::range("y", 0.2, -2.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [space, twist, spin, x, y]
            let xx = a.x - d[3];
            let yy = a.y + d[4];
            let mut rr = (xx * xx + yy * yy).sqrt();
            if rr < amount {
                let ang = yy.atan2(xx) + d[2] + d[1] * (amount - rr);
                let (sina, cosa) = ang.sin_cos();
                rr *= amount;
                v.x += rr * cosa + d[3];
                v.y += rr * sina - d[4];
            } else {
                rr = amount * (1.0 + d[0] / rr);
                v.x += rr * xx + d[3];
                v.y += rr * yy - d[4];
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let xx = a.x - D(3);
         let yy = a.y + D(4);
         let rr = sqrt(xx * xx + yy * yy);
         if (rr < AMOUNT) {
           let ang = atan2(yy, xx) + D(2) + D(1) * (AMOUNT - rr);
           let r = AMOUNT * rr;
           v.x += r * cos(ang) + D(3);
           v.y += r * sin(ang) - D(4);
         } else {
           let r = AMOUNT * (1.0 + D(0) / rr);
           v.x += r * xx + D(3);
           v.y += r * yy - D(4);
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "loonie",
        false,
        |_, ctx, a, v, amount| {
            let r2 = a.sumsq();
            let w2 = amount * amount;
            if r2 < w2 && r2 != 0.0 {
                let r = amount * (w2 / r2 - 1.0).sqrt();
                v.x += r * a.x;
                v.y += r * a.y;
            } else {
                v.x += amount * a.x;
                v.y += amount * a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        "let r2 = sumsq(a);
         let w2 = AMOUNT * AMOUNT;
         if (r2 < w2 && r2 != 0.0) {
           let r = AMOUNT * sqrt(w2 / r2 - 1.0);
           v.x += r * a.x;
           v.y += r * a.y;
         } else {
           v.x += AMOUNT * a.x;
           v.y += AMOUNT * a.y;
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "pie", "ngon", "curl", "rectangles", "arch", "tangent", "square", "rays", "blade", "secant2", "twintrian",
        "cross", "disc2", "super_shape", "flower", "conic", "parabola", "bent2", "bipolar", "boarders", "butterfly",
        "cell", "cpow", "curve", "edisc", "elliptic", "escher", "foci", "lazysusan", "loonie",
    ];

    #[test]
    fn all_variations_present() {
        for n in NAMES {
            assert!(DEFS.iter().any(|d| d.name == *n), "missing {n}");
        }
        assert_eq!(DEFS.len(), NAMES.len());
    }

    #[test]
    fn finite_on_regular_points() {
        let coefs = Affine::default();
        let mut pts = Ctx::new(12345, false);
        for def in DEFS {
            let data = def.prepare_data(&[], &coefs);
            for preserve_z in [false, true] {
                let mut ctx = Ctx::new(7, preserve_z);
                for _ in 0..500 {
                    // Non-integer points away from the origin (avoids JWildfire singularities).
                    let x = pts.random() * 4.0 - 2.0 + 0.0123;
                    let y = pts.random() * 4.0 - 2.0 + 0.0177;
                    let mut a = Pt { x, y, z: 0.3, ..Default::default() };
                    let mut v = Pt::default();
                    (def.apply)(&data, &mut ctx, &mut a, &mut v, 0.8);
                    assert!(!v.is_bad() && !a.is_bad(), "{}: non-finite result for ({x}, {y}): {v:?}", def.name);
                }
            }
        }
    }

    #[test]
    fn elliptic_modes_finite() {
        let def = DEFS.iter().find(|d| d.name == "elliptic").unwrap();
        let mut ctx = Ctx::new(3, false);
        for mode in [0.0, 1.0, 2.0] {
            let data = def.prepare_data(&[mode], &Affine::default());
            for &(x, y) in &[(0.01, 0.02), (0.5, -0.3), (-1.5, 0.7), (2.0, 0.0)] {
                let mut a = Pt { x, y, ..Default::default() };
                let mut v = Pt::default();
                (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
                assert!(!v.is_bad(), "mode {mode}: ({x}, {y}) -> {v:?}");
            }
        }
    }
}
