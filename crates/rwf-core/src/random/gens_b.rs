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

//! Ported generators: Julian family, EDisc, spirals, gnarl, brokat, synth and the 3D
//! wrappers of `AbstractAffine3DRandomFlameGenerator`.

use std::f64::consts::PI;
use std::sync::OnceLock;

use super::gens_a::experimental;
use super::java::{self, Plane, add, global_translate, local_translate, pick, rand_int, randomize_colors, rotate, scale, set, set_xaos, xform};
use super::{Generator, State};
use crate::model::{Affine, BgColorType, ColorType, DrawMode, Flame, VariationInstance, XForm};
use crate::rng::Rng;

/// Affine coefficients in JWildfire's `setCoeff00 .. setCoeff21` order.
pub(super) fn affine(c00: f64, c01: f64, c10: f64, c11: f64, c20: f64, c21: f64) -> Affine {
    Affine { c00, c01, c10, c11, c20, c21 }
}

/// `Tools.FTOI`: rounds half away from zero.
pub(super) fn ftoi(v: f64) -> i32 {
    v.round() as i32
}

pub(super) fn base(ppu: f64) -> Flame {
    let mut flame = java::flame();
    flame.centre_x = 0.0;
    flame.centre_y = 0.0;
    flame.pixels_per_unit = ppu;
    flame
}

/// A random variation added with a priority (`setPriority`: -1 pre, +1 post).
pub(super) fn add_prio(xf: &mut XForm, amount: f64, name: &str, priority: i32) {
    add(xf, amount, name).priority = priority;
}

// ---------------------------------------------------------------------------------------
// AbstractAffine3DRandomFlameGenerator

/// `AbstractAffine3DRandomFlameGenerator.prepareFlame`: a 2D generator's flame seen through
/// a random 3D camera.
fn affine3d_wrapper(st: &mut State, select: fn(&mut Rng) -> Generator, pre: fn(&mut Flame), post: fn(&mut Flame, &mut Rng)) -> Flame {
    let g = *st.sub.get_or_insert_with(|| select(st.rng));
    let mut flame = g.prepare(&mut State { rng: &mut *st.rng, sub: None, simple: false });
    pre(&mut flame);
    let r = &mut *st.rng;
    flame.cam_roll = 0.0;
    flame.cam_pitch = 25.0 + r.random() * 30.0;
    flame.cam_yaw = 10.0 + r.random() * 20.0;
    flame.cam_bank = 5.0 + r.random() * 10.0;
    flame.cam_perspective = 0.1 + r.random() * 0.3;
    flame.cam_zoom = 0.5;
    flame.preserve_z = true;
    if r.random() < 0.5 {
        flame.cam_dof = 0.1 + r.random() * 0.2;
    }
    post(&mut flame, r);
    flame
}

fn rotate_xform(flame: &mut Flame, idx: usize, amp: f64, r: &mut Rng) {
    if let Some(xf) = flame.first_layer_mut().xforms.get_mut(idx) {
        java::rotate_in(xf, Plane::ZX, (0.5 - r.random()) * amp, false);
    }
}

fn scale_xform(flame: &mut Flame, idx: usize, offset: f64, amp: f64, r: &mut Rng) {
    if let Some(xf) = flame.first_layer_mut().xforms.get_mut(idx) {
        java::scale_in(xf, Plane::ZX, offset + (0.5 - r.random()) * amp, true, true, false);
    }
}

fn add_flatten(flame: &mut Flame, idx: usize) {
    if let Some(xf) = flame.first_layer_mut().xforms.get_mut(idx)
        && !xf.variations.iter().any(|v| v.name == "flatten")
    {
        add(xf, 1.0, "flatten");
    }
}

fn keep_two_xforms(flame: &mut Flame) {
    flame.first_layer_mut().xforms.truncate(2);
}

/// `Spirals3DRandomFlameGenerator`.
pub fn spirals3d(st: &mut State) -> Flame {
    affine3d_wrapper(
        st,
        |_| Generator::Spirals,
        keep_two_xforms,
        |f, r| {
            f.cam_yaw += (0.5 - r.random()) * 75.0;
            f.cam_pitch += (0.5 - r.random()) * 135.0;
            // JWildfire adds the yaw here, not the bank.
            f.cam_bank = (0.5 - r.random()) * 75.0 + f.cam_yaw;
            f.cam_perspective = 0.1 + r.random() * 0.4;
            f.cam_zoom = 2.0;
            rotate_xform(f, 0, 30.0, r);
            scale_xform(f, 0, 0.97, 0.01, r);
            rotate_xform(f, 1, 10.0, r);
            scale_xform(f, 1, 0.97, 0.01, r);
        },
    )
}

/// `Gnarl3DRandomFlameGenerator`.
pub fn gnarl3d(st: &mut State) -> Flame {
    affine3d_wrapper(
        st,
        |r| if r.random() > 0.33 { Generator::Gnarl } else { Generator::ExperimentalGnarl },
        keep_two_xforms,
        |f, r| {
            rotate_xform(f, 0, 3.0, r);
            scale_xform(f, 0, 0.95, 0.01, r);
        },
    )
}

/// `Brokat3DRandomFlameGenerator`.
pub fn brokat3d(st: &mut State) -> Flame {
    affine3d_wrapper(
        st,
        |_| Generator::Brokat,
        |_| {},
        |f, r| {
            f.cam_zoom *= 2.0;
            f.cam_yaw += (0.5 - r.random()) * 75.0;
            f.cam_pitch += (0.5 - r.random()) * 135.0;
            f.cam_bank += (0.5 - r.random()) * 15.0;
            f.cam_perspective = 0.1 + r.random() * 0.4;
            rotate_xform(f, 0, 33.0, r);
            for _ in 1..=3 {
                if r.random() > 0.5 {
                    rotate_xform(f, 1, 15.0, r);
                    if r.random() > 0.67 {
                        add_flatten(f, 1);
                    }
                }
            }
        },
    )
}

// ---------------------------------------------------------------------------------------
// Julians, JulianDisc, JulianRings, EDisc

fn rand_in_range(r: &mut Rng, min: f64, max: f64) -> f64 {
    min + r.random() * (max - min)
}

/// `randInRange(int, int)`.
fn rand_in_range_i(r: &mut Rng, min: i32, max: i32) -> f64 {
    ftoi(min as f64 + r.random() * (max - min) as f64) as f64
}

fn julians_affine(xf: &mut XForm, r: &mut Rng) {
    let sel = r.random();
    if sel > 0.2 && sel <= 0.4 {
        let a = rand_in_range(r, 0.0, 360.0);
        rotate(xf, a, false);
    }
    if sel > 0.4 && sel <= 0.6 {
        let (x, y) = (rand_in_range(r, 0.0, 1.5), rand_in_range(r, 0.0, 1.5));
        local_translate(xf, x, y, false);
    }
    if sel > 0.6 && sel <= 0.8 {
        let s = rand_in_range(r, 0.25, 1.5);
        scale(xf, s, true, true, false);
    } else if sel > 0.8 {
        let a = rand_in_range(r, 0.0, 360.0);
        rotate(xf, a, false);
        let (x, y) = (rand_in_range(r, 0.0, 1.5), rand_in_range(r, 0.0, 1.5));
        local_translate(xf, x, y, false);
        let s = rand_in_range(r, 0.0, 1.2);
        scale(xf, s, true, true, false);
    }
}

fn julians_post_affine(xf: &mut XForm, r: &mut Rng) {
    let sel = r.random();
    if sel > 0.6 && sel <= 0.7 {
        let a = rand_in_range(r, 0.0, 360.0);
        rotate(xf, a, true);
    }
    if sel > 0.7 && sel <= 0.8 {
        let (x, y) = (rand_in_range(r, 0.0, 1.5), rand_in_range(r, 0.0, 1.5));
        local_translate(xf, x, y, true);
    }
    if sel > 0.8 && sel <= 0.9 {
        // JWildfire scales the pre affine here.
        let s = rand_in_range(r, 0.25, 1.5);
        scale(xf, s, true, true, false);
    } else if sel > 0.9 {
        let a = rand_in_range(r, 0.0, 360.0);
        rotate(xf, a, true);
        let (x, y) = (rand_in_range(r, 0.0, 1.5), rand_in_range(r, 0.0, 1.5));
        local_translate(xf, x, y, true);
        let s = rand_in_range(r, 0.0, 1.2);
        scale(xf, s, true, true, true);
    }
}

/// `JuliansRandomFlameGenerator`.
pub fn julians(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.cam_zoom = 1.0;
    let primary = if r.random() < 0.666 { "julian" } else { "juliascope" };
    let layer = flame.first_layer_mut();
    let julian = |xf: &mut XForm, r: &mut Rng, max_power: i32| {
        let power = rand_in_range_i(r, 1, max_power);
        let dist = -2.0 + 4.0 * r.random();
        let amount = -2.0 + r.random() * 4.0;
        set(add(xf, amount, primary), &[("power", power), ("dist", dist)]);
    };
    {
        let mut xf = xform();
        xf.weight = 0.00001 + r.random() * 20.0;
        xf.color = r.random();
        if r.random() < 0.5 {
            let name = java::random_variation(r);
            add(&mut xf, -1.0 + r.random() * 2.0, name);
        }
        julian(&mut xf, r, 7);
        julians_affine(&mut xf, r);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.00001 + r.random() * 6.0;
        xf.color = r.random();
        julian(&mut xf, r, 7);
        if r.random() < 0.8 {
            add(&mut xf, -1.0 + r.random() * 2.0, "linear");
            let name = java::random_variation(r);
            add(&mut xf, -1.0 + r.random() * 2.0, name);
        }
        let w = if r.random() < 0.5 { 0.0 } else { r.random() };
        set_xaos(&mut xf, 1, w);
        let s = 0.5 + r.random() * 0.5;
        let (sx, sy) = (r.random() < 0.5, r.random() < 0.5);
        scale(&mut xf, s, sx, sy, false);
        julians_affine(&mut xf, r);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.00001 + r.random() * 2.0;
        xf.color = r.random();
        julian(&mut xf, r, 5);
        julians_affine(&mut xf, r);
        julians_post_affine(&mut xf, r);
        layer.xforms.push(xf);
    }
    flame
}

/// `getNonBlurRandomFunc` of JulianDisc.
fn non_blur_random_func(r: &mut Rng) -> &'static str {
    let list = experimental();
    loop {
        let name = list[rand_int(r, list.len())];
        if !name.contains("blur") && !name.contains("pre_") && !name.contains("post_") && name != "flatten" && !name.contains("inflate") {
            return name;
        }
    }
}

/// Random offset and rotation of the JulianDisc transforms.
fn julian_disc_place(xf: &mut XForm, r: &mut Rng) {
    if r.random() < 0.33 {
        global_translate(xf, -0.0125 + 0.025 * r.random(), -0.0125 + 0.025 * r.random(), false);
    } else if r.random() < 0.75 {
        global_translate(xf, -0.125 + 0.25 * r.random(), -0.125 + 0.25 * r.random(), false);
    }
    if r.random() < 0.15 {
        rotate(xf, 6.0 - r.random() * 12.0, false);
    } else if r.random() < 0.3 {
        rotate(xf, -45.0, false);
    } else if r.random() < 0.75 {
        rotate(xf, 90.0 - r.random() * 180.0, false);
    }
}

/// `JulianDiscRandomFlameGenerator`.
pub fn julian_disc(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0 + r.random() * 60.0);
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 0.5 + r.random() * 0.5;
        let mut power = ftoi(200.0 - r.random() * 400.0);
        if matches!(power, -1..=1) {
            power = -30;
        }
        let amounts = [0.25, 0.5, 0.75];
        let amount = amounts[(r.random() * amounts.len() as f64) as usize];
        set(add(&mut xf, amount, "julian"), &[("power", power as f64)]);
        if r.random() < 0.5 {
            let name = if r.random() > 0.5 { non_blur_random_func(r) } else { "gaussian_blur" };
            add(&mut xf, 0.001 + r.random() * 0.039, name);
        }
        xf.color_symmetry = -1.0;
        xf.color = 0.0;
        julian_disc_place(&mut xf, r);
        if r.random() < 0.5 {
            scale(&mut xf, 1.2 - r.random() * 0.4, true, true, false);
        }
        if r.random() < 0.5 {
            scale(&mut xf, 1.2 - r.random() * 0.4, true, true, true);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 2.0 + r.random() * 24.0;
        add(&mut xf, 1.0, "disc");
        if r.random() < 0.125 {
            let name = if r.random() > 0.5 { non_blur_random_func(r) } else { "gaussian_blur" };
            add(&mut xf, 0.001 + r.random() * 0.039, name);
        }
        xf.color = 0.5 + r.random() * 0.5;
        xf.color_symmetry = 0.6 + r.random() * 0.33;
        julian_disc_place(&mut xf, r);
        layer.xforms.push(xf);
    }
    if r.random() < 0.15 {
        let mut xf = xform();
        if r.random() < 0.5 {
            static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
            let list = LIST.get_or_init(|| {
                java::filter(&[
                    "auger",
                    "bent",
                    "bent2",
                    "boarders",
                    "bubble",
                    "butterfly",
                    "bwraps7",
                    "cosine",
                    "curve",
                    "cylinder",
                    "diamond",
                    "disc",
                    "eclipse",
                    "edisc",
                    "elliptic",
                    "ex",
                    "exp",
                    "exponential",
                    "eyefish",
                    "fan",
                    "fan2",
                    "fisheye",
                    "heart_wf",
                    "hemisphere",
                    "horseshoe",
                    "hyperbolic",
                    "julia",
                    "julia3D",
                    "julia3Dz",
                    "julian",
                    "juliascope",
                    "linearT",
                    "log",
                    "mobius",
                    "ngon",
                    "oscilloscope",
                    "rings",
                    "rings2",
                    "scry",
                    "xtrb",
                    "sec",
                    "sin",
                    "sinh",
                    "sinusoidal",
                    "spherical",
                    "swirl",
                    "tan",
                    "tangent",
                    "tanh",
                    "boarders2",
                    "polar",
                ])
            });
            let name = pick(r, list);
            add(&mut xf, 2.0 + r.random() * 2.0, name);
        }
        layer.final_xforms.push(xf);
    }
    randomize_colors(layer, r);
    flame
}

/// `JulianRingsRandomFlameGenerator`.
pub fn julian_rings(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = java::flame();
    flame.cam_roll = r.random() * 360.0;
    flame.width = 962;
    flame.height = 541;
    flame.pixels_per_unit = 100.0 + r.random() * 200.0;
    flame.cam_zoom = 3.0 + r.random() * 2.0;
    flame.bg_transparency = false;
    if r.random() > 0.5 {
        flame.background.kind = BgColorType::SingleColor;
        flame.background.color = [255, 255, 255];
    }
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 1.0 + r.random();
        xf.color = r.random();
        xf.color_symmetry = -1.0 + 2.0 * r.random();
        xf.coefs = affine(1.87375, 0.0, 0.0, 1.873754, 0.0, 0.0);
        static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
        let list = LIST
            .get_or_init(|| java::filter(&["julia3D", "jubiQ", "julia3Dq", "julia3Dz", "julian", "julian2", "julian3Dx", "juliaq", "juliascope"]));
        let name = pick(r, list);
        let power = if r.random() > 0.5 { 10.0 + r.random() * 5000.0 } else { -2.0 - r.random() * 100.0 };
        set(add(&mut xf, 1.0, name), &[("power", power)]);
        if r.random() > 0.75 {
            let name = java::random_variation(r);
            add(&mut xf, -0.125 + r.random() * 0.25, name);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 25.0 + r.random() * 25.0;
        xf.color = r.random();
        xf.color_symmetry = -1.0 + 2.0 * r.random();
        xf.coefs = affine(0.990702, -0.136053, 0.154938, 0.92994, -0.140437, 0.159563);
        let amount = if r.random() > 0.33 { 1.0 } else { 0.9 + r.random() * 0.3 };
        set(add(&mut xf, amount, "rings2"), &[("val", 1.0)]);
        if r.random() > 0.5 {
            let name = java::random_variation(r);
            add(&mut xf, -0.25 + r.random() * 0.5, name);
        }
        rotate(&mut xf, 45.0 - 90.0 * r.random(), false);
        local_translate(&mut xf, -0.125 + 0.25 * r.random(), -0.125 + 0.25 * r.random(), false);
        layer.xforms.push(xf);
    }
    flame
}

/// `EDiscRandomFlameGenerator`.
pub fn edisc(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = java::flame();
    flame.width = 638;
    flame.height = 359;
    flame.cam_roll = 360.0 * r.random();
    flame.centre_x = 0.5 - r.random();
    flame.centre_y = 0.5 - r.random();
    flame.pixels_per_unit = 90.0;
    flame.cam_zoom = 0.55 + r.random() * 0.25;
    flame.preserve_z = true;
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 3.5 + r.random();
        xf.color = r.random();
        xf.color_symmetry = 0.9;
        xf.coefs = affine(0.88067445, 0.50058358, -0.50058358, 0.88067445, 0.111166, -0.873736);
        xf.post = affine(-0.172546, 1.052875, -1.052875, -0.172546, -0.057973, 0.051023);
        set_xaos(&mut xf, 0, 1.0 + r.random() * 10.0);
        set_xaos(&mut xf, 2, 0.0);
        add(&mut xf, 15.0, "edisc");
        scale(&mut xf, 1.25 - r.random() * 0.5, true, true, false);
        rotate(&mut xf, 360.0 * r.random(), false);
        layer.xforms.push(xf);
    }
    let with_tx3 = r.random() > 0.666;
    {
        let mut xf = xform();
        xf.weight = 0.5 + r.random();
        xf.color = r.random();
        xf.color_symmetry = 0.0;
        xf.draw_mode = DrawMode::Hidden;
        xf.coefs = affine(1.0, 0.0, 0.0, 1.0, 0.166332, -0.620971);
        if with_tx3 {
            set_xaos(&mut xf, 0, 0.0);
            set_xaos(&mut xf, 1, 0.0);
        } else {
            set_xaos(&mut xf, 0, 1.0);
            set_xaos(&mut xf, 1, 1.0 + r.random() * 2.0);
        }
        static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
        let list = LIST.get_or_init(|| java::filter(&["juliascope3Db", "juliascope", "julian", "julian2", "julian3Dx"]));
        let name = pick(r, list);
        let (power, dist) = (50.0 + r.random() * 100.0, 2.0 + r.random() * 6.0);
        let amount = 5.0 + r.random() * 20.0;
        set(add(&mut xf, amount, name), &[("power", power), ("dist", dist)]);
        layer.xforms.push(xf);
    }
    if with_tx3 {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.color = r.random();
        xf.color_symmetry = 1.0;
        xf.color_type = ColorType::None;
        set_xaos(&mut xf, 1, 1.0 + r.random() * 2.0);
        set_xaos(&mut xf, 2, 0.0);
        let name = java::random_variation(r);
        add(&mut xf, 0.25 - r.random() * 0.5, name);
        layer.xforms.push(xf);
    }
    flame
}

// ---------------------------------------------------------------------------------------
// Spirals, Brokat

/// `SpiralsRandomFlameGenerator`.
pub fn spirals(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = java::flame();
    flame.centre_x = -r.random() * 0.5;
    flame.centre_y = -r.random() * 0.5;
    flame.pixels_per_unit = 200.0;
    flame.cam_roll = 90.0 - r.random() * 180.0;
    flame.cam_zoom = 2.0;
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 25.0 + r.random() * 55.0;
        xf.coefs = affine(0.23168009, 1.01859563, -0.87153216, 0.23718475, -1.09851548, 0.30609214);
        let sx = 0.04933602 + r.random() * 0.04;
        set(add(&mut xf, 1.0, "waves2"), &[("scalex", sx), ("scaley", 0.06933602), ("freqx", 2.98088993), ("freqy", 2.98088993)]);
        let name = java::random_variation(r);
        add(&mut xf, 0.001 + r.random() * 0.001, name);
        if r.random() < 0.33 {
            let name = java::random_variation(r);
            add_prio(&mut xf, 0.0001 + r.random() * 0.0001, name, -1);
        }
        xf.color = 0.4 + r.random() * 0.2;
        xf.color_symmetry = 0.82 + r.random() * 0.16;
        local_translate(&mut xf, 0.5 * (0.5 - r.random()), 0.5 - r.random(), true);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.coefs = affine(1.1144261, 0.03033403, -0.1144261, 0.96966597, -0.1144261, -0.03033403);
        let name = java::random_variation(r);
        add(&mut xf, 0.2 + r.random() * 0.2, name);
        if r.random() > 0.42 {
            let name = java::random_variation(r);
            add_prio(&mut xf, 0.1 + r.random() * 0.1, name, -1);
        }
        if r.random() > 0.42 {
            let name = java::random_variation(r);
            add_prio(&mut xf, 0.0001 + r.random() * 0.0001, name, 1);
        }
        xf.color = r.random();
        xf.color_symmetry = r.random();
        layer.xforms.push(xf);
    }
    flame
}

/// `BrokatRandomFlameGenerator`.
pub fn brokat(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.cam_zoom = 2.0;
    flame.cam_roll = -90.0;
    let layer = flame.first_layer_mut();
    let exp = experimental();
    {
        let mut xf = xform();
        xf.weight = 1.5 + r.random() * 1.0;
        let c2 = 0.001 + r.random() * 0.0199;
        let amount = 1.6 + r.random() * 0.8;
        set(add(&mut xf, amount, "curl"), &[("c1", -1.0), ("c2", c2)]);
        xf.color = 0.4 + r.random() * 0.2;
        xf.color_symmetry = 0.82 + r.random() * 0.16;
        rotate(&mut xf, 180.0, false);
        local_translate(&mut xf, 1.0, 0.0, true);
        for (i, w) in [0.0, 1.0, 0.0, 0.0].into_iter().enumerate() {
            set_xaos(&mut xf, i, w);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.05 + r.random() * 0.35;
        let names = ["juliascope", "julia3D", "julia3Dz", "julian"];
        let name = names[rand_int(r, names.len())];
        let power = if r.random() < 0.33 {
            2.0
        } else if r.random() < 0.5 {
            3.0
        } else {
            4.0
        };
        set(add(&mut xf, 1.0, name), &[("power", power)]);
        xf.color = 0.5 + r.random() * 0.5;
        xf.color_symmetry = 0.5;
        for (i, w) in [1.0, 0.0, 1.0, 1.0].into_iter().enumerate() {
            set_xaos(&mut xf, i, w);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.4 + r.random() * 0.2;
        let name = if r.random() < 0.33 { "bubble" } else { exp[rand_int(r, exp.len())] };
        add(&mut xf, 0.01 + r.random() * 0.04, name);
        add(&mut xf, 5.0 + r.random() * 10.0, "pre_blur");
        xf.color = 0.1 + r.random() * 0.3;
        xf.color_symmetry = 0.0;
        local_translate(&mut xf, -1.0, 0.0, true);
        for (i, w) in [1.0, 1.0, 0.0, 0.0].into_iter().enumerate() {
            set_xaos(&mut xf, i, w);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.4 + r.random() * 0.2;
        let name = exp[rand_int(r, exp.len())];
        add(&mut xf, 0.01 + r.random() * 0.04, name);
        if r.random() > 0.5 {
            let amount = (0.01 + r.random() * 0.04) * 0.5;
            let name = java::random_variation(r);
            add_prio(&mut xf, amount, name, -1);
        }
        xf.color = 0.1 + r.random() * 0.3;
        xf.color_symmetry = 0.0;
        rotate(&mut xf, r.random() * 360.0, true);
        scale(&mut xf, 1.1 + r.random() * 3.0, true, true, true);
        layer.xforms.push(xf);
    }
    flame
}

// ---------------------------------------------------------------------------------------
// Gnarl

/// `GnarlRandomFlameGenerator.createWaves2BVariation`.
fn waves2b(r: &mut Rng, sx: f64, sy: f64, fx: f64, fy: f64) -> VariationInstance {
    let mut w = VariationInstance::new("waves2b", 1.0);
    let mut pwx = 3.0 * r.random();
    pwx = if pwx < 1.0 {
        1.0
    } else if pwx < 2.0 {
        (if pwx - 1.5 > 0.0 { -1.0 } else { 1.0 }) * (0.5 + 2.0 * r.random())
    } else {
        (if pwx - 2.5 > 0.0 { -1.0 } else { 1.0 }) * 1e-6 * (0.9 - r.random() * 0.2)
    };
    let mut pwy = 3.0 * r.random();
    pwy = if pwy < 1.5 {
        1.0
    } else if pwy < 2.25 {
        (if pwy - 1.5 > 0.0 { -1.0 } else { 1.0 }) * (0.5 + 2.0 * r.random())
    } else {
        (if pwy - 2.5 > 0.0 { -1.0 } else { 1.0 }) * 1e-6
    };
    set(&mut w, &[("pwx", pwx), ("pwy", pwy)]);
    let six = if r.random() < 0.5 { sx } else { 0.5 - r.random() };
    let siy = if r.random() < 0.5 { sy } else { 0.5 - r.random() };
    set(&mut w, &[("scaleinfx", six), ("scaleinfy", siy)]);
    if r.random() > 0.75 {
        let u = r.random() * 10.0;
        set(&mut w, &[("unity", if u < 5.0 { u - 1.0 } else { u + 5.0 })]);
    }
    if r.random() < 0.33 {
        set(&mut w, &[("jacok", 0.375 - r.random() * 0.75)]);
    }
    set(&mut w, &[("freqx", fx), ("scalex", sx), ("freqy", fy), ("scaley", sy)]);
    w
}

/// `GnarlRandomFlameGenerator.createWaves2Variation`.
fn waves2(sx: f64, sy: f64, fx: f64, fy: f64) -> VariationInstance {
    let mut w = VariationInstance::new("waves2", 1.0);
    set(&mut w, &[("freqx", fx), ("scalex", sx), ("freqy", fy), ("scaley", sy)]);
    w
}

/// `GnarlRandomFlameGenerator.createCrazyWavesVariation`.
fn crazy_waves(r: &mut Rng, sx: f64, sy: f64, fx: f64, fy: f64) -> VariationInstance {
    let sel = r.random();
    let basic = [("freqx", fx), ("scalex", sx), ("freqy", fy), ("scaley", sy)];
    if sel < 0.16 {
        let mut w = VariationInstance::new("waves22", 1.0);
        set(&mut w, &basic);
        let (px, py) = (2.0 + r.random() * 7.0, 2.0 + r.random() * 7.0);
        set(&mut w, &[("modex", 1.0), ("modey", 1.0), ("powerx", px), ("powery", py)]);
        w
    } else if sel < 0.34 {
        let mut w = VariationInstance::new("waves23", 1.0);
        set(&mut w, &basic);
        w
    } else if sel < 0.61 {
        let mut w = VariationInstance::new("vibration2", 1.0);
        let tau = 2.0 * PI;
        let p = [
            ("dir", r.random() * tau),
            ("angle", r.random() * tau),
            ("freq", 0.5 + r.random()),
            ("amp", 0.25 + r.random() * 0.5),
            ("phase", r.random() * PI),
            ("dir2", r.random() * tau),
            ("angle2", r.random() * tau),
            ("freq2", 0.5 + r.random()),
            ("amp2", 0.25 + r.random() * 0.5),
            ("phase2", r.random() * PI),
        ];
        set(&mut w, &p);
        for (m, mf) in [
            ("dm", "dmfreq"),
            ("tm", "tmfreq"),
            ("fm", "fmfreq"),
            ("am", "amfreq"),
            ("d2m", "d2mfreq"),
            ("t2m", "t2mfreq"),
            ("f2m", "f2mfreq"),
            ("a2m", "a2mfreq"),
        ] {
            if r.random() > 0.25 {
                let (a, b) = (r.random() * 0.1, 0.1 + r.random() * 0.6);
                set(&mut w, &[(m, a), (mf, b)]);
            }
        }
        w
    } else if sel < 0.821 {
        let mut w = VariationInstance::new("waves4", 1.0);
        set(&mut w, &basic);
        let yfact = 0.1 + r.random() * 0.49;
        set(&mut w, &[("cont", 1.0), ("yfact", yfact)]);
        w
    } else {
        let mut w = VariationInstance::new("waves42", 1.0);
        set(&mut w, &basic);
        let (yfact, fx2) = (0.1 + r.random() * 0.59, 0.5 + r.random() * 0.25);
        set(&mut w, &[("cont", 1.0), ("yfact", yfact), ("freqx2", fx2)]);
        w
    }
}

/// Rotation and translation range of the waves transform for a number of sides.
fn gnarl_sides(r: &mut Rng, sides: i32) -> (f64, f64, f64) {
    let (angle, span, off) = match sides {
        2 => (-180.0, 8.0, 4.0),
        3 => (-120.0, 8.0, 4.0),
        4 => (-90.0, 8.0, 4.0),
        5 => (-72.0, 7.0, 3.5),
        6 => (-60.0, 7.0, 3.5),
        7 => (-51.42857, 6.0, 3.0),
        8 => (-135.0, 6.0, 3.0),
        9 => (-40.0, 5.0, 2.5),
        10 => (-36.0, 4.0, 2.0),
        11 => (-32.73, 4.2, 3.0),
        _ => (-30.0, 4.2, 3.0),
    };
    let tx = r.random() * span - off;
    let ty = r.random() * span - off;
    (angle, tx, ty)
}

/// Wave scale shared by both gnarl generators.
fn gnarl_scales(r: &mut Rng) -> (f64, f64) {
    let mut sx = r.random() * 0.04 + 0.04;
    if r.random() > 0.75 {
        sx = -sx;
    }
    let mut sy = r.random() * 0.04 + 0.04;
    if r.random() > 0.75 {
        sy = -sy;
    } else if r.random() < 0.25 {
        sy = sx;
    }
    (sx, sy)
}

/// `GnarlRandomFlameGenerator`.
pub fn gnarl(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    let layer = flame.first_layer_mut();
    let (sx, sy) = gnarl_scales(r);
    let (fx, fy);
    if r.random() < 0.5 {
        fx = r.random() * 2.0 + 2.0;
        fy = if r.random() > 0.5 { fx } else { r.random() * 2.0 + 2.0 };
    } else {
        fx = -r.random() * 2.0 + 2.0;
        fy = if r.random() > 0.5 { fx } else { -r.random() * 2.0 + 16.0 };
    }
    let mut blur = 0.0025 * r.random();
    if r.random() < 0.33 {
        blur = -blur;
    }
    let waves_weight = r.random() * 15.0 + 135.0;
    let symmetry = 0.7 + r.random() * 0.3;
    let sides = (r.random() * 8.0 + 3.0) as i32;
    {
        let mut xf = xform();
        xf.weight = waves_weight;
        let mut w = if r.random() > 0.333 {
            if r.random() > 0.666 { waves2(sx, sy, fx, fy) } else { crazy_waves(r, sx, sy, fx, fy) }
        } else {
            waves2b(r, sx, sy, fx, fy)
        };
        w.amount = 1.0 + r.random() * 0.001;
        xf.variations.push(w);
        static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
        let list = LIST.get_or_init(|| {
            java::filter(&[
                "blur",
                "cos",
                "exp",
                "exponential",
                "lazysusan",
                "ngon",
                "sech",
                "sinh",
                "epispiral_wf",
                "tanh",
                "twintrian",
                "epispiral",
            ])
        });
        let name = if r.random() > 0.25 { pick(r, list) } else { java::random_variation(r) };
        add(&mut xf, blur, name);
        xf.color_symmetry = symmetry;
        xf.color = 0.9;
        if r.random() > 0.5 {
            scale(&mut xf, 0.995, true, true, false);
        }
        let (angle, tx, ty) = gnarl_sides(r, sides);
        rotate(&mut xf, angle, false);
        local_translate(&mut xf, tx * 1.5, ty * 1.5, false);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.5;
        match rand_int(r, 3) {
            0 => {
                add(&mut xf, r.random() * 0.37 + 0.1, "radial_blur");
            }
            1 => {
                add(&mut xf, r.random() * 0.37 + 0.3, "bubble");
                add(&mut xf, r.random() * 0.37 + 0.1, "radial_blur");
            }
            _ => {
                add(&mut xf, r.random() * 0.1, "radial_blur");
                let (power, dist) = (50.0 - r.random() * 100.0, r.random() * 10.0 - 2.0);
                let amount = r.random() * 0.06 + 0.1;
                set(add(&mut xf, amount, "julian"), &[("power", power), ("dist", dist)]);
            }
        }
        xf.color_symmetry = -1.0;
        xf.color = r.random();
        layer.xforms.push(xf);
    }
    if r.random() > 0.75 {
        layer.xforms.push(gnarl_linear(r));
    }
    flame
}

/// The optional third, linear transform of the gnarl generators.
fn gnarl_linear(r: &mut Rng) -> XForm {
    let mut xf = xform();
    xf.weight = 0.5;
    add(&mut xf, 1.0, "linear3D");
    rotate(&mut xf, 180.0 - r.random() * 360.0, false);
    local_translate(&mut xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), false);
    xf.color_symmetry = -1.0;
    xf.color = r.random();
    xf
}

/// `ExperimentalGnarlRandomFlameGenerator` ("Gnarl (experimental)").
pub fn experimental_gnarl(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    let layer = flame.first_layer_mut();
    let (sx, sy) = gnarl_scales(r);
    let fx = r.random() * 2.0 + 7.0;
    let fy = if r.random() < 0.25 { fx } else { r.random() * 2.0 + 7.0 };
    let blur = 0.0025 * r.random();
    let non_blur = 0.25 * r.random();
    let waves_weight = r.random() * 10.0 + 75.0;
    let symmetry = 0.6 + r.random() * 0.4;
    let sides = (r.random() * 11.0 + 2.0) as i32;
    {
        let mut xf = xform();
        xf.weight = waves_weight;
        let var_rnd = r.random();
        let mut w = if r.random() > 0.666 {
            let name = if var_rnd < 0.6 {
                "waves2_wf"
            } else if var_rnd < 0.8 {
                "waves3_wf"
            } else {
                "waves4_wf"
            };
            let mut w = VariationInstance::new(name, 1.0);
            let cx = if r.random() < 0.25 { 1.0 } else { 0.0 };
            let cy = if r.random() < 0.25 { 1.0 } else { 0.0 };
            set(&mut w, &[("use_cos_x", cx), ("use_cos_y", cy)]);
            if r.random() < 0.25 {
                set(&mut w, &[("dampx", -(0.01 + r.random() * 0.39))]);
            }
            if r.random() < 0.25 {
                set(&mut w, &[("dampy", -(0.01 + r.random() * 0.39))]);
            }
            set(&mut w, &[("freqx", fx), ("scalex", sx), ("freqy", fy), ("scaley", sy)]);
            w
        } else {
            let sel = r.random();
            if sel < 0.25 {
                waves2(sx, sy, fx, fy)
            } else if sel < 0.666 {
                crazy_waves(r, sx, sy, fx, fy)
            } else {
                waves2b(r, sx, sy, fx, fy)
            }
        };
        w.amount = 1.0;
        xf.variations.push(w);
        const SIMPLE: [&str; 13] =
            ["blur", "cos", "exp", "exponential", "lazysusan", "ngon", "sech", "sinh", "epispiral_wf", "tanh", "twintrian", "bubble", "epispiral"];
        match rand_int(r, 36) {
            i @ (0 | 11) => {
                add(&mut xf, blur, SIMPLE[i]);
            }
            i @ 1..=12 => {
                add(&mut xf, non_blur, SIMPLE[i]);
            }
            _ => {
                let exp = experimental();
                let name = exp[rand_int(r, exp.len())];
                add(&mut xf, blur, name);
            }
        }
        xf.color_symmetry = symmetry;
        xf.color = 0.9;
        if r.random() > 0.5 {
            scale(&mut xf, 0.9 + r.random() * 0.09, true, true, false);
        }
        let (angle, tx, ty) = gnarl_sides(r, sides);
        rotate(&mut xf, angle, false);
        local_translate(&mut xf, tx, ty, false);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.5;
        match rand_int(r, 4) {
            0 => {
                add(&mut xf, r.random() * 0.7 + 0.1, "radial_blur");
            }
            1 => {
                add(&mut xf, r.random() * 0.7 + 0.3, "bubble");
                add(&mut xf, r.random() * 0.7 + 0.1, "radial_blur");
            }
            2 => {
                add(&mut xf, r.random() * 0.1, "radial_blur");
                let (power, dist) = (50.0 - r.random() * 100.0, r.random() * 10.0 - 2.0);
                let amount = r.random() * 0.06 + 0.1;
                set(add(&mut xf, amount, "julian"), &[("power", power), ("dist", dist)]);
            }
            _ => {
                add(&mut xf, r.random() * 0.7 + 0.3, "spherical3D");
            }
        }
        rotate(&mut xf, 180.0 - r.random() * 360.0, false);
        if r.random() > 0.5 {
            scale(&mut xf, 0.5 + r.random(), true, true, false);
        }
        xf.color_symmetry = -1.0;
        xf.color = r.random();
        layer.xforms.push(xf);
    }
    if r.random() > 0.75 {
        if r.random() > 0.5 {
            layer.xforms[1].weight = 5.0 + r.random() * 20.0;
        }
        layer.xforms.push(gnarl_linear(r));
    }
    flame
}

// ---------------------------------------------------------------------------------------
// Synth

/// `SynthRandomFlameGenerator`.
pub fn synth(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(300.0 + r.random() * 300.0);
    let layer = flame.first_layer_mut();
    let centre_mode = rand_int(r, 20) as f64;
    let centre_color = 0.4 + 0.2 * r.random();
    let centre_symmetry = 0.6 + 0.4 * r.random();
    let mut xf1 = xform();
    xf1.weight = 1.0;
    xf1.color = 0.0;
    xf1.color_symmetry = -1.0;
    {
        let mut s = VariationInstance::new("synth", 1.0);
        let mode = rand_int(r, 20) as f64;
        set(&mut s, &[("mode", mode), ("power", -0.2)]);
        let num_waves = (r.random() * 3.5 + 2.0) as i32;
        let a = 0.8 + r.random() * 0.4;
        set(&mut s, &[("a", a)]);
        // b and c always, d, e, f with more waves.
        for (wave, min_waves) in [("b", 0), ("c", 0), ("d", 3), ("e", 4), ("f", 5)] {
            if num_waves < min_waves {
                continue;
            }
            let k = |suffix: &str| format!("{wave}_{suffix}");
            let mut amp = if wave == "b" { r.random() * 2.0 } else { r.random() };
            let typ = rand_int(r, 7) as f64;
            let mut frq = (rand_int(r, 7) + 1) as f64;
            let phs = r.random() * PI;
            let layer_no = rand_int(r, 4) as f64;
            set(&mut s, &[(wave, amp), (&k("type"), typ), (&k("frq"), frq), (&k("phs"), phs), (&k("layer"), layer_no)]);
            if r.random() < 0.2 {
                set(&mut s, &[(&k("skew"), r.random() * 2.0 - 1.0)]);
            }
            if r.random() < 0.1 {
                frq = (rand_int(r, 20) + 7) as f64;
                set(&mut s, &[(&k("frq"), frq)]);
            }
            if r.random() < 0.8 {
                amp /= 1.0 + 0.3 * frq;
                set(&mut s, &[(wave, amp)]);
            }
        }
        xf1.variations.push(s);
    }
    let mut xf2 = xf1.clone();
    xf2.weight = 1.0;
    xf2.color = centre_color;
    xf2.color_symmetry = centre_symmetry;
    xf2.variations[0].amount = 0.7;
    set(&mut xf2.variations[0], &[("power", -1.0), ("mode", centre_mode), ("smooth", 1.0)]);
    add(&mut xf2, 0.0, "noise");
    layer.xforms.push(xf1);
    layer.xforms.push(xf2);
    if r.random() < 0.55 {
        let mut xf = xform();
        xf.weight = 5.0 * r.random() + 0.125;
        xf.color = centre_color + 0.2 * r.random();
        xf.color_symmetry = centre_symmetry - 0.4 * r.random();
        let name = java::random_variation(r);
        add(&mut xf, 0.25 + r.random() * 1.5, name);
        set_xaos(&mut xf, 1, 0.0);
        layer.xforms.push(xf);
    }
    flame
}
