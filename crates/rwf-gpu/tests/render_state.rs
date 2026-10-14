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

//! A render that is saved, written to disk, restored into a new renderer and continued must
//! give the same image as an uninterrupted render (skipped without a GPU).

use rwf_core::random::{Generator, generate};
use rwf_core::rng::Rng;
use rwf_gpu::{GpuContext, GpuRenderer, RenderState};

fn continued_equals_uninterrupted(seed: u64, solid: bool) {
    let Ok(ctx) = GpuContext::new_headless() else {
        eprintln!("no GPU, skipped");
        return;
    };
    let mut flame = generate(Generator::Classic, &mut Rng::new(seed));
    if solid {
        flame.solid.enabled = true;
        flame.solid.shadow_type = rwf_core::solid::ShadowType::Smooth;
    }
    let (w, h) = (320, 200);
    let run = |r: &mut GpuRenderer, n: u32| {
        for _ in 0..n {
            r.iterate(1, 64);
        }
        r.wait_idle();
    };

    let mut a = GpuRenderer::new(ctx.clone());
    a.set_flame(&flame, w, h);
    run(&mut a, 6);
    a.tonemap();
    let expected = a.read_image().unwrap();

    let mut b = GpuRenderer::new(ctx.clone());
    b.set_flame(&flame, w, h);
    run(&mut b, 3);
    b.tonemap();
    let halfway = b.read_image().unwrap();
    let st = b.save_state(1234).unwrap();
    let path = std::env::temp_dir().join(format!("rwf-render-state-{}-{seed}.rwfrender", std::process::id()));
    st.write(&path).unwrap();
    let st = RenderState::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(st.elapsed_ms, 1234);

    let mut c = GpuRenderer::new(ctx);
    c.restore_state(&st).unwrap();
    assert_eq!(c.samples(), b.samples());
    run(&mut c, 3);
    assert_eq!(c.samples(), a.samples());
    c.tonemap();
    let got = c.read_image().unwrap();
    let diff = |img: &[u8]| expected.iter().zip(img).filter(|(x, y)| x != y).count();
    // Sanity check: the comparison sees the missing half of the samples.
    assert!(diff(&halfway) * 20 > expected.len(), "halfway image too similar");
    let differing = diff(&got);
    eprintln!("{differing} differing bytes");
    assert!(differing * 1000 <= expected.len(), "{differing} of {} bytes differ", expected.len());
}

#[test]
fn classic_render_continues() {
    continued_equals_uninterrupted(11, false);
}

#[test]
fn solid_render_continues() {
    continued_equals_uninterrupted(5, true);
}
