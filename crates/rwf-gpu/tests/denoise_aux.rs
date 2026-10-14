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
//! effect: a low quality render denoised with them must come closer to a high quality render
//! (skipped without a GPU or without Open Image Denoise).

use rwf_core::Flame;
use rwf_core::render::denoise::{Denoiser, apply_post_denoiser_with};
use rwf_gpu::{GpuContext, GpuRenderer};

fn flame(name: &str, solid: bool) -> Flame {
    let path = format!("{}/../../testdata/{name}.flame", env!("CARGO_MANIFEST_DIR"));
    let mut f = rwf_core::io::read_flames_from_file(std::path::Path::new(&path)).unwrap().remove(0);
    f.ai_post_denoiser = Denoiser::Oidn;
    f.post_denoiser_blend = 0.0;
    f.post_denoiser_only_for_cpu = false;
    f.solid.enabled = solid;
    f.solid.ao_enabled = solid;
    f
}

fn rms(a: &[u8], b: &[u8]) -> f64 {
    let (mut s, mut n) = (0.0, 0usize);
    for (p, q) in a.chunks(4).zip(b.chunks(4)) {
        for c in 0..3 {
            let d = p[c] as f64 - q[c] as f64;
            s += d * d;
            n += 1;
        }
    }
    (s / n as f64).sqrt()
}

#[test]
fn helper_images_improve_denoising() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    if let Err(e) = rwf_core::render::oidn::library() {
        eprintln!("OIDN not available, skipped ({e})");
        return;
    }
    let (w, h) = (480u32, 320u32);
    let mut r = GpuRenderer::new(ctx);
    for (name, solid, q_low) in [("TINA0007", false, 20.0), ("TINA0001", false, 20.0), ("TINA0001", true, 20.0)] {
        let f = flame(name, solid);
        let reference = r.render_frame(&f, 0.0, w, h, 2000.0).unwrap();
        // The same flame again: start over instead of accumulating on.
        r.reset_accumulation();
        let noisy = r.render_frame(&f, 0.0, w, h, q_low).unwrap();
        let aux = r.read_denoise_aux().expect("helper images");
        let (amin, amax) = aux.albedo.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        let amean = aux.albedo.iter().sum::<f32>() / aux.albedo.len() as f32;
        let lit: Vec<&[f32]> = aux.albedo.chunks(3).filter(|p| p.iter().any(|v| *v > 0.02)).collect();
        let clipped = lit.iter().filter(|p| p.iter().any(|v| *v >= 0.999)).count();
        // Beauty vs albedo in bright pixels: the beauty is about albedo x 255 there.
        let ratio: Vec<f64> = noisy
            .chunks(4)
            .zip(aux.albedo.chunks(3))
            .filter(|(b, a)| b[0] as u32 + b[1] as u32 + b[2] as u32 > 450 && a.iter().sum::<f32>() > 0.1)
            .map(|(b, a)| (b[0] as f64 + b[1] as f64 + b[2] as f64) / (255.0 * (a[0] + a[1] + a[2]) as f64))
            .collect();
        eprintln!(
            "  albedo clipped in {clipped} of {} lit pixels; beauty/albedo in bright pixels {:.2}",
            lit.len(),
            ratio.iter().sum::<f64>() / ratio.len().max(1) as f64
        );
        assert_eq!(aux.albedo.len(), (w * h * 3) as usize);
        assert_eq!(aux.normal.is_some(), solid);
        if let Some(n) = &aux.normal {
            let lit = n.chunks(3).filter(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2] - 1.0).abs() < 0.01).count();
            eprintln!("  normals: {lit} of {} pixels with a unit normal", n.len() / 3);
            assert!(lit > 100);
        }
        let mut plain = noisy.clone();
        let o1 = apply_post_denoiser_with(&f, &mut plain, w, h, true, None).unwrap();
        let mut helped = noisy.clone();
        let o2 = apply_post_denoiser_with(&f, &mut helped, w, h, true, Some(&aux)).unwrap();
        eprintln!(
            "{name}{}: albedo {amin:.2}..{amax:.2} (mean {amean:.2}); rms vs q2000: noisy {:.2}, denoised {:.2}, with helpers {:.2}  [{}]",
            if solid { " solid" } else { "" },
            rms(&noisy, &reference),
            rms(&plain, &reference),
            rms(&helped, &reference),
            o2.describe()
        );
        assert!(o1.denoised() && o2.denoised());
        assert!(rms(&helped, &reference) <= rms(&plain, &reference) * 1.05, "helper images made it worse");
        assert!(amax <= 1.0 && amin >= 0.0 && amax > 0.2, "albedo out of range");
    }
}
