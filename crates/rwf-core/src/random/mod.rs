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

//! Random flame generators and mutations, inspired by JWildfire's random flame generators.

use crate::gradient::Gradient;
use crate::model::{Flame, Layer, PostSymmetryType, VariationInstance, XForm};
use crate::render::analysis::{auto_frame, coverage};
use crate::rng::Rng;
use crate::variations;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Generator {
    Classic,
    Linear,
    Bubbles,
    Julians,
    Spirals,
    Gnarl,
    Symmetric,
    Synth,
}

impl Generator {
    pub const ALL: &'static [Generator] = &[
        Generator::Classic,
        Generator::Linear,
        Generator::Bubbles,
        Generator::Julians,
        Generator::Spirals,
        Generator::Gnarl,
        Generator::Symmetric,
        Generator::Synth,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Generator::Classic => "Classic",
            Generator::Linear => "Linear",
            Generator::Bubbles => "Bubbles",
            Generator::Julians => "Julians",
            Generator::Spirals => "Spirals",
            Generator::Gnarl => "Gnarl",
            Generator::Symmetric => "Symmetric",
            Generator::Synth => "Wild mix",
        }
    }
}

/// Variations that tend to produce nice results in random flames.
const NICE: &[&str] = &[
    "spherical", "julia", "swirl", "horseshoe", "polar", "handkerchief", "heart", "disc", "spiral", "hyperbolic",
    "diamond", "ex", "bent", "waves", "fisheye", "popcorn", "exponential", "power", "cosine", "rings", "fan",
    "blob", "pdj", "fan2", "rings2", "eyefish", "bubble", "cylinder", "julian", "juliascope", "curl", "arch",
    "tangent", "rays", "blade", "secant2", "twintrian", "cross", "disc2", "super_shape", "flower", "conic",
    "parabola", "bipolar", "butterfly", "edisc", "elliptic", "escher", "foci", "loonie", "polar2", "scry",
    "split", "wedge_julia", "wedge_sph", "whorl", "waves2", "exp", "log", "sin", "cos", "tan", "sinh",
    "cosh", "auger", "flux", "mobius", "spherical3D", "julia3D", "hemisphere", "epispiral", "bwraps",
    "hypertile1", "eclipse", "cpow", "lazysusan", "ngon", "pie", "sec", "csc", "cot",
];

fn available(names: &[&'static str]) -> Vec<&'static str> {
    names.iter().copied().filter(|n| variations::find(n).is_some()).collect()
}

fn pick<'a>(rng: &mut Rng, list: &[&'a str]) -> &'a str {
    list[rng.random_int(list.len())]
}

fn random_affine(xf: &mut XForm, rng: &mut Rng, scl: f64) {
    xf.coefs.rotate(rng.range(-360.0, 360.0));
    let (dx, dy) = (rng.random() - 1.0, rng.random() - 1.0);
    // Translate in the transform's local coordinate system.
    let (tx, ty) = (xf.coefs.c00 * dx + xf.coefs.c10 * dy, xf.coefs.c01 * dx + xf.coefs.c11 * dy);
    xf.coefs.translate(tx, ty);
    xf.coefs.scale(scl);
}

fn random_affine_r(xf: &mut XForm, rng: &mut Rng, lo: f64, hi: f64) {
    let scl = rng.range(lo, hi);
    random_affine(xf, rng, scl);
}

fn add_var(xf: &mut XForm, name: &str, amount: f64, rng: &mut Rng) {
    let mut v = VariationInstance::new(name, amount);
    // Lightly randomize parameters around their defaults.
    if let Some(def) = variations::find(name) {
        for (i, p) in def.params.iter().enumerate() {
            if rng.chance(0.5) {
                let span = (p.max - p.min).abs().max(1e-6) * 0.25;
                let mut val = p.default + rng.range(-span, span);
                if p.integer {
                    val = val.round();
                    if val == 0.0 {
                        val = p.default;
                    }
                }
                v.params[i] = val;
            }
        }
    }
    xf.variations.push(v);
}

fn base_flame(rng: &mut Rng) -> Flame {
    let mut flame = Flame { width: 1280, height: 720, pixels_per_unit: 200.0, bg_transparency: false, ..Flame::default() };
    flame.layers = vec![Layer { gradient: Gradient::random(rng), ..Layer::default() }];
    flame
}

fn generate_raw(generator: Generator, rng: &mut Rng) -> Flame {
    let nice = available(NICE);
    let mut flame = base_flame(rng);
    let layer = &mut flame.layers[0];
    match generator {
        Generator::Classic | Generator::Synth => {
            let n = 2 + rng.random_int(4);
            let mut scl = 1.0;
            for _ in 0..n {
                let mut xf = XForm::default();
                scl *= 0.75 + rng.random() / 4.0;
                random_affine(&mut xf, rng, scl);
                xf.color = rng.random();
                xf.weight = rng.random() * 0.9 + 0.1;
                add_var(&mut xf, "linear3D", rng.random() * 0.3 + 0.2, rng);
                let extra = if generator == Generator::Synth { 1 + rng.random_int(3) } else { usize::from(rng.chance(0.9)) };
                for _ in 0..extra {
                    let name = pick(rng, &nice);
                    add_var(&mut xf, name, 0.2 + rng.random() * 0.6, rng);
                }
                layer.xforms.push(xf);
            }
            if rng.chance(0.3) {
                let mut f = XForm::default();
                add_var(&mut f, pick(rng, &nice), 1.0, rng);
                layer.final_xforms.push(f);
            }
        }
        Generator::Linear => {
            let n = 3 + rng.random_int(3);
            for _ in 0..n {
                let mut xf = XForm::default();
                random_affine_r(&mut xf, rng, 0.4, 0.8);
                xf.color = rng.random();
                xf.weight = rng.range(0.2, 1.0);
                add_var(&mut xf, "linear3D", 1.0, rng);
                layer.xforms.push(xf);
            }
            let mut f = XForm::default();
            let finals = available(&["spherical", "julia", "bubble", "swirl", "polar", "disc", "eyefish"]);
            add_var(&mut f, pick(rng, &finals), 1.0, rng);
            layer.final_xforms.push(f);
        }
        Generator::Bubbles => {
            let n = 2 + rng.random_int(3);
            let others = available(&["bubble", "linear3D", "julia", "spherical3D"]);
            for i in 0..n {
                let mut xf = XForm::default();
                random_affine_r(&mut xf, rng, 0.3, 0.9);
                xf.color = rng.random();
                xf.weight = rng.range(0.3, 1.0);
                let name = if i == 0 { "spherical" } else { pick(rng, &others) };
                add_var(&mut xf, name, rng.range(0.5, 1.0), rng);
                layer.xforms.push(xf);
            }
        }
        Generator::Julians => {
            let n = 2 + rng.random_int(3);
            let blend = available(&["linear3D", "spherical", "bubble", "disc"]);
            for _ in 0..n {
                let mut xf = XForm::default();
                random_affine_r(&mut xf, rng, 0.5, 1.2);
                xf.color = rng.random();
                xf.weight = rng.range(0.2, 1.0);
                let mut v = VariationInstance::new("julian", rng.range(0.5, 1.0));
                v.set_param("power", (2 + rng.random_int(8)) as f64 * if rng.chance(0.3) { -1.0 } else { 1.0 });
                v.set_param("dist", rng.range(-1.0, 2.0));
                xf.variations.push(v);
                if rng.chance(0.5) {
                    add_var(&mut xf, pick(rng, &blend), rng.range(0.05, 0.4), rng);
                }
                layer.xforms.push(xf);
            }
        }
        Generator::Spirals => {
            let n = 2 + rng.random_int(2);
            for i in 0..n {
                let mut xf = XForm::default();
                if i == 0 {
                    xf.coefs.rotate(rng.range(5.0, 45.0) * if rng.chance(0.5) { -1.0 } else { 1.0 });
                    xf.coefs.scale(rng.range(0.85, 0.98));
                    xf.weight = rng.range(5.0, 15.0);
                } else {
                    random_affine_r(&mut xf, rng, 0.1, 0.5);
                    xf.weight = rng.range(0.3, 1.0);
                }
                xf.color = rng.random();
                xf.color_symmetry = if i == 0 { 0.9 } else { 0.0 };
                let name = if i == 0 { "linear3D" } else { pick(rng, &nice) };
                add_var(&mut xf, name, 1.0, rng);
                layer.xforms.push(xf);
            }
        }
        Generator::Gnarl => {
            let mut xf = XForm::default();
            xf.coefs.rotate(rng.range(-30.0, 30.0));
            xf.coefs.scale(rng.range(0.9, 1.0));
            xf.weight = 1.0;
            xf.color = rng.random();
            xf.color_symmetry = 0.95;
            let mut v = VariationInstance::new("waves2", 1.0);
            v.set_param("scalex", rng.range(-0.1, 0.1));
            v.set_param("scaley", rng.range(-0.1, 0.1));
            v.set_param("freqx", rng.range(1.0, 6.0));
            v.set_param("freqy", rng.range(1.0, 6.0));
            xf.variations.push(v);
            layer.xforms.push(xf);
            let mut x2 = XForm::default();
            random_affine(&mut x2, rng, 0.4);
            x2.weight = 0.05;
            x2.color = rng.random();
            add_var(&mut x2, "linear3D", 1.0, rng);
            layer.xforms.push(x2);
        }
        Generator::Symmetric => {
            let n = 1 + rng.random_int(3);
            for _ in 0..n {
                let mut xf = XForm::default();
                random_affine_r(&mut xf, rng, 0.4, 0.9);
                xf.color = rng.random();
                xf.weight = rng.range(0.3, 1.0);
                add_var(&mut xf, pick(rng, &nice), rng.range(0.5, 1.0), rng);
                if rng.chance(0.5) {
                    add_var(&mut xf, "linear3D", rng.range(0.1, 0.4), rng);
                }
                layer.xforms.push(xf);
            }
            flame.post_symmetry.kind = PostSymmetryType::Point;
            flame.post_symmetry.order = 3 + rng.random_int(8) as i32;
        }
    }
    flame
}

/// Generates a random flame, retrying until it covers a reasonable part of the frame.
pub fn generate(generator: Generator, rng: &mut Rng) -> Flame {
    let mut best: Option<(f64, Flame)> = None;
    for _ in 0..12 {
        let mut f = generate_raw(generator, rng);
        if !auto_frame(&mut f) {
            continue;
        }
        let c = coverage(&f);
        if (0.12..0.85).contains(&c) {
            return f;
        }
        let score = -(c - 0.4).abs();
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, f));
        }
    }
    best.map(|(_, f)| f).unwrap_or_else(|| generate_raw(generator, rng))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mutation {
    /// Small random changes to affine coefficients.
    Affine,
    /// Change variation amounts/parameters, sometimes swap a variation.
    Variations,
    /// New gradient or gradient shift.
    Colors,
    /// One of the above, chosen at random.
    Mixed,
}

/// Returns a mutated copy of `flame`; `strength` in 0..1.
pub fn mutate(flame: &Flame, kind: Mutation, strength: f64, rng: &mut Rng) -> Flame {
    let mut f = flame.clone();
    let nice = available(NICE);
    let kind = if kind == Mutation::Mixed { [Mutation::Affine, Mutation::Variations, Mutation::Colors][rng.random_int(3)] } else { kind };
    let Some(layer) = f.layers.first_mut() else {
        return f;
    };
    match kind {
        Mutation::Affine => {
            for xf in &mut layer.xforms {
                if rng.chance(0.6) {
                    xf.coefs.rotate(rng.range(-30.0, 30.0) * strength);
                    xf.coefs.scale(1.0 + rng.range(-0.15, 0.15) * strength);
                    xf.coefs.translate(rng.range(-0.3, 0.3) * strength, rng.range(-0.3, 0.3) * strength);
                }
            }
        }
        Mutation::Variations => {
            if layer.xforms.is_empty() {
                return f;
            }
            let i = rng.random_int(layer.xforms.len());
            let xf = &mut layer.xforms[i];
            if rng.chance(0.4) || xf.variations.is_empty() {
                let name = pick(rng, &nice);
                if xf.variations.len() > 1 && rng.chance(0.5) {
                    let j = rng.random_int(xf.variations.len());
                    let amount = xf.variations[j].amount;
                    xf.variations[j] = VariationInstance::new(name, amount);
                } else {
                    add_var(xf, name, rng.range(0.1, 0.6) * strength.max(0.2), rng);
                }
            } else {
                for v in &mut xf.variations {
                    v.amount *= 1.0 + rng.range(-0.5, 0.5) * strength;
                    if let Some(def) = variations::find(&v.name) {
                        for (k, p) in def.params.iter().enumerate() {
                            if k < v.params.len() && rng.chance(0.5) {
                                let span = (p.max - p.min).abs() * 0.2 * strength;
                                v.params[k] += rng.range(-span, span);
                                if p.integer {
                                    v.params[k] = v.params[k].round();
                                }
                            }
                        }
                    }
                }
            }
        }
        Mutation::Colors => {
            if rng.chance(0.5) {
                layer.gradient = Gradient::random(rng);
            } else {
                layer.gradient.adjust_hsl(rng.range(-0.5, 0.5) * strength, 0.0, 0.0);
            }
            for xf in &mut layer.xforms {
                if rng.chance(0.5) {
                    xf.color = (xf.color + rng.range(-0.3, 0.3) * strength).rem_euclid(1.0);
                }
            }
        }
        Mutation::Mixed => unreachable!(),
    }
    f
}
