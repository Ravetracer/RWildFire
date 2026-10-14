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

//! Export dialog: renders the flame at full size and saves it (PNG, JPEG, HDR, EXR, z-buffer).

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use egui::RichText;
use rwf_core::Flame;
use rwf_core::output::ImageFormat;
use rwf_core::render::{CpuRender, RenderOptions};
use rwf_gpu::{GpuContext, GpuRenderer};

use crate::preview::Backend;
use crate::theme;

enum Job {
    Gpu { renderer: Box<GpuRenderer>, target: u64 },
    Cpu { render: Option<CpuRender>, saving: Option<Receiver<Result<(), String>>> },
}

struct Running {
    job: Job,
    save: SaveJob,
    started: Instant,
    target_density: f64,
}

/// A running animation export (background thread).
struct AnimJob {
    done: std::sync::Arc<std::sync::atomic::AtomicU32>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    total: u32,
    result: Receiver<Result<String, String>>,
    started: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Image,
    Animation,
}

pub struct ExportDialog {
    pub open: bool,
    mode: Mode,
    from: u32,
    to: u32,
    format: rwf_core::video::VideoFormat,
    ffmpeg: Option<bool>,
    anim: Option<AnimJob>,
    width: u32,
    height: u32,
    quality: f64,
    backend: Backend,
    transparent: bool,
    image_format: ImageFormat,
    jpeg_quality: u8,
    zbuffer: bool,
    running: Option<Running>,
    status: Option<String>,
}

impl Default for ExportDialog {
    fn default() -> Self {
        Self {
            open: false,
            mode: Mode::Image,
            from: 0,
            to: 99,
            format: rwf_core::video::VideoFormat::Mp4,
            ffmpeg: None,
            anim: None,
            width: 1920,
            height: 1080,
            quality: 1000.0,
            backend: Backend::Gpu,
            transparent: false,
            image_format: ImageFormat::Png,
            jpeg_quality: 95,
            zbuffer: false,
            running: None,
            status: None,
        }
    }
}

/// Runs the flame's AI post denoiser, if any (`apply_post_denoiser`).
fn denoise(flame: &Flame, img: &mut [u8], w: u32, h: u32, gpu: bool) -> Result<(), String> {
    rwf_core::render::denoise::apply_post_denoiser(flame, img, w, h, gpu).map(|_| ()).map_err(|e| format!("denoiser: {e}"))
}

/// Saves a finished CPU render: image in the chosen format plus the optional z-buffer.
fn save_cpu(r: &CpuRender, run: &SaveJob) -> Result<(), String> {
    let (w, h) = (run.width, run.height);
    if run.format.is_hdr() {
        rwf_core::output::write_float_rgb(&run.path, run.format, w, h, &r.image_hdr())?;
    } else {
        let mut img = r.image(run.with_alpha);
        denoise(&run.flame, &mut img, w, h, false)?;
        rwf_core::output::write_rgba8(&run.path, run.format, w, h, &img, run.jpeg_quality)?;
    }
    if run.zbuffer
        && let Some(z) = r.zbuffer_image()
    {
        rwf_core::output::write_gray16_png(&run.flame.zbuffer_filename.path_for(&run.path), w, h, &z)?;
    }
    Ok(())
}

/// Saves a finished GPU render: image in the chosen format plus the optional z-buffer.
fn save_gpu(r: &mut GpuRenderer, run: &SaveJob) -> Result<(), String> {
    let (w, h) = (run.width, run.height);
    if run.format.is_hdr() {
        let rgb = r.read_hdr().ok_or("readback failed")?;
        rwf_core::output::write_float_rgb(&run.path, run.format, w, h, &rgb)?;
    } else {
        r.tonemap();
        let mut img = r.read_image().ok_or("readback failed")?;
        denoise(&run.flame, &mut img, w, h, true)?;
        rwf_core::output::write_rgba8(&run.path, run.format, w, h, &img, run.jpeg_quality)?;
    }
    if run.zbuffer
        && let Some(z) = r.read_zbuffer()
    {
        rwf_core::output::write_gray16_png(&run.flame.zbuffer_filename.path_for(&run.path), w, h, &z)?;
    }
    Ok(())
}

/// Output settings of a running export.
#[derive(Clone)]
struct SaveJob {
    path: PathBuf,
    width: u32,
    height: u32,
    with_alpha: bool,
    format: ImageFormat,
    jpeg_quality: u8,
    zbuffer: bool,
    flame: Flame,
}

impl ExportDialog {
    pub fn open_for(&mut self, flame: &Flame, gpu: bool) {
        self.open = true;
        self.width = flame.width;
        self.height = flame.height;
        self.quality = flame.sample_density.max(200.0);
        self.transparent = flame.bg_transparency;
        self.mode = if flame.animation.is_animated() { Mode::Animation } else { Mode::Image };
        self.from = 0;
        self.to = flame.animation.frame_count.saturating_sub(1);
        if self.ffmpeg.is_none() {
            self.ffmpeg = Some(rwf_core::video::ffmpeg_available());
        }
        if self.ffmpeg == Some(false) {
            self.format = rwf_core::video::VideoFormat::PngSequence;
        }
        if !gpu {
            self.backend = Backend::Cpu;
        }
        self.status = None;
    }


    fn start(&mut self, flame: &Flame, gpu: Option<GpuContext>, path: PathBuf) {
        // Only PNG keeps an alpha channel.
        let alpha = self.transparent && self.image_format == ImageFormat::Png;
        let mut f = flame.clone();
        f.bg_transparency = alpha;
        let job = match (self.backend, gpu) {
            (Backend::Gpu, Some(ctx)) => {
                let mut r = GpuRenderer::new(ctx);
                r.set_with_alpha(alpha);
                r.set_zbuffer(self.zbuffer);
                r.set_flame(&f, self.width, self.height);
                if let Some(e) = r.last_error() {
                    self.status = Some(format!("GPU error: {e}"));
                    return;
                }
                let v = r.view().cloned().expect("view after set_flame");
                let target = (self.quality * v.raster_width as f64 * v.raster_height as f64 / v.oversample as f64) as u64;
                Job::Gpu { renderer: Box::new(r), target }
            }
            _ => {
                let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
                let render = CpuRender::start(
                    &f,
                    RenderOptions { width: self.width, height: self.height, threads, max_density: Some(self.quality), seed: 0x5EED, zbuffer: self.zbuffer },
                );
                Job::Cpu { render: Some(render), saving: None }
            }
        };
        self.running = Some(Running {
            job,
            save: SaveJob {
                path,
                width: self.width,
                height: self.height,
                with_alpha: alpha,
                format: self.image_format,
                jpeg_quality: self.jpeg_quality,
                zbuffer: self.zbuffer,
                flame: f,
            },
            started: Instant::now(),
            target_density: self.quality,
        });
        self.status = None;
    }

    /// Advances a running export; returns progress 0..1.
    fn step(&mut self, ctx: &egui::Context) -> f32 {
        let Some(run) = &mut self.running else { return 0.0 };
        ctx.request_repaint();
        match &mut run.job {
            Job::Gpu { renderer, target } => {
                if renderer.samples() < *target {
                    renderer.iterate(2, 256);
                    renderer.wait_idle();
                    return (renderer.samples() as f64 / *target as f64) as f32;
                }
                let result = save_gpu(renderer, &run.save);
                self.finish(result);
                1.0
            }
            Job::Cpu { render, saving } => {
                if let Some(rx) = saving {
                    if let Ok(result) = rx.try_recv() {
                        self.finish(result);
                        return 1.0;
                    }
                    return 0.99;
                }
                let r = render.as_ref().expect("cpu render");
                if !r.is_finished() {
                    return (r.density() / run.target_density).min(0.98) as f32;
                }
                let r = render.take().expect("cpu render");
                let (tx, rx) = channel();
                let job = run.save.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(save_cpu(&r, &job));
                });
                *saving = Some(rx);
                0.99
            }
        }
    }

    fn finish(&mut self, result: Result<(), String>) {
        if let Some(run) = self.running.take() {
            self.status = Some(match result {
                Ok(()) => format!("Saved {} ({:.1}s)", run.save.path.display(), run.started.elapsed().as_secs_f64()),
                Err(e) => format!("Export failed: {e}"),
            });
        }
    }

    fn start_animation(&mut self, flame: &Flame, path: PathBuf, gpu: bool) {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
        let (from, to) = (self.from.min(self.to), self.to.max(self.from));
        let total = to - from + 1;
        let done = Arc::new(AtomicU32::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let (w, h, q, format) = (self.width, self.height, self.quality, self.format);
        let mut flame = flame.clone();
        flame.bg_transparency = false;
        let (d2, c2) = (done.clone(), cancel.clone());
        std::thread::spawn(move || {
            let result = (|| -> Result<String, String> {
                let mut sink = rwf_core::video::VideoSink::new(&path, format, w, h, flame.animation.fps).map_err(|e| e.to_string())?;
                // Own headless device so the editor stays responsive.
                let mut gpu_r = if gpu { Some(GpuRenderer::new(GpuContext::new_headless()?)) } else { None };
                for (i, frame) in (from..=to).enumerate() {
                    if c2.load(Ordering::Relaxed) {
                        let _ = sink.finish();
                        return Err("Cancelled".into());
                    }
                    let mut img = match &mut gpu_r {
                        Some(r) => r.render_frame(&flame, frame as f64, w, h, q)?,
                        None => {
                            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
                            CpuRender::start_frame(&flame, frame as f64, RenderOptions { width: w, height: h, threads, max_density: Some(q), seed: 0x5EED + frame as u64, zbuffer: false })
                                .wait()
                                .image(false)
                        }
                    };
                    denoise(&flame, &mut img, w, h, gpu_r.is_some())?;
                    sink.write_frame(i as u32, &img).map_err(|e| e.to_string())?;
                    d2.fetch_add(1, Ordering::Relaxed);
                }
                sink.finish().map_err(|e| e.to_string())?;
                Ok(path.display().to_string())
            })();
            let _ = tx.send(result);
        });
        self.anim = Some(AnimJob { done, cancel, total, result: rx, started: Instant::now() });
        self.status = None;
    }

    pub fn show(&mut self, ctx: &egui::Context, flame: &Flame, gpu: Option<GpuContext>) {
        if let Some(job) = &self.anim {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            if let Ok(r) = job.result.try_recv() {
                let secs = job.started.elapsed().as_secs_f64();
                self.status = Some(match r {
                    Ok(p) => format!("Saved {p} ({:.1}s)", secs),
                    Err(e) => format!("Animation export: {e}"),
                });
                self.anim = None;
            }
        }
        if !self.open {
            return;
        }
        let progress = if self.running.is_some() { Some(self.step(ctx)) } else { None };
        let mut open = self.open;
        egui::Window::new("Export image").open(&mut open).collapsible(false).resizable(false).default_width(340.0).show(ctx, |ui| {
            let busy = self.running.is_some() || self.anim.is_some();
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.mode, Mode::Image, "Image");
                    ui.selectable_value(&mut self.mode, Mode::Animation, "Animation");
                });
                ui.add_space(4.0);
                egui::Grid::new("export").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label(RichText::new("Size").color(theme::TEXT_MUTED));
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut self.width).range(16..=16384));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut self.height).range(16..=16384));
                    });
                    ui.end_row();
                    ui.label(RichText::new("Presets").color(theme::TEXT_MUTED));
                    ui.horizontal_wrapped(|ui| {
                        let aspect = flame.height as f64 / flame.width.max(1) as f64;
                        for (l, w) in [("HD", 1280u32), ("Full HD", 1920), ("QHD", 2560), ("4K", 3840), ("8K", 7680)] {
                            if ui.small_button(l).clicked() {
                                self.width = w;
                                self.height = (w as f64 * aspect).round() as u32;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label(RichText::new("Quality").color(theme::TEXT_MUTED));
                    ui.add(egui::Slider::new(&mut self.quality, 50.0..=20000.0).logarithmic(true));
                    ui.end_row();
                    ui.label(RichText::new("Renderer").color(theme::TEXT_MUTED));
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(gpu.is_some(), |ui| {
                            ui.selectable_value(&mut self.backend, Backend::Gpu, "GPU (fast)");
                        });
                        ui.selectable_value(&mut self.backend, Backend::Cpu, "CPU (exact)");
                    });
                    ui.end_row();
                    if self.mode == Mode::Image {
                        ui.label(RichText::new("Format").color(theme::TEXT_MUTED));
                        egui::ComboBox::from_id_salt("ifmt").selected_text(self.image_format.label()).show_ui(ui, |ui| {
                            for f in ImageFormat::ALL {
                                ui.selectable_value(&mut self.image_format, *f, f.label());
                            }
                        });
                        ui.end_row();
                        if self.image_format == ImageFormat::Jpeg {
                            ui.label(RichText::new("JPEG quality").color(theme::TEXT_MUTED));
                            ui.add(egui::Slider::new(&mut self.jpeg_quality, 50..=100));
                            ui.end_row();
                        }
                        if matches!(self.image_format, ImageFormat::Png) {
                            ui.label(RichText::new("Background").color(theme::TEXT_MUTED));
                            ui.checkbox(&mut self.transparent, "Transparent");
                            ui.end_row();
                        }
                        ui.label(RichText::new("Depth").color(theme::TEXT_MUTED));
                        ui.checkbox(&mut self.zbuffer, "Z-buffer (16-bit PNG)").on_hover_text(format!(
                            "Saved as {}",
                            flame.zbuffer_filename.path_for(std::path::Path::new(&format!("image.{}", self.image_format.extension()))).display()
                        ));
                        ui.end_row();
                    } else {
                        let last = flame.animation.frame_count.saturating_sub(1);
                        ui.label(RichText::new("Frames").color(theme::TEXT_MUTED));
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.from).range(0..=last));
                            ui.label("–");
                            ui.add(egui::DragValue::new(&mut self.to).range(0..=last));
                            ui.label(RichText::new(format!("@ {} fps", flame.animation.fps)).color(theme::TEXT_MUTED));
                        });
                        ui.end_row();
                        ui.label(RichText::new("Format").color(theme::TEXT_MUTED));
                        egui::ComboBox::from_id_salt("vfmt").selected_text(self.format.label()).show_ui(ui, |ui| {
                            for f in rwf_core::video::VideoFormat::ALL {
                                let ok = *f == rwf_core::video::VideoFormat::PngSequence || self.ffmpeg == Some(true);
                                ui.add_enabled_ui(ok, |ui| {
                                    ui.selectable_value(&mut self.format, *f, f.label());
                                });
                            }
                        });
                        ui.end_row();
                        if self.ffmpeg == Some(false) {
                            ui.label("");
                            ui.label(RichText::new("Install ffmpeg for video files").small().color(theme::WARN));
                            ui.end_row();
                        }
                        if flame.animation.motion_blur_length > 0 {
                            ui.label("");
                            ui.label(RichText::new(format!("Motion blur: {} sub-frames", flame.animation.motion_blur_length)).small().color(theme::TEXT_MUTED));
                            ui.end_row();
                        }
                    }
                });
            });
            ui.add_space(8.0);
            if let Some(job) = &self.anim {
                let d = job.done.load(std::sync::atomic::Ordering::Relaxed);
                ui.add(egui::ProgressBar::new(d as f32 / job.total.max(1) as f32).text(format!("Frame {d} / {}", job.total)));
                if ui.button("Cancel").clicked() {
                    job.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            } else if let Some(p) = progress {
                ui.add(egui::ProgressBar::new(p).show_percentage());
            } else if self.mode == Mode::Animation {
                if ui.add_sized([ui.available_width(), 30.0], egui::Button::new(RichText::new("Render animation…").strong()).fill(theme::ACCENT_DIM)).clicked() {
                    let target = match self.format.extension() {
                        Some(ext) => rfd::FileDialog::new().add_filter(self.format.label(), &[ext]).set_file_name(format!("animation.{ext}")).save_file(),
                        None => rfd::FileDialog::new().set_title("Folder for the PNG frames").pick_folder(),
                    };
                    if let Some(path) = target {
                        self.start_animation(flame, path, self.backend == Backend::Gpu && gpu.is_some());
                    }
                }
            } else if ui
                .add_sized([ui.available_width(), 30.0], egui::Button::new(RichText::new(format!("Render & save {}…", self.image_format.label())).strong()).fill(theme::ACCENT_DIM))
                .clicked()
            {
                let ext = self.image_format.extension();
                let default_name = format!("{}.{ext}", if flame.name.is_empty() { "flame" } else { &flame.name });
                if let Some(path) = rfd::FileDialog::new().add_filter(self.image_format.label(), &[ext]).set_file_name(default_name).save_file() {
                    self.start(flame, gpu.clone(), path);
                }
            }
            if let Some(s) = &self.status {
                ui.add_space(4.0);
                ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
            }
        });
        if !open && self.running.is_none() {
            self.open = false;
        }
    }
}
