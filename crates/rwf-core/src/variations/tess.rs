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

//! `dc_triantess` (`DC_TrianTessFunc`): triangle-group tessellations of the sphere, the plane
//! or the hyperbolic disc (`DC_BaseFunc` color variation; the six colors are hex string
//! resources).

use super::glsl::{Vec2, Vec3, dc_output, smoothstep, vec2};
use super::resource_vars::text;
use super::{Ctx, LoadEnv, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::math::*;

type V3 = [f64; 3];

/// Own parameters before the five `DC_BaseFunc` ones.
const N: usize = 16;
/// Precalculated `init()` values: `[spaceType, nb, nc, gA, gB, gC, p, tVR, tSR, cRA, sRA]`.
const PRE: usize = N + 5;
/// Colors `[faceA, faceB, faceC, segment, vertex, background]` (0..1 RGB).
const COL: usize = PRE + 1 + 6 * 3 + 4;

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn v3(d: &[f64], i: usize) -> V3 {
    [d[i], d[i + 1], d[i + 2]]
}

struct Space {
    st: f64,
}

impl Space {
    fn hdotd(&self, a: V3, b: V3) -> f64 {
        a[0] * b[0] + a[1] * b[1] + self.st * a[2] * b[2]
    }
    fn hdotv(&self, a: V3, b: V3) -> f64 {
        self.st * (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
    }
    fn hlengthv(&self, v: V3) -> f64 {
        self.hdotv(v, v).abs().sqrt()
    }
    fn hlengthd(&self, v: V3) -> f64 {
        self.hdotd(v, v).abs().sqrt()
    }
    fn hnormalizev(&self, v: V3) -> V3 {
        mul(v, 1.0 / self.hlengthv(v))
    }
    fn dd(&self, tha: f64, r: f64) -> f64 {
        tha * (1.0 + self.st * r * r) / (1.0 + self.st * self.st * r * tha)
    }
}

/// `init()`
fn prepare(p: &[f64]) -> Vec<f64> {
    let lim = limit_value;
    let mut d = p.to_vec();
    d[3] = lim(p[3], 0.0, 20.0).trunc();
    for i in 4..7 {
        d[i] = lim(p[i], 2.0, 20.0).trunc();
    }
    for i in 7..10 {
        d[i] = lim(p[i], 0.0, 1.0);
    }
    d[10] = lim(p[10], 0.0, 0.25);
    d[11] = lim(p[11], 0.0, 0.05);
    d[12] = lim(p[12], -3.0, 3.0);
    for i in 13..16 {
        d[i] = lim(p[i], 0.0, 1.0).trunc();
    }
    let (pp, qp, rp) = (d[4], d[5], d[6]);
    let (u, v, w, vrad, srad, rot) = (d[7], d[8], d[9], d[10], d[11], d[12]);
    let sum = qp * rp + pp * rp + pp * qp - pp * qp * rp;
    let st = if sum > 0.0 {
        1.0
    } else if sum < 0.0 {
        -1.0
    } else {
        0.0
    };
    let s = Space { st };
    let (cospip, sinpip) = ((M_PI / pp).cos(), (M_PI / pp).sin());
    let cospiq = (M_PI / qp).cos();
    let (cospir, sinpir) = ((M_PI / rp).cos(), (M_PI / rp).sin());
    let ncsincos = (cospiq + cospip * cospir) / sinpip;
    let nb = [-cospip, sinpip, 0.0];
    let mut nc = [-cospir, -ncsincos, ((ncsincos + sinpir) * (-ncsincos + sinpir)).abs().sqrt()];
    if st == 0.0 {
        nc[2] = 0.25;
    }
    let pa = [nb[1] * nc[2], -nb[0] * nc[2], nb[0] * nc[1] - nb[1] * nc[0]];
    let pb = [0.0, nc[2], -nc[1]];
    let pc = [0.0, 0.0, nb[1]];
    let q = add(add(mul(pa, u), mul(pb, v)), mul(pc, w));
    let qq = s.hlengthv(q);
    let g = |pv: V3| sub(mul(pv, qq / s.hdotv(pv, q)), q);
    let pn = s.hnormalizev(q);
    let (tvr, tsr, cra, sra) = if st == -1.0 {
        ((0.5 * vrad).tanh(), (0.5 * srad).tanh(), rot.cosh(), rot.sinh())
    } else if st == 1.0 {
        ((0.5 * vrad).tan(), (0.5 * srad).tan(), rot.cos(), rot.sin())
    } else {
        (0.5 * vrad, 0.5 * srad, 1.0, rot)
    };
    d.push(st);
    for x in [nb, nc, g(pa), g(pb), g(pc), pn] {
        d.extend(x);
    }
    d.extend([tvr, tsr, cra, sra]);
    d.extend([1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
    d
}

/// `RGBtovec3`: six hex digits, else the default.
#[allow(clippy::ptr_arg)] // `LoadFn` signature
fn load(d: &mut Vec<f64>, res: &[Vec<u8>], _: &LoadEnv) {
    for (i, r) in res.iter().enumerate().take(6) {
        let s = text(r);
        if let Some(h) = s.get(0..6).filter(|h| h.chars().all(|c| c.is_ascii_hexdigit())) {
            for k in 0..3 {
                d[COL + 3 * i + k] = u8::from_str_radix(&h[2 * k..2 * k + 2], 16).unwrap_or(0) as f64 / 255.0;
            }
        }
    }
}

/// `getRGBColor`
fn rgb(d: &[f64], xp: f64, yp: f64) -> Vec3 {
    let st = d[PRE];
    let s = Space { st };
    let (nb, nc, ga, gb, gc, p) = (v3(d, PRE + 1), v3(d, PRE + 4), v3(d, PRE + 7), v3(d, PRE + 10), v3(d, PRE + 13), v3(d, PRE + 16));
    let (tvr, tsr, cra, sra) = (d[PRE + 19], d[PRE + 20], d[PRE + 21], d[PRE + 22]);
    let col = |i: usize| Vec3::new(d[COL + 3 * i], d[COL + 3 * i + 1], d[COL + 3 * i + 2]);
    let bg = col(5);
    const AA: f64 = 0.0005;
    let pos = vec2(xp / d[0] + d[1], yp / d[0] + d[2]);
    let r = (pos.x * pos.x + pos.y * pos.y).sqrt();
    if st == -1.0 && r >= 1.0 {
        return bg; // outside the Poincaré disc
    }
    let k = 1.0 / (1.0 + st * r * r);
    let mut z = [2.0 * pos.x * k, 2.0 * pos.y * k, (1.0 - st * r * r) * k];
    // Rotate (RotVector = (0, 1))
    let vp = z[1];
    z[1] += vp * (cra - 1.0) - z[2] * sra;
    z[2] += vp * st * sra + z[2] * (cra - 1.0);
    // fold (Java aliases `ap` with `pos` after the abs())
    let nbs = [nb[0], nb[1], nb[2] * st];
    let ncs = [nc[0], nc[1], nc[2] * st];
    let mut ap = add(z, [1.0; 3]);
    let mut i = 0;
    while i < d[3] as i32 && (z[0] != ap[0] || z[1] != ap[1]) {
        z[0] = z[0].abs();
        ap = z;
        z = add(z, mul(nbs, -2.0 * (dot3(nb, z)).min(0.0)));
        z = add(z, mul(ncs, -2.0 * (dot3(nc, z)).min(0.0)));
        i += 1;
    }
    let mut color = bg;
    if d[15] as i32 == 1 {
        let (fa, fb, fc) = (s.hdotd(ga, z), s.hdotd(gb, z), s.hdotd(gc, z));
        let f = fa.max(fb.max(fc));
        let c = [(fa == f) as i32 as f64, (fb == f) as i32 as f64, (fc == f) as i32 as f64];
        let k = 1.0 / (c[0] + c[1] + c[2]);
        color = col(0) * (c[0] * k) + col(1) * (c[1] * k) + col(2) * (c[2] * k);
    }
    if d[14] as i32 == 1 {
        let seg = |n: V3| {
            let (pn, nn, pp, zn, zp) = (s.hdotv(p, n), s.hdotv(n, n), s.hdotv(p, p), s.hdotv(z, n), s.hdotv(z, p));
            let det = 1.0 / (pn * pn - pp * nn);
            let alpha = det * (pn * zn - zp * nn);
            let beta = det * (-pp * zn + pn * zp);
            let pmin = s.hnormalizev(add(mul(p, alpha), mul(n, beta.min(0.0))));
            let tha = s.hlengthd(sub(pmin, z)) / s.hlengthv(add(pmin, z));
            s.dd((tha - tsr) / (1.0 + st * tha * tsr), r)
        };
        let ds = seg([1.0, 0.0, 0.0]).min(seg(nb)).min(seg(ncs));
        color = mix3(col(3), color, smoothstep(-1.0, 1.0, ds * 0.5 / AA));
    }
    if d[13] as i32 == 1 {
        let tha = s.hlengthd(sub(p, z)) / s.hlengthv(add(p, z));
        let dv = s.dd((tha - tvr) / (1.0 + st * tha * tvr), r);
        color = mix3(col(4), color, smoothstep(-1.0, 1.0, dv * 0.5 / AA));
    }
    if st == -1.0 {
        color = mix3(bg, color, smoothstep(0.0, 1.0, (1.0 - r) * 0.5 / AA));
    }
    color
}

fn dot3(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn mix3(a: Vec3, b: Vec3, t: f64) -> Vec3 {
    a * (1.0 - t) + b * t
}

fn apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let uv: Vec2 = if d[N] as i32 == 1 { vec2(a.x, a.y) } else { vec2(ctx.random() - 0.5, ctx.random() - 0.5) };
    let color = rgb(d, uv.x, uv.y);
    dc_output(ctx, v, amount, uv, color, d[N + 1] as i32, d[N + 2], d[N + 3], d[N + 4] as i32);
}

pub static DEFS: &[VarDef] = &[VarDef {
    name: "dc_triantess",
    params: &[
        ParamDef::range("zoom", 0.5, 0.0, 10.0),
        ParamDef::range("CenterX", 0.0, -5.0, 5.0),
        ParamDef::range("CenterY", 0.0, -5.0, 5.0),
        ParamDef::int("Iterations", 13.0, 0.0, 20.0),
        ParamDef::int("pParam", 2.0, 2.0, 20.0),
        ParamDef::int("qParam", 3.0, 2.0, 20.0),
        ParamDef::int("rParam", 7.0, 2.0, 20.0),
        ParamDef::range("U", 1.0, 0.0, 1.0),
        ParamDef::range("V", 1.0, 0.0, 1.0),
        ParamDef::range("W", 0.0, 0.0, 1.0),
        ParamDef::range("VRadius", 0.0, 0.0, 0.25),
        ParamDef::range("SRadius", 0.02528, 0.0, 0.05),
        ParamDef::range("RotAngle", 0.0, -3.0, 3.0),
        ParamDef::int("Vertex", 1.0, 0.0, 1.0),
        ParamDef::int("Segments", 1.0, 0.0, 1.0),
        ParamDef::int("Faces", 1.0, 0.0, 1.0),
        ParamDef::int("ColorOnly", 0.0, 0.0, 1.0),
        ParamDef::int("Gradient", 0.0, 0.0, 2.0),
        ParamDef::range("scale_z", 0.0, -2.0, 2.0),
        ParamDef::range("offset_z", 0.0, -2.0, 2.0),
        ParamDef::int("reset_z", 1.0, 0.0, 1.0),
    ],
    kind: VarKind::Normal,
    is_3d: false,
    apply: |d, ctx, a, v, amount| apply(d, ctx, a, v, amount),
    prepare: Some(|p, _| prepare(p)),
    wgsl: "fn dc_triantess_hdotd(a: vec3<f32>, b: vec3<f32>, st: f32) -> f32 { return a.x * b.x + a.y * b.y + st * a.z * b.z; }
fn dc_triantess_hdotv(a: vec3<f32>, b: vec3<f32>, st: f32) -> f32 { return st * (a.x * b.x + a.y * b.y) + a.z * b.z; }
fn dc_triantess_lenv(v: vec3<f32>, st: f32) -> f32 { return sqrt(abs(dc_triantess_hdotv(v, v, st))); }
fn dc_triantess_lend(v: vec3<f32>, st: f32) -> f32 { return sqrt(abs(dc_triantess_hdotd(v, v, st))); }
fn dc_triantess_dd(tha: f32, r: f32, st: f32) -> f32 { return tha * (1.0 + st * r * r) / (1.0 + st * st * r * tha); }
fn dc_triantess_v3(base: u32, i: u32) -> vec3<f32> { return vec3<f32>(prm[base + i], prm[base + i + 1u], prm[base + i + 2u]); }
fn dc_triantess_seg(z: vec3<f32>, n: vec3<f32>, p: vec3<f32>, r: f32, tsr: f32, st: f32) -> f32 {
  let pn = dc_triantess_hdotv(p, n, st);
  let nn = dc_triantess_hdotv(n, n, st);
  let pp = dc_triantess_hdotv(p, p, st);
  let zn = dc_triantess_hdotv(z, n, st);
  let zp = dc_triantess_hdotv(z, p, st);
  let det = 1.0 / (pn * pn - pp * nn);
  let alpha = det * (pn * zn - zp * nn);
  let beta = det * (-pp * zn + pn * zp);
  var pmin = p * alpha + n * min(0.0, beta);
  pmin = pmin / dc_triantess_lenv(pmin, st);
  let tha = dc_triantess_lend(pmin - z, st) / dc_triantess_lenv(pmin + z, st);
  return dc_triantess_dd((tha - tsr) / (1.0 + st * tha * tsr), r, st);
}
fn dc_triantess_rgb(base: u32, xp: f32, yp: f32) -> vec3<f32> {
  let pre = 21u;
  let col = 44u;
  let st = prm[base + pre];
  let nb = dc_triantess_v3(base, pre + 1u);
  let nc = dc_triantess_v3(base, pre + 4u);
  let ga = dc_triantess_v3(base, pre + 7u);
  let gb = dc_triantess_v3(base, pre + 10u);
  let gc = dc_triantess_v3(base, pre + 13u);
  let p = dc_triantess_v3(base, pre + 16u);
  let tvr = prm[base + pre + 19u];
  let tsr = prm[base + pre + 20u];
  let cra = prm[base + pre + 21u];
  let sra = prm[base + pre + 22u];
  let bg = dc_triantess_v3(base, col + 15u);
  let aa = 0.0005;
  let pos = vec2<f32>(xp, yp) / prm[base] + vec2<f32>(prm[base + 1u], prm[base + 2u]);
  let r = length(pos);
  if (st == -1.0 && r >= 1.0) { return bg; }
  let k = 1.0 / (1.0 + st * r * r);
  var z = vec3<f32>(2.0 * pos.x * k, 2.0 * pos.y * k, (1.0 - st * r * r) * k);
  let vp = z.y;
  z.y += vp * (cra - 1.0) - z.z * sra;
  z.z += vp * st * sra + z.z * (cra - 1.0);
  let nbs = vec3<f32>(nb.xy, nb.z * st);
  let ncs = vec3<f32>(nc.xy, nc.z * st);
  var ap = z + 1.0;
  let iters = i32(prm[base + 3u]);
  for (var i = 0; i < iters && (z.x != ap.x || z.y != ap.y); i++) {
    z.x = abs(z.x);
    ap = z;
    z += nbs * (-2.0 * min(0.0, dot(nb, z)));
    z += ncs * (-2.0 * min(0.0, dot(nc, z)));
  }
  var color = bg;
  if (i32(prm[base + 15u]) == 1) {
    let fa = dc_triantess_hdotd(ga, z, st);
    let fb = dc_triantess_hdotd(gb, z, st);
    let fc = dc_triantess_hdotd(gc, z, st);
    let f = max(fa, max(fb, fc));
    let c = vec3<f32>(select(0.0, 1.0, fa == f), select(0.0, 1.0, fb == f), select(0.0, 1.0, fc == f));
    let kc = 1.0 / (c.x + c.y + c.z);
    color = dc_triantess_v3(base, col) * (c.x * kc) + dc_triantess_v3(base, col + 3u) * (c.y * kc) + dc_triantess_v3(base, col + 6u) * (c.z * kc);
  }
  if (i32(prm[base + 14u]) == 1) {
    let ds = min(min(dc_triantess_seg(z, vec3<f32>(1.0, 0.0, 0.0), p, r, tsr, st), dc_triantess_seg(z, nb, p, r, tsr, st)), dc_triantess_seg(z, ncs, p, r, tsr, st));
    // NaN distances (e.g. Euclidean groups) make the color NaN on the CPU and in Java
    if (glsl_is_nan(ds)) { return vec3<f32>(math_nan()); }
    color = mix(dc_triantess_v3(base, col + 9u), color, glsl_smoothstep(-1.0, 1.0, ds * 0.5 / aa));
  }
  if (i32(prm[base + 13u]) == 1) {
    let tha = dc_triantess_lend(p - z, st) / dc_triantess_lenv(p + z, st);
    let dv = dc_triantess_dd((tha - tvr) / (1.0 + st * tha * tvr), r, st);
    if (glsl_is_nan(dv)) { return vec3<f32>(math_nan()); }
    color = mix(dc_triantess_v3(base, col + 12u), color, glsl_smoothstep(-1.0, 1.0, dv * 0.5 / aa));
  }
  if (st == -1.0) { color = mix(bg, color, glsl_smoothstep(0.0, 1.0, (1.0 - r) * 0.5 / aa)); }
  return color;
}
@body
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(16)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_triantess_rgb(base, uv.x, uv.y), i32(D(17)), D(18), D(19), i32(D(20)));",
}];

const fn color(name: &'static str, default: &'static str) -> ResDef {
    ResDef { name, kind: ResKind::Text, default }
}

pub static RESOURCES: &[ResourceVar] = &[ResourceVar {
    name: "dc_triantess",
    resources: &[
        color("FaceA_-RGB", "FFFFFF"),
        color("FaceB_-RGB", "000000"),
        color("FaceC_-RGB", "000000"),
        color("Segment_-RGB", "FFFFFF"),
        color("Vertex_-RGB", "000000"),
        color("Background_-RGB", "000000"),
    ],
    load,
}];

#[cfg(test)]
mod tests {
    use super::super::{find, prepare_instance};
    use super::*;
    use crate::model::{Affine, VariationInstance};

    #[test]
    fn layout_and_colors() {
        assert_eq!((PRE, COL), (21, 44));
        let mut inst = VariationInstance::new("dc_triantess", 1.0);
        inst.set_resource("FaceB_-RGB", "FF8000");
        let d = prepare_instance(find("dc_triantess").unwrap(), &inst, &Affine::IDENTITY, 1);
        assert_eq!(d.len(), COL + 18);
        assert_eq!(&d[COL + 3..COL + 6], &[1.0, 128.0 / 255.0, 0.0]);
        // (2, 3, 7) is hyperbolic
        assert_eq!(d[PRE], -1.0);
        let c = rgb(&d, 0.1, 0.05);
        assert!(c.x.is_finite() && (0.0..=1.0).contains(&c.x));
    }
}
