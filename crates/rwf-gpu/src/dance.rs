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

//! Renders the recorded performance of a dance project into a video (with the music as audio
//! track) or a PNG sequence.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rwf_core::dance::{DanceProject, Spectrum};
use rwf_core::render::{CpuRender, RenderOptions};
use rwf_core::video::{VideoFormat, VideoSink};

use crate::{GpuContext, GpuRenderer};

/// Renders frame `frame` of the performance (RGBA8), on the GPU if `gpu` is given.
pub fn render_performance_frame(
    project: &DanceProject,
    spectrum: Option<&Spectrum>,
    frame: u32,
    gpu: Option<&mut GpuRenderer>,
) -> Result<Vec<u8>, String> {
    let flame = project.performance_flame(spectrum, frame).ok_or("no recorded performance")?;
    let (w, h, q) = (project.width.max(1), project.height.max(1), project.quality.max(1.0));
    let img = match gpu {
        Some(r) => {
            r.accumulate_packets(std::slice::from_ref(&flame), w, h, q)?;
            r.tonemap();
            let mut img = r.read_image().ok_or("readback failed")?;
            r.apply_post_denoiser(&flame, &mut img, w, h).map_err(|e| format!("denoiser: {e}"))?;
            img
        }
        None => {
            let threads = rwf_core::render::cpu_threads();
            let opts = RenderOptions { width: w, height: h, threads, max_density: Some(q), seed: 0xDA2CE + frame as u64, zbuffer: false };
            let r = CpuRender::start(&flame, opts).wait();
            let mut img = r.image(false);
            r.apply_post_denoiser(&mut img, false).map_err(|e| format!("denoiser: {e}"))?;
            img
        }
    };
    Ok(img)
}

/// Renders the whole performance into `path`. `on_frame(index, total, rgba)` is called after
/// every frame; stops early (with an error) when `cancel` is set.
pub fn render_performance(
    project: &DanceProject,
    spectrum: Option<&Spectrum>,
    path: &Path,
    format: VideoFormat,
    gpu: Option<GpuContext>,
    cancel: &AtomicBool,
    mut on_frame: impl FnMut(u32, u32, &[u8]),
) -> Result<(), String> {
    let total = project.performance_frames();
    if total == 0 {
        return Err("nothing recorded yet".into());
    }
    let audio = project.sound.as_deref().filter(|p| p.exists()).map(|p| (p, project.performance.start));
    let mut sink = VideoSink::with_audio(path, format, project.width.max(1), project.height.max(1), project.fps, audio).map_err(|e| e.to_string())?;
    let mut renderer = gpu.map(GpuRenderer::new);
    for frame in 0..total {
        if cancel.load(Ordering::Relaxed) {
            let _ = sink.finish();
            return Err("Cancelled".into());
        }
        let img = render_performance_frame(project, spectrum, frame, renderer.as_mut())?;
        sink.write_frame(frame, &img).map_err(|e| e.to_string())?;
        on_frame(frame, total, &img);
    }
    sink.finish().map_err(|e| e.to_string())
}
