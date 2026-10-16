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
use rwf_core::model::{ColorType, DrawMode};
use rwf_core::{VariationInstance, XForm};

use super::widgets;
use crate::canvas::xform_color;
use crate::doc::{Document, Selection};
use crate::{icons, theme};

#[derive(Default)]
pub struct TransformsState {
    pub var_search: String,
    pub expanded: std::collections::HashSet<usize>,
    /// Favourite variations (from the preferences; `favourites_changed` asks to save them).
    pub favourites: std::collections::BTreeSet<String>,
    pub favourites_changed: bool,
    /// The add-variation list shows only favourites.
    pub only_favourites: bool,
}

/// Random values in the parameters' ranges (integers rounded).
fn randomize_params(var: &mut VariationInstance, def: &variations::VarDef) {
    let mut rng = rwf_core::rng::Rng::from_entropy();
    for (i, p) in def.params.iter().enumerate() {
        let v = p.min + rng.random() * (p.max - p.min);
        if let Some(slot) = var.params.get_mut(i) {
            *slot = if p.integer { v.round() } else { v };
        }
    }
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
    /// `TinaController.addLinkedXForm`
    AddLinked,
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
    p.text(egui::pos2(text_x, rect.top() + 13.0), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(theme::fs(13.0)), theme::TEXT);
    let max_w = rect.width() - 48.0 - 60.0;
    let galley = p.layout(sub.to_string(), egui::FontId::proportional(theme::fs(11.0)), theme::TEXT_MUTED, f32::INFINITY);
    let sub_txt = if galley.size().x > max_w {
        let mut s: String = sub.chars().take((sub.len() as f32 * max_w / galley.size().x) as usize).collect();
        s.push('…');
        s
    } else {
        sub.to_string()
    };
    p.text(egui::pos2(text_x, rect.top() + 30.0), egui::Align2::LEFT_CENTER, sub_txt, egui::FontId::proportional(theme::fs(11.0)), theme::TEXT_MUTED);

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
        if ui.button("Add linked transform").on_hover_text("A new transform that always follows this one (via xaos); this one is hidden").clicked() {
            action = Some(RowAction::AddLinked);
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

    ui.add_space(4.0);
    if header(ui, "Background", "Add background transform") {
        let xf = rwf_core::render::background::random_bg_xform(&mut rwf_core::rng::Rng::from_entropy());
        doc.layer_mut().bg_xforms.push(xf);
        doc.selection = Selection::Background(doc.layer().bg_xforms.len() - 1);
    }
    for i in 0..doc.layer().bg_xforms.len() {
        let xf = &doc.layer().bg_xforms[i];
        let title = if xf.name.is_empty() { format!("Background {}", i + 1) } else { xf.name.clone() };
        let sub = format!("{}  ·  w {:.2}", summary(xf), xf.weight);
        let sw = Color32::from_rgb(0x50, 0x44, 0x30);
        if let Some(a) = row(ui, &title, &sub, sw, Color32::from_rgb(0xc8, 0xa8, 0x70), doc.selection == Selection::Background(i)) {
            act = Some((Selection::Background(i), a));
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
    egui::CollapsingHeader::new(RichText::new(title.to_uppercase()).size(theme::fs(10.5)).color(theme::TEXT_MUTED).strong())
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
                                widgets::reset_on_double_click(ui.add(egui::Slider::new(&mut layer.weight, 0.0..=4.0).clamping(egui::SliderClamping::Never)), &mut layer.weight, widgets::flame_defaults().layers[0].weight);
                                ui.end_row();
                                widgets::label(ui, "Density");
                                widgets::reset_on_double_click(ui.add(egui::Slider::new(&mut layer.density, 0.0..=1.0)), &mut layer.density, widgets::flame_defaults().layers[0].density);
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
    let Some((kind, i)) = sel.kind() else { return };
    let l = doc.layer_mut();
    let list = l.xforms_of_mut(kind);
    let wrap = |i| Selection::of(kind, i);
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
        RowAction::AddLinked if is_xform => {
            let to = list.len();
            let mut xf = XForm::with_variation("linear3D", 1.0);
            xf.weight = 0.5;
            xf.color_type = ColorType::None;
            xf.color_symmetry = 1.0;
            list.push(xf);
            let n = list.len();
            for x in list.iter_mut() {
                x.chaos.resize(n, 1.0);
            }
            // the new one takes over the xaos of `i`, which now only jumps to the new one
            let from_chaos = std::mem::replace(&mut list[i].chaos, (0..n).map(|j| if j == to { 1.0 } else { 0.0 }).collect());
            list[to].chaos = from_chaos;
            list[i].draw_mode = DrawMode::Hidden;
            list[i].opacity = 0.0;
            for (k, x) in list.iter_mut().enumerate() {
                if k != i {
                    x.chaos[to] = 0.0;
                }
            }
            new_sel = Some(wrap(to));
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
        Selection::Background(i) => format!("Variations · Background {}", i + 1),
        Selection::None => "Variations".into(),
    };
    theme::section_label(ui, &title);
    let Some(xf) = doc.selected_mut() else {
        ui.label(RichText::new("Select a transform").color(theme::TEXT_MUTED));
        return;
    };

    let mut remove: Option<usize> = None;
    let mut move_up: Option<usize> = None;
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
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if widgets::icon_button(ui, icons::X, "Remove variation").clicked() {
                        remove = Some(vi);
                    }
                    ui.add(egui::DragValue::new(&mut var.amount).speed(0.005).max_decimals(4));
                    // pre/post slot (JWildfire priority); prepost variations have both
                    if def.is_some_and(|d| !matches!(d.kind, variations::VarKind::PrePost(_))) {
                        let mut post = var.priority > 0;
                        if ui.toggle_value(&mut post, RichText::new("post").small()).on_hover_text("Run after the normal variations (post_ slot)").changed() {
                            var.priority = if post { 1 } else { 0 };
                        }
                        let mut pre = var.priority < 0;
                        if ui.toggle_value(&mut pre, RichText::new("pre").small()).on_hover_text("Run before the normal variations (pre_ slot)").changed() {
                            var.priority = if pre { -1 } else { 0 };
                        }
                    }
                    if vi > 0 && widgets::icon_button(ui, icons::CARET_UP, "Move up (variations run in this order)").clicked() {
                        move_up = Some(vi);
                    }
                    let fav = st.favourites.contains(&var.name);
                    let star = RichText::new(icons::STAR).color(if fav { theme::ACCENT } else { theme::TEXT_MUTED });
                    if ui.add(egui::Button::new(star).frame(false)).on_hover_text(if fav { "Favourite (click to remove)" } else { "Add to the favourites" }).clicked() {
                        if fav {
                            st.favourites.remove(&var.name);
                        } else {
                            st.favourites.insert(var.name.clone());
                        }
                        st.favourites_changed = true;
                    }
                    // The name takes the space left of the controls (shortened when narrow).
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let lbl = ui.add(egui::Label::new(name).truncate());
                        if def.is_none() {
                            lbl.on_hover_text("Not implemented in this version yet — kept in the file, ignored when rendering");
                        } else if def.is_some_and(|d| !d.supports_gpu()) {
                            lbl.on_hover_text("CPU only");
                        } else {
                            lbl.on_hover_text(var.name.as_str());
                        }
                    });
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
                            } else if variations::presets::id_param(def.name) == Some(p.name)
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
                    if !def.params.is_empty() {
                        ui.horizontal(|ui| {
                            if ui.small_button(format!("{} Random", icons::DICE)).on_hover_text("Random values in the parameters' ranges").clicked() {
                                randomize_params(var, def);
                            }
                            if ui.small_button(format!("{} Reset", icons::REFRESH)).on_hover_text("Parameters back to their defaults").clicked() {
                                var.params = def.params.iter().map(|p| p.default).collect();
                            }
                        });
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
    if let Some(i) = move_up {
        xf.variations.swap(i - 1, i);
        // keep the open parameter panels with their variations
        let (a, b) = (st.expanded.remove(&(i - 1)), st.expanded.remove(&i));
        if a {
            st.expanded.insert(i);
        }
        if b {
            st.expanded.insert(i - 1);
        }
    }

    ui.add_space(4.0);
    let btn = ui.add_sized([ui.available_width(), 28.0], egui::Button::new(format!("{}  Add variation", icons::PLUS)));
    egui::Popup::from_toggle_button_response(&btn).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(300.0);
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut st.var_search).hint_text("Search variations…").desired_width(250.0));
            r.request_focus();
            ui.toggle_value(&mut st.only_favourites, RichText::new(icons::STAR)).on_hover_text("Only favourites (mark them with the star of a variation)");
        });
        ui.add_space(4.0);
        let q = st.var_search.to_lowercase();
        let mut chosen = None;
        // favourites first
        let mut defs: Vec<&variations::VarDef> = variations::all().to_vec();
        defs.sort_by_key(|d| !st.favourites.contains(d.name));
        egui::ScrollArea::vertical().max_height(320.0).auto_shrink([false, true]).show(ui, |ui| {
            for def in defs {
                let fav = st.favourites.contains(def.name);
                if (!q.is_empty() && !def.name.to_lowercase().contains(&q)) || (st.only_favourites && !fav) {
                    continue;
                }
                ui.horizontal(|ui| {
                    let label = if fav { RichText::new(format!("{} {}", icons::STAR, def.name)) } else { RichText::new(def.name) };
                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
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
        ResKind::Svg => {
            let len = var.resource_bytes(r.name).len();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("SVG, {} KB", len.div_ceil(1024))).color(theme::TEXT_MUTED));
                if ui.small_button("Load…").clicked()
                    && let Some(p) = crate::settings::dialog(crate::settings::Folder::Images).add_filter("SVG", &["svg"]).pick_file()
                    && let Ok(text) = std::fs::read_to_string(&p)
                {
                    var.set_resource(r.name, &text);
                }
            });
        }
        ResKind::FlameXml => {
            let len = var.resource_bytes(r.name).len();
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("flame, {} KB", len.div_ceil(1024))).color(theme::TEXT_MUTED));
                if ui.small_button("Load…").clicked()
                    && let Some(p) = crate::settings::dialog(crate::settings::Folder::Flames).add_filter("Flame", &["flame"]).pick_file()
                    && let Ok(text) = std::fs::read_to_string(&p)
                {
                    var.set_resource(r.name, &text);
                }
            });
        }
        ResKind::FlameFile => {
            let mut value = var.resource(r.name);
            ui.horizontal(|ui| {
                let mut changed = ui.add(egui::TextEdit::singleline(&mut value).desired_width(140.0).hint_text("Flame file")).changed();
                if ui.small_button("…").clicked()
                    && let Some(p) = crate::settings::dialog(crate::settings::Folder::Flames).add_filter("Flame", &["flame"]).pick_file()
                {
                    value = p.display().to_string();
                    changed = true;
                    // as in JWildfire, the file is also copied into the flame resource
                    if let Ok(text) = std::fs::read_to_string(&p) {
                        var.set_resource("flame", &text);
                    }
                }
                if changed {
                    var.set_resource(r.name, &value);
                }
            });
        }
        ResKind::Font => {
            let mut value = var.resource(r.name);
            let before = value.clone();
            egui::ComboBox::from_id_salt((r.name, var.name.as_str())).selected_text(value.clone()).width(180.0).show_ui(ui, |ui| {
                static FAMILIES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
                for f in FAMILIES.get_or_init(rwf_core::shapes::font_families) {
                    ui.selectable_value(&mut value, f.clone(), f);
                }
            });
            if value != before {
                var.set_resource(r.name, &value);
            }
        }
        ResKind::Formula => {
            let mut value = var.resource(r.name);
            ui.vertical(|ui| {
                let edit = egui::TextEdit::multiline(&mut value).desired_width(200.0).desired_rows(2).code_editor();
                if ui.add(edit).changed() {
                    var.set_resource(r.name, &value);
                    // an edited formula no longer matches its preset (`validatePresetId`)
                    if let Some(id) = variations::presets::id_param(&var.name) {
                        var.set_param(id, -1.0);
                    }
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
                    && let Some(p) = crate::settings::dialog(crate::settings::Folder::Images).add_filter(image_filter.0, image_filter.1).pick_file()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_params_stay_in_range() {
        let def = variations::find("julian").unwrap();
        let mut v = VariationInstance::new("julian", 1.0);
        for _ in 0..50 {
            randomize_params(&mut v, def);
            for (p, x) in def.params.iter().zip(&v.params) {
                assert!(*x >= p.min && *x <= p.max && (!p.integer || x.fract() == 0.0), "{} = {x}", p.name);
            }
        }
    }

    #[test]
    fn linked_transform_follows_its_source() {
        let mut flame = rwf_core::Flame::default();
        for _ in 0..3 {
            flame.first_layer_mut().xforms.push(XForm::with_variation("linear3D", 1.0));
        }
        flame.first_layer_mut().xforms[1].chaos = vec![0.5, 2.0, 1.0];
        let mut doc = Document::new(flame);
        apply_row_action(&mut doc, Selection::XForm(1), RowAction::AddLinked);
        let x = &doc.layer().xforms;
        assert_eq!(x.len(), 4);
        assert_eq!(doc.selection, Selection::XForm(3));
        // the source only jumps to the new one and is hidden
        assert_eq!(x[1].chaos, vec![0.0, 0.0, 0.0, 1.0]);
        assert_eq!(x[1].draw_mode, DrawMode::Hidden);
        // the new one continues like the source did, and nothing else jumps to it
        assert_eq!(x[3].chaos, vec![0.5, 2.0, 1.0, 0.0]);
        assert_eq!((x[0].chaos_to(3), x[2].chaos_to(3)), (0.0, 0.0));
        assert_eq!((x[3].weight, x[3].color_type), (0.5, ColorType::None));
    }
}
