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

//! Flame data model, mirroring JWildfire's `Flame`, `Layer` and `XForm`.

use crate::gradient::Gradient;
use crate::variations;

/// 2D affine transform in JWildfire coefficient order:
/// `x' = c00*x + c10*y + c20`, `y' = c01*x + c11*y + c21`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub c00: f64,
    pub c01: f64,
    pub c10: f64,
    pub c11: f64,
    pub c20: f64,
    pub c21: f64,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine { c00: 1.0, c01: 0.0, c10: 0.0, c11: 1.0, c20: 0.0, c21: 0.0 };

    pub fn is_identity(&self) -> bool {
        use crate::math::EPSILON;
        (self.c00 - 1.0).abs() <= EPSILON
            && self.c01.abs() <= EPSILON
            && self.c10.abs() <= EPSILON
            && (self.c11 - 1.0).abs() <= EPSILON
            && self.c20.abs() <= EPSILON
            && self.c21.abs() <= EPSILON
    }

    #[inline]
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.c00 * x + self.c10 * y + self.c20, self.c01 * x + self.c11 * y + self.c21)
    }

    pub fn as_array(&self) -> [f64; 6] {
        [self.c00, self.c01, self.c10, self.c11, self.c20, self.c21]
    }

    pub fn from_array(a: [f64; 6]) -> Self {
        Self { c00: a[0], c01: a[1], c10: a[2], c11: a[3], c20: a[4], c21: a[5] }
    }

    /// The X axis vector (image of (1,0) without translation).
    pub fn x_axis(&self) -> (f64, f64) {
        (self.c00, self.c01)
    }

    pub fn y_axis(&self) -> (f64, f64) {
        (self.c10, self.c11)
    }

    pub fn origin(&self) -> (f64, f64) {
        (self.c20, self.c21)
    }

    /// Rotates the linear part around the origin of the transform (angle in degrees).
    pub fn rotate(&mut self, deg: f64) {
        let a = deg.to_radians();
        let (s, c) = a.sin_cos();
        let (x0, x1) = (self.c00, self.c01);
        let (y0, y1) = (self.c10, self.c11);
        self.c00 = c * x0 - s * x1;
        self.c01 = s * x0 + c * x1;
        self.c10 = c * y0 - s * y1;
        self.c11 = s * y0 + c * y1;
    }

    pub fn scale(&mut self, f: f64) {
        self.c00 *= f;
        self.c01 *= f;
        self.c10 *= f;
        self.c11 *= f;
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.c20 += dx;
        self.c21 += dy;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ColorType {
    #[default]
    Unset,
    None,
    Diffusion,
    Target,
    TargetG,
    Distance,
    Cyclic,
}

impl ColorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ColorType::Unset => "UNSET",
            ColorType::None => "NONE",
            ColorType::Diffusion => "DIFFUSION",
            ColorType::Target => "TARGET",
            ColorType::TargetG => "TARGETG",
            ColorType::Distance => "DISTANCE",
            ColorType::Cyclic => "CYCLIC",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "NONE" => ColorType::None,
            "DIFFUSION" => ColorType::Diffusion,
            "TARGET" => ColorType::Target,
            "TARGETG" => ColorType::TargetG,
            "DISTANCE" => ColorType::Distance,
            "CYCLIC" => ColorType::Cyclic,
            _ => ColorType::Unset,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DrawMode {
    #[default]
    Normal,
    Opaque,
    Hidden,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariationInstance {
    pub name: String,
    pub amount: f64,
    /// Execution slot: <0 pre, 0 normal, >0 post (JWildfire "priority").
    pub priority: i32,
    /// Parameter values, aligned with the definition's parameter list.
    pub params: Vec<f64>,
    /// Raw `(param, value)` attributes not mapped to `params` (unknown variations,
    /// resources, motion curves), kept for round-tripping.
    pub raw_params: Vec<(String, String)>,
}

impl VariationInstance {
    pub fn new(name: &str, amount: f64) -> Self {
        match variations::find(name) {
            Some(def) => {
                let mut inst = Self {
                    name: def.name.to_string(),
                    amount,
                    priority: def.kind.default_priority(),
                    params: def.params.iter().map(|p| p.default).collect(),
                    raw_params: Vec::new(),
                };
                for r in variations::resources_of(name).map_or(&[][..], |rv| rv.resources) {
                    if !r.default.is_empty() {
                        inst.set_resource(r.name, r.default);
                    }
                }
                // Formula variations start with their first preset (JWildfire: a random one).
                if let Some(p) = variations::presets::presets(name).and_then(|ps| ps.first()) {
                    variations::presets::apply(&mut inst, p.id);
                }
                inst
            }
            None => Self { name: name.to_string(), amount, priority: 0, params: Vec::new(), raw_params: Vec::new() },
        }
    }

    pub fn param(&self, name: &str) -> Option<f64> {
        let def = variations::find(&self.name)?;
        def.params.iter().position(|p| p.name == name).and_then(|i| self.params.get(i).copied())
    }

    /// Decoded value of a text resource parameter (empty if unset).
    pub fn resource(&self, name: &str) -> String {
        String::from_utf8_lossy(&self.resource_bytes(name)).into_owned()
    }

    /// Raw bytes of a resource parameter (empty if unset or malformed).
    pub fn resource_bytes(&self, name: &str) -> Vec<u8> {
        self.raw_params.iter().find(|(k, _)| k == name).and_then(|(_, v)| crate::resources::hex_decode(v)).unwrap_or_default()
    }

    /// Sets a text resource parameter (stored hex-encoded in `raw_params`, as in JWildfire files).
    pub fn set_resource(&mut self, name: &str, value: &str) {
        self.set_resource_bytes(name, value.as_bytes());
    }

    /// Sets a binary resource parameter (e.g. an embedded image file).
    pub fn set_resource_bytes(&mut self, name: &str, value: &[u8]) {
        let hex = crate::resources::hex_encode(value);
        match self.raw_params.iter_mut().find(|(k, _)| k == name) {
            Some(e) => e.1 = hex,
            None => self.raw_params.push((name.to_string(), hex)),
        }
    }

    pub fn set_param(&mut self, name: &str, value: f64) -> bool {
        if let Some(def) = variations::find(&self.name)
            && let Some(i) = def.params.iter().position(|p| p.name == name)
        {
            for k in self.params.len()..def.params.len() {
                self.params.push(def.params[k].default);
            }
            self.params[i] = value;
            return true;
        }
        false
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct XForm {
    pub name: String,
    pub weight: f64,
    pub color: f64,
    pub color_symmetry: f64,
    pub color_type: ColorType,
    pub target_color: [u8; 3],
    pub material: f64,
    pub material_speed: f64,
    pub mod_gamma: f64,
    pub mod_gamma_speed: f64,
    pub mod_contrast: f64,
    pub mod_contrast_speed: f64,
    pub mod_saturation: f64,
    pub mod_saturation_speed: f64,
    pub mod_hue: f64,
    pub mod_hue_speed: f64,
    pub opacity: f64,
    pub draw_mode: DrawMode,
    pub coefs: Affine,
    pub post: Affine,
    pub yz_coefs: Affine,
    pub yz_post: Affine,
    pub zx_coefs: Affine,
    pub zx_post: Affine,
    /// Per-target weight multipliers ("chaos"/xaos).
    pub chaos: Vec<f64>,
    pub variations: Vec<VariationInstance>,
    pub weighting: crate::weighting::WeightingField,
    /// Attributes not understood by this port, kept for round-tripping.
    pub extra_attrs: Vec<(String, String)>,
}

impl Default for XForm {
    fn default() -> Self {
        Self {
            name: String::new(),
            weight: 0.5,
            color: 0.0,
            color_symmetry: 0.0,
            color_type: ColorType::Unset,
            target_color: [255, 255, 255],
            material: 0.0,
            material_speed: 0.0,
            mod_gamma: 0.0,
            mod_gamma_speed: 0.0,
            mod_contrast: 0.0,
            mod_contrast_speed: 0.0,
            mod_saturation: 0.0,
            mod_saturation_speed: 0.0,
            mod_hue: 0.0,
            mod_hue_speed: 0.0,
            opacity: 1.0,
            draw_mode: DrawMode::Normal,
            coefs: Affine::IDENTITY,
            post: Affine::IDENTITY,
            yz_coefs: Affine::IDENTITY,
            yz_post: Affine::IDENTITY,
            zx_coefs: Affine::IDENTITY,
            zx_post: Affine::IDENTITY,
            chaos: Vec::new(),
            variations: Vec::new(),
            weighting: Default::default(),
            extra_attrs: Vec::new(),
        }
    }
}

impl XForm {
    pub fn with_variation(name: &str, amount: f64) -> Self {
        let mut xf = Self::default();
        xf.variations.push(VariationInstance::new(name, amount));
        xf
    }

    pub fn chaos_to(&self, idx: usize) -> f64 {
        self.chaos.get(idx).copied().unwrap_or(1.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub weight: f64,
    pub density: f64,
    pub visible: bool,
    pub gradient: Gradient,
    /// Moves the colors along the gradient when rendering (entries; fractional values blend);
    /// animatable for color cycling. JWildfire's `palette_modShift`.
    pub gradient_shift: f64,
    /// An image used as a two-dimensional gradient (JWildfire's gradient map).
    pub gradient_map: GradientMap,
    /// The hue/saturation/luminosity curves the gradient was made with (gradient curve editor).
    pub gradient_curves: Option<crate::gradient_ops::GradientCurves>,
    pub smooth_gradient: bool,
    pub xforms: Vec<XForm>,
    pub final_xforms: Vec<XForm>,
    /// Background xforms (`<bgxform>`): evaluated once per output pixel to color the background.
    pub bg_xforms: Vec<XForm>,
    pub extra_attrs: Vec<(String, String)>,
}

/// The three xform lists of a layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XFormKind {
    Normal,
    Final,
    Background,
}

impl XFormKind {
    pub const ALL: [XFormKind; 3] = [XFormKind::Normal, XFormKind::Final, XFormKind::Background];

    /// Prefix of the xform segment in property paths (`layer0.xf2`, `layer0.final0`, `layer0.bg1`).
    pub fn path_prefix(self) -> &'static str {
        match self {
            XFormKind::Normal => "xf",
            XFormKind::Final => "final",
            XFormKind::Background => "bg",
        }
    }
}

impl Layer {
    pub fn xforms_of(&self, kind: XFormKind) -> &Vec<XForm> {
        match kind {
            XFormKind::Normal => &self.xforms,
            XFormKind::Final => &self.final_xforms,
            XFormKind::Background => &self.bg_xforms,
        }
    }

    pub fn xforms_of_mut(&mut self, kind: XFormKind) -> &mut Vec<XForm> {
        match kind {
            XFormKind::Normal => &mut self.xforms,
            XFormKind::Final => &mut self.final_xforms,
            XFormKind::Background => &mut self.bg_xforms,
        }
    }
}

/// JWildfire's gradient map: transforms with diffusion or cyclic coloring take their color
/// from an image, at a position from the point and its color index
/// (`TransformationGradientMapColorStep`).
#[derive(Clone, Debug, PartialEq)]
pub struct GradientMap {
    /// Image file; empty = off (the gradient is used).
    pub file: String,
    pub h_offset: f64,
    pub h_scale: f64,
    pub v_offset: f64,
    pub v_scale: f64,
    /// How much the color index moves the position (`gradient_map_lcolor_add`).
    pub local_color_add: f64,
    /// How much the color index scales the position (`gradient_map_lcolor_scale`).
    pub local_color_scale: f64,
}

impl Default for GradientMap {
    fn default() -> Self {
        Self { file: String::new(), h_offset: 0.0, h_scale: 1.0, v_offset: 0.0, v_scale: 1.0, local_color_add: 0.8, local_color_scale: 0.2 }
    }
}

impl GradientMap {
    pub fn is_on(&self) -> bool {
        !self.file.is_empty()
    }

    /// Reads a `gradient_map*` attribute; false if `key` is none of them.
    pub fn read_attr(&mut self, key: &str, v: &str) -> bool {
        let f = || v.trim().parse::<f64>().ok();
        match key {
            "gradient_map" => self.file = v.to_string(),
            "gradient_map_hoffset" => self.h_offset = f().unwrap_or(0.0),
            "gradient_map_hscale" => self.h_scale = f().unwrap_or(1.0),
            "gradient_map_voffset" => self.v_offset = f().unwrap_or(0.0),
            "gradient_map_vscale" => self.v_scale = f().unwrap_or(1.0),
            "gradient_map_lcolor_add" => self.local_color_add = f().unwrap_or(0.8),
            "gradient_map_lcolor_scale" => self.local_color_scale = f().unwrap_or(0.2),
            _ => return false,
        }
        true
    }

    /// The attributes to write (none when off).
    pub fn attrs(&self) -> Vec<(&'static str, String)> {
        if !self.is_on() {
            return Vec::new();
        }
        let f = crate::io::fmt_f;
        vec![
            ("gradient_map", self.file.clone()),
            ("gradient_map_hoffset", f(self.h_offset)),
            ("gradient_map_hscale", f(self.h_scale)),
            ("gradient_map_voffset", f(self.v_offset)),
            ("gradient_map_vscale", f(self.v_scale)),
            ("gradient_map_lcolor_add", f(self.local_color_add)),
            ("gradient_map_lcolor_scale", f(self.local_color_scale)),
        ]
    }
}

/// A group of flame settings that can be reset to its defaults (JWildfire's
/// `Flame.reset*Settings`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlameSection {
    View,
    Orientation,
    Camera3d,
    Stereo,
    DepthOfField,
    Tone,
    Background,
    Filter,
    PostBlur,
    DensityEstimation,
    Symmetry,
    MotionBlur,
}

impl Flame {
    /// Resets the settings of `section` to the defaults.
    pub fn reset_section(&mut self, section: FlameSection) {
        let d = Flame::default();
        match section {
            FlameSection::View => {
                (self.centre_x, self.centre_y, self.cam_zoom) = (d.centre_x, d.centre_y, d.cam_zoom);
            }
            FlameSection::Orientation => {
                (self.cam_roll, self.cam_pitch, self.cam_yaw, self.cam_bank) = (d.cam_roll, d.cam_pitch, d.cam_yaw, d.cam_bank);
            }
            FlameSection::Camera3d => {
                (self.cam_perspective, self.cam_pos_x, self.cam_pos_y, self.cam_pos_z) = (d.cam_perspective, d.cam_pos_x, d.cam_pos_y, d.cam_pos_z);
                (self.dimish_z, self.dim_z_distance, self.dim_z_color) = (d.dimish_z, d.dim_z_distance, d.dim_z_color);
            }
            FlameSection::Stereo => self.stereo3d = d.stereo3d,
            FlameSection::DepthOfField => {
                (self.cam_dof, self.new_cam_dof, self.cam_z) = (d.cam_dof, d.new_cam_dof, d.cam_z);
                (self.focus_x, self.focus_y, self.focus_z, self.cam_dof_area, self.cam_dof_exponent) = (d.focus_x, d.focus_y, d.focus_z, d.cam_dof_area, d.cam_dof_exponent);
                self.set_dof_shape(d.cam_dof_shape);
                (self.cam_dof_scale, self.cam_dof_angle, self.cam_dof_fade) = (d.cam_dof_scale, d.cam_dof_angle, d.cam_dof_fade);
            }
            FlameSection::Tone => {
                (self.brightness, self.gamma, self.gamma_threshold, self.vibrancy) = (d.brightness, d.gamma, d.gamma_threshold, d.vibrancy);
                (self.contrast, self.saturation, self.white_level, self.low_density_brightness) = (d.contrast, d.saturation, d.white_level, d.low_density_brightness);
                (self.balance_red, self.balance_green, self.balance_blue) = (d.balance_red, d.balance_green, d.balance_blue);
            }
            FlameSection::Background => {
                self.background = d.background;
                self.bg_image = d.bg_image;
                self.fg_opacity = d.fg_opacity;
            }
            FlameSection::Filter => {
                (self.filter_kernel, self.spatial_filter_radius, self.filter_sharpness, self.filter_low_density) = (d.filter_kernel, d.spatial_filter_radius, d.filter_sharpness, d.filter_low_density);
                (self.filter_indicator, self.spatial_oversampling, self.antialias_amount, self.antialias_radius) = (d.filter_indicator, d.spatial_oversampling, d.antialias_amount, d.antialias_radius);
            }
            FlameSection::PostBlur => {
                (self.post_blur_radius, self.post_blur_fade, self.post_blur_falloff) = (d.post_blur_radius, d.post_blur_fade, d.post_blur_falloff);
            }
            FlameSection::DensityEstimation => {
                (self.de_radius, self.de_curve, self.de_comparison_line) = (d.de_radius, d.de_curve, d.de_comparison_line);
            }
            FlameSection::Symmetry => self.post_symmetry = d.post_symmetry,
            FlameSection::MotionBlur => {
                let a = &mut self.animation;
                (a.motion_blur_length, a.motion_blur_time_step, a.motion_blur_decay) = (d.animation.motion_blur_length, d.animation.motion_blur_time_step, d.animation.motion_blur_decay);
            }
        }
    }
}

impl Layer {
    /// The gradient as rendered: with [`Layer::gradient_shift`] applied.
    pub fn render_gradient(&self) -> std::borrow::Cow<'_, Gradient> {
        if self.gradient_shift == 0.0 {
            std::borrow::Cow::Borrowed(&self.gradient)
        } else {
            std::borrow::Cow::Owned(self.gradient.shifted(self.gradient_shift))
        }
    }
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            name: String::new(),
            weight: 1.0,
            density: 1.0,
            visible: true,
            gradient: Gradient::default(),
            gradient_shift: 0.0,
            gradient_map: GradientMap::default(),
            gradient_curves: None,
            smooth_gradient: false,
            xforms: Vec::new(),
            final_xforms: Vec::new(),
            bg_xforms: Vec::new(),
            extra_attrs: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BgColorType {
    SingleColor,
    Gradient2x2,
    #[default]
    Gradient2x2C,
}

impl BgColorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            BgColorType::SingleColor => "SINGLE_COLOR",
            BgColorType::Gradient2x2 => "GRADIENT_2X2",
            BgColorType::Gradient2x2C => "GRADIENT_2X2_C",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "SINGLE_COLOR" => BgColorType::SingleColor,
            "GRADIENT_2X2" => BgColorType::Gradient2x2,
            _ => BgColorType::Gradient2x2C,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Background {
    pub kind: BgColorType,
    pub color: [u8; 3],
    pub ul: [u8; 3],
    pub ur: [u8; 3],
    pub ll: [u8; 3],
    pub lr: [u8; 3],
    pub cc: [u8; 3],
}

/// Name of the Z-buffer file next to a rendered image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ZBufferFilename {
    /// `zbuf_<name>.png`
    PreZbuf,
    /// `<name>_depth.png`
    #[default]
    PostDepth,
}

impl ZBufferFilename {
    pub fn as_str(&self) -> &'static str {
        match self {
            ZBufferFilename::PreZbuf => "PRE_ZBUF",
            ZBufferFilename::PostDepth => "POST_DEPTH",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "PRE_ZBUF" => Some(ZBufferFilename::PreZbuf),
            "POST_DEPTH" => Some(ZBufferFilename::PostDepth),
            _ => None,
        }
    }
    /// Z-buffer file for an image file (`Tools.makeZBufferFilename`); always a PNG.
    pub fn path_for(&self, image: &std::path::Path) -> std::path::PathBuf {
        let stem = image.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let name = match self {
            ZBufferFilename::PreZbuf => format!("zbuf_{stem}.png"),
            ZBufferFilename::PostDepth => format!("{stem}_depth.png"),
        };
        image.with_file_name(name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PostSymmetryType {
    #[default]
    None,
    Point,
    XAxis,
    YAxis,
}

impl PostSymmetryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PostSymmetryType::None => "NONE",
            PostSymmetryType::Point => "POINT",
            PostSymmetryType::XAxis => "X_AXIS",
            PostSymmetryType::YAxis => "Y_AXIS",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "POINT" => PostSymmetryType::Point,
            "X_AXIS" => PostSymmetryType::XAxis,
            "Y_AXIS" => PostSymmetryType::YAxis,
            _ => PostSymmetryType::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PostSymmetry {
    pub kind: PostSymmetryType,
    pub order: i32,
    pub centre_x: f64,
    pub centre_y: f64,
    pub distance: f64,
    pub rotation: f64,
}

impl Default for PostSymmetry {
    fn default() -> Self {
        Self { kind: PostSymmetryType::None, order: 3, centre_x: 0.0, centre_y: 0.0, distance: 1.25, rotation: 6.0 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Flame {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// The resolution profile (`WxH`) and quality profile (caption) last chosen for the flame
    /// (JWildfire's `resolution_profile`, `quality_profile`; empty: none).
    pub resolution_profile: String,
    pub quality_profile: String,
    pub centre_x: f64,
    pub centre_y: f64,
    pub pixels_per_unit: f64,
    pub cam_zoom: f64,
    /// Rotation in the image plane, degrees.
    pub cam_roll: f64,
    /// Degrees.
    pub cam_pitch: f64,
    /// Degrees.
    pub cam_yaw: f64,
    /// Degrees.
    pub cam_bank: f64,
    pub cam_perspective: f64,
    pub cam_pos_x: f64,
    pub cam_pos_y: f64,
    pub cam_pos_z: f64,
    pub cam_z: f64,
    pub cam_dof: f64,
    /// `true` = focus-point DOF (`new_dof`), `false` = legacy z-plane DOF.
    pub new_cam_dof: bool,
    pub cam_dof_shape: crate::render::dof::DofShape,
    pub cam_dof_scale: f64,
    /// Degrees.
    pub cam_dof_angle: f64,
    pub cam_dof_fade: f64,
    pub cam_dof_param1: f64,
    pub cam_dof_param2: f64,
    pub cam_dof_param3: f64,
    pub cam_dof_param4: f64,
    pub cam_dof_param5: f64,
    pub cam_dof_param6: f64,
    pub cam_dof_area: f64,
    pub cam_dof_exponent: f64,
    pub focus_x: f64,
    pub focus_y: f64,
    pub focus_z: f64,
    pub dimish_z: f64,
    pub dim_z_distance: f64,
    /// Color that depth-faded points fade into (`cam_zdimcolor`; black by default).
    pub dim_z_color: [u8; 3],

    pub brightness: f64,
    pub contrast: f64,
    pub gamma: f64,
    pub gamma_threshold: f64,
    pub vibrancy: f64,
    pub white_level: f64,
    pub saturation: f64,
    pub low_density_brightness: f64,
    pub balance_red: f64,
    pub balance_green: f64,
    pub balance_blue: f64,
    pub fg_opacity: f64,

    pub sample_density: f64,
    pub spatial_filter_radius: f64,
    pub filter_kernel: crate::render::filter::FilterKernel,
    pub spatial_oversampling: u32,
    /// Adaptive filter (`MitchellSinePow`): edge detection threshold and the intensity below
    /// which the low-density kernel is used.
    pub filter_sharpness: f64,
    pub filter_low_density: f64,
    /// Debug view of the adaptive filter: colors pixels by the kernel chosen for them.
    pub filter_indicator: bool,
    /// Post blur: every sample is splatted over a `(2r+1)²` kernel, except a `fade` fraction
    /// of them; `falloff` controls the kernel decay.
    pub post_blur_radius: u32,
    pub post_blur_fade: f64,
    pub post_blur_falloff: f64,
    /// Z-buffer output: depth mapping (`gray = (scale * 0.001 * z - shift) * 32767 + 32767`,
    /// a negative scale inverts it), minimum gray level `bias` (0..1) and file naming.
    pub zbuffer_scale: f64,
    pub zbuffer_bias: f64,
    pub zbuffer_shift: f64,
    pub zbuffer_filename: ZBufferFilename,
    /// AI post denoiser applied to final renders; `blend` mixes the original back in.
    pub ai_post_denoiser: crate::render::denoise::Denoiser,
    pub post_denoiser_blend: f64,
    pub post_denoiser_only_for_cpu: bool,
    /// RWildFire: keeps the original image where pixels have enough samples (0 = off, 1 =
    /// already from few samples), see [`crate::render::denoise::detail_weights`].
    pub post_denoiser_detail: f64,
    /// RWildFire: contrast adaptive sharpening of the denoised image (0 = off .. 1).
    pub post_denoiser_sharpen: f64,
    /// Solid rendering (lights, materials, ambient occlusion, shadows).
    pub solid: crate::solid::SolidSettings,
    pub mixer: crate::mixer::ChannelMixer,
    pub antialias_amount: f64,
    pub antialias_radius: f64,
    /// Density estimation radius (0 = off), curve and the before/after comparison line (0..1).
    pub de_radius: f64,
    pub de_curve: f64,
    pub de_comparison_line: f64,

    /// When set, 2D variations pass the z coordinate through (`preserve_z`).
    pub preserve_z: bool,
    /// Current animation frame, 1-based as in JWildfire (set by `apply_animation`; used by
    /// image sequences of variations).
    pub frame: u32,
    pub background: Background,
    /// Image file shown behind the flame instead of the background colors, stretched to the
    /// image size (`background_image`; empty = none).
    pub bg_image: String,
    pub bg_transparency: bool,
    /// Stereo 3D images (anaglyph, side by side, interpolated pairs).
    pub stereo3d: crate::stereo::Stereo3d,
    /// The eye being rendered (set while rendering stereo images; not saved).
    pub stereo_eye: crate::stereo::StereoEye,
    pub post_symmetry: PostSymmetry,

    pub layers: Vec<Layer>,
    pub animation: crate::animation::Animation,
    /// Attributes not understood by this port, kept for round-tripping.
    pub extra_attrs: Vec<(String, String)>,
}

impl Default for Flame {
    fn default() -> Self {
        Self {
            name: String::new(),
            width: 800,
            height: 600,
            resolution_profile: String::new(),
            quality_profile: String::new(),
            centre_x: 0.0,
            centre_y: 0.0,
            pixels_per_unit: 50.0,
            cam_zoom: 1.0,
            cam_roll: 0.0,
            cam_pitch: 0.0,
            cam_yaw: 0.0,
            cam_bank: 0.0,
            cam_perspective: 0.0,
            cam_pos_x: 0.0,
            cam_pos_y: 0.0,
            cam_pos_z: 0.0,
            cam_z: 0.0,
            cam_dof: 0.0,
            new_cam_dof: false,
            cam_dof_shape: crate::render::dof::DofShape::Bubble,
            cam_dof_scale: 1.0,
            cam_dof_angle: 0.0,
            cam_dof_fade: 1.0,
            cam_dof_param1: 0.0,
            cam_dof_param2: 0.0,
            cam_dof_param3: 0.0,
            cam_dof_param4: 0.0,
            cam_dof_param5: 0.0,
            cam_dof_param6: 0.0,
            cam_dof_area: 0.5,
            cam_dof_exponent: 2.0,
            focus_x: 0.0,
            focus_y: 0.0,
            focus_z: 0.0,
            dimish_z: 0.0,
            dim_z_distance: 0.0,
            dim_z_color: [0, 0, 0],
            brightness: 4.0,
            contrast: 1.0,
            gamma: 4.0,
            gamma_threshold: 0.01,
            vibrancy: 1.0,
            white_level: 220.0,
            saturation: 1.0,
            low_density_brightness: 0.24,
            balance_red: 1.0,
            balance_green: 1.0,
            balance_blue: 1.0,
            fg_opacity: 1.0,
            sample_density: 100.0,
            spatial_filter_radius: 0.75,
            filter_kernel: crate::render::filter::FilterKernel::MitchellSmooth,
            spatial_oversampling: 1,
            filter_sharpness: 4.0,
            filter_low_density: 0.025,
            filter_indicator: false,
            post_blur_radius: 0,
            post_blur_fade: 0.95,
            post_blur_falloff: 2.0,
            zbuffer_scale: 1.0,
            zbuffer_bias: 0.0,
            zbuffer_shift: 0.0,
            zbuffer_filename: ZBufferFilename::PostDepth,
            ai_post_denoiser: crate::render::denoise::Denoiser::None,
            post_denoiser_blend: 0.11111,
            // JWildfire defaults to true; RWildFire renders mostly on the GPU.
            post_denoiser_only_for_cpu: false,
            post_denoiser_detail: 0.6,
            post_denoiser_sharpen: 0.0,
            solid: crate::solid::SolidSettings::default(),
            mixer: crate::mixer::ChannelMixer::default(),
            antialias_amount: 0.25,
            antialias_radius: 0.5,
            de_radius: 1.0,
            de_curve: 0.8,
            de_comparison_line: 0.0,
            preserve_z: false,
            frame: 1,
            background: Background::default(),
            bg_image: String::new(),
            bg_transparency: true,
            stereo3d: crate::stereo::Stereo3d::default(),
            stereo_eye: crate::stereo::StereoEye::Unspecified,
            post_symmetry: PostSymmetry::default(),
            layers: vec![Layer::default()],
            animation: crate::animation::Animation::default(),
            extra_attrs: Vec::new(),
        }
    }
}

impl Flame {
    /// True if any layer has background xforms (`Flame.hasBGTransforms`).
    pub fn has_bg_xforms(&self) -> bool {
        self.layers.iter().any(|l| !l.bg_xforms.is_empty())
    }

    /// Selects a bokeh shape and applies its default parameters (JWildfire `setDefaultParams`).
    pub fn set_dof_shape(&mut self, shape: crate::render::dof::DofShape) {
        let (fade, scale, angle, p) = shape.defaults();
        self.cam_dof_shape = shape;
        self.cam_dof_fade = fade;
        self.cam_dof_scale = scale;
        self.cam_dof_angle = angle;
        [self.cam_dof_param1, self.cam_dof_param2, self.cam_dof_param3, self.cam_dof_param4, self.cam_dof_param5, self.cam_dof_param6] = p;
    }

    pub fn first_layer(&self) -> &Layer {
        &self.layers[0]
    }

    pub fn first_layer_mut(&mut self) -> &mut Layer {
        if self.layers.is_empty() {
            self.layers.push(Layer::default());
        }
        &mut self.layers[0]
    }

    /// True if the 3D camera path (pitch/yaw/perspective...) is needed.
    pub fn is_3d_projection_required(&self) -> bool {
        use crate::math::EPSILON;
        self.solid.enabled
            || self.cam_yaw.abs() > EPSILON
            || self.cam_pitch.abs() > EPSILON
            || self.cam_bank.abs() > EPSILON
            || self.cam_perspective.abs() > EPSILON
            || self.dimish_z.abs() > EPSILON
            || self.cam_dof.abs() > EPSILON
            || self.cam_pos_x.abs() > EPSILON
            || self.cam_pos_y.abs() > EPSILON
            || self.cam_pos_z.abs() > EPSILON
    }
}
