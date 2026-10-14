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

//! A recorded dance performance renders on the GPU: the music moves the flames, and the
//! recorded switch shows the second flame (skipped without a GPU).

use std::path::Path;

use rwf_core::audio::Audio;
use rwf_core::dance::{Cue, DanceProject, Performance, Spectrum};
use rwf_core::random::{Generator, generate};
use rwf_core::rng::Rng;
use rwf_gpu::GpuContext;
use rwf_gpu::GpuRenderer;
use rwf_gpu::dance::render_performance_frame;

/// Writes a mono 16-bit WAV file: bass beats (60 Hz bursts) twice a second.
fn write_beats(path: &Path, seconds: f64) {
    let rate = 44_100u32;
    let n = (seconds * rate as f64) as usize;
    let mut data = Vec::with_capacity(n * 2);
    for i in 0..n {
        let t = i as f64 / rate as f64;
        let beat = (t * 2.0).fract() < 0.15;
        let v = if beat { 0.8 * (std::f64::consts::TAU * 60.0 * t).sin() } else { 0.0 };
        data.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::write(path, wav).unwrap();
}

fn diff(a: &[u8], b: &[u8]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (*x as f64 - *y as f64).abs()).sum::<f64>() / a.len() as f64
}

#[test]
fn performance_renders_and_follows_the_music() {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let dir = std::env::temp_dir().join(format!("rwf-dance-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("beats.wav");
    write_beats(&wav, 4.0);
    let audio = Audio::load(&wav).unwrap();
    let spectrum = Spectrum::analyse(&audio);

    let mut rng = Rng::new(21);
    let mut project = DanceProject {
        flames: vec![generate(Generator::Classic, &mut rng), generate(Generator::Spherical, &mut rng)],
        sound: Some(wav.clone()),
        width: 240,
        height: 160,
        quality: 30.0,
        fps: 20.0,
        morph_frames: 4,
        ..Default::default()
    };
    project.create_default_motions();
    // Strong bass motion so the beat is clearly visible.
    project.motions.push(rwf_core::dance::Motion {
        links: vec![rwf_core::dance::Link { flame: 0, target: "layer0.xf0.@angle".into() }],
        ..rwf_core::dance::Motion::new("beat", rwf_core::dance::MotionKind::Fft { band: 1, amplitude: 60.0, offset: 0.0, release: 0.05 })
    });
    project.performance = Performance { cues: vec![Cue { time: 0.0, flame: 0 }, Cue { time: 2.0, flame: 1 }], start: 0.0, end: 3.0 };
    assert_eq!(project.performance_frames(), 60);

    let mut r = GpuRenderer::new(ctx);
    let mut frame = |i: u32| render_performance_frame(&project, Some(&spectrum), i, Some(&mut r)).unwrap();
    // 0.05 s is on the first beat, 0.35 s between beats; 2 s+ is the second flame.
    let (on_beat, off_beat, off_beat2, second) = (frame(1), frame(7), frame(8), frame(50));
    let beat = diff(&on_beat, &off_beat);
    let calm = diff(&off_beat, &off_beat2);
    let switched = diff(&off_beat, &second);
    eprintln!("beat {beat:.2}, between frames {calm:.2}, switched {switched:.2}");
    assert!(beat > calm * 1.5, "the beat does not move the flame ({beat:.2} vs {calm:.2})");
    assert!(switched > calm * 1.5, "no switch to the second flame");
    let _ = std::fs::remove_dir_all(&dir);
}
