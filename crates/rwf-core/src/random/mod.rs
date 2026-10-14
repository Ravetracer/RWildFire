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

//! Random flame generators (ports of JWildfire's random flame generators plus RWildFire's
//! own "Classic" and "Symmetric") and mutations.

mod coverage;
pub mod extras;
mod gens_a;
mod gens_b;
mod gens_c;
mod gens_d;
mod gens_e;
pub mod gradients;
pub mod java;
pub mod mutagen;
pub mod mutations;

use crate::gradient::Gradient;
use crate::model::{Flame, Layer, PostSymmetryType, VariationInstance, XForm};
use crate::render::analysis::{auto_frame, coverage};
use crate::rng::Rng;
use crate::{solid, variations};

pub use extras::{Extras, SymmetryGen, WeightingGen};

/// What a generator remembers between the attempts of one sample
/// (`RandomFlameGeneratorState`).
pub struct State<'a> {
    pub rng: &'a mut Rng,
    /// The base generator each wrapper ("(All)", the 3D and Black&White wrappers, ...) chose,
    /// kept for all attempts of a sample (JWildfire keeps them as separate state parameters).
    pub(crate) subs: Vec<(Generator, Generator)>,
    /// "(All)" chooses among the simple generators only (no layers, sub-flames, solid).
    pub(crate) simple: bool,
}

impl State<'_> {
    /// The base generator `owner` chose (choosing it with `pick` the first time).
    pub(crate) fn choice(&mut self, owner: Generator, pick: impl FnOnce(&mut Rng) -> Generator) -> Generator {
        if let Some(&(_, g)) = self.subs.iter().find(|(o, _)| *o == owner) {
            return g;
        }
        let g = pick(self.rng);
        self.subs.push((owner, g));
        g
    }

    /// The base generator `owner` chose, if any.
    pub(crate) fn chosen(&self, owner: Generator) -> Option<Generator> {
        self.subs.iter().find(|(o, _)| *o == owner).map(|&(_, g)| g)
    }
}

/// Coverage a sample must reach and the number of attempts (`RandomBatchQuality`).
#[derive(Clone, Copy, Debug)]
pub struct Quality {
    pub min_coverage: f64,
    pub max_samples: usize,
}

impl Quality {
    pub const LOW: Quality = Quality { min_coverage: 0.32, max_samples: 8 };
    pub const NORMAL: Quality = Quality { min_coverage: 0.42, max_samples: 16 };
}

/// The solid rendering generators (base generators of several wrappers).
pub(crate) const SOLID_GENERATORS: &[Generator] = &[
    Generator::SolidExperimental,
    Generator::SolidStunning,
    Generator::SolidJulia3D,
    Generator::SolidShadows,
    Generator::SolidLabyrinth,
    Generator::SolidRecursive,
];

macro_rules! generators {
    ($($variant:ident => $label:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Generator {
            $($variant),*
        }

        impl Generator {
            pub const ALL: &'static [Generator] = &[$(Generator::$variant),*];

            pub fn label(&self) -> &'static str {
                match self {
                    $(Generator::$variant => $label),*
                }
            }
        }
    };
}

generators! {
    All => "(All)",
    Classic => "Classic",
    Symmetric => "Symmetric",
    Affine3D => "Affine3D",
    Brokat => "Brokat",
    Brokat3D => "Brokat3D",
    Bubbles => "Bubbles",
    BlackAndWhite => "Black&White",
    Bokeh => "Bokeh",
    Bubbles3D => "Bubbles3D",
    ExperimentalBubbles3D => "Bubbles3D (experimental)",
    Cross => "Cross",
    Duality => "Duality",
    EDisc => "EDisc",
    FlameWithBackground => "Flame with Background",
    FilledFlowers3D => "Flowers3D (filled)",
    Flowers3D => "Flowers3D (stunning)",
    Galaxies => "Galaxies",
    Ghosts => "Ghosts",
    Gnarl => "Gnarl",
    Gnarl3D => "Gnarl3D",
    ExperimentalGnarl => "Gnarl (experimental)",
    JulianDisc => "JulianDisc",
    JulianRings => "JulianRings",
    Julians => "Julians",
    Layers => "Layers",
    Linear => "Linear only",
    Machine => "Machine",
    Mandelbrot => "Mandelbrot",
    Orchids => "Orchids",
    Outlines => "Outlines",
    Painterly => "Painterly",
    Phoenix => "Phoenix",
    Raster => "Raster",
    Rays => "Rays",
    Sierpinsky => "Sierpinsky",
    ExperimentalSimple => "Simple (experimental)",
    Simple => "Simple (stunning)",
    SimpleTiling => "Simple tiling",
    SolidExperimental => "Solid (experimental)",
    SolidRecursive => "Solid (recursive)",
    SolidShadows => "Solid (shadows)",
    SolidStunning => "Solid (stunning)",
    SolidJulia3D => "Solid Julia3D",
    SolidLabyrinth => "Solid Labyrinth",
    Spherical => "Spherical",
    Spherical3D => "Spherical3D",
    Spirals => "Spirals",
    Spirals3D => "Spirals3D",
    Splits => "Splits",
    SubFlame => "SubFlame",
    Synth => "Synth",
    Tentacle => "Tentacle",
    TileBall => "Tile Ball",
    Underwater => "Underwater",
    Xenomorph => "Xenomorph",
}

impl Generator {
    /// Looks a generator up by its label.
    pub fn by_label(label: &str) -> Option<Generator> {
        Generator::ALL.iter().copied().find(|g| g.label() == label)
    }

    /// RWildFire's own generators (no JWildfire counterpart): framed automatically.
    fn is_own(self) -> bool {
        matches!(self, Generator::Classic | Generator::Symmetric)
    }

    /// The generators "(All)" chooses from (JWildfire's GPU list: no sub-flames; the simple
    /// list also without layers and solid rendering).
    fn all_candidates(simple: bool) -> Vec<Generator> {
        Generator::ALL
            .iter()
            .copied()
            .filter(|g| *g != Generator::All && !g.is_own() && *g != Generator::SubFlame)
            .filter(|g| !simple || (*g != Generator::Layers && !SOLID_GENERATORS.contains(g)))
            .collect()
    }

    /// `prepareFlame`.
    fn prepare(self, st: &mut State) -> Flame {
        match self {
            Generator::All => {
                let simple = st.simple;
                let g = st.choice(Generator::All, |r| {
                    let c = Generator::all_candidates(simple);
                    c[r.random_int(c.len())]
                });
                let mut f = g.prepare(st);
                f.name = g.label().to_string();
                f
            }
            Generator::Classic | Generator::Symmetric => own_generator(self, st.rng),
            Generator::Affine3D => gens_a::affine3d(st),
            Generator::Bubbles => gens_a::bubbles(st),
            Generator::Bubbles3D => gens_a::bubbles3d(st),
            Generator::ExperimentalBubbles3D => gens_a::experimental_bubbles3d(st),
            Generator::Linear => gens_a::linear(st),
            Generator::Sierpinsky => gens_a::sierpinsky(st),
            Generator::ExperimentalSimple => gens_a::experimental_simple(st),
            Generator::Simple => gens_a::simple(st),
            Generator::Spherical => gens_a::spherical(st),
            Generator::Spherical3D => gens_a::spherical3d(st),
            Generator::Brokat => gens_b::brokat(st),
            Generator::Brokat3D => gens_b::brokat3d(st),
            Generator::EDisc => gens_b::edisc(st),
            Generator::Gnarl => gens_b::gnarl(st),
            Generator::Gnarl3D => gens_b::gnarl3d(st),
            Generator::ExperimentalGnarl => gens_b::experimental_gnarl(st),
            Generator::JulianDisc => gens_b::julian_disc(st),
            Generator::JulianRings => gens_b::julian_rings(st),
            Generator::Julians => gens_b::julians(st),
            Generator::Spirals => gens_b::spirals(st),
            Generator::Spirals3D => gens_b::spirals3d(st),
            Generator::Synth => gens_b::synth(st),
            Generator::Cross => gens_c::cross(st),
            Generator::Duality => gens_c::duality(st),
            Generator::FilledFlowers3D => gens_c::filled_flowers3d(st),
            Generator::Flowers3D => gens_c::flowers3d(st),
            Generator::Galaxies => gens_c::galaxies(st),
            Generator::Ghosts => gens_c::ghosts(st),
            Generator::Machine => gens_c::machine(st),
            Generator::Mandelbrot => gens_c::mandelbrot(st),
            Generator::Orchids => gens_c::orchids(st),
            Generator::Outlines => gens_c::outlines(st),
            Generator::Phoenix => gens_c::phoenix(st),
            Generator::Raster => gens_c::raster(st),
            Generator::Rays => gens_c::rays(st),
            Generator::SimpleTiling => gens_c::simple_tiling(st),
            Generator::Splits => gens_c::splits(st),
            Generator::Tentacle => gens_c::tentacle(st),
            Generator::TileBall => gens_c::tile_ball(st),
            Generator::Underwater => gens_c::underwater(st),
            Generator::Xenomorph => gens_c::xenomorph(st),
            Generator::BlackAndWhite => gens_d::black_and_white(st),
            Generator::Bokeh => gens_d::bokeh(st),
            Generator::FlameWithBackground => gens_d::flame_with_background(st),
            Generator::Layers => gens_d::layers(st),
            Generator::Painterly => gens_d::painterly(st),
            Generator::SubFlame => gens_d::sub_flame(st),
            Generator::SolidExperimental => gens_e::solid_experimental(st),
            Generator::SolidRecursive => gens_e::solid_recursive(st),
            Generator::SolidShadows => gens_e::solid_shadows(st),
            Generator::SolidStunning => gens_e::solid_stunning(st),
            Generator::SolidJulia3D => gens_e::solid_julia3d(st),
            Generator::SolidLabyrinth => gens_e::solid_labyrinth(st),
        }
    }

    /// `isUseFilter`: judge the preview by its edges instead of its area.
    fn use_filter(self, st: &mut State) -> bool {
        match self {
            Generator::All => st.chosen(Generator::All).is_some_and(|g| g.use_filter(st)),
            Generator::Affine3D => st.rng.random() > 0.42,
            Generator::Rays => st.rng.random() > 0.5,
            Generator::Bubbles
            | Generator::Bubbles3D
            | Generator::ExperimentalBubbles3D
            | Generator::ExperimentalSimple
            | Generator::Simple
            | Generator::EDisc
            | Generator::Synth
            | Generator::Duality
            | Generator::FilledFlowers3D
            | Generator::Flowers3D
            | Generator::Machine
            | Generator::Mandelbrot
            | Generator::Orchids
            | Generator::Raster
            | Generator::SimpleTiling
            | Generator::Splits
            | Generator::Tentacle
            | Generator::BlackAndWhite
            | Generator::Bokeh
            | Generator::FlameWithBackground
            | Generator::Painterly
            | Generator::SubFlame => true,
            _ => false,
        }
    }

    /// `postProcessFlameBeforeRendering` (after the random gradient was applied).
    fn post_process(self, st: &mut State, flame: &mut Flame) {
        match self {
            Generator::All => {
                if let Some(g) = st.chosen(self) {
                    g.post_process(st, flame);
                }
            }
            Generator::BlackAndWhite => gens_d::black_and_white_post(st, flame),
            Generator::SolidExperimental | Generator::SolidRecursive | Generator::SolidStunning | Generator::SolidJulia3D | Generator::SolidLabyrinth => {
                gens_e::solid_post(flame, st.rng)
            }
            Generator::SolidShadows => {
                if let Some(g) = st.chosen(self) {
                    g.post_process(st, flame);
                }
                flame.post_symmetry.kind = PostSymmetryType::None;
            }
            Generator::Bokeh => mutations::bokeh_mutation(flame, 1.0, st.rng),
            Generator::Painterly => {
                for layer in &mut flame.layers {
                    mutations::painterly_mutation(layer, st.rng);
                }
            }
            _ => {}
        }
    }

    /// `postProcessFlameAfterRendering` (after the coverage check).
    fn post_process_after(self, st: &mut State, flame: &mut Flame) {
        match self {
            Generator::All => {
                if let Some(g) = st.chosen(self) {
                    g.post_process_after(st, flame);
                }
            }
            Generator::FlameWithBackground => {
                if let Some(layer) = flame.layers.first_mut() {
                    mutations::random_background_mutation(layer, 1.0, st.rng);
                }
            }
            Generator::SolidShadows => gens_e::solid_shadows_after(flame, st.rng),
            Generator::SolidLabyrinth if st.rng.random() > 0.25 => {
                flame.solid.shadow_type = if st.rng.random() > 0.25 { solid::ShadowType::Fast } else { solid::ShadowType::Smooth };
            }
            _ => {}
        }
    }

    /// `supportsSymmetry`: the random symmetry generator may add a post symmetry.
    fn supports_symmetry(self) -> bool {
        !matches!(
            self,
            Generator::EDisc
                | Generator::JulianRings
                | Generator::Orchids
                | Generator::Outlines
                | Generator::Phoenix
                | Generator::Raster
                | Generator::SolidLabyrinth
                | Generator::SolidShadows
                | Generator::TileBall
                | Generator::Underwater
        )
    }

    /// `getMaxCoverage`: solid flames must leave some background.
    fn max_coverage(self) -> f64 {
        if SOLID_GENERATORS.contains(&self) && self != Generator::SolidShadows { 0.75 } else { 1.0 }
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

/// RWildFire's own generators.
fn own_generator(generator: Generator, rng: &mut Rng) -> Flame {
    let nice = available(NICE);
    let mut flame = base_flame(rng);
    let layer = &mut flame.layers[0];
    match generator {
        Generator::Classic => {
            let n = 2 + rng.random_int(4);
            let mut scl = 1.0;
            for _ in 0..n {
                let mut xf = XForm::default();
                scl *= 0.75 + rng.random() / 4.0;
                random_affine(&mut xf, rng, scl);
                xf.color = rng.random();
                xf.weight = rng.random() * 0.9 + 0.1;
                add_var(&mut xf, "linear3D", rng.random() * 0.3 + 0.2, rng);
                let extra = usize::from(rng.chance(0.9));
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
        _ => unreachable!("not an own generator"),
    }
    flame
}

/// Generates a random flame (`RandomFlameGeneratorSampler.createSample`): up to 16
/// attempts with a random gradient each, the first one that fills enough of a quick preview
/// wins, otherwise the one that came closest. Symmetry and weighting fields as in
/// JWildfire's defaults ("(All, sparse)").
pub fn generate(generator: Generator, rng: &mut Rng) -> Flame {
    generate_with(generator, Extras::default(), rng)
}

/// [`generate`] with the given random symmetry and weighting field generators (RWildFire's
/// own generators ignore them).
pub fn generate_with(generator: Generator, extras: Extras, rng: &mut Rng) -> Flame {
    if generator.is_own() {
        return generate_own(generator, rng);
    }
    sample(generator, rng, Quality::NORMAL, false, true, extras)
}

/// One sample of a generator (`RandomFlameGeneratorSampler.createSample`). `simple`: "(All)"
/// uses the simple generators only; `finish`: name it and bring it to the 720 pixel frame
/// (not for flames that become part of another one); `extras`: random symmetry and
/// weighting fields.
pub(crate) fn sample(generator: Generator, rng: &mut Rng, quality: Quality, simple: bool, finish: bool, extras: Extras) -> Flame {
    let mut st = State { rng, subs: Vec::new(), simple };
    let mut best: Option<(f64, Flame)> = None;
    for _ in 0..quality.max_samples {
        let mut flame = generator.prepare(&mut st);
        if generator.supports_symmetry() {
            extras.symmetry.apply(&mut flame, st.rng);
        }
        extras.weighting.apply(&mut flame, st.rng);
        for layer in &mut flame.layers {
            layer.gradient = gradients::random_gradient(st.rng);
        }
        generator.post_process(&mut st, &mut flame);
        if st.rng.random() > 0.666 {
            let strength = 0.125 + st.rng.random() * 0.25;
            if let Some(layer) = flame.layers.first_mut() {
                mutations::random_param_mutation(layer, strength, st.rng);
            }
        }
        let use_filter = generator.use_filter(&mut st);
        let mut preview = flame.clone();
        java::to_widescreen(&mut preview);
        let c = coverage::coverage(&preview, use_filter);
        generator.post_process_after(&mut st, &mut flame);
        if finish {
            if flame.name.is_empty() {
                flame.name = generator.label().to_string();
            }
            java::to_widescreen(&mut flame);
            flame.bg_transparency = false;
        }
        if c >= quality.min_coverage && c < generator.max_coverage() {
            return flame;
        }
        if best.as_ref().is_none_or(|(b, _)| c > *b) {
            best = Some((c, flame));
        }
    }
    best.map(|(_, f)| f).expect("at least one attempt")
}

/// RWildFire's own generators: framed automatically, retried until they cover a reasonable
/// part of the frame.
fn generate_own(generator: Generator, rng: &mut Rng) -> Flame {
    let mut best: Option<(f64, Flame)> = None;
    for _ in 0..12 {
        let mut f = own_generator(generator, rng);
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
    best.map(|(_, f)| f).unwrap_or_else(|| own_generator(generator, rng))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_generator_builds_flames() {
        for &g in Generator::ALL {
            for seed in 0..3 {
                let mut rng = Rng::new(seed * 7919 + 1);
                let mut st = State { rng: &mut rng, subs: Vec::new(), simple: false };
                let flame = g.prepare(&mut st);
                assert!(flame.layers.iter().any(|l| !l.xforms.is_empty()), "{} produced no transforms", g.label());
                for l in &flame.layers {
                    for xf in l.xforms.iter().chain(&l.final_xforms) {
                        for v in &xf.variations {
                            assert!(variations::find(&v.name).is_some(), "{}: unknown variation {}", g.label(), v.name);
                        }
                    }
                }
            }
            assert_eq!(Generator::by_label(g.label()), Some(g));
        }
    }

    /// "(All)" with every generator as its choice: the post-processing chain must end (it
    /// once recursed forever when "(All)" and "Solid (shadows)" shared one choice).
    #[test]
    fn all_runs_every_generator_through_post_processing() {
        for &g in Generator::all_candidates(false).iter().filter(|g| **g != Generator::Layers) {
            let mut rng = Rng::new(17);
            let mut st = State { rng: &mut rng, subs: vec![(Generator::All, g)], simple: false };
            let mut flame = Generator::All.prepare(&mut st);
            Generator::All.post_process(&mut st, &mut flame);
            Generator::All.post_process_after(&mut st, &mut flame);
            assert_eq!(st.chosen(Generator::All), Some(g));
            assert!(!flame.layers.is_empty(), "{}", g.label());
        }
    }
}
