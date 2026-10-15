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

//! Mesh generator (JWildfire's "Mesh Gen"): turns a 3D flame into a triangle mesh. The chaos
//! game is binned into a voxel grid (density and color per voxel, in world coordinates), the
//! log density is smoothed and its iso-surface extracted with Surface Nets, then the mesh is
//! smoothed (Taubin, as in JWildfire) and written as OBJ or PLY (with vertex colors).

use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use rayon::prelude::*;

use crate::model::Flame;
use crate::render::prepared::{NEXT_XFORM_TABLE_SIZE, PreparedFlame};
use crate::variations::{Ctx, Pt};

/// An axis-aligned box in world coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Bounds {
    pub fn size(&self) -> [f64; 3] {
        std::array::from_fn(|i| (self.max[i] - self.min[i]).max(1e-9))
    }

    /// Grid dimensions with cubic voxels, `resolution` voxels along the longest side.
    pub fn dims(&self, resolution: u32) -> [usize; 3] {
        let s = self.size();
        let longest = s[0].max(s[1]).max(s[2]);
        std::array::from_fn(|i| ((s[i] / longest * resolution as f64).round() as usize).max(2))
    }
}

/// Settings of a mesh.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshSettings {
    pub bounds: Bounds,
    /// Voxels along the longest side of the box.
    pub resolution: u32,
    /// Average samples per voxel.
    pub quality: f64,
    /// Iso level of the normalized log density (0..1): higher gives thinner shapes.
    pub threshold: f64,
    /// Gaussian blur of the density before the surface is extracted (voxels, 0 = off).
    pub blur: f64,
    /// Dilation of every slice before the blur (JWildfire's "Dilate 3x3/5x5" pre-filters):
    /// kernel size 3 or 5, 0 = off.
    pub dilate: u32,
    /// Taubin smoothing passes of the mesh (0 = off) and its factors.
    pub smooth_passes: u32,
    pub smooth_lambda: f64,
    pub smooth_mu: f64,
}

impl MeshSettings {
    pub fn new(bounds: Bounds) -> Self {
        Self { bounds, resolution: 160, quality: 20.0, threshold: 0.3, blur: 0.8, dilate: 0, smooth_passes: 8, smooth_lambda: 0.5, smooth_mu: -0.52 }
    }
}

/// Samples of the chaos game binned into voxels.
pub struct Volume {
    pub dims: [usize; 3],
    pub bounds: Bounds,
    /// Hits per voxel.
    pub counts: Vec<u32>,
    /// Sums of the sample colors (0..31 per sample and channel).
    pub colors: Vec<[u32; 3]>,
    /// The counts are a density already (0..255, from images), not hits.
    pub linear: bool,
}

const COLOR_SHIFT: u32 = 3;

impl Volume {
    fn index(&self, x: usize, y: usize, z: usize) -> usize {
        (z * self.dims[1] + y) * self.dims[0] + x
    }

    /// Normalized log density (0..1, 1 = the busiest voxels), dilated slice by slice
    /// (kernel `dilate`, 0 = off) and blurred by `blur` voxels.
    pub fn density_filtered(&self, dilate: u32, blur: f64) -> Vec<f32> {
        let mut d = self.density(0.0);
        if dilate >= 3 {
            dilate_slices(&mut d, self.dims, (dilate / 2) as isize);
        }
        if blur > 0.05 {
            gaussian_blur(&mut d, self.dims, blur);
        }
        d
    }

    /// Normalized log density (0..1, 1 = the busiest voxels), blurred by `blur` voxels.
    pub fn density(&self, blur: f64) -> Vec<f32> {
        if self.linear {
            let mut d: Vec<f32> = self.counts.iter().map(|&c| c as f32 / 255.0).collect();
            if blur > 0.05 {
                gaussian_blur(&mut d, self.dims, blur);
            }
            return d;
        }
        let mut nonzero: Vec<u32> = self.counts.iter().copied().filter(|c| *c > 0).collect();
        if nonzero.is_empty() {
            return vec![0.0; self.counts.len()];
        }
        // The 99.5th percentile, so a few hot spots do not flatten everything else.
        let k = ((nonzero.len() - 1) as f64 * 0.995) as usize;
        let reference = *nonzero.select_nth_unstable(k).1 as f32;
        let norm = (1.0 + reference).ln();
        let mut d: Vec<f32> = self.counts.iter().map(|&c| ((1.0 + c as f32).ln() / norm).min(1.0)).collect();
        if blur > 0.05 {
            gaussian_blur(&mut d, self.dims, blur);
        }
        d
    }

    /// Average color (0..1) of a voxel; `None` if it has no hits.
    pub fn color(&self, x: usize, y: usize, z: usize) -> Option<[f32; 3]> {
        let i = self.index(x, y, z);
        let n = self.counts[i];
        (n > 0).then(|| {
            let c = self.colors[i];
            std::array::from_fn(|k| (c[k] as f32 * (1 << COLOR_SHIFT) as f32 / n as f32 / 255.0).min(1.0))
        })
    }

    /// A slice of the density as RGBA8 (top view, y up), the part above `threshold` tinted.
    pub fn slice_rgba(&self, density: &[f32], z: usize, threshold: f64) -> Vec<u8> {
        let [nx, ny, nz] = self.dims;
        let z = z.min(nz - 1);
        let mut out = Vec::with_capacity(nx * ny * 4);
        for y in (0..ny).rev() {
            for x in 0..nx {
                let v = density[self.index(x, y, z)];
                let g = (v.sqrt() * 255.0) as u8;
                if v as f64 > threshold {
                    let c = self.color(x, y, z).unwrap_or([1.0; 3]);
                    out.extend_from_slice(&[(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255]);
                } else {
                    out.extend_from_slice(&[g / 2, g / 2, g / 2, 255]);
                }
            }
        }
        out
    }
}

/// Separable gaussian blur of a 3D field.
/// Maximum over a disk of radius `r` in every z slice (`ErodeTransformer` in dilate mode).
fn dilate_slices(d: &mut [f32], dims: [usize; 3], r: isize) {
    let [nx, ny, nz] = dims;
    let offsets: Vec<(isize, isize)> = (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).filter(|(dx, dy)| dx * dx + dy * dy <= r * r).collect();
    d.par_chunks_mut(nx * ny).take(nz).for_each(|slice| {
        let src = slice.to_vec();
        for y in 0..ny as isize {
            for x in 0..nx as isize {
                let mut m = 0.0f32;
                for (dx, dy) in &offsets {
                    let (sx, sy) = (x + dx, y + dy);
                    if sx >= 0 && sy >= 0 && sx < nx as isize && sy < ny as isize {
                        m = m.max(src[sy as usize * nx + sx as usize]);
                    }
                }
                slice[y as usize * nx + x as usize] = m;
            }
        }
    });
}

fn gaussian_blur(d: &mut [f32], dims: [usize; 3], sigma: f64) {
    let r = (sigma * 2.5).ceil() as isize;
    let kernel: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp() as f32).collect();
    let sum: f32 = kernel.iter().sum();
    let kernel: Vec<f32> = kernel.iter().map(|k| k / sum).collect();
    let [nx, ny, nz] = dims;
    let strides = [1usize, nx, nx * ny];
    for axis in 0..3 {
        let n = dims[axis];
        let stride = strides[axis];
        let src = d.to_vec();
        // Every line along `axis` starts at a voxel whose `axis` coordinate is 0.
        let starts: Vec<usize> = (0..nz)
            .flat_map(|z| (0..ny).flat_map(move |y| (0..nx).map(move |x| (x, y, z))))
            .filter(|&(x, y, z)| [x, y, z][axis] == 0)
            .map(|(x, y, z)| (z * ny + y) * nx + x)
            .collect();
        let lines: Vec<Vec<(usize, f32)>> = starts
            .par_iter()
            .map(|&s| {
                (0..n)
                    .map(|i| {
                        let mut v = 0.0;
                        for (k, w) in kernel.iter().enumerate() {
                            let j = i as isize + k as isize - r;
                            if j >= 0 && (j as usize) < n {
                                v += src[s + j as usize * stride] * w;
                            }
                        }
                        (s + i * stride, v)
                    })
                    .collect()
            })
            .collect();
        for line in lines {
            for (i, v) in line {
                d[i] = v;
            }
        }
    }
}

/// One chaos game per layer, calling `visit` with every visible point (after the finals).
fn chaos_game(prepared: &PreparedFlame, seed: u64, samples: u64, cancel: &AtomicBool, mut visit: impl FnMut(&Pt)) {
    let layers: Vec<_> = prepared.layers.iter().filter(|l| !l.xforms.is_empty()).collect();
    if layers.is_empty() {
        return;
    }
    let per_layer = samples / layers.len() as u64;
    for (li, layer) in layers.iter().enumerate() {
        let mut ctx = Ctx::new(seed ^ (li as u64 * 0x9E37_79B9), prepared.preserve_z);
        ctx.set_palette(&layer.palette);
        let mut p = Pt::default();
        let mut xf = 0usize;
        let mut skip = 0;
        let mut i = 0u64;
        let mut restart = true;
        while i < per_layer {
            if restart {
                p = Pt { x: 2.0 * ctx.random() - 1.0, y: 2.0 * ctx.random() - 1.0, color: ctx.random(), ..Pt::default() };
                xf = 0;
                skip = 30;
                restart = false;
            }
            if i.is_multiple_of(65_536) && cancel.load(Ordering::Relaxed) {
                return;
            }
            i += 1;
            let Some(next) = &layer.xforms[xf].next else { return };
            xf = next[ctx.random_int(NEXT_XFORM_TABLE_SIZE)] as usize;
            let x = &layer.xforms[xf];
            p = x.transform(&mut ctx, &layer.colormap, layer.smooth_gradient, &p);
            if p.is_bad() {
                restart = true;
                continue;
            }
            if skip > 0 {
                skip -= 1;
                continue;
            }
            if !x.is_visible_sample(&mut ctx) {
                continue;
            }
            let mut q = p;
            for f in &layer.finals {
                q = f.transform(&mut ctx, &layer.colormap, layer.smooth_gradient, &q);
            }
            if !q.do_hide && q.x.is_finite() && q.y.is_finite() && q.z.is_finite() {
                visit(&q);
            }
        }
    }
}

/// A box around the bulk of the flame's points (per axis the shortest interval with 92 % of
/// them, plus a margin).
pub fn auto_bounds(flame: &Flame) -> Bounds {
    let prepared = PreparedFlame::new(flame);
    let mut pts: Vec<[f64; 3]> = Vec::with_capacity(200_000);
    chaos_game(&prepared, 1234, 200_000, &AtomicBool::new(false), |q| pts.push([q.x, q.y, q.z]));
    if pts.len() < 100 {
        return Bounds { min: [-1.0; 3], max: [1.0; 3] };
    }
    let mut min = [0.0; 3];
    let mut max = [0.0; 3];
    for a in 0..3 {
        let mut v: Vec<f64> = pts.iter().map(|p| p[a]).collect();
        v.sort_by(f64::total_cmp);
        // The shortest interval with 92 % of the points: follows the dense part, also for
        // flames with long tails (julia3D and the like).
        let m = v.len() * 92 / 100;
        let i = (0..v.len() - m).min_by(|&i, &j| (v[i + m] - v[i]).total_cmp(&(v[j + m] - v[j]))).unwrap_or(0);
        let (lo, hi) = (v[i], v[i + m]);
        let margin = ((hi - lo) * 0.08).max(0.01);
        (min[a], max[a]) = (lo - margin, hi + margin);
    }
    Bounds { min, max }
}

/// Bins `settings.quality` samples per voxel into the grid of `settings.bounds`.
/// `progress` counts the samples done; stops early when `cancel` is set.
pub fn sample_volume(flame: &Flame, settings: &MeshSettings, progress: &AtomicU64, cancel: &AtomicBool) -> Volume {
    let bounds = settings.bounds;
    let dims = bounds.dims(settings.resolution);
    let n = dims[0] * dims[1] * dims[2];
    let counts: Vec<AtomicU32> = (0..n).map(|_| AtomicU32::new(0)).collect();
    let colors: Vec<[AtomicU32; 3]> = (0..n).map(|_| std::array::from_fn(|_| AtomicU32::new(0))).collect();
    let prepared = PreparedFlame::new(flame);
    let total = (settings.quality.max(0.1) * n as f64) as u64;
    let threads = rayon::current_num_threads().max(1) as u64;
    let size = bounds.size();
    let scale: [f64; 3] = std::array::from_fn(|i| dims[i] as f64 / size[i]);
    (0..threads).into_par_iter().for_each(|t| {
        let mut done = 0u64;
        chaos_game(&prepared, 0x5EED_0000 + t * 7919, total / threads, cancel, |q| {
            done += 1;
            if done.is_multiple_of(1 << 16) {
                progress.fetch_add(1 << 16, Ordering::Relaxed);
            }
            let c = [(q.x - bounds.min[0]) * scale[0], (q.y - bounds.min[1]) * scale[1], (q.z - bounds.min[2]) * scale[2]];
            if c.iter().zip(&dims).any(|(v, d)| *v < 0.0 || *v >= *d as f64) {
                return;
            }
            let i = (c[2] as usize * dims[1] + c[1] as usize) * dims[0] + c[0] as usize;
            counts[i].fetch_add(1, Ordering::Relaxed);
            for (k, v) in [q.red, q.green, q.blue].into_iter().enumerate() {
                colors[i][k].fetch_add((v.clamp(0.0, 255.0) as u32) >> COLOR_SHIFT, Ordering::Relaxed);
            }
        });
    });
    Volume {
        dims,
        bounds,
        counts: counts.into_iter().map(AtomicU32::into_inner).collect(),
        colors: colors.into_iter().map(|c| c.map(AtomicU32::into_inner)).collect(),
        linear: false,
    }
}

/// A point of a point cloud (JWildfire's `PCPoint`): position, color (0..1), intensity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudPoint {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub intensity: f32,
}

impl Volume {
    /// The centre of a voxel in world coordinates.
    fn centre(&self, x: usize, y: usize, z: usize) -> [f32; 3] {
        let s = self.bounds.size();
        let c = [x, y, z];
        std::array::from_fn(|i| (self.bounds.min[i] + (c[i] as f64 + 0.5) / self.dims[i] as f64 * s[i]) as f32)
    }

    /// A point cloud (JWildfire's point cloud output): one point per voxel whose density is
    /// above `threshold`, with its average color; the density is the intensity.
    pub fn point_cloud(&self, density: &[f32], threshold: f64) -> Vec<CloudPoint> {
        let [nx, ny, nz] = self.dims;
        let mut out = Vec::new();
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let d = density[self.index(x, y, z)];
                    if d as f64 > threshold {
                        let color = self.color(x, y, z).unwrap_or([1.0; 3]);
                        out.push(CloudPoint { position: self.centre(x, y, z), color, intensity: d });
                    }
                }
            }
        }
        out
    }

    /// A slice as RGBA8 for an image stack (bottom slice first, y up): the voxel color times
    /// the density, so the brightness carries the density (as JWildfire's slice images).
    pub fn slice_image(&self, density: &[f32], z: usize) -> Vec<u8> {
        let [nx, ny, _] = self.dims;
        let mut out = Vec::with_capacity(nx * ny * 4);
        for y in (0..ny).rev() {
            for x in 0..nx {
                let d = density[self.index(x, y, z)].clamp(0.0, 1.0);
                let c = self.color(x, y, z).unwrap_or([0.0; 3]);
                out.extend(c.map(|v| (v * d * 255.0).round() as u8));
                out.push(255);
            }
        }
        out
    }

    /// A volume from an image stack (RGBA8 slices of equal size, bottom first, y up), scaled
    /// so the longest side has `resolution` voxels; the brightness is the density, the hue the
    /// color. The box is centred, 2 units along its longest side.
    pub fn from_slices(slices: &[Vec<u8>], width: usize, height: usize, resolution: u32) -> Option<Volume> {
        let depth = slices.len();
        if depth < 2 || width < 2 || height < 2 || slices.iter().any(|s| s.len() != width * height * 4) {
            return None;
        }
        let longest = width.max(height).max(depth) as f64;
        let f = (resolution as f64 / longest).min(1.0);
        let dims = [((width as f64 * f).round() as usize).max(2), ((height as f64 * f).round() as usize).max(2), ((depth as f64 * f).round() as usize).max(2)];
        let half: [f64; 3] = std::array::from_fn(|i| [width, height, depth][i] as f64 / longest);
        let bounds = Bounds { min: half.map(|h| -h), max: half };
        let n = dims[0] * dims[1] * dims[2];
        let (mut counts, mut colors) = (vec![0u32; n], vec![[0u32; 3]; n]);
        for z in 0..dims[2] {
            let img = &slices[(z * depth / dims[2]).min(depth - 1)];
            for y in 0..dims[1] {
                // images are stored top row first
                let sy = height - 1 - (y * height / dims[1]).min(height - 1);
                for x in 0..dims[0] {
                    let sx = (x * width / dims[0]).min(width - 1);
                    let p = &img[(sy * width + sx) * 4..(sy * width + sx) * 4 + 4];
                    let a = p[3] as f32 / 255.0;
                    let rgb = [p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a];
                    let lum = 0.299 * rgb[0] + 0.588 * rgb[1] + 0.113 * rgb[2];
                    let max = rgb[0].max(rgb[1]).max(rgb[2]);
                    let i = (z * dims[1] + y) * dims[0] + x;
                    let count = lum.round().clamp(0.0, 255.0) as u32;
                    counts[i] = count;
                    if max > 0.0 {
                        // full-brightness hue, scaled like summed samples
                        colors[i] = rgb.map(|v| ((v / max * 255.0) as u32 * count) >> COLOR_SHIFT);
                    }
                }
            }
        }
        Some(Volume { dims, bounds, counts, colors, linear: true })
    }
}

/// Writes the slices of a volume as `slice_0001.png`, … into `dir` (bottom slice first);
/// returns the number of files.
pub fn save_slices(volume: &Volume, density: &[f32], dir: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let [nx, ny, nz] = volume.dims;
    for z in 0..nz {
        let path = dir.join(format!("slice_{:04}.png", z + 1));
        crate::video::write_png(&path, nx as u32, ny as u32, &volume.slice_image(density, z)).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(nz)
}

/// A volume from image files (one slice each, sorted by name, bottom first; PNG, JPEG, …).
pub fn load_slices(paths: &[std::path::PathBuf], resolution: u32) -> Result<Volume, String> {
    let mut paths = paths.to_vec();
    paths.sort();
    let mut slices = Vec::with_capacity(paths.len());
    let mut size = None;
    for p in &paths {
        let img = image::open(p).map_err(|e| format!("{}: {e}", p.display()))?.to_rgba8();
        let dims = (img.width() as usize, img.height() as usize);
        if *size.get_or_insert(dims) != dims {
            return Err(format!("{} is {}×{}, the other slices are {}×{}", p.display(), dims.0, dims.1, size.unwrap().0, size.unwrap().1));
        }
        slices.push(img.into_raw());
    }
    let (w, h) = size.ok_or("no slices")?;
    Volume::from_slices(&slices, w, h, resolution).ok_or_else(|| "at least 2 slices of at least 2×2 pixels are needed".to_string())
}

/// Writes a point cloud as ASCII PLY in JWildfire's layout (position, RGBA, intensity, scale).
pub fn write_points_ply(points: &[CloudPoint], w: &mut impl std::io::Write) -> std::io::Result<()> {
    write!(
        w,
        "ply\nformat ascii 1.0\ncomment RWildFire point cloud\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nproperty uchar alpha\nproperty float intensity\nproperty float pscale\nelement face 0\nproperty list uchar int vertex_indices\nend_header\n",
        points.len()
    )?;
    for p in points {
        let [r, g, b] = p.color.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        writeln!(w, "{} {} {} {r} {g} {b} 255 {} 1", p.position[0], p.position[1], p.position[2], p.intensity)?;
    }
    Ok(())
}

/// Writes a point cloud as OBJ vertices with colors (`v x y z r g b`).
pub fn write_points_obj(points: &[CloudPoint], w: &mut impl std::io::Write) -> std::io::Result<()> {
    writeln!(w, "# RWildFire point cloud, {} points", points.len())?;
    for p in points {
        writeln!(w, "v {} {} {} {} {} {}", p.position[0], p.position[1], p.position[2], p.color[0], p.color[1], p.color[2])?;
    }
    Ok(())
}

/// Writes a point cloud as OBJ or PLY by the file extension.
pub fn save_points(points: &[CloudPoint], path: &Path) -> std::io::Result<()> {
    let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
        Some("ply") => write_points_ply(points, &mut w)?,
        _ => write_points_obj(points, &mut w)?,
    }
    w.flush()
}

/// A triangle mesh with optional vertex colors.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

/// Extracts the surface where `field` crosses `iso` (Surface Nets; outside the grid counts as
/// empty, so the surfaces are closed). Positions are in world coordinates, the triangles
/// face outwards (counter-clockwise).
pub fn surface_nets(volume: &Volume, field: &[f32], iso: f32) -> Mesh {
    let [nx, ny, nz] = volume.dims;
    let value = |x: isize, y: isize, z: isize| -> f32 {
        if x < 0 || y < 0 || z < 0 || x >= nx as isize || y >= ny as isize || z >= nz as isize {
            0.0
        } else {
            field[(z as usize * ny + y as usize) * nx + x as usize]
        }
    };
    // Cells have their minimum corner at -1..n-1 on every axis.
    let (cx, cy, cz) = (nx + 1, ny + 1, nz + 1);
    let cell_index = |x: isize, y: isize, z: isize| ((z + 1) as usize * cy + (y + 1) as usize) * cx + (x + 1) as usize;
    let mut cell_vertex = vec![u32::MAX; cx * cy * cz];
    let mut mesh = Mesh::default();
    let size = volume.bounds.size();
    let voxel: [f64; 3] = std::array::from_fn(|i| size[i] / volume.dims[i] as f64);
    const CORNERS: [[isize; 3]; 8] = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0], [0, 0, 1], [1, 0, 1], [0, 1, 1], [1, 1, 1]];
    const EDGES: [(usize, usize); 12] = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];
    for z in -1..nz as isize {
        for y in -1..ny as isize {
            for x in -1..nx as isize {
                let v: [f32; 8] = std::array::from_fn(|i| value(x + CORNERS[i][0], y + CORNERS[i][1], z + CORNERS[i][2]));
                let inside = v.iter().filter(|&&a| a > iso).count();
                if inside == 0 || inside == 8 {
                    continue;
                }
                let mut sum = [0.0f32; 3];
                let mut n = 0.0;
                for &(a, b) in &EDGES {
                    if (v[a] > iso) != (v[b] > iso) {
                        let t = (iso - v[a]) / (v[b] - v[a]);
                        for k in 0..3 {
                            sum[k] += CORNERS[a][k] as f32 + t * (CORNERS[b][k] - CORNERS[a][k]) as f32;
                        }
                        n += 1.0;
                    }
                }
                let local = sum.map(|s| s / n);
                let grid = [x as f32 + local[0], y as f32 + local[1], z as f32 + local[2]];
                // Voxel centres sit at +0.5.
                let pos: [f32; 3] = std::array::from_fn(|k| (volume.bounds.min[k] + (grid[k] as f64 + 0.5) * voxel[k]) as f32);
                // Color: average of the corner voxels above the iso level.
                let mut col = [0.0f32; 3];
                let mut cn = 0.0;
                for (i, c) in CORNERS.iter().enumerate() {
                    let (px, py, pz) = (x + c[0], y + c[1], z + c[2]);
                    if v[i] > iso
                        && px >= 0
                        && py >= 0
                        && pz >= 0
                        && let Some(rgb) = volume.color(px as usize, py as usize, pz as usize)
                    {
                        for k in 0..3 {
                            col[k] += rgb[k];
                        }
                        cn += 1.0;
                    }
                }
                cell_vertex[cell_index(x, y, z)] = mesh.positions.len() as u32;
                mesh.positions.push(pos);
                mesh.colors.push(if cn > 0.0 { col.map(|c| c / cn) } else { [0.7; 3] });
            }
        }
    }
    // A quad around every grid edge whose ends lie on different sides.
    for z in -1..nz as isize {
        for y in -1..ny as isize {
            for x in -1..nx as isize {
                let p = [x, y, z];
                let here = value(x, y, z) > iso;
                for a in 0..3 {
                    let (b, c) = ((a + 1) % 3, (a + 2) % 3);
                    // The four cells around the edge need p[b], p[c] >= 0.
                    if p[b] < 0 || p[c] < 0 {
                        continue;
                    }
                    let mut q = p;
                    q[a] += 1;
                    if q[a] > [nx, ny, nz][a] as isize {
                        continue;
                    }
                    let there = value(q[0], q[1], q[2]) > iso;
                    if here == there {
                        continue;
                    }
                    let cell = |db: isize, dc: isize| {
                        let mut m = p;
                        m[b] -= db;
                        m[c] -= dc;
                        cell_vertex[cell_index(m[0], m[1], m[2])]
                    };
                    let quad = [cell(0, 0), cell(1, 0), cell(1, 1), cell(0, 1)];
                    if quad.contains(&u32::MAX) {
                        continue;
                    }
                    // (e_b × e_c) = e_a: this order faces +a, outwards when `here` is inside.
                    let [q0, q1, q2, q3] = if here { quad } else { [quad[0], quad[3], quad[2], quad[1]] };
                    mesh.triangles.push([q0, q1, q2]);
                    mesh.triangles.push([q0, q2, q3]);
                }
            }
        }
    }
    mesh
}

impl Mesh {
    /// Taubin smoothing (`Mesh.taubinSmooth`): alternating Laplacian steps with `lambda` and
    /// `mu` (shrinks less than plain Laplacian smoothing).
    pub fn taubin_smooth(&mut self, passes: u32, lambda: f64, mu: f64) {
        let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); self.positions.len()];
        for t in &self.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                if !neighbours[a as usize].contains(&b) {
                    neighbours[a as usize].push(b);
                }
                if !neighbours[b as usize].contains(&a) {
                    neighbours[b as usize].push(a);
                }
            }
        }
        for _ in 0..passes {
            for factor in [lambda as f32, mu as f32] {
                let old = self.positions.clone();
                self.positions.par_iter_mut().enumerate().for_each(|(i, p)| {
                    let nb = &neighbours[i];
                    if nb.is_empty() {
                        return;
                    }
                    let mut avg = [0.0f32; 3];
                    for &j in nb {
                        for k in 0..3 {
                            avg[k] += old[j as usize][k];
                        }
                    }
                    for k in 0..3 {
                        p[k] += factor * (avg[k] / nb.len() as f32 - p[k]);
                    }
                });
            }
        }
    }

    /// Area-weighted vertex normals.
    pub fn normals(&self) -> Vec<[f32; 3]> {
        let mut n = vec![[0.0f32; 3]; self.positions.len()];
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| self.positions[i as usize]);
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cr = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            for &i in t {
                for k in 0..3 {
                    n[i as usize][k] += cr[k];
                }
            }
        }
        for v in &mut n {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-20);
            *v = v.map(|c| c / l);
        }
        n
    }

    /// Wavefront OBJ, optionally with vertex colors (`v x y z r g b`) and normals.
    pub fn write_obj(&self, w: &mut impl std::io::Write, colors: bool, normals: bool) -> std::io::Result<()> {
        writeln!(w, "# RWildFire mesh: {} vertices, {} triangles", self.positions.len(), self.triangles.len())?;
        for (i, p) in self.positions.iter().enumerate() {
            match self.colors.get(i).filter(|_| colors) {
                Some(c) => writeln!(w, "v {} {} {} {:.4} {:.4} {:.4}", p[0], p[1], p[2], c[0], c[1], c[2])?,
                None => writeln!(w, "v {} {} {}", p[0], p[1], p[2])?,
            }
        }
        if normals {
            for n in self.normals() {
                writeln!(w, "vn {:.5} {:.5} {:.5}", n[0], n[1], n[2])?;
            }
        }
        for t in &self.triangles {
            let [a, b, c] = t.map(|i| i + 1);
            if normals {
                writeln!(w, "f {a}//{a} {b}//{b} {c}//{c}")?;
            } else {
                writeln!(w, "f {a} {b} {c}")?;
            }
        }
        Ok(())
    }

    /// Binary little-endian PLY with vertex colors.
    pub fn write_ply(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        write!(
            w,
            "ply\nformat binary_little_endian 1.0\ncomment RWildFire mesh\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nelement face {}\nproperty list uchar int vertex_indices\nend_header\n",
            self.positions.len(),
            self.triangles.len()
        )?;
        let mut buf = Vec::with_capacity(self.positions.len() * 15 + self.triangles.len() * 13);
        for (i, p) in self.positions.iter().enumerate() {
            for v in p {
                buf.extend_from_slice(&v.to_le_bytes());
            }
            let c = self.colors.get(i).copied().unwrap_or([0.7; 3]);
            buf.extend(c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
        for t in &self.triangles {
            buf.push(3);
            for i in t {
                buf.extend_from_slice(&(*i as i32).to_le_bytes());
            }
        }
        w.write_all(&buf)
    }

    /// A Sunflow scene with the mesh (JWildfire's "Sunflow export" of the mesh generator):
    /// JWildfire's example scene (camera, sun-sky light, ground plate) with the mesh scaled to
    /// 4 units, centred and standing on the plate, in a shiny shader of its average color.
    pub fn write_sunflow(&self, w: &mut impl std::io::Write) -> std::io::Result<()> {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for p in &self.positions {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        let extent = (0..3).map(|i| hi[i] - lo[i]).fold(1e-6f32, f32::max);
        let scale = 4.0 / extent;
        let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
        let n = self.colors.len().max(1) as f32;
        let avg: [f32; 3] = if self.colors.is_empty() { [0.49, 0.91, 0.0] } else { std::array::from_fn(|k| self.colors.iter().map(|c| c[k]).sum::<f32>() / n) };
        let rgb = |c: [f32; 3]| format!("{{ \"sRGB nonlinear\" {} {} {} }}", c[0], c[1], c[2]);
        write!(
            w,
            "image {{\n  resolution 640 480\n  aa 1 1\n  samples 4\n  filter gaussian\n}}\n\
             camera {{\n  type pinhole\n  eye 3.27743673325 -9.07978439331 9.93055152893\n  target 0.0 0.0 0.0\n  up 0.0 0.0 1.0\n  fov 60.0\n  aspect 1.0\n}}\n\
             light {{\n  type sunsky\n  up 0.0 0.0 1.0\n  east 0.0 1.0 0.0\n  sundir 1.0 -1.0 0.31\n  turbidity 2.0\n  samples 16\n}}\n\
             shader {{\n  name ground\n  type diffuse\n  diff {}\n}}\n\
             shader {{\n  name shader_shiny\n  type shiny\n  diff {}\n  refl 0.1\n}}\n",
            rgb([0.7, 0.5, 0.5]),
            rgb(avg)
        )?;
        // ground plate (JWildfire's example scene)
        write!(
            w,
            "object {{\n  shader ground\n  type generic-mesh\n  name Plane\n  points 8\n    7.1 7.1 -1.0\n    7.1 -7.1 -1.0\n    -7.1 -7.1 -1.0\n    -7.1 7.1 -1.0\n    -7.1 7.1 -1.61\n    -7.1 -7.1 -1.61\n    7.1 -7.1 -1.61\n    7.1 7.1 -1.61\n  triangles 12\n    0 3 2\n    0 2 1\n    2 3 4\n    2 4 5\n    3 0 7\n    3 7 4\n    0 1 6\n    0 6 7\n    1 2 5\n    1 5 6\n    5 4 7\n    5 7 6\n  normals none\n  uvs none\n}}\n"
        )?;
        writeln!(w, "object {{\n  shader shader_shiny\n  type generic-mesh\n  name \"generated mesh\"\n  points {}", self.positions.len())?;
        for p in &self.positions {
            writeln!(w, "    {} {} {}", (p[0] - centre[0]) * scale, (p[1] - centre[1]) * scale, (p[2] - lo[2]) * scale - 1.0)?;
        }
        writeln!(w, "  triangles {}", self.triangles.len())?;
        for t in &self.triangles {
            writeln!(w, "    {} {} {}", t[0], t[1], t[2])?;
        }
        writeln!(w, "  normals vertex")?;
        for nrm in self.normals() {
            writeln!(w, "    {} {} {}", nrm[0], nrm[1], nrm[2])?;
        }
        writeln!(w, "  uvs none\n}}")
    }

    /// Writes OBJ, PLY or a Sunflow scene (`.sc`) by the file extension.
    pub fn save(&self, path: &Path, colors: bool, normals: bool) -> std::io::Result<()> {
        let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
        match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
            Some("ply") => self.write_ply(&mut w)?,
            Some("sc") => self.write_sunflow(&mut w)?,
            _ => self.write_obj(&mut w, colors, normals)?,
        }
        w.flush()
    }
}

/// A shaded view of the mesh (RGBA8, z-buffered, light from the viewer), turned by `yaw` and
/// `pitch` (radians) around the centre of its bounding box and scaled by `zoom`.
pub fn render_preview(mesh: &Mesh, yaw: f64, pitch: f64, zoom: f64, w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    let mut img = vec![0u8; w * h * 4];
    for px in img.chunks_mut(4) {
        px.copy_from_slice(&[18, 19, 23, 255]);
    }
    if mesh.positions.is_empty() {
        return img;
    }
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for p in &mesh.positions {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let centre: [f32; 3] = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
    let radius = (0..3).map(|k| (hi[k] - lo[k]) / 2.0).fold(0.0f32, |a, b| a.hypot(b)).max(1e-6);
    let scale = (w.min(h) as f32 / 2.0 / radius) * zoom as f32;
    let (sy, cy) = (yaw as f32).sin_cos();
    let (sp, cp) = (pitch as f32).sin_cos();
    // View space: x right, y up, z towards the viewer.
    let view: Vec<[f32; 3]> = mesh
        .positions
        .iter()
        .map(|p| {
            let (x, y, z) = (p[0] - centre[0], p[1] - centre[1], p[2] - centre[2]);
            let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
            let (y, z) = (y * cp - z * sp, y * sp + z * cp);
            [w as f32 / 2.0 + x * scale, h as f32 / 2.0 - y * scale, z]
        })
        .collect();
    let mut depth = vec![f32::MIN; w * h];
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|i| view[i as usize]);
        // Screen y points down, so front faces are clockwise here.
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if area >= 0.0 {
            continue;
        }
        // Shading from the view-space normal.
        let [pa, pb, pc] = t.map(|i| {
            let v = view[i as usize];
            [v[0], -v[1], v[2] * scale]
        });
        let u = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
        let v = [pc[0] - pa[0], pc[1] - pa[1], pc[2] - pa[2]];
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let shade = 0.25 + 0.75 * (n[2] / (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-20)).abs();
        let col = t.map(|i| mesh.colors.get(i as usize).copied().unwrap_or([0.8; 3]));
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(w - 1);
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let y1 = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(h - 1);
        if x0 > x1 || y0 > y1 {
            continue;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((b[0] - px) * (c[1] - py) - (b[1] - py) * (c[0] - px)) / area;
                let w1 = ((c[0] - px) * (a[1] - py) - (c[1] - py) * (a[0] - px)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let i = y * w + x;
                if z <= depth[i] {
                    continue;
                }
                depth[i] = z;
                for k in 0..3 {
                    let v = (w0 * col[0][k] + w1 * col[1][k] + w2 * col[2][k]) * shade;
                    img[i * 4 + k] = (v.clamp(0.0, 1.0) * 255.0) as u8;
                }
            }
        }
    }
    img
}

/// The whole pipeline: volume, density, surface and smoothing.
pub fn generate(flame: &Flame, settings: &MeshSettings, progress: &AtomicU64, cancel: &AtomicBool) -> Option<(Volume, Mesh)> {
    let volume = sample_volume(flame, settings, progress, cancel);
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    let density = volume.density_filtered(settings.dilate, settings.blur);
    let mut mesh = surface_nets(&volume, &density, settings.threshold as f32);
    if settings.smooth_passes > 0 {
        mesh.taubin_smooth(settings.smooth_passes, settings.smooth_lambda, settings.smooth_mu);
    }
    Some((volume, mesh))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ball of density 1 in a 20³ grid.
    fn ball() -> (Volume, Vec<f32>) {
        let n = 20;
        let bounds = Bounds { min: [-1.0; 3], max: [1.0; 3] };
        let mut field = vec![0.0f32; n * n * n];
        let mut counts = vec![0u32; n * n * n];
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    let c = |v: usize| (v as f32 + 0.5) / n as f32 * 2.0 - 1.0;
                    if c(x).powi(2) + c(y).powi(2) + c(z).powi(2) < 0.5 {
                        field[(z * n + y) * n + x] = 1.0;
                        counts[(z * n + y) * n + x] = 1;
                    }
                }
            }
        }
        let colors = counts.iter().map(|&c| [c * 31, 0, 0]).collect();
        (Volume { dims: [n; 3], bounds, counts, colors, linear: false }, field)
    }

    #[test]
    fn ball_surface_is_closed_and_faces_outwards() {
        let (vol, field) = ball();
        let mesh = surface_nets(&vol, &field, 0.5);
        assert!(mesh.triangles.len() > 500, "{}", mesh.triangles.len());
        // Closed: every edge is used by exactly two triangles, in opposite directions.
        let mut edges = std::collections::HashMap::new();
        for t in &mesh.triangles {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *edges.entry((a, b)).or_insert(0) += 1;
            }
        }
        assert!(edges.iter().all(|(&(a, b), &n)| n == 1 && edges.get(&(b, a)) == Some(&1)), "open or inconsistent edges");
        // Outward: the normals point away from the centre.
        let normals = mesh.normals();
        let outward = mesh.positions.iter().zip(&normals).filter(|(p, n)| p[0] * n[0] + p[1] * n[1] + p[2] * n[2] > 0.0).count();
        assert_eq!(outward, mesh.positions.len());
        // Radius about sqrt(0.5).
        let r: f32 = mesh.positions.iter().map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt()).sum::<f32>() / mesh.positions.len() as f32;
        assert!((r - 0.5f32.sqrt()).abs() < 0.08, "{r}");
        // Red vertex colors from the voxels.
        assert!(mesh.colors.iter().all(|c| c[0] > 0.9 && c[1] == 0.0));
    }

    #[test]
    fn smoothing_keeps_the_size() {
        let (vol, field) = ball();
        let mut mesh = surface_nets(&vol, &field, 0.5);
        let radius = |m: &Mesh| m.positions.iter().map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt()).sum::<f32>() / m.positions.len() as f32;
        let before = radius(&mesh);
        mesh.taubin_smooth(10, 0.5, -0.52);
        assert!((radius(&mesh) - before).abs() < 0.03, "{} -> {}", before, radius(&mesh));
    }

    #[test]
    fn writes_obj_and_ply() {
        let (vol, field) = ball();
        let mesh = surface_nets(&vol, &field, 0.5);
        let mut obj = Vec::new();
        mesh.write_obj(&mut obj, true, true).unwrap();
        let text = String::from_utf8(obj).unwrap();
        assert_eq!(text.lines().filter(|l| l.starts_with("v ")).count(), mesh.positions.len());
        assert_eq!(text.lines().filter(|l| l.starts_with("f ")).count(), mesh.triangles.len());
        let mut ply = Vec::new();
        mesh.write_ply(&mut ply).unwrap();
        let header_end = ply.windows(11).position(|w| w == b"end_header\n").unwrap() + 11;
        assert_eq!(ply.len() - header_end, mesh.positions.len() * 15 + mesh.triangles.len() * 13);
    }

    #[test]
    fn preview_shows_the_ball() {
        let (vol, field) = ball();
        let mesh = surface_nets(&vol, &field, 0.5);
        let img = render_preview(&mesh, 0.3, 0.2, 1.0, 64, 48);
        let lit = |x: usize, y: usize| img[(y * 64 + x) * 4] > 60;
        assert!(lit(32, 24), "centre not drawn");
        assert!(!lit(1, 1), "corner drawn");
    }

    #[test]
    fn flame_mesh() {
        let mut flame = crate::random::generate(crate::random::Generator::Classic, &mut crate::rng::Rng::new(3));
        // Some depth.
        for xf in &mut flame.layers[0].xforms {
            xf.variations.push(crate::model::VariationInstance::new("blur3D", 0.05));
        }
        let mut s = MeshSettings::new(auto_bounds(&flame));
        s.resolution = 48;
        s.quality = 5.0;
        let (vol, mesh) = generate(&flame, &s, &AtomicU64::new(0), &AtomicBool::new(false)).unwrap();
        assert!(vol.counts.iter().any(|c| *c > 0));
        assert!(mesh.triangles.len() > 100, "{} triangles", mesh.triangles.len());
    }

    #[test]
    fn dilate_grows_a_point_to_a_disk() {
        let dims = [9, 9, 2];
        let mut d = vec![0.0f32; 9 * 9 * 2];
        d[4 * 9 + 4] = 1.0;
        dilate_slices(&mut d, dims, 1);
        assert_eq!(d[..81].iter().filter(|v| **v == 1.0).count(), 5, "radius 1 disk: centre and 4 neighbours");
        assert!(d[81..].iter().all(|v| *v == 0.0), "other slices untouched");
    }

    #[test]
    fn point_cloud_covers_the_ball() {
        let (volume, field) = ball();
        let pts = volume.point_cloud(&field, 0.5);
        assert_eq!(pts.len(), field.iter().filter(|v| **v > 0.5).count());
        assert!(pts.iter().all(|p| p.position.iter().all(|c| c.abs() < 1.0) && p.color[0] > 0.9));
        let mut ply = Vec::new();
        write_points_ply(&pts, &mut ply).unwrap();
        let text = String::from_utf8(ply).unwrap();
        assert!(text.contains(&format!("element vertex {}", pts.len())) && text.lines().count() == pts.len() + 16);
    }

    #[test]
    fn slices_round_trip_to_a_mesh() {
        let (volume, field) = ball();
        let slices: Vec<Vec<u8>> = (0..volume.dims[2]).map(|z| volume.slice_image(&field, z)).collect();
        let back = Volume::from_slices(&slices, volume.dims[0], volume.dims[1], 64).unwrap();
        assert_eq!(back.dims, volume.dims);
        let d = back.density(0.0);
        // the red ball comes back where it was, with its color
        let i = (10 * 20 + 10) * 20 + 10;
        assert!(d[i] > 0.2 && d[0] == 0.0, "{} {}", d[i], d[0]);
        assert!(back.color(10, 10, 10).unwrap()[0] > 0.9);
        let mesh = surface_nets(&back, &d, 0.1);
        assert!(!mesh.triangles.is_empty());
    }

    #[test]
    fn sunflow_scene_holds_the_mesh() {
        let (volume, field) = ball();
        let mesh = surface_nets(&volume, &field, 0.5);
        let mut sc = Vec::new();
        mesh.write_sunflow(&mut sc).unwrap();
        let text = String::from_utf8(sc).unwrap();
        assert!(text.contains(&format!("points {}", mesh.positions.len())));
        assert!(text.contains(&format!("triangles {}", mesh.triangles.len())));
        assert_eq!(text.matches("object {").count(), 2);
    }
}
