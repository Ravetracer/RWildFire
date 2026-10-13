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

/// `Hexaplay3DFunc` / `Hexnix3DFunc` segment cycling: JWildfire draws a mode (60 degree steps for
/// `rswtch <= 1`) per instance and redraws it after 6 calls in 60 degree mode or 3 calls in 120
/// degree mode. On the CPU the counters are kept per render thread; returns (mode60, fcycle,
/// bcycle) before the step.
fn hex_lattice_step(d: &[f64], ctx: &mut Ctx) -> (bool, usize, usize) {
    let key = d.as_ptr() as usize;
    let st = match ctx.orbits.get(&key) {
        Some(st) => *st,
        // init: rswtch, fcycle = bcycle = 0
        None => [(ctx.random() * 3.0).trunc(), 0.0],
    };
    let mut rswtch = st[0] as i32;
    let (mut fcycle, mut bcycle) = ((st[1] as usize) % 8, (st[1] as usize) / 8);
    if fcycle > 5 {
        fcycle = 0;
        rswtch = (ctx.random() * 3.0).trunc() as i32;
    }
    if bcycle > 2 {
        bcycle = 0;
        rswtch = (ctx.random() * 3.0).trunc() as i32;
    }
    let mode60 = rswtch <= 1;
    let next = if mode60 { (fcycle + 1, bcycle) } else { (fcycle, bcycle + 1) };
    ctx.orbits.insert(key, [rswtch as f64, (next.0 + 8 * next.1) as f64]);
    (mode60, fcycle, bcycle)
}

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
    VarDef {
        name: "hexagonal",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sc = java_max(d[0].abs(), 0.0001);
            const SQRT3: f64 = 1.732050807568877;
            let q = (2.0 / 3.0) * a.x / sc;
            let r = ((-1.0 / 3.0) * a.x + (1.0 / SQRT3) * a.y) / sc;
            let s = -q - r;
            let (mut rq, mut rr, rs) = ((q + 0.5).floor(), (r + 0.5).floor(), (s + 0.5).floor());
            let (dq, dr, ds) = ((rq - q).abs(), (rr - r).abs(), (rs - s).abs());
            if dq > dr && dq > ds {
                rq = -rr - rs;
            } else if dr > ds {
                rr = -rq - rs;
            }
            v.x += amount * (a.x - sc * 1.5 * rq);
            v.y += amount * (a.y - sc * SQRT3 * (rr + rq * 0.5));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sc = max(abs(D(0)), 0.0001);
let q = (2.0 / 3.0) * a.x / sc;
let r = ((-1.0 / 3.0) * a.x + (1.0 / 1.732050807568877) * a.y) / sc;
let s = -q - r;
var rq = floor(q + 0.5);
var rr = floor(r + 0.5);
let rs = floor(s + 0.5);
let dq = abs(rq - q);
let dr = abs(rr - r);
let ds = abs(rs - s);
if (dq > dr && dq > ds) { rq = -rr - rs; } else if (dr > ds) { rr = -rq - rs; }
v.x += AMOUNT * (a.x - sc * 1.5 * rq);
v.y += AMOUNT * (a.y - sc * 1.732050807568877 * (rr + rq * 0.5));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hexaplay3D",
        params: &[ParamDef::new("majp", 1.0), ParamDef::new("scale", 0.25), ParamDef::new("zlift", 0.25)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            let (mode60, fcycle, bcycle) = hex_lattice_step(d, ctx);
            let scale = d[1] * 0.5;
            let pos_neg = if ctx.random() < 0.5 { -1.0 } else { 1.0 };
            let abmajp = d[0].abs();
            if abmajp <= 1.0 {
                v.z += a.z * 0.5 * d[2];
            } else {
                v.z += a.z * 0.5 * d[2] + pos_neg * ((abmajp - 1.0) * 0.5);
            }
            let h = (M_PI / 3.0).sin();
            let (sx, sy) = if mode60 {
                [(1.0, 0.0), (0.5, h), (-0.5, h), (-1.0, 0.0), (-0.5, -h), (0.5, -h)][fcycle]
            } else {
                [(1.0, 0.0), (-0.5, h), (-0.5, -h)][bcycle]
            };
            v.x = ((v.x + a.x) * scale) + amount * sx;
            v.y = ((v.y + a.y) * scale) + amount * sy;
        },
        prepare: None,
        // GPU: no state across calls; the 60 degree mode is drawn with its long-run share (4/5)
        // and a random segment instead of the next one.
        wgsl: "let mode60 = rnd() < 0.8;
let scale = D(1) * 0.5;
var pos_neg = 1.0;
if (rnd() < 0.5) { pos_neg = -1.0; }
let abmajp = abs(D(0));
if (abmajp <= 1.0) {
  v.z += a.z * 0.5 * D(2);
} else {
  v.z += a.z * 0.5 * D(2) + pos_neg * ((abmajp - 1.0) * 0.5);
}
let h = sin(PI / 3.0);
var sg = vec2<f32>(1.0, 0.0);
if (mode60) {
  let k = min(u32(rnd() * 6.0), 5u);
  let ang = f32(k) * PI / 3.0;
  sg = vec2<f32>(cos(ang), sin(ang));
} else {
  let k = min(u32(rnd() * 3.0), 2u);
  if (k == 1u) { sg = vec2<f32>(-0.5, h); } else if (k == 2u) { sg = vec2<f32>(-0.5, -h); }
}
v.x = (v.x + a.x) * scale + AMOUNT * sg.x;
v.y = (v.y + a.y) * scale + AMOUNT * sg.y;",
    },
    VarDef {
        name: "hex_modulus",
        params: &[ParamDef::new("size", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            const SQRT3_2: f64 = 0.866_025_403_784_438_6;
            const SQRT3: f64 = 1.732_050_807_568_877_2;
            let hsize = SQRT3_2 / d[0];
            let weight = amount / SQRT3_2;
            let (xx, yy) = (a.x * hsize, a.y * hsize);
            let x = 0.577_350_269_189_625_7 * xx - yy / 3.0;
            let z = 2.0 * yy / 3.0;
            let y = -x - z;
            let (mut rx, mut ry, mut rz) = (java_round(x), java_round(y), java_round(z));
            let (dx, dy, dz) = ((rx - x).abs(), (ry - y).abs(), (rz - z).abs());
            if dx > dy && dx > dz {
                rx = -ry - rz;
            } else if dy > dz {
                ry = -rx - rz;
            } else {
                rz = -rx - ry;
            }
            let _ = ry;
            v.x += (xx - (SQRT3 * rx + SQRT3_2 * rz)) * weight;
            v.y += (yy - 1.5 * rz) * weight;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let hsize = 0.8660254037844386 / D(0);
let weight = AMOUNT / 0.8660254037844386;
let xx = a.x * hsize;
let yy = a.y * hsize;
let x = 0.5773502691896257 * xx - yy / 3.0;
let z = 2.0 * yy / 3.0;
let y = -x - z;
var rx = java_round(x);
var ry = java_round(y);
var rz = java_round(z);
let dx = abs(rx - x);
let dy = abs(ry - y);
let dz = abs(rz - z);
if (dx > dy && dx > dz) { rx = -ry - rz; } else if (dy > dz) { ry = -rx - rz; } else { rz = -rx - ry; }
v.x += (xx - (1.7320508075688772 * rx + 0.8660254037844386 * rz)) * weight;
v.y += (yy - 1.5 * rz) * weight;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hexnix3D",
        params: &[ParamDef::new("majp", 1.0), ParamDef::new("scale", 0.25), ParamDef::new("zlift", 0.0), ParamDef::new("3side", 0.67)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            let (mode60, _, _) = hex_lattice_step(d, ctx);
            let (majp, scale, zlift, scale3) = (d[0], d[1], d[2], d[3]);
            let smooth = if amount.abs() <= 0.5 { amount * 2.0 } else { 1.0 };
            let pos_neg = if ctx.random() < 0.5 { -1.0 } else { 1.0 };
            let abmajp = majp.abs();
            let (majplane, boost) = if abmajp <= 1.0 {
                (0, 0.0)
            } else if abmajp > 1.0 && abmajp < 2.0 {
                (1, 0.0)
            } else {
                (2, (abmajp - 2.0) * 0.5)
            };
            if majplane == 0 {
                v.z += smooth * a.z * scale * zlift;
            } else if majplane == 1 && majp < 0.0 {
                let gentle = if (-2.0..-1.0).contains(&majp) { abmajp - 1.0 } else { 1.0 };
                if pos_neg < 0.0 {
                    v.z += -2.0 * (v.z * gentle);
                }
            }
            if majplane == 2 && majp < 0.0 {
                if pos_neg > 0.0 {
                    v.z += smooth * (a.z * scale * zlift + boost);
                } else {
                    v.z = (v.z - (2.0 * smooth * v.z)) + (smooth * pos_neg * (a.z * scale * zlift + boost));
                }
            } else {
                v.z += smooth * (a.z * scale * zlift + (pos_neg * boost));
            }
            let h = (M_PI / 3.0).sin();
            let ((sx, sy), tail) = if mode60 {
                ([(1.0, 0.0), (0.5, -h), (-0.5, -h), (-1.0, 0.0), (-0.5, h), (0.5, h)][(ctx.random() * 6.0).trunc() as usize], 1.0)
            } else {
                ([(0.0, -1.0), ((7.0 * M_PI / 6.0).cos(), 0.5), ((11.0 * M_PI / 6.0).cos(), 0.5)][(ctx.random() * 3.0).trunc() as usize], scale3)
            };
            let rx_tp = (smooth * scale * v.x * sx) - (smooth * scale * v.y * sy);
            let ry_tp = (smooth * scale * v.y * sx) + (smooth * scale * v.x * sy);
            let rx_ft = (a.x * smooth * scale * sx) - (a.y * smooth * scale * sy);
            let ry_ft = (a.y * smooth * scale * sx) + (a.x * smooth * scale * sy);
            v.x = v.x * (1.0 - smooth) + rx_tp + rx_ft + smooth * amount * tail * sx;
            v.y = v.y * (1.0 - smooth) + ry_tp + ry_ft + smooth * amount * tail * sy;
        },
        prepare: None,
        // GPU: no state across calls; the 60 degree mode is drawn with its long-run share (4/5).
        wgsl: "let mode60 = rnd() < 0.8;
let majp = D(0);
let scale = D(1);
let zlift = D(2);
var smooth_f = 1.0;
if (abs(AMOUNT) <= 0.5) { smooth_f = AMOUNT * 2.0; }
var pos_neg = 1.0;
if (rnd() < 0.5) { pos_neg = -1.0; }
let abmajp = abs(majp);
var majplane = 2;
var boost = (abmajp - 2.0) * 0.5;
if (abmajp <= 1.0) { majplane = 0; boost = 0.0; } else if (abmajp < 2.0) { majplane = 1; boost = 0.0; }
if (majplane == 0) {
  v.z += smooth_f * a.z * scale * zlift;
} else if (majplane == 1 && majp < 0.0) {
  var gentle = 1.0;
  if (majp < -1.0 && majp >= -2.0) { gentle = abmajp - 1.0; }
  if (pos_neg < 0.0) { v.z += -2.0 * (v.z * gentle); }
}
if (majplane == 2 && majp < 0.0) {
  if (pos_neg > 0.0) {
    v.z += smooth_f * (a.z * scale * zlift + boost);
  } else {
    v.z = (v.z - (2.0 * smooth_f * v.z)) + (smooth_f * pos_neg * (a.z * scale * zlift + boost));
  }
} else {
  v.z += smooth_f * (a.z * scale * zlift + (pos_neg * boost));
}
var sg = vec2<f32>(0.0, -1.0);
var tail = D(3);
if (mode60) {
  let k = min(u32(trunc(rnd() * 6.0)), 5u);
  let ang = -f32(k) * PI / 3.0;
  sg = vec2<f32>(cos(ang), sin(ang));
  tail = 1.0;
} else {
  let k = min(u32(trunc(rnd() * 3.0)), 2u);
  if (k == 1u) { sg = vec2<f32>(cos(7.0 * PI / 6.0), 0.5); } else if (k == 2u) { sg = vec2<f32>(cos(11.0 * PI / 6.0), 0.5); }
}
let k = smooth_f * scale;
let rx_tp = k * v.x * sg.x - k * v.y * sg.y;
let ry_tp = k * v.y * sg.x + k * v.x * sg.y;
let rx_ft = a.x * k * sg.x - a.y * k * sg.y;
let ry_ft = a.y * k * sg.x + a.x * k * sg.y;
v.x = v.x * (1.0 - smooth_f) + rx_tp + rx_ft + smooth_f * AMOUNT * tail * sg.x;
v.y = v.y * (1.0 - smooth_f) + ry_tp + ry_ft + smooth_f * AMOUNT * tail * sg.y;",
    },
];
