//! Application shell: layout, shortcuts, file handling.

use std::path::{Path, PathBuf};
use std::time::Instant;

use egui::RichText;
use jwf_core::Flame;
use jwf_core::random::{Generator, generate};
use jwf_core::rng::Rng;

use crate::canvas::{self, CanvasState};
use crate::doc::Document;
use crate::explorer::Explorer;
use crate::export::ExportDialog;
use crate::preview::Preview;
use crate::theme;
use crate::ui::inspector::{self, InspectorState};
use crate::ui::topbar::{self, Action, TopBarInfo};
use crate::ui::transforms::{self, TransformsState};

pub struct JwfApp {
    doc: Document,
    preview: Preview,
    canvas: CanvasState,
    transforms: TransformsState,
    inspector: InspectorState,
    explorer: Explorer,
    explorer_open: bool,
    timeline: crate::timeline::Timeline,
    export: ExportDialog,
    generator: Generator,
    rng: Rng,
    toast: Option<(String, Instant)>,
    last_title: String,
}

impl JwfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        crate::icons::install(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let mut rng = Rng::from_entropy();
        let flame = generate(Generator::Classic, &mut rng);
        let mut app = Self {
            doc: Document::new(flame),
            preview: Preview::new(cc.wgpu_render_state.as_ref()),
            canvas: CanvasState::default(),
            transforms: TransformsState::default(),
            inspector: InspectorState::default(),
            explorer: Explorer::default(),
            explorer_open: false,
            timeline: Default::default(),
            export: ExportDialog::default(),
            generator: Generator::Classic,
            rng,
            toast: None,
            last_title: String::new(),
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
        match jwf_core::io::read_flames_from_file(path) {
            Ok(flames) if !flames.is_empty() => {
                let n = flames.len();
                self.doc.load(flames[0].clone(), Some(path.to_path_buf()));
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
        match jwf_core::io::write_flame_to_file(&self.doc.flame, &path) {
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
                let mut xf = jwf_core::XForm::with_variation("linear3D", 1.0);
                xf.coefs.scale(0.5);
                f.first_layer_mut().xforms.push(xf);
                f.pixels_per_unit = 200.0;
                self.doc.load(f, None);
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
            Action::Undo => self.doc.undo(),
            Action::Redo => self.doc.redo(),
            Action::Randomize => {
                let f = generate(self.generator, &mut self.rng);
                self.doc.replace(f);
                self.doc.path = None;
            }
            Action::ToggleTimeline => self.timeline.open = !self.timeline.open,
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
        let typing = ctx.memory(|m| m.focused().is_some()) && ctx.egui_wants_keyboard_input();
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
                if jwf_core::render::analysis::auto_frame(&mut f) {
                    self.doc.flame = f;
                }
            }
            if del {
                let sel = self.doc.selection;
                let l = self.doc.layer_mut();
                match sel {
                    crate::doc::Selection::XForm(i) if i < l.xforms.len() => {
                        l.xforms.remove(i);
                    }
                    crate::doc::Selection::Final(i) if i < l.final_xforms.len() => {
                        l.final_xforms.remove(i);
                    }
                    _ => {}
                }
                self.doc.fix_selection();
            }
        }
        act
    }
}

impl eframe::App for JwfApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        // Dropped files
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if let Some(p) = dropped.first() {
            self.open_path(p);
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
            };
            if let Some(a) = topbar::show(ui, &info, &mut self.generator) {
                action = Some(a);
            }
        });

        // ---- explorer strip ----
        if self.explorer_open {
            egui::Panel::bottom("explorer").resizable(false).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| {
                if let Some(f) = self.explorer.show(ui, &self.doc.flame, &self.doc.library) {
                    self.doc.replace(f);
                }
            });
        }

        // ---- timeline ----
        if self.timeline.open {
            egui::Panel::bottom("timeline").resizable(false).exact_size(crate::timeline::Timeline::panel_height(&self.doc.flame)).frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| {
                self.timeline.show(ui, &mut self.doc);
            });
        }

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

        self.export.show(&ctx, &self.doc.flame, self.preview.gpu_context());
        let current = self.doc.layer().gradient.clone();
        if let Some(g) = self.inspector.library.show_window(&ctx, &current) {
            self.doc.layer_mut().gradient = g;
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
        self.timeline.auto_key(&mut self.doc, pointer_down);
        self.doc.commit_if_settled(pointer_down);
        let title = format!("{} — JWildfire", self.doc.title());
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }
}
