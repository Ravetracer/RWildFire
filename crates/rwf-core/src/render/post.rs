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
}

impl PostOutcome {
    pub fn denoised(&self) -> bool {
        self.denoise.denoised()
    }

    /// "denoised with …" / "not denoised: …", plus "image effects applied"; only the latter
    /// when the flame has no denoiser.
    pub fn describe(&self) -> String {
        match (self.denoiser_off, self.effects) {
            (true, true) => "image effects applied".into(),
            (false, true) => format!("{}; image effects applied", self.denoise.describe()),
            _ => self.denoise.describe(),
        }
    }

    /// Whether there is anything worth reporting for `flame`: it was denoised, the flame asks
    /// for a denoiser, or effects were applied.
    pub fn worth_reporting(&self, flame: &Flame) -> bool {
        self.denoised() || self.effects || flame.ai_post_denoiser != denoise::Denoiser::None
    }
}

/// Denoises `rgba` with the flame's post denoiser (reading `source`, the unfiltered image, and
/// the helper images `aux` if given; see [`denoise::apply_post_denoiser_from`]) and applies the
/// flame's image effects.
pub fn process(
    flame: &Flame,
    rgba: &mut [u8],
    source: Option<&[u8]>,
    width: u32,
    height: u32,
    gpu_render: bool,
    aux: Option<&DenoiseAux>,
) -> Result<PostOutcome, String> {
    let outcome = denoise::apply_post_denoiser_from(flame, rgba, source, width, height, gpu_render, aux)?;
    let effects = flame.effects.is_active();
    if effects {
        super::effects::apply(&flame.effects, rgba, width, height);
    }
    Ok(PostOutcome { denoise: outcome, effects, denoiser_off: flame.ai_post_denoiser == denoise::Denoiser::None })
}

/// The status note for an HDR file, which skips the post-processing.
pub fn hdr_note(flame: &Flame) -> String {
    let effects = if flame.effects.is_active() { ", no image effects" } else { "" };
    format!("not denoised{effects}: HDR images are saved as rendered")
}
