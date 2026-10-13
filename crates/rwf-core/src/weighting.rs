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

//! Xform weighting fields (`org.jwildfire.create.tina.base.weightingfield`): a noise or image
//! field evaluated at the affine (or source) point that modulates the color, the variation
//! amounts and up to three variation parameters of an xform, and can jitter its output.

use std::collections::HashSet;
use std::sync::{Arc, OnceLock};

use crate::fastnoise::{CellularDistance, CellularReturn, FastNoise, FractalType, NoiseType};
use crate::math::EPSILON;
use crate::resources::ImageMap;

/// `WeightingFieldType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FieldType {
    #[default]
    None,
    CellularNoise,
    CubicNoise,
    CubicFractalNoise,
    PerlinNoise,
    PerlinFractalNoise,
    SimplexNoise,
    SimplexFractalNoise,
    ValueNoise,
    ValueFractalNoise,
    WhiteNoise,
    ImageMap,
}

impl FieldType {
    pub const ALL: [FieldType; 12] = [
        FieldType::None,
        FieldType::CellularNoise,
        FieldType::CubicNoise,
        FieldType::CubicFractalNoise,
        FieldType::PerlinNoise,
        FieldType::PerlinFractalNoise,
        FieldType::SimplexNoise,
        FieldType::SimplexFractalNoise,
        FieldType::ValueNoise,
        FieldType::ValueFractalNoise,
        FieldType::WhiteNoise,
        FieldType::ImageMap,
    ];

    /// Name in `.flame` files (the Java enum constant).
    pub fn as_str(self) -> &'static str {
        match self {
            FieldType::None => "NONE",
            FieldType::CellularNoise => "CELLULAR_NOISE",
            FieldType::CubicNoise => "CUBIC_NOISE",
            FieldType::CubicFractalNoise => "CUBIC_FRACTAL_NOISE",
            FieldType::PerlinNoise => "PERLIN_NOISE",
            FieldType::PerlinFractalNoise => "PERLIN_FRACTAL_NOISE",
            FieldType::SimplexNoise => "SIMPLEX_NOISE",
            FieldType::SimplexFractalNoise => "SIMPLEX_FRACTAL_NOISE",
            FieldType::ValueNoise => "VALUE_NOISE",
            FieldType::ValueFractalNoise => "VALUE_FRACTAL_NOISE",
            FieldType::WhiteNoise => "WHITE_NOISE",
            FieldType::ImageMap => "IMAGE_MAP",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FieldType::None => "None",
            FieldType::CellularNoise => "Cellular noise",
            FieldType::CubicNoise => "Cubic noise",
            FieldType::CubicFractalNoise => "Cubic fractal noise",
            FieldType::PerlinNoise => "Perlin noise",
            FieldType::PerlinFractalNoise => "Perlin fractal noise",
            FieldType::SimplexNoise => "Simplex noise",
            FieldType::SimplexFractalNoise => "Simplex fractal noise",
            FieldType::ValueNoise => "Value noise",
            FieldType::ValueFractalNoise => "Value fractal noise",
            FieldType::WhiteNoise => "White noise",
            FieldType::ImageMap => "Image map",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s.trim())
    }

    pub fn is_fractal(self) -> bool {
        matches!(self, FieldType::CubicFractalNoise | FieldType::PerlinFractalNoise | FieldType::SimplexFractalNoise | FieldType::ValueFractalNoise)
    }
}

/// `WeightingFieldInputType`: the point the field is evaluated at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FieldInput {
    /// The point after the affine transform.
    #[default]
    Affine,
    /// The source point of the xform.
    Position,
}

/// Modulation of one variation parameter (`weightingFieldVarParam1..3`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct VarParamWeight {
    pub intensity: f64,
    pub var_name: String,
    /// A parameter name of the variation, or `amount`.
    pub param_name: String,
}

/// The weighting field settings of an xform.
#[derive(Clone, Debug, PartialEq)]
pub struct WeightingField {
    pub field_type: FieldType,
    pub input: FieldInput,
    pub color_intensity: f64,
    pub var_amount_intensity: f64,
    pub var_params: [VarParamWeight; 3],
    pub jitter_intensity: f64,
    pub cmap_filename: String,
    pub cmap_x_size: f64,
    pub cmap_y_size: f64,
    pub cmap_x_centre: f64,
    pub cmap_y_centre: f64,
    pub noise_seed: i32,
    pub noise_frequency: f64,
    pub fractal_type: FractalType,
    pub fractal_octaves: i32,
    pub fractal_gain: f64,
    pub fractal_lacunarity: f64,
    pub cellular_return: CellularReturn,
    pub cellular_distance: CellularDistance,
}

impl Default for WeightingField {
    fn default() -> Self {
        Self {
            field_type: FieldType::None,
            input: FieldInput::Affine,
            color_intensity: 0.0,
            var_amount_intensity: 0.0,
            var_params: Default::default(),
            jitter_intensity: 0.0,
            cmap_filename: String::new(),
            cmap_x_size: 4.0,
            cmap_y_size: 4.0,
            cmap_x_centre: 0.0,
            cmap_y_centre: 0.0,
            noise_seed: 1337,
            noise_frequency: 1.0,
            fractal_type: FractalType::Fbm,
            fractal_octaves: 3,
            fractal_gain: 0.5,
            fractal_lacunarity: 2.0,
            cellular_return: CellularReturn::Distance2,
            cellular_distance: CellularDistance::Euclidean,
        }
    }
}

pub fn fractal_type_str(t: FractalType) -> &'static str {
    match t {
        FractalType::Fbm => "FBM",
        FractalType::Billow => "BILLOW",
        FractalType::RigidMulti => "RIGID_MULTI",
    }
}

pub fn cellular_return_str(t: CellularReturn) -> &'static str {
    match t {
        CellularReturn::CellValue => "CELL_VALUE",
        CellularReturn::Distance => "DISTANCE",
        CellularReturn::Distance2 => "DISTANCE2",
        CellularReturn::Distance2Add => "DISTANCE_ADD",
        CellularReturn::Distance2Sub => "DISTANCE_SUB",
        CellularReturn::Distance2Mul => "DISTANCE_MUL",
        CellularReturn::Distance2Div => "DISTANCE_DIV",
    }
}

pub fn cellular_distance_str(t: CellularDistance) -> &'static str {
    match t {
        CellularDistance::Euclidean => "EUCLIDIAN",
        CellularDistance::Manhattan => "MANHATTAN",
        CellularDistance::Natural => "NATURAL",
    }
}

/// The attributes of [`WeightingField`] (motion curves of them are kept as other attributes).
const ATTRS: [&str; 27] = [
    "wfield_type",
    "wfield_input",
    "wfield_color_intensity",
    "wfield_var_amount_intensity",
    "wfield_var_param1_intensity",
    "wfield_var_param1_var_name",
    "wfield_var_param1_param_name",
    "wfield_var_param2_intensity",
    "wfield_var_param2_var_name",
    "wfield_var_param2_param_name",
    "wfield_var_param3_intensity",
    "wfield_var_param3_var_name",
    "wfield_var_param3_param_name",
    "wfield_jitter_intensity",
    "wfield_cmap_filename",
    "wfield_cmap_xsize",
    "wfield_cmap_ysize",
    "wfield_cmap_xcentre",
    "wfield_cmap_ycentre",
    "wfield_noise_seed",
    "wfield_noise_frequency",
    "wfield_fract_noise_fract_type",
    "wfield_fract_noise_gain",
    "wfield_fract_noise_lacunarity",
    "wfield_fract_noise_octaves",
    "wfield_cell_noise_return_type",
    "wfield_cell_noise_dist_function",
];

pub const FRACTAL_TYPES: [FractalType; 3] = [FractalType::Fbm, FractalType::Billow, FractalType::RigidMulti];
pub const CELLULAR_RETURNS: [CellularReturn; 7] = [
    CellularReturn::CellValue,
    CellularReturn::Distance,
    CellularReturn::Distance2,
    CellularReturn::Distance2Add,
    CellularReturn::Distance2Sub,
    CellularReturn::Distance2Mul,
    CellularReturn::Distance2Div,
];
pub const CELLULAR_DISTANCES: [CellularDistance; 3] = [CellularDistance::Euclidean, CellularDistance::Manhattan, CellularDistance::Natural];

impl WeightingField {
    /// Reads the `wfield_*` attributes of an xform and removes them from `attrs`.
    pub fn take_from_attrs(attrs: &mut Vec<(String, String)>) -> Self {
        let mut w = Self::default();
        let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.trim().to_string());
        let f = |k: &str, d: &mut f64| {
            if let Some(v) = get(k).and_then(|v| v.parse().ok()) {
                *d = v;
            }
        };
        let i = |k: &str, d: &mut i32| {
            if let Some(v) = get(k).and_then(|v| v.parse::<f64>().ok()) {
                *d = v as i32;
            }
        };
        if let Some(t) = get("wfield_type").and_then(|v| FieldType::parse(&v)) {
            w.field_type = t;
        }
        if let Some(v) = get("wfield_input") {
            w.input = if v == "POSITION" { FieldInput::Position } else { FieldInput::Affine };
        }
        f("wfield_color_intensity", &mut w.color_intensity);
        f("wfield_var_amount_intensity", &mut w.var_amount_intensity);
        for (k, p) in w.var_params.iter_mut().enumerate() {
            let n = k + 1;
            f(&format!("wfield_var_param{n}_intensity"), &mut p.intensity);
            if let Some(v) = get(&format!("wfield_var_param{n}_var_name")) {
                p.var_name = v;
            }
            if let Some(v) = get(&format!("wfield_var_param{n}_param_name")) {
                p.param_name = v;
            }
        }
        f("wfield_jitter_intensity", &mut w.jitter_intensity);
        if let Some(v) = get("wfield_cmap_filename") {
            w.cmap_filename = v;
        }
        f("wfield_cmap_xsize", &mut w.cmap_x_size);
        f("wfield_cmap_ysize", &mut w.cmap_y_size);
        f("wfield_cmap_xcentre", &mut w.cmap_x_centre);
        f("wfield_cmap_ycentre", &mut w.cmap_y_centre);
        i("wfield_noise_seed", &mut w.noise_seed);
        f("wfield_noise_frequency", &mut w.noise_frequency);
        if let Some(t) = get("wfield_fract_noise_fract_type").and_then(|v| FRACTAL_TYPES.into_iter().find(|t| fractal_type_str(*t) == v)) {
            w.fractal_type = t;
        }
        f("wfield_fract_noise_gain", &mut w.fractal_gain);
        f("wfield_fract_noise_lacunarity", &mut w.fractal_lacunarity);
        i("wfield_fract_noise_octaves", &mut w.fractal_octaves);
        if let Some(t) = get("wfield_cell_noise_return_type").and_then(|v| CELLULAR_RETURNS.into_iter().find(|t| cellular_return_str(*t) == v)) {
            w.cellular_return = t;
        }
        if let Some(t) = get("wfield_cell_noise_dist_function").and_then(|v| CELLULAR_DISTANCES.into_iter().find(|t| cellular_distance_str(*t) == v)) {
            w.cellular_distance = t;
        }
        attrs.retain(|(k, _)| !ATTRS.contains(&k.as_str()));
        w
    }

    /// Writes the attributes read by [`Self::take_from_attrs`] (only for a field type other
    /// than none, as JWildfire).
    pub fn write_attrs(&self, mut attr: impl FnMut(&str, String)) {
        if self.field_type == FieldType::None {
            return;
        }
        let f = crate::io::fmt_f;
        attr("wfield_type", self.field_type.as_str().to_string());
        attr("wfield_input", if self.input == FieldInput::Position { "POSITION" } else { "AFFINE" }.to_string());
        attr("wfield_color_intensity", f(self.color_intensity));
        attr("wfield_var_amount_intensity", f(self.var_amount_intensity));
        for (k, p) in self.var_params.iter().enumerate() {
            let n = k + 1;
            attr(&format!("wfield_var_param{n}_intensity"), f(p.intensity));
            if p.intensity.abs() > EPSILON {
                attr(&format!("wfield_var_param{n}_var_name"), p.var_name.clone());
                attr(&format!("wfield_var_param{n}_param_name"), p.param_name.clone());
            }
        }
        attr("wfield_jitter_intensity", f(self.jitter_intensity));
        match self.field_type {
            FieldType::CellularNoise => {
                attr("wfield_noise_seed", self.noise_seed.to_string());
                attr("wfield_noise_frequency", f(self.noise_frequency));
                attr("wfield_cell_noise_return_type", cellular_return_str(self.cellular_return).to_string());
                attr("wfield_cell_noise_dist_function", cellular_distance_str(self.cellular_distance).to_string());
            }
            FieldType::CubicNoise | FieldType::PerlinNoise | FieldType::SimplexNoise | FieldType::ValueNoise => {
                attr("wfield_noise_seed", self.noise_seed.to_string());
                attr("wfield_noise_frequency", f(self.noise_frequency));
            }
            FieldType::WhiteNoise => attr("wfield_noise_seed", self.noise_seed.to_string()),
            t if t.is_fractal() => {
                attr("wfield_noise_seed", self.noise_seed.to_string());
                attr("wfield_noise_frequency", f(self.noise_frequency));
                attr("wfield_fract_noise_fract_type", fractal_type_str(self.fractal_type).to_string());
                attr("wfield_fract_noise_gain", f(self.fractal_gain));
                attr("wfield_fract_noise_lacunarity", f(self.fractal_lacunarity));
                attr("wfield_fract_noise_octaves", self.fractal_octaves.to_string());
            }
            FieldType::ImageMap => {
                attr("wfield_cmap_filename", self.cmap_filename.clone());
                attr("wfield_cmap_xsize", f(self.cmap_x_size));
                attr("wfield_cmap_ysize", f(self.cmap_y_size));
                attr("wfield_cmap_xcentre", f(self.cmap_x_centre));
                attr("wfield_cmap_ycentre", f(self.cmap_y_centre));
            }
            _ => {}
        }
    }

    /// `XForm.isWeightingFieldActive`.
    pub fn is_active(&self) -> bool {
        let p = |p: &VarParamWeight| p.intensity.abs() > EPSILON && !p.var_name.is_empty();
        (self.color_intensity.abs() > EPSILON
            || self.var_amount_intensity.abs() > EPSILON
            || self.var_params.iter().any(p)
            || self.jitter_intensity.abs() > EPSILON)
            && self.field_type != FieldType::None
    }

    /// The field function (`WeightingFieldType.getInstance`).
    pub fn field(&self) -> Field {
        let noise = |noise_type: NoiseType| FastNoise {
            seed: self.noise_seed,
            frequency: self.noise_frequency as f32,
            noise_type,
            octaves: self.fractal_octaves,
            lacunarity: self.fractal_lacunarity as f32,
            gain: self.fractal_gain as f32,
            fractal_type: self.fractal_type,
            cellular_distance: self.cellular_distance,
            cellular_return: self.cellular_return,
        };
        match self.field_type {
            FieldType::None => Field::Empty,
            FieldType::CellularNoise => Field::Noise(noise(NoiseType::Cellular)),
            FieldType::CubicNoise => Field::Noise(noise(NoiseType::Cubic)),
            FieldType::CubicFractalNoise => Field::Noise(noise(NoiseType::CubicFractal)),
            FieldType::PerlinNoise => Field::Noise(noise(NoiseType::Perlin)),
            FieldType::PerlinFractalNoise => Field::Noise(noise(NoiseType::PerlinFractal)),
            FieldType::SimplexNoise => Field::Noise(noise(NoiseType::Simplex)),
            FieldType::SimplexFractalNoise => Field::Noise(noise(NoiseType::SimplexFractal)),
            FieldType::ValueNoise => Field::Noise(noise(NoiseType::Value)),
            FieldType::ValueFractalNoise => Field::Noise(noise(NoiseType::ValueFractal)),
            FieldType::WhiteNoise => Field::Noise(noise(NoiseType::WhiteNoise)),
            FieldType::ImageMap => {
                let img = if self.cmap_filename.is_empty() { Some(default_image()) } else { crate::resources::image(&self.cmap_filename) };
                match img {
                    Some(img) => Field::Image {
                        img,
                        x_centre: self.cmap_x_centre,
                        y_centre: self.cmap_y_centre,
                        x_size: self.cmap_x_size,
                        y_size: self.cmap_y_size,
                    },
                    None => Field::Empty,
                }
            }
        }
    }
}

/// An evaluable weighting field.
#[derive(Clone, Debug)]
pub enum Field {
    Empty,
    Noise(FastNoise),
    Image { img: Arc<ImageMap>, x_centre: f64, y_centre: f64, x_size: f64, y_size: f64 },
}

impl Field {
    /// `WeightingField.getValue`.
    pub fn value(&self, x: f64, y: f64, z: f64) -> f64 {
        match self {
            Field::Empty => 0.0,
            Field::Noise(n) => n.get_noise(x as f32, y as f32, z as f32) as f64,
            Field::Image { img, x_centre, y_centre, x_size, y_size } => {
                let (w, h) = (img.width as i64, img.height as i64);
                let x_coord = (x + x_size * 0.5 - x_centre) / x_size * w as f64;
                let y_coord = (y + y_size * 0.5 - y_centre) / y_size * h as f64;
                // SimpleImage.getRValueIgnoreBounds
                let red = |ix: i64, iy: i64| if ix < 0 || ix >= w || iy < 0 || iy >= h { 0.0 } else { (img.pixels[(iy * w + ix) as usize] >> 16 & 255) as f64 };
                let (ix, iy) = (x_coord as i32 as i64, y_coord as i32 as i64);
                let (lu, ru, lb, rb) = (red(ix, iy), red(ix + 1, iy), red(ix, iy + 1), red(ix + 1, iy + 1));
                // GfxMathLib.blerp
                let lerp = |s: f64, e: f64, t: f64| s + (e - s) * t;
                let (px, py) = (x_coord - x_coord.trunc(), y_coord - y_coord.trunc());
                let gray = lerp(lerp(lu, ru, px), lerp(lb, rb, px), py);
                0.5 - gray / 255.0
            }
        }
    }
}

/// The image of an image map field without a file (`mb3dhm.jpg` of JWildfire).
fn default_image() -> Arc<ImageMap> {
    static IMG: OnceLock<Arc<ImageMap>> = OnceLock::new();
    IMG.get_or_init(|| {
        let bytes = include_bytes!("../assets/weighting_field_default.jpg");
        let img = image::load_from_memory(bytes).expect("bundled weighting field image").to_rgba8();
        Arc::new(ImageMap::from_rgba8(&img))
    })
    .clone()
}

/// `VariationFuncList.isValidVariationForWeightingFields`: variations cheap enough to have
/// their amount and parameters modulated (from JWildfire's measured variation costs).
pub fn is_valid_variation(name: &str) -> bool {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        include_str!("../assets/weighting_field_variations.txt").lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()
    })
    .contains(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attrs_round_trip() {
        let mut w = WeightingField { field_type: FieldType::PerlinFractalNoise, color_intensity: 0.5, jitter_intensity: -0.2, noise_seed: 42, ..Default::default() };
        w.var_params[1] = VarParamWeight { intensity: 1.5, var_name: "julia".into(), param_name: "amount".into() };
        let mut attrs = Vec::new();
        w.write_attrs(|k, v| attrs.push((k.to_string(), v)));
        attrs.push(("other".into(), "1".into()));
        let r = WeightingField::take_from_attrs(&mut attrs);
        assert_eq!(r, w);
        assert_eq!(attrs, vec![("other".to_string(), "1".to_string())]);
        assert!(r.is_active());
        assert!(!WeightingField::default().is_active());
    }

    #[test]
    fn valid_variations() {
        assert!(is_valid_variation("julia"));
        // JWildfire excludes linear (its measured init memory exceeds the limit)
        assert!(!is_valid_variation("linear"));
        assert!(!is_valid_variation("fract_mandelbrot_wf"));
    }

    #[test]
    fn noise_values_match_fastnoise_java() {
        // FastNoise (Java): seed 1337, frequency 0.7, octaves 2, gain 0.5, lacunarity 2, FBM,
        // CellValue, Euclidean at (0.3, 0.2, 0.1) and (-1.7, 2.4, -0.2) (raw float bits)
        let n = FastNoise { seed: 1337, frequency: 0.7, noise_type: NoiseType::Value, octaves: 2, ..FastNoise::default() };
        assert_eq!(n.get_noise(0.3, 0.2, 0.1).to_bits() as i32, -1123929163);
        assert_eq!(n.get_noise(-1.7, 2.4, -0.2).to_bits() as i32, 1042008876);
    }
}
