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

//! Scripts window: list of example and own Lua scripts, editor, parameter form, run/stop and
//! the script output. Scripts run on a background thread on a copy of the editor flame.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use egui::{Color32, RichText, Ui};
use rwf_core::model::Flame;
use rwf_script::{Header, ParamKind, ParamValue, RunInput, RunOutput, ScriptError};

use crate::theme;

const ERROR: Color32 = Color32::from_rgb(0xf0, 0x6a, 0x5f);

/// Where the open script came from.
#[derive(Clone, PartialEq)]
enum Source {
    Example(&'static str),
    File(PathBuf),
    New,
}

enum Msg {
    Print(String),
    Done(Box<Result<RunOutput, ScriptError>>),
}

struct Running {
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
    started: Instant,
}

/// What a finished script hands to the editor.
pub struct ScriptOutcome {
    /// The changed editor flame.
    pub flame: Option<Flame>,
    /// Flames created with `rwf.output`.
    pub flames: Vec<Flame>,
}

pub struct ScriptWindow {
    pub open: bool,
    source: Source,
    text: String,
    dirty: bool,
    header: Header,
    /// Parameter values of the form, kept by name while the header is edited.
    values: Vec<(String, ParamValue)>,
    log: Vec<(String, bool)>,
    running: Option<Running>,
    /// Script files of the scripts folder (refreshed when the window opens).
    files: Vec<PathBuf>,
    /// A script to switch to once the user confirmed dropping the unsaved changes.
    pending: Option<Source>,
    /// Opens the API reference in the help window.
    pub show_reference: bool,
}

impl Default for ScriptWindow {
    fn default() -> Self {
        let mut w = Self {
            open: false,
            source: Source::New,
            text: String::new(),
            dirty: false,
            header: Header::default(),
            values: Vec::new(),
            log: Vec::new(),
            running: None,
            files: Vec::new(),
            pending: None,
            show_reference: false,
        };
        let (name, src) = rwf_script::EXAMPLES[0];
        w.load(Source::Example(name), src.to_string());
        w
    }
}

/// Folder of the user's scripts: the Scripts folder of the preferences, else `scripts` in the
/// configuration folder.
pub fn scripts_dir() -> Option<PathBuf> {
    let chosen = crate::settings::folder(crate::settings::Folder::Scripts);
    if !chosen.is_empty() {
        return Some(PathBuf::from(chosen));
    }
    crate::prefs::config_dir().map(|d| d.join("scripts"))
}

const TEMPLATE: &str = "--- New script
--- What the script does.
--@param amount number 0.5 0 1  Example parameter

-- `flame` is the editor flame; see Help › Scripting for the API.
local xf = flame:add_xform(\"spherical\", params.amount)
xf:scale(0.5)
print(flame)
";

impl ScriptWindow {
    pub fn open(&mut self) {
        self.open = true;
        self.refresh_files();
    }

    fn refresh_files(&mut self) {
        self.files = scripts_dir()
            .and_then(|d| std::fs::read_dir(d).ok())
            .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == rwf_script::EXTENSION)).collect())
            .unwrap_or_default();
        self.files.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    }

    fn name(&self) -> String {
        match &self.source {
            Source::Example(n) => n.to_string(),
            Source::File(p) => p.file_name().map_or("script.lua".into(), |n| n.to_string_lossy().to_string()),
            Source::New => "new script.lua".into(),
        }
    }

    fn load(&mut self, source: Source, text: String) {
        self.source = source;
        self.text = text;
        self.dirty = false;
        self.values.clear();
        self.reparse();
    }

    /// Opens `source` (reads files from disk).
    fn switch_to(&mut self, source: Source) {
        let text = match &source {
            Source::Example(name) => rwf_script::EXAMPLES.iter().find(|(n, _)| n == name).map(|(_, s)| s.to_string()).unwrap_or_default(),
            Source::File(p) => match std::fs::read_to_string(p) {
                Ok(t) => t,
                Err(e) => {
                    self.log.push((format!("Cannot read {}: {e}", p.display()), true));
                    return;
                }
            },
            Source::New => TEMPLATE.to_string(),
        };
        self.load(source, text);
    }

    fn request_switch(&mut self, source: Source) {
        if self.dirty && source != self.source {
            self.pending = Some(source);
        } else {
            self.switch_to(source);
        }
    }

    /// Re-reads the header; keeps the values of parameters that still exist with the same type.
    fn reparse(&mut self) {
        self.header = Header::parse(&self.text);
        let old = std::mem::take(&mut self.values);
        self.values = self
            .header
            .params
            .iter()
            .map(|p| {
                let kept = old.iter().find(|(n, _)| *n == p.name).map(|(_, v)| v.clone()).filter(|v| match (&p.kind, v) {
                    (ParamKind::Number { .. } | ParamKind::Integer { .. }, ParamValue::Number(_)) | (ParamKind::Bool, ParamValue::Bool(_)) => true,
                    (ParamKind::Choice(o), ParamValue::Text(t)) => o.contains(t),
                    _ => false,
                });
                (p.name.clone(), kept.unwrap_or_else(|| p.default.clone()))
            })
            .collect();
    }

    fn save_to(&mut self, path: PathBuf) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(&path, &self.text) {
            Ok(()) => {
                self.log.push((format!("Saved {}", path.display()), false));
                self.source = Source::File(path);
                self.dirty = false;
                self.refresh_files();
            }
            Err(e) => self.log.push((format!("Cannot save {}: {e}", path.display()), true)),
        }
    }

    fn save_as(&mut self) {
        let mut d =
            crate::settings::dialog(crate::settings::Folder::Scripts).add_filter("Lua script", &[rwf_script::EXTENSION]).set_file_name(self.name());
        if crate::settings::folder(crate::settings::Folder::Scripts).is_empty()
            && let Some(dir) = scripts_dir()
        {
            let _ = std::fs::create_dir_all(&dir);
            d = d.set_directory(dir);
        }
        if let Some(p) = d.save_file() {
            self.save_to(p);
        }
    }

    fn run(&mut self, flame: &Flame) {
        if self.running.is_some() {
            return;
        }
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (text, name, params, flame) = (self.text.clone(), self.name(), self.values.clone(), flame.clone());
        let base_dir = match &self.source {
            Source::File(p) => p.parent().map(Path::to_path_buf),
            _ => scripts_dir(),
        };
        let stop = cancel.clone();
        std::thread::spawn(move || {
            let print_tx = tx.clone();
            let input = RunInput {
                flame,
                params,
                cancel: stop,
                base_dir,
                on_print: Some(Box::new(move |l: &str| drop(print_tx.send(Msg::Print(l.to_string()))))),
            };
            let _ = tx.send(Msg::Done(Box::new(rwf_script::run(&text, &name, input))));
        });
        self.log.clear();
        self.running = Some(Running { rx, cancel, started: Instant::now() });
    }

    /// Collects messages of a running script; the outcome once it finished.
    fn poll(&mut self, ctx: &egui::Context) -> Option<ScriptOutcome> {
        let run = self.running.as_ref()?;
        let mut done = None;
        while let Ok(msg) = run.rx.try_recv() {
            match msg {
                Msg::Print(l) => self.log.push((l, false)),
                Msg::Done(r) => done = Some(*r),
            }
        }
        let Some(result) = done else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            return None;
        };
        let secs = run.started.elapsed().as_secs_f64();
        self.running = None;
        match result {
            Ok(out) => {
                let mut what = Vec::new();
                if out.changed {
                    what.push("flame changed (Undo takes it back)".to_string());
                }
                if !out.flames.is_empty() {
                    what.push(format!("{} flame(s) created, shown in Explore › File", out.flames.len()));
                }
                if what.is_empty() {
                    what.push("the flame is unchanged".into());
                }
                self.log.push((format!("Finished in {secs:.2} s: {}.", what.join(", ")), false));
                Some(ScriptOutcome { flame: out.changed.then_some(out.flame), flames: out.flames })
            }
            Err(e) => {
                self.log.push((if e.cancelled { format!("Stopped after {secs:.1} s.") } else { e.message }, !e.cancelled));
                None
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, flame: &Flame) -> Option<ScriptOutcome> {
        let outcome = self.poll(ctx);
        if !self.open {
            if let Some(r) = &self.running {
                r.cancel.store(true, Ordering::Relaxed);
            }
            return outcome;
        }
        let mut open = self.open;
        egui::Window::new("Scripts").open(&mut open).default_size([900.0, 600.0]).min_width(640.0).collapsible(false).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(200.0);
                    self.list(ui);
                });
                ui.separator();
                ui.vertical(|ui| self.editor(ui, flame));
            });
        });
        self.open = open;
        self.confirm_switch(ctx);
        outcome
    }

    fn list(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui.button("New").on_hover_text("A new script from a short template").clicked() {
                self.request_switch(Source::New);
            }
            if ui.button("Open…").on_hover_text("Open a Lua script from any folder").clicked()
                && let Some(p) =
                    crate::settings::dialog(crate::settings::Folder::Scripts).add_filter("Lua script", &[rwf_script::EXTENSION]).pick_file()
            {
                self.request_switch(Source::File(p));
            }
            if ui.small_button(crate::icons::REFRESH).on_hover_text("Read the scripts folder again").clicked() {
                self.refresh_files();
            }
        });
        egui::ScrollArea::vertical().id_salt("script-list").auto_shrink([false, false]).show(ui, |ui| {
            let heading = |ui: &mut Ui, t: &str| {
                ui.add_space(6.0);
                ui.label(RichText::new(t.to_uppercase()).size(10.5).color(theme::TEXT_MUTED).strong());
            };
            heading(ui, "Examples");
            for (name, _) in rwf_script::EXAMPLES {
                let selected = self.source == Source::Example(name);
                if ui.selectable_label(selected, name.trim_end_matches(".lua")).clicked() {
                    self.request_switch(Source::Example(name));
                }
            }
            heading(ui, "My scripts");
            if self.files.is_empty() {
                let dir = scripts_dir().map_or("(no folder)".into(), |d| d.display().to_string());
                ui.label(RichText::new(format!("None yet. Saved scripts appear here ({dir}).")).small().color(theme::TEXT_MUTED));
            }
            let files = self.files.clone();
            for p in files {
                let selected = self.source == Source::File(p.clone());
                let name = p.file_stem().map_or(String::new(), |n| n.to_string_lossy().to_string());
                if ui.selectable_label(selected, name).on_hover_text(p.display().to_string()).clicked() {
                    self.request_switch(Source::File(p));
                }
            }
        });
    }

    fn editor(&mut self, ui: &mut Ui, flame: &Flame) {
        let busy = self.running.is_some();
        // Ctrl+Enter runs (consumed before the text editor turns it into a line break).
        if !busy && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter)) {
            self.run(flame);
        }
        ui.horizontal(|ui| {
            if busy {
                if ui
                    .button(format!("{} Stop", egui_phosphor::regular::STOP))
                    .on_hover_text("Stops the script; the editor flame stays as it was")
                    .clicked()
                    && let Some(r) = &self.running
                {
                    r.cancel.store(true, Ordering::Relaxed);
                }
                ui.spinner();
            } else if ui
                .add(egui::Button::new(RichText::new(format!("{} Run", crate::icons::PLAY)).color(theme::ACCENT)))
                .on_hover_text("Runs the script on a copy of the editor flame (Ctrl+Enter); the result is one undo step")
                .clicked()
            {
                self.run(flame);
            }
            ui.separator();
            let can_save = matches!(self.source, Source::File(_));
            if ui.add_enabled(can_save && self.dirty, egui::Button::new("Save")).on_hover_text("Save the script file").clicked()
                && let Source::File(p) = self.source.clone()
            {
                self.save_to(p);
            }
            if ui.button("Save as…").on_hover_text("Save the script under a new name (examples are saved as your own copy)").clicked() {
                self.save_as();
            }
            ui.separator();
            if ui.button("API reference").on_hover_text("All functions scripts can use (Help › Scripting)").clicked() {
                self.show_reference = true;
            }
            let title = format!("{}{}", self.name(), if self.dirty { " •" } else { "" });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(title).color(theme::TEXT_MUTED)).truncate());
            });
        });
        if !self.header.title.is_empty() || !self.header.description.is_empty() {
            ui.add_space(2.0);
            ui.label(RichText::new(&self.header.title).strong());
            if !self.header.description.is_empty() {
                ui.label(RichText::new(&self.header.description).small().color(theme::TEXT_MUTED));
            }
        }
        for w in &self.header.warnings {
            ui.label(RichText::new(format!("{} {w}", crate::icons::WARNING)).small().color(theme::WARN));
        }
        if !self.values.is_empty() {
            ui.add_space(4.0);
            ui.add_enabled_ui(!busy, |ui| self.params_form(ui));
        }
        ui.add_space(4.0);
        let log_height = 120.0;
        let editor_height = (ui.available_height() - log_height - 12.0).max(120.0);
        egui::ScrollArea::vertical().id_salt("script-code").max_height(editor_height).auto_shrink([false, false]).show(ui, |ui| {
            let r = ui.add_enabled(
                !busy,
                egui::TextEdit::multiline(&mut self.text)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(((editor_height / 15.0) as usize).max(6))
                    .lock_focus(true),
            );
            if r.changed() {
                self.dirty = true;
                self.reparse();
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().id_salt("script-log").max_height(log_height).auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            if self.log.is_empty() {
                ui.label(RichText::new("Output of print() and errors appear here.").small().color(theme::TEXT_MUTED));
            }
            for (line, is_err) in &self.log {
                let t = RichText::new(line).monospace().size(12.0);
                ui.add(egui::Label::new(if *is_err { t.color(ERROR) } else { t }).wrap());
            }
        });
    }

    fn params_form(&mut self, ui: &mut Ui) {
        let mut reset = false;
        egui::Grid::new("script-params").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
            for (decl, (_, value)) in self.header.params.iter().zip(self.values.iter_mut()) {
                let label = ui.label(RichText::new(decl.name.replace('_', " ")).color(theme::TEXT_MUTED));
                let r = match (&decl.kind, value) {
                    (ParamKind::Number { min, max }, ParamValue::Number(v)) => {
                        ui.add(egui::Slider::new(v, *min..=*max).clamping(egui::SliderClamping::Never).max_decimals(4))
                    }
                    (ParamKind::Integer { min, max }, ParamValue::Number(v)) => {
                        ui.add(egui::Slider::new(v, *min as f64..=*max as f64).integer().clamping(egui::SliderClamping::Never))
                    }
                    (ParamKind::Bool, ParamValue::Bool(b)) => ui.checkbox(b, ""),
                    (ParamKind::Choice(options), ParamValue::Text(t)) => {
                        egui::ComboBox::from_id_salt(("script-choice", &decl.name))
                            .selected_text(t.as_str())
                            .show_ui(ui, |ui| {
                                for o in options {
                                    ui.selectable_value(t, o.clone(), o);
                                }
                            })
                            .response
                    }
                    _ => ui.label("?"),
                };
                if !decl.help.is_empty() {
                    label.on_hover_text(&decl.help);
                    r.on_hover_text(&decl.help);
                }
                ui.end_row();
            }
        });
        if ui.small_button("Defaults").on_hover_text("Set all parameters to the defaults of the script").clicked() {
            reset = true;
        }
        if reset {
            self.values = self.header.params.iter().map(|p| (p.name.clone(), p.default.clone())).collect();
        }
    }

    fn confirm_switch(&mut self, ctx: &egui::Context) {
        let Some(target) = self.pending.clone() else { return };
        let mut answer = None;
        egui::Window::new("Unsaved script").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(format!("{} has unsaved changes.", self.name()));
            ui.horizontal(|ui| {
                if ui.button("Discard changes").clicked() {
                    answer = Some(true);
                }
                if ui.button("Keep editing").clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                self.pending = None;
                self.switch_to(target);
            }
            Some(false) => self.pending = None,
            None => {}
        }
    }
}
