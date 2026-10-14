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

//! Ported generators: Cross, Duality, Galaxies, Ghosts, Machine, Mandelbrot, Orchids,
//! Outlines, Phoenix and Raster.

use std::f64::consts::PI;
use std::sync::OnceLock;

use super::{State, mutations};
use super::gens_b::{add_prio, affine, base};
use super::java::{self, Plane, add, local_translate, pick, rand_int, randomize_colors, rotate, scale, set, set_xaos, xform};
use crate::model::Flame;

/// `Math.random() < 0.5 ? -1 : 1` as a variation priority.
fn pre_or_post(r: &mut crate::rng::Rng) -> i32 {
    if r.random() < 0.5 { -1 } else { 1 }
}

/// `CrossRandomFlameGenerator`.
pub fn cross(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    if r.random() > 0.6 {
        flame.new_cam_dof = true;
        flame.cam_dof = r.random() * 0.07;
    } else {
        flame.new_cam_dof = false;
        flame.cam_dof = r.random() * 0.2;
    }
    flame.cam_pitch = 10.0 + r.random() * 50.0;
    flame.preserve_z = r.random() > 0.33;
    flame.cam_perspective = 0.10 + r.random() * 0.5;
    let layer = flame.first_layer_mut();
    for (i, (px, py)) in [(1.0, -1.0), (-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)].into_iter().enumerate() {
        let mut xf = xform();
        xf.weight = 0.5;
        match i {
            0 => xf.color_symmetry = 0.99 - r.random() * 0.2,
            1 => xf.color_symmetry = 0.8 - r.random() * 0.1,
            2 => {
                xf.color = 0.1 + r.random() * 0.1;
                xf.color_symmetry = 0.92;
            }
            _ => xf.color_symmetry = 0.9 - r.random() * 0.2,
        }
        xf.post = affine(1.0, 0.0, 0.0, 1.0, px, py);
        add(&mut xf, 1.0, "linear3D");
        layer.xforms.push(xf);
    }
    let diagonal = affine(0.70711, -0.70711, 0.70711, 0.70711, 0.0, 0.0);
    {
        let mut xf = xform();
        xf.weight = 0.05 + r.random() * 0.2;
        xf.color_symmetry = 0.10 + r.random() * 0.2;
        xf.post = diagonal;
        let name = java::random_variation(r);
        add(&mut xf, 0.45, name);
        layer.xforms.push(xf);
    }
    if r.random() > 0.75 {
        let mut xf = xform();
        xf.weight = 0.05 + r.random() * 0.2;
        xf.color_symmetry = 0.10 + r.random() * 0.2;
        xf.post = diagonal;
        set_xaos(&mut xf, 5, 0.0);
        for i in 0..5 {
            set_xaos(&mut layer.xforms[4], i, 0.0);
        }
        let name = java::random_variation(r);
        add(&mut xf, 0.1 + r.random() * 0.6, name);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        rotate(&mut xf, r.random() * 360.0, true);
        local_translate(&mut xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), true);
        if r.random() > 0.75 {
            static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
            let list = LIST.get_or_init(|| {
                java::filter(&[
                    "cross",
                    "boarders2",
                    "boarders",
                    "butterfly",
                    "cos",
                    "cosh",
                    "cosine",
                    "csc",
                    "cylinder",
                    "cylinder_apo",
                    "dc_ztransl",
                    "elliptic",
                    "eyefish",
                    "fibonacci2",
                    "heart_wf",
                    "hypertile1",
                    "loonie",
                    "mobius",
                    "perspective",
                    "popcorn",
                    "popcorn2_3D",
                    "ripple",
                    "roundspher3D",
                    "scry_3D",
                    "sec",
                    "secant2",
                    "separation",
                    "shredlin",
                    "sin",
                    "spherical",
                    "spiral",
                    "stripes",
                    "unpolar",
                    "waves2",
                    "waves4_wf",
                    "whorl",
                    "xtrb",
                    "rays1",
                    "rays2",
                    "rays3",
                ])
            });
            let name = if r.random() < 0.25 {
                "cross"
            } else if r.random() < 0.25 {
                "rays2"
            } else {
                pick(r, list)
            };
            add(&mut xf, 1.57, name);
        } else {
            let three_d = r.random() <= 0.75;
            let name = java::random_variation_of(r, three_d);
            add(&mut xf, 1.0, name);
        }
        layer.final_xforms.push(xf);
    }
    randomize_colors(layer, r);
    flame
}

/// `DualityRandomFlameGenerator` (without preferred variations from the preferences).
pub fn duality(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    let layer = flame.first_layer_mut();
    let mutations = |layer: &mut crate::model::Layer, r: &mut crate::rng::Rng| {
        for p in [0.25, 0.50, 0.75] {
            if r.random() > p {
                mutations::random_param_mutation(layer, 1.0, r);
            }
        }
    };
    {
        let mut xf = xform();
        xf.weight = 0.01 + r.random() * 50.0;
        xf.color = 0.74488914;
        xf.color_symmetry = 0.0;
        let name = java::random_variation_no_fract(r, true);
        add(&mut xf, 1.0, name);
        local_translate(&mut xf, 2.0 - 4.0 * r.random(), 2.0 - 4.0 * r.random(), false);
        layer.xforms.push(xf);
    }
    mutations(layer, r);
    {
        let mut xf = xform();
        xf.weight = 0.1 + r.random() * 400.0;
        xf.color = 0.90312262;
        xf.color_symmetry = 0.95;
        let amount = if r.random() < 0.33 { 0.5 + r.random() * 0.5 } else { 1.0 };
        let name = java::random_variation_no_fract(r, true);
        add(&mut xf, amount, name);
        scale(&mut xf, 1.25 - r.random() * 0.5, true, true, false);
        rotate(&mut xf, 36.0 * r.random(), false);
        local_translate(&mut xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), false);
        if r.random() < 0.5 {
            let w = if r.random() < 0.5 { 0.1 + r.random() * 0.5 } else { 3.0 + r.random() * 7.0 };
            set_xaos(&mut xf, 0, w);
        }
        layer.xforms.push(xf);
    }
    mutations(layer, r);
    if r.random() < 0.1 {
        let mut xf = xform();
        xf.weight = 0.01 + r.random() * 200.0;
        xf.color = 0.6312262;
        xf.color_symmetry = 0.12;
        let amount = 0.5 + r.random() * 0.75;
        let name = java::random_variation_no_fract(r, true);
        add(&mut xf, amount, name);
        set_xaos(&mut xf, 2, 0.0);
        scale(&mut xf, 1.05 - r.random() * 0.45, true, true, false);
        rotate(&mut xf, 24.0 * r.random(), false);
        local_translate(&mut xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), false);
        layer.xforms.push(xf);
    }
    mutations(layer, r);
    flame
}

/// `GalaxiesRandomFlameGenerator`.
pub fn galaxies(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = java::flame();
    flame.cam_roll = 1.49758722;
    flame.width = 601;
    flame.height = 338;
    flame.pixels_per_unit = 92.48366013;
    flame.cam_zoom = 0.72 + r.random() * 0.42;
    flame.centre_x = 1.5357526;
    flame.centre_y = -0.4416446;
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 25.75871591;
        xf.color = 0.74488914;
        xf.color_symmetry = 0.0;
        xf.coefs = affine(1.0, 0.0, 0.0, 1.0, 1.09171281, -1.22115911);
        let amount = if r.random() > 0.25 { 1.0 - 2.0 * r.random() } else { 1.0 };
        let name = java::random_variation_no_fract(r, false);
        add(&mut xf, amount, name);
        layer.xforms.push(xf);
        if r.random() > 0.5 {
            mutations::set_random_flame_property(layer, 1.0, r);
        }
    }
    {
        let mut xf = xform();
        xf.weight = 286.87636036;
        xf.color = 0.90312262;
        xf.color_symmetry = 0.95;
        xf.coefs = affine(0.96333808, -0.12845865, 0.12845865, 0.96333808, 0.31387449, 0.08003269);
        add(&mut xf, 1.0, "sec");
        layer.xforms.push(xf);
    }
    if r.random() > 0.5 {
        mutations::local_gamma_mutation(layer, r);
    }
    flame
}

/// `GhostsRandomFlameGenerator`.
pub fn ghosts(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.cam_zoom = 2.0;
    flame.cam_roll = -90.0;
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 1.5 + r.random();
        add(&mut xf, 0.5 + r.random(), "spherical");
        let three_d = r.random() <= 0.75;
        let name = java::random_variation_of(r, three_d);
        let amount = 0.05 + r.random() * 0.15;
        let prio = pre_or_post(r);
        add_prio(&mut xf, amount, name, prio);
        xf.color = 0.4 + r.random() * 0.2;
        xf.color_symmetry = 0.82 + r.random() * 0.16;
        rotate(&mut xf, 180.0, false);
        scale(&mut xf, 2.0 + r.random() * 25.0, true, true, false);
        local_translate(&mut xf, 0.5 * (0.5 - r.random()), 0.5 - r.random(), false);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 1.5 + r.random();
        xf.color = r.random();
        add(&mut xf, 0.05 + r.random() * 0.25, "linear");
        if r.random() < 0.66 {
            let angle = r.random() * PI;
            let amount = 0.025 + r.random() * 0.1;
            set(add(&mut xf, amount, "radial_blur"), &[("angle", angle)]);
        } else {
            let name = java::random_variation(r);
            add(&mut xf, 0.05 + r.random() * 0.5, name);
        }
        let name = java::random_variation(r);
        let amount = 0.05 + r.random() * 0.5;
        let prio = pre_or_post(r);
        add_prio(&mut xf, amount, name, prio);
        scale(&mut xf, 0.1 + r.random() * 0.8, false, true, false);
        scale(&mut xf, 1.0 + r.random() * 3.0, true, false, false);
        scale(&mut xf, 0.1 + r.random() * 0.8, true, true, false);
        rotate(&mut xf, 180.0, false);
        local_translate(&mut xf, 2.0 - 4.0 * r.random(), 0.5 * (2.0 - 4.0 * r.random()), false);
        xf.color_symmetry = r.random();
        layer.xforms.push(xf);
    }
    flame
}

/// `MachineRandomFlameGenerator`.
pub fn machine(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.centre_x = r.random() - 0.5;
    flame.centre_y = r.random() - 0.5;
    let layer = flame.first_layer_mut();
    let place = |xf: &mut crate::model::XForm, r: &mut crate::rng::Rng| {
        scale(xf, 1.25 - r.random() * 0.5, true, true, false);
        rotate(xf, 360.0 * r.random(), false);
        local_translate(xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), false);
    };
    {
        let mut xf = xform();
        xf.weight = 32.16406573;
        xf.color = 0.74488914;
        xf.color_symmetry = 0.0;
        xf.coefs = affine(1.0, 0.0, 0.0, 1.0, 1.82382673, -1.8206164);
        let name = java::random_variation(r);
        add(&mut xf, 1.0, name);
        place(&mut xf, r);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 257.3608644;
        xf.color = 0.90312262;
        xf.color_symmetry = 0.95;
        xf.coefs = affine(0.82496229, -0.55902149, 0.55902149, 0.82496229, 0.05455807, 0.73105973);
        set_xaos(&mut xf, 0, 0.54127492);
        let (radius, ang2) = (0.25 + r.random() * 0.5, 0.1 + r.random() * 0.5);
        let amount = 0.75 + r.random() * 0.5;
        set(add(&mut xf, amount, "yin_yang"), &[("radius", radius), ("ang1", 0.0), ("ang2", ang2), ("dual_t", 1.0), ("outside", 1.0)]);
        place(&mut xf, r);
        layer.xforms.push(xf);
    }
    randomize_colors(layer, r);
    flame
}

/// `MandelbrotRandomFlameGenerator`.
pub fn mandelbrot(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.centre_y = 0.45;
    flame.cam_pitch = 49.0;
    flame.gamma = 2.0;
    flame.cam_perspective = 0.05 + r.random() * 0.12;
    flame.preserve_z = true;
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 0.5;
        let var_id = rand_int(r, 6);
        let name = ["fract_dragon_wf", "fract_julia_wf", "fract_pearls_wf", "fract_salamander_wf", "fract_mandelbrot_wf", "fract_meteors_wf"][var_id];
        let mut v = crate::model::VariationInstance::new(name, 1.0);
        if var_id < 4 {
            let (xs, ys) = (-1.0 + 2.0 * r.random(), -1.0 + 2.0 * r.random());
            set(&mut v, &[("scale", 2.0), ("xmin", -2.0), ("xmax", 2.0), ("ymin", -2.0), ("ymax", 2.0), ("xseed", xs), ("yseed", ys)]);
        }
        if r.random() < 0.8 {
            let s = 3.0 + r.random() * 3.0;
            let xmin = -1.0 + 2.0 * r.random();
            let ymin = -1.0 + 2.0 * r.random();
            let (xmax, ymax) = (xmin + 4.0 / s, ymin + 4.0 / s);
            set(
                &mut v,
                &[
                    ("xmin", xmin),
                    ("xmax", xmax),
                    ("ymin", ymin),
                    ("ymax", ymax),
                    ("offsetx", -(xmax - xmin) * 0.5),
                    ("offsety", -(ymax - ymin) * 0.5),
                    ("scale", 2.0 * s),
                ],
            );
        }
        set(&mut v, &[("scalez", 1.0 + r.random() * 10.0)]);
        xf.variations.push(v);
        layer.xforms.push(xf);
    }
    if r.random() < 0.75 {
        static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
        let list = LIST.get_or_init(|| {
            java::filter(&[
                "bipolar",
                "boarders",
                "boarders2",
                "bubble",
                "butterfly",
                "bwraps7",
                "circlize",
                "collideoscope",
                "cosh",
                "cross",
                "curl",
                "curl3D",
                "curve",
                "eJulia",
                "edisc",
                "elliptic",
                "eyefish",
                "flux",
                "foci",
                "foci_3D",
                "glynnia",
                "heart_wf",
                "xtrb",
                "hemisphere",
                "horseshoe",
                "hypertile",
                "hypertile1",
                "hypertile2",
                "julia",
                "julian",
                "juliascope",
                "loonie",
                "loonie_3D",
                "mobius",
                "npolar",
                "phoenix_julia",
                "popcorn2_3D",
                "power",
                "ripple",
                "scry",
                "scry_3D",
                "sec",
                "sech",
                "separation",
                "spherical",
                "spiral",
                "stripes",
                "unpolar",
                "waves2",
                "whorl",
                "xheart",
            ])
        });
        let mut xf = xform();
        let name = pick(r, list);
        add(&mut xf, 1.0, name);
        layer.final_xforms.push(xf);
    }
    randomize_colors(layer, r);
    flame
}

/// `OrchidsRandomFlameGenerator`.
pub fn orchids(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = java::flame();
    flame.width = 638;
    flame.height = 359;
    flame.pixels_per_unit = 315.33902046;
    flame.cam_zoom = 2.2 + r.random() * 0.6;
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 4.0 + r.random() * 2.0;
        xf.color = 0.6 * r.random();
        add(&mut xf, 1.5 + r.random(), "elliptic");
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 1.5 + r.random();
        xf.color = 0.5 + r.random() * 0.2;
        xf.coefs = affine(-0.00283201, -0.99999599, 0.99999599, -0.00283201, 0.0, 0.0);
        let amount = r.random() + 1.0;
        set(add(&mut xf, amount, "poincare3D"), &[("r", 0.0), ("a", 0.0), ("b", 0.0)]);
        let amount = 10.0 * r.random() - 5.0;
        let name = java::random_variation(r);
        add(&mut xf, amount, name);
        scale(&mut xf, 0.5 + 2.0 * r.random(), true, true, false);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 1.2 + r.random();
        xf.color = 0.2 + r.random() * 0.5;
        add(&mut xf, 0.75, "rays");
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.color = 0.96;
        let amount = 0.3 + r.random() * 0.8;
        set(add(&mut xf, amount, "hypertile1"), &[("p", 4.0), ("q", 6.0)]);
        layer.final_xforms.push(xf);
    }
    {
        let mut xf = xform();
        if r.random() > 0.25 {
            let name = if r.random() < 0.5 { "polar" } else { "polar2" };
            add(&mut xf, 1.0, name);
        } else {
            let three_d = r.random() <= 0.75;
            let name = java::random_variation_of(r, three_d);
            add(&mut xf, 1.0, name);
        }
        layer.final_xforms.push(xf);
    }
    if r.random() > 0.666 {
        let w0 = 1.0 + r.random();
        let w2 = 0.2 * r.random();
        set_xaos(&mut layer.xforms[0], 0, w0);
        set_xaos(&mut layer.xforms[0], 2, w2);
    }
    flame
}

/// `OutlinesRandomFlameGenerator`.
pub fn outlines(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.centre_x = 0.1 - r.random() * 0.2;
    flame.centre_y = 0.1 - r.random() * 0.2;
    flame.cam_zoom = 1.05 + r.random() * 0.15;
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 26.32600695;
        xf.color = 0.74488914;
        xf.coefs = affine(1.0, 0.0, 0.0, 1.0, 1.25325706, -0.93791588);
        add(&mut xf, 1.0, "spherical3D");
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 224.48613855;
        xf.color = 0.90312262;
        xf.color_symmetry = 0.95;
        xf.coefs = affine(1.07039553, -0.4803511, 0.4803511, 1.07039553, -0.2742276, -0.04097021);
        if r.random() > 0.1 {
            let sides = (3 + (r.random() * 6.0) as i32) as f64;
            let (star, circle) = (0.1 + r.random() * 0.3, 0.1 + r.random() * 0.3);
            set(add(&mut xf, 1.0, "loonie2"), &[("sides", sides), ("star", star), ("circle", circle)]);
        } else {
            let name = java::random_variation(r);
            add(&mut xf, 1.0, name);
        }
        rotate(&mut xf, 360.0 * r.random(), true);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 146.3720822;
        xf.color = 0.6312262;
        xf.color_symmetry = 0.12;
        xf.coefs = affine(0.9031166, -0.38124873, 0.38124873, 0.9031166, 0.37716179, -0.53412667);
        set_xaos(&mut xf, 2, 0.0);
        let name = java::random_variation(r);
        add(&mut xf, 0.4 + r.random() * 0.2, name);
        rotate(&mut xf, 360.0 * r.random(), true);
        layer.xforms.push(xf);
    }
    flame
}

/// `PhoenixRandomFlameGenerator`.
pub fn phoenix(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = java::flame();
    flame.centre_x = -0.43687754;
    flame.centre_y = -0.84902392;
    flame.width = 1920;
    flame.height = 1080;
    flame.cam_zoom = 0.6 + r.random();
    flame.bg_transparency = false;
    flame.pixels_per_unit = 200.0;
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.color = 0.0;
        xf.coefs = affine(0.465496, 0.174105, -0.702038, 0.956244, -0.554347, -0.366901);
        add(&mut xf, 1.0, "linear");
        scale(&mut xf, 1.125 - r.random() * 0.25, true, true, false);
        rotate(&mut xf, 360.0 * r.random(), false);
        if r.random() > 0.5 {
            java::scale_in(&mut xf, Plane::YZ, 1.125 - r.random() * 0.25, true, true, false);
            java::rotate_in(&mut xf, Plane::YZ, 360.0 * r.random(), false);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.color = 1.0;
        xf.coefs = affine(0.706669, 0.515362, -0.515362, 0.706669, -1.045926, 0.383008);
        set_xaos(&mut xf, 0, 0.25 + r.random() * 0.1);
        set_xaos(&mut xf, 1, 1.15 + r.random() * 0.2);
        add(&mut xf, 1.0, "linear");
        let (c1, c2) = (0.05 + r.random() * 0.1, 0.86 + r.random() * 0.1);
        set(add(&mut xf, 1.0, "curl"), &[("c1", c1), ("c2", c2)]);
        local_translate(&mut xf, 0.5 - r.random(), 0.5 - r.random(), false);
        if r.random() > 0.5 {
            local_translate(&mut xf, 0.5 - r.random(), 0.5 - r.random(), false);
        }
        if r.random() > 0.666 {
            rotate(&mut xf, -12.0 + 24.0 * r.random(), false);
        }
        layer.xforms.push(xf);
    }
    randomize_colors(layer, r);
    flame
}

/// `RasterRandomFlameGenerator`.
pub fn raster(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.centre_x = 2.45;
    flame.centre_y = 1.37;
    flame.cam_zoom = 1.25 + r.random() * 0.5;
    let layer = flame.first_layer_mut();
    let random_vars = |xf: &mut crate::model::XForm, r: &mut crate::rng::Rng, extra: [(f64, f64, f64); 3]| {
        for (i, (chance, base, span)) in extra.into_iter().enumerate() {
            if i == 0 || r.random() < chance {
                let name = java::random_variation(r);
                add(xf, base + r.random() * span, name);
            }
        }
    };
    {
        let mut xf = xform();
        xf.weight = 0.05;
        xf.color = 0.0;
        xf.color_symmetry = 0.89103422;
        xf.post = affine(0.45811152, 0.0, 0.0, 0.45811152, 0.0, 0.0);
        random_vars(&mut xf, r, [(1.0, 0.5, 0.65), (0.20, 0.4, 0.5), (0.10, 0.3, 0.4)]);
        if r.random() < 0.5 {
            rotate(&mut xf, 360.0 * r.random(), false);
        }
        scale(&mut xf, 1.15 - r.random() * 0.65, true, true, true);
        rotate(&mut xf, 360.0 * r.random(), true);
        local_translate(&mut xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), true);
        layer.xforms.push(xf);
    }
    for (color, symmetry, (px, py)) in [(0.33333333, 0.62668861, (1.0, 0.0)), (0.66666667, 0.33786964, (0.0, 1.0))] {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.color = color;
        xf.color_symmetry = symmetry;
        xf.post = affine(1.0, 0.0, 0.0, 1.0, px, py);
        add(&mut xf, 1.0, "linear3D");
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.05;
        xf.color = 1.0;
        xf.color_symmetry = 0.95291095;
        xf.post = affine(0.0423868, -0.14299816, 0.14299816, 0.0423868, 0.5, 0.5);
        random_vars(&mut xf, r, [(1.0, 0.4, 0.65), (0.10, 0.4, 0.5), (0.05, 0.3, 0.4)]);
        if r.random() < 0.25 {
            rotate(&mut xf, 360.0 * r.random(), false);
        }
        scale(&mut xf, 1.15 - r.random() * 0.75, true, true, true);
        rotate(&mut xf, 360.0 * r.random(), true);
        local_translate(&mut xf, 0.25 - 0.5 * r.random(), 0.25 - 0.5 * r.random(), true);
        layer.xforms.push(xf);
    }
    randomize_colors(layer, r);
    flame
}

/// `RaysRandomFlameGenerator`.
pub fn rays(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.cam_roll = r.random() * 360.0;
    flame.cam_zoom = 1.0 + r.random() * 0.75;
    flame.gamma = 1.2 + r.random() * 0.8;
    flame.gamma_threshold = r.random() * 0.15;
    flame.brightness = 2.5 + r.random() * 1.5;
    let layer = flame.first_layer_mut();
    let place = |xf: &mut crate::model::XForm, r: &mut crate::rng::Rng| {
        local_translate(xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), false);
        scale(xf, 0.9 + r.random() * 0.5, true, true, false);
        rotate(xf, 360.0 * r.random(), false);
    };
    {
        let mut xf = xform();
        xf.weight = 0.1 + r.random() * 0.7;
        xf.coefs = affine(-0.43686351, -0.59864436, 0.63649767, -0.50111868, 0.7325555, 0.08008955);
        add(&mut xf, 0.125 + r.random() * 0.125, "linear");
        set(add(&mut xf, 0.6 + r.random() * 0.4, "julian"), &[("power", 3.0), ("dist", 1.0)]);
        set(add(&mut xf, 0.6 + r.random() * 0.3, "juliascope"), &[("power", 6.0), ("dist", 1.0)]);
        place(&mut xf, r);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.23272111;
        xf.color = 0.8;
        xf.coefs = affine(-0.15514012, -0.15087636, 0.20816567, 0.20174952, 0.19004075, -0.04671182);
        add(&mut xf, 1.0, "linear");
        set(add(&mut xf, 0.25 + r.random() * 0.5, "juliascope"), &[("power", 2.0), ("dist", 1.0)]);
        let name = if r.random() < 0.33 {
            let exp = super::gens_a::experimental();
            exp[rand_int(r, exp.len())]
        } else {
            java::random_variation(r)
        };
        add(&mut xf, 0.18 - r.random() * 0.36, name);
        place(&mut xf, r);
        layer.xforms.push(xf);
    }
    let small = [
        (0.1, 0.75, affine(0.92931929, -0.02999303, 0.17076568, 0.67104838, 0.15869232, -0.19510192)),
        (0.15, 0.75, affine(0.53699507, 0.23469299, 0.12206942, 0.79862481, -0.0380713, 0.04291273)),
        (0.2, 0.0, affine(0.89779523, 0.1734196, -0.23112747, 0.94619577, -0.4497571, -0.0672393)),
        (0.25, 0.0, affine(0.70624241, -0.04953437, 0.15652237, 0.78838922, 0.43007701, 0.21371425)),
    ];
    for (color, symmetry, coefs) in small {
        let mut xf = xform();
        xf.weight = 0.04166667;
        xf.color = color;
        xf.color_symmetry = symmetry;
        xf.coefs = coefs;
        let name = if r.random() > 0.5 { "linear" } else { java::random_variation_no_fract(r, false) };
        add(&mut xf, 1.0, name);
        layer.xforms.push(xf);
    }
    if r.random() > 0.33 {
        let mut xf = xform();
        let finals = ["falloff2", "bwrands", "bwraps7", "cosine", "falloff3", "bwrands", "bwrands", "falloff2"];
        let name = finals[(r.random() * finals.len() as f64) as usize];
        let v = add(&mut xf, 1.0, name);
        if name == "bwrands" {
            let seed = rand_int(r, i16::MAX as usize) as f64;
            let minp = ((3.0 + r.random() * 3.0) as i32) as f64;
            let maxp = ((6.0 + r.random() * 12.0) as i32) as f64;
            set(v, &[("seed", seed), ("minpetals", minp), ("maxpetals", maxp)]);
        }
        if name == "bwrands" || name == "bwraps7" {
            let (cell, space) = (1.0 - r.random() * 0.9, r.random() * 0.2);
            set(v, &[("cellsize", cell), ("space", space)]);
            if r.random() > 0.5 {
                let (inner, outer) = (0.2 - r.random() * 0.4, 2.0 - r.random() * 4.0);
                set(v, &[("inner_twist", inner), ("outer_twist", outer)]);
            }
        }
        layer.final_xforms.push(xf);
    }
    flame
}

/// `SplitsRandomFlameGenerator`.
pub fn splits(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.cam_zoom = 2.0;
    flame.gamma = 2.7;
    let layer = flame.first_layer_mut();
    static TX1: OnceLock<Vec<&'static str>> = OnceLock::new();
    let tx1 = TX1.get_or_init(|| {
        java::filter(&[
            "bipolar",
            "boarders",
            "boarders2",
            "bubble",
            "butterfly",
            "bwraps7",
            "cannabiscurve_wf",
            "circlize",
            "cloverleaf_wf",
            "collideoscope",
            "cos",
            "cosh",
            "cot",
            "coth",
            "csc",
            "csch",
            "cubic3D",
            "cubicLattice_3D",
            "dc_perlin",
            "diamond",
            "disc2",
            "eJulia",
            "edisc",
            "elliptic",
            "epispiral",
            "epispiral_wf",
            "ex",
            "exp",
            "exponential",
            "eyefish",
            "flower",
            "foci",
            "xtrb",
            "glynnSim1",
            "glynnia",
            "heart",
            "hexnix3D",
            "julia",
            "julian",
            "julia3D",
            "julia3Dz",
            "juliascope",
            "layered_spiral",
            "mobius",
            "ngon",
            "phoenix_julia",
            "pie",
            "popcorn2_3D",
            "radial_blur",
            "rays",
            "ripple",
            "spherical",
            "sphericalN",
            "spiral",
            "tan",
            "tanh",
            "waves2_3D",
            "wedge_sph",
            "whorl",
            "xheart",
        ])
    });
    let quarter = |r: &mut crate::rng::Rng| if r.random() < 0.5 { 90.0 } else { -90.0 };
    {
        let mut xf = xform();
        xf.weight = 0.5 + 0.4 * r.random();
        let name = tx1[rand_int(r, tx1.len())];
        add(&mut xf, 1.0, name);
        let a = quarter(r);
        rotate(&mut xf, a, false);
        xf.color = 0.60 + r.random() * 0.20;
        xf.color_symmetry = r.random() * 0.2;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.5;
        let (x, y) = if r.random() < 0.25 { (-0.5 + r.random(), -0.5 + r.random()) } else { (-1.0 + r.random() * 4.0, 0.0) };
        set(add(&mut xf, 1.0, "splits"), &[("x", x), ("y", y)]);
        scale(&mut xf, 0.6 + r.random() * 0.8, true, true, false);
        scale(&mut xf, 1.0 + r.random() * 0.8, true, true, true);
        xf.color = 0.40 + r.random() * 0.20;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.2 + 0.4 * r.random();
        let exp = super::gens_a::experimental();
        let name = exp[rand_int(r, exp.len())];
        add(&mut xf, 1.0, name);
        let a = quarter(r);
        rotate(&mut xf, a, false);
        xf.color = 0.60 + r.random() * 0.20;
        xf.color_symmetry = r.random() * 0.2;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.3 + 0.8 * r.random();
        let name = if r.random() < 0.5 { "noise" } else { "cylinder" };
        add(&mut xf, 1.0, name);
        scale(&mut xf, 5.0 + r.random() * 10.0, false, true, true);
        scale(&mut xf, 0.5 + r.random() * 0.75, true, false, true);
        scale(&mut xf, 0.2 + r.random() * 0.8, true, true, false);
        xf.color = 0.60 + r.random() * 0.20;
        xf.color_symmetry = r.random() * 0.2;
        layer.xforms.push(xf);
    }
    flame
}

/// `TentacleRandomFlameGenerator`.
pub fn tentacle(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0 + r.random() * 100.0);
    let layer = flame.first_layer_mut();
    let nx = (2.0 + r.random() * 3.0) as i32;
    let ny = (2.0 + r.random() * 3.0) as i32;
    let (x_min, y_min) = (-(nx as f64) * 0.5, -(ny as f64) * 0.5 + 1.0);
    let fnc = super::gens_a::experimental();
    let mut scl = 1.0;
    for y in 0..ny {
        for x in 0..nx {
            let mut xf = xform();
            xf.weight = 0.5 + r.random() * 99.5;
            java::global_translate(&mut xf, x_min + x as f64, y_min + y as f64, false);
            if r.random() < 0.5 {
                rotate(&mut xf, 360.0 * r.random(), true);
            } else {
                rotate(&mut xf, -360.0 * r.random(), true);
            }
            local_translate(&mut xf, -1.0 + 2.0 * r.random(), -1.0 + 2.0 * r.random(), true);
            scl *= 0.75 + r.random() / 4.0;
            scale(&mut xf, scl, true, true, true);
            let idx = ((r.random() * fnc.len() as f64) as usize).min(fnc.len() - 1);
            let amount = r.random() * 0.9 + 0.1;
            let name = if r.random() > 0.25 { fnc[idx] } else { "linear3D" };
            add(&mut xf, amount, name);
            xf.color = r.random();
            layer.xforms.push(xf);
        }
    }
    flame
}

/// `TileBallRandomFlameGenerator` ("Tile Ball").
pub fn tile_ball(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.centre_x = 0.1 - r.random() * 0.2;
    flame.centre_y = -0.6 + r.random() * 0.3;
    flame.cam_zoom = 1.0 + r.random() * 0.5;
    flame.cam_pitch = 25.0 + r.random() * 40.0;
    flame.cam_perspective = 0.1 + r.random() * 0.2;
    flame.preserve_z = true;
    let layer = flame.first_layer_mut();
    let a = 0.5;
    let tiles = [(0.0, 0.0), (0.0, a), (a, a), (a, 0.0), (a, -a), (0.0, -a), (-a, -a), (-a, 0.0), (-a, a)];
    for (i, (tx, ty)) in tiles.into_iter().enumerate() {
        let mut xf = xform();
        xf.weight = 0.5;
        add(&mut xf, 1.0, "linear3D");
        if i == 0 {
            add(&mut xf, 1.5 + r.random() * 1.5, "ztranslate");
            xf.color = r.random();
        }
        xf.color_symmetry = 1.0;
        local_translate(&mut xf, tx, ty, false);
        set_xaos(&mut xf, 10, 0.0);
        layer.xforms.push(xf);
    }
    // `randomizeParams`: a random number of parameter steps.
    let randomize = |v: &mut crate::model::VariationInstance, r: &mut crate::rng::Rng| {
        let n = crate::variations::find(&v.name).map_or(0, |d| d.params.len());
        let steps = (r.random() * n as f64).round() as usize;
        for _ in 0..steps {
            let amount = 0.1 + r.random();
            mutations::mutate_variation(v, amount, r);
        }
    };
    for (amount, last) in [((0.2, 0.6), true), ((0.1, 0.3), false)] {
        let mut xf = xform();
        xf.weight = 0.25 + r.random() * 1.25;
        let name = java::random_variation(r);
        let v = add(&mut xf, amount.0 + r.random() * amount.1, name);
        randomize(v, r);
        add(&mut xf, 1.0, "flatten");
        xf.color = r.random();
        xf.color_symmetry = r.random();
        if last {
            for j in 0..10 {
                set_xaos(&mut xf, j, 0.0);
            }
            set_xaos(&mut xf, 10, 1.0);
        } else {
            set_xaos(&mut xf, 10, 0.0);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.8 + r.random() * 0.5;
        let (cx, cy, cz) = (-0.75 + r.random() * 1.5, 0.8 + r.random() * 0.4, 0.025 + r.random() * 0.05);
        set(add(&mut xf, 1.0, "curl3D"), &[("cx", cx), ("cy", cy), ("cz", cz)]);
        layer.final_xforms.push(xf);
    }
    flame
}

/// `SimpleTilingRandomFlameGenerator` ("Simple tiling").
pub fn simple_tiling(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 0.4 + r.random() * 50.0;
        let amount = 0.01 + 0.4 * r.random();
        let name = java::random_variation(r);
        add(&mut xf, amount, name);
        add(&mut xf, 0.02 + 0.2 * r.random(), "linear3D");
        xf.color = 0.87 + r.random() * 0.1;
        layer.xforms.push(xf);
    }
    let two_primary = r.random() > 0.33;
    let linked = r.random() < 0.4;
    if two_primary {
        let mut xf = xform();
        xf.weight = 0.4 + r.random() * 72.0;
        let name = java::random_variation(r);
        add(&mut xf, 1.0, name);
        xf.color_symmetry = -1.0;
        xf.color = 0.89 + r.random() * 0.06;
        let s = 0.75 + r.random() * 0.25;
        let (sx, sy) = (r.random() > 0.125, r.random() < 0.875);
        scale(&mut xf, s, sx, sy, false);
        rotate(&mut xf, -180.0 + r.random() * 360.0, false);
        layer.xforms.push(xf);
    }
    let n_forms = 2 + rand_int(r, 5);
    for i in 0..n_forms {
        let mut xf = xform();
        xf.weight = 0.25 + r.random() * 0.5;
        add(&mut xf, 0.5 + r.random() * 0.5, "linear3D");
        let s = if r.random() > 0.75 { 0.125 + r.random() * 0.5 } else { 0.75 + r.random() * 0.25 };
        let (sx, sy) = (r.random() > 0.5, r.random() < 0.5);
        scale(&mut xf, s, sx, sy, false);
        rotate(&mut xf, -45.0 + r.random() * 90.0, false);
        let off = -2.0 + i as f64 * 0.5;
        local_translate(&mut xf, off + r.random() * 4.0, off + r.random() * 4.0, false);
        xf.color = r.random();
        layer.xforms.push(xf);
    }
    let n = layer.xforms.len();
    for i in 0..n {
        let (others, to_second) = if linked {
            if i == 0 { (0.0, 1.0) } else { (1.0, 0.0) }
        } else {
            let mut wg = r.random() * 12.0;
            if r.random() < 0.25 {
                wg = 0.0;
            }
            if i == 0 { (wg, 1.0) } else { (1.0, wg) }
        };
        for j in 0..n {
            set_xaos(&mut layer.xforms[i], j, if j != 1 { others } else { to_second });
        }
    }
    flame
}

/// `UnderwaterRandomFlameGenerator`.
pub fn underwater(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = java::flame();
    flame.cam_roll = -90.0 + r.random() * 180.0;
    flame.width = 1920;
    flame.height = 1080;
    flame.pixels_per_unit = 313.97436855;
    flame.bg_transparency = false;
    flame.preserve_z = false;
    let typed = |r: &mut crate::rng::Rng, p_2d: f64| {
        let three_d = r.random() >= p_2d;
        java::random_variation_of(r, three_d)
    };
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 0.32019788;
        xf.color = 0.075 + r.random() * 0.15;
        xf.color_symmetry = 0.3 + r.random() * 0.15;
        xf.coefs = affine(0.75092904, -0.56527515, 0.56527515, 0.75092904, 0.3204811, -0.98334041);
        if r.random() > 0.333 {
            add(&mut xf, 0.41432335, "linear3D");
        } else {
            let amount = 0.25 + r.random() * 0.5;
            let name = typed(r, 0.65);
            add(&mut xf, amount, name);
        }
        if r.random() > 0.5 {
            set(add(&mut xf, 0.22103179, "eclipse"), &[("shift", 0.0)]);
        } else {
            let amount = 0.15 + r.random() * 0.15;
            let name = typed(r, 0.5);
            add(&mut xf, amount, name);
        }
        if r.random() < 0.5 {
            rotate(&mut xf, -30.0 + 60.0 * r.random(), false);
        } else {
            local_translate(&mut xf, -0.05 + 0.1 * r.random(), -0.05 + 0.1 * r.random(), false);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = if r.random() < 0.5 { 0.64084507 } else { 0.5 + r.random() * 0.3 };
        xf.color = 0.8 + r.random() * 0.1;
        xf.color_symmetry = 0.87685786;
        xf.coefs = affine(-0.36319594, 0.64089754, -0.64089754, -0.36319594, -0.3641259, 0.40615455);
        if r.random() > 0.42 {
            add(&mut xf, 0.21369106, "linear3D");
        } else {
            let amount = 0.125 + r.random() * 0.25;
            let name = typed(r, 0.65);
            add(&mut xf, amount, name);
        }
        add(&mut xf, 0.61264366, "foci");
        if r.random() > 0.5 {
            let amount = 0.05 + r.random() * 0.15;
            let name = typed(r, 0.5);
            add(&mut xf, amount, name);
        }
        if r.random() < 0.5 {
            rotate(&mut xf, -30.0 + 60.0 * r.random(), false);
        } else {
            local_translate(&mut xf, -0.15 + 0.3 * r.random(), -0.15 + 0.3 * r.random(), false);
        }
        layer.xforms.push(xf);
    }
    if r.random() > 0.72 {
        let mut xf = xform();
        xf.color = 0.5;
        if r.random() > 0.666 {
            add(&mut xf, 1.0, "log");
        } else {
            let three_d = r.random() <= 0.25;
            let name = java::random_variation_of(r, three_d);
            add(&mut xf, 1.0, name);
        }
        scale(&mut xf, 1.25 - r.random() * 0.5, true, true, false);
        rotate(&mut xf, 360.0 * r.random(), false);
        local_translate(&mut xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), false);
        layer.final_xforms.push(xf);
    }
    let rgb = if r.random() < 0.125 {
        [0, 102, 102]
    } else if r.random() < 0.5 {
        let g = (50.0 + 102.0 * r.random()) as u8;
        [0, g, g]
    } else {
        let g = (50.0 + 102.0 * r.random()) as u8;
        let b = (50.0 + 102.0 * r.random()) as u8;
        [0, g, b]
    };
    flame.background.kind = crate::model::BgColorType::SingleColor;
    flame.background.color = rgb;
    flame
}

/// `XenomorphRandomFlameGenerator`.
pub fn xenomorph(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 0.1 + r.random() * 0.4;
        xf.color = 0.74488914;
        xf.coefs = affine(0.75610133, 0.74186252, -0.74186252, 0.75610133, 5.25778565, -0.34949139);
        xf.post = affine(-0.42606416, -0.10610689, 0.44290131, -0.40885976, -2.81712, 7.390367);
        add(&mut xf, 1.43, "bubble");
        add(&mut xf, 0.012, "linear");
        if r.random() > 0.6 {
            set(add(&mut xf, -0.249, "radial_blur"), &[("angle", 0.609835)]);
        } else {
            let amount = if r.random() < 0.5 { -0.249 } else { 0.5 - r.random() };
            let name = java::random_variation(r);
            add(&mut xf, amount, name);
        }
        add(&mut xf, 1.0, "power");
        local_translate(&mut xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), false);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 10.0 + r.random() * 8.0;
        xf.color = 0.90312262;
        xf.color_symmetry = 0.95;
        xf.coefs = affine(-0.85421287, -0.63343313, -0.63343313, 0.85421287, 1.09379129, -0.20406326);
        xf.post = affine(0.84389756, 0.43174917, -0.35800434, 0.89637273, -0.945758, -0.4502584);
        for (i, w) in [2.05, 1.25, 0.9].into_iter().enumerate() {
            set_xaos(&mut xf, i, w);
        }
        add(&mut xf, 0.008, "linear");
        add(&mut xf, 10.72, "spherical");
        scale(&mut xf, 1.25 - r.random() * 0.5, true, true, false);
        rotate(&mut xf, 36.0 * r.random(), false);
        local_translate(&mut xf, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), false);
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.7;
        xf.color = 0.47272985;
        xf.coefs = affine(0.68724662, 0.72642418, -0.72642418, 0.68724662, -2.14812602, 2.39994214);
        xf.post = affine(0.60646395, -0.79511098, 0.79511098, 0.60646395, -1.06135064, -0.6369509);
        set(add(&mut xf, 1.0, "bipolar"), &[("shift", 0.0)]);
        layer.xforms.push(xf);
    }
    flame
}

/// `Flowers3DRandomFlameGenerator` ("Flowers3D (stunning)").
pub fn flowers3d(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.cam_pitch = 49.0;
    flame.cam_yaw = 12.0;
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 0.5 + r.random();
        add(&mut xf, 0.5, "gaussian_blur");
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 3.0 + r.random() * 10.0;
        if r.random() < 0.33 {
            add(&mut xf, 1.0, "linear3D");
        } else {
            let x = 2.0 * r.random() - 0.5;
            let z = 2.0 * r.random() - 0.5;
            set(add(&mut xf, 1.0, "linearT3D"), &[("powX", x), ("powY", x), ("powZ", z)]);
        }
        add(&mut xf, 0.1 + r.random() * 0.3, "spherical");
        add(&mut xf, 0.2 + r.random() * 0.9, "zcone");
        add(&mut xf, 0.01 + 0.045 * r.random(), "cross");
        if r.random() < 0.33 {
            let waves = (3 + rand_int(r, 10)) as f64;
            let amount = 0.02 + 0.29 * r.random();
            set(add(&mut xf, amount, "epispiral_wf"), &[("waves", waves)]);
            if r.random() < 0.33 {
                let (thickness, n) = (0.05 + r.random() * 0.15, 3.0 + r.random() * 10.0);
                let amount = 0.01 + 0.14 * r.random();
                set(add(&mut xf, amount, "epispiral"), &[("thickness", thickness), ("n", n)]);
            }
        }
        xf.color = r.random();
        xf.color_symmetry = r.random();
        scale(&mut xf, 1.0 + (0.1 - r.random() * 0.2), true, true, false);
        rotate(&mut xf, 45.0 - r.random() * 90.0, false);
        local_translate(&mut xf, 0.01 - 0.02 * r.random(), 0.01 - 0.02 * r.random(), false);
        layer.xforms.push(xf);
    }
    let adv = r.random() > 0.25;
    if adv {
        let mut xf = xform();
        xf.weight = 0.3 + r.random() * 0.3;
        set(add(&mut xf, 0.05, "blob3D"), &[("low", 0.1), ("high", 0.3), ("waves", 9.0)]);
        xf.color_symmetry = 1.0;
        layer.xforms.push(xf);
    }
    if adv && r.random() > 0.25 {
        let mut xf = xform();
        xf.weight = 0.1 + r.random() * 0.3;
        add(&mut xf, 0.5, "blur3D");
        add(&mut xf, 15.0, "ztranslate");
        xf.color_symmetry = 1.0;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        let mut power = -2.0;
        if r.random() < 0.25 {
            power -= r.random() * 4.0;
        }
        let amount = 2.0 + (1.0 - 2.0 * r.random());
        set(add(&mut xf, amount, "julia3D"), &[("power", power)]);
        layer.final_xforms.push(xf);
    }
    flame
}

/// `FilledFlowers3DRandomFlameGenerator` ("Flowers3D (filled)").
pub fn filled_flowers3d(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = base(200.0);
    flame.cam_pitch = 37.0;
    flame.cam_zoom = 2.0;
    flame.cam_perspective = 0.32;
    let layer = flame.first_layer_mut();
    let from = |r: &mut crate::rng::Rng, names: &[&'static str]| {
        let list = java::filter(names);
        list[rand_int(r, list.len())]
    };
    {
        let mut xf = xform();
        xf.weight = 0.25 + r.random() * 0.5;
        add(&mut xf, 0.25 + r.random() * 0.5, "gaussian_blur");
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 1.0 + r.random() * 50.0;
        local_translate(&mut xf, 0.3 - 0.6 * r.random(), 0.3 - 0.6 * r.random(), false);
        rotate(&mut xf, 90.0 + r.random() * 180.0, false);
        scale(&mut xf, 1.25 + r.random() * 1.25, true, true, false);
        add(&mut xf, 1.0, "linear3D");
        add(&mut xf, 0.01 - r.random() * 0.02, "epispiral_wf");
        add(&mut xf, 0.1 + r.random() * 0.1, "ztranslate");
        add(&mut xf, 0.00001 - r.random() * 0.00002, "zcone");
        let amount = 0.001 + r.random() * 0.099;
        let name = from(r, &["bubble", "log"]);
        add(&mut xf, amount, name);
        let amount = 0.001 + r.random() * 0.099;
        let name = from(
            r,
            &[
                "arch",
                "bipolar",
                "hyperbolic",
                "butterfly",
                "cannabiscurve_wf",
                "cell",
                "checks",
                "circlize",
                "conic",
                "coth",
                "cpow",
                "ex",
                "falloff2",
                "fan",
                "flux",
                "foci",
                "heart",
                "kaleidoscope",
                "log",
                "mobius",
                "ngon",
                "pdj",
                "oscilloscope",
                "spherical",
                "spiral",
            ],
        );
        add(&mut xf, amount, name);
        {
            let name = from(r, &["waves2", "waves2_wf", "waves3_wf", "waves4_wf"]);
            let (sx, sy, fx, fy) = (0.5 + r.random(), 0.5 + r.random(), 1.0 + r.random() * 2.0, 1.0 + r.random() * 2.0);
            let amount = 0.05 - r.random() * 0.1;
            set(add(&mut xf, amount, name), &[("scalex", sx), ("scaley", sy), ("freqx", fx), ("freqy", fy)]);
        }
        let amount = 0.001 + r.random() * 0.015;
        let name = from(r, &["cross", "checks", "conic", "kaleidoscope", "lazysusan", "log"]);
        add(&mut xf, amount, name);
        xf.color = 0.33;
        layer.xforms.push(xf);
    }
    {
        let blur_count = rand_int(r, 5);
        let mut weight = 0.25 + r.random() * 0.5;
        for _ in 0..blur_count {
            let mut xf = xform();
            xf.weight = weight;
            weight *= 0.75;
            add(&mut xf, 0.25 + r.random() * 0.5, "gaussian_blur");
            local_translate(&mut xf, 0.3 - 0.6 * r.random(), 0.3 - 0.6 * r.random(), true);
            layer.xforms.push(xf);
        }
    }
    {
        const LIST: &[&str] = &[
            "blade",
            "blur",
            "blur3D",
            "bubble_wf",
            "cannabiscurve_wf",
            "circlecrop",
            "cloverleaf_wf",
            "conic",
            "crop",
            "cross",
            "flower",
            "flux",
            "hemisphere",
            "hyperbolic",
            "julia3D",
            "julia3Dz",
            "lazysusan",
            "lissajous",
            "log",
            "mandelbrot",
            "mobius",
            "npolar",
            "pdj",
            "perspective",
            "pie",
            "pie3D",
            "polar",
            "polar2",
            "power",
            "pre_subflame_wf",
            "radial_blur",
            "scry",
            "separation",
            "spiral",
            "spirograph",
            "split",
            "tangent",
            "tangent3D",
            "twintrian",
            "unpolar",
            "wedge_sph",
            "zblur",
        ];
        let mut xf = xform();
        xf.weight = 0.1 + r.random() * 1.4;
        local_translate(&mut xf, 0.3 - 0.6 * r.random(), 0.3 - 0.6 * r.random(), true);
        match rand_int(r, 3) {
            0 => {
                add(&mut xf, 0.05 + r.random() * 0.15, "bubble");
                let amount = 0.01 + r.random() * 0.5;
                let name = from(r, LIST);
                add(&mut xf, amount, name);
            }
            1 => {
                add(&mut xf, 0.05 + r.random() * 0.15, "bubble");
            }
            _ => {
                let amount = 0.05 + r.random() * 0.15;
                let name = from(r, LIST);
                add(&mut xf, amount, name);
            }
        }
        add(&mut xf, 0.05 + r.random() * 0.15, "bubble");
        xf.color_symmetry = 0.8 + r.random() * 0.2;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        add(&mut xf, 0.05 + r.random() * 0.2, "zscale");
        let power = if r.random() < 0.5 { -3.0 } else { -4.0 };
        set(add(&mut xf, 1.0, "julia3D"), &[("power", power)]);
        let zero = if r.random() < 0.5 { 1.0 } else { 0.0 };
        set(add(&mut xf, 1.0, "pre_circlecrop"), &[("radius", 10000.0), ("zero", zero)]);
        layer.final_xforms.push(xf);
    }
    flame
}
