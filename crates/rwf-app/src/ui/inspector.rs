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
use rwf_core::model::{BgColorType, ColorType, DrawMode, FlameSection, PostSymmetryType};
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
    /// JWildfire's gradient modifications, plus a lightness change.
    mods: rwf_core::gradient_ops::GradientMods,
    lum: f64,
    /// Range of entries for the range operations, and copied colors.
    from: usize,
    to: usize,
    clipboard: Vec<[u8; 3]>,
}

#[derive(Default)]
pub struct InspectorState {
    pub tab: Tab,
    pub gradient: GradientEdit,
    pub keys: crate::gradients::KeyEditor,
    pub curves: crate::gradients::CurvesEdit,
    /// Random gradient settings: generator (`None`: RWildFire's own), key points, fade, uniform.
    random_gen: Option<rwf_core::random::gradients::GradientGenerator>,
    random_points: usize,
    random_fade: bool,
    random_uniform: bool,
    grad_status: Option<String>,
    pub library: crate::gradients::GradientLibrary,
    pub wfield_preview: super::weighting::FieldPreview,
    pub mixer: super::mixer::MixerState,
    rotate_step: f64,
    scale_step: f64,
    move_step: f64,
    /// Xaos shown as the probabilities into the selected transform instead of out of it.
    xaos_to: bool,
    /// A running z-buffer suggestion render, and its last outcome.
    zsuggest: Option<std::sync::mpsc::Receiver<Option<(f64, f64, f64)>>>,
    zsuggest_status: Option<String>,
}

impl InspectorState {
    /// Rotate (degrees), scale (percent) and move steps of the transform buttons and keys.
    pub fn steps(&self) -> (f64, f64, f64) {
        if self.rotate_step == 0.0 { (15.0, 10.0, 0.1) } else { (self.rotate_step, self.scale_step, self.move_step) }
    }
}

/// Layer-wide color actions (`Layer.distributeColors` etc.).
#[derive(Clone, Copy)]
enum ColorAction {
    Distribute,
    Random,
    RandomSpeed,
    Reset,
    RandomCycle,
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
        Tab::Render => render_tab(ui, doc, preview, canvas, st),
    });
}

fn transform_tab(ui: &mut Ui, doc: &mut Document, st: &mut InspectorState) {
    let solid = doc.flame.solid.enabled;
    let n_xforms = doc.layer().xforms.len();
    let gradient = doc.layer().gradient.clone();
    let sel = doc.selection;
    let edit_post = doc.edit_post;
    let plane = doc.edit_plane;
    let mut color_action = None;
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
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("All transforms:").small().color(theme::TEXT_MUTED));
        for (a, label, tip) in [
            (ColorAction::Distribute, "Distribute", "Spread the color positions evenly over the gradient"),
            (ColorAction::Random, "Random", "Random color positions"),
            (ColorAction::RandomSpeed, "Random speed", "Random color speeds"),
            (ColorAction::Reset, "Reset", "Color positions and speeds to 0"),
            (ColorAction::RandomCycle, "Random cycle", "Random gradient color cycle (Look tab)"),
        ] {
            if ui.small_button(label).on_hover_text(tip).clicked() {
                color_action = Some(a);
            }
        }
    });

    let title = match (edit_post, plane) {
        (false, crate::doc::EditPlane::XY) => "Affine".to_string(),
        (true, crate::doc::EditPlane::XY) => "Post affine".to_string(),
        (false, p) => format!("Affine ({})", p.label()),
        (true, p) => format!("Post affine ({})", p.label()),
    };
    theme::section_label(ui, &title);
    let a = crate::doc::edit_affine_mut(xf, plane, edit_post);
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

    if let Some(a) = color_action {
        apply_color_action(doc, a);
    }
    if let Selection::XForm(i) = sel
        && n_xforms > 1
    {
        xaos_section(ui, doc, st, i);
    }
}

/// Z-buffer output settings and JWildfire's "suggest" (a quick depth render).
fn zbuffer_section(ui: &mut Ui, f: &mut rwf_core::Flame, st: &mut InspectorState) {
    use rwf_core::model::ZBufferFilename;
    if let Some(rx) = &st.zsuggest {
        match rx.try_recv() {
            Ok(Some((scale, shift, _bias))) => {
                // JWildfire applies scale and shift
                (f.zbuffer_scale, f.zbuffer_shift) = (scale, shift);
                st.zsuggest_status = Some("Scale and shift fitted to the depth range of the flame".into());
                st.zsuggest = None;
            }
            Ok(None) => {
                st.zsuggest_status = Some("No depth found (is the flame visible?)".into());
                st.zsuggest = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(100)),
            Err(_) => st.zsuggest = None,
        }
    }
    egui::CollapsingHeader::new(RichText::new("Z-buffer").color(theme::TEXT_MUTED)).id_salt("zbuffer").show(ui, |ui| {
        ui.label(RichText::new("How depth maps to gray in z-buffer images (Export › Z-buffer, rwf-cli render --zbuffer).").small().color(theme::TEXT_MUTED));
        grid(ui, "zbuf", |ui| {
            slider(ui, "Scale", &mut f.zbuffer_scale, -10.0..=10.0).on_hover_text("Negative: black near the camera");
            slider(ui, "Shift", &mut f.zbuffer_shift, -2.0..=2.0);
            slider(ui, "Bias", &mut f.zbuffer_bias, 0.0..=1.0).on_hover_text("Gray level of the empty background");
            widgets::label(ui, "File name");
            ui.horizontal(|ui| {
                ui.selectable_value(&mut f.zbuffer_filename, ZBufferFilename::PostDepth, "image_depth.png");
                ui.selectable_value(&mut f.zbuffer_filename, ZBufferFilename::PreZbuf, "zbuf_image.png");
            });
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if st.zsuggest.is_some() {
                ui.spinner();
                ui.label(RichText::new("Measuring depth…").small());
            } else if ui.button("Suggest").on_hover_text("Renders the depth quickly and fits scale and shift to it (JWildfire's suggestion)").clicked() {
                let mut flame = f.clone();
                flame.sample_density = 50.0;
                let k = (640.0 / flame.width.max(flame.height).max(1) as f64).min(1.0);
                let (w, h) = (((flame.width as f64 * k) as u32).max(16), ((flame.height as f64 * k) as u32).max(16));
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let threads = crate::settings::cpu_threads();
                    let opts = rwf_core::render::RenderOptions { width: w, height: h, threads, max_density: Some(50.0), seed: 7, zbuffer: true };
                    let r = rwf_core::render::CpuRender::start(&flame, opts).wait();
                    let _ = tx.send(r.zbuffer_suggestion());
                });
                st.zsuggest = Some(rx);
                st.zsuggest_status = None;
            }
            if let Some(s) = &st.zsuggest_status {
                ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
            }
        });
    });
}

fn apply_color_action(doc: &mut Document, a: ColorAction) {
    let layer = doc.layer_mut();
    let n = layer.xforms.len();
    let mut rng = rwf_core::rng::Rng::from_entropy();
    match a {
        ColorAction::Distribute if n > 1 => {
            for (i, xf) in layer.xforms.iter_mut().enumerate() {
                xf.color = i as f64 / (n - 1) as f64;
            }
        }
        ColorAction::Random => layer.xforms.iter_mut().for_each(|xf| xf.color = rng.random()),
        ColorAction::RandomSpeed => layer.xforms.iter_mut().for_each(|xf| xf.color_symmetry = rng.random()),
        ColorAction::Reset => layer.xforms.iter_mut().for_each(|xf| {
            xf.color = 0.0;
            xf.color_symmetry = 0.0;
        }),
        // JWildfire: 255 - random(511)
        ColorAction::RandomCycle => layer.gradient_shift = (255 - (rng.random() * 511.0) as i32) as f64,
        _ => {}
    }
}

/// Xaos of transform `i`: the probabilities to jump from it to each transform, or into it from
/// each transform.
fn xaos_section(ui: &mut Ui, doc: &mut Document, st: &mut InspectorState, i: usize) {
    ui.add_space(4.0);
    egui::CollapsingHeader::new(RichText::new("Xaos (jump probabilities)").color(theme::TEXT_MUTED)).id_salt("xaos").show(ui, |ui| {
        let layer = doc.layer_mut();
        let n = layer.xforms.len();
        for xf in layer.xforms.iter_mut() {
            if xf.chaos.len() < n {
                xf.chaos.resize(n, 1.0);
            }
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut st.xaos_to, false, "From this").on_hover_text("How likely the next transform after this one is each transform");
            ui.selectable_value(&mut st.xaos_to, true, "To this").on_hover_text("How likely each transform is followed by this one");
        });
        grid(ui, "xaos", |ui| {
            for j in 0..n {
                if st.xaos_to {
                    slider(ui, &crate::icons::arrows(&format!("Transform {} →", j + 1)), &mut layer.xforms[j].chaos[i], 0.0..=2.0);
                } else {
                    slider(ui, &crate::icons::arrows(&format!("→ Transform {}", j + 1)), &mut layer.xforms[i].chaos[j], 0.0..=2.0);
                }
            }
        });
        ui.horizontal(|ui| {
            for (v, label) in [(0.0, "All 0"), (1.0, "All 1")] {
                if ui.button(label).on_hover_text("Set all the values shown above").clicked() {
                    for j in 0..n {
                        if st.xaos_to {
                            layer.xforms[j].chaos[i] = v;
                        } else {
                            layer.xforms[i].chaos[j] = v;
                        }
                    }
                }
            }
            if ui.button("Reset all").on_hover_text("Xaos of every transform back to 1 (no xaos)").clicked() {
                layer.xforms.iter_mut().for_each(|xf| xf.chaos.clear());
            }
        });
    });
}

fn camera_tab(ui: &mut Ui, doc: &mut Document) {
    use super::camera_gizmo::{self as gizmo, Wheel};
    let f = &mut doc.flame;
    if widgets::section_reset(ui, "View") {
        f.reset_section(FlameSection::View);
    }
    grid(ui, "cam", |ui| {
        drag(ui, "Center X", &mut f.centre_x, 0.005);
        drag(ui, "Center Y", &mut f.centre_y, 0.005);
        gizmo::wheel_row(ui, "Scale", &mut f.pixels_per_unit, Wheel::Log(150.0), 0.01..=1e6, 50.0, 3);
        gizmo::wheel_row(ui, "Zoom", &mut f.cam_zoom, Wheel::Log(150.0), 0.001..=1000.0, 1.0, 3);
    });
    if ui.button(format!("{}  Fit flame to view", icons::FIT)).clicked() {
        let mut c = f.clone();
        if rwf_core::render::analysis::auto_frame(&mut c) {
            *f = c;
        }
    }
    if widgets::section_reset(ui, "Orientation") {
        f.reset_section(FlameSection::Orientation);
    }
    ui.vertical_centered(|ui| {
        gizmo::orbit_globe(ui, f, 74.0, false);
        gizmo::angle_fields(ui, f);
    });
    if widgets::section_reset(ui, "3D camera") {
        f.reset_section(FlameSection::Camera3d);
    }
    grid(ui, "cam3d", |ui| {
        gizmo::wheel_row(ui, "Perspective", &mut f.cam_perspective, Wheel::Linear(0.002), 0.0..=10.0, 0.0, 4);
        gizmo::wheel_row(ui, "Dolly", &mut f.cam_pos_z, Wheel::Linear(0.01), f64::MIN..=f64::MAX, 0.0, 4).on_hover_text("Camera position Z");
        drag(ui, "Position X", &mut f.cam_pos_x, 0.01);
        drag(ui, "Position Y", &mut f.cam_pos_y, 0.01);
        slider(ui, "Depth fade", &mut f.dimish_z, 0.0..=4.0);
        drag(ui, "Fade distance", &mut f.dim_z_distance, 0.01);
        widgets::label(ui, "Fade color");
        widgets::color_rgb(ui, &mut f.dim_z_color).on_hover_text("Color that distant points fade into with \"Depth fade\" (black: they just get darker)");
        ui.end_row();
    });
    egui::CollapsingHeader::new(RichText::new("Stereo 3D").color(theme::TEXT_MUTED)).id_salt("stereo3d").show(ui, |ui| {
        use rwf_core::stereo::{Stereo3dColor, Stereo3dMode};
        ui.label(RichText::new("Rendered in exported images (Export, rwf-cli render, batch); the canvas shows the normal view.").small().color(theme::TEXT_MUTED));
        if ui.small_button(format!("{} Reset", icons::REFRESH)).on_hover_text("Stereo settings back to the defaults").clicked() {
            f.reset_section(FlameSection::Stereo);
        }
        let st = &mut f.stereo3d;
        grid(ui, "stereo-grid", |ui| {
            widgets::label(ui, "Mode");
            egui::ComboBox::from_id_salt("stereo-mode").selected_text(st.mode.label()).show_ui(ui, |ui| {
                for m in Stereo3dMode::ALL {
                    ui.selectable_value(&mut st.mode, m, m.label());
                }
            })
            .response
            .on_hover_text("Anaglyph: one image for red/cyan glasses. Side by side: both eyes next to each other. Interpolated pairs: side-by-side pairs at growing eye distance, one row each.");
            ui.end_row();
            if st.is_on() {
                slider(ui, "Eye angle", &mut st.angle, 0.0..=10.0).on_hover_text("How far each eye turns towards the focal point (degrees)");
                slider(ui, "Eye distance", &mut st.eye_dist, 0.0..=0.5).on_hover_text("Distance of each eye from the center");
                slider(ui, "Focal offset", &mut st.focal_offset, -2.0..=2.0).on_hover_text("Depth of the point the eyes turn around");
                if st.mode == Stereo3dMode::Anaglyph {
                    for (label, c) in [("Left eye", &mut st.left_color), ("Right eye", &mut st.right_color)] {
                        widgets::label(ui, label);
                        ui.horizontal(|ui| {
                            ui.selectable_value(c, Stereo3dColor::Red, "Red");
                            ui.selectable_value(c, Stereo3dColor::Cyan, "Cyan");
                        });
                        ui.end_row();
                    }
                }
                if st.mode == Stereo3dMode::InterpolatedImages {
                    widgets::label(ui, "Pairs");
                    ui.add(egui::DragValue::new(&mut st.interpolated_image_count).range(1..=12));
                    ui.end_row();
                }
                widgets::label(ui, "");
                ui.checkbox(&mut st.swap_sides, "Swap eyes").on_hover_text("For cross-eyed viewing or glasses with swapped colors");
                ui.end_row();
            }
        });
    });

    if widgets::section_reset(ui, "Depth of field") {
        f.reset_section(FlameSection::DepthOfField);
    }
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

/// The post denoiser settings of a flame.
pub fn denoiser_settings(ui: &mut Ui, id: &str, f: &mut rwf_core::Flame) {
    grid(ui, id, |ui| {
        widgets::label(ui, "AI denoiser");
        egui::ComboBox::from_id_salt(format!("{id}-kind")).selected_text(f.ai_post_denoiser.label()).show_ui(ui, |ui| {
            for d in Denoiser::ALL {
                ui.selectable_value(&mut f.ai_post_denoiser, *d, d.label());
            }
        });
        ui.end_row();
        if f.ai_post_denoiser != Denoiser::None {
            slider(ui, "Blend", &mut f.post_denoiser_blend, 0.0..=1.0).on_hover_text("Mixes the original image back in (0 = fully denoised)");
            slider(ui, "Keep detail", &mut f.post_denoiser_detail, 0.0..=1.0).on_hover_text(
                "Keeps the original image at lines and edges where the render already has enough samples per pixel: the denoiser takes thin lines for noise and softens them. Higher values count pixels with fewer samples as converged. 0 = off; not for solid flames.",
            );
            slider(ui, "Sharpen", &mut f.post_denoiser_sharpen, 0.0..=1.0)
                .on_hover_text("Sharpens the denoised image (contrast adaptive sharpening: little effect on edges that are already sharp, few halos). 0 = off");
            widgets::label(ui, "");
            ui.checkbox(&mut f.post_denoiser_only_for_cpu, "Only for CPU renders");
            ui.end_row();
        }
    });
}

fn look_tab(ui: &mut Ui, doc: &mut Document, st: &mut InspectorState) {
    {
        let f = &mut doc.flame;
        theme::section_label(ui, "Flame");
        grid(ui, "flame-name", |ui| {
            widgets::label(ui, "Name");
            ui.text_edit_singleline(&mut f.name).on_hover_text("Saved with the flame; shown in the title bar when the flame has no file");
            ui.end_row();
        });
        if widgets::section_reset(ui, "Tone") {
            f.reset_section(FlameSection::Tone);
        }
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

        if widgets::section_reset(ui, "Background") {
            f.reset_section(FlameSection::Background);
        }
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
            widgets::label(ui, "Image");
            ui.horizontal(|ui| {
                let name = std::path::Path::new(&f.bg_image).file_name().map(|n| n.to_string_lossy().to_string());
                let missing = !f.bg_image.is_empty() && rwf_core::render::background::background_image(f).is_none();
                if ui.button(name.clone().unwrap_or_else(|| "Choose…".into())).on_hover_text("Image shown behind the flame instead of the colors above, stretched to the image size").clicked()
                    && let Some(p) = crate::settings::dialog(crate::settings::Folder::Images).add_filter("Image", &["png", "jpg", "jpeg"]).pick_file()
                {
                    f.bg_image = p.to_string_lossy().to_string();
                }
                if !f.bg_image.is_empty() && widgets::icon_button(ui, icons::X, "Remove the background image").clicked() {
                    f.bg_image.clear();
                }
                if missing {
                    ui.label(RichText::new("not found").small().color(theme::WARN)).on_hover_text(format!("Cannot load {}; the colors above are used", f.bg_image));
                }
            });
            ui.end_row();
            slider(ui, "Foreground opacity", &mut f.fg_opacity, 0.0..=3.0)
                .on_hover_text("How much the flame covers the background in transparent images (1 = normal, less = more see-through)");
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
        if ui.button(format!("{} Random", icons::DICE)).on_hover_text("A random gradient with the settings of the menu next to it").clicked() {
            let mut rng = rwf_core::rng::Rng::from_entropy();
            layer.gradient = match st.random_gen {
                None => Gradient::random(&mut rng),
                Some(g) => g.generate(if st.random_points < 3 { 7 } else { st.random_points }, st.random_fade, st.random_uniform, &mut rng),
            };
            recapture = true;
        }
        ui.menu_button(icons::SLIDERS, |ui| {
            use rwf_core::random::gradients::GradientGenerator;
            if st.random_points < 3 {
                (st.random_points, st.random_fade) = (7, true);
            }
            widgets::label(ui, "Random gradients");
            egui::ComboBox::from_id_salt("rnd-grad").selected_text(st.random_gen.map_or("RWildFire", |g| g.label())).show_ui(ui, |ui| {
                ui.selectable_value(&mut st.random_gen, None, "RWildFire").on_hover_text("A few harmonic hues");
                for g in GradientGenerator::ALL {
                    ui.selectable_value(&mut st.random_gen, Some(*g), g.label()).on_hover_text("JWildfire's generator");
                }
            });
            if st.random_gen.is_some() {
                ui.add(egui::Slider::new(&mut st.random_points, 3..=58).text("key colors"));
                ui.checkbox(&mut st.random_fade, "Fade between the colors");
                ui.checkbox(&mut st.random_uniform, "Uniform spacing");
            }
            ui.separator();
            if ui.button("Similar to this gradient").on_hover_text("Clusters the colors of the gradient and builds a new one from them").clicked() {
                let mut rng = rwf_core::rng::Rng::from_entropy();
                layer.gradient = rwf_core::random::gradients::similar_gradient(&layer.gradient, &mut rng);
                recapture = true;
            }
        })
        .response
        .on_hover_text("Random gradient settings");
        if ui.button(format!("{} Reverse", icons::SWAP)).clicked() {
            layer.gradient.reverse();
            recapture = true;
        }
        ui.checkbox(&mut layer.smooth_gradient, "Smooth");
    });
    ui.horizontal(|ui| {
        if ui.button("From image…").on_hover_text("The 256 colors that represent an image best, sorted by hue (JWildfire's median cut)").clicked()
            && let Some(p) = crate::settings::dialog(crate::settings::Folder::Images).add_filter("Image", &["png", "jpg", "jpeg"]).pick_file()
        {
            match rwf_core::resources::image(&p.to_string_lossy()) {
                Some(img) => {
                    layer.gradient = rwf_core::gradient_ops::gradient_from_image(&img);
                    recapture = true;
                    st.grad_status = None;
                }
                None => st.grad_status = Some(format!("Cannot read {}", p.display())),
            }
        }
        if ui.button("From flame…").on_hover_text("The gradient of a flame file (its first flame and layer)").clicked()
            && let Some(p) = crate::settings::dialog(crate::settings::Folder::Flames).add_filter("Flame", &["flame", "xml"]).pick_file()
        {
            match rwf_core::io::read_flames_from_file(&p) {
                Ok(f) if !f.is_empty() => {
                    layer.gradient = f[0].first_layer().gradient.clone();
                    recapture = true;
                    st.grad_status = None;
                }
                _ => st.grad_status = Some(format!("No flame in {}", p.display())),
            }
        }
        if ui.button("Export…").on_hover_text("Saves the gradient as .map (Fractint/JWildfire) or .ugr (Ultra Fractal)").clicked()
            && let Some(p) = crate::settings::dialog(crate::settings::Folder::Gradients).add_filter("Map gradient", &["map"]).add_filter("Ultra Fractal gradient", &["ugr"]).set_file_name("gradient.map").save_file()
        {
            let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let text = if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ugr")) {
                rwf_core::gradient_io::write_ugr(&[rwf_core::gradient_io::NamedGradient { name, gradient: layer.gradient.clone() }])
            } else {
                rwf_core::gradient_io::write_map(&layer.gradient, &name)
            };
            st.grad_status = Some(match std::fs::write(&p, text) {
                Ok(()) => format!("Saved {}", p.display()),
                Err(e) => format!("Could not save: {e}"),
            });
        }
    });
    if let Some(s) = &st.grad_status {
        ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
    }
    grid(ui, "grad-cycle", |ui| {
        slider(ui, "Color cycle", &mut layer.gradient_shift, 0.0..=256.0).on_hover_text(
            "Moves the colors along the gradient when rendering, without changing the gradient (256 = one full turn). \
             Key it on the timeline to cycle the colors in animations; fractional values blend neighbouring colors.",
        );
    });

    // Relative adjustments on top of a captured base gradient.
    let ge = &mut st.gradient;
    if recapture || ge.last.as_ref() != Some(&layer.gradient) {
        ge.base = Some(layer.gradient.clone());
        ge.mods = Default::default();
        ge.lum = 0.0;
    }
    egui::CollapsingHeader::new(RichText::new("Adjust").color(theme::TEXT_MUTED)).id_salt("grad-adjust").show(ui, |ui| {
        let mut changed = false;
        let m = &mut ge.mods;
        grid(ui, "grad", |ui| {
            let mut int = |ui: &mut Ui, label: &str, v: &mut i32, range: std::ops::RangeInclusive<i32>, tip: &str| {
                widgets::label(ui, label);
                changed |= ui.add(egui::Slider::new(v, range)).on_hover_text(tip).changed();
                ui.end_row();
            };
            int(ui, "Shift", &mut m.shift, -255..=255, "Moves the colors along the gradient");
            int(ui, "Frequency", &mut m.frequency, 1..=16, "Repeats the gradient");
            int(ui, "Blur", &mut m.blur, 0..=127, "Averages neighbouring colors");
            int(ui, "Hue", &mut m.hue, -255..=255, "Turns the hues");
            int(ui, "Saturation", &mut m.saturation, -255..=255, "");
            int(ui, "Brightness", &mut m.brightness, -255..=255, "");
            int(ui, "Contrast", &mut m.contrast, -255..=255, "");
            int(ui, "Gamma", &mut m.gamma, -255..=255, "Midtones");
            int(ui, "Red", &mut m.red, -255..=255, "");
            int(ui, "Green", &mut m.green, -255..=255, "");
            int(ui, "Blue", &mut m.blue, -255..=255, "");
            int(ui, "Swap RGB", &mut m.swap_rgb, 0..=255, "Swaps the color channels (RBG, GRB, GBR, BRG, BGR over the range)");
            changed |= slider(ui, "Lightness", &mut ge.lum, -0.5..=0.5).changed();
        });
        if ui.small_button(format!("{} Reset", icons::REFRESH)).on_hover_text("Back to the gradient before these adjustments").clicked() {
            ge.mods = Default::default();
            ge.lum = 0.0;
            changed = true;
        }
        if changed && let Some(base) = &ge.base {
            let mut g = base.with_mods(&ge.mods);
            if ge.lum != 0.0 {
                g.adjust_hsl(0.0, 0.0, ge.lum);
            }
            layer.gradient = g;
        }
    });
    egui::CollapsingHeader::new(RichText::new("Range").color(theme::TEXT_MUTED)).id_salt("grad-range").show(ui, |ui| {
        let n = layer.gradient.colors.len().max(1);
        let strip = widgets::gradient_strip(ui, &layer.gradient, 14.0, None);
        let r = strip.rect;
        let (a, b) = (ge.from.min(ge.to), ge.from.max(ge.to));
        let x = |i: usize| r.left() + r.width() * i as f32 / n as f32;
        ui.painter().rect_stroke(egui::Rect::from_x_y_ranges(x(a)..=x(b + 1), r.y_range()), 0, egui::Stroke::new(2.0, theme::ACCENT), egui::StrokeKind::Outside);
        // drag on the strip to select a range
        if let Some(p) = strip.interact_pointer_pos() {
            let i = (((p.x - r.left()) / r.width() * n as f32) as usize).min(n - 1);
            if strip.drag_started() || strip.clicked() {
                ge.from = i;
            }
            ge.to = i;
        }
        ui.horizontal(|ui| {
            widgets::label(ui, "From");
            ui.add(egui::DragValue::new(&mut ge.from).range(0..=n - 1));
            widgets::label(ui, "to");
            ui.add(egui::DragValue::new(&mut ge.to).range(0..=n - 1));
            if ui.small_button("All").clicked() {
                (ge.from, ge.to) = (0, n - 1);
            }
        });
        let (a, b) = (ge.from.min(ge.to), ge.from.max(ge.to));
        let g = &mut layer.gradient;
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Copy").clicked() {
                ge.clipboard = g.copy_range(a, b);
            }
            if ui.add_enabled(!ge.clipboard.is_empty(), egui::Button::new("Paste").small()).on_hover_text("The copied colors from the start of the range").clicked() {
                g.paste_range(a, b, &ge.clipboard);
            }
            if ui.small_button("Erase").on_hover_text("Black").clicked() {
                g.erase_range(a, b);
            }
            if ui.small_button("Fade").on_hover_text("Straight transition between the range's first and last color").clicked() {
                g.fade_range(a, b);
            }
            if ui.small_button("Monochrome").on_hover_text("One hue (the average) for the whole range").clicked() {
                g.monochrome_range(a, b);
            }
            if ui.small_button("Reverse").clicked() {
                g.reverse_range(a, b);
            }
            if ui.small_button("Sort").on_hover_text("By hue, then brightness").clicked() {
                g.sort_range(a, b);
            }
            if ui.small_button("Negative").clicked() {
                g.negative_range(a, b);
            }
        });
        if ui.small_button("Fade all").on_hover_text("Fills the black gaps of the whole gradient with transitions between its colors").clicked() {
            g.fade_all();
        }
    });
    egui::CollapsingHeader::new(RichText::new("Curves").color(theme::TEXT_MUTED)).id_salt("grad-curves").show(ui, |ui| {
        st.curves.show(ui, layer);
    });
    let ge = &mut st.gradient;
    ge.last = Some(layer.gradient.clone());

    egui::CollapsingHeader::new(RichText::new("Gradient map").color(theme::TEXT_MUTED))
        .id_salt("grad-map")
        .show(ui, |ui| {
            ui.label(RichText::new("Colors from an image instead of the gradient (transforms with diffusion or cyclic coloring), as in JWildfire.").small().color(theme::TEXT_MUTED));
            let gm = &mut layer.gradient_map;
            grid(ui, "grad-map-grid", |ui| {
                widgets::label(ui, "Image");
                ui.horizontal(|ui| {
                    let name = std::path::Path::new(&gm.file).file_name().map(|n| n.to_string_lossy().to_string());
                    if ui.button(name.unwrap_or_else(|| "Choose…".into())).on_hover_text("Image used as a two-dimensional gradient").clicked()
                        && let Some(p) = crate::settings::dialog(crate::settings::Folder::Images).add_filter("Image", &["png", "jpg", "jpeg"]).pick_file()
                    {
                        gm.file = p.to_string_lossy().to_string();
                    }
                    if gm.is_on() && widgets::icon_button(ui, icons::X, "Remove the gradient map (use the gradient again)").clicked() {
                        gm.file.clear();
                    }
                    if gm.is_on() && rwf_core::resources::image(&gm.file).is_none() {
                        ui.label(RichText::new("not found").small().color(theme::WARN))
                            .on_hover_text(format!("Cannot load {}; JWildfire's default gradient image is used", gm.file));
                    }
                });
                ui.end_row();
                if gm.is_on() {
                    slider(ui, "Horiz. scale", &mut gm.h_scale, -4.0..=4.0).on_hover_text("How far a point's x moves across the image (it is mirrored at the edges)");
                    slider(ui, "Horiz. offset", &mut gm.h_offset, -2.0..=2.0);
                    slider(ui, "Vert. scale", &mut gm.v_scale, -4.0..=4.0).on_hover_text("How far a point's y moves across the image (it is mirrored at the edges)");
                    slider(ui, "Vert. offset", &mut gm.v_offset, -2.0..=2.0);
                    slider(ui, "Color shift", &mut gm.local_color_add, -2.0..=2.0).on_hover_text("How much the color index moves the position in the image");
                    slider(ui, "Color scale", &mut gm.local_color_scale, -2.0..=2.0).on_hover_text("How much the color index scales the position in the image");
                }
            });
        });
}

fn render_tab(ui: &mut Ui, doc: &mut Document, preview: &mut Preview, canvas: &mut CanvasState, st: &mut InspectorState) {
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
    if widgets::section_reset(ui, "Quality & filtering") {
        let q = f.sample_density;
        f.reset_section(FlameSection::Filter);
        f.sample_density = q;
    }
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

    if widgets::section_reset(ui, "Post blur") {
        f.reset_section(FlameSection::PostBlur);
    }
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
    ui.label(RichText::new("Applied to saved images and videos; the canvas shows it with \"Denoised\".").small().color(theme::TEXT_MUTED));
    denoiser_settings(ui, "denoiser", f);
    if f.ai_post_denoiser != Denoiser::None {
        match denoise::backend() {
            Ok(b) => {
                ui.add(egui::Label::new(RichText::new(format!("Used for exports and renders: {b}")).small().color(theme::TEXT_MUTED)).truncate());
            }
            Err(e) => {
                ui.label(RichText::new(format!("{e}; exports stay un-denoised")).small().color(theme::WARN));
            }
        }
    }

    if widgets::section_reset(ui, "Density estimation") {
        f.reset_section(FlameSection::DensityEstimation);
    }
    grid(ui, "de", |ui| {
        slider(ui, "Radius", &mut f.de_radius, 0.0..=2.0).on_hover_text("Adaptive smoothing of sparse areas (0 = off)");
        slider(ui, "Curve", &mut f.de_curve, 0.0..=1.0);
        slider(ui, "Compare line", &mut f.de_comparison_line, 0.0..=1.0).on_hover_text("Shows the image with/without density estimation side by side (0 = off)");
    });

    if widgets::section_reset(ui, "Symmetry") {
        f.reset_section(FlameSection::Symmetry);
    }
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

    zbuffer_section(ui, f, st);

    theme::section_label(ui, "Advanced");
    ui.checkbox(&mut f.preserve_z, "Preserve Z (2D variations keep the z coordinate)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_actions_cover_all_transforms() {
        let mut flame = rwf_core::Flame::default();
        for _ in 0..3 {
            flame.first_layer_mut().xforms.push(rwf_core::XForm::with_variation("linear3D", 1.0));
        }
        let mut doc = Document::new(flame);
        apply_color_action(&mut doc, ColorAction::Distribute);
        assert_eq!(doc.layer().xforms.iter().map(|x| x.color).collect::<Vec<_>>(), vec![0.0, 0.5, 1.0]);
        apply_color_action(&mut doc, ColorAction::RandomSpeed);
        assert!(doc.layer().xforms.iter().all(|x| (0.0..1.0).contains(&x.color_symmetry)));
        apply_color_action(&mut doc, ColorAction::Reset);
        assert!(doc.layer().xforms.iter().all(|x| x.color == 0.0 && x.color_symmetry == 0.0));
        apply_color_action(&mut doc, ColorAction::RandomCycle);
        assert!((-255.0..=255.0).contains(&doc.layer().gradient_shift));
    }
}
