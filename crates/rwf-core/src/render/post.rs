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

//! Post-processing of a finished RGBA8 render: the flame's post denoiser, then its image
//! effects. Every final image (saved images, videos, batch renders) goes through
//! [`process`].

use super::denoise::{self, DenoiseAux, DenoiseOutcome};
use crate::model::Flame;

/// What [`process`] did.
#[derive(Clone, Debug, PartialEq)]
pub struct PostOutcome {
    pub denoise: DenoiseOutcome,
    /// The image effects were applied.
    pub effects: bool,
    /// The flame has no denoiser (so `denoise` only says that).
    pub denoiser_off: bool,
    /// The image was rendered larger and scaled down by this factor after denoising (1: not).
    pub supersampled: u32,
    /// The flame asks for supersampling, but this render was not made larger.
    pub supersample_skipped: bool,
}

impl PostOutcome {
    pub fn denoised(&self) -> bool {
        self.denoise.denoised()
    }

    /// "denoised with …" / "not denoised: …", plus "image effects applied"; only the latter
    /// when the flame has no denoiser.
    pub fn describe(&self) -> String {
        let mut denoise = self.denoise.describe();
        if self.supersampled > 1 {
            denoise.push_str(&format!(", supersampled ×{}", self.supersampled));
        } else if self.supersample_skipped && self.denoised() {
            denoise.push_str(", not supersampled (only saved images, videos and batch renders are)");
        }
        match (self.denoiser_off, self.effects) {
            (true, true) => "image effects applied".into(),
            (false, true) => format!("{denoise}; image effects applied"),
            _ => denoise,
        }
    }

    /// Whether there is anything worth reporting for `flame`: it was denoised, the flame asks
    /// for a denoiser, or effects were applied.
    pub fn worth_reporting(&self, flame: &Flame) -> bool {
        self.denoised() || self.effects || flame.ai_post_denoiser != denoise::Denoiser::None
    }
}

/// The factor by which a final image of `flame` is rendered larger (2 with the denoiser's
/// supersampling, when the denoiser runs for this render; else 1). Render at the final size
/// times this factor and pass it to [`process`].
pub fn supersample(flame: &Flame, gpu_render: bool) -> u32 {
    if flame.post_denoiser_supersample && denoise::skip_reason(flame, gpu_render).is_none() { 2 } else { 1 }
}

/// Denoises `rgba` (`width` × `height`, the rendered size) with the flame's post denoiser
/// (reading `source`, the unfiltered image, and the helper images `aux` if given; see
/// [`denoise::apply_post_denoiser_from`]), scales it down by `scale` (see [`supersample`]) and
/// applies the flame's image effects. Afterwards `rgba` has the final size.
#[allow(clippy::too_many_arguments)]
pub fn process(
    flame: &Flame,
    rgba: &mut Vec<u8>,
    source: Option<&[u8]>,
    width: u32,
    height: u32,
    gpu_render: bool,
    aux: Option<&DenoiseAux>,
    scale: u32,
) -> Result<PostOutcome, String> {
    let outcome = denoise::apply_post_denoiser_from(flame, rgba, source, width, height, gpu_render, aux)?;
    let scale = scale.max(1);
    if scale > 1 {
        *rgba = downscale(rgba, width, height, scale);
    }
    let (w, h) = (width / scale, height / scale);
    let effects = flame.effects.is_active();
    if effects {
        super::effects::apply(&flame.effects, rgba, w, h);
    }
    Ok(PostOutcome {
        denoise: outcome,
        effects,
        denoiser_off: flame.ai_post_denoiser == denoise::Denoiser::None,
        supersampled: scale,
        supersample_skipped: scale == 1 && flame.post_denoiser_supersample,
    })
}

/// The status note for an HDR file, which skips the post-processing.
pub fn hdr_note(flame: &Flame) -> String {
    let effects = if flame.effects.is_active() { ", no image effects" } else { "" };
    format!("not denoised{effects}: HDR images are saved as rendered")
}

/// Scales an RGBA8 image of `width` × `height` down by the integer `factor` (box average of
/// `factor` × `factor` pixels, alpha included); the size must be a multiple of `factor`.
pub fn downscale(rgba: &[u8], width: u32, height: u32, factor: u32) -> Vec<u8> {
    let (w, h, f) = (width as usize, height as usize, factor.max(1) as usize);
    let (ow, oh) = (w / f, h / f);
    let mut out = vec![0u8; ow * oh * 4];
    let n = (f * f) as u32;
    for oy in 0..oh {
        for ox in 0..ow {
            let mut sum = [0u32; 4];
            for dy in 0..f {
                let row = ((oy * f + dy) * w + ox * f) * 4;
                for px in rgba[row..row + f * 4].chunks_exact(4) {
                    for c in 0..4 {
                        sum[c] += px[c] as u32;
                    }
                }
            }
            let o = (oy * ow + ox) * 4;
            for c in 0..4 {
                out[o + c] = ((sum[c] + n / 2) / n) as u8;
            }
        }
    }
    out
}

/// [`downscale`] for a 16-bit gray image (the z-buffer).
pub fn downscale_gray16(gray: &[u16], width: u32, height: u32, factor: u32) -> Vec<u16> {
    let (w, h, f) = (width as usize, height as usize, factor.max(1) as usize);
    let (ow, oh) = (w / f, h / f);
    let n = (f * f) as u32;
    (0..ow * oh)
        .map(|i| {
            let (ox, oy) = (i % ow, i / ow);
            let sum: u32 = (0..f).flat_map(|dy| (0..f).map(move |dx| ((oy * f + dy) * w + ox * f + dx) as usize)).map(|k| gray[k] as u32).sum();
            ((sum + n / 2) / n) as u16
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_averages_blocks() {
        let img: Vec<u8> = (0..4 * 2 * 4).map(|i| (i * 8) as u8).collect();
        let out = downscale(&img, 4, 2, 2);
        assert_eq!(out.len(), 2 * 4);
        // Pixel 0: pixels 0, 1, 4, 5 of the 4 x 2 image.
        let avg = |c: usize| ((img[c] as u32 + img[4 + c] as u32 + img[16 + c] as u32 + img[20 + c] as u32 + 2) / 4) as u8;
        assert_eq!(&out[..4], &[avg(0), avg(1), avg(2), avg(3)]);
        assert_eq!(downscale_gray16(&[0, 4, 8, 12], 2, 2, 2), vec![6]);
    }

    #[test]
    fn supersampling_needs_a_running_denoiser() {
        let mut f = Flame { post_denoiser_supersample: true, ..Flame::default() };
        assert_eq!(supersample(&f, true), 1);
        f.ai_post_denoiser = denoise::Denoiser::Oidn;
        f.post_denoiser_blend = 0.0;
        assert_eq!(supersample(&f, true), 2);
        f.post_denoiser_only_for_cpu = true;
        assert_eq!((supersample(&f, true), supersample(&f, false)), (1, 2));
    }
}
