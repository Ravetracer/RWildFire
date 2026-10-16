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

//! Channel mixer editor: mode and curve editing.

use egui::{Color32, Pos2, Sense, Shape, Stroke, Ui, pos2, vec2};
use rwf_core::mixer::{ChannelMixer, Curve, CurveInterpolation, Envelope, FILTER_SCALE, MixerMode};

use super::widgets::{self, grid};
use crate::theme;

/// Curves work on colors in 0..25600.
const SCALE: f64 = 256.0 * FILTER_SCALE;
/// Input and output channel of each curve.
const CURVE_CHANNELS: [(&str, &str); 9] =
    [("R", "R"), ("G", "R"), ("B", "R"), ("R", "G"), ("G", "G"), ("B", "G"), ("R", "B"), ("G", "B"), ("B", "B")];
const POINT_RADIUS: f32 = 5.0;

#[derive(Default)]
pub struct MixerState {
    curve: usize,
    drag: Option<usize>,
}

pub fn section(ui: &mut Ui, mixer: &mut ChannelMixer, st: &mut MixerState) {
    theme::section_label(ui, "Channel mixer");
    grid(ui, "mixer", |ui| {
        widgets::label(ui, "Mode");
        egui::ComboBox::from_id_salt("mixer-mode").selected_text(mixer.mode.label()).show_ui(ui, |ui| {
            for m in MixerMode::ALL {
                ui.selectable_value(&mut mixer.mode, m, m.label());
            }
        });
        ui.end_row();
    });
    let curves = mixer.mode.curves();
    if curves.is_empty() {
        return;
    }
    if !curves.contains(&st.curve) {
        st.curve = curves[0];
        st.drag = None;
    }
    if curves.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            for &i in curves {
                let (from, to) = CURVE_CHANNELS[i];
                if ui.selectable_label(st.curve == i, format!("{from} {} {to}", crate::icons::ARROW_RIGHT)).clicked() {
                    st.curve = i;
                    st.drag = None;
                }
            }
        });
    }
    let curve = &mut mixer.curves[st.curve];
    curve_editor(ui, curve, &mut st.drag, curve_color(st.curve));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("mixer-interp").selected_text(interp_label(curve.interpolation)).width(80.0).show_ui(ui, |ui| {
            for m in [CurveInterpolation::Spline, CurveInterpolation::Bezier, CurveInterpolation::Linear] {
                ui.selectable_value(&mut curve.interpolation, m, interp_label(m));
            }
        });
        if ui.small_button("Reset").clicked() {
            *curve = Curve::standard(st.curve % 4 == 0);
        }
    });
    ui.label(egui::RichText::new("Click to add a point, right-click a point to remove it.").small().color(theme::TEXT_MUTED));
}

pub(super) fn interp_label(m: CurveInterpolation) -> &'static str {
    match m {
        CurveInterpolation::Spline => "Spline",
        CurveInterpolation::Bezier => "Bezier",
        CurveInterpolation::Linear => "Linear",
    }
}

/// Color of the output channel of a curve.
fn curve_color(i: usize) -> Color32 {
    match i / 3 {
        0 => Color32::from_rgb(0xe8, 0x60, 0x60),
        1 => Color32::from_rgb(0x60, 0xd0, 0x70),
        _ => Color32::from_rgb(0x60, 0x90, 0xf0),
    }
}

/// Editor for one curve over 0..25600 (click adds a point, drag moves it, right-click removes
/// it); `drag` keeps the dragged point between frames.
pub(super) fn curve_editor(ui: &mut Ui, curve: &mut Curve, drag: &mut Option<usize>, color: Color32) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, (w * 0.75).min(240.0)), Sense::click_and_drag());
    let plot = rect.shrink(POINT_RADIUS + 2.0);
    let to_screen = |x: f64, y: f64| pos2(plot.left() + (x / SCALE) as f32 * plot.width(), plot.bottom() - (y / SCALE) as f32 * plot.height());
    let from_screen = |p: Pos2| (((p.x - plot.left()) / plot.width()) as f64 * SCALE, ((plot.bottom() - p.y) / plot.height()) as f64 * SCALE);

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4, theme::BG);
    for k in 0..=4 {
        let t = k as f32 / 4.0;
        let s = Stroke::new(1.0, theme::BORDER);
        painter.line_segment([pos2(plot.left() + t * plot.width(), plot.top()), pos2(plot.left() + t * plot.width(), plot.bottom())], s);
        painter.line_segment([pos2(plot.left(), plot.top() + t * plot.height()), pos2(plot.right(), plot.top() + t * plot.height())], s);
    }
    // identity reference
    painter.line_segment([to_screen(0.0, 0.0), to_screen(SCALE, SCALE)], Stroke::new(1.0, theme::TEXT_MUTED.gamma_multiply(0.4)));

    // interaction
    let hit = |p: Pos2, c: &Curve| (0..c.x.len()).find(|&i| to_screen(c.x[i] as f64, c.y[i]).distance(p) <= POINT_RADIUS + 3.0);
    if let Some(p) = resp.interact_pointer_pos() {
        if resp.drag_started() {
            let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
            *drag = hit(origin, curve);
        }
        if resp.dragged()
            && let Some(i) = *drag
            && i < curve.x.len()
        {
            let (x, y) = from_screen(p);
            let lo = if i > 0 { curve.x[i - 1] + 1 } else { 0 };
            let hi = if i + 1 < curve.x.len() { curve.x[i + 1] - 1 } else { SCALE as i32 };
            curve.x[i] = (x.round() as i32).clamp(lo, hi.max(lo));
            curve.y[i] = y.clamp(0.0, SCALE).round();
            curve.selected_idx = i as i32;
        }
    }
    if resp.drag_stopped() {
        *drag = None;
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && hit(p, curve).is_none()
    {
        let (x, y) = from_screen(p);
        let x = (x.round() as i32).clamp(0, SCALE as i32);
        if !curve.x.contains(&x) {
            let i = curve.x.partition_point(|&v| v < x);
            curve.x.insert(i, x);
            curve.y.insert(i, y.clamp(0.0, SCALE).round());
            curve.selected_idx = i as i32;
        }
    }
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && let Some(i) = hit(p, curve)
        && curve.x.len() > 2
    {
        curve.x.remove(i);
        curve.y.remove(i);
        curve.selected_idx = curve.selected_idx.min(curve.x.len() as i32 - 1);
    }

    // curve
    let env = Envelope::new(curve);
    let mut pts: Vec<Pos2> = Vec::new();
    if let (Some(&first), Some(&last)) = (curve.x.first(), curve.x.last()) {
        pts.push(to_screen(0.0, env.evaluate(0.0)));
        pts.push(to_screen(first as f64, env.evaluate(first as f64)));
        pts.extend(env.points().into_iter().map(|(x, y)| to_screen(x, y)));
        pts.push(to_screen(last as f64, env.evaluate(last as f64)));
        pts.push(to_screen(SCALE, env.evaluate(SCALE)));
    }
    let pts: Vec<Pos2> = pts.into_iter().map(|p| pos2(p.x, p.y.clamp(rect.top(), rect.bottom()))).collect();
    painter.add(Shape::line(pts, Stroke::new(1.8, color)));
    for i in 0..curve.x.len() {
        let c = to_screen(curve.x[i] as f64, curve.y[i]);
        let active = *drag == Some(i) || curve.selected_idx == i as i32;
        painter.circle(c, POINT_RADIUS, if active { color } else { theme::SURFACE }, Stroke::new(1.5, color));
    }
}
