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

use rwf_core::animation::{Interp, Key, Track};
use rwf_core::random::{Generator, generate};
use rwf_core::rng::Rng;
use rwf_gpu::batch::{BatchJob, BatchSettings, SizeMode, render_job};
use rwf_gpu::{GpuContext, GpuRenderer};

#[test]
fn batch_jobs() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("rwf-batch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut rng = Rng::new(3);
    let flames: Vec<_> = (0..2).map(|_| generate(Generator::Classic, &mut rng)).collect();
    let file = dir.join("two.flame");
    std::fs::write(&file, rwf_core::io::write_flames(&flames)).unwrap();

    let mut animated = generate(Generator::Classic, &mut rng);
    animated.name = "anim".into();
    animated.animation.frame_count = 3;
    animated.animation.tracks.push(Track {
        path: "brightness".into(),
        interp: Interp::Linear,
        keys: vec![Key { frame: 0.0, value: 2.0 }, Key { frame: 2.0, value: 6.0 }],
    });

    let mut s =
        BatchSettings { size: SizeMode::LongEdge(160), quality: 50.0, out_dir: Some(dir.join("out")), animations: true, ..Default::default() };
    s.video_format = rwf_core::video::VideoFormat::PngSequence;
    let mut jobs = BatchJob::from_file(&file).unwrap();
    assert_eq!(jobs.len(), 2);
    jobs.push(BatchJob::from_flame(animated));

    let mut r = GpuRenderer::new(ctx);
    let mut calls = 0;
    for job in &jobs {
        let out = render_job(&mut r, job, &s, &mut |p, _| {
            assert!((0.0..=1.0).contains(&p));
            calls += 1;
            true
        })
        .unwrap();
        assert!(!out.skipped);
    }
    assert!(calls >= 5);
    let out = dir.join("out");
    assert!(out.join("two_001.png").is_file() && out.join("two_002.png").is_file());
    let frames = std::fs::read_dir(out.join("anim_frames")).unwrap().count();
    assert_eq!(frames, 3);

    // Existing output is skipped, cancelling stops a job.
    assert!(render_job(&mut r, &jobs[0], &s, &mut |_, _| true).unwrap().skipped);
    s.overwrite = true;
    s.quality = 5000.0;
    assert_eq!(render_job(&mut r, &jobs[0], &s, &mut |_, _| false).unwrap_err(), "cancelled");
    std::fs::remove_dir_all(&dir).ok();
}
