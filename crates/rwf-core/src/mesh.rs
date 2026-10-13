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

//! Triangle meshes for the `obj_mesh_*` variations (`SimpleMesh`, `OBJMeshUtil`): Wavefront OBJ
//! loading, subdivision with Taubin smoothing and area-weighted face selection.
//!
//! Vertex coordinates are `f32` like the Java `Vertex`, and the arithmetic mirrors the Java
//! float/double mix so subdivided meshes match.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex, OnceLock};

use crate::math::{EPSILON, ftoi};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Texture coordinates (`VertexWithUV`).
    pub uv: Option<(f32, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub faces: Vec<[u32; 3]>,
    /// Exclusive cumulative face counts after `distributeFaces` (face `i` is repeated
    /// `cum[i] - cum[i - 1]` times in the Java face list); `None` means every face once.
    pub cum_counts: Option<Vec<u64>>,
    vertex_map: HashMap<(i32, i32, i32), u32>,
}

impl Mesh {
    /// Number of entries in the (virtually expanded) face list.
    pub fn face_count(&self) -> u64 {
        match &self.cum_counts {
            Some(c) => c.last().copied().unwrap_or(0),
            None => self.faces.len() as u64,
        }
    }

    fn vertex_key(x: f64, y: f64, z: f64) -> (i32, i32, i32) {
        const PREC: f64 = 10000.0;
        (ftoi(x * PREC), ftoi(y * PREC), ftoi(z * PREC))
    }

    /// `addVertex`: vertices are shared by position (the first one's texture coordinates win).
    pub fn add_vertex(&mut self, x: f64, y: f64, z: f64, uv: Option<(f64, f64)>) -> u32 {
        let key = Self::vertex_key(x, y, z);
        if let Some(&i) = self.vertex_map.get(&key) {
            return i;
        }
        let i = self.vertices.len() as u32;
        self.vertices.push(Vertex { x: x as f32, y: y as f32, z: z as f32, uv: uv.map(|(u, v)| (u as f32, v as f32)) });
        self.vertex_map.insert(key, i);
        i
    }

    pub fn add_face(&mut self, a: u32, b: u32, c: u32) {
        self.faces.push([a, b, c]);
    }

    /// Quad as two triangles (`addFace(p1, p2, p3, p4)`).
    pub fn add_quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.faces.push([a, b, c]);
        self.faces.push([a, c, d]);
    }

    /// `OBJMeshUtil.createDfltMesh`: a cube.
    pub fn default_cube() -> Self {
        let mut m = Self::default();
        let v: Vec<u32> = [
            [-1.0, -1.0, 1.0],
            [1.0, -1.0, 1.0],
            [1.0, 1.0, 1.0],
            [-1.0, 1.0, 1.0],
            [-1.0, -1.0, -1.0],
            [1.0, -1.0, -1.0],
            [1.0, 1.0, -1.0],
            [-1.0, 1.0, -1.0],
        ]
        .iter()
        .map(|p| m.add_vertex(p[0], p[1], p[2], None))
        .collect();
        m.add_quad(v[0], v[1], v[2], v[3]);
        m.add_quad(v[1], v[5], v[6], v[2]);
        m.add_quad(v[5], v[6], v[7], v[4]);
        m.add_quad(v[4], v[7], v[3], v[0]);
        m.add_quad(v[3], v[2], v[6], v[7]);
        m.add_quad(v[0], v[1], v[5], v[4]);
        m
    }

    /// Parses Wavefront OBJ text: `v`, `vt` and triangle/quad `f` records (other polygons are
    /// skipped, as by the oobjloader-based Java importer).
    pub fn parse_obj(text: &str) -> Self {
        let mut pos: Vec<[f32; 3]> = Vec::new();
        let mut tex: Vec<(f32, f32)> = Vec::new();
        let mut mesh = Self::default();
        let num = |s: Option<&str>| s.and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
        // 1-based, negative = relative to the end.
        let index = |s: &str, len: usize| -> Option<usize> {
            let i: i64 = s.parse().ok()?;
            let i = if i < 0 { len as i64 + i } else { i - 1 };
            (0..len as i64).contains(&i).then_some(i as usize)
        };
        for line in text.lines() {
            let mut it = line.split_whitespace();
            match it.next() {
                Some("v") => pos.push([num(it.next()), num(it.next()), num(it.next())]),
                Some("vt") => tex.push((num(it.next()), num(it.next()))),
                Some("f") => {
                    let refs: Vec<(usize, Option<usize>)> = it
                        .filter_map(|r| {
                            let mut p = r.split('/');
                            let v = index(p.next()?, pos.len())?;
                            let t = p.next().filter(|s| !s.is_empty()).and_then(|s| index(s, tex.len()));
                            Some((v, t))
                        })
                        .collect();
                    if refs.len() != 3 && refs.len() != 4 {
                        continue;
                    }
                    let with_uv = refs.iter().all(|r| r.1.is_some());
                    let ids: Vec<u32> = refs
                        .iter()
                        .map(|&(v, t)| {
                            let p = pos[v];
                            let uv = if with_uv { t.map(|t| (tex[t].0 as f64, tex[t].1 as f64)) } else { None };
                            mesh.add_vertex(p[0] as f64, p[1] as f64, p[2] as f64, uv)
                        })
                        .collect();
                    mesh.add_face(ids[0], ids[1], ids[2]);
                    if ids.len() == 4 {
                        mesh.add_face(ids[0], ids[2], ids[3]);
                    }
                }
                _ => {}
            }
        }
        mesh
    }

    /// `loadAndSmoothMeshFromFile` after parsing: subdivide + smooth, or distribute the faces
    /// by area when not subdivided.
    pub fn refine(mut self, subdiv_level: i32, smooth_passes: i32, lambda: f64, mu: f64) -> Self {
        // Guard against runaway memory use (every level quadruples the face count).
        const MAX_FACES: usize = 8_000_000;
        if subdiv_level > 0 {
            for _ in 0..subdiv_level {
                if self.faces.len() * 4 > MAX_FACES {
                    break;
                }
                self = self.interpolate();
                self.taubin_smooth(smooth_passes, lambda, mu);
            }
        } else {
            self.distribute_faces();
        }
        self
    }

    /// `distributeFaces`: larger faces are picked proportionally more often.
    fn distribute_faces(&mut self) {
        let areas: Vec<f64> = self
            .faces
            .iter()
            .map(|f| {
                let (p1, p2, p3) = (self.vertices[f[0] as usize], self.vertices[f[1] as usize], self.vertices[f[2] as usize]);
                let a = [(p2.x - p1.x) as f64, (p2.y - p1.y) as f64, (p2.z - p1.z) as f64];
                let b = [(p3.x - p1.x) as f64, (p3.y - p1.y) as f64, (p3.z - p1.z) as f64];
                let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
            })
            .collect();
        let area_min = areas.iter().copied().fold(f64::MAX, f64::min);
        let area_max = areas.iter().copied().fold(0.0, f64::max);
        if (area_min - area_max).abs() > EPSILON {
            let max_faces = if self.faces.len() < 5000 { 5000 } else { 500 };
            let mut total = 0u64;
            self.cum_counts = Some(
                areas
                    .iter()
                    .map(|&a| {
                        total += ftoi(a / area_min).clamp(0, max_faces) as u64;
                        total
                    })
                    .collect(),
            );
        }
    }

    /// `NeightboursList`: neighbours per vertex in insertion order.
    fn neighbours(&self) -> Vec<Vec<u32>> {
        let mut n: Vec<Vec<u32>> = vec![Vec::new(); self.vertices.len()];
        let mut add = |from: u32, to: u32| {
            let l = &mut n[from as usize];
            if !l.contains(&to) {
                l.push(to);
            }
        };
        for f in &self.faces {
            add(f[0], f[1]);
            add(f[0], f[2]);
            add(f[1], f[0]);
            add(f[1], f[2]);
            add(f[2], f[0]);
            add(f[2], f[1]);
        }
        n
    }

    fn laplace_smooth(&mut self, neighbours: &[Vec<u32>], strength: f64) {
        let disp: Vec<[f32; 3]> = (0..self.vertices.len())
            .map(|i| {
                let mut d = [0.0f32; 3];
                let nl = &neighbours[i];
                if nl.len() > 1 {
                    let w = 1.0 / nl.len() as f64;
                    let v = self.vertices[i];
                    for &j in nl {
                        let vn = self.vertices[j as usize];
                        d[0] = (d[0] as f64 + (vn.x - v.x) as f64 * w) as f32;
                        d[1] = (d[1] as f64 + (vn.y - v.y) as f64 * w) as f32;
                        d[2] = (d[2] as f64 + (vn.z - v.z) as f64 * w) as f32;
                    }
                }
                d
            })
            .collect();
        for (v, d) in self.vertices.iter_mut().zip(disp) {
            v.x = (v.x as f64 + d[0] as f64 * strength) as f32;
            v.y = (v.y as f64 + d[1] as f64 * strength) as f32;
            v.z = (v.z as f64 + d[2] as f64 * strength) as f32;
        }
    }

    fn taubin_smooth(&mut self, passes: i32, lambda: f64, mu: f64) {
        let n = self.neighbours();
        for _ in 0..passes {
            self.laplace_smooth(&n, lambda);
            self.laplace_smooth(&n, mu);
        }
    }

    /// One subdivision step: every triangle becomes four (`SimpleMesh.interpolate`).
    fn interpolate(&self) -> Self {
        let mut m = Self::default();
        let mid = |a: f32, b: f32| (a as f64 + (b - a) as f64 * 0.5) as f32;
        let half = |a: &Vertex, b: &Vertex| Vertex {
            x: mid(a.x, b.x),
            y: mid(a.y, b.y),
            z: mid(a.z, b.z),
            uv: match (a.uv, b.uv) {
                (Some(p), Some(q)) => Some((mid(p.0, q.0), mid(p.1, q.1))),
                _ => None,
            },
        };
        for f in &self.faces {
            let (v1, v2, v3) = (&self.vertices[f[0] as usize], &self.vertices[f[1] as usize], &self.vertices[f[2] as usize]);
            let with_uv = v1.uv.is_some() && v2.uv.is_some() && v3.uv.is_some();
            let ids: Vec<u32> = [*v1, *v2, *v3, half(v1, v2), half(v2, v3), half(v3, v1)]
                .iter()
                .map(|v| {
                    let uv = if with_uv { v.uv.map(|(u, w)| (u as f64, w as f64)) } else { None };
                    m.add_vertex(v.x as f64, v.y as f64, v.z as f64, uv)
                })
                .collect();
            m.add_face(ids[0], ids[3], ids[5]);
            m.add_face(ids[3], ids[1], ids[4]);
            m.add_face(ids[4], ids[2], ids[5]);
            m.add_face(ids[3], ids[4], ids[5]);
        }
        m
    }
}

/// Built-in meshes of `obj_mesh_primitive_wf`, in parameter order.
pub const PRIMITIVES: &[&str] = &[
    "ball",
    "capsule",
    "cone",
    "diamond",
    "torus",
    "box",
    "gear15",
    "icosahedron",
    "tetrahedron",
    "octahedron",
    "dodecahedron",
    "wedge",
    "icosidodecahedron",
    "cubeoctahedron",
    "gears6a",
    "gears6s",
    "gears8a",
    "gears8s",
    "gears12a",
    "gears12s",
    "gears16a",
    "gears16s",
    "gears24a",
    "gears24s",
    "mandelbulb",
    "drop",
];

/// zlib-compressed OBJ text of a built-in mesh (vertex normals stripped, they are not used).
fn primitive_source(name: &str) -> Option<&'static [u8]> {
    macro_rules! prims {
        ($($n:literal),*) => {
            match name {
                $($n => Some(include_bytes!(concat!("../assets/meshes/", $n, ".obj.zz")).as_slice()),)*
                _ => None,
            }
        };
    }
    prims!(
        "ball",
        "capsule",
        "cone",
        "diamond",
        "torus",
        "box",
        "gear15",
        "icosahedron",
        "tetrahedron",
        "octahedron",
        "dodecahedron",
        "wedge",
        "icosidodecahedron",
        "cubeoctahedron",
        "gears6a",
        "gears6s",
        "gears8a",
        "gears8s",
        "gears12a",
        "gears12s",
        "gears16a",
        "gears16s",
        "gears24a",
        "gears24s",
        "mandelbulb",
        "drop"
    )
}

/// Where a mesh comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MeshSource {
    File(String),
    Primitive(usize),
}

/// Subdivision settings (part of the cache key, see `OBJMeshUtil.getMeshname`).
#[derive(Clone, Copy, Debug)]
pub struct Refinement {
    pub level: i32,
    pub smooth_passes: i32,
    pub lambda: f64,
    pub mu: f64,
}

/// Loads (and caches) a refined mesh; `None` if the source cannot be read.
pub fn load(src: &MeshSource, r: Refinement) -> Option<Arc<Mesh>> {
    type Key = (MeshSource, i32, i32, u64, u64);
    static CACHE: OnceLock<Mutex<HashMap<Key, Arc<Mesh>>>> = OnceLock::new();
    let key: Key = (src.clone(), r.level, r.smooth_passes, r.lambda.to_bits(), r.mu.to_bits());
    let cache = CACHE.get_or_init(Default::default);
    if let Some(m) = cache.lock().unwrap().get(&key) {
        return Some(m.clone());
    }
    let text = match src {
        MeshSource::File(path) => String::from_utf8_lossy(&std::fs::read(path).ok()?).into_owned(),
        MeshSource::Primitive(i) => {
            let mut s = String::new();
            flate2::read::ZlibDecoder::new(primitive_source(PRIMITIVES.get(*i)?)?).read_to_string(&mut s).ok()?;
            s
        }
    };
    let mesh = Arc::new(Mesh::parse_obj(&text).refine(r.level, r.smooth_passes, r.lambda, r.mu));
    cache.lock().unwrap().insert(key, mesh.clone());
    Some(mesh)
}

/// Mesh built by a variation (e.g. from formulas), cached by a key describing all inputs.
pub fn cached(key: String, build: impl FnOnce() -> Mesh) -> Arc<Mesh> {
    type Entry = (String, Arc<Mesh>);
    static CACHE: OnceLock<Mutex<Vec<Entry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, m)) = cache.lock().unwrap().iter().find(|(k, _)| *k == key) {
        return m.clone();
    }
    let m = Arc::new(build());
    let mut c = cache.lock().unwrap();
    if c.len() >= 16 {
        c.remove(0);
    }
    c.push((key, m.clone()));
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_SUBDIV: Refinement = Refinement { level: 0, smooth_passes: 12, lambda: 0.42, mu: -0.45 };

    #[test]
    fn parses_quads_uvs_and_shares_vertices() {
        let m = Mesh::parse_obj("v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nf 1/1 2/2 3/3 4/4\nf -4 -2 -1\nf 1 2 3 4 1\n");
        assert_eq!(m.vertices.len(), 4);
        assert_eq!(m.faces, vec![[0, 1, 2], [0, 2, 3], [0, 2, 3]]);
        assert_eq!(m.vertices[2].uv, Some((1.0, 1.0)));
    }

    #[test]
    fn distributes_faces_by_area() {
        // A unit triangle and one twice as large.
        let mut m = Mesh::parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\nf 1 4 5\n");
        m.distribute_faces();
        assert_eq!(m.cum_counts, Some(vec![1, 5]));
        assert_eq!(m.face_count(), 5);
    }

    #[test]
    fn subdivision_quadruples_faces() {
        let m = Mesh::default_cube().refine(2, 2, 0.42, -0.45);
        assert_eq!(m.faces.len(), 12 * 16);
        assert!(m.cum_counts.is_none());
    }

    #[test]
    fn all_primitives_load() {
        for (i, name) in PRIMITIVES.iter().enumerate() {
            if *name == "mandelbulb" {
                continue; // large and slow to parse in debug builds
            }
            let m = load(&MeshSource::Primitive(i), NO_SUBDIV).unwrap_or_else(|| panic!("{name}"));
            assert!(m.face_count() > 0, "{name}");
        }
    }
}
