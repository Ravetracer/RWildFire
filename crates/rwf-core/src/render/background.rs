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

//! Background xforms (`LogDensityFilter.calculateBGColor`): every output pixel is mapped into a
//! fixed coordinate frame, sent through each background xform of each layer and the resulting
//! colors, scaled by the xform weights, are added to the background color.

use rayon::prelude::*;

use super::prepared::{PreparedXForm, colormap_index, prepare_xform};
use super::tonemap::{background_color, background_image_color};
use crate::resources::ImageMap;
use std::sync::Arc;
use crate::math::EPSILON;
use crate::model::{ColorType, Flame, VariationInstance, XForm};
use crate::variations::{Ctx, Pt};

/// `DC_BaseFunc` subclasses: `prepareBackgroundRendering` sets `ColorOnly = 1`.
const DC_COLOR_ONLY: &[&str] = &[
    "dc_acrilic",
    "dc_apollonian",
    "dc_booleans",
    "dc_cairotiles",
    "dc_circlesblue",
    "dc_circuits",
    "dc_fingerprint",
    "dc_fractaldots",
    "dc_fractcolor",
    "dc_gabornoise",
    "dc_glypho",
    "dc_gmandelbroot",
    "dc_grid3D",
    "dc_hexagons",
    "dc_hoshi",
    "dc_hyperbolictile",
    "dc_inversion",
    "dc_kaleidocomplex",
    "dc_kaleidoscopic",
    "dc_kaliset",
    "dc_kaliset2",
    "dc_layers",
    "dc_mandala",
    "dc_mandelbox2D",
    "dc_menger",
    "dc_moebiuslog",
    "dc_pentatiles",
    "dc_poincaredisc",
    "dc_portal",
    "dc_quadtree",
    "dc_randomoctree",
    "dc_rotations",
    "dc_spacefold",
    "dc_squares",
    "dc_starsfield",
    "dc_sunflower",
    "dc_tesla",
    "dc_tree",
    "dc_triantess",
    "dc_truchet",
    "dc_turbulence",
    "dc_voronoise",
    "dc_vortex",
    "dc_warping",
    "dc_worley",
];

/// `AbstractFractWFFunc` subclasses: `color_only = 1`, `buddhabrot_mode = 0`.
const FRACT_COLOR_ONLY: &[&str] =
    &["fract_dragon_wf", "fract_julia_wf", "fract_mandelbrot_wf", "fract_meteors_wf", "fract_pearls_wf", "fract_salamander_wf"];

/// `SupportsBackground.prepareBackgroundRendering`: switches variations into their color-only
/// mode, which samples the pattern at the input point.
fn prepare_for_background(var: &mut VariationInstance) {
    let name = var.name.as_str();
    if DC_COLOR_ONLY.contains(&name) {
        var.set_param("ColorOnly", 1.0);
    } else if name == "dc_perlin" {
        var.set_param("color_only", 1.0);
        var.set_param("shape", 0.0);
    } else if FRACT_COLOR_ONLY.contains(&name) {
        var.set_param("color_only", 1.0);
        var.set_param("buddhabrot_mode", 0.0);
    }
}

/// Variations with a color-only mode for backgrounds (`VARTYPE_SUPPORTS_BACKGROUND`).
pub fn background_variations() -> Vec<&'static str> {
    DC_COLOR_ONLY.iter().chain(&["dc_perlin"]).chain(FRACT_COLOR_ONLY).copied().filter(|n| crate::variations::find(n).is_some()).collect()
}

/// A new background xform with a random background variation (`TinaController.addBGForm`,
/// without the affine mutation).
pub fn random_bg_xform(rng: &mut crate::rng::Rng) -> XForm {
    let names = background_variations();
    let mut xf = XForm::with_variation(names[rng.random_int(names.len())], 0.5 + rng.random());
    xf.weight = 0.5;
    xf.color_symmetry = 1.0;
    xf.color_type = ColorType::None;
    xf
}

struct BgLayer {
    xforms: Vec<(PreparedXForm, f64)>,
    colormap: Vec<[f64; 3]>,
    smooth: bool,
}

/// Background color of every output pixel: the flame background (single color or gradient)
/// plus the contributions of the background xforms.
pub struct Backdrop<'a> {
    flame: &'a Flame,
    width: u32,
    height: u32,
    /// Precomputed colors (row major), only with background xforms.
    colors: Option<Vec<[i32; 3]>>,
}

/// The flame's background image, if it has one that can be loaded.
pub fn background_image(flame: &Flame) -> Option<Arc<ImageMap>> {
    crate::resources::image(&flame.bg_image).filter(|i| i.width >= 2 && i.height >= 2)
}

/// True if the background needs colors per pixel (background xforms or an image).
pub fn needs_backdrop(flame: &Flame) -> bool {
    flame.has_bg_xforms() || !flame.bg_image.is_empty()
}

impl<'a> Backdrop<'a> {
    pub fn new(flame: &'a Flame, width: u32, height: u32) -> Self {
        Self::new_region(flame, (width, height), (0, 0, width, height))
    }

    /// The colors of the `w` × `h` pixels at `(x0, y0)` of a `full_w` × `full_h` image (a
    /// quilt tile). Only precomputes with background xforms; [`Backdrop::get`] is then
    /// relative to the region.
    pub fn new_region(flame: &'a Flame, full: (u32, u32), region: (u32, u32, u32, u32)) -> Self {
        let image = background_image(flame);
        let colors = (flame.has_bg_xforms() || image.is_some()).then(|| render_bg_xforms(flame, image.as_deref(), full, region));
        Self { flame, width: region.2, height: region.3, colors }
    }

    #[inline]
    pub fn get(&self, x: u32, y: u32) -> [i32; 3] {
        match &self.colors {
            Some(c) => c[(y * self.width + x) as usize],
            None => background_color(self.flame, x, y, self.width, self.height),
        }
    }

    /// The precomputed colors if the flame has background xforms or an image.
    pub fn colors(&self) -> Option<&[[i32; 3]]> {
        self.colors.as_deref()
    }
}

/// Colors of the region `(x0, y0, rw, rh)` of a `full` sized image: the background color or
/// image plus the background xforms.
fn render_bg_xforms(flame: &Flame, image: Option<&ImageMap>, full: (u32, u32), region: (u32, u32, u32, u32)) -> Vec<[i32; 3]> {
    let (w, h) = full;
    let (x0, y0, rw, rh) = region;
    let mut missing = Vec::new();
    let layers: Vec<BgLayer> = flame
        .layers
        .iter()
        .filter(|l| !l.bg_xforms.is_empty())
        .map(|l| {
            let colormap = l.render_gradient().render_colors();
            let gradient_map = super::prepared::PreparedGradientMap::new(&l.gradient_map);
            let xforms = l
                .bg_xforms
                .iter()
                .map(|xf| {
                    let mut xf = xf.clone();
                    xf.variations.iter_mut().for_each(prepare_for_background);
                    // Unlike the other xform lists, an unset color type is kept (no color step).
                    let mut p = prepare_xform(&xf, xf.color_type, &colormap, flame.frame, &mut missing);
                    p.gradient_map = gradient_map.clone();
                    (p, xf.weight)
                })
                .collect();
            BgLayer { xforms, colormap, smooth: l.smooth_gradient }
        })
        .collect();

    let img_size = ((w as f64) * (w as f64) + (h as f64) * (h as f64)).sqrt() * 0.5;
    let coord_scale = 1.41 / flame.cam_zoom;
    let mut out = vec![[0i32; 3]; rw as usize * rh as usize];
    out.par_chunks_mut(rw as usize).enumerate().for_each(|(ry, row)| {
        let py = ry + y0 as usize;
        let mut ctx = Ctx::new(0x6B67_7866 ^ py as u64, flame.preserve_z);
        let y = (py as f64 - (h as f64 - 1.0) * 0.5) / img_size * coord_scale;
        for (rx, dst) in row.iter_mut().enumerate() {
            let px = rx + x0 as usize;
            let x = (px as f64 - (w as f64 - 1.0) * 0.5) / img_size * coord_scale;
            let mut c = match image {
                Some(img) => background_image_color(img, px as u32, py as u32, w, h),
                None => background_color(flame, px as u32, py as u32, w, h),
            };
            for layer in &layers {
                let p = Pt { x, y, ..Pt::default() };
                for (xf, weight) in &layer.xforms {
                    let (q, wmv) = xf.transform_wmv(&mut ctx, &layer.colormap, layer.smooth, &p);
                    let vai = xf.weighting.as_ref().map_or(0.0, |w| w.var_amount_intensity);
                    let amount = if wmv.abs() > EPSILON && vai.abs() > EPSILON { weight * (1.0 + wmv * vai) } else { *weight };
                    let rgb = if q.rgb_color { [q.red, q.green, q.blue] } else { layer.colormap[colormap_index(&layer.colormap, q.color)] };
                    // Java `int += double`: the sum is truncated towards zero.
                    for i in 0..3 {
                        c[i] = (c[i] as f64 + rgb[i] * amount) as i32;
                    }
                }
            }
            *dst = c;
        }
    });
    out
}

/// True if `a` and `b` have the same background colors (for caching a [`Backdrop`]).
pub fn same_backdrop(a: &Flame, b: &Flame) -> bool {
    a.background == b.background
        && a.bg_image == b.bg_image
        && a.has_bg_xforms() == b.has_bg_xforms()
        && (!a.has_bg_xforms()
            || (a.cam_zoom == b.cam_zoom
                && a.frame == b.frame
                && a.preserve_z == b.preserve_z
                && a.layers.len() == b.layers.len()
                && a.layers.iter().zip(&b.layers).all(|(la, lb)| {
                    la.bg_xforms == lb.bg_xforms
                        && (la.bg_xforms.is_empty()
                            || (la.gradient == lb.gradient && la.gradient_shift == lb.gradient_shift && la.smooth_gradient == lb.smooth_gradient && la.gradient_map == lb.gradient_map))
                })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layer, XForm};

    fn flame_with_bg(xf: XForm) -> Flame {
        let mut flame = Flame::default();
        flame.background.kind = crate::model::BgColorType::SingleColor;
        flame.background.color = [10, 20, 30];
        let mut layer = Layer::default();
        layer.bg_xforms.push(xf);
        flame.layers = vec![layer];
        flame
    }

    #[test]
    fn linear_bg_xform_adds_the_weighted_palette_color() {
        let mut xf = XForm::with_variation("linear", 1.0);
        xf.weight = 0.5;
        let flame = flame_with_bg(xf);
        let c0 = flame.layers[0].gradient.render_colors()[0];
        let b = Backdrop::new(&flame, 8, 6);
        let expect = [10.0 + c0[0] * 0.5, 20.0 + c0[1] * 0.5, 30.0 + c0[2] * 0.5].map(|v| v as i32);
        assert_eq!(b.get(3, 2), expect);
        assert!(same_backdrop(&flame, &flame.clone()));
        let mut other = flame.clone();
        other.layers[0].bg_xforms[0].weight = 0.7;
        assert!(!same_backdrop(&flame, &other));
    }

    #[test]
    fn background_image_is_stretched_bilinearly() {
        let img = ImageMap { width: 2, height: 2, pixels: vec![0x000000, 0xFF0000, 0x0000FF, 0xFFFFFF], alpha: vec![255; 4] };
        // same size: the pixels themselves
        assert_eq!(background_image_color(&img, 1, 0, 2, 2), [255, 0, 0]);
        // 3 x 3: the middle pixel blends all four corners
        assert_eq!(background_image_color(&img, 1, 1, 3, 3), [128, 64, 128]);
        assert_eq!(background_image_color(&img, 2, 2, 3, 3), [255, 255, 255]);
        let mut flame = Flame { bg_image: "/no/such/image.png".into(), ..Flame::default() };
        assert!(needs_backdrop(&flame) && Backdrop::new(&flame, 4, 4).colors().is_none(), "a missing image keeps the plain background");
        flame.bg_image.clear();
        let back = crate::io::read_flames(&crate::io::write_flame(&Flame { bg_image: "bg.png".into(), ..flame })).unwrap();
        assert_eq!(back[0].bg_image, "bg.png");
    }

    #[test]
    fn bg_xforms_survive_a_file_round_trip() {
        let flame = flame_with_bg(XForm::with_variation("dc_hexagons", 0.8));
        let text = crate::io::write_flame(&flame);
        assert!(text.contains("<jwf-flame") && text.contains("<bgxform"));
        let back = crate::io::read_flames(&text).unwrap().remove(0);
        assert_eq!(back.layers[0].bg_xforms.len(), 1);
        assert_eq!(back.layers[0].bg_xforms[0].variations[0].name, "dc_hexagons");
    }
}
