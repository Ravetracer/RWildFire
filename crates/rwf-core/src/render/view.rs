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

//! Camera / view projection (port of `FlameRendererView` without DOF).

use crate::math::EPSILON;
use crate::model::Flame;

#[derive(Clone, Debug)]
pub struct View {
    pub image_width: u32,
    pub image_height: u32,
    pub raster_width: u32,
    pub raster_height: u32,
    pub border: u32,
    pub oversample: u32,
    pub cam_w: f64,
    pub cam_h: f64,
    pub bws: f64,
    pub bhs: f64,
    pub rc_x: f64,
    pub rc_y: f64,
    pub cosa: f64,
    pub sina: f64,
    pub project_3d: bool,
    pub matrix: [[f64; 3]; 3],
    pub cam_pos: [f64; 3],
    pub cam_perspective: f64,
    pub dimish_z: f64,
    pub dim_z_distance: f64,
    pub dof: Option<super::dof::Dof>,
    /// Effective pixels per unit (scale * zoom * resolution scale).
    pub ppu: f64,
    /// Output size relative to the flame's reference size (`pixelsPerUnitScale`).
    pub ppu_scale: f64,
}

impl View {
    /// `image_*` is the size of the output image; the flame's own `width`/`height`
    /// define the reference frame (`pixelsPerUnitScale`).
    pub fn new(flame: &Flame, image_width: u32, image_height: u32, filter_size: usize) -> Self {
        let oversample = flame.spatial_oversampling.max(1);
        let border = (filter_size as i64 - oversample as i64).max(0) as u32 / 2;
        let raster_width = oversample * image_width + 2 * border;
        let raster_height = oversample * image_height + 2 * border;

        let w_scl = image_width as f64 / flame.width.max(1) as f64;
        let h_scl = image_height as f64 / flame.height.max(1) as f64;
        let ppu_scale = (w_scl + h_scl) * 0.5;
        let ppu = flame.pixels_per_unit * flame.cam_zoom * ppu_scale;

        let corner_x = flame.centre_x - image_width as f64 / ppu / 2.0;
        let corner_y = flame.centre_y - image_height as f64 / ppu / 2.0;
        let border_size = border as f64 / ppu;
        let cam_x0 = corner_x - border_size;
        let cam_y0 = corner_y - border_size;
        let cam_x1 = corner_x + image_width as f64 / ppu + border_size;
        let cam_y1 = corner_y + image_height as f64 / ppu + border_size;
        let cam_w = cam_x1 - cam_x0;
        let cam_h = cam_y1 - cam_y0;

        let project_3d = flame.is_3d_projection_required();
        let (cosa, sina, rc_x, rc_y) = if !project_3d {
            let a = -std::f64::consts::PI * flame.cam_roll / 180.0;
            let (sina, cosa) = a.sin_cos();
            let rc_x = flame.centre_x * (1.0 - cosa) - flame.centre_y * sina - cam_x0;
            let rc_y = flame.centre_y * (1.0 - cosa) + flame.centre_x * sina - cam_y0;
            (cosa, sina, rc_x, rc_y)
        } else {
            (1.0, 0.0, -cam_x0, -cam_y0)
        };

        let matrix = camera_matrix(flame.cam_yaw, flame.cam_pitch, flame.cam_bank, flame.cam_roll);

        Self {
            image_width,
            image_height,
            raster_width,
            raster_height,
            border,
            oversample,
            cam_w,
            cam_h,
            bws: raster_width as f64 / cam_w,
            bhs: raster_height as f64 / cam_h,
            rc_x,
            rc_y,
            cosa,
            sina,
            project_3d,
            matrix,
            cam_pos: [flame.cam_pos_x, flame.cam_pos_y, flame.cam_pos_z],
            cam_perspective: flame.cam_perspective,
            dimish_z: flame.dimish_z,
            dim_z_distance: flame.dim_z_distance,
            dof: super::dof::Dof::new(flame),
            ppu,
            ppu_scale,
        }
    }

    /// The part of this view that shows the `w` × `h` image pixels starting at `(x0, y0)`
    /// (may lie partly outside the image): same scale and rotation, so tiles rendered with
    /// windows of one view fit together exactly (quilt rendering).
    pub fn window(&self, x0: i32, y0: i32, w: u32, h: u32) -> View {
        let mut v = self.clone();
        let os = self.oversample;
        v.image_width = w;
        v.image_height = h;
        v.raster_width = os * w + 2 * self.border;
        v.raster_height = os * h + 2 * self.border;
        v.rc_x -= (x0 as f64 * os as f64) / self.bws;
        v.rc_y -= (y0 as f64 * os as f64) / self.bhs;
        v.cam_w = v.raster_width as f64 / self.bws;
        v.cam_h = v.raster_height as f64 / self.bhs;
        v
    }

    /// Projects a world point; returns `(px, py, intensity, z)` in raster units (not rounded),
    /// `z` is the camera depth scaled like x (0 without 3D projection).
    #[inline]
    pub fn project(&self, x: f64, y: f64, z: f64, ctx: &mut crate::variations::Ctx) -> Option<(f64, f64, f64, f64)> {
        let (mut x, mut y) = (x, y);
        let mut intensity = 1.0;
        let mut cam_z = 0.0;
        if self.project_3d {
            let m = &self.matrix;
            let cx = m[0][0] * x + m[1][0] * y + m[2][0] * z + self.cam_pos[0];
            let cy = m[0][1] * x + m[1][1] * y + m[2][1] * z + self.cam_pos[1];
            let cz = m[0][2] * x + m[1][2] * y + m[2][2] * z + self.cam_pos[2];
            let zr = 1.0 - self.cam_perspective * cz + self.cam_pos[2];
            if zr < EPSILON {
                return None;
            }
            if self.dimish_z > EPSILON {
                let zdist = self.dim_z_distance - cz;
                if zdist > 0.0 {
                    intensity = (-zdist * zdist * self.dimish_z).exp();
                }
            }
            cam_z = cz;
            match self.dof.as_ref().and_then(|d| d.distance(cx, cy, cz).map(|dist| (d, dist))) {
                Some((dof, dist)) => {
                    let (bx, by) = dof.apply(ctx, cx, cy, dist);
                    x = bx / zr;
                    y = by / zr;
                }
                None => {
                    x = cx / zr;
                    y = cy / zr;
                }
            }
        }
        let px = x * self.cosa + y * self.sina + self.rc_x;
        if px < 0.0 || px > self.cam_w {
            return None;
        }
        let py = y * self.cosa - x * self.sina + self.rc_y;
        if py < 0.0 || py > self.cam_h {
            return None;
        }
        Some((self.bws * px, self.bhs * py, intensity, cam_z * self.bws))
    }

    /// Projection for solid rendering: camera only, DOF becomes a post effect. Returns
    /// `(px, py, intensity, camera z, dof distance)`.
    pub fn project_solid(&self, x: f64, y: f64, z: f64) -> Option<(f64, f64, f64, f64, f64)> {
        let m = &self.matrix;
        let cx = m[0][0] * x + m[1][0] * y + m[2][0] * z + self.cam_pos[0];
        let cy = m[0][1] * x + m[1][1] * y + m[2][1] * z + self.cam_pos[1];
        let cz = m[0][2] * x + m[1][2] * y + m[2][2] * z + self.cam_pos[2];
        let zr = 1.0 - self.cam_perspective * cz + self.cam_pos[2];
        if zr < EPSILON {
            return None;
        }
        let mut intensity = 1.0;
        if self.dimish_z > EPSILON {
            let zdist = self.dim_z_distance - cz;
            if zdist > 0.0 {
                intensity = (-zdist * zdist * self.dimish_z).exp();
            }
        }
        let dof = self.dof.as_ref().and_then(|d| d.distance(cx, cy, cz)).unwrap_or(0.0);
        let (x, y) = (cx / zr, cy / zr);
        let px = x * self.cosa + y * self.sina + self.rc_x;
        if px < 0.0 || px > self.cam_w {
            return None;
        }
        let py = y * self.cosa - x * self.sina + self.rc_y;
        if py < 0.0 || py > self.cam_h {
            return None;
        }
        Some((self.bws * px, self.bhs * py, intensity, cz, dof))
    }

    /// Inverse of the 2D projection for UI interaction: image pixel -> world coordinates.
    /// Ignores the 3D camera.
    pub fn image_to_world(&self, ix: f64, iy: f64) -> (f64, f64) {
        let px = (ix * self.oversample as f64 + self.border as f64) / self.bws - self.rc_x;
        let py = (iy * self.oversample as f64 + self.border as f64) / self.bhs - self.rc_y;
        // Inverse rotation of [cos sin; -sin cos].
        (px * self.cosa - py * self.sina, px * self.sina + py * self.cosa)
    }

    /// World -> image pixel (2D only), used for drawing transform handles.
    pub fn world_to_image(&self, x: f64, y: f64) -> (f64, f64) {
        let px = x * self.cosa + y * self.sina + self.rc_x;
        let py = y * self.cosa - x * self.sina + self.rc_y;
        (
            (self.bws * px - self.border as f64) / self.oversample as f64,
            (self.bhs * py - self.border as f64) / self.oversample as f64,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_maps_like_the_full_view() {
        let mut flame = Flame { width: 800, height: 600, pixels_per_unit: 120.0, cam_roll: 17.0, spatial_oversampling: 2, ..Flame::default() };
        flame.centre_x = 0.3;
        let full = View::new(&flame, 1600, 1200, 7);
        let tile = full.window(400, 300, 512, 256);
        let mut ctx = crate::variations::Ctx::new(1, false);
        for (ix, iy) in [(410.0, 310.0), (600.5, 400.25), (900.0, 550.0)] {
            let (x, y) = full.image_to_world(ix, iy);
            let (fx, fy, ..) = full.project(x, y, 0.0, &mut ctx).unwrap();
            let (tx, ty, ..) = tile.project(x, y, 0.0, &mut ctx).unwrap();
            assert!((fx - tx - 800.0).abs() < 1e-6 && (fy - ty - 600.0).abs() < 1e-6, "{fx} {tx} {fy} {ty}");
        }
        // Outside the window: not projected.
        assert!(tile.project(-3.0, -3.0, 0.0, &mut ctx).is_none());
    }
}

/// The 3D camera rotation for the flame angles in degrees. A world point `w` maps to camera
/// space as `c[j] = sum_i m[i][j] * w[i]`; camera x/y follow the image axes (y down) and a
/// larger camera z is closer to the viewer.
pub fn camera_matrix(yaw: f64, pitch: f64, bank: f64, roll: f64) -> [[f64; 3]; 3] {
    let deg = std::f64::consts::PI / 180.0;
    projection_matrix(-yaw * deg, pitch * deg, bank * deg, roll * deg)
}

fn projection_matrix(yaw: f64, pitch: f64, bank: f64, roll: f64) -> [[f64; 3]; 3] {
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let (sb, cb) = bank.sin_cos();
    let (sr, cr) = roll.sin_cos();
    let mut m = [[0.0; 3]; 3];
    m[0][0] = -cp * sr * sy - (sp * sb * sr - cb * cr) * cy;
    m[1][0] = -cp * cy * sr + (sp * sb * sr - cb * cr) * sy;
    m[2][0] = cb * sp * sr + cr * sb;
    m[0][1] = cp * cr * sy + (cr * sp * sb + cb * sr) * cy;
    m[1][1] = cp * cr * cy - (cr * sp * sb + cb * sr) * sy;
    m[2][1] = -cb * cr * sp + sb * sr;
    m[0][2] = -cp * cy * sb + sp * sy;
    m[1][2] = cp * sb * sy + cy * sp;
    m[2][2] = cp * cb;
    m
}
