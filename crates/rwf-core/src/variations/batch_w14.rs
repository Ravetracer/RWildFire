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

/// `HexPrism3DFunc.sdHexPrism`
fn hexprism3d_sd(p: [f64; 3], hx: f64, hy: f64) -> f64 {
    let k = [-0.8660254, 0.5, 0.57735];
    let mut p = p.map(f64::abs);
    let t = 2.0 * java_min(k[0] * p[0] + k[1] * p[1], 0.0);
    p[0] -= k[0] * t;
    p[1] -= k[1] * t;
    let cx = java_min(java_max(p[0], -k[2] * hx), k[2] * hx);
    let (ex, ey) = (p[0] - cx, p[1] - hx);
    let sgn = if p[1] - hx > 0.0 { 1.0 } else if p[1] - hx < 0.0 { -1.0 } else { 0.0 };
    let (dx, dy) = ((ex * ex + ey * ey).sqrt() * sgn, p[2] - hy);
    let (mx, my) = (java_max(dx, 0.0), java_max(dy, 0.0));
    java_min(java_max(dx, dy), 0.0) + (mx * mx + my * my).sqrt()
}

/// `HilbertFunc` turtle path as a polyline: d = [level, show_lines, line_thickness, show_points,
/// point_thickness, line_threshold, point_threshold, line_thickness / 100, point_thickness / 100,
/// segments, x0, y0, x1, y1, ...].
fn hilbert_prepare(p: &[f64]) -> Vec<f64> {
    let level = limit_value(ftoi(p[0]), 1, 10);
    struct Turtle {
        x: f64,
        y: f64,
        angle: f64,
        pts: Vec<f64>,
    }
    impl Turtle {
        fn forward(&mut self) {
            self.x += self.angle.to_radians().cos();
            self.y += self.angle.to_radians().sin();
            self.pts.extend([self.x, self.y]);
        }
    }
    fn draw(t: &mut Turtle, n: i32, sign: f64) {
        if n == 0 {
            return;
        }
        t.angle += 90.0 * sign;
        draw(t, n - 1, -sign);
        t.forward();
        t.angle -= 90.0 * sign;
        draw(t, n - 1, sign);
        t.forward();
        draw(t, n - 1, sign);
        t.angle -= 90.0 * sign;
        t.forward();
        draw(t, n - 1, -sign);
        t.angle += 90.0 * sign;
    }
    let mut t = Turtle { x: 0.0, y: 0.0, angle: 0.0, pts: vec![0.0, 0.0] };
    draw(&mut t, level, 1.0);
    let (show_lines, show_points) = (p[1].trunc(), p[3].trunc());
    let sum = show_lines + show_points;
    let line_fraction = show_lines / sum;
    let mut d = vec![level as f64, show_lines, p[2], show_points, p[4], line_fraction, line_fraction + show_points / sum, p[2] / 100.0, p[4] / 100.0];
    d.push((t.pts.len() / 2 - 1) as f64);
    d.extend_from_slice(&t.pts);
    d
}

/// `HtreeFunc.draw`: three segments per H, d = [level, size, show_lines, line_thickness,
/// show_points, point_thickness, line_threshold, point_threshold, line_thickness / 100,
/// point_thickness / 100, segments, x1, y1, x2, y2, ...].
fn htree_prepare(p: &[f64]) -> Vec<f64> {
    let level = limit_value(ftoi(p[0]), 1, 10);
    fn draw(segs: &mut Vec<f64>, n: i32, x: f64, y: f64, size: f64) {
        if n == 0 {
            return;
        }
        let (x0, x1, y0, y1) = (x - size / 2.0, x + size / 2.0, y - size / 2.0, y + size / 2.0);
        segs.extend([x0, y0, x0, y1, x1, y0, x1, y1, x0, y, x1, y]);
        draw(segs, n - 1, x0, y0, size / 2.0);
        draw(segs, n - 1, x0, y1, size / 2.0);
        draw(segs, n - 1, x1, y0, size / 2.0);
        draw(segs, n - 1, x1, y1, size / 2.0);
    }
    let mut segs = Vec::new();
    draw(&mut segs, level, 0.0, 0.0, p[1]);
    let (show_lines, show_points) = (p[2].trunc(), p[4].trunc());
    let sum = show_lines + show_points;
    let lf = show_lines / sum;
    let mut d = vec![level as f64, p[1], show_lines, p[3], show_points, p[5], lf, lf + show_points / sum, p[3] / 100.0, p[5] / 100.0];
    d.push((segs.len() / 4) as f64);
    d.extend_from_slice(&segs);
    d
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
    VarDef {
        name: "hexprism3D",
        params: &[ParamDef::range("d", 0.3, 0.0, 0.5), ParamDef::range("H", 0.25, 0.0, 0.5)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| {
            for _ in 0..50 {
                let p = [ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5];
                if hexprism3d_sd(p, d[0], d[1]) < 0.0 {
                    v.do_hide = false;
                    v.x += amount * p[0];
                    v.y += amount * p[1];
                    v.z += amount * p[2];
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, 0.5), limit_value(p[1], 0.0, 0.5)]),
        wgsl: "fn hexprism3D_sd(p_in: vec3<f32>, h: vec2<f32>) -> f32 {
  let k = vec3<f32>(-0.8660254, 0.5, 0.57735);
  var p = abs(p_in);
  let t = p.xy - 2.0 * min(dot(k.xy, p.xy), 0.0) * k.xy;
  p = vec3<f32>(t, p.z);
  let e = p.xy - vec2<f32>(min(max(p.x, -k.z * h.x), k.z * h.x), h.x);
  let dd = vec2<f32>(length(e) * sign(p.y - h.x), p.z - h.y);
  return min(max(dd.x, dd.y), 0.0) + length(max(dd, vec2<f32>(0.0)));
}
@body
var found = false;
for (var i = 0; i < 50; i++) {
  let p = vec3<f32>(rnd() - 0.5, rnd() - 0.5, rnd() - 0.5);
  if (hexprism3D_sd(p, vec2<f32>(D(0), D(1))) < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * p.x;
    v.y += AMOUNT * p.y;
    v.z += AMOUNT * p.z;
    found = true;
    break;
  }
}
if (!found) { v.hide = 1.0; }",
    },
    VarDef {
        name: "hilbert_js",
        params: &[
            ParamDef::int("level", 2.0, 1.0, 10.0),
            ParamDef::int("show_lines", 1.0, 0.0, 1.0),
            ParamDef::new("line_thickness", 0.5),
            ParamDef::int("show_points", 0.0, 0.0, 1.0),
            ParamDef::new("point_thickness", 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // Java cycles through the segments; here a random segment is drawn.
            let nseg = d[9] as usize;
            let k = ((ctx.random() * nseg as f64) as usize).min(nseg - 1);
            let (x1, y1, x2, y2) = (d[10 + 2 * k], d[11 + 2 * k], d[12 + 2 * k], d[13 + 2 * k]);
            let rnd = ctx.random();
            let (mut xo, mut yo) = (0.0, 0.0);
            if rnd < d[5] {
                let (xdiff, ydiff) = (x2 - x1, y2 - y1);
                let m = if xdiff == 0.0 { 10000.0 } else { ydiff / xdiff };
                let dd = ctx.random() * (xdiff * xdiff + ydiff * ydiff).sqrt();
                let mut xoffset = dd / (1.0 + m * m).sqrt();
                if x2 < x1 {
                    xoffset = -xoffset;
                }
                let mut yoffset = (m * xoffset).abs();
                if y2 < y1 {
                    yoffset = -yoffset;
                }
                if d[7] != 0.0 {
                    xoffset += (ctx.random() - 0.5) * d[7];
                    yoffset += (ctx.random() - 0.5) * d[7];
                }
                xo = x1 + xoffset;
                yo = y1 + yoffset;
            } else if rnd <= d[6] {
                let (mut xoffset, mut yoffset) = (0.0, 0.0);
                if d[8] != 0.0 {
                    let roffset = ctx.random() * d[8];
                    let rangle = ctx.random() * M_2PI;
                    xoffset = roffset * rangle.cos();
                    yoffset = roffset * rangle.sin();
                }
                xo = x1 + xoffset;
                yo = y1 + yoffset;
            }
            v.x += amount * xo;
            v.y += amount * yo;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| hilbert_prepare(p)),
        wgsl: "let nseg = u32(D(9));
let k = min(u32(rnd() * f32(nseg)), nseg - 1u);
let o = base + 10u + 2u * k;
let x1 = prm[o];
let y1 = prm[o + 1u];
let x2 = prm[o + 2u];
let y2 = prm[o + 3u];
let r0 = rnd();
var q = vec2<f32>(0.0);
if (r0 < D(5)) {
  let xdiff = x2 - x1;
  let ydiff = y2 - y1;
  var m = 10000.0;
  if (xdiff != 0.0) { m = ydiff / xdiff; }
  let dd = rnd() * sqrt(xdiff * xdiff + ydiff * ydiff);
  var xoffset = dd / sqrt(1.0 + m * m);
  if (x2 < x1) { xoffset = -xoffset; }
  var yoffset = abs(m * xoffset);
  if (y2 < y1) { yoffset = -yoffset; }
  if (D(7) != 0.0) {
    xoffset += (rnd() - 0.5) * D(7);
    yoffset += (rnd() - 0.5) * D(7);
  }
  q = vec2<f32>(x1 + xoffset, y1 + yoffset);
} else if (r0 <= D(6)) {
  q = vec2<f32>(x1, y1);
  if (D(8) != 0.0) {
    let roffset = rnd() * D(8);
    let rangle = rnd() * TWO_PI;
    q += roffset * vec2<f32>(cos(rangle), sin(rangle));
  }
}
v.x += AMOUNT * q.x;
v.y += AMOUNT * q.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "ho",
        params: &[ParamDef::new("xpow", 3.0), ParamDef::new("ypow", 3.0), ParamDef::new("zpow", 3.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let (u, w0, w) = (a.x, a.y, a.z);
            let ax = (w0 * w0).atan2(w * w);
            let ay = (u * u).atan2(w * w);
            let (sv, cv, su, cu) = (w0.sin(), w0.cos(), u.sin(), u.cos());
            v.x += amount * ((cu * cv).powf(d[0]) + ((cu * cv) * d[0]) + (0.25 * ax));
            v.y += amount * ((su * cv).powf(d[1]) + ((su * cv) * d[1]) + (0.25 * ay));
            v.z += amount * (sv.powf(d[2]) + sv * d[2]);
        },
        prepare: None,
        wgsl: "let ax = atan2(a.y * a.y, a.z * a.z);
let ay = atan2(a.x * a.x, a.z * a.z);
let sv = sin(a.y);
let cv = cos(a.y);
let su = sin(a.x);
let cu = cos(a.x);
v.x += AMOUNT * (java_pow(cu * cv, D(0)) + cu * cv * D(0) + 0.25 * ax);
v.y += AMOUNT * (java_pow(su * cv, D(1)) + su * cv * D(1) + 0.25 * ay);
v.z += AMOUNT * (java_pow(sv, D(2)) + sv * D(2));",
    },
    VarDef {
        name: "hole",
        params: &[ParamDef::new("a", 1.0), ParamDef::int("inside", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let alpha = a.y.atan2(a.x);
            let delta = (alpha / M_PI + 1.0).powf(d[0]);
            let r = if d[1] != 0.0 { amount * delta / (a.x * a.x + a.y * a.y + delta) } else { amount * (a.x * a.x + a.y * a.y + delta).sqrt() };
            v.x += r * alpha.cos();
            v.y += r * alpha.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], limit_value(ftoi(p[1]), 0, 1) as f64]),
        wgsl: "let alpha = atan2(a.y, a.x);
let delta = java_pow(alpha / PI + 1.0, D(0));
var r = AMOUNT * sqrt(a.x * a.x + a.y * a.y + delta);
if (D(1) != 0.0) { r = AMOUNT * delta / (a.x * a.x + a.y * a.y + delta); }
v.x += r * cos(alpha);
v.y += r * sin(alpha);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hole2",
        params: &[
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 2.0),
            ParamDef::new("c", 1.0),
            ParamDef::new("d", 1.0),
            ParamDef::int("inside", 0.0, 0.0, 1.0),
            ParamDef::int("shape", 0.0, 0.0, 9.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let rhosq = a.sumsq();
            let theta = a.atan_yx() * d[3];
            let delta = (theta / M_PI + 1.0).powf(d[0]) * d[2];
            let b = d[1];
            let r1 = match d[5] as i32 {
                0 => rhosq.sqrt() + delta,
                1 => (rhosq + delta).sqrt(),
                2 => (rhosq + (b * theta).sin() + delta).sqrt(),
                3 => (rhosq + theta.sin() + delta).sqrt(),
                4 => (rhosq + theta * theta - delta + 1.0).sqrt(),
                5 => (rhosq + theta.tan().abs() + delta).sqrt(),
                6 => (rhosq * (1.0 + (b * theta).sin()) + delta).sqrt(),
                7 => (rhosq + (0.5 * b * theta).sin().abs() + delta).sqrt(),
                8 => (rhosq + (M_PI * (b * theta).sin()).sin() + delta).sqrt(),
                _ => (rhosq + ((b * theta).sin() + (2.0 * b * theta + M_PI_2).sin()) / 2.0 + delta).sqrt(),
            };
            let r1 = if d[4] != 0.0 { amount / r1 } else { amount * r1 };
            v.x += r1 * theta.cos();
            v.y += r1 * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = limit_value(ftoi(p[4]), 0, 1) as f64;
            d[5] = limit_value(ftoi(p[5]), 0, 9) as f64;
            d
        }),
        wgsl: "let rhosq = sumsq(a);
let theta = atan_yx(a) * D(3);
let delta = java_pow(theta / PI + 1.0, D(0)) * D(2);
let b = D(1);
var r1 = 1.0;
switch (i32(D(5))) {
  case 0: { r1 = sqrt(rhosq) + delta; }
  case 1: { r1 = sqrt(rhosq + delta); }
  case 2: { r1 = sqrt(rhosq + sin(b * theta) + delta); }
  case 3: { r1 = sqrt(rhosq + sin(theta) + delta); }
  case 4: { r1 = sqrt(rhosq + theta * theta - delta + 1.0); }
  case 5: { r1 = sqrt(rhosq + abs(tan(theta)) + delta); }
  case 6: { r1 = sqrt(rhosq * (1.0 + sin(b * theta)) + delta); }
  case 7: { r1 = sqrt(rhosq + abs(sin(0.5 * b * theta)) + delta); }
  case 8: { r1 = sqrt(rhosq + sin(PI * sin(b * theta)) + delta); }
  default: { r1 = sqrt(rhosq + (sin(b * theta) + sin(2.0 * b * theta + HALF_PI)) / 2.0 + delta); }
}
if (D(4) != 0.0) { r1 = AMOUNT / r1; } else { r1 = AMOUNT * r1; }
v.x += r1 * cos(theta);
v.y += r1 * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "holesq",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |_, ctx, a, v, amount| {
            let (x, y) = (amount * a.x, amount * a.y);
            let (fax, fay) = (x.abs(), y.abs());
            if fax + fay > 1.0 {
                v.x += x;
                v.y += y;
            } else if fax > fay {
                v.x += if x < 0.0 { (x + fay - 1.0) * 0.5 } else { (x - fay + 1.0) * 0.5 };
                v.y += y;
            } else {
                v.x += x;
                v.y += if y < 0.0 { (y + fax - 1.0) * 0.5 } else { (y - fax + 1.0) * 0.5 };
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = AMOUNT * a.x;
let y = AMOUNT * a.y;
let fax = abs(x);
let fay = abs(y);
if (fax + fay > 1.0) {
  v.x += x;
  v.y += y;
} else if (fax > fay) {
  var t = (x - fay + 1.0) * 0.5;
  if (x < 0.0) { t = (x + fay - 1.0) * 0.5; }
  v.x += t;
  v.y += y;
} else {
  var t = (y - fax + 1.0) * 0.5;
  if (y < 0.0) { t = (y + fax - 1.0) * 0.5; }
  v.x += x;
  v.y += t;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "honeycomb",
        params: &[ParamDef::new("scale", 1.0), ParamDef::new("pull", 0.7)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sc = java_max(d[0].abs(), 0.01);
            let pull = java_max(0.0, java_min(1.0, d[1]));
            let q = (2.0 / 3.0) * a.x / sc;
            let r2 = (-1.0 / 3.0) * a.x / sc + 0.5773502691896257 * a.y / sc;
            let (mut rq, mut rr, rs) = (java_round(q), java_round(r2), java_round(-q - r2));
            let (dq, dr, ds) = ((rq - q).abs(), (rr - r2).abs(), (rs - (-q - r2)).abs());
            if dq > dr && dq > ds {
                rq = -rr - rs;
            } else if dr > ds {
                rr = -rq - rs;
            }
            let cx = sc * (rq + 0.5 * rr);
            let cy = sc * 0.8660254037844386 * rr;
            v.x += amount * (cx + (a.x - cx) * pull);
            v.y += amount * (cy + (a.y - cy) * pull);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sc = max(abs(D(0)), 0.01);
let pull = clamp(D(1), 0.0, 1.0);
let q = (2.0 / 3.0) * a.x / sc;
let r2 = (-1.0 / 3.0) * a.x / sc + 0.5773502691896257 * a.y / sc;
var rq = java_round(q);
var rr = java_round(r2);
let rs = java_round(-q - r2);
let dq = abs(rq - q);
let dr = abs(rr - r2);
let ds = abs(rs - (-q - r2));
if (dq > dr && dq > ds) { rq = -rr - rs; } else if (dr > ds) { rr = -rq - rs; }
let cx = sc * (rq + 0.5 * rr);
let cy = sc * 0.8660254037844386 * rr;
v.x += AMOUNT * (cx + (a.x - cx) * pull);
v.y += AMOUNT * (cy + (a.y - cy) * pull);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hopalong",
        params: &[
            ParamDef::int("random", 1.0, 0.0, 10000000.0),
            ParamDef::range("a", 0.75, -1.0, 1.0),
            ParamDef::range("b", 0.5, -1.0, 1.0),
            ParamDef::range("c", 0.25, -1.0, 1.0),
            ParamDef::new("startx", 0.0),
            ParamDef::new("starty", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _, v, amount| {
            // Java follows one endless orbit per instance; here one per render thread.
            let p = ctx.orbits.entry(d.as_ptr() as usize).or_insert([d[4], d[5]]);
            let (x0, y0) = (p[0], p[1]);
            // Math.signum
            let sg = if x0 > 0.0 { 1.0 } else if x0 < 0.0 { -1.0 } else { x0 };
            let x1 = y0 - sg * (d[2] * x0 - d[3]).abs().sqrt();
            let y1 = d[1] - x0;
            *p = [x1, y1];
            v.x += amount * x1;
            v.y += amount * y1;
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for i in 1..4 {
                d[i] = limit_value(p[i], -1.0, 1.0);
            }
            d
        }),
        // GPU: no state across calls, every call iterates a slightly perturbed start point.
        wgsl: "var x0 = D(4) + 1.0e-2 * (rnd() - 0.5);
var y0 = D(5) + 1.0e-2 * (rnd() - 0.5);
let n = 32 + min(i32(rnd() * 1024.0), 1023);
for (var i = 0; i < n; i++) {
  let x1 = y0 - sign(x0) * sqrt(abs(D(2) * x0 - D(3)));
  y0 = D(1) - x0;
  x0 = x1;
}
v.x += AMOUNT * x0;
v.y += AMOUNT * y0;",
    },
    VarDef {
        name: "hourglass3D",
        params: &[
            ParamDef::new("kx", 1.0),
            ParamDef::new("ky", 1.0),
            ParamDef::new("kz", 1.0),
            // JWildfire: random in 3..6
            ParamDef::new("maxheight", 4.5),
            ParamDef::int("limit", 1.0, 0.0, 1.0),
            ParamDef::int("zero", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        // d = [kx, ky, kz, maxheight, limit, zero, h, hb]
        apply: |d, _, a, v, amount| {
            let (vkx, vky, vkz, h) = (amount * d[0], amount * d[1], amount * d[2], d[6]);
            let (sn, cn) = (a.x.sin(), a.x.cos());
            let (sh, ch) = (a.y.sinh(), a.y.cosh());
            if d[4] == 0.0 || ch <= h {
                v.x += vkx * ch * sn;
                v.y += vky * sh;
                v.z += vkz * ch * cn;
            } else if d[5] == 0.0 {
                let sg = if sh > 0.0 { 1.0 } else if sh < 0.0 { -1.0 } else { 0.0 };
                v.x += vkx * h * sn;
                v.y += vky * sg * d[7];
                v.z += vkz * h * cn;
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            for k in &mut d[0..3] {
                if *k < EPSILON {
                    *k = EPSILON;
                }
            }
            if d[3] < 0.1 {
                d[3] = 0.1;
            }
            d[4] = if p[4] <= EPSILON { 0.0 } else { 1.0 };
            d[5] = if p[5] <= EPSILON { 0.0 } else { 1.0 };
            let h = (1.0 + (d[3] / d[1]) * (d[3] / d[1])).sqrt();
            d.extend([h, (h * h - 1.0).sqrt()]);
            d
        }),
        wgsl: "let vk = AMOUNT * vec3<f32>(D(0), D(1), D(2));
let h = D(6);
let sn = sin(a.x);
let cn = cos(a.x);
let sh = sinh(a.y);
let ch = cosh(a.y);
if (D(4) == 0.0 || ch <= h) {
  v.x += vk.x * ch * sn;
  v.y += vk.y * sh;
  v.z += vk.z * ch * cn;
} else if (D(5) == 0.0) {
  v.x += vk.x * h * sn;
  v.y += vk.y * sign(sh) * D(7);
  v.z += vk.z * h * cn;
}",
    },
    VarDef {
        name: "htree_js",
        params: &[
            ParamDef::int("level", 2.0, 1.0, 10.0),
            ParamDef::new("size", 2.0),
            ParamDef::int("show_lines", 1.0, 0.0, 1.0),
            ParamDef::new("line_thickness", 0.5),
            ParamDef::int("show_points", 0.0, 0.0, 1.0),
            ParamDef::new("point_thickness", 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // Java cycles through the segments; here a random segment is drawn.
            let nseg = d[10] as usize;
            let k = ((ctx.random() * nseg as f64) as usize).min(nseg - 1);
            let o = 11 + 4 * k;
            let (x1, y1, x2, y2) = (d[o], d[o + 1], d[o + 2], d[o + 3]);
            let rnd = ctx.random();
            let (mut xo, mut yo) = (0.0, 0.0);
            if rnd < d[6] {
                let (xdiff, ydiff) = (x2 - x1, y2 - y1);
                let m = if xdiff == 0.0 { 10000.0 } else { ydiff / xdiff };
                let dd = ctx.random() * (xdiff * xdiff + ydiff * ydiff).sqrt();
                let mut xoffset = dd / (1.0 + m * m).sqrt();
                if x2 < x1 {
                    xoffset = -xoffset;
                }
                let mut yoffset = (m * xoffset).abs();
                if y2 < y1 {
                    yoffset = -yoffset;
                }
                if d[8] != 0.0 {
                    xoffset += (ctx.random() - 0.5) * d[8];
                    yoffset += (ctx.random() - 0.5) * d[8];
                }
                xo = x1 + xoffset;
                yo = y1 + yoffset;
            } else if rnd <= d[7] {
                let (mut xoffset, mut yoffset) = (0.0, 0.0);
                if d[9] != 0.0 {
                    let roffset = ctx.random() * d[9];
                    let rangle = ctx.random() * M_2PI;
                    xoffset = roffset * rangle.cos();
                    yoffset = roffset * rangle.sin();
                }
                xo = x1 + xoffset;
                yo = y1 + yoffset;
            }
            v.x += amount * xo;
            v.y += amount * yo;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| htree_prepare(p)),
        wgsl: "let nseg = u32(D(10));
let k = min(u32(rnd() * f32(nseg)), nseg - 1u);
let o = base + 11u + 4u * k;
let x1 = prm[o];
let y1 = prm[o + 1u];
let x2 = prm[o + 2u];
let y2 = prm[o + 3u];
let r0 = rnd();
var q = vec2<f32>(0.0);
if (r0 < D(6)) {
  let xdiff = x2 - x1;
  let ydiff = y2 - y1;
  var m = 10000.0;
  if (xdiff != 0.0) { m = ydiff / xdiff; }
  let dd = rnd() * sqrt(xdiff * xdiff + ydiff * ydiff);
  var xoffset = dd / sqrt(1.0 + m * m);
  if (x2 < x1) { xoffset = -xoffset; }
  var yoffset = abs(m * xoffset);
  if (y2 < y1) { yoffset = -yoffset; }
  if (D(8) != 0.0) {
    xoffset += (rnd() - 0.5) * D(8);
    yoffset += (rnd() - 0.5) * D(8);
  }
  q = vec2<f32>(x1 + xoffset, y1 + yoffset);
} else if (r0 <= D(7)) {
  q = vec2<f32>(x1, y1);
  if (D(9) != 0.0) {
    let roffset = rnd() * D(9);
    let rangle = rnd() * TWO_PI;
    q += roffset * vec2<f32>(cos(rangle), sin(rangle));
  }
}
v.x += AMOUNT * q.x;
v.y += AMOUNT * q.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "hyperbolicellipse",
        params: &[ParamDef::new("a", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, _, a, v, amount| {
            let (x, y) = (a.x, a.y);
            v.x = ((x.exp() - (-x).exp()) / 2.0) * (d[0] * y).cos() * amount;
            v.y = ((x.exp() + (-x).exp()) / 2.0) * (d[0] * y).sin() * amount;
        },
        prepare: None,
        wgsl: "v.x = ((exp(a.x) - exp(-a.x)) / 2.0) * cos(D(0) * a.y) * AMOUNT;
v.y = ((exp(a.x) + exp(-a.x)) / 2.0) * sin(D(0) * a.y) * AMOUNT;",
    },
    VarDef {
        name: "hypercrop",
        params: &[ParamDef::int("n", 4.0, 1.0, 100.0), ParamDef::new("rad", 1.0), ParamDef::new("zero", 0.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let n = d[0];
            let coef = n * 0.5 / M_PI;
            let (mut fx, mut fy, mut fz) = (a.x, a.y, a.z);
            let a0 = M_PI / n;
            let len = 1.0 / a0.cos();
            let dd = d[1] * a0.sin() * len;
            let angle = (a.y.atan2(a.x) * coef).floor() / coef + M_PI / n;
            let (x0, y0) = (angle.cos() * len, angle.sin() * len);
            if ((a.x - x0) * (a.x - x0) + (a.y - y0) * (a.y - y0)).sqrt() < dd {
                if d[2] > 1.5 {
                    (fx, fy, fz) = (x0, y0, 0.0);
                } else if d[2] > 0.5 {
                    (fx, fy, fz) = (0.0, 0.0, 0.0);
                } else {
                    let rangle = (a.y - y0).atan2(a.x - x0);
                    (fx, fy, fz) = (x0 + rangle.cos() * dd, y0 + rangle.sin() * dd, 0.0);
                }
            }
            v.x += fx * amount;
            v.y += fy * amount;
            v.z += fz * amount;
        },
        prepare: Some(|p, _| vec![ftoi(p[0]) as f64, p[1], p[2]]),
        wgsl: "let n = D(0);
let coef = n * 0.5 / PI;
var f = vec3<f32>(a.x, a.y, a.z);
let a0 = PI / n;
let len = 1.0 / cos(a0);
let dd = D(1) * sin(a0) * len;
let angle = floor(atan2(a.y, a.x) * coef) / coef + PI / n;
let c0 = vec2<f32>(cos(angle), sin(angle)) * len;
if (length(vec2<f32>(a.x, a.y) - c0) < dd) {
  if (D(2) > 1.5) {
    f = vec3<f32>(c0, 0.0);
  } else if (D(2) > 0.5) {
    f = vec3<f32>(0.0);
  } else {
    let rangle = atan2(a.y - c0.y, a.x - c0.x);
    f = vec3<f32>(c0 + dd * vec2<f32>(cos(rangle), sin(rangle)), 0.0);
  }
}
v.x += f.x * AMOUNT;
v.y += f.y * AMOUNT;
v.z += f.z * AMOUNT;",
    },
];
