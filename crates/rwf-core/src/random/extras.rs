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

//! Random symmetry and random weighting field generators (`RandomSymmetryGeneratorList`,
//! `RandomWeightingFieldGeneratorList`): applied by the sampler to every new flame of a
//! generator that supports them.

use crate::model::{Flame, PostSymmetryType};
use crate::rng::Rng;
use crate::weighting::FieldType;

use super::mutations;

/// A random symmetry generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SymmetryGen {
    None,
    All,
    #[default]
    AllSparse,
    XAxis,
    YAxis,
    Point,
}

impl SymmetryGen {
    pub const ALL: &'static [SymmetryGen] =
        &[SymmetryGen::None, SymmetryGen::All, SymmetryGen::AllSparse, SymmetryGen::XAxis, SymmetryGen::YAxis, SymmetryGen::Point];

    /// The generators "(All)" chooses from.
    const CHOICES: [SymmetryGen; 4] = [SymmetryGen::None, SymmetryGen::XAxis, SymmetryGen::YAxis, SymmetryGen::Point];

    pub fn label(self) -> &'static str {
        match self {
            SymmetryGen::None => "None",
            SymmetryGen::All => "(All)",
            SymmetryGen::AllSparse => "(All, sparse)",
            SymmetryGen::XAxis => "XAxis",
            SymmetryGen::YAxis => "YAxis",
            SymmetryGen::Point => "Point",
        }
    }

    pub fn by_label(label: &str) -> Option<SymmetryGen> {
        SymmetryGen::ALL.iter().copied().find(|g| g.label() == label)
    }

    /// `addSymmetry`
    pub fn apply(self, flame: &mut Flame, rng: &mut Rng) {
        let ps = &mut flame.post_symmetry;
        match self {
            SymmetryGen::None => ps.kind = PostSymmetryType::None,
            SymmetryGen::All => Self::CHOICES[rng.random_int(Self::CHOICES.len())].apply(flame, rng),
            SymmetryGen::AllSparse => {
                if rng.random() > 0.66 {
                    Self::CHOICES[rng.random_int(Self::CHOICES.len())].apply(flame, rng);
                } else {
                    ps.kind = PostSymmetryType::None;
                }
            }
            SymmetryGen::XAxis => {
                ps.kind = PostSymmetryType::XAxis;
                ps.centre_x = 0.5 - rng.random();
                ps.centre_y = 0.5 - rng.random();
                ps.distance = -0.25 + rng.random() * 1.75;
                ps.rotation = -30.0 + rng.random() * 60.0;
            }
            SymmetryGen::YAxis => {
                ps.kind = PostSymmetryType::YAxis;
                ps.centre_x = 0.5 - rng.random();
                ps.centre_y = 0.5 - rng.random();
                ps.distance = -0.25 + rng.random() * 0.75;
                ps.rotation = 0.0;
            }
            SymmetryGen::Point => {
                ps.kind = PostSymmetryType::Point;
                ps.centre_x = 0.5 - rng.random();
                ps.centre_y = 0.5 - rng.random();
                ps.order = (2.0 + rng.random() * 6.0) as i32;
            }
        }
    }
}

/// A random weighting field generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum WeightingGen {
    None,
    All,
    #[default]
    AllSparse,
    CellularNoise,
    BasicNoise,
    FractalNoise,
    ImageMap,
}

impl WeightingGen {
    pub const ALL: &'static [WeightingGen] = &[
        WeightingGen::None,
        WeightingGen::All,
        WeightingGen::AllSparse,
        WeightingGen::CellularNoise,
        WeightingGen::BasicNoise,
        WeightingGen::FractalNoise,
        WeightingGen::ImageMap,
    ];

    /// The generators "(All)" chooses from.
    const CHOICES: [WeightingGen; 4] = [WeightingGen::CellularNoise, WeightingGen::BasicNoise, WeightingGen::FractalNoise, WeightingGen::ImageMap];

    pub fn label(self) -> &'static str {
        match self {
            WeightingGen::None => "None",
            WeightingGen::All => "(All)",
            WeightingGen::AllSparse => "(All, sparse)",
            WeightingGen::CellularNoise => "Cellular Noise",
            WeightingGen::BasicNoise => "Basic Noise",
            WeightingGen::FractalNoise => "Fractal Noise",
            WeightingGen::ImageMap => "Image Map",
        }
    }

    pub fn by_label(label: &str) -> Option<WeightingGen> {
        WeightingGen::ALL.iter().copied().find(|g| g.label() == label)
    }

    /// `addWeightingField`
    pub fn apply(self, flame: &mut Flame, rng: &mut Rng) {
        use FieldType::*;
        let types: &[FieldType] = match self {
            WeightingGen::None => return,
            WeightingGen::All => return Self::CHOICES[rng.random_int(Self::CHOICES.len())].apply(flame, rng),
            WeightingGen::AllSparse => {
                // JWildfire leaves the flame as it is otherwise (`NONE` does nothing).
                if rng.random() > 0.66 {
                    Self::CHOICES[rng.random_int(Self::CHOICES.len())].apply(flame, rng);
                }
                return;
            }
            WeightingGen::CellularNoise => &[CellularNoise],
            WeightingGen::BasicNoise => &[CellularNoise, CubicNoise, PerlinNoise, SimplexNoise, ValueNoise],
            WeightingGen::FractalNoise => &[CubicFractalNoise, PerlinFractalNoise, SimplexFractalNoise, ValueFractalNoise],
            WeightingGen::ImageMap => &[ImageMap],
        };
        for layer in &mut flame.layers {
            mutations::weighting_field_of(layer, 1.0, types, rng);
        }
    }
}

/// The symmetry and weighting field generators of a sampler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Extras {
    pub symmetry: SymmetryGen,
    pub weighting: WeightingGen,
}

impl Extras {
    /// Neither symmetry nor weighting fields (flames that become part of another one).
    pub const NONE: Extras = Extras { symmetry: SymmetryGen::None, weighting: WeightingGen::None };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, XForm};

    fn flame() -> Flame {
        let layer = Layer { xforms: vec![XForm::default(), XForm::default()], ..Layer::default() };
        Flame { layers: vec![layer], ..Flame::default() }
    }

    #[test]
    fn labels_round_trip() {
        for &g in SymmetryGen::ALL {
            assert_eq!(SymmetryGen::by_label(g.label()), Some(g));
        }
        for &g in WeightingGen::ALL {
            assert_eq!(WeightingGen::by_label(g.label()), Some(g));
        }
    }

    #[test]
    fn generators_set_their_kind() {
        let mut rng = Rng::new(3);
        for (g, kind) in [
            (SymmetryGen::XAxis, PostSymmetryType::XAxis),
            (SymmetryGen::YAxis, PostSymmetryType::YAxis),
            (SymmetryGen::Point, PostSymmetryType::Point),
        ] {
            let mut f = flame();
            g.apply(&mut f, &mut rng);
            assert_eq!(f.post_symmetry.kind, kind);
        }
        for _ in 0..20 {
            let mut f = flame();
            WeightingGen::FractalNoise.apply(&mut f, &mut rng);
            let types: Vec<FieldType> = f.layers[0].xforms.iter().map(|x| x.weighting.field_type).filter(|t| *t != FieldType::None).collect();
            assert!(!types.is_empty());
            assert!(types.iter().all(|t| matches!(
                t,
                FieldType::CubicFractalNoise | FieldType::PerlinFractalNoise | FieldType::SimplexFractalNoise | FieldType::ValueFractalNoise
            )));
        }
    }
}
