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

use crate::{icons, theme};
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
    /// Double-click on the canvas, and the grid spacing of the canvas grid.
    pub double_click: crate::canvas::DoubleClick,
    pub grid_size: f64,
    /// Folder of the flame browser (empty: none chosen yet).
    pub browser_dir: String,
    /// Flames per random batch (Explore strip) and where a new batch goes.
    pub batch_size: usize,
    pub batch_refresh: crate::explorer::RefreshMode,
    /// Resolution and quality profiles.
    pub render_profiles: crate::profiles::Profiles,
    /// Default folders of the file dialogs.
    pub folders: crate::settings::Folders,
    pub new_flame: crate::settings::NewFlameDefaults,
    /// Quality the editor preview renders up to, and its time limit in seconds (0: none).
    pub preview_quality: f64,
    pub preview_seconds: f64,
    /// CPU cores the renderers leave free.
    pub free_cores: usize,
    pub controls: crate::settings::ControlStyle,
    /// UI font size in percent of the built-in sizes.
    pub font_percent: u32,
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
            double_click: crate::canvas::DoubleClick::Nothing,
            grid_size: 0.5,
            browser_dir: String::new(),
            batch_size: 24,
            batch_refresh: crate::explorer::RefreshMode::Clear,
            render_profiles: Default::default(),
            folders: Default::default(),
            new_flame: Default::default(),
            preview_quality: 400.0,
            preview_seconds: 0.0,
            free_cores: 0,
            controls: Default::default(),
            font_percent: 100,
        }
    }
}

impl Prefs {
    /// Reads the preferences; missing or unreadable values keep their defaults.
    pub fn load() -> Self {
        let mut p = Self::default();
        // Stored profile lists replace the default ones.
        let (mut own_res, mut own_qual) = (false, false);
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
                "double_click" => p.double_click = crate::canvas::DoubleClick::parse(v),
                "resolution_profile" => {
                    if !std::mem::replace(&mut own_res, true) {
                        p.render_profiles.resolutions.clear();
                    }
                    p.render_profiles.resolutions.extend(crate::profiles::ResolutionProfile::from_line(v));
                }
                "quality_profile" => {
                    if !std::mem::replace(&mut own_qual, true) {
                        p.render_profiles.qualities.clear();
                    }
                    p.render_profiles.qualities.extend(crate::profiles::QualityProfile::from_line(v));
                }
                k if k.starts_with("folder_") => {
                    if let Some(f) = crate::settings::Folder::ALL.into_iter().find(|f| f.key() == k) {
                        p.folders.set(f, v.to_string());
                    }
                }
                "new_title" => p.new_flame.title = v.to_string(),
                "new_oversampling" => p.new_flame.oversampling = v.parse::<u32>().map(|n| n.clamp(1, 6)).unwrap_or(p.new_flame.oversampling),
                "new_filter_kernel" => p.new_flame.filter_kernel = rwf_core::render::filter::FilterKernel::parse(v).unwrap_or(p.new_flame.filter_kernel),
                "new_filter_radius" => p.new_flame.filter_radius = num(v, p.new_flame.filter_radius),
                "new_de_radius" => p.new_flame.de_radius = num(v, p.new_flame.de_radius),
                "new_de_curve" => p.new_flame.de_curve = num(v, p.new_flame.de_curve),
                "new_white_level" => p.new_flame.white_level = num(v, p.new_flame.white_level),
                "new_fg_opacity" => p.new_flame.fg_opacity = num(v, p.new_flame.fg_opacity),
                "new_fps" => p.new_flame.fps = num(v, p.new_flame.fps),
                "new_frame_count" => p.new_flame.frame_count = v.parse::<u32>().ok().filter(|n| *n > 0).unwrap_or(p.new_flame.frame_count),
                "preview_quality" => p.preview_quality = num(v, p.preview_quality).max(10.0),
                "preview_seconds" => p.preview_seconds = num(v, p.preview_seconds).max(0.0),
                "free_cores" => p.free_cores = v.parse().unwrap_or(p.free_cores),
                "control_shape" => p.controls.shape = crate::settings::ControlShape::parse(v),
                "control_color" => p.controls.color = v == "true",
                "control_numbers" => p.controls.numbers = v == "true",
                "control_shadows" => p.controls.shadows = v == "true",
                "associate_profiles" => p.render_profiles.associate = v == "true",
                "batch_size" => p.batch_size = v.parse::<usize>().map(|n| n.clamp(1, 64)).unwrap_or(p.batch_size),
                "batch_refresh" => p.batch_refresh = crate::explorer::RefreshMode::parse(v),
                "browser_dir" => p.browser_dir = v.to_string(),
                "grid_size" => p.grid_size = v.parse::<f64>().ok().filter(|g| *g > 0.0).unwrap_or(p.grid_size),
                "font_size" => {
                    p.font_percent = v.parse::<u32>().map(|n| n.clamp(*theme::FONT_PERCENT_RANGE.start(), *theme::FONT_PERCENT_RANGE.end())).unwrap_or(p.font_percent)
                }
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
        text += &format!("double_click = {}\ngrid_size = {}\nbrowser_dir = {}\n", self.double_click.as_str(), self.grid_size, self.browser_dir);
        for r in &self.render_profiles.resolutions {
            text += &format!("resolution_profile = {}\n", r.to_line());
        }
        for q in &self.render_profiles.qualities {
            text += &format!("quality_profile = {}\n", q.to_line());
        }
        text += &format!("associate_profiles = {}\n", self.render_profiles.associate);
        for f in crate::settings::Folder::ALL {
            text += &format!("{} = {}\n", f.key(), self.folders.get(f));
        }
        let n = &self.new_flame;
        text += &format!(
            "new_title = {}\nnew_oversampling = {}\nnew_filter_kernel = {}\nnew_filter_radius = {}\nnew_de_radius = {}\nnew_de_curve = {}\nnew_white_level = {}\nnew_fg_opacity = {}\nnew_fps = {}\nnew_frame_count = {}\n",
            n.title, n.oversampling, n.filter_kernel.as_str(), n.filter_radius, n.de_radius, n.de_curve, n.white_level, n.fg_opacity, n.fps, n.frame_count
        );
        text += &format!("preview_quality = {}\npreview_seconds = {}\nfree_cores = {}\n", self.preview_quality, self.preview_seconds, self.free_cores);
        let c = &self.controls;
        text += &format!("control_shape = {}\ncontrol_color = {}\ncontrol_numbers = {}\ncontrol_shadows = {}\n", c.shape.as_str(), c.color, c.numbers, c.shadows);
        text += &format!("batch_size = {}\nbatch_refresh = {}\n", self.batch_size, self.batch_refresh.as_str());
        text += &format!("font_size = {}\n", self.font_percent);
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
    /// Applies the settings held globally: the variation profile of the random generators,
    /// default folders and render threads.
    pub fn apply_profile(&self) {
        rwf_core::random::java::set_allowed_variations(self.allowed_variations());
        crate::settings::set_globals(&self.folders, self.free_cores);
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
/// A finite number, or `default`.
fn num(v: &str, default: f64) -> f64 {
    v.parse::<f64>().ok().filter(|x| x.is_finite()).unwrap_or(default)
}

fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    date_time(secs)[..10].to_string()
}

/// `YYYY-MM-DD HH:MM` (UTC) of seconds since 1970-01-01.
pub fn date_time(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let mins = secs % 86_400 / 60;
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
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", mins / 60, mins % 60)
}

/// Editable resolution and quality profiles.
fn render_profiles_ui(ui: &mut egui::Ui, p: &mut crate::profiles::Profiles) {
    use crate::profiles::{QualityProfile, ResolutionProfile};
    ui.checkbox(&mut p.associate, "Associate profiles with flames")
        .on_hover_text("The export dialog starts with the profiles last chosen for a flame (stored in the flame file)");
    ui.label(RichText::new("Resolutions — the default one sizes new flames").small().color(theme::TEXT_MUTED));
    let (mut remove, mut default) = (None, None);
    egui::Grid::new("prefs-res-profiles").num_columns(4).spacing([8.0, 4.0]).show(ui, |ui| {
        for (i, r) in p.resolutions.iter_mut().enumerate() {
            ui.add(egui::DragValue::new(&mut r.width).range(16..=16384));
            ui.add(egui::DragValue::new(&mut r.height).range(16..=16384));
            if ui.radio(r.default, "Default").clicked() {
                default = Some(i);
            }
            if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(k) = default {
        for (j, r) in p.resolutions.iter_mut().enumerate() {
            r.default = j == k;
        }
    }
    if let Some(i) = remove {
        p.resolutions.remove(i);
    }
    if ui.small_button(format!("{} Resolution", icons::PLUS)).clicked() {
        p.resolutions.push(ResolutionProfile { width: 1920, height: 1080, default: false });
    }
    ui.add_space(4.0);
    ui.label(RichText::new("Qualities — the default one is the quality of new flames; HDR also picks HDR output in the export dialog").small().color(theme::TEXT_MUTED));
    let (mut remove, mut default) = (None, None);
    egui::Grid::new("prefs-qual-profiles").num_columns(6).spacing([8.0, 4.0]).show(ui, |ui| {
        for (i, q) in p.qualities.iter_mut().enumerate() {
            ui.add_sized([130.0, 20.0], egui::TextEdit::singleline(&mut q.caption));
            ui.add(egui::DragValue::new(&mut q.quality).range(1.0..=100_000.0).max_decimals(0));
            ui.checkbox(&mut q.hdr, "HDR");
            ui.checkbox(&mut q.zbuffer, "Z").on_hover_text("Z-buffer");
            if ui.radio(q.default, "Default").on_hover_text("Quality of new flames").clicked() {
                default = Some(i);
            }
            if ui.small_button(icons::X).on_hover_text("Remove").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(k) = default {
        for (j, q) in p.qualities.iter_mut().enumerate() {
            q.default = j == k;
        }
    }
    if let Some(i) = remove {
        p.qualities.remove(i);
    }
    if ui.small_button(format!("{} Quality", icons::PLUS)).clicked() {
        p.qualities.push(QualityProfile { caption: "New quality".to_string(), quality: 1000.0, hdr: false, zbuffer: false, default: false });
    }
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
            let h = ctx.content_rect().height() - 120.0;
            egui::ScrollArea::vertical().max_height(h).min_scrolled_height(h).show(ui, |ui| {
                theme::section_label(ui, "Appearance");
                widgets::grid(ui, "prefs-appearance", |ui| {
                    widgets::label(ui, "Font size");
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut prefs.font_percent).range(theme::FONT_PERCENT_RANGE).suffix(" %"))
                            .on_hover_text("Size of all texts in the user interface, in percent of the built-in size (100 %). Drag or type a value.");
                        if ui.add_enabled(prefs.font_percent != 100, egui::Button::new("Reset").small()).on_hover_text("Back to 100 %").clicked() {
                            prefs.font_percent = 100;
                        }
                    });
                    ui.end_row();
                });
                ui.label(
                    RichText::new("Other font sizes than 100 % can lead to UI glitches: texts may be cut off, overlap or not fit their controls.")
                        .small()
                        .color(theme::WARN),
                );
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
                theme::section_label(ui, "Editor");
                widgets::grid(ui, "prefs-editor", |ui| {
                    widgets::label(ui, "Double-click");
                    egui::ComboBox::from_id_salt("prefs-dblclick").selected_text(prefs.double_click.label()).show_ui(ui, |ui| {
                        for d in crate::canvas::DoubleClick::ALL {
                            ui.selectable_value(&mut prefs.double_click, d, d.label());
                        }
                    })
                    .response
                    .on_hover_text("What a double-click on the canvas does");
                    ui.end_row();
                    widgets::label(ui, "Preview quality");
                    ui.add(egui::DragValue::new(&mut prefs.preview_quality).range(10.0..=5000.0).max_decimals(0))
                        .on_hover_text("The editor preview renders up to this quality (Render tab › Target quality changes it for the session)");
                    ui.end_row();
                    widgets::label(ui, "Preview time limit");
                    ui.add(egui::DragValue::new(&mut prefs.preview_seconds).range(0.0..=3600.0).suffix(" s").max_decimals(0))
                        .on_hover_text("The preview stops refining after this many seconds; 0 = no limit");
                    ui.end_row();
                    widgets::label(ui, "Free CPU cores");
                    ui.add(egui::DragValue::new(&mut prefs.free_cores).range(0..=256)).on_hover_text(format!(
                        "CPU cores the renderers leave free for other programs ({} of {} cores used)",
                        crate::settings::cpu_threads_for(prefs.free_cores),
                        std::thread::available_parallelism().map_or(4, |n| n.get())
                    ));
                    ui.end_row();
                    widgets::label(ui, "Grid spacing");
                    ui.add(egui::DragValue::new(&mut prefs.grid_size).speed(0.01).range(0.01..=100.0)).on_hover_text("Default spacing of the canvas grid (View › Grid)");
                    ui.end_row();
                });
                crate::settings::controls_ui(ui, &mut prefs.controls);
                theme::section_label(ui, "New flames");
                crate::settings::new_flame_ui(ui, &mut prefs.new_flame);
                theme::section_label(ui, "Folders");
                crate::settings::folders_ui(ui, &mut prefs.folders);
                theme::section_label(ui, "Random flames");
                widgets::grid(ui, "prefs-batch", |ui| {
                    widgets::label(ui, "Batch size");
                    ui.add(egui::DragValue::new(&mut prefs.batch_size).range(1..=64)).on_hover_text("Flames per \"New batch\" in Explore › Random");
                    ui.end_row();
                    widgets::label(ui, "New batches");
                    egui::ComboBox::from_id_salt("prefs-batch-refresh").selected_text(prefs.batch_refresh.label()).show_ui(ui, |ui| {
                        for r in crate::explorer::RefreshMode::ALL {
                            ui.selectable_value(&mut prefs.batch_refresh, r, r.label());
                        }
                    })
                    .response
                    .on_hover_text("Where a new random batch goes in the Explore strip (JWildfire's random batch refresh type)");
                    ui.end_row();
                });
                profiles_ui(ui, prefs, &mut self.profile_search);
                theme::section_label(ui, "Render profiles");
                render_profiles_ui(ui, &mut prefs.render_profiles);
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
