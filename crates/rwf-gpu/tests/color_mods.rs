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

//! The per-xform color modulation (`modGamma`, `modContrast`, `modSaturation`, `modHue`,
//! e.g. from MutaGen's "Local gamma" mutation) on the GPU.

use rwf_core::Flame;
use rwf_core::random::{Generator, generate};
use rwf_core::render::{CpuRender, PreparedFlame, RenderOptions};
use rwf_core::rng::Rng;
use rwf_gpu::codegen::{Program, validate_wgsl};
use rwf_gpu::{GpuContext, GpuRenderer};

fn flame(modulated: bool) -> Flame {
    let mut flame = generate(Generator::Classic, &mut Rng::new(42));
    flame.bg_transparency = false;
    if modulated {
        for (i, xf) in flame.layers[0].xforms.iter_mut().enumerate() {
            xf.mod_hue = 0.45 + 0.1 * i as f64;
            xf.mod_saturation = 0.7;
            xf.mod_contrast = -0.4;
            xf.mod_gamma = 0.3;
        }
    }
    flame
}

/// Mean red, green and blue share of the image.
fn tint(img: &[u8]) -> [f64; 3] {
    let mut s = [0.0; 3];
    for p in img.chunks_exact(4) {
        for c in 0..3 {
            s[c] += p[c] as f64;
        }
    }
    let t = s.iter().sum::<f64>().max(1.0);
    s.map(|v| v / t)
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

#[test]
fn program_with_color_mods_is_valid() {
    let f = flame(true);
    let prepared = PreparedFlame::new(&f);
    let prog = Program::new(&f, &prepared);
    assert!(prog.wgsl.contains("mod_xf_0"), "no color modulation emitted");
    if let Err(e) = validate_wgsl(&prog.wgsl) {
        panic!("{e}");
    }
    let plain = flame(false);
    assert!(!Program::new(&plain, &PreparedFlame::new(&plain)).wgsl.contains("mod_xf_0"));
}

#[test]
fn gpu_color_mods_match_cpu() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let (w, h) = (320u32, 180u32);
    let gpu = |f: &Flame| {
        let mut r = GpuRenderer::new(ctx.clone());
        r.set_flame(f, w, h);
        assert!(r.last_error().is_none(), "{:?}", r.last_error());
        for _ in 0..12 {
            r.iterate(1, 64);
        }
        r.tonemap();
        tint(&r.read_image().unwrap())
    };
    let f = flame(true);
    let opts = RenderOptions { width: w, height: h, threads: 4, max_density: Some(200.0), seed: 5, zbuffer: false };
    let cpu = tint(&CpuRender::start(&f, opts).wait().image(false));
    let (with, without) = (dist(gpu(&f), cpu), dist(gpu(&flame(false)), cpu));
    eprintln!("tint distance to the CPU: modulated {with:.4}, unmodulated {without:.4}");
    assert!(with < 0.03, "GPU and CPU colors differ ({with:.4})");
    assert!(without > with * 3.0, "modulation has no visible effect ({with:.4} vs {without:.4})");
}
