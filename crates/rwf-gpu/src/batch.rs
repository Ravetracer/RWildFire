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

//! Batch rendering (JWildfire's batch renderer): a queue of flames rendered one after another
//! to image or video files with shared defaults and per-job overrides.

use std::path::{Path, PathBuf};

use rwf_core::Flame;
use rwf_core::output::ImageFormat;
use rwf_core::video::{VideoFormat, VideoSink};

use crate::GpuRenderer;

/// Largest image side the GPU output texture supports (larger images: quilt renderer).
pub const MAX_SIZE: u32 = 8192;

/// Where the flame of a job comes from.
#[derive(Clone, Debug)]
pub enum JobSource {
    /// Flame `index` of a file with `count` flames; read when the job is rendered.
    File { path: PathBuf, index: usize, count: usize },
    /// A flame held in memory (e.g. taken from the editor).
    Flame(Box<Flame>),
}

/// One queued render. `None` fields use the [`BatchSettings`].
#[derive(Clone, Debug)]
pub struct BatchJob {
    pub source: JobSource,
    /// Display name.
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub quality: Option<f64>,
}

/// Default output size of the jobs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SizeMode {
    /// The size stored in the flame.
    Flame,
    /// The longer side has this many pixels, the flame's aspect ratio is kept.
    LongEdge(u32),
    /// Fixed size; the flame is scaled to fit like when its size is changed in the editor.
    Fixed(u32, u32),
}

/// Settings shared by all jobs of a batch.
#[derive(Clone, Debug)]
pub struct BatchSettings {
    pub size: SizeMode,
    pub quality: f64,
    pub format: ImageFormat,
    pub jpeg_quality: u8,
    pub zbuffer: bool,
    /// Render animated flames as videos (all frames) instead of a still image.
    pub animations: bool,
    pub video_format: VideoFormat,
    /// Output folder; `None` writes next to the flame file.
    pub out_dir: Option<PathBuf>,
    /// Render jobs whose output file already exists (otherwise they are skipped).
    pub overwrite: bool,
    /// Run the flame's AI post denoiser.
    pub denoise: bool,
}

impl Default for BatchSettings {
    fn default() -> Self {
        Self {
            size: SizeMode::LongEdge(1920),
            quality: 1000.0,
            format: ImageFormat::Png,
            jpeg_quality: 95,
            zbuffer: false,
            animations: false,
            video_format: VideoFormat::Mp4,
            out_dir: None,
            overwrite: false,
            denoise: true,
        }
    }
}

/// Result of a finished job.
#[derive(Clone, Debug)]
pub struct JobOutcome {
    pub path: PathBuf,
    /// The output existed and was not overwritten.
    pub skipped: bool,
    pub warning: Option<String>,
}

/// Keeps file names portable.
fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || "-_. ".contains(c) { c } else { '_' }).collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "flame".into() } else { s }
}

impl BatchJob {
    /// All flames of a flame file as jobs.
    pub fn from_file(path: &Path) -> Result<Vec<BatchJob>, String> {
        let flames = rwf_core::io::read_flames_from_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let count = flames.len();
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(flames
            .iter()
            .enumerate()
            .map(|(index, f)| {
                let name = match (count, f.name.is_empty()) {
                    (1, _) => stem.clone(),
                    (_, true) => format!("{stem} #{}", index + 1),
                    (_, false) => format!("{stem} › {}", f.name),
                };
                BatchJob { source: JobSource::File { path: path.to_path_buf(), index, count }, name, width: None, height: None, quality: None }
            })
            .collect())
    }

    pub fn from_flame(flame: Flame) -> BatchJob {
        let name = if flame.name.is_empty() { "Untitled".to_string() } else { flame.name.clone() };
        BatchJob { source: JobSource::Flame(Box::new(flame)), name, width: None, height: None, quality: None }
    }

    /// Reads the flame (file jobs read the file again, so later edits are picked up).
    pub fn load_flame(&self) -> Result<Flame, String> {
        match &self.source {
            JobSource::Flame(f) => Ok((**f).clone()),
            JobSource::File { path, index, .. } => {
                let flames = rwf_core::io::read_flames_from_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
                flames.into_iter().nth(*index).ok_or_else(|| format!("{}: flame {} not found", path.display(), index + 1))
            }
        }
    }

    /// Output size for `flame`.
    pub fn size(&self, flame: &Flame, s: &BatchSettings) -> (u32, u32) {
        self.size_for(flame.width, flame.height, s)
    }

    /// Output size for a flame of the given size.
    pub fn size_for(&self, flame_w: u32, flame_h: u32, s: &BatchSettings) -> (u32, u32) {
        let aspect = flame_h.max(1) as f64 / flame_w.max(1) as f64;
        let (w, h) = match (self.width, self.height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, (w as f64 * aspect).round() as u32),
            (None, Some(h)) => ((h as f64 / aspect).round() as u32, h),
            (None, None) => match s.size {
                SizeMode::Flame => (flame_w, flame_h),
                SizeMode::Fixed(w, h) => (w, h),
                SizeMode::LongEdge(n) if aspect <= 1.0 => (n, (n as f64 * aspect).round() as u32),
                SizeMode::LongEdge(n) => ((n as f64 / aspect).round() as u32, n),
            },
        };
        (w.max(16), h.max(16))
    }

    pub fn quality(&self, s: &BatchSettings) -> f64 {
        self.quality.unwrap_or(s.quality).max(1.0)
    }

    /// True if this job renders a video for `flame`.
    pub fn is_animation(flame: &Flame, s: &BatchSettings) -> bool {
        s.animations && flame.animation.is_animated() && flame.animation.frame_count > 1
    }

    /// Output file (or folder, for PNG sequences) for `flame`.
    pub fn output_path(&self, flame: &Flame, s: &BatchSettings) -> Result<PathBuf, String> {
        let (dir, stem) = match &self.source {
            JobSource::File { path, index, count } => {
                let dir = s.out_dir.clone().or_else(|| path.parent().map(Path::to_path_buf)).unwrap_or_default();
                let stem = sanitize(&path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
                (dir, if *count > 1 { format!("{stem}_{:03}", index + 1) } else { stem })
            }
            JobSource::Flame(f) => {
                let dir = s.out_dir.clone().ok_or("choose an output folder for flames without a file")?;
                (dir, sanitize(&f.name))
            }
        };
        Ok(if Self::is_animation(flame, s) {
            match s.video_format.extension() {
                Some(ext) => dir.join(format!("{stem}.{ext}")),
                None => dir.join(format!("{stem}_frames")),
            }
        } else {
            dir.join(format!("{stem}.{}", s.format.extension()))
        })
    }
}

/// Iterates `packets` (one flame state plus motion blur sub-frames) to `quality`. `progress`
/// gets the fraction done and returns false to cancel; returns false if cancelled.
fn accumulate(
    r: &mut GpuRenderer,
    packets: &[Flame],
    w: u32,
    h: u32,
    quality: f64,
    progress: &mut dyn FnMut(f32, &mut GpuRenderer) -> bool,
) -> Result<bool, String> {
    for (i, f) in packets.iter().enumerate() {
        if i == 0 {
            // Always from scratch, also when the renderer still holds the same flame.
            r.set_flame(f, w, h);
            r.reset_accumulation();
        } else {
            r.add_packet(f);
        }
        if let Some(e) = r.last_error() {
            return Err(e.to_string());
        }
        let view = r.view().cloned().ok_or("no view")?;
        let per_packet = (quality * view.raster_width as f64 * view.raster_height as f64 / view.oversample as f64) as u64;
        let target = per_packet * (i as u64 + 1);
        let total = per_packet * packets.len() as u64;
        loop {
            let done = r.iterate_towards(target, 4, 128);
            r.wait_idle();
            if !progress((r.samples() as f64 / total.max(1) as f64).min(1.0) as f32, r) {
                return Ok(false);
            }
            if done {
                break;
            }
        }
    }
    Ok(true)
}

/// Renders one job and writes its output. `progress` gets the fraction done (0..1) and the
/// renderer (e.g. for a preview); returning false cancels the job.
pub fn render_job(
    r: &mut GpuRenderer,
    job: &BatchJob,
    s: &BatchSettings,
    progress: &mut dyn FnMut(f32, &mut GpuRenderer) -> bool,
) -> Result<JobOutcome, String> {
    let mut flame = job.load_flame()?;
    let (w, h) = job.size(&flame, s);
    if w > MAX_SIZE || h > MAX_SIZE {
        return Err(format!("{w} × {h} is too large for the GPU renderer (at most {MAX_SIZE} pixels per side)"));
    }
    let quality = job.quality(s);
    let path = job.output_path(&flame, s)?;
    if !s.overwrite && path.exists() {
        return Ok(JobOutcome { path, skipped: true, warning: None });
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    if !s.denoise {
        flame.ai_post_denoiser = Default::default();
    }
    let animation = BatchJob::is_animation(&flame, s);
    let alpha = !animation && flame.bg_transparency && s.format == ImageFormat::Png;
    r.set_with_alpha(alpha);
    r.set_zbuffer(s.zbuffer && !animation);
    r.set_flame(&flame, w, h);
    let warning =
        (!r.unsupported_variations().is_empty()).then(|| format!("not supported on the GPU, ignored: {}", r.unsupported_variations().join(", ")));
    let cancelled = || Err("cancelled".to_string());

    // Stereo 3D stills: eye images combined (PNG/JPEG only, no z-buffer, as in JWildfire)
    if !animation && flame.stereo3d.is_on() && !s.format.is_hdr() {
        let total = flame.stereo3d.image_count() as f32;
        let mut k = 0.0;
        let mut eye = |f: &Flame, q: f64| -> Result<Vec<u8>, String> {
            let mut sub = |p: f32, r: &mut GpuRenderer| progress((k + p) / total, r);
            if !accumulate(r, std::slice::from_ref(f), w, h, q, &mut sub)? {
                return Err("cancelled".to_string());
            }
            r.tonemap();
            let mut img = r.read_image().ok_or("readback failed")?;
            r.apply_post_processing(f, &mut img, w, h).map_err(|e| format!("denoiser: {e}"))?;
            k += 1.0;
            Ok(img)
        };
        let (img, ow, oh) = rwf_core::stereo::render_stereo(&flame, w, h, quality, &mut eye)?;
        rwf_core::output::write_rgba8(&path, s.format, ow, oh, &img, s.jpeg_quality)?;
        let warning = warning.or_else(|| s.zbuffer.then(|| "no z-buffer for stereo 3D images".to_string()));
        return Ok(JobOutcome { path, skipped: false, warning });
    }
    let warning = if flame.stereo3d.is_on() && !animation && s.format.is_hdr() {
        Some(warning.map_or(String::new(), |w| format!("{w}; ")) + "stereo 3D is only rendered into PNG/JPEG; this HDR image shows the normal view")
    } else {
        warning
    };

    if animation {
        let frames = flame.animation.frame_count;
        let mut sink = VideoSink::new(&path, s.video_format, w, h, flame.animation.fps).map_err(|e| e.to_string())?;
        for frame in 0..frames {
            let packets = rwf_core::render::frame_packets(&flame, frame as f64);
            let mut sub = |p: f32, r: &mut GpuRenderer| progress((frame as f32 + p) / frames as f32, r);
            if !accumulate(r, &packets, w, h, quality, &mut sub)? {
                let _ = sink.finish();
                return cancelled();
            }
            r.tonemap();
            let mut img = r.read_image().ok_or("readback failed")?;
            r.apply_post_processing(&flame, &mut img, w, h).map_err(|e| format!("denoiser: {e}"))?;
            sink.write_frame(frame, &img).map_err(|e| e.to_string())?;
        }
        sink.finish().map_err(|e| e.to_string())?;
    } else {
        if !accumulate(r, std::slice::from_ref(&flame), w, h, quality, progress)? {
            return cancelled();
        }
        if s.format.is_hdr() {
            let rgb = r.read_hdr().ok_or("readback failed")?;
            rwf_core::output::write_float_rgb(&path, s.format, w, h, &rgb)?;
        } else {
            r.tonemap();
            let mut img = r.read_image().ok_or("readback failed")?;
            r.apply_post_processing(&flame, &mut img, w, h).map_err(|e| format!("denoiser: {e}"))?;
            rwf_core::output::write_rgba8(&path, s.format, w, h, &img, s.jpeg_quality)?;
        }
        if s.zbuffer
            && let Some(z) = r.read_zbuffer()
        {
            rwf_core::output::write_gray16_png(&flame.zbuffer_filename.path_for(&path), w, h, &z)?;
        }
    }
    Ok(JobOutcome { path, skipped: false, warning })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flame(w: u32, h: u32, name: &str) -> Flame {
        Flame { width: w, height: h, name: name.into(), ..Flame::default() }
    }

    #[test]
    fn sizes() {
        let f = flame(800, 600, "a");
        let mut s = BatchSettings { size: SizeMode::LongEdge(1600), ..Default::default() };
        let mut job = BatchJob::from_flame(f.clone());
        assert_eq!(job.size(&f, &s), (1600, 1200));
        let tall = flame(600, 800, "b");
        assert_eq!(job.size(&tall, &s), (1200, 1600));
        s.size = SizeMode::Flame;
        assert_eq!(job.size(&f, &s), (800, 600));
        s.size = SizeMode::Fixed(640, 640);
        assert_eq!(job.size(&f, &s), (640, 640));
        job.width = Some(400);
        assert_eq!(job.size(&f, &s), (400, 300));
        job.height = Some(100);
        assert_eq!(job.size(&f, &s), (400, 100));
    }

    #[test]
    fn output_paths() {
        let f = flame(800, 600, "My: flame");
        let s = BatchSettings::default();
        let job = BatchJob {
            source: JobSource::File { path: "/x/y/z.flame".into(), index: 0, count: 1 },
            name: "z".into(),
            width: None,
            height: None,
            quality: None,
        };
        assert_eq!(job.output_path(&f, &s).unwrap(), PathBuf::from("/x/y/z.png"));
        let job = BatchJob { source: JobSource::File { path: "/x/y/z.flame".into(), index: 4, count: 9 }, ..job };
        let s2 = BatchSettings { out_dir: Some("/out".into()), format: ImageFormat::Exr, ..Default::default() };
        assert_eq!(job.output_path(&f, &s2).unwrap(), PathBuf::from("/out/z_005.exr"));
        let job = BatchJob::from_flame(f.clone());
        assert!(job.output_path(&f, &s).is_err());
        assert_eq!(job.output_path(&f, &s2).unwrap(), PathBuf::from("/out/My_ flame.exr"));
    }
}
