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

//! AI post denoiser (port of `org.jwildfire.create.tina.render.denoiser`).
//!
//! The denoising is done by Intel Open Image Denoise: the library loaded at runtime (see
//! [`super::oidn`]), or else, like JWildfire, its command line tool `oidnDenoise` (found via
//! `JWF_OIDN_DENOISE` or `PATH`; images are exchanged as PFM files). `Optix` flames are
//! denoised with OIDN on an NVIDIA GPU (CUDA) when there is one.

use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use rayon::prelude::*;

use super::oidn::{self, DeviceType};
use crate::model::Flame;

/// `AIPostDenoiserType`
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Denoiser {
    #[default]
    None,
    Oidn,
    Optix,
}

impl Denoiser {
    pub const ALL: &'static [Denoiser] = &[Denoiser::None, Denoiser::Oidn, Denoiser::Optix];

    pub fn as_str(&self) -> &'static str {
        match self {
            Denoiser::None => "NONE",
            Denoiser::Oidn => "OIDN",
            Denoiser::Optix => "OPTIX",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Denoiser::None => "Off",
            Denoiser::Oidn => "OIDN",
            Denoiser::Optix => "OIDN on NVIDIA GPU",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|d| d.as_str() == s.trim())
    }
}

/// Environment variable with the path of the `oidnDenoise` executable.
pub const TOOL_ENV: &str = "JWF_OIDN_DENOISE";

/// The `oidnDenoise` executable, if available.
pub fn find_tool() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(TOOL_ENV).map(PathBuf::from) {
        return p.is_file().then_some(p);
    }
    let exe = if cfg!(windows) { "oidnDenoise.exe" } else { "oidnDenoise" };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(exe)).find(|p| p.is_file())
}

/// What the denoiser gets from the render besides the image: helper images for OIDN, per
/// pixel RGB floats (the albedo, 0..1, and for solid flames the surface normals, -1..1; empty
/// or `None` if not available), and the samples per pixel of flat flames (for
/// [`Flame::post_denoiser_detail`]).
#[derive(Clone, Debug, Default)]
pub struct DenoiseAux {
    pub albedo: Vec<f32>,
    pub normal: Option<Vec<f32>>,
    pub density: Option<Vec<f32>>,
}

/// What [`apply_post_denoiser`] did.
#[derive(Clone, Debug, PartialEq)]
pub enum DenoiseOutcome {
    /// Denoised; how (e.g. "OIDN on NVIDIA GPU").
    Denoised(String),
    /// Left unchanged, and why.
    Skipped(String),
}

impl DenoiseOutcome {
    pub fn denoised(&self) -> bool {
        matches!(self, DenoiseOutcome::Denoised(_))
    }

    /// "denoised with …" / "not denoised: …".
    pub fn describe(&self) -> String {
        match self {
            DenoiseOutcome::Denoised(how) => format!("denoised with {how}"),
            DenoiseOutcome::Skipped(why) => format!("not denoised: {why}"),
        }
    }
}

/// Why the flame's post denoiser would not run for a render (`None`: it runs, if a denoiser
/// is installed).
pub fn skip_reason(flame: &Flame, gpu_render: bool) -> Option<&'static str> {
    if flame.ai_post_denoiser == Denoiser::None {
        Some("the flame's denoiser is off")
    } else if gpu_render && flame.post_denoiser_only_for_cpu {
        Some("\"Only for CPU renders\" is ticked and this is a GPU render")
    } else if flame.post_denoiser_blend >= 1.0 {
        Some("Blend is 1 (only the original image)")
    } else {
        None
    }
}

/// Applies the flame's post denoiser to a finished RGBA8 image (`FlameRenderer.renderFlame`)
/// and tells what happened: skipped if the flame has no denoiser, it is disabled for GPU
/// renders, the blend leaves the image unchanged, or no denoiser is installed.
pub fn apply_post_denoiser(flame: &Flame, rgba: &mut [u8], width: u32, height: u32, gpu_render: bool) -> Result<DenoiseOutcome, String> {
    apply_post_denoiser_with(flame, rgba, width, height, gpu_render, None)
}

/// [`apply_post_denoiser`] with helper images (see [`DenoiseAux`]); they are used by the OIDN
/// library, not by the `oidnDenoise` tool.
pub fn apply_post_denoiser_with(flame: &Flame, rgba: &mut [u8], width: u32, height: u32, gpu_render: bool, aux: Option<&DenoiseAux>) -> Result<DenoiseOutcome, String> {
    apply_post_denoiser_from(flame, rgba, None, width, height, gpu_render, aux)
}

/// [`apply_post_denoiser_with`] that denoises `source` (the same image without spatial filter,
/// as RGBA8 of the same size) instead of `rgba`, and writes the result into `rgba`; Blend
/// mixes the original `rgba` back in. The denoiser removes independent pixel noise well, but
/// keeps the grain that a spatial filter has spread into small blobs.
pub fn apply_post_denoiser_from(
    flame: &Flame,
    rgba: &mut [u8],
    source: Option<&[u8]>,
    width: u32,
    height: u32,
    gpu_render: bool,
    aux: Option<&DenoiseAux>,
) -> Result<DenoiseOutcome, String> {
    let source = source.filter(|s| s.len() == rgba.len());
    if let Some(why) = skip_reason(flame, gpu_render) {
        return Ok(DenoiseOutcome::Skipped(why.into()));
    }
    let prefer = if flame.ai_post_denoiser == Denoiser::Optix { DeviceType::Cuda } else { DeviceType::Default };
    let post = PostProcess::of(flame, aux, width, height);
    let library = oidn::library();
    let mut how = if library.is_ok() {
        let used = denoise_with_library(rgba, source, width, height, &post, prefer, aux)?;
        let mut how = format!("OIDN on {}", used.label());
        let albedo = aux.is_some_and(|a| !a.albedo.is_empty());
        match aux.and_then(|a| a.normal.as_ref()) {
            Some(_) if albedo => how.push_str(", with albedo and normals"),
            _ if albedo => how.push_str(", with albedo"),
            _ => {}
        }
        if prefer == DeviceType::Cuda && used != DeviceType::Cuda {
            how.push_str(" (no usable NVIDIA GPU)");
        }
        how
    } else {
        let Some(tool) = find_tool() else {
            let why = library.err().unwrap_or_default();
            return Ok(DenoiseOutcome::Skipped(format!("Open Image Denoise not found ({why})")));
        };
        denoise_with_tool(&tool, rgba, source, width, height, &post)?;
        "the oidnDenoise tool".into()
    };
    if post.detail.is_some() {
        how.push_str(", converged detail kept");
    } else if flame.post_denoiser_detail > 0.0 {
        how.push_str(", detail not kept (no sample counts)");
    }
    if post.sharpen > 0.0 {
        how.push_str(", sharpened");
    }
    Ok(DenoiseOutcome::Denoised(how))
}

/// How the denoised image is finished and mixed with the original.
struct PostProcess {
    blend: f64,
    sharpen: f64,
    /// Per pixel share of the original image kept for its sample count (see
    /// [`detail_weights`]); `None` if off or the render gave no counts.
    detail: Option<Vec<f32>>,
}

impl PostProcess {
    fn of(flame: &Flame, aux: Option<&DenoiseAux>, width: u32, height: u32) -> Self {
        let n = width as usize * height as usize;
        let detail = aux
            .and_then(|a| a.density.as_deref())
            .filter(|d| d.len() == n && flame.post_denoiser_detail > 0.0)
            .map(|d| detail_weights(d, flame.post_denoiser_detail));
        Self { blend: flame.post_denoiser_blend, sharpen: flame.post_denoiser_sharpen, detail }
    }

    /// Sharpens the denoised RGB (0..1) and writes it into the RGBA8 image, mixing in the
    /// original: the global blend, or more where the pixel had enough samples.
    fn finish(&self, rgba: &mut [u8], denoised: &mut [f32], width: u32, height: u32) {
        if self.sharpen > 0.0 {
            sharpen_cas(denoised, width as usize, height as usize, self.sharpen as f32);
        }
        let b = self.blend.clamp(0.0, 1.0) as f32;
        // Converged pixels keep the original only at structures: in smooth areas the
        // denoised image is as good and has no leftover grain.
        let edges = self.detail.as_ref().map(|_| edge_mask(denoised, width as usize, height as usize));
        for (i, (px, d)) in rgba.chunks_exact_mut(4).zip(denoised.chunks_exact(3)).enumerate() {
            let b = match (&self.detail, &edges) {
                (Some(w), Some(e)) => b.max(w[i] * e[i]),
                _ => b,
            };
            for c in 0..3 {
                let v = d[c] * 255.0 * (1.0 - b) + px[c] as f32 * b;
                px[c] = (v + 0.5).clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Where the (denoised) RGB image has structure: 0 in smooth areas, 1 near edges, from the
/// largest luminance gradient around each pixel (0.01 .. 0.05 of full scale per pixel).
fn edge_mask(rgb: &[f32], width: usize, height: usize) -> Vec<f32> {
    let l: Vec<f32> = rgb.chunks_exact(3).map(|p| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]).collect();
    let at = |x: usize, y: usize| l[y * width + x];
    let mut g = vec![0f32; width * height];
    g.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        let (yu, yd) = (y.saturating_sub(1), (y + 1).min(height - 1));
        for (x, v) in row.iter_mut().enumerate() {
            let gx = at((x + 1).min(width - 1), y) - at(x.saturating_sub(1), y);
            let gy = at(x, yd) - at(x, yu);
            *v = (gx * gx + gy * gy).sqrt();
        }
    });
    let mut out = vec![0f32; width * height];
    out.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            let mut m = 0f32;
            for yy in y.saturating_sub(1)..=(y + 1).min(height - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                    m = m.max(g[yy * width + xx]);
                }
            }
            let t = ((m - 0.01) / 0.04).clamp(0.0, 1.0);
            *v = t * t * (3.0 - 2.0 * t);
        }
    });
    out
}

/// Share of the original image to keep per pixel, from its sample count `density`: noise
/// falls with the square root of the samples, so pixels with many samples are converged and
/// keep their detail, which the denoiser would soften. `strength` (0..1) lowers the count
/// from which pixels count as converged: from 2¹² samples at 0 to 8 at 1; the original fades
/// in over the four octaves below.
pub fn detail_weights(density: &[f32], strength: f64) -> Vec<f32> {
    let full = 12.0 - 9.0 * strength.clamp(0.0, 1.0) as f32;
    density
        .iter()
        .map(|&n| {
            let t = ((n.max(1.0).log2() - full + 4.0) / 4.0).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
        .collect()
}

/// Contrast adaptive sharpening (AMD FidelityFX CAS) of RGB values (0..1) in place: each
/// pixel is pushed away from its four neighbours, less where that would clip, so edges get
/// crisper with few halos. `amount` 0..1.
pub fn sharpen_cas(rgb: &mut [f32], width: usize, height: usize, amount: f32) {
    if width < 3 || height < 3 || rgb.len() != width * height * 3 {
        return;
    }
    let src = rgb.to_vec();
    let peak = -1.0 / (8.0 - 3.0 * amount.clamp(0.0, 1.0));
    let at = |x: usize, y: usize, c: usize| src[(y * width + x) * 3 + c];
    rgb.par_chunks_mut(width * 3).enumerate().for_each(|(y, row)| {
        let (yu, yd) = (y.saturating_sub(1), (y + 1).min(height - 1));
        for x in 0..width {
            let (xl, xr) = (x.saturating_sub(1), (x + 1).min(width - 1));
            for c in 0..3 {
                let (n, w, e, s, m) = (at(x, yu, c), at(xl, y, c), at(x, y, c), at(xr, y, c), at(x, yd, c));
                let lo = n.min(w).min(e).min(s).min(m);
                let hi = n.max(w).max(e).max(s).max(m);
                let amp = if hi > 0.0 { (lo.min(1.0 - hi) / hi).clamp(0.0, 1.0).sqrt() } else { 0.0 };
                let k = amp * peak;
                row[x * 3 + c] = ((n + w + s + m) * k + e) / (1.0 + 4.0 * k);
            }
        }
    });
}

/// Denoises an image file (JWildfire's "denoise external image"): PNG, JPEG, … from `src`,
/// written to `dst` as PNG or JPEG by its extension, with `denoiser` and `blend` as the post
/// denoiser of a flame. Tells what happened.
pub fn denoise_image_file(src: &Path, dst: &Path, denoiser: Denoiser, blend: f64) -> Result<DenoiseOutcome, String> {
    let img = image::open(src).map_err(|e| format!("{}: {e}", src.display()))?.to_rgba8();
    let (w, h) = img.dimensions();
    let mut rgba = img.into_raw();
    let settings = Flame { ai_post_denoiser: denoiser, post_denoiser_blend: blend, ..Flame::default() };
    let outcome = apply_post_denoiser(&settings, &mut rgba, w, h, false)?;
    let format = crate::output::ImageFormat::from_path(dst).filter(|f| !f.is_hdr()).ok_or_else(|| format!("{}: save as PNG or JPEG", dst.display()))?;
    crate::output::write_rgba8(dst, format, w, h, &rgba, 95)?;
    Ok(outcome)
}

/// What denoises images: the OIDN library, the `oidnDenoise` tool, or `Err` with a hint.
pub fn backend() -> Result<String, String> {
    match oidn::library() {
        Ok(p) => Ok(format!("Open Image Denoise ({})", p.display())),
        Err(e) => match find_tool() {
            Some(t) => Ok(format!("oidnDenoise tool ({})", t.display())),
            None => Err(format!("Open Image Denoise not found ({e}); run tools/fetch_oidn.py or set {}", oidn::DIR_ENV)),
        },
    }
}

/// The denoiser's input from `source` (default: `rgba`) as RGB 0..1.
fn input_rgb(rgba: &[u8], source: Option<&[u8]>) -> Vec<f32> {
    source.unwrap_or(rgba).chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).map(|c| c as f32 / 255.0).collect()
}

/// Denoises `source` (default: `rgba`) with the OIDN library into the RGBA8 image, keeping
/// alpha. Returns the device type used.
fn denoise_with_library(
    rgba: &mut [u8],
    source: Option<&[u8]>,
    width: u32,
    height: u32,
    post: &PostProcess,
    prefer: DeviceType,
    aux: Option<&DenoiseAux>,
) -> Result<DeviceType, String> {
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("image size mismatch".into());
    }
    let mut color = input_rgb(rgba, source);
    let albedo = aux.map(|a| a.albedo.as_slice()).filter(|a| !a.is_empty());
    let normal = aux.and_then(|a| a.normal.as_deref());
    let used = oidn::denoise(&mut color, width as usize, height as usize, false, true, prefer, albedo, normal)?;
    post.finish(rgba, &mut color, width, height);
    Ok(used)
}

/// Denoises the color channels of an RGBA8 image in place with the `oidnDenoise` tool,
/// keeping alpha. `blend` mixes the original back in (0 = fully denoised, 1 = original), as
/// the OptiX denoiser's blend factor.
pub fn denoise(tool: &Path, rgba: &mut [u8], width: u32, height: u32, blend: f64) -> Result<(), String> {
    denoise_with_tool(tool, rgba, None, width, height, &PostProcess { blend, sharpen: 0.0, detail: None })
}

/// [`denoise`] with the denoiser's input taken from `source` (default: `rgba`).
fn denoise_with_tool(tool: &Path, rgba: &mut [u8], source: Option<&[u8]>, width: u32, height: u32, post: &PostProcess) -> Result<(), String> {
    let n = width as usize * height as usize;
    if rgba.len() != n * 4 {
        return Err("image size mismatch".into());
    }
    let dir = std::env::temp_dir();
    let stem = format!("rwf_denoise_{}_{:x}", std::process::id(), rgba.as_ptr() as usize);
    let input = dir.join(format!("{stem}.pfm"));
    let output = dir.join(format!("{stem}_out.pfm"));
    let result = (|| {
        let color = input_rgb(rgba, source);
        write_pfm(&input, width, height, &color).map_err(|e| format!("writing {}: {e}", input.display()))?;
        let out = Command::new(tool)
            .arg("--ldr")
            .arg(&input)
            .arg("--srgb")
            .arg("-o")
            .arg(&output)
            .output()
            .map_err(|e| format!("running {}: {e}", tool.display()))?;
        if !out.status.success() {
            return Err(format!("{} failed: {}", tool.display(), String::from_utf8_lossy(&out.stderr).trim()));
        }
        let (w, h, mut denoised) = read_pfm(&output).map_err(|e| format!("reading {}: {e}", output.display()))?;
        if w != width || h != height {
            return Err("denoised image has a different size".into());
        }
        post.finish(rgba, &mut denoised, width, height);
        Ok(())
    })();
    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
    result
}

/// Writes a little-endian RGB PFM (rows bottom to top) from top-to-bottom RGB data.
fn write_pfm(path: &Path, w: u32, h: u32, rgb: &[f32]) -> std::io::Result<()> {
    let mut f = BufWriter::new(std::fs::File::create(path)?);
    write!(f, "PF\n{w} {h}\n-1.0\n")?;
    let row = w as usize * 3;
    for y in (0..h as usize).rev() {
        for v in &rgb[y * row..(y + 1) * row] {
            f.write_all(&v.to_le_bytes())?;
        }
    }
    f.flush()
}

/// Reads an RGB or grayscale PFM into top-to-bottom RGB data.
fn read_pfm(path: &Path) -> std::io::Result<(u32, u32, Vec<f32>)> {
    let mut data = Vec::new();
    std::fs::File::open(path)?.read_to_end(&mut data)?;
    let bad = |m: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, m.to_string());
    // Header: three whitespace separated tokens after the magic, then one whitespace byte.
    let mut tokens = Vec::new();
    let mut pos = 0;
    while tokens.len() < 4 {
        while pos < data.len() && data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let start = pos;
        while pos < data.len() && !data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if start == pos {
            return Err(bad("truncated header"));
        }
        tokens.push(String::from_utf8_lossy(&data[start..pos]).to_string());
    }
    pos += 1;
    let channels = match tokens[0].as_str() {
        "PF" => 3,
        "Pf" => 1,
        _ => return Err(bad("not a PFM file")),
    };
    let w: u32 = tokens[1].parse().map_err(|_| bad("bad width"))?;
    let h: u32 = tokens[2].parse().map_err(|_| bad("bad height"))?;
    let scale: f32 = tokens[3].parse().map_err(|_| bad("bad scale"))?;
    let n = w as usize * h as usize * channels;
    let body = data.get(pos..pos + n * 4).ok_or_else(|| bad("truncated data"))?;
    let vals: Vec<f32> = body
        .chunks_exact(4)
        .map(|b| {
            let b = [b[0], b[1], b[2], b[3]];
            if scale < 0.0 { f32::from_le_bytes(b) } else { f32::from_be_bytes(b) }
        })
        .collect();
    let row = w as usize * channels;
    let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
    for y in (0..h as usize).rev() {
        for v in vals[y * row..(y + 1) * row].chunks_exact(channels) {
            if channels == 3 {
                rgb.extend_from_slice(v);
            } else {
                rgb.extend_from_slice(&[v[0]; 3]);
            }
        }
    }
    Ok((w, h, rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_weights_grow_with_samples_and_strength() {
        let counts = [0.0, 4.0, 16.0, 64.0, 256.0, 1024.0, 8192.0];
        let w = detail_weights(&counts, 0.6);
        assert!(w.windows(2).all(|p| p[0] <= p[1]), "{w:?}");
        assert_eq!((w[0], w[6]), (0.0, 1.0));
        let weak = detail_weights(&counts, 0.2);
        assert!(weak.iter().zip(&w).all(|(a, b)| a <= b));
        assert!(detail_weights(&counts, 0.0).iter().take(5).all(|v| *v == 0.0));
    }

    #[test]
    fn cas_keeps_flat_areas_and_sharpens_edges() {
        let (w, h) = (8, 4);
        let mut flat = vec![0.4f32; w * h * 3];
        sharpen_cas(&mut flat, w, h, 1.0);
        assert!(flat.iter().all(|v| (v - 0.4).abs() < 1e-6));
        // A soft step from 0.3 to 0.7 gets steeper.
        let row = [0.3, 0.3, 0.3, 0.4, 0.6, 0.7, 0.7, 0.7];
        let mut img: Vec<f32> = (0..h).flat_map(|_| row.iter().flat_map(|v| [*v; 3])).collect();
        sharpen_cas(&mut img, w, h, 0.5);
        let px = |x: usize| img[(w + x) * 3];
        assert!(px(3) < 0.4 && px(4) > 0.6, "{} {}", px(3), px(4));
    }

    #[test]
    fn converged_edges_keep_the_original() {
        let (w, h) = (8u32, 1u32);
        // Original: a sharp line at x = 4; denoised: the line smeared out.
        let mut rgba: Vec<u8> = (0..8).flat_map(|x| if x == 4 { [200, 200, 200, 255] } else { [20, 20, 20, 255] }).collect();
        let smeared = [20.0, 20.0, 20.0, 80.0, 110.0, 80.0, 20.0, 20.0];
        let mut denoised: Vec<f32> = smeared.iter().flat_map(|v| [v / 255.0; 3]).collect();
        let density = vec![5000.0; 8];
        let post = PostProcess { blend: 0.0, sharpen: 0.0, detail: Some(detail_weights(&density, 0.6)) };
        post.finish(&mut rgba, &mut denoised, w, h);
        assert_eq!(rgba[16], 200);
        // Few samples: the denoised image.
        let mut rgba2: Vec<u8> = (0..8).flat_map(|x| if x == 4 { [200, 200, 200, 255] } else { [20, 20, 20, 255] }).collect();
        let mut denoised: Vec<f32> = smeared.iter().flat_map(|v| [v / 255.0; 3]).collect();
        let post = PostProcess { blend: 0.0, sharpen: 0.0, detail: Some(detail_weights(&[2.0; 8], 0.6)) };
        post.finish(&mut rgba2, &mut denoised, w, h);
        assert_eq!(rgba2[16], 110);
    }

    #[test]
    fn pfm_roundtrip() {
        let path = std::env::temp_dir().join(format!("rwf_pfm_test_{}.pfm", std::process::id()));
        let rgb: Vec<f32> = (0..3 * 4 * 3).map(|i| i as f32 / 10.0).collect();
        write_pfm(&path, 4, 3, &rgb).unwrap();
        let (w, h, back) = read_pfm(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!((w, h), (4, 3));
        assert_eq!(back, rgb);
    }
}
