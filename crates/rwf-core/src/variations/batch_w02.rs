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

//! Variations ported from the JWildfire sources (batch w02).

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

// ---------------------------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------------------------

/// `DiscretNoise2` of the eralex circle plugins (`circleLinear`, `circleRand`, `circleTrans1`).
fn circle_discret_noise2(x: i32, y: i32) -> f64 {
    let mut n = x.wrapping_add(y.wrapping_mul(57));
    n = (n << 13) ^ n;
    let t = n
        .wrapping_mul(n.wrapping_mul(n).wrapping_mul(15731).wrapping_add(789221))
        .wrapping_add(1376312589);
    (t & 0x7fff_ffff) as f64 * (1.0 / 2147483647.0)
}

/// Rejection sampling of the unit cube against a signed distance function (the Jesus Sosa
/// `*3D` SDF shapes): 50 tries, hides the point on failure.
fn sdf_sample(ctx: &mut Ctx, v: &mut Pt, amount: f64, sd: impl Fn(f64, f64, f64) -> f64) {
    for _ in 0..50 {
        let x = ctx.random() - 0.5;
        let y = ctx.random() - 0.5;
        let z = ctx.random() - 0.5;
        if sd(x, y, z) < 0.0 {
            v.do_hide = false;
            v.x += amount * x;
            v.y += amount * y;
            v.z += amount * z;
            return;
        }
    }
    v.do_hide = true;
}

fn clamp_params(p: &[f64], ranges: &[(f64, f64)]) -> Vec<f64> {
    p.iter().zip(ranges).map(|(&v, &(lo, hi))| limit_value(v, lo, hi)).collect()
}

// ---------------------------------------------------------------------------------------------
// busybrad
// ---------------------------------------------------------------------------------------------

// d = [mode, gridSize, xOffset, yOffset, spin, twist, space, n, corner, mod_spin_strength,
//      mod_twist_strength, sensen_post_effect, sensen_fold]
fn busybrad_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = p.to_vec();
    for i in [0, 7, 8, 11] {
        d[i] = ftoi(d[i]) as f64;
    }
    let mode = d[0] as i32;
    if (mode == 1 || mode == 2) && d[7] < 2.0 {
        d[7] = 2.0;
    }
    d
}

fn busybrad_fold(t: f64, fold: f64) -> f64 {
    let nr = (t * fold).floor();
    let m = fmod(nr, 2.0);
    if (nr >= 0.0 && m == 1.0) || (nr < 0.0 && m == 0.0) { -t } else { t }
}

fn busybrad_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let mode = d[0] as i32;
    let grid = d[1];
    let (xo, yo, spin, twist, space) = (d[2], d[3], d[4], d[5], d[6]);
    let n = d[7] as i32;
    let ccx = if grid == 0.0 { 0.0 } else { grid * java_round(a.x / grid) };
    let ccy = if grid == 0.0 { 0.0 } else { grid * java_round(a.y / grid) };
    let lx = a.x - ccx - xo;
    let ly = a.y - ccy + yo;
    let rr = (lx * lx + ly * ly).sqrt();
    let th = ly.atan2(lx);
    // Susan "outside" factor (also Jess' new_modulus and the combined modulation value)
    let modv = if rr != 0.0 { amount * (1.0 + space / rr) } else { 0.0 };
    let (mut tx, mut ty);
    if mode == 0 {
        if rr < amount {
            let an = th + spin + twist * (amount - rr);
            let r2 = amount * rr;
            tx = r2 * an.cos();
            ty = r2 * an.sin();
        } else {
            tx = modv * lx;
            ty = modv * ly;
        }
    } else {
        let (sp, tw) = if mode == 1 { (spin, twist) } else { (spin + modv * d[9], twist + modv * d[10]) };
        let nd = if n == 0 { 1.0 } else { n as f64 };
        let r_poly = amount * (M_PI / nd).cos() / (th - M_2PI / nd * (nd * th / M_2PI + 0.5).floor()).cos();
        if rr < r_poly {
            let te = if r_poly > 1e-9 { tw * (r_poly - rr) / r_poly } else { 0.0 };
            let theta = th + sp + te;
            tx = rr * theta.cos();
            ty = rr * theta.sin();
        } else {
            tx = modv * lx;
            ty = modv * ly;
        }
    }
    if d[11] as i32 == 1 && d[12] != 0.0 {
        tx = busybrad_fold(tx, d[12]);
        ty = busybrad_fold(ty, d[12]);
    }
    // JWildfire quirk: the result is not scaled by the amount.
    v.x += ccx + tx - xo;
    v.y += ccy + ty + yo;
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// butterfly_fay
// ---------------------------------------------------------------------------------------------

// d = [offset, unified_inner_outer, outer_mode, inner_mode, outer_spread, inner_spread,
//      outer_spread_ratio, inner_spread_ratio, spread_split, cycles, fill, number_of_cycles]
fn butterfly_fay_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = p.to_vec();
    d[1] = if d[1] == 0.0 { 0.0 } else { 1.0 };
    for i in [2, 3] {
        let m = d[i].floor() as i32;
        d[i] = if (0..=5).contains(&m) { m as f64 } else { 0.0 };
    }
    let cycles_to_close = (2.0 * M_PI * M_PI * M_PI) / (2.0 * M_PI);
    let number_of_cycles = if d[9] == 0.0 { cycles_to_close } else { d[9] };
    d.push(number_of_cycles);
    d
}

/// One spread mode; `sgn3` is +1 for the outer and -1 for the inner variant (they only differ
/// in mode 3).
#[allow(clippy::too_many_arguments)]
fn butterfly_fay_mode(mode: i32, spread: f64, ratio: f64, rin: f64, x: f64, y: f64, ax: f64, ay: f64, sgn3: f64) -> (f64, f64) {
    let xin = if x < 0.0 { -ax.abs() } else { ax.abs() };
    let yin = if y < 0.0 { -ay.abs() } else { ay.abs() };
    match mode {
        1 => {
            let rinx = rin * spread * ratio - spread * ratio + 1.0;
            let riny = rin * spread - spread + 1.0;
            (rinx * x, riny * y)
        }
        2 => (x + spread * ratio * (xin - x), y + spread * (yin - y)),
        3 => (x + sgn3 * spread * ratio * xin, y + sgn3 * spread * yin),
        4 => ((0.5 * rin + spread * ratio) * x, (0.5 * rin + spread) * y),
        5 => (x + spread * ratio * ax, y + spread * ay),
        _ => (x, y),
    }
}

fn butterfly_fay_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let theta = a.y.atan2(a.x);
    let t = d[11] * theta;
    let rin = d[8] * (a.x * a.x + a.y * a.y).sqrt();
    let mut r = 0.5 * (t.cos().exp() - 2.0 * (4.0 * t).cos() - (t / 12.0).sin().abs().powf(5.0) + d[0]);
    if d[10] != 0.0 {
        r += d[10] * (ctx.random() - 0.5);
    }
    let x = r * t.sin();
    let y = -r * t.cos();
    let (px, py) = if rin.abs() > r.abs() || d[1] as i32 == 1 {
        butterfly_fay_mode(d[2] as i32, d[4], d[6], rin, x, y, a.x, a.y, 1.0)
    } else {
        butterfly_fay_mode(d[3] as i32, d[5], d[7], rin, x, y, a.x, a.y, -1.0)
    };
    v.x += amount * px;
    v.y += amount * py;
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// bwrands
// ---------------------------------------------------------------------------------------------

fn bwrands_bytemix(a: i32, b: i32) -> i32 {
    (a & 0x5A5A) ^ (b & 0xA5A5)
}
fn bwrands_bytexim(a: i32, b: i32) -> i32 {
    (a & 0xAAAA) ^ (b & 0x5555)
}
fn bwrands_byteshf(a: i32, b: i32) -> i32 {
    ((a << 8) ^ (b >> 8)) & 0xFFFF
}
fn bwrands_byteprimes(a: i32, b: i32) -> i32 {
    (a.wrapping_mul(857).wrapping_sub(4) & 0xFF00) ^ (b.wrapping_neg().wrapping_mul(977).wrapping_add(8) & 0x00FF)
}

// d = [cellsize, space, gain, inner_twist, outer_twist, seed, rrot, rmin, loonie_chance,
//      petals_chance, minpetals, maxpetals, g2, r2, rfactor, petx, pety]
fn bwrands_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = p.to_vec();
    d[5] = ftoi(d[5]) as f64;
    for x in &mut d[6..=9] {
        *x = limit_value(*x, 0.0, 1.0);
    }
    for i in [10, 11] {
        d[i] = limit_value(ftoi(d[i]), 1, 1000) as f64;
    }
    let radius = 0.5 * (d[0] / (1.0 + d[1] * d[1]));
    let g2 = sqr(d[2]) / radius + 1.0e-6;
    let mut max_bubble = g2 * radius;
    if max_bubble > 2.0 {
        max_bubble = 1.0;
    } else {
        max_bubble *= 1.0 / ((max_bubble * max_bubble) / 4.0 + 1.0);
    }
    let r2 = radius * radius;
    let rfactor = radius / max_bubble;
    let petx = d[10].min(d[11]);
    let pety = (d[11] - d[10]).abs();
    d.extend_from_slice(&[g2, r2, rfactor, petx, pety]);
    d
}

fn bwrands_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let cellsize = d[0];
    let vx = a.x;
    let vy = a.y;
    if cellsize.abs() < SMALL_EPSILON {
        v.x += amount * vx;
        v.y += amount * vy;
        ctx.pass_z(a, v, amount);
        return;
    }
    let ix = (vx / cellsize).floor() as i32;
    let iy = (vy / cellsize).floor() as i32;
    let cx = (ix as f64 + 0.5) * cellsize;
    let cy = (iy as f64 + 0.5) * cellsize;

    let mut xx = ix ^ 0xB641;
    let mut yy = iy ^ 0x9D81;
    let mut xy = xx.wrapping_mul(yy).wrapping_add(d[5] as i32);
    xx &= 0xFFFF;
    yy &= 0xFFFF;
    xy &= 0xFFFF;
    let mut tt = bwrands_bytemix(xx, yy);
    yy = bwrands_bytemix(yy, xx);
    xx = tt;
    tt = bwrands_byteshf(xx, xy);
    yy = bwrands_byteshf(xy, yy);
    xx = tt;
    tt = bwrands_bytexim(xx, yy);
    yy = bwrands_bytexim(yy, xx);
    xx = tt;
    let ssz = d[7] + (xx as f64 / 65535.0) * (1.0 - d[7]);
    let aan = d[6] * M_2PI * (yy as f64) / 65535.0;
    tt = bwrands_byteprimes(xx, yy);
    yy = bwrands_byteprimes(yy, xx);
    xx = tt;
    let mut loonie = -(xx as f64) / 65535.0 + d[8];
    let mut petals = 0.0;
    if loonie < 0.0 {
        petals = loonie + d[9];
    }
    let petx = d[15] as i32;
    let pety = d[16] as i32;
    let npetals = if pety == 0 { petx as f64 } else { (petx + (yy >> 3) % (1 + pety)) as f64 };
    if loonie <= petals {
        loonie = -1.0;
    }

    let mut lx = vx - cx;
    let mut ly = vy - cy;
    let mut vv2 = ssz * d[13];
    if lx * lx + ly * ly > vv2 {
        v.x += amount * vx;
        v.y += amount * vy;
        ctx.pass_z(a, v, amount);
        return;
    }
    if loonie > 0.0 {
        let r = (vv2 / (lx * lx + ly * ly) - 1.0).sqrt();
        lx *= r;
        ly *= r;
        vv2 = 1.0;
    } else if petals > 0.0 {
        let mut flwr = npetals / M_2PI * (M_PI + ly.atan2(lx));
        flwr -= (flwr as i32) as f64;
        flwr = (flwr - 0.5).abs() * 2.0;
        let r = (lx * lx + ly * ly).sqrt();
        if ctx.random() < 0.5 * (flwr + 0.5) {
            let f = (1.0 - r) * (flwr * 1.1);
            lx *= f;
            ly *= f;
        } else {
            vv2 = vv2.sqrt();
            let f = (vv2 - r * (1.0 - flwr)) / (r + SMALL_EPSILON);
            lx *= f;
            ly *= f;
        }
        vv2 = 1.0;
    } else {
        lx *= d[12];
        ly *= d[12];
        let r = d[14] / ((lx * lx + ly * ly) / (4.0 * ssz) + 1.0);
        lx *= r;
        ly *= r;
        vv2 = ssz.sqrt();
    }
    let r = vv2 * (lx * lx + ly * ly) / d[13];
    let theta = d[3] * (1.0 - r) + d[4] * r;
    let (s, c) = (theta + aan).sin_cos();
    v.x += amount * (cx + c * lx + s * ly);
    v.y += amount * (cy - s * lx + c * ly);
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// bwraps7
// ---------------------------------------------------------------------------------------------

// d = [cellsize, space, gain, inner_twist, outer_twist, g2, r2, rfactor]
fn bwraps7_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = p.to_vec();
    let radius = 0.5 * (d[0] / (1.0 + d[1] * d[1]));
    let g2 = d[2] * d[2] + 1.0e-6;
    let mut max_bubble = g2 * radius;
    if max_bubble > 2.0 {
        max_bubble = 1.0;
    } else {
        max_bubble *= 1.0 / ((max_bubble * max_bubble) / 4.0 + 1.0);
    }
    d.extend_from_slice(&[g2, radius * radius, radius / max_bubble]);
    d
}

fn bwraps7_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let cellsize = d[0];
    let vx = a.x;
    let vy = a.y;
    if cellsize.abs() < SMALL_EPSILON {
        v.x += amount * vx;
        v.y += amount * vy;
        ctx.pass_z(a, v, amount);
        return;
    }
    let cx = ((vx / cellsize).floor() + 0.5) * cellsize;
    let cy = ((vy / cellsize).floor() + 0.5) * cellsize;
    let mut lx = vx - cx;
    let mut ly = vy - cy;
    if lx * lx + ly * ly > d[6] {
        v.x += amount * vx;
        v.y += amount * vy;
        ctx.pass_z(a, v, amount);
        return;
    }
    lx *= d[5];
    ly *= d[5];
    let r = d[7] / ((lx * lx + ly * ly) / 4.0 + 1.0);
    lx *= r;
    ly *= r;
    let r = (lx * lx + ly * ly) / d[6];
    let theta = d[3] * (1.0 - r) + d[4] * r;
    let (s, c) = theta.sin_cos();
    v.x += amount * (cx + c * lx + s * ly);
    v.y += amount * (cy - s * lx + c * ly);
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// camouflage
// ---------------------------------------------------------------------------------------------

/// Ken Perlin's permutation table (`CamouflageFunc.p`, which repeats it twice).
const CAMOUFLAGE_PERM: [i32; 256] = [
    151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225, //
    140, 36, 103, 30, 69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148, //
    247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117, 35, 11, 32, //
    57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175, //
    74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122, //
    60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54, //
    65, 25, 63, 161, 1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169, //
    200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186, 3, 64, //
    52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212, //
    207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213, //
    119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9, //
    129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104, //
    218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241, //
    81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157, //
    184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93, //
    222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180,
];

fn camouflage_hash(x: i32, y: i32, z: i32) -> f64 {
    let mut h = x
        .wrapping_mul(374761393)
        .wrapping_add(y.wrapping_mul(668265263))
        .wrapping_add(z.wrapping_mul(1103515245));
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h & 0x7fff_ffff) as f64 / 1073741823.5 - 1.0
}

/// `CamouflageFunc.lerp(a, b, t)` (note: `perlinNoise` calls it with Perlin's `(t, a, b)` order).
fn camouflage_lerp(a: f64, b: f64, t: f64) -> f64 {
    a + t * (b - a)
}

fn camouflage_smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn camouflage_value(x: f64, y: f64, z: f64) -> f64 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let iz = z.floor() as i32;
    let u = camouflage_smooth(x - ix as f64);
    let v = camouflage_smooth(y - iy as f64);
    let w = camouflage_smooth(z - iz as f64);
    let h = |dx: i32, dy: i32, dz: i32| camouflage_hash(ix.wrapping_add(dx), iy.wrapping_add(dy), iz.wrapping_add(dz));
    let nx00 = camouflage_lerp(h(0, 0, 0), h(1, 0, 0), u);
    let nx10 = camouflage_lerp(h(0, 1, 0), h(1, 1, 0), u);
    let nx01 = camouflage_lerp(h(0, 0, 1), h(1, 0, 1), u);
    let nx11 = camouflage_lerp(h(0, 1, 1), h(1, 1, 1), u);
    let nxy0 = camouflage_lerp(nx00, nx10, v);
    let nxy1 = camouflage_lerp(nx01, nx11, v);
    camouflage_lerp(nxy0, nxy1, w)
}

fn camouflage_cellular(x: f64, y: f64, z: f64) -> f64 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let iz = z.floor() as i32;
    let mut min_dist = f64::MAX;
    for dz in -1..=1 {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let cx = ix.wrapping_add(dx);
                let cy = iy.wrapping_add(dy);
                let cz = iz.wrapping_add(dz);
                let px = cx as f64 + 0.5 * (1.0 + camouflage_hash(cx, cy, cz));
                let py = cy as f64 + 0.5 * (1.0 + camouflage_hash(cy, cz, cx));
                let pz = cz as f64 + 0.5 * (1.0 + camouflage_hash(cz, cx, cy));
                let dist = (x - px) * (x - px) + (y - py) * (y - py) + (z - pz) * (z - pz);
                if dist < min_dist {
                    min_dist = dist;
                }
            }
        }
    }
    min_dist.sqrt() * 2.0 - 1.0
}

fn camouflage_fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn camouflage_grad(hash: i32, x: f64, y: f64, z: f64) -> f64 {
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
}

fn camouflage_perlin(x: f64, y: f64, z: f64) -> f64 {
    let p = |i: i32| CAMOUFLAGE_PERM[(i & 255) as usize];
    let xi = (x.floor() as i32) & 255;
    let yi = (y.floor() as i32) & 255;
    let zi = (z.floor() as i32) & 255;
    let x = x - x.floor();
    let y = y - y.floor();
    let z = z - z.floor();
    let u = camouflage_fade(x);
    let v = camouflage_fade(y);
    let w = camouflage_fade(z);
    let a = p(xi) + yi;
    let aa = p(a) + zi;
    let ab = p(a + 1) + zi;
    let b = p(xi + 1) + yi;
    let ba = p(b) + zi;
    let bb = p(b + 1) + zi;
    let g = camouflage_grad;
    let l = camouflage_lerp;
    // JWildfire quirk: Perlin's lerp(t, a, b) argument order fed into lerp(a, b, t).
    let l1 = l(u, g(p(aa), x, y, z), g(p(ba), x - 1.0, y, z));
    let l2 = l(u, g(p(ab), x, y - 1.0, z), g(p(bb), x - 1.0, y - 1.0, z));
    let l3 = l(u, g(p(aa + 1), x, y, z - 1.0), g(p(ba + 1), x - 1.0, y, z - 1.0));
    let l4 = l(u, g(p(ab + 1), x, y - 1.0, z - 1.0), g(p(bb + 1), x - 1.0, y - 1.0, z - 1.0));
    l(w, l(v, l1, l2), l(v, l3, l4))
}

// d = [seed, scale, noiseType, levels, octaves, persistence, lacunarity, stretch, rotation,
//      displacement, zOffset, colorize]
fn camouflage_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = p.to_vec();
    d[2] = (d[2] as i32).clamp(0, 2) as f64;
    d[3] = (d[3] as i32).max(1) as f64;
    d[4] = (d[4] as i32).max(1) as f64;
    d[11] = if d[11] > 0.5 { 1.0 } else { 0.0 };
    d
}

fn camouflage_apply(d: &[f64], _ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let seed = d[0];
    let scale = d[1];
    let x = a.x * scale;
    let y = a.y * scale * d[7];
    let z = a.z * scale;
    let noise_type = d[2] as i32;
    let levels = d[3];
    let mut noise = 0.0;
    let mut freq = 1.0;
    let mut amp = 1.0;
    for _ in 0..d[4] as i32 {
        let n = match noise_type {
            1 => camouflage_perlin(x * freq + seed, y * freq + seed, z * freq + seed),
            2 => camouflage_cellular(x * freq + seed, y * freq + seed, z * freq + seed),
            _ => camouflage_value(x * freq, y * freq, z * freq + seed),
        };
        noise += n * amp;
        freq *= d[6];
        amp *= d[5];
    }
    let q = (noise * levels).floor();
    if d[11] as i32 == 1 {
        let ci = fmod(fmod(q, levels) + levels, levels);
        v.color = ci / levels;
    }
    let angle = q * M_PI * 2.0 * d[8] / levels;
    let mag = q * d[9] / levels;
    let zo = q * d[10] / levels;
    let (sin_a, cos_a) = angle.sin_cos();
    let x_new = a.x * cos_a - a.y * sin_a + mag;
    let y_new = a.x * sin_a + a.y * cos_a + mag;
    let z_new = a.z + zo;
    v.x += (x_new - a.x) * amount;
    v.y += (y_new - a.y) * amount;
    v.z += (z_new - a.z) * amount;
}

// ---------------------------------------------------------------------------------------------
// SDF shapes
// ---------------------------------------------------------------------------------------------

fn cappedcone3d_sd(x: f64, y: f64, z: f64, h: f64, r1: f64, r2: f64) -> f64 {
    let qx = (x * x + z * z).sqrt();
    let qy = y;
    let (k1x, k1y) = (r2, h);
    let (k2x, k2y) = (r2 - r1, 2.0 * h);
    let cax = qx - java_min(qx, if qy < 0.0 { r1 } else { r2 });
    let cay = qy.abs() - h;
    let t = java_min(java_max(((k1x - qx) * k2x + (k1y - qy) * k2y) / (k2x * k2x + k2y * k2y), 0.0), 1.0);
    let cbx = qx - k1x + k2x * t;
    let cby = qy - k1y + k2y * t;
    let s = if cbx < 0.0 && cay < 0.0 { -1.0 } else { 1.0 };
    s * java_min(cax * cax + cay * cay, cbx * cbx + cby * cby).sqrt()
}

#[allow(clippy::too_many_arguments)]
fn cappedtorus3d_sd(x: f64, y: f64, z: f64, scx: f64, scy: f64, ra: f64, rb: f64) -> f64 {
    let px = x.abs();
    let k = if scy * px > scx * y { px * scx + y * scy } else { (px * px + y * y).sqrt() };
    (px * px + y * y + z * z + ra * ra - 2.0 * ra * k).sqrt() - rb
}

#[allow(clippy::too_many_arguments)]
fn capsule3d_sd(x: f64, y: f64, z: f64, ax: f64, ay: f64, az: f64, bx: f64, by: f64, bz: f64, r: f64) -> f64 {
    let (pax, pay, paz) = (x - ax, y - ay, z - az);
    let (bax, bay, baz) = (bx - ax, by - ay, bz - az);
    let h = java_min(java_max((pax * bax + pay * bay + paz * baz) / (bax * bax + bay * bay + baz * baz), 0.0), 1.0);
    let (dx, dy, dz) = (pax - bax * h, pay - bay * h, paz - baz * h);
    (dx * dx + dy * dy + dz * dz).sqrt() - r
}

// ---------------------------------------------------------------------------------------------
// checkerboard_wf
// ---------------------------------------------------------------------------------------------

// d = [position, size, axis, checker_size, displ_amount, checker_color1, checker_color2,
//      side_color, with_sides, side_prob, max_checks]
fn checkerboard_wf_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let mut d = p.to_vec();
    d[2] = limit_value(ftoi(d[2]), 0, 2) as f64;
    for x in &mut d[5..=7] {
        *x = limit_value(*x, 0.0, 1.0);
    }
    d[8] = limit_value(ftoi(d[8]), 0, 1) as f64;
    let side_area = 4.0 * d[4];
    let side_prob = side_area / (1.0 + side_area);
    let mut max_checks = ftoi(1.0 / d[3]);
    if max_checks as f64 * d[3] >= 1.0 {
        max_checks -= 1;
    }
    d.extend_from_slice(&[side_prob, max_checks as f64]);
    d
}

fn checkerboard_wf_apply(d: &[f64], ctx: &mut Ctx, _a: &mut Pt, v: &mut Pt, amount: f64) {
    let displ = d[4];
    let max_checks = d[10] as i32;
    let (x, y, z);
    let side = d[8] as i32 != 0 && displ.abs() >= EPSILON && max_checks > 0 && ctx.random() < d[9];
    if side {
        v.color = d[7];
        if ctx.random() < 0.5 {
            x = ctx.random_int((max_checks + 1) as usize) as f64 * d[3];
            y = ctx.random();
        } else {
            x = ctx.random();
            y = ctx.random_int((max_checks + 1) as usize) as f64 * d[3];
        }
        z = displ * ctx.random();
    } else {
        x = ctx.random();
        y = ctx.random();
        let even = fmod((x / d[3]).floor() + (y / d[3]).floor(), 2.0) < 1.0;
        z = if even { displ } else { 0.0 };
        v.color = limit_value(if even { d[5] } else { d[6] }, 0.0, 1.0);
    }
    let x = (x - 0.5) * d[1];
    let y = (y - 0.5) * d[1];
    let z = z * d[1] + d[0];
    match d[2] as i32 {
        0 => {
            v.x += amount * x;
            v.y += amount * y;
            v.z += amount * z;
        }
        1 => {
            v.y += amount * x;
            v.z += amount * y;
            v.x += amount * z;
        }
        _ => {
            v.z += amount * x;
            v.x += amount * y;
            v.y += amount * z;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// circleLinear / circleRand / circleTrans1
// ---------------------------------------------------------------------------------------------

fn circle_cell_offset(m: i32, sc: f64) -> f64 {
    (m.wrapping_mul(2).wrapping_add(1)) as f64 * sc
}

// d = [Sc, K, Dens1, Dens2, Reverse, Seed]
fn circle_linear_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (sc, k, dens1, dens2, reverse) = (d[0], d[1], d[2], d[3], d[4]);
    let seed = d[5] as i32;
    let m = (0.5 * a.x / sc).floor() as i32;
    let n = (0.5 * a.y / sc).floor() as i32;
    let mut x = a.x - circle_cell_offset(m, sc);
    let mut y = a.y - circle_cell_offset(n, sc);
    let u = (x * x + y * y).sqrt();
    let vv = (0.3 + 0.7 * circle_discret_noise2(m.wrapping_add(10), n.wrapping_add(3))) * sc;
    let z1 = circle_discret_noise2(m.wrapping_add(seed), n);
    if z1 < dens1 && u < vv {
        let near = if reverse > 0.0 { z1 < dens1 * dens2 } else { z1 > dens1 * dens2 };
        if near {
            x *= k;
            y *= k;
        } else {
            let z = vv / u * (1.0 - k) + k;
            x *= z;
            y *= z;
        }
    }
    v.x += amount * (x + circle_cell_offset(m, sc));
    v.y += amount * (y + circle_cell_offset(n, sc));
    ctx.pass_z(a, v, amount);
}

// d = [Sc, Dens, X, Y, Seed]
fn circle_rand_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (sc, dens) = (d[0], d[1]);
    let seed = d[4] as i32;
    let mut iter = 0;
    let (mut x, mut y, mut m, mut n);
    loop {
        x = d[2] * (1.0 - 2.0 * ctx.random());
        y = d[3] * (1.0 - 2.0 * ctx.random());
        m = (0.5 * x / sc).floor() as i32;
        n = (0.5 * y / sc).floor() as i32;
        x -= circle_cell_offset(m, sc);
        y -= circle_cell_offset(n, sc);
        let u = (x * x + y * y).sqrt();
        iter += 1;
        if iter > 100 {
            break;
        }
        let reject = circle_discret_noise2(m.wrapping_add(seed), n) > dens
            || u > (0.3 + 0.7 * circle_discret_noise2(m.wrapping_add(10), n.wrapping_add(3))) * sc;
        if !reject {
            break;
        }
    }
    v.x += amount * (x + circle_cell_offset(m, sc));
    v.y += amount * (y + circle_cell_offset(n, sc));
    ctx.pass_z(a, v, amount);
}

// d = [Sc, Dens, X, Y, Seed]
fn circle_trans1_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (sc, dens) = (d[0], d[1]);
    let seed = d[4] as i32;
    let mut ux = (a.x - d[2]) * 0.5 + d[2];
    let mut uy = (a.y - d[3]) * 0.5 + d[3];
    let m = (0.5 * ux / sc).floor() as i32;
    let n = (0.5 * uy / sc).floor() as i32;
    let x = ux - circle_cell_offset(m, sc);
    let y = uy - circle_cell_offset(n, sc);
    let u = (x * x + y * y).sqrt();
    let outside = circle_discret_noise2(m.wrapping_add(seed), n) > dens
        || u > (0.3 + 0.7 * circle_discret_noise2(m.wrapping_add(10), n.wrapping_add(3))) * sc;
    if !outside {
        // CircleR
        let mut iter = 0;
        let (mut x, mut y, mut m, mut n);
        loop {
            x = d[2].abs() * (1.0 - 2.0 * ctx.random());
            y = d[3].abs() * (1.0 - 2.0 * ctx.random());
            m = (0.5 * x / sc).floor() as i32;
            n = (0.5 * y / sc).floor() as i32;
            let alpha = 2.0 * M_PI * ctx.random();
            let (s, c) = alpha.sin_cos();
            let uu = 0.3 + 0.7 * circle_discret_noise2(m.wrapping_add(10), n.wrapping_add(3));
            x = uu * c;
            y = uu * s;
            iter += 1;
            if iter > 100 || circle_discret_noise2(m.wrapping_add(seed), n) <= dens {
                break;
            }
        }
        ux = x + circle_cell_offset(m, sc);
        uy = y + circle_cell_offset(n, sc);
    }
    v.x += amount * ux;
    v.y += amount * uy;
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// collideoscope
// ---------------------------------------------------------------------------------------------

// d = [a, num, kn_pi, pi_kn, ka_kn]
fn collideoscope_prepare(p: &[f64], _: &Affine) -> Vec<f64> {
    let a = limit_value(p[0], 0.0, 1.0);
    let num = limit_value(ftoi(p[1]), 1, i32::MAX) as f64;
    vec![a, num, num * M_1_PI, M_PI / num, M_PI * a / num]
}

fn collideoscope_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (kn_pi, pi_kn, ka_kn) = (d[2], d[3], d[4]);
    let mut ang = a.y.atan2(a.x);
    let r = amount * (a.x * a.x + a.y * a.y).sqrt();
    if ang >= 0.0 {
        let alt = (ang * kn_pi) as i32;
        if alt % 2 == 0 {
            ang = alt as f64 * pi_kn + fmod(ka_kn + ang, pi_kn);
        } else {
            ang = alt as f64 * pi_kn + fmod(-ka_kn + ang, pi_kn);
        }
    } else {
        let alt = (-ang * kn_pi) as i32;
        if alt % 2 != 0 {
            ang = -(alt as f64 * pi_kn + fmod(-ka_kn - ang, pi_kn));
        } else {
            ang = -(alt as f64 * pi_kn + fmod(ka_kn - ang, pi_kn));
        }
    }
    v.x += r * ang.cos();
    v.y += r * ang.sin();
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// registry
// ---------------------------------------------------------------------------------------------

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "busybrad",
        params: &[
            ParamDef::int("mode", 2.0, 0.0, 2.0),
            ParamDef::new("gridSize", 1.0),
            ParamDef::new("xOffset", 0.0),
            ParamDef::new("yOffset", 0.0),
            ParamDef::new("spin", 0.10),
            ParamDef::new("twist", 0.20),
            ParamDef::new("space", 0.40),
            ParamDef::int("n", 4.0, 2.0, 12.0),
            ParamDef::int("corner", 1.0, 0.0, 12.0),
            ParamDef::new("mod_spin_strength", 0.0),
            ParamDef::new("mod_twist_strength", 0.0),
            ParamDef::int("sensen_post_effect", 0.0, 0.0, 1.0),
            ParamDef::new("sensen_fold", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: busybrad_apply,
        prepare: Some(busybrad_prepare),
        wgsl: "fn busybrad_fold(t: f32, fold: f32) -> f32 {
  let nr = floor(t * fold);
  let m = fmod(nr, 2.0);
  if ((nr >= 0.0 && m == 1.0) || (nr < 0.0 && m == 0.0)) { return -t; }
  return t;
}
@body
let mode = i32(D(0));
let grid = D(1);
var ccx = 0.0;
var ccy = 0.0;
if (grid != 0.0) {
  ccx = grid * java_round(a.x / grid);
  ccy = grid * java_round(a.y / grid);
}
let lx = a.x - ccx - D(2);
let ly = a.y - ccy + D(3);
let rr = sqrt(lx * lx + ly * ly);
let th = atan2(ly, lx);
var modv = 0.0;
if (rr != 0.0) { modv = AMOUNT * (1.0 + D(6) / rr); }
var tx = modv * lx;
var ty = modv * ly;
if (mode == 0) {
  if (rr < AMOUNT) {
    let an = th + D(4) + D(5) * (AMOUNT - rr);
    let r2 = AMOUNT * rr;
    tx = r2 * cos(an);
    ty = r2 * sin(an);
  }
} else {
  var sp = D(4);
  var tw = D(5);
  if (mode != 1) {
    sp += modv * D(9);
    tw += modv * D(10);
  }
  var nd = D(7);
  if (i32(nd) == 0) { nd = 1.0; }
  let r_poly = AMOUNT * cos(PI / nd) / cos(th - TWO_PI / nd * floor(nd * th / TWO_PI + 0.5));
  if (rr < r_poly) {
    var te = 0.0;
    if (r_poly > 1e-9) { te = tw * (r_poly - rr) / r_poly; }
    let theta = th + sp + te;
    tx = rr * cos(theta);
    ty = rr * sin(theta);
  }
}
if (i32(D(11)) == 1 && D(12) != 0.0) {
  tx = busybrad_fold(tx, D(12));
  ty = busybrad_fold(ty, D(12));
}
v.x += ccx + tx - D(2);
v.y += ccy + ty + D(3);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "butterfly3D",
        true,
        |_, _ctx, a, v, amount| {
            let wx = amount * 1.302_940_031_741_119_8;
            let y2 = a.y * 2.0;
            let r = wx * ((a.y * a.x).abs() / (SMALL_EPSILON + a.x * a.x + y2 * y2)).sqrt();
            v.x += r * a.x;
            v.y += r * y2;
            v.z += r * y2.abs() * (a.x * a.x + a.y * a.y).sqrt() / 4.0;
        },
        "let wx = AMOUNT * 1.30294003174112;
let y2 = a.y * 2.0;
let r = wx * sqrt(abs(a.y * a.x) / (EPS + a.x * a.x + y2 * y2));
v.x += r * a.x;
v.y += r * y2;
v.z += r * abs(y2) * sqrt(a.x * a.x + a.y * a.y) / 4.0;",
    ),
    VarDef {
        name: "butterfly_fay",
        params: &[
            ParamDef::new("offset", 0.0),
            ParamDef::int("unified_inner_outer", 1.0, 0.0, 1.0),
            ParamDef::int("outer_mode", 1.0, 0.0, 5.0),
            ParamDef::int("inner_mode", 1.0, 0.0, 5.0),
            ParamDef::new("outer_spread", 0.0),
            ParamDef::new("inner_spread", 0.0),
            ParamDef::new("outer_spread_ratio", 1.0),
            ParamDef::new("inner_spread_ratio", 1.0),
            ParamDef::new("spread_split", 1.0),
            ParamDef::range("cycles", 0.0, 0.0, 20.0),
            ParamDef::new("fill", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: butterfly_fay_apply,
        prepare: Some(butterfly_fay_prepare),
        wgsl: "fn butterfly_fay_mode(mode: i32, spread: f32, ratio: f32, rin: f32, x: f32, y: f32, ax: f32, ay: f32, sgn3: f32) -> vec2<f32> {
  var xin = abs(ax);
  var yin = abs(ay);
  if (x < 0.0) { xin = -xin; }
  if (y < 0.0) { yin = -yin; }
  switch (mode) {
    case 1: {
      let rinx = rin * spread * ratio - spread * ratio + 1.0;
      let riny = rin * spread - spread + 1.0;
      return vec2<f32>(rinx * x, riny * y);
    }
    case 2: { return vec2<f32>(x + spread * ratio * (xin - x), y + spread * (yin - y)); }
    case 3: { return vec2<f32>(x + sgn3 * spread * ratio * xin, y + sgn3 * spread * yin); }
    case 4: { return vec2<f32>((0.5 * rin + spread * ratio) * x, (0.5 * rin + spread) * y); }
    case 5: { return vec2<f32>(x + spread * ratio * ax, y + spread * ay); }
    default: { return vec2<f32>(x, y); }
  }
}
@body
let t = D(11) * atan2(a.y, a.x);
let rin = D(8) * sqrt(a.x * a.x + a.y * a.y);
var r = 0.5 * (exp(cos(t)) - 2.0 * cos(4.0 * t) - pow(abs(sin(t / 12.0)), 5.0) + D(0));
if (D(10) != 0.0) { r += D(10) * (rnd() - 0.5); }
let x = r * sin(t);
let y = -r * cos(t);
var p = vec2<f32>(0.0);
if (abs(rin) > abs(r) || i32(D(1)) == 1) {
  p = butterfly_fay_mode(i32(D(2)), D(4), D(6), rin, x, y, a.x, a.y, 1.0);
} else {
  p = butterfly_fay_mode(i32(D(3)), D(5), D(7), rin, x, y, a.x, a.y, -1.0);
}
v.x += AMOUNT * p.x;
v.y += AMOUNT * p.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bwrands",
        params: &[
            ParamDef::new("cellsize", 1.0),
            ParamDef::new("space", 0.0),
            ParamDef::new("gain", 1.25),
            ParamDef::new("inner_twist", 0.0),
            ParamDef::new("outer_twist", 0.0),
            ParamDef::int("seed", 3210.0, 0.0, 100000.0),
            ParamDef::range("rrot", 1.0, 0.0, 1.0),
            ParamDef::range("rmin", 0.25, 0.0, 1.0),
            ParamDef::range("loonie_chance", 0.50, 0.0, 1.0),
            ParamDef::range("petals_chance", 0.50, 0.0, 1.0),
            ParamDef::int("minpetals", 3.0, 1.0, 1000.0),
            ParamDef::int("maxpetals", 20.0, 1.0, 1000.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: bwrands_apply,
        prepare: Some(bwrands_prepare),
        wgsl: "fn bwrands_bytemix(a: i32, b: i32) -> i32 { return (a & 0x5A5A) ^ (b & 0xA5A5); }
fn bwrands_bytexim(a: i32, b: i32) -> i32 { return (a & 0xAAAA) ^ (b & 0x5555); }
fn bwrands_byteshf(a: i32, b: i32) -> i32 { return ((a << 8u) ^ (b >> 8u)) & 0xFFFF; }
fn bwrands_byteprimes(a: i32, b: i32) -> i32 { return ((a * 857 - 4) & 0xFF00) ^ ((-b * 977 + 8) & 0x00FF); }
@body
let cellsize = D(0);
if (abs(cellsize) < EPS) {
  v.x += AMOUNT * a.x;
  v.y += AMOUNT * a.y;
} else {
  let ix = i32(floor(a.x / cellsize));
  let iy = i32(floor(a.y / cellsize));
  let cx = (f32(ix) + 0.5) * cellsize;
  let cy = (f32(iy) + 0.5) * cellsize;
  var xx = ix ^ 0xB641;
  var yy = iy ^ 0x9D81;
  var xy = xx * yy + i32(D(5));
  xx = xx & 0xFFFF;
  yy = yy & 0xFFFF;
  xy = xy & 0xFFFF;
  var tt = bwrands_bytemix(xx, yy);
  yy = bwrands_bytemix(yy, xx);
  xx = tt;
  tt = bwrands_byteshf(xx, xy);
  yy = bwrands_byteshf(xy, yy);
  xx = tt;
  tt = bwrands_bytexim(xx, yy);
  yy = bwrands_bytexim(yy, xx);
  xx = tt;
  let ssz = D(7) + (f32(xx) / 65535.0) * (1.0 - D(7));
  let aan = D(6) * TWO_PI * f32(yy) / 65535.0;
  tt = bwrands_byteprimes(xx, yy);
  yy = bwrands_byteprimes(yy, xx);
  xx = tt;
  var loonie = -f32(xx) / 65535.0 + D(8);
  var petals = 0.0;
  if (loonie < 0.0) { petals = loonie + D(9); }
  let petx = i32(D(15));
  let pety = i32(D(16));
  var npetals = f32(petx);
  if (pety != 0) { npetals = f32(petx + (yy >> 3u) % (1 + pety)); }
  if (loonie <= petals) { loonie = -1.0; }
  var lx = a.x - cx;
  var ly = a.y - cy;
  var vv2 = ssz * D(13);
  if (lx * lx + ly * ly > vv2) {
    v.x += AMOUNT * a.x;
    v.y += AMOUNT * a.y;
  } else {
    if (loonie > 0.0) {
      let r = sqrt(vv2 / (lx * lx + ly * ly) - 1.0);
      lx *= r;
      ly *= r;
      vv2 = 1.0;
    } else if (petals > 0.0) {
      var flwr = npetals / TWO_PI * (PI + atan2(ly, lx));
      flwr = flwr - trunc(flwr);
      flwr = abs(flwr - 0.5) * 2.0;
      let r = sqrt(lx * lx + ly * ly);
      if (rnd() < 0.5 * (flwr + 0.5)) {
        let f = (1.0 - r) * (flwr * 1.1);
        lx *= f;
        ly *= f;
      } else {
        vv2 = sqrt(vv2);
        let f = (vv2 - r * (1.0 - flwr)) / (r + EPS);
        lx *= f;
        ly *= f;
      }
      vv2 = 1.0;
    } else {
      lx *= D(12);
      ly *= D(12);
      let r = D(14) / ((lx * lx + ly * ly) / (4.0 * ssz) + 1.0);
      lx *= r;
      ly *= r;
      vv2 = sqrt(ssz);
    }
    let r = vv2 * (lx * lx + ly * ly) / D(13);
    let theta = D(3) * (1.0 - r) + D(4) * r + aan;
    let s = sin(theta);
    let c = cos(theta);
    v.x += AMOUNT * (cx + c * lx + s * ly);
    v.y += AMOUNT * (cy - s * lx + c * ly);
  }
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "bwraps7",
        params: &[
            ParamDef::new("cellsize", 1.0),
            ParamDef::new("space", 0.0),
            ParamDef::new("gain", 2.0),
            ParamDef::new("inner_twist", 0.0),
            ParamDef::new("outer_twist", 0.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: bwraps7_apply,
        prepare: Some(bwraps7_prepare),
        wgsl: "let cellsize = D(0);
var nx = a.x;
var ny = a.y;
if (abs(cellsize) >= EPS) {
  let cx = (floor(a.x / cellsize) + 0.5) * cellsize;
  let cy = (floor(a.y / cellsize) + 0.5) * cellsize;
  var lx = a.x - cx;
  var ly = a.y - cy;
  if (lx * lx + ly * ly <= D(6)) {
    lx *= D(5);
    ly *= D(5);
    let r0 = D(7) / ((lx * lx + ly * ly) / 4.0 + 1.0);
    lx *= r0;
    ly *= r0;
    let r = (lx * lx + ly * ly) / D(6);
    let theta = D(3) * (1.0 - r) + D(4) * r;
    let s = sin(theta);
    let c = cos(theta);
    nx = cx + c * lx + s * ly;
    ny = cy - s * lx + c * ly;
  }
}
v.x += AMOUNT * nx;
v.y += AMOUNT * ny;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "calcination",
        params: &[ParamDef::new("freq", 4.0), ParamDef::new("intensity", 0.3), ParamDef::new("decay", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let decay = java_max(d[2].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let heat = d[1] * (d[0] * r).sin() * (-decay * r).exp();
            let new_r = java_max(r - heat, 0.0);
            v.x += amount * new_r * theta.cos();
            v.y += amount * new_r * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let decay = max(abs(D(2)), 0.01);
let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
let heat = D(1) * sin(D(0) * r) * exp(-decay * r);
let new_r = max(r - heat, 0.0);
v.x += AMOUNT * new_r * cos(theta);
v.y += AMOUNT * new_r * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cam",
        params: &[ParamDef::new("k1", 0.5), ParamDef::new("k2", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r2 = a.x * a.x + a.y * a.y;
            let f = 1.0 + d[0] * r2 + d[1] * r2 * r2;
            v.x += amount * a.x * f;
            v.y += amount * a.y * f;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r2 = a.x * a.x + a.y * a.y;
let f = 1.0 + D(0) * r2 + D(1) * r2 * r2;
v.x += AMOUNT * a.x * f;
v.y += AMOUNT * a.y * f;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "camouflage",
        params: &[
            ParamDef::range("seed", 1000.0, 100.0, 2500.0),
            ParamDef::range("scale", 2.0, 0.5, 5.0),
            ParamDef::int("noiseType", 0.0, 0.0, 2.0),
            ParamDef::int("levels", 4.0, 1.0, 10.0),
            ParamDef::int("octaves", 5.0, 1.0, 10.0),
            ParamDef::range("persistence", 0.5, 0.0, 1.0),
            ParamDef::range("lacunarity", 2.0, 1.0, 3.0),
            ParamDef::new("stretch", 1.0),
            ParamDef::new("rotation", 0.5),
            ParamDef::new("displacement", 0.1),
            ParamDef::new("zOffset", 0.1),
            ParamDef::int("colorize", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: camouflage_apply,
        prepare: Some(camouflage_prepare),
        wgsl: "const camouflage_P: array<i32, 256> = array<i32, 256>(
151, 160, 137, 91, 90, 15, 131, 13, 201, 95, 96, 53, 194, 233, 7, 225,
140, 36, 103, 30, 69, 142, 8, 99, 37, 240, 21, 10, 23, 190, 6, 148,
247, 120, 234, 75, 0, 26, 197, 62, 94, 252, 219, 203, 117, 35, 11, 32,
57, 177, 33, 88, 237, 149, 56, 87, 174, 20, 125, 136, 171, 168, 68, 175,
74, 165, 71, 134, 139, 48, 27, 166, 77, 146, 158, 231, 83, 111, 229, 122,
60, 211, 133, 230, 220, 105, 92, 41, 55, 46, 245, 40, 244, 102, 143, 54,
65, 25, 63, 161, 1, 216, 80, 73, 209, 76, 132, 187, 208, 89, 18, 169,
200, 196, 135, 130, 116, 188, 159, 86, 164, 100, 109, 198, 173, 186, 3, 64,
52, 217, 226, 250, 124, 123, 5, 202, 38, 147, 118, 126, 255, 82, 85, 212,
207, 206, 59, 227, 47, 16, 58, 17, 182, 189, 28, 42, 223, 183, 170, 213,
119, 248, 152, 2, 44, 154, 163, 70, 221, 153, 101, 155, 167, 43, 172, 9,
129, 22, 39, 253, 19, 98, 108, 110, 79, 113, 224, 232, 178, 185, 112, 104,
218, 246, 97, 228, 251, 34, 242, 193, 238, 210, 144, 12, 191, 179, 162, 241,
81, 51, 145, 235, 249, 14, 239, 107, 49, 192, 214, 31, 181, 199, 106, 157,
184, 84, 204, 176, 115, 121, 50, 45, 127, 4, 150, 254, 138, 236, 205, 93,
222, 114, 67, 29, 24, 72, 243, 141, 128, 195, 78, 66, 215, 61, 156, 180);
fn camouflage_hash(x: i32, y: i32, z: i32) -> f32 {
  var h = x * 374761393 + y * 668265263 + z * 1103515245;
  h = (h ^ (h >> 13u)) * 1274126177;
  h = h ^ (h >> 16u);
  return f32(h & 0x7fffffff) / 1073741823.5 - 1.0;
}
fn camouflage_lerp(a: f32, b: f32, t: f32) -> f32 { return a + t * (b - a); }
fn camouflage_smooth(t: f32) -> f32 { return t * t * (3.0 - 2.0 * t); }
fn camouflage_value(x: f32, y: f32, z: f32) -> f32 {
  let ix = i32(floor(x));
  let iy = i32(floor(y));
  let iz = i32(floor(z));
  let u = camouflage_smooth(x - f32(ix));
  let v = camouflage_smooth(y - f32(iy));
  let w = camouflage_smooth(z - f32(iz));
  let nx00 = camouflage_lerp(camouflage_hash(ix, iy, iz), camouflage_hash(ix + 1, iy, iz), u);
  let nx10 = camouflage_lerp(camouflage_hash(ix, iy + 1, iz), camouflage_hash(ix + 1, iy + 1, iz), u);
  let nx01 = camouflage_lerp(camouflage_hash(ix, iy, iz + 1), camouflage_hash(ix + 1, iy, iz + 1), u);
  let nx11 = camouflage_lerp(camouflage_hash(ix, iy + 1, iz + 1), camouflage_hash(ix + 1, iy + 1, iz + 1), u);
  return camouflage_lerp(camouflage_lerp(nx00, nx10, v), camouflage_lerp(nx01, nx11, v), w);
}
fn camouflage_cellular(x: f32, y: f32, z: f32) -> f32 {
  let ix = i32(floor(x));
  let iy = i32(floor(y));
  let iz = i32(floor(z));
  var min_dist = 3.0e38;
  for (var dz = -1; dz <= 1; dz++) {
    for (var dy = -1; dy <= 1; dy++) {
      for (var dx = -1; dx <= 1; dx++) {
        let cx = ix + dx;
        let cy = iy + dy;
        let cz = iz + dz;
        let px = f32(cx) + 0.5 * (1.0 + camouflage_hash(cx, cy, cz));
        let py = f32(cy) + 0.5 * (1.0 + camouflage_hash(cy, cz, cx));
        let pz = f32(cz) + 0.5 * (1.0 + camouflage_hash(cz, cx, cy));
        let dist = (x - px) * (x - px) + (y - py) * (y - py) + (z - pz) * (z - pz);
        if (dist < min_dist) { min_dist = dist; }
      }
    }
  }
  return sqrt(min_dist) * 2.0 - 1.0;
}
fn camouflage_fade(t: f32) -> f32 { return t * t * t * (t * (t * 6.0 - 15.0) + 10.0); }
fn camouflage_grad(hash: i32, x: f32, y: f32, z: f32) -> f32 {
  let h = hash & 15;
  let u = select(y, x, h < 8);
  var v = z;
  if (h < 4) { v = y; } else if (h == 12 || h == 14) { v = x; }
  return select(-u, u, (h & 1) == 0) + select(-v, v, (h & 2) == 0);
}
fn camouflage_p(i: i32) -> i32 { return camouflage_P[i & 255]; }
fn camouflage_perlin(x0: f32, y0: f32, z0: f32) -> f32 {
  let xi = i32(floor(x0)) & 255;
  let yi = i32(floor(y0)) & 255;
  let zi = i32(floor(z0)) & 255;
  let x = x0 - floor(x0);
  let y = y0 - floor(y0);
  let z = z0 - floor(z0);
  let u = camouflage_fade(x);
  let v = camouflage_fade(y);
  let w = camouflage_fade(z);
  let a = camouflage_p(xi) + yi;
  let aa = camouflage_p(a) + zi;
  let ab = camouflage_p(a + 1) + zi;
  let b = camouflage_p(xi + 1) + yi;
  let ba = camouflage_p(b) + zi;
  let bb = camouflage_p(b + 1) + zi;
  let l1 = camouflage_lerp(u, camouflage_grad(camouflage_p(aa), x, y, z), camouflage_grad(camouflage_p(ba), x - 1.0, y, z));
  let l2 = camouflage_lerp(u, camouflage_grad(camouflage_p(ab), x, y - 1.0, z), camouflage_grad(camouflage_p(bb), x - 1.0, y - 1.0, z));
  let l3 = camouflage_lerp(u, camouflage_grad(camouflage_p(aa + 1), x, y, z - 1.0), camouflage_grad(camouflage_p(ba + 1), x - 1.0, y, z - 1.0));
  let l4 = camouflage_lerp(u, camouflage_grad(camouflage_p(ab + 1), x, y - 1.0, z - 1.0), camouflage_grad(camouflage_p(bb + 1), x - 1.0, y - 1.0, z - 1.0));
  return camouflage_lerp(w, camouflage_lerp(v, l1, l2), camouflage_lerp(v, l3, l4));
}
@body
let seed = D(0);
let x = a.x * D(1);
let y = a.y * D(1) * D(7);
let z = a.z * D(1);
let noise_type = i32(D(2));
let levels = D(3);
let octaves = i32(D(4));
var noise = 0.0;
var freq = 1.0;
var amp = 1.0;
for (var i = 0; i < octaves; i++) {
  var n = 0.0;
  if (noise_type == 1) {
    n = camouflage_perlin(x * freq + seed, y * freq + seed, z * freq + seed);
  } else if (noise_type == 2) {
    n = camouflage_cellular(x * freq + seed, y * freq + seed, z * freq + seed);
  } else {
    n = camouflage_value(x * freq, y * freq, z * freq + seed);
  }
  noise += n * amp;
  freq *= D(6);
  amp *= D(5);
}
let q = floor(noise * levels);
if (i32(D(11)) == 1) {
  v.color = fmod(fmod(q, levels) + levels, levels) / levels;
}
let angle = q * PI * 2.0 * D(8) / levels;
let mag = q * D(9) / levels;
let zo = q * D(10) / levels;
let sa = sin(angle);
let ca = cos(angle);
v.x += (a.x * ca - a.y * sa + mag - a.x) * AMOUNT;
v.y += (a.x * sa + a.y * ca + mag - a.y) * AMOUNT;
v.z += zo * AMOUNT;",
    },
    VarDef {
        name: "cantor",
        params: &[ParamDef::new("freq", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let freq = d[0];
            let mut fx = fmod(a.x * freq, 3.0);
            if fx < 0.0 {
                fx += 3.0;
            }
            let mut fy = fmod(a.y * freq, 3.0);
            if fy < 0.0 {
                fy += 3.0;
            }
            if fx > 1.0 && fx < 2.0 {
                fx = if fx < 1.5 { 1.0 } else { 2.0 };
            }
            if fy > 1.0 && fy < 2.0 {
                fy = if fy < 1.5 { 1.0 } else { 2.0 };
            }
            v.x += amount * fx / freq;
            v.y += amount * fy / freq;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let freq = D(0);
var fx = fmod(a.x * freq, 3.0);
if (fx < 0.0) { fx += 3.0; }
var fy = fmod(a.y * freq, 3.0);
if (fy < 0.0) { fy += 3.0; }
if (fx > 1.0 && fx < 2.0) { fx = select(2.0, 1.0, fx < 1.5); }
if (fy > 1.0 && fy < 2.0) { fy = select(2.0, 1.0, fy < 1.5); }
v.x += AMOUNT * fx / freq;
v.y += AMOUNT * fy / freq;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cappedcone3D",
        params: &[ParamDef::range("h", 0.5, -1.0, 1.0), ParamDef::range("r1", 0.10, -1.0, 1.0), ParamDef::range("r2", 0.30, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            sdf_sample(ctx, v, amount, |x, y, z| cappedcone3d_sd(x, y, z, d[0], d[1], d[2]));
        },
        prepare: Some(|p, _| clamp_params(p, &[(-1.0, 1.0); 3])),
        wgsl: "fn cappedcone3D_sd(x: f32, y: f32, z: f32, h: f32, r1: f32, r2: f32) -> f32 {
  let q = vec2<f32>(sqrt(x * x + z * z), y);
  let k1 = vec2<f32>(r2, h);
  let k2 = vec2<f32>(r2 - r1, 2.0 * h);
  let ca = vec2<f32>(q.x - min(q.x, select(r2, r1, q.y < 0.0)), abs(q.y) - h);
  let cb = q - k1 + k2 * clamp(dot(k1 - q, k2) / dot(k2, k2), 0.0, 1.0);
  let s = select(1.0, -1.0, cb.x < 0.0 && ca.y < 0.0);
  return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}
@body
v.hide = 1.0;
for (var i = 0; i < 50; i++) {
  let x = rnd() - 0.5;
  let y = rnd() - 0.5;
  let z = rnd() - 0.5;
  if (cappedcone3D_sd(x, y, z, D(0), D(1), D(2)) < 0.0) {
    v.x += AMOUNT * x;
    v.y += AMOUNT * y;
    v.z += AMOUNT * z;
    v.hide = 0.0;
    break;
  }
}",
    },
    VarDef {
        name: "cappedtorus3D",
        params: &[
            ParamDef::range("dx", 0.866025, 0.0, 1.0),
            ParamDef::range("dy", -0.5, -5.0, 5.0),
            ParamDef::range("R1", 0.25, 0.0, 0.5),
            ParamDef::range("R2", 0.15, 0.0, 0.5),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            sdf_sample(ctx, v, amount, |x, y, z| cappedtorus3d_sd(x, y, z, d[0], d[1], d[2], d[3]));
        },
        prepare: Some(|p, _| clamp_params(p, &[(0.0, 1.0), (-5.0, 5.0), (0.0, 0.5), (0.0, 0.5)])),
        wgsl: "fn cappedtorus3D_sd(x: f32, y: f32, z: f32, scx: f32, scy: f32, ra: f32, rb: f32) -> f32 {
  let px = abs(x);
  var k = sqrt(px * px + y * y);
  if (scy * px > scx * y) { k = px * scx + y * scy; }
  return sqrt(px * px + y * y + z * z + ra * ra - 2.0 * ra * k) - rb;
}
@body
v.hide = 1.0;
for (var i = 0; i < 50; i++) {
  let x = rnd() - 0.5;
  let y = rnd() - 0.5;
  let z = rnd() - 0.5;
  if (cappedtorus3D_sd(x, y, z, D(0), D(1), D(2), D(3)) < 0.0) {
    v.x += AMOUNT * x;
    v.y += AMOUNT * y;
    v.z += AMOUNT * z;
    v.hide = 0.0;
    break;
  }
}",
    },
    VarDef {
        name: "capsule3D",
        params: &[
            ParamDef::range("dx", 0.1, -1.0, 1.0),
            ParamDef::range("dy", 0.0, -1.0, 1.0),
            ParamDef::range("dz", 0.0, -1.0, 1.0),
            ParamDef::range("tx", 0.0, -1.0, 1.0),
            ParamDef::range("ty", 0.0, -1.0, 1.0),
            ParamDef::range("tz", 0.0, -1.0, 1.0),
            ParamDef::range("R", 0.05, -1.0, 1.0),
        ],
        kind: VarKind::Normal,
        // JWildfire marks it VARTYPE_BASE_SHAPE (not VARTYPE_3D), although it writes z.
        is_3d: false,
        apply: |d, ctx, _a, v, amount| {
            sdf_sample(ctx, v, amount, |x, y, z| capsule3d_sd(x, y, z, d[0], d[1], d[2], d[3], d[4], d[5], d[6]));
        },
        prepare: Some(|p, _| clamp_params(p, &[(-1.0, 1.0); 7])),
        wgsl: "fn capsule3D_sd(p: vec3<f32>, a: vec3<f32>, b: vec3<f32>, r: f32) -> f32 {
  let pa = p - a;
  let ba = b - a;
  let h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
  return length(pa - ba * h) - r;
}
@body
v.hide = 1.0;
for (var i = 0; i < 50; i++) {
  let x = rnd() - 0.5;
  let y = rnd() - 0.5;
  let z = rnd() - 0.5;
  if (capsule3D_sd(vec3<f32>(x, y, z), vec3<f32>(D(0), D(1), D(2)), vec3<f32>(D(3), D(4), D(5)), D(6)) < 0.0) {
    v.x += AMOUNT * x;
    v.y += AMOUNT * y;
    v.z += AMOUNT * z;
    v.hide = 0.0;
    break;
  }
}",
    },
    VarDef {
        name: "cardioid",
        params: &[ParamDef::new("a", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let ang = a.y.atan2(a.x);
            let r = amount * (sqr(a.x) + sqr(a.y) + (ang * d[0]).sin() + 1.0).sqrt();
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ang = atan2(a.y, a.x);
let r = AMOUNT * sqrt(a.x * a.x + a.y * a.y + sin(ang * D(0)) + 1.0);
v.x += r * cos(ang);
v.y += r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "catapult",
        params: &[ParamDef::new("height", 0.5), ParamDef::new("range", 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let range = java_max(d[1].abs(), 0.01);
            let t = a.x / range;
            let arc = d[0] * (1.0 - t * t);
            v.x += amount * a.x;
            v.y += amount * (a.y + arc);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let range = max(abs(D(1)), 0.01);
let t = a.x / range;
let arc = D(0) * (1.0 - t * t);
v.x += AMOUNT * a.x;
v.y += AMOUNT * (a.y + arc);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "catenary",
        params: &[ParamDef::new("a", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let ca = java_max(d[0].abs(), 0.001);
            v.x += amount * a.x;
            v.y += amount * ca * (a.x / ca).cosh();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ca = max(abs(D(0)), 0.001);
v.x += AMOUNT * a.x;
v.y += AMOUNT * ca * cosh(a.x / ca);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "caustic",
        params: &[ParamDef::new("freq", 5.0), ParamDef::new("strength", 0.3)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let new_r = r + d[1] * (r * d[0]).sin();
            v.x += amount * new_r * theta.cos();
            v.y += amount * new_r * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
let theta = atan2(a.y, a.x);
let new_r = r + D(1) * sin(r * D(0));
v.x += AMOUNT * new_r * cos(theta);
v.y += AMOUNT * new_r * sin(theta);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cell2",
        params: &[
            ParamDef::new("size", 0.60),
            ParamDef::int("mirror_x", 0.0, 0.0, 1.0),
            ParamDef::int("mirror_y", 0.0, 0.0, 1.0),
            ParamDef::new("a", 1.0),
            ParamDef::new("space_ya", 2.0),
            ParamDef::new("space_xa", 2.0),
            ParamDef::new("space_yb", 2.0),
            ParamDef::new("space_xb", 2.0),
            ParamDef::new("move_xa", 1.0),
            ParamDef::new("space_yc", 2.0),
            ParamDef::new("move_ya", 1.0),
            ParamDef::new("space_xc", 2.0),
            ParamDef::new("space_yd", 2.0),
            ParamDef::new("move_yb", 1.0),
            ParamDef::new("space_xd", 2.0),
            ParamDef::new("move_xb", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [size, mirror_x, mirror_y, a, space_ya, space_xa, space_yb, space_xb, move_xa,
        //      space_yc, move_ya, space_xc, space_yd, move_yb, space_xd, move_xb]
        apply: |d, ctx, a, v, amount| {
            let size = d[0];
            let inv = d[3] / size;
            let mut x = (a.x * inv).floor() as i32 as f64;
            let mut y = (a.y * inv).floor() as i32 as f64;
            let dx = a.x - x * size;
            let dy = a.y - y * size;
            if y >= 0.0 {
                if x >= 0.0 {
                    y *= d[4];
                    x *= d[5];
                } else {
                    y *= d[6];
                    x = -(d[7] * x + d[8]);
                }
            } else if x >= 0.0 {
                y = -(d[9] * y + d[10]);
                x *= d[11];
            } else {
                y = -(d[12] * y + d[13]);
                x = -(d[14] * x + d[15]);
            }
            v.x += amount * (dx + x * size);
            v.y -= amount * (dy + y * size);
            if d[1] as i32 > 0 && ctx.random() < 0.5 {
                v.x = -v.x;
            }
            if d[2] as i32 > 0 && ctx.random() < 0.5 {
                v.y = -v.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[1] = ftoi(d[1]) as f64;
            d[2] = ftoi(d[2]) as f64;
            d
        }),
        wgsl: "let size = D(0);
let inv = D(3) / size;
var x = f32(i32(floor(a.x * inv)));
var y = f32(i32(floor(a.y * inv)));
let dx = a.x - x * size;
let dy = a.y - y * size;
if (y >= 0.0) {
  if (x >= 0.0) {
    y *= D(4);
    x *= D(5);
  } else {
    y *= D(6);
    x = -(D(7) * x + D(8));
  }
} else {
  if (x >= 0.0) {
    y = -(D(9) * y + D(10));
    x *= D(11);
  } else {
    y = -(D(12) * y + D(13));
    x = -(D(14) * x + D(15));
  }
}
v.x += AMOUNT * (dx + x * size);
v.y -= AMOUNT * (dy + y * size);
if (i32(D(1)) > 0 && rnd() < 0.5) { v.x = -v.x; }
if (i32(D(2)) > 0 && rnd() < 0.5) { v.y = -v.y; }
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "chainmail",
        params: &[ParamDef::new("scale", 3.0), ParamDef::new("ring_ratio", 0.35)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let cx = a.x * d[0];
            let cy = a.y * d[0];
            let row = cy.floor();
            let mut row_mod = fmod(row, 2.0);
            if row_mod < 0.0 {
                row_mod += 2.0;
            }
            let offset = if row_mod < 0.5 { 0.5 } else { 0.0 };
            let lx = (cx + offset) - (cx + offset).floor() - 0.5;
            let ly = cy - cy.floor() - 0.5;
            let r = (lx * lx + ly * ly).sqrt();
            let s = if r < d[1] { d[1] / (r + 1e-6) } else { 1.0 };
            v.x += amount * (lx * s) / d[0];
            v.y += amount * (ly * s) / d[0];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let cx = a.x * D(0);
let cy = a.y * D(0);
let row = floor(cy);
var row_mod = fmod(row, 2.0);
if (row_mod < 0.0) { row_mod += 2.0; }
let offset = select(0.0, 0.5, row_mod < 0.5);
let lx = (cx + offset) - floor(cx + offset) - 0.5;
let ly = cy - floor(cy) - 0.5;
let r = sqrt(lx * lx + ly * ly);
var s = 1.0;
if (r < D(1)) { s = D(1) / (r + 1e-6); }
v.x += AMOUNT * (lx * s) / D(0);
v.y += AMOUNT * (ly * s) / D(0);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "checkerboard_wf",
        params: &[
            ParamDef::new("position", 3.0),
            ParamDef::range("size", 10.0, 1.0, 11.0),
            ParamDef::int("axis", 2.0, 0.0, 2.0),
            // random default in JWildfire (0.1 + random * 0.2)
            ParamDef::range("checker_size", 0.2, 0.02, 0.5),
            ParamDef::range("displ_amount", 0.05, 0.0, 0.2),
            // random defaults in JWildfire (random * 0.5, random * 0.5 + 0.5, random * 0.75)
            ParamDef::range("checker_color1", 0.25, 0.0, 1.0),
            ParamDef::range("checker_color2", 0.75, 0.0, 1.0),
            ParamDef::range("side_color", 0.375, 0.0, 1.0),
            ParamDef::int("with_sides", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: checkerboard_wf_apply,
        prepare: Some(checkerboard_wf_prepare),
        wgsl: "let displ = D(4);
let max_checks = i32(D(10));
var x = 0.0;
var y = 0.0;
var z = 0.0;
var side = false;
if (i32(D(8)) != 0 && abs(displ) >= EPSILON && max_checks > 0) {
  side = rnd() < D(9);
}
if (side) {
  v.color = D(7);
  if (rnd() < 0.5) {
    x = floor(rnd() * f32(max_checks + 1)) * D(3);
    y = rnd();
  } else {
    x = rnd();
    y = floor(rnd() * f32(max_checks + 1)) * D(3);
  }
  z = displ * rnd();
} else {
  x = rnd();
  y = rnd();
  let even = fmod(floor(x / D(3)) + floor(y / D(3)), 2.0) < 1.0;
  z = select(0.0, displ, even);
  v.color = clamp(select(D(6), D(5), even), 0.0, 1.0);
}
x = (x - 0.5) * D(1);
y = (y - 0.5) * D(1);
z = z * D(1) + D(0);
let axis = i32(D(2));
if (axis == 0) {
  v.x += AMOUNT * x;
  v.y += AMOUNT * y;
  v.z += AMOUNT * z;
} else if (axis == 1) {
  v.y += AMOUNT * x;
  v.z += AMOUNT * y;
  v.x += AMOUNT * z;
} else {
  v.z += AMOUNT * x;
  v.x += AMOUNT * y;
  v.y += AMOUNT * z;
}",
    },
    VarDef {
        name: "checks",
        params: &[ParamDef::new("x", 3.0), ParamDef::new("y", 3.0), ParamDef::new("size", 1.0), ParamDef::new("rnd", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [x, y, size, rnd, cs, ncx, ncy]
        apply: |d, ctx, a, v, amount| {
            let is_xy = (rint(a.x * d[4]) as i32).wrapping_add(rint(a.y * d[4]) as i32);
            let rnx = d[3] * ctx.random();
            let rny = d[3] * ctx.random();
            let (dx, dy) = if is_xy % 2 == 0 { (d[5] + rnx, d[6]) } else { (d[0], d[1] + rny) };
            v.x += amount * (a.x + dx);
            v.y += amount * (a.y + dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d.extend_from_slice(&[1.0 / (p[2] + EPSILON), -p[0], -p[1]]);
            d
        }),
        wgsl: "let is_xy = i32(round(a.x * D(4))) + i32(round(a.y * D(4)));
let rnx = D(3) * rnd();
let rny = D(3) * rnd();
var dx = D(0);
var dy = D(1) + rny;
if (is_xy % 2 == 0) {
  dx = D(5) + rnx;
  dy = D(6);
}
v.x += AMOUNT * (a.x + dx);
v.y += AMOUNT * (a.y + dy);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "chladni",
        params: &[ParamDef::range("m", 2.0, -5.0, 5.0), ParamDef::range("n", 3.0, -5.0, 5.0), ParamDef::range("amplitude", 0.25, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let dx = d[2] * (d[0] * M_PI * a.x).cos() * (d[1] * M_PI * a.y).sin();
            let dy = d[2] * (d[0] * M_PI * a.x).sin() * (d[1] * M_PI * a.y).cos();
            v.x += amount * (a.x + dx);
            v.y += amount * (a.y + dy);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dx = D(2) * cos(D(0) * PI * a.x) * sin(D(1) * PI * a.y);
let dy = D(2) * sin(D(0) * PI * a.x) * cos(D(1) * PI * a.y);
v.x += AMOUNT * (a.x + dx);
v.y += AMOUNT * (a.y + dy);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "chrysanthemum",
        false,
        |_, ctx, a, v, amount| {
            let u = 21.0 * M_PI * ctx.random();
            let p4 = (17.0 * u / 3.0).sin();
            let p8 = (2.0 * (3.0 * u).cos() - 28.0 * u).sin();
            let p4_2 = p4 * p4;
            let p8_2 = p8 * p8;
            let p8_4 = p8_2 * p8_2;
            let mut r = 5.0 * (1.0 + (11.0 * u / 5.0).sin()) - 4.0 * p4_2 * p4_2 * p8_4 * p8_4;
            r *= amount * 0.1;
            v.x += r * u.cos();
            v.y += r * u.sin();
            ctx.pass_z(a, v, amount);
        },
        "let u = 21.0 * PI * rnd();
let p4 = sin(17.0 * u / 3.0);
let p8 = sin(2.0 * cos(3.0 * u) - 28.0 * u);
let p4_2 = p4 * p4;
let p8_2 = p8 * p8;
let p8_4 = p8_2 * p8_2;
let r = (5.0 * (1.0 + sin(11.0 * u / 5.0)) - 4.0 * p4_2 * p4_2 * p8_4 * p8_4) * AMOUNT * 0.1;
v.x += r * cos(u);
v.y += r * sin(u);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "chunk",
        params: &[
            ParamDef::new("a", 1.0),
            ParamDef::new("b", 0.0),
            ParamDef::new("c", 1.0),
            ParamDef::new("d", 0.0),
            ParamDef::new("e", 0.0),
            ParamDef::new("f", -1.0),
            ParamDef::int("mode", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // JWildfire quirk: the point is added without the amount.
        apply: |d, ctx, a, v, amount| {
            let r = amount * d[0] * sqr(a.x) + amount * d[1] * a.x * a.y + amount * d[2] * sqr(a.y)
                + amount * d[3] * a.x
                + amount * d[4] * a.y
                + amount * d[5];
            let mode = d[6] as i32;
            if (mode == 0 && r <= 0.0) || (mode == 1 && r > 0.0) {
                v.x += a.x;
                v.y += a.y;
                ctx.pass_z(a, v, amount);
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[6] = limit_value(ftoi(d[6]), 0, 1) as f64;
            d
        }),
        wgsl: "let r = AMOUNT * D(0) * a.x * a.x + AMOUNT * D(1) * a.x * a.y + AMOUNT * D(2) * a.y * a.y
  + AMOUNT * D(3) * a.x + AMOUNT * D(4) * a.y + AMOUNT * D(5);
let mode = i32(D(6));
if ((mode == 0 && r <= 0.0) || (mode == 1 && r > 0.0)) {
  v.x += a.x;
  v.y += a.y;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
    },
    simple(
        "circleblur",
        false,
        |_, ctx, a, v, amount| {
            let rad = ctx.random().sqrt();
            let ang = ctx.random() * M_2PI;
            v.x += amount * ang.cos() * rad;
            v.y += amount * ang.sin() * rad;
            ctx.pass_z(a, v, amount);
        },
        "let rad = sqrt(rnd());
let ang = rnd() * TWO_PI;
v.x += AMOUNT * cos(ang) * rad;
v.y += AMOUNT * sin(ang) * rad;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "circlecrop",
        params: &[
            ParamDef::new("radius", 1.0),
            ParamDef::new("x", 0.0),
            ParamDef::new("y", 0.0),
            ParamDef::range("scatter_area", 0.0, -1.0, 1.0),
            ParamDef::int("zero", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [radius, x, y, scatter_area, zero, cA]
        apply: |d, ctx, a, v, amount| {
            let (cr, x0, y0) = (d[0], d[1], d[2]);
            a.x -= x0;
            a.y -= y0;
            let rad = (a.x * a.x + a.y * a.y).sqrt();
            let ang = a.y.atan2(a.x);
            let rdc = cr + ctx.random() * 0.5 * d[5];
            let esc = rad > cr;
            let cr0 = d[4] as i32 == 1;
            v.do_hide = false;
            if cr0 && esc {
                v.x = 0.0;
                v.y = 0.0;
                v.do_hide = true;
            } else if !cr0 && esc {
                v.x += amount * rdc * ang.cos() + x0;
                v.y += amount * rdc * ang.sin() + y0;
            } else {
                v.x += amount * a.x + x0;
                v.y += amount * a.y + y0;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = ftoi(d[4]) as f64;
            d.push(java_max(-1.0, java_min(p[3], 1.0)));
            d
        }),
        wgsl: "let cr = D(0);
let x0 = D(1);
let y0 = D(2);
a.x -= x0;
a.y -= y0;
let rad = sqrt(a.x * a.x + a.y * a.y);
let ang = atan2(a.y, a.x);
let rdc = cr + rnd() * 0.5 * D(5);
let esc = rad > cr;
let cr0 = i32(D(4)) == 1;
v.hide = 0.0;
if (cr0 && esc) {
  v.x = 0.0;
  v.y = 0.0;
  v.hide = 1.0;
} else if (!cr0 && esc) {
  v.x += AMOUNT * rdc * cos(ang) + x0;
  v.y += AMOUNT * rdc * sin(ang) + y0;
} else {
  v.x += AMOUNT * a.x + x0;
  v.y += AMOUNT * a.y + y0;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "circleLinear",
        params: &[
            ParamDef::range("Sc", 1.0, 0.01, 1.0),
            ParamDef::new("K", 0.5),
            ParamDef::range("Dens1", 0.5, 0.1, 2.0),
            ParamDef::range("Dens2", 0.5, 0.0, 1.0),
            ParamDef::int("Reverse", 1.0, 0.0, 1.0),
            ParamDef::int("Seed", 0.0, 0.0, 1000000.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: circle_linear_apply,
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[5] = ftoi(d[5]) as f64;
            d
        }),
        wgsl: "fn circleLinear_noise(x: i32, y: i32) -> f32 {
  var n = x + y * 57;
  n = (n << 13u) ^ n;
  return f32((n * (n * n * 15731 + 789221) + 1376312589) & 0x7fffffff) / 2147483647.0;
}
@body
let sc = D(0);
let k = D(1);
let m = i32(floor(0.5 * a.x / sc));
let n = i32(floor(0.5 * a.y / sc));
let ox = f32(m * 2 + 1) * sc;
let oy = f32(n * 2 + 1) * sc;
var x = a.x - ox;
var y = a.y - oy;
let u = sqrt(x * x + y * y);
let vv = (0.3 + 0.7 * circleLinear_noise(m + 10, n + 3)) * sc;
let z1 = circleLinear_noise(m + i32(D(5)), n);
if (z1 < D(2) && u < vv) {
  var near = z1 > D(2) * D(3);
  if (D(4) > 0.0) { near = z1 < D(2) * D(3); }
  if (near) {
    x *= k;
    y *= k;
  } else {
    let z = vv / u * (1.0 - k) + k;
    x *= z;
    y *= z;
  }
}
v.x += AMOUNT * (x + ox);
v.y += AMOUNT * (y + oy);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "circleRand",
        params: &[
            ParamDef::range("Sc", 1.0, 0.1, 1.6),
            ParamDef::range("Dens", 0.5, 0.1, 1.0),
            ParamDef::range("X", 10.0, 1.0, 10.0),
            ParamDef::range("Y", 10.0, 1.0, 10.0),
            ParamDef::int("Seed", 0.0, 0.0, 1000000.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: circle_rand_apply,
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = ftoi(d[4]) as f64;
            d
        }),
        wgsl: "fn circleRand_noise(x: i32, y: i32) -> f32 {
  var n = x + y * 57;
  n = (n << 13u) ^ n;
  return f32((n * (n * n * 15731 + 789221) + 1376312589) & 0x7fffffff) / 2147483647.0;
}
@body
let sc = D(0);
let seed = i32(D(4));
var x = 0.0;
var y = 0.0;
var m = 0;
var n = 0;
var iter = 0;
loop {
  x = D(2) * (1.0 - 2.0 * rnd());
  y = D(3) * (1.0 - 2.0 * rnd());
  m = i32(floor(0.5 * x / sc));
  n = i32(floor(0.5 * y / sc));
  x -= f32(m * 2 + 1) * sc;
  y -= f32(n * 2 + 1) * sc;
  let u = sqrt(x * x + y * y);
  iter += 1;
  if (iter > 100) { break; }
  if (!(circleRand_noise(m + seed, n) > D(1) || u > (0.3 + 0.7 * circleRand_noise(m + 10, n + 3)) * sc)) { break; }
}
v.x += AMOUNT * (x + f32(m * 2 + 1) * sc);
v.y += AMOUNT * (y + f32(n * 2 + 1) * sc);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "circlesplit",
        params: &[ParamDef::new("cs_radius", 1.0), ParamDef::new("cs_split", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (x0, y0) = (a.x, a.y);
            let r = (sqr(x0) + sqr(y0)).sqrt();
            let (x1, y1) = if r < d[0] - d[1] {
                (x0, y0)
            } else {
                let ang = y0.atan2(x0);
                let len = r + d[1];
                (ang.cos() * len, ang.sin() * len)
            };
            v.x += amount * x1;
            v.y += amount * y1;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
var x1 = a.x;
var y1 = a.y;
if (!(r < D(0) - D(1))) {
  let ang = atan2(a.y, a.x);
  let len = r + D(1);
  x1 = cos(ang) * len;
  y1 = sin(ang) * len;
}
v.x += AMOUNT * x1;
v.y += AMOUNT * y1;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "circleTrans1",
        params: &[
            ParamDef::new("Sc", 1.0),
            ParamDef::new("Dens", 0.5),
            ParamDef::new("X", 10.0),
            ParamDef::new("Y", 10.0),
            ParamDef::int("Seed", 0.0, 0.0, 1000000.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: circle_trans1_apply,
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[4] = ftoi(d[4]) as f64;
            d
        }),
        wgsl: "fn circleTrans1_noise(x: i32, y: i32) -> f32 {
  var n = x + y * 57;
  n = (n << 13u) ^ n;
  return f32((n * (n * n * 15731 + 789221) + 1376312589) & 0x7fffffff) / 2147483647.0;
}
@body
let sc = D(0);
let seed = i32(D(4));
var ux = (a.x - D(2)) * 0.5 + D(2);
var uy = (a.y - D(3)) * 0.5 + D(3);
let m0 = i32(floor(0.5 * ux / sc));
let n0 = i32(floor(0.5 * uy / sc));
let x0 = ux - f32(m0 * 2 + 1) * sc;
let y0 = uy - f32(n0 * 2 + 1) * sc;
let u0 = sqrt(x0 * x0 + y0 * y0);
if (!(circleTrans1_noise(m0 + seed, n0) > D(1) || u0 > (0.3 + 0.7 * circleTrans1_noise(m0 + 10, n0 + 3)) * sc)) {
  var x = 0.0;
  var y = 0.0;
  var m = 0;
  var n = 0;
  var iter = 0;
  loop {
    x = abs(D(2)) * (1.0 - 2.0 * rnd());
    y = abs(D(3)) * (1.0 - 2.0 * rnd());
    m = i32(floor(0.5 * x / sc));
    n = i32(floor(0.5 * y / sc));
    let alpha = TWO_PI * rnd();
    let uu = 0.3 + 0.7 * circleTrans1_noise(m + 10, n + 3);
    x = uu * cos(alpha);
    y = uu * sin(alpha);
    iter += 1;
    if (iter > 100 || circleTrans1_noise(m + seed, n) <= D(1)) { break; }
  }
  ux = x + f32(m * 2 + 1) * sc;
  uy = y + f32(n * 2 + 1) * sc;
}
v.x += AMOUNT * ux;
v.y += AMOUNT * uy;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "circlize2",
        params: &[ParamDef::new("hole", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let absx = a.x.abs();
            let absy = a.y.abs();
            let (side, perimeter);
            if absx >= absy {
                perimeter = if a.x >= absy { absx + a.y } else { 5.0 * absx - a.y };
                side = absx;
            } else {
                perimeter = if a.y >= absx { 3.0 * absy - a.x } else { 7.0 * absy + a.x };
                side = absy;
            }
            let r = amount * (side + d[0]);
            let ang = M_PI_4 * perimeter / side - M_PI_4;
            v.x += r * ang.cos();
            v.y += r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let absx = abs(a.x);
let absy = abs(a.y);
var side = absy;
var perimeter = 0.0;
if (absx >= absy) {
  perimeter = select(5.0 * absx - a.y, absx + a.y, a.x >= absy);
  side = absx;
} else {
  perimeter = select(7.0 * absy + a.x, 3.0 * absy - a.x, a.y >= absx);
}
let r = AMOUNT * (side + D(0));
let ang = 0.785398163397448 * perimeter / side - 0.785398163397448;
v.x += r * cos(ang);
v.y += r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "circular2",
        params: &[
            ParamDef::range("angle", 90.0, -90.0, 90.0),
            ParamDef::new("seed", 0.0),
            ParamDef::range("xx", 12.9898, -100.0, 100.0),
            ParamDef::range("yy", 78.233, -100.0, 100.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let c_a = d[0] * M_PI / 180.0;
            let aux = (a.x * d[2] + a.y * d[3] + d[1]).sin() * 43758.5453;
            let aux = aux - aux.trunc();
            let rnd = (2.0 * (ctx.random() + aux) - 2.0) * c_a;
            let rad = (sqr(a.x) + sqr(a.y)).sqrt();
            let ang = a.y.atan2(a.x) + rnd;
            v.x += amount * (ang.cos() * rad);
            v.y += amount * (ang.sin() * rad);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let c_a = D(0) * PI / 180.0;
let aux0 = sin(a.x * D(2) + a.y * D(3) + D(1)) * 43758.5453;
let aux = aux0 - trunc(aux0);
let rn = (2.0 * (rnd() + aux) - 2.0) * c_a;
let rad = sqrt(a.x * a.x + a.y * a.y);
let ang = atan2(a.y, a.x) + rn;
v.x += AMOUNT * (cos(ang) * rad);
v.y += AMOUNT * (sin(ang) * rad);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "circular",
        params: &[ParamDef::range("angle", 90.0, -90.0, 90.0), ParamDef::new("seed", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let c_a = d[0] * M_PI / 180.0;
            let aux = (a.x * 12.9898 + a.y * 78.233 + d[1]).sin() * 43758.5453;
            let aux = aux - aux.trunc();
            let rnd = (2.0 * (ctx.random() + aux) - 2.0) * c_a;
            let rad = (sqr(a.x) + sqr(a.y)).sqrt();
            let ang = a.y.atan2(a.x) + rnd;
            v.x += amount * (ang.cos() * rad);
            v.y += amount * (ang.sin() * rad);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let c_a = D(0) * PI / 180.0;
let aux0 = sin(a.x * 12.9898 + a.y * 78.233 + D(1)) * 43758.5453;
let aux = aux0 - trunc(aux0);
let rn = (2.0 * (rnd() + aux) - 2.0) * c_a;
let rad = sqrt(a.x * a.x + a.y * a.y);
let ang = atan2(a.y, a.x) + rn;
v.x += AMOUNT * (cos(ang) * rad);
v.y += AMOUNT * (sin(ang) * rad);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "circus",
        params: &[ParamDef::new("scale", 0.92)],
        kind: VarKind::Normal,
        is_3d: false,
        // d = [scale, 1 / scale]
        apply: |d, ctx, a, v, amount| {
            let mut r = (a.x * a.x + a.y * a.y).sqrt();
            let ang = a.y.atan2(a.x);
            if r <= 1.0 {
                r *= d[0];
            } else {
                r *= d[1];
            }
            v.x += amount * r * ang.cos();
            v.y += amount * r * ang.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], 1.0 / p[0]]),
        wgsl: "var r = sqrt(a.x * a.x + a.y * a.y);
let ang = atan2(a.y, a.x);
r *= select(D(1), D(0), r <= 1.0);
v.x += AMOUNT * r * cos(ang);
v.y += AMOUNT * r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cirrus",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("amp", 0.3), ParamDef::new("angle", 0.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let (sa, ca) = d[2].sin_cos();
            let u = a.x * ca + a.y * sa;
            let w = -a.x * sa + a.y * ca;
            let wisp = d[1] * ((d[0] * u).sin() + 0.5 * (d[0] * 2.3 * u + 0.7).sin());
            let nv = w + wisp;
            v.x += amount * (u * ca - nv * sa);
            v.y += amount * (u * sa + nv * ca);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ca = cos(D(2));
let sa = sin(D(2));
let u = a.x * ca + a.y * sa;
let w = -a.x * sa + a.y * ca;
let wisp = D(1) * (sin(D(0) * u) + 0.5 * sin(D(0) * 2.3 * u + 0.7));
let nv = w + wisp;
v.x += AMOUNT * (u * ca - nv * sa);
v.y += AMOUNT * (u * sa + nv * ca);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "clifford_js",
        params: &[ParamDef::new("a", -1.4), ParamDef::new("b", 1.6), ParamDef::new("c", 1.0), ParamDef::new("d", 0.7)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let x = (d[0] * a.y).sin() + d[2] * (d[0] * a.x).cos();
            let y = (d[1] * a.x).sin() + d[3] * (d[1] * a.y).cos();
            v.x += x * amount;
            v.y += y * amount;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let x = sin(D(0) * a.y) + D(2) * cos(D(0) * a.x);
let y = sin(D(1) * a.x) + D(3) * cos(D(1) * a.y);
v.x += x * AMOUNT;
v.y += y * AMOUNT;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cloverleaf_wf",
        params: &[ParamDef::range("filled", 0.85, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let ang = a.atan_xy();
            let mut r = (2.0 * ang).sin() + 0.25 * (6.0 * ang).sin();
            if d[0] > 0.0 && d[0] > ctx.random() {
                r *= ctx.random();
            }
            v.x += amount * ang.sin() * r;
            v.y += amount * ang.cos() * r;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ang = atan_xy(a);
var r = sin(2.0 * ang) + 0.25 * sin(6.0 * ang);
if (D(0) > 0.0 && D(0) > rnd()) { r *= rnd(); }
v.x += AMOUNT * sin(ang) * r;
v.y += AMOUNT * cos(ang) * r;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "coagulation",
        params: &[ParamDef::new("freq", 3.0), ParamDef::new("pull", 0.5)],
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
        name: "collideoscope",
        params: &[ParamDef::range("a", 0.20, 0.0, 1.0), ParamDef::int("num", 1.0, 1.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: collideoscope_apply,
        prepare: Some(collideoscope_prepare),
        wgsl: "let kn_pi = D(2);
let pi_kn = D(3);
let ka_kn = D(4);
var ang = atan2(a.y, a.x);
let r = AMOUNT * sqrt(a.x * a.x + a.y * a.y);
if (ang >= 0.0) {
  let alt = i32(ang * kn_pi);
  if (alt % 2 == 0) {
    ang = f32(alt) * pi_kn + fmod(ka_kn + ang, pi_kn);
  } else {
    ang = f32(alt) * pi_kn + fmod(-ka_kn + ang, pi_kn);
  }
} else {
  let alt = i32(-ang * kn_pi);
  if (alt % 2 != 0) {
    ang = -(f32(alt) * pi_kn + fmod(-ka_kn - ang, pi_kn));
  } else {
    ang = -(f32(alt) * pi_kn + fmod(ka_kn - ang, pi_kn));
  }
}
v.x += r * cos(ang);
v.y += r * sin(ang);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "collision",
        params: &[ParamDef::new("radius", 0.3), ParamDef::new("force", 0.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let radius = java_max(d[0].abs(), 0.01);
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let safe = java_max(r, 0.0001);
            let f = d[1] * (-r * r / (radius * radius)).exp();
            v.x += amount * (a.x + f * a.x / safe);
            v.y += amount * (a.y + f * a.y / safe);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let radius = max(abs(D(0)), 0.01);
let r = sqrt(a.x * a.x + a.y * a.y);
let safe = max(r, 0.0001);
let f = D(1) * exp(-r * r / (radius * radius));
v.x += AMOUNT * (a.x + f * a.x / safe);
v.y += AMOUNT * (a.y + f * a.y / safe);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "colorscale_wf",
        params: &[
            ParamDef::new("scale_x", 0.0),
            ParamDef::new("scale_y", 0.0),
            ParamDef::new("scale_z", 0.5),
            ParamDef::new("offset_z", 0.0),
            ParamDef::int("reset_z", 0.0, 0.0, 1.0),
            ParamDef::int("sides", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * d[0] * a.x;
            v.y += amount * d[1] * a.y;
            let dz = a.color * d[2] * amount + d[3];
            if d[4] > 0.0 {
                v.z = dz;
            } else if d[5] > 0.0 {
                v.z += dz * ctx.random();
            } else {
                v.z += dz;
            }
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * D(0) * a.x;
v.y += AMOUNT * D(1) * a.y;
let dz = a.color * D(2) * AMOUNT + D(3);
if (D(4) > 0.0) {
  v.z = dz;
} else if (D(5) > 0.0) {
  v.z += dz * rnd();
} else {
  v.z += dz;
}",
    },
    VarDef {
        name: "color_wheel",
        params: &[ParamDef::range("sectors", 6.0, 2.0, 20.0), ParamDef::range("speed", 0.3, 0.0, 2.33)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y + 1.0e-10).sqrt();
            let theta = a.y.atan2(a.x);
            let nt = theta + d[1] * (d[0] * theta).sin();
            v.x += amount * r * nt.cos();
            v.y += amount * r * nt.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y + 1.0e-10);
let theta = atan2(a.y, a.x);
let nt = theta + D(1) * sin(D(0) * theta);
v.x += AMOUNT * r * cos(nt);
v.y += AMOUNT * r * sin(nt);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];

#[cfg(test)]
mod tests {
    use super::super::{Ctx, Pt};
    use super::*;
    use crate::model::Affine;

    const NAMES: &[&str] = &[
        "busybrad",
        "butterfly3D",
        "butterfly_fay",
        "bwrands",
        "bwraps7",
        "calcination",
        "cam",
        "camouflage",
        "cantor",
        "cappedcone3D",
        "cappedtorus3D",
        "capsule3D",
        "cardioid",
        "catapult",
        "catenary",
        "caustic",
        "cell2",
        "chainmail",
        "checkerboard_wf",
        "checks",
        "chladni",
        "chrysanthemum",
        "chunk",
        "circleblur",
        "circlecrop",
        "circleLinear",
        "circleRand",
        "circlesplit",
        "circleTrans1",
        "circlize2",
        "circular2",
        "circular",
        "circus",
        "cirrus",
        "clifford_js",
        "cloverleaf_wf",
        "coagulation",
        "collideoscope",
        "collision",
        "colorscale_wf",
        "color_wheel",
    ];

    const POINTS: &[(f64, f64, f64)] = &[
        (0.3, 0.2, 0.1),
        (-0.7, 0.5, -0.2),
        (1.3, -0.9, 0.4),
        (0.05, -0.02, 0.0),
        (-2.1, -1.7, 1.1),
        (0.6, 0.0, -0.5),
    ];

    fn run(def: &VarDef, data: &[f64], ctx: &mut Ctx, p: (f64, f64, f64)) -> (Pt, Pt) {
        let mut a = Pt {
            x: p.0,
            y: p.1,
            z: p.2,
            color: 0.5,
            ..Default::default()
        };
        let mut v = Pt {
            x: 0.1,
            y: -0.2,
            z: 0.05,
            color: 0.5,
            ..Default::default()
        };
        (def.apply)(data, ctx, &mut a, &mut v, 0.8);
        (a, v)
    }

    #[test]
    fn all_names_present_and_unique() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert_eq!(DEFS.iter().filter(|d| d.name == *n).count(), 1, "{n}");
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
    fn non_default_modes_are_finite() {
        let cases: &[(&str, &[f64])] = &[
            ("busybrad", &[0.0, 1.0, 0.1, 0.2, 0.1, 0.2, 0.4, 5.0, 1.0, 0.3, 0.2, 1.0, 2.0]),
            ("busybrad", &[1.0, 0.5, 0.0, 0.0, 0.1, 0.2, 0.4, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
            ("butterfly_fay", &[0.2, 0.0, 3.0, 2.0, 0.5, 0.3, 1.0, 1.0, 1.0, 2.0, 0.3]),
            ("butterfly_fay", &[0.0, 1.0, 4.0, 1.0, 0.5, 0.3, 1.5, 1.0, 1.0, 0.0, 0.0]),
            ("bwrands", &[0.5, 0.1, 1.25, 0.3, 0.5, 77.0, 1.0, 0.25, 0.1, 0.9, 3.0, 20.0]),
            ("bwrands", &[0.5, 0.1, 1.25, 0.3, 0.5, 5.0, 1.0, 0.25, 0.9, 0.1, 5.0, 5.0]),
            ("camouflage", &[1000.0, 2.0, 1.0, 4.0, 5.0, 0.5, 2.0, 1.0, 0.5, 0.1, 0.1, 1.0]),
            ("camouflage", &[500.0, 3.0, 2.0, 6.0, 3.0, 0.5, 2.0, 1.5, 0.5, 0.1, 0.1, 1.0]),
            ("checkerboard_wf", &[3.0, 10.0, 0.0, 0.15, 0.1, 0.2, 0.8, 0.5, 1.0]),
            ("checkerboard_wf", &[3.0, 10.0, 1.0, 0.15, 0.0, 0.2, 0.8, 0.5, 0.0]),
            ("chunk", &[1.0, 0.0, 1.0, 0.0, 0.0, -1.0, 1.0]),
            ("circlecrop", &[0.5, 0.1, 0.1, 0.5, 0.0]),
            ("circleLinear", &[0.3, 0.5, 0.9, 0.5, 0.0, 17.0]),
            ("circleRand", &[0.3, 0.9, 3.0, 3.0, 11.0]),
            ("circleTrans1", &[0.3, 0.9, 0.0, 0.0, 3.0]),
            ("collideoscope", &[0.4, 5.0]),
            ("colorscale_wf", &[0.5, 0.5, 0.5, 0.1, 1.0, 1.0]),
        ];
        let mut ctx = Ctx::new(5, false);
        for (name, params) in cases {
            let def = DEFS.iter().find(|d| d.name == *name).unwrap();
            let data = def.prepare_data(params, &Affine::IDENTITY);
            for &p in POINTS {
                for _ in 0..50 {
                    let (a, v) = run(def, &data, &mut ctx, p);
                    assert!(!a.is_bad() && !v.is_bad() && v.color.is_finite(), "{name} {params:?} at {p:?}: v={v:?}");
                }
            }
        }
    }

    #[test]
    fn camouflage_wgsl_table_matches() {
        let def = DEFS.iter().find(|d| d.name == "camouflage").unwrap();
        let joined: String = CAMOUFLAGE_PERM.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(",");
        let compact: String = def.wgsl.chars().filter(|c| !c.is_whitespace()).collect();
        assert!(compact.contains(&joined));
    }

    #[test]
    fn discret_noise_in_unit_range() {
        for i in -50..50 {
            for j in -50..50 {
                let n = circle_discret_noise2(i * 7919, j * 104729);
                assert!((0.0..=1.0).contains(&n), "{n}");
            }
        }
    }

    #[test]
    fn sdf_shapes_hit_inside() {
        // a point inside each default shape gives a negative distance
        assert!(cappedcone3d_sd(0.0, 0.0, 0.0, 0.5, 0.1, 0.3) < 0.0);
        assert!(capsule3d_sd(0.05, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.05) < 0.0);
        assert!(cappedtorus3d_sd(0.25, 0.0, 0.0, 0.866025, -0.5, 0.25, 0.15) < 0.0);
    }
}
