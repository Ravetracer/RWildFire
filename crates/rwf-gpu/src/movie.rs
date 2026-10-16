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

//! Renders a [`Movie`] (parts, morphs, scripts, motion blur) into a video file or a PNG
//! sequence.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rwf_core::movie::Movie;
use rwf_core::render::{CpuRender, RenderOptions};
use rwf_core::video::{VideoFormat, VideoSink};

use crate::{GpuContext, GpuRenderer};

/// Renders frame `frame` of `movie` (RGBA8), on the GPU if `gpu` is given.
pub fn render_movie_frame(movie: &Movie, frame: u32, gpu: Option<&mut GpuRenderer>) -> Result<Vec<u8>, String> {
    let packets = movie.packets(frame as f64);
    let first = packets.first().ok_or("the movie has no parts")?;
    let (w, h, q) = (movie.width.max(1), movie.height.max(1), movie.quality.max(1.0));
    let s = rwf_core::render::post::supersample(first, gpu.is_some());
    let img = match gpu {
        Some(r) => {
            r.accumulate_packets(&packets, w * s, h * s, q)?;
            r.tonemap();
            let mut img = r.read_image().ok_or("readback failed")?;
            r.apply_post_processing(first, &mut img, w * s, h * s, s).map_err(|e| format!("denoiser: {e}"))?;
            img
        }
        None => {
            let threads = rwf_core::render::cpu_threads();
            let opts = RenderOptions { width: w * s, height: h * s, threads, max_density: Some(q), seed: 0x5EED + frame as u64, zbuffer: false };
            let r = CpuRender::start_packets(&packets, opts).wait();
            let mut img = r.image(false);
            r.apply_post_processing(&mut img, false, s).map_err(|e| format!("denoiser: {e}"))?;
            img
        }
    };
    Ok(img)
}

/// Renders all frames into `path` (a video file, or a folder for [`VideoFormat::PngSequence`]).
/// `gpu`: render on this device (own renderer), otherwise on the CPU. `on_frame(index, total,
/// rgba)` is called after every frame; stops early (with an error) when `cancel` is set.
pub fn render_movie(
    movie: &Movie,
    path: &Path,
    format: VideoFormat,
    gpu: Option<GpuContext>,
    cancel: &AtomicBool,
    mut on_frame: impl FnMut(u32, u32, &[u8]),
) -> Result<(), String> {
    let total = movie.frame_count();
    if total == 0 {
        return Err("the movie has no frames".into());
    }
    let mut sink = VideoSink::new(path, format, movie.width.max(1), movie.height.max(1), movie.fps).map_err(|e| e.to_string())?;
    let mut renderer = gpu.map(GpuRenderer::new);
    for frame in 0..total {
        if cancel.load(Ordering::Relaxed) {
            let _ = sink.finish();
            return Err("Cancelled".into());
        }
        let img = render_movie_frame(movie, frame, renderer.as_mut())?;
        sink.write_frame(frame, &img).map_err(|e| e.to_string())?;
        on_frame(frame, total, &img);
    }
    sink.finish().map_err(|e| e.to_string())
}
