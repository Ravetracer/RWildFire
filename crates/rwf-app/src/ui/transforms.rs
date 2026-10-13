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

//! Left panel: transform list and the variations of the selected transform.

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Ui, vec2};
use rwf_core::variations;
use rwf_core::{VariationInstance, XForm};

use super::widgets;
use crate::canvas::xform_color;
use crate::doc::{Document, Selection};
use crate::{icons, theme};

#[derive(Default)]
pub struct TransformsState {
    pub var_search: String,
    pub expanded: std::collections::HashSet<usize>,
}

fn summary(xf: &XForm) -> String {
    if xf.variations.is_empty() {
        return "no variations".into();
    }
    xf.variations.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(" · ")
}

enum RowAction {
    Select,
    Duplicate,
    Delete,
    MoveUp,
    MoveDown,
}

fn row(ui: &mut Ui, title: &str, sub: &str, swatch: Color32, handle_color: Color32, selected: bool) -> Option<RowAction> {
    let mut action = None;
    let h = 44.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click());
    let p = ui.painter();
    let fill = if selected {
        theme::SURFACE_HOVER
    } else if resp.hovered() {
        theme::SURFACE
    } else {
        Color32::TRANSPARENT
    };
    p.rect_filled(rect, CornerRadius::same(6), fill);
    if selected {
        let bar = egui::Rect::from_min_size(rect.min + vec2(0.0, 8.0), vec2(3.0, h - 16.0));
        p.rect_filled(bar, CornerRadius::same(2), theme::ACCENT);
    }
    // handle color dot + gradient swatch
    p.circle_filled(rect.left_center() + vec2(14.0, 0.0), 4.0, handle_color);
    let sw = egui::Rect::from_center_size(rect.left_center() + vec2(32.0, 0.0), vec2(14.0, 14.0));
    p.rect_filled(sw, CornerRadius::same(3), swatch);
    p.rect_stroke(sw, CornerRadius::same(3), Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Outside);
    let text_x = rect.left() + 48.0;
    p.text(egui::pos2(text_x, rect.top() + 13.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(13.0), theme::TEXT);
    let max_w = rect.width() - 48.0 - 60.0;
    let galley = p.layout(sub.to_string(), egui::FontId::proportional(11.0), theme::TEXT_MUTED, f32::INFINITY);
    let sub_txt = if galley.size().x > max_w {
        let mut s: String = sub.chars().take((sub.len() as f32 * max_w / galley.size().x) as usize).collect();
        s.push('…');
        s
    } else {
        sub.to_string()
    };
    p.text(egui::pos2(text_x, rect.top() + 30.0), egui::Align2::LEFT_CENTER, sub_txt, egui::FontId::proportional(11.0), theme::TEXT_MUTED);

    if resp.hovered() || selected {
        // inline actions
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(egui::Rect::from_min_max(rect.right_top() + vec2(-58.0, 10.0), rect.right_bottom() + vec2(-4.0, -10.0)))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        child.spacing_mut().item_spacing.x = 0.0;
        if widgets::icon_button(&mut child, icons::X, "Delete").clicked() {
            action = Some(RowAction::Delete);
        }
        if widgets::icon_button(&mut child, icons::COPY, "Duplicate").clicked() {
            action = Some(RowAction::Duplicate);
        }
    }
    if action.is_none() && resp.clicked() {
        action = Some(RowAction::Select);
    }
    resp.context_menu(|ui| {
        if ui.button("Duplicate").clicked() {
            action = Some(RowAction::Duplicate);
        }
        if ui.button("Move up").clicked() {
            action = Some(RowAction::MoveUp);
        }
        if ui.button("Move down").clicked() {
            action = Some(RowAction::MoveDown);
        }
        ui.separator();
        if ui.button("Delete").clicked() {
            action = Some(RowAction::Delete);
        }
    });
    action
}

fn header(ui: &mut Ui, text: &str, add_tip: &str) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        theme::section_label(ui, text);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            clicked = widgets::icon_button(ui, icons::PLUS, add_tip).clicked();
        });
    });
    clicked
}

fn new_xform(doc: &Document) -> XForm {
    let mut xf = XForm::with_variation("linear3D", 1.0);
    xf.coefs.scale(0.5);
    xf.color = (doc.layer().xforms.len() as f64 * 0.37) % 1.0;
    xf
}

pub fn show(ui: &mut Ui, doc: &mut Document, st: &mut TransformsState) {
    layers_section(ui, doc);

    if header(ui, "Transforms", "Add transform") {
        let xf = new_xform(doc);
        doc.layer_mut().xforms.push(xf);
        doc.selection = Selection::XForm(doc.layer().xforms.len() - 1);
    }

    let mut act: Option<(Selection, RowAction)> = None;
    let n = doc.layer().xforms.len();
    for i in 0..n {
        let xf = &doc.layer().xforms[i];
        let sw = widgets::gradient_color(&doc.layer().gradient, xf.color);
        let title = if xf.name.is_empty() { format!("Transform {}", i + 1) } else { xf.name.clone() };
        let sub = format!("{}  ·  w {:.2}", summary(xf), xf.weight);
        if let Some(a) = row(ui, &title, &sub, sw, xform_color(i), doc.selection == Selection::XForm(i)) {
            act = Some((Selection::XForm(i), a));
        }
    }
    if n == 0 {
        ui.label(RichText::new("No transforms yet — add one with +").color(theme::TEXT_MUTED).italics());
    }

    ui.add_space(4.0);
    if header(ui, "Final", "Add final transform") {
        doc.layer_mut().final_xforms.push(XForm::with_variation("linear3D", 1.0));
        doc.selection = Selection::Final(doc.layer().final_xforms.len() - 1);
    }
    for i in 0..doc.layer().final_xforms.len() {
        let xf = &doc.layer().final_xforms[i];
        let title = if xf.name.is_empty() { format!("Final {}", i + 1) } else { xf.name.clone() };
        if let Some(a) = row(ui, &title, &summary(xf), Color32::from_gray(90), Color32::from_gray(200), doc.selection == Selection::Final(i)) {
            act = Some((Selection::Final(i), a));
        }
    }

    if let Some((sel, a)) = act {
        apply_row_action(doc, sel, a);
    }

    ui.add_space(8.0);
    ui.separator();
    variations_editor(ui, doc, st);
}

/// Layer list with add/duplicate/delete; collapsed into one line for single-layer flames.
fn layers_section(ui: &mut Ui, doc: &mut Document) {
    let n = doc.flame.layers.len();
    let title = if n > 1 { format!("Layers ({n})") } else { "Layers".to_string() };
    egui::CollapsingHeader::new(RichText::new(title.to_uppercase()).size(10.5).color(theme::TEXT_MUTED).strong())
        .id_salt("layers")
        .default_open(n > 1)
        .show(ui, |ui| {
            let mut action: Option<(usize, &str)> = None;
            for i in 0..doc.flame.layers.len() {
                let selected = doc.layer == i;
                egui::Frame::new()
                    .fill(if selected { theme::SURFACE_HOVER } else { theme::SURFACE })
                    .corner_radius(6)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let layer = &mut doc.flame.layers[i];
                            let eye = if layer.visible { icons::EYE } else { icons::EYE_SLASH };
                            if widgets::icon_button(ui, eye, "Show / hide").clicked() {
                                layer.visible = !layer.visible;
                            }
                            let name = if layer.name.is_empty() { format!("Layer {}", i + 1) } else { layer.name.clone() };
                            if ui.selectable_label(selected, name).clicked() {
                                action = Some((i, "select"));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.add_enabled_ui(n > 1, |ui| {
                                    if widgets::icon_button(ui, icons::X, "Delete layer").clicked() {
                                        action = Some((i, "delete"));
                                    }
                                });
                                if widgets::icon_button(ui, icons::COPY, "Duplicate layer").clicked() {
                                    action = Some((i, "dup"));
                                }
                            });
                        });
                        if selected {
                            let layer = &mut doc.flame.layers[i];
                            egui::Grid::new(("layer-props", i)).num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                                widgets::label(ui, "Name");
                                ui.text_edit_singleline(&mut layer.name);
                                ui.end_row();
                                widgets::label(ui, "Weight");
                                ui.add(egui::Slider::new(&mut layer.weight, 0.0..=4.0).clamping(egui::SliderClamping::Never));
                                ui.end_row();
                                widgets::label(ui, "Density");
                                ui.add(egui::Slider::new(&mut layer.density, 0.0..=1.0));
                                ui.end_row();
                            });
                        }
                    });
                ui.add_space(2.0);
            }
            if ui.add_sized([ui.available_width(), 24.0], egui::Button::new(format!("{}  Add layer", icons::PLUS))).clicked() {
                let mut l = rwf_core::Layer { gradient: doc.layer().gradient.clone(), ..Default::default() };
                let mut xf = XForm::with_variation("linear3D", 1.0);
                xf.coefs.scale(0.5);
                l.xforms.push(xf);
                doc.flame.layers.push(l);
                doc.layer = doc.flame.layers.len() - 1;
                doc.selection = Selection::XForm(0);
            }
            match action {
                Some((i, "select")) => {
                    doc.layer = i;
                    doc.selection = Selection::XForm(0);
                }
                Some((i, "dup")) => {
                    let mut l = doc.flame.layers[i].clone();
                    if !l.name.is_empty() {
                        l.name.push_str(" copy");
                    }
                    doc.flame.layers.insert(i + 1, l);
                    doc.layer = i + 1;
                }
                Some((i, "delete")) if doc.flame.layers.len() > 1 => {
                    doc.flame.layers.remove(i);
                    doc.layer = doc.layer.min(doc.flame.layers.len() - 1);
                }
                _ => {}
            }
            doc.fix_selection();
        });
    ui.add_space(4.0);
}

fn apply_row_action(doc: &mut Document, sel: Selection, a: RowAction) {
    let l = doc.layer_mut();
    let (list, i, wrap): (&mut Vec<XForm>, usize, fn(usize) -> Selection) = match sel {
        Selection::XForm(i) => (&mut l.xforms, i, Selection::XForm),
        Selection::Final(i) => (&mut l.final_xforms, i, Selection::Final),
        Selection::None => return,
    };
    let is_xform = matches!(sel, Selection::XForm(_));
    let mut new_sel = Some(sel);
    match a {
        RowAction::Select => {}
        RowAction::Duplicate => {
            let c = list[i].clone();
            list.insert(i + 1, c);
            new_sel = Some(wrap(i + 1));
        }
        RowAction::Delete => {
            list.remove(i);
            new_sel = if list.is_empty() { None } else { Some(wrap(i.min(list.len() - 1))) };
        }
        RowAction::MoveUp if i > 0 => {
            list.swap(i, i - 1);
            new_sel = Some(wrap(i - 1));
        }
        RowAction::MoveDown if i + 1 < list.len() => {
            list.swap(i, i + 1);
            new_sel = Some(wrap(i + 1));
        }
        _ => {}
    }
    // Keep xaos ("chaos") vectors consistent with the transform order.
    if is_xform {
        match a {
            RowAction::Duplicate => {
                for xf in l.xforms.iter_mut() {
                    if xf.chaos.len() > i {
                        let v = xf.chaos[i];
                        xf.chaos.insert(i + 1, v);
                    }
                }
            }
            RowAction::Delete => {
                for xf in l.xforms.iter_mut() {
                    if xf.chaos.len() > i {
                        xf.chaos.remove(i);
                    }
                }
            }
            _ => {}
        }
    }
    doc.selection = new_sel.unwrap_or(Selection::None);
    doc.fix_selection();
}

fn variations_editor(ui: &mut Ui, doc: &mut Document, st: &mut TransformsState) {
    let title = match doc.selection {
        Selection::XForm(i) => format!("Variations · Transform {}", i + 1),
        Selection::Final(i) => format!("Variations · Final {}", i + 1),
        Selection::None => "Variations".into(),
    };
    theme::section_label(ui, &title);
    let Some(xf) = doc.selected_mut() else {
        ui.label(RichText::new("Select a transform").color(theme::TEXT_MUTED));
        return;
    };

    let mut remove: Option<usize> = None;
    for (vi, var) in xf.variations.iter_mut().enumerate() {
        let def = variations::find(&var.name);
        let has_params = def.is_some_and(|d| !d.params.is_empty()) || !var.raw_params.is_empty();
        egui::Frame::new().fill(theme::SURFACE).corner_radius(6).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let open = st.expanded.contains(&vi);
                if has_params {
                    let arrow = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
                    if ui.add(egui::Button::new(arrow).frame(false)).clicked() {
                        if open {
                            st.expanded.remove(&vi);
                        } else {
                            st.expanded.insert(vi);
                        }
                    }
                } else {
                    ui.add_space(18.0);
                }
                let mut name = RichText::new(&var.name);
                if def.is_none() {
                    name = name.color(theme::WARN);
                }
                let lbl = ui.label(name);
                if def.is_none() {
                    lbl.on_hover_text("Not implemented in this version yet — kept in the file, ignored when rendering");
                } else if def.is_some_and(|d| !d.supports_gpu()) {
                    lbl.on_hover_text("CPU only");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icons::X, "Remove variation").clicked() {
                        remove = Some(vi);
                    }
                    ui.add(egui::DragValue::new(&mut var.amount).speed(0.005).max_decimals(4));
                });
            });
            if has_params && st.expanded.contains(&vi) {
                if let Some(def) = def {
                    if var.params.len() < def.params.len() {
                        var.params.resize(def.params.len(), 0.0);
                    }
                    let mut chosen_preset = None;
                    egui::Grid::new(("vparams", vi)).num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                        for (pi, p) in def.params.iter().enumerate() {
                            widgets::label(ui, p.name);
                            let v = &mut var.params[pi];
                            if def.name == "obj_mesh_primitive_wf" && p.name == "primitive" {
                                primitive_combo(ui, vi, v);
                            } else if p.name == "preset_id"
                                && let Some(presets) = variations::presets::presets(def.name)
                            {
                                chosen_preset = preset_combo(ui, vi, *v as i32, presets);
                            } else if p.integer {
                                let mut iv = v.round() as i64;
                                ui.add(egui::DragValue::new(&mut iv).speed(0.05).range(p.min as i64..=p.max as i64 * 4));
                                *v = iv as f64;
                            } else {
                                ui.add(egui::DragValue::new(v).speed((p.max - p.min).abs() / 400.0).max_decimals(5));
                            }
                            ui.end_row();
                        }
                        if let Some(rv) = variations::resources_of(&var.name) {
                            for r in rv.resources {
                                widgets::label(ui, r.name);
                                resource_picker(ui, var, r);
                                ui.end_row();
                            }
                        }
                    });
                    if let Some(id) = chosen_preset {
                        variations::presets::apply(var, id);
                    }
                }
                let res = variations::resources_of(&var.name).map_or(&[][..], |rv| rv.resources);
                for (k, v) in &var.raw_params {
                    if res.iter().any(|r| r.name == k) {
                        continue;
                    }
                    ui.label(RichText::new(format!("{k} = {v}")).small().color(theme::TEXT_MUTED));
                }
            }
        });
        ui.add_space(2.0);
    }
    if let Some(i) = remove {
        xf.variations.remove(i);
        st.expanded.clear();
    }

    ui.add_space(4.0);
    let btn = ui.add_sized([ui.available_width(), 28.0], egui::Button::new(format!("{}  Add variation", icons::PLUS)));
    egui::Popup::from_toggle_button_response(&btn).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(260.0);
        let r = ui.add(egui::TextEdit::singleline(&mut st.var_search).hint_text("Search variations…"));
        r.request_focus();
        ui.add_space(4.0);
        let q = st.var_search.to_lowercase();
        let mut chosen = None;
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for def in variations::all() {
                if !q.is_empty() && !def.name.to_lowercase().contains(&q) {
                    continue;
                }
                ui.horizontal(|ui| {
                    if ui.add(egui::Button::new(def.name).frame(false)).clicked() {
                        chosen = Some(def.name);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if def.is_3d {
                            ui.label(RichText::new("3D").small().color(theme::TEXT_MUTED));
                        }
                        match def.kind {
                            variations::VarKind::Pre => {
                                ui.label(RichText::new("pre").small().color(theme::TEXT_MUTED));
                            }
                            variations::VarKind::Post => {
                                ui.label(RichText::new("post").small().color(theme::TEXT_MUTED));
                            }
                            variations::VarKind::PrePost(_) => {
                                ui.label(RichText::new("prepost").small().color(theme::TEXT_MUTED));
                            }
                            _ => {}
                        }
                        if !def.supports_gpu() {
                            ui.label(RichText::new("CPU").small().color(theme::WARN));
                        }
                    });
                });
            }
        });
        if let Some(name) = chosen {
            let amount = if xf.variations.is_empty() { 1.0 } else { 0.5 };
            xf.variations.push(VariationInstance::new(name, amount));
            st.var_search.clear();
            ui.close();
        }
    });
}

/// Built-in mesh selection of `obj_mesh_primitive_wf`.
fn primitive_combo(ui: &mut Ui, vi: usize, v: &mut f64) {
    let prims = rwf_core::mesh::PRIMITIVES;
    let cur = v.round() as i64;
    let text = usize::try_from(cur).ok().and_then(|i| prims.get(i)).map_or_else(|| cur.to_string(), |n| n.to_string());
    egui::ComboBox::from_id_salt(("primitive", vi)).selected_text(text).show_ui(ui, |ui| {
        for (i, name) in prims.iter().enumerate() {
            if ui.selectable_label(cur == i as i64, *name).clicked() {
                *v = i as f64;
            }
        }
    });
}

/// A variation resource: a file name (typed or picked), an embedded image or a text field.
/// As in JWildfire, a file name and an embedded image of the same variation exclude each other.
fn resource_picker(ui: &mut Ui, var: &mut VariationInstance, r: &variations::ResDef) {
    use variations::ResKind;
    let siblings = variations::resources_of(&var.name).map_or(&[][..], |rv| rv.resources);
    let clear_kind = |var: &mut VariationInstance, kind: ResKind| {
        for s in siblings.iter().filter(|s| s.kind == kind) {
            if !var.resource_bytes(s.name).is_empty() {
                var.set_resource_bytes(s.name, &[]);
            }
        }
    };
    let image_filter = ("Image", &["png", "jpg", "jpeg"][..]);
    match r.kind {
        ResKind::Formula => {
            let mut value = var.resource(r.name);
            ui.vertical(|ui| {
                let edit = egui::TextEdit::multiline(&mut value).desired_width(200.0).desired_rows(2).code_editor();
                if ui.add(edit).changed() {
                    var.set_resource(r.name, &value);
                    // an edited formula no longer matches its preset (`validatePresetId`)
                    var.set_param("preset_id", -1.0);
                }
                if let Err(e) = variations::check_formula(&var.name, &value) {
                    ui.label(RichText::new(e).small().color(theme::WARN));
                }
            });
        }
        ResKind::ImageData => {
            let len = var.resource_bytes(r.name).len();
            ui.horizontal(|ui| {
                let txt = if len == 0 { "none".to_string() } else { format!("embedded, {} KB", len.div_ceil(1024)) };
                ui.label(RichText::new(txt).color(theme::TEXT_MUTED));
                if ui.small_button("Embed…").clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter(image_filter.0, image_filter.1).pick_file()
                    && let Ok(bytes) = std::fs::read(&p)
                {
                    var.set_resource_bytes(r.name, &bytes);
                    clear_kind(var, ResKind::ImageFile);
                }
                if len > 0 && ui.small_button("Remove").clicked() {
                    var.set_resource_bytes(r.name, &[]);
                }
            });
        }
        _ => {
            let file: Option<(&str, &[&str])> = match r.kind {
                ResKind::ImageFile => Some(image_filter),
                ResKind::ObjFile => Some(("Wavefront OBJ", &["obj"])),
                _ => None,
            };
            let mut value = var.resource(r.name);
            ui.horizontal(|ui| {
                let hint = file.map_or("Text".to_string(), |(kind, _)| format!("{kind} file"));
                let mut changed = ui.add(egui::TextEdit::singleline(&mut value).desired_width(140.0).hint_text(hint)).changed();
                if let Some((kind, exts)) = file
                    && ui.small_button("…").clicked()
                    && let Some(p) = rfd::FileDialog::new().add_filter(kind, exts).pick_file()
                {
                    value = p.display().to_string();
                    changed = true;
                }
                if changed {
                    var.set_resource(r.name, &value);
                    if r.kind == ResKind::ImageFile && !value.is_empty() {
                        clear_kind(var, ResKind::ImageData);
                    }
                }
            });
        }
    }
}

/// Preset selection of the formula variations; returns the chosen preset id.
fn preset_combo(ui: &mut Ui, vi: usize, current: i32, presets: &[variations::presets::Preset]) -> Option<i32> {
    let text = presets.iter().find(|p| p.id == current).map_or_else(|| "custom".to_string(), |p| format!("{} {}", p.id, p.name));
    let mut chosen = None;
    egui::ComboBox::from_id_salt(("preset", vi)).selected_text(text).width(180.0).show_ui(ui, |ui| {
        for p in presets {
            if ui.selectable_label(p.id == current, format!("{} {}", p.id, p.name)).clicked() {
                chosen = Some(p.id);
            }
        }
    });
    chosen
}
