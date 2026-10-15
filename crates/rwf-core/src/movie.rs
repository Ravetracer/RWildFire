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

//! Movies from a sequence of flames (JWildfire's "Easy Movie Maker", `FlameMovie`): every
//! part shows a flame for some frames and fades or morphs into the next one during its last
//! frames; global and transform scripts add camera and transform motion on top.

use std::collections::HashSet;
use std::path::Path;

use crate::animation::FLAME_PROPS;
use crate::io::xml::{self, Element};
use crate::math::ftoi;
use crate::model::{Affine, Flame, Layer, VariationInstance, XForm};
use crate::variations;

/// Writes a sequence of flames as `<name>00001.flame`, `<name>00002.flame`, … next to `path`
/// (JWildfire's flame sequence output). `flame_at(i)` gives frame `i` of `total`; stops early
/// (with an error) when `cancel` is set; `on_frame(index, total)` after each file. Returns the
/// number of files.
pub fn write_flame_sequence(
    path: &Path,
    total: u32,
    cancel: &std::sync::atomic::AtomicBool,
    mut flame_at: impl FnMut(u32) -> Option<Flame>,
    mut on_frame: impl FnMut(u32, u32),
) -> Result<u32, String> {
    if total == 0 {
        return Err("no frames".into());
    }
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let stem = name.strip_suffix(".flame").unwrap_or(&name);
    let stem = if stem.is_empty() { "frame" } else { stem };
    let mut written = 0;
    for i in 0..total {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let Some(f) = flame_at(i) else { continue };
        let file = dir.join(format!("{stem}{:05}.flame", i + 1));
        crate::io::write_flame_to_file(&f, &file).map_err(|e| format!("{}: {e}", file.display()))?;
        written += 1;
        on_frame(i, total);
    }
    Ok(written)
}

/// JWildfire's random movie generators (`RandomMovieGeneratorList`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RandomMovie {
    #[default]
    All,
    RotatingMandelbrots,
    TransformingBubbles,
    TransformingDuckies,
}

impl RandomMovie {
    pub const ALL: [RandomMovie; 4] = [RandomMovie::All, RandomMovie::RotatingMandelbrots, RandomMovie::TransformingBubbles, RandomMovie::TransformingDuckies];

    pub fn label(self) -> &'static str {
        match self {
            RandomMovie::All => "All",
            RandomMovie::RotatingMandelbrots => "Rotating Mandelbrots",
            RandomMovie::TransformingBubbles => "Transforming Bubbles",
            RandomMovie::TransformingDuckies => "Transforming Duckies",
        }
    }

    /// A random movie: one part of 320 frames with camera and transform scripts
    /// (`RandomMovieGenerator.createMovie`).
    pub fn generate(self, rng: &mut crate::rng::Rng) -> Movie {
        use crate::random::{Extras, Generator, Quality, generate_quality};
        let kind = match self {
            RandomMovie::All => [RandomMovie::RotatingMandelbrots, RandomMovie::TransformingDuckies, RandomMovie::TransformingBubbles][rng.random_int(3)],
            k => k,
        };
        let script = |kind, amplitude| Script { kind, amplitude };
        let mut movie = Movie::default();
        let (generator, xform_script) = match kind {
            RandomMovie::TransformingDuckies => (Generator::Duckies, XFormScript::Rotate2nd),
            RandomMovie::TransformingBubbles => (Generator::Bubbles, XFormScript::RotateFirst),
            _ => (Generator::Mandelbrot, XFormScript::None),
        };
        let mut flame = generate_quality(generator, Extras::NONE, Quality::NORMAL, rng);
        if kind == RandomMovie::RotatingMandelbrots {
            flame.cam_perspective = 0.1 + 0.2 * rng.random();
            if rng.random() < 0.5 {
                flame.cam_dof = 0.05 + 0.1 * rng.random();
            }
            // JWildfire fills three slots and overwrites some of them; the last write wins.
            let mut slots = [Script::default(); 3];
            slots[0] = script(GlobalScript::RotateYaw, -0.75 * rng.random() * 1.5);
            slots[1] = script(GlobalScript::RotateRoll, -0.25 * rng.random() * 0.5);
            slots[2] = script(GlobalScript::RotatePitch, -0.125 * rng.random() * 0.25);
            slots[0] = script(GlobalScript::RotateBank, -0.75 * rng.random() * 1.5);
            slots[2] = script(GlobalScript::MoveCamZ, 0.2 + rng.random() * 0.6);
            slots[2] = script(GlobalScript::MoveCamY, 0.2 + rng.random() * 1.4);
            movie.global_scripts = slots.to_vec();
        } else {
            movie.global_scripts.push(script(GlobalScript::RotateRoll, -1.25 * rng.random() * 2.5));
            movie.global_scripts.push(script(GlobalScript::MoveCamX, rng.random() * 2.0 + 0.2));
            if rng.random() < 0.5 {
                movie.global_scripts.push(script(GlobalScript::MoveCamZ, rng.random() * 0.25));
            }
            movie.xform_scripts.push(Script { kind: xform_script, amplitude: -0.5 * rng.random() });
        }
        movie.parts.push(MoviePart { flame, frames: 320, morph_frames: 0, morph: MorphType::Fade });
        movie.name = format!("{} - {}", kind.label(), rng.random_int(1_000_000));
        movie
    }
}

/// How a part changes into the next one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MorphType {
    /// Crossfade: both flames are rendered, with opposite densities.
    #[default]
    Fade,
    /// Interpolates every value of the two flames.
    Morph,
}

impl MorphType {
    pub const ALL: [MorphType; 2] = [MorphType::Fade, MorphType::Morph];

    pub fn label(self) -> &'static str {
        match self {
            MorphType::Fade => "Fade",
            MorphType::Morph => "Morph",
        }
    }

    fn jwf(self) -> &'static str {
        match self {
            MorphType::Fade => "FADE",
            MorphType::Morph => "MORPH",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MoviePart {
    pub flame: Flame,
    pub frames: u32,
    /// Frames at the end of the part that change into the next part.
    pub morph_frames: u32,
    pub morph: MorphType,
}

impl MoviePart {
    pub fn new(flame: Flame) -> Self {
        Self { flame, frames: 75, morph_frames: 25, morph: MorphType::Fade }
    }
}

macro_rules! script_types {
    ($name:ident { $($variant:ident => $jwf:literal, $label:literal),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
        pub enum $name {
            #[default]
            $($variant),*
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),*];

            pub fn label(self) -> &'static str {
                match self {
                    $($name::$variant => $label),*
                }
            }

            fn jwf(self) -> &'static str {
                match self {
                    $($name::$variant => $jwf),*
                }
            }

            fn from_jwf(s: &str) -> Self {
                $name::ALL.iter().copied().find(|t| t.jwf() == s).unwrap_or_default()
            }
        }
    };
}

script_types!(GlobalScript {
    None => "NONE", "None",
    RotatePitch => "ROTATE_PITCH", "Rotate pitch",
    RotateRoll => "ROTATE_ROLL", "Rotate roll",
    RotateYaw => "ROTATE_YAW", "Rotate yaw",
    RotateBank => "ROTATE_BANK", "Rotate bank",
    MoveCamX => "MOVE_CAM_X", "Move camera x",
    MoveCamY => "MOVE_CAM_Y", "Move camera y",
    MoveCamZ => "MOVE_CAM_Z", "Move camera z",
});

script_types!(XFormScript {
    None => "NONE", "None",
    RotateFull => "ROTATE_FULL", "Rotate all transforms",
    RotateLast => "ROTATE_LAST_XFORM", "Rotate last transform",
    RotateFirst => "ROTATE_FIRST_XFORM", "Rotate 1st transform",
    Rotate2nd => "ROTATE_2ND_XFORM", "Rotate 2nd transform",
    Rotate3rd => "ROTATE_3RD_XFORM", "Rotate 3rd transform",
    Rotate4th => "ROTATE_4TH_XFORM", "Rotate 4th transform",
    Rotate5th => "ROTATE_5TH_XFORM", "Rotate 5th transform",
    RotateFinal => "ROTATE_FINAL_XFORM", "Rotate final transform",
    RotateBg => "ROTATE_BG_XFORM", "Rotate background transform",
    RotatePostFull => "ROTATE_POST_FULL", "Rotate all post affines",
    RotatePostLast => "ROTATE_POST_LAST_XFORM", "Rotate last post affine",
    RotatePostFirst => "ROTATE_POST_FIRST_XFORM", "Rotate 1st post affine",
    RotatePost2nd => "ROTATE_POST_2ND_XFORM", "Rotate 2nd post affine",
    RotatePost3rd => "ROTATE_POST_3RD_XFORM", "Rotate 3rd post affine",
    RotatePost4th => "ROTATE_POST_4TH_XFORM", "Rotate 4th post affine",
    RotatePost5th => "ROTATE_POST_5TH_XFORM", "Rotate 5th post affine",
    RotatePostFinal => "ROTATE_POST_FINAL_XFORM", "Rotate final post affine",
});

/// A script with its strength (1 = one turn in six seconds, or a camera sway of one unit).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Script<T> {
    pub kind: T,
    pub amplitude: f64,
}

/// `AnimationService.DFLT_DURATION`: seconds per turn at amplitude 1.
const DEFAULT_DURATION: f64 = 6.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Movie {
    pub name: String,
    pub parts: Vec<MoviePart>,
    pub global_scripts: Vec<Script<GlobalScript>>,
    pub xform_scripts: Vec<Script<XFormScript>>,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Render quality (samples per pixel).
    pub quality: f64,
    pub motion_blur_length: u32,
    pub motion_blur_time_step: f64,
    pub motion_blur_decay: f64,
}

impl Default for Movie {
    fn default() -> Self {
        Self {
            name: String::new(),
            parts: Vec::new(),
            global_scripts: Vec::new(),
            xform_scripts: Vec::new(),
            width: 1280,
            height: 720,
            fps: 25.0,
            quality: 100.0,
            motion_blur_length: 0,
            motion_blur_time_step: 0.015,
            motion_blur_decay: 0.03,
        }
    }
}

impl Movie {
    pub fn frame_count(&self) -> u32 {
        self.parts.iter().map(|p| p.frames).sum()
    }

    /// The part and its start frame at `frame` (0-based).
    pub fn part_at(&self, frame: f64) -> Option<(usize, f64)> {
        let mut start = 0.0;
        for (i, p) in self.parts.iter().enumerate() {
            let end = start + p.frames as f64;
            if frame < end || i + 1 == self.parts.len() {
                return Some((i, start));
            }
            start = end;
        }
        None
    }

    /// The flame of the parts at `frame` (`FlameMovie.getFlame`), before the scripts.
    pub fn base_flame(&self, frame: f64) -> Option<Flame> {
        let (i, start) = self.part_at(frame)?;
        let part = &self.parts[i];
        let f1 = part.flame.at_frame(frame);
        let end = start + part.frames as f64;
        let m = part.morph_frames.min(part.frames) as f64;
        if m < 1.0 || frame < end - m || i + 1 == self.parts.len() {
            return Some(f1);
        }
        let f2 = self.parts[i + 1].flame.at_frame(frame);
        // JWildfire: morph frame k = 1..m gives (k - 1) / (m - 1).
        let t = if m < 2.0 { 0.0 } else { ((frame - (end - m)) / (m - 1.0)).clamp(0.0, 1.0) };
        Some(morph_flames(part.morph, &f1, &f2, t))
    }

    /// The flame of `frame` (0-based, fractional for motion blur): parts, morphs and scripts.
    pub fn frame_flame(&self, frame: f64) -> Option<Flame> {
        let mut f = self.base_flame(frame)?;
        self.apply_scripts(&mut f, frame);
        f.width = self.width;
        f.height = self.height;
        f.bg_transparency = false;
        // The movie has its own motion blur; the flames' animations are already evaluated.
        f.animation = Default::default();
        Some(f)
    }

    /// The motion blur packets of `frame` (`frame_flame` of the frame and its sub-frames, the
    /// later ones fainter), like [`crate::render::frame_packets`].
    pub fn packets(&self, frame: f64) -> Vec<Flame> {
        let Some(first) = self.frame_flame(frame) else { return Vec::new() };
        let mut out = vec![first];
        if self.motion_blur_length > 0 {
            let len = self.motion_blur_length as f64;
            let mut t = frame + len * self.motion_blur_time_step / 2.0;
            for p in 1..=self.motion_blur_length {
                t -= self.motion_blur_time_step;
                if let Some(mut f) = self.frame_flame(t.max(0.0)) {
                    let scl = (1.0 - (p * p) as f64 * self.motion_blur_decay * 0.07 / len).max(0.01);
                    for l in &mut f.layers {
                        l.weight *= scl;
                    }
                    out.push(f);
                }
            }
        }
        out
    }

    /// `AnimationService.addMotionCurve`: camera turns grow linearly (one turn per six seconds at
    /// amplitude 1), camera moves sway in a sine with a period of twelve seconds; both are
    /// added to the flame's own values.
    fn apply_scripts(&self, f: &mut Flame, frame: f64) {
        let fps = self.fps.max(1e-6);
        let turn = |amp: f64| 360.0 * amp * frame / (DEFAULT_DURATION * fps);
        // JWildfire places spline points every `phase` frames: -A, 0, A, 0, ...
        let phase = ftoi(DEFAULT_DURATION * fps / 2.0).max(1) as f64;
        let sway = |amp: f64| amp * (std::f64::consts::FRAC_PI_2 * frame / phase).sin();
        for s in &self.global_scripts {
            if s.amplitude.abs() < 1e-10 {
                continue;
            }
            match s.kind {
                GlobalScript::None => {}
                GlobalScript::RotatePitch => f.cam_pitch += turn(s.amplitude),
                GlobalScript::RotateRoll => f.cam_roll += turn(s.amplitude),
                GlobalScript::RotateYaw => f.cam_yaw += turn(s.amplitude),
                GlobalScript::RotateBank => f.cam_bank += turn(s.amplitude),
                GlobalScript::MoveCamX => f.cam_pos_x += sway(s.amplitude),
                GlobalScript::MoveCamY => f.cam_pos_y += sway(s.amplitude),
                GlobalScript::MoveCamZ => f.cam_pos_z += sway(s.amplitude),
            }
        }
        // JWildfire applies the transform scripts to the first layer.
        let Some(layer) = f.layers.first_mut() else { return };
        for s in &self.xform_scripts {
            if s.amplitude.abs() < 1e-10 {
                continue;
            }
            let angle = turn(s.amplitude);
            use XFormScript::*;
            match s.kind {
                None => {}
                RotateFull | RotatePostFull => {
                    for (i, xf) in layer.xforms.iter_mut().enumerate() {
                        let a = if i % 2 == 0 { angle } else { -angle };
                        if s.kind == RotateFull { xf.coefs.rotate(a) } else { xf.post.rotate(a) }
                    }
                }
                kind => {
                    let post = matches!(
                        kind,
                        RotatePostLast | RotatePostFirst | RotatePost2nd | RotatePost3rd | RotatePost4th | RotatePost5th | RotatePostFinal
                    );
                    let n = layer.xforms.len();
                    let xf = match kind {
                        RotateFirst | RotatePostFirst => layer.xforms.first_mut(),
                        Rotate2nd | RotatePost2nd => layer.xforms.get_mut(1),
                        Rotate3rd | RotatePost3rd => layer.xforms.get_mut(2),
                        Rotate4th | RotatePost4th => layer.xforms.get_mut(3),
                        Rotate5th | RotatePost5th => layer.xforms.get_mut(4),
                        RotateLast | RotatePostLast => layer.xforms.get_mut(n.wrapping_sub(1)),
                        RotateFinal | RotatePostFinal => layer.final_xforms.first_mut(),
                        RotateBg => layer.bg_xforms.first_mut(),
                        _ => Option::None,
                    };
                    if let Some(xf) = xf {
                        if post { xf.post.rotate(angle) } else { xf.coefs.rotate(angle) }
                    }
                }
            }
        }
    }
}

// ---- morphing (`FlameMorphService`) -------------------------------------------------------

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn lerp_color(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    std::array::from_fn(|i| lerp(a[i] as f64, b[i] as f64, t).round().clamp(0.0, 255.0) as u8)
}

/// Morphs `f1` into `f2` (`t` = 0..1).
pub fn morph_flames(kind: MorphType, f1: &Flame, f2: &Flame, t: f64) -> Flame {
    if t <= 1e-10 {
        return f1.clone();
    }
    if t >= 1.0 - 1e-10 {
        return f2.clone();
    }
    let mut res = f1.clone();
    match kind {
        MorphType::Fade => {
            for l in &mut res.layers {
                l.density *= 1.0 - t;
            }
            res.layers.extend(f2.layers.iter().map(|l| Layer { density: l.density * t, ..l.clone() }));
        }
        MorphType::Morph => {
            let n = f1.layers.len().max(f2.layers.len());
            res.layers = (0..n)
                .map(|i| match (f1.layers.get(i), f2.layers.get(i)) {
                    (Some(a), Some(b)) => morph_layers(a, b, t),
                    (Some(a), None) => Layer { density: lerp(a.density, 0.0, t), ..a.clone() },
                    (None, Some(b)) => Layer { density: lerp(0.0, b.density, t), ..b.clone() },
                    (None, None) => unreachable!(),
                })
                .collect();
        }
    }
    morph_flame_values(f1, f2, t, &mut res);
    res
}

fn morph_flame_values(f1: &Flame, f2: &Flame, t: f64, res: &mut Flame) {
    for (_, get, set) in FLAME_PROPS {
        set(res, lerp(get(f1), get(f2), t));
    }
    res.dim_z_distance = lerp(f1.dim_z_distance, f2.dim_z_distance, t);
    res.fg_opacity = lerp(f1.fg_opacity, f2.fg_opacity, t);
    res.spatial_filter_radius = lerp(f1.spatial_filter_radius, f2.spatial_filter_radius, t);
    res.filter_sharpness = lerp(f1.filter_sharpness, f2.filter_sharpness, t);
    res.filter_low_density = lerp(f1.filter_low_density, f2.filter_low_density, t);
    res.antialias_amount = lerp(f1.antialias_amount, f2.antialias_amount, t);
    res.antialias_radius = lerp(f1.antialias_radius, f2.antialias_radius, t);
    res.width = lerp(f1.width as f64, f2.width as f64, t).round() as u32;
    res.height = lerp(f1.height as f64, f2.height as f64, t).round() as u32;
    res.preserve_z = if t >= 0.5 { f2.preserve_z } else { f1.preserve_z };
    let (b1, b2) = (&f1.background, &f2.background);
    res.background.kind = b2.kind;
    res.background.color = lerp_color(b1.color, b2.color, t);
    res.background.ul = lerp_color(b1.ul, b2.ul, t);
    res.background.ur = lerp_color(b1.ur, b2.ur, t);
    res.background.ll = lerp_color(b1.ll, b2.ll, t);
    res.background.lr = lerp_color(b1.lr, b2.lr, t);
    res.background.cc = lerp_color(b1.cc, b2.cc, t);
    let (s1, s2) = (&f1.post_symmetry, &f2.post_symmetry);
    res.post_symmetry.centre_x = lerp(s1.centre_x, s2.centre_x, t);
    res.post_symmetry.centre_y = lerp(s1.centre_y, s2.centre_y, t);
    res.post_symmetry.distance = lerp(s1.distance, s2.distance, t);
    res.post_symmetry.order = ftoi(lerp(s1.order as f64, s2.order as f64, t));
    res.post_symmetry.rotation = lerp(s1.rotation, s2.rotation, t);
}

fn morph_layers(a: &Layer, b: &Layer, t: f64) -> Layer {
    let morph_list = |xa: &[XForm], xb: &[XForm]| -> Vec<XForm> {
        (0..xa.len().max(xb.len()))
            .map(|i| {
                let empty = || {
                    let mut x = XForm { weight: 0.0, ..XForm::default() };
                    x.variations = vec![VariationInstance::new("linear3D", 0.0)];
                    x
                };
                let x1 = xa.get(i).cloned().unwrap_or_else(empty);
                let x2 = xb.get(i).cloned().unwrap_or_else(empty);
                morph_xforms(x1, x2, t)
            })
            .collect()
    };
    // the rendered gradients (with their shift) are blended
    let (ga, gb) = (a.render_gradient(), b.render_gradient());
    let n = ga.colors.len().max(gb.colors.len());
    let color = |g: &crate::gradient::Gradient, i: usize| g.colors.get(i).or(g.colors.last()).copied().unwrap_or([0, 0, 0]);
    let mut gradient = a.gradient.clone();
    gradient.colors = (0..n).map(|i| lerp_color(color(&ga, i), color(&gb, i), t)).collect();
    Layer {
        weight: lerp(a.weight, b.weight, t),
        density: lerp(a.density, b.density, t),
        gradient,
        gradient_shift: 0.0,
        xforms: morph_list(&a.xforms, &b.xforms),
        final_xforms: morph_list(&a.final_xforms, &b.final_xforms),
        bg_xforms: morph_list(&a.bg_xforms, &b.bg_xforms),
        ..a.clone()
    }
}

/// `prepareMorphXForm`: an inverted `spherical3D_wf` becomes a negative amount, `post_mirror_wf`
/// is dropped.
fn prepare_morph(xf: &mut XForm) {
    xf.variations.retain(|v| v.name != "post_mirror_wf");
    for v in &mut xf.variations {
        if v.name == "spherical3D_wf" && v.param("invert") == Some(1.0) {
            v.set_param("invert", 0.0);
            v.amount = -v.amount;
        }
    }
}

fn morph_affine(a: &Affine, b: &Affine, t: f64) -> Affine {
    let (x, y) = (a.as_array(), b.as_array());
    Affine::from_array(std::array::from_fn(|i| lerp(x[i], y[i], t)))
}

fn morph_xforms(mut x1: XForm, mut x2: XForm, t: f64) -> XForm {
    prepare_morph(&mut x1);
    prepare_morph(&mut x2);
    let second = t >= 0.5;
    let n = x1.chaos.len().max(x2.chaos.len());
    let chaos = (0..n).map(|i| lerp(x1.chaos.get(i).copied().unwrap_or(1.0), x2.chaos.get(i).copied().unwrap_or(1.0), t)).collect();
    // Variations matched by name; missing ones come in or go out with amount 0.
    let mut pairs: Vec<(VariationInstance, VariationInstance)> = Vec::new();
    let mut seen = HashSet::new();
    for v1 in &x1.variations {
        seen.insert(v1.name.clone());
        let v2 = x2.variations.iter().find(|v| v.name == v1.name).cloned().unwrap_or_else(|| VariationInstance::new(&v1.name, 0.0));
        pairs.push((v1.clone(), v2));
    }
    for v2 in &x2.variations {
        if !seen.contains(&v2.name) {
            pairs.push((VariationInstance::new(&v2.name, 0.0), v2.clone()));
        }
    }
    let variations = pairs
        .into_iter()
        .map(|(v1, v2)| {
            let def = variations::find(&v1.name);
            let mut v = if v1.raw_params.is_empty() && !v2.raw_params.is_empty() { v2.clone() } else { v1.clone() };
            v.amount = lerp(v1.amount, v2.amount, t);
            for (i, p) in v.params.iter_mut().enumerate() {
                let (a, b) = (v1.params.get(i).copied().unwrap_or(*p), v2.params.get(i).copied().unwrap_or(*p));
                let param = def.and_then(|d| d.params.get(i));
                *p = if param.is_some_and(|p| p.integer) {
                    let m = ftoi(lerp(a, b, t)) as f64;
                    if m == 0.0 && param.is_some_and(|p| p.name == "power") { 1.0 } else { m }
                } else {
                    lerp(a, b, t)
                };
            }
            v
        })
        .collect();
    XForm {
        weight: lerp(x1.weight, x2.weight, t),
        color: lerp(x1.color, x2.color, t),
        color_symmetry: lerp(x1.color_symmetry, x2.color_symmetry, t),
        material: lerp(x1.material, x2.material, t),
        material_speed: lerp(x1.material_speed, x2.material_speed, t),
        opacity: lerp(x1.opacity, x2.opacity, t),
        draw_mode: if second { x2.draw_mode } else { x1.draw_mode },
        color_type: if second { x2.color_type } else { x1.color_type },
        target_color: lerp_color(x1.target_color, x2.target_color, t),
        coefs: morph_affine(&x1.coefs, &x2.coefs, t),
        post: morph_affine(&x1.post, &x2.post, t),
        yz_coefs: morph_affine(&x1.yz_coefs, &x2.yz_coefs, t),
        yz_post: morph_affine(&x1.yz_post, &x2.yz_post, t),
        zx_coefs: morph_affine(&x1.zx_coefs, &x2.zx_coefs, t),
        zx_post: morph_affine(&x1.zx_post, &x2.zx_post, t),
        chaos,
        variations,
        ..if second { x2 } else { x1 }
    }
}

// ---- .jwfmovie files (`FlameMovieWriter` / `FlameMovieReader`) -----------------------------

#[derive(Debug, thiserror::Error)]
pub enum MovieIoError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not a movie: {0}")]
    Format(String),
}

fn fmt(v: f64) -> String {
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') { s } else { format!("{s}.0") }
}

/// The movie as JWildfire `.jwfmovie` XML.
pub fn write_movie(m: &Movie) -> String {
    let mut s = String::from("<jwf-movie");
    let mut attr = |k: &str, v: String| s.push_str(&format!(" {k}=\"{}\"", xml::escape(&v)));
    attr("application", format!("RWildFire {}", env!("CARGO_PKG_VERSION")));
    attr("movie_version", "1".into());
    for i in 0..12 {
        let g = m.global_scripts.get(i).copied().unwrap_or_default();
        attr(&format!("script_global{}", i + 1), g.kind.jwf().into());
        attr(&format!("script_global{}_amplitude", i + 1), fmt(g.amplitude));
    }
    for i in 0..12 {
        let x = m.xform_scripts.get(i).copied().unwrap_or_default();
        attr(&format!("script_xform{}", i + 1), x.kind.jwf().into());
        attr(&format!("script_xform{}_amplitude", i + 1), fmt(x.amplitude));
    }
    if !m.name.is_empty() {
        attr("name", m.name.replace('"', ""));
    }
    attr("frame_width", m.width.to_string());
    attr("frame_height", m.height.to_string());
    attr("fps", fmt(m.fps));
    attr("quality", (m.quality.round() as i64).to_string());
    attr("sequence_output_type", "FLAMES".into());
    attr("compatibility", "false".into());
    attr("motion_blur_length", m.motion_blur_length.to_string());
    attr("motion_blur_timestep", fmt(m.motion_blur_time_step));
    attr("motion_blur_decay", fmt(m.motion_blur_decay));
    s.push_str(">\n");
    for p in &m.parts {
        s.push_str(&format!(
            "  <jwf-movie-part frame_count=\"{}\" morph_type=\"{}\" frame_morph_count=\"{}\">\n",
            p.frames,
            p.morph.jwf(),
            p.morph_frames
        ));
        s.push_str(&crate::io::write_flame(&p.flame));
        s.push_str("\n  </jwf-movie-part>\n");
    }
    s.push_str("</jwf-movie>\n");
    s
}

/// Parses a JWildfire `.jwfmovie` file.
pub fn read_movie(src: &str) -> Result<Movie, MovieIoError> {
    let root = xml::parse(src).map_err(|e| MovieIoError::Format(e.to_string()))?;
    let mut all = Vec::new();
    root.find_all("jwf-movie", &mut all);
    let el: &Element =
        if root.name == "jwf-movie" { &root } else { all.first().copied().ok_or_else(|| MovieIoError::Format("no <jwf-movie>".into()))? };
    let num = |k: &str| el.attr(k).and_then(|v| v.trim().parse::<f64>().ok());
    let d = Movie::default();
    let mut m = Movie {
        name: el.attr("name").unwrap_or_default().to_string(),
        width: num("frame_width").map_or(d.width, |v| v.max(1.0) as u32),
        height: num("frame_height").map_or(d.height, |v| v.max(1.0) as u32),
        fps: num("fps").filter(|v| *v > 0.0).unwrap_or(d.fps),
        quality: num("quality").filter(|v| *v > 0.0).unwrap_or(d.quality),
        motion_blur_length: num("motion_blur_length").map_or(d.motion_blur_length, |v| v.max(0.0) as u32),
        motion_blur_time_step: num("motion_blur_timestep").unwrap_or(d.motion_blur_time_step),
        motion_blur_decay: num("motion_blur_decay").unwrap_or(d.motion_blur_decay),
        ..d
    };
    for i in 1..=12 {
        let kind = GlobalScript::from_jwf(el.attr(&format!("script_global{i}")).unwrap_or("NONE"));
        let amplitude = num(&format!("script_global{i}_amplitude")).unwrap_or(1.0);
        if kind != GlobalScript::None {
            m.global_scripts.push(Script { kind, amplitude });
        }
        let kind = XFormScript::from_jwf(el.attr(&format!("script_xform{i}")).unwrap_or("NONE"));
        let amplitude = num(&format!("script_xform{i}_amplitude")).unwrap_or(1.0);
        if kind != XFormScript::None {
            m.xform_scripts.push(Script { kind, amplitude });
        }
    }
    for p in el.children_named("jwf-movie-part") {
        let Some(fe) = p.children.iter().find(|c| c.name == "flame" || c.name == "jwf-flame") else { continue };
        let pn = |k: &str| p.attr(k).and_then(|v| v.trim().parse::<f64>().ok());
        m.parts.push(MoviePart {
            flame: crate::io::parse_flame(fe),
            frames: pn("frame_count").map_or(75, |v| v.max(0.0) as u32),
            morph_frames: pn("frame_morph_count").map_or(0, |v| v.max(0.0) as u32),
            morph: if p.attr("morph_type") == Some("MORPH") { MorphType::Morph } else { MorphType::Fade },
        });
    }
    Ok(m)
}

pub fn read_movie_file(path: impl AsRef<Path>) -> Result<Movie, MovieIoError> {
    read_movie(&std::fs::read_to_string(path)?)
}

pub fn write_movie_file(m: &Movie, path: impl AsRef<Path>) -> Result<(), MovieIoError> {
    Ok(std::fs::write(path, write_movie(m))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flame(brightness: f64, var: &str, c20: f64) -> Flame {
        let mut xf = XForm::default();
        xf.coefs.c20 = c20;
        xf.variations = vec![VariationInstance::new(var, 1.0)];
        let layer = Layer { xforms: vec![xf], ..Layer::default() };
        Flame { brightness, layers: vec![layer], ..Flame::default() }
    }

    fn movie() -> Movie {
        let mut a = MoviePart::new(flame(2.0, "linear", 0.0));
        a.frames = 10;
        a.morph_frames = 5;
        a.morph = MorphType::Morph;
        let mut b = MoviePart::new(flame(4.0, "spherical", 1.0));
        b.frames = 10;
        Movie { parts: vec![a, b], ..Movie::default() }
    }

    #[test]
    fn parts_and_morph_frames() {
        let m = movie();
        assert_eq!(m.frame_count(), 20);
        assert_eq!(m.base_flame(0.0).unwrap().brightness, 2.0);
        assert_eq!(m.base_flame(4.0).unwrap().brightness, 2.0);
        // Morph frames 5..9 of part 0: 0, 0.25, .., 1.
        let mid = m.base_flame(7.0).unwrap();
        assert!((mid.brightness - 3.0).abs() < 1e-9, "{}", mid.brightness);
        let xf = &mid.layers[0].xforms[0];
        assert!((xf.coefs.c20 - 0.5).abs() < 1e-9);
        let names: Vec<(&str, f64)> = xf.variations.iter().map(|v| (v.name.as_str(), v.amount)).collect();
        assert_eq!(names, vec![("linear", 0.5), ("spherical", 0.5)]);
        assert_eq!(m.base_flame(9.0).unwrap().brightness, 4.0);
        assert_eq!(m.base_flame(15.0).unwrap().brightness, 4.0);
    }

    #[test]
    fn fade_crossfades_the_layers() {
        let (a, b) = (flame(2.0, "linear", 0.0), flame(4.0, "spherical", 1.0));
        let f = morph_flames(MorphType::Fade, &a, &b, 0.25);
        assert_eq!(f.layers.len(), 2);
        assert!((f.layers[0].density - 0.75).abs() < 1e-9 && (f.layers[1].density - 0.25).abs() < 1e-9);
    }

    #[test]
    fn scripts_turn_and_sway() {
        let mut m = movie();
        m.fps = 10.0;
        m.global_scripts = vec![Script { kind: GlobalScript::RotateYaw, amplitude: 1.0 }, Script { kind: GlobalScript::MoveCamX, amplitude: 0.5 }];
        m.xform_scripts = vec![Script { kind: XFormScript::RotateFirst, amplitude: 1.0 }];
        // One turn in six seconds = 60 frames; the sway peaks after 30 frames.
        let f = m.frame_flame(15.0).unwrap();
        assert!((f.cam_yaw - 90.0).abs() < 1e-9);
        assert!((f.cam_pos_x - 0.5 * (std::f64::consts::FRAC_PI_2 * 0.5).sin()).abs() < 1e-9);
        let c = &f.layers[0].xforms[0].coefs;
        assert!(c.c00.abs() < 1e-9 && (c.c01 - 1.0).abs() < 1e-9, "{c:?}");
    }

    #[test]
    fn movie_file_round_trip() {
        let mut m = movie();
        m.name = "Test".into();
        m.global_scripts = vec![Script { kind: GlobalScript::RotateRoll, amplitude: 0.25 }];
        m.motion_blur_length = 4;
        let back = read_movie(&write_movie(&m)).unwrap();
        assert_eq!(back.parts.len(), 2);
        assert_eq!(back.parts[0].morph, MorphType::Morph);
        assert_eq!((back.parts[0].frames, back.parts[0].morph_frames), (10, 5));
        assert_eq!(back.parts[1].flame.brightness, 4.0);
        assert_eq!(back.global_scripts, m.global_scripts);
        assert_eq!((back.width, back.height, back.motion_blur_length), (m.width, m.height, 4));
        assert_eq!(back.name, "Test");
    }

    #[test]
    fn motion_blur_packets() {
        let mut m = movie();
        m.motion_blur_length = 3;
        let p = m.packets(4.0);
        assert_eq!(p.len(), 4);
        assert!(p[3].layers[0].weight < p[1].layers[0].weight);
    }

    #[test]
    fn random_movies_have_a_part_and_scripts() {
        let mut rng = crate::rng::Rng::new(21);
        for kind in RandomMovie::ALL {
            let m = kind.generate(&mut rng);
            assert_eq!((m.parts.len(), m.frame_count()), (1, 320), "{}", kind.label());
            assert!(!m.global_scripts.is_empty() && !m.parts[0].flame.layers[0].xforms.is_empty(), "{}", kind.label());
            assert!(m.frame_flame(100.0).is_some());
        }
        let m = RandomMovie::RotatingMandelbrots.generate(&mut rng);
        let kinds: Vec<GlobalScript> = m.global_scripts.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [GlobalScript::RotateBank, GlobalScript::RotateRoll, GlobalScript::MoveCamY]);
    }
}
