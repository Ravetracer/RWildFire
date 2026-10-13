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

//! A flame "compiled" for rendering: resolved variation functions, prepared parameter data,
//! xaos selection tables and color maps. Mirrors `XForm.initTransform` and
//! `Layer.refreshModWeightTables`.

use crate::math::{EPSILON, lerp};
use crate::model::{Affine, ColorType, DrawMode, Flame, Layer, XForm};
use crate::variations::{self, Ctx, Pt, VarDef, VarKind};

pub const NEXT_XFORM_TABLE_SIZE: usize = 1024;

/// How a variation is executed (`createTransformations`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepMode {
    /// Pre function in the pre slot: modifies the affine point in place.
    Pre,
    /// Normal function forced into the pre slot: `f(copy(a), a)`.
    EnforcedPre,
    /// Normal function in the normal slot: `f(a, v)`.
    Normal,
    /// Pre/post function forced into the normal slot: `f(copy(v), v)`.
    Enforced,
    /// Post function in the post slot: modifies the accumulated point in place.
    Post,
    /// Normal function forced into the post slot: `f(copy(v), v)`.
    EnforcedPost,
}

#[derive(Clone)]
pub struct VarStep {
    pub def: &'static VarDef,
    pub mode: StepMode,
    pub amount: f64,
    pub data: Vec<f64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AffineMode {
    None,
    Flat,
    Full,
}

#[derive(Clone)]
pub struct PreparedXForm {
    pub weight: f64,
    pub color_type: ColorType,
    pub color: f64,
    pub color_symmetry: f64,
    pub c1: f64,
    pub c2: f64,
    pub dist_mult: f64,
    pub target_color: [f64; 3],
    pub mod_gamma: (f64, f64),
    pub mod_contrast: (f64, f64),
    pub mod_saturation: (f64, f64),
    pub mod_hue: (f64, f64),
    /// `material1` / `material2`: material index blending like the color.
    pub material: (f64, f64),
    pub draw_mode: DrawMode,
    pub opacity: f64,
    pub affine_mode: AffineMode,
    pub post_mode: AffineMode,
    pub coefs: Affine,
    pub post: Affine,
    pub yz: Affine,
    pub yz_post: Affine,
    pub zx: Affine,
    pub zx_post: Affine,
    pub pre: Vec<VarStep>,
    pub normal: Vec<VarStep>,
    pub post_vars: Vec<VarStep>,
    /// Indices of the next xform, `None` if all weights are zero.
    pub next: Option<Box<[u16; NEXT_XFORM_TABLE_SIZE]>>,
}

#[derive(Clone)]
pub struct PreparedLayer {
    pub weight: f64,
    pub density: f64,
    pub xforms: Vec<PreparedXForm>,
    pub finals: Vec<PreparedXForm>,
    pub colormap: Vec<[f64; 3]>,
    /// Raw gradient colors (0..255), for variations that read the palette.
    pub palette: Vec<[u8; 3]>,
    pub smooth_gradient: bool,
}

#[derive(Clone)]
pub struct PreparedFlame {
    pub layers: Vec<PreparedLayer>,
    pub preserve_z: bool,
    /// Variation names found in the flame without an implementation in this port.
    pub missing_variations: Vec<String>,
}

fn affine_mode(xy: &Affine, yz: &Affine, zx: &Affine) -> AffineMode {
    let has_xy = !xy.is_identity();
    let has_yz = !yz.is_identity();
    let has_zx = !zx.is_identity();
    if !has_xy && !has_yz && !has_zx {
        AffineMode::None
    } else if has_xy && !has_yz && !has_zx {
        AffineMode::Flat
    } else {
        AffineMode::Full
    }
}

fn colormap_index(colormap: &[[f64; 3]], color: f64) -> usize {
    let scl = colormap.len() as f64 - 2.0;
    let idx = (color * scl + 0.5) as i64;
    idx.clamp(0, colormap.len() as i64 - 1) as usize
}

fn prepare_xform(xf: &XForm, color_type: ColorType, colormap: &[[f64; 3]], missing: &mut Vec<String>) -> PreparedXForm {
    let mut c1 = (1.0 + xf.color_symmetry) * 0.5;
    let mut c2 = xf.color * (1.0 - xf.color_symmetry) * 0.5;
    if color_type != ColorType::Diffusion && color_type != ColorType::TargetG {
        c1 = 1.0;
        c2 = 0.0;
    }
    let target_color = match color_type {
        ColorType::Target => {
            let s = 200.0 / 256.0;
            [xf.target_color[0] as f64 * s, xf.target_color[1] as f64 * s, xf.target_color[2] as f64 * s]
        }
        ColorType::TargetG => colormap[colormap_index(colormap, xf.color)],
        _ => [0.0; 3],
    };
    let speed = |v: f64, s: f64| ((1.0 + s) * 0.5, v * (1.0 - s) * 0.5);

    let mut pre = Vec::new();
    let mut normal = Vec::new();
    let mut post_vars = Vec::new();
    for var in &xf.variations {
        let Some(def) = variations::find(&var.name) else {
            if !missing.contains(&var.name) {
                missing.push(var.name.clone());
            }
            continue;
        };
        let data = def.prepare_data(&var.params, &xf.coefs);
        let step = |def, mode| VarStep { def, mode, amount: var.amount, data: data.clone() };
        // Slot selection of `XForm.createTransformations`: priority 2 and -2 put a variation into
        // both the pre and the post slot.
        let p = var.priority;
        if p < 0 || p == 2 {
            pre.push(match def.kind {
                VarKind::PrePost(inverse) => step(inverse, StepMode::Pre),
                VarKind::Pre => step(def, StepMode::Pre),
                VarKind::Normal | VarKind::Post => step(def, StepMode::EnforcedPre),
            });
        }
        if p == 0 {
            normal.push(step(def, if def.kind == VarKind::Normal { StepMode::Normal } else { StepMode::Enforced }));
        }
        if p > 0 || p == -2 {
            post_vars.push(match def.kind {
                VarKind::Post | VarKind::PrePost(_) => step(def, StepMode::Post),
                VarKind::Normal | VarKind::Pre => step(def, StepMode::EnforcedPost),
            });
        }
    }

    PreparedXForm {
        weight: xf.weight,
        color_type,
        color: xf.color,
        color_symmetry: xf.color_symmetry,
        c1,
        c2,
        dist_mult: xf.color_symmetry + 1.0,
        target_color,
        mod_gamma: speed(xf.mod_gamma, xf.mod_gamma_speed),
        mod_contrast: speed(xf.mod_contrast, xf.mod_contrast_speed),
        mod_saturation: speed(xf.mod_saturation, xf.mod_saturation_speed),
        mod_hue: speed(xf.mod_hue, xf.mod_hue_speed),
        material: speed(xf.material, xf.material_speed),
        draw_mode: xf.draw_mode,
        opacity: xf.opacity,
        affine_mode: affine_mode(&xf.coefs, &xf.yz_coefs, &xf.zx_coefs),
        post_mode: affine_mode(&xf.post, &xf.yz_post, &xf.zx_post),
        coefs: xf.coefs,
        post: xf.post,
        yz: xf.yz_coefs,
        yz_post: xf.yz_post,
        zx: xf.zx_coefs,
        zx_post: xf.zx_post,
        pre,
        normal,
        post_vars,
        next: None,
    }
}

fn prepare_layer(layer: &Layer, missing: &mut Vec<String>) -> PreparedLayer {
    let colormap = layer.gradient.render_colors();
    let mut xforms: Vec<PreparedXForm> = layer
        .xforms
        .iter()
        .map(|xf| {
            let ct = if xf.color_type == ColorType::Unset { ColorType::Diffusion } else { xf.color_type };
            prepare_xform(xf, ct, &colormap, missing)
        })
        .collect();
    let finals = layer
        .final_xforms
        .iter()
        .map(|xf| {
            let ct = if xf.color_type == ColorType::Unset { ColorType::None } else { xf.color_type };
            prepare_xform(xf, ct, &colormap, missing)
        })
        .collect();

    // Xaos tables (Layer.refreshModWeightTables).
    let n = layer.xforms.len();
    for k in 0..n {
        let tp: Vec<f64> = (0..n).map(|i| layer.xforms[i].weight * layer.xforms[k].chaos_to(i)).collect();
        let tot: f64 = tp.iter().sum();
        if tot > 0.0 {
            let mut table = Box::new([0u16; NEXT_XFORM_TABLE_SIZE]);
            let mut loop_value = 0.0;
            for slot in table.iter_mut() {
                let mut total_prob = 0.0;
                let mut j: usize = 0;
                loop {
                    total_prob += tp[j];
                    if total_prob > loop_value || j == n - 1 {
                        break;
                    }
                    j += 1;
                }
                *slot = j as u16;
                loop_value += tot / NEXT_XFORM_TABLE_SIZE as f64;
            }
            xforms[k].next = Some(table);
        }
    }

    PreparedLayer {
        weight: layer.weight,
        density: layer.density,
        xforms,
        finals,
        colormap,
        palette: layer.gradient.colors.clone(),
        smooth_gradient: layer.smooth_gradient,
    }
}

impl PreparedFlame {
    pub fn new(flame: &Flame) -> Self {
        let mut missing = Vec::new();
        let layers = flame
            .layers
            .iter()
            .filter(|l| l.visible && !l.xforms.is_empty())
            .map(|l| prepare_layer(l, &mut missing))
            .collect();
        Self { layers, preserve_z: flame.preserve_z, missing_variations: missing }
    }
}

#[inline]
fn apply_affine(mode: AffineMode, xy: &Affine, yz: &Affine, zx: &Affine, sx: f64, sy: f64, sz: f64) -> (f64, f64, f64) {
    match mode {
        AffineMode::None => (sx, sy, sz),
        AffineMode::Flat => (xy.c00 * sx + xy.c10 * sy + xy.c20, xy.c01 * sx + xy.c11 * sy + xy.c21, sz),
        AffineMode::Full => {
            let (mut x, mut y, mut z) = (xy.c00 * sx + xy.c10 * sy, xy.c01 * sx + xy.c11 * sy, sz);
            let ny = yz.c00 * y + yz.c10 * z;
            let nz = yz.c01 * y + yz.c11 * z;
            y = ny;
            z = nz;
            let nx = zx.c00 * x + zx.c10 * z;
            let nz = zx.c01 * x + zx.c11 * z;
            x = nx;
            z = nz;
            (x + xy.c20 + zx.c20, y + xy.c21 + yz.c20, z + yz.c21 + zx.c21)
        }
    }
}

#[inline]
fn run_step(step: &VarStep, ctx: &mut Ctx, a: &mut Pt, v: &mut Pt) {
    let f = step.def.apply;
    match step.mode {
        StepMode::Pre | StepMode::Normal | StepMode::Post => f(&step.data, ctx, a, v, step.amount),
        StepMode::EnforcedPre => {
            let mut tmp = *a;
            f(&step.data, ctx, &mut tmp, a, step.amount);
        }
        StepMode::Enforced | StepMode::EnforcedPost => {
            let mut tmp = *v;
            f(&step.data, ctx, &mut tmp, v, step.amount);
        }
    }
}

impl PreparedXForm {
    /// Full transform chain of `XForm.transformPoint`: `src` -> returned destination point.
    #[inline]
    pub fn transform(&self, ctx: &mut Ctx, colormap: &[[f64; 3]], smooth: bool, src: &Pt) -> Pt {
        // Init step
        let mut a = Pt { do_hide: src.do_hide, ..Pt::default() };
        if self.color_type != ColorType::Cyclic {
            a.color = src.color * self.c1 + self.c2;
        } else {
            let mut c = (src.color + self.color_symmetry) % 1.0;
            if c < 0.0 {
                c += 1.0;
            }
            a.color = c;
        }
        a.red = src.red;
        a.green = src.green;
        a.blue = src.blue;
        a.mod_gamma = src.mod_gamma * self.mod_gamma.0 + self.mod_gamma.1;
        a.mod_contrast = src.mod_contrast * self.mod_contrast.0 + self.mod_contrast.1;
        a.mod_saturation = src.mod_saturation * self.mod_saturation.0 + self.mod_saturation.1;
        a.mod_hue = src.mod_hue * self.mod_hue.0 + self.mod_hue.1;
        a.material = src.material * self.material.0 + self.material.1;

        // Affine step
        let (x, y, z) = apply_affine(self.affine_mode, &self.coefs, &self.yz, &self.zx, src.x, src.y, src.z);
        a.x = x;
        a.y = y;
        a.z = z;

        // Prepare pre-variations: v starts at zero and carries the attributes of a.
        let mut v = Pt { x: 0.0, y: 0.0, z: 0.0, ..a };
        for step in &self.pre {
            run_step(step, ctx, &mut a, &mut v);
        }
        // Prepare variations: copy attributes (not coordinates) again.
        v = Pt { x: v.x, y: v.y, z: v.z, ..a };
        for step in &self.normal {
            run_step(step, ctx, &mut a, &mut v);
        }
        for step in &self.post_vars {
            run_step(step, ctx, &mut a, &mut v);
        }

        // Post affine
        let (x, y, z) = apply_affine(self.post_mode, &self.post, &self.yz_post, &self.zx_post, v.x, v.y, v.z);
        let mut dst = Pt { x, y, z, ..v };

        // Color step
        if !dst.rgb_color {
            match self.color_type {
                ColorType::Diffusion | ColorType::Cyclic => {
                    let c = if smooth { smooth_color(colormap, dst.color) } else { colormap[colormap_index(colormap, dst.color)] };
                    dst.red = c[0];
                    dst.green = c[1];
                    dst.blue = c[2];
                }
                ColorType::Target | ColorType::TargetG => {
                    let t = (self.color_symmetry + 1.0) / 2.0;
                    dst.red = lerp(self.target_color[0], dst.red, t);
                    dst.green = lerp(self.target_color[1], dst.green, t);
                    dst.blue = lerp(self.target_color[2], dst.blue, t);
                }
                ColorType::Distance => {
                    let dist = ((src.x - dst.x).powi(2) + (src.y - dst.y).powi(2) + (src.z - dst.z).powi(2)).sqrt();
                    let scl = colormap.len() as f64 - 2.0;
                    let idx = (((self.color + dist * self.dist_mult) * scl + 0.5) as i64).rem_euclid(colormap.len() as i64) as usize;
                    let c = colormap[idx];
                    dst.red = c[0];
                    dst.green = c[1];
                    dst.blue = c[2];
                }
                ColorType::None | ColorType::Unset => {}
            }
        }
        dst
    }

    #[inline]
    pub fn is_visible_sample(&self, ctx: &mut Ctx) -> bool {
        match self.draw_mode {
            DrawMode::Hidden => false,
            DrawMode::Opaque => ctx.random() <= self.opacity,
            DrawMode::Normal => true,
        }
    }
}

fn smooth_color(colormap: &[[f64; 3]], color: f64) -> [f64; 3] {
    let scl = colormap.len() as f64 - 2.0;
    let idx = color * scl;
    let l = idx as i64;
    let get = |i: i64| if i >= 0 && (i as usize) < colormap.len() { colormap[i as usize] } else { [0.0; 3] };
    let lc = get(l);
    let rc = get(l + 1);
    let t = idx - idx.trunc();
    [lerp(lc[0], rc[0], t), lerp(lc[1], rc[1], t), lerp(lc[2], rc[2], t)]
}

/// Simple sanity check used by the UI: a flame is renderable if any layer has xforms.
pub fn is_renderable(flame: &Flame) -> bool {
    flame.layers.iter().any(|l| l.visible && l.xforms.iter().any(|x| x.weight > EPSILON))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::VariationInstance;

    fn slots(var: &str, priority: i32) -> (Vec<(String, StepMode)>, Vec<(String, StepMode)>, Vec<(String, StepMode)>) {
        let mut xf = XForm::default();
        let mut inst = VariationInstance::new(var, 1.0);
        inst.priority = priority;
        xf.variations.push(inst);
        let p = prepare_xform(&xf, ColorType::Diffusion, &[[0.0; 3]; 256], &mut Vec::new());
        let names = |l: &[VarStep]| l.iter().map(|s| (s.def.name.to_string(), s.mode)).collect();
        (names(&p.pre), names(&p.normal), names(&p.post_vars))
    }

    #[test]
    fn prepost_runs_inverse_in_pre_and_forward_in_post() {
        let (pre, normal, post) = slots("prepost_mobius", 2);
        assert_eq!(pre, vec![("prepost_mobius_inv".to_string(), StepMode::Pre)]);
        assert!(normal.is_empty());
        assert_eq!(post, vec![("prepost_mobius".to_string(), StepMode::Post)]);
    }

    #[test]
    fn priority_slots_match_create_transformations() {
        // Java: priority 2 / -2 put a normal variation into both the pre and the post slot.
        let (pre, normal, post) = slots("linear", 2);
        assert_eq!((pre[0].1, normal.len(), post[0].1), (StepMode::EnforcedPre, 0, StepMode::EnforcedPost));
        let (pre, _, post) = slots("prepost_mobius", -1);
        assert_eq!((pre[0].0.as_str(), post.len()), ("prepost_mobius_inv", 0));
        let (pre, normal, post) = slots("prepost_mobius", 0);
        assert_eq!((pre.len(), normal[0].1, post.len()), (0, StepMode::Enforced, 0));
    }
}
