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

//! Bottom "explore" strip: thumbnails of random flames or the flames of a file, generated in
//! background threads; MutaGen opens from here.

use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Color32, CornerRadius, Sense, Stroke, vec2};
use rwf_core::Flame;
use rwf_core::random::{Generator, generate};
use rwf_core::rng::Rng;

use crate::{icons, theme};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Random,
    File,
}

const THUMB_W: u32 = 192;
const THUMB_H: u32 = 108;
const SLOTS: usize = 8;

struct Thumb {
    flame: Flame,
    texture: Option<egui::TextureHandle>,
}

pub struct Explorer {
    pub mode: Mode,
    pub generator: Generator,
    /// The MutaGen button was clicked.
    pub open_mutagen: bool,
    thumbs: Vec<Option<Thumb>>,
    generation: u64,
    tx: Sender<(u64, usize, Flame, Vec<u8>)>,
    rx: Receiver<(u64, usize, Flame, Vec<u8>)>,
    pending: usize,
}

impl Default for Explorer {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            mode: Mode::Random,
            generator: Generator::Classic,
            open_mutagen: false,
            thumbs: (0..SLOTS).map(|_| None).collect(),
            generation: 0,
            tx,
            rx,
            pending: 0,
        }
    }
}

fn render_thumb(flame: &Flame) -> Vec<u8> {
    let mut f = flame.clone();
    f.bg_transparency = false;
    let opts = rwf_core::render::RenderOptions { width: THUMB_W, height: THUMB_H, threads: 2, max_density: Some(60.0), seed: 3, zbuffer: false };
    rwf_core::render::CpuRender::start(&f, opts).wait().image(false)
}

impl Explorer {
    /// Starts (re)filling all slots in the background.
    pub fn refresh(&mut self, current: &Flame, library: &[Flame]) {
        self.generation += 1;
        let generation = self.generation;
        let count = if self.mode == Mode::File { library.len().min(64) } else { SLOTS };
        self.thumbs = (0..count).map(|_| None).collect();
        self.pending = count;
        for i in 0..count {
            let tx = self.tx.clone();
            let mode = self.mode;
            let generator = self.generator;
            let base = if mode == Mode::File { library[i].clone() } else { current.clone() };
            std::thread::spawn(move || {
                let mut rng = Rng::from_entropy();
                let flame = match mode {
                    Mode::Random => generate(generator, &mut rng),
                    Mode::File => base,
                };
                let img = render_thumb(&flame);
                let _ = tx.send((generation, i, flame, img));
            });
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((generation, i, flame, img)) = self.rx.try_recv() {
            if generation != self.generation || i >= self.thumbs.len() {
                continue;
            }
            let color = egui::ColorImage::from_rgba_unmultiplied([THUMB_W as usize, THUMB_H as usize], &img);
            let tex = ctx.load_texture(format!("thumb-{i}"), color, egui::TextureOptions::LINEAR);
            self.thumbs[i] = Some(Thumb { flame, texture: Some(tex) });
            self.pending = self.pending.saturating_sub(1);
        }
        if self.pending > 0 {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    /// Shows the strip; returns a flame the user picked.
    pub fn show(&mut self, ui: &mut egui::Ui, current: &Flame, library: &[Flame]) -> Option<Flame> {
        self.poll(ui.ctx());
        let mut picked = None;
        ui.horizontal(|ui| {
            theme::section_label(ui, "Explore");
            ui.add_space(8.0);
            let before = self.mode;
            ui.selectable_value(&mut self.mode, Mode::Random, "Random");
            ui.add_enabled_ui(!library.is_empty(), |ui| {
                ui.selectable_value(&mut self.mode, Mode::File, format!("File ({})", library.len()));
            });
            ui.separator();
            match self.mode {
                Mode::Random => {
                    egui::ComboBox::from_id_salt("explore-gen").selected_text(self.generator.label()).show_ui(ui, |ui| {
                        for g in Generator::ALL {
                            ui.selectable_value(&mut self.generator, *g, g.label());
                        }
                    });
                }
                Mode::File => {}
            }
            let refresh_label = if self.mode == Mode::File { "Load thumbnails".to_string() } else { format!("{}  New batch", icons::REFRESH) };
            if ui.button(refresh_label).clicked() || before != self.mode {
                self.refresh(current, library);
            }
            if self.pending > 0 {
                ui.spinner();
            }
            ui.separator();
            if ui.button(format!("{}  MutaGen…", icons::SPARKLE)).on_hover_text("A grid of mutations of the current flame").clicked() {
                self.open_mutagen = true;
            }
        });
        ui.add_space(4.0);
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for slot in &self.thumbs {
                    let size = vec2(THUMB_W as f32, THUMB_H as f32) * 0.85;
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rect, CornerRadius::same(6), theme::BG);
                    match slot.as_ref().and_then(|t| t.texture.as_ref()) {
                        Some(tex) => {
                            p.image(tex.id(), rect.shrink(1.0), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                        }
                        None => {
                            p.text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(18.0), theme::TEXT_MUTED);
                        }
                    }
                    let stroke = if resp.hovered() { Stroke::new(2.0, theme::ACCENT) } else { Stroke::new(1.0, theme::BORDER) };
                    p.rect_stroke(rect, CornerRadius::same(6), stroke, egui::StrokeKind::Inside);
                    if let Some(t) = slot
                        && resp.on_hover_text("Click to open").clicked()
                    {
                        picked = Some(t.flame.clone());
                    }
                }
            });
        });
        picked
    }
}
