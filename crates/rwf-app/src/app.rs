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

//! Application shell: layout, shortcuts, file handling.

use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::RichText;
use rwf_core::Flame;
use rwf_core::random::{Generator, generate, generate_with};
use rwf_core::rng::Rng;

use crate::canvas::{self, CanvasState};
use crate::doc::Document;
use crate::explorer::Explorer;
use crate::export::ExportDialog;
use crate::preview::Preview;
use crate::theme;
use crate::ui::inspector::{self, InspectorState};
use crate::ui::topbar::{self, Action, TopBarInfo};
use crate::ui::transforms::{self, TransformsState};

#[derive(Clone, Copy, PartialEq, Eq)]
enum RenderTab {
    Interactive,
    Batch,
    Quilt,
    Movie,
    Dance,
    Mesh,
}

pub struct RwfApp {
    doc: Document,
    preview: Preview,
    canvas: CanvasState,
    transforms: TransformsState,
    inspector: InspectorState,
    explorer: Explorer,
    explorer_open: bool,
    mutagen: crate::mutagen::MutaGen,
    curves: crate::curve_editor::CurveEditor,
    /// The curve editor changed keys; re-evaluate the frame once the pointer is released.
    curves_changed: bool,
    timeline: crate::timeline::Timeline,
    export: ExportDialog,
    render: crate::render_view::InteractiveRenderer,
    batch: crate::batch_view::BatchView,
    quilt: crate::quilt_view::QuiltView,
    movie: crate::movie_view::MovieView,
    mesh: crate::mesh_view::MeshView,
    dance: crate::dance_view::DanceView,
    render_tab: RenderTab,
    generator: Generator,
    rng: Rng,
    /// A random flame being generated on a background thread.
    randomizing: Option<std::sync::mpsc::Receiver<Flame>>,
    toast: Option<(String, Instant)>,
    last_title: String,
    prefs: crate::prefs::Prefs,
    prefs_window: crate::prefs::PrefsWindow,
}

impl RwfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        crate::icons::install(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let mut rng = Rng::from_entropy();
        let prefs = crate::prefs::Prefs::load();
        let flame = prefs.adopt(generate(Generator::Classic, &mut rng));
        let mut app = Self {
            doc: Document::new(flame),
            preview: Preview::new(cc.wgpu_render_state.as_ref()),
            canvas: CanvasState::default(),
            transforms: TransformsState::default(),
            inspector: InspectorState::default(),
            explorer: Explorer::default(),
            explorer_open: false,
            mutagen: Default::default(),
            curves: Default::default(),
            curves_changed: false,
            timeline: Default::default(),
            export: ExportDialog::default(),
            render: Default::default(),
            batch: Default::default(),
            quilt: Default::default(),
            movie: Default::default(),
            mesh: Default::default(),
            dance: Default::default(),
            render_tab: RenderTab::Interactive,
            generator: Generator::Classic,
            rng,
            randomizing: None,
            toast: None,
            last_title: String::new(),
            prefs,
            prefs_window: Default::default(),
        };
        if let Some(p) = initial {
            app.open_path(&p);
        }
        app
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn open_path(&mut self, path: &Path) {
        if path.extension().is_some_and(|e| e == "rwfdance") {
            match self.dance.open_file(path) {
                Ok(()) => {
                    self.render.open = true;
                    self.render_tab = RenderTab::Dance;
                }
                Err(e) => self.notify(format!("Could not open file: {e}")),
            }
            return;
        }
        match rwf_core::io::read_flames_from_file(path) {
            Ok(flames) if !flames.is_empty() => {
                let n = flames.len();
                self.doc.load(self.prefs.adopt(flames[0].clone()), Some(path.to_path_buf()));
                self.doc.library = flames;
                if n > 1 {
                    self.notify(format!("{n} flames in file — browse them in Explore › File"));
                    self.explorer.mode = crate::explorer::Mode::File;
                    self.explorer_open = true;
                    self.explorer.refresh(&self.doc.flame, &self.doc.library);
                }
            }
            Ok(_) => self.notify("No flame found in file"),
            Err(e) => self.notify(format!("Could not open file: {e}")),
        }
    }

    fn save_to(&mut self, path: PathBuf) {
        match rwf_core::io::write_flame_to_file(&self.doc.flame, &path) {
            Ok(()) => {
                self.doc.path = Some(path.clone());
                self.doc.dirty = false;
                self.notify(format!("Saved {}", path.display()));
            }
            Err(e) => self.notify(format!("Save failed: {e}")),
        }
    }

    fn run(&mut self, action: Action) {
        match action {
            Action::New => {
                let mut f = Flame { bg_transparency: false, width: 1280, height: 720, ..Flame::default() };
                let mut xf = rwf_core::XForm::with_variation("linear3D", 1.0);
                xf.coefs.scale(0.5);
                f.first_layer_mut().xforms.push(xf);
                f.pixels_per_unit = 200.0;
                self.doc.load(self.prefs.adopt(f), None);
            }
            Action::Open => {
                if let Some(p) = rfd::FileDialog::new().add_filter("Flame", &["flame", "xml"]).pick_file() {
                    self.open_path(&p);
                }
            }
            Action::Save => match self.doc.path.clone() {
                Some(p) => self.save_to(p),
                None => self.run(Action::SaveAs),
            },
            Action::SaveAs => {
                if let Some(p) = rfd::FileDialog::new().add_filter("Flame", &["flame"]).set_file_name("flame.flame").save_file() {
                    self.save_to(p);
                }
            }
            Action::Export => {
                self.export.open_for(&self.doc.flame, self.preview.gpu_available());
            }
            Action::Preferences => self.prefs_window.open = true,
            Action::Undo => self.doc.undo(),
            Action::Redo => self.doc.redo(),
            Action::Randomize => {
                // Some generators render several previews (seconds): keep the UI responsive.
                if self.randomizing.is_none() {
                    let (tx, rx) = std::sync::mpsc::channel();
                    let (generator, extras, seed) = (self.generator, self.explorer.extras, self.rng.next_u64());
                    std::thread::spawn(move || {
                        let _ = tx.send(generate_with(generator, extras, &mut Rng::new(seed)));
                    });
                    self.randomizing = Some(rx);
                }
            }
            Action::ToggleTimeline => self.timeline.open = !self.timeline.open,
            Action::ToggleRender => {
                if self.render.open {
                    self.render.open = false;
                } else {
                    self.render.open_with(&self.doc.flame);
                }
            }
            Action::ToggleExplorer => {
                self.explorer_open = !self.explorer_open;
                if self.explorer_open {
                    self.explorer.generator = self.generator;
                    self.explorer.refresh(&self.doc.flame, &self.doc.library);
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) -> Option<Action> {
        use egui::{Key, KeyboardShortcut, Modifiers};
        // The editing shortcuts act on the editor only.
        let typing = (ctx.memory(|m| m.focused().is_some()) && ctx.egui_wants_keyboard_input()) || self.render.open;
        let mut act = None;
        ctx.input_mut(|i| {
            let c = Modifiers::COMMAND;
            let cs = Modifiers::COMMAND | Modifiers::SHIFT;
            if i.consume_shortcut(&KeyboardShortcut::new(cs, Key::Z)) || i.consume_shortcut(&KeyboardShortcut::new(c, Key::Y)) {
                act = Some(Action::Redo);
            } else if i.consume_shortcut(&KeyboardShortcut::new(c, Key::Z)) {
                act = Some(Action::Undo);
            } else if i.consume_shortcut(&KeyboardShortcut::new(cs, Key::S)) {
                act = Some(Action::SaveAs);
            } else if i.consume_shortcut(&KeyboardShortcut::new(c, Key::S)) {
                act = Some(Action::Save);
            } else if i.consume_shortcut(&KeyboardShortcut::new(c, Key::O)) {
                act = Some(Action::Open);
            } else if i.consume_shortcut(&KeyboardShortcut::new(c, Key::N)) {
                act = Some(Action::New);
            } else if i.consume_shortcut(&KeyboardShortcut::new(c, Key::E)) {
                act = Some(Action::Export);
            } else if !typing && i.consume_key(Modifiers::NONE, Key::Space) {
                if self.timeline.open {
                    self.timeline.playing = !self.timeline.playing;
                } else {
                    act = Some(Action::Randomize);
                }
            }
        });
        if !typing {
            let (fit, del) = ctx.input(|i| (i.key_pressed(egui::Key::F) && i.modifiers.is_none(), i.key_pressed(egui::Key::Delete)));
            if fit {
                let mut f = self.doc.flame.clone();
                if rwf_core::render::analysis::auto_frame(&mut f) {
                    self.doc.flame = f;
                }
            }
            if del {
                if let Some((kind, i)) = self.doc.selection.kind() {
                    let list = self.doc.layer_mut().xforms_of_mut(kind);
                    if i < list.len() {
                        list.remove(i);
                    }
                }
                self.doc.fix_selection();
            }
        }
        act
    }
}

impl eframe::App for RwfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Dropped files
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if self.render.open && self.render_tab == RenderTab::Batch && !dropped.is_empty() {
            let flames: Vec<PathBuf> = dropped.iter().filter(|p| crate::batch_view::is_flame_file(p)).cloned().collect();
            self.batch.add_paths(&flames);
        } else if let Some(p) = dropped.first() {
            self.open_path(p);
        }
        self.batch.poll(&ctx);
        self.quilt.poll(&ctx);

        if let Some(rx) = &self.randomizing {
            match rx.try_recv() {
                Ok(f) => {
                    self.randomizing = None;
                    self.doc.replace(self.prefs.adopt(f));
                    self.doc.path = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(50)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.randomizing = None,
            }
        }

        let mut action = self.shortcuts(&ctx);
        self.timeline.update(&ctx, &mut self.doc);

        // ---- top bar ----
        egui::Panel::top("topbar").exact_size(46.0).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 6))).show(ui, |ui| {
            let title = self.doc.title();
            let info = TopBarInfo {
                title: &title,
                can_undo: self.doc.can_undo(),
                can_redo: self.doc.can_redo(),
                explorer_open: self.explorer_open,
                timeline_open: self.timeline.open,
                render_open: self.render.open,
                render_busy: self.batch.is_running() || self.quilt.is_running() || self.movie.is_running() || self.mesh.is_running() || self.dance.is_running(),
                randomizing: self.randomizing.is_some(),
            };
            if let Some(a) = topbar::show(ui, &info, &mut self.generator, &mut self.explorer.extras) {
                action = Some(a);
            }
        });

        // ---- interactive renderer (replaces the editor panels) ----
        if self.render.open {
            egui::Panel::top("render-tabs").exact_size(36.0).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 4))).show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.selectable_value(&mut self.render_tab, RenderTab::Interactive, "Interactive").on_hover_text("One flame, progressive, with pause and resume");
                    let batch = if self.batch.is_running() { "Batch •" } else { "Batch" };
                    ui.selectable_value(&mut self.render_tab, RenderTab::Batch, batch).on_hover_text("Render a queue of flames to files");
                    let quilt = if self.quilt.is_running() { "Quilt •" } else { "Quilt" };
                    ui.selectable_value(&mut self.render_tab, RenderTab::Quilt, quilt).on_hover_text("Very large images, rendered tile by tile");
                    let movie = if self.movie.is_running() { "Movie •" } else { "Movie" };
                    ui.selectable_value(&mut self.render_tab, RenderTab::Movie, movie).on_hover_text("A movie from flames that fade or morph into each other");
                    let dance = if self.dance.is_running() { "Dance •" } else { "Dance" };
                    ui.selectable_value(&mut self.render_tab, RenderTab::Dance, dance).on_hover_text("Dancing flames: a pool of flames moved by music in real time");
                    let mesh = if self.mesh.is_running() { "Mesh •" } else { "Mesh" };
                    ui.selectable_value(&mut self.render_tab, RenderTab::Mesh, mesh).on_hover_text("A 3D mesh (OBJ, PLY) of the flame");
                });
            });
            match self.render_tab {
                RenderTab::Interactive => {
                    if let Some(f) = self.render.show(ui, frame, &self.doc.flame, self.preview.gpu_context()) {
                        self.doc.replace(self.prefs.adopt(f));
                        self.render.open = false;
                        self.notify("Flame loaded into the editor");
                    }
                }
                RenderTab::Batch => self.batch.show(ui, &self.doc, self.preview.gpu_available()),
                RenderTab::Quilt => {
                    self.quilt.sync_flame(&self.doc);
                    self.quilt.show(ui, &self.doc, self.preview.gpu_available());
                }
                RenderTab::Mesh => self.mesh.show(ui, &self.doc),
                RenderTab::Dance => {
                    if let Some(f) = self.dance.show(ui, frame, &self.doc.flame, self.preview.gpu_context(), (self.generator, self.explorer.extras)) {
                        self.doc.replace(self.prefs.adopt(f));
                        self.render.open = false;
                        self.notify("Flame loaded into the editor");
                    }
                }
                RenderTab::Movie => {
                    if let Some(f) = self.movie.show(ui, &self.doc, self.preview.gpu_context(), (self.generator, self.explorer.extras)) {
                        self.doc.replace(self.prefs.adopt(f));
                        self.render.open = false;
                        self.notify("Flame loaded into the editor");
                    }
                }
            }
        }

        // ---- explorer strip ----
        if self.explorer_open && !self.render.open {
            egui::Panel::bottom("explorer").resizable(false).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| {
                if let Some(f) = self.explorer.show(ui, &self.doc.flame, &self.doc.library) {
                    self.doc.replace(self.prefs.adopt(f));
                }
                if std::mem::take(&mut self.explorer.open_mutagen) {
                    self.mutagen.open_with(&self.doc.flame);
                }
            });
        }

        // ---- timeline ----
        if self.timeline.open && !self.render.open {
            egui::Panel::bottom("timeline").resizable(false).exact_size(crate::timeline::Timeline::panel_height(&self.doc.flame)).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| {
                self.timeline.show(ui, &mut self.doc);
            });
        }
        if let Some(path) = self.timeline.curve_request.take() {
            self.curves.open_with(path);
        }

        if !self.render.open {
            // ---- left: transforms ----
            egui::Panel::left("transforms").resizable(true).default_size(290.0).size_range(240.0..=460.0).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(10, 8))).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    transforms::show(ui, &mut self.doc, &mut self.transforms);
                });
            });

            // ---- right: inspector ----
            egui::Panel::right("inspector").resizable(true).default_size(340.0).size_range(300.0..=520.0).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 10))).show(ui, |ui| {
                inspector::show(ui, &mut self.doc, &mut self.inspector, &mut self.preview, &mut self.canvas);
            });

            // ---- center: canvas ----
            egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::CANVAS)).show(ui, |ui| {
                canvas::show(ui, frame, &mut self.doc, &mut self.canvas, &mut self.preview);
            });
        }

        self.export.show(&ctx, &self.doc.flame, self.preview.gpu_context());
        let mut playhead = self.timeline.current_frame();
        self.curves_changed |= self.curves.show(&ctx, &mut self.doc.flame, &mut playhead);
        if playhead != self.timeline.current_frame() {
            self.timeline.frame = playhead;
            self.timeline.playing = false;
        }
        if let Some(f) = self.mutagen.show(&ctx, &self.doc.flame) {
            self.doc.replace(self.prefs.adopt(f));
            self.notify("Flame loaded into the editor");
        }
        let current = self.doc.layer().gradient.clone();
        if let Some(g) = self.inspector.library.show_window(&ctx, &current) {
            self.doc.layer_mut().gradient = g;
        }

        if self.prefs_window.show(&ctx, &mut self.prefs) {
            self.prefs.apply(&mut self.doc.flame);
        }

        if let Some(a) = action {
            self.run(a);
        }

        // Toast
        if let Some((msg, t)) = &self.toast {
            if t.elapsed().as_secs_f32() < 4.0 {
                egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_TOP, [0.0, 58.0]).interactable(false).show(&ctx, |ui| {
                    egui::Frame::new().fill(theme::SURFACE).corner_radius(8).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
                        ui.label(RichText::new(msg).color(theme::TEXT));
                    });
                });
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            } else {
                self.toast = None;
            }
        }

        // History + window title
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if self.curves_changed && !pointer_down {
            self.curves_changed = false;
            self.timeline.invalidate();
        }
        self.timeline.auto_key(&mut self.doc, pointer_down);
        self.doc.commit_if_settled(pointer_down);
        let title = format!("{} — RWildFire", self.doc.title());
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}
