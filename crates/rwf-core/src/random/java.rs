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

//! Building blocks of the ported JWildfire random flame generators, with JWildfire's
//! semantics (`XFormTransformService`, `VariationFuncList`, `new XForm()`).

use std::sync::OnceLock;

use crate::model::{Affine, Flame, VariationInstance, XForm};
use crate::rng::Rng;
use crate::variations;

const SMALL_EPSILON: f64 = 1.0e-6;

/// `new XForm()`: identity affine and, unlike [`XForm::default`], weight 0.
pub fn xform() -> XForm {
    XForm { weight: 0.0, ..XForm::default() }
}

/// `new Flame()` (800 × 600, 50 pixels per unit), as the generators expect it.
pub fn flame() -> Flame {
    Flame::default()
}

/// Edit plane of `XFormTransformService` (JWildfire `EditPlane`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Plane {
    XY,
    YZ,
    ZX,
}

fn coefs(xf: &mut XForm, plane: Plane, post: bool) -> &mut Affine {
    match (plane, post) {
        (Plane::XY, false) => &mut xf.coefs,
        (Plane::XY, true) => &mut xf.post,
        (Plane::YZ, false) => &mut xf.yz_coefs,
        (Plane::YZ, true) => &mut xf.yz_post,
        (Plane::ZX, false) => &mut xf.zx_coefs,
        (Plane::ZX, true) => &mut xf.zx_post,
    }
}

/// `XFormTransformService.rotate` in an edit plane: `M * R(angle)`.
pub fn rotate_in(xf: &mut XForm, plane: Plane, deg: f64, post: bool) {
    if deg.abs() < SMALL_EPSILON {
        return;
    }
    let (s, c) = (deg * std::f64::consts::PI / 180.0).sin_cos();
    let a = coefs(xf, plane, post);
    let (c00, c01, c10, c11) = (a.c00, a.c01, a.c10, a.c11);
    a.c00 = c00 * c + c01 * s;
    a.c01 = -c00 * s + c01 * c;
    a.c10 = c10 * c + c11 * s;
    a.c11 = -c10 * s + c11 * c;
}

/// `XFormTransformService.rotate(xForm, angle, post)`.
pub fn rotate(xf: &mut XForm, deg: f64, post: bool) {
    rotate_in(xf, Plane::XY, deg, post);
}

/// `XFormTransformService.scale` in an edit plane: scales the x and/or y axis vector.
pub fn scale_in(xf: &mut XForm, plane: Plane, s: f64, x: bool, y: bool, post: bool) {
    if (s - 1.0).abs() < SMALL_EPSILON {
        return;
    }
    let a = coefs(xf, plane, post);
    if x {
        a.c00 *= s;
        a.c01 *= s;
    }
    if y {
        a.c10 *= s;
        a.c11 *= s;
    }
}

/// `XFormTransformService.scale(xForm, scale, scaleX, scaleY, post)`.
pub fn scale(xf: &mut XForm, s: f64, x: bool, y: bool, post: bool) {
    scale_in(xf, Plane::XY, s, x, y, post);
}

/// `XFormTransformService.localTranslate`: `M * T(dx, dy)`.
pub fn local_translate(xf: &mut XForm, dx: f64, dy: f64, post: bool) {
    if dx.abs() < SMALL_EPSILON && dy.abs() < SMALL_EPSILON {
        return;
    }
    let a = coefs(xf, Plane::XY, post);
    a.c20 += a.c00 * dx + a.c01 * dy;
    a.c21 += a.c10 * dx + a.c11 * dy;
}

/// `XFormTransformService.globalTranslate`.
pub fn global_translate(xf: &mut XForm, dx: f64, dy: f64, post: bool) {
    if dx.abs() < SMALL_EPSILON && dy.abs() < SMALL_EPSILON {
        return;
    }
    let a = coefs(xf, Plane::XY, post);
    a.c20 += dx;
    a.c21 += dy;
}

/// `layer.randomizeColors()`.
pub fn randomize_colors(layer: &mut crate::model::Layer, rng: &mut Rng) {
    for xf in &mut layer.xforms {
        xf.color = rng.random();
    }
}

/// `layer.distributeColors()`.
pub fn distribute_colors(layer: &mut crate::model::Layer) {
    let n = layer.xforms.len();
    if n > 1 {
        for (i, xf) in layer.xforms.iter_mut().enumerate() {
            xf.color = i as f64 / (n - 1) as f64;
        }
    }
}

/// `layer.randomizeColorSpeed()`.
pub fn randomize_color_speed(layer: &mut crate::model::Layer, rng: &mut Rng) {
    for xf in &mut layer.xforms {
        xf.color_symmetry = rng.random();
    }
}

/// `xForm.addVariation(amount, VariationFuncList.getVariationFuncInstance(name))`: the
/// variation with its default parameters; returns it for `setParameter` calls.
pub fn add<'a>(xf: &'a mut XForm, amount: f64, name: &str) -> &'a mut VariationInstance {
    xf.variations.push(VariationInstance::new(name, amount));
    xf.variations.last_mut().expect("just pushed")
}

/// `func.setParameter(name, value)` for a chain of parameters.
pub fn set(v: &mut VariationInstance, params: &[(&str, f64)]) {
    for (k, val) in params {
        v.set_param(k, *val);
    }
}

/// `xForm.getModifiedWeights()[to] = value` (xaos).
pub fn set_xaos(xf: &mut XForm, to: usize, value: f64) {
    if xf.chaos.len() <= to {
        xf.chaos.resize(to + 1, 1.0);
    }
    xf.chaos[to] = value;
}

/// `VariationFuncList.filterVariations`: the names RWildFire knows.
pub fn filter(names: &[&'static str]) -> Vec<&'static str> {
    names.iter().copied().filter(|n| variations::find(n).is_some()).collect()
}

/// `(int) (Math.random() * n)` as a list index.
pub fn pick<'a>(rng: &mut Rng, list: &[&'a str]) -> &'a str {
    list[((rng.random() * list.len() as f64) as usize).min(list.len() - 1)]
}

/// `Tools.randomInt(n)`.
pub fn rand_int(rng: &mut Rng, n: usize) -> usize {
    rng.random_int(n.max(1))
}

/// Names `VariationFuncList.getRandomVariationname()` chooses from in GPU mode: all
/// variations with GPU support except internal, pre/post, inflate, colorscale, flatten,
/// SVG and resource heavy ones (`isValidRandomVariation`).
fn random_variation_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| {
        variations::all()
            .iter()
            .filter(|d| d.supports_gpu())
            .map(|d| d.name)
            .filter(|n| {
                !n.starts_with('_')
                    && !n.starts_with("inflate")
                    && !n.starts_with("post_")
                    && !n.starts_with("pre_")
                    && !n.starts_with("prepost_")
                    && !n.contains("colorscale")
                    && !matches!(*n, "svg_wf" | "iflames_wf" | "flatten" | "subflame_wf" | "text_wf")
                    && variations::resources_of(n).is_none()
            })
            .collect()
    })
}

/// `VariationFuncList.getRandomVariationname()`.
pub fn random_variation(rng: &mut Rng) -> &'static str {
    pick(rng, random_variation_names())
}

/// `VariationFuncList.getRandomVariationname(VARTYPE_2D / VARTYPE_3D)`.
pub fn random_variation_of(rng: &mut Rng, three_d: bool) -> &'static str {
    static NAMES: OnceLock<[Vec<&'static str>; 2]> = OnceLock::new();
    let lists = NAMES.get_or_init(|| {
        let split =
            |is_3d: bool| random_variation_names().iter().copied().filter(|n| variations::find(n).is_some_and(|d| d.is_3d == is_3d)).collect();
        [split(false), split(true)]
    });
    pick(rng, &lists[three_d as usize])
}

/// `getRandomVariationName` of several generators: a random 2D (25 %) or 3D variation that
/// is no fractal formula, inflate, pre/post or flatten variation.
pub fn random_variation_no_fract(rng: &mut Rng, two_d_or_3d: bool) -> &'static str {
    loop {
        let name = if two_d_or_3d {
            let three_d = rng.random() <= 0.75;
            random_variation_of(rng, three_d)
        } else {
            random_variation(rng)
        };
        if !name.starts_with("fract") && !name.starts_with("inflate") && !name.starts_with("pre_") && !name.starts_with("post_") && name != "flatten"
        {
            return name;
        }
    }
}

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
pub fn set_random_flame_property(layer: &mut crate::model::Layer, amount: f64, rng: &mut Rng) {
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
pub fn random_param_mutation(layer: &mut crate::model::Layer, strength: f64, rng: &mut Rng) {
    for f in [6.0, 5.0, 2.0] {
        let amount = f * (0.25 + 0.75 * rng.random()) * strength;
        set_random_flame_property(layer, amount, rng);
    }
}

/// `LocalGammaMutation.execute` (`XForm.randomizeModColorEffects` on every transform).
pub fn local_gamma_mutation(layer: &mut crate::model::Layer, rng: &mut Rng) {
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

/// Brings a generated flame to RWildFire's 720 pixel high frame, keeping its vertical
/// extent: JWildfire's default 800 × 600 becomes 1280 × 720 (a bit more is visible at the
/// sides), other sizes keep their aspect ratio.
pub fn to_widescreen(flame: &mut Flame) {
    let h = flame.height.max(1) as f64;
    flame.pixels_per_unit *= 720.0 / h;
    flame.width = if (flame.width, flame.height) == (800, 600) { 1280 } else { (flame.width as f64 * 720.0 / h).round() as u32 };
    flame.height = 720;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_match_xform_transform_service() {
        // Values computed with JWildfire's XFormTransformService.
        let mut xf = xform();
        rotate(&mut xf, 30.0, false);
        local_translate(&mut xf, 0.5, -0.25, false);
        scale(&mut xf, 0.5, true, true, false);
        let a = xf.coefs;
        let (s, c) = (30f64.to_radians().sin(), 30f64.to_radians().cos());
        assert!((a.c00 - 0.5 * c).abs() < 1e-12 && (a.c01 + 0.5 * s).abs() < 1e-12);
        assert!((a.c10 - 0.5 * s).abs() < 1e-12 && (a.c11 - 0.5 * c).abs() < 1e-12);
        assert!((a.c20 - (c * 0.5 - s * -0.25)).abs() < 1e-12 && (a.c21 - (s * 0.5 + c * -0.25)).abs() < 1e-12);
        assert!(!random_variation_names().is_empty() && !random_variation_names().contains(&"pre_blur"));
    }
}
