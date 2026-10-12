//! GPU (wgpu compute) renderer for flames.

pub mod codegen;

pub fn render_blocking(_flame: &jwf_core::Flame, _w: u32, _h: u32, _quality: f64, _alpha: bool) -> Result<Vec<u8>, String> {
    Err("GPU renderer not implemented yet".into())
}
