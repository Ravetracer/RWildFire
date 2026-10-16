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

//! "Denoise image" window (JWildfire's "denoise external image"): runs the AI denoiser on an
//! image file and saves the result.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use egui::RichText;
use rwf_core::render::denoise::{self, Denoiser};

use crate::theme;
use crate::ui::widgets;

pub struct ImageDenoiser {
    pub open: bool,
    source: Option<PathBuf>,
    denoiser: Denoiser,
    blend: f64,
    running: Option<Receiver<Result<String, String>>>,
    status: Option<(String, bool)>,
}

impl Default for ImageDenoiser {
    fn default() -> Self {
        Self { open: false, source: None, denoiser: Denoiser::Oidn, blend: 0.0, running: None, status: None }
    }
}

impl ImageDenoiser {
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        if let Some(rx) = &self.running {
            match rx.try_recv() {
                Ok(r) => {
                    self.running = None;
                    self.status = Some(match r {
                        Ok(msg) => (msg, false),
                        Err(e) => (e, true),
                    });
                }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        let mut open = self.open;
        egui::Window::new("Denoise an image").open(&mut open).default_width(380.0).resizable(false).collapsible(false).show(ctx, |ui| {
            ui.label(RichText::new("Removes the grain of any image (PNG, JPEG) with Open Image Denoise and saves a copy.").small().color(theme::TEXT_MUTED));
            ui.add_space(4.0);
            let busy = self.running.is_some();
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Image…").clicked()
                        && let Some(p) = crate::settings::dialog(crate::settings::Folder::Images).add_filter("Image", &["png", "jpg", "jpeg"]).pick_file()
                    {
                        self.source = Some(p);
                        self.status = None;
                    }
                    let name = self.source.as_ref().map_or("No image chosen".to_string(), |p| p.display().to_string());
                    ui.add(egui::Label::new(RichText::new(name).small().color(theme::TEXT_MUTED)).truncate());
                });
                widgets::grid(ui, "image-denoise", |ui| {
                    widgets::label(ui, "Denoiser");
                    egui::ComboBox::from_id_salt("image-denoiser").selected_text(self.denoiser.label()).show_ui(ui, |ui| {
                        for d in Denoiser::ALL.iter().filter(|d| **d != Denoiser::None) {
                            ui.selectable_value(&mut self.denoiser, *d, d.label());
                        }
                    });
                    ui.end_row();
                    widgets::slider(ui, "Blend", &mut self.blend, 0.0..=0.99, 0.0).on_hover_text("Mixes the original image back in (0 = fully denoised)");
                });
                if let Err(e) = denoise::backend() {
                    ui.label(RichText::new(e).small().color(theme::WARN));
                }
                if ui.add_enabled(self.source.is_some(), egui::Button::new("Denoise and save…")).clicked()
                    && let Some(src) = self.source.clone()
                {
                    let stem = src.file_stem().map_or("image".into(), |s| s.to_string_lossy().to_string());
                    let dir = src.parent().map(|p| p.to_path_buf());
                    let mut dlg = rfd::FileDialog::new().add_filter("PNG", &["png"]).add_filter("JPEG", &["jpg", "jpeg"]).set_file_name(format!("{stem}_denoised.png"));
                    if let Some(d) = dir {
                        dlg = dlg.set_directory(d);
                    }
                    if let Some(dst) = dlg.save_file() {
                        let (denoiser, blend) = (self.denoiser, self.blend);
                        let (tx, rx) = channel();
                        std::thread::spawn(move || {
                            let r = denoise::denoise_image_file(&src, &dst, denoiser, blend).map(|o| format!("Saved {} ({})", dst.display(), o.describe()));
                            let _ = tx.send(r);
                        });
                        self.running = Some(rx);
                        self.status = None;
                    }
                }
            });
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Denoising…");
                });
            }
            if let Some((msg, error)) = &self.status {
                ui.label(RichText::new(msg).small().color(if *error { theme::WARN } else { theme::TEXT_MUTED }));
            }
        });
        self.open = open;
    }
}
