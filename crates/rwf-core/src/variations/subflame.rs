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

//! `subflame_wf` (`SubFlameWFFunc`): every call advances the chaos game of a complete flame
//! (stored as XML in the variation, or read from a flame file sequence) by one visible point.
//!
//! The sub-flame is prepared like a normal flame and registered under an id that goes into the
//! prepared data; the chaos-game state lives per render thread in [`Ctx`]. Like in JWildfire
//! (where it is "not desired for GPU rendering"), there is no GPU implementation.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use super::resource_vars::text;
use super::{Ctx, LoadEnv, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::math::*;
use crate::model::ColorType;
use crate::render::prepared::{NEXT_XFORM_TABLE_SIZE, PreparedFlame};

/// A prepared sub-flame (first layer) and its `preserve_z` flag.
pub struct SubFlame {
    flame: PreparedFlame,
    preserve_z: bool,
}

/// Chaos-game state of one sub-flame in one render thread (`SubFlameWFFunc.p/q/xf`).
pub struct SubState {
    sub: Arc<SubFlame>,
    p: Pt,
    q: Pt,
    xf: Option<usize>,
    fused: bool,
}

/// Per-thread states, keyed by sub-flame id (see [`Ctx`]).
pub type SubStates = HashMap<usize, SubState>;

type Registry = RwLock<Vec<(String, Arc<SubFlame>)>>;

fn registry() -> &'static Registry {
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(Default::default)
}

/// Registers (or finds) a sub-flame; returns its id (1-based, 0 = none).
fn register(key: String, build: impl FnOnce() -> Option<SubFlame>) -> usize {
    if let Some(i) = registry().read().unwrap().iter().position(|(k, _)| *k == key) {
        return i + 1;
    }
    let Some(sub) = build() else { return 0 };
    let mut reg = registry().write().unwrap();
    reg.push((key, Arc::new(sub)));
    reg.len()
}

fn sub_flame(id: usize) -> Option<Arc<SubFlame>> {
    registry().read().unwrap().get(id.checked_sub(1)?).map(|(_, s)| s.clone())
}

/// `getCurrFlameFilename`: the file of the current frame of a flame file sequence.
fn sequence_filename(name: &str, frame: i32, digits: i32) -> String {
    let chars: Vec<char> = name.chars().collect();
    let Some(p) = chars.iter().rposition(|&c| c == '.') else { return name.to_string() };
    if p as i32 <= digits || p == chars.len() - 1 {
        return name.to_string();
    }
    let base: String = chars[..p - digits as usize].iter().collect();
    let ext: String = chars[p..].iter().collect();
    let mut number = frame.to_string();
    while (number.chars().count() as i32) < digits {
        number.insert(0, '0');
    }
    format!("{base}{number}{ext}")
}

const SEQ_FILES: i32 = 1;
const SEQ_CURVE: i32 = 2;
/// Index of the sub-flame id in the prepared data (relative to the sub-flame parameters).
const ID: usize = 12;
/// Index of the sub-flame parameters in the prepared data of `glynns3subfl`.
const GLYNN: usize = 9;

/// Loads the sub-flame; `o` = index of the sub-flame parameters (`scale`).
fn load(d: &mut [f64], res: &[Vec<u8>], env: &LoadEnv, o: usize) {
    // init: frame of the sequence
    let (is_seq, start, end, repeat, digits) = (d[o + 7] as i32, d[o + 8] as i32, d[o + 9] as i32, d[o + 10] as i32, d[o + 11] as i32);
    let seq_start = start.max(1);
    let seq_end = if end <= 0 { 0 } else { end.max(seq_start) };
    let seq_repeat = if repeat < 1 || repeat > seq_end { seq_end } else { repeat };
    let seq_period = seq_end - seq_repeat + 1;
    let mut frame = env.frame as i32 + start - 1;
    if seq_end > 0 && frame > seq_end && seq_period > 0 {
        frame = (frame - seq_repeat) % seq_period + seq_repeat;
    }
    let filename = text(&res[1]);
    let source = if is_seq == SEQ_FILES && !filename.is_empty() {
        let f = sequence_filename(&filename, frame, digits);
        if std::path::Path::new(&f).exists() { std::fs::read_to_string(&f).unwrap_or_default() } else { text(&res[0]) }
    } else {
        text(&res[0])
    };
    let key_time = if is_seq == SEQ_CURVE { frame } else { -1 };
    d[o + ID] = register(format!("{key_time}#{source}"), || {
        let mut flame = crate::io::read_flames(&source).ok()?.into_iter().next()?;
        let time = if is_seq == SEQ_CURVE { frame } else { flame.frame as i32 };
        if flame.animation.is_animated() {
            flame.apply_animation((time - 1).max(0) as f64);
        }
        let layer = flame.layers.first_mut()?;
        for xf in &mut layer.final_xforms {
            if xf.color_type == ColorType::Unset {
                xf.color_type = ColorType::Diffusion;
            }
        }
        flame.layers.truncate(1);
        flame.layers[0].visible = true;
        let preserve_z = flame.preserve_z;
        Some(SubFlame { flame: PreparedFlame::new(&flame), preserve_z })
    }) as f64;
}

/// `prefuseIter`
fn prefuse(st: &mut SubState, ctx: &mut Ctx) {
    st.fused = true;
    let Some(layer) = st.sub.flame.layers.first() else { return };
    if layer.xforms.is_empty() {
        st.xf = None;
        return;
    }
    st.xf = Some(0);
    st.p = Pt { x: ctx.random() - 0.5, y: ctx.random() - 0.5, z: 0.0, color: ctx.random(), ..Pt::default() };
    for _ in 0..42 {
        let Some(next) = st.xf.and_then(|i| layer.xforms[i].next.as_ref()) else {
            st.xf = None;
            return;
        };
        let i = next[ctx.random_int(NEXT_XFORM_TABLE_SIZE)] as usize;
        st.xf = Some(i);
        st.p = layer.xforms[i].transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
    }
}

/// `subflameIter`: advances to the next visible point `q` (scaled, rotated and moved).
fn iterate(st: &mut SubState, ctx: &mut Ctx, d: &[f64]) {
    if !st.fused || st.p.is_bad() || !st.p.color.is_finite() {
        prefuse(st, ctx);
    }
    let sub = st.sub.clone();
    let Some(layer) = sub.flame.layers.first() else { return };
    let mut iter = 0;
    while let Some(cur) = st.xf {
        iter += 1;
        if iter > 1000 {
            return;
        }
        let Some(next) = layer.xforms[cur].next.as_ref() else {
            st.xf = None;
            return;
        };
        let i = next[ctx.random_int(NEXT_XFORM_TABLE_SIZE)] as usize;
        st.xf = Some(i);
        let xf = &layer.xforms[i];
        st.p = xf.transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
        if !xf.is_visible_sample(ctx) {
            continue;
        }
        st.q = st.p;
        for f in &layer.finals {
            st.q = f.transform(ctx, &layer.colormap, layer.smooth_gradient, &st.q);
        }
        break;
    }
    // d = [scale, angle (degrees), offset_x, offset_y, offset_z, colorscale_z, ..]
    let (sina, cosa) = (d[1] * M_PI / 180.0).sin_cos();
    let (x, y) = (d[0] * st.q.x, d[0] * st.q.y);
    st.q.x = x * cosa - y * sina + d[2];
    st.q.y = x * sina + y * cosa + d[3];
    st.q.z = d[0] * st.q.z + d[4] + d[5] * st.q.color;
}

/// `setColor`
fn set_color(q: &Pt, mode: i32, v: &mut Pt) {
    if q.do_hide {
        v.do_hide = true;
        return;
    }
    v.do_hide = false;
    if mode == -1 {
        return;
    }
    if q.rgb_color || mode == -2 {
        v.rgb_color = true;
        v.red = q.red;
        v.green = q.green;
        v.blue = q.blue;
        v.color = q.color;
    }
    match mode {
        0 => v.color = q.color,
        1 => v.color = q.red / 255.0,
        2 => v.color = q.green / 255.0,
        3 => v.color = q.blue / 255.0,
        4 => v.color = (0.2990 * q.red + 0.5880 * q.green + 0.1130 * q.blue) / 255.0,
        _ => {}
    }
}

/// Next point of the sub-flame; `d` starts at the sub-flame parameters.
fn next_point(d: &[f64], ctx: &mut Ctx) -> Option<Pt> {
    let id = d[ID] as usize;
    let mut st = match ctx.subflames.remove(&id) {
        Some(st) => st,
        None => SubState { sub: sub_flame(id)?, p: Pt::default(), q: Pt::default(), xf: None, fused: false },
    };
    // the sub-flame's variations see its preserve_z flag (`sfContext`)
    let preserve_z = std::mem::replace(&mut ctx.preserve_z, st.sub.preserve_z);
    iterate(&mut st, ctx, d);
    ctx.preserve_z = preserve_z;
    let q = st.q;
    ctx.subflames.insert(id, st);
    Some(q)
}

fn apply(d: &[f64], ctx: &mut Ctx, v: &mut Pt) {
    let Some(q) = next_point(d, ctx) else { return };
    v.x += q.x;
    v.y += q.y;
    v.z += q.z;
    set_color(&q, d[6] as i32, v);
}

/// `GlynnS3SubflWFFunc.transform`; d = [weight, show, radius, thickness, contrast, pow, type,
/// phi1, phi2, <sub-flame parameters>, id, radius1, radius2, gamma, abs_pow, phi10, delta].
fn glynns3subfl_apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let g = GLYNN + ID + 1;
    let (radius1, radius2, gamma, abs_pow, phi10, delta) = (d[g], d[g + 1], d[g + 2], d[g + 3], d[g + 4], d[g + 5]);
    let (show, radius, thickness, contrast, kind) = (d[1] as i32, d[2], d[3], d[4], d[6] as i32);
    let r = (a.x * a.x + a.y * a.y).sqrt();
    let alpha = radius / r;
    let inside = if kind != 0 { r < radius } else { r < radius1 };
    let (nx, ny) = if inside {
        if show == 1 {
            let (cr, phi) = if kind != 0 {
                // circle
                let cr = radius + thickness - gamma * ctx.random();
                (cr, phi10 + delta * ctx.random())
            } else {
                // circle2
                let phi = 2.0 * M_PI * ctx.random();
                (if ctx.random() < gamma { radius1 } else { radius2 }, phi)
            };
            (amount * cr * phi.cos(), amount * cr * phi.sin())
        } else {
            (amount * a.x, amount * a.y)
        }
    } else if ctx.random() > contrast * alpha.powf(abs_pow) {
        (amount * a.x, amount * a.y)
    } else {
        (amount * alpha * alpha * a.x, amount * alpha * alpha * a.y)
    };
    if ctx.random() < d[0] {
        v.x = nx;
        v.y = ny;
        ctx.pass_z(a, v, amount);
    } else if let Some(q) = next_point(&d[GLYNN..], ctx) {
        v.x += q.x * amount;
        v.y += q.y * amount;
        v.z += q.z * amount;
        set_color(&q, d[GLYNN + 6] as i32, v);
    }
}

/// `SubFlameWFFunc` parameters with the default `color_mode`.
const fn sub_params(color_mode: f64) -> [ParamDef; 12] {
    [
        ParamDef::new("scale", 1.0),
        ParamDef::range("angle", 0.0, -180.0, 180.0),
        ParamDef::new("offset_x", 0.0),
        ParamDef::new("offset_y", 0.0),
        ParamDef::new("offset_z", 0.0),
        ParamDef::new("colorscale_z", 0.0),
        ParamDef::int("color_mode", color_mode, -2.0, 4.0),
        ParamDef::int("flame_is_sequence", 0.0, 0.0, 2.0),
        ParamDef::int("flame_sequence_start", 1.0, 0.0, 9999.0),
        ParamDef::int("flame_sequence_end", 0.0, 0.0, 9999.0),
        ParamDef::int("flame_sequence_repeat", 1.0, 0.0, 9999.0),
        ParamDef::int("flame_sequence_digits", 4.0, 1.0, 8.0),
    ]
}

static SUBFLAME_PARAMS: [ParamDef; 12] = sub_params(-1.0);

static GLYNNS3SUBFL_PARAMS: [ParamDef; GLYNN + 12] = {
    let glynn = [
        ParamDef::range("weight", 0.5, 0.0, 1.0),
        ParamDef::int("show/hide(1/0)", 1.0, 0.0, 1.0),
        ParamDef::new("radius", 1.0),
        ParamDef::range("thickness", 0.42, 0.0, 1.0),
        ParamDef::range("contrast", 1.0, 0.0, 1.0),
        ParamDef::new("pow", 1.5),
        ParamDef::int("type", 0.0, 0.0, 1.0),
        ParamDef::new("phi1", 0.0),
        ParamDef::new("phi2", 360.0),
    ];
    // CM_DIRECT
    let sub = sub_params(0.0);
    let mut p = [glynn[0]; GLYNN + 12];
    let mut i = 0;
    while i < GLYNN + 12 {
        p[i] = if i < GLYNN { glynn[i] } else { sub[i - GLYNN] };
        i += 1;
    }
    p
};

/// Limits the sub-flame parameters at `d[o..]` and reserves the sub-flame id.
fn prepare_sub(d: &mut Vec<f64>, o: usize) {
    d[o + 6] = ftoi(d[o + 6]).clamp(-2, 4) as f64;
    d[o + 7] = ftoi(d[o + 7]).clamp(0, 2) as f64;
    for x in &mut d[o + 8..o + 12] {
        *x = ftoi(*x) as f64;
    }
    d.push(0.0);
}

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "subflame_wf",
        params: &SUBFLAME_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _, v, _| apply(d, ctx, v),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            prepare_sub(&mut d, 0);
            d
        }),
        wgsl: "",
    },
    VarDef {
        name: "glynns3subfl",
        params: &GLYNNS3SUBFL_PARAMS,
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| glynns3subfl_apply(d, ctx, a, v, amount),
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[0] = p[0].clamp(0.0, 1.0);
            d[1] = p[1].clamp(0.0, 1.0).trunc();
            d[3] = p[3].clamp(0.0, 1.0);
            d[4] = p[4].clamp(0.0, 1.0);
            d[6] = p[6].trunc();
            prepare_sub(&mut d, GLYNN);
            let (radius, thickness) = (d[2], d[3]);
            if d[6] as i32 == 0 {
                let radius1 = radius + thickness;
                let radius2 = radius * radius / radius1;
                d.extend([radius1, radius2, radius1 / (radius1 + radius2), d[5].abs(), 0.0, 0.0]);
            } else {
                let (phi10, phi20) = (M_PI * d[7] / 180.0, M_PI * d[8] / 180.0);
                let gamma = thickness * (2.0 * radius + thickness) / (radius + thickness);
                d.extend([0.0, 0.0, gamma, d[5].abs(), phi10, phi20 - phi10]);
            }
            d
        }),
        wgsl: "",
    },
];

pub static RESOURCES: &[ResourceVar] = &[ResourceVar {
    name: "subflame_wf",
    resources: &[
        ResDef { name: "flame", kind: ResKind::FlameXml, default: include_str!("../../assets/subflame_wf_default.flame") },
        ResDef { name: "flame_filename", kind: ResKind::FlameFile, default: "" },
    ],
    load: |d, res, env| load(d, res, env, 0),
}, ResourceVar {
    name: "glynns3subfl",
    resources: &[
        ResDef { name: "flame", kind: ResKind::FlameXml, default: include_str!("../../assets/subflame_wf_default.flame") },
        ResDef { name: "flame_filename", kind: ResKind::FlameFile, default: "" },
    ],
    load: |d, res, env| load(d, res, env, GLYNN),
}];

#[cfg(test)]
mod tests {
    use super::super::{find, prepare_instance};
    use super::*;
    use crate::model::{Affine, VariationInstance};

    #[test]
    fn default_subflame_points() {
        let inst = VariationInstance::new("subflame_wf", 1.0);
        let def = find("subflame_wf").unwrap();
        let d = prepare_instance(def, &inst, &Affine::IDENTITY, 1);
        assert!(d[ID] > 0.0, "default sub-flame did not load");
        let mut ctx = Ctx::new(5, false);
        let mut n_finite = 0;
        let mut spread = 0.0f64;
        for _ in 0..2000 {
            let mut v = Pt::default();
            (def.apply)(&d, &mut ctx, &mut Pt::default(), &mut v, 1.0);
            if !v.is_bad() {
                n_finite += 1;
                spread = spread.max(v.x.abs()).max(v.y.abs());
            }
        }
        assert!(n_finite > 1900 && spread > 0.1, "{n_finite} {spread}");
    }

    #[test]
    fn glynns3subfl_draws_from_the_subflame() {
        let mut inst = VariationInstance::new("glynns3subfl", 1.0);
        assert!(inst.set_param("weight", 0.0));
        let def = find("glynns3subfl").unwrap();
        let d = prepare_instance(def, &inst, &Affine::IDENTITY, 1);
        assert!(d[GLYNN + ID] > 0.0, "default sub-flame did not load");
        let mut ctx = Ctx::new(5, false);
        let mut spread = 0.0f64;
        for _ in 0..500 {
            let mut v = Pt::default();
            (def.apply)(&d, &mut ctx, &mut Pt { x: 0.3, y: 0.2, ..Pt::default() }, &mut v, 1.0);
            assert!(!v.is_bad());
            spread = spread.max(v.x.abs()).max(v.y.abs());
        }
        assert!(spread > 0.1, "{spread}");
    }

    #[test]
    fn sequence_names() {
        assert_eq!(sequence_filename("/f/a0001.flame", 12, 4), "/f/a0012.flame");
    }
}
