//! Rendering: compiled flame, camera, CPU iteration and tonemapping.

pub mod analysis;
pub mod cpu;
pub mod de;
pub mod dof;
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
    f.animation = Default::default();
    f
}

/// True if `a` and `b` produce the same histogram (they may differ in tonemapping only).
pub fn same_iteration(a: &Flame, b: &Flame) -> bool {
    iteration_key(a) == iteration_key(b)
}

/// The flame states to render for an animation frame: the frame itself plus the motion blur
/// sub-frames (JWildfire `createRenderPackets`), each with scaled layer weights.
pub fn frame_packets(flame: &Flame, frame: f64) -> Vec<Flame> {
    let mut out = vec![flame.at_frame(frame)];
    let a = &flame.animation;
    if a.motion_blur_length > 0 {
        let len = a.motion_blur_length as f64;
        let mut t = frame + len * a.motion_blur_time_step / 2.0;
        for p in 1..=a.motion_blur_length {
            t -= a.motion_blur_time_step;
            let mut f = flame.at_frame(t);
            let scl = (1.0 - (p * p) as f64 * a.motion_blur_decay * 0.07 / len).max(0.01);
            for l in &mut f.layers {
                l.weight *= scl;
            }
            out.push(f);
        }
    }
    out
}
