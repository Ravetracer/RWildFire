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

//! MutaGen window: a 5 × 5 grid of mutations around a flame; clicking a cell mutates around
//! it, the generations are kept as a history.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, vec2};
use rwf_core::Flame;
use rwf_core::random::mutagen::{GRID, Reference, cell};
use rwf_core::random::mutations::MutationType;
use rwf_core::rng::Rng;

use crate::{icons, theme};

const THUMB_W: u32 = 176;
const THUMB_H: u32 = 99;
const CENTRE: usize = GRID * GRID / 2;

struct Cell {
    flame: Flame,
    texture: egui::TextureHandle,
}

/// One grid of mutations.
struct Generation {
    id: u64,
    cells: Vec<Option<Cell>>,
}

pub struct MutaGen {
    pub open: bool,
    horizontal: MutationType,
    vertical: MutationType,
    history: Vec<Generation>,
    current: usize,
    next_id: u64,
    tx: Sender<(u64, usize, Flame, Vec<u8>)>,
    rx: Receiver<(u64, usize, Flame, Vec<u8>)>,
}

impl Default for MutaGen {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self { open: false, horizontal: MutationType::All, vertical: MutationType::All, history: Vec::new(), current: 0, next_id: 1, tx, rx }
    }
}

fn thumbnail(flame: &Flame) -> Vec<u8> {
    let mut f = flame.clone();
    f.bg_transparency = false;
    let opts = rwf_core::render::RenderOptions { width: THUMB_W, height: THUMB_H, threads: 2, max_density: Some(60.0), seed: 3, zbuffer: false };
    rwf_core::render::CpuRender::start(&f, opts).wait().image(false)
}

impl MutaGen {
    /// Opens the window with a first generation around `flame`.
    pub fn open_with(&mut self, flame: &Flame) {
        self.open = true;
        self.history.clear();
        self.start(flame.clone());
    }

    fn busy(&self) -> bool {
        self.history.get(self.current).is_some_and(|g| g.cells.iter().any(Option::is_none))
    }

    /// A new generation around `base` (drops the generations after the current one).
    fn start(&mut self, base: Flame) {
        if !self.history.is_empty() {
            self.history.truncate(self.current + 1);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.history.push(Generation { id, cells: (0..GRID * GRID).map(|_| None).collect() });
        self.current = self.history.len() - 1;
        let (h, v) = (self.horizontal, self.vertical);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let base = Arc::new(base);
            let reference = Arc::new(Reference::new(&base));
            for i in 0..GRID * GRID {
                let (tx, base, reference) = (tx.clone(), base.clone(), reference.clone());
                std::thread::spawn(move || {
                    let mut rng = Rng::from_entropy();
                    let flame = cell(&base, &reference, i % GRID, i / GRID, h, v, &mut rng);
                    let img = thumbnail(&flame);
                    let _ = tx.send((id, i, flame, img));
                });
            }
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((id, i, flame, img)) = self.rx.try_recv() {
            let Some(g) = self.history.iter_mut().find(|g| g.id == id) else { continue };
            let color = egui::ColorImage::from_rgba_unmultiplied([THUMB_W as usize, THUMB_H as usize], &img);
            let texture = ctx.load_texture(format!("mutagen-{id}-{i}"), color, egui::TextureOptions::LINEAR);
            g.cells[i] = Some(Cell { flame, texture });
        }
        if self.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    /// Shows the window; returns a flame to load into the editor.
    pub fn show(&mut self, ctx: &egui::Context, editor_flame: &Flame) -> Option<Flame> {
        if !self.open {
            return None;
        }
        self.poll(ctx);
        let mut to_editor = None;
        let mut mutate_from = None;
        let mut open = self.open;
        egui::Window::new("MutaGen").open(&mut open).default_size([960.0, 640.0]).resizable(true).collapsible(false).show(ctx, |ui| {
            ui.horizontal(|ui| {
                let combo = |ui: &mut egui::Ui, id: &str, value: &mut MutationType| {
                    egui::ComboBox::from_id_salt(id).width(170.0).height(480.0).selected_text(value.label()).show_ui(ui, |ui| {
                        for &m in MutationType::ALL {
                            ui.selectable_value(value, m, m.label());
                        }
                    });
                };
                ui.label(RichText::new("Horizontal").color(theme::TEXT_MUTED));
                combo(ui, "mg-h", &mut self.horizontal);
                ui.label(RichText::new("Vertical").color(theme::TEXT_MUTED));
                combo(ui, "mg-v", &mut self.vertical);
                ui.separator();
                if ui.button("From editor").on_hover_text("Start over with the flame of the editor").clicked() {
                    self.history.clear();
                    self.start(editor_flame.clone());
                }
                if ui.add_enabled(self.current > 0, egui::Button::new(icons::ARROW_LEFT)).on_hover_text("Previous generation").clicked() {
                    self.current -= 1;
                }
                if ui
                    .add_enabled(self.current + 1 < self.history.len(), egui::Button::new(icons::ARROW_RIGHT))
                    .on_hover_text("Next generation")
                    .clicked()
                {
                    self.current += 1;
                }
                ui.label(RichText::new(format!("Generation {} / {}", self.current + 1, self.history.len().max(1))).color(theme::TEXT_MUTED));
                if self.busy() {
                    ui.spinner();
                }
            });
            ui.label(
                RichText::new("Click a flame to mutate around it (more mutations towards the edges). Right-click for more.")
                    .small()
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(4.0);
            let Some(g) = self.history.get(self.current) else { return };
            let gap = 6.0;
            let avail = ui.available_size();
            let w =
                ((avail.x - gap * (GRID as f32 - 1.0)) / GRID as f32).min((avail.y - gap * (GRID as f32 - 1.0)) / GRID as f32 * 16.0 / 9.0).max(40.0);
            let size = vec2(w, w * 9.0 / 16.0);
            egui::Grid::new("mutagen-grid").spacing([gap, gap]).show(ui, |ui| {
                for (i, slot) in g.cells.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rect, CornerRadius::same(6), theme::BG);
                    match slot {
                        Some(c) => {
                            p.image(
                                c.texture.id(),
                                rect.shrink(1.0),
                                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        None => {
                            p.text(rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(18.0), theme::TEXT_MUTED);
                        }
                    }
                    let stroke = if i == CENTRE {
                        Stroke::new(2.0, theme::ACCENT)
                    } else if resp.hovered() {
                        Stroke::new(2.0, theme::TEXT_MUTED)
                    } else {
                        Stroke::new(1.0, theme::BORDER)
                    };
                    p.rect_stroke(rect, CornerRadius::same(6), stroke, egui::StrokeKind::Inside);
                    if let Some(c) = slot {
                        if resp.clicked() && i != CENTRE {
                            mutate_from = Some(c.flame.clone());
                        }
                        resp.context_menu(|ui| {
                            if ui.button("Open in editor").clicked() {
                                to_editor = Some(c.flame.clone());
                                ui.close();
                            }
                            if ui.button("Mutate around this flame").clicked() {
                                mutate_from = Some(c.flame.clone());
                                ui.close();
                            }
                        });
                    }
                    if (i + 1) % GRID == 0 {
                        ui.end_row();
                    }
                }
            });
        });
        self.open = open;
        if let Some(f) = mutate_from {
            self.start(f);
        }
        to_editor
    }
}
