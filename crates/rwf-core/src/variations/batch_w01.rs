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

//! Variations ported from the JWildfire sources (batch w01).

use super::complex::Complex;
#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;
use crate::rng::JavaRandom;

// ---------------------------------------------------------------------------------------------
// affine3D (AbstractAffine3DFunc)
// d = [translateX, translateY, translateZ, scaleX, scaleY, scaleZ, rotateX, rotateY, rotateZ,
//      shearXY, shearXZ, shearYX, shearYZ, shearZX, shearZY,
//      sinX, cosX, sinY, cosY, sinZ, cosZ, has_shear]

fn affine3d_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    for &rot in &p[6..9] {
        let ang = rot * M_PI / 180.0;
        d.push(ang.sin());
        d.push(ang.cos());
    }
    let has_shear = p[9..15].iter().any(|s| s.abs() > EPSILON);
    d.push(if has_shear { 1.0 } else { 0.0 });
    d
}

fn affine3d_transform(d: &[f64], x: f64, y: f64, z: f64, amount: f64) -> (f64, f64, f64) {
    let (sx, sy, sz) = (d[3], d[4], d[5]);
    let (sin_x, cos_x, sin_y, cos_y, sin_z, cos_z) = (d[15], d[16], d[17], d[18], d[19], d[20]);
    let (u, w, t) = if d[21] != 0.0 {
        (
            d[9] * sy * y + d[10] * sz * z + sx * x,
            d[11] * sx * x + d[12] * sz * z + sy * y,
            d[13] * sx * x + d[14] * sy * y + sz * z,
        )
    } else {
        (sx * x, sy * y, sz * z)
    };
    let m = cos_y * u + sin_y * (sin_x * w + cos_x * t);
    let n = cos_x * w - sin_x * t;
    (
        amount * (cos_z * m - sin_z * n + d[0]),
        amount * (sin_z * m + cos_z * n + d[1]),
        amount * (-sin_y * u + cos_y * (sin_x * w + cos_x * t) + d[2]),
    )
}

// ---------------------------------------------------------------------------------------------
// arctruchet: d = [seed, thickness, TilesPerRow, TilesPerColumn, tilt[0..rows*cols]]

const ARCTRUCHET_RADIUS: f64 = 0.25;

fn arctruchet_prepare(p: &[f64]) -> Vec<f64> {
    let seed = limit_value(p[0], 0.0, 100000.0) as i32;
    let thickness = limit_value(p[1], 0.0, 1.0);
    let rows = limit_value(p[2], 1.0, 100.0) as i32;
    let cols = limit_value(p[3], 1.0, 100.0) as i32;
    let mut d = vec![seed as f64, thickness, rows as f64, cols as f64];
    let mut tilt = vec![0.0; (rows * cols) as usize];
    let mut rnd = JavaRandom::new(seed as i64);
    rnd.next_double();
    for i in 0..rows {
        for j in 0..cols {
            tilt[(j * rows + i) as usize] = (rnd.next_double() * 4.0) as i32 as f64;
        }
    }
    d.extend(tilt);
    // state of the tile picker after the setup (continued by `transform`)
    d.push(rnd.seed as f64);
    d
}

// ---------------------------------------------------------------------------------------------
// attractor_flow
// d = [preset, attractorMode, waveMode, zMode, yScale, xScale, zScale, xAmplitude, yAmplitude,
//      zAmplitude, freqX, freqY, phaseX, phaseY] (preset applied)

fn attractor_flow_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    for k in 0..4 {
        d[k] = (p[k] as i32) as f64;
    }
    // Choosing a preset overwrites the other parameters (Java `applyPreset`).
    match d[0] as i32 {
        1 => {
            d[1..6].copy_from_slice(&[0.0, 0.0, 0.0, -1.7, 1.8]);
            d[7] = -0.5;
            d[8] = -1.9;
            d[10..14].copy_from_slice(&[1.0, 1.0, 0.0, 0.0]);
        }
        2 => {
            d[1..4].copy_from_slice(&[1.0, 1.0, 0.0]);
            d[4] = -2.0;
            d[5] = -2.0;
            d[7] = 1.2;
            d[8] = -1.9;
        }
        3 => {
            d[1..4].copy_from_slice(&[2.0, 2.0, 0.0]);
            d[4] = 1.6;
            d[5] = -1.9;
            d[7] = 1.2;
            d[8] = -1.8;
        }
        4 => {
            d[1..4].copy_from_slice(&[0.0, 0.0, 1.0]);
            d[4] = 1.2;
            d[5] = 1.2;
            d[6] = 1.4;
            d[7] = -1.8;
            d[8] = -1.3;
            d[9] = 1.1;
        }
        _ => {}
    }
    d
}

fn attractor_flow_wave(value: f64, mode: i32) -> f64 {
    match mode {
        1 => sgn(value.sin()),
        2 => value.sin().asin() * (2.0 / M_PI),
        3 => 2.0 * (value / (2.0 * M_PI) - (0.5 + value / (2.0 * M_PI)).floor()),
        _ => value.sin(),
    }
}

fn attractor_flow_cwave(value: f64, mode: i32) -> f64 {
    attractor_flow_wave(value + M_PI_2, mode)
}

// ---------------------------------------------------------------------------------------------
// bbox3D / box3D signed distance functions

fn bbox3d_sd(p: [f64; 3], b: [f64; 3], e: f64) -> f64 {
    let p = [p[0].abs() - b[0], p[1].abs() - b[1], p[2].abs() - b[2]];
    let q = [(p[0] + e).abs() - e, (p[1] + e).abs() - e, (p[2] + e).abs() - e];
    let term = |x: f64, y: f64, z: f64| {
        (sqr(x.max(0.0)) + sqr(y.max(0.0)) + sqr(z.max(0.0))).sqrt() + x.max(y.max(z)).min(0.0)
    };
    term(p[0], q[1], q[2]).min(term(q[0], p[1], q[2])).min(term(q[0], q[1], p[2]))
}

fn box3d_sd(p: [f64; 3], b: [f64; 3]) -> f64 {
    let q = [p[0].abs() - b[0], p[1].abs() - b[1], p[2].abs() - b[2]];
    (sqr(q[0].max(0.0)) + sqr(q[1].max(0.0)) + sqr(q[2].max(0.0))).sqrt() + q[0].max(q[1].max(q[2])).min(0.0)
}

// ---------------------------------------------------------------------------------------------
// brownian_js
// d = [level, variation, seed, line_thickness, show_lines, show_points, point_thickness,
//      line_thickness/100, point_thickness/100, line_threshold, point_threshold, n_segments,
//      path (n_segments + 1 points as x, y)]

const BROWNIAN_PATH: usize = 12;

fn brownian_gaussian(rnd: &mut JavaRandom) -> f64 {
    loop {
        let x = -1.0 + rnd.next_double() * 2.0;
        let y = -1.0 + rnd.next_double() * 2.0;
        let r = x * x + y * y;
        if !(r >= 1.0 || r == 0.0) {
            return x * (-2.0 * r.ln() / r).sqrt();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn brownian_midpoint(rnd: &mut JavaRandom, path: &mut Vec<f64>, x0: f64, y0: f64, x1: f64, y1: f64, var: f64, n: i32) {
    if n == 0 {
        // consecutive leaf segments share their end points: store the path only
        if path.is_empty() {
            path.push(x0);
            path.push(y0);
        }
        path.push(x1);
        path.push(y1);
        return;
    }
    let xmid = 0.5 * (x0 + x1) + (0.0 + var.sqrt() * brownian_gaussian(rnd));
    let ymid = 0.5 * (y0 + y1) + (0.0 + var.sqrt() * brownian_gaussian(rnd));
    brownian_midpoint(rnd, path, x0, y0, xmid, ymid, var / 2.7, n - 1);
    brownian_midpoint(rnd, path, xmid, ymid, x1, y1, var / 2.7, n - 1);
}

fn brownian_prepare(p: &[f64]) -> Vec<f64> {
    let level = limit_value(ftoi(p[0]), 1, 15);
    let variation = p[1];
    let seed = p[2] as i32;
    let show_sum = p[4] + p[5];
    let line_fraction = p[4] / show_sum;
    let point_fraction = p[5] / show_sum;
    let mut d = vec![
        level as f64,
        variation,
        seed as f64,
        p[3],
        p[4],
        p[5],
        p[6],
        p[3] / 100.0,
        p[6] / 100.0,
        line_fraction,
        line_fraction + point_fraction,
        (1u32 << level) as f64,
    ];
    // Java seeds 0 with the current time; here seed 0 is used as is.
    let mut rnd = JavaRandom::new(seed as i64);
    let mut path = Vec::with_capacity(2 * ((1usize << level) + 1));
    brownian_midpoint(&mut rnd, &mut path, 0.0, 0.0, 0.0, 0.0, variation / 2f64.sqrt(), level);
    d.extend(path);
    d
}

// ---------------------------------------------------------------------------------------------
// bubbleT3D
// d = [number_of_stripes, ratio_of_stripes, angle_of_hole (rad), exponent_z, symmetry_z, modus_blur,
//      angStrip1, angStrip2, s, c, invHole, angHoleComp]

fn bubblet3d_prepare(p: &[f64]) -> Vec<f64> {
    let stripes = ftoi(p[0]).max(0);
    let mut ratio = limit_value(p[1], 0.0, 2.0);
    let mut hole = p[2];
    let exponent_z = p[3];
    let symmetry_z = ftoi(p[4]);
    let modus_blur = ftoi(p[5]);
    let (mut strip1, mut strip2, mut s, mut c) = (0.0, 0.0, 0.0, 0.0);
    if stripes != 0 {
        let strip = M_PI / stripes as f64;
        strip2 = 2.0 * strip;
        s = strip.sin();
        c = strip.cos();
        ratio = ratio.clamp(0.01, 1.99);
        strip1 = ratio * strip;
    }
    if symmetry_z == 1 {
        if hole < 0.0 {
            hole = hole.abs();
        }
        if hole > 179.9 {
            hole = 179.9;
        }
    }
    let inv_hole;
    if hole < 0.0 {
        hole = hole.abs();
        inv_hole = 1.0;
        hole = (hole / 360.0 * M_2PI) / 2.0;
    } else {
        inv_hole = 0.0;
        hole = M_PI - (hole / 360.0 * M_2PI) / 2.0;
    }
    vec![
        stripes as f64,
        ratio,
        hole,
        exponent_z,
        symmetry_z as f64,
        modus_blur as f64,
        strip1,
        strip2,
        s,
        c,
        inv_hole,
        M_PI - hole,
    ]
}

fn bubblet3d_bend(x: &mut f64, y: &mut f64, z: &mut f64, ang_tmp: f64, ang_z: f64) {
    let ang_z = ang_z - M_PI_2;
    let fac = ang_tmp.cos() / ang_z.cos();
    *x *= fac;
    *y *= fac;
    *z *= ang_tmp.sin() / ang_z.sin();
}

fn bubblet3d_apply(d: &[f64], a: &Pt, v: &mut Pt, amount: f64) {
    let (mut x, mut y) = (a.x, a.y);
    let mut z;
    let rad = (sqr(x) + sqr(y)) / 4.0 + 1.0;
    let modus_blur = d[5] as i32;
    let exponent_z = d[3];
    // stripes (number_of_stripes is clamped to >= 0, so invStripes is always FALSE)
    let mut ang_xy = x.atan2(y);
    if ang_xy < 0.0 {
        ang_xy += M_2PI;
    }
    if d[0] != 0.0 {
        let (strip1, strip2) = (d[6], d[7]);
        while ang_xy > strip2 {
            ang_xy -= strip2;
        }
        if ang_xy > strip1 {
            if modus_blur == 0 {
                x = 0.0;
                y = 0.0;
            } else {
                let (s, c) = if d[1] == 1.0 {
                    (d[8], d[9])
                } else {
                    let ang_rot = (ang_xy - strip1) / (strip2 - strip1);
                    let ang_rot = ang_xy - ang_rot * strip1;
                    ang_rot.sin_cos()
                };
                let xt = c * x - s * y;
                let yt = s * x + c * y;
                x = xt;
                y = yt;
            }
        }
    }
    // holes
    x /= rad;
    y /= rad;
    let ang_z;
    if x != 0.0 || y != 0.0 {
        z = 2.0 / rad.powf(exponent_z) - 1.0;
        ang_z = if exponent_z <= 2.0 {
            M_PI - (z / (x * x + y * y + z * z)).acos()
        } else {
            M_PI - sqr(x * x + y * y).atan2(z)
        };
    } else {
        z = 0.0;
        ang_z = 0.0;
    }
    let hole = d[2];
    let comp = d[11];
    let cut = modus_blur == 0 || exponent_z != 1.0;
    if d[4] as i32 == 0 {
        if d[10] == 0.0 {
            if ang_z > hole {
                if cut {
                    x = 0.0;
                    y = 0.0;
                    z = 0.0;
                } else {
                    let ang_tmp = (M_PI - ang_z) / comp * hole - M_PI_2;
                    bubblet3d_bend(&mut x, &mut y, &mut z, ang_tmp, ang_z);
                }
            }
        } else if ang_z < hole {
            if cut {
                x = 0.0;
                y = 0.0;
                z = 0.0;
            } else {
                let ang_tmp = M_PI - ang_z / comp * hole - M_PI_2;
                bubblet3d_bend(&mut x, &mut y, &mut z, ang_tmp, ang_z);
            }
        }
    } else if ang_z > hole || ang_z < (M_PI - hole) {
        if cut {
            x = 0.0;
            y = 0.0;
            z = 0.0;
        } else {
            let ang_tmp = if ang_z > hole {
                (M_PI - ang_z) / comp * (M_PI - 2.0 * comp) + comp - M_PI_2
            } else {
                M_PI_2 - (ang_z / comp * (M_PI - 2.0 * comp) + comp)
            };
            bubblet3d_bend(&mut x, &mut y, &mut z, ang_tmp, ang_z);
        }
    }
    v.x += amount * x;
    v.y += amount * y;
    v.z += amount * z;
}

// ---------------------------------------------------------------------------------------------
// boxfold: d = [foldLimit, rotateX, rotateY, rotateZ, sinX, cosX, sinY, cosY, sinZ, cosZ]

/// `rotatePoint`: Z, then Y, then X rotation (sign `sg` = -1 for the negated angles).
fn boxfold_rotate(d: &[f64], p: [f64; 3], sg: f64) -> [f64; 3] {
    let [mut x, mut y, mut z] = p;
    if d[3] != 0.0 {
        let (s, c) = (sg * d[8], d[9]);
        let tx = x * c - y * s;
        let ty = x * s + y * c;
        x = tx;
        y = ty;
    }
    if d[2] != 0.0 {
        let (s, c) = (sg * d[6], d[7]);
        let tx = x * c + z * s;
        let tz = -x * s + z * c;
        x = tx;
        z = tz;
    }
    if d[1] != 0.0 {
        let (s, c) = (sg * d[4], d[5]);
        let ty = y * c - z * s;
        let tz = y * s + z * c;
        y = ty;
        z = tz;
    }
    [x, y, z]
}

fn boxfold_fold(v: f64, f: f64) -> f64 {
    if v > f {
        2.0 * f - v
    } else if v < -f {
        -2.0 * f - v
    } else {
        v
    }
}

// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "abs_fold",
        params: &[ParamDef::new("fold", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let f = d[0];
            let fold = |n: f64| {
                if n > f {
                    2.0 * f - n
                } else if n < -f {
                    -2.0 * f - n
                } else {
                    n
                }
            };
            v.x += amount * fold(a.x);
            v.y += amount * fold(a.y);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let f = D(0);
         var nx = a.x;
         var ny = a.y;
         if (nx > f) { nx = 2.0 * f - nx; } else if (nx < -f) { nx = -2.0 * f - nx; }
         if (ny > f) { ny = 2.0 * f - ny; } else if (ny < -f) { ny = -2.0 * f - ny; }
         v.x += AMOUNT * nx;
         v.y += AMOUNT * ny;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "acosech",
        false,
        |_, ctx, a, v, amount| {
            let mut z = Complex::new(a.x, a.y);
            z.acosech();
            z.flip();
            z.scale(amount * M_2_PI);
            if ctx.random() < 0.5 {
                v.y += z.im;
                v.x += z.re;
            } else {
                v.y += -z.im;
                v.x += -z.re;
            }
            ctx.pass_z(a, v, amount);
        },
        "let z = cx_scale(cx_flip(cx_acosech(vec2<f32>(a.x, a.y))), AMOUNT * 2.0 / PI);
         if (rnd() < 0.5) { v.x += z.x; v.y += z.y; } else { v.x -= z.x; v.y -= z.y; }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "acosh",
        false,
        |_, ctx, a, v, amount| {
            let mut z = Complex::new(a.x, a.y);
            z.acosh();
            z.scale(amount * M_2_PI);
            if ctx.random() < 0.5 {
                v.y += z.im;
                v.x += z.re;
            } else {
                v.y += -z.im;
                v.x += -z.re;
            }
            ctx.pass_z(a, v, amount);
        },
        "let z = cx_scale(cx_acosh(vec2<f32>(a.x, a.y)), AMOUNT * 2.0 / PI);
         if (rnd() < 0.5) { v.x += z.x; v.y += z.y; } else { v.x -= z.x; v.y -= z.y; }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "acoth",
        false,
        |_, ctx, a, v, amount| {
            let mut z = Complex::new(a.x, a.y);
            z.acoth();
            z.flip();
            z.scale(amount * M_2_PI);
            v.y += z.im;
            v.x += z.re;
            ctx.pass_z(a, v, amount);
        },
        "let z = cx_scale(cx_flip(cx_acoth(vec2<f32>(a.x, a.y))), AMOUNT * 2.0 / PI);
         v.x += z.x;
         v.y += z.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "affine3D",
        params: &[
            ParamDef::new("translateX", 0.0),
            ParamDef::new("translateY", 0.0),
            ParamDef::new("translateZ", 0.0),
            ParamDef::new("scaleX", 1.0),
            ParamDef::new("scaleY", 1.0),
            ParamDef::new("scaleZ", 1.0),
            ParamDef::range("rotateX", 0.0, -180.0, 180.0),
            ParamDef::range("rotateY", 0.0, -180.0, 180.0),
            ParamDef::range("rotateZ", 0.0, -180.0, 180.0),
            ParamDef::new("shearXY", 0.0),
            ParamDef::new("shearXZ", 0.0),
            ParamDef::new("shearYX", 0.0),
            ParamDef::new("shearYZ", 0.0),
            ParamDef::new("shearZX", 0.0),
            ParamDef::new("shearZY", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let (x, y, z) = affine3d_transform(d, a.x, a.y, a.z, amount);
            v.x += x;
            v.y += y;
            v.z += z;
        },
        prepare: Some(|p, _| affine3d_prepare(p)),
        wgsl: "var u = D(3) * a.x;
         var w = D(4) * a.y;
         var t = D(5) * a.z;
         if (D(21) != 0.0) {
           u = D(9) * D(4) * a.y + D(10) * D(5) * a.z + D(3) * a.x;
           w = D(11) * D(3) * a.x + D(12) * D(5) * a.z + D(4) * a.y;
           t = D(13) * D(3) * a.x + D(14) * D(4) * a.y + D(5) * a.z;
         }
         let m = D(18) * u + D(17) * (D(15) * w + D(16) * t);
         let n = D(16) * w - D(15) * t;
         v.x += AMOUNT * (D(20) * m - D(19) * n + D(0));
         v.y += AMOUNT * (D(19) * m + D(20) * n + D(1));
         v.z += AMOUNT * (-D(17) * u + D(18) * (D(15) * w + D(16) * t) + D(2));",
    },
    VarDef {
        name: "afterimage",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("len", 0.4), ParamDef::new("decay", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let decay = java_max(d[2].abs(), 0.01);
            let smear = d[1] * (d[0] * a.y).sin() * (-a.y * a.y * decay).exp();
            v.x += amount * (a.x + smear);
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let decay = max(abs(D(2)), 0.01);
         let smear = D(1) * sin(D(0) * a.y) * exp(-a.y * a.y * decay);
         v.x += AMOUNT * (a.x + smear);
         v.y += AMOUNT * a.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "aftershock",
        params: &[ParamDef::new("freq", 5.0), ParamDef::new("amp", 0.3), ParamDef::new("decay", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let decay = java_max(d[2].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let ring = d[1] * (d[0] * r).sin() * (-decay * r).exp();
            let nr = r + ring;
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let decay = max(abs(D(2)), 0.01);
         let r = sqrt(a.x * a.x + a.y * a.y);
         let theta = atan2(a.y, a.x);
         let nr = r + D(1) * sin(D(0) * r) * exp(-decay * r);
         v.x += AMOUNT * nr * cos(theta);
         v.y += AMOUNT * nr * sin(theta);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "amoeba",
        params: &[ParamDef::range("lobes", 4.0, 2.0, 15.0), ParamDef::new("amp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let n = java_max(d[0].abs(), 2.0);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let nr = r * (1.0 + d[1] * (n * theta).cos() + 0.4 * d[1] * ((n + 1.0) * theta + 0.5).sin());
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let n = max(abs(D(0)), 2.0);
         let r = sqrt(a.x * a.x + a.y * a.y);
         let theta = atan2(a.y, a.x);
         let nr = r * (1.0 + D(1) * cos(n * theta) + 0.4 * D(1) * sin((n + 1.0) * theta + 0.5));
         v.x += AMOUNT * nr * cos(theta);
         v.y += AMOUNT * nr * sin(theta);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "anamorphcyl",
        params: &[ParamDef::new("a", 1.0), ParamDef::new("b", 1.3), ParamDef::new("k", 3.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // Java assigns (does not accumulate) x/y and leaves z untouched.
        apply: |d, _, a, v, amount| {
            let xt = d[0] * (a.y + d[1]) * (d[2] * a.x).cos();
            let yt = d[0] * (a.y + d[1]) * (d[2] * a.x).sin();
            v.x = xt * amount;
            v.y = yt * amount;
        },
        prepare: None,
        wgsl: "let r = D(0) * (a.y + D(1));
         v.x = r * cos(D(2) * a.x) * AMOUNT;
         v.y = r * sin(D(2) * a.x) * AMOUNT;",
    },
    simple(
        "apocarpet_js",
        false,
        |_, ctx, a, v, amount| {
            let r = 1.0 / (1.0 + 2f64.sqrt());
            let denom = a.x * a.x + a.y * a.y;
            let (x, y) = match (6.0 * ctx.random()) as i32 {
                0 => (2.0 * a.x * a.y / denom, (a.x * a.x - a.y * a.y) / denom),
                1 => (a.x * r - r, a.y * r - r),
                2 => (a.x * r + r, a.y * r + r),
                3 => (a.x * r + r, a.y * r - r),
                4 => (a.x * r - r, a.y * r + r),
                5 => ((a.x * a.x - a.y * a.y) / denom, 2.0 * a.x * a.y / denom),
                _ => (0.0, 0.0),
            };
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        "let r = 1.0 / (1.0 + sqrt(2.0));
         let denom = a.x * a.x + a.y * a.y;
         let w = i32(6.0 * rnd());
         var x = 0.0;
         var y = 0.0;
         switch (w) {
           case 0: { x = 2.0 * a.x * a.y / denom; y = (a.x * a.x - a.y * a.y) / denom; }
           case 1: { x = a.x * r - r; y = a.y * r - r; }
           case 2: { x = a.x * r + r; y = a.y * r + r; }
           case 3: { x = a.x * r + r; y = a.y * r - r; }
           case 4: { x = a.x * r - r; y = a.y * r + r; }
           case 5: { x = (a.x * a.x - a.y * a.y) / denom; y = 2.0 * a.x * a.y / denom; }
           default: { }
         }
         v.x += x * AMOUNT;
         v.y += y * AMOUNT;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "apollony",
        false,
        |_, ctx, a, v, amount| {
            let r = 3f64.sqrt();
            let t = 1.0 + r - a.x;
            let den = t * t + a.y * a.y;
            let a0 = 3.0 * t / den - (1.0 + r) / (2.0 + r);
            let b0 = 3.0 * a.y / den;
            let n = a0 * a0 + b0 * b0;
            let f1x = a0 / n;
            let f1y = -b0 / n;
            let w = (4.0 * ctx.random()) as i32;
            let (x, y) = match w % 3 {
                0 => (a0, b0),
                1 => (-f1x / 2.0 - f1y * r / 2.0, f1x * r / 2.0 - f1y / 2.0),
                _ => (-f1x / 2.0 + f1y * r / 2.0, -f1x * r / 2.0 - f1y / 2.0),
            };
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        "let r = sqrt(3.0);
         let t = 1.0 + r - a.x;
         let den = t * t + a.y * a.y;
         let a0 = 3.0 * t / den - (1.0 + r) / (2.0 + r);
         let b0 = 3.0 * a.y / den;
         let n = a0 * a0 + b0 * b0;
         let f1x = a0 / n;
         let f1y = -b0 / n;
         let w = i32(4.0 * rnd()) % 3;
         var x = -f1x / 2.0 + f1y * r / 2.0;
         var y = -f1x * r / 2.0 - f1y / 2.0;
         if (w == 0) { x = a0; y = b0; }
         else if (w == 1) { x = -f1x / 2.0 - f1y * r / 2.0; y = f1x * r / 2.0 - f1y / 2.0; }
         v.x += x * AMOUNT;
         v.y += y * AMOUNT;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "arcsech2",
        false,
        |_, ctx, a, v, amount| {
            let mut z = Complex::new(a.x, a.y);
            z.recip();
            let mut z2 = Complex::copy_of(&z);
            z2.dec();
            z2.sqrt();
            let mut z3 = Complex::copy_of(&z);
            z3.inc();
            z3.sqrt();
            z3.mul(&z2);
            z.add(&z3);
            z.log();
            z.scale(amount * 2.0 * M_1_PI);
            v.y += z.im;
            if z.im < 0.0 {
                v.x += z.re;
                v.y += 1.0;
            } else {
                v.x -= z.re;
                v.y -= 1.0;
            }
            ctx.pass_z(a, v, amount);
        },
        "let z = cx_recip(vec2<f32>(a.x, a.y));
         let z2 = cx_sqrt(cx_dec(z));
         let z3 = cx_mul(cx_sqrt(cx_inc(z)), z2);
         let w = cx_scale(cx_log(cx_add(z, z3)), AMOUNT * 2.0 / PI);
         v.y += w.y;
         if (w.y < 0.0) { v.x += w.x; v.y += 1.0; } else { v.x -= w.x; v.y -= 1.0; }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "arcsinh",
        false,
        |_, ctx, a, v, amount| {
            let mut z = Complex::new(a.x, a.y);
            z.asinh();
            z.scale(amount * M_2_PI);
            v.x += z.re;
            v.y += z.im;
            ctx.pass_z(a, v, amount);
        },
        "let z = cx_scale(cx_asinh(vec2<f32>(a.x, a.y)), AMOUNT * 2.0 / PI);
         v.x += z.x;
         v.y += z.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "arctanh",
        false,
        |_, ctx, a, v, amount| {
            let z = Complex::new(a.x, a.y);
            let mut z2 = Complex::copy_of(&z);
            z2.scale(-1.0);
            z2.inc();
            let mut z3 = Complex::copy_of(&z);
            z3.inc();
            z3.div(&z2);
            z3.log();
            z3.scale(amount * M_2_PI);
            v.x += z3.re;
            v.y += z3.im;
            ctx.pass_z(a, v, amount);
        },
        "let z = vec2<f32>(a.x, a.y);
         let w = cx_scale(cx_log(cx_div(cx_inc(z), cx_inc(cx_scale(z, -1.0)))), AMOUNT * 2.0 / PI);
         v.x += w.x;
         v.y += w.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "arctruchet",
        params: &[
            ParamDef::int("seed", 10000.0, 0.0, 100000.0),
            ParamDef::range("thickness", 0.025, 0.0, 1.0),
            ParamDef::int("TilesPerRow", 10.0, 1.0, 100.0),
            ParamDef::int("TilesPerColumn", 10.0, 1.0, 100.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // The tile picker continues the seeded java.util.Random of the setup, per render thread
        // (JWildfire: per instance). The GPU picks tiles with the shared random generator.
        apply: |d, ctx, a, v, amount| {
            const TILE: f64 = 2.0 * ARCTRUCHET_RADIUS;
            let rows = d[2] as i32;
            let cols = d[3] as i32;
            let key = d.as_ptr() as usize;
            let seed = ctx.states.get(&key).map_or(d[4 + (rows * cols) as usize], |st| st[0]);
            let mut picker = JavaRandom { seed: seed as u64 };
            let i = (picker.next_double() * rows as f64) as i32;
            let j = (picker.next_double() * cols as f64) as i32;
            ctx.states.insert(key, vec![picker.seed as f64]);
            let x = (i as f64 * TILE + TILE / 2.0) as f32 as f64;
            let y = (j as f64 * TILE + TILE / 2.0) as f32 as f64;
            let tilt = d[4 + (j * rows + i) as usize] * 90.0;
            let (phi1, phi2) = if ctx.random() < 0.5 { (0.0, 90.0) } else { (180.0, 270.0) };
            // Tile.arc
            let thickness = d[1];
            let radius = ARCTRUCHET_RADIUS;
            let phi10 = M_PI * phi1 / 180.0;
            let phi20 = M_PI * phi2 / 180.0;
            let delta = phi20 - phi10;
            let gamma = thickness * (2.0 * radius + thickness) / (radius + thickness);
            let r = radius + thickness - gamma * ctx.random();
            let phi = phi10 + delta * ctx.random();
            let xp = r * phi.cos();
            let yp = r * phi.sin();
            let ang = tilt * M_PI / 180.0;
            let (sa, ca) = ang.sin_cos();
            let mut px = (xp * ca + yp * sa) as f32 as f64;
            let mut py = (-xp * sa + yp * ca) as f32 as f64;
            let radio = if phi2 == 270.0 { -radius } else { radius };
            px -= (radio * ca + radio * sa) as f32 as f64;
            py -= (-radio * sa + radio * ca) as f32 as f64;
            v.x += amount * (px + x - TILE * rows as f64 / 2.0);
            v.y += amount * (py + y - TILE * cols as f64 / 2.0);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| arctruchet_prepare(p)),
        wgsl: "let rows = D(2);
         let cols = D(3);
         let i = min(floor(rnd() * rows), rows - 1.0);
         let j = min(floor(rnd() * cols), cols - 1.0);
         let x = i * 0.5 + 0.25;
         let y = j * 0.5 + 0.25;
         let tilt = prm[base + 4u + u32(j * rows + i)] * 90.0;
         var phi1 = PI;
         var phi2 = 1.5 * PI;
         var radio = -0.25;
         if (rnd() < 0.5) { phi1 = 0.0; phi2 = HALF_PI; radio = 0.25; }
         let th = D(1);
         let gamma = th * (0.5 + th) / (0.25 + th);
         let r = 0.25 + th - gamma * rnd();
         let phi = phi1 + (phi2 - phi1) * rnd();
         let xp = r * cos(phi);
         let yp = r * sin(phi);
         let ang = tilt * PI / 180.0;
         let ca = cos(ang);
         let sa = sin(ang);
         let px = (xp * ca + yp * sa) - (radio * ca + radio * sa);
         let py = (-xp * sa + yp * ca) - (-radio * sa + radio * ca);
         v.x += AMOUNT * (px + x - 0.5 * rows / 2.0);
         v.y += AMOUNT * (py + y - 0.5 * cols / 2.0);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "asteria",
        params: &[ParamDef::new("alpha", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [alpha, sina, cosa]
        apply: |d, ctx, a, v, amount| {
            let x0 = amount * a.x;
            let y0 = amount * a.y;
            let r = sqr(x0) + sqr(y0);
            let xx = sqr(x0.abs() - 1.0);
            let yy = sqr(y0.abs() - 1.0);
            let r2 = (yy + xx).sqrt();
            let mut in1 = r < 1.0;
            let out2 = r2 < 1.0;
            if in1 && out2 {
                in1 = ctx.random() > 0.35;
            } else {
                in1 = !in1;
            }
            if in1 {
                v.x += x0;
                v.y += y0;
            } else {
                let (sina, cosa) = (d[1], d[2]);
                let xx = x0 * cosa - y0 * sina;
                let yy = x0 * sina + y0 * cosa;
                let nx = xx / (1.0 - yy * yy).sqrt() * (1.0 - (1.0 - sqr(-yy.abs() + 1.0)).sqrt());
                v.x += nx * cosa + yy * sina;
                v.y += -nx * sina + yy * cosa;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], (M_PI * p[0]).sin(), (M_PI * p[0]).cos()]),
        wgsl: "let x0 = AMOUNT * a.x;
         let y0 = AMOUNT * a.y;
         let r = x0 * x0 + y0 * y0;
         let r2 = sqrt(sqr(abs(y0) - 1.0) + sqr(abs(x0) - 1.0));
         var in1 = r < 1.0;
         let out2 = r2 < 1.0;
         if (in1 && out2) { in1 = rnd() > 0.35; } else { in1 = !in1; }
         if (in1) {
           v.x += x0;
           v.y += y0;
         } else {
           let sina = D(1);
           let cosa = D(2);
           let xx = x0 * cosa - y0 * sina;
           let yy = x0 * sina + y0 * cosa;
           let nx = xx / sqrt(1.0 - yy * yy) * (1.0 - sqrt(1.0 - sqr(-abs(yy) + 1.0)));
           v.x += nx * cosa + yy * sina;
           v.y += -nx * sina + yy * cosa;
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "astroid",
        params: &[ParamDef::new("a", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let theta = a.y.atan2(a.x);
            let (st, ct) = theta.sin_cos();
            v.x += amount * d[0] * ct * ct * ct;
            v.y += amount * d[0] * st * st * st;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let theta = atan2(a.y, a.x);
         let ct = cos(theta);
         let st = sin(theta);
         v.x += AMOUNT * D(0) * ct * ct * ct;
         v.y += AMOUNT * D(0) * st * st * st;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "atan2_spirals",
        params: &[
            ParamDef::new("r_mult", 1.5),
            ParamDef::new("r_add", 1.0),
            ParamDef::new("xy2_mult", 1.125),
            ParamDef::new("xy2_add", 0.132),
            ParamDef::new("x_mult", 2.0),
            ParamDef::new("x_add", 0.25),
            ParamDef::new("yx_div", 1.0),
            ParamDef::new("yx_add", 0.0),
            ParamDef::new("yy_div", 1.25),
            ParamDef::new("yy_add", 0.0),
            ParamDef::new("sin_add", 0.0),
            ParamDef::new("y_mult", 1.0),
            ParamDef::new("r_power", 0.5),
            ParamDef::new("x2y2_power", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x, y) = (a.x, a.y);
            let xs = x * x;
            let ys = y * y;
            let xy2 = (xs + ys).powf(d[13]);
            let r = (xs + ys).powf(d[12]);
            let fx = d[4] * (r * d[0] + d[1]).atan2(xy2 * d[2] + d[3]) + d[5];
            let fy = ((y / d[8] + d[9]).atan2((x / d[6]) + d[7]) + d[10]).sin() * d[11];
            if x >= 0.0 {
                v.x += amount * (-fx + M_PI);
            } else {
                v.x += amount * (fx - M_PI);
            }
            v.y += amount * fy;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let s = a.x * a.x + a.y * a.y;
         let xy2 = pow(s, D(13));
         let r = pow(s, D(12));
         let fx = D(4) * atan2(r * D(0) + D(1), xy2 * D(2) + D(3)) + D(5);
         let fy = sin(atan2(a.y / D(8) + D(9), (a.x / D(6)) + D(7)) + D(10)) * D(11);
         if (a.x >= 0.0) { v.x += AMOUNT * (-fx + PI); } else { v.x += AMOUNT * (fx - PI); }
         v.y += AMOUNT * fy;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "atan",
        params: &[ParamDef::int("Mode", 0.0, 0.0, 2.0), ParamDef::new("Stretch", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let norm = 1.0 / M_PI_2 * amount;
            match d[0] as i32 {
                0 => {
                    v.x += amount * a.x;
                    v.y += norm * (d[1] * a.y).atan();
                }
                1 => {
                    v.x += norm * (d[1] * a.x).atan();
                    v.y += amount * a.y;
                }
                _ => {
                    v.x += norm * (d[1] * a.x).atan();
                    v.y += norm * (d[1] * a.y).atan();
                }
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![limit_value(ftoi(p[0]), 0, 2) as f64, p[1]]),
        wgsl: "let norm = 1.0 / HALF_PI * AMOUNT;
         let mode = i32(D(0));
         if (mode == 0) {
           v.x += AMOUNT * a.x;
           v.y += norm * atan(D(1) * a.y);
         } else if (mode == 1) {
           v.x += norm * atan(D(1) * a.x);
           v.y += AMOUNT * a.y;
         } else {
           v.x += norm * atan(D(1) * a.x);
           v.y += norm * atan(D(1) * a.y);
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "attractor_flow",
        params: &[
            ParamDef::int("preset", 0.0, 0.0, 4.0),
            ParamDef::int("attractorMode", 0.0, 0.0, 3.0),
            ParamDef::int("waveMode", 0.0, 0.0, 3.0),
            ParamDef::int("zMode", 0.0, 0.0, 1.0),
            ParamDef::new("yScale", -1.7),
            ParamDef::new("xScale", 1.8),
            ParamDef::new("zScale", 1.5),
            ParamDef::new("xAmplitude", -0.5),
            ParamDef::new("yAmplitude", -1.9),
            ParamDef::new("zAmplitude", 1.2),
            ParamDef::new("freqX", 1.0),
            ParamDef::new("freqY", 1.0),
            ParamDef::new("phaseX", 0.0),
            ParamDef::new("phaseY", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            let (x, y, z) = (a.x, a.y, a.z);
            let wm = d[2] as i32;
            let zm = d[3] as i32;
            let (y_scale, x_scale, z_scale) = (d[4], d[5], d[6]);
            let (x_amp, y_amp, z_amp) = (d[7], d[8], d[9]);
            let (freq_x, freq_y, phase_x, phase_y) = (d[10], d[11], d[12], d[13]);
            let w = |val: f64| attractor_flow_wave(val, wm);
            let cw = |val: f64| attractor_flow_cwave(val, wm);
            let xz = if zm == 1 { z } else { x };
            let (new_x, new_y) = match d[1] as i32 {
                1 => (w(y_scale * y) - cw(x_scale * x), w(x_amp * xz) - cw(y_amp * y)),
                2 => (
                    y_amp * w(y_scale * x) - w(x_scale * y),
                    x_amp * cw(y_scale * xz) + cw(x_scale * y),
                ),
                3 => {
                    let g_x = x_amp * x + (2.0 * (1.0 - x_amp) * x * x) / (1.0 + x * x);
                    (-x + g_x, (y_scale * y) + g_x)
                }
                _ => (
                    w(y_scale * y * freq_y + phase_y) + x_amp * cw(y_scale * x * freq_x + phase_x),
                    w(x_scale * xz * freq_x + phase_x) + y_amp * cw(x_scale * y * freq_y + phase_y),
                ),
            };
            v.x += (new_x - x) * amount;
            v.y += (new_y - y) * amount;
            if ctx.preserve_z {
                if zm == 1 {
                    let new_z = w(z_scale * x) + z_amp * cw(z_scale * z);
                    v.z += (new_z - z) * amount;
                } else {
                    let new_z = w(x_scale * z) + x_amp * cw(y_amp * z);
                    v.z += new_z * amount;
                }
            }
        },
        prepare: Some(|p, _| attractor_flow_prepare(p)),
        wgsl: "fn attractor_flow_wave(val: f32, mode: i32) -> f32 {
           if (mode == 1) { return sign(sin(val)); }
           if (mode == 2) { return asin(clamp(sin(val), -1.0, 1.0)) * (2.0 / PI); }
           if (mode == 3) { return 2.0 * (val / TWO_PI - floor(0.5 + val / TWO_PI)); }
           return sin(val);
         }
         fn attractor_flow_cwave(val: f32, mode: i32) -> f32 { return attractor_flow_wave(val + HALF_PI, mode); }
         @body
         let x = a.x;
         let y = a.y;
         let z = a.z;
         let wm = i32(D(2));
         let zm = i32(D(3));
         let ys = D(4);
         let xs = D(5);
         let zs = D(6);
         let xa = D(7);
         let ya = D(8);
         let za = D(9);
         var xz = x;
         if (zm == 1) { xz = z; }
         let am = i32(D(1));
         var nx = 0.0;
         var ny = 0.0;
         if (am == 1) {
           nx = attractor_flow_wave(ys * y, wm) - attractor_flow_cwave(xs * x, wm);
           ny = attractor_flow_wave(xa * xz, wm) - attractor_flow_cwave(ya * y, wm);
         } else if (am == 2) {
           nx = ya * attractor_flow_wave(ys * x, wm) - attractor_flow_wave(xs * y, wm);
           ny = xa * attractor_flow_cwave(ys * xz, wm) + attractor_flow_cwave(xs * y, wm);
         } else if (am == 3) {
           let g_x = xa * x + (2.0 * (1.0 - xa) * x * x) / (1.0 + x * x);
           ny = (ys * y) + g_x;
           nx = -x + g_x;
         } else {
           nx = attractor_flow_wave(ys * y * D(11) + D(13), wm) + xa * attractor_flow_cwave(ys * x * D(10) + D(12), wm);
           ny = attractor_flow_wave(xs * xz * D(10) + D(12), wm) + ya * attractor_flow_cwave(xs * y * D(11) + D(13), wm);
         }
         v.x += (nx - x) * AMOUNT;
         v.y += (ny - y) * AMOUNT;
         if (PRESERVE_Z) {
           if (zm == 1) {
             let nz = attractor_flow_wave(zs * x, wm) + za * attractor_flow_cwave(zs * z, wm);
             v.z += (nz - z) * AMOUNT;
           } else {
             let nz = attractor_flow_wave(xs * z, wm) + xa * attractor_flow_cwave(ya * z, wm);
             v.z += nz * AMOUNT;
           }
         }",
    },
    VarDef {
        name: "aurora",
        params: &[
            ParamDef::new("freq", 4.0),
            ParamDef::new("amp", 0.5),
            ParamDef::new("width", 0.3),
            ParamDef::new("lat", 0.3),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let width = java_max(d[2].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let ov = 1.0 + d[3] * theta.cos();
            let band = r - ov;
            let env = (-band * band / (width * width)).exp();
            let wave = d[1] * (d[0] * theta).sin() * env;
            v.x += amount * (a.x + wave * theta.cos());
            v.y += amount * (a.y + wave * theta.sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let width = max(abs(D(2)), 0.01);
         let r = sqrt(a.x * a.x + a.y * a.y);
         let theta = atan2(a.y, a.x);
         let band = r - (1.0 + D(3) * cos(theta));
         let env = exp(-band * band / (width * width));
         let wave = D(1) * sin(D(0) * theta) * env;
         v.x += AMOUNT * (a.x + wave * cos(theta));
         v.y += AMOUNT * (a.y + wave * sin(theta));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "aurora_curtain",
        params: &[ParamDef::new("freq", 2.0), ParamDef::new("amp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let f = d[0];
            let dx = d[1] * ((f * a.y).sin() + 0.5 * (2.0 * f * a.y + 1.1).sin() + 0.25 * (3.0 * f * a.y + 2.3).sin());
            v.x += amount * (a.x + dx);
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let f = D(0);
         let dx = D(1) * (sin(f * a.y) + 0.5 * sin(2.0 * f * a.y + 1.1) + 0.25 * sin(3.0 * f * a.y + 2.3));
         v.x += AMOUNT * (a.x + dx);
         v.y += AMOUNT * a.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "azimuthal",
        params: &[ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let nr = if r > 0.001 { (r * d[0]).sin() / d[0] } else { r };
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
         let theta = atan2(a.y, a.x);
         var nr = r;
         if (r > 0.001) { nr = sin(r * D(0)) / D(0); }
         v.x += AMOUNT * nr * cos(theta);
         v.y += AMOUNT * nr * sin(theta);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bbox3D",
        params: &[
            ParamDef::range("dx", 0.3, 0.0, 0.5),
            ParamDef::range("dy", 0.25, 0.0, 0.5),
            ParamDef::range("dz", 0.2, 0.0, 0.5),
            ParamDef::range("r", 0.05, 0.0, 0.25),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| {
            for _ in 0..50 {
                let x = ctx.random() - 0.5;
                let y = ctx.random() - 0.5;
                let z = ctx.random() - 0.5;
                if bbox3d_sd([x, y, z], [d[0], d[1], d[2]], d[3]) < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| {
            vec![
                limit_value(p[0], 0.0, 0.5),
                limit_value(p[1], 0.0, 0.5),
                limit_value(p[2], 0.0, 0.5),
                limit_value(p[3], 0.0, 0.25),
            ]
        }),
        wgsl: "fn bbox3D_term(x: f32, y: f32, z: f32) -> f32 {
           return length(max(vec3<f32>(x, y, z), vec3<f32>(0.0))) + min(max(x, max(y, z)), 0.0);
         }
         fn bbox3D_sd(p0: vec3<f32>, b: vec3<f32>, e: f32) -> f32 {
           let p = abs(p0) - b;
           let q = abs(p + e) - e;
           return min(min(bbox3D_term(p.x, q.y, q.z), bbox3D_term(q.x, p.y, q.z)), bbox3D_term(q.x, q.y, p.z));
         }
         @body
         var found = false;
         for (var i = 0; i < 50; i++) {
           let x = rnd() - 0.5;
           let y = rnd() - 0.5;
           let z = rnd() - 0.5;
           if (bbox3D_sd(vec3<f32>(x, y, z), vec3<f32>(D(0), D(1), D(2)), D(3)) < 0.0) {
             v.x += AMOUNT * x;
             v.y += AMOUNT * y;
             v.z += AMOUNT * z;
             v.hide = 0.0;
             found = true;
             break;
           }
         }
         if (!found) { v.hide = 1.0; }",
    },
    VarDef {
        name: "bCollide",
        params: &[ParamDef::int("num", 1.0, 1.0, 50.0), ParamDef::range("a", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [num, a, bCn_pi, pi_bCn, bCa_bCn]
        apply: |d, ctx, a, v, amount| {
            let tau = 0.5 * ((sqr(a.x + 1.0) + sqr(a.y)).ln() - (sqr(a.x - 1.0) + sqr(a.y)).ln());
            let mut sigma = M_PI - a.y.atan2(a.x + 1.0) - a.y.atan2(1.0 - a.x);
            let alt = (sigma * d[2]) as i32;
            if alt % 2 == 0 {
                sigma = alt as f64 * d[3] + fmod(sigma + d[4], d[3]);
            } else {
                sigma = alt as f64 * d[3] + fmod(sigma - d[4], d[3]);
            }
            let temp = tau.cosh() - sigma.cos();
            v.x += amount * tau.sinh() / temp;
            v.y += amount * sigma.sin() / temp;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let num = limit_value(ftoi(p[0]), 1, i32::MAX) as f64;
            let ap = limit_value(p[1], 0.0, 1.0);
            vec![num, ap, num * M_1_PI, M_PI / num, M_PI * ap / num]
        }),
        wgsl: "let tau = 0.5 * (log(sqr(a.x + 1.0) + sqr(a.y)) - log(sqr(a.x - 1.0) + sqr(a.y)));
         var sigma = PI - atan2(a.y, a.x + 1.0) - atan2(a.y, 1.0 - a.x);
         let alt = i32(sigma * D(2));
         if (alt % 2 == 0) {
           sigma = f32(alt) * D(3) + fmod(sigma + D(4), D(3));
         } else {
           sigma = f32(alt) * D(3) + fmod(sigma - D(4), D(3));
         }
         let temp = cosh(tau) - cos(sigma);
         v.x += AMOUNT * sinh(tau) / temp;
         v.y += AMOUNT * sin(sigma) / temp;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bedhead",
        params: &[ParamDef::new("a", -0.81), ParamDef::new("b", -0.92)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let b = d[1];
            let b_safe = if b.abs() < 0.001 { (if b + 0.00001 < 0.0 { -1.0 } else { 1.0 }) * 0.001 } else { b };
            let nx = (a.x * a.y / b_safe).sin() * a.y + (d[0] * a.x - a.y).cos();
            let ny = a.x + a.y.sin() / b_safe;
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let b = D(1);
         var b_safe = b;
         if (abs(b) < 0.001) { b_safe = select(1.0, -1.0, b + 0.00001 < 0.0) * 0.001; }
         let nx = sin(a.x * a.y / b_safe) * a.y + cos(D(0) * a.x - a.y);
         let ny = a.x + sin(a.y) / b_safe;
         v.x += AMOUNT * nx;
         v.y += AMOUNT * ny;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "bi_linear",
        false,
        |_, ctx, a, v, amount| {
            v.x += amount * a.y;
            v.y += amount * a.x;
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * a.y;
         v.y += AMOUNT * a.x;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "bipolar2",
        params: &[
            ParamDef::new("shift", 0.0),
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 2.0),
            ParamDef::new("c", 0.5),
            ParamDef::new("d", 1.0),
            ParamDef::new("e", 2.0),
            ParamDef::new("f1", 0.25),
            ParamDef::new("g1", 1.0),
            ParamDef::new("h", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let x2y2 = (a.x * a.x + a.y * a.y) * d[7];
            let t = x2y2 + d[1];
            let x2 = d[2] * a.x;
            let ps = -M_PI_2 * d[0];
            let mut y = d[3] * (d[5] * a.y).atan2(x2y2 - d[4]) + ps;
            if y > M_PI_2 {
                y = -M_PI_2 + fmod(y + M_PI_2, M_PI);
            } else if y < -M_PI_2 {
                y = M_PI_2 - fmod(M_PI_2 - y, M_PI);
            }
            let f = t + x2;
            let g = t - x2;
            if g == 0.0 || f / g <= 0.0 {
                return;
            }
            v.x += amount * d[6] * M_2_PI * ((t + x2) / (t - x2)).ln();
            v.y += amount * M_2_PI * y * d[8];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x2y2 = (a.x * a.x + a.y * a.y) * D(7);
         let t = x2y2 + D(1);
         let x2 = D(2) * a.x;
         let ps = -HALF_PI * D(0);
         var y = D(3) * atan2(D(5) * a.y, x2y2 - D(4)) + ps;
         if (y > HALF_PI) {
           y = -HALF_PI + fmod(y + HALF_PI, PI);
         } else if (y < -HALF_PI) {
           y = HALF_PI - fmod(HALF_PI - y, PI);
         }
         let f = t + x2;
         let g = t - x2;
         if (!(g == 0.0 || f / g <= 0.0)) {
           v.x += AMOUNT * D(6) * (2.0 / PI) * log((t + x2) / (t - x2));
           v.y += AMOUNT * (2.0 / PI) * y * D(8);
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    },
    simple(
        "blade3D",
        true,
        |_, ctx, a, v, amount| {
            let r = ctx.random() * amount * (a.x * a.x + a.y * a.y).sqrt();
            let (sinr, cosr) = r.sin_cos();
            v.x += amount * a.x * (cosr + sinr);
            v.y += amount * a.x * (cosr - sinr);
            v.z += amount * a.y * (sinr - cosr);
        },
        "let r = rnd() * AMOUNT * sqrt(a.x * a.x + a.y * a.y);
         let sinr = sin(r);
         let cosr = cos(r);
         v.x += AMOUNT * a.x * (cosr + sinr);
         v.y += AMOUNT * a.x * (cosr - sinr);
         v.z += AMOUNT * a.y * (sinr - cosr);",
    ),
    VarDef {
        name: "blob3D",
        params: &[ParamDef::new("low", 0.3), ParamDef::new("high", 1.2), ParamDef::range("waves", 6.0, 1.0, 31.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let ang = a.x.atan2(a.y);
            let mut r = (a.x * a.x + a.y * a.y).sqrt();
            r *= d[0] + (d[1] - d[0]) * (0.5 + 0.5 * (d[2] * ang).sin());
            v.x += amount * ang.sin() * r;
            v.y += amount * ang.cos() * r;
            v.z += amount * (d[2] * ang).sin() * r;
        },
        prepare: None,
        wgsl: "let ang = atan2(a.x, a.y);
         let r = sqrt(a.x * a.x + a.y * a.y) * (D(0) + (D(1) - D(0)) * (0.5 + 0.5 * sin(D(2) * ang)));
         v.x += AMOUNT * sin(ang) * r;
         v.y += AMOUNT * cos(ang) * r;
         v.z += AMOUNT * sin(D(2) * ang) * r;",
    },
    VarDef {
        name: "blocky",
        params: &[ParamDef::new("x", 1.0), ParamDef::new("y", 1.0), ParamDef::new("mp", 4.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sqrt_safe = |x: f64| if x < 0.0 { 0.0 } else { x.sqrt() };
            let vv = amount / M_PI_2;
            let t = (a.x.cos() + a.y.cos()) / d[2] + 1.0;
            let r = amount / t;
            let tmp = sqr(a.y) + sqr(a.x) + 1.0;
            let x2 = 2.0 * a.x;
            let y2 = 2.0 * a.y;
            let xmax = 0.5 * ((tmp + x2).sqrt() + (tmp - x2).sqrt());
            let ymax = 0.5 * ((tmp + y2).sqrt() + (tmp - y2).sqrt());
            let aa = a.x / xmax;
            let b = sqrt_safe(1.0 - sqr(aa));
            v.x += vv * aa.atan2(b) * r * d[0];
            let aa = a.y / ymax;
            let b = sqrt_safe(1.0 - sqr(aa));
            v.y += vv * aa.atan2(b) * r * d[1];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let vv = AMOUNT / HALF_PI;
         let r = AMOUNT / ((cos(a.x) + cos(a.y)) / D(2) + 1.0);
         let tmp = sqr(a.y) + sqr(a.x) + 1.0;
         let x2 = 2.0 * a.x;
         let y2 = 2.0 * a.y;
         let xmax = 0.5 * (sqrt(max(tmp + x2, 0.0)) + sqrt(max(tmp - x2, 0.0)));
         let ymax = 0.5 * (sqrt(max(tmp + y2, 0.0)) + sqrt(max(tmp - y2, 0.0)));
         let ax = a.x / xmax;
         v.x += vv * atan2(ax, sqrt(max(1.0 - sqr(ax), 0.0))) * r * D(0);
         let ay = a.y / ymax;
         v.y += vv * atan2(ay, sqrt(max(1.0 - sqr(ay), 0.0))) * r * D(1);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "blur_linear",
        params: &[ParamDef::new("length", 1.0), ParamDef::range("angle", 0.5, 0.0, M_2PI)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [length, angle, s, c]
        apply: |d, ctx, a, v, amount| {
            let r = d[0] * ctx.random();
            v.x += amount * (a.x + r * d[3]);
            v.y += amount * (a.y + r * d[2]);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let angle = limit_value(p[1], 0.0, M_2PI);
            vec![p[0], angle, angle.sin(), angle.cos()]
        }),
        wgsl: "let r = D(0) * rnd();
         v.x += AMOUNT * (a.x + r * D(3));
         v.y += AMOUNT * (a.y + r * D(2));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "blur_pixelize",
        params: &[ParamDef::new("size", 0.1), ParamDef::new("scale", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [size, scale, inv_size]
        apply: |d, ctx, a, v, amount| {
            let vv = amount * d[0];
            let x = (a.x * d[2]).floor();
            let y = (a.y * d[2]).floor();
            v.x += vv * (x + d[1] * (ctx.random() - 0.5) + 0.5);
            v.y += vv * (y + d[1] * (ctx.random() - 0.5) + 0.5);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], 1.0 / p[0]]),
        wgsl: "let vv = AMOUNT * D(0);
         let x = floor(a.x * D(2));
         let y = floor(a.y * D(2));
         v.x += vv * (x + D(1) * (rnd() - 0.5) + 0.5);
         v.y += vv * (y + D(1) * (rnd() - 0.5) + 0.5);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "blur_zoom",
        params: &[ParamDef::new("length", 0.24), ParamDef::new("x", 0.2), ParamDef::new("y", -0.1)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let z = 1.0 + d[0] * ctx.random();
            v.x += amount * ((a.x - d[1]) * z + d[1]);
            v.y += amount * ((a.y + d[2]) * z - d[2]);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let z = 1.0 + D(0) * rnd();
         v.x += AMOUNT * ((a.x - D(1)) * z + D(1));
         v.y += AMOUNT * ((a.y + D(2)) * z - D(2));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bMod",
        params: &[ParamDef::range("radius", 1.0, 0.0, 5.0), ParamDef::range("distance", 0.0, 0.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (radius, distance) = (d[0], d[1]);
            let mut tau = 0.5 * ((sqr(a.x + 1.0) + sqr(a.y)).ln() - (sqr(a.x - 1.0) + sqr(a.y)).ln());
            let sigma = M_PI - a.y.atan2(a.x + 1.0) - a.y.atan2(1.0 - a.x);
            if tau < radius && -tau < radius {
                tau = fmod(tau + radius + distance * radius, 2.0 * radius) - radius;
            }
            let temp = tau.cosh() - sigma.cos();
            if temp == 0.0 {
                return;
            }
            v.x += amount * tau.sinh() / temp;
            v.y += amount * sigma.sin() / temp;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, f64::MAX), limit_value(p[1], 0.0, 2.0)]),
        wgsl: "let radius = D(0);
         var tau = 0.5 * (log(sqr(a.x + 1.0) + sqr(a.y)) - log(sqr(a.x - 1.0) + sqr(a.y)));
         let sigma = PI - atan2(a.y, a.x + 1.0) - atan2(a.y, 1.0 - a.x);
         if (tau < radius && -tau < radius) {
           tau = fmod(tau + radius + D(1) * radius, 2.0 * radius) - radius;
         }
         let temp = cosh(tau) - cos(sigma);
         if (temp != 0.0) {
           v.x += AMOUNT * sinh(tau) / temp;
           v.y += AMOUNT * sin(sigma) / temp;
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    },
    VarDef {
        name: "boarders2",
        params: &[ParamDef::new("c", 0.4), ParamDef::new("left", 0.65), ParamDef::new("right", 0.35)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [c, left, right, _c, _cl, _cr]
        apply: |d, ctx, a, v, amount| {
            let (c, cl, cr) = (d[3], d[4], d[5]);
            let round_x = rint(a.x);
            let round_y = rint(a.y);
            let offset_x = a.x - round_x;
            let offset_y = a.y - round_y;
            if ctx.random() >= cr {
                v.x += amount * (offset_x * c + round_x);
                v.y += amount * (offset_y * c + round_y);
            } else if offset_x.abs() >= offset_y.abs() {
                if offset_x >= 0.0 {
                    v.x += amount * (offset_x * c + round_x + cl);
                    v.y += amount * (offset_y * c + round_y + cl * offset_y / offset_x);
                } else {
                    v.x += amount * (offset_x * c + round_x - cl);
                    v.y += amount * (offset_y * c + round_y - cl * offset_y / offset_x);
                }
            } else if offset_y >= 0.0 {
                v.y += amount * (offset_y * c + round_y + cl);
                v.x += amount * (offset_x * c + round_x + offset_x / offset_y * cl);
            } else {
                v.y += amount * (offset_y * c + round_y - cl);
                v.x += amount * (offset_x * c + round_x - offset_x / offset_y * cl);
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let nz = |x: f64| if x == 0.0 { EPSILON } else { x };
            let c = nz(p[0].abs());
            let cl = nz(p[1].abs());
            let cr = nz(p[2].abs());
            vec![p[0], p[1], p[2], c, c * cl, c + c * cr]
        }),
        wgsl: "let c = D(3);
         let cl = D(4);
         let rx = round(a.x);
         let ry = round(a.y);
         let ox = a.x - rx;
         let oy = a.y - ry;
         if (rnd() >= D(5)) {
           v.x += AMOUNT * (ox * c + rx);
           v.y += AMOUNT * (oy * c + ry);
         } else if (abs(ox) >= abs(oy)) {
           if (ox >= 0.0) {
             v.x += AMOUNT * (ox * c + rx + cl);
             v.y += AMOUNT * (oy * c + ry + cl * oy / ox);
           } else {
             v.x += AMOUNT * (ox * c + rx - cl);
             v.y += AMOUNT * (oy * c + ry - cl * oy / ox);
           }
         } else if (oy >= 0.0) {
           v.y += AMOUNT * (oy * c + ry + cl);
           v.x += AMOUNT * (ox * c + rx + ox / oy * cl);
         } else {
           v.y += AMOUNT * (oy * c + ry - cl);
           v.x += AMOUNT * (ox * c + rx - ox / oy * cl);
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "box3D",
        params: &[
            ParamDef::range("dx", 0.5, 0.0, 0.5),
            ParamDef::range("dy", 0.5, 0.0, 0.5),
            ParamDef::range("dz", 0.5, 0.0, 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| {
            for _ in 0..50 {
                let x = ctx.random() - 0.5;
                let y = ctx.random() - 0.5;
                let z = ctx.random() - 0.5;
                if box3d_sd([x, y, z], [d[0], d[1], d[2]]) < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: None,
        wgsl: "fn box3D_sd(p: vec3<f32>, b: vec3<f32>) -> f32 {
           let q = abs(p) - b;
           return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
         }
         @body
         var found = false;
         for (var i = 0; i < 50; i++) {
           let x = rnd() - 0.5;
           let y = rnd() - 0.5;
           let z = rnd() - 0.5;
           if (box3D_sd(vec3<f32>(x, y, z), vec3<f32>(D(0), D(1), D(2))) < 0.0) {
             v.x += AMOUNT * x;
             v.y += AMOUNT * y;
             v.z += AMOUNT * z;
             v.hide = 0.0;
             found = true;
             break;
           }
         }
         if (!found) { v.hide = 1.0; }",
    },
    VarDef {
        name: "boxfold",
        params: &[
            ParamDef::range("foldLimit", 1.0, 0.05, 5.0),
            ParamDef::range("rotateX", 0.0, -180.0, 180.0),
            ParamDef::range("rotateY", 0.0, -180.0, 180.0),
            ParamDef::range("rotateZ", 0.0, -180.0, 180.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| {
            let f = d[0];
            let local = boxfold_rotate(d, [a.x, a.y, a.z], -1.0);
            let folded = [boxfold_fold(local[0], f), boxfold_fold(local[1], f), boxfold_fold(local[2], f)];
            let w = boxfold_rotate(d, folded, 1.0);
            v.x += w[0] * amount;
            v.y += w[1] * amount;
            v.z += w[2] * amount;
        },
        prepare: Some(|p, _| {
            let f = if p[0].is_nan() { 1e-9 } else { p[0].clamp(1e-9, 10000.0) };
            let mut d = vec![f, p[1], p[2], p[3]];
            for &rot in &p[1..4] {
                let ang = rot.to_radians();
                d.push(ang.sin());
                d.push(ang.cos());
            }
            d
        }),
        wgsl: "fn boxfold_rot(p: vec3<f32>, sg: f32, rx: f32, ry: f32, rz: f32, sx: f32, cx: f32, sy: f32, cy: f32, sz: f32, cz: f32) -> vec3<f32> {
           var q = p;
           if (rz != 0.0) { let s = sg * sz; q = vec3<f32>(q.x * cz - q.y * s, q.x * s + q.y * cz, q.z); }
           if (ry != 0.0) { let s = sg * sy; q = vec3<f32>(q.x * cy + q.z * s, q.y, -q.x * s + q.z * cy); }
           if (rx != 0.0) { let s = sg * sx; q = vec3<f32>(q.x, q.y * cx - q.z * s, q.y * s + q.z * cx); }
           return q;
         }
         fn boxfold_fold(x: f32, f: f32) -> f32 {
           if (x > f) { return 2.0 * f - x; }
           if (x < -f) { return -2.0 * f - x; }
           return x;
         }
         @body
         let f = D(0);
         let l = boxfold_rot(vec3<f32>(a.x, a.y, a.z), -1.0, D(1), D(2), D(3), D(4), D(5), D(6), D(7), D(8), D(9));
         let fl = vec3<f32>(boxfold_fold(l.x, f), boxfold_fold(l.y, f), boxfold_fold(l.z, f));
         let w = boxfold_rot(fl, 1.0, D(1), D(2), D(3), D(4), D(5), D(6), D(7), D(8), D(9));
         v.x += w.x * AMOUNT;
         v.y += w.y * AMOUNT;
         v.z += w.z * AMOUNT;",
    },
    VarDef {
        name: "bravais",
        params: &[ParamDef::new("scale", 3.0), ParamDef::new("pull", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let cx = java_round(a.x * d[0]) / d[0];
            let cy = java_round(a.y * d[0]) / d[0];
            v.x += amount * (a.x + d[1] * (cx - a.x));
            v.y += amount * (a.y + d[1] * (cy - a.y));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let cx = java_round(a.x * D(0)) / D(0);
         let cy = java_round(a.y * D(0)) / D(0);
         v.x += AMOUNT * (a.x + D(1) * (cx - a.x));
         v.y += AMOUNT * (a.y + D(1) * (cy - a.y));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "brick",
        params: &[ParamDef::new("scale_x", 1.0), ParamDef::new("scale_y", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sx = java_max(d[0].abs(), 1e-4);
            let sy = java_max(d[1].abs(), 1e-4);
            let row = (a.y / sy).floor();
            let offset = if ((row % 2.0) + 2.0) % 2.0 > 0.5 { sx * 0.5 } else { 0.0 };
            let nx = a.x - (((a.x + offset) / sx).floor() * sx + sx * 0.5 - offset);
            let ny = a.y - ((a.y / sy).floor() * sy + sy * 0.5);
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sx = max(abs(D(0)), 1e-4);
         let sy = max(abs(D(1)), 1e-4);
         let row = floor(a.y / sy);
         var offset = 0.0;
         if (fmod(fmod(row, 2.0) + 2.0, 2.0) > 0.5) { offset = sx * 0.5; }
         let nx = a.x - (floor((a.x + offset) / sx) * sx + sx * 0.5 - offset);
         let ny = a.y - (floor(a.y / sy) * sy + sy * 0.5);
         v.x += AMOUNT * nx;
         v.y += AMOUNT * ny;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "brownian_js",
        params: &[
            ParamDef::int("level", 10.0, 1.0, 15.0),
            ParamDef::range("variation", 3.0, 1.0, 100.0),
            // Java default is random (0..999); 500 is used here.
            ParamDef::int("seed", 500.0, 0.0, 1000000.0),
            ParamDef::range("line_thickness", 0.5, 0.0, 3.0),
            ParamDef::range("show_lines", 1.0, 0.0, 1.0),
            ParamDef::range("show_points", 0.0, 0.0, 1.0),
            ParamDef::range("point_thickness", 3.0, 0.0, 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // The Java canvas cycles through the segments in order; here a random segment is picked.
        apply: |d, ctx, a, v, amount| {
            let nseg = d[11] as usize;
            let k = BROWNIAN_PATH + 2 * ctx.random_int(nseg);
            let (x1, y1, x2, y2) = (d[k], d[k + 1], d[k + 2], d[k + 3]);
            let rnd = ctx.random();
            let (mut ox, mut oy) = (0.0, 0.0);
            if rnd < d[9] {
                // pLine
                let ydiff = y2 - y1;
                let xdiff = x2 - x1;
                let m = if xdiff == 0.0 { 10000.0 } else { ydiff / xdiff };
                let line_length = (xdiff * xdiff + ydiff * ydiff).sqrt();
                let dist = ctx.random() * line_length;
                let mut xoffset = dist / (1.0 + m * m).sqrt();
                if x2 < x1 {
                    xoffset = -xoffset;
                }
                let mut yoffset = (m * xoffset).abs();
                if y2 < y1 {
                    yoffset = -yoffset;
                }
                let lt = d[7];
                if lt != 0.0 {
                    xoffset += (ctx.random() - 0.5) * lt;
                    yoffset += (ctx.random() - 0.5) * lt;
                }
                ox = x1 + xoffset;
                oy = y1 + yoffset;
            } else if rnd <= d[10] {
                // pDot
                let pt = d[8];
                let (mut xoffset, mut yoffset) = (0.0, 0.0);
                if pt != 0.0 {
                    let roffset = ctx.random() * pt;
                    let rangle = ctx.random() * M_2PI;
                    xoffset = roffset * rangle.cos();
                    yoffset = roffset * rangle.sin();
                }
                ox = x1 + xoffset;
                oy = y1 + yoffset;
            }
            v.x += amount * ox;
            v.y += amount * oy;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| brownian_prepare(p)),
        wgsl: "let nseg = u32(D(11));
         let k = base + 12u + 2u * min(u32(rnd() * f32(nseg)), nseg - 1u);
         let x1 = prm[k];
         let y1 = prm[k + 1u];
         let x2 = prm[k + 2u];
         let y2 = prm[k + 3u];
         let r0 = rnd();
         var ox = 0.0;
         var oy = 0.0;
         if (r0 < D(9)) {
           let ydiff = y2 - y1;
           let xdiff = x2 - x1;
           var m = 10000.0;
           if (xdiff != 0.0) { m = ydiff / xdiff; }
           let dist = rnd() * sqrt(xdiff * xdiff + ydiff * ydiff);
           var xoffset = dist / sqrt(1.0 + m * m);
           if (x2 < x1) { xoffset = -xoffset; }
           var yoffset = abs(m * xoffset);
           if (y2 < y1) { yoffset = -yoffset; }
           if (D(7) != 0.0) {
             xoffset += (rnd() - 0.5) * D(7);
             yoffset += (rnd() - 0.5) * D(7);
           }
           ox = x1 + xoffset;
           oy = y1 + yoffset;
         } else if (r0 <= D(10)) {
           var xoffset = 0.0;
           var yoffset = 0.0;
           if (D(8) != 0.0) {
             let roffset = rnd() * D(8);
             let rangle = rnd() * TWO_PI;
             xoffset = roffset * cos(rangle);
             yoffset = roffset * sin(rangle);
           }
           ox = x1 + xoffset;
           oy = y1 + yoffset;
         }
         v.x += AMOUNT * ox;
         v.y += AMOUNT * oy;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bsplit",
        params: &[ParamDef::new("x", 0.0), ParamDef::new("y", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            if a.x + d[0] == 0.0 || a.x + d[0] == M_PI {
                v.do_hide = true;
            } else {
                v.do_hide = false;
                v.x += amount / (a.x + d[0]).tan() * (a.y + d[1]).cos();
                v.y += amount / (a.x + d[0]).sin() * (-a.y + d[1]);
                ctx.pass_z(a, v, amount);
            }
        },
        prepare: None,
        wgsl: "let sx = a.x + D(0);
         if (sx == 0.0 || sx == PI) {
           v.hide = 1.0;
         } else {
           v.hide = 0.0;
           v.x += AMOUNT / tan(sx) * cos(a.y + D(1));
           v.y += AMOUNT / sin(sx) * (-a.y + D(1));
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    },
    VarDef {
        name: "bSwirl",
        params: &[ParamDef::new("in", 0.0), ParamDef::new("out", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let tau = 0.5 * ((sqr(a.x + 1.0) + sqr(a.y)).ln() - (sqr(a.x - 1.0) + sqr(a.y)).ln());
            let mut sigma = M_PI - a.y.atan2(a.x + 1.0) - a.y.atan2(1.0 - a.x);
            sigma = sigma + tau * d[1] + d[0] / tau;
            let temp = tau.cosh() - sigma.cos();
            if temp == 0.0 {
                return;
            }
            v.x += amount * tau.sinh() / temp;
            v.y += amount * sigma.sin() / temp;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let tau = 0.5 * (log(sqr(a.x + 1.0) + sqr(a.y)) - log(sqr(a.x - 1.0) + sqr(a.y)));
         let sigma = PI - atan2(a.y, a.x + 1.0) - atan2(a.y, 1.0 - a.x) + tau * D(1) + D(0) / tau;
         let temp = cosh(tau) - cos(sigma);
         if (temp != 0.0) {
           v.x += AMOUNT * sinh(tau) / temp;
           v.y += AMOUNT * sin(sigma) / temp;
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    },
    VarDef {
        name: "bTransform",
        params: &[
            ParamDef::new("rotate", 0.0),
            ParamDef::int("power", 1.0, 1.0, 10.0),
            ParamDef::new("move", 0.0),
            ParamDef::new("split", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let power = d[1];
            let mut tau =
                0.5 * ((sqr(a.x + 1.0) + sqr(a.y)).ln() - (sqr(a.x - 1.0) + sqr(a.y)).ln()) / power + d[2];
            let mut sigma = M_PI - a.y.atan2(a.x + 1.0) - a.y.atan2(1.0 - a.x) + d[0];
            sigma = sigma / power + M_2PI / power * (ctx.random() * power).floor();
            if a.x >= 0.0 {
                tau += d[3];
            } else {
                tau -= d[3];
            }
            let temp = tau.cosh() - sigma.cos();
            v.x += amount * tau.sinh() / temp;
            v.y += amount * sigma.sin() / temp;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], limit_value(ftoi(p[1]), 1, i32::MAX) as f64, p[2], p[3]]),
        wgsl: "let power = D(1);
         var tau = 0.5 * (log(sqr(a.x + 1.0) + sqr(a.y)) - log(sqr(a.x - 1.0) + sqr(a.y))) / power + D(2);
         var sigma = PI - atan2(a.y, a.x + 1.0) - atan2(a.y, 1.0 - a.x) + D(0);
         sigma = sigma / power + TWO_PI / power * floor(rnd() * power);
         if (a.x >= 0.0) { tau += D(3); } else { tau -= D(3); }
         let temp = cosh(tau) - cos(sigma);
         v.x += AMOUNT * sinh(tau) / temp;
         v.y += AMOUNT * sin(sigma) / temp;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bubble_col",
        params: &[ParamDef::new("freq", 4.0), ParamDef::new("amp", 0.3), ParamDef::new("rise", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let freq = java_max(d[0].abs(), 0.01);
            let col = java_round(a.x * freq) / freq;
            let phase = col * 2.39996323;
            let sway = d[1] * (freq * a.y + phase).sin();
            v.x += amount * (col + sway);
            v.y += amount * (a.y + d[2] / (freq + 1e-4));
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let freq = max(abs(D(0)), 0.01);
         let col = java_round(a.x * freq) / freq;
         let sway = D(1) * sin(freq * a.y + col * 2.39996323);
         v.x += AMOUNT * (col + sway);
         v.y += AMOUNT * (a.y + D(2) / (freq + 1e-4));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bubbleT3D",
        params: &[
            ParamDef::int("number_of_stripes", 0.0, 0.0, 10.0),
            ParamDef::range("ratio_of_stripes", 1.0, 0.0, 2.0),
            ParamDef::range("angle_of_hole", 0.0, -360.0, 360.0),
            ParamDef::range("exponent_z", 1.0, 0.0, 4.0),
            ParamDef::int("symmetry_z", 0.0, 0.0, 1.0),
            ParamDef::int("modus_blur", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _, a, v, amount| bubblet3d_apply(d, a, v, amount),
        prepare: Some(|p, _| bubblet3d_prepare(p)),
        wgsl: "fn bubbleT3D_bend(p: vec3<f32>, ang_tmp: f32, ang_z: f32) -> vec3<f32> {
           let az = ang_z - HALF_PI;
           let fac = cos(ang_tmp) / cos(az);
           return vec3<f32>(p.x * fac, p.y * fac, p.z * (sin(ang_tmp) / sin(az)));
         }
         @body
         var x = a.x;
         var y = a.y;
         var z = 0.0;
         let rad = (sqr(x) + sqr(y)) / 4.0 + 1.0;
         let blur = i32(D(5));
         let ez = D(3);
         var ang_xy = atan2(x, y);
         if (ang_xy < 0.0) { ang_xy += TWO_PI; }
         if (D(0) != 0.0) {
           let s1 = D(6);
           let s2 = D(7);
           for (var i = 0; i < 10000 && ang_xy > s2; i++) { ang_xy -= s2; }
           if (ang_xy > s1) {
             if (blur == 0) {
               x = 0.0;
               y = 0.0;
             } else {
               var s = D(8);
               var c = D(9);
               if (D(1) != 1.0) {
                 let ang_rot = ang_xy - (ang_xy - s1) / (s2 - s1) * s1;
                 s = sin(ang_rot);
                 c = cos(ang_rot);
               }
               let xt = c * x - s * y;
               let yt = s * x + c * y;
               x = xt;
               y = yt;
             }
           }
         }
         x = x / rad;
         y = y / rad;
         var ang_z = 0.0;
         if (x != 0.0 || y != 0.0) {
           z = 2.0 / pow(rad, ez) - 1.0;
           if (ez <= 2.0) {
             ang_z = PI - acos(z / (x * x + y * y + z * z));
           } else {
             ang_z = PI - atan2(sqr(x * x + y * y), z);
           }
         }
         let hole = D(2);
         let comp = D(11);
         let cut = blur == 0 || ez != 1.0;
         var p = vec3<f32>(x, y, z);
         if (i32(D(4)) == 0) {
           if (D(10) == 0.0) {
             if (ang_z > hole) {
               if (cut) { p = vec3<f32>(0.0); } else { p = bubbleT3D_bend(p, (PI - ang_z) / comp * hole - HALF_PI, ang_z); }
             }
           } else if (ang_z < hole) {
             if (cut) { p = vec3<f32>(0.0); } else { p = bubbleT3D_bend(p, PI - ang_z / comp * hole - HALF_PI, ang_z); }
           }
         } else if (ang_z > hole || ang_z < (PI - hole)) {
           if (cut) {
             p = vec3<f32>(0.0);
           } else {
             var ang_tmp = HALF_PI - (ang_z / comp * (PI - 2.0 * comp) + comp);
             if (ang_z > hole) { ang_tmp = (PI - ang_z) / comp * (PI - 2.0 * comp) + comp - HALF_PI; }
             p = bubbleT3D_bend(p, ang_tmp, ang_z);
           }
         }
         v.x += AMOUNT * p.x;
         v.y += AMOUNT * p.y;
         v.z += AMOUNT * p.z;",
    },
    simple(
        "bubble_wf",
        true,
        |_, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y) / 4.0 + 1.0;
            let t = amount / r;
            v.x += t * a.x;
            v.y += t * a.y;
            if ctx.random() < 0.5 {
                v.z -= amount * (2.0 / r - 1.0);
            } else {
                v.z += amount * (2.0 / r - 1.0);
            }
        },
        "let r = (a.x * a.x + a.y * a.y) / 4.0 + 1.0;
         let t = AMOUNT / r;
         v.x += t * a.x;
         v.y += t * a.y;
         if (rnd() < 0.5) { v.z -= AMOUNT * (2.0 / r - 1.0); } else { v.z += AMOUNT * (2.0 / r - 1.0); }",
    ),
    VarDef {
        name: "bulge",
        params: &[ParamDef::new("N", 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let rn = r.powf(d[0]);
            v.x += amount * (rn * a.x / r);
            v.y += amount * (rn * a.y / r);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = length(vec2<f32>(a.x, a.y));
         let rn = pow(r, D(0));
         v.x += AMOUNT * (rn * a.x / r);
         v.y += AMOUNT * (rn * a.y / r);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "abs_fold",
        "acosech",
        "acosh",
        "acoth",
        "affine3D",
        "afterimage",
        "aftershock",
        "amoeba",
        "anamorphcyl",
        "apocarpet_js",
        "apollony",
        "arcsech2",
        "arcsinh",
        "arctanh",
        "arctruchet",
        "asteria",
        "astroid",
        "atan2_spirals",
        "atan",
        "attractor_flow",
        "aurora",
        "aurora_curtain",
        "azimuthal",
        "bbox3D",
        "bCollide",
        "bedhead",
        "bi_linear",
        "bipolar2",
        "blade3D",
        "blob3D",
        "blocky",
        "blur_linear",
        "blur_pixelize",
        "blur_zoom",
        "bMod",
        "boarders2",
        "box3D",
        "boxfold",
        "bravais",
        "brick",
        "brownian_js",
        "bsplit",
        "bSwirl",
        "bTransform",
        "bubble_col",
        "bubbleT3D",
        "bubble_wf",
        "bulge",
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
    }

    #[test]
    fn affine3d_identity() {
        let d = def("affine3D").prepare_data(&[], &Affine::IDENTITY);
        let mut ctx = Ctx::new(1, false);
        let (_, v) = run(def("affine3D"), &d, &mut ctx, (0.3, -0.4, 0.7));
        assert!((v.x - (0.1 + 0.8 * 0.3)).abs() < 1e-12);
        assert!((v.y - (-0.2 - 0.8 * 0.4)).abs() < 1e-12);
        assert!((v.z - (0.05 + 0.8 * 0.7)).abs() < 1e-12);
    }

    #[test]
    fn brownian_path_size_and_extent() {
        let d = def("brownian_js").prepare_data(&[4.0], &Affine::IDENTITY);
        assert_eq!(d[11] as usize, 16);
        assert_eq!(d.len(), BROWNIAN_PATH + 2 * 17);
        // the path starts and ends at the origin
        assert_eq!((d[12], d[13]), (0.0, 0.0));
        assert_eq!((d[d.len() - 2], d[d.len() - 1]), (0.0, 0.0));
    }

    #[test]
    fn arctruchet_tilts_in_range() {
        let d = def("arctruchet").prepare_data(&[], &Affine::IDENTITY);
        assert_eq!(d.len(), 4 + 100 + 1);
        assert!(d[4..104].iter().all(|t| (0.0..4.0).contains(t) && t.fract() == 0.0));
    }

    #[test]
    fn box3d_hides_nothing_with_defaults() {
        let mut ctx = Ctx::new(5, false);
        let d = def("box3D").prepare_data(&[], &Affine::IDENTITY);
        for _ in 0..100 {
            let (_, v) = run(def("box3D"), &d, &mut ctx, (0.0, 0.0, 0.0));
            assert!(!v.do_hide);
        }
    }

    #[test]
    fn attractor_flow_preset_overrides() {
        let d = def("attractor_flow").prepare_data(&[2.0], &Affine::IDENTITY);
        assert_eq!(&d[1..9], &[1.0, 1.0, 0.0, -2.0, -2.0, 1.5, 1.2, -1.9]);
    }
}
