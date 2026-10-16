// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
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

//! Inspector tab with the image effects of the flame.

use egui::{Color32, RichText, Ui};
use rwf_core::mixer::{Curve, CurveInterpolation};
use rwf_core::render::effects::ImageEffects;

use super::mixer::{curve_editor, interp_label};
use super::widgets::{self, grid, slider};
use crate::theme;

#[derive(Default)]
pub struct EffectsState {
    /// The curve shown: all channels, red, green, blue.
    curve: usize,
    drag: Option<usize>,
}

const CURVES: [(&str, &str); 4] = [
    ("All", "Tone curve of all channels: x is the brightness before, y after"),
    ("Red", "Curve of the red channel (after the curve of all channels)"),
    ("Green", "Curve of the green channel (after the curve of all channels)"),
    ("Blue", "Curve of the blue channel (after the curve of all channels)"),
];

fn curve_color(i: usize) -> Color32 {
    match i {
        1 => Color32::from_rgb(0xe8, 0x60, 0x60),
        2 => Color32::from_rgb(0x60, 0xd0, 0x70),
        3 => Color32::from_rgb(0x60, 0x90, 0xf0),
        _ => theme::TEXT,
    }
}

pub fn tab(ui: &mut Ui, fx: &mut ImageEffects, st: &mut EffectsState) {
    ui.label(
        RichText::new(
            "Photo-style adjustments of the finished image, after the denoiser. They show in the canvas and in saved images, videos and batch renders (not in HDR files and quilts); transparency stays as it is.",
        )
        .small()
        .color(theme::TEXT_MUTED),
    );
    ui.horizontal(|ui| {
        let active = fx.is_active();
        let text = if active { "Effects are on" } else { "No effect set" };
        ui.label(RichText::new(text).small().color(if active { theme::ACCENT } else { theme::TEXT_MUTED }));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(*fx != ImageEffects::default(), egui::Button::new("Reset all").small())
                .on_hover_text("Sets every effect back to neutral")
                .clicked()
            {
                *fx = ImageEffects::default();
            }
        });
    });
    let d = ImageEffects::default();

    if widgets::section_reset(ui, "Light") {
        (fx.exposure, fx.contrast, fx.highlights, fx.shadows, fx.whites, fx.blacks) =
            (d.exposure, d.contrast, d.highlights, d.shadows, d.whites, d.blacks);
    }
    grid(ui, "fx-light", |ui| {
        slider(ui, "Exposure", &mut fx.exposure, -3.0..=3.0, d.exposure).on_hover_text(
            "Brightens or darkens the finished image in photographic stops (+1 doubles the light). Brightness on the Look tab changes the tone mapping of the render instead.",
        );
        slider(ui, "Contrast", &mut fx.contrast, -1.0..=1.0, d.contrast).on_hover_text(
            "S-curve around the middle gray: > 0 darker shadows and brighter highlights, < 0 flatter. Black and white stay. Contrast on the Look tab changes the tone mapping instead.",
        );
        slider(ui, "Highlights", &mut fx.highlights, -1.0..=1.0, d.highlights).on_hover_text(
            "Brightens (> 0) or darkens (< 0) only the bright areas, found on a blurred copy so fine detail is not cut apart. Negative values recover detail in bright glows.",
        );
        slider(ui, "Shadows", &mut fx.shadows, -1.0..=1.0, d.shadows)
            .on_hover_text("Brightens (> 0) or darkens (< 0) only the dark areas; positive values bring out faint structures.");
        slider(ui, "Whites", &mut fx.whites, -1.0..=1.0, d.whites).on_hover_text(
            "Moves the white point: > 0 brighter highlights that clip sooner, < 0 compressed highlights without pure white. White level on the Look tab works before the gamma curve instead.",
        );
        slider(ui, "Blacks", &mut fx.blacks, -1.0..=1.0, d.blacks)
            .on_hover_text("Moves the black point: > 0 lifts the blacks to a dark gray, < 0 deepens them (dark detail turns black).");
    });

    if widgets::section_reset(ui, "Detail") {
        (fx.texture, fx.clarity, fx.dehaze) = (d.texture, d.clarity, d.dehaze);
    }
    grid(ui, "fx-detail", |ui| {
        slider(ui, "Texture", &mut fx.texture, -1.0..=1.0, d.texture).on_hover_text(
            "Local contrast of fine detail (a few pixels): > 0 crisper lines and structures, < 0 softer. Scales with the image size, so the preview matches the render.",
        );
        slider(ui, "Clarity", &mut fx.clarity, -1.0..=1.0, d.clarity)
            .on_hover_text("Local contrast of larger structures, mostly in the midtones: > 0 more depth and punch, < 0 a soft, dreamy look.");
        slider(ui, "Dehaze", &mut fx.dehaze, 0.0..=1.0, d.dehaze).on_hover_text(
            "Removes haze and milky glow: subtracts the local minimum of the color channels and stretches the rest. Strongest in bright, washed-out areas; on a black background it mostly deepens the contrast.",
        );
    });

    if widgets::section_reset(ui, "Curves") {
        fx.curves = d.curves.clone();
        st.drag = None;
    }
    ui.horizontal(|ui| {
        for (i, (label, help)) in CURVES.iter().enumerate() {
            let changed = !rwf_core::render::effects::is_identity(&fx.curves[i]);
            let text = if changed { format!("{label} •") } else { label.to_string() };
            if ui.selectable_label(st.curve == i, RichText::new(text).color(curve_color(i))).on_hover_text(*help).clicked() {
                st.curve = i;
                st.drag = None;
            }
        }
    });
    let curve = &mut fx.curves[st.curve];
    curve_editor(ui, curve, &mut st.drag, curve_color(st.curve));
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("fx-curve-interp")
            .selected_text(interp_label(curve.interpolation))
            .width(80.0)
            .show_ui(ui, |ui| {
                for m in [CurveInterpolation::Spline, CurveInterpolation::Bezier, CurveInterpolation::Linear] {
                    ui.selectable_value(&mut curve.interpolation, m, interp_label(m));
                }
            })
            .response
            .on_hover_text("How the curve runs between its points");
        if ui.small_button("Reset").on_hover_text("Straight curve: no change").clicked() {
            *curve = Curve::standard(true);
            st.drag = None;
        }
    });
    ui.label(RichText::new("Click to add a point, drag to move it, right-click a point to remove it.").small().color(theme::TEXT_MUTED));

    if widgets::section_reset(ui, "Vignette") {
        (fx.vignette, fx.vignette_size, fx.vignette_feather) = (d.vignette, d.vignette_size, d.vignette_feather);
    }
    grid(ui, "fx-vignette", |ui| {
        slider(ui, "Amount", &mut fx.vignette, -1.0..=1.0, d.vignette).on_hover_text("Darkens (< 0) or lightens (> 0) the edges and corners of the image");
        slider(ui, "Size", &mut fx.vignette_size, 0.0..=1.0, d.vignette_size).on_hover_text("Where the vignette starts: 0 at the center, 1 only in the corners");
        slider(ui, "Feather", &mut fx.vignette_feather, 0.0..=1.0, d.vignette_feather).on_hover_text("How soft the transition is: 0 a hard edge, 1 a gradual fade");
    });

    if widgets::section_reset(ui, "Grain") {
        (fx.grain, fx.grain_size) = (d.grain, d.grain_size);
    }
    grid(ui, "fx-grain", |ui| {
        slider(ui, "Amount", &mut fx.grain, 0.0..=1.0, d.grain)
            .on_hover_text("Adds film grain, strongest in the midtones. It is reproducible: the same image always gets the same grain.");
        slider(ui, "Size", &mut fx.grain_size, 0.5..=4.0, d.grain_size)
            .on_hover_text("Grain size: 1 = one pixel at an image height of 1000 pixels, so the preview shows it like the final render");
    });
}
