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

//! Ported generators built on other generators: Black&White, Bokeh, Flame with Background,
//! Painterly, Layers and SubFlame.

use super::gens_b::affine;
use super::java::{add, set, xform};
use super::{Generator, Quality, State, sample};
use crate::gradient::Gradient;
use crate::model::{BgColorType, Flame};

/// The base generators of `BlackAndWhiteRandomFlameGenerator` (also used by "Bokeh" and
/// "Flame with Background"); duplicates weight the choice as in JWildfire. Duckies and
/// RunRandomScript are not available.
const BW_BASE: &[Generator] = &[
    Generator::Brokat,
    Generator::Brokat3D,
    Generator::Bubbles,
    Generator::Duality,
    Generator::Bubbles3D,
    Generator::Cross,
    Generator::Duality,
    Generator::Galaxies,
    Generator::ExperimentalBubbles3D,
    Generator::ExperimentalGnarl,
    Generator::ExperimentalSimple,
    Generator::FilledFlowers3D,
    Generator::Flowers3D,
    Generator::Gnarl,
    Generator::Duality,
    Generator::Galaxies,
    Generator::Gnarl3D,
    Generator::JulianDisc,
    Generator::Julians,
    Generator::Layers,
    Generator::Affine3D,
    Generator::JulianRings,
    Generator::Linear,
    Generator::Machine,
    Generator::Mandelbrot,
    Generator::Outlines,
    Generator::Phoenix,
    Generator::Raster,
    Generator::Rays,
    Generator::Simple,
    Generator::SimpleTiling,
    Generator::Sierpinsky,
    Generator::Duality,
    Generator::Galaxies,
    Generator::Spherical,
    Generator::Spherical3D,
    Generator::Ghosts,
    Generator::Orchids,
    Generator::EDisc,
    Generator::Spirals,
    Generator::Spirals3D,
    Generator::Splits,
    Generator::SubFlame,
    Generator::Synth,
    Generator::Tentacle,
    Generator::TileBall,
    Generator::Duality,
    Generator::Underwater,
    Generator::Xenomorph,
];

/// Prepares a flame with a base generator chosen once per sample (`initState`).
fn wrapped(st: &mut State, owner: Generator, base: &[Generator], extra: &[Generator]) -> Flame {
    let g = st.choice(owner, |r| {
        let n = base.len() + extra.len();
        let i = ((r.random() * n as f64) as usize).min(n - 1);
        if i < base.len() { base[i] } else { extra[i - base.len()] }
    });
    g.prepare(st)
}

/// `BlackAndWhiteRandomFlameGenerator` ("Black&White").
pub fn black_and_white(st: &mut State) -> Flame {
    wrapped(st, Generator::BlackAndWhite, BW_BASE, super::SOLID_GENERATORS)
}

/// `BlackAndWhiteRandomFlameGenerator.postProcessFlameBeforeRendering`: white on dark or
/// black on light.
pub fn black_and_white_post(st: &mut State, flame: &mut Flame) {
    let r = &mut *st.rng;
    let ink = if r.random() < 0.42 {
        if r.random() < 0.5 {
            flame.background.kind = BgColorType::SingleColor;
            flame.background.color = [0, 0, 0];
        } else {
            // JWildfire sets the single color here, the gradient corners stay as they are.
            flame.background.kind = BgColorType::Gradient2x2;
            flame.background.color = [(r.random() * 64.0) as u8, (r.random() * 64.0) as u8, (r.random() * 64.0) as u8];
        }
        [255, 255, 255]
    } else {
        if r.random() < 0.5 {
            flame.background.kind = BgColorType::SingleColor;
            flame.background.color = [255, 255, 255];
        } else {
            flame.background.kind = BgColorType::Gradient2x2;
            flame.background.color = [(255.0 - r.random() * 64.0) as u8, (255.0 - r.random() * 64.0) as u8, (255.0 - r.random() * 64.0) as u8];
        }
        [0, 0, 0]
    };
    for layer in &mut flame.layers {
        layer.gradient = Gradient::from_colors(vec![ink]);
    }
    flame.gamma = 0.45 + r.random() * 1.5;
    flame.gamma_threshold = 0.2 * r.random();
    flame.saturation = 1.0;
    flame.brightness = 3.0 + r.random() * 2.0;
    flame.contrast = 2.5;
    flame.bg_transparency = false;
}

/// `BokehRandomFlameGenerator`: the Black&White base flames with a bokeh.
pub fn bokeh(st: &mut State) -> Flame {
    wrapped(st, Generator::Bokeh, BW_BASE, super::SOLID_GENERATORS)
}

/// `FlameWithBackgroundRandomFlameGenerator` ("Flame with Background").
pub fn flame_with_background(st: &mut State) -> Flame {
    wrapped(st, Generator::FlameWithBackground, BW_BASE, super::SOLID_GENERATORS)
}

/// `PainterlyRandomFlameGenerator`: like Black&White's base list, plus Black&White itself.
pub fn painterly(st: &mut State) -> Flame {
    wrapped(st, Generator::Painterly, BW_BASE, &[Generator::BlackAndWhite])
}

/// `LayersRandomFlameGenerator`: two or three quick samples of simple generators as layers.
pub fn layers(st: &mut State) -> Flame {
    let mut flame = sample(Generator::All, st.rng, Quality::LOW, true, false);
    let count = 2 + usize::from(st.rng.random() > 0.66);
    for _ in 1..count {
        let sub = sample(Generator::All, st.rng, Quality::LOW, true, false);
        flame.layers.extend(sub.layers.into_iter().take(1));
        flame.layers[0].weight = 0.85 + st.rng.random() * 0.3;
    }
    flame.pixels_per_unit = 200.0;
    flame
}

/// `SubFlameRandomFlameGenerator`: a quick sample of a simple generator embedded with
/// `subflame_wf` (CPU only, like in JWildfire).
pub fn sub_flame(st: &mut State) -> Flame {
    let sub = sample(Generator::All, st.rng, Quality::LOW, true, false);
    let r = &mut *st.rng;
    let mut flame = sub.clone();
    let layer = flame.first_layer_mut();
    layer.xforms.clear();
    layer.final_xforms.clear();
    {
        let mut xf = xform();
        xf.weight = 0.5;
        add(&mut xf, 1.0, "subflame_wf").set_resource("flame", &crate::io::write_flame(&sub));
        xf.color = 0.0;
        xf.color_symmetry = -0.22;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.25 + r.random() * 0.5;
        let (tx, ty) = (1.5 + r.random() * 2.5, -0.25 - r.random() * 0.35);
        xf.coefs = affine(0.17254603006834707, 0.6439505508593787, -0.6439505508593787, 0.17254603006834707, tx, ty);
        add(&mut xf, 1.0, "linear3D");
        xf.color = r.random();
        xf.color_symmetry = -0.62;
        layer.xforms.push(xf);
    }
    {
        let mut xf = xform();
        xf.weight = 0.25 + r.random() * 0.5;
        xf.coefs = affine(0.17254603006834707, 0.6439505508593787, -0.6439505508593787, 0.17254603006834707, -3.0, 0.3);
        let cx = -0.2 + 0.4 * r.random();
        set(add(&mut xf, 1.0, "curl3D"), &[("cx", cx), ("cy", 0.0), ("cz", 0.0)]);
        xf.color = r.random();
        xf.color_symmetry = 0.0;
        layer.xforms.push(xf);
    }
    flame.centre_x = 2.0;
    flame.centre_y = 1.0;
    flame.cam_pitch = 0.0;
    flame.cam_roll = -2.0;
    flame.cam_yaw = 0.0;
    flame.cam_bank = 0.0;
    flame.pixels_per_unit = 200.0;
    flame
}
