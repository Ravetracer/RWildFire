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

//! Ports of JWildfire's MutaGen mutations (`org.jwildfire.create.tina.mutagen`) that the
//! random flame generators use.

use crate::model::{Flame, Layer, VariationInstance, XForm};
use crate::render::dof::DofShape;
use crate::rng::Rng;
use crate::variations;

use super::java::{self, global_translate, rotate, scale};

/// `VariationFunc.mutate` (generic version): moves one random parameter a step.
pub fn mutate_variation(v: &mut VariationInstance, amount: f64, rng: &mut Rng) {
    let Some(def) = variations::find(&v.name) else { return };
    if def.params.is_empty() {
        return;
    }
    let idx = ((rng.random() * def.params.len() as f64) as usize).min(def.params.len() - 1);
    let p = &def.params[idx];
    let old = v.params.get(idx).copied().unwrap_or(p.default);
    let new = if p.integer {
        let da = (amount.round() as i64).max(1) as f64;
        if old >= 0.0 { old + da } else { old - da }
    } else {
        // `mutateStep`
        let step = if old < 1.0e-6 || rng.random() < 0.3 {
            if old >= 0.0 { 0.1 * amount } else { -0.1 * amount }
        } else if old >= 0.0 {
            old / 100.0 * amount
        } else {
            old / -100.0 * amount
        };
        old + step
    };
    v.set_param(p.name, new);
}

/// `RandomParamMutation.setRandomFlameProperty`: mutates one parameter of a random
/// variation (with parameters) of the layer.
pub fn set_random_flame_property(layer: &mut Layer, amount: f64, rng: &mut Rng) {
    const BLACKLIST: &[&str] = &[
        "truchet",
        "mandelbrot",
        "fract_formula_julia_wf",
        "fract_formula_mand_wf",
        "dc_perlin",
        "snowflake_wf",
        "crob",
        "tree_js",
        "brownian_js",
        "dragon_js",
        "maurer_lines",
        "htree_js",
        "gosperisland_js",
        "rsquares_js",
        "hilbert_js",
        "koch_js",
        "bubblet3d",
    ];
    let mut candidates: Vec<&mut VariationInstance> = layer
        .xforms
        .iter_mut()
        .chain(layer.final_xforms.iter_mut())
        .flat_map(|xf| xf.variations.iter_mut())
        .filter(|v| variations::find(&v.name).is_some_and(|d| !d.params.is_empty()))
        .filter(|v| !BLACKLIST.contains(&v.name.to_lowercase().as_str()))
        .collect();
    if !candidates.is_empty() {
        let idx = ((rng.random() * candidates.len() as f64) as usize).min(candidates.len() - 1);
        mutate_variation(candidates[idx], amount, rng);
    }
}

/// `RandomParamMutation.execute`.
pub fn random_param_mutation(layer: &mut Layer, strength: f64, rng: &mut Rng) {
    for f in [6.0, 5.0, 2.0] {
        let amount = f * (0.25 + 0.75 * rng.random()) * strength;
        set_random_flame_property(layer, amount, rng);
    }
}

/// `LocalGammaMutation.execute` (`XForm.randomizeModColorEffects` on every transform).
pub fn local_gamma_mutation(layer: &mut Layer, rng: &mut Rng) {
    for xf in &mut layer.xforms {
        let pair = |rng: &mut Rng| {
            let v = 1.0 - 2.0 * rng.random();
            let speed = if rng.random() < 0.33 { 1.0 - 2.0 * rng.random() } else { 0.0 };
            (v, speed)
        };
        (xf.mod_gamma, xf.mod_gamma_speed) = pair(rng);
        (xf.mod_contrast, xf.mod_contrast_speed) = pair(rng);
        (xf.mod_saturation, xf.mod_saturation_speed) = pair(rng);
        (xf.mod_hue, xf.mod_hue_speed) = pair(rng);
    }
}

/// `XFormTransformService.flipHorizontal`.
fn flip_horizontal(xf: &mut XForm, post: bool) {
    let a = if post { &mut xf.post } else { &mut xf.coefs };
    a.c00 = -a.c00;
    a.c10 = -a.c10;
}

/// `XFormTransformService.flipVertical`.
fn flip_vertical(xf: &mut XForm, post: bool) {
    let a = if post { &mut xf.post } else { &mut xf.coefs };
    a.c01 = -a.c01;
    a.c11 = -a.c11;
}

/// `AffineMutation.applyToXForm`.
pub fn affine_mutation_xform(xf: &mut XForm, amount: f64, rng: &mut Rng) {
    if rng.random() < 0.5 {
        let post = rng.random() >= 0.75;
        let a = if post { &mut xf.post } else { &mut xf.coefs };
        for c in [&mut a.c00, &mut a.c01, &mut a.c10, &mut a.c11, &mut a.c20, &mut a.c21] {
            if rng.random() < 0.5 {
                *c += amount * (-0.25 + 0.5 * rng.random());
            }
        }
    } else {
        if rng.random() < 0.33 {
            let post = rng.random() < 0.5;
            flip_horizontal(xf, post);
        }
        if rng.random() > 0.67 {
            let post = rng.random() < 0.5;
            flip_vertical(xf, post);
        }
        if rng.random() < 0.75 {
            let (dx, dy) = (-amount + 2.0 * rng.random() * amount, -amount + 2.0 * rng.random() * amount);
            let post = rng.random() < 0.5;
            global_translate(xf, dx, dy, post);
        }
        if rng.random() < 0.5 {
            let deg = -amount * 10.0 + 20.0 * rng.random() * amount;
            let post = rng.random() < 0.5;
            rotate(xf, deg, post);
        }
        if rng.random() < 0.5 {
            let s = -amount + 2.0 * rng.random() * amount;
            let (sx, sy) = (rng.random() < 0.75, rng.random() < 0.75);
            scale(xf, s, sx, sy, false);
        }
    }
}

/// `RandomBackgroundMutation.execute`: a new background xform with a random background
/// variation.
pub fn random_background_mutation(layer: &mut Layer, strength: f64, rng: &mut Rng) {
    layer.bg_xforms.clear();
    let mut xf = java::xform();
    xf.weight = (0.25 + rng.random() * 0.5) * strength;
    let names = crate::render::background::background_variations();
    if names.is_empty() {
        return;
    }
    let name = java::pick(rng, &names);
    let mut v = VariationInstance::new(name, 0.5 + rng.random());
    affine_mutation_xform(&mut xf, strength, rng);
    mutate_variation(&mut v, strength, rng);
    xf.variations.push(v);
    layer.bg_xforms.push(xf);
}

/// `PainterlyStyleMutation.getRandomBrushId`.
pub fn random_brush_id(rng: &mut Rng) -> f64 {
    ((1.0 + rng.random() * 245.0).round() as i32).min(245) as f64
}

/// `BokehMutation.execute`: depth of field with a random bokeh shape and crackle "lights".
pub fn bokeh_mutation(flame: &mut Flame, strength: f64, rng: &mut Rng) {
    flame.cam_dof = (0.1 + rng.random() * 0.3) * strength;
    flame.new_cam_dof = true;
    flame.cam_pitch = 30.0 + rng.random() * 20.0;
    flame.cam_yaw = 15.0 - rng.random() * 30.0;
    flame.cam_bank = 15.0 - rng.random() * 30.0;
    flame.cam_perspective = 0.05 + rng.random() * 0.2;
    flame.cam_dof_area = 0.2 + rng.random() * 0.5;
    for li in 0..flame.layers.len() {
        // An existing crackle with scale 0, else a new transform with one.
        let mut found = None;
        for (xi, xf) in flame.layers[li].xforms.iter_mut().enumerate() {
            if let Some(vi) = (0..xf.variations.len())
                .rev()
                .find(|&i| xf.variations[i].name == "crackle" && xf.variations[i].param("scale").is_some_and(|s| s.abs() < 1.0e-6))
            {
                found = Some((xi, vi));
                xf.color = rng.random();
                break;
            }
        }
        let (xi, vi) = match found {
            Some(f) => f,
            None => {
                let mut xf = java::xform();
                xf.weight = 0.5;
                java::set(java::add(&mut xf, 1.0, "crackle"), &[("scale", 0.0)]);
                flame.layers[li].xforms.push(xf);
                (flame.layers[li].xforms.len() - 1, 0)
            }
        };
        let crackle = &mut flame.layers[li].xforms[xi].variations[vi];
        crackle.amount = 1.0 + rng.random() * 2.0;
        let (distort, cellsize) = (1.5 + rng.random() * 1.5, 0.5 + rng.random() * 2.0);
        java::set(crackle, &[("distort", distort), ("cellsize", cellsize)]);
        if rng.random() < 0.33 {
            flame.focus_x = 0.33 - rng.random() * 0.66;
            flame.focus_y = 0.25 - rng.random() * 0.50;
            flame.focus_z = 0.1 - rng.random() * 0.2;
        } else {
            (flame.focus_x, flame.focus_y, flame.focus_z) = (0.0, 0.0, 0.0);
        }
        flame.cam_dof_scale = 1.5 + rng.random() * 2.0;
        flame.cam_dof_angle = 20.0 * rng.random();
        (flame.cam_dof_param1, flame.cam_dof_param2, flame.cam_dof_param3, flame.cam_dof_param4, flame.cam_dof_param5) = (0.0, 0.0, 0.0, 0.0, 0.0);
        let faded = |rng: &mut Rng| if rng.random() < 0.25 { 0.2 + rng.random() * 0.8 } else { 0.0 };
        let sel = rng.random();
        if sel < 0.08 {
            flame.cam_dof_shape = DofShape::Bubble;
            flame.cam_dof_fade = 0.6 + rng.random() * 0.4;
        } else if sel < 0.16 {
            flame.cam_dof_shape = DofShape::Heart;
            flame.cam_dof_fade = faded(rng);
        } else if sel < 0.20 {
            flame.cam_dof_shape = DofShape::CannabisCurve;
            flame.cam_dof_fade = faded(rng);
        } else if sel < 0.28 {
            flame.cam_dof_shape = DofShape::NBlur;
            flame.cam_dof_fade = 0.0;
            flame.cam_dof_param1 = 3.0 + rng.random() * 5.0;
            if rng.random() < 0.33 {
                flame.cam_dof_param2 = 2.0 + rng.random() * 5.0;
                flame.cam_dof_param3 = 1.0;
                flame.cam_dof_param4 = 0.0;
                flame.cam_dof_param5 = if rng.random() < 0.33 { 1.0 } else { 0.0 };
            }
        } else if sel < 0.36 {
            flame.cam_dof_shape = DofShape::Flower;
            flame.cam_dof_fade = 0.0;
            flame.cam_dof_param1 = 0.3 + rng.random() * 0.2;
            flame.cam_dof_param2 = 5.0 + rng.random() * 5.0;
        } else if sel < 0.44 {
            flame.cam_dof_shape = DofShape::Cloverleaf;
            flame.cam_dof_fade = faded(rng);
        } else if sel < 0.52 {
            flame.cam_dof_shape = DofShape::SineBlur;
            flame.cam_dof_fade = 0.0;
            flame.cam_dof_param1 = 1.2 + rng.random();
        } else if sel < 0.60 {
            flame.cam_dof_shape = DofShape::PerlinNoise;
            flame.cam_dof_fade = 0.0;
            flame.cam_dof_param1 = rng.random();
            flame.cam_dof_param2 = 1.2 + rng.random() * 1.8;
            flame.cam_dof_param3 = 0.1 + rng.random() * 0.4;
        } else if sel < 0.64 {
            flame.cam_dof_fade = 0.2 + rng.random() * 0.8;
            flame.cam_dof_shape = DofShape::StarBlur;
            flame.cam_dof_param1 = 4.0 + rng.random() * 6.0;
            flame.cam_dof_param2 = 0.40162283177245455973959534526548;
        } else if sel < 0.72 {
            flame.cam_dof_angle = 0.0;
            flame.cam_dof_fade = 0.0;
            flame.cam_dof_shape = DofShape::Taurus;
            flame.cam_dof_param1 = 2.5 + rng.random();
            flame.cam_dof_param2 = 4.0 + rng.random() * 3.0;
            flame.cam_dof_param3 = 1.25 * rng.random() * 0.5;
            flame.cam_dof_param4 = 0.9 + rng.random() * 0.2;
        } else if sel < 0.80 {
            flame.cam_dof_angle = 0.0;
            flame.cam_dof_fade = 0.0;
            flame.cam_dof_shape = DofShape::BrushStroke;
            flame.cam_dof_param1 = random_brush_id(rng);
            flame.cam_dof_param2 = if rng.random() > 0.25 { random_brush_id(rng) } else { 0.0 };
            flame.cam_dof_param3 = if rng.random() > 0.75 { random_brush_id(rng) } else { 0.0 };
        } else if sel < 0.92 {
            flame.cam_dof_shape = DofShape::Rect;
            flame.cam_dof_fade = 0.0;
            flame.cam_dof_param1 = 0.4 + rng.random() * 0.4;
        }
    }
}

/// `PainterlyStyleMutation.execute`: a brush stroke variation on the (last) final transform.
pub fn painterly_mutation(layer: &mut Layer, rng: &mut Rng) {
    let is_brush = |v: &VariationInstance| v.name == "brush_stroke_wf" || v.name == "post_brush_stroke_wf";
    let existing = layer.final_xforms.iter().enumerate().find_map(|(xi, xf)| xf.variations.iter().position(is_brush).map(|vi| (xi, vi)));
    let (xi, vi) = match existing {
        Some(found) => found,
        None => {
            if layer.final_xforms.is_empty() {
                let mut xf = java::xform();
                xf.weight = 1.0;
                xf.color = rng.random();
                xf.color_symmetry = rng.random();
                xf.color_type = crate::model::ColorType::Diffusion;
                java::add(&mut xf, 1.0, "linear3D");
                layer.final_xforms.push(xf);
            }
            let name = if rng.random() > 0.35 { "post_brush_stroke_wf" } else { "brush_stroke_wf" };
            let xi = layer.final_xforms.len() - 1;
            let xf = &mut layer.final_xforms[xi];
            java::add(xf, 0.25 + rng.random() * 0.5, name);
            (xi, xf.variations.len() - 1)
        }
    };
    let brush = &mut layer.final_xforms[xi].variations[vi];
    let (blend, size, deform) = (0.1 + 0.2 * rng.random(), 0.015 + 0.02 * rng.random(), 0.05 + 0.1 * rng.random());
    java::set(brush, &[("blend", blend), ("grid_size", size), ("grid_deform", deform)]);
    let count = 1 + (rng.random() * 5.0).round() as usize;
    let mut ids: Vec<String> = Vec::new();
    while ids.len() < count {
        let id = format!("{:03}", 1 + (rng.random() * 244.0).round() as i32);
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    brush.set_resource("brush_list", &ids.join(", "));
}
