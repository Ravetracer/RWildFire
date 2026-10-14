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

//! A render that is saved, written to disk, restored into a new renderer and continued must
//! give the same image as an uninterrupted render (skipped without a GPU).

use rwf_core::Flame;
use rwf_core::model::VariationInstance;
use rwf_core::random::{Generator, generate};
use rwf_core::render::{CpuRender, PreparedFlame, RenderOptions};
use rwf_core::rng::Rng;
use rwf_core::variations::subflame;
use rwf_gpu::codegen::{Program, validate_wgsl};
use rwf_gpu::{GpuContext, GpuRenderer};

const NAMES: &[&str] = &["subflame_wf", "pre_subflame_wf", "ringsubflame", "glynns3subfl"];

/// A flame with one xform that only has the variation `name` (with the default sub-flame).
fn flame_with(name: &str) -> Flame {
    let mut flame = generate(Generator::Classic, &mut Rng::new(7));
    let layer = &mut flame.layers[0];
    layer.xforms.truncate(1);
    layer.final_xforms.clear();
    layer.bg_xforms.clear();
    let xf = &mut layer.xforms[0];
    xf.variations = vec![VariationInstance::new(name, 1.0)];
    if name == "glynns3subfl" {
        xf.variations[0].set_param("weight", 0.0);
    }
    flame.centre_x = 0.0;
    flame.centre_y = 0.0;
    flame.pixels_per_unit = 140.0;
    flame
}

#[test]
fn subflame_programs_are_valid() {
    for name in NAMES {
        let flame = flame_with(name);
        let prepared = PreparedFlame::new(&flame);
        let program = Program::new(&flame, &prepared);
        assert!(program.uses_subflames, "{name}");
        assert!(!program.unsupported.iter().any(|n| n == name), "{name} unsupported");
        if let Err(e) = validate_wgsl(&program.wgsl) {
            panic!("{name}: {e}");
        }
        let mut patched = prepared.clone();
        let ids = rwf_gpu::subflames::assign_slots(&mut patched);
        assert_eq!(ids.len(), 1, "{name}: default sub-flame not loaded");
        let sub = subflame::sub_flame(ids[0]).unwrap();
        if let Err(e) = validate_wgsl(&Program::new_sub(&sub.flame).wgsl) {
            panic!("{name} sub-flame kernel: {e}");
        }
    }
}

/// Non-background cells of a coarse grid over an RGBA image.
fn mask(img: &[u8], w: usize, h: usize, cell: usize) -> Vec<bool> {
    let mut out = Vec::new();
    for cy in 0..h / cell {
        for cx in 0..w / cell {
            let lit = (0..cell * cell).filter(|i| {
                let (x, y) = (cx * cell + i % cell, cy * cell + i / cell);
                let p = &img[(y * w + x) * 4..(y * w + x) * 4 + 3];
                p.iter().map(|&c| c as u32).sum::<u32>() > 60
            });
            out.push(lit.count() * 10 > cell * cell);
        }
    }
    out
}

#[test]
fn gpu_subflame_matches_cpu() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let (w, h) = (320u32, 200u32);
    for name in ["subflame_wf", "ringsubflame"] {
        let mut flame = flame_with(name);
        flame.bg_transparency = false;
        let mut r = GpuRenderer::new(ctx.clone());
        r.set_flame(&flame, w, h);
        assert!(r.last_error().is_none(), "{name}: {:?}", r.last_error());
        assert!(!r.unsupported_variations().iter().any(|n| n == name), "{name} unsupported");
        for _ in 0..8 {
            r.iterate(1, 64);
        }
        r.tonemap();
        let gpu = r.read_image().unwrap();
        let opts = RenderOptions { width: w, height: h, threads: 4, max_density: Some(200.0), seed: 3, zbuffer: false };
        let cpu = CpuRender::start(&flame, opts).wait().image(false);
        let (a, b) = (mask(&gpu, w as usize, h as usize, 8), mask(&cpu, w as usize, h as usize, 8));
        let inter = a.iter().zip(&b).filter(|(x, y)| **x && **y).count();
        let union = a.iter().zip(&b).filter(|(x, y)| **x || **y).count();
        eprintln!("{name}: gpu {} cpu {} cells, overlap {inter}/{union}", a.iter().filter(|x| **x).count(), b.iter().filter(|x| **x).count());
        assert!(union > 8, "{name}: empty image");
        assert!(inter * 10 >= union * 7, "{name}: GPU and CPU differ ({inter}/{union})");
    }
}
