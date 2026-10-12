//! JWildfire (Rust) — desktop fractal flame editor.

mod app;
mod canvas;
mod doc;
mod export;
mod explorer;
mod gradients;
mod icons;
mod preview;
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
            label: Some("jwildfire device"),
            required_limits: wgpu::Limits {
                max_texture_dimension_2d: 8192.min(adapter.limits().max_texture_dimension_2d),
                ..jwf_gpu::GpuContext::required_limits(adapter)
            },
            ..Default::default()
        });
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("JWildfire")
            .with_inner_size([1480.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options,
        ..Default::default()
    };
    eframe::run_native("JWildfire", options, Box::new(move |cc| Ok(Box::new(app::JwfApp::new(cc, initial)))))
}
