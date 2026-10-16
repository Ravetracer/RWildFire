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

//! Help window (JWildfire's "Hints, Help and About" and its variation list): a quick tour,
//! shortcuts, FAQ, coloring types, the searchable list of variations and the about page.

use egui::{RichText, Ui};

use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Start,
    Shortcuts,
    Faq,
    Coloring,
    Variations,
    Scripting,
    About,
}

impl Tab {
    const ALL: [(Tab, &str); 7] = [
        (Tab::Start, "Getting started"),
        (Tab::Shortcuts, "Shortcuts"),
        (Tab::Faq, "FAQ"),
        (Tab::Coloring, "Coloring types"),
        (Tab::Variations, "Variations"),
        (Tab::Scripting, "Scripting"),
        (Tab::About, "About"),
    ];
}

#[derive(Default)]
pub struct HelpWindow {
    pub open: bool,
    tab: Tab,
    search: String,
    only_3d: bool,
    only_gpu: bool,
}

/// Paragraphs of a help page: `## ` starts a heading, `- ` a bullet.
fn page(ui: &mut Ui, text: &str) {
    for block in text.trim().split("\n\n") {
        let block = block.trim();
        if let Some(h) = block.strip_prefix("## ") {
            ui.add_space(6.0);
            ui.label(RichText::new(h).strong().size(theme::fs(15.0)).color(theme::TEXT));
        } else if block.lines().all(|l| l.trim_start().starts_with("- ")) {
            for l in block.lines() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("•").color(theme::ACCENT));
                    ui.label(l.trim_start().trim_start_matches("- "));
                });
            }
        } else {
            ui.label(block.replace('\n', " "));
        }
        ui.add_space(4.0);
    }
}

/// Text with `code` spans: monospace, accent colored.
fn rich_line(ui: &mut Ui, text: &str) {
    let mut job = egui::text::LayoutJob::default();
    let body = egui::TextFormat { font_id: egui::TextStyle::Body.resolve(ui.style()), color: theme::TEXT, ..Default::default() };
    let code = egui::TextFormat { font_id: egui::TextStyle::Monospace.resolve(ui.style()), color: theme::ACCENT, ..Default::default() };
    for (i, part) in text.split('`').enumerate() {
        job.append(part, 0.0, if i % 2 == 1 { code.clone() } else { body.clone() });
    }
    ui.add(egui::Label::new(job).wrap());
}

/// The Markdown subset of the scripting reference: `#`/`##` headings, `- ` bullets (indented
/// continuation lines), indented code blocks, paragraphs with `code` spans.
fn markdown(ui: &mut Ui, text: &str) {
    for block in text.trim().split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(first) = lines.first() else { continue };
        if let Some(h) = first.strip_prefix("# ") {
            ui.label(RichText::new(h).strong().size(theme::fs(18.0)).color(theme::TEXT));
        } else if let Some(h) = first.strip_prefix("## ") {
            ui.add_space(6.0);
            ui.label(RichText::new(h).strong().size(theme::fs(15.0)).color(theme::TEXT));
            if lines.len() > 1 {
                rich_line(ui, &lines[1..].join(" "));
            }
        } else if lines.iter().all(|l| l.starts_with("    ")) {
            let code: Vec<&str> = lines.iter().map(|l| &l[4..]).collect();
            egui::Frame::new().fill(theme::SURFACE).corner_radius(4).inner_margin(8).show(ui, |ui| {
                ui.add(egui::Label::new(RichText::new(code.join("\n")).monospace().color(theme::TEXT)).wrap());
            });
        } else if first.starts_with("- ") {
            let mut items: Vec<String> = Vec::new();
            for l in &lines {
                match l.strip_prefix("- ") {
                    Some(item) => items.push(item.to_string()),
                    None => {
                        if let Some(last) = items.last_mut() {
                            last.push(' ');
                            last.push_str(l.trim());
                        }
                    }
                }
            }
            for item in items {
                ui.horizontal_top(|ui| {
                    ui.label(RichText::new("•").color(theme::ACCENT));
                    rich_line(ui, &item);
                });
            }
        } else {
            rich_line(ui, &lines.join(" "));
        }
        ui.add_space(4.0);
    }
}

const START: &str = "
## The editor

The window shows one flame: the transforms on the left, the preview in the middle, the inspector (Transform, Camera, Look, Render) on the right. Drag the triangles on the preview to move, turn and scale the transforms; every change shows at once.

## Making flames

- Random flame (Space) makes a flame with the generator chosen next to it; the sliders button holds the symmetry, weighting field, gradient and variation profile of random flames.
- Explore shows strips of random flames, quick mutations of the editor flame or the flames of an opened file; click one to take it. MutaGen opens a grid of mutations to steer a flame step by step.
- Browse shows the flames of a folder as thumbnails, by date.
- File › Scripts runs Lua scripts that build or change flames; Help › Scripting lists what scripts can do.

## Animating and rendering

- Animate opens the timeline: keyframes for any property, motion curves, playback and video export.
- Export (Ctrl+E) renders and saves an image or an animation.
- Render opens the render workspace: long interactive renders, the batch renderer, very large images (Quilt), movies from several flames, dancing flames with music and 3D meshes.

## Files

RWildFire opens and saves JWildfire, Apophysis and other XML flames (.flame), JWildfire movies (.jwfmovie) and its own dance projects (.rwfdance). Copy (Ctrl+C) and paste (Ctrl+V) move flames as text, e.g. to and from forums.
";

const SHORTCUTS: &[(&str, &str)] = &[
    ("F1", "This help"),
    ("Ctrl+N / Ctrl+O", "New flame / open a file"),
    ("Ctrl+S / Ctrl+Shift+S", "Save / save as"),
    ("Ctrl+Alt+S", "Quick save (no dialog)"),
    ("Ctrl+E", "Export an image or animation"),
    ("Ctrl+Q", "Quit"),
    ("Ctrl+Z / Ctrl+Shift+Z, Ctrl+Y", "Undo / redo"),
    ("Ctrl+C / Ctrl+V", "Copy / paste the flame as text"),
    ("Space", "Random flame; play/pause while the timeline is open"),
    ("F", "Fit the flame into the view"),
    ("Delete", "Remove the selected transform"),
    ("Arrow keys", "Move the selected transform (Ctrl: ×0.1, Shift: ×10)"),
    ("Alt+←/→, Alt+↑/↓", "Turn / scale the selected transform"),
    ("Drag a triangle", "Move the transform; drag its X or Y corner to turn and scale"),
    ("Shift+drag / Ctrl+drag a corner", "Only turn / only scale"),
    ("Drag the background", "Move the view (any mouse button)"),
    ("Mouse wheel", "Zoom around the cursor"),
    ("Browse: arrows, Enter, Delete", "Select, open, delete a flame"),
    ("Dance: 1 … 9, Space", "Switch to pool flame 1 … 9, play/pause"),
];

const FAQ: &[(&str, &str)] = &[
    ("What are params?", "Flame definitions as XML text, starting with <flame …> or <flames>. Paste them with Edit › Paste flame (Ctrl+V) or save them as .flame files and open them."),
    ("Which files can RWildFire read?", "JWildfire flames (also with layers, motion curves and solid rendering), Apophysis and other flam3-style XML, JWildfire movies (.jwfmovie). RWildFire saves flames in the same XML format; JWildfire reads them as long as they use variations both programs know."),
    ("GPU or CPU?", "The preview and most renders run on the GPU and are much faster. The CPU renderer is exact and used when there is no suitable GPU or where you choose it (Export, Movie, Dance). A few variations have no GPU version yet; the preview names them, and the CPU renderer draws them."),
    ("How do I change the render size?", "Export and the batch renderer take any size; resolution and quality profiles (Preferences) make often used sizes and qualities one click away, and a flame remembers the profiles chosen for it."),
    ("What are HDR files?", "Images with the raw, untonemapped brightness (Radiance .hdr or OpenEXR .exr, 32 bits per channel). Tonemapping programs such as Luminance HDR turn them into normal images. HDR images are saved as rendered, without the denoiser and the image effects."),
    ("How do I get a transparent background?", "Choose PNG in the export dialog and tick Transparent. The flame's foreground opacity (Look) sets how opaque the flame itself is."),
    ("What is the interactive renderer for?", "Long renders of one flame: it refines the image for as long as you let it, can pause, continue later from a saved render state, and saves the image at any time."),
    ("Where are the movie and video options?", "In Render › Movie and Render › Dance: size, frame rate, quality and the output format. MP4, MOV and WebM need ffmpeg in the PATH; PNG frames, ANB files and .flame sequences work without it."),
    ("What does the denoiser do?", "It removes the remaining grain of a render with Intel's Open Image Denoise (on the CPU or an NVIDIA GPU). Set it per flame (Render tab of the inspector) or for all flames in the Preferences; Blend mixes some of the original grain back in, Keep detail keeps the original at well sampled lines, which the denoiser would soften."),
    ("What are the image effects?", "Photo-style adjustments of the finished image on the Effects tab of the inspector: exposure, contrast, highlights, shadows, whites, blacks, texture, clarity, dehaze, tone curves, vignette and grain. They run after the denoiser, are saved with the flame and show in the canvas and in every saved image and video (not in HDR files and quilts)."),
    ("My text variation shows no text or the wrong font.", "Fonts installed after RWildFire started are found after a restart. The font name must match the name the system knows; unknown names fall back to a sans-serif font."),
    ("Where are my settings?", "In the preferences file (Linux: ~/.config/rwildfire/preferences.txt, Windows: %APPDATA%\\rwildfire). The Preferences window saves every change at once."),
    ("Can RWildFire run JWildfire scripts?", "No: JWildfire scripts are Java code. RWildFire scripts are Lua (File › Scripts); the examples include ports of JWildfire scripts, and most JWildfire scripts translate line by line (getXForms().add → flame:add_xform, setCoeff20 → c20, getModifiedWeights → set_chaos)."),
    ("How do I make a 3D mesh?", "Use a 3D flame (e.g. from a 3D random generator), open Render › Mesh, fit the box, sample the volume and adjust threshold, pre-filter, blur and smoothing; then save it as OBJ, PLY or a Sunflow scene, or export a point cloud or slice images."),
];

const COLORING: &str = "
Every point has a color. When a transform moves a point it also gives it a new color; how, the transform's coloring type decides (inspector › Transform › Color). The defaults suit most flames; the others meet special needs.

## None

Keeps the color of the incoming point. Default for final transforms, since anything else would narrow the colors.

## Diffusion

The standard flame coloring (Scott Draves'), default for normal transforms. The color is an index into the gradient (0 left, 1 right). Every step moves it towards the transform's Color; Speed sets how far: 0 halfway, 1 not at all, −1 all the way.

## Target

Ignores the gradient: the point's color is blended towards the transform's target color, with Speed as above.

## Target (gradient)

Like Target, but the target color comes from the gradient at the transform's Color, so it follows a new gradient.

## Distance

The color comes from the gradient at the distance the transform moved the point, starting at Color with Speed as the scale. The incoming color is ignored. Especially good for flames with a single non-blur transform.

## Cyclic

Shifts the gradient index by Speed and wraps around. It never settles on a color by itself, so another transform must use Diffusion. Speeds like m/n cycle through n colors, which suits tilings.

## Direct and true color variations

Direct color variations set the gradient index themselves (use Diffusion with them). True color variations set the color directly; other transforms keep those colors only with None, Target or Target (gradient).
";

impl HelpWindow {
    pub fn open_at(&mut self, tab: Tab) {
        self.open = true;
        self.tab = tab;
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("Help").open(&mut open).default_size([680.0, 560.0]).resizable(true).collapsible(false).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (t, label) in Tab::ALL {
                    ui.selectable_value(&mut self.tab, t, label);
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                Tab::Start => page(ui, START),
                Tab::Shortcuts => {
                    egui::Grid::new("help-shortcuts").num_columns(2).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
                        for (k, v) in SHORTCUTS {
                            ui.label(RichText::new(crate::icons::arrows(k)).monospace().color(theme::ACCENT));
                            ui.label(*v);
                            ui.end_row();
                        }
                    });
                }
                Tab::Faq => {
                    for (q, a) in FAQ {
                        egui::CollapsingHeader::new(RichText::new(*q).strong()).id_salt(q).show(ui, |ui| {
                            ui.label(crate::icons::arrows(a));
                        });
                    }
                }
                Tab::Coloring => page(ui, COLORING),
                Tab::Variations => self.variations(ui),
                Tab::Scripting => markdown(ui, rwf_script::API_REFERENCE),
                Tab::About => about(ui),
            });
        });
        self.open = open;
    }

    /// JWildfire's variation list: every variation with its parameters, searchable.
    fn variations(&mut self, ui: &mut Ui) {
        let all = rwf_core::variations::all();
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search names and parameters").desired_width(240.0));
            ui.checkbox(&mut self.only_3d, "3D only");
            ui.checkbox(&mut self.only_gpu, "With GPU support");
        });
        let q = self.search.trim().to_lowercase();
        let list: Vec<_> = all
            .iter()
            .filter(|d| !self.only_3d || d.is_3d)
            .filter(|d| !self.only_gpu || d.supports_gpu())
            .filter(|d| q.is_empty() || d.name.contains(&q) || d.params.iter().any(|p| p.name.to_lowercase().contains(&q)))
            .collect();
        ui.label(RichText::new(format!("{} of {} variations", list.len(), all.len())).small().color(theme::TEXT_MUTED));
        ui.add_space(4.0);
        egui::Grid::new("help-variations").num_columns(3).spacing([16.0, 4.0]).striped(true).show(ui, |ui| {
            for d in list {
                ui.label(RichText::new(d.name).monospace().color(theme::TEXT));
                let tags = [d.is_3d.then_some("3D"), (!d.supports_gpu()).then_some("CPU only")].into_iter().flatten().collect::<Vec<_>>().join(", ");
                ui.label(RichText::new(tags).small().color(theme::TEXT_MUTED));
                let params = d.params.iter().map(|p| p.name).collect::<Vec<_>>().join(", ");
                let short: String = if params.chars().count() > 60 { format!("{}…", params.chars().take(60).collect::<String>()) } else { params.clone() };
                let resp = ui.label(RichText::new(short).small().color(theme::TEXT_MUTED));
                if params.chars().count() > 60 {
                    resp.on_hover_text(params);
                }
                ui.end_row();
            }
        });
    }
}

fn about(ui: &mut Ui) {
    ui.label(RichText::new(format!("RWildFire {}", env!("CARGO_PKG_VERSION"))).strong().size(theme::fs(18.0)).color(theme::TEXT));
    ui.label("A fractal flame editor and renderer, written in Rust.");
    ui.add_space(8.0);
    page(
        ui,
        "
RWildFire is a port of JWildfire (Copyright (C) 1995-2026 Andreas Maschke and the JWildfire contributors), which is licensed under the GNU LGPL 2.1 or later.

RWildFire is licensed under the GNU Lesser General Public License v3.0 or later. Builds with the optional taprats variation (a port of Taprats by Craig S. Kaplan and Pierre Baillargeon) are covered by the GNU GPL v3 as a whole.

The AI denoiser uses Intel Open Image Denoise (Apache License 2.0).
",
    );
    ui.hyperlink_to("JWildfire on GitHub", "https://github.com/thargor6/JWildfire");
    ui.hyperlink_to("RWildFire on GitHub", "https://github.com/Ravetracer/RWildFire");
}

/// Opens the user manual PDF (`RWildFire-Manual-*.pdf`) with the system's PDF viewer. Searched
/// next to the executable, inside an AppImage (`usr/share/doc/rwildfire`), next to the AppImage
/// file and, for development builds, in the source tree's `docs/manual/build`.
pub fn open_user_manual() -> Result<std::path::PathBuf, String> {
    use std::path::PathBuf;
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let mut dirs: Vec<PathBuf> = exe_dir.iter().cloned().collect();
    if let Some(d) = &exe_dir {
        dirs.extend(d.parent().map(|usr| usr.join("share").join("doc").join("rwildfire")));
        // target/<profile>/rwf-app -> docs/manual/build
        dirs.extend(d.ancestors().nth(2).map(|root| root.join("docs").join("manual").join("build")));
    }
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        dirs.extend(PathBuf::from(appimage).parent().map(|d| d.to_path_buf()));
    }
    let pdf = dirs
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flat_map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())))
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("RWildFire-Manual") && n.to_string_lossy().ends_with(".pdf")))
        .max()
        .ok_or("The user manual (RWildFire-Manual-….pdf) was not found next to the program")?;
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(&pdf).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&pdf).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(&pdf).spawn();
    result.map(|_| pdf.clone()).map_err(|e| format!("Could not open {}: {e}", pdf.display()))
}
