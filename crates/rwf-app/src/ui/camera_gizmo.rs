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

//! Camera gizmos: an orbit globe for pitch/yaw/bank with a rotation ring, and thumbwheels for
//! unbounded values such as scale and zoom.
//!
//! The globe works on the camera matrix only, so it does not depend on the Euler angle
//! convention: ring axes are derived numerically and a free trackball drag is solved back into
//! yaw/pitch/bank with a small least-squares fit.

use std::f64::consts::PI;

use egui::{Align2, Color32, FontId, Id, Pos2, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};
use rwf_core::Flame;
use rwf_core::render::view::camera_matrix;

use crate::theme;

type V3 = [f64; 3];
/// Camera-space matrix: `c = M * w` (rows = camera axes).
type M3 = [[f64; 3]; 3];

const YAW: usize = 0;
const PITCH: usize = 1;
const BANK: usize = 2;
const ROLL: usize = 3;
const NAMES: [&str; 4] = ["Yaw", "Pitch", "Bank", "Rotation"];

const RING_COLORS: [Color32; 3] = [
    Color32::from_rgb(0x4d, 0x9e, 0xff), // yaw: around the flame plane normal (world Z)
    Color32::from_rgb(0xf0, 0x5a, 0x5a), // pitch
    Color32::from_rgb(0x6c, 0xd0, 0x5c), // bank
];
const AXIS_COLORS: [Color32; 3] = [Color32::from_rgb(0xf0, 0x5a, 0x5a), Color32::from_rgb(0x6c, 0xd0, 0x5c), Color32::from_rgb(0x4d, 0x9e, 0xff)];
const SNAP_DEG: f64 = 15.0;

fn angles(f: &Flame) -> [f64; 4] {
    [f.cam_yaw, f.cam_pitch, f.cam_bank, f.cam_roll]
}

fn set_angles(f: &mut Flame, a: [f64; 4]) {
    f.cam_yaw = a[YAW];
    f.cam_pitch = a[PITCH];
    f.cam_bank = a[BANK];
    f.cam_roll = a[ROLL];
}

/// World to camera rotation for the angles (degrees).
fn cam(a: [f64; 4]) -> M3 {
    let m = camera_matrix(a[YAW], a[PITCH], a[BANK], a[ROLL]);
    let mut c = [[0.0; 3]; 3];
    for (i, row) in m.iter().enumerate() {
        for (j, v) in row.iter().enumerate() {
            c[j][i] = *v;
        }
    }
    c
}

fn mul(a: &M3, b: &M3) -> M3 {
    let mut r = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            r[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    r
}

fn apply(m: &M3, v: V3) -> V3 {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// Rotation by `angle` (radians) around the unit `axis`.
fn rotation(axis: V3, angle: f64) -> M3 {
    let (s, c) = angle.sin_cos();
    let t = 1.0 - c;
    let [x, y, z] = axis;
    [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ]
}

/// Camera-space rotation axis that a change of angle `k` produces (per radian).
fn param_axis(a: [f64; 4], k: usize) -> V3 {
    let h = 0.01;
    let (mut lo, mut hi) = (a, a);
    lo[k] -= h;
    hi[k] += h;
    let (c0, c1, c) = (cam(lo), cam(hi), cam(a));
    let dr = 2.0 * h * PI / 180.0;
    // dC = [w]x C  =>  [w]x = dC * C^T
    let mut w = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            w[i][j] = (0..3).map(|k| (c1[i][k] - c0[i][k]) / dr * c[j][k]).sum();
        }
    }
    let v = [(w[2][1] - w[1][2]) * 0.5, (w[0][2] - w[2][0]) * 0.5, (w[1][0] - w[0][1]) * 0.5];
    let n = norm(v).max(1e-12);
    v.map(|x| x / n)
}

/// Finds yaw/pitch/bank (roll fixed) whose camera matrix best matches `target`, starting from `a`.
fn solve(target: &M3, mut a: [f64; 4]) -> [f64; 4] {
    let residual = |a: [f64; 4]| {
        let c = cam(a);
        let mut r = [0.0; 9];
        for i in 0..3 {
            for j in 0..3 {
                r[i * 3 + j] = c[i][j] - target[i][j];
            }
        }
        r
    };
    let mut lambda = 1e-3;
    let mut r = residual(a);
    let mut err: f64 = r.iter().map(|x| x * x).sum();
    for _ in 0..30 {
        if err < 1e-14 {
            break;
        }
        let h = 1e-3;
        let mut jac = [[0.0; 3]; 9];
        for (p, k) in [YAW, PITCH, BANK].into_iter().enumerate() {
            let mut b = a;
            b[k] += h;
            let rb = residual(b);
            for i in 0..9 {
                jac[i][p] = (rb[i] - r[i]) / h;
            }
        }
        let mut jtj = [[0.0; 3]; 3];
        let mut jtr = [0.0; 3];
        for i in 0..9 {
            for p in 0..3 {
                jtr[p] += jac[i][p] * r[i];
                for q in 0..3 {
                    jtj[p][q] += jac[i][p] * jac[i][q];
                }
            }
        }
        loop {
            let mut m = jtj;
            for (p, row) in m.iter_mut().enumerate() {
                row[p] += lambda * (1.0 + jtj[p][p]);
            }
            let Some(d) = solve3(m, jtr) else { break };
            let mut b = a;
            for (p, k) in [YAW, PITCH, BANK].into_iter().enumerate() {
                b[k] -= d[p];
            }
            let rb = residual(b);
            let eb: f64 = rb.iter().map(|x| x * x).sum();
            if eb < err {
                a = b;
                r = rb;
                err = eb;
                lambda = (lambda * 0.3).max(1e-9);
                break;
            }
            lambda *= 10.0;
            if lambda > 1e8 {
                return a;
            }
        }
    }
    a
}

fn solve3(m: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(&m);
    if d.abs() < 1e-18 {
        return None;
    }
    let mut x = [0.0; 3];
    for (c, xc) in x.iter_mut().enumerate() {
        let mut mc = m;
        for r in 0..3 {
            mc[r][c] = b[r];
        }
        *xc = det(&mc) / d;
    }
    Some(x)
}

fn wrap_deg(v: f64) -> f64 {
    let w = (v + 180.0).rem_euclid(360.0) - 180.0;
    if w <= -180.0 + 1e-9 { 180.0 } else { w }
}

fn snap(v: f64, on: bool) -> f64 {
    if on { (v / SNAP_DEG).round() * SNAP_DEG } else { v }
}

/// Angles that turn the world axis `axis * sign` towards the viewer, keeping the rotation;
/// prefers the smallest angles.
fn facing(axis: usize, sign: f64, roll: f64) -> [f64; 4] {
    let steps = [0.0, 90.0, -90.0, 180.0];
    let mut best = ([0.0, 0.0, 0.0, roll], f64::MAX);
    for y in steps {
        for p in steps {
            for b in steps {
                let a = [y, p, b, roll];
                let mut w = [0.0; 3];
                w[axis] = sign;
                let c = apply(&cam(a), w);
                if c[2] > 0.999 {
                    let cost = y.abs() + p.abs() + b.abs();
                    if cost < best.1 {
                        best = (a, cost);
                    }
                }
            }
        }
    }
    best.0
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Target {
    Axis(usize, bool),
    Ring(usize),
    Roll,
    Ball,
}

#[derive(Clone, Copy, Debug)]
struct GlobeDrag {
    target: Target,
    start: [f64; 4],
    start_pos: Pos2,
    /// Screen motion of the grabbed ring point per radian (rings only).
    tangent: Vec2,
    /// Accumulated rotation-ring angle in degrees.
    acc: f64,
}

struct Geometry {
    center: Pos2,
    r: f32,
    roll_r: f32,
    cam: M3,
    axes: [V3; 3],
}

impl Geometry {
    fn screen(&self, c: V3) -> Pos2 {
        self.center + vec2(c[0] as f32, c[1] as f32) * self.r
    }

    /// Points of the unit circle perpendicular to `n` (camera space).
    fn ring(&self, n: V3, segments: usize) -> Vec<V3> {
        let helper = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let u = cross(n, helper);
        let u = u.map(|x| x / norm(u));
        let v = cross(n, u);
        (0..=segments)
            .map(|i| {
                let t = i as f64 / segments as f64 * 2.0 * PI;
                let (s, c) = t.sin_cos();
                [0, 1, 2].map(|k| u[k] * c + v[k] * s)
            })
            .collect()
    }

    fn roll_knob(&self, roll: f64) -> Pos2 {
        let phi = (roll - 90.0).to_radians() as f32;
        self.center + vec2(phi.cos(), phi.sin()) * self.roll_r
    }

    fn axis_dot(&self, i: usize, positive: bool) -> (Pos2, f64) {
        let s = if positive { 1.0 } else { -1.0 };
        let c = self.cam;
        let v = [c[0][i] * s, c[1][i] * s, c[2][i] * s];
        (self.screen(v.map(|x| x * 0.86)), v[2])
    }

    fn hit(&self, p: Pos2, compact: bool) -> Option<Target> {
        let dot_r = if compact { 7.0 } else { 9.0 };
        let mut dots: Vec<(Target, f32, f64)> = Vec::new();
        for i in 0..3 {
            for positive in [true, false] {
                let (q, depth) = self.axis_dot(i, positive);
                dots.push((Target::Axis(i, positive), q.distance(p), depth));
            }
        }
        // Nearest dot in front wins.
        dots.sort_by(|a, b| b.2.total_cmp(&a.2));
        if let Some(d) = dots.iter().find(|d| d.1 <= dot_r) {
            return Some(d.0);
        }
        let mut best: Option<(Target, f32)> = None;
        for (k, n) in self.axes.iter().enumerate() {
            let pts = self.ring(*n, 72);
            // A ring seen edge-on is a line through the ball; grab it only near the rim so
            // the middle stays free for orbiting.
            let edge_on = n[2].abs() < 0.3;
            for w in pts.windows(2) {
                // Grabbing a ring works on its front half.
                if w[0][2] + w[1][2] < -0.1 {
                    continue;
                }
                if edge_on && (w[0][0].hypot(w[0][1]) < 0.6 || w[1][0].hypot(w[1][1]) < 0.6) {
                    continue;
                }
                let d = seg_dist(p, self.screen(w[0]), self.screen(w[1]));
                if d < 5.0 && best.is_none_or(|b| d < b.1) {
                    best = Some((Target::Ring(k), d));
                }
            }
        }
        if let Some((t, _)) = best {
            return Some(t);
        }
        let d = p.distance(self.center);
        if (d - self.roll_r).abs() <= 6.0 {
            return Some(Target::Roll);
        }
        if d <= self.r {
            return Some(Target::Ball);
        }
        None
    }
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Orbit globe: drag the ball to tumble the camera, drag a colored ring to change one of
/// yaw/pitch/bank, drag the outer ring to rotate, click an axis dot to look along it,
/// double-click to reset. Shift snaps to 15° steps. `compact` draws a smaller variant for
/// the canvas overlay.
pub fn orbit_globe(ui: &mut Ui, f: &mut Flame, radius: f32, compact: bool) -> Response {
    let pad = if compact { 12.0 } else { 16.0 };
    let size = 2.0 * (radius + pad);
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let id = resp.id;
    let a = angles(f);
    let geo =
        Geometry { center: rect.center(), r: radius, roll_r: radius + pad * 0.62, cam: cam(a), axes: [YAW, PITCH, BANK].map(|k| param_axis(a, k)) };
    let shift = ui.input(|i| i.modifiers.shift);

    // ---- interaction ----
    let mut drag: Option<GlobeDrag> = ui.data(|d| d.get_temp(id));
    if resp.drag_started()
        && let Some(p) = ui.input(|i| i.pointer.press_origin())
        && let Some(target) = geo.hit(p, compact)
    {
        // Axis dots are for clicking; dragging them orbits.
        let target = if matches!(target, Target::Axis(..)) { Target::Ball } else { target };
        let tangent = match target {
            Target::Ring(k) => {
                // Nearest ring point to the grab position and its screen velocity.
                let n = geo.axes[k];
                let g = geo
                    .ring(n, 180)
                    .into_iter()
                    .filter(|c| c[2] > -0.1)
                    .min_by(|x, y| geo.screen(*x).distance(p).total_cmp(&geo.screen(*y).distance(p)));
                let v = g.map(|g| cross(n, g)).unwrap_or([0.0; 3]);
                let t = vec2(v[0] as f32, v[1] as f32) * radius;
                if t.length() < 0.3 * radius { t.normalized() * 0.3 * radius } else { t }
            }
            _ => Vec2::ZERO,
        };
        drag = Some(GlobeDrag { target, start: a, start_pos: p, tangent, acc: 0.0 });
    }
    if let Some(dr) = drag.as_mut()
        && resp.dragged()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let d = p - dr.start_pos;
        let mut n = dr.start;
        match dr.target {
            Target::Ball => {
                let len = d.length() as f64;
                if len > 0.5 {
                    // The front of the ball follows the pointer.
                    let axis = [-d.y as f64 / len, d.x as f64 / len, 0.0];
                    let target = mul(&rotation(axis, len / radius as f64), &cam(dr.start));
                    n = solve(&target, a);
                    n[ROLL] = dr.start[ROLL];
                    for k in [YAW, PITCH, BANK] {
                        n[k] = snap(wrap_deg(n[k]), shift);
                    }
                }
            }
            Target::Ring(k) => {
                let t = dr.tangent;
                let delta = (d.dot(t) / t.length_sq()) as f64;
                n[[YAW, PITCH, BANK][k]] = snap(wrap_deg(dr.start[[YAW, PITCH, BANK][k]] + delta.to_degrees()), shift);
            }
            Target::Roll => {
                let prev = p - resp.drag_delta();
                let a0 = (prev - geo.center).angle();
                let a1 = (p - geo.center).angle();
                let mut da = (a1 - a0) as f64;
                if da > PI {
                    da -= 2.0 * PI;
                } else if da < -PI {
                    da += 2.0 * PI;
                }
                dr.acc += da.to_degrees();
                n[ROLL] = snap(wrap_deg(dr.start[ROLL] + dr.acc), shift);
            }
            Target::Axis(..) => {}
        }
        if n != a {
            set_angles(f, n);
        }
    }
    if resp.drag_stopped() {
        drag = None;
    }
    ui.data_mut(|d| match drag {
        Some(dr) => {
            d.insert_temp(id, dr);
        }
        None => d.remove::<GlobeDrag>(id),
    });

    let hover = if drag.is_none() { resp.hover_pos().and_then(|p| geo.hit(p, compact)) } else { None };
    if resp.clicked()
        && let Some(Target::Axis(i, positive)) = hover
    {
        set_angles(f, facing(i, if positive { 1.0 } else { -1.0 }, f.cam_roll));
    }
    if resp.double_clicked() {
        match hover {
            Some(Target::Ball) => set_angles(f, [0.0, 0.0, 0.0, f.cam_roll]),
            Some(Target::Roll) => f.cam_roll = 0.0,
            Some(Target::Ring(k)) => set_angles(f, {
                let mut n = angles(f);
                n[[YAW, PITCH, BANK][k]] = 0.0;
                n
            }),
            _ => {}
        }
    }
    let active = drag.map(|d| d.target).or(hover);
    match active {
        Some(Target::Axis(..)) => ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand),
        Some(_) if drag.is_some() => ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing),
        Some(_) => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
        None => {}
    }

    // ---- drawing (with the possibly updated angles) ----
    let a = angles(f);
    let geo = Geometry { cam: cam(a), axes: [YAW, PITCH, BANK].map(|k| param_axis(a, k)), ..geo };
    let painter = ui.painter_at(rect);
    let lit = resp.hovered() || drag.is_some();
    let base_alpha = if compact && !lit { 0.55 } else { 1.0 };
    let fade = |c: Color32, k: f32| c.gamma_multiply(k * base_alpha);

    painter.circle_filled(geo.center, geo.roll_r + 5.0, fade(theme::PANEL, 0.9));
    painter.circle(geo.center, geo.r, fade(theme::SURFACE, 1.0), Stroke::new(1.0, fade(theme::BORDER, 1.0)));

    // Globe grid: meridians and parallels around world Z.
    if !compact {
        let grid = fade(theme::TEXT_MUTED, 0.22);
        let grid_back = fade(theme::TEXT_MUTED, 0.07);
        let mut lines: Vec<Vec<V3>> = Vec::new();
        for m in 0..6 {
            let t = m as f64 * PI / 6.0;
            lines.push(
                (0..=48)
                    .map(|i| {
                        let u = i as f64 / 48.0 * 2.0 * PI;
                        [u.sin() * t.cos(), u.sin() * t.sin(), u.cos()]
                    })
                    .collect(),
            );
        }
        for lat in [-60.0f64, -30.0, 30.0, 60.0] {
            let (s, c) = lat.to_radians().sin_cos();
            lines.push(
                (0..=48)
                    .map(|i| {
                        let u = i as f64 / 48.0 * 2.0 * PI;
                        [c * u.cos(), c * u.sin(), s]
                    })
                    .collect(),
            );
        }
        for l in lines {
            for w in l.windows(2) {
                let (p, q) = (apply(&geo.cam, w[0]), apply(&geo.cam, w[1]));
                let col = if p[2] + q[2] >= 0.0 { grid } else { grid_back };
                painter.line_segment([geo.screen(p), geo.screen(q)], Stroke::new(1.0, col));
            }
        }
    }

    // Rings: back halves first, then dots behind, then front halves.
    let ring_pts: Vec<Vec<V3>> = geo.axes.iter().map(|n| geo.ring(*n, 96)).collect();
    let ring_width = |k: usize| {
        let on = active == Some(Target::Ring(k));
        if on {
            3.0
        } else if compact {
            1.4
        } else {
            2.0
        }
    };
    let draw_ring = |front: bool| {
        for (k, pts) in ring_pts.iter().enumerate() {
            let col = if front { fade(RING_COLORS[k], 1.0) } else { fade(RING_COLORS[k], 0.25) };
            for w in pts.windows(2) {
                if (w[0][2] + w[1][2] >= 0.0) == front {
                    painter.line_segment([geo.screen(w[0]), geo.screen(w[1])], Stroke::new(ring_width(k), col));
                }
            }
        }
    };
    let mut dots: Vec<(usize, bool, Pos2, f64)> = Vec::new();
    for i in 0..3 {
        for positive in [true, false] {
            let (q, depth) = geo.axis_dot(i, positive);
            dots.push((i, positive, q, depth));
        }
    }
    dots.sort_by(|x, y| x.3.total_cmp(&y.3));
    let dot_r = if compact { 5.5 } else { 7.5 };
    let draw_dot = |&(i, positive, q, _): &(usize, bool, Pos2, f64)| {
        let hot = active == Some(Target::Axis(i, positive));
        let r = if hot { dot_r + 1.5 } else { dot_r };
        if positive {
            painter.circle(q, r, fade(AXIS_COLORS[i], 1.0), Stroke::new(if hot { 1.5 } else { 0.0 }, fade(Color32::WHITE, 1.0)));
            if !compact || hot {
                painter.text(
                    q,
                    Align2::CENTER_CENTER,
                    ["X", "Y", "Z"][i],
                    FontId::proportional(theme::fs(if compact { 8.5 } else { 10.0 })),
                    fade(Color32::BLACK, 1.0),
                );
            }
        } else {
            painter.circle(q, r * 0.8, fade(AXIS_COLORS[i], 0.3), Stroke::new(1.0, fade(AXIS_COLORS[i], if hot { 1.0 } else { 0.7 })));
        }
    };
    draw_ring(false);
    dots.iter().filter(|d| d.3 < 0.0).for_each(draw_dot);
    draw_ring(true);
    dots.iter().filter(|d| d.3 >= 0.0).for_each(draw_dot);

    // Rotation ring with knob.
    let roll_on = active == Some(Target::Roll);
    painter.circle_stroke(
        geo.center,
        geo.roll_r,
        Stroke::new(if roll_on { 2.5 } else { 1.5 }, fade(theme::TEXT_MUTED, if roll_on { 1.0 } else { 0.6 })),
    );
    let knob = geo.roll_knob(a[ROLL]);
    painter.circle(knob, if roll_on { 5.0 } else { 4.0 }, fade(theme::ACCENT, 1.0), Stroke::new(1.0, fade(theme::PANEL, 1.0)));

    // Value readout of what is being dragged / hovered.
    if let Some(t) = active {
        let text = match t {
            Target::Ring(k) => {
                let k = [YAW, PITCH, BANK][k];
                format!("{} {:.1}°", NAMES[k], a[k])
            }
            Target::Roll => format!("{} {:.1}°", NAMES[ROLL], a[ROLL]),
            Target::Ball => format!("P {:.0}°  Y {:.0}°  B {:.0}°", a[PITCH], a[YAW], a[BANK]),
            Target::Axis(i, positive) => format!("View along {}{}", if positive { "+" } else { "−" }, ["X", "Y", "Z"][i]),
        };
        let pos = pos2(geo.center.x, rect.bottom() - 1.0);
        let galley = painter.layout_no_wrap(text, FontId::proportional(theme::fs(10.5)), theme::TEXT);
        let bg = egui::Rect::from_center_size(pos - vec2(0.0, galley.size().y * 0.5), galley.size() + vec2(8.0, 2.0));
        painter.rect_filled(bg, 4, theme::PANEL.gamma_multiply(0.9));
        painter.galley(bg.min + vec2(4.0, 1.0), galley, theme::TEXT);
    }

    let mut resp = if compact || drag.is_some() {
        resp
    } else {
        resp.on_hover_text(
            "Drag the ball to orbit · drag a ring: yaw (blue), pitch (red), bank (green) · outer ring: rotation\nClick an axis to look along it · double-click to reset · Shift: 15° steps",
        )
    };
    if drag.is_some() {
        resp.mark_changed();
    }
    resp
}

/// How a [`thumbwheel`] maps pointer motion to the value.
#[derive(Clone, Copy)]
pub enum Wheel {
    /// Multiplies the value by `e` every `n` pixels (for scale and zoom).
    Log(f64),
    /// Adds the amount per pixel.
    Linear(f64),
}

/// An endless thumbwheel: drag sideways to change an unbounded value. The ridges follow the
/// value, so changes made elsewhere move the wheel too. Shift = fine, double-click = `default`.
pub fn thumbwheel(ui: &mut Ui, v: &mut f64, wheel: Wheel, range: std::ops::RangeInclusive<f64>, default: f64) -> Response {
    let w = 110.0f32.min(ui.available_width() - 70.0).max(60.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 18.0), Sense::click_and_drag());
    let fine = if ui.input(|i| i.modifiers.shift) { 0.1 } else { 1.0 };
    let mut changed = false;
    if resp.dragged() {
        let dx = resp.drag_delta().x as f64 * fine;
        if dx != 0.0 {
            let nv = match wheel {
                Wheel::Log(n) => *v * (dx / n).exp(),
                Wheel::Linear(s) => *v + dx * s,
            };
            *v = nv.clamp(*range.start(), *range.end());
            changed = true;
        }
    }
    if resp.double_clicked() {
        *v = default;
        changed = true;
    }
    let lit = resp.hovered() || resp.dragged();
    if lit {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }

    // Ridges on a cylinder, phase from the value.
    let phase = match wheel {
        Wheel::Log(n) => v.max(1e-12).ln() * n,
        Wheel::Linear(s) => *v / s,
    } as f32;
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4, theme::SURFACE);
    let half = rect.width() * 0.5;
    let spacing = 9.0f32;
    let circumference = std::f32::consts::PI * half;
    let start = -(phase.rem_euclid(spacing));
    let mut x = start - circumference * 0.5;
    while x < circumference * 0.5 + spacing {
        let theta = x / half;
        if theta.abs() < std::f32::consts::FRAC_PI_2 {
            let sx = rect.center().x + half * theta.sin();
            let lightness = theta.cos();
            let col = if lit { theme::TEXT } else { theme::TEXT_MUTED }.gamma_multiply(0.15 + 0.6 * lightness);
            painter.line_segment([pos2(sx, rect.top() + 3.0), pos2(sx, rect.bottom() - 3.0)], Stroke::new(1.0 + lightness, col));
        }
        x += spacing;
    }
    // Edge shading.
    for (i, k) in [0.7f32, 0.45, 0.25].iter().enumerate() {
        let bw = 4.0 + i as f32 * 4.0;
        let l = egui::Rect::from_min_size(rect.min, vec2(bw, rect.height()));
        let r = egui::Rect::from_min_size(pos2(rect.right() - bw, rect.top()), vec2(bw, rect.height()));
        painter.rect_filled(l, 4, theme::BG.gamma_multiply(*k * 0.5));
        painter.rect_filled(r, 4, theme::BG.gamma_multiply(*k * 0.5));
    }
    painter.line_segment([pos2(rect.center().x, rect.top()), pos2(rect.center().x, rect.top() + 3.0)], Stroke::new(2.0, theme::ACCENT));
    painter.rect_stroke(rect, 4, Stroke::new(1.0, if lit { theme::TEXT_MUTED } else { theme::BORDER }), egui::StrokeKind::Inside);
    let mut resp = resp.on_hover_text("Drag sideways · Shift: fine · double-click: reset");
    if changed {
        resp.mark_changed();
    }
    resp
}

/// Grid row: label, thumbwheel and a value field for exact input.
pub fn wheel_row(
    ui: &mut Ui,
    text: &str,
    v: &mut f64,
    wheel: Wheel,
    range: std::ops::RangeInclusive<f64>,
    default: f64,
    decimals: usize,
) -> Response {
    super::widgets::label(ui, text);
    let r = ui
        .horizontal(|ui| {
            let r = thumbwheel(ui, v, wheel, range.clone(), default);
            let speed = match wheel {
                Wheel::Log(_) => (v.abs() * 0.005).max(1e-4),
                Wheel::Linear(s) => s,
            };
            r | ui.add(egui::DragValue::new(v).speed(speed).range(range).max_decimals(decimals))
        })
        .inner;
    ui.end_row();
    r
}

/// Compact row of exact angle fields for the globe.
pub fn angle_fields(ui: &mut Ui, f: &mut Flame) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for (name, k) in [("P", PITCH), ("Y", YAW), ("B", BANK), ("R", ROLL)] {
            let mut a = angles(f);
            ui.label(egui::RichText::new(name).color(if k == ROLL { theme::ACCENT } else { RING_COLORS[[1, 0, 2, 0][k]] }).small())
                .on_hover_text(NAMES[k]);
            if ui.add(egui::DragValue::new(&mut a[k]).speed(0.5).suffix("°").max_decimals(2)).on_hover_text(NAMES[k]).changed() {
                set_angles(f, a);
            }
        }
    });
}

/// Canvas overlay variant: a small globe anchored at `pos` (top-left of its square).
pub fn overlay_globe(ui: &mut Ui, pos: Pos2, f: &mut Flame) -> Response {
    let radius = 34.0;
    let size = 2.0 * (radius + 12.0);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(pos, Vec2::splat(size))).id_salt(Id::new("canvas-globe")));
    orbit_globe(&mut child, f, radius, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_axes_are_unit_rotations() {
        let a = [20.0, -35.0, 10.0, 5.0];
        for k in [YAW, PITCH, BANK, ROLL] {
            let w = param_axis(a, k);
            assert!((norm(w) - 1.0).abs() < 1e-9);
        }
        // Rotation is around the viewing axis.
        let w = param_axis(a, ROLL);
        assert!(w[2].abs() > 0.999, "{w:?}");
        // Yaw spins around the world Z axis.
        let c = cam(a);
        let z = [c[0][2], c[1][2], c[2][2]];
        let w = param_axis(a, YAW);
        assert!((w[0] * z[0] + w[1] * z[1] + w[2] * z[2]).abs() > 0.999);
    }

    #[test]
    fn solve_recovers_angles() {
        for a in [[10.0, 20.0, -30.0, 15.0], [-120.0, 45.0, 60.0, 0.0], [0.0, 0.0, 0.0, 0.0]] {
            let target = cam(a);
            let mut start = a;
            start[YAW] += 7.0;
            start[PITCH] -= 5.0;
            start[BANK] += 4.0;
            let s = solve(&target, start);
            let m = cam(s);
            let err: f64 = (0..3).flat_map(|i| (0..3).map(move |j| (i, j))).map(|(i, j)| (m[i][j] - target[i][j]).powi(2)).sum();
            assert!(err < 1e-10, "{a:?} -> {s:?} err {err}");
        }
    }

    #[test]
    fn facing_axes() {
        assert_eq!(facing(2, 1.0, 0.0), [0.0, 0.0, 0.0, 0.0]);
        for i in 0..3 {
            for s in [1.0, -1.0] {
                let a = facing(i, s, 30.0);
                let mut w = [0.0; 3];
                w[i] = s;
                assert!(apply(&cam(a), w)[2] > 0.999, "axis {i} {s}: {a:?}");
            }
        }
    }
}
