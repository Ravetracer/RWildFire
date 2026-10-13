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

//! Variations ported from the JWildfire sources (batch w13).

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;
use super::complex::Complex;

/// WGSL for [`sym_apply`]; the argument is the number of parameters (start of the block).
macro_rules! sym_wgsl {
    ($n:literal) => {
        concat!(
            "let o = base + ",
            $n,
            "u;
let x = a.x + prm[o];
let y = a.y + prm[o + 1u];
let n = prm[o + 2u];
let k = o + 3u + 6u * u32(min(floor(n * rnd()), n - 1.0));
v.x += AMOUNT * (prm[k] * x + prm[k + 1u] * y + prm[k + 2u]);
v.y += AMOUNT * (prm[k + 3u] * x + prm[k + 4u] * y + prm[k + 5u]);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        )
    };
}

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "spirograph3D",
        params: &[
            ParamDef::new("a", 1.0),
            ParamDef::new("b", -0.3),
            ParamDef::new("c", 0.4),
            ParamDef::new("tmin", 0.0),
            ParamDef::range("tmax", 1000.0, 0.0, 2000.0),
            ParamDef::new("width", 0.0),
            ParamDef::int("mode", 0.0, 0.0, 4.0),
            ParamDef::int("direct_color", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            let (a, b, c, tmin, tmax, width) = (d[0], d[1], d[2], d[3], d[4], d[5]);
            let t = (tmax - tmin) * ctx.random() + tmin;
            let (w1, w2, w3);
            match d[6] as i32 {
                0 => {
                    w1 = width * ctx.random() - width / 2.0;
                    w2 = w1;
                    w3 = w1;
                }
                1 => {
                    let w = width * ctx.random() - width / 2.0;
                    w2 = w * (36.0 * t + M_2PI / 3.0).sin();
                    w3 = w * (36.0 * t + 2.0 * M_2PI / 3.0).sin();
                    w1 = w * (36.0 * t).sin();
                }
                2 => {
                    w1 = width * ctx.random() - width / 2.0;
                    w2 = width * ctx.random() - width / 2.0;
                    w3 = width * ctx.random() - width / 2.0;
                }
                3 => {
                    w1 = width * (ctx.random() + ctx.random() + ctx.random() + ctx.random() - 2.0) / 2.0;
                    w2 = width * (ctx.random() + ctx.random() + ctx.random() + ctx.random() - 2.0) / 2.0;
                    w3 = width * (ctx.random() + ctx.random() + ctx.random() + ctx.random() - 2.0) / 2.0;
                }
                4 => {
                    w1 = if ctx.random() < 0.5 { width } else { -width };
                    w2 = 0.0;
                    w3 = 0.0;
                }
                _ => {
                    w1 = 0.0;
                    w2 = 0.0;
                    w3 = 0.0;
                }
            }
            let x1 = (a + b) * t.cos() - c * ((a + b) / b * t).cos();
            let y1 = (a + b) * t.sin() - c * ((a + b) / b * t).sin();
            let z1 = c * ((a + b) / b * t).sin();
            v.x += amount * (x1 + w1);
            v.y += amount * (y1 + w2);
            v.z += amount * (z1 + w3);
            if d[7] as i32 != 0 {
                v.color = fmod(t / M_2PI, 1.0);
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[6] = limit_value(ftoi(p[6]), 0, 4) as f64;
            d[7] = limit_value(ftoi(p[7]), 0, 1) as f64;
            d
        }),
        wgsl: "let a0 = D(0);
let b0 = D(1);
let c0 = D(2);
let width = D(5);
let t = (D(4) - D(3)) * rnd() + D(3);
var w1 = 0.0;
var w2 = 0.0;
var w3 = 0.0;
let mode = i32(D(6));
if (mode == 0) {
  w1 = width * rnd() - width / 2.0;
  w2 = w1;
  w3 = w1;
} else if (mode == 1) {
  let w = width * rnd() - width / 2.0;
  w2 = w * sin(36.0 * t + TWO_PI / 3.0);
  w3 = w * sin(36.0 * t + 2.0 * TWO_PI / 3.0);
  w1 = w * sin(36.0 * t);
} else if (mode == 2) {
  w1 = width * rnd() - width / 2.0;
  w2 = width * rnd() - width / 2.0;
  w3 = width * rnd() - width / 2.0;
} else if (mode == 3) {
  w1 = width * (rnd() + rnd() + rnd() + rnd() - 2.0) / 2.0;
  w2 = width * (rnd() + rnd() + rnd() + rnd() - 2.0) / 2.0;
  w3 = width * (rnd() + rnd() + rnd() + rnd() - 2.0) / 2.0;
} else if (mode == 4) {
  w1 = select(-width, width, rnd() < 0.5);
}
let x1 = (a0 + b0) * cos(t) - c0 * cos((a0 + b0) / b0 * t);
let y1 = (a0 + b0) * sin(t) - c0 * sin((a0 + b0) / b0 * t);
let z1 = c0 * sin((a0 + b0) / b0 * t);
v.x += AMOUNT * (x1 + w1);
v.y += AMOUNT * (y1 + w2);
v.z += AMOUNT * (z1 + w3);
if (i32(D(7)) != 0) { v.color = fmod(t / TWO_PI, 1.0); }",
    },
    VarDef {
        name: "spligon",
        params: &[ParamDef::new("sides", 3.0), ParamDef::new("r", 1.0), ParamDef::new("i", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [sides, r, i, th, thi, j]
        apply: |d, ctx, a, v, amount| {
            let t = d[4] * (a.atan_yx() * d[3]).floor() + d[5];
            let (dx, dy) = (t.sin(), t.cos());
            v.x += amount * (a.x + dy * d[1]);
            v.y += amount * (a.y + dx * d[1]);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let sides = if ftoi(p[0]) == 0 { 1.0 } else { p[0] };
            let th = sides * M_1_2PI;
            vec![sides, p[1], p[2], th, 1.0 / th, M_PI * p[2] / (-2.0 * sides)]
        }),
        wgsl: "let t = D(4) * floor(atan_yx(a) * D(3)) + D(5);
v.x += AMOUNT * (a.x + cos(t) * D(1));
v.y += AMOUNT * (a.y + sin(t) * D(1));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "spliptic_bs",
        params: &[ParamDef::new("x", 0.05), ParamDef::new("y", 0.05)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sqrt_safe = |x: f64| if x < SMALL_EPSILON { 0.0 } else { x.sqrt() };
            let vv = amount / (M_PI / 2.0);
            let tmp = a.y * a.y + a.x * a.x + 1.0;
            let x2 = 2.0 * a.x;
            let xmax = 0.5 * ((tmp + x2).sqrt() + (tmp - x2).sqrt());
            let aa = a.x / xmax;
            let b = sqrt_safe(1.0 - aa * aa);
            if a.x >= 0.0 {
                v.x += vv * aa.atan2(b) + d[0];
            } else {
                v.x += vv * aa.atan2(b) - d[0];
            }
            if ctx.random() < 0.5 {
                v.y += vv * (xmax + sqrt_safe(xmax - 1.0)).ln() + d[1];
            } else {
                v.y -= vv * (xmax + sqrt_safe(xmax - 1.0)).ln() + d[1];
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "fn spliptic_bs_sqrt_safe(x: f32) -> f32 { return select(sqrt(x), 0.0, x < EPS); }
@body
let vv = AMOUNT / (PI / 2.0);
let tmp = a.y * a.y + a.x * a.x + 1.0;
let x2 = 2.0 * a.x;
let xmax = 0.5 * (sqrt(tmp + x2) + sqrt(tmp - x2));
let aa = a.x / xmax;
let b = spliptic_bs_sqrt_safe(1.0 - aa * aa);
if (a.x >= 0.0) {
  v.x += vv * atan2(aa, b) + D(0);
} else {
  v.x += vv * atan2(aa, b) - D(0);
}
let l = vv * log(xmax + spliptic_bs_sqrt_safe(xmax - 1.0));
if (rnd() < 0.5) {
  v.y += l + D(1);
} else {
  v.y -= l + D(1);
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "splitbrdr",
        params: &[ParamDef::new("x", 0.25), ParamDef::new("y", 0.25), ParamDef::new("px", 0.0), ParamDef::new("py", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x, y) = (d[0], d[1]);
            let b = amount / ((sqr(a.x) + sqr(a.y)) / 4.0 + 1.0);
            v.x += a.x * b;
            v.y += a.y * b;
            let (round_x, round_y) = (rint(a.x), rint(a.y));
            let (ox, oy) = (a.x - round_x, a.y - round_y);
            if ctx.random() >= 0.75 {
                v.x += amount * (ox * 0.5 + round_x);
                v.y += amount * (oy * 0.5 + round_y);
            } else if ox.abs() >= oy.abs() {
                if ox >= 0.0 {
                    v.x += amount * (ox * 0.5 + round_x + x);
                    v.y += amount * (oy * 0.5 + round_y + y * oy / ox);
                } else {
                    v.x += amount * (ox * 0.5 + round_x - y);
                    v.y += amount * (oy * 0.5 + round_y - y * oy / ox);
                }
            } else if oy >= 0.0 {
                v.y += amount * (oy * 0.5 + round_y + y);
                v.x += amount * (ox * 0.5 + round_x + ox / oy * y);
            } else {
                v.y += amount * (oy * 0.5 + round_y - y);
                v.x += amount * (ox * 0.5 + round_x - ox / oy * x);
            }
            v.x += a.x * d[2];
            v.y += a.y * d[3];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = D(0);
let y = D(1);
let b = AMOUNT / ((sqr(a.x) + sqr(a.y)) / 4.0 + 1.0);
v.x += a.x * b;
v.y += a.y * b;
let round_x = round(a.x);
let round_y = round(a.y);
let ox = a.x - round_x;
let oy = a.y - round_y;
if (rnd() >= 0.75) {
  v.x += AMOUNT * (ox * 0.5 + round_x);
  v.y += AMOUNT * (oy * 0.5 + round_y);
} else if (abs(ox) >= abs(oy)) {
  if (ox >= 0.0) {
    v.x += AMOUNT * (ox * 0.5 + round_x + x);
    v.y += AMOUNT * (oy * 0.5 + round_y + y * oy / ox);
  } else {
    v.x += AMOUNT * (ox * 0.5 + round_x - y);
    v.y += AMOUNT * (oy * 0.5 + round_y - y * oy / ox);
  }
} else if (oy >= 0.0) {
  v.y += AMOUNT * (oy * 0.5 + round_y + y);
  v.x += AMOUNT * (ox * 0.5 + round_x + ox / oy * y);
} else {
  v.y += AMOUNT * (oy * 0.5 + round_y - y);
  v.x += AMOUNT * (ox * 0.5 + round_x - ox / oy * x);
}
v.x += a.x * D(2);
v.y += a.y * D(3);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "splits3D",
        params: &[ParamDef::new("x", 0.1), ParamDef::new("y", 0.3), ParamDef::new("z", 0.2)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            v.x += amount * if a.x >= 0.0 { a.x + d[0] } else { a.x - d[0] };
            v.y += amount * if a.y >= 0.0 { a.y + d[1] } else { a.y - d[1] };
            v.z += amount * if a.z >= 0.0 { a.z + d[2] } else { a.z - d[2] };
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * select(a.x - D(0), a.x + D(0), a.x >= 0.0);
v.y += AMOUNT * select(a.y - D(1), a.y + D(1), a.y >= 0.0);
v.z += AMOUNT * select(a.z - D(2), a.z + D(2), a.z >= 0.0);",
    },
    simple("sqrt_acosech", false, |_, ctx, a, v, amount| sqrt_complex(ctx, a, v, amount, Complex::acosech), SQRT_ACOSECH_WGSL),
    simple("sqrt_acosh", false, |_, ctx, a, v, amount| sqrt_complex(ctx, a, v, amount, Complex::acosh), SQRT_ACOSH_WGSL),
    simple("sqrt_acoth", false, |_, ctx, a, v, amount| sqrt_complex(ctx, a, v, amount, Complex::acoth), SQRT_ACOTH_WGSL),
    simple("sqrt_asech", false, |_, ctx, a, v, amount| sqrt_complex(ctx, a, v, amount, Complex::asech), SQRT_ASECH_WGSL),
    simple("sqrt_asinh", false, |_, ctx, a, v, amount| sqrt_complex(ctx, a, v, amount, Complex::asinh), SQRT_ASINH_WGSL),
    simple("sqrt_atanh", false, |_, ctx, a, v, amount| sqrt_complex(ctx, a, v, amount, Complex::atanh), SQRT_ATANH_WGSL),
    simple(
        "square3D",
        true,
        |_, ctx, _a, v, amount| {
            v.x += amount * (ctx.random() - 0.5);
            v.y += amount * (ctx.random() - 0.5);
            v.z += amount * (ctx.random() - 0.5);
        },
        "v.x += AMOUNT * (rnd() - 0.5);
v.y += AMOUNT * (rnd() - 0.5);
v.z += AMOUNT * (rnd() - 0.5);",
    ),
    simple(
        "squircular",
        false,
        |_, ctx, a, v, amount| {
            let (u, w) = (a.x, a.y);
            let mut r = u * u + w * w;
            let rs = r.sqrt();
            let xs = if u > 0.0 { 1.0 } else { -1.0 };
            r = (amount * amount * r - 4.0 * u * u * w * w).sqrt();
            r = (1.0 + u * u / (w * w) - rs / (amount * w * w) * r).sqrt();
            r /= 2f64.sqrt();
            v.x += xs * r;
            v.y += w / u * r;
            ctx.pass_z(a, v, amount);
        },
        "let u = a.x;
let w = a.y;
var r = u * u + w * w;
let rs = sqrt(r);
let xs = select(-1.0, 1.0, u > 0.0);
r = sqrt(AMOUNT * AMOUNT * r - 4.0 * u * u * w * w);
r = sqrt(1.0 + u * u / (w * w) - rs / (AMOUNT * w * w) * r);
r = r / sqrt(2.0);
v.x += xs * r;
v.y += w / u * r;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "squirrel",
        params: &[ParamDef::new("a", 1.0), ParamDef::new("b", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let u = (d[0] + EPSILON) * sqr(a.x) + (d[1] + EPSILON) * sqr(a.y);
            v.x = u.sqrt().cos() * a.x.tan() * amount;
            v.y = u.sqrt().sin() * a.y.tan() * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let u = (D(0) + EPSILON) * sqr(a.x) + (D(1) + EPSILON) * sqr(a.y);
v.x = cos(sqrt(u)) * tan(a.x) * AMOUNT;
v.y = sin(sqrt(u)) * tan(a.y) * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "squish",
        params: &[ParamDef::int("power", 2.0, 2.0, 20.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [power, inv_power]
        apply: |d, ctx, a, v, amount| {
            let (x, y) = (a.x.abs(), a.y.abs());
            let (s, mut p);
            if x > y {
                s = x;
                p = if a.x > 0.0 { a.y } else { 4.0 * s - a.y };
            } else {
                s = y;
                p = if a.y > 0.0 { 2.0 * s - a.x } else { 6.0 * s + a.x };
            }
            p = d[1] * (p + 8.0 * s * (d[0] * ctx.random()).floor());
            if p <= s {
                v.x += amount * s;
                v.y += amount * p;
            } else if p <= 3.0 * s {
                v.x += amount * (2.0 * s - p);
                v.y += amount * s;
            } else if p <= 5.0 * s {
                v.x -= amount * s;
                v.y += amount * (4.0 * s - p);
            } else if p <= 7.0 * s {
                v.x -= amount * (6.0 * s - p);
                v.y -= amount * s;
            } else {
                v.x += amount * s;
                v.y -= amount * (8.0 * s - p);
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let power = limit_value(ftoi(p[0]), 2, i32::MAX) as f64;
            vec![power, 1.0 / power]
        }),
        wgsl: "let x = abs(a.x);
let y = abs(a.y);
var s = 0.0;
var p = 0.0;
if (x > y) {
  s = x;
  p = select(4.0 * s - a.y, a.y, a.x > 0.0);
} else {
  s = y;
  p = select(6.0 * s + a.x, 2.0 * s - a.x, a.y > 0.0);
}
p = D(1) * (p + 8.0 * s * floor(D(0) * rnd()));
if (p <= s) {
  v.x += AMOUNT * s;
  v.y += AMOUNT * p;
} else if (p <= 3.0 * s) {
  v.x += AMOUNT * (2.0 * s - p);
  v.y += AMOUNT * s;
} else if (p <= 5.0 * s) {
  v.x -= AMOUNT * s;
  v.y += AMOUNT * (4.0 * s - p);
} else if (p <= 7.0 * s) {
  v.x -= AMOUNT * (6.0 * s - p);
  v.y -= AMOUNT * s;
} else {
  v.x += AMOUNT * s;
  v.y -= AMOUNT * (8.0 * s - p);
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "standing_wave",
        params: &[ParamDef::new("freq_x", 3.0), ParamDef::new("freq_y", 3.0), ParamDef::new("amp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let dx = d[2] * (d[0] * M_PI * a.x).sin() * (d[1] * M_PI * a.y).cos();
            let dy = d[2] * (d[0] * M_PI * a.x).cos() * (d[1] * M_PI * a.y).sin();
            v.x += amount * (a.x + dx);
            v.y += amount * (a.y + dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dx = D(2) * sin(D(0) * PI * a.x) * cos(D(1) * PI * a.y);
let dy = D(2) * cos(D(0) * PI * a.x) * sin(D(1) * PI * a.y);
v.x += AMOUNT * (a.x + dx);
v.y += AMOUNT * (a.y + dy);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "starblur",
        params: &[ParamDef::int("power", 5.0, 1.0, 20.0), ParamDef::new("range", 0.401_622_831_772_454_56)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [power, range, alpha, length]
        apply: |d, ctx, a, v, amount| {
            let (power, alpha) = (d[0], d[2]);
            let mut f = ctx.random() * power * 2.0;
            let mut angle = f.trunc();
            f -= angle;
            let x = f * d[3];
            let mut z = (1.0 + sqr(x) - 2.0 * x * alpha.cos()).sqrt();
            let ia = angle as i32;
            if ia % 2 == 0 {
                angle = M_2PI / power * (ia / 2) as f64 + (alpha.sin() * x / z).asin();
            } else {
                angle = M_2PI / power * (ia / 2) as f64 - (alpha.sin() * x / z).asin();
            }
            z *= ctx.random().sqrt();
            let ang = angle - M_PI_2;
            v.x += amount * z * ang.cos();
            v.y += amount * z * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let power = ftoi(p[0]) as f64;
            let range = p[1];
            let alpha = M_PI / power;
            let length = (1.0 + sqr(range) - 2.0 * range * alpha.cos()).sqrt();
            vec![power, range, (alpha.sin() * range / length).asin(), length]
        }),
        wgsl: "let power = D(0);
let alpha = D(2);
var f = rnd() * power * 2.0;
let ia = i32(trunc(f));
f = f - trunc(f);
let x = f * D(3);
var z = sqrt(1.0 + sqr(x) - 2.0 * x * cos(alpha));
var angle = TWO_PI / power * f32(ia / 2);
if (ia % 2 == 0) {
  angle += asin(sin(alpha) * x / z);
} else {
  angle -= asin(sin(alpha) * x / z);
}
z = z * sqrt(rnd());
let ang = angle - HALF_PI;
v.x += AMOUNT * z * cos(ang);
v.y += AMOUNT * z * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "starfractal",
        params: &[ParamDef::int("m", 3.0, 3.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [m, a[0..20], b[0..20]]
        apply: |d, ctx, _a, v, amount| {
            // Java follows one endless orbit from (1, 1), driven by its own java.util.Random(1000),
            // per instance; here one per render thread.
            let m = d[0] as i32;
            let key = d.as_ptr() as usize;
            let mut seed = ctx.orbits.get(&(key + 1)).map_or(java_random_seed(1000), |s| s[0] as u64);
            let c = (java_random_next_int(&mut seed).unsigned_abs() % (2 * m) as u32) as usize;
            ctx.orbits.insert(key + 1, [seed as f64, 0.0]);
            let p = ctx.orbits.entry(key).or_insert([1.0, 1.0]);
            let (x, y) = (p[0], p[1]);
            let x1 = (x / (x * x + y * y)) / 3.0 + d[1 + c];
            let y1 = (-y / (x * x + y * y)) / 3.0 + d[21 + c];
            let (x, y) = (x1 / (x1 * x1 + y1 * y1), y1 / (x1 * x1 + y1 * y1));
            *p = [x, y];
            v.x = x * amount;
            v.y = y * amount;
        },
        prepare: Some(|p, _| {
            let m = limit_value(p[0], 3.0, 10.0) as i32;
            let mut d = vec![0.0; 41];
            d[0] = m as f64;
            let mf = m as f64;
            for i in 0..m as usize {
                let i1 = (i + 1) as f64;
                d[1 + 2 * i] = (2.0 * M_PI * i1 / mf).cos();
                d[21 + 2 * i] = (2.0 * M_PI * i1 / mf).sin();
                d[2 + 2 * i] = 0.5 * ((2.0 * M_PI * i1 / mf).cos() + (2.0 * M_PI * i as f64 / mf).cos());
                d[22 + 2 * i] = 0.5 * ((2.0 * M_PI * i1 / mf).sin() + (2.0 * M_PI * i as f64 / mf).sin());
            }
            d
        }),
        // GPU: no state across calls, every call iterates the orbit from (1, 1) with random maps.
        wgsl: "let n2 = 2 * i32(D(0));
var x = 1.0;
var y = 1.0;
let n = 32 + min(i32(rnd() * 32.0), 31);
for (var i = 0; i < n; i++) {
  let c = u32(min(i32(rnd() * f32(n2)), n2 - 1));
  let x1 = (x / (x * x + y * y)) / 3.0 + prm[base + 1u + c];
  let y1 = (-y / (x * x + y * y)) / 3.0 + prm[base + 21u + c];
  x = x1 / (x1 * x1 + y1 * y1);
  y = y1 / (x1 * x1 + y1 * y1);
}
v.x = x * AMOUNT;
v.y = y * AMOUNT;",
    },
    VarDef {
        name: "stereographic",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let st_d = 1.0 + (a.x * a.x + a.y * a.y) * d[0];
            v.x += amount * 2.0 * a.x / st_d;
            v.y += amount * 2.0 * a.y / st_d;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let st_d = 1.0 + (a.x * a.x + a.y * a.y) * D(0);
v.x += AMOUNT * 2.0 * a.x / st_d;
v.y += AMOUNT * 2.0 * a.y / st_d;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "stereographic_plane",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let lat = a.y * d[0];
            let lon = a.x * d[0];
            let cos_lat = lat.cos();
            let denom = java_max(1.0 - lat.sin(), 0.001);
            v.x += amount * cos_lat * lon.cos() / denom;
            v.y += amount * cos_lat * lon.sin() / denom;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let lat = a.y * D(0);
let lon = a.x * D(0);
let denom = max(1.0 - sin(lat), 0.001);
v.x += AMOUNT * cos(lat) * cos(lon) / denom;
v.y += AMOUNT * cos(lat) * sin(lon) / denom;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "stripfit",
        params: &[ParamDef::new("dx", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let dxp = -0.5 * d[0];
            if amount != 0.0 {
                v.x += amount * a.x;
                if a.y > 1.0 {
                    let fity = fmod(a.y + 1.0, 2.0);
                    v.y += amount * (-1.0 + fity);
                    v.x += (a.y - fity + 1.0) * dxp;
                } else if a.y < -1.0 {
                    let fity = fmod(1.0 - a.y, 2.0);
                    v.y += amount * (1.0 - fity);
                    v.x += (a.y + fity - 1.0) * dxp;
                } else {
                    v.y += amount * a.y;
                }
                ctx.pass_z(a, v, amount);
            }
        },
        prepare: None,
        wgsl: "let dxp = -0.5 * D(0);
if (AMOUNT != 0.0) {
  v.x += AMOUNT * a.x;
  if (a.y > 1.0) {
    let fity = fmod(a.y + 1.0, 2.0);
    v.y += AMOUNT * (-1.0 + fity);
    v.x += (a.y - fity + 1.0) * dxp;
  } else if (a.y < -1.0) {
    let fity = fmod(1.0 - a.y, 2.0);
    v.y += AMOUNT * (1.0 - fity);
    v.x += (a.y + fity - 1.0) * dxp;
  } else {
    v.y += AMOUNT * a.y;
  }
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    VarDef {
        name: "stwin",
        params: &[ParamDef::new("distort", 1.0), ParamDef::new("offset_xy", 0.0), ParamDef::new("offset_x2", 0.0), ParamDef::new("offset_y2", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let x = a.x * amount * 0.05;
            let y = a.y * amount * 0.05;
            let x2 = x * x + (d[2] * 0.0001);
            let y2 = y * y + (d[3] * 0.0001);
            let mut result = (x2 - y2) * (M_2PI * d[0] * (x + y + (d[1] * 0.1))).sin();
            let mut divident = x2 + y2;
            if divident == 0.0 {
                divident = 1.0;
            }
            result /= divident;
            v.x += (amount * a.x) + result;
            v.y += (amount * a.y) + result;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = a.x * AMOUNT * 0.05;
let y = a.y * AMOUNT * 0.05;
let x2 = x * x + (D(2) * 0.0001);
let y2 = y * y + (D(3) * 0.0001);
var divident = x2 + y2;
if (divident == 0.0) { divident = 1.0; }
let result = (x2 - y2) * sin(TWO_PI * D(0) * (x + y + (D(1) * 0.1))) / divident;
v.x += (AMOUNT * a.x) + result;
v.y += (AMOUNT * a.y) + result;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sublimation",
        params: &[ParamDef::new("lift", 0.3), ParamDef::new("spread", 1.0), ParamDef::new("freq", 3.0), ParamDef::new("amp", 0.1)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let rise = (-a.x.abs() * d[1]).exp() * d[0];
            let turb = d[3] * (d[2] * a.y).sin();
            v.x += amount * (a.x + turb);
            v.y += amount * (a.y + rise);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let rise = exp(-abs(a.x) * D(1)) * D(0);
let turb = D(3) * sin(D(2) * a.y);
v.x += AMOUNT * (a.x + turb);
v.y += AMOUNT * (a.y + rise);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sunflower",
        params: &[
            ParamDef::int("nPoints", 500.0, 10.0, 1000.0),
            ParamDef::int("shape", 10.0, 3.0, 20.0),
            ParamDef::range("scale", 0.02, 0.0, 100.0),
            ParamDef::range("angle", 180.0, 0.0, 360.0),
            ParamDef::range("color", 0.0, 0.0, 1.0),
            ParamDef::range("F. filling", 0.0, 0.0, 1.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [n_points, shape, scale, angle, color, fill, invert]
        apply: |d, ctx, a, v, amount| {
            // The primitives (n-gons on a golden-angle spiral) are evaluated on demand.
            let n = d[0] as i32;
            let ang = d[3] * (3.0 - 5f64.sqrt());
            let rmax = ((n + 1) as f64).sqrt() / 30.0;
            let i = (ctx.random() * n as f64) as i32;
            let r = ((i + 1) as f64).sqrt() / 30.0;
            let t = (i + 1) as f64 * ang * M_PI / 180.0;
            let (px, py) = (r * t.cos() / rmax, r * t.sin() / rmax);
            let sc = if d[6] as i32 == 1 { r / rmax } else { 1.0 - r / rmax };
            let scale = d[2] * sc;
            let (ox, oy) = super::draw::ngon_point(ctx, d[1] as i32, d[5]);
            v.x += amount * (ox * scale + px);
            v.y += amount * (oy * scale + py);
            v.color = sc;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(p[0], 10.0, 1000.0) as i32 as f64;
            d[1] = limit_value(p[1], 3.0, 20.0) as i32 as f64;
            d[2] = limit_value(p[2], 0.0, 100.0);
            d[3] = limit_value(p[3], 0.0, 360.0);
            d[4] = limit_value(p[4], 0.0, 1.0);
            d[5] = limit_value(p[5], 0.0, 1.0);
            d[6] = limit_value(p[6], 0.0, 1.0) as i32 as f64;
            d
        }),
        wgsl: "let n = D(0);
let ang = D(3) * (3.0 - sqrt(5.0));
let rmax = sqrt(n + 1.0) / 30.0;
let i = floor(rnd() * n);
let r = sqrt(i + 1.0) / 30.0;
let t = (i + 1.0) * ang * PI / 180.0;
let px = r * cos(t) / rmax;
let py = r * sin(t) / rmax;
var sc = 1.0 - r / rmax;
if (i32(D(6)) == 1) { sc = r / rmax; }
let scale = D(2) * sc;
let o = res_ngon_point(i32(D(1)), D(5));
v.x += AMOUNT * (o.x * scale + px);
v.y += AMOUNT * (o.y * scale + py);
v.color = sc;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "supernova",
        params: &[ParamDef::new("radius", 0.5), ParamDef::new("boost", 2.0), ParamDef::new("spin", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let radius = java_max(d[0].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let ring = r - radius;
            let env = (-ring * ring / (radius * radius * 0.1 + 1e-6)).exp();
            let new_r = r + env * d[1] * r;
            let new_theta = theta + d[2] * env;
            v.x += amount * new_r * new_theta.cos();
            v.y += amount * new_r * new_theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let radius = max(abs(D(0)), 0.01);
let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
let ring = r - radius;
let env = exp(-ring * ring / (radius * radius * 0.1 + 1e-6));
let new_r = r + env * D(1) * r;
let new_theta = theta + D(2) * env;
v.x += AMOUNT * new_r * cos(new_theta);
v.y += AMOUNT * new_r * sin(new_theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "superposition",
        params: &[ParamDef::new("freq1", 3.0), ParamDef::new("freq2", 5.0), ParamDef::new("phase", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let wave = 0.5 * ((d[0] * r).sin() + (d[1] * r + d[2]).sin());
            v.x += amount * a.x * wave;
            v.y += amount * a.y * wave;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let wave = 0.5 * (sin(D(0) * r) + sin(D(1) * r + D(2)));
v.x += AMOUNT * a.x * wave;
v.y += AMOUNT * a.y * wave;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "superShape3d",
        params: &[
            ParamDef::new("rho", 9.9),
            ParamDef::new("phi", 2.5),
            ParamDef::new("m1", 6.0),
            ParamDef::new("m2", 3.0),
            ParamDef::new("a1", 1.0),
            ParamDef::new("a2", 1.0),
            ParamDef::new("b1", 1.0),
            ParamDef::new("b2", 1.0),
            ParamDef::new("n1_1", 1.0),
            ParamDef::new("n1_2", 1.0),
            ParamDef::new("n2_1", 1.0),
            ParamDef::new("n2_2", 1.0),
            ParamDef::new("n3_1", 1.0),
            ParamDef::new("n3_2", 1.0),
            ParamDef::new("spiral", 0.0),
            ParamDef::int("toroidmap", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [16 params, n1n_1(16), n1n_2, an2_1, an2_2, bn3_1, bn3_2, m4_1, m4_2, rho_pi, phi_pi(25)]
        apply: |d, ctx, _a, v, amount| {
            let rho1 = ctx.random() * d[24];
            let mut phi1 = ctx.random() * d[25];
            if ctx.random_int(i32::MAX as usize) % 2 == 1 {
                phi1 = -phi1;
            }
            let (sinr, cosr) = rho1.sin_cos();
            let (sinp, cosp) = phi1.sin_cos();
            let (msinr, mcosr) = (d[22] * rho1).sin_cos();
            let (msinp, mcosp) = (d[23] * phi1).sin_cos();
            let pr1 = d[18] * mcosr.abs().powf(d[10]) + d[20] * msinr.abs().powf(d[12]);
            let pr2 = d[19] * mcosp.abs().powf(d[11]) + d[21] * msinp.abs().powf(d[13]);
            let r1 = pr1.powf(d[16]) + d[14] * rho1;
            let r2 = pr2.powf(d[17]);
            if d[15] as i32 == 1 {
                v.x += amount * cosr * (r1 + r2 * cosp);
                v.y += amount * sinr * (r1 + r2 * cosp);
            } else {
                v.x += amount * r1 * cosr * r2 * cosp;
                v.y += amount * r1 * sinr * r2 * cosp;
            }
            v.z += amount * r2 * sinp;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[15] = ftoi(p[15]).clamp(0, 1) as f64;
            d.extend([
                -1.0 / p[8],
                -1.0 / p[9],
                (1.0 / p[4]).abs().powf(p[10]),
                (1.0 / p[5]).abs().powf(p[11]),
                (1.0 / p[6]).abs().powf(p[12]),
                (1.0 / p[7]).abs().powf(p[13]),
                p[2] / 4.0,
                p[3] / 4.0,
                p[0] * M_2_PI,
                p[1] * M_2_PI,
            ]);
            d
        }),
        wgsl: "let rho1 = rnd() * D(24);
var phi1 = rnd() * D(25);
if (rnd() >= 0.5) { phi1 = -phi1; }
let sinr = sin(rho1);
let cosr = cos(rho1);
let sinp = sin(phi1);
let cosp = cos(phi1);
let msinr = sin(D(22) * rho1);
let mcosr = cos(D(22) * rho1);
let msinp = sin(D(23) * phi1);
let mcosp = cos(D(23) * phi1);
let pr1 = D(18) * pow(abs(mcosr), D(10)) + D(20) * pow(abs(msinr), D(12));
let pr2 = D(19) * pow(abs(mcosp), D(11)) + D(21) * pow(abs(msinp), D(13));
let r1 = pow(pr1, D(16)) + D(14) * rho1;
let r2 = pow(pr2, D(17));
if (i32(D(15)) == 1) {
  v.x += AMOUNT * cosr * (r1 + r2 * cosp);
  v.y += AMOUNT * sinr * (r1 + r2 * cosp);
} else {
  v.x += AMOUNT * r1 * cosr * r2 * cosp;
  v.y += AMOUNT * r1 * sinr * r2 * cosp;
}
v.z += AMOUNT * r2 * sinp;",
    },
    VarDef {
        name: "svensson_js",
        params: &[ParamDef::new("a", 1.4), ParamDef::new("b", 1.56), ParamDef::new("c", 1.4), ParamDef::new("d", -6.56)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let x = d[3] * (d[0] * a.x).sin() - (d[1] * a.y).sin();
            let y = d[2] * (d[0] * a.x).cos() + (d[1] * a.y).cos();
            v.x = x * amount;
            v.y = y * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = D(3) * sin(D(0) * a.x) - sin(D(1) * a.y);
let y = D(2) * cos(D(0) * a.x) + cos(D(1) * a.y);
v.x = x * AMOUNT;
v.y = y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "svf",
        params: &[ParamDef::new("n", 2.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            let cn = (d[0] * a.y).cos();
            let (sx, cx) = a.x.sin_cos();
            let (sy, cy) = a.y.sin_cos();
            v.x += amount * (cy * (cn * cx));
            v.y += amount * (cy * (cn * sx));
            v.z += amount * (sy * cn);
        },
        prepare: None,
        wgsl: "let cn = cos(D(0) * a.y);
v.x += AMOUNT * (cos(a.y) * (cn * cos(a.x)));
v.y += AMOUNT * (cos(a.y) * (cn * sin(a.x)));
v.z += AMOUNT * (sin(a.y) * cn);",
    },
    VarDef {
        name: "swirl3",
        params: &[ParamDef::new("shift", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let rad = a.rad();
            let ang = a.atan_yx() + rad.ln() * d[0];
            v.x += amount * rad * ang.cos();
            v.y += amount * rad * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = rad(a);
let ang = atan_yx(a) + log(r) * D(0);
v.x += AMOUNT * r * cos(ang);
v.y += AMOUNT * r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "sym_bg1",
        params: &[ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_bg1_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_bg2",
        params: &[ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_bg2_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_bg3",
        params: &[ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_bg3_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_bg4",
        params: &[ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_bg4_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_bg5",
        params: &[ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_bg5_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_bg6",
        params: &[ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_bg6_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_bg7",
        params: &[ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_bg7_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_ng1",
        params: &[ParamDef::new("stepx", 0.5), ParamDef::new("stepy", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_ng1_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_ng10",
        params: &[ParamDef::new("space", 0.0), ParamDef::new("spacex", 0.0), ParamDef::new("spacey", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng10_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng11",
        params: &[ParamDef::new("space", 0.0), ParamDef::new("spacex", 0.0), ParamDef::new("spacey", 0.0), ParamDef::new("stepx", 1.5), ParamDef::new("stepy", 1.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[5..], ctx, a, v, amount),
        prepare: Some(sym_ng11_prepare),
        wgsl: sym_wgsl!("5"),
    },
    VarDef {
        name: "sym_ng12",
        params: &[ParamDef::new("space", 0.0), ParamDef::new("spacex", 0.0), ParamDef::new("spacey", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng12_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng13",
        params: &[ParamDef::new("radius", 0.0), ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng13_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng14",
        params: &[ParamDef::new("radius", 0.0), ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng14_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng15",
        params: &[ParamDef::new("radius", 0.0), ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng15_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng16",
        params: &[ParamDef::new("radius", 0.0), ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng16_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng17",
        params: &[ParamDef::new("radius", 0.0), ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng17_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng2",
        params: &[ParamDef::new("stepx", 2.0), ParamDef::new("stepy", 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_ng2_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_ng3",
        params: &[ParamDef::new("sepx", 0.5), ParamDef::new("sepy", 0.0), ParamDef::new("step", 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng3_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng4",
        params: &[ParamDef::new("sepx", 0.0), ParamDef::new("sepy", 0.0), ParamDef::new("stepx", 0.0), ParamDef::new("stepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[4..], ctx, a, v, amount),
        prepare: Some(sym_ng4_prepare),
        wgsl: sym_wgsl!("4"),
    },
    VarDef {
        name: "sym_ng5",
        params: &[ParamDef::new("sepx", 0.0), ParamDef::new("sepy", 0.0), ParamDef::new("stepx", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng5_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng6",
        params: &[ParamDef::new("sepy", 0.1), ParamDef::new("stepx", 1.0), ParamDef::new("stepy", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[3..], ctx, a, v, amount),
        prepare: Some(sym_ng6_prepare),
        wgsl: sym_wgsl!("3"),
    },
    VarDef {
        name: "sym_ng7",
        params: &[ParamDef::new("sepx", 0.0), ParamDef::new("sepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_ng7_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "sym_ng8",
        params: &[ParamDef::new("sepx", 0.0), ParamDef::new("sepy", 0.0), ParamDef::new("skewx", 0.0), ParamDef::new("skewy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[4..], ctx, a, v, amount),
        prepare: Some(sym_ng8_prepare),
        wgsl: sym_wgsl!("4"),
    },
    VarDef {
        name: "sym_ng9",
        params: &[ParamDef::new("sepx", 0.0), ParamDef::new("sepy", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| sym_apply(&d[2..], ctx, a, v, amount),
        prepare: Some(sym_ng9_prepare),
        wgsl: sym_wgsl!("2"),
    },
    VarDef {
        name: "symmetric_icon",
        params: &[ParamDef::new("lambda", 1.56), ParamDef::new("alpha", -1.0), ParamDef::new("beta", 0.1), ParamDef::new("omega", -0.82)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (lambda, alpha, beta, omega) = (d[0], d[1], d[2], d[3]);
            let r2 = a.x * a.x + a.y * a.y;
            let nx = lambda * a.x + alpha * (a.x * a.x - a.y * a.y) + beta * r2 * a.x + omega * a.x * a.y;
            let ny = lambda * a.y + 2.0 * alpha * a.x * a.y + beta * r2 * a.y + omega * a.x * a.x;
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r2 = a.x * a.x + a.y * a.y;
let nx = D(0) * a.x + D(1) * (a.x * a.x - a.y * a.y) + D(2) * r2 * a.x + D(3) * a.x * a.y;
let ny = D(0) * a.y + 2.0 * D(1) * a.x * a.y + D(2) * r2 * a.y + D(3) * a.x * a.x;
v.x += AMOUNT * nx;
v.y += AMOUNT * ny;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "synth",
        params: &[
            ParamDef::new("a", 1.0),
            ParamDef::int("mode", 3.0, 0.0, 1007.0),
            ParamDef::new("power", -2.0),
            ParamDef::new("mix", 1.0),
            ParamDef::int("smooth", 0.0, 0.0, 1.0),
            ParamDef::new("b", 0.0),
            ParamDef::int("b_type", 0.0, 0.0, 8.0),
            ParamDef::new("b_skew", 0.0),
            ParamDef::new("b_frq", 1.0),
            ParamDef::new("b_phs", 0.0),
            ParamDef::int("b_layer", 0.0, 0.0, 3.0),
            ParamDef::new("c", 0.0),
            ParamDef::int("c_type", 0.0, 0.0, 8.0),
            ParamDef::new("c_skew", 0.0),
            ParamDef::new("c_frq", 1.0),
            ParamDef::new("c_phs", 0.0),
            ParamDef::int("c_layer", 0.0, 0.0, 3.0),
            ParamDef::new("d", 0.0),
            ParamDef::int("d_type", 0.0, 0.0, 8.0),
            ParamDef::new("d_skew", 0.0),
            ParamDef::new("d_frq", 1.0),
            ParamDef::new("d_phs", 0.0),
            ParamDef::int("d_layer", 0.0, 0.0, 3.0),
            ParamDef::new("e", 0.0),
            ParamDef::int("e_type", 0.0, 0.0, 8.0),
            ParamDef::new("e_skew", 0.0),
            ParamDef::new("e_frq", 1.0),
            ParamDef::new("e_phs", 0.0),
            ParamDef::int("e_layer", 0.0, 0.0, 3.0),
            ParamDef::new("f", 0.0),
            ParamDef::int("f_type", 0.0, 0.0, 8.0),
            ParamDef::new("f_skew", 0.0),
            ParamDef::new("f_frq", 1.0),
            ParamDef::new("f_phs", 0.0),
            ParamDef::int("f_layer", 0.0, 0.0, 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [a, mode, power, mix, smooth, 5 x (amp, type, skew, frq, phs, layer)]
        apply: synth_apply,
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for i in [1, 4, 6, 10, 12, 16, 18, 22, 24, 28, 30, 34] {
                d[i] = ftoi(p[i]) as f64;
            }
            d
        }),
        wgsl: SYNTH_WGSL,
    },
    VarDef {
        name: "szubieta",
        params: &[
            ParamDef::int("width", 128.0, 32.0, 256.0),
            ParamDef::int("height", 128.0, 32.0, 256.0),
            ParamDef::int("type", 0.0, 0.0, 1.0),
            ParamDef::range("scale", 0.5, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [width, height, type, scale]
        apply: |d, ctx, a, v, amount| {
            // The width x height grid of 20-gons is evaluated on demand.
            let (w, h) = (d[0] as i32, d[1] as i32);
            let idx = (ctx.random() * (w * h) as f64) as i32;
            let (i, j) = (idx / h, idx % h);
            let x = if d[2] as i32 == 0 { (i * i - 2 * (i | j) + j * j) % 255 } else { (i & (j - 2 * (i ^ j) + j) & i) % 256 };
            let color = x.abs() as f64 / 255.0;
            let (px, py) = (i as f64 - w as f64 / 2.0, j as f64 - h as f64 / 2.0);
            let (ox, oy) = super::draw::ngon_point(ctx, 20, 0.0);
            v.x += amount * 0.1 * (ox * d[3] + px);
            v.y += amount * 0.1 * (oy * d[3] + py);
            v.color = color;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            vec![
                limit_value(p[0], 32.0, 256.0) as i32 as f64,
                limit_value(p[1], 32.0, 256.0) as i32 as f64,
                limit_value(p[2], 0.0, 1.0) as i32 as f64,
                limit_value(p[3], 0.0, 1.0),
            ]
        }),
        wgsl: "let w = i32(D(0));
let h = i32(D(1));
let idx = min(i32(rnd() * f32(w * h)), w * h - 1);
let i = idx / h;
let j = idx % h;
var x = (i & (j - 2 * (i ^ j) + j) & i) % 256;
if (i32(D(2)) == 0) { x = (i * i - 2 * (i | j) + j * j) % 255; }
let px = f32(i) - f32(w) / 2.0;
let py = f32(j) - f32(h) / 2.0;
let o = res_ngon_point(20, 0.0);
v.x += AMOUNT * 0.1 * (o.x * D(3) + px);
v.y += AMOUNT * 0.1 * (o.y * D(3) + py);
v.color = f32(abs(x)) / 255.0;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tan2_bs",
        params: &[ParamDef::new("x1", 2.0), ParamDef::new("x2", 2.0), ParamDef::new("y1", 2.0), ParamDef::new("y2", 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let tansin = (d[0] * a.x).sin();
            let tancos = (d[1] * a.x).cos();
            let tansinh = (d[2] * a.y).sinh();
            let tancosh = (d[3] * a.y).cosh();
            let den = tancos + tancosh;
            if den == 0.0 {
                return;
            }
            let tanden = 1.0 / den;
            v.x += amount * tanden * tansin;
            v.y += amount * tanden * tansinh;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let den = cos(D(1) * a.x) + cosh(D(3) * a.y);
if (den != 0.0) {
  let tanden = 1.0 / den;
  v.x += AMOUNT * tanden * sin(D(0) * a.x);
  v.y += AMOUNT * tanden * sinh(D(2) * a.y);
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    simple(
        "tancos",
        false,
        |_, ctx, a, v, amount| {
            let d1 = SMALL_EPSILON + sqr(a.x) + sqr(a.y);
            let d2 = amount / d1;
            v.x += d2 * (d1.tanh() * (2.0 * a.x));
            v.y += d2 * (d1.cos() * (2.0 * a.y));
            ctx.pass_z(a, v, amount);
        },
        "let d1 = EPS + sqr(a.x) + sqr(a.y);
let d2 = AMOUNT / d1;
v.x += d2 * (tanh(d1) * (2.0 * a.x));
v.y += d2 * (cos(d1) * (2.0 * a.y));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "tanh2_bs",
        params: &[ParamDef::new("x1", 2.0), ParamDef::new("x2", 2.0), ParamDef::new("y1", 2.0), ParamDef::new("y2", 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let tanhsin = (d[2] * a.y).sin();
            let tanhcos = (d[3] * a.y).cos();
            let tanhsinh = (d[0] * a.x).sinh();
            let tanhcosh = (d[1] * a.x).cosh();
            let den = tanhcos + tanhcosh;
            if den == 0.0 {
                return;
            }
            let tanhden = 1.0 / den;
            v.x += amount * tanhden * tanhsinh;
            v.y += amount * tanhden * tanhsin;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let den = cos(D(3) * a.y) + cosh(D(1) * a.x);
if (den != 0.0) {
  let tanhden = 1.0 / den;
  v.x += AMOUNT * tanhden * sinh(D(0) * a.x);
  v.y += AMOUNT * tanhden * sin(D(2) * a.y);
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    simple(
        "tanhq",
        true,
        |_, _ctx, a, v, amount| {
            let abs_v = a.y.hypot(a.z);
            let sysz = sqr(a.y) + sqr(a.z);
            let ni = amount / (sqr(a.x) + sysz);
            let (s, c) = abs_v.sin_cos();
            let (sh, ch) = (a.x.sinh(), a.x.cosh());
            let cc = ch * s / abs_v;
            let b = sh * s / abs_v;
            let stcv = sh * c;
            let nstcv = -stcv;
            let ctcv = ch * c;
            v.x += (stcv * ctcv + cc * b * sysz) * ni;
            v.y += (nstcv * b * a.y + cc * a.y * ctcv) * ni;
            v.z += (nstcv * b * a.z + cc * a.z * ctcv) * ni;
        },
        "let abs_v = sqrt(a.y * a.y + a.z * a.z);
let sysz = sqr(a.y) + sqr(a.z);
let ni = AMOUNT / (sqr(a.x) + sysz);
let s = sin(abs_v);
let c = cos(abs_v);
let sh = sinh(a.x);
let ch = cosh(a.x);
let cc = ch * s / abs_v;
let b = sh * s / abs_v;
let stcv = sh * c;
let ctcv = ch * c;
v.x += (stcv * ctcv + cc * b * sysz) * ni;
v.y += (-stcv * b * a.y + cc * a.y * ctcv) * ni;
v.z += (-stcv * b * a.z + cc * a.z * ctcv) * ni;",
    ),
    simple(
        "tanq",
        true,
        |_, _ctx, a, v, amount| {
            let abs_v = a.y.hypot(a.z);
            let s = a.x.sin();
            let sysz = sqr(a.y) + sqr(a.z);
            let ni = amount / (sqr(a.x) + sysz);
            let c = a.x.cos();
            let (sh, ch) = (abs_v.sinh(), abs_v.cosh());
            let cc = c * sh / abs_v;
            let b = -s * sh / abs_v;
            let stcv = s * ch;
            let nstcv = -stcv;
            let ctcv = c * ch;
            v.x += (stcv * ctcv + cc * b * sysz) * ni;
            v.y += (nstcv * b * a.y + cc * a.y * ctcv) * ni;
            v.z += (nstcv * b * a.z + cc * a.z * ctcv) * ni;
        },
        "let abs_v = sqrt(a.y * a.y + a.z * a.z);
let s = sin(a.x);
let sysz = sqr(a.y) + sqr(a.z);
let ni = AMOUNT / (sqr(a.x) + sysz);
let c = cos(a.x);
let sh = sinh(abs_v);
let ch = cosh(abs_v);
let cc = c * sh / abs_v;
let b = -s * sh / abs_v;
let stcv = s * ch;
let ctcv = c * ch;
v.x += (stcv * ctcv + cc * b * sysz) * ni;
v.y += (-stcv * b * a.y + cc * a.y * ctcv) * ni;
v.z += (-stcv * b * a.z + cc * a.z * ctcv) * ni;",
    ),
    VarDef {
        name: "taper",
        params: &[ParamDef::new("strength", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let tp_scale = java_max(1.0 - d[0] * a.y.abs(), 0.01);
            v.x += amount * a.x * tp_scale;
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let tp_scale = max(1.0 - D(0) * abs(a.y), 0.01);
v.x += AMOUNT * a.x * tp_scale;
v.y += AMOUNT * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "target",
        // JWildfire initializes the parameters randomly (even, odd: pi + rnd, size: 0.5 + 0.75 rnd).
        params: &[ParamDef::new("even", M_PI + 0.5), ParamDef::new("odd", M_PI + 0.5), ParamDef::new("size", 0.875)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let mut ang = a.y.atan2(a.x);
            let r = (sqr(a.x) + sqr(a.y)).sqrt();
            let t_size_2 = 0.5 * d[2];
            let mut t = r.ln();
            if t < 0.0 {
                t -= t_size_2;
            }
            t = fmod(t.abs(), d[2]);
            if t < t_size_2 {
                ang += d[0];
            } else {
                ang += d[1];
            }
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "var ang = atan2(a.y, a.x);
let r = sqrt(sqr(a.x) + sqr(a.y));
let t_size_2 = 0.5 * D(2);
var t = log(r);
if (t < 0.0) { t -= t_size_2; }
t = fmod(abs(t), D(2));
if (t < t_size_2) { ang += D(0); } else { ang += D(1); }
v.x += r * cos(ang);
v.y += r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "target_sp",
        // JWildfire initializes size (0.5 + 3 rnd) and tightness (0.5 + 0.25 rnd) randomly.
        params: &[ParamDef::new("twist", 0.0), ParamDef::int("n_of_sp", 1.0, -10.0, 10.0), ParamDef::new("size", 2.0), ParamDef::new("tightness", 0.625)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [twist, n_of_sp, size, tightness, t_size_2, rota, rotb]
        apply: |d, ctx, a, v, amount| {
            let mut ang = a.y.atan2(a.x);
            let r = (sqr(a.x) + sqr(a.y)).sqrt();
            let mut t = d[3] * r.ln() + d[1] * M_1_PI * (ang + M_PI);
            if t < 0.0 {
                t -= d[4];
            }
            t = fmod(t.abs(), d[2]);
            if t < d[4] {
                ang += d[5];
            } else {
                ang += d[6];
            }
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let rota = M_PI * p[0];
            vec![p[0], p[1] as i32 as f64, p[2], p[3], 0.5 * p[2], rota, -M_PI + rota]
        }),
        wgsl: "var ang = atan2(a.y, a.x);
let r = sqrt(sqr(a.x) + sqr(a.y));
var t = D(3) * log(r) + D(1) * (1.0 / PI) * (ang + PI);
if (t < 0.0) { t -= D(4); }
t = fmod(abs(t), D(2));
if (t < D(4)) { ang += D(5); } else { ang += D(6); }
v.x += r * cos(ang);
v.y += r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "taurus",
        params: &[ParamDef::new("r", 3.0), ParamDef::new("n", 5.0), ParamDef::new("inv", 1.5), ParamDef::new("sor", 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            let (r, n, inv, sor) = (d[0], d[1], d[2], d[3]);
            let (sx, cx) = a.x.sin_cos();
            let (sy, cy) = a.y.sin_cos();
            let ir = (inv * r) + ((1.0 - inv) * (r * (n * a.x).cos()));
            v.x += amount * (cx * (ir + sy));
            v.y += amount * (sx * (ir + sy));
            v.z += amount * (sor * cy) + ((1.0 - sor) * a.y);
        },
        prepare: None,
        wgsl: "let sy = sin(a.y);
let ir = (D(2) * D(0)) + ((1.0 - D(2)) * (D(0) * cos(D(1) * a.x)));
v.x += AMOUNT * (cos(a.x) * (ir + sy));
v.y += AMOUNT * (sin(a.x) * (ir + sy));
v.z += AMOUNT * (D(3) * cos(a.y)) + ((1.0 - D(3)) * a.y);",
    },
    VarDef {
        name: "tentacle",
        params: &[ParamDef::new("freq", 2.0), ParamDef::new("amp", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x) + d[1] * (d[0] * r).sin();
            v.x += amount * r * theta.cos();
            v.y += amount * r * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x) + D(1) * sin(D(0) * r);
v.x += AMOUNT * r * cos(theta);
v.y += AMOUNT * r * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tessellate",
        params: &[ParamDef::new("scale", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let s = java_max(d[0].abs(), 0.01);
            let mut nx = (a.x + s * 0.5) % s;
            if nx < 0.0 {
                nx += s;
            }
            nx -= s * 0.5;
            let mut ny = (a.y + s * 0.5) % s;
            if ny < 0.0 {
                ny += s;
            }
            ny -= s * 0.5;
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let s = max(abs(D(0)), 0.01);
var nx = fmod(a.x + s * 0.5, s);
if (nx < 0.0) { nx += s; }
nx -= s * 0.5;
var ny = fmod(a.y + s * 0.5, s);
if (ny < 0.0) { ny += s; }
ny -= s * 0.5;
v.x += AMOUNT * nx;
v.y += AMOUNT * ny;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tess_shape",
        params: &[
            ParamDef::new("strength", 1.0),
            ParamDef::new("shapeSize", 0.8),
            ParamDef::int("shapeType", 0.0, 0.0, 7.0),
            ParamDef::new("shapeAspectRatio", 1.0),
            ParamDef::new("lrRadialAmp", 0.2),
            ParamDef::new("lrRadialWidthFreq", 0.6),
            ParamDef::new("tbAmp", 0.2),
            ParamDef::new("tbWidthFreq", 0.6),
            ParamDef::new("lrRadialPosPhaseDuty", 0.0),
            ParamDef::new("tbPosPhaseDuty", 0.0),
            ParamDef::int("lrRadialProfileType", 0.0, 0.0, 4.0),
            ParamDef::int("tbProfileType", 0.0, 0.0, 4.0),
            ParamDef::int("operationMode", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: tess_shape_apply,
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[2] = limit_value(ftoi(p[2]), 0, 7) as f64;
            d[10] = limit_value(ftoi(p[10]), 0, 4) as f64;
            d[11] = limit_value(ftoi(p[11]), 0, 4) as f64;
            d[12] = limit_value(ftoi(p[12]), 0, 1) as f64;
            d
        }),
        wgsl: TESS_SHAPE_WGSL,
    },
    VarDef {
        name: "threeply",
        params: &[ParamDef::new("a", -55.0), ParamDef::new("b", -1.0), ParamDef::new("c", -42.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // Java follows one orbit from (0, 0) per instance; here one per render thread.
            let p = ctx.orbits.entry(d.as_ptr() as usize).or_insert([0.0, 0.0]);
            let (x0, y0) = (p[0], p[1]);
            let (pa, pb, pc) = (d[0], d[1], d[2]);
            let sgn = if x0 > 0.0 { 1.0 } else if x0 < 0.0 { -1.0 } else { 0.0 };
            let xn = y0 - sgn * (x0.sin() * pb.cos() + pc - x0 * (pa + pb + pc).sin()).abs();
            let yn = pa - x0;
            *p = [xn, yn];
            v.x = 0.001 * xn * amount;
            v.y = 0.001 * yn * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        // GPU: no state across calls, every call returns a random point (1..1024) of the orbit.
        wgsl: "var x0 = 0.0;
var y0 = 0.0;
let n = 1 + min(i32(rnd() * 1024.0), 1023);
let cb = cos(D(1));
let sabc = sin(D(0) + D(1) + D(2));
for (var i = 0; i < n; i++) {
  let xn = y0 - sign(x0) * abs(sin(x0) * cb + D(2) - x0 * sabc);
  y0 = D(0) - x0;
  x0 = xn;
}
v.x = 0.001 * x0 * AMOUNT;
v.y = 0.001 * y0 * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "threepoint_js",
        false,
        |_, ctx, a, v, amount| {
            let (x, y);
            if ctx.random() < 0.333 {
                x = a.x / 2.0 - a.y / 2.0 + 0.5;
                y = -a.x / 2.0 - a.y / 2.0 + 0.5;
            } else if ctx.random() < 0.666 {
                x = a.y;
                y = a.x;
            } else {
                x = -a.y / 2.0 + 0.5;
                y = -a.x / 2.0 + 0.5;
            }
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        "var x = -a.y / 2.0 + 0.5;
var y = -a.x / 2.0 + 0.5;
if (rnd() < 0.333) {
  x = a.x / 2.0 - a.y / 2.0 + 0.5;
  y = -a.x / 2.0 - a.y / 2.0 + 0.5;
} else if (rnd() < 0.666) {
  x = a.y;
  y = a.x;
}
v.x += x * AMOUNT;
v.y += y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    // @@NEXT@@
];

/// `TessShapeFunc.calculate_profile_offset`
fn tess_shape_profile(v: f64, profile_type: i32, amp: f64, wf: f64, ppd: f64) -> f64 {
    if amp == 0.0 {
        return 0.0;
    }
    if profile_type != 0 && wf.abs() <= 1e-6 {
        return 0.0;
    }
    match profile_type {
        0 => {
            if wf <= 1e-6 {
                return 0.0;
            }
            let ev = v - ppd;
            let half = wf / 2.0;
            if ev.abs() < half { amp * (1.0 + (ev / half * M_PI).cos()) / 2.0 } else { 0.0 }
        }
        1 => amp * (v * wf * M_2PI + ppd * M_2PI).sin(),
        2 => {
            let t = v * wf + ppd;
            let pt = t - t.floor();
            let y = if pt < 0.5 { 2.0 * pt } else { 2.0 * (1.0 - pt) };
            amp * (y * 2.0 - 1.0)
        }
        3 => {
            let duty = java_max(0.01, java_min(0.99, ppd.abs()));
            let t = v * wf;
            let pt = t - t.floor();
            if pt < duty { amp } else { -amp }
        }
        4 => {
            let t = v * wf + ppd;
            amp * (2.0 * (t - t.floor()) - 1.0)
        }
        _ => 0.0,
    }
}

/// `TessShapeFunc.isPointInTriangle`
#[allow(clippy::too_many_arguments)]
fn tess_shape_in_triangle(px: f64, py: f64, v1x: f64, v1y: f64, v2x: f64, v2y: f64, v3x: f64, v3y: f64) -> bool {
    let d1 = (px - v2x) * (v1y - v2y) - (v1x - v2x) * (py - v2y);
    let d2 = (px - v3x) * (v2y - v3y) - (v2x - v3x) * (py - v3y);
    let d3 = (px - v1x) * (v3y - v1y) - (v3x - v1x) * (py - v1y);
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

fn tess_shape_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (strength, size, shape_type, aspect) = (d[0], d[1], d[2] as i32, d[3]);
    let lr = |x: f64| tess_shape_profile(x, d[10] as i32, d[4], d[5], d[8]);
    let tb = |x: f64| tess_shape_profile(x, d[11] as i32, d[6], d[7], d[9]);
    let (lr_pt, tb_pt) = (d[10] as i32, d[11] as i32);
    let (x_aff, y_aff) = (a.x, a.y);
    let base_h = size;
    let base_w = size * aspect;
    let half_h = if base_h == 0.0 { 1e-9 } else { base_h / 2.0 };
    let half_w = if base_w == 0.0 { 1e-9 } else { base_w / 2.0 };
    let norm = |p: f64, half: f64| if half == 0.0 { 0.0 } else { java_min(1.0, java_max(-1.0, p / half)) };
    let sqrt3 = 3f64.sqrt();
    if d[12] as i32 == 1 {
        let (px, py) = (x_aff, y_aff);
        let inside = match shape_type {
            0 => {
                let s_ext = size / 2.0;
                px.abs() <= s_ext && py.abs() <= s_ext
            }
            1 => px.abs() <= half_w && py.abs() <= half_h,
            2 => {
                let s = size;
                let h = s * sqrt3 / 2.0;
                tess_shape_in_triangle(px, py, 0.0, (2.0 / 3.0) * h, -s / 2.0, -(1.0 / 3.0) * h, s / 2.0, -(1.0 / 3.0) * h)
            }
            3 => {
                let s = size;
                (0..6).any(|i| {
                    let (a1, a2) = (i as f64 * M_PI / 3.0, (i + 1) as f64 * M_PI / 3.0);
                    tess_shape_in_triangle(px, py, 0.0, 0.0, s * a1.cos(), s * a1.sin(), s * a2.cos(), s * a2.sin())
                })
            }
            4 => {
                let (pyn, pxn) = (norm(py, half_h), norm(px, half_w));
                let lr_off = lr(if lr_pt == 0 { pyn } else { (pyn + 1.0) / 2.0 });
                let tb_off = tb(if tb_pt == 0 { pxn } else { (pxn + 1.0) / 2.0 });
                px >= -half_w + lr_off && px <= half_w - lr_off && py >= -half_h + tb_off && py <= half_h - tb_off
            }
            5 => {
                let (pyn, pxn) = (norm(py, half_h), norm(px, half_w));
                let dx = lr(if lr_pt == 0 { pyn } else { (pyn + 1.0) / 2.0 });
                let dy = tb(if tb_pt == 0 { pxn } else { (pxn + 1.0) / 2.0 });
                let eps = 1e-7;
                (px - dx).abs() <= half_w + eps && (py - dy).abs() <= half_h + eps
            }
            6 => {
                let (pyn, pxn) = (norm(py, half_h), norm(px, half_w));
                let reo = lr(if lr_pt == 0 { pyn } else { (pyn + 1.0) / 2.0 });
                let teo = tb(if tb_pt == 0 { pxn } else { (pxn + 1.0) / 2.0 });
                px >= -half_w && px <= half_w + reo && py >= -half_h && py <= half_h + teo
            }
            7 => {
                let angle = py.atan2(px);
                let radius = (px * px + py * py).sqrt();
                let ang01 = (angle / M_2PI + 1.0) % 1.0;
                let off = lr(if lr_pt == 0 { ang01 * 2.0 - 1.0 } else { ang01 });
                radius <= java_max(0.0, size + off)
            }
            _ => false,
        };
        if inside {
            v.do_hide = false;
            v.x = x_aff;
            v.y = y_aff;
        } else {
            v.do_hide = true;
        }
        ctx.pass_z(a, v, amount);
        return;
    }
    let (mut tx, mut ty) = (0.0, 0.0);
    match shape_type {
        0 => {
            tx = (ctx.random() * 2.0 - 1.0) * (size / 2.0);
            ty = (ctx.random() * 2.0 - 1.0) * (size / 2.0);
        }
        1 => {
            tx = (ctx.random() * 2.0 - 1.0) * half_w;
            ty = (ctx.random() * 2.0 - 1.0) * half_h;
        }
        2 => {
            let s = size;
            let h = s * sqrt3 / 2.0;
            let (v1x, v1y, v2x, v2y, v3x, v3y) = (0.0, (2.0 / 3.0) * h, -s / 2.0, -(1.0 / 3.0) * h, s / 2.0, -(1.0 / 3.0) * h);
            let (mut r1, mut r2) = (ctx.random(), ctx.random());
            if r1 + r2 > 1.0 {
                r1 = 1.0 - r1;
                r2 = 1.0 - r2;
            }
            let r3 = 1.0 - r1 - r2;
            tx = r1 * v1x + r2 * v2x + r3 * v3x;
            ty = r1 * v1y + r2 * v2y + r3 * v3y;
        }
        3 => {
            let s = size;
            let ti = (ctx.random() * 6.0).floor() as i32;
            let ao = ti as f64 * M_PI / 3.0;
            let (v1x, v1y, v2x, v2y) = (s * ao.cos(), s * ao.sin(), s * (ao + M_PI / 3.0).cos(), s * (ao + M_PI / 3.0).sin());
            let (mut r1, mut r2) = (ctx.random(), ctx.random());
            if r1 + r2 > 1.0 {
                r1 = 1.0 - r1;
                r2 = 1.0 - r2;
            }
            tx = (1.0 - r1 - r2) * 0.0 + r1 * v1x + r2 * v2x;
            ty = (1.0 - r1 - r2) * 0.0 + r1 * v1y + r2 * v2y;
        }
        4 => {
            let (sx, sy) = (half_w + d[4].abs(), half_h + d[6].abs());
            for _ in 0..100 {
                let px = (ctx.random() * 2.0 - 1.0) * sx;
                let py = (ctx.random() * 2.0 - 1.0) * sy;
                let (pyn, pxn) = (norm(py, half_h), norm(px, half_w));
                let lro = lr(if lr_pt == 0 { pyn } else { (pyn + 1.0) / 2.0 });
                let tbo = tb(if tb_pt == 0 { pxn } else { (pxn + 1.0) / 2.0 });
                if px >= -half_w + lro && px <= half_w - lro && py >= -half_h + tbo && py <= half_h - tbo {
                    tx = px;
                    ty = py;
                    break;
                }
            }
        }
        5 => {
            let rxb = (ctx.random() * 2.0 - 1.0) * half_w;
            let ryb = (ctx.random() * 2.0 - 1.0) * half_h;
            let rynd = if half_h == 0.0 { 0.0 } else { ryb / half_h };
            let rxnd = if half_w == 0.0 { 0.0 } else { rxb / half_w };
            tx = rxb + lr(if lr_pt == 0 { rynd } else { (rynd + 1.0) / 2.0 });
            ty = ryb + tb(if tb_pt == 0 { rxnd } else { (rxnd + 1.0) / 2.0 });
        }
        6 => {
            let (xmb, xab, ymb, yab) = (-half_w, half_w + d[4].abs(), -half_h, half_h + d[6].abs());
            for _ in 0..100 {
                let px = xmb + ctx.random() * (xab - xmb);
                let py = ymb + ctx.random() * (yab - ymb);
                if px < -half_w || py < -half_h {
                    continue;
                }
                let (pyn, pxn) = (norm(py, half_h), norm(px, half_w));
                let reo = lr(if lr_pt == 0 { pyn } else { (pyn + 1.0) / 2.0 });
                if px > half_w + reo {
                    continue;
                }
                let teo = tb(if tb_pt == 0 { pxn } else { (pxn + 1.0) / 2.0 });
                if py > half_h + teo {
                    continue;
                }
                tx = px;
                ty = py;
                break;
            }
        }
        7 => {
            let ang = ctx.random() * M_2PI;
            let ang01 = ang / M_2PI;
            let off = lr(if lr_pt == 0 { ang01 * 2.0 - 1.0 } else { ang01 });
            let r = ctx.random().sqrt() * java_max(0.0, size + off);
            tx = r * ang.cos();
            ty = r * ang.sin();
        }
        _ => {}
    }
    let c = java_max(0.0, java_min(1.0, amount * strength));
    v.x = (1.0 - c) * x_aff + c * tx;
    v.y = (1.0 - c) * y_aff + c * ty;
    ctx.pass_z(a, v, amount);
}

const TESS_SHAPE_WGSL: &str = "fn tess_shape_profile(v: f32, profile_type: i32, amp: f32, wf: f32, ppd: f32) -> f32 {
  if (amp == 0.0) { return 0.0; }
  if (profile_type != 0 && abs(wf) <= 1e-6) { return 0.0; }
  switch (profile_type) {
    case 0: {
      if (wf <= 1e-6) { return 0.0; }
      let ev = v - ppd;
      let hwid = wf / 2.0;
      if (abs(ev) < hwid) { return amp * (1.0 + cos(ev / hwid * PI)) / 2.0; }
      return 0.0;
    }
    case 1: { return amp * sin(v * wf * TWO_PI + ppd * TWO_PI); }
    case 2: {
      let t = v * wf + ppd;
      let pt = t - floor(t);
      let y = select(2.0 * (1.0 - pt), 2.0 * pt, pt < 0.5);
      return amp * (y * 2.0 - 1.0);
    }
    case 3: {
      let duty = max(0.01, min(0.99, abs(ppd)));
      let t = v * wf;
      return select(-amp, amp, t - floor(t) < duty);
    }
    case 4: {
      let t = v * wf + ppd;
      return amp * (2.0 * (t - floor(t)) - 1.0);
    }
    default: { return 0.0; }
  }
}
fn tess_shape_in_triangle(p: vec2<f32>, v1: vec2<f32>, v2: vec2<f32>, v3: vec2<f32>) -> bool {
  let d1 = (p.x - v2.x) * (v1.y - v2.y) - (v1.x - v2.x) * (p.y - v2.y);
  let d2 = (p.x - v3.x) * (v2.y - v3.y) - (v2.x - v3.x) * (p.y - v3.y);
  let d3 = (p.x - v1.x) * (v3.y - v1.y) - (v3.x - v1.x) * (p.y - v1.y);
  return !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0));
}
fn tess_shape_lr(base: u32, x: f32) -> f32 {
  return tess_shape_profile(x, i32(prm[base + 10u]), prm[base + 4u], prm[base + 5u], prm[base + 8u]);
}
fn tess_shape_tb(base: u32, x: f32) -> f32 {
  return tess_shape_profile(x, i32(prm[base + 11u]), prm[base + 6u], prm[base + 7u], prm[base + 9u]);
}
fn tess_shape_norm(p: f32, hwid: f32) -> f32 {
  if (hwid == 0.0) { return 0.0; }
  return min(1.0, max(-1.0, p / hwid));
}
fn tess_shape_vin(n: f32, pt: i32) -> f32 {
  if (pt == 0) { return n; }
  return (n + 1.0) / 2.0;
}
@body
let size = D(1);
let shape_type = i32(D(2));
let lr_pt = i32(D(10));
let tb_pt = i32(D(11));
let x_aff = a.x;
let y_aff = a.y;
let base_w = size * D(3);
var half_h = size / 2.0;
if (size == 0.0) { half_h = 1e-9; }
var half_w = base_w / 2.0;
if (base_w == 0.0) { half_w = 1e-9; }
let sqrt3 = sqrt(3.0);
if (i32(D(12)) == 1) {
  let px = x_aff;
  let py = y_aff;
  var inside = false;
  switch (shape_type) {
    case 0: { inside = abs(px) <= size / 2.0 && abs(py) <= size / 2.0; }
    case 1: { inside = abs(px) <= half_w && abs(py) <= half_h; }
    case 2: {
      let h = size * sqrt3 / 2.0;
      inside = tess_shape_in_triangle(vec2<f32>(px, py), vec2<f32>(0.0, (2.0 / 3.0) * h), vec2<f32>(-size / 2.0, -(1.0 / 3.0) * h), vec2<f32>(size / 2.0, -(1.0 / 3.0) * h));
    }
    case 3: {
      for (var i = 0; i < 6; i++) {
        let a1 = f32(i) * PI / 3.0;
        let a2 = f32(i + 1) * PI / 3.0;
        if (tess_shape_in_triangle(vec2<f32>(px, py), vec2<f32>(0.0), size * vec2<f32>(cos(a1), sin(a1)), size * vec2<f32>(cos(a2), sin(a2)))) { inside = true; break; }
      }
    }
    case 4: {
      let lr_off = tess_shape_lr(base, tess_shape_vin(tess_shape_norm(py, half_h), lr_pt));
      let tb_off = tess_shape_tb(base, tess_shape_vin(tess_shape_norm(px, half_w), tb_pt));
      inside = px >= -half_w + lr_off && px <= half_w - lr_off && py >= -half_h + tb_off && py <= half_h - tb_off;
    }
    case 5: {
      let dx = tess_shape_lr(base, tess_shape_vin(tess_shape_norm(py, half_h), lr_pt));
      let dy = tess_shape_tb(base, tess_shape_vin(tess_shape_norm(px, half_w), tb_pt));
      inside = abs(px - dx) <= half_w + 1e-7 && abs(py - dy) <= half_h + 1e-7;
    }
    case 6: {
      let reo = tess_shape_lr(base, tess_shape_vin(tess_shape_norm(py, half_h), lr_pt));
      let teo = tess_shape_tb(base, tess_shape_vin(tess_shape_norm(px, half_w), tb_pt));
      inside = px >= -half_w && px <= half_w + reo && py >= -half_h && py <= half_h + teo;
    }
    case 7: {
      let ang01 = fmod(atan2(py, px) / TWO_PI + 1.0, 1.0);
      var vin = ang01;
      if (lr_pt == 0) { vin = ang01 * 2.0 - 1.0; }
      inside = sqrt(px * px + py * py) <= max(0.0, size + tess_shape_lr(base, vin));
    }
    default: {}
  }
  if (inside) {
    v.hide = 0.0;
    v.x = x_aff;
    v.y = y_aff;
  } else {
    v.hide = 1.0;
  }
} else {
  var tx = 0.0;
  var ty = 0.0;
  switch (shape_type) {
    case 0: {
      tx = (rnd() * 2.0 - 1.0) * (size / 2.0);
      ty = (rnd() * 2.0 - 1.0) * (size / 2.0);
    }
    case 1: {
      tx = (rnd() * 2.0 - 1.0) * half_w;
      ty = (rnd() * 2.0 - 1.0) * half_h;
    }
    case 2: {
      let h = size * sqrt3 / 2.0;
      var r1 = rnd();
      var r2 = rnd();
      if (r1 + r2 > 1.0) { r1 = 1.0 - r1; r2 = 1.0 - r2; }
      let r3 = 1.0 - r1 - r2;
      tx = r2 * (-size / 2.0) + r3 * (size / 2.0);
      ty = r1 * (2.0 / 3.0) * h + r2 * (-(1.0 / 3.0) * h) + r3 * (-(1.0 / 3.0) * h);
    }
    case 3: {
      let ao = floor(rnd() * 6.0) * PI / 3.0;
      var r1 = rnd();
      var r2 = rnd();
      if (r1 + r2 > 1.0) { r1 = 1.0 - r1; r2 = 1.0 - r2; }
      tx = r1 * size * cos(ao) + r2 * size * cos(ao + PI / 3.0);
      ty = r1 * size * sin(ao) + r2 * size * sin(ao + PI / 3.0);
    }
    case 4: {
      let sx = half_w + abs(D(4));
      let sy = half_h + abs(D(6));
      for (var i = 0; i < 100; i++) {
        let px = (rnd() * 2.0 - 1.0) * sx;
        let py = (rnd() * 2.0 - 1.0) * sy;
        let lro = tess_shape_lr(base, tess_shape_vin(tess_shape_norm(py, half_h), lr_pt));
        let tbo = tess_shape_tb(base, tess_shape_vin(tess_shape_norm(px, half_w), tb_pt));
        if (px >= -half_w + lro && px <= half_w - lro && py >= -half_h + tbo && py <= half_h - tbo) { tx = px; ty = py; break; }
      }
    }
    case 5: {
      let rxb = (rnd() * 2.0 - 1.0) * half_w;
      let ryb = (rnd() * 2.0 - 1.0) * half_h;
      tx = rxb + tess_shape_lr(base, tess_shape_vin(ryb / half_h, lr_pt));
      ty = ryb + tess_shape_tb(base, tess_shape_vin(rxb / half_w, tb_pt));
    }
    case 6: {
      let xab = half_w + abs(D(4));
      let yab = half_h + abs(D(6));
      for (var i = 0; i < 100; i++) {
        let px = -half_w + rnd() * (xab + half_w);
        let py = -half_h + rnd() * (yab + half_h);
        if (px < -half_w || py < -half_h) { continue; }
        let reo = tess_shape_lr(base, tess_shape_vin(tess_shape_norm(py, half_h), lr_pt));
        if (px > half_w + reo) { continue; }
        let teo = tess_shape_tb(base, tess_shape_vin(tess_shape_norm(px, half_w), tb_pt));
        if (py > half_h + teo) { continue; }
        tx = px;
        ty = py;
        break;
      }
    }
    case 7: {
      let ang = rnd() * TWO_PI;
      let ang01 = ang / TWO_PI;
      var vin = ang01;
      if (lr_pt == 0) { vin = ang01 * 2.0 - 1.0; }
      let r = sqrt(rnd()) * max(0.0, size + tess_shape_lr(base, vin));
      tx = r * cos(ang);
      ty = r * sin(ang);
    }
    default: {}
  }
  let c = clamp(AMOUNT * D(0), 0.0, 1.0);
  v.x = (1.0 - c) * x_aff + c * tx;
  v.y = (1.0 - c) * y_aff + c * ty;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

const SYNTH_EPS: f64 = SMALL_EPSILON;

/// `SynthFunc.synth_value`
fn synth_value(d: &[f64], theta: f64) -> f64 {
    let (a, mix) = (d[0], d[3]);
    let mut tf = a;
    let mut x = 0.0;
    for k in 0..5 {
        let l = &d[5 + 6 * k..11 + 6 * k];
        let amp = l[0];
        if amp == 0.0 {
            continue;
        }
        let frq = l[3];
        let mut z = l[4] + theta * frq;
        let mut y = z / (2.0 * M_PI);
        y -= y.floor();
        if l[2] != 0.0 {
            z = 0.5 + 0.5 * l[2];
            if y > z {
                y = 0.5 + 0.5 * (y - z) / (1.0 - z + SYNTH_EPS);
            } else {
                y = 0.5 - 0.5 * (z - y) / (z + SYNTH_EPS);
            }
        }
        match l[1] as i32 {
            0 => x = (y * 2.0 * M_PI).sin(),
            1 => x = (y * 2.0 * M_PI).cos(),
            2 => x = if y > 0.5 { 1.0 } else { -1.0 },
            3 => x = 1.0 - 2.0 * y,
            4 => x = if y > 0.5 { 3.0 - 4.0 * y } else { 2.0 * y - 1.0 },
            5 => x = 8.0 * (y - 0.5) * (y - 0.5) - 1.0,
            6 => x = 2.0 * y.sqrt() - 1.0,
            7 => {
                y -= 0.5;
                y *= 2.0 * M_PI / frq;
                x = 1.0 / (y.cos() + SYNTH_EPS) - 1.0;
            }
            8 => {
                y -= 0.5;
                y *= 2.0 * M_PI / frq;
                z = y.cos();
                x = z / (1.0 + SYNTH_EPS - z);
            }
            _ => {}
        }
        match l[5] as i32 {
            0 => tf += amp * x,
            1 => tf *= 1.0 + amp * x,
            2 => {
                z = a + amp * x;
                tf = if tf > z { tf } else { z };
            }
            3 => {
                z = a + amp * x;
                tf = if tf < z { tf } else { z };
            }
            _ => {}
        }
    }
    tf * mix + (1.0 - mix)
}

/// `SynthFunc.bezier_quad_map`
fn synth_bezier_quad_map(mut x: f64, mut m: f64) -> f64 {
    let mut a = 1.0;
    if m < 0.0 {
        m = -m;
        a = -1.0;
    }
    if x < 0.0 {
        x = -x;
        a = -a;
    }
    let im = if m > 1.0e-10 { 1.0 / m } else { 1e10 };
    let l = if (2.0 - m) > 2.0 * m { 2.0 - m } else { 2.0 * m };
    if x > l || m == 1.0 {
        return a * x;
    }
    if m < 1.0 && x <= 1.0 {
        let mut t = x;
        if (m - 0.5) * (m - 0.5) > 1e-10 {
            t = (-m + (m * m + (1.0 - 2.0 * m) * x).sqrt()) / (1.0 - 2.0 * m);
        }
        return a * (x + (m - 1.0) * t * t);
    }
    if 1.0 < m && x <= 1.0 {
        let mut t = x;
        if (m - 2.0) * (m - 2.0) > 1e-10 {
            t = (-im + (im * im + (1.0 - 2.0 * im) * x).sqrt()) / (1.0 - 2.0 * im);
        }
        return a * (x + (m - 1.0) * t * t);
    }
    if m < 1.0 {
        let t = ((x - 1.0) / (l - 1.0)).sqrt();
        return a * (x + (m - 1.0) * t * t + 2.0 * (1.0 - m) * t + (m - 1.0));
    }
    let t = (1.0 - m) + ((m - 1.0) * (m - 1.0) + (x - 1.0)).sqrt();
    a * (x + (m - 1.0) * t * t - 2.0 * (m - 1.0) * t + (m - 1.0))
}

fn synth_interpolate(x: f64, m: f64, lerp_type: i32) -> f64 {
    if lerp_type == 1 { synth_bezier_quad_map(x, m) } else { x * m }
}

/// `SynthFunc.synthsincos` -> (s, c)
fn synth_sincos(d: &[f64], theta: f64, sine_type: i32) -> (f64, f64) {
    let (s, c) = theta.sin_cos();
    match sine_type {
        0 => (s * synth_value(d, theta), c * synth_value(d, theta + M_PI / 2.0)),
        1 => ((1.0 - d[3]) * s + (synth_value(d, theta) - 1.0), (1.0 - d[3]) * c + (synth_value(d, theta + M_PI / 2.0) - 1.0)),
        _ => (s, c),
    }
}

fn synth_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (power, mix, smooth) = (d[2], d[3], d[4] as i32);
    let eps = SYNTH_EPS;
    let (mut vx, mut vy) = (a.x, a.y);
    match d[1] as i32 {
        5 => {
            let mut radius = (vx * vx + vy * vy).sqrt();
            let theta = vx.atan2(vy);
            radius = synth_interpolate(radius, synth_value(d, theta), smooth);
            v.x += amount * radius * theta.sin();
            v.y += amount * radius * theta.cos();
        }
        7 => {
            let tf = synth_value(d, vx);
            v.x += amount * vx;
            v.y += amount * synth_interpolate(vy, tf, smooth);
        }
        6 => {
            let tf = synth_value(d, vy);
            v.x += amount * synth_interpolate(vx, tf, smooth);
            v.y += amount * vy;
        }
        8 => {
            let tf = synth_value(d, vy);
            v.x += amount * synth_interpolate(vx, tf, smooth);
            let tf = synth_value(d, vx);
            v.y += amount * synth_interpolate(vy, tf, smooth);
        }
        0 => {
            let mut radius = (vx * vx + vy * vy + eps).powf((power + 1.0) / 2.0);
            let theta = vx.atan2(vy);
            radius = synth_interpolate(radius, synth_value(d, theta), smooth);
            v.x += amount * radius * theta.sin();
            v.y += amount * radius * theta.cos();
        }
        1 => {
            let mut radius = (vx * vx + vy * vy).sqrt() / ((vx * vx + vy * vy) / 4.0 + 1.0);
            let theta = vx.atan2(vy);
            radius = synth_interpolate(radius, synth_value(d, theta), smooth);
            v.x += amount * radius * theta.sin();
            v.y += amount * radius * theta.cos();
        }
        2 => {
            let mut radius = (ctx.random() + ctx.random() + 0.002 * ctx.random()) / 2.002;
            let theta = 2.0 * M_PI * ctx.random() - M_PI;
            vx = radius * theta.sin();
            vy = radius * theta.cos();
            radius = (radius * radius + eps).powf(power / 2.0);
            radius = amount * synth_interpolate(radius, synth_value(d, theta), smooth);
            v.x += vx * radius;
            v.y += vy * radius;
        }
        3 => {
            let mut radius = 0.5 * (ctx.random() + ctx.random());
            let theta = 2.0 * M_PI * ctx.random() - M_PI;
            radius = (radius * radius + eps).powf(-power / 2.0);
            radius = synth_interpolate(radius, synth_value(d, theta), smooth);
            v.x += amount * radius * theta.sin();
            v.y += amount * radius * theta.cos();
        }
        12 => {
            let mut radius = 1.0 + 0.1 * (ctx.random() + ctx.random() - 1.0) * power;
            let theta = 2.0 * M_PI * ctx.random() - M_PI;
            radius = synth_interpolate(radius, synth_value(d, theta), smooth);
            v.x += amount * radius * theta.sin();
            v.y += amount * radius * theta.cos();
        }
        13 => {
            let theta = 2.0 * M_PI * ctx.random() - M_PI;
            let mut radius = (ctx.random() + eps).powf(power);
            radius = synth_value(d, theta) + 0.1 * radius;
            v.x += amount * radius * theta.sin();
            v.y += amount * radius * theta.cos();
        }
        14 => {
            let radius = (vx * vx + vy * vy + eps).powf(power / 2.0);
            let theta = vx.atan2(vy) - 1.0 + synth_value(d, radius);
            v.x += amount * radius * theta.sin();
            v.y += amount * radius * theta.cos();
        }
        15 => {
            let radius = (vx * vx + vy * vy + eps).powf(power / 2.0);
            let theta = vx.atan2(vy);
            let (s, c) = theta.sin_cos();
            let mu = synth_value(d, radius) - 1.0;
            vx += mu * c;
            vy -= mu * s;
            v.x += amount * vx;
            v.y += amount * vy;
        }
        16 => {
            let radius = (vx * vx + vy * vy + eps).powf(power / 2.0);
            let theta = vx.atan2(vy) - 1.0 + synth_value(d, radius);
            let radius = (vx * vx + vy * vy).sqrt();
            v.x += amount * radius * theta.sin();
            v.y += amount * radius * theta.cos();
        }
        4 => {
            vy = 1.0 + 0.1 * (ctx.random() + ctx.random() - 1.0) * power;
            let theta = 2.0 * ((ctx.random() - 0.5) * 2.0).asin();
            vy = synth_interpolate(vy, synth_value(d, theta), smooth);
            v.x += amount * (theta / M_PI);
            v.y += amount * (vy - 1.0);
        }
        9 => {
            v.x += amount * (vx + synth_value(d, vy) - 1.0);
            v.y += amount * vy;
        }
        10 => {
            v.x += amount * vx;
            v.y += amount * (vy + synth_value(d, vx) - 1.0);
        }
        11 => {
            v.x += amount * (vx + synth_value(d, vy) - 1.0);
            v.y += amount * (vy + synth_value(d, vx) - 1.0);
        }
        1001 => {
            v.x += amount * (synth_value(d, vx) - 1.0 + (1.0 - mix) * vx.sin());
            v.y += amount * (synth_value(d, vy) - 1.0 + (1.0 - mix) * vy.sin());
        }
        1002 => {
            let radius = (vx * vx + vy * vy + eps).powf(power / 2.0);
            let (s, c) = synth_sincos(d, radius, smooth);
            v.x += amount * (s * vx - c * vy);
            v.y += amount * (c * vx + s * vy);
        }
        1003 => {
            let radius = (vx * vx + vy * vy + eps).powf(power / 2.0);
            let (s, c) = synth_sincos(d, vx.atan2(vy), smooth);
            v.x += amount * s / radius;
            v.y += amount * c * radius;
        }
        1004 => {
            let radius = (vx * vx + vy * vy + eps).powf(power / 4.0);
            let mut theta = vx.atan2(vy) / 2.0;
            if ctx.random() < 0.5 {
                theta += M_PI;
            }
            let (s, c) = synth_sincos(d, theta, smooth);
            v.x += amount * radius * c;
            v.y += amount * radius * s;
        }
        1005 => {
            let theta = vx.atan2(vy) / M_PI;
            let radius = M_PI * (vx * vx + vy * vy + eps).powf(power / 2.0);
            let (s, c) = synth_sincos(d, radius, smooth);
            v.x = amount * s * theta;
            v.y = amount * c * theta;
        }
        1006 => {
            let mut radius = (vx * vx + vy * vy).sqrt();
            let theta = vx.atan2(vy);
            let mu = power * power + eps;
            radius += -2.0 * mu * (((radius + mu) / (2.0 * mu)) as i32) as f64 + radius * (1.0 - mu);
            let (s, c) = synth_sincos(d, theta, smooth);
            v.x += amount * s * radius;
            v.y += amount * c * radius;
        }
        1007 => {
            let radius = (vx * vx + vy * vy + eps).powf(power / 2.0);
            let (s, _) = synth_sincos(d, vx, smooth);
            v.x += amount * radius * s;
            v.y += amount * radius * vy;
        }
        17 => {
            let mu = synth_value(d, vx) - 1.0;
            vy = 2.0 * mu - vy;
            v.x += amount * vx;
            v.y += amount * vy;
        }
        18 => {
            let mu = synth_value(d, vx) - 1.0;
            let radius = synth_value(d, vy) - 1.0;
            vy = 2.0 * mu - vy;
            vx = 2.0 * radius - vx;
            v.x += amount * vx;
            v.y += amount * vy;
        }
        19 => {
            let mut radius = (vx * vx + vy * vy).sqrt();
            let theta = vx.atan2(vy);
            radius = synth_interpolate(radius, synth_value(d, theta), smooth);
            radius = radius.powf(power);
            v.x += amount * radius * theta.sin();
            v.y += amount * radius * theta.cos();
        }
        _ => {}
    }
    ctx.pass_z(a, v, amount);
}

const SYNTH_WGSL: &str = "fn synth_value(base: u32, theta: f32) -> f32 {
  let a = prm[base];
  let mixv = prm[base + 3u];
  var tf = a;
  var x = 0.0;
  for (var k = 0u; k < 5u; k++) {
    let o = base + 5u + 6u * k;
    let amp = prm[o];
    if (amp == 0.0) { continue; }
    let skew = prm[o + 2u];
    let frq = prm[o + 3u];
    var z = prm[o + 4u] + theta * frq;
    var y = z / (2.0 * PI);
    y -= floor(y);
    if (skew != 0.0) {
      z = 0.5 + 0.5 * skew;
      if (y > z) { y = 0.5 + 0.5 * (y - z) / (1.0 - z + EPS); } else { y = 0.5 - 0.5 * (z - y) / (z + EPS); }
    }
    switch (i32(prm[o + 1u])) {
      case 0: { x = sin(y * 2.0 * PI); }
      case 1: { x = cos(y * 2.0 * PI); }
      case 2: { x = select(-1.0, 1.0, y > 0.5); }
      case 3: { x = 1.0 - 2.0 * y; }
      case 4: { x = select(2.0 * y - 1.0, 3.0 - 4.0 * y, y > 0.5); }
      case 5: { x = 8.0 * (y - 0.5) * (y - 0.5) - 1.0; }
      case 6: { x = 2.0 * sqrt(y) - 1.0; }
      case 7: { y = (y - 0.5) * (2.0 * PI / frq); x = 1.0 / (cos(y) + EPS) - 1.0; }
      case 8: { y = (y - 0.5) * (2.0 * PI / frq); z = cos(y); x = z / (1.0 + EPS - z); }
      default: {}
    }
    switch (i32(prm[o + 5u])) {
      case 0: { tf += amp * x; }
      case 1: { tf *= 1.0 + amp * x; }
      case 2: { z = a + amp * x; tf = select(z, tf, tf > z); }
      case 3: { z = a + amp * x; tf = select(z, tf, tf < z); }
      default: {}
    }
  }
  return tf * mixv + (1.0 - mixv);
}
fn synth_bezier_quad_map(x_in: f32, m_in: f32) -> f32 {
  var x = x_in;
  var m = m_in;
  var a = 1.0;
  if (m < 0.0) { m = -m; a = -1.0; }
  if (x < 0.0) { x = -x; a = -a; }
  var im = 1e10;
  if (m > 1.0e-10) { im = 1.0 / m; }
  let l = select(2.0 * m, 2.0 - m, (2.0 - m) > 2.0 * m);
  if (x > l || m == 1.0) { return a * x; }
  if (m < 1.0 && x <= 1.0) {
    var t = x;
    if ((m - 0.5) * (m - 0.5) > 1e-10) { t = (-m + sqrt(m * m + (1.0 - 2.0 * m) * x)) / (1.0 - 2.0 * m); }
    return a * (x + (m - 1.0) * t * t);
  }
  if (1.0 < m && x <= 1.0) {
    var t = x;
    if ((m - 2.0) * (m - 2.0) > 1e-10) { t = (-im + sqrt(im * im + (1.0 - 2.0 * im) * x)) / (1.0 - 2.0 * im); }
    return a * (x + (m - 1.0) * t * t);
  }
  if (m < 1.0) {
    let t = sqrt((x - 1.0) / (l - 1.0));
    return a * (x + (m - 1.0) * t * t + 2.0 * (1.0 - m) * t + (m - 1.0));
  }
  let t = (1.0 - m) + sqrt((m - 1.0) * (m - 1.0) + (x - 1.0));
  return a * (x + (m - 1.0) * t * t - 2.0 * (m - 1.0) * t + (m - 1.0));
}
fn synth_interpolate(x: f32, m: f32, lerp_type: i32) -> f32 {
  if (lerp_type == 1) { return synth_bezier_quad_map(x, m); }
  return x * m;
}
fn synth_sincos(base: u32, theta: f32, sine_type: i32) -> vec2<f32> {
  let s = sin(theta);
  let c = cos(theta);
  if (sine_type == 0) { return vec2<f32>(s * synth_value(base, theta), c * synth_value(base, theta + PI / 2.0)); }
  if (sine_type == 1) {
    let mixv = prm[base + 3u];
    return vec2<f32>((1.0 - mixv) * s + (synth_value(base, theta) - 1.0), (1.0 - mixv) * c + (synth_value(base, theta + PI / 2.0) - 1.0));
  }
  return vec2<f32>(s, c);
}
@body
let power = D(2);
let mixv = D(3);
let smooth_t = i32(D(4));
var vx = a.x;
var vy = a.y;
switch (i32(D(1))) {
  case 5: {
    let theta = atan2(vx, vy);
    let radius = synth_interpolate(sqrt(vx * vx + vy * vy), synth_value(base, theta), smooth_t);
    v.x += AMOUNT * radius * sin(theta);
    v.y += AMOUNT * radius * cos(theta);
  }
  case 7: {
    let tf = synth_value(base, vx);
    v.x += AMOUNT * vx;
    v.y += AMOUNT * synth_interpolate(vy, tf, smooth_t);
  }
  case 6: {
    let tf = synth_value(base, vy);
    v.x += AMOUNT * synth_interpolate(vx, tf, smooth_t);
    v.y += AMOUNT * vy;
  }
  case 8: {
    v.x += AMOUNT * synth_interpolate(vx, synth_value(base, vy), smooth_t);
    v.y += AMOUNT * synth_interpolate(vy, synth_value(base, vx), smooth_t);
  }
  case 0: {
    let theta = atan2(vx, vy);
    let radius = synth_interpolate(pow(vx * vx + vy * vy + EPS, (power + 1.0) / 2.0), synth_value(base, theta), smooth_t);
    v.x += AMOUNT * radius * sin(theta);
    v.y += AMOUNT * radius * cos(theta);
  }
  case 1: {
    let theta = atan2(vx, vy);
    let r0 = sqrt(vx * vx + vy * vy) / ((vx * vx + vy * vy) / 4.0 + 1.0);
    let radius = synth_interpolate(r0, synth_value(base, theta), smooth_t);
    v.x += AMOUNT * radius * sin(theta);
    v.y += AMOUNT * radius * cos(theta);
  }
  case 2: {
    var radius = (rnd() + rnd() + 0.002 * rnd()) / 2.002;
    let theta = 2.0 * PI * rnd() - PI;
    vx = radius * sin(theta);
    vy = radius * cos(theta);
    radius = pow(radius * radius + EPS, power / 2.0);
    radius = AMOUNT * synth_interpolate(radius, synth_value(base, theta), smooth_t);
    v.x += vx * radius;
    v.y += vy * radius;
  }
  case 3: {
    var radius = 0.5 * (rnd() + rnd());
    let theta = 2.0 * PI * rnd() - PI;
    radius = pow(radius * radius + EPS, -power / 2.0);
    radius = synth_interpolate(radius, synth_value(base, theta), smooth_t);
    v.x += AMOUNT * radius * sin(theta);
    v.y += AMOUNT * radius * cos(theta);
  }
  case 12: {
    var radius = 1.0 + 0.1 * (rnd() + rnd() - 1.0) * power;
    let theta = 2.0 * PI * rnd() - PI;
    radius = synth_interpolate(radius, synth_value(base, theta), smooth_t);
    v.x += AMOUNT * radius * sin(theta);
    v.y += AMOUNT * radius * cos(theta);
  }
  case 13: {
    let theta = 2.0 * PI * rnd() - PI;
    let radius = synth_value(base, theta) + 0.1 * pow(rnd() + EPS, power);
    v.x += AMOUNT * radius * sin(theta);
    v.y += AMOUNT * radius * cos(theta);
  }
  case 14: {
    let radius = pow(vx * vx + vy * vy + EPS, power / 2.0);
    let theta = atan2(vx, vy) - 1.0 + synth_value(base, radius);
    v.x += AMOUNT * radius * sin(theta);
    v.y += AMOUNT * radius * cos(theta);
  }
  case 15: {
    let radius = pow(vx * vx + vy * vy + EPS, power / 2.0);
    let theta = atan2(vx, vy);
    let mu = synth_value(base, radius) - 1.0;
    vx += mu * cos(theta);
    vy -= mu * sin(theta);
    v.x += AMOUNT * vx;
    v.y += AMOUNT * vy;
  }
  case 16: {
    let r0 = pow(vx * vx + vy * vy + EPS, power / 2.0);
    let theta = atan2(vx, vy) - 1.0 + synth_value(base, r0);
    let radius = sqrt(vx * vx + vy * vy);
    v.x += AMOUNT * radius * sin(theta);
    v.y += AMOUNT * radius * cos(theta);
  }
  case 4: {
    vy = 1.0 + 0.1 * (rnd() + rnd() - 1.0) * power;
    let theta = 2.0 * asin((rnd() - 0.5) * 2.0);
    vy = synth_interpolate(vy, synth_value(base, theta), smooth_t);
    v.x += AMOUNT * (theta / PI);
    v.y += AMOUNT * (vy - 1.0);
  }
  case 9: {
    v.x += AMOUNT * (vx + synth_value(base, vy) - 1.0);
    v.y += AMOUNT * vy;
  }
  case 10: {
    v.x += AMOUNT * vx;
    v.y += AMOUNT * (vy + synth_value(base, vx) - 1.0);
  }
  case 11: {
    v.x += AMOUNT * (vx + synth_value(base, vy) - 1.0);
    v.y += AMOUNT * (vy + synth_value(base, vx) - 1.0);
  }
  case 1001: {
    v.x += AMOUNT * (synth_value(base, vx) - 1.0 + (1.0 - mixv) * sin(vx));
    v.y += AMOUNT * (synth_value(base, vy) - 1.0 + (1.0 - mixv) * sin(vy));
  }
  case 1002: {
    let radius = pow(vx * vx + vy * vy + EPS, power / 2.0);
    let sc = synth_sincos(base, radius, smooth_t);
    v.x += AMOUNT * (sc.x * vx - sc.y * vy);
    v.y += AMOUNT * (sc.y * vx + sc.x * vy);
  }
  case 1003: {
    let radius = pow(vx * vx + vy * vy + EPS, power / 2.0);
    let sc = synth_sincos(base, atan2(vx, vy), smooth_t);
    v.x += AMOUNT * sc.x / radius;
    v.y += AMOUNT * sc.y * radius;
  }
  case 1004: {
    let radius = pow(vx * vx + vy * vy + EPS, power / 4.0);
    var theta = atan2(vx, vy) / 2.0;
    if (rnd() < 0.5) { theta += PI; }
    let sc = synth_sincos(base, theta, smooth_t);
    v.x += AMOUNT * radius * sc.y;
    v.y += AMOUNT * radius * sc.x;
  }
  case 1005: {
    let theta = atan2(vx, vy) / PI;
    let radius = PI * pow(vx * vx + vy * vy + EPS, power / 2.0);
    let sc = synth_sincos(base, radius, smooth_t);
    v.x = AMOUNT * sc.x * theta;
    v.y = AMOUNT * sc.y * theta;
  }
  case 1006: {
    var radius = sqrt(vx * vx + vy * vy);
    let theta = atan2(vx, vy);
    let mu = power * power + EPS;
    radius += -2.0 * mu * trunc((radius + mu) / (2.0 * mu)) + radius * (1.0 - mu);
    let sc = synth_sincos(base, theta, smooth_t);
    v.x += AMOUNT * sc.x * radius;
    v.y += AMOUNT * sc.y * radius;
  }
  case 1007: {
    let radius = pow(vx * vx + vy * vy + EPS, power / 2.0);
    let sc = synth_sincos(base, vx, smooth_t);
    v.x += AMOUNT * radius * sc.x;
    v.y += AMOUNT * radius * vy;
  }
  case 17: {
    let mu = synth_value(base, vx) - 1.0;
    v.x += AMOUNT * vx;
    v.y += AMOUNT * (2.0 * mu - vy);
  }
  case 18: {
    let mu = synth_value(base, vx) - 1.0;
    let radius = synth_value(base, vy) - 1.0;
    v.x += AMOUNT * (2.0 * radius - vx);
    v.y += AMOUNT * (2.0 * mu - vy);
  }
  case 19: {
    let theta = atan2(vx, vy);
    var radius = synth_interpolate(sqrt(vx * vx + vy * vy), synth_value(base, theta), smooth_t);
    radius = java_pow(radius, power);
    v.x += AMOUNT * radius * sin(theta);
    v.y += AMOUNT * radius * cos(theta);
  }
  default: {}
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

/// Initial state of `new java.util.Random(seed)`.
fn java_random_seed(seed: i64) -> u64 {
    (seed as u64 ^ 0x5DEECE66D) & ((1 << 48) - 1)
}

/// `java.util.Random.nextInt()` on a raw 48-bit state.
fn java_random_next_int(seed: &mut u64) -> i32 {
    *seed = seed.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & ((1 << 48) - 1);
    (*seed >> 16) as i64 as i32
}

/// Shared body of the `Sqrt_*Func` variations: `z.Sqrt(); z.<f>(); z.Scale(amount * 2/pi)`
/// with a random sign.
fn sqrt_complex(ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64, f: fn(&mut Complex)) {
    let mut z = Complex::new(a.x, a.y);
    z.sqrt();
    f(&mut z);
    z.scale(amount * M_2_PI);
    if ctx.random() < 0.5 {
        v.y += z.im;
        v.x += z.re;
    } else {
        v.y += -z.im;
        v.x += -z.re;
    }
    ctx.pass_z(a, v, amount);
}

macro_rules! sqrt_complex_wgsl {
    ($f:literal) => {
        concat!(
            "let z = cx_scale(",
            $f,
            "(cx_sqrt(vec2<f32>(a.x, a.y))), AMOUNT * 2.0 / PI);
if (rnd() < 0.5) {
  v.y += z.y;
  v.x += z.x;
} else {
  v.y += -z.y;
  v.x += -z.x;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        )
    };
}
const SQRT_ACOSECH_WGSL: &str = sqrt_complex_wgsl!("cx_acosech");
const SQRT_ACOSH_WGSL: &str = sqrt_complex_wgsl!("cx_acosh");
const SQRT_ACOTH_WGSL: &str = sqrt_complex_wgsl!("cx_acoth");
const SQRT_ASECH_WGSL: &str = sqrt_complex_wgsl!("cx_asech");
const SQRT_ASINH_WGSL: &str = sqrt_complex_wgsl!("cx_asinh");
const SQRT_ATANH_WGSL: &str = sqrt_complex_wgsl!("cx_atanh");

/// `SymBandG*Func` / `SymNetG*Func`: prepared data after the parameters,
/// `[offset_x, offset_y, n, n affine maps (a, b, c, d, e, f)]`.
fn sym_data(p: &[f64], offset: [f64; 2], t: &[[f64; 6]]) -> Vec<f64> {
    let mut d = p.to_vec();
    d.extend([offset[0], offset[1], t.len() as f64]);
    for m in t {
        d.extend_from_slice(m);
    }
    d
}

/// Applies one uniformly chosen map of a [`sym_data`] block to the shifted input point.
fn sym_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let (x, y) = (a.x + d[0], a.y + d[1]);
    // Java draws the map with Math.random()
    let k = (d[2] * ctx.random()) as usize;
    let m = &d[3 + 6 * k..9 + 6 * k];
    v.x += amount * (m[0] * x + m[1] * y + m[2]);
    v.y += amount * (m[3] * x + m[4] * y + m[5]);
    ctx.pass_z(a, v, amount);
}


fn sym_bg1_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let stepx = p[0];
    let stepy = p[1];
    let mut t: [[f64; 6]; 2] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0 - 1.0;
    t[0][5] = -stepy / 2.0;
    t[1][2] = stepx / 2.0;
    t[1][5] = stepy / 2.0;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_bg2_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let stepx = p[0];
    let stepy = p[1];
    let mut t: [[f64; 6]; 2] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0 - 1.0;
    t[0][5] = -stepy / 2.0;
    t[1][2] = stepx / 2.0;
    t[1][5] = stepy / 2.0 + 1.0;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_bg3_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let stepx = p[0];
    let stepy = p[1];
    let mut t: [[f64; 6]; 2] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0 - 1.0;
    t[0][5] = -stepy / 2.0 - 0.5;
    t[1][2] = stepx / 2.0 + 1.0;
    t[1][5] = stepy / 2.0 + 0.5;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_bg4_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let stepx = p[0];
    let stepy = p[1];
    let mut t: [[f64; 6]; 2] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0 - 1.0;
    t[0][5] = -stepy / 2.0 - 0.5;
    t[1][2] = stepx / 2.0 + 1.0;
    t[1][5] = stepy / 2.0 + 0.5;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_bg5_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let stepx = p[0];
    let stepy = p[1];
    let mut t: [[f64; 6]; 2] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0 - 1.0;
    t[0][5] = -stepy / 2.0 - 0.5;
    t[1][2] = stepx / 2.0 - 1.0;
    t[1][5] = stepy / 2.0 + 0.5;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_bg6_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let stepx = p[0];
    let stepy = p[1];
    let mut t: [[f64; 6]; 4] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0 - 1.0;
    t[0][5] = -stepy / 2.0 - 0.5;
    t[1][2] = stepx / 2.0 + 1.0;
    t[1][5] = stepy / 2.0 - 0.5;
    t[2][2] = -stepx / 2.0 - 1.0;
    t[2][5] = -stepy / 2.0 + 0.5;
    t[3][2] = stepx / 2.0 + 1.0;
    t[3][5] = stepy / 2.0 + 0.5;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_bg7_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let stepx = p[0];
    let stepy = p[1];
    let mut t: [[f64; 6]; 4] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0 - 2.0;
    t[0][5] = -stepy / 2.0 - 0.5;
    t[1][2] = stepx / 2.0 + 2.0;
    t[1][5] = stepy / 2.0 - 0.5;
    t[2][2] = stepx / 2.0;
    t[2][5] = stepy / 2.0 + 0.5;
    t[3][2] = -stepx / 2.0;
    t[3][5] = -stepy / 2.0 + 0.5;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_ng1_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let stepx = p[0];
    let stepy = p[1];
    let mut t: [[f64; 6]; 2] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0;
    t[0][5] = -stepy / 2.0;
    t[1][2] = stepx / 2.0;
    t[1][5] = stepy / 2.0;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_ng10_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let space = p[0];
    let spacex = p[1];
    let spacey = p[2];
    let mut t: [[f64; 6]; 4] = [
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [0.0, 1.0, 0.0, -1.0, -0.0, 0.0],
        [-0.0, -1.0, -0.0, 1.0, -0.0, 0.0],
        [1.0, -0.0, 0.0, 0.0, 1.0, 0.0],
    ];
    t[0][2] = -spacex / 2.0;
    t[1][2] = -spacex / 2.0;
    t[2][2] = spacex / 2.0;
    t[3][2] = spacex / 2.0;
    t[0][5] = -spacey / 2.0;
    t[1][5] = -spacey / 2.0;
    t[2][5] = spacey / 2.0;
    t[3][5] = spacey / 2.0;
    sym_data(p, [space, space], &t)
}

fn sym_ng11_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let space = p[0];
    let spacex = p[1];
    let spacey = p[2];
    let stepx = p[3];
    let stepy = p[4];
    let mut t: [[f64; 6]; 8] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-0.0, 1.0, -0.0, -1.0, -0.0, 0.0],
        [-0.0, -1.0, -0.0, 1.0, -0.0, 0.0],
        [-1.0, 0.0, -0.0, 0.0, 1.0, 0.0],
        [-0.0, -1.0, -0.0, -1.0, -0.0, 0.0],
        [-0.0, 1.0, -0.0, 1.0, -0.0, 0.0],
        [1.0, 0.0, -0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = spacex + stepx / 2.0;
    t[0][5] = spacey + stepy / 2.0;
    t[1][2] = -spacex + stepx / 2.0;
    t[1][5] = -spacey + stepy / 2.0;
    t[2][2] = spacex + stepx / 2.0;
    t[2][5] = -spacey + stepy / 2.0;
    t[3][2] = -spacex + stepx / 2.0;
    t[3][5] = spacey + stepy / 2.0;
    t[4][2] = -spacex - stepx / 2.0;
    t[4][5] = spacey - stepy / 2.0;
    t[5][2] = -spacex - stepx / 2.0;
    t[5][5] = -spacey - stepy / 2.0;
    t[6][2] = spacex - stepx / 2.0;
    t[6][5] = spacey - stepy / 2.0;
    t[7][2] = spacex - stepx / 2.0;
    t[7][5] = -spacey - stepy / 2.0;
    sym_data(p, [space, space], &t)
}

fn sym_ng12_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let space = p[0];
    let spacex = p[1];
    let spacey = p[2];
    let mut t: [[f64; 6]; 8] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-0.0, 1.0, 0.0, -1.0, -0.0, 0.0],
        [-0.0, -1.0, -0.0, 1.0, -0.0, 0.0],
        [-1.0, 0.0, -0.0, 0.0, 1.0, 0.0],
        [-0.0, -1.0, -0.0, -1.0, -0.0, 0.0],
        [-0.0, 1.0, 0.0, 1.0, -0.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = spacex;
    t[0][5] = spacey;
    t[1][2] = -spacex;
    t[1][5] = -spacey;
    t[2][2] = spacex;
    t[2][5] = -spacey;
    t[3][2] = -spacex;
    t[3][5] = spacey;
    t[4][2] = -spacex;
    t[4][5] = spacey;
    t[5][2] = -spacex;
    t[5][5] = -spacey;
    t[6][2] = spacex;
    t[6][5] = spacey;
    t[7][2] = spacex;
    t[7][5] = -spacey;
    sym_data(p, [space, space], &t)
}

fn sym_ng13_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let radius = p[0];
    let stepx = p[1];
    let stepy = p[2];
    let mut t: [[f64; 6]; 6] = [
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-0.5, -0.866, -0.0, -0.866, 0.5, 0.0],
        [-0.5, 0.866, -0.0, 0.866, 0.5, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-0.5, -0.866, -0.0, -0.866, 0.5, 0.0],
        [-0.5, 0.866, -0.0, 0.866, 0.5, 0.0],
    ];
    let sx = stepx / 2.0;
    let sy = stepy / 2.0;
    let spacex = f64::sqrt(radius * radius / 2.0);
    let spacey = spacex;
    t[0][2] = -sx;
    t[0][5] = -sy;
    t[1][2] = -sx;
    t[1][5] = -sy;
    t[2][2] = -sx;
    t[2][5] = -sy;
    t[3][2] = sx;
    t[3][5] = sy;
    t[4][2] = sx;
    t[4][5] = sy;
    t[5][2] = sx;
    t[5][5] = sy;
    sym_data(p, [spacex, spacey], &t)
}

fn sym_ng14_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let radius = p[0];
    let stepx = p[1];
    let stepy = p[2];
    let mut t: [[f64; 6]; 12] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-0.5, -0.866, -0.0, 0.866, -0.5, 0.0],
        [-0.5, 0.866, -0.0, -0.866, -0.5, 0.0],
        [-0.5, -0.866, -0.0, -0.866, 0.5, 0.0],
        [-0.5, 0.866, -0.0, 0.866, 0.5, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-0.5, -0.866, -0.0, 0.866, -0.5, 0.0],
        [-0.5, 0.866, -0.0, -0.866, -0.5, 0.0],
        [-0.5, -0.866, -0.0, -0.866, 0.5, 0.0],
        [-0.5, 0.866, -0.0, 0.866, 0.5, 0.0],
    ];
    let sx = stepx / 2.0;
    let sy = stepy / 2.0;
    let spacex = f64::sqrt(radius * radius / 2.0);
    t[0][2] = -sx;
    t[0][5] = -sy;
    t[1][2] = -sx;
    t[1][5] = -sy;
    t[2][2] = -sx;
    t[2][5] = -sy;
    t[3][2] = -sx;
    t[3][5] = -sy;
    t[4][2] = -sx;
    t[4][5] = -sy;
    t[5][2] = -sx;
    t[5][5] = -sy;
    t[6][2] = sx;
    t[6][5] = sy;
    t[7][2] = sx;
    t[7][5] = sy;
    t[8][2] = sx;
    t[8][5] = sy;
    t[9][2] = sx;
    t[9][5] = sy;
    t[10][2] = sx;
    t[10][5] = sy;
    t[11][2] = sx;
    t[11][5] = sy;
    sym_data(p, [spacex, spacex], &t)
}

fn sym_ng15_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let radius = p[0];
    let stepx = p[1];
    let stepy = p[2];
    let mut t: [[f64; 6]; 12] = [
        [0.0, 1.0, 0.0, -1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0, -1.0, 0.0, 0.0],
        [0.866, -0.5, 0.0, 0.5, 0.866, 0.0],
        [0.866, 0.5, -0.0, 0.5, -0.866, 0.0],
        [-0.866, -0.5, -0.0, 0.5, -0.866, 0.0],
        [-0.866, 0.5, -0.0, 0.5, 0.866, 0.0],
        [0.0, 1.0, 0.0, -1.0, 0.0, 0.0],
        [0.0, -1.0, 0.0, -1.0, 0.0, 0.0],
        [0.866, -0.5, 0.0, 0.5, 0.866, 0.0],
        [0.866, 0.5, -0.0, 0.5, -0.866, 0.0],
        [-0.866, -0.5, -0.0, 0.5, -0.866, 0.0],
        [-0.866, 0.5, -0.0, 0.5, 0.866, 0.0],
    ];
    let sx = stepx / 2.0;
    let sy = stepy / 2.0;
    let spacex = f64::sqrt(radius * radius / 2.0);
    t[0][2] = -sx;
    t[0][5] = -sy;
    t[1][2] = -sx;
    t[1][5] = -sy;
    t[2][2] = -sx;
    t[2][5] = -sy;
    t[3][2] = -sx;
    t[3][5] = -sy;
    t[4][2] = -sx;
    t[4][5] = -sy;
    t[5][2] = -sx;
    t[5][5] = -sy;
    t[6][2] = sx;
    t[6][5] = sy;
    t[7][2] = sx;
    t[7][5] = sy;
    t[8][2] = sx;
    t[8][5] = sy;
    t[9][2] = sx;
    t[9][5] = sy;
    t[10][2] = sx;
    t[10][5] = sy;
    t[11][2] = sx;
    t[11][5] = sy;
    sym_data(p, [spacex, spacex], &t)
}

fn sym_ng16_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let radius = p[0];
    let stepx = p[1];
    let stepy = p[2];
    let mut t: [[f64; 6]; 12] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [0.5, -0.866, 0.0, 0.866, 0.5, 0.0],
        [-0.5, -0.866, -0.0, 0.866, -0.5, 0.0],
        [-1.0, 0.0, -0.0, 0.0, -1.0, 0.0],
        [-0.5, 0.866, -0.0, -0.866, -0.5, 0.0],
        [0.5, 0.866, -0.0, -0.866, 0.5, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [0.5, -0.866, 0.0, 0.866, 0.5, 0.0],
        [-0.5, -0.866, -0.0, 0.866, -0.5, 0.0],
        [-1.0, 0.0, -0.0, 0.0, -1.0, 0.0],
        [-0.5, 0.866, -0.0, -0.866, -0.5, 0.0],
        [0.5, 0.866, -0.0, -0.866, 0.5, 0.0],
    ];
    let space = f64::sqrt(radius * radius / 2.0);
    let sx = stepx / 2.0;
    let sy = stepy / 2.0;
    t[0][2] = -sx;
    t[0][5] = -sy;
    t[1][2] = -sx;
    t[1][5] = -sy;
    t[2][2] = -sx;
    t[2][5] = -sy;
    t[3][2] = -sx;
    t[3][5] = -sy;
    t[4][2] = -sx;
    t[4][5] = -sy;
    t[5][2] = -sx;
    t[5][5] = -sy;
    t[6][2] = sx;
    t[6][5] = sy;
    t[7][2] = sx;
    t[7][5] = sy;
    t[8][2] = sx;
    t[8][5] = sy;
    t[9][2] = sx;
    t[9][5] = sy;
    t[10][2] = sx;
    t[10][5] = sy;
    t[11][2] = sx;
    t[11][5] = sy;
    sym_data(p, [space, space], &t)
}

fn sym_ng17_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let radius = p[0];
    let stepx = p[1];
    let stepy = p[2];
    let mut t: [[f64; 6]; 24] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [0.5, -0.866, -0.0, 0.866, 0.5, 0.0],
        [0.5, 0.866, -0.0, 0.866, -0.5, 0.0],
        [-0.5, -0.866, -0.0, 0.866, -0.5, 0.0],
        [-0.5, 0.866, -0.0, 0.866, 0.5, 0.0],
        [-1.0, 0.0, -0.0, 0.0, -1.0, 0.0],
        [-1.0, 0.0, -0.0, 0.0, 1.0, 0.0],
        [-0.5, 0.866, -0.0, -0.866, -0.5, 0.0],
        [-0.5, -0.866, -0.0, -0.866, 0.5, 0.0],
        [0.5, 0.866, -0.0, -0.866, 0.5, 0.0],
        [0.5, -0.866, -0.0, -0.866, -0.5, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [0.5, -0.866, -0.0, 0.866, 0.5, 0.0],
        [0.5, 0.866, -0.0, 0.866, -0.5, 0.0],
        [-0.5, -0.866, -0.0, 0.866, -0.5, 0.0],
        [-0.5, 0.866, -0.0, 0.866, 0.5, 0.0],
        [-1.0, 0.0, -0.0, 0.0, -1.0, 0.0],
        [-1.0, 0.0, -0.0, 0.0, 1.0, 0.0],
        [-0.5, 0.866, -0.0, -0.866, -0.5, 0.0],
        [-0.5, -0.866, -0.0, -0.866, 0.5, 0.0],
        [0.5, 0.866, -0.0, -0.866, 0.5, 0.0],
        [0.5, -0.866, -0.0, -0.866, -0.5, 0.0],
    ];
    let space = f64::sqrt(radius * radius / 2.0);
    let sx = stepx / 2.0;
    let sy = stepy / 2.0;
    t[0][2] = -sx;
    t[0][5] = -sy;
    t[1][2] = -sx;
    t[1][5] = -sy;
    t[2][2] = -sx;
    t[2][5] = -sy;
    t[3][2] = -sx;
    t[3][5] = -sy;
    t[4][2] = -sx;
    t[4][5] = -sy;
    t[5][2] = -sx;
    t[5][5] = -sy;
    t[6][2] = -sx;
    t[6][5] = -sy;
    t[7][2] = -sx;
    t[7][5] = -sy;
    t[8][2] = -sx;
    t[8][5] = -sy;
    t[9][2] = -sx;
    t[9][5] = -sy;
    t[10][2] = -sx;
    t[10][5] = -sy;
    t[11][2] = -sx;
    t[11][5] = -sy;
    t[12][2] = sx;
    t[12][5] = sy;
    t[13][2] = sx;
    t[13][5] = sy;
    t[14][2] = sx;
    t[14][5] = sy;
    t[15][2] = sx;
    t[15][5] = sy;
    t[16][2] = sx;
    t[16][5] = sy;
    t[17][2] = sx;
    t[17][5] = sy;
    t[18][2] = sx;
    t[18][5] = sy;
    t[19][2] = sx;
    t[19][5] = sy;
    t[20][2] = sx;
    t[20][5] = sy;
    t[21][2] = sx;
    t[21][5] = sy;
    t[22][2] = sx;
    t[22][5] = sy;
    t[23][2] = sx;
    t[23][5] = sy;
    sym_data(p, [space, space], &t)
}

fn sym_ng2_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let stepx = p[0];
    let stepy = p[1];
    let mut t: [[f64; 6]; 2] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0;
    t[0][5] = -stepy / 2.0;
    t[1][2] = stepx / 2.0;
    t[1][5] = stepy / 2.0;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_ng3_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let sepx = p[0];
    let sepy = p[1];
    let step = p[2];
    let mut t: [[f64; 6]; 2] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = -sepx - step / 2.0;
    t[0][5] = -sepy - step / 2.0;
    t[1][2] = sepx + step / 2.0;
    t[1][5] = sepy + step / 2.0;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_ng4_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let sepx = p[0];
    let sepy = p[1];
    let stepx = p[2];
    let stepy = p[3];
    let mut t: [[f64; 6]; 4] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    ];
    let sx = sepx / 2.0;
    let sy = sepy / 2.0;
    let stpx = stepx / 2.0;
    let stpy = stepy / 2.0;
    t[0][2] = -sx - 2.0 - stpx;
    t[0][5] = sy - 1.5 - stpy;
    t[1][2] = sx - stpx;
    t[1][5] = -sy - 0.5 - stpy;
    t[2][2] = -sx + stpx;
    t[2][5] = -sy + 0.5 + stpy;
    t[3][2] = sx + 2.0 + stpx;
    t[3][5] = sy - 0.5 + stpy;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_ng5_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let sepx = p[0];
    let sepy = p[1];
    let stepx = p[2];
    let mut t: [[f64; 6]; 4] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    ];
    t[0][2] = -sepx - 2.0 - stepx / 2.0;
    t[0][5] = sepy - 0.5;
    t[1][2] = sepx - stepx / 2.0;
    t[1][5] = -sepy - 0.5;
    t[2][2] = -sepx + stepx / 2.0;
    t[2][5] = -sepy - 0.5;
    t[3][2] = 2.0 + sepx + stepx / 2.0;
    t[3][5] = sepy - 0.5;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_ng6_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let sepy = p[0];
    let stepx = p[1];
    let stepy = p[2];
    let mut t: [[f64; 6]; 4] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = -stepx / 2.0;
    t[0][5] = -stepy / 2.0 + sepy / 2.0;
    t[1][2] = -stepx / 2.0;
    t[1][5] = -stepy / 2.0 - sepy / 2.0;
    t[2][2] = stepx / 2.0;
    t[2][5] = stepy / 2.0 + sepy / 2.0;
    t[3][2] = stepx / 2.0;
    t[3][5] = stepy / 2.0 - sepy / 2.0;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_ng7_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let sepx = p[0];
    let sepy = p[1];
    let mut t: [[f64; 6]; 2] = [
        [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
    ];
    t[0][2] = -sepx / 2.0;
    t[0][5] = sepy / 2.0;
    t[1][2] = sepx / 2.0;
    t[1][5] = -sepy / 2.0;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_ng8_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let sepx = p[0];
    let sepy = p[1];
    let skewx = p[2];
    let skewy = p[3];
    let mut t: [[f64; 6]; 8] = [
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, -0.0],
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, -0.0],
    ];
    t[0][2] = -1.0 - sepx / 2.0 - skewx / 2.0;
    t[0][5] = -sepy / 2.0 - skewy / 2.0;
    t[1][2] = 1.0 + sepx / 2.0 - skewx / 2.0;
    t[1][5] = sepy / 2.0 - skewy / 2.0;
    t[2][2] = 1.0 + sepx / 2.0 - skewx / 2.0;
    t[2][5] = -sepy / 2.0 - skewy / 2.0;
    t[3][2] = -1.0 - sepx / 2.0 - skewx / 2.0;
    t[3][5] = sepy / 2.0 - skewy / 2.0;
    t[4][2] = -1.0 - sepx / 2.0 + skewx / 2.0;
    t[4][5] = -sepy / 2.0 + skewy / 2.0;
    t[5][2] = 1.0 + sepx / 2.0 + skewx / 2.0;
    t[5][5] = sepy / 2.0 + skewy / 2.0;
    t[6][2] = 1.0 + sepx / 2.0 + skewx / 2.0;
    t[6][5] = -sepy / 2.0 + skewy / 2.0;
    t[7][2] = -1.0 - sepx / 2.0 + skewx / 2.0;
    t[7][5] = sepy / 2.0 + skewy / 2.0;
    sym_data(p, [0.0, 0.0], &t)
}

fn sym_ng9_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let sepx = p[0];
    let sepy = p[1];
    let mut t: [[f64; 6]; 4] = [
        [1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0, -1.0, 0.0],
        [1.0, 0.0, 0.0, 0.0, 1.0, -0.0],
    ];
    t[0][2] = sepx / 2.0;
    t[0][5] = -sepy / 2.0;
    t[1][2] = -sepx / 2.0;
    t[1][5] = sepy / 2.0;
    t[2][2] = -sepx / 2.0;
    t[2][5] = -sepy / 2.0;
    t[3][2] = sepx / 2.0;
    t[3][5] = sepy / 2.0;
    sym_data(p, [0.0, 0.0], &t)
}
