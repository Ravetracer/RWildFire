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

//! User preferences (`preferences.txt` in the config folder, `key = value` lines) and the
//! Preferences window.

use std::path::PathBuf;

use egui::RichText;
use rwf_core::Flame;
use rwf_core::render::denoise::{self, Denoiser};

use crate::theme;
use crate::ui::widgets;

/// The config folder (`$XDG_CONFIG_HOME/rwildfire`, `~/.config/rwildfire` or
/// `%APPDATA%\rwildfire`).
pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))?;
    let dir = base.join("rwildfire");
    // Settings of the earlier "jwildfire-rs" builds move over once.
    let legacy = base.join("jwildfire-rs");
    if !dir.exists() && legacy.is_dir() {
        let _ = std::fs::rename(&legacy, &dir);
    }
    Some(dir)
}

const FILE: &str = "preferences.txt";

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    /// Give every flame that comes into the editor without a denoiser `denoiser` and `blend`.
    pub always_denoise: bool,
    pub denoiser: Denoiser,
    pub blend: f64,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { always_denoise: false, denoiser: Denoiser::Oidn, blend: Flame::default().post_denoiser_blend }
    }
}

impl Prefs {
    /// Reads the preferences; missing or unreadable values keep their defaults.
    pub fn load() -> Self {
        let mut p = Self::default();
        let Some(src) = config_dir().and_then(|d| std::fs::read_to_string(d.join(FILE)).ok()) else { return p };
        for line in src.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "always_denoise" => p.always_denoise = v == "true",
                "denoiser" => p.denoiser = Denoiser::parse(v).filter(|d| *d != Denoiser::None).unwrap_or(p.denoiser),
                "denoiser_blend" => p.blend = v.parse::<f64>().map(|b| b.clamp(0.0, 1.0)).unwrap_or(p.blend),
                _ => {}
            }
        }
        p
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let dir = config_dir().ok_or("no config folder (HOME or APPDATA not set)")?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(FILE);
        let text = format!("always_denoise = {}\ndenoiser = {}\ndenoiser_blend = {}\n", self.always_denoise, self.denoiser.as_str(), self.blend);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    /// Gives `flame` the preferred denoiser if "always denoise" is on and it has none.
    pub fn apply(&self, flame: &mut Flame) {
        if self.always_denoise && flame.ai_post_denoiser == Denoiser::None {
            flame.ai_post_denoiser = self.denoiser;
            flame.post_denoiser_blend = self.blend;
        }
    }

    /// `flame` with [`Prefs::apply`] done.
    pub fn adopt(&self, mut flame: Flame) -> Flame {
        self.apply(&mut flame);
        flame
    }
}

/// The Preferences window; saves on every change.
#[derive(Default)]
pub struct PrefsWindow {
    pub open: bool,
    /// Changed and not saved yet.
    unsaved: bool,
    status: Option<Result<PathBuf, String>>,
}

impl PrefsWindow {
    /// Shows the window; returns true when the preferences changed.
    pub fn show(&mut self, ctx: &egui::Context, prefs: &mut Prefs) -> bool {
        if !self.open {
            return false;
        }
        let before = prefs.clone();
        let mut open = self.open;
        egui::Window::new("Preferences").open(&mut open).default_width(420.0).resizable(false).collapsible(false).show(ctx, |ui| {
            theme::section_label(ui, "Denoiser");
            ui.checkbox(&mut prefs.always_denoise, "Always denoise")
                .on_hover_text("Every flame that comes into the editor without a denoiser (new, random, Explore, MutaGen, opened files) gets the settings below, so saved images and renders are denoised. The canvas shows it only with \"Denoised\". Flames that already have a denoiser keep theirs.");
            ui.add_enabled_ui(prefs.always_denoise, |ui| {
                widgets::grid(ui, "prefs-denoise", |ui| {
                    widgets::label(ui, "AI denoiser");
                    egui::ComboBox::from_id_salt("prefs-denoiser").selected_text(prefs.denoiser.label()).show_ui(ui, |ui| {
                        for d in Denoiser::ALL.iter().filter(|d| **d != Denoiser::None) {
                            ui.selectable_value(&mut prefs.denoiser, *d, d.label());
                        }
                    });
                    ui.end_row();
                    widgets::slider(ui, "Blend", &mut prefs.blend, 0.0..=1.0).on_hover_text("Mixes the original image back in (0 = fully denoised)");
                });
                match denoise::backend() {
                    Ok(b) => ui.label(RichText::new(format!("Denoiser found: {b}")).small().color(theme::TEXT_MUTED)),
                    Err(e) => ui.label(RichText::new(format!("{e}; flames get the setting, but images stay un-denoised")).small().color(theme::WARN)),
                };
            });
            ui.add_space(6.0);
            match &self.status {
                Some(Ok(p)) => ui.label(RichText::new(format!("Saved to {}", p.display())).small().color(theme::TEXT_MUTED)),
                Some(Err(e)) => ui.label(RichText::new(format!("Could not save preferences: {e}")).small().color(theme::WARN)),
                None => ui.label(RichText::new("Changes are saved right away.").small().color(theme::TEXT_MUTED)),
            };
        });
        self.open = open;
        let changed = *prefs != before;
        self.unsaved |= changed;
        // Save once the blend slider is let go, not on every drag step.
        if self.unsaved && !ctx.input(|i| i.pointer.any_down()) {
            self.unsaved = false;
            self.status = Some(prefs.save());
        }
        changed
    }
}
