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

//! Animation timeline: playback, scrubbing, keyframes per track, auto-key.

use std::time::Instant;

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};
use rwf_core::animation::{self, Interp};

use crate::doc::Document;
use crate::{icons, theme};

pub struct Timeline {
    pub open: bool,
    pub frame: f64,
    pub playing: bool,
    pub auto_key: bool,
    last_tick: Instant,
    selected: Option<(String, f64)>,
    shown_frame: Option<f64>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self { open: false, frame: 0.0, playing: false, auto_key: false, last_tick: Instant::now(), selected: None, shown_frame: None }
    }
}

const LABEL_W: f32 = 220.0;
const ROW_H: f32 = 20.0;

fn diamond(p: &egui::Painter, c: Pos2, r: f32, fill: Color32, stroke: Stroke) {
    p.add(Shape::convex_polygon(vec![c + vec2(0.0, -r), c + vec2(r, 0.0), c + vec2(0.0, r), c + vec2(-r, 0.0)], fill, stroke));
}

impl Timeline {
    /// Height the timeline panel needs for the given flame.
    pub fn panel_height(flame: &rwf_core::Flame) -> f32 {
        let lanes = (flame.animation.tracks.len() + 1).min(8) as f32;
        let hint = if flame.animation.tracks.is_empty() { 18.0 } else { 0.0 };
        46.0 + 22.0 + lanes * ROW_H + hint
    }

    /// Advances playback and keeps the document at the current frame.
    pub fn update(&mut self, ctx: &egui::Context, doc: &mut Document) {
        let count = doc.flame.animation.frame_count.max(1) as f64;
        if self.playing {
            let dt = self.last_tick.elapsed().as_secs_f64();
            self.frame += dt * doc.flame.animation.fps;
            if self.frame >= count {
                self.frame %= count;
            }
            ctx.request_repaint();
        }
        self.last_tick = Instant::now();
        self.frame = self.frame.clamp(0.0, count - 1.0);
        let f = if self.playing { self.frame } else { self.frame.round() };
        if self.shown_frame != Some(f) {
            if doc.flame.animation.is_animated() {
                doc.show_frame(f);
            }
            self.shown_frame = Some(f);
        }
    }

    pub fn current_frame(&self) -> f64 {
        if self.playing { self.frame } else { self.frame.round() }
    }

    /// Called before the undo commit: records keyframes for edits when auto-key is on.
    pub fn auto_key(&mut self, doc: &mut Document, pointer_down: bool) {
        if !self.auto_key || pointer_down || self.playing || doc.flame == *doc.committed() {
            return;
        }
        let before = doc.committed().clone();
        let frame = self.current_frame();
        animation::auto_key(&before, &mut doc.flame, frame);
    }

    fn key_paths(doc: &Document, which: &str) -> Vec<String> {
        let all = animation::all_props(&doc.flame);
        let prefix = match (which, doc.selection) {
            ("transform", sel) => match sel.kind() {
                Some((kind, i)) => format!("layer{}.{}{i}.", doc.layer, kind.path_prefix()),
                None => String::new(),
            },
            _ => String::new(),
        };
        all.into_iter()
            .map(|(p, _)| p)
            .filter(|p| match which {
                "camera" => p.starts_with("centre_") || p.starts_with("cam_") || p == "pixels_per_unit",
                "tone" => ["brightness", "contrast", "gamma", "gamma_threshold", "vibrancy", "white_level", "saturation"].contains(&p.as_str()),
                "transform" => !prefix.is_empty() && p.starts_with(&prefix) && !p.contains("yz_") && !p.contains("zx_"),
                _ => false,
            })
            .collect()
    }

    fn key_now(&self, doc: &mut Document, which: &str) {
        let frame = self.current_frame();
        let paths = Self::key_paths(doc, which);
        for p in paths {
            if let Some(v) = animation::get_prop(&doc.flame, &p) {
                doc.flame.animation.track_mut(&p).set_key(frame, v);
            }
        }
    }

    pub fn show(&mut self, ui: &mut Ui, doc: &mut Document) {
        let count = doc.flame.animation.frame_count.max(1);
        // ---- transport ----
        ui.horizontal(|ui| {
            theme::section_label(ui, "Animation");
            ui.add_space(6.0);
            if ui.button(icons::SKIP_BACK).on_hover_text("First frame").clicked() {
                self.frame = 0.0;
            }
            let play = if self.playing { icons::PAUSE } else { icons::PLAY };
            if ui.add(egui::Button::new(RichText::new(play).size(15.0)).fill(theme::ACCENT_DIM)).on_hover_text("Play / pause").clicked() {
                self.playing = !self.playing;
                self.last_tick = Instant::now();
            }
            if ui.button(icons::SKIP_FORWARD).on_hover_text("Last frame").clicked() {
                self.frame = (count - 1) as f64;
            }
            ui.separator();
            ui.label(RichText::new("Frame").color(theme::TEXT_MUTED));
            let mut fr = self.frame.round() as u32;
            if ui.add(egui::DragValue::new(&mut fr).range(0..=count - 1)).changed() {
                self.frame = fr as f64;
                self.playing = false;
            }
            ui.label(RichText::new("of").color(theme::TEXT_MUTED));
            let mut fc = count;
            if ui.add(egui::DragValue::new(&mut fc).range(1..=100_000)).changed() {
                doc.flame.animation.frame_count = fc;
            }
            ui.label(RichText::new("fps").color(theme::TEXT_MUTED));
            ui.add(egui::DragValue::new(&mut doc.flame.animation.fps).range(1.0..=240.0).speed(0.2));
            ui.separator();
            let rec = RichText::new(format!("{} Auto-key", icons::RECORD)).color(if self.auto_key { Color32::from_rgb(0xff, 0x55, 0x55) } else { theme::TEXT });
            ui.toggle_value(&mut self.auto_key, rec).on_hover_text("Every change creates a keyframe at the current frame");
            ui.menu_button(format!("{} Add key", icons::DIAMOND), |ui| {
                if ui.button("Camera").clicked() {
                    self.key_now(doc, "camera");
                    ui.close();
                }
                if ui.button("Selected transform").clicked() {
                    self.key_now(doc, "transform");
                    ui.close();
                }
                if ui.button("Tone (brightness, gamma, …)").clicked() {
                    self.key_now(doc, "tone");
                    ui.close();
                }
            });
            ui.menu_button("Motion blur", |ui| {
                let a = &mut doc.flame.animation;
                egui::Grid::new("mb").num_columns(2).show(ui, |ui| {
                    ui.label("Sub-frames");
                    ui.add(egui::DragValue::new(&mut a.motion_blur_length).range(0..=64));
                    ui.end_row();
                    ui.label("Time step");
                    ui.add(egui::DragValue::new(&mut a.motion_blur_time_step).speed(0.005).range(0.0..=10.0));
                    ui.end_row();
                    ui.label("Decay");
                    ui.add(egui::DragValue::new(&mut a.motion_blur_decay).speed(0.005).range(0.0..=10.0));
                    ui.end_row();
                });
                ui.label(RichText::new("Applied when exporting animations.").small().color(theme::TEXT_MUTED));
            });
            if let Some((path, frame)) = self.selected.clone() {
                ui.separator();
                if let Some(t) = doc.flame.animation.tracks.iter_mut().find(|t| t.path == path) {
                    egui::ComboBox::from_id_salt("interp").selected_text(t.interp.as_str()).width(80.0).show_ui(ui, |ui| {
                        for i in [Interp::Smooth, Interp::Linear, Interp::Hold] {
                            ui.selectable_value(&mut t.interp, i, i.as_str());
                        }
                    });
                }
                if ui.button(format!("{} Delete key", icons::X)).clicked() {
                    if let Some(t) = doc.flame.animation.tracks.iter_mut().find(|t| t.path == path) {
                        t.remove_key_at(frame);
                    }
                    animation::prune_tracks(&mut doc.flame);
                    self.selected = None;
                    self.shown_frame = None;
                }
            }
        });
        ui.add_space(2.0);

        // ---- lanes ----
        let tracks: Vec<(String, Vec<f64>)> = doc.flame.animation.tracks.iter().map(|t| (t.path.clone(), t.keys.iter().map(|k| k.frame).collect())).collect();
        let lanes = tracks.len() + 1;
        let height = (ROW_H * lanes as f32 + 22.0).min(ROW_H * 7.0 + 22.0);
        let width = ui.available_width();
        egui::ScrollArea::vertical().max_height(height).auto_shrink([false, true]).show(ui, |ui| {
            let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H * lanes as f32 + 18.0), Sense::click_and_drag());
            let p = ui.painter_at(rect);
            let lane_x0 = rect.left() + LABEL_W;
            let lane_w = (rect.width() - LABEL_W - 8.0).max(10.0);
            let fx = |f: f64| lane_x0 + lane_w * (f as f32 / (count.max(2) - 1) as f32);
            let ruler = Rect::from_min_size(pos2(lane_x0, rect.top()), vec2(lane_w, 16.0));
            p.rect_filled(Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + 16.0)), CornerRadius::ZERO, theme::BG);
            // ticks
            let step = [1u32, 5, 10, 25, 50, 100, 250, 500, 1000].into_iter().find(|s| lane_w / (count as f32 / *s as f32) > 40.0).unwrap_or(1000);
            let mut f = 0;
            while f < count {
                let x = fx(f as f64);
                p.line_segment([pos2(x, rect.top() + 10.0), pos2(x, rect.bottom())], Stroke::new(1.0, theme::BORDER.gamma_multiply(0.6)));
                p.text(pos2(x + 3.0, rect.top() + 1.0), Align2::LEFT_TOP, f.to_string(), FontId::proportional(10.0), theme::TEXT_MUTED);
                f += step;
            }
            // rows
            let row_y = |i: usize| rect.top() + 18.0 + ROW_H * i as f32 + ROW_H / 2.0;
            p.text(pos2(rect.left() + 6.0, row_y(0)), Align2::LEFT_CENTER, "All keys", FontId::proportional(11.5), theme::TEXT);
            let all_frames = doc.flame.animation.key_frames();
            for fr in &all_frames {
                diamond(&p, pos2(fx(*fr), row_y(0)), 4.5, theme::TEXT_MUTED, Stroke::NONE);
            }
            let mut clicked_key: Option<(String, f64)> = None;
            let pointer = resp.interact_pointer_pos();
            for (i, (path, frames)) in tracks.iter().enumerate() {
                let y = row_y(i + 1);
                if i % 2 == 0 {
                    p.rect_filled(Rect::from_min_max(pos2(rect.left(), y - ROW_H / 2.0), pos2(rect.right(), y + ROW_H / 2.0)), CornerRadius::ZERO, theme::SURFACE.gamma_multiply(0.35));
                }
                let lbl = animation::label(&doc.flame, path);
                p.text(pos2(rect.left() + 6.0, y), Align2::LEFT_CENTER, lbl, FontId::proportional(11.0), theme::TEXT_MUTED);
                for fr in frames {
                    let c = pos2(fx(*fr), y);
                    let sel = self.selected.as_ref().is_some_and(|(sp, sf)| sp == path && (sf - fr).abs() < 1e-6);
                    diamond(&p, c, 5.0, if sel { theme::ACCENT } else { Color32::from_rgb(0xd8, 0xa0, 0x50) }, Stroke::new(1.0, theme::BG));
                    if resp.clicked() && pointer.is_some_and(|pp| pp.distance(c) < 7.0) {
                        clicked_key = Some((path.clone(), *fr));
                    }
                }
            }
            // playhead
            let px = fx(self.current_frame());
            p.line_segment([pos2(px, rect.top()), pos2(px, rect.bottom())], Stroke::new(1.5, theme::ACCENT));
            p.add(Shape::convex_polygon(vec![pos2(px - 5.0, rect.top()), pos2(px + 5.0, rect.top()), pos2(px, rect.top() + 7.0)], theme::ACCENT, Stroke::NONE));

            if let Some(k) = clicked_key {
                self.selected = Some(k.clone());
                self.frame = k.1;
                self.playing = false;
            } else if (resp.clicked() || resp.dragged())
                && let Some(pp) = pointer
                && (pp.x >= lane_x0 || ruler.contains(pp))
            {
                // scrub
                let t = ((pp.x - lane_x0) / lane_w).clamp(0.0, 1.0);
                self.frame = (t * (count.max(2) - 1) as f32) as f64;
                self.playing = false;
                if resp.clicked() {
                    self.selected = None;
                }
            }
        });
        if tracks.is_empty() {
            ui.label(
                RichText::new("No keyframes yet. Turn on Auto-key and change something at another frame, or use Add key.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
        }
    }
}
