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

// ---- noise.wgsl: NoiseTools (simplex / Perlin) and VoronoiTools (mirrors noise.rs) ---------

const NOISE_VORONOI_MAXPOINTS: i32 = 25;

// `NoiseTools.p` and `grad3` are packed at the start of `prm` (`variations::noise_tables`):
// the first 1024 hash values (the Java table repeats with period 1024), then the 1024
// gradients (x, y, z). Constant arrays would be copied for every dynamic index on some
// drivers.
fn noise_perm(i: i32) -> i32 { return i32(prm[u32(i & 0x3ff)]); }

fn noise_simplex_corner(c: vec3<f32>, gi: i32) -> f32 {
  var t = 0.6 - c.x * c.x - c.y * c.y - c.z * c.z;
  if (t <= 0.0) { return 0.0; }
  let k = u32(gi) * 3u;
  let g = vec3<f32>(prm[1024u + k], prm[1025u + k], prm[1026u + k]);
  t *= t;
  return t * t * dot(g, c);
}

// NoiseTools.simplexNoise3D: roughly -1..1.
fn noise_simplex3d(v: vec3<f32>) -> f32 {
  let skew_in = (v.x + v.y + v.z) * (1.0 / 3.0);
  let i = i32(floor(v.x + skew_in));
  let j = i32(floor(v.y + skew_in));
  let k = i32(floor(v.z + skew_in));
  let t = f32(i + j + k) * (1.0 / 6.0);
  let c0 = v - (vec3<f32>(f32(i), f32(j), f32(k)) - t);
  var o1: vec3<i32>;
  var o2: vec3<i32>;
  if (c0.x >= c0.y) {
    if (c0.y >= c0.z) {
      o1 = vec3<i32>(1, 0, 0); o2 = vec3<i32>(1, 1, 0);
    } else if (c0.x >= c0.z) {
      o1 = vec3<i32>(1, 0, 0); o2 = vec3<i32>(1, 0, 1);
    } else {
      o1 = vec3<i32>(0, 0, 1); o2 = vec3<i32>(1, 0, 1);
    }
  } else if (c0.y < c0.z) {
    o1 = vec3<i32>(0, 0, 1); o2 = vec3<i32>(0, 1, 1);
  } else if (c0.x < c0.z) {
    o1 = vec3<i32>(0, 1, 0); o2 = vec3<i32>(0, 1, 1);
  } else {
    o1 = vec3<i32>(0, 1, 0); o2 = vec3<i32>(1, 1, 0);
  }
  let c1 = c0 - vec3<f32>(o1) + (1.0 / 6.0);
  let c2 = c0 - vec3<f32>(o2) + (2.0 / 6.0);
  let c3 = c0 - 1.0 + 0.5;
  let ii = i & 0x3ff;
  let jj = j & 0x3ff;
  let kk = k & 0x3ff;
  let g0 = noise_perm(ii + noise_perm(jj + noise_perm(kk)));
  let g1 = noise_perm(ii + o1.x + noise_perm(jj + o1.y + noise_perm(kk + o1.z)));
  let g2 = noise_perm(ii + o2.x + noise_perm(jj + o2.y + noise_perm(kk + o2.z)));
  let g3 = noise_perm(ii + 1 + noise_perm(jj + 1 + noise_perm(kk + 1)));
  return 32.0 * (noise_simplex_corner(c0, g0) + noise_simplex_corner(c1, g1)
    + noise_simplex_corner(c2, g2) + noise_simplex_corner(c3, g3));
}

// NoiseTools.perlinNoise3D
fn noise_perlin3d(v: vec3<f32>, a_scale: f32, f_scale: f32, octaves: i32) -> f32 {
  var n = 0.0;
  var u = v;
  var a = 1.0;
  for (var i = 0; i < octaves; i++) {
    n += noise_simplex3d(u) / a;
    a *= a_scale;
    u *= f_scale;
  }
  return n;
}

// VoronoiTools.vratio(P, Q, U)
fn noise_vratio(p: vec2<f32>, q: vec2<f32>, u: vec2<f32>) -> f32 {
  let pmq = p - q;
  if (pmq.x == 0.0 && pmq.y == 0.0) { return 1.0; }
  return 2.0 * dot(u - q, pmq) / dot(pmq, pmq);
}

// VoronoiTools.closest(P, n, U): index of the closest of the first n points.
fn noise_closest(p: array<vec2<f32>, 25>, n: i32, u: vec2<f32>) -> i32 {
  var d2min = 3.0e38;
  var j = 0;
  for (var i = 0; i < n; i++) {
    let d = p[i] - u;
    let d2 = dot(d, d);
    if (d2 < d2min) {
      d2min = d2;
      j = i;
    }
  }
  return j;
}

// VoronoiTools.voronoi(P, n, q, U)
fn noise_voronoi(p: array<vec2<f32>, 25>, n: i32, q: i32, u: vec2<f32>) -> f32 {
  var ratiomax = -3.0e38;
  for (var i = 0; i < n; i++) {
    if (i != q) {
      ratiomax = max(ratiomax, noise_vratio(p[i], p[q], u));
    }
  }
  return ratiomax;
}
