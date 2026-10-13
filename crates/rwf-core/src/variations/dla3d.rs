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

//! `dla3d_wf` (`DLA3DWFFunc`, `DLA3DWFFuncIterator`): a 3D diffusion limited aggregation
//! cluster, displayed as blurred nodes and junctions or as optional node/junction meshes.
//!
//! The cluster is always grown with JWildfire's single-thread algorithm (the multi-threaded one
//! is not deterministic). The transform draws from the per-thread generator instead of the
//! variation's shared `MarsagliaRandomGenerator`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::resource_vars::{FREC, M_FSTART, M_HEADER, M_NF, M_TOTAL, M_VSTART, VREC, apply_image_color, load_image, pack_mesh, text};
use super::{Ctx, LoadEnv, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::brush::Marsaglia;
use crate::math::*;
use crate::mesh::{self, MeshSource, Refinement};

/// Number of parameters.
const NP: usize = 25;
/// Mesh blocks: colormap image, unused displacement image, mesh header, bounding box centre.
const MESH_BLOCK: usize = M_HEADER + 3;
const NODE: usize = NP;
const JUNCT: usize = NODE + MESH_BLOCK;
/// Number of cluster points, followed by the point records.
const NPTS: usize = JUNCT + MESH_BLOCK;
const PTS: usize = NPTS + 1;
/// Values per point `[x, y, z, rel_depth, parent (-1 for the root), rotation (3x3)]`.
const PREC: usize = 14;

#[derive(Clone, Copy)]
struct Point {
    parent: Option<usize>,
    path_length: i32,
    x: f64,
    y: f64,
    z: f64,
}

/// `DLA3DWFFuncIterator.iterate` (single thread).
fn iterate(max_iter: i32, seed: i32, glue_radius: f64, force: [f64; 3]) -> Vec<f64> {
    let mut rg = Marsaglia::new(seed as i64);
    let mut res = vec![Point { parent: None, path_length: 0, x: 0.0, y: 0.0, z: 0.0 }];
    // Points by grid cell of size glue_radius, to find the nearest point within the glue radius
    // (the first in list order on ties, as in Java).
    let cell = |x: f64, y: f64, z: f64| ((x / glue_radius).floor() as i64, (y / glue_radius).floor() as i64, (z / glue_radius).floor() as i64);
    let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    grid.entry(cell(0.0, 0.0, 0.0)).or_default().push(0);
    let mut inner_radius = 3.0;
    let mut outer_radius = 3.0 * inner_radius;
    // Guards against clusters that cannot grow (e.g. strong forces), which hang JWildfire.
    let mut failed = 0u64;
    let max_failed = 10_000 + 1000 * max_iter.max(0) as u64;
    while (res.len() as i32) < max_iter && failed < max_failed {
        let phi = rg.random() * (M_PI + M_PI);
        let theta = rg.random() * (M_PI + M_PI);
        let (ci0, cj0, ck0) = (inner_radius * theta.sin() * phi.cos(), inner_radius * theta.sin() * phi.sin(), inner_radius * theta.cos());
        // nextWalk
        let (mut ci, mut cj, mut ck) = (ci0, cj0, ck0);
        let mut next = None;
        for _ in 0..1_000_000 {
            let move_radius = 0.9;
            match (rg.random() * 6.0) as i32 {
                0 => ci = ci + move_radius + force[0],
                1 => cj = cj - move_radius + force[1],
                2 => ci = ci - move_radius + force[0],
                3 => ck = ck - move_radius + force[2],
                4 => ck = ck + move_radius + force[2],
                5 => cj = cj + move_radius + force[1],
                _ => {}
            }
            let radius = (ci * ci + cj * cj + ck * ck).sqrt();
            let (gi, gj, gk) = cell(ci, cj, ck);
            let mut parent: Option<usize> = None;
            let mut min_radius = f64::MAX;
            for di in -1..=1 {
                for dj in -1..=1 {
                    for dk in -1..=1 {
                        let Some(list) = grid.get(&(gi + di, gj + dj, gk + dk)) else { continue };
                        for &i in list {
                            let r = &res[i];
                            let dr0 = ((r.x - ci) * (r.x - ci) + (r.y - cj) * (r.y - cj) + (r.z - ck) * (r.z - ck)).sqrt();
                            if dr0 <= glue_radius && (dr0 < min_radius || (dr0 == min_radius && parent.is_some_and(|p| i < p))) {
                                min_radius = dr0;
                                parent = Some(i);
                            }
                        }
                    }
                }
            }
            if let Some(p) = parent {
                next = Some(Point { parent: Some(p), path_length: res[p].path_length + 1, x: ci, y: cj, z: ck });
                if radius > inner_radius {
                    inner_radius = radius;
                    outer_radius = 2.1 * inner_radius;
                }
                break;
            } else if radius > outer_radius {
                break;
            }
        }
        match next {
            Some(p) => {
                grid.entry(cell(p.x, p.y, p.z)).or_default().push(res.len());
                res.push(p);
            }
            None => failed += 1,
        }
    }
    // calcMaxPathLength: the longest path through each point (children follow their parents)
    let mut max_path: Vec<i32> = res.iter().map(|p| p.path_length).collect();
    for i in (0..res.len()).rev() {
        if let Some(p) = res[i].parent {
            max_path[p] = max_path[p].max(max_path[i]);
        }
    }
    let mut d = Vec::with_capacity(res.len() * PREC);
    for (i, p) in res.iter().enumerate() {
        d.extend([p.x, p.y, p.z, p.path_length as f64 / max_path[i] as f64]);
        match p.parent {
            Some(qi) => {
                let q = &res[qi];
                let (mut dx, mut dy, mut dz) = (p.x - q.x, p.y - q.y, p.z - q.z);
                let l = (dx * dx + dy * dy + dz * dz).sqrt() + 1.0e-16;
                dx /= l;
                dy /= l;
                dz /= l;
                let elevation = dy.atan2(dx) + M_PI_2;
                let azimuth = -dz.atan2((dx * dx + dy * dy).sqrt());
                let (sina, cosa, sinb, cosb) = (elevation.sin(), elevation.cos(), azimuth.sin(), azimuth.cos());
                d.push(qi as f64);
                d.extend([cosb, 0.0, sinb, sina * sinb, cosa, -sina * cosb, -cosa * sinb, sina, cosa * cosb]);
            }
            None => d.extend([-1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
        }
    }
    d
}

/// Cached point records of a cluster (`DLA3DWFFunc.getPoints`).
fn points(max_iter: i32, seed: i32, glue_radius: f64, force: [f64; 3]) -> Arc<Vec<f64>> {
    type Key = (i32, i32, u64, [u64; 3]);
    static CACHE: OnceLock<Mutex<HashMap<Key, Arc<Vec<f64>>>>> = OnceLock::new();
    let key: Key = (max_iter, seed, glue_radius.to_bits(), force.map(f64::to_bits));
    let cache = CACHE.get_or_init(Default::default);
    if let Some(p) = cache.lock().unwrap().get(&key) {
        return p.clone();
    }
    let p = Arc::new(iterate(max_iter, seed, glue_radius, force));
    let mut c = cache.lock().unwrap();
    if c.len() >= 16 {
        c.clear();
    }
    c.insert(key, p.clone());
    p
}

fn prepare(p: &[f64]) -> Vec<f64> {
    let mut d = p.to_vec();
    for i in [0, 1, 5, 10, 11, 12] {
        d[i] = ftoi(p[i]) as f64;
    }
    d[6] = if p[6] < 0.1 { 0.1 } else { p[6] };
    for (i, hi) in [(13, 6), (14, 24), (17, 1), (19, 6), (20, 24), (23, 1)] {
        d[i] = ftoi(p[i]).clamp(0, hi) as f64;
    }
    d.resize(PTS, 0.0);
    let pts = points(d[0].max(0.0) as i32, d[1] as i32, d[6], [d[7], d[8], d[9]]);
    d[NPTS] = (pts.len() / PREC) as f64;
    d.extend_from_slice(&pts);
    d
}

/// Packs a node or junction mesh with its colormap into the block at `o`.
fn load_mesh(d: &mut Vec<f64>, o: usize, p: usize, obj: &[u8], colormap: &[u8]) {
    if obj.is_empty() {
        return;
    }
    let r = Refinement { level: d[p] as i32, smooth_passes: d[p + 1] as i32, lambda: d[p + 2], mu: d[p + 3] };
    let Some(m) = mesh::load(&MeshSource::File(text(obj)), r) else { return };
    load_image(d, o, colormap);
    pack_mesh(d, o, &m);
    // SimpleMesh.getBoundingBox (with Double.MIN_VALUE as the initial maximum)
    let min_value = f64::from_bits(1);
    let mut bb = [f64::MAX, min_value, f64::MAX, min_value, f64::MAX, min_value];
    for v in &m.vertices {
        for (k, c) in [v.x as f64, v.y as f64, v.z as f64].into_iter().enumerate() {
            bb[2 * k] = bb[2 * k].min(c);
            bb[2 * k + 1] = bb[2 * k + 1].max(c);
        }
    }
    for k in 0..3 {
        d[o + M_HEADER + k] = (bb[2 * k] + bb[2 * k + 1]) / 2.0;
    }
}

struct Sample {
    x: f64,
    y: f64,
    z: f64,
    color: f64,
}

/// `getRawSampleFromNodeMesh` / `getRawSampleFromJunctMesh`: `o` = mesh block, `p` = index of
/// the mesh parameters, `rot` = point record whose rotation applies.
fn mesh_sample(d: &[f64], ctx: &mut Ctx, o: usize, p: usize, scale: f64, default_color: f64, rot: Option<usize>) -> Sample {
    let k = ctx.random_int(d[o + M_TOTAL] as usize) as f64;
    let fs = d[o + M_FSTART] as usize;
    let (mut lo, mut hi) = (0, d[o + M_NF] as usize - 1);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if d[fs + mid * FREC + 3] > k {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let f = fs + lo * FREC;
    let vs = d[o + M_VSTART] as usize;
    let vi = [0, 1, 2].map(|i| vs + d[f + i] as usize * VREC);
    let (pre_scale, centre) = (d[p + 5], [d[o + M_HEADER], d[o + M_HEADER + 1], d[o + M_HEADER + 2]]);
    // nodeTransform (float precision)
    let pt = vi.map(|b| {
        let q = match rot {
            Some(r) => {
                let m = &d[r + 5..r + 14];
                let (x, y, z) = (d[b] - centre[0], d[b + 1] - centre[1], d[b + 2] - centre[2]);
                [x * m[0] + y * m[1] + z * m[2], x * m[3] + y * m[4] + z * m[5], x * m[6] + y * m[7] + z * m[8]]
            }
            None => [d[b], d[b + 1], d[b + 2]],
        };
        q.map(|c| (c * pre_scale * scale) as f32 as f64)
    });
    let sqrt_r1 = ctx.random().sqrt();
    let r2 = ctx.random();
    let (a, b, c) = (1.0 - sqrt_r1, sqrt_r1 * (1.0 - r2), r2 * sqrt_r1);
    let mut s = Sample {
        x: a * pt[0][0] + b * pt[1][0] + c * pt[2][0],
        y: a * pt[0][1] + b * pt[1][1] + c * pt[2][1],
        z: a * pt[0][2] + b * pt[1][2] + c * pt[2][2],
        color: default_color,
    };
    if d[o] > 0.0 && d[vi[0] + 5] != 0.0 {
        let u = a * d[vi[0] + 3] + b * d[vi[1] + 3] + c * d[vi[2] + 3];
        let w = a * d[vi[0] + 4] + b * d[vi[1] + 4] + c * d[vi[2] + 4];
        let (iw, ih) = (d[o] - 1.0, d[o + 1] - 1.0);
        let mut holder = Pt::default();
        apply_image_color(d, o, d[p + 4] > 0.0, &mut holder, (u * iw).clamp(0.0, iw), (ih - w * ih).clamp(0.0, ih));
        s.color = ctx.uv_color_idx(ftoi(holder.red), ftoi(holder.green), ftoi(holder.blue));
    }
    s
}

/// Random point on a sphere of `radius` around `q`.
fn sphere_sample(ctx: &mut Ctx, q: &[f64], radius: f64, color: f64) -> Sample {
    let phi = ctx.random() * (M_PI + M_PI);
    let theta = ctx.random() * (M_PI + M_PI);
    Sample { x: q[0] + radius * theta.sin() * phi.cos(), y: q[1] + radius * theta.sin() * phi.sin(), z: q[2] + radius * theta.cos(), color }
}

/// `displayCentreNode` (`outer` false) / `displayOuterNode`.
fn node(d: &[f64], ctx: &mut Ctx, q: usize, radius: f64, outer: bool) -> Sample {
    let color = if outer { d[q + 3] } else { 0.0 };
    let radius = radius * d[4];
    if d[NODE + M_TOTAL] < 1.0 {
        return sphere_sample(ctx, &d[q..q + 3], radius, color);
    }
    let mut s = mesh_sample(d, ctx, NODE, 13, radius, color, outer.then_some(q));
    s.x += d[q];
    s.y += d[q + 1];
    s.z += d[q + 2];
    s
}

/// `displayJunction`.
fn junction(d: &[f64], ctx: &mut Ctx, q: usize, pq: usize, min_radius: f64, max_radius: f64, rnd: f64) -> Sample {
    let radius = min_radius + (max_radius - min_radius) * rnd;
    let default_color = d[q + 3] + (d[pq + 3] - d[q + 3]) * rnd;
    let (ax, ay, az) = (d[pq] - d[q], d[pq + 1] - d[q + 1], d[pq + 2] - d[q + 2]);
    if d[JUNCT + M_TOTAL] < 1.0 {
        let mut s = Sample { x: d[q] + ax * rnd, y: d[q + 1] + ay * rnd, z: d[q + 2] + az * rnd, color: default_color };
        for _ in 0..10 {
            let (bx, by, bz) = (0.5 - ctx.random(), 0.5 - ctx.random(), 0.5 - ctx.random());
            let (cx, cy, cz) = (ay * bz - az * by, az * bx - ax * bz, ax * by - ay * bx);
            let r = (cx * cx + cy * cy + cz * cz).sqrt();
            if r > 0.001 {
                s.x += cx / r * radius;
                s.y += cy / r * radius;
                s.z += cz / r * radius;
                break;
            }
        }
        return s;
    }
    let mut s = mesh_sample(d, ctx, JUNCT, 19, radius, default_color, Some(q));
    s.x += d[q] + ax * 0.5;
    s.y += d[q + 1] + ay * 0.5;
    s.z += d[q + 2] + az * 0.5;
    s
}

fn apply(d: &[f64], ctx: &mut Ctx, v: &mut Pt, amount: f64) {
    let n = d[NPTS] as usize;
    if n == 0 {
        v.do_hide = true;
        return;
    }
    let q = PTS + ctx.random_int(n) * PREC;
    let rnd = ctx.random();
    let (inner, outer) = (d[2], d[3]);
    let do_blur = outer > EPSILON;
    let (nodes, junctions) = (d[10] > 0.0, d[11] > 0.0);
    let sample = if d[q + 4] < 0.0 {
        (nodes && do_blur).then(|| node(d, ctx, q, inner, false))
    } else {
        let pq = PTS + d[q + 4] as usize * PREC;
        if do_blur {
            let min_radius = inner + (outer - inner) * d[q + 3];
            let max_radius = inner + (outer - inner) * d[pq + 3];
            if junctions && nodes {
                if ctx.random() < 0.333 {
                    Some(node(d, ctx, q, min_radius, true))
                } else {
                    Some(junction(d, ctx, q, pq, min_radius, max_radius, rnd))
                }
            } else if junctions {
                Some(junction(d, ctx, q, pq, min_radius, max_radius, rnd))
            } else if nodes {
                Some(node(d, ctx, q, max_radius, true))
            } else {
                None
            }
        } else {
            // displayUnblurredPart
            Some(Sample {
                x: d[q] + (d[pq] - d[q]) * rnd,
                y: d[q + 1] + (d[pq + 1] - d[q + 1]) * rnd,
                z: d[q + 2] + (d[pq + 2] - d[q + 2]) * rnd,
                color: d[q + 3] + (d[pq + 3] - d[q + 3]) * rnd,
            })
        }
    };
    match sample {
        Some(s) => {
            v.do_hide = false;
            v.x += amount * s.x;
            v.y += amount * s.y;
            v.z += amount * s.z;
            if d[5] > 0.0 {
                v.color = java_max(0.0, java_min(1.0, s.color));
            }
        }
        None => v.do_hide = true,
    }
}

pub static RESOURCES: &[ResourceVar] = &[ResourceVar {
    name: "dla3d_wf",
    resources: &[
        ResDef { name: "node_obj_filename", kind: ResKind::ObjFile, default: "" },
        ResDef { name: "node_colormap_filename", kind: ResKind::ImageFile, default: "" },
        ResDef { name: "junct_obj_filename", kind: ResKind::ObjFile, default: "" },
        ResDef { name: "junct_colormap_filename", kind: ResKind::ImageFile, default: "" },
    ],
    load: |d, res, _: &LoadEnv| {
        load_mesh(d, NODE, 13, &res[0], &res[1]);
        load_mesh(d, JUNCT, 19, &res[2], &res[3]);
    },
}];

pub static DEFS: &[VarDef] = &[VarDef {
    name: "dla3d_wf",
    params: &[
        ParamDef::int("max_iter", 320.0, 0.0, 10000.0),
        ParamDef::int("seed", 1234.0, 0.0, 10000.0),
        ParamDef::new("inner_blur_radius", 0.5),
        ParamDef::new("outer_blur_radius", 0.01),
        ParamDef::new("junction_scale", 1.4),
        ParamDef::int("dc_color", 1.0, 0.0, 1.0),
        ParamDef::new("glue_radius", 1.0),
        ParamDef::new("force_x", 0.0),
        ParamDef::new("force_y", 0.0),
        ParamDef::new("force_z", 0.0),
        ParamDef::int("display_nodes", 1.0, 0.0, 1.0),
        ParamDef::int("display_junctions", 1.0, 0.0, 1.0),
        ParamDef::int("single_thread", 0.0, 0.0, 1.0),
        ParamDef::int("node_sdiv_level", 0.0, 0.0, 6.0),
        ParamDef::int("node_sdiv_smooth_passes", 12.0, 0.0, 24.0),
        ParamDef::new("node_sdiv_smooth_lambda", 0.42),
        ParamDef::new("node_sdiv_smooth_mu", -0.45),
        ParamDef::int("node_blend_colormap", 1.0, 0.0, 1.0),
        ParamDef::new("node_mesh_scale", 1.0),
        ParamDef::int("junct_sdiv_level", 0.0, 0.0, 6.0),
        ParamDef::int("junct_sdiv_smooth_passes", 12.0, 0.0, 24.0),
        ParamDef::new("junct_sdiv_smooth_lambda", 0.42),
        ParamDef::new("junct_sdiv_smooth_mu", -0.45),
        ParamDef::int("junct_blend_colormap", 1.0, 0.0, 1.0),
        ParamDef::new("junct_mesh_scale", 1.0),
    ],
    kind: VarKind::Normal,
    is_3d: true,
    apply: |d, ctx, _, v, amount| apply(d, ctx, v, amount),
    prepare: Some(|p, _| prepare(p)),
    wgsl: "struct Dla3dSample { p: vec3<f32>, color: f32 }

// getRawSampleFromNodeMesh / getRawSampleFromJunctMesh; rot = point record or 0u (no rotation).
fn dla3d_wf_mesh(base: u32, o: u32, p: u32, scale: f32, default_color: f32, rot: u32) -> Dla3dSample {
  let k = floor(rnd() * prm[base + o + 8u]);
  let fs = base + u32(prm[base + o + 10u]);
  let vs = base + u32(prm[base + o + 9u]);
  var lo = 0u;
  var hi = u32(prm[base + o + 7u]) - 1u;
  loop {
    if (lo >= hi) { break; }
    let mid = (lo + hi) / 2u;
    if (prm[fs + mid * 4u + 3u] > k) { hi = mid; } else { lo = mid + 1u; }
  }
  let f = fs + lo * 4u;
  let i1 = vs + u32(prm[f]) * 6u;
  let i2 = vs + u32(prm[f + 1u]) * 6u;
  let i3 = vs + u32(prm[f + 2u]) * 6u;
  var m = mat3x3<f32>(1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0);
  var centre = vec3<f32>(0.0);
  if (rot > 0u) {
    let r = base + rot + 5u;
    // column-major: the rows of the Java matrix become columns of the transpose
    m = transpose(mat3x3<f32>(prm[r], prm[r + 1u], prm[r + 2u], prm[r + 3u], prm[r + 4u], prm[r + 5u], prm[r + 6u], prm[r + 7u], prm[r + 8u]));
    centre = vec3<f32>(prm[base + o + 11u], prm[base + o + 12u], prm[base + o + 13u]);
  }
  let s = prm[base + p + 5u] * scale;
  let p1 = m * (vec3<f32>(prm[i1], prm[i1 + 1u], prm[i1 + 2u]) - centre) * s;
  let p2 = m * (vec3<f32>(prm[i2], prm[i2 + 1u], prm[i2 + 2u]) - centre) * s;
  let p3 = m * (vec3<f32>(prm[i3], prm[i3 + 1u], prm[i3 + 2u]) - centre) * s;
  let sqrt_r1 = sqrt(rnd());
  let r2 = rnd();
  let a = 1.0 - sqrt_r1;
  let b = sqrt_r1 * (1.0 - r2);
  let c = r2 * sqrt_r1;
  var res = Dla3dSample(a * p1 + b * p2 + c * p3, default_color);
  if (prm[base + o] > 0.0 && prm[i1 + 5u] != 0.0) {
    let u = a * prm[i1 + 3u] + b * prm[i2 + 3u] + c * prm[i3 + 3u];
    let w = a * prm[i1 + 4u] + b * prm[i2 + 4u] + c * prm[i3 + 4u];
    let iw = prm[base + o] - 1.0;
    let ih = prm[base + o + 1u] - 1.0;
    let rgb = res_image_color(base, o, prm[base + p + 4u] > 0.0, clamp(u * iw, 0.0, iw), clamp(ih - w * ih, 0.0, ih));
    res.color = res_uv_color_idx(rgb);
  }
  return res;
}

fn dla3d_wf_pt(base: u32, q: u32) -> vec3<f32> {
  return vec3<f32>(prm[base + q], prm[base + q + 1u], prm[base + q + 2u]);
}

// displayCentreNode (outer false) / displayOuterNode
fn dla3d_wf_node(base: u32, q: u32, radius0: f32, outer: bool) -> Dla3dSample {
  var color = 0.0;
  if (outer) { color = prm[base + q + 3u]; }
  let radius = radius0 * prm[base + 4u];
  if (prm[base + 25u + 8u] < 1.0) {
    let phi = rnd() * TWO_PI;
    let theta = rnd() * TWO_PI;
    return Dla3dSample(dla3d_wf_pt(base, q) + radius * vec3<f32>(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta)), color);
  }
  var res = dla3d_wf_mesh(base, 25u, 13u, radius, color, select(0u, q, outer));
  res.p += dla3d_wf_pt(base, q);
  return res;
}

fn dla3d_wf_junction(base: u32, q: u32, pq: u32, min_radius: f32, max_radius: f32, t: f32) -> Dla3dSample {
  let radius = min_radius + (max_radius - min_radius) * t;
  let default_color = prm[base + q + 3u] + (prm[base + pq + 3u] - prm[base + q + 3u]) * t;
  let p0 = dla3d_wf_pt(base, q);
  let ax = dla3d_wf_pt(base, pq) - p0;
  if (prm[base + 39u + 8u] < 1.0) {
    var res = Dla3dSample(p0 + ax * t, default_color);
    for (var i = 0; i < 10; i++) {
      let bv = vec3<f32>(0.5 - rnd(), 0.5 - rnd(), 0.5 - rnd());
      let cv = cross(ax, bv);
      let r = length(cv);
      if (r > 0.001) {
        res.p += cv / r * radius;
        break;
      }
    }
    return res;
  }
  var res = dla3d_wf_mesh(base, 39u, 19u, radius, default_color, q);
  res.p += p0 + ax * 0.5;
  return res;
}
@body
let n = u32(D(53));
if (n == 0u) {
  v.hide = 1.0;
} else {
  let q = 54u + min(u32(rnd() * f32(n)), n - 1u) * 14u;
  let t = rnd();
  let inner = D(2);
  let outer = D(3);
  let do_blur = outer > EPSILON;
  let nodes = D(10) > 0.0;
  let junctions = D(11) > 0.0;
  var s = Dla3dSample(vec3<f32>(0.0), 0.0);
  var ok = false;
  if (prm[base + q + 4u] < 0.0) {
    if (nodes && do_blur) { s = dla3d_wf_node(base, q, inner, false); ok = true; }
  } else {
    let pq = 54u + u32(prm[base + q + 4u]) * 14u;
    if (do_blur) {
      let min_radius = inner + (outer - inner) * prm[base + q + 3u];
      let max_radius = inner + (outer - inner) * prm[base + pq + 3u];
      ok = true;
      if (junctions && nodes) {
        if (rnd() < 0.333) {
          s = dla3d_wf_node(base, q, min_radius, true);
        } else {
          s = dla3d_wf_junction(base, q, pq, min_radius, max_radius, t);
        }
      } else if (junctions) {
        s = dla3d_wf_junction(base, q, pq, min_radius, max_radius, t);
      } else if (nodes) {
        s = dla3d_wf_node(base, q, max_radius, true);
      } else {
        ok = false;
      }
    } else {
      let p0 = dla3d_wf_pt(base, q);
      s = Dla3dSample(p0 + (dla3d_wf_pt(base, pq) - p0) * t, prm[base + q + 3u] + (prm[base + pq + 3u] - prm[base + q + 3u]) * t);
      ok = true;
    }
  }
  if (ok) {
    v.hide = 0.0;
    v.x += AMOUNT * s.p.x;
    v.y += AMOUNT * s.p.y;
    v.z += AMOUNT * s.p.z;
    if (D(5) > 0.0) { v.color = clamp(s.color, 0.0, 1.0); }
  } else {
    v.hide = 1.0;
  }
}",
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_constants_match_wgsl() {
        // the WGSL snippet hard-codes these offsets
        assert_eq!((NODE, JUNCT, NPTS, PTS, PREC), (25, 39, 53, 54, 14));
        assert_eq!((M_NF, M_TOTAL, M_VSTART, M_FSTART, M_HEADER, super::super::resource_vars::IMG), (7, 8, 9, 10, 11, 3));
    }
    /// Compares a cluster with a dump of JWildfire's `DLA3DWFFuncIterator` (single thread):
    /// `DLA_ARGS="max_iter seed glue fx fy fz" DLA_REF=dump.txt`, rows
    /// `x y z rel_depth parent [m10 m21 m22]`.
    #[test]
    #[ignore]
    fn cluster_matches_jwildfire() {
        let args: Vec<f64> = std::env::var("DLA_ARGS").unwrap().split_whitespace().map(|s| s.parse().unwrap()).collect();
        let reference = std::fs::read_to_string(std::env::var("DLA_REF").unwrap()).unwrap();
        let d = iterate(args[0] as i32, args[1] as i32, args[2], [args[3], args[4], args[5]]);
        let rows: Vec<Vec<f64>> = reference.lines().map(|l| l.split_whitespace().map(|s| s.parse().unwrap()).collect()).collect();
        assert_eq!(rows.len(), d.len() / PREC);
        for (i, row) in rows.iter().enumerate() {
            let p = &d[i * PREC..(i + 1) * PREC];
            let mut got = vec![p[0], p[1], p[2], p[3], p[4]];
            if p[4] >= 0.0 {
                got.extend([p[5 + 3], p[5 + 7], p[5 + 8]]);
            }
            for (g, w) in got.iter().zip(row) {
                assert!((g - w).abs() <= 1e-9 * (1.0 + w.abs()) || (g.is_nan() && w.is_nan()), "point {i}: {got:?} vs {row:?}");
            }
        }
    }
}
