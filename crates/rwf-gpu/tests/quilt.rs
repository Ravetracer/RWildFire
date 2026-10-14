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

//! A quilt must look like a single render of the whole image: same brightness, same
//! background gradient, no seams at the tile borders (skipped without a GPU).

use rwf_core::model::BgColorType;
use rwf_core::random::{Generator, generate};
use rwf_core::rng::Rng;
use rwf_gpu::quilt::{QuiltEvent, QuiltJob, render_quilt};
use rwf_gpu::{GpuContext, GpuRenderer};

fn read_png(path: &std::path::Path) -> Vec<u8> {
    let mut dec = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).unwrap()));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut reader = dec.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut buf).unwrap();
    buf
}

#[test]
fn quilt_matches_a_single_render() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let mut flame = generate(Generator::Classic, &mut Rng::new(21));
    flame.bg_transparency = false;
    flame.background.kind = BgColorType::Gradient2x2;
    (flame.background.ul, flame.background.ur, flame.background.ll, flame.background.lr) = ([200, 0, 0], [0, 200, 0], [0, 0, 200], [200, 200, 0]);
    flame.de_radius = 0.6;
    let (w, h, q) = (480u32, 300u32, 400.0);

    let mut r = GpuRenderer::new(ctx);
    let full = r.render_frame(&flame, 0.0, w, h, q).unwrap();

    let dir = std::env::temp_dir().join(format!("rwf-quilt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let job = QuiltJob { flame: flame.clone(), width: w, height: h, cols: 3, rows: 2, quality: q, output: dir.join("q.png") };
    let mut done = 0;
    let out = render_quilt(&mut r, &job, &mut |e, _| {
        if let QuiltEvent::TileDone { .. } = e {
            done += 1;
        }
        true
    })
    .unwrap();
    assert_eq!(done, 6);
    assert!(!job.tile_dir().exists(), "tiles are removed after the merge");
    let quilt = read_png(&out);
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(quilt.len(), full.len());

    let px = |img: &[u8], x: u32, y: u32, c: usize| img[((y * w + x) * 4) as usize + c] as f64;
    let mean = |img: &[u8]| {
        (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| px(img, x, y, 0) + px(img, x, y, 1) + px(img, x, y, 2)).sum::<f64>()
            / (w * h) as f64
    };
    let (mf, mq) = (mean(&full), mean(&quilt));
    assert!((mf - mq).abs() / mf < 0.01, "brightness differs: {mf} vs {mq}");

    // Mean absolute difference per column / row: the seams must not stand out.
    let col_diff = |x: u32| (0..h).map(|y| (0..3).map(|c| (px(&full, x, y, c) - px(&quilt, x, y, c)).abs()).sum::<f64>()).sum::<f64>() / h as f64;
    let row_diff = |y: u32| (0..w).map(|x| (0..3).map(|c| (px(&full, x, y, c) - px(&quilt, x, y, c)).abs()).sum::<f64>()).sum::<f64>() / w as f64;
    let avg_col = (0..w).map(col_diff).sum::<f64>() / w as f64;
    let avg_row = (0..h).map(row_diff).sum::<f64>() / h as f64;
    eprintln!("mean {mf:.2} / {mq:.2}, average column difference {avg_col:.2}");
    for t in job.tiles().iter().filter(|t| t.x > 0) {
        for x in [t.x - 1, t.x] {
            assert!(col_diff(x) < avg_col * 2.0 + 1.0, "seam at x = {x}: {} vs {avg_col}", col_diff(x));
        }
    }
    for t in job.tiles().iter().filter(|t| t.y > 0) {
        for y in [t.y - 1, t.y] {
            assert!(row_diff(y) < avg_row * 2.0 + 1.0, "seam at y = {y}: {} vs {avg_row}", row_diff(y));
        }
    }
}

#[test]
fn stopped_quilt_continues() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let flame = generate(Generator::Classic, &mut Rng::new(5));
    let dir = std::env::temp_dir().join(format!("rwf-quilt-resume-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let job = QuiltJob { flame, width: 300, height: 200, cols: 2, rows: 2, quality: 100.0, output: dir.join("r.png") };
    let mut r = GpuRenderer::new(ctx);
    // Stop as soon as the second tile starts.
    let mut finished = 0;
    let err = render_quilt(&mut r, &job, &mut |e, _| match e {
        QuiltEvent::TileDone { .. } => {
            finished += 1;
            true
        }
        QuiltEvent::Progress { tile, .. } => tile == 0,
        QuiltEvent::Merging => true,
    })
    .unwrap_err();
    assert_eq!((err.as_str(), finished), ("cancelled", 1));
    assert!(!job.output.exists());
    let (mut reused, mut rendered) = (0, 0);
    render_quilt(&mut r, &job, &mut |e, _| {
        if let QuiltEvent::TileDone { reused: re, .. } = e {
            if re { reused += 1 } else { rendered += 1 }
        }
        true
    })
    .unwrap();
    assert_eq!((reused, rendered), (1, 3));
    assert!(job.output.is_file());
    std::fs::remove_dir_all(&dir).ok();
}
