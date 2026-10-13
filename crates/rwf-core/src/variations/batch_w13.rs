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

// Some JWildfire defaults are truncated pi literals (e.g. 1.5708).
#![allow(clippy::approx_constant)]

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

/// WGSL for [`wx_total`] as a helper function with the given name (`prm[base]` = hypergon).
macro_rules! wx_total_wgsl {
    ($name:literal) => {
        concat!(
            "fn ",
            $name,
            "(base: u32, ang: f32) -> f32 {
  var total = 0.0;
  let hypergon = prm[base];
  let star = prm[base + 3u];
  let lituus = prm[base + 6u];
  let sup = prm[base + 8u];
  let hd = prm[base + 13u];
  let hn = prm[base + 14u];
  let sn = prm[base + 16u];
  let ss = prm[base + 17u];
  if (hypergon != 0.0) {
    let t1 = fmod(abs(ang), TWO_PI / hn) - PI / hn;
    let t2 = sqr(tan(t1)) + 1.0;
    if (t2 >= sqr(hd)) {
      total += hypergon;
    } else {
      total += hypergon * (hd - sqrt(sqr(hd) - t2)) / sqrt(t2);
    }
  }
  if (star != 0.0) {
    let t1 = tan(abs(fmod(abs(ang), TWO_PI / sn) - PI / sn));
    total += star * sqrt(sqr(ss) * (1.0 + sqr(t1)) / sqr(t1 + ss));
  }
  if (lituus != 0.0) {
    total += lituus * java_pow(ang / PI + 1.0, prm[base + 15u]);
  }
  if (sup != 0.0) {
    let sa = ang * prm[base + 18u];
    total += sup * pow(pow(abs(cos(sa)), prm[base + 11u]) + pow(abs(sin(sa)), prm[base + 12u]), prm[base + 19u]);
  }
  return total;
}
"
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
    VarDef {
        name: "tidal_lock",
        params: &[ParamDef::new("ratio", 1.0), ParamDef::new("ecc", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let ang = a.y.atan2(a.x);
            let new_a = ang * d[0] + d[1] * (2.0 * ang).sin();
            v.x += amount * r * new_a.cos();
            v.y += amount * r * new_a.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let ang = atan2(a.y, a.x);
let new_a = ang * D(0) + D(1) * sin(2.0 * ang);
v.x += AMOUNT * r * cos(new_a);
v.y += AMOUNT * r * sin(new_a);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tile_hlp",
        params: &[ParamDef::new("width", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let width2 = d[0] * amount;
            let x = a.x / d[0];
            let mut aux = if x > 0.0 { x - (x as i32) as f64 } else { x + (x as i32) as f64 };
            aux = (aux * M_PI).cos();
            let mut aux2 = 0.0;
            if aux < ctx.random() * 2.0 - 1.0 {
                aux2 = if x > 0.0 { -width2 } else { width2 };
            }
            v.x += a.x * amount + aux2;
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let width2 = D(0) * AMOUNT;
let x = a.x / D(0);
var aux = x + trunc(x);
if (x > 0.0) { aux = x - trunc(x); }
aux = cos(aux * PI);
var aux2 = 0.0;
if (aux < rnd() * 2.0 - 1.0) { aux2 = select(width2, -width2, x > 0.0); }
v.x += a.x * AMOUNT + aux2;
v.y += AMOUNT * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tile_reverse",
        params: &[ParamDef::new("space", 1.0), ParamDef::new("reversal", 1.0), ParamDef::int("vertical", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let n = ctx.random();
            let rev = if d[1] == 1.0 { -1.0 } else { 1.0 };
            let vertical = d[2] as i32 == 1;
            if n < 0.5 && !vertical {
                v.x = amount * (rev * a.x) + d[0];
                v.y = amount * a.y;
            } else if n > 0.5 && !vertical {
                v.x = amount * (rev * a.x) - d[0];
                v.y = amount * a.y;
            } else if n < 0.5 && vertical {
                v.y = amount * (rev * a.y) + d[0];
                v.x = amount * a.x;
            } else if n > 0.5 && vertical {
                v.y = amount * (rev * a.y) - d[0];
                v.x = amount * a.x;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], ftoi(p[2]) as f64]),
        wgsl: "let n = rnd();
let rev = select(1.0, -1.0, D(1) == 1.0);
let vertical = i32(D(2)) == 1;
if (n < 0.5 && !vertical) {
  v.x = AMOUNT * (rev * a.x) + D(0);
  v.y = AMOUNT * a.y;
} else if (n > 0.5 && !vertical) {
  v.x = AMOUNT * (rev * a.x) - D(0);
  v.y = AMOUNT * a.y;
} else if (n < 0.5 && vertical) {
  v.y = AMOUNT * (rev * a.y) + D(0);
  v.x = AMOUNT * a.x;
} else if (n > 0.5 && vertical) {
  v.y = AMOUNT * (rev * a.y) - D(0);
  v.x = AMOUNT * a.x;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tinkerbell",
        params: &[ParamDef::new("a", -0.3), ParamDef::new("b", -0.6013), ParamDef::new("c", 2.0), ParamDef::new("d", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let nx = a.x * a.x - a.y * a.y + d[0] * a.x + d[1] * a.y;
            let ny = 2.0 * a.x * a.y + d[2] * a.x + d[3] * a.y;
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let nx = a.x * a.x - a.y * a.y + D(0) * a.x + D(1) * a.y;
let ny = 2.0 * a.x * a.y + D(2) * a.x + D(3) * a.y;
v.x += AMOUNT * nx;
v.y += AMOUNT * ny;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "toroidal",
        params: &[ParamDef::new("major", 2.0), ParamDef::new("minor", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let tor_x = (d[0] + d[1] * a.y.cos()) * a.x.cos();
            let tor_y = (d[0] + d[1] * a.y.cos()) * a.x.sin();
            v.x += amount * tor_x;
            v.y += amount * tor_y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let rr = D(0) + D(1) * cos(a.y);
v.x += AMOUNT * rr * cos(a.x);
v.y += AMOUNT * rr * sin(a.x);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "torus3D",
        params: &[ParamDef::range("r1", 0.5, 0.0, 0.5), ParamDef::range("r2", 0.5, 0.0, 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            for _ in 0..50 {
                let x = ctx.random() - 0.5;
                let y = ctx.random() - 0.5;
                let z = ctx.random() - 0.5;
                let q = (x * x + z * z).sqrt() - d[0];
                if (q * q + y * y).sqrt() - d[1] < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, 0.5), limit_value(p[1], 0.0, 0.5)]),
        wgsl: "var found = false;
for (var i = 0; i < 50; i++) {
  let p = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  if (length(vec2<f32>(length(p.xz) - D(0), p.y)) - D(1) < 0.0) {
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
        name: "torus_knot",
        params: &[ParamDef::new("p", 2.0), ParamDef::new("q", 3.0), ParamDef::new("r_big", 1.0), ParamDef::new("r_small", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let t = a.y.atan2(a.x);
            let outer = d[2] + d[3] * (d[1] * t).cos();
            v.x += amount * outer * (d[0] * t).cos();
            v.y += amount * outer * (d[0] * t).sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let t = atan2(a.y, a.x);
let outer = D(2) + D(3) * cos(D(1) * t);
v.x += AMOUNT * outer * cos(D(0) * t);
v.y += AMOUNT * outer * sin(D(0) * t);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tqmirror",
        params: &[
            ParamDef::int("preset", 0.0, 0.0, 3.0),
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
            ParamDef::new("l", 0.0),
            ParamDef::new("m", 0.0),
            ParamDef::new("n", 0.0),
            ParamDef::new("o", 0.0),
            ParamDef::new("p", 0.0),
            ParamDef::new("q", 0.0),
            ParamDef::new("r", 1.0),
            ParamDef::new("s", 1.0),
            ParamDef::int("type", 0.0, 0.0, 1.0),
            ParamDef::int("mode", 0.0, 0.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [preset, a .. s (1..19), type, mode]; JWildfire's preset only overwrites a .. s when
        // it is changed, the stored values of a .. s follow it when a flame is loaded.
        apply: |d, ctx, a, v, amount| {
            let (x, y) = (a.x, a.y);
            let p = &d[1..20];
            let (pa, pb, pc, pd, pe, pf, pg, ph, pi, pj, pk) = (p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7], p[8], p[9], p[10]);
            let (pl, pm, pn, po, pp, pq, pr, ps) = (p[11], p[12], p[13], p[14], p[15], p[16], p[17], p[18]);
            if amount * pd + x < pl || amount * pe + y < pm {
                if d[20] as i32 != 0 {
                    v.x += x * pr;
                    v.y += y * ps;
                } else {
                    v.x += y * pr;
                    v.y += x * ps;
                    ctx.pass_z(a, v, amount);
                }
                return;
            }
            if x < pn && y < po {
                v.x += x + amount * pf;
                v.y += y + amount * pg;
            } else {
                let inside = x + pq < amount && y < amount * pa && x + amount * pb > pl && y + amount * pc > pp;
                match d[21] as i32 {
                    0 => {
                        if inside {
                            v.x -= y * ph;
                            v.y -= x * pi;
                        } else {
                            v.x += x * pj;
                            v.y += y * pk;
                        }
                    }
                    1 => {
                        if inside {
                            v.x -= y * ph;
                            v.y -= x * pi;
                        } else {
                            v.x -= x * pj;
                            v.y -= y * pk;
                        }
                    }
                    2 => {
                        if inside {
                            v.x += y * ph;
                            v.y -= x * pi;
                        } else {
                            v.x += x * pj;
                            v.y -= y * pk;
                        }
                    }
                    _ => {}
                }
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = ftoi(p[0]) as f64;
            d[20] = limit_value(p[20], 0.0, 1.0) as i32 as f64;
            d[21] = limit_value(p[21], 0.0, 2.0) as i32 as f64;
            d
        }),
        wgsl: "let x = a.x;
let y = a.y;
if (AMOUNT * D(4) + x < D(12) || AMOUNT * D(5) + y < D(13)) {
  if (i32(D(20)) != 0) {
    v.x += x * D(18);
    v.y += y * D(19);
  } else {
    v.x += y * D(18);
    v.y += x * D(19);
    if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
  }
} else {
  if (x < D(14) && y < D(15)) {
    v.x += x + AMOUNT * D(6);
    v.y += y + AMOUNT * D(7);
  } else {
    let inside = x + D(17) < AMOUNT && y < AMOUNT * D(1) && x + AMOUNT * D(2) > D(12) && y + AMOUNT * D(3) > D(16);
    let mode = i32(D(21));
    if (mode == 0) {
      if (inside) { v.x -= y * D(8); v.y -= x * D(9); } else { v.x += x * D(10); v.y += y * D(11); }
    } else if (mode == 1) {
      if (inside) { v.x -= y * D(8); v.y -= x * D(9); } else { v.x -= x * D(10); v.y -= y * D(11); }
    } else if (mode == 2) {
      if (inside) { v.x += y * D(8); v.y -= x * D(9); } else { v.x += x * D(10); v.y -= y * D(11); }
    }
  }
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    VarDef {
        name: "trade",
        params: &[ParamDef::new("r1", 1.0), ParamDef::new("d1", 1.0), ParamDef::new("r2", 1.0), ParamDef::new("d2", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [r1, d1, r2, d2, c1, c2]
        apply: |d, ctx, a, v, amount| {
            let (r1, r2, c1, c2) = (d[0], d[2], d[4], d[5]);
            if a.x > 0.0 {
                let mut r = (sqr(c1 - a.x) + sqr(a.y)).sqrt();
                if r <= r1 {
                    r *= r2 / r1;
                    let ang = a.y.atan2(c1 - a.x);
                    v.x += amount * (r * ang.cos() - c2);
                    v.y += amount * r * ang.sin();
                } else {
                    v.x += amount * a.x;
                    v.y += amount * a.y;
                }
            } else {
                let mut r = (sqr(-c2 - a.x) + sqr(a.y)).sqrt();
                if r <= r2 {
                    r *= r1 / r2;
                    let ang = a.y.atan2(-c2 - a.x);
                    v.x += amount * (r * ang.cos() + c1);
                    v.y += amount * r * ang.sin();
                } else {
                    v.x += amount * a.x;
                    v.y += amount * a.y;
                }
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let r1 = limit_value(p[0], EPSILON, f64::MAX);
            let d1 = limit_value(p[1], 0.0, f64::MAX);
            let r2 = limit_value(p[2], EPSILON, f64::MAX);
            let d2 = limit_value(p[3], 0.0, f64::MAX);
            vec![r1, d1, r2, d2, r1 + d1, r2 + d2]
        }),
        wgsl: "let r1 = D(0);
let r2 = D(2);
let c1 = D(4);
let c2 = D(5);
if (a.x > 0.0) {
  var r = sqrt(sqr(c1 - a.x) + sqr(a.y));
  if (r <= r1) {
    r *= r2 / r1;
    let ang = atan2(a.y, c1 - a.x);
    v.x += AMOUNT * (r * cos(ang) - c2);
    v.y += AMOUNT * r * sin(ang);
  } else {
    v.x += AMOUNT * a.x;
    v.y += AMOUNT * a.y;
  }
} else {
  var r = sqrt(sqr(-c2 - a.x) + sqr(a.y));
  if (r <= r2) {
    r *= r1 / r2;
    let ang = atan2(a.y, -c2 - a.x);
    v.x += AMOUNT * (r * cos(ang) + c1);
    v.y += AMOUNT * r * sin(ang);
  } else {
    v.x += AMOUNT * a.x;
    v.y += AMOUNT * a.y;
  }
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tree_js",
        params: &[
            ParamDef::int("level", 2.0, 1.0, 10.0),
            ParamDef::int("show_lines", 1.0, 0.0, 1.0),
            ParamDef::new("line_thickness", 0.5),
            ParamDef::new("bend_angle", 15.0),
            ParamDef::new("branch_angle", 37.0),
            ParamDef::new("branch_ratio", 0.65),
            ParamDef::int("show_points", 0.0, 0.0, 1.0),
            ParamDef::new("point_thickness", 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [level, show_lines, line_thickness, bend, branch, ratio, show_points, point_thickness,
        //      line_threshold, point_threshold, bend_rad, branch_rad]
        apply: |d, ctx, a, v, amount| {
            // Java walks the branches of the tree one by one (in recursion order) per instance;
            // here per render thread.
            let level = d[0] as i32;
            let n_nodes = (3i64.pow(level as u32 + 1) - 1) / 2;
            let key = d.as_ptr() as usize;
            let k = ctx.orbits.get(&key).map_or(0, |s| s[0] as i64);
            ctx.orbits.insert(key, [((k + 1) % n_nodes) as f64, 0.0]);
            let (x1, y1, x2, y2) = tree_js_branch(d, level, k);
            let rnd = ctx.random();
            let (mut ox, mut oy) = (0.0, 0.0);
            if rnd < d[8] {
                let lt = d[2] / 100.0;
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
                if lt != 0.0 {
                    xoffset += (ctx.random() - 0.5) * lt;
                    yoffset += (ctx.random() - 0.5) * lt;
                }
                ox = x1 + xoffset;
                oy = y1 + yoffset;
            } else if rnd <= d[9] {
                let pt = d[7] / 100.0;
                let (mut xoffset, mut yoffset) = (0.0, 0.0);
                if pt != 0.0 {
                    let roffset = ctx.random() * pt;
                    let rangle = ctx.random() * M_2PI;
                    xoffset = roffset * rangle.cos();
                    yoffset = roffset * rangle.sin();
                }
                ox = x1 + xoffset;
                oy = y1 + yoffset;
            }
            v.x += amount * ox;
            v.y += amount * oy;
            v.x += amount * ox;
            v.y += amount * oy;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = limit_value(ftoi(p[0]), 1, 10) as f64;
            d[1] = p[1] as i32 as f64;
            d[6] = p[6] as i32 as f64;
            let show_sum = d[1] + d[6];
            let line_fraction = d[1] / show_sum;
            let point_fraction = d[6] / show_sum;
            d.extend([line_fraction, line_fraction + point_fraction, p[3].to_radians(), p[4].to_radians()]);
            d
        }),
        // GPU: a random branch instead of the sequential walk.
        wgsl: "fn tree_js_branch(base: u32, level: i32, k_in: i32) -> vec4<f32> {
  let bend = prm[base + 10u];
  let branch = prm[base + 11u];
  let ratio = prm[base + 5u];
  var x = 0.0;
  var y = 0.0;
  var ang = PI / 2.0;
  var r = 2.0;
  var k = k_in;
  var sub = (i32(pow(3.0, f32(level + 1)) + 0.5) - 1) / 2;
  for (var t = 0; t < level; t++) {
    if (k == 0) { break; }
    k -= 1;
    sub = (sub - 1) / 3;
    let c = k / sub;
    k = k % sub;
    x = x + cos(ang) * r;
    y = y + sin(ang) * r;
    if (c == 0) { ang = ang + bend - branch; r = r * ratio; }
    else if (c == 1) { ang = ang + bend + branch; r = r * ratio; }
    else { ang = ang + bend; r = r * (1.0 - ratio); }
  }
  return vec4<f32>(x, y, x + cos(ang) * r, y + sin(ang) * r);
}
@body
let level = i32(D(0));
let n_nodes = (i32(pow(3.0, f32(level + 1)) + 0.5) - 1) / 2;
let seg = tree_js_branch(base, level, min(i32(rnd() * f32(n_nodes)), n_nodes - 1));
let x1 = seg.x;
let y1 = seg.y;
let x2 = seg.z;
let y2 = seg.w;
let rv = rnd();
var ox = 0.0;
var oy = 0.0;
if (rv < D(8)) {
  let lt = D(2) / 100.0;
  let ydiff = y2 - y1;
  let xdiff = x2 - x1;
  var m = 10000.0;
  if (xdiff != 0.0) { m = ydiff / xdiff; }
  let line_length = sqrt(xdiff * xdiff + ydiff * ydiff);
  var xoffset = rnd() * line_length / sqrt(1.0 + m * m);
  if (x2 < x1) { xoffset = -xoffset; }
  var yoffset = abs(m * xoffset);
  if (y2 < y1) { yoffset = -yoffset; }
  if (lt != 0.0) {
    xoffset += (rnd() - 0.5) * lt;
    yoffset += (rnd() - 0.5) * lt;
  }
  ox = x1 + xoffset;
  oy = y1 + yoffset;
} else if (rv <= D(9)) {
  let pt = D(7) / 100.0;
  ox = x1;
  oy = y1;
  if (pt != 0.0) {
    let roffset = rnd() * pt;
    let rangle = rnd() * TWO_PI;
    ox += roffset * cos(rangle);
    oy += roffset * sin(rangle);
  }
}
v.x += 2.0 * AMOUNT * ox;
v.y += 2.0 * AMOUNT * oy;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "triangle",
        params: &[
            ParamDef::new("x1", -0.2),
            ParamDef::new("y1", -0.1),
            ParamDef::new("z1", 0.0),
            ParamDef::new("x2", 0.2),
            ParamDef::new("y2", -0.1),
            ParamDef::new("z2", 0.0),
            ParamDef::new("x3", 0.2),
            ParamDef::new("y3", 0.1),
            ParamDef::new("z3", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            let sqrt_r1 = ctx.random().sqrt();
            let r2 = ctx.random();
            let (a, b, c) = (1.0 - sqrt_r1, sqrt_r1 * (1.0 - r2), r2 * sqrt_r1);
            v.x = amount * (a * d[0] + b * d[3] + c * d[6]);
            v.y = amount * (a * d[1] + b * d[4] + c * d[7]);
            v.z = amount * (a * d[2] + b * d[5] + c * d[8]);
        },
        prepare: None,
        wgsl: "let sqrt_r1 = sqrt(rnd());
let r2 = rnd();
let ta = 1.0 - sqrt_r1;
let tb = sqrt_r1 * (1.0 - r2);
let tc = r2 * sqrt_r1;
v.x = AMOUNT * (ta * D(0) + tb * D(3) + tc * D(6));
v.y = AMOUNT * (ta * D(1) + tb * D(4) + tc * D(7));
v.z = AMOUNT * (ta * D(2) + tb * D(5) + tc * D(8));",
    },
    VarDef {
        name: "tri_lattice",
        params: &[ParamDef::new("scale", 2.0), ParamDef::new("morph", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let ax = a.x * d[0];
            let ay = a.y * d[0];
            let u = ax - ay * 0.5773502691896257;
            let w = ay * 1.1547005383792515;
            let fu = u - java_round(u);
            let fv = w - java_round(w);
            let bx = fu + fv * 0.5;
            let by = fv * 0.8660254037844386;
            v.x += amount * (a.x + d[1] * (bx / d[0] - a.x));
            v.y += amount * (a.y + d[1] * (by / d[0] - a.y));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ax = a.x * D(0);
let ay = a.y * D(0);
let u = ax - ay * 0.5773502691896257;
let w = ay * 1.1547005383792515;
let fu = u - java_round(u);
let fv = w - java_round(w);
let bx = fu + fv * 0.5;
let by = fv * 0.8660254037844386;
v.x += AMOUNT * (a.x + D(1) * (bx / D(0) - a.x));
v.y += AMOUNT * (a.y + D(1) * (by / D(0) - a.y));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "triprism3D",
        params: &[ParamDef::range("d", 0.25, 0.0, 0.5), ParamDef::range("h", 0.5, 0.0, 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            for _ in 0..50 {
                let x = ctx.random() - 0.5;
                let y = ctx.random() - 0.5;
                let z = ctx.random() - 0.5;
                if triprism3d_sd([x, y, z], d[0], d[1]) < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, 0.5), limit_value(p[1], 0.0, 0.5)]),
        wgsl: "fn triprism3D_sd(p_in: vec3<f32>, hx_in: f32, hy: f32) -> f32 {
  let k = sqrt(3.0);
  let hx = hx_in * (0.5 * k);
  var px = p_in.x / hx;
  var py = p_in.y / hx;
  px = abs(px) - 1.0;
  py = py + 1.0 / k;
  if (px + k * py > 0.0) {
    let t0 = vec2<f32>(px - k * py, -k * px - py) / 2.0;
    px = t0.x;
    py = t0.y;
  }
  px = px - clamp(px, -2.0, 0.0);
  let d1 = sqrt(px * px + py * py) * sign(-py) * hx;
  let d2 = abs(p_in.z) - hy;
  return length(max(vec2<f32>(d1, d2), vec2<f32>(0.0))) + min(max(d1, d2), 0.0);
}
@body
var found = false;
for (var i = 0; i < 50; i++) {
  let p = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  if (triprism3D_sd(p, D(0), D(1)) < 0.0) {
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
        name: "truchet",
        params: &[
            ParamDef::int("extended", 0.0, 0.0, 1.0),
            ParamDef::range("exponent", 2.0, 0.001, 2.0),
            ParamDef::range("arc_width", 0.5, 0.001, 1.0),
            ParamDef::new("rotation", 0.0),
            ParamDef::range("size", 1.0, 0.001, 10.0),
            ParamDef::range("seed", 50.0, 0.0, 100.0),
            ParamDef::int("direct_color", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (extended, n, width, size) = (d[0] as i32, d[1], d[2], d[4]);
            let onen = 1.0 / n;
            let mut seed = d[5].abs();
            let seed2 = (seed + (seed / 2.0) + SMALL_EPSILON).sqrt() / ((seed * 0.5) + SMALL_EPSILON) * 0.25;
            let mut r = -d[3];
            let rmax = 0.5 * (2f64.powf(1.0 / n) - 1.0) * width;
            let scale = (r.cos() - r.sin()) / amount;
            let mut x = a.x * scale;
            let mut y = a.y * scale;
            let intx = java_round(x) as i32;
            let inty = java_round(y) as i32;
            r = x - intx as f64;
            x = if r < 0.0 { 1.0 + r } else { r };
            r = y - inty as f64;
            y = if r < 0.0 { 1.0 + r } else { r };
            let tiletype = if seed == 0.0 {
                0.0
            } else if seed == 1.0 {
                1.0
            } else {
                let mut randint;
                if extended == 0 {
                    let xrand = java_round(a.x) * seed2;
                    let yrand = java_round(a.y) * seed2;
                    let niter = xrand + yrand + xrand * yrand;
                    randint = (niter + seed) * seed2 / 2.0;
                    randint = fmod(randint * 32747.0 + 12345.0, 65535.0);
                } else {
                    seed = seed.floor();
                    let xrand = java_round(a.x) as i32;
                    let yrand = java_round(a.y) as i32;
                    let mut niter = (xrand.wrapping_add(yrand).wrapping_add(xrand.wrapping_mul(yrand)) as f64).abs();
                    if niter > 1000.0 {
                        niter = 1000.0;
                    }
                    randint = seed + niter;
                    let mut randiter = 0;
                    while (randiter as f64) < niter {
                        randiter += 1;
                        randint = fmod(randint * 32747.0 + 12345.0, 65535.0);
                    }
                }
                fmod(randint, 2.0)
            };
            let rp = |dx: f64, dy: f64| ((dx.abs()).powf(n) + (dy.abs()).powf(n)).powf(onen);
            let (r0, r1) = if (extended == 0 && tiletype < 1.0) || (extended != 0 && tiletype == 1.0) {
                (rp(x, y), rp(x - 1.0, y - 1.0))
            } else {
                (rp(x - 1.0, y), rp(x, y - 1.0))
            };
            if (r0 - 0.5).abs() / rmax < 1.0 {
                if d[6] as i32 == 1 {
                    v.color = limit_value(r0, 0.0, 1.0);
                }
                v.x += size * (x + a.x.floor());
                v.y += size * (y + a.y.floor());
            }
            if (r1 - 0.5).abs() / rmax < 1.0 {
                if d[6] as i32 == 1 {
                    v.color = limit_value(1.0 - r1, 0.0, 1.0);
                }
                v.x += size * (x + a.x.floor());
                v.y += size * (y + a.y.floor());
                ctx.pass_z(a, v, amount);
            }
        },
        prepare: Some(|p, _| {
            vec![
                limit_value(ftoi(p[0]), 0, 1) as f64,
                limit_value(p[1], 0.001, 2.0),
                limit_value(p[2], 0.001, 1.0),
                p[3],
                limit_value(p[4], 0.001, 10.0),
                p[5],
                limit_value(ftoi(p[6]), 0, 1) as f64,
            ]
        }),
        // GPU: the tile hash runs in f32 (non-extended) / u32 (extended) arithmetic.
        wgsl: "fn truchet_rp(dx: f32, dy: f32, n: f32, onen: f32) -> f32 {
  return pow(pow(abs(dx), n) + pow(abs(dy), n), onen);
}
@body
let extended = i32(D(0));
let n = D(1);
let onen = 1.0 / n;
let size = D(4);
let seed = abs(D(5));
let seed2 = sqrt(seed + (seed / 2.0) + EPS) / ((seed * 0.5) + EPS) * 0.25;
let rot = -D(3);
let rmax = 0.5 * (pow(2.0, 1.0 / n) - 1.0) * D(2);
let scale = (cos(rot) - sin(rot)) / AMOUNT;
var x = a.x * scale;
var y = a.y * scale;
var r = x - java_round(x);
x = select(r, 1.0 + r, r < 0.0);
r = y - java_round(y);
y = select(r, 1.0 + r, r < 0.0);
var tiletype = 0.0;
if (seed == 1.0) {
  tiletype = 1.0;
} else if (seed != 0.0) {
  if (extended == 0) {
    let xrand = java_round(a.x) * seed2;
    let yrand = java_round(a.y) * seed2;
    let niter = xrand + yrand + xrand * yrand;
    let randint = fmod(((niter + seed) * seed2 / 2.0) * 32747.0 + 12345.0, 65535.0);
    tiletype = fmod(randint, 2.0);
  } else {
    let xrand = i32(java_round(a.x));
    let yrand = i32(java_round(a.y));
    let niter = min(abs(xrand + yrand + xrand * yrand), 1000);
    var randint = u32(floor(seed)) + u32(niter);
    for (var i = 0; i < niter; i++) { randint = (randint * 32747u + 12345u) % 65535u; }
    tiletype = f32(randint % 2u);
  }
}
var r0 = 0.0;
var r1 = 0.0;
if ((extended == 0 && tiletype < 1.0) || (extended != 0 && tiletype == 1.0)) {
  r0 = truchet_rp(x, y, n, onen);
  r1 = truchet_rp(x - 1.0, y - 1.0, n, onen);
} else {
  r0 = truchet_rp(x - 1.0, y, n, onen);
  r1 = truchet_rp(x, y - 1.0, n, onen);
}
if (abs(r0 - 0.5) / rmax < 1.0) {
  if (i32(D(6)) == 1) { v.color = clamp(r0, 0.0, 1.0); }
  v.x += size * (x + floor(a.x));
  v.y += size * (y + floor(a.y));
}
if (abs(r1 - 0.5) / rmax < 1.0) {
  if (i32(D(6)) == 1) { v.color = clamp(1.0 - r1, 0.0, 1.0); }
  v.x += size * (x + floor(a.x));
  v.y += size * (y + floor(a.y));
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    VarDef {
        name: "truchet2",
        params: &[
            ParamDef::range("exponent1", 1.0, -1.0, 3.0),
            ParamDef::range("exponent2", 2.0, -1.0, 3.0),
            ParamDef::range("width1", 0.5, -1.0, 2.0),
            ParamDef::range("width2", 0.5, -1.0, 2.0),
            ParamDef::new("scale", 10.0),
            ParamDef::range("seed", 50.0, 0.0, 100.0),
            ParamDef::int("inverse", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, _ctx, a, v, amount| {
            let scale = d[4];
            let xp = ((a.x / scale - (a.x / scale).floor()) - 0.5).abs() * 2.0;
            let mut width = d[2] * (1.0 - xp) + xp * d[3];
            width = if width < 1.0 { width } else { 1.0 };
            if width <= 0.0 {
                v.x += a.x * amount;
                v.y += a.y * amount;
                return;
            }
            let xp2 = d[0] * (1.0 - xp) + xp * d[1];
            let n = if xp2 < 2.0 { xp2 } else { 2.0 };
            if n <= 0.0 {
                v.x += a.x * amount;
                v.y += a.y * amount;
                return;
            }
            let onen = 1.0 / xp2;
            let seed = d[5].abs();
            let seed2 = (seed + (seed / 2.0) + SMALL_EPSILON).sqrt() / ((seed * 0.5) + SMALL_EPSILON) * 0.25;
            let (mut x, mut y) = (a.x, a.y);
            let mut r = x - java_round(x);
            x = if r < 0.0 { 1.0 + r } else { r };
            r = y - java_round(y);
            y = if r < 0.0 { 1.0 + r } else { r };
            let tiletype = if seed == 0.0 {
                0.0
            } else if seed == 1.0 {
                1.0
            } else {
                let xrand = java_round(a.x) * seed2;
                let yrand = java_round(a.y) * seed2;
                let niter = xrand + yrand + xrand * yrand;
                let randint = fmod(((niter + seed) * seed2 / 2.0) * 32747.0 + 12345.0, 65535.0);
                fmod(randint, 2.0)
            };
            let rp = |dx: f64, dy: f64| ((dx.abs()).powf(n) + (dy.abs()).powf(n)).powf(onen);
            let (r0, r1) = if tiletype < 1.0 { (rp(x, y), rp(x - 1.0, y - 1.0)) } else { (rp(x - 1.0, y), rp(x, y - 1.0)) };
            let rmax = 0.5 * (2f64.powf(onen) - 1.0) * width;
            let r00 = (r0 - 0.5).abs() / rmax;
            let r11 = (r1 - 0.5).abs() / rmax;
            if d[6] as i32 == 0 {
                if r00 < 1.0 || r11 < 1.0 {
                    v.x += (x + a.x.floor()) * amount;
                    v.y += (y + a.y.floor()) * amount;
                } else {
                    v.x += 100.0;
                    v.y += 100.0;
                }
            } else if r00 > 1.0 && r11 > 1.0 {
                v.x += (x + a.x.floor()) * amount;
                v.y += (y + a.y.floor()) * amount;
            } else {
                v.x += 10000.0;
                v.y += 10000.0;
            }
        },
        prepare: Some(|p, _| {
            vec![
                limit_value(p[0], -1.0, 3.0),
                limit_value(p[1], -1.0, 3.0),
                limit_value(p[2], -1.0, 2.0),
                limit_value(p[3], -1.0, 2.0),
                p[4],
                p[5],
                limit_value(ftoi(p[6]), 0, 1) as f64,
            ]
        }),
        // GPU: the tile hash runs in f32 arithmetic.
        wgsl: "fn truchet2_rp(dx: f32, dy: f32, n: f32, onen: f32) -> f32 {
  return pow(pow(abs(dx), n) + pow(abs(dy), n), onen);
}
@body
let scale = D(4);
let xp = abs((a.x / scale - floor(a.x / scale)) - 0.5) * 2.0;
let width = min(D(2) * (1.0 - xp) + xp * D(3), 1.0);
let xp2 = D(0) * (1.0 - xp) + xp * D(1);
let n = min(xp2, 2.0);
if (width <= 0.0 || n <= 0.0) {
  v.x += a.x * AMOUNT;
  v.y += a.y * AMOUNT;
} else {
  let onen = 1.0 / xp2;
  let seed = abs(D(5));
  let seed2 = sqrt(seed + (seed / 2.0) + EPS) / ((seed * 0.5) + EPS) * 0.25;
  var r = a.x - java_round(a.x);
  let x = select(r, 1.0 + r, r < 0.0);
  r = a.y - java_round(a.y);
  let y = select(r, 1.0 + r, r < 0.0);
  var tiletype = 0.0;
  if (seed == 1.0) {
    tiletype = 1.0;
  } else if (seed != 0.0) {
    let xrand = java_round(a.x) * seed2;
    let yrand = java_round(a.y) * seed2;
    let niter = xrand + yrand + xrand * yrand;
    tiletype = fmod(fmod(((niter + seed) * seed2 / 2.0) * 32747.0 + 12345.0, 65535.0), 2.0);
  }
  var r0 = 0.0;
  var r1 = 0.0;
  if (tiletype < 1.0) {
    r0 = truchet2_rp(x, y, n, onen);
    r1 = truchet2_rp(x - 1.0, y - 1.0, n, onen);
  } else {
    r0 = truchet2_rp(x - 1.0, y, n, onen);
    r1 = truchet2_rp(x, y - 1.0, n, onen);
  }
  let rmax = 0.5 * (pow(2.0, onen) - 1.0) * width;
  let r00 = abs(r0 - 0.5) / rmax;
  let r11 = abs(r1 - 0.5) / rmax;
  if (i32(D(6)) == 0) {
    if (r00 < 1.0 || r11 < 1.0) {
      v.x += (x + floor(a.x)) * AMOUNT;
      v.y += (y + floor(a.y)) * AMOUNT;
    } else {
      v.x += 100.0;
      v.y += 100.0;
    }
  } else if (r00 > 1.0 && r11 > 1.0) {
    v.x += (x + floor(a.x)) * AMOUNT;
    v.y += (y + floor(a.y)) * AMOUNT;
  } else {
    v.x += 10000.0;
    v.y += 10000.0;
  }
}",
    },
    VarDef {
        name: "truchet_ae",
        params: &[
            ParamDef::int("seed", 0.0, 0.0, 1000.0),
            ParamDef::new("step", 0.5),
            ParamDef::new("width", 0.25),
            ParamDef::new("angle", 0.0),
            ParamDef::int("layout", 0.0, 0.0, 4.0),
            ParamDef::int("style", 0.0, 0.0, 5.0),
            ParamDef::new("K01A", 0.5),
            ParamDef::new("K02A", 0.5),
            ParamDef::new("K01B", 0.5),
            ParamDef::new("K02B", 0.5),
            ParamDef::new("K11A", 0.5),
            ParamDef::new("K12A", 0.5),
            ParamDef::new("K11B", 0.5),
            ParamDef::new("K12B", 0.5),
            ParamDef::new("K21A", 0.5),
            ParamDef::new("K22A", 0.5),
            ParamDef::new("K21B", 0.5),
            ParamDef::new("K22B", 0.5),
            ParamDef::new("K31A", 0.5),
            ParamDef::new("K32A", 0.5),
            ParamDef::new("K31B", 0.5),
            ParamDef::new("K32B", 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [22 params, sinang, cosang]
        apply: |d, ctx, a, v, amount| {
            // Java iterates one orbit (x, y, z1, z2) per instance; here one per render thread.
            let key = d.as_ptr() as usize;
            let xy = ctx.orbits.get(&key).copied().unwrap_or([0.0, 0.0]);
            let zz = ctx.orbits.get(&(key + 1)).copied().unwrap_or([0.0, 0.0]);
            let mut st = [xy[0], xy[1], zz[0], zz[1]];
            let i = (4.0 * ctx.random()).floor() as i32;
            let (u1, v1, z) = truchet_ae_step(d, &mut st, i);
            ctx.orbits.insert(key, [u1, v1]);
            ctx.orbits.insert(key + 1, [z, z]);
            v.x += u1;
            v.y += v1;
            ctx.pass_z(a, v, amount);
            v.color = z;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = ftoi(p[0]) as f64;
            d[4] = limit_value(ftoi(p[4]), 0, 4) as f64;
            d[5] = limit_value(ftoi(p[5]), 0, 5) as f64;
            d.extend([(p[3] * M_PI).sin(), (p[3] * M_PI).cos()]);
            d
        }),
        // GPU: no state across calls, every call iterates the orbit from the origin.
        wgsl: TRUCHET_AE_WGSL,
    },
    VarDef {
        name: "truchet_fill",
        params: &[ParamDef::range("exponent", 0.0, 0.001, 2.0), ParamDef::range("arc_width", 0.0, 0.001, 1.0), ParamDef::new("seed", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [exponent, arc_width, seed, exponent (clamped), onen, width, seed2]
        apply: |d, ctx, a, v, amount| {
            let (seed, exponent, onen, width, seed2) = (d[2], d[3], d[4], d[5], d[6]);
            let rmax = 0.5 * (2f64.powf(onen) - 1.0) * width;
            let scale = 1.0 / amount;
            let mut x = a.x * scale;
            let mut y = a.y * scale;
            let mut r = x - java_round(x);
            x = if r < 0.0 { 1.0 + r } else { r };
            r = y - java_round(y);
            y = if r < 0.0 { 1.0 + r } else { r };
            let tiletype = if seed == 0.0 {
                0.0
            } else if seed == 1.0 {
                1.0
            } else {
                let xrand = java_round(a.x.abs()) * seed2;
                let yrand = java_round(a.y.abs()) * seed2;
                let niter = xrand + yrand + xrand * yrand;
                let randint = (seed + niter) * seed2 * 0.5;
                fmod(fmod(randint * 32747.0 + 12345.0, 65535.0), 2.0)
            };
            let rp = |dx: f64, dy: f64| ((dx.abs()).powf(exponent) + (dy.abs()).powf(exponent)).powf(onen);
            let (r0, r1) = if tiletype < 1.0 { (rp(x, y), rp(x - 1.0, y - 1.0)) } else { (rp(x - 1.0, y), rp(x, y - 1.0)) };
            let (x1, y1) = if (r0 - 0.5).abs() / rmax < 1.0 { (2.0 * (x + a.x.floor()), 2.0 * (y + a.y.floor())) } else { (0.0, 0.0) };
            if (r1 - 0.5).abs() / rmax < 1.0 {
                v.x += x1 + 2.0 * (x + a.x.floor()) - a.x;
                v.y += y1 + 2.0 * (y + a.y.floor()) - a.y;
            } else {
                v.x += x1 - a.x;
                v.y += y1 - a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let exponent = limit_value(p[0], 0.001, 2.0);
            let width = limit_value(p[1], 0.001, 1.0);
            vec![p[0], p[1], p[2], exponent, 1.0 / exponent, width, (p[2] * 1.5).sqrt() / (p[2] * 0.5) * 0.25]
        }),
        // GPU: the tile hash runs in f32 arithmetic.
        wgsl: "fn truchet_fill_rp(dx: f32, dy: f32, n: f32, onen: f32) -> f32 {
  return pow(pow(abs(dx), n) + pow(abs(dy), n), onen);
}
@body
let seed = D(2);
let exponent = D(3);
let onen = D(4);
let seed2 = D(6);
let rmax = 0.5 * (pow(2.0, onen) - 1.0) * D(5);
let scale = 1.0 / AMOUNT;
var x = a.x * scale;
var y = a.y * scale;
var r = x - java_round(x);
x = select(r, 1.0 + r, r < 0.0);
r = y - java_round(y);
y = select(r, 1.0 + r, r < 0.0);
var tiletype = 0.0;
if (seed == 1.0) {
  tiletype = 1.0;
} else if (seed != 0.0) {
  let xrand = java_round(abs(a.x)) * seed2;
  let yrand = java_round(abs(a.y)) * seed2;
  let niter = xrand + yrand + xrand * yrand;
  tiletype = fmod(fmod((seed + niter) * seed2 * 0.5 * 32747.0 + 12345.0, 65535.0), 2.0);
}
var r0 = 0.0;
var r1 = 0.0;
if (tiletype < 1.0) {
  r0 = truchet_fill_rp(x, y, exponent, onen);
  r1 = truchet_fill_rp(x - 1.0, y - 1.0, exponent, onen);
} else {
  r0 = truchet_fill_rp(x - 1.0, y, exponent, onen);
  r1 = truchet_fill_rp(x, y - 1.0, exponent, onen);
}
var x1 = 0.0;
var y1 = 0.0;
if (abs(r0 - 0.5) / rmax < 1.0) {
  x1 = 2.0 * (x + floor(a.x));
  y1 = 2.0 * (y + floor(a.y));
}
if (abs(r1 - 0.5) / rmax < 1.0) {
  v.x += x1 + 2.0 * (x + floor(a.x)) - a.x;
  v.y += y1 + 2.0 * (y + floor(a.y)) - a.y;
} else {
  v.x += x1 - a.x;
  v.y += y1 - a.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "truchetflow",
        params: &[ParamDef::int("randomize", 0.0, 0.0, 1000.0), ParamDef::range("zoom", 5.0, 0.1, 50.0), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [seed, zoom, invert]
        apply: |d, ctx, a, v, amount| {
            let mut x = ctx.random();
            let mut y = ctx.random();
            let (seed, zoom) = (d[0], d[1]);
            let uvx = seed * (seed * 180.0 / M_PI).sin();
            let ux = (x - 0.5) * zoom - uvx;
            let uy = (y - 0.5) * zoom - uvx;
            let fract = |t: f64| t - t.floor();
            let mut gx = fract(ux) - 0.5;
            let gy = fract(uy) - 0.5;
            // N21(floor(uv))
            let (mut px, mut py) = (fract(ux.floor() * 234.234), fract(uy.floor() * 823.923));
            let dp = px * (px + 42.34) + py * (py + 42.34);
            px += dp;
            py += dp;
            let n = fract(px * py);
            let band = -0.03 + (0.5 * y).powf(1.5);
            let width = 0.01 + 0.1 * x;
            if n > 0.5 {
                gx *= -1.0;
            }
            let t = gx + gy + 0.001;
            let s = 0.5 * if t > 0.0 { 1.0 } else if t < 0.0 { -1.0 } else { t };
            let mut dd = ((gx - s) * (gx - s) + (gy - s) * (gy - s)).sqrt() - 0.5;
            dd = dd.abs();
            dd = (dd - band).abs();
            dd = (dd - band).abs();
            dd = (dd - band).abs();
            dd = dd.abs() - width;
            let fuzzy = 0.01 + 0.2 * ((x - 0.5).abs() + (y - 0.5).abs()).powf(2.0);
            let tv = java_min(java_max((dd - fuzzy) / (0.0 - fuzzy), 0.0), 1.0);
            let col = tv * tv * (3.0 - 2.0 * tv);
            v.do_hide = false;
            if (d[2] as i32 == 0 && col > 0.0) || (d[2] as i32 != 0 && col <= 0.0) {
                x = 0.0;
                y = 0.0;
                v.do_hide = true;
            }
            v.x = amount * (x - 0.5);
            v.y = amount * (y - 0.5);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0] as i32 as f64, limit_value(p[1], 0.1, 50.0), limit_value(p[2], 0.0, 1.0) as i32 as f64]),
        wgsl: "var x = rnd();
var y = rnd();
let seed = D(0);
let zoom = D(1);
let uvx = seed * sin(seed * 180.0 / PI);
let uv = vec2<f32>((x - 0.5) * zoom - uvx, (y - 0.5) * zoom - uvx);
var gv = fract(uv) - 0.5;
var p = fract(floor(uv) * vec2<f32>(234.234, 823.923));
p = p + dot(p, p + 42.34);
let n = fract(p.x * p.y);
let band = -0.03 + pow(0.5 * y, 1.5);
let width = 0.01 + 0.1 * x;
if (n > 0.5) { gv.x = -gv.x; }
let s = 0.5 * sign(gv.x + gv.y + 0.001);
var dd = abs(length(gv - s) - 0.5);
dd = abs(dd - band);
dd = abs(dd - band);
dd = abs(dd - band);
dd = abs(dd) - width;
let fuzzy = 0.01 + 0.2 * pow(abs(x - 0.5) + abs(y - 0.5), 2.0);
let col = glsl_smoothstep(fuzzy, 0.0, dd);
v.hide = 0.0;
if ((i32(D(2)) == 0 && col > 0.0) || (i32(D(2)) != 0 && col <= 0.0)) {
  x = 0.0;
  y = 0.0;
  v.hide = 1.0;
}
v.x = AMOUNT * (x - 0.5);
v.y = AMOUNT * (y - 0.5);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "truchet_hex_crop",
        params: &[ParamDef::range("wd", 0.05, 0.0, 5.0), ParamDef::int("mode", 0.0, 0.0, 2.0), ParamDef::int("inv", 0.0, 0.0, 1.0), ParamDef::int("seed", 13.0, 0.0, 100.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            const SQRT3_2: f64 = 0.866_025_403_784_438_6;
            const SQRT3: f64 = 1.732_050_807_568_877_2;
            const PI_3: f64 = std::f64::consts::FRAC_PI_3;
            const PI_6: f64 = std::f64::consts::FRAC_PI_6;
            let (wd, mode, inv, seed) = (d[0], d[1] as i32, d[2] as i32, d[3] as i32);
            let x = 0.577_350_269_189_625_7 * a.x - a.y / 3.0;
            let z = 2.0 * a.y / 3.0;
            let y = -x - z;
            let (mut rx, ry, mut rz) = (java_round(x), java_round(y), java_round(z));
            let (x_diff, y_diff, z_diff) = ((rx - x).abs(), (ry - y).abs(), (rz - z).abs());
            // (the ry correction of the y_diff case is not used afterwards)
            if x_diff > y_diff && x_diff > z_diff {
                rx = -ry - rz;
            } else if y_diff <= z_diff || y_diff.is_nan() || z_diff.is_nan() {
                rz = -rx - ry;
            }
            let fx_h = SQRT3 * rx + SQRT3_2 * rz;
            let fy_h = 1.5 * rz;
            let mut fx = a.x - fx_h;
            let mut fy = a.y - fy_h;
            let mut add = 0.0;
            if seed == 1 && fmod(rx, 2.0) == 0.0 && fmod(rz, 2.0) == 0.0 {
                add = PI_3;
            }
            if seed >= 2 {
                let mut hash_f = (fx_h * 12.9898 + fy_h * 78.233 + seed as f64).sin() * 43758.5453;
                hash_f -= hash_f.floor();
                if hash_f < 0.5 {
                    add = PI_3;
                }
            }
            let angle = fy.atan2(fx) + PI_6 - add;
            let coef = 0.477_464_829_275_686;
            let angle2 = (angle * coef).floor() / coef + PI_6 + add;
            let (x0, y0) = (angle2.cos(), angle2.sin());
            let dist = (sqr(fx - x0) + sqr(fy - y0)).sqrt();
            let d1 = 0.5 + wd;
            let d2 = 0.5 - wd;
            let hit = if inv == 1 { dist > d1 || dist < d2 } else { dist < d1 && dist > d2 };
            if hit {
                if mode == 0 {
                    fx = 0.0;
                    fy = 0.0;
                } else if mode == 1 {
                    fx = x0;
                    fy = y0;
                } else {
                    let rangle = (fy - y0).atan2(fx - x0);
                    let dd = if ctx.random() < 0.5 { d1 } else { d2 };
                    fx = x0 + rangle.cos() * dd;
                    fy = y0 + rangle.sin() * dd;
                }
            }
            v.x += (fx + fx_h) * amount;
            v.y += (fy + fy_h) * amount;
        },
        prepare: Some(|p, _| {
            vec![limit_value(p[0], 0.0, 5.0), limit_value(p[1], 0.0, 2.0) as i32 as f64, limit_value(p[2], 0.0, 1.0) as i32 as f64, p[3] as i32 as f64]
        }),
        wgsl: "let SQRT3_2 = 0.8660254037844386;
let SQRT3 = 1.7320508075688772;
let PI_3 = 1.0471975511965976;
let PI_6 = 0.5235987755982988;
let wd = D(0);
let mode = i32(D(1));
let seed = i32(D(3));
let x = 0.5773502691896258 * a.x - a.y / 3.0;
let z = 2.0 * a.y / 3.0;
let y = -x - z;
var rx = java_round(x);
let ry = java_round(y);
var rz = java_round(z);
let x_diff = abs(rx - x);
let y_diff = abs(ry - y);
let z_diff = abs(rz - z);
if (x_diff > y_diff && x_diff > z_diff) {
  rx = -ry - rz;
} else if (!(y_diff > z_diff)) {
  rz = -rx - ry;
}
let fx_h = SQRT3 * rx + SQRT3_2 * rz;
let fy_h = 1.5 * rz;
var fx = a.x - fx_h;
var fy = a.y - fy_h;
var add = 0.0;
if (seed == 1 && fmod(rx, 2.0) == 0.0 && fmod(rz, 2.0) == 0.0) { add = PI_3; }
if (seed >= 2) {
  let hash_f = fract(sin(fx_h * 12.9898 + fy_h * 78.233 + f32(seed)) * 43758.5453);
  if (hash_f < 0.5) { add = PI_3; }
}
let angle = atan2(fy, fx) + PI_6 - add;
let coef = 0.477464829275686;
let angle2 = floor(angle * coef) / coef + PI_6 + add;
let x0 = cos(angle2);
let y0 = sin(angle2);
let dist = sqrt(sqr(fx - x0) + sqr(fy - y0));
let d1 = 0.5 + wd;
let d2 = 0.5 - wd;
var hit = dist < d1 && dist > d2;
if (i32(D(2)) == 1) { hit = dist > d1 || dist < d2; }
if (hit) {
  if (mode == 0) {
    fx = 0.0;
    fy = 0.0;
  } else if (mode == 1) {
    fx = x0;
    fy = y0;
  } else {
    let rangle = atan2(fy - y0, fx - x0);
    let dd = select(d2, d1, rnd() < 0.5);
    fx = x0 + cos(rangle) * dd;
    fy = y0 + sin(rangle) * dd;
  }
}
v.x += (fx + fx_h) * AMOUNT;
v.y += (fy + fy_h) * AMOUNT;",
    },
    VarDef {
        name: "truchet_hex_fill",
        params: &[
            ParamDef::int("n", 3.0, 1.0, 20.0),
            ParamDef::int("flipx", 1.0, 0.0, 1.0),
            ParamDef::int("flipy", 1.0, 0.0, 1.0),
            ParamDef::new("spreadx", 1.0),
            ParamDef::new("spready", 1.0),
            ParamDef::int("seed", 13.0, 0.0, 100.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            const SQRT3_2: f64 = 0.866_025_403_784_438_6;
            const SQRT3: f64 = 1.732_050_807_568_877_2;
            let (n, flipx, flipy, spreadx, spready, seed) = (d[0] as i32, d[1] as i32, d[2] as i32, d[3], d[4], d[5] as i32);
            let l = ctx.random().ln();
            let rx = (l * if ctx.random() < 0.5 { spreadx } else { -spreadx }).floor();
            let l = ctx.random().ln();
            let rz = (l * if ctx.random() < 0.5 { spready } else { -spready }).floor();
            let fx_h = SQRT3 * rx + SQRT3_2 * if ctx.random() < 0.5 { rz } else { -rz };
            let fy_h = 1.5 * rz;
            let mut add = true;
            if seed == 1 && fmod(rx, 2.0) == 0.0 && fmod(rz, 2.0) == 0.0 {
                add = false;
            }
            if seed >= 2 {
                let mut hash_f = (fx_h * 12.9898 + fy_h * 78.233 + seed as f64).sin() * 43758.5453;
                hash_f -= hash_f.floor();
                if hash_f < 0.5 {
                    add = false;
                }
            }
            let nnn = 3.0 * n as f64;
            let nnnn = 1.0 / nnn;
            let s1 = (M_PI * nnnn).exp();
            let k_factor = 2.0 / (s1 + 1.0 / s1);
            let rangle = (ctx.random() * nnn).floor() * M_2PI * nnnn;
            let y_aux = if flipy == 0 && !add { -a.y } else { a.y };
            let x_aux = if flipx == 0 && !add { -a.x } else { a.x };
            let fy0 = y_aux * nnnn;
            let fx0 = x_aux * nnnn;
            let sn = (fy0 * M_PI + rangle).sin();
            let cs = (fy0 * M_PI + rangle).cos();
            let aa = k_factor * (fx0 * M_PI).exp();
            let (fx, fy) = (aa * cs, aa * sn);
            let mut ang_a = fy.atan2(fx);
            if ang_a < 0.0 {
                ang_a += M_2PI;
            }
            ang_a = (1.5 * ang_a * M_1_PI).floor();
            let ang = (M_PI + ang_a * M_2PI) / 3.0;
            let fx_new = fx - ang.cos() * 2.0;
            let fy_new = fy - ang.sin() * 2.0;
            let (fx, fy) = if add {
                (SQRT3_2 * fx_new - 0.5 * fy_new, 0.5 * fx_new + SQRT3_2 * fy_new)
            } else {
                (SQRT3_2 * fx_new + 0.5 * fy_new, -0.5 * fx_new + SQRT3_2 * fy_new)
            };
            v.x += amount * (fx * 0.5 + fx_h);
            v.y += amount * (fy * 0.5 + fy_h);
        },
        prepare: Some(|p, _| {
            vec![p[0] as i32 as f64, limit_value(p[1], 0.0, 1.0) as i32 as f64, limit_value(p[2], 0.0, 1.0) as i32 as f64, p[3], p[4], p[5] as i32 as f64]
        }),
        wgsl: "let SQRT3_2 = 0.8660254037844386;
let SQRT3 = 1.7320508075688772;
let seed = i32(D(5));
let l1 = log(rnd());
let rx = floor(l1 * select(-D(3), D(3), rnd() < 0.5));
let l2 = log(rnd());
let rz = floor(l2 * select(-D(4), D(4), rnd() < 0.5));
let fx_h = SQRT3 * rx + SQRT3_2 * select(-rz, rz, rnd() < 0.5);
let fy_h = 1.5 * rz;
var add = true;
if (seed == 1 && fmod(rx, 2.0) == 0.0 && fmod(rz, 2.0) == 0.0) { add = false; }
if (seed >= 2) {
  let hash_f = fract(sin(fx_h * 12.9898 + fy_h * 78.233 + f32(seed)) * 43758.5453);
  if (hash_f < 0.5) { add = false; }
}
let nnn = 3.0 * f32(i32(D(0)));
let nnnn = 1.0 / nnn;
let s1 = exp(PI * nnnn);
let k_factor = 2.0 / (s1 + 1.0 / s1);
let rangle = floor(rnd() * nnn) * TWO_PI * nnnn;
var y_aux = a.y;
if (i32(D(2)) == 0 && !add) { y_aux = -a.y; }
var x_aux = a.x;
if (i32(D(1)) == 0 && !add) { x_aux = -a.x; }
let fy0 = y_aux * nnnn;
let fx0 = x_aux * nnnn;
let aa = k_factor * exp(fx0 * PI);
let fx = aa * cos(fy0 * PI + rangle);
let fy = aa * sin(fy0 * PI + rangle);
var ang_a = atan2(fy, fx);
if (ang_a < 0.0) { ang_a += TWO_PI; }
ang_a = floor(1.5 * ang_a / PI);
let ang = (PI + ang_a * TWO_PI) / 3.0;
let fx_new = fx - cos(ang) * 2.0;
let fy_new = fy - sin(ang) * 2.0;
var ox = SQRT3_2 * fx_new + 0.5 * fy_new;
var oy = -0.5 * fx_new + SQRT3_2 * fy_new;
if (add) {
  ox = SQRT3_2 * fx_new - 0.5 * fy_new;
  oy = 0.5 * fx_new + SQRT3_2 * fy_new;
}
v.x += AMOUNT * (ox * 0.5 + fx_h);
v.y += AMOUNT * (oy * 0.5 + fy_h);",
    },
    VarDef {
        name: "tunnel",
        params: &[ParamDef::new("Sx", 200.0), ParamDef::new("Sy", 50.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (ux, uy) = (a.x + 0.5, a.y + 0.5);
            let deform = 0.1;
            let (dist_x, dist_y) = (0.5 - ux, 0.5 - uy);
            let distortion = -(0.25 - (uy * deform - deform * 0.5).powf(2.0)).sqrt() * deform + deform * 0.5;
            let shift_x = distortion * dist_x * d[0];
            let deform_y_fixed = (deform - distortion) * deform;
            let shift_y = d[1] * deform_y_fixed * dist_y;
            v.x = amount * (v.x + shift_x);
            v.y = amount * (v.y + shift_y);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ux = a.x + 0.5;
let uy = a.y + 0.5;
let deform = 0.1;
let distortion = -sqrt(0.25 - pow(uy * deform - deform * 0.5, 2.0)) * deform + deform * 0.5;
let shift_x = distortion * (0.5 - ux) * D(0);
let shift_y = D(1) * ((deform - distortion) * deform) * (0.5 - uy);
v.x = AMOUNT * (v.x + shift_x);
v.y = AMOUNT * (v.y + shift_y);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tunneling",
        params: &[ParamDef::new("barrier", 0.5), ParamDef::new("decay", 3.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let qt_x = a.x - d[0];
            let qt_prob = if qt_x > 0.0 { (-d[1] * qt_x).exp() } else { 1.0 };
            v.x += amount * a.x * qt_prob;
            v.y += amount * a.y * qt_prob;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let qt_x = a.x - D(0);
var qt_prob = 1.0;
if (qt_x > 0.0) { qt_prob = exp(-D(1) * qt_x); }
v.x += AMOUNT * a.x * qt_prob;
v.y += AMOUNT * a.y * qt_prob;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "turbulence",
        params: &[ParamDef::new("freq", 2.0), ParamDef::new("amp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (f, am) = (d[0], d[1]);
            let dx = am * ((f * a.x).sin() * (f * 0.7 * a.y).cos() + 0.5 * (2.0 * f * a.x).sin() * (1.4 * f * a.y).cos());
            let dy = am * ((0.8 * f * a.x).cos() * (f * a.y).sin() + 0.5 * (1.6 * f * a.x).cos() * (2.0 * f * a.y).sin());
            v.x += amount * (a.x + dx);
            v.y += amount * (a.y + dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let f = D(0);
let dx = D(1) * (sin(f * a.x) * cos(f * 0.7 * a.y) + 0.5 * sin(2.0 * f * a.x) * cos(1.4 * f * a.y));
let dy = D(1) * (cos(0.8 * f * a.x) * sin(f * a.y) + 0.5 * cos(1.6 * f * a.x) * sin(2.0 * f * a.y));
v.x += AMOUNT * (a.x + dx);
v.y += AMOUNT * (a.y + dy);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "tweed",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("amp", 0.2)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let d1 = ((a.x + a.y) * d[0]) - ((a.x + a.y) * d[0]).floor();
            let d2 = ((a.x - a.y) * d[0]) - ((a.x - a.y) * d[0]).floor();
            let p = (d1 * M_PI).sin();
            let c = (d2 * M_PI).sin();
            let sgn = |t: f64| if t == 0.0 { 0.0 } else if t < 0.0 { -1.0 } else { 1.0 };
            v.x += amount * (a.x + d[1] * p * sgn(c));
            v.y += amount * (a.y - d[1] * c * sgn(p));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let d1 = fract((a.x + a.y) * D(0));
let d2 = fract((a.x - a.y) * D(0));
let p = sin(d1 * PI);
let c = sin(d2 * PI);
v.x += AMOUNT * (a.x + D(1) * p * sign(c));
v.y += AMOUNT * (a.y - D(1) * c * sign(p));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "twist_ribbon",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("width", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (s, c) = (d[0] * a.x).sin_cos();
            v.x += amount * (c * a.x - s * a.y * d[1]);
            v.y += amount * (s * a.x + c * a.y * d[1]);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let s = sin(D(0) * a.x);
let c = cos(D(0) * a.x);
v.x += AMOUNT * (c * a.x - s * a.y * D(1));
v.y += AMOUNT * (s * a.x + c * a.y * D(1));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "twoface",
        false,
        |_, ctx, a, v, amount| {
            let mut r = amount;
            if a.x > 0.0 {
                r /= sqr(a.x) + sqr(a.y);
            }
            v.x += r * a.x;
            v.y += r * a.y;
            ctx.pass_z(a, v, amount);
        },
        "var r = AMOUNT;
if (a.x > 0.0) { r = r / (sqr(a.x) + sqr(a.y)); }
v.x += r * a.x;
v.y += r * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "unpolar",
        false,
        |_, ctx, a, v, amount| {
            let vvar_2 = amount / M_PI * 0.5;
            let r = a.y.exp();
            v.y += vvar_2 * r * a.x.cos();
            v.x += vvar_2 * r * a.x.sin();
            ctx.pass_z(a, v, amount);
        },
        "let vvar_2 = AMOUNT / PI * 0.5;
let r = exp(a.y);
v.y += vvar_2 * r * cos(a.x);
v.x += vvar_2 * r * sin(a.x);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "vibration2",
        params: &[
            ParamDef::new("dir", 0.0),
            ParamDef::new("angle", 1.5708),
            ParamDef::new("freq", 1.0),
            ParamDef::new("amp", 0.25),
            ParamDef::new("phase", 0.0),
            ParamDef::new("dir2", 1.5708),
            ParamDef::new("angle2", 1.5708),
            ParamDef::new("freq2", 1.0),
            ParamDef::new("amp2", 0.25),
            ParamDef::new("phase2", 0.0),
            ParamDef::new("dm", 0.0),
            ParamDef::new("dmfreq", 0.1),
            ParamDef::new("tm", 0.0),
            ParamDef::new("tmfreq", 0.1),
            ParamDef::new("fm", 0.0),
            ParamDef::new("fmfreq", 0.1),
            ParamDef::new("am", 0.0),
            ParamDef::new("amfreq", 0.1),
            ParamDef::new("d2m", 0.0),
            ParamDef::new("d2mfreq", 0.1),
            ParamDef::new("t2m", 0.0),
            ParamDef::new("t2mfreq", 0.1),
            ParamDef::new("f2m", 0.0),
            ParamDef::new("f2mfreq", 0.1),
            ParamDef::new("a2m", 0.0),
            ParamDef::new("a2mfreq", 0.1),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // wave k: (dir, angle, freq, amp, phase) at 5k, modulations (amp, freq) x 4 at 10 + 8k
            let wave = |w: &[f64], m: &[f64]| {
                let modulate = |amp: f64, freq: f64, x: f64| amp * (x * freq * M_2PI).cos();
                let (dir, angle, freq, amp, phase) = (w[0], w[1], w[2], w[3], w[4]);
                let mut dd = a.x * dir.cos() + a.y * dir.sin();
                let dir_l = dir + modulate(m[0], m[1], dd);
                let angle_l = angle + modulate(m[2], m[3], dd);
                let freq_l = modulate(m[4], m[5], dd) / freq;
                let amp_l = amp + amp * modulate(m[6], m[7], dd);
                let total = angle_l + dir_l;
                dd = a.x * dir_l.cos() + a.y * dir_l.sin();
                let local_amp = amp_l * ((dd * (freq * M_2PI)) + freq_l + M_2PI * phase / freq).sin();
                (local_amp * total.cos(), local_amp * total.sin())
            };
            let (dx1, dy1) = wave(&d[0..5], &d[10..18]);
            let (dx2, dy2) = wave(&d[5..10], &d[18..26]);
            let mut x = a.x + dx1;
            let mut y = a.y + dy1;
            x += dx2;
            y += dy2;
            v.x += amount * x;
            v.y += amount * y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "fn vibration2_wave(base: u32, w: u32, m: u32, ax: f32, ay: f32) -> vec2<f32> {
  let dir = prm[base + w];
  let angle = prm[base + w + 1u];
  let freq = prm[base + w + 2u];
  let amp = prm[base + w + 3u];
  let phase = prm[base + w + 4u];
  var dd = ax * cos(dir) + ay * sin(dir);
  let dir_l = dir + prm[base + m] * cos(dd * prm[base + m + 1u] * TWO_PI);
  let angle_l = angle + prm[base + m + 2u] * cos(dd * prm[base + m + 3u] * TWO_PI);
  let freq_l = prm[base + m + 4u] * cos(dd * prm[base + m + 5u] * TWO_PI) / freq;
  let amp_l = amp + amp * prm[base + m + 6u] * cos(dd * prm[base + m + 7u] * TWO_PI);
  let total = angle_l + dir_l;
  dd = ax * cos(dir_l) + ay * sin(dir_l);
  let local_amp = amp_l * sin((dd * (freq * TWO_PI)) + freq_l + TWO_PI * phase / freq);
  return local_amp * vec2<f32>(cos(total), sin(total));
}
@body
let w1 = vibration2_wave(base, 0u, 10u, a.x, a.y);
let w2 = vibration2_wave(base, 5u, 18u, a.x, a.y);
v.x += AMOUNT * (a.x + w1.x + w2.x);
v.y += AMOUNT * (a.y + w1.y + w2.y);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "vine",
        params: &[ParamDef::new("density", 1.0), ParamDef::new("scale", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let density = java_max(d[0].abs(), 0.01);
            let scale = java_max(d[1].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let n = r * density;
            let spiral = theta + n * 2.39996323;
            let new_r = n.sqrt() * scale;
            v.x += amount * new_r * spiral.cos();
            v.y += amount * new_r * spiral.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let n = sqrt(a.x * a.x + a.y * a.y) * max(abs(D(0)), 0.01);
let spiral = atan2(a.y, a.x) + n * 2.39996323;
let new_r = sqrt(n) * max(abs(D(1)), 0.01);
v.x += AMOUNT * new_r * cos(spiral);
v.y += AMOUNT * new_r * sin(spiral);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "virus",
        params: &[ParamDef::new("spread", 2.0), ParamDef::new("mut", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let s = (a.x * d[0]).sin() * (a.y * d[0]).cos();
            let sgn = |t: f64| if t == 0.0 { 0.0 } else if t < 0.0 { -1.0 } else { 1.0 };
            let nx = a.x + d[1] * s * sgn(a.x);
            let ny = a.y - d[1] * s * sgn(a.y);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let nr = (nx * nx + ny * ny).sqrt();
            let scale = r / (nr + 1e-6);
            v.x += amount * nx * scale;
            v.y += amount * ny * scale;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let s = sin(a.x * D(0)) * cos(a.y * D(0));
let nx = a.x + D(1) * s * sign(a.x);
let ny = a.y - D(1) * s * sign(a.y);
let scale = sqrt(a.x * a.x + a.y * a.y) / (sqrt(nx * nx + ny * ny) + 1e-6);
v.x += AMOUNT * nx * scale;
v.y += AMOUNT * ny * scale;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "vogel",
        params: &[ParamDef::int("n", 20.0, 1.0, 1000.0), ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            const M_PHI: f64 = 1.618_033_988_749_895;
            let i = ctx.random_int(d[0] as usize) + 1;
            let ang = i as f64 * (M_2PI / (M_PHI * M_PHI));
            let r = amount * (a.rad() + (i as f64).sqrt());
            v.x += r * (ang.cos() + (d[1] * a.x));
            v.y += r * (ang.sin() + (d[1] * a.y));
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![limit_value(ftoi(p[0]), 1, i32::MAX) as f64, p[1]]),
        wgsl: "let phi = 1.618033988749895;
let i = f32(min(i32(rnd() * D(0)), i32(D(0)) - 1) + 1);
let ang = i * (TWO_PI / (phi * phi));
let r = AMOUNT * (rad(a) + sqrt(i));
v.x += r * (cos(ang) + (D(1) * a.x));
v.y += r * (sin(ang) + (D(1) * a.y));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "voron",
        params: &[ParamDef::new("k", 0.99), ParamDef::new("step", 0.25), ParamDef::int("num", 1.0, 1.0, 25.0), ParamDef::int("xseed", 3.0, 1.0, 100.0), ParamDef::int("yseed", 7.0, 1.0, 100.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (k, step, num, xseed, yseed) = (d[0], d[1], d[2] as i32, d[3] as i32, d[4] as i32);
            let mut rmin = 20.0;
            let (mut x0, mut y0) = (0.0, 0.0);
            let m = (a.x / step).floor() as i32;
            let n = (a.y / step).floor() as i32;
            for i in -1..2 {
                let m1 = m.wrapping_add(i);
                for j in -1..2 {
                    let n1 = n.wrapping_add(j);
                    let kk = (1.0 + (num as f64 * truchet_ae_dn1(19i32.wrapping_mul(m1).wrapping_add(257i32.wrapping_mul(n1)).wrapping_add(xseed))).floor()) as i32;
                    for l in 0..kk {
                        let x = (truchet_ae_dn1(l.wrapping_add(64i32.wrapping_mul(m1)).wrapping_add(15i32.wrapping_mul(n1)).wrapping_add(xseed)) + m1 as f64) * step;
                        let y = (truchet_ae_dn1(l.wrapping_add(21i32.wrapping_mul(m1)).wrapping_add(33i32.wrapping_mul(n1)).wrapping_add(yseed)) + n1 as f64) * step;
                        let (ox, oy) = (a.x - x, a.y - y);
                        let r = (ox * ox + oy * oy).sqrt();
                        if r < rmin {
                            rmin = r;
                            x0 = x;
                            y0 = y;
                        }
                    }
                }
            }
            v.x += amount * (k * (a.x - x0) + x0);
            v.y += amount * (k * (a.y - y0) + y0);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], limit_value(ftoi(p[2]), 1, 25) as f64, ftoi(p[3]).max(1) as f64, ftoi(p[4]).max(1) as f64]),
        wgsl: "fn voron_noise(x: i32) -> f32 {
  let n = (x << 13u) ^ x;
  let h = n * (n * n * 15731 + 789221) + 1376312589;
  return f32(h & 0x7fffffff) * (1.0 / 2147483647.0);
}
@body
let step = D(1);
let num = f32(i32(D(2)));
let xseed = i32(D(3));
let yseed = i32(D(4));
var rmin = 20.0;
var x0 = 0.0;
var y0 = 0.0;
let m = i32(floor(a.x / step));
let n = i32(floor(a.y / step));
for (var i = -1; i < 2; i++) {
  let m1 = m + i;
  for (var j = -1; j < 2; j++) {
    let n1 = n + j;
    let kk = i32(1.0 + floor(num * voron_noise(19 * m1 + 257 * n1 + xseed)));
    for (var l = 0; l < kk; l++) {
      let x = (voron_noise(l + 64 * m1 + 15 * n1 + xseed) + f32(m1)) * step;
      let y = (voron_noise(l + 21 * m1 + 33 * n1 + yseed) + f32(n1)) * step;
      let r = length(vec2<f32>(a.x - x, a.y - y));
      if (r < rmin) {
        rmin = r;
        x0 = x;
        y0 = y;
      }
    }
  }
}
v.x += AMOUNT * (D(0) * (a.x - x0) + x0);
v.y += AMOUNT * (D(0) * (a.y - y0) + y0);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "voronoi_fold",
        params: &[ParamDef::new("scale", 1.0), ParamDef::new("fold", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let scale = java_max(d[0].abs(), 0.01);
            let cx = (a.x / scale).floor();
            let cy = (a.y / scale).floor();
            let (mut nx, mut ny) = (0.0, 0.0);
            let mut min_d = 1e10;
            for di in -1..=1 {
                for dj in -1..=1 {
                    let cell_x = cx + di as f64;
                    let cell_y = cy + dj as f64;
                    let mut hx = (cell_x * 127.1 + cell_y * 311.7).sin() * 43758.5453;
                    hx -= hx.floor();
                    let mut hy = (cell_x * 269.5 + cell_y * 183.3).sin() * 43758.5453;
                    hy -= hy.floor();
                    let sx = (cell_x + hx) * scale;
                    let sy = (cell_y + hy) * scale;
                    let (dx, dy) = (a.x - sx, a.y - sy);
                    let dd = (dx * dx + dy * dy).sqrt();
                    if dd < min_d {
                        min_d = dd;
                        nx = sx;
                        ny = sy;
                    }
                }
            }
            v.x += amount * (nx + (a.x - nx) * (1.0 - d[1]));
            v.y += amount * (ny + (a.y - ny) * (1.0 - d[1]));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let scale = max(abs(D(0)), 0.01);
let cx = floor(a.x / scale);
let cy = floor(a.y / scale);
var nx = 0.0;
var ny = 0.0;
var min_d = 1e10;
for (var di = -1; di <= 1; di++) {
  for (var dj = -1; dj <= 1; dj++) {
    let cell_x = cx + f32(di);
    let cell_y = cy + f32(dj);
    let hx = fract(sin(cell_x * 127.1 + cell_y * 311.7) * 43758.5453);
    let hy = fract(sin(cell_x * 269.5 + cell_y * 183.3) * 43758.5453);
    let sx = (cell_x + hx) * scale;
    let sy = (cell_y + hy) * scale;
    let dd = length(vec2<f32>(a.x - sx, a.y - sy));
    if (dd < min_d) {
      min_d = dd;
      nx = sx;
      ny = sy;
    }
  }
}
v.x += AMOUNT * (nx + (a.x - nx) * (1.0 - D(1)));
v.y += AMOUNT * (ny + (a.y - ny) * (1.0 - D(1)));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "vortex",
        params: &[
            ParamDef::int("randomize", 0.0, 0.0, 1000.0),
            ParamDef::new("time", 0.0),
            ParamDef::int("flowVel", 20.0, 1.0, 50.0),
            ParamDef::range("zoom", 4.0, 0.0, 9.0),
            ParamDef::new("UpDown", 0.0),
            ParamDef::new("vSep", 0.6),
            ParamDef::new("v_viDistance", 4.0),
            ParamDef::new("vDiameter", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [seed (unused), time, strength, zoom, updown, p1, p2, p3]
        apply: |d, ctx, a, v, amount| {
            let (time, strength, zoom, updown, p1, p2, p3) = (d[1], d[2] as i32, d[3], d[4], d[5], d[6], d[7]);
            let flow = |qx: f64, qy: f64| {
                let (mut cx, mut cy) = (time - 10.0 * (time / 10.0).floor() - 20.0, p1);
                let (mut vx, mut vy, mut dir) = (0.0, 0.0, 1.0);
                let (sx, sy) = (qx * (10.0 - zoom), qy * (10.0 - zoom));
                for _ in 0..30 {
                    let (dx, dy) = (sx - cx, sy - cy);
                    let den = dx * dx + dy * dy + 0.05;
                    vx += dy / den * p3 * dir;
                    vy += -dx / den * p3 * dir;
                    cx += p2;
                    cy = -cy;
                    dir = -dir;
                }
                (vx, vy)
            };
            let (mut px, mut py) = (a.x + 0.0, a.y + updown);
            for _ in 0..strength {
                let (fx, fy) = flow(px, py);
                px -= fx * 0.03;
                py -= fy * 0.03;
            }
            v.x += amount * px;
            v.y += amount * py;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = p[0] as i32 as f64;
            d[2] = limit_value(p[2], 1.0, 50.0) as i32 as f64;
            d[3] = limit_value(p[3], 0.0, 9.0);
            d
        }),
        wgsl: "fn vortex_flow(q: vec2<f32>, time: f32, zoom: f32, p1: f32, p2: f32, p3: f32) -> vec2<f32> {
  var c = vec2<f32>(time - 10.0 * floor(time / 10.0) - 20.0, p1);
  var vr = vec2<f32>(0.0);
  var dir = 1.0;
  let s = q * (10.0 - zoom);
  for (var k = 0; k < 30; k++) {
    let dd = s - c;
    vr += vec2<f32>(dd.y, -dd.x) / (dot(dd, dd) + 0.05) * p3 * dir;
    c = vec2<f32>(c.x + p2, -c.y);
    dir = -dir;
  }
  return vr;
}
@body
var p = vec2<f32>(a.x, a.y + D(4));
let strength = i32(D(2));
for (var i = 0; i < strength; i++) {
  p -= vortex_flow(p, D(1), D(3), D(5), D(6), D(7)) * 0.03;
}
v.x += AMOUNT * p.x;
v.y += AMOUNT * p.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "w",
        // JWildfire initializes angle and hypergon randomly.
        params: &[
            ParamDef::new("angle", 0.5),
            ParamDef::new("hypergon", 0.5),
            ParamDef::int("hypergon_n", 4.0, 3.0, 20.0),
            ParamDef::new("hypergon_r", 1.0),
            ParamDef::new("star", 0.0),
            ParamDef::int("star_n", 5.0, 3.0, 20.0),
            ParamDef::new("star_slope", 2.0),
            ParamDef::new("lituus", 0.0),
            ParamDef::new("lituus_a", 1.0),
            ParamDef::new("super", 0.0),
            ParamDef::new("super_m", 1.0),
            ParamDef::new("super_n1", 1.0),
            ParamDef::new("super_n2", 1.0),
            ParamDef::new("super_n3", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [14 params, hypergon_d(14), hypergon_n, lituus_a, star_n, star_slope, super_m, super_n1]
        apply: |d, ctx, a, v, amount| {
            let ang = a.y.atan2(a.x);
            let r = (sqr(a.x) + sqr(a.y)).sqrt();
            let mut a2 = ang + d[0];
            if a2 < -M_PI {
                a2 += M_2PI;
            }
            if a2 > M_PI {
                a2 -= M_2PI;
            }
            let total = wx_total(&d[1..], ang);
            if r <= total {
                let total2 = wx_total(&d[1..], a2);
                let r = amount * total2 * r / total;
                v.x += r * a2.cos();
                v.y += r * a2.sin();
            } else {
                v.x += amount * a.x;
                v.y += amount * a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[2] = limit_value(ftoi(p[2]), 3, i32::MAX) as f64;
            d[5] = limit_value(ftoi(p[5]), 3, i32::MAX) as f64;
            d.extend([(1.0 + sqr(p[3])).sqrt(), d[2], -p[8], d[5], p[6].tan(), p[10] / 4.0, -1.0 / (p[11] + SMALL_EPSILON)]);
            d
        }),
        wgsl: concat!(wx_total_wgsl!("w_total"), "@body
let ang = atan2(a.y, a.x);
let r = sqrt(sqr(a.x) + sqr(a.y));
var a2 = ang + D(0);
if (a2 < -PI) { a2 += TWO_PI; }
if (a2 > PI) { a2 -= TWO_PI; }
let total = w_total(base + 1u, ang);
if (r <= total) {
  let rr = AMOUNT * w_total(base + 1u, a2) * r / total;
  v.x += rr * cos(a2);
  v.y += rr * sin(a2);
} else {
  v.x += AMOUNT * a.x;
  v.y += AMOUNT * a.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"),
    },
    VarDef {
        name: "waffle",
        params: &[ParamDef::int("slices", 6.0, 1.0, 50.0), ParamDef::new("xthickness", 0.5), ParamDef::new("ythickness", 0.5), ParamDef::new("rotation", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let slices = d[0] as i32;
            let sl = slices as f64;
            let n = slices.max(0) as usize;
            let (xt, yt) = (d[1], d[2]);
            let (mut wa, mut wr) = (0.0, 0.0);
            match ctx.random_int(5) {
                0 => {
                    wa = (ctx.random_int(n) as f64 + ctx.random() * xt) / sl;
                    wr = (ctx.random_int(n) as f64 + ctx.random() * yt) / sl;
                }
                1 => {
                    wa = (ctx.random_int(n) as f64 + ctx.random()) / sl;
                    wr = (ctx.random_int(n) as f64 + yt) / sl;
                }
                2 => {
                    wa = (ctx.random_int(n) as f64 + xt) / sl;
                    wr = (ctx.random_int(n) as f64 + ctx.random()) / sl;
                }
                3 => {
                    wa = ctx.random();
                    wr = (ctx.random_int(n) as f64 + yt + ctx.random() * (1.0 - yt)) / sl;
                }
                4 => {
                    wa = (ctx.random_int(n) as f64 + xt + ctx.random() * (1.0 - xt)) / sl;
                    wr = ctx.random();
                }
                _ => {}
            }
            let vcosr = amount * d[3].cos();
            let vsinr = amount * d[3].sin();
            v.x += vcosr * wa + vsinr * wr;
            v.y += -vsinr * wa + vcosr * wr;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![ftoi(p[0]) as f64, p[1], p[2], p[3]]),
        wgsl: "let sl = D(0);
let xt = D(1);
let yt = D(2);
var wa = 0.0;
var wr = 0.0;
switch (min(i32(rnd() * 5.0), 4)) {
  case 0: {
    wa = (floor(rnd() * sl) + rnd() * xt) / sl;
    wr = (floor(rnd() * sl) + rnd() * yt) / sl;
  }
  case 1: {
    wa = (floor(rnd() * sl) + rnd()) / sl;
    wr = (floor(rnd() * sl) + yt) / sl;
  }
  case 2: {
    wa = (floor(rnd() * sl) + xt) / sl;
    wr = (floor(rnd() * sl) + rnd()) / sl;
  }
  case 3: {
    wa = rnd();
    wr = (floor(rnd() * sl) + yt + rnd() * (1.0 - yt)) / sl;
  }
  default: {
    wa = (floor(rnd() * sl) + xt + rnd() * (1.0 - xt)) / sl;
    wr = rnd();
  }
}
let vcosr = AMOUNT * cos(D(3));
let vsinr = AMOUNT * sin(D(3));
v.x += vcosr * wa + vsinr * wr;
v.y += -vsinr * wa + vcosr * wr;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "wallpaper_js",
        params: &[ParamDef::new("a", 1.156), ParamDef::new("b", -0.28), ParamDef::new("c", 21.288)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            if ctx.random() < 0.5 {
                v.x += a.y - ((a.x as i32).signum() as f64) * (d[1] * a.x - d[2]).abs().sqrt();
                v.y += d[0] - a.x;
            } else {
                v.x = a.x;
                v.y = a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "if (rnd() < 0.5) {
  v.x += a.y - sign(trunc(a.x)) * sqrt(abs(D(1) * a.x - D(2)));
  v.y += D(0) - a.x;
} else {
  v.x = a.x;
  v.y = a.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waveblur_wf",
        params: &[
            ParamDef::int("count", 5.0, 1.0, 50.0),
            ParamDef::new("phase", 0.0),
            ParamDef::new("amplitude_z", 0.5),
            ParamDef::range("damping_z", 0.0, 0.0, 2.0),
            ParamDef::int("direct_color", 0.0, 0.0, 1.0),
            ParamDef::new("color_scale", 0.5),
            ParamDef::new("color_offset", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            let count = d[0] as i32;
            let ang = ctx.random() * M_2PI;
            let rnd = 1.0 - 2.0 * ctx.random();
            let mut r = if ctx.random() < 0.5 {
                rnd.acos() - (ctx.random_int(count as usize) + 1) as f64 * M_PI + d[1]
            } else {
                (-rnd).acos() - (ctx.random_int(count as usize) + 1) as f64 * M_PI + d[1]
            };
            r *= amount / (count as f64 * M_PI);
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            if d[4] as i32 == 1 {
                v.color = limit_value(rnd.acos() * d[5] + d[6], 0.0, 1.0);
            }
            if d[2] != 0.0 {
                let mut zamp = (rnd.cos() - 0.5) * d[2];
                if d[3].abs() > SMALL_EPSILON {
                    zamp *= (r * d[3]).exp();
                }
                v.z += amount * zamp;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = ftoi(p[0]).max(1) as f64;
            d[3] = if p[3] < 0.0 { 0.0 } else { p[3] };
            d[4] = limit_value(ftoi(p[4]), 0, 1) as f64;
            d
        }),
        wgsl: "let count = D(0);
let ang = rnd() * TWO_PI;
let rv = 1.0 - 2.0 * rnd();
var r = 0.0;
if (rnd() < 0.5) {
  r = acos(rv) - (floor(rnd() * count) + 1.0) * PI + D(1);
} else {
  r = acos(-rv) - (floor(rnd() * count) + 1.0) * PI + D(1);
}
r *= AMOUNT / (count * PI);
v.x += r * cos(ang);
v.y += r * sin(ang);
if (i32(D(4)) == 1) { v.color = clamp(acos(rv) * D(5) + D(6), 0.0, 1.0); }
if (D(2) != 0.0) {
  var zamp = (cos(rv) - 0.5) * D(2);
  if (abs(D(3)) > EPS) { zamp *= exp(r * D(3)); }
  v.z += AMOUNT * zamp;
}",
    },
    VarDef {
        name: "wave_func",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("phase", 1.0), ParamDef::new("decay", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let ang = a.y.atan2(a.x);
            let psi = (d[0] * r).cos() * (-d[2] * r).exp() * (d[1] * ang).cos();
            let nr = r + psi;
            v.x += amount * nr * ang.cos();
            v.y += amount * nr * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let ang = atan2(a.y, a.x);
let nr = r + cos(D(0) * r) * exp(-D(2) * r) * cos(D(1) * ang);
v.x += AMOUNT * nr * cos(ang);
v.y += AMOUNT * nr * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves22",
        params: &[
            ParamDef::new("scalex", 0.05),
            ParamDef::new("scaley", 0.05),
            ParamDef::new("freqx", 7.0),
            ParamDef::new("freqy", 13.0),
            ParamDef::int("modex", 0.0, 0.0, 1.0),
            ParamDef::int("modey", 0.0, 0.0, 1.0),
            ParamDef::new("powerx", 2.0),
            ParamDef::new("powery", 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x0, y0) = (a.x, a.y);
            let px = d[6] as i32 as f64;
            let py = d[7] as i32 as f64;
            let sinx = if d[4] < 0.5 { (y0 * d[2]).sin() } else { 0.5 * (1.0 + (y0 * d[2]).sin()) };
            let offsetx = sinx.powf(px) * d[0];
            let siny = if d[5] < 0.5 { (x0 * d[3]).sin() } else { 0.5 * (1.0 + (x0 * d[3]).sin()) };
            let offsety = siny.powf(py) * d[1];
            v.x += amount * (x0 + offsetx);
            v.y += amount * (y0 + offsety);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = limit_value(p[4], 0.0, 1.0) as i32 as f64;
            d[5] = limit_value(p[5], 0.0, 1.0) as i32 as f64;
            d
        }),
        wgsl: "var sinx = sin(a.y * D(2));
if (D(4) >= 0.5) { sinx = 0.5 * (1.0 + sinx); }
var siny = sin(a.x * D(3));
if (D(5) >= 0.5) { siny = 0.5 * (1.0 + siny); }
v.x += AMOUNT * (a.x + java_pow(sinx, trunc(D(6))) * D(0));
v.y += AMOUNT * (a.y + java_pow(siny, trunc(D(7))) * D(1));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves23",
        params: &[ParamDef::new("scalex", 0.05), ParamDef::new("scaley", 0.05), ParamDef::new("freqx", 7.0), ParamDef::new("freqy", 13.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let mx = a.y * d[2] * M_1_2PI;
            let mut fx = mx - mx.floor();
            if fx > 0.5 {
                fx = 0.5 - fx;
            }
            let my = a.x * d[3] * M_1_2PI;
            let mut fy = my - my.floor();
            if fy > 0.5 {
                fy = 0.5 - fy;
            }
            v.x += amount * (a.x + fx * d[0]);
            v.y += amount * (a.y + fy * d[1]);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "var fx = fract(a.y * D(2) / TWO_PI);
if (fx > 0.5) { fx = 0.5 - fx; }
var fy = fract(a.x * D(3) / TWO_PI);
if (fy > 0.5) { fy = 0.5 - fy; }
v.x += AMOUNT * (a.x + fx * D(0));
v.y += AMOUNT * (a.y + fy * D(1));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves2_3D",
        params: &[ParamDef::new("freq", 2.0), ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            let avgxy = (a.x + a.y) / 2.0;
            v.x += amount * (a.x + d[1] * (a.y * d[0]).sin());
            v.y += amount * (a.y + d[1] * (a.x * d[0]).sin());
            v.z += amount * (a.z + d[1] * (avgxy * d[0]).sin());
        },
        prepare: None,
        wgsl: "let avgxy = (a.x + a.y) / 2.0;
v.x += AMOUNT * (a.x + D(1) * sin(a.y * D(0)));
v.y += AMOUNT * (a.y + D(1) * sin(a.x * D(0)));
v.z += AMOUNT * (a.z + D(1) * sin(avgxy * D(0)));",
    },
    VarDef {
        name: "waves2b",
        params: &[
            ParamDef::new("freqx", 1.5),
            ParamDef::new("freqy", 2.5),
            ParamDef::range("pwx", 1.0, -10.0, 10.0),
            ParamDef::range("pwy", 1.5, -10.0, 10.0),
            ParamDef::new("scalex", 2.0),
            ParamDef::new("scaleinfx", 1.0),
            ParamDef::new("scaley", 0.75),
            ParamDef::new("scaleinfy", 1.5),
            ParamDef::new("unity", 1.0),
            ParamDef::range("jacok", 0.25, -1.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [10 params, six, siy]
        apply: |d, ctx, a, v, amount| {
            let (freqx, freqy, pwx, pwy, scaleinfx, scaleinfy, unity, jacok) = (d[0], d[1], d[2], d[3], d[5], d[7], d[8], d[9]);
            let safediv = |q: f64, r: f64| if r < 1e-10 { 1.0 / r } else { q / r };
            let sgn = |x: f64| if x < 0.0 { -1.0 } else { 1.0 };
            let csx = safediv(unity, unity + sqr(a.x)) * d[10] + scaleinfx;
            let csy = safediv(unity, unity + sqr(a.y)) * d[11] + scaleinfy;
            if (0.0..1e-4).contains(&pwx) {
                v.x += amount * (a.x + csx * waves2b_jacobi_sn(a.y * freqx, jacok));
            } else if pwx < 0.0 && pwx > -1e-4 {
                v.x += amount * (a.x + csx * waves2b_bessel_j1(a.y * freqx));
            } else {
                v.x += amount * (a.x + csx * (sgn(a.y) * (a.y.abs() + 1e-10).powf(pwx) * freqx).sin());
            }
            if (0.0..1e-4).contains(&pwy) {
                v.y += amount * (a.y + csy * waves2b_jacobi_sn(a.x * freqy, jacok));
            } else if pwy < 0.0 && pwy > -1e-4 {
                v.y += amount * (a.y + csy * waves2b_bessel_j1(a.x * freqy));
            } else {
                v.y += amount * (a.y + csy * (sgn(a.x) * (a.x.abs() + 1e-10).powf(pwy) * freqy).sin());
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[2] = limit_value(p[2], -10.0, 10.0);
            d[3] = limit_value(p[3], -10.0, 10.0);
            d[9] = limit_value(p[9], -1.0, 1.0);
            d.extend([p[4] - p[5], p[6] - p[7]]);
            d
        }),
        wgsl: "fn waves2b_safediv(q: f32, r: f32) -> f32 {
  if (r < 1e-10) { return 1.0 / r; }
  return q / r;
}
fn waves2b_sgn(x: f32) -> f32 { return select(1.0, -1.0, x < 0.0); }
fn waves2b_bessel_j1(x: f32) -> f32 {
  let ax = abs(x);
  if (ax < 8.0) {
    let y = x * x;
    let ans1 = x * (72362614232.0 + y * (-7895059235.0 + y * (242396853.1 + y * (-2972611.439 + y * (15704.48260 + y * (-30.16036606))))));
    let ans2 = 144725228442.0 + y * (2300535178.0 + y * (18583304.74 + y * (99447.43394 + y * (376.9991397 + y * 1.0))));
    return ans1 / ans2;
  }
  let z = 8.0 / ax;
  let xx = ax - 2.356194491;
  let y = z * z;
  let ans1 = 1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * (-0.240337019e-6))));
  let ans2 = 0.04687499995 + y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
  let ans = sqrt(0.636619772 / ax) * (cos(xx) * ans1 - z * sin(xx) * ans2);
  return select(ans, -ans, x < 0.0);
}
fn waves2b_jacobi_sn(uu: f32, emmc: f32) -> f32 {
  if (emmc == 0.0) { return tanh(uu); }
  var em: array<f32, 13>;
  var en: array<f32, 13>;
  var emc = emmc;
  var u = uu;
  var d = 0.0;
  let bo = emc < 0.0;
  if (bo) {
    d = 1.0 - emc;
    emc = -emc / d;
    d = sqrt(d);
    u = d * u;
  }
  var a = 1.0;
  var dn = 1.0;
  var c = 0.0;
  var l = 0;
  for (var i = 0; i < 8; i++) {
    l = i;
    em[i] = a;
    emc = sqrt(emc);
    en[i] = emc;
    c = 0.5 * (a + emc);
    if (abs(a - emc) <= 0.0003 * a) { break; }
    emc = a * emc;
    a = c;
  }
  u = c * u;
  var sn = sin(u);
  var cn = cos(u);
  if (sn != 0.0) {
    a = cn / sn;
    c = a * c;
    for (var ii = l; ii >= 0; ii--) {
      let b = em[ii];
      a = c * a;
      c = dn * c;
      dn = (en[ii] + a) / (b + a);
      a = c / b;
    }
    a = 1.0 / sqrt(c * c + 1.0);
    sn = select(a, -a, sn < 0.0);
    cn = c * sn;
  }
  if (bo) { sn = sn / d; }
  return sn;
}
@body
let csx = waves2b_safediv(D(8), D(8) + sqr(a.x)) * D(10) + D(5);
let csy = waves2b_safediv(D(8), D(8) + sqr(a.y)) * D(11) + D(7);
let pwx = D(2);
let pwy = D(3);
if (pwx >= 0.0 && pwx < 1e-4) {
  v.x += AMOUNT * (a.x + csx * waves2b_jacobi_sn(a.y * D(0), D(9)));
} else if (pwx < 0.0 && pwx > -1e-4) {
  v.x += AMOUNT * (a.x + csx * waves2b_bessel_j1(a.y * D(0)));
} else {
  v.x += AMOUNT * (a.x + csx * sin(waves2b_sgn(a.y) * pow(abs(a.y) + 1e-10, pwx) * D(0)));
}
if (pwy >= 0.0 && pwy < 1e-4) {
  v.y += AMOUNT * (a.y + csy * waves2b_jacobi_sn(a.x * D(1), D(9)));
} else if (pwy < 0.0 && pwy > -1e-4) {
  v.y += AMOUNT * (a.y + csy * waves2b_bessel_j1(a.x * D(1)));
} else {
  v.y += AMOUNT * (a.y + csy * sin(waves2b_sgn(a.x) * pow(abs(a.x) + 1e-10, pwy) * D(1)));
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves2_radial",
        params: &[
            ParamDef::new("w2r_scalex", 0.1),
            ParamDef::new("w2r_scaley", 0.1),
            ParamDef::new("w2r_freqx", 7.0),
            ParamDef::new("w2r_freqy", 13.0),
            ParamDef::new("w2r_null", 2.0),
            ParamDef::new("w2r_distance", 10.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x0, y0) = (a.x, a.y);
            let dist = (sqr(x0) + sqr(y0)).sqrt();
            let mut factor = if dist < d[5] { (dist - d[4]) / (d[5] - d[4]) } else { 1.0 };
            if dist < d[4] {
                factor = 0.0;
            }
            v.x += amount * (x0 + factor * (y0 * d[2]).sin() * d[0]);
            v.y += amount * (y0 + factor * (x0 * d[3]).sin() * d[1]);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dist = sqrt(sqr(a.x) + sqr(a.y));
var factor = 1.0;
if (dist < D(5)) { factor = (dist - D(4)) / (D(5) - D(4)); }
if (dist < D(4)) { factor = 0.0; }
v.x += AMOUNT * (a.x + factor * sin(a.y * D(2)) * D(0));
v.y += AMOUNT * (a.y + factor * sin(a.x * D(3)) * D(1));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves3",
        params: &[
            ParamDef::new("scalex", 0.05),
            ParamDef::new("scaley", 0.05),
            ParamDef::new("freqx", 7.0),
            ParamDef::new("freqy", 13.0),
            ParamDef::new("sx_freq", 0.0),
            ParamDef::new("sy_freq", 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x0, y0) = (a.x, a.y);
            let sxx = 0.5 * d[0] * (1.0 + (y0 * d[4]).sin());
            let syy = 0.5 * d[1] * (1.0 + (x0 * d[5]).sin());
            v.x += amount * (x0 + (y0 * d[2]).sin() * sxx);
            v.y += amount * (y0 + (x0 * d[3]).sin() * syy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sxx = 0.5 * D(0) * (1.0 + sin(a.y * D(4)));
let syy = 0.5 * D(1) * (1.0 + sin(a.x * D(5)));
v.x += AMOUNT * (a.x + sin(a.y * D(2)) * sxx);
v.y += AMOUNT * (a.y + sin(a.x * D(3)) * syy);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves3d_decay",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("decay", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let nr = r + (r * d[0]).sin() * (-r * d[1]).exp();
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
let nr = r + sin(r * D(0)) * exp(-r * D(1));
v.x += AMOUNT * nr * cos(theta);
v.y += AMOUNT * nr * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves3_wf",
        params: &[
            ParamDef::new("scalex", 0.25),
            ParamDef::new("scaley", 0.5),
            ParamDef::new("freqx", M_PI / 2.0),
            ParamDef::new("freqy", M_PI / 4.0),
            ParamDef::int("use_cos_x", 1.0, 0.0, 1.0),
            ParamDef::int("use_cos_y", 0.0, 0.0, 1.0),
            ParamDef::new("dampx", 0.0),
            ParamDef::new("dampy", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [8 params, damping_x, damping_y]
        apply: |d, ctx, a, v, amount| {
            let (dx, dy) = (d[8], d[9]);
            let tx = if d[4] as i32 == 1 { (a.y * d[2]).cos() } else { (a.y * d[2]).sin() };
            v.x += amount * (a.x + dx * d[0] * tx * tx) * dx;
            let ty = if d[5] as i32 == 1 { (a.x * d[3]).cos() } else { (a.x * d[3]).sin() };
            v.y += amount * (a.y + dy * d[1] * ty * ty) * dy;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = ftoi(p[4]) as f64;
            d[5] = ftoi(p[5]) as f64;
            d.extend([if p[6].abs() < EPSILON { 1.0 } else { p[6].exp() }, if p[7].abs() < EPSILON { 1.0 } else { p[7].exp() }]);
            d
        }),
        wgsl: "var tx = sin(a.y * D(2));
if (i32(D(4)) == 1) { tx = cos(a.y * D(2)); }
v.x += AMOUNT * (a.x + D(8) * D(0) * tx * tx) * D(8);
var ty = sin(a.x * D(3));
if (i32(D(5)) == 1) { ty = cos(a.x * D(3)); }
v.y += AMOUNT * (a.y + D(9) * D(1) * ty * ty) * D(9);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves4",
        params: &[
            ParamDef::new("scalex", 0.05),
            ParamDef::new("scaley", 0.05),
            ParamDef::new("freqx", 7.0),
            ParamDef::new("freqy", 13.0),
            ParamDef::int("cont", 0.0, 0.0, 1.0),
            ParamDef::new("yfact", 0.1),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| waves4_apply(d, ctx, a, v, amount, (a.y * d[2] / M_2PI).floor()),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = limit_value(p[4], 0.0, 1.0) as i32 as f64;
            d
        }),
        wgsl: "let ax0 = floor(a.y * D(2) / TWO_PI);
var ax = sin(ax0 * 12.9898 + ax0 * 78.233 + 1.0 + a.y * 0.001 * D(5)) * 43758.5453;
ax = ax - trunc(ax);
if (i32(D(4)) == 1) { ax = select(0.0, 1.0, ax > 0.5); }
v.x += AMOUNT * (a.x + sin(a.y * D(2)) * ax * ax * D(0));
v.y += AMOUNT * (a.y + sin(a.x * D(3)) * D(1));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves42",
        params: &[
            ParamDef::new("scalex", 0.05),
            ParamDef::new("scaley", 0.05),
            ParamDef::new("freqx", 7.0),
            ParamDef::new("freqy", 13.0),
            ParamDef::int("cont", 0.0, 0.0, 1.0),
            ParamDef::new("yfact", 0.1),
            ParamDef::new("freqx2", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| waves4_apply(d, ctx, a, v, amount, (a.y * d[6]).floor()),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = limit_value(p[4], 0.0, 1.0) as i32 as f64;
            d
        }),
        wgsl: "let ax0 = floor(a.y * D(6));
var ax = sin(ax0 * 12.9898 + ax0 * 78.233 + 1.0 + a.y * 0.001 * D(5)) * 43758.5453;
ax = ax - trunc(ax);
if (i32(D(4)) == 1) { ax = select(0.0, 1.0, ax > 0.5); }
v.x += AMOUNT * (a.x + sin(a.y * D(2)) * ax * ax * D(0));
v.y += AMOUNT * (a.y + sin(a.x * D(3)) * D(1));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves4_wf",
        params: &[
            ParamDef::new("scalex", 0.25),
            ParamDef::new("scaley", 0.5),
            ParamDef::new("freqx", M_PI / 2.0),
            ParamDef::new("freqy", M_PI / 4.0),
            ParamDef::int("use_cos_x", 1.0, 0.0, 1.0),
            ParamDef::int("use_cos_y", 0.0, 0.0, 1.0),
            ParamDef::new("dampx", 0.0),
            ParamDef::new("dampy", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [8 params, damping_x, damping_y]
        apply: |d, ctx, a, v, amount| {
            let (dx, dy) = (d[8], d[9]);
            let (sx, cx) = ((a.y * d[2]).sin(), (a.y * d[2]).cos());
            if d[4] as i32 == 1 {
                v.x += amount * (a.x + dx * d[0] * cx * sx * cx) * dx;
            } else {
                v.x += amount * (a.x + dx * d[0] * sx * cx * sx) * dx;
            }
            let (sy, cy) = ((a.x * d[3]).sin(), (a.x * d[3]).cos());
            if d[5] as i32 == 1 {
                v.y += amount * (a.y + dy * d[1] * cy * sy * cy) * dy;
            } else {
                v.y += amount * (a.y + dy * d[1] * sy * cy * sy) * dy;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = ftoi(p[4]) as f64;
            d[5] = ftoi(p[5]) as f64;
            d.extend([if p[6].abs() < EPSILON { 1.0 } else { p[6].exp() }, if p[7].abs() < EPSILON { 1.0 } else { p[7].exp() }]);
            d
        }),
        wgsl: "let sx = sin(a.y * D(2));
let cx = cos(a.y * D(2));
var tx = sx * cx * sx;
if (i32(D(4)) == 1) { tx = cx * sx * cx; }
v.x += AMOUNT * (a.x + D(8) * D(0) * tx) * D(8);
let sy = sin(a.x * D(3));
let cy = cos(a.x * D(3));
var ty = sy * cy * sy;
if (i32(D(5)) == 1) { ty = cy * sy * cy; }
v.y += AMOUNT * (a.y + D(9) * D(1) * ty) * D(9);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves_phase",
        params: &[
            ParamDef::new("scale_x", 0.25),
            ParamDef::new("scale_y", 0.25),
            ParamDef::new("freq_x", 2.0),
            ParamDef::new("freq_y", 2.0),
            ParamDef::new("phase_x", 0.0),
            ParamDef::new("phase_y", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let safe = |s: f64| if s.abs() < 1e-4 { (if s + 1e-5 < 0.0 { -1.0 } else { 1.0 }) * 1e-4 } else { s * s };
            v.x += amount * (a.x + d[0] * (a.y * d[2] / safe(d[0]) + d[4]).sin());
            v.y += amount * (a.y + d[1] * (a.x * d[3] / safe(d[1]) + d[5]).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "var sx2 = D(0) * D(0);
if (abs(D(0)) < 1e-4) { sx2 = select(1.0, -1.0, D(0) + 1e-5 < 0.0) * 1e-4; }
var sy2 = D(1) * D(1);
if (abs(D(1)) < 1e-4) { sy2 = select(1.0, -1.0, D(1) + 1e-5 < 0.0) * 1e-4; }
v.x += AMOUNT * (a.x + D(0) * sin(a.y * D(2) / sx2 + D(4)));
v.y += AMOUNT * (a.y + D(1) * sin(a.x * D(3) / sy2 + D(5)));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "wdisc",
        false,
        |_, ctx, a, v, amount| {
            let mut ang = M_PI / ((sqr(a.x) + sqr(a.y)).sqrt() + 1.0);
            let r = a.y.atan2(a.x) * M_1_PI;
            if r > 0.0 {
                ang = M_PI - ang;
            }
            v.x += amount * r * ang.cos();
            v.y += amount * r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        "var ang = PI / (sqrt(sqr(a.x) + sqr(a.y)) + 1.0);
let r = atan2(a.y, a.x) / PI;
if (r > 0.0) { ang = PI - ang; }
v.x += AMOUNT * r * cos(ang);
v.y += AMOUNT * r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "weave",
        params: &[ParamDef::new("scale", 0.5), ParamDef::new("warp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let scale = java_max(d[0].abs(), 0.01);
            let col = (a.x / scale).floor();
            let row = (a.y / scale).floor();
            let lx = a.x / scale - col - 0.5;
            let ly = a.y / scale - row - 0.5;
            let mut m = (col + row) % 2.0;
            if m < 0.0 {
                m += 2.0;
            }
            let over = 2.0 * m - 1.0;
            let dx = d[1] * over * (M_PI * ly).sin();
            let dy = d[1] * (-over) * (M_PI * lx).sin();
            v.x += amount * (a.x + dx * scale);
            v.y += amount * (a.y + dy * scale);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let scale = max(abs(D(0)), 0.01);
let col = floor(a.x / scale);
let row = floor(a.y / scale);
let lx = a.x / scale - col - 0.5;
let ly = a.y / scale - row - 0.5;
var m = fmod(col + row, 2.0);
if (m < 0.0) { m += 2.0; }
let over = 2.0 * m - 1.0;
v.x += AMOUNT * (a.x + D(1) * over * sin(PI * ly) * scale);
v.y += AMOUNT * (a.y + D(1) * (-over) * sin(PI * lx) * scale);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "weld_seam",
        params: &[ParamDef::new("pitch", 0.5), ParamDef::new("height", 0.2), ParamDef::new("width", 0.2)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let bx = (a.x / d[0] + 0.5).floor() * d[0];
            let dx = a.x - bx;
            let dist2 = dx * dx + a.y * a.y;
            let push = d[1] * (-dist2 / (d[2] * d[2] + 0.001)).exp();
            let len = dist2.sqrt() + 0.001;
            v.x += amount * (a.x + push * dx / len);
            v.y += amount * (a.y + push * a.y / len);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dx = a.x - floor(a.x / D(0) + 0.5) * D(0);
let dist2 = dx * dx + a.y * a.y;
let push = D(1) * exp(-dist2 / (D(2) * D(2) + 0.001));
let len = sqrt(dist2) + 0.001;
v.x += AMOUNT * (a.x + push * dx / len);
v.y += AMOUNT * (a.y + push * a.y / len);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "whirligig",
        params: &[ParamDef::int("Mode", 0.0, 0.0, 15.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (csin, ccos) = (a.x.sin(), a.x.cos());
            let (csinh, ccosh) = (a.y.sinh(), a.y.cosh());
            ctx.pass_z(a, v, amount);
            let mode = d[0] as i32;
            let (tx, ty) = match mode / 4 {
                0 => (amount * ccos / ccosh, amount * csin / csinh),
                1 => (amount * csin / ccosh, amount * ccos / csinh),
                2 => (amount * csin / csinh, amount * ccos / ccosh),
                _ => (amount * ccos / csinh, amount * csin / ccosh),
            };
            let (sx, sy) = [(1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (-1.0, -1.0)][(mode % 4) as usize];
            v.x += sx * tx;
            v.y += sy * ty;
        },
        prepare: Some(|p, _| vec![limit_value(ftoi(p[0]), 0, 15) as f64]),
        wgsl: "let csin = sin(a.x);
let ccos = cos(a.x);
let csinh = sinh(a.y);
let ccosh = cosh(a.y);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
let mode = i32(D(0));
let g = mode / 4;
var t = vec2<f32>(ccos / ccosh, csin / csinh);
if (g == 1) { t = vec2<f32>(csin / ccosh, ccos / csinh); }
else if (g == 2) { t = vec2<f32>(csin / csinh, ccos / ccosh); }
else if (g == 3) { t = vec2<f32>(ccos / csinh, csin / ccosh); }
let s = mode % 4;
let sx = select(-1.0, 1.0, s == 0 || s == 2);
let sy = select(-1.0, 1.0, s == 1 || s == 2);
v.x += sx * AMOUNT * t.x;
v.y += sy * AMOUNT * t.y;",
    },
    simple(
        "whitney_umbrella",
        true,
        |_, _ctx, a, v, amount| {
            v.x += amount * a.x * a.y;
            v.y += amount * a.x;
            v.z += amount * a.y * a.y;
        },
        "v.x += AMOUNT * a.x * a.y;
v.y += AMOUNT * a.x;
v.z += AMOUNT * a.y * a.y;",
    ),
    VarDef {
        name: "woggle_js",
        params: &[ParamDef::int("m", 2.0, 2.0, 12.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [m, a[0..12], b[0..12]]
        apply: |d, ctx, a, v, amount| {
            let m = d[0] as i32;
            let r = 1.25f64.sqrt() * (m as f64).sqrt();
            let c = (m as f64 * ctx.random()) as usize;
            let ra = 1.0 / (3f64.sqrt() * (a.x * a.x + a.y * a.y).sqrt());
            let (x, y) = if c.is_multiple_of(2) {
                (-a.x / r + ra * a.y / r + d[1 + c], -ra * a.x / r - a.y / r + d[13 + c])
            } else {
                (a.x / r + ra * a.y / r + d[1 + c], -ra * a.x / r + a.y / r + d[13 + c])
            };
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let m = (p[0] as i32).clamp(2, 12);
            let mut d = vec![0.0; 25];
            d[0] = m as f64;
            for i in 0..m as usize {
                d[1 + i] = (2.0 * M_PI * i as f64 / m as f64).cos();
                d[13 + i] = (2.0 * M_PI * i as f64 / m as f64).sin();
            }
            d
        }),
        wgsl: "let m = D(0);
let r = sqrt(1.25) * sqrt(m);
let c = u32(min(floor(m * rnd()), m - 1.0));
let ra = 1.0 / (sqrt(3.0) * sqrt(a.x * a.x + a.y * a.y));
var x = a.x / r + ra * a.y / r + prm[base + 1u + c];
var y = -ra * a.x / r + a.y / r + prm[base + 13u + c];
if (c % 2u == 0u) {
  x = -a.x / r + ra * a.y / r + prm[base + 1u + c];
  y = -ra * a.x / r - a.y / r + prm[base + 13u + c];
}
v.x += x * AMOUNT;
v.y += y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "wood_grain",
        params: &[ParamDef::new("freq", 5.0), ParamDef::new("amp", 0.2), ParamDef::new("grain", 0.5), ParamDef::new("grain_freq", 6.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let disp = d[1] * (d[0] * r + d[2] * (d[3] * theta).cos()).sin();
            v.x += amount * (a.x + disp * theta.cos());
            v.y += amount * (a.y + disp * theta.sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
let disp = D(1) * sin(D(0) * r + D(2) * cos(D(3) * theta));
v.x += AMOUNT * (a.x + disp * cos(theta));
v.y += AMOUNT * (a.y + disp * sin(theta));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "worley",
        params: &[ParamDef::new("scale", 1.0), ParamDef::range("jitter", 0.8, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let s = java_max(d[0].abs(), 0.01);
            let j = java_max(0.0, java_min(1.0, d[1]));
            let cx = (a.x / s).floor();
            let cy = (a.y / s).floor();
            let mut min_d = 1e10;
            let (mut nx, mut ny) = (0.0, 0.0);
            for di in -1..=1 {
                for dj in -1..=1 {
                    let cell_x = cx + di as f64;
                    let cell_y = cy + dj as f64;
                    let mut hx = (cell_x * 127.1 + cell_y * 311.7).sin() * 43758.5453;
                    hx -= hx.floor();
                    let mut hy = (cell_x * 269.5 + cell_y * 183.3).sin() * 43758.5453;
                    hy -= hy.floor();
                    let sx = (cell_x + hx * j + (1.0 - j) * 0.5) * s;
                    let sy = (cell_y + hy * j + (1.0 - j) * 0.5) * s;
                    let (dx, dy) = (a.x - sx, a.y - sy);
                    let dd = (dx * dx + dy * dy).sqrt();
                    if dd < min_d {
                        min_d = dd;
                        nx = sx;
                        ny = sy;
                    }
                }
            }
            v.x += amount * ((a.x - nx) / (s + 1e-6));
            v.y += amount * ((a.y - ny) / (s + 1e-6));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let s = max(abs(D(0)), 0.01);
let j = max(0.0, min(1.0, D(1)));
let cx = floor(a.x / s);
let cy = floor(a.y / s);
var min_d = 1e10;
var nx = 0.0;
var ny = 0.0;
for (var di = -1; di <= 1; di++) {
  for (var dj = -1; dj <= 1; dj++) {
    let cell_x = cx + f32(di);
    let cell_y = cy + f32(dj);
    let hx = fract(sin(cell_x * 127.1 + cell_y * 311.7) * 43758.5453);
    let hy = fract(sin(cell_x * 269.5 + cell_y * 183.3) * 43758.5453);
    let sx = (cell_x + hx * j + (1.0 - j) * 0.5) * s;
    let sy = (cell_y + hy * j + (1.0 - j) * 0.5) * s;
    let dd = length(vec2<f32>(a.x - sx, a.y - sy));
    if (dd < min_d) {
      min_d = dd;
      nx = sx;
      ny = sy;
    }
  }
}
v.x += AMOUNT * ((a.x - nx) / (s + 1e-6));
v.y += AMOUNT * ((a.y - ny) / (s + 1e-6));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "x",
        // JWildfire initializes hypergon and star randomly.
        params: &[
            ParamDef::new("hypergon", 0.5),
            ParamDef::int("hypergon_n", 4.0, 3.0, 20.0),
            ParamDef::new("hypergon_r", 1.0),
            ParamDef::new("star", 0.5),
            ParamDef::int("star_n", 5.0, 3.0, 20.0),
            ParamDef::new("star_slope", 2.0),
            ParamDef::new("lituus", 0.0),
            ParamDef::new("lituus_a", 1.0),
            ParamDef::new("super", 0.0),
            ParamDef::new("super_m", 1.0),
            ParamDef::new("super_n1", 1.0),
            ParamDef::new("super_n2", 1.0),
            ParamDef::new("super_n3", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [13 params, hypergon_d(13), hypergon_n, lituus_a, star_n, star_slope, super_m, super_n1]
        apply: |d, ctx, a, v, amount| {
            let ang = a.y.atan2(a.x);
            let total = wx_total(d, ang);
            let r = amount * (sqr(a.x) + sqr(a.y) + sqr(total)).sqrt();
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[1] = limit_value(ftoi(p[1]), 3, i32::MAX) as f64;
            d[4] = limit_value(ftoi(p[4]), 3, i32::MAX) as f64;
            d.extend([(1.0 + sqr(p[2])).sqrt(), d[1], -p[7], d[4], p[5].tan(), p[9] / 4.0, -1.0 / (p[10] + SMALL_EPSILON)]);
            d
        }),
        wgsl: concat!(
            wx_total_wgsl!("x_total"),
            "@body
let ang = atan2(a.y, a.x);
let total = x_total(base, ang);
let r = AMOUNT * sqrt(sqr(a.x) + sqr(a.y) + sqr(total));
v.x += r * cos(ang);
v.y += r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        ),
    },
    simple(
        "xerf",
        true,
        |_, _ctx, a, v, amount| {
            let mut r2 = sqr(a.x) + sqr(a.y) + sqr(a.z);
            if r2 <= SMALL_EPSILON {
                r2 = SMALL_EPSILON;
            }
            let f = |t: f64| if t.abs() >= 2.0 { t / r2 } else { erf(t) };
            v.x += f(a.x) * amount;
            v.y += f(a.y) * amount;
            v.z += f(a.z) * amount;
        },
        "var r2 = sqr(a.x) + sqr(a.y) + sqr(a.z);
if (r2 <= EPS) { r2 = EPS; }
v.x += select(erf(a.x), a.x / r2, abs(a.x) >= 2.0) * AMOUNT;
v.y += select(erf(a.y), a.y / r2, abs(a.y) >= 2.0) * AMOUNT;
v.z += select(erf(a.z), a.z / r2, abs(a.z) >= 2.0) * AMOUNT;",
    ),
    VarDef {
        name: "xheart",
        params: &[ParamDef::new("angle", 0.0), ParamDef::new("ratio", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [angle, ratio, rat, cosa, sina]
        apply: |d, ctx, a, v, amount| {
            xheart_apply(d, a.x, a.y, v, amount);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| xheart_prepare(p)),
        wgsl: "var r2_4 = a.x * a.x + a.y * a.y + 4.0;
if (r2_4 == 0.0) { r2_4 = 1.0; }
let bx = 4.0 / r2_4;
let by = D(2) / r2_4;
let x = D(3) * (bx * a.x) - D(4) * (by * a.y);
let y = D(4) * (bx * a.x) + D(3) * (by * a.y);
v.x += AMOUNT * x;
v.y += select(-AMOUNT * y, AMOUNT * y, x > 0.0);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "xheart_blur_wf",
        params: &[ParamDef::new("angle", 0.0), ParamDef::new("ratio", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [angle, ratio, rat, cosa, sina]
        apply: |d, ctx, a, v, amount| {
            let dx = 2.0 - ctx.random() * 4.0;
            let dy = 2.0 - ctx.random() * 4.0;
            xheart_apply(d, dx, dy, v, amount);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| xheart_prepare(p)),
        wgsl: "let dx = 2.0 - rnd() * 4.0;
let dy = 2.0 - rnd() * 4.0;
var r2_4 = dx * dx + dy * dy + 4.0;
if (r2_4 == 0.0) { r2_4 = 1.0; }
let bx = 4.0 / r2_4;
let by = D(2) / r2_4;
let x = D(3) * (bx * dx) - D(4) * (by * dy);
let y = D(4) * (bx * dx) + D(3) * (by * dy);
v.x += AMOUNT * x;
v.y += select(-AMOUNT * y, AMOUNT * y, x > 0.0);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    // @@NEXT@@
];

fn xheart_prepare(p: &[f64]) -> Vec<f64> {
    let ang = M_PI_4 + (0.5 * M_PI_4 * p[0]);
    vec![p[0], p[1], 6.0 + 2.0 * p[1], ang.cos(), ang.sin()]
}

/// `XHeartFunc.transform` for the point (px, py).
fn xheart_apply(d: &[f64], px: f64, py: f64, v: &mut Pt, amount: f64) {
    let mut r2_4 = px * px + py * py + 4.0;
    if r2_4 == 0.0 {
        r2_4 = 1.0;
    }
    let (bx, by) = (4.0 / r2_4, d[2] / r2_4);
    let x = d[3] * (bx * px) - d[4] * (by * py);
    let y = d[4] * (bx * px) + d[3] * (by * py);
    v.x += amount * x;
    if x > 0.0 {
        v.y += amount * y;
    } else {
        v.y += -amount * y;
    }
}

/// Shared tail of `Waves4Func` / `Waves42Func`; `ax0` is the floored row index.
fn waves4_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64, ax0: f64) {
    let (x0, y0) = (a.x, a.y);
    let mut ax = (ax0 * 12.9898 + ax0 * 78.233 + 1.0 + y0 * 0.001 * d[5]).sin() * 43758.5453;
    ax -= (ax as i32) as f64;
    if d[4] as i32 == 1 {
        ax = if ax > 0.5 { 1.0 } else { 0.0 };
    }
    v.x += amount * (x0 + (y0 * d[2]).sin() * ax * ax * d[0]);
    v.y += amount * (y0 + (x0 * d[3]).sin() * d[1]);
    ctx.pass_z(a, v, amount);
}

/// `cern.jet.math.Bessel.j1` (Colt)
fn waves2b_bessel_j1(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let ans1 = x * (72362614232.0 + y * (-7895059235.0 + y * (242396853.1 + y * (-2972611.439 + y * (15704.48260 + y * (-30.16036606))))));
        let ans2 = 144725228442.0 + y * (2300535178.0 + y * (18583304.74 + y * (99447.43394 + y * (376.9991397 + y * 1.0))));
        ans1 / ans2
    } else {
        let z = 8.0 / ax;
        let xx = ax - 2.356194491;
        let y = z * z;
        let ans1 = 1.0 + y * (0.183105e-2 + y * (-0.3516396496e-4 + y * (0.2457520174e-5 + y * (-0.240337019e-6))));
        let ans2 = 0.04687499995 + y * (-0.2002690873e-3 + y * (0.8449199096e-5 + y * (-0.88228987e-6 + y * 0.105787412e-6)));
        let ans = (0.636619772 / ax).sqrt() * (xx.cos() * ans1 - z * xx.sin() * ans2);
        if x < 0.0 { -ans } else { ans }
    }
}

/// `Waves2BFunc.Jacobi_elliptic` (sn only)
fn waves2b_jacobi_sn(uu: f64, emmc: f64) -> f64 {
    if emmc == 0.0 {
        return uu.tanh();
    }
    let mut em = [0.0; 13];
    let mut en = [0.0; 13];
    let mut emc = emmc;
    let mut u = uu;
    let mut d = 0.0;
    let bo = emc < 0.0;
    if bo {
        d = 1.0 - emc;
        emc = -emc / d;
        d = d.sqrt();
        u *= d;
    }
    let mut a = 1.0;
    let mut dn = 1.0;
    let mut c = 0.0;
    let mut l = 0;
    for i in 0..8 {
        l = i;
        em[i] = a;
        emc = emc.sqrt();
        en[i] = emc;
        c = 0.5 * (a + emc);
        if (a - emc).abs() <= 0.0003 * a {
            break;
        }
        emc *= a;
        a = c;
    }
    u *= c;
    let mut sn = u.sin();
    let cn = u.cos();
    if sn != 0.0 {
        a = cn / sn;
        c *= a;
        for ii in (0..=l).rev() {
            let b = em[ii];
            a *= c;
            c *= dn;
            dn = (en[ii] + a) / (b + a);
            a = c / b;
        }
        a = 1.0 / (c * c + 1.0).sqrt();
        sn = if sn < 0.0 { -a } else { a };
    }
    if bo {
        sn /= d;
    }
    sn
}

/// `WFunc` / `XFunc`: the shape radius at angle `ang`; `d` starts at the `hypergon` parameter
/// (`[hypergon, hypergon_n, hypergon_r, star, star_n, star_slope, lituus, lituus_a, super, super_m,
/// super_n1, super_n2, super_n3, hypergon_d, hypergon_n, lituus_a, star_n, star_slope, super_m, super_n1]`).
fn wx_total(d: &[f64], ang: f64) -> f64 {
    let (hypergon, star, lituus, sup) = (d[0], d[3], d[6], d[8]);
    let (hd, hn, la, sn, ss, sm, sn1) = (d[13], d[14], d[15], d[16], d[17], d[18], d[19]);
    let mut total = 0.0;
    if hypergon != 0.0 {
        let t1 = fmod(ang.abs(), M_2PI / hn) - M_PI / hn;
        let t2 = sqr(t1.tan()) + 1.0;
        if t2 >= sqr(hd) {
            total += hypergon;
        } else {
            total += hypergon * (hd - (sqr(hd) - t2).sqrt()) / t2.sqrt();
        }
    }
    if star != 0.0 {
        let t1 = (fmod(ang.abs(), M_2PI / sn) - M_PI / sn).abs().tan();
        total += star * (sqr(ss) * (1.0 + sqr(t1)) / sqr(t1 + ss)).sqrt();
    }
    if lituus != 0.0 {
        total += lituus * (ang / M_PI + 1.0).powf(la);
    }
    if sup != 0.0 {
        let sa = ang * sm;
        total += sup * (sa.cos().abs().powf(d[11]) + sa.sin().abs().powf(d[12])).powf(sn1);
    }
    total
}

/// `TruchetAEFunc.dn1`
fn truchet_ae_dn1(x: i32) -> f64 {
    let n = (x << 13) ^ x;
    let h = n.wrapping_mul(n.wrapping_mul(n).wrapping_mul(15731).wrapping_add(789221)).wrapping_add(1376312589);
    (h & 0x7fffffff) as f64 * (1.0 / 2147483647.0)
}

/// One `TruchetAEFunc.transform` step on the state `[x, y, z1, z2]` with map `i`; returns (u1, v1, z).
fn truchet_ae_step(d: &[f64], st: &mut [f64; 4], i: i32) -> (f64, f64, f64) {
    const M_SQ3_2: f64 = 0.866_025_403_784_438_6;
    let fmod2 = |num: f64, den: f64| {
        let t = fmod(num, den);
        if t < 0.0 && den > 0.0 { t + den } else { t }
    };
    let (x, y) = (st[0], st[1]);
    let (u1, v1) = match (d[4] as i32, i) {
        (1, 0) => (-x * 0.5 + 1.0, y * 0.5 + 1.0),
        (1, 1) => (x * 0.5 - 1.0, y * 0.5 + 1.0),
        (1, 2) => (-x * 0.5 + 1.0, -y * 0.5 - 1.0),
        (1, 3) => (x * 0.5 - 1.0, -y * 0.5 - 1.0),
        (2, 0) => (x * 0.5 + 1.0, y * 0.5 + 1.0),
        (2, 1) => (x * 0.5 - 1.0, y * 0.5 - 1.0),
        (2, 2) => (x * 0.5 - 1.0, y * 0.5 + 1.0),
        (2, 3) => (-x * 0.5 - 1.0, -y * 0.5 + 1.0),
        (3, 0) => (x * 0.5, y * 0.5),
        (3, 1) => (x * 0.5 - 1.0, y * 0.5 + 1.0),
        (3, 2) => (-y * 0.5 - 1.0, x * 0.5 - 1.0),
        (3, 3) => (y * 0.5 + 1.0, -x * 0.5 + 1.0),
        (4, 0) => (x * 0.5 + 1.0, y * 0.5 + M_SQ3_2),
        (4, 1) => (x * 0.5 - 1.0, y * 0.5 + M_SQ3_2),
        (4, 2) => (x * 0.5, y * 0.5 - M_SQ3_2),
        (4, 3) => (x * 0.5, -y * 0.5 + M_SQ3_2),
        (_, 0) => {
            let mut j = y * 0.5 + 1.0 + 0.7;
            if j > 2.0 {
                j -= 2.0;
            }
            (x * 0.5 + 1.0, j)
        }
        (_, 1) => (x * 0.5 - 1.0, y * 0.5 + 1.0),
        (_, 2) => (x * 0.5 + 1.0, y * 0.5 - 1.0),
        (_, 3) => (x * 0.5 - 1.0, y * 0.5 - 1.0),
        _ => (0.0, 0.0),
    };
    if (0..4).contains(&i) {
        let k = &d[6 + 4 * i as usize..10 + 4 * i as usize];
        st[2] = fmod2(k[0] * st[2] + k[1], 1.0);
        st[3] = fmod2(k[2] * st[3] + k[3], if i == 2 { 0.5 } else { 1.0 });
    }
    let (z1, z2) = (st[2], st[3]);
    let (step, width, seed, style) = (d[1], d[2], d[0] as i32, d[5] as i32);
    let (sinang, cosang) = (d[22], d[23]);
    let uu1 = cosang * u1 - sinang * v1;
    let vv1 = sinang * u1 + cosang * v1;
    let m = (uu1 / step).floor() as i32;
    let n = (vv1 / step).floor() as i32;
    let u = uu1 / step - m as f64;
    let v = vv1 / step - n as f64;
    let chance = truchet_ae_dn1(m.wrapping_add(n.wrapping_mul(34)).wrapping_add(seed));
    let pick = |c: bool| if c { z1 } else { z2 };
    let dist = |du: f64, dv: f64| (du * du + dv * dv).sqrt();
    let z = match style {
        0 => {
            if chance > 0.5 {
                pick((v < u + 1.0 - width && v > u + width) || (v < u - width && v > u - 1.0 + width))
            } else {
                pick((v < -u + 1.0 - width && v > -u + width) || (v > -u + 1.0 + width && v < -u + 2.0 - width))
            }
        }
        1 => {
            let sq = java_max((u - 0.5).abs(), (v - 0.5).abs()) - 0.5 * (1.0 - width) < 0.0;
            if chance < 0.25 {
                pick(sq ^ (u > v))
            } else if chance < 0.5 {
                pick(sq ^ (u > 1.0 - v))
            } else if chance < 0.75 {
                pick(sq ^ (u < v))
            } else {
                pick(sq ^ (u < 1.0 - v))
            }
        }
        2 | 3 => {
            let lim = 0.5 * (1.0 - width);
            let arc = |c: f64| ((c - 0.5).abs() - 0.5).abs() - lim;
            let t1 = style == 2 && chance < 0.5 || style == 3 && chance < 0.333333;
            let t2 = style == 2 || chance < 0.666666;
            let cir = if t1 {
                arc(java_min(dist(u - 1.0, v - 1.0), dist(u, v)))
            } else if t2 {
                arc(java_min(dist(u - 1.0, v), dist(u, v - 1.0)))
            } else {
                0.5 * width - java_min((u - 0.5).abs(), (v - 0.5).abs())
            };
            pick(cir < 0.0)
        }
        4 => {
            let sq = java_max((u - 0.5).abs(), (v - 0.5).abs()) - 0.5 * (1.0 - width) < 0.0;
            let c = if chance < 0.125 {
                u > v
            } else if chance < 0.25 {
                u > 1.0 - v
            } else if chance < 0.375 {
                u < v
            } else if chance < 0.5 {
                u < 1.0 - v
            } else if chance < 0.625 {
                u < 0.5
            } else if chance < 0.75 {
                v < 0.5
            } else if chance < 0.875 {
                u > 0.5
            } else {
                v > 0.5
            };
            pick(sq ^ c)
        }
        5 => {
            let inv = (m % 2 == 0) ^ (n % 2 == 0);
            if chance < 0.5 {
                let cir0 = java_min(dist(u - 1.0, v - 1.0), dist(u, v));
                let mut sq = java_min(cir0, dist(u - 1.0, v));
                sq = java_min(sq, dist(u, v - 1.0));
                pick((cir0.abs() - 0.5 < 0.0) ^ (sq.abs() - width < 0.0) ^ inv)
            } else {
                let cir0 = java_min(dist(u - 1.0, v), dist(u, v - 1.0));
                let mut sq = java_min(cir0, dist(u - 1.0, v - 1.0));
                sq = java_min(sq, dist(u, v));
                if (cir0.abs() - 0.5 < 0.0) ^ (sq.abs() - width < 0.0) ^ inv { z2 } else { z1 }
            }
        }
        _ => 0.0,
    };
    (u1, v1, z)
}

const TRUCHET_AE_WGSL: &str = "fn truchet_ae_dn1(x: i32) -> f32 {
  let n = (x << 13u) ^ x;
  let h = n * (n * n * 15731 + 789221) + 1376312589;
  return f32(h & 0x7fffffff) * (1.0 / 2147483647.0);
}
fn truchet_ae_fmod2(num: f32, den: f32) -> f32 {
  let t = fmod(num, den);
  if (t < 0.0 && den > 0.0) { return t + den; }
  return t;
}
fn truchet_ae_dist(du: f32, dv: f32) -> f32 { return sqrt(du * du + dv * dv); }
// returns (u1, v1, z1, z2) of one step from the state st
fn truchet_ae_step(base: u32, st: vec4<f32>, i: i32) -> vec4<f32> {
  let SQ3_2 = 0.8660254037844386;
  let x = st.x;
  let y = st.y;
  var u1 = 0.0;
  var v1 = 0.0;
  let lay = i32(prm[base + 4u]);
  if (lay == 1) {
    if (i == 0) { u1 = -x * 0.5 + 1.0; v1 = y * 0.5 + 1.0; }
    else if (i == 1) { u1 = x * 0.5 - 1.0; v1 = y * 0.5 + 1.0; }
    else if (i == 2) { u1 = -x * 0.5 + 1.0; v1 = -y * 0.5 - 1.0; }
    else { u1 = x * 0.5 - 1.0; v1 = -y * 0.5 - 1.0; }
  } else if (lay == 2) {
    if (i == 0) { u1 = x * 0.5 + 1.0; v1 = y * 0.5 + 1.0; }
    else if (i == 1) { u1 = x * 0.5 - 1.0; v1 = y * 0.5 - 1.0; }
    else if (i == 2) { u1 = x * 0.5 - 1.0; v1 = y * 0.5 + 1.0; }
    else { u1 = -x * 0.5 - 1.0; v1 = -y * 0.5 + 1.0; }
  } else if (lay == 3) {
    if (i == 0) { u1 = x * 0.5; v1 = y * 0.5; }
    else if (i == 1) { u1 = x * 0.5 - 1.0; v1 = y * 0.5 + 1.0; }
    else if (i == 2) { u1 = -y * 0.5 - 1.0; v1 = x * 0.5 - 1.0; }
    else { u1 = y * 0.5 + 1.0; v1 = -x * 0.5 + 1.0; }
  } else if (lay == 4) {
    if (i == 0) { u1 = x * 0.5 + 1.0; v1 = y * 0.5 + SQ3_2; }
    else if (i == 1) { u1 = x * 0.5 - 1.0; v1 = y * 0.5 + SQ3_2; }
    else if (i == 2) { u1 = x * 0.5; v1 = y * 0.5 - SQ3_2; }
    else { u1 = x * 0.5; v1 = -y * 0.5 + SQ3_2; }
  } else {
    if (i == 0) {
      u1 = x * 0.5 + 1.0;
      v1 = y * 0.5 + 1.0 + 0.7;
      if (v1 > 2.0) { v1 -= 2.0; }
    }
    else if (i == 1) { u1 = x * 0.5 - 1.0; v1 = y * 0.5 + 1.0; }
    else if (i == 2) { u1 = x * 0.5 + 1.0; v1 = y * 0.5 - 1.0; }
    else { u1 = x * 0.5 - 1.0; v1 = y * 0.5 - 1.0; }
  }
  let ko = base + 6u + 4u * u32(i);
  let z1 = truchet_ae_fmod2(prm[ko] * st.z + prm[ko + 1u], 1.0);
  let z2 = truchet_ae_fmod2(prm[ko + 2u] * st.w + prm[ko + 3u], select(1.0, 0.5, i == 2));
  let step = prm[base + 1u];
  let width = prm[base + 2u];
  let style = i32(prm[base + 5u]);
  let sinang = prm[base + 22u];
  let cosang = prm[base + 23u];
  let uu1 = cosang * u1 - sinang * v1;
  let vv1 = sinang * u1 + cosang * v1;
  let m = i32(floor(uu1 / step));
  let n = i32(floor(vv1 / step));
  let u = uu1 / step - f32(m);
  let v = vv1 / step - f32(n);
  let chance = truchet_ae_dn1(m + 34 * n + i32(prm[base]));
  var c = false;
  var swap = false;
  if (style == 0) {
    if (chance > 0.5) {
      c = (v < u + 1.0 - width && v > u + width) || (v < u - width && v > u - 1.0 + width);
    } else {
      c = (v < -u + 1.0 - width && v > -u + width) || (v > -u + 1.0 + width && v < -u + 2.0 - width);
    }
  } else if (style == 1 || style == 4) {
    let sq = max(abs(u - 0.5), abs(v - 0.5)) - 0.5 * (1.0 - width) < 0.0;
    var t = false;
    if (style == 1) {
      if (chance < 0.25) { t = u > v; } else if (chance < 0.5) { t = u > 1.0 - v; } else if (chance < 0.75) { t = u < v; } else { t = u < 1.0 - v; }
    } else {
      if (chance < 0.125) { t = u > v; } else if (chance < 0.25) { t = u > 1.0 - v; } else if (chance < 0.375) { t = u < v; }
      else if (chance < 0.5) { t = u < 1.0 - v; } else if (chance < 0.625) { t = u < 0.5; } else if (chance < 0.75) { t = v < 0.5; }
      else if (chance < 0.875) { t = u > 0.5; } else { t = v > 0.5; }
    }
    c = sq != t;
  } else if (style == 2 || style == 3) {
    let lim = 0.5 * (1.0 - width);
    var cir = 0.0;
    if ((style == 2 && chance < 0.5) || (style == 3 && chance < 0.333333)) {
      cir = abs(abs(min(truchet_ae_dist(u - 1.0, v - 1.0), truchet_ae_dist(u, v)) - 0.5) - 0.5) - lim;
    } else if (style == 2 || chance < 0.666666) {
      cir = abs(abs(min(truchet_ae_dist(u - 1.0, v), truchet_ae_dist(u, v - 1.0)) - 0.5) - 0.5) - lim;
    } else {
      cir = 0.5 * width - min(abs(u - 0.5), abs(v - 0.5));
    }
    c = cir < 0.0;
  } else if (style == 5) {
    let inv = (m % 2 == 0) != (n % 2 == 0);
    if (chance < 0.5) {
      let cir0 = min(truchet_ae_dist(u - 1.0, v - 1.0), truchet_ae_dist(u, v));
      let sq = min(min(cir0, truchet_ae_dist(u - 1.0, v)), truchet_ae_dist(u, v - 1.0));
      c = ((abs(cir0) - 0.5 < 0.0) != (abs(sq) - width < 0.0)) != inv;
    } else {
      let cir0 = min(truchet_ae_dist(u - 1.0, v), truchet_ae_dist(u, v - 1.0));
      let sq = min(min(cir0, truchet_ae_dist(u - 1.0, v - 1.0)), truchet_ae_dist(u, v));
      c = !(((abs(cir0) - 0.5 < 0.0) != (abs(sq) - width < 0.0)) != inv);
    }
  } else {
    return vec4<f32>(u1, v1, 0.0, 0.0);
  }
  let z = select(z2, z1, c);
  return vec4<f32>(u1, v1, z, z);
}
@body
var st = vec4<f32>(0.0);
let n = 16 + min(i32(rnd() * 16.0), 15);
for (var k = 0; k < n; k++) {
  st = truchet_ae_step(base, st, min(i32(floor(4.0 * rnd())), 3));
}
v.x += st.x;
v.y += st.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
v.color = st.z;";

/// `TreeFunc`: branch `k` (in the recursion order of `tree()`) -> (x, y, cx, cy).
fn tree_js_branch(d: &[f64], level: i32, mut k: i64) -> (f64, f64, f64, f64) {
    let (ratio, bend, branch) = (d[5], d[10], d[11]);
    let (mut x, mut y, mut ang, mut r) = (0.0, 0.0, M_PI / 2.0, 2.0);
    let mut sub = (3i64.pow(level as u32 + 1) - 1) / 2;
    for _ in 0..level {
        if k == 0 {
            break;
        }
        k -= 1;
        sub = (sub - 1) / 3;
        let c = k / sub;
        k %= sub;
        x += ang.cos() * r;
        y += ang.sin() * r;
        match c {
            0 => {
                ang = ang + bend - branch;
                r *= ratio;
            }
            1 => {
                ang = ang + bend + branch;
                r *= ratio;
            }
            _ => {
                ang += bend;
                r *= 1.0 - ratio;
            }
        }
    }
    (x, y, x + ang.cos() * r, y + ang.sin() * r)
}

/// `TriPrism3DFunc.sdTriPrism`
fn triprism3d_sd(p: [f64; 3], hx: f64, hy: f64) -> f64 {
    let k = 3f64.sqrt();
    let hx = hx * (0.5 * k);
    let mut px = p[0] / hx;
    let mut py = p[1] / hx;
    px = px.abs() - 1.0;
    py += 1.0 / k;
    if px + k * py > 0.0 {
        let (tx, ty) = ((px - k * py) / 2.0, (-k * px - py) / 2.0);
        px = tx;
        py = ty;
    }
    px -= java_min(java_max(px, -2.0), 0.0);
    let s = if -py > 0.0 { 1.0 } else if -py < 0.0 { -1.0 } else { -py };
    let d1 = (px * px + py * py).sqrt() * s * hx;
    let d2 = p[2].abs() - hy;
    let m1 = if d1 < 0.0 { 0.0 } else { d1 };
    let m2 = if d2 < 0.0 { 0.0 } else { d2 };
    let mx = if d1 < d2 { d2 } else { d1 };
    (m1 * m1 + m2 * m2).sqrt() + if 0.0 < mx { 0.0 } else { mx }
}

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
