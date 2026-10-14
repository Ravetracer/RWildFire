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
use crate::weighting::FieldType;

use super::java::{self, Plane};

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

/// `XFormTransformService.flipHorizontal` in an edit plane.
fn flip_horizontal(xf: &mut XForm, plane: Plane, post: bool) {
    let a = java::coefs(xf, plane, post);
    a.c00 = -a.c00;
    a.c10 = -a.c10;
}

/// `XFormTransformService.flipVertical` in an edit plane.
fn flip_vertical(xf: &mut XForm, plane: Plane, post: bool) {
    let a = java::coefs(xf, plane, post);
    a.c01 = -a.c01;
    a.c11 = -a.c11;
}

/// `AffineMutation.applyToXForm` (and the `apply` of `Affine3DMutation` /
/// `RandomAffineZMutation`'s sibling): either nudges the XY coefficients, or flips,
/// translates, rotates and scales in the edit plane.
fn affine_apply(xf: &mut XForm, amount: f64, plane: Plane, rng: &mut Rng) {
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
            flip_horizontal(xf, plane, post);
        }
        if rng.random() > 0.67 {
            let post = rng.random() < 0.5;
            flip_vertical(xf, plane, post);
        }
        if rng.random() < 0.75 {
            let (dx, dy) = (-amount + 2.0 * rng.random() * amount, -amount + 2.0 * rng.random() * amount);
            let post = rng.random() < 0.5;
            java::global_translate_in(xf, plane, dx, dy, post);
        }
        if rng.random() < 0.5 {
            let deg = -amount * 10.0 + 20.0 * rng.random() * amount;
            let post = rng.random() < 0.5;
            java::rotate_in(xf, plane, deg, post);
        }
        if rng.random() < 0.5 {
            let s = -amount + 2.0 * rng.random() * amount;
            let (sx, sy) = (rng.random() < 0.75, rng.random() < 0.75);
            java::scale_in(xf, plane, s, sx, sy, false);
        }
    }
}

/// `AffineMutation.applyToXForm`.
pub fn affine_mutation_xform(xf: &mut XForm, amount: f64, rng: &mut Rng) {
    affine_apply(xf, amount, Plane::XY, rng);
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
            flame.cam_dof_param2 = 0.401_622_831_772_454_56;
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

/// A random xform: a final one with probability `final_chance` (if there are any).
fn pick_xform<'a>(layer: &'a mut Layer, final_chance: f64, rng: &mut Rng) -> Option<&'a mut XForm> {
    if layer.final_xforms.is_empty() || rng.random() >= final_chance {
        let n = layer.xforms.len();
        if n == 0 {
            return None;
        }
        let i = rng.random_int(n);
        layer.xforms.get_mut(i)
    } else {
        let i = rng.random_int(layer.final_xforms.len());
        layer.final_xforms.get_mut(i)
    }
}

/// A random variation name: from the experimental list (33 %) or any random variation.
fn random_name(rng: &mut Rng, avoid_blur: bool) -> &'static str {
    if rng.random() < 0.33 {
        let exp = super::gens_a::experimental();
        return exp[rng.random_int(exp.len())];
    }
    loop {
        let name = java::random_variation(rng);
        if !avoid_blur || (!name.contains("blur") && !name.contains("Blur") && !name.starts_with("fract_")) {
            return name;
        }
    }
}

/// `AddTransformMutation`: a new transform (sometimes replacing one), or a new final one.
pub fn add_transform(layer: &mut Layer, strength: f64, rng: &mut Rng) {
    let to_final = rng.random() >= 0.75;
    let list = if to_final { &mut layer.final_xforms } else { &mut layer.xforms };
    if rng.random() < 0.5 && !list.is_empty() {
        let i = ((rng.random() * list.len() as f64) as usize).min(list.len() - 1);
        list.remove(i);
    }
    let mut xf = java::xform();
    xf.weight = (0.1 + rng.random() * 2.0) * strength;
    let name = random_name(rng, to_final);
    java::add(&mut xf, (0.01 + rng.random() * 10.0) * strength, name);
    list.push(xf);
}

/// `AddVariationMutation`: adds (and sometimes first removes) a variation of a transform.
pub fn add_variation(layer: &mut Layer, strength: f64, rng: &mut Rng) {
    let Some(xf) = pick_xform(layer, 0.25, rng) else { return };
    if rng.random() < 0.75 {
        if rng.random() < 0.67 && !xf.variations.is_empty() {
            let i = ((rng.random() * xf.variations.len() as f64) as usize).min(xf.variations.len() - 1);
            xf.variations.remove(i);
        }
        let name = random_name(rng, false);
        java::add(xf, (0.01 + rng.random() * 10.0) * strength, name);
    }
}

/// `AffineMutation.execute`: three affine changes of decreasing size.
pub fn affine(layer: &mut Layer, strength: f64, rng: &mut Rng) {
    for f in [0.75, 0.2, 0.1] {
        if let Some(xf) = pick_xform(layer, 0.5, rng) {
            affine_apply(xf, f * strength, Plane::XY, rng);
        }
    }
}

/// `YZ` or `ZX` at random (the 3D mutations' edit planes).
fn plane_3d(rng: &mut Rng) -> Plane {
    if rng.random() < 0.5 { Plane::YZ } else { Plane::ZX }
}

/// `Affine3DMutation`: a 3D camera, and affine changes in the YZ/ZX planes (with a little
/// `post_zscale_wf` to give flat transforms depth).
pub fn affine_3d(flame: &mut Flame, layer: usize, strength: f64, rng: &mut Rng) {
    flame.cam_pitch = 30.0 + rng.random() * 20.0;
    flame.cam_yaw = 15.0 - rng.random() * 30.0;
    flame.cam_bank = 15.0 - rng.random() * 30.0;
    flame.cam_perspective = 0.05 + rng.random() * 0.2;
    flame.cam_dof_area = 0.2 + rng.random() * 0.5;
    if !flame.preserve_z && rng.random() > 0.33 {
        flame.preserve_z = true;
    }
    let preserve_z = flame.preserve_z;
    for f in [0.5, 0.2, 0.1] {
        let plane = plane_3d(rng);
        let Some(xf) = pick_xform(&mut flame.layers[layer], 0.5, rng) else { continue };
        if preserve_z && rng.random() > 0.25 && !xf.variations.iter().any(|v| v.name == "post_zscale_wf") {
            java::add(xf, 0.001 + rng.random() * 0.1, "post_zscale_wf");
        }
        affine_apply(xf, f * strength, plane, rng);
    }
}

/// `RandomAffineZMutation`: like the 3D affine mutation, but smaller and keeping an existing
/// 3D camera.
pub fn random_affine_z(flame: &mut Flame, layer: usize, strength: f64, rng: &mut Rng) {
    if flame.cam_pitch.abs() < 2.5 && flame.cam_bank.abs() < 2.5 {
        flame.cam_pitch = 30.0 + rng.random() * 20.0;
        flame.cam_yaw = 15.0 - rng.random() * 30.0;
        flame.cam_bank = 15.0 - rng.random() * 30.0;
        if rng.random() < 0.5 {
            flame.cam_perspective = 0.05 + rng.random() * 0.2;
            flame.cam_dof_area = 0.2 + rng.random() * 0.5;
        }
    }
    if !flame.preserve_z && rng.random() > 0.33 {
        flame.preserve_z = true;
    }
    for f in [0.2, 0.1, 0.05] {
        let amount = f * strength;
        let plane = plane_3d(rng);
        let Some(xf) = pick_xform(&mut flame.layers[layer], 0.25, rng) else { continue };
        if rng.random() < 0.75 {
            let (dx, dy) = (-amount + 2.0 * rng.random() * amount, -amount + 2.0 * rng.random() * amount);
            let post = rng.random() < 0.25;
            java::global_translate_in(xf, plane, dx, dy, post);
        }
        if rng.random() < 0.5 {
            let deg = -amount * 10.0 + 20.0 * rng.random() * amount;
            let post = rng.random() < 0.25;
            java::rotate_in(xf, plane, deg, post);
        }
        if rng.random() < 0.5 {
            let s = -amount + 2.0 * rng.random() * amount;
            let (sx, sy, post) = (rng.random() < 0.75, rng.random() < 0.75, rng.random() < 0.25);
            java::scale_in(xf, plane, s, sx, sy, post);
        }
    }
}

/// `ChangeWeightMutation`: toggles one xaos weight of every transform.
pub fn change_weight(layer: &mut Layer, rng: &mut Rng) {
    let n = layer.xforms.len();
    for xf in &mut layer.xforms {
        let idx = ((rng.random() * n as f64) as usize).min(n.saturating_sub(1));
        let w = xf.chaos_to(idx);
        java::set_xaos(xf, idx, if w > 0.0 { 0.0 } else { w + 1.0 });
    }
}

/// `ColorTypeMutation`: random color types (and target colors).
pub fn color_type(layer: &mut Layer, rng: &mut Rng) {
    use crate::model::ColorType::*;
    const XFORMS: [crate::model::ColorType; 12] =
        [Diffusion, Unset, Distance, Target, Unset, Diffusion, TargetG, Unset, None, Unset, Unset, Diffusion];
    const FINALS: [crate::model::ColorType; 18] =
        [None, Diffusion, None, Unset, Unset, Distance, None, Target, Unset, Diffusion, Unset, Unset, None, TargetG, Unset, Unset, Diffusion, None];
    let apply = |list: &mut Vec<XForm>, types: &[crate::model::ColorType], rng: &mut Rng| {
        for xf in list {
            let t = types[((rng.random() * types.len() as f64) as usize).min(types.len() - 1)];
            xf.color_type = t;
            if t == Target {
                xf.target_color = [(rng.random() * 256.0) as u8, (rng.random() * 256.0) as u8, (rng.random() * 256.0) as u8];
            }
        }
    };
    apply(&mut layer.xforms, &XFORMS, rng);
    apply(&mut layer.final_xforms, &FINALS, rng);
}

/// `GradientPositionMutation`: new transform colors, or a shifted gradient.
pub fn gradient_position(layer: &mut Layer, rng: &mut Rng) {
    if rng.random() < 0.5 {
        for xf in &mut layer.xforms {
            xf.color = rng.random();
            if rng.random() < 0.25 {
                xf.color_symmetry = rng.random();
            }
        }
    } else {
        layer.gradient.shift(-256 + rng.random_int(512) as i32);
    }
}

/// `RandomBGColorMutation`: a background (single color or corner gradient) from the colors
/// of a random gradient.
pub fn random_bg_color(flame: &mut Flame, rng: &mut Rng) {
    use crate::model::BgColorType;
    let palette = super::gradients::random_gradient(rng).colors;
    let n = palette.len();
    let pick = |rng: &mut Rng, span: usize| ((rng.random() * span as f64) as usize).min(span - 1);
    let mode = rng.random();
    let bg = &mut flame.background;
    if mode < 0.5 {
        bg.kind = BgColorType::SingleColor;
        bg.color = palette[pick(rng, n)];
    } else {
        bg.kind = if mode < 0.8 { BgColorType::Gradient2x2 } else { BgColorType::Gradient2x2C };
        let i = pick(rng, n - 2);
        (bg.ll, bg.lr) = (palette[i], palette[i + 1]);
        let i = pick(rng, n - 2);
        (bg.ul, bg.ur) = (palette[i], palette[i + 1]);
        if mode >= 0.8 {
            bg.cc = palette[pick(rng, n)];
        }
    }
}

/// `RandomFlameMutation`: replaces the layer with one of a new random flame.
pub fn random_flame(layer: &mut Layer, rng: &mut Rng) {
    let flame = super::sample(super::Generator::All, rng, super::Quality::NORMAL, false, false, super::Extras::default());
    if let Some(l) = flame.layers.into_iter().next() {
        *layer = l;
    }
}

/// `WeightingFieldMutation`: random weighting fields on some transforms (the heavier ones
/// more likely), none on the others.
pub fn weighting_field(layer: &mut Layer, strength: f64, rng: &mut Rng) {
    weighting_field_of(layer, strength, &DEFAULT_FIELD_TYPES, rng);
}

/// [`weighting_field`] with field types chosen from `types` (`setWeightingFieldTypeList`).
pub fn weighting_field_of(layer: &mut Layer, strength: f64, types: &[FieldType], rng: &mut Rng) {
    let n = layer.xforms.len();
    if n == 0 {
        return;
    }
    let mut chosen = vec![false; n];
    if n == 1 {
        chosen[0] = true;
    } else {
        let max_weight = layer.xforms.iter().map(|x| x.weight).fold(0.0, f64::max);
        let mut count = 0;
        while count < 1 && max_weight > 0.0 {
            for (i, xf) in layer.xforms.iter().enumerate() {
                if xf.weight / max_weight * rng.random() > 0.25 {
                    chosen[i] = true;
                    count += 1;
                    if count > n / 2 {
                        break;
                    }
                }
            }
        }
    }
    for (xf, chosen) in layer.xforms.iter_mut().zip(chosen) {
        if chosen {
            random_weighting_field_of(xf, strength, types, rng);
        } else {
            xf.weighting.field_type = crate::weighting::FieldType::None;
        }
    }
}

/// `DEFAULT_WEIGHTING_FIELD_TYPE_LIST` (repeated entries are chosen more often).
pub const DEFAULT_FIELD_TYPES: [FieldType; 19] = [
    FieldType::CellularNoise,
    FieldType::CellularNoise,
    FieldType::CellularNoise,
    FieldType::CubicNoise,
    FieldType::CubicFractalNoise,
    FieldType::CubicFractalNoise,
    FieldType::PerlinNoise,
    FieldType::PerlinFractalNoise,
    FieldType::PerlinFractalNoise,
    FieldType::PerlinFractalNoise,
    FieldType::PerlinFractalNoise,
    FieldType::PerlinFractalNoise,
    FieldType::WhiteNoise,
    FieldType::SimplexNoise,
    FieldType::SimplexFractalNoise,
    FieldType::SimplexFractalNoise,
    FieldType::ValueNoise,
    FieldType::ValueFractalNoise,
    FieldType::ValueFractalNoise,
];

/// `WeightingFieldMutation.applyRandomWeightField`.
pub fn random_weighting_field(xf: &mut XForm, strength: f64, rng: &mut Rng) {
    random_weighting_field_of(xf, strength, &DEFAULT_FIELD_TYPES, rng);
}

/// [`random_weighting_field`] with the field type chosen from `types`.
pub fn random_weighting_field_of(xf: &mut XForm, strength: f64, types: &[FieldType], rng: &mut Rng) {
    use crate::fastnoise::{CellularDistance, CellularReturn, FractalType};
    use crate::weighting::{FieldInput, FieldType::*, VarParamWeight};
    let pick = |rng: &mut Rng, n: usize| ((rng.random() * n as f64) as usize).min(n - 1);
    let field_type = types[pick(rng, types.len())];
    let names: Vec<(String, Vec<&'static str>)> = xf
        .variations
        .iter()
        .map(|v| (v.name.clone(), variations::find(&v.name).map_or(Vec::new(), |d| d.params.iter().map(|p| p.name).collect())))
        .collect();
    let w = &mut xf.weighting;
    w.field_type = field_type;
    w.input = if rng.random() < 0.25 { FieldInput::Position } else { FieldInput::Affine };
    if rng.random() > 0.42 {
        w.color_intensity = 0.5 - rng.random();
    }
    if rng.random() > 0.65 {
        w.jitter_intensity = if rng.random() > 0.65 { 0.5 - rng.random() } else { 0.25 - 0.5 * rng.random() };
    }
    w.var_params = Default::default();
    let intensity = |rng: &mut Rng| if rng.random() > 0.33 { (0.05 + rng.random() * 0.2) * strength } else { (0.25 - rng.random() * 0.5) * strength };
    let mut has_var_param = false;
    for (name, params) in &names {
        if !params.is_empty() && rng.random() > 0.33 {
            let idx = pick(rng, params.len());
            w.var_params[0] = VarParamWeight { intensity: intensity(rng), var_name: name.clone(), param_name: params[idx].to_string() };
            if params.len() > 2 {
                let mut idx2 = idx;
                while idx2 == idx {
                    idx2 = pick(rng, params.len());
                }
                w.var_params[1] = VarParamWeight { intensity: intensity(rng), var_name: name.clone(), param_name: params[idx2].to_string() };
            }
            has_var_param = true;
        }
    }
    if !has_var_param && names.len() > 1 && rng.random() > 0.5 {
        let idx = pick(rng, names.len());
        w.var_params[0] = VarParamWeight { intensity: intensity(rng), var_name: names[idx].0.clone(), param_name: "amount".into() };
        has_var_param = true;
    }
    w.var_amount_intensity = if (has_var_param && rng.random() > 0.66) || (!has_var_param && rng.random() > 0.33) {
        if rng.random() > 0.33 { (0.05 + rng.random() * 0.5) * strength } else { (0.25 - rng.random() * 0.5) * strength }
    } else {
        (0.01 + rng.random() * 0.15) * strength
    };
    let seed_and_frequency = |w: &mut crate::weighting::WeightingField, rng: &mut Rng| {
        w.noise_seed = 1 + (rng.random() * 30000.0) as i32;
        w.noise_frequency = 0.75 + rng.random() * 3.0;
    };
    match field_type {
        CellularNoise => {
            seed_and_frequency(w, rng);
            use CellularDistance::*;
            const DIST: [CellularDistance; 4] = [Euclidean, Manhattan, Euclidean, Natural];
            w.cellular_distance = DIST[pick(rng, DIST.len())];
            use CellularReturn::*;
            const RET: [CellularReturn; 14] = [
                CellValue,
                Distance2,
                Distance2,
                Distance2,
                Distance,
                Distance2Add,
                Distance2Add,
                Distance2Div,
                Distance2,
                Distance2Div,
                Distance2Mul,
                Distance2Mul,
                Distance2Div,
                Distance2Div,
            ];
            w.cellular_return = RET[pick(rng, RET.len())];
        }
        CubicNoise | PerlinNoise | SimplexNoise | ValueNoise => seed_and_frequency(w, rng),
        CubicFractalNoise | PerlinFractalNoise | SimplexFractalNoise | ValueFractalNoise => {
            seed_and_frequency(w, rng);
            w.fractal_octaves = 2 + (rng.random() * 4.0) as i32;
            w.fractal_lacunarity = 1.25 + rng.random() * 3.0;
            w.fractal_gain = 0.2 + rng.random() * 0.75;
            use FractalType::*;
            const FT: [FractalType; 5] = [Fbm, Billow, Fbm, RigidMulti, Fbm];
            w.fractal_type = FT[pick(rng, FT.len())];
        }
        _ => {}
    }
}

/// JWildfire's MutaGen mutation types (`MutationType`, without the user-defined lists).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MutationType {
    All,
    AddTransform,
    AddVariation,
    Affine,
    Affine3D,
    Bokeh,
    ChangeWeight,
    ColorType,
    GradientPosition,
    LocalGamma,
    PainterlyStyle,
    RandomBackground,
    RandomBgColor,
    RandomFlame,
    RandomGradient,
    RandomParameter,
    RandomZTransform,
    SimilarGradient,
    WeightingField,
}

impl MutationType {
    pub const ALL: &'static [MutationType] = &[
        MutationType::All,
        MutationType::AddTransform,
        MutationType::AddVariation,
        MutationType::Affine,
        MutationType::Affine3D,
        MutationType::Bokeh,
        MutationType::ChangeWeight,
        MutationType::ColorType,
        MutationType::GradientPosition,
        MutationType::LocalGamma,
        MutationType::PainterlyStyle,
        MutationType::RandomBackground,
        MutationType::RandomBgColor,
        MutationType::RandomFlame,
        MutationType::RandomGradient,
        MutationType::RandomParameter,
        MutationType::RandomZTransform,
        MutationType::SimilarGradient,
        MutationType::WeightingField,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MutationType::All => "All",
            MutationType::AddTransform => "Add transform",
            MutationType::AddVariation => "Add variation",
            MutationType::Affine => "Affine",
            MutationType::Affine3D => "Affine 3D",
            MutationType::Bokeh => "Bokeh",
            MutationType::ChangeWeight => "Change weight",
            MutationType::ColorType => "Color type",
            MutationType::GradientPosition => "Gradient position",
            MutationType::LocalGamma => "Local gamma",
            MutationType::PainterlyStyle => "Painterly style",
            MutationType::RandomBackground => "Random background",
            MutationType::RandomBgColor => "Random background color",
            MutationType::RandomFlame => "Random flame",
            MutationType::RandomGradient => "Random gradient",
            MutationType::RandomParameter => "Random parameter",
            MutationType::RandomZTransform => "Random Z transform",
            MutationType::SimilarGradient => "Similar gradient",
            MutationType::WeightingField => "Weighting field",
        }
    }

    /// `AllMutation`: a random type (duplicates weight the choice).
    fn resolve(self, rng: &mut Rng) -> MutationType {
        if self != MutationType::All {
            return self;
        }
        use MutationType::*;
        const LIST: [MutationType; 46] = [
            Bokeh,
            AddTransform,
            Affine,
            ChangeWeight,
            RandomGradient,
            PainterlyStyle,
            Affine3D,
            LocalGamma,
            ColorType,
            RandomParameter,
            Affine,
            WeightingField,
            AddTransform,
            LocalGamma,
            SimilarGradient,
            RandomBgColor,
            RandomBackground,
            Affine,
            ChangeWeight,
            Affine3D,
            AddVariation,
            ChangeWeight,
            WeightingField,
            AddTransform,
            LocalGamma,
            PainterlyStyle,
            ColorType,
            Affine,
            RandomParameter,
            Affine3D,
            Bokeh,
            SimilarGradient,
            GradientPosition,
            ColorType,
            LocalGamma,
            RandomBgColor,
            RandomBackground,
            AddTransform,
            Affine,
            PainterlyStyle,
            ChangeWeight,
            Affine,
            WeightingField,
            ChangeWeight,
            Affine3D,
            RandomParameter,
        ];
        LIST[rng.random_int(LIST.len())]
    }

    /// `Mutation.execute(layer, strength)` on layer `layer` of `flame`.
    pub fn execute(self, flame: &mut Flame, layer: usize, strength: f64, rng: &mut Rng) {
        if layer >= flame.layers.len() {
            return;
        }
        let kind = self.resolve(rng);
        let l = &mut flame.layers[layer];
        match kind {
            MutationType::All => {}
            MutationType::AddTransform => add_transform(l, strength, rng),
            MutationType::AddVariation => add_variation(l, strength, rng),
            MutationType::Affine => affine(l, strength, rng),
            MutationType::Affine3D => affine_3d(flame, layer, strength, rng),
            MutationType::Bokeh => bokeh_mutation(flame, strength, rng),
            MutationType::ChangeWeight => change_weight(l, rng),
            MutationType::ColorType => color_type(l, rng),
            MutationType::GradientPosition => gradient_position(l, rng),
            MutationType::LocalGamma => local_gamma_mutation(l, rng),
            MutationType::PainterlyStyle => painterly_mutation(l, rng),
            MutationType::RandomBackground => random_background_mutation(l, strength, rng),
            MutationType::RandomBgColor => random_bg_color(flame, rng),
            MutationType::RandomFlame => random_flame(l, rng),
            MutationType::RandomGradient => l.gradient = super::gradients::random_gradient(rng),
            MutationType::RandomParameter => random_param_mutation(l, strength, rng),
            MutationType::RandomZTransform => random_affine_z(flame, layer, strength, rng),
            MutationType::SimilarGradient => l.gradient = super::gradients::similar_gradient(&l.gradient, rng),
            MutationType::WeightingField => weighting_field(l, strength, rng),
        }
    }
}

/// Applies a mutation to every visible layer (`MutaGenController.modifyFlame`).
pub fn mutate_flame(flame: &mut Flame, kind: MutationType, strength: f64, rng: &mut Rng) {
    for i in 0..flame.layers.len() {
        if flame.layers[i].visible {
            kind.execute(flame, i, strength, rng);
        }
    }
}
