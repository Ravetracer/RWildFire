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

use std::collections::{BTreeSet, HashSet};
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
    /// Favourite variations (star in the variation list).
    pub favourites: BTreeSet<String>,
    /// Variation profiles for the random generators, and the active one (empty: none).
    pub profiles: Vec<VariationProfile>,
    pub active_profile: String,
}

/// A set of variations the random generators may use (JWildfire's variation profiles, by
/// name): the listed ones, or all but the listed ones; optionally only favourites.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct VariationProfile {
    pub name: String,
    pub exclude: bool,
    pub only_favourites: bool,
    pub variations: BTreeSet<String>,
}

impl VariationProfile {
    fn to_line(&self) -> String {
        let names: Vec<&str> = self.variations.iter().map(String::as_str).collect();
        format!("{}|{}|{}|{}", self.name.replace('|', "/"), self.exclude as u8, self.only_favourites as u8, names.join(","))
    }

    fn from_line(v: &str) -> Option<Self> {
        let mut parts = v.splitn(4, '|');
        Some(Self {
            name: parts.next()?.to_string(),
            exclude: parts.next()? == "1",
            only_favourites: parts.next()? == "1",
            variations: parts.next()?.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect(),
        })
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            always_denoise: false,
            denoiser: Denoiser::Oidn,
            blend: Flame::default().post_denoiser_blend,
            quick_save_dir: String::new(),
            favourites: BTreeSet::new(),
            profiles: Vec::new(),
            active_profile: String::new(),
        }
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
                "favourite_variations" => p.favourites = v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect(),
                "variation_profile" => p.profiles.extend(VariationProfile::from_line(v)),
                "active_profile" => p.active_profile = v.to_string(),
                _ => {}
            }
        }
        p
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let dir = config_dir().ok_or("no config folder (HOME or APPDATA not set)")?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = dir.join(FILE);
        let mut text = format!(
            "always_denoise = {}\ndenoiser = {}\ndenoiser_blend = {}\nquick_save_dir = {}\n",
            self.always_denoise,
            self.denoiser.as_str(),
            self.blend,
            self.quick_save_dir
        );
        let favs: Vec<&str> = self.favourites.iter().map(String::as_str).collect();
        text += &format!("favourite_variations = {}\n", favs.join(","));
        for pr in &self.profiles {
            text += &format!("variation_profile = {}\n", pr.to_line());
        }
        text += &format!("active_profile = {}\n", self.active_profile);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(path)
    }

    /// The variations the random generators may use with the active profile (`None`: all).
    pub fn allowed_variations(&self) -> Option<HashSet<String>> {
        let p = self.profiles.iter().find(|p| p.name == self.active_profile)?;
        let mut set: HashSet<String> = if p.exclude {
            rwf_core::variations::all().iter().map(|d| d.name.to_string()).filter(|n| !p.variations.contains(n)).collect()
        } else {
            p.variations.iter().cloned().collect()
        };
        if p.only_favourites {
            set.retain(|n| self.favourites.contains(n));
        }
        Some(set)
    }

    /// Hands the active profile to the random generators.
    pub fn apply_profile(&self) {
        rwf_core::random::java::set_allowed_variations(self.allowed_variations());
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

/// The active variation profile and the profile editor.
fn profiles_ui(ui: &mut egui::Ui, prefs: &mut Prefs, search: &mut String) {
    ui.horizontal(|ui| {
        widgets::label(ui, "Variation profile");
        profile_combo(ui, "prefs-profile", &prefs.profiles.iter().map(|p| p.name.clone()).collect::<Vec<_>>(), &mut prefs.active_profile);
    });
    if let Some(set) = prefs.allowed_variations() {
        let msg = if set.is_empty() { "This profile allows no variation: the generators use all of them".to_string() } else { format!("{} variations allowed", set.len()) };
        ui.label(RichText::new(msg).small().color(if set.is_empty() { theme::WARN } else { theme::TEXT_MUTED }));
    }
    let mut delete = None;
    for (i, p) in prefs.profiles.iter_mut().enumerate() {
        egui::CollapsingHeader::new(if p.name.is_empty() { "(unnamed)".to_string() } else { p.name.clone() }).id_salt(("profile", i)).show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::label(ui, "Name");
                ui.text_edit_singleline(&mut p.name);
            });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut p.exclude, false, "Only these").on_hover_text("The generators use only the checked variations");
                ui.selectable_value(&mut p.exclude, true, "All but these").on_hover_text("The generators use every variation except the checked ones");
                ui.checkbox(&mut p.only_favourites, "Favourites only");
            });
            ui.add(egui::TextEdit::singleline(search).hint_text("Search variations…"));
            let q = search.to_lowercase();
            egui::ScrollArea::vertical().id_salt(("profile-list", i)).max_height(160.0).show(ui, |ui| {
                for d in rwf_core::variations::all() {
                    if !q.is_empty() && !d.name.contains(&q) {
                        continue;
                    }
                    let mut on = p.variations.contains(d.name);
                    if ui.checkbox(&mut on, d.name).changed() {
                        if on {
                            p.variations.insert(d.name.to_string());
                        } else {
                            p.variations.remove(d.name);
                        }
                    }
                }
            });
            ui.label(RichText::new(format!("{} checked", p.variations.len())).small().color(theme::TEXT_MUTED));
            if ui.small_button("Delete profile").clicked() {
                delete = Some(i);
            }
        });
    }
    if let Some(i) = delete {
        if prefs.profiles[i].name == prefs.active_profile {
            prefs.active_profile.clear();
        }
        prefs.profiles.remove(i);
    }
    if ui.button("New profile").on_hover_text("A set of variations for the random flame generators").clicked() {
        prefs.profiles.push(VariationProfile { name: format!("Profile {}", prefs.profiles.len() + 1), ..Default::default() });
    }
}

/// Choice of the active variation profile ("All variations" = none).
pub fn profile_combo(ui: &mut egui::Ui, id: &str, names: &[String], active: &mut String) {
    let shown = if active.is_empty() { "All variations".to_string() } else { active.clone() };
    egui::ComboBox::from_id_salt(id).selected_text(shown).show_ui(ui, |ui| {
        ui.selectable_value(active, String::new(), "All variations");
        for n in names {
            ui.selectable_value(active, n.clone(), n);
        }
    })
    .response
    .on_hover_text("Variations the random flame generators choose from (profiles are edited in Edit › Preferences)");
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
    profile_search: String,
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
            theme::section_label(ui, "Random flames");
            profiles_ui(ui, prefs, &mut self.profile_search);
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
    fn profiles_round_trip_and_limit() {
        let pr = VariationProfile { name: "Round".into(), exclude: false, only_favourites: true, variations: ["julia", "spherical", "bubble"].iter().map(|s| s.to_string()).collect() };
        assert_eq!(VariationProfile::from_line(&pr.to_line()), Some(pr.clone()));
        let mut p = Prefs { profiles: vec![pr], favourites: ["julia", "bubble", "linear"].iter().map(|s| s.to_string()).collect(), ..Prefs::default() };
        assert_eq!(p.allowed_variations(), None, "no active profile");
        p.active_profile = "Round".into();
        let set = p.allowed_variations().unwrap();
        assert_eq!(set.len(), 2, "favourites only: {set:?}");
        p.profiles[0].exclude = true;
        p.profiles[0].only_favourites = false;
        let set = p.allowed_variations().unwrap();
        assert!(!set.contains("julia") && set.contains("linear"));
    }

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
