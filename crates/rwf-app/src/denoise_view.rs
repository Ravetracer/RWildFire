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

//! "Show denoised": runs the flame's post denoiser on a read-back image on a background
//! thread and shows the result with what the denoiser did (or why it did nothing).

use std::hash::{Hash, Hasher};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use egui::{Color32, FontId, Painter, Pos2, Rect, vec2};
use rwf_core::Flame;
use rwf_core::render::denoise::{DenoiseAux, DenoiseOutcome, apply_post_denoiser_from};

use crate::theme;

type JobResult = Result<(DenoiseOutcome, Vec<u8>, Duration), String>;

#[derive(Default)]
pub struct DenoisedView {
    job: Option<(u64, Receiver<JobResult>)>,
    /// Key of the image shown.
    shown: Option<u64>,
    texture: Option<egui::TextureHandle>,
    size: (u32, u32),
    outcome: Option<Result<(DenoiseOutcome, Duration), String>>,
    last_start: Option<Instant>,
}

/// A key for an image `revision` with the denoiser settings of `flame`.
pub fn key(revision: u64, flame: &Flame, gpu_render: bool) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    revision.hash(&mut h);
    flame.ai_post_denoiser.as_str().hash(&mut h);
    flame.post_denoiser_blend.to_bits().hash(&mut h);
    flame.post_denoiser_only_for_cpu.hash(&mut h);
    gpu_render.hash(&mut h);
    h.finish()
}

impl DenoisedView {
    /// Whether the image with `key` should be denoised now (not shown or running yet, and the
    /// last run started at least `min_interval` ago).
    pub fn wants(&self, key: u64, min_interval: Duration) -> bool {
        let current = self.job.as_ref().map(|(k, _)| *k).or(self.shown);
        self.job.is_none() && current != Some(key) && self.last_start.is_none_or(|t| t.elapsed() >= min_interval)
    }

    /// Denoises `rgba` in the background with the settings of `flame`; the denoiser reads
    /// `source` (the unfiltered image) and the renderer's helper images, if given.
    #[allow(clippy::too_many_arguments)]
    pub fn start(&mut self, key: u64, flame: &Flame, mut rgba: Vec<u8>, source: Option<Vec<u8>>, w: u32, h: u32, gpu_render: bool, aux: Option<DenoiseAux>) {
        let (tx, rx) = channel();
        let flame = flame.clone();
        std::thread::spawn(move || {
            let t0 = Instant::now();
            let r = apply_post_denoiser_from(&flame, &mut rgba, source.as_deref(), w, h, gpu_render, aux.as_ref()).map(|o| (o, rgba, t0.elapsed()));
            let _ = tx.send(r);
        });
        self.job = Some((key, rx));
        self.size = (w, h);
        self.last_start = Some(Instant::now());
    }

    /// Takes a finished result; call every frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some((key, rx)) = &self.job else { return };
        match rx.try_recv() {
            Ok(r) => {
                let key = *key;
                self.job = None;
                match r {
                    Ok((outcome, rgba, took)) => {
                        let color = egui::ColorImage::from_rgba_unmultiplied([self.size.0 as usize, self.size.1 as usize], &rgba);
                        match &mut self.texture {
                            Some(t) => t.set(color, egui::TextureOptions::LINEAR),
                            None => self.texture = Some(ctx.load_texture("denoised-view", color, egui::TextureOptions::LINEAR)),
                        }
                        self.outcome = Some(Ok((outcome, took)));
                    }
                    Err(e) => self.outcome = Some(Err(e)),
                }
                self.shown = Some(key);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(50)),
            Err(_) => self.job = None,
        }
    }

    /// Whether the image shown is the one for `key`.
    pub fn is_current(&self, key: u64) -> bool {
        self.shown == Some(key)
    }

    /// The denoised image (if one is ready).
    pub fn texture(&self) -> Option<egui::TextureId> {
        self.texture.as_ref().filter(|_| self.shown.is_some()).map(|t| t.id())
    }

    /// Forgets the shown image (the view was switched off or the image changed size).
    pub fn clear(&mut self) {
        self.shown = None;
        self.outcome = None;
    }

    /// One line about the last run, and whether it denoised.
    pub fn status(&self) -> Option<(String, bool)> {
        if self.job.is_some() && self.outcome.is_none() {
            return Some(("Denoising…".into(), true));
        }
        Some(match self.outcome.as_ref()? {
            Ok((o, took)) if o.denoised() => (format!("{} ({} ms)", capitalize(&o.describe()), took.as_millis()), true),
            Ok((o, _)) => (capitalize(&o.describe()), false),
            Err(e) => (format!("Denoiser failed: {e}"), false),
        })
    }

    /// Draws the status as a chip at `pos` (top-left corner).
    pub fn paint_status(&self, painter: &Painter, pos: Pos2) {
        let Some((text, ok)) = self.status() else { return };
        let color = if ok { theme::TEXT } else { theme::WARN };
        let galley = painter.layout(text, FontId::proportional(11.5), color, 520.0);
        let r = Rect::from_min_size(pos, galley.size() + vec2(16.0, 8.0));
        painter.rect_filled(r, 6, theme::PANEL.gamma_multiply(0.92));
        painter.galley(r.min + vec2(8.0, 4.0), galley, color);
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

/// Draws `tex` into `rect` (full texture).
pub fn paint_image(painter: &Painter, tex: egui::TextureId, rect: Rect) {
    painter.image(tex, rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
}
