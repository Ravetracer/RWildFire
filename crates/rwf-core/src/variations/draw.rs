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

//! `DrawFunc` sampling (random points in n-gons and triangles) and the variations built on it
//! that keep their drawing parameters in string resources: `gpattern`, `triantruchet`.
//!
//! WGSL counterparts: `res_ngon_point`, `res_triangle_point` in `wgsl/resource.wgsl`.

use super::resource_vars::text;
use super::{Ctx, LoadEnv, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::math::*;
use crate::rng::JavaRandom;

/// `DrawFunc.randXY` + `plotPolygon`: a random point of a regular n-gon (unit "radius" to the
/// edge midpoints); `fill` is the hole ratio (1 = only the outline).
pub(super) fn ngon_point(ctx: &mut Ctx, sides: i32, fill: f64) -> (f64, f64) {
    let n = sides.max(3);
    let mid = M_2PI / n as f64;
    let tan90_m_2 = (M_PI_2 + mid / 2.0).tan();
    let arc_tan1 = 13.0 / (n as f64).powf(1.3);
    let arc_tan2 = 2.0 * (arc_tan1 / -2.0).atan();
    let mut ang = ((arc_tan1 * (ctx.random() - 0.5)).atan() / arc_tan2 + 0.5 + (ctx.random_int(32767) as i32 % n) as f64) * mid;
    let (mut x, mut y) = (ang.sin(), ang.cos());
    while ang > mid {
        ang -= mid;
    }
    let xt = tan90_m_2 / (tan90_m_2 - ang.tan());
    let yt = xt * ang.tan();
    let outer = (xt * xt + yt * yt).sqrt();
    let _ = ctx.random(); // the first radius draw is overwritten in JWildfire
    let inner = fill * outer;
    let r = inner + ctx.random().sqrt() * (outer - inner);
    x *= r;
    y *= r;
    let (s, c) = (mid / 2.0).sin_cos();
    (c * x - s * y, s * x + c * y)
}

/// `DrawFunc.plotTriangle`: uniform point in a triangle.
pub(super) fn triangle_point(ctx: &mut Ctx, p: [(f64, f64); 3]) -> (f64, f64) {
    let sqrt_r1 = ctx.random().sqrt();
    let r2 = ctx.random();
    let (a, b, c) = (1.0 - sqrt_r1, sqrt_r1 * (1.0 - r2), r2 * sqrt_r1);
    (a * p[0].0 + b * p[1].0 + c * p[2].0, a * p[0].1 + b * p[1].1 + c * p[2].1)
}

/// Java `String.split(",")` (trailing empty strings removed) + `Integer.parseInt` (0 on error).
fn java_split_ints(s: &str) -> Vec<i32> {
    let mut parts: Vec<&str> = s.split(',').collect();
    while parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    parts.iter().map(|p| p.parse().unwrap_or(0)).collect()
}

// ---------------------------------------------------------------------------------------------
// gpattern (GPatternFunc)

/// `gpattern` data: 12 parameters, `[count, start]`, n-gons
/// `[sides, scale, cos, sin, x, y, color, fill]`.
const GP_N: usize = 12;
const NGON: usize = 8;

fn gpattern_load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
    // d = [seed, width, height, lineheight, angle, randPos, randSize, fill, ouline, fill color,
    //      fill color speed, outline color, ..]
    let polys: Vec<i32> = java_split_ints(&text(&res[0])).into_iter().map(|p| if !(3..=20).contains(&p) { 3 } else { p }).collect();
    let (width, height, lh, angle, rpos, rsize) = (d[1], d[2], d[3], d[4], d[5], d[6]);
    let (fill, outline, color, speed, ocolor) = (d[7] as i32, d[8] as i32, d[9], d[10], d[11]);
    let (s, c) = (angle * M_PI / 180.0).sin_cos();
    let mut rnd = JavaRandom::new(d[0] as i64);
    let mut ngons: Vec<[f64; NGON]> = Vec::new();
    let mut posj = 0.0;
    while posj <= width {
        let mut y = 0.0;
        let mut k: i32 = 0;
        while y <= height {
            let x = posj + (-rpos + 2.0 * rpos * rnd.next_double());
            let r = (lh / 2.0) as f32 as f64 + (-rsize + 2.0 * rsize * rnd.next_double());
            let sides = polys[k as usize] as f64;
            if outline == 1 {
                ngons.push([sides, r, c, s, x, y, ocolor, outline as f64]);
            }
            if fill == 1 {
                let col = (color + speed * rnd.next_double()) % 1.0;
                ngons.push([sides, r, c, s, x, y, col, 0.0]);
            }
            if k > polys.len() as i32 - 2 {
                k = -1;
            }
            k += 1;
            y += lh;
        }
        posj += lh;
    }
    d[GP_N] = ngons.len() as f64;
    d[GP_N + 1] = d.len() as f64;
    d.extend(ngons.into_iter().flatten());
}

fn gpattern_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let n = d[GP_N] as usize;
    if n == 0 {
        return;
    }
    let g = &d[d[GP_N + 1] as usize + NGON * ((ctx.random() * n as f64) as usize).min(n - 1)..];
    let (x, y) = ngon_point(ctx, g[0] as i32, g[7]);
    let (scale, c, s) = (g[1], g[2], g[3]);
    v.x += x * scale * c + y * scale * s + g[4];
    v.y += -x * scale * s + y * scale * c + g[5];
    v.color = g[6];
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// triantruchet (TrianTruchetFunc)

#[allow(clippy::ptr_arg)] // `LoadFn` signature
fn triantruchet_load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
    // d = [seed, Size, TilesPerRow, TilesPerColumn, n1, n2, n3, n4, ..]
    let mut rnd = JavaRandom::new(d[0] as i64);
    rnd.next_double();
    let mut n: [f64; 4] = [0.0; 4];
    for v in &mut n {
        *v = (rnd.next_double() * 4.0).trunc();
    }
    if d[0] as i32 == 0 {
        // the string's four digits (JWildfire keeps the random ones if it cannot parse them)
        let s: Vec<char> = text(&res[0]).chars().collect();
        let digits: Option<Vec<f64>> = (0..4).map(|i| s.get(i).and_then(|c| c.to_digit(10)).map(|v| v as f64)).collect();
        if let Some(dg) = digits {
            n.copy_from_slice(&dg);
        }
    }
    d[4..8].copy_from_slice(&n);
}

fn triantruchet_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let (rows, cols) = (d[2] as i32, d[3] as i32);
    let ts = d[1] as f32 / rows as f32;
    let i = (ctx.random() * rows as f64) as i32;
    let j = (ctx.random() * cols as f64) as i32;
    let rot = match (i % 2 == 0, j % 2 == 0) {
        (true, true) => d[4],
        (true, false) => d[6],
        (false, true) => d[5],
        (false, false) => d[7],
    };
    // Tile (float precision as in JWildfire)
    let (cx, cy) = (i as f32 * ts + ts / 2.0, j as f32 * ts + ts / 2.0);
    let tilt = (0.0f32 as f64 + rot * M_PI / 2.0) as f32;
    let (st, ct) = ((tilt as f64).sin(), (tilt as f64).cos());
    let h = ts / 2.0;
    let corner = |x: f32, y: f32| (((x as f64 * ct + y as f64 * st) + cx as f64) as f32 as f64, ((-x as f64 * st + y as f64 * ct) + cy as f64) as f32 as f64);
    let tri = [corner(-h, -h), corner(-h, h), corner(h, h)];
    let (x, y) = triangle_point(ctx, tri);
    v.x += amount * (x - (ts * rows as f32) as f64 / 2.0);
    v.y += amount * (y - (ts * cols as f32) as f64 / 2.0);
    v.color = (rows * j + i) as f64 / (rows * cols) as f64;
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "gpattern",
        params: &[
            ParamDef::int("seed", 10000.0, 0.0, 1000000.0),
            ParamDef::range("width", 4.0, 0.0, 10.0),
            ParamDef::range("height", 2.0, 0.0, 10.0),
            ParamDef::range("lineheight", 0.15, 0.01, 1.5),
            ParamDef::range("angle", 0.0, -180.0, 180.0),
            ParamDef::new("randPos", 0.0),
            ParamDef::range("randSize", 0.0, -1.0, 1.0),
            ParamDef::int("fill", 1.0, 0.0, 1.0),
            ParamDef::int("ouline", 0.0, 0.0, 1.0),
            ParamDef::range("fill color", 0.0, 0.0, 1.0),
            ParamDef::range("fill color speed", 1.0, 0.0, 1.0),
            ParamDef::range("outline color", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| gpattern_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let lim = limit_value;
            let mut d = vec![
                p[0].trunc(),
                lim(p[1], 0.0, 10.0),
                lim(p[2], 0.0, 10.0),
                lim(p[3], 0.01, 1.5),
                p[4],
                p[5],
                lim(p[6], -1.0, 1.0),
                lim(p[7], 0.0, 1.0).trunc(),
                lim(p[8], 0.0, 1.0).trunc(),
                lim(p[9], 0.0, 1.0),
                lim(p[10], 0.0, 1.0),
                lim(p[11], 0.0, 1.0),
            ];
            d.extend([0.0; 2]);
            d
        }),
        wgsl: "let n = D(12);
if (n > 0.0) {
  let g = base + u32(D(13)) + 8u * min(u32(rnd() * n), u32(n) - 1u);
  let q = res_ngon_point(i32(prm[g]), prm[g + 7u]);
  let sc = prm[g + 1u];
  v.x += q.x * sc * prm[g + 2u] + q.y * sc * prm[g + 3u] + prm[g + 4u];
  v.y += -q.x * sc * prm[g + 3u] + q.y * sc * prm[g + 2u] + prm[g + 5u];
  v.color = prm[g + 6u];
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    VarDef {
        name: "triantruchet",
        params: &[
            ParamDef::int("seed", 10000.0, 0.0, 1000000.0),
            ParamDef::range("Size", 2.0, 0.0, 10.0),
            ParamDef::int("TilesPerRow", 10.0, 2.0, 600.0),
            ParamDef::int("TilesPerColumn", 10.0, 2.0, 600.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| triantruchet_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let mut d = vec![p[0].trunc(), limit_value(p[1], 0.0, 10.0), limit_value(p[2], 2.0, 600.0).trunc(), limit_value(p[3], 2.0, 600.0).trunc()];
            d.extend([0.0; 4]);
            d
        }),
        wgsl: "let rows = i32(D(2));
let cols = i32(D(3));
let ts = D(1) / f32(rows);
let i = i32(rnd() * f32(rows));
let j = i32(rnd() * f32(cols));
var rot = D(7);
if (i % 2 == 0 && j % 2 == 0) { rot = D(4); }
else if (i % 2 == 0) { rot = D(6); }
else if (j % 2 == 0) { rot = D(5); }
let c = vec2<f32>(f32(i) * ts + ts / 2.0, f32(j) * ts + ts / 2.0);
let tilt = rot * PI / 2.0;
let st = sin(tilt);
let ct = cos(tilt);
let h = ts / 2.0;
let p1 = vec2<f32>(-h * ct - h * st, h * st - h * ct) + c;
let p2 = vec2<f32>(-h * ct + h * st, h * st + h * ct) + c;
let p3 = vec2<f32>(h * ct + h * st, -h * st + h * ct) + c;
let q = res_triangle_point(p1, p2, p3);
v.x += AMOUNT * (q.x - ts * f32(rows) / 2.0);
v.y += AMOUNT * (q.y - ts * f32(cols) / 2.0);
v.color = f32(rows * j + i) / f32(rows * cols);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];

pub static RESOURCES: &[ResourceVar] = &[
    ResourceVar { name: "gpattern", resources: &[ResDef { name: "string", kind: ResKind::Text, default: "3,4,5,6,5,4,3" }], load: gpattern_load },
    ResourceVar { name: "triantruchet", resources: &[ResDef { name: "string", kind: ResKind::Text, default: "0132" }], load: triantruchet_load },
];

#[cfg(test)]
mod tests {
    use super::super::{find, prepare_instance};
    use super::*;
    use crate::model::{Affine, VariationInstance};

    #[test]
    fn split_like_java() {
        assert_eq!(java_split_ints("3,4,5"), vec![3, 4, 5]);
        assert_eq!(java_split_ints("3,x,,5,,"), vec![3, 0, 0, 5]);
    }

    #[test]
    fn ngon_points_inside() {
        let mut ctx = Ctx::new(3, false);
        for _ in 0..1000 {
            // vertices at distance 1, edge midpoints at cos(pi / n)
            let (x, y) = ngon_point(&mut ctx, 6, 0.0);
            assert!(x.hypot(y) <= 1.0 + 1e-9);
            // outline only: on the edges
            let (x, y) = ngon_point(&mut ctx, 6, 1.0);
            assert!(x.hypot(y) >= (M_PI / 6.0).cos() - 1e-9);
        }
    }

    #[test]
    fn variations_run() {
        for name in ["gpattern", "triantruchet"] {
            let def = find(name).unwrap();
            let d = prepare_instance(def, &VariationInstance::new(name, 1.0), &Affine::IDENTITY, 1);
            let mut ctx = Ctx::new(1, false);
            for _ in 0..200 {
                let mut v = Pt::default();
                (def.apply)(&d, &mut ctx, &mut Pt::default(), &mut v, 1.0);
                assert!(!v.is_bad() && (0.0..=1.0).contains(&v.color), "{name} {v:?}");
            }
        }
    }
}
