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

//! Ported solid rendering generators (`SolidRandomFlameGenerator` and its subclasses).

use std::f64::consts::PI;

use super::gens_b::{affine, ftoi};
use super::java::{self, Plane, add, rand_int, set, xform};
use super::{Generator, State};
use crate::model::{DrawMode, Flame, Layer, VariationInstance, XForm};
use crate::rng::Rng;
use crate::solid::{LightDiffFunc, ShadowType};

/// `randomizeSolidRenderingSettings`.
fn randomize_solid_settings(flame: &mut Flame, r: &mut Rng) {
    let s = &mut flame.solid;
    s.enabled = true;
    flame.preserve_z = true;
    if let Some(m) = s.materials.first_mut() {
        m.diffuse = 0.3 + r.random() * 0.3;
        m.ambient = 0.7 + r.random() * 0.3;
        m.phong = 0.2 + r.random() * 0.8;
        m.phong_size = 2.0 + r.random() * 10.0;
        m.light_diff_func = if r.random() < 0.5 { LightDiffFunc::CosaSquare } else { LightDiffFunc::Cosa };
    }
    if let Some(l) = s.lights.first_mut() {
        l.altitude = 60.0 + r.random() * 60.0;
        l.azimuth = r.random() * 360.0;
    }
    s.shadow_type = ShadowType::Fast;
    s.shadowmap_size = 4096;
    s.shadowmap_bias = 0.05 + r.random() * 0.1;
}

/// `SolidRandomFlameGenerator.postProcessFlameBeforeRendering`: JWildfire's solid render
/// defaults (`setDefaultSolidRenderingSettings`) and the light directions.
pub fn solid_post(flame: &mut Flame, r: &mut Rng) {
    flame.spatial_filter_radius = 0.5;
    flame.filter_kernel = crate::render::filter::FilterKernel::Gaussian;
    flame.de_radius = 1.0;
    flame.de_curve = 0.8;
    flame.antialias_amount = 0.0;
    flame.antialias_radius = 0.0;
    flame.spatial_oversampling = 1;
    let lights = &mut flame.solid.lights;
    if lights.len() > 1 {
        lights[0].altitude = 60.0 - 120.0 * r.random();
        lights[0].azimuth = 30.0 - 60.0 * r.random();
    }
    if lights.len() > 2 {
        lights[1].altitude = 40.0 - 80.0 * r.random();
        lights[1].azimuth = 80.0 - 160.0 * r.random();
    }
}

/// Sets a random formula preset (`WFFuncPresetsStore....getRandomPresetId`).
fn random_preset(v: &mut VariationInstance, r: &mut Rng) {
    if let Some(ps) = crate::variations::presets::presets(&v.name).filter(|ps| !ps.is_empty()) {
        let id = ps[rand_int(r, ps.len())].id;
        crate::variations::presets::apply(v, id);
    }
}

/// `getRandom3DShape`.
fn random_3d_shape(r: &mut Rng) -> VariationInstance {
    let new = |name: &str| VariationInstance::new(name, 1.0);
    match rand_int(r, 23) {
        0 => {
            let mut v = new("yplot2d_wf");
            random_preset(&mut v, r);
            set(&mut v, &[("xmin", -3.0), ("xmax", 2.0), ("ymin", -4.0), ("ymax", 4.0), ("zmin", -2.0), ("zmax", 2.0), ("direct_color", 1.0)]);
            v
        }
        1 => {
            let mut v = new("seashell3D");
            let p = [("height", 3.0 + r.random() * 3.0), ("inner_radius", 0.2 + r.random() * 0.4), ("final_radius", 0.2 + r.random() * 0.4)];
            set(&mut v, &p);
            v
        }
        2 => {
            let mut v = new("yplot3d_wf");
            random_preset(&mut v, r);
            set(&mut v, &[("xmin", -3.0), ("xmax", 2.0), ("ymin", -2.0), ("ymax", 2.0), ("zmin", -2.0), ("zmax", 2.0), ("direct_color", 1.0)]);
            v
        }
        3 => new("text_wf"),
        4 => new("terrain3D"),
        5 => new("superShape3d"),
        6 => {
            let mut v = new("polarplot2d_wf");
            random_preset(&mut v, r);
            set(&mut v, &[("tmin", -PI), ("tmax", PI), ("rmin", -4.0), ("rmax", 4.0), ("zmin", -2.0), ("zmax", 2.0), ("direct_color", 1.0)]);
            v
        }
        7 => new("sunflower"),
        8 => new("rhodonea"),
        9 => new("dustpoint"),
        10 => new("klein_group"),
        11 => {
            let mut v = new("polarplot3d_wf");
            random_preset(&mut v, r);
            set(&mut v, &[("tmin", -PI), ("tmax", PI), ("umin", -PI), ("umax", PI), ("rmin", -2.0), ("rmax", 2.0), ("direct_color", 1.0)]);
            v
        }
        12 => new("butterfly_fay"),
        13 => new("knots3D"),
        14 => new("mandelbrot"),
        15 => {
            let mut v = new("pie3D");
            set(&mut v, &[("thickness", 0.2 + r.random() * 0.4)]);
            v
        }
        16 => {
            let mut v = new("parplot2d_wf");
            random_preset(&mut v, r);
            set(&mut v, &[("umin", -PI), ("umax", PI), ("vmin", -3.0), ("vmax", 8.0), ("direct_color", 1.0)]);
            let mode = if r.random() > 0.666 {
                2.0
            } else if r.random() < 0.5 {
                0.0
            } else {
                1.0
            };
            set(&mut v, &[("color_mode", mode)]);
            v
        }
        17 => {
            let mut v = new("dla3d_wf");
            let p = [
                ("max_iter", 100.0 + r.random() * 200.0),
                ("inner_blur_radius", r.random() * 0.5 + 0.1),
                ("outer_blur_radius", r.random() * 0.5),
                ("glue_radius", 0.6 + r.random() * 0.8),
            ];
            set(&mut v, &p);
            v
        }
        18 => {
            let mut v = new("obj_mesh_primitive_wf");
            let primitive = rand_int(r, 14) as f64;
            let subdiv = if r.random() > 0.5 {
                0.0
            } else if r.random() > 0.5 {
                2.0
            } else {
                1.0
            };
            set(
                &mut v,
                &[
                    ("primitive", primitive),
                    ("scale_x", 1.0),
                    ("scale_y", 1.0),
                    ("scale_z", 1.0),
                    ("offset_x", 0.0),
                    ("offset_y", 0.0),
                    ("offset_z", 0.0),
                    ("subdiv_level", subdiv),
                    ("subdiv_smooth_passes", 12.0),
                    ("subdiv_smooth_lambda", 0.42),
                    ("subdiv_smooth_mu", -0.45),
                ],
            );
            v
        }
        19 => new("sattractor3D"),
        20 => new("oscilloscope2"),
        21 => new("spirograph"),
        _ => {
            let mut v = new("primitives_wf");
            set(&mut v, &[("shape", rand_int(r, 6) as f64)]);
            v
        }
    }
}

fn add_shape(xf: &mut XForm, amount: f64, r: &mut Rng) {
    let mut v = random_3d_shape(r);
    v.amount = amount;
    xf.variations.push(v);
}

fn add_random(xf: &mut XForm, amount: f64, r: &mut Rng) {
    let name = java::random_variation_no_fract(r, false);
    add(xf, amount, name);
}

/// Random scale, rotation and translation in an edit plane.
fn place(xf: &mut XForm, plane: Plane, post: bool, r: &mut Rng) {
    java::scale_in(xf, plane, 1.25 - r.random() * 0.5, true, true, post);
    java::rotate_in(xf, plane, 360.0 * r.random(), post);
    java::local_translate_in(xf, plane, 1.0 - 2.0 * r.random(), 1.0 - 2.0 * r.random(), post);
}

/// `flame.setEditPlane(Math.random() > 0.666 ? XY : Math.random() < 0.5 ? YZ : ZX)`.
fn random_plane(r: &mut Rng) -> Plane {
    if r.random() > 0.666 {
        Plane::XY
    } else if r.random() < 0.5 {
        Plane::YZ
    } else {
        Plane::ZX
    }
}

/// The common camera of the solid generators (601 × 338 frame).
fn solid_flame(r: &mut Rng, bank: f64, zoom: (f64, f64)) -> Flame {
    let mut flame = java::flame();
    flame.cam_pitch = 90.0 - r.random() * 180.0;
    flame.cam_yaw = 30.0 - r.random() * 60.0;
    flame.cam_bank = bank - r.random() * 2.0 * bank;
    flame.cam_perspective = r.random() * 0.2;
    flame.width = 601;
    flame.height = 338;
    flame.pixels_per_unit = 100.0 + r.random() * 100.0;
    flame.cam_zoom = zoom.0 + r.random() * zoom.1;
    randomize_solid_settings(&mut flame, r);
    flame
}

/// The shape transform of several solid generators (mostly hidden, random placement).
fn shape_xform(r: &mut Rng, amount: impl FnOnce(&mut Rng) -> f64) -> XForm {
    let mut xf = xform();
    xf.weight = 0.5;
    xf.color = r.random();
    xf.color_symmetry = 1.0 - 2.0 * r.random();
    if r.random() > 0.125 {
        xf.draw_mode = DrawMode::Hidden;
    }
    let a = amount(r);
    add_shape(&mut xf, a, r);
    let plane = random_plane(r);
    place(&mut xf, plane, false, r);
    if r.random() > 0.5 {
        place(&mut xf, plane, true, r);
    }
    xf
}

/// The optional random final transform of the solid generators.
fn solid_final(r: &mut Rng) -> XForm {
    let mut xf = xform();
    add_random(&mut xf, 0.75 + r.random() * 0.25, r);
    if r.random() > 0.5 {
        add(&mut xf, 0.25 + r.random() * 0.25, "linear3D");
    }
    if r.random() > 0.5 {
        place(&mut xf, Plane::XY, false, r);
    }
    if r.random() > 0.5 {
        place(&mut xf, Plane::XY, true, r);
    }
    xf
}

/// `SolidExperimentalRandomFlameGenerator` ("Solid (experimental)").
pub fn solid_experimental(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = solid_flame(r, 45.0, (0.2, 0.4));
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = if r.random() < 0.3 { 0.5 } else { 0.25 + r.random() };
        xf.color = r.random();
        xf.color_symmetry = 1.0 - 2.0 * r.random();
        xf.coefs = affine(0.58839186, 0.93477004, -0.93477004, 0.58839186, 0.88339811, 0.59357541);
        xf.post = affine(-1.0, -0.0000000000000001, 0.0000000000000001, -1.0, 0.0, 0.0);
        xf.yz_post = affine(-0.00074102, -0.99999973, 0.99999973, -0.00074102, 0.0, 0.0);
        if r.random() < 0.85 {
            let (p, q) = if r.random() > 0.75 { (4.0 + r.random() * 3.0, 4.0 + r.random() * 2.0) } else { (5.0, 4.0) };
            let amount = 0.25 + r.random() * 0.75;
            set(add(&mut xf, amount, "hypertile3D2"), &[("p", p), ("q", q)]);
        } else {
            add_random(&mut xf, 0.25 + r.random() * 0.75, r);
        }
        layer.xforms.push(xf);
    }
    let xf = shape_xform(r, |r| 0.2 + r.random() * 0.5);
    layer.xforms.push(xf);
    if r.random() > 0.666 {
        layer.final_xforms.push(solid_final(r));
    }
    flame
}

/// `SolidStunningRandomFlameGenerator` ("Solid (stunning)").
pub fn solid_stunning(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = solid_flame(r, 30.0, (0.5, 0.7));
    let layer = flame.first_layer_mut();
    let xf = shape_xform(r, |r| 0.2 + r.random());
    layer.xforms.push(xf);
    let linear = |r: &mut Rng, zx: crate::model::Affine, post_chance: f64| {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.coefs = affine(0.54625622, -0.26758811, 0.26758811, 0.54625622, 0.0, 0.0);
        xf.yz_coefs = affine(0.46864442, 0.17017929, -0.17017929, 0.46864442, 1.21536218, -0.02558657);
        xf.zx_coefs = zx;
        add(&mut xf, 1.0, "linear3D");
        place(&mut xf, Plane::XY, false, r);
        if r.random() > post_chance {
            place(&mut xf, Plane::XY, true, r);
        }
        xf
    };
    let xf = linear(r, affine(0.81078767, -0.12035676, 0.12035676, 0.81078767, 0.21748586, -0.05117314), 0.666);
    layer.xforms.push(xf);
    if r.random() > 0.666 {
        let xf = linear(r, affine(0.59207155, 0.56684538, -0.56684538, 0.59207155, 1.31770847, -0.84435688), 0.5);
        layer.xforms.push(xf);
    }
    if r.random() > 0.42 {
        layer.final_xforms.push(solid_final(r));
    }
    flame
}

/// `SolidJulia3DRandomFlameGenerator` ("Solid Julia3D").
pub fn solid_julia3d(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = solid_flame(r, 15.0, (0.6, 0.7));
    let layer = flame.first_layer_mut();
    {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.color = 0.16556899;
        add_shape(&mut xf, 0.5 + r.random() * 2.0, r);
        if r.random() > 0.75 {
            let (wavelen, phase) = (0.75 + r.random() * 0.5, 0.5 + r.random());
            let amount = 0.2 + r.random() * 0.2;
            let v = add(&mut xf, amount, "pre_wave3D_wf");
            set(v, &[("wavelen", wavelen), ("phase", phase), ("damping", 0.01)]);
            v.priority = 0;
        }
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.color = 0.02637356;
        xf.coefs = affine(0.53041695, 0.14810786, -0.14810786, 0.53041695, 2.09422612, 0.23893855);
        xf.yz_coefs = affine(1.0, 0.0, 0.0, 1.0, 0.01405521, -0.01405521);
        xf.zx_coefs = affine(0.74081053, 0.04366518, -0.04366518, 0.74081053, -0.07027604, -0.14055209);
        add(&mut xf, 1.0, "linear3D");
        add(&mut xf, -0.524, "zscale");
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        let power = ftoi(3.0 + r.random() * 4.0) as f64;
        let divisor = if r.random() > 0.5 { 2.0 } else { 1.0 + r.random() * 2.0 };
        set(add(&mut xf, 1.0, "julia3Dq"), &[("power", power), ("divisor", divisor)]);
        layer.final_xforms.push(xf);
    }
    flame
}

/// `SolidrecursiveRandomFlameGenerator` ("Solid (recursive)").
pub fn solid_recursive(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = solid_flame(r, 30.0, (0.5, 0.25));
    let layer = flame.first_layer_mut();
    let xf = shape_xform(r, |r| if r.random() < 0.75 { 0.1 + r.random() * 0.3 } else { 0.2 + r.random() * 0.5 });
    layer.xforms.push(xf);
    let affine3d = |r: &mut Rng, t: [f64; 3], scl: f64, rot: [f64; 3]| {
        let mut xf = xform();
        xf.weight = 0.5;
        xf.color = r.random();
        xf.color_symmetry = 1.0 - 2.0 * r.random();
        set(
            add(&mut xf, 1.0, "affine3D"),
            &[
                ("translateX", t[0]),
                ("translateY", t[1]),
                ("translateZ", t[2]),
                ("scaleX", scl),
                ("scaleY", scl),
                ("scaleZ", scl),
                ("rotateX", rot[0]),
                ("rotateY", rot[1]),
                ("rotateZ", rot[2]),
                ("shearXY", 0.0),
                ("shearXZ", 0.0),
                ("shearYX", 0.0),
                ("shearYZ", 0.0),
                ("shearZX", 0.0),
                ("shearZY", 0.0),
            ],
        );
        xf
    };
    {
        let t = [0.5 + r.random() * 2.0, 0.25 + r.random(), 0.0];
        let scl = 0.55 + r.random() * 0.2;
        let rot = [r.random() * 4.0, 0.0, r.random() * 4.0];
        let xf = affine3d(r, t, scl, rot);
        layer.xforms.push(xf);
    }
    {
        let t = [0.1 + r.random() * 0.2, 0.5 + r.random() * 3.0, 0.35 + r.random()];
        let scl = 0.45 + r.random() * 0.3;
        let rot = [1.0 + r.random() * 5.0, 25.0 - 50.0 * r.random(), 2.0 - 4.0 * r.random()];
        let xf = affine3d(r, t, scl, rot);
        layer.xforms.push(xf);
    }
    if r.random() < 0.25 {
        let t = [0.5 - r.random(), 0.5 - r.random(), 0.5 - r.random()];
        let scl = 0.35 + r.random() * 0.2;
        let rot = [3.0 - r.random() * 6.0, 3.0 - r.random() * 6.0, 3.0 - r.random() * 6.0];
        let xf = affine3d(r, t, scl, rot);
        layer.xforms.push(xf);
    }
    flame
}

/// The sub-flame of "Solid Labyrinth": crackle, metaballs, truchet or perlin noise.
fn labyrinth_sub_flame(r: &mut Rng) -> Flame {
    let mut flame = java::flame();
    flame.width = 711;
    flame.height = 400;
    flame.pixels_per_unit = 38.88427164;
    flame.layers = vec![Layer::default()];
    let mut xf = xform();
    xf.weight = 0.5;
    if r.random() < 0.5 {
        if r.random() < 0.45 {
            let p = [
                ("cellsize", 0.15 + r.random() * 0.5),
                ("power", 0.2 + r.random() * 1.2),
                ("distort", if r.random() > 0.2 { r.random() * 0.5 } else { 0.0 }),
                ("scale", if r.random() > 0.667 { 1.06 - r.random() * 0.12 } else { 1.0 }),
                ("z", r.random() * 0.5),
                ("color_scale", 0.5),
                ("color_offset", 0.0),
            ];
            let amount = 1.0 + r.random() * 2.0;
            set(add(&mut xf, amount, "dc_crackle_wf"), &p);
        } else if r.random() < 0.45 {
            add(&mut xf, 0.15 + r.random() * 0.75, "metaballs3d_wf");
        } else {
            let p = [
                ("extended", if r.random() < 0.25 { 1.0 } else { 0.0 }),
                ("exponent", 0.2 + r.random() * 1.3),
                ("arc_width", 0.5 + r.random() * 0.5),
                ("rotation", 0.0),
                ("size", 0.4 + r.random() * 0.3),
                ("seed", 50.0 + r.random() * 50.0),
                ("direct_color", 1.0),
            ];
            set(add(&mut xf, 1.0, "truchet"), &p);
            add(&mut xf, 2.0 + r.random() * 2.0, "pre_blur");
        }
    } else {
        let octave = |r: &mut Rng| if r.random() < 0.33 { ftoi(2.0 + r.random() * 2.0) as f64 } else { 2.0 };
        let p = [
            ("shape", (r.random() * 3.0) as i32 as f64),
            ("map", if r.random() < 0.15 { (r.random() * 6.0) as i32 as f64 } else { 2.0 }),
            ("select_centre", 0.0),
            ("select_range", 1.0),
            ("centre", 0.25 + r.random() * 0.5),
            ("range", 0.25 + r.random() * 0.5),
            ("edge", 0.0),
            ("scale", 1.0 + r.random() * 5.0),
            ("octaves", octave(r)),
            ("amps", octave(r)),
            ("freqs", octave(r)),
            ("z", 0.0),
            ("select_bailout", 10.0),
        ];
        let amount = 3.0 + r.random() * 4.0;
        set(add(&mut xf, amount, "dc_perlin"), &p);
    }
    flame.layers[0].xforms.push(xf);
    flame
}

/// `SolidLabyrinthRandomFlameGenerator` ("Solid Labyrinth"): a sub-flame lifted into 3D.
pub fn solid_labyrinth(st: &mut State) -> Flame {
    let r = &mut *st.rng;
    let mut flame = java::flame();
    flame.cam_pitch = 30.0 + r.random() * 40.0;
    flame.cam_yaw = 30.0 - r.random() * 60.0;
    flame.cam_bank = 5.0 - r.random() * 10.0;
    flame.cam_perspective = r.random() * 0.2;
    flame.width = 601;
    flame.height = 338;
    flame.pixels_per_unit = 100.0 + r.random() * 100.0;
    flame.cam_zoom = 0.75 + r.random() * 0.5;
    randomize_solid_settings(&mut flame, r);
    if r.random() < 0.333 {
        flame.solid.enabled = false;
    }
    let sub = labyrinth_sub_flame(r);
    let mut xf = xform();
    xf.weight = 0.5;
    {
        let v = add(&mut xf, 1.0, "subflame_wf");
        v.set_resource("flame", &crate::io::write_flame(&sub));
        let mut cs = 1.5 - r.random() * 3.0;
        if cs.abs() < 0.4 {
            cs = if r.random() < 0.5 { -0.3 - r.random() } else { 0.3 + r.random() };
        }
        set(v, &[("color_mode", 0.0), ("colorscale_z", cs)]);
    }
    xf.color = 0.0;
    xf.color_symmetry = -0.22;
    flame.first_layer_mut().xforms.push(xf);
    flame
}

/// The base generators of "Solid (shadows)" (duplicates weight the choice).
const SHADOWS_BASE: &[Generator] = &[
    Generator::SolidStunning,
    Generator::SolidExperimental,
    Generator::SolidStunning,
    Generator::SolidStunning,
    Generator::SolidJulia3D,
    Generator::SolidStunning,
    Generator::SolidRecursive,
];

/// `SolidShadowsRandomFlameGenerator` ("Solid (shadows)").
pub fn solid_shadows(st: &mut State) -> Flame {
    let g = *st.sub.get_or_insert_with(|| SHADOWS_BASE[((st.rng.random() * SHADOWS_BASE.len() as f64) as usize).min(SHADOWS_BASE.len() - 1)]);
    g.prepare(&mut State { rng: &mut *st.rng, sub: None, simple: false })
}

/// `SolidShadowsRandomFlameGenerator.postProcessFlameAfterRendering`: a lower camera and a
/// floor (plane or checkerboard) layer that catches the shadows.
pub fn solid_shadows_after(flame: &mut Flame, r: &mut Rng) {
    flame.cam_zoom = 0.5 + r.random() * 0.25;
    flame.cam_perspective = 0.05 + r.random() * 0.15;
    flame.cam_pitch = -5.0 - r.random() * 30.0;
    flame.cam_yaw = -25.0 + r.random() * 50.0;
    flame.cam_bank = -15.0 + r.random() * 30.0;
    flame.cam_roll = 0.0;
    if let Some(v) = flame.layers.first_mut().and_then(|l| l.xforms.first_mut()).and_then(|xf| xf.variations.first_mut()) {
        v.amount *= 0.25 + r.random() * 0.25;
    }
    let mut layer = Layer { weight: 0.25 + r.random() * 0.5, gradient: crate::gradient::Gradient::random(r), ..Layer::default() };
    let mut xf = xform();
    xf.weight = 0.5;
    if r.random() < 0.5 {
        let p = [("axis", 2.0), ("position", r.random() * 2.0 - 0.25), ("color_mode", ftoi(4.0 * r.random()) as f64)];
        let amount = 1.0 + r.random();
        set(add(&mut xf, amount, "plane_wf"), &p);
    } else {
        let p = [
            ("axis", 2.0),
            ("position", r.random() * 2.0 - 0.25),
            ("checker_size", 0.05 + r.random() * 0.1),
            ("displ_amount", 0.005 + r.random() * 0.03),
            ("with_sides", if r.random() > 0.25 { 1.0 } else { 0.0 }),
        ];
        let amount = 1.0 + r.random();
        set(add(&mut xf, amount, "checkerboard_wf"), &p);
    }
    layer.xforms.push(xf);
    flame.layers.push(layer);
    flame.solid.shadow_type = if r.random() < 0.5 { ShadowType::Smooth } else { ShadowType::Fast };
}
