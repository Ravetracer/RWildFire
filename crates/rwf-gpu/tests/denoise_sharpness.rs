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

//! Measures how well the post denoiser keeps lines: low quality renders, denoised with several
//! settings, against a high quality render without denoiser. Prints the error overall, at the
//! reference's strongest edges and in the other lit pixels, and the edge contrast relative to
//! the reference ("sharp", 1 = as sharp). Ignored (slow, needs a GPU and OIDN):
//!
//! `cargo test -p rwf-gpu --test denoise_sharpness -- --ignored --nocapture`
//!
//! `DN_FLAMES` (test flame names), `DN_Q` (qualities) and `DN_OUT` (folder for the images)
//! change the run.

use rwf_core::Flame;
use rwf_core::render::denoise::{DenoiseAux, Denoiser, apply_post_denoiser_from};
use rwf_gpu::{GpuContext, GpuRenderer};

fn flame(name: &str) -> Flame {
    let path = format!("{}/../../testdata/{name}.flame", env!("CARGO_MANIFEST_DIR"));
    let mut f = rwf_core::io::read_flames_from_file(std::path::Path::new(&path)).unwrap().remove(0);
    f.ai_post_denoiser = Denoiser::Oidn;
    f.post_denoiser_blend = 0.0;
    f.post_denoiser_only_for_cpu = false;
    f.bg_transparency = false;
    f
}

fn lum(p: &[u8]) -> f64 {
    0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64
}

fn grad(img: &[u8], w: usize, h: usize, x: usize, y: usize) -> f64 {
    let l = |x: usize, y: usize| lum(&img[(y * w + x) * 4..]);
    let (xl, xr, yu, yd) = (x.saturating_sub(1), (x + 1).min(w - 1), y.saturating_sub(1), (y + 1).min(h - 1));
    let gx = l(xr, y) - l(xl, y);
    let gy = l(x, yd) - l(x, yu);
    (gx * gx + gy * gy).sqrt()
}

struct Metrics {
    rmse: f64,
    edge_rmse: f64,
    flat_rmse: f64,
    sharp: f64,
}

fn metrics(img: &[u8], reference: &[u8], w: usize, h: usize) -> Metrics {
    let mut g: Vec<(f64, usize)> = (0..w * h).map(|i| (grad(reference, w, h, i % w, i / w), i)).collect();
    g.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let lit: Vec<usize> = (0..w * h).filter(|&i| lum(&reference[i * 4..]) > 8.0).collect();
    let n_edge = (lit.len() / 20).max(1);
    let edges: Vec<usize> = g.iter().take(n_edge).map(|e| e.1).collect();
    let mut is_edge = vec![false; w * h];
    for &i in &edges {
        is_edge[i] = true;
    }
    let err = |idx: &mut dyn Iterator<Item = usize>| {
        let (mut s, mut n) = (0.0, 0);
        for i in idx {
            for c in 0..3 {
                let d = img[i * 4 + c] as f64 - reference[i * 4 + c] as f64;
                s += d * d;
                n += 1;
            }
        }
        (s / n.max(1) as f64).sqrt()
    };
    let rmse = err(&mut (0..w * h));
    let edge_rmse = err(&mut edges.iter().copied());
    let flat_rmse = err(&mut lit.iter().copied().filter(|&i| !is_edge[i]));
    let gi: f64 = edges.iter().map(|&i| grad(img, w, h, i % w, i / w)).sum();
    let gr: f64 = edges.iter().map(|&i| grad(reference, w, h, i % w, i / w)).sum();
    Metrics { rmse, edge_rmse, flat_rmse, sharp: gi / gr }
}

fn save(name: &str, img: &[u8], w: u32, h: u32) {
    if let Ok(dir) = std::env::var("DN_OUT") {
        let p = format!("{dir}/{name}.png");
        rwf_core::output::write_rgba8(std::path::Path::new(&p), rwf_core::output::ImageFormat::Png, w, h, img, 95).unwrap();
    }
}

#[test]
#[ignore]
fn denoise_sharpness() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    if let Err(e) = rwf_core::render::oidn::library() {
        eprintln!("OIDN not available, skipped ({e})");
        return;
    }
    let (w, h) = (640u32, 480u32);
    let mut r = GpuRenderer::new(ctx);
    let names: Vec<String> =
        std::env::var("DN_FLAMES").unwrap_or("TINA0007,TINA0001,TINA0012,TINA0020".into()).split(',').map(String::from).collect();
    let qs: Vec<f64> = std::env::var("DN_Q").unwrap_or("30,150".into()).split(',').map(|q| q.parse().unwrap()).collect();
    for name in &names {
        let base = flame(name);
        r.reset_accumulation();
        let reference = r.render_frame(&base, 0.0, w, h, 5000.0).unwrap();
        save(&format!("{name}_ref"), &reference, w, h);
        for &q in &qs {
            r.reset_accumulation();
            let noisy = r.render_frame(&base, 0.0, w, h, q).unwrap();
            let (src, aux) = r.read_denoise_input(&Flame { post_denoiser_float: true, ..base.clone() });
            let aux = aux.unwrap();
            let d = aux.density.as_ref().unwrap();
            let lit: Vec<f32> = d.iter().copied().filter(|v| *v > 0.0).collect();
            let mut sorted = lit.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let pct = |p: f64| sorted.get(((sorted.len() as f64 - 1.0) * p) as usize).copied().unwrap_or(0.0);
            eprintln!("{name} q={q}: samples/pixel of lit pixels p10 {} p50 {} p90 {} p99 {}", pct(0.1), pct(0.5), pct(0.9), pct(0.99));
            let report = |label: &str, img: &[u8]| {
                let m = metrics(img, &reference, w as usize, h as usize);
                eprintln!("  {label:<28} rmse {:6.2}  edge {:6.2}  flat {:6.2}  sharp {:.3}", m.rmse, m.edge_rmse, m.flat_rmse, m.sharp);
                save(&format!("{name}_q{q}_{}", label.replace([' ', '=', '.'], "_")), img, w, h);
            };
            report("noisy", &noisy);
            let run = |f: &Flame, aux: Option<&DenoiseAux>| {
                let mut img = noisy.clone();
                apply_post_denoiser_from(f, &mut img, src.as_deref(), w, h, true, aux).unwrap();
                img
            };
            report("keep detail off", &run(&Flame { post_denoiser_detail: 0.0, ..base.clone() }, Some(&aux)));
            let no_albedo = DenoiseAux { albedo: Vec::new(), ..aux.clone() };
            report("keep detail off, no albedo", &run(&Flame { post_denoiser_detail: 0.0, ..base.clone() }, Some(&no_albedo)));
            report("filtered input, no helpers", &{
                let mut img = noisy.clone();
                apply_post_denoiser_from(&base, &mut img, None, w, h, true, None).unwrap();
                img
            });
            for det in [0.0, 0.6] {
                let f = Flame { post_denoiser_detail: det, ..base.clone() };
                report(&format!("detail={det}"), &run(&f, Some(&aux)));
                let fl = Flame { post_denoiser_float: true, ..f.clone() };
                report(&format!("detail={det} float"), &run(&fl, Some(&aux)));
                let nm = Flame { post_denoiser_normals: true, ..f.clone() };
                report(&format!("detail={det} normals"), &run(&nm, Some(&aux)));
                let both = Flame { post_denoiser_normals: true, post_denoiser_float: true, ..f.clone() };
                report(&format!("detail={det} float+normals"), &run(&both, Some(&aux)));
            }
            // Supersampling: the same quality at twice the size, denoised and scaled down,
            // against a high quality render at twice the size, scaled down (supersampling also
            // changes the look: the filter is half as wide).
            r.reset_accumulation();
            let ref2 = rwf_core::render::post::downscale(&r.render_frame(&base, 0.0, 2 * w, 2 * h, 5000.0).unwrap(), 2 * w, 2 * h, 2);
            let report2 = |label: &str, img: &[u8]| {
                let m = metrics(img, &ref2, w as usize, h as usize);
                eprintln!("  {label:<28} rmse {:6.2}  edge {:6.2}  flat {:6.2}  sharp {:.3}  (2x reference)", m.rmse, m.edge_rmse, m.flat_rmse, m.sharp);
                save(&format!("{name}_q{q}_{}", label.replace([' ', '=', '.', ','], "_")), img, w, h);
            };
            r.reset_accumulation();
            let big = r.render_frame(&base, 0.0, 2 * w, 2 * h, q).unwrap();
            let (bsrc, baux) = r.read_denoise_input(&Flame { post_denoiser_float: true, ..base.clone() });
            for det in [0.0, 0.6] {
                let f = Flame { post_denoiser_detail: det, ..base.clone() };
                let mut img = big.clone();
                apply_post_denoiser_from(&f, &mut img, bsrc.as_deref(), 2 * w, 2 * h, true, baux.as_ref()).unwrap();
                report2(&format!("detail={det} supersample"), &rwf_core::render::post::downscale(&img, 2 * w, 2 * h, 2));
            }
            report2("supersample, no denoiser", &rwf_core::render::post::downscale(&big, 2 * w, 2 * h, 2));
            for sh in [0.3, 0.6] {
                let f = Flame { post_denoiser_sharpen: sh, post_denoiser_detail: 0.6, ..base.clone() };
                report(&format!("detail=0.6 sharpen={sh}"), &run(&f, Some(&aux)));
            }
        }
    }
}
