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

// Post-render image effects, the same computation as rwf_core::render::effects (keep both in
// step). Stages: prep_dark (dark channel), base_lum (luminance after dehaze, exposure,
// contrast), finish (everything, into the output texture); the blurs are in effects_blur.wgsl.

struct Fx {
    width: u32, height: u32, flags: u32, _p0: u32,
    exposure_gain: f32, contrast_pow: f32, highlights: f32, shadows: f32,
    black: f32, level_scale: f32, texture: f32, clarity: f32,
    dehaze: f32, vignette: f32, vig_lo: f32, vig_hi: f32,
    grain: f32, grain_cell: f32, _p1: f32, _p2: f32,
}

@group(0) @binding(0) var<uniform> FX: Fx;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> scalar_out: array<f32>;
@group(0) @binding(3) var<storage, read> dark_buf: array<f32>;
@group(0) @binding(4) var<storage, read> lum_buf: array<f32>;
@group(0) @binding(5) var<storage, read> mask_buf: array<f32>;
@group(0) @binding(6) var<storage, read> texture_buf: array<f32>;
@group(0) @binding(7) var<storage, read> clarity_buf: array<f32>;
// Four curves (all, red, green, blue) of LUT_SIZE samples.
@group(0) @binding(8) var<storage, read> lut: array<f32>;
@group(0) @binding(9) var out_tex: texture_storage_2d<rgba8unorm, write>;

const LUT_SIZE: u32 = 1024u;
const DEHAZE: u32 = 1u;
const EXPOSURE: u32 = 2u;
const CONTRAST: u32 = 4u;
const TONES: u32 = 8u;
const TEXTURE: u32 = 16u;
const CLARITY: u32 = 32u;
const LEVELS: u32 = 64u;
const CURVES: u32 = 128u;
const VIGNETTE: u32 = 256u;
const GRAIN: u32 = 512u;

fn has(flag: u32) -> bool { return (FX.flags & flag) != 0u; }

fn lum(c: vec3<f32>) -> f32 { return 0.299 * c.x + 0.587 * c.y + 0.114 * c.z; }

fn smooth_step(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

fn s_curve(v: f32, k: f32) -> f32 {
    if (v < 0.5) {
        let b = 2.0 * v;
        return select(0.0, 0.5 * pow(b, k), b > 0.0);
    }
    let b = 2.0 * (1.0 - v);
    return 1.0 - select(0.0, 0.5 * pow(b, k), b > 0.0);
}

fn input(x: u32, y: u32) -> vec3<f32> {
    return textureLoad(src_tex, vec2<i32>(i32(x), i32(y)), 0).xyz;
}

// Dehaze, exposure and contrast (`effects::base`).
fn base(c0: vec3<f32>, d: f32) -> vec3<f32> {
    var c = c0;
    if (has(DEHAZE)) {
        let haze = FX.dehaze * 0.8 * d;
        let inv = 1.0 / (1.0 - haze);
        c = max((c - vec3<f32>(haze)) * inv, vec3<f32>(0.0));
    }
    if (has(EXPOSURE)) {
        c = c * FX.exposure_gain;
    }
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    if (has(CONTRAST)) {
        c = vec3<f32>(s_curve(c.x, FX.contrast_pow), s_curve(c.y, FX.contrast_pow), s_curve(c.z, FX.contrast_pow));
    }
    return c;
}

fn hash(x: u32, y: u32) -> u32 {
    var h = (x * 0x8da6b343u) ^ (y * 0xd8163841u) ^ 0x9e3779b9u;
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    return h ^ (h >> 16u);
}

fn lattice(x: u32, y: u32) -> f32 {
    let h = hash(x, y);
    return f32(h & 0xffffu) / 65535.0 + f32(h >> 16u) / 65535.0 - 1.0;
}

fn grain_noise(x: u32, y: u32, cell: f32) -> f32 {
    let fx = f32(x) / cell;
    let fy = f32(y) / cell;
    let ix = floor(fx);
    let iy = floor(fy);
    let sx = smooth_step(0.0, 1.0, fx - ix);
    let sy = smooth_step(0.0, 1.0, fy - iy);
    let x0 = u32(ix);
    let y0 = u32(iy);
    let top = lattice(x0, y0) + (lattice(x0 + 1u, y0) - lattice(x0, y0)) * sx;
    let bottom = lattice(x0, y0 + 1u) + (lattice(x0 + 1u, y0 + 1u) - lattice(x0, y0 + 1u)) * sx;
    return top + (bottom - top) * sy;
}

fn lut_at(k: u32, v: f32) -> f32 {
    let t = clamp(v, 0.0, 1.0) * f32(LUT_SIZE - 1u);
    let i = min(u32(t), LUT_SIZE - 2u);
    let f = t - f32(i);
    let a = lut[k * LUT_SIZE + i];
    return a + (lut[k * LUT_SIZE + i + 1u] - a) * f;
}

fn to_u8(v: f32) -> f32 { return clamp(floor(v * 255.0 + 0.5), 0.0, 255.0); }

@compute @workgroup_size(16, 16)
fn prep_dark(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= FX.width || gid.y >= FX.height) { return; }
    let c = input(gid.x, gid.y);
    scalar_out[gid.y * FX.width + gid.x] = min(1.0, min(min(c.x, c.y), c.z));
}

@compute @workgroup_size(16, 16)
fn base_lum(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= FX.width || gid.y >= FX.height) { return; }
    let i = gid.y * FX.width + gid.x;
    var d = 0.0;
    if (has(DEHAZE)) { d = dark_buf[i]; }
    scalar_out[i] = lum(base(input(gid.x, gid.y), d));
}

@compute @workgroup_size(16, 16)
fn finish(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= FX.width || gid.y >= FX.height) { return; }
    let x = gid.x;
    let y = gid.y;
    let i = y * FX.width + x;
    let src = textureLoad(src_tex, vec2<i32>(i32(x), i32(y)), 0);
    var d = 0.0;
    if (has(DEHAZE)) { d = dark_buf[i]; }
    var c = base(src.xyz, d);
    if (has(TONES)) {
        let m = mask_buf[i];
        let ms = 1.0 - smooth_step(0.0, 0.5, m);
        let mh = smooth_step(0.5, 1.0, m);
        c = c * exp2(2.0 * (FX.shadows * ms + FX.highlights * mh) / 2.2);
    }
    var dl = 0.0;
    if (has(TEXTURE)) {
        dl += FX.texture * 1.5 * (lum_buf[i] - texture_buf[i]);
    }
    if (has(CLARITY)) {
        let l = lum_buf[i];
        let m = 2.0 * l - 1.0;
        dl += FX.clarity * 1.5 * (1.0 - m * m) * (l - clarity_buf[i]);
    }
    c = c + vec3<f32>(dl);
    if (has(LEVELS)) {
        c = (c - vec3<f32>(FX.black)) * FX.level_scale;
    }
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    if (has(CURVES)) {
        c = vec3<f32>(lut_at(1u, lut_at(0u, c.x)), lut_at(2u, lut_at(0u, c.y)), lut_at(3u, lut_at(0u, c.z)));
    }
    if (has(VIGNETTE)) {
        let dx = (f32(x) + 0.5) / f32(FX.width) * 2.0 - 1.0;
        let dy = (f32(y) + 0.5) / f32(FX.height) * 2.0 - 1.0;
        let v = smooth_step(FX.vig_lo, FX.vig_hi, sqrt((dx * dx + dy * dy) * 0.5));
        if (FX.vignette < 0.0) {
            c = c * (1.0 + FX.vignette * v);
        } else {
            c = c + FX.vignette * v * (vec3<f32>(1.0) - c);
        }
    }
    if (has(GRAIN)) {
        let l = lum(c);
        let a = FX.grain * 0.15 * grain_noise(x, y, FX.grain_cell) * (0.3 + 2.8 * l * (1.0 - l));
        c = c + vec3<f32>(a);
    }
    c = clamp(c, vec3<f32>(0.0), vec3<f32>(1.0));
    let q = vec3<f32>(to_u8(c.x), to_u8(c.y), to_u8(c.z)) / 255.0;
    textureStore(out_tex, vec2<i32>(i32(x), i32(y)), vec4<f32>(q, src.w));
}
