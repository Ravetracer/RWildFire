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

//! Keyframe animation: tracks of keyframes on numeric flame properties.
//!
//! Properties are addressed by string paths:
//! * flame:     `centre_x`, `brightness`, ...                 (see [`FLAME_PROPS`])
//! * layer:     `layer0.weight`, `layer0.density`
//! * xform:     `layer0.xf2.weight`, `layer0.xf2.coefs.c20`, `layer0.final0.post.c00`, ...
//! * variation: `layer0.xf2.var1.amount`, `layer0.xf2.var1.p.power`

use crate::model::{Affine, Flame, XForm};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Interp {
    Linear,
    /// Catmull-Rom spline through the keyframes (JWildfire's default "spline").
    #[default]
    Smooth,
    /// Step: keeps the value until the next keyframe.
    Hold,
}

impl Interp {
    pub fn as_str(&self) -> &'static str {
        match self {
            Interp::Linear => "linear",
            Interp::Smooth => "smooth",
            Interp::Hold => "hold",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "linear" => Interp::Linear,
            "hold" | "step" => Interp::Hold,
            _ => Interp::Smooth,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub frame: f64,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub path: String,
    pub interp: Interp,
    /// Sorted by frame.
    pub keys: Vec<Key>,
}

impl Track {
    pub fn eval(&self, frame: f64) -> Option<f64> {
        let k = &self.keys;
        match k.len() {
            0 => return None,
            1 => return Some(k[0].value),
            _ => {}
        }
        if frame <= k[0].frame {
            return Some(k[0].value);
        }
        if frame >= k[k.len() - 1].frame {
            return Some(k[k.len() - 1].value);
        }
        let i = k.partition_point(|key| key.frame <= frame) - 1;
        let (a, b) = (k[i], k[i + 1]);
        let t = (frame - a.frame) / (b.frame - a.frame).max(1e-12);
        Some(match self.interp {
            Interp::Hold => a.value,
            Interp::Linear => a.value + (b.value - a.value) * t,
            Interp::Smooth => {
                // Catmull-Rom with clamped end tangents, non-uniform spacing aware.
                let p0 = if i > 0 { k[i - 1] } else { a };
                let p3 = if i + 2 < k.len() { k[i + 2] } else { b };
                let dt = b.frame - a.frame;
                let m1 = if i > 0 { (b.value - p0.value) / (b.frame - p0.frame).max(1e-12) * dt } else { b.value - a.value };
                let m2 = if i + 2 < k.len() { (p3.value - a.value) / (p3.frame - a.frame).max(1e-12) * dt } else { b.value - a.value };
                let (t2, t3) = (t * t, t * t * t);
                (2.0 * t3 - 3.0 * t2 + 1.0) * a.value + (t3 - 2.0 * t2 + t) * m1 + (-2.0 * t3 + 3.0 * t2) * b.value + (t3 - t2) * m2
            }
        })
    }

    /// Inserts or replaces the keyframe at `frame`.
    pub fn set_key(&mut self, frame: f64, value: f64) {
        match self.keys.iter_mut().find(|k| (k.frame - frame).abs() < 1e-6) {
            Some(k) => k.value = value,
            None => {
                self.keys.push(Key { frame, value });
                self.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
            }
        }
    }

    pub fn remove_key_at(&mut self, frame: f64) -> bool {
        let n = self.keys.len();
        self.keys.retain(|k| (k.frame - frame).abs() >= 1e-6);
        self.keys.len() != n
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub frame_count: u32,
    pub fps: f64,
    pub tracks: Vec<Track>,
    /// Motion blur: number of sub-frames (0 = off), their time step (frames) and decay.
    pub motion_blur_length: u32,
    pub motion_blur_time_step: f64,
    pub motion_blur_decay: f64,
}

impl Default for Animation {
    fn default() -> Self {
        Self { frame_count: 100, fps: 25.0, tracks: Vec::new(), motion_blur_length: 0, motion_blur_time_step: 0.05, motion_blur_decay: 0.03 }
    }
}

impl Animation {
    pub fn is_animated(&self) -> bool {
        self.tracks.iter().any(|t| !t.keys.is_empty())
    }

    pub fn track(&self, path: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.path == path)
    }

    pub fn track_mut(&mut self, path: &str) -> &mut Track {
        if let Some(i) = self.tracks.iter().position(|t| t.path == path) {
            return &mut self.tracks[i];
        }
        self.tracks.push(Track { path: path.to_string(), interp: Interp::Smooth, keys: Vec::new() });
        self.tracks.last_mut().unwrap()
    }

    /// All keyframe frames (deduplicated, sorted).
    pub fn key_frames(&self) -> Vec<f64> {
        let mut f: Vec<f64> = self.tracks.iter().flat_map(|t| t.keys.iter().map(|k| k.frame)).collect();
        f.sort_by(|a, b| a.total_cmp(b));
        f.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        f
    }
}

// ---------------------------------------------------------------------------------------------
// Property access

type Getter = fn(&Flame) -> f64;
type Setter = fn(&mut Flame, f64);

macro_rules! flame_props {
    ($($name:literal => $field:ident),* $(,)?) => {
        /// Animatable flame-level properties: (path, getter, setter).
        pub static FLAME_PROPS: &[(&str, Getter, Setter)] = &[
            $(($name, |f: &Flame| f.$field as f64, |f: &mut Flame, v: f64| f.$field = v as _)),*
        ];
    };
}

flame_props! {
    "centre_x" => centre_x, "centre_y" => centre_y, "pixels_per_unit" => pixels_per_unit, "cam_zoom" => cam_zoom,
    "cam_roll" => cam_roll, "cam_pitch" => cam_pitch, "cam_yaw" => cam_yaw, "cam_bank" => cam_bank,
    "cam_perspective" => cam_perspective, "cam_pos_x" => cam_pos_x, "cam_pos_y" => cam_pos_y, "cam_pos_z" => cam_pos_z,
    "cam_z" => cam_z, "cam_dof" => cam_dof, "cam_dof_area" => cam_dof_area, "cam_dof_exponent" => cam_dof_exponent,
    "focus_x" => focus_x, "focus_y" => focus_y, "focus_z" => focus_z, "dimish_z" => dimish_z,
    "brightness" => brightness, "contrast" => contrast, "gamma" => gamma, "gamma_threshold" => gamma_threshold,
    "vibrancy" => vibrancy, "white_level" => white_level, "saturation" => saturation,
    "low_density_brightness" => low_density_brightness, "balance_red" => balance_red, "balance_green" => balance_green,
    "balance_blue" => balance_blue,
}

const AFFINE_FIELDS: [&str; 6] = ["c00", "c01", "c10", "c11", "c20", "c21"];
const AFFINES: [&str; 6] = ["coefs", "post", "yz_coefs", "yz_post", "zx_coefs", "zx_post"];
const XFORM_SCALARS: [&str; 5] = ["weight", "color", "color_symmetry", "opacity", "material"];

fn affine_mut<'a>(xf: &'a mut XForm, name: &str) -> Option<&'a mut Affine> {
    Some(match name {
        "coefs" => &mut xf.coefs,
        "post" => &mut xf.post,
        "yz_coefs" => &mut xf.yz_coefs,
        "yz_post" => &mut xf.yz_post,
        "zx_coefs" => &mut xf.zx_coefs,
        "zx_post" => &mut xf.zx_post,
        _ => return None,
    })
}

fn affine_ref<'a>(xf: &'a XForm, name: &str) -> Option<&'a Affine> {
    Some(match name {
        "coefs" => &xf.coefs,
        "post" => &xf.post,
        "yz_coefs" => &xf.yz_coefs,
        "yz_post" => &xf.yz_post,
        "zx_coefs" => &xf.zx_coefs,
        "zx_post" => &xf.zx_post,
        _ => return None,
    })
}

fn affine_field(a: &Affine, f: &str) -> Option<f64> {
    let i = AFFINE_FIELDS.iter().position(|n| *n == f)?;
    Some(a.as_array()[i])
}

fn set_affine_field(a: &mut Affine, f: &str, v: f64) -> bool {
    let Some(i) = AFFINE_FIELDS.iter().position(|n| *n == f) else { return false };
    let mut arr = a.as_array();
    arr[i] = v;
    *a = Affine::from_array(arr);
    true
}

fn index_after(s: &str, prefix: &str) -> Option<usize> {
    s.strip_prefix(prefix)?.parse().ok()
}

/// Splits `layer0.xf2.rest` into (layer, is_final, xform index, rest).
fn xform_path(path: &str) -> Option<(usize, bool, usize, &str)> {
    let mut parts = path.splitn(3, '.');
    let l = index_after(parts.next()?, "layer")?;
    let x = parts.next()?;
    let rest = parts.next()?;
    if let Some(i) = index_after(x, "xf") {
        Some((l, false, i, rest))
    } else {
        Some((l, true, index_after(x, "final")?, rest))
    }
}

fn get_xform_prop(xf: &XForm, rest: &str) -> Option<f64> {
    if let Some((a, f)) = rest.split_once('.')
        && let Some(af) = affine_ref(xf, a)
    {
        return affine_field(af, f);
    }
    match rest {
        "weight" => Some(xf.weight),
        "color" => Some(xf.color),
        "color_symmetry" => Some(xf.color_symmetry),
        "opacity" => Some(xf.opacity),
        "material" => Some(xf.material),
        _ => {
            let mut parts = rest.splitn(3, '.');
            let vi = index_after(parts.next()?, "var")?;
            let var = xf.variations.get(vi)?;
            match parts.next()? {
                "amount" => Some(var.amount),
                "p" => var.param(parts.next()?),
                _ => None,
            }
        }
    }
}

fn set_xform_prop(xf: &mut XForm, rest: &str, v: f64) -> bool {
    if let Some((a, f)) = rest.split_once('.')
        && let Some(af) = affine_mut(xf, a)
    {
        return set_affine_field(af, f, v);
    }
    match rest {
        "weight" => xf.weight = v,
        "color" => xf.color = v,
        "color_symmetry" => xf.color_symmetry = v,
        "opacity" => xf.opacity = v,
        "material" => xf.material = v,
        _ => {
            let mut parts = rest.splitn(3, '.');
            let Some(vi) = parts.next().and_then(|p| index_after(p, "var")) else { return false };
            let Some(var) = xf.variations.get_mut(vi) else { return false };
            return match parts.next() {
                Some("amount") => {
                    var.amount = v;
                    true
                }
                Some("p") => parts.next().is_some_and(|p| var.set_param(p, v)),
                _ => false,
            };
        }
    }
    true
}

/// Animatable fields of a solid rendering light (`solid.light<i>.<field>`).
const LIGHT_FIELDS: [&str; 4] = ["altitude", "azimuth", "intensity", "shadow_intensity"];

/// `solid.light<i>.<field>` -> (light index, field)
fn light_path(path: &str) -> Option<(usize, &str)> {
    let (l, field) = path.strip_prefix("solid.")?.split_once('.')?;
    Some((index_after(l, "light")?, field))
}

fn light_field<'a>(light: &'a mut crate::solid::DistantLight, field: &str) -> Option<&'a mut f64> {
    match field {
        "altitude" => Some(&mut light.altitude),
        "azimuth" => Some(&mut light.azimuth),
        "intensity" => Some(&mut light.intensity),
        "shadow_intensity" => Some(&mut light.shadow_intensity),
        _ => None,
    }
}

pub fn get_prop(flame: &Flame, path: &str) -> Option<f64> {
    if let Some((_, g, _)) = FLAME_PROPS.iter().find(|(n, _, _)| *n == path) {
        return Some(g(flame));
    }
    if let Some((i, field)) = light_path(path) {
        let mut light = flame.solid.lights.get(i)?.clone();
        return light_field(&mut light, field).map(|v| *v);
    }
    if let Some((l, rest)) = path.split_once('.')
        && let Some(li) = index_after(l, "layer")
        && !rest.contains('.')
    {
        let layer = flame.layers.get(li)?;
        return match rest {
            "weight" => Some(layer.weight),
            "density" => Some(layer.density),
            _ => None,
        };
    }
    let (l, fin, x, rest) = xform_path(path)?;
    let layer = flame.layers.get(l)?;
    let xf = if fin { layer.final_xforms.get(x)? } else { layer.xforms.get(x)? };
    get_xform_prop(xf, rest)
}

pub fn set_prop(flame: &mut Flame, path: &str, v: f64) -> bool {
    if let Some((_, _, s)) = FLAME_PROPS.iter().find(|(n, _, _)| *n == path) {
        s(flame, v);
        return true;
    }
    if let Some((i, field)) = light_path(path) {
        return flame.solid.lights.get_mut(i).and_then(|l| light_field(l, field)).map(|f| *f = v).is_some();
    }
    if let Some((l, rest)) = path.split_once('.')
        && let Some(li) = index_after(l, "layer")
        && !rest.contains('.')
    {
        let Some(layer) = flame.layers.get_mut(li) else { return false };
        match rest {
            "weight" => layer.weight = v,
            "density" => layer.density = v,
            _ => return false,
        }
        return true;
    }
    let Some((l, fin, x, rest)) = xform_path(path) else { return false };
    let Some(layer) = flame.layers.get_mut(l) else { return false };
    let xf = if fin { layer.final_xforms.get_mut(x) } else { layer.xforms.get_mut(x) };
    xf.is_some_and(|xf| set_xform_prop(xf, rest, v))
}

/// All animatable properties of a flame with their current values.
pub fn all_props(flame: &Flame) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = FLAME_PROPS.iter().map(|(n, g, _)| (n.to_string(), g(flame))).collect();
    if flame.solid.enabled {
        for (i, l) in flame.solid.lights.iter().enumerate() {
            for (f, v) in LIGHT_FIELDS.iter().zip([l.altitude, l.azimuth, l.intensity, l.shadow_intensity]) {
                out.push((format!("solid.light{i}.{f}"), v));
            }
        }
    }
    for (li, layer) in flame.layers.iter().enumerate() {
        out.push((format!("layer{li}.weight"), layer.weight));
        out.push((format!("layer{li}.density"), layer.density));
        for (fin, list) in [(false, &layer.xforms), (true, &layer.final_xforms)] {
            for (xi, xf) in list.iter().enumerate() {
                let p = if fin { format!("layer{li}.final{xi}") } else { format!("layer{li}.xf{xi}") };
                for s in XFORM_SCALARS {
                    out.push((format!("{p}.{s}"), get_xform_prop(xf, s).unwrap_or(0.0)));
                }
                for a in AFFINES {
                    let af = affine_ref(xf, a).unwrap();
                    for (i, f) in AFFINE_FIELDS.iter().enumerate() {
                        out.push((format!("{p}.{a}.{f}"), af.as_array()[i]));
                    }
                }
                for (vi, var) in xf.variations.iter().enumerate() {
                    out.push((format!("{p}.var{vi}.amount"), var.amount));
                    if let Some(def) = crate::variations::find(&var.name) {
                        for (pi, pd) in def.params.iter().enumerate() {
                            out.push((format!("{p}.var{vi}.p.{}", pd.name), var.params.get(pi).copied().unwrap_or(pd.default)));
                        }
                    }
                }
            }
        }
    }
    out
}

/// Human readable label for a property path (for the timeline).
pub fn label(flame: &Flame, path: &str) -> String {
    if let Some((l, fin, x, rest)) = xform_path(path) {
        let layer_prefix = if flame.layers.len() > 1 { format!("L{} ", l + 1) } else { String::new() };
        let xf_name = if fin { format!("Final {}", x + 1) } else { format!("Transform {}", x + 1) };
        let rest_label = if let Some(r) = rest.strip_prefix("var") {
            let (vi, tail) = r.split_once('.').unwrap_or((r, ""));
            let var_name = vi
                .parse::<usize>()
                .ok()
                .and_then(|i| flame.layers.get(l).and_then(|ly| if fin { ly.final_xforms.get(x) } else { ly.xforms.get(x) }).and_then(|xf| xf.variations.get(i)))
                .map(|v| v.name.clone())
                .unwrap_or_else(|| format!("var{vi}"));
            format!("{var_name} {}", tail.trim_start_matches("p."))
        } else {
            rest.to_string()
        };
        return format!("{layer_prefix}{xf_name} · {rest_label}");
    }
    if let Some((l, field)) = path.strip_prefix("solid.").and_then(|r| r.split_once('.'))
        && let Some(i) = index_after(l, "light")
    {
        return format!("Light {} · {}", i + 1, field.replace('_', " "));
    }
    path.replace('_', " ")
}

impl Flame {
    /// Applies all animation tracks for `frame` (in place; un-animated values stay untouched).
    pub fn apply_animation(&mut self, frame: f64) {
        self.frame = frame.max(0.0).floor() as u32 + 1;
        let tracks = std::mem::take(&mut self.animation.tracks);
        for t in &tracks {
            if let Some(v) = t.eval(frame) {
                set_prop(self, &t.path, v);
            }
        }
        self.animation.tracks = tracks;
    }

    /// Copy of the flame evaluated at `frame`.
    pub fn at_frame(&self, frame: f64) -> Flame {
        let mut f = self.clone();
        f.apply_animation(frame);
        f
    }
}

/// Auto-key: records keyframes at `frame` for every property that differs between
/// `before` and `after` (same structure required). Returns the number of keys set.
pub fn auto_key(before: &Flame, after: &mut Flame, frame: f64) -> usize {
    let a = all_props(before);
    let b = all_props(after);
    if a.len() != b.len() {
        return 0;
    }
    let mut n = 0;
    for ((pa, va), (pb, vb)) in a.iter().zip(&b) {
        if pa == pb && (va - vb).abs() > 1e-12 {
            let track = after.animation.track_mut(pb);
            // First key on a new track: also keep the old value at frame 0 so the change animates.
            if track.keys.is_empty() && frame > 0.0 {
                track.set_key(0.0, *va);
            }
            track.set_key(frame, *vb);
            n += 1;
        }
    }
    n
}

/// Removes tracks whose property no longer exists (e.g. after deleting a transform).
pub fn prune_tracks(flame: &mut Flame) {
    let tracks = std::mem::take(&mut flame.animation.tracks);
    flame.animation.tracks = tracks.into_iter().filter(|t| !t.keys.is_empty() && get_prop(flame, &t.path).is_some()).collect();
}

// ---------------------------------------------------------------------------------------------
// JWildfire motion curves (import)

/// Maps a JWildfire motion-curve field name (without the `Curve` suffix) of a flame to our path.
pub fn jwf_flame_curve_path(name: &str) -> Option<&'static str> {
    Some(match name {
        "centreX" => "centre_x",
        "centreY" => "centre_y",
        "pixelsPerUnit" => "pixels_per_unit",
        "camZoom" => "cam_zoom",
        "camRoll" => "cam_roll",
        "camPitch" => "cam_pitch",
        "camYaw" => "cam_yaw",
        "camBank" => "cam_bank",
        "camPerspective" => "cam_perspective",
        "camPosX" => "cam_pos_x",
        "camPosY" => "cam_pos_y",
        "camPosZ" => "cam_pos_z",
        "camZ" => "cam_z",
        "camDOF" => "cam_dof",
        "camDOFArea" => "cam_dof_area",
        "camDOFExponent" => "cam_dof_exponent",
        "focusX" => "focus_x",
        "focusY" => "focus_y",
        "focusZ" => "focus_z",
        "dimishZ" => "dimish_z",
        "brightness" => "brightness",
        "contrast" => "contrast",
        "gamma" => "gamma",
        "gammaThreshold" => "gamma_threshold",
        "vibrancy" => "vibrancy",
        "whiteLevel" => "white_level",
        "saturation" => "saturation",
        "lowDensityBrightness" => "low_density_brightness",
        "balanceRed" => "balance_red",
        "balanceGreen" => "balance_green",
        "balanceBlue" => "balance_blue",
        _ => return None,
    })
}

/// Maps a JWildfire xform curve field name to the path suffix below `layerN.xfM.`.
pub fn jwf_xform_curve_suffix(name: &str) -> Option<String> {
    let simple = match name {
        "weight" => Some("weight"),
        "color" => Some("color"),
        "colorSymmetry" => Some("color_symmetry"),
        "material" => Some("material"),
        _ => None,
    };
    if let Some(s) = simple {
        return Some(s.to_string());
    }
    for (jwf, ours) in [("xyCoeff", "coefs"), ("xyPostCoeff", "post"), ("yzCoeff", "yz_coefs"), ("yzPostCoeff", "yz_post"), ("zxCoeff", "zx_coefs"), ("zxPostCoeff", "zx_post")] {
        if let Some(idx) = name.strip_prefix(jwf)
            && idx.len() == 2
        {
            return Some(format!("{ours}.c{idx}"));
        }
    }
    None
}

/// Builds a track from JWildfire curve attributes `<prefix>enabled`, `<prefix>point_count`,
/// `<prefix>x0`/`<prefix>y0`, ..., `<prefix>interpolation`.
pub fn jwf_curve_track(attrs: &[(String, String)], prefix: &str, path: &str) -> Option<Track> {
    let get = |k: &str| attrs.iter().find(|(n, _)| n == &format!("{prefix}{k}")).map(|(_, v)| v.as_str());
    if get("enabled") != Some("true") {
        return None;
    }
    let n: usize = get("point_count")?.parse().ok()?;
    let mut keys = Vec::with_capacity(n);
    for i in 0..n {
        let x: f64 = get(&format!("x{i}"))?.parse().ok()?;
        let y: f64 = get(&format!("y{i}"))?.parse().ok()?;
        keys.push(Key { frame: x, value: y });
    }
    keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    let interp = match get("interpolation") {
        Some("LINEAR") => Interp::Linear,
        _ => Interp::Smooth,
    };
    (!keys.is_empty()).then(|| Track { path: path.to_string(), interp, keys })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props_roundtrip() {
        let mut f = Flame::default();
        f.solid.enabled = true;
        let mut xf = XForm::with_variation("julian", 1.0);
        xf.coefs.c20 = 0.3;
        f.first_layer_mut().xforms.push(xf);
        for (p, v) in all_props(&f) {
            assert_eq!(get_prop(&f, &p), Some(v), "{p}");
            assert!(set_prop(&mut f, &p, v + 1.0), "{p}");
            assert_eq!(get_prop(&f, &p), Some(v + 1.0), "{p}");
        }
        assert_eq!(get_prop(&f, "layer0.xf0.var0.p.power"), Some(3.0));
    }

    #[test]
    fn interpolation() {
        let t = Track { path: "x".into(), interp: Interp::Linear, keys: vec![Key { frame: 0.0, value: 0.0 }, Key { frame: 10.0, value: 10.0 }] };
        assert_eq!(t.eval(5.0), Some(5.0));
        assert_eq!(t.eval(-1.0), Some(0.0));
        assert_eq!(t.eval(20.0), Some(10.0));
        let s = Track { interp: Interp::Smooth, ..t.clone() };
        assert!((s.eval(5.0).unwrap() - 5.0).abs() < 1e-9);
        let h = Track { interp: Interp::Hold, ..t };
        assert_eq!(h.eval(9.9), Some(0.0));
    }

    #[test]
    fn auto_key_records_changes() {
        let mut a = Flame::default();
        a.first_layer_mut().xforms.push(XForm::with_variation("linear3D", 1.0));
        let mut b = a.clone();
        b.brightness = 8.0;
        b.first_layer_mut().xforms[0].coefs.c20 = 0.5;
        assert_eq!(auto_key(&a, &mut b, 50.0), 2);
        assert_eq!(b.at_frame(0.0).brightness, a.brightness);
        assert_eq!(b.at_frame(50.0).brightness, 8.0);
    }
}
