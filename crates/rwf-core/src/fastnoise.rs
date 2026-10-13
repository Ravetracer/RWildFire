// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// Ported from FastNoise (Java) by Jordan Peck, as bundled with JWildfire
// Copyright (C) 2017 Jordan Peck (MIT License)
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

//! The 3D noise functions of FastNoise (Java, single precision), used by the xform weighting
//! fields. The arithmetic follows the Java code operation by operation in `f32`.

#![allow(clippy::excessive_precision, clippy::unreadable_literal)]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseType {
    Value,
    ValueFractal,
    Perlin,
    PerlinFractal,
    Simplex,
    SimplexFractal,
    Cellular,
    WhiteNoise,
    Cubic,
    CubicFractal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FractalType {
    Fbm,
    Billow,
    RigidMulti,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellularDistance {
    Euclidean,
    Manhattan,
    Natural,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellularReturn {
    CellValue,
    Distance,
    Distance2,
    Distance2Add,
    Distance2Sub,
    Distance2Mul,
    Distance2Div,
}

/// FastNoise configuration (interpolation is always quintic, as in JWildfire).
#[derive(Clone, Debug)]
pub struct FastNoise {
    pub seed: i32,
    pub frequency: f32,
    pub noise_type: NoiseType,
    pub octaves: i32,
    pub lacunarity: f32,
    pub gain: f32,
    pub fractal_type: FractalType,
    pub cellular_distance: CellularDistance,
    pub cellular_return: CellularReturn,
}

impl Default for FastNoise {
    fn default() -> Self {
        Self {
            seed: 1337,
            frequency: 0.01,
            noise_type: NoiseType::Simplex,
            octaves: 3,
            lacunarity: 2.0,
            gain: 0.5,
            fractal_type: FractalType::Fbm,
            cellular_distance: CellularDistance::Euclidean,
            cellular_return: CellularReturn::CellValue,
        }
    }
}

const X_PRIME: i32 = 1619;
const Y_PRIME: i32 = 31337;
const Z_PRIME: i32 = 6971;

#[inline]
fn fast_floor(f: f32) -> i32 {
    if f >= 0.0 { f as i32 } else { f as i32 - 1 }
}

#[inline]
fn fast_round(f: f32) -> i32 {
    if f >= 0.0 { (f + 0.5) as i32 } else { (f - 0.5) as i32 }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + t * (b - a)
}

#[inline]
fn interp_quintic(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[inline]
fn cubic_lerp(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
    let p = (d - c) - (a - b);
    t * t * t * p + t * t * ((a - b) - p) + t * (c - a) + b
}

#[inline]
fn hash3(seed: i32, x: i32, y: i32, z: i32) -> i32 {
    let mut hash = seed;
    hash ^= X_PRIME.wrapping_mul(x);
    hash ^= Y_PRIME.wrapping_mul(y);
    hash ^= Z_PRIME.wrapping_mul(z);
    hash = hash.wrapping_mul(hash).wrapping_mul(hash).wrapping_mul(60493);
    (hash >> 13) ^ hash
}

#[inline]
fn val_coord3(seed: i32, x: i32, y: i32, z: i32) -> f32 {
    let mut n = seed;
    n ^= X_PRIME.wrapping_mul(x);
    n ^= Y_PRIME.wrapping_mul(y);
    n ^= Z_PRIME.wrapping_mul(z);
    n.wrapping_mul(n).wrapping_mul(n).wrapping_mul(60493) as f32 / 2147483648.0f32
}

#[inline]
fn grad_coord3(seed: i32, x: i32, y: i32, z: i32, xd: f32, yd: f32, zd: f32) -> f32 {
    let g = GRAD_3D[(hash3(seed, x, y, z) & 15) as usize];
    xd * g[0] + yd * g[1] + zd * g[2]
}

/// `Float.floatToRawIntBits(f)` folded like `FloatCast2Int`.
#[inline]
fn float_cast2int(f: f32) -> i32 {
    let i = f.to_bits() as i32;
    i ^ (i >> 16)
}

const F3: f32 = (1.0 / 3.0) as f32;
const G3: f32 = (1.0 / 6.0) as f32;
const G33: f32 = G3 * 3.0 - 1.0;
const CUBIC_3D_BOUNDING: f32 = 1.0 / (1.5 * 1.5 * 1.5) as f32;

impl FastNoise {
    fn fractal_bounding(&self) -> f32 {
        let mut amp = self.gain;
        let mut amp_fractal = 1.0f32;
        for _ in 1..self.octaves {
            amp_fractal += amp;
            amp *= self.gain;
        }
        1.0 / amp_fractal
    }

    /// `GetNoise(x, y, z)`.
    pub fn get_noise(&self, x: f32, y: f32, z: f32) -> f32 {
        let (x, y, z) = (x * self.frequency, y * self.frequency, z * self.frequency);
        match self.noise_type {
            NoiseType::Value => single_value(self.seed, x, y, z),
            NoiseType::ValueFractal => self.fractal(single_value, x, y, z),
            NoiseType::Perlin => single_perlin(self.seed, x, y, z),
            NoiseType::PerlinFractal => self.fractal(single_perlin, x, y, z),
            NoiseType::Simplex => single_simplex(self.seed, x, y, z),
            NoiseType::SimplexFractal => self.fractal(single_simplex, x, y, z),
            NoiseType::Cellular => match self.cellular_return {
                CellularReturn::CellValue | CellularReturn::Distance => self.single_cellular(x, y, z),
                _ => self.single_cellular_2edge(x, y, z),
            },
            NoiseType::WhiteNoise => val_coord3(self.seed, float_cast2int(x), float_cast2int(y), float_cast2int(z)),
            NoiseType::Cubic => single_cubic(self.seed, x, y, z),
            NoiseType::CubicFractal => self.fractal(single_cubic, x, y, z),
        }
    }

    /// `Single*FractalFBM` / `Billow` / `RigidMulti` (identical for all base noises).
    fn fractal(&self, single: fn(i32, f32, f32, f32) -> f32, mut x: f32, mut y: f32, mut z: f32) -> f32 {
        let mut seed = self.seed;
        let mut amp = 1.0f32;
        match self.fractal_type {
            FractalType::Fbm => {
                let mut sum = single(seed, x, y, z);
                for _ in 1..self.octaves {
                    x *= self.lacunarity;
                    y *= self.lacunarity;
                    z *= self.lacunarity;
                    amp *= self.gain;
                    seed = seed.wrapping_add(1);
                    sum += single(seed, x, y, z) * amp;
                }
                sum * self.fractal_bounding()
            }
            FractalType::Billow => {
                let mut sum = single(seed, x, y, z).abs() * 2.0 - 1.0;
                for _ in 1..self.octaves {
                    x *= self.lacunarity;
                    y *= self.lacunarity;
                    z *= self.lacunarity;
                    amp *= self.gain;
                    seed = seed.wrapping_add(1);
                    sum += (single(seed, x, y, z).abs() * 2.0 - 1.0) * amp;
                }
                sum * self.fractal_bounding()
            }
            FractalType::RigidMulti => {
                let mut sum = 1.0 - single(seed, x, y, z).abs();
                for _ in 1..self.octaves {
                    x *= self.lacunarity;
                    y *= self.lacunarity;
                    z *= self.lacunarity;
                    amp *= self.gain;
                    seed = seed.wrapping_add(1);
                    sum -= (1.0 - single(seed, x, y, z).abs()) * amp;
                }
                sum
            }
        }
    }

    /// Distance of the cell point `(xi, yi, zi)` from `(x, y, z)` by the distance function.
    #[inline]
    fn cell_distance(&self, xi: i32, yi: i32, zi: i32, x: f32, y: f32, z: f32) -> f32 {
        let vec = CELL_3D[(hash3(self.seed, xi, yi, zi) & 255) as usize];
        let vx = xi as f32 - x + vec[0];
        let vy = yi as f32 - y + vec[1];
        let vz = zi as f32 - z + vec[2];
        match self.cellular_distance {
            CellularDistance::Euclidean => vx * vx + vy * vy + vz * vz,
            CellularDistance::Manhattan => vx.abs() + vy.abs() + vz.abs(),
            CellularDistance::Natural => (vx.abs() + vy.abs() + vz.abs()) + (vx * vx + vy * vy + vz * vz),
        }
    }

    fn single_cellular(&self, x: f32, y: f32, z: f32) -> f32 {
        let (xr, yr, zr) = (fast_round(x), fast_round(y), fast_round(z));
        let mut distance = 999999.0f32;
        let (mut xc, mut yc, mut zc) = (0, 0, 0);
        for xi in xr - 1..=xr + 1 {
            for yi in yr - 1..=yr + 1 {
                for zi in zr - 1..=zr + 1 {
                    let nd = self.cell_distance(xi, yi, zi, x, y, z);
                    if nd < distance {
                        distance = nd;
                        (xc, yc, zc) = (xi, yi, zi);
                    }
                }
            }
        }
        match self.cellular_return {
            CellularReturn::CellValue => val_coord3(0, xc, yc, zc),
            CellularReturn::Distance => distance - 1.0,
            _ => 0.0,
        }
    }

    fn single_cellular_2edge(&self, x: f32, y: f32, z: f32) -> f32 {
        let (xr, yr, zr) = (fast_round(x), fast_round(y), fast_round(z));
        let mut distance = 999999.0f32;
        let mut distance2 = 999999.0f32;
        for xi in xr - 1..=xr + 1 {
            for yi in yr - 1..=yr + 1 {
                for zi in zr - 1..=zr + 1 {
                    let nd = self.cell_distance(xi, yi, zi, x, y, z);
                    distance2 = java_maxf(java_minf(distance2, nd), distance);
                    distance = java_minf(distance, nd);
                }
            }
        }
        match self.cellular_return {
            CellularReturn::Distance2 => distance2 - 1.0,
            CellularReturn::Distance2Add => distance2 + distance - 1.0,
            CellularReturn::Distance2Sub => distance2 - distance - 1.0,
            CellularReturn::Distance2Mul => distance2 * distance - 1.0,
            CellularReturn::Distance2Div => distance / distance2 - 1.0,
            _ => 0.0,
        }
    }
}

/// `Math.min(float, float)` (NaN propagates).
#[inline]
fn java_minf(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() { f32::NAN } else { a.min(b) }
}

#[inline]
fn java_maxf(a: f32, b: f32) -> f32 {
    if a.is_nan() || b.is_nan() { f32::NAN } else { a.max(b) }
}

fn single_value(seed: i32, x: f32, y: f32, z: f32) -> f32 {
    let (x0, y0, z0) = (fast_floor(x), fast_floor(y), fast_floor(z));
    let (x1, y1, z1) = (x0 + 1, y0 + 1, z0 + 1);
    let xs = interp_quintic(x - x0 as f32);
    let ys = interp_quintic(y - y0 as f32);
    let zs = interp_quintic(z - z0 as f32);
    let xf00 = lerp(val_coord3(seed, x0, y0, z0), val_coord3(seed, x1, y0, z0), xs);
    let xf10 = lerp(val_coord3(seed, x0, y1, z0), val_coord3(seed, x1, y1, z0), xs);
    let xf01 = lerp(val_coord3(seed, x0, y0, z1), val_coord3(seed, x1, y0, z1), xs);
    let xf11 = lerp(val_coord3(seed, x0, y1, z1), val_coord3(seed, x1, y1, z1), xs);
    let yf0 = lerp(xf00, xf10, ys);
    let yf1 = lerp(xf01, xf11, ys);
    lerp(yf0, yf1, zs)
}

fn single_perlin(seed: i32, x: f32, y: f32, z: f32) -> f32 {
    let (x0, y0, z0) = (fast_floor(x), fast_floor(y), fast_floor(z));
    let (x1, y1, z1) = (x0 + 1, y0 + 1, z0 + 1);
    let xs = interp_quintic(x - x0 as f32);
    let ys = interp_quintic(y - y0 as f32);
    let zs = interp_quintic(z - z0 as f32);
    let xd0 = x - x0 as f32;
    let yd0 = y - y0 as f32;
    let zd0 = z - z0 as f32;
    let (xd1, yd1, zd1) = (xd0 - 1.0, yd0 - 1.0, zd0 - 1.0);
    let xf00 = lerp(grad_coord3(seed, x0, y0, z0, xd0, yd0, zd0), grad_coord3(seed, x1, y0, z0, xd1, yd0, zd0), xs);
    let xf10 = lerp(grad_coord3(seed, x0, y1, z0, xd0, yd1, zd0), grad_coord3(seed, x1, y1, z0, xd1, yd1, zd0), xs);
    let xf01 = lerp(grad_coord3(seed, x0, y0, z1, xd0, yd0, zd1), grad_coord3(seed, x1, y0, z1, xd1, yd0, zd1), xs);
    let xf11 = lerp(grad_coord3(seed, x0, y1, z1, xd0, yd1, zd1), grad_coord3(seed, x1, y1, z1, xd1, yd1, zd1), xs);
    let yf0 = lerp(xf00, xf10, ys);
    let yf1 = lerp(xf01, xf11, ys);
    lerp(yf0, yf1, zs)
}

fn single_simplex(seed: i32, x: f32, y: f32, z: f32) -> f32 {
    let mut t = (x + y + z) * F3;
    let i = fast_floor(x + t);
    let j = fast_floor(y + t);
    let k = fast_floor(z + t);
    t = (i.wrapping_add(j).wrapping_add(k)) as f32 * G3;
    let x0 = x - (i as f32 - t);
    let y0 = y - (j as f32 - t);
    let z0 = z - (k as f32 - t);
    let (i1, j1, k1, i2, j2, k2) = if x0 >= y0 {
        if y0 >= z0 {
            (1, 0, 0, 1, 1, 0)
        } else if x0 >= z0 {
            (1, 0, 0, 1, 0, 1)
        } else {
            (0, 0, 1, 1, 0, 1)
        }
    } else if y0 < z0 {
        (0, 0, 1, 0, 1, 1)
    } else if x0 < z0 {
        (0, 1, 0, 0, 1, 1)
    } else {
        (0, 1, 0, 1, 1, 0)
    };
    let x1 = x0 - i1 as f32 + G3;
    let y1 = y0 - j1 as f32 + G3;
    let z1 = z0 - k1 as f32 + G3;
    let x2 = x0 - i2 as f32 + F3;
    let y2 = y0 - j2 as f32 + F3;
    let z2 = z0 - k2 as f32 + F3;
    let x3 = x0 + G33;
    let y3 = y0 + G33;
    let z3 = z0 + G33;
    let corner = |xd: f32, yd: f32, zd: f32, ci: i32, cj: i32, ck: i32| {
        let mut t = 0.6f32 - xd * xd - yd * yd - zd * zd;
        if t < 0.0 {
            0.0
        } else {
            t *= t;
            t * t * grad_coord3(seed, ci, cj, ck, xd, yd, zd)
        }
    };
    let n0 = corner(x0, y0, z0, i, j, k);
    let n1 = corner(x1, y1, z1, i + i1, j + j1, k + k1);
    let n2 = corner(x2, y2, z2, i + i2, j + j2, k + k2);
    let n3 = corner(x3, y3, z3, i + 1, j + 1, k + 1);
    32.0 * (n0 + n1 + n2 + n3)
}

fn single_cubic(seed: i32, x: f32, y: f32, z: f32) -> f32 {
    let (x1, y1, z1) = (fast_floor(x), fast_floor(y), fast_floor(z));
    let xs = [x1 - 1, x1, x1 + 1, x1 + 2];
    let ys = [y1 - 1, y1, y1 + 1, y1 + 2];
    let zs = [z1 - 1, z1, z1 + 1, z1 + 2];
    let (tx, ty, tz) = (x - x1 as f32, y - y1 as f32, z - z1 as f32);
    let row = |yi: i32, zi: i32| {
        cubic_lerp(val_coord3(seed, xs[0], yi, zi), val_coord3(seed, xs[1], yi, zi), val_coord3(seed, xs[2], yi, zi), val_coord3(seed, xs[3], yi, zi), tx)
    };
    let plane = |zi: i32| cubic_lerp(row(ys[0], zi), row(ys[1], zi), row(ys[2], zi), row(ys[3], zi), ty);
    cubic_lerp(plane(zs[0]), plane(zs[1]), plane(zs[2]), plane(zs[3]), tz) * CUBIC_3D_BOUNDING
}

const GRAD_3D: [[f32; 3]; 16] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
    [1.0, 1.0, 0.0],
    [0.0, -1.0, 1.0],
    [-1.0, 1.0, 0.0],
    [0.0, -1.0, -1.0],
];

#[rustfmt::skip]
const CELL_3D: [[f32; 3]; 256] = [
    [0.1453787434, -0.4149781685, -0.0956981749],
    [-0.01242829687, -0.1457918398, -0.4255470325],
    [0.2877979582, -0.02606483451, -0.3449535616],
    [-0.07732986802, 0.2377094325, 0.3741848704],
    [0.1107205875, -0.3552302079, -0.2530858567],
    [0.2755209141, 0.2640521179, -0.238463215],
    [0.294168941, 0.1526064594, 0.3044271714],
    [0.4000921098, -0.2034056362, 0.03244149937],
    [-0.1697304074, 0.3970864695, -0.1265461359],
    [-0.1483224484, -0.3859694688, 0.1775613147],
    [0.2623596946, -0.2354852944, 0.2796677792],
    [-0.2709003183, 0.3505271138, -0.07901746678],
    [-0.03516550699, 0.3885234328, 0.2243054374],
    [-0.1267712655, 0.1920044036, 0.3867342179],
    [0.02952021915, 0.4409685861, 0.08470692262],
    [-0.2806854217, -0.266996757, 0.2289725438],
    [-0.171159547, 0.2141185563, 0.3568720405],
    [0.2113227183, 0.3902405947, -0.07453178509],
    [-0.1024352839, 0.2128044156, -0.3830421561],
    [-0.3304249877, -0.1566986703, 0.2622305365],
    [0.2091111325, 0.3133278055, -0.2461670583],
    [0.344678154, -0.1944240454, -0.2142341261],
    [0.1984478035, -0.3214342325, -0.2445373252],
    [-0.2929008603, 0.2262915116, 0.2559320961],
    [-0.1617332831, 0.006314769776, -0.4198838754],
    [-0.3582060271, -0.148303178, -0.2284613961],
    [-0.1852067326, -0.3454119342, -0.2211087107],
    [0.3046301062, 0.1026310383, 0.314908508],
    [-0.03816768434, -0.2551766358, -0.3686842991],
    [-0.4084952196, 0.1805950793, 0.05492788837],
    [-0.02687443361, -0.2749741471, 0.3551999201],
    [-0.03801098351, 0.3277859044, 0.3059600725],
    [0.2371120802, 0.2900386767, -0.2493099024],
    [0.4447660503, 0.03946930643, 0.05590469027],
    [0.01985147278, -0.01503183293, -0.4493105419],
    [0.4274339143, 0.03345994256, -0.1366772882],
    [-0.2072988631, 0.2871414597, -0.2776273824],
    [-0.3791240978, 0.1281177671, 0.2057929936],
    [-0.2098721267, -0.1007087278, -0.3851122467],
    [0.01582798878, 0.4263894424, 0.1429738373],
    [-0.1888129464, -0.3160996813, -0.2587096108],
    [0.1612988974, -0.1974805082, -0.3707885038],
    [-0.08974491322, 0.229148752, -0.3767448739],
    [0.07041229526, 0.4150230285, -0.1590534329],
    [-0.1082925611, -0.1586061639, 0.4069604477],
    [0.2474100658, -0.3309414609, 0.1782302128],
    [-0.1068836661, -0.2701644537, -0.3436379634],
    [0.2396452163, 0.06803600538, -0.3747549496],
    [-0.3063886072, 0.2597428179, 0.2028785103],
    [0.1593342891, -0.3114350249, -0.2830561951],
    [0.2709690528, 0.1412648683, -0.3303331794],
    [-0.1519780427, 0.3623355133, 0.2193527988],
    [0.1699773681, 0.3456012883, 0.2327390037],
    [-0.1986155616, 0.3836276443, -0.1260225743],
    [-0.1887482106, -0.2050154888, -0.353330953],
    [0.2659103394, 0.3015631259, -0.2021172246],
    [-0.08838976154, -0.4288819642, -0.1036702021],
    [-0.04201869311, 0.3099592485, 0.3235115047],
    [-0.3230334656, 0.201549922, -0.2398478873],
    [0.2612720941, 0.2759854499, -0.2409749453],
    [0.385713046, 0.2193460345, 0.07491837764],
    [0.07654967953, 0.3721732183, 0.241095919],
    [0.4317038818, -0.02577753072, 0.1243675091],
    [-0.2890436293, -0.3418179959, -0.04598084447],
    [-0.2201947582, 0.383023377, -0.08548310451],
    [0.4161322773, -0.1669634289, -0.03817251927],
    [0.2204718095, 0.02654238946, -0.391391981],
    [-0.1040307469, 0.3890079625, -0.2008741118],
    [-0.1432122615, 0.371614387, -0.2095065525],
    [0.3978380468, -0.06206669342, 0.2009293758],
    [-0.2599274663, 0.2616724959, -0.2578084893],
    [0.4032618332, -0.1124593585, 0.1650235939],
    [-0.08953470255, -0.3048244735, 0.3186935478],
    [0.118937202, -0.2875221847, 0.325092195],
    [0.02167047076, -0.03284630549, -0.4482761547],
    [-0.3411343612, 0.2500031105, 0.1537068389],
    [0.3162964612, 0.3082064153, -0.08640228117],
    [0.2355138889, -0.3439334267, -0.1695376245],
    [-0.02874541518, -0.3955933019, 0.2125550295],
    [-0.2461455173, 0.02020282325, -0.3761704803],
    [0.04208029445, -0.4470439576, 0.02968078139],
    [0.2727458746, 0.2288471896, -0.2752065618],
    [-0.1347522818, -0.02720848277, -0.4284874806],
    [0.3829624424, 0.1231931484, -0.2016512234],
    [-0.3547613644, 0.1271702173, 0.2459107769],
    [0.2305790207, 0.3063895591, 0.2354968222],
    [-0.08323845599, -0.1922245118, 0.3982726409],
    [0.2993663085, -0.2619918095, -0.2103333191],
    [-0.2154865723, 0.2706747713, 0.287751117],
    [0.01683355354, -0.2680655787, -0.3610505186],
    [0.05240429123, 0.4335128183, -0.1087217856],
    [0.00940104872, -0.4472890582, 0.04841609928],
    [0.3465688735, 0.01141914583, -0.2868093776],
    [-0.3706867948, -0.2551104378, 0.003156692623],
    [0.2741169781, 0.2139972417, -0.2855959784],
    [0.06413433865, 0.1708718512, 0.4113266307],
    [-0.388187972, -0.03973280434, -0.2241236325],
    [0.06419469312, -0.2803682491, 0.3460819069],
    [-0.1986120739, -0.3391173584, 0.2192091725],
    [-0.203203009, -0.3871641506, 0.1063600375],
    [-0.1389736354, -0.2775901578, -0.3257760473],
    [-0.06555641638, 0.342253257, -0.2847192729],
    [-0.2529246486, -0.2904227915, 0.2327739768],
    [0.1444476522, 0.1069184044, 0.4125570634],
    [-0.3643780054, -0.2447099973, -0.09922543227],
    [0.4286142488, -0.1358496089, -0.01829506817],
    [0.165872923, -0.3136808464, -0.2767498872],
    [0.2219610524, -0.3658139958, 0.1393320198],
    [0.04322940318, -0.3832730794, 0.2318037215],
    [-0.08481269795, -0.4404869674, -0.03574965489],
    [0.1822082075, -0.3953259299, 0.1140946023],
    [-0.3269323334, 0.3036542563, 0.05838957105],
    [-0.4080485344, 0.04227858267, -0.184956522],
    [0.2676025294, -0.01299671652, 0.36155217],
    [0.3024892441, -0.1009990293, -0.3174892964],
    [0.1448494052, 0.425921681, -0.0104580805],
    [0.4198402157, 0.08062320474, 0.1404780841],
    [-0.3008872161, -0.333040905, -0.03241355801],
    [0.3639310428, -0.1291284382, -0.2310412139],
    [0.3295806598, 0.0184175994, -0.3058388149],
    [0.2776259487, -0.2974929052, -0.1921504723],
    [0.4149000507, -0.144793182, -0.09691688386],
    [0.145016715, -0.0398992945, 0.4241205002],
    [0.09299023471, -0.299732164, -0.3225111565],
    [0.1028907093, -0.361266869, 0.247789732],
    [0.2683057049, -0.07076041213, -0.3542668666],
    [-0.4227307273, -0.07933161816, -0.1323073187],
    [-0.1781224702, 0.1806857196, -0.3716517945],
    [0.4390788626, -0.02841848598, -0.09435116353],
    [0.2972583585, 0.2382799621, -0.2394997452],
    [-0.1707002821, 0.2215845691, 0.3525077196],
    [0.3806686614, 0.1471852559, -0.1895464869],
    [-0.1751445661, -0.274887877, 0.3102596268],
    [-0.2227237566, -0.2316778837, 0.3149912482],
    [0.1369633021, 0.1341343041, -0.4071228836],
    [-0.3529503428, -0.2472893463, -0.129514612],
    [-0.2590744185, -0.2985577559, -0.2150435121],
    [-0.3784019401, 0.2199816631, -0.1044989934],
    [-0.05635805671, 0.1485737441, 0.4210102279],
    [0.3251428613, 0.09666046873, -0.2957006485],
    [-0.4190995804, 0.1406751354, -0.08405978803],
    [-0.3253150961, -0.3080335042, -0.04225456877],
    [0.2857945863, -0.05796152095, 0.3427271751],
    [-0.2733604046, 0.1973770973, -0.2980207554],
    [0.219003657, 0.2410037886, -0.3105713639],
    [0.3182767252, -0.271342949, 0.1660509868],
    [-0.03222023115, -0.3331161506, -0.300824678],
    [-0.3087780231, 0.1992794134, -0.2596995338],
    [-0.06487611647, -0.4311322747, 0.1114273361],
    [0.3921171432, -0.06294284106, -0.2116183942],
    [-0.1606404506, -0.358928121, -0.2187812825],
    [-0.03767771199, -0.2290351443, 0.3855169162],
    [0.1394866832, -0.3602213994, 0.2308332918],
    [-0.4345093872, 0.005751117145, 0.1169124335],
    [-0.1044637494, 0.4168128432, -0.1336202785],
    [0.2658727501, 0.2551943237, 0.2582393035],
    [0.2051461999, 0.1975390727, 0.3484154868],
    [-0.266085566, 0.23483312, 0.2766800993],
    [0.07849405464, -0.3300346342, -0.2956616708],
    [-0.2160686338, 0.05376451292, -0.3910546287],
    [-0.185779186, 0.2148499206, 0.3490352499],
    [0.02492421743, -0.3229954284, -0.3123343347],
    [-0.120167831, 0.4017266681, 0.1633259825],
    [-0.02160084693, -0.06885389554, 0.4441762538],
    [0.2597670064, 0.3096300784, 0.1978643903],
    [-0.1611553854, -0.09823036005, 0.4085091653],
    [-0.3278896792, 0.1461670309, 0.2713366126],
    [0.2822734956, 0.03754421121, -0.3484423997],
    [0.03169341113, 0.347405252, -0.2842624114],
    [0.2202613604, -0.3460788041, -0.1849713341],
    [0.2933396046, 0.3031973659, 0.1565989581],
    [-0.3194922995, 0.2453752201, -0.200538455],
    [-0.3441586045, -0.1698856132, -0.2349334659],
    [0.2703645948, -0.3574277231, 0.04060059933],
    [0.2298568861, 0.3744156221, 0.0973588921],
    [0.09326603877, -0.3170108894, 0.3054595587],
    [-0.1116165319, -0.2985018719, 0.3177080142],
    [0.2172907365, -0.3460005203, -0.1885958001],
    [0.1991339479, 0.3820341668, -0.1299829458],
    [-0.0541918155, -0.2103145071, 0.39412061],
    [0.08871336998, 0.2012117383, 0.3926114802],
    [0.2787673278, 0.3505404674, 0.04370535101],
    [-0.322166438, 0.3067213525, 0.06804996813],
    [-0.4277366384, 0.132066775, 0.04582286686],
    [0.240131882, -0.1612516055, 0.344723946],
    [0.1448607981, -0.2387819045, 0.3528435224],
    [-0.3837065682, -0.2206398454, 0.08116235683],
    [-0.4382627882, -0.09082753406, -0.04664855374],
    [-0.37728353, 0.05445141085, 0.2391488697],
    [0.1259579313, 0.348394558, 0.2554522098],
    [-0.1406285511, -0.270877371, -0.3306796947],
    [-0.1580694418, 0.4162931958, -0.06491553533],
    [0.2477612106, -0.2927867412, -0.2353514536],
    [0.2916132853, 0.3312535401, 0.08793624968],
    [0.07365265219, -0.1666159848, 0.411478311],
    [-0.26126526, -0.2422237692, 0.2748965434],
    [-0.3721862032, 0.252790166, 0.008634938242],
    [-0.3691191571, -0.255281188, 0.03290232422],
    [0.2278441737, -0.3358364886, 0.1944244981],
    [0.363398169, -0.2310190248, 0.1306597909],
    [-0.304231482, -0.2698452035, 0.1926830856],
    [-0.3199312232, 0.316332536, -0.008816977938],
    [0.2874852279, 0.1642275508, -0.304764754],
    [-0.1451096801, 0.3277541114, -0.2720669462],
    [0.3220090754, 0.0511344108, 0.3101538769],
    [-0.1247400865, -0.04333605335, -0.4301882115],
    [-0.2829555867, -0.3056190617, -0.1703910946],
    [0.1069384374, 0.3491024667, -0.2630430352],
    [-0.1420661144, -0.3055376754, -0.2982682484],
    [-0.250548338, 0.3156466809, -0.2002316239],
    [0.3265787872, 0.1871229129, 0.2466400438],
    [0.07646097258, -0.3026690852, 0.324106687],
    [0.3451771584, 0.2757120714, -0.0856480183],
    [0.298137964, 0.2852657134, 0.179547284],
    [0.2812250376, 0.3466716415, 0.05684409612],
    [0.4390345476, -0.09790429955, -0.01278335452],
    [0.2148373234, 0.1850172527, 0.3494474791],
    [0.2595421179, -0.07946825393, 0.3589187731],
    [0.3182823114, -0.307355516, -0.08203022006],
    [-0.4089859285, -0.04647718411, 0.1818526372],
    [-0.2826749061, 0.07417482322, 0.3421885344],
    [0.3483864637, 0.225442246, -0.1740766085],
    [-0.3226415069, -0.1420585388, -0.2796816575],
    [0.4330734858, -0.118868561, -0.02859407492],
    [-0.08717822568, -0.3909896417, -0.2050050172],
    [-0.2149678299, 0.3939973956, -0.03247898316],
    [-0.2687330705, 0.322686276, -0.1617284888],
    [0.2105665099, -0.1961317136, -0.3459683451],
    [0.4361845915, -0.1105517485, 0.004616608544],
    [0.05333333359, -0.313639498, -0.3182543336],
    [-0.05986216652, 0.1361029153, -0.4247264031],
    [0.3664988455, 0.2550543014, -0.05590974511],
    [-0.2341015558, -0.182405731, 0.3382670703],
    [-0.04730947785, -0.4222150243, -0.1483114513],
    [-0.2391566239, -0.2577696514, -0.2808182972],
    [-0.1242081035, 0.4256953395, -0.07652336246],
    [0.2614832715, -0.3650179274, 0.02980623099],
    [-0.2728794681, -0.3499628774, 0.07458404908],
    [0.007892900508, -0.1672771315, 0.4176793787],
    [-0.01730330376, 0.2978486637, -0.3368779738],
    [0.2054835762, -0.3252600376, -0.2334146693],
    [-0.3231994983, 0.1564282844, -0.2712420987],
    [-0.2669545963, 0.2599343665, -0.2523278991],
    [-0.05554372779, 0.3170813944, -0.3144428146],
    [-0.2083935713, -0.310922837, -0.2497981362],
    [0.06989323478, -0.3156141536, 0.3130537363],
    [0.3847566193, -0.1605309138, -0.1693876312],
    [-0.3026215288, -0.3001537679, -0.1443188342],
    [0.3450735512, 0.08611519592, 0.2756962409],
    [0.1814473292, -0.2788782453, -0.3029914042],
    [-0.03855010448, 0.09795110726, 0.4375151083],
    [0.3533670318, 0.2665752752, 0.08105160988],
    [-0.007945601311, 0.140359426, -0.4274764309],
    [0.4063099273, -0.1491768253, -0.1231199324],
    [-0.2016773589, 0.008816271194, -0.4021797064],
    [-0.07527055435, -0.425643481, -0.1251477955],
];
