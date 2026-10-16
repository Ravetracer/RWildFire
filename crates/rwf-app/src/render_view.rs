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

//! Interactive renderer workspace (JWildfire's "Interactive renderer"): a long progressive
//! GPU render of one flame with pause and resume, live tone changes, saved render state
//! (`.rwfrender`) and image export.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use egui::{Color32, Rect, RichText, Sense, Ui, pos2, vec2};
use rwf_core::Flame;
use rwf_core::output::ImageFormat;
use rwf_gpu::{GpuContext, GpuRenderer, RenderState};

use crate::theme;
use crate::ui::widgets::{self, grid, slider};

/// Image refresh: starts fast and slows down while the image converges (tonemapping a large
/// image costs GPU time that would otherwise go into the iteration).
const FIRST_REFRESH: Duration = Duration::from_millis(50);
const REFRESH_STEP: Duration = Duration::from_millis(25);
const MAX_REFRESH: Duration = Duration::from_millis(1500);
/// Frame time the iteration aims for, so the UI stays responsive.
const FRAME_BUDGET_MS: f64 = 30.0;
/// Renders that ran longer than this ask before they are discarded.
const CONFIRM_DISCARD_AFTER: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    Idle,
    Running,
    Paused,
    /// Reached the quality limit.
    Finished,
}

/// Something that discards the current render, waiting for confirmation.
enum Pending {
    Start,
    FromEditor,
    Load(PathBuf),
}

/// A file operation running on a background thread.
enum Busy {
    SaveImage(Receiver<Result<(PathBuf, String), String>>),
    SaveState(Receiver<Result<PathBuf, String>>),
    LoadState(PathBuf, Receiver<Result<RenderState, String>>),
}

/// Pixels read back for saving.
enum Pixels {
    Rgba(Vec<u8>),
    Hdr(Vec<f32>),
}

pub struct InteractiveRenderer {
    pub open: bool,
    renderer: Option<GpuRenderer>,
    flame: Option<Flame>,
    /// Size and options of the next render.
    width: u32,
    height: u32,
    zbuffer: bool,
    limit_quality: bool,
    max_quality: f64,
    image_format: ImageFormat,
    jpeg_quality: u8,
    state: State,
    /// Size of the running render.
    render_size: (u32, u32),
    elapsed: Duration,
    last_tick: Option<Instant>,
    iters: u32,
    texture: Option<(egui::TextureId, u64)>,
    /// The flame's image effects on the shown image: the pass, its egui texture, and for which
    /// image (revision, output generation, effects) it ran last.
    effects: Option<rwf_gpu::effects::EffectsPass>,
    effects_texture: Option<(egui::TextureId, u64)>,
    effects_for: Option<(u64, u64, String)>,
    next_refresh: Instant,
    refresh_interval: Duration,
    needs_refresh: bool,
    /// Quality speed (density per second) and its measurement window.
    speed: f64,
    speed_window: (Instant, f64),
    /// Zoom factor (`None` fits the image into the view) and pan offset in points.
    zoom: Option<f32>,
    pan: egui::Vec2,
    /// Render time at the last save of the state (no confirmation needed before discarding).
    saved_at: Option<Duration>,
    pending: Option<Pending>,
    busy: Option<Busy>,
    status: Option<String>,
    /// Shows the image through the flame's post denoiser.
    show_denoised: bool,
    denoised: crate::denoise_view::DenoisedView,
    /// Bumped whenever the shown image is refreshed.
    image_rev: u64,
}

impl Default for InteractiveRenderer {
    fn default() -> Self {
        Self {
            open: false,
            renderer: None,
            flame: None,
            width: 1920,
            height: 1080,
            zbuffer: false,
            limit_quality: false,
            max_quality: 5000.0,
            image_format: ImageFormat::Png,
            jpeg_quality: 95,
            state: State::Idle,
            render_size: (0, 0),
            elapsed: Duration::ZERO,
            last_tick: None,
            iters: 32,
            texture: None,
            effects: None,
            effects_texture: None,
            effects_for: None,
            next_refresh: Instant::now(),
            refresh_interval: FIRST_REFRESH,
            needs_refresh: false,
            speed: 0.0,
            speed_window: (Instant::now(), 0.0),
            zoom: None,
            pan: egui::Vec2::ZERO,
            saved_at: None,
            pending: None,
            busy: None,
            status: None,
            show_denoised: false,
            denoised: Default::default(),
            image_rev: 0,
        }
    }
}

fn format_duration(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

impl InteractiveRenderer {
    /// Opens the workspace; without a render in progress it takes the editor's flame.
    pub fn open_with(&mut self, flame: &Flame) {
        self.open = true;
        if self.state == State::Idle {
            self.take_flame(flame);
        }
    }

    fn density(&self) -> f64 {
        self.renderer.as_ref().map_or(0.0, |r| r.density())
    }

    fn has_progress(&self) -> bool {
        self.state != State::Idle && self.elapsed >= CONFIRM_DISCARD_AFTER && self.saved_at != Some(self.elapsed)
    }

    /// Runs `p` now, or asks first if that would discard a long render.
    fn request(&mut self, p: Pending, editor_flame: &Flame, gpu: &Option<GpuContext>) {
        if self.has_progress() {
            self.pending = Some(p);
        } else {
            self.run_pending(p, editor_flame, gpu);
        }
    }

    fn run_pending(&mut self, p: Pending, editor_flame: &Flame, gpu: &Option<GpuContext>) {
        match p {
            Pending::Start => self.start(gpu),
            Pending::FromEditor => {
                self.take_flame(editor_flame);
                self.stop();
            }
            Pending::Load(path) => self.load_state(path),
        }
    }

    /// Takes over a flame and sets the output size to its aspect ratio.
    fn take_flame(&mut self, flame: &Flame) {
        let aspect = flame.height.max(1) as f64 / flame.width.max(1) as f64;
        if (self.height as f64 / self.width.max(1) as f64 - aspect).abs() > 1e-3 {
            self.height = ((self.width as f64 * aspect).round() as u32).clamp(16, 8192);
        }
        self.flame = Some(flame.clone());
    }

    fn stop(&mut self) {
        self.renderer = None;
        self.texture = None;
        self.state = State::Idle;
        self.elapsed = Duration::ZERO;
        self.saved_at = None;
    }

    fn start(&mut self, gpu: &Option<GpuContext>) {
        let (Some(flame), Some(ctx)) = (self.flame.clone(), gpu.clone()) else { return };
        let mut r = self.renderer.take().unwrap_or_else(|| GpuRenderer::new(ctx));
        // Shown opaque; a transparent PNG is tonemapped with alpha when it is saved.
        r.set_with_alpha(false);
        r.set_zbuffer(self.zbuffer);
        r.set_flame(&flame, self.width, self.height);
        r.reset_accumulation();
        self.begin(r, (self.width, self.height), Duration::ZERO);
    }

    /// Switches to `r` as the running render.
    fn begin(&mut self, r: GpuRenderer, size: (u32, u32), elapsed: Duration) {
        self.status = r.last_error().map(|e| format!("GPU error: {e}"));
        if self.status.is_none() && !r.unsupported_variations().is_empty() {
            self.status = Some(format!("Not supported on the GPU, ignored: {}", r.unsupported_variations().join(", ")));
        }
        self.renderer = Some(r);
        self.render_size = size;
        self.state = State::Running;
        self.elapsed = elapsed;
        self.saved_at = None;
        self.last_tick = None;
        self.iters = 32;
        self.refresh_interval = FIRST_REFRESH;
        self.next_refresh = Instant::now();
        self.needs_refresh = true;
        self.speed = 0.0;
        self.speed_window = (Instant::now(), self.density());
        self.zoom = None;
        self.pan = egui::Vec2::ZERO;
    }

    fn quality_reached(&self) -> bool {
        self.limit_quality && self.density() >= self.max_quality
    }

    /// Iterates for this frame and refreshes the image when due.
    fn step(&mut self, ctx: &egui::Context) {
        if self.state == State::Running && self.quality_reached() {
            self.state = State::Finished;
            self.needs_refresh = true;
        }
        let running = self.state == State::Running;
        let Some(r) = self.renderer.as_mut() else { return };
        if running {
            // The time since the last iterating frame is render time; longer gaps are not
            // (the workspace was hidden or the window was stalled).
            let now = Instant::now();
            let ms = self.last_tick.map_or(0.0, |t| now.duration_since(t).as_secs_f64() * 1000.0);
            if ms > 0.0 && ms < 1000.0 {
                self.elapsed += now.duration_since(self.last_tick.unwrap_or(now));
            }
            self.last_tick = Some(now);
            if ms > 0.0 && ms < FRAME_BUDGET_MS * 0.6 {
                self.iters = (self.iters + self.iters / 4 + 1).min(4096);
            } else if ms > FRAME_BUDGET_MS * 1.3 && ms < 1000.0 {
                self.iters = ((self.iters as f64 * FRAME_BUDGET_MS / ms) as u32).min(self.iters * 3 / 4).max(1);
            }
            r.iterate(1, self.iters);
            let (t0, d0) = self.speed_window;
            let dt = t0.elapsed().as_secs_f64();
            if dt >= 2.0 {
                let d = r.density();
                self.speed = (d - d0).max(0.0) / dt;
                self.speed_window = (Instant::now(), d);
            }
            ctx.request_repaint();
        } else {
            self.last_tick = None;
        }
        let now = Instant::now();
        if self.needs_refresh || (running && now >= self.next_refresh) {
            r.tonemap();
            self.image_rev += 1;
            if running && !self.needs_refresh {
                self.refresh_interval = (self.refresh_interval + REFRESH_STEP).min(MAX_REFRESH);
            }
            self.next_refresh = now + self.refresh_interval;
            self.needs_refresh = false;
        }
    }

    /// Denoises the shown image in the background while "Show denoised" is on.
    fn update_denoised(&mut self, ctx: &egui::Context) {
        self.denoised.poll(ctx);
        if !self.show_denoised {
            self.denoised.clear();
            return;
        }
        let (Some(r), Some(flame)) = (self.renderer.as_mut(), self.flame.as_ref()) else { return };
        let key = crate::denoise_view::key(self.image_rev, flame, true);
        let interval = if self.state == State::Running { Duration::from_secs(3) } else { Duration::ZERO };
        if self.denoised.wants(key, interval)
            && let (Some(img), Some((w, h))) = (r.read_image(), r.output_size())
        {
            let (source, aux) = r.read_denoise_input(flame);
            self.denoised.start(key, flame, img, source, w, h, true, aux);
        }
    }

    /// Registers the output texture with egui (again after it was recreated); with the
    /// flame's image effects applied, if it has any.
    fn texture(&mut self, frame: &eframe::Frame) -> Option<egui::TextureId> {
        let r = self.renderer.as_ref()?;
        let (view, generation) = (r.output_view()?.clone(), r.output_generation());
        let rs = frame.wgpu_render_state()?;
        if let (Some(fx), Some((w, h))) = (self.flame.as_ref().map(|f| &f.effects).filter(|e| e.is_active()), r.output_size()) {
            let key = (self.image_rev, generation, format!("{fx:?}"));
            let ctx = r.context().clone();
            let pass = self.effects.get_or_insert_with(|| rwf_gpu::effects::EffectsPass::new(&ctx.device));
            let ran = self.effects_for.as_ref() == Some(&key) || pass.run(&ctx, &view, w, h, fx);
            if ran {
                self.effects_for = Some(key);
                let (fx_view, fx_gen) = (pass.output_view()?.clone(), pass.generation());
                match self.effects_texture {
                    Some((_, g)) if g == fx_gen => {}
                    Some((id, _)) => {
                        rs.renderer.write().update_egui_texture_from_wgpu_texture(&rs.device, &fx_view, wgpu::FilterMode::Linear, id);
                        self.effects_texture = Some((id, fx_gen));
                    }
                    None => self.effects_texture = Some((rs.renderer.write().register_native_texture(&rs.device, &fx_view, wgpu::FilterMode::Linear), fx_gen)),
                }
                return self.effects_texture.map(|t| t.0);
            }
        }
        match self.texture {
            Some((_, g)) if g == generation => {}
            Some((id, _)) => {
                rs.renderer.write().update_egui_texture_from_wgpu_texture(&rs.device, &view, wgpu::FilterMode::Linear, id);
                self.texture = Some((id, generation));
            }
            None => {
                let id = rs.renderer.write().register_native_texture(&rs.device, &view, wgpu::FilterMode::Linear);
                self.texture = Some((id, generation));
            }
        }
        self.texture.map(|t| t.0)
    }

    /// Shows the workspace. Returns a flame to load into the editor.
    pub fn show(&mut self, ui: &mut Ui, frame: &eframe::Frame, editor_flame: &Flame, gpu: Option<GpuContext>) -> Option<Flame> {
        let ctx = ui.ctx().clone();
        self.poll_busy(&gpu);
        self.step(&ctx);
        self.update_denoised(&ctx);
        let mut to_editor = None;
        egui::Panel::right("ir-panel")
            .resizable(true)
            .default_size(350.0)
            .size_range(330.0..=520.0)
            .frame(egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().slider_width = 120.0;
                    to_editor = self.panel(ui, editor_flame, &gpu);
                });
            });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::CANVAS)).show(ui, |ui| {
            self.image_view(ui, frame, gpu.is_some());
        });
        self.confirm_dialog(&ctx, editor_flame, &gpu);
        to_editor
    }

    fn panel(&mut self, ui: &mut Ui, editor_flame: &Flame, gpu: &Option<GpuContext>) -> Option<Flame> {
        let mut to_editor = None;
        let idle = self.state == State::Idle;
        let busy = self.busy.is_some();

        theme::section_label(ui, "Flame");
        ui.label(RichText::new(self.flame.as_ref().map_or("—", |f| if f.name.is_empty() { "Untitled" } else { f.name.as_str() })).color(theme::TEXT));
        ui.horizontal(|ui| {
            if ui.button("From editor").on_hover_text("Render the flame of the editor").clicked() {
                self.request(Pending::FromEditor, editor_flame, gpu);
            }
            if ui
                .add_enabled(self.flame.is_some(), egui::Button::new("To editor"))
                .on_hover_text("Load this flame (with its tone settings) into the editor")
                .clicked()
            {
                to_editor = self.flame.clone();
            }
        });

        theme::section_label(ui, "Output");
        ui.add_enabled_ui(idle && !busy, |ui| {
            grid(ui, "ir-size", |ui| {
                widgets::label(ui, "Size");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut self.width).range(16..=8192));
                    ui.label("×");
                    ui.add(egui::DragValue::new(&mut self.height).range(16..=8192));
                });
                ui.end_row();
                widgets::label(ui, "Presets");
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let aspect = self.flame.as_ref().map_or(9.0 / 16.0, |f| f.height.max(1) as f64 / f.width.max(1) as f64);
                    for (l, w) in [("HD", 1280u32), ("FHD", 1920), ("QHD", 2560), ("4K", 3840), ("8K", 7680)] {
                        if ui.small_button(l).clicked() {
                            self.width = w;
                            self.height = ((w as f64 * aspect).round() as u32).clamp(16, 8192);
                        }
                    }
                });
                ui.end_row();
                widgets::label(ui, "Depth");
                ui.checkbox(&mut self.zbuffer, "Record z-buffer");
                ui.end_row();
            });
        });
        grid(ui, "ir-limit", |ui| {
            widgets::label(ui, "Stop at");
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.limit_quality, "");
                ui.add_enabled(self.limit_quality, egui::DragValue::new(&mut self.max_quality).range(10.0..=1.0e6).speed(10.0).suffix(" quality"));
            });
            ui.end_row();
        });
        if self.state == State::Finished && !self.quality_reached() {
            self.state = State::Paused;
        }

        ui.add_space(8.0);
        ui.add_enabled_ui(gpu.is_some() && self.flame.is_some() && !busy, |ui| {
            ui.horizontal(|ui| {
                let w = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                let start_label = if idle { format!("{}  Start", crate::icons::PLAY) } else { format!("{}  Restart", crate::icons::REFRESH) };
                if ui.add_sized([w, 30.0], egui::Button::new(RichText::new(start_label).strong()).fill(theme::ACCENT_DIM)).clicked() {
                    self.request(Pending::Start, editor_flame, gpu);
                }
                match self.state {
                    State::Running => {
                        if ui.add_sized([w, 30.0], egui::Button::new(format!("{}  Pause", crate::icons::PAUSE))).clicked() {
                            self.state = State::Paused;
                            self.needs_refresh = true;
                        }
                    }
                    State::Paused => {
                        if ui.add_sized([w, 30.0], egui::Button::new(format!("{}  Resume", crate::icons::PLAY))).clicked() {
                            self.state = State::Running;
                        }
                    }
                    State::Idle | State::Finished => {
                        ui.add_enabled(false, egui::Button::new(format!("{}  Pause", crate::icons::PAUSE)).min_size(vec2(w, 30.0)));
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!idle, egui::Button::new("Save state…"))
                    .on_hover_text("Save the render to continue it later (.rwfrender)")
                    .clicked()
                {
                    self.save_state();
                }
                if ui.button("Load state…").on_hover_text("Continue a saved render").clicked()
                    && let Some(p) = crate::settings::dialog(crate::settings::Folder::Images).add_filter("Render state", &["rwfrender"]).pick_file()
                {
                    self.request(Pending::Load(p), editor_flame, gpu);
                }
            });
        });
        if gpu.is_none() {
            ui.label(RichText::new("The interactive renderer needs a GPU.").small().color(theme::WARN));
        }

        theme::section_label(ui, "Progress");
        self.stats(ui);

        if let Some(f) = self.flame.as_mut() {
            theme::section_label(ui, "Tone");
            let before = f.clone();
            grid(ui, "ir-tone", |ui| {
                slider(ui, "Brightness", &mut f.brightness, 0.0..=20.0, widgets::flame_defaults().brightness);
                slider(ui, "Gamma", &mut f.gamma, 1.0..=8.0, widgets::flame_defaults().gamma);
                slider(ui, "Gamma threshold", &mut f.gamma_threshold, 0.0..=0.2, widgets::flame_defaults().gamma_threshold);
                slider(ui, "Vibrancy", &mut f.vibrancy, 0.0..=1.0, widgets::flame_defaults().vibrancy);
                slider(ui, "Contrast", &mut f.contrast, 0.1..=4.0, widgets::flame_defaults().contrast);
                slider(ui, "Saturation", &mut f.saturation, 0.0..=2.0, widgets::flame_defaults().saturation);
                slider(ui, "White level", &mut f.white_level, 20.0..=400.0, widgets::flame_defaults().white_level);
                slider(ui, "Low density", &mut f.low_density_brightness, 0.0..=1.0, widgets::flame_defaults().low_density_brightness);
            });
            if *f != before
                && let Some(r) = self.renderer.as_mut()
            {
                // Tone-only change: the renderer keeps its accumulation.
                r.set_flame(f, self.render_size.0, self.render_size.1);
                self.needs_refresh = true;
            }
        }

        if let Some(f) = self.flame.as_mut() {
            theme::section_label(ui, "Denoiser");
            ui.label(RichText::new("Applied when the image is saved; these settings belong to this render.").small().color(theme::TEXT_MUTED));
            crate::ui::inspector::denoiser_settings(ui, "ir-denoiser", f);
            ui.checkbox(&mut self.show_denoised, "Show denoised").on_hover_text("Shows the image as it will be saved (refreshed every few seconds while rendering)");
            match rwf_core::render::denoise::skip_reason(f, true) {
                Some(why) => {
                    ui.label(RichText::new(format!("Saving will not denoise: {why}.")).small().color(theme::WARN));
                }
                None => {
                    if let Err(e) = rwf_core::render::denoise::backend() {
                        ui.label(RichText::new(e).small().color(theme::WARN));
                    }
                }
            }
        }

        theme::section_label(ui, "Save image");
        ui.add_enabled_ui(!idle && !busy, |ui| {
            grid(ui, "ir-save", |ui| {
                widgets::label(ui, "Format");
                egui::ComboBox::from_id_salt("ir-fmt").selected_text(self.image_format.label()).show_ui(ui, |ui| {
                    for f in ImageFormat::ALL {
                        ui.selectable_value(&mut self.image_format, *f, f.label());
                    }
                });
                ui.end_row();
                if self.image_format == ImageFormat::Jpeg {
                    widgets::label(ui, "JPEG quality");
                    widgets::reset_on_double_click(ui.add(egui::Slider::new(&mut self.jpeg_quality, 50..=100)), &mut self.jpeg_quality, 95);
                    ui.end_row();
                }
            });
            if ui.add_sized([ui.available_width(), 28.0], egui::Button::new(format!("{}  Save image…", crate::icons::EXPORT))).clicked() {
                self.save_image();
            }
        });

        if busy {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Working…").color(theme::TEXT_MUTED));
            });
        }
        if let Some(s) = &self.status {
            ui.add_space(6.0);
            ui.label(RichText::new(s).small().color(theme::TEXT_MUTED));
        }
        to_editor
    }

    fn stats(&self, ui: &mut Ui) {
        let Some(r) = self.renderer.as_ref() else {
            ui.label(RichText::new("Not started").color(theme::TEXT_MUTED));
            return;
        };
        let q = r.density();
        grid(ui, "ir-stats", |ui| {
            widgets::label(ui, "State");
            ui.label(match self.state {
                State::Idle => "Idle",
                State::Running => "Rendering",
                State::Paused => "Paused",
                State::Finished => "Finished",
            });
            ui.end_row();
            widgets::label(ui, "Quality");
            ui.label(format!("{q:.1}"));
            ui.end_row();
            widgets::label(ui, "Samples");
            ui.label(format!("{:.3} M", r.samples() as f64 / 1e6));
            ui.end_row();
            widgets::label(ui, "Size");
            ui.label(format!("{} × {}", self.render_size.0, self.render_size.1));
            ui.end_row();
            widgets::label(ui, "Elapsed");
            ui.label(format_duration(self.elapsed));
            ui.end_row();
            if self.state == State::Running && self.speed > 0.0 {
                widgets::label(ui, "Speed");
                ui.label(format!("{:.0} quality / min", self.speed * 60.0));
                ui.end_row();
                // Next goal: the limit, or (like JWildfire) the next multiple of 500.
                let goal = if self.limit_quality { self.max_quality } else { ((q / 500.0).floor() + 1.0) * 500.0 };
                if goal > q {
                    widgets::label(ui, format!("Quality {goal:.0}").as_str());
                    ui.label(format!("in {}", format_duration(Duration::from_secs_f64((goal - q) / self.speed))));
                    ui.end_row();
                }
            }
        });
        if self.limit_quality && self.max_quality > 0.0 {
            ui.add(egui::ProgressBar::new((q / self.max_quality).min(1.0) as f32).desired_height(6.0));
        }
    }

    fn image_view(&mut self, ui: &mut Ui, frame: &eframe::Frame, gpu: bool) {
        let rect = ui.max_rect();
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let (Some(tex), Some((w, h))) = (self.texture(frame), self.renderer.as_ref().and_then(|r| r.output_size())) else {
            let msg = if gpu { "Press Start to render the flame" } else { "No GPU available" };
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(theme::fs(15.0)), theme::TEXT_MUTED);
            return;
        };
        let size = vec2(w as f32, h as f32);
        let fit = ((rect.width() - 24.0) / size.x).min((rect.height() - 24.0) / size.y).max(0.01);
        // Zoom with the wheel around the pointer, pan by dragging, double click: fit / 100 %.
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let old = self.zoom.unwrap_or(fit);
                let new = (old * (scroll / 200.0).exp()).clamp(0.02, 16.0);
                if let Some(p) = resp.hover_pos() {
                    let rel = p - rect.center() - self.pan;
                    self.pan += rel - rel * (new / old);
                }
                self.zoom = Some(new);
            }
        }
        if resp.dragged() {
            self.pan += resp.drag_delta();
            if self.zoom.is_none() {
                self.zoom = Some(fit);
            }
        }
        if resp.double_clicked() {
            self.zoom = if self.zoom.is_some() { None } else { Some(1.0) };
            self.pan = egui::Vec2::ZERO;
        }
        let scale = self.zoom.unwrap_or(fit);
        let pan = if self.zoom.is_some() { self.pan } else { egui::Vec2::ZERO };
        let img_rect = Rect::from_center_size(rect.center() + pan, size * scale);
        painter.rect_filled(img_rect, 0.0, Color32::BLACK);
        match self.denoised.texture().filter(|_| self.show_denoised) {
            Some(dt) => crate::denoise_view::paint_image(&painter, dt, img_rect),
            None => {
                painter.image(tex, img_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
        }
        if self.show_denoised {
            self.denoised.paint_status(&painter, rect.left_top() + vec2(10.0, 10.0));
        }
        let zoom_text = if self.zoom.is_some() { format!("{:.0} %", scale * 100.0) } else { format!("Fit ({:.0} %)", scale * 100.0) };
        painter.text(
            rect.left_bottom() + vec2(10.0, -10.0),
            egui::Align2::LEFT_BOTTOM,
            zoom_text,
            egui::FontId::proportional(theme::fs(12.0)),
            theme::TEXT_MUTED,
        );
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context, editor_flame: &Flame, gpu: &Option<GpuContext>) {
        if self.pending.is_none() {
            return;
        }
        let mut answer = None;
        egui::Modal::new(egui::Id::new("ir-confirm")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.label(RichText::new("Discard the current render?").strong());
            ui.label(
                RichText::new(format!("It has been rendering for {} and its state is not saved.", format_duration(self.elapsed)))
                    .color(theme::TEXT_MUTED),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Discard").clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        match answer {
            Some(true) => {
                if let Some(p) = self.pending.take() {
                    self.run_pending(p, editor_flame, gpu);
                }
            }
            Some(false) => self.pending = None,
            None => {}
        }
    }

    fn save_image(&mut self) {
        let Some(r) = self.renderer.as_mut() else { return };
        let Some(flame) = self.flame.clone() else { return };
        let ext = self.image_format.extension();
        let name = format!("{}.{ext}", if flame.name.is_empty() { "flame" } else { &flame.name });
        let Some(path) = crate::settings::dialog(crate::settings::Folder::Images).add_filter(self.image_format.label(), &[ext]).set_file_name(name).save_file() else { return };
        // Read back here (the device belongs to this thread), encode and write in the background.
        let (format, jpeg_quality) = (self.image_format, self.jpeg_quality);
        let mut denoise_input = (None, None);
        let pixels = if format.is_hdr() {
            r.read_hdr().map(Pixels::Hdr)
        } else {
            // Only PNG keeps an alpha channel.
            r.set_with_alpha(flame.bg_transparency && format == ImageFormat::Png);
            r.tonemap();
            let img = r.read_image().map(Pixels::Rgba);
            denoise_input = r.read_denoise_input(&flame);
            r.set_with_alpha(false);
            self.needs_refresh = true;
            img
        };
        let Some(pixels) = pixels else {
            self.status = Some("Readback failed".into());
            return;
        };
        let zbuf = if self.zbuffer { r.read_zbuffer() } else { None };
        let (source, aux) = denoise_input;
        let (w, h) = self.render_size;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let result = (|| -> Result<(PathBuf, String), String> {
                let note = match pixels {
                    Pixels::Hdr(rgb) => {
                        rwf_core::output::write_float_rgb(&path, format, w, h, &rgb)?;
                        rwf_core::render::post::hdr_note(&flame)
                    }
                    Pixels::Rgba(mut img) => {
                        let outcome = rwf_core::render::post::process(&flame, &mut img, source.as_deref(), w, h, true, aux.as_ref())
                            .map_err(|e| format!("denoiser: {e}"))?;
                        rwf_core::output::write_rgba8(&path, format, w, h, &img, jpeg_quality)?;
                        outcome.describe()
                    }
                };
                if let Some(z) = zbuf {
                    rwf_core::output::write_gray16_png(&flame.zbuffer_filename.path_for(&path), w, h, &z)?;
                }
                Ok((path, note))
            })();
            let _ = tx.send(result);
        });
        self.busy = Some(Busy::SaveImage(rx));
    }

    fn save_state(&mut self) {
        let Some(r) = self.renderer.as_ref() else { return };
        let name = format!("{}.rwfrender", self.flame.as_ref().filter(|f| !f.name.is_empty()).map_or("render", |f| f.name.as_str()));
        let Some(path) = crate::settings::dialog(crate::settings::Folder::Images).add_filter("Render state", &["rwfrender"]).set_file_name(name).save_file() else { return };
        let Some(st) = r.save_state(self.elapsed.as_millis() as u64) else {
            self.status = Some("Readback failed".into());
            return;
        };
        self.saved_at = Some(self.elapsed);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(st.write(&path).map(|_| path));
        });
        self.busy = Some(Busy::SaveState(rx));
    }

    fn load_state(&mut self, path: PathBuf) {
        let (tx, rx) = channel();
        let p = path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(RenderState::read(&p));
        });
        self.busy = Some(Busy::LoadState(path, rx));
    }

    /// Finishes background file operations.
    fn poll_busy(&mut self, gpu: &Option<GpuContext>) {
        let Some(busy) = self.busy.take() else { return };
        let saving_state = matches!(busy, Busy::SaveState(_));
        let done = match &busy {
            Busy::SaveImage(rx) => rx.try_recv().ok().map(|r| match r {
                Ok((p, note)) => Ok(format!("Saved {} — {note}", p.display())),
                Err(e) => Err(format!("Save failed: {e}")),
            }),
            Busy::SaveState(rx) => rx.try_recv().ok().map(|r| match r {
                Ok(p) => Ok(format!("Saved {}", p.display())),
                Err(e) => Err(format!("Save failed: {e}")),
            }),
            Busy::LoadState(path, rx) => rx.try_recv().ok().map(|r| {
                let st = r.map_err(|e| format!("Could not load {}: {e}", path.display()))?;
                let ctx = gpu.clone().ok_or("No GPU available")?;
                let mut renderer = self.renderer.take().unwrap_or_else(|| GpuRenderer::new(ctx));
                renderer.restore_state(&st).map_err(|e| format!("Could not continue the render: {e}"))?;
                let elapsed = Duration::from_millis(st.elapsed_ms);
                self.zbuffer = st.zbuffer;
                self.width = st.width;
                self.height = st.height;
                self.flame = Some(st.flame);
                self.begin(renderer, (st.width, st.height), elapsed);
                self.state = State::Paused;
                self.saved_at = Some(elapsed);
                Ok(format!("Loaded {} — press Resume to continue", path.display()))
            }),
        };
        match done {
            Some(result) => {
                if saving_state && result.is_err() {
                    self.saved_at = None;
                }
                self.status = Some(result.unwrap_or_else(|e| e));
            }
            None => self.busy = Some(busy),
        }
    }
}
