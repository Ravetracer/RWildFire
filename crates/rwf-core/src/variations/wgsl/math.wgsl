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

// ---- math.wgsl: Java / MathLib / Tools / GfxMathLib helpers (mirrors crate::math) ----------

// +inf / NaN produced at runtime (const-expressions must not evaluate to inf or NaN).
fn math_inf() -> f32 {
  var bits = 0x7f800000u;
  return bitcast<f32>(bits);
}
fn math_nan() -> f32 {
  var bits = 0x7fc00000u;
  return bitcast<f32>(bits);
}

// Tools.FTOI: round half away from zero.
fn ftoi(v: f32) -> i32 {
  if (v > 0.0) { return i32(v + 0.5); }
  if (v < 0.0) { return i32(-(abs(v) + 0.5)); }
  return 0;
}

// Tools.limitValue (double / int overloads).
fn limit_value(v: f32, lo: f32, hi: f32) -> f32 {
  if (v < lo) { return lo; }
  if (v > hi) { return hi; }
  return v;
}
fn limit_value_i(v: i32, lo: i32, hi: i32) -> i32 {
  if (v < lo) { return lo; }
  if (v > hi) { return hi; }
  return v;
}

// Math.round / MathLib.round: ties towards +inf (WGSL round() rounds ties to even).
fn java_round(x: f32) -> f32 {
  let f = floor(x);
  return select(f, f + 1.0, x - f >= 0.5);
}

// Math.pow semantics for negative bases (WGSL pow() is undefined for x < 0):
// integer exponents give the signed result, other exponents NaN.
fn java_pow(x: f32, y: f32) -> f32 {
  if (x > 0.0) { return pow(x, y); }
  if (y == 0.0) { return 1.0; }
  if (x == 0.0) {
    if (y > 0.0) { return 0.0; }
    return math_inf();
  }
  if (y != floor(y)) { return math_nan(); }
  let r = pow(-x, y);
  return select(r, -r, fract(y * 0.5) != 0.0);
}

// GfxMathLib.lerp / blerp (s + (e - s) * t).
fn gfx_lerp(s: f32, e: f32, t: f32) -> f32 { return s + (e - s) * t; }
fn gfx_blerp(c00: f32, c10: f32, c01: f32, c11: f32, tx: f32, ty: f32) -> f32 {
  return gfx_lerp(gfx_lerp(c00, c10, tx), gfx_lerp(c01, c11, tx), ty);
}
fn gfx_smootherstep(edge0: f32, edge1: f32, x_in: f32) -> f32 {
  let x = clamp((x_in - edge0) / (edge1 - edge0), 0.0, 1.0);
  return x * x * x * (x * (x * 6.0 - 15.0) + 10.0);
}
fn gfx_smootheststep(edge0: f32, edge1: f32, x_in: f32) -> f32 {
  let x = clamp((x_in - edge0) / (edge1 - edge0), 0.0, 1.0);
  return x * x * x * x * (x * (x * (x * -20.0 + 70.0) - 84.0) + 35.0);
}

// MathLib.erf (Chebyshev approximation).
fn erf(z: f32) -> f32 {
  let t = 1.0 / (1.0 + 0.5 * abs(z));
  let ans = 1.0 - t * exp(-z * z - 1.26551223 + t * (1.00002368 + t * (0.37409196 + t * (0.09678418
    + t * (-0.18628806 + t * (0.27886807 + t * (-1.13520398 + t * (1.48851587
    + t * (-0.82215223 + t * 0.17087277)))))))));
  return select(-ans, ans, z >= 0.0);
}

// MathLib.lgamma
fn lgamma(x: f32) -> f32 {
  let tmp = (x - 0.5) * log(x + 4.5) - (x + 4.5);
  let ser = 1.0 + 76.18009173 / x - 86.50532033 / (x + 1.0) + 24.01409822 / (x + 2.0)
    - 1.231739516 / (x + 3.0) + 0.00120858003 / (x + 4.0) - 0.00000536382 / (x + 5.0);
  return tmp + log(ser * sqrt(TWO_PI));
}

// JacCnFunc.Jacobi_elliptic: (sn, cn, dn) of u for the parameter emmc.
fn math_jacobi_elliptic(uu: f32, emmc: f32) -> vec3<f32> {
  var sn = 0.0;
  var cn = 0.0;
  var dn = 0.0;
  var emc = emmc;
  var u = uu;
  if (emc == 0.0) {
    cn = 1.0 / cosh(u);
    return vec3<f32>(tanh(u), cn, cn);
  }
  var em = array<f32, 13>();
  var en = array<f32, 13>();
  var a = 1.0;
  var b = 0.0;
  var c = 0.0;
  var d = 0.0;
  var l = 0;
  let bo = emc < 0.0;
  if (bo) {
    d = 1.0 - emc;
    emc = -emc / d;
    d = sqrt(d);
    u = d * u;
  }
  dn = 1.0;
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
  sn = sin(u);
  cn = cos(u);
  if (sn != 0.0) {
    a = cn / sn;
    c = a * c;
    for (var ii = l; ii >= 0; ii--) {
      b = em[ii];
      a = c * a;
      c = dn * c;
      dn = (en[ii] + a) / (b + a);
      a = c / b;
    }
    a = 1.0 / sqrt(c * c + 1.0);
    if (sn < 0.0) { sn = -a; } else { sn = a; }
    cn = c * sn;
  }
  if (bo) {
    a = dn;
    dn = cn;
    cn = a;
    sn = sn / d;
  }
  return vec3<f32>(sn, cn, dn);
}
