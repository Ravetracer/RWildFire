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

//! Tube meshes around formula curves: `knots3D` (`Knots3DFunc.Knot`) and `sattractor3D`
//! (`Strange3DFunc.AttractorCurve`). The vertices go through [`Mesh::add_vertex`] like in
//! JWildfire (shared by position, stored as `f32`), which the construction relies on.

use crate::mesh::Mesh;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
/// `Vector3.normalize` (`multiplyScalar(1 / length)`)
fn normalize(a: V3) -> V3 {
    scale(a, 1.0 / dot(a, a).sqrt())
}

fn vertex(m: &Mesh, i: usize) -> Option<V3> {
    m.vertices.get(i).map(|v| [v.x as f64, v.y as f64, v.z as f64])
}

/// Drops faces that point past the vertex list (where JWildfire would fail).
fn finish(mut m: Mesh) -> Mesh {
    let n = m.vertices.len() as u32;
    m.faces.retain(|f| f.iter().all(|&i| i < n));
    m
}

/// `Knot.initrotate` / `rotate`: rotation through `angle` about the axis `start + l * dir`.
struct Rotation([f64; 12]);

impl Rotation {
    fn new(start: V3, dir: V3, angle: f64) -> Self {
        let [a1, a2, a3] = start;
        let [v1, v2, v3] = dir;
        let (cal, sal) = (angle.cos(), angle.sin());
        let cal1 = 1.0 - cal;
        let rho = (v1 * v1 + v2 * v2 + v3 * v3).sqrt();
        let pi = 4.0 * 1f64.atan();
        let (theta, cph, sph) = if rho == 0.0 {
            (0.0, 1.0, 0.0)
        } else {
            let mut theta = if v1 == 0.0 {
                if v2 >= 0.0 { 0.5 * pi } else { 1.5 * pi }
            } else {
                (v2 / v1).atan()
            };
            if v1 != 0.0 && v1 < 0.0 {
                theta += pi;
            }
            let cph = v3 / rho;
            (theta, cph, (1.0 - cph * cph).sqrt())
        };
        let (cth, sth) = (theta.cos(), theta.sin());
        let (cph2, cth2) = (cph * cph, cth * cth);
        let (sph2, sth2) = (1.0 - cph2, 1.0 - cth2);
        let r11 = (cal * cph2 + sph2) * cth2 + cal * sth2;
        let r12 = sal * cph + cal1 * sph2 * cth * sth;
        let r13 = sph * (cph * cth * cal1 - sal * sth);
        let r21 = sph2 * cth * sth * cal1 - sal * cph;
        let r22 = sth2 * (cal * cph2 + sph2) + cal * cth2;
        let r23 = sph * (cph * sth * cal1 + sal * cth);
        let r31 = sph * (cph * cth * cal1 + sal * sth);
        let r32 = sph * (cph * sth * cal1 - sal * cth);
        let r33 = cal * sph2 + cph2;
        let r41 = a1 - a1 * r11 - a2 * r21 - a3 * r31;
        let r42 = a2 - a1 * r12 - a2 * r22 - a3 * r32;
        let r43 = a3 - a1 * r13 - a2 * r23 - a3 * r33;
        Self([r11, r12, r13, r21, r22, r23, r31, r32, r33, r41, r42, r43])
    }
    fn apply(&self, p: V3) -> V3 {
        let r = &self.0;
        [
            p[0] * r[0] + p[1] * r[3] + p[2] * r[6] + r[9],
            p[0] * r[1] + p[1] * r[4] + p[2] * r[7] + r[10],
            p[0] * r[2] + p[1] * r[5] + p[2] * r[8] + r[11],
        ]
    }
}

/// `Knot.buildMesh`: a tube with `facets` sides and radius `radius` through the curve points
/// `pt(0) .. pt(count - 1)`; each circle is the previous one rotated about the bisecting axis.
pub fn knot(pt: impl Fn(i32) -> V3, count: i32, radius: f64, facets: i32) -> Mesh {
    let mut mesh = Mesh::default();
    if count < 3 || facets < 2 {
        return mesh;
    }
    let zero = |x: f64| x.abs() < 1e-5;
    let pi = 4.0 * 1f64.atan();
    let (c0, c1, c2) = (pt(0), pt(1), pt(2));
    let n = facets as usize;
    let r = if radius.abs() < 0.01 { 0.01 } else { radius };
    let [mut a, mut b, mut c] = sub(c2, c0);
    let mut d = a * c1[0] + b * c1[1] + c * c1[2];
    let rv = if zero(a) && zero(b) { [0.0, c, -b] } else { [b, -a, 0.0] };
    let rv = scale(rv, 1.0 / dot(rv, rv).sqrt());
    mesh.add_vertex(c1[0] + rv[0] * r, c1[1] + rv[1] * r, c1[2] + rv[2] * r, None);
    let rot = Rotation::new(c1, [a, b, c], 2.0 * pi / n as f64);
    for i in 1..n {
        let Some(v) = vertex(&mesh, i - 1) else { return Mesh::default() };
        let p = rot.apply(v);
        mesh.add_vertex(p[0], p[1], p[2], None);
    }
    for j in 1..count as usize {
        let jn0 = j * n - n;
        let (pa, pb) = (pt(j as i32 - 1), pt(j as i32));
        let dv = sub(pb, pa);
        let c1 = a * a + b * b + c * c;
        let c2 = a * dv[0] + b * dv[1] + c * dv[2];
        let c0 = d - a * pa[0] - b * pa[1] - c * pa[2];
        let m = scale([pa[0] + pb[0], pa[1] + pb[1], pa[2] + pb[2]], 0.5);
        let d0 = dot(dv, m);
        let e1 = dv[0] * a + dv[1] * b + dv[2] * c;
        let e2 = dot(dv, dv);
        let e0 = d0 - dot(dv, pa);
        let denom = c1 * e2 - c2 * e1;
        if denom.abs() < 1e-12 {
            // direction does not change: translation
            for i in 0..n {
                let Some(v) = vertex(&mesh, jn0 + i) else { return Mesh::default() };
                mesh.add_vertex(v[0], v[1], v[2], None);
            }
        } else {
            let lambda = (c0 * e2 - c2 * e0) / denom;
            let mu = (c1 * e0 - c0 * e1) / denom;
            let p = [pa[0] + lambda * a + mu * dv[0], pa[1] + lambda * b + mu * dv[1], pa[2] + lambda * c + mu * dv[2]];
            let (ap, bp) = (sub(pa, p), sub(pb, p));
            let v = [ap[1] * bp[2] - bp[1] * ap[2], bp[0] * ap[2] - ap[0] * bp[2], ap[0] * bp[1] - bp[0] * ap[1]];
            let cosphi = dot(ap, bp) / (dot(ap, ap) * dot(bp, bp)).sqrt();
            let phi = if cosphi == 0.0 { 0.5 * pi } else { ((1.0 - cosphi * cosphi).sqrt() / cosphi).atan() };
            let rot = Rotation::new(p, v, phi);
            for i in 0..n {
                let Some(q) = vertex(&mesh, jn0 + i) else { return Mesh::default() };
                let q = rot.apply(q);
                mesh.add_vertex(q[0], q[1], q[2], None);
            }
            [a, b, c] = Rotation::new([0.0; 3], v, phi).apply([a, b, c]);
            let xc = pt(j as i32);
            d = a * xc[0] + b * xc[1] + c * xc[2];
        }
    }
    for j in 1..count as u32 {
        let n = n as u32;
        let jn = j * n + 1;
        let jn0 = jn - n;
        for i in 0..n - 1 {
            mesh.add_face(jn + i, jn0 + i - 1, jn0 + i);
            mesh.add_face(jn0 + i - 1, jn + i, jn + i - 1);
        }
        mesh.add_face(jn - 1, jn0 + n - 2, jn0 - 1);
        mesh.add_face(jn0 + n - 2, jn - 1, jn + n - 2);
    }
    finish(mesh)
}

/// `AttractorCurve.build` + `getGeometry`. `eval(k, p, dt)` is `evaluateX/Y/Z`:
/// `p[k] + f_k(p) * dt`. Integration updates one coordinate after the other (as JWildfire
/// does), then a tube is built around the path.
pub fn attractor(eval: impl Fn(usize, V3, f64) -> f64, start: V3, count: i32, warmup: i32, time_step: f64, radius: f64, facets: i32) -> Mesh {
    let mut mesh = Mesh::default();
    if count < 2 || facets < 1 {
        return mesh;
    }
    let step = |mut p: V3, dt: f64| {
        for k in 0..3 {
            p[k] = eval(k, p, dt);
        }
        p
    };
    let dt = time_step / 2.0;
    let mut p = start;
    for _ in 0..warmup {
        p = step(p, dt);
    }
    let (mut pos, mut tangents, mut normals) = (Vec::new(), Vec::new(), Vec::new());
    for i in 0..count {
        let old = p;
        let mut tangent = [0.0; 3];
        for _ in 0..2 {
            p = step(p, dt);
            tangent = normalize(sub(p, old));
        }
        // later normals: the formulas evaluated at the unchanged point
        let raw = if i == 0 { [1.0, 0.0, 0.0] } else { sub([0, 1, 2].map(|k| eval(k, p, time_step)), p) };
        // projectOnPlane(tangent)
        let proj = scale(tangent, dot(tangent, raw) / dot(tangent, tangent));
        normals.push(normalize(sub(raw, proj)));
        pos.push(p);
        tangents.push(tangent);
    }
    // get2DShape
    let fac = facets as f64;
    let shape: Vec<(f64, f64)> = (0..facets).map(|i| i as f64 / (fac / 2.0) * std::f64::consts::PI).map(|a| (a.cos() * radius, a.sin() * radius)).collect();
    let sc = shape.len() as u32;
    for i in 0..count as usize {
        let binormal = normalize(cross(tangents[i], normals[i]));
        for &(sx, sy) in &shape {
            let q = [0, 1, 2].map(|k| pos[i][k] + sx * normals[i][k] + sy * binormal[k]);
            mesh.add_vertex(q[0], q[1], q[2], None);
        }
    }
    for i in 0..count as u32 - 1 {
        for j in 0..sc {
            let p1 = i * sc + j;
            let p2 = if j == sc - 1 { p1 - (sc - 1) } else { p1 + 1 };
            let p1n = p1 + sc;
            let p2n = if j == sc - 1 { p1n - (sc - 1) } else { p1n + 1 };
            mesh.add_face(p1, p2, p1n);
            mesh.add_face(p1n, p2, p2n);
        }
    }
    // caps (the index arithmetic counts vertices as if none were shared, as in JWildfire)
    let (first, last) = (pos[0], pos[pos.len() - 1]);
    mesh.add_vertex(first[0], first[1], first[2], None);
    mesh.add_vertex(last[0], last[1], last[2], None);
    let pv = count as u32 * sc + 2;
    for j in 0..sc {
        let p2 = if j == sc - 1 { 0 } else { j + 1 };
        mesh.add_face(p2, j, pv - 2);
        let p1 = j + pv - sc - 2;
        let p2 = if j == sc - 1 { pv - sc - 2 } else { p1 + 1 };
        mesh.add_face(p1, p2, pv - 1);
    }
    finish(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knot_tube_around_circle() {
        let steps = 200;
        let m = knot(
            |s| {
                let t = 2.0 * std::f64::consts::PI * s as f64 / steps as f64;
                [10.0 * t.cos(), 10.0 * t.sin(), 0.0]
            },
            steps,
            1.0,
            4,
        );
        assert_eq!(m.vertices.len(), 200 * 4);
        assert_eq!(m.faces.len(), 199 * 8);
        // every vertex lies about 1 away from the circle of radius 10
        for v in &m.vertices {
            let r = ((v.x as f64).hypot(v.y as f64) - 10.0).hypot(v.z as f64);
            assert!((r - 1.0).abs() < 0.05, "{r}");
        }
    }
}
