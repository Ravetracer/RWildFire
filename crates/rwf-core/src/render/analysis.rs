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

//! Cheap flame analysis used by generators and the editor (auto-framing, coverage).

use super::cpu::{CpuRender, RenderOptions};
use super::prepared::{NEXT_XFORM_TABLE_SIZE, PreparedFlame};
use crate::model::Flame;
use crate::variations::{Ctx, Pt};

/// Iterates the first visible layer and returns up to `n` plotted points (after finals).
pub fn sample_points(flame: &Flame, n: usize, seed: u64) -> Vec<(f64, f64)> {
    let prepared = PreparedFlame::new(flame);
    let Some(layer) = prepared.layers.first() else {
        return Vec::new();
    };
    let mut ctx = Ctx::new(seed, prepared.preserve_z);
    ctx.set_palette(&layer.palette);
    let mut out = Vec::with_capacity(n);
    let mut restarts = 0;
    'outer: while out.len() < n && restarts < 20 {
        restarts += 1;
        let mut p = Pt { x: 2.0 * ctx.random() - 1.0, y: 2.0 * ctx.random() - 1.0, color: ctx.random(), ..Pt::default() };
        let mut xf = 0usize;
        for i in 0..(n * 2 + 100) {
            let Some(next) = &layer.xforms[xf].next else {
                break 'outer;
            };
            xf = next[ctx.random_int(NEXT_XFORM_TABLE_SIZE)] as usize;
            p = layer.xforms[xf].transform(&mut ctx, &layer.colormap, false, &p);
            if p.is_bad() {
                continue 'outer;
            }
            if i < 20 {
                continue;
            }
            let mut q = p;
            for f in &layer.finals {
                q = f.transform(&mut ctx, &layer.colormap, false, &q);
            }
            if q.x.is_finite() && q.y.is_finite() {
                out.push((q.x, q.y));
                if out.len() >= n {
                    break 'outer;
                }
            }
        }
    }
    out
}

/// Centers the camera on the bulk of the attractor and fits it into the frame.
/// Only touches centre and scale (ignores 3D camera settings).
pub fn auto_frame(flame: &mut Flame) -> bool {
    let mut pts = sample_points(flame, 20_000, 7);
    if pts.len() < 500 {
        return false;
    }
    // Include the copies produced by post symmetry.
    let ps = &flame.post_symmetry;
    match ps.kind {
        crate::model::PostSymmetryType::Point if ps.order > 1 => {
            let base = pts.clone();
            for i in 1..ps.order.min(16) {
                let (s, c) = (std::f64::consts::TAU * i as f64 / ps.order as f64).sin_cos();
                pts.extend(base.iter().step_by(4).map(|&(x, y)| {
                    let (dx, dy) = (x - ps.centre_x, y - ps.centre_y);
                    (ps.centre_x + dx * c + dy * s, ps.centre_y + dy * c - dx * s)
                }));
            }
        }
        crate::model::PostSymmetryType::XAxis => {
            let base = pts.clone();
            pts.extend(base.iter().map(|&(x, y)| (2.0 * ps.centre_x - x, y)));
        }
        crate::model::PostSymmetryType::YAxis => {
            let base = pts.clone();
            pts.extend(base.iter().map(|&(x, y)| (x, 2.0 * ps.centre_y - y)));
        }
        _ => {}
    }
    let lo = pts.len() / 50;
    let hi = pts.len() - 1 - lo;
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (x0, x1) = (pts[lo].0, pts[hi].0);
    pts.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (y0, y1) = (pts[lo].1, pts[hi].1);
    let w = (x1 - x0).max(1e-6);
    let h = (y1 - y0).max(1e-6);
    // Rotation around the centre: approximate the rotated extents by the larger dimension.
    let roll = flame.cam_roll.to_radians();
    let (s, c) = roll.sin_cos();
    let rw = w * c.abs() + h * s.abs();
    let rh = w * s.abs() + h * c.abs();
    flame.centre_x = (x0 + x1) / 2.0;
    flame.centre_y = (y0 + y1) / 2.0;
    let ppu = (flame.width as f64 / rw).min(flame.height as f64 / rh) * 0.88;
    flame.pixels_per_unit = ppu / flame.cam_zoom.max(1e-6);
    true
}

/// Fraction of non-empty pixels in a tiny low-quality render (0..1).
pub fn coverage(flame: &Flame) -> f64 {
    let (w, h) = (96u32, 72u32);
    let r = CpuRender::start(flame, RenderOptions { width: w, height: h, threads: 1, max_density: Some(8.0), seed: 99 }).wait();
    let img = r.image(true);
    let filled = img.chunks_exact(4).filter(|p| p[3] > 24).count();
    filled as f64 / (w * h) as f64
}
