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

//! The main canvas: flame preview plus direct-manipulation handles for transforms.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};
use rwf_core::render::View;
use rwf_core::{Affine, Flame};

use crate::doc::{Document, Selection};
use crate::preview::Preview;
use crate::{icons, theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handle {
    Origin,
    AxisX,
    AxisY,
    Body,
}

#[derive(Clone, Copy, Debug)]
struct Hit {
    sel: Selection,
    handle: Handle,
}

#[derive(Clone, Copy, Debug)]
enum DragKind {
    Edit { sel: Selection, handle: Handle, rotate: bool, scale: bool },
    Pan,
}

#[derive(Clone, Debug)]
struct Drag {
    kind: DragKind,
    start_world: (f64, f64),
    start_affine: Affine,
    start_centre: (f64, f64),
    start_view: View,
}

pub struct CanvasState {
    pub show_handles: bool,
    /// Shows the finished preview through the flame's post denoiser.
    pub show_denoised: bool,
    denoised: crate::denoise_view::DenoisedView,
    /// The flame and preview size the denoised image belongs to, and its revision.
    denoised_for: Option<(Flame, (u32, u32))>,
    denoised_rev: u64,
    /// Small orbit globe in the top-right corner of the canvas.
    pub show_camera_gizmo: bool,
    /// Fraction of the canvas resolution used for rendering (performance knob).
    pub resolution_scale: f32,
    drag: Option<Drag>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self { show_handles: true, show_denoised: false, denoised: Default::default(), denoised_for: None, denoised_rev: 0, show_camera_gizmo: true, resolution_scale: 1.0, drag: None }
    }
}

/// Distinct, calm colors for transform handles.
pub fn xform_color(i: usize) -> Color32 {
    const C: [Color32; 8] = [
        Color32::from_rgb(0xff, 0x7a, 0x3d),
        Color32::from_rgb(0x4d, 0xb6, 0xff),
        Color32::from_rgb(0x7e, 0xd9, 0x6a),
        Color32::from_rgb(0xe0, 0x6c, 0xf0),
        Color32::from_rgb(0xf5, 0xd0, 0x4c),
        Color32::from_rgb(0x4f, 0xe0, 0xc8),
        Color32::from_rgb(0xff, 0x6b, 0x8a),
        Color32::from_rgb(0xa8, 0x9b, 0xff),
    ];
    C[i % C.len()]
}

fn fit_rect(avail: Rect, aspect: f32) -> Rect {
    let avail = avail.shrink(14.0);
    let (w, h) = if avail.width() / avail.height().max(1.0) > aspect {
        (avail.height() * aspect, avail.height())
    } else {
        (avail.width(), avail.width() / aspect)
    };
    Rect::from_center_size(avail.center(), vec2(w.max(16.0), h.max(16.0)))
}

struct Mapper {
    view: View,
    rect: Rect,
    scale: f32,
}

impl Mapper {
    fn new(flame: &Flame, rect: Rect, iw: u32, ih: u32) -> Self {
        let fs = flame.filter_kernel.filter_size(flame.spatial_filter_radius, flame.spatial_oversampling.max(1));
        Self { view: View::new(flame, iw, ih, fs), rect, scale: rect.width() / iw as f32 }
    }
    fn to_screen(&self, x: f64, y: f64) -> Pos2 {
        let (px, py) = self.view.world_to_image(x, y);
        self.rect.min + vec2(px as f32, py as f32) * self.scale
    }
    fn to_world(&self, p: Pos2) -> (f64, f64) {
        let v = (p - self.rect.min) / self.scale;
        self.view.image_to_world(v.x as f64, v.y as f64)
    }
}

fn affine_of(doc: &Document, sel: Selection) -> Option<Affine> {
    let (kind, i) = sel.kind()?;
    let xf = doc.layer().xforms_of(kind).get(i)?;
    Some(if doc.edit_post { xf.post } else { xf.coefs })
}

fn set_affine(doc: &mut Document, sel: Selection, a: Affine) {
    let post = doc.edit_post;
    let xf = sel.kind().and_then(|(kind, i)| doc.layer_mut().xforms_of_mut(kind).get_mut(i));
    if let Some(xf) = xf {
        if post {
            xf.post = a;
        } else {
            xf.coefs = a;
        }
    }
}

fn triangle(m: &Mapper, a: &Affine) -> [Pos2; 3] {
    let o = m.to_screen(a.c20, a.c21);
    let x = m.to_screen(a.c20 + a.c00, a.c21 + a.c01);
    let y = m.to_screen(a.c20 + a.c10, a.c21 + a.c11);
    [o, x, y]
}

fn point_in_triangle(p: Pos2, t: &[Pos2; 3]) -> bool {
    let s = |a: Pos2, b: Pos2, c: Pos2| (a.x - c.x) * (b.y - c.y) - (b.x - c.x) * (a.y - c.y);
    let d1 = s(p, t[0], t[1]);
    let d2 = s(p, t[1], t[2]);
    let d3 = s(p, t[2], t[0]);
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

fn all_handles(doc: &Document) -> Vec<Selection> {
    let l = doc.layer();
    let mut v: Vec<Selection> = (0..l.xforms.len()).map(Selection::XForm).collect();
    v.extend((0..l.final_xforms.len()).map(Selection::Final));
    // Background xforms live in their own coordinate frame: only the selected one is shown.
    if let Selection::Background(i) = doc.selection {
        v.push(Selection::Background(i));
    }
    v
}

fn hit_test(doc: &Document, m: &Mapper, p: Pos2) -> Option<Hit> {
    let mut order = all_handles(doc);
    // The selected transform wins ties.
    if let Some(pos) = order.iter().position(|s| *s == doc.selection) {
        let s = order.remove(pos);
        order.insert(0, s);
    }
    const R: f32 = 9.0;
    for sel in &order {
        let Some(a) = affine_of(doc, *sel) else { continue };
        let t = triangle(m, &a);
        for (i, h) in [Handle::Origin, Handle::AxisX, Handle::AxisY].into_iter().enumerate() {
            if t[i].distance(p) <= R {
                return Some(Hit { sel: *sel, handle: h });
            }
        }
    }
    for sel in &order {
        let Some(a) = affine_of(doc, *sel) else { continue };
        if point_in_triangle(p, &triangle(m, &a)) {
            return Some(Hit { sel: *sel, handle: Handle::Body });
        }
    }
    None
}

pub fn show(ui: &mut egui::Ui, frame: &eframe::Frame, doc: &mut Document, state: &mut CanvasState, preview: &mut Preview) {
    let avail = ui.available_rect_before_wrap();
    let painter = ui.painter_at(avail);
    painter.rect_filled(avail, CornerRadius::ZERO, theme::CANVAS);
    let flame_aspect = doc.flame.width as f32 / doc.flame.height.max(1) as f32;
    let img_rect = fit_rect(avail, flame_aspect);
    let ppp = ui.ctx().pixels_per_point() * state.resolution_scale;
    let iw = (img_rect.width() * ppp).round().max(16.0) as u32;
    let ih = (img_rect.height() * ppp).round().max(16.0) as u32;

    let resp = ui.allocate_rect(avail, Sense::click_and_drag());

    // Render & draw the image.
    let image = preview.update(ui.ctx(), frame, &doc.flame, (iw, ih));
    let denoised = update_denoised(ui.ctx(), doc, state, preview, image.as_ref().map(|i| i.size));
    if let Some(tex) = denoised {
        crate::denoise_view::paint_image(&painter, tex, img_rect);
    } else if let Some(img) = &image {
        painter.image(img.texture, img_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    painter.rect_stroke(img_rect, CornerRadius::ZERO, Stroke::new(1.0, theme::BORDER), StrokeKind::Outside);

    let map_size = image.as_ref().map(|i| i.size).unwrap_or((iw, ih));
    let mapper = Mapper::new(&doc.flame, img_rect, map_size.0, map_size.1);

    // ---- interaction ----
    let pointer = resp.hover_pos();
    let modifiers = ui.input(|i| i.modifiers);
    let hover_hit = if state.show_handles && state.drag.is_none() { pointer.and_then(|p| hit_test(doc, &mapper, p)) } else { None };

    if resp.drag_started_by(egui::PointerButton::Primary) || resp.drag_started_by(egui::PointerButton::Middle) || resp.drag_started_by(egui::PointerButton::Secondary) {
        // Hit-test where the press started, not where the drag threshold was crossed.
        if let Some(p) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
            let world = mapper.to_world(p);
            let primary = resp.dragged_by(egui::PointerButton::Primary);
            let hit = if primary && state.show_handles { hit_test(doc, &mapper, p) } else { None };
            let kind = match hit {
                Some(h) => {
                    doc.selection = h.sel;
                    DragKind::Edit { sel: h.sel, handle: h.handle, rotate: modifiers.shift, scale: modifiers.command || modifiers.ctrl }
                }
                None => DragKind::Pan,
            };
            let start_affine = match kind {
                DragKind::Edit { sel, .. } => affine_of(doc, sel).unwrap_or_default(),
                DragKind::Pan => Affine::IDENTITY,
            };
            state.drag = Some(Drag {
                kind,
                start_world: world,
                start_affine,
                start_centre: (doc.flame.centre_x, doc.flame.centre_y),
                start_view: mapper.view.clone(),
            });
        }
    }

    if let (Some(drag), Some(p)) = (state.drag.clone(), resp.interact_pointer_pos())
        && resp.dragged()
    {
        // Map with the view at drag start so panning does not feed back into itself.
        let start_mapper = Mapper { view: drag.start_view.clone(), rect: img_rect, scale: mapper.scale };
        let (wx, wy) = start_mapper.to_world(p);
        let (sx, sy) = drag.start_world;
        match drag.kind {
            DragKind::Pan => {
                doc.flame.centre_x = drag.start_centre.0 - (wx - sx);
                doc.flame.centre_y = drag.start_centre.1 - (wy - sy);
            }
            DragKind::Edit { sel, handle, rotate, scale } => {
                let mut a = drag.start_affine;
                let (ox, oy) = (a.c20, a.c21);
                match handle {
                    Handle::Origin | Handle::Body if !rotate && !scale => {
                        a.c20 += wx - sx;
                        a.c21 += wy - sy;
                    }
                    Handle::AxisX if !rotate && !scale => {
                        a.c00 = wx - ox;
                        a.c01 = wy - oy;
                    }
                    Handle::AxisY if !rotate && !scale => {
                        a.c10 = wx - ox;
                        a.c11 = wy - oy;
                    }
                    _ => {
                        let a0 = (sy - oy).atan2(sx - ox);
                        let a1 = (wy - oy).atan2(wx - ox);
                        if rotate {
                            a.rotate((a1 - a0).to_degrees());
                        }
                        if scale {
                            let d0 = ((sx - ox).powi(2) + (sy - oy).powi(2)).sqrt().max(1e-9);
                            let d1 = ((wx - ox).powi(2) + (wy - oy).powi(2)).sqrt();
                            a.scale(d1 / d0);
                        }
                    }
                }
                set_affine(doc, sel, a);
            }
        }
    }
    if resp.drag_stopped() {
        state.drag = None;
    }

    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        if let Some(h) = hit_test(doc, &mapper, p).filter(|_| state.show_handles) {
            doc.selection = h.sel;
        }
    }

    // Zoom around the cursor.
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.1
            && let Some(p) = pointer
        {
            let before = mapper.to_world(p);
            let factor = (scroll as f64 / 300.0).exp();
            doc.flame.pixels_per_unit *= factor;
            let m2 = Mapper::new(&doc.flame, img_rect, map_size.0, map_size.1);
            let after = m2.to_world(p);
            doc.flame.centre_x += before.0 - after.0;
            doc.flame.centre_y += before.1 - after.1;
        }
    }

    // Cursor feedback
    if let Some(h) = hover_hit {
        ui.ctx().set_cursor_icon(match h.handle {
            Handle::Body => egui::CursorIcon::Move,
            _ => egui::CursorIcon::Grab,
        });
    } else if matches!(state.drag.as_ref().map(|d| d.kind), Some(DragKind::Pan)) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }

    // ---- handles ----
    if state.show_handles {
        let painter = ui.painter_at(img_rect.expand(40.0));
        for sel in all_handles(doc) {
            let Some(a) = affine_of(doc, sel) else { continue };
            let t = triangle(&mapper, &a);
            let idx = match sel {
                Selection::XForm(i) => i,
                _ => 7,
            };
            let color = match sel {
                Selection::Final(_) => Color32::from_rgb(0xd0, 0xd4, 0xdc),
                Selection::Background(_) => Color32::from_rgb(0xc8, 0xa8, 0x70),
                _ => xform_color(idx),
            };
            let selected = sel == doc.selection;
            let hovered = hover_hit.is_some_and(|h| h.sel == sel);
            let alpha = if selected { 255 } else if hovered { 200 } else { 110 };
            let c = color.gamma_multiply(alpha as f32 / 255.0);
            if selected {
                painter.add(Shape::convex_polygon(t.to_vec(), color.gamma_multiply(0.12), Stroke::NONE));
            }
            let width = if selected { 2.0 } else { 1.2 };
            let dashed = matches!(sel, Selection::Final(_) | Selection::Background(_)) || doc.edit_post;
            if dashed {
                for (p, q) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    painter.extend(Shape::dashed_line(&[p, q], Stroke::new(width, c), 6.0, 4.0));
                }
            } else {
                painter.add(Shape::closed_line(t.to_vec(), Stroke::new(width, c)));
            }
            if selected || hovered {
                let hv = hover_hit.filter(|h| h.sel == sel).map(|h| h.handle);
                for (i, h) in [Handle::Origin, Handle::AxisX, Handle::AxisY].into_iter().enumerate() {
                    let r = if hv == Some(h) { 6.5 } else { 4.5 };
                    painter.circle(t[i], r, theme::PANEL, Stroke::new(1.5, color));
                }
                let lbl = |p: Pos2, o: Pos2, s: &str| {
                    let dir = (p - o).normalized();
                    painter.text(p + dir * 12.0, Align2::CENTER_CENTER, s, FontId::proportional(11.0), color);
                };
                lbl(t[1], t[0], "X");
                lbl(t[2], t[0], "Y");
            }
        }
    }

    overlay(ui, avail, doc, state, preview);
}

/// Runs the denoiser on the finished preview while "Denoised" is on; returns the denoised
/// image when it belongs to the current flame.
fn update_denoised(ctx: &egui::Context, doc: &Document, state: &mut CanvasState, preview: &mut Preview, size: Option<(u32, u32)>) -> Option<egui::TextureId> {
    state.denoised.poll(ctx);
    if !state.show_denoised {
        state.denoised.clear();
        state.denoised_for = None;
        return None;
    }
    let size = size?;
    if state.denoised_for.as_ref().is_none_or(|(f, s)| *s != size || *f != doc.flame) {
        state.denoised_for = Some((doc.flame.clone(), size));
        state.denoised_rev += 1;
    }
    let gpu = preview.backend == crate::preview::Backend::Gpu && preview.gpu_available();
    let key = crate::denoise_view::key(state.denoised_rev, &doc.flame, gpu);
    if preview.is_converged()
        && state.denoised.wants(key, std::time::Duration::ZERO)
        && let Some((img, w, h, on_gpu)) = preview.read_image()
    {
        let aux = (on_gpu && rwf_core::render::denoise::skip_reason(&doc.flame, true).is_none() && rwf_core::render::oidn::library().is_ok())
            .then(|| preview.read_denoise_aux())
            .flatten();
        state.denoised.start(key, &doc.flame, img, w, h, on_gpu, aux);
    }
    state.denoised.is_current(key).then(|| state.denoised.texture()).flatten()
}

fn overlay(ui: &mut egui::Ui, avail: Rect, doc: &mut Document, state: &mut CanvasState, preview: &Preview) {
    // Floating toolbar (top-left of the canvas).
    let tb = Rect::from_min_size(avail.min + vec2(12.0, 10.0), vec2(avail.width() - 24.0, 30.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(tb).layout(egui::Layout::left_to_right(egui::Align::Center)));
    egui::Frame::new().fill(theme::PANEL.gamma_multiply(0.92)).corner_radius(8).inner_margin(egui::Margin::symmetric(6, 3)).show(&mut child, |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.toggle_value(&mut state.show_handles, format!("{} Transforms", icons::TRIANGLE)).on_hover_text("Show transform handles on the canvas");
        ui.add_enabled_ui(state.show_handles, |ui| {
            ui.selectable_value(&mut doc.edit_post, false, "Pre").on_hover_text("Edit the pre-affine transform");
            ui.selectable_value(&mut doc.edit_post, true, "Post").on_hover_text("Edit the post-affine transform");
        });
        ui.toggle_value(&mut state.show_camera_gizmo, format!("{} Camera", icons::GLOBE)).on_hover_text("Show the camera orbit globe on the canvas");
        ui.toggle_value(&mut state.show_denoised, format!("{} Denoised", icons::SPARKLE))
            .on_hover_text("Show the finished preview through the flame's AI denoiser, as saved images get it");
        ui.separator();
        if ui.button(format!("{} Fit", icons::FIT)).on_hover_text("Center and fit the flame (F)").clicked() {
            let mut f = doc.flame.clone();
            if rwf_core::render::analysis::auto_frame(&mut f) {
                doc.flame = f;
            }
        }
    });

    if state.show_camera_gizmo {
        crate::ui::camera_gizmo::overlay_globe(ui, pos2(avail.max.x - 104.0, avail.min.y + 46.0), &mut doc.flame);
    }

    if state.show_denoised {
        let pos = avail.min + vec2(12.0, 46.0);
        if preview.is_converged() {
            state.denoised.paint_status(&ui.painter_at(avail), pos);
        } else {
            let painter = ui.painter_at(avail);
            let text = format!("Denoised view: waits for the preview to reach quality {:.0}", preview.target_quality);
            let galley = painter.layout_no_wrap(text, FontId::proportional(11.5), theme::TEXT_MUTED);
            let r = Rect::from_min_size(pos, galley.size() + vec2(16.0, 8.0));
            painter.rect_filled(r, 6, theme::PANEL.gamma_multiply(0.92));
            painter.galley(r.min + vec2(8.0, 4.0), galley, theme::TEXT_MUTED);
        }
    }

    // Status chip (bottom-left).
    let density = preview.density();
    let target = preview.target_quality;
    let progress = (density / target).clamp(0.0, 1.0) as f32;
    let text = if preview.is_converged() {
        format!("Quality {:.0}  ·  done", density)
    } else {
        format!("Quality {:.0} / {:.0}  ·  {:.1} M samples/s", density, target, preview.samples_per_sec() / 1e6)
    };
    let chip = Rect::from_min_size(pos2(avail.min.x + 12.0, avail.max.y - 34.0), vec2(300.0, 24.0));
    let painter = ui.painter_at(avail);
    painter.rect_filled(chip, 8, theme::PANEL.gamma_multiply(0.92));
    let bar = Rect::from_min_size(chip.left_bottom() + vec2(8.0, -4.0), vec2((chip.width() - 16.0) * progress, 2.0));
    painter.rect_filled(bar, 1, theme::ACCENT);
    painter.text(chip.left_center() + vec2(10.0, -1.0), Align2::LEFT_CENTER, text, FontId::proportional(11.5), theme::TEXT_MUTED);

    // Warnings (unsupported variations / shader errors), bottom-right.
    let mut warn = Vec::new();
    if let Some(e) = preview.error() {
        warn.push(format!("Renderer: {}", e.lines().next().unwrap_or(e)));
    }
    let unsupported = preview.unsupported();
    if !unsupported.is_empty() {
        warn.push(format!("Not supported yet (ignored): {}", unsupported.join(", ")));
    }
    if !warn.is_empty() {
        let galley = painter.layout(warn.join("\n"), FontId::proportional(11.5), theme::WARN, 520.0);
        let r = Rect::from_min_size(pos2(avail.max.x - galley.size().x - 32.0, avail.max.y - galley.size().y - 22.0), galley.size() + Vec2::splat(16.0));
        painter.rect_filled(r, 8, theme::PANEL.gamma_multiply(0.95));
        painter.galley(r.min + Vec2::splat(8.0), galley, theme::WARN);
    }
}
