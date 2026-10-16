// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
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

//! Post-render image effects (RWildFire only): photo-style adjustments of the finished RGBA8
//! image, after tone mapping and the denoiser. This is the reference implementation; the GPU
//! preview (`rwf-gpu` `effects.wgsl`) computes the same in the same order, in f32.
//!
//! Order: dehaze, exposure, contrast (stage "base"), then highlights and shadows, texture and
//! clarity, whites and blacks, curves, vignette and grain. Blurs are three box blurs per
//! direction (a Gaussian approximation) computed with a sliding window; their radii are
//! fractions of the image height, so a preview looks like the full size render.

use rayon::prelude::*;

use crate::mixer::{Curve, CurveInterpolation, Envelope, FILTER_SCALE};

/// Curve coordinates run over 0..25600, as those of the channel mixer.
pub const CURVE_SCALE: f64 = 256.0 * FILTER_SCALE;
/// Samples per curve in [`ImageEffects::curve_lut`].
pub const LUT_SIZE: usize = 1024;

/// Names of the four tone curves in the flame attributes.
const CURVE_NAMES: [&str; 4] = ["rgb", "red", "green", "blue"];

/// The image effects of a flame. Neutral values leave the image unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageEffects {
    /// Exposure in stops (-3..3).
    pub exposure: f64,
    /// S-curve contrast around the middle gray (-1..1).
    pub contrast: f64,
    /// Brightens (> 0) or darkens bright areas, found on a blurred copy (-1..1).
    pub highlights: f64,
    /// Brightens (> 0) or darkens dark areas (-1..1).
    pub shadows: f64,
    /// Moves the white point: > 0 brighter highlights (more clipping), < 0 compressed (-1..1).
    pub whites: f64,
    /// Moves the black point: > 0 lifted blacks, < 0 deeper blacks (-1..1).
    pub blacks: f64,
    /// Local contrast of fine detail (-1..1; < 0 softens).
    pub texture: f64,
    /// Local contrast of larger structures, mostly in the midtones (-1..1).
    pub clarity: f64,
    /// Removes haze: subtracts the local minimum of the channels (0..1).
    pub dehaze: f64,
    /// Darkens (< 0) or lightens the image edges (-1..1).
    pub vignette: f64,
    /// Where the vignette starts, from the center (0) to the corners (1).
    pub vignette_size: f64,
    /// How soft the vignette edge is (0..1).
    pub vignette_feather: f64,
    /// Film grain strength (0..1).
    pub grain: f64,
    /// Grain size; 1 = one pixel at an image height of 1000 pixels (0.5..4).
    pub grain_size: f64,
    /// Tone curves: all channels, red, green, blue (x and y over 0..[`CURVE_SCALE`]).
    pub curves: [Curve; 4],
}

impl Default for ImageEffects {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            texture: 0.0,
            clarity: 0.0,
            dehaze: 0.0,
            vignette: 0.0,
            vignette_size: 0.5,
            vignette_feather: 0.5,
            grain: 0.0,
            grain_size: 1.0,
            curves: std::array::from_fn(|_| Curve::standard(true)),
        }
    }
}

/// True if `c` maps every value to itself (all points on the diagonal).
pub fn is_identity(c: &Curve) -> bool {
    c.x.len() >= 2 && c.x.iter().zip(&c.y).all(|(x, y)| (*x as f64 - y).abs() < 0.5)
}

impl ImageEffects {
    /// True if any effect changes the image.
    pub fn is_active(&self) -> bool {
        Prepared::new(self, 1000, 1000).flags != 0
    }

    /// True if one of the tone curves is not the identity.
    pub fn curves_active(&self) -> bool {
        self.curves.iter().any(|c| !is_identity(c))
    }

    /// The four curves sampled at [`LUT_SIZE`] points over 0..1, values 0..1 (identity for
    /// unchanged curves).
    pub fn curve_lut(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(4 * LUT_SIZE);
        for c in &self.curves {
            let curve = (!is_identity(c)).then(|| CurveSampler::new(c));
            for k in 0..LUT_SIZE {
                let t = k as f64 / (LUT_SIZE - 1) as f64;
                let v = curve.as_ref().map_or(t, |s| s.at(t * CURVE_SCALE) / CURVE_SCALE);
                out.push(v.clamp(0.0, 1.0) as f32);
            }
        }
        out
    }

    /// Reads the `effects_*` attributes and removes them from `attrs`.
    pub fn take_from_attrs(attrs: &mut Vec<(String, String)>) -> Self {
        let mut fx = Self::default();
        for (k, v) in attrs.iter() {
            let Some(name) = k.strip_prefix("effects_") else { continue };
            let f = v.trim().parse::<f64>().ok();
            let slot = match name {
                "exposure" => &mut fx.exposure,
                "contrast" => &mut fx.contrast,
                "highlights" => &mut fx.highlights,
                "shadows" => &mut fx.shadows,
                "whites" => &mut fx.whites,
                "blacks" => &mut fx.blacks,
                "texture" => &mut fx.texture,
                "clarity" => &mut fx.clarity,
                "dehaze" => &mut fx.dehaze,
                "vignette" => &mut fx.vignette,
                "vignette_size" => &mut fx.vignette_size,
                "vignette_feather" => &mut fx.vignette_feather,
                "grain" => &mut fx.grain,
                "grain_size" => &mut fx.grain_size,
                _ => {
                    if let Some(i) = name.strip_prefix("curve_").and_then(|c| CURVE_NAMES.iter().position(|n| *n == c))
                        && let Some(c) = parse_curve(v)
                    {
                        fx.curves[i] = c;
                    }
                    continue;
                }
            };
            if let Some(f) = f.filter(|f| f.is_finite()) {
                *slot = f;
            }
        }
        attrs.retain(|(k, _)| !k.starts_with("effects_"));
        fx
    }

    /// Writes the attributes read by [`Self::take_from_attrs`]; nothing while no effect is set.
    pub fn write_attrs(&self, mut attr: impl FnMut(&str, String)) {
        if *self == Self::default() {
            return;
        }
        let f = crate::io::fmt_f;
        let values = [
            ("exposure", self.exposure),
            ("contrast", self.contrast),
            ("highlights", self.highlights),
            ("shadows", self.shadows),
            ("whites", self.whites),
            ("blacks", self.blacks),
            ("texture", self.texture),
            ("clarity", self.clarity),
            ("dehaze", self.dehaze),
            ("vignette", self.vignette),
            ("vignette_size", self.vignette_size),
            ("vignette_feather", self.vignette_feather),
            ("grain", self.grain),
            ("grain_size", self.grain_size),
        ];
        for (k, v) in values {
            attr(&format!("effects_{k}"), f(v));
        }
        for (name, c) in CURVE_NAMES.iter().zip(&self.curves) {
            if !is_identity(c) {
                let pts: Vec<String> = c.x.iter().zip(&c.y).map(|(x, y)| format!("{x} {}", f(*y))).collect();
                attr(&format!("effects_curve_{name}"), format!("{} {}", c.interpolation.as_str(), pts.join(" ")));
            }
        }
    }
}

/// A curve evaluated on its interpolated points. Unlike [`Envelope::evaluate`], which keeps a
/// JWildfire quirk for the channel mixer (it searches the unused tail of the point buffer and
/// fails near the end of the range), this is exact over the whole range.
struct CurveSampler {
    points: Vec<(f64, f64)>,
    first: (f64, f64),
    last: (f64, f64),
}

impl CurveSampler {
    fn new(c: &Curve) -> Self {
        let n = c.x.len().min(c.y.len());
        let first = (c.x[0] as f64, c.y[0]);
        let last = (c.x[n - 1] as f64, c.y[n - 1]);
        Self { points: Envelope::new(c).points(), first, last }
    }

    fn at(&self, t: f64) -> f64 {
        if t <= self.first.0 || self.points.len() < 2 {
            return self.first.1;
        }
        if t >= self.last.0 {
            return self.last.1;
        }
        let i = self.points.partition_point(|p| p.0 < t).clamp(1, self.points.len() - 1);
        let ((x0, y0), (x1, y1)) = (self.points[i - 1], self.points[i]);
        if x1 - x0 < 1e-8 { y0 } else { y0 + (t - x0) / (x1 - x0) * (y1 - y0) }
    }
}

/// `"SPLINE x0 y0 x1 y1 …"` → a curve.
fn parse_curve(s: &str) -> Option<Curve> {
    let mut it = s.split_whitespace();
    let interpolation = match it.next()? {
        "LINEAR" => CurveInterpolation::Linear,
        "BEZIER" => CurveInterpolation::Bezier,
        _ => CurveInterpolation::Spline,
    };
    let v: Vec<f64> = it.filter_map(|t| t.parse().ok()).collect();
    if v.len() < 4 || !v.len().is_multiple_of(2) {
        return None;
    }
    let mut c = Curve::standard(true);
    c.interpolation = interpolation;
    c.x = v.iter().step_by(2).map(|x| x.round().clamp(0.0, CURVE_SCALE) as i32).collect();
    c.y = v.iter().skip(1).step_by(2).map(|y| y.clamp(0.0, CURVE_SCALE)).collect();
    c.x.windows(2).all(|p| p[0] < p[1]).then_some(c)
}

pub const FLAG_DEHAZE: u32 = 1;
pub const FLAG_EXPOSURE: u32 = 2;
pub const FLAG_CONTRAST: u32 = 4;
pub const FLAG_TONES: u32 = 8;
pub const FLAG_TEXTURE: u32 = 16;
pub const FLAG_CLARITY: u32 = 32;
pub const FLAG_LEVELS: u32 = 64;
pub const FLAG_CURVES: u32 = 128;
pub const FLAG_VIGNETTE: u32 = 256;
pub const FLAG_GRAIN: u32 = 512;

/// The effects as the per-pixel code needs them (the GPU gets the same values).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Prepared {
    pub width: u32,
    pub height: u32,
    /// `FLAG_*` of the effects that change the image.
    pub flags: u32,
    pub exposure_gain: f32,
    pub contrast_pow: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub black: f32,
    pub level_scale: f32,
    pub texture: f32,
    pub clarity: f32,
    pub dehaze: f32,
    pub vignette: f32,
    pub vig_lo: f32,
    pub vig_hi: f32,
    pub grain: f32,
    pub grain_cell: f32,
    /// Box radii of the blurs: dark channel (dehaze), tone mask, texture, clarity.
    pub boxes: [[u32; 3]; 4],
}

/// Blur strengths (Gaussian sigma) as fractions of the image height.
const SIGMA_HAZE: f64 = 0.04;
const SIGMA_TONES: f64 = 0.025;
const SIGMA_TEXTURE: f64 = 0.0015;
const SIGMA_CLARITY: f64 = 0.012;

impl Prepared {
    pub fn new(fx: &ImageEffects, width: u32, height: u32) -> Self {
        let on = |v: f64| v.abs() > 1e-6;
        let mut flags = 0;
        let mut set = |cond: bool, flag: u32| {
            if cond {
                flags |= flag;
            }
        };
        let dehaze = fx.dehaze.clamp(0.0, 1.0);
        set(on(dehaze), FLAG_DEHAZE);
        set(on(fx.exposure), FLAG_EXPOSURE);
        set(on(fx.contrast), FLAG_CONTRAST);
        set(on(fx.highlights) || on(fx.shadows), FLAG_TONES);
        set(on(fx.texture), FLAG_TEXTURE);
        set(on(fx.clarity), FLAG_CLARITY);
        set(on(fx.whites) || on(fx.blacks), FLAG_LEVELS);
        set(fx.curves_active(), FLAG_CURVES);
        set(on(fx.vignette), FLAG_VIGNETTE);
        set(fx.grain > 1e-6, FLAG_GRAIN);
        let h = height.max(1) as f64;
        let white = 1.0 - 0.25 * fx.whites.clamp(-1.0, 1.0);
        let black = -0.15 * fx.blacks.clamp(-1.0, 1.0);
        let size = fx.vignette_size.clamp(0.0, 1.0);
        let feather = fx.vignette_feather.clamp(0.0, 1.0);
        let vig_lo = size * (1.0 - feather);
        Self {
            width,
            height,
            flags,
            exposure_gain: 2f64.powf(fx.exposure.clamp(-10.0, 10.0) / 2.2) as f32,
            contrast_pow: 2f64.powf(1.5 * fx.contrast.clamp(-1.0, 1.0)) as f32,
            highlights: fx.highlights.clamp(-1.0, 1.0) as f32,
            shadows: fx.shadows.clamp(-1.0, 1.0) as f32,
            black: black as f32,
            level_scale: (1.0 / (white - black)) as f32,
            texture: fx.texture.clamp(-1.0, 1.0) as f32,
            clarity: fx.clarity.clamp(-1.0, 1.0) as f32,
            dehaze: dehaze as f32,
            vignette: fx.vignette.clamp(-1.0, 1.0) as f32,
            vig_lo: vig_lo as f32,
            vig_hi: (vig_lo + feather * (1.0 - vig_lo) + 1e-3) as f32,
            grain: fx.grain.clamp(0.0, 1.0) as f32,
            grain_cell: (fx.grain_size.clamp(0.1, 10.0) * h / 1000.0).max(1.0) as f32,
            boxes: [SIGMA_HAZE, SIGMA_TONES, SIGMA_TEXTURE, SIGMA_CLARITY].map(|s| box_radii((s * h).max(0.6))),
        }
    }

    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }

    /// Whether the dark channel is needed (dehaze).
    pub fn needs_dark(&self) -> bool {
        self.has(FLAG_DEHAZE)
    }

    /// Whether the luminance after the base stage is needed (tones, texture, clarity).
    pub fn needs_lum(&self) -> bool {
        self.flags & (FLAG_TONES | FLAG_TEXTURE | FLAG_CLARITY) != 0
    }
}

/// Radii of three box blurs that together approximate a Gaussian with `sigma` (Kutskir).
pub fn box_radii(sigma: f64) -> [u32; 3] {
    let n = 3.0;
    let w_ideal = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let mut wl = w_ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let wu = wl + 2;
    let wlf = wl as f64;
    let m = ((12.0 * sigma * sigma - n * wlf * wlf - 4.0 * n * wlf - 3.0 * n) / (-4.0 * wlf - 4.0)).round() as i64;
    [0, 1, 2].map(|i| (((if i < m { wl } else { wu }) - 1) / 2) as u32)
}

/// One box blur of radius `r` along a line of `n` values (`get(i)`), averaging only the values
/// inside the line; writes with `put(i, v)`. The same sliding window runs on the GPU.
#[inline]
fn box_line(n: usize, r: usize, get: impl Fn(usize) -> f32, mut put: impl FnMut(usize, f32)) {
    let mut sum = 0f32;
    let mut cnt = 0u32;
    for j in 0..=r.min(n - 1) {
        sum += get(j);
        cnt += 1;
    }
    for i in 0..n {
        put(i, sum / cnt as f32);
        let add = i + r + 1;
        if add < n {
            sum += get(add);
            cnt += 1;
        }
        if i >= r {
            sum -= get(i - r);
            cnt -= 1;
        }
    }
}

/// Three box blurs per direction (rows first) of a `w` × `h` image of floats.
pub fn blur(src: &[f32], w: usize, h: usize, radii: [u32; 3]) -> Vec<f32> {
    let mut rows = vec![0f32; w * h];
    rows.par_chunks_mut(w).zip(src.par_chunks(w)).for_each(|(out, line)| {
        let (mut a, mut b) = (vec![0f32; w], vec![0f32; w]);
        box_line(w, radii[0] as usize, |i| line[i], |i, v| a[i] = v);
        box_line(w, radii[1] as usize, |i| a[i], |i, v| b[i] = v);
        box_line(w, radii[2] as usize, |i| b[i], |i, v| out[i] = v);
    });
    let cols: Vec<Vec<f32>> = (0..w)
        .into_par_iter()
        .map(|x| {
            let (mut a, mut b, mut c) = (vec![0f32; h], vec![0f32; h], vec![0f32; h]);
            box_line(h, radii[0] as usize, |i| rows[i * w + x], |i, v| a[i] = v);
            box_line(h, radii[1] as usize, |i| a[i], |i, v| b[i] = v);
            box_line(h, radii[2] as usize, |i| b[i], |i, v| c[i] = v);
            c
        })
        .collect();
    let mut out = vec![0f32; w * h];
    for (x, col) in cols.iter().enumerate() {
        for (y, v) in col.iter().enumerate() {
            out[y * w + x] = *v;
        }
    }
    out
}

#[inline]
fn lum(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

#[inline]
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
fn s_curve(v: f32, k: f32) -> f32 {
    if v < 0.5 { 0.5 * (2.0 * v).powf(k) } else { 1.0 - 0.5 * (2.0 * (1.0 - v)).powf(k) }
}

/// Dehaze, exposure and contrast of one pixel (`d`: blurred dark channel).
#[inline]
pub fn base(p: &Prepared, c0: [f32; 3], d: f32) -> [f32; 3] {
    let mut c = c0;
    if p.has(FLAG_DEHAZE) {
        let haze = p.dehaze * 0.8 * d;
        let inv = 1.0 / (1.0 - haze);
        c = c.map(|v| ((v - haze) * inv).max(0.0));
    }
    if p.has(FLAG_EXPOSURE) {
        c = c.map(|v| v * p.exposure_gain);
    }
    c = c.map(|v| v.clamp(0.0, 1.0));
    if p.has(FLAG_CONTRAST) {
        c = c.map(|v| s_curve(v, p.contrast_pow));
    }
    c
}

/// Integer hash of a grain lattice point (the GPU uses the same).
#[inline]
fn hash(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x8da6_b343) ^ y.wrapping_mul(0xd816_3841) ^ 0x9e37_79b9;
    h = (h ^ (h >> 16)).wrapping_mul(0x7feb_352d);
    h = (h ^ (h >> 15)).wrapping_mul(0x846c_a68b);
    h ^ (h >> 16)
}

/// A random value in -1..1 (triangular distribution) per lattice point.
#[inline]
fn lattice(x: u32, y: u32) -> f32 {
    let h = hash(x, y);
    (h & 0xffff) as f32 / 65535.0 + (h >> 16) as f32 / 65535.0 - 1.0
}

/// Value noise with lattice spacing `cell` pixels.
#[inline]
pub fn grain_noise(x: u32, y: u32, cell: f32) -> f32 {
    let fx = x as f32 / cell;
    let fy = y as f32 / cell;
    let (ix, iy) = (fx.floor(), fy.floor());
    let sx = smoothstep(0.0, 1.0, fx - ix);
    let sy = smoothstep(0.0, 1.0, fy - iy);
    let (x0, y0) = (ix as u32, iy as u32);
    let top = lattice(x0, y0) + (lattice(x0 + 1, y0) - lattice(x0, y0)) * sx;
    let bottom = lattice(x0, y0 + 1) + (lattice(x0 + 1, y0 + 1) - lattice(x0, y0 + 1)) * sx;
    top + (bottom - top) * sy
}

#[inline]
fn lut_at(lut: &[f32], k: usize, v: f32) -> f32 {
    let t = v.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32;
    let i = (t as usize).min(LUT_SIZE - 2);
    let f = t - i as f32;
    let a = lut[k * LUT_SIZE + i];
    a + (lut[k * LUT_SIZE + i + 1] - a) * f
}

/// The blurred helper images of one pixel: dark channel, luminance after the base stage and
/// its three blurs (tone mask, texture, clarity).
#[derive(Clone, Copy, Default)]
pub struct Local {
    pub dark: f32,
    pub lum: f32,
    pub mask: f32,
    pub texture: f32,
    pub clarity: f32,
}

/// The final color (0..1) of the pixel at `x`, `y` with input color `c0`.
#[inline]
pub fn finish(p: &Prepared, lut: &[f32], x: u32, y: u32, c0: [f32; 3], l: Local) -> [f32; 3] {
    let mut c = base(p, c0, l.dark);
    if p.has(FLAG_TONES) {
        let ms = 1.0 - smoothstep(0.0, 0.5, l.mask);
        let mh = smoothstep(0.5, 1.0, l.mask);
        let g = (2.0 * (p.shadows * ms + p.highlights * mh) / 2.2).exp2();
        c = c.map(|v| v * g);
    }
    let mut dl = 0.0;
    if p.has(FLAG_TEXTURE) {
        dl += p.texture * 1.5 * (l.lum - l.texture);
    }
    if p.has(FLAG_CLARITY) {
        let m = 2.0 * l.lum - 1.0;
        dl += p.clarity * 1.5 * (1.0 - m * m) * (l.lum - l.clarity);
    }
    c = c.map(|v| v + dl);
    if p.has(FLAG_LEVELS) {
        c = c.map(|v| (v - p.black) * p.level_scale);
    }
    c = c.map(|v| v.clamp(0.0, 1.0));
    if p.has(FLAG_CURVES) {
        c = [1, 2, 3].map(|k| lut_at(lut, k, lut_at(lut, 0, c[k - 1])));
    }
    if p.has(FLAG_VIGNETTE) {
        let dx = (x as f32 + 0.5) / p.width as f32 * 2.0 - 1.0;
        let dy = (y as f32 + 0.5) / p.height as f32 * 2.0 - 1.0;
        let v = smoothstep(p.vig_lo, p.vig_hi, ((dx * dx + dy * dy) * 0.5).sqrt());
        if p.vignette < 0.0 {
            let k = 1.0 + p.vignette * v;
            c = c.map(|ch| ch * k);
        } else {
            let k = p.vignette * v;
            c = c.map(|ch| ch + k * (1.0 - ch));
        }
    }
    if p.has(FLAG_GRAIN) {
        let l = lum(c);
        let a = p.grain * 0.15 * grain_noise(x, y, p.grain_cell) * (0.3 + 2.8 * l * (1.0 - l));
        c = c.map(|v| v + a);
    }
    c.map(|v| v.clamp(0.0, 1.0))
}

/// 0..1 → 0..255.
#[inline]
pub fn to_u8(v: f32) -> u8 {
    (v * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8
}

/// Applies the effects to an RGBA8 image in place (alpha unchanged).
pub fn apply(fx: &ImageEffects, rgba: &mut [u8], width: u32, height: u32) {
    let (w, h) = (width as usize, height as usize);
    if rgba.len() != w * h * 4 || w == 0 || h == 0 {
        return;
    }
    let p = Prepared::new(fx, width, height);
    if p.flags == 0 {
        return;
    }
    let lut = if p.has(FLAG_CURVES) { fx.curve_lut() } else { Vec::new() };
    let color = |i: usize| [rgba[i * 4] as f32 / 255.0, rgba[i * 4 + 1] as f32 / 255.0, rgba[i * 4 + 2] as f32 / 255.0];
    let n = w * h;
    let dark = if p.needs_dark() {
        let raw: Vec<f32> = (0..n).into_par_iter().map(|i| color(i).into_iter().fold(1f32, f32::min)).collect();
        blur(&raw, w, h, p.boxes[0])
    } else {
        Vec::new()
    };
    let dark_at = |i: usize| dark.get(i).copied().unwrap_or(0.0);
    let lums: Vec<f32> = if p.needs_lum() { (0..n).into_par_iter().map(|i| lum(base(&p, color(i), dark_at(i)))).collect() } else { Vec::new() };
    let blurred = |flag: u32, k: usize| if p.has(flag) { blur(&lums, w, h, p.boxes[k]) } else { Vec::new() };
    let (mask, texture, clarity) = (blurred(FLAG_TONES, 1), blurred(FLAG_TEXTURE, 2), blurred(FLAG_CLARITY, 3));
    let at = |v: &Vec<f32>, i: usize| v.get(i).copied().unwrap_or(0.0);
    let out: Vec<[u8; 3]> = (0..n)
        .into_par_iter()
        .map(|i| {
            let l = Local { dark: dark_at(i), lum: at(&lums, i), mask: at(&mask, i), texture: at(&texture, i), clarity: at(&clarity, i) };
            finish(&p, &lut, (i % w) as u32, (i / w) as u32, color(i), l).map(to_u8)
        })
        .collect();
    for (px, c) in rgba.chunks_exact_mut(4).zip(out) {
        px[..3].copy_from_slice(&c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: u32, h: u32) -> Vec<u8> {
        (0..w * h).flat_map(|i| [((i % w) * 255 / (w - 1)) as u8, ((i / w) * 255 / (h - 1)) as u8, 128, 200]).collect()
    }

    #[test]
    fn neutral_effects_change_nothing() {
        let fx = ImageEffects::default();
        assert!(!fx.is_active());
        let mut img = ramp(32, 16);
        let orig = img.clone();
        apply(&fx, &mut img, 32, 16);
        assert_eq!(img, orig);
        // Active effects with values that cancel out still keep alpha.
        let fx = ImageEffects { exposure: 1.0, ..Default::default() };
        apply(&fx, &mut img, 32, 16);
        assert!(img.chunks(4).all(|p| p[3] == 200));
        assert!(img.chunks(4).zip(orig.chunks(4)).all(|(a, b)| a[0] >= b[0]));
    }

    #[test]
    fn box_radii_approximate_a_gaussian() {
        for sigma in [0.6, 2.0, 7.5, 30.0] {
            let r = box_radii(sigma);
            // Variance of a box of radius r: ((2r + 1)² - 1) / 12.
            let var: f64 = r.iter().map(|&r| (((2 * r + 1) * (2 * r + 1) - 1) as f64) / 12.0).sum();
            assert!((var.sqrt() - sigma).abs() < 0.6, "{sigma}: {r:?} -> {}", var.sqrt());
        }
    }

    #[test]
    fn blur_keeps_constant_images_and_the_mean() {
        let (w, h) = (17, 9);
        let flat = vec![0.25f32; w * h];
        assert!(blur(&flat, w, h, [2, 3, 3]).iter().all(|v| (v - 0.25).abs() < 1e-6));
        let mut spot = vec![0f32; w * h];
        spot[4 * w + 8] = 1.0;
        let b = blur(&spot, w, h, [1, 1, 1]);
        assert!((b.iter().sum::<f32>() - 1.0).abs() < 1e-4);
        assert!(b[4 * w + 8] < 1.0 && b[4 * w + 8] > b[4 * w + 6]);
    }

    #[test]
    fn effects_move_values_the_right_way() {
        let (w, h) = (64u32, 48u32);
        let mean = |img: &[u8]| img.chunks(4).map(|p| p[0] as f64 + p[1] as f64 + p[2] as f64).sum::<f64>() / (img.len() / 4) as f64;
        let base = ramp(w, h);
        let run = |fx: ImageEffects| {
            let mut img = base.clone();
            apply(&fx, &mut img, w, h);
            img
        };
        let m0 = mean(&base);
        assert!(mean(&run(ImageEffects { exposure: 1.0, ..Default::default() })) > m0);
        assert!(mean(&run(ImageEffects { exposure: -1.0, ..Default::default() })) < m0);
        assert!(mean(&run(ImageEffects { shadows: 1.0, ..Default::default() })) > m0);
        assert!(mean(&run(ImageEffects { highlights: -1.0, ..Default::default() })) < m0);
        assert!(mean(&run(ImageEffects { blacks: 1.0, ..Default::default() })) > m0);
        assert!(mean(&run(ImageEffects { vignette: -1.0, ..Default::default() })) < m0);
        assert!(mean(&run(ImageEffects { dehaze: 1.0, ..Default::default() })) < m0);
        // Contrast: dark gets darker, bright brighter, the middle stays.
        let c = run(ImageEffects { contrast: 1.0, ..Default::default() });
        let px = |img: &[u8], x: u32| img[((h / 2 * w + x) * 4) as usize];
        assert!(px(&c, 5) < px(&base, 5) && px(&c, w - 6) > px(&base, w - 6));
        // Grain is reproducible.
        let g = ImageEffects { grain: 0.5, ..Default::default() };
        assert_eq!(run(g.clone()), run(g));
    }

    #[test]
    fn curves_and_attributes_round_trip() {
        let mut fx = ImageEffects { exposure: 0.5, clarity: -0.25, vignette: -0.4, grain: 0.3, ..Default::default() };
        fx.curves[2].y[2] = 9000.0;
        assert!(fx.curves_active());
        let lut = fx.curve_lut();
        assert!((lut[LUT_SIZE / 2] - 0.5).abs() < 0.01, "rgb curve is the identity");
        assert!(lut[2 * LUT_SIZE + LUT_SIZE / 2] < 0.4, "green curve is lowered");
        // The top of an S-curve stays near the top (the mixer's evaluation fails there).
        let mut s_curve = ImageEffects::default();
        s_curve.curves[0] = parse_curve("SPLINE 0 0 6400 4000 12800 12800 19200 21000 25600 25600").unwrap();
        let lut = s_curve.curve_lut();
        assert!(lut[..LUT_SIZE].windows(2).all(|p| p[1] >= p[0] - 1e-3), "not monotone");
        assert!(lut[LUT_SIZE - 10] > 0.98);
        let mut attrs = Vec::new();
        fx.write_attrs(|k, v| attrs.push((k.to_string(), v)));
        attrs.push(("other".into(), "1".into()));
        let back = ImageEffects::take_from_attrs(&mut attrs);
        assert_eq!(attrs, vec![("other".to_string(), "1".to_string())]);
        assert_eq!((back.exposure, back.clarity, back.vignette, back.grain), (0.5, -0.25, -0.4, 0.3));
        assert_eq!(back.curves[2].y, fx.curves[2].y);
        assert!(is_identity(&back.curves[0]));
        let mut none = Vec::new();
        ImageEffects::default().write_attrs(|k, v| none.push((k.to_string(), v)));
        assert!(none.is_empty());
    }
}
