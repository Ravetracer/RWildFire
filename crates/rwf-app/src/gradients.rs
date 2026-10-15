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

//! Gradient key-point editor and gradient library browser.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use egui::{Color32, CornerRadius, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use rwf_core::Gradient;
use rwf_core::gradient_io::{NamedGradient, Stop, fit_stops, parse_any, stops_to_gradient, write_ugr};

use crate::ui::widgets;
use crate::{icons, theme};

// ---------------------------------------------------------------------------------------------
// Key-point editor

#[derive(Default)]
pub struct KeyEditor {
    stops: Vec<Stop>,
    selected: Option<usize>,
    last: Option<Gradient>,
}

impl KeyEditor {
    /// Shows the editor for `gradient`; returns true if it changed the gradient.
    pub fn show(&mut self, ui: &mut Ui, gradient: &mut Gradient) -> bool {
        if self.last.as_ref() != Some(gradient) {
            self.stops = fit_stops(gradient, 4.0, 48);
            self.selected = None;
            self.last = Some(gradient.clone());
        }
        let mut changed = false;
        let width = ui.available_width();
        let (rect, bar) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
        widgets::paint_gradient(ui, rect, gradient);
        let (handles_rect, _) = ui.allocate_exact_size(vec2(width, 16.0), Sense::hover());
        let to_x = |p: f64| rect.left() + rect.width() * p as f32;

        // Stop handles
        let mut drag_target: Option<(usize, f32)> = None;
        for (i, s) in self.stops.iter().enumerate() {
            let x = to_x(s.pos);
            let r = Rect::from_center_size(pos2(x, handles_rect.center().y + 1.0), vec2(11.0, 14.0));
            let resp = ui.interact(r, ui.id().with(("stop", i)), Sense::click_and_drag());
            if resp.drag_started() || resp.clicked() {
                self.selected = Some(i);
            }
            if resp.dragged() {
                drag_target = Some((i, resp.drag_delta().x));
            }
            let sel = self.selected == Some(i);
            let p = ui.painter();
            p.line_segment([pos2(x, rect.bottom()), pos2(x, r.top())], Stroke::new(1.0, if sel { theme::ACCENT } else { theme::TEXT_MUTED }));
            p.rect_filled(r, CornerRadius::same(3), Color32::from_rgb(s.color[0], s.color[1], s.color[2]));
            let stroke = if sel { Stroke::new(2.0, theme::ACCENT) } else if resp.hovered() { Stroke::new(1.5, Color32::WHITE) } else { Stroke::new(1.0, theme::BORDER) };
            p.rect_stroke(r, CornerRadius::same(3), stroke, egui::StrokeKind::Outside);
            resp.on_hover_text("Drag to move · click to select");
        }
        if let Some((i, dx)) = drag_target {
            self.stops[i].pos = (self.stops[i].pos + (dx / rect.width()) as f64).clamp(0.0, 1.0);
            changed = true;
        }
        // Click on the bar adds a stop.
        if bar.clicked()
            && let Some(p) = bar.interact_pointer_pos()
        {
            let pos = ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
            let c = gradient.colors[(pos * 255.0).round() as usize];
            self.stops.push(Stop { pos, color: c });
            self.selected = Some(self.stops.len() - 1);
            changed = true;
        }
        bar.on_hover_text("Click to add a color stop");

        // Selected stop properties
        ui.horizontal(|ui| match self.selected.filter(|i| *i < self.stops.len()) {
            Some(i) => {
                let s = &mut self.stops[i];
                let mut col = Color32::from_rgb(s.color[0], s.color[1], s.color[2]);
                if egui::color_picker::color_edit_button_srgba(ui, &mut col, egui::color_picker::Alpha::Opaque).changed() {
                    s.color = [col.r(), col.g(), col.b()];
                    changed = true;
                }
                let mut pct = s.pos * 100.0;
                if ui.add(egui::DragValue::new(&mut pct).range(0.0..=100.0).suffix(" %").speed(0.2).max_decimals(1)).changed() {
                    s.pos = pct / 100.0;
                    changed = true;
                }
                let can_delete = self.stops.len() > 2;
                if ui.add_enabled(can_delete, egui::Button::new(format!("{} Remove", icons::X))).clicked() {
                    self.stops.remove(i);
                    self.selected = None;
                    changed = true;
                }
            }
            None => {
                ui.label(RichText::new(format!("{} stops — click the bar to add, drag to move", self.stops.len())).small().color(theme::TEXT_MUTED));
            }
        });
        ui.horizontal(|ui| {
            if ui.small_button("Even spacing").clicked() && self.stops.len() > 1 {
                let n = self.stops.len() - 1;
                self.stops.sort_by(|a, b| a.pos.total_cmp(&b.pos));
                for (i, s) in self.stops.iter_mut().enumerate() {
                    s.pos = i as f64 / n as f64;
                }
                changed = true;
            }
            if ui.small_button("Simplify").on_hover_text("Fewer stops, same look").clicked() {
                let g = stops_to_gradient(&self.stops);
                self.stops = fit_stops(&g, 10.0, 16);
                self.selected = None;
                changed = true;
            }
        });

        if changed {
            // Keep the selection on the same stop through re-sorting.
            let sel = self.selected.and_then(|i| self.stops.get(i).copied());
            self.stops.sort_by(|a, b| a.pos.total_cmp(&b.pos));
            self.selected = sel.and_then(|s| self.stops.iter().position(|x| *x == s));
            *gradient = stops_to_gradient(&self.stops);
            self.last = Some(gradient.clone());
        }
        changed
    }
}

// ---------------------------------------------------------------------------------------------
// Library

struct Collection {
    name: String,
    gradients: Vec<NamedGradient>,
}

pub struct GradientLibrary {
    pub open: bool,
    collections: Vec<Collection>,
    loading: Option<Receiver<Vec<Collection>>>,
    selected: usize,
    search: String,
    status: Option<String>,
}

impl Default for GradientLibrary {
    fn default() -> Self {
        Self { open: false, collections: Vec::new(), loading: None, selected: 0, search: String::new(), status: None }
    }
}

const BUNDLED: &[(&str, &str, &str)] = &[
    ("flam3 classics", "flam3-palettes.xml", include_str!("../../../assets/gradients/flam3-palettes.xml")),
    ("Floral", "floral.ugr", include_str!("../../../assets/gradients/floral.ugr")),
    ("Sky", "skygradients.ugr", include_str!("../../../assets/gradients/skygradients.ugr")),
    ("Universe", "universe.ugr", include_str!("../../../assets/gradients/universe.ugr")),
    ("Stars", "stargradients.ugr", include_str!("../../../assets/gradients/stargradients.ugr")),
    ("Carr", "carr.ugr", include_str!("../../../assets/gradients/carr.ugr")),
];

/// Folder for user gradients (`<config folder>/gradients`).
pub fn user_dir() -> Option<PathBuf> {
    Some(crate::prefs::config_dir()?.join("gradients"))
}

const USER_FILE: &str = "my-gradients.ugr";

fn is_image(p: &std::path::Path) -> bool {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).is_some_and(|e| matches!(e.as_str(), "png" | "jpg" | "jpeg"))
}

/// A gradient image file: its first row stretched over the gradient (`ImgPaletteReader`).
fn image_gradient(p: &std::path::Path, name: &str) -> Option<NamedGradient> {
    let img = rwf_core::resources::image(&p.to_string_lossy())?;
    Some(NamedGradient { name: name.to_string(), gradient: rwf_core::gradient_ops::gradient_from_image_row(&img) })
}

fn load_all() -> Vec<Collection> {
    let mut out = Vec::new();
    // User gradients first.
    let mut mine = Vec::new();
    if let Some(dir) = user_dir()
        && let Ok(entries) = std::fs::read_dir(&dir)
    {
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            let name = p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            if is_image(&p) {
                mine.extend(image_gradient(&p, &name));
            } else if let Ok(src) = std::fs::read_to_string(&p) {
                mine.extend(parse_any(&src, &name));
            }
        }
    }
    out.push(Collection { name: "My gradients".into(), gradients: mine });
    for (name, file, src) in BUNDLED {
        out.push(Collection { name: name.to_string(), gradients: parse_any(src, file) });
    }
    out
}

impl GradientLibrary {
    pub fn show_window(&mut self, ctx: &egui::Context, current: &Gradient) -> Option<Gradient> {
        if !self.open {
            return None;
        }
        if self.collections.is_empty() && self.loading.is_none() {
            let (tx, rx) = channel();
            std::thread::spawn(move || {
                let _ = tx.send(load_all());
            });
            self.loading = Some(rx);
            self.selected = 1;
        }
        if let Some(rx) = &self.loading
            && let Ok(c) = rx.try_recv()
        {
            self.collections = c;
            self.loading = None;
        }
        let mut picked = None;
        let mut open = self.open;
        egui::Window::new("Gradient library").open(&mut open).default_size([460.0, 560.0]).show(ctx, |ui| {
            if self.loading.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Loading gradients…");
                });
                ctx.request_repaint();
                return;
            }
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("grad-coll")
                    .selected_text(self.collections.get(self.selected).map(|c| format!("{} ({})", c.name, c.gradients.len())).unwrap_or_default())
                    .show_ui(ui, |ui| {
                        for (i, c) in self.collections.iter().enumerate() {
                            ui.selectable_value(&mut self.selected, i, format!("{} ({})", c.name, c.gradients.len()));
                        }
                    });
                ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search…").desired_width(140.0));
            });
            ui.horizontal(|ui| {
                if ui.button(format!("{} Save current to My gradients", icons::PLUS)).clicked() {
                    self.status = Some(self.save_user(current));
                }
                if ui.button("Import file…").on_hover_text("Load .ugr / .map / flam3 palette files").clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter("Gradients", &["ugr", "map", "xml", "gradient"]).pick_file()
                {
                    self.status = Some(self.import(&p));
                }
            });
            if let Some(s) = &self.status {
                ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
            }
            ui.separator();
            let Some(coll) = self.collections.get(self.selected) else { return };
            let q = self.search.to_lowercase();
            let items: Vec<&NamedGradient> = coll.gradients.iter().filter(|g| q.is_empty() || g.name.to_lowercase().contains(&q)).collect();
            if items.is_empty() {
                ui.label(RichText::new("Nothing here yet.").color(theme::TEXT_MUTED));
            }
            let row_h = 26.0;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, items.len(), |ui, range| {
                for g in &items[range] {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), row_h - 4.0), Sense::click());
                    let bar = Rect::from_min_max(rect.min, pos2(rect.right() - 150.0, rect.bottom()));
                    paint_coarse(ui, bar, &g.gradient);
                    ui.painter().text(
                        pos2(bar.right() + 10.0, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        if g.name.is_empty() { "(unnamed)" } else { &g.name },
                        egui::FontId::proportional(12.0),
                        if resp.hovered() { theme::TEXT } else { theme::TEXT_MUTED },
                    );
                    if resp.hovered() {
                        ui.painter().rect_stroke(bar, CornerRadius::same(3), Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Outside);
                    }
                    if resp.on_hover_text("Click to apply").clicked() {
                        picked = Some(g.gradient.clone());
                    }
                    ui.add_space(4.0);
                }
            });
        });
        self.open = open;
        picked
    }

    fn save_user(&mut self, g: &Gradient) -> String {
        let Some(dir) = user_dir() else { return "No home directory found".into() };
        if let Err(e) = std::fs::create_dir_all(&dir) {
            return format!("Could not create {}: {e}", dir.display());
        }
        let path = dir.join(USER_FILE);
        let mut list = std::fs::read_to_string(&path).map(|s| parse_any(&s, USER_FILE)).unwrap_or_default();
        let name = format!("gradient {}", list.len() + 1);
        list.push(NamedGradient { name: name.clone(), gradient: g.clone() });
        match std::fs::write(&path, write_ugr(&list)) {
            Ok(()) => {
                if let Some(c) = self.collections.get_mut(0) {
                    c.gradients.push(NamedGradient { name: name.clone(), gradient: g.clone() });
                }
                format!("Saved as \"{name}\" in {}", path.display())
            }
            Err(e) => format!("Save failed: {e}"),
        }
    }

    fn import(&mut self, path: &std::path::Path) -> String {
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let found = if is_image(path) {
            image_gradient(path, &name).into_iter().collect()
        } else {
            let Ok(src) = std::fs::read_to_string(path) else { return "Could not read file".into() };
            parse_any(&src, &name)
        };
        if found.is_empty() {
            return "No gradients found in file".into();
        }
        let n = found.len();
        // Copy into the user folder so it shows up next time.
        if let Some(dir) = user_dir() {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::copy(path, dir.join(&name));
        }
        self.collections.push(Collection { name: name.clone(), gradients: found });
        self.selected = self.collections.len() - 1;
        format!("Imported {n} gradients from {name}")
    }
}

/// Cheap gradient preview (64 segments) for long lists.
fn paint_coarse(ui: &Ui, rect: Rect, g: &Gradient) {
    let mut mesh = egui::Mesh::default();
    let n = 64;
    let w = rect.width() / n as f32;
    for i in 0..n {
        let c = g.colors[i * (g.colors.len() - 1) / (n - 1)];
        let r = Rect::from_min_max(pos2(rect.left() + i as f32 * w, rect.top()), pos2(rect.left() + (i + 1) as f32 * w + 0.5, rect.bottom()));
        mesh.add_colored_rect(r, Color32::from_rgb(c[0], c[1], c[2]));
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// The point being dragged in the gradient curve editor: (curve, point).
#[derive(Default)]
pub struct CurvesEdit {
    drag: Option<(usize, usize)>,
}

impl CurvesEdit {
    /// JWildfire's gradient curve editor: the hue, saturation and luminosity curves of the
    /// layer's gradient; changing them rebuilds the gradient. Returns true when it changed.
    pub fn show(&mut self, ui: &mut Ui, layer: &mut rwf_core::Layer) -> bool {
        use rwf_core::gradient_ops::GradientCurves;
        let mut changed = false;
        let Some(curves) = layer.gradient_curves.as_mut() else {
            ui.label(RichText::new("Shape the gradient with hue, saturation and luminosity curves.").small().color(theme::TEXT_MUTED));
            if ui.button("Start from the gradient").on_hover_text("Curves through the current gradient's colors").clicked() {
                layer.gradient_curves = Some(GradientCurves::from_gradient(&layer.gradient));
            }
            return false;
        };
        for (k, (name, color)) in [("Hue", Color32::from_rgb(230, 160, 60)), ("Saturation", Color32::from_rgb(90, 200, 120)), ("Luminosity", Color32::from_rgb(200, 200, 200))].into_iter().enumerate() {
            ui.label(RichText::new(name).small().color(theme::TEXT_MUTED));
            changed |= self.plot(ui, k, &mut curves.curves[k], color);
        }
        ui.label(RichText::new("Drag points; double-click adds one, right-click removes one.").small().color(theme::TEXT_MUTED));
        let mut remove = false;
        ui.horizontal(|ui| {
            if ui.small_button("From gradient").on_hover_text("Curves through the current gradient's colors").clicked() {
                *curves = GradientCurves::from_gradient(&layer.gradient);
                changed = true;
            }
            if ui.small_button("Remove curves").on_hover_text("Keeps the gradient, forgets the curves").clicked() {
                remove = true;
            }
        });
        if changed && let Some(c) = &layer.gradient_curves {
            layer.gradient = c.to_gradient();
        }
        if remove {
            layer.gradient_curves = None;
        }
        changed
    }

    fn plot(&mut self, ui: &mut Ui, k: usize, pts: &mut Vec<(f64, f64)>, color: Color32) -> bool {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 56.0), egui::Sense::click_and_drag());
        let p = ui.painter_at(rect.expand(4.0));
        p.rect_filled(rect, 4, theme::CANVAS);
        let to_screen = |x: f64, y: f64| egui::pos2(rect.left() + rect.width() * (x / 255.0) as f32, rect.bottom() - rect.height() * (y / 255.0) as f32);
        let to_value = |pos: egui::Pos2| (((pos.x - rect.left()) / rect.width()) as f64 * 255.0, ((rect.bottom() - pos.y) / rect.height()) as f64 * 255.0);
        // the curve as JWildfire evaluates it
        let track = rwf_core::animation::Track {
            path: String::new(),
            interp: rwf_core::animation::Interp::Spline,
            keys: pts.iter().map(|&(frame, value)| rwf_core::animation::Key { frame, value }).collect(),
        };
        let line: Vec<egui::Pos2> = (0..=255).step_by(3).map(|x| to_screen(x as f64, track.eval(x as f64).unwrap_or(0.0).clamp(0.0, 255.0))).collect();
        p.add(egui::Shape::line(line, egui::Stroke::new(1.5, color)));
        for (x, y) in pts.iter() {
            p.circle_filled(to_screen(*x, *y), 3.5, color);
        }
        let mut changed = false;
        let nearest = |pos: egui::Pos2, pts: &[(f64, f64)]| pts.iter().position(|(x, y)| to_screen(*x, *y).distance(pos) < 9.0);
        if resp.drag_started()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            self.drag = nearest(pos, pts).map(|i| (k, i));
        }
        if resp.dragged()
            && let (Some((ck, i)), Some(pos)) = (self.drag, resp.interact_pointer_pos())
            && ck == k
            && i < pts.len()
        {
            let (mut x, y) = to_value(pos);
            // ends stay at the gradient's ends, inner points between their neighbours
            x = if i == 0 {
                0.0
            } else if i == pts.len() - 1 {
                255.0
            } else {
                x.clamp(pts[i - 1].0 + 1.0, pts[i + 1].0 - 1.0)
            };
            pts[i] = (x.round(), y.clamp(0.0, 255.0));
            changed = true;
        }
        if resp.drag_stopped() {
            self.drag = None;
        }
        if resp.double_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let (x, y) = to_value(pos);
            let x = x.round().clamp(1.0, 254.0);
            if !pts.iter().any(|(px, _)| (*px - x).abs() < 1.0) {
                let at = pts.iter().position(|(px, _)| *px > x).unwrap_or(pts.len());
                pts.insert(at, (x, y.clamp(0.0, 255.0)));
                changed = true;
            }
        }
        if resp.secondary_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
            && let Some(i) = nearest(pos, pts)
            && i > 0
            && i < pts.len() - 1
        {
            pts.remove(i);
            changed = true;
        }
        changed
    }
}
