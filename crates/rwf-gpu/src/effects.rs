// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
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

//! The image effects on the GPU, for the live preview: reads an RGBA8 texture (the
//! renderer's output) and writes the result into a texture of its own. Saved images use the
//! CPU reference ([`rwf_core::render::effects::apply`]) after the denoiser; both compute the
//! same within rounding.

use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use rwf_core::render::effects::{FLAG_CLARITY, FLAG_CURVES, FLAG_TEXTURE, FLAG_TONES, ImageEffects, LUT_SIZE, Prepared};

use crate::GpuContext;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FxUniforms {
    width: u32,
    height: u32,
    flags: u32,
    _p0: u32,
    exposure_gain: f32,
    contrast_pow: f32,
    highlights: f32,
    shadows: f32,
    black: f32,
    level_scale: f32,
    texture: f32,
    clarity: f32,
    dehaze: f32,
    vignette: f32,
    vig_lo: f32,
    vig_hi: f32,
    grain: f32,
    grain_cell: f32,
    _p1: f32,
    _p2: f32,
}

impl From<&Prepared> for FxUniforms {
    fn from(p: &Prepared) -> Self {
        Self {
            width: p.width,
            height: p.height,
            flags: p.flags,
            _p0: 0,
            exposure_gain: p.exposure_gain,
            contrast_pow: p.contrast_pow,
            highlights: p.highlights,
            shadows: p.shadows,
            black: p.black,
            level_scale: p.level_scale,
            texture: p.texture,
            clarity: p.clarity,
            dehaze: p.dehaze,
            vignette: p.vignette,
            vig_lo: p.vig_lo,
            vig_hi: p.vig_hi,
            grain: p.grain,
            grain_cell: p.grain_cell,
            _p1: 0.0,
            _p2: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlurUniforms {
    width: u32,
    height: u32,
    r: [u32; 3],
    _p: [u32; 3],
}

/// Buffers for one image size.
struct Buffers {
    width: u32,
    height: u32,
    /// Dark channel before the blur; also the bound but unused source of vertical blurs.
    raw: wgpu::Buffer,
    dark: wgpu::Buffer,
    lum: wgpu::Buffer,
    mask: wgpu::Buffer,
    texture: wgpu::Buffer,
    clarity: wgpu::Buffer,
    mid: wgpu::Buffer,
    t1: wgpu::Buffer,
    t2: wgpu::Buffer,
    output: wgpu::Texture,
    output_view: wgpu::TextureView,
}

pub struct EffectsPass {
    prep_dark: wgpu::ComputePipeline,
    base_lum: wgpu::ComputePipeline,
    finish: wgpu::ComputePipeline,
    blur_h: wgpu::ComputePipeline,
    blur_v: wgpu::ComputePipeline,
    uniforms: wgpu::Buffer,
    /// One per blur (dark channel, tone mask, texture, clarity): they run in one submission.
    blur_uniforms: [wgpu::Buffer; 4],
    lut: wgpu::Buffer,
    buffers: Option<Buffers>,
    generation: u64,
}

fn storage(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(16),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn uniform(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl EffectsPass {
    pub fn new(device: &wgpu::Device) -> Self {
        let pipelines = |label: &str, src: &'static str, entries: &[&str]| -> Vec<wgpu::ComputePipeline> {
            let module = device
                .create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(src)) });
            entries
                .iter()
                .map(|e| {
                    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                        label: Some(e),
                        layout: None,
                        module: &module,
                        entry_point: Some(e),
                        compilation_options: Default::default(),
                        cache: None,
                    })
                })
                .collect()
        };
        let mut fx = pipelines("effects", include_str!("effects.wgsl"), &["prep_dark", "base_lum", "finish"]).into_iter();
        let mut blur = pipelines("effects-blur", include_str!("effects_blur.wgsl"), &["blur_h", "blur_v"]).into_iter();
        let bsize = std::mem::size_of::<BlurUniforms>() as u64;
        Self {
            prep_dark: fx.next().unwrap(),
            base_lum: fx.next().unwrap(),
            finish: fx.next().unwrap(),
            blur_h: blur.next().unwrap(),
            blur_v: blur.next().unwrap(),
            uniforms: uniform(device, "effects-uniforms", std::mem::size_of::<FxUniforms>() as u64),
            blur_uniforms: std::array::from_fn(|_| uniform(device, "effects-blur-uniforms", bsize)),
            lut: storage(device, "effects-lut", (4 * LUT_SIZE * 4) as u64),
            buffers: None,
            generation: 0,
        }
    }

    /// The result of the last [`EffectsPass::run`].
    pub fn output_view(&self) -> Option<&wgpu::TextureView> {
        self.buffers.as_ref().map(|b| &b.output_view)
    }

    /// Changes when the output texture is recreated (for egui texture registration).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn ensure_buffers(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.buffers.as_ref().is_some_and(|b| (b.width, b.height) == (width, height)) {
            return;
        }
        let n = width as u64 * height as u64 * 4;
        let output = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("effects-output"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_view = output.create_view(&Default::default());
        self.buffers = Some(Buffers {
            width,
            height,
            raw: storage(device, "effects-raw", n),
            dark: storage(device, "effects-dark", n),
            lum: storage(device, "effects-lum", n),
            mask: storage(device, "effects-mask", n),
            texture: storage(device, "effects-texture", n),
            clarity: storage(device, "effects-clarity", n),
            mid: storage(device, "effects-mid", n),
            t1: storage(device, "effects-t1", n),
            t2: storage(device, "effects-t2", n),
            output,
            output_view,
        });
        self.generation += 1;
    }

    /// Applies `fx` to `input` (an RGBA8 texture of `width` × `height`) into
    /// [`EffectsPass::output_view`]. Returns false (and does nothing) if no effect is set.
    pub fn run(&mut self, ctx: &GpuContext, input: &wgpu::TextureView, width: u32, height: u32, fx: &ImageEffects) -> bool {
        let p = Prepared::new(fx, width, height);
        if p.flags == 0 || width == 0 || height == 0 {
            return false;
        }
        let device = &ctx.device;
        self.ensure_buffers(device, width, height);
        let b = self.buffers.as_ref().unwrap();
        ctx.queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&FxUniforms::from(&p)));
        if p.has(FLAG_CURVES) {
            ctx.queue.write_buffer(&self.lut, 0, bytemuck::cast_slice(&fx.curve_lut()));
        }
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("effects") });
        let groups = (width.div_ceil(16), height.div_ceil(16));
        let pixel_pass = |enc: &mut wgpu::CommandEncoder, pipeline: &wgpu::ComputePipeline, scalar_out: Option<&wgpu::Buffer>| {
            let layout = pipeline.get_bind_group_layout(0);
            let mut entries = vec![
                wgpu::BindGroupEntry { binding: 0, resource: self.uniforms.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(input) },
            ];
            match scalar_out {
                Some(out) => {
                    entries.push(wgpu::BindGroupEntry { binding: 2, resource: out.as_entire_binding() });
                    if std::ptr::eq(pipeline, &self.base_lum) {
                        entries.push(wgpu::BindGroupEntry { binding: 3, resource: b.dark.as_entire_binding() });
                    }
                }
                None => {
                    for (k, buf) in [(3, &b.dark), (4, &b.lum), (5, &b.mask), (6, &b.texture), (7, &b.clarity), (8, &self.lut)] {
                        entries.push(wgpu::BindGroupEntry { binding: k, resource: buf.as_entire_binding() });
                    }
                    entries.push(wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::TextureView(&b.output_view) });
                }
            }
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("effects-bg"), layout: &layout, entries: &entries });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("effects"), timestamp_writes: None });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(groups.0, groups.1, 1);
        };
        let blur = |enc: &mut wgpu::CommandEncoder, k: usize, from: &wgpu::Buffer, to: &wgpu::Buffer| {
            let u = BlurUniforms { width, height, r: p.boxes[k], _p: [0; 3] };
            ctx.queue.write_buffer(&self.blur_uniforms[k], 0, bytemuck::bytes_of(&u));
            for (pipeline, count) in [(&self.blur_h, height), (&self.blur_v, width)] {
                let entries = [
                    wgpu::BindGroupEntry { binding: 0, resource: self.blur_uniforms[k].as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: from.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: b.mid.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: b.t1.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: b.t2.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 5, resource: to.as_entire_binding() },
                ];
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("effects-blur-bg"),
                    layout: &pipeline.get_bind_group_layout(0),
                    entries: &entries,
                });
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("effects-blur"), timestamp_writes: None });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.dispatch_workgroups(count.div_ceil(64), 1, 1);
            }
        };
        if p.needs_dark() {
            pixel_pass(&mut enc, &self.prep_dark, Some(&b.raw));
            blur(&mut enc, 0, &b.raw, &b.dark);
        }
        if p.needs_lum() {
            pixel_pass(&mut enc, &self.base_lum, Some(&b.lum));
            for (flag, k, to) in [(FLAG_TONES, 1, &b.mask), (FLAG_TEXTURE, 2, &b.texture), (FLAG_CLARITY, 3, &b.clarity)] {
                if p.has(flag) {
                    blur(&mut enc, k, &b.lum, to);
                }
            }
        }
        pixel_pass(&mut enc, &self.finish, None);
        ctx.queue.submit(Some(enc.finish()));
        true
    }

    /// Reads the output texture back as RGBA8 (blocking).
    pub fn read_image(&self, ctx: &GpuContext) -> Option<Vec<u8>> {
        let b = self.buffers.as_ref()?;
        crate::renderer::read_texture_rgba8(ctx, &b.output, b.width, b.height)
    }
}
