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

//! Log-density filtering and gamma correction (port of `LogDensityFilter`,
//! `LogScaleCalculator` and `GammaCorrectionFilter`), producing an RGBA8 image.

use rayon::prelude::*;

use super::filter::{FilterKernel, FilterMatrix};
use crate::math::{EPSILON, blerp, hsl_to_rgb, lerp, rgb_to_hsl, round_color};
use crate::model::{BgColorType, Flame, PostSymmetryType};

/// Read access to an accumulation raster: `[count, red, green, blue]` per cell.
pub trait Raster: Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn cell(&self, x: u32, y: u32) -> [f64; 4];
}

/// Parameters shared by the CPU tonemapper and the GPU shader.
#[derive(Clone, Debug)]
pub struct ToneParams {
    pub k1: f64,
    pub k2: f64,
    pub bg_glow: f64,
    pub white_level: f64,
    pub balance: [f64; 3],
    /// `1 / flame.gamma` (or 0).
    pub gamma: f64,
    pub gamma_threshold: f64,
    pub scl_gamma: f64,
    pub vib_int: f64,
    pub inverse_vib_int: f64,
    pub alpha_scale: f64,
    pub mod_saturation: f64,
    /// `1 / packets` for motion blur (JWildfire `motionBlurScl`), 1 otherwise.
    pub motion_blur_scl: f64,
}

impl ToneParams {
    /// `density` is the current sample density (samples per raster cell * oversample).
    pub fn new(flame: &Flame, image_width: u32, image_height: u32, ppu: f64, density: f64) -> Self {
        let oversample = flame.spatial_oversampling.max(1) as f64;
        let mut k1 = flame.contrast * 2.0 * flame.brightness / oversample;
        match flame.post_symmetry.kind {
            PostSymmetryType::Point => k1 /= flame.post_symmetry.order.max(1) as f64,
            PostSymmetryType::XAxis | PostSymmetryType::YAxis => k1 /= 2.0,
            PostSymmetryType::None => {}
        }
        let area = image_width as f64 * image_height as f64 / (ppu * ppu);
        let density = density.max(1e-9);
        let k2 = 1.0 / (flame.contrast * area * density);
        let bg_glow = flame.low_density_brightness * k2 * area / oversample;

        let gamma = if flame.gamma == 0.0 { 0.0 } else { 1.0 / flame.gamma };
        let vib_int = ((flame.vibrancy * 256.0 + 0.5) as i64).clamp(0, 256) as f64;
        let scl_gamma = if flame.gamma_threshold != 0.0 { flame.gamma_threshold.powf(gamma - 1.0) } else { 0.0 };
        Self {
            k1,
            k2,
            bg_glow,
            white_level: flame.white_level,
            balance: [flame.balance_red, flame.balance_green, flame.balance_blue],
            gamma,
            gamma_threshold: flame.gamma_threshold,
            scl_gamma,
            vib_int,
            inverse_vib_int: 256.0 - vib_int,
            alpha_scale: 1.0 - (3.0 * (flame.fg_opacity - 1.0)).atan() / 1.25,
            mod_saturation: (flame.saturation - 1.0).max(-1.0),
            motion_blur_scl: 1.0,
        }
    }

    #[inline]
    pub fn log_scale(&self, count: f64) -> f64 {
        let count = count * self.motion_blur_scl;
        (self.k1 * (1.0 + count * self.k2).log10() + self.bg_glow / (count + 1.0)) / (self.white_level * count)
    }
}

/// Background color per output pixel (0..255 ints as f64).
pub fn background_color(flame: &Flame, x: u32, y: u32, w: u32, h: u32) -> [i32; 3] {
    let bg = &flame.background;
    let c = |v: [u8; 3]| [v[0] as f64, v[1] as f64, v[2] as f64];
    match bg.kind {
        BgColorType::SingleColor => [bg.color[0] as i32, bg.color[1] as i32, bg.color[2] as i32],
        BgColorType::Gradient2x2 => {
            let fx = x as f64 / (w.max(2) - 1) as f64;
            let fy = y as f64 / (h.max(2) - 1) as f64;
            let (ul, ur, ll, lr) = (c(bg.ul), c(bg.ur), c(bg.ll), c(bg.lr));
            [0, 1, 2].map(|i| round_color(blerp(ul[i], ur[i], ll[i], lr[i], fx, fy)))
        }
        BgColorType::Gradient2x2C => {
            let (ul, ur, ll, lr, cc) = (c(bg.ul), c(bg.ur), c(bg.ll), c(bg.lr), c(bg.cc));
            let w2 = (w as i64 / 2 - 1).max(1) as f64;
            let h2 = (h as i64 / 2 - 1).max(1) as f64;
            let (px, py) = (x as f64, y as f64);
            let mid = |a: [f64; 3], b: [f64; 3]| [lerp(a[0], b[0], 0.5), lerp(a[1], b[1], 0.5), lerp(a[2], b[2], 0.5)];
            let (fx, fy, q_ul, q_ur, q_ll, q_lr);
            if px <= w2 {
                fx = px / w2;
                if py <= h2 {
                    fy = py / h2;
                    (q_ul, q_ur, q_ll, q_lr) = (ul, mid(ul, ur), mid(ll, ul), cc);
                } else {
                    fy = (py - h2) / h2;
                    (q_ul, q_ur, q_ll, q_lr) = (mid(ul, ll), cc, ll, mid(ll, lr));
                }
            } else {
                fx = (px - w2) / w2;
                if py <= h2 {
                    fy = py / h2;
                    (q_ul, q_ur, q_ll, q_lr) = (mid(ul, ur), ur, cc, mid(ur, lr));
                } else {
                    fy = (py - h2) / h2;
                    (q_ul, q_ur, q_ll, q_lr) = (cc, mid(ur, lr), mid(ll, lr), lr);
                }
            }
            [0, 1, 2].map(|i| round_color(blerp(q_ul[i], q_ur[i], q_ll[i], q_lr[i], fx.min(1.0), fy.min(1.0))))
        }
    }
}

/// Per-pixel kernel selection of the adaptive filter (`LogDensityFilter.getFilter`).
pub struct AdaptiveFilter {
    pub low_density: FilterMatrix,
    pub soften: FilterMatrix,
    pub detail: FilterMatrix,
    pub sharpness: f64,
    pub low_density_threshold: f64,
    /// Debug view: pixels are colored by the chosen kernel instead of being filtered.
    pub indicator: bool,
}

/// `FilterIndicator` colors.
const INDICATOR_GREY: [f64; 3] = [1.0, 1.0, 1.0];
const INDICATOR_DETAIL: [f64; 3] = [1.0, 0.2, 0.0];
const INDICATOR_SOFTEN: [f64; 3] = [0.0, 0.9, 0.0];
const INDICATOR_LOW_DENSITY: [f64; 3] = [0.0, 0.2, 1.1];

impl AdaptiveFilter {
    pub fn new(flame: &Flame, oversample: u32) -> Option<Self> {
        let radius_a = flame.spatial_filter_radius + 0.25;
        if !flame.filter_kernel.is_adaptive() || radius_a < 0.5 {
            return None;
        }
        Some(Self {
            low_density: FilterMatrix::new(FilterKernel::SinePow10, oversample, 1.5 * radius_a),
            soften: FilterMatrix::new(FilterKernel::SinePow10, oversample, radius_a),
            detail: FilterMatrix::new(flame.filter_kernel, oversample, 0.75 * radius_a),
            sharpness: flame.filter_sharpness,
            low_density_threshold: flame.filter_low_density,
            indicator: flame.filter_indicator,
        })
    }
}

#[inline]
fn lum(r: f64, g: f64, b: f64) -> f64 {
    0.299 * r + 0.588 * g + 0.113 * b
}

pub struct ToneMapper {
    pub params: ToneParams,
    pub primary: FilterMatrix,
    pub smoothing: FilterMatrix,
    pub adaptive: Option<AdaptiveFilter>,
    pub oversample: u32,
    pub border: u32,
    pub with_alpha: bool,
}

impl ToneMapper {
    pub fn new(flame: &Flame, image_width: u32, image_height: u32, ppu: f64, density: f64, with_alpha: bool) -> Self {
        let oversample = flame.spatial_oversampling.max(1);
        let primary = FilterMatrix::new(flame.filter_kernel, oversample, flame.spatial_filter_radius);
        let border = (primary.size as i64 - oversample as i64).max(0) as u32 / 2;
        Self {
            params: ToneParams::new(flame, image_width, image_height, ppu, density),
            primary,
            smoothing: FilterMatrix::new(FilterKernel::Gaussian, oversample, 0.75),
            adaptive: AdaptiveFilter::new(flame, oversample),
            oversample,
            border,
            with_alpha,
        }
    }

    /// Normalizes counts for a histogram filled by `n` motion-blur packets.
    pub fn with_packets(mut self, n: usize) -> Self {
        self.params.motion_blur_scl = 1.0 / n.max(1) as f64;
        self
    }

    /// Kernel and indicator color for one output pixel. Like JWildfire, the adaptive filter
    /// samples the raster without the border offset.
    fn choose_filter<R: Raster>(&self, raster: &R, x: u32, y: u32) -> (&FilterMatrix, [f64; 3]) {
        let Some(ad) = &self.adaptive else {
            return (&self.primary, INDICATOR_GREY);
        };
        let read = |rx: i64, ry: i64| read_cell(raster, rx, ry);
        let (cx, cy) = (x as i64 * self.oversample as i64, y as i64 * self.oversample as i64);
        let c = read(cx, cy);
        if c[0] < 1.0 {
            return (&self.primary, INDICATOR_GREY);
        }
        let intensity = self.params.log_scale(c[0]) * c[0] * self.params.white_level;
        if intensity < ad.low_density_threshold {
            return (&ad.low_density, INDICATOR_LOW_DENSITY);
        }
        let l = |dx: i64, dy: i64| {
            let c = read(cx + dx, cy + dy);
            lum(c[1], c[2], c[3])
        };
        // Scharr operator, including the (x + 1, y - 1) typo of LogDensityFilter.scharr_x.
        let sx = 3.0 * l(-1, -1) - 3.0 * l(1, -1) + 10.0 * l(-1, 0) - 10.0 * l(1, -1) + 3.0 * l(-1, 1) - 3.0 * l(1, 1);
        let sy = 3.0 * l(-1, -1) + 10.0 * l(0, -1) + 3.0 * l(1, -1) - 3.0 * l(-1, 1) - 10.0 * l(0, 1) - 3.0 * l(1, 1);
        if (1.0 + (sx * sx + sy * sy).sqrt()).log10() < ad.sharpness {
            (&ad.soften, INDICATOR_SOFTEN)
        } else {
            (&ad.detail, INDICATOR_DETAIL)
        }
    }

    /// Log-density filtered `[intensity, r, g, b]` for one output pixel.
    fn filtered<R: Raster>(&self, raster: &R, x: u32, y: u32) -> [f64; 4] {
        let p = &self.params;
        let os = self.oversample as i64;
        let b = self.border as i64;
        let read = |rx: i64, ry: i64| read_cell(raster, rx + b, ry + b);
        let (mut r, mut g, mut bl, mut intensity) = (0.0, 0.0, 0.0, 0.0);
        let (f, indicator) = self.choose_filter(raster, x, y);
        if self.adaptive.as_ref().is_some_and(|ad| ad.indicator) {
            let scl = if f.is_empty() { [1.0; 3] } else { indicator.map(|c| c * f.radius) };
            for px in 0..os {
                for py in 0..os {
                    let c = read(x as i64 * os + px, y as i64 * os + py);
                    if c[0] > 0.0 {
                        let ls = p.log_scale(c[0]);
                        let l = lum(ls * c[1], ls * c[2], ls * c[3]);
                        r += l * scl[0];
                        g += l * scl[1];
                        bl += l * scl[2];
                        intensity += ls * c[0] * p.white_level;
                    }
                }
            }
        } else if f.is_empty() {
            for px in 0..os {
                for py in 0..os {
                    let c = read(x as i64 * os + px, y as i64 * os + py);
                    if c[0] > 0.0 {
                        let ls = p.log_scale(c[0]);
                        r += ls * c[1] * p.balance[0];
                        g += ls * c[2] * p.balance[1];
                        bl += ls * c[3] * p.balance[2];
                        intensity += ls * c[0] * p.white_level;
                    }
                }
            }
        } else {
            for i in 0..f.size {
                for j in 0..f.size {
                    let c = read(x as i64 * os + j as i64 - f.half, y as i64 * os + i as i64 - f.half);
                    if c[0] > 0.0 {
                        let ls = p.log_scale(c[0]) * f.at(i, j);
                        r += ls * c[1] * p.balance[0];
                        g += ls * c[2] * p.balance[1];
                        bl += ls * c[3] * p.balance[2];
                        if !f.kernel.is_sharpening() {
                            intensity += ls * c[0] * p.white_level;
                        }
                    }
                }
            }
            if f.kernel.is_sharpening() {
                let s = &self.smoothing;
                for i in 0..s.size {
                    for j in 0..s.size {
                        let c = read(x as i64 * os + j as i64 - s.half, y as i64 * os + i as i64 - s.half);
                        if c[0] > 0.0 {
                            intensity += s.at(i, j) * p.white_level * p.log_scale(c[0]) * c[0];
                        }
                    }
                }
            }
        }
        [intensity, r, g, bl]
    }

    /// Gamma correction + background blend for one pixel, returns RGBA8.
    pub fn gamma_pixel(&self, f: [f64; 4], bg: [i32; 3]) -> [u8; 4] {
        let p = &self.params;
        let intensity = f[0];
        let (mut red, mut green, mut blue, alpha_out);
        if intensity > 0.0 {
            let alpha = if intensity <= p.gamma_threshold {
                let frac = intensity / p.gamma_threshold;
                (1.0 - frac) * intensity * p.scl_gamma + frac * intensity.powf(p.gamma)
            } else {
                intensity.powf(p.gamma)
            };
            let log_scl = p.vib_int * alpha / intensity;
            let alpha_int = ((alpha * 255.0 * p.alpha_scale + 0.5) as i32).clamp(0, 255);
            let inv_alpha = 255 - alpha_int;
            alpha_out = if self.with_alpha { alpha_int } else { 255 };
            let (rr, gg, bb) = if p.inverse_vib_int > 0.0 {
                (
                    log_scl * f[1] + p.inverse_vib_int * f[1].powf(p.gamma),
                    log_scl * f[2] + p.inverse_vib_int * f[2].powf(p.gamma),
                    log_scl * f[3] + p.inverse_vib_int * f[3].powf(p.gamma),
                )
            } else {
                (log_scl * f[1], log_scl * f[2], log_scl * f[3])
            };
            red = ((rr + 0.5) as i32 + (inv_alpha.wrapping_mul(bg[0]) >> 8)).clamp(0, 255);
            green = ((gg + 0.5) as i32 + (inv_alpha.wrapping_mul(bg[1]) >> 8)).clamp(0, 255);
            blue = ((bb + 0.5) as i32 + (inv_alpha.wrapping_mul(bg[2]) >> 8)).clamp(0, 255);
        } else {
            // Background xforms can leave the 0..255 range.
            red = bg[0].clamp(0, 255);
            green = bg[1].clamp(0, 255);
            blue = bg[2].clamp(0, 255);
            alpha_out = if self.with_alpha { 0 } else { 255 };
        }
        if p.mod_saturation != 0.0 {
            let (h, s, l) = rgb_to_hsl(red as f64 / 255.0, green as f64 / 255.0, blue as f64 / 255.0);
            let (r2, g2, b2) = hsl_to_rgb(h, s + p.mod_saturation, l);
            red = round_color(r2 * 255.0);
            green = round_color(g2 * 255.0);
            blue = round_color(b2 * 255.0);
        }
        [red as u8, green as u8, blue as u8, alpha_out as u8]
    }

    /// Gamma correction + background blend without quantization (`transformPointHDR`): linear
    /// RGB where 1.0 is 8-bit full scale. The saturation modifier is not applied (as in JWildfire).
    pub fn gamma_pixel_hdr(&self, f: [f64; 4], bg: [i32; 3]) -> [f32; 3] {
        let p = &self.params;
        let intensity = f[0];
        if intensity <= 0.0 {
            return bg.map(|c| c as f32 / 255.0);
        }
        let alpha = if intensity <= p.gamma_threshold {
            let frac = intensity / p.gamma_threshold;
            (1.0 - frac) * intensity * p.scl_gamma + frac * intensity.powf(p.gamma)
        } else {
            intensity.powf(p.gamma)
        };
        let log_scl = p.vib_int * alpha / intensity;
        let inv_alpha = 255.0 - (alpha * 255.0).clamp(0.0, 255.0);
        let mut out = [0f32; 3];
        for k in 0..3 {
            let c = f[k + 1];
            let raw = if p.inverse_vib_int > 0.0 { log_scl * c + p.inverse_vib_int * c.powf(p.gamma) } else { log_scl * c };
            out[k] = ((raw + inv_alpha * bg[k] as f64 / 255.0).max(0.0) / 255.0) as f32;
        }
        out
    }

    /// Tonemaps the whole raster into linear float RGB (`w * h * 3` values).
    pub fn render_hdr<R: Raster>(&self, flame: &Flame, raster: &R, w: u32, h: u32) -> Vec<f32> {
        match flame.mixer.color_func() {
            Some(func) => self.render_raster_hdr(flame, &mix_raster(raster, &func), w, h),
            None => self.render_raster_hdr(flame, raster, w, h),
        }
    }

    fn render_raster_hdr<R: Raster>(&self, flame: &Flame, raster: &R, w: u32, h: u32) -> Vec<f32> {
        let backdrop = super::background::Backdrop::new(flame, w, h);
        let mut out = vec![0f32; (w * h * 3) as usize];
        out.par_chunks_mut((w * 3) as usize).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let f = self.filtered(raster, x, y as u32);
                let px = self.gamma_pixel_hdr(f, backdrop.get(x, y as u32));
                row[(x * 3) as usize..(x * 3 + 3) as usize].copy_from_slice(&px);
            }
        });
        out
    }

    /// Tonemaps the whole raster into an RGBA8 buffer of `w * h * 4` bytes.
    pub fn render<R: Raster>(&self, flame: &Flame, raster: &R, w: u32, h: u32) -> Vec<u8> {
        match flame.mixer.color_func() {
            Some(func) => self.render_raster(flame, &mix_raster(raster, &func), w, h),
            None => self.render_raster(flame, raster, w, h),
        }
    }

    fn render_raster<R: Raster>(&self, flame: &Flame, raster: &R, w: u32, h: u32) -> Vec<u8> {
        let backdrop = super::background::Backdrop::new(flame, w, h);
        let mut out = vec![0u8; (w * h * 4) as usize];
        out.par_chunks_mut((w * 4) as usize).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let f = self.filtered(raster, x, y as u32);
                let bg = backdrop.get(x, y as u32);
                let px = self.gamma_pixel(f, bg);
                row[(x * 4) as usize..(x * 4 + 4) as usize].copy_from_slice(&px);
            }
        });
        out
    }
}

/// The raster with the colors of every cell remapped by the channel mixer (`LogDensityFilter`
/// with a `ColorFunc`).
pub fn mix_raster<R: Raster>(raster: &R, func: &crate::mixer::ColorFunc) -> super::cpu::FloatRaster {
    let (w, h) = (raster.width(), raster.height());
    let mut data = vec![0.0f32; (w * h * 4) as usize];
    data.par_chunks_mut((w * 4) as usize).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let c = raster.cell(x, y as u32);
            let o = (x * 4) as usize;
            row[o] = c[0] as f32;
            if c[0] > 0.0 {
                let m = func.map_cell(c[0], [c[1], c[2], c[3]]);
                row[o + 1] = m[0] as f32;
                row[o + 2] = m[1] as f32;
                row[o + 3] = m[2] as f32;
            }
        }
    });
    super::cpu::FloatRaster { width: w, height: h, data }
}

fn read_cell<R: Raster>(raster: &R, rx: i64, ry: i64) -> [f64; 4] {
    if rx < 0 || ry < 0 || rx >= raster.width() as i64 || ry >= raster.height() as i64 {
        [0.0; 4]
    } else {
        raster.cell(rx as u32, ry as u32)
    }
}

/// True if a filter radius is effectively off.
pub fn filter_disabled(radius: f64) -> bool {
    radius < EPSILON
}
