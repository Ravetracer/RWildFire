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

//! Small reusable widgets.

use egui::{Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use rwf_core::Gradient;

use crate::theme;

pub fn gradient_color(g: &Gradient, t: f64) -> Color32 {
    let i = ((t.clamp(0.0, 1.0)) * (g.colors.len() - 1) as f64).round() as usize;
    let c = g.colors[i.min(g.colors.len() - 1)];
    Color32::from_rgb(c[0], c[1], c[2])
}

/// Paints a horizontal gradient strip; returns the response (clickable).
pub fn gradient_strip(ui: &mut Ui, g: &Gradient, height: f32, marker: Option<f64>) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click_and_drag());
    paint_gradient(ui, rect, g);
    if let Some(m) = marker {
        let x = rect.left() + rect.width() * m.clamp(0.0, 1.0) as f32;
        let p = ui.painter();
        p.line_segment([pos2(x, rect.top() - 2.0), pos2(x, rect.bottom() + 2.0)], Stroke::new(3.0, Color32::BLACK));
        p.line_segment([pos2(x, rect.top() - 2.0), pos2(x, rect.bottom() + 2.0)], Stroke::new(1.5, Color32::WHITE));
    }
    resp
}

pub fn paint_gradient(ui: &Ui, rect: Rect, g: &Gradient) {
    let p = ui.painter();
    let n = g.colors.len();
    let w = rect.width() / n as f32;
    let mut mesh = egui::Mesh::default();
    for (i, c) in g.colors.iter().enumerate() {
        let r = Rect::from_min_max(pos2(rect.left() + i as f32 * w, rect.top()), pos2(rect.left() + (i + 1) as f32 * w + 0.5, rect.bottom()));
        mesh.add_colored_rect(r, Color32::from_rgb(c[0], c[1], c[2]));
    }
    p.add(egui::Shape::mesh(mesh));
    p.rect_stroke(rect, CornerRadius::same(3), Stroke::new(1.0, theme::BORDER), StrokeKind::Outside);
}

/// Two-column property grid helper.
pub fn grid(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([10.0, 6.0]).min_col_width(92.0).show(ui, add);
}

pub fn label(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(theme::TEXT_MUTED));
}

/// Labeled slider row inside a [`grid`]. Values outside the range can still be typed in.
pub fn slider(ui: &mut Ui, text: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>) -> Response {
    label(ui, text);
    let r = ui.add(egui::Slider::new(v, range).clamping(egui::SliderClamping::Never).max_decimals(4));
    ui.end_row();
    r
}

pub fn slider_log(ui: &mut Ui, text: &str, v: &mut f64, range: std::ops::RangeInclusive<f64>) -> Response {
    label(ui, text);
    let r = ui.add(egui::Slider::new(v, range).logarithmic(true).clamping(egui::SliderClamping::Never).max_decimals(3));
    ui.end_row();
    r
}

pub fn drag(ui: &mut Ui, text: &str, v: &mut f64, speed: f64) -> Response {
    label(ui, text);
    let r = ui.add(egui::DragValue::new(v).speed(speed).max_decimals(5));
    ui.end_row();
    r
}

/// Color picker for a `[u8; 3]`.
pub fn color_rgb(ui: &mut Ui, c: &mut [u8; 3]) -> Response {
    let mut col = Color32::from_rgb(c[0], c[1], c[2]);
    let r = egui::color_picker::color_edit_button_srgba(ui, &mut col, egui::color_picker::Alpha::Opaque);
    *c = [col.r(), col.g(), col.b()];
    r
}

/// Segmented control (tab bar).
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) {
    egui::Frame::new().fill(theme::BG).corner_radius(8).inner_margin(3).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let w = (ui.available_width() - 2.0 * (options.len() as f32 - 1.0)) / options.len() as f32;
            for (opt, text) in options {
                let selected = *value == *opt;
                let fill = if selected { theme::SURFACE_HOVER } else { Color32::TRANSPARENT };
                let txt = egui::RichText::new(*text).color(if selected { theme::TEXT } else { theme::TEXT_MUTED });
                if ui.add_sized([w, 26.0], egui::Button::new(txt).fill(fill).corner_radius(6)).clicked() {
                    *value = *opt;
                }
            }
        });
    });
}

/// Flat icon-ish button with tooltip.
pub fn icon_button(ui: &mut Ui, icon: &str, tip: &str) -> Response {
    ui.add(egui::Button::new(egui::RichText::new(icon).size(14.0)).frame(false).min_size(vec2(24.0, 24.0))).on_hover_text(tip)
}

/// Combo box of the random symmetry generators.
pub fn symmetry_combo(ui: &mut Ui, id: &str, value: &mut rwf_core::random::SymmetryGen) {
    egui::ComboBox::from_id_salt(id).width(120.0).selected_text(value.label()).show_ui(ui, |ui| {
        for g in rwf_core::random::SymmetryGen::ALL {
            ui.selectable_value(value, *g, g.label());
        }
    });
}

/// Combo box of the random weighting field generators.
pub fn weighting_combo(ui: &mut Ui, id: &str, value: &mut rwf_core::random::WeightingGen) {
    egui::ComboBox::from_id_salt(id).width(120.0).selected_text(value.label()).show_ui(ui, |ui| {
        for g in rwf_core::random::WeightingGen::ALL {
            ui.selectable_value(value, *g, g.label());
        }
    });
}
