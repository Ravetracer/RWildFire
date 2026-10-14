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

//! `lsystem3D_js` (Jesus Sosa, after Laurens Lapré's lparser): a 3D L-system drawn by a turtle
//! as cylinders or blocks and polygons; the triangles are sampled like `obj_mesh_wf`. The
//! grammar is a resource (lparser syntax) or one of JWildfire's 21 presets (`presetId` > 0).
//! The port keeps JWildfire's quirks (float vector math, the last command is never executed,
//! the characters of `(value)` arguments run as commands too). Differences: the random turns of
//! `~` use a fixed seed (the mesh does not change with every load), and a command JWildfire
//! fails on stops the drawing but keeps what was drawn so far.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::resource_vars::{M_HEADER, mesh_apply, mesh_prepare, pack_mesh, text};
use super::{ParamDef, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::math::ftoi;
use crate::mesh::Mesh;
use crate::rng::JavaRandom;

type V3 = [f32; 3];

const MAX_COLORS: usize = 100;
const MAX_P_OBJECT: usize = 400;
const MAX_STACK: usize = 1024;
const MAX_PROD: usize = 4 * 1024 * 1024;
const PI: f64 = 3.141592;

/// `strTok`: Java's port of C `strtok` (the position stays on the delimiter after a token).
struct StrTok {
    s: Vec<char>,
    pos: isize,
}

impl StrTok {
    fn new(s: &str) -> Self {
        Self { s: s.chars().collect(), pos: -1 }
    }

    fn next(&mut self, delims: &str) -> Option<String> {
        let n = self.s.len() as isize;
        if self.pos == n {
            return None;
        }
        self.pos += 1;
        while self.pos < n && delims.contains(self.s[self.pos as usize]) {
            self.pos += 1;
        }
        if self.pos == n {
            return None;
        }
        let start = self.pos as usize;
        loop {
            self.pos += 1;
            if !(self.pos < n && !delims.contains(self.s[self.pos as usize])) {
                break;
            }
        }
        Some(self.s[start..self.pos as usize].iter().collect())
    }
}

/// `Float.parseFloat` (surrounding whitespace and an `f`/`d` suffix allowed).
fn parse_float(s: &str) -> Option<f32> {
    let t = s.trim_matches(|c: char| c <= ' ');
    let t = t.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(t);
    t.parse::<f32>().ok()
}

fn parse_double(s: &str) -> Option<f64> {
    let t = s.trim_matches(|c: char| c <= ' ');
    let t = t.strip_suffix(['f', 'F', 'd', 'D']).unwrap_or(t);
    t.parse::<f64>().ok()
}

/// A parsed grammar (`readGrammar`), with the angle in radians and thicknesses as fractions.
struct Grammar {
    lev: i32,
    ang: f64,
    thick: f64,
    min_thick: f32,
    closed_form: bool,
    poly_limit: Option<i32>,
    axiom: String,
    rules: Vec<Vec<char>>,
    colors: [V3; MAX_COLORS],
}

fn default_colors() -> [V3; MAX_COLORS] {
    let mut c = [[0.5f32; 3]; MAX_COLORS];
    let set: [(usize, V3); 14] = [
        (2, [0.8, 0.4, 0.4]),
        (3, [0.8, 0.8, 0.4]),
        (4, [0.4, 0.8, 0.4]),
        (5, [0.4, 0.8, 0.8]),
        (6, [0.4, 0.4, 0.8]),
        (7, [0.8, 0.4, 0.8]),
        (8, [0.2, 0.5, 0.2]),
        (9, [0.2, 0.5, 0.5]),
        (10, [0.2, 0.2, 0.5]),
        (11, [0.5, 0.2, 0.5]),
        (12, [0.6, 0.2, 0.2]),
        (13, [0.5, 0.5, 0.5]),
        (14, [0.75, 0.75, 0.75]),
        (15, [1.0, 1.0, 1.0]),
    ];
    for (i, v) in set {
        c[i] = v;
    }
    c
}

/// `readGrammar`; `None` where JWildfire fails (then it builds no mesh).
fn read_grammar(input: &str) -> Option<Grammar> {
    if input.is_empty() {
        return None;
    }
    let mut g = Grammar {
        lev: 1,
        ang: 45.0,
        thick: 100.0,
        min_thick: 0.0,
        closed_form: true,
        poly_limit: None,
        axiom: String::new(),
        rules: Vec::new(),
        colors: default_colors(),
    };
    let mut closed = 1.0f64;
    let mut axiom: Option<String> = None;
    // BufferedReader.readLine: \n, \r or \r\n end a line.
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines: Vec<&str> = normalized.split('\n').collect();
    if normalized.ends_with('\n') {
        lines.pop();
    }
    for line in lines {
        if line.is_empty() || line.starts_with('#') || line.encode_utf16().count() < 5 {
            continue;
        }
        let mut tok = StrTok::new(line);
        let keyword = tok.next(" \r\n#")?;
        let command = tok.next("\r\n#");
        let int = |c: &Option<String>| c.as_deref().and_then(|s| s.parse::<i32>().ok());
        let float = |c: &Option<String>| c.as_deref().and_then(parse_float);
        match keyword.as_str() {
            "recursion" => g.lev = int(&command)?,
            "mutation" | "switch_yz" | "trace" | "no_wait" => {
                int(&command)?;
            }
            "angle" => g.ang = float(&command)? as f64,
            "thickness" => g.thick = float(&command)? as f64,
            "min_thickness" => g.min_thick = float(&command)?,
            "poly_limit" => g.poly_limit = Some(int(&command)?),
            "shape" => closed = float(&command)? as f64,
            "color" => {
                let c = command?;
                let mut parts = c.split(' ').filter(|s| !s.is_empty());
                let mut next_int = || parts.next().and_then(|s| s.parse::<i32>().ok());
                let (tt, r, gg, b) = (next_int()?, next_int()?, next_int()?, next_int()?);
                let slot = g.colors.get_mut(usize::try_from(tt).ok()?)?;
                *slot = [r as f32 / 255.0, gg as f32 / 255.0, b as f32 / 255.0];
            }
            "axiom" => {
                let tmp: String = command?.chars().filter(|c| !c.is_whitespace()).collect();
                axiom = Some(StrTok::new(&tmp).next("\r\n\t#")?);
            }
            "rule" => {
                let tmp: String = command?.chars().filter(|c| !c.is_whitespace()).collect();
                let mut t = StrTok::new(&tmp);
                // Java's string concatenation turns a missing part into "null".
                let r1 = t.next("=").unwrap_or_else(|| "null".into());
                let r2 = t.next("\r\n\t#").unwrap_or_else(|| "null".into());
                g.rules.push(format!("{r1}={r2}").chars().collect());
            }
            _ => return None,
        }
    }
    g.closed_form = closed == 1.0;
    g.axiom = axiom?;
    for r in [
        "+=+", "-=-", "&=&", "^=^", "<=<", ">=>", "%=%", "|=|", "!=!", "?=?", ":=:", ";=;", "'='", "\"=\"", "c=c", "[=[", "]=]", "{={", "}=}", "F=F",
        "f=f", "t=t", "g=g", "Z=Z", "z=z", "*=*", "$=$", "~=~", ".=.", "1=1", "2=2", "3=3", "4=4", "5=5", "6=6", "7=7", "8=8", "9=9", "0=0", "(=(",
        ")=)", "_=_",
    ] {
        g.rules.push(r.chars().collect());
    }
    g.ang = ((g.ang / 180.0) * PI) as f32 as f64;
    g.thick /= 100.0;
    g.min_thick = (g.min_thick as f64 / 100.0) as f32;
    Some(g)
}

/// `expand`: the production string (characters without a rule become `_`).
fn expand(g: &Grammar) -> Option<Vec<char>> {
    let num = g.rules.len();
    let mut lookup = [num - 1; 256];
    for i in (0..num).rev() {
        let c = *g.rules[i].first()? as usize;
        *lookup.get_mut(c)? = i;
    }
    let max = MAX_PROD - 50;
    let mut s: Vec<char> = g.axiom.chars().collect();
    for _ in 0..g.lev.max(0) {
        let mut out = Vec::new();
        let mut ss = 0usize;
        let mut overflow = false;
        for &c in &s {
            let i = *lookup.get(c as usize)?;
            let rule = &g.rules[i];
            ss += rule.len().saturating_sub(2);
            if ss >= max {
                overflow = true;
                break;
            }
            out.extend_from_slice(&rule[2.min(rule.len())..]);
        }
        s = out;
        if overflow {
            break;
        }
    }
    Some(s)
}

/// Color of a mesh triangle (`FaceWithColor`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum FaceColor {
    None,
    /// Palette index (`color_mode` 1).
    Index(f64),
    /// True color (`color_mode` 2; 0..1 for triangles, 0..255 for quads, as in JWildfire).
    Rgb(f64, f64, f64),
}

#[derive(Clone, Copy, Default)]
struct SRec {
    pos: V3,
    fow: V3,
    lef: V3,
    upp: V3,
    last: V3,
    last_v: [V3; 9],
    dis: f64,
    ang: f64,
    thick: f64,
    dis2: f64,
    tr: f64,
    col: i32,
    last_col: i32,
}

fn length(v: V3) -> f32 {
    ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) as f64).sqrt() as f32
}

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(v: &mut V3) {
    let l = length(*v);
    for c in v.iter_mut() {
        *c /= l;
    }
}

/// `float Dist = 1.0f / (float) sqrt(..); v *= Dist`
fn normalize_mul(v: &mut V3) {
    let dist = 1.0f32 / ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) as f64).sqrt() as f32;
    for c in v.iter_mut() {
        *c *= dist;
    }
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

/// `Vector_plus_fac`
fn plus_fac(a: V3, b: V3, t: f64) -> V3 {
    [(a[0] as f64 + t * b[0] as f64) as f32, (a[1] as f64 + t * b[1] as f64) as f32, (a[2] as f64 + t * b[2] as f64) as f32]
}

/// `p + s * a` in double precision, stored as float.
fn offset(p: V3, s: f64, a: V3) -> V3 {
    [(p[0] as f64 + s * a[0] as f64) as f32, (p[1] as f64 + s * a[1] as f64) as f32, (p[2] as f64 + s * a[2] as f64) as f32]
}

/// `p + s * a + t * b` in double precision, stored as float.
fn offset2(p: V3, s: f64, a: V3, t: f64, b: V3) -> V3 {
    std::array::from_fn(|k| (p[k] as f64 + s * a[k] as f64 + t * b[k] as f64) as f32)
}

/// The turtle (`L_draw`) and the mesh it builds (`Define_*`, `Save_object`).
struct Turtle<'a> {
    g: &'a Grammar,
    color_mode: i32,
    s: Vec<char>,
    rot: [V3; 3],
    ver: [V3; MAX_P_OBJECT],
    poly_store: [[usize; 4]; MAX_P_OBJECT + 1],
    last: V3,
    last_v: [V3; 9],
    last_col: i32,
    thick: f64,
    mesh: Mesh,
    colors: Vec<FaceColor>,
}

impl Turtle<'_> {
    /// `Set_rot`
    fn set_rot(&mut self, a: f32, n: V3) {
        let cosa = (a as f64).cos() as f32;
        let sina = (a as f64).sin() as f32;
        let (n11, n22, n33) = (n[0] * n[0], n[1] * n[1], n[2] * n[2]);
        let (nxy, nxz, nyz) = (n[0] * n[1], n[0] * n[2], n[1] * n[2]);
        self.rot = [
            [n11 + (1.0 - n11) * cosa, nxy * (1.0 - cosa) - n[2] * sina, nxz * (1.0 - cosa) + n[1] * sina],
            [nxy * (1.0 - cosa) + n[2] * sina, n22 + (1.0 - n22) * cosa, nyz * (1.0 - cosa) - n[0] * sina],
            [nxz * (1.0 - cosa) - n[1] * sina, nyz * (1.0 - cosa) + n[0] * sina, n33 + (1.0 - n33) * cosa],
        ];
    }

    /// `util_t`
    fn turn(&self, v: V3) -> V3 {
        [dot(self.rot[0], v), dot(self.rot[1], v), dot(self.rot[2], v)]
    }

    /// Rotates `a` and `b` with the current rotation and normalizes them.
    fn rotate_pair(&mut self, angle: f32, axis: V3, a: &mut V3, b: &mut V3) {
        self.set_rot(angle, axis);
        *a = self.turn(*a);
        *b = self.turn(*b);
        normalize(a);
        normalize(b);
    }

    /// `Get_value`: the number in `(..)` after position `j`.
    fn value(&self, j: usize) -> Option<f64> {
        let mut text = String::new();
        let mut k = j + 2;
        loop {
            let c = *self.s.get(k)?;
            if c == ')' {
                break;
            }
            text.push(c);
            k += 1;
        }
        parse_double(&text)
    }

    /// `Save_object`
    fn save_object(&mut self, vertices: usize, polygons: usize, color: i32) -> Option<()> {
        let mut nvert = [0u32; MAX_P_OBJECT];
        for t in 1..=vertices {
            let v = *self.ver.get(t)?;
            nvert[t - 1] = self.mesh.add_vertex(v[0] as f64, v[1] as f64, v[2] as f64, None);
        }
        for t in 1..=polygons {
            let p = self.poly_store[t];
            let v = |k: usize| nvert[p[k] - 1];
            let (v1, v2, v3) = (v(0), v(1), v(2));
            let color = |rgb_scale: bool| -> Option<FaceColor> {
                Some(match self.color_mode {
                    0 => FaceColor::None,
                    1 => FaceColor::Index(color as f64 / MAX_COLORS as f64),
                    _ => {
                        let c = *self.g.colors.get(usize::try_from(color.min(MAX_COLORS as i32 - 1)).ok()?)?;
                        if rgb_scale {
                            FaceColor::Rgb((c[0] as f64 * 256.0).floor(), (c[1] as f64 * 256.0).floor(), (c[2] as f64 * 256.0).floor())
                        } else {
                            FaceColor::Rgb(c[0] as f64, c[1] as f64, c[2] as f64)
                        }
                    }
                })
            };
            if p[2] == p[3] {
                let fc = color(false)?;
                self.mesh.add_face(v1, v2, v3);
                self.colors.push(fc);
            } else {
                let fc = color(true)?;
                self.mesh.add_quad(v1, v2, v3, v(3));
                self.colors.extend([fc, fc]);
            }
        }
        Some(())
    }

    /// `Define_block`
    fn define_block(&mut self, p1: V3, p2: V3, up: V3, c: i32) -> Option<()> {
        let dis = [p2[0] - p1[0], p2[1] - p1[1], p2[2] - p1[2]];
        let d = ((dis[0] * dis[0] + dis[1] * dis[1] + dis[2] * dis[2]) as f64).sqrt() as f32 as f64;
        if d == 0.0 {
            return Some(());
        }
        let mut s = d * self.thick;
        if s < self.g.min_thick as f64 {
            s = self.g.min_thick as f64;
        }
        s *= 0.7071;
        let mut d1 = dis;
        normalize_mul(&mut d1);
        let mut d2 = up;
        normalize_mul(&mut d2);
        let mut d3 = cross(d1, d2);
        normalize_mul(&mut d3);
        let mut b = [d2[0] + d3[0], d2[1] + d3[1], d2[2] + d3[2]];
        normalize_mul(&mut b);
        self.ver[1] = offset(p1, s, b);
        self.ver[3] = offset(p1, -s, b);
        let mut b = [d2[0] - d3[0], d2[1] - d3[1], d2[2] - d3[2]];
        normalize_mul(&mut b);
        self.ver[2] = offset(p1, s, b);
        self.ver[4] = offset(p1, -s, b);
        for i in 1..=4 {
            self.ver[i + 4] = [self.ver[i][0] + dis[0], self.ver[i][1] + dis[1], self.ver[i][2] + dis[2]];
        }
        self.poly_store[1..=6].copy_from_slice(&[[1, 5, 6, 2], [2, 6, 7, 3], [3, 7, 8, 4], [4, 8, 5, 1], [1, 2, 3, 4], [8, 7, 6, 5]]);
        self.save_object(8, 6, c)
    }

    /// `Define_closed`: a cylinder connected to the previous one of the same color.
    fn define_closed(&mut self, p1: V3, p2: V3, up: V3, c: i32) -> Option<()> {
        let mut dis = [p2[0] - p1[0], p2[1] - p1[1], p2[2] - p1[2]];
        let mut d = ((dis[0] * dis[0] + dis[1] * dis[1] + dis[2] * dis[2]) as f64).sqrt() as f32 as f64;
        if d == 0.0 {
            return Some(());
        }
        let mut s = d * self.thick;
        if s < self.g.min_thick as f64 {
            s = self.g.min_thick as f64;
        }
        s *= 0.5f32 as f64;
        let mut d1 = dis;
        normalize_mul(&mut d1);
        let mut d2 = up;
        normalize_mul(&mut d2);
        let mut d3 = cross(d1, d2);
        normalize_mul(&mut d3);
        let mut t1 = [d2[0] + d3[0], d2[1] + d3[1], d2[2] + d3[2]];
        normalize_mul(&mut t1);
        let mut t2 = [d2[0] - d3[0], d2[1] - d3[1], d2[2] - d3[2]];
        normalize_mul(&mut t2);
        self.ver[1] = offset(p1, s, t1);
        self.ver[5] = offset(p1, -s, t1);
        self.ver[3] = offset(p1, s, t2);
        self.ver[7] = offset(p1, -s, t2);
        s *= 0.7071f32 as f64;
        self.ver[2] = offset2(p1, s, t1, s, t2);
        self.ver[4] = offset2(p1, -s, t1, s, t2);
        self.ver[6] = offset2(p1, -s, t1, -s, t2);
        self.ver[8] = offset2(p1, s, t1, -s, t2);
        for i in 1..=8 {
            self.ver[i + 8] = [self.ver[i][0] + dis[0], self.ver[i][1] + dis[1], self.ver[i][2] + dis[2]];
        }
        if self.last_col == c {
            dis = [p1[0] - self.last[0], p1[1] - self.last[1], p1[2] - self.last[2]];
            d = ((dis[0] * dis[0] + dis[1] * dis[1] + dis[2] * dis[2]) as f64).sqrt() as f32 as f64;
            if d < 1.0 {
                let mut dd = 1e30f32 as f64;
                let mut ii = 0usize;
                for i in 1..=8 {
                    let v = [self.ver[1][0] - self.last_v[i][0], self.ver[1][1] - self.last_v[i][1], self.ver[1][2] - self.last_v[i][2]];
                    let dist = ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) as f64).sqrt() as f32 as f64;
                    if dist < dd {
                        dd = dist;
                        ii = i;
                    }
                }
                for i in 1..=8 {
                    self.ver[i] = self.last_v[ii];
                    ii = (ii + 1) % 9;
                    if ii == 0 {
                        ii = 1;
                    }
                }
            }
        }
        for k in 1..=8 {
            self.poly_store[k] = [k, k + 8, if k == 8 { 9 } else { k + 9 }, if k == 8 { 1 } else { k + 1 }];
        }
        self.save_object(16, 8, c)?;
        self.last_col = c;
        self.last = p2;
        for i in 1..=8 {
            self.last_v[i] = self.ver[i + 8];
        }
        Some(())
    }

    /// `L_draw`; `None` where JWildfire throws (drawing stops there).
    fn draw(&mut self) -> Option<()> {
        let g = self.g;
        let (mut pos, mut fow, mut lef, mut upp): (V3, V3, V3, V3) = ([0.0; 3], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]);
        let sky: V3 = [0.0, 0.0, 1.0];
        let mut trope: V3 = [f32::NAN; 3];
        let mut stack = vec![SRec::default(); MAX_STACK];
        let mut scount = 0usize;
        let (mut dis, mut dis2, mut tr, mut ang) = (1.0f64, 0.5f64, 0.2f64, g.ang);
        let mut col = 2i32;
        let mut pstack: Vec<(usize, Vec<V3>)> = Vec::new();
        let mut pscount = 0usize;
        let (mut vcount, mut pcount) = (0usize, 0usize);
        let mut poly_on = false;
        let mut rng = JavaRandom::new(0);
        let n = self.s.len();
        for i in 0..n.saturating_sub(1) {
            let next = self.s[i + 1];
            if g.poly_limit.is_some_and(|l| l < 0) {
                break;
            }
            let value = |t: &Self| if next == '(' { t.value(i).map(Some) } else { Some(None) };
            match self.s[i] {
                c @ ('+' | '-' | '&' | '^' | '<' | '>') => {
                    let a = value(self)?.map_or(ang, |v| 0.017453 * v);
                    let signed = if matches!(c, '+' | '^' | '<') { -a } else { a };
                    match c {
                        '+' | '-' => self.rotate_pair(signed as f32, upp, &mut fow, &mut lef),
                        '&' | '^' => self.rotate_pair(signed as f32, lef, &mut fow, &mut upp),
                        _ => self.rotate_pair(signed as f32, fow, &mut lef, &mut upp),
                    }
                }
                '~' => {
                    let r = value(self)?.map_or(6.0, |v| 0.017453 * v);
                    let a = rng.next_double() * r * 2.0 - r;
                    self.rotate_pair(a as f32, upp, &mut fow, &mut lef);
                    let a = rng.next_double() * r * 2.0 - r;
                    self.rotate_pair(a as f32, lef, &mut fow, &mut upp);
                    let a = rng.next_double() * r * 2.0 - r;
                    self.rotate_pair(a as f32, fow, &mut lef, &mut upp);
                }
                't' => {
                    if fow[0] == 0.0 && fow[1] == 0.0 {
                        continue;
                    }
                    let t = value(self)?.unwrap_or(tr);
                    trope = [-fow[0], -fow[1], 0.0];
                    normalize(&mut trope);
                    let r = t * dot(fow, trope) as f64;
                    self.rotate_pair(-r as f32, lef, &mut fow, &mut upp);
                }
                '$' => {
                    let v = [fow[0] - sky[0], fow[1] - sky[1], fow[2] - sky[2]];
                    if length(v) == 0.0 {
                        continue;
                    }
                    lef = cross(fow, sky);
                    upp = cross(fow, lef);
                    if upp[2] < 0.0 {
                        upp = upp.map(|c| -c);
                        lef = lef.map(|c| -c);
                    }
                }
                '%' => self.rotate_pair(PI as f32, fow, &mut lef, &mut upp),
                '|' => self.rotate_pair(PI as f32, upp, &mut fow, &mut lef),
                '!' => self.thick = value(self)?.map_or(self.thick * 0.7, |v| self.thick * v),
                '?' => self.thick = value(self)?.map_or(self.thick / 0.7, |v| self.thick * v),
                ':' => ang = value(self)?.map_or(ang * 0.9, |v| ang * v),
                ';' => ang = value(self)?.map_or(ang / 0.9, |v| ang * v),
                c @ ('\'' | '"') => match value(self)? {
                    Some(r) => {
                        dis *= r;
                        dis2 *= r;
                    }
                    None if c == '\'' => {
                        dis *= 0.9;
                        dis2 *= 0.9;
                    }
                    None => {
                        dis /= 0.9;
                        dis2 /= 0.9;
                    }
                },
                c @ ('F' | 'Z') => {
                    let base = if c == 'F' { dis } else { dis2 };
                    let len = value(self)?.unwrap_or(base);
                    let end = plus_fac(pos, fow, len);
                    if g.closed_form {
                        self.define_closed(pos, end, upp, col)?;
                    } else {
                        self.define_block(pos, end, upp, col)?;
                    }
                    pos = end;
                }
                '[' => {
                    let r = stack.get_mut(scount)?;
                    *r = SRec { pos, fow, lef, upp, dis, dis2, ang, thick: self.thick, tr, col, ..*r };
                    if g.closed_form {
                        r.last = self.last;
                        r.last_col = self.last_col;
                        r.last_v[1..=8].copy_from_slice(&self.last_v[1..=8]);
                    }
                    scount += 1;
                }
                ']' => {
                    scount = scount.checked_sub(1)?;
                    let r = stack[scount];
                    (pos, fow, lef, upp, col, dis, dis2, ang, self.thick, tr) =
                        (r.pos, r.fow, r.lef, r.upp, r.col, r.dis, r.dis2, r.ang, r.thick, r.tr);
                    if g.closed_form {
                        self.last = r.last;
                        self.last_col = r.last_col;
                        self.last_v[1..=8].copy_from_slice(&r.last_v[1..=8]);
                    }
                }
                '{' => {
                    if poly_on {
                        // The polygon stack is an ArrayList that `}` reads by index but never
                        // shrinks (as in JWildfire).
                        pstack.push((vcount, self.ver[..vcount].to_vec()));
                        pscount += 1;
                    }
                    poly_on = true;
                    vcount = 1;
                    pcount = 1;
                }
                c @ ('f' | 'z' | 'g') => {
                    let base = if c == 'z' { dis2 } else { dis };
                    let len = value(self)?.unwrap_or(base);
                    pos = plus_fac(pos, fow, len);
                    if poly_on && c != 'g' {
                        *self.ver.get_mut(vcount)? = pos;
                        vcount += 1;
                    }
                }
                '.' => {
                    if poly_on {
                        *self.ver.get_mut(vcount)? = pos;
                        vcount += 1;
                    }
                }
                '}' => {
                    if vcount > 3 {
                        for j in 1..vcount - 2 {
                            *self.poly_store.get_mut(pcount)? = [1, j + 1, j + 2, j + 2];
                            pcount += 1;
                        }
                        self.save_object(vcount - 1, pcount - 1, col)?;
                    }
                    poly_on = false;
                    if pscount > 0 {
                        pscount -= 1;
                        let (count, vers) = pstack.get(pscount)?.clone();
                        self.ver[..count].copy_from_slice(&vers);
                        vcount = count;
                        poly_on = true;
                    }
                }
                'c' => col = value(self)?.map_or(col + 1, |v| v as i32),
                _ => {}
            }
            let _ = trope;
        }
        Some(())
    }
}

/// The mesh of a grammar and its triangle colors (cached).
fn build(grammar: &str, color_mode: i32) -> Arc<(Mesh, Vec<FaceColor>)> {
    type Cache = Mutex<HashMap<(String, i32), Arc<(Mesh, Vec<FaceColor>)>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let key = (grammar.to_string(), color_mode);
    if let Some(m) = cache.lock().expect("lsystem3D cache").get(&key) {
        return m.clone();
    }
    let result = match read_grammar(grammar).and_then(|g| expand(&g).map(|s| (g, s))) {
        Some((g, s)) => {
            let mut t = Turtle {
                g: &g,
                color_mode,
                s,
                rot: [[0.0; 3]; 3],
                ver: [[0.0; 3]; MAX_P_OBJECT],
                poly_store: [[0; 4]; MAX_P_OBJECT + 1],
                last: [0.0; 3],
                last_v: [[0.0; 3]; 9],
                last_col: 0,
                thick: g.thick,
                mesh: Mesh::default(),
                colors: Vec::new(),
            };
            let _ = t.draw();
            (t.mesh, t.colors)
        }
        None => (Mesh::default(), Vec::new()),
    };
    let m = Arc::new(result);
    cache.lock().expect("lsystem3D cache").insert(key, m.clone());
    m
}

/// `getGrammarId`: the grammar after marker `##NN` up to `##END` (the last one if `id` is not
/// found).
fn preset_grammar(file: &str, id: i32) -> String {
    let mut grammar = String::new();
    let b = file.as_bytes();
    let mut i = 0;
    while i + 4 <= b.len() {
        if b[i] == b'#' && b[i + 1] == b'#' && b[i + 2].is_ascii_digit() && b[i + 3].is_ascii_digit() {
            let number = ((b[i + 2] - b'0') * 10 + (b[i + 3] - b'0')) as i32;
            let rest = &file[i + 4..];
            grammar = match rest.find("##END") {
                Some(end) => rest[..end + 1].to_string(),
                None => String::new(),
            };
            if number == id {
                break;
            }
            i += 4;
        } else {
            i += 1;
        }
    }
    grammar
}

/// Data: [presetId, color_mode, scale_x, .. receive_only_shadows (the obj_mesh_wf parameters),
/// mesh header, face color start (0 = none), .., mesh, face colors (kind, a, b, c) ..].
const MESH: usize = 2;
const FCOL: usize = MESH + 14 + M_HEADER;

pub static DEFS: &[VarDef] = &[VarDef {
    name: "lsystem3D_js",
    params: &[
        ParamDef::int("presetId", 1.0, 0.0, 21.0),
        ParamDef::range("scale_x", 1.0, 0.0, 4.0),
        ParamDef::range("scale_y", 1.0, 0.0, 4.0),
        ParamDef::range("scale_z", 1.0, 0.0, 4.0),
        ParamDef::new("offset_x", 0.0),
        ParamDef::new("offset_y", 0.0),
        ParamDef::new("offset_z", 0.0),
        ParamDef::int("color_mode", 0.0, 0.0, 2.0),
        ParamDef::int("subdiv_level", 0.0, 0.0, 6.0),
        ParamDef::int("subdiv_smooth_passes", 12.0, 0.0, 24.0),
        ParamDef::range("subdiv_smooth_lambda", 0.42, 0.0, 1.0),
        ParamDef::range("subdiv_smooth_mu", -0.45, -1.0, 0.0),
        ParamDef::int("blend_colormap", 1.0, 0.0, 1.0),
        ParamDef::new("displ_amount", 0.1),
        ParamDef::int("blend_displ_map", 1.0, 0.0, 1.0),
        ParamDef::int("receive_only_shadows", 0.0, 0.0, 1.0),
    ],
    kind: VarKind::Normal,
    is_3d: true,
    apply: |d, ctx, _, v, amount| {
        if let Some(f) = mesh_apply(d, MESH, ctx, v, amount)
            && d[FCOL] > 0.0
        {
            let c = d[FCOL] as usize + 4 * f;
            match d[c] as i32 {
                1 => v.color = d[c + 1],
                2 => {
                    v.rgb_color = true;
                    (v.red, v.green, v.blue) = (d[c + 1], d[c + 2], d[c + 3]);
                }
                _ => {}
            }
        }
    },
    prepare: Some(|p, _| {
        // Reordered so that the mesh parameters are contiguous (obj_mesh_wf layout).
        let mut params = vec![(p[0] as i64) as f64, ftoi(p[7]).clamp(0, 2) as f64];
        params.extend_from_slice(&p[1..7]);
        params.extend_from_slice(&p[8..16]);
        let mut d = mesh_prepare(&params, MESH);
        d.push(0.0);
        d
    }),
    wgsl: "v = res_mesh(base, 2u, v, AMOUNT);
let cs = u32(D(27));
if (cs > 0u && D(24) >= 1.0) {
  let c = base + cs + 4u * res_mesh_face;
  let kind = prm[c];
  if (kind == 1.0) {
    v.color = prm[c + 1u];
  } else if (kind == 2.0) {
    v.rgbc = 1.0; v.r = prm[c + 1u]; v.g = prm[c + 2u]; v.b = prm[c + 3u];
  }
}",
}];

pub static RESOURCES: &[ResourceVar] = &[ResourceVar {
    name: "lsystem3D_js",
    resources: &[ResDef { name: "grammar", kind: ResKind::Text, default: "" }],
    load: |d, res, _| {
        let preset = d[0] as i32;
        let grammar = if preset > 0 { preset_grammar(include_str!("../../assets/lsystems3D.txt"), preset) } else { text(&res[0]) };
        let built = build(&grammar, d[1] as i32);
        let (mesh, colors) = (&built.0, &built.1);
        pack_mesh(d, MESH + 14, mesh);
        if colors.iter().any(|c| *c != FaceColor::None) {
            d[FCOL] = d.len() as f64;
            d.reserve(4 * colors.len());
            for c in colors {
                d.extend(match *c {
                    FaceColor::None => [0.0; 4],
                    FaceColor::Index(i) => [1.0, i, 0.0, 0.0],
                    FaceColor::Rgb(r, g, b) => [2.0, r, g, b],
                });
            }
        }
    },
}];

#[cfg(test)]
mod tests {
    use super::super::resource_vars::M_TOTAL;
    use super::*;

    const PRESETS: &str = include_str!("../../assets/lsystems3D.txt");

    #[test]
    fn layout_constants_match_wgsl() {
        assert_eq!((FCOL, MESH + 14 + M_TOTAL), (27, 24));
    }

    #[test]
    fn presets_build_meshes() {
        for id in 1..=21 {
            let g = preset_grammar(PRESETS, id);
            assert!(read_grammar(&g).is_some(), "preset {id} does not parse");
            let m = build(&g, 0);
            assert!(!m.0.faces.is_empty(), "preset {id} has no faces");
        }
    }

    #[test]
    fn strtok_like_c() {
        let mut t = StrTok::new("  rule X = F # c");
        assert_eq!(t.next(" \r\n#").as_deref(), Some("rule"));
        assert_eq!(t.next("\r\n#").as_deref(), Some("X = F "));
        // Like strtok, the next call skips the '#' (readGrammar never asks again).
        assert_eq!(t.next("\r\n#").as_deref(), Some(" c"));
        assert_eq!(t.next("\r\n#"), None);
    }

    #[test]
    fn a_cylinder_has_eight_sides() {
        // Two characters: the last one is never executed.
        let m = build("recursion 1\naxiom FF\n", 0);
        assert_eq!(m.0.faces.len(), 16);
        assert_eq!(m.0.vertices.len(), 16);
        let colored = build("recursion 1\naxiom FF\n", 2);
        assert!(colored.1.iter().all(|c| matches!(c, FaceColor::Rgb(r, ..) if (*r - 204.0).abs() < 1e-9)));
    }
}
