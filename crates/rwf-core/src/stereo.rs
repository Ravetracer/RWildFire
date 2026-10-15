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

//! Stereo 3D images (JWildfire's `Stereo3d*`): the flame is rendered for a left and a right
//! eye (camera turned by the eye angle around the focal point and moved by the eye distance,
//! `Stereo3dFlameRendererView`), and the two images are combined into an anaglyph, put side
//! by side, or rendered at several eye distances as a grid (`FlameRenderer.renderImageStereo3d`).

use crate::model::Flame;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Stereo3dMode {
    #[default]
    None,
    Anaglyph,
    SideBySide,
    /// Pairs at growing eye distance and angle, one row per pair.
    InterpolatedImages,
}

impl Stereo3dMode {
    pub const ALL: [Stereo3dMode; 4] = [Stereo3dMode::None, Stereo3dMode::Anaglyph, Stereo3dMode::SideBySide, Stereo3dMode::InterpolatedImages];

    pub fn as_str(&self) -> &'static str {
        match self {
            Stereo3dMode::None => "NONE",
            Stereo3dMode::Anaglyph => "ANAGLYPH",
            Stereo3dMode::SideBySide => "SIDE_BY_SIDE",
            Stereo3dMode::InterpolatedImages => "INTERPOLATED_IMAGES",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Stereo3dMode::None => "Off",
            Stereo3dMode::Anaglyph => "Anaglyph",
            Stereo3dMode::SideBySide => "Side by side",
            Stereo3dMode::InterpolatedImages => "Interpolated pairs",
        }
    }
}

/// The color filter of an eye in anaglyph images (`Stereo3dColor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stereo3dColor {
    Red,
    Cyan,
}

impl Stereo3dColor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Stereo3dColor::Red => "RED",
            Stereo3dColor::Cyan => "CYAN",
        }
    }

    /// The channels of `c` this filter lets through.
    fn filter(&self, c: [u8; 3]) -> [i32; 3] {
        match self {
            Stereo3dColor::Red => [c[0] as i32, 0, 0],
            Stereo3dColor::Cyan => [0, c[1] as i32, c[2] as i32],
        }
    }
}

/// JWildfire's stereo preview setting (kept for its files; RWildFire previews in mono).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Stereo3dPreview {
    None,
    #[default]
    Anaglyph,
    SideBySide,
    SideBySideFull,
}

impl Stereo3dPreview {
    fn as_str(&self) -> &'static str {
        match self {
            Stereo3dPreview::None => "NONE",
            Stereo3dPreview::Anaglyph => "ANAGLYPH",
            Stereo3dPreview::SideBySide => "SIDE_BY_SIDE",
            Stereo3dPreview::SideBySideFull => "SIDE_BY_SIDE_FULL",
        }
    }
}

/// The eye a view renders for (not saved; set while rendering stereo images).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StereoEye {
    #[default]
    Unspecified,
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stereo3d {
    pub mode: Stereo3dMode,
    /// Eye angle (degrees).
    pub angle: f64,
    pub eye_dist: f64,
    /// Depth of the point the eyes turn around.
    pub focal_offset: f64,
    pub left_color: Stereo3dColor,
    pub right_color: Stereo3dColor,
    pub preview: Stereo3dPreview,
    pub interpolated_image_count: u32,
    pub swap_sides: bool,
}

impl Default for Stereo3d {
    fn default() -> Self {
        Self {
            mode: Stereo3dMode::None,
            angle: 1.6,
            eye_dist: 0.032,
            focal_offset: 0.5,
            left_color: Stereo3dColor::Red,
            right_color: Stereo3dColor::Cyan,
            preview: Stereo3dPreview::Anaglyph,
            interpolated_image_count: 3,
            swap_sides: false,
        }
    }
}

impl Stereo3d {
    pub fn is_on(&self) -> bool {
        self.mode != Stereo3dMode::None
    }

    /// Number of eye images a stereo render needs (`calcStereo3dSampleMultiplier`); each gets
    /// that fraction of the samples.
    pub fn image_count(&self) -> u32 {
        match self.mode {
            Stereo3dMode::None => 1,
            Stereo3dMode::Anaglyph | Stereo3dMode::SideBySide => 2,
            Stereo3dMode::InterpolatedImages => 2 * self.interpolated_image_count.max(1),
        }
    }

    /// Size of the combined image for `w` × `h` eye images.
    pub fn output_size(&self, w: u32, h: u32) -> (u32, u32) {
        match self.mode {
            Stereo3dMode::None | Stereo3dMode::Anaglyph => (w, h),
            Stereo3dMode::SideBySide => (2 * w, h),
            Stereo3dMode::InterpolatedImages => (2 * w, self.interpolated_image_count.max(1) * h),
        }
    }

    /// Reads and removes the `stereo3d_*` attributes.
    pub fn take_from_attrs(attrs: &mut Vec<(String, String)>) -> Self {
        let mut s = Self::default();
        attrs.retain(|(k, v)| {
            let v = v.trim();
            let f = || v.parse::<f64>().ok();
            let color = || match v {
                "RED" => Some(Stereo3dColor::Red),
                "CYAN" => Some(Stereo3dColor::Cyan),
                _ => None,
            };
            match k.as_str() {
                "stereo3d_mode" => s.mode = Stereo3dMode::ALL.into_iter().find(|m| m.as_str() == v).unwrap_or_default(),
                "stereo3d_angle" => s.angle = f().unwrap_or(s.angle),
                "stereo3d_eye_dist" => s.eye_dist = f().unwrap_or(s.eye_dist),
                "stereo3d_focal_offset" => s.focal_offset = f().unwrap_or(s.focal_offset),
                "stereo3d_left_eye_color" => s.left_color = color().unwrap_or(s.left_color),
                "stereo3d_right_eye_color" => s.right_color = color().unwrap_or(s.right_color),
                "stereo3d_interpolated_image_count" => s.interpolated_image_count = f().map_or(s.interpolated_image_count, |n| n.max(1.0) as u32),
                "stereo3d_preview" => {
                    s.preview = [Stereo3dPreview::None, Stereo3dPreview::Anaglyph, Stereo3dPreview::SideBySide, Stereo3dPreview::SideBySideFull]
                        .into_iter()
                        .find(|p| p.as_str() == v)
                        .unwrap_or_default()
                }
                "stereo3d_swap_sides" => s.swap_sides = v == "1",
                _ => return true,
            }
            false
        });
        s
    }

    /// Writes the `stereo3d_*` attributes (only when stereo is on, as JWildfire does).
    pub fn write_attrs(&self, mut attr: impl FnMut(&str, String)) {
        if !self.is_on() {
            return;
        }
        let f = crate::io::fmt_f;
        attr("stereo3d_mode", self.mode.as_str().to_string());
        attr("stereo3d_angle", f(self.angle));
        attr("stereo3d_eye_dist", f(self.eye_dist));
        attr("stereo3d_focal_offset", f(self.focal_offset));
        attr("stereo3d_left_eye_color", self.left_color.as_str().to_string());
        attr("stereo3d_right_eye_color", self.right_color.as_str().to_string());
        attr("stereo3d_interpolated_image_count", self.interpolated_image_count.to_string());
        attr("stereo3d_preview", self.preview.as_str().to_string());
        attr("stereo3d_swap_sides", if self.swap_sides { "1" } else { "0" }.to_string());
    }
}

/// The camera change of an eye (`Stereo3dFlameRendererView`): camera points are turned in the
/// x-z plane around `(cam_x0, focal_offset)` and moved by the eye distance. Returns the new
/// camera matrix (columns as in [`crate::render::View`]) and the added translation.
pub fn eye_camera(flame: &Flame, matrix: [[f64; 3]; 3], cam_x0: f64) -> Option<([[f64; 3]; 3], [f64; 3])> {
    let s = &flame.stereo3d;
    let (angle, dist) = match flame.stereo_eye {
        StereoEye::Left => (s.angle, -s.eye_dist),
        StereoEye::Right => (-s.angle, s.eye_dist),
        StereoEye::Unspecified => return None,
    };
    if !s.is_on() {
        return None;
    }
    let (sin, cos) = (angle * std::f64::consts::PI / 180.0).sin_cos();
    let f = s.focal_offset;
    let m = matrix.map(|c| [cos * c[0] + sin * c[2], c[1], -sin * c[0] + cos * c[2]]);
    let t = [cam_x0 - cos * cam_x0 - sin * f + dist, 0.0, f + sin * cam_x0 - cos * f];
    Some((m, t))
}

/// Renders one eye image: the flame (with its eye set) at a quality, to RGBA8.
pub type RenderEye<'a, E> = dyn FnMut(&Flame, f64) -> Result<Vec<u8>, E> + 'a;

/// Renders a stereo image of `flame` (`w` × `h` per eye) with `render_eye`, which renders a
/// flame (with its eye set) at the given quality to RGBA8. Each eye gets the quality divided by
/// the number of eye images, as in JWildfire. Returns the combined image and its size.
pub fn render_stereo<E>(flame: &Flame, w: u32, h: u32, quality: f64, render_eye: &mut RenderEye<'_, E>) -> Result<(Vec<u8>, u32, u32), E> {
    let s = &flame.stereo3d;
    let q = quality / s.image_count() as f64;
    let mut eye = |e: StereoEye, step: Option<(u32, u32)>| {
        let mut f = flame.clone();
        f.stereo_eye = e;
        if let Some((i, n)) = step {
            // interpolated images: angle and distance grow in n steps
            f.stereo3d.angle = s.angle / n as f64 * (i + 1) as f64;
            f.stereo3d.eye_dist = s.eye_dist / n as f64 * (i + 1) as f64;
        }
        render_eye(&f, q)
    };
    let (ow, oh) = s.output_size(w, h);
    let mut out = vec![0u8; ow as usize * oh as usize * 4];
    let blit = |out: &mut [u8], img: &[u8], x0: u32, y0: u32| {
        for y in 0..h {
            let src = &img[(y * w * 4) as usize..((y + 1) * w * 4) as usize];
            let o = (((y0 + y) * ow + x0) * 4) as usize;
            out[o..o + src.len()].copy_from_slice(src);
        }
    };
    match s.mode {
        Stereo3dMode::None => return render_eye(flame, quality).map(|img| (img, w, h)),
        Stereo3dMode::Anaglyph => {
            let (mut l, mut r) = (eye(StereoEye::Left, None)?, eye(StereoEye::Right, None)?);
            if s.swap_sides {
                std::mem::swap(&mut l, &mut r);
            }
            for ((o, a), b) in out.as_chunks_mut::<4>().0.iter_mut().zip(l.as_chunks::<4>().0).zip(r.as_chunks::<4>().0) {
                let (ca, cb) = (s.left_color.filter([a[0], a[1], a[2]]), s.right_color.filter([b[0], b[1], b[2]]));
                for i in 0..3 {
                    o[i] = (ca[i] + cb[i]).clamp(0, 255) as u8;
                }
                o[3] = a[3].max(b[3]);
            }
        }
        Stereo3dMode::SideBySide => {
            let (mut l, mut r) = (eye(StereoEye::Left, None)?, eye(StereoEye::Right, None)?);
            if s.swap_sides {
                std::mem::swap(&mut l, &mut r);
            }
            blit(&mut out, &l, 0, 0);
            blit(&mut out, &r, w, 0);
        }
        Stereo3dMode::InterpolatedImages => {
            let n = s.interpolated_image_count.max(1);
            let lefts = (0..n).map(|i| eye(StereoEye::Left, Some((i, n)))).collect::<Result<Vec<_>, E>>()?;
            let rights = (0..n).map(|i| eye(StereoEye::Right, Some((i, n)))).collect::<Result<Vec<_>, E>>()?;
            let (lefts, rights) = if s.swap_sides { (rights, lefts) } else { (lefts, rights) };
            for (i, (l, r)) in lefts.iter().zip(&rights).enumerate() {
                blit(&mut out, l, 0, i as u32 * h);
                blit(&mut out, r, w, i as u32 * h);
            }
        }
    }
    Ok((out, ow, oh))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_round_trip() {
        let mut attrs: Vec<(String, String)> = [("stereo3d_mode", "SIDE_BY_SIDE"), ("stereo3d_angle", "2.5"), ("stereo3d_swap_sides", "1"), ("stereo3d_left_eye_color", "CYAN"), ("other", "x")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let s = Stereo3d::take_from_attrs(&mut attrs);
        assert_eq!((s.mode, s.angle, s.swap_sides, s.left_color), (Stereo3dMode::SideBySide, 2.5, true, Stereo3dColor::Cyan));
        assert_eq!(attrs.len(), 1);
        let mut written = Vec::new();
        s.write_attrs(|k, v| written.push((k.to_string(), v)));
        assert_eq!(Stereo3d::take_from_attrs(&mut written), s);
    }

    #[test]
    fn eyes_are_combined() {
        let mut flame = Flame::default();
        flame.stereo3d.mode = Stereo3dMode::Anaglyph;
        // left eye white, right eye gray
        let mut render = |f: &Flame, q: f64| -> Result<Vec<u8>, ()> {
            assert_eq!(q, 50.0);
            let v = if f.stereo_eye == StereoEye::Left { 255 } else { 100 };
            Ok(vec![v; 2 * 2 * 4])
        };
        let (img, w, h) = render_stereo(&flame, 2, 2, 100.0, &mut render).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(&img[0..4], &[255, 100, 100, 255]);
        flame.stereo3d.mode = Stereo3dMode::InterpolatedImages;
        let mut eyes = Vec::new();
        let mut render = |f: &Flame, _: f64| -> Result<Vec<u8>, ()> {
            eyes.push((f.stereo_eye, f.stereo3d.eye_dist));
            Ok(vec![if f.stereo_eye == StereoEye::Left { 1 } else { 2 }; 2 * 2 * 4])
        };
        let (img, w, h) = render_stereo(&flame, 2, 2, 60.0, &mut render).unwrap();
        assert_eq!((w, h), (4, 6));
        assert_eq!(eyes.len(), 6);
        assert!((eyes[2].1 - 0.032).abs() < 1e-12, "third pair at the full eye distance");
        assert_eq!((img[0], img[8]), (1, 2), "left eye left, right eye right");
    }

    #[test]
    fn eye_camera_turns_and_moves() {
        let mut flame = Flame::default();
        let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert!(eye_camera(&flame, id, -1.0).is_none());
        flame.stereo3d.mode = Stereo3dMode::Anaglyph;
        flame.stereo_eye = StereoEye::Right;
        flame.stereo3d.angle = 0.0;
        let (m, t) = eye_camera(&flame, id, -1.0).unwrap();
        assert_eq!(m, id);
        assert!((t[0] - 0.032).abs() < 1e-12 && t[1] == 0.0 && t[2].abs() < 1e-12);
    }
}
