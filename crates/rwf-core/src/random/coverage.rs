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

//! How much of a quick preview a random flame fills
//! (`RandomFlameGeneratorSampler.calculateCoverage`).

use crate::model::Flame;
use crate::render::{CpuRender, RenderOptions};

/// Preview size and quality of the sampler (JWildfire renders with sample density 22).
const PREVIEW: (u32, u32) = (128, 96);
const PREVIEW_QUALITY: f64 = 22.0;

/// Renders a small opaque preview (RGBA8) on the CPU.
pub fn preview(flame: &Flame) -> Vec<u8> {
    let mut f = flame.clone();
    f.bg_transparency = false;
    // `applyFastOversamplingSettings`
    f.spatial_oversampling = 1;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let opts = RenderOptions { width: PREVIEW.0, height: PREVIEW.1, threads, max_density: Some(PREVIEW_QUALITY), seed: 99, zbuffer: false };
    CpuRender::start(&f, opts).wait().image(false)
}

/// `ConvolveTransformer` with the `SOBEL_3X3` kernel (north, mirrored edges, kernel sum 0
/// counts as 1, results clamped to 0..255).
fn sobel(img: &[u8], w: usize, h: usize) -> Vec<u8> {
    const K: [[i32; 3]; 3] = [[-1, 0, 1], [-2, 0, 2], [-1, 0, 1]];
    let mirror = |v: i64, n: usize| -> usize {
        if v < 0 {
            (-v) as usize
        } else if v >= n as i64 {
            (2 * n as i64 - v - 1) as usize
        } else {
            v as usize
        }
    };
    let mut out = img.to_vec();
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0i32; 3];
            for (k, row) in K.iter().enumerate() {
                for (l, &kv) in row.iter().enumerate() {
                    let sx = mirror(x as i64 + l as i64 - 1, w);
                    let sy = mirror(y as i64 + k as i64 - 1, h);
                    let i = (sy * w + sx) * 4;
                    for c in 0..3 {
                        sum[c] += img[i + c] as i32 * kv;
                    }
                }
            }
            let o = (y * w + x) * 4;
            for c in 0..3 {
                out[o + c] = sum[c].clamp(0, 255) as u8;
            }
        }
    }
    out
}

/// Fraction of the preview that is not background (black), optionally of its edges.
pub fn coverage(flame: &Flame, use_filter: bool) -> f64 {
    coverage_of(&preview(flame), PREVIEW.0 as usize, PREVIEW.1 as usize, use_filter)
}

/// `calculateCoverage` of an RGBA8 image.
pub fn coverage_of(img: &[u8], w: usize, h: usize, use_filter: bool) -> f64 {
    let filtered;
    let img = if use_filter {
        filtered = sobel(img, w, h);
        &filtered[..]
    } else {
        img
    };
    let covered = img.as_chunks::<4>().0.iter().filter(|p| p[0] > 29 || p[1] > 15 || p[2] > 78).count();
    covered as f64 / (w * h) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sobel_finds_vertical_edges() {
        // Left half black, right half white: one bright column at the edge.
        let (w, h) = (8usize, 4usize);
        let mut img = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in w / 2..w {
                img[(y * w + x) * 4..(y * w + x) * 4 + 3].fill(255);
            }
        }
        let e = sobel(&img, w, h);
        let bright: Vec<usize> = (0..w).filter(|&x| e[x * 4] > 0).collect();
        assert_eq!(bright, vec![w / 2 - 1, w / 2]);
    }
}
