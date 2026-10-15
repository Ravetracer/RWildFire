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
//! * xform:     `layer0.xf2.weight`, `layer0.xf2.coefs.c20`, `layer0.final0.post.c00`, `layer0.bg0.weight`, ...
//! * variation: `layer0.xf2.var1.amount`, `layer0.xf2.var1.p.power`

use crate::model::{Affine, Flame, XForm, XFormKind};
use crate::weighting::{FieldType, WeightingField};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Interp {
    Linear,
    /// Catmull-Rom spline through the keyframes (JWildfire's default "spline").
    #[default]
    Smooth,
    /// Step: keeps the value until the next keyframe.
    Hold,
    /// JWildfire's "SPLINE": a Catmull-Rom spline through the keys in both frame and value,
    /// sampled and evaluated like JWildfire's `Envelope` (imported motion curves).
    Spline,
    /// JWildfire's "BEZIER": quadratic Bezier segments between the key midpoints.
    Bezier,
}

impl Interp {
    pub const ALL: [Interp; 5] = [Interp::Smooth, Interp::Linear, Interp::Hold, Interp::Spline, Interp::Bezier];

    pub fn as_str(&self) -> &'static str {
        match self {
            Interp::Linear => "linear",
            Interp::Smooth => "smooth",
            Interp::Hold => "hold",
            Interp::Spline => "spline",
            Interp::Bezier => "bezier",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "linear" => Interp::Linear,
            "hold" | "step" => Interp::Hold,
            "spline" => Interp::Spline,
            "bezier" => Interp::Bezier,
            _ => Interp::Smooth,
        }
    }
    /// Tooltip for the interpolation pickers.
    pub fn help(&self) -> &'static str {
        match self {
            Interp::Smooth => "Smooth curve through the keys",
            Interp::Linear => "Straight lines between the keys",
            Interp::Hold => "Keeps each value until the next key",
            Interp::Spline => "JWildfire's spline (as in imported JWildfire motion curves)",
            Interp::Bezier => "JWildfire's Bezier curve: passes near, not through, the inner keys",
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
        if matches!(self.interp, Interp::Spline | Interp::Bezier) {
            return Some(jwf_envelope_eval(k, self.interp, frame));
        }
        let i = k.partition_point(|key| key.frame <= frame) - 1;
        let (a, b) = (k[i], k[i + 1]);
        let t = (frame - a.frame) / (b.frame - a.frame).max(1e-12);
        Some(match self.interp {
            Interp::Hold => a.value,
            Interp::Spline | Interp::Bezier => unreachable!(),
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

    /// `Envelope.smooth(3)`: every key value becomes the mean of itself and its neighbours
    /// (only when there are more than three keys).
    pub fn smooth(&mut self) {
        if self.keys.len() <= 3 {
            return;
        }
        let old: Vec<f64> = self.keys.iter().map(|k| k.value).collect();
        for (i, k) in self.keys.iter_mut().enumerate() {
            let w = &old[i.saturating_sub(1)..(i + 2).min(old.len())];
            k.value = w.iter().sum::<f64>() / w.len() as f64;
        }
    }

    /// Scales all key values around `pivot` and adds `offset`.
    pub fn scale_values(&mut self, pivot: f64, scale: f64, offset: f64) {
        for k in &mut self.keys {
            k.value = pivot + (k.value - pivot) * scale + offset;
        }
    }

    pub fn remove_key_at(&mut self, frame: f64) -> bool {
        let n = self.keys.len();
        self.keys.retain(|k| (k.frame - frame).abs() >= 1e-6);
        self.keys.len() != n
    }
}

/// `Interpolation.calcSubDivPRV`: samples per segment from the largest key distance.
fn jwf_subdiv(x: &[f64]) -> usize {
    let xdistmax = x.windows(2).map(|w| (w[1] - w[0]) as i64).max().unwrap_or(0);
    ((1.6 * xdistmax as f64 + 0.5) as i64).max(3) as usize
}

/// `SplineInterpolation` / `BezierInterpolation` / `LinearInterpolation.interpolate`.
fn jwf_interpolate(src: &[f64], interp: Interp, subdiv: usize) -> Vec<f64> {
    const SMALL: f64 = 1.0e-5;
    let n = src.len();
    let du = 1.0 / subdiv as f64;
    let mut dest = Vec::with_capacity(n * (subdiv + 5));
    match interp {
        Interp::Spline if n > 2 => {
            let eval = |u: f64, xa: f64, xb: f64, xc: f64, xd: f64| {
                let b = 0.5;
                let mut c = u * u * u * (-b * xa + (2.0 - b) * xb + (b - 2.0) * xc + b * xd);
                c += u * u * (2.0 * b * xa + (b - 3.0) * xb + (3.0 - 2.0 * b) * xc - b * xd);
                c += u * (-b * xa + b * xc);
                c + xb
            };
            for i in 0..n - 1 {
                let xa = if i == 0 { src[i] } else { src[i - 1] };
                let xd = if i == n - 2 { src[i + 1] } else { src[i + 2] };
                let mut u = 0.0;
                loop {
                    dest.push(eval(u, xa, src[i], src[i + 1], xd));
                    u += du;
                    if u >= 1.0 + SMALL {
                        break;
                    }
                }
            }
        }
        Interp::Bezier if n > 2 => {
            for i in 1..n - 1 {
                let xm1 = if i == 1 { src[i - 1] } else { 0.5 * (src[i - 1] + src[i]) };
                let xm3 = if i == n - 2 { src[i + 1] } else { 0.5 * (src[i] + src[i + 1]) };
                let xm2 = src[i];
                let mut u = 0.0;
                loop {
                    let mut c = (xm1 - xm2 - xm2 + xm3) * (u * u);
                    c += (-xm1 - xm1 + xm2 + xm2) * u;
                    dest.push(c + xm1);
                    u += du;
                    if u >= 1.0 + SMALL {
                        break;
                    }
                }
            }
        }
        _ => {
            for i in 0..n - 1 {
                let (mut x0, x1) = (src[i], src[i + 1]);
                let dx = (x1 - x0) / (subdiv as f64 - 1.0);
                for _ in 0..subdiv {
                    dest.push(x0);
                    x0 += dx;
                }
            }
        }
    }
    dest
}

/// `Tools.FTOI`: rounds half away from zero.
fn jwf_ftoi(v: f64) -> f64 {
    if v > 0.0 {
        (v + 0.5).trunc()
    } else if v < 0.0 {
        (v - 0.5).trunc()
    } else {
        0.0
    }
}

/// `Envelope.evaluate` for more than one key and `frame` inside their range: frames and values
/// are interpolated separately, then the value is read off the sampled curve at `frame`.
fn jwf_envelope_eval(keys: &[Key], interp: Interp, frame: f64) -> f64 {
    let xs: Vec<f64> = keys.iter().map(|k| k.frame).collect();
    let ys: Vec<f64> = keys.iter().map(|k| k.value).collect();
    let subdiv = jwf_subdiv(&xs);
    let vx = jwf_interpolate(&xs, interp, subdiv);
    let vy = jwf_interpolate(&ys, interp, subdiv);
    let (mut indl, mut indr) = (None, None);
    for (i, x) in vx.iter().enumerate() {
        if jwf_ftoi(*x) <= frame {
            indl = Some(i);
        } else {
            indr = Some(i);
            break;
        }
    }
    match (indl, indr) {
        (Some(l), Some(r)) => {
            let xdist = vx[r] - vx[l];
            if xdist < 0.00000001 { vy[l] } else { vy[l] + (frame - vx[l]) / xdist * (vy[r] - vy[l]) }
        }
        (Some(l), None) => vy[l],
        (None, Some(r)) => vy[r],
        (None, None) => 0.0,
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
    ($($name:literal => $($field:ident).+),* $(,)?) => {
        /// Animatable flame-level properties: (path, getter, setter). Integer fields take the
        /// rounded value (as JWildfire's motion curves do).
        pub static FLAME_PROPS: &[(&str, Getter, Setter)] = &[
            $(($name, |f: &Flame| f.$($field).+ as f64, |f: &mut Flame, v: f64| f.$($field).+ = Settable::from_f64(v))),*
        ];
    };
}

/// Conversion of an animated value into a property's type.
trait Settable {
    fn from_f64(v: f64) -> Self;
}

impl Settable for f64 {
    fn from_f64(v: f64) -> Self {
        v
    }
}

impl Settable for i32 {
    fn from_f64(v: f64) -> Self {
        (v + 0.5).floor() as i32
    }
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
    "balance_blue" => balance_blue, "dim_z_distance" => dim_z_distance, "fg_opacity" => fg_opacity,
    "cam_dof_scale" => cam_dof_scale, "cam_dof_angle" => cam_dof_angle, "cam_dof_fade" => cam_dof_fade,
    "cam_dof_param1" => cam_dof_param1, "cam_dof_param2" => cam_dof_param2, "cam_dof_param3" => cam_dof_param3,
    "cam_dof_param4" => cam_dof_param4, "cam_dof_param5" => cam_dof_param5, "cam_dof_param6" => cam_dof_param6,
    "post_symmetry_order" => post_symmetry.order, "post_symmetry_centre_x" => post_symmetry.centre_x,
    "post_symmetry_centre_y" => post_symmetry.centre_y, "post_symmetry_distance" => post_symmetry.distance,
    "post_symmetry_rotation" => post_symmetry.rotation,
}

const AFFINE_FIELDS: [&str; 6] = ["c00", "c01", "c10", "c11", "c20", "c21"];
const AFFINES: [&str; 6] = ["coefs", "post", "yz_coefs", "yz_post", "zx_coefs", "zx_post"];
const XFORM_SCALARS: [&str; 6] = ["weight", "color", "color_symmetry", "opacity", "material", "material_speed"];

/// Animatable weighting field settings of an xform (`wfield.<name>`) with the name of their
/// JWildfire motion curve.
const WFIELD_PROPS: [(&str, &str); 15] = [
    ("color_intensity", "weightingFieldColorIntensity"),
    ("var_amount_intensity", "weightingFieldVarAmountIntensity"),
    ("param1_intensity", "weightingFieldVarParam1Intensity"),
    ("param2_intensity", "weightingFieldVarParam2Intensity"),
    ("param3_intensity", "weightingFieldVarParam3Intensity"),
    ("jitter_intensity", "weightingFieldJitterIntensity"),
    ("cmap_x_size", "weightingFieldColorMapXSize"),
    ("cmap_y_size", "weightingFieldColorMapYSize"),
    ("cmap_x_centre", "weightingFieldColorMapXCentre"),
    ("cmap_y_centre", "weightingFieldColorMapYCentre"),
    ("noise_seed", "weightingFieldNoiseSeed"),
    ("noise_frequency", "weightingFieldNoiseFrequency"),
    ("fractal_octaves", "weightingFieldFractalNoiseOctaves"),
    ("fractal_gain", "weightingFieldFractalNoiseGain"),
    ("fractal_lacunarity", "weightingFieldFractalNoiseLacunarity"),
];

fn wfield_get(w: &WeightingField, name: &str) -> Option<f64> {
    Some(match name {
        "color_intensity" => w.color_intensity,
        "var_amount_intensity" => w.var_amount_intensity,
        "param1_intensity" => w.var_params[0].intensity,
        "param2_intensity" => w.var_params[1].intensity,
        "param3_intensity" => w.var_params[2].intensity,
        "jitter_intensity" => w.jitter_intensity,
        "cmap_x_size" => w.cmap_x_size,
        "cmap_y_size" => w.cmap_y_size,
        "cmap_x_centre" => w.cmap_x_centre,
        "cmap_y_centre" => w.cmap_y_centre,
        "noise_seed" => w.noise_seed as f64,
        "noise_frequency" => w.noise_frequency,
        "fractal_octaves" => w.fractal_octaves as f64,
        "fractal_gain" => w.fractal_gain,
        "fractal_lacunarity" => w.fractal_lacunarity,
        _ => return None,
    })
}

fn wfield_set(w: &mut WeightingField, name: &str, v: f64) -> bool {
    match name {
        "color_intensity" => w.color_intensity = v,
        "var_amount_intensity" => w.var_amount_intensity = v,
        "param1_intensity" => w.var_params[0].intensity = v,
        "param2_intensity" => w.var_params[1].intensity = v,
        "param3_intensity" => w.var_params[2].intensity = v,
        "jitter_intensity" => w.jitter_intensity = v,
        "cmap_x_size" => w.cmap_x_size = v,
        "cmap_y_size" => w.cmap_y_size = v,
        "cmap_x_centre" => w.cmap_x_centre = v,
        "cmap_y_centre" => w.cmap_y_centre = v,
        "noise_seed" => w.noise_seed = v.round() as i32,
        "noise_frequency" => w.noise_frequency = v,
        "fractal_octaves" => w.fractal_octaves = v.round() as i32,
        "fractal_gain" => w.fractal_gain = v,
        "fractal_lacunarity" => w.fractal_lacunarity = v,
        _ => return false,
    }
    true
}

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

/// Splits `layer0.xf2.rest` into (layer, xform list, xform index, rest).
fn xform_path(path: &str) -> Option<(usize, XFormKind, usize, &str)> {
    let mut parts = path.splitn(3, '.');
    let l = index_after(parts.next()?, "layer")?;
    let x = parts.next()?;
    let rest = parts.next()?;
    XFormKind::ALL.into_iter().find_map(|k| Some((l, k, index_after(x, k.path_prefix())?, rest)))
}

/// The xform of a path prefix like `layer0.xf2` or `layer1.final0`.
pub fn xform_at_mut<'a>(flame: &'a mut Flame, path: &str) -> Option<&'a mut XForm> {
    let (l, x) = path.split_once('.')?;
    let li = index_after(l, "layer")?;
    let (kind, xi) = XFormKind::ALL.into_iter().find_map(|k| Some((k, index_after(x, k.path_prefix())?)))?;
    flame.layers.get_mut(li)?.xforms_of_mut(kind).get_mut(xi)
}

fn get_xform_prop(xf: &XForm, rest: &str) -> Option<f64> {
    if let Some(name) = rest.strip_prefix("wfield.") {
        return wfield_get(&xf.weighting, name);
    }
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
        "material_speed" => Some(xf.material_speed),
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
    if let Some(name) = rest.strip_prefix("wfield.") {
        return wfield_set(&mut xf.weighting, name, v);
    }
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
        "material_speed" => xf.material_speed = v,
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
    let (l, kind, x, rest) = xform_path(path)?;
    let xf = flame.layers.get(l)?.xforms_of(kind).get(x)?;
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
    let Some((l, kind, x, rest)) = xform_path(path) else { return false };
    let Some(layer) = flame.layers.get_mut(l) else { return false };
    layer.xforms_of_mut(kind).get_mut(x).is_some_and(|xf| set_xform_prop(xf, rest, v))
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
        for kind in XFormKind::ALL {
            for (xi, xf) in layer.xforms_of(kind).iter().enumerate() {
                let p = format!("layer{li}.{}{xi}", kind.path_prefix());
                for s in XFORM_SCALARS {
                    out.push((format!("{p}.{s}"), get_xform_prop(xf, s).unwrap_or(0.0)));
                }
                for a in AFFINES {
                    let af = affine_ref(xf, a).unwrap();
                    for (i, f) in AFFINE_FIELDS.iter().enumerate() {
                        out.push((format!("{p}.{a}.{f}"), af.as_array()[i]));
                    }
                }
                if xf.weighting.field_type != FieldType::None {
                    for (n, _) in WFIELD_PROPS {
                        out.push((format!("{p}.wfield.{n}"), wfield_get(&xf.weighting, n).unwrap_or(0.0)));
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
    if let Some((l, kind, x, rest)) = xform_path(path) {
        let layer_prefix = if flame.layers.len() > 1 { format!("L{} ", l + 1) } else { String::new() };
        let xf_name = match kind {
            XFormKind::Normal => format!("Transform {}", x + 1),
            XFormKind::Final => format!("Final {}", x + 1),
            XFormKind::Background => format!("Background {}", x + 1),
        };
        let rest_label = if let Some(r) = rest.strip_prefix("var") {
            let (vi, tail) = r.split_once('.').unwrap_or((r, ""));
            let var_name = vi
                .parse::<usize>()
                .ok()
                .and_then(|i| flame.layers.get(l).and_then(|ly| ly.xforms_of(kind).get(x)).and_then(|xf| xf.variations.get(i)))
                .map(|v| v.name.clone())
                .unwrap_or_else(|| format!("var{vi}"));
            format!("{var_name} {}", tail.trim_start_matches("p."))
        } else if let Some(w) = rest.strip_prefix("wfield.") {
            format!("weighting {}", w.replace('_', " "))
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
        "dimZDistance" => "dim_z_distance",
        "foregroundOpacity" => "fg_opacity",
        "camDOFScale" => "cam_dof_scale",
        "camDOFAngle" => "cam_dof_angle",
        "camDOFFade" => "cam_dof_fade",
        "camDOFParam1" => "cam_dof_param1",
        "camDOFParam2" => "cam_dof_param2",
        "camDOFParam3" => "cam_dof_param3",
        "camDOFParam4" => "cam_dof_param4",
        "camDOFParam5" => "cam_dof_param5",
        "camDOFParam6" => "cam_dof_param6",
        "postSymmetryOrder" => "post_symmetry_order",
        "postSymmetryCentreX" => "post_symmetry_centre_x",
        "postSymmetryCentreY" => "post_symmetry_centre_y",
        "postSymmetryDistance" => "post_symmetry_distance",
        "postSymmetryRotation" => "post_symmetry_rotation",
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
        "materialSpeed" => Some("material_speed"),
        "opacity" => Some("opacity"),
        _ => None,
    };
    if let Some(s) = simple {
        return Some(s.to_string());
    }
    if let Some((ours, _)) = WFIELD_PROPS.iter().find(|(_, jwf)| *jwf == name) {
        return Some(format!("wfield.{ours}"));
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

/// Reads one JWildfire curve (`<prefix>point_count`, `<prefix>x0`/`<prefix>y0`, ...,
/// `<prefix>interpolation`) as keys and interpolation, whether enabled or not.
fn jwf_curve_keys(get: &dyn Fn(&str) -> Option<String>, prefix: &str) -> Option<(Vec<Key>, Interp)> {
    let n: usize = get(&format!("{prefix}point_count"))?.parse().ok()?;
    let mut keys = Vec::with_capacity(n);
    for i in 0..n {
        let x: f64 = get(&format!("{prefix}x{i}")).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let y: f64 = get(&format!("{prefix}y{i}")).and_then(|v| v.parse().ok()).unwrap_or(0.0);
        keys.push(Key { frame: x, value: y });
    }
    let interp = match get(&format!("{prefix}interpolation")).as_deref() {
        Some("LINEAR") => Interp::Linear,
        Some("BEZIER") => Interp::Bezier,
        _ => Interp::Spline,
    };
    Some((keys, interp))
}

/// The attribute prefixes of a JWildfire curve and its parent curves (`<prefix>parent_curve`
/// names a curve whose values are added).
pub fn jwf_curve_prefixes(attrs: &[(String, String)], prefix: &str) -> Vec<String> {
    let mut out = vec![prefix.to_string()];
    while out.len() < 16 {
        let key = format!("{}parent_curve", out[out.len() - 1]);
        match attrs.iter().find(|(n, _)| *n == key) {
            Some((_, v)) if !out.contains(&format!("{v}_")) => out.push(format!("{v}_")),
            _ => break,
        }
    }
    out
}

/// Builds a track from an enabled JWildfire motion curve (`<prefix>enabled`, `<prefix>point_count`,
/// `<prefix>x0`/`<prefix>y0`, ..., `<prefix>interpolation`), evaluated like JWildfire: keys with
/// its interpolation, or, with parent curves (their values are added), the sum baked into a key
/// per frame.
pub fn jwf_curve_track(attrs: &[(String, String)], prefix: &str, path: &str) -> Option<Track> {
    let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
    if get(&format!("{prefix}enabled")).as_deref() != Some("true") {
        return None;
    }
    let curves: Vec<Track> = jwf_curve_prefixes(attrs, prefix)
        .iter()
        .filter_map(|p| jwf_curve_keys(&get, p))
        .map(|(keys, interp)| Track { path: path.to_string(), interp, keys })
        .collect();
    let first = curves.first()?;
    if first.keys.is_empty() {
        return None;
    }
    if curves.len() == 1 {
        return Some(curves.into_iter().next().unwrap());
    }
    let frames = curves.iter().flat_map(|c| c.keys.iter().map(|k| k.frame));
    let (lo, hi) = frames.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), f| (a.min(f), b.max(f)));
    let keys = (lo.floor() as i64..=hi.ceil() as i64)
        .map(|f| Key { frame: f as f64, value: curves.iter().map(|c| c.eval(f as f64).unwrap_or(0.0)).sum() })
        .collect();
    Some(Track { path: path.to_string(), interp: Interp::Linear, keys })
}

/// A relative rotate/scale curve: the affine it acts on, rotate (or scale), its values.
pub type RelativeCurve = (&'static str, bool, Track);

/// JWildfire's relative rotate/scale curves of an xform (`xyRotate`, `xyPostScale`, ...): the
/// affine they act on and whether they rotate (degrees) or scale.
pub fn jwf_relative_curve(name: &str) -> Option<(&'static str, bool)> {
    let (plane, rest) = name.split_at_checked(2)?;
    let post = rest.starts_with("Post");
    let op = rest.strip_prefix("Post").unwrap_or(rest);
    let affine = match (plane, post) {
        ("xy", false) => "coefs",
        ("xy", true) => "post",
        ("yz", false) => "yz_coefs",
        ("yz", true) => "yz_post",
        ("zx", false) => "zx_coefs",
        ("zx", true) => "zx_post",
        _ => return None,
    };
    match op {
        "Rotate" => Some((affine, true)),
        "Scale" => Some((affine, false)),
        _ => None,
    }
}

/// Applies JWildfire's relative rotate/scale curves of the xform at `base` (`layer0.xf1`) by
/// baking them into keys per frame on the affine coefficients they change: at every frame the
/// (animated) coefficients are rotated or scaled by the curves' values, as JWildfire does on
/// each frame's copy of the flame. `curves` are (affine, rotate?, values) in JWildfire's order.
pub fn bake_relative_affine_curves(flame: &mut Flame, base: &str, curves: &[RelativeCurve]) {
    let Some(xf) = xform_at_mut(flame, base).map(|x| x.clone()) else { return };
    let mut affines: Vec<&str> = curves.iter().map(|(a, _, _)| *a).collect();
    affines.dedup();
    for affine in affines {
        let fields = ["c00", "c01", "c10", "c11"];
        let paths: Vec<String> = fields.iter().map(|f| format!("{base}.{affine}.{f}")).collect();
        let own: Vec<&RelativeCurve> = curves.iter().filter(|(a, _, _)| *a == affine).collect();
        let coef_tracks: Vec<Option<Track>> = paths.iter().map(|p| flame.animation.track(p).cloned()).collect();
        let frames = own.iter().map(|(_, _, t)| &t.keys).chain(coef_tracks.iter().flatten().map(|t| &t.keys)).flatten().map(|k| k.frame);
        let (lo, hi) = frames.fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), f| (a.min(f), b.max(f)));
        if lo > hi {
            continue;
        }
        let stat = affine_ref(&xf, affine).map(|a| a.as_array()).unwrap_or_default();
        let mut baked: Vec<Vec<Key>> = vec![Vec::new(); 4];
        for f in lo.floor() as i64..=hi.ceil() as i64 {
            let frame = f as f64;
            let mut c: [f64; 4] = std::array::from_fn(|i| coef_tracks[i].as_ref().and_then(|t| t.eval(frame)).unwrap_or(stat[i]));
            for (_, rotate, t) in &own {
                let v = t.eval(frame).unwrap_or(0.0);
                if *rotate {
                    // XFormTransformService.rotate: coefficients times the rotation matrix
                    if v.abs() >= 1.0e-300 {
                        let (sn, cs) = (v * std::f64::consts::PI / 180.0).sin_cos();
                        c = [c[0] * cs + c[1] * sn, -c[0] * sn + c[1] * cs, c[2] * cs + c[3] * sn, -c[2] * sn + c[3] * cs];
                    }
                } else if (v - 1.0).abs() >= 1.0e-300 {
                    c = c.map(|x| x * v);
                }
            }
            for i in 0..4 {
                baked[i].push(Key { frame, value: c[i] });
            }
        }
        for (path, keys) in paths.iter().zip(baked) {
            flame.animation.tracks.retain(|t| &t.path != path);
            flame.animation.tracks.push(Track { path: path.clone(), interp: Interp::Linear, keys });
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn weighting_field_props() {
        use super::*;
        let mut flame = Flame::default();
        flame.layers[0].xforms.push(XForm::default());
        let path = "layer0.xf0.wfield.noise_frequency";
        assert!(!all_props(&flame).iter().any(|(p, _)| p == path), "listed without a field");
        flame.layers[0].xforms[0].weighting.field_type = FieldType::PerlinNoise;
        assert!(all_props(&flame).iter().any(|(p, _)| p == path));
        assert!(set_prop(&mut flame, path, 2.5));
        assert_eq!(get_prop(&flame, path), Some(2.5));
        assert!(set_prop(&mut flame, "layer0.xf0.wfield.noise_seed", 41.6));
        assert_eq!(flame.layers[0].xforms[0].weighting.noise_seed, 42);
        assert_eq!(jwf_xform_curve_suffix("weightingFieldVarParam2Intensity").as_deref(), Some("wfield.param2_intensity"));
        assert_eq!(label(&flame, path), "Transform 1 · weighting noise frequency");
    }

    /// Compares the JWildfire interpolations with JWildfire's `Envelope.evaluate`: `JWF_CURVES`
    /// holds lines `INTERP x0 y0 x1 y1 ...`, `JWF_CURVE_VALUES` lines `<line> <frame> <value>`.
    #[test]
    #[ignore]
    fn jwf_curves_match_jwildfire() {
        let curves = std::fs::read_to_string(std::env::var("JWF_CURVES").expect("JWF_CURVES")).unwrap();
        let tracks: Vec<Track> = curves
            .lines()
            .map(|l| {
                let t: Vec<&str> = l.split_whitespace().collect();
                let interp = match t[0] {
                    "LINEAR" => Interp::Linear,
                    "BEZIER" => Interp::Bezier,
                    _ => Interp::Spline,
                };
                let keys = t[1..].chunks(2).map(|c| Key { frame: c[0].parse().unwrap(), value: c[1].parse().unwrap() }).collect();
                Track { path: String::new(), interp, keys }
            })
            .collect();
        let values = std::fs::read_to_string(std::env::var("JWF_CURVE_VALUES").expect("JWF_CURVE_VALUES")).unwrap();
        let mut worst: f64 = 0.0;
        for l in values.lines() {
            let t: Vec<f64> = l.split_whitespace().map(|v| v.parse().unwrap()).collect();
            let ours = tracks[t[0] as usize].eval(t[1]).unwrap();
            let d = (ours - t[2]).abs();
            assert!(d < 1e-9, "curve {} frame {}: {ours} vs {}", t[0], t[1], t[2]);
            worst = worst.max(d);
        }
        println!("{} values, largest difference {worst:e}", values.lines().count());
    }

    #[test]
    fn smoothing_averages_neighbours() {
        let keys = [0.0, 3.0, 0.0, 3.0, 0.0].iter().enumerate().map(|(i, &value)| Key { frame: i as f64, value }).collect();
        let mut t = Track { path: "brightness".into(), interp: Interp::Linear, keys };
        t.smooth();
        let v: Vec<f64> = t.keys.iter().map(|k| k.value).collect();
        assert_eq!(v, vec![1.5, 1.0, 2.0, 1.0, 1.5]);
        t.scale_values(1.0, 2.0, 0.5);
        assert_eq!(t.keys[1].value, 1.5);
    }

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
