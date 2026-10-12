//! Rendering: compiled flame, camera, CPU iteration and tonemapping.

pub mod cpu;
pub mod filter;
pub mod prepared;
pub mod tonemap;
pub mod view;

pub use cpu::{CpuRender, RenderOptions, render_image};
pub use prepared::PreparedFlame;
pub use view::View;
