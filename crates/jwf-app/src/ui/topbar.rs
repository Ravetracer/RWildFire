//! Top bar: file actions, history, random generator, explore toggle, export.

use egui::{RichText, Ui};
use jwf_core::random::Generator;

use super::widgets;
use crate::{icons, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    New,
    Open,
    Save,
    SaveAs,
    Export,
    Undo,
    Redo,
    Randomize,
    ToggleExplorer,
}

pub struct TopBarInfo<'a> {
    pub title: &'a str,
    pub can_undo: bool,
    pub can_redo: bool,
    pub explorer_open: bool,
}

pub fn show(ui: &mut Ui, info: &TopBarInfo, generator: &mut Generator) -> Option<Action> {
    let mut action = None;
    ui.horizontal_centered(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(icons::FIRE).color(theme::ACCENT).size(18.0));
        ui.label(RichText::new("JWildfire").strong().size(15.0));
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
            ui.separator();
            if item(ui, "Export image…", "Ctrl+E").clicked() {
                action = Some(Action::Export);
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
        egui::ComboBox::from_id_salt("generator").width(110.0).selected_text(generator.label()).show_ui(ui, |ui| {
            for g in Generator::ALL {
                ui.selectable_value(generator, *g, g.label());
            }
        });
        if ui.add(egui::Button::new(RichText::new(format!("{}  Random flame", icons::DICE)).strong()).fill(theme::ACCENT_DIM)).on_hover_text("New random flame (Space)").clicked() {
            action = Some(Action::Randomize);
        }
        let mut open = info.explorer_open;
        if ui.toggle_value(&mut open, format!("{}  Explore", icons::SPARKLE)).on_hover_text("Browse random flames and mutations").clicked() {
            action = Some(Action::ToggleExplorer);
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
