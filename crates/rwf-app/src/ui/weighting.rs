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

//! Weighting field editor of the selected transform.

use egui::{RichText, Ui};
use rwf_core::XForm;
use rwf_core::weighting::{self as wf, FieldInput, FieldType, WeightingField};

use super::widgets::{self, grid, slider};
use crate::theme;

/// Cached preview image of the edited field.
#[derive(Default)]
pub struct FieldPreview {
    key: Option<WeightingField>,
    tex: Option<egui::TextureHandle>,
}

const PREVIEW_W: usize = 160;
const PREVIEW_H: usize = 120;

pub fn section(ui: &mut Ui, xf: &mut XForm, preview: &mut FieldPreview) {
    let active = xf.weighting.is_active();
    let title = if active { "Weighting field (on)" } else { "Weighting field" };
    egui::CollapsingHeader::new(RichText::new(title).color(theme::TEXT_MUTED)).id_salt("wfield").show(ui, |ui| {
        let var_names: Vec<String> = xf.variations.iter().map(|v| v.name.clone()).filter(|n| wf::is_valid_variation(n)).collect();
        let w = &mut xf.weighting;
        grid(ui, "wfield", |ui| {
            widgets::label(ui, "Type");
            egui::ComboBox::from_id_salt("wf-type").selected_text(w.field_type.label()).show_ui(ui, |ui| {
                for t in FieldType::ALL {
                    ui.selectable_value(&mut w.field_type, t, t.label());
                }
            });
            ui.end_row();
            if w.field_type == FieldType::None {
                return;
            }
            widgets::label(ui, "Input");
            egui::ComboBox::from_id_salt("wf-input")
                .selected_text(if w.input == FieldInput::Affine { "Affine point" } else { "Source point" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut w.input, FieldInput::Affine, "Affine point");
                    ui.selectable_value(&mut w.input, FieldInput::Position, "Source point");
                });
            ui.end_row();
            slider(ui, "Color", &mut w.color_intensity, -2.0..=2.0, rwf_core::weighting::WeightingField::default().color_intensity);
            slider(ui, "Variation amount", &mut w.var_amount_intensity, -2.0..=2.0, rwf_core::weighting::WeightingField::default().var_amount_intensity);
            slider(ui, "Jitter", &mut w.jitter_intensity, -2.0..=2.0, rwf_core::weighting::WeightingField::default().jitter_intensity);
            for (k, p) in w.var_params.iter_mut().enumerate() {
                widgets::label(ui, &format!("Parameter {}", k + 1));
                var_param_row(ui, k, p, &var_names, &xf_params(&p.var_name));
                ui.end_row();
            }
            if w.field_type == FieldType::ImageMap {
                widgets::label(ui, "Image");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut w.cmap_filename).desired_width(140.0).hint_text("default image"));
                    if ui.small_button("…").clicked()
                        && let Some(p) = crate::settings::dialog(crate::settings::Folder::Images).add_filter("Image", &["png", "jpg", "jpeg"]).pick_file()
                    {
                        w.cmap_filename = p.display().to_string();
                    }
                });
                ui.end_row();
                slider(ui, "Width", &mut w.cmap_x_size, 0.1..=20.0, rwf_core::weighting::WeightingField::default().cmap_x_size);
                slider(ui, "Height", &mut w.cmap_y_size, 0.1..=20.0, rwf_core::weighting::WeightingField::default().cmap_y_size);
                slider(ui, "Centre X", &mut w.cmap_x_centre, -5.0..=5.0, rwf_core::weighting::WeightingField::default().cmap_x_centre);
                slider(ui, "Centre Y", &mut w.cmap_y_centre, -5.0..=5.0, rwf_core::weighting::WeightingField::default().cmap_y_centre);
            } else {
                widgets::label(ui, "Seed");
                ui.add(egui::DragValue::new(&mut w.noise_seed).speed(1.0));
                ui.end_row();
                widgets::label(ui, "Frequency");
                widgets::reset_on_double_click(ui.add(egui::Slider::new(&mut w.noise_frequency, 0.01..=20.0).logarithmic(true).clamping(egui::SliderClamping::Never)), &mut w.noise_frequency, rwf_core::weighting::WeightingField::default().noise_frequency);
                ui.end_row();
            }
            if w.field_type.is_fractal() {
                widgets::label(ui, "Fractal type");
                egui::ComboBox::from_id_salt("wf-fract").selected_text(wf::fractal_type_str(w.fractal_type)).show_ui(ui, |ui| {
                    for t in wf::FRACTAL_TYPES {
                        ui.selectable_value(&mut w.fractal_type, t, wf::fractal_type_str(t));
                    }
                });
                ui.end_row();
                widgets::label(ui, "Octaves");
                ui.add(egui::DragValue::new(&mut w.fractal_octaves).speed(0.05).range(1..=10));
                ui.end_row();
                slider(ui, "Gain", &mut w.fractal_gain, 0.0..=1.0, rwf_core::weighting::WeightingField::default().fractal_gain);
                slider(ui, "Lacunarity", &mut w.fractal_lacunarity, 1.0..=4.0, rwf_core::weighting::WeightingField::default().fractal_lacunarity);
            }
            if w.field_type == FieldType::CellularNoise {
                widgets::label(ui, "Return");
                egui::ComboBox::from_id_salt("wf-cret").selected_text(wf::cellular_return_str(w.cellular_return)).show_ui(ui, |ui| {
                    for t in wf::CELLULAR_RETURNS {
                        ui.selectable_value(&mut w.cellular_return, t, wf::cellular_return_str(t));
                    }
                });
                ui.end_row();
                widgets::label(ui, "Distance");
                egui::ComboBox::from_id_salt("wf-cdist").selected_text(wf::cellular_distance_str(w.cellular_distance)).show_ui(ui, |ui| {
                    for t in wf::CELLULAR_DISTANCES {
                        ui.selectable_value(&mut w.cellular_distance, t, wf::cellular_distance_str(t));
                    }
                });
                ui.end_row();
            }
        });
        if w.field_type != FieldType::None {
            show_preview(ui, w, preview);
            if !w.is_active() {
                ui.label(RichText::new("Set an intensity to apply the field.").small().color(theme::TEXT_MUTED));
            }
        }
    });
}

/// Parameter names a weighting field can modulate: the amount and the numeric parameters.
fn xf_params(var_name: &str) -> Vec<&'static str> {
    let mut out = vec!["amount"];
    if let Some(def) = rwf_core::variations::find(var_name) {
        out.extend(def.params.iter().map(|p| p.name));
    }
    out
}

fn var_param_row(ui: &mut Ui, k: usize, p: &mut wf::VarParamWeight, var_names: &[String], params: &[&'static str]) {
    ui.horizontal(|ui| {
        let var_text = if p.var_name.is_empty() { "—".to_string() } else { p.var_name.clone() };
        egui::ComboBox::from_id_salt(("wf-var", k)).selected_text(var_text).width(90.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut p.var_name, String::new(), "—");
            for n in var_names {
                ui.selectable_value(&mut p.var_name, n.clone(), n);
            }
        });
        if !p.var_name.is_empty() {
            if !params.contains(&p.param_name.as_str()) {
                p.param_name = "amount".into();
            }
            egui::ComboBox::from_id_salt(("wf-param", k)).selected_text(p.param_name.clone()).width(70.0).show_ui(ui, |ui| {
                for n in params {
                    ui.selectable_value(&mut p.param_name, n.to_string(), *n);
                }
            });
            ui.add(egui::DragValue::new(&mut p.intensity).speed(0.01).max_decimals(4));
        }
    });
}

/// Grayscale-ish preview of the field over [-2, 2] (`WeightingFieldControlsUpdater`).
fn show_preview(ui: &mut Ui, w: &WeightingField, preview: &mut FieldPreview) {
    if preview.key.as_ref() != Some(w) || preview.tex.is_none() {
        let field = w.field();
        let mut px = Vec::with_capacity(PREVIEW_W * PREVIEW_H);
        for i in 0..PREVIEW_H {
            let y = i as f64 / PREVIEW_H as f64 * 4.0 - 2.0;
            for j in 0..PREVIEW_W {
                let x = j as f64 / PREVIEW_W as f64 * 4.0 - 2.0;
                let v = ((field.value(x, y, 0.0) * 128.0 + 128.0) as i32).clamp(0, 255);
                px.push(egui::Color32::from_rgb((2 * v / 3) as u8, v as u8, (v / 2) as u8));
            }
        }
        let img = egui::ColorImage::new([PREVIEW_W, PREVIEW_H], px);
        match &mut preview.tex {
            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
            None => preview.tex = Some(ui.ctx().load_texture("wfield-preview", img, egui::TextureOptions::LINEAR)),
        }
        preview.key = Some(w.clone());
    }
    if let Some(t) = &preview.tex {
        ui.add_space(4.0);
        ui.image((t.id(), egui::vec2(PREVIEW_W as f32, PREVIEW_H as f32)));
    }
}
