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
    /// Folder of "Quick save" (empty: `~/RWildFire`).
    pub quick_save_dir: String,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { always_denoise: false, denoiser: Denoiser::Oidn, blend: Flame::default().post_denoiser_blend, quick_save_dir: String::new() }
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
                "quick_save_dir" => p.quick_save_dir = v.to_string(),
                _ => {}
            }
        }
        p
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let dir = config_dir().ok_or("no config folder (HOME or APPDATA not set)")?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(FILE);
        let text = format!(
            "always_denoise = {}\ndenoiser = {}\ndenoiser_blend = {}\nquick_save_dir = {}\n",
            self.always_denoise,
            self.denoiser.as_str(),
            self.blend,
            self.quick_save_dir
        );
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    /// The quick save folder.
    pub fn quick_save_folder(&self) -> Option<PathBuf> {
        if !self.quick_save_dir.trim().is_empty() {
            return Some(PathBuf::from(self.quick_save_dir.trim()));
        }
        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|h| PathBuf::from(h).join("RWildFire"))
    }

    /// Saves `flame` as `qsave_<date>_<number>.flame` in the quick save folder
    /// (`QuickSaveFilenameGen`); returns the file.
    pub fn quick_save(&self, flame: &Flame) -> Result<PathBuf, String> {
        let dir = self.quick_save_folder().ok_or("no quick save folder (HOME not set)")?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let date = today();
        let path = (1..)
            .map(|n| dir.join(format!("qsave_{date}_{n:03}.flame")))
            .find(|p| !p.exists())
            .expect("a free name");
        rwf_core::io::write_flame_to_file(flame, &path).map_err(|e| format!("{}: {e}", path.display()))?;
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

/// Today's date (UTC) as `yyyy-mm-dd`.
fn today() -> String {
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 86_400) as i64;
    // civil date from days since 1970-01-01 (H. Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02}")
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
            theme::section_label(ui, "Quick save");
            ui.horizontal(|ui| {
                let shown = prefs.quick_save_folder().map(|p| p.display().to_string()).unwrap_or_default();
                if ui.button("Folder…").on_hover_text("Where File › Quick save puts its flames (qsave_<date>_<number>.flame)").clicked()
                    && let Some(p) = rfd::FileDialog::new().pick_folder()
                {
                    prefs.quick_save_dir = p.to_string_lossy().to_string();
                }
                ui.label(RichText::new(shown).small().color(theme::TEXT_MUTED));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_save_numbers_files() {
        let d = today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"), "{d}");
        let dir = &std::env::temp_dir().join(format!("rwf-qsave-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(dir);
        let p = Prefs { quick_save_dir: dir.to_string_lossy().to_string(), ..Prefs::default() };
        let a = p.quick_save(&Flame::default()).unwrap();
        let b = p.quick_save(&Flame::default()).unwrap();
        assert_eq!(a.file_name().unwrap().to_string_lossy(), format!("qsave_{d}_001.flame"));
        assert_eq!(b.file_name().unwrap().to_string_lossy(), format!("qsave_{d}_002.flame"));
        assert!(rwf_core::io::read_flames_from_file(&b).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }
}
