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

//! Variations ported from the JWildfire sources (batch w14).

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "harmonograph_js",
        // Defaults: JWildfire's values for seed 1234 (the damping values are random there).
        params: &[
            ParamDef::int("seed", 1234.0, 0.0, 100000.0),
            ParamDef::int("time", 100.0, 1.0, 10000.0),
            ParamDef::int("a1", 128.0, 0.0, 180.0),
            ParamDef::new("f1", 5.0),
            ParamDef::int("p1", 113.0, 0.0, 180.0),
            ParamDef::new("d1", 0.001),
            ParamDef::int("a2", 0.0, 0.0, 180.0),
            ParamDef::new("f2", 3.0),
            ParamDef::int("p2", 33.0, 0.0, 180.0),
            ParamDef::new("d2", 0.001),
            ParamDef::int("a3", 49.0, 0.0, 180.0),
            ParamDef::new("f3", -4.0),
            ParamDef::int("p3", 57.0, 0.0, 90.0),
            ParamDef::new("d3", 0.001),
            ParamDef::int("a4", 97.0, 0.0, 180.0),
            ParamDef::new("f4", 1.0),
            ParamDef::int("p4", 68.0, 0.0, 90.0),
            ParamDef::new("d4", 0.001),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            let mo = |t: f64, k: usize, dp: f64| {
                let (pa, pb) = ((d[k + 2] + dp) * M_PI / 180.0, (d[k + 6] + dp) * M_PI / 180.0);
                d[k] * (t * d[k + 1] + pa).sin() * (-d[k + 3] * t).exp() + d[k + 4] * (t * d[k + 5] + pb).sin() * (-d[k + 7] * t).exp()
            };
            let t = ctx.random() * d[1];
            v.x += mo(t, 2, 0.0) / 180.0 * amount;
            v.y += mo(t, 10, 90.0) / 180.0 * amount;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for i in [1, 2, 4, 6, 8, 10, 12, 14, 16] {
                d[i] = p[i].trunc();
            }
            d
        }),
        wgsl: "fn harmonograph_js_mo(base: u32, t: f32, k: u32, dp: f32) -> f32 {
  let pa = (prm[base + k + 2u] + dp) * PI / 180.0;
  let pb = (prm[base + k + 6u] + dp) * PI / 180.0;
  return prm[base + k] * sin(t * prm[base + k + 1u] + pa) * exp(-prm[base + k + 3u] * t)
    + prm[base + k + 4u] * sin(t * prm[base + k + 5u] + pb) * exp(-prm[base + k + 7u] * t);
}
@body
let t = rnd() * D(1);
v.x += harmonograph_js_mo(base, t, 2u, 0.0) / 180.0 * AMOUNT;
v.y += harmonograph_js_mo(base, t, 10u, 90.0) / 180.0 * AMOUNT;",
    },
    VarDef {
        name: "helicoid",
        params: &[ParamDef::new("frequency", 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let range = (a.x * a.x + a.y * a.y).sqrt();
            let ang = a.z * M_2PI * d[0] + a.y.atan2(a.x);
            v.x += amount * ang.cos() * range;
            v.y += amount * ang.sin() * range;
            v.z += amount * a.z;
        },
        prepare: None,
        wgsl: "let range = sqrt(a.x * a.x + a.y * a.y);
let ang = a.z * TWO_PI * D(0) + atan2(a.y, a.x);
v.x += AMOUNT * cos(ang) * range;
v.y += AMOUNT * sin(ang) * range;
v.z += AMOUNT * a.z;",
    },
    VarDef {
        name: "helix",
        params: &[ParamDef::new("frequency", 1.0), ParamDef::new("width", 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let ang = a.z * M_2PI * d[0];
            v.x += amount * (a.x + ang.cos() * d[1]);
            v.y += amount * (a.y + ang.sin() * d[1]);
            v.z += amount * a.z;
        },
        prepare: None,
        wgsl: "let ang = a.z * TWO_PI * D(0);
v.x += AMOUNT * (a.x + cos(ang) * D(1));
v.y += AMOUNT * (a.y + sin(ang) * D(1));
v.z += AMOUNT * a.z;",
    },
    VarDef {
        name: "henon",
        params: &[ParamDef::new("a", 0.5), ParamDef::new("b", 1.0), ParamDef::new("c", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += (d[2] - (d[0] * (a.x * a.x)) + a.y) * amount;
            v.y += d[1] * a.x * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += (D(2) - (D(0) * (a.x * a.x)) + a.y) * AMOUNT;
v.y += D(1) * a.x * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];
