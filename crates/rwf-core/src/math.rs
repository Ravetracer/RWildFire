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

//! Math constants and helpers mirroring `org.jwildfire.base.mathlib.MathLib`, `GfxMathLib` and the
//! numeric helpers of `org.jwildfire.base.Tools`.
//!
//! Java -> Rust quick reference (see also `variations/PORTING.md`):
//! * `MathLib.sin/cos/tan/sqrt/exp/log/log10/atan/asin/acos/atan2/sinh/cosh/tanh/floor` -> the `f64`
//!   methods (`x.sin()`, `y.atan2(x)`, ...); `MathLib.pow(x, y)` -> `x.powf(y)`;
//!   `MathLib.fabs` -> `x.abs()`; `MathLib.trunc` -> `x.trunc()`; `MathLib.sinAndCos` -> `x.sin_cos()`
//! * `MathLib.min/max` (= `Math.min/max`, NaN-propagating) -> [`java_min`] / [`java_max`]
//!   (`a.min(b)` is fine whenever NaN cannot occur)
//! * `MathLib.round` / `Math.round` -> [`java_round`], `MathLib.rint` -> [`rint`],
//!   `MathLib.sign` -> [`sgn`], `MathLib.acosh` -> [`acosh`], `MathLib.erf` -> [`erf`],
//!   `MathLib.lgamma` -> [`lgamma`]
//! * `Tools.FTOI` -> [`ftoi`], `Tools.limitValue` -> [`limit_value`]
//! * `GfxMathLib.lerp/blerp/clamp/smoothstep/smootherstep/smootheststep/step` ->
//!   [`gfx_lerp`], [`gfx_blerp`], [`gfx_clamp`], [`gfx_smoothstep`], [`gfx_smootherstep`],
//!   [`gfx_smootheststep`], [`gfx_step`] (note: [`lerp`]/[`blerp`] use `(1-t)*a + t*b`, the
//!   GfxMathLib versions use `a + (b-a)*t`)

pub const EPSILON: f64 = 1.0e-8;
pub const SMALL_EPSILON: f64 = 1.0e-300;
pub const M_PI: f64 = std::f64::consts::PI;
pub const M_2PI: f64 = 2.0 * M_PI;
pub const M_PI_2: f64 = M_PI * 0.5;
pub const M_PI_4: f64 = M_PI * 0.25;
pub const M_1_PI: f64 = 1.0 / M_PI;
pub const M_2_PI: f64 = 2.0 / M_PI;
pub const M_1_2PI: f64 = M_1_PI / 2.0;
pub const M_SQRT2: f64 = std::f64::consts::SQRT_2;
pub const M_E: f64 = std::f64::consts::E;
pub const M_LOG2E: f64 = std::f64::consts::LOG2_E;
pub const C_255: f64 = 255.0;

#[inline]
pub fn sqr(x: f64) -> f64 {
    x * x
}

/// Java-style `fmod`: result has the sign of the dividend.
#[inline]
pub fn fmod(a: f64, b: f64) -> f64 {
    a % b
}

#[inline]
pub fn frac(x: f64) -> f64 {
    x - x.trunc()
}

#[inline]
pub fn sgn(x: f64) -> f64 {
    if x < 0.0 {
        -1.0
    } else if x > 0.0 {
        1.0
    } else {
        0.0
    }
}

#[inline]
pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    (1.0 - t) * a + t * b
}

#[inline]
pub fn blerp(ul: f64, ur: f64, ll: f64, lr: f64, x: f64, y: f64) -> f64 {
    lerp(lerp(ul, ur, x), lerp(ll, lr, x), y)
}

#[inline]
pub fn sinc(x: f64) -> f64 {
    let x = x * M_PI;
    if x.abs() > EPSILON { x.sin() / x } else { 1.0 }
}

#[inline]
pub fn round_color(v: f64) -> i32 {
    let r = (v + 0.5) as i32;
    r.clamp(0, 255)
}

/// RGB (0..1) <-> HSL (0..1) conversion as implemented in JWildfire's `HSLRGBConverter`.
pub fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let h = h.clamp(0.0, 1.0);
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let v = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
    if v <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let h6 = (h * 6.0).clamp(0.0, 6.0);
    let y = l + l - v;
    let f = h6 - h6.trunc();
    let x = y + (v - y) * f;
    let z = v - (v - y) * f;
    match h6 as i32 {
        0 => (v, x, y),
        1 => (z, v, y),
        2 => (y, v, x),
        3 => (y, z, v),
        4 => (x, y, v),
        5 => (v, y, z),
        _ => (v, x, y),
    }
}

// ---------------------------------------------------------------------------------------------
// Java semantics helpers (`Math`, `MathLib`, `Tools`)
// ---------------------------------------------------------------------------------------------

/// `Math.max` / `MathLib.max`: NaN if either argument is NaN, `max(-0.0, 0.0) == 0.0`.
#[inline]
pub fn java_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        // Only differs for +-0.0: prefer the positive zero.
        if a.is_sign_negative() { b } else { a }
    } else if a > b {
        a
    } else {
        b
    }
}

/// `Math.min` / `MathLib.min`: NaN if either argument is NaN, `min(-0.0, 0.0) == -0.0`.
#[inline]
pub fn java_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        if a.is_sign_negative() { a } else { b }
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Math.round` / `MathLib.round` (as double): nearest integer, ties towards positive infinity,
/// NaN -> 0, saturating at the `long` range.
#[inline]
pub fn java_round(x: f64) -> f64 {
    if x.is_nan() {
        return 0.0;
    }
    let f = x.floor();
    // `x - floor(x)` is exact, so this does not suffer from the `floor(x + 0.5)` rounding issue.
    let r = if x - f >= 0.5 { f + 1.0 } else { f };
    r.clamp(i64::MIN as f64, i64::MAX as f64)
}

/// `MathLib.rint` (`Math.rint`): nearest integer, ties to even.
#[inline]
pub fn rint(x: f64) -> f64 {
    x.round_ties_even()
}

/// `Tools.FTOI`: round half away from zero to `int` (saturating; NaN -> 0).
#[inline]
pub fn ftoi(val: f64) -> i32 {
    if val > 0.0 {
        (val + 0.5) as i32
    } else if val < 0.0 {
        (-(val.abs() + 0.5)) as i32
    } else {
        0
    }
}

/// `Tools.limitValue` (both the `double` and the `int` overload).
#[inline]
pub fn limit_value<T: PartialOrd>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// `MathLib.acosh`: `log(sqrt(d² - 1) + d)`.
#[inline]
pub fn acosh(d: f64) -> f64 {
    ((d * d - 1.0).sqrt() + d).ln()
}

/// `MathLib.erf`: Chebyshev approximation (fractional error < 1.2e-7).
pub fn erf(z: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.5 * z.abs());
    let ans = 1.0
        - t * (-z * z - 1.26551223
            + t * (1.00002368
                + t * (0.37409196
                    + t * (0.09678418
                        + t * (-0.18628806
                            + t * (0.27886807
                                + t * (-1.13520398 + t * (1.48851587 + t * (-0.82215223 + t * 0.17087277)))))))))
            .exp();
    if z >= 0.0 { ans } else { -ans }
}

/// `MathLib.lgamma`: log of the gamma function (Lanczos-style approximation).
pub fn lgamma(x: f64) -> f64 {
    let tmp = (x - 0.5) * (x + 4.5).ln() - (x + 4.5);
    let ser = 1.0 + 76.18009173 / x - 86.50532033 / (x + 1.0) + 24.01409822 / (x + 2.0) - 1.231739516 / (x + 3.0)
        + 0.00120858003 / (x + 4.0)
        - 0.00000536382 / (x + 5.0);
    tmp + (ser * (2.0 * std::f64::consts::PI).sqrt()).ln()
}

// ---------------------------------------------------------------------------------------------
// GfxMathLib
// ---------------------------------------------------------------------------------------------

/// `GfxMathLib.clamp(val, min, max)`: `max(min, min(max, val))` (Java `Math` semantics).
#[inline]
pub fn gfx_clamp(val: f64, min: f64, max: f64) -> f64 {
    java_max(min, java_min(max, val))
}

/// `GfxMathLib.lerp(s, e, t)`: `s + (e - s) * t`.
#[inline]
pub fn gfx_lerp(s: f64, e: f64, t: f64) -> f64 {
    s + (e - s) * t
}

/// `GfxMathLib.blerp(c00, c10, c01, c11, tx, ty)`.
#[inline]
pub fn gfx_blerp(c00: f64, c10: f64, c01: f64, c11: f64, tx: f64, ty: f64) -> f64 {
    gfx_lerp(gfx_lerp(c00, c10, tx), gfx_lerp(c01, c11, tx), ty)
}

/// `GfxMathLib.smoothstep`.
#[inline]
pub fn gfx_smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let x = gfx_clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// `GfxMathLib.smootherstep`.
#[inline]
pub fn gfx_smootherstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let x = gfx_clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

/// `GfxMathLib.smootheststep`.
#[inline]
pub fn gfx_smootheststep(edge0: f64, edge1: f64, x: f64) -> f64 {
    let x = gfx_clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    x * x * x * x * (x * (x * (x * -20.0 + 70.0) - 84.0) + 35.0)
}

/// `GfxMathLib.step`: `x < edge ? 0 : 1`.
#[inline]
pub fn gfx_step(edge: f64, x: f64) -> f64 {
    if x < edge { 0.0 } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_rounding_helpers() {
        assert_eq!(java_round(0.5), 1.0);
        assert_eq!(java_round(-0.5), 0.0);
        assert_eq!(java_round(-1.5), -1.0);
        assert_eq!(java_round(2.4), 2.0);
        assert_eq!(java_round(0.49999999999999994), 0.0);
        assert_eq!(java_round(f64::NAN), 0.0);
        assert_eq!(rint(2.5), 2.0);
        assert_eq!(rint(3.5), 4.0);
        assert_eq!(ftoi(2.5), 3);
        assert_eq!(ftoi(-2.5), -3);
        assert_eq!(ftoi(-2.4), -2);
        assert_eq!(ftoi(0.0), 0);
        assert_eq!(ftoi(f64::NAN), 0);
        assert_eq!(ftoi(1e20), i32::MAX);
    }

    #[test]
    fn limit_and_minmax() {
        assert_eq!(limit_value(5, 0, 3), 3);
        assert_eq!(limit_value(-1.5, 0.0, 3.0), 0.0);
        assert_eq!(limit_value(1.5, 0.0, 3.0), 1.5);
        assert!(java_max(f64::NAN, 1.0).is_nan());
        assert!(java_min(1.0, f64::NAN).is_nan());
        assert!(java_max(-0.0, 0.0).is_sign_positive());
        assert!(java_min(0.0, -0.0).is_sign_negative());
        assert_eq!(java_max(2.0, 3.0), 3.0);
        assert_eq!(java_min(2.0, 3.0), 2.0);
    }

    #[test]
    fn special_functions() {
        assert!((erf(0.5) - 0.520_499_877_8).abs() < 2e-7);
        assert!((erf(-1.0) + 0.842_700_792_9).abs() < 2e-7);
        // lgamma(5) = ln(24)
        assert!((lgamma(5.0) - 24f64.ln()).abs() < 1e-8);
        assert!((acosh(2.0) - 2f64.acosh()).abs() < 1e-12);
        assert_eq!(gfx_blerp(0.0, 1.0, 2.0, 3.0, 0.5, 0.5), 1.5);
        assert_eq!(gfx_smoothstep(0.0, 1.0, 0.5), 0.5);
        assert_eq!(gfx_smootherstep(0.0, 1.0, 2.0), 1.0);
        assert_eq!(gfx_step(0.5, 0.5), 1.0);
    }
}
