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

// Blur of the image effects: three box blurs per direction with a sliding window, as
// rwf_core::render::effects::blur. blur_h (one invocation per row): src -> t1 -> t2 -> mid;
// blur_v (one per column): mid -> t1 -> t2 -> dst.

struct Bu { width: u32, height: u32, r0: u32, r1: u32, r2: u32, _p0: u32, _p1: u32, _p2: u32 }

@group(0) @binding(0) var<uniform> BU: Bu;
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var<storage, read_write> mid: array<f32>;
@group(0) @binding(3) var<storage, read_write> t1: array<f32>;
@group(0) @binding(4) var<storage, read_write> t2: array<f32>;
@group(0) @binding(5) var<storage, read_write> dst: array<f32>;

// Stages: 0 src, 1 t1, 2 t2, 3 mid, 4 dst.
fn load(stage: u32, i: u32) -> f32 {
    switch stage {
        case 0u: { return src[i]; }
        case 1u: { return t1[i]; }
        case 2u: { return t2[i]; }
        default: { return mid[i]; }
    }
}

fn store(stage: u32, i: u32, v: f32) {
    switch stage {
        case 1u: { t1[i] = v; }
        case 2u: { t2[i] = v; }
        case 3u: { mid[i] = v; }
        default: { dst[i] = v; }
    }
}

// One box blur of radius r along n values at base + k * stride (`effects::box_line`).
fn box_line(src_stage: u32, dst_stage: u32, base: u32, stride: u32, n: u32, r: u32) {
    var sum = 0.0;
    var cnt = 0u;
    let first = min(r, n - 1u);
    for (var j = 0u; j <= first; j++) {
        sum += load(src_stage, base + j * stride);
        cnt += 1u;
    }
    for (var i = 0u; i < n; i++) {
        store(dst_stage, base + i * stride, sum / f32(cnt));
        let add = i + r + 1u;
        if (add < n) {
            sum += load(src_stage, base + add * stride);
            cnt += 1u;
        }
        if (i >= r) {
            sum -= load(src_stage, base + (i - r) * stride);
            cnt -= 1u;
        }
    }
}

@compute @workgroup_size(64)
fn blur_h(@builtin(global_invocation_id) gid: vec3<u32>) {
    let y = gid.x;
    if (y >= BU.height) { return; }
    let base = y * BU.width;
    box_line(0u, 1u, base, 1u, BU.width, BU.r0);
    box_line(1u, 2u, base, 1u, BU.width, BU.r1);
    box_line(2u, 3u, base, 1u, BU.width, BU.r2);
}

@compute @workgroup_size(64)
fn blur_v(@builtin(global_invocation_id) gid: vec3<u32>) {
    let x = gid.x;
    if (x >= BU.width) { return; }
    box_line(3u, 1u, x, BU.width, BU.height, BU.r0);
    box_line(1u, 2u, x, BU.width, BU.height, BU.r1);
    box_line(2u, 4u, x, BU.width, BU.height, BU.r2);
}
