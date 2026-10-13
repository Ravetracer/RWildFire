//! GPU (wgpu compute) renderer for flames.

pub mod codegen;
pub mod renderer;
mod solid;

pub use renderer::{GpuContext, GpuRenderer, render_blocking};
