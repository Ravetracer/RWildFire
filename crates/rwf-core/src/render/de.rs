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

//! Density estimation (port of JWildfire's `DeCalculator` / `RasterFloatIntWithDe`):
//! adaptive blur of the histogram that smooths sparse areas while keeping dense detail.

use rayon::prelude::*;

use super::cpu::FloatRaster;
use super::tonemap::Raster;
use crate::math::EPSILON;
use crate::model::Flame;

#[derive(Clone, Copy, Debug)]
pub struct DeParams {
    pub radius: i32,
    pub curve: f64,
    pub splitter: f64,
}

impl DeParams {
    /// `ppu_scale` = output size relative to the flame's reference size.
    pub fn new(flame: &Flame, ppu_scale: f64) -> Option<Self> {
        if flame.de_radius.abs() <= EPSILON {
            return None;
        }
        let radius = ftoi(flame.de_radius * 9.0 * flame.spatial_oversampling.max(1) as f64 * ppu_scale).min(18);
        if radius < 1 {
            return None;
        }
        Some(Self { radius, curve: flame.de_curve, splitter: flame.de_comparison_line })
    }
}

/// Java `Tools.FTOI`: round half away from zero.
fn ftoi(v: f64) -> i32 {
    if v >= 0.0 { (v + 0.5) as i32 } else { (v - 0.5) as i32 }
}

/// `MathLib.erf` (numerical approximation used by JWildfire).
pub fn erf(z: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.5 * z.abs());
    let ans = 1.0
        - t * (-z * z - 1.26551223
            + t * (1.00002368
                + t * (0.37409196
                    + t * (0.09678418 + t * (-0.18628806 + t * (0.27886807 + t * (-1.13520398 + t * (1.48851587 + t * (-0.82215223 + t * 0.17087277)))))))))
            .exp();
    if z >= 0.0 { ans } else { -ans }
}

/// Size of the per-pixel deviation memo of [`apply`] (a power of two).
const DE_MEMO: usize = 256;

/// Applies density estimation to `raster` (`[count, r, g, b]` cells).
pub fn apply<R: Raster>(raster: &R, p: &DeParams) -> FloatRaster {
    let (w, h) = (raster.width() as i32, raster.height() as i32);
    let r = p.radius;
    let n = (2 * r + 1) as usize;
    let mut inv_dist = vec![0.0f64; n * n];
    let mut inv_dist_sqrt = vec![0.0f64; n * n];
    for dy in -r..=r {
        for dx in -r..=r {
            let id = 1.0 / ((dx * dx + dy * dy) as f32 as f64 + 1.0);
            let i = ((dx + r) as usize) * n + (dy + r) as usize;
            inv_dist[i] = id;
            inv_dist_sqrt[i] = (1.0 / id).sqrt();
        }
    }
    // Per-offset threshold factor pow(curve, sqrt(1/invDist)) is independent of the pixel.
    let thresh: Vec<f64> = inv_dist_sqrt.iter().map(|s| p.curve.powf(*s)).collect();

    // Snapshot the raster once (Raster::cell may be relatively expensive, e.g. atomics).
    let mut src = vec![[0.0f64; 4]; (w * h) as usize];
    src.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
        for (x, c) in row.iter_mut().enumerate() {
            let mut v = raster.cell(x as u32, y as u32);
            v[0] = v[0].round();
            *c = v;
        }
    });

    let show_splitter = p.splitter > EPSILON && p.splitter < 1.0 - EPSILON;
    let splitter_pos = (p.splitter * w as f64 + 0.5) as i32;
    let mut data = vec![0.0f32; (w * h * 4) as usize];
    data.par_chunks_mut((w * 4) as usize).enumerate().for_each(|(y, row)| {
        let y = y as i32;
        // memo entries: (count, deviation, pixel the entry belongs to)
        let mut memo = vec![(0.0f64, 0.0f64, i32::MIN); DE_MEMO];
        for x in 0..w {
            let center = src[(y * w + x) as usize];
            let center_w = center[0];
            let m1 = (8.0 * center_w).sqrt() + 5.0;
            let m2 = (center_w + 1.0).powf(-0.25);
            // The deviation only depends on the neighbour's (integer) count: memo of
            // `|erf((count - center_w) / m1)|` for this pixel (direct mapped).
            let (mut sr, mut sg, mut sb, mut sa, mut cnt) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for dy in -r..=r {
                let ly = y + dy;
                if ly < 0 || ly >= h {
                    continue;
                }
                for dx in -r..=r {
                    let lx = x + dx;
                    if lx < 0 || lx >= w {
                        continue;
                    }
                    let i = ((dx + r) as usize) * n + (dy + r) as usize;
                    let c = src[(ly * w + lx) as usize];
                    let slot = &mut memo[(c[0] as usize) & (DE_MEMO - 1)];
                    let deviation = if slot.2 == x && slot.0 == c[0] {
                        slot.1
                    } else {
                        let dev = erf((c[0] - center_w) / m1).abs();
                        *slot = (c[0], dev, x);
                        dev
                    };
                    if deviation <= thresh[i] * m2 {
                        let id = inv_dist[i];
                        sr += c[1] * id;
                        sg += c[2] * id;
                        sb += c[3] * id;
                        sa += c[0] * id;
                        cnt += id;
                    }
                }
            }
            let out = &mut row[(x * 4) as usize..(x * 4 + 4) as usize];
            if show_splitter && splitter_pos >= x - 2 && splitter_pos <= x + 2 {
                let c = center_w + 10.0;
                out.copy_from_slice(&[c as f32, (180.0 * c) as f32, (180.0 * c) as f32, 0.0]);
            } else if show_splitter && splitter_pos < x - 2 {
                out.copy_from_slice(&[center[0] as f32, center[1] as f32, center[2] as f32, center[3] as f32]);
            } else if cnt > EPSILON {
                out.copy_from_slice(&[((sa / cnt) + 0.5).floor() as f32, (sr / cnt) as f32, (sg / cnt) as f32, (sb / cnt) as f32]);
            } else {
                out.copy_from_slice(&[0.0; 4]);
            }
        }
    });
    FloatRaster { width: w as u32, height: h as u32, data }
}
