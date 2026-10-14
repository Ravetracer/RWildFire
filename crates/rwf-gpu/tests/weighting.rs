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

//! Weighting field modulated parameters of a variation with a `prepare` step (step tables on
//! the GPU) must render like the CPU (skipped without a GPU).

use rwf_core::Flame;
use rwf_core::model::VariationInstance;
use rwf_core::random::{Generator, generate};
use rwf_core::render::{CpuRender, PreparedFlame, RenderOptions};
use rwf_core::rng::Rng;
use rwf_core::weighting::{FieldType, VarParamWeight};
use rwf_gpu::codegen::{Program, validate_wgsl};
use rwf_gpu::{GpuContext, GpuRenderer};

/// Two xforms; the first is `julian` with `dist` and `power` modulated by a Perlin field.
fn flame(intensity: f64) -> Flame {
    let mut flame = generate(Generator::Classic, &mut Rng::new(11));
    let layer = &mut flame.layers[0];
    layer.xforms.truncate(2);
    layer.final_xforms.clear();
    layer.bg_xforms.clear();
    let mut julian = VariationInstance::new("julian", 1.0);
    julian.set_param("power", 3.0);
    julian.set_param("dist", 0.6);
    let xf = &mut layer.xforms[0];
    xf.variations = vec![julian];
    xf.weighting.field_type = FieldType::PerlinNoise;
    xf.weighting.noise_frequency = 0.8;
    xf.weighting.var_params[0] = VarParamWeight { intensity, var_name: "julian".into(), param_name: "dist".into() };
    xf.weighting.var_params[1] = VarParamWeight { intensity: intensity * 0.5, var_name: "julian".into(), param_name: "power".into() };
    layer.xforms[1].variations = vec![VariationInstance::new("spherical", 0.8)];
    flame.centre_x = 0.0;
    flame.centre_y = 0.0;
    flame.pixels_per_unit = 90.0;
    flame.bg_transparency = false;
    flame
}

#[test]
fn step_table_program_is_valid() {
    let f = flame(2.0);
    let prepared = PreparedFlame::new(&f);
    let prog = Program::new(&f, &prepared);
    assert!(prog.wgsl.contains("wf_level(wmv"), "no step table used");
    if let Err(e) = validate_wgsl(&prog.wgsl) {
        panic!("{e}");
    }
    assert_eq!(prog.pack_params(&f, &prepared).len(), prog.param_len.max(1));
    // Without parameter modulation there is no table.
    let f0 = flame(0.0);
    assert!(!Program::new(&f0, &PreparedFlame::new(&f0)).wgsl.contains("wf_level(wmv"));
}

/// Mean luminance of `cell` x `cell` blocks.
fn cells(img: &[u8], w: usize, h: usize, cell: usize) -> Vec<f64> {
    let mut out = Vec::new();
    for cy in 0..h / cell {
        for cx in 0..w / cell {
            let mut s = 0.0;
            for i in 0..cell * cell {
                let (x, y) = (cx * cell + i % cell, cy * cell + i / cell);
                let p = &img[(y * w + x) * 4..(y * w + x) * 4 + 3];
                s += p.iter().map(|&c| c as f64).sum::<f64>();
            }
            out.push(s / (cell * cell) as f64);
        }
    }
    out
}

fn correlation(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let cov: f64 = a.iter().zip(b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    let va: f64 = a.iter().map(|x| (x - ma).powi(2)).sum();
    let vb: f64 = b.iter().map(|y| (y - mb).powi(2)).sum();
    cov / (va * vb).sqrt().max(1e-12)
}

#[test]
fn gpu_step_table_matches_cpu() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let (w, h) = (320u32, 240u32);
    let gpu = |f: &Flame| {
        let mut r = GpuRenderer::new(ctx.clone());
        r.set_flame(f, w, h);
        assert!(r.last_error().is_none(), "{:?}", r.last_error());
        for _ in 0..12 {
            r.iterate(1, 64);
        }
        r.tonemap();
        cells(&r.read_image().unwrap(), w as usize, h as usize, 8)
    };
    let modulated = flame(2.0);
    let opts = RenderOptions { width: w, height: h, threads: 4, max_density: Some(300.0), seed: 5, zbuffer: false };
    let cpu = cells(&CpuRender::start(&modulated, opts).wait().image(false), w as usize, h as usize, 8);
    let with = correlation(&gpu(&modulated), &cpu);
    let without = correlation(&gpu(&flame(0.0)), &cpu);
    eprintln!("correlation with CPU: step table {with:.3}, unmodulated {without:.3}");
    assert!(with > 0.9, "GPU and CPU differ ({with:.3})");
    assert!(with > without + 0.05, "modulation has no visible effect ({with:.3} vs {without:.3})");
}
