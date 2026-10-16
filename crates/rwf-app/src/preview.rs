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

//! Progressive preview rendering on the GPU (default) or CPU.

use std::time::{Duration, Instant};

use rwf_core::Flame;
use rwf_core::render::{CpuRender, RenderOptions, same_iteration};
use rwf_gpu::{GpuContext, GpuRenderer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Gpu,
    Cpu,
}

pub struct Preview {
    pub backend: Backend,
    /// Accumulation stops at this sample density.
    pub target_quality: f64,
    /// ... or after this many seconds of rendering one state (0: no limit).
    pub max_seconds: f64,
    /// When the current render (re)started, and its sample count last frame.
    run_start: Instant,
    run_samples: u64,
    gpu: Option<GpuRenderer>,
    gpu_error: Option<String>,
    gpu_texture: Option<(egui::TextureId, u64)>,
    /// Image effects of the GPU preview, and the egui texture of their output.
    effects: Option<rwf_gpu::effects::EffectsPass>,
    effects_texture: Option<(egui::TextureId, u64)>,
    gpu_iters: u32,
    cpu: Option<CpuRender>,
    cpu_texture: Option<egui::TextureHandle>,
    cpu_last_upload: Instant,
    cpu_uploaded_samples: u64,
    cpu_final_uploaded: bool,
    last_frame: Instant,
    size: (u32, u32),
    samples_per_sec: f64,
    rate_window: (Instant, u64),
}

pub struct PreviewImage {
    pub texture: egui::TextureId,
    pub size: (u32, u32),
}

impl Preview {
    pub fn new(render_state: Option<&egui_wgpu::RenderState>) -> Self {
        let (gpu, gpu_error) = match render_state {
            Some(rs) => {
                let ctx = GpuContext {
                    device: rs.device.clone(),
                    queue: rs.queue.clone(),
                    adapter_name: rs.adapter.get_info().name,
                };
                (Some(GpuRenderer::new(ctx)), None)
            }
            None => (None, Some("wgpu render state unavailable".to_string())),
        };
        Self {
            backend: if gpu.is_some() { Backend::Gpu } else { Backend::Cpu },
            target_quality: 400.0,
            max_seconds: 0.0,
            run_start: Instant::now(),
            run_samples: 0,
            gpu,
            gpu_error,
            gpu_texture: None,
            effects: None,
            effects_texture: None,
            gpu_iters: 32,
            cpu: None,
            cpu_texture: None,
            cpu_last_upload: Instant::now(),
            cpu_uploaded_samples: 0,
            cpu_final_uploaded: false,
            last_frame: Instant::now(),
            size: (0, 0),
            samples_per_sec: 0.0,
            rate_window: (Instant::now(), 0),
        }
    }

    pub fn gpu_available(&self) -> bool {
        self.gpu.is_some()
    }

    pub fn adapter_name(&self) -> Option<&str> {
        self.gpu.as_ref().map(|g| g.context().adapter_name.as_str())
    }

    pub fn gpu_context(&self) -> Option<GpuContext> {
        self.gpu.as_ref().map(|g| g.context().clone())
    }

    pub fn error(&self) -> Option<&str> {
        match self.backend {
            Backend::Gpu => self.gpu.as_ref().and_then(|g| g.last_error()).or(self.gpu_error.as_deref()),
            Backend::Cpu => None,
        }
    }

    pub fn density(&self) -> f64 {
        match self.backend {
            Backend::Gpu => self.gpu.as_ref().map(|g| g.density()).unwrap_or(0.0),
            Backend::Cpu => self.cpu.as_ref().map(|c| c.density()).unwrap_or(0.0),
        }
    }

    pub fn samples_per_sec(&self) -> f64 {
        self.samples_per_sec
    }

    pub fn unsupported(&self) -> Vec<String> {
        match self.backend {
            Backend::Gpu => self.gpu.as_ref().map(|g| g.unsupported_variations().to_vec()).unwrap_or_default(),
            Backend::Cpu => self.cpu.as_ref().map(|c| c.missing_variations().to_vec()).unwrap_or_default(),
        }
    }

    /// The current preview image (RGBA8) with its size, and whether it was rendered on the GPU.
    pub fn read_image(&self) -> Option<(Vec<u8>, u32, u32, bool)> {
        match self.backend {
            Backend::Gpu if self.gpu.is_some() => {
                let g = self.gpu.as_ref()?;
                let (w, h) = g.output_size()?;
                Some((g.read_image()?, w, h, true))
            }
            _ => {
                let r = self.cpu.as_ref()?;
                let v = r.view();
                Some((r.image(false), v.image_width, v.image_height, false))
            }
        }
    }

    /// The denoiser's input for the shown image: the unfiltered image, the helper images (GPU)
    /// and the sample counts.
    pub fn read_denoise_input(&mut self, flame: &Flame) -> (Option<Vec<u8>>, Option<rwf_core::render::denoise::DenoiseAux>) {
        match self.backend {
            Backend::Gpu if self.gpu.is_some() => self.gpu.as_mut().map_or((None, None), |g| g.read_denoise_input(flame)),
            _ => {
                let active = rwf_core::render::denoise::skip_reason(flame, false).is_none();
                let r = self.cpu.as_ref().filter(|_| active);
                (r.and_then(|r| r.image_unfiltered(false)), r.and_then(|r| r.denoise_aux()))
            }
        }
    }

    pub fn is_converged(&self) -> bool {
        self.density() >= self.target_quality || self.timed_out()
    }

    /// The time limit stopped the render before it reached the target quality.
    pub fn timed_out(&self) -> bool {
        self.max_seconds > 0.0 && self.run_start.elapsed().as_secs_f64() > self.max_seconds
    }

    /// Restarts the time limit when the render started over.
    fn track_restart(&mut self, samples: u64) {
        if samples == 0 || samples < self.run_samples {
            self.run_start = Instant::now();
        }
        self.run_samples = samples;
    }


    fn track_rate(&mut self, samples: u64) {
        let (t0, s0) = self.rate_window;
        let dt = t0.elapsed().as_secs_f64();
        if dt > 0.5 {
            self.samples_per_sec = (samples.saturating_sub(s0)) as f64 / dt;
            self.rate_window = (Instant::now(), samples);
        } else if samples < s0 {
            self.rate_window = (Instant::now(), samples);
        }
    }

    /// Advances rendering for this frame and returns the image to show.
    pub fn update(&mut self, ctx: &egui::Context, frame: &eframe::Frame, flame: &Flame, size: (u32, u32)) -> Option<PreviewImage> {
        let size = (size.0.clamp(16, 4096), size.1.clamp(16, 4096));
        self.size = size;
        let dt = self.last_frame.elapsed();
        self.last_frame = Instant::now();
        match self.backend {
            Backend::Gpu if self.gpu.is_some() => {
                self.cpu = None;
                self.update_gpu(ctx, frame, flame, size, dt)
            }
            _ => self.update_cpu(ctx, flame, size),
        }
    }

    fn update_gpu(&mut self, ctx: &egui::Context, frame: &eframe::Frame, flame: &Flame, size: (u32, u32), dt: Duration) -> Option<PreviewImage> {
        let target = self.target_quality;
        let samples = {
            let gpu = self.gpu.as_mut()?;
            gpu.set_flame(flame, size.0, size.1);
            if gpu.last_error().is_some() {
                return None;
            }
            gpu.samples()
        };
        self.track_restart(samples);
        let timed_out = self.timed_out();
        let gpu = self.gpu.as_mut()?;
        // Adaptive work per frame: aim for roughly 60 fps while accumulating.
        if gpu.density() < target && !timed_out {
            let ms = dt.as_secs_f64() * 1000.0;
            if ms < 14.0 {
                self.gpu_iters = (self.gpu_iters + self.gpu_iters / 4 + 1).min(2048);
            } else if ms > 24.0 {
                // Scale with the measured frame time so expensive flames (e.g. metaballs3d_wf)
                // get back to an interactive frame rate within a frame or two.
                self.gpu_iters = ((self.gpu_iters as f64 * 16.0 / ms) as u32).min(self.gpu_iters * 3 / 4).max(1);
            }
            gpu.iterate(1, self.gpu_iters);
            gpu.tonemap();
            ctx.request_repaint();
        } else {
            // Converged: still tonemap so tone-only edits show up.
            gpu.tonemap();
        }
        let samples = gpu.samples();
        let generation = gpu.output_generation();
        let view = gpu.output_view()?.clone();
        let out_size = gpu.output_size()?;
        let gpu_ctx = gpu.context().clone();
        self.track_rate(samples);

        let rs = frame.wgpu_render_state()?;
        if flame.effects.is_active() {
            let pass = self.effects.get_or_insert_with(|| rwf_gpu::effects::EffectsPass::new(&gpu_ctx.device));
            if pass.run(&gpu_ctx, &view, out_size.0, out_size.1, &flame.effects) {
                let (fx_view, fx_gen) = (pass.output_view()?.clone(), pass.generation());
                let mut renderer = rs.renderer.write();
                match self.effects_texture {
                    Some((_, g)) if g == fx_gen => {}
                    Some((id, _)) => {
                        renderer.update_egui_texture_from_wgpu_texture(&rs.device, &fx_view, wgpu::FilterMode::Linear, id);
                        self.effects_texture = Some((id, fx_gen));
                    }
                    None => self.effects_texture = Some((renderer.register_native_texture(&rs.device, &fx_view, wgpu::FilterMode::Linear), fx_gen)),
                }
                return Some(PreviewImage { texture: self.effects_texture?.0, size: out_size });
            }
        }
        let mut renderer = rs.renderer.write();
        match self.gpu_texture {
            Some((id, g)) if g == generation => {
                let _ = id;
            }
            Some((id, _)) => {
                renderer.update_egui_texture_from_wgpu_texture(&rs.device, &view, wgpu::FilterMode::Linear, id);
                self.gpu_texture = Some((id, generation));
            }
            None => {
                let id = renderer.register_native_texture(&rs.device, &view, wgpu::FilterMode::Linear);
                self.gpu_texture = Some((id, generation));
            }
        }
        Some(PreviewImage { texture: self.gpu_texture?.0, size: out_size })
    }

    fn update_cpu(&mut self, ctx: &egui::Context, flame: &Flame, size: (u32, u32)) -> Option<PreviewImage> {
        let needs_restart = match &self.cpu {
            None => true,
            Some(r) => {
                let v = r.view();
                (v.image_width, v.image_height) != size || !same_iteration(r.flame(), flame)
            }
        };
        if needs_restart {
            let threads = crate::settings::cpu_threads().saturating_sub(1).max(1);
            self.cpu = Some(CpuRender::start(
                flame,
                RenderOptions { width: size.0, height: size.1, threads, max_density: Some(self.target_quality), seed: 7, zbuffer: false },
            ));
            self.cpu_uploaded_samples = u64::MAX;
            self.cpu_final_uploaded = false;
            self.cpu_last_upload = Instant::now() - Duration::from_secs(1);
            self.run_start = Instant::now();
        } else if let Some(r) = &mut self.cpu
            && r.flame() != flame
        {
            r.update_tone(flame);
            self.cpu_uploaded_samples = u64::MAX;
            self.cpu_final_uploaded = false;
        }
        let samples = self.cpu.as_ref()?.samples();
        self.track_rate(samples);
        if self.timed_out() {
            self.cpu.as_ref()?.stop();
        }
        let r = self.cpu.as_ref()?;
        let interval = if samples < 2_000_000 { 60 } else { 250 };
        let finished = r.is_finished();
        let needs_final = finished && !self.cpu_final_uploaded;
        if (samples != self.cpu_uploaded_samples || needs_final) && self.cpu_last_upload.elapsed() >= Duration::from_millis(interval) {
            // Density estimation is costly on the CPU: only for the converged image.
            let mut img = if finished { r.image(false) } else { r.image_fast(false) };
            rwf_core::render::effects::apply(&flame.effects, &mut img, size.0, size.1);
            self.cpu_final_uploaded = finished;
            let color = egui::ColorImage::from_rgba_unmultiplied([size.0 as usize, size.1 as usize], &img);
            match &mut self.cpu_texture {
                Some(t) => t.set(color, egui::TextureOptions::LINEAR),
                None => self.cpu_texture = Some(ctx.load_texture("cpu-preview", color, egui::TextureOptions::LINEAR)),
            }
            self.cpu_uploaded_samples = samples;
            self.cpu_last_upload = Instant::now();
        }
        if !finished || samples != self.cpu_uploaded_samples || !self.cpu_final_uploaded {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        let t = self.cpu_texture.as_ref()?;
        let [w, h] = t.size();
        Some(PreviewImage { texture: t.id(), size: (w as u32, h as u32) })
    }
}
