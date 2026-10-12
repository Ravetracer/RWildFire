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

pub struct ExportDialog {
    pub open: bool,
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
        Self { open: false, width: 1920, height: 1080, quality: 1000.0, backend: Backend::Gpu, transparent: false, running: None, status: None }
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

    pub fn show(&mut self, ctx: &egui::Context, flame: &Flame, gpu: Option<GpuContext>) {
        if !self.open {
            return;
        }
        let progress = if self.running.is_some() { Some(self.step(ctx)) } else { None };
        let mut open = self.open;
        egui::Window::new("Export image").open(&mut open).collapsible(false).resizable(false).default_width(340.0).show(ctx, |ui| {
            let busy = self.running.is_some();
            ui.add_enabled_ui(!busy, |ui| {
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
                    ui.label(RichText::new("Background").color(theme::TEXT_MUTED));
                    ui.checkbox(&mut self.transparent, "Transparent");
                    ui.end_row();
                });
            });
            ui.add_space(8.0);
            if let Some(p) = progress {
                ui.add(egui::ProgressBar::new(p).show_percentage());
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
