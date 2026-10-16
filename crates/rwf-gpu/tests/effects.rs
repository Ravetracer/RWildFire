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

//! Denoiser helper images from the GPU renderer (albedo, normals for solid flames) and their

//! The GPU image effects (live preview) against the CPU reference (saved images).

use rwf_core::render::effects::{self, ImageEffects};
use rwf_gpu::effects::EffectsPass;
use rwf_gpu::{GpuContext, GpuRenderer};

fn flame(name: &str) -> rwf_core::Flame {
    let path = format!("{}/../../testdata/{name}.flame", env!("CARGO_MANIFEST_DIR"));
    let mut f = rwf_core::io::read_flames_from_file(std::path::Path::new(&path)).unwrap().remove(0);
    f.bg_transparency = false;
    f
}

#[test]
fn gpu_effects_match_the_cpu() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let (w, h) = (300u32, 200u32);
    let mut r = GpuRenderer::new(ctx.clone());
    let img = r.render_frame(&flame("TINA0020"), 0.0, w, h, 100.0).unwrap();
    let mut pass = EffectsPass::new(&ctx.device);
    let mut curved = ImageEffects::default();
    curved.curves[0].y[1] = 9000.0;
    curved.curves[3].y[3] = 17000.0;
    let all = ImageEffects {
        exposure: 0.4,
        contrast: 0.3,
        highlights: -0.5,
        shadows: 0.6,
        whites: 0.2,
        blacks: -0.3,
        texture: 0.5,
        clarity: 0.7,
        dehaze: 0.4,
        vignette: -0.5,
        grain: 0.4,
        grain_size: 2.0,
        curves: curved.curves.clone(),
        ..Default::default()
    };
    let cases = [
        ("exposure", ImageEffects { exposure: -0.8, ..Default::default() }),
        ("contrast", ImageEffects { contrast: -0.6, ..Default::default() }),
        ("tones", ImageEffects { highlights: 0.5, shadows: -0.4, ..Default::default() }),
        ("texture", ImageEffects { texture: -0.7, ..Default::default() }),
        ("clarity", ImageEffects { clarity: 1.0, ..Default::default() }),
        ("dehaze", ImageEffects { dehaze: 1.0, ..Default::default() }),
        ("levels", ImageEffects { whites: -0.5, blacks: 0.5, ..Default::default() }),
        ("curves", curved),
        ("vignette", ImageEffects { vignette: 0.6, vignette_size: 0.3, vignette_feather: 0.8, ..Default::default() }),
        ("grain", ImageEffects { grain: 1.0, ..Default::default() }),
        ("all", all),
    ];
    for (name, fx) in cases {
        let mut cpu = img.clone();
        effects::apply(&fx, &mut cpu, w, h);
        assert_ne!(cpu, img, "{name}: no effect");
        assert!(pass.run(&ctx, r.output_view().unwrap(), w, h, &fx), "{name}: GPU pass did not run");
        let gpu = pass.read_image(&ctx).unwrap();
        let diffs: Vec<i32> = cpu.iter().zip(&gpu).map(|(a, b)| (*a as i32 - *b as i32).abs()).collect();
        let max = diffs.iter().max().copied().unwrap_or(0);
        let mean = diffs.iter().sum::<i32>() as f64 / diffs.len() as f64;
        eprintln!("{name}: max diff {max}, mean {mean:.4}");
        assert!(max <= 2 && mean < 0.02, "{name}: GPU and CPU differ (max {max}, mean {mean})");
        assert!(cpu.chunks(4).zip(img.chunks(4)).all(|(a, b)| a[3] == b[3]), "{name}: alpha changed");
    }
    assert!(!pass.run(&ctx, r.output_view().unwrap(), w, h, &ImageEffects::default()));
}
