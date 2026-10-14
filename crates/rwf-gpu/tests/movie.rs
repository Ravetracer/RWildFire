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

//! Batch jobs end to end: multi-flame files, animations, skipping and cancelling (skipped
//! without a GPU).

use std::sync::atomic::AtomicBool;

use rwf_core::movie::{GlobalScript, MorphType, Movie, MoviePart, Script};
use rwf_core::random::{Generator, generate};
use rwf_core::rng::Rng;
use rwf_core::video::VideoFormat;
use rwf_gpu::GpuContext;
use rwf_gpu::movie::render_movie;

#[test]
fn renders_a_png_sequence() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let part =
        |seed: u64, morph: MorphType| MoviePart { flame: generate(Generator::Classic, &mut Rng::new(seed)), frames: 3, morph_frames: 2, morph };
    let movie = Movie {
        parts: vec![part(1, MorphType::Morph), part(2, MorphType::Fade), part(3, MorphType::Fade)],
        global_scripts: vec![Script { kind: GlobalScript::RotateRoll, amplitude: 1.0 }],
        width: 160,
        height: 90,
        quality: 20.0,
        motion_blur_length: 2,
        ..Movie::default()
    };
    let dir = std::env::temp_dir().join(format!("rwf-movie-test-{}", std::process::id()));
    let mut seen = Vec::new();
    render_movie(&movie, &dir, VideoFormat::PngSequence, Some(ctx), &AtomicBool::new(false), |i, total, img| {
        assert_eq!(total, 9);
        assert_eq!(img.len(), 160 * 90 * 4);
        seen.push((i, img.chunks(4).filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 30).count()));
    })
    .unwrap();
    let files = std::fs::read_dir(&dir).unwrap().count();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(files, 9);
    assert_eq!(seen.len(), 9);
    assert!(seen.iter().all(|(_, lit)| *lit > 50), "{seen:?}");
}
