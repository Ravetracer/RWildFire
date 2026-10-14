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

//! MutaGen (JWildfire's `MutaGenController`): a 5 × 5 grid of mutations around a flame. The
//! further a cell is from the centre, the more mutations it gets (one per step, horizontal
//! and vertical type); every cell is retried until it is visible and differs from the base.

use super::mutations::{MutationType, mutate_flame};
use crate::model::Flame;
use crate::render::{CpuRender, RenderOptions};
use crate::rng::Rng;

/// Rows and columns of a MutaGen set.
pub const GRID: usize = 5;
const CENTRE: i32 = (GRID / 2) as i32;

/// Probe render size and quality (`renderFlame(.., probeSize, ..)` with density 18).
const PROBE: (u32, u32) = (50, 40);
const PROBE_QUALITY: f64 = 18.0;
const MAX_ITER: usize = 10;
const MIN_RENDER_COVERAGE: f64 = 0.32;
const MIN_DIFF_COVERAGE: f64 = 0.18;
/// Block size of `createSimplifiedRefImage` (`PixelizeTransformer` with its 20 × 20 default).
const BLOCK: usize = 20;

fn probe(flame: &Flame) -> Vec<u8> {
    let mut f = flame.clone();
    f.bg_transparency = false;
    f.spatial_oversampling = 1;
    let opts = RenderOptions { width: PROBE.0, height: PROBE.1, threads: 1, max_density: Some(PROBE_QUALITY), seed: 11, zbuffer: false };
    CpuRender::start(&f, opts).wait().image(false)
}

/// `RandomFlameGeneratorSampler.createSimplifiedRefImage`: every 20 × 20 block takes the
/// color of its last pixel (JWildfire's `PixelizeTransformer` writes the last pixel it read,
/// not the average; its grid lines are the same in both images and never differ).
fn simplified(img: &[u8]) -> Vec<u8> {
    let (w, h) = (PROBE.0 as usize, PROBE.1 as usize);
    let mut out = img.to_vec();
    for by in (0..h).step_by(BLOCK) {
        for bx in (0..w).step_by(BLOCK) {
            let (x1, y1) = ((bx + BLOCK).min(w), (by + BLOCK).min(h));
            let last = ((y1 - 1) * w + x1 - 1) * 4;
            let c = [img[last], img[last + 1], img[last + 2]];
            for y in by..y1 {
                for x in bx..x1 {
                    out[(y * w + x) * 4..(y * w + x) * 4 + 3].copy_from_slice(&c);
                }
            }
        }
    }
    out
}

/// The simplified preview of a base flame, compared against its mutations.
pub struct Reference(Vec<u8>);

impl Reference {
    pub fn new(base: &Flame) -> Self {
        Reference(simplified(&probe(base)))
    }

    /// `calculateDiffCoverage`: share of pixels that changed noticeably.
    fn diff(&self, img: &[u8]) -> f64 {
        let s = simplified(img);
        let n = s.len() / 4;
        let changed = (0..n)
            .filter(|&i| {
                let (a, b) = (&s[i * 4..i * 4 + 3], &self.0[i * 4..i * 4 + 3]);
                (a[0] as f64 - b[0] as f64).abs() > 29.0 || (a[1] as f64 - b[1] as f64).abs() > 15.0 || (a[2] as f64 - b[2] as f64).abs() > 78.0
            })
            .count();
        changed as f64 / n as f64
    }
}

/// `createMutationTypes`: one mutation per axis for the inner ring, two for the outer one.
fn mutation_types(dx: i32, dy: i32, horizontal: MutationType, vertical: MutationType, rng: &mut Rng) -> Vec<MutationType> {
    let mut out = Vec::new();
    let near = |d: i32| d.abs() == 1;
    let far = |d: i32| d.abs() == 2;
    if rng.random() < 0.5 {
        if near(dx) {
            out.push(horizontal);
        }
        if near(dy) {
            out.push(vertical);
        }
    } else {
        if near(dy) {
            out.push(vertical);
        }
        if near(dx) {
            out.push(horizontal);
        }
    }
    if rng.random() > 0.5 {
        if far(dx) {
            out.extend([horizontal, horizontal]);
        }
        if far(dy) {
            out.extend([vertical, vertical]);
        }
    } else {
        if far(dy) {
            out.extend([vertical, vertical]);
        }
        if far(dx) {
            out.extend([horizontal, horizontal]);
        }
    }
    out
}

/// The flame of the cell in column `x` and row `y`: the base itself in the centre, else a
/// mutation that is visible and different from the base (the best of 10 attempts otherwise).
/// Columns away from the centre get `horizontal` mutations, rows `vertical` ones.
pub fn cell(base: &Flame, reference: &Reference, x: usize, y: usize, horizontal: MutationType, vertical: MutationType, rng: &mut Rng) -> Flame {
    let (dx, dy) = (x as i32 - CENTRE, y as i32 - CENTRE);
    if dx == 0 && dy == 0 {
        return base.clone();
    }
    let mut best: Option<(f64, Flame)> = None;
    for _ in 0..MAX_ITER {
        let mut flame = base.clone();
        for kind in mutation_types(dx, dy, horizontal, vertical, rng) {
            mutate_flame(&mut flame, kind, 1.0, rng);
        }
        let img = probe(&flame);
        let mut coverage = super::coverage::coverage_of(&img, PROBE.0 as usize, PROBE.1 as usize, true);
        if coverage > MIN_RENDER_COVERAGE {
            coverage = reference.diff(&img);
        }
        if coverage > MIN_DIFF_COVERAGE {
            return flame;
        }
        if best.as_ref().is_none_or(|(b, _)| coverage > *b) {
            best = Some((coverage, flame));
        }
    }
    best.map(|(_, f)| f).unwrap_or_else(|| base.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn more_mutations_further_out() {
        let mut rng = Rng::new(1);
        let (h, v) = (MutationType::Affine, MutationType::RandomGradient);
        assert!(mutation_types(0, 0, h, v, &mut rng).is_empty());
        assert_eq!(mutation_types(1, 0, h, v, &mut rng), vec![h]);
        assert_eq!(mutation_types(-2, 1, h, v, &mut rng).len(), 3);
        assert_eq!(mutation_types(2, -2, h, v, &mut rng).len(), 4);
    }

    #[test]
    fn cells_differ_from_the_base() {
        let mut rng = Rng::new(4);
        let base = super::super::generate(super::super::Generator::Simple, &mut rng);
        let reference = Reference::new(&base);
        assert_eq!(cell(&base, &reference, 2, 2, MutationType::All, MutationType::All, &mut rng), base);
        for &kind in MutationType::ALL {
            let f = cell(&base, &reference, 0, 4, kind, kind, &mut rng);
            assert!(f.layers.iter().any(|l| !l.xforms.is_empty()), "{}", kind.label());
        }
    }
}
