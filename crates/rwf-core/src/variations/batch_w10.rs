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

//! Variations ported from the JWildfire sources (batch w10).
//!
//! Notes on stateful variations:
//! * `fract_*_wf` (buddhabrot mode): Java follows one orbit across calls. Here every call picks
//!   an orbit (accepted with a probability proportional to its number of output points, so long
//!   orbits keep their weight) and outputs one uniformly chosen point of it.
//! * `gingerbread_man`: Java follows one endless orbit starting at (-0.1, 0); the CPU does the
//!   same per render thread. On the GPU every call iterates a slightly perturbed start point
//!   128..639 times, which samples the same chaotic region of the gingerbread-man map.

use super::complex::Complex;
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
use crate::math::*;
use crate::model::Affine;

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

/// `Tools.FTOI` as `f64` (integer parameters arrive as `f64`).
#[inline]
fn ftoi_f(x: f64) -> f64 {
    ftoi(x) as f64
}

/// `limitIntVal(Tools.FTOI(x), lo, hi)` as `f64`.
#[inline]
fn ftoi_limit(x: f64, lo: i32, hi: i32) -> f64 {
    limit_value(ftoi(x), lo, hi) as f64
}

// ---------------------------------------------------------------------------------------------
// falloff3 helpers (AbstractFalloff3Func)
// ---------------------------------------------------------------------------------------------

#[inline]
fn falloff3_sgnd(x: f64) -> f64 {
    if x < 0.0 { -1.0 } else { 1.0 }
}

#[inline]
fn falloff3_log_scale(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { ((x.abs() + 1.0) * M_E).ln() * falloff3_sgnd(x) / M_E }
}

#[inline]
fn falloff3_log_map(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { (M_E + (x * M_E).ln()) / 4.0 * falloff3_sgnd(x) }
}

// ---------------------------------------------------------------------------------------------
// gdoffs helpers
// ---------------------------------------------------------------------------------------------

#[inline]
fn gdoffs_fclp(a: f64) -> f64 {
    if a < 0.0 { -fmod(a.abs(), 1.0) } else { fmod(a.abs(), 1.0) }
}

#[inline]
fn gdoffs_fscl(a: f64) -> f64 {
    gdoffs_fclp((a + 1.0) / 2.0)
}

#[inline]
fn gdoffs_fosc(p: f64, a: f64) -> f64 {
    gdoffs_fscl(-(p * a * M_2PI).cos())
}

#[inline]
fn gdoffs_flip(a: f64, b: f64, c: f64) -> f64 {
    c * (b - a) + a
}

// ---------------------------------------------------------------------------------------------
// glitchy1 helpers
// ---------------------------------------------------------------------------------------------

/// Bob Jenkins' integer hash (Java `int` arithmetic) scaled by `Integer.MAX_VALUE`.
fn glitchy1_hash(a: i32) -> f64 {
    let mut a = (a ^ 61) ^ (a >> 16);
    a = a.wrapping_add(a.wrapping_shl(3));
    a ^= a >> 4;
    a = a.wrapping_mul(0x27d4eb2d);
    a ^= a >> 15;
    a as f64 / i32::MAX as f64
}

/// Mirrors every `n`-th band of width `1/scale` (shifted by `shift`).
fn glitchy1_fold(c: f64, shift: f64, scale: f64, n: f64) -> f64 {
    if scale == 0.0 {
        return c;
    }
    let nr = (((c - shift) * scale).floor() as i32) as f64;
    let hit = if nr >= 0.0 { nr % n == n - 1.0 } else { nr % n == 0.0 };
    if hit { 2.0 * shift - c } else { c }
}

/// `int b = (int) floor(t * width); b += (2.0 - 4.0 * hash(b * seed + 1));`
fn glitchy1_block(t: f64, width: f64, seed: i32) -> i32 {
    let b = (t * width).floor() as i32;
    (b as f64 + (2.0 - 4.0 * glitchy1_hash(b.wrapping_mul(seed).wrapping_add(1)))) as i32
}

// ---------------------------------------------------------------------------------------------
// geometricPrimitives helpers
// ---------------------------------------------------------------------------------------------

#[inline]
fn gp_random_for_cell(gx: f64, gy: f64, salt: f64) -> f64 {
    frac(((gx * 13.9898 + gy * 73.233 + salt).sin()) * 43758.5453)
}

fn gp_circle(ctx: &mut Ctx, filled: bool, cx: f64, cy: f64, rad: f64, hol: f64) -> (f64, f64) {
    let angle = ctx.random() * 2.0 * M_PI;
    let r = if filled {
        ctx.random().sqrt() * rad
    } else {
        let inner = rad * hol;
        inner + ctx.random() * (rad - inner)
    };
    (cx + r * angle.cos(), cy + r * angle.sin())
}

fn gp_square(ctx: &mut Ctx, filled: bool, cx: f64, cy: f64, size: f64) -> (f64, f64) {
    let hw = size * 0.5;
    if filled {
        let x = cx + (ctx.random() * 2.0 - 1.0) * hw;
        let y = cy + (ctx.random() * 2.0 - 1.0) * hw;
        (x, y)
    } else {
        let side = (ctx.random() * 4.0).floor();
        let t = (ctx.random() * 2.0 - 1.0) * hw;
        if side == 0.0 {
            (cx + t, cy + hw)
        } else if side == 1.0 {
            (cx + t, cy - hw)
        } else if side == 2.0 {
            (cx + hw, cy + t)
        } else {
            (cx - hw, cy + t)
        }
    }
}

/// Random point in the triangle `o, p1, p2` (barycentric, as in Java).
fn gp_tri_fill(ctx: &mut Ctx, o: (f64, f64), p1: (f64, f64), p2: (f64, f64)) -> (f64, f64) {
    let mut r1 = ctx.random();
    let mut r2 = ctx.random();
    if r1 + r2 > 1.0 {
        r1 = 1.0 - r1;
        r2 = 1.0 - r2;
    }
    (
        (1.0 - r1 - r2) * o.0 + r1 * p1.0 + r2 * p2.0,
        (1.0 - r1 - r2) * o.1 + r1 * p1.1 + r2 * p2.1,
    )
}

#[inline]
fn gp_lerp(p: (f64, f64), q: (f64, f64), t: f64) -> (f64, f64) {
    (p.0 * (1.0 - t) + q.0 * t, p.1 * (1.0 - t) + q.1 * t)
}

fn gp_triangle(ctx: &mut Ctx, filled: bool, cx: f64, cy: f64, size: f64) -> (f64, f64) {
    let v1 = (cx, cy + size);
    let v2 = (cx - size * 0.866, cy - size * 0.5);
    let v3 = (cx + size * 0.866, cy - size * 0.5);
    if filled {
        gp_tri_fill(ctx, v1, v2, v3)
    } else {
        let side = (ctx.random() * 3.0).floor();
        let t = ctx.random();
        if side == 0.0 {
            gp_lerp(v1, v2, t)
        } else if side == 1.0 {
            gp_lerp(v2, v3, t)
        } else {
            gp_lerp(v3, v1, t)
        }
    }
}

fn gp_hexagon(ctx: &mut Ctx, filled: bool, cx: f64, cy: f64, size: f64) -> (f64, f64) {
    let seg_angle = M_PI / 3.0;
    if filled {
        let segment = (ctx.random() * 6.0).floor();
        let (a1, a2) = (segment * seg_angle, (segment + 1.0) * seg_angle);
        let v1 = (cx + size * a1.cos(), cy + size * a1.sin());
        let v2 = (cx + size * a2.cos(), cy + size * a2.sin());
        gp_tri_fill(ctx, (cx, cy), v1, v2)
    } else {
        let angle = ctx.random() * 2.0 * M_PI;
        let segment = (angle / seg_angle).floor();
        let (a1, a2) = (segment * seg_angle, (segment + 1.0) * seg_angle);
        let v1 = (cx + size * a1.cos(), cy + size * a1.sin());
        let v2 = (cx + size * a2.cos(), cy + size * a2.sin());
        let t = ctx.random();
        gp_lerp(v1, v2, t)
    }
}

/// Grid-cell jitter `(jx, jy)` of `GeometricPrimitivesFunc`.
fn gp_jitter(d: &[f64], gx: f64, gy: f64) -> (f64, f64) {
    if d[5] > 0.0 {
        let max_jitter = d[5] * d[3] * 0.5;
        (
            (gp_random_for_cell(gx, gy, d[4] + 40.0) * 2.0 - 1.0) * max_jitter,
            (gp_random_for_cell(gx, gy, d[4] + 50.0) * 2.0 - 1.0) * max_jitter,
        )
    } else {
        (0.0, 0.0)
    }
}

// d = [line_vs_shape_mix, density, scale, gridSize, seed, grid_jitter, shape_type,
//      shape_mix_enable, fill_prob, radius, hollow, square_size, connection_mode,
//      recursion_depth, recursion_shape_mix, recursion_scale]
fn geometric_primitives_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let exists = |gx: f64, gy: f64| gp_random_for_cell(gx, gy, d[4]) < d[1];
    let u = a.x * d[2];
    let w = a.y * d[2];
    let grid_x = java_round(u);
    let grid_y = java_round(w);
    let (jx, jy) = gp_jitter(d, grid_x, grid_y);
    let center_x = grid_x * d[3] + jx;
    let center_y = grid_y * d[3] + jy;
    let mut out: Option<(f64, f64)> = None;
    if ctx.random() < d[0] {
        if exists(grid_x, grid_y) {
            let (mut ngx, mut ngy) = (grid_x, grid_y);
            if d[12] as i32 == 0 {
                let dir = ctx.random() * 4.0;
                if dir < 1.0 {
                    ngx += 1.0;
                } else if dir < 2.0 {
                    ngx -= 1.0;
                } else if dir < 3.0 {
                    ngy += 1.0;
                } else {
                    ngy -= 1.0;
                }
            } else {
                let mut valid: [(f64, f64); 4] = [(0.0, 0.0); 4];
                let mut nvalid = 0;
                for cand in [
                    (grid_x + 1.0, grid_y),
                    (grid_x - 1.0, grid_y),
                    (grid_x, grid_y + 1.0),
                    (grid_x, grid_y - 1.0),
                ] {
                    if exists(cand.0, cand.1) {
                        valid[nvalid] = cand;
                        nvalid += 1;
                    }
                }
                if nvalid > 0 {
                    let chosen = valid[(ctx.random() * nvalid as f64).floor() as usize];
                    ngx = chosen.0;
                    ngy = chosen.1;
                }
            }
            if ngx != grid_x || ngy != grid_y {
                let (njx, njy) = gp_jitter(d, ngx, ngy);
                let ncx = ngx * d[3] + njx;
                let ncy = ngy * d[3] + njy;
                let depth = d[13] as i32;
                if depth > 0 {
                    let step = (ctx.random() * depth as f64).floor() as i32;
                    let t = (step as f64 + 0.5) / depth as f64;
                    let scx = center_x * (1.0 - t) + ncx * t;
                    let scy = center_y * (1.0 - t) + ncy * t;
                    let filled = gp_random_for_cell(grid_x, grid_y, d[4] + 30.0 + step as f64) < d[8];
                    out = Some(if ctx.random() < d[14] {
                        gp_square(ctx, filled, scx, scy, d[11] * d[15])
                    } else {
                        gp_circle(ctx, filled, scx, scy, d[9] * d[15], d[10])
                    });
                } else {
                    let t = ctx.random();
                    out = Some((center_x * (1.0 - t) + ncx * t, center_y * (1.0 - t) + ncy * t));
                }
            }
        }
    } else if exists(grid_x, grid_y) {
        let mut shape = d[6] as i32;
        if d[7] as i32 == 1 {
            shape = (gp_random_for_cell(grid_x, grid_y, d[4] + 10.0) * 4.0).floor() as i32;
        }
        let filled = gp_random_for_cell(grid_x, grid_y, d[4] + 20.0) < d[8];
        out = Some(match shape {
            0 => {
                let rad = gp_random_for_cell(grid_x, grid_y, -d[4]) * d[9] * d[3];
                gp_circle(ctx, filled, center_x, center_y, rad, d[10])
            }
            1 => {
                let size = gp_random_for_cell(grid_x, grid_y, 1.0 - d[4]) * d[11] * d[3];
                gp_square(ctx, filled, center_x, center_y, size)
            }
            2 => {
                let size = gp_random_for_cell(grid_x, grid_y, 2.0 - d[4]) * d[9] * d[3];
                gp_triangle(ctx, filled, center_x, center_y, size)
            }
            3 => {
                let size = gp_random_for_cell(grid_x, grid_y, 3.0 - d[4]) * d[9] * d[3];
                gp_hexagon(ctx, filled, center_x, center_y, size)
            }
            // negative hash values select no shape: Java keeps the zero point
            _ => (0.0, 0.0),
        });
    }
    match out {
        Some((ox, oy)) => {
            v.do_hide = false;
            v.x += ox * amount;
            v.y += oy * amount;
            ctx.pass_z(a, v, amount);
        }
        None => v.do_hide = true,
    }
}

// ---------------------------------------------------------------------------------------------
// fract_*_wf (AbstractFractWFFunc)
// ---------------------------------------------------------------------------------------------

// Prepared layout shared by all fract_*_wf variations:
// d = [max_iter, xmin, xmax, ymin, ymax, buddhabrot_mode, direct_color, scalez, clip_iter_min,
//      clip_iter_max, max_clip_iter, buddhabrot_min_iter, scale, offsetx, offsety, offsetz,
//      z_fill, z_logscale, color_only, kind, power, xseed, yseed]
const FW_KIND_DRAGON: f64 = 0.0;
const FW_KIND_JULIA: f64 = 1.0;
const FW_KIND_MANDELBROT: f64 = 2.0;
const FW_KIND_METEORS: f64 = 3.0;
const FW_KIND_PEARLS: f64 = 4.0;
const FW_KIND_SALAMANDER: f64 = 5.0;

/// Start points failing the buddhabrot test before the point is hidden (`bb_max_clip_iter`).
const FW_BB_MAX_CLIP_ITER: i32 = 250;
/// Upper bound of start points tried per call in buddhabrot mode (incl. weight rejections).
const FW_BB_MAX_TRIES: i32 = 4096;

/// Parameter list of `AbstractFractWFFunc` with the subclass defaults (`initParams`) and the
/// custom parameters inserted after `buddhabrot_mode`.
macro_rules! fract_wf_params {
    ($xmin:expr, $xmax:expr, $ymin:expr, $ymax:expr, $clip_min:expr, $scale:expr, $offx:expr; $($custom:expr),*) => {
        &[
            ParamDef::int("max_iter", 100.0, 1.0, 1000.0),
            ParamDef::new("xmin", $xmin),
            ParamDef::new("xmax", $xmax),
            ParamDef::new("ymin", $ymin),
            ParamDef::new("ymax", $ymax),
            ParamDef::int("buddhabrot_mode", 0.0, 0.0, 1.0),
            $($custom,)*
            ParamDef::int("direct_color", 1.0, 0.0, 1.0),
            ParamDef::new("scalez", 1.0),
            ParamDef::int("clip_iter_min", $clip_min, -10.0, 100.0),
            ParamDef::int("clip_iter_max", -5.0, -100.0, 10.0),
            ParamDef::int("max_clip_iter", 3.0, 0.0, 20.0),
            ParamDef::int("buddhabrot_min_iter", 7.0, 0.0, 100.0),
            ParamDef::new("scale", $scale),
            ParamDef::new("offsetx", $offx),
            ParamDef::new("offsety", 0.0),
            ParamDef::new("offsetz", 0.0),
            ParamDef::range("z_fill", 0.0, 0.0, 1.0),
            ParamDef::int("z_logscale", 0.0, 0.0, 1.0),
            ParamDef::int("color_only", 0.0, 0.0, 1.0),
        ]
    };
}

/// Normalises the parameters (`nc` custom ones after index 5) into the shared layout.
fn fract_wf_prepare(p: &[f64], nc: usize, kind: f64, power: f64, xseed: f64, yseed: f64) -> Vec<f64> {
    let t = &p[6 + nc..];
    vec![
        ftoi_f(p[0]),
        p[1],
        p[2],
        p[3],
        p[4],
        ftoi_f(p[5]),
        ftoi_f(t[0]),
        t[1],
        ftoi_f(t[2]),
        ftoi_f(t[3]),
        ftoi_f(t[4]),
        ftoi_f(t[5]),
        t[6],
        t[7],
        t[8],
        t[9],
        limit_value(t[10], 0.0, 1.0),
        ftoi_limit(t[11], 0, 1),
        ftoi_limit(t[12], 0, 1),
        kind,
        power,
        xseed,
        yseed,
    ]
}

/// `z^power + c` as written out in the Julia/Mandelbrot iterators.
fn fract_wf_poly(x1: f64, y1: f64, xs: f64, ys: f64, power: i32, cx: f64, cy: f64) -> (f64, f64) {
    match power {
        2 => (xs - ys + cx, 2.0 * x1 * y1 + cy),
        3 => (xs * x1 - 3.0 * x1 * ys + cx, 3.0 * xs * y1 - ys * y1 + cy),
        4 => (xs * xs + ys * ys - 6.0 * xs * ys + cx, 4.0 * x1 * y1 * (xs - ys) + cy),
        _ => {
            let mut x2 = x1 * (xs * xs - 10.0 * xs * ys + 5.0 * ys * ys);
            let mut y2 = y1 * (ys * ys - 10.0 * xs * ys + 5.0 * xs * xs);
            for _ in 5..power {
                let xa = x1 * x2 - y1 * y2;
                let ya = x1 * y2 + x2 * y1;
                x2 = xa;
                y2 = ya;
            }
            (x2 + cx, y2 + cy)
        }
    }
}

/// State of `AbstractFractWFFunc.Iterator` (`q` = `xs + ys` of the last step).
#[derive(Clone, Copy)]
struct FractWfIter {
    x: f64,
    y: f64,
    start_x: f64,
    start_y: f64,
    q: f64,
}

impl FractWfIter {
    fn new(x0: f64, y0: f64) -> Self {
        Self { x: x0, y: y0, start_x: x0, start_y: y0, q: 0.0 }
    }

    #[inline]
    fn bailout(&self) -> bool {
        self.q >= 4.0
    }

    fn next(&mut self, d: &[f64]) {
        let (x1, y1) = (self.x, self.y);
        let xs = x1 * x1;
        let ys = y1 * y1;
        self.q = xs + ys;
        let (xseed, yseed) = (d[21], d[22]);
        let (nx, ny) = match d[19] as i32 {
            0 => {
                let t1 = x1 - xs + ys;
                let t2 = y1 - 2.0 * x1 * y1;
                (t1 * xseed - t2 * yseed, t1 * yseed + t2 * xseed)
            }
            1 => fract_wf_poly(x1, y1, xs, ys, d[20] as i32, xseed, yseed),
            2 => fract_wf_poly(x1, y1, xs, ys, d[20] as i32, self.start_x, self.start_y),
            3 => {
                let (sx, sy) = (self.start_x, self.start_y);
                (
                    (sx * x1 - sy * y1) - (sx * x1 + sy * y1) / (xs + ys),
                    (sx * y1 + sy * x1) + (sx * y1 - sy * x1) / (xs + ys),
                )
            }
            4 => (
                xseed * x1 - yseed * y1 - ((xseed * x1 + yseed * y1) / (xs + ys)),
                xseed * y1 + yseed * x1 + ((xseed * y1 - yseed * x1) / (xs + ys)),
            ),
            _ => {
                let t1 = xs - ys;
                let t2 = 2.0 * x1 * y1;
                (t1 * xseed - t2 * yseed - 1.0, t1 * yseed + t2 * xseed)
            }
        };
        self.x = nx;
        self.y = ny;
    }

    /// `Iterator.iterate`: returns the Java iteration count and the final state.
    fn iterate(d: &[f64], x0: f64, y0: f64, max_iter: i32) -> (i32, Self) {
        let mut it = Self::new(x0, y0);
        let mut n = 0;
        loop {
            let more = n < max_iter;
            n += 1;
            if !more || it.bailout() {
                break;
            }
            it.next(d);
        }
        (n, it)
    }
}

fn fract_wf_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let max_iter = d[0] as i32;
    let sa = d[12] * amount;
    if d[5] as i32 > 0 {
        // Buddhabrot mode. Java emits the points currIter = bb_min+1 ..= maxIter of one orbit
        // over consecutive calls; an orbit is accepted with probability ~ its number of points.
        let bb_raw = d[11] as i32;
        let bb_min = bb_raw.max(0);
        let mut found: Option<(f64, f64, i32)> = None;
        let mut fails = 0;
        for _ in 0..FW_BB_MAX_TRIES {
            let x0 = (d[2] - d[1]) * ctx.random() + d[1];
            let y0 = (d[4] - d[3]) * ctx.random() + d[3];
            let (n, it) = FractWfIter::iterate(d, x0, y0, max_iter);
            if it.bailout() && n > bb_raw {
                let w = (n - bb_min) as f64 / (max_iter + 1 - bb_min).max(1) as f64;
                if ctx.random() < w {
                    found = Some((x0, y0, n));
                    break;
                }
            } else {
                fails += 1;
                if fails >= FW_BB_MAX_CLIP_ITER {
                    break;
                }
            }
        }
        let Some((x0, y0, len)) = found else {
            v.do_hide = true;
            return;
        };
        let k = bb_min + 1 + ctx.random_int((len - bb_min) as usize) as i32;
        let mut it = FractWfIter::new(x0, y0);
        for _ in 0..k {
            it.next(d);
        }
        v.x += sa * (it.x + d[13]);
        v.y += sa * (it.y + d[14]);
        v.color += k as f64 / len as f64;
        if v.color < 0.0 {
            v.color = 0.0;
        } else if v.color > 1.0 {
            v.color = 1.0;
        }
        return;
    }

    v.do_hide = false;
    let clip_min = d[8] as i32;
    let clip_max = d[9] as i32;
    let max_clip = d[10] as i32;
    let color_only = d[18] as i32 > 0;
    let (mut x0, mut y0, mut cnt) = (0.0, 0.0, 0);
    for i in 0..max_clip {
        if color_only {
            x0 = a.x;
            y0 = a.y;
        } else {
            x0 = (d[2] - d[1]) * ctx.random() + d[1];
            y0 = (d[4] - d[3]) * ctx.random() + d[3];
        }
        cnt = FractWfIter::iterate(d, x0, y0, max_iter).0;
        if (clip_max < 0 && cnt >= max_iter.saturating_add(clip_max)) || (clip_min > 0 && cnt <= clip_min) {
            if i == max_clip - 1 {
                v.do_hide = true;
                return;
            }
        } else {
            break;
        }
    }
    if !color_only {
        v.x += sa * (x0 + d[13]);
        v.y += sa * (y0 + d[14]);
    }
    let fc = cnt as f64 / max_iter as f64;
    let fp = (cnt - 1) as f64 / max_iter as f64;
    let (scalez, offz, z_fill) = (d[7], d[15], d[16]);
    let zf = |f: f64| {
        if d[17] as i32 == 1 {
            sa * (scalez / 10.0 * (1.0 + f).log10() + offz)
        } else {
            sa * (scalez / 10.0 * f + offz)
        }
    };
    let mut z = zf(fc);
    if z_fill > EPSILON && ctx.random() < z_fill {
        let prev_z = zf(fp);
        z = (prev_z - z) * ctx.random() + z;
    }
    v.z += z;
    if d[6] as i32 != 0 {
        v.color += fc;
        if v.color > 1.0 {
            v.color -= 1.0;
        }
        if v.color < 0.0 {
            v.color = 0.0;
        } else if v.color > 1.0 {
            v.color = 1.0;
        }
    }
}

/// WGSL for a fract_*_wf variation: `$step` computes the next orbit point from `z` (current
/// point, with `xs`/`ys` its squared components), `s` (start point) and `prm[base + 20..22]`.
macro_rules! fract_wf_wgsl {
    ($name:literal, $step:literal) => {
        concat!(
            "fn ", $name, "_step(z: vec2<f32>, s: vec2<f32>, base: u32) -> vec2<f32> {
  let xs = z.x * z.x;
  let ys = z.y * z.y;
", $step, "
}
// Iterator.iterate: (Java iteration count, bailout flag)
fn ", $name, "_iterate(start: vec2<f32>, max_iter: i32, base: u32) -> vec2<i32> {
  var z = start;
  var q = 0.0;
  var n = 0;
  loop {
    let more = n < max_iter;
    n += 1;
    if (!more || q >= 4.0) { break; }
    q = z.x * z.x + z.y * z.y;
    z = ", $name, "_step(z, start, base);
  }
  return vec2<i32>(n, select(0, 1, q >= 4.0));
}
fn ", $name, "_orbit(start: vec2<f32>, k: i32, base: u32) -> vec2<f32> {
  var z = start;
  for (var j = 0; j < k; j++) { z = ", $name, "_step(z, start, base); }
  return z;
}
@body
let max_iter = i32(D(0));
let sa = D(12) * AMOUNT;
var hide = false;
if (i32(D(5)) > 0) {
  let bb_raw = i32(D(11));
  let bb_min = max(bb_raw, 0);
  var x0 = 0.0;
  var y0 = 0.0;
  var len = 0;
  var fails = 0;
  for (var i = 0; i < 4096; i++) {
    x0 = (D(2) - D(1)) * rnd() + D(1);
    y0 = (D(4) - D(3)) * rnd() + D(3);
    let r = ", $name, "_iterate(vec2<f32>(x0, y0), max_iter, base);
    if (r.y == 1 && r.x > bb_raw) {
      if (rnd() * f32(max(max_iter + 1 - bb_min, 1)) < f32(r.x - bb_min)) {
        len = r.x;
        break;
      }
    } else {
      fails += 1;
      if (fails >= 250) { break; }
    }
  }
  if (len == 0) {
    hide = true;
  } else {
    let cnt = len - bb_min;
    let k = bb_min + 1 + min(i32(rnd() * f32(cnt)), cnt - 1);
    let zz = ", $name, "_orbit(vec2<f32>(x0, y0), k, base);
    v.x += sa * (zz.x + D(13));
    v.y += sa * (zz.y + D(14));
    v.color = clamp(v.color + f32(k) / f32(len), 0.0, 1.0);
  }
} else {
  v.hide = 0.0;
  let clip_min = i32(D(8));
  let clip_max = i32(D(9));
  let max_clip = i32(D(10));
  let color_only = i32(D(18)) > 0;
  var x0 = 0.0;
  var y0 = 0.0;
  var cnt = 0;
  for (var i = 0; i < max_clip; i++) {
    if (color_only) {
      x0 = a.x;
      y0 = a.y;
    } else {
      x0 = (D(2) - D(1)) * rnd() + D(1);
      y0 = (D(4) - D(3)) * rnd() + D(3);
    }
    cnt = ", $name, "_iterate(vec2<f32>(x0, y0), max_iter, base).x;
    if ((clip_max < 0 && cnt >= max_iter + clip_max) || (clip_min > 0 && cnt <= clip_min)) {
      if (i == max_clip - 1) { hide = true; }
    } else {
      break;
    }
  }
  if (!hide) {
    if (!color_only) {
      v.x += sa * (x0 + D(13));
      v.y += sa * (y0 + D(14));
    }
    let fc = f32(cnt) / f32(max_iter);
    let fp = f32(cnt - 1) / f32(max_iter);
    var fz = fc;
    var fpz = fp;
    if (i32(D(17)) == 1) {
      fz = log10(1.0 + fc);
      fpz = log10(1.0 + fp);
    }
    var z = sa * (D(7) / 10.0 * fz + D(15));
    if (D(16) > EPSILON && rnd() < D(16)) {
      let prev_z = sa * (D(7) / 10.0 * fpz + D(15));
      z = (prev_z - z) * rnd() + z;
    }
    v.z += z;
    if (i32(D(6)) != 0) {
      v.color += fc;
      if (v.color > 1.0) { v.color -= 1.0; }
      v.color = clamp(v.color, 0.0, 1.0);
    }
  }
}
if (hide) { v.hide = 1.0; }"
        )
    };
}

// ---------------------------------------------------------------------------------------------
// glitchy2
// ---------------------------------------------------------------------------------------------

macro_rules! glitchy2_params {
    ($(($q:literal, $spin:expr)),*) => {
        &[
            ParamDef::int("mode", 0.0, 0.0, 1.0),
            $(
                ParamDef::new(concat!($q, "_spin"), $spin),
                ParamDef::new(concat!($q, "_x"), 0.0),
                ParamDef::new(concat!($q, "_shiftx"), 0.0),
                ParamDef::new(concat!($q, "_y"), 0.0),
                ParamDef::new(concat!($q, "_shifty"), 0.0),
                ParamDef::new(concat!($q, "_re_a"), 0.0),
                ParamDef::new(concat!($q, "_re_b"), 0.0),
                ParamDef::new(concat!($q, "_re_c"), 0.0),
                ParamDef::new(concat!($q, "_re_d"), 1.0),
                ParamDef::new(concat!($q, "_im_a"), 0.0),
                ParamDef::new(concat!($q, "_im_b"), 0.0),
                ParamDef::new(concat!($q, "_im_c"), 0.0),
                ParamDef::new(concat!($q, "_im_d"), 0.0),
            )*
        ]
    };
}

/// Quadrant selectors of glitchy2 (lr, ur, ll, ul): sign of `x - shiftx` and `y - shifty`.
const GLITCHY2_SIGNS: [(f64, f64); 4] = [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)];

// d = [mode, 4 x (spin, x, shiftx, y, shifty, re_a, re_b, re_c, re_d, im_a, im_b, im_c, im_d)
//      for lr/ur/ll/ul, 4 x (pz_sin, pz_cos)]
fn glitchy2_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let mut count = 0;
    let mut x = 0.0;
    let mut y = 0.0;
    for (q, &(sx, sy)) in GLITCHY2_SIGNS.iter().enumerate() {
        let b = 1 + 13 * q;
        let in_x = if sx > 0.0 { a.x > d[b + 2] } else { a.x < d[b + 2] };
        let in_y = if sy > 0.0 { a.y > d[b + 4] } else { a.y < d[b + 4] };
        if !(in_x && in_y) {
            continue;
        }
        let (re_a, re_b, re_c, re_d) = (d[b + 5], d[b + 6], d[b + 7], d[b + 8]);
        let (im_a, im_b, im_c, im_d) = (d[b + 9], d[b + 10], d[b + 11], d[b + 12]);
        // Mobius, by eralex
        let re_u = re_a * a.x - im_a * a.y + re_b;
        let im_u = re_a * a.y + im_a * a.x + im_b;
        let re_v = re_c * a.x - im_c * a.y + re_d;
        let im_v = re_c * a.y + im_c * a.x + im_d;
        let dd = re_v * re_v + im_v * im_v;
        if dd == 0.0 {
            return;
        }
        let (ps, pc) = (d[53 + 2 * q], d[54 + 2 * q]);
        x += amount * (ps * a.y + pc * a.x) + d[b + 1] + (dd * (re_u * re_v + im_u * im_v));
        y += amount * (pc * a.y - ps * a.x) + d[b + 3] + (dd * (im_u * re_v - re_u * im_v));
        count += 1;
    }
    let cnt = count as f64;
    if d[0] as i32 != 0 {
        if count == 0 {
            v.x += amount / a.y;
            v.y += amount / a.x;
        } else {
            v.x += amount / y / cnt;
            v.y += amount / x / cnt;
        }
    } else if count == 0 {
        v.x += amount * a.x;
        v.y += amount * a.y;
    } else {
        v.x += amount * x / cnt;
        v.y += amount * y / cnt;
    }
    ctx.pass_z(a, v, amount);
}


// ---------------------------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "exblur",
        params: &[
            ParamDef::new("dist", 0.5),
            ParamDef::new("r", 0.25),
            ParamDef::new("x_origin", 0.0),
            ParamDef::new("y_origin", 0.0),
            ParamDef::new("z_origin", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [dist, r, x_origin, y_origin, z_origin]
            let ox = a.x - d[2];
            let oy = a.y + d[3];
            let oz = a.z - d[4];
            let n = ox * ox + oy * oy + oz * oz;
            let key = d.as_ptr() as usize;
            let rsum = ctx.gauss_ring_sum(key, 4) - 2.0;
            let rr = amount * n.powf(d[0]) * rsum;
            let theta = oy.atan2(ox);
            let phi = (oz / n.sqrt()).acos();
            let (su, cu) = theta.sin_cos();
            let (sv, cv) = phi.sin_cos();
            let (sru, cru) = (ctx.random() * M_2PI).sin_cos();
            let (srv, crv) = (ctx.random() * M_2PI).sin_cos();
            ctx.gauss_ring_push(key, 4, 3);
            let rsrv = d[1] * srv;
            v.x += rr * (sv * cu + rsrv * cru);
            v.y += rr * (sv * su + rsrv * sru);
            v.z += rr * (cv + d[1] * crv);
        },
        prepare: None,
        wgsl: "var<private> exblur_ring: array<f32, 4>;
var<private> exblur_idx: u32;
var<private> exblur_init: bool;
fn exblur_sum() -> f32 {
  if (!exblur_init) {
    exblur_init = true;
    for (var i = 0; i < 4; i++) { exblur_ring[i] = rnd(); }
  }
  return exblur_ring[0] + exblur_ring[1] + exblur_ring[2] + exblur_ring[3];
}
fn exblur_push() {
  exblur_ring[exblur_idx] = rnd();
  exblur_idx = (exblur_idx + 1u) & 3u;
}
@body
let ox = a.x - D(2);
let oy = a.y + D(3);
let oz = a.z - D(4);
let n = ox * ox + oy * oy + oz * oz;
let rsum = exblur_sum() - 2.0;
let rr = AMOUNT * java_pow(n, D(0)) * rsum;
let theta = atan2(oy, ox);
let phi = acos(clamp(oz / sqrt(n), -1.0, 1.0));
let tu = rnd() * TWO_PI;
let tv = rnd() * TWO_PI;
exblur_push();
let rsrv = D(1) * sin(tv);
v.x += rr * (sin(phi) * cos(theta) + rsrv * cos(tu));
v.y += rr * (sin(phi) * sin(theta) + rsrv * sin(tu));
v.z += rr * (cos(phi) + D(1) * cos(tv));",
    },
    VarDef {
        name: "exp2_bs",
        params: &[ParamDef::new("x1", 1.0), ParamDef::new("y1", 1.0), ParamDef::new("y2", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x1, y1, y2]
            let expe = (a.x * d[0]).exp();
            let expsin = (a.y * d[1]).sin();
            let expcos = (a.y * d[2]).cos();
            v.x += amount * expe * expcos;
            v.y += amount * expe * expsin;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let expe = exp(a.x * D(0));
v.x += AMOUNT * expe * cos(a.y * D(2));
v.y += AMOUNT * expe * sin(a.y * D(1));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "exp_multi",
        params: &[
            ParamDef::new("re", 1.0),
            ParamDef::new("im", 0.0),
            ParamDef::new("re_add", 0.0),
            ParamDef::new("im_add", 0.0),
            ParamDef::new("sqr", 0.0),
            ParamDef::new("asech", 0.0),
            ParamDef::new("acosech", 0.0),
            ParamDef::new("acoth", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [re, im, re_add, im_add, sqr, asech, acosech, acoth]
            let mut z = Complex::new(a.x, a.y);
            z.exp();
            z.div(&Complex::new(d[0], d[1]));
            if d[4] != 0.0 {
                z.sqr();
                z.mul(&Complex::new(d[4], 0.0));
            }
            z.add(&Complex::new(d[2], d[3]));
            z.sqrt();
            if d[5] != 0.0 {
                z.asech();
                z.mul(&Complex::new(d[5], 0.0));
            }
            if d[6] != 0.0 {
                z.acosech();
                z.mul(&Complex::new(d[6], 0.0));
            }
            if d[7] != 0.0 {
                z.acoth();
                z.mul(&Complex::new(d[7], 0.0));
            }
            z.scale(amount * M_2_PI);
            if ctx.random() < 0.5 {
                v.x += z.re;
                v.y += z.im;
            } else {
                v.x -= z.re;
                v.y -= z.im;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "var z = cx_div(cx_exp(vec2<f32>(a.x, a.y)), vec2<f32>(D(0), D(1)));
if (D(4) != 0.0) { z = cx_mul(cx_sqr(z), vec2<f32>(D(4), 0.0)); }
z = cx_sqrt(cx_add(z, vec2<f32>(D(2), D(3))));
if (D(5) != 0.0) { z = cx_mul(cx_asech(z), vec2<f32>(D(5), 0.0)); }
if (D(6) != 0.0) { z = cx_mul(cx_acosech(z), vec2<f32>(D(6), 0.0)); }
if (D(7) != 0.0) { z = cx_mul(cx_acoth(z), vec2<f32>(D(7), 0.0)); }
z = cx_scale(z, AMOUNT * 2.0 / PI);
if (rnd() < 0.5) {
  v.x += z.x;
  v.y += z.y;
} else {
  v.x -= z.x;
  v.y -= z.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "extrude",
        params: &[ParamDef::range("root_face", 0.5, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            // d = [root_face]
            let mut rnd28 = || {
                let r1 = ctx.random_int(0xfffffff) as i32;
                let r2 = ctx.random_int(0xfffffff) as i32;
                ((r1 ^ r2.wrapping_shl(15)) & 0xfffffff) as f64 / 0xfffffff as f64
            };
            if rnd28() < d[0] {
                v.z = if amount < 0.0 { 0.0 } else { amount };
            } else {
                v.z = amount * rnd28();
            }
        },
        prepare: None,
        wgsl: "if (rnd() < D(0)) {
  v.z = max(AMOUNT, 0.0);
} else {
  v.z = AMOUNT * rnd();
}",
    },
    VarDef {
        name: "falloff2",
        params: &[
            ParamDef::range("scatter", 1.0, 0.0, 2.0),
            ParamDef::range("mindist", 0.5, 0.0, 2.0),
            ParamDef::range("mul_x", 1.0, 0.0, 1.0),
            ParamDef::range("mul_y", 1.0, 0.0, 1.0),
            ParamDef::range("mul_z", 0.0, 0.0, 1.0),
            ParamDef::range("mul_c", 0.0, 0.0, 1.0),
            ParamDef::new("x0", 0.0),
            ParamDef::new("y0", 0.0),
            ParamDef::new("z0", 0.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
            ParamDef::int("type", 0.0, 0.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [scatter, mindist, mul_x, mul_y, mul_z, mul_c, x0, y0, z0, invert, type, rmax]
            let (x, y, z) = (a.x, a.y, a.z);
            let mut dist = (sqr(x - d[6]) + sqr(y - d[7]) + sqr(z - d[8])).sqrt();
            if d[9] as i32 != 0 {
                dist = 1.0 - dist;
            }
            if dist < 0.0 {
                dist = 0.0;
            }
            dist = (dist - d[1]) * d[11];
            if dist < 0.0 {
                dist = 0.0;
            }
            match d[10] as i32 {
                1 => {
                    let r_in = (x * x + y * y + z * z).sqrt() + 1e-6;
                    let sigma = (z / r_in).asin() + d[4] * ctx.random() * dist;
                    let phi = y.atan2(x) + d[3] * ctx.random() * dist;
                    let r = r_in + d[2] * ctx.random() * dist;
                    let (sins, coss) = sigma.sin_cos();
                    let (sinp, cosp) = phi.sin_cos();
                    v.x += amount * (r * coss * cosp);
                    v.y += amount * (r * coss * sinp);
                    v.z += amount * sins;
                }
                2 => {
                    let sigma = dist * ctx.random() * 2.0 * M_PI;
                    let phi = dist * ctx.random() * M_PI;
                    let r = dist * ctx.random();
                    let (sins, coss) = sigma.sin_cos();
                    let (sinp, cosp) = phi.sin_cos();
                    v.x += amount * (x + d[2] * r * coss * cosp);
                    v.y += amount * (y + d[3] * r * coss * sinp);
                    v.z += amount * (z + d[4] * r * sins);
                }
                _ => {
                    v.x += amount * (x + d[2] * ctx.random() * dist);
                    v.y += amount * (y + d[3] * ctx.random() * dist);
                    v.z += amount * (z + d[4] * ctx.random() * dist);
                }
            }
            v.color = frac(v.color + d[5] * ctx.random() * dist).abs();
        },
        prepare: Some(|p: &[f64], _: &Affine| {
            let scatter = if p[0] < 1.0e-6 { 1.0e-6 } else { p[0] };
            let mindist = if p[1] < 0.0 { 0.0 } else { p[1] };
            vec![
                scatter,
                mindist,
                limit_value(p[2], 0.0, 1.0),
                limit_value(p[3], 0.0, 1.0),
                limit_value(p[4], 0.0, 1.0),
                limit_value(p[5], 0.0, 1.0),
                p[6],
                p[7],
                p[8],
                ftoi_limit(p[9], 0, 1),
                ftoi_limit(p[10], 0, 2),
                0.04 * scatter,
            ]
        }),
        wgsl: "let x = a.x;
let y = a.y;
let z = a.z;
var dist = length(vec3<f32>(x - D(6), y - D(7), z - D(8)));
if (i32(D(9)) != 0) { dist = 1.0 - dist; }
dist = max((max(dist, 0.0) - D(1)) * D(11), 0.0);
let ty = i32(D(10));
if (ty == 1) {
  let r_in = sqrt(x * x + y * y + z * z) + 1e-6;
  let sigma = asin(clamp(z / r_in, -1.0, 1.0)) + D(4) * rnd() * dist;
  let phi = atan2(y, x) + D(3) * rnd() * dist;
  let r = r_in + D(2) * rnd() * dist;
  v.x += AMOUNT * (r * cos(sigma) * cos(phi));
  v.y += AMOUNT * (r * cos(sigma) * sin(phi));
  v.z += AMOUNT * sin(sigma);
} else if (ty == 2) {
  let sigma = dist * rnd() * TWO_PI;
  let phi = dist * rnd() * PI;
  let r = dist * rnd();
  v.x += AMOUNT * (x + D(2) * r * cos(sigma) * cos(phi));
  v.y += AMOUNT * (y + D(3) * r * cos(sigma) * sin(phi));
  v.z += AMOUNT * (z + D(4) * r * sin(sigma));
} else {
  v.x += AMOUNT * (x + D(2) * rnd() * dist);
  v.y += AMOUNT * (y + D(3) * rnd() * dist);
  v.z += AMOUNT * (z + D(4) * rnd() * dist);
}
let c = v.color + D(5) * rnd() * dist;
v.color = abs(c - trunc(c));",
    },
    VarDef {
        name: "falloff3",
        params: &[
            ParamDef::int("blur_type", 0.0, 0.0, 2.0),
            ParamDef::int("blur_shape", 0.0, 0.0, 1.0),
            ParamDef::range("blur_strength", 1.0, 0.0, 2.0),
            ParamDef::range("min_distance", 0.5, 0.0, 2.0),
            ParamDef::int("invert_distance", 0.0, 0.0, 1.0),
            ParamDef::range("mul_x", 1.0, 0.0, 1.0),
            ParamDef::range("mul_y", 1.0, 0.0, 1.0),
            ParamDef::range("mul_z", 0.0, 0.0, 1.0),
            ParamDef::range("mul_c", 0.0, 0.0, 1.0),
            ParamDef::new("center_x", 0.0),
            ParamDef::new("center_y", 0.0),
            ParamDef::new("center_z", 0.0),
            ParamDef::new("alpha", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [blur_type, blur_shape, blur_strength, min_distance, invert_distance,
            //      mul_x, mul_y, mul_z, mul_c, center_x, center_y, center_z, alpha, r_max]
            let vin = [a.x, a.y, a.z, a.color];
            let rnd = [ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5, ctx.random() - 0.5];
            let (dx, dy, dz) = (vin[0] - d[9], vin[1] - d[10], vin[2] - d[11]);
            let radius = if d[1] as i32 == 0 {
                (dx * dx + dy * dy + dz * dz).sqrt()
            } else {
                java_max(dx.abs(), java_max(dy.abs(), dz.abs()))
            };
            let r_max = d[13];
            let base = if d[4] as i32 != 0 { java_max(1.0 - radius, 0.0) } else { java_max(radius, 0.0) };
            let dist = java_max((base - d[3]) * r_max, 0.0);
            let mul = [d[5], d[6], d[7], d[8]];
            let alpha = d[12];
            let vout = match d[0] as i32 {
                0 => {
                    let sigma = dist * rnd[1] * M_2PI;
                    let phi = dist * rnd[2] * M_PI;
                    let rad = dist * rnd[0];
                    let (ss, sc) = sigma.sin_cos();
                    let (ps, pc) = phi.sin_cos();
                    [
                        vin[0] + mul[0] * rad * sc * pc,
                        vin[1] + mul[1] * rad * sc * ps,
                        vin[2] + mul[2] * rad * ss,
                        vin[3] + mul[3] * dist * rnd[3],
                    ]
                }
                1 => {
                    if vin[0] == 0.0 && vin[1] == 0.0 && vin[2] == 0.0 {
                        vin
                    } else {
                        let r_in = (sqr(vin[0]) + sqr(vin[1]) + sqr(vin[2])).sqrt();
                        let aa = mul[1] * dist + alpha * dist / r_max.sqrt();
                        let bb = mul[2] * dist;
                        let sigma = (vin[2] / r_in).asin() + bb * rnd[2];
                        let phi = vin[1].atan2(vin[0]) + aa * rnd[1];
                        let r = r_in + mul[0] * rnd[0] * dist;
                        let (ss, sc) = sigma.sin_cos();
                        let (ps, pc) = phi.sin_cos();
                        [r * sc * pc, r * sc * ps, r * ss, vin[3] + mul[3] * rnd[3] * dist]
                    }
                }
                _ => {
                    let coeff = if r_max <= EPSILON { dist } else { dist + alpha * (falloff3_log_map(dist) - dist) };
                    [
                        vin[0] + falloff3_log_map(mul[0]) * falloff3_log_scale(rnd[0]) * coeff,
                        vin[1] + falloff3_log_map(mul[1]) * falloff3_log_scale(rnd[1]) * coeff,
                        vin[2] + falloff3_log_map(mul[2]) * falloff3_log_scale(rnd[2]) * coeff,
                        vin[3] + falloff3_log_map(mul[3]) * falloff3_log_scale(rnd[3]) * coeff,
                    ]
                }
            };
            v.x += vout[0] * amount;
            v.y += vout[1] * amount;
            v.z += vout[2] * amount;
            v.color = fmod(vout[3], 1.0).abs();
        },
        prepare: Some(|p: &[f64], _: &Affine| {
            let strength = if p[2] < 1.0e-6 { 1.0e-6 } else { p[2] };
            let min_dist = if p[3] < 0.0 { 0.0 } else { p[3] };
            vec![
                ftoi_limit(p[0], 0, 2),
                ftoi_limit(p[1], 0, 1),
                strength,
                min_dist,
                ftoi_limit(p[4], 0, 1),
                limit_value(p[5], 0.0, 1.0),
                limit_value(p[6], 0.0, 1.0),
                limit_value(p[7], 0.0, 1.0),
                limit_value(p[8], 0.0, 1.0),
                p[9],
                p[10],
                p[11],
                p[12],
                0.04 * strength,
            ]
        }),
        wgsl: "fn falloff3_sgnd(x: f32) -> f32 { return select(1.0, -1.0, x < 0.0); }
fn falloff3_log_scale(x: f32) -> f32 {
  if (x == 0.0) { return 0.0; }
  return log((abs(x) + 1.0) * 2.718281828459045) * falloff3_sgnd(x) / 2.718281828459045;
}
fn falloff3_log_map(x: f32) -> f32 {
  if (x == 0.0) { return 0.0; }
  return (2.718281828459045 + log(x * 2.718281828459045)) / 4.0 * falloff3_sgnd(x);
}
@body
let rx = rnd() - 0.5;
let ry = rnd() - 0.5;
let rz = rnd() - 0.5;
let rc = rnd() - 0.5;
let vin = vec4<f32>(a.x, a.y, a.z, a.color);
let dc = abs(vin.xyz - vec3<f32>(D(9), D(10), D(11)));
var radius = length(dc);
if (i32(D(1)) != 0) { radius = max(dc.x, max(dc.y, dc.z)); }
var rb = max(radius, 0.0);
if (i32(D(4)) != 0) { rb = max(1.0 - radius, 0.0); }
let r_max = D(13);
let dist = max((rb - D(3)) * r_max, 0.0);
let alpha = D(12);
var vo = vin;
let bt = i32(D(0));
if (bt == 0) {
  let sigma = dist * ry * TWO_PI;
  let phi = dist * rz * PI;
  let rad0 = dist * rx;
  vo = vec4<f32>(vin.x + D(5) * rad0 * cos(sigma) * cos(phi), vin.y + D(6) * rad0 * cos(sigma) * sin(phi),
    vin.z + D(7) * rad0 * sin(sigma), vin.w + D(8) * dist * rc);
} else if (bt == 1) {
  if (!(vin.x == 0.0 && vin.y == 0.0 && vin.z == 0.0)) {
    let r_in = length(vin.xyz);
    let aa = D(6) * dist + alpha * dist / sqrt(r_max);
    let bb = D(7) * dist;
    let sigma = asin(clamp(vin.z / r_in, -1.0, 1.0)) + bb * rz;
    let phi = atan2(vin.y, vin.x) + aa * ry;
    let r = r_in + D(5) * rx * dist;
    vo = vec4<f32>(r * cos(sigma) * cos(phi), r * cos(sigma) * sin(phi), r * sin(sigma), vin.w + D(8) * rc * dist);
  }
} else {
  var coeff = dist;
  if (r_max > EPSILON) { coeff = dist + alpha * (falloff3_log_map(dist) - dist); }
  vo = vin + vec4<f32>(falloff3_log_map(D(5)) * falloff3_log_scale(rx), falloff3_log_map(D(6)) * falloff3_log_scale(ry),
    falloff3_log_map(D(7)) * falloff3_log_scale(rz), falloff3_log_map(D(8)) * falloff3_log_scale(rc)) * coeff;
}
v.x += vo.x * AMOUNT;
v.y += vo.y * AMOUNT;
v.z += vo.z * AMOUNT;
v.color = abs(fmod(vo.w, 1.0));",
    },
    VarDef {
        name: "faraday",
        params: &[ParamDef::new("freq", 4.0), ParamDef::int("modes", 4.0, 1.0, 20.0), ParamDef::new("amp", 0.2)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [freq, modes, amp]
            let modes = java_max(d[1].abs(), 1.0);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let wave = d[2] * (d[0] * r).cos() * (modes * theta).cos();
            let nr = r + wave;
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![p[0], (p[1] as i32) as f64, p[2]]),
        wgsl: "let modes = max(abs(D(1)), 1.0);
let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
let nr = r + D(2) * cos(D(0) * r) * cos(modes * theta);
v.x += AMOUNT * nr * cos(theta);
v.y += AMOUNT * nr * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "farblur",
        params: &[
            ParamDef::new("x", 1.0),
            ParamDef::new("y", 1.0),
            ParamDef::new("z", 1.0),
            ParamDef::new("x_origin", 0.0),
            ParamDef::new("y_origin", 0.0),
            ParamDef::new("z_origin", 0.0),
        ],
        kind: VarKind::Post,
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            // d = [x, y, z, x_origin, y_origin, z_origin]
            let rsum = ctx.gauss_ring(d.as_ptr() as usize, 4, 3) - 2.0;
            let r = amount * (sqr(v.x - d[3]) + sqr(v.y - d[4]) + sqr(v.z - d[5])) * rsum;
            let (su, cu) = (ctx.random() * M_2PI).sin_cos();
            let (sv, cv) = (ctx.random() * M_2PI).sin_cos();
            v.x += d[0] * r * sv * cu;
            v.y += d[1] * r * sv * su;
            v.z += d[2] * r * cv;
        },
        prepare: None,
        wgsl: "var<private> farblur_ring: array<f32, 4>;
var<private> farblur_idx: u32;
var<private> farblur_init: bool;
fn farblur_sum() -> f32 {
  if (!farblur_init) {
    farblur_init = true;
    for (var i = 0; i < 4; i++) { farblur_ring[i] = rnd(); }
  }
  return farblur_ring[0] + farblur_ring[1] + farblur_ring[2] + farblur_ring[3];
}
fn farblur_push() {
  farblur_ring[farblur_idx] = rnd();
  farblur_idx = (farblur_idx + 1u) & 3u;
}
@body
let rsum = farblur_sum() - 2.0;
farblur_push();
let dx = v.x - D(3);
let dy = v.y - D(4);
let dz = v.z - D(5);
let r = AMOUNT * (dx * dx + dy * dy + dz * dz) * rsum;
let tu = rnd() * TWO_PI;
let tv = rnd() * TWO_PI;
v.x += D(0) * r * sin(tv) * cos(tu);
v.y += D(1) * r * sin(tv) * sin(tu);
v.z += D(2) * r * cos(tv);",
    },
    VarDef {
        name: "fault",
        params: &[ParamDef::range("angle", 30.0, -180.0, 180.0), ParamDef::new("displacement", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [angle, displacement]
            let ang = d[0] * M_PI / 180.0;
            let nx = (ang + M_PI * 0.5).cos();
            let ny = (ang + M_PI * 0.5).sin();
            let side = a.x * nx + a.y * ny;
            let s = if side == 0.0 {
                0.0
            } else if side < 0.0 {
                -1.0
            } else {
                1.0
            };
            v.x += amount * (a.x + s * d[1] * ang.cos());
            v.y += amount * (a.y + s * d[1] * ang.sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ang = D(0) * PI / 180.0;
let side = a.x * cos(ang + HALF_PI) + a.y * sin(ang + HALF_PI);
var s = 1.0;
if (side < 0.0) { s = -1.0; } else if (side == 0.0) { s = 0.0; }
v.x += AMOUNT * (a.x + s * D(1) * cos(ang));
v.y += AMOUNT * (a.y + s * D(1) * sin(ang));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "fbm_warp",
        params: &[ParamDef::new("freq", 1.0), ParamDef::new("amp", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [freq, amp]
            let x = a.x * d[0];
            let y = a.y * d[0];
            let dx = d[1]
                * ((x + 0.7).sin() * (y * 1.3).cos()
                    + 0.5 * (x * 2.0 + 1.1).sin() * (y * 2.6 + 0.2).cos()
                    + 0.25 * (x * 4.0 + 2.3).sin() * (y * 5.2 + 0.9).cos());
            let dy = d[1]
                * ((x * 1.1 + 0.3).cos() * (y * 0.9 + 0.5).sin()
                    + 0.5 * (x * 2.2 + 1.3).cos() * (y * 1.8 + 1.1).sin()
                    + 0.25 * (x * 4.4 + 2.7).cos() * (y * 3.6 + 2.1).sin());
            v.x += amount * (a.x + dx);
            v.y += amount * (a.y + dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = a.x * D(0);
let y = a.y * D(0);
let dx = D(1) * (sin(x + 0.7) * cos(y * 1.3) + 0.5 * sin(x * 2.0 + 1.1) * cos(y * 2.6 + 0.2)
  + 0.25 * sin(x * 4.0 + 2.3) * cos(y * 5.2 + 0.9));
let dy = D(1) * (cos(x * 1.1 + 0.3) * sin(y * 0.9 + 0.5) + 0.5 * cos(x * 2.2 + 1.3) * sin(y * 1.8 + 1.1)
  + 0.25 * cos(x * 4.4 + 2.7) * sin(y * 3.6 + 2.1));
v.x += AMOUNT * (a.x + dx);
v.y += AMOUNT * (a.y + dy);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "fdisc",
        params: &[
            ParamDef::new("ashift", 1.0),
            ParamDef::new("rshift", 1.0),
            ParamDef::new("xshift", 0.0),
            ParamDef::new("yshift", 0.0),
            ParamDef::new("term1", 1.0),
            ParamDef::new("term2", 0.0),
            ParamDef::new("term3", 0.0),
            ParamDef::new("term4", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [ashift, rshift, xshift, yshift, term1, term2, term3, term4]
            let afactor = M_2PI / (a.rad() + d[0]);
            let r = (a.y.atan2(a.x) * M_1_PI + d[1]) * 0.5;
            let xfactor = (afactor + d[2]).cos();
            let yfactor = (afactor + d[3]).sin();
            let pr = amount * r;
            let prx = pr * xfactor;
            let pry = pr * yfactor;
            v.x += (d[4] * prx) + (d[5] * a.x * prx) + (d[6] * a.x * pr) + (d[7] * a.x);
            v.y += (d[4] * pry) + (d[5] * a.y * pry) + (d[6] * a.y * pr) + (d[7] * a.y);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let afactor = TWO_PI / (rad(a) + D(0));
let r = (atan2(a.y, a.x) / PI + D(1)) * 0.5;
let pr = AMOUNT * r;
let prx = pr * cos(afactor + D(2));
let pry = pr * sin(afactor + D(3));
v.x += (D(4) * prx) + (D(5) * a.x * prx) + (D(6) * a.x * pr) + (D(7) * a.x);
v.y += (D(4) * pry) + (D(5) * a.y * pry) + (D(6) * a.y * pr) + (D(7) * a.y);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "fibonacci2",
        params: &[ParamDef::new("sc", 1.0), ParamDef::new("sc2", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [sc, sc2]
            let ffive = 1.0 / 2.2360679774997898;
            let fnatlog = 1.618_033_988_749_895_f64.ln();
            let (snum1, cnum1) = (a.y * fnatlog).sin_cos();
            let (snum2, cnum2) = ((a.x * M_PI + a.y * fnatlog) * -1.0).sin_cos();
            let eradius1 = d[0] * (d[1] * (a.x * fnatlog)).exp();
            let eradius2 = d[0] * (d[1] * ((a.x * fnatlog - a.y * M_PI) * -1.0)).exp();
            v.x += amount * (eradius1 * cnum1 - eradius2 * cnum2) * ffive;
            v.y += amount * (eradius1 * snum1 - eradius2 * snum2) * ffive;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ffive = 0.447213595499958;
let fnatlog = 0.481211825059603;
let aa = a.y * fnatlog;
let bb = -(a.x * PI + a.y * fnatlog);
let e1 = D(0) * exp(D(1) * (a.x * fnatlog));
let e2 = D(0) * exp(D(1) * -(a.x * fnatlog - a.y * PI));
v.x += AMOUNT * (e1 * cos(aa) - e2 * cos(bb)) * ffive;
v.y += AMOUNT * (e1 * sin(aa) - e2 * sin(bb)) * ffive;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "flame_bulb",
        params: &[
            ParamDef::new("root_power", 8.0),
            ParamDef::new("radius_pow", 1.0),
            ParamDef::new("add C", 1.0),
            ParamDef::int("color mode 1-4", 1.0, 0.0, 6.0),
            ParamDef::new("color scale", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [root_power, radius_pow, add_C, color_mode, color_scale]
            let power = d[0];
            let pw = 1.0 / power;
            let mut x = amount * a.x;
            let mut y = amount * a.y;
            let mut z = amount * a.z;
            let (xo, yo, zo) = (x, y, z);
            let mag1 = ((x * x + y * y + z * z).sqrt() / 2.0) * d[2];
            if d[2] != 0.0 {
                x += mag1;
                y += mag1;
                z += mag1;
            }
            // java.util.Random.nextInt((int) power + 1)
            let bound = (power as i32).saturating_add(1).max(1) as usize;
            let k = ctx.random_int(bound) as f64;
            let k2 = ctx.random_int(bound) as f64;
            for i in 0..2 {
                x -= xo;
                y -= yo;
                z -= zo;
                let sq_r = (x * x + y * y + z * z).sqrt();
                let r = sq_r.powf(pw);
                let theta = ((z / sq_r).acos() + 2.0 * k * M_PI) * pw;
                let zangle = (y.atan2(x) + i as f64 * 2.0 * k2 * M_PI) * pw;
                let rp = r.powf(d[1]);
                x = theta.sin() * zangle.cos() * rp;
                y = theta.sin() * zangle.sin() * rp;
                z = theta.cos() * rp;
            }
            v.x = x;
            v.y = y;
            v.z = z;
            let mag2 = mag1 / d[4];
            let m = (x * x + y * y + z * z).sqrt();
            match d[3] as i32 {
                1 => v.color = mag2,
                2 => v.color += mag2,
                3 => v.color = (m + mag2 * 2.0).sin(),
                4 => v.color += (m + mag2 * 2.0).sin(),
                5 => v.color = (m + mag2 * 2.0).ln(),
                6 => v.color += (m + mag2 * 2.0).ln(),
                _ => {}
            }
            if ctx.preserve_z {
                v.z = amount * a.z;
            }
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![p[0], p[1], p[2], ftoi_limit(p[3], 0, 6), p[4]]),
        wgsl: "let pw = 1.0 / D(0);
var x = AMOUNT * a.x;
var y = AMOUNT * a.y;
var z = AMOUNT * a.z;
let xo = x;
let yo = y;
let zo = z;
let mag1 = (sqrt(x * x + y * y + z * z) / 2.0) * D(2);
if (D(2) != 0.0) {
  x += mag1;
  y += mag1;
  z += mag1;
}
let bound = max(i32(D(0)) + 1, 1);
let k = f32(min(i32(rnd() * f32(bound)), bound - 1));
let k2 = f32(min(i32(rnd() * f32(bound)), bound - 1));
for (var i = 0; i < 2; i++) {
  x -= xo;
  y -= yo;
  z -= zo;
  let sq_r = sqrt(x * x + y * y + z * z);
  let r = java_pow(sq_r, pw);
  let theta = (acos(clamp(z / sq_r, -1.0, 1.0)) + 2.0 * k * PI) * pw;
  let zangle = (atan2(y, x) + f32(i) * 2.0 * k2 * PI) * pw;
  let rp = java_pow(r, D(1));
  x = sin(theta) * cos(zangle) * rp;
  y = sin(theta) * sin(zangle) * rp;
  z = cos(theta) * rp;
}
v.x = x;
v.y = y;
v.z = z;
let mag2 = mag1 / D(4);
let m = sqrt(x * x + y * y + z * z);
let cm = i32(D(3));
if (cm == 1) { v.color = mag2; }
else if (cm == 2) { v.color += mag2; }
else if (cm == 3) { v.color = sin(m + mag2 * 2.0); }
else if (cm == 4) { v.color += sin(m + mag2 * 2.0); }
else if (cm == 5) { v.color = log(m + mag2 * 2.0); }
else if (cm == 6) { v.color += log(m + mag2 * 2.0); }
if (PRESERVE_Z) { v.z = AMOUNT * a.z; }",
    },
    VarDef {
        name: "flatten",
        params: &[],
        kind: VarKind::Post,
        is_3d: false,
        apply: |_, _, _, v, _| {
            v.z = 0.0;
        },
        prepare: None,
        wgsl: "v.z = 0.0;",
    },
    VarDef {
        name: "flourish",
        params: &[ParamDef::range("n", 3.0, 1.0, 50.0), ParamDef::new("amp", 0.5), ParamDef::new("damp", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [n, amp, damp]
            let damp = java_max(d[2].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let nr = r + d[1] * (d[0] * theta).sin() * (-damp * r).exp();
            v.x += amount * nr * theta.cos();
            v.y += amount * nr * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let damp = max(abs(D(2)), 0.01);
let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
let nr = r + D(1) * sin(D(0) * theta) * exp(-damp * r);
v.x += AMOUNT * nr * cos(theta);
v.y += AMOUNT * nr * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "flower_db",
        params: &[
            ParamDef::range("petals", 6.0, 0.0, 20.0),
            ParamDef::new("petal_split", 0.0),
            ParamDef::new("petal_spread", 1.0),
            ParamDef::new("stem_thickness", 1.0),
            ParamDef::new("stem_length", 0.0),
            ParamDef::new("petal_fold_strength", 0.0),
            ParamDef::new("petal_fold_radius", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, _ctx, a, v, amount| {
            // d = [petals, petal_split, petal_spread, stem_thickness, stem_length,
            //      petal_fold_strength, petal_fold_radius]
            let t = a.atan_yx();
            let r = amount * a.sumsq().sqrt();
            let r = r * ((d[2] + (d[0] * t).sin()) * (d[1] * d[0] * t).cos()).abs();
            v.x += t.sin() * r;
            v.y += t.cos() * r;
            v.z -= d[3] * ((2.0 / r) - 1.0);
            let rnew = (v.x * v.x + v.y * v.y).sqrt();
            if rnew > d[6] {
                v.z += (rnew - d[6]) * d[5];
            }
            if d[4] != 0.0 && v.z <= -d[4] {
                v.z = -d[4];
            }
        },
        prepare: None,
        wgsl: "let t = atan_yx(a);
let r = AMOUNT * sqrt(sumsq(a)) * abs((D(2) + sin(D(0) * t)) * cos(D(1) * D(0) * t));
v.x += sin(t) * r;
v.y += cos(t) * r;
v.z -= D(3) * ((2.0 / r) - 1.0);
let rnew = sqrt(v.x * v.x + v.y * v.y);
if (rnew > D(6)) { v.z += (rnew - D(6)) * D(5); }
if (D(4) != 0.0 && v.z <= -D(4)) { v.z = -D(4); }",
    },
    simple(
        "foci_3D",
        true,
        |_, _ctx, a, v, amount| {
            let expx = a.x.exp() * 0.5;
            let expnx = 0.25 / expx;
            let mut boot = a.z;
            let kikr = a.y.atan2(a.x);
            if boot == 0.0 {
                boot = kikr;
            }
            let (siny, cosy) = a.y.sin_cos();
            let (sinz, cosz) = boot.sin_cos();
            let tmp = amount / (expx + expnx - (cosy * cosz));
            v.x += (expx - expnx) * tmp;
            v.y += siny * tmp;
            v.z += sinz * tmp;
        },
        "let expx = exp(a.x) * 0.5;
let expnx = 0.25 / expx;
var boot = a.z;
if (boot == 0.0) { boot = atan2(a.y, a.x); }
let tmp = AMOUNT / (expx + expnx - (cos(a.y) * cos(boot)));
v.x += (expx - expnx) * tmp;
v.y += sin(a.y) * tmp;
v.z += sin(boot) * tmp;",
    ),
    VarDef {
        name: "fog_bank",
        params: &[ParamDef::range("haze", 0.5, 0.0, 1.0), ParamDef::new("wind", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [haze, wind]
            let haze = java_max(0.0, java_min(1.0, d[0]));
            let r2 = a.x * a.x + a.y * a.y;
            let fade = 1.0 - haze * (-r2 * 0.5).exp();
            let dx = d[1] * 0.3;
            let dy = d[1] * 0.05 * a.x.sin();
            v.x += amount * (a.x * fade + dx);
            v.y += amount * (a.y * fade + dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let haze = clamp(D(0), 0.0, 1.0);
let fade = 1.0 - haze * exp(-(a.x * a.x + a.y * a.y) * 0.5);
v.x += AMOUNT * (a.x * fade + D(1) * 0.3);
v.y += AMOUNT * (a.y * fade + D(1) * 0.05 * sin(a.x));
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "fourth",
        params: &[
            ParamDef::new("spin", M_PI),
            ParamDef::new("space", 0.1),
            ParamDef::new("twist", 0.2),
            ParamDef::new("x", 0.3),
            ParamDef::new("y", 0.12),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [spin, space, twist, x, y]
            let sqrvvar = amount * amount;
            if a.x > 0.0 && a.y > 0.0 {
                // spherical
                let ang = a.y.atan2(a.x);
                let r = 1.0 / (sqr(a.x) + sqr(a.y)).sqrt();
                let (s, c) = ang.sin_cos();
                v.x += amount * r * c;
                v.y += amount * r * s;
            } else if a.x > 0.0 && a.y < 0.0 {
                // loonie
                let r2 = sqr(a.x) + sqr(a.y);
                if r2 < sqrvvar {
                    let r = amount * (sqrvvar / r2 - 1.0).sqrt();
                    v.x += r * a.x;
                    v.y += r * a.y;
                } else {
                    v.x += amount * a.x;
                    v.y += amount * a.y;
                }
            } else if a.x < 0.0 && a.y > 0.0 {
                // susan
                let x = a.x - d[3];
                let y = a.y + d[4];
                let r = (x * x + y * y).sqrt();
                if r < amount {
                    let ang = y.atan2(x) + d[0] + d[2] * (amount - r);
                    let (sina, cosa) = ang.sin_cos();
                    let r = amount * r;
                    v.x += r * cosa + d[3];
                    v.y += r * sina - d[4];
                } else {
                    let r = amount * (1.0 + d[1] / r);
                    v.x += r * x + d[3];
                    v.y += r * y - d[4];
                }
            } else {
                v.x += amount * a.x;
                v.y += amount * a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sqrvvar = AMOUNT * AMOUNT;
if (a.x > 0.0 && a.y > 0.0) {
  let ang = atan2(a.y, a.x);
  let r = 1.0 / sqrt(a.x * a.x + a.y * a.y);
  v.x += AMOUNT * r * cos(ang);
  v.y += AMOUNT * r * sin(ang);
} else if (a.x > 0.0 && a.y < 0.0) {
  let r2 = a.x * a.x + a.y * a.y;
  if (r2 < sqrvvar) {
    let r = AMOUNT * sqrt(sqrvvar / r2 - 1.0);
    v.x += r * a.x;
    v.y += r * a.y;
  } else {
    v.x += AMOUNT * a.x;
    v.y += AMOUNT * a.y;
  }
} else if (a.x < 0.0 && a.y > 0.0) {
  let x = a.x - D(3);
  let y = a.y + D(4);
  let r = sqrt(x * x + y * y);
  if (r < AMOUNT) {
    let ang = atan2(y, x) + D(0) + D(2) * (AMOUNT - r);
    v.x += AMOUNT * r * cos(ang) + D(3);
    v.y += AMOUNT * r * sin(ang) - D(4);
  } else {
    let rr = AMOUNT * (1.0 + D(1) / r);
    v.x += rr * x + D(3);
    v.y += rr * y - D(4);
  }
} else {
  v.x += AMOUNT * a.x;
  v.y += AMOUNT * a.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "fractal_zoom",
        params: &[ParamDef::new("zoom", 2.0), ParamDef::new("angle", 0.5), ParamDef::int("iters", 3.0, 0.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [zoom, angle, iters]
            let (mut x, mut y) = (a.x, a.y);
            let (sn, cs) = d[1].sin_cos();
            for _ in 0..d[2] as i32 {
                let nx = x * cs - y * sn;
                let ny = x * sn + y * cs;
                x = nx * d[0];
                y = ny * d[0];
            }
            v.x += amount * x;
            v.y += amount * y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![p[0], p[1], (p[2] as i32) as f64]),
        wgsl: "var x = a.x;
var y = a.y;
let cs = cos(D(1));
let sn = sin(D(1));
let iters = i32(D(2));
for (var i = 0; i < iters; i++) {
  let nx = x * cs - y * sn;
  let ny = x * sn + y * cs;
  x = nx * D(0);
  y = ny * D(0);
}
v.x += AMOUNT * x;
v.y += AMOUNT * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "fract_dragon_wf",
        params: fract_wf_params!(-0.25, 1.2, -0.75, 0.75, 4.0, 5.0, -0.5;
            ParamDef::new("xseed", 1.4), ParamDef::new("yseed", 0.85)),
        kind: VarKind::Normal,
        is_3d: false,
        apply: fract_wf_apply,
        prepare: Some(|p: &[f64], _: &Affine| fract_wf_prepare(p, 2, FW_KIND_DRAGON, 2.0, p[6], p[7])),
        wgsl: fract_wf_wgsl!(
            "fract_dragon_wf",
            "  let xseed = prm[base + 21u];
  let yseed = prm[base + 22u];
  let t1 = z.x - xs + ys;
  let t2 = z.y - 2.0 * z.x * z.y;
  return vec2<f32>(t1 * xseed - t2 * yseed, t1 * yseed + t2 * xseed);"
        ),
    },
    VarDef {
        name: "fract_julia_wf",
        params: fract_wf_params!(-1.0, 1.0, -1.2, 1.2, 4.0, 3.8, 0.0;
            ParamDef::int("power", 2.0, 2.0, 12.0), ParamDef::new("xseed", 0.35), ParamDef::new("yseed", 0.09)),
        kind: VarKind::Normal,
        is_3d: false,
        apply: fract_wf_apply,
        prepare: Some(|p: &[f64], _: &Affine| {
            fract_wf_prepare(p, 3, FW_KIND_JULIA, ftoi(p[6]).max(2) as f64, p[7], p[8])
        }),
        wgsl: fract_wf_wgsl!(
            "fract_julia_wf",
            "  let c = vec2<f32>(prm[base + 21u], prm[base + 22u]);
  let power = i32(prm[base + 20u]);
  if (power == 2) { return vec2<f32>(xs - ys, 2.0 * z.x * z.y) + c; }
  if (power == 3) { return vec2<f32>(xs * z.x - 3.0 * z.x * ys, 3.0 * xs * z.y - ys * z.y) + c; }
  if (power == 4) { return vec2<f32>(xs * xs + ys * ys - 6.0 * xs * ys, 4.0 * z.x * z.y * (xs - ys)) + c; }
  var x2 = z.x * (xs * xs - 10.0 * xs * ys + 5.0 * ys * ys);
  var y2 = z.y * (ys * ys - 10.0 * xs * ys + 5.0 * xs * xs);
  for (var k = 5; k < power; k++) {
    let xa = z.x * x2 - z.y * y2;
    let ya = z.x * y2 + x2 * z.y;
    x2 = xa;
    y2 = ya;
  }
  return vec2<f32>(x2, y2) + c;"
        ),
    },
    VarDef {
        name: "fract_mandelbrot_wf",
        params: fract_wf_params!(-2.35, 0.75, -1.2, 1.2, 3.0, 4.0, 0.55;
            ParamDef::int("power", 2.0, 2.0, 12.0)),
        kind: VarKind::Normal,
        is_3d: false,
        apply: fract_wf_apply,
        prepare: Some(|p: &[f64], _: &Affine| {
            fract_wf_prepare(p, 1, FW_KIND_MANDELBROT, ftoi(p[6]).max(2) as f64, 0.0, 0.0)
        }),
        wgsl: fract_wf_wgsl!(
            "fract_mandelbrot_wf",
            "  let c = s;
  let power = i32(prm[base + 20u]);
  if (power == 2) { return vec2<f32>(xs - ys, 2.0 * z.x * z.y) + c; }
  if (power == 3) { return vec2<f32>(xs * z.x - 3.0 * z.x * ys, 3.0 * xs * z.y - ys * z.y) + c; }
  if (power == 4) { return vec2<f32>(xs * xs + ys * ys - 6.0 * xs * ys, 4.0 * z.x * z.y * (xs - ys)) + c; }
  var x2 = z.x * (xs * xs - 10.0 * xs * ys + 5.0 * ys * ys);
  var y2 = z.y * (ys * ys - 10.0 * xs * ys + 5.0 * xs * xs);
  for (var k = 5; k < power; k++) {
    let xa = z.x * x2 - z.y * y2;
    let ya = z.x * y2 + x2 * z.y;
    x2 = xa;
    y2 = ya;
  }
  return vec2<f32>(x2, y2) + c;"
        ),
    },
    VarDef {
        name: "fract_meteors_wf",
        params: fract_wf_params!(-1.7, 1.7, -1.1, 1.1, 3.0, 5.7, 0.0;),
        kind: VarKind::Normal,
        is_3d: false,
        apply: fract_wf_apply,
        prepare: Some(|p: &[f64], _: &Affine| fract_wf_prepare(p, 0, FW_KIND_METEORS, 2.0, 0.0, 0.0)),
        wgsl: fract_wf_wgsl!(
            "fract_meteors_wf",
            "  let q = xs + ys;
  return vec2<f32>((s.x * z.x - s.y * z.y) - (s.x * z.x + s.y * z.y) / q,
    (s.x * z.y + s.y * z.x) + (s.x * z.y - s.y * z.x) / q);"
        ),
    },
    VarDef {
        name: "fract_pearls_wf",
        params: fract_wf_params!(-2.0, 2.0, -2.0, 2.0, 1.0, 2.45, 0.0;
            ParamDef::new("xseed", 0.0), ParamDef::new("yseed", -0.31)),
        kind: VarKind::Normal,
        is_3d: false,
        apply: fract_wf_apply,
        prepare: Some(|p: &[f64], _: &Affine| fract_wf_prepare(p, 2, FW_KIND_PEARLS, 2.0, p[6], p[7])),
        wgsl: fract_wf_wgsl!(
            "fract_pearls_wf",
            "  let xseed = prm[base + 21u];
  let yseed = prm[base + 22u];
  let q = xs + ys;
  return vec2<f32>(xseed * z.x - yseed * z.y - ((xseed * z.x + yseed * z.y) / q),
    xseed * z.y + yseed * z.x + ((xseed * z.y - yseed * z.x) / q));"
        ),
    },
    VarDef {
        name: "fract_salamander_wf",
        params: fract_wf_params!(-2.0, 2.0, -2.0, 2.0, 1.0, 2.45, 0.0;
            ParamDef::new("xseed", 0.8), ParamDef::new("yseed", -0.15)),
        kind: VarKind::Normal,
        is_3d: false,
        apply: fract_wf_apply,
        prepare: Some(|p: &[f64], _: &Affine| fract_wf_prepare(p, 2, FW_KIND_SALAMANDER, 2.0, p[6], p[7])),
        wgsl: fract_wf_wgsl!(
            "fract_salamander_wf",
            "  let xseed = prm[base + 21u];
  let yseed = prm[base + 22u];
  let t1 = xs - ys;
  let t2 = 2.0 * z.x * z.y;
  return vec2<f32>(t1 * xseed - t2 * yseed - 1.0, t1 * yseed + t2 * xseed);"
        ),
    },
    VarDef {
        name: "fresnel",
        params: &[ParamDef::new("x0", 0.0), ParamDef::new("y0", 0.0), ParamDef::new("ring", -2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x0, y0, ring]
            let r = ((a.x - d[0]) * (a.x - d[0]) + (a.y - d[1]) * (a.y - d[1])).sqrt();
            let r = r * d[2];
            let r = r - r.floor();
            v.x = amount * (a.x * r);
            v.y = amount * (a.y * r);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = fract(distance(vec2<f32>(a.x, a.y), vec2<f32>(D(0), D(1))) * D(2));
v.x = AMOUNT * (a.x * r);
v.y = AMOUNT * (a.y * r);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "funnel",
        params: &[ParamDef::int("effect", 8.0, -20.0, 20.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [effect]
            v.x += amount * a.x.tanh() * (1.0 / a.x.cos() + d[0] * M_PI);
            v.y += amount * a.y.tanh() * (1.0 / a.y.cos() + d[0] * M_PI);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![ftoi_f(p[0])]),
        wgsl: "v.x += AMOUNT * tanh(a.x) * (1.0 / cos(a.x) + D(0) * PI);
v.y += AMOUNT * tanh(a.y) * (1.0 / cos(a.y) + D(0) * PI);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "gamma",
        false,
        |_, ctx, a, v, amount| {
            v.x += lgamma(a.y.hypot(a.x)) * amount;
            v.y += a.y.atan2(a.x) * amount;
            ctx.pass_z(a, v, amount);
        },
        "v.x += lgamma(length(vec2<f32>(a.y, a.x))) * AMOUNT;
v.y += atan2(a.y, a.x) * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "gdoffs",
        params: &[
            ParamDef::new("delta_x", 0.0),
            ParamDef::new("delta_y", 0.0),
            ParamDef::new("area_x", 2.0),
            ParamDef::new("area_y", 2.0),
            ParamDef::new("center_x", 0.0),
            ParamDef::new("center_y", 0.0),
            ParamDef::int("gamma", 1.0, -10.0, 10.0),
            ParamDef::int("square", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [<8 params>, gdodx, gdody, gdocx, gdocy, gdob, gdos]
            let osc_x = gdoffs_fosc(d[8], 1.0);
            let osc_y = if d[13] as i32 != 0 { osc_x } else { gdoffs_fosc(d[9], 1.0) };
            let in_x = a.x + d[10];
            let in_y = a.y + d[11];
            let gdob = d[12];
            let out_x = gdoffs_flip(
                gdoffs_flip(in_x, gdoffs_fosc(in_x, 4.0), osc_x),
                gdoffs_fosc(gdoffs_fclp(gdob * in_x), 4.0),
                osc_x,
            );
            let out_y = gdoffs_flip(
                gdoffs_flip(in_y, gdoffs_fosc(in_y, 4.0), osc_y),
                gdoffs_fosc(gdoffs_fclp(gdob * in_y), 4.0),
                osc_y,
            );
            v.x += amount * out_x;
            v.y += amount * out_y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p: &[f64], _: &Affine| {
            let gdoax = (if p[2].abs() < 0.1 { 0.1 } else { p[2].abs() }) * 2.0;
            let gdoay = (if p[3].abs() < 0.1 { 0.1 } else { p[3].abs() }) * 2.0;
            let gdob = ftoi(p[6]) as f64 * 2.0 / java_max(gdoax, gdoay);
            let mut d = p.to_vec();
            d.extend([p[0] * 0.1, p[1] * 0.1, p[4], p[5], gdob, ftoi_f(p[7])]);
            d
        }),
        wgsl: "fn gdoffs_fclp(a: f32) -> f32 {
  let m = fmod(abs(a), 1.0);
  return select(m, -m, a < 0.0);
}
fn gdoffs_fosc(p: f32, a: f32) -> f32 { return gdoffs_fclp((-cos(p * a * TWO_PI) + 1.0) / 2.0); }
fn gdoffs_flip(a: f32, b: f32, c: f32) -> f32 { return c * (b - a) + a; }
@body
let osc_x = gdoffs_fosc(D(8), 1.0);
var osc_y = gdoffs_fosc(D(9), 1.0);
if (i32(D(13)) != 0) { osc_y = osc_x; }
let in_x = a.x + D(10);
let in_y = a.y + D(11);
let out_x = gdoffs_flip(gdoffs_flip(in_x, gdoffs_fosc(in_x, 4.0), osc_x), gdoffs_fosc(gdoffs_fclp(D(12) * in_x), 4.0), osc_x);
let out_y = gdoffs_flip(gdoffs_flip(in_y, gdoffs_fosc(in_y, 4.0), osc_y), gdoffs_fosc(gdoffs_fclp(D(12) * in_y), 4.0), osc_y);
v.x += AMOUNT * out_x;
v.y += AMOUNT * out_y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "gear_teeth",
        params: &[ParamDef::range("teeth", 12.0, 3.0, 25.0), ParamDef::range("depth", 0.3, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [teeth, depth]
            let ang = a.y.atan2(a.x);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let tooth = 0.5 * (1.0 + (d[0] * ang).cos());
            let rmod = r + d[1] * tooth;
            v.x += amount * rmod * ang.cos();
            v.y += amount * rmod * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ang = atan2(a.y, a.x);
let rmod = sqrt(a.x * a.x + a.y * a.y) + D(1) * 0.5 * (1.0 + cos(D(0) * ang));
v.x += AMOUNT * rmod * cos(ang);
v.y += AMOUNT * rmod * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "geometricPrimitives",
        params: &[
            ParamDef::range("line_vs_shape_mix", 0.5, 0.0, 1.0),
            ParamDef::range("density", 0.5, 0.0, 1.0),
            ParamDef::range("scale", 2.0, 0.5, 3.0),
            ParamDef::range("gridSize", 1.0, 0.5, 2.0),
            ParamDef::range("seed", 0.0, 0.0, 50.0),
            ParamDef::range("grid_jitter", 0.0, 0.0, 1.0),
            ParamDef::int("shape_type", 0.0, 0.0, 3.0),
            ParamDef::int("shape_mix_enable", 0.0, 0.0, 1.0),
            ParamDef::range("fill_prob", 0.5, 0.0, 1.0),
            ParamDef::range("radius", 0.45, 0.1, 2.1),
            ParamDef::range("hollow", 0.8, 0.0, 1.0),
            ParamDef::range("square_size", 0.9, 0.1, 2.1),
            ParamDef::int("connection_mode", 0.0, 0.0, 1.0),
            ParamDef::int("recursion_depth", 0.0, 0.0, 50.0),
            ParamDef::range("recursion_shape_mix", 0.5, 0.0, 1.0),
            ParamDef::range("recursion_scale", 0.25, 0.1, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: geometric_primitives_apply,
        prepare: Some(|p: &[f64], _: &Affine| {
            let mut d = p.to_vec();
            d[0] = limit_value(p[0], 0.0, 1.0);
            d[6] = (limit_value(p[6], 0.0, 3.0) as i32) as f64;
            d[7] = (limit_value(p[7], 0.0, 1.0) as i32) as f64;
            d[10] = limit_value(p[10], 0.0, 1.0);
            d[12] = (limit_value(p[12], 0.0, 1.0) as i32) as f64;
            d[13] = (limit_value(p[13], 0.0, 50.0) as i32) as f64;
            d[14] = limit_value(p[14], 0.0, 1.0);
            d
        }),
        wgsl: "fn geometricPrimitives_rfc(gx: f32, gy: f32, salt: f32) -> f32 {
  let t = sin(gx * 13.9898 + gy * 73.233 + salt) * 43758.5453;
  return t - trunc(t);
}
fn geometricPrimitives_exists(gx: f32, gy: f32, base: u32) -> bool {
  return geometricPrimitives_rfc(gx, gy, prm[base + 4u]) < prm[base + 1u];
}
fn geometricPrimitives_jitter(gx: f32, gy: f32, base: u32) -> vec2<f32> {
  let gj = prm[base + 5u];
  if (gj > 0.0) {
    let mj = gj * prm[base + 3u] * 0.5;
    let seed = prm[base + 4u];
    return vec2<f32>((geometricPrimitives_rfc(gx, gy, seed + 40.0) * 2.0 - 1.0) * mj,
      (geometricPrimitives_rfc(gx, gy, seed + 50.0) * 2.0 - 1.0) * mj);
  }
  return vec2<f32>(0.0);
}
fn geometricPrimitives_circle(filled: bool, c: vec2<f32>, rad: f32, hol: f32) -> vec2<f32> {
  let angle = rnd() * TWO_PI;
  var r = 0.0;
  if (filled) {
    r = sqrt(rnd()) * rad;
  } else {
    let inner = rad * hol;
    r = inner + rnd() * (rad - inner);
  }
  return c + r * vec2<f32>(cos(angle), sin(angle));
}
fn geometricPrimitives_square(filled: bool, c: vec2<f32>, size: f32) -> vec2<f32> {
  let hw = size * 0.5;
  if (filled) {
    let x = c.x + (rnd() * 2.0 - 1.0) * hw;
    let y = c.y + (rnd() * 2.0 - 1.0) * hw;
    return vec2<f32>(x, y);
  }
  let side = floor(rnd() * 4.0);
  let t = (rnd() * 2.0 - 1.0) * hw;
  if (side == 0.0) { return vec2<f32>(c.x + t, c.y + hw); }
  if (side == 1.0) { return vec2<f32>(c.x + t, c.y - hw); }
  if (side == 2.0) { return vec2<f32>(c.x + hw, c.y + t); }
  return vec2<f32>(c.x - hw, c.y + t);
}
fn geometricPrimitives_tri(o: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> vec2<f32> {
  var r1 = rnd();
  var r2 = rnd();
  if (r1 + r2 > 1.0) {
    r1 = 1.0 - r1;
    r2 = 1.0 - r2;
  }
  return (1.0 - r1 - r2) * o + r1 * p1 + r2 * p2;
}
fn geometricPrimitives_triangle(filled: bool, c: vec2<f32>, size: f32) -> vec2<f32> {
  let v1 = c + vec2<f32>(0.0, size);
  let v2 = c + vec2<f32>(-size * 0.866, -size * 0.5);
  let v3 = c + vec2<f32>(size * 0.866, -size * 0.5);
  if (filled) { return geometricPrimitives_tri(v1, v2, v3); }
  let side = floor(rnd() * 3.0);
  let t = rnd();
  if (side == 0.0) { return mix(v1, v2, t); }
  if (side == 1.0) { return mix(v2, v3, t); }
  return mix(v3, v1, t);
}
fn geometricPrimitives_hexagon(filled: bool, c: vec2<f32>, size: f32) -> vec2<f32> {
  let sa = PI / 3.0;
  var segment = 0.0;
  if (filled) { segment = floor(rnd() * 6.0); } else { segment = floor(rnd() * TWO_PI / sa); }
  let a1 = segment * sa;
  let a2 = (segment + 1.0) * sa;
  let p1 = c + size * vec2<f32>(cos(a1), sin(a1));
  let p2 = c + size * vec2<f32>(cos(a2), sin(a2));
  if (filled) { return geometricPrimitives_tri(c, p1, p2); }
  return mix(p1, p2, rnd());
}
@body
let seed = D(4);
let gs = D(3);
let gx = java_round(a.x * D(2));
let gy = java_round(a.y * D(2));
let center = vec2<f32>(gx, gy) * gs + geometricPrimitives_jitter(gx, gy, base);
var outp = vec2<f32>(0.0);
var valid = false;
if (rnd() < D(0)) {
  if (geometricPrimitives_exists(gx, gy, base)) {
    var ng = vec2<f32>(gx, gy);
    if (i32(D(12)) == 0) {
      let dir = rnd() * 4.0;
      if (dir < 1.0) { ng.x += 1.0; } else if (dir < 2.0) { ng.x -= 1.0; } else if (dir < 3.0) { ng.y += 1.0; } else { ng.y -= 1.0; }
    } else {
      var cands = array<vec2<f32>, 4>(vec2<f32>(gx + 1.0, gy), vec2<f32>(gx - 1.0, gy), vec2<f32>(gx, gy + 1.0), vec2<f32>(gx, gy - 1.0));
      var vl: array<vec2<f32>, 4>;
      var nv = 0;
      for (var i = 0; i < 4; i++) {
        if (geometricPrimitives_exists(cands[i].x, cands[i].y, base)) {
          vl[nv] = cands[i];
          nv += 1;
        }
      }
      if (nv > 0) { ng = vl[min(i32(floor(rnd() * f32(nv))), nv - 1)]; }
    }
    if (ng.x != gx || ng.y != gy) {
      let nc = ng * gs + geometricPrimitives_jitter(ng.x, ng.y, base);
      let depth = i32(D(13));
      if (depth > 0) {
        let stp = min(i32(floor(rnd() * f32(depth))), depth - 1);
        let t = (f32(stp) + 0.5) / f32(depth);
        let sc = center * (1.0 - t) + nc * t;
        let filled = geometricPrimitives_rfc(gx, gy, seed + 30.0 + f32(stp)) < D(8);
        if (rnd() < D(14)) {
          outp = geometricPrimitives_square(filled, sc, D(11) * D(15));
        } else {
          outp = geometricPrimitives_circle(filled, sc, D(9) * D(15), D(10));
        }
      } else {
        let t = rnd();
        outp = center * (1.0 - t) + nc * t;
      }
      valid = true;
    }
  }
} else if (geometricPrimitives_exists(gx, gy, base)) {
  var shape = i32(D(6));
  if (i32(D(7)) == 1) { shape = i32(floor(geometricPrimitives_rfc(gx, gy, seed + 10.0) * 4.0)); }
  let filled = geometricPrimitives_rfc(gx, gy, seed + 20.0) < D(8);
  if (shape == 0) {
    outp = geometricPrimitives_circle(filled, center, geometricPrimitives_rfc(gx, gy, -seed) * D(9) * gs, D(10));
  } else if (shape == 1) {
    outp = geometricPrimitives_square(filled, center, geometricPrimitives_rfc(gx, gy, 1.0 - seed) * D(11) * gs);
  } else if (shape == 2) {
    outp = geometricPrimitives_triangle(filled, center, geometricPrimitives_rfc(gx, gy, 2.0 - seed) * D(9) * gs);
  } else if (shape == 3) {
    outp = geometricPrimitives_hexagon(filled, center, geometricPrimitives_rfc(gx, gy, 3.0 - seed) * D(9) * gs);
  }
  valid = true;
}
if (valid) {
  v.hide = 0.0;
  v.x += outp.x * AMOUNT;
  v.y += outp.y * AMOUNT;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
} else {
  v.hide = 1.0;
}",
    },
    simple(
        "gingerbread",
        false,
        |_, ctx, a, v, amount| {
            let nx = 1.0 - a.y + a.x.abs();
            let ny = a.x;
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * (1.0 - a.y + abs(a.x));
v.y += AMOUNT * a.x;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "gingerbread_man",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // One orbit per render thread (see the module notes).
            let st = ctx.orbits.entry(d.as_ptr() as usize).or_insert([-0.1, 0.0]);
            let (x0, y0) = (st[0], st[1]);
            let xn = 1.0 - y0 + x0.abs();
            *st = [xn, x0];
            v.x = xn * amount;
            v.y = x0 * amount;
            ctx.pass_z(a, v, amount);
        },
        // one value, so that every instance has its own state key
        prepare: Some(|_, _| vec![0.0]),
        wgsl: "var x = -0.1 + 1.0e-4 * (rnd() - 0.5);
var y = 1.0e-4 * (rnd() - 0.5);
let n = 128 + min(i32(rnd() * 512.0), 511);
for (var i = 0; i < n; i++) {
  let xn = 1.0 - y + abs(x);
  y = x;
  x = xn;
}
v.x = x * AMOUNT;
v.y = y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "glitchy1",
        params: &[
            ParamDef::new("scale_x", 1.0),
            ParamDef::new("shift_x", 0.0),
            ParamDef::int("n_x", 2.0, 1.0, 100.0),
            ParamDef::new("scale_y", 1.0),
            ParamDef::new("shift_y", 0.0),
            ParamDef::int("n_y", 2.0, 1.0, 100.0),
            ParamDef::new("scale_z", 0.0),
            ParamDef::new("shift_z", 0.0),
            ParamDef::int("n_z", 2.0, 1.0, 100.0),
            ParamDef::new("width", 1.0),
            ParamDef::int("seed", 42.0, 0.0, 1000.0),
            ParamDef::range("angle", 30.0, -180.0, 180.0),
            ParamDef::new("u", 1.0),
            ParamDef::new("v", 1.0),
            ParamDef::new("w", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, _ctx, a, v, amount| {
            // d = [scale_x, shift_x, n_x, scale_y, shift_y, n_y, scale_z, shift_z, n_z, width,
            //      seed, angle, u, v, w, h = sin(angle), k = cos(angle)]
            let x = glitchy1_fold(a.x, d[1], d[0], d[2]);
            let y = glitchy1_fold(a.y, d[4], d[3], d[5]);
            let z = glitchy1_fold(a.z, d[7], d[6], d[8]);
            let (h, k) = (d[15], d[16]);
            let seed = d[10] as i32;
            let uu = d[12] * a.x;
            let vv = d[13] * (k * uu + h * a.y);
            let ww = d[14] * (k * uu - h * a.y);
            let bu = glitchy1_block(uu, d[9], seed);
            let bv = glitchy1_block(vv, d[9], seed);
            let bw = glitchy1_block(ww, d[9], seed);
            let base = bu.wrapping_add(bv);
            let flen = glitchy1_hash(base.wrapping_add(bw.wrapping_mul(seed.wrapping_neg())))
                + glitchy1_hash(base.wrapping_add(bw.wrapping_mul(seed) / 2));
            v.x += amount * x * flen;
            v.y += amount * y * flen;
            v.z += amount * z * flen;
        },
        prepare: Some(|p: &[f64], _: &Affine| {
            let mut d = p.to_vec();
            d[2] = ftoi_limit(p[2], 1, 100);
            d[5] = ftoi_limit(p[5], 1, 100);
            d[8] = ftoi_limit(p[8], 1, 100);
            d[10] = ftoi_f(p[10]);
            let ang = p[11].to_radians();
            d.extend([ang.sin(), ang.cos()]);
            d
        }),
        wgsl: "fn glitchy1_hash(a_in: i32) -> f32 {
  var h = (a_in ^ 61) ^ (a_in >> 16u);
  h = h + (h << 3u);
  h = h ^ (h >> 4u);
  h = h * 0x27d4eb2d;
  h = h ^ (h >> 15u);
  return f32(h) / 2147483647.0;
}
fn glitchy1_fold(c: f32, shift: f32, scale: f32, n: i32) -> f32 {
  if (scale == 0.0) { return c; }
  let nr = i32(floor((c - shift) * scale));
  if (nr >= 0) {
    if (nr % n == n - 1) { return 2.0 * shift - c; }
  } else {
    if (nr % n == 0) { return 2.0 * shift - c; }
  }
  return c;
}
fn glitchy1_block(t: f32, width: f32, seed: i32) -> i32 {
  let b = i32(floor(t * width));
  return i32(f32(b) + (2.0 - 4.0 * glitchy1_hash(b * seed + 1)));
}
@body
let seed = i32(D(10));
let x = glitchy1_fold(a.x, D(1), D(0), i32(D(2)));
let y = glitchy1_fold(a.y, D(4), D(3), i32(D(5)));
let z = glitchy1_fold(a.z, D(7), D(6), i32(D(8)));
let uu = D(12) * a.x;
let vv = D(13) * (D(16) * uu + D(15) * a.y);
let ww = D(14) * (D(16) * uu - D(15) * a.y);
let bu = glitchy1_block(uu, D(9), seed);
let bv = glitchy1_block(vv, D(9), seed);
let bw = glitchy1_block(ww, D(9), seed);
let flen = glitchy1_hash(bu + bv + bw * -seed) + glitchy1_hash(bu + bv + bw * seed / 2);
v.x += AMOUNT * x * flen;
v.y += AMOUNT * y * flen;
v.z += AMOUNT * z * flen;",
    },
    VarDef {
        name: "glitchy2",
        params: glitchy2_params!(("lr", 1.0), ("ur", -1.0), ("ll", -1.0), ("ul", 1.0)),
        kind: VarKind::Normal,
        is_3d: false,
        apply: glitchy2_apply,
        prepare: Some(|p: &[f64], _: &Affine| {
            let mut d = p.to_vec();
            d[0] = (limit_value(p[0], 0.0, 1.0) as i32) as f64;
            for q in 0..4 {
                let (s, c) = (p[1 + 13 * q] * M_PI_2).sin_cos();
                d.extend([s, c]);
            }
            d
        }),
        wgsl: "var count = 0;
var x = 0.0;
var y = 0.0;
var skip = false;
for (var q = 0u; q < 4u; q++) {
  let b = base + 1u + 13u * q;
  let in_x = select((a.x > prm[b + 2u]), (a.x < prm[b + 2u]), q >= 2u);
  let in_y = select((a.y > prm[b + 4u]), (a.y < prm[b + 4u]), (q & 1u) == 1u);
  if (!(in_x && in_y)) { continue; }
  let re_u = prm[b + 5u] * a.x - prm[b + 9u] * a.y + prm[b + 6u];
  let im_u = prm[b + 5u] * a.y + prm[b + 9u] * a.x + prm[b + 10u];
  let re_v = prm[b + 7u] * a.x - prm[b + 11u] * a.y + prm[b + 8u];
  let im_v = prm[b + 7u] * a.y + prm[b + 11u] * a.x + prm[b + 12u];
  let dd = re_v * re_v + im_v * im_v;
  if (dd == 0.0) {
    skip = true;
    break;
  }
  let ps = prm[base + 53u + 2u * q];
  let pc = prm[base + 54u + 2u * q];
  x += AMOUNT * (ps * a.y + pc * a.x) + prm[b + 1u] + (dd * (re_u * re_v + im_u * im_v));
  y += AMOUNT * (pc * a.y - ps * a.x) + prm[b + 3u] + (dd * (im_u * re_v - re_u * im_v));
  count += 1;
}
if (!skip) {
  let cnt = f32(count);
  if (i32(D(0)) != 0) {
    if (count == 0) {
      v.x += AMOUNT / a.y;
      v.y += AMOUNT / a.x;
    } else {
      v.x += AMOUNT / y / cnt;
      v.y += AMOUNT / x / cnt;
    }
  } else if (count == 0) {
    v.x += AMOUNT * a.x;
    v.y += AMOUNT * a.y;
  } else {
    v.x += AMOUNT * x / cnt;
    v.y += AMOUNT * y / cnt;
  }
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "exblur",
        "exp2_bs",
        "exp_multi",
        "extrude",
        "falloff2",
        "falloff3",
        "faraday",
        "farblur",
        "fault",
        "fbm_warp",
        "fdisc",
        "fibonacci2",
        "flame_bulb",
        "flatten",
        "flourish",
        "flower_db",
        "foci_3D",
        "fog_bank",
        "fourth",
        "fractal_zoom",
        "fract_dragon_wf",
        "fract_julia_wf",
        "fract_mandelbrot_wf",
        "fract_meteors_wf",
        "fract_pearls_wf",
        "fract_salamander_wf",
        "fresnel",
        "funnel",
        "gamma",
        "gdoffs",
        "gear_teeth",
        "geometricPrimitives",
        "gingerbread",
        "gingerbread_man",
        "glitchy1",
        "glitchy2",
    ];

    /// Registered without a GPU implementation.
    const CPU_ONLY: &[&str] = &[];

    const POINTS: &[(f64, f64, f64)] = &[
        (0.3, 0.2, 0.1),
        (-0.7, 0.5, -0.2),
        (1.3, -0.9, 0.4),
        (0.05, -0.02, 0.0),
        (-2.1, -1.7, 1.1),
        (0.6, 0.0, -0.5),
    ];

    fn find(name: &str) -> &'static VarDef {
        DEFS.iter().find(|d| d.name == name).unwrap()
    }

    fn run(def: &VarDef, data: &[f64], ctx: &mut Ctx, p: (f64, f64, f64)) -> (Pt, Pt) {
        let mut a = Pt { x: p.0, y: p.1, z: p.2, color: 0.5, ..Default::default() };
        let mut v = Pt { x: 0.1, y: -0.2, z: 0.05, color: 0.5, ..Default::default() };
        (def.apply)(data, ctx, &mut a, &mut v, 0.8);
        (a, v)
    }

    #[test]
    fn all_names_present_and_unique() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert_eq!(DEFS.iter().filter(|d| d.name == *n).count(), 1, "{n}");
        }
        for def in DEFS {
            assert_eq!(def.supports_gpu(), !CPU_ONLY.contains(&def.name), "{}", def.name);
        }
    }

    #[test]
    fn default_params_give_finite_results() {
        let mut ctx = Ctx::new(42, true);
        for def in DEFS {
            let data = def.prepare_data(&[], &Affine::IDENTITY);
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
    fn non_default_modes_are_finite() {
        let mut ctx = Ctx::new(5, false);
        let cases: &[(&str, &[f64])] = &[
            ("falloff2", &[1.0, 0.1, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0]),
            ("falloff2", &[1.0, 0.1, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 2.0]),
            ("falloff3", &[1.0, 1.0, 1.0, 0.1, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.5]),
            ("falloff3", &[2.0, 0.0, 1.0, 0.1, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.5]),
            ("exp_multi", &[1.0, 0.5, 0.2, 0.1, 1.0, 1.0, 1.0, 1.0]),
            ("fract_julia_wf", &[100.0, -1.0, 1.0, -1.2, 1.2, 0.0, 6.0, 0.35, 0.09]),
            ("fract_mandelbrot_wf", &[100.0, -2.35, 0.75, -1.2, 1.2, 1.0, 3.0]),
            ("geometricPrimitives", &[0.5, 0.7, 2.0, 1.0, 3.0, 0.5, 0.0, 1.0, 0.5, 0.45, 0.8, 0.9, 1.0, 3.0]),
            ("glitchy2", &[1.0]),
        ];
        for (name, params) in cases {
            let def = find(name);
            let data = def.prepare_data(params, &Affine::IDENTITY);
            for &p in POINTS {
                // glitchy2 mode 1 divides by the input coordinates (Java gives inf there too)
                if *name == "glitchy2" && (p.0 == 0.0 || p.1 == 0.0) {
                    continue;
                }
                for _ in 0..50 {
                    let (_, v) = run(def, &data, &mut ctx, p);
                    assert!(!v.is_bad() && v.color.is_finite(), "{name} {params:?} at {p:?}: {v:?}");
                }
            }
        }
    }

    #[test]
    fn fract_wf_mostly_visible() {
        // Mandelbrot: most samples pass the clip test; buddhabrot mode finds orbits.
        let def = find("fract_mandelbrot_wf");
        let mut ctx = Ctx::new(11, false);
        for bb in [0.0, 1.0] {
            let data = def.prepare_data(&[100.0, -2.35, 0.75, -1.2, 1.2, bb], &Affine::IDENTITY);
            let mut shown = 0;
            for _ in 0..500 {
                let (_, v) = run(def, &data, &mut ctx, (0.0, 0.0, 0.0));
                if !v.do_hide {
                    shown += 1;
                    // the last orbit point (one step past the bailout) may be far out
                    assert!(!v.is_bad(), "{v:?}");
                }
            }
            assert!(shown > 400, "buddhabrot_mode {bb}: {shown}");
        }
    }

    #[test]
    fn julia_iteration_matches_complex_square() {
        let data = find("fract_julia_wf").prepare_data(&[], &Affine::IDENTITY);
        let mut it = FractWfIter::new(0.3, -0.4);
        it.next(&data);
        // z^2 + c with c = (0.35, 0.09)
        assert!((it.x - (0.09 - 0.16 + 0.35)).abs() < 1e-12);
        assert!((it.y - (2.0 * 0.3 * -0.4 + 0.09)).abs() < 1e-12);
    }

    #[test]
    fn gingerbread_man_stays_on_the_attractor() {
        let def = find("gingerbread_man");
        let mut ctx = Ctx::new(9, false);
        for _ in 0..200 {
            let (_, v) = run(def, &[], &mut ctx, (0.0, 0.0, 0.0));
            assert!(v.x.abs() < 0.8 * 9.0 && v.y.abs() < 0.8 * 9.0, "{v:?}");
        }
    }

    #[test]
    fn glitchy1_hash_matches_java() {
        // reference values from a 32-bit two's complement emulation of the Java code
        let m = i32::MAX as f64;
        assert_eq!(glitchy1_hash(1), 663891101.0 / m);
        assert_eq!(glitchy1_hash(-5), 867643996.0 / m);
        assert_eq!(glitchy1_hash(123456), 1792739405.0 / m);
    }
}
