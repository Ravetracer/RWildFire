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

//! Quilt rendering (JWildfire's `QuiltFlameRenderer`): images larger than the GPU can render
//! at once, rendered as a grid of tiles and merged into one PNG.
//!
//! Unlike JWildfire, every tile is a window of the same full-size view (see
//! [`rwf_core::render::View::window`]) and is rendered with a margin that is cropped
//! afterwards, so filters and density estimation see their neighbours and the tiles fit
//! together without seams. Quality refers to the full image: each tile gets as many samples
//! as a single render of the whole image would need. Finished tiles are kept as files until
//! the merge, so a cancelled quilt continues where it stopped.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use rwf_core::Flame;

use crate::batch::MAX_SIZE;
use crate::{GpuRenderer, Tile};

/// Default largest tile side (plus margins), a size every GPU handles comfortably.
pub const DEFAULT_TILE_SIZE: u32 = 4096;

/// A quilt render: `flame` at `width` × `height`, split into `cols` × `rows` tiles.
#[derive(Clone, Debug)]
pub struct QuiltJob {
    pub flame: Flame,
    pub width: u32,
    pub height: u32,
    pub cols: u32,
    pub rows: u32,
    pub quality: f64,
    /// The merged PNG.
    pub output: PathBuf,
}

/// A tile without its margin, in pixels of the full image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRect {
    pub col: u32,
    pub row: u32,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Reported by [`render_quilt`].
pub enum QuiltEvent<'a> {
    /// Tile `tile` (of `tiles`) is `fraction` done.
    Progress { tile: usize, tiles: usize, fraction: f32 },
    /// A tile is finished, or `reused` from an earlier run: its pixels, RGBA8.
    TileDone { rect: TileRect, rgba: &'a [u8], reused: bool },
    /// All tiles are done, the PNG is being written.
    Merging,
}

/// Splits `len` pixels into `n` parts (`JWildfire`: equal parts, the last takes the rest).
fn split(len: u32, n: u32) -> Vec<(u32, u32)> {
    let n = n.clamp(1, len.max(1));
    let part = (len as f64 / n as f64 + 0.5) as u32;
    (0..n)
        .map(|i| {
            let start = (i * part).min(len);
            let end = if i + 1 == n { len } else { ((i + 1) * part).min(len) };
            (start, end - start)
        })
        .collect()
}

/// Tile grid for an image so no tile (with margins) is larger than `max_tile`.
pub fn suggest_tiles(width: u32, height: u32, max_tile: u32) -> (u32, u32) {
    (width.div_ceil(max_tile.max(64)), height.div_ceil(max_tile.max(64)))
}

impl QuiltJob {
    pub fn tiles(&self) -> Vec<TileRect> {
        let xs = split(self.width, self.cols);
        let ys = split(self.height, self.rows);
        let mut out = Vec::with_capacity(xs.len() * ys.len());
        for (row, &(y, h)) in ys.iter().enumerate() {
            for (col, &(x, w)) in xs.iter().enumerate() {
                out.push(TileRect { col: col as u32, row: row as u32, x, y, w, h });
            }
        }
        out
    }

    /// Overlap rendered around each tile and cropped afterwards (density estimation, filters,
    /// post blur and the image space effects of solid rendering reach this far).
    pub fn margin(&self) -> u32 {
        let solid = if self.flame.solid.enabled { 96 } else { 0 };
        32 + self.flame.post_blur_radius.min(64) + solid
    }

    /// The tile rendered for `t`: with margins, clipped to the image.
    fn padded(&self, t: &TileRect) -> (u32, u32, u32, u32) {
        let m = self.margin();
        let (x0, y0) = (t.x.saturating_sub(m), t.y.saturating_sub(m));
        let (x1, y1) = ((t.x + t.w + m).min(self.width), (t.y + t.h + m).min(self.height));
        (x0, y0, x1 - x0, y1 - y0)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.width < 16 || self.height < 16 {
            return Err("the image is too small".into());
        }
        let largest = self.tiles().iter().map(|t| self.padded(t)).map(|(_, _, w, h)| w.max(h)).max().unwrap_or(0);
        if largest > MAX_SIZE {
            return Err(format!("tiles are too large ({largest} pixels with margins, at most {MAX_SIZE}): use more tiles"));
        }
        Ok(())
    }

    /// Folder for the finished tiles, next to the output.
    pub fn tile_dir(&self) -> PathBuf {
        let stem = self.output.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "quilt".into());
        self.output.with_file_name(format!("{stem}.quilt-tiles"))
    }

    /// File of a finished tile; the name holds everything that changes its pixels' meaning,
    /// so tiles of another size, grid or quality are not reused.
    pub fn tile_path(&self, t: &TileRect) -> PathBuf {
        let key = format!("{}x{}_{}x{}_q{}_{:016x}", self.width, self.height, self.cols, self.rows, self.quality.round(), flame_hash(&self.flame));
        self.tile_dir().join(format!("{key}_r{:03}_c{:03}.png", t.row, t.col))
    }
}

/// Hash of the flame file text (tiles of an edited flame must not be reused).
fn flame_hash(flame: &Flame) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    rwf_core::io::write_flame(flame).hash(&mut h);
    h.finish()
}

fn read_png(path: &Path) -> Result<(u32, u32, Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = dec.read_info().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("PNG too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
    buf.truncate(info.buffer_size());
    Ok((info.width, info.height, buf))
}

/// Writes a tile atomically (a cancelled write must not look like a finished tile).
fn write_tile(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("png.part");
    rwf_core::video::write_png(&tmp, w, h, rgba).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Renders the quilt; `on` gets the progress and returns false to cancel (finished tiles
/// stay on disk for the next run). Returns the merged PNG.
pub fn render_quilt(r: &mut GpuRenderer, job: &QuiltJob, on: &mut dyn FnMut(QuiltEvent, &mut GpuRenderer) -> bool) -> Result<PathBuf, String> {
    job.check()?;
    let dir = job.tile_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tiles = job.tiles();
    let mut flame = job.flame.clone();
    // The denoiser and the image effects work on whole images; tiles would get visible seams.
    // The tile renders are not post-processed anyway; tiles are read with `read_image`.
    flame.ai_post_denoiser = Default::default();
    r.set_with_alpha(flame.bg_transparency);
    r.set_zbuffer(false);
    for (i, t) in tiles.iter().enumerate() {
        let path = job.tile_path(t);
        if let Ok((w, h, rgba)) = read_png(&path)
            && (w, h) == (t.w, t.h)
        {
            on(QuiltEvent::TileDone { rect: *t, rgba: &rgba, reused: true }, r);
            continue;
        }
        let (px, py, pw, ph) = job.padded(t);
        r.set_tile(Some(Tile { full_width: job.width, full_height: job.height, x: px, y: py }));
        r.set_flame(&flame, pw, ph);
        r.reset_accumulation();
        if let Some(e) = r.last_error() {
            return Err(e.to_string());
        }
        let fv = r.full_view().cloned().ok_or("no view")?;
        let target = (job.quality * fv.raster_width as f64 * fv.raster_height as f64 / fv.oversample as f64) as u64;
        loop {
            let done = r.iterate_towards(target, 4, 128);
            r.wait_idle();
            let fraction = (r.samples() as f64 / target.max(1) as f64).min(1.0) as f32;
            if !on(QuiltEvent::Progress { tile: i, tiles: tiles.len(), fraction }, r) {
                return Err("cancelled".into());
            }
            if done {
                break;
            }
        }
        r.tonemap();
        let img = r.read_image().ok_or("readback failed")?;
        // Crop the margins.
        let (ox, oy) = ((t.x - px) as usize, (t.y - py) as usize);
        let mut core = Vec::with_capacity(t.w as usize * t.h as usize * 4);
        for y in 0..t.h as usize {
            let start = ((oy + y) * pw as usize + ox) * 4;
            core.extend_from_slice(&img[start..start + t.w as usize * 4]);
        }
        write_tile(&path, t.w, t.h, &core)?;
        on(QuiltEvent::TileDone { rect: *t, rgba: &core, reused: false }, r);
    }
    r.set_tile(None);
    on(QuiltEvent::Merging, r);
    merge(job, &tiles)?;
    for t in &tiles {
        let _ = std::fs::remove_file(job.tile_path(t));
    }
    let _ = std::fs::remove_dir(&dir);
    Ok(job.output.clone())
}

/// Streams the tiles into one PNG, one row of tiles in memory at a time.
fn merge(job: &QuiltJob, tiles: &[TileRect]) -> Result<(), String> {
    let file = std::fs::File::create(&job.output).map_err(|e| format!("{}: {e}", job.output.display()))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), job.width, job.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    let mut stream = writer.stream_writer().map_err(|e| e.to_string())?;
    for row in 0..job.rows.max(1) {
        let row_tiles: Vec<&TileRect> = tiles.iter().filter(|t| t.row == row).collect();
        if row_tiles.is_empty() {
            continue;
        }
        let images = row_tiles.iter().map(|t| read_png(&job.tile_path(t))).collect::<Result<Vec<_>, _>>()?;
        for y in 0..row_tiles[0].h as usize {
            for (t, (w, _, rgba)) in row_tiles.iter().zip(&images) {
                debug_assert_eq!(*w, t.w);
                let start = y * t.w as usize * 4;
                stream.write_all(&rgba[start..start + t.w as usize * 4]).map_err(|e| e.to_string())?;
            }
        }
    }
    stream.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_the_image() {
        let job = QuiltJob { flame: Flame::default(), width: 1001, height: 600, cols: 3, rows: 2, quality: 100.0, output: "/tmp/x.png".into() };
        let tiles = job.tiles();
        assert_eq!(tiles.len(), 6);
        assert_eq!(tiles.iter().filter(|t| t.row == 0).map(|t| t.w).sum::<u32>(), 1001);
        assert_eq!(tiles.iter().filter(|t| t.col == 0).map(|t| t.h).sum::<u32>(), 600);
        assert_eq!(tiles[5], TileRect { col: 2, row: 1, x: 668, y: 300, w: 333, h: 300 });
        assert_eq!(job.padded(&tiles[0]), (0, 0, 334 + job.margin(), 300 + job.margin()));
        assert_eq!(suggest_tiles(20000, 8000, 4096), (5, 2));
        assert!(job.tile_path(&tiles[0]).starts_with("/tmp/x.quilt-tiles"));
    }
}
