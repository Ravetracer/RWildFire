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

/// Helper images for the denoiser, per pixel RGB floats: the albedo (0..1) and, for solid
/// flames, the surface normals (-1..1).
#[derive(Clone, Debug, Default)]
pub struct DenoiseAux {
    pub albedo: Vec<f32>,
    pub normal: Option<Vec<f32>>,
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
    if let Some(why) = skip_reason(flame, gpu_render) {
        return Ok(DenoiseOutcome::Skipped(why.into()));
    }
    let prefer = if flame.ai_post_denoiser == Denoiser::Optix { DeviceType::Cuda } else { DeviceType::Default };
    let library = oidn::library();
    if library.is_ok() {
        let used = denoise_with_library(rgba, width, height, flame.post_denoiser_blend, prefer, aux)?;
        let mut how = format!("OIDN on {}", used.label());
        match aux {
            Some(DenoiseAux { normal: Some(_), .. }) => how.push_str(", with albedo and normals"),
            Some(_) => how.push_str(", with albedo"),
            None => {}
        }
        if prefer == DeviceType::Cuda && used != DeviceType::Cuda {
            how.push_str(" (no usable NVIDIA GPU)");
        }
        return Ok(DenoiseOutcome::Denoised(how));
    }
    let Some(tool) = find_tool() else {
        let why = library.err().unwrap_or_default();
        return Ok(DenoiseOutcome::Skipped(format!("Open Image Denoise not found ({why})")));
    };
    denoise(&tool, rgba, width, height, flame.post_denoiser_blend)?;
    Ok(DenoiseOutcome::Denoised("the oidnDenoise tool".into()))
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

/// Denoises the color channels of an RGBA8 image in place with the OIDN library, keeping
/// alpha; `blend` as in [`denoise`]. Returns the device type used.
pub fn denoise_with_library(rgba: &mut [u8], width: u32, height: u32, blend: f64, prefer: DeviceType, aux: Option<&DenoiseAux>) -> Result<DeviceType, String> {
    let n = width as usize * height as usize;
    if rgba.len() != n * 4 {
        return Err("image size mismatch".into());
    }
    let mut color: Vec<f32> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).map(|c| c as f32 / 255.0).collect();
    let albedo = aux.map(|a| a.albedo.as_slice());
    let normal = aux.and_then(|a| a.normal.as_deref());
    let used = oidn::denoise(&mut color, width as usize, height as usize, false, true, prefer, albedo, normal)?;
    blend_back(rgba, &color, blend);
    Ok(used)
}

/// Writes the denoised RGB (0..1) into the RGBA8 image, mixing `blend` of the original back in.
fn blend_back(rgba: &mut [u8], denoised: &[f32], blend: f64) {
    let b = blend.clamp(0.0, 1.0) as f32;
    for (px, d) in rgba.chunks_exact_mut(4).zip(denoised.chunks_exact(3)) {
        for c in 0..3 {
            let v = d[c] * 255.0 * (1.0 - b) + px[c] as f32 * b;
            px[c] = (v + 0.5).clamp(0.0, 255.0) as u8;
        }
    }
}

/// Denoises the color channels of an RGBA8 image in place, keeping alpha. `blend` mixes the
/// original back in (0 = fully denoised, 1 = original), as the OptiX denoiser's blend factor.
pub fn denoise(tool: &Path, rgba: &mut [u8], width: u32, height: u32, blend: f64) -> Result<(), String> {
    let n = width as usize * height as usize;
    if rgba.len() != n * 4 {
        return Err("image size mismatch".into());
    }
    let dir = std::env::temp_dir();
    let stem = format!("rwf_denoise_{}_{:x}", std::process::id(), rgba.as_ptr() as usize);
    let input = dir.join(format!("{stem}.pfm"));
    let output = dir.join(format!("{stem}_out.pfm"));
    let result = (|| {
        let color: Vec<f32> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).map(|c| c as f32 / 255.0).collect();
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
        let (w, h, denoised) = read_pfm(&output).map_err(|e| format!("reading {}: {e}", output.display()))?;
        if w != width || h != height {
            return Err("denoised image has a different size".into());
        }
        blend_back(rgba, &denoised, blend);
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
