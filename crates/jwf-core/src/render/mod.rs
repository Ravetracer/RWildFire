//! Rendering: compiled flame, camera, CPU iteration and tonemapping.

pub mod analysis;
pub mod cpu;
pub mod filter;
pub mod prepared;
pub mod tonemap;
pub mod view;

pub use cpu::{CpuRender, RenderOptions, render_image};
pub use prepared::PreparedFlame;
pub use view::View;

use crate::model::{Background, Flame};

/// Copy of `flame` with every value that only affects tonemapping neutralized.
fn iteration_key(flame: &Flame) -> Flame {
    let mut f = flame.clone();
    f.name.clear();
    f.brightness = 0.0;
    f.contrast = 0.0;
    f.gamma = 0.0;
    f.gamma_threshold = 0.0;
    f.vibrancy = 0.0;
    f.white_level = 0.0;
    f.saturation = 0.0;
    f.low_density_brightness = 0.0;
    f.balance_red = 0.0;
    f.balance_green = 0.0;
    f.balance_blue = 0.0;
    f.fg_opacity = 0.0;
    f.sample_density = 0.0;
    f.background = Background::default();
    f.bg_transparency = false;
    f.extra_attrs.clear();
    f
}

/// True if `a` and `b` produce the same histogram (they may differ in tonemapping only).
pub fn same_iteration(a: &Flame, b: &Flame) -> bool {
    iteration_key(a) == iteration_key(b)
}
