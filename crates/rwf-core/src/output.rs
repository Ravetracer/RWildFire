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

//! Image file output: PNG, JPEG, Radiance HDR, OpenEXR and 16-bit z-buffer PNGs.

use std::io::BufWriter;
use std::path::Path;

/// Output file format, chosen by file extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
    /// Radiance RGBE (`.hdr`), linear float colors.
    Hdr,
    /// OpenEXR (`.exr`), linear float colors.
    Exr,
}

impl ImageFormat {
    pub const ALL: &'static [ImageFormat] = &[ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::Hdr, ImageFormat::Exr];

    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => Some(ImageFormat::Png),
            "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
            "hdr" => Some(ImageFormat::Hdr),
            "exr" => Some(ImageFormat::Exr),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Hdr => "hdr",
            ImageFormat::Exr => "exr",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ImageFormat::Png => "PNG",
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Hdr => "HDR (Radiance)",
            ImageFormat::Exr => "OpenEXR",
        }
    }

    /// Float formats need the linear (unquantized) tonemapping.
    pub fn is_hdr(&self) -> bool {
        matches!(self, ImageFormat::Hdr | ImageFormat::Exr)
    }
}

fn io_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Writes an RGBA8 image as PNG or JPEG. JPEG has no alpha: transparent pixels are composited
/// onto black (like JWildfire's `saveAsJPEG`).
pub fn write_rgba8(path: &Path, format: ImageFormat, w: u32, h: u32, rgba: &[u8], jpeg_quality: u8) -> Result<(), String> {
    let file = BufWriter::new(std::fs::File::create(path).map_err(io_err)?);
    match format {
        ImageFormat::Png => {
            let mut enc = png::Encoder::new(file, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().map_err(io_err)?;
            writer.write_image_data(rgba).map_err(io_err)
        }
        ImageFormat::Jpeg => {
            let rgb: Vec<u8> =
                rgba.as_chunks::<4>().0.iter().flat_map(|p| [0, 1, 2].map(|k| ((p[k] as u32 * p[3] as u32 + 127) / 255) as u8)).collect();
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(file, jpeg_quality.clamp(1, 100));
            image::ImageEncoder::write_image(enc, &rgb, w, h, image::ExtendedColorType::Rgb8).map_err(io_err)
        }
        _ => Err(format!("{} needs a float image", format.label())),
    }
}

/// Writes linear float RGB (`w * h * 3` values) as Radiance HDR or OpenEXR.
pub fn write_float_rgb(path: &Path, format: ImageFormat, w: u32, h: u32, rgb: &[f32]) -> Result<(), String> {
    let file = BufWriter::new(std::fs::File::create(path).map_err(io_err)?);
    match format {
        ImageFormat::Hdr => {
            let px: Vec<image::Rgb<f32>> = rgb.as_chunks::<3>().0.iter().map(|p| image::Rgb(*p)).collect();
            image::codecs::hdr::HdrEncoder::new(file).encode(&px, w as usize, h as usize).map_err(io_err)
        }
        ImageFormat::Exr => {
            let bytes: Vec<u8> = rgb.iter().flat_map(|v| v.to_le_bytes()).collect();
            let enc = image::codecs::openexr::OpenExrEncoder::new(file);
            image::ImageEncoder::write_image(enc, &bytes, w, h, image::ExtendedColorType::Rgb32F).map_err(io_err)
        }
        _ => Err(format!("{} is not a float format", format.label())),
    }
}

/// Writes a 16-bit grayscale PNG (z-buffer).
pub fn write_gray16_png(path: &Path, w: u32, h: u32, gray: &[u16]) -> Result<(), String> {
    let file = BufWriter::new(std::fs::File::create(path).map_err(io_err)?);
    let mut enc = png::Encoder::new(file, w, h);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Sixteen);
    let mut writer = enc.write_header().map_err(io_err)?;
    let bytes: Vec<u8> = gray.iter().flat_map(|v| v.to_be_bytes()).collect();
    writer.write_image_data(&bytes).map_err(io_err)
}
