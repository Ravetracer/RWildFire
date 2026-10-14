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

//! RWildFire — desktop fractal flame editor.

mod app;
mod audio_player;
mod batch_view;
mod canvas;
mod curve_editor;
mod dance_view;
mod doc;
mod export;
mod explorer;
mod gradients;
mod icons;
mod mesh_view;
mod movie_view;
mod mutagen;
mod preview;
mod quilt_view;
mod render_view;
mod theme;
mod timeline;
mod ui;

use std::sync::Arc;

fn main() -> eframe::Result {
    let initial = std::env::args().nth(1).map(std::path::PathBuf::from);

    let mut wgpu_options = egui_wgpu::WgpuConfiguration::default();
    if let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut wgpu_options.wgpu_setup {
        // The flame renderer needs large storage buffers (histograms of big images).
        setup.device_descriptor = Arc::new(|adapter: &wgpu::Adapter| wgpu::DeviceDescriptor {
            label: Some("rwildfire device"),
            required_limits: wgpu::Limits {
                max_texture_dimension_2d: 8192.min(adapter.limits().max_texture_dimension_2d),
                ..rwf_gpu::GpuContext::required_limits(adapter)
            },
            ..Default::default()
        });
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RWildFire")
            .with_inner_size([1480.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options,
        ..Default::default()
    };
    eframe::run_native("RWildFire", options, Box::new(move |cc| Ok(Box::new(app::RwfApp::new(cc, initial)))))
}
