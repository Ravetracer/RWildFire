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


//! Bottom "explore" strip: random batches (size, quality, how new batches join the strip,
//! snapshots of the editor's flame), quick mutations of the editor's flame, or the flames of
//! a file; thumbnails are generated in background threads. MutaGen opens from here.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, vec2};
use rwf_core::Flame;
use rwf_core::random::mutations::MutationType;
use rwf_core::random::{Extras, Generator, Quality, generate_quality, quick_mutation};
use rwf_core::rng::Rng;

use crate::ui::widgets;
use crate::{icons, theme};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Random,
    Mutations,
    File,
}

/// Where a new random batch goes (`RandomBatchRefreshType`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RefreshMode {
    /// Replaces the strip.
    #[default]
    Clear,
    /// In front of the strip.
    Insert,
    /// At the end of the strip.
    Append,
}

impl RefreshMode {
    pub const ALL: [RefreshMode; 3] = [RefreshMode::Clear, RefreshMode::Insert, RefreshMode::Append];

    pub fn label(&self) -> &'static str {
        match self {
            RefreshMode::Clear => "Replace the strip",
            RefreshMode::Insert => "Insert in front",
            RefreshMode::Append => "Append at the end",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            RefreshMode::Clear => "clear",
            RefreshMode::Insert => "insert",
            RefreshMode::Append => "append",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::ALL.into_iter().find(|d| d.as_str() == s).unwrap_or_default()
    }
}

const THUMB_W: u32 = 192;
const THUMB_H: u32 = 108;
/// Most thumbnails a strip keeps (inserted or appended batches drop the oldest).
const MAX_SLOTS: usize = 256;

/// A thumbnail; empty while it is being generated.
struct Slot {
    key: u64,
    thumb: Option<(Flame, egui::TextureHandle)>,
}

/// What a batch generates for each of its slots.
#[derive(Clone)]
enum Source {
    Random(Generator, Extras, Quality, Arc<crate::settings::NewFlameDefaults>),
    Mutations(Arc<Flame>, MutationType, f64),
    File(Arc<Vec<Flame>>),
    Snapshot(Arc<Flame>),
}

impl Source {
    fn flame(&self, i: usize, rng: &mut Rng) -> Flame {
        match self {
            Source::Random(g, extras, quality, defaults) => {
                let mut f = generate_quality(*g, *extras, *quality, rng);
                defaults.apply_to_random(&mut f);
                f
            }
            Source::Mutations(base, kind, strength) => quick_mutation(base, *kind, *strength, rng),
            Source::File(flames) => flames[i].clone(),
            Source::Snapshot(f) => (**f).clone(),
        }
    }
}

pub struct Explorer {
    pub mode: Mode,
    pub generator: Generator,
    /// Random symmetry and weighting fields (also used by the "Random flame" button).
    pub extras: Extras,
    pub quality: Quality,
    /// Flames per random batch, and where a new batch goes (Preferences).
    pub batch_size: usize,
    pub refresh_mode: RefreshMode,
    /// Settings of new flames (Preferences), for the random flames.
    pub new_flame: crate::settings::NewFlameDefaults,
    mutation: MutationType,
    mutation_count: usize,
    strength: f64,
    /// The MutaGen button was clicked.
    pub open_mutagen: bool,
    /// The strips of the modes (random, mutations, file).
    strips: [Vec<Slot>; 3],
    /// The running batch of each strip; a batch stops when it no longer matches.
    batches: [Arc<AtomicU64>; 3],
    next_key: u64,
    tx: Sender<(u64, Flame, Vec<u8>)>,
    rx: Receiver<(u64, Flame, Vec<u8>)>,
}

impl Default for Explorer {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            mode: Mode::Random,
            generator: Generator::Classic,
            extras: Extras::default(),
            quality: Quality::NORMAL,
            batch_size: 24,
            refresh_mode: RefreshMode::Clear,
            new_flame: Default::default(),
            mutation: MutationType::All,
            mutation_count: 11,
            strength: 0.5,
            open_mutagen: false,
            strips: Default::default(),
            batches: Default::default(),
            next_key: 1,
            tx,
            rx,
        }
    }
}

fn render_thumb(flame: &Flame) -> Vec<u8> {
    let mut f = flame.clone();
    f.bg_transparency = false;
    let opts = rwf_core::render::RenderOptions { width: THUMB_W, height: THUMB_H, threads: 2, max_density: Some(60.0), seed: 3, zbuffer: false };
    rwf_core::render::CpuRender::start(&f, opts).wait().image(false)
}

fn strip_index(mode: Mode) -> usize {
    match mode {
        Mode::Random => 0,
        Mode::Mutations => 1,
        Mode::File => 2,
    }
}

impl Explorer {
    /// Generates `count` thumbnails from `source` into `strip`, `refresh` decides where they go;
    /// stops the strip's running batch.
    fn start(&mut self, strip: usize, source: Source, count: usize, refresh: RefreshMode) {
        self.cancel(strip);
        let keys: Vec<u64> = (0..count as u64).map(|k| self.next_key + k).collect();
        self.next_key += count as u64;
        let slots = keys.iter().map(|&key| Slot { key, thumb: None });
        let list = &mut self.strips[strip];
        match refresh {
            RefreshMode::Clear => *list = slots.collect(),
            RefreshMode::Insert => {
                list.splice(0..0, slots);
                list.truncate(MAX_SLOTS);
            }
            RefreshMode::Append => {
                list.extend(slots);
                let extra = list.len().saturating_sub(MAX_SLOTS);
                list.drain(..extra);
            }
        }
        let batch = self.batches[strip].load(Ordering::Relaxed);
        let (keys, next) = (Arc::new(keys), Arc::new(AtomicUsize::new(0)));
        let threads = (crate::settings::cpu_threads() / 2).max(1).clamp(1, count.max(1));
        for _ in 0..threads {
            let (tx, token, source, keys, next) = (self.tx.clone(), self.batches[strip].clone(), source.clone(), keys.clone(), next.clone());
            std::thread::spawn(move || {
                let mut rng = Rng::from_entropy();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= keys.len() || token.load(Ordering::Relaxed) != batch {
                        return;
                    }
                    let flame = source.flame(i, &mut rng);
                    let img = render_thumb(&flame);
                    if tx.send((keys[i], flame, img)).is_err() {
                        return;
                    }
                }
            });
        }
    }

    /// Stops the running batch of a strip and drops its unfinished thumbnails.
    fn cancel(&mut self, strip: usize) {
        self.batches[strip].fetch_add(1, Ordering::Relaxed);
        self.strips[strip].retain(|s| s.thumb.is_some());
    }

    fn busy(&self, strip: usize) -> bool {
        self.strips[strip].iter().any(|s| s.thumb.is_none())
    }

    /// Starts (re)filling the strip of the current mode in the background.
    pub fn refresh(&mut self, current: &Flame, library: &[Flame]) {
        match self.mode {
            Mode::Random => {
                let source = Source::Random(self.generator, self.extras, self.quality, Arc::new(self.new_flame.clone()));
                self.start(0, source, self.batch_size.clamp(1, 64), self.refresh_mode);
            }
            Mode::Mutations => {
                let source = Source::Mutations(Arc::new(current.clone()), self.mutation, self.strength);
                self.start(1, source, self.mutation_count.clamp(1, 64), RefreshMode::Clear);
            }
            Mode::File => {
                let flames: Vec<Flame> = library.iter().take(64).cloned().collect();
                let n = flames.len();
                self.start(2, Source::File(Arc::new(flames)), n, RefreshMode::Clear);
            }
        }
    }

    /// Fills the strip of the current mode if it is empty (random and file only).
    pub fn ensure_filled(&mut self, current: &Flame, library: &[Flame]) {
        if self.mode != Mode::Mutations && self.strips[strip_index(self.mode)].is_empty() {
            self.refresh(current, library);
        }
    }

    /// Puts a copy of `flame` in front of the random strip (`snapshotButton_clicked`).
    fn snapshot(&mut self, flame: &Flame) {
        let key = self.next_key;
        self.next_key += 1;
        self.strips[0].insert(0, Slot { key, thumb: None });
        self.strips[0].truncate(MAX_SLOTS);
        let (tx, source) = (self.tx.clone(), Source::Snapshot(Arc::new(flame.clone())));
        std::thread::spawn(move || {
            let flame = source.flame(0, &mut Rng::new(1));
            let img = render_thumb(&flame);
            let _ = tx.send((key, flame, img));
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((key, flame, img)) = self.rx.try_recv() {
            let Some(slot) = self.strips.iter_mut().flatten().find(|s| s.key == key) else { continue };
            let color = egui::ColorImage::from_rgba_unmultiplied([THUMB_W as usize, THUMB_H as usize], &img);
            let tex = ctx.load_texture(format!("thumb-{key}"), color, egui::TextureOptions::LINEAR);
            slot.thumb = Some((flame, tex));
        }
        if (0..3).any(|s| self.busy(s)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    /// Shows the strip; returns a flame the user picked.
    pub fn show(&mut self, ui: &mut egui::Ui, current: &Flame, library: &[Flame]) -> Option<Flame> {
        self.poll(ui.ctx());
        if self.mode == Mode::File && library.is_empty() {
            self.mode = Mode::Random;
        }
        let mut picked = None;
        ui.horizontal(|ui| {
            theme::section_label(ui, "Explore");
            ui.add_space(8.0);
            let before = self.mode;
            ui.selectable_value(&mut self.mode, Mode::Random, "Random").on_hover_text("Batches of random flames");
            ui.selectable_value(&mut self.mode, Mode::Mutations, "Mutations").on_hover_text("Quick mutations of the editor's flame");
            ui.add_enabled_ui(!library.is_empty(), |ui| {
                ui.selectable_value(&mut self.mode, Mode::File, format!("File ({})", library.len())).on_hover_text("The flames of the opened file");
            });
            ui.separator();
            let strip = strip_index(self.mode);
            let busy = self.busy(strip);
            let start = match self.mode {
                Mode::Random => {
                    egui::ComboBox::from_id_salt("explore-gen").selected_text(self.generator.label()).show_ui(ui, |ui| {
                        for g in Generator::ALL {
                            ui.selectable_value(&mut self.generator, *g, g.label());
                        }
                    });
                    ui.label(RichText::new("Symmetry").color(theme::TEXT_MUTED));
                    widgets::symmetry_combo(ui, "explore-symmetry", &mut self.extras.symmetry);
                    ui.label(RichText::new("Weighting").color(theme::TEXT_MUTED));
                    widgets::weighting_combo(ui, "explore-weighting", &mut self.extras.weighting);
                    ui.label(RichText::new("Gradient").color(theme::TEXT_MUTED));
                    widgets::gradient_gen_combo(ui, "explore-gradient", &mut self.extras.gradient);
                    ui.label(RichText::new("Quality").color(theme::TEXT_MUTED));
                    egui::ComboBox::from_id_salt("explore-quality")
                        .width(80.0)
                        .selected_text(self.quality.label())
                        .show_ui(ui, |ui| {
                            for (q, label) in Quality::LEVELS {
                                ui.selectable_value(&mut self.quality, q, label);
                            }
                        })
                        .response
                        .on_hover_text("How hard each random flame tries to fill the frame: higher finds fuller flames but takes longer");
                    let tip = format!("{} random flames ({}); size and placement in Preferences", self.batch_size, self.refresh_mode.label().to_lowercase());
                    let start = batch_button(ui, busy, &format!("{}  New batch", icons::REFRESH), &tip);
                    if ui.button(format!("{}  Snapshot", icons::PLUS)).on_hover_text("Puts a copy of the editor's flame in front of the strip").clicked() {
                        self.snapshot(current);
                    }
                    start
                }
                Mode::Mutations => {
                    egui::ComboBox::from_id_salt("explore-mutation").selected_text(self.mutation.label()).show_ui(ui, |ui| {
                        for m in MutationType::ALL {
                            ui.selectable_value(&mut self.mutation, *m, m.label());
                        }
                    });
                    ui.label(RichText::new("Count").color(theme::TEXT_MUTED));
                    ui.add(egui::DragValue::new(&mut self.mutation_count).range(1..=64)).on_hover_text("Mutations per click");
                    ui.label(RichText::new("Strength").color(theme::TEXT_MUTED));
                    ui.add(egui::DragValue::new(&mut self.strength).speed(0.01).range(0.0..=1.0)).on_hover_text("How far the mutations go");
                    batch_button(ui, busy, &format!("{}  Mutate", icons::SPARKLE), "Mutations of the editor's flame; click one to take it")
                }
                Mode::File => batch_button(ui, busy, "Load thumbnails", "Thumbnails of the flames in the file (up to 64)"),
            };
            if start && busy && before == self.mode {
                self.cancel(strip);
            } else if start || (before != self.mode && self.mode != Mode::Mutations && self.strips[strip_index(self.mode)].is_empty()) {
                self.refresh(current, library);
            }
            if self.busy(strip_index(self.mode)) {
                ui.spinner();
            }
            ui.separator();
            if ui.button(format!("{}  MutaGen…", icons::SPARKLE)).on_hover_text("A grid of mutations of the current flame").clicked() {
                self.open_mutagen = true;
            }
        });
        ui.add_space(4.0);
        let strip = &self.strips[strip_index(self.mode)];
        let size = vec2(THUMB_W as f32, THUMB_H as f32) * 0.85;
        if strip.is_empty() {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), size.y), Sense::hover());
            let text = if self.mode == Mode::Mutations { "Mutate makes variants of the editor's flame." } else { "Empty — start a new batch." };
            ui.painter().text(rect.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(13.0), theme::TEXT_MUTED);
            return None;
        }
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for slot in strip {
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rect, CornerRadius::same(6), theme::BG);
                    match &slot.thumb {
                        Some((_, tex)) => {
                            p.image(tex.id(), rect.shrink(1.0), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        }
                        None => {
                            p.text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(18.0), theme::TEXT_MUTED);
                        }
                    }
                    let stroke = if resp.hovered() { Stroke::new(2.0, theme::ACCENT) } else { Stroke::new(1.0, theme::BORDER) };
                    p.rect_stroke(rect, CornerRadius::same(6), stroke, egui::StrokeKind::Inside);
                    if let Some((f, _)) = &slot.thumb {
                        let tip = if f.name.is_empty() { "Click to open".to_string() } else { format!("{}\nClick to open", f.name) };
                        if resp.on_hover_text(tip).clicked() {
                            picked = Some(f.clone());
                        }
                    }
                }
            });
        });
        picked
    }
}

/// The button that starts a batch; "Cancel" while the strip's batch runs. True when clicked.
fn batch_button(ui: &mut egui::Ui, busy: bool, label: &str, tip: &str) -> bool {
    if busy {
        ui.button(format!("{}  Cancel", icons::X)).on_hover_text("Stops generating; finished thumbnails stay").clicked()
    } else {
        ui.button(label).on_hover_text(tip).clicked()
    }
}
