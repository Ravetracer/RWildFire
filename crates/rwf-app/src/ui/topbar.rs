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

//! Top bar: file actions, history, random generator, explore toggle, export.

use egui::{RichText, Ui};
use rwf_core::random::{Extras, Generator};

use super::widgets;
use crate::{icons, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    QuickSave,
    Export,
    Undo,
    Redo,
    CopyFlame,
    PasteFlame,
    Preferences,
    Randomize,
    ToggleExplorer,
    ToggleTimeline,
    ToggleRender,
    ToggleBrowser,
    Help,
    VariationList,
    About,
    DenoiseImage,
    Scripts,
    UserManual,
    Quit,
}

pub struct TopBarInfo<'a> {
    pub title: &'a str,
    pub can_undo: bool,
    pub can_redo: bool,
    pub explorer_open: bool,
    pub timeline_open: bool,
    pub render_open: bool,
    pub browser_open: bool,
    /// A batch render is running.
    pub render_busy: bool,
    /// A random flame is being generated.
    pub randomizing: bool,
}

pub fn show(ui: &mut Ui, info: &TopBarInfo, generator: &mut Generator, extras: &mut Extras, profiles: &[String], active_profile: &mut String) -> Option<Action> {
    let mut action = None;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(icons::FIRE).color(theme::ACCENT).size(theme::fs(18.0)));
        ui.label(RichText::new("RWildFire").strong().size(theme::fs(15.0)));
        ui.add_space(10.0);

        ui.menu_button("File", |ui| {
            ui.set_min_width(200.0);
            let item = |ui: &mut Ui, text: &str, sc: &str| ui.add(egui::Button::new(text).shortcut_text(sc));
            if item(ui, "New", "Ctrl+N").clicked() {
                action = Some(Action::New);
            }
            if item(ui, "Open…", "Ctrl+O").clicked() {
                action = Some(Action::Open);
            }
            ui.separator();
            if item(ui, "Save", "Ctrl+S").clicked() {
                action = Some(Action::Save);
            }
            if item(ui, "Save as…", "Ctrl+Shift+S").clicked() {
                action = Some(Action::SaveAs);
            }
            if item(ui, "Quick save", "Ctrl+Alt+S").on_hover_text("Saves a copy as qsave_<date>_<number>.flame in the quick save folder (Preferences), without asking").clicked() {
                action = Some(Action::QuickSave);
            }
            ui.separator();
            if item(ui, "Export image…", "Ctrl+E").clicked() {
                action = Some(Action::Export);
            }
            if ui.button("Denoise image…").on_hover_text("Runs the AI denoiser on any image file").clicked() {
                action = Some(Action::DenoiseImage);
            }
            if ui.button("Scripts…").on_hover_text("Lua scripts that build or change flames: examples, your own scripts and an editor").clicked() {
                action = Some(Action::Scripts);
            }
            ui.separator();
            if item(ui, "Quit", "Ctrl+Q").on_hover_text("Closes RWildFire, like the window's close button").clicked() {
                action = Some(Action::Quit);
            }
        });
        ui.menu_button("Edit", |ui| {
            ui.set_min_width(200.0);
            if ui.add_enabled(info.can_undo, egui::Button::new("Undo").shortcut_text("Ctrl+Z")).clicked() {
                action = Some(Action::Undo);
            }
            if ui.add_enabled(info.can_redo, egui::Button::new("Redo").shortcut_text("Ctrl+Shift+Z")).clicked() {
                action = Some(Action::Redo);
            }
            ui.separator();
            if ui.add(egui::Button::new("Copy flame").shortcut_text("Ctrl+C")).on_hover_text("The flame as XML, e.g. to paste into a forum post or another program").clicked() {
                action = Some(Action::CopyFlame);
            }
            if ui.add(egui::Button::new("Paste flame").shortcut_text("Ctrl+V")).on_hover_text("Loads a flame (RWildFire, JWildfire or Apophysis XML) from the clipboard").clicked() {
                action = Some(Action::PasteFlame);
            }
            ui.separator();
            if ui.button("Preferences…").on_hover_text("Settings that stay between sessions, e.g. always denoise").clicked() {
                action = Some(Action::Preferences);
            }
        });

        ui.menu_button("Help", |ui| {
            ui.set_min_width(200.0);
            if ui.add(egui::Button::new("Help…").shortcut_text("F1")).on_hover_text("Getting started, shortcuts, FAQ and coloring types").clicked() {
                action = Some(Action::Help);
            }
            if ui.button("Variation list…").on_hover_text("All variations with their parameters, searchable").clicked() {
                action = Some(Action::VariationList);
            }
            if ui.button("User manual (PDF)").on_hover_text("Opens the user manual that comes with RWildFire in your PDF viewer").clicked() {
                action = Some(Action::UserManual);
            }
            ui.separator();
            if ui.button("About RWildFire").clicked() {
                action = Some(Action::About);
            }
        });

        ui.add_space(6.0);
        ui.add_enabled_ui(info.can_undo, |ui| {
            if widgets::icon_button(ui, icons::UNDO, "Undo (Ctrl+Z)").clicked() {
                action = Some(Action::Undo);
            }
        });
        ui.add_enabled_ui(info.can_redo, |ui| {
            if widgets::icon_button(ui, icons::REDO, "Redo (Ctrl+Shift+Z)").clicked() {
                action = Some(Action::Redo);
            }
        });

        ui.separator();
        egui::ComboBox::from_id_salt("generator").width(150.0).height(560.0).selected_text(generator.label()).show_ui(ui, |ui| {
            for g in Generator::ALL {
                ui.selectable_value(generator, *g, g.label());
            }
        });
        let label = |ui: &mut Ui, t: &str| ui.label(RichText::new(t).color(theme::TEXT_MUTED));
        ui.menu_button(icons::SLIDERS, |ui| {
            egui::Grid::new("random-extras").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                label(ui, "Symmetry");
                widgets::symmetry_combo(ui, "top-symmetry", &mut extras.symmetry);
                ui.end_row();
                label(ui, "Weighting field");
                widgets::weighting_combo(ui, "top-weighting", &mut extras.weighting);
                ui.end_row();
                label(ui, "Gradient");
                widgets::gradient_gen_combo(ui, "top-gradient", &mut extras.gradient);
                ui.end_row();
                label(ui, "Variations");
                crate::prefs::profile_combo(ui, "top-profile", profiles, active_profile);
                ui.end_row();
            });
        })
        .response
        .on_hover_text("Random symmetry, weighting fields, gradients and variations");
        if info.randomizing {
            ui.add_enabled(false, egui::Button::new(RichText::new(format!("{}  Generating…", icons::DICE)).strong()).fill(theme::ACCENT_DIM));
            ui.spinner();
        } else if ui.add(egui::Button::new(RichText::new(format!("{}  Random flame", icons::DICE)).strong()).fill(theme::ACCENT_DIM)).on_hover_text("New random flame (Space)").clicked() {
            action = Some(Action::Randomize);
        }
        let mut br = info.browser_open;
        if ui.toggle_value(&mut br, format!("{}  Browse", icons::FOLDER_OPEN)).on_hover_text("Flame browser: the flames of a folder as thumbnails, by date").clicked() {
            action = Some(Action::ToggleBrowser);
        }
        let mut open = info.explorer_open;
        if ui.toggle_value(&mut open, format!("{}  Explore", icons::SPARKLE)).on_hover_text("Browse random flames and mutations").clicked() {
            action = Some(Action::ToggleExplorer);
        }

        let mut tl = info.timeline_open;
        if ui.toggle_value(&mut tl, format!("{}  Animate", icons::FILM_STRIP)).on_hover_text("Timeline, keyframes and playback").clicked() {
            action = Some(Action::ToggleTimeline);
        }

        let mut ir = info.render_open;
        let label = if info.render_busy { format!("{}  Render •", icons::RENDER) } else { format!("{}  Render", icons::RENDER) };
        if ui.toggle_value(&mut ir, label).on_hover_text("Interactive renderer: long renders with pause, resume and saved render state").clicked() {
            action = Some(Action::ToggleRender);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(format!("{}  Export", icons::EXPORT)).on_hover_text("Render and save an image (Ctrl+E)").clicked() {
                action = Some(Action::Export);
            }
            ui.add_space(8.0);
            ui.label(RichText::new(info.title).color(theme::TEXT_MUTED));
        });
    });
    action
}
