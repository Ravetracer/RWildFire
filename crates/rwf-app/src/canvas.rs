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

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};
use rwf_core::render::View;
use rwf_core::{Affine, Flame};

use crate::doc::{Document, Selection};
use crate::preview::Preview;
use crate::settings::ControlShape;
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
    /// Moves the depth-of-field focus point (focus mode).
    Focus,
}

/// What a double-click on the canvas does (JWildfire's editor double-click action).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DoubleClick {
    #[default]
    Nothing,
    OpenRender,
    ToggleHandles,
}

impl DoubleClick {
    pub const ALL: [DoubleClick; 3] = [DoubleClick::Nothing, DoubleClick::OpenRender, DoubleClick::ToggleHandles];

    pub fn label(&self) -> &'static str {
        match self {
            DoubleClick::Nothing => "Nothing",
            DoubleClick::OpenRender => "Open the render window",
            DoubleClick::ToggleHandles => "Show/hide transform handles",
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            DoubleClick::Nothing => "none",
            DoubleClick::OpenRender => "render",
            DoubleClick::ToggleHandles => "handles",
        }
    }

    pub fn parse(s: &str) -> Self {
        Self::ALL.into_iter().find(|d| d.as_str() == s).unwrap_or_default()
    }
}

#[derive(Clone, Debug)]
struct Drag {
    kind: DragKind,
    start_world: (f64, f64),
    start_affine: Affine,
    start_centre: (f64, f64),
    start_focus: (f64, f64),
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
    /// Dragging moves, turns and scales at a tenth of the speed.
    pub fine: bool,
    /// Which axes Ctrl-drag scales.
    pub scale_x: bool,
    pub scale_y: bool,
    /// Grid in world units, `grid_size` apart; composition guides over the image.
    pub show_grid: bool,
    pub grid_size: f64,
    /// Look of the transform controls (Preferences).
    pub controls: crate::settings::ControlStyle,
    pub show_guides: bool,
    /// The selected transform applied to a grid ("variation effect").
    pub show_var_effect: bool,
    var_mesh: Option<(Flame, usize, Selection, Vec<[f64; 2]>)>,
    /// Dragging on the canvas moves the depth-of-field focus point.
    pub focus_mode: bool,
    pub double_click: DoubleClick,
    /// Set by a double-click that asks for the render window.
    pub open_render: bool,
    /// The preview is also shown in a window of its own.
    pub detached: bool,
    drag: Option<Drag>,
}

impl Default for CanvasState {
    fn default() -> Self {
        Self { show_handles: true, show_denoised: false, denoised: Default::default(), denoised_for: None, denoised_rev: 0, show_camera_gizmo: true, resolution_scale: 1.0, fine: false,
            scale_x: true,
            scale_y: true,
            show_grid: false,
            grid_size: 0.5,
            controls: Default::default(),
            show_guides: false,
            show_var_effect: false,
            var_mesh: None,
            focus_mode: false,
            double_click: DoubleClick::Nothing,
            open_render: false,
            detached: false,
            drag: None,
        }
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
    Some(*crate::doc::edit_affine(xf, doc.edit_plane, doc.edit_post))
}

fn set_affine(doc: &mut Document, sel: Selection, a: Affine) {
    let (plane, post) = (doc.edit_plane, doc.edit_post);
    let xf = sel.kind().and_then(|(kind, i)| doc.layer_mut().xforms_of_mut(kind).get_mut(i));
    if let Some(xf) = xf {
        *crate::doc::edit_affine_mut(xf, plane, post) = a;
    }
}

fn triangle(m: &Mapper, a: &Affine) -> [Pos2; 3] {
    let o = m.to_screen(a.c20, a.c21);
    let x = m.to_screen(a.c20 + a.c00, a.c21 + a.c01);
    let y = m.to_screen(a.c20 + a.c10, a.c21 + a.c11);
    [o, x, y]
}

/// The lines of a transform control (`t`: origin, end of the X axis, end of the Y axis).
fn control_segments(shape: ControlShape, t: &[Pos2; 3]) -> Vec<(Pos2, Pos2)> {
    let [o, x, y] = *t;
    match shape {
        ControlShape::Triangle => vec![(o, x), (x, y), (y, o)],
        ControlShape::Rectangle => {
            let far = x + (y - o);
            vec![(o, x), (x, far), (far, y), (y, o)]
        }
        ControlShape::Axis => {
            let mut v = vec![(o, x), (o, y)];
            for tip in [x, y] {
                let d = (tip - o).normalized() * 9.0;
                if d.is_finite() {
                    let rot = |v: Vec2, a: f32| vec2(v.x * a.cos() - v.y * a.sin(), v.x * a.sin() + v.y * a.cos());
                    v.push((tip, tip - rot(d, 0.45)));
                    v.push((tip, tip - rot(d, -0.45)));
                }
            }
            v
        }
        ControlShape::Crosshair => vec![(o - (x - o), x), (o - (y - o), y)],
    }
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

fn hit_test(doc: &Document, m: &Mapper, p: Pos2, shape: ControlShape) -> Option<Hit> {
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
        let t = triangle(m, &a);
        let far = t[1] + (t[2] - t[0]);
        if point_in_triangle(p, &t) || (shape == ControlShape::Rectangle && point_in_triangle(p, &[t[1], far, t[2]])) {
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
    draw_overlays(&ui.painter_at(img_rect), doc, state, &mapper, img_rect);
    if state.detached
        && let Some(img) = &image
    {
        detached_preview(ui.ctx(), img.texture, flame_aspect, &mut state.detached);
    }

    // ---- interaction ----
    let pointer = resp.hover_pos();
    let modifiers = ui.input(|i| i.modifiers);
    let hover_hit = if state.show_handles && state.drag.is_none() { pointer.and_then(|p| hit_test(doc, &mapper, p, state.controls.shape)) } else { None };

    if resp.drag_started_by(egui::PointerButton::Primary) || resp.drag_started_by(egui::PointerButton::Middle) || resp.drag_started_by(egui::PointerButton::Secondary) {
        // Hit-test where the press started, not where the drag threshold was crossed.
        if let Some(p) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
            let world = mapper.to_world(p);
            let primary = resp.dragged_by(egui::PointerButton::Primary);
            let hit = if primary && state.show_handles { hit_test(doc, &mapper, p, state.controls.shape) } else { None };
            let kind = match hit {
                Some(h) => {
                    doc.selection = h.sel;
                    DragKind::Edit { sel: h.sel, handle: h.handle, rotate: modifiers.shift, scale: modifiers.command || modifiers.ctrl }
                }
                None if primary && state.focus_mode => DragKind::Focus,
                None => DragKind::Pan,
            };
            let start_affine = match kind {
                DragKind::Edit { sel, .. } => affine_of(doc, sel).unwrap_or_default(),
                DragKind::Pan | DragKind::Focus => Affine::IDENTITY,
            };
            state.drag = Some(Drag {
                kind,
                start_world: world,
                start_affine,
                start_centre: (doc.flame.centre_x, doc.flame.centre_y),
                start_focus: (doc.flame.focus_x, doc.flame.focus_y),
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
            DragKind::Focus => {
                let f = if state.fine { 0.1 } else { 1.0 };
                doc.flame.focus_x = drag.start_focus.0 + (wx - sx) * f;
                doc.flame.focus_y = drag.start_focus.1 + (wy - sy) * f;
            }
            DragKind::Edit { sel, handle, rotate, scale } => {
                let mut a = drag.start_affine;
                let (ox, oy) = (a.c20, a.c21);
                // fine mode: a tenth of the pointer movement
                let f = if state.fine { 0.1 } else { 1.0 };
                let (dx, dy) = ((wx - sx) * f, (wy - sy) * f);
                match handle {
                    Handle::Origin | Handle::Body if !rotate && !scale => {
                        a.c20 += dx;
                        a.c21 += dy;
                    }
                    Handle::AxisX if !rotate && !scale && state.fine => {
                        a.c00 += dx;
                        a.c01 += dy;
                    }
                    Handle::AxisY if !rotate && !scale && state.fine => {
                        a.c10 += dx;
                        a.c11 += dy;
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
                            a.rotate((a1 - a0).to_degrees() * f);
                        }
                        if scale {
                            let d0 = ((sx - ox).powi(2) + (sy - oy).powi(2)).sqrt().max(1e-9);
                            let d1 = ((wx - ox).powi(2) + (wy - oy).powi(2)).sqrt();
                            let s = (d1 / d0).powf(f);
                            if state.scale_x {
                                a.c00 *= s;
                                a.c01 *= s;
                            }
                            if state.scale_y {
                                a.c10 *= s;
                                a.c11 *= s;
                            }
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

    if resp.double_clicked() {
        match state.double_click {
            DoubleClick::Nothing => {}
            DoubleClick::OpenRender => state.open_render = true,
            DoubleClick::ToggleHandles => state.show_handles = !state.show_handles,
        }
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        if let Some(h) = hit_test(doc, &mapper, p, state.controls.shape).filter(|_| state.show_handles) {
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
            let style = state.controls;
            let color = match sel {
                Selection::Final(_) => Color32::from_rgb(0xd0, 0xd4, 0xdc),
                Selection::Background(_) => Color32::from_rgb(0xc8, 0xa8, 0x70),
                _ if !style.color => Color32::from_rgb(0xe4, 0xe6, 0xeb),
                _ => xform_color(idx),
            };
            let selected = sel == doc.selection;
            let hovered = hover_hit.is_some_and(|h| h.sel == sel);
            let alpha = if selected { 255 } else if hovered { 200 } else { 110 };
            let c = color.gamma_multiply(alpha as f32 / 255.0);
            let far = t[1] + (t[2] - t[0]);
            let outline: Vec<Pos2> = match style.shape {
                ControlShape::Rectangle => vec![t[0], t[1], far, t[2]],
                _ => t.to_vec(),
            };
            if selected && matches!(style.shape, ControlShape::Triangle | ControlShape::Rectangle) {
                painter.add(Shape::convex_polygon(outline.clone(), color.gamma_multiply(0.12), Stroke::NONE));
            }
            let width = if selected { 2.0 } else { 1.2 };
            let dashed = matches!(sel, Selection::Final(_) | Selection::Background(_)) || doc.edit_post;
            for (stroke, shadow) in [(Stroke::new(width + 2.0, Color32::from_black_alpha(140)), true), (Stroke::new(width, c), false)] {
                if shadow && !style.shadows {
                    continue;
                }
                for (p, q) in control_segments(style.shape, &t) {
                    if dashed && !shadow {
                        painter.extend(Shape::dashed_line(&[p, q], stroke, 6.0, 4.0));
                    } else {
                        painter.line_segment([p, q], stroke);
                    }
                }
            }
            if style.numbers {
                let label = match sel {
                    Selection::XForm(i) => (i + 1).to_string(),
                    Selection::Final(i) => format!("F{}", i + 1),
                    Selection::Background(i) => format!("B{}", i + 1),
                    Selection::None => String::new(),
                };
                let centre = match style.shape {
                    ControlShape::Rectangle => pos2((t[0].x + far.x) / 2.0, (t[0].y + far.y) / 2.0),
                    _ => pos2((t[0].x + t[1].x + t[2].x) / 3.0, (t[0].y + t[1].y + t[2].y) / 3.0),
                };
                if style.shadows {
                    painter.text(centre + vec2(1.0, 1.0), Align2::CENTER_CENTER, &label, FontId::proportional(12.0), Color32::from_black_alpha(200));
                }
                painter.text(centre, Align2::CENTER_CENTER, &label, FontId::proportional(12.0), c);
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
        let (source, aux) = preview.read_denoise_input(&doc.flame);
        state.denoised.start(key, &doc.flame, img, source, w, h, on_gpu, aux);
    }
    state.denoised.is_current(key).then(|| state.denoised.texture()).flatten()
}

/// Grid, guides, variation effect and focus point.
fn draw_overlays(painter: &egui::Painter, doc: &Document, state: &mut CanvasState, m: &Mapper, img_rect: Rect) {
    if state.show_grid && state.grid_size > 1e-6 {
        let step = state.grid_size;
        let (a, b) = (m.to_world(img_rect.left_top()), m.to_world(img_rect.right_bottom()));
        let (x0, x1, y0, y1) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
        // no more than ~200 lines per direction
        if (x1 - x0) / step < 200.0 && (y1 - y0) / step < 200.0 {
            let line = |v: f64| if v.abs() < step * 1e-6 { Stroke::new(1.2, Color32::from_white_alpha(110)) } else { Stroke::new(1.0, Color32::from_white_alpha(40)) };
            let mut x = (x0 / step).floor() * step;
            while x <= x1 {
                painter.line_segment([m.to_screen(x, y0), m.to_screen(x, y1)], line(x));
                x += step;
            }
            let mut y = (y0 / step).floor() * step;
            while y <= y1 {
                painter.line_segment([m.to_screen(x0, y), m.to_screen(x1, y)], line(y));
                y += step;
            }
        }
    }
    if state.show_guides {
        // as in JWildfire (from Apophysis): center, rule of thirds, golden ratio
        let r = img_rect;
        let both = |f: f32, color: Color32| {
            let s = Stroke::new(1.2, color);
            for t in [f, 1.0 - f] {
                let x = r.left() + r.width() * t;
                let y = r.top() + r.height() * t;
                painter.line_segment([pos2(x, r.top()), pos2(x, r.bottom())], s);
                painter.line_segment([pos2(r.left(), y), pos2(r.right(), y)], s);
            }
        };
        both(0.5, Color32::from_white_alpha(150));
        both(1.0 / 3.0, Color32::from_rgba_unmultiplied(255, 60, 60, 150));
        both(0.618_034, Color32::from_rgba_unmultiplied(60, 255, 60, 150));
    }
    if state.show_var_effect {
        let key_ok = state.var_mesh.as_ref().is_some_and(|(f, l, s, _)| *l == doc.layer && *s == doc.selection && *f == doc.flame);
        if !key_ok {
            state.var_mesh = variation_mesh(doc).map(|mesh| (doc.flame.clone(), doc.layer, doc.selection, mesh));
        }
        if let Some((_, _, _, mesh)) = &state.var_mesh {
            let n = VAR_GRID;
            let s = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 200, 80, 120));
            let ok = |p: [f64; 2]| p[0].is_finite() && p[1].is_finite() && p[0].abs() < 1e4 && p[1].abs() < 1e4;
            for y in 0..n {
                for x in 0..n {
                    let p = mesh[y * n + x];
                    for q in [(x + 1 < n).then(|| mesh[y * n + x + 1]), (y + 1 < n).then(|| mesh[(y + 1) * n + x])].into_iter().flatten() {
                        if ok(p) && ok(q) {
                            painter.line_segment([m.to_screen(p[0], p[1]), m.to_screen(q[0], q[1])], s);
                        }
                    }
                }
            }
        }
    } else {
        state.var_mesh = None;
    }
    if state.focus_mode {
        let c = m.to_screen(doc.flame.focus_x, doc.flame.focus_y);
        let s = Stroke::new(1.5, theme::ACCENT);
        painter.circle_stroke(c, 9.0, s);
        painter.line_segment([c - vec2(16.0, 0.0), c + vec2(16.0, 0.0)], s);
        painter.line_segment([c - vec2(0.0, 16.0), c + vec2(0.0, 16.0)], s);
    }
}

/// Points per side of the variation effect grid.
const VAR_GRID: usize = 32;

/// The selected transform applied to a 32 × 32 grid over [-1, 1]² (JWildfire's "variation
/// effect" display), row by row.
fn variation_mesh(doc: &Document) -> Option<Vec<[f64; 2]>> {
    use rwf_core::model::XFormKind;
    let (kind, i) = doc.selection.kind()?;
    let prepared = rwf_core::render::PreparedFlame::new(&doc.flame);
    let layer = prepared.layers.get(doc.layer)?;
    let xf = match kind {
        XFormKind::Normal => layer.xforms.get(i)?,
        XFormKind::Final => layer.finals.get(i)?,
        XFormKind::Background => return None,
    };
    let mut ctx = rwf_core::variations::Ctx::new(1, false);
    let n = VAR_GRID;
    let mut out = Vec::with_capacity(n * n);
    for yi in 0..n {
        for xi in 0..n {
            let p = rwf_core::variations::Pt { x: -1.0 + 2.0 * xi as f64 / (n - 1) as f64, y: -1.0 + 2.0 * yi as f64 / (n - 1) as f64, ..Default::default() };
            let q = xf.transform(&mut ctx, &layer.colormap, layer.smooth_gradient, &p);
            out.push([q.x, q.y]);
        }
    }
    Some(out)
}

/// The preview in a window of its own (JWildfire's detached preview).
fn detached_preview(ctx: &egui::Context, texture: egui::TextureId, aspect: f32, open: &mut bool) {
    let id = egui::ViewportId::from_hash_of("rwf-detached-preview");
    let builder = egui::ViewportBuilder::default().with_title("RWildFire preview").with_inner_size([800.0, 800.0 / aspect.max(0.1)]);
    ctx.show_viewport_immediate(id, builder, |ctx, _class| {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::CANVAS)).show(ctx, |ui| {
            let r = fit_rect(ui.available_rect_before_wrap(), aspect);
            ui.painter().image(texture, r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        });
        if ctx.input(|i| i.viewport().close_requested()) {
            *open = false;
        }
    });
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
            let label = format!("{}{}", doc.edit_plane.label(), if state.fine { " · fine" } else { "" });
            ui.menu_button(label, |ui| {
                ui.label(RichText::new("Plane").small().color(theme::TEXT_MUTED));
                for p in crate::doc::EditPlane::ALL {
                    ui.radio_value(&mut doc.edit_plane, p, p.label()).on_hover_text(match p {
                        crate::doc::EditPlane::XY => "The usual affine",
                        _ => "The extra affine of 3D transforms in this plane",
                    });
                }
                ui.separator();
                ui.checkbox(&mut state.fine, "Fine movement").on_hover_text("Dragging moves, turns and scales at a tenth of the speed");
                ui.label(RichText::new("Ctrl-drag scales").small().color(theme::TEXT_MUTED));
                ui.checkbox(&mut state.scale_x, "X axis");
                ui.checkbox(&mut state.scale_y, "Y axis");
                ui.separator();
                ui.label(RichText::new(crate::icons::arrows("Keys: arrows move, Alt+←/→ turn, Alt+↑/↓ scale;\nCtrl ×0.1, Shift ×10")).small().color(theme::TEXT_MUTED));
            })
            .response
            .on_hover_text("Edit plane, fine movement and scale axes");
        });
        ui.toggle_value(&mut state.show_camera_gizmo, format!("{} Camera", icons::GLOBE)).on_hover_text("Show the camera orbit globe on the canvas");
        ui.toggle_value(&mut state.show_denoised, format!("{} Denoised", icons::SPARKLE))
            .on_hover_text("Show the finished preview through the flame's AI denoiser, as saved images get it");
        ui.menu_button("View", |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut state.show_grid, "Grid").on_hover_text("Lines in world units, the zero lines brighter");
                ui.add_enabled(state.show_grid, egui::DragValue::new(&mut state.grid_size).speed(0.01).range(0.01..=100.0)).on_hover_text("Grid spacing");
            });
            ui.checkbox(&mut state.show_guides, "Guides").on_hover_text("Center (white), rule of thirds (red) and golden ratio (green)");
            ui.checkbox(&mut state.show_var_effect, "Variation effect").on_hover_text("How the selected transform bends a grid over [-1, 1]²");
            ui.checkbox(&mut state.focus_mode, "Focus point").on_hover_text("Dragging on the canvas moves the depth-of-field focus point (\"Focus point\" DOF mode)");
            ui.separator();
            ui.checkbox(&mut state.detached, "Detached preview").on_hover_text("Shows the preview in a window of its own, e.g. on a second screen");
        });
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
    let text = if preview.is_converged() && density < target {
        format!("Quality {:.0}  ·  time limit reached", density)
    } else if preview.is_converged() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variation_mesh_applies_the_transform() {
        let mut flame = Flame::default();
        let mut xf = rwf_core::XForm::with_variation("linear3D", 1.0);
        xf.coefs.scale(0.5);
        xf.coefs.c20 = 0.25;
        flame.first_layer_mut().xforms.push(xf);
        let mut doc = Document::new(flame);
        doc.selection = Selection::XForm(0);
        let mesh = variation_mesh(&doc).unwrap();
        assert_eq!(mesh.len(), VAR_GRID * VAR_GRID);
        // corner (-1, -1) -> 0.5 * (-1, -1) + (0.25, 0)
        assert!((mesh[0][0] + 0.25).abs() < 1e-9 && (mesh[0][1] + 0.5).abs() < 1e-9, "{:?}", mesh[0]);
        doc.selection = Selection::None;
        assert!(variation_mesh(&doc).is_none());
    }
}
