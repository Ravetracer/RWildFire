//! Solid rendering settings (port of `org.jwildfire.create.tina.base.solidrender`): lights,
//! materials, ambient occlusion, hard shadows and the post DOF bokeh.

use crate::render::filter::FilterKernel;

/// `ShadowType`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ShadowType {
    #[default]
    Off,
    Fast,
    Smooth,
}

impl ShadowType {
    pub const ALL: &'static [ShadowType] = &[ShadowType::Off, ShadowType::Fast, ShadowType::Smooth];

    pub fn as_str(&self) -> &'static str {
        match self {
            ShadowType::Off => "OFF",
            ShadowType::Fast => "FAST",
            ShadowType::Smooth => "SMOOTH",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ShadowType::Off => "Off",
            ShadowType::Fast => "Fast",
            ShadowType::Smooth => "Smooth",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == s.trim())
    }
}

/// `LightDiffFuncPreset`: diffuse response to the cosine between light and normal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LightDiffFunc {
    #[default]
    Cosa,
    CosaSquare,
    CosaHalve,
    CosaHalveSquare,
}

impl LightDiffFunc {
    pub const ALL: &'static [LightDiffFunc] = &[LightDiffFunc::Cosa, LightDiffFunc::CosaSquare, LightDiffFunc::CosaHalve, LightDiffFunc::CosaHalveSquare];

    pub fn as_str(&self) -> &'static str {
        match self {
            LightDiffFunc::Cosa => "COSA",
            LightDiffFunc::CosaSquare => "COSA_SQUARE",
            LightDiffFunc::CosaHalve => "COSA_HALVE",
            LightDiffFunc::CosaHalveSquare => "COSA_HALVE_SQUARE",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            LightDiffFunc::Cosa => "cos",
            LightDiffFunc::CosaSquare => "cos²",
            LightDiffFunc::CosaHalve => "cos / 2 + ½",
            LightDiffFunc::CosaHalveSquare => "(cos / 2 + ½)²",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == s.trim())
    }

    /// Index for the GPU shader.
    pub fn index(&self) -> u32 {
        Self::ALL.iter().position(|f| f == self).unwrap_or(0) as u32
    }

    #[inline]
    pub fn evaluate(&self, cosa: f64) -> f64 {
        match self {
            LightDiffFunc::Cosa => cosa,
            LightDiffFunc::CosaSquare => cosa * cosa,
            LightDiffFunc::CosaHalve => cosa * 0.5 + 0.5,
            LightDiffFunc::CosaHalveSquare => {
                let v = cosa * 0.5 + 0.5;
                v * v
            }
        }
    }
}

/// `ReflectionMapping`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ReflectionMapping {
    Spherical,
    #[default]
    BlinnNewell,
}

impl ReflectionMapping {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReflectionMapping::Spherical => "SPHERICAL",
            ReflectionMapping::BlinnNewell => "BLINN_NEWELL",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "SPHERICAL" => Some(ReflectionMapping::Spherical),
            "BLINN_NEWELL" => Some(ReflectionMapping::BlinnNewell),
            _ => None,
        }
    }
}

/// `MaterialSettings`
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    pub diffuse: f64,
    pub ambient: f64,
    pub phong: f64,
    pub phong_size: f64,
    pub phong_color: [f64; 3],
    pub light_diff_func: LightDiffFunc,
    pub refl_map_intensity: f64,
    /// Reflection map image; empty = none.
    pub refl_map_filename: String,
    pub reflection_mapping: ReflectionMapping,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            diffuse: 0.5,
            ambient: 1.0,
            phong: 1.0,
            phong_size: 24.0,
            phong_color: [0.0; 3],
            light_diff_func: LightDiffFunc::Cosa,
            refl_map_intensity: 0.5,
            refl_map_filename: String::new(),
            reflection_mapping: ReflectionMapping::BlinnNewell,
        }
    }
}

impl Material {
    /// `SolidRenderSettings.addMaterial`
    pub fn new_added() -> Self {
        Self { ambient: 0.6, diffuse: 0.5, phong: 0.7, phong_size: 36.0, phong_color: [1.0, 0.9, 0.3], refl_map_intensity: 0.5, ..Self::default() }
    }
}

/// `DistantLight`: a directional light given by altitude/azimuth in degrees.
#[derive(Clone, Debug, PartialEq)]
pub struct DistantLight {
    pub altitude: f64,
    pub azimuth: f64,
    pub intensity: f64,
    pub color: [f64; 3],
    pub cast_shadows: bool,
    pub shadow_intensity: f64,
}

impl Default for DistantLight {
    fn default() -> Self {
        Self { altitude: 0.0, azimuth: 0.0, intensity: 0.5, color: [0.0; 3], cast_shadows: true, shadow_intensity: 0.8 }
    }
}

impl DistantLight {
    /// `SolidRenderSettings.addLight`
    pub fn new_added() -> Self {
        Self { altitude: 60.0, azimuth: -30.0, intensity: 0.7, shadow_intensity: 0.6, color: [1.0; 3], cast_shadows: false }
    }

    /// Rows of the light's projection matrix and its direction (`LightViewCalculator`).
    pub fn projection(&self) -> ([[f64; 3]; 3], [f64; 3]) {
        let alpha = (-self.altitude).to_radians();
        let beta = (-self.azimuth).to_radians();
        let (sina, cosa) = alpha.sin_cos();
        let (sinb, cosb) = beta.sin_cos();
        let m = [[cosb, 0.0, sinb], [sina * sinb, cosa, -sina * cosb], [-cosa * sinb, sina, cosa * cosb]];
        // transpose(m) * (0, 0, -1)
        let dir = [-m[2][0], -m[2][1], -m[2][2]];
        (m, dir)
    }
}

/// `SolidRenderSettings`
#[derive(Clone, Debug, PartialEq)]
pub struct SolidSettings {
    pub enabled: bool,
    pub ao_enabled: bool,
    pub ao_intensity: f64,
    pub ao_search_radius: f64,
    pub ao_blur_radius: f64,
    pub ao_radius_samples: u32,
    pub ao_azimuth_samples: u32,
    pub ao_falloff: f64,
    pub ao_affect_diffuse: f64,
    pub shadow_type: ShadowType,
    pub shadowmap_bias: f64,
    pub shadowmap_size: u32,
    pub shadow_smooth_radius: f64,
    pub post_bokeh_filter_kernel: FilterKernel,
    pub post_bokeh_intensity: f64,
    pub post_bokeh_brightness: f64,
    pub post_bokeh_size: f64,
    pub post_bokeh_activation: f64,
    pub materials: Vec<Material>,
    pub lights: Vec<DistantLight>,
}

impl Default for SolidSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            ao_enabled: true,
            ao_intensity: 0.6,
            ao_search_radius: 4.0,
            ao_blur_radius: 1.5,
            ao_radius_samples: 6,
            ao_azimuth_samples: 7,
            ao_falloff: 0.5,
            ao_affect_diffuse: 0.1,
            shadow_type: ShadowType::Off,
            shadowmap_bias: 0.01,
            shadowmap_size: 2048,
            shadow_smooth_radius: 1.0,
            post_bokeh_filter_kernel: FilterKernel::SinePow15,
            post_bokeh_intensity: 0.005,
            post_bokeh_brightness: 1.0,
            post_bokeh_size: 2.0,
            post_bokeh_activation: 0.2,
            materials: vec![Material {
                ambient: 0.5,
                diffuse: 0.7,
                phong: 0.6,
                phong_size: 24.0,
                phong_color: [1.0; 3],
                refl_map_intensity: 0.5,
                ..Material::default()
            }],
            lights: vec![
                DistantLight { altitude: 55.0, azimuth: -22.0, intensity: 0.8, shadow_intensity: 0.8, color: [1.0; 3], cast_shadows: true },
                DistantLight { altitude: 64.0, azimuth: 55.0, intensity: 0.6, shadow_intensity: 0.7, color: [1.0; 3], cast_shadows: false },
            ],
        }
    }
}

/// A material blended between two neighbours (`SolidRenderSettings.getInterpolatedMaterial`).
#[derive(Clone, Copy, Debug)]
pub struct MaterialRef<'a> {
    pub from: &'a Material,
    pub to: &'a Material,
    pub t: f64,
    /// Index of the material if it is not blended (blended materials have no reflection map).
    pub exact: Option<usize>,
}

impl MaterialRef<'_> {
    #[inline]
    fn mix(&self, f: impl Fn(&Material) -> f64) -> f64 {
        if self.t == 0.0 { f(self.from) } else { f(self.from) + (f(self.to) - f(self.from)) * self.t }
    }
    pub fn diffuse(&self) -> f64 {
        // Java morphMaterial overwrites the morphed diffuse with the reflection map intensity.
        if self.t == 0.0 { self.from.diffuse } else { self.mix(|m| m.refl_map_intensity) }
    }
    pub fn ambient(&self) -> f64 {
        self.mix(|m| m.ambient)
    }
    pub fn phong(&self) -> f64 {
        self.mix(|m| m.phong)
    }
    pub fn phong_size(&self) -> f64 {
        self.mix(|m| m.phong_size)
    }
    pub fn phong_color(&self) -> [f64; 3] {
        [self.mix(|m| m.phong_color[0]), self.mix(|m| m.phong_color[1]), self.mix(|m| m.phong_color[2])]
    }
    pub fn refl_map_intensity(&self) -> f64 {
        self.mix(|m| m.refl_map_intensity)
    }
    pub fn light_diff(&self, cosa: f64) -> f64 {
        let a = self.from.light_diff_func.evaluate(cosa);
        if self.t == 0.0 { a } else { a + (self.to.light_diff_func.evaluate(cosa) - a) * self.t }
    }
}

impl SolidSettings {
    /// True if hard shadows are rendered (`Flame.isWithShadows`).
    pub fn with_shadows(&self) -> bool {
        self.enabled && self.shadow_type != ShadowType::Off && self.lights.iter().any(|l| l.cast_shadows)
    }

    /// `getInterpolatedMaterial`; `None` for "receive only shadows" points (index < 0).
    pub fn material(&self, idx: f64) -> Option<MaterialRef<'_>> {
        if idx < 0.0 || self.materials.is_empty() {
            return None;
        }
        let n = self.materials.len();
        let from = idx as usize;
        if from >= n {
            let m = &self.materials[n - 1];
            return Some(MaterialRef { from: m, to: m, t: 0.0, exact: Some(n - 1) });
        }
        let to = if from + 1 >= n { 0 } else { from + 1 };
        let t = idx - idx.floor();
        const EPS: f64 = 0.01;
        Some(if t < EPS {
            MaterialRef { from: &self.materials[from], to: &self.materials[from], t: 0.0, exact: Some(from) }
        } else if t > 1.0 - EPS {
            MaterialRef { from: &self.materials[to], to: &self.materials[to], t: 0.0, exact: Some(to) }
        } else {
            MaterialRef { from: &self.materials[from], to: &self.materials[to], t, exact: None }
        })
    }

    /// Reads the `sld_render_*` / `post_bokeh_*` attributes and removes them from `attrs`.
    /// Like JWildfire, the details are only read if solid rendering is enabled.
    pub fn take_from_attrs(attrs: &mut Vec<(String, String)>) -> Self {
        let mut s = Self::default();
        let get = |attrs: &[(String, String)], k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.trim().to_string());
        let flag = |v: &str| v == "1" || v.eq_ignore_ascii_case("true");
        s.enabled = get(attrs, "sld_render_enabled").is_some_and(|v| flag(&v));
        if s.enabled {
            let f = |k: &str, d: &mut f64| {
                if let Some(v) = get(attrs, k).and_then(|v| v.parse().ok()) {
                    *d = v;
                }
            };
            let u = |k: &str, d: &mut u32| {
                if let Some(v) = get(attrs, k).and_then(|v| v.parse::<f64>().ok()) {
                    *d = v.max(0.0) as u32;
                }
            };
            if let Some(v) = get(attrs, "sld_render_ao_enabled") {
                s.ao_enabled = flag(&v);
            }
            f("sld_render_ao_intensity", &mut s.ao_intensity);
            f("sld_render_ao_search_radius", &mut s.ao_search_radius);
            f("sld_render_ao_blur_radius", &mut s.ao_blur_radius);
            u("sld_render_ao_radius_samples", &mut s.ao_radius_samples);
            u("sld_render_ao_azimuth_samples", &mut s.ao_azimuth_samples);
            f("sld_render_ao_falloff", &mut s.ao_falloff);
            f("sld_render_ao_affect_diffuse", &mut s.ao_affect_diffuse);
            if let Some(t) = get(attrs, "sld_render_shadow_type").and_then(|v| ShadowType::parse(&v)) {
                s.shadow_type = t;
            }
            f("sld_render_shadow_smooth_radius", &mut s.shadow_smooth_radius);
            u("sld_render_shadowmap_size", &mut s.shadowmap_size);
            f("sld_render_shadowmap_bias", &mut s.shadowmap_bias);
            if let Some(k) = get(attrs, "post_bokeh_filter_kernel").and_then(|v| FilterKernel::parse(&v)) {
                s.post_bokeh_filter_kernel = k;
            }
            f("post_bokeh_intensity", &mut s.post_bokeh_intensity);
            f("post_bokeh_brightness", &mut s.post_bokeh_brightness);
            f("post_bokeh_size", &mut s.post_bokeh_size);
            f("post_bokeh_activation", &mut s.post_bokeh_activation);
            if let Some(n) = get(attrs, "sld_render_material_count").and_then(|v| v.parse::<usize>().ok()) {
                s.materials = (0..n)
                    .map(|i| {
                        let mut m = Material::default();
                        f(&format!("sld_render_material_diffuse{i}"), &mut m.diffuse);
                        f(&format!("sld_render_material_ambient{i}"), &mut m.ambient);
                        f(&format!("sld_render_material_phong{i}"), &mut m.phong);
                        f(&format!("sld_render_material_phong_size{i}"), &mut m.phong_size);
                        f(&format!("sld_render_material_phong_red{i}"), &mut m.phong_color[0]);
                        f(&format!("sld_render_material_phong_green{i}"), &mut m.phong_color[1]);
                        f(&format!("sld_render_material_phong_blue{i}"), &mut m.phong_color[2]);
                        if let Some(v) = get(attrs, &format!("sld_render_material_light_diif_func{i}")).and_then(|v| LightDiffFunc::parse(&v)) {
                            m.light_diff_func = v;
                        }
                        f(&format!("sld_render_material_refl_map_intensity{i}"), &mut m.refl_map_intensity);
                        if let Some(v) = get(attrs, &format!("sld_render_material_refl_map_filename{i}")) {
                            m.refl_map_filename = v;
                        }
                        if let Some(v) = get(attrs, &format!("sld_render_material_refl_mappping{i}")).and_then(|v| ReflectionMapping::parse(&v)) {
                            m.reflection_mapping = v;
                        }
                        m
                    })
                    .collect();
            }
            if let Some(n) = get(attrs, "sld_render_ligtht_count").and_then(|v| v.parse::<usize>().ok()) {
                s.lights = (0..n)
                    .map(|i| {
                        let mut l = DistantLight::default();
                        f(&format!("sld_render_light_altitude{i}"), &mut l.altitude);
                        f(&format!("sld_render_light_azimuth{i}"), &mut l.azimuth);
                        f(&format!("sld_render_light_intensity{i}"), &mut l.intensity);
                        f(&format!("sld_render_light_shadow_intensity{i}"), &mut l.shadow_intensity);
                        f(&format!("sld_render_light_red{i}"), &mut l.color[0]);
                        f(&format!("sld_render_light_green{i}"), &mut l.color[1]);
                        f(&format!("sld_render_light_blue{i}"), &mut l.color[2]);
                        if let Some(v) = get(attrs, &format!("sld_render_light_shadows{i}")) {
                            l.cast_shadows = flag(&v);
                        }
                        l
                    })
                    .collect();
            }
        }
        // Light motion curves (`sld_render_light_altitude0_...`) stay as unknown attributes.
        attrs.retain(|(k, _)| !is_solid_attr(k) || is_light_curve_attr(k));
        s
    }

    /// Writes the attributes read by [`Self::take_from_attrs`] (only if enabled, as JWildfire).
    pub fn write_attrs(&self, mut attr: impl FnMut(&str, String)) {
        if !self.enabled {
            return;
        }
        let f = crate::io::fmt_f;
        let b = |v: bool| if v { "1" } else { "0" }.to_string();
        attr("sld_render_enabled", b(true));
        attr("sld_render_ao_enabled", b(self.ao_enabled));
        attr("sld_render_ao_intensity", f(self.ao_intensity));
        attr("sld_render_ao_search_radius", f(self.ao_search_radius));
        attr("sld_render_ao_blur_radius", f(self.ao_blur_radius));
        attr("sld_render_ao_radius_samples", self.ao_radius_samples.to_string());
        attr("sld_render_ao_azimuth_samples", self.ao_azimuth_samples.to_string());
        attr("sld_render_ao_falloff", f(self.ao_falloff));
        attr("sld_render_ao_affect_diffuse", f(self.ao_affect_diffuse));
        attr("sld_render_shadow_type", self.shadow_type.as_str().to_string());
        attr("sld_render_shadow_smooth_radius", f(self.shadow_smooth_radius));
        attr("sld_render_shadowmap_size", self.shadowmap_size.to_string());
        attr("sld_render_shadowmap_bias", f(self.shadowmap_bias));
        attr("post_bokeh_filter_kernel", self.post_bokeh_filter_kernel.as_str().to_string());
        attr("post_bokeh_intensity", f(self.post_bokeh_intensity));
        attr("post_bokeh_brightness", f(self.post_bokeh_brightness));
        attr("post_bokeh_size", f(self.post_bokeh_size));
        attr("post_bokeh_activation", f(self.post_bokeh_activation));
        attr("sld_render_material_count", self.materials.len().to_string());
        for (i, m) in self.materials.iter().enumerate() {
            attr(&format!("sld_render_material_diffuse{i}"), f(m.diffuse));
            attr(&format!("sld_render_material_ambient{i}"), f(m.ambient));
            attr(&format!("sld_render_material_phong{i}"), f(m.phong));
            attr(&format!("sld_render_material_phong_size{i}"), f(m.phong_size));
            attr(&format!("sld_render_material_phong_red{i}"), f(m.phong_color[0]));
            attr(&format!("sld_render_material_phong_green{i}"), f(m.phong_color[1]));
            attr(&format!("sld_render_material_phong_blue{i}"), f(m.phong_color[2]));
            attr(&format!("sld_render_material_refl_map_intensity{i}"), f(m.refl_map_intensity));
            attr(&format!("sld_render_material_refl_map_filename{i}"), m.refl_map_filename.clone());
            attr(&format!("sld_render_material_refl_mappping{i}"), m.reflection_mapping.as_str().to_string());
            attr(&format!("sld_render_material_light_diif_func{i}"), m.light_diff_func.as_str().to_string());
        }
        attr("sld_render_ligtht_count", self.lights.len().to_string());
        for (i, l) in self.lights.iter().enumerate() {
            attr(&format!("sld_render_light_altitude{i}"), f(l.altitude));
            attr(&format!("sld_render_light_azimuth{i}"), f(l.azimuth));
            attr(&format!("sld_render_light_intensity{i}"), f(l.intensity));
            attr(&format!("sld_render_light_red{i}"), f(l.color[0]));
            attr(&format!("sld_render_light_green{i}"), f(l.color[1]));
            attr(&format!("sld_render_light_blue{i}"), f(l.color[2]));
            attr(&format!("sld_render_light_shadows{i}"), b(l.cast_shadows));
            attr(&format!("sld_render_light_shadow_intensity{i}"), f(l.shadow_intensity));
        }
    }
}

fn is_solid_attr(k: &str) -> bool {
    k.starts_with("sld_render_") || k.starts_with("post_bokeh_")
}

/// `sld_render_light_altitude0_<curve attr>` / `sld_render_light_azimuth0_<curve attr>`
fn is_light_curve_attr(k: &str) -> bool {
    for p in ["sld_render_light_altitude", "sld_render_light_azimuth"] {
        if let Some(rest) = k.strip_prefix(p) {
            let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            return digits > 0 && rest[digits..].starts_with('_');
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attrs_roundtrip() {
        let mut s = SolidSettings { enabled: true, shadow_type: ShadowType::Smooth, ..Default::default() };
        s.materials.push(Material::new_added());
        s.lights[1].cast_shadows = true;
        let mut attrs = Vec::new();
        s.write_attrs(|k, v| attrs.push((k.to_string(), v)));
        attrs.push(("sld_render_light_altitude0_curve_points".into(), "3".into()));
        attrs.push(("brightness".into(), "4".into()));
        let back = SolidSettings::take_from_attrs(&mut attrs);
        assert_eq!(back, s);
        let names: Vec<&str> = attrs.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["sld_render_light_altitude0_curve_points", "brightness"]);
    }

    #[test]
    fn light_direction_matches_java() {
        // transpose(m) * (0, 0, -1) for altitude 90, azimuth 0.
        let l = DistantLight { altitude: 90.0, azimuth: 0.0, ..Default::default() };
        let (_, d) = l.projection();
        assert!(d[0].abs() < 1e-12 && (d[1] - 1.0).abs() < 1e-12 && d[2].abs() < 1e-12, "{d:?}");
    }
}

#[cfg(test)]
mod io_tests {
    #[test]
    fn jwildfire_light_curves_become_tracks() {
        let xml = r#"<flame sld_render_enabled="1" sld_render_ligtht_count="1" sld_render_light_altitude0="30"
            sld_render_light_altitude0_enabled="true" sld_render_light_altitude0_point_count="2"
            sld_render_light_altitude0_x0="0" sld_render_light_altitude0_y0="10"
            sld_render_light_altitude0_x1="100" sld_render_light_altitude0_y1="80"
            sld_render_light_altitude0_interpolation="LINEAR">
            <xform weight="1" linear="1" /></flame>"#;
        let f = crate::io::read_flames(xml).unwrap().remove(0);
        assert!(f.extra_attrs.iter().all(|(k, _)| !k.starts_with("sld_render")), "{:?}", f.extra_attrs);
        assert_eq!(f.solid.lights[0].altitude, 30.0);
        assert!((f.at_frame(50.0).solid.lights[0].altitude - 45.0).abs() < 1e-9);
    }
}
