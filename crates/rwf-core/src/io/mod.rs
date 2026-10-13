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

//! Reading and writing JWildfire / flam3 `.flame` files.

pub mod xml;


use std::collections::HashSet;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::OnceLock;

use crate::gradient::Gradient;
use crate::model::*;
use crate::render::denoise::Denoiser;
use crate::render::filter::FilterKernel;
use crate::variations;
use xml::{Element, escape};

#[derive(Debug, thiserror::Error)]
pub enum FlameIoError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML error: {0}")]
    Xml(#[from] xml::XmlError),
    #[error("no flame found in document")]
    NoFlame,
}

/// All variation names known to JWildfire (including legacy aliases), used to recognize
/// variation attributes even for variations this port does not implement yet.
fn jwf_variation_names() -> &'static HashSet<&'static str> {
    static NAMES: OnceLock<HashSet<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| include_str!("jwf_variation_names.txt").lines().map(str::trim).filter(|s| !s.is_empty()).collect())
}

fn is_variation_name(name: &str) -> bool {
    variations::find(name).is_some() || jwf_variation_names().contains(name)
}

/// `"julia#1#"` -> `"julia"`, `"julia#1#_power"` -> `"julia_power"` (JWildfire `removeIndexFromAttr`).
fn remove_index(name: &str) -> String {
    let Some(s) = name.find('#') else {
        return name.to_string();
    };
    let Some(e) = name[s + 1..].find('#').map(|i| i + s + 1) else {
        return name.to_string();
    };
    format!("{}{}", &name[..s], &name[e + 1..])
}

fn f64_of(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn floats(s: &str) -> Vec<f64> {
    s.split_whitespace().filter_map(|t| t.parse::<f64>().ok()).collect()
}

fn color_of(s: &str) -> [u8; 3] {
    let v = floats(s);
    let c = |i: usize| (255.0 * v.get(i).copied().unwrap_or(0.0)).round().clamp(0.0, 255.0) as u8;
    [c(0), c(1), c(2)]
}

fn affine_of(s: &str) -> Option<Affine> {
    let v = floats(s);
    (v.len() >= 6).then(|| Affine::from_array([v[0], v[1], v[2], v[3], v[4], v[5]]))
}

pub fn read_flames_from_file(path: impl AsRef<Path>) -> Result<Vec<Flame>, FlameIoError> {
    let src = std::fs::read_to_string(path)?;
    read_flames(&src)
}

pub fn read_flames(src: &str) -> Result<Vec<Flame>, FlameIoError> {
    let root = xml::parse(src)?;
    let mut elements = Vec::new();
    collect_flames(&root, &mut elements);
    if elements.is_empty() {
        return Err(FlameIoError::NoFlame);
    }
    Ok(elements.into_iter().map(parse_flame).collect())
}

fn collect_flames<'a>(el: &'a Element, out: &mut Vec<&'a Element>) {
    for c in &el.children {
        if c.name == "flame" || c.name == "jwf-flame" {
            out.push(c);
        } else {
            collect_flames(c, out);
        }
    }
}

fn parse_flame(el: &Element) -> Flame {
    let mut flame = Flame::default();
    // The shape sets its defaults first; explicit attributes override them below.
    if let Some(shape) = el.attr("cam_dof_shape").and_then(crate::render::dof::DofShape::parse) {
        flame.set_dof_shape(shape);
    }
    let has_bg_type = el.attr("background_type").is_some();
    let mut smooth_gradient = None;
    for (k, v) in &el.attrs {
        let f = f64_of(v);
        let mut known = true;
        match k.as_str() {
            "name" => flame.name = v.clone(),
            "size" => {
                let s = floats(v);
                if s.len() >= 2 {
                    flame.width = s[0].max(1.0) as u32;
                    flame.height = s[1].max(1.0) as u32;
                }
            }
            "center" => {
                let s = floats(v);
                if s.len() >= 2 {
                    flame.centre_x = s[0];
                    flame.centre_y = s[1];
                }
            }
            "scale" => flame.pixels_per_unit = f.unwrap_or(50.0),
            "rotate" => flame.cam_roll = f.unwrap_or(0.0),
            "filter" => flame.spatial_filter_radius = f.unwrap_or(0.75),
            "filter_kernel" => {
                if let Some(kern) = FilterKernel::parse(v) {
                    flame.filter_kernel = kern;
                }
            }
            "post_optix_denoiser" => flame.ai_post_denoiser = Denoiser::Optix,
            "ai_post_denoiser" => {
                if let Some(d) = Denoiser::parse(v) {
                    flame.ai_post_denoiser = d;
                }
            }
            "post_optix_denoiser_blend" => flame.post_denoiser_blend = f.unwrap_or(0.11111),
            "ai_post_denoiser_only_for_cpu" => flame.post_denoiser_only_for_cpu = v.trim() == "1",
            "post_blur_radius" => flame.post_blur_radius = f.unwrap_or(0.0).max(0.0) as u32,
            "post_blur_fade" => flame.post_blur_fade = f.unwrap_or(0.95),
            "post_blur_falloff" => flame.post_blur_falloff = f.unwrap_or(2.0),
            "filter_sharpness" => flame.filter_sharpness = f.unwrap_or(4.0),
            "filter_low_density" => flame.filter_low_density = f.unwrap_or(0.025),
            "filter_indicator" => flame.filter_indicator = v.trim() == "1",
            "oversample" => flame.spatial_oversampling = f.unwrap_or(1.0).clamp(1.0, 6.0) as u32,
            "quality" => flame.sample_density = f.unwrap_or(100.0),
            "background" => {
                flame.background.color = color_of(v);
                if !has_bg_type {
                    flame.background.kind = BgColorType::SingleColor;
                }
            }
            "background_type" => flame.background.kind = BgColorType::parse(v),
            "background_ul" => flame.background.ul = color_of(v),
            "background_ur" => flame.background.ur = color_of(v),
            "background_ll" => flame.background.ll = color_of(v),
            "background_lr" => flame.background.lr = color_of(v),
            "background_cc" => flame.background.cc = color_of(v),
            "bg_transparency" => flame.bg_transparency = v == "1",
            "fg_opacity" => flame.fg_opacity = f.unwrap_or(1.0),
            "brightness" => flame.brightness = f.unwrap_or(4.0),
            "saturation" => flame.saturation = f.unwrap_or(1.0),
            "gamma" => flame.gamma = f.unwrap_or(4.0),
            "gamma_threshold" => flame.gamma_threshold = f.unwrap_or(0.01),
            "vibrancy" => flame.vibrancy = f.unwrap_or(1.0),
            "contrast" => flame.contrast = f.unwrap_or(1.0),
            "white_level" => flame.white_level = f.unwrap_or(220.0),
            "cam_zoom" => flame.cam_zoom = f.unwrap_or(1.0),
            "cam_pitch" => flame.cam_pitch = f.unwrap_or(0.0).to_degrees(),
            "cam_yaw" => flame.cam_yaw = f.unwrap_or(0.0).to_degrees(),
            "cam_roll" => flame.cam_bank = f.unwrap_or(0.0).to_degrees(),
            "cam_persp" | "cam_perspective" => flame.cam_perspective = f.unwrap_or(0.0),
            "cam_pos_x" => flame.cam_pos_x = f.unwrap_or(0.0),
            "cam_pos_y" => flame.cam_pos_y = f.unwrap_or(0.0),
            "cam_pos_z" => flame.cam_pos_z = f.unwrap_or(0.0),
            "cam_zpos" => flame.cam_z = f.unwrap_or(0.0),
            "cam_dof" => flame.cam_dof = f.unwrap_or(0.0),
            "new_dof" => flame.new_cam_dof = v == "1",
            "cam_dof_shape" => {}
            "cam_dof_scale" => flame.cam_dof_scale = f.unwrap_or(1.0),
            "cam_dof_rotate" => flame.cam_dof_angle = f.unwrap_or(0.0),
            "cam_dof_fade" => flame.cam_dof_fade = f.unwrap_or(1.0),
            "cam_dof_param1" => flame.cam_dof_param1 = f.unwrap_or(0.0),
            "cam_dof_param2" => flame.cam_dof_param2 = f.unwrap_or(0.0),
            "cam_dof_param3" => flame.cam_dof_param3 = f.unwrap_or(0.0),
            "cam_dof_param4" => flame.cam_dof_param4 = f.unwrap_or(0.0),
            "cam_dof_param5" => flame.cam_dof_param5 = f.unwrap_or(0.0),
            "cam_dof_param6" => flame.cam_dof_param6 = f.unwrap_or(0.0),
            "cam_dof_area" => flame.cam_dof_area = f.unwrap_or(0.5),
            "cam_dof_exponent" => flame.cam_dof_exponent = f.unwrap_or(2.0),
            "cam_xfocus" => flame.focus_x = f.unwrap_or(0.0),
            "cam_yfocus" => flame.focus_y = f.unwrap_or(0.0),
            "cam_zfocus" => flame.focus_z = f.unwrap_or(0.0),
            "cam_zdimish" => flame.dimish_z = f.unwrap_or(0.0),
            "cam_zdimdist" => flame.dim_z_distance = f.unwrap_or(0.0),
            "low_density_brightness" => flame.low_density_brightness = f.unwrap_or(0.24),
            "balancing_red" => flame.balance_red = f.unwrap_or(1.0),
            "balancing_green" => flame.balance_green = f.unwrap_or(1.0),
            "balancing_blue" => flame.balance_blue = f.unwrap_or(1.0),
            "antialias_amount" => flame.antialias_amount = f.unwrap_or(0.25),
            "antialias_radius" => {
                if let Some(r) = f.filter(|r| *r > 0.0) {
                    flame.antialias_radius = r;
                }
            }
            "de_radius" => flame.de_radius = f.unwrap_or(1.0),
            "de_curve" => flame.de_curve = f.unwrap_or(0.8),
            "de_comparison_line" => flame.de_comparison_line = f.unwrap_or(0.0),
            "post_symmetry_type" => flame.post_symmetry.kind = PostSymmetryType::parse(v),
            "post_symmetry_order" => flame.post_symmetry.order = f.unwrap_or(3.0) as i32,
            "post_symmetry_centre_x" => flame.post_symmetry.centre_x = f.unwrap_or(0.0),
            "post_symmetry_centre_y" => flame.post_symmetry.centre_y = f.unwrap_or(0.0),
            "post_symmetry_distance" => flame.post_symmetry.distance = f.unwrap_or(1.25),
            "post_symmetry_rotation" => flame.post_symmetry.rotation = f.unwrap_or(6.0),
            "preserve_z" => flame.preserve_z = v == "1",
            "new_linear" => flame.preserve_z = v.trim() == "1",
            "smooth_gradient" => smooth_gradient = Some(v == "1"),
            "frame_count" => flame.animation.frame_count = f.unwrap_or(100.0).max(1.0) as u32,
            "fps" => flame.animation.fps = f.unwrap_or(25.0).max(1.0),
            "motion_blur_length" => flame.animation.motion_blur_length = f.unwrap_or(0.0).max(0.0) as u32,
            "motion_blur_timestep" => flame.animation.motion_blur_time_step = f.unwrap_or(0.05),
            "motion_blur_decay" => flame.animation.motion_blur_decay = f.unwrap_or(0.03),
            "frame" => flame.frame = f.unwrap_or(1.0).max(1.0) as u32,
            // Regenerated on save.
            "version" => {}
            _ => known = false,
        }
        if !known {
            flame.extra_attrs.push((k.clone(), v.clone()));
        }
    }
    flame.solid = crate::solid::SolidSettings::take_from_attrs(&mut flame.extra_attrs);
    flame.mixer = crate::mixer::ChannelMixer::take_from_attrs(&mut flame.extra_attrs);

    let layer_elements: Vec<&Element> = el.children_named("layer").collect();
    if layer_elements.is_empty() {
        let mut layer = parse_layer_content(el);
        if let Some(s) = smooth_gradient {
            layer.smooth_gradient = s;
        }
        flame.layers = vec![layer];
    } else {
        flame.layers = layer_elements
            .into_iter()
            .map(|le| {
                let mut layer = parse_layer_content(le);
                for (k, v) in &le.attrs {
                    match k.as_str() {
                        "weight" => layer.weight = f64_of(v).unwrap_or(1.0),
                        "density" => layer.density = f64_of(v).unwrap_or(1.0),
                        "visible" => layer.visible = v.trim() == "1",
                        "name" => layer.name = v.clone(),
                        "smooth_gradient" => layer.smooth_gradient = v == "1",
                        _ => layer.extra_attrs.push((k.clone(), v.clone())),
                    }
                }
                layer
            })
            .collect();
    }
    match el.children_named("animation").next() {
        Some(a) => parse_animation(a, &mut flame),
        None => import_jwf_curves(&mut flame),
    }
    flame
}

fn parse_animation(el: &Element, flame: &mut Flame) {
    use crate::animation::{Interp, Key, Track};
    for t in el.children_named("track") {
        let Some(path) = t.attr("path") else { continue };
        let keys: Vec<Key> = t
            .attr("keys")
            .unwrap_or("")
            .split_whitespace()
            .filter_map(|kv| {
                let (f, v) = kv.split_once(':')?;
                Some(Key { frame: f.parse().ok()?, value: v.parse().ok()? })
            })
            .collect();
        if !keys.is_empty() {
            let mut tr = Track { path: path.to_string(), interp: Interp::parse(t.attr("interp").unwrap_or("smooth")), keys };
            tr.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
            flame.animation.tracks.push(tr);
        }
    }
}

/// Converts JWildfire motion curves (`<name>Curve_*` attributes) into animation tracks.
fn import_jwf_curves(flame: &mut Flame) {
    use crate::animation::{jwf_curve_track, jwf_flame_curve_path, jwf_xform_curve_suffix};
    let is_curve_attr = |k: &str, prefix: &str| k.starts_with(prefix);
    // Flame-level curves
    let names: Vec<String> = flame.extra_attrs.iter().filter_map(|(k, _)| k.strip_suffix("Curve_enabled").map(str::to_string)).collect();
    for n in names {
        let prefix = format!("{n}Curve_");
        if let Some(path) = jwf_flame_curve_path(&n)
            && let Some(t) = jwf_curve_track(&flame.extra_attrs, &prefix, path)
        {
            flame.animation.tracks.push(t);
        }
        flame.extra_attrs.retain(|(k, _)| !is_curve_attr(k, &prefix));
    }
    // XForm and variation parameter curves
    let mut tracks = Vec::new();
    for (li, layer) in flame.layers.iter_mut().enumerate() {
        for (fin, list) in [(false, &mut layer.xforms), (true, &mut layer.final_xforms)] {
            for (xi, xf) in list.iter_mut().enumerate() {
                let base = if fin { format!("layer{li}.final{xi}") } else { format!("layer{li}.xf{xi}") };
                let names: Vec<String> = xf.extra_attrs.iter().filter_map(|(k, _)| k.strip_suffix("Curve_enabled").map(str::to_string)).collect();
                for n in names {
                    let prefix = format!("{n}Curve_");
                    if let Some(suffix) = jwf_xform_curve_suffix(&n)
                        && let Some(t) = jwf_curve_track(&xf.extra_attrs, &prefix, &format!("{base}.{suffix}"))
                    {
                        tracks.push(t);
                    }
                    xf.extra_attrs.retain(|(k, _)| !is_curve_attr(k, &prefix));
                }
                for (vi, var) in xf.variations.iter_mut().enumerate() {
                    let pnames: Vec<String> = var.raw_params.iter().filter_map(|(k, _)| k.strip_suffix("_enabled").map(str::to_string)).collect();
                    for p in pnames {
                        let prefix = format!("{p}_");
                        if let Some(t) = jwf_curve_track(&var.raw_params, &prefix, &format!("{base}.var{vi}.p.{p}")) {
                            tracks.push(t);
                        }
                        var.raw_params.retain(|(k, _)| !(k.starts_with(&prefix) && k != &p));
                    }
                }
            }
        }
    }
    // Solid rendering light curves (`sld_render_light_altitude0_*`)
    for (i, _) in flame.solid.lights.clone().iter().enumerate() {
        for field in ["altitude", "azimuth"] {
            let prefix = format!("sld_render_light_{field}{i}_");
            if let Some(t) = jwf_curve_track(&flame.extra_attrs, &prefix, &format!("solid.light{i}.{field}")) {
                tracks.push(t);
            }
            flame.extra_attrs.retain(|(k, _)| !k.starts_with(&prefix));
        }
    }
    flame.animation.tracks.extend(tracks);
}

fn parse_layer_content(el: &Element) -> Layer {
    let mut layer = Layer::default();
    layer.xforms = el.children_named("xform").map(parse_xform).collect();
    layer.final_xforms = el.children_named("finalxform").map(parse_xform).collect();
    layer.gradient = parse_gradient(el).unwrap_or_default();
    layer
}

fn parse_gradient(el: &Element) -> Option<Gradient> {
    if let Some(p) = el.children_named("palette").next() {
        let hex: String = p.text.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        if hex.len() >= 6 && hex.len() % 6 == 0 {
            let colors: Vec<[u8; 3]> = (0..hex.len() / 6)
                .map(|i| {
                    let h = |o: usize| u8::from_str_radix(&hex[i * 6 + o..i * 6 + o + 2], 16).unwrap_or(0);
                    [h(0), h(2), h(4)]
                })
                .collect();
            let mut g = Gradient { colors: vec![[0, 0, 0]; crate::gradient::GRADIENT_SIZE] };
            for (i, c) in colors.into_iter().take(crate::gradient::GRADIENT_SIZE).enumerate() {
                g.colors[i] = c;
            }
            return Some(g);
        }
    }
    let colors: Vec<&Element> = el.children_named("color").collect();
    if !colors.is_empty() {
        let mut g = Gradient { colors: vec![[0, 0, 0]; crate::gradient::GRADIENT_SIZE] };
        for c in colors {
            let idx = c.attr("index").and_then(f64_of).unwrap_or(0.0) as usize;
            let rgb = c.attr("rgb").map(floats).unwrap_or_default();
            if idx < g.colors.len() && rgb.len() >= 3 {
                g.colors[idx] = [rgb[0].round().clamp(0.0, 255.0) as u8, rgb[1].round().clamp(0.0, 255.0) as u8, rgb[2].round().clamp(0.0, 255.0) as u8];
            }
        }
        return Some(g);
    }
    None
}

fn parse_xform(el: &Element) -> XForm {
    let mut xf = XForm::default();
    // First pass: find variation attributes (raw name -> instance index).
    let mut var_raw_names: Vec<String> = Vec::new();
    for (k, v) in &el.attrs {
        let name = remove_index(k);
        if is_variation_name(&name) {
            let mut inst = VariationInstance::new(&name, f64_of(v).unwrap_or(0.0));
            // Keep the original (alias) name for unknown variations.
            if variations::find(&name).is_none() {
                inst.name = name.clone();
            }
            xf.variations.push(inst);
            var_raw_names.push(k.clone());
        }
    }
    let var_of_param = |key: &str| -> Option<(usize, String)> {
        // Longest raw name that prefixes "<raw>_" wins.
        let mut best: Option<(usize, usize)> = None;
        for (i, raw) in var_raw_names.iter().enumerate() {
            if key.len() > raw.len() + 1 && key.starts_with(raw.as_str()) && key.as_bytes()[raw.len()] == b'_' {
                if best.is_none_or(|(_, l)| raw.len() > l) {
                    best = Some((i, raw.len()));
                }
            }
        }
        best.map(|(i, l)| (i, key[l + 1..].to_string()))
    };

    for (k, v) in &el.attrs {
        if var_raw_names.contains(k) {
            continue;
        }
        let f = f64_of(v);
        let mut known = true;
        match k.as_str() {
            "name" => xf.name = v.clone(),
            "weight" => xf.weight = f.unwrap_or(0.5),
            "color" => xf.color = f.unwrap_or(0.0),
            "symmetry" | "color_speed" => xf.color_symmetry = f.unwrap_or(0.0),
            "color_type" => xf.color_type = ColorType::parse(v),
            "targetcolor" => xf.target_color = color_of(v),
            "material" => xf.material = f.unwrap_or(0.0),
            "material_speed" => xf.material_speed = f.unwrap_or(0.0),
            "mod_gamma" => xf.mod_gamma = f.unwrap_or(0.0),
            "mod_gamma_speed" => xf.mod_gamma_speed = f.unwrap_or(0.0),
            "mod_contrast" => xf.mod_contrast = f.unwrap_or(0.0),
            "mod_contrast_speed" => xf.mod_contrast_speed = f.unwrap_or(0.0),
            "mod_saturation" => xf.mod_saturation = f.unwrap_or(0.0),
            "mod_saturation_speed" => xf.mod_saturation_speed = f.unwrap_or(0.0),
            "mod_hue" => xf.mod_hue = f.unwrap_or(0.0),
            "mod_hue_speed" => xf.mod_hue_speed = f.unwrap_or(0.0),
            "opacity" => {
                let o = f.unwrap_or(1.0);
                xf.opacity = o;
                xf.draw_mode = if o.abs() <= crate::math::EPSILON {
                    DrawMode::Hidden
                } else if (o - 1.0).abs() > crate::math::EPSILON {
                    DrawMode::Opaque
                } else {
                    DrawMode::Normal
                };
            }
            "coefs" => xf.coefs = affine_of(v).unwrap_or_default(),
            "post" => xf.post = affine_of(v).unwrap_or_default(),
            "yzCoefs" => xf.yz_coefs = affine_of(v).unwrap_or_default(),
            "yzPost" => xf.yz_post = affine_of(v).unwrap_or_default(),
            "zxCoefs" => xf.zx_coefs = affine_of(v).unwrap_or_default(),
            "zxPost" => xf.zx_post = affine_of(v).unwrap_or_default(),
            "chaos" => xf.chaos = floats(v),
            _ => known = false,
        }
        if known {
            continue;
        }
        if let Some((vi, pname)) = var_of_param(k) {
            let inst = &mut xf.variations[vi];
            if pname == "fx_priority" {
                if let Some(p) = f {
                    inst.priority = p as i32;
                }
                continue;
            }
            if let Some(val) = f
                && inst.set_param(&pname, val)
            {
                continue;
            }
            // replaces defaults of resources (see VariationInstance::new)
            match inst.raw_params.iter_mut().find(|(k, _)| *k == pname) {
                Some(e) => e.1 = v.clone(),
                None => inst.raw_params.push((pname, v.clone())),
            }
            continue;
        }
        xf.extra_attrs.push((k.clone(), v.clone()));
    }
    xf.weighting = crate::weighting::WeightingField::take_from_attrs(&mut xf.extra_attrs);
    xf
}

// ---------------------------------------------------------------------------------------------
// Writing

pub(crate) fn fmt_f(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 { format!("{:.1}", v) } else { format!("{}", v) }
}

fn fmt_color(c: [u8; 3]) -> String {
    format!("{} {} {}", fmt_f(c[0] as f64 / 255.0), fmt_f(c[1] as f64 / 255.0), fmt_f(c[2] as f64 / 255.0))
}

fn fmt_affine(a: &Affine) -> String {
    a.as_array().iter().map(|v| fmt_f(*v)).collect::<Vec<_>>().join(" ")
}

fn attr(out: &mut String, k: &str, v: impl AsRef<str>) {
    let _ = write!(out, " {}=\"{}\"", k, escape(v.as_ref()));
}

pub fn write_flame(flame: &Flame) -> String {
    let multi_layer = flame.layers.len() > 1;
    let tag = if multi_layer { "jwf-flame" } else { "flame" };
    let mut s = String::new();
    let _ = write!(s, "<{tag}");
    if !flame.name.is_empty() {
        attr(&mut s, "name", &flame.name);
    }
    attr(&mut s, "version", concat!("RWildFire ", env!("CARGO_PKG_VERSION")));
    attr(&mut s, "size", format!("{} {}", flame.width, flame.height));
    attr(&mut s, "center", format!("{} {}", fmt_f(flame.centre_x), fmt_f(flame.centre_y)));
    attr(&mut s, "scale", fmt_f(flame.pixels_per_unit));
    attr(&mut s, "rotate", fmt_f(flame.cam_roll));
    attr(&mut s, "filter", fmt_f(flame.spatial_filter_radius));
    attr(&mut s, "filter_type", if flame.filter_kernel == FilterKernel::MitchellSinePow { "ADAPTIVE" } else if flame.filter_kernel.is_sharpening() { "GLOBAL_SHARPENING" } else { "GLOBAL_SMOOTHING" });
    attr(&mut s, "filter_kernel", flame.filter_kernel.as_str());
    attr(&mut s, "filter_indicator", if flame.filter_indicator { "1" } else { "0" });
    attr(&mut s, "filter_sharpness", fmt_f(flame.filter_sharpness));
    attr(&mut s, "filter_low_density", fmt_f(flame.filter_low_density));
    attr(&mut s, "post_blur_radius", flame.post_blur_radius.to_string());
    attr(&mut s, "post_blur_fade", fmt_f(flame.post_blur_fade));
    attr(&mut s, "post_blur_falloff", fmt_f(flame.post_blur_falloff));
    flame.solid.write_attrs(|k, v| attr(&mut s, k, v));
    flame.mixer.write_attrs(|k, v| attr(&mut s, k, v));
    attr(&mut s, "ai_post_denoiser", flame.ai_post_denoiser.as_str());
    attr(&mut s, "post_optix_denoiser_blend", fmt_f(flame.post_denoiser_blend));
    attr(&mut s, "ai_post_denoiser_only_for_cpu", if flame.post_denoiser_only_for_cpu { "1" } else { "0" });
    attr(&mut s, "oversample", flame.spatial_oversampling.to_string());
    attr(&mut s, "de_radius", fmt_f(flame.de_radius));
    attr(&mut s, "de_curve", fmt_f(flame.de_curve));
    attr(&mut s, "de_comparison_line", fmt_f(flame.de_comparison_line));
    attr(&mut s, "quality", fmt_f(flame.sample_density));
    attr(&mut s, "background_type", flame.background.kind.as_str());
    attr(&mut s, "background_ul", fmt_color(flame.background.ul));
    attr(&mut s, "background_ur", fmt_color(flame.background.ur));
    attr(&mut s, "background_ll", fmt_color(flame.background.ll));
    attr(&mut s, "background_lr", fmt_color(flame.background.lr));
    attr(&mut s, "background_cc", fmt_color(flame.background.cc));
    attr(&mut s, "background", fmt_color(flame.background.color));
    attr(&mut s, "bg_transparency", if flame.bg_transparency { "1" } else { "0" });
    attr(&mut s, "fg_opacity", fmt_f(flame.fg_opacity));
    attr(&mut s, "brightness", fmt_f(flame.brightness));
    attr(&mut s, "saturation", fmt_f(flame.saturation));
    attr(&mut s, "gamma", fmt_f(flame.gamma));
    attr(&mut s, "gamma_threshold", fmt_f(flame.gamma_threshold));
    attr(&mut s, "vibrancy", fmt_f(flame.vibrancy));
    attr(&mut s, "contrast", fmt_f(flame.contrast));
    attr(&mut s, "white_level", fmt_f(flame.white_level));
    attr(&mut s, "temporal_samples", "1.0");
    attr(&mut s, "cam_zoom", fmt_f(flame.cam_zoom));
    attr(&mut s, "cam_pitch", fmt_f(flame.cam_pitch.to_radians()));
    attr(&mut s, "cam_yaw", fmt_f(flame.cam_yaw.to_radians()));
    attr(&mut s, "cam_roll", fmt_f(flame.cam_bank.to_radians()));
    attr(&mut s, "cam_persp", fmt_f(flame.cam_perspective));
    attr(&mut s, "cam_zdimish", fmt_f(flame.dimish_z));
    attr(&mut s, "cam_zdimdist", fmt_f(flame.dim_z_distance));
    attr(&mut s, "cam_pos_x", fmt_f(flame.cam_pos_x));
    attr(&mut s, "cam_pos_y", fmt_f(flame.cam_pos_y));
    attr(&mut s, "cam_pos_z", fmt_f(flame.cam_pos_z));
    attr(&mut s, "cam_zpos", fmt_f(flame.cam_z));
    attr(&mut s, "cam_dof", fmt_f(flame.cam_dof));
    if flame.new_cam_dof {
        attr(&mut s, "new_dof", "1");
    }
    attr(&mut s, "cam_dof_shape", flame.cam_dof_shape.as_str());
    attr(&mut s, "cam_dof_scale", fmt_f(flame.cam_dof_scale));
    attr(&mut s, "cam_dof_rotate", fmt_f(flame.cam_dof_angle));
    attr(&mut s, "cam_dof_fade", fmt_f(flame.cam_dof_fade));
    for (i, v) in [flame.cam_dof_param1, flame.cam_dof_param2, flame.cam_dof_param3, flame.cam_dof_param4, flame.cam_dof_param5, flame.cam_dof_param6].iter().enumerate() {
        attr(&mut s, &format!("cam_dof_param{}", i + 1), fmt_f(*v));
    }
    attr(&mut s, "cam_dof_area", fmt_f(flame.cam_dof_area));
    attr(&mut s, "cam_dof_exponent", fmt_f(flame.cam_dof_exponent));
    attr(&mut s, "cam_xfocus", fmt_f(flame.focus_x));
    attr(&mut s, "cam_yfocus", fmt_f(flame.focus_y));
    attr(&mut s, "cam_zfocus", fmt_f(flame.focus_z));
    attr(&mut s, "low_density_brightness", fmt_f(flame.low_density_brightness));
    attr(&mut s, "balancing_red", fmt_f(flame.balance_red));
    attr(&mut s, "balancing_green", fmt_f(flame.balance_green));
    attr(&mut s, "balancing_blue", fmt_f(flame.balance_blue));
    if flame.preserve_z {
        attr(&mut s, "preserve_z", "1");
    }
    attr(&mut s, "antialias_amount", fmt_f(flame.antialias_amount));
    attr(&mut s, "antialias_radius", fmt_f(flame.antialias_radius));
    let ps = &flame.post_symmetry;
    attr(&mut s, "post_symmetry_type", ps.kind.as_str());
    attr(&mut s, "post_symmetry_order", ps.order.to_string());
    attr(&mut s, "post_symmetry_centre_x", fmt_f(ps.centre_x));
    attr(&mut s, "post_symmetry_centre_y", fmt_f(ps.centre_y));
    attr(&mut s, "post_symmetry_distance", fmt_f(ps.distance));
    attr(&mut s, "post_symmetry_rotation", fmt_f(ps.rotation));
    if !multi_layer {
        attr(&mut s, "smooth_gradient", if flame.first_layer().smooth_gradient { "1" } else { "0" });
    }
    let anim = &flame.animation;
    attr(&mut s, "frame", flame.frame.to_string());
    attr(&mut s, "frame_count", anim.frame_count.to_string());
    attr(&mut s, "fps", fmt_f(anim.fps));
    attr(&mut s, "motion_blur_length", anim.motion_blur_length.to_string());
    attr(&mut s, "motion_blur_timestep", fmt_f(anim.motion_blur_time_step));
    attr(&mut s, "motion_blur_decay", fmt_f(anim.motion_blur_decay));
    for (k, v) in &flame.extra_attrs {
        attr(&mut s, k, v);
    }
    s.push_str(">\n");

    if multi_layer {
        for layer in &flame.layers {
            s.push_str("  <layer");
            attr(&mut s, "weight", fmt_f(layer.weight));
            attr(&mut s, "density", fmt_f(layer.density));
            attr(&mut s, "visible", if layer.visible { "1" } else { "0" });
            if !layer.name.is_empty() {
                attr(&mut s, "name", &layer.name);
            }
            attr(&mut s, "smooth_gradient", if layer.smooth_gradient { "1" } else { "0" });
            for (k, v) in &layer.extra_attrs {
                attr(&mut s, k, v);
            }
            s.push_str(">\n");
            write_layer_content(&mut s, layer, "    ");
            s.push_str("  </layer>\n");
        }
    } else {
        write_layer_content(&mut s, flame.first_layer(), "  ");
    }
    if flame.animation.is_animated() {
        s.push_str("  <animation>\n");
        for t in &flame.animation.tracks {
            if t.keys.is_empty() {
                continue;
            }
            let keys: Vec<String> = t.keys.iter().map(|k| format!("{}:{}", fmt_f(k.frame), fmt_f(k.value))).collect();
            let _ = writeln!(s, "    <track path=\"{}\" interp=\"{}\" keys=\"{}\" />", escape(&t.path), t.interp.as_str(), keys.join(" "));
        }
        s.push_str("  </animation>\n");
    }
    let _ = writeln!(s, "</{tag}>");
    s
}

fn write_layer_content(s: &mut String, layer: &Layer, indent: &str) {
    for xf in &layer.xforms {
        let _ = write!(s, "{indent}<xform");
        write_xform_attrs(s, xf);
        s.push_str(" />\n");
    }
    for xf in &layer.final_xforms {
        let _ = write!(s, "{indent}<finalxform");
        write_xform_attrs(s, xf);
        s.push_str(" />\n");
    }
    let _ = write!(s, "{indent}<palette count=\"256\" format=\"RGB\">{}</palette>\n", layer.gradient.to_hex());
}

fn write_xform_attrs(s: &mut String, xf: &XForm) {
    if !xf.name.is_empty() {
        attr(s, "name", &xf.name);
    }
    attr(s, "weight", fmt_f(xf.weight));
    if xf.color_type != ColorType::Unset {
        attr(s, "color_type", xf.color_type.as_str());
    }
    if xf.color_type == ColorType::Target {
        attr(s, "targetcolor", fmt_color(xf.target_color));
    } else {
        attr(s, "color", fmt_f(xf.color));
    }
    attr(s, "symmetry", fmt_f(xf.color_symmetry));
    attr(s, "material", fmt_f(xf.material));
    attr(s, "material_speed", fmt_f(xf.material_speed));
    attr(s, "mod_gamma", fmt_f(xf.mod_gamma));
    attr(s, "mod_gamma_speed", fmt_f(xf.mod_gamma_speed));
    attr(s, "mod_contrast", fmt_f(xf.mod_contrast));
    attr(s, "mod_contrast_speed", fmt_f(xf.mod_contrast_speed));
    attr(s, "mod_saturation", fmt_f(xf.mod_saturation));
    attr(s, "mod_saturation_speed", fmt_f(xf.mod_saturation_speed));
    attr(s, "mod_hue", fmt_f(xf.mod_hue));
    attr(s, "mod_hue_speed", fmt_f(xf.mod_hue_speed));
    match xf.draw_mode {
        DrawMode::Opaque => attr(s, "opacity", fmt_f(xf.opacity)),
        DrawMode::Hidden => attr(s, "opacity", "0.0"),
        DrawMode::Normal => {}
    }

    let mut used: Vec<String> = Vec::new();
    for var in &xf.variations {
        let mut name = var.name.clone();
        let mut counter = 1;
        while used.contains(&name) {
            name = format!("{}#{}#", var.name, counter);
            counter += 1;
        }
        used.push(name.clone());
        attr(s, &name, fmt_f(var.amount));
        attr(s, &format!("{name}_fx_priority"), var.priority.to_string());
        if let Some(def) = variations::find(&var.name) {
            for (i, p) in def.params.iter().enumerate() {
                let val = var.params.get(i).copied().unwrap_or(p.default);
                let txt = if p.integer { format!("{}", val.round() as i64) } else { fmt_f(val) };
                attr(s, &format!("{name}_{}", p.name), txt);
            }
        }
        for (k, v) in &var.raw_params {
            attr(s, &format!("{name}_{k}"), v);
        }
    }

    attr(s, "coefs", fmt_affine(&xf.coefs));
    if !xf.post.is_identity() {
        attr(s, "post", fmt_affine(&xf.post));
    }
    if !xf.yz_coefs.is_identity() {
        attr(s, "yzCoefs", fmt_affine(&xf.yz_coefs));
    }
    if !xf.yz_post.is_identity() {
        attr(s, "yzPost", fmt_affine(&xf.yz_post));
    }
    if !xf.zx_coefs.is_identity() {
        attr(s, "zxCoefs", fmt_affine(&xf.zx_coefs));
    }
    if !xf.zx_post.is_identity() {
        attr(s, "zxPost", fmt_affine(&xf.zx_post));
    }
    if !xf.chaos.is_empty() {
        attr(s, "chaos", xf.chaos.iter().map(|v| fmt_f(*v)).collect::<Vec<_>>().join(" "));
    }
    xf.weighting.write_attrs(|k, v| attr(s, k, v));
    for (k, v) in &xf.extra_attrs {
        attr(s, k, v);
    }
}

/// Writes several flames into one document (`<flames>` wrapper when more than one).
pub fn write_flames(flames: &[Flame]) -> String {
    if flames.len() == 1 {
        return write_flame(&flames[0]);
    }
    let mut s = String::from("<flames>\n");
    for f in flames {
        s.push_str(&write_flame(f));
    }
    s.push_str("</flames>\n");
    s
}

pub fn write_flame_to_file(flame: &Flame, path: impl AsRef<Path>) -> Result<(), FlameIoError> {
    std::fs::write(path, write_flame(flame))?;
    Ok(())
}
