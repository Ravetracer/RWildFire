// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// Ported from JWildfire - an image and animation processor written in Java
// Copyright (C) 1995-2026 Andreas Maschke
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

//! Rendering: compiled flame, camera, CPU iteration and tonemapping.

pub mod analysis;
pub mod background;
pub mod cpu;
pub mod de;
pub mod denoise;
pub mod dof;
pub mod effects;
pub mod oidn;
pub mod filter;
pub mod post;
pub mod prepared;
pub mod solid;
pub mod tonemap;
pub mod view;

pub use cpu::{CpuRender, RenderOptions, render_image};
pub use prepared::PreparedFlame;
pub use view::View;

use crate::model::{Background, Flame};

/// Copy of `flame` with every value that only affects tonemapping neutralized.
fn iteration_key(flame: &Flame) -> Flame {
    let mut f = flame.clone();
    f.name.clear();
    f.resolution_profile.clear();
    f.quality_profile.clear();
    f.brightness = 0.0;
    f.contrast = 0.0;
    f.gamma = 0.0;
    f.gamma_threshold = 0.0;
    f.vibrancy = 0.0;
    f.white_level = 0.0;
    f.saturation = 0.0;
    f.low_density_brightness = 0.0;
    f.balance_red = 0.0;
    f.balance_green = 0.0;
    f.balance_blue = 0.0;
    f.fg_opacity = 0.0;
    f.sample_density = 0.0;
    f.background = Background::default();
    f.mixer = Default::default();
    for l in &mut f.layers {
        l.bg_xforms.clear();
    }
    f.bg_transparency = false;
    f.extra_attrs.clear();
    f.animation = Default::default();
    f.ai_post_denoiser = Default::default();
    f.post_denoiser_blend = 0.0;
    f.post_denoiser_only_for_cpu = false;
    f.post_denoiser_detail = 0.0;
    f.post_denoiser_sharpen = 0.0;
    (f.post_denoiser_float, f.post_denoiser_normals, f.post_denoiser_supersample) = (false, false, false);
    f.effects = Default::default();
    // Solid rendering: only the shadow map projection affects the iteration; materials,
    // ambient occlusion, light colors and the bokeh are applied when shading.
    let s = &mut f.solid;
    let shadows = s.with_shadows();
    s.materials.clear();
    s.ao_enabled = false;
    (s.ao_intensity, s.ao_search_radius, s.ao_blur_radius, s.ao_falloff, s.ao_affect_diffuse) = (0.0, 0.0, 0.0, 0.0, 0.0);
    (s.ao_radius_samples, s.ao_azimuth_samples) = (0, 0);
    (s.shadowmap_bias, s.shadow_smooth_radius) = (0.0, 0.0);
    (s.post_bokeh_intensity, s.post_bokeh_brightness, s.post_bokeh_size, s.post_bokeh_activation) = (0.0, 0.0, 0.0, 0.0);
    s.post_bokeh_filter_kernel = Default::default();
    s.shadow_type = if shadows { crate::solid::ShadowType::Fast } else { crate::solid::ShadowType::Off };
    for l in &mut s.lights {
        (l.intensity, l.color, l.shadow_intensity) = (0.0, [0.0; 3], 0.0);
        if !shadows || !l.cast_shadows {
            (l.altitude, l.azimuth, l.cast_shadows) = (0.0, 0.0, false);
        }
    }
    if !shadows {
        s.shadowmap_size = 0;
    }
    f
}

/// True if `a` and `b` produce the same histogram (they may differ in tonemapping only).
static CPU_THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Sets the CPU threads of renders that choose their own count (0: all cores).
pub fn set_cpu_threads(n: usize) {
    CPU_THREADS.store(n, std::sync::atomic::Ordering::Relaxed);
}

/// CPU threads for renders that choose their own count: the set number, or all cores.
pub fn cpu_threads() -> usize {
    match CPU_THREADS.load(std::sync::atomic::Ordering::Relaxed) {
        0 => std::thread::available_parallelism().map_or(4, |n| n.get()),
        n => n,
    }
}

pub fn same_iteration(a: &Flame, b: &Flame) -> bool {
    iteration_key(a) == iteration_key(b)
}

/// The flame states to render for an animation frame: the frame itself plus the motion blur
/// sub-frames (JWildfire `createRenderPackets`), each with scaled layer weights.
pub fn frame_packets(flame: &Flame, frame: f64) -> Vec<Flame> {
    let mut out = vec![flame.at_frame(frame)];
    let a = &flame.animation;
    if a.motion_blur_length > 0 {
        let len = a.motion_blur_length as f64;
        let mut t = frame + len * a.motion_blur_time_step / 2.0;
        for p in 1..=a.motion_blur_length {
            t -= a.motion_blur_time_step;
            let mut f = flame.at_frame(t);
            let scl = (1.0 - (p * p) as f64 * a.motion_blur_decay * 0.07 / len).max(0.01);
            for l in &mut f.layers {
                l.weight *= scl;
            }
            out.push(f);
        }
    }
    out
}
