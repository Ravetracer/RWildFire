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

//! GPU solid rendering: buffers and passes after the iteration (normals, ambient occlusion,
//! shadows, shading, post DOF). The iteration kernel writes the z-buffer, the point
//! attributes and the shadow maps (see `codegen::solid_plot_wgsl`).

use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use rwf_core::math::{EPSILON, ftoi};
use rwf_core::model::{BgColorType, Flame};
use rwf_core::render::View;
use rwf_core::render::filter::{FilterKernel, FilterMatrix};
use rwf_core::render::solid::{LightViews, ReflectionMap, ShadowFrame};
use rwf_core::solid::{ReflectionMapping, ShadowType};

const LIGHT_STRIDE: usize = 24;
const MAT_STRIDE: usize = 16;
const BOKEH_TABLE: usize = 2048;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct SolidUniforms {
    bg_ul: [f32; 4],
    bg_ur: [f32; 4],
    bg_ll: [f32; 4],
    bg_lr: [f32; 4],
    bg_cc: [f32; 4],
    bg_single: [f32; 4],
    bal: [f32; 4],
    raster_w: u32,
    raster_h: u32,
    image_w: u32,
    image_h: u32,
    os: u32,
    border: u32,
    fsize: u32,
    fhalf: i32,
    light_count: u32,
    mat_count: u32,
    ao_enabled: u32,
    shadows: u32,
    ao_int: f32,
    ao_affect: f32,
    gamma: f32,
    alpha_scale: f32,
    mod_sat: f32,
    with_alpha: u32,
    bg_kind: u32,
    seed: u32,
    ao_rsamples: u32,
    ao_asamples: u32,
    sphere_radius: f32,
    radius_step: f32,
    azimuth_step: f32,
    r_jitter: f32,
    ao_falloff: f32,
    ao_blur: u32,
    ao_bsize: u32,
    ao_bcenter: i32,
    smooth: u32,
    smooth_r: i32,
    smooth_dl: i32,
    bias: f32,
    map_size: u32,
    dof_on: u32,
    dof_amount: f32,
    bokeh_int: f32,
    bokeh_bright: f32,
    bokeh_size: f32,
    bokeh_act: f32,
    bokeh_rmax: f32,
    img_diag: f32,
    bokeh_support: f32,
    light_off: u32,
    mat_off: u32,
    filt_off: u32,
    aob_off: u32,
    smooth_off: u32,
    bokeh_off: u32,
    bg_off: u32,
    _p2: u32,
    /// Background region: offset and size of the full image (quilt tiles).
    region: [u32; 4],
}

/// Buffers shared with the iteration kernel (bindings 6..8 there).
pub(crate) struct SolidBuffers {
    raster_w: u32,
    raster_h: u32,
    image_w: u32,
    image_h: u32,
    lights: usize,
    maps: usize,
    map_size: usize,
    pub zbuf: wgpu::Buffer,
    pub sattr: wgpu::Buffer,
    pub smap: wgpu::Buffer,
    normals: wgpu::Buffer,
    ao_raw: wgpu::Buffer,
    ao: wgpu::Buffer,
    accel: wgpu::Buffer,
    vis: wgpu::Buffer,
    shaded: wgpu::Buffer,
    dofpix: wgpu::Buffer,
    dofacc: wgpu::Buffer,
}

struct Passes {
    normals: wgpu::ComputePipeline,
    ao: wgpu::ComputePipeline,
    ao_blur: wgpu::ComputePipeline,
    accel: wgpu::ComputePipeline,
    vis: wgpu::ComputePipeline,
    shade: wgpu::ComputePipeline,
    shade_hdr: wgpu::ComputePipeline,
    aux: wgpu::ComputePipeline,
    output: wgpu::ComputePipeline,
    dof_splat: wgpu::ComputePipeline,
    dof_resolve: wgpu::ComputePipeline,
}

pub(crate) struct SolidGpu {
    passes: Option<Passes>,
    uniforms: Option<wgpu::Buffer>,
    sp: Option<wgpu::Buffer>,
    refl: Option<wgpu::Buffer>,
    pub buffers: Option<SolidBuffers>,
    /// Shadow map placement (from a CPU warm-up iteration).
    pub frames: Vec<Option<ShadowFrame>>,
    seed: u32,
}

fn storage(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: size.max(16).next_multiple_of(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

fn c4(c: [u8; 3]) -> [f32; 4] {
    [c[0] as f32, c[1] as f32, c[2] as f32, 0.0]
}

impl SolidGpu {
    pub fn new() -> Self {
        Self { passes: None, uniforms: None, sp: None, refl: None, buffers: None, frames: Vec::new(), seed: 1 }
    }

    fn passes(&mut self, device: &wgpu::Device) -> &Passes {
        self.passes.get_or_insert_with(|| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("solid"),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("solid.wgsl"))),
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
            Passes {
                normals: mk("sld_normals"),
                ao: mk("sld_ao"),
                ao_blur: mk("sld_ao_blur"),
                accel: mk("sld_shadow_accel"),
                vis: mk("sld_shadow_vis"),
                shade: mk("sld_shade"),
                shade_hdr: mk("sld_shade_hdr"),
                aux: mk("sld_aux"),
                output: mk("sld_output"),
                dof_splat: mk("sld_dof_splat"),
                dof_resolve: mk("sld_dof_resolve"),
            }
        })
    }

    /// (Re)allocates the buffers for the given sizes. Returns true if they were recreated.
    #[allow(clippy::too_many_arguments)]
    pub fn ensure_buffers(&mut self, device: &wgpu::Device, raster_w: u32, raster_h: u32, image_w: u32, image_h: u32, lights: usize, maps: usize, map_size: usize) -> bool {
        if self.buffers.as_ref().is_some_and(|b| {
            b.raster_w == raster_w && b.raster_h == raster_h && b.image_w == image_w && b.image_h == image_h && b.lights == lights && b.maps == maps && b.map_size == map_size
        }) {
            return false;
        }
        let cells = raster_w as u64 * raster_h as u64;
        let pixels = image_w as u64 * image_h as u64;
        let l = lights.max(1) as u64;
        self.buffers = Some(SolidBuffers {
            raster_w,
            raster_h,
            image_w,
            image_h,
            lights,
            maps,
            map_size,
            zbuf: storage(device, "sld-zbuf", cells * 4),
            sattr: storage(device, "sld-attr", cells * 32),
            smap: storage(device, "sld-shadowmaps", (maps * map_size * map_size) as u64 * 4),
            normals: storage(device, "sld-normals", cells * 16),
            ao_raw: storage(device, "sld-ao-raw", cells * 4),
            ao: storage(device, "sld-ao", cells * 4),
            accel: storage(device, "sld-accel", l * cells * 4),
            vis: storage(device, "sld-vis", l * cells * 4),
            shaded: storage(device, "sld-shaded", pixels * 4),
            dofpix: storage(device, "sld-dof", pixels * 4),
            dofacc: storage(device, "sld-dofacc", pixels * 12),
        });
        true
    }

    /// Clears the z-buffer and the shadow maps (new accumulation).
    pub fn clear(&self, enc: &mut wgpu::CommandEncoder) {
        if let Some(b) = &self.buffers {
            enc.clear_buffer(&b.zbuf, 0, None);
            enc.clear_buffer(&b.smap, 0, None);
        }
    }

    /// Runs all passes and writes the final image into `output`, or, with `hdr_out`, linear
    /// float RGB (three `f32` per pixel, no post DOF) into that buffer.
    /// `backdrop`: background xform colors (three per pixel), if any.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        flame: &Flame,
        view: &View,
        output: &wgpu::TextureView,
        with_alpha: bool,
        backdrop: Option<&[[i32; 3]]>,
        hdr_out: Option<&wgpu::Buffer>,
        aux: bool,
        region: [u32; 4],
    ) {
        if self.buffers.is_none() {
            return;
        }
        self.passes(device);
        let s = &flame.solid;
        let b = self.buffers.as_ref().expect("solid buffers");
        let (rw, rh, iw, ih) = (b.raster_w, b.raster_h, b.image_w, b.image_h);

        // ---- parameter table ----
        let mut sp: Vec<f32> = Vec::new();
        let lights = LightViews::new(s);
        let light_off = sp.len() as u32;
        let mut map_idx = 0;
        for (li, l) in s.lights.iter().enumerate() {
            let mut rec = [0f32; LIGHT_STRIDE];
            let d = lights.dir[li];
            rec[0..3].copy_from_slice(&[d[0] as f32, d[1] as f32, d[2] as f32]);
            rec[3..6].copy_from_slice(&[l.color[0] as f32, l.color[1] as f32, l.color[2] as f32]);
            rec[6] = l.intensity as f32;
            rec[7] = l.cast_shadows as u8 as f32;
            if let Some(m) = lights.proj[li] {
                rec[8] = 1.0;
                rec[9] = map_idx as f32;
                for r in 0..3 {
                    for c in 0..3 {
                        rec[10 + r * 3 + c] = m[r][c] as f32;
                    }
                }
                if let Some(Some(f)) = self.frames.get(li) {
                    rec[19..23].copy_from_slice(&[f.scale_x as f32, f.scale_y as f32, f.centre_x as f32, f.centre_y as f32]);
                }
                map_idx += 1;
            }
            rec[23] = (1.0 - l.shadow_intensity.clamp(0.0, 1.0)) as f32;
            sp.extend_from_slice(&rec);
        }
        let mat_off = sp.len() as u32;
        let mut refl: Vec<u32> = Vec::new();
        for m in &s.materials {
            let mut rec = [0f32; MAT_STRIDE];
            rec[0] = m.ambient as f32;
            rec[1] = m.diffuse as f32;
            rec[2] = m.phong as f32;
            rec[3] = m.phong_size as f32;
            rec[4..7].copy_from_slice(&[m.phong_color[0] as f32, m.phong_color[1] as f32, m.phong_color[2] as f32]);
            rec[7] = m.light_diff_func.index() as f32;
            rec[8] = m.refl_map_intensity as f32;
            rec[9] = -1.0;
            if m.refl_map_intensity > EPSILON
                && !m.refl_map_filename.is_empty()
                && let Some(map) = ReflectionMap::load(&m.refl_map_filename)
            {
                rec[9] = refl.len() as f32;
                rec[10] = map.width as f32;
                rec[11] = map.height as f32;
                refl.extend(map.rgb.iter().map(|c| {
                    let q = |v: f64| (v * 255.0 + 0.5).clamp(0.0, 255.0) as u32;
                    q(c[0]) | (q(c[1]) << 8) | (q(c[2]) << 16)
                }));
            }
            rec[12] = if m.reflection_mapping == ReflectionMapping::Spherical { 0.0 } else { 1.0 };
            sp.extend_from_slice(&rec);
        }
        let os = flame.spatial_oversampling.max(1);
        let filt = FilterMatrix::new(flame.filter_kernel, os, flame.spatial_filter_radius);
        let filt_off = sp.len() as u32;
        sp.extend(filt.data.iter().map(|v| *v as f32));

        // AO
        let img_size = ((rw as f64).powi(2) + (rh as f64).powi(2)).sqrt();
        let search = s.ao_search_radius.clamp(0.25, 120.0);
        let blur = s.ao_blur_radius.clamp(0.25, 10.0);
        let rsamples = s.ao_radius_samples.clamp(1, 128);
        let asamples = s.ao_azimuth_samples.clamp(1, 128);
        let sphere_radius = search * img_size / 500.0;
        let radius_step = sphere_radius / rsamples as f64;
        let blur_radius = blur * img_size / 500.0;
        let do_blur = blur_radius >= 0.42;
        let aob = FilterMatrix::new(FilterKernel::Gaussian, 1, blur_radius);
        let aob_off = sp.len() as u32;
        if do_blur {
            // f[k][l]: k along x, l along y (symmetric kernel)
            sp.extend(aob.data.iter().map(|v| *v as f32));
        }

        // Shadow smoothing kernel
        let raw_smooth = if s.shadow_smooth_radius < EPSILON { 0.0 } else { s.shadow_smooth_radius };
        let smooth_r = (ftoi(raw_smooth * 6.0 * img_size / 1000.0)).min(128);
        let smooth = s.shadow_type == ShadowType::Smooth && smooth_r >= 1;
        let smooth_off = sp.len() as u32;
        if smooth {
            let inv = 1.0 / smooth_r as f64 * FilterKernel::Gaussian.spatial_support();
            for a in -smooth_r..=smooth_r {
                for c in -smooth_r..=smooth_r {
                    let r = ((a as f64 * inv).powi(2) + (c as f64 * inv).powi(2)).sqrt();
                    sp.push(FilterKernel::Gaussian.coeff(r) as f32);
                }
            }
        }

        // Bokeh kernel table
        let kernel = s.post_bokeh_filter_kernel;
        let rmax = 4.0 * kernel.spatial_support();
        let bokeh_off = sp.len() as u32;
        for k in 0..BOKEH_TABLE {
            sp.push(kernel.coeff(k as f64 / (BOKEH_TABLE - 1) as f64 * rmax) as f32);
        }
        // The backdrop shares the read-only u32 table with the reflection maps.
        let bg_off = refl.len() as u32;
        if let Some(b) = backdrop {
            refl.extend(b.iter().flatten().map(|&c| c as u32));
        }
        if refl.is_empty() {
            refl.push(0);
        }
        let sp_buf = upload(device, queue, &mut self.sp, "sld-params", bytemuck::cast_slice(&sp));
        let refl_buf = upload(device, queue, &mut self.refl, "sld-refl", bytemuck::cast_slice(&refl));

        // The bokeh scales with the full image (quilt tiles: not the tile).
        let pixel_diag = ((region[2] as f64).powi(2) + (region[3] as f64).powi(2)).sqrt();
        let dof_on = flame.cam_dof > EPSILON;
        self.seed = self.seed.wrapping_add(1);
        let u = SolidUniforms {
            bg_ul: c4(flame.background.ul),
            bg_ur: c4(flame.background.ur),
            bg_ll: c4(flame.background.ll),
            bg_lr: c4(flame.background.lr),
            bg_cc: c4(flame.background.cc),
            bg_single: c4(flame.background.color),
            bal: [flame.balance_red as f32, flame.balance_green as f32, flame.balance_blue as f32, 0.0],
            raster_w: rw,
            raster_h: rh,
            image_w: iw,
            image_h: ih,
            os,
            border: view.border,
            fsize: filt.size as u32,
            fhalf: filt.half as i32,
            light_count: s.lights.len() as u32,
            mat_count: s.materials.len() as u32,
            ao_enabled: s.ao_enabled as u32,
            shadows: s.with_shadows() as u32,
            ao_int: s.ao_intensity as f32,
            ao_affect: s.ao_affect_diffuse as f32,
            gamma: if flame.gamma == 0.0 { 0.0 } else { (1.0 / flame.gamma) as f32 },
            alpha_scale: (1.0 - (3.0 * (flame.fg_opacity - 1.0)).atan() / 1.25) as f32,
            mod_sat: (flame.saturation - 1.0).max(-1.0) as f32,
            with_alpha: with_alpha as u32,
            bg_kind: match flame.background.kind {
                _ if backdrop.is_some() => 3,
                BgColorType::SingleColor => 0,
                BgColorType::Gradient2x2 => 1,
                BgColorType::Gradient2x2C => 2,
            },
            seed: self.seed,
            ao_rsamples: rsamples,
            ao_asamples: asamples,
            sphere_radius: sphere_radius as f32,
            radius_step: radius_step as f32,
            azimuth_step: (std::f64::consts::TAU / asamples as f64) as f32,
            r_jitter: (radius_step / 10.0) as f32,
            ao_falloff: s.ao_falloff.clamp(0.0, 10.0) as f32,
            ao_blur: do_blur as u32,
            ao_bsize: aob.size as u32,
            ao_bcenter: aob.size as i32 / 2,
            smooth: smooth as u32,
            smooth_r,
            smooth_dl: smooth_r / 8 + 1,
            bias: s.shadowmap_bias as f32,
            map_size: b.map_size as u32,
            dof_on: dof_on as u32,
            dof_amount: if dof_on { (flame.cam_dof * img_size / (1000.0 * os as f64)) as f32 } else { 0.0 },
            bokeh_int: (s.post_bokeh_intensity * 1000.0 / pixel_diag) as f32,
            bokeh_bright: s.post_bokeh_brightness as f32,
            bokeh_size: s.post_bokeh_size as f32,
            bokeh_act: s.post_bokeh_activation as f32,
            bokeh_rmax: rmax as f32,
            img_diag: pixel_diag as f32,
            bokeh_support: kernel.spatial_support() as f32,
            light_off,
            mat_off,
            filt_off,
            aob_off,
            smooth_off,
            bokeh_off,
            bg_off,
            _p2: 0,
            region,
        };
        let ubuf = self.uniforms.get_or_insert_with(|| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sld-uniforms"),
                size: std::mem::size_of::<SolidUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        queue.write_buffer(ubuf, 0, bytemuck::bytes_of(&u));

        let p = self.passes.as_ref().expect("solid passes");
        let b = self.buffers.as_ref().expect("solid buffers");
        let ubuf = self.uniforms.as_ref().expect("solid uniforms");
        let res = |i: u32| -> wgpu::BindingResource<'_> {
            match i {
                0 => ubuf.as_entire_binding(),
                1 => b.zbuf.as_entire_binding(),
                2 => b.sattr.as_entire_binding(),
                3 => b.smap.as_entire_binding(),
                4 => b.normals.as_entire_binding(),
                5 => b.ao_raw.as_entire_binding(),
                6 => b.ao.as_entire_binding(),
                7 => b.accel.as_entire_binding(),
                8 => b.vis.as_entire_binding(),
                9 => sp_buf.as_entire_binding(),
                10 => b.shaded.as_entire_binding(),
                11 => b.dofpix.as_entire_binding(),
                12 => b.dofacc.as_entire_binding(),
                13 => wgpu::BindingResource::TextureView(output),
                15 => hdr_out.expect("hdr buffer").as_entire_binding(),
                _ => refl_buf.as_entire_binding(),
            }
        };
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("solid") });
        let run = |enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, bindings: &[u32], groups: (u32, u32, u32)| {
            let entries: Vec<wgpu::BindGroupEntry> = bindings.iter().map(|&i| wgpu::BindGroupEntry { binding: i, resource: res(i) }).collect();
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("solid-bg"), layout: &pipe.get_bind_group_layout(0), entries: &entries });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("solid"), timestamp_writes: None });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(groups.0, groups.1, groups.2);
        };
        let rg = (rw.div_ceil(16), rh.div_ceil(16), 1);
        let ig = (iw.div_ceil(16), ih.div_ceil(16), 1);
        let nl = (s.lights.len() as u32).max(1);
        run(&mut enc, &p.normals, &[0, 1, 2, 4], rg);
        if s.ao_enabled {
            run(&mut enc, &p.ao, &[0, 1, 4, 5], rg);
            run(&mut enc, &p.ao_blur, &[0, 5, 6, 9], rg);
        } else {
            enc.clear_buffer(&b.ao, 0, None);
        }
        if s.with_shadows() {
            run(&mut enc, &p.accel, &[0, 1, 2, 3, 7, 9], (rg.0, rg.1, nl));
            run(&mut enc, &p.vis, &[0, 7, 8, 9], (rg.0, rg.1, nl));
        }
        if aux && hdr_out.is_some() {
            run(&mut enc, &p.aux, &[0, 2, 4, 14, 15], ig);
        } else if hdr_out.is_some() {
            run(&mut enc, &p.shade_hdr, &[0, 2, 4, 6, 8, 9, 14, 15], ig);
        } else {
            run(&mut enc, &p.shade, &[0, 2, 4, 6, 8, 9, 10, 11, 14], ig);
            if dof_on {
                enc.clear_buffer(&b.dofacc, 0, None);
                run(&mut enc, &p.dof_splat, &[0, 9, 10, 11, 12], ig);
                run(&mut enc, &p.dof_resolve, &[0, 10, 12, 13], ig);
            } else {
                run(&mut enc, &p.output, &[0, 10, 13], ig);
            }
        }
        queue.submit(Some(enc.finish()));
    }
}

fn upload<'a>(device: &wgpu::Device, queue: &wgpu::Queue, slot: &'a mut Option<wgpu::Buffer>, label: &str, data: &[u8]) -> &'a wgpu::Buffer {
    let size = (data.len() as u64).max(16).next_multiple_of(4);
    if slot.as_ref().is_none_or(|b| b.size() < size) {
        *slot = Some(storage(device, label, size));
    }
    let buf = slot.as_ref().expect("buffer");
    queue.write_buffer(buf, 0, data);
    buf
}
