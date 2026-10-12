//! Math constants and helpers mirroring `org.jwildfire.base.mathlib.MathLib`.

pub const EPSILON: f64 = 1.0e-8;
pub const SMALL_EPSILON: f64 = 1.0e-300;
pub const M_PI: f64 = std::f64::consts::PI;
pub const M_2PI: f64 = 2.0 * M_PI;
pub const M_PI_2: f64 = M_PI * 0.5;
pub const M_PI_4: f64 = M_PI * 0.25;
pub const M_1_PI: f64 = 1.0 / M_PI;
pub const M_2_PI: f64 = 2.0 / M_PI;

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
