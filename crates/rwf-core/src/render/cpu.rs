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

//! Multi-threaded, progressive CPU renderer.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use super::prepared::{NEXT_XFORM_TABLE_SIZE, PreparedFlame, PreparedLayer};
use super::solid::{self, LightSample, LightViews, ShadowFrame, SolidAccum, SolidSample};
use super::tonemap::{Raster, ToneMapper};
use super::view::View;
use crate::math::{EPSILON, M_2PI, M_PI, hsl_to_rgb, rgb_to_hsl, round_color};
use crate::model::{Flame, PostSymmetryType};
use crate::variations::{Ctx, Pt};

const INITIAL_ITERATIONS: usize = 42;
/// Fixed-point scale of the color channels in the atomic histogram.
const COLOR_SCALE: f64 = 1024.0;

/// Shared accumulation raster: `[count, r, g, b]` per cell, colors in signed fixed point.
pub struct Histogram {
    width: u32,
    height: u32,
    cells: Vec<AtomicU64>,
}

impl Histogram {
    pub fn new(width: u32, height: u32) -> Self {
        let n = width as usize * height as usize * 4;
        let mut cells = Vec::with_capacity(n);
        cells.resize_with(n, || AtomicU64::new(0));
        Self { width, height, cells }
    }

    #[inline]
    pub fn add(&self, x: u32, y: u32, r: f64, g: f64, b: f64) {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.cells[i].fetch_add(1, Ordering::Relaxed);
        self.cells[i + 1].fetch_add((r * COLOR_SCALE) as i64 as u64, Ordering::Relaxed);
        self.cells[i + 2].fetch_add((g * COLOR_SCALE) as i64 as u64, Ordering::Relaxed);
        self.cells[i + 3].fetch_add((b * COLOR_SCALE) as i64 as u64, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        for c in &self.cells {
            c.store(0, Ordering::Relaxed);
        }
    }
}

impl Raster for Histogram {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    #[inline]
    fn cell(&self, x: u32, y: u32) -> [f64; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let ld = |k: usize| self.cells[i + k].load(Ordering::Relaxed) as i64 as f64 / COLOR_SCALE;
        [self.cells[i].load(Ordering::Relaxed) as f64, ld(1), ld(2), ld(3)]
    }
}

/// Per-cell maximum camera depth of bright samples (`RasterFloatIntWithZBuffer`), f32 bits.
pub struct ZBuffer {
    width: u32,
    cells: Vec<AtomicU32>,
}

impl ZBuffer {
    pub fn new(width: u32, height: u32) -> Self {
        let n = width as usize * height as usize;
        let mut cells = Vec::with_capacity(n);
        cells.resize_with(n, || AtomicU32::new(solid::ZMIN.to_bits()));
        Self { width, cells }
    }

    #[inline]
    pub fn add(&self, x: u32, y: u32, z: f32) {
        let c = &self.cells[y as usize * self.width as usize + x as usize];
        let _ = c.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| (z > f32::from_bits(old)).then_some(z.to_bits()));
    }

    /// Depth of a cell, `None` if nothing was plotted there.
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> Option<f32> {
        let z = f32::from_bits(self.cells[y as usize * self.width as usize + x as usize].load(Ordering::Relaxed));
        (z != solid::ZMIN).then_some(z)
    }
}

/// Plain f32 raster (e.g. read back from the GPU).
pub struct FloatRaster {
    pub width: u32,
    pub height: u32,
    pub data: Vec<f32>,
}

impl Raster for FloatRaster {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    #[inline]
    fn cell(&self, x: u32, y: u32) -> [f64; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.data[i] as f64, self.data[i + 1] as f64, self.data[i + 2] as f64, self.data[i + 3] as f64]
    }
}

/// Everything a render thread needs, shared read-only.
/// One flame state iterated into the shared histogram (several for motion blur).
struct Packet {
    prepared: PreparedFlame,
    view: View,
    antialias_amount: f64,
    antialias_radius: f64,
    symmetry: Symmetry,
    post_blur: Option<PostBlur>,
    /// Solid rendering: light projections (`None` = classic flame rendering).
    lights: Option<LightViews>,
}

/// Per-thread buffers of solid samples, flushed into [`SolidAccum`] in batches.
#[derive(Default)]
struct PlotOut {
    samples: Vec<SolidSample>,
    lights: Vec<LightSample>,
}

/// `PostBlurRenderIterationState`: splats samples over a radial kernel.
pub struct PostBlur {
    pub radius: i64,
    pub fade: f64,
    /// `(2 * radius + 1)²` weights, row major.
    pub kernel: Vec<f64>,
}

impl PostBlur {
    pub fn new(flame: &Flame) -> Option<Self> {
        if flame.post_blur_radius == 0 {
            return None;
        }
        let radius = flame.post_blur_radius as i64;
        let mut kernel = Vec::with_capacity(((2 * radius + 1) * (2 * radius + 1)) as usize);
        for y in -radius..=radius {
            for x in -radius..=radius {
                let r = ((x * x + y * y) as f64).sqrt() / radius as f64;
                kernel.push(2f64.powf(-r * flame.post_blur_falloff));
            }
        }
        Some(Self { radius, fade: flame.post_blur_fade.clamp(0.0, 1.0), kernel })
    }
}

struct Shared {
    packets: Vec<Packet>,
    histogram: Histogram,
    samples: AtomicU64,
    stop: AtomicBool,
    target_samples: u64,
    dimish_color: [f64; 3],
    solid: Option<Mutex<SolidAccum>>,
    zbuffer: Option<ZBuffer>,
}

#[derive(Clone)]
struct Symmetry {
    kind: PostSymmetryType,
    centre: (f64, f64),
    /// Point symmetry rotations (sin, cos).
    rotations: Vec<(f64, f64)>,
    halve_dist: f64,
    rot: Option<(f64, f64)>,
}

impl Symmetry {
    fn new(flame: &Flame) -> Self {
        let ps = &flame.post_symmetry;
        let mut rotations = Vec::new();
        if ps.kind == PostSymmetryType::Point && ps.order > 1 {
            let order = ps.order.min(64);
            let da = M_2PI / order as f64;
            for i in 0..order {
                let a = da * i as f64;
                rotations.push(a.sin_cos());
            }
        }
        let a = ps.rotation * M_2PI / 180.0 / 2.0;
        Self {
            kind: if ps.kind == PostSymmetryType::Point && ps.order <= 1 { PostSymmetryType::None } else { ps.kind },
            centre: (ps.centre_x, ps.centre_y),
            rotations,
            halve_dist: ps.distance / 2.0,
            rot: if a.abs() > EPSILON { Some(a.sin_cos()) } else { None },
        }
    }
}

struct LayerState {
    p: Pt,
    xf: usize,
    valid: bool,
}

pub struct CpuRender {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
    flame: Flame,
}

pub struct RenderOptions {
    pub width: u32,
    pub height: u32,
    pub threads: usize,
    /// Stop after this sample density (flame quality); `None` renders until stopped.
    pub max_density: Option<f64>,
    pub seed: u64,
    /// Records depths for [`CpuRender::zbuffer_image`] (solid renders always have them).
    pub zbuffer: bool,
}

impl CpuRender {
    pub fn start(flame: &Flame, opts: RenderOptions) -> Self {
        Self::start_packets(std::slice::from_ref(flame), opts)
    }

    /// Renders an animation frame (incl. motion blur sub-frames if enabled).
    pub fn start_frame(flame: &Flame, frame: f64, opts: RenderOptions) -> Self {
        Self::start_packets(&super::frame_packets(flame, frame), opts)
    }

    /// Iterates several flame states into one histogram (motion blur). `flames[0]` is the
    /// main state; all must have the same image settings.
    pub fn start_packets(flames: &[Flame], opts: RenderOptions) -> Self {
        let flame = &flames[0];
        let filter_size = flame.filter_kernel.filter_size(flame.spatial_filter_radius, flame.spatial_oversampling.max(1));
        let packets: Vec<Packet> = flames
            .iter()
            .map(|f| Packet {
                prepared: PreparedFlame::new(f),
                view: View::new(f, opts.width.max(1), opts.height.max(1), filter_size),
                antialias_amount: f.antialias_amount,
                antialias_radius: f.antialias_radius,
                symmetry: Symmetry::new(f),
                post_blur: PostBlur::new(f),
                lights: f.solid.enabled.then(|| LightViews::new(&f.solid)),
            })
            .collect();
        let view = &packets[0].view;
        let raster_size = view.raster_width as u64 * view.raster_height as u64;
        let target_samples = match opts.max_density {
            Some(d) => ((d * raster_size as f64 / view.oversample as f64 + 0.5) as u64).saturating_mul(packets.len() as u64),
            None => u64::MAX,
        };
        let solid = flame.solid.enabled.then(|| {
            let frames = shadow_frames(&packets[0], flame);
            Mutex::new(SolidAccum::new(view.raster_width, view.raster_height, &frames, solid::shadow_map_size(flame, view.ppu_scale)))
        });
        let shared = Arc::new(Shared {
            histogram: Histogram::new(if solid.is_some() { 0 } else { view.raster_width }, if solid.is_some() { 0 } else { view.raster_height }),
            samples: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            target_samples,
            // JWildfire blends the render colors with the 0..255 values
            dimish_color: flame.dim_z_color.map(|c| c as f64),
            zbuffer: (opts.zbuffer && solid.is_none()).then(|| ZBuffer::new(view.raster_width, view.raster_height)),
            solid,
            packets,
        });
        let n = opts.threads.max(1);
        let mut threads = Vec::with_capacity(n);
        if !shared.packets[0].prepared.layers.is_empty() {
            for i in 0..n {
                let sh = shared.clone();
                let seed = opts.seed.wrapping_add((i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                threads.push(
                    std::thread::Builder::new()
                        .name(format!("rwf-render-{i}"))
                        .spawn(move || render_thread(sh, seed))
                        .expect("spawn render thread"),
                );
            }
        }
        Self { shared, threads, flame: flame.clone() }
    }

    pub fn samples(&self) -> u64 {
        self.shared.samples.load(Ordering::Relaxed)
    }

    /// Current sample density per packet ("quality" in JWildfire terms).
    pub fn density(&self) -> f64 {
        let v = &self.shared.packets[0].view;
        self.samples() as f64 / (v.raster_width as f64 * v.raster_height as f64) * v.oversample as f64 / self.shared.packets.len() as f64
    }

    pub fn is_finished(&self) -> bool {
        self.threads.iter().all(|t| t.is_finished())
    }

    pub fn view(&self) -> &View {
        &self.shared.packets[0].view
    }

    pub fn missing_variations(&self) -> &[String] {
        &self.shared.packets[0].prepared.missing_variations
    }

    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }

    pub fn wait(mut self) -> Self {
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        self
    }

    /// Replaces the flame used for tonemapping (only valid if [`super::same_iteration`] holds).
    pub fn update_tone(&mut self, flame: &Flame) {
        self.flame = flame.clone();
    }

    pub fn flame(&self) -> &Flame {
        &self.flame
    }

    /// Tonemaps without density estimation (cheap, for progressive previews).
    pub fn image_fast(&self, with_alpha: bool) -> Vec<u8> {
        if self.shared.solid.is_some() {
            return self.solid_image(with_alpha, false);
        }
        let v = &self.shared.packets[0].view;
        let tm = ToneMapper::new(&self.flame, v.image_width, v.image_height, v.ppu, self.density(), with_alpha).with_packets(self.shared.packets.len());
        tm.render(&self.flame, &self.shared.histogram, v.image_width, v.image_height)
    }

    /// Tonemaps the current state into an RGBA8 image (with density estimation if enabled).
    pub fn image(&self, with_alpha: bool) -> Vec<u8> {
        if self.shared.solid.is_some() {
            return self.solid_image(with_alpha, true);
        }
        let v = &self.shared.packets[0].view;
        let tm = ToneMapper::new(&self.flame, v.image_width, v.image_height, v.ppu, self.density(), with_alpha).with_packets(self.shared.packets.len());
        match super::de::DeParams::new(&self.flame, v.ppu_scale) {
            Some(de) => {
                let raster = super::de::apply(&self.shared.histogram, &de);
                tm.render(&self.flame, &raster, v.image_width, v.image_height)
            }
            None => tm.render(&self.flame, &self.shared.histogram, v.image_width, v.image_height),
        }
    }
}

impl CpuRender {
    /// The image without spatial filter and density estimation (see
    /// [`ToneMapper::unfiltered`]): the input for the denoiser. `None` for solid flames.
    pub fn image_unfiltered(&self, with_alpha: bool) -> Option<Vec<u8>> {
        if self.shared.solid.is_some() {
            return None;
        }
        let v = &self.shared.packets[0].view;
        let tm = ToneMapper::new(&self.flame, v.image_width, v.image_height, v.ppu, self.density(), with_alpha).with_packets(self.shared.packets.len()).unfiltered();
        Some(tm.render(&self.flame, &self.shared.histogram, v.image_width, v.image_height))
    }

    /// Samples per pixel of [`CpuRender::image_unfiltered`] (no albedo or normals on the
    /// CPU). `None` for solid flames.
    pub fn denoise_aux(&self) -> Option<super::denoise::DenoiseAux> {
        if self.shared.solid.is_some() {
            return None;
        }
        let v = &self.shared.packets[0].view;
        let tm = ToneMapper::new(&self.flame, v.image_width, v.image_height, v.ppu, self.density(), false).unfiltered();
        let density = tm.pixel_counts(&self.shared.histogram, v.image_width, v.image_height);
        Some(super::denoise::DenoiseAux { density: Some(density), ..Default::default() })
    }

    /// Applies the flame's post denoiser and image effects to `img` (this render's
    /// [`CpuRender::image`]); the denoiser works on the unfiltered image, Blend mixes `img`
    /// back in.
    pub fn apply_post_processing(&self, img: &mut [u8], with_alpha: bool) -> Result<super::post::PostOutcome, String> {
        let v = &self.shared.packets[0].view;
        let (w, h) = (v.image_width, v.image_height);
        let active = super::denoise::skip_reason(&self.flame, false).is_none();
        let unfiltered = if active { self.image_unfiltered(with_alpha) } else { None };
        let aux = if active { self.denoise_aux() } else { None };
        super::post::process(&self.flame, img, unfiltered.as_deref(), w, h, false, aux.as_ref())
    }

    /// Tonemaps into linear float RGB (`RenderHDRImageThread`), 1.0 = full 8-bit white.
    pub fn image_hdr(&self) -> Vec<f32> {
        let v = &self.shared.packets[0].view;
        if let Some(acc) = &self.shared.solid {
            let acc = acc.lock().expect("solid accum").clone();
            let lights = LightViews::new(&self.flame.solid);
            let raster = solid::finalize(&acc, &self.flame, &lights);
            return solid::SolidShader::new(&self.flame, &raster, &lights, v.border, false).render_hdr(v.image_width, v.image_height);
        }
        let tm = ToneMapper::new(&self.flame, v.image_width, v.image_height, v.ppu, self.density(), false).with_packets(self.shared.packets.len());
        match super::de::DeParams::new(&self.flame, v.ppu_scale) {
            Some(de) => tm.render_hdr(&self.flame, &super::de::apply(&self.shared.histogram, &de), v.image_width, v.image_height),
            None => tm.render_hdr(&self.flame, &self.shared.histogram, v.image_width, v.image_height),
        }
    }

    /// 16-bit gray depth image (`RenderZBufferThread`), mapped by the flame's z-buffer scale,
    /// shift and bias. `None` unless the render recorded depths.
    pub fn zbuffer_image(&self) -> Option<Vec<u16>> {
        let v = &self.shared.packets[0].view;
        let solid = self.shared.solid.as_ref().map(|acc| acc.lock().expect("solid accum").z.clone());
        match (&solid, &self.shared.zbuffer) {
            (Some(z), _) => Some(zbuffer_image(&self.flame, v, |x, y| Some(z[(y * v.raster_width + x) as usize]).filter(|z| *z != solid::ZMIN))),
            (None, Some(zb)) => Some(zbuffer_image(&self.flame, v, |x, y| zb.get(x, y))),
            (None, None) => None,
        }
    }

    /// Suggested z-buffer scale, shift and bias for this render ([`suggest_zbuffer_params`]).
    pub fn zbuffer_suggestion(&self) -> Option<(f64, f64, f64)> {
        let v = &self.shared.packets[0].view;
        let solid = self.shared.solid.as_ref().map(|acc| acc.lock().expect("solid accum").z.clone());
        match (&solid, &self.shared.zbuffer) {
            (Some(z), _) => suggest_zbuffer_params(v, |x, y| Some(z[(y * v.raster_width + x) as usize]).filter(|z| *z != solid::ZMIN)),
            (None, Some(zb)) => suggest_zbuffer_params(v, |x, y| zb.get(x, y)),
            (None, None) => None,
        }
    }

    /// Solid rendering: normals, AO, shadows, shading and (optionally) post DOF of the current
    /// z-buffer.
    fn solid_image(&self, with_alpha: bool, post_dof: bool) -> Vec<u8> {
        let v = &self.shared.packets[0].view;
        let acc = self.shared.solid.as_ref().expect("solid render").lock().expect("solid accum").clone();
        let lights = LightViews::new(&self.flame.solid);
        let raster = solid::finalize(&acc, &self.flame, &lights);
        let shader = solid::SolidShader::new(&self.flame, &raster, &lights, v.border, with_alpha);
        let (mut img, dof) = shader.render(v.image_width, v.image_height);
        if post_dof && self.flame.cam_dof > EPSILON {
            solid::post_dof(&self.flame, &mut img, &dof, v.image_width, v.image_height);
        }
        img
    }
}

impl Drop for CpuRender {
    fn drop(&mut self) {
        self.stop();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Z-buffer scale, shift and bias that spread the depths of a render over the gray range
/// (`ZBufferAnalyzer` / `ZBufferGrayValueCalculator.suggestParams`): the depth range plus 6% on
/// both sides maps to black..white; the bias grows when less of the image is covered.
pub fn suggest_zbuffer_params(v: &View, depth: impl Fn(u32, u32) -> Option<f32> + Sync) -> Option<(f64, f64, f64)> {
    let (w, h) = (v.image_width, v.image_height);
    let (mut zmin, mut zmax, mut count) = (f64::MAX, f64::MIN, 0u64);
    for y in 0..h {
        for x in 0..w {
            if let Some(z) = pixel_depth(v, &depth, x, y) {
                zmin = zmin.min(z);
                zmax = zmax.max(z);
                count += 1;
            }
        }
    }
    if count == 0 || zmax - zmin <= 0.0 {
        return None;
    }
    let coverage = count as f64 / (w as f64 * h as f64);
    let (lo, hi) = (zmin - (zmax - zmin) * 0.06, zmax + (zmax - zmin) * 0.06);
    let scale = 2.0 / (hi - lo);
    let shift = 2.0 * lo / (hi - lo) + 1.0;
    let bias = if coverage > 0.95 {
        0.0
    } else if coverage > 0.85 {
        0.1
    } else {
        0.2
    };
    // the flame's scale is in thousandths (ZSCALE_SCALE)
    Some((scale / 0.001, shift, bias))
}

/// The average depth of the oversampling cells of output pixel `(x, y)` (`transformZPoint`).
fn pixel_depth(v: &View, depth: &impl Fn(u32, u32) -> Option<f32>, x: u32, y: u32) -> Option<f64> {
    let os = v.oversample;
    let (mut sum, mut n) = (0.0, 0);
    for px in 0..os {
        for py in 0..os {
            let (rx, ry) = (x * os + px + v.border, y * os + py + v.border);
            if rx < v.raster_width
                && ry < v.raster_height
                && let Some(z) = depth(rx, ry)
            {
                sum += z as f64;
                n += 1;
            }
        }
    }
    (n > 0).then(|| sum / n as f64)
}

/// 16-bit gray depth image from the raster depths `depth(x, y)` (`None` = empty cell): averaged
/// over the oversampling cells (`transformZPoint`), mapped by the flame's z-buffer scale, shift
/// and bias (`RenderZBufferThread`).
pub fn zbuffer_image(flame: &Flame, v: &View, depth: impl Fn(u32, u32) -> Option<f32> + Sync) -> Vec<u16> {
    use rayon::prelude::*;
    let (w, h) = (v.image_width, v.image_height);
    let depth = |x: u32, y: u32| pixel_depth(v, &depth, x, y);
    let z_scale = flame.zbuffer_scale * 0.001;
    let z_bias = if flame.zbuffer_bias <= 0.0 {
        0
    } else if flame.zbuffer_bias >= 1.0 {
        0xffff
    } else {
        crate::math::ftoi(flame.zbuffer_bias * 65535.0) as i64
    };
    let z_shift = flame.zbuffer_shift;
    let mut out = vec![0u16; (w * h) as usize];
    out.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let d = depth(x, y as u32);
            let lvl = |s: f64, z: f64| (crate::math::ftoi((s * z - z_shift) * 32767.0 + 32767.0) as i64).clamp(0, 0xffff);
            // Negative scale: black near the camera, white background.
            let gray = if z_scale < 0.0 {
                match d {
                    Some(z) => {
                        let g = 0xffff - lvl(-z_scale, z);
                        if z_bias > 0 && g > 0xffff - z_bias { 0xffff - z_bias } else { g }
                    }
                    None => 0xffff - z_bias,
                }
            } else {
                match d {
                    Some(z) => lvl(z_scale, z).max(z_bias),
                    None => z_bias,
                }
            };
            row[x as usize] = gray as u16;
        }
    });
    out
}

/// Blocking convenience render to RGBA8.
pub fn render_image(flame: &Flame, width: u32, height: u32, quality: f64, with_alpha: bool) -> Vec<u8> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let r = CpuRender::start(flame, RenderOptions { width, height, threads, max_density: Some(quality), seed: 0x5EED_1234, zbuffer: false }).wait();
    r.image(with_alpha)
}

fn render_thread(sh: Arc<Shared>, seed: u64) {
    let mut ctx = Ctx::new(seed, sh.packets[0].prepared.preserve_z);
    let mut states: Vec<Vec<LayerState>> =
        sh.packets.iter().map(|pk| pk.prepared.layers.iter().map(|_| LayerState { p: Pt::default(), xf: 0, valid: false }).collect()).collect();
    for (pk, sts) in sh.packets.iter().zip(states.iter_mut()) {
        for (l, st) in pk.prepared.layers.iter().zip(sts.iter_mut()) {
            ctx.set_palette(&l.palette);
            pre_fuse(l, st, &mut ctx);
        }
    }
    let per_round: u64 = sh.packets.iter().map(|pk| pk.prepared.layers.len() as u64).sum();
    const BATCH: u64 = 4096;
    let mut out = PlotOut::default();
    loop {
        if sh.stop.load(Ordering::Relaxed) {
            break;
        }
        for (pk, sts) in sh.packets.iter().zip(states.iter_mut()) {
            let layers = &pk.prepared.layers;
            for (l, st) in layers.iter().zip(sts.iter_mut()) {
                if ctx.palette != l.palette {
                    ctx.set_palette(&l.palette);
                }
                if !st.valid || st.p.is_bad() {
                    pre_fuse(l, st, &mut ctx);
                    if !st.valid {
                        continue;
                    }
                }
                for _ in 0..BATCH {
                    iterate(&sh, pk, l, st, &mut ctx, &mut out);
                }
            }
        }
        if let Some(acc) = &sh.solid
            && (!out.samples.is_empty() || !out.lights.is_empty())
        {
            let mut acc = acc.lock().expect("solid accum");
            acc.add(&out.samples);
            acc.add_light(&out.lights);
            out.samples.clear();
            out.lights.clear();
        }
        let total = sh.samples.fetch_add(BATCH * per_round, Ordering::Relaxed);
        if total >= sh.target_samples {
            break;
        }
    }
}

fn pre_fuse(layer: &PreparedLayer, st: &mut LayerState, ctx: &mut Ctx) {
    st.p = Pt { x: 2.0 * ctx.random() - 1.0, y: 2.0 * ctx.random() - 1.0, z: 0.0, color: ctx.random(), ..Pt::default() };
    st.p.material = ctx.random();
    st.xf = 0;
    st.valid = false;
    if layer.xforms.is_empty() {
        return;
    }
    st.p = layer.xforms[0].transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
    for _ in 0..=INITIAL_ITERATIONS {
        let Some(next) = &layer.xforms[st.xf].next else {
            return;
        };
        st.xf = next[ctx.random_int(NEXT_XFORM_TABLE_SIZE)] as usize;
        st.p = layer.xforms[st.xf].transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
    }
    st.valid = true;
}

#[inline]
fn iterate(sh: &Shared, pk: &Packet, layer: &PreparedLayer, st: &mut LayerState, ctx: &mut Ctx, out: &mut PlotOut) {
    if layer.density < 1.0 && ctx.random() > layer.density * layer.density {
        return;
    }
    let Some(next) = &layer.xforms[st.xf].next else {
        st.valid = false;
        return;
    };
    st.xf = next[ctx.random_int(NEXT_XFORM_TABLE_SIZE)] as usize;
    let xf = &layer.xforms[st.xf];
    st.p = xf.transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
    if !xf.is_visible_sample(ctx) {
        return;
    }
    let q = if layer.finals.is_empty() {
        st.p
    } else {
        let mut q = layer.finals[0].transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
        for f in &layer.finals[1..] {
            q = f.transform(ctx, &layer.colormap, layer.smooth_gradient, &q);
        }
        q
    };
    if q.do_hide {
        return;
    }
    let sym = &pk.symmetry;
    match sym.kind {
        PostSymmetryType::None => plot(sh, pk, layer, &st.p, &q, q.x, q.y, ctx, out),
        PostSymmetryType::Point => {
            let (cx, cy) = sym.centre;
            let dx = q.x - cx;
            let dy = q.y - cy;
            plot(sh, pk, layer, &st.p, &q, q.x, q.y, ctx, out);
            for &(s, c) in &sym.rotations {
                plot(sh, pk, layer, &st.p, &q, cx + dx * c + dy * s, cy + dy * c - dx * s, ctx, out);
            }
        }
        PostSymmetryType::XAxis | PostSymmetryType::YAxis => {
            let (cx, cy) = sym.centre;
            let (mut ax, mut ay, mut bx, mut by) = (q.x, q.y, q.x, q.y);
            if sym.kind == PostSymmetryType::XAxis {
                let dx = q.x - cx;
                ax = cx + dx + sym.halve_dist;
                bx = cx - dx - sym.halve_dist;
            } else {
                let dy = q.y - cy;
                ay = cy + dy + sym.halve_dist;
                by = cy - dy - sym.halve_dist;
            }
            if let Some((s, c)) = sym.rot {
                let (dx, dy) = (ax - cx, ay - cy);
                ax = cx + dx * c + dy * s;
                ay = cy + dy * c - dx * s;
                let (dx, dy) = (bx - cx, by - cy);
                bx = cx + dx * c - dy * s;
                by = cy + dy * c + dx * s;
            }
            plot(sh, pk, layer, &st.p, &q, ax, ay, ctx, out);
            plot(sh, pk, layer, &st.p, &q, bx, by, ctx, out);
        }
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn plot(sh: &Shared, pk: &Packet, layer: &PreparedLayer, p: &Pt, q: &Pt, x: f64, y: f64, ctx: &mut Ctx, out: &mut PlotOut) {
    if let Some(lights) = &pk.lights {
        plot_solid(sh, pk, lights, layer, p, q, x, y, out);
        return;
    }
    let Some((mut raw_x, mut raw_y, dz, cam_z)) = pk.view.project(x, y, q.z, ctx) else {
        return;
    };
    if !(x.is_finite() && y.is_finite()) {
        return;
    }
    if pk.antialias_amount > EPSILON && pk.antialias_radius > EPSILON && ctx.random() > 1.0 - pk.antialias_amount {
        let dr = (pk.antialias_radius * (-ctx.random().ln()).sqrt()).exp() - 1.0;
        let da = ctx.random() * 2.0 * M_PI;
        raw_x += dr * da.cos();
        raw_y += dr * da.sin();
    }
    let sx = (raw_x + 0.5) as i64;
    let sy = (raw_y + 0.5) as i64;
    let v = &pk.view;
    if sx < 0 || sy < 0 || sx >= v.raster_width as i64 || sy >= v.raster_height as i64 {
        return;
    }
    let (mut r, mut g, mut b) = (q.red, q.green, q.blue);
    transform_plot_color(p, &mut r, &mut g, &mut b);
    let w = layer.weight;
    let d = sh.dimish_color;
    r = (d[0] + (r - d[0]) * dz) * w;
    g = (d[1] + (g - d[1]) * dz) * w;
    b = (d[2] + (b - d[2]) * dz) * w;
    if let Some(zb) = &sh.zbuffer
        && 0.299 * r + 0.588 * g + 0.113 * b > 8.0
    {
        zb.add(sx as u32, sy as u32, cam_z as f32);
    }
    match &pk.post_blur {
        Some(pb) if ctx.random() > pb.fade => {
            let side = 2 * pb.radius + 1;
            for ky in 0..side {
                let y = sy + ky - pb.radius;
                if y < 0 || y >= v.raster_height as i64 {
                    continue;
                }
                for kx in 0..side {
                    let x = sx + kx - pb.radius;
                    if x >= 0 && x < v.raster_width as i64 {
                        let scl = pb.kernel[(ky * side + kx) as usize];
                        sh.histogram.add(x as u32, y as u32, r * scl, g * scl, b * scl);
                    }
                }
            }
        }
        _ => sh.histogram.add(sx as u32, sy as u32, r, g, b),
    }
}

/// Solid rendering plot (`DefaultPointProjector` + `plotPoint` with a z-buffer).
#[allow(clippy::too_many_arguments)]
fn plot_solid(sh: &Shared, pk: &Packet, lights: &LightViews, layer: &PreparedLayer, p: &Pt, q: &Pt, x: f64, y: f64, out: &mut PlotOut) {
    if !p.receive_only_shadows {
        for (li, m) in lights.proj.iter().enumerate() {
            if let Some(m) = m {
                let lp = LightViews::project(m, x, y, q.z);
                out.lights.push(LightSample { light: li as u32, p: [lp[0] as f32, lp[1] as f32, lp[2] as f32] });
            }
        }
    }
    if !(x.is_finite() && y.is_finite() && q.z.is_finite()) {
        return;
    }
    let v = &pk.view;
    let Some((raw_x, raw_y, dz, cz, dof)) = v.project_solid(x, y, q.z) else {
        return;
    };
    let sx = (raw_x + 0.5) as i64;
    let sy = (raw_y + 0.5) as i64;
    if sx < 0 || sy < 0 || sx >= v.raster_width as i64 || sy >= v.raster_height as i64 {
        return;
    }
    let d = sh.dimish_color;
    let w = layer.weight;
    let c = |i: usize, c: f64| ((d[i] + (c - d[i]) * dz) * w) as f32;
    out.samples.push(SolidSample {
        x: sx as u32,
        y: sy as u32,
        z: (cz * v.bws) as f32,
        origin: [x as f32, y as f32, q.z as f32],
        color: [c(0, q.red), c(1, q.green), c(2, q.blue)],
        material: if p.receive_only_shadows { -1.0 } else { p.material as f32 },
        dof: dof as f32,
    });
}

/// Shadow map placement for solid rendering at `width`×`height` (also used by the GPU renderer).
pub fn solid_shadow_frames(flame: &Flame, width: u32, height: u32) -> Vec<Option<ShadowFrame>> {
    let filter_size = flame.filter_kernel.filter_size(flame.spatial_filter_radius, flame.spatial_oversampling.max(1));
    let pk = Packet {
        prepared: PreparedFlame::new(flame),
        view: View::new(flame, width.max(1), height.max(1), filter_size),
        antialias_amount: 0.0,
        antialias_radius: 0.0,
        symmetry: Symmetry::new(flame),
        post_blur: None,
        lights: flame.solid.enabled.then(|| LightViews::new(&flame.solid)),
    };
    shadow_frames(&pk, flame)
}

/// Places the shadow maps from the light-space bounds of the first samples
/// (`ShadowCalculator.addShadowMapSamples`).
fn shadow_frames(pk: &Packet, flame: &Flame) -> Vec<Option<ShadowFrame>> {
    let Some(lights) = pk.lights.as_ref().filter(|l| l.proj.iter().any(Option::is_some)) else {
        return vec![None; flame.solid.lights.len()];
    };
    let tmp = Shared {
        packets: Vec::new(),
        histogram: Histogram::new(0, 0),
        samples: AtomicU64::new(0),
        stop: AtomicBool::new(false),
        target_samples: 0,
        dimish_color: [0.0; 3],
        solid: None,
        zbuffer: None,
    };
    let mut ctx = Ctx::new(solid::SHADOW_SEED ^ 0x5AD0_3D5E, pk.prepared.preserve_z);
    let mut out = PlotOut::default();
    let mut points: Vec<Vec<[f32; 3]>> = vec![Vec::new(); lights.proj.len()];
    let enough = |points: &[Vec<[f32; 3]>]| lights.proj.iter().zip(points).all(|(m, p)| m.is_none() || p.len() >= solid::SHADOW_MAP_PRE_SAMPLES);
    'outer: for _ in 0..64 {
        for layer in &pk.prepared.layers {
            ctx.set_palette(&layer.palette);
            let mut st = LayerState { p: Pt::default(), xf: 0, valid: false };
            pre_fuse(layer, &mut st, &mut ctx);
            if !st.valid {
                continue;
            }
            for _ in 0..16384 {
                if !st.valid || st.p.is_bad() {
                    pre_fuse(layer, &mut st, &mut ctx);
                    if !st.valid {
                        break;
                    }
                }
                iterate(&tmp, pk, layer, &mut st, &mut ctx, &mut out);
            }
            for s in out.lights.drain(..) {
                if points[s.light as usize].len() < solid::SHADOW_MAP_PRE_SAMPLES {
                    points[s.light as usize].push(s.p);
                }
            }
            out.samples.clear();
            if enough(&points) {
                break 'outer;
            }
        }
    }
    let size = solid::shadow_map_size(flame, pk.view.ppu_scale);
    lights.proj.iter().zip(&points).map(|(m, p)| m.map(|_| ShadowFrame::fit(p, size))).collect()
}

#[inline]
fn transform_plot_color(p: &Pt, r: &mut f64, g: &mut f64, b: &mut f64) {
    if p.mod_gamma.abs() > EPSILON {
        let gamma = 4.2 / (4.2 - p.mod_gamma);
        let alpha = *r * 0.299 + *g * 0.588 + *b * 0.113;
        if alpha > EPSILON {
            let m = alpha.powf(gamma) / alpha;
            *r *= m;
            *g *= m;
            *b *= m;
        }
    }
    if p.mod_contrast.abs() > EPSILON {
        let gamma = 1.2 / (1.2 - p.mod_contrast * 0.5);
        *r = (*r - 127.5) * gamma + 127.5;
        *g = (*g - 127.5) * gamma + 127.5;
        *b = (*b - 127.5) * gamma + 127.5;
    }
    if p.mod_saturation.abs() > EPSILON {
        let avg = *r * 0.299 + *g * 0.588 + *b * 0.113;
        *r += (*r - avg) * p.mod_saturation;
        *g += (*g - avg) * p.mod_saturation;
        *b += (*b - avg) * p.mod_saturation;
    }
    if p.mod_hue.abs() > EPSILON {
        let (h, s, l) = rgb_to_hsl(*r / 255.0, *g / 255.0, *b / 255.0);
        let (r2, g2, b2) = hsl_to_rgb(h + p.mod_hue, s, l);
        *r = round_color(r2 * 255.0) as f64;
        *g = round_color(g2 * 255.0) as f64;
        *b = round_color(b2 * 255.0) as f64;
    }
}
