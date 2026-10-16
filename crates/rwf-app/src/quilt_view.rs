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

//! Quilt renderer tab of the render workspace: very large images rendered tile by tile on a
//! background thread (own GPU device), shown as a growing mosaic.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use egui::{Color32, Rect, RichText, Stroke, Ui, pos2, vec2};
use rwf_core::Flame;
use rwf_gpu::quilt::{DEFAULT_TILE_SIZE, QuiltEvent, QuiltJob, TileRect, render_quilt, suggest_tiles};
use rwf_gpu::{GpuContext, GpuRenderer};

use crate::theme;
use crate::ui::widgets::{self, grid};

/// Long side of the mosaic preview.
const MOSAIC_SIZE: u32 = 1600;
const PREVIEW_INTERVAL: Duration = Duration::from_secs(2);

/// Pixels for the mosaic: a tile scaled down, placed at `(x, y)` of the mosaic.
struct Patch {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

enum Msg {
    Progress {
        tile: usize,
        tiles: usize,
        fraction: f32,
    },
    /// `finished`: a whole tile (`reused`: from an earlier run), otherwise a running tile.
    Patch {
        patch: Patch,
        finished: bool,
        reused: bool,
    },
    Merging,
    Done(Result<PathBuf, String>),
}

struct Worker {
    cancel: Arc<AtomicBool>,
    msgs: Receiver<Msg>,
    started: Instant,
}

pub struct QuiltView {
    flame: Option<Flame>,
    width: u32,
    height: u32,
    auto_tiles: bool,
    cols: u32,
    rows: u32,
    quality: f64,
    output: Option<PathBuf>,
    worker: Option<Worker>,
    /// Current tile, tile count and its progress.
    progress: Option<(usize, usize, f32)>,
    tiles_done: usize,
    /// Tiles rendered (not reused) in this run, for the time estimate.
    tiles_rendered: usize,
    merging: bool,
    /// Mosaic of the finished tiles (scaled down) and its texture.
    mosaic: Option<(u32, u32, Vec<u8>)>,
    texture: Option<egui::TextureHandle>,
    texture_dirty: bool,
    status: Option<String>,
}

impl Default for QuiltView {
    fn default() -> Self {
        Self {
            flame: None,
            width: 15360,
            height: 8640,
            auto_tiles: true,
            cols: 4,
            rows: 3,
            quality: 1000.0,
            output: None,
            worker: None,
            progress: None,
            tiles_done: 0,
            tiles_rendered: 0,
            merging: false,
            mosaic: None,
            texture: None,
            texture_dirty: false,
            status: None,
        }
    }
}

fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// Mosaic size for an image.
fn mosaic_size(w: u32, h: u32) -> (u32, u32, f64) {
    let s = (MOSAIC_SIZE as f64 / w.max(h) as f64).min(1.0);
    (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1), s)
}

/// Scales a tile (RGBA8, `rect` of the full image) down into its mosaic patch.
fn patch(rect: &TileRect, rgba: &[u8], scale: f64) -> Patch {
    let x0 = (rect.x as f64 * scale).round() as u32;
    let y0 = (rect.y as f64 * scale).round() as u32;
    let x1 = (((rect.x + rect.w) as f64 * scale).round() as u32).max(x0 + 1);
    let y1 = (((rect.y + rect.h) as f64 * scale).round() as u32).max(y0 + 1);
    let (w, h) = (x1 - x0, y1 - y0);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let sy = (((y as f64 + 0.5) / h as f64 * rect.h as f64) as u32).min(rect.h - 1);
        for x in 0..w {
            let sx = (((x as f64 + 0.5) / w as f64 * rect.w as f64) as u32).min(rect.w - 1);
            let i = ((sy * rect.w + sx) * 4) as usize;
            out.extend_from_slice(&rgba[i..i + 4]);
        }
    }
    Patch { x: x0, y: y0, w, h, rgba: out }
}

fn spawn_worker(job: QuiltJob) -> Worker {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    let c = cancel.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<PathBuf, String> {
            let mut r = GpuRenderer::new(GpuContext::new_headless()?);
            let scale = mosaic_size(job.width, job.height).2;
            let tiles = job.tiles();
            let mut last_progress = Instant::now();
            let mut last_preview = Instant::now();
            render_quilt(&mut r, &job, &mut |e, r: &mut GpuRenderer| {
                match e {
                    QuiltEvent::Progress { tile, tiles: n, fraction } => {
                        if last_progress.elapsed() >= Duration::from_millis(100) {
                            let _ = tx.send(Msg::Progress { tile, tiles: n, fraction });
                            last_progress = Instant::now();
                        }
                        // A look at the running tile (rendered with margins: crop them).
                        if last_preview.elapsed() >= PREVIEW_INTERVAL {
                            last_preview = Instant::now();
                            let t = &tiles[tile];
                            r.tonemap();
                            if let (Some(img), Some((pw, _))) = (r.read_image(), r.output_size()) {
                                let m = job.margin();
                                let (ox, oy) = (t.x.min(m) as usize, t.y.min(m) as usize);
                                let mut core = Vec::with_capacity(t.w as usize * t.h as usize * 4);
                                for y in 0..t.h as usize {
                                    let start = ((oy + y) * pw as usize + ox) * 4;
                                    core.extend_from_slice(&img[start..start + t.w as usize * 4]);
                                }
                                let _ = tx.send(Msg::Patch { patch: patch(t, &core, scale), finished: false, reused: false });
                            }
                        }
                    }
                    QuiltEvent::TileDone { rect, rgba, reused } => {
                        let _ = tx.send(Msg::Patch { patch: patch(&rect, rgba, scale), finished: true, reused });
                        last_preview = Instant::now();
                    }
                    QuiltEvent::Merging => {
                        let _ = tx.send(Msg::Merging);
                    }
                }
                !c.load(Ordering::Relaxed)
            })
        })();
        let _ = tx.send(Msg::Done(result));
    });
    Worker { cancel, msgs: rx, started: Instant::now() }
}

impl QuiltView {
    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    /// Takes the editor's flame the first time the tab is shown.
    pub fn sync_flame(&mut self, doc: &crate::doc::Document) {
        if self.worker.is_none() && self.flame.is_none() {
            self.take_flame(doc);
        }
    }

    /// Takes the editor's flame; the output defaults to `<flame file>_quilt.png`.
    fn take_flame(&mut self, doc: &crate::doc::Document) {
        let flame = &doc.flame;
        if let Some(p) = &doc.path {
            let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            self.output = Some(p.with_file_name(format!("{stem}_quilt.png")));
        }
        let aspect = flame.height.max(1) as f64 / flame.width.max(1) as f64;
        self.height = ((self.width as f64 * aspect).round() as u32).max(16);
        self.flame = Some(flame.clone());
        self.mosaic = None;
        self.texture = None;
    }

    fn job(&self) -> Option<QuiltJob> {
        Some(QuiltJob {
            flame: self.flame.clone()?,
            width: self.width,
            height: self.height,
            cols: self.cols,
            rows: self.rows,
            quality: self.quality,
            output: self.output.clone()?,
        })
    }

    fn start(&mut self) {
        let Some(job) = self.job() else { return };
        if let Err(e) = job.check() {
            self.status = Some(e);
            return;
        }
        let (mw, mh, _) = mosaic_size(job.width, job.height);
        self.mosaic = Some((mw, mh, vec![0; (mw * mh * 4) as usize]));
        self.texture_dirty = true;
        self.tiles_done = 0;
        self.tiles_rendered = 0;
        self.merging = false;
        self.progress = None;
        self.status = None;
        self.worker = Some(spawn_worker(job));
    }

    /// Handles the worker's messages (every frame, also while the tab is hidden).
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(w) = &self.worker else { return };
        ctx.request_repaint_after(Duration::from_millis(100));
        let msgs: Vec<Msg> = w.msgs.try_iter().collect();
        for m in msgs {
            match m {
                Msg::Progress { tile, tiles, fraction } => self.progress = Some((tile, tiles, fraction)),
                Msg::Patch { patch, finished, reused } => {
                    if finished {
                        self.tiles_done += 1;
                        self.tiles_rendered += !reused as usize;
                    }
                    if let Some((mw, _, buf)) = &mut self.mosaic {
                        for y in 0..patch.h as usize {
                            let dst = ((patch.y as usize + y) * *mw as usize + patch.x as usize) * 4;
                            let src = y * patch.w as usize * 4;
                            let n = (patch.w as usize * 4).min(buf.len().saturating_sub(dst));
                            buf[dst..dst + n].copy_from_slice(&patch.rgba[src..src + n]);
                        }
                        self.texture_dirty = true;
                    }
                }
                Msg::Merging => self.merging = true,
                Msg::Done(result) => {
                    let secs = self.worker.as_ref().map_or(0.0, |w| w.started.elapsed().as_secs_f64());
                    self.status = Some(match result {
                        Ok(p) => format!("Saved {} ({})", p.display(), format_duration(Duration::from_secs_f64(secs))),
                        Err(e) if e == "cancelled" => "Stopped. The finished tiles are kept: Start continues the quilt.".into(),
                        Err(e) => format!("Quilt failed: {e}"),
                    });
                    self.worker = None;
                    self.progress = None;
                    self.merging = false;
                    return;
                }
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, doc: &crate::doc::Document, gpu: bool) {
        egui::Panel::right("quilt-panel")
            .resizable(true)
            .default_size(350.0)
            .size_range(330.0..=520.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().slider_width = 120.0;
                    self.panel(ui, doc, gpu);
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::CANVAS)).show(ui, |ui| {
            self.mosaic_view(ui);
        });
    }

    fn panel(&mut self, ui: &mut Ui, doc: &crate::doc::Document, gpu: bool) {
        let running = self.is_running();
        theme::section_label(ui, "Flame");
        ui.label(RichText::new(self.flame.as_ref().map_or("—", |f| if f.name.is_empty() { "Untitled" } else { f.name.as_str() })).color(theme::TEXT));
        if ui.add_enabled(!running, egui::Button::new("From editor")).on_hover_text("Render the flame of the editor").clicked() {
            self.take_flame(doc);
        }

        theme::section_label(ui, "Image");
        let aspect = self.flame.as_ref().map_or(9.0 / 16.0, |f| f.height.max(1) as f64 / f.width.max(1) as f64);
        ui.add_enabled_ui(!running, |ui| {
            grid(ui, "quilt-size", |ui| {
                widgets::label(ui, "Size");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.width).range(16..=200_000));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut self.height).range(16..=200_000));
                });
                ui.end_row();
                widgets::label(ui, "Presets");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (l, w) in [("8K", 7680u32), ("16K", 15360), ("24K", 23040), ("32K", 30720)] {
                        if ui.small_button(l).on_hover_text(format!("{w} pixels wide")).clicked() {
                            self.width = w;
                            self.height = (w as f64 * aspect).round() as u32;
                        }
                    }
                });
                ui.end_row();
                widgets::label(ui, "Tiles");
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.auto_tiles, "Auto");
                    ui.add_enabled_ui(!self.auto_tiles, |ui| {
                        ui.add(egui::DragValue::new(&mut self.cols).range(1..=64));
                        ui.label("×");
                        ui.add(egui::DragValue::new(&mut self.rows).range(1..=64));
                    });
                });
                ui.end_row();
                widgets::label(ui, "Quality");
                ui.add(egui::Slider::new(&mut self.quality, 50.0..=20000.0).logarithmic(true).max_decimals(0));
                ui.end_row();
            });
        });
        if self.auto_tiles {
            (self.cols, self.rows) = suggest_tiles(self.width, self.height, DEFAULT_TILE_SIZE);
        }
        let n = self.cols * self.rows;
        let info = format!(
            "{} tiles of about {} × {} pixels. Every tile is iterated like a full render of the image, so a quilt takes about {n} times as long.",
            n,
            self.width.div_ceil(self.cols),
            self.height.div_ceil(self.rows)
        );
        ui.label(RichText::new(info).small().color(theme::TEXT_MUTED));
        if let Some(Err(e)) = self.job().map(|j| j.check()) {
            ui.label(RichText::new(e).small().color(theme::WARN));
        }

        theme::section_label(ui, "Output");
        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Choose…").clicked() {
                    let name = format!("{}.png", self.flame.as_ref().filter(|f| !f.name.is_empty()).map_or("quilt", |f| f.name.as_str()));
                    if let Some(p) = crate::settings::dialog(crate::settings::Folder::Images).add_filter("PNG", &["png"]).set_file_name(name).save_file() {
                        self.output = Some(p.with_extension("png"));
                    }
                }
            });
            let path = self.output.as_ref().map_or("PNG file not chosen".into(), |p| p.display().to_string());
            ui.add(egui::Label::new(RichText::new(&path).small().color(theme::TEXT_MUTED)).truncate()).on_hover_text(path);
        });

        ui.add_space(10.0);
        if running {
            let stopping = self.worker.as_ref().is_some_and(|w| w.cancel.load(Ordering::Relaxed));
            let label = if stopping { "Stopping…" } else { "Stop" };
            if ui
                .add_enabled(
                    !stopping && !self.merging,
                    egui::Button::new(RichText::new(format!("{}  {label}", crate::icons::PAUSE)).strong()).min_size(vec2(ui.available_width(), 30.0)),
                )
                .clicked()
                && let Some(w) = &self.worker
            {
                w.cancel.store(true, Ordering::Relaxed);
            }
        } else if ui
            .add_enabled(
                gpu && self.flame.is_some() && self.output.is_some(),
                egui::Button::new(RichText::new(format!("{}  Render quilt", crate::icons::PLAY)).strong())
                    .fill(theme::ACCENT_DIM)
                    .min_size(vec2(ui.available_width(), 30.0)),
            )
            .on_hover_text("Finished tiles of an earlier, stopped run of the same quilt are reused")
            .clicked()
        {
            self.start();
        }
        if !gpu {
            ui.label(RichText::new("The quilt renderer needs a GPU.").small().color(theme::WARN));
        }

        if let Some(w) = &self.worker {
            theme::section_label(ui, "Progress");
            let elapsed = w.started.elapsed();
            grid(ui, "quilt-progress", |ui| {
                widgets::label(ui, "Tiles");
                ui.label(format!("{} / {n}", self.tiles_done));
                ui.end_row();
                widgets::label(ui, "Elapsed");
                ui.label(format_duration(elapsed));
                ui.end_row();
                if let Some((_, tiles, f)) = self.progress {
                    // Speed from the tiles rendered in this run (reused tiles took no time).
                    let work = self.tiles_rendered as f64 + f as f64;
                    let left = (tiles - self.tiles_done.min(tiles)) as f64 - f as f64;
                    if work > 0.05 {
                        widgets::label(ui, "Remaining");
                        ui.label(format!("about {}", format_duration(Duration::from_secs_f64(elapsed.as_secs_f64() / work * left.max(0.0)))));
                        ui.end_row();
                    }
                }
            });
            let overall = match self.progress {
                Some((tile, tiles, f)) => (tile as f32 + f) / tiles.max(1) as f32,
                None => self.tiles_done as f32 / n.max(1) as f32,
            };
            ui.add(egui::ProgressBar::new(if self.merging { 1.0 } else { overall }).text(if self.merging {
                "Writing the PNG…".to_string()
            } else {
                format!("{:.0} %", overall * 100.0)
            }));
        }
        if let Some(s) = &self.status {
            ui.add_space(6.0);
            ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
        }
    }

    fn mosaic_view(&mut self, ui: &mut Ui) {
        let rect = ui.max_rect();
        ui.allocate_rect(rect, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let Some((mw, mh, buf)) = &self.mosaic else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Choose a size and an output file, then render the quilt",
                egui::FontId::proportional(theme::fs(15.0)),
                theme::TEXT_MUTED,
            );
            return;
        };
        if self.texture_dirty {
            let img = egui::ColorImage::from_rgba_unmultiplied([*mw as usize, *mh as usize], buf);
            match &mut self.texture {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.texture = Some(ui.ctx().load_texture("quilt-mosaic", img, egui::TextureOptions::LINEAR)),
            }
            self.texture_dirty = false;
        }
        let Some(tex) = &self.texture else { return };
        let size = vec2(*mw as f32, *mh as f32);
        let scale = ((rect.width() - 32.0) / size.x).min((rect.height() - 32.0) / size.y).max(0.01);
        let img = Rect::from_center_size(rect.center(), size * scale);
        painter.rect_filled(img, 0.0, Color32::BLACK);
        painter.image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        // Tile grid, the running tile highlighted.
        if let Some(job) = self.job() {
            let (sx, sy) = (img.width() / job.width as f32, img.height() / job.height as f32);
            for (i, t) in job.tiles().iter().enumerate() {
                let r = Rect::from_min_size(img.min + vec2(t.x as f32 * sx, t.y as f32 * sy), vec2(t.w as f32 * sx, t.h as f32 * sy));
                let running = self.worker.is_some() && self.progress.is_some_and(|(tile, ..)| tile == i);
                let stroke = if running { Stroke::new(2.0, theme::ACCENT) } else { Stroke::new(1.0, Color32::from_white_alpha(28)) };
                painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Inside);
            }
        }
    }
}
