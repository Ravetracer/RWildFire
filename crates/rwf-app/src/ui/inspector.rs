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

//! Right panel: tabbed property inspector.

use egui::{RichText, Ui};
use rwf_core::Gradient;
use rwf_core::model::{BgColorType, ColorType, DrawMode, PostSymmetryType};
use rwf_core::render::denoise::{self, Denoiser};
use rwf_core::render::filter::FilterKernel;

use super::widgets::{self, drag, grid, slider, slider_log};
use crate::canvas::CanvasState;
use crate::doc::{Document, Selection};
use crate::preview::{Backend, Preview};
use crate::{icons, theme};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tab {
    #[default]
    Transform,
    Camera,
    Look,
    Render,
}

/// Relative gradient adjustments applied on top of a captured base gradient.
#[derive(Default)]
pub struct GradientEdit {
    base: Option<Gradient>,
    last: Option<Gradient>,
    shift: i32,
    hue: f64,
    sat: f64,
    lum: f64,
}

#[derive(Default)]
pub struct InspectorState {
    pub tab: Tab,
    pub gradient: GradientEdit,
    pub keys: crate::gradients::KeyEditor,
    pub library: crate::gradients::GradientLibrary,
    pub wfield_preview: super::weighting::FieldPreview,
    pub mixer: super::mixer::MixerState,
    rotate_step: f64,
    scale_step: f64,
    move_step: f64,
}

pub fn show(ui: &mut Ui, doc: &mut Document, st: &mut InspectorState, preview: &mut Preview, canvas: &mut CanvasState) {
    if st.rotate_step == 0.0 {
        st.rotate_step = 15.0;
        st.scale_step = 10.0;
        st.move_step = 0.1;
    }
    widgets::segmented(ui, &mut st.tab, &[(Tab::Transform, "Transform"), (Tab::Camera, "Camera"), (Tab::Look, "Look"), (Tab::Render, "Render")]);
    ui.add_space(6.0);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match st.tab {
        Tab::Transform => transform_tab(ui, doc, st),
        Tab::Camera => camera_tab(ui, doc),
        Tab::Look => look_tab(ui, doc, st),
        Tab::Render => render_tab(ui, doc, preview, canvas),
    });
}

fn transform_tab(ui: &mut Ui, doc: &mut Document, st: &mut InspectorState) {
    let solid = doc.flame.solid.enabled;
    let n_xforms = doc.layer().xforms.len();
    let gradient = doc.layer().gradient.clone();
    let sel = doc.selection;
    let edit_post = doc.edit_post;
    let Some(xf) = doc.selected_mut() else {
        ui.label(RichText::new("Select a transform on the left or on the canvas.").color(theme::TEXT_MUTED));
        return;
    };

    theme::section_label(ui, "General");
    grid(ui, "xf-general", |ui| {
        widgets::label(ui, "Name");
        ui.text_edit_singleline(&mut xf.name);
        ui.end_row();
        if matches!(sel, Selection::XForm(_) | Selection::Background(_)) {
            slider_log(ui, "Weight", &mut xf.weight, 0.001..=50.0);
        }
        widgets::label(ui, "Opacity");
        let mut op = if xf.draw_mode == DrawMode::Hidden { 0.0 } else { xf.opacity };
        if ui.add(egui::Slider::new(&mut op, 0.0..=1.0)).changed() {
            xf.opacity = op;
            xf.draw_mode = if op <= 1e-8 {
                DrawMode::Hidden
            } else if (op - 1.0).abs() > 1e-8 {
                DrawMode::Opaque
            } else {
                DrawMode::Normal
            };
        }
        ui.end_row();
    });

    theme::section_label(ui, "Color");
    widgets::gradient_strip(ui, &gradient, 12.0, Some(xf.color));
    grid(ui, "xf-color", |ui| {
        widgets::label(ui, "Mode");
        egui::ComboBox::from_id_salt("ctype")
            .selected_text(match xf.color_type {
                ColorType::Unset => "Default",
                ColorType::None => "None",
                ColorType::Diffusion => "Diffusion",
                ColorType::Target => "Target color",
                ColorType::TargetG => "Target (gradient)",
                ColorType::Distance => "Distance",
                ColorType::Cyclic => "Cyclic",
            })
            .show_ui(ui, |ui| {
                for (t, l) in [
                    (ColorType::Unset, "Default"),
                    (ColorType::None, "None"),
                    (ColorType::Diffusion, "Diffusion"),
                    (ColorType::Target, "Target color"),
                    (ColorType::TargetG, "Target (gradient)"),
                    (ColorType::Distance, "Distance"),
                    (ColorType::Cyclic, "Cyclic"),
                ] {
                    ui.selectable_value(&mut xf.color_type, t, l);
                }
            });
        ui.end_row();
        slider(ui, "Position", &mut xf.color, 0.0..=1.0);
        slider(ui, "Speed", &mut xf.color_symmetry, -1.0..=1.0);
        if xf.color_type == ColorType::Target {
            widgets::label(ui, "Target");
            widgets::color_rgb(ui, &mut xf.target_color);
            ui.end_row();
        }
    });

    theme::section_label(ui, if edit_post { "Post affine" } else { "Affine" });
    let a = if edit_post { &mut xf.post } else { &mut xf.coefs };
    egui::Grid::new("affine").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("");
        ui.label(RichText::new("x").color(theme::TEXT_MUTED));
        ui.label(RichText::new("y").color(theme::TEXT_MUTED));
        ui.end_row();
        for (label, p, q) in [("X axis", 0, 1), ("Y axis", 2, 3), ("Origin", 4, 5)] {
            widgets::label(ui, label);
            let mut arr = a.as_array();
            ui.add(egui::DragValue::new(&mut arr[p]).speed(0.005).max_decimals(5));
            ui.add(egui::DragValue::new(&mut arr[q]).speed(0.005).max_decimals(5));
            *a = rwf_core::Affine::from_array(arr);
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button(icons::ROTATE_CCW).on_hover_text("Rotate counter-clockwise").clicked() {
            a.rotate(-st.rotate_step);
        }
        if ui.button(icons::ROTATE_CW).on_hover_text("Rotate clockwise").clicked() {
            a.rotate(st.rotate_step);
        }
        ui.add(egui::DragValue::new(&mut st.rotate_step).suffix("°").speed(0.5).range(0.1..=180.0));
        ui.separator();
        if ui.button(icons::MINUS).on_hover_text("Shrink").clicked() {
            a.scale(1.0 / (1.0 + st.scale_step / 100.0));
        }
        if ui.button(icons::PLUS).on_hover_text("Grow").clicked() {
            a.scale(1.0 + st.scale_step / 100.0);
        }
        ui.add(egui::DragValue::new(&mut st.scale_step).suffix("%").speed(0.5).range(0.1..=100.0));
    });
    ui.horizontal(|ui| {
        let s = st.move_step;
        if ui.button(icons::ARROW_LEFT).clicked() {
            a.translate(-s, 0.0);
        }
        if ui.button(icons::ARROW_RIGHT).clicked() {
            a.translate(s, 0.0);
        }
        if ui.button(icons::ARROW_UP).clicked() {
            a.translate(0.0, -s);
        }
        if ui.button(icons::ARROW_DOWN).clicked() {
            a.translate(0.0, s);
        }
        ui.add(egui::DragValue::new(&mut st.move_step).speed(0.005).range(0.001..=10.0));
    });
    ui.horizontal(|ui| {
        if ui.button(format!("{} Flip H", icons::FLIP_HORIZONTAL)).clicked() {
            a.c00 = -a.c00;
            a.c01 = -a.c01;
        }
        if ui.button(format!("{} Flip V", icons::FLIP_VERTICAL)).clicked() {
            a.c10 = -a.c10;
            a.c11 = -a.c11;
        }
        if ui.button("Reset").clicked() {
            *a = rwf_core::Affine::IDENTITY;
        }
    });

    if let Selection::XForm(_) = sel
        && n_xforms > 1
    {
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new("Xaos (jump probabilities)").color(theme::TEXT_MUTED)).show(ui, |ui| {
            if xf.chaos.len() < n_xforms {
                xf.chaos.resize(n_xforms, 1.0);
            }
            grid(ui, "xaos", |ui| {
                for j in 0..n_xforms {
                    slider(ui, &format!("→ Transform {}", j + 1), &mut xf.chaos[j], 0.0..=2.0);
                }
            });
            if ui.button("Reset xaos").clicked() {
                xf.chaos.clear();
            }
        });
    }

    ui.add_space(4.0);
    egui::CollapsingHeader::new(RichText::new("Color modulation").color(theme::TEXT_MUTED)).show(ui, |ui| {
        grid(ui, "mods", |ui| {
            slider(ui, "Gamma", &mut xf.mod_gamma, -2.0..=2.0);
            slider(ui, "Gamma speed", &mut xf.mod_gamma_speed, -1.0..=1.0);
            slider(ui, "Contrast", &mut xf.mod_contrast, -2.0..=2.0);
            slider(ui, "Contrast speed", &mut xf.mod_contrast_speed, -1.0..=1.0);
            slider(ui, "Saturation", &mut xf.mod_saturation, -2.0..=2.0);
            slider(ui, "Saturation speed", &mut xf.mod_saturation_speed, -1.0..=1.0);
            slider(ui, "Hue", &mut xf.mod_hue, -1.0..=1.0);
            slider(ui, "Hue speed", &mut xf.mod_hue_speed, -1.0..=1.0);
        });
        ui.label(RichText::new("Color modulation is applied by the CPU renderer.").small().color(theme::TEXT_MUTED));
    });
    super::weighting::section(ui, xf, &mut st.wfield_preview);
    if solid {
        egui::CollapsingHeader::new(RichText::new("Solid material").color(theme::TEXT_MUTED)).default_open(true).show(ui, |ui| {
            grid(ui, "xf-material", |ui| {
                slider(ui, "Material", &mut xf.material, 0.0..=8.0).on_hover_text("Material index; fractional values blend two materials");
                slider(ui, "Material speed", &mut xf.material_speed, -1.0..=1.0);
            });
        });
    }
}

fn camera_tab(ui: &mut Ui, doc: &mut Document) {
    let f = &mut doc.flame;
    theme::section_label(ui, "View");
    grid(ui, "cam", |ui| {
        drag(ui, "Center X", &mut f.centre_x, 0.005);
        drag(ui, "Center Y", &mut f.centre_y, 0.005);
        slider_log(ui, "Scale", &mut f.pixels_per_unit, 1.0..=5000.0);
        slider_log(ui, "Zoom", &mut f.cam_zoom, 0.05..=20.0);
        slider(ui, "Rotation", &mut f.cam_roll, -180.0..=180.0);
    });
    if ui.button(format!("{}  Fit flame to view", icons::FIT)).clicked() {
        let mut c = f.clone();
        if rwf_core::render::analysis::auto_frame(&mut c) {
            *f = c;
        }
    }
    theme::section_label(ui, "3D camera");
    grid(ui, "cam3d", |ui| {
        slider(ui, "Pitch", &mut f.cam_pitch, -180.0..=180.0);
        slider(ui, "Yaw", &mut f.cam_yaw, -180.0..=180.0);
        slider(ui, "Bank", &mut f.cam_bank, -180.0..=180.0);
        slider(ui, "Perspective", &mut f.cam_perspective, 0.0..=1.0);
        drag(ui, "Position X", &mut f.cam_pos_x, 0.01);
        drag(ui, "Position Y", &mut f.cam_pos_y, 0.01);
        drag(ui, "Position Z", &mut f.cam_pos_z, 0.01);
        slider(ui, "Depth fade", &mut f.dimish_z, 0.0..=4.0);
        drag(ui, "Fade distance", &mut f.dim_z_distance, 0.01);
    });
    theme::section_label(ui, "Depth of field");
    grid(ui, "dof", |ui| {
        slider(ui, "Amount", &mut f.cam_dof, 0.0..=5.0).on_hover_text("0 = off. Needs a 3D camera (pitch/yaw/perspective) to be visible.");
        if f.cam_dof.abs() > 1e-8 {
            widgets::label(ui, "Mode");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut f.new_cam_dof, false, "Z plane").on_hover_text("Legacy: blur grows with distance behind the camera plane");
                ui.selectable_value(&mut f.new_cam_dof, true, "Focus point").on_hover_text("Sharp area around a focus point");
            });
            ui.end_row();
            if f.new_cam_dof {
                drag(ui, "Focus X", &mut f.focus_x, 0.01);
                drag(ui, "Focus Y", &mut f.focus_y, 0.01);
                drag(ui, "Focus Z", &mut f.focus_z, 0.01);
                slider(ui, "Sharp area", &mut f.cam_dof_area, 0.0..=4.0);
                slider(ui, "Exponent", &mut f.cam_dof_exponent, 0.1..=4.0);
            } else {
                drag(ui, "Focus plane Z", &mut f.cam_z, 0.01);
            }
            widgets::label(ui, "Bokeh shape");
            let mut shape = f.cam_dof_shape;
            egui::ComboBox::from_id_salt("dofshape").selected_text(shape.label()).show_ui(ui, |ui| {
                for s in rwf_core::render::dof::DofShape::ALL {
                    let avail = s.variation().is_none_or(|(n, _, _)| rwf_core::variations::find(n).is_some());
                    ui.add_enabled_ui(avail, |ui| {
                        ui.selectable_value(&mut shape, *s, s.label());
                    });
                }
            });
            if shape != f.cam_dof_shape {
                f.set_dof_shape(shape);
            }
            ui.end_row();
            slider(ui, "Shape scale", &mut f.cam_dof_scale, 0.0..=4.0);
            slider(ui, "Shape angle", &mut f.cam_dof_angle, -180.0..=180.0);
            slider(ui, "Fade", &mut f.cam_dof_fade, 0.0..=1.0);
            let names = f.cam_dof_shape.param_names();
            let params = [&mut f.cam_dof_param1, &mut f.cam_dof_param2, &mut f.cam_dof_param3, &mut f.cam_dof_param4, &mut f.cam_dof_param5, &mut f.cam_dof_param6];
            for (name, v) in names.iter().zip(params) {
                drag(ui, name, v, 0.01);
            }
        }
    });

    theme::section_label(ui, "Image size");
    grid(ui, "size", |ui| {
        widgets::label(ui, "Size");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut f.width).range(16..=16384));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut f.height).range(16..=16384));
        });
        ui.end_row();
        widgets::label(ui, "Presets");
        ui.horizontal_wrapped(|ui| {
            for (l, w, h) in [("16:9", 1920, 1080), ("4:3", 1600, 1200), ("1:1", 1200, 1200), ("3:4", 1200, 1600), ("9:16", 1080, 1920)] {
                if ui.small_button(l).clicked() {
                    // Keep the visible scale roughly constant.
                    let s = ((w as f64 / f.width as f64) + (h as f64 / f.height as f64)) / 2.0;
                    f.pixels_per_unit *= s;
                    f.width = w;
                    f.height = h;
                }
            }
        });
        ui.end_row();
    });
}

fn look_tab(ui: &mut Ui, doc: &mut Document, st: &mut InspectorState) {
    {
        let f = &mut doc.flame;
        theme::section_label(ui, "Tone");
        grid(ui, "tone", |ui| {
            slider(ui, "Brightness", &mut f.brightness, 0.0..=20.0);
            slider(ui, "Gamma", &mut f.gamma, 1.0..=8.0);
            slider(ui, "Gamma threshold", &mut f.gamma_threshold, 0.0..=0.2);
            slider(ui, "Vibrancy", &mut f.vibrancy, 0.0..=1.0);
            slider(ui, "Contrast", &mut f.contrast, 0.1..=4.0);
            slider(ui, "Saturation", &mut f.saturation, 0.0..=2.0);
            slider(ui, "White level", &mut f.white_level, 20.0..=400.0);
            slider(ui, "Low density", &mut f.low_density_brightness, 0.0..=1.0);
        });
        egui::CollapsingHeader::new(RichText::new("Color balance").color(theme::TEXT_MUTED)).show(ui, |ui| {
            grid(ui, "balance", |ui| {
                slider(ui, "Red", &mut f.balance_red, 0.0..=2.0);
                slider(ui, "Green", &mut f.balance_green, 0.0..=2.0);
                slider(ui, "Blue", &mut f.balance_blue, 0.0..=2.0);
            });
        });

        theme::section_label(ui, "Background");
        grid(ui, "bg", |ui| {
            widgets::label(ui, "Type");
            egui::ComboBox::from_id_salt("bgtype")
                .selected_text(match f.background.kind {
                    BgColorType::SingleColor => "Solid",
                    BgColorType::Gradient2x2 => "4 corners",
                    BgColorType::Gradient2x2C => "4 corners + center",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut f.background.kind, BgColorType::SingleColor, "Solid");
                    ui.selectable_value(&mut f.background.kind, BgColorType::Gradient2x2, "4 corners");
                    ui.selectable_value(&mut f.background.kind, BgColorType::Gradient2x2C, "4 corners + center");
                });
            ui.end_row();
            widgets::label(ui, "Colors");
            ui.horizontal(|ui| match f.background.kind {
                BgColorType::SingleColor => {
                    widgets::color_rgb(ui, &mut f.background.color);
                }
                k => {
                    widgets::color_rgb(ui, &mut f.background.ul);
                    widgets::color_rgb(ui, &mut f.background.ur);
                    widgets::color_rgb(ui, &mut f.background.ll);
                    widgets::color_rgb(ui, &mut f.background.lr);
                    if k == BgColorType::Gradient2x2C {
                        widgets::color_rgb(ui, &mut f.background.cc);
                    }
                }
            });
            ui.end_row();
            widgets::label(ui, "Transparent");
            ui.checkbox(&mut f.bg_transparency, "in exported images");
            ui.end_row();
        });
        super::mixer::section(ui, &mut f.mixer, &mut st.mixer);
    }

    gradient_editor(ui, doc, st);
}

fn gradient_editor(ui: &mut Ui, doc: &mut Document, st: &mut InspectorState) {
    theme::section_label(ui, "Gradient");
    let layer = doc.layer_mut();
    st.keys.show(ui, &mut layer.gradient);
    ui.add_space(2.0);
    let mut recapture = false;
    ui.horizontal(|ui| {
        if ui.button(format!("{} Library", icons::PALETTE)).on_hover_text("Browse ~1600 gradients, save your own").clicked() {
            st.library.open = true;
        }
        if ui.button(format!("{} Random", icons::DICE)).clicked() {
            let mut rng = rwf_core::rng::Rng::from_entropy();
            layer.gradient = Gradient::random(&mut rng);
            recapture = true;
        }
        if ui.button(format!("{} Reverse", icons::SWAP)).clicked() {
            layer.gradient.reverse();
            recapture = true;
        }
        ui.checkbox(&mut layer.smooth_gradient, "Smooth");
    });

    // Relative adjustments on top of a captured base gradient.
    let ge = &mut st.gradient;
    if recapture || ge.last.as_ref() != Some(&layer.gradient) {
        ge.base = Some(layer.gradient.clone());
        ge.shift = 0;
        ge.hue = 0.0;
        ge.sat = 0.0;
        ge.lum = 0.0;
    }
    egui::CollapsingHeader::new(RichText::new("Adjust").color(theme::TEXT_MUTED)).id_salt("grad-adjust").show(ui, |ui| {
        let mut changed = false;
        grid(ui, "grad", |ui| {
            widgets::label(ui, "Shift");
            changed |= ui.add(egui::Slider::new(&mut ge.shift, -128..=128)).changed();
            ui.end_row();
            changed |= slider(ui, "Hue", &mut ge.hue, -0.5..=0.5).changed();
            changed |= slider(ui, "Saturation", &mut ge.sat, -1.0..=1.0).changed();
            changed |= slider(ui, "Lightness", &mut ge.lum, -0.5..=0.5).changed();
        });
        if changed && let Some(base) = &ge.base {
            let mut g = base.clone();
            g.shift(ge.shift);
            if ge.hue != 0.0 || ge.sat != 0.0 || ge.lum != 0.0 {
                g.adjust_hsl(ge.hue, ge.sat, ge.lum);
            }
            layer.gradient = g;
        }
    });
    ge.last = Some(layer.gradient.clone());
}

fn render_tab(ui: &mut Ui, doc: &mut Document, preview: &mut Preview, canvas: &mut CanvasState) {
    theme::section_label(ui, "Preview");
    grid(ui, "preview", |ui| {
        widgets::label(ui, "Renderer");
        ui.horizontal(|ui| {
            ui.add_enabled_ui(preview.gpu_available(), |ui| {
                ui.selectable_value(&mut preview.backend, Backend::Gpu, "GPU");
            });
            ui.selectable_value(&mut preview.backend, Backend::Cpu, "CPU");
        });
        ui.end_row();
        slider_log(ui, "Target quality", &mut preview.target_quality, 10.0..=5000.0);
        widgets::label(ui, "Resolution");
        ui.horizontal(|ui| {
            for (s, l) in [(0.5, "½"), (0.75, "¾"), (1.0, "Full")] {
                ui.selectable_value(&mut canvas.resolution_scale, s, l);
            }
        });
        ui.end_row();
    });
    if let Some(name) = preview.adapter_name() {
        ui.label(RichText::new(format!("GPU: {name}")).small().color(theme::TEXT_MUTED));
    }

    let f = &mut doc.flame;
    theme::section_label(ui, "Quality & filtering");
    grid(ui, "quality", |ui| {
        slider_log(ui, "Quality", &mut f.sample_density, 10.0..=10000.0);
        widgets::label(ui, "Filter");
        egui::ComboBox::from_id_salt("kernel").selected_text(f.filter_kernel.label()).show_ui(ui, |ui| {
            for k in FilterKernel::ALL {
                ui.selectable_value(&mut f.filter_kernel, *k, k.label());
            }
        });
        ui.end_row();
        slider(ui, "Filter radius", &mut f.spatial_filter_radius, 0.0..=3.0);
        if f.filter_kernel.is_adaptive() {
            slider(ui, "Sharpness", &mut f.filter_sharpness, 0.0..=15.0).on_hover_text("Edge threshold: below it the soft kernel is used");
            slider(ui, "Low density", &mut f.filter_low_density, 0.0..=0.5).on_hover_text("Intensity below which the wide low-density kernel is used");
            widgets::label(ui, "Indicator");
            ui.checkbox(&mut f.filter_indicator, "Show chosen kernel").on_hover_text("Red: detail, green: soft, blue: low density");
            ui.end_row();
        }
        widgets::label(ui, "Oversampling");
        ui.add(egui::Slider::new(&mut f.spatial_oversampling, 1..=4));
        ui.end_row();
        slider(ui, "Antialias amount", &mut f.antialias_amount, 0.0..=1.0);
        slider(ui, "Antialias radius", &mut f.antialias_radius, 0.0..=2.0);
    });

    theme::section_label(ui, "Post blur");
    grid(ui, "post_blur", |ui| {
        widgets::label(ui, "Radius");
        ui.add(egui::Slider::new(&mut f.post_blur_radius, 0..=16)).on_hover_text("Splats samples over a radial kernel (0 = off)");
        ui.end_row();
        if f.post_blur_radius > 0 {
            slider(ui, "Fade", &mut f.post_blur_fade, 0.0..=1.0).on_hover_text("Fraction of samples that are not blurred");
            slider(ui, "Falloff", &mut f.post_blur_falloff, 0.0..=10.0);
        }
    });

    super::solid::solid_section(ui, f);
    let f = &mut doc.flame;

    theme::section_label(ui, "Denoiser");
    grid(ui, "denoiser", |ui| {
        widgets::label(ui, "AI denoiser");
        egui::ComboBox::from_id_salt("denoiser").selected_text(f.ai_post_denoiser.label()).show_ui(ui, |ui| {
            for d in Denoiser::ALL {
                ui.selectable_value(&mut f.ai_post_denoiser, *d, d.label());
            }
        });
        ui.end_row();
        if f.ai_post_denoiser != Denoiser::None {
            slider(ui, "Blend", &mut f.post_denoiser_blend, 0.0..=1.0).on_hover_text("Mixes the original image back in (0 = fully denoised)");
            widgets::label(ui, "");
            ui.checkbox(&mut f.post_denoiser_only_for_cpu, "Only for CPU renders");
            ui.end_row();
        }
    });
    if f.ai_post_denoiser != Denoiser::None && denoise::find_tool().is_none() {
        ui.label(
            RichText::new(format!("oidnDenoise not found (PATH or {}); exports stay un-denoised", denoise::TOOL_ENV)).small().color(theme::WARN),
        );
    }

    theme::section_label(ui, "Density estimation");
    grid(ui, "de", |ui| {
        slider(ui, "Radius", &mut f.de_radius, 0.0..=2.0).on_hover_text("Adaptive smoothing of sparse areas (0 = off)");
        slider(ui, "Curve", &mut f.de_curve, 0.0..=1.0);
        slider(ui, "Compare line", &mut f.de_comparison_line, 0.0..=1.0).on_hover_text("Shows the image with/without density estimation side by side (0 = off)");
    });

    theme::section_label(ui, "Symmetry");
    let ps = &mut f.post_symmetry;
    grid(ui, "sym", |ui| {
        widgets::label(ui, "Type");
        egui::ComboBox::from_id_salt("symtype").selected_text(format!("{:?}", ps.kind)).show_ui(ui, |ui| {
            for (k, l) in [(PostSymmetryType::None, "None"), (PostSymmetryType::Point, "Point"), (PostSymmetryType::XAxis, "X axis"), (PostSymmetryType::YAxis, "Y axis")] {
                ui.selectable_value(&mut ps.kind, k, l);
            }
        });
        ui.end_row();
        match ps.kind {
            PostSymmetryType::None => {}
            PostSymmetryType::Point => {
                widgets::label(ui, "Order");
                ui.add(egui::Slider::new(&mut ps.order, 2..=32));
                ui.end_row();
                drag(ui, "Center X", &mut ps.centre_x, 0.01);
                drag(ui, "Center Y", &mut ps.centre_y, 0.01);
            }
            _ => {
                drag(ui, "Center X", &mut ps.centre_x, 0.01);
                drag(ui, "Center Y", &mut ps.centre_y, 0.01);
                slider(ui, "Distance", &mut ps.distance, -4.0..=4.0);
                slider(ui, "Rotation", &mut ps.rotation, -180.0..=180.0);
            }
        }
    });

    theme::section_label(ui, "Advanced");
    ui.checkbox(&mut f.preserve_z, "Preserve Z (2D variations keep the z coordinate)");
}
