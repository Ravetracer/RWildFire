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

//! Solid rendering: z-buffer accumulation, normals, ambient occlusion, hard shadows, shading
//! and post DOF. Port of `RasterFloatIntForSolidRendering`, `NormalsCalculator`,
//! `AOCalculator`, `ShadowCalculator`, `LogDensityFilter.addSolidColors`, the solid branch of
//! `GammaCorrectionFilter` and `PostDOFCalculator`.

use std::sync::atomic::{AtomicI64, Ordering};

use rayon::prelude::*;

use super::filter::{FilterKernel, FilterMatrix};
use crate::math::{EPSILON, ftoi, hsl_to_rgb, rgb_to_hsl, round_color};
use crate::model::Flame;
use crate::rng::Rng;
use crate::solid::{ReflectionMapping, ShadowType, SolidSettings};

/// Marker for empty z-buffer / normal cells (`NormalsCalculator.ZBUF_ZMIN`).
pub const ZMIN: f32 = -f32::MAX;

/// Seed of the warm-up iteration that places the shadow maps (fixed, so the CPU and GPU
/// renderers and repeated renders agree).
pub const SHADOW_SEED: u64 = 42;

/// Samples used to place a light's shadow map (`ShadowCalculator.PRE_SHADOWMAP_SIZE`).
pub const SHADOW_MAP_PRE_SAMPLES: usize = 40960;

/// Light projections and directions (`LightViewCalculator`).
#[derive(Clone, Debug)]
pub struct LightViews {
    /// Projection matrix of every light that casts shadows (if shadows are on).
    pub proj: Vec<Option<[[f64; 3]; 3]>>,
    pub dir: Vec<[f64; 3]>,
}

impl LightViews {
    pub fn new(s: &SolidSettings) -> Self {
        let shadows = s.with_shadows();
        let (proj, dir) = s
            .lights
            .iter()
            .map(|l| {
                let (m, d) = l.projection();
                ((shadows && l.cast_shadows).then_some(m), d)
            })
            .unzip();
        Self { proj, dir }
    }

    #[inline]
    pub fn project(m: &[[f64; 3]; 3], x: f64, y: f64, z: f64) -> [f64; 3] {
        [m[0][0] * x + m[0][1] * y + m[0][2] * z, m[1][0] * x + m[1][1] * y + m[1][2] * z, m[2][0] * x + m[2][1] * y + m[2][2] * z]
    }
}

/// Placement of a shadow map in light space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShadowFrame {
    pub scale_x: f64,
    pub scale_y: f64,
    pub centre_x: f64,
    pub centre_y: f64,
}

impl ShadowFrame {
    /// Fits a `size`² map around light-space points (`addShadowMapSamples`, 3 % margin).
    pub fn fit(points: &[[f32; 3]], size: usize) -> Self {
        let (mut xmin, mut xmax, mut ymin, mut ymax) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for p in points {
            xmin = xmin.min(p[0] as f64);
            xmax = xmax.max(p[0] as f64);
            ymin = ymin.min(p[1] as f64);
            ymax = ymax.max(p[1] as f64);
        }
        if points.is_empty() {
            (xmin, xmax, ymin, ymax) = (-1.0, 1.0, -1.0, 1.0);
        }
        let safety = 0.03;
        let (dx, dy) = (xmax - xmin, ymax - ymin);
        xmin -= safety * dx;
        xmax += safety * dx;
        ymin -= safety * dy;
        ymax += safety * dy;
        let dx = (xmax - xmin).max(0.001);
        let dy = (ymax - ymin).max(0.001);
        let scale_x = size as f64 / dx;
        let scale_y = size as f64 / dy;
        Self { scale_x, scale_y, centre_x: -xmin * scale_x, centre_y: -ymin * scale_y }
    }

    #[inline]
    pub fn map(&self, x: f64, y: f64) -> (i64, i64) {
        ((self.scale_x * x + self.centre_x + 0.5) as i64, (self.scale_y * y + self.centre_y + 0.5) as i64)
    }
}

/// Shadow map size in texels (`ShadowCalculator`).
pub fn shadow_map_size(flame: &Flame, ppu_scale: f64) -> usize {
    ftoi(flame.solid.shadowmap_size as f64 * ppu_scale).max(64) as usize
}

/// One plotted point.
#[derive(Clone, Copy, Debug)]
pub struct SolidSample {
    pub x: u32,
    pub y: u32,
    pub z: f32,
    pub origin: [f32; 3],
    pub color: [f32; 3],
    /// Material index, -1 for "receive only shadows".
    pub material: f32,
    pub dof: f32,
}

/// A point projected into a light's view.
#[derive(Clone, Copy, Debug)]
pub struct LightSample {
    pub light: u32,
    pub p: [f32; 3],
}

#[derive(Clone)]
pub struct ShadowMap {
    pub size: usize,
    pub frame: ShadowFrame,
    /// `z[x * size + y]`, `ZMIN` = empty.
    pub z: Vec<f32>,
}

impl ShadowMap {
    #[inline]
    pub fn add(&mut self, p: [f32; 3]) {
        let (x, y) = self.frame.map(p[0] as f64, p[1] as f64);
        if x >= 0 && (x as usize) < self.size && y >= 0 && (y as usize) < self.size {
            let i = x as usize * self.size + y as usize;
            if p[2] > self.z[i] {
                self.z[i] = p[2];
            }
        }
    }
}

/// Accumulation buffers of the solid raster.
#[derive(Clone)]
pub struct SolidAccum {
    pub width: u32,
    pub height: u32,
    pub z: Vec<f32>,
    pub origin: Vec<[f32; 3]>,
    pub color: Vec<[f32; 3]>,
    pub material: Vec<f32>,
    pub dof: Vec<f32>,
    pub shadow_maps: Vec<Option<ShadowMap>>,
}

impl SolidAccum {
    pub fn new(width: u32, height: u32, frames: &[Option<ShadowFrame>], map_size: usize) -> Self {
        let n = width as usize * height as usize;
        Self {
            width,
            height,
            z: vec![ZMIN; n],
            origin: vec![[ZMIN; 3]; n],
            color: vec![[0.0; 3]; n],
            material: vec![0.0; n],
            dof: vec![0.0; n],
            shadow_maps: frames.iter().map(|f| f.map(|frame| ShadowMap { size: map_size, frame, z: vec![ZMIN; map_size * map_size] })).collect(),
        }
    }

    /// `RasterFloatIntForSolidRendering.addSamples`
    pub fn add(&mut self, samples: &[SolidSample]) {
        for s in samples {
            let i = s.y as usize * self.width as usize + s.x as usize;
            if s.z > self.z[i] {
                self.z[i] = s.z;
                self.origin[i] = s.origin;
                self.color[i] = if s.color.iter().all(|c| *c >= 0.0) { s.color } else { [0.0; 3] };
                self.material[i] = s.material;
                self.dof[i] = s.dof;
            }
        }
    }

    pub fn add_light(&mut self, samples: &[LightSample]) {
        for s in samples {
            if let Some(Some(m)) = self.shadow_maps.get_mut(s.light as usize) {
                m.add(s.p);
            }
        }
    }
}

/// Finalized per-cell data, ready for shading.
pub struct SolidRaster {
    pub width: u32,
    pub height: u32,
    pub normal: Vec<[f32; 3]>,
    pub color: Vec<[f32; 3]>,
    pub material: Vec<f32>,
    /// `|dof * dofAmount|`
    pub dof: Vec<f32>,
    pub ao: Vec<f32>,
    /// Per light visibility (0..1); empty without shadows.
    pub visibility: Vec<Vec<f32>>,
}

const NEIGHBOURS: [((i32, i32), (i32, i32)); 16] = [
    ((0, 1), (1, 0)),
    ((1, 0), (0, -1)),
    ((0, -1), (-1, 0)),
    ((-1, 0), (0, 1)),
    ((-1, 1), (1, 1)),
    ((1, 1), (1, -1)),
    ((1, -1), (-1, -1)),
    ((-1, -1), (-1, 1)),
    ((0, 1), (1, 1)),
    ((1, 1), (1, 0)),
    ((1, 0), (1, -1)),
    ((1, -1), (0, -1)),
    ((0, -1), (-1, -1)),
    ((-1, -1), (-1, 0)),
    ((-1, 0), (-1, 1)),
    ((-1, 1), (0, 1)),
];

/// `NormalsCalculator.refreshNormalAtLocation` with 8 samples.
fn normal_at(acc: &SolidAccum, x: i32, y: i32) -> [f32; 3] {
    let (w, h) = (acc.width as i32, acc.height as i32);
    let o = |x: i32, y: i32| acc.origin[y as usize * w as usize + x as usize];
    let c = o(x, y);
    if c[2] <= ZMIN {
        return [ZMIN; 3];
    }
    let (xb, yb, zb) = (c[0] as f64, c[1] as f64, c[2] as f64);
    let inside = |dx: i32, dy: i32| x + dx >= 0 && x + dx < w && y + dy >= 0 && y + dy < h;
    let (mut nx, mut ny, mut nz) = (0.0, 0.0, 0.0);
    let mut samples = 0;
    for &((adx, ady), (bdx, bdy)) in &NEIGHBOURS {
        if inside(adx, ady) && inside(bdx, bdy) {
            let a = o(x + adx, y + ady);
            if a[2] > ZMIN {
                let b = o(x + bdx, y + bdy);
                if b[2] > ZMIN {
                    let (ax, ay, az) = (xb - a[0] as f64, yb - a[1] as f64, zb - a[2] as f64);
                    let (bx, by, bz) = (xb - b[0] as f64, yb - b[1] as f64, zb - b[2] as f64);
                    samples += 1;
                    nx += ay * bz - az * by;
                    ny += az * bx - ax * bz;
                    nz += ax * by - ay * bx;
                }
            }
        }
        if samples >= 8 {
            break;
        }
    }
    let r = (nx * nx + ny * ny + nz * nz).sqrt();
    if samples > 0 && r > EPSILON { [(nx / r) as f32, (ny / r) as f32, (nz / r) as f32] } else { [ZMIN; 3] }
}

/// `AOCalculator.refreshAO`
fn ambient_occlusion(acc: &SolidAccum, normal: &[[f32; 3]], s: &SolidSettings) -> Vec<f32> {
    let (w, h) = (acc.width as usize, acc.height as usize);
    let search = s.ao_search_radius.clamp(0.25, 120.0);
    let blur = s.ao_blur_radius.clamp(0.25, 10.0);
    let falloff = s.ao_falloff.clamp(0.0, 10.0);
    let radius_samples = s.ao_radius_samples.clamp(1, 128) as usize;
    let azimuth_samples = s.ao_azimuth_samples.clamp(1, 128) as usize;
    let img_size = ((w * w + h * h) as f64).sqrt();
    let sphere_radius = search * img_size / 500.0;
    let radius_step = sphere_radius / radius_samples as f64;
    let azimuth_step = std::f64::consts::TAU / azimuth_samples as f64;
    let r_jitter = radius_step / 10.0;
    let get_z = |x: f64, y: f64| {
        let xi = (ftoi(x) as i64).clamp(0, w as i64 - 1) as usize;
        let yi = (ftoi(y) as i64).clamp(0, h as i64 - 1) as usize;
        acc.z[yi * w + xi] as f64
    };
    let mut ao = vec![0.0f32; w * h];
    ao.par_chunks_mut(w).enumerate().for_each(|(i, row)| {
        let mut rng = Rng::new(32851137 ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        for (j, out) in row.iter_mut().enumerate() {
            let idx = i * w + j;
            let z0 = acc.z[idx] as f64;
            let n = normal[idx];
            if acc.z[idx] <= ZMIN || n[0] <= ZMIN {
                continue;
            }
            let (nx, ny, nz) = (n[0] as f64, n[1] as f64, n[2] as f64);
            let (x0, y0) = (j as f64, i as f64);
            let mut sum = 0.0;
            let mut angle = (0.5 - rng.random()) * azimuth_step / 4.0;
            for _ in 0..azimuth_samples {
                let (dy, dx) = angle.sin_cos();
                let mut r0 = radius_step;
                let mut prev_h = 0.0;
                for _ in 0..radius_samples {
                    let r = r0 + (0.5 - rng.random()) * r_jitter;
                    let x = x0 + r * dx + (0.5 - rng.random()) * r_jitter;
                    let y = y0 + r * dy + (0.5 - rng.random()) * r_jitter;
                    let hz = (get_z(x, y) - z0).atan2(r) + 0.001;
                    let rr = (x * x + y * y).sqrt();
                    let (rx, ry) = (x / rr, y / rr);
                    let tt = nx * rx + ry * ny;
                    let tx = -(ry * nx * ny - rx * ny * ny - rx * nz * nz) / tt;
                    let ty = (ry * nx * nx + ry * nz * nz - nx * rx * ny) / tt;
                    let tz = (-ry * ny * nz - nx * rx * nz) / tt;
                    let ta = tz.atan2((tx * tx + ty * ty).sqrt());
                    let a = hz.sin() - ta.sin();
                    if a > prev_h {
                        prev_h = a;
                        let dist = r / sphere_radius;
                        sum += a * (-dist * dist * falloff).exp() * 0.5;
                    }
                    r0 += radius_step;
                }
                angle += azimuth_step;
            }
            *out = sum as f32;
        }
    });
    let blur_radius = blur * img_size / 500.0;
    if blur_radius < 0.42 {
        return ao;
    }
    // SmoothAOBufferThread: Gaussian blur, scaled by 0.1.
    let f = FilterMatrix::new(FilterKernel::Gaussian, 1, blur_radius);
    let (size, center) = (f.size as i64, f.size as i64 / 2);
    let mut out = vec![0.0f32; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(i, row)| {
        for (j, o) in row.iter_mut().enumerate() {
            let mut v = 0.0;
            for k in 0..size {
                let px = j as i64 + k - center;
                if px < 0 || px >= w as i64 {
                    continue;
                }
                for l in 0..size {
                    let py = i as i64 + l - center;
                    if py >= 0 && py < h as i64 {
                        v += ao[py as usize * w + px as usize] as f64 * f.at(k as usize, l as usize);
                    }
                }
            }
            *o = (v * 0.1) as f32;
        }
    });
    out
}

/// `ShadowCalculator`: per light and cell visibility (0..1), shadow intensity included.
fn shadow_visibility(acc: &SolidAccum, flame: &Flame, lights: &LightViews, img_size: f64) -> Vec<Vec<f32>> {
    let s = &flame.solid;
    let (w, h) = (acc.width as usize, acc.height as usize);
    let raw_smooth = if s.shadow_smooth_radius < EPSILON { 0.0 } else { s.shadow_smooth_radius };
    let smooth_radius = (ftoi(raw_smooth * 6.0 * img_size / 1000.0) as i64).min(128);
    let smooth = s.shadow_type == ShadowType::Smooth && smooth_radius >= 1;
    let kernel: Vec<f64> = if smooth {
        let inv = 1.0 / smooth_radius as f64 * FilterKernel::Gaussian.spatial_support();
        let n = 2 * smooth_radius + 1;
        let mut k = vec![0.0; (n * n) as usize];
        for a in -smooth_radius..=smooth_radius {
            for b in -smooth_radius..=smooth_radius {
                let r = ((a as f64 * inv).powi(2) + (b as f64 * inv).powi(2)).sqrt();
                k[((a + smooth_radius) * n + b + smooth_radius) as usize] = FilterKernel::Gaussian.coeff(r);
            }
        }
        k
    } else {
        Vec::new()
    };
    let bias = s.shadowmap_bias;
    let mut out = Vec::with_capacity(s.lights.len());
    for (li, light) in s.lights.iter().enumerate() {
        let (Some(Some(map)), Some(Some(m))) = (acc.shadow_maps.get(li), lights.proj.get(li)) else {
            out.push(vec![1.0f32; w * h]);
            continue;
        };
        // accelerateShadows
        let accel: Vec<f32> = (0..w * h)
            .into_par_iter()
            .map(|i| {
                let o = acc.origin[i];
                if o[0] <= ZMIN || o[1] <= ZMIN || o[2] <= ZMIN {
                    return ZMIN;
                }
                let p = LightViews::project(m, o[0] as f64, o[1] as f64, o[2] as f64);
                let (x, y) = map.frame.map(p[0], p[1]);
                if x < 0 || y < 0 || x as usize >= map.size || y as usize >= map.size {
                    return ZMIN;
                }
                let edge = map.z[x as usize * map.size + y as usize] as f64 - bias;
                if p[2] < edge { 0.0 } else { 1.0 }
            })
            .collect();
        let shadow_int = 1.0 - light.shadow_intensity.clamp(0.0, 1.0);
        let vis: Vec<f32> = (0..w * h)
            .into_par_iter()
            .map(|i| {
                let (px, py) = ((i % w) as i64, (i / w) as i64);
                let mut v = 1.0;
                if smooth {
                    // calcSmoothShadowIntensity (starts at 1.0, as in JWildfire)
                    let mut total = 0.0;
                    let dl = (smooth_radius / 8 + 1) as usize;
                    let n = 2 * smooth_radius + 1;
                    for k in (-smooth_radius..=smooth_radius).step_by(dl) {
                        let x = px + k;
                        if x < 0 || x >= w as i64 {
                            continue;
                        }
                        for l in (-smooth_radius..=smooth_radius).step_by(dl) {
                            let y = py + l;
                            if y >= 0 && y < h as i64 {
                                let a = accel[y as usize * w + x as usize];
                                if a > ZMIN {
                                    let f = kernel[((k + smooth_radius) * n + l + smooth_radius) as usize];
                                    total += f;
                                    v += a as f64 * f;
                                }
                            }
                        }
                    }
                    if total > EPSILON {
                        v /= total;
                    }
                } else if accel[i] > ZMIN {
                    v = accel[i] as f64;
                }
                (v + shadow_int).clamp(0.0, 1.0) as f32
            })
            .collect();
        out.push(vis);
    }
    out
}

/// Builds the shading inputs from the accumulation buffers (`finalizeRaster`).
pub fn finalize(acc: &SolidAccum, flame: &Flame, lights: &LightViews) -> SolidRaster {
    let (w, h) = (acc.width as usize, acc.height as usize);
    let normal: Vec<[f32; 3]> = (0..w * h).into_par_iter().map(|i| normal_at(acc, (i % w) as i32, (i / w) as i32)).collect();
    let ao = if flame.solid.ao_enabled { ambient_occlusion(acc, &normal, &flame.solid) } else { vec![0.0; w * h] };
    // JWildfire allocates the solid raster with the raster size, so "imgSize" is its diagonal.
    let img_size = ((w as f64).powi(2) + (h as f64).powi(2)).sqrt();
    let visibility = if flame.solid.with_shadows() { shadow_visibility(acc, flame, lights, img_size) } else { Vec::new() };
    let dof_amount = if flame.cam_dof > EPSILON { flame.cam_dof * img_size / (1000.0 * flame.spatial_oversampling.max(1) as f64) } else { 0.0 };
    SolidRaster {
        width: acc.width,
        height: acc.height,
        normal,
        color: acc.color.clone(),
        material: acc.material.clone(),
        dof: acc.dof.iter().map(|d| (*d as f64 * dof_amount).abs() as f32).collect(),
        ao,
        visibility,
    }
}

/// An RGB image used as a reflection map.
pub struct ReflectionMap {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<[f64; 3]>,
}

impl ReflectionMap {
    /// Loads a PNG or JPEG reflection map.
    pub fn load(path: &str) -> Option<Self> {
        let img = image::open(path).ok()?.into_rgb8();
        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return None;
        }
        let rgb = img.pixels().map(|p| [p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0]).collect();
        Some(Self { width, height, rgb })
    }

    /// `UVPairD.getColorFromMap`
    fn color(&self, u: f64, v: f64) -> [f64; 3] {
        let x = ((self.width as f64 * u) as i64).clamp(0, self.width as i64 - 1) as usize;
        let y = ((self.height as f64 * v) as i64).clamp(0, self.height as i64 - 1) as usize;
        self.rgb[y * self.width as usize + x]
    }
}

#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn reflect(d: [f64; 3], n: [f64; 3]) -> [f64; 3] {
    let dn = dot(d, n);
    [d[0] - 2.0 * dn * n[0], d[1] - 2.0 * dn * n[1], d[2] - 2.0 * dn * n[2]]
}

/// Shading of solid raster cells into RGBA8 (`addSolidColors` + gamma correction).
pub struct SolidShader<'a> {
    pub flame: &'a Flame,
    pub raster: &'a SolidRaster,
    pub lights: &'a LightViews,
    pub filter: FilterMatrix,
    pub oversample: u32,
    pub border: u32,
    pub with_alpha: bool,
    pub refl_maps: Vec<Option<ReflectionMap>>,
}

impl<'a> SolidShader<'a> {
    pub fn new(flame: &'a Flame, raster: &'a SolidRaster, lights: &'a LightViews, border: u32, with_alpha: bool) -> Self {
        let oversample = flame.spatial_oversampling.max(1);
        let refl_maps = flame
            .solid
            .materials
            .iter()
            .map(|m| if m.refl_map_intensity > EPSILON && !m.refl_map_filename.is_empty() { ReflectionMap::load(&m.refl_map_filename) } else { None })
            .collect();
        Self {
            flame,
            raster,
            lights,
            filter: FilterMatrix::new(flame.filter_kernel, oversample, flame.spatial_filter_radius),
            oversample,
            border,
            with_alpha,
            refl_maps,
        }
    }

    /// `addSolidColors`: adds the lit color of a raster cell; false if it has no surface.
    fn add_cell(&self, i: usize, scale: f64, bg: [i32; 3], acc: &mut [f64; 3]) -> bool {
        let r = self.raster;
        let n = r.normal[i];
        if n[0] <= ZMIN {
            return false;
        }
        let s = &self.flame.solid;
        let has_shadows = !r.visibility.is_empty();
        let avg_vis = if has_shadows && !s.lights.is_empty() {
            r.visibility.iter().map(|v| v[i] as f64).sum::<f64>() / s.lights.len() as f64
        } else {
            1.0
        };
        let vis = |li: usize| if s.lights[li].cast_shadows && has_shadows { r.visibility[li][i] as f64 } else { avg_vis };
        let mut raw = [0.0; 3];
        match s.material(r.material[i] as f64) {
            None => {
                let v = (0..s.lights.len()).map(vis).sum::<f64>().clamp(0.0, 1.0);
                raw = [bg[0] as f64 / 255.0 * v, bg[1] as f64 / 255.0 * v, bg[2] as f64 / 255.0 * v];
            }
            Some(mat) => {
                let ao = r.ao[i] as f64;
                let ao_int = s.ao_intensity.clamp(0.0, 4.0);
                let ambient = (if s.ao_enabled { mat.ambient() - ao * ao_int } else { mat.ambient() }).max(0.0);
                let diffuse = (if s.ao_enabled { mat.diffuse() - ao * ao_int * s.ao_affect_diffuse } else { mat.diffuse() }).max(0.0);
                let specular = mat.phong();
                let bal = [self.flame.balance_red, self.flame.balance_green, self.flame.balance_blue];
                let c = r.color[i];
                let obj = [c[0] as f64 * bal[0] / 255.0, c[1] as f64 * bal[1] / 255.0, c[2] as f64 * bal[2] / 255.0];
                for k in 0..3 {
                    raw[k] = obj[k] * ambient * avg_vis;
                }
                let normal = [n[0] as f64, n[1] as f64, n[2] as f64];
                let view_dir = [0.0, 0.0, 1.0];
                let refl = mat.exact.and_then(|m| self.refl_maps.get(m)).and_then(|m| m.as_ref());
                let phong_color = mat.phong_color();
                for (li, light) in s.lights.iter().enumerate() {
                    let dir = self.lights.dir[li];
                    let v = vis(li);
                    let cosa = dot(dir, normal);
                    if cosa > EPSILON {
                        let d = mat.light_diff(cosa) * v * diffuse * light.intensity;
                        for k in 0..3 {
                            raw[k] += (light.color[k] + obj[k] * ambient / 3.0) * d;
                        }
                    }
                    if specular > EPSILON {
                        let rv = reflect(dir, normal);
                        let vr = dot(view_dir, rv);
                        if vr < EPSILON {
                            let sp = mat.light_diff(-vr).powf(mat.phong_size()) * v * specular * light.intensity;
                            for k in 0..3 {
                                raw[k] += phong_color[k] * sp;
                            }
                        }
                    }
                    if let Some(map) = refl {
                        let int = (if s.ao_enabled { mat.refl_map_intensity() - ao * ao_int * s.ao_affect_diffuse } else { mat.refl_map_intensity() }).max(0.0);
                        let rv = reflect(view_dir, normal);
                        let (u, w) = match mat.from.reflection_mapping {
                            ReflectionMapping::Spherical => {
                                let m = 2.0 * (rv[0] * rv[0] + rv[1] * rv[1] + (rv[2] + 1.0).powi(2)).sqrt();
                                ((rv[0] / m + 0.5).clamp(0.0, 1.0), (rv[1] / m + 0.5).clamp(0.0, 1.0))
                            }
                            ReflectionMapping::BlinnNewell => (
                                (((rv[0] / rv[2]).atan() + std::f64::consts::PI) / std::f64::consts::TAU).clamp(0.0, 1.0),
                                ((rv[1].asin() + std::f64::consts::FRAC_PI_2) / std::f64::consts::PI).clamp(0.0, 1.0),
                            ),
                        };
                        let rc = map.color(u, w);
                        for k in 0..3 {
                            raw[k] += rc[k] * bal[k] * v * int;
                        }
                    }
                }
            }
        }
        for k in 0..3 {
            acc[k] += raw[k] * scale * 255.0;
        }
        true
    }

    /// Filtered solid color, intensity and DOF distance of one output pixel.
    fn pixel(&self, x: u32, y: u32, bg: [i32; 3]) -> ([f64; 3], f64, f64) {
        let r = self.raster;
        let os = self.oversample as i64;
        let b = self.border as i64;
        let idx = |rx: i64, ry: i64| {
            let (rx, ry) = (rx + b, ry + b);
            (rx >= 0 && ry >= 0 && rx < r.width as i64 && ry < r.height as i64).then(|| ry as usize * r.width as usize + rx as usize)
        };
        let mut col = [0.0; 3];
        let (mut intensity, mut dof) = (0.0, 0.0);
        let f = &self.filter;
        if !f.is_empty() {
            for i in 0..f.size {
                for j in 0..f.size {
                    if let Some(c) = idx(x as i64 * os + j as i64 - f.half, y as i64 * os + i as i64 - f.half) {
                        let w = f.at(i, j) / (os * os) as f64;
                        if self.add_cell(c, w, bg, &mut col) {
                            dof += w * r.dof[c] as f64;
                            intensity += w;
                        }
                    }
                }
            }
        } else {
            let mut count = 0;
            for px in 0..os {
                for py in 0..os {
                    if let Some(c) = idx(x as i64 * os + px, y as i64 * os + py)
                        && self.add_cell(c, 1.0, bg, &mut col)
                    {
                        dof += r.dof[c] as f64;
                        count += 1;
                    }
                }
            }
            if count > 0 {
                let d = 1.0 / count as f64;
                dof *= d;
                col = col.map(|c| c * d);
                intensity = count as f64 / (os * os) as f64;
            }
        }
        (col, intensity.max(0.0), dof)
    }

    /// RGBA8 image plus the per-pixel DOF distance (for [`post_dof`]).
    pub fn render(&self, w: u32, h: u32) -> (Vec<u8>, Vec<f32>) {
        let flame = self.flame;
        let gamma = if flame.gamma == 0.0 { 0.0 } else { 1.0 / flame.gamma };
        let alpha_scale = 1.0 - (3.0 * (flame.fg_opacity - 1.0)).atan() / 1.25;
        let mod_sat = (flame.saturation - 1.0).max(-1.0);
        let mut img = vec![0u8; (w * h * 4) as usize];
        let mut dofs = vec![0f32; (w * h) as usize];
        let backdrop = super::background::Backdrop::new(flame, w, h);
        img.par_chunks_mut((w * 4) as usize).zip(dofs.par_chunks_mut(w as usize)).enumerate().for_each(|(y, (row, drow))| {
            for x in 0..w {
                let bg = backdrop.get(x, y as u32);
                let (col, intensity, dof) = self.pixel(x, y as u32, bg);
                drow[x as usize] = dof as f32;
                let (mut red, mut green, mut blue, alpha);
                if intensity > 0.0 {
                    let a = intensity.powf(1.0 + gamma);
                    let alpha_int = ((a * 255.0 * alpha_scale + 0.5) as i32).clamp(0, 255);
                    let inv = 255 - alpha_int;
                    alpha = if self.with_alpha { alpha_int } else { 255 };
                    red = ((col[0] + 0.5) as i32 + (inv.wrapping_mul(bg[0]) >> 8)).clamp(0, 255);
                    green = ((col[1] + 0.5) as i32 + (inv.wrapping_mul(bg[1]) >> 8)).clamp(0, 255);
                    blue = ((col[2] + 0.5) as i32 + (inv.wrapping_mul(bg[2]) >> 8)).clamp(0, 255);
                } else {
                    (red, green, blue) = (bg[0].clamp(0, 255), bg[1].clamp(0, 255), bg[2].clamp(0, 255));
                    alpha = if self.with_alpha { 0 } else { 255 };
                }
                if mod_sat != 0.0 {
                    let (hh, s, l) = rgb_to_hsl(red as f64 / 255.0, green as f64 / 255.0, blue as f64 / 255.0);
                    let (r2, g2, b2) = hsl_to_rgb(hh, s + mod_sat, l);
                    red = round_color(r2 * 255.0);
                    green = round_color(g2 * 255.0);
                    blue = round_color(b2 * 255.0);
                }
                row[(x * 4) as usize..(x * 4 + 4) as usize].copy_from_slice(&[red as u8, green as u8, blue as u8, alpha as u8]);
            }
        });
        (img, dofs)
    }
}

impl SolidShader<'_> {
    /// Linear float RGB without quantization (`transformPointHDR`); no post DOF.
    pub fn render_hdr(&self, w: u32, h: u32) -> Vec<f32> {
        let flame = self.flame;
        let gamma = if flame.gamma == 0.0 { 0.0 } else { 1.0 / flame.gamma };
        let alpha_scale = 1.0 - (3.0 * (flame.fg_opacity - 1.0)).atan() / 1.25;
        let mut out = vec![0f32; (w * h * 3) as usize];
        let backdrop = super::background::Backdrop::new(flame, w, h);
        out.par_chunks_mut((w * 3) as usize).enumerate().for_each(|(y, row)| {
            for x in 0..w {
                let bg = backdrop.get(x, y as u32);
                let (col, intensity, _) = self.pixel(x, y as u32, bg);
                let inv = if intensity > 0.0 { 255.0 - (intensity.powf(1.0 + gamma) * 255.0 * alpha_scale).clamp(0.0, 255.0) } else { 255.0 };
                let col = if intensity > 0.0 { col } else { [0.0; 3] };
                for k in 0..3 {
                    row[(x * 3) as usize + k] = ((col[k] + inv * bg[k] as f64 / 255.0).max(0.0) / 255.0) as f32;
                }
            }
        });
        out
    }
}

/// Fixed point scale of the post DOF accumulation buffer.
const DOF_SCALE: f64 = 1024.0;

/// `PostDOFCalculator`: splats every pixel with a bokeh kernel sized by its DOF distance.
pub fn post_dof(flame: &Flame, rgba: &mut [u8], dof: &[f32], w: u32, h: u32) {
    let s = &flame.solid;
    let (wi, hi) = (w as i64, h as i64);
    let img_size = ((w as f64).powi(2) + (h as f64).powi(2)).sqrt();
    let kernel = s.post_bokeh_filter_kernel;
    let support = kernel.spatial_support();
    let bokeh_intensity = s.post_bokeh_intensity * 1000.0 / img_size;
    let buf: Vec<[AtomicI64; 3]> = (0..(w * h) as usize).map(|_| [AtomicI64::new(0), AtomicI64::new(0), AtomicI64::new(0)]).collect();
    let src: &[u8] = rgba;
    (0..h).into_par_iter().for_each(|y| {
        let mut rng = Rng::new(0x0D0F_5EED ^ (y as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut taps: Vec<(i64, i64, f64)> = Vec::new();
        for x in 0..w {
            let i = (y * w + x) as usize;
            let (mut r, mut g, mut b) = (src[i * 4] as f64, src[i * 4 + 1] as f64, src[i * 4 + 2] as f64);
            if r <= 0.0 && g <= 0.0 && b <= 0.0 {
                continue;
            }
            let plain_radius = (dof[i] as f64).abs() * 10.0;
            let mut radius = ftoi((dof[i] as f64).abs() * 10.0) as f64;
            if radius < 2.0 {
                radius = 2.0;
            }
            if rng.random() > 1.0 - bokeh_intensity && (r * 0.299 + g * 0.588 + b * 0.113) / 255.0 >= s.post_bokeh_activation {
                let int = (rng.random() + 1.0) / 5.0;
                let f = int * radius * radius * s.post_bokeh_brightness;
                r = ftoi(r * f) as f64;
                g = ftoi(g * f) as f64;
                b = ftoi(b * f) as f64;
                radius *= s.post_bokeh_size * (1.0 + rng.random() * s.post_bokeh_size);
            } else {
                radius *= 1.0 + (0.5 - rng.random()) * 0.1;
            }
            let inv = 1.0 / plain_radius * support;
            let mut step = match radius {
                r if r < 2.0 => 0.0625,
                r if r < 3.0 => 0.125,
                r if r < 4.0 => 0.25,
                r if r < 5.0 => 0.375,
                r if r < 6.0 => 0.5,
                r if r < 7.0 => 0.625,
                r if r < 8.0 => 0.75,
                r if r < 9.0 => 0.875,
                _ => 1.0,
            } * 0.5;
            if radius / step > img_size.floor() {
                step = radius / img_size.floor();
            }
            taps.clear();
            let mut sum = 0.0;
            let mut di = -radius;
            while di < radius + EPSILON {
                let i2 = (di * inv) * (di * inv);
                let dst_x = x as f64 + di;
                let mut dj = -radius;
                while dj < radius + EPSILON {
                    let rr = (i2 + (dj * inv) * (dj * inv)).sqrt();
                    let c = kernel.coeff(rr);
                    if c > EPSILON {
                        sum += c;
                        taps.push((ftoi(dst_x) as i64, ftoi(y as f64 + dj) as i64, c));
                    }
                    dj += step;
                }
                di += step;
            }
            if taps.is_empty() {
                taps.push((x as i64, y as i64, 1.0));
                sum = 1.0;
            }
            for &(tx, ty, c) in &taps {
                if tx >= 0 && tx < wi && ty >= 0 && ty < hi {
                    let cell = &buf[(ty * wi + tx) as usize];
                    let f = c / sum * DOF_SCALE;
                    cell[0].fetch_add((r * f) as i64, Ordering::Relaxed);
                    cell[1].fetch_add((g * f) as i64, Ordering::Relaxed);
                    cell[2].fetch_add((b * f) as i64, Ordering::Relaxed);
                }
            }
        }
    });
    for (px, cell) in rgba.chunks_exact_mut(4).zip(&buf) {
        for k in 0..3 {
            px[k] = round_color(cell[k].load(Ordering::Relaxed) as f64 / DOF_SCALE) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_plane_has_facing_normals() {
        let frames = [None];
        let mut acc = SolidAccum::new(8, 8, &frames, 64);
        let mut samples = Vec::new();
        for y in 0..8 {
            for x in 0..8 {
                samples.push(SolidSample {
                    x,
                    y,
                    z: 1.0,
                    origin: [x as f32 * 0.1, y as f32 * 0.1, 0.5],
                    color: [100.0; 3],
                    material: 0.0,
                    dof: 0.0,
                });
            }
        }
        acc.add(&samples);
        let n = normal_at(&acc, 4, 4);
        assert!(n[0].abs() < 1e-6 && n[1].abs() < 1e-6 && (n[2].abs() - 1.0).abs() < 1e-6, "{n:?}");
        assert_eq!(normal_at(&SolidAccum::new(4, 4, &frames, 64), 1, 1)[0], ZMIN);
    }

    #[test]
    fn shadow_frame_maps_bounds_into_map() {
        let f = ShadowFrame::fit(&[[0.0, 0.0, 0.0], [2.0, 1.0, 0.0]], 100);
        let (x0, y0) = f.map(0.0, 0.0);
        let (x1, y1) = f.map(2.0, 1.0);
        assert!(x0 > 0 && y0 > 0 && x1 < 100 && y1 < 100);
    }
}
