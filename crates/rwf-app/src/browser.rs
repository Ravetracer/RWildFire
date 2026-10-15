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

//! Flame browser workspace (JWildfire's flame browser): the `.flame` files of a folder and its
//! sub-folders, grouped by month and day of their last change, as thumbnails rendered in
//! background threads; open, send to the batch renderer or the mesh generator, rename, copy,
//! move and delete.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Ui, vec2};

use crate::{icons, theme};

const THUMB_W: u32 = 192;
const THUMB_H: u32 = 120;
const LABEL_H: f32 = 18.0;
const GAP: f32 = 10.0;
/// Thumbnail render threads (each renders with two threads).
const WORKERS: usize = 3;
/// Thumbnails kept as textures; the least recently shown ones are dropped beyond this.
const CACHE: usize = 600;

/// What the browser asks the app to do with a file.
pub enum BrowserAction {
    Open(PathBuf),
    ToBatch(PathBuf),
    ToMesh(PathBuf),
    /// A new folder was chosen (remembered in the preferences).
    FolderChosen(PathBuf),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    None,
    Copied,
    Moved,
    Deleted,
}

struct Entry {
    path: PathBuf,
    modified: SystemTime,
    /// `YYYY-MM-DD HH:MM` of `modified`.
    date: String,
    mark: Mark,
}

impl Entry {
    fn name(&self) -> String {
        self.path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
    }
    fn day(&self) -> &str {
        &self.date[..10]
    }
    fn month(&self) -> &str {
        &self.date[..7]
    }
    /// Still in place (not moved or deleted).
    fn present(&self) -> bool {
        matches!(self.mark, Mark::None | Mark::Copied)
    }
}

#[derive(Clone, PartialEq, Eq)]
enum Group {
    All,
    Month(String),
    Day(String),
}

enum Thumb {
    Pending,
    Done { texture: egui::TextureHandle, flames: usize },
    Failed(String),
}

struct Cached {
    modified: SystemTime,
    thumb: Thumb,
    used: u64,
}

type Job = (PathBuf, SystemTime);
type JobResult = (PathBuf, SystemTime, Result<(Vec<u8>, usize), String>);

/// Jobs for the render threads; the last one is taken first.
#[derive(Default)]
struct Queue {
    jobs: Mutex<Vec<Job>>,
    ready: Condvar,
}

pub struct FlameBrowser {
    pub open: bool,
    root: Option<PathBuf>,
    entries: Vec<Entry>,
    scan: Option<Receiver<Vec<Entry>>>,
    group: Group,
    expanded: HashSet<String>,
    filter: String,
    /// Index into `entries`.
    selected: Option<usize>,
    /// The selection moved by keyboard: scroll it into view.
    scroll_to_selected: bool,
    cache: HashMap<PathBuf, Cached>,
    frame: u64,
    queue: Arc<Queue>,
    workers_started: bool,
    tx: Sender<JobResult>,
    rx: Receiver<JobResult>,
    rename: String,
    confirm_delete: bool,
    last_dir: Option<PathBuf>,
    /// Outcome of the last file operation (text, is error).
    status: Option<(String, bool)>,
}

impl Default for FlameBrowser {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            open: false,
            root: None,
            entries: Vec::new(),
            scan: None,
            group: Group::All,
            expanded: HashSet::new(),
            filter: String::new(),
            selected: None,
            scroll_to_selected: false,
            cache: HashMap::new(),
            frame: 0,
            queue: Arc::default(),
            workers_started: false,
            tx,
            rx,
            rename: String::new(),
            confirm_delete: false,
            last_dir: None,
            status: None,
        }
    }
}

fn date_of(t: SystemTime) -> String {
    crate::prefs::date_time(t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()))
}

/// All `.flame` files below `dir` (symbolic links to folders are not followed), newest first.
fn scan_folder(dir: &Path) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(list) = std::fs::read_dir(&d) else { continue };
        for e in list.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let path = e.path();
            if ft.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("flame")) {
                let modified = e.metadata().and_then(|m| m.modified()).unwrap_or(UNIX_EPOCH);
                entries.push(Entry { date: date_of(modified), path, modified, mark: Mark::None });
            }
        }
    }
    entries.sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.name().cmp(&b.name())));
    entries
}

/// The first flame of a file as a thumbnail, and the number of flames in the file.
fn render_file(path: &Path) -> Result<(Vec<u8>, usize), String> {
    let flames = rwf_core::io::read_flames_from_file(path).map_err(|e| e.to_string())?;
    let first = flames.first().ok_or("no flame in the file")?;
    let mut f = first.at_frame(0.0);
    f.bg_transparency = false;
    let opts = rwf_core::render::RenderOptions { width: THUMB_W, height: THUMB_H, threads: 2, max_density: Some(60.0), seed: 3, zbuffer: false };
    let img = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rwf_core::render::CpuRender::start(&f, opts).wait().image(false)))
        .map_err(|_| "the renderer failed on this flame".to_string())?;
    Ok((img, flames.len()))
}

fn worker(queue: Arc<Queue>, tx: Sender<JobResult>, ctx: egui::Context) {
    loop {
        let (path, modified) = {
            let mut jobs = queue.jobs.lock().unwrap();
            loop {
                if let Some(j) = jobs.pop() {
                    break j;
                }
                jobs = queue.ready.wait(jobs).unwrap();
            }
        };
        let res = render_file(&path);
        if tx.send((path, modified, res)).is_err() {
            return;
        }
        ctx.request_repaint();
    }
}

/// Moves a file, also across file systems.
fn move_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst)?;
    std::fs::remove_file(src)
}

impl FlameBrowser {
    /// Opens the workspace; scans `dir` (the remembered folder) if no folder is shown yet.
    pub fn open_with(&mut self, dir: &str) {
        self.open = true;
        if self.root.is_none() && !dir.trim().is_empty() {
            let p = PathBuf::from(dir.trim());
            if p.is_dir() {
                self.set_root(p);
            }
        }
    }

    pub fn set_root(&mut self, dir: PathBuf) {
        self.root = Some(dir);
        self.group = Group::All;
        self.refresh();
    }

    fn refresh(&mut self) {
        let Some(root) = self.root.clone() else { return };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(scan_folder(&root));
        });
        self.scan = Some(rx);
        self.status = None;
        self.confirm_delete = false;
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.selected.and_then(|i| self.entries.get(i))
    }

    fn select(&mut self, i: Option<usize>) {
        if i != self.selected {
            self.selected = i;
            self.rename = self.selected_entry().map(Entry::name).unwrap_or_default();
            self.confirm_delete = false;
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.scan {
            match rx.try_recv() {
                Ok(entries) => {
                    let path = self.selected_entry().map(|e| e.path.clone());
                    self.entries = entries;
                    self.scan = None;
                    self.selected = None;
                    self.select(path.and_then(|p| self.entries.iter().position(|e| e.path == p)));
                    if let Some(month) = self.entries.first().map(|e| e.month().to_string()) {
                        self.expanded.insert(month);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.scan = None,
            }
        }
        while let Ok((path, modified, res)) = self.rx.try_recv() {
            let Some(c) = self.cache.get_mut(&path).filter(|c| c.modified == modified) else { continue };
            c.thumb = match res {
                Ok((img, flames)) => {
                    let color = egui::ColorImage::from_rgba_unmultiplied([THUMB_W as usize, THUMB_H as usize], &img);
                    let texture = ctx.load_texture(format!("browser-{}", path.display()), color, egui::TextureOptions::LINEAR);
                    Thumb::Done { texture, flames }
                }
                Err(e) => Thumb::Failed(e),
            };
        }
        // Queued jobs not started yet are dropped; the visible cells queue theirs again.
        let dropped: Vec<Job> = std::mem::take(&mut *self.queue.jobs.lock().unwrap());
        for (p, _) in dropped {
            if self.cache.get(&p).is_some_and(|c| matches!(c.thumb, Thumb::Pending)) {
                self.cache.remove(&p);
            }
        }
        if self.cache.len() > CACHE {
            let mut used: Vec<(u64, PathBuf)> = self.cache.iter().map(|(p, c)| (c.used, p.clone())).collect();
            used.sort_unstable();
            for (_, p) in used.into_iter().take(self.cache.len() - CACHE * 4 / 5) {
                self.cache.remove(&p);
            }
        }
    }

    /// The thumbnail of an entry; requests it if missing or outdated.
    fn thumb(&mut self, i: usize, wanted: &mut Vec<Job>) -> Option<&Thumb> {
        let e = &self.entries[i];
        let frame = self.frame;
        let stale = self.cache.get(&e.path).is_none_or(|c| c.modified != e.modified);
        if stale && e.present() {
            self.cache.insert(e.path.clone(), Cached { modified: e.modified, thumb: Thumb::Pending, used: frame });
            wanted.push((e.path.clone(), e.modified));
        }
        self.cache.get_mut(&self.entries[i].path).map(|c| {
            c.used = frame;
            &c.thumb
        })
    }

    fn visible(&self) -> Vec<usize> {
        let filter = self.filter.trim().to_lowercase();
        (0..self.entries.len())
            .filter(|&i| {
                let e = &self.entries[i];
                let in_group = match &self.group {
                    Group::All => true,
                    Group::Month(m) => e.month() == m,
                    Group::Day(d) => e.day() == d,
                };
                in_group && (filter.is_empty() || e.name().to_lowercase().contains(&filter))
            })
            .collect()
    }

    /// Shows the workspace; returns what the app should do.
    pub fn show(&mut self, ui: &mut Ui) -> Option<BrowserAction> {
        let ctx = ui.ctx().clone();
        if !self.workers_started {
            self.workers_started = true;
            for _ in 0..WORKERS {
                let (q, tx, c) = (self.queue.clone(), self.tx.clone(), ctx.clone());
                std::thread::spawn(move || worker(q, tx, c));
            }
        }
        self.frame += 1;
        self.poll(&ctx);
        let mut action = None;

        egui::Panel::left("browser-tree")
            .resizable(true)
            .default_size(210.0)
            .size_range(170.0..=360.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(10, 8)))
            .show(ui, |ui| self.tree(ui, &mut action));
        egui::Panel::right("browser-details")
            .resizable(false)
            .exact_size(300.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.details(ui, &mut action));
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::CANVAS).inner_margin(egui::Margin::same(12))).show(ui, |ui| {
            self.grid(ui, &mut action);
        });
        action
    }

    fn tree(&mut self, ui: &mut Ui, action: &mut Option<BrowserAction>) {
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Folder…", icons::FOLDER_OPEN)).on_hover_text("Choose the folder to browse; its sub-folders are included").clicked() {
                let mut dlg = rfd::FileDialog::new();
                if let Some(r) = &self.root {
                    dlg = dlg.set_directory(r);
                }
                if let Some(p) = dlg.pick_folder() {
                    self.set_root(p.clone());
                    *action = Some(BrowserAction::FolderChosen(p));
                }
            }
            ui.add_enabled_ui(self.root.is_some() && self.scan.is_none(), |ui| {
                if crate::ui::widgets::icon_button(ui, icons::REFRESH, "Scan the folder again").clicked() {
                    self.refresh();
                }
            });
            if self.scan.is_some() {
                ui.spinner();
            }
        });
        if let Some(r) = &self.root {
            ui.label(RichText::new(r.display().to_string()).small().color(theme::TEXT_MUTED)).on_hover_text(r.display().to_string());
        }
        ui.add_space(6.0);
        theme::section_label(ui, "Changed");
        // month → days, newest first (entries are sorted)
        let mut months: Vec<(String, Vec<(String, usize)>, usize)> = Vec::new();
        for e in &self.entries {
            if months.last().is_none_or(|m| m.0 != e.month()) {
                months.push((e.month().to_string(), Vec::new(), 0));
            }
            let m = months.last_mut().unwrap();
            m.2 += 1;
            if m.1.last().is_none_or(|d| d.0 != e.day()) {
                m.1.push((e.day().to_string(), 0));
            }
            m.1.last_mut().unwrap().1 += 1;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.selectable_value(&mut self.group, Group::All, format!("All flames ({})", self.entries.len()));
            for (month, days, n) in months {
                let open = self.expanded.contains(&month);
                ui.horizontal(|ui| {
                    let caret = if open { icons::CARET_DOWN } else { icons::CARET_RIGHT };
                    if ui.add(egui::Button::new(caret).frame(false)).clicked() {
                        if open {
                            self.expanded.remove(&month);
                        } else {
                            self.expanded.insert(month.clone());
                        }
                    }
                    ui.selectable_value(&mut self.group, Group::Month(month.clone()), format!("{month} ({n})"));
                });
                if open {
                    for (day, n) in days {
                        ui.horizontal(|ui| {
                            ui.add_space(28.0);
                            ui.selectable_value(&mut self.group, Group::Day(day.clone()), format!("{day} ({n})"));
                        });
                    }
                }
            }
        });
    }

    fn grid(&mut self, ui: &mut Ui, action: &mut Option<BrowserAction>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icons::MAGNIFYING_GLASS).color(theme::TEXT_MUTED));
            ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter by name").desired_width(220.0));
        });
        ui.add_space(8.0);
        if self.root.is_none() {
            ui.label(RichText::new("Choose a folder (Folder… on the left) to see its flames.").color(theme::TEXT_MUTED));
            return;
        }
        let visible = self.visible();
        if visible.is_empty() {
            let msg = if self.scan.is_some() {
                "Scanning…"
            } else if self.entries.is_empty() {
                "No .flame files in this folder or its sub-folders."
            } else {
                "No flame matches."
            };
            ui.label(RichText::new(msg).color(theme::TEXT_MUTED));
            return;
        }
        let cell = vec2(THUMB_W as f32, THUMB_H as f32 + LABEL_H);
        let cols = (((ui.available_width() + GAP) / (cell.x + GAP)).floor() as usize).max(1);
        let rows = visible.len().div_ceil(cols);

        // keyboard: arrows move the selection, Enter opens, Delete asks to delete
        let typing = ctx_typing(ui.ctx());
        if !typing {
            let pos = self.selected.and_then(|s| visible.iter().position(|&i| i == s));
            let (l, r, u, d, enter, del) = ui.input(|i| {
                use egui::Key::*;
                (i.key_pressed(ArrowLeft), i.key_pressed(ArrowRight), i.key_pressed(ArrowUp), i.key_pressed(ArrowDown), i.key_pressed(Enter), i.key_pressed(Delete))
            });
            let step: isize = if l { -1 } else if r { 1 } else if u { -(cols as isize) } else if d { cols as isize } else { 0 };
            if step != 0 {
                let next = match pos {
                    Some(p) => (p as isize + step).clamp(0, visible.len() as isize - 1) as usize,
                    None => 0,
                };
                self.select(Some(visible[next]));
                self.scroll_to_selected = true;
            }
            if let Some(e) = self.selected_entry().filter(|e| e.present()) {
                if enter {
                    *action = Some(BrowserAction::Open(e.path.clone()));
                }
                if del {
                    self.confirm_delete = true;
                }
            }
        }

        let row_h = cell.y + GAP;
        let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
        if std::mem::take(&mut self.scroll_to_selected)
            && let Some(row) = self.selected.and_then(|s| visible.iter().position(|&i| i == s)).map(|p| p / cols)
        {
            let id = ui.make_persistent_id("browser-grid");
            let (offset, height) = ui.data(|d| d.get_temp::<(f32, f32)>(id)).unwrap_or((0.0, 0.0));
            let top = row as f32 * row_h;
            if top < offset {
                area = area.vertical_scroll_offset(top);
            } else if top + cell.y > offset + height {
                area = area.vertical_scroll_offset(top + cell.y - height);
            }
        }
        let mut wanted = Vec::new();
        let out = area.show_rows(ui, cell.y, rows, |ui, range| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            for r in range {
                ui.horizontal(|ui| {
                    for &i in visible.iter().skip(r * cols).take(cols) {
                        self.cell(ui, i, &mut wanted, action);
                    }
                });
            }
        });
        let id = ui.make_persistent_id("browser-grid");
        ui.data_mut(|d| d.insert_temp(id, (out.state.offset.y, out.inner_rect.height())));
        if !wanted.is_empty() {
            // first visible cell last: it is taken first
            wanted.reverse();
            *self.queue.jobs.lock().unwrap() = wanted;
            self.queue.ready.notify_all();
        }
    }

    fn cell(&mut self, ui: &mut Ui, i: usize, wanted: &mut Vec<Job>, action: &mut Option<BrowserAction>) {
        let size = vec2(THUMB_W as f32, THUMB_H as f32 + LABEL_H);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let img_rect = egui::Rect::from_min_size(rect.min, vec2(THUMB_W as f32, THUMB_H as f32));
        let selected = self.selected == Some(i);
        let (name, mark, present) = {
            let e = &self.entries[i];
            (e.name(), e.mark, e.present())
        };
        let p = ui.painter().clone();
        p.rect_filled(img_rect, CornerRadius::same(6), theme::BG);
        let mut tip = None;
        match self.thumb(i, wanted) {
            Some(Thumb::Done { texture, flames }) => {
                p.image(texture.id(), img_rect.shrink(1.0), egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                if *flames > 1 {
                    tip = Some(format!("{flames} flames in the file; opens all of them (Explore › File)"));
                    let badge = egui::Rect::from_min_size(img_rect.right_top() + vec2(-34.0, 4.0), vec2(30.0, 16.0));
                    p.rect_filled(badge, CornerRadius::same(4), Color32::from_black_alpha(180));
                    p.text(badge.center(), egui::Align2::CENTER_CENTER, format!("×{flames}"), egui::FontId::proportional(11.0), theme::TEXT);
                }
            }
            Some(Thumb::Failed(e)) => {
                tip = Some(format!("Could not render: {e}"));
                if present {
                    p.text(img_rect.center(), egui::Align2::CENTER_CENTER, icons::WARNING, egui::FontId::proportional(22.0), theme::WARN);
                }
            }
            _ => {
                p.text(img_rect.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(18.0), theme::TEXT_MUTED);
            }
        }
        let overlay = match mark {
            Mark::None => None,
            Mark::Copied => Some(("copied", theme::TEXT)),
            Mark::Moved => Some(("moved", theme::WARN)),
            Mark::Deleted => Some(("deleted", theme::WARN)),
        };
        if let Some((text, color)) = overlay {
            p.rect_filled(img_rect, CornerRadius::same(6), Color32::from_black_alpha(if present { 90 } else { 170 }));
            p.text(img_rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(18.0), color);
        }
        let stroke = if selected {
            Stroke::new(2.0, theme::ACCENT)
        } else if resp.hovered() {
            Stroke::new(1.5, theme::TEXT_MUTED)
        } else {
            Stroke::new(1.0, theme::BORDER)
        };
        p.rect_stroke(img_rect, CornerRadius::same(6), stroke, egui::StrokeKind::Inside);
        let mut job = egui::text::LayoutJob::single_section(
            name,
            egui::TextFormat::simple(egui::FontId::proportional(12.0), if selected { theme::TEXT } else { theme::TEXT_MUTED }),
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(THUMB_W as f32);
        let galley = p.layout_job(job);
        let label_pos = egui::pos2(rect.center().x - galley.size().x / 2.0, img_rect.bottom() + (LABEL_H - galley.size().y) / 2.0 + 1.0);
        p.galley(label_pos, galley, theme::TEXT);

        let e = &self.entries[i];
        let mut hover = format!("{}\nChanged {} UTC", e.path.display(), e.date);
        if let Some(t) = tip {
            hover += &format!("\n{t}");
        }
        if present {
            hover += "\nDouble-click: open in the editor · right-click: more";
        }
        let resp = resp.on_hover_text(hover);
        if resp.clicked() || resp.secondary_clicked() {
            self.select(Some(i));
        }
        if present {
            if resp.double_clicked() {
                *action = Some(BrowserAction::Open(self.entries[i].path.clone()));
            }
            resp.context_menu(|ui| {
                let path = self.entries[i].path.clone();
                if ui.button("Open in the editor").clicked() {
                    *action = Some(BrowserAction::Open(path.clone()));
                }
                if ui.button("To the batch renderer").clicked() {
                    *action = Some(BrowserAction::ToBatch(path.clone()));
                }
                if ui.button("To the mesh generator").clicked() {
                    *action = Some(BrowserAction::ToMesh(path));
                }
            });
        }
    }

    fn details(&mut self, ui: &mut Ui, action: &mut Option<BrowserAction>) {
        theme::section_label(ui, "Flame");
        let Some(i) = self.selected.filter(|&i| i < self.entries.len()) else {
            ui.label(RichText::new("Click a thumbnail to select it.").color(theme::TEXT_MUTED));
            return;
        };
        let (path, date, present) = {
            let e = &self.entries[i];
            (e.path.clone(), e.date.clone(), e.present())
        };
        let w = ui.available_width();
        match self.cache.get(&path).map(|c| &c.thumb) {
            Some(Thumb::Done { texture, flames }) => {
                ui.add(egui::Image::new(texture).fit_to_exact_size(vec2(w, w * THUMB_H as f32 / THUMB_W as f32)).corner_radius(6));
                if *flames > 1 {
                    ui.label(RichText::new(format!("{flames} flames in the file")).color(theme::TEXT_MUTED));
                }
            }
            Some(Thumb::Failed(e)) => {
                ui.label(RichText::new(format!("Could not render: {e}")).color(theme::WARN));
            }
            _ => {}
        }
        ui.label(RichText::new(self.entries[i].name()).strong().color(theme::TEXT));
        if let Some(dir) = path.parent() {
            ui.label(RichText::new(dir.display().to_string()).small().color(theme::TEXT_MUTED));
        }
        ui.label(RichText::new(format!("Changed {date} UTC")).small().color(theme::TEXT_MUTED));
        ui.add_space(8.0);

        ui.add_enabled_ui(present, |ui| {
            let full = |ui: &mut Ui, text: &str| ui.add_sized([w, 26.0], egui::Button::new(text));
            if full(ui, "Open in the editor").on_hover_text("Loads the flame (all flames of the file, in Explore › File). Double-click or Enter does the same.").clicked() {
                *action = Some(BrowserAction::Open(path.clone()));
            }
            if full(ui, "To the batch renderer").on_hover_text("Adds the flames of the file to the batch renderer's queue").clicked() {
                *action = Some(BrowserAction::ToBatch(path.clone()));
            }
            if full(ui, "To the mesh generator").on_hover_text("Uses the (first) flame in the mesh generator").clicked() {
                *action = Some(BrowserAction::ToMesh(path.clone()));
            }

            theme::section_label(ui, "File");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.rename).desired_width(w - 80.0));
                if ui.button("Rename").on_hover_text("Renames the file (in its folder)").clicked() {
                    self.rename_selected(i);
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Copy to…").on_hover_text("Copies the file to another folder or name").clicked() {
                    self.transfer(i, false);
                }
                if ui.button("Move to…").on_hover_text("Moves the file to another folder or name").clicked() {
                    self.transfer(i, true);
                }
                if ui.button("Delete…").on_hover_text("Deletes the file permanently (Delete key)").clicked() {
                    self.confirm_delete = true;
                }
            });
            if self.confirm_delete {
                egui::Frame::new().fill(theme::SURFACE).corner_radius(6).inner_margin(8).show(ui, |ui| {
                    ui.label(RichText::new("Delete this file permanently? This cannot be undone.").color(theme::WARN));
                    ui.horizontal(|ui| {
                        if ui.button("Delete").clicked() {
                            self.delete(i);
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete = false;
                        }
                    });
                });
            }
        });
        if let Some((msg, error)) = &self.status {
            ui.add_space(6.0);
            ui.label(RichText::new(msg).small().color(if *error { theme::WARN } else { theme::TEXT_MUTED }));
        }
    }

    fn rename_selected(&mut self, i: usize) {
        let new = self.rename.trim().trim_end_matches(".flame").to_string();
        let old = self.entries[i].path.clone();
        let res = if new.is_empty() || new.contains(['/', '\\']) {
            Err("Enter a file name without folder".to_string())
        } else {
            let dst = old.with_file_name(format!("{new}.flame"));
            if dst == old {
                return;
            }
            if dst.exists() {
                Err(format!("{} already exists", dst.display()))
            } else {
                std::fs::rename(&old, &dst).map(|_| dst).map_err(|e| format!("Rename failed: {e}"))
            }
        };
        match res {
            Ok(dst) => {
                if let Some(c) = self.cache.remove(&old) {
                    self.cache.insert(dst.clone(), c);
                }
                self.status = Some((format!("Renamed to {}", dst.file_name().unwrap_or_default().to_string_lossy()), false));
                self.entries[i].path = dst;
            }
            Err(e) => self.status = Some((e, true)),
        }
    }

    fn transfer(&mut self, i: usize, moving: bool) {
        let src = self.entries[i].path.clone();
        let dir = self.last_dir.clone().or_else(|| src.parent().map(Path::to_path_buf));
        let mut dlg = rfd::FileDialog::new().add_filter("Flame", &["flame"]).set_file_name(src.file_name().unwrap_or_default().to_string_lossy());
        if let Some(d) = dir {
            dlg = dlg.set_directory(d);
        }
        let Some(dst) = dlg.save_file() else { return };
        if dst == src {
            self.status = Some(("Source and destination are the same file".to_string(), true));
            return;
        }
        self.last_dir = dst.parent().map(Path::to_path_buf);
        let res = if moving { move_file(&src, &dst) } else { std::fs::copy(&src, &dst).map(|_| ()) };
        match res {
            Ok(()) => {
                self.entries[i].mark = if moving { Mark::Moved } else { Mark::Copied };
                self.status = Some((format!("{} to {}", if moving { "Moved" } else { "Copied" }, dst.display()), false));
            }
            Err(e) => self.status = Some((format!("{} failed: {e}", if moving { "Move" } else { "Copy" }), true)),
        }
    }

    fn delete(&mut self, i: usize) {
        self.confirm_delete = false;
        let path = self.entries[i].path.clone();
        match std::fs::remove_file(&path) {
            Ok(()) => {
                self.entries[i].mark = Mark::Deleted;
                self.status = Some((format!("Deleted {}", path.display()), false));
            }
            Err(e) => self.status = Some((format!("Delete failed: {e}"), true)),
        }
    }
}

/// A text field has the keyboard.
fn ctx_typing(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.focused().is_some()) && ctx.egui_wants_keyboard_input()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_finds_flames_recursively_newest_first() {
        let dir = std::env::temp_dir().join(format!("rwf-browser-test-{}", std::process::id()));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(dir.join("a.flame"), "x").unwrap();
        std::fs::write(sub.join("b.FLAME"), "x").unwrap();
        std::fs::write(dir.join("c.txt"), "x").unwrap();
        let old = SystemTime::now() - std::time::Duration::from_secs(86_400 * 40);
        std::fs::File::options().write(true).open(dir.join("a.flame")).unwrap().set_modified(old).unwrap();
        let entries = scan_folder(&dir);
        let names: Vec<String> = entries.iter().map(Entry::name).collect();
        assert_eq!(names, ["b", "a"]);
        assert_eq!(entries[1].day(), &date_of(old)[..10]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn date_time_formats_utc() {
        assert_eq!(crate::prefs::date_time(0), "1970-01-01 00:00");
        assert_eq!(crate::prefs::date_time(1_760_531_400), "2025-10-15 12:30");
    }
}
