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

// ---- complex.wgsl: org.jwildfire.base.mathlib.Complex as pure functions on vec2 -------------
// cx_<rust method>(z, ...) returns the new value: Java `z.AtanH()` -> `z = cx_atanh(z);`.
// The save slot (Save/Restore/Keep/Recall/Switch) becomes a local variable; PerFix is not
// supported. f32 adaptation: MagInv treats |z|^2 < 1e-37 (instead of 1e-100) as zero.

const CX_MAG_INV_EPS: f32 = 1.0e-37;

fn cx_flip(z: vec2<f32>) -> vec2<f32> { return z.yx; }
fn cx_conj(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(z.x, -z.y); }
fn cx_neg(z: vec2<f32>) -> vec2<f32> { return -z; }
fn cx_sig(z: vec2<f32>) -> f32 {
  if (z.x == 0.0) { return 0.0; }
  return select(-1.0, 1.0, z.x > 0.0);
}
fn cx_sig2(z: vec2<f32>) -> f32 { return select(-1.0, 1.0, z.x >= 0.0); }
fn cx_mag2(z: vec2<f32>) -> f32 { return z.x * z.x + z.y * z.y; }
fn cx_mag2eps(z: vec2<f32>) -> f32 { return z.x * z.x + z.y * z.y + 1.0e-20; }
fn cx_mag_inv(z: vec2<f32>) -> f32 {
  let m2 = cx_mag2(z);
  return select(1.0 / m2, 1.0, m2 < CX_MAG_INV_EPS);
}

fn cx_sqr(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y); }
fn cx_recip(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(z.x, -z.y) * cx_mag_inv(z); }
fn cx_scale(z: vec2<f32>, s: f32) -> vec2<f32> { return z * s; }
fn cx_mul(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> {
  if (w.y == 0.0) { return z * w.x; }
  return vec2<f32>(z.x * w.x - z.y * w.y, z.x * w.y + z.y * w.x);
}
// z / w
fn cx_div(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(z.y * w.y + z.x * w.x, z.y * w.x - z.x * w.y) * cx_mag_inv(w);
}
// DivR: w / z
fn cx_div_r(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(w.y * z.y + w.x * z.x, w.y * z.x - w.x * z.y) * cx_mag_inv(z);
}
fn cx_add(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> { return z + w; }
fn cx_sub(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> { return z - w; }
// SubR: w - z
fn cx_sub_r(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> { return w - z; }
fn cx_inc(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(z.x + 1.0, z.y); }
fn cx_dec(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(z.x - 1.0, z.y); }

fn cx_radius(z: vec2<f32>) -> f32 { return length(z); }
fn cx_arg(z: vec2<f32>) -> f32 { return atan2(z.y, z.x); }
fn cx_to_p(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(length(z), atan2(z.y, z.x)); }
fn cx_un_p(p: vec2<f32>) -> vec2<f32> { return vec2<f32>(p.x * cos(p.y), p.x * sin(p.y)); }
fn cx_norm(z: vec2<f32>) -> vec2<f32> { return z * sqrt(cx_mag_inv(z)); }

fn cx_sqrt(z: vec2<f32>) -> vec2<f32> {
  let rad = length(z);
  let sb = select(1.0, -1.0, z.y < 0.0);
  // max(): guards against rad < |re| through f32 rounding.
  return vec2<f32>(sqrt(max(0.5 * (rad + z.x), 0.0)), sb * sqrt(max(0.5 * (rad - z.x), 0.0)));
}
fn cx_pow(z_in: vec2<f32>, e: f32) -> vec2<f32> {
  if (e == 0.0) { return vec2<f32>(1.0, 0.0); }
  let ex = abs(e);
  var z = z_in;
  if (e < 0.0) { z = cx_recip(z); }
  if (ex == 0.5) { return cx_sqrt(z); }
  if (ex == 1.0) { return z; }
  if (ex == 2.0) { return cx_sqr(z); }
  let p = cx_to_p(z);
  return cx_un_p(vec2<f32>(pow(p.x, ex), p.y * ex));
}
fn cx_exp(z: vec2<f32>) -> vec2<f32> { return cx_un_p(vec2<f32>(exp(z.x), z.y)); }
fn cx_log(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(0.5 * log(cx_mag2eps(z)), atan2(z.y, z.x)); }
fn cx_cpow(z: vec2<f32>, ex: vec2<f32>) -> vec2<f32> {
  if (ex.y == 0.0) { return cx_pow(z, ex.x); }
  return cx_exp(cx_mul(cx_log(z), ex));
}
// NextPow with the saved value s: z * s
fn cx_next_pow(z: vec2<f32>, s: vec2<f32>) -> vec2<f32> { return cx_mul(z, s); }

fn cx_sinh(z: vec2<f32>) -> vec2<f32> {
  let re = exp(z.x);
  let er = 1.0 / re;
  let rr = 0.5 * (re - er);
  let ri = rr + er;
  return vec2<f32>(cos(z.y) * rr, sin(z.y) * ri);
}
fn cx_cosh(z: vec2<f32>) -> vec2<f32> {
  let re = exp(z.x);
  let er = 1.0 / re;
  let rr = 0.5 * (re - er);
  let ri = rr + er;
  return vec2<f32>(cos(z.y) * ri, sin(z.y) * rr);
}
fn cx_sin(z: vec2<f32>) -> vec2<f32> { return cx_sinh(z.yx).yx; }
// Java quirk kept: Cos() = flip/cosh/flip evaluates to i * cos(z).
fn cx_cos(z: vec2<f32>) -> vec2<f32> { return cx_cosh(z.yx).yx; }

fn cx_atanh(z: vec2<f32>) -> vec2<f32> {
  let d = -cx_dec(z);
  return cx_log(cx_div(cx_inc(z), d)) * 0.5;
}
fn cx_asinh(z: vec2<f32>) -> vec2<f32> { return cx_log(z + cx_pow(cx_inc(cx_sqr(z)), 0.5)); }
fn cx_acosh(z: vec2<f32>) -> vec2<f32> { return cx_log(z + cx_pow(cx_dec(cx_sqr(z)), 0.5)); }
fn cx_acoth(z: vec2<f32>) -> vec2<f32> { return cx_atanh(cx_recip(z)); }
fn cx_asech(z: vec2<f32>) -> vec2<f32> { return cx_asinh(cx_recip(z)); }
fn cx_acosech(z: vec2<f32>) -> vec2<f32> { return cx_acosh(cx_recip(z)); }
fn cx_atan(z: vec2<f32>) -> vec2<f32> { return cx_atanh(z.yx).yx; }
fn cx_asin(z: vec2<f32>) -> vec2<f32> { return cx_asinh(z.yx).yx; }
fn cx_acos(z: vec2<f32>) -> vec2<f32> {
  let a = cx_asinh(z.yx).yx;
  return vec2<f32>(HALF_PI - a.x, -a.y);
}

// Means: z.XMean(w)
fn cx_a_mean(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> { return (z + w) * 0.5; }
fn cx_root_mean_s(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> {
  return cx_pow((cx_sqr(z) + cx_sqr(w)) * 0.5, 0.5);
}
fn cx_g_mean(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> { return cx_pow(cx_mul(z, w), 0.5); }
fn cx_heronian(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> {
  let hm = cx_g_mean(z, w);
  return (z + w + hm) * 0.333333333333333333;
}
fn cx_h_mean(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> {
  let s = z + w;
  let d = 0.5 * dot(s, s);
  if (d == 0.0) { return vec2<f32>(0.0); }
  let p2 = cx_mag2(z);
  let q2 = cx_mag2(w);
  if (p2 * q2 == 0.0) { return vec2<f32>(0.0); }
  return (1.0 / d) * (z * q2 + w * p2);
}
fn cx_l_mean(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> {
  return cx_div(z - w, cx_log(cx_div(z, w)));
}
