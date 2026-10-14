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

//! Core of RWildFire (a Rust port of JWildfire): flame model, file IO, variations and the CPU renderer.

pub mod animation;
pub mod audio;
pub mod formula;
pub mod brush;
pub mod curves;
pub mod dance;
pub mod fastnoise;
pub mod gradient;
pub mod gradient_io;
pub mod io;
pub mod math;
pub mod mesh;
pub mod meshgen;
pub mod mixer;
pub mod model;
pub mod movie;
pub mod output;
pub mod random;
pub mod render;
pub mod resources;
pub mod rng;
pub mod shapes;
pub mod solid;
pub mod variations;
pub mod video;
pub mod weighting;

pub use gradient::Gradient;
pub use model::{Affine, ColorType, DrawMode, Flame, Layer, VariationInstance, XForm};
