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

//! The suggested z-buffer parameters spread the depths of a render over the gray range.

use rwf_core::render::{CpuRender, RenderOptions};

#[test]
fn suggested_params_use_the_gray_range() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/TINA0001.flame");
    let mut flame = rwf_core::io::read_flames_from_file(path).unwrap().remove(0);
    flame.cam_pitch = 50.0;
    flame.cam_perspective = 0.2;
    let opts = || RenderOptions { width: 160, height: 120, threads: 4, max_density: Some(30.0), seed: 1, zbuffer: true };
    let r = CpuRender::start(&flame, opts()).wait();
    let (scale, shift, _) = r.zbuffer_suggestion().expect("depths");
    flame.zbuffer_scale = scale;
    flame.zbuffer_shift = shift;
    flame.zbuffer_bias = 0.0;
    let r = CpuRender::start(&flame, opts()).wait();
    let z = r.zbuffer_image().unwrap();
    let covered: Vec<u16> = z.iter().copied().filter(|v| *v > 0).collect();
    let (lo, hi) = (covered.iter().min().unwrap(), covered.iter().max().unwrap());
    // the depth range plus 6% margins maps to the full range
    assert!(*hi as f64 > 0.8 * 65535.0 && (*lo as f64) < 0.2 * 65535.0, "{lo}..{hi}");
}
