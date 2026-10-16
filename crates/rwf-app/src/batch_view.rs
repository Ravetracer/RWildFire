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

//! Batch renderer tab of the render workspace: a queue of flames rendered one after another
//! on a background thread (own GPU device, so the editor stays responsive).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Ui, vec2};
use rwf_core::Flame;
use rwf_core::output::ImageFormat;
use rwf_core::video::VideoFormat;
use rwf_gpu::batch::{BatchJob, BatchSettings, JobOutcome, SizeMode};
use rwf_gpu::{GpuContext, GpuRenderer};

use crate::theme;
use crate::ui::widgets::{self, grid};

/// Long side of the preview images sent by the worker.
const PREVIEW_SIZE: u32 = 640;
const PREVIEW_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, PartialEq, Debug)]
enum JobState {
    Pending,
    Running(f32),
    Done,
    Skipped,
    Failed,
}

struct Entry {
    id: u64,
    job: BatchJob,
    /// Size and animation of the flame when it was added (for the list).
    flame_size: (u32, u32),
    frames: Option<u32>,
    state: JobState,
    elapsed: Option<Duration>,
    message: Option<String>,
    output: Option<PathBuf>,
}

/// Messages from the worker thread.
enum Msg {
    /// Ready for the next job.
    Next,
    Progress(u64, f32),
    Preview(u64, egui::ColorImage),
    Finished(u64, Result<JobOutcome, String>, Duration),
    Failed(String),
}

struct Worker {
    cancel: Arc<AtomicBool>,
    jobs: Sender<Option<(u64, BatchJob, BatchSettings)>>,
    msgs: Receiver<Msg>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SizeKind {
    Flame,
    LongEdge,
    Fixed,
}

pub struct BatchView {
    entries: Vec<Entry>,
    next_id: u64,
    selected: Option<u64>,
    settings: BatchSettings,
    size_kind: SizeKind,
    long_edge: u32,
    fixed: (u32, u32),
    worker: Option<Worker>,
    preview: Option<(egui::TextureHandle, String)>,
    ffmpeg: Option<bool>,
    status: Option<String>,
}

impl Default for BatchView {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            selected: None,
            settings: BatchSettings::default(),
            size_kind: SizeKind::LongEdge,
            long_edge: 1920,
            fixed: (1920, 1080),
            worker: None,
            preview: None,
            ffmpeg: None,
            status: None,
        }
    }
}

fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// Box-filters an RGBA8 image so its long side is at most `max`.
fn downscale(rgba: &[u8], w: u32, h: u32, max: u32) -> egui::ColorImage {
    let f = w.max(h).div_ceil(max).max(1);
    let (dw, dh) = ((w / f).max(1), (h / f).max(1));
    let mut out = Vec::with_capacity((dw * dh * 4) as usize);
    for y in 0..dh {
        for x in 0..dw {
            let mut sum = [0u32; 4];
            for sy in y * f..((y + 1) * f).min(h) {
                for sx in x * f..((x + 1) * f).min(w) {
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += rgba[i + c] as u32;
                    }
                }
            }
            out.extend(sum.map(|v| (v / (f * f)) as u8));
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([dw as usize, dh as usize], &out)
}

fn spawn_worker() -> Worker {
    let cancel = Arc::new(AtomicBool::new(false));
    let (job_tx, job_rx) = channel::<Option<(u64, BatchJob, BatchSettings)>>();
    let (msg_tx, msg_rx) = channel();
    let c = cancel.clone();
    std::thread::spawn(move || {
        // Own device: long batches must not stall the editor's frames.
        let ctx = match GpuContext::new_headless() {
            Ok(ctx) => ctx,
            Err(e) => {
                let _ = msg_tx.send(Msg::Failed(e));
                return;
            }
        };
        let mut r = GpuRenderer::new(ctx);
        while msg_tx.send(Msg::Next).is_ok() {
            let Ok(Some((id, job, settings))) = job_rx.recv() else { break };
            let t0 = Instant::now();
            let mut last_progress = Instant::now();
            let mut last_preview = Instant::now();
            let result = rwf_gpu::batch::render_job(&mut r, &job, &settings, &mut |p, r: &mut GpuRenderer| {
                if last_progress.elapsed() >= Duration::from_millis(100) {
                    let _ = msg_tx.send(Msg::Progress(id, p));
                    last_progress = Instant::now();
                }
                if last_preview.elapsed() >= PREVIEW_INTERVAL {
                    r.tonemap();
                    if let (Some(img), Some((w, h))) = (r.read_image(), r.output_size()) {
                        let _ = msg_tx.send(Msg::Preview(id, downscale(&img, w, h, PREVIEW_SIZE)));
                    }
                    last_preview = Instant::now();
                }
                !c.load(Ordering::Relaxed)
            });
            // A last preview of the finished image.
            if result.as_ref().is_ok_and(|o| !o.skipped)
                && let (Some(img), Some((w, h))) = (r.read_image(), r.output_size())
            {
                let _ = msg_tx.send(Msg::Preview(id, downscale(&img, w, h, PREVIEW_SIZE)));
            }
            let _ = msg_tx.send(Msg::Finished(id, result, t0.elapsed()));
            if c.load(Ordering::Relaxed) {
                break;
            }
        }
    });
    Worker { cancel, jobs: job_tx, msgs: msg_rx }
}

impl BatchView {
    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    fn push(&mut self, job: BatchJob, flame: &Flame) {
        let frames = flame.animation.is_animated().then_some(flame.animation.frame_count).filter(|n| *n > 1);
        self.entries.push(Entry {
            id: self.next_id,
            job,
            flame_size: (flame.width, flame.height),
            frames,
            state: JobState::Pending,
            elapsed: None,
            message: None,
            output: None,
        });
        self.next_id += 1;
    }

    /// Adds the editor's flame; unchanged flames of a file become file jobs (output next to
    /// the file).
    fn add_editor_flame(&mut self, doc: &crate::doc::Document) {
        if let Some(path) = &doc.path
            && let Some(i) = doc.library.iter().position(|f| *f == doc.flame)
            && let Ok(jobs) = BatchJob::from_file(path)
            && let Some(job) = jobs.into_iter().nth(i)
        {
            self.push(job, &doc.flame);
            return;
        }
        self.push(BatchJob::from_flame(doc.flame.clone()), &doc.flame);
    }

    /// Adds all flames of the given files (folders: their `.flame` files).
    pub fn add_paths(&mut self, paths: &[PathBuf]) {
        let mut added = 0;
        let mut errors = Vec::new();
        for p in paths {
            let files = if p.is_dir() {
                let mut files: Vec<PathBuf> = std::fs::read_dir(p)
                    .map(|d| {
                        d.filter_map(|e| e.ok().map(|e| e.path()))
                            .filter(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("flame")))
                            .collect()
                    })
                    .unwrap_or_default();
                files.sort();
                files
            } else {
                vec![p.clone()]
            };
            for f in files {
                match rwf_core::io::read_flames_from_file(&f) {
                    Ok(flames) => match BatchJob::from_file(&f) {
                        Ok(jobs) => {
                            for (job, flame) in jobs.into_iter().zip(&flames) {
                                self.push(job, flame);
                                added += 1;
                            }
                        }
                        Err(e) => errors.push(e),
                    },
                    Err(e) => errors.push(format!("{}: {e}", f.display())),
                }
            }
        }
        self.status = Some(match errors.first() {
            Some(e) => format!("Added {}; {} not readable ({e})", plural(added, "flame"), plural(errors.len(), "file")),
            None => format!("Added {}", plural(added, "flame")),
        });
    }

    fn selected_index(&self) -> Option<usize> {
        self.entries.iter().position(|e| Some(e.id) == self.selected)
    }

    fn start(&mut self) {
        for e in &mut self.entries {
            if e.state == JobState::Pending {
                e.message = None;
            }
        }
        self.worker = Some(spawn_worker());
        self.status = None;
    }

    /// Handles the worker's messages (every frame, also while the tab is hidden).
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(w) = &self.worker else { return };
        ctx.request_repaint_after(Duration::from_millis(100));
        let mut msgs = Vec::new();
        let disconnected = loop {
            match w.msgs.try_recv() {
                Ok(m) => msgs.push(m),
                Err(std::sync::mpsc::TryRecvError::Empty) => break false,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break true,
            }
        };
        let cancelled = w.cancel.load(Ordering::Relaxed);
        let mut finished = disconnected;
        for m in msgs {
            match m {
                Msg::Next => {
                    let next = if cancelled { None } else { self.entries.iter_mut().find(|e| e.state == JobState::Pending) };
                    let job = next.map(|e| {
                        e.state = JobState::Running(0.0);
                        e.message = None;
                        (e.id, e.job.clone(), self.settings.clone())
                    });
                    finished |= job.is_none();
                    if let Some(w) = &self.worker {
                        let _ = w.jobs.send(job);
                    }
                }
                Msg::Progress(id, p) => {
                    if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                        e.state = JobState::Running(p);
                    }
                }
                Msg::Preview(id, img) => {
                    let name = self.entries.iter().find(|e| e.id == id).map(|e| e.job.name.clone()).unwrap_or_default();
                    match &mut self.preview {
                        Some((t, n)) => {
                            t.set(img, egui::TextureOptions::LINEAR);
                            *n = name;
                        }
                        None => self.preview = Some((ctx.load_texture("batch-preview", img, egui::TextureOptions::LINEAR), name)),
                    }
                }
                Msg::Finished(id, result, elapsed) => {
                    if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                        e.elapsed = Some(elapsed);
                        match result {
                            Ok(o) => {
                                e.state = if o.skipped { JobState::Skipped } else { JobState::Done };
                                e.message = if o.skipped { Some("output exists".into()) } else { o.warning };
                                e.output = Some(o.path);
                            }
                            Err(msg) if msg == "cancelled" => {
                                e.state = JobState::Pending;
                                e.elapsed = None;
                                e.message = Some("cancelled".into());
                            }
                            Err(msg) => {
                                e.state = JobState::Failed;
                                e.message = Some(msg);
                            }
                        }
                    }
                }
                Msg::Failed(e) => {
                    self.status = Some(format!("GPU: {e}"));
                    finished = true;
                }
            }
        }
        if finished {
            self.worker = None;
            for e in &mut self.entries {
                if matches!(e.state, JobState::Running(_)) {
                    e.state = JobState::Pending;
                }
            }
            if self.status.is_none() {
                let n = |s: JobState| self.entries.iter().filter(|e| e.state == s).count();
                self.status =
                    Some(format!("Batch finished: {} rendered, {} skipped, {} failed", n(JobState::Done), n(JobState::Skipped), n(JobState::Failed)));
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, doc: &crate::doc::Document, gpu: bool, profiles: &crate::profiles::Profiles) {
        egui::Panel::right("batch-panel")
            .resizable(true)
            .default_size(350.0)
            .size_range(330.0..=520.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().slider_width = 120.0;
                    self.panel(ui, gpu, profiles);
                });
            });
        egui::Panel::bottom("batch-preview")
            .resizable(true)
            .default_size(300.0)
            .size_range(120.0..=800.0)
            .frame(egui::Frame::new().fill(theme::CANVAS).inner_margin(8))
            .show(ui, |ui| {
                self.preview_area(ui);
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| {
            self.queue(ui, doc);
        });
    }

    fn queue(&mut self, ui: &mut Ui, doc: &crate::doc::Document) {
        let running = self.is_running();
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Add files…", crate::icons::PLUS)).clicked()
                && let Some(files) = crate::settings::dialog(crate::settings::Folder::Flames).add_filter("Flame", &["flame", "xml"]).pick_files()
            {
                self.add_paths(&files);
            }
            if ui.button("Add folder…").on_hover_text("All .flame files of a folder").clicked()
                && let Some(dir) = crate::settings::dialog(crate::settings::Folder::Flames).pick_folder()
            {
                self.add_paths(&[dir]);
            }
            if ui.button("Add editor flame").clicked() {
                self.add_editor_flame(doc);
            }
            ui.separator();
            let sel = self.selected_index();
            let sel_idle = sel.is_some_and(|i| !matches!(self.entries[i].state, JobState::Running(_)));
            if ui.add_enabled(sel.is_some_and(|i| i > 0), egui::Button::new(crate::icons::ARROW_UP)).on_hover_text("Move up").clicked()
                && let Some(i) = sel
            {
                self.entries.swap(i, i - 1);
            }
            if ui
                .add_enabled(sel.is_some_and(|i| i + 1 < self.entries.len()), egui::Button::new(crate::icons::ARROW_DOWN))
                .on_hover_text("Move down")
                .clicked()
                && let Some(i) = sel
            {
                self.entries.swap(i, i + 1);
            }
            if ui.add_enabled(sel_idle, egui::Button::new(crate::icons::X)).on_hover_text("Remove").clicked()
                && let Some(i) = sel
            {
                self.entries.remove(i);
                self.selected = None;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let finished = self.entries.iter().any(|e| matches!(e.state, JobState::Done | JobState::Skipped | JobState::Failed));
                if ui.add_enabled(!running && !self.entries.is_empty(), egui::Button::new("Clear")).on_hover_text("Remove all jobs").clicked() {
                    self.entries.clear();
                    self.selected = None;
                }
                if ui.add_enabled(finished, egui::Button::new("Remove finished")).clicked() {
                    self.entries.retain(|e| matches!(e.state, JobState::Pending | JobState::Running(_)));
                }
                if ui.add_enabled(finished, egui::Button::new("Render again")).on_hover_text("Queue the finished and failed jobs again").clicked() {
                    for e in &mut self.entries {
                        if matches!(e.state, JobState::Done | JobState::Skipped | JobState::Failed) {
                            e.state = JobState::Pending;
                            e.message = None;
                        }
                    }
                }
            });
        });
        ui.add_space(6.0);
        if self.entries.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("No jobs yet").color(theme::TEXT_MUTED).size(theme::fs(15.0)));
                ui.label(RichText::new("Add flame files or folders, drop files here, or add the editor's flame").color(theme::TEXT_MUTED));
            });
            return;
        }
        let s = self.settings.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("batch-jobs").num_columns(6).striped(true).spacing([14.0, 6.0]).min_col_width(24.0).show(ui, |ui| {
                for h in ["", "Flame", "Size", "Quality", "State", "Time"] {
                    ui.label(RichText::new(h).small().color(theme::TEXT_MUTED));
                }
                ui.end_row();
                for (i, e) in self.entries.iter().enumerate() {
                    ui.label(RichText::new(format!("{}", i + 1)).color(theme::TEXT_MUTED));
                    let name = match e.frames {
                        Some(n) if s.animations => format!("{}  {} {n}", e.job.name, crate::icons::FILM_STRIP),
                        _ => e.job.name.clone(),
                    };
                    let tip = match &e.job.source {
                        rwf_gpu::batch::JobSource::File { path, .. } => path.display().to_string(),
                        rwf_gpu::batch::JobSource::Flame(_) => "Flame from the editor".into(),
                    };
                    if ui.selectable_label(self.selected == Some(e.id), name).on_hover_text(tip).clicked() {
                        self.selected = Some(e.id);
                    }
                    let (w, h) = e.job.size_for(e.flame_size.0, e.flame_size.1, &s);
                    let custom = |c: bool| if c { theme::ACCENT } else { theme::TEXT };
                    ui.label(RichText::new(format!("{w} × {h}")).color(custom(e.job.width.is_some() || e.job.height.is_some())));
                    ui.label(RichText::new(format!("{:.0}", e.job.quality(&s))).color(custom(e.job.quality.is_some())));
                    match e.state {
                        JobState::Running(p) => {
                            ui.add(egui::ProgressBar::new(p).desired_width(140.0).desired_height(14.0).show_percentage());
                        }
                        st => {
                            let (text, color) = match st {
                                JobState::Pending => ("Queued", theme::TEXT_MUTED),
                                JobState::Done => ("Done", Color32::from_rgb(0x6c, 0xc4, 0x7e)),
                                JobState::Skipped => ("Skipped", theme::TEXT_MUTED),
                                _ => ("Failed", Color32::from_rgb(0xe5, 0x5b, 0x5b)),
                            };
                            let r = ui.label(RichText::new(text).color(color));
                            if let Some(m) = &e.message {
                                r.on_hover_text(m);
                            }
                        }
                    }
                    ui.label(RichText::new(e.elapsed.map(format_duration).unwrap_or_default()).color(theme::TEXT_MUTED));
                    ui.end_row();
                }
            });
        });
    }

    fn preview_area(&self, ui: &mut Ui) {
        // Claim the whole area, so the resizable panel keeps its size.
        let rect = ui.available_rect_before_wrap();
        ui.allocate_rect(rect, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let Some((tex, name)) = &self.preview else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "The running job is shown here",
                egui::FontId::proportional(theme::fs(14.0)),
                theme::TEXT_MUTED,
            );
            return;
        };
        let size = tex.size_vec2();
        let area = rect.shrink2(vec2(4.0, 10.0)).with_min_y(rect.top() + 20.0);
        let scale = (area.width() / size.x).min(area.height() / size.y).clamp(0.01, 2.0);
        let img = egui::Rect::from_center_size(area.center(), size * scale);
        painter.text(
            egui::pos2(rect.center().x, rect.top() + 2.0),
            egui::Align2::CENTER_TOP,
            name,
            egui::FontId::proportional(theme::fs(12.0)),
            theme::TEXT_MUTED,
        );
        painter.image(tex.id(), img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }

    fn panel(&mut self, ui: &mut Ui, gpu: bool, profiles: &crate::profiles::Profiles) {
        let running = self.is_running();
        theme::section_label(ui, "Defaults");
        ui.add_enabled_ui(!running, |ui| {
            grid(ui, "batch-defaults", |ui| {
                widgets::label(ui, "Profiles");
                ui.horizontal(|ui| {
                    let fixed = if self.size_kind == SizeKind::Fixed { self.fixed } else { (0, 0) };
                    if let Some(r) = crate::profiles::resolution_combo(ui, "batch-res-profile", profiles, fixed) {
                        self.size_kind = SizeKind::Fixed;
                        self.fixed = (r.width, r.height);
                    }
                    let current = profiles.qualities.iter().find(|q| q.quality == self.settings.quality).map(|q| q.caption.as_str());
                    if let Some(q) = crate::profiles::quality_combo(ui, "batch-qual-profile", profiles, current) {
                        self.settings.quality = q.quality;
                        self.settings.zbuffer = q.zbuffer;
                    }
                })
                .response
                .on_hover_text("Resolution and quality profiles (Preferences): set the size and quality below");
                ui.end_row();
                widgets::label(ui, "Size");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.size_kind, SizeKind::LongEdge, "Long edge");
                    ui.selectable_value(&mut self.size_kind, SizeKind::Fixed, "Fixed");
                    ui.selectable_value(&mut self.size_kind, SizeKind::Flame, "Flame");
                });
                ui.end_row();
                match self.size_kind {
                    SizeKind::LongEdge => {
                        widgets::label(ui, "");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.long_edge).range(16..=rwf_gpu::batch::MAX_SIZE).suffix(" px"));
                            for (l, n) in [("FHD", 1920u32), ("4K", 3840), ("8K", 7680)] {
                                if ui.small_button(l).clicked() {
                                    self.long_edge = n;
                                }
                            }
                        });
                        ui.end_row();
                    }
                    SizeKind::Fixed => {
                        widgets::label(ui, "");
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.fixed.0).range(16..=rwf_gpu::batch::MAX_SIZE));
                            ui.label("×");
                            ui.add(egui::DragValue::new(&mut self.fixed.1).range(16..=rwf_gpu::batch::MAX_SIZE));
                        });
                        ui.end_row();
                    }
                    SizeKind::Flame => {}
                }
                widgets::label(ui, "Quality");
                ui.add(egui::Slider::new(&mut self.settings.quality, 50.0..=20000.0).logarithmic(true).max_decimals(0));
                ui.end_row();
                widgets::label(ui, "Format");
                egui::ComboBox::from_id_salt("batch-fmt").selected_text(self.settings.format.label()).show_ui(ui, |ui| {
                    for f in ImageFormat::ALL {
                        ui.selectable_value(&mut self.settings.format, *f, f.label());
                    }
                });
                ui.end_row();
                if self.settings.format == ImageFormat::Jpeg {
                    widgets::label(ui, "JPEG quality");
                    ui.add(egui::Slider::new(&mut self.settings.jpeg_quality, 50..=100));
                    ui.end_row();
                }
                widgets::label(ui, "Depth");
                ui.checkbox(&mut self.settings.zbuffer, "Z-buffer (16-bit PNG)");
                ui.end_row();
                widgets::label(ui, "Denoiser");
                ui.checkbox(&mut self.settings.denoise, "Run the flame's AI denoiser");
                ui.end_row();
                widgets::label(ui, "Animations");
                ui.checkbox(&mut self.settings.animations, "Render as video");
                ui.end_row();
                if self.settings.animations {
                    let ffmpeg = *self.ffmpeg.get_or_insert_with(rwf_core::video::ffmpeg_available);
                    if !ffmpeg && self.settings.video_format.needs_ffmpeg() {
                        self.settings.video_format = VideoFormat::PngSequence;
                    }
                    widgets::label(ui, "Video format");
                    egui::ComboBox::from_id_salt("batch-vfmt").selected_text(self.settings.video_format.label()).show_ui(ui, |ui| {
                        for f in VideoFormat::ALL {
                            ui.add_enabled_ui(!f.needs_ffmpeg() || ffmpeg, |ui| {
                                ui.selectable_value(&mut self.settings.video_format, *f, f.label());
                            });
                        }
                    });
                    ui.end_row();
                }
            });
            self.settings.size = match self.size_kind {
                SizeKind::Flame => SizeMode::Flame,
                SizeKind::LongEdge => SizeMode::LongEdge(self.long_edge),
                SizeKind::Fixed => SizeMode::Fixed(self.fixed.0, self.fixed.1),
            };

            theme::section_label(ui, "Output");
            let mut next_to = self.settings.out_dir.is_none();
            ui.radio_value(&mut next_to, true, "Next to the flame files");
            ui.horizontal(|ui| {
                if ui.radio(!next_to, "Folder").clicked() && self.settings.out_dir.is_none() {
                    next_to = false;
                }
                if ui.small_button("Choose…").clicked()
                    && let Some(d) = crate::settings::dialog(crate::settings::Folder::Images).pick_folder()
                {
                    self.settings.out_dir = Some(d);
                    next_to = false;
                }
            });
            if next_to {
                self.settings.out_dir = None;
            } else if self.settings.out_dir.is_none() {
                self.settings.out_dir = std::env::current_dir().ok();
            }
            if let Some(d) = &self.settings.out_dir {
                ui.label(RichText::new(d.display().to_string()).small().color(theme::TEXT_MUTED));
            }
            ui.checkbox(&mut self.settings.overwrite, "Overwrite existing files");
        });

        ui.add_space(10.0);
        let pending = self.entries.iter().filter(|e| e.state == JobState::Pending).count();
        if running {
            let stopping = self.worker.as_ref().is_some_and(|w| w.cancel.load(Ordering::Relaxed));
            let label = if stopping { "Stopping…" } else { "Stop" };
            if ui
                .add_enabled(
                    !stopping,
                    egui::Button::new(RichText::new(format!("{}  {label}", crate::icons::PAUSE)).strong()).min_size(vec2(ui.available_width(), 30.0)),
                )
                .clicked()
                && let Some(w) = &self.worker
            {
                w.cancel.store(true, Ordering::Relaxed);
            }
        } else if ui
            .add_enabled(
                gpu && pending > 0,
                egui::Button::new(RichText::new(format!("{}  Render {pending} jobs", crate::icons::PLAY)).strong())
                    .fill(theme::ACCENT_DIM)
                    .min_size(vec2(ui.available_width(), 30.0)),
            )
            .clicked()
        {
            self.start();
        }
        if !gpu {
            ui.label(RichText::new("The batch renderer needs a GPU.").small().color(theme::WARN));
        }
        let total = self.entries.len();
        if total > 0 {
            let done: f32 = self
                .entries
                .iter()
                .map(|e| match e.state {
                    JobState::Running(p) => p,
                    JobState::Pending => 0.0,
                    _ => 1.0,
                })
                .sum();
            ui.add(egui::ProgressBar::new(done / total as f32).desired_height(6.0));
        }
        if let Some(s) = &self.status {
            ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
        }

        self.job_details(ui);
    }

    fn job_details(&mut self, ui: &mut Ui) {
        let Some(i) = self.selected_index() else { return };
        let s = self.settings.clone();
        let e = &mut self.entries[i];
        theme::section_label(ui, "Selected job");
        ui.label(RichText::new(&e.job.name).color(theme::TEXT));
        if let rwf_gpu::batch::JobSource::File { path, .. } = &e.job.source {
            ui.label(RichText::new(path.display().to_string()).small().color(theme::TEXT_MUTED));
        }
        let editable = !matches!(e.state, JobState::Running(_));
        ui.add_enabled_ui(editable, |ui| {
            grid(ui, "batch-job", |ui| {
                widgets::label(ui, "Size");
                ui.horizontal(|ui| {
                    let mut custom = e.job.width.is_some();
                    if ui.checkbox(&mut custom, "").on_hover_text("Own size for this job").changed() {
                        let (w, h) = e.job.size_for(e.flame_size.0, e.flame_size.1, &s);
                        (e.job.width, e.job.height) = if custom { (Some(w), Some(h)) } else { (None, None) };
                    }
                    if let (Some(w), Some(h)) = (e.job.width.as_mut(), e.job.height.as_mut()) {
                        ui.add(egui::DragValue::new(w).range(16..=rwf_gpu::batch::MAX_SIZE));
                        ui.label("×");
                        ui.add(egui::DragValue::new(h).range(16..=rwf_gpu::batch::MAX_SIZE));
                    } else {
                        ui.label(RichText::new("default").color(theme::TEXT_MUTED));
                    }
                });
                ui.end_row();
                widgets::label(ui, "Quality");
                ui.horizontal(|ui| {
                    let mut custom = e.job.quality.is_some();
                    if ui.checkbox(&mut custom, "").on_hover_text("Own quality for this job").changed() {
                        e.job.quality = custom.then_some(s.quality);
                    }
                    match e.job.quality.as_mut() {
                        Some(q) => {
                            ui.add(egui::DragValue::new(q).range(1.0..=1.0e6).speed(10.0));
                        }
                        None => {
                            ui.label(RichText::new("default").color(theme::TEXT_MUTED));
                        }
                    }
                });
                ui.end_row();
            });
        });
        if let Some(out) = &e.output {
            widgets::label(ui, "Output");
            ui.label(RichText::new(out.display().to_string()).small());
        }
        if let Some(m) = &e.message {
            ui.label(RichText::new(m).small().color(if e.state == JobState::Failed { theme::WARN } else { theme::TEXT_MUTED }));
        }
    }
}

/// True if the path looks like a flame file (for dropped files).
pub fn is_flame_file(p: &Path) -> bool {
    p.is_dir() || p.extension().is_some_and(|e| e.eq_ignore_ascii_case("flame") || e.eq_ignore_ascii_case("xml"))
}

/// "1 flame", "3 flames".
fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}
