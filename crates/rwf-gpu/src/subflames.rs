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

//! Sub-flames on the GPU (`subflame_wf`, `pre_subflame_wf`, `ringsubflame`, `glynns3subfl`):
//! every sub-flame used by a flame runs as its own chaos game in a separate kernel which
//! refills its slot of a shared point buffer before each iterate pass. The variations pick
//! random points of their slot (see `subflame.wgsl` in rwf-core).

use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use rwf_core::render::PreparedFlame;
use rwf_core::variations::subflame;

use crate::codegen::Program;

/// Points per sub-flame slot (`SUBFLAME_POINTS` in `subflame.wgsl`).
pub const SUBFLAME_POINTS: u32 = 262_144;
const POINT_SIZE: u64 = 48;
const PER_THREAD: u32 = 16;
const THREADS: u32 = SUBFLAME_POINTS / PER_THREAD;
const MAX_ITER: u32 = 100;
const WORKGROUP: u32 = 64;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SubUniforms {
    thread_count: u32,
    per_thread: u32,
    out_base: u32,
    max_iter: u32,
}

/// The compiled kernel and buffers of one sub-flame.
struct SubKernel {
    id: usize,
    pipeline: wgpu::ComputePipeline,
    uniforms: wgpu::Buffer,
    prm: wgpu::Buffer,
    nxt: wgpu::Buffer,
    cmap: wgpu::Buffer,
    states: wgpu::Buffer,
}

/// Replaces the sub-flame ids in the prepared data by slot indices (-1 = no sub-flame) and
/// returns the sub-flame ids in slot order.
pub fn assign_slots(prepared: &mut PreparedFlame) -> Vec<usize> {
    let mut ids = Vec::new();
    for layer in &mut prepared.layers {
        for xf in layer.xforms.iter_mut().chain(layer.finals.iter_mut()) {
            for s in xf.pre.iter_mut().chain(xf.normal.iter_mut()).chain(xf.post_vars.iter_mut()) {
                for i in subflame::id_indices(s.def.name, &s.data) {
                    let Some(v) = s.data.get_mut(i) else { continue };
                    let id = *v as usize;
                    *v = if id > 0 && subflame::sub_flame(id).is_some() {
                        ids.iter().position(|&x| x == id).unwrap_or_else(|| {
                            ids.push(id);
                            ids.len() - 1
                        }) as f64
                    } else {
                        -1.0
                    };
                }
            }
        }
    }
    ids
}

fn storage_init(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, data: &[u8]) -> wgpu::Buffer {
    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (data.len() as u64).max(16).next_multiple_of(4),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buf, 0, data);
    buf
}

/// Random start points (fused by the kernel before it writes points).
fn initial_states(seed: u32) -> Vec<u32> {
    let mut seed = 0x2545_F491u32 ^ seed.wrapping_mul(0x9E37_79B9);
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut data = Vec::with_capacity(THREADS as usize * 12);
    for _ in 0..THREADS {
        let fx = (next() >> 8) as f32 / 16_777_216.0 - 0.5;
        let fy = (next() >> 8) as f32 / 16_777_216.0 - 0.5;
        let fc = (next() >> 8) as f32 / 16_777_216.0;
        data.extend_from_slice(&[fx.to_bits(), fy.to_bits(), 0, fc.to_bits(), 0, 0, 0, 0, next() | 1, 30, 0, 0]);
    }
    data
}

/// The sub-flame kernels of the current flame and their shared point buffer.
#[derive(Default)]
pub struct SubFlames {
    kernels: Vec<SubKernel>,
    points: Option<wgpu::Buffer>,
}

impl SubFlames {
    /// Sets up the kernels for the sub-flames `ids` (slot order); kernels of sub-flames that
    /// are still in use keep their state. Returns an error message if a kernel failed to compile.
    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, ids: &[usize], needed: bool) -> Option<String> {
        let mut old = std::mem::take(&mut self.kernels);
        let mut error = None;
        for (slot, &id) in ids.iter().enumerate() {
            let kernel = match old.iter().position(|k| k.id == id) {
                Some(i) => Some(old.swap_remove(i)),
                None => match Self::build(device, queue, id) {
                    Ok(k) => Some(k),
                    Err(e) => {
                        error = Some(e);
                        None
                    }
                },
            };
            if let Some(k) = kernel {
                let u = SubUniforms { thread_count: THREADS, per_thread: PER_THREAD, out_base: slot as u32 * SUBFLAME_POINTS, max_iter: MAX_ITER };
                queue.write_buffer(&k.uniforms, 0, bytemuck::bytes_of(&u));
                self.kernels.push(k);
            }
        }
        let size = ids.len().max(1) as u64 * SUBFLAME_POINTS as u64 * POINT_SIZE;
        if !needed && ids.is_empty() {
            self.points = None;
        } else if self.points.as_ref().is_none_or(|b| b.size() < size) {
            // Zeroed: points of slots without a kernel stay invalid.
            self.points = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("subflame-points"),
                size,
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            }));
        }
        error
    }

    fn build(device: &wgpu::Device, queue: &wgpu::Queue, id: usize) -> Result<SubKernel, String> {
        let sub = subflame::sub_flame(id).ok_or_else(|| format!("sub-flame {id} not found"))?;
        let program = Program::new_sub(&sub.flame);
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("subflame"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(program.wgsl.clone())),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("subflame"),
            layout: None,
            module: &module,
            entry_point: Some("sub_iterate"),
            compilation_options: Default::default(),
            cache: None,
        });
        if let Some(err) = pollster::block_on(scope.pop()) {
            return Err(format!("sub-flame shader compilation failed: {err}"));
        }
        let prm = program.pack_layers(&sub.flame);
        let nxt = program.pack_next(&sub.flame);
        let cmap = program.pack_colormaps(&sub.flame);
        Ok(SubKernel {
            id,
            pipeline,
            uniforms: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("subflame-uniforms"),
                size: std::mem::size_of::<SubUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            prm: storage_init(device, queue, "subflame-prm", bytemuck::cast_slice(&prm)),
            nxt: storage_init(device, queue, "subflame-nxt", bytemuck::cast_slice(&nxt)),
            cmap: storage_init(device, queue, "subflame-cmap", bytemuck::cast_slice(&cmap)),
            states: storage_init(device, queue, "subflame-states", bytemuck::cast_slice(&initial_states(id as u32))),
        })
    }

    /// The point buffer (binding 9 of the iterate kernel), if a sub-flame variation is in use.
    pub fn points(&self) -> Option<&wgpu::Buffer> {
        self.points.as_ref()
    }

    /// Bind groups of the kernels (create once per batch of dispatches).
    pub fn bind_groups(&self, device: &wgpu::Device) -> Vec<(wgpu::ComputePipeline, wgpu::BindGroup)> {
        let Some(points) = &self.points else { return Vec::new() };
        self.kernels
            .iter()
            .map(|k| {
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("subflame-bg"),
                    layout: &k.pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: k.uniforms.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 1, resource: k.prm.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 2, resource: k.nxt.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 3, resource: k.cmap.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 4, resource: k.states.as_entire_binding() },
                        wgpu::BindGroupEntry { binding: 5, resource: points.as_entire_binding() },
                    ],
                });
                (k.pipeline.clone(), bg)
            })
            .collect()
    }

    /// Records one refill of all slots.
    pub fn dispatch(enc: &mut wgpu::CommandEncoder, groups: &[(wgpu::ComputePipeline, wgpu::BindGroup)]) {
        if groups.is_empty() {
            return;
        }
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("subflames"), timestamp_writes: None });
        for (pipeline, bg) in groups {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, bg, &[]);
            pass.dispatch_workgroups(THREADS.div_ceil(WORKGROUP), 1, 1);
        }
    }
}
