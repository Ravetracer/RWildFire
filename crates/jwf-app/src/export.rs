//! Export dialog: renders the flame at full size and saves a PNG.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use egui::RichText;
use jwf_core::Flame;
use jwf_core::render::{CpuRender, RenderOptions};
use jwf_gpu::{GpuContext, GpuRenderer};

use crate::preview::Backend;
use crate::theme;

enum Job {
    Gpu { renderer: Box<GpuRenderer>, target: u64 },
    Cpu { render: Option<CpuRender>, saving: Option<Receiver<Result<(), String>>> },
}

struct Running {
    job: Job,
    path: PathBuf,
    width: u32,
    height: u32,
    with_alpha: bool,
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
    format: jwf_core::video::VideoFormat,
    ffmpeg: Option<bool>,
    anim: Option<AnimJob>,
    width: u32,
    height: u32,
    quality: f64,
    backend: Backend,
    transparent: bool,
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
            format: jwf_core::video::VideoFormat::Mp4,
            ffmpeg: None,
            anim: None,
            width: 1920,
            height: 1080,
            quality: 1000.0,
            backend: Backend::Gpu,
            transparent: false,
            running: None,
            status: None,
        }
    }
}

pub fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    Ok(())
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
            self.ffmpeg = Some(jwf_core::video::ffmpeg_available());
        }
        if self.ffmpeg == Some(false) {
            self.format = jwf_core::video::VideoFormat::PngSequence;
        }
        if !gpu {
            self.backend = Backend::Cpu;
        }
        self.status = None;
    }


    fn start(&mut self, flame: &Flame, gpu: Option<GpuContext>, path: PathBuf) {
        let mut f = flame.clone();
        f.bg_transparency = self.transparent;
        let job = match (self.backend, gpu) {
            (Backend::Gpu, Some(ctx)) => {
                let mut r = GpuRenderer::new(ctx);
                r.set_with_alpha(self.transparent);
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
                    RenderOptions { width: self.width, height: self.height, threads, max_density: Some(self.quality), seed: 0x5EED },
                );
                Job::Cpu { render: Some(render), saving: None }
            }
        };
        self.running = Some(Running {
            job,
            path,
            width: self.width,
            height: self.height,
            with_alpha: self.transparent,
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
                renderer.tonemap();
                let result = renderer
                    .read_image()
                    .ok_or_else(|| "readback failed".to_string())
                    .and_then(|img| write_png(&run.path, run.width, run.height, &img));
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
                let (path, w, h, alpha) = (run.path.clone(), run.width, run.height, run.with_alpha);
                std::thread::spawn(move || {
                    let img = r.image(alpha);
                    let _ = tx.send(write_png(&path, w, h, &img));
                });
                *saving = Some(rx);
                0.99
            }
        }
    }

    fn finish(&mut self, result: Result<(), String>) {
        if let Some(run) = self.running.take() {
            self.status = Some(match result {
                Ok(()) => format!("Saved {} ({:.1}s)", run.path.display(), run.started.elapsed().as_secs_f64()),
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
                let mut sink = jwf_core::video::VideoSink::new(&path, format, w, h, flame.animation.fps).map_err(|e| e.to_string())?;
                // Own headless device so the editor stays responsive.
                let mut gpu_r = if gpu { Some(GpuRenderer::new(GpuContext::new_headless()?)) } else { None };
                for (i, frame) in (from..=to).enumerate() {
                    if c2.load(Ordering::Relaxed) {
                        let _ = sink.finish();
                        return Err("Cancelled".into());
                    }
                    let img = match &mut gpu_r {
                        Some(r) => r.render_frame(&flame, frame as f64, w, h, q)?,
                        None => {
                            let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
                            CpuRender::start_frame(&flame, frame as f64, RenderOptions { width: w, height: h, threads, max_density: Some(q), seed: 0x5EED + frame as u64 })
                                .wait()
                                .image(false)
                        }
                    };
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
                        ui.label(RichText::new("Background").color(theme::TEXT_MUTED));
                        ui.checkbox(&mut self.transparent, "Transparent");
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
                            for f in jwf_core::video::VideoFormat::ALL {
                                let ok = *f == jwf_core::video::VideoFormat::PngSequence || self.ffmpeg == Some(true);
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
            } else if ui.add_sized([ui.available_width(), 30.0], egui::Button::new(RichText::new("Render & save PNG…").strong()).fill(theme::ACCENT_DIM)).clicked() {
                let default_name = if flame.name.is_empty() { "flame.png".to_string() } else { format!("{}.png", flame.name) };
                if let Some(path) = rfd::FileDialog::new().add_filter("PNG image", &["png"]).set_file_name(default_name).save_file() {
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
