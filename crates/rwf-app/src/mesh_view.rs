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

//! Mesh tab of the render workspace (JWildfire's "Mesh Gen"): a 3D flame becomes a triangle
//! mesh. Sampling the voxel volume is the slow step and runs in the background; the surface
//! follows threshold, blur and smoothing changes from the cached volume.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use egui::{Color32, Rect, RichText, Sense, Ui, pos2, vec2};
use rwf_core::Flame;
use rwf_core::meshgen::{self, Bounds, Mesh, MeshSettings, Volume};

use crate::ui::widgets::{self, grid};
use crate::{icons, theme};

const PREVIEW_W: u32 = 900;
const PREVIEW_H: u32 = 640;

/// The settings the surface depends on (besides the volume).
#[derive(Clone, Copy, PartialEq)]
struct SurfaceKey {
    threshold: f64,
    blur: f64,
    smooth_passes: u32,
    smooth_lambda: f64,
    smooth_mu: f64,
}

/// The density a surface was built from, and the surface.
type SurfaceResult = (Arc<Vec<f32>>, Mesh);

struct Sampling {
    cancel: Arc<AtomicBool>,
    progress: Arc<AtomicU64>,
    total: u64,
    started: Instant,
    rx: Receiver<Option<Volume>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Mesh,
    Slices,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Obj,
    Ply,
}

pub struct MeshView {
    flame: Option<Flame>,
    settings: MeshSettings,
    sampling: Option<Sampling>,
    volume: Option<Arc<Volume>>,
    /// Density of the volume for the current blur (slices and surface).
    density: Option<(f64, Arc<Vec<f32>>)>,
    surface: Option<(SurfaceKey, Arc<Mesh>)>,
    surface_job: Option<(SurfaceKey, Receiver<SurfaceResult>)>,
    view: ViewMode,
    yaw: f64,
    pitch: f64,
    zoom: f64,
    slice: usize,
    texture: Option<egui::TextureHandle>,
    /// What the texture shows.
    shown: Option<(usize, ViewMode, u64, u64, u64, usize)>,
    format: Format,
    colors: bool,
    normals: bool,
    status: Option<String>,
}

impl Default for MeshView {
    fn default() -> Self {
        Self {
            flame: None,
            settings: MeshSettings::new(Bounds { min: [-1.0; 3], max: [1.0; 3] }),
            sampling: None,
            volume: None,
            density: None,
            surface: None,
            surface_job: None,
            view: ViewMode::Mesh,
            yaw: 0.6,
            pitch: 0.45,
            zoom: 1.0,
            slice: 0,
            texture: None,
            shown: None,
            format: Format::Obj,
            colors: true,
            normals: true,
            status: None,
        }
    }
}

impl MeshView {
    pub fn is_running(&self) -> bool {
        self.sampling.is_some()
    }

    fn surface_key(&self) -> SurfaceKey {
        let s = &self.settings;
        SurfaceKey { threshold: s.threshold, blur: s.blur, smooth_passes: s.smooth_passes, smooth_lambda: s.smooth_lambda, smooth_mu: s.smooth_mu }
    }

    /// Uses `flame` (frame 0) for the next mesh.
    pub fn load_flame(&mut self, flame: &Flame) {
        self.take_flame(flame);
    }

    fn take_flame(&mut self, flame: &Flame) {
        let mut f = flame.at_frame(0.0);
        f.animation = Default::default();
        self.settings.bounds = meshgen::auto_bounds(&f);
        self.flame = Some(f);
        self.volume = None;
        self.density = None;
        self.surface = None;
        self.status = None;
    }

    fn start_sampling(&mut self) {
        let Some(flame) = self.flame.clone() else { return };
        let settings = self.settings;
        let dims = settings.bounds.dims(settings.resolution);
        let total = (settings.quality * (dims[0] * dims[1] * dims[2]) as f64) as u64;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(AtomicU64::new(0));
        let (tx, rx) = channel();
        let (c2, p2) = (cancel.clone(), progress.clone());
        std::thread::spawn(move || {
            let v = meshgen::sample_volume(&flame, &settings, &p2, &c2);
            let _ = tx.send((!c2.load(Ordering::Relaxed)).then_some(v));
        });
        self.sampling = Some(Sampling { cancel, progress, total, started: Instant::now(), rx });
        self.status = None;
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(s) = &self.sampling {
            match s.rx.try_recv() {
                Ok(v) => {
                    let secs = s.started.elapsed().as_secs_f64();
                    self.sampling = None;
                    match v {
                        Some(v) => {
                            let hits = v.counts.iter().filter(|c| **c > 0).count();
                            self.status = Some(format!("Sampled in {secs:.1} s, {hits} voxels hit"));
                            self.slice = v.dims[2] / 2;
                            self.volume = Some(Arc::new(v));
                            self.density = None;
                            self.surface = None;
                        }
                        None => self.status = Some("Cancelled".into()),
                    }
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        if let Some((key, rx)) = &self.surface_job
            && let Ok((density, mesh)) = rx.try_recv()
        {
            self.density = Some((key.blur, density));
            self.surface = Some((*key, Arc::new(mesh)));
            self.surface_job = None;
        }
        // Re-mesh when the surface settings changed (one job at a time, after the pointer
        // was released).
        let key = self.surface_key();
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        if let Some(volume) = self.volume.clone()
            && self.surface_job.is_none()
            && self.surface.as_ref().is_none_or(|(k, _)| *k != key)
            && !pointer_down
        {
            let (tx, rx) = channel();
            let cached = self.density.as_ref().filter(|(b, _)| *b == key.blur).map(|(_, d)| d.clone());
            std::thread::spawn(move || {
                let density = cached.unwrap_or_else(|| Arc::new(volume.density(key.blur)));
                let mut mesh = meshgen::surface_nets(&volume, &density, key.threshold as f32);
                if key.smooth_passes > 0 {
                    mesh.taubin_smooth(key.smooth_passes, key.smooth_lambda, key.smooth_mu);
                }
                let _ = tx.send((density, mesh));
            });
            self.surface_job = Some((key, rx));
        }
        if self.surface_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    pub fn show(&mut self, ui: &mut Ui, doc: &crate::doc::Document) {
        if self.flame.is_none() {
            self.take_flame(&doc.flame);
        }
        self.poll(ui.ctx());
        egui::Panel::right("mesh-panel")
            .resizable(true)
            .default_size(350.0)
            .size_range(330.0..=520.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().slider_width = 120.0;
                    self.panel(ui, doc);
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::CANVAS).inner_margin(egui::Margin::same(12))).show(ui, |ui| {
            self.preview(ui);
        });
    }

    fn panel(&mut self, ui: &mut Ui, doc: &crate::doc::Document) {
        let running = self.is_running();
        theme::section_label(ui, "Flame");
        ui.label(RichText::new(self.flame.as_ref().map_or("—", |f| if f.name.is_empty() { "Untitled" } else { f.name.as_str() })).color(theme::TEXT));
        if ui.add_enabled(!running, egui::Button::new("From editor")).on_hover_text("Use the flame of the editor (frame 0)").clicked() {
            self.take_flame(&doc.flame);
        }

        theme::section_label(ui, "Volume");
        ui.add_enabled_ui(!running, |ui| {
            grid(ui, "mesh-volume", |ui| {
                let b = &mut self.settings.bounds;
                for (k, axis) in ["x", "y", "z"].iter().enumerate() {
                    widgets::label(ui, &format!("{axis} range"));
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut b.min[k]).speed(0.01).max_decimals(3));
                        ui.label("…");
                        ui.add(egui::DragValue::new(&mut b.max[k]).speed(0.01).max_decimals(3));
                    });
                    ui.end_row();
                }
                widgets::label(ui, "");
                if ui.small_button("Fit to the flame").on_hover_text("The box around the bulk of the points").clicked()
                    && let Some(f) = &self.flame
                {
                    *b = meshgen::auto_bounds(f);
                }
                ui.end_row();
                widgets::label(ui, "Resolution");
                ui.add(egui::Slider::new(&mut self.settings.resolution, 32..=512).logarithmic(true)).on_hover_text("Voxels along the longest side");
                ui.end_row();
                widgets::label(ui, "Quality");
                ui.add(egui::Slider::new(&mut self.settings.quality, 1.0..=500.0).logarithmic(true).max_decimals(0))
                    .on_hover_text("Samples per voxel");
                ui.end_row();
            });
            let d = self.settings.bounds.dims(self.settings.resolution);
            let voxels = d[0] * d[1] * d[2];
            let mb = voxels as f64 * 16.0 / 1e6;
            let text = format!("{} × {} × {} voxels ({mb:.0} MB), {:.0} M samples", d[0], d[1], d[2], self.settings.quality * voxels as f64 / 1e6);
            ui.label(RichText::new(text).small().color(if mb > 2000.0 { theme::WARN } else { theme::TEXT_MUTED }));
        });
        ui.add_space(6.0);
        if let Some(s) = &self.sampling {
            let f = (s.progress.load(Ordering::Relaxed) as f64 / s.total.max(1) as f64).min(1.0) as f32;
            ui.add(egui::ProgressBar::new(f).text(format!("Sampling {:.0} %", f * 100.0)));
            if ui.button(format!("{}  Stop", icons::PAUSE)).clicked() {
                s.cancel.store(true, Ordering::Relaxed);
            }
        } else if ui
            .add_enabled(
                self.flame.is_some(),
                egui::Button::new(RichText::new(format!("{}  Sample volume", icons::PLAY)).strong())
                    .fill(theme::ACCENT_DIM)
                    .min_size(vec2(ui.available_width(), 30.0)),
            )
            .clicked()
        {
            self.start_sampling();
        }

        theme::section_label(ui, "Surface");
        grid(ui, "mesh-surface", |ui| {
            let s = &mut self.settings;
            widgets::label(ui, "Threshold");
            ui.add(egui::Slider::new(&mut s.threshold, 0.01..=0.99)).on_hover_text("Density level of the surface (higher: thinner)");
            ui.end_row();
            widgets::label(ui, "Blur");
            ui.add(egui::Slider::new(&mut s.blur, 0.0..=4.0).suffix(" vx")).on_hover_text("Smooths the density before the surface is extracted");
            ui.end_row();
            widgets::label(ui, "Smoothing");
            ui.add(egui::Slider::new(&mut s.smooth_passes, 0..=50)).on_hover_text("Taubin smoothing passes of the mesh");
            ui.end_row();
            widgets::label(ui, "λ / μ");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut s.smooth_lambda).speed(0.01).range(0.0..=1.0));
                ui.add(egui::DragValue::new(&mut s.smooth_mu).speed(0.01).range(-1.0..=0.0));
            });
            ui.end_row();
        });
        match (&self.surface, &self.surface_job) {
            (_, Some(_)) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Building the surface…").small().color(theme::TEXT_MUTED));
                });
            }
            (Some((_, m)), None) => {
                ui.label(RichText::new(format!("{} vertices, {} triangles", m.positions.len(), m.triangles.len())).small().color(theme::TEXT_MUTED));
            }
            _ => {
                ui.label(RichText::new("Sample the volume first.").small().color(theme::TEXT_MUTED));
            }
        }

        theme::section_label(ui, "Export");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.format, Format::Obj, "OBJ");
            ui.selectable_value(&mut self.format, Format::Ply, "PLY");
            ui.checkbox(&mut self.colors, "Colors");
            ui.add_enabled(self.format == Format::Obj, egui::Checkbox::new(&mut self.normals, "Normals"));
        });
        let ready = self.surface.as_ref().filter(|(k, _)| *k == self.surface_key()).map(|(_, m)| m.clone());
        if ui
            .add_enabled(ready.as_ref().is_some_and(|m| !m.triangles.is_empty()), egui::Button::new(format!("{}  Save mesh…", icons::EXPORT)))
            .clicked()
            && let Some(mesh) = ready
        {
            let ext = if self.format == Format::Obj { "obj" } else { "ply" };
            let name = self.flame.as_ref().filter(|f| !f.name.is_empty()).map_or("mesh".to_string(), |f| f.name.clone());
            if let Some(p) = crate::settings::dialog(crate::settings::Folder::Meshes).add_filter(ext.to_uppercase(), &[ext]).set_file_name(format!("{name}.{ext}")).save_file() {
                let p: PathBuf = p.with_extension(ext);
                let mut m = (*mesh).clone();
                if !self.colors {
                    m.colors.clear();
                }
                self.status = Some(match m.save(&p, self.colors, self.normals) {
                    Ok(()) => format!("Saved {}", p.display()),
                    Err(e) => format!("Cannot save: {e}"),
                });
            }
        }
        if let Some(s) = &self.status {
            ui.add_space(6.0);
            ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
        }
    }

    fn preview(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, ViewMode::Mesh, "Mesh");
            ui.selectable_value(&mut self.view, ViewMode::Slices, "Slices");
            if let (ViewMode::Slices, Some(v)) = (self.view, &self.volume) {
                ui.separator();
                ui.spacing_mut().slider_width = 260.0;
                ui.add(egui::Slider::new(&mut self.slice, 0..=v.dims[2] - 1).text("z slice"));
            }
            if self.view == ViewMode::Mesh && ui.small_button("Reset view").clicked() {
                (self.yaw, self.pitch, self.zoom) = (0.6, 0.45, 1.0);
            }
        });
        let rect = Rect::from_min_size(ui.cursor().min, ui.available_size());
        let resp = ui.allocate_rect(rect, Sense::drag());
        if self.view == ViewMode::Mesh {
            if resp.dragged() {
                let d = resp.drag_delta();
                self.yaw += d.x as f64 * 0.01;
                self.pitch = (self.pitch + d.y as f64 * 0.01).clamp(-1.55, 1.55);
            }
            if resp.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    self.zoom = (self.zoom * (scroll as f64 * 0.002).exp()).clamp(0.1, 20.0);
                }
            }
        }
        let painter = ui.painter_at(rect);
        // Image of the current view (rendered when something changed; smaller while dragging).
        let (w, h, key) = match self.view {
            ViewMode::Mesh => {
                let Some((_, mesh)) = &self.surface else {
                    let msg = if self.is_running() { "Sampling the volume…" } else { "Sample the volume to see the mesh" };
                    painter.text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(15.0), theme::TEXT_MUTED);
                    return;
                };
                let s = if resp.dragged() { 2 } else { 1 };
                let key = (Arc::as_ptr(mesh) as usize, ViewMode::Mesh, self.yaw.to_bits(), self.pitch.to_bits(), self.zoom.to_bits(), s);
                (PREVIEW_W / s as u32, PREVIEW_H / s as u32, key)
            }
            ViewMode::Slices => {
                let (Some(v), Some((_, d))) = (&self.volume, &self.density) else {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Sample the volume to see its slices",
                        egui::FontId::proportional(15.0),
                        theme::TEXT_MUTED,
                    );
                    return;
                };
                let key = (Arc::as_ptr(d) as usize, ViewMode::Slices, self.slice as u64, self.settings.threshold.to_bits(), 0, 0);
                (v.dims[0] as u32, v.dims[1] as u32, key)
            }
        };
        if self.shown != Some(key) {
            let img = match self.view {
                ViewMode::Mesh => meshgen::render_preview(&self.surface.as_ref().expect("checked").1, self.yaw, self.pitch, self.zoom, w, h),
                ViewMode::Slices => {
                    let (v, d) = (self.volume.as_ref().expect("checked"), &self.density.as_ref().expect("checked").1);
                    v.slice_rgba(d, self.slice, self.settings.threshold)
                }
            };
            let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img);
            let opts = if self.view == ViewMode::Slices { egui::TextureOptions::NEAREST } else { egui::TextureOptions::LINEAR };
            match &mut self.texture {
                Some(t) => t.set(color, opts),
                None => self.texture = Some(ui.ctx().load_texture("mesh-preview", color, opts)),
            }
            self.shown = Some(key);
        }
        if let Some(t) = &self.texture {
            let size = vec2(w as f32, h as f32);
            let scale = (rect.width() / size.x).min(rect.height() / size.y);
            let img = Rect::from_center_size(rect.center(), size * scale);
            painter.image(t.id(), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        if self.view == ViewMode::Mesh {
            painter.text(
                rect.left_bottom() + vec2(4.0, -4.0),
                egui::Align2::LEFT_BOTTOM,
                "Drag: turn · wheel: zoom",
                egui::FontId::proportional(11.0),
                theme::TEXT_MUTED,
            );
        }
    }
}
