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

//! "Quality" (sample density) means the same on the GPU as on the CPU: a GPU render stops at
//! the requested density (also for small images), does not count the warm-up iterations of
//! its chains, and is as bright as a CPU render (skipped without a GPU).

use rwf_core::render::{CpuRender, RenderOptions};
use rwf_gpu::{GpuContext, GpuRenderer};

fn mean_rgb(img: &[u8]) -> f64 {
    img.chunks(4).map(|p| p[0] as f64 + p[1] as f64 + p[2] as f64).sum::<f64>() / (img.len() / 4) as f64
}

#[test]
fn gpu_quality_matches_cpu() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/benchmark1.flame");
    let flame = rwf_core::io::read_flames_from_file(std::path::Path::new(path)).unwrap().remove(0);
    let (w, h) = (240u32, 160u32);
    let mut r = GpuRenderer::new(ctx);
    for q in [20.0, 200.0] {
        r.accumulate_frame(&flame, 0.0, w, h, q).unwrap();
        let density = r.density();
        r.tonemap();
        let gpu = mean_rgb(&r.read_image().unwrap());
        // Several CPU renders, as the brightness at low quality varies a bit with the seed.
        let cpu: f64 = (0..3)
            .map(|seed| {
                let opts = RenderOptions { width: w, height: h, threads: 4, max_density: Some(q), seed, zbuffer: false };
                mean_rgb(&CpuRender::start(&flame, opts).wait().image(false))
            })
            .sum::<f64>()
            / 3.0;
        eprintln!("quality {q}: GPU density {density:.1}, brightness GPU/CPU {:.3}", gpu / cpu);
        assert!(density >= q && density < q * 1.1, "GPU density {density} for quality {q}");
        assert!((gpu / cpu - 1.0).abs() < 0.03, "GPU brightness {gpu:.2} vs CPU {cpu:.2} at quality {q}");
    }
}
