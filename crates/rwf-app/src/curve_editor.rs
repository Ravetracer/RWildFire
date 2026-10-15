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

//! Curve editor: the keyframes of one animation track as an editable graph (JWildfire's
//! envelope editor), including envelopes from audio files.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use egui::{Align2, Color32, CornerRadius, FontId, PointerButton, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use rwf_core::Flame;
use rwf_core::animation::{self, Interp, Key, Track};
use rwf_core::audio::{self, Audio, BANDS, Shaping};

use crate::{icons, theme};

/// Visible part of the graph (frames × values).
#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

/// Inputs of the cached raw audio envelope.
#[derive(Clone, Copy, PartialEq)]
struct EnvelopeKey {
    band: usize,
    start: f64,
    fps: f64,
    frames: usize,
}

struct AudioState {
    file: Option<PathBuf>,
    loading: Option<Receiver<Result<Audio, String>>>,
    audio: Option<Arc<Audio>>,
    error: Option<String>,
    band: usize,
    /// Position in the audio file (seconds) of the first frame.
    start: f64,
    first_frame: u32,
    shaping: Shaping,
    raw: Option<(EnvelopeKey, Vec<f64>)>,
}

impl Default for AudioState {
    fn default() -> Self {
        Self { file: None, loading: None, audio: None, error: None, band: 1, start: 0.0, first_frame: 0, shaping: Shaping::default(), raw: None }
    }
}

#[derive(Default)]
pub struct CurveEditor {
    pub open: bool,
    /// Property path of the edited track.
    path: Option<String>,
    filter: String,
    selected: Option<usize>,
    view: Option<View>,
    dragging: Option<usize>,
    audio: AudioState,
}

const GRAPH_MARGIN: f32 = 44.0;

fn track<'a>(flame: &'a Flame, path: &str) -> Option<&'a Track> {
    flame.animation.track(path)
}

impl CurveEditor {
    /// Opens the editor, on the track of `path` if given.
    pub fn open_with(&mut self, path: Option<String>) {
        self.open = true;
        if path.is_some() && path != self.path {
            self.path = path;
            self.selected = None;
            self.view = None;
        }
    }

    /// The view that shows all keys and the whole animation.
    fn fit(flame: &Flame, path: &str) -> View {
        let count = flame.animation.frame_count.max(2) as f64;
        let mut x1 = count - 1.0;
        let (mut y0, mut y1) = (f64::MAX, f64::MIN);
        if let Some(t) = track(flame, path) {
            for k in &t.keys {
                x1 = x1.max(k.frame);
                y0 = y0.min(k.value);
                y1 = y1.max(k.value);
            }
        }
        if y0 > y1 {
            let v = animation::get_prop(flame, path).unwrap_or(0.0);
            (y0, y1) = (v, v);
        }
        let span = (y1 - y0).max(1e-3).max(y0.abs().max(y1.abs()) * 0.2);
        let (pad_x, pad_y) = (x1 * 0.03 + 1.0, span * 0.12);
        let mid = (y0 + y1) / 2.0;
        View { x0: -pad_x, x1: x1 + pad_x, y0: mid.min(y0) - pad_y - (span - (y1 - y0)) / 2.0, y1: mid.max(y1) + pad_y + (span - (y1 - y0)) / 2.0 }
    }

    /// Shows the window. `playhead` is the timeline frame (the graph moves it on a click);
    /// returns true if the animation changed.
    pub fn show(&mut self, ctx: &egui::Context, flame: &mut Flame, playhead: &mut f64) -> bool {
        if !self.open {
            return false;
        }
        self.poll_audio(ctx);
        let mut changed = false;
        let mut open = self.open;
        egui::Window::new("Curve editor").open(&mut open).default_size([860.0, 540.0]).resizable(true).collapsible(false).show(ctx, |ui| {
            changed |= self.toolbar(ui, flame);
            if let Some(path) = self.path.clone() {
                changed |= self.audio_panel(ui, flame, &path);
                ui.add_space(4.0);
                changed |= self.key_fields(ui, flame, &path);
                let graph_h = ui.available_height().max(200.0);
                changed |= self.graph(ui, flame, &path, playhead, graph_h);
            } else {
                ui.set_min_size(vec2(640.0, 360.0));
                ui.add_space(40.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Choose a property to edit its curve.").color(theme::TEXT_MUTED));
                });
            }
        });
        self.open = open;
        if changed {
            animation::prune_tracks(flame);
        }
        changed
    }

    fn toolbar(&mut self, ui: &mut Ui, flame: &mut Flame) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            let current = self.path.as_deref().map_or("Choose property…".to_string(), |p| animation::label(flame, p));
            let btn = ui.button(format!("{current}  {}", icons::CARET_DOWN));
            egui::Popup::from_toggle_button_response(&btn).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                ui.set_min_width(300.0);
                let edit = ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter properties…").desired_width(290.0));
                edit.request_focus();
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let filter = self.filter.to_lowercase();
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    let mut props: Vec<(String, String, bool)> = animation::all_props(flame)
                        .into_iter()
                        .map(|(p, _)| {
                            let animated = track(flame, &p).is_some_and(|t| !t.keys.is_empty());
                            (animation::label(flame, &p), p, animated)
                        })
                        .filter(|(l, p, _)| filter.is_empty() || l.to_lowercase().contains(&filter) || p.to_lowercase().contains(&filter))
                        .collect();
                    props.sort_by_key(|(_, _, animated)| !animated);
                    if enter && let Some((_, p, _)) = props.first() {
                        self.path = Some(p.clone());
                        self.selected = None;
                        self.view = None;
                        ui.close();
                        return;
                    }
                    for (label, p, animated) in props {
                        let text =
                            if animated { RichText::new(format!("{} {label}", icons::DIAMOND)).color(theme::ACCENT) } else { RichText::new(label) };
                        if ui.selectable_label(self.path.as_deref() == Some(&p), text).clicked() {
                            self.path = Some(p);
                            self.selected = None;
                            self.view = None;
                            ui.close();
                        }
                    }
                });
            });
            let Some(path) = self.path.clone() else { return };
            if let Some(t) = flame.animation.tracks.iter_mut().find(|t| t.path == path) {
                egui::ComboBox::from_id_salt("curve-interp").width(80.0).selected_text(t.interp.as_str()).show_ui(ui, |ui| {
                    for i in Interp::ALL {
                        changed |= ui.selectable_value(&mut t.interp, i, i.as_str()).on_hover_text(i.help()).changed();
                    }
                });
                if ui.button("Smooth").on_hover_text("Average every key with its neighbours").clicked() {
                    t.smooth();
                    changed = true;
                }
            }
            if ui.button(format!("{}  Fit", icons::FIT)).on_hover_text("Show all keys (double-click the graph background)").clicked() {
                self.view = None;
            }
            ui.menu_button("Transform", |ui| {
                let id = ui.id().with("curve-transform");
                let (mut scale, mut offset) = ui.data_mut(|d| *d.get_temp_mut_or(id, (1.0f64, 0.0f64)));
                egui::Grid::new("curve-transform-grid").num_columns(2).show(ui, |ui| {
                    ui.label("Scale");
                    ui.add(egui::DragValue::new(&mut scale).speed(0.01));
                    ui.end_row();
                    ui.label("Offset");
                    ui.add(egui::DragValue::new(&mut offset).speed(0.01));
                    ui.end_row();
                });
                ui.data_mut(|d| d.insert_temp(id, (scale, offset)));
                ui.label(RichText::new("Scales the values around their mean.").small().color(theme::TEXT_MUTED));
                if ui.button("Apply").clicked()
                    && let Some(t) = flame.animation.tracks.iter_mut().find(|t| t.path == path)
                    && !t.keys.is_empty()
                {
                    let mean = t.keys.iter().map(|k| k.value).sum::<f64>() / t.keys.len() as f64;
                    t.scale_values(mean, scale, offset);
                    changed = true;
                    ui.close();
                }
            });
            if ui.button(format!("{}  Delete curve", icons::X)).clicked() {
                flame.animation.tracks.retain(|t| t.path != path);
                self.selected = None;
                changed = true;
            }
        });
        changed
    }

    fn key_fields(&mut self, ui: &mut Ui, flame: &mut Flame, path: &str) -> bool {
        let mut changed = false;
        ui.horizontal(|ui| {
            let Some(t) = flame.animation.tracks.iter_mut().find(|t| t.path == path) else {
                ui.label(RichText::new("No keys yet: double-click the graph to add one.").small().color(theme::TEXT_MUTED));
                return;
            };
            let Some(i) = self.selected.filter(|i| *i < t.keys.len()) else {
                ui.label(
                    RichText::new(format!("{} keys. Click a key to edit it, double-click to add one, right-click to delete.", t.keys.len()))
                        .small()
                        .color(theme::TEXT_MUTED),
                );
                return;
            };
            ui.label(RichText::new("Frame").color(theme::TEXT_MUTED));
            let mut frame = t.keys[i].frame;
            if ui.add(egui::DragValue::new(&mut frame).speed(0.2).range(0.0..=1.0e6).max_decimals(2)).changed()
                && !t.keys.iter().enumerate().any(|(j, k)| j != i && (k.frame - frame).abs() < 1e-6)
            {
                t.keys[i].frame = frame;
                self.selected = Some(sort_keys(t, i));
                changed = true;
            }
            let i = self.selected.unwrap_or(i);
            ui.label(RichText::new("Value").color(theme::TEXT_MUTED));
            changed |= ui.add(egui::DragValue::new(&mut t.keys[i].value).speed(0.01).max_decimals(5)).changed();
            if ui.button(format!("{}  Delete key", icons::X)).clicked() {
                t.keys.remove(i);
                self.selected = None;
                changed = true;
            }
        });
        changed
    }

    fn graph(&mut self, ui: &mut Ui, flame: &mut Flame, path: &str, playhead: &mut f64, height: f32) -> bool {
        let mut changed = false;
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
        let p = ui.painter_at(rect);
        p.rect_filled(rect, CornerRadius::same(6), theme::BG);
        let plot = Rect::from_min_max(pos2(rect.left() + GRAPH_MARGIN + 12.0, rect.top() + 8.0), pos2(rect.right() - 10.0, rect.bottom() - 22.0));
        let mut v = *self.view.get_or_insert_with(|| Self::fit(flame, path));

        // Zoom (wheel: frames, Shift + wheel: values) and pan (middle or right drag).
        if resp.hovered() {
            let (scroll, shift) = ui.input(|i| (i.smooth_scroll_delta.y + i.smooth_scroll_delta.x, i.modifiers.shift));
            if scroll != 0.0
                && let Some(m) = resp.hover_pos()
            {
                let f = (-scroll as f64 * 0.003).exp();
                if shift {
                    let my = v.y1 - (m.y - plot.top()) as f64 / plot.height() as f64 * (v.y1 - v.y0);
                    (v.y0, v.y1) = (my + (v.y0 - my) * f, my + (v.y1 - my) * f);
                } else {
                    let mx = v.x0 + (m.x - plot.left()) as f64 / plot.width() as f64 * (v.x1 - v.x0);
                    (v.x0, v.x1) = (mx + (v.x0 - mx) * f, mx + (v.x1 - mx) * f);
                }
            }
        }
        if resp.dragged_by(PointerButton::Middle) {
            let d = resp.drag_delta();
            let (dx, dy) = (d.x as f64 / plot.width() as f64 * (v.x1 - v.x0), d.y as f64 / plot.height() as f64 * (v.y1 - v.y0));
            (v.x0, v.x1, v.y0, v.y1) = (v.x0 - dx, v.x1 - dx, v.y0 + dy, v.y1 + dy);
        }
        self.view = Some(v);
        let to_screen = |f: f64, val: f64| {
            pos2(plot.left() + ((f - v.x0) / (v.x1 - v.x0)) as f32 * plot.width(), plot.top() + ((v.y1 - val) / (v.y1 - v.y0)) as f32 * plot.height())
        };
        let from_screen = |q: Pos2| {
            (
                v.x0 + (q.x - plot.left()) as f64 / plot.width() as f64 * (v.x1 - v.x0),
                v.y1 - (q.y - plot.top()) as f64 / plot.height() as f64 * (v.y1 - v.y0),
            )
        };

        // Animation range and grid.
        let count = flame.animation.frame_count.max(1) as f64;
        let range = Rect::from_min_max(
            pos2(to_screen(0.0, 0.0).x.max(plot.left()), plot.top()),
            pos2(to_screen(count - 1.0, 0.0).x.min(plot.right()), plot.bottom()),
        );
        if range.width() > 0.0 {
            p.rect_filled(range, CornerRadius::ZERO, theme::SURFACE.gamma_multiply(0.4));
        }
        let grid = Stroke::new(1.0, theme::BORDER.gamma_multiply(0.7));
        let font = FontId::proportional(10.0);
        for f in nice_steps(v.x0, v.x1, (plot.width() / 70.0) as usize) {
            let x = to_screen(f, 0.0).x;
            p.line_segment([pos2(x, plot.top()), pos2(x, plot.bottom())], grid);
            p.text(pos2(x, plot.bottom() + 4.0), Align2::CENTER_TOP, format_num(f), font.clone(), theme::TEXT_MUTED);
        }
        for val in nice_steps(v.y0, v.y1, (plot.height() / 40.0) as usize) {
            let y = to_screen(0.0, val).y;
            p.line_segment([pos2(plot.left(), y), pos2(plot.right(), y)], grid);
            p.text(pos2(plot.left() - 6.0, y), Align2::RIGHT_CENTER, format_num(val), font.clone(), theme::TEXT_MUTED);
        }

        // Audio preview, then the curve.
        if let Some(values) = self.audio_preview(flame) {
            let first = self.audio.first_frame as f64;
            let pts: Vec<Pos2> = values.iter().enumerate().map(|(i, &val)| to_screen(first + i as f64, val)).collect();
            p.add(egui::Shape::line(pts, Stroke::new(1.0, Color32::from_rgb(0x5a, 0x9b, 0xd5).gamma_multiply(0.7))));
        }
        let keys: Vec<Key> = track(flame, path).map(|t| t.keys.clone()).unwrap_or_default();
        if let Some(t) = track(flame, path).filter(|t| !t.keys.is_empty()) {
            let n = plot.width().max(2.0) as usize;
            let pts: Vec<Pos2> = (0..=n)
                .filter_map(|i| {
                    let f = v.x0 + (v.x1 - v.x0) * i as f64 / n as f64;
                    t.eval(f).map(|val| to_screen(f, val))
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(1.8, theme::ACCENT)));
        } else if let Some(val) = animation::get_prop(flame, path) {
            let y = to_screen(0.0, val).y;
            p.line_segment([pos2(plot.left(), y), pos2(plot.right(), y)], Stroke::new(1.0, theme::TEXT_MUTED));
        }
        let hit = |q: Pos2| {
            keys.iter()
                .enumerate()
                .map(|(i, k)| (i, to_screen(k.frame, k.value).distance(q)))
                .filter(|(_, d)| *d < 7.0)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i)
        };
        // Dense keys (audio curves) as small dots.
        let px_per_frame = plot.width() / (v.x1 - v.x0) as f32;
        let r = if px_per_frame >= 10.0 { 4.0 } else { 2.0 };
        for (i, k) in keys.iter().enumerate() {
            let c = to_screen(k.frame, k.value);
            if plot.expand(4.0).contains(c) {
                let sel = self.selected == Some(i);
                p.circle(
                    c,
                    if sel { 5.5 } else { r },
                    if sel { theme::TEXT } else { theme::ACCENT },
                    if r > 2.0 || sel { Stroke::new(1.0, theme::BG) } else { Stroke::NONE },
                );
            }
        }
        // Playhead.
        let px = to_screen(*playhead, 0.0).x;
        if (plot.left()..=plot.right()).contains(&px) {
            p.line_segment([pos2(px, plot.top()), pos2(px, plot.bottom())], Stroke::new(1.2, theme::TEXT_MUTED));
        }

        // Interaction.
        let pointer = resp.interact_pointer_pos();
        if resp.drag_started_by(PointerButton::Primary) {
            // The drag starts some pixels away from where the button went down.
            let origin = ui.input(|i| i.pointer.press_origin()).or(pointer);
            self.dragging = origin.and_then(hit);
            if self.dragging.is_some() {
                self.selected = self.dragging;
            }
        }
        if let (Some(i), Some(q)) = (self.dragging, pointer)
            && resp.dragged_by(PointerButton::Primary)
            && let Some(t) = flame.animation.tracks.iter_mut().find(|t| t.path == path)
            && i < t.keys.len()
        {
            let (f, val) = from_screen(q);
            let f = f.round().max(0.0);
            if !t.keys.iter().enumerate().any(|(j, k)| j != i && (k.frame - f).abs() < 1e-6) {
                t.keys[i].frame = f;
            }
            t.keys[i].value = val;
            let i = sort_keys(t, i);
            self.dragging = Some(i);
            self.selected = Some(i);
            changed = true;
        }
        if resp.drag_stopped() {
            self.dragging = None;
        }
        if resp.double_clicked()
            && let Some(q) = pointer
        {
            if hit(q).is_none() && plot.contains(q) {
                let (f, val) = from_screen(q);
                let t = flame.animation.track_mut(path);
                t.set_key(f.round().max(0.0), val);
                self.selected = t.keys.iter().position(|k| (k.frame - f.round().max(0.0)).abs() < 1e-6);
                changed = true;
            }
        } else if resp.clicked()
            && let Some(q) = pointer
        {
            match hit(q) {
                Some(i) => self.selected = Some(i),
                None => {
                    self.selected = None;
                    *playhead = from_screen(q).0.round().clamp(0.0, count - 1.0);
                }
            }
        }
        if resp.secondary_clicked()
            && let Some(i) = pointer.and_then(hit)
            && let Some(t) = flame.animation.tracks.iter_mut().find(|t| t.path == path)
        {
            t.keys.remove(i);
            self.selected = None;
            changed = true;
        }
        resp.on_hover_text_at_pointer("Double-click: add key · drag: move · right-click: delete · wheel: zoom (Shift: values) · middle drag: pan");
        changed
    }

    fn poll_audio(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.audio.loading else { return };
        match rx.try_recv() {
            Ok(Ok(a)) => {
                self.audio.audio = Some(Arc::new(a));
                self.audio.loading = None;
                self.audio.raw = None;
            }
            Ok(Err(e)) => {
                self.audio.error = Some(e);
                self.audio.loading = None;
            }
            Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
        }
    }

    /// The shaped audio envelope for the current settings (cached raw analysis).
    fn audio_preview(&mut self, flame: &Flame) -> Option<Vec<f64>> {
        let a = self.audio.audio.clone()?;
        let fps = flame.animation.fps.max(1.0);
        let available = ((a.duration() - self.audio.start) * fps).max(0.0) as usize;
        let frames = (flame.animation.frame_count as usize).saturating_sub(self.audio.first_frame as usize).min(available);
        let key = EnvelopeKey { band: self.audio.band, start: self.audio.start, fps, frames };
        if self.audio.raw.as_ref().is_none_or(|(k, _)| *k != key) {
            let band = BANDS.get(self.audio.band).map_or(audio::Band::Level, |b| b.1);
            self.audio.raw = Some((key, a.envelope(band, fps, self.audio.start, frames)));
        }
        self.audio.raw.as_ref().map(|(_, raw)| audio::shape(raw, &self.audio.shaping))
    }

    fn audio_panel(&mut self, ui: &mut Ui, flame: &mut Flame, path: &str) -> bool {
        let mut changed = false;
        egui::CollapsingHeader::new(RichText::new("Audio envelope").strong()).id_salt("curve-audio").default_open(self.audio.audio.is_some()).show(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Open audio…").on_hover_text("MP3, WAV, FLAC or Ogg Vorbis").clicked()
                        && let Some(file) = crate::settings::dialog(crate::settings::Folder::Sound).add_filter("Audio", &["mp3", "wav", "flac", "ogg"]).pick_file()
                    {
                        let (tx, rx) = channel();
                        let f = file.clone();
                        std::thread::spawn(move || {
                            let _ = tx.send(Audio::load(&f).map_err(|e| e.to_string()));
                        });
                        self.audio = AudioState { file: Some(file), loading: Some(rx), ..std::mem::take(&mut self.audio) };
                        self.audio.audio = None;
                        self.audio.error = None;
                    }
                    match (&self.audio.file, &self.audio.audio) {
                        (Some(f), Some(a)) => {
                            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                            ui.label(format!("{name} ({:.1} s)", a.duration()));
                        }
                        (Some(_), None) if self.audio.loading.is_some() => {
                            ui.spinner();
                            ui.label(RichText::new("Decoding…").color(theme::TEXT_MUTED));
                        }
                        _ => {
                            ui.label(RichText::new("The loudness of a frequency band per frame becomes the curve.").small().color(theme::TEXT_MUTED));
                        }
                    }
                    if let Some(e) = &self.audio.error {
                        ui.label(RichText::new(e).color(theme::WARN));
                    }
                });
                ui.add_enabled_ui(self.audio.audio.is_some(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let a = &mut self.audio;
                        let label = |ui: &mut Ui, t: &str| ui.label(RichText::new(t).color(theme::TEXT_MUTED));
                        label(ui, "Band");
                        egui::ComboBox::from_id_salt("curve-band").width(170.0).selected_text(BANDS.get(a.band).map_or("", |b| b.0)).show_ui(
                            ui,
                            |ui| {
                                for (i, (name, _)) in BANDS.iter().enumerate() {
                                    ui.selectable_value(&mut a.band, i, *name);
                                }
                            },
                        );
                        label(ui, "Audio from");
                        ui.add(egui::DragValue::new(&mut a.start).range(0.0..=36_000.0).speed(0.05).suffix(" s"));
                        label(ui, "at frame");
                        ui.add(egui::DragValue::new(&mut a.first_frame).range(0..=1_000_000));
                        ui.separator();
                        label(ui, "Min");
                        ui.add(egui::DragValue::new(&mut a.shaping.min).speed(0.01));
                        label(ui, "Max");
                        ui.add(egui::DragValue::new(&mut a.shaping.max).speed(0.01));
                        label(ui, "Release");
                        ui.add(egui::DragValue::new(&mut a.shaping.release).range(0.0..=100.0).speed(0.1).suffix(" fr"))
                            .on_hover_text("Frames the curve needs to fall back after a peak");
                        label(ui, "Threshold");
                        ui.add(egui::DragValue::new(&mut a.shaping.threshold).range(0.0..=0.95).speed(0.005))
                            .on_hover_text("Share of the loudest frame that still counts as silence");
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .button(RichText::new(format!("{}  Apply to curve", icons::DIAMOND)).strong())
                            .on_hover_text("Replaces the keys of this curve (the blue preview)")
                            .clicked()
                            && let Some(values) = self.audio_preview(flame)
                            && !values.is_empty()
                        {
                            audio::apply_to_track(flame.animation.track_mut(path), &values, self.audio.first_frame as f64);
                            self.selected = None;
                            self.view = None;
                            changed = true;
                        }
                        if ui.button("Fit range to the animation").on_hover_text("Sets the frame count to the length of the audio").clicked()
                            && let Some(a) = &self.audio.audio
                        {
                            let frames = ((a.duration() - self.audio.start) * flame.animation.fps).ceil().max(1.0) as u32 + self.audio.first_frame;
                            flame.animation.frame_count = frames;
                            self.view = None;
                            changed = true;
                        }
                    });
                });
            },
        );
        changed
    }
}

/// Re-sorts the keys after key `i` moved; returns its new index.
fn sort_keys(t: &mut Track, i: usize) -> usize {
    let moved = t.keys[i];
    t.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    t.keys.iter().position(|k| k.frame == moved.frame && k.value == moved.value).unwrap_or(i)
}

/// Round grid positions between `a` and `b`, about `n` of them.
fn nice_steps(a: f64, b: f64, n: usize) -> Vec<f64> {
    let span = (b - a).abs();
    if !span.is_finite() || span <= 0.0 {
        return Vec::new();
    }
    let raw = span / n.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag);
    let first = (a.min(b) / step).ceil() as i64;
    let mut out = Vec::new();
    for k in first.. {
        let v = k as f64 * step;
        if v > a.max(b) || out.len() >= 200 {
            break;
        }
        out.push(v);
    }
    out
}

fn format_num(v: f64) -> String {
    if v.abs() >= 1000.0 || v.fract() == 0.0 {
        format!("{v:.0}")
    } else if v.abs() >= 10.0 {
        format!("{v:.1}")
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_steps_are_round() {
        assert_eq!(nice_steps(0.0, 100.0, 5), vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        assert_eq!(nice_steps(-0.13, 0.13, 4), vec![-0.1, 0.0, 0.1]);
        assert_eq!(nice_steps(-0.13, 0.13, 6).len(), 5);
        assert_eq!(format_num(0.25), "0.25");
        assert_eq!(format_num(120.0), "120");
    }
}
