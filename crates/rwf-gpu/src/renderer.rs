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

//! Progressive wgpu renderer: iterate kernel -> integer histogram -> float accumulation ->
//! tonemap into an RGBA8 storage texture.

use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use rwf_core::model::{BgColorType, Flame, PostSymmetryType};
use rwf_core::render::filter::{FilterKernel, FilterMatrix};
use rwf_core::render::tonemap::AdaptiveFilter;
use rwf_core::render::tonemap::ToneParams;
use rwf_core::render::{PreparedFlame, View};

use crate::codegen::Program;
use crate::state::RenderState;

/// Fixed-point scale for colors in the integer histogram.
const COLOR_SCALE: f32 = 8.0;
const WORKGROUP: u32 = 64;
const STATE_SIZE: u64 = 48;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct IterUniforms {
    cosa: f32,
    sina: f32,
    rc_x: f32,
    rc_y: f32,
    bws: f32,
    bhs: f32,
    cam_w: f32,
    cam_h: f32,
    m0: [f32; 4],
    m1: [f32; 4],
    m2: [f32; 4],
    cam_pos: [f32; 4],
    cam_persp: f32,
    dimish_z: f32,
    dim_z_dist: f32,
    project_3d: u32,
    raster_w: u32,
    raster_h: u32,
    iters: u32,
    thread_count: u32,
    aa_amount: f32,
    aa_radius: f32,
    color_scale: f32,
    layer_count: u32,
    sym_cx: f32,
    sym_cy: f32,
    sym_halve_dist: f32,
    sym_order: u32,
    sym_rot_sin: f32,
    sym_rot_cos: f32,
    sym_do_rot: u32,
    _pad0: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ResolveUniforms {
    cells: u32,
    color_scale: f32,
    row: u32,
    _p: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DeUniforms {
    raster_w: u32,
    raster_h: u32,
    radius: i32,
    show_splitter: u32,
    curve: f32,
    splitter_pos: i32,
    _p0: u32,
    _p1: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ToneUniforms {
    image_w: u32,
    image_h: u32,
    raster_w: u32,
    raster_h: u32,
    os: u32,
    border: u32,
    fsize: u32,
    ssize: u32,
    fhalf: i32,
    shalf: i32,
    sharpen: u32,
    with_alpha: u32,
    k1: f32,
    k2: f32,
    bg_glow: f32,
    white_level: f32,
    bal: [f32; 4],
    gamma: f32,
    gamma_threshold: f32,
    scl_gamma: f32,
    vib_int: f32,
    inv_vib_int: f32,
    alpha_scale: f32,
    mod_sat: f32,
    bg_kind: u32,
    bg_ul: [f32; 4],
    bg_ur: [f32; 4],
    bg_ll: [f32; 4],
    bg_lr: [f32; 4],
    bg_cc: [f32; 4],
    bg_single: [f32; 4],
    smooth_offset: u32,
    mb_scl: f32,
    /// 0 = off, 1 = adaptive filter, 2 = adaptive filter indicator view.
    adaptive: u32,
    ad_sharpness: f32,
    ad_low_density: f32,
    low_off: u32,
    soft_off: u32,
    detail_off: u32,
    low_size: u32,
    soft_size: u32,
    detail_size: u32,
    low_half: i32,
    soft_half: i32,
    detail_half: i32,
    low_radius: f32,
    soft_radius: f32,
    detail_radius: f32,
    prim_radius: f32,
    /// Channel mixer: 0 off, 1 brightness, 2 RGB, 3 full.
    mix_mode: u32,
    _q2: u32,
    /// Background region: offset and size of the full image (quilt tiles).
    region: [u32; 4],
}

/// A part of a larger image (quilt rendering): the renderer then shows the `width` × `height`
/// pixels at `(x, y)` of the full image, with the full image's geometry, background and
/// brightness. The tile must lie inside the full image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    pub full_width: u32,
    pub full_height: u32,
    pub x: u32,
    pub y: u32,
}

/// Background xform colors with the flame, image size and tile they were computed for.
struct CachedBackdrop {
    flame: Flame,
    width: u32,
    height: u32,
    tile: Option<Tile>,
    colors: Vec<[i32; 3]>,
}

/// A wgpu device + queue pair.
#[derive(Clone)]
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_name: String,
}

impl GpuContext {
    /// Creates a headless context on the best available adapter.
    pub fn new_headless() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let limits = Self::required_limits(&adapter);
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rwf-gpu"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(|e| format!("request_device failed: {e}"))?;
        Ok(Self { device, queue, adapter_name: adapter.get_info().name })
    }

    /// Limits we want: the adapter's storage buffer limits (large rasters need big buffers).
    pub fn required_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
        let al = adapter.limits();
        wgpu::Limits {
            max_storage_buffer_binding_size: al.max_storage_buffer_binding_size,
            max_buffer_size: al.max_buffer_size,
            max_storage_buffers_per_shader_stage: al.max_storage_buffers_per_shader_stage.max(8),
            max_compute_workgroups_per_dimension: al.max_compute_workgroups_per_dimension,
            ..wgpu::Limits::default()
        }
    }
}

/// Number of compiled pipelines kept besides the current one.
const PIPELINE_CACHE_SIZE: usize = 12;

struct Pipelines {
    signature: String,
    iterate: wgpu::ComputePipeline,
}

struct Buffers {
    raster_w: u32,
    raster_h: u32,
    image_w: u32,
    image_h: u32,
    hist: wgpu::Buffer,
    accum: wgpu::Buffer,
    de: wgpu::Buffer,
    /// Depth keys of classic rendering (only if the z-buffer is enabled).
    zbuf: Option<wgpu::Buffer>,
    output: wgpu::Texture,
    output_view: wgpu::TextureView,
}

pub struct GpuRenderer {
    ctx: GpuContext,
    resolve_pipeline: wgpu::ComputePipeline,
    tonemap_pipeline: wgpu::ComputePipeline,
    tonemap_hdr_pipeline: wgpu::ComputePipeline,
    de_pipeline: wgpu::ComputePipeline,
    de_uniforms: wgpu::Buffer,
    pipelines: Option<Pipelines>,
    /// Recently used pipelines (most recent last), so switching back to a flame does not
    /// compile its shader again.
    pipeline_cache: Vec<Pipelines>,
    buffers: Option<Buffers>,
    iter_uniforms: wgpu::Buffer,
    resolve_uniforms: wgpu::Buffer,
    tone_uniforms: wgpu::Buffer,
    mixer_lut: wgpu::Buffer,
    /// Background colors of the background xforms (three `i32` per pixel).
    backdrop_buf: wgpu::Buffer,
    backdrop: Option<CachedBackdrop>,
    prm: Option<wgpu::Buffer>,
    nxt: Option<wgpu::Buffer>,
    cmap: Option<wgpu::Buffer>,
    filter: Option<wgpu::Buffer>,
    states: wgpu::Buffer,
    thread_count: u32,
    flame: Option<Flame>,
    view: Option<View>,
    /// With a tile: the view of the full image (normalizes density and brightness).
    full_view: Option<View>,
    tile: Option<Tile>,
    program: Option<Program>,
    samples: u64,
    needs_state_reset: bool,
    last_error: Option<String>,
    frame_seed: u32,
    with_alpha: bool,
    tone_base: Option<ToneUniforms>,
    output_generation: u64,
    /// Number of flame states accumulated into the histogram (motion blur packets).
    packets: u32,
    solid: crate::solid::SolidGpu,
    /// Record depths for [`GpuRenderer::read_zbuffer`].
    zbuffer: bool,
    subflames: crate::subflames::SubFlames,
}

fn storage_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(16),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn set_background(t: &mut ToneUniforms, flame: &Flame) {
    t.bg_kind = match flame.background.kind {
        BgColorType::SingleColor => 0,
        BgColorType::Gradient2x2 => 1,
        BgColorType::Gradient2x2C => 2,
    };
    t.bg_ul = c4(flame.background.ul);
    t.bg_ur = c4(flame.background.ur);
    t.bg_ll = c4(flame.background.ll);
    t.bg_lr = c4(flame.background.lr);
    t.bg_cc = c4(flame.background.cc);
    t.bg_single = c4(flame.background.color);
}

fn c4(c: [u8; 3]) -> [f32; 4] {
    [c[0] as f32, c[1] as f32, c[2] as f32, 0.0]
}

impl GpuRenderer {
    pub fn new(ctx: GpuContext) -> Self {
        let device = &ctx.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tonemap"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("tonemap.wgsl"))),
        });
        let mk = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let resolve_pipeline = mk("resolve");
        let tonemap_pipeline = mk("tonemap");
        let tonemap_hdr_pipeline = mk("tonemap_hdr");
        let de_pipeline = mk("density_estimate");
        let thread_count = 64 * 1024;
        Self {
            iter_uniforms: uniform_buffer(device, "iter-uniforms", std::mem::size_of::<IterUniforms>() as u64),
            resolve_uniforms: uniform_buffer(device, "resolve-uniforms", std::mem::size_of::<ResolveUniforms>() as u64),
            tone_uniforms: uniform_buffer(device, "tone-uniforms", std::mem::size_of::<ToneUniforms>() as u64),
            mixer_lut: storage_buffer(device, "mixer-lut", (9 * rwf_core::mixer::LUT_SIZE * 4) as u64),
            backdrop_buf: storage_buffer(device, "backdrop", 16),
            backdrop: None,
            states: storage_buffer(device, "states", thread_count as u64 * STATE_SIZE),
            de_uniforms: uniform_buffer(device, "de-uniforms", std::mem::size_of::<DeUniforms>() as u64),
            resolve_pipeline,
            tonemap_pipeline,
            tonemap_hdr_pipeline,
            de_pipeline,
            pipelines: None,
            pipeline_cache: Vec::new(),
            buffers: None,
            prm: None,
            nxt: None,
            cmap: None,
            filter: None,
            thread_count,
            flame: None,
            view: None,
            full_view: None,
            tile: None,
            program: None,
            samples: 0,
            needs_state_reset: true,
            last_error: None,
            frame_seed: 1,
            with_alpha: false,
            tone_base: None,
            output_generation: 0,
            packets: 1,
            solid: crate::solid::SolidGpu::new(),
            zbuffer: false,
            subflames: Default::default(),
            ctx,
        }
    }

    pub fn context(&self) -> &GpuContext {
        &self.ctx
    }

    pub fn samples(&self) -> u64 {
        self.samples
    }

    /// Samples per raster cell; with a tile relative to the full image, so a tile reaches the
    /// quality of a full render when it has as many samples as that render.
    pub fn density(&self) -> f64 {
        match self.full_view.as_ref().or(self.view.as_ref()) {
            Some(v) => self.samples as f64 / (v.raster_width as f64 * v.raster_height as f64) * v.oversample as f64 / self.packets as f64,
            None => 0.0,
        }
    }

    pub fn view(&self) -> Option<&View> {
        self.view.as_ref()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn unsupported_variations(&self) -> &[String] {
        self.program.as_ref().map(|p| p.unsupported.as_slice()).unwrap_or(&[])
    }

    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.buffers.as_ref().map(|b| &b.output_view)
    }

    pub fn output_size(&self) -> Option<(u32, u32)> {
        self.buffers.as_ref().map(|b| (b.image_w, b.image_h))
    }

    /// Incremented whenever the output texture is recreated (re-register it with the UI).
    pub fn output_generation(&self) -> u64 {
        self.output_generation
    }

    pub fn set_with_alpha(&mut self, with_alpha: bool) {
        self.with_alpha = with_alpha;
    }

    /// Sets the flame to render at the given output size. Restarts accumulation when anything
    /// relevant changed; recompiles the kernel only on structural changes.
    /// Enables the z-buffer of classic rendering (solid rendering always has one); applies to
    /// the next [`GpuRenderer::set_flame`].
    pub fn set_zbuffer(&mut self, on: bool) {
        if self.zbuffer != on {
            self.zbuffer = on;
            self.flame = None;
        }
    }

    /// Renders only a tile of a larger image (`None`: the whole image); applies to the next
    /// [`GpuRenderer::set_flame`], whose size is then the tile size.
    pub fn set_tile(&mut self, tile: Option<Tile>) {
        if self.tile != tile {
            self.tile = tile;
            self.flame = None;
        }
    }

    /// The view of the full image (the tile's image with [`GpuRenderer::set_tile`]).
    pub fn full_view(&self) -> Option<&View> {
        self.full_view.as_ref().or(self.view.as_ref())
    }

    pub fn set_flame(&mut self, flame: &Flame, width: u32, height: u32) {
        self.update_flame(flame, width, height, true);
    }

    /// Switches to another flame state *without* clearing the histogram (motion blur
    /// sub-frames). The output size must stay the same.
    pub fn add_packet(&mut self, flame: &Flame) {
        let Some((w, h)) = self.output_size() else { return };
        self.update_flame(flame, w, h, false);
        self.packets += 1;
    }

    fn update_flame(&mut self, flame: &Flame, width: u32, height: u32, reset: bool) {
        let width = width.max(1);
        let height = height.max(1);
        let same_size = self.buffers.as_ref().is_some_and(|b| b.image_w == width && b.image_h == height);
        if same_size && self.flame.as_ref() == Some(flame) {
            return;
        }
        if same_size && self.tone_base.is_some() && self.flame.as_ref().is_some_and(|f| rwf_core::render::same_iteration(f, flame)) {
            // Tonemapping-only change: keep accumulating.
            if let Some(t) = self.tone_base.as_mut() {
                set_background(t, flame);
            }
            self.flame = Some(flame.clone());
            return;
        }
        let device = &self.ctx.device;
        let mut prepared = PreparedFlame::new(flame);
        let sub_ids = crate::subflames::assign_slots(&mut prepared);
        let filter_size = flame.filter_kernel.filter_size(flame.spatial_filter_radius, flame.spatial_oversampling.max(1));
        let (view, full_view) = match self.tile {
            Some(t) => {
                let full = View::new(flame, t.full_width, t.full_height, filter_size);
                (full.window(t.x as i32, t.y as i32, width, height), Some(full))
            }
            None => (View::new(flame, width, height, filter_size), None),
        };
        let program = Program::new_scaled(flame, &prepared, view.ppu_scale, self.zbuffer);

        // Pipeline
        if self.pipelines.as_ref().map(|p| p.signature.as_str()) != Some(program.signature.as_str()) {
            if let Some(old) = self.pipelines.take() {
                self.pipeline_cache.push(old);
                if self.pipeline_cache.len() > PIPELINE_CACHE_SIZE {
                    self.pipeline_cache.remove(0);
                }
            }
            if let Some(i) = self.pipeline_cache.iter().position(|p| p.signature == program.signature) {
                self.pipelines = Some(self.pipeline_cache.remove(i));
                self.last_error = None;
            }
        }
        if self.pipelines.as_ref().map(|p| p.signature.as_str()) != Some(program.signature.as_str()) {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("iterate"),
                source: wgpu::ShaderSource::Wgsl(Cow::Owned(program.wgsl.clone())),
            });
            let iterate = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("iterate"),
                layout: None,
                module: &module,
                entry_point: Some("iterate"),
                compilation_options: Default::default(),
                cache: None,
            });
            if let Some(err) = pollster::block_on(scope.pop()) {
                self.last_error = Some(format!("shader compilation failed: {err}"));
                self.pipelines = None;
            } else {
                self.last_error = None;
                self.pipelines = Some(Pipelines { signature: program.signature.clone(), iterate });
            }
        }

        let sub_error = self.subflames.update(device, &self.ctx.queue, &sub_ids, program.uses_subflames);
        if sub_error.is_some() && self.last_error.is_none() {
            self.last_error = sub_error;
        }

        // Raster/output buffers
        let mut need_new = !self
            .buffers
            .as_ref()
            .is_some_and(|b| b.raster_w == view.raster_width && b.raster_h == view.raster_height && b.image_w == width && b.image_h == height);
        if need_new {
            let cells = view.raster_width as u64 * view.raster_height as u64;
            let output = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("flame-output"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let output_view = output.create_view(&Default::default());
            self.output_generation += 1;
            self.buffers = Some(Buffers {
                raster_w: view.raster_width,
                raster_h: view.raster_height,
                image_w: width,
                image_h: height,
                hist: storage_buffer(device, "hist", cells * 16),
                accum: storage_buffer(device, "accum", cells * 16),
                de: storage_buffer(device, "de", cells * 16),
                zbuf: None,
                output,
                output_view,
            });
        }
        if let Some(b) = self.buffers.as_mut() {
            if !program.zbuffer {
                b.zbuf = None;
            } else if b.zbuf.is_none() {
                b.zbuf = Some(storage_buffer(device, "zbuf", b.raster_w as u64 * b.raster_h as u64 * 4));
                need_new = true;
            }
        }

        // Solid rendering buffers and shadow map placement
        if !flame.solid.enabled {
            self.solid.buffers = None;
            self.solid.frames.clear();
        } else {
            let maps = program.shadow_maps.iter().flatten().count();
            // Shadow maps are placed for the full image, so all tiles share them.
            let (sw, sh) = self.tile.map_or((width, height), |t| (t.full_width, t.full_height));
            self.solid.frames = if maps > 0 { rwf_core::render::cpu::solid_shadow_frames(flame, sw, sh) } else { Vec::new() };
            self.solid.ensure_buffers(
                device,
                view.raster_width,
                view.raster_height,
                width,
                height,
                flame.solid.lights.len(),
                maps,
                program.shadow_map_size,
            );
        }

        // Parameter data
        let mut prm = program.pack_params(flame, &prepared);
        program.pack_solid(&mut prm, flame, &self.solid.frames);
        let nxt = program.pack_next(&prepared);
        let cmap = program.pack_colormaps(&prepared);
        self.prm = Some(self.upload(&self.prm, "prm", bytemuck::cast_slice(&prm)));
        self.nxt = Some(self.upload(&self.nxt, "nxt", bytemuck::cast_slice(&nxt)));
        self.cmap = Some(self.upload(&self.cmap, "cmap", bytemuck::cast_slice(&cmap)));

        // Filter matrices
        let os = flame.spatial_oversampling.max(1);
        let primary = FilterMatrix::new(flame.filter_kernel, os, flame.spatial_filter_radius);
        let smoothing = FilterMatrix::new(FilterKernel::Gaussian, os, 0.75);
        let mut fdata: Vec<f32> = primary.data.iter().map(|v| *v as f32).collect();
        let smooth_offset = fdata.len() as u32;
        fdata.extend(smoothing.data.iter().map(|v| *v as f32));
        let adaptive = AdaptiveFilter::new(flame, os);
        let mut push = |m: Option<&FilterMatrix>| {
            let off = fdata.len() as u32;
            if let Some(m) = m {
                fdata.extend(m.data.iter().map(|v| *v as f32));
            }
            (off, m.map_or(0, |m| m.size as u32), m.map_or(0, |m| m.half as i32), m.map_or(0.0, |m| m.radius as f32))
        };
        let (low_off, low_size, low_half, low_radius) = push(adaptive.as_ref().map(|a| &a.low_density));
        let (soft_off, soft_size, soft_half, soft_radius) = push(adaptive.as_ref().map(|a| &a.soften));
        let (detail_off, detail_size, detail_half, detail_radius) = push(adaptive.as_ref().map(|a| &a.detail));
        fdata.push(0.0);
        self.filter = Some(self.upload(&self.filter, "filter", bytemuck::cast_slice(&fdata)));

        // Uniforms
        let ps = &flame.post_symmetry;
        let rot = ps.rotation * std::f64::consts::TAU / 180.0 / 2.0;
        let m = view.matrix;
        let iu = IterUniforms {
            cosa: view.cosa as f32,
            sina: view.sina as f32,
            rc_x: view.rc_x as f32,
            rc_y: view.rc_y as f32,
            bws: view.bws as f32,
            bhs: view.bhs as f32,
            cam_w: view.cam_w as f32,
            cam_h: view.cam_h as f32,
            m0: [m[0][0] as f32, m[0][1] as f32, m[0][2] as f32, 0.0],
            m1: [m[1][0] as f32, m[1][1] as f32, m[1][2] as f32, 0.0],
            m2: [m[2][0] as f32, m[2][1] as f32, m[2][2] as f32, 0.0],
            cam_pos: [view.cam_pos[0] as f32, view.cam_pos[1] as f32, view.cam_pos[2] as f32, 0.0],
            cam_persp: view.cam_perspective as f32,
            dimish_z: view.dimish_z as f32,
            dim_z_dist: view.dim_z_distance as f32,
            project_3d: view.project_3d as u32,
            raster_w: view.raster_width,
            raster_h: view.raster_height,
            iters: 64,
            thread_count: self.thread_count,
            aa_amount: flame.antialias_amount as f32,
            aa_radius: flame.antialias_radius as f32,
            color_scale: COLOR_SCALE,
            layer_count: program.layer_count as u32,
            sym_cx: ps.centre_x as f32,
            sym_cy: ps.centre_y as f32,
            sym_halve_dist: (ps.distance / 2.0) as f32,
            sym_order: if ps.kind == PostSymmetryType::Point { ps.order.clamp(1, 64) as u32 } else { 0 },
            sym_rot_sin: rot.sin() as f32,
            sym_rot_cos: rot.cos() as f32,
            sym_do_rot: (rot.abs() > 1e-8) as u32,
            _pad0: 0,
        };
        self.ctx.queue.write_buffer(&self.iter_uniforms, 0, bytemuck::bytes_of(&iu));

        let cells = view.raster_width * view.raster_height;
        let row = 65535 * 256;
        let ru = ResolveUniforms { cells, color_scale: COLOR_SCALE, row, _p: 0 };
        self.ctx.queue.write_buffer(&self.resolve_uniforms, 0, bytemuck::bytes_of(&ru));

        let tone = ToneUniforms {
            image_w: width,
            image_h: height,
            raster_w: view.raster_width,
            raster_h: view.raster_height,
            os,
            border: view.border,
            fsize: primary.size as u32,
            ssize: smoothing.size as u32,
            fhalf: primary.half as i32,
            shalf: smoothing.half as i32,
            sharpen: primary.kernel.is_sharpening() as u32,
            with_alpha: 0,
            k1: 0.0,
            k2: 0.0,
            bg_glow: 0.0,
            white_level: 0.0,
            bal: [0.0; 4],
            gamma: 0.0,
            gamma_threshold: 0.0,
            scl_gamma: 0.0,
            vib_int: 0.0,
            inv_vib_int: 0.0,
            alpha_scale: 0.0,
            mod_sat: 0.0,
            bg_kind: match flame.background.kind {
                BgColorType::SingleColor => 0,
                BgColorType::Gradient2x2 => 1,
                BgColorType::Gradient2x2C => 2,
            },
            bg_ul: c4(flame.background.ul),
            bg_ur: c4(flame.background.ur),
            bg_ll: c4(flame.background.ll),
            bg_lr: c4(flame.background.lr),
            bg_cc: c4(flame.background.cc),
            bg_single: c4(flame.background.color),
            smooth_offset,
            mb_scl: 1.0,
            adaptive: match &adaptive {
                None => 0,
                Some(a) if a.indicator => 2,
                Some(_) => 1,
            },
            ad_sharpness: flame.filter_sharpness as f32,
            ad_low_density: flame.filter_low_density as f32,
            low_off,
            soft_off,
            detail_off,
            low_size,
            soft_size,
            detail_size,
            low_half,
            soft_half,
            detail_half,
            low_radius,
            soft_radius,
            detail_radius,
            prim_radius: primary.radius as f32,
            mix_mode: 0,
            _q2: 0,
            region: self.region(width, height),
        };
        self.tone_base = Some(tone);

        self.flame = Some(flame.clone());
        self.view = Some(view);
        self.full_view = full_view;
        self.program = Some(program);
        if reset || need_new {
            self.reset_accumulation();
        }
    }

    /// Offset and full size for the background (the whole image without a tile).
    fn region(&self, width: u32, height: u32) -> [u32; 4] {
        match self.tile {
            Some(t) => [t.x, t.y, t.full_width, t.full_height],
            None => [0, 0, width, height],
        }
    }

    fn upload(&self, old: &Option<wgpu::Buffer>, label: &str, data: &[u8]) -> wgpu::Buffer {
        let size = (data.len() as u64).max(16).next_multiple_of(4);
        let buf = match old {
            Some(b) if b.size() >= size => b.clone(),
            _ => storage_buffer(&self.ctx.device, label, size),
        };
        self.ctx.queue.write_buffer(&buf, 0, data);
        buf
    }

    /// Clears the histogram and restarts the iteration (call after any change).
    pub fn reset_accumulation(&mut self) {
        self.samples = 0;
        self.packets = 1;
        self.needs_state_reset = true;
        if let Some(b) = &self.buffers {
            let mut enc = self.ctx.device.create_command_encoder(&Default::default());
            enc.clear_buffer(&b.hist, 0, None);
            enc.clear_buffer(&b.accum, 0, None);
            if let Some(z) = &b.zbuf {
                enc.clear_buffer(z, 0, None);
            }
            self.solid.clear(&mut enc);
            self.ctx.queue.submit(Some(enc.finish()));
        }
    }

    fn init_states(&mut self) {
        // Random start points; the kernel fuses them before plotting.
        let mut seed = 0x1234_5678u32 ^ self.frame_seed.wrapping_mul(0x9E37_79B9);
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let mut data: Vec<u32> = Vec::with_capacity(self.thread_count as usize * 12);
        for _ in 0..self.thread_count {
            let fx = (next() >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0;
            let fy = (next() >> 8) as f32 / 16_777_216.0 * 2.0 - 1.0;
            let fc = (next() >> 8) as f32 / 16_777_216.0;
            let fm = (next() >> 8) as f32 / 16_777_216.0;
            data.extend_from_slice(&[fx.to_bits(), fy.to_bits(), 0f32.to_bits(), fc.to_bits(), 0, 0, 0, 0, next() | 1, 30, 0, fm.to_bits()]);
        }
        self.ctx.queue.write_buffer(&self.states, 0, bytemuck::cast_slice(&data));
        self.frame_seed = self.frame_seed.wrapping_add(1);
        self.needs_state_reset = false;
    }

    /// Runs `dispatches` iterate passes of `iters` iterations per thread, then resolves the
    /// histogram. Returns the number of samples added.
    pub fn iterate(&mut self, dispatches: u32, iters: u32) -> u64 {
        let (Some(pipes), Some(bufs), Some(prm), Some(nxt), Some(cmap)) = (&self.pipelines, &self.buffers, &self.prm, &self.nxt, &self.cmap) else {
            return 0;
        };
        if self.program.as_ref().is_none_or(|p| p.layer_count == 0) {
            return 0;
        }
        let device = self.ctx.device.clone();
        let iterate = pipes.iterate.clone();
        let (hist, accum, zbuf) = (bufs.hist.clone(), bufs.accum.clone(), bufs.zbuf.clone());
        let (prm, nxt, cmap) = (prm.clone(), nxt.clone(), cmap.clone());
        let cells = bufs.raster_w as u64 * bufs.raster_h as u64;
        if self.needs_state_reset {
            self.init_states();
        }
        // Update the per-dispatch iteration count (offset of `iters` in IterUniforms).
        let iters_offset = std::mem::offset_of!(IterUniforms, iters) as u64;
        self.ctx.queue.write_buffer(&self.iter_uniforms, iters_offset, bytemuck::bytes_of(&iters));

        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: self.iter_uniforms.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: prm.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: nxt.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: cmap.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 4, resource: self.states.as_entire_binding() },
        ];
        let program = self.program.as_ref().expect("program");
        if let Some(points) = self.subflames.points().filter(|_| program.uses_subflames) {
            entries.push(wgpu::BindGroupEntry { binding: 9, resource: points.as_entire_binding() });
        }
        let sub_groups = self.subflames.bind_groups(&device);
        match (&self.solid.buffers, program.solid_base) {
            (Some(sb), Some(_)) => {
                entries.push(wgpu::BindGroupEntry { binding: 6, resource: sb.zbuf.as_entire_binding() });
                entries.push(wgpu::BindGroupEntry { binding: 7, resource: sb.sattr.as_entire_binding() });
                if program.shadow_maps.iter().any(Option::is_some) {
                    entries.push(wgpu::BindGroupEntry { binding: 8, resource: sb.smap.as_entire_binding() });
                }
            }
            _ => {
                entries.push(wgpu::BindGroupEntry { binding: 5, resource: hist.as_entire_binding() });
                if let Some(z) = zbuf.as_ref().filter(|_| program.zbuffer) {
                    entries.push(wgpu::BindGroupEntry { binding: 6, resource: z.as_entire_binding() });
                }
            }
        }
        let iter_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("iterate-bg"),
            layout: &iterate.get_bind_group_layout(0),
            entries: &entries,
        });
        let resolve_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("resolve-bg"),
            layout: &self.resolve_pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.resolve_uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: hist.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: accum.as_entire_binding() },
            ],
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("iterate") });
        let groups = self.thread_count.div_ceil(WORKGROUP);
        let resolve_groups = cells.div_ceil(256);
        let (rx, ry) = if resolve_groups > 65535 { (65535u32, resolve_groups.div_ceil(65535) as u32) } else { (resolve_groups as u32, 1) };
        for _ in 0..dispatches {
            crate::subflames::SubFlames::dispatch(&mut enc, &sub_groups);
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("iterate"), timestamp_writes: None });
                pass.set_pipeline(&iterate);
                pass.set_bind_group(0, &iter_bg, &[]);
                pass.dispatch_workgroups(groups, 1, 1);
            }
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("resolve"), timestamp_writes: None });
                pass.set_pipeline(&self.resolve_pipeline);
                pass.set_bind_group(0, &resolve_bg, &[]);
                pass.dispatch_workgroups(rx, ry, 1);
            }
        }
        self.ctx.queue.submit(Some(enc.finish()));
        let added = self.thread_count as u64 * iters as u64 * dispatches as u64;
        self.samples += added;
        added
    }

    /// Computes the background xform colors on the CPU if the flame has background xforms and
    /// they changed; returns true if the backdrop is in use.
    fn update_backdrop(&mut self) -> bool {
        let (Some(flame), Some(bufs)) = (&self.flame, &self.buffers) else {
            return false;
        };
        if !flame.has_bg_xforms() {
            self.backdrop = None;
            return false;
        }
        let (w, h) = (bufs.image_w, bufs.image_h);
        let tile = self.tile;
        if self
            .backdrop
            .as_ref()
            .is_some_and(|b| b.width == w && b.height == h && b.tile == tile && rwf_core::render::background::same_backdrop(&b.flame, flame))
        {
            return true;
        }
        let backdrop = match tile {
            Some(t) => rwf_core::render::background::Backdrop::new_region(flame, (t.full_width, t.full_height), (t.x, t.y, w, h)),
            None => rwf_core::render::background::Backdrop::new(flame, w, h),
        };
        let colors = backdrop.colors().map(<[_]>::to_vec).unwrap_or_default();
        let bytes: &[u8] = bytemuck::cast_slice(&colors);
        if self.backdrop_buf.size() < bytes.len() as u64 {
            self.backdrop_buf = storage_buffer(&self.ctx.device, "backdrop", bytes.len() as u64);
        }
        self.ctx.queue.write_buffer(&self.backdrop_buf, 0, bytes);
        self.backdrop = Some(CachedBackdrop { flame: flame.clone(), width: w, height: h, tile, colors });
        true
    }

    /// Writes the current image into the output texture.
    pub fn tonemap(&mut self) {
        self.tonemap_into(None);
    }

    /// Tonemaps into linear float RGB (`transformPointHDR`, 1.0 = 8-bit full scale, three
    /// values per pixel) and reads it back. Solid renders skip the post DOF.
    pub fn read_hdr(&mut self) -> Option<Vec<f32>> {
        let (w, h) = self.output_size()?;
        let size = w as u64 * h as u64 * 12;
        let buf = storage_buffer(&self.ctx.device, "hdr-out", size);
        self.tonemap_into(Some(&buf));
        let bytes = self.read_buffer(&buf, size)?;
        Some(bytemuck::pod_collect_to_vec(&bytes))
    }

    /// 16-bit gray depth image mapped by the flame's z-buffer settings (`None` unless the
    /// z-buffer was enabled with [`GpuRenderer::set_zbuffer`] or the flame is rendered solid).
    pub fn read_zbuffer(&self) -> Option<Vec<u16>> {
        let (flame, view, bufs) = (self.flame.as_ref()?, self.view.as_ref()?, self.buffers.as_ref()?);
        let zbuf = match &self.solid.buffers {
            Some(sb) if flame.solid.enabled => &sb.zbuf,
            _ => bufs.zbuf.as_ref()?,
        };
        let bytes = self.read_buffer(zbuf, bufs.raster_w as u64 * bufs.raster_h as u64 * 4)?;
        let keys: Vec<u32> = bytemuck::pod_collect_to_vec(&bytes);
        // Inverse of the shaders' `zkey`; 0 marks an empty cell.
        let decode = |k: u32| match k {
            0 => None,
            k if k & 0x8000_0000 != 0 => Some(f32::from_bits(k & 0x7fff_ffff)),
            k => Some(f32::from_bits(!k)),
        };
        Some(rwf_core::render::cpu::zbuffer_image(flame, view, |x, y| decode(keys[(y * bufs.raster_w + x) as usize])))
    }

    /// Copies a storage buffer into host memory (blocking).
    fn read_buffer(&self, buf: &wgpu::Buffer, size: u64) -> Option<Vec<u8>> {
        let device = &self.ctx.device;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(buf, 0, &staging, 0, size);
        self.ctx.queue.submit(Some(enc.finish()));
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().ok()?.to_vec();
        staging.unmap();
        Some(data)
    }

    /// Tonemaps into the output texture, or with `hdr` into that float buffer.
    fn tonemap_into(&mut self, hdr: Option<&wgpu::Buffer>) {
        let with_backdrop = self.update_backdrop();
        let (Some(flame), Some(view), Some(bufs), Some(filter)) = (&self.flame, &self.view, &self.buffers, &self.filter) else {
            return;
        };
        if flame.solid.enabled {
            let (flame, view, output) = (flame.clone(), view.clone(), bufs.output_view.clone());
            let backdrop = self.backdrop.as_ref().filter(|_| with_backdrop).map(|b| b.colors.as_slice());
            let region = self.region(view.image_width, view.image_height);
            self.solid.render(&self.ctx.device, &self.ctx.queue, &flame, &view, &output, self.with_alpha, backdrop, hdr, region);
            return;
        }
        let Some(mut tone) = self.tone_base else {
            return;
        };
        // Brightness of the full image (tiles: the area and density of the whole image).
        let norm = self.full_view.as_ref().unwrap_or(view);
        let tp = ToneParams::new(flame, norm.image_width, norm.image_height, view.ppu, self.density());
        tone.k1 = tp.k1 as f32;
        tone.k2 = tp.k2 as f32;
        tone.bg_glow = tp.bg_glow as f32;
        tone.white_level = tp.white_level as f32;
        tone.bal = [tp.balance[0] as f32, tp.balance[1] as f32, tp.balance[2] as f32, 0.0];
        tone.gamma = tp.gamma as f32;
        tone.gamma_threshold = tp.gamma_threshold as f32;
        tone.scl_gamma = tp.scl_gamma as f32;
        tone.vib_int = tp.vib_int as f32;
        tone.inv_vib_int = tp.inverse_vib_int as f32;
        tone.alpha_scale = tp.alpha_scale as f32;
        tone.mod_sat = tp.mod_saturation as f32;
        tone.with_alpha = self.with_alpha as u32;
        tone.mb_scl = 1.0 / self.packets.max(1) as f32;
        if with_backdrop {
            tone.bg_kind = 3;
        }
        tone.mix_mode = 0;
        if let Some(func) = flame.mixer.color_func() {
            tone.mix_mode = func.mode_index();
            self.ctx.queue.write_buffer(&self.mixer_lut, 0, bytemuck::cast_slice(&func.lut()));
        }
        self.ctx.queue.write_buffer(&self.tone_uniforms, 0, bytemuck::bytes_of(&tone));

        let device = &self.ctx.device;
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("tonemap") });
        // Optional density estimation: accum -> de, then tonemap reads de.
        let de = rwf_core::render::de::DeParams::new(flame, view.ppu_scale);
        let source = match de {
            Some(p) => {
                let show = p.splitter > 1e-8 && p.splitter < 1.0 - 1e-8;
                let du = DeUniforms {
                    raster_w: bufs.raster_w,
                    raster_h: bufs.raster_h,
                    radius: p.radius,
                    show_splitter: show as u32,
                    curve: p.curve as f32,
                    splitter_pos: (p.splitter * bufs.raster_w as f64 + 0.5) as i32,
                    _p0: 0,
                    _p1: 0,
                };
                self.ctx.queue.write_buffer(&self.de_uniforms, 0, bytemuck::bytes_of(&du));
                let de_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("de-bg"),
                    layout: &self.de_pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: self.de_uniforms.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: bufs.accum.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 2, resource: bufs.de.as_entire_binding() },
                    ],
                });
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("density-estimate"), timestamp_writes: None });
                pass.set_pipeline(&self.de_pipeline);
                pass.set_bind_group(0, &de_bg, &[]);
                pass.dispatch_workgroups(bufs.raster_w.div_ceil(16), bufs.raster_h.div_ceil(16), 1);
                &bufs.de
            }
            None => &bufs.accum,
        };
        let pipeline = if hdr.is_some() { &self.tonemap_hdr_pipeline } else { &self.tonemap_pipeline };
        let target = match hdr {
            Some(b) => wgpu::BindGroupEntry { binding: 6, resource: b.as_entire_binding() },
            None => wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&bufs.output_view) },
        };
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tonemap-bg"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.tone_uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: source.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: filter.as_entire_binding() },
                target,
                wgpu::BindGroupEntry { binding: 4, resource: self.mixer_lut.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 5, resource: self.backdrop_buf.as_entire_binding() },
            ],
        });

        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("tonemap"), timestamp_writes: None });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(bufs.image_w.div_ceil(16), bufs.image_h.div_ceil(16), 1);
        }
        self.ctx.queue.submit(Some(enc.finish()));
    }

    /// Blocks until all submitted GPU work is done.
    pub fn wait_idle(&self) {
        let _ = self.ctx.device.poll(wgpu::PollType::wait_indefinitely());
    }

    /// Reads the output texture back as tightly packed RGBA8.
    pub fn read_image(&self) -> Option<Vec<u8>> {
        let bufs = self.buffers.as_ref()?;
        let (w, h) = (bufs.image_w, bufs.image_h);
        let row = (w * 4).next_multiple_of(256);
        let device = &self.ctx.device;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: row as u64 * h as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &bufs.output, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.ctx.queue.submit(Some(enc.finish()));
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().ok()?;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            let start = y * row as usize;
            out.extend_from_slice(&data[start..start + (w * 4) as usize]);
        }
        drop(data);
        staging.unmap();
        Some(out)
    }
}

impl GpuRenderer {
    /// Captures the running accumulation (flame, settings and buffers, blocking readback) so
    /// it can be continued later with [`GpuRenderer::restore_state`].
    pub fn save_state(&self, elapsed_ms: u64) -> Option<RenderState> {
        let (flame, bufs) = (self.flame.as_ref()?, self.buffers.as_ref()?);
        let mut named: Vec<(&str, &wgpu::Buffer)> = vec![("states", &self.states)];
        match &self.solid.buffers {
            Some(sb) if flame.solid.enabled => named.extend([("sld-zbuf", &sb.zbuf), ("sld-attr", &sb.sattr), ("sld-shadowmaps", &sb.smap)]),
            _ => {
                named.push(("accum", &bufs.accum));
                if let Some(z) = &bufs.zbuf {
                    named.push(("zbuf", z));
                }
            }
        }
        let buffers = named.into_iter().map(|(n, b)| Some((n.to_string(), self.read_buffer(b, b.size())?))).collect::<Option<Vec<_>>>()?;
        Some(RenderState {
            flame: flame.clone(),
            width: bufs.image_w,
            height: bufs.image_h,
            with_alpha: self.with_alpha,
            zbuffer: self.zbuffer,
            samples: self.samples,
            packets: self.packets,
            frame_seed: self.frame_seed,
            elapsed_ms,
            buffers,
        })
    }

    /// Sets up the saved flame and continues its accumulation.
    pub fn restore_state(&mut self, st: &RenderState) -> Result<(), String> {
        self.set_with_alpha(st.with_alpha);
        self.set_zbuffer(st.zbuffer);
        self.flame = None;
        self.set_flame(&st.flame, st.width, st.height);
        if let Some(e) = self.last_error() {
            return Err(e.to_string());
        }
        let bufs = self.buffers.as_ref().ok_or("no render buffers")?;
        let solid = self.solid.buffers.as_ref().filter(|_| st.flame.solid.enabled);
        let mut fresh_states = true;
        for (name, data) in &st.buffers {
            let target = match (name.as_str(), solid) {
                ("states", _) => &self.states,
                ("accum", None) => &bufs.accum,
                ("zbuf", None) => bufs.zbuf.as_ref().ok_or("z-buffer missing")?,
                ("sld-zbuf", Some(sb)) => &sb.zbuf,
                ("sld-attr", Some(sb)) => &sb.sattr,
                ("sld-shadowmaps", Some(sb)) => &sb.smap,
                _ => return Err(format!("unexpected buffer `{name}` in render state")),
            };
            if target.size() != data.len() as u64 {
                if name == "states" {
                    // Other thread count: restart the iteration points, keep the image.
                    continue;
                }
                return Err(format!("buffer `{name}` does not fit the flame ({} instead of {} bytes)", data.len(), target.size()));
            }
            self.ctx.queue.write_buffer(target, 0, data);
            fresh_states &= name != "states";
        }
        self.samples = st.samples;
        self.packets = st.packets.max(1);
        self.frame_seed = st.frame_seed;
        self.needs_state_reset = fresh_states || st.samples == 0;
        Ok(())
    }
}

impl GpuRenderer {
    /// Renders one animation frame (incl. motion blur packets) to RGBA8, blocking.
    pub fn render_frame(&mut self, flame: &Flame, frame: f64, w: u32, h: u32, quality: f64) -> Result<Vec<u8>, String> {
        self.accumulate_frame(flame, frame, w, h, quality)?;
        self.tonemap();
        self.read_image().ok_or_else(|| "readback failed".to_string())
    }

    /// Iterates one animation frame (incl. motion blur packets) up to `quality`, blocking;
    /// read the result with [`GpuRenderer::tonemap`] + [`GpuRenderer::read_image`],
    /// [`GpuRenderer::read_hdr`] or [`GpuRenderer::read_zbuffer`].
    pub fn accumulate_frame(&mut self, flame: &Flame, frame: f64, w: u32, h: u32, quality: f64) -> Result<(), String> {
        self.accumulate_packets(&rwf_core::render::frame_packets(flame, frame), w, h, quality)
    }

    /// Iterates several flame states into one image up to `quality` (motion blur packets,
    /// `packets[0]` first), blocking.
    pub fn accumulate_packets(&mut self, packets: &[Flame], w: u32, h: u32, quality: f64) -> Result<(), String> {
        for (i, f) in packets.iter().enumerate() {
            if i == 0 {
                self.set_flame(f, w, h);
            } else {
                self.add_packet(f);
            }
            if let Some(e) = self.last_error() {
                return Err(e.to_string());
            }
            let view = self.view().cloned().ok_or("no view")?;
            let per_packet = (quality * view.raster_width as f64 * view.raster_height as f64 / view.oversample as f64) as u64;
            let target = per_packet * (i as u64 + 1);
            while self.samples() < target {
                self.iterate(4, 128);
                self.wait_idle();
            }
        }
        Ok(())
    }
}

/// Renders a flame to RGBA8 on a headless GPU context (blocking).
pub fn render_blocking(flame: &Flame, w: u32, h: u32, quality: f64, with_alpha: bool) -> Result<Vec<u8>, String> {
    let ctx = GpuContext::new_headless()?;
    let mut r = GpuRenderer::new(ctx);
    r.set_with_alpha(with_alpha);
    r.set_flame(flame, w, h);
    if !r.unsupported_variations().is_empty() {
        eprintln!("warning: variations without GPU support ignored: {}", r.unsupported_variations().join(", "));
    }
    r.render_frame(flame, 0.0, w, h, quality)
}
