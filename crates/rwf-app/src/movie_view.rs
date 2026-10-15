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

//! Movie tab of the render workspace (JWildfire's "Easy Movie Maker"): a sequence of flames
//! that fade or morph into each other, camera and transform scripts on top, rendered into a
//! video.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use egui::{Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use rwf_core::Flame;
use rwf_core::movie::{GlobalScript, MorphType, Movie, MoviePart, Script, XFormScript};
use rwf_core::random::{Extras, Generator};
use rwf_core::render::{CpuRender, RenderOptions};
use rwf_core::video::VideoFormat;
use rwf_gpu::GpuContext;

use crate::ui::widgets::{self, grid};
use crate::{icons, theme};

const THUMB_W: u32 = 160;
const THUMB_H: u32 = 90;
const PREVIEW_W: u32 = 640;
const PREVIEW_QUALITY: f64 = 30.0;

/// Width, height and RGBA8 pixels of a preview.
type PreviewImage = (u32, u32, Vec<u8>);

enum Msg {
    Frame { index: u32, total: u32, rgba: Vec<u8> },
    Done(Result<PathBuf, String>),
}

struct Worker {
    cancel: Arc<AtomicBool>,
    msgs: Receiver<Msg>,
    started: Instant,
}

pub struct MovieView {
    movie: Movie,
    file: Option<PathBuf>,
    /// Stable ids of the parts (thumbnails survive reordering).
    ids: Vec<u64>,
    next_id: u64,
    thumbs: HashMap<u64, egui::TextureHandle>,
    thumb_tx: Sender<(u64, Vec<u8>)>,
    thumb_rx: Receiver<(u64, Vec<u8>)>,
    frame: u32,
    playing: bool,
    /// Bumped on every edit; the preview is rendered for (version, frame).
    version: u64,
    preview: Option<(egui::TextureHandle, u32, u32)>,
    preview_key: Option<(u64, u32)>,
    preview_job: Option<((u64, u32), Receiver<PreviewImage>)>,
    random: Option<Receiver<Flame>>,
    output: Option<PathBuf>,
    format: VideoFormat,
    on_gpu: bool,
    worker: Option<Worker>,
    progress: Option<(u32, u32)>,
    status: Option<String>,
    ffmpeg: Option<bool>,
}

impl Default for MovieView {
    fn default() -> Self {
        let (thumb_tx, thumb_rx) = channel();
        Self {
            movie: Movie::default(),
            file: None,
            ids: Vec::new(),
            next_id: 1,
            thumbs: HashMap::new(),
            thumb_tx,
            thumb_rx,
            frame: 0,
            playing: false,
            version: 0,
            preview: None,
            preview_key: None,
            preview_job: None,
            random: None,
            output: None,
            format: VideoFormat::Mp4,
            on_gpu: true,
            worker: None,
            progress: None,
            status: None,
            ffmpeg: None,
        }
    }
}

fn render_small(flame: &Flame, w: u32, h: u32, quality: f64) -> Vec<u8> {
    let threads = crate::settings::cpu_threads().saturating_sub(1).max(1);
    let opts = RenderOptions { width: w, height: h, threads, max_density: Some(quality), seed: 7, zbuffer: false };
    let mut f = flame.clone();
    f.bg_transparency = false;
    CpuRender::start(&f, opts).wait().image(false)
}

fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

impl MovieView {
    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    fn edited(&mut self) {
        self.version += 1;
    }

    fn add_part(&mut self, flame: Flame) {
        if self.movie.parts.is_empty() {
            // The first flame sets the frame size.
            self.movie.width = flame.width.max(16);
            self.movie.height = flame.height.max(16);
        }
        self.push_part(MoviePart::new(flame));
    }

    fn push_part(&mut self, part: MoviePart) {
        let id = self.next_id;
        self.next_id += 1;
        let tx = self.thumb_tx.clone();
        let f = part.flame.clone();
        std::thread::spawn(move || {
            let _ = tx.send((id, render_small(&f, THUMB_W, THUMB_H, 40.0)));
        });
        self.movie.parts.push(part);
        self.ids.push(id);
        self.edited();
    }

    fn set_movie(&mut self, movie: Movie) {
        self.ids.clear();
        self.thumbs.clear();
        let Movie { parts, .. } = &movie;
        let parts = parts.clone();
        self.movie = Movie { parts: Vec::new(), ..movie };
        for p in parts {
            self.push_part(p);
        }
        self.frame = 0;
        self.edited();
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((id, img)) = self.thumb_rx.try_recv() {
            let color = egui::ColorImage::from_rgba_unmultiplied([THUMB_W as usize, THUMB_H as usize], &img);
            self.thumbs.insert(id, ctx.load_texture(format!("movie-part-{id}"), color, egui::TextureOptions::LINEAR));
        }
        if let Some(rx) = &self.random
            && let Ok(f) = rx.try_recv()
        {
            self.random = None;
            self.add_part(f);
        }
        if let Some((key, rx)) = &self.preview_job
            && let Ok((w, h, img)) = rx.try_recv()
        {
            let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img);
            match &mut self.preview {
                Some((tex, pw, ph)) => {
                    tex.set(color, egui::TextureOptions::LINEAR);
                    (*pw, *ph) = (w, h);
                }
                None => self.preview = Some((ctx.load_texture("movie-preview", color, egui::TextureOptions::LINEAR), w, h)),
            }
            self.preview_key = Some(*key);
            self.preview_job = None;
            if self.playing {
                let total = self.movie.frame_count().max(1);
                self.frame = (self.frame + 1) % total;
            }
        }
        let mut done = None;
        if let Some(w) = &self.worker {
            while let Ok(m) = w.msgs.try_recv() {
                match m {
                    Msg::Frame { index, total, rgba } => {
                        self.progress = Some((index + 1, total));
                        let (pw, ph) = (self.movie.width, self.movie.height);
                        let color = egui::ColorImage::from_rgba_unmultiplied([pw as usize, ph as usize], &rgba);
                        match &mut self.preview {
                            Some((tex, w, h)) => {
                                tex.set(color, egui::TextureOptions::LINEAR);
                                (*w, *h) = (pw, ph);
                            }
                            None => self.preview = Some((ctx.load_texture("movie-preview", color, egui::TextureOptions::LINEAR), pw, ph)),
                        }
                        self.frame = index;
                        self.preview_key = Some((self.version, index));
                    }
                    Msg::Done(r) => done = Some(r),
                }
            }
        }
        if let Some(r) = done {
            let elapsed = self.worker.take().map(|w| w.started.elapsed()).unwrap_or_default();
            self.status = Some(match r {
                Ok(p) => format!("Saved {} ({})", p.display(), format_duration(elapsed)),
                Err(e) => format!("Movie not finished: {e}"),
            });
        }
        // Preview of the current frame (CPU, small) while not rendering the movie.
        let total = self.movie.frame_count();
        self.frame = self.frame.min(total.saturating_sub(1));
        let key = (self.version, self.frame);
        if self.worker.is_none()
            && total > 0
            && self.preview_key != Some(key)
            && self.preview_job.as_ref().is_none_or(|(k, _)| *k != key)
            && let Some(mut f) = self.movie.frame_flame(self.frame as f64)
        {
            let (tx, rx) = channel();
            let w = PREVIEW_W.min(self.movie.width.max(1));
            let h = ((w as f64 * self.movie.height as f64 / self.movie.width.max(1) as f64).round() as u32).max(1);
            // Keep the framing of the full size.
            f.pixels_per_unit *= w as f64 / self.movie.width.max(1) as f64;
            f.width = w;
            f.height = h;
            std::thread::spawn(move || {
                let _ = tx.send((w, h, render_small(&f, w, h, PREVIEW_QUALITY)));
            });
            self.preview_job = Some((key, rx));
        }
        if self.preview_job.is_some() || self.worker.is_some() || self.random.is_some() || self.thumbs.len() < self.ids.len() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    /// Shows the tab; returns a flame to load into the editor.
    pub fn show(&mut self, ui: &mut Ui, doc: &crate::doc::Document, gpu: Option<GpuContext>, random: (Generator, Extras)) -> Option<Flame> {
        self.poll(ui.ctx());
        let mut to_editor = None;
        egui::Panel::right("movie-panel")
            .resizable(true)
            .default_size(360.0)
            .size_range(330.0..=520.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().slider_width = 120.0;
                    self.panel(ui, gpu);
                });
            });
        egui::Panel::bottom("movie-parts")
            .resizable(false)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 8)))
            .show(ui, |ui| {
                if let Some(f) = self.parts_strip(ui, doc, random) {
                    to_editor = Some(f);
                }
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::CANVAS).inner_margin(egui::Margin::same(12))).show(ui, |ui| {
            if let Some(f) = self.preview_area(ui) {
                to_editor = Some(f);
            }
        });
        to_editor
    }

    fn panel(&mut self, ui: &mut Ui, gpu: Option<GpuContext>) {
        let running = self.is_running();
        theme::section_label(ui, "Movie");
        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal(|ui| {
                if ui.button("New").clicked() {
                    self.set_movie(Movie::default());
                    self.file = None;
                }
                if ui.button("Open…").clicked()
                    && let Some(p) = crate::settings::dialog(crate::settings::Folder::Movies).add_filter("JWildfire movie", &["jwfmovie"]).pick_file()
                {
                    match rwf_core::movie::read_movie_file(&p) {
                        Ok(m) => {
                            self.set_movie(m);
                            self.file = Some(p);
                            self.status = None;
                        }
                        Err(e) => self.status = Some(format!("Cannot open {}: {e}", p.display())),
                    }
                }
                let save_as = |name: &str| {
                    crate::settings::dialog(crate::settings::Folder::Movies).add_filter("JWildfire movie", &["jwfmovie"]).set_file_name(format!("{name}.jwfmovie")).save_file()
                };
                if ui.add_enabled(!self.movie.parts.is_empty(), egui::Button::new("Save as…")).clicked()
                    && let Some(p) = save_as(if self.movie.name.is_empty() { "movie" } else { &self.movie.name })
                {
                    let p = p.with_extension("jwfmovie");
                    self.status = Some(match rwf_core::movie::write_movie_file(&self.movie, &p) {
                        Ok(()) => format!("Saved {}", p.display()),
                        Err(e) => format!("Cannot save: {e}"),
                    });
                    self.file = Some(p);
                }
            });
            if let Some(f) = &self.file {
                ui.label(RichText::new(f.display().to_string()).small().color(theme::TEXT_MUTED));
            }
        });

        theme::section_label(ui, "Frames");
        let mut changed = false;
        ui.add_enabled_ui(!running, |ui| {
            grid(ui, "movie-frames", |ui| {
                let m = &mut self.movie;
                widgets::label(ui, "Size");
                ui.horizontal(|ui| {
                    changed |= ui.add(egui::DragValue::new(&mut m.width).range(16..=16384)).changed();
                    ui.label("×");
                    changed |= ui.add(egui::DragValue::new(&mut m.height).range(16..=16384)).changed();
                });
                ui.end_row();
                widgets::label(ui, "Presets");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (l, w, h) in [("720p", 1280u32, 720u32), ("1080p", 1920, 1080), ("1440p", 2560, 1440), ("4K", 3840, 2160)] {
                        if ui.small_button(l).clicked() {
                            (m.width, m.height) = (w, h);
                            changed = true;
                        }
                    }
                });
                ui.end_row();
                widgets::label(ui, "Frame rate");
                changed |= ui.add(egui::DragValue::new(&mut m.fps).range(1.0..=240.0).speed(0.2).suffix(" fps")).changed();
                ui.end_row();
                widgets::label(ui, "Quality");
                ui.add(egui::Slider::new(&mut m.quality, 20.0..=5000.0).logarithmic(true).max_decimals(0));
                ui.end_row();
                widgets::label(ui, "Motion blur");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut m.motion_blur_length).range(0..=64).suffix(" sub-frames"));
                    ui.add_enabled(
                        m.motion_blur_length > 0,
                        egui::DragValue::new(&mut m.motion_blur_time_step).range(0.0..=2.0).speed(0.002).max_decimals(3),
                    )
                    .on_hover_text("Time between the sub-frames (frames)");
                });
                ui.end_row();
            });
            let total = self.movie.frame_count();
            ui.label(RichText::new(format!("{total} frames, {:.1} s", total as f64 / self.movie.fps.max(1.0))).small().color(theme::TEXT_MUTED));

            theme::section_label(ui, "Camera scripts");
            changed |= script_list(ui, "movie-global", &mut self.movie.global_scripts, GlobalScript::ALL, GlobalScript::label);
            theme::section_label(ui, "Transform scripts");
            changed |= script_list(ui, "movie-xform", &mut self.movie.xform_scripts, XFormScript::ALL, XFormScript::label);
            ui.label(
                RichText::new("Amplitude 1 turns once in six seconds; camera moves sway by one unit in twelve seconds.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        });
        if changed {
            self.edited();
        }

        theme::section_label(ui, "Output");
        ui.add_enabled_ui(!running, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("movie-format").width(150.0).selected_text(self.format.label()).show_ui(ui, |ui| {
                    for f in VideoFormat::ALL {
                        if ui.selectable_value(&mut self.format, *f, f.label()).changed() {
                            self.output = None;
                        }
                    }
                });
                if ui.button("Choose…").clicked() {
                    let name = if self.movie.name.is_empty() { "movie".to_string() } else { self.movie.name.clone() };
                    self.output = match self.format.extension() {
                        Some(ext) => crate::settings::dialog(crate::settings::Folder::Images)
                            .add_filter(self.format.label(), &[ext])
                            .set_file_name(format!("{name}.{ext}"))
                            .save_file()
                            .map(|p| p.with_extension(ext)),
                        None => crate::settings::dialog(crate::settings::Folder::Images).set_title("Folder for the PNG frames").pick_folder(),
                    };
                }
            });
            let path = self.output.as_ref().map_or("Output not chosen".into(), |p| p.display().to_string());
            ui.add(egui::Label::new(RichText::new(&path).small().color(theme::TEXT_MUTED)).truncate()).on_hover_text(path);
            ui.add_enabled(gpu.is_some(), egui::Checkbox::new(&mut self.on_gpu, "Render on the GPU"));
        });
        let ffmpeg = *self.ffmpeg.get_or_insert_with(rwf_core::video::ffmpeg_available);
        let needs_ffmpeg = self.format != VideoFormat::PngSequence;
        if needs_ffmpeg && !ffmpeg {
            ui.label(RichText::new("Video files need ffmpeg in the PATH; PNG sequences work without it.").small().color(theme::WARN));
        }

        ui.add_space(10.0);
        if let Some(w) = &self.worker {
            let stopping = w.cancel.load(Ordering::Relaxed);
            if ui
                .add_enabled(
                    !stopping,
                    egui::Button::new(RichText::new(format!("{}  {}", icons::PAUSE, if stopping { "Stopping…" } else { "Stop" })).strong())
                        .min_size(vec2(ui.available_width(), 30.0)),
                )
                .clicked()
            {
                w.cancel.store(true, Ordering::Relaxed);
            }
            let elapsed = w.started.elapsed();
            if let Some((done, total)) = self.progress {
                let f = done as f32 / total.max(1) as f32;
                ui.add(egui::ProgressBar::new(f).text(format!("Frame {done} / {total}")));
                if done > 0 {
                    let left = elapsed.as_secs_f64() / done as f64 * (total - done) as f64;
                    ui.label(
                        RichText::new(format!("{} elapsed, about {} left", format_duration(elapsed), format_duration(Duration::from_secs_f64(left))))
                            .small()
                            .color(theme::TEXT_MUTED),
                    );
                }
            }
        } else if ui
            .add_enabled(
                !self.movie.parts.is_empty() && self.output.is_some() && (ffmpeg || !needs_ffmpeg),
                egui::Button::new(RichText::new(format!("{}  Render movie", icons::PLAY)).strong())
                    .fill(theme::ACCENT_DIM)
                    .min_size(vec2(ui.available_width(), 30.0)),
            )
            .clicked()
        {
            self.start(if self.on_gpu { gpu } else { None });
        }
        if let Some(s) = &self.status {
            ui.add_space(6.0);
            ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
        }
    }

    fn start(&mut self, gpu: Option<GpuContext>) {
        let Some(path) = self.output.clone() else { return };
        let movie = self.movie.clone();
        let format = self.format;
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let c2 = cancel.clone();
        self.playing = false;
        self.progress = Some((0, movie.frame_count()));
        self.status = None;
        std::thread::spawn(move || {
            // An own device keeps the editor responsive.
            let device = gpu.and_then(|_| GpuContext::new_headless().ok());
            let tx2 = tx.clone();
            let r = rwf_gpu::movie::render_movie(&movie, &path, format, device, &c2, |index, total, rgba| {
                let _ = tx2.send(Msg::Frame { index, total, rgba: rgba.to_vec() });
            });
            let _ = tx.send(Msg::Done(r.map(|_| path)));
        });
        self.worker = Some(Worker { cancel, msgs: rx, started: Instant::now() });
    }

    fn preview_area(&mut self, ui: &mut Ui) -> Option<Flame> {
        let mut to_editor = None;
        let total = self.movie.frame_count();
        let avail = ui.available_size();
        let rect = Rect::from_min_size(ui.cursor().min, vec2(avail.x, (avail.y - 40.0).max(80.0)));
        ui.allocate_rect(rect, Sense::hover());
        let painter = ui.painter_at(rect);
        match &self.preview {
            Some((tex, w, h)) if total > 0 => {
                let size = vec2(*w as f32, *h as f32);
                let scale = (rect.width() / size.x).min(rect.height() / size.y);
                let img = Rect::from_center_size(rect.center(), size * scale);
                painter.rect_filled(img, 0.0, Color32::BLACK);
                painter.image(tex.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                if self.preview_job.is_some() {
                    painter.text(
                        img.right_top() + vec2(-8.0, 8.0),
                        egui::Align2::RIGHT_TOP,
                        "…",
                        egui::FontId::proportional(18.0),
                        theme::TEXT_MUTED,
                    );
                }
            }
            _ => {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Add flames below to start a movie",
                    egui::FontId::proportional(15.0),
                    theme::TEXT_MUTED,
                );
            }
        }
        ui.add_space(6.0);
        ui.add_enabled_ui(total > 0 && self.worker.is_none(), |ui| {
            ui.horizontal(|ui| {
                let play = if self.playing { icons::PAUSE } else { icons::PLAY };
                if ui.button(play).on_hover_text("Play the preview (as fast as it renders)").clicked() {
                    self.playing = !self.playing;
                }
                ui.spacing_mut().slider_width = (ui.available_width() - 220.0).max(80.0);
                let mut f = self.frame;
                if ui.add(egui::Slider::new(&mut f, 0..=total.saturating_sub(1)).text("frame")).changed() {
                    self.frame = f;
                    self.playing = false;
                }
                if ui.button("To editor").on_hover_text("Open this frame in the editor").clicked() {
                    to_editor = self.movie.frame_flame(self.frame as f64);
                }
            });
        });
        to_editor
    }

    fn parts_strip(&mut self, ui: &mut Ui, doc: &crate::doc::Document, random: (Generator, Extras)) -> Option<Flame> {
        let running = self.is_running();
        let mut to_editor = None;
        ui.horizontal(|ui| {
            theme::section_label(ui, "Parts");
            ui.add_space(8.0);
            ui.add_enabled_ui(!running, |ui| {
                if ui.button(format!("{}  Editor flame", icons::PLUS)).on_hover_text("Add the flame of the editor").clicked() {
                    self.add_part(doc.flame.at_frame(0.0));
                }
                if ui.button(format!("{}  Files…", icons::PLUS)).clicked()
                    && let Some(files) = crate::settings::dialog(crate::settings::Folder::Flames).add_filter("Flame", &["flame", "xml"]).pick_files()
                {
                    for p in files {
                        match rwf_core::io::read_flames_from_file(&p) {
                            Ok(flames) => flames.into_iter().for_each(|f| self.add_part(f)),
                            Err(e) => self.status = Some(format!("{}: {e}", p.display())),
                        }
                    }
                }
                if self.random.is_some() {
                    ui.spinner();
                } else if ui
                    .button(format!("{}  Random flame", icons::DICE))
                    .on_hover_text(format!("A new flame of the generator \"{}\"", random.0.label()))
                    .clicked()
                {
                    let (tx, rx) = channel();
                    std::thread::spawn(move || {
                        let mut rng = rwf_core::rng::Rng::from_entropy();
                        let _ = tx.send(rwf_core::random::generate_with(random.0, random.1, &mut rng));
                    });
                    self.random = Some(rx);
                }
            });
        });
        ui.add_space(4.0);
        let current = self.movie.part_at(self.frame as f64).map(|(i, _)| i);
        let mut action: Option<(usize, i32)> = None;
        let mut changed = false;
        egui::ScrollArea::horizontal().id_salt("movie-parts-scroll").show(ui, |ui| {
            ui.horizontal(|ui| {
                let n = self.movie.parts.len();
                for i in 0..n {
                    let id = self.ids[i];
                    let frame_start: u32 = self.movie.parts[..i].iter().map(|p| p.frames).sum();
                    let selected = current == Some(i);
                    let stroke = if selected { Stroke::new(1.5, theme::ACCENT) } else { Stroke::new(1.0, theme::BORDER) };
                    egui::Frame::new().fill(theme::SURFACE).stroke(stroke).corner_radius(6).inner_margin(6).show(ui, |ui| {
                        ui.set_width(THUMB_W as f32);
                        ui.vertical(|ui| {
                            let (r, resp) = ui.allocate_exact_size(vec2(THUMB_W as f32, THUMB_H as f32), Sense::click());
                            match self.thumbs.get(&id) {
                                Some(t) => {
                                    ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                                }
                                None => {
                                    ui.painter().rect_filled(r, CornerRadius::same(4), theme::BG);
                                    ui.painter().text(
                                        r.center(),
                                        egui::Align2::CENTER_CENTER,
                                        "…",
                                        egui::FontId::proportional(16.0),
                                        theme::TEXT_MUTED,
                                    );
                                }
                            }
                            if resp.on_hover_text("Click: go to this part").clicked() {
                                self.frame = frame_start;
                                self.playing = false;
                            }
                            ui.add_enabled_ui(!running, |ui| {
                                let p = &mut self.movie.parts[i];
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("{}", i + 1)).strong());
                                    changed |= ui
                                        .add(egui::DragValue::new(&mut p.frames).range(1..=100_000).suffix(" fr"))
                                        .on_hover_text("Frames of this part")
                                        .changed();
                                });
                                if i + 1 < n {
                                    ui.horizontal(|ui| {
                                        egui::ComboBox::from_id_salt(("movie-morph", id)).width(62.0).selected_text(p.morph.label()).show_ui(
                                            ui,
                                            |ui| {
                                                for m in MorphType::ALL {
                                                    changed |= ui.selectable_value(&mut p.morph, m, m.label()).changed();
                                                }
                                            },
                                        );
                                        changed |= ui
                                            .add(egui::DragValue::new(&mut p.morph_frames).range(0..=p.frames).suffix(" fr"))
                                            .on_hover_text("The last frames of this part change into the next part")
                                            .changed();
                                    });
                                } else {
                                    ui.label(RichText::new("last part").small().color(theme::TEXT_MUTED));
                                }
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    if ui.add_enabled(i > 0, egui::Button::new(icons::ARROW_LEFT).small()).on_hover_text("Move left").clicked() {
                                        action = Some((i, -1));
                                    }
                                    if ui.add_enabled(i + 1 < n, egui::Button::new(icons::ARROW_RIGHT).small()).on_hover_text("Move right").clicked()
                                    {
                                        action = Some((i, 1));
                                    }
                                    if ui.small_button("Edit").on_hover_text("Open this flame in the editor").clicked() {
                                        to_editor = Some(self.movie.parts[i].flame.clone());
                                    }
                                    if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                                        action = Some((i, 0));
                                    }
                                });
                            });
                        });
                    });
                }
                if n == 0 {
                    ui.label(RichText::new("No parts yet: add the editor flame, flame files or random flames.").color(theme::TEXT_MUTED));
                }
            });
        });
        if let Some((i, d)) = action {
            if d == 0 {
                self.movie.parts.remove(i);
                let id = self.ids.remove(i);
                self.thumbs.remove(&id);
            } else {
                let j = (i as i32 + d) as usize;
                self.movie.parts.swap(i, j);
                self.ids.swap(i, j);
            }
            changed = true;
        }
        if changed {
            self.edited();
        }
        to_editor
    }
}

/// Rows of scripts (type and amplitude) with add and remove buttons; returns true on a change.
fn script_list<T: Copy + PartialEq + Default>(ui: &mut Ui, id: &str, scripts: &mut Vec<Script<T>>, all: &[T], label: fn(T) -> &'static str) -> bool {
    let mut changed = false;
    let mut remove = None;
    for (i, s) in scripts.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt((id, i)).width(190.0).selected_text(label(s.kind)).show_ui(ui, |ui| {
                for k in all.iter().skip(1) {
                    changed |= ui.selectable_value(&mut s.kind, *k, label(*k)).changed();
                }
            });
            changed |= ui
                .add(egui::DragValue::new(&mut s.amplitude).speed(0.01).range(-100.0..=100.0))
                .on_hover_text("Amplitude (negative: other direction)")
                .changed();
            if ui.small_button(icons::X).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        scripts.remove(i);
        changed = true;
    }
    if scripts.len() < 12 && ui.small_button(format!("{}  Add", icons::PLUS)).clicked() {
        scripts.push(Script { kind: all.get(1).copied().unwrap_or_default(), amplitude: 1.0 });
        changed = true;
    }
    changed
}
