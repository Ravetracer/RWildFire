// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// Ported from JWildfire - an image and animation processor written in Java
// Copyright (C) 1995-2026 Andreas Maschke
//
// and from Taprats - an interactive design tool for computer-generated Islamic patterns
// Copyright (C) 2000 Craig S. Kaplan, 2010 Pierre Baillargeon
//
// Unlike the rest of RWildFire (LGPL), this file is a port of Taprats and is licensed under
// the GNU General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version. It is only compiled with
// the optional `taprats` feature; builds with it are covered by the GPL as a whole.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! `taprats` (`TapratsFunc`, Jesus Sosa): Islamic star patterns built by Craig S. Kaplan's
//! Taprats (`csk.taprats`) and drawn as lines and triangles (`DrawFunc`).
//!
//! The pattern is built once per parameter set in `prepare`: a tiling (`assets/taprats/
//! tilings.txt`, from `Tiling.java`) gets a rosette or star in every regular feature with more
//! than four sides (optionally inferred figures in the others), the figures are repeated over
//! the view `[-1.5, 1.5]²` into one planar map, and a render style turns the map into
//! primitives. `d` holds them as records `[type (2 line, 3 triangle), color, x1, y1, x2, y2, x3,
//! y3]`; `transform` picks one at random and plots a point on it.
//!
//! JWildfire keeps the figures in a `Hashtable` keyed by object identity, so their order (and
//! with it the order of the primitives) changes from run to run there; here it is the order of
//! the tiling's features.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::draw::triangle_point;
use super::{Ctx, ParamDef, Pt, VarDef, VarKind};
use crate::math::*;
use crate::rng::JavaRandom;

/// `Loose.TOL`, `Loose.TOL2`
const TOL: f64 = 1e-7;
const TOL2: f64 = 1e-10;

fn zero(a: f64) -> bool {
    a.abs() < TOL
}

fn loose_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < TOL
}

/// Java `Math.min` / `Math.max` for doubles (NaN wins).
fn jmin(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

fn jmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

// ---------------------------------------------------------------------------------------------
// geometry.Point, geometry.Transform, geometry.Intersect

#[derive(Clone, Copy, Debug, PartialEq)]
struct P {
    x: f64,
    y: f64,
}

const fn p(x: f64, y: f64) -> P {
    P { x, y }
}

impl P {
    fn add(self, o: P) -> P {
        p(self.x + o.x, self.y + o.y)
    }
    fn sub(self, o: P) -> P {
        p(self.x - o.x, self.y - o.y)
    }
    fn scale(self, r: f64) -> P {
        p(self.x * r, self.y * r)
    }
    fn dot(self, o: P) -> f64 {
        self.x * o.x + self.y * o.y
    }
    fn cross(self, o: P) -> f64 {
        (self.x * o.y) - (self.y * o.x)
    }
    fn mag2(self) -> f64 {
        self.x * self.x + self.y * self.y
    }
    fn mag(self) -> f64 {
        self.mag2().sqrt()
    }
    fn dist2(self, o: P) -> f64 {
        let (dx, dy) = (self.x - o.x, self.y - o.y);
        dx * dx + dy * dy
    }
    fn dist(self, o: P) -> f64 {
        self.dist2(o).sqrt()
    }
    fn normalize(self) -> P {
        let m = self.mag();
        if m != 0.0 { self.scale(1.0 / m) } else { self }
    }
    fn perp(self) -> P {
        p(-self.y, self.x)
    }
    fn convex_sum(self, o: P, t: f64) -> P {
        let mt = 1.0 - t;
        p(mt * self.x + t * o.x, mt * self.y + t * o.y)
    }
    fn angle(self) -> f64 {
        self.y.atan2(self.x)
    }
    fn angle_to(self, o: P) -> f64 {
        (o.y - self.y).atan2(o.x - self.x)
    }
    fn sweep(self, from: P, to: P) -> f64 {
        let mut res = self.angle_to(to) - self.angle_to(from);
        while res < 0.0 {
            res += 2.0 * M_PI;
        }
        res
    }
    fn dist_to_line(self, a: P, b: P) -> f64 {
        let qmp = b.sub(a);
        let t = self.sub(a).dot(qmp) / qmp.dot(qmp);
        if (0.0..=1.0).contains(&t) {
            let (ox, oy) = (a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
            ((self.x - ox) * (self.x - ox) + (self.y - oy) * (self.y - oy)).sqrt()
        } else if t < 0.0 {
            a.dist(self)
        } else {
            b.dist(self)
        }
    }
    /// `Loose.equals(Point, Point)`
    fn near(self, o: P) -> bool {
        self.dist2(o) < TOL2
    }
}

/// Unit vector at `frac` of a full turn (`RadialFigure.getArc`).
fn arc(frac: f64) -> P {
    let ang = frac * 2.0 * M_PI;
    p(ang.cos(), ang.sin())
}

#[derive(Clone, Copy, Debug)]
struct Tf {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Tf {
    const fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Tf { a, b, c, d, e, f }
    }
    fn translate(t: P) -> Self {
        Tf::new(1.0, 0.0, t.x, 0.0, 1.0, t.y)
    }
    fn scale(xs: f64, ys: f64) -> Self {
        Tf::new(xs, 0.0, 0.0, 0.0, ys, 0.0)
    }
    fn rotate(t: f64) -> Self {
        Tf::new(t.cos(), -t.sin(), 0.0, t.sin(), t.cos(), 0.0)
    }
    fn rotate_around(pt: P, t: f64) -> Self {
        Tf::translate(pt).compose(&Tf::rotate(t).compose(&Tf::translate(p(-pt.x, -pt.y))))
    }
    fn compose(&self, o: &Tf) -> Tf {
        Tf::new(
            self.a * o.a + self.b * o.d,
            self.a * o.b + self.b * o.e,
            self.a * o.c + self.b * o.f + self.c,
            self.d * o.a + self.e * o.d,
            self.d * o.b + self.e * o.e,
            self.d * o.c + self.e * o.f + self.f,
        )
    }
    fn apply(&self, v: P) -> P {
        p(self.a * v.x + self.b * v.y + self.c, self.d * v.x + self.e * v.y + self.f)
    }
    fn invert(&self) -> Option<Tf> {
        let det = self.a * self.e - self.b * self.d;
        if det == 0.0 {
            return None;
        }
        let (a, b, c, d, e, f) = (self.a, self.b, self.c, self.d, self.e, self.f);
        Some(Tf::new(e / det, -b / det, (b * f - c * e) / det, -d / det, a / det, (c * d - a * f) / det))
    }
}

/// `Intersect.getIntersectionParams`
fn intersection_params(p1: P, q1: P, p2: P, q2: P) -> Option<(f64, f64)> {
    let (d1x, d1y) = (q1.x - p1.x, q1.y - p1.y);
    let (d2x, d2y) = (q2.x - p2.x, q2.y - p2.y);
    let det = (d1x * d2y) - (d1y * d2x);
    if zero(det) {
        return None;
    }
    let is = -((p1.x * d2y) + p2.x * (p1.y - q2.y) + q2.x * (p2.y - p1.y)) / det;
    let it = ((p1.x * (p2.y - q1.y)) + q1.x * (p1.y - p2.y) + p2.x * d1y) / det;
    if !(-TOL..=1.0 + TOL).contains(&is) || !(-TOL..=1.0 + TOL).contains(&it) {
        return None;
    }
    Some((is, it))
}

/// `Intersect.getIntersection`
fn intersection(p1: P, q1: P, p2: P, q2: P) -> Option<P> {
    intersection_params(p1, q1, p2, q2).map(|(s, _)| p1.convex_sum(q1, s))
}

/// `Intersect.getTrueIntersection`: no intersections at shared end points.
fn true_intersection(p1: P, q1: P, p2: P, q2: P) -> Option<P> {
    let (s, t) = intersection_params(p1, q1, p2, q2)?;
    let at_end = |t: f64| t < TOL || (1.0 - t) < TOL;
    if (s < TOL || (1.0 - s) < TOL) && at_end(t) {
        return None;
    }
    Some(p1.convex_sum(q1, s))
}

/// `general.Sort.quickSort` (Hoare partition), which decides the order of elements that
/// compare equal within the tolerance.
fn quick_sort(v: &mut [usize], i1: isize, i2: isize, cmp: &dyn Fn(usize, usize) -> i32) {
    if i1 < i2 {
        let Some(q) = partition(v, i1, i2, cmp) else { return };
        quick_sort(v, i1, q, cmp);
        quick_sort(v, q + 1, i2, cmp);
    }
}

fn partition(v: &mut [usize], i1: isize, i2: isize, cmp: &dyn Fn(usize, usize) -> i32) -> Option<isize> {
    let x = v[i1 as usize];
    let (mut i, mut j) = (i1 - 1, i2 + 1);
    loop {
        loop {
            j -= 1;
            if j < i1 {
                return None;
            }
            if cmp(v[j as usize], x) <= 0 {
                break;
            }
        }
        loop {
            i += 1;
            if i > i2 {
                return None;
            }
            if cmp(v[i as usize], x) >= 0 {
                break;
            }
        }
        if i < j {
            v.swap(i as usize, j as usize);
        } else {
            return Some(j);
        }
    }
}

fn lex_cmp(d: f64) -> i32 {
    if zero(d) {
        0
    } else if d < 0.0 {
        -1
    } else {
        1
    }
}

/// `Map.lexComparePoints`
fn lex_cmp_points(a: P, b: P) -> i32 {
    let dx = a.x - b.x;
    if zero(dx) { lex_cmp(a.y - b.y) } else { lex_cmp(dx) }
}

// ---------------------------------------------------------------------------------------------
// geometry.Map, Vertex, Edge: a planar map. Vertices and edges live in arenas; `vlist` and
// `elist` are the map's sorted lists, a vertex's `ring` its neighbour cycle starting at the
// head (`Vertex.edges`) with the angles as stored by JWildfire (not updated on every move).

#[derive(Clone, Copy)]
struct Nb {
    edge: usize,
    angle: f64,
}

#[derive(Clone)]
struct Vert {
    pos: P,
    ring: Vec<Nb>,
}

#[derive(Clone, Copy)]
struct Ed {
    v1: usize,
    v2: usize,
}

#[derive(Default)]
struct Map {
    verts: Vec<Vert>,
    edges: Vec<Ed>,
    vlist: Vec<usize>,
    elist: Vec<usize>,
}

/// `neighbour.pseudoAtan2`
fn pseudo_atan2(x: f64, y: f64) -> f64 {
    if x > 0.0 {
        if y >= 0.0 { y * 0.25 } else { 1.0 + (y * 0.25) }
    } else if y > 0.0 {
        0.25 * (2.0 - y)
    } else {
        0.5 - (y * 0.25)
    }
}

impl Map {
    fn pos(&self, v: usize) -> P {
        self.verts[v].pos
    }

    fn other(&self, e: usize, v: usize) -> usize {
        let ed = self.edges[e];
        if v == ed.v1 { ed.v2 } else { ed.v1 }
    }

    fn min_x(&self, e: usize) -> f64 {
        let ed = self.edges[e];
        jmin(self.pos(ed.v1).x, self.pos(ed.v2).x)
    }

    fn degree(&self, v: usize) -> usize {
        self.verts[v].ring.len()
    }

    fn new_vertex(&mut self, pos: P) -> usize {
        self.verts.push(Vert { pos, ring: Vec::new() });
        self.verts.len() - 1
    }

    fn new_edge(&mut self, v1: usize, v2: usize) -> usize {
        self.edges.push(Ed { v1, v2 });
        self.edges.len() - 1
    }

    fn nb_angle(&self, v: usize, e: usize) -> f64 {
        let pd = self.pos(self.other(e, v)).sub(self.pos(v)).normalize();
        pseudo_atan2(pd.x, pd.y)
    }

    /// `Vertex.insertEdge`: into the ring, sorted by angle.
    fn v_insert_edge(&mut self, v: usize, e: usize) {
        let nb = Nb { edge: e, angle: self.nb_angle(v, e) };
        let ring = &mut self.verts[v].ring;
        let n = ring.len();
        if n == 0 {
            ring.push(nb);
            return;
        }
        let a = nb.angle;
        let mut acur = ring[0].angle;
        for i in 0..n {
            let ncur = ring[(i + 1) % n].angle;
            let here = if ncur > acur { acur <= a && a <= ncur } else { acur <= a || a <= ncur };
            if here {
                ring.insert(i + 1, nb);
                return;
            }
            acur = ncur;
        }
        // JWildfire throws here (only possible with NaN angles)
        ring.push(nb);
    }

    /// `Vertex.getBeforeAndAfter`: the ring neighbours of edge `e` at `v`.
    fn v_before_after(&self, v: usize, e: usize) -> Option<(usize, usize)> {
        let ring = &self.verts[v].ring;
        let n = ring.len();
        (1..=n).map(|k| k % n).find(|&j| ring[j].edge == e).map(|j| (ring[(j + n - 1) % n].edge, ring[(j + 1) % n].edge))
    }

    /// `Vertex.findNeighbour`: the ring index of the edge to `other`.
    fn v_find(&self, v: usize, other: usize) -> Option<usize> {
        self.verts[v].ring.iter().position(|nb| self.other(nb.edge, v) == other)
    }

    /// `Vertex.getNeighbour`
    fn edge_between(&self, v: usize, other: usize) -> Option<usize> {
        self.v_find(v, other).map(|i| self.verts[v].ring[i].edge)
    }

    /// The edges around `v`, starting at the head (`Vertex.neighbours`).
    fn ring_edges(&self, v: usize) -> Vec<usize> {
        self.verts[v].ring.iter().map(|nb| nb.edge).collect()
    }

    /// `Map.clone`: fresh vertices and edges (the rings are rebuilt by insertion).
    fn clone_map(&self) -> Map {
        let mut ret = Map::default();
        let mut copy = vec![usize::MAX; self.verts.len()];
        for &v in &self.vlist {
            let nv = ret.new_vertex(self.pos(v));
            copy[v] = nv;
            ret.vlist.push(nv);
        }
        for &e in &self.elist {
            let ed = self.edges[e];
            let ne = ret.new_edge(copy[ed.v1], copy[ed.v2]);
            ret.elist.push(ne);
            ret.v_insert_edge(copy[ed.v1], ne);
            ret.v_insert_edge(copy[ed.v2], ne);
        }
        ret
    }

    fn sort_vertices(&mut self) {
        let verts = &self.verts;
        let n = self.vlist.len() as isize;
        quick_sort(&mut self.vlist, 0, n - 1, &|a, b| lex_cmp_points(verts[a].pos, verts[b].pos));
    }

    fn sort_edges(&mut self) {
        let mins: Vec<f64> = (0..self.edges.len()).map(|e| self.min_x(e)).collect();
        let n = self.elist.len() as isize;
        quick_sort(&mut self.elist, 0, n - 1, &|a, b| lex_cmp(mins[a] - mins[b]));
    }

    /// `Map.getVertex_Simple`: the vertex at `pt`, inserted in sort order if new.
    fn vertex_simple(&mut self, pt: P) -> usize {
        for idx in 0..self.vlist.len() {
            let v = self.vlist[idx];
            match lex_cmp_points(pt, self.pos(v)) {
                0 => return v,
                c if c < 0 => {
                    let nv = self.new_vertex(pt);
                    self.vlist.insert(idx, nv);
                    return nv;
                }
                _ => {}
            }
        }
        let nv = self.new_vertex(pt);
        self.vlist.push(nv);
        nv
    }

    /// `Map.insertEdge_Simple`
    fn insert_edge_simple(&mut self, e: usize) {
        let xm = self.min_x(e);
        let at = self.elist.iter().position(|&c| lex_cmp(xm - self.min_x(c)) < 0);
        match at {
            Some(i) => self.elist.insert(i, e),
            None => self.elist.push(e),
        }
    }

    /// `Map.insertEdge`
    fn insert_edge(&mut self, v1: usize, v2: usize) -> usize {
        let e = self.new_edge(v1, v2);
        self.insert_edge_simple(e);
        self.v_insert_edge(v1, e);
        self.v_insert_edge(v2, e);
        e
    }

    /// `Map.splitEdgesByVertex`: splits the first edge running through `vert`.
    fn split_edges_by_vertex(&mut self, vert: usize) {
        let vp = self.pos(vert);
        for idx in 0..self.elist.len() {
            let e = self.elist[idx];
            if lex_cmp(self.min_x(e) - vp.x) > 0 {
                return;
            }
            let Ed { v1, v2 } = self.edges[e];
            let (p1, p2) = (self.pos(v1), self.pos(v2));
            if zero(vp.dist_to_line(p1, p2)) {
                if zero(vp.dist(p1)) || zero(vp.dist(p2)) {
                    continue;
                }
                let nedge = self.new_edge(vert, v2);
                if let Some(i) = self.v_find(v2, v1) {
                    self.verts[v2].ring[i].edge = nedge;
                }
                self.edges[e].v2 = vert;
                self.elist.remove(idx);
                self.insert_edge_simple(nedge);
                self.insert_edge_simple(e);
                self.v_insert_edge(vert, e);
                self.v_insert_edge(vert, nedge);
                return;
            }
        }
    }

    /// `Map.insertVertex` (`getVertex_Complex`)
    fn insert_vertex(&mut self, pt: P) -> usize {
        let v = self.vertex_simple(pt);
        self.split_edges_by_vertex(v);
        v
    }

    /// `Map.transformMap` (`applyGeneralRigidMotion`)
    fn transform_map(&mut self, t: &Tf) {
        for k in 0..self.vlist.len() {
            let v = self.vlist[k];
            self.verts[v].pos = t.apply(self.verts[v].pos);
            for i in 0..self.verts[v].ring.len() {
                let a = self.nb_angle(v, self.verts[v].ring[i].edge);
                self.verts[v].ring[i].angle = a;
            }
        }
        self.sort_vertices();
        self.sort_edges();
    }

    /// `Map.mergeVertices`: merges the sorted vertex lists; returns the vertex of this map
    /// for each vertex of `other` (by arena index).
    fn merge_vertices(&mut self, other: &Map) -> Vec<usize> {
        let mut copy = vec![usize::MAX; other.verts.len()];
        let mine = std::mem::take(&mut self.vlist);
        let mut out = Vec::with_capacity(mine.len() + other.vlist.len());
        let (mut i, mut j) = (0, 0);
        while i < mine.len() || j < other.vlist.len() {
            if i == mine.len() {
                let yv = other.vlist[j];
                let nv = self.new_vertex(other.pos(yv));
                out.push(nv);
                copy[yv] = nv;
                j += 1;
            } else if j == other.vlist.len() {
                out.push(mine[i]);
                i += 1;
            } else {
                let (mv, yv) = (mine[i], other.vlist[j]);
                match lex_cmp_points(self.pos(mv), other.pos(yv)) {
                    c if c < 0 => {
                        out.push(mv);
                        i += 1;
                    }
                    0 => {
                        out.push(mv);
                        copy[yv] = mv;
                        i += 1;
                        j += 1;
                    }
                    _ => {
                        let nv = self.new_vertex(other.pos(yv));
                        out.push(nv);
                        copy[yv] = nv;
                        j += 1;
                    }
                }
            }
        }
        self.vlist = out;
        copy
    }

    /// `Map.mergeMap(other, true)`: merges `other` in, splitting edges at their crossings.
    fn merge_map(&mut self, mut other: Map) {
        let mut isects = Vec::new();
        for &e in &other.elist {
            let (ep, eq) = (other.pos(other.edges[e].v1), other.pos(other.edges[e].v2));
            let exm = jmax(ep.x, eq.x);
            for &cur in &self.elist {
                if lex_cmp(self.min_x(cur) - exm) > 0 {
                    break;
                }
                let (cp, cq) = (self.pos(self.edges[cur].v1), self.pos(self.edges[cur].v2));
                if let Some(ipt) = true_intersection(ep, eq, cp, cq) {
                    isects.push(ipt);
                }
            }
        }
        for pt in isects {
            self.insert_vertex(pt);
            other.insert_vertex(pt);
        }
        let copy = self.merge_vertices(&other);
        for &e in &other.elist {
            let (v1, v2) = (copy[other.edges[e].v1], copy[other.edges[e].v2]);
            let ne = self.new_edge(v1, v2);
            self.elist.push(ne);
            self.v_insert_edge(v1, ne);
            self.v_insert_edge(v2, ne);
        }
        self.sort_edges();
    }

    /// `Map.mergeSimpleMany`: copies of `other` under each transform, without splitting.
    fn merge_simple_many(&mut self, other: &Map, ts: &[Tf]) {
        let mut copy = vec![usize::MAX; other.verts.len()];
        for t in ts {
            for &v in &other.vlist {
                copy[v] = self.vertex_simple(t.apply(other.pos(v)));
            }
            for &e in &other.elist {
                let (v1, v2) = (copy[other.edges[e].v1], copy[other.edges[e].v2]);
                let ne = self.new_edge(v1, v2);
                self.v_insert_edge(v1, ne);
                self.v_insert_edge(v2, ne);
                self.elist.push(ne);
            }
        }
        self.sort_edges();
    }
}

// ---------------------------------------------------------------------------------------------
// tile.Tiling, tile.Feature

struct Feature {
    regular: bool,
    pts: Vec<P>,
}

impl Feature {
    /// `Feature(int n)`: a regular n-gon with unit inradius.
    fn regular(n: usize) -> Self {
        let pts = (0..n)
            .map(|idx| {
                let angle = (M_PI / n as f64) * (2 * idx + 1) as f64;
                let sc = 1.0 / (M_PI / n as f64).cos();
                p(sc * angle.cos(), sc * angle.sin())
            })
            .collect();
        Feature { regular: true, pts }
    }
}

struct Tiling {
    t1: P,
    t2: P,
    features: Vec<Feature>,
    /// Placed features: feature index and transform.
    placed: Vec<(usize, Tf)>,
}

/// The tilings of the variation's `Id` parameter (`TapratsFunc.patterns`).
const PATTERNS: &[&str] = &[
    "csk_7", "4.8^2", "10", "6", "4.6.12", "3.12^2", "csk_5", "csk_9", "18", "8.12", "9.12", "square_12", "16.8", "12.18", "alhambra16", "8_var", "18.9", "24.12", "14", "6.5x6", "3.4.6",
    "8 Ring", "8 and 8", "Seville Alcazar 12.8.6", "7.6", "Al-Mustansiriyya", "6.5 flower", "9.6", "9.5.3.6", "8 6 ~5", "pbn_7", "Alcazar De Seville 12 8 - Shorter", "Green Mosque",
    "Ilkhanid Uljaytu", "Ilkhanid Uljaytu Vault", "Mamluk Quran", "Mughal I'timad", "Sultan Lodge", "Timurid Tumaq Aqa", "3-8-4-6", "9.6 ~5", "Square 12.4.3", "wonka", "var_5",
    "9.6 Losange", "6.5", "6.4 Boats", "6.4 Circles", "4.7 Stars",
];

fn tilings() -> &'static HashMap<String, Tiling> {
    static T: OnceLock<HashMap<String, Tiling>> = OnceLock::new();
    T.get_or_init(|| parse_tilings(include_str!("../../../../assets/taprats/tilings.txt")))
}

fn parse_tilings(src: &str) -> HashMap<String, Tiling> {
    let mut out = HashMap::new();
    let mut cur: Option<(String, Tiling)> = None;
    for line in src.lines() {
        let (kw, rest) = line.split_once(' ').unwrap_or((line, ""));
        let nums = || rest.split_whitespace().map(|s| s.parse::<f64>().unwrap_or(0.0)).collect::<Vec<f64>>();
        match kw {
            "tiling" => {
                if let Some((n, t)) = cur.take() {
                    out.insert(n, t);
                }
                cur = Some((rest.to_string(), Tiling { t1: p(0.0, 0.0), t2: p(0.0, 0.0), features: Vec::new(), placed: Vec::new() }));
            }
            _ => {
                let Some((_, t)) = cur.as_mut() else { continue };
                let v = nums();
                match kw {
                    "trans" if v.len() == 4 => (t.t1, t.t2) = (p(v[0], v[1]), p(v[2], v[3])),
                    "regular" if !v.is_empty() => t.features.push(Feature::regular(v[0] as usize)),
                    "polygon" => t.features.push(Feature { regular: false, pts: v.as_chunks::<2>().0.iter().map(|c| p(c[0], c[1])).collect() }),
                    "place" if v.len() == 6 && !t.features.is_empty() => t.placed.push((t.features.len() - 1, Tf::new(v[0], v[1], v[2], v[3], v[4], v[5]))),
                    _ => {}
                }
            }
        }
    }
    if let Some((n, t)) = cur {
        out.insert(n, t);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// app: figures, prototype, inference

enum Fig {
    Rosette { n: usize, q: f64, s: i32 },
    Star { n: usize, d: f64, s: i32 },
    Explicit(Map),
}

impl Fig {
    fn is_radial(&self) -> bool {
        !matches!(self, Fig::Explicit(_))
    }

    /// `Figure.getMap`; `None` where JWildfire fails (lines that do not meet).
    fn map(&self) -> Option<Map> {
        let (n, unit) = match *self {
            Fig::Explicit(ref m) => return Some(m.clone_map()),
            Fig::Rosette { n, q, s } => (n, rosette_unit(n, q, s)?),
            Fig::Star { n, d, s } => (n, star_unit(n, d, s)?),
        };
        // RadialFigure.getMap: n rotated copies of the unit
        let tr = Tf::rotate(2.0 * M_PI * (1.0 / n as f64));
        let mut ts = Vec::with_capacity(n);
        let mut base = tr;
        for _ in 0..n {
            ts.push(base);
            base = base.compose(&tr);
        }
        let mut ret = Map::default();
        ret.merge_simple_many(&unit, &ts);
        Some(ret)
    }
}

/// The chain of points from the tip, mirrored at the x axis (`Star`/`Rosette.buildUnit`).
fn unit_from(tip: P, points: &[P]) -> (Map, usize) {
    let mut from = Map::default();
    let vt = from.insert_vertex(tip);
    let (mut top_prev, mut bot_prev) = (vt, vt);
    for pt in points {
        let top = from.insert_vertex(*pt);
        let bot = from.insert_vertex(p(pt.x, -pt.y));
        from.insert_edge(top_prev, top);
        from.insert_edge(bot_prev, bot);
        top_prev = top;
        bot_prev = bot;
    }
    (from, top_prev)
}

/// `Star.buildUnit`
fn star_unit(n: usize, d: f64, s: i32) -> Option<Map> {
    let dn = n as f64;
    let don = 1.0 / dn;
    let tr = Tf::rotate(2.0 * M_PI * don);
    let clamp_d = jmax(1.0, jmin(d, 0.5 * dn - 0.01));
    let did = clamp_d.floor();
    let mut dfrac = clamp_d - did;
    let mut di = did as i32;
    let mut is_int = false;
    let clamp_s = s.min(di);
    let outer_s = s.min(di - 1);
    if dfrac < TOL {
        dfrac = 0.0;
        is_int = true;
    } else if (1.0 - dfrac) < TOL {
        dfrac = 0.0;
        di = did as i32 + 1;
        is_int = true;
    }
    let a = p(1.0, 0.0);
    let b = arc(clamp_d * don);
    let mut points = Vec::new();
    for idx in 1..=outer_s {
        let ar = arc(idx as f64 * don);
        let br = arc((idx as f64 - clamp_d) * don);
        points.push(intersection(a, b, ar, br)?);
    }
    let (mut from, top_prev) = unit_from(a, &points);
    if clamp_s == di {
        let midr = tr.apply(from.pos(top_prev));
        let v4 = from.insert_vertex(midr);
        if is_int {
            from.insert_edge(top_prev, v4);
        } else {
            let ar = arc(did * don);
            let br = arc(-dfrac * don);
            let c = arc(d * don);
            let cent = intersection(ar, br, a, c)?;
            let vcent = from.insert_vertex(cent);
            from.insert_edge(top_prev, vcent);
            from.insert_edge(vcent, v4);
        }
    }
    Some(from)
}

/// `Rosette.buildUnit`
fn rosette_unit(n: usize, q: f64, s: i32) -> Option<Map> {
    let don = 1.0 / n as f64;
    let tr = Tf::rotate(2.0 * M_PI * don);
    let tip = p(1.0, 0.0);
    let rtip = arc(don);
    let q_clamp = jmin(jmax(q, -0.99), 0.99);
    let s_clamp = s.min((n as i32 - 1) / 2);
    let r_outer = 1.0 / (M_PI * don).cos();
    let up_outer = arc(0.5 * don).scale(r_outer);
    let bisector = up_outer.sub(p(0.0, r_outer)).scale(0.5);
    let mut th = rtip.sub(tip).angle();
    let stable_isect = up_outer.add(up_outer.normalize().scale(-up_outer.y));
    let stable_angle = stable_isect.sub(tip).angle();
    if q_clamp >= 0.0 {
        th = th * (1.0 - q_clamp) + (M_PI * 0.5) * q_clamp;
    } else {
        th = th * (1.0 + q_clamp) - stable_angle * q_clamp;
    }
    let qtip = p(1.0 + th.cos(), th.sin());
    let key_point = intersection(tip, qtip, up_outer, bisector)?;
    let key_end = key_point.convex_sum(stable_isect, 10.0);
    let mut points = vec![key_point];
    let mut key_r_p = p(key_point.x, -key_point.y);
    let mut key_r_e = p(key_end.x, -key_end.y);
    for _ in 1..=s_clamp {
        key_r_p = tr.apply(key_r_p);
        key_r_e = tr.apply(key_r_e);
        if let Some(mid) = intersection(key_point, key_end, key_r_p, key_r_e) {
            points.push(mid);
        }
    }
    Some(unit_from(tip, &points).0)
}

/// `Prototype`: the figure of each feature (in order of the features' first placement).
#[derive(Default)]
struct Proto {
    figs: Vec<(usize, Fig)>,
}

impl Proto {
    fn add(&mut self, feature: usize, fig: Fig) {
        match self.figs.iter_mut().find(|(f, _)| *f == feature) {
            Some(e) => e.1 = fig,
            None => self.figs.push((feature, fig)),
        }
    }

    fn get(&self, feature: usize) -> Option<&Fig> {
        self.figs.iter().find(|(f, _)| *f == feature).map(|(_, g)| g)
    }

    /// `Prototype.construct`: the figures repeated over `region`.
    fn construct(&self, tiling: &Tiling, region: &[P; 4]) -> Option<Map> {
        let locations: Vec<Tf> = fill_region(region, tiling.t1, tiling.t2)?
            .into_iter()
            .map(|(t1, t2)| Tf::translate(tiling.t1.scale(t1 as f64).add(tiling.t2.scale(t2 as f64))))
            .collect();
        let mut ret = Map::default();
        for (feature, fig) in &self.figs {
            let figmap = fig.map()?;
            let sub_t: Vec<Tf> = tiling.placed.iter().filter(|(f, _)| f == feature).map(|(_, t)| *t).collect();
            let mut transmap = Map::default();
            transmap.merge_simple_many(&figmap, &sub_t);
            let mut featuremap = Map::default();
            featuremap.merge_simple_many(&transmap, &locations);
            ret.merge_map(featuremap.clone_map());
        }
        Some(ret)
    }
}

/// `FillRegion.fill`: the translation units `(t1, t2)` that cover the quadrilateral `pgon`.
fn fill_region(pgon: &[P; 4], t1: P, t2: P) -> Option<Vec<(i32, i32)>> {
    let det = 1.0 / (t1.x * t2.y - t2.x * t1.y);
    let basis = [t2.y * det, -t2.x * det, -t1.y * det, t1.x * det];
    let ps = pgon.map(|pt| p(basis[0] * pt.x + basis[1] * pt.y, basis[2] * pt.x + basis[3] * pt.y));
    let (mut ymin, mut ymax) = (ps[0].y, ps[0].y);
    for pt in &ps[1..] {
        if pt.y < ymin {
            ymin = pt.y;
        }
        if pt.y > ymax {
            ymax = pt.y;
        }
    }
    let (iymin, iymax) = (ymin.floor() as i32, ymax.floor() as i32);
    if iymax as i64 - iymin as i64 > 999 {
        return None;
    }
    let rows = (iymax - iymin + 1).max(0) as usize;
    let mut xmins = vec![f64::MAX; rows];
    let mut xmaxs = vec![-f64::MAX; rows];
    let mut upd = |iy: i32, x: f64| {
        if let Some(o) = usize::try_from(iy - iymin).ok().filter(|&o| o < rows) {
            if x < xmins[o] {
                xmins[o] = x;
            }
            if x > xmaxs[o] {
                xmaxs[o] = x;
            }
        }
    };
    for lc in 0..4 {
        let (from, to) = (ps[lc], ps[(lc + 1) % 4]);
        let (mut fx, mut fy, mut tx, mut ty) = (from.x, from.y, to.x, to.y);
        if fy == ty {
            let iy = fy.floor() as i32;
            upd(iy, fx);
            upd(iy, tx);
        } else {
            if fy > ty {
                (fx, fy, tx, ty) = (tx, ty, fx, fy);
            }
            let mut y = fy.floor();
            let mut iy = y as i32;
            let iym = ty.floor() as i32 as f64;
            let m = (tx - fx) / (ty - fy);
            let mut x = fx + (y - fy) * m;
            while iy as f64 <= iym {
                let (nx, ny) = (x + m, y + 1.0);
                upd(iy, if y <= fy { fx } else { x });
                upd(iy, if ny > ty { tx } else { nx });
                iy += 1;
                y = ny;
                x = nx;
            }
        }
    }
    let mut out = Vec::new();
    for idx in 0..rows {
        let y = (idx as i32 + iymin) as f64;
        let (mut startx, endx) = (xmins[idx].floor(), xmaxs[idx].floor());
        while startx <= endx {
            if out.len() >= 100_000 {
                return None;
            }
            out.push((startx as i32, y as i32));
            startx += 1.0;
        }
    }
    Some(out)
}

/// `Infer.contact`
struct Contact {
    position: P,
    other: P,
    end: P,
    isect: Option<P>,
    isect_idx: i64,
    colinear: u8,
    taken: bool,
}

const COLINEAR_MASTER: u8 = 1;
const COLINEAR_SLAVE: u8 = 2;

impl Contact {
    fn new(position: P, other: P) -> Self {
        Contact { position, other, end: position.add(position.sub(other).normalize().scale(100.0)), isect: None, isect_idx: -1, colinear: 0, taken: false }
    }
}

const INSIDE_EVEN: i32 = 0;
const INSIDE_COLINEAR: i32 = 1;
const INSIDE_UNEVEN: i32 = 2;
const OUTSIDE_EVEN: i32 = 3;
const OUTSIDE_UNEVEN: i32 = 4;
const NONE: i32 = 5;

fn lex_cmp_distances(kind1: i32, dist1: f64, kind2: i32, dist2: f64) -> i32 {
    if kind1 != kind2 {
        return if kind1 < kind2 { -1 } else { 1 };
    }
    if loose_eq(dist1, dist2) {
        0
    } else if dist1 < dist2 {
        -1
    } else {
        1
    }
}

fn is_colinear(pp: P, q: P, a: P) -> bool {
    loose_eq((q.x - pp.x) * (a.y - pp.y), (q.y - pp.y) * (a.x - pp.x))
}

/// `Polygon.pointInPoly`
fn point_in_poly(pts: &[P], apt: P) -> bool {
    let size = pts.len();
    let mut outside = false;
    let (x, y) = (apt.x, apt.y);
    let mut a = pts[0];
    for idx in 0..size {
        let b = pts[(idx + 1) % size];
        let (xmin, ymin, xmax, ymax) = if a.x < b.x { (a.x, a.y, b.x, b.y) } else { (b.x, b.y, a.x, a.y) };
        if xmin <= x && x < xmax && (x - xmin) * (ymax - ymin) > (y - ymin) * (xmax - xmin) {
            outside = !outside;
        }
        a = b;
    }
    outside
}

/// `Infer.infer`: a figure for a feature without one, continuing the lines of its neighbours.
fn infer(tiling: &Tiling, proto: &Proto, feature: usize) -> Option<Map> {
    let mut maps: HashMap<usize, Map> = HashMap::new();
    for (f, fig) in &proto.figs {
        maps.insert(*f, fig.map()?);
    }
    // placed_points of the 3x3 units around the origin
    let mut placed: Vec<(usize, Tf, Vec<P>)> = Vec::new();
    for y in -1..=1 {
        for x in -1..=1 {
            let t = Tf::translate(tiling.t1.scale(x as f64).add(tiling.t2.scale(y as f64)));
            for (fi, tf) in &tiling.placed {
                let tt = t.compose(tf);
                let fpts: Vec<P> = tiling.features[*fi].pts.iter().map(|q| tt.apply(*q)).collect();
                let sz = fpts.len();
                let mids = (0..sz).map(|i| fpts[i].convex_sum(fpts[(i + 1) % sz], 0.5)).collect();
                placed.push((*fi, tt, mids));
            }
        }
    }
    let nf = tiling.placed.len();
    let cur = (nf * 4..nf * 5).find(|&i| placed[i].0 == feature)?;
    let (_, main_t, main_mids) = &placed[cur];
    let fpts: Vec<P> = tiling.features[feature].pts.iter().map(|q| main_t.apply(*q)).collect();
    // getAdjacencies: the feature (and transform) across each edge
    let mut adjs: Vec<(usize, Tf)> = Vec::new();
    for pv in main_mids {
        let found = placed.iter().enumerate().filter(|(i, _)| *i != cur).find_map(|(_, (f, t, mids))| mids.iter().any(|m| m.near(*pv)).then_some((*f, *t)));
        adjs.push(found?);
    }
    // buildContacts: where the neighbours' lines meet the feature's edges
    let mut cons: Vec<Contact> = Vec::new();
    for (idx, (af, at)) in adjs.iter().enumerate() {
        let mut map = maps.get(af)?.clone_map();
        map.transform_map(at);
        let (a, b) = (fpts[idx], fpts[(idx + 1) % fpts.len()]);
        for &v in &map.vlist {
            let pos = map.pos(v);
            if zero(pos.dist_to_line(a, b)) && pos.dist2(a) > TOL2 {
                for e in map.ring_edges(v) {
                    cons.push(Contact::new(pos, map.pos(map.other(e, v))));
                }
            }
        }
    }
    // pair the contacts
    for idx in 0..cons.len() {
        if cons[idx].taken {
            continue;
        }
        let con = &cons[idx];
        let (mut jbest, mut bestisect, mut bestdist, mut bestkind) = (-1i64, None, 0.0, NONE);
        for (jdx, ocon) in cons.iter().enumerate() {
            if jdx == idx || ocon.taken || con.position.near(ocon.position) {
                continue;
            }
            let mut isect = None;
            let (mykind, mydist);
            if is_colinear(con.other, con.position, ocon.position) && is_colinear(con.position, ocon.position, ocon.other) {
                let d1 = con.position.sub(con.other);
                let d2 = ocon.position.sub(ocon.other);
                let dc = con.position.sub(ocon.position);
                if d1.dot(d2) > 0.0 || d1.dot(dc) > 0.0 || d2.dot(dc) < 0.0 {
                    continue;
                }
                mykind = INSIDE_COLINEAR;
                mydist = con.position.dist(ocon.position);
            } else {
                let Some(is) = true_intersection(con.position, con.end, ocon.position, ocon.end) else { continue };
                if is.near(con.position) || is.near(ocon.position) {
                    continue;
                }
                isect = Some(is);
                let dist = con.position.dist(is);
                let odist = ocon.position.dist(is);
                let inside = point_in_poly(&fpts, is);
                if !loose_eq(dist, odist) {
                    mykind = if inside { INSIDE_UNEVEN } else { OUTSIDE_UNEVEN };
                    mydist = (dist - odist).abs();
                } else {
                    mykind = if inside { INSIDE_EVEN } else { OUTSIDE_EVEN };
                    mydist = dist;
                }
            }
            if lex_cmp_distances(mykind, mydist, bestkind, bestdist) < 0 {
                jbest = jdx as i64;
                bestkind = mykind;
                bestdist = mydist;
                bestisect = isect;
            }
        }
        if jbest == -1 {
            continue;
        }
        let jb = jbest as usize;
        cons[idx].taken = true;
        cons[jb].taken = true;
        if bestkind == INSIDE_COLINEAR {
            cons[idx].colinear = COLINEAR_MASTER;
            cons[jb].colinear = COLINEAR_SLAVE;
        } else {
            cons[idx].isect = bestisect;
            cons[jb].isect = bestisect;
        }
        cons[idx].isect_idx = jbest;
        cons[jb].isect_idx = idx as i64;
    }
    let segment = |a: P, b: P| {
        let mut t = Map::default();
        let v1 = t.insert_vertex(a);
        let v2 = t.insert_vertex(b);
        t.insert_edge(v1, v2);
        t
    };
    let mut ret = Map::default();
    for con in &cons {
        match con.isect {
            None if con.colinear == COLINEAR_MASTER => ret.merge_map(segment(con.position, cons[con.isect_idx as usize].position)),
            None => {}
            Some(is) => ret.merge_map(segment(con.position, is)),
        }
    }
    // unmatched contacts: join them pairwise with a short bend
    let mut minlen = fpts[0].dist(fpts[fpts.len() - 1]);
    for idx in 1..fpts.len() {
        minlen = jmin(minlen, fpts[idx - 1].dist(fpts[idx]));
    }
    for idx in 0..cons.len() {
        if cons[idx].isect_idx != -1 {
            continue;
        }
        for jdx in 0..cons.len() {
            if jdx == idx || cons[jdx].isect_idx != -1 {
                continue;
            }
            let (con, ocon) = (&cons[idx], &cons[jdx]);
            let ex1 = con.position.add(con.position.sub(con.other).normalize().scale(minlen * 0.5));
            let ex2 = ocon.position.add(ocon.position.sub(ocon.other).normalize().scale(minlen * 0.5));
            let mut t = Map::default();
            let vx1 = t.insert_vertex(ex1);
            let vx2 = t.insert_vertex(ex2);
            let c = t.insert_vertex(con.position);
            let oc = t.insert_vertex(ocon.position);
            t.insert_edge(c, vx1);
            t.insert_edge(vx1, vx2);
            t.insert_edge(vx2, oc);
            ret.merge_map(t);
            cons[idx].isect_idx = jdx as i64;
            cons[jdx].isect_idx = idx as i64;
        }
    }
    ret.transform_map(&main_t.invert()?);
    Some(ret)
}

// ---------------------------------------------------------------------------------------------
// ui1: render styles -> primitives

/// `[type, color, x1, y1, x2, y2, x3, y3]`
const PRIM: usize = 8;

#[derive(Default)]
struct Prims(Vec<[f64; PRIM]>);

impl Prims {
    fn line(&mut self, a: P, b: P, color: f64) {
        self.0.push([2.0, color, a.x, a.y, b.x, b.y, 0.0, 0.0]);
    }

    /// `RenderStyle.drawPolygon`: triangles (filled) or the outline.
    fn polygon(&mut self, pts: &[P], fill: bool, color: f64) {
        if fill {
            let c: Vec<(f64, f64)> = pts.iter().map(|q| (q.x, q.y)).collect();
            for t in super::batch_w13::sunvoroni_triangulate(&c).as_chunks::<3>().0 {
                self.0.push([3.0, color, t[0].0, t[0].1, t[1].0, t[1].1, t[2].0, t[2].1]);
            }
        } else {
            for i in 0..pts.len() {
                self.line(pts[i], pts[(i + 1) % pts.len()], color);
            }
        }
    }
}

/// `RenderOutline.getJoinPoint`
fn join_point(joint: P, a: P, b: P, width: f64) -> Option<P> {
    let th = joint.sweep(a, b);
    if (th - M_PI).abs() < 1e-7 {
        return None;
    }
    let d1 = joint.sub(a).normalize();
    let d2 = joint.sub(b).normalize();
    let l = width / th.sin();
    Some(p(joint.x - (d1.x + d2.x) * l, joint.y - (d1.y + d2.y) * l))
}

/// `RenderOutline.getPoints`: the two outline corners at `to`.
fn outline_points(map: &Map, edge: usize, from: usize, to: usize, width: f64) -> Option<[P; 2]> {
    let (pfrom, pto) = (map.pos(from), map.pos(to));
    let perp = pto.sub(pfrom).normalize().perp();
    let square = [pto.sub(perp.scale(width)), pto.add(perp.scale(width))];
    match map.degree(to) {
        1 => Some(square),
        2 => {
            let (before, _) = map.v_before_after(to, edge)?;
            match join_point(pto, pfrom, map.pos(map.other(before, to)), width) {
                Some(jp) => Some([jp, jp.convex_sum(pto, 2.0)]),
                None => Some(square),
            }
        }
        _ => {
            let (before, after) = map.v_before_after(to, edge)?;
            let (before_pt, after_pt) = (map.pos(map.other(before, to)), map.pos(map.other(after, to)));
            Some([join_point(pto, pfrom, after_pt, width)?, join_point(pto, before_pt, pfrom, width)?])
        }
    }
}

/// Six points per edge: `RenderOutline.setMap`.
fn outline_pts(map: &Map, width: f64) -> Option<Vec<P>> {
    let mut pts = Vec::with_capacity(map.elist.len() * 6);
    for &e in &map.elist {
        let Ed { v1, v2 } = map.edges[e];
        let top = outline_points(map, e, v1, v2, width)?;
        let fromp = outline_points(map, e, v2, v1, width)?;
        pts.extend([top[0], map.pos(v2), top[1], fromp[0], map.pos(v1), fromp[1]]);
    }
    Some(pts)
}

fn render_outline(map: &Map, width: f64, fill: bool, out: &mut Prims) -> Option<()> {
    let pts = outline_pts(map, width)?;
    for q in pts.as_chunks::<6>().0 {
        out.polygon(q, fill, 1.0);
        out.line(q[2], q[3], 0.0);
        out.line(q[5], q[0], 0.0);
    }
    Some(())
}

fn render_emboss(map: &Map, width: f64, fill: bool, out: &mut Prims) -> Option<()> {
    let pts = outline_pts(map, width)?;
    let (light_x, light_y) = ((M_PI * 0.25).cos(), (M_PI * 0.25).sin());
    let trap = |a: P, b: P, c: P, d: P, out: &mut Prims| {
        let n = a.sub(d).perp().normalize();
        let dd = 0.5 * (n.x * light_x + n.y * light_y + 1.0);
        out.polygon(&[a, b, c, d], fill, dd);
    };
    for q in pts.as_chunks::<6>().0 {
        trap(q[1], q[2], q[3], q[4], out);
        trap(q[4], q[5], q[0], q[1], out);
        out.polygon(q, fill, 1.0);
        out.line(q[1], q[4], 0.0);
    }
    Some(())
}

/// `RenderInterlace.assignInterlacing`: which end of each edge goes under, by a depth-first
/// walk (iterative, in the order of JWildfire's recursion).
fn assign_interlacing(map: &Map) -> Vec<bool> {
    enum Frame {
        Build { e: usize, under: bool, step: u8 },
        Prop { cands: Vec<(usize, bool)>, k: usize },
    }
    let mut e_visited = vec![false; map.edges.len()];
    let mut start_under = vec![false; map.edges.len()];
    let mut v_visited = vec![false; map.verts.len()];
    // propagate(): mark the vertex and list the edges it continues into
    let propagate = |v: usize, edge: usize, under_at_vert: bool, v_visited: &mut Vec<bool>| -> Frame {
        v_visited[v] = true;
        let nn = map.degree(v);
        let mut cands = Vec::new();
        if nn == 2 {
            if let Some((oe, _)) = map.v_before_after(v, edge) {
                cands.push((oe, if map.edges[oe].v1 == v { !under_at_vert } else { under_at_vert }));
            }
        } else if nn > 2 {
            let ns = map.ring_edges(v);
            let edge_idx = ns.iter().rposition(|&e| e == edge).map_or(-1, |i| i as i64);
            let mut cur_under = under_at_vert;
            for idx in 1..nn as i64 {
                let oe = ns[(edge_idx + idx).rem_euclid(nn as i64) as usize];
                cands.push((oe, if map.edges[oe].v1 == v { !cur_under } else { cur_under }));
                cur_under = !cur_under;
            }
        }
        Frame::Prop { cands, k: 0 }
    };
    for &root in &map.elist {
        if e_visited[root] {
            continue;
        }
        let mut stack = vec![Frame::Build { e: root, under: true, step: 0 }];
        while let Some(top) = stack.last_mut() {
            match top {
                Frame::Build { e, under, step } => {
                    let (e, under) = (*e, *under);
                    let Ed { v1, v2 } = map.edges[e];
                    match *step {
                        0 => {
                            e_visited[e] = true;
                            start_under[e] = under;
                            *step = 1;
                            if !v_visited[v1] {
                                let f = propagate(v1, e, under, &mut v_visited);
                                stack.push(f);
                            }
                        }
                        1 => {
                            *step = 2;
                            if !v_visited[v2] {
                                let f = propagate(v2, e, !under, &mut v_visited);
                                stack.push(f);
                            }
                        }
                        _ => {
                            stack.pop();
                        }
                    }
                }
                Frame::Prop { cands, k } => {
                    let next = cands[*k..].iter().position(|(oe, _)| !e_visited[*oe]).map(|i| *k + i);
                    match next {
                        Some(i) => {
                            *k = i + 1;
                            let (oe, under) = cands[i];
                            stack.push(Frame::Build { e: oe, under, step: 0 });
                        }
                        None => {
                            stack.pop();
                        }
                    }
                }
            }
        }
    }
    start_under
}

/// `RenderInterlace.getPoints`: three points at `to` (below, center, above).
fn interlace_points(map: &Map, start_under: &[bool], edge: usize, from: usize, to: usize, width: f64, gap: f64) -> Option<[P; 3]> {
    let from_under = (map.edges[edge].v1 == from) == start_under[edge];
    let (pfrom, pto) = (map.pos(from), map.pos(to));
    let dir = pto.sub(pfrom).normalize();
    let perp = dir.perp();
    let nn = map.degree(to);
    if nn == 1 {
        let (perp, dir) = (perp.scale(width), dir.scale(width));
        return Some([pto.sub(perp).add(dir), pto.add(dir), pto.add(perp).add(dir)]);
    }
    if nn == 2 {
        let jps = outline_points(map, edge, from, to, width)?;
        return Some([jps[0], pto, jps[1]]);
    }
    if from_under {
        let ns = map.ring_edges(to);
        let edge_idx = ns.iter().rposition(|&e| e == edge).map_or(-1, |i| i as i64);
        let op = map.pos(map.other(ns[(edge_idx + 2).rem_euclid(nn as i64) as usize], to));
        let below = join_point(pto, pfrom, op, width).unwrap_or_else(|| pto.sub(perp.scale(width)));
        Some([below, pto, below.convex_sum(pto, 2.0)])
    } else {
        let (before, after) = map.v_before_after(to, edge)?;
        let (before_pt, after_pt) = (map.pos(map.other(before, to)), map.pos(map.other(after, to)));
        let below = join_point(pto, pfrom, after_pt, width).unwrap_or_else(|| pto.sub(perp.scale(width)));
        let above = join_point(pto, before_pt, pfrom, width).unwrap_or_else(|| pto.add(perp.scale(width)));
        let cen = join_point(pto, before_pt, after_pt, width).unwrap_or_else(|| pto.sub(after_pt.sub(before_pt).normalize().perp().scale(width)));
        let g = dir.scale(gap);
        Some([below.sub(g), cen.sub(g), above.sub(g)])
    }
}

fn render_interlace(map: &Map, width: f64, gap: f64, fill: bool, out: &mut Prims) -> Option<()> {
    let map = map.clone_map();
    let under = assign_interlacing(&map);
    let mut pts = Vec::with_capacity(map.elist.len() * 6);
    for &e in &map.elist {
        let Ed { v1, v2 } = map.edges[e];
        pts.extend(interlace_points(&map, &under, e, v1, v2, width, gap)?);
        pts.extend(interlace_points(&map, &under, e, v2, v1, width, gap)?);
    }
    for q in pts.as_chunks::<6>().0 {
        out.polygon(q, fill, 1.0);
    }
    for q in pts.as_chunks::<6>().0 {
        out.line(q[2], q[3], 0.0);
        out.line(q[5], q[0], 0.0);
    }
    Some(())
}

/// Face outlines.
type Faces = Vec<Vec<P>>;

/// `Faces.extractFaces`: the faces of the map, two-coloured (black, white).
fn extract_faces(map: &Map) -> Option<(Faces, Faces)> {
    const UNDONE: u8 = 0;
    const PROCESSING: u8 = 1;
    const DONE_BLACK: u8 = 2;
    const DONE_WHITE: u8 = 3;
    let ne = map.edges.len();
    let (mut f1, mut f2): (Vec<Option<usize>>, Vec<Option<usize>>) = (vec![None; ne], vec![None; ne]);
    let mut infos: Vec<(Vec<usize>, u8)> = Vec::new();
    let mut faces: Vec<usize> = Vec::new();
    let extract_face = |from: usize, edge: usize| -> Option<Vec<usize>> {
        let mut ret = Vec::new();
        let (mut cur, mut ecur) = (from, edge);
        loop {
            ret.push(cur);
            if ret.len() > 4 * ne + 4 {
                return None;
            }
            let n = map.other(ecur, cur);
            if map.degree(n) < 2 {
                return None;
            }
            ecur = map.v_before_after(n, ecur)?.0;
            cur = n;
            if cur == from {
                return Some(ret);
            }
        }
    };
    let is_clockwise = |pts: &[usize]| {
        let sz = pts.len();
        let mut imax = 0;
        let mut xmax = map.pos(pts[0]).x;
        for (idx, &v) in pts.iter().enumerate().skip(1) {
            let x = map.pos(v).x;
            if x > xmax {
                imax = idx;
                xmax = x;
            }
        }
        let pmax = map.pos(pts[imax]);
        let pnext = map.pos(pts[(imax + 1) % sz]);
        let pprev = map.pos(pts[(imax + sz - 1) % sz]);
        pmax.sub(pprev).cross(pnext.sub(pmax)) < 0.0
    };
    for &e in &map.elist {
        for side in 0..2 {
            let (vert, done) = if side == 0 { (map.edges[e].v1, f1[e].is_some()) } else { (map.edges[e].v2, f2[e].is_some()) };
            if done {
                continue;
            }
            let Some(fc) = extract_face(vert, e) else { continue };
            let fi = infos.len();
            if !is_clockwise(&fc) {
                faces.push(fi);
            }
            for v in 0..fc.len() {
                let (from, to) = (fc[v], fc[(v + 1) % fc.len()]);
                let conn = map.edge_between(from, to)?;
                if map.edges[conn].v1 == from {
                    f1[conn] = Some(fi);
                } else {
                    f2[conn] = Some(fi);
                }
            }
            infos.push((fc, UNDONE));
        }
    }
    let twin = |infos: &[(Vec<usize>, u8)], fi: usize, idx: usize| -> Option<Option<usize>> {
        let face = &infos[fi].0;
        let (from, to) = (face[idx], face[(idx + 1) % face.len()]);
        let conn = map.edge_between(from, to)?;
        Some(if map.edges[conn].v1 == from { f2[conn] } else { f1[conn] })
    };
    let first = *faces.first()?;
    let mut st = vec![first];
    infos[first].1 = PROCESSING;
    while let Some(fi) = st.pop() {
        let mut col = -1;
        for idx in 0..infos[fi].0.len() {
            let Some(nfi) = twin(&infos, fi, idx)? else { continue };
            match infos[nfi].1 {
                UNDONE => {
                    infos[nfi].1 = PROCESSING;
                    st.push(nfi);
                }
                DONE_BLACK => {
                    if col == 0 {
                        return None;
                    }
                    col = 1;
                }
                DONE_WHITE => {
                    if col == 1 {
                        return None;
                    }
                    col = 0;
                }
                _ => {}
            }
        }
        infos[fi].1 = if col == 1 { DONE_WHITE } else { DONE_BLACK };
    }
    let (mut blk, mut wht) = (Vec::new(), Vec::new());
    for &fi in &faces {
        let pts: Vec<P> = infos[fi].0.iter().map(|&v| map.pos(v)).collect();
        match infos[fi].1 {
            DONE_WHITE => wht.push(pts),
            DONE_BLACK => blk.push(pts),
            _ => {}
        }
    }
    Some((blk, wht))
}

/// `RenderVecView`: the pattern of tiling `name` as primitives.
#[allow(clippy::too_many_arguments)]
fn build(name: &str, style: i32, width: f64, gap: f64, fill: bool, q: f64, s: i32, rosette: bool, sd: f64, ss: i32, infer_figs: bool) -> Option<Vec<[f64; PRIM]>> {
    let tiling = tilings().get(name)?;
    let mut proto = Proto::default();
    for (fi, _) in &tiling.placed {
        let f = &tiling.features[*fi];
        let n = f.pts.len();
        let fig = if f.regular && n > 4 {
            if rosette { Fig::Rosette { n, q, s } } else { Fig::Star { n, d: sd, s: ss } }
        } else {
            Fig::Explicit(Map::default())
        };
        proto.add(*fi, fig);
    }
    if infer_figs {
        for (fi, _) in &tiling.placed {
            if proto.get(*fi).is_some_and(Fig::is_radial) {
                continue;
            }
            let m = infer(tiling, &proto, *fi)?;
            proto.add(*fi, Fig::Explicit(m));
        }
    }
    // GeoView(-1.5, 1.5, 3.0) at 200 x 200 pixels: the boundary of the view in world units
    let (ww, wh, w, left, top) = (200.0, 200.0, 3.0, -1.5, 1.5);
    let h = w / (ww / wh);
    let first = Tf::translate(p(-left, -(top - h)));
    let second = Tf::scale(ww / w, -(wh / h));
    let third = Tf::translate(p(0.0, wh));
    let fourth = Tf::rotate_around(p(ww / 2.0, wh / 2.0), 0.0);
    let inv = fourth.compose(&third.compose(&second.compose(&first))).invert()?;
    let region = [inv.apply(p(0.0, 0.0)), inv.apply(p(ww, 0.0)), inv.apply(p(ww, wh)), inv.apply(p(0.0, wh))];
    let map = proto.construct(tiling, &region)?;

    let mut out = Prims::default();
    match style {
        0 | 5 => {
            let edges = map.elist.iter().map(|&e| (map.pos(map.edges[e].v1), map.pos(map.edges[e].v2)));
            if style == 0 {
                for (a, b) in edges {
                    out.line(a, b, 0.0);
                }
            } else {
                // RenderSketch: eight jittered copies of each edge
                let mut rand = JavaRandom::new(279401);
                for (mut a, mut b) in edges {
                    for _ in 0..8 {
                        a.x += (rand.next_double() * 0.05) - 0.025;
                        a.y += (rand.next_double() * 0.05) - 0.025;
                        b.x += (rand.next_double() * 0.05) - 0.025;
                        b.y += (rand.next_double() * 0.05) - 0.025;
                        out.line(a, b, rand.next_double());
                    }
                }
            }
        }
        1 => render_outline(&map, width, fill, &mut out)?,
        2 => render_interlace(&map, width, gap, fill, &mut out)?,
        3 => render_emboss(&map, width, fill, &mut out)?,
        _ => {
            let (blk, wht) = extract_faces(&map)?;
            for pts in &wht {
                out.polygon(pts, fill, 0.0);
            }
            for pts in &blk {
                out.polygon(pts, fill, 1.0);
            }
        }
    }
    Some(out.0)
}

/// Primitives for the (limited) parameters, cached.
fn primitives(d: &[f64]) -> Arc<Vec<f64>> {
    type Cache = Mutex<HashMap<Vec<u64>, Arc<Vec<f64>>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let key: Vec<u64> = d.iter().map(|v| v.to_bits()).collect();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(p) = cache.lock().unwrap().get(&key) {
        return p.clone();
    }
    let name = PATTERNS[d[0] as usize];
    let built = build(name, d[1] as i32, d[2], d[3], d[4] as i32 == 1, d[5], d[6] as i32, d[7] as i32 == 1, d[8], d[9] as i32, d[10] as i32 == 1).unwrap_or_default();
    let prims = Arc::new(built.into_iter().flatten().collect::<Vec<f64>>());
    let mut c = cache.lock().unwrap();
    if c.len() >= 16 {
        c.clear();
    }
    c.insert(key, prims.clone());
    prims
}

// d = [Id, style, width, gap, fill, Rosette Q, Rosette S, Rosette(1/0), Star D, Star S,
//      Infer(1/0), count, start, primitives...]
const N_PARAMS: usize = 11;

fn prepare(p: &[f64]) -> Vec<f64> {
    let lim = limit_value;
    let mut d = vec![
        lim(p[0], 0.0, (PATTERNS.len() - 1) as f64).trunc(),
        lim(p[1], 0.0, 5.0).trunc(),
        p[2],
        p[3],
        lim(p[4], 0.0, 1.0).trunc(),
        lim(p[5], -1.0, 0.9),
        lim(p[6], 1.0, 2.0).trunc(),
        lim(p[7], 0.0, 1.0).trunc(),
        lim(p[8], 1.0, 3.5),
        lim(p[9], 1.0, 3.0).trunc(),
        lim(p[10], 0.0, 1.0).trunc(),
    ];
    let prims = primitives(&d);
    d.push((prims.len() / PRIM) as f64);
    d.push((N_PARAMS + 2) as f64);
    d.extend_from_slice(&prims);
    d
}

/// `DrawFunc.plotLine` with `line_thickness = 0.005` (`DrawFunc.init`).
fn plot_line(ctx: &mut Ctx, x1: f64, y1: f64, x2: f64, y2: f64) -> (f64, f64) {
    let (xdiff, ydiff) = (x2 - x1, y2 - y1);
    let m = if xdiff == 0.0 { 10000.0 } else { ydiff / xdiff };
    let line_length = (xdiff * xdiff + ydiff * ydiff).sqrt();
    let d = ctx.random() * line_length;
    let mut xoffset = d / (1.0 + m * m).sqrt();
    if x2 < x1 {
        xoffset = -xoffset;
    }
    let mut yoffset = (m * xoffset).abs();
    if y2 < y1 {
        yoffset = -yoffset;
    }
    xoffset += (ctx.random() - 0.5) * 0.005;
    yoffset += (ctx.random() - 0.5) * 0.005;
    (x1 + xoffset, y1 + yoffset)
}

fn apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let n = d[N_PARAMS] as usize;
    if n == 0 {
        return;
    }
    let g = &d[d[N_PARAMS + 1] as usize + PRIM * ((ctx.random() * n as f64) as usize).min(n - 1)..];
    let (x, y) = if g[0] == 2.0 { plot_line(ctx, g[2], g[3], g[4], g[5]) } else { triangle_point(ctx, [(g[2], g[3]), (g[4], g[5]), (g[6], g[7])]) };
    v.x += amount * x;
    v.y += amount * y;
    v.color = g[1];
    ctx.pass_z(a, v, amount);
}

pub static DEFS: &[VarDef] = &[VarDef {
    name: "taprats",
    params: &[
        ParamDef::int("Id", 0.0, 0.0, 48.0),
        ParamDef::int("style", 2.0, 0.0, 5.0),
        ParamDef::range("width", 0.06, 0.0, 0.5),
        ParamDef::range("gap", 0.08, 0.0, 0.5),
        ParamDef::int("fill", 0.0, 0.0, 1.0),
        ParamDef::range("Rosette Q", 0.0, -1.0, 0.9),
        ParamDef::int("Rosett  S", 2.0, 1.0, 2.0),
        ParamDef::int("Rosette(1/0)", 1.0, 0.0, 1.0),
        ParamDef::range("Star D", 3.0, 1.0, 3.5),
        ParamDef::int("Star S", 2.0, 1.0, 3.0),
        ParamDef::int("Infer(1/0)", 0.0, 0.0, 1.0),
    ],
    kind: VarKind::Normal,
    is_3d: false,
    apply: |d, ctx, a, v, amount| apply(d, ctx, a, v, amount),
    prepare: Some(|p, _| prepare(p)),
    wgsl: "let n = D(11);
if (n > 0.0) {
  let g = base + u32(D(12)) + 8u * min(u32(rnd() * n), u32(n) - 1u);
  var q: vec2<f32>;
  if (prm[g] < 2.5) {
    let x1 = prm[g + 2u];
    let y1 = prm[g + 3u];
    let x2 = prm[g + 4u];
    let y2 = prm[g + 5u];
    let xdiff = x2 - x1;
    let ydiff = y2 - y1;
    var m = 10000.0;
    if (xdiff != 0.0) { m = ydiff / xdiff; }
    var xo = rnd() * sqrt(xdiff * xdiff + ydiff * ydiff) / sqrt(1.0 + m * m);
    if (x2 < x1) { xo = -xo; }
    var yo = abs(m * xo);
    if (y2 < y1) { yo = -yo; }
    xo += (rnd() - 0.5) * 0.005;
    yo += (rnd() - 0.5) * 0.005;
    q = vec2<f32>(x1 + xo, y1 + yo);
  } else {
    q = res_triangle_point(vec2<f32>(prm[g + 2u], prm[g + 3u]), vec2<f32>(prm[g + 4u], prm[g + 5u]), vec2<f32>(prm[g + 6u], prm[g + 7u]));
  }
  v.x += AMOUNT * q.x;
  v.y += AMOUNT * q.y;
  v.color = prm[g + 1u];
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}",
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_patterns_are_built_in() {
        for name in PATTERNS {
            assert!(tilings().contains_key(*name), "{name}");
        }
    }

    #[test]
    fn every_pattern_and_style_gives_primitives() {
        for (id, name) in PATTERNS.iter().enumerate() {
            for style in 0..6 {
                for fill in [0.0, 1.0] {
                    let p = [id as f64, style as f64, 0.06, 0.08, fill, 0.0, 2.0, 1.0, 3.0, 2.0, 0.0];
                    let d = prepare(&p);
                    assert!(d[N_PARAMS] > 0.0, "{name} style {style} fill {fill}");
                    assert!(d[N_PARAMS + 2..].iter().all(|v| v.is_finite()), "{name} style {style}");
                }
            }
        }
    }

    #[test]
    fn stars_and_inferred_figures() {
        for (id, name) in PATTERNS.iter().enumerate() {
            let d = prepare(&[id as f64, 0.0, 0.06, 0.08, 0.0, 0.3, 2.0, 0.0, 2.5, 2.0, 1.0]);
            assert!(d[N_PARAMS] > 0.0, "{name}");
        }
    }

    /// Writes the primitives for the parameter lines of `TAPRATS_SPEC` to `TAPRATS_OUT`
    /// (`<line> <type> <color> <coords>`), for comparing with JWildfire.
    #[test]
    #[ignore]
    fn dump_primitives() {
        let spec = std::fs::read_to_string(std::env::var("TAPRATS_SPEC").expect("TAPRATS_SPEC")).unwrap();
        let mut out = String::new();
        for (no, line) in spec.lines().map(str::trim).filter(|l| !l.is_empty()).enumerate() {
            let p: Vec<f64> = line.split_whitespace().map(|t| t.parse().unwrap()).collect();
            let d = prepare(&p);
            for g in d[N_PARAMS + 2..].as_chunks::<PRIM>().0 {
                out += &format!("{no} {} {:.17e} {:.17e} {:.17e} {:.17e} {:.17e} {:.17e} {:.17e}\n", g[0] as i32, g[1], g[2], g[3], g[4], g[5], g[6], g[7]);
            }
        }
        std::fs::write(std::env::var("TAPRATS_OUT").expect("TAPRATS_OUT"), out).unwrap();
    }

    #[test]
    fn points_stay_near_the_view() {
        let d = prepare(&[1.0, 2.0, 0.06, 0.08, 1.0, 0.0, 2.0, 1.0, 3.0, 2.0, 0.0]);
        let mut ctx = Ctx::new(5, false);
        for _ in 0..2000 {
            let mut v = Pt::default();
            apply(&d, &mut ctx, &Pt::default(), &mut v, 1.0);
            assert!(!v.is_bad() && v.x.abs() < 4.0 && v.y.abs() < 4.0 && (0.0..=1.0).contains(&v.color), "{v:?}");
        }
    }
}
