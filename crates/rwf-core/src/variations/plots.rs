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

//! Formula plot variations: `parplot2d_wf`, `polarplot2d_wf`, `polarplot3d_wf`, `yplot2d_wf`,
//! `yplot3d_wf`, `isosfplot3d_wf`.
//!
//! Data layout: the parameters, then `(min, delta)` of every sorted range, the colormap image
//! block, the displacement map image block (not for `isosfplot3d_wf`) and `[start, length]` of
//! every formula program (length 0: the formula did not compile, the variation does nothing).

use std::collections::HashMap;

use super::resource_vars::{IMG, apply_image_color, ftoi_limit, image_displacement, load_image, text};
use super::{Ctx, LoadEnv, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::formula;
use crate::math::*;

/// Description of a plot variation's data layout.
struct Spec {
    /// Number of parameters.
    params: usize,
    /// `(min, max)` parameter indices of the ranges.
    ranges: &'static [(usize, usize)],
    /// Integer parameters (`Tools.FTOI`, optionally limited).
    ints: &'static [(usize, Option<(i32, i32)>)],
    /// Index of `param_a`.
    param_a: usize,
    /// Formula resource names and the runtime variable names.
    formulas: &'static [&'static str],
    vars: &'static [&'static str],
    /// An empty formula means "no evaluator" (all but `parplot2d_wf`, which uses `0.0`).
    empty_invalid: bool,
    displ: bool,
}

impl Spec {
    const fn cm(&self) -> usize {
        self.params + 2 * self.ranges.len()
    }
    const fn dm(&self) -> usize {
        self.cm() + IMG
    }
    const fn formula(&self, i: usize) -> usize {
        self.cm() + if self.displ { 2 * IMG } else { IMG } + 2 * i
    }
    fn range(&self, d: &[f64], i: usize) -> (f64, f64) {
        let o = self.params + 2 * i;
        (d[o], d[o + 1])
    }

    fn prepare(&self, p: &[f64]) -> Vec<f64> {
        let mut d = p.to_vec();
        for &(i, lim) in self.ints {
            d[i] = match lim {
                Some((lo, hi)) => ftoi_limit(p[i], lo, hi),
                None => ftoi(p[i]) as f64,
            };
        }
        for &(a, b) in self.ranges {
            let (lo, hi) = if p[a] > p[b] { (p[b], p[a]) } else { (p[a], p[b]) };
            d.extend([lo, hi - lo]);
        }
        d.resize(self.formula(self.formulas.len()), 0.0);
        d
    }

    /// Resources: formulas first, then the colormap and (if any) the displacement map.
    fn load(&self, d: &mut Vec<f64>, res: &[Vec<u8>]) {
        let nf = self.formulas.len();
        load_image(d, self.cm(), &res[nf]);
        if self.displ {
            load_image(d, self.dm(), &res[nf + 1]);
        }
        let mut consts = HashMap::new();
        consts.insert("pi".to_string(), M_PI);
        for (k, name) in super::presets::param_names().iter().enumerate() {
            consts.insert(name.to_string(), d[self.param_a + k]);
        }
        for (i, res) in res.iter().enumerate().take(nf) {
            let src = text(res);
            let slot = self.formula(i);
            if self.empty_invalid && src.trim().is_empty() {
                continue;
            }
            if let Ok(p) = formula::compile(&src, self.vars, &consts) {
                d[slot] = d.len() as f64;
                d[slot + 1] = p.code.len() as f64;
                d.extend(p.code);
            }
        }
    }

    fn valid(&self, d: &[f64]) -> bool {
        (0..self.formulas.len()).all(|i| d[self.formula(i) + 1] > 0.0)
    }

    fn eval(&self, d: &[f64], i: usize, vars: &[f64]) -> f64 {
        let slot = self.formula(i);
        let (s, n) = (d[slot] as usize, d[slot + 1] as usize);
        formula::eval(&d[s..s + n], vars)
    }

    /// Displacement along `n` by the map at `(ru, rv)` (`displacementMapHolder.isActive()`).
    #[allow(clippy::too_many_arguments)]
    fn displace(&self, d: &[f64], displ_amount: f64, blend: bool, n: [f64; 3], ru: f64, rv: f64, v: &mut Pt, amount: f64) {
        let o = self.dm();
        if d[o] > 0.0 && displ_amount.abs() > EPSILON {
            let (iw, ih) = (d[o] - 1.0, d[o + 1] - 1.0);
            let dd = image_displacement(d, o, blend, (ru * iw).clamp(0.0, iw), (ih - rv * ih).clamp(0.0, ih)) * displ_amount;
            v.x += amount * n[0] * dd;
            v.y += amount * n[1] * dd;
            v.z += amount * n[2] * dd;
        }
    }

    /// Colormap coloring at `(ru, rv)` (`CM_COLORMAP`).
    fn colormap(&self, d: &[f64], blend: bool, ru: f64, rv: f64, ctx: &mut Ctx, v: &mut Pt) {
        let o = self.cm();
        if d[o] > 0.0 {
            let (iw, ih) = (d[o] - 1.0, d[o + 1] - 1.0);
            apply_image_color(d, o, blend, v, (ru * iw).clamp(0.0, iw), (ih - rv * ih).clamp(0.0, ih));
            v.color = ctx.uv_color_idx(ftoi(v.red), ftoi(v.green), ftoi(v.blue));
        }
    }
}

/// Compiles a formula of a plot variation (with all `param_*` as 0) to report errors.
pub fn check_formula(variation: &str, src: &str) -> Result<(), String> {
    let spec = match variation {
        "parplot2d_wf" => &PARPLOT,
        "polarplot2d_wf" => &POLAR2D,
        "polarplot3d_wf" => &POLAR3D,
        "yplot2d_wf" => &YPLOT2D,
        "yplot3d_wf" => &YPLOT3D,
        "isosfplot3d_wf" => &ISOSF,
        _ => return Ok(()),
    };
    let mut consts = HashMap::new();
    consts.insert("pi".to_string(), M_PI);
    for name in super::presets::param_names() {
        consts.insert(name.to_string(), 0.0);
    }
    formula::compile(src, spec.vars, &consts).map(|_| ())
}

/// `VectorD.cross` + `normalize`
fn normal(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    let c = [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt() + 1.0e-16;
    [c[0] / l, c[1] / l, c[2] / l]
}

macro_rules! abcdef {
    () => {
        [
            ParamDef::new("param_a", 0.0),
            ParamDef::new("param_b", 0.0),
            ParamDef::new("param_c", 0.0),
            ParamDef::new("param_d", 0.0),
            ParamDef::new("param_e", 0.0),
            ParamDef::new("param_f", 0.0),
        ]
    };
}

const fn concat<const A: usize, const B: usize, const C: usize>(a: [ParamDef; A], b: [ParamDef; B]) -> [ParamDef; C] {
    let mut out = [ParamDef::new("", 0.0); C];
    let mut i = 0;
    while i < A {
        out[i] = a[i];
        i += 1;
    }
    while i < C {
        out[i] = b[i - A];
        i += 1;
    }
    out
}

const PRESET: ParamDef = ParamDef::int("preset_id", 0.0, -1.0, 200.0);
const BLEND: ParamDef = ParamDef::int("blend_colormap", 1.0, 0.0, 1.0);
const DISPL: ParamDef = ParamDef::new("displ_amount", 0.1);
const BLEND_DISPL: ParamDef = ParamDef::int("blend_displ_map", 1.0, 0.0, 1.0);
const DIRECT: ParamDef = ParamDef::int("direct_color", 1.0, 0.0, 1.0);

// ---------------------------------------------------------------------------------------------
// parplot2d_wf

static PARPLOT_PARAMS: [ParamDef; 17] = concat(
    [
        PRESET,
        ParamDef::new("umin", 0.0),
        ParamDef::new("umax", M_2PI),
        ParamDef::new("vmin", 0.0),
        ParamDef::new("vmax", M_2PI),
        DIRECT,
        ParamDef::int("color_mode", 3.0, 0.0, 3.0),
        BLEND,
        DISPL,
        BLEND_DISPL,
        ParamDef::int("solid", 1.0, 0.0, 1.0),
    ],
    abcdef!(),
);

static PARPLOT: Spec = Spec {
    params: 17,
    ranges: &[(1, 2), (3, 4)],
    ints: &[(0, None), (5, None), (6, None), (7, Some((0, 1))), (9, Some((0, 1))), (10, None)],
    param_a: 11,
    formulas: &["xformula", "yformula", "zformula"],
    vars: &["u", "v"],
    empty_invalid: false,
    displ: true,
};

fn parplot_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let s = &PARPLOT;
    if !s.valid(d) {
        return;
    }
    let ((u0, du), (v0, dv)) = (s.range(d, 0), s.range(d, 1));
    let (ru, rv) = if d[10] as i32 == 0 { (a.x, a.y) } else { (ctx.random(), ctx.random()) };
    let (u, w) = (u0 + ru * du, v0 + rv * dv);
    let p = |u: f64, w: f64| [s.eval(d, 0, &[u, w]), s.eval(d, 1, &[u, w]), s.eval(d, 2, &[u, w])];
    let q = p(u, w);
    if d[s.dm()] > 0.0 && d[8].abs() > EPSILON {
        let (q1, q2) = (p(u + du / 100.0, w), p(u, w + dv / 100.0));
        let n = normal([q1[0] - q[0], q1[1] - q[1], q1[2] - q[2]], [q2[0] - q[0], q2[1] - q[1], q2[2] - q[2]]);
        s.displace(d, d[8], d[9] > 0.0, n, ru, rv, v, amount);
    }
    if d[5] > 0.0 {
        let (fu, fv) = ((u - u0) / du, (w - v0) / dv);
        match d[6] as i32 {
            2 => v.color = fv,
            1 => v.color = fu,
            0 => s.colormap(d, d[7] > 0.0, ru, rv, ctx, v),
            _ => v.color = fv * fu,
        }
        v.color = v.color.clamp(0.0, 1.0);
    }
    v.x += amount * q[0];
    v.y += amount * q[1];
    v.z += amount * q[2];
}

// ---------------------------------------------------------------------------------------------
// polarplot2d_wf

static POLAR2D_PARAMS: [ParamDef; 18] = concat(
    [
        PRESET,
        ParamDef::new("tmin", -3.0),
        ParamDef::new("tmax", 2.0),
        ParamDef::new("rmin", 0.0),
        ParamDef::new("rmax", 2.0),
        ParamDef::new("zmin", -2.0),
        ParamDef::new("zmax", 2.0),
        DIRECT,
        ParamDef::int("color_mode", 1.0, 0.0, 2.0),
        BLEND,
        DISPL,
        BLEND_DISPL,
    ],
    abcdef!(),
);

static POLAR2D: Spec = Spec {
    params: 18,
    ranges: &[(1, 2), (3, 4), (5, 6)],
    ints: &[(0, None), (7, None), (8, None), (9, Some((0, 1))), (11, Some((0, 1)))],
    param_a: 12,
    formulas: &["formula"],
    vars: &["t"],
    empty_invalid: true,
    displ: true,
};

fn polar2d_apply(d: &[f64], ctx: &mut Ctx, _: &Pt, v: &mut Pt, amount: f64) {
    let s = &POLAR2D;
    if !s.valid(d) {
        return;
    }
    let ((t0, dt), (r0, dr), (z0, dz)) = (s.range(d, 0), s.range(d, 1), s.range(d, 2));
    let (ru, rv) = (ctx.random(), ctx.random());
    let (t, z) = (t0 + ru * dt, z0 + rv * dz);
    let r = s.eval(d, 0, &[t]);
    let (x, y) = (r * t.cos(), r * t.sin());
    if d[s.dm()] > 0.0 && d[10].abs() > EPSILON {
        let eps = dt / 100.0;
        let t1 = t + eps;
        let y1 = s.eval(d, 0, &[t1]) * t1.sin();
        // as in JWildfire, the x difference is eps
        let n = normal([eps, y1 - y, 0.0], [0.0, 0.0, 1.0]);
        s.displace(d, d[10], d[11] > 0.0, n, ru, rv, v, amount);
    }
    if d[7] > 0.0 {
        match d[8] as i32 {
            0 => s.colormap(d, d[9] > 0.0, ru, rv, ctx, v),
            2 => v.color = (r - r0) / dr,
            _ => v.color = (t - t0) / dt,
        }
        v.color = v.color.clamp(0.0, 1.0);
    }
    v.x += amount * x;
    v.y += amount * y;
    v.z += amount * z;
}

// ---------------------------------------------------------------------------------------------
// polarplot3d_wf

static POLAR3D_PARAMS: [ParamDef; 19] = concat(
    [
        PRESET,
        ParamDef::new("tmin", -M_PI),
        ParamDef::new("tmax", M_PI),
        ParamDef::new("umin", 0.0),
        ParamDef::new("umax", M_PI),
        ParamDef::new("rmin", -2.0),
        ParamDef::new("rmax", 2.0),
        ParamDef::int("cylindrical", 0.0, 0.0, 1.0),
        DIRECT,
        ParamDef::int("color_mode", 2.0, 0.0, 4.0),
        BLEND,
        DISPL,
        BLEND_DISPL,
    ],
    abcdef!(),
);

static POLAR3D: Spec = Spec {
    params: 19,
    ranges: &[(1, 2), (3, 4), (5, 6)],
    ints: &[(0, None), (7, None), (8, None), (9, None), (10, Some((0, 1))), (12, Some((0, 1)))],
    param_a: 13,
    formulas: &["formula"],
    vars: &["t", "u"],
    empty_invalid: true,
    displ: true,
};

fn polar3d_apply(d: &[f64], ctx: &mut Ctx, _: &Pt, v: &mut Pt, amount: f64) {
    let s = &POLAR3D;
    if !s.valid(d) {
        return;
    }
    let ((t0, dt), (u0, du), (r0, dr)) = (s.range(d, 0), s.range(d, 1), s.range(d, 2));
    let (rt, ru) = (ctx.random(), ctx.random());
    let (t, u) = (t0 + rt * dt, u0 + ru * du);
    let r = s.eval(d, 0, &[t, u]);
    let cyl = d[7] as i32 != 0;
    let (x, y, z) = if cyl { (r * t.cos(), r * t.sin(), u) } else { (r * u.sin() * t.cos(), r * u.sin() * t.sin(), r * u.cos()) };
    if d[s.dm()] > 0.0 && d[11].abs() > EPSILON {
        let ycoord = |r: f64, t: f64, u: f64| if cyl { r * t.sin() } else { r * u.sin() * t.sin() };
        let (et, eu) = (dt / 100.0, du / 100.0);
        let y1 = ycoord(s.eval(d, 0, &[t + et, u]), t + et, u);
        let y2 = ycoord(s.eval(d, 0, &[t, u + eu]), t, u + eu);
        let n = normal([et, y1 - y, 0.0], [0.0, y2 - y, eu]);
        s.displace(d, d[11], d[12] > 0.0, n, rt, ru, v, amount);
    }
    if d[8] > 0.0 {
        match d[9] as i32 {
            1 => v.color = (t - t0) / dt,
            2 => v.color = (u - u0) / du,
            3 => v.color = (r - r0) / dr,
            0 => s.colormap(d, d[10] > 0.0, rt, ru, ctx, v),
            4 => v.color = (t - t0) / dt * (u - u0) / du,
            _ => {}
        }
        v.color = v.color.clamp(0.0, 1.0);
    }
    v.x += amount * x;
    v.y += amount * y;
    v.z += amount * z;
}

// ---------------------------------------------------------------------------------------------
// yplot2d_wf / yplot3d_wf

const fn yplot_params(color_mode: f64, max_mode: f64) -> [ParamDef; 18] {
    concat(
        [
            PRESET,
            ParamDef::new("xmin", -3.0),
            ParamDef::new("xmax", 2.0),
            ParamDef::new("ymin", -4.0),
            ParamDef::new("ymax", 4.0),
            ParamDef::new("zmin", -2.0),
            ParamDef::new("zmax", 2.0),
            DIRECT,
            ParamDef::int("color_mode", color_mode, 0.0, max_mode),
            BLEND,
            DISPL,
            BLEND_DISPL,
        ],
        abcdef!(),
    )
}

static YPLOT2D_PARAMS: [ParamDef; 18] = yplot_params(1.0, 2.0);
static YPLOT3D_PARAMS: [ParamDef; 18] = yplot_params(3.0, 4.0);

const YPLOT_INTS: &[(usize, Option<(i32, i32)>)] = &[(0, None), (7, None), (8, None), (9, Some((0, 1))), (11, Some((0, 1)))];

static YPLOT2D: Spec = YPLOT2D_SPEC;
static YPLOT3D: Spec = Spec { vars: &["x", "z"], ..YPLOT2D_SPEC };
const YPLOT2D_SPEC: Spec = Spec {
    params: 18,
    ranges: &[(1, 2), (3, 4), (5, 6)],
    ints: YPLOT_INTS,
    param_a: 12,
    formulas: &["formula"],
    vars: &["x"],
    empty_invalid: true,
    displ: true,
};

fn yplot_apply(d: &[f64], ctx: &mut Ctx, v: &mut Pt, amount: f64, three_d: bool) {
    let s = if three_d { &YPLOT3D } else { &YPLOT2D };
    if !s.valid(d) {
        return;
    }
    let ((x0, dx), (y0, dy), (z0, dz)) = (s.range(d, 0), s.range(d, 1), s.range(d, 2));
    let (ru, rv) = (ctx.random(), ctx.random());
    let (x, z) = (x0 + ru * dx, z0 + rv * dz);
    let y = s.eval(d, 0, &[x, z]);
    if d[s.dm()] > 0.0 && d[10].abs() > EPSILON {
        let ex = dx / 100.0;
        let y1 = s.eval(d, 0, &[x + ex, z]);
        let n = if three_d {
            let ez = dz / 100.0;
            let y2 = s.eval(d, 0, &[x, z + ez]);
            normal([ex, y1 - y, 0.0], [0.0, y2 - y, ez])
        } else {
            normal([ex, y1 - y, 0.0], [0.0, 0.0, 1.0])
        };
        s.displace(d, d[10], d[11] > 0.0, n, ru, rv, v, amount);
    }
    if d[7] > 0.0 {
        let (fx, fy, fz) = ((x - x0) / dx, (y - y0) / dy, (z - z0) / dz);
        match (d[8] as i32, three_d) {
            (0, _) => s.colormap(d, d[9] > 0.0, ru, rv, ctx, v),
            (2, _) => v.color = fy,
            (_, false) => v.color = fx,
            (1, true) => v.color = fx,
            (4, true) => v.color = fx * fz,
            (_, true) => v.color = fz,
        }
        v.color = v.color.clamp(0.0, 1.0);
    }
    v.x += amount * x;
    v.y += amount * y;
    v.z += amount * z;
}

// ---------------------------------------------------------------------------------------------
// isosfplot3d_wf

static ISOSF_PARAMS: [ParamDef; 18] = concat(
    [
        PRESET,
        ParamDef::new("xmin", -M_PI),
        ParamDef::new("xmax", M_PI),
        ParamDef::new("ymin", -M_PI),
        ParamDef::new("ymax", M_PI),
        ParamDef::new("zmin", -M_PI),
        ParamDef::new("zmax", M_PI),
        ParamDef::range("thickness", 0.05, 0.0, 1.0),
        ParamDef::int("max_iter", 160.0, 1.0, 1000.0),
        DIRECT,
        ParamDef::int("color_mode", 9.0, 0.0, 9.0),
        BLEND,
    ],
    abcdef!(),
);

static ISOSF: Spec = Spec {
    params: 18,
    ranges: &[(1, 2), (3, 4), (5, 6)],
    ints: &[(0, None), (8, None), (9, None), (10, None), (11, Some((0, 1)))],
    param_a: 12,
    formulas: &["formula"],
    vars: &["x", "y", "z"],
    empty_invalid: true,
    displ: false,
};

fn isosf_prepare(p: &[f64]) -> Vec<f64> {
    let mut d = ISOSF.prepare(p);
    d[7] = p[7].max(EPSILON);
    d[8] = d[8].max(1.0);
    d
}

fn isosf_apply(d: &[f64], ctx: &mut Ctx, v: &mut Pt, amount: f64) {
    let s = &ISOSF;
    if !s.valid(d) {
        return;
    }
    let ((x0, dx), (y0, dy), (z0, dz)) = (s.range(d, 0), s.range(d, 1), s.range(d, 2));
    v.do_hide = true;
    let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
    for _ in 0..d[8] as i32 {
        x = x0 + ctx.random() * dx;
        y = y0 + ctx.random() * dy;
        z = z0 + ctx.random() * dz;
        if s.eval(d, 0, &[x, y, z]).abs() <= d[7] {
            v.x += amount * x;
            v.y += amount * y;
            v.z += amount * z;
            v.do_hide = false;
            break;
        }
    }
    if v.do_hide || d[9] <= 0.0 {
        return;
    }
    let (fx, fy, fz) = ((x - x0) / dx, (y - y0) / dy, (z - z0) / dz);
    match d[10] as i32 {
        3 => v.color = fx,
        4 => v.color = fy,
        5 => v.color = fz,
        0 => s.colormap(d, d[11] > 0.0, fy, fz, ctx, v),
        1 => s.colormap(d, d[11] > 0.0, fz, fx, ctx, v),
        2 => s.colormap(d, d[11] > 0.0, fx, fy, ctx, v),
        6 => v.color = fx * fy,
        7 => v.color = fy * fz,
        8 => v.color = fz * fx,
        _ => v.color = fx * fy * fz,
    }
    v.color = v.color.clamp(0.0, 1.0);
}

// ---------------------------------------------------------------------------------------------

/// WGSL: `F(i, vars)` evaluates formula slot `i` (see [`Spec::formula`]).
pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "parplot2d_wf",
        params: &PARPLOT_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| parplot_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| PARPLOT.prepare(p)),
        wgsl: "if (D(28) > 0.0 && D(30) > 0.0 && D(32) > 0.0) {
  var ru = a.x;
  var rv = a.y;
  if (i32(D(10)) != 0) { ru = rnd(); rv = rnd(); }
  let u = D(17) + ru * D(18);
  let w = D(19) + rv * D(20);
  let q = vec3<f32>(res_f(base, 27u, vec4<f32>(u, w, 0.0, 0.0)), res_f(base, 29u, vec4<f32>(u, w, 0.0, 0.0)), res_f(base, 31u, vec4<f32>(u, w, 0.0, 0.0)));
  if (D(24) > 0.0 && abs(D(8)) > EPSILON) {
    let u1 = u + D(18) / 100.0;
    let w2 = w + D(20) / 100.0;
    let q1 = vec3<f32>(res_f(base, 27u, vec4<f32>(u1, w, 0.0, 0.0)), res_f(base, 29u, vec4<f32>(u1, w, 0.0, 0.0)), res_f(base, 31u, vec4<f32>(u1, w, 0.0, 0.0)));
    let q2 = vec3<f32>(res_f(base, 27u, vec4<f32>(u, w2, 0.0, 0.0)), res_f(base, 29u, vec4<f32>(u, w2, 0.0, 0.0)), res_f(base, 31u, vec4<f32>(u, w2, 0.0, 0.0)));
    let n = res_normal(q1 - q, q2 - q);
    let dd = res_plot_displ(base, 24u, D(9) > 0.0, ru, rv) * D(8);
    v.x += AMOUNT * n.x * dd; v.y += AMOUNT * n.y * dd; v.z += AMOUNT * n.z * dd;
  }
  if (D(5) > 0.0) {
    let fu = (u - D(17)) / D(18);
    let fv = (w - D(19)) / D(20);
    let mode = i32(D(6));
    if (mode == 2) { v.color = fv; }
    else if (mode == 1) { v.color = fu; }
    else if (mode == 0) { v = res_plot_color(base, 21u, D(7) > 0.0, ru, rv, v); }
    else { v.color = fv * fu; }
    v.color = clamp(v.color, 0.0, 1.0);
  }
  v.x += AMOUNT * q.x; v.y += AMOUNT * q.y; v.z += AMOUNT * q.z;
}",
    },
    VarDef {
        name: "polarplot2d_wf",
        params: &POLAR2D_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| polar2d_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| POLAR2D.prepare(p)),
        wgsl: "if (D(31) > 0.0) {
  let ru = rnd();
  let rv = rnd();
  let t = D(18) + ru * D(19);
  let z = D(22) + rv * D(23);
  let r = res_f(base, 30u, vec4<f32>(t, 0.0, 0.0, 0.0));
  let x = r * cos(t);
  let y = r * sin(t);
  if (D(27) > 0.0 && abs(D(10)) > EPSILON) {
    let eps = D(19) / 100.0;
    let t1 = t + eps;
    let y1 = res_f(base, 30u, vec4<f32>(t1, 0.0, 0.0, 0.0)) * sin(t1);
    let n = res_normal(vec3<f32>(eps, y1 - y, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    let dd = res_plot_displ(base, 27u, D(11) > 0.0, ru, rv) * D(10);
    v.x += AMOUNT * n.x * dd; v.y += AMOUNT * n.y * dd; v.z += AMOUNT * n.z * dd;
  }
  if (D(7) > 0.0) {
    let mode = i32(D(8));
    if (mode == 0) { v = res_plot_color(base, 24u, D(9) > 0.0, ru, rv, v); }
    else if (mode == 2) { v.color = (r - D(20)) / D(21); }
    else { v.color = (t - D(18)) / D(19); }
    v.color = clamp(v.color, 0.0, 1.0);
  }
  v.x += AMOUNT * x; v.y += AMOUNT * y; v.z += AMOUNT * z;
}",
    },
    VarDef {
        name: "polarplot3d_wf",
        params: &POLAR3D_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| polar3d_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| POLAR3D.prepare(p)),
        wgsl: "if (D(32) > 0.0) {
  let rt = rnd();
  let ru = rnd();
  let t = D(19) + rt * D(20);
  let u = D(21) + ru * D(22);
  let r = res_f(base, 31u, vec4<f32>(t, u, 0.0, 0.0));
  let cyl = i32(D(7)) != 0;
  var p = vec3<f32>(r * sin(u) * cos(t), r * sin(u) * sin(t), r * cos(u));
  if (cyl) { p = vec3<f32>(r * cos(t), r * sin(t), u); }
  if (D(28) > 0.0 && abs(D(11)) > EPSILON) {
    let et = D(20) / 100.0;
    let eu = D(22) / 100.0;
    let r1 = res_f(base, 31u, vec4<f32>(t + et, u, 0.0, 0.0));
    let r2 = res_f(base, 31u, vec4<f32>(t, u + eu, 0.0, 0.0));
    var y1 = r1 * sin(u) * sin(t + et);
    var y2 = r2 * sin(u + eu) * sin(t);
    if (cyl) { y1 = r1 * sin(t + et); y2 = r2 * sin(t); }
    let n = res_normal(vec3<f32>(et, y1 - p.y, 0.0), vec3<f32>(0.0, y2 - p.y, eu));
    let dd = res_plot_displ(base, 28u, D(12) > 0.0, rt, ru) * D(11);
    v.x += AMOUNT * n.x * dd; v.y += AMOUNT * n.y * dd; v.z += AMOUNT * n.z * dd;
  }
  if (D(8) > 0.0) {
    let mode = i32(D(9));
    if (mode == 1) { v.color = (t - D(19)) / D(20); }
    else if (mode == 2) { v.color = (u - D(21)) / D(22); }
    else if (mode == 3) { v.color = (r - D(23)) / D(24); }
    else if (mode == 0) { v = res_plot_color(base, 25u, D(10) > 0.0, rt, ru, v); }
    else if (mode == 4) { v.color = (t - D(19)) / D(20) * (u - D(21)) / D(22); }
    v.color = clamp(v.color, 0.0, 1.0);
  }
  v.x += AMOUNT * p.x; v.y += AMOUNT * p.y; v.z += AMOUNT * p.z;
}",
    },
    VarDef {
        name: "yplot2d_wf",
        params: &YPLOT2D_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| yplot_apply(d, ctx, v, amount, false),
        prepare: Some(|p, _| YPLOT2D.prepare(p)),
        wgsl: "if (D(31) > 0.0) {
  let ru = rnd();
  let rv = rnd();
  let x = D(18) + ru * D(19);
  let z = D(22) + rv * D(23);
  let y = res_f(base, 30u, vec4<f32>(x, z, 0.0, 0.0));
  if (D(27) > 0.0 && abs(D(10)) > EPSILON) {
    let ex = D(19) / 100.0;
    let y1 = res_f(base, 30u, vec4<f32>(x + ex, z, 0.0, 0.0));
    let n = res_normal(vec3<f32>(ex, y1 - y, 0.0), vec3<f32>(0.0, 0.0, 1.0));
    let dd = res_plot_displ(base, 27u, D(11) > 0.0, ru, rv) * D(10);
    v.x += AMOUNT * n.x * dd; v.y += AMOUNT * n.y * dd; v.z += AMOUNT * n.z * dd;
  }
  if (D(7) > 0.0) {
    let mode = i32(D(8));
    if (mode == 0) { v = res_plot_color(base, 24u, D(9) > 0.0, ru, rv, v); }
    else if (mode == 2) { v.color = (y - D(20)) / D(21); }
    else { v.color = (x - D(18)) / D(19); }
    v.color = clamp(v.color, 0.0, 1.0);
  }
  v.x += AMOUNT * x; v.y += AMOUNT * y; v.z += AMOUNT * z;
}",
    },
    VarDef {
        name: "yplot3d_wf",
        params: &YPLOT3D_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| yplot_apply(d, ctx, v, amount, true),
        prepare: Some(|p, _| YPLOT3D.prepare(p)),
        wgsl: "if (D(31) > 0.0) {
  let ru = rnd();
  let rv = rnd();
  let x = D(18) + ru * D(19);
  let z = D(22) + rv * D(23);
  let y = res_f(base, 30u, vec4<f32>(x, z, 0.0, 0.0));
  if (D(27) > 0.0 && abs(D(10)) > EPSILON) {
    let ex = D(19) / 100.0;
    let ez = D(23) / 100.0;
    let y1 = res_f(base, 30u, vec4<f32>(x + ex, z, 0.0, 0.0));
    let y2 = res_f(base, 30u, vec4<f32>(x, z + ez, 0.0, 0.0));
    let n = res_normal(vec3<f32>(ex, y1 - y, 0.0), vec3<f32>(0.0, y2 - y, ez));
    let dd = res_plot_displ(base, 27u, D(11) > 0.0, ru, rv) * D(10);
    v.x += AMOUNT * n.x * dd; v.y += AMOUNT * n.y * dd; v.z += AMOUNT * n.z * dd;
  }
  if (D(7) > 0.0) {
    let mode = i32(D(8));
    if (mode == 0) { v = res_plot_color(base, 24u, D(9) > 0.0, ru, rv, v); }
    else if (mode == 1) { v.color = (x - D(18)) / D(19); }
    else if (mode == 2) { v.color = (y - D(20)) / D(21); }
    else if (mode == 4) { v.color = (x - D(18)) / D(19) * (z - D(22)) / D(23); }
    else { v.color = (z - D(22)) / D(23); }
    v.color = clamp(v.color, 0.0, 1.0);
  }
  v.x += AMOUNT * x; v.y += AMOUNT * y; v.z += AMOUNT * z;
}",
    },
    VarDef {
        name: "isosfplot3d_wf",
        params: &ISOSF_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, amount| isosf_apply(d, ctx, v, amount),
        prepare: Some(|p, _| isosf_prepare(p)),
        wgsl: "if (D(28) > 0.0) {
  var hit = false;
  var p = vec3<f32>(0.0);
  for (var i = 0; i < i32(D(8)); i++) {
    p = vec3<f32>(D(18) + rnd() * D(19), D(20) + rnd() * D(21), D(22) + rnd() * D(23));
    if (abs(res_f(base, 27u, vec4<f32>(p, 0.0))) <= D(7)) {
      v.x += AMOUNT * p.x; v.y += AMOUNT * p.y; v.z += AMOUNT * p.z;
      hit = true;
      break;
    }
  }
  if (!hit) { v.hide = 1.0; } else {
    v.hide = 0.0;
    if (D(9) > 0.0) {
      let f = vec3<f32>((p.x - D(18)) / D(19), (p.y - D(20)) / D(21), (p.z - D(22)) / D(23));
      let mode = i32(D(10));
      if (mode == 3) { v.color = f.x; }
      else if (mode == 4) { v.color = f.y; }
      else if (mode == 5) { v.color = f.z; }
      else if (mode == 0) { v = res_plot_color(base, 24u, D(11) > 0.0, f.y, f.z, v); }
      else if (mode == 1) { v = res_plot_color(base, 24u, D(11) > 0.0, f.z, f.x, v); }
      else if (mode == 2) { v = res_plot_color(base, 24u, D(11) > 0.0, f.x, f.y, v); }
      else if (mode == 6) { v.color = f.x * f.y; }
      else if (mode == 7) { v.color = f.y * f.z; }
      else if (mode == 8) { v.color = f.z * f.x; }
      else { v.color = f.x * f.y * f.z; }
      v.color = clamp(v.color, 0.0, 1.0);
    }
  }
}",
    },
];

const fn formula_res(name: &'static str) -> ResDef {
    ResDef { name, kind: ResKind::Formula, default: "" }
}
const CM: ResDef = ResDef { name: "colormap_filename", kind: ResKind::ImageFile, default: "" };
const DM: ResDef = ResDef { name: "displ_map_filename", kind: ResKind::ImageFile, default: "" };

pub static RESOURCES: &[ResourceVar] = &[
    ResourceVar {
        name: "parplot2d_wf",
        resources: &[formula_res("xformula"), formula_res("yformula"), formula_res("zformula"), CM, DM],
        load: |d, res, _: &LoadEnv| PARPLOT.load(d, res),
    },
    ResourceVar { name: "polarplot2d_wf", resources: &[formula_res("formula"), CM, DM], load: |d, res, _| POLAR2D.load(d, res) },
    ResourceVar { name: "polarplot3d_wf", resources: &[formula_res("formula"), CM, DM], load: |d, res, _| POLAR3D.load(d, res) },
    ResourceVar { name: "yplot2d_wf", resources: &[formula_res("formula"), CM, DM], load: |d, res, _| YPLOT2D.load(d, res) },
    ResourceVar { name: "yplot3d_wf", resources: &[formula_res("formula"), CM, DM], load: |d, res, _| YPLOT3D.load(d, res) },
    ResourceVar { name: "isosfplot3d_wf", resources: &[formula_res("formula"), CM], load: |d, res, _| ISOSF.load(d, res) },
];

#[cfg(test)]
mod tests {
    use super::super::{find, prepare_instance};
    use super::*;
    use crate::model::{Affine, VariationInstance};

    fn points(name: &str, preset: i32, n: usize) -> Vec<Pt> {
        let mut inst = VariationInstance::new(name, 1.0);
        assert!(super::super::presets::apply(&mut inst, preset));
        let def = find(name).unwrap();
        let d = prepare_instance(def, &inst, &Affine::IDENTITY, 1);
        let mut ctx = Ctx::new(3, false);
        (0..n)
            .map(|_| {
                let mut a = Pt::default();
                let mut v = Pt::default();
                (def.apply)(&d, &mut ctx, &mut a, &mut v, 1.0);
                v
            })
            .collect()
    }

    #[test]
    fn layouts_match_the_wgsl_offsets() {
        assert_eq!((PARPLOT.cm(), PARPLOT.dm(), PARPLOT.formula(0)), (21, 24, 27));
        assert_eq!((POLAR2D.cm(), POLAR2D.dm(), POLAR2D.formula(0)), (24, 27, 30));
        assert_eq!((POLAR3D.cm(), POLAR3D.dm(), POLAR3D.formula(0)), (25, 28, 31));
        assert_eq!((YPLOT2D.cm(), YPLOT3D.dm(), YPLOT3D.formula(0)), (24, 27, 30));
        assert_eq!((ISOSF.cm(), ISOSF.formula(0)), (24, 27));
    }

    #[test]
    fn parplot_torus_preset() {
        // preset 0: torus x = cos(u)*(4+cos(v)), y = sin(u)*(4+cos(v))
        for v in points("parplot2d_wf", 0, 200) {
            let r = (v.x * v.x + v.y * v.y).sqrt();
            assert!((3.0 - 1e-9..=5.0 + 1e-9).contains(&r), "{v:?}");
        }
    }

    #[test]
    fn isosf_ball_preset() {
        // preset 1: x*x + y*y + z*z - 1, thickness 0.05
        for v in points("isosfplot3d_wf", 1, 100).iter().filter(|v| !v.do_hide) {
            let r2 = v.x * v.x + v.y * v.y + v.z * v.z;
            assert!((r2 - 1.0).abs() <= 0.05 + 1e-9, "{v:?}");
        }
    }

    #[test]
    fn invalid_formula_does_nothing() {
        let mut inst = VariationInstance::new("yplot2d_wf", 1.0);
        inst.set_resource("formula", "sin(");
        let def = find("yplot2d_wf").unwrap();
        let d = prepare_instance(def, &inst, &Affine::IDENTITY, 1);
        let mut v = Pt::default();
        (def.apply)(&d, &mut Ctx::new(1, false), &mut Pt::default(), &mut v, 1.0);
        assert_eq!((v.x, v.y, v.z), (0.0, 0.0, 0.0));
    }

    #[test]
    fn file_values_replace_preset_defaults() {
        let hex = crate::resources::hex_encode(b"x*x");
        let xml = format!(
            r#"<flame name="t" size="10 10" center="0 0" scale="1"><xform weight="1" color="0" coefs="1 0 0 1 0 0" yplot2d_wf="1" yplot2d_wf_xmin="-1" yplot2d_wf_formula="{hex}" /><palette count="1" format="RGB">000000</palette></flame>"#
        );
        let f = &crate::io::read_flames(&xml).unwrap()[0];
        let inst = &f.layers[0].xforms[0].variations[0];
        assert_eq!(inst.resource("formula"), "x*x");
        assert_eq!(inst.param("xmin"), Some(-1.0));
        assert_eq!(inst.raw_params.iter().filter(|(k, _)| k == "formula").count(), 1);
    }
}
