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
    // @@NEXT@@
];

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
