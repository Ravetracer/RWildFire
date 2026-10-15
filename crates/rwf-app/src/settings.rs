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

//! Preferences beyond the basics (JWildfire's `Prefs`): default folders of the file dialogs,
//! settings for new flames, preview limits, render threads and the style of the editor
//! controls. Folders and threads are read through globals, so every dialog and renderer
//! sees them without passing the preferences around.

use std::path::PathBuf;
use std::sync::RwLock;

use egui::{RichText, Ui};
use rwf_core::Flame;
use rwf_core::render::filter::FilterKernel;

use crate::theme;
use crate::ui::widgets;

/// The kinds of files with a default folder (JWildfire's flame, image, gradient, sound file,
/// movie and mesh drawers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Folder {
    Flames,
    Images,
    Gradients,
    Sound,
    Movies,
    Meshes,
}

impl Folder {
    pub const ALL: [Folder; 6] = [Folder::Flames, Folder::Images, Folder::Gradients, Folder::Sound, Folder::Movies, Folder::Meshes];

    pub fn key(self) -> &'static str {
        match self {
            Folder::Flames => "folder_flames",
            Folder::Images => "folder_images",
            Folder::Gradients => "folder_gradients",
            Folder::Sound => "folder_sound",
            Folder::Movies => "folder_movies",
            Folder::Meshes => "folder_meshes",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Folder::Flames => "Flames",
            Folder::Images => "Images",
            Folder::Gradients => "Gradients",
            Folder::Sound => "Music",
            Folder::Movies => "Movies, dances",
            Folder::Meshes => "Meshes",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Folder::Flames => "Opening and saving flames, the batch renderer's flames",
            Folder::Images => "Rendered images, videos and frames, background and texture images, render states",
            Folder::Gradients => "Gradient files",
            Folder::Sound => "Music and audio files (dances, audio envelopes)",
            Folder::Movies => "Movie maker (.jwfmovie) and dance (.rwfdance) projects",
            Folder::Meshes => "Exported meshes (OBJ, PLY)",
        }
    }
}

/// Default folders (empty: the system's choice).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Folders(pub [String; 6]);

impl Folders {
    pub fn get(&self, f: Folder) -> &str {
        &self.0[f as usize]
    }

    pub fn set(&mut self, f: Folder, v: String) {
        self.0[f as usize] = v;
    }
}

/// Settings of flames made with File › New, the random flame button and Explore's random
/// batches (JWildfire's `tinaDefault…` preferences).
#[derive(Clone, Debug, PartialEq)]
pub struct NewFlameDefaults {
    pub title: String,
    pub oversampling: u32,
    pub filter_kernel: FilterKernel,
    pub filter_radius: f64,
    pub de_radius: f64,
    pub de_curve: f64,
    pub white_level: f64,
    pub fg_opacity: f64,
    pub fps: f64,
    pub frame_count: u32,
}

impl Default for NewFlameDefaults {
    fn default() -> Self {
        let f = Flame::default();
        Self {
            title: String::new(),
            oversampling: f.spatial_oversampling,
            filter_kernel: f.filter_kernel,
            filter_radius: f.spatial_filter_radius,
            de_radius: f.de_radius,
            de_curve: f.de_curve,
            white_level: f.white_level,
            fg_opacity: f.fg_opacity,
            fps: f.animation.fps,
            frame_count: f.animation.frame_count,
        }
    }
}

impl NewFlameDefaults {
    /// A flame made from scratch (File › New): all settings and the title.
    pub fn apply(&self, f: &mut Flame) {
        if !self.title.trim().is_empty() {
            f.name = self.title.trim().to_string();
        }
        self.apply_to_defaults(f, &Flame::default(), false);
    }

    /// A generated flame: only the settings its generator left at their defaults
    /// (JWildfire applies them before the generator runs).
    pub fn apply_to_random(&self, f: &mut Flame) {
        self.apply_to_defaults(f, &Flame::default(), true);
    }

    fn apply_to_defaults(&self, f: &mut Flame, d: &Flame, only_untouched: bool) {
        macro_rules! set {
            ($($field:ident).+ = $v:expr) => {
                if !only_untouched || f.$($field).+ == d.$($field).+ {
                    f.$($field).+ = $v;
                }
            };
        }
        set!(spatial_oversampling = self.oversampling.max(1));
        set!(filter_kernel = self.filter_kernel);
        set!(spatial_filter_radius = self.filter_radius);
        set!(de_radius = self.de_radius);
        set!(de_curve = self.de_curve);
        set!(white_level = self.white_level);
        set!(fg_opacity = self.fg_opacity);
        set!(animation.fps = self.fps);
        set!(animation.frame_count = self.frame_count.max(1));
    }
}

/// How transforms are drawn on the canvas (`FlamePanelControlStyle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ControlShape {
    #[default]
    Triangle,
    Rectangle,
    Axis,
    Crosshair,
}

impl ControlShape {
    pub const ALL: [ControlShape; 4] = [ControlShape::Triangle, ControlShape::Rectangle, ControlShape::Axis, ControlShape::Crosshair];

    pub fn label(self) -> &'static str {
        match self {
            ControlShape::Triangle => "Triangles",
            ControlShape::Rectangle => "Rectangles",
            ControlShape::Axis => "Axes",
            ControlShape::Crosshair => "Crosshairs",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ControlShape::Triangle => "triangle",
            ControlShape::Rectangle => "rectangle",
            ControlShape::Axis => "axis",
            ControlShape::Crosshair => "crosshair",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::ALL.into_iter().find(|c| c.as_str() == s).unwrap_or_default()
    }
}

/// Style of the editor controls (`tinaEditorControls…`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlStyle {
    pub shape: ControlShape,
    /// One color per transform (otherwise all alike).
    pub color: bool,
    /// The transform's number inside its control.
    pub numbers: bool,
    /// Dark outlines for contrast with the image.
    pub shadows: bool,
}

impl Default for ControlStyle {
    fn default() -> Self {
        Self { shape: ControlShape::Triangle, color: true, numbers: false, shadows: true }
    }
}

struct Globals {
    folders: Folders,
    free_cores: usize,
}

static GLOBALS: RwLock<Globals> = RwLock::new(Globals { folders: Folders([const { String::new() }; 6]), free_cores: 0 });

/// Makes the folders and the thread setting visible to all dialogs and renderers.
pub fn set_globals(folders: &Folders, free_cores: usize) {
    if let Ok(mut g) = GLOBALS.write() {
        g.folders = folders.clone();
        g.free_cores = free_cores;
    }
    rwf_core::render::set_cpu_threads(cpu_threads_for(free_cores));
}

/// A file dialog that starts in the default folder of `kind` (if set and present).
pub fn dialog(kind: Folder) -> rfd::FileDialog {
    let dir = GLOBALS.read().ok().map(|g| g.folders.get(kind).trim().to_string()).unwrap_or_default();
    let d = rfd::FileDialog::new();
    if !dir.is_empty() && PathBuf::from(&dir).is_dir() { d.set_directory(dir) } else { d }
}

/// CPU threads for rendering: all cores but the ones kept free (at least one).
pub fn cpu_threads() -> usize {
    cpu_threads_for(GLOBALS.read().map_or(0, |g| g.free_cores))
}

pub fn cpu_threads_for(free_cores: usize) -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get()).saturating_sub(free_cores).max(1)
}

pub fn folders_ui(ui: &mut Ui, folders: &mut Folders) {
    widgets::grid(ui, "prefs-folders", |ui| {
        for f in Folder::ALL {
            widgets::label(ui, f.label());
            ui.horizontal(|ui| {
                if ui.small_button("Choose…").on_hover_text(f.hint()).clicked()
                    && let Some(p) = rfd::FileDialog::new().pick_folder()
                {
                    folders.set(f, p.to_string_lossy().to_string());
                }
                let v = folders.get(f).to_string();
                if !v.is_empty() {
                    if ui.small_button(crate::icons::X).on_hover_text("Back to the system's choice").clicked() {
                        folders.set(f, String::new());
                    }
                    ui.label(RichText::new(v).small().color(theme::TEXT_MUTED));
                } else {
                    ui.label(RichText::new("—").small().color(theme::TEXT_MUTED));
                }
            });
            ui.end_row();
        }
    });
}

pub fn new_flame_ui(ui: &mut Ui, d: &mut NewFlameDefaults) {
    ui.label(RichText::new("For File › New, the random flame button and Explore's random flames").small().color(theme::TEXT_MUTED));
    widgets::grid(ui, "prefs-new-flame", |ui| {
        widgets::label(ui, "Title");
        ui.add(egui::TextEdit::singleline(&mut d.title).hint_text("(none)").desired_width(160.0))
            .on_hover_text("Name of flames made with File › New (random flames keep their generator's name)");
        ui.end_row();
        widgets::label(ui, "Oversampling");
        ui.add(egui::DragValue::new(&mut d.oversampling).range(1..=6));
        ui.end_row();
        widgets::label(ui, "Filter kernel");
        egui::ComboBox::from_id_salt("prefs-new-kernel").selected_text(d.filter_kernel.label()).show_ui(ui, |ui| {
            for k in FilterKernel::ALL {
                ui.selectable_value(&mut d.filter_kernel, *k, k.label());
            }
        });
        ui.end_row();
        widgets::label(ui, "Filter radius");
        ui.add(egui::DragValue::new(&mut d.filter_radius).speed(0.01).range(0.0..=10.0)).on_hover_text("0 turns spatial filtering off");
        ui.end_row();
        widgets::label(ui, "DE radius");
        ui.add(egui::DragValue::new(&mut d.de_radius).speed(0.01).range(0.0..=20.0)).on_hover_text("Density estimation; 0 turns it off");
        ui.end_row();
        widgets::label(ui, "DE curve");
        ui.add(egui::DragValue::new(&mut d.de_curve).speed(0.01).range(0.0..=5.0)).on_hover_text("Higher: stronger effect, but more blur");
        ui.end_row();
        widgets::label(ui, "White level");
        ui.add(egui::DragValue::new(&mut d.white_level).speed(1.0).range(1.0..=1000.0)).on_hover_text("Fade to white");
        ui.end_row();
        widgets::label(ui, "Foreground opacity");
        ui.add(egui::DragValue::new(&mut d.fg_opacity).speed(0.01).range(0.0..=1.0)).on_hover_text("With transparent background");
        ui.end_row();
        widgets::label(ui, "Animation");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut d.frame_count).range(1..=100_000).suffix(" frames"));
            ui.add(egui::DragValue::new(&mut d.fps).range(1.0..=240.0).suffix(" fps"));
        });
        ui.end_row();
    });
}

pub fn controls_ui(ui: &mut Ui, c: &mut ControlStyle) {
    widgets::grid(ui, "prefs-controls", |ui| {
        widgets::label(ui, "Transform controls");
        egui::ComboBox::from_id_salt("prefs-control-shape").selected_text(c.shape.label()).show_ui(ui, |ui| {
            for s in ControlShape::ALL {
                ui.selectable_value(&mut c.shape, s, s.label());
            }
        });
        ui.end_row();
        widgets::label(ui, "");
        ui.horizontal(|ui| {
            ui.checkbox(&mut c.color, "Colored").on_hover_text("One color per transform");
            ui.checkbox(&mut c.numbers, "Numbers").on_hover_text("The transform's number inside its control");
            ui.checkbox(&mut c.shadows, "Shadows").on_hover_text("Dark outlines for contrast with bright images");
        });
        ui.end_row();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_flames_keep_their_generator_settings() {
        let d = NewFlameDefaults { title: "Mine".into(), oversampling: 2, filter_radius: 0.0, white_level: 150.0, ..Default::default() };
        let mut generated = Flame { name: "Classic".into(), white_level: 300.0, ..Flame::default() };
        d.apply_to_random(&mut generated);
        assert_eq!((generated.name.as_str(), generated.white_level), ("Classic", 300.0));
        assert_eq!((generated.spatial_oversampling, generated.spatial_filter_radius), (2, 0.0));
        let mut new = Flame { white_level: 300.0, ..Flame::default() };
        d.apply(&mut new);
        assert_eq!((new.name.as_str(), new.white_level), ("Mine", 150.0));
    }
}
