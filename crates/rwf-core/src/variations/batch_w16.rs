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

//! Variations ported from the JWildfire sources (batch w16).
// approx_constant: the Java code uses truncated constants (0.707106781, 1.41421356), kept as is.
#![allow(clippy::needless_range_loop, clippy::approx_constant)]

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

/// `java.util.Random` (48-bit LCG), used to rebuild seeded tables and per-cell hashes.
struct JavaRandom {
    seed: i64,
}

const JAVA_RANDOM_MULT: i64 = 0x5_DEEC_E66D;
const JAVA_RANDOM_MASK: i64 = (1 << 48) - 1;

impl JavaRandom {
    fn new(seed: i64) -> Self {
        Self { seed: (seed ^ JAVA_RANDOM_MULT) & JAVA_RANDOM_MASK }
    }

    fn next(&mut self, bits: u32) -> i32 {
        self.seed = self.seed.wrapping_mul(JAVA_RANDOM_MULT).wrapping_add(0xB) & JAVA_RANDOM_MASK;
        (self.seed >> (48 - bits)) as i32
    }

    fn next_double(&mut self) -> f64 {
        (((self.next(26) as i64) << 27) + self.next(27) as i64) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        let mut r = self.next(31);
        let m = bound - 1;
        if bound & m == 0 {
            return ((bound as i64 * r as i64) >> 31) as i32;
        }
        let mut u = r;
        loop {
            r = u % bound;
            if u.wrapping_sub(r).wrapping_add(m) >= 0 {
                return r;
            }
            u = self.next(31);
        }
    }
}

/// `js.glsl.G.max/min/clamp` (comparison based, NaN handling like the Java code).
fn g_max(x: f64, y: f64) -> f64 {
    if x < y { y } else { x }
}

fn g_min(x: f64, y: f64) -> f64 {
    if y < x { y } else { x }
}

fn g_clamp(x: f64, lo: f64, hi: f64) -> f64 {
    java_min(java_max(x, lo), hi)
}

type V3 = [f64; 3];

fn v3_sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn v3_dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Common sampling loop of the SDF base shapes: 50 random points in the unit cube centered at
/// the origin, the first one inside the shape is plotted, otherwise the point is hidden.
fn sdf_shape_sample(ctx: &mut Ctx, v: &mut Pt, amount: f64, sd: impl Fn(V3) -> f64) {
    for _ in 0..50 {
        let x = ctx.random() - 0.5;
        let y = ctx.random() - 0.5;
        let z = ctx.random() - 0.5;
        if sd([x, y, z]) < 0.0 {
            v.do_hide = false;
            v.x += amount * x;
            v.y += amount * y;
            v.z += amount * z;
            return;
        }
    }
    v.do_hide = true;
}

// ---------------------------------------------------------------------------------------------
// multi_ifs: 74 IFS with 4 affine maps each, per map [a, b, e, c, d, f, color_index]
// (nx = a*x + b*y + e, ny = c*x + d*y + f), extracted from MultiIFSFunc.java.

#[rustfmt::skip]
static MULTI_IFS_TABLE: [[f64; 28]; 74] = [
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, -0.5, 0.0, 0.5, -0.5, 1.0, 0.5, 0.0, -0.5, 0.0, 0.5, 0.0, 2.0, 0.5, 0.0, 0.0, 0.0, 0.5, -0.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.25, -0.4330127018922193, 0.25, 0.4330127018922193, -0.25, 0.4330127018922193, 1.0, -0.25, 0.4330127018922193, -0.5, -0.4330127018922193, -0.25, -0.0, 2.0, -0.5, -0.0, -0.25, 0.0, -0.5, 0.4330127018922193, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, -0.5, 0.0, 0.5, 0.5, 1.0, -0.5, 0.0, 0.5, 0.0, 0.5, 0.5, 2.0, 0.0, 0.5, 0.5, -0.5, 0.0, 0.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, 0.5, 0.0, 1.0, -0.25, -0.4330127018922193, 0.25, -0.4330127018922193, 0.25, 0.4330127018922193, 2.0, -0.25, 0.4330127018922193, 0.25, -0.4330127018922193, -0.25, 0.4330127018922193, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, 0.5, 0.0, 0.5, 0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.5, 2.0, 0.0, 0.5, 0.5, -0.5, 0.0, 1.0, 3.0],
    [0.25, -0.4330127018922193, 0.0, 0.4330127018922193, 0.25, 0.0, 0.0, -0.25, 0.4330127018922193, -0.5, -0.4330127018922193, -0.25, 0.0, 1.0, -0.25, 0.4330127018922193, 0.25, -0.4330127018922193, -0.25, -0.4330127018922193, 2.0, -0.25, 0.4330127018922193, 0.25, -0.4330127018922193, -0.25, 0.4330127018922193, 3.0],
    [-0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.25, -0.4330127018922193, 0.25, -0.4330127018922194, 0.25, -0.4330127018922194, 1.0, -0.25, -0.4330127018922193, -0.5, 0.4330127018922194, -0.25, -0.8660254037844387, 2.0, 0.5, 0.0, 1.0, 0.0, 0.5, 0.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, -0.5, 1.0, 0.0, 0.5, 0.0, 0.5, 0.0, -0.5, 2.0, 0.5, 0.0, -0.5, 0.0, 0.5, 0.0, 3.0],
    [-0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.5, 0.0, -0.25, 0.0, -0.5, 0.4330127018922194, 1.0, 0.25, -0.4330127018922193, 0.25, 0.4330127018922194, 0.25, 0.4330127018922194, 2.0, 0.25, -0.4330127018922193, 0.25, 0.4330127018922194, 0.25, -0.4330127018922194, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.25, 0.4330127018922193, -0.5, -0.4330127018922194, -0.25, 0.0, 1.0, -0.25, 0.4330127018922193, -0.25, 0.4330127018922194, 0.25, 0.4330127018922194, 2.0, -0.25, -0.4330127018922193, -0.25, -0.4330127018922194, -0.25, 0.4330127018922194, 3.0],
    [-0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, -1.0, 0.0, -0.5, -1.5, 1.0, -0.5, 0.0, 0.0, 0.0, -0.5, -1.0, 2.0, 0.5, 0.0, -1.0, 0.0, 0.5, -1.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, 0.5, 0.0, 1.0, -0.25, -0.4330127018922193, 0.25, -0.4330127018922194, 0.25, 0.4330127018922194, 2.0, -0.25, 0.4330127018922193, 0.25, -0.4330127018922194, -0.25, 0.4330127018922194, 3.0],
    [0.0, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.5, 0.0, -0.5, 1.0, 0.0, 0.5, 0.0, -0.5, 0.0, -0.5, 2.0, 0.0, -0.5, -1.0, -0.5, 0.0, -1.0, 3.0],
    [-0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.25, -0.4330127018922193, 0.5, -0.4330127018922194, 0.25, 0.0, 1.0, 0.5, 0.0, 0.5, 0.0, -0.5, 0.0, 2.0, -0.25, 0.4330127018922193, 0.5, 0.4330127018922194, 0.25, 0.0, 3.0],
    [0.0, -0.5, 0.0, -0.5, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, 0.5, 1.0, 0.0, -0.5, -0.5, -0.5, 0.0, 0.0, 2.0, 0.0, 0.5, -0.5, 0.5, 0.0, 0.5, 3.0],
    [0.0, -0.5, 0.0, -0.5, 0.0, 0.0, 0.0, 0.5, 0.0, -1.0, 0.0, 0.5, 0.0, 1.0, 0.0, 0.5, -1.5, 0.5, 0.0, 0.5, 2.0, 0.0, -0.5, -0.5, -0.5, 0.0, 0.5, 3.0],
    [-0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.5, -0.5, 0.0, -0.5, 1.0, 0.0, 0.5, 2.0, -0.5, 0.0, -0.5, 2.0, 0.5, 0.0, 1.0, 0.0, -0.5, -1.5, 3.0],
    [-0.25, -0.4330127018922193, 0.0, -0.4330127018922194, 0.25, 0.0, 0.0, -0.25, -0.4330127018922193, 0.5, -0.4330127018922194, 0.25, -0.8660254037844387, 1.0, 0.25, 0.4330127018922193, 0.25, 0.4330127018922194, -0.25, -1.299038105676658, 2.0, -0.25, -0.4330127018922193, 1.25, -0.4330127018922194, 0.25, 0.4330127018922194, 3.0],
    [0.0, -0.5, 0.0, -0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, -0.5, -0.5, 1.0, -0.5, 0.0, 0.5, 0.0, 0.5, -0.5, 2.0, 0.0, -0.5, 0.0, -0.5, 0.0, -0.5, 3.0],
    [0.0, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, 0.5, 0.0, 1.0, 0.0, 0.5, 0.5, 0.5, 0.0, 0.0, 2.0, 0.0, -0.5, 0.5, -0.5, 0.0, 0.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.0, 0.0, 1.0, 0.0, -0.5, 0.5, -0.5, 0.0, 0.5, 2.0, -0.5, 0.0, 0.5, 0.0, -0.5, 0.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.5, 0.0, -0.5, 0.0, -0.5, 0.5, 1.0, 0.0, 0.5, -0.5, 0.5, 0.0, 1.0, 2.0, -0.5, 0.0, -0.5, 0.0, -0.5, 1.0, 3.0],
    [0.0, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, 0.0, -0.5, -0.5, 1.0, 0.5, 0.0, -0.5, 0.0, -0.5, -1.0, 2.0, 0.0, 0.5, -1.5, 0.5, 0.0, 0.0, 3.0],
    [0.0, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, -0.5, 0.0, -0.5, 0.0, -0.5, -0.5, 1.0, 0.5, 0.0, 0.0, 0.0, -0.5, -1.0, 2.0, 0.0, -0.5, -0.5, 0.5, 0.0, -0.5, 3.0],
    [0.25, 0.4330127018922193, 0.0, 0.4330127018922194, -0.25, 0.0, 0.0, 0.25, 0.4330127018922193, 0.5, 0.4330127018922194, -0.25, -0.8660254037844387, 1.0, -0.25, -0.4330127018922193, 1.5, -0.4330127018922194, 0.25, 0.0, 2.0, 0.25, 0.4330127018922193, 1.5, 0.4330127018922194, -0.25, 0.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.25, 0.4330127018922193, -0.25, -0.4330127018922194, 0.25, 0.4330127018922194, 1.0, -0.5, 0.0, 0.5, 0.0, 0.5, 0.0, 2.0, -0.25, 0.4330127018922193, 0.25, -0.4330127018922194, -0.25, 0.4330127018922194, 3.0],
    [0.25, -0.4330127018922193, 0.0, 0.4330127018922194, 0.25, 0.0, 0.0, -0.25, -0.4330127018922193, 0.25, 0.4330127018922194, -0.25, -0.4330127018922194, 1.0, 0.5, 0.0, 0.25, 0.0, -0.5, -0.4330127018922194, 2.0, 0.5, 0.0, 0.25, 0.0, 0.5, -0.4330127018922194, 3.0],
    [0.25, 0.4330127018922193, 0.0, -0.4330127018922194, 0.25, 0.0, 0.0, 0.25, -0.4330127018922193, 0.5, 0.4330127018922194, 0.25, 0.0, 1.0, -0.5, 0.0, -0.5, 0.0, -0.5, 0.0, 2.0, -0.5, 0.0, 0.25, 0.0, -0.5, 0.4330127018922194, 3.0],
    [0.25, 0.4330127018922193, 0.0, -0.4330127018922194, 0.25, 0.0, 0.0, 0.5, 0.0, 0.25, 0.0, -0.5, 0.4330127018922194, 1.0, -0.25, 0.4330127018922193, 0.25, 0.4330127018922194, 0.25, -0.4330127018922194, 2.0, -0.5, 0.0, -0.75, 0.0, -0.5, 0.4330127018922194, 3.0],
    [-0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, 0.5, 0.5, -0.5, 0.0, 0.5, 1.0, -0.5, 0.0, 1.5, 0.0, -0.5, 0.0, 2.0, 0.5, 0.0, 0.0, 0.0, 0.5, -0.5, 3.0],
    [-0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.5, 0.0, -0.25, 0.0, 0.5, 1.299038105676658, 1.0, 0.25, 0.4330127018922193, -0.75, 0.4330127018922194, -0.25, -0.4330127018922194, 2.0, 0.25, 0.4330127018922193, -0.25, 0.4330127018922194, -0.25, 1.299038105676658, 3.0],
    [0.0, -0.5, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, -0.5, -0.5, 0.0, 2.0, 1.0, 0.0, 0.5, -1.0, -0.5, 0.0, 1.0, 2.0, 0.0, 0.5, -1.5, -0.5, 0.0, 0.0, 3.0],
    [-0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 0.0, -1.0, 1.0, -0.5, 0.0, 1.5, 0.0, -0.5, 0.0, 2.0, 0.0, -0.5, 0.5, -0.5, 0.0, -0.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.25, 0.4330127018922193, 0.5, 0.4330127018922194, 0.25, 0.0, 1.0, -0.5, 0.0, 0.5, 0.0, -0.5, 0.0, 2.0, -0.25, -0.4330127018922193, 0.5, 0.4330127018922194, -0.25, 0.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.5, 0.0, -1.5, 0.0, -0.5, 0.0, 1.0, -0.5, 0.0, -1.5, 0.0, 0.5, -0.5, 2.0, 0.5, 0.0, 0.0, 0.0, -0.5, 1.5, 3.0],
    [-0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, -1.0, 0.0, -0.5, -1.5, 1.0, -0.5, 0.0, 0.0, 0.0, -0.5, -1.0, 2.0, 0.5, 0.0, -1.0, 0.0, 0.5, -1.5, 3.0],
    [0.25, -0.4330127018922193, 0.0, 0.4330127018922194, 0.25, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, -0.5, 0.0, 1.0, -0.5, 0.0, 1.25, 0.0, -0.5, -1.299038105676658, 2.0, 0.25, 0.4330127018922193, 0.25, -0.4330127018922194, 0.25, 0.4330127018922194, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.25, 0.4330127018922193, -0.25, -0.4330127018922194, 0.25, 0.4330127018922194, 1.0, -0.5, 0.0, 0.5, 0.0, 0.5, 0.0, 2.0, -0.25, 0.4330127018922193, 0.25, -0.4330127018922194, -0.25, 0.4330127018922194, 3.0],
    [-0.25, 0.4330127018922193, 0.0, 0.4330127018922194, 0.25, 0.0, 0.0, 0.25, -0.4330127018922193, 0.5, 0.4330127018922194, 0.25, 0.0, 1.0, -0.5, 0.0, -0.5, 0.0, -0.5, 0.0, 2.0, -0.5, 0.0, 0.25, 0.0, -0.5, 0.4330127018922194, 3.0],
    [-0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, -0.5, -0.5, 0.5, 0.0, -1.0, 1.0, 0.0, 0.5, 0.5, -0.5, 0.0, 0.5, 2.0, -0.5, 0.0, 1.5, 0.0, -0.5, 0.0, 3.0],
    [0.0, -0.5, 0.0, -0.5, 0.0, 0.0, 0.0, 0.5, 0.0, -1.0, 0.0, 0.5, 0.0, 1.0, 0.0, 0.5, -1.5, 0.5, 0.0, 0.5, 2.0, 0.0, -0.5, -0.5, -0.5, 0.0, 0.5, 3.0],
    [0.0, -0.5, 0.0, -0.5, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.5, 0.0, 0.5, 1.0, 0.0, -0.5, -0.5, -0.5, 0.0, 0.0, 2.0, 0.0, 0.5, -0.5, 0.5, 0.0, 0.5, 3.0],
    [0.25, 0.4330127018922193, 0.0, -0.4330127018922194, 0.25, 0.0, 0.0, 0.5, 0.0, 0.25, 0.0, -0.5, 0.4330127018922194, 1.0, -0.25, 0.4330127018922193, 0.25, 0.4330127018922194, 0.25, -0.4330127018922194, 2.0, -0.5, 0.0, -0.75, 0.0, -0.5, 0.4330127018922194, 3.0],
    [0.25, 0.4330127018922193, 0.0, -0.4330127018922194, 0.25, 0.0, 0.0, 0.25, -0.4330127018922193, 0.5, 0.4330127018922194, 0.25, 0.0, 1.0, -0.5, 0.0, -0.5, 0.0, -0.5, 0.0, 2.0, -0.5, 0.0, 0.25, 0.0, -0.5, 0.4330127018922194, 3.0],
    [-0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.25, -0.4330127018922193, 0.25, -0.4330127018922194, 0.25, -0.4330127018922194, 1.0, -0.25, 0.4330127018922193, 0.25, -0.4330127018922194, -0.25, -0.4330127018922194, 2.0, 0.5, 0.0, -0.5, 0.0, 0.5, 0.0, 3.0],
    [-0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.5, -0.5, 0.0, -0.5, 1.0, -0.5, 0.0, -0.5, 0.0, 0.5, -0.5, 2.0, -0.5, 0.0, 0.0, 0.0, -0.5, -1.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, -1.5, 0.0, 0.5, -0.5, 1.0, 0.5, 0.0, 0.0, 0.0, 0.5, -2.0, 2.0, 0.5, 0.0, 0.5, 0.0, 0.5, -0.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.5, 0.0, -1.0, 0.0, 0.5, 0.0, 1.0, 0.0, 0.5, -0.5, -0.5, 0.0, -0.5, 2.0, -0.5, 0.0, -0.5, 0.0, -0.5, -0.5, 3.0],
    [-0.25, 0.4330127018922193, 0.0, 0.4330127018922194, 0.25, 0.0, 0.0, -0.25, 0.4330127018922193, -0.75, 0.4330127018922194, 0.25, 0.4330127018922194, 1.0, 0.5, 0.0, -0.5, 0.0, -0.5, 0.0, 2.0, 0.25, -0.4330127018922193, -0.5, 0.4330127018922194, 0.25, 0.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.5, 0.0, -0.5, 0.0, -0.5, 0.5, 1.0, 0.0, 0.5, -0.5, 0.5, 0.0, 1.0, 2.0, -0.5, 0.0, -0.5, 0.0, -0.5, 1.0, 3.0],
    [-0.25, 0.4330127018922193, 0.0, 0.4330127018922194, 0.25, 0.0, 0.0, -0.25, -0.4330127018922193, -0.25, -0.4330127018922194, 0.25, 0.4330127018922194, 1.0, 0.25, 0.4330127018922193, -0.25, -0.4330127018922194, 0.25, 0.4330127018922194, 2.0, -0.25, -0.4330127018922193, 0.5, -0.4330127018922194, 0.25, 0.0, 3.0],
    [0.25, 0.4330127018922193, 0.0, 0.4330127018922194, -0.25, 0.0, 0.0, 0.5, 0.0, -0.25, 0.0, -0.5, -0.4330127018922194, 1.0, 0.25, 0.4330127018922193, -0.25, 0.4330127018922194, -0.25, -0.4330127018922194, 2.0, 0.25, -0.4330127018922193, -0.25, -0.4330127018922194, -0.25, -0.4330127018922194, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.25, 0.4330127018922193, -0.25, 0.4330127018922194, 0.25, -0.4330127018922194, 1.0, 0.5, 0.0, 0.0, 0.0, -0.5, -0.8660254037844387, 2.0, -0.25, -0.4330127018922193, 0.25, -0.4330127018922194, 0.25, -0.4330127018922194, 3.0],
    [-0.25, 0.4330127018922193, 0.0, 0.4330127018922194, 0.25, 0.0, 0.0, -0.25, 0.4330127018922193, -0.75, 0.4330127018922194, 0.25, 0.4330127018922194, 1.0, 0.5, 0.0, -0.5, 0.0, -0.5, 0.0, 2.0, 0.25, -0.4330127018922193, -0.5, 0.4330127018922194, 0.25, 0.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.25, -0.4330127018922193, 0.5, -0.4330127018922194, 0.25, 0.0, 1.0, 0.5, 0.0, 0.5, 0.0, -0.5, 0.0, 2.0, -0.25, 0.4330127018922193, 0.5, 0.4330127018922194, 0.25, 0.0, 3.0],
    [-0.25, -0.4330127018922193, 0.0, -0.4330127018922194, 0.25, 0.0, 0.0, -0.25, 0.4330127018922193, -0.5, 0.4330127018922194, 0.25, 0.0, 1.0, -0.25, 0.4330127018922193, 0.25, 0.4330127018922194, 0.25, 0.4330127018922194, 2.0, -0.25, -0.4330127018922193, 0.25, -0.4330127018922194, 0.25, 0.4330127018922194, 3.0],
    [-0.25, 0.4330127018922193, 0.0, -0.4330127018922194, -0.25, 0.0, 0.0, 0.25, 0.4330127018922193, 0.75, -0.4330127018922194, 0.25, 0.4330127018922194, 1.0, 0.25, 0.4330127018922193, -0.25, -0.4330127018922194, 0.25, -0.4330127018922194, 2.0, -0.25, -0.4330127018922193, 0.5, 0.4330127018922194, -0.25, 0.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.25, 0.4330127018922193, -0.5, 0.4330127018922194, 0.25, 0.0, 1.0, -0.25, -0.4330127018922193, -0.25, -0.4330127018922194, 0.25, -0.4330127018922194, 2.0, -0.25, -0.4330127018922193, -0.5, 0.4330127018922194, -0.25, 0.0, 3.0],
    [-0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.5, -0.5, 0.0, -0.5, 1.0, -0.5, 0.0, -0.5, 0.0, 0.5, -0.5, 2.0, -0.5, 0.0, 0.0, 0.0, -0.5, -1.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, -1.5, 0.0, 0.5, -0.5, 1.0, 0.5, 0.0, 0.0, 0.0, 0.5, -2.0, 2.0, 0.5, 0.0, 0.5, 0.0, 0.5, -0.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.5, 0.0, -1.0, 0.0, 0.5, 0.0, 1.0, 0.0, 0.5, -0.5, -0.5, 0.0, -0.5, 2.0, -0.5, 0.0, -0.5, 0.0, -0.5, -0.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.25, -0.4330127018922193, -0.5, 0.4330127018922194, -0.25, 0.0, 1.0, -0.5, 0.0, -0.25, 0.0, 0.5, -0.4330127018922194, 2.0, -0.25, 0.4330127018922193, -0.25, -0.4330127018922194, -0.25, -0.4330127018922194, 3.0],
    [0.0, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, -0.5, -1.0, -0.5, 0.0, -1.5, 1.0, 0.0, 0.5, 0.0, -0.5, 0.0, -0.5, 2.0, 0.0, 0.5, 0.0, -0.5, 0.0, -1.0, 3.0],
    [0.0, 0.5, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0.5, 0.0, -1.0, 1.0, 0.0, 0.5, 0.0, -0.5, 0.0, -0.5, 2.0, 0.0, -0.5, -1.0, 0.5, 0.0, -0.5, 3.0],
    [0.0, -0.5, 0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 0.5, 1.5, -0.5, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5, -0.5, 0.0, 0.0, 2.0, 0.0, 0.5, 1.0, 0.5, 0.0, -1.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.25, 0.4330127018922193, -0.5, -0.4330127018922194, -0.25, 0.0, 1.0, -0.25, 0.4330127018922193, -0.25, 0.4330127018922194, 0.25, 0.4330127018922194, 2.0, -0.25, -0.4330127018922193, -0.25, -0.4330127018922194, -0.25, 0.4330127018922194, 3.0],
    [-0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.5, 0.0, 0.25, 0.0, -0.5, 0.4330127018922194, 1.0, 0.25, -0.4330127018922193, 0.25, 0.4330127018922194, 0.25, 0.4330127018922194, 2.0, 0.25, -0.4330127018922193, 0.25, 0.4330127018922194, 0.25, -0.4330127018922194, 3.0],
    [0.25, -0.4330127018922193, 0.0, 0.4330127018922194, 0.25, 0.0, 0.0, -0.25, 0.4330127018922193, 0.5, -0.4330127018922194, -0.25, 0.0, 1.0, -0.5, 0.0, 0.5, 0.0, 0.5, 0.0, 2.0, -0.25, -0.4330127018922193, 0.5, -0.4330127018922194, 0.25, 0.0, 3.0],
    [0.25, 0.4330127018922193, 0.0, -0.4330127018922194, 0.25, 0.0, 0.0, -0.25, -0.4330127018922193, 0.25, 0.4330127018922194, -0.25, 0.4330127018922194, 1.0, -0.25, -0.4330127018922193, 0.25, -0.4330127018922194, 0.25, 0.4330127018922194, 2.0, -0.5, 0.0, 0.25, 0.0, -0.5, 0.4330127018922194, 3.0],
    [0.5, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, -0.25, 0.4330127018922193, -0.5, 0.4330127018922194, 0.25, 0.0, 1.0, -0.25, -0.4330127018922193, -0.25, -0.4330127018922194, 0.25, -0.4330127018922194, 2.0, -0.25, -0.4330127018922193, -0.5, 0.4330127018922194, -0.25, 0.0, 3.0],
    [-0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5, 0.5, -0.5, 0.0, -0.5, 1.0, -0.5, 0.0, -0.5, 0.0, 0.5, -0.5, 2.0, -0.5, 0.0, 0.0, 0.0, -0.5, -1.0, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.5, 0.0, -1.5, 0.0, 0.5, -0.5, 1.0, 0.5, 0.0, 0.0, 0.0, 0.5, -2.0, 2.0, 0.5, 0.0, 0.5, 0.0, 0.5, -0.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.5, 0.0, -1.0, 0.0, 0.5, 0.0, 1.0, 0.0, 0.5, -0.5, -0.5, 0.0, -0.5, 2.0, -0.5, 0.0, -0.5, 0.0, -0.5, -0.5, 3.0],
    [0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, -0.25, -0.4330127018922193, -0.5, 0.4330127018922194, -0.25, 0.0, 1.0, -0.5, 0.0, -0.25, 0.0, 0.5, -0.4330127018922194, 2.0, -0.25, 0.4330127018922193, -0.25, -0.4330127018922194, -0.25, -0.4330127018922194, 3.0],
];

// d = [ifs_type, coloring_mode, color_speed, color1..color4, map table of the type (28)]
fn multi_ifs_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    let t = limit_value(ftoi(p[0]), 0, 73);
    d[0] = t as f64;
    d[1] = limit_value(ftoi(p[1]), 0, 1) as f64;
    d.extend_from_slice(&MULTI_IFS_TABLE[t as usize]);
    d
}

// ---------------------------------------------------------------------------------------------
// nBlur
// d = [numEdges, numStripes, ratioStripes, ratioHole, circumCircle, adjustToLinear, equalBlur,
//      exactCalc, highlightEdges (raw), 9 numEdges, 10 hasStripes, 11 negStripes,
//      12 |numStripes|, 13 midAngle, 14 angStripes, 15 angStart, 16 ratioComplement,
//      17 ratioHole, 18 tan90_m_2, 19 sina, 20 cosa, 21 exactCalc, 22 highlightEdges,
//      23 speedCalc1, 24 speedCalc2, 25 maxStripes, 26 arc_tan1, 27 arc_tan2, 28 ratioStripes,
//      29 circumCircle, 30 equalBlur, 31 adjustToLinear]

fn nblur_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    let mut num_edges = ftoi(p[0]);
    let num_stripes_raw = ftoi(p[1]);
    let ratio_stripes = limit_value(p[2], 0.0, 2.0);
    let mut ratio_hole = limit_value(p[3], 0.0, 1.0);
    let circum = limit_value(ftoi(p[4]), 0, 1);
    let adjust = limit_value(ftoi(p[5]), 0, 1);
    let equal = limit_value(ftoi(p[6]), 0, 1);
    let mut exact = limit_value(ftoi(p[7]), 0, 1);
    let mut highlight = p[8];
    if num_edges < 3 {
        num_edges = 3;
    }
    let has_stripes = num_stripes_raw != 0;
    let neg_stripes = num_stripes_raw < 0;
    let num_stripes = num_stripes_raw.wrapping_abs();
    let mid_angle = M_2PI / num_edges as f64;
    let (mut ang_stripes, mut ang_start, mut ratio_complement) = (0.0, 0.0, 0.0);
    if has_stripes {
        ang_stripes = mid_angle / (2 * num_stripes) as f64;
        ang_start = ang_stripes / 2.0;
        ratio_complement = 2.0 - ratio_stripes;
    }
    if ratio_hole > 0.95 && exact == 1 && circum == 0 {
        ratio_hole = 0.95;
    }
    let tan90_m_2 = (M_PI_2 + mid_angle / 2.0).tan();
    let angle = mid_angle / 2.0;
    if highlight <= 0.1 {
        highlight = 0.1;
    }
    if circum == 1 {
        exact = 0;
        highlight = 0.1;
    }
    let speed_calc1 = ratio_complement * ang_start;
    let speed_calc2 = ratio_stripes * ang_start;
    let max_stripes = (2 * num_stripes) as f64;
    let arc_tan1 = if neg_stripes { 7.5 } else { 13.0 } / (num_edges as f64).powf(1.3) * highlight;
    let arc_tan2 = 2.0 * (arc_tan1 / -2.0).atan();
    let b = |f: bool| if f { 1.0 } else { 0.0 };
    d.extend_from_slice(&[
        num_edges as f64,
        b(has_stripes),
        b(neg_stripes),
        num_stripes as f64,
        mid_angle,
        ang_stripes,
        ang_start,
        ratio_complement,
        ratio_hole,
        tan90_m_2,
        angle.sin(),
        angle.cos(),
        exact as f64,
        highlight,
        speed_calc1,
        speed_calc2,
        max_stripes,
        arc_tan1,
        arc_tan2,
        ratio_stripes,
        circum as f64,
        equal as f64,
        adjust as f64,
    ]);
    d
}

/// Java `randXY`: returns (x, y, lenXY, lenOuterEdges, lenInnerEdges).
fn nblur_rand_xy(d: &[f64], ctx: &mut Ctx) -> (f64, f64, f64, f64, f64) {
    let num_edges = d[9] as i32;
    let mid = d[13];
    let ang_stripes = d[14];
    let ang_start = d[15];
    let exact = d[21] as i32 == 1;
    let circum = d[29] as i32 == 1;
    let equal = d[30] as i32 == 1;
    let max_stripes = d[25];
    let mut ang_xy = if exact {
        ctx.random() * M_2PI
    } else {
        let t = (d[26] * (ctx.random() - 0.5)).atan() / d[27] + 0.5;
        (t + (ctx.random_int(32767) as i32 % num_edges) as f64) * mid
    };
    let mut x = ang_xy.sin();
    let mut y = ang_xy.cos();
    let mut ang_mem = ang_xy;
    while ang_xy > mid {
        ang_xy -= mid;
    }

    if d[10] != 0.0 {
        let mut ang_tmp = ang_start;
        let mut count: i32 = 0;
        while ang_xy > ang_tmp {
            ang_tmp += ang_stripes;
            if ang_tmp > mid {
                ang_tmp = mid;
            }
            count += 1;
        }
        if ang_tmp != mid {
            ang_tmp -= ang_start;
        }
        let neg = d[11] != 0.0;
        // negative stripes swap ratioStripes <-> ratioComplement and speedCalc1 <-> speedCalc2
        let (ratio, sc2) = if neg { (d[16], d[23]) } else { (d[28], d[24]) };
        let shift = |count: &mut i32, ang_xy: &mut f64, ang_mem: &mut f64, ang_tmp: &mut f64, x: &mut f64, y: &mut f64| {
            if *ang_xy > *ang_tmp && (!neg || *count as f64 != max_stripes) {
                *ang_xy += ang_start;
                *ang_mem += ang_start;
                *ang_tmp += ang_stripes;
                *count += 1;
            } else {
                *ang_xy -= ang_start;
                *ang_mem -= ang_start;
                *ang_tmp -= ang_stripes;
                *count -= 1;
            }
            *x = ang_mem.sin();
            *y = ang_mem.cos();
        };
        // the stripe parity that is shifted (first block) and stretched (ratio blocks)
        let shift_odd = !neg;
        if (count % 2 != 0) == shift_odd {
            shift(&mut count, &mut ang_xy, &mut ang_mem, &mut ang_tmp, &mut x, &mut y);
        }
        let stretch = (count % 2 != 0) != shift_odd;
        if stretch && ratio > 1.0 {
            if ang_xy > ang_tmp && count as f64 != max_stripes {
                ang_mem = ang_mem - ang_xy + ang_tmp + (ang_xy - ang_tmp) / ang_start * ratio * ang_start;
            } else {
                ang_mem = ang_mem - ang_xy + ang_tmp - (ang_tmp - ang_xy) / ang_start * ratio * ang_start;
            }
            ang_xy = ang_tmp + (ang_xy - ang_tmp) / ang_start * ratio * ang_start;
            x = ang_mem.sin();
            y = ang_mem.cos();
        }
        if stretch && ratio < 1.0 {
            let lower = |ang_xy: &mut f64, ang_tmp: f64, x: &mut f64, y: &mut f64| {
                if ang_tmp - *ang_xy > sc2 {
                    let rt = (ang_tmp - sc2 - *ang_xy) * sc2 / (ang_start - sc2);
                    let a = ang_mem - *ang_xy + ang_tmp - rt;
                    *x = a.sin();
                    *y = a.cos();
                    *ang_xy = ang_tmp - rt;
                }
            };
            if (ang_xy - ang_tmp).abs() > sc2 && count as f64 != max_stripes {
                if ang_xy - ang_tmp > sc2 {
                    let rt = (ang_xy - (ang_tmp + sc2)) * sc2 / (ang_start - sc2);
                    let a = ang_mem - ang_xy + ang_tmp + rt;
                    x = a.sin();
                    y = a.cos();
                    ang_xy = ang_tmp + rt;
                }
                lower(&mut ang_xy, ang_tmp, &mut x, &mut y);
            }
            if count as f64 == max_stripes {
                if neg {
                    ang_tmp = mid;
                }
                lower(&mut ang_xy, ang_tmp, &mut x, &mut y);
            }
        }
    }

    let tn = ang_xy.tan();
    let x_tmp = d[18] / (d[18] - tn);
    let y_tmp = x_tmp * tn;
    let len_outer = (x_tmp * x_tmp + y_tmp * y_tmp).sqrt();
    let ran = |ctx: &mut Ctx| if equal { ctx.random().sqrt() } else { ctx.random() };
    let mut ran_tmp = if exact || circum { ran(ctx) } else { ran(ctx) * len_outer };
    let len_inner = d[17] * len_outer;
    if !exact && ran_tmp < len_inner {
        ran_tmp = if circum {
            len_inner + ran(ctx) * (1.0 - len_inner + SMALL_EPSILON)
        } else {
            len_inner + ran(ctx) * (len_outer - len_inner)
        };
    }
    x *= ran_tmp;
    y *= ran_tmp;
    (x, y, (x * x + y * y).sqrt(), len_outer, len_inner)
}

/// Cap for the rejection loops of `exactCalc` (unbounded in Java).
const NBLUR_MAX_TRIES: usize = 1000;

fn nblur_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let mut amount = amount;
    if d[31] as i32 == 1 {
        let ne = d[9];
        let mid = d[13];
        if (ne as i32) % 4 == 0 {
            amount /= (2.0 - 2.0 * (mid * (ne / 2.0 - 1.0)).cos()).sqrt() / 2.0;
        } else {
            amount /= (2.0 - 2.0 * (mid * (ne / 2.0).floor()).cos()).sqrt() / 2.0;
        }
    }
    let mut r = nblur_rand_xy(d, ctx);
    let exact = d[21] as i32 == 1;
    let circum = d[29] as i32 == 1;
    if exact && !circum {
        for _ in 0..NBLUR_MAX_TRIES {
            if !(r.2 < r.4 || r.2 > r.3) {
                break;
            }
            r = nblur_rand_xy(d, ctx);
        }
    }
    if exact && circum {
        for _ in 0..NBLUR_MAX_TRIES {
            if r.2 >= r.4 {
                break;
            }
            r = nblur_rand_xy(d, ctx);
        }
    }
    let (sina, cosa) = (d[19], d[20]);
    v.x += amount * (cosa * r.0 - sina * r.1);
    v.y += amount * (sina * r.0 + cosa * r.1);
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// neuron3D
// d = [zoom, density, seed, radius, orbColor, type2_prob, radius2, orbColor2, noiseFreq,
//      noiseAmp, turbulence, connectorThickness, connectorProb, connectorTexture,
//      connectorTexFreq, lineColor, perlin permutation (256)]

const NEURON3D_PERM: usize = 16;

fn neuron3d_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    for k in [1, 4, 5, 7, 12, 15] {
        d[k] = java_max(0.0, java_min(1.0, p[k]));
    }
    let seed = p[2] as i32;
    d[2] = seed as f64;
    let mut rnd = JavaRandom::new(seed as i64);
    let mut perm: Vec<i32> = (0..256).collect();
    for i in (1..256).rev() {
        let index = rnd.next_int(i + 1) as usize;
        perm.swap(index, i as usize);
    }
    d.extend(perm.iter().map(|&v| v as f64));
    d
}

fn neuron3d_noise(d: &[f64], x: f64, y: f64, z: f64) -> f64 {
    let p = |i: i32| d[NEURON3D_PERM + (i & 255) as usize] as i32;
    let fade = |t: f64| t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
    let lerp = |t: f64, a: f64, b: f64| a + t * (b - a);
    let grad = |hash: i32, x: f64, y: f64, z: f64| {
        let h = hash & 15;
        let u = if h < 8 { x } else { y };
        let v = if h < 4 {
            y
        } else if h == 12 || h == 14 {
            x
        } else {
            z
        };
        (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
    };
    let (fx, fy, fz) = (x.floor(), y.floor(), z.floor());
    let (xi, yi, zi) = ((fx as i32) & 255, (fy as i32) & 255, (fz as i32) & 255);
    let (x, y, z) = (x - fx, y - fy, z - fz);
    let (u, v, w) = (fade(x), fade(y), fade(z));
    let a = p(xi) + yi;
    let aa = p(a) + zi;
    let ab = p(a + 1) + zi;
    let b = p(xi + 1) + yi;
    let ba = p(b) + zi;
    let bb = p(b + 1) + zi;
    lerp(
        w,
        lerp(
            v,
            lerp(u, grad(p(aa), x, y, z), grad(p(ba), x - 1.0, y, z)),
            lerp(u, grad(p(ab), x, y - 1.0, z), grad(p(bb), x - 1.0, y - 1.0, z)),
        ),
        lerp(
            v,
            lerp(u, grad(p(aa + 1), x, y, z - 1.0), grad(p(ba + 1), x - 1.0, y, z - 1.0)),
            lerp(u, grad(p(ab + 1), x, y - 1.0, z - 1.0), grad(p(bb + 1), x - 1.0, y - 1.0, z - 1.0)),
        ),
    )
}

/// 0 = no neuron, 1 / 2 = neuron type.
fn neuron3d_type(d: &[f64], ix: f64, iy: f64, iz: f64) -> i32 {
    let seed = d[2] as i64;
    let h = (ix as i64).wrapping_mul(73856093)
        ^ (iy as i64).wrapping_mul(19349663)
        ^ (iz as i64).wrapping_mul(83492791)
        ^ seed;
    let mut r = JavaRandom::new(h);
    if r.next_double() < d[1] {
        if r.next_double() < d[5] { 2 } else { 1 }
    } else {
        0
    }
}

fn neuron3d_apply(d: &[f64], ctx: &mut Ctx, _a: &mut Pt, v: &mut Pt, amount: f64) {
    let zoom = d[0];
    let (radius, radius2) = (d[3], d[6]);
    for _ in 0..20 {
        let ix = ((ctx.random() * 2.0 - 1.0) * zoom).floor();
        let iy = ((ctx.random() * 2.0 - 1.0) * zoom).floor();
        let iz = ((ctx.random() * 2.0 - 1.0) * zoom).floor();
        let start_type = neuron3d_type(d, ix, iy, iz);
        if start_type == 0 {
            continue;
        }
        let draw_connector = ctx.random() < 0.5;
        if draw_connector && d[11] > 0.0 {
            let dx = (ctx.random() * 3.0).floor() as i32 - 1;
            let dy = (ctx.random() * 3.0).floor() as i32 - 1;
            let dz = (ctx.random() * 3.0).floor() as i32 - 1;
            if dx == 0 && dy == 0 && dz == 0 {
                continue;
            }
            let (nx, ny, nz) = (ix + dx as f64, iy + dy as f64, iz + dz as f64);
            let neighbor_type = neuron3d_type(d, nx, ny, nz);
            if neighbor_type == 0 {
                continue;
            }
            let conn_hash = (ix as i64)
                .wrapping_mul(13)
                .wrapping_add((iy as i64).wrapping_mul(31))
                .wrapping_add((iz as i64).wrapping_mul(53))
                .wrapping_add((nx as i64).wrapping_mul(71))
                .wrapping_add((ny as i64).wrapping_mul(97))
                .wrapping_add((nz as i64).wrapping_mul(113))
                .wrapping_add(d[2] as i64);
            if JavaRandom::new(conn_hash).next_double() < d[12] {
                let start_radius = if start_type == 1 { radius } else { radius2 };
                let neighbor_radius = if neighbor_type == 1 { radius } else { radius2 };
                let c = [ix + 0.5, iy + 0.5, iz + 0.5];
                let n = [nx + 0.5, ny + 0.5, nz + 0.5];
                let to_n = v3_sub(n, c);
                let vec_len = v3_dot(to_n, to_n).sqrt();
                if vec_len > 1e-9 {
                    let norm = [to_n[0] / vec_len, to_n[1] / vec_len, to_n[2] / vec_len];
                    let start = [c[0] + norm[0] * start_radius, c[1] + norm[1] * start_radius, c[2] + norm[2] * start_radius];
                    let end = [n[0] - norm[0] * neighbor_radius, n[1] - norm[1] * neighbor_radius, n[2] - norm[2] * neighbor_radius];
                    let cv = v3_sub(end, start);
                    let t = ctx.random();
                    let pol = [start[0] + t * cv[0], start[1] + t * cv[1], start[2] + t * cv[2]];
                    let r = ctx.random() * d[11];
                    let angle = ctx.random() * 2.0 * M_PI;
                    let mut p1 = if cv[0].abs() < 1e-6 && cv[2].abs() < 1e-6 { [1.0, 0.0, 0.0] } else { [-cv[2], 0.0, cv[0]] };
                    let len1 = v3_dot(p1, p1).sqrt();
                    p1 = [p1[0] / len1, p1[1] / len1, p1[2] / len1];
                    let mut p2 = [
                        cv[1] * p1[2] - cv[2] * p1[1],
                        cv[2] * p1[0] - cv[0] * p1[2],
                        cv[0] * p1[1] - cv[1] * p1[0],
                    ];
                    let len2 = v3_dot(p2, p2).sqrt();
                    p2 = [p2[0] / len2, p2[1] / len2, p2[2] / len2];
                    let (sa, ca) = angle.sin_cos();
                    let mut f = [0.0; 3];
                    for k in 0..3 {
                        f[k] = pol[k] + r * (ca * p1[k] + sa * p2[k]);
                    }
                    if d[13] > 0.0 {
                        let na = t * d[14];
                        let h = conn_hash as f64;
                        let n1 = neuron3d_noise(d, na, h + 1.2, h + 3.4);
                        let n2 = neuron3d_noise(d, na, h + 5.6, h + 7.8);
                        for k in 0..3 {
                            f[k] += (p1[k] * n1 + p2[k] * n2) * d[13];
                        }
                    }
                    v.x = f[0] / zoom * amount;
                    v.y = f[1] / zoom * amount;
                    v.z = f[2] / zoom * amount;
                    v.color = d[15];
                    return;
                }
            }
        }
        let (current_radius, current_color) = if start_type == 1 { (radius, d[4]) } else { (radius2, d[7]) };
        let r = current_radius * ctx.random().cbrt();
        let theta = ctx.random() * 2.0 * M_PI;
        let phi = (2.0 * ctx.random() - 1.0).acos();
        let cx = r * phi.sin() * theta.cos();
        let cy = r * phi.sin() * theta.sin();
        let cz = r * phi.cos();
        let dist = (cx * cx + cy * cy + cz * cz).sqrt();
        let (nf, turb) = (d[8], d[10]);
        let freq = cx * nf;
        let qx = cx + turb * neuron3d_noise(d, freq, cy * nf, cz * nf);
        let qy = cy + turb * neuron3d_noise(d, freq + 5.2, cy * nf + 1.3, cz * nf);
        let qz = cz + turb * neuron3d_noise(d, freq + 8.7, cy * nf + 3.4, cz * nf + 4.6);
        let noise = neuron3d_noise(d, qx * nf, qy * nf, qz * nf);
        let new_dist = dist + noise * d[9];
        if dist > 1e-9 {
            let scale = new_dist / dist;
            v.x = (ix + 0.5 + cx * scale) / zoom * amount;
            v.y = (iy + 0.5 + cy * scale) / zoom * amount;
            v.z = (iz + 0.5 + cz * scale) / zoom * amount;
            v.color = current_color;
            return;
        }
    }
    v.x = 0.0;
    v.y = 0.0;
    v.z = 0.0;
}

// ---------------------------------------------------------------------------------------------
// nsudoku
// d = [Level, thickness, size, angle, type (raw), 5 levels built, 6 fill thickness, 7 size,
//      8 type, 9 cos(angle), 10 sin(angle), 11 level, colors (levels built * 81)]

const NSUDOKU_COLORS: usize = 12;
/// Java uses an unseeded `Random` for the board: a fixed seed keeps the port deterministic.
const NSUDOKU_BOARD_SEED: i64 = 1000;

fn nsudoku_legal(board: &[[i32; 9]; 9], x: usize, y: usize, current: i32) -> bool {
    for i in 0..9 {
        if board[x][i] == current || board[i][y] == current {
            return false;
        }
    }
    let (cx, cy) = (x / 3 * 3, y / 3 * 3);
    for i in cx..cx + 3 {
        for j in cy..cy + 3 {
            if board[i][j] == current {
                return false;
            }
        }
    }
    true
}

fn nsudoku_next_cell(board: &mut [[i32; 9]; 9], rnd: &mut JavaRandom, x: usize, y: usize) -> bool {
    let mut to_check = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    for i in (1..9).rev() {
        let current = rnd.next_int(i as i32) as usize;
        to_check.swap(current, i);
    }
    for &c in &to_check {
        if nsudoku_legal(board, x, y, c) {
            board[x][y] = c;
            let (nx, ny) = if x == 8 {
                if y == 8 {
                    return true;
                }
                (0, y + 1)
            } else {
                (x + 1, y)
            };
            if nsudoku_next_cell(board, rnd, nx, ny) {
                return true;
            }
        }
    }
    board[x][y] = 0;
    false
}

/// The 1296 row permutations of `tSudoku` (first 9 columns).
fn nsudoku_table() -> Vec<[i32; 9]> {
    const PERM: [[i32; 3]; 6] = [[1, 2, 3], [1, 3, 2], [2, 1, 3], [2, 3, 1], [3, 1, 2], [3, 2, 1]];
    let mut rows = Vec::with_capacity(1296);
    for n in 0..6 {
        for m in 0..6 {
            for l in 0..6 {
                for j in 0..6 {
                    let mut c = [0; 12];
                    c[0..3].copy_from_slice(&PERM[n]);
                    c[3..6].copy_from_slice(&PERM[m]);
                    c[6..9].copy_from_slice(&PERM[l]);
                    c[9..12].copy_from_slice(&PERM[j]);
                    let mut row = [0; 9];
                    for (i, r) in row.iter_mut().enumerate() {
                        *r = c[i] + 3 * c[9 + i / 3] - 3;
                    }
                    rows.push(row);
                }
            }
        }
    }
    rows
}

fn nsudoku_prepare(p: &[f64]) -> Vec<f64> {
    let level = limit_value(p[0], 1.0, 9.0) as i32;
    let thickness = 1.0 - limit_value(p[1], 0.0, 1.0);
    let size = limit_value(p[2], 0.3, 1.0);
    let ty = limit_value(p[4], 3.0, 6.0) as i32;
    let ang = p[3] * M_PI / 180.0;

    // board (SudokuGenerator.nextBoard(0))
    let mut board = [[0i32; 9]; 9];
    let mut brnd = JavaRandom::new(NSUDOKU_BOARD_SEED);
    nsudoku_next_cell(&mut board, &mut brnd, 0, 0);
    // static Random(1000): makeHoles(0) draws 81 numbers (no holes), then peek() uses it
    let mut rnd = JavaRandom::new(1000);
    for _ in 0..81 {
        rnd.next_double();
    }
    let vec_sudoku = |s: &[[i32; 9]; 9]| {
        let mut v = [0i32; 81];
        for i in 0..9 {
            for j in 0..9 {
                v[9 * i + j] = s[j][i];
            }
        }
        v
    };
    let mut total = nsudoku_table();
    let mut colors: Vec<f64> = Vec::new();
    let mut built = 0;
    for n in (1..=level).rev() {
        let (tran, vector) = if n == level {
            ([1, 2, 3, 4, 5, 6, 7, 8, 9], vec_sudoku(&board))
        } else {
            if total.is_empty() {
                // Java fails here (index out of bounds); keep the levels built so far
                break;
            }
            let peek = (rnd.next_double() * total.len() as f64) as usize;
            let tran = total[peek];
            let mut ns = [[0i32; 9]; 9];
            for i in 0..9 {
                ns[i] = board[(tran[i] - 1) as usize];
            }
            (tran, vec_sudoku(&ns))
        };
        colors.extend(vector.iter().map(|&c| c as f64 / 9.0));
        built += 1;
        total.retain(|row| row.iter().zip(tran.iter()).all(|(a, b)| a != b));
    }
    let mut d = p.to_vec();
    d.extend_from_slice(&[built as f64, thickness, size, ty as f64, ang.cos(), ang.sin(), level as f64]);
    d.extend(colors);
    d
}

fn nsudoku_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let built = d[5] as usize;
    let level = d[11];
    let idx = ((ctx.random() * (built * 81) as f64) as usize).min(built * 81 - 1);
    let (k, cell) = (idx / 81, idx % 81);
    let (i, j) = ((cell / 9) as f64, (cell % 9) as f64);
    let n = level - k as f64;
    let scale = d[7] * (n / level) / 9.0;
    let fill = if n == 1.0 { 0.0 } else { d[6] };
    let (px, py) = (i / 9.0 - 0.5, j / 9.0 - 0.5);
    // DrawFunc.plotPolygon / randXY
    let sides = d[8] as i32;
    let mid = M_2PI / sides as f64;
    let tan90 = (M_PI_2 + mid / 2.0).tan();
    let arc1 = 13.0 / (sides as f64).powf(1.3);
    let arc2 = 2.0 * (arc1 / -2.0).atan();
    let t = (arc1 * (ctx.random() - 0.5)).atan() / arc2 + 0.5;
    let mut ang_xy = (t + (ctx.random_int(32767) as i32 % sides) as f64) * mid;
    let mut x = ang_xy.sin();
    let mut y = ang_xy.cos();
    while ang_xy > mid {
        ang_xy -= mid;
    }
    let tn = ang_xy.tan();
    let x_tmp = tan90 / (tan90 - tn);
    let y_tmp = x_tmp * tn;
    let len_outer = (x_tmp * x_tmp + y_tmp * y_tmp).sqrt();
    let _ = ctx.random(); // first ranTmp, overwritten in Java
    let len_inner = fill * len_outer;
    let ran = len_inner + ctx.random().sqrt() * (len_outer - len_inner);
    x *= ran;
    y *= ran;
    let (sa, ca) = (mid / 2.0).sin_cos();
    let ox = ca * x - sa * y;
    let oy = sa * x + ca * y;
    let (rc, rs) = (d[9], d[10]);
    v.x += amount * (ox * scale * rc + oy * scale * rs + px);
    v.y += amount * (-ox * scale * rs + oy * scale * rc + py);
    v.color = d[NSUDOKU_COLORS + idx];
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// octapol
// d = [polarweight, radius, s, t, 4 a, 5 rad, 6.. points A..L (x, y)]

fn octapol_prepare(p: &[f64]) -> Vec<f64> {
    let (s, t) = (p[2], p[3]);
    let mut d = p.to_vec();
    d.push(s * 0.5 + t);
    d.push(0.707106781 * s * p[1].abs());
    let pts = [
        (-0.5 * s, 0.5 * s + t),  // A
        (0.5 * s, 0.5 * s + t),   // B
        (t, 0.5 * s),             // C
        (t, -0.5 * s),            // D
        (0.5 * s, -0.5 * s - t),  // E
        (-0.5 * s, -0.5 * s - t), // F
        (-t, -0.5 * s),           // G
        (-t, 0.5 * s),            // H
        (-0.5 * s, 0.5 * s),      // I
        (0.5 * s, 0.5 * s),       // J
        (-0.5 * s, -0.5 * s),     // K
        (0.5 * s, -0.5 * s),      // L
    ];
    for (x, y) in pts {
        d.push(x);
        d.push(y);
    }
    d
}

fn octapol_pt(d: &[f64], k: usize) -> (f64, f64) {
    (d[6 + 2 * k], d[7 + 2 * k])
}

fn octapol_hits_rect(tl: (f64, f64), br: (f64, f64), p: (f64, f64)) -> bool {
    p.0 >= tl.0 && p.1 >= tl.1 && p.0 <= br.0 && p.1 <= br.1
}

fn octapol_hits_triangle(a: (f64, f64), b: (f64, f64), c: (f64, f64), p: (f64, f64)) -> bool {
    let v0 = (c.0 - a.0, c.1 - a.1);
    let v1 = (b.0 - a.0, b.1 - a.1);
    let v2 = (p.0 - a.0, p.1 - a.1);
    let d00 = v0.0 * v0.0 + v0.1 * v0.1;
    let d01 = v0.0 * v1.0 + v0.1 * v1.1;
    let d02 = v0.0 * v2.0 + v0.1 * v2.1;
    let d11 = v1.0 * v1.0 + v1.1 * v1.1;
    let d12 = v1.0 * v2.0 + v1.1 * v2.1;
    let denom = d00 * d11 - d01 * d01;
    let (u, v) = if denom != 0.0 { ((d11 * d02 - d01 * d12) / denom, (d00 * d12 - d01 * d02) / denom) } else { (0.0, 0.0) };
    (u + v) < 1.0 && u > 0.0 && v > 0.0
}

fn octapol_apply(d: &[f64], _ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (x, y, z) = (a.x * 0.15, a.y * 0.15, a.z);
    let (aa, rad, pw) = (d[4], d[5], d[0]);
    let p = (x, y);
    let pt = |k: usize| octapol_pt(d, k);
    let (pa, pb, pc, pd, pe, pf, pg, ph, pi, pj, pk, pl) =
        (pt(0), pt(1), pt(2), pt(3), pt(4), pt(5), pt(6), pt(7), pt(8), pt(9), pt(10), pt(11));
    let r = (x * x + y * y).sqrt();
    if rad > 0.0 && r <= rad {
        let rd = sqr(r / rad).ln();
        let phi = y.atan2(x);
        v.x += amount * (x + rd * pw * (phi - x));
        v.y += amount * (y + rd * pw * (r - y));
    } else if x.abs() <= aa && y.abs() <= aa {
        if octapol_hits_rect(ph, pk, p)
            || octapol_hits_rect(pj, pd, p)
            || octapol_hits_rect(pa, pj, p)
            || octapol_hits_rect(pk, pe, p)
            || octapol_hits_triangle(pi, pa, ph, p)
            || octapol_hits_triangle(pj, pb, pc, p)
            || octapol_hits_triangle(pl, pd, pe, p)
            || octapol_hits_triangle(pk, pf, pg, p)
        {
            v.x += amount * x;
            v.y += amount * y;
        } else {
            v.x = 0.0;
            v.y = 0.0;
        }
    } else {
        v.x = 0.0;
        v.y = 0.0;
    }
    v.x += amount * x;
    v.y += amount * y;
    v.z += amount * z;
}

// ---------------------------------------------------------------------------------------------
// signed distance functions of the base shapes

fn ocappedcone3d_sd(p: V3, a: V3, b: V3, ra: f64, rb: f64) -> f64 {
    let rba = rb - ra;
    let ba = v3_sub(b, a);
    let pa = v3_sub(p, a);
    let baba = v3_dot(ba, ba);
    let papa = v3_dot(pa, pa);
    let paba = v3_dot(pa, ba) / baba;
    let x = (papa - paba * paba * baba).sqrt();
    let cax = g_max(0.0, x - if paba < 0.5 { ra } else { rb });
    let cay = (paba - 0.5).abs() - 0.5;
    let k = rba * rba + baba;
    let f = g_clamp((rba * (x - ra) + paba * baba) / k, 0.0, 1.0);
    let cbx = x - ra - f * rba;
    let cby = paba - f;
    let s = if cbx < 0.0 && cay < 0.0 { -1.0 } else { 1.0 };
    s * g_min(cax * cax + cay * cay * baba, cbx * cbx + cby * cby * baba).sqrt()
}

fn ocylinder3d_sd(p: V3, a: V3, b: V3, r: f64) -> f64 {
    let pa = v3_sub(p, a);
    let ba = v3_sub(b, a);
    let baba = v3_dot(ba, ba);
    let paba = v3_dot(pa, ba);
    let q = [pa[0] * baba - ba[0] * paba, pa[1] * baba - ba[1] * paba, pa[2] * baba - ba[2] * paba];
    let x = v3_dot(q, q).sqrt() - r * baba;
    let y = (paba - baba * 0.5).abs() - baba * 0.5;
    let x2 = x * x;
    let y2 = y * y * baba;
    let d = if g_max(x, y) < 0.0 {
        -g_min(x2, y2)
    } else {
        (if x > 0.0 { x2 } else { 0.0 }) + (if y > 0.0 { y2 } else { 0.0 })
    };
    java_signum(d) * d.abs().sqrt() / baba
}

fn oroundcone3d_sd(p: V3, a: V3, b: V3, r1: f64, r2: f64) -> f64 {
    let ba = v3_sub(b, a);
    let l2 = v3_dot(ba, ba);
    let rr = r1 - r2;
    let a2 = l2 - rr * rr;
    let il2 = 1.0 / l2;
    let pa = v3_sub(p, a);
    let y = v3_dot(pa, ba);
    let z = y - l2;
    let q = [pa[0] * l2 - ba[0] * y, pa[1] * l2 - ba[1] * y, pa[2] * l2 - ba[2] * y];
    let x2 = v3_dot(q, q);
    let y2 = y * y * l2;
    let z2 = z * z * l2;
    let k = java_signum(rr) * rr * rr * x2;
    if java_signum(z) * a2 * z2 > k {
        return (x2 + z2).sqrt() * il2 - r2;
    }
    if java_signum(y) * a2 * y2 < k {
        return (x2 + y2).sqrt() * il2 - r1;
    }
    ((x2 * a2 * il2).sqrt() + y * rr) * il2 - r1
}

/// `Math.signum` (0 for 0, NaN for NaN).
fn java_signum(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

fn octahedron3d_sd(p: V3, s: f64) -> f64 {
    let p = [p[0].abs(), p[1].abs(), p[2].abs()];
    let m = p[0] + p[1] + p[2] - s;
    let q = if 3.0 * p[0] < m {
        p
    } else if 3.0 * p[1] < m {
        [p[1], p[2], p[0]]
    } else if 3.0 * p[2] < m {
        [p[2], p[0], p[1]]
    } else {
        return m * 0.57735027;
    };
    let k = g_clamp(0.5 * (q[2] - q[1] + s), 0.0, s);
    let w = [q[0], q[1] - s + k, q[2] - k];
    v3_dot(w, w).sqrt()
}

fn octogonprism3d_sd(p: V3, r: f64, h: f64) -> f64 {
    let (kx, ky, kz) = (-0.9238795325, 0.3826834323, 0.4142135623);
    let (mut px, mut py, pz) = (p[0].abs(), p[1].abs(), p[2].abs());
    let t = 2.0 * g_min(kx * px + ky * py, 0.0);
    px -= kx * t;
    py -= ky * t;
    let t = 2.0 * g_min(-kx * px + ky * py, 0.0);
    px -= -kx * t;
    py -= ky * t;
    px -= g_clamp(px, -kz * r, kz * r);
    py -= r;
    let dx = (px * px + py * py).sqrt() * java_signum(py);
    let dy = pz - h;
    let mx = g_max(dx, 0.0);
    let my = g_max(dy, 0.0);
    g_min(g_max(dx, dy), 0.0) + (mx * mx + my * my).sqrt()
}

// ---------------------------------------------------------------------------------------------
// WGSL snippets with helpers

const NBLUR_WGSL: &str = r#"fn nBlur_p(base: u32, k: u32) -> f32 { return prm[base + k]; }
fn nBlur_rand_xy(base: u32) -> vec4<f32> {
  let num_edges = i32(nBlur_p(base, 9u));
  let mid = nBlur_p(base, 13u);
  let ang_stripes = nBlur_p(base, 14u);
  let ang_start = nBlur_p(base, 15u);
  let exact = i32(nBlur_p(base, 21u)) == 1;
  let circum = i32(nBlur_p(base, 29u)) == 1;
  let equal = i32(nBlur_p(base, 30u)) == 1;
  let max_stripes = nBlur_p(base, 25u);
  var ang_xy: f32;
  if (exact) {
    ang_xy = rnd() * TWO_PI;
  } else {
    let t = atan(nBlur_p(base, 26u) * (rnd() - 0.5)) / nBlur_p(base, 27u) + 0.5;
    let k = min(i32(rnd() * 32767.0), 32766) % num_edges;
    ang_xy = (t + f32(k)) * mid;
  }
  var x = sin(ang_xy);
  var y = cos(ang_xy);
  var ang_mem = ang_xy;
  while (ang_xy > mid) { ang_xy -= mid; }
  if (nBlur_p(base, 10u) != 0.0) {
    var ang_tmp = ang_start;
    var count = 0;
    while (ang_xy > ang_tmp && count < 100000) {
      ang_tmp += ang_stripes;
      if (ang_tmp > mid) { ang_tmp = mid; }
      count += 1;
    }
    if (ang_tmp != mid) { ang_tmp -= ang_start; }
    let neg = nBlur_p(base, 11u) != 0.0;
    var ratio = nBlur_p(base, 28u);
    var sc2 = nBlur_p(base, 24u);
    if (neg) {
      ratio = nBlur_p(base, 16u);
      sc2 = nBlur_p(base, 23u);
    }
    let shift_odd = !neg;
    if ((count % 2 != 0) == shift_odd) {
      if (ang_xy > ang_tmp && (!neg || f32(count) != max_stripes)) {
        ang_xy += ang_start;
        ang_mem += ang_start;
        ang_tmp += ang_stripes;
        count += 1;
      } else {
        ang_xy -= ang_start;
        ang_mem -= ang_start;
        ang_tmp -= ang_stripes;
        count -= 1;
      }
      x = sin(ang_mem);
      y = cos(ang_mem);
    }
    let stretch = (count % 2 != 0) != shift_odd;
    if (stretch && ratio > 1.0) {
      if (ang_xy > ang_tmp && f32(count) != max_stripes) {
        ang_mem = ang_mem - ang_xy + ang_tmp + (ang_xy - ang_tmp) / ang_start * ratio * ang_start;
      } else {
        ang_mem = ang_mem - ang_xy + ang_tmp - (ang_tmp - ang_xy) / ang_start * ratio * ang_start;
      }
      ang_xy = ang_tmp + (ang_xy - ang_tmp) / ang_start * ratio * ang_start;
      x = sin(ang_mem);
      y = cos(ang_mem);
    }
    if (stretch && ratio < 1.0) {
      if (abs(ang_xy - ang_tmp) > sc2 && f32(count) != max_stripes) {
        if (ang_xy - ang_tmp > sc2) {
          let rt = (ang_xy - (ang_tmp + sc2)) * sc2 / (ang_start - sc2);
          let an = ang_mem - ang_xy + ang_tmp + rt;
          x = sin(an);
          y = cos(an);
          ang_xy = ang_tmp + rt;
        }
        if (ang_tmp - ang_xy > sc2) {
          let rt = (ang_tmp - sc2 - ang_xy) * sc2 / (ang_start - sc2);
          let an = ang_mem - ang_xy + ang_tmp - rt;
          x = sin(an);
          y = cos(an);
          ang_xy = ang_tmp - rt;
        }
      }
      if (f32(count) == max_stripes) {
        if (neg) { ang_tmp = mid; }
        if (ang_tmp - ang_xy > sc2) {
          let rt = (ang_tmp - sc2 - ang_xy) * sc2 / (ang_start - sc2);
          let an = ang_mem - ang_xy + ang_tmp - rt;
          x = sin(an);
          y = cos(an);
          ang_xy = ang_tmp - rt;
        }
      }
    }
  }
  let tn = tan(ang_xy);
  let tan90 = nBlur_p(base, 18u);
  let x_tmp = tan90 / (tan90 - tn);
  let y_tmp = x_tmp * tn;
  let len_outer = sqrt(x_tmp * x_tmp + y_tmp * y_tmp);
  var ran_tmp = rnd();
  if (equal) { ran_tmp = sqrt(ran_tmp); }
  if (!exact && !circum) { ran_tmp *= len_outer; }
  let len_inner = nBlur_p(base, 17u) * len_outer;
  if (!exact && ran_tmp < len_inner) {
    var r2 = rnd();
    if (equal) { r2 = sqrt(r2); }
    if (circum) {
      ran_tmp = len_inner + r2 * (1.0 - len_inner + EPS);
    } else {
      ran_tmp = len_inner + r2 * (len_outer - len_inner);
    }
  }
  x *= ran_tmp;
  y *= ran_tmp;
  return vec4<f32>(x, y, sqrt(x * x + y * y), len_outer);
}
@body
var amt = AMOUNT;
if (i32(D(31)) == 1) {
  let ne = D(9);
  let mid = D(13);
  if (i32(ne) % 4 == 0) {
    amt /= sqrt(2.0 - 2.0 * cos(mid * (ne / 2.0 - 1.0))) / 2.0;
  } else {
    amt /= sqrt(2.0 - 2.0 * cos(mid * floor(ne / 2.0))) / 2.0;
  }
}
var r = nBlur_rand_xy(base);
let exact = i32(D(21)) == 1;
let circum = i32(D(29)) == 1;
if (exact) {
  for (var i = 0; i < 100; i++) {
    let inner = D(17) * r.w;
    if (circum) {
      if (r.z >= inner) { break; }
    } else if (!(r.z < inner || r.z > r.w)) {
      break;
    }
    r = nBlur_rand_xy(base);
  }
}
v.x += amt * (D(20) * r.x - D(19) * r.y);
v.y += amt * (D(19) * r.x + D(20) * r.y);
if (PRESERVE_Z) { v.z += amt * a.z; }"#;

const NEURON3D_WGSL: &str = r#"// java.util.Random emulated with 3 x 16 bit limbs (48 bit state)
fn neuron3D_mul48(a: vec3<u32>, b: vec3<u32>, add: u32) -> vec3<u32> {
  let p00 = a.x * b.x + add;
  let p01 = a.x * b.y;
  let p10 = a.y * b.x;
  let s1 = (p01 & 0xFFFFu) + (p10 & 0xFFFFu) + (p00 >> 16u);
  let c1 = (s1 >> 16u) + (p01 >> 16u) + (p10 >> 16u);
  let o2 = (a.x * b.z + a.y * b.y + a.z * b.x + c1) & 0xFFFFu;
  return vec3<u32>(p00 & 0xFFFFu, s1 & 0xFFFFu, o2);
}
fn neuron3D_from_i32(v: i32) -> vec3<u32> {
  let u = bitcast<u32>(v);
  return vec3<u32>(u & 0xFFFFu, u >> 16u, select(0u, 0xFFFFu, v < 0));
}
fn neuron3D_rng(h: vec3<u32>) -> vec3<u32> {
  return h ^ vec3<u32>(0xE66Du, 0xDEECu, 0x5u);
}
fn neuron3D_next_double(s: ptr<function, vec3<u32>>) -> f32 {
  let m = vec3<u32>(0xE66Du, 0xDEECu, 0x5u);
  *s = neuron3D_mul48(*s, m, 0xBu);
  let n26 = ((*s).z << 10u) | ((*s).y >> 6u);
  *s = neuron3D_mul48(*s, m, 0xBu);
  let n27 = ((*s).z << 11u) | ((*s).y >> 5u);
  return f32(n26) * (1.0 / 67108864.0) + f32(n27) * (1.0 / 9007199254740992.0);
}
fn neuron3D_type(base: u32, ix: i32, iy: i32, iz: i32) -> i32 {
  let h = neuron3D_mul48(neuron3D_from_i32(ix), vec3<u32>(0xF45Du, 0x0466u, 0u), 0u)
    ^ neuron3D_mul48(neuron3D_from_i32(iy), vec3<u32>(0x409Fu, 0x0127u, 0u), 0u)
    ^ neuron3D_mul48(neuron3D_from_i32(iz), vec3<u32>(0xFFB7u, 0x04F9u, 0u), 0u)
    ^ neuron3D_from_i32(i32(prm[base + 2u]));
  var s = neuron3D_rng(h);
  if (neuron3D_next_double(&s) < prm[base + 1u]) {
    if (neuron3D_next_double(&s) < prm[base + 5u]) { return 2; }
    return 1;
  }
  return 0;
}
fn neuron3D_perm(base: u32, i: i32) -> i32 { return i32(prm[base + 16u + u32(i & 255)]); }
fn neuron3D_fade(t: f32) -> f32 { return t * t * t * (t * (t * 6.0 - 15.0) + 10.0); }
fn neuron3D_grad(hash: i32, x: f32, y: f32, z: f32) -> f32 {
  let h = hash & 15;
  var u = y;
  if (h < 8) { u = x; }
  var w = z;
  if (h < 4) { w = y; } else if (h == 12 || h == 14) { w = x; }
  var r = -u;
  if ((h & 1) == 0) { r = u; }
  if ((h & 2) == 0) { r += w; } else { r -= w; }
  return r;
}
fn neuron3D_noise(base: u32, xin: f32, yin: f32, zin: f32) -> f32 {
  let fx = floor(xin);
  let fy = floor(yin);
  let fz = floor(zin);
  let xi = i32(fx) & 255;
  let yi = i32(fy) & 255;
  let zi = i32(fz) & 255;
  let x = xin - fx;
  let y = yin - fy;
  let z = zin - fz;
  let u = neuron3D_fade(x);
  let w = neuron3D_fade(y);
  let t = neuron3D_fade(z);
  let pa = neuron3D_perm(base, xi) + yi;
  let aa = neuron3D_perm(base, pa) + zi;
  let ab = neuron3D_perm(base, pa + 1) + zi;
  let pb = neuron3D_perm(base, xi + 1) + yi;
  let ba = neuron3D_perm(base, pb) + zi;
  let bb = neuron3D_perm(base, pb + 1) + zi;
  let g0 = mix(neuron3D_grad(neuron3D_perm(base, aa), x, y, z), neuron3D_grad(neuron3D_perm(base, ba), x - 1.0, y, z), u);
  let g1 = mix(neuron3D_grad(neuron3D_perm(base, ab), x, y - 1.0, z), neuron3D_grad(neuron3D_perm(base, bb), x - 1.0, y - 1.0, z), u);
  let g2 = mix(neuron3D_grad(neuron3D_perm(base, aa + 1), x, y, z - 1.0), neuron3D_grad(neuron3D_perm(base, ba + 1), x - 1.0, y, z - 1.0), u);
  let g3 = mix(neuron3D_grad(neuron3D_perm(base, ab + 1), x, y - 1.0, z - 1.0), neuron3D_grad(neuron3D_perm(base, bb + 1), x - 1.0, y - 1.0, z - 1.0), u);
  return mix(mix(g0, g1, w), mix(g2, g3, w), t);
}
@body
let zoom = D(0);
var done = false;
for (var i = 0; i < 20; i++) {
  let ix = floor((rnd() * 2.0 - 1.0) * zoom);
  let iy = floor((rnd() * 2.0 - 1.0) * zoom);
  let iz = floor((rnd() * 2.0 - 1.0) * zoom);
  let st = neuron3D_type(base, i32(ix), i32(iy), i32(iz));
  if (st == 0) { continue; }
  let draw_connector = rnd() < 0.5;
  if (draw_connector && D(11) > 0.0) {
    let dx = i32(floor(rnd() * 3.0)) - 1;
    let dy = i32(floor(rnd() * 3.0)) - 1;
    let dz = i32(floor(rnd() * 3.0)) - 1;
    if (dx == 0 && dy == 0 && dz == 0) { continue; }
    let nx = ix + f32(dx);
    let ny = iy + f32(dy);
    let nz = iz + f32(dz);
    let nt = neuron3D_type(base, i32(nx), i32(ny), i32(nz));
    if (nt == 0) { continue; }
    let ch = i32(ix) * 13 + i32(iy) * 31 + i32(iz) * 53 + i32(nx) * 71 + i32(ny) * 97 + i32(nz) * 113 + i32(D(2));
    var cs = neuron3D_rng(neuron3D_from_i32(ch));
    if (neuron3D_next_double(&cs) < D(12)) {
      var sr = D(6);
      if (st == 1) { sr = D(3); }
      var nr = D(6);
      if (nt == 1) { nr = D(3); }
      let c = vec3<f32>(ix, iy, iz) + 0.5;
      let n = vec3<f32>(nx, ny, nz) + 0.5;
      let to_n = n - c;
      let vec_len = length(to_n);
      if (vec_len > 1e-9) {
        let norm = to_n / vec_len;
        let sp = c + norm * sr;
        let ep = n - norm * nr;
        let cv = ep - sp;
        let t = rnd();
        let pol = sp + cv * t;
        let r = rnd() * D(11);
        let angle = rnd() * TWO_PI;
        var p1 = vec3<f32>(-cv.z, 0.0, cv.x);
        if (abs(cv.x) < 1e-6 && abs(cv.z) < 1e-6) { p1 = vec3<f32>(1.0, 0.0, 0.0); }
        p1 = p1 / length(p1);
        var p2 = cross(cv, p1);
        p2 = p2 / length(p2);
        var f = pol + r * (cos(angle) * p1 + sin(angle) * p2);
        if (D(13) > 0.0) {
          let na = t * D(14);
          let h = f32(ch);
          let n1 = neuron3D_noise(base, na, h + 1.2, h + 3.4);
          let n2 = neuron3D_noise(base, na, h + 5.6, h + 7.8);
          f += (p1 * n1 + p2 * n2) * D(13);
        }
        v.x = f.x / zoom * AMOUNT;
        v.y = f.y / zoom * AMOUNT;
        v.z = f.z / zoom * AMOUNT;
        v.color = D(15);
        done = true;
        break;
      }
    }
  }
  var cr = D(6);
  var cc = D(7);
  if (st == 1) {
    cr = D(3);
    cc = D(4);
  }
  let r = cr * pow(rnd(), 1.0 / 3.0);
  let theta = rnd() * TWO_PI;
  let phi = acos(2.0 * rnd() - 1.0);
  let cx = r * sin(phi) * cos(theta);
  let cy = r * sin(phi) * sin(theta);
  let cz = r * cos(phi);
  let dist = sqrt(cx * cx + cy * cy + cz * cz);
  let nf = D(8);
  let freq = cx * nf;
  let qx = cx + D(10) * neuron3D_noise(base, freq, cy * nf, cz * nf);
  let qy = cy + D(10) * neuron3D_noise(base, freq + 5.2, cy * nf + 1.3, cz * nf);
  let qz = cz + D(10) * neuron3D_noise(base, freq + 8.7, cy * nf + 3.4, cz * nf + 4.6);
  let noise = neuron3D_noise(base, qx * nf, qy * nf, qz * nf);
  let new_dist = dist + noise * D(9);
  if (dist > 1e-9) {
    let scale = new_dist / dist;
    v.x = (ix + 0.5 + cx * scale) / zoom * AMOUNT;
    v.y = (iy + 0.5 + cy * scale) / zoom * AMOUNT;
    v.z = (iz + 0.5 + cz * scale) / zoom * AMOUNT;
    v.color = cc;
    done = true;
    break;
  }
}
if (!done) {
  v.x = 0.0;
  v.y = 0.0;
  v.z = 0.0;
}"#;

const NSUDOKU_WGSL: &str = r#"let built = u32(D(5));
let nprim = built * 81u;
let idx = min(u32(rnd() * f32(nprim)), nprim - 1u);
let k = idx / 81u;
let cell = idx % 81u;
let level = D(11);
let n = level - f32(k);
let scale = D(7) * (n / level) / 9.0;
var fill = D(6);
if (n == 1.0) { fill = 0.0; }
let px = f32(cell / 9u) / 9.0 - 0.5;
let py = f32(cell % 9u) / 9.0 - 0.5;
let sides = i32(D(8));
let mid = TWO_PI / f32(sides);
let tan90 = tan(HALF_PI + mid / 2.0);
let arc1 = 13.0 / pow(f32(sides), 1.3);
let arc2 = 2.0 * atan(arc1 / -2.0);
let t = atan(arc1 * (rnd() - 0.5)) / arc2 + 0.5;
var ang = (t + f32(min(i32(rnd() * 32767.0), 32766) % sides)) * mid;
var x = sin(ang);
var y = cos(ang);
while (ang > mid) { ang -= mid; }
let tn = tan(ang);
let x_tmp = tan90 / (tan90 - tn);
let y_tmp = x_tmp * tn;
let len_outer = sqrt(x_tmp * x_tmp + y_tmp * y_tmp);
_ = rnd();
let len_inner = fill * len_outer;
let ran = len_inner + sqrt(rnd()) * (len_outer - len_inner);
x *= ran;
y *= ran;
let sa = sin(mid / 2.0);
let ca = cos(mid / 2.0);
let ox = ca * x - sa * y;
let oy = sa * x + ca * y;
v.x += AMOUNT * (ox * scale * D(9) + oy * scale * D(10) + px);
v.y += AMOUNT * (-ox * scale * D(10) + oy * scale * D(9) + py);
v.color = prm[base + 12u + idx];
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"#;

const OCTAPOL_WGSL: &str = r#"fn octapol_rect(tl: vec2<f32>, br: vec2<f32>, p: vec2<f32>) -> bool {
  return p.x >= tl.x && p.y >= tl.y && p.x <= br.x && p.y <= br.y;
}
fn octapol_tri(a: vec2<f32>, b: vec2<f32>, c: vec2<f32>, p: vec2<f32>) -> bool {
  let v0 = c - a;
  let v1 = b - a;
  let v2 = p - a;
  let d00 = dot(v0, v0);
  let d01 = dot(v0, v1);
  let d02 = dot(v0, v2);
  let d11 = dot(v1, v1);
  let d12 = dot(v1, v2);
  let denom = d00 * d11 - d01 * d01;
  var u = 0.0;
  var w = 0.0;
  if (denom != 0.0) {
    u = (d11 * d02 - d01 * d12) / denom;
    w = (d00 * d12 - d01 * d02) / denom;
  }
  return (u + w) < 1.0 && u > 0.0 && w > 0.0;
}
@body
let x = a.x * 0.15;
let y = a.y * 0.15;
let p = vec2<f32>(x, y);
let rad = D(5);
let r = sqrt(x * x + y * y);
if (rad > 0.0 && r <= rad) {
  let rd = log(sqr(r / rad));
  let phi = atan2(y, x);
  v.x += AMOUNT * (x + rd * D(0) * (phi - x));
  v.y += AMOUNT * (y + rd * D(0) * (r - y));
} else if (abs(x) <= D(4) && abs(y) <= D(4)) {
  let pa = vec2<f32>(D(6), D(7));
  let pb = vec2<f32>(D(8), D(9));
  let pc = vec2<f32>(D(10), D(11));
  let pd = vec2<f32>(D(12), D(13));
  let pe = vec2<f32>(D(14), D(15));
  let pf = vec2<f32>(D(16), D(17));
  let pg = vec2<f32>(D(18), D(19));
  let ph = vec2<f32>(D(20), D(21));
  let pi = vec2<f32>(D(22), D(23));
  let pj = vec2<f32>(D(24), D(25));
  let pk = vec2<f32>(D(26), D(27));
  let pl = vec2<f32>(D(28), D(29));
  if (octapol_rect(ph, pk, p) || octapol_rect(pj, pd, p) || octapol_rect(pa, pj, p) || octapol_rect(pk, pe, p)
      || octapol_tri(pi, pa, ph, p) || octapol_tri(pj, pb, pc, p) || octapol_tri(pl, pd, pe, p) || octapol_tri(pk, pf, pg, p)) {
    v.x += AMOUNT * x;
    v.y += AMOUNT * y;
  } else {
    v.x = 0.0;
    v.y = 0.0;
  }
} else {
  v.x = 0.0;
  v.y = 0.0;
}
v.x += AMOUNT * x;
v.y += AMOUNT * y;
v.z += AMOUNT * a.z;"#;

const OCAPPEDCONE3D_WGSL: &str = r#"fn ocappedcone3D_sd(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, ra: f32, rb: f32) -> f32 {
  let rba = rb - ra;
  let baba = dot(b - a, b - a);
  let papa = dot(p - a, p - a);
  let paba = dot(p - a, b - a) / baba;
  let x = sqrt(papa - paba * paba * baba);
  var rr = rb;
  if (paba < 0.5) { rr = ra; }
  let cax = max(0.0, x - rr);
  let cay = abs(paba - 0.5) - 0.5;
  let k = rba * rba + baba;
  let f = clamp((rba * (x - ra) + paba * baba) / k, 0.0, 1.0);
  let cbx = x - ra - f * rba;
  let cby = paba - f;
  var s = 1.0;
  if (cbx < 0.0 && cay < 0.0) { s = -1.0; }
  return s * sqrt(min(cax * cax + cay * cay * baba, cbx * cbx + cby * cby * baba));
}
@body
let pa = vec3<f32>(D(0), D(1), D(2));
let pb = vec3<f32>(D(3), D(4), D(5));
v.hide = 1.0;
for (var i = 0; i < 50; i++) {
  let q = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  if (ocappedcone3D_sd(q, pa, pb, D(6), D(7)) < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * q.x;
    v.y += AMOUNT * q.y;
    v.z += AMOUNT * q.z;
    break;
  }
}"#;

const OCYLINDER3D_WGSL: &str = r#"fn ocylinder3D_sd(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
  let pa = p - a;
  let ba = b - a;
  let baba = dot(ba, ba);
  let paba = dot(pa, ba);
  let x = length(pa * baba - ba * paba) - r * baba;
  let y = abs(paba - baba * 0.5) - baba * 0.5;
  let x2 = x * x;
  let y2 = y * y * baba;
  var d = 0.0;
  if (max(x, y) < 0.0) {
    d = -min(x2, y2);
  } else {
    if (x > 0.0) { d += x2; }
    if (y > 0.0) { d += y2; }
  }
  return sign(d) * sqrt(abs(d)) / baba;
}
@body
let pa = vec3<f32>(D(0), D(1), D(2));
let pb = vec3<f32>(D(3), D(4), D(5));
v.hide = 1.0;
for (var i = 0; i < 50; i++) {
  let q = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  if (ocylinder3D_sd(q, pa, pb, D(6)) < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * q.x;
    v.y += AMOUNT * q.y;
    v.z += AMOUNT * q.z;
    break;
  }
}"#;

const OROUNDCONE3D_WGSL: &str = r#"fn oroundcone3D_sd(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, r1: f32, r2: f32) -> f32 {
  let ba = b - a;
  let l2 = dot(ba, ba);
  let rr = r1 - r2;
  let a2 = l2 - rr * rr;
  let il2 = 1.0 / l2;
  let pa = p - a;
  let y = dot(pa, ba);
  let z = y - l2;
  let q = pa * l2 - ba * y;
  let x2 = dot(q, q);
  let y2 = y * y * l2;
  let z2 = z * z * l2;
  let k = sign(rr) * rr * rr * x2;
  if (sign(z) * a2 * z2 > k) { return sqrt(x2 + z2) * il2 - r2; }
  if (sign(y) * a2 * y2 < k) { return sqrt(x2 + y2) * il2 - r1; }
  return (sqrt(x2 * a2 * il2) + y * rr) * il2 - r1;
}
@body
let pa = vec3<f32>(D(0), D(1), D(2));
let pb = vec3<f32>(D(3), D(4), D(5));
v.hide = 1.0;
for (var i = 0; i < 50; i++) {
  let q = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  if (oroundcone3D_sd(q, pa, pb, D(6), D(7)) < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * q.x;
    v.y += AMOUNT * q.y;
    v.z += AMOUNT * q.z;
    break;
  }
}"#;

const OCTAHEDRON3D_WGSL: &str = r#"fn octahedron3D_sd(pin: vec3<f32>, s: f32) -> f32 {
  let p = abs(pin);
  let m = p.x + p.y + p.z - s;
  var q: vec3<f32>;
  if (3.0 * p.x < m) {
    q = p;
  } else if (3.0 * p.y < m) {
    q = p.yzx;
  } else if (3.0 * p.z < m) {
    q = p.zxy;
  } else {
    return m * 0.57735027;
  }
  let k = clamp(0.5 * (q.z - q.y + s), 0.0, s);
  return length(vec3<f32>(q.x, q.y - s + k, q.z - k));
}
@body
v.hide = 1.0;
for (var i = 0; i < 50; i++) {
  let q = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  if (octahedron3D_sd(q, D(0)) < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * q.x;
    v.y += AMOUNT * q.y;
    v.z += AMOUNT * q.z;
    break;
  }
}"#;

const OCTOGONPRISM3D_WGSL: &str = r#"fn octogonprism3D_sd(pin: vec3<f32>, r: f32, h: f32) -> f32 {
  let k = vec3<f32>(-0.9238795325, 0.3826834323, 0.4142135623);
  let pa = abs(pin);
  var p = pa.xy;
  p -= 2.0 * min(dot(k.xy, p), 0.0) * k.xy;
  p -= 2.0 * min(dot(vec2<f32>(-k.x, k.y), p), 0.0) * vec2<f32>(-k.x, k.y);
  p -= vec2<f32>(clamp(p.x, -k.z * r, k.z * r), r);
  let d = vec2<f32>(length(p) * sign(p.y), pa.z - h);
  return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0)));
}
@body
v.hide = 1.0;
for (var i = 0; i < 50; i++) {
  let q = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  if (octogonprism3D_sd(q, D(0), D(1)) < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * q.x;
    v.y += AMOUNT * q.y;
    v.z += AMOUNT * q.z;
    break;
  }
}"#;

fn limits_prepare(p: &[f64], lim: &[(f64, f64)]) -> Vec<f64> {
    p.iter().zip(lim.iter()).map(|(&v, &(lo, hi))| limit_value(v, lo, hi)).collect()
}

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "multi_ifs",
        params: &[
            ParamDef::int("ifs_type", 0.0, 0.0, 73.0),
            ParamDef::int("coloring_mode", 0.0, 0.0, 1.0),
            ParamDef::new("color_speed", 0.5),
            ParamDef::range("color1", 0.0, 0.0, 1.0),
            ParamDef::range("color2", 0.25, 0.0, 1.0),
            ParamDef::range("color3", 0.5, 0.0, 1.0),
            ParamDef::range("color4", 0.75, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = ctx.random();
            let k = if r < 0.25 {
                0
            } else if r < 0.5 {
                1
            } else if r < 0.75 {
                2
            } else {
                3
            };
            let t = &d[7 + 7 * k..14 + 7 * k];
            let nx = t[0] * a.x + t[1] * a.y + t[2];
            let ny = t[3] * a.x + t[4] * a.y + t[5];
            let new_color = d[3 + t[6] as usize];
            v.color = if d[1] as i32 == 1 { a.color + (new_color - a.color) * d[2] } else { new_color };
            v.x += nx * amount;
            v.y += ny * amount;
            // Java quirk: z is passed through only when preserve_z is *off*
            if !ctx.preserve_z {
                v.z += a.z * amount;
            }
        },
        prepare: Some(|p, _| multi_ifs_prepare(p)),
        wgsl: "let r = rnd();
         var k = 3u;
         if (r < 0.25) { k = 0u; } else if (r < 0.5) { k = 1u; } else if (r < 0.75) { k = 2u; }
         let o = base + 7u + 7u * k;
         let nx = prm[o] * a.x + prm[o + 1u] * a.y + prm[o + 2u];
         let ny = prm[o + 3u] * a.x + prm[o + 4u] * a.y + prm[o + 5u];
         let nc = prm[base + 3u + u32(prm[o + 6u])];
         if (i32(D(1)) == 1) { v.color = a.color + (nc - a.color) * D(2); } else { v.color = nc; }
         v.x += nx * AMOUNT;
         v.y += ny * AMOUNT;
         if (!PRESERVE_Z) { v.z += a.z * AMOUNT; }",
    },
    VarDef {
        name: "murl2",
        params: &[ParamDef::new("c", 0.1), ParamDef::int("power", 3.0, -10.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let c = d[0];
            let power = d[1] as i32 as f64;
            let p2 = power / 2.0;
            let (invp, vp) = if power != 0.0 {
                (1.0 / power, if c == -1.0 { 0.0 } else { amount * (c + 1.0).powf(2.0 / power) })
            } else {
                (100000000000.0, amount * (c + 1.0).powf(4.0))
            };
            let mut ang = a.y.atan2(a.x) * power;
            let mut r = c * (sqr(a.x) + sqr(a.y)).powf(p2);
            let mut re = r * ang.cos() + 1.0;
            let mut im = r * ang.sin();
            r = (sqr(re) + sqr(im)).powf(invp);
            ang = im.atan2(re) * 2.0 * invp;
            re = r * ang.cos();
            im = r * ang.sin();
            let rl = vp / sqr(r);
            v.x += rl * (a.x * re + a.y * im);
            v.y += rl * (a.y * re - a.x * im);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let c = D(0);
         let power = f32(i32(D(1)));
         let p2 = power / 2.0;
         var invp = 100000000000.0;
         var vp = AMOUNT * java_pow(c + 1.0, 4.0);
         if (power != 0.0) {
           invp = 1.0 / power;
           if (c == -1.0) { vp = 0.0; } else { vp = AMOUNT * java_pow(c + 1.0, 2.0 / power); }
         }
         var ang = atan2(a.y, a.x) * power;
         var r = c * java_pow(a.x * a.x + a.y * a.y, p2);
         var re = r * cos(ang) + 1.0;
         var im = r * sin(ang);
         r = java_pow(re * re + im * im, invp);
         ang = atan2(im, re) * 2.0 * invp;
         re = r * cos(ang);
         im = r * sin(ang);
         let rl = vp / (r * r);
         v.x += rl * (a.x * re + a.y * im);
         v.y += rl * (a.y * re - a.x * im);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "murl",
        params: &[ParamDef::new("c", 0.1), ParamDef::int("power", 1.0, -10.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let power = d[1] as i32 as f64;
            let mut c = d[0];
            if power != 1.0 {
                c /= power - 1.0;
            }
            let p2 = power / 2.0;
            let vp = amount * (c + 1.0);
            let ang = a.y.atan2(a.x) * power;
            let r = c * (sqr(a.x) + sqr(a.y)).powf(p2);
            let re = r * ang.cos() + 1.0;
            let im = r * ang.sin();
            let rl = vp / (sqr(re) + sqr(im));
            v.x += rl * (a.x * re + a.y * im);
            v.y += rl * (a.y * re - a.x * im);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let power = f32(i32(D(1)));
         var c = D(0);
         if (power != 1.0) { c /= power - 1.0; }
         let p2 = power / 2.0;
         let vp = AMOUNT * (c + 1.0);
         let ang = atan2(a.y, a.x) * power;
         let r = c * java_pow(a.x * a.x + a.y * a.y, p2);
         let re = r * cos(ang) + 1.0;
         let im = r * sin(ang);
         let rl = vp / (re * re + im * im);
         v.x += rl * (a.x * re + a.y * im);
         v.y += rl * (a.y * re - a.x * im);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "mushroom",
        params: &[ParamDef::new("cap_r", 0.5), ParamDef::new("cap_w", 2.0), ParamDef::new("stalk_w", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let cr = java_max(d[0].abs(), 0.01);
            let cw = java_max(d[1].abs(), 0.01);
            let sw = java_max(0.05, java_min(2.0, d[2]));
            let above = 0.5 + 0.5 * (a.y / (cr * 0.3)).tanh();
            let cap_env = (-(a.y - cr * 0.5) * (a.y - cr * 0.5) / (cr * cr * 0.5 + 1e-6)).exp();
            let scale = sw + (cw - sw) * above + cap_env * 0.4;
            v.x = amount * a.x * scale;
            v.y = amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let cr = max(abs(D(0)), 0.01);
         let cw = max(abs(D(1)), 0.01);
         let sw = max(0.05, min(2.0, D(2)));
         let above = 0.5 + 0.5 * tanh(clamp(a.y / (cr * 0.3), -20.0, 20.0));
         let cap_env = exp(-(a.y - cr * 0.5) * (a.y - cr * 0.5) / (cr * cr * 0.5 + 1e-6));
         let scale = sw + (cw - sw) * above + cap_env * 0.4;
         v.x = AMOUNT * a.x * scale;
         v.y = AMOUNT * a.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "nBlur",
        params: &[
            ParamDef::int("numEdges", 3.0, 3.0, 20.0),
            ParamDef::int("numStripes", 0.0, -10.0, 10.0),
            ParamDef::range("ratioStripes", 1.0, 0.0, 2.0),
            ParamDef::range("ratioHole", 0.0, 0.0, 1.0),
            ParamDef::int("circumCircle", 0.0, 0.0, 1.0),
            ParamDef::int("adjustToLinear", 1.0, 0.0, 1.0),
            ParamDef::int("equalBlur", 1.0, 0.0, 1.0),
            ParamDef::int("exactCalc", 0.0, 0.0, 1.0),
            ParamDef::range("highlightEdges", 1.0, 0.1, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: nblur_apply,
        prepare: Some(|p, _| nblur_prepare(p)),
        wgsl: NBLUR_WGSL,
    },
    VarDef {
        name: "nephroid",
        params: &[ParamDef::new("r", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let theta = a.y.atan2(a.x);
            v.x += amount * d[0] * (2.0 * theta.cos() - (2.0 * theta).cos());
            v.y += amount * d[0] * (2.0 * theta.sin() - (2.0 * theta).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let theta = atan2(a.y, a.x);
         v.x += AMOUNT * D(0) * (2.0 * cos(theta) - cos(2.0 * theta));
         v.y += AMOUNT * D(0) * (2.0 * sin(theta) - sin(2.0 * theta));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "neuron3D",
        params: &[
            ParamDef::range("zoom", 10.0, 1.0, 50.0),
            ParamDef::range("density", 0.5, 0.0, 1.0),
            ParamDef::int("seed", 12345.0, 0.0, 100000.0),
            ParamDef::range("radius", 0.4, 0.0, 1.0),
            ParamDef::range("orbColor", 0.25, 0.0, 1.0),
            ParamDef::range("type2_prob", 0.3, 0.0, 1.0),
            ParamDef::range("radius2", 0.25, 0.0, 1.0),
            ParamDef::range("orbColor2", 0.9, 0.0, 1.0),
            ParamDef::range("noiseFreq", 12.0, 0.0, 50.0),
            ParamDef::range("noiseAmp", 0.15, 0.0, 1.0),
            ParamDef::range("turbulence", 0.8, 0.0, 2.0),
            ParamDef::range("connectorThickness", 0.05, 0.0, 0.5),
            ParamDef::range("connectorProb", 0.6, 0.0, 1.0),
            ParamDef::range("connectorTexture", 0.1, 0.0, 1.0),
            ParamDef::range("connectorTexFreq", 5.0, 0.0, 20.0),
            ParamDef::range("lineColor", 0.75, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: neuron3d_apply,
        prepare: Some(|p, _| neuron3d_prepare(p)),
        wgsl: NEURON3D_WGSL,
    },
    VarDef {
        name: "npolar",
        params: &[ParamDef::int("parity", 0.0, 0.0, 1.0), ParamDef::int("n", 1.0, -12.0, 12.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let parity = ftoi(d[0]);
            let n = ftoi(d[1]);
            let nnz = if n == 0 { 1 } else { n };
            let vvar = amount / M_PI;
            let vvar_2 = vvar * 0.5;
            let absn = (nnz as f64).abs();
            let cn = 1.0 / nnz as f64 / 2.0;
            let isodd = parity.wrapping_abs() % 2;
            let x = if isodd != 0 { a.x } else { vvar * a.x.atan2(a.y) };
            let y = if isodd != 0 { a.y } else { vvar_2 * (a.x * a.x + a.y * a.y).ln() };
            let k = ctx.random_int(absn as usize) as f64;
            let angle = (y.atan2(x) + M_2PI * k) / nnz as f64;
            let r = amount * (sqr(x) + sqr(y)).powf(cn) * if isodd == 0 { 1.0 } else { parity as f64 };
            let sina = angle.sin() * r;
            let cosa = angle.cos() * r;
            let x = if isodd != 0 { cosa } else { vvar_2 * (cosa * cosa + sina * sina).ln() };
            let y = if isodd != 0 { sina } else { vvar * cosa.atan2(sina) };
            v.x += x;
            v.y += y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let parity = ftoi(D(0));
         let n = ftoi(D(1));
         var nnz = n;
         if (n == 0) { nnz = 1; }
         let vvar = AMOUNT / PI;
         let vvar_2 = vvar * 0.5;
         let absn = abs(f32(nnz));
         let cn = 1.0 / f32(nnz) / 2.0;
         let isodd = abs(parity) % 2;
         var x = vvar * atan2(a.x, a.y);
         var y = vvar_2 * log(a.x * a.x + a.y * a.y);
         if (isodd != 0) { x = a.x; y = a.y; }
         let k = f32(min(i32(rnd() * absn), i32(absn) - 1));
         let angle = (atan2(y, x) + TWO_PI * k) / f32(nnz);
         var r = AMOUNT * java_pow(x * x + y * y, cn);
         if (isodd != 0) { r *= f32(parity); }
         let sina = sin(angle) * r;
         let cosa = cos(angle) * r;
         if (isodd != 0) {
           x = cosa;
           y = sina;
         } else {
           x = vvar_2 * log(cosa * cosa + sina * sina);
           y = vvar * atan2(cosa, sina);
         }
         v.x += x;
         v.y += y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "nsudoku",
        params: &[
            ParamDef::int("Level", 5.0, 1.0, 9.0),
            ParamDef::range("thickness", 0.5, 0.0, 1.0),
            ParamDef::range("size", 0.5, 0.3, 1.0),
            ParamDef::range("angle", 0.0, -180.0, 180.0),
            ParamDef::int("type", 4.0, 3.0, 6.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: nsudoku_apply,
        prepare: Some(|p, _| nsudoku_prepare(p)),
        wgsl: NSUDOKU_WGSL,
    },
    VarDef {
        name: "oak_leaf",
        params: &[ParamDef::range("filled", 0.25, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let t = -a.atan_xy() * 0.5 - M_PI_2;
            let filled = d[0];
            let r = if filled > 0.0 && filled > ctx.random() { amount * ctx.random() * 2.0 } else { amount * 2.0 };
            let lx = t.cos().powf(9.0) / 100.0 * (5.0 * t).cos().powf(10.0)
                + (2.0 * t).sin() / 4.0
                    * (1.0 - (10.0 * t).sin().powf(2.0) / 2.0)
                    * (1.0 - (t.cos() * (3.0 * t).cos()).powf(8.0));
            let ly = t.sin() * (1.0 - (10.0 * t).sin().powf(2.0) / 5.0 * (0.5 + (2.0 * t).sin().powf(2.0)));
            v.x += r * lx;
            v.y += r * ly;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let t = -atan_xy(a) * 0.5 - HALF_PI;
         var r = AMOUNT * 2.0;
         if (D(0) > 0.0) {
           if (D(0) > rnd()) { r = AMOUNT * rnd() * 2.0; }
         }
         let ct = cos(t);
         let c5 = cos(5.0 * t);
         let c3 = ct * cos(3.0 * t);
         let s10 = sin(10.0 * t);
         let s2 = sin(2.0 * t);
         let ct3 = ct * ct * ct;
         let c55 = c5 * c5 * c5 * c5 * c5;
         let c34 = c3 * c3 * c3 * c3;
         let lx = ct3 * ct3 * ct3 / 100.0 * c55 * c55 + s2 / 4.0 * (1.0 - s10 * s10 / 2.0) * (1.0 - c34 * c34);
         let ly = sin(t) * (1.0 - s10 * s10 / 5.0 * (0.5 + s2 * s2));
         v.x += r * lx;
         v.y += r * ly;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "ocappedcone3D",
        params: &[
            ParamDef::range("dx", 0.0, -1.0, 1.0),
            ParamDef::range("dy", 0.0, -1.0, 1.0),
            ParamDef::range("dz", 0.1, -1.0, 1.0),
            ParamDef::range("tx", 0.0, -1.0, 1.0),
            ParamDef::range("ty", 0.0, -1.0, 1.0),
            ParamDef::range("tz", 0.5, -1.0, 1.0),
            ParamDef::range("r1", 0.25, 0.0, 1.0),
            ParamDef::range("r2", 0.05, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            let (a3, b3) = ([d[0], d[1], d[2]], [d[3], d[4], d[5]]);
            sdf_shape_sample(ctx, v, amount, |p| ocappedcone3d_sd(p, a3, b3, d[6], d[7]));
        },
        prepare: Some(|p, _| {
            const L: &[(f64, f64)] = &[(-1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0), (0.0, 1.0), (0.0, 1.0)];
            limits_prepare(p, L)
        }),
        wgsl: OCAPPEDCONE3D_WGSL,
    },
    VarDef {
        name: "octagon",
        params: &[
            ParamDef::new("x", 0.0),
            ParamDef::new("y", 0.0),
            ParamDef::new("z", 0.0),
            ParamDef::int("mode", 1.0, 0.0, 6.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            let mode = d[3] as i32;
            let mut r = sqr(sqr(a.x)) + sqr(sqr(a.y)) + 2.0 * sqr(a.z);
            if r == 0.0 {
                r = SMALL_EPSILON;
            }
            let mut t = a.x.abs() + a.y.abs() + 2.0 * (if mode > 1 { a.z.abs() } else { a.z }).sqrt();
            if t == 0.0 {
                t = SMALL_EPSILON;
            }
            let h = amount / 2.0;
            let (m, splits) = match mode {
                0 => {
                    if r <= h { (1.0 + 1.0 / t, true) } else { (1.0 / r, false) }
                }
                1 => {
                    if r <= h {
                        (1.0 + 1.0 / t, true)
                    } else if amount >= 0.0 {
                        (1.0 / r + 1.0 / t, true)
                    } else {
                        (1.0 + 1.0 / r, true)
                    }
                }
                2 => {
                    if r <= h { (1.0 + 1.0 / t, true) } else { (1.0 + 1.0 / r, false) }
                }
                3 => {
                    if r <= h { (1.0 / t, true) } else { (1.0 / r, false) }
                }
                4 => (1.0 / r, r <= h),
                5 => {
                    if t <= h { (1.0 / t, true) } else { (1.0 / r, false) }
                }
                6 => {
                    if t <= h { (1.0 + 1.0 / t, true) } else { (1.0 + 1.0 / r, false) }
                }
                _ => (1.0, false),
            };
            v.x += amount * m * a.x;
            v.y += amount * m * a.y;
            v.z += amount * m * a.z;
            if splits {
                v.x += amount * if a.x >= 0.0 { a.x + d[0] } else { a.x - d[0] };
                v.y += amount * if a.y >= 0.0 { a.y + d[1] } else { a.y - d[1] };
                v.z += amount * if a.z >= 0.0 { a.z + d[2] } else { a.z - d[2] };
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[3] = limit_value(ftoi(p[3]), 0, 6) as f64;
            d
        }),
        wgsl: "let mode = i32(D(3));
         var r = sqr(sqr(a.x)) + sqr(sqr(a.y)) + 2.0 * sqr(a.z);
         if (r == 0.0) { r = EPS; }
         var zz = a.z;
         if (mode > 1) { zz = abs(a.z); }
         var t = abs(a.x) + abs(a.y) + 2.0 * sqrt(zz);
         if (t == 0.0) { t = EPS; }
         let h = AMOUNT / 2.0;
         var m = 1.0;
         var splits = false;
         switch (mode) {
           case 0: { if (r <= h) { m = 1.0 + 1.0 / t; splits = true; } else { m = 1.0 / r; } }
           case 1: {
             splits = true;
             if (r <= h) { m = 1.0 + 1.0 / t; } else if (AMOUNT >= 0.0) { m = 1.0 / r + 1.0 / t; } else { m = 1.0 + 1.0 / r; }
           }
           case 2: { if (r <= h) { m = 1.0 + 1.0 / t; splits = true; } else { m = 1.0 + 1.0 / r; } }
           case 3: { if (r <= h) { m = 1.0 / t; splits = true; } else { m = 1.0 / r; } }
           case 4: { m = 1.0 / r; splits = r <= h; }
           case 5: { if (t <= h) { m = 1.0 / t; splits = true; } else { m = 1.0 / r; } }
           case 6: { if (t <= h) { m = 1.0 + 1.0 / t; splits = true; } else { m = 1.0 + 1.0 / r; } }
           default: {}
         }
         v.x += AMOUNT * m * a.x;
         v.y += AMOUNT * m * a.y;
         v.z += AMOUNT * m * a.z;
         if (splits) {
           if (a.x >= 0.0) { v.x += AMOUNT * (a.x + D(0)); } else { v.x += AMOUNT * (a.x - D(0)); }
           if (a.y >= 0.0) { v.y += AMOUNT * (a.y + D(1)); } else { v.y += AMOUNT * (a.y - D(1)); }
           if (a.z >= 0.0) { v.z += AMOUNT * (a.z + D(2)); } else { v.z += AMOUNT * (a.z - D(2)); }
         }",
    },
    VarDef {
        name: "octahedron3D",
        params: &[ParamDef::range("h", 0.25, 0.0, 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            sdf_shape_sample(ctx, v, amount, |p| octahedron3d_sd(p, d[0]));
        },
        prepare: Some(|p, _| limits_prepare(p, &[(0.0, 0.5)])),
        wgsl: OCTAHEDRON3D_WGSL,
    },
    VarDef {
        name: "octapol",
        params: &[
            ParamDef::new("polarweight", 0.0),
            ParamDef::new("radius", 1.0),
            ParamDef::new("s", 0.5),
            ParamDef::new("t", 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: octapol_apply,
        prepare: Some(|p, _| octapol_prepare(p)),
        wgsl: OCTAPOL_WGSL,
    },
    VarDef {
        name: "octogonprism3D",
        params: &[ParamDef::range("R", 0.25, 0.0, 0.5), ParamDef::range("H", 0.5, 0.0, 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            sdf_shape_sample(ctx, v, amount, |p| octogonprism3d_sd(p, d[0], d[1]));
        },
        prepare: Some(|p, _| limits_prepare(p, &[(0.0, 0.5), (0.0, 0.5)])),
        wgsl: OCTOGONPRISM3D_WGSL,
    },
    VarDef {
        name: "ocylinder3D",
        params: &[
            ParamDef::range("dx", 1.0, -1.0, 1.0),
            ParamDef::range("dy", 0.0, -1.0, 1.0),
            ParamDef::range("dz", 0.0, -1.0, 1.0),
            ParamDef::range("tx", 0.0, -0.5, 0.5),
            ParamDef::range("ty", 0.0, -0.5, 0.5),
            ParamDef::range("tz", 0.0, -0.5, 0.5),
            ParamDef::range("r", 0.5, 0.0, 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            let (a3, b3) = ([d[0], d[1], d[2]], [d[3], d[4], d[5]]);
            sdf_shape_sample(ctx, v, amount, |p| ocylinder3d_sd(p, a3, b3, d[6]));
        },
        prepare: Some(|p, _| {
            const L: &[(f64, f64)] = &[(-1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0), (-0.5, 0.5), (-0.5, 0.5), (-0.5, 0.5), (0.0, 0.5)];
            limits_prepare(p, L)
        }),
        wgsl: OCYLINDER3D_WGSL,
    },
    VarDef {
        name: "onion2",
        params: &[
            ParamDef::new("meeting_pt", 0.5),
            ParamDef::new("circle_a", 1.0),
            ParamDef::new("circle_b", 1.0),
            ParamDef::new("shift_x", 0.0),
            ParamDef::new("shift_y", 0.0),
            ParamDef::new("shift_z", 0.0),
            ParamDef::new("top_crop", 0.0),
            ParamDef::range("stretch", 1.0, 0.01, 4.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            let (mp, ca, cb, sx, sy, sz, top, stretch) = (d[0], d[1], d[2], d[3], d[4], d[5], d[6], d[7]);
            let (ix, iy, iz) = (a.x - sx, a.y - sy, a.z - sz);
            let rr = (ix * ix + iy * iy).sqrt() + SMALL_EPSILON;
            let t = rr / stretch - M_PI / 2.0;
            let (mut r, mut z);
            if t > mp {
                r = t.cos();
                let tm = mp.tan();
                if tm != 0.0 {
                    z = (mp.cos() - r).exp() / tm + mp.sin() - 1.0 / tm;
                    if z > top && top > 0.0 {
                        z = top;
                        r = 0.0;
                    }
                } else {
                    z = top;
                }
            } else {
                r = t.cos();
                z = t.sin();
            }
            r *= ca * amount;
            z *= cb * amount;
            v.x += r * (ix / rr);
            v.y += r * (iy / rr);
            v.z += z;
            v.x += r * iz * (ix / rr);
            v.y += r * iz * (iy / rr);
            v.z += iz * z / (r * r + z * z);
            v.x += sx;
            v.y += sy;
            v.z += sz;
        },
        // setParameter ignores stretch <= 0 (the default 1 stays)
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            if d[7] <= 0.0 {
                d[7] = 1.0;
            }
            d
        }),
        wgsl: "let ix = a.x - D(3);
         let iy = a.y - D(4);
         let iz = a.z - D(5);
         let rr = sqrt(ix * ix + iy * iy) + EPS;
         let t = rr / D(7) - HALF_PI;
         let mp = D(0);
         let top = D(6);
         var r = cos(t);
         var z = sin(t);
         if (t > mp) {
           let tm = tan(mp);
           if (tm != 0.0) {
             z = exp(cos(mp) - r) / tm + sin(mp) - 1.0 / tm;
             if (z > top && top > 0.0) {
               z = top;
               r = 0.0;
             }
           } else {
             z = top;
           }
         }
         r *= D(1) * AMOUNT;
         z *= D(2) * AMOUNT;
         v.x += r * (ix / rr);
         v.y += r * (iy / rr);
         v.z += z;
         v.x += r * iz * (ix / rr);
         v.y += r * iz * (iy / rr);
         v.z += iz * z / (r * r + z * z);
         v.x += D(3);
         v.y += D(4);
         v.z += D(5);",
    },
    VarDef {
        name: "onion",
        params: &[ParamDef::new("centre_x", 0.0), ParamDef::new("centre_y", 0.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            let r0 = if amount == 0.0 { 1.0 } else { amount };
            let x0 = a.x - d[0];
            let y0 = a.y - d[1];
            let d0 = x0 * x0 + y0 * y0;
            let dr = d0.sqrt();
            let (mut x1, mut y1, z1);
            if d0 <= r0 * r0 {
                z1 = -(r0 * r0 - d0).sqrt();
                x1 = x0;
                y1 = y0;
            } else if 2.0 * r0 - dr > r0 / 1.41421356 {
                x1 = (2.0 * r0 - dr) * x0 / dr;
                y1 = (2.0 * r0 - dr) * y0 / dr;
                z1 = (r0 * r0 - (x1 * x1 + y1 * y1)).sqrt();
            } else {
                z1 = (dr - r0 - (r0 - r0 / 1.414213569)).exp() - 1.0 + r0 / 1.414213569;
                x1 = (2.0 * r0 - dr) * x0 / dr;
                y1 = (2.0 * r0 - dr) * y0 / dr;
            }
            x1 += d[0];
            y1 += d[1];
            v.x += x1;
            v.y += y1;
            v.z += z1 + a.z;
        },
        prepare: None,
        wgsl: "var r0 = AMOUNT;
         if (r0 == 0.0) { r0 = 1.0; }
         let x0 = a.x - D(0);
         let y0 = a.y - D(1);
         let d0 = x0 * x0 + y0 * y0;
         let dr = sqrt(d0);
         var x1 = x0;
         var y1 = y0;
         var z1 = 0.0;
         if (d0 <= r0 * r0) {
           z1 = -sqrt(r0 * r0 - d0);
         } else if (2.0 * r0 - dr > r0 / 1.41421356) {
           x1 = (2.0 * r0 - dr) * x0 / dr;
           y1 = (2.0 * r0 - dr) * y0 / dr;
           z1 = sqrt(r0 * r0 - (x1 * x1 + y1 * y1));
         } else {
           z1 = exp(dr - r0 - (r0 - r0 / 1.414213569)) - 1.0 + r0 / 1.414213569;
           x1 = (2.0 * r0 - dr) * x0 / dr;
           y1 = (2.0 * r0 - dr) * y0 / dr;
         }
         v.x += x1 + D(0);
         v.y += y1 + D(1);
         v.z += z1 + a.z;",
    },
    VarDef {
        name: "oroundcone3D",
        params: &[
            ParamDef::range("dx", 0.1, -1.0, 1.0),
            ParamDef::range("dy", 0.0, -1.0, 1.0),
            ParamDef::range("dz", 0.0, -1.0, 1.0),
            ParamDef::range("tx", -0.1, -0.5, 0.5),
            ParamDef::range("ty", 0.35, -0.5, 0.5),
            ParamDef::range("tz", 0.1, -0.5, 0.5),
            ParamDef::range("r1", 0.15, 0.0, 0.5),
            ParamDef::range("r2", 0.05, 0.0, 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            let (a3, b3) = ([d[0], d[1], d[2]], [d[3], d[4], d[5]]);
            sdf_shape_sample(ctx, v, amount, |p| oroundcone3d_sd(p, a3, b3, d[6], d[7]));
        },
        prepare: Some(|p, _| {
            const L: &[(f64, f64)] = &[(-1.0, 1.0), (-1.0, 1.0), (-1.0, 1.0), (-0.5, 0.5), (-0.5, 0.5), (-0.5, 0.5), (0.0, 0.5), (0.0, 0.5)];
            limits_prepare(p, L)
        }),
        wgsl: OROUNDCONE3D_WGSL,
    },
    VarDef {
        name: "ortho",
        params: &[ParamDef::new("in", 0.0), ParamDef::new("out", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (pin, pout) = (d[0], d[1]);
            let r = sqr(a.x) + sqr(a.y);
            if r < 1.0 {
                if a.x >= 0.0 {
                    let xo = (r + 1.0) / (2.0 * a.x);
                    let ro = (sqr(a.x - xo) + sqr(a.y)).sqrt();
                    let theta = 1.0f64.atan2(ro);
                    let an = fmod(pin * theta + a.y.atan2(xo - a.x) + theta, 2.0 * theta) - theta;
                    v.x += amount * (xo - an.cos() * ro);
                    v.y += amount * an.sin() * ro;
                } else {
                    let xo = -(r + 1.0) / (2.0 * a.x);
                    let ro = (sqr(-a.x - xo) + sqr(a.y)).sqrt();
                    let theta = 1.0f64.atan2(ro);
                    let an = fmod(pin * theta + a.y.atan2(xo + a.x) + theta, 2.0 * theta) - theta;
                    v.x -= amount * (xo - an.cos() * ro);
                    v.y += amount * an.sin() * ro;
                }
            } else {
                let r = 1.0 / r.sqrt();
                let ta = a.y.atan2(a.x);
                let x = r * ta.cos();
                let y = r * ta.sin();
                let sign = if x >= 0.0 { 1.0 } else { -1.0 };
                let xo = sign * (sqr(x) + sqr(y) + 1.0) / (2.0 * x);
                let ro = (sqr(sign * x - xo) + sqr(y)).sqrt();
                let theta = 1.0f64.atan2(ro);
                let an = fmod(pout * theta + y.atan2(xo - sign * x) + theta, 2.0 * theta) - theta;
                let x = xo - an.cos() * ro;
                let y = an.sin() * ro;
                let ta = y.atan2(x);
                let r = 1.0 / (sqr(x) + sqr(y)).sqrt();
                v.x += sign * amount * r * ta.cos();
                v.y += amount * r * ta.sin();
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = a.x * a.x + a.y * a.y;
         if (r < 1.0) {
           var sg = 1.0;
           if (a.x < 0.0) { sg = -1.0; }
           let xo = sg * (r + 1.0) / (2.0 * a.x);
           let ro = sqrt(sqr(sg * a.x - xo) + sqr(a.y));
           let theta = atan2(1.0, ro);
           let an = fmod(D(0) * theta + atan2(a.y, xo - sg * a.x) + theta, 2.0 * theta) - theta;
           v.x += sg * AMOUNT * (xo - cos(an) * ro);
           v.y += AMOUNT * sin(an) * ro;
         } else {
           let ri = 1.0 / sqrt(r);
           let ta = atan2(a.y, a.x);
           let x = ri * cos(ta);
           let y = ri * sin(ta);
           var sg = 1.0;
           if (x < 0.0) { sg = -1.0; }
           let xo = sg * (x * x + y * y + 1.0) / (2.0 * x);
           let ro = sqrt(sqr(sg * x - xo) + sqr(y));
           let theta = atan2(1.0, ro);
           let an = fmod(D(1) * theta + atan2(y, xo - sg * x) + theta, 2.0 * theta) - theta;
           let x2 = xo - cos(an) * ro;
           let y2 = sin(an) * ro;
           let ta2 = atan2(y2, x2);
           let r2 = 1.0 / sqrt(x2 * x2 + y2 * y2);
           v.x += sg * AMOUNT * r2 * cos(ta2);
           v.y += AMOUNT * r2 * sin(ta2);
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "oscilloscope2",
        params: &[
            ParamDef::new("separation", 1.0),
            ParamDef::new("frequencyx", M_PI),
            ParamDef::new("frequencyy", M_PI),
            ParamDef::new("amplitude", 1.0),
            ParamDef::new("perturbation", 1.0),
            ParamDef::new("damping", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let pt = d[4] * (d[7] * a.y).sin();
            let t = if d[8] != 0.0 {
                d[3] * (d[6] * a.x + pt).cos() + d[0]
            } else {
                d[3] * (-a.x.abs() * d[5]).exp() * (d[6] * a.x + pt).cos() + d[0]
            };
            if a.y.abs() <= t {
                v.x -= amount * a.x;
                v.y -= amount * a.y;
            } else {
                v.x += amount * a.x;
                v.y += amount * a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        // d = [separation, frequencyx, frequencyy, amplitude, perturbation, damping, tpf, tpf2, noDamping]
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d.push(2.0 * M_PI * p[1]);
            d.push(2.0 * M_PI * p[2]);
            d.push(if p[5].abs() <= EPSILON { 1.0 } else { 0.0 });
            d
        }),
        wgsl: "let pt = D(4) * sin(D(7) * a.y);
         var t = D(3) * cos(D(6) * a.x + pt) + D(0);
         if (D(8) == 0.0) { t = D(3) * exp(-abs(a.x) * D(5)) * cos(D(6) * a.x + pt) + D(0); }
         if (abs(a.y) <= t) {
           v.x -= AMOUNT * a.x;
           v.y -= AMOUNT * a.y;
         } else {
           v.x += AMOUNT * a.x;
           v.y += AMOUNT * a.y;
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "ouroboros",
        params: &[ParamDef::new("radius", 0.5), ParamDef::new("twist", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let radius = java_max(d[0].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let wrapped = radius * ((r / radius) - (r / radius).floor());
            let nt = theta + d[1] * r;
            v.x += amount * wrapped * nt.cos();
            v.y += amount * wrapped * nt.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let radius = max(abs(D(0)), 0.01);
         let r = sqrt(a.x * a.x + a.y * a.y);
         let theta = atan2(a.y, a.x);
         let wrapped = radius * (r / radius - floor(r / radius));
         let nt = theta + D(1) * r;
         v.x += AMOUNT * wrapped * cos(nt);
         v.y += AMOUNT * wrapped * sin(nt);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "multi_ifs",
        "murl2",
        "murl",
        "mushroom",
        "nBlur",
        "nephroid",
        "neuron3D",
        "npolar",
        "nsudoku",
        "oak_leaf",
        "ocappedcone3D",
        "octagon",
        "octahedron3D",
        "octapol",
        "octogonprism3D",
        "ocylinder3D",
        "onion2",
        "onion",
        "oroundcone3D",
        "ortho",
        "oscilloscope2",
        "ouroboros",
    ];

    const POINTS: &[(f64, f64, f64)] = &[
        (0.3, 0.2, 0.1),
        (-0.7, 0.5, -0.2),
        (1.3, -0.9, 0.4),
        (0.05, -0.02, 0.0),
        (-2.1, -1.7, 1.1),
        (0.6, 0.1, -0.5),
    ];

    fn run(def: &VarDef, data: &[f64], ctx: &mut Ctx, p: (f64, f64, f64)) -> (Pt, Pt) {
        let mut a = Pt { x: p.0, y: p.1, z: p.2, color: 0.5, ..Default::default() };
        let mut v = Pt { x: 0.1, y: -0.2, z: 0.05, color: 0.5, ..Default::default() };
        (def.apply)(data, ctx, &mut a, &mut v, 0.8);
        (a, v)
    }

    fn def(name: &str) -> &'static VarDef {
        DEFS.iter().find(|d| d.name == name).unwrap()
    }

    #[test]
    fn all_names_present_and_unique() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert_eq!(DEFS.iter().filter(|d| d.name == *n).count(), 1, "{n}");
            assert!(crate::variations::find(n).is_some(), "{n} not registered");
        }
    }

    #[test]
    fn default_params_give_finite_results() {
        let mut ctx = Ctx::new(42, true);
        for def in DEFS {
            let data = def.prepare_data(&[], &Affine::IDENTITY);
            assert!(def.supports_gpu(), "{} has no WGSL", def.name);
            for &p in POINTS {
                // octagon (mode 1) takes sqrt(z): NaN for z < 0 like in Java
                if def.name == "octagon" && p.2 < 0.0 {
                    continue;
                }
                for _ in 0..50 {
                    let (a, v) = run(def, &data, &mut ctx, p);
                    assert!(
                        !a.is_bad() && !v.is_bad() && v.color.is_finite(),
                        "{} at {:?}: a={:?} v={:?}",
                        def.name,
                        p,
                        a,
                        v
                    );
                }
            }
        }
    }

    #[test]
    fn java_random_matches_reference() {
        // new java.util.Random(42): nextDouble() -> 0.7275636800328681, 0.6832234717598454
        let mut r = JavaRandom::new(42);
        assert!((r.next_double() - 0.7275636800328681).abs() < 1e-16);
        assert!((r.next_double() - 0.6832234717598454).abs() < 1e-16);
        // new Random(42).nextInt(10) == 0 (nextInt() == -1170105035)
        assert_eq!(JavaRandom::new(42).next_int(10), 0);
        assert_eq!(JavaRandom::new(42).next(32), -1170105035);
    }

    /// Rust mirror of the WGSL 16-bit-limb emulation of `java.util.Random` (neuron3D).
    fn limb_mul48(a: [u32; 3], b: [u32; 3], add: u32) -> [u32; 3] {
        let p00 = a[0] * b[0] + add;
        let p01 = a[0] * b[1];
        let p10 = a[1] * b[0];
        let s1 = (p01 & 0xFFFF) + (p10 & 0xFFFF) + (p00 >> 16);
        let c1 = (s1 >> 16) + (p01 >> 16) + (p10 >> 16);
        let o2 = a[0].wrapping_mul(b[2]).wrapping_add(a[1].wrapping_mul(b[1])).wrapping_add(a[2].wrapping_mul(b[0])).wrapping_add(c1) & 0xFFFF;
        [p00 & 0xFFFF, s1 & 0xFFFF, o2]
    }

    fn limb_from_i32(v: i32) -> [u32; 3] {
        let u = v as u32;
        [u & 0xFFFF, u >> 16, if v < 0 { 0xFFFF } else { 0 }]
    }

    fn limb_xor(a: [u32; 3], b: [u32; 3]) -> [u32; 3] {
        [a[0] ^ b[0], a[1] ^ b[1], a[2] ^ b[2]]
    }

    fn limb_next_double(s: &mut [u32; 3]) -> f64 {
        let m = [0xE66D, 0xDEEC, 0x5];
        *s = limb_mul48(*s, m, 0xB);
        let n26 = (s[2] << 10) | (s[1] >> 6);
        *s = limb_mul48(*s, m, 0xB);
        let n27 = (s[2] << 11) | (s[1] >> 5);
        ((n26 as u64) << 27 | n27 as u64) as f64 / (1u64 << 53) as f64
    }

    #[test]
    fn neuron3d_limb_rng_matches_java_random() {
        let seed = 12345;
        for (ix, iy, iz) in [(0, 0, 0), (-3, 7, 2), (9, -10, -1), (-25, 25, 13), (4000, -3000, 77)] {
            let h = (ix as i64).wrapping_mul(73856093) ^ (iy as i64).wrapping_mul(19349663) ^ (iz as i64).wrapping_mul(83492791) ^ seed;
            let mut jr = JavaRandom::new(h);
            let lh = limb_xor(
                limb_xor(
                    limb_mul48(limb_from_i32(ix), [0xF45D, 0x0466, 0], 0),
                    limb_mul48(limb_from_i32(iy), [0x409F, 0x0127, 0], 0),
                ),
                limb_xor(limb_mul48(limb_from_i32(iz), [0xFFB7, 0x04F9, 0], 0), limb_from_i32(seed as i32)),
            );
            let mut s = limb_xor(lh, [0xE66D, 0xDEEC, 0x5]);
            for _ in 0..3 {
                assert_eq!(jr.next_double(), limb_next_double(&mut s), "{ix} {iy} {iz}");
            }
        }
    }

    #[test]
    fn multi_ifs_default_is_vicsek() {
        let d = def("multi_ifs").prepare_data(&[], &Affine::IDENTITY);
        assert_eq!(d.len(), 7 + 28);
        assert_eq!(&d[7..14], &[0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0]);
        assert_eq!(&d[28..35], &[0.5, 0.0, 0.0, 0.0, 0.5, -0.5, 3.0]);
        let d = def("multi_ifs").prepare_data(&[99.0], &Affine::IDENTITY);
        assert_eq!(d[0], 73.0);
    }

    #[test]
    fn nsudoku_board_and_levels() {
        let mut board = [[0i32; 9]; 9];
        let mut rnd = JavaRandom::new(NSUDOKU_BOARD_SEED);
        assert!(nsudoku_next_cell(&mut board, &mut rnd, 0, 0));
        for i in 0..9 {
            let mut row: Vec<i32> = (0..9).map(|j| board[i][j]).collect();
            let mut col: Vec<i32> = (0..9).map(|j| board[j][i]).collect();
            row.sort();
            col.sort();
            assert_eq!(row, (1..=9).collect::<Vec<_>>());
            assert_eq!(col, (1..=9).collect::<Vec<_>>());
        }
        for level in 1..=9 {
            let d = def("nsudoku").prepare_data(&[level as f64], &Affine::IDENTITY);
            let built = d[5] as usize;
            assert!(built >= 1 && built <= level, "level {level}: {built}");
            assert_eq!(d.len(), NSUDOKU_COLORS + built * 81);
            assert!(d[NSUDOKU_COLORS..].iter().all(|c| *c > 0.0 && *c <= 1.0));
        }
    }

    #[test]
    fn sdf_shapes_plot_points() {
        let mut ctx = Ctx::new(7, false);
        for name in ["ocappedcone3D", "octahedron3D", "octogonprism3D", "ocylinder3D", "oroundcone3D"] {
            let d = def(name).prepare_data(&[], &Affine::IDENTITY);
            let mut shown = 0;
            for _ in 0..200 {
                let (_, v) = run(def(name), &d, &mut ctx, (0.0, 0.0, 0.0));
                if !v.do_hide {
                    shown += 1;
                }
            }
            assert!(shown > 100, "{name}: {shown}");
        }
    }

    #[test]
    fn nblur_stripes_and_exact_are_finite() {
        let mut ctx = Ctx::new(3, false);
        for params in [
            [5.0, 3.0, 1.5, 0.3, 0.0, 1.0, 1.0, 0.0, 1.0],
            [6.0, -2.0, 0.5, 0.2, 0.0, 0.0, 0.0, 1.0, 1.0],
            [4.0, 4.0, 0.4, 0.5, 1.0, 1.0, 1.0, 1.0, 2.0],
            [7.0, -3.0, 1.7, 0.95, 0.0, 1.0, 0.0, 1.0, 0.5],
        ] {
            let d = def("nBlur").prepare_data(&params, &Affine::IDENTITY);
            for _ in 0..500 {
                let (_, v) = run(def("nBlur"), &d, &mut ctx, (0.1, 0.2, 0.3));
                assert!(!v.is_bad(), "{params:?}: {v:?}");
            }
        }
    }
}
