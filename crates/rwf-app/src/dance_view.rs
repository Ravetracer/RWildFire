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

//! Dance tab of the render workspace (JWildfire's "Dancing flames"): a pool of flames moved by
//! music in real time. Clicking a flame (or pressing 1–9) switches to it with a morph; a
//! recording of the switches renders into a video with the music.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use rwf_core::Flame;
use rwf_core::audio::Pcm;
use rwf_core::dance::{BANDS, Cue, DanceProject, Link, Motion, MotionKind, Performance, Spectrum, band_label, targets};
use rwf_core::random::{Extras, Generator};
use rwf_core::solid::ShadowType;
use rwf_core::video::VideoFormat;
use rwf_gpu::{GpuContext, GpuRenderer};

use crate::audio_player::{FreeClock, Player};
use crate::ui::widgets::{self, grid};
use crate::{icons, theme};

enum Msg {
    Frame { index: u32, total: u32 },
    Done(Result<PathBuf, String>),
}

struct Worker {
    cancel: Arc<AtomicBool>,
    msgs: Receiver<Msg>,
    started: Instant,
}

/// A decoded and analysed music file (or why loading failed).
type Loaded = Result<(PathBuf, Pcm, Spectrum), String>;

/// The music of the show: playback and its spectrum.
struct Music {
    path: PathBuf,
    player: Player,
    spectrum: Arc<Spectrum>,
}

pub struct DanceView {
    project: DanceProject,
    file: Option<PathBuf>,
    music: Option<Music>,
    loading: Option<Receiver<Loaded>>,
    sound_status: Option<String>,
    muted: bool,
    clock: FreeClock,
    /// The flame shown and the one it morphs from (with the show time the morph started).
    current: usize,
    morph: Option<(usize, f64)>,
    recording: Option<Performance>,
    // realtime view
    renderer: Option<GpuRenderer>,
    texture: Option<(egui::TextureId, u64)>,
    iters: u32,
    last_frame: Instant,
    fps: f64,
    /// Fraction of the view resolution that is rendered.
    resolution: f32,
    show_spectrum: bool,
    // editing
    selected_motion: Option<usize>,
    link_flame: usize,
    link_filter: String,
    random: Option<Receiver<Flame>>,
    // export
    output: Option<PathBuf>,
    format: VideoFormat,
    on_gpu: bool,
    worker: Option<Worker>,
    progress: Option<(u32, u32)>,
    status: Option<String>,
    ffmpeg: Option<bool>,
}

impl Default for DanceView {
    fn default() -> Self {
        Self {
            project: DanceProject::default(),
            file: None,
            music: None,
            loading: None,
            sound_status: None,
            muted: false,
            clock: FreeClock::default(),
            current: 0,
            morph: None,
            recording: None,
            renderer: None,
            texture: None,
            iters: 16,
            last_frame: Instant::now(),
            fps: 0.0,
            resolution: 0.5,
            show_spectrum: true,
            selected_motion: None,
            link_flame: 0,
            link_filter: String::new(),
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

fn format_time(t: f64) -> String {
    let s = t.max(0.0);
    format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

fn flame_name(f: &Flame, i: usize) -> String {
    if f.name.trim().is_empty() { format!("Flame {}", i + 1) } else { f.name.clone() }
}

/// Cheaper settings for the realtime view (as JWildfire's `FlamePreparer`).
fn realtime_flame(mut f: Flame) -> Flame {
    f.bg_transparency = false;
    f.solid.ao_enabled = false;
    f.solid.shadow_type = ShadowType::Off;
    f.cam_dof = 0.0;
    f
}

impl DanceView {
    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    fn spectrum(&self) -> Option<&Spectrum> {
        self.music.as_ref().map(|m| m.spectrum.as_ref())
    }

    /// Show time in seconds.
    fn now(&self) -> f64 {
        match &self.music {
            Some(m) => m.player.position(),
            None => self.clock.position(),
        }
    }

    fn is_playing(&self) -> bool {
        match &self.music {
            Some(m) => m.player.is_playing() && !m.player.at_end(),
            None => self.clock.is_playing(),
        }
    }

    fn play(&mut self) {
        match &mut self.music {
            Some(m) => m.player.play(),
            None => self.clock.play(),
        }
    }

    fn pause(&mut self) {
        match &mut self.music {
            Some(m) => m.player.pause(),
            None => self.clock.pause(),
        }
        self.stop_recording();
    }

    fn seek(&mut self, t: f64) {
        self.stop_recording();
        self.morph = None;
        match &mut self.music {
            Some(m) => m.player.seek(t),
            None => self.clock.seek(t),
        }
    }

    fn switch_to(&mut self, index: usize) {
        if index >= self.project.flames.len() || index == self.current {
            return;
        }
        let now = self.now();
        // The morph runs on show time: while paused the switch is immediate.
        self.morph = self.is_playing().then_some((self.current, now));
        self.current = index;
        if let Some(rec) = &mut self.recording {
            rec.cues.push(Cue { time: now, flame: index });
        }
    }

    fn start_recording(&mut self) {
        let now = self.now();
        self.morph = None;
        self.recording = Some(Performance { cues: vec![Cue { time: now, flame: self.current }], start: now, end: now });
        self.play();
    }

    fn stop_recording(&mut self) {
        if let Some(mut rec) = self.recording.take() {
            rec.end = self.now();
            if rec.end > rec.start + 0.2 {
                self.status = Some(format!("Recorded {} with {} flame switches", format_time(rec.end - rec.start), rec.cues.len() - 1));
                self.project.performance = rec;
            }
        }
    }

    fn add_flame(&mut self, f: Flame) {
        self.project.flames.push(f);
        if self.project.flames.len() == 1 {
            self.current = 0;
        }
    }

    fn load_music(&mut self, path: PathBuf) {
        let (tx, rx) = channel();
        self.sound_status = Some("Analysing the music…".into());
        std::thread::spawn(move || {
            let r = Pcm::load(&path).map_err(|e| e.to_string()).map(|pcm| {
                let spectrum = Spectrum::analyse(&pcm.mono());
                (path, pcm, spectrum)
            });
            let _ = tx.send(r);
        });
        self.loading = Some(rx);
    }

    /// Opens a `.rwfdance` project.
    pub fn open_file(&mut self, path: &std::path::Path) -> Result<(), String> {
        let p = rwf_core::dance::read_project_file(path).map_err(|e| e.to_string())?;
        self.set_project(p, Some(path.to_path_buf()));
        Ok(())
    }

    fn set_project(&mut self, p: DanceProject, file: Option<PathBuf>) {
        self.stop_recording();
        let sound = p.sound.clone();
        self.project = p;
        self.file = file;
        self.current = 0;
        self.morph = None;
        self.selected_motion = None;
        self.music = None;
        self.clock = FreeClock::default();
        if let Some(s) = sound {
            self.load_music(s);
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.loading
            && let Ok(r) = rx.try_recv()
        {
            self.loading = None;
            match r {
                Ok((path, pcm, spectrum)) => {
                    let player = Player::new(&pcm);
                    player.set_muted(self.muted);
                    self.sound_status = player.device_error.as_ref().map(|e| format!("No sound output ({e}); the show runs silently."));
                    self.project.sound = Some(path.clone());
                    self.music = Some(Music { path, player, spectrum: Arc::new(spectrum) });
                }
                Err(e) => self.sound_status = Some(format!("Could not load the music: {e}")),
            }
        }
        if let Some(rx) = &self.random
            && let Ok(f) = rx.try_recv()
        {
            self.random = None;
            self.add_flame(f);
        }
        let mut done = None;
        if let Some(w) = &self.worker {
            while let Ok(m) = w.msgs.try_recv() {
                match m {
                    Msg::Frame { index, total } => self.progress = Some((index + 1, total)),
                    Msg::Done(r) => done = Some(r),
                }
            }
        }
        if let Some(r) = done {
            let elapsed = self.worker.take().map(|w| w.started.elapsed()).unwrap_or_default();
            self.status = Some(match r {
                Ok(p) => format!("Saved {} ({:.0} s)", p.display(), elapsed.as_secs_f64()),
                Err(e) => format!("Video not finished: {e}"),
            });
        }
        if self.loading.is_some() || self.random.is_some() || self.worker.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // The music ended: end the recording there.
        if self.recording.is_some() && self.music.as_ref().is_some_and(|m| m.player.at_end()) {
            self.stop_recording();
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let keys = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
            egui::Key::Num8,
            egui::Key::Num9,
        ];
        let (pressed, space) = ctx.input(|i| (keys.iter().position(|k| i.key_pressed(*k)), i.key_pressed(egui::Key::Space)));
        if let Some(i) = pressed {
            self.switch_to(i);
        }
        if space {
            if self.is_playing() { self.pause() } else { self.play() }
        }
    }

    /// Shows the tab; returns a flame to load into the editor.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        frame: &eframe::Frame,
        editor_flame: &Flame,
        gpu: Option<GpuContext>,
        random: (Generator, Extras),
    ) -> Option<Flame> {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        self.shortcuts(&ctx);
        if self.current >= self.project.flames.len() {
            self.current = 0;
        }
        let mut to_editor = None;
        egui::Panel::bottom("dance-transport")
            .exact_size(44.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 6)))
            .show(ui, |ui| self.transport(ui));
        egui::Panel::left("dance-pool")
            .resizable(true)
            .default_size(250.0)
            .size_range(220.0..=380.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(10, 8)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    to_editor = self.pool_panel(ui, editor_flame, random);
                });
            });
        egui::Panel::right("dance-motions")
            .resizable(true)
            .default_size(340.0)
            .size_range(300.0..=520.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    self.motions_panel(ui);
                    self.export_panel(ui, &gpu);
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::CANVAS)).show(ui, |ui| {
            self.view(ui, frame, gpu);
        });
        to_editor
    }

    // ---- pool -----------------------------------------------------------------------------

    fn pool_panel(&mut self, ui: &mut Ui, editor_flame: &Flame, random: (Generator, Extras)) -> Option<Flame> {
        let mut to_editor = None;
        theme::section_label(ui, "Project");
        ui.horizontal(|ui| {
            if ui.button("Open…").clicked()
                && let Some(p) = rfd::FileDialog::new().add_filter("Dance project", &["rwfdance"]).pick_file()
                && let Err(e) = self.open_file(&p)
            {
                self.status = Some(format!("Could not open {}: {e}", p.display()));
            }
            if ui.button("Save…").clicked() {
                let name = self.file.as_ref().and_then(|f| f.file_name()).map_or("show.rwfdance".into(), |n| n.to_string_lossy().to_string());
                if let Some(p) = rfd::FileDialog::new().add_filter("Dance project", &["rwfdance"]).set_file_name(name).save_file() {
                    let p = p.with_extension("rwfdance");
                    match rwf_core::dance::write_project_file(&self.project, &p) {
                        Ok(()) => {
                            self.status = Some(format!("Saved {}", p.display()));
                            self.file = Some(p);
                        }
                        Err(e) => self.status = Some(format!("Could not save: {e}")),
                    }
                }
            }
            if ui.button("New").on_hover_text("Empty pool, no motions").clicked() {
                self.set_project(DanceProject::default(), None);
            }
        });

        theme::section_label(ui, "Flames");
        ui.horizontal_wrapped(|ui| {
            if ui.button(format!("{} Editor flame", icons::PLUS)).on_hover_text("Add the flame of the editor").clicked() {
                self.add_flame(editor_flame.clone());
            }
            if self.random.is_some() {
                ui.add_enabled(false, egui::Button::new("Generating…"));
            } else if ui
                .button(format!("{} Random", icons::DICE))
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
            if ui.button(format!("{} Files…", icons::PLUS)).clicked()
                && let Some(files) = rfd::FileDialog::new().add_filter("Flame", &["flame", "xml"]).pick_files()
            {
                for f in files {
                    match rwf_core::io::read_flames_from_file(&f) {
                        Ok(flames) => flames.into_iter().for_each(|fl| self.add_flame(fl)),
                        Err(e) => self.status = Some(format!("Could not read {}: {e}", f.display())),
                    }
                }
            }
        });
        ui.add_space(4.0);
        if self.project.flames.is_empty() {
            ui.label(
                RichText::new("Add flames to dance with. Click one (or press 1–9) to switch to it while the music plays.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        }
        let mut remove = None;
        for i in 0..self.project.flames.len() {
            let selected = i == self.current;
            let fill = if selected { theme::SURFACE_HOVER } else { theme::SURFACE };
            let resp = egui::Frame::new()
                .fill(fill)
                .stroke(Stroke::new(1.0, if selected { theme::ACCENT } else { theme::BORDER }))
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let key = if i < 9 { format!("{}", i + 1) } else { " ".into() };
                        ui.label(RichText::new(key).monospace().color(theme::ACCENT));
                        ui.label(RichText::new(flame_name(&self.project.flames[i], i)).color(theme::TEXT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(ui, icons::X, "Remove from the pool").clicked() {
                                remove = Some(i);
                            }
                            if widgets::icon_button(ui, icons::ARROW_LEFT, "Load into the editor").clicked() {
                                to_editor = Some(self.project.flames[i].clone());
                            }
                        });
                    });
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
                    if let Some(l) = self.project.flames[i].layers.first() {
                        widgets::paint_gradient(ui, r, &l.gradient);
                    }
                })
                .response;
            if resp.interact(Sense::click()).clicked() {
                self.switch_to(i);
            }
            ui.add_space(3.0);
        }
        if let Some(i) = remove {
            self.project.remove_flame(i);
            self.morph = None;
            if self.current >= i && self.current > 0 {
                self.current -= 1;
            }
        }
        to_editor
    }

    // ---- motions --------------------------------------------------------------------------

    fn motions_panel(&mut self, ui: &mut Ui) {
        theme::section_label(ui, "Motions");
        ui.horizontal_wrapped(|ui| {
            if ui.button(format!("{} Motion", icons::PLUS)).clicked() {
                let n = self.project.motions.len() + 1;
                self.project.motions.push(Motion::new(format!("Motion {n}"), MotionKind::default_of(0)));
                self.selected_motion = Some(self.project.motions.len() - 1);
            }
            if ui
                .add_enabled(!self.project.flames.is_empty(), egui::Button::new("Default motions"))
                .on_hover_text("Replace all motions by a set that moves the first transforms of every flame with the music")
                .clicked()
            {
                self.project.create_default_motions();
                self.selected_motion = None;
            }
        });
        let now = self.now();
        let spectrum = self.music.as_ref().map(|m| m.spectrum.clone());
        let mut remove = None;
        for (i, m) in self.project.motions.iter().enumerate() {
            let selected = self.selected_motion == Some(i);
            let value = m.value(spectrum.as_deref(), now);
            let resp = egui::Frame::new()
                .fill(if selected { theme::SURFACE_HOVER } else { theme::SURFACE })
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&m.name).color(theme::TEXT));
                        ui.label(RichText::new(format!("{} links", m.links.len())).small().color(theme::TEXT_MUTED));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(ui, icons::X, "Remove the motion").clicked() {
                                remove = Some(i);
                            }
                            ui.label(RichText::new(format!("{value:+.2}")).monospace().small().color(theme::TEXT_MUTED));
                        });
                    });
                })
                .response;
            if resp.interact(Sense::click()).clicked() {
                self.selected_motion = if selected { None } else { Some(i) };
            }
            ui.add_space(2.0);
        }
        if let Some(i) = remove {
            self.project.motions.remove(i);
            self.selected_motion = None;
        }
        let Some(mi) = self.selected_motion.filter(|i| *i < self.project.motions.len()) else { return };
        ui.add_space(6.0);
        theme::section_label(ui, "Selected motion");
        let flames = &self.project.flames;
        let m = &mut self.project.motions[mi];
        grid(ui, "dance-motion", |ui| {
            widgets::label(ui, "Name");
            ui.text_edit_singleline(&mut m.name);
            ui.end_row();
            widgets::label(ui, "Type");
            let mut ti = m.kind.index();
            egui::ComboBox::from_id_salt("dance-motion-type").selected_text(MotionKind::NAMES[ti]).show_ui(ui, |ui| {
                for (k, n) in MotionKind::NAMES.iter().enumerate() {
                    ui.selectable_value(&mut ti, k, *n);
                }
            });
            if ti != m.kind.index() {
                m.kind = MotionKind::default_of(ti);
            }
            ui.end_row();
            match &mut m.kind {
                MotionKind::Fft { band, amplitude, offset, release } => {
                    widgets::label(ui, "Band");
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("dance-band").selected_text(band_label(*band)).height(300.0).show_ui(ui, |ui| {
                            for b in 0..BANDS {
                                ui.selectable_value(band, b, band_label(b));
                            }
                        });
                        if let Some(s) = &spectrum {
                            let level = s.level(*band, now) as f32;
                            let (r, _) = ui.allocate_exact_size(vec2(40.0, 10.0), Sense::hover());
                            ui.painter().rect_filled(r, 2, theme::SURFACE);
                            ui.painter().rect_filled(Rect::from_min_size(r.min, vec2(r.width() * level, r.height())), 2, theme::ACCENT);
                        }
                    });
                    ui.end_row();
                    widgets::drag(ui, "Amplitude", amplitude, 0.05);
                    widgets::drag(ui, "Offset", offset, 0.01);
                    widgets::slider(ui, "Release (s)", release, 0.0..=2.0).on_hover_text("How slowly the value falls back after a peak");
                }
                MotionKind::Sine { min, max, rpm } | MotionKind::SawTooth { min, max, rpm } => {
                    widgets::drag(ui, "From", min, 0.05);
                    widgets::drag(ui, "To", max, 0.05);
                    widgets::drag(ui, "Per minute", rpm, 0.1);
                }
            }
            widgets::drag(ui, "Start (s)", &mut m.start, 0.1);
            widgets::drag(ui, "End (s)", &mut m.end, 0.1).on_hover_text("0 = until the end");
        });
        ui.label(
            RichText::new("The value is added to every linked property; rotations are in degrees, zoom adds to 1.").small().color(theme::TEXT_MUTED),
        );

        theme::section_label(ui, "Links");
        let mut unlink = None;
        for (li, l) in m.links.iter().enumerate() {
            let Some(f) = flames.get(l.flame) else { continue };
            let label = targets_label(f, &l.target);
            ui.horizontal(|ui| {
                if widgets::icon_button(ui, icons::X, "Remove the link").clicked() {
                    unlink = Some(li);
                }
                ui.label(RichText::new(format!("{} · {label}", flame_name(f, l.flame))).small());
            });
        }
        if let Some(li) = unlink {
            m.links.remove(li);
        }
        if flames.is_empty() {
            return;
        }
        ui.add_space(4.0);
        if self.link_flame >= flames.len() {
            self.link_flame = self.current.min(flames.len() - 1);
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("Link to").color(theme::TEXT_MUTED));
            egui::ComboBox::from_id_salt("dance-link-flame").selected_text(flame_name(&flames[self.link_flame], self.link_flame)).show_ui(ui, |ui| {
                for (i, f) in flames.iter().enumerate() {
                    ui.selectable_value(&mut self.link_flame, i, flame_name(f, i));
                }
            });
        });
        ui.add(egui::TextEdit::singleline(&mut self.link_filter).hint_text("Filter properties, e.g. \"rotate\" or \"brightness\""));
        let filter = self.link_filter.to_lowercase();
        let flame_idx = self.link_flame;
        egui::ScrollArea::vertical().id_salt("dance-link-targets").max_height(180.0).show(ui, |ui| {
            for (target, label) in targets(&flames[flame_idx]) {
                if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                    continue;
                }
                let linked = m.links.iter().any(|l| l.flame == flame_idx && l.target == target);
                if ui.add_enabled(!linked, egui::Button::new(RichText::new(&label).small()).frame(false)).clicked() {
                    m.links.push(Link { flame: flame_idx, target });
                }
            }
        });
    }

    // ---- export ---------------------------------------------------------------------------

    fn export_panel(&mut self, ui: &mut Ui, gpu: &Option<GpuContext>) {
        theme::section_label(ui, "Recording");
        let perf = &self.project.performance;
        if perf.is_empty() {
            ui.label(
                RichText::new("Press Record and switch flames while the music plays; the switches are recorded and can be rendered into a video.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        } else {
            ui.label(
                RichText::new(format!("{} – {} · {} switches", format_time(perf.start), format_time(perf.end), perf.cues.len().saturating_sub(1)))
                    .color(theme::TEXT),
            );
        }
        let running = self.worker.is_some();
        ui.add_enabled_ui(!running, |ui| {
            grid(ui, "dance-out", |ui| {
                widgets::label(ui, "Size");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.project.width).range(16..=8192));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut self.project.height).range(16..=8192));
                });
                ui.end_row();
                widgets::label(ui, "Frames/s");
                ui.add(egui::DragValue::new(&mut self.project.fps).range(1.0..=120.0));
                ui.end_row();
                widgets::label(ui, "Quality");
                ui.add(egui::DragValue::new(&mut self.project.quality).range(1.0..=10000.0).speed(5.0));
                ui.end_row();
                widgets::label(ui, "Format");
                egui::ComboBox::from_id_salt("dance-format").selected_text(self.format.label()).show_ui(ui, |ui| {
                    for f in VideoFormat::ALL {
                        if ui.selectable_value(&mut self.format, *f, f.label()).changed() {
                            self.output = None;
                        }
                    }
                });
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.button("Choose output…").clicked() {
                    self.output = match self.format.extension() {
                        Some(ext) => rfd::FileDialog::new()
                            .add_filter(self.format.label(), &[ext])
                            .set_file_name(format!("dance.{ext}"))
                            .save_file()
                            .map(|p| p.with_extension(ext)),
                        None => rfd::FileDialog::new().set_title("Folder for the PNG frames").pick_folder(),
                    };
                }
                ui.add_enabled(gpu.is_some(), egui::Checkbox::new(&mut self.on_gpu, "GPU"));
            });
            let path = self.output.as_ref().map_or("Output not chosen".into(), |p| p.display().to_string());
            ui.add(egui::Label::new(RichText::new(&path).small().color(theme::TEXT_MUTED)).truncate()).on_hover_text(path);
        });
        let ffmpeg = *self.ffmpeg.get_or_insert_with(rwf_core::video::ffmpeg_available);
        let needs_ffmpeg = self.format != VideoFormat::PngSequence;
        if needs_ffmpeg && !ffmpeg {
            ui.label(RichText::new("Video files need ffmpeg in the PATH; PNG sequences work without it.").small().color(theme::WARN));
        }
        ui.add_space(6.0);
        if let Some(w) = &self.worker {
            let stopping = w.cancel.load(Ordering::Relaxed);
            if ui
                .add_enabled(!stopping, egui::Button::new(if stopping { "Stopping…" } else { "Stop" }).min_size(vec2(ui.available_width(), 28.0)))
                .clicked()
            {
                w.cancel.store(true, Ordering::Relaxed);
            }
            if let Some((done, total)) = self.progress {
                ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32).text(format!("Frame {done} / {total}")));
            }
        } else if ui
            .add_enabled(
                !self.project.performance.is_empty() && self.output.is_some() && (ffmpeg || !needs_ffmpeg),
                egui::Button::new(RichText::new(format!("{}  Render video", icons::FILM_STRIP)).strong())
                    .fill(theme::ACCENT_DIM)
                    .min_size(vec2(ui.available_width(), 30.0)),
            )
            .clicked()
        {
            self.start_export(if self.on_gpu { gpu.clone() } else { None });
        }
        if let Some(s) = &self.status {
            ui.add_space(4.0);
            ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
        }
    }

    fn start_export(&mut self, gpu: Option<GpuContext>) {
        let Some(path) = self.output.clone() else { return };
        self.pause();
        let project = self.project.clone();
        let spectrum = self.music.as_ref().map(|m| m.spectrum.clone());
        let format = self.format;
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let c2 = cancel.clone();
        self.progress = Some((0, project.performance_frames()));
        self.status = None;
        std::thread::spawn(move || {
            // An own device keeps the show responsive.
            let device = gpu.and_then(|_| GpuContext::new_headless().ok());
            let tx2 = tx.clone();
            let r = rwf_gpu::dance::render_performance(&project, spectrum.as_deref(), &path, format, device, &c2, |index, total, _| {
                let _ = tx2.send(Msg::Frame { index, total });
            });
            let _ = tx.send(Msg::Done(r.map(|_| path)));
        });
        self.worker = Some(Worker { cancel, msgs: rx, started: Instant::now() });
    }

    // ---- transport ------------------------------------------------------------------------

    fn transport(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            if ui.button(format!("{} Music…", icons::PLUS)).on_hover_text("MP3, WAV, FLAC or Ogg").clicked()
                && let Some(p) = rfd::FileDialog::new().add_filter("Music", &["mp3", "wav", "flac", "ogg"]).pick_file()
            {
                self.pause();
                self.load_music(p);
            }
            let name = self.music.as_ref().and_then(|m| m.path.file_name()).map_or("No music".into(), |n| n.to_string_lossy().to_string());
            ui.label(RichText::new(name).color(if self.music.is_some() { theme::TEXT } else { theme::TEXT_MUTED }));
            ui.separator();
            if widgets::icon_button(ui, icons::SKIP_BACK, "Back to the start").clicked() {
                self.seek(0.0);
            }
            let playing = self.is_playing();
            if ui.button(if playing { format!("{}  Pause", icons::PAUSE) } else { format!("{}  Play", icons::PLAY) }).on_hover_text("Space").clicked()
            {
                if playing { self.pause() } else { self.play() }
            }
            let rec = self.recording.is_some();
            let rec_text = RichText::new(format!("{}  {}", icons::RECORD, if rec { "Recording" } else { "Record" })).color(if rec {
                Color32::from_rgb(0xff, 0x55, 0x55)
            } else {
                theme::TEXT
            });
            if ui.button(rec_text).on_hover_text("Record the flame switches (replaces the last recording)").clicked() {
                if rec { self.stop_recording() } else { self.start_recording() }
            }
            let now = self.now();
            let duration = self.music.as_ref().map(|m| m.player.duration());
            match duration {
                Some(d) => {
                    let mut t = now;
                    let w = 220.0f32.min(ui.available_width() * 0.35);
                    ui.spacing_mut().slider_width = w;
                    if ui.add(egui::Slider::new(&mut t, 0.0..=d.max(0.01)).show_value(false)).changed() {
                        self.seek(t);
                    }
                    ui.label(RichText::new(format!("{} / {}", format_time(now), format_time(d))).monospace().small());
                }
                None => {
                    ui.label(RichText::new(format_time(now)).monospace().small());
                }
            }
            ui.separator();
            if ui.checkbox(&mut self.muted, "Mute").changed()
                && let Some(m) = &self.music
            {
                m.player.set_muted(self.muted);
            }
            ui.label(RichText::new("Morph").color(theme::TEXT_MUTED));
            ui.add(egui::DragValue::new(&mut self.project.morph_frames).range(0..=240).suffix(" fr"))
                .on_hover_text("Length of the morph when switching flames (frames at the export frame rate)");
            ui.label(RichText::new("Detail").color(theme::TEXT_MUTED));
            ui.add(egui::Slider::new(&mut self.resolution, 0.2..=1.0).show_value(false)).on_hover_text("Resolution of the realtime view");
            ui.checkbox(&mut self.show_spectrum, "Spectrum");
            if let Some(s) = &self.sound_status {
                ui.label(RichText::new(s).small().color(theme::WARN));
            }
        });
    }

    // ---- realtime view --------------------------------------------------------------------

    fn view(&mut self, ui: &mut Ui, frame: &eframe::Frame, gpu: Option<GpuContext>) {
        let avail = ui.available_rect_before_wrap();
        let painter = ui.painter_at(avail);
        let dt = self.last_frame.elapsed().as_secs_f64();
        self.last_frame = Instant::now();
        if self.project.flames.is_empty() {
            painter.text(avail.center(), Align2::CENTER_CENTER, "Add flames on the left to start", FontId::proportional(14.0), theme::TEXT_MUTED);
            return;
        }
        let Some(gpu) = gpu else {
            painter.text(avail.center(), Align2::CENTER_CENTER, "The realtime view needs a GPU", FontId::proportional(14.0), theme::WARN);
            return;
        };
        let now = self.now();
        let morph_s = self.project.morph_seconds();
        let morph = self.morph.and_then(|(from, t0)| {
            let x = if morph_s > 1e-6 { (now - t0) / morph_s } else { 1.0 };
            (0.0..1.0).contains(&x).then_some((from, x))
        });
        if morph.is_none() {
            self.morph = None;
        }
        let Some(flame) = self.project.flame_at((self.current, morph), self.spectrum(), now) else { return };
        let flame = realtime_flame(flame);
        let aspect = flame.width.max(1) as f32 / flame.height.max(1) as f32;
        let img_rect = fit(avail.shrink(8.0), aspect);
        let ppp = ui.ctx().pixels_per_point() * self.resolution;
        let (w, h) = (((img_rect.width() * ppp) as u32).max(16), ((img_rect.height() * ppp) as u32).max(16));

        let playing = self.is_playing();
        let r = self.renderer.get_or_insert_with(|| GpuRenderer::new(gpu));
        r.set_flame(&flame, w, h);
        // Work per frame: aim at about 30 frames per second while playing.
        if playing || morph.is_some() {
            let ms = dt * 1000.0;
            if ms < 28.0 {
                self.iters = (self.iters + self.iters / 4 + 1).min(1024);
            } else if ms > 40.0 {
                self.iters = ((self.iters as f64 * 30.0 / ms) as u32).max(1);
            }
            self.fps = self.fps * 0.9 + 0.1 / dt.max(1e-3);
        }
        if r.last_error().is_none() && (playing || morph.is_some() || r.density() < 400.0) {
            r.iterate(1, self.iters);
            r.tonemap();
            ui.ctx().request_repaint();
        }
        if let Some(e) = r.last_error() {
            painter.text(avail.center(), Align2::CENTER_CENTER, e, FontId::proportional(12.0), theme::WARN);
            return;
        }
        let generation = r.output_generation();
        let (Some(view), Some(_)) = (r.output_view().cloned(), r.output_size()) else { return };
        let density = r.density();
        if let Some(rs) = frame.wgpu_render_state() {
            let mut renderer = rs.renderer.write();
            match self.texture {
                Some((_, g)) if g == generation => {}
                Some((id, _)) => {
                    renderer.update_egui_texture_from_wgpu_texture(&rs.device, &view, wgpu::FilterMode::Linear, id);
                    self.texture = Some((id, generation));
                }
                None => {
                    let id = renderer.register_native_texture(&rs.device, &view, wgpu::FilterMode::Linear);
                    self.texture = Some((id, generation));
                }
            }
        }
        if let Some((id, _)) = self.texture {
            painter.image(id, img_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }

        // Overlays: spectrum, status chip.
        if self.show_spectrum
            && let Some(s) = self.spectrum()
        {
            let levels = s.levels(now);
            let h = 48.0;
            let base = Rect::from_min_max(
                pos2(img_rect.left() + 12.0, img_rect.bottom() - h - 12.0),
                pos2(img_rect.left() + 12.0 + 6.0 * BANDS as f32, img_rect.bottom() - 12.0),
            );
            painter.rect_filled(base.expand(4.0), 4, Color32::from_black_alpha(120));
            for (b, l) in levels.iter().enumerate() {
                let x = base.left() + b as f32 * 6.0;
                let top = base.bottom() - h * l;
                painter.rect_filled(Rect::from_min_max(pos2(x, top), pos2(x + 4.0, base.bottom())), 1, theme::ACCENT.gamma_multiply(0.5 + 0.5 * l));
            }
        }
        let mut chip = format!("{}  ·  {}", flame_name(&self.project.flames[self.current], self.current), format_time(now));
        if playing {
            chip.push_str(&format!("  ·  {:.0} fps", self.fps));
        } else {
            chip.push_str(&format!("  ·  quality {density:.0}"));
        }
        let galley = painter.layout_no_wrap(chip, FontId::proportional(11.5), theme::TEXT);
        let cr = Rect::from_min_size(img_rect.left_top() + vec2(12.0, 12.0), galley.size() + vec2(16.0, 8.0));
        painter.rect_filled(cr, 6, theme::PANEL.gamma_multiply(0.9));
        painter.galley(cr.min + vec2(8.0, 4.0), galley, theme::TEXT);
        if self.recording.is_some() {
            let c = pos2(img_rect.right() - 24.0, img_rect.top() + 24.0);
            painter.circle_filled(c, 7.0, Color32::from_rgb(0xff, 0x40, 0x40));
            painter.text(c - vec2(14.0, 0.0), Align2::RIGHT_CENTER, "REC", FontId::proportional(12.0), Color32::from_rgb(0xff, 0x70, 0x70));
        }
    }
}

fn fit(avail: Rect, aspect: f32) -> Rect {
    let (aw, ah) = (avail.width(), avail.height());
    let (w, h) = if aw / ah.max(1.0) > aspect { (ah * aspect, ah) } else { (aw, aw / aspect) };
    Rect::from_center_size(avail.center(), vec2(w, h))
}

/// Label of a link target (falls back to the raw path).
fn targets_label(flame: &Flame, target: &str) -> String {
    targets(flame).into_iter().find(|(t, _)| t == target).map_or_else(|| target.to_string(), |(_, l)| l)
}
