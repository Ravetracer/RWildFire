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

//! Stereo 3D still images on the GPU or CPU (see [`rwf_core::stereo`]).

use rwf_core::Flame;
use rwf_core::render::{CpuRender, RenderOptions};
use rwf_core::stereo::render_stereo;

use crate::{GpuContext, GpuRenderer};

/// A rendered stereo image: RGBA8 pixels, size, and what the denoiser did.
pub struct StereoImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub denoise_note: String,
}

/// Renders `flame` as a stereo image with `w` × `h` per eye: on `gpu`, else on the CPU with
/// `threads`. Every eye image gets the flame's post denoiser before the images are combined.
/// `progress` is called with (eye images done, eye images total).
pub fn render_stereo_image(flame: &Flame, w: u32, h: u32, quality: f64, gpu: Option<GpuContext>, threads: usize, progress: &mut dyn FnMut(u32, u32)) -> Result<StereoImage, String> {
    let total = flame.stereo3d.image_count();
    let mut done = 0;
    let mut note = String::new();
    let mut gpu_r = gpu.map(GpuRenderer::new);
    let alpha = flame.bg_transparency;
    let mut render_eye = |f: &Flame, q: f64| -> Result<Vec<u8>, String> {
        let mut img = match gpu_r.as_mut() {
            Some(r) => {
                r.set_with_alpha(alpha);
                r.accumulate_packets(std::slice::from_ref(f), w, h, q)?;
                r.tonemap();
                let mut img = r.read_image().ok_or("readback failed")?;
                let outcome = r.apply_post_denoiser(f, &mut img, w, h).map_err(|e| format!("denoiser: {e}"))?;
                note = outcome.describe();
                img
            }
            None => {
                let r = CpuRender::start(f, RenderOptions { width: w, height: h, threads, max_density: Some(q), seed: 0x5EED + done as u64, zbuffer: false }).wait();
                let mut img = r.image(alpha);
                let outcome = r.apply_post_denoiser(&mut img, alpha).map_err(|e| format!("denoiser: {e}"))?;
                note = outcome.describe();
                img
            }
        };
        img.truncate(w as usize * h as usize * 4);
        done += 1;
        progress(done, total);
        Ok(img)
    };
    let (rgba, width, height) = render_stereo(flame, w, h, quality, &mut render_eye)?;
    Ok(StereoImage { rgba, width, height, denoise_note: format!("stereo {} ({width} × {height}); {note}", flame.stereo3d.mode.label().to_lowercase()) })
}
