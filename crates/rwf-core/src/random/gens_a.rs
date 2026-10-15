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

//! Ported generators: simple, linear, affine 3D, Sierpinsky, spherical and bubbles
//! (JWildfire `*RandomFlameGenerator`).

use std::sync::OnceLock;

use super::State;
use super::java::{
    self, add, distribute_colors, global_translate, local_translate, pick, rand_int, randomize_colors, rotate, scale, set, set_xaos, xform,
};
use crate::model::Flame;

/// `ExperimentalSimpleRandomFlameGenerator.FNCLST_EXPERIMENTAL` (valid random variations).
pub fn experimental() -> &'static [&'static str] {
    static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
    LIST.get_or_init(|| {
        const NAMES: &[&str] = &[
            "blur3D",
            "bubble",
            "escher",
            "rays",
            "epispiral_wf",
            "curl3D",
            "diamond",
            "juliaq",
            "julia3Dq",
            "post_juliaq",
            "post_julia3Dq",
            "cloverleaf_wf",
            "disc",
            "sech",
            "loonie",
            "exp",
            "cosh",
            "split",
            "waves2_3D",
            "wedge_sph",
            "circlize",
            "heart_wf",
            "bwraps7",
            "colorscale_wf",
            "gdoffs",
            "taurus",
            "dc_crackle_wf",
            "mandelbrot",
            "spirograph",
            "target",
            "eclipse",
            "butterfly3D",
            "cpow",
            "pre_subflame_wf",
            "conic",
            "julia3D",
            "cell",
            "dc_hexes_wf",
            "stripes",
            "post_mirror_wf",
            "flipcircle",
            "waves2_3D",
            "juliac",
            "colorscale_wf",
            "crackle",
            "truchet",
            "cannabiscurve_wf",
            "cpow",
            "subflame_wf",
            "post_smartcrop",
            "glynnSim3",
            "flower",
            "fourth",
            "heart",
            "julia3D",
            "disc2",
            "polar2",
            "farblur",
            "waves3_wf",
            "waves2b",
            "foci",
            "scry",
            "flux",
            "bwraps7",
            "splitbrdr",
            "checks",
            "colorscale_wf",
            "falloff2",
            "sinusoidal3d",
            "cloverleaf_wf",
            "lazyTravis",
            "kaleidoscope",
            "eclipse",
            "hemisphere",
            "flipy",
            "phoenix_julia",
            "popcorn2",
            "sec",
            "lazysusan",
            "sin",
            "separation",
            "bi_linear",
            "hexnix3D",
            "popcorn2_3D",
            "julian3Dx",
            "post_mirror_wf",
            "heart_wf",
            "mcarpet",
            "mandelbrot",
            "cannabiscurve_wf",
            "colormap_wf",
            "juliac",
            "rose_wf",
            "edisc",
            "blocky",
            "octagon",
            "murl",
            "waves2",
            "twintrian",
            "coth",
            "super_shape",
            "post_colormap_wf",
            "waves2_3D",
            "auger",
            "pre_wave3D_wf",
            "hexes",
            "dc_hexes_wf",
            "barycentroid",
            "spirograph",
            "truchet",
            "epispiral",
            "waves4_wf",
            "glynnSim2",
            "tanh",
            "bipolar",
            "cot",
            "horseshoe",
            "target",
            "wedge",
            "unpolar",
            "pre_boarders2",
            "modulus",
            "mobius",
            "bubble2",
            "bwraps7",
            "colorscale_wf",
            "truchet",
            "collideoscope",
            "xheart",
            "waves2b",
            "kaleidoscope",
            "glynnSim2",
            "twoface",
            "cross",
            "tangent3D",
            "csc",
            "curve",
            "boarders2",
            "julian3Dx",
            "csch",
            "bent2",
            "splits",
            "julian3Dx",
            "whorl",
            "xtrb",
            "post_mirror_wf",
            "mandelbrot",
            "sphericalN",
            "waves2_3D",
            "cloverleaf_wf",
            "cannabiscurve_wf",
            "tan",
            "blob3D",
            "julia3D",
            "hypertile1",
            "svf",
            "dc_crackle_wf",
            "log",
            "cos",
            "oscilloscope",
            "wedge_julia",
            "bwraps7",
            "heart_wf",
            "linearT3D",
            "juliac",
            "hexes",
            "truchet",
            "spirograph",
            "glynnSim3",
            "pdj",
            "popcorn",
            "hypertile2",
            "waves2_3D",
            "parabola",
            "rings2",
            "spherical3D",
            "spiral",
            "rectangles",
            "foci_3D",
            "sintrange",
            "waves2b",
            "elliptic",
            "waves",
            "swirl",
            "glynnSim1",
            "eclipse",
            "bwraps7",
            "layered_spiral",
            "heart_wf",
            "colorscale_wf",
            "boarders",
            "secant2",
            "waffle",
            "lissajous",
            "hypertile",
            "circus",
            "lazyTravis",
            "ovoid3d",
            "circleblur",
            "sineblur",
            "starblur",
            "lace_js",
            "japanese_maple_leaf",
            "fdisc",
        ];
        // `VariationFuncList.isValidRandomVariation`
        java::filter(NAMES)
            .into_iter()
            .filter(|n| {
                !n.starts_with("pre_")
                    && !n.starts_with("post_")
                    && !n.starts_with("prepost_")
                    && !n.contains("colorscale")
                    && !matches!(*n, "flatten" | "svg_wf" | "iflames_wf")
            })
            .collect()
    })
}

fn empty_flame() -> Flame {
    let mut flame = java::flame();
    flame.centre_x = 0.0;
    flame.centre_y = 0.0;
    flame.pixels_per_unit = 200.0;
    flame
}

/// `DuckiesRandomFlameGenerator`.
pub fn duckies(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = empty_flame();
    let layer = flame.first_layer_mut();
    {
        let mut xf = java::xform();
        xf.weight = 0.5 + r.random();
        let name = if r.random() < 0.12 { java::random_variation(r) } else { "spherical" };
        java::add(&mut xf, 1.5 + r.random(), name);
        xf.color_symmetry = -0.5;
        java::local_translate(&mut xf, 0.75 - 5.50 * r.random(), 0.75 - 1.50 * r.random(), false);
        java::rotate(&mut xf, -60.0 + r.random() * 30.0, false);
        java::scale(&mut xf, 0.1 + r.random() * 0.4, true, true, false);
        layer.xforms.push(xf);
    }
    {
        let mut xf = java::xform();
        xf.weight = 1.0 + r.random() * 100.0;
        let name = if r.random() < 0.8 { "juliascope" } else { "julian" };
        if r.random() > 0.75 {
            let extra = java::random_variation(r);
            java::add(&mut xf, 0.125 + 0.25 * r.random(), extra);
        }
        let power = if r.random() < 0.8 { 2.0 } else { 2.0 + r.random() * 10.0 };
        let dist = if r.random() < 0.8 { 1.0 } else { -2.0 + 4.0 * r.random() };
        let amount = 0.5 + r.random();
        java::set(java::add(&mut xf, amount, name), &[("power", power), ("dist", dist)]);
        xf.color_symmetry = 0.5 + r.random();
        java::rotate(&mut xf, r.random() * 360.0, false);
        java::local_translate(&mut xf, 1.75 - 3.50 * r.random(), 0.75 - 5.50 * r.random(), false);
        java::scale(&mut xf, 1.1 + r.random() * 2.0, true, true, false);
        layer.xforms.push(xf);
    }
    java::set_xaos(&mut layer.xforms[0], 0, 0.0);
    java::set_xaos(&mut layer.xforms[0], 1, 1.0);
    if r.random() > 0.667 {
        let mut xf = java::xform();
        xf.weight = 1.0 + r.random() * 100.0;
        let name = java::random_variation(r);
        java::add(&mut xf, 0.25 + 1.25 * r.random(), name);
        if r.random() > 0.5 {
            let extra = java::random_variation(r);
            java::add(&mut xf, 0.2 + 0.75 * r.random(), extra);
        }
        java::rotate(&mut xf, -12.0 + r.random() * 24.0, true);
        java::local_translate(&mut xf, -0.125 + r.random() * 0.25, -0.125 + r.random() * 0.25, true);
        java::scale(&mut xf, 0.9 + r.random() * 0.2, true, true, true);
        layer.xforms.push(xf);
        java::set_xaos(&mut layer.xforms[0], 1, 0.0);
        java::set_xaos(&mut layer.xforms[1], 2, 0.0);
        java::set_xaos(&mut layer.xforms[2], 2, 0.0);
        if r.random() > 0.667 {
            let mut xf = java::xform();
            xf.weight = 0.50 + r.random() * 50.0;
            let name = java::random_variation(r);
            java::add(&mut xf, 0.125 + 0.75 * r.random(), name);
            if r.random() > 0.5 {
                let extra = java::random_variation(r);
                java::add(&mut xf, 0.125 + 0.35 * r.random(), extra);
            }
            java::rotate(&mut xf, -24.0 + r.random() * 48.0, true);
            java::local_translate(&mut xf, -0.25 + r.random() * 0.5, -0.25 + r.random() * 0.5, true);
            java::scale(&mut xf, 0.5 + r.random() * 0.25, true, true, true);
            layer.xforms.push(xf);
            java::set_xaos(&mut layer.xforms[0], 2, 0.0);
            java::set_xaos(&mut layer.xforms[2], 2, 0.0);
            java::set_xaos(&mut layer.xforms[1], 3, 0.0);
            java::set_xaos(&mut layer.xforms[2], 3, 0.0);
            java::set_xaos(&mut layer.xforms[3], 3, 0.0);
        }
    }
    java::randomize_colors(layer, r);
    flame
}

/// `SimpleRandomFlameGenerator` ("Simple (stunning)").
pub fn simple(st: &mut State) -> Flame {
    static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
    let fnc = LIST.get_or_init(|| {
        java::filter(&[
            "blur3D",
            "bubble",
            "curl3D",
            "diamond",
            "waves2_3D",
            "disc",
            "julia3D",
            "heart",
            "julia3D",
            "hemisphere",
            "waffle",
            "bwraps7",
            "horseshoe",
            "boarders",
            "blob3D",
            "xtrb",
            "xheart",
            "julia3D",
            "pdj",
            "popcorn",
            "rings2",
            "rose_wf",
            "spherical3D",
            "spiral",
            "rectangles",
            "blur",
            "waves",
            "swirl",
            "secant2",
            "boarders2",
        ])
    });
    simple_like(st, 2.0, 0.8, 0.2, 0.33, fnc, (0.0, 0.5))
}

/// `ExperimentalSimpleRandomFlameGenerator` ("Simple (experimental)").
pub fn experimental_simple(st: &mut State) -> Flame {
    simple_like(st, 1.0, 0.3, 0.2, 0.1, experimental(), (0.2, 0.6))
}

/// Shared body of the two simple generators.
fn simple_like(st: &mut State, min_xforms: f64, lin_rnd: f64, lin_base: f64, no_var: f64, fnc: &[&str], amount: (f64, f64)) -> Flame {
    let r = &mut *st.rng;
    let mut flame = empty_flame();
    let layer = flame.first_layer_mut();
    let max_xforms = (min_xforms + r.random() * 5.0) as usize;
    let mut scl = 1.0;
    for _ in 0..max_xforms {
        let mut xf = xform();
        if r.random() < 0.5 {
            rotate(&mut xf, 360.0 * r.random(), false);
        } else {
            rotate(&mut xf, -360.0 * r.random(), false);
        }
        local_translate(&mut xf, r.random() - 1.0, r.random() - 1.0, false);
        scl *= 0.75 + r.random() / 4.0;
        scale(&mut xf, scl, true, true, false);
        xf.color = r.random();
        add(&mut xf, r.random() * lin_rnd + lin_base, "linear3D");
        if r.random() > no_var {
            let name = pick(r, fnc);
            add(&mut xf, amount.0 + r.random() * amount.1, name);
        }
        xf.weight = r.random() * 0.9 + 0.1;
        layer.xforms.push(xf);
    }
    flame
}

/// `LinearRandomFlameGenerator` ("Linear only").
pub fn linear(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = empty_flame();
    let layer = flame.first_layer_mut();
    let max_xforms = (1.0 + r.random() * 5.0) as usize;
    let (mut scl, mut tscl) = (1.0, 2.0);
    let cont_rot = r.random() < 0.5;
    let mut r0 = 0.0;
    for _ in 0..max_xforms {
        let mut xf = xform();
        if cont_rot {
            r0 += 45.0 * r.random() - 9.0 * r.random();
            rotate(&mut xf, r0, false);
        } else if r.random() < 0.5 {
            rotate(&mut xf, 360.0 * r.random(), false);
        } else {
            rotate(&mut xf, -360.0 * r.random(), false);
        }
        local_translate(&mut xf, (2.0 * r.random() - 1.0) * tscl, (2.0 * r.random() - 1.0) * tscl, false);
        scl *= 0.8 + r.random() * 0.1;
        tscl *= 0.8 + r.random() * 0.1;
        scale(&mut xf, scl, true, true, false);
        xf.color = r.random();
        add(&mut xf, r.random() * 0.5 + 0.5, "linear3D");
        xf.weight = scl * r.random() * 19.9 + 0.1;
        layer.xforms.push(xf);
    }
    flame
}

/// `Affine3DRandomFlameGenerator`.
pub fn affine3d(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = empty_flame();
    flame.cam_pitch = 49.0 - 100.0 * r.random();
    flame.cam_yaw = 22.0 - 44.0 * r.random();
    flame.cam_bank = 44.0 - 88.0 * r.random();
    flame.cam_perspective = 0.06 + 0.36 * r.random();
    let layer = flame.first_layer_mut();
    let max_xforms = (1.0 + r.random() * 5.0) as usize;
    let (mut scl, mut tscl) = (1.0, 2.0);
    let cont_rot = r.random() < 0.5;
    let with_shear = r.random() < 0.5;
    let mut r0 = 0.0;
    for _ in 0..max_xforms {
        let mut xf = xform();
        let amount = r.random() * 0.5 + 0.5;
        let mut p: Vec<(&str, f64)> = Vec::new();
        if cont_rot {
            r0 += 45.0 * r.random() - 9.0 * r.random();
            p.push(("rotateZ", r0));
        } else if r.random() < 0.5 {
            p.push(("rotateZ", 360.0 * r.random()));
        } else {
            p.push(("rotateZ", -360.0 * r.random()));
        }
        p.push(("rotateX", 18.0 - 36.0 * r.random()));
        p.push(("rotateY", 18.0 - 36.0 * r.random()));
        p.push(("translateX", (2.0 * r.random() - 1.0) * tscl));
        p.push(("translateY", (2.0 * r.random() - 1.0) * tscl));
        p.push(("translateZ", (2.0 * r.random() - 1.0) * tscl));
        if with_shear {
            for name in ["shearXY", "shearXZ", "shearYX", "shearYZ", "shearZX", "shearZY"] {
                if r.random() > 0.5 {
                    p.push((name, (2.0 * r.random() - 1.0) * tscl * 0.1));
                }
            }
        }
        scl *= 0.8 + r.random() * 0.1;
        tscl *= 0.8 + r.random() * 0.1;
        p.extend([("scaleX", scl), ("scaleY", scl), ("scaleZ", scl)]);
        set(add(&mut xf, amount, "affine3D"), &p);
        xf.color = r.random();
        xf.weight = scl * r.random() * 19.9 + 0.1;
        if r.random() < 0.56 {
            let name = java::random_variation(r);
            add(&mut xf, r.random() * 0.25 + 0.25, name);
        }
        layer.xforms.push(xf);
    }
    flame
}

/// `SierpinskyRandomFlameGenerator`.
pub fn sierpinsky(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = empty_flame();
    flame.cam_roll = rand_int(r, 8) as f64 * -45.0;
    flame.cam_zoom = 4.56;
    let layer = flame.first_layer_mut();
    let pos = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];
    for (i, &(x, y)) in pos.iter().enumerate() {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.coefs.c20 = x;
        xf.coefs.c21 = y;
        xf.color = r.random();
        xf.color_symmetry = r.random();
        let lin = |r: &mut crate::rng::Rng| if r.random() < 0.25 { "dc_linear" } else { "linear3D" };
        if i == pos.len() - 1 {
            if r.random() < 0.5 {
                let name = lin(r);
                add(&mut xf, 0.5, name);
                rotate(&mut xf, 5.0 - 10.0 * r.random(), false);
                local_translate(&mut xf, 0.1 - 0.2 * r.random(), 0.1 - 0.2 * r.random(), false);
                let s = 1.0 - 0.1 * r.random();
                let (sx, sy) = (r.random() < 0.75, r.random() > 0.25);
                scale(&mut xf, s, sx, sy, false);
            }
            let name = pick(r, experimental());
            let amount = if r.random() < 0.33 {
                0.5 - r.random()
            } else if r.random() < 0.67 {
                0.25 + 0.5 * r.random()
            } else {
                0.5
            };
            add(&mut xf, amount, name);
            if r.random() < 0.5 {
                let s = 1.0 - 0.1 * r.random();
                let (sx, sy) = (r.random() < 0.75, r.random() > 0.25);
                scale(&mut xf, s, sx, sy, false);
            }
        } else {
            let name = lin(r);
            add(&mut xf, 0.5, name);
        }
        layer.xforms.push(xf);
    }
    flame
}

/// `SphericalRandomFlameGenerator`.
pub fn spherical(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = empty_flame();
    flame.cam_roll = 90.0;
    flame.cam_zoom = 2.4;
    flame.cam_perspective = 0.32;
    let layer = flame.first_layer_mut();
    let quarter = |r: &mut crate::rng::Rng| {
        if r.random() < 0.5 { if r.random() < 0.5 { 180.0 } else { 90.0 } } else { -90.0 }
    };
    {
        let mut xf = xform();
        xf.weight = 4.0 + 12.0 * r.random();
        add(&mut xf, 1.0, "spherical3D");
        let a = quarter(r);
        rotate(&mut xf, a, false);
        global_translate(&mut xf, 1.0, 0.0, false);
        xf.color = 1.0;
        xf.color_symmetry = 0.9 + r.random() * 0.2;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 3.0 + 11.0 * r.random();
        add(&mut xf, 1.0, "spherical3D");
        let a = quarter(r);
        rotate(&mut xf, a, false);
        xf.color = 0.5;
        xf.color_symmetry = 0.9 + r.random() * 0.2;
        layer.xforms.push(xf);
    }
    for (w0, w1, sign) in [(0.3, 0.2, 1.0), (0.2, 0.15, -1.0)] {
        let mut xf = xform();
        xf.weight = w0 + w1 * r.random();
        add(&mut xf, 1.0, "linear3D");
        rotate(&mut xf, 90.0, false);
        global_translate(&mut xf, sign * ((2.0 + r.random() * 2.0) as i32) as f64, 0.0, false);
        xf.color = r.random();
        xf.color_symmetry = 0.0;
        layer.xforms.push(xf);
    }
    for _ in 0..rand_int(r, 4) {
        let mut xf = xform();
        xf.weight = 0.5 + 0.3 * r.random();
        let list = experimental();
        let name = list[rand_int(r, list.len())];
        add(&mut xf, 1.0, name);
        rotate(&mut xf, 90.0 - r.random() * 180.0, false);
        scale(&mut xf, 0.2 + 0.2 * r.random(), true, true, false);
        global_translate(&mut xf, 0.25 - r.random() * 0.5, 0.25 - r.random() * 0.5, false);
        xf.color = r.random();
        xf.color_symmetry = 0.0;
        layer.xforms.push(xf);
    }
    randomize_colors(layer, r);
    flame
}

/// `Spherical3DRandomFlameGenerator`.
pub fn spherical3d(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = empty_flame();
    flame.centre_y = -0.20;
    flame.cam_pitch = 48.0;
    flame.cam_yaw = 112.0;
    flame.cam_zoom = 3.6;
    flame.cam_perspective = 0.32;
    flame.preserve_z = r.random() < 0.5;
    let invert = r.random() > 0.5;
    let layer = flame.first_layer_mut();
    for (i, w) in [(0, 1.0 + 4.0 * r.random()), (1, 0.5 + r.random() * 4.5)] {
        let mut xf = xform();
        xf.weight = w;
        let v = add(&mut xf, 1.0, "spherical3D_wf");
        if invert {
            set(v, &[("invert", 1.0)]);
        }
        if i == 0 {
            let a = if r.random() < 0.5 { 90.0 } else { -90.0 };
            rotate(&mut xf, a, false);
            global_translate(&mut xf, 1.0, 0.0, false);
            xf.color = 1.0;
        } else {
            rotate(&mut xf, 90.0, false);
            xf.color = 0.5;
        }
        xf.color_symmetry = 0.9 + r.random() * 0.1;
        layer.xforms.push(xf);
    }
    let cylinder = if r.random() < 0.5 { "cylinder" } else { "cylinder_apo" };
    static LIST: OnceLock<Vec<&'static str>> = OnceLock::new();
    let fnc_list = LIST.get_or_init(|| {
        java::filter(&[
            "bipolar",
            "blade",
            "blur",
            "blur3D",
            "cannabiscurve_wf",
            "crackle",
            "cylinder",
            "cylinder_apo",
            "edisc",
            "flower",
            "glynnSim2",
            "julia3D",
            "mandelbrot",
            "modulus",
            "noise",
            "parabola",
            "pie",
            "pie3D",
            "checks",
            "pre_subflame_wf",
            "radial_blur",
            "rays",
            "rings",
            "rose_wf",
            "secant2",
            "sinusoidal",
            "spiral",
            "spirograph",
            "splits",
            "square",
            "twintrian",
            "wedge_julia",
        ])
    });
    for (blur, cyl_chance, sy) in [((0.25, 0.25), 0.66, 5.0), ((0.025, 0.025), 0.33, 3.0)] {
        let mut xf = xform();
        xf.weight = 0.5;
        add(&mut xf, blur.0 + blur.1 * r.random(), "pre_blur");
        add(&mut xf, 0.01 + r.random() * 0.39, "ztranslate");
        let fnc = if r.random() < cyl_chance { cylinder } else { pick(r, fnc_list) };
        add(&mut xf, 0.01 + r.random() * 0.39, fnc);
        scale(&mut xf, sy, false, true, true);
        xf.color = 1.0;
        xf.color_symmetry = -1.0;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.1;
        add(&mut xf, 0.005 + r.random() * 0.005, "gaussian_blur");
        add(&mut xf, 0.005 + r.random() * 0.005, "ztranslate");
        global_translate(&mut xf, 0.3, 0.0, true);
        xf.color = 1.0;
        xf.color_symmetry = -1.0;
        layer.xforms.push(xf);
    }
    if r.random() < 0.5 {
        let mut xf = xform();
        xf.weight = 0.1;
        let style = rand_int(r, 3);
        let mut v = match style {
            0 | 1 => {
                let mut v = crate::model::VariationInstance::new(if style == 1 { "julia3D" } else { "julia3Dz" }, 1.0);
                let power = -4 + rand_int(r, 9) as i32;
                v.set_param("power", power as f64);
                if r.random() > 0.5 {
                    add(&mut xf, 0.005 + r.random() * 0.5, "linear3D");
                }
                v
            }
            _ => {
                let mut v = crate::model::VariationInstance::new("spherical3D_wf", 1.0);
                if r.random() < 0.5 {
                    v.set_param("invert", 1.0);
                }
                v
            }
        };
        v.amount = 0.25 + r.random() * 0.75;
        xf.variations.push(v);
        rotate(&mut xf, 45.0, false);
        global_translate(&mut xf, 0.0, 0.25, false);
        global_translate(&mut xf, 0.0, -0.25, true);
        xf.color = 1.0;
        xf.color_symmetry = 0.5 + r.random() * 0.5;
        layer.final_xforms.push(xf);
    }
    if r.random() > 0.5 {
        for i in 0..2 {
            set_xaos(&mut layer.xforms[i], 3, 0.0);
            set_xaos(&mut layer.xforms[i], 4, 0.0);
        }
    }
    randomize_colors(layer, r);
    flame
}

/// `BubblesRandomFlameGenerator`.
pub fn bubbles(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = empty_flame();
    flame.cam_zoom = 0.5;
    flame.pixels_per_unit = 75.0 + r.random() * 42.0;
    let layer = flame.first_layer_mut();
    let exp = experimental();
    {
        let mut xf = xform();
        xf.weight = 12.0 + r.random() * 80.0;
        add(&mut xf, 2.0 + r.random() * 4.0, "spherical");
        let name = if r.random() < 0.15 {
            exp[rand_int(r, exp.len())]
        } else if r.random() < 0.8 {
            "eyefish"
        } else {
            "fisheye"
        };
        add(&mut xf, 0.05 + r.random() * 0.5, name);
        xf.color_symmetry = 0.991 + r.random() * 0.08;
        scale(&mut xf, 0.5 - r.random() * 0.5, true, true, false);
        rotate(&mut xf, 180.0 - r.random() * 360.0, false);
        local_translate(&mut xf, 3.0 - 6.0 * r.random(), 3.0 - 6.0 * r.random(), false);
        if r.random() < 0.33 {
            local_translate(&mut xf, 0.75 - 1.5 * r.random(), 0.75 - 1.5 * r.random(), true);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.5 + r.random() * 0.8;
        if r.random() < 0.05 {
            add(&mut xf, 0.01 + r.random() * 0.4, "bubble");
            set(add(&mut xf, 0.01 + r.random() * 0.04, "checks"), &[("size", 5.0), ("x", 3.0), ("y", 3.0)]);
        } else {
            add(&mut xf, 0.1 + r.random() * 0.5, "bubble");
        }
        add(&mut xf, 4.0 + r.random() * 2.0, "pre_blur");
        xf.color_symmetry = -0.5;
        scale(&mut xf, 1.1 + r.random() * 1.9, true, true, false);
        local_translate(&mut xf, 0.75 - 1.50 * r.random(), 0.75 - 1.50 * r.random(), false);
        rotate(&mut xf, 30.0 - r.random() * 60.0, false);
        layer.xforms.push(xf);
    }
    if r.random() > 0.25 {
        let mut xf = xform();
        xf.weight = 0.5 + r.random() * 1.5;
        let name = if r.random() > 0.8 {
            exp[rand_int(r, exp.len())]
        } else if r.random() > 0.5 {
            "linear3D"
        } else {
            "noise"
        };
        add(&mut xf, 0.5, name);
        xf.color_symmetry = -0.5;
        rotate(&mut xf, 30.0 - r.random() * 60.0, false);
        if r.random() < 0.5 {
            scale(&mut xf, 0.5 + r.random() * 1.5, true, true, false);
        }
        layer.xforms.push(xf);
    }
    if r.random() > 0.5 {
        let mut xf = xform();
        xf.weight = 0.5 + r.random() * 1.5;
        let name = if r.random() > 0.8 {
            if r.random() > 0.75 { java::random_variation(r) } else { exp[rand_int(r, exp.len())] }
        } else if r.random() > 0.5 {
            "linear3D"
        } else {
            "gaussian_blur"
        };
        add(&mut xf, 0.5, name);
        xf.color_symmetry = -0.5;
        rotate(&mut xf, 30.0 - r.random() * 60.0, false);
        if r.random() < 0.5 {
            scale(&mut xf, 0.15 + r.random() * 1.25, true, true, false);
        }
        layer.xforms.push(xf);
    }
    distribute_colors(layer);
    flame
}

/// `Bubbles3DRandomFlameGenerator`.
pub fn bubbles3d(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = empty_flame();
    flame.cam_pitch = 49.0;
    flame.cam_yaw = 12.0;
    flame.cam_perspective = 0.12;
    let layer = flame.first_layer_mut();
    let exp = experimental();
    {
        let mut xf = xform();
        xf.weight = 1.0 + r.random() * 100.0;
        if r.random() < 0.75 {
            add(&mut xf, 0.05 + r.random() * 0.5, "bubble");
        } else {
            let z = -0.5 - r.random() * 2.0;
            let amount = 0.25 + r.random() * 0.5;
            set(add(&mut xf, amount, "bubble2"), &[("z", z)]);
        }
        add(&mut xf, 0.5 + 1.5 * r.random(), "pre_blur");
        static WORKING: OnceLock<Vec<&'static str>> = OnceLock::new();
        let working = WORKING.get_or_init(|| {
            java::filter(&[
                "fan2",
                "blade",
                "blade3D",
                "blob",
                "blob3D",
                "bwraps7",
                "cell",
                "cannabiscurve_wf",
                "cloverleaf_wf",
                "cos",
                "cot",
                "coth",
                "cross",
                "csch",
                "diamond",
                "disc",
                "edisc",
                "epispiral_wf",
                "fan",
                "fisheye",
                "eyefish",
                "flux",
                "heart",
                "julia",
                "julian",
                "juliascope",
                "log",
                "parabola",
                "power",
                "epispiral",
                "pre_subflame_wf",
                "rectangles",
                "rose_wf",
                "sech",
                "separation",
                "split",
                "truchet",
                "wedge",
                "zcone",
            ])
        });
        let list: &[&str] = if r.random() < 0.5 { working } else { exp };
        let name = if r.random() < 0.5 { java::random_variation(r) } else { list[rand_int(r, list.len())] };
        add(&mut xf, -0.02 + 0.04 * r.random(), name);
        let shape = r.random();
        if shape < 0.125 {
            add(&mut xf, 0.001 + r.random() * 0.01, "hexes");
        } else if shape < 0.25 {
            add(&mut xf, 0.001 + r.random() * 0.01, "oscilloscope");
        } else if shape < 0.5 {
            let name = java::random_variation(r);
            add(&mut xf, 0.001 + r.random() * 0.01, name);
        }
        if r.random() > 0.33 {
            global_translate(&mut xf, -1.0 + 2.0 * r.random(), -1.0 + 2.0 * r.random(), true);
        }
        if r.random() > 0.33 {
            rotate(&mut xf, -32.0 + r.random() * 72.0, false);
        }
        if r.random() > 0.33 {
            scale(&mut xf, 0.8 + r.random() * 0.4, true, false, false);
        }
        xf.color = 0.0;
        xf.color_symmetry = 0.0;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 1.0 + r.random() * 50.0;
        let name = if r.random() > 0.5 { exp[rand_int(r, exp.len())] } else { "eyefish" };
        add(&mut xf, -1.5 + 2.0 * r.random() * 3.0, name);
        add(&mut xf, 1.0 - 3.5 * r.random(), "hemisphere");
        xf.color = 0.0;
        xf.color_symmetry = 0.0;
        if r.random() > 0.33 {
            global_translate(&mut xf, -3.0 + 62.0 * r.random(), -3.0 + 6.0 * r.random(), false);
        }
        if r.random() > 0.33 {
            rotate(&mut xf, -32.0 + r.random() * 72.0, false);
        }
        if r.random() > 0.33 {
            scale(&mut xf, 0.8 + r.random() * 0.4, true, false, false);
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 1.0 + r.random() * 100.0;
        let name = if r.random() > 0.5 { "julia3Dz" } else { "julia3D" };
        let mut idx = if r.random() > 0.5 { 2 } else { (-20.0 + r.random() * 40.0) as i32 };
        if idx == 0 || idx == 1 {
            idx = 2;
        }
        set(add(&mut xf, 1.0, name), &[("power", idx as f64)]);
        xf.color = 0.0;
        xf.color_symmetry = 0.0;
        global_translate(&mut xf, -1.0 + 2.0 * r.random(), -3.0 + 6.0 * r.random(), false);
        global_translate(&mut xf, -0.1 + 0.2 * r.random(), -0.1 + 0.2 * r.random(), true);
        rotate(&mut xf, -20.0 + r.random() * 40.0, false);
        scale(&mut xf, 0.8 + r.random() * 0.4, true, true, false);
        layer.xforms.push(xf);
    }
    flame.preserve_z = r.random() > 0.5;
    randomize_colors(flame.first_layer_mut(), r);
    flame
}

/// `ExperimentalBubbles3DRandomFlameGenerator` ("Bubbles3D (experimental)").
pub fn experimental_bubbles3d(st: &mut State) -> Flame {
    let mut flame = bubbles3d(st);
    let r = &mut *st.rng;
    if let Some(xf) = flame.first_layer_mut().xforms.last_mut() {
        let s = 0.5 + 5.0 * r.random();
        let (sx, sy) = (r.random() < 0.75, r.random() < 0.75);
        scale(xf, s, sx, sy, false);
        rotate(xf, 180.0 - r.random() * 360.0, false);
        local_translate(xf, 4.0 - 8.0 * r.random(), 4.0 - 8.0 * r.random(), false);
    }
    flame
}
