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

//! Output of rendered animation frames: PNG sequences or video files via ffmpeg.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

/// Writes an RGBA8 image as PNG.
pub fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    writer.write_image_data(rgba).map_err(std::io::Error::other)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoFormat {
    /// Numbered PNG files in a folder.
    PngSequence,
    /// H.264 MP4 (most compatible).
    Mp4,
    /// ProRes 422 HQ in MOV (editing).
    ProRes,
    /// VP9 WebM.
    WebM,
}

impl VideoFormat {
    pub const ALL: &'static [VideoFormat] = &[VideoFormat::Mp4, VideoFormat::ProRes, VideoFormat::WebM, VideoFormat::PngSequence];

    pub fn label(&self) -> &'static str {
        match self {
            VideoFormat::PngSequence => "PNG sequence",
            VideoFormat::Mp4 => "MP4 (H.264)",
            VideoFormat::ProRes => "MOV (ProRes)",
            VideoFormat::WebM => "WebM (VP9)",
        }
    }

    pub fn extension(&self) -> Option<&'static str> {
        match self {
            VideoFormat::PngSequence => None,
            VideoFormat::Mp4 => Some("mp4"),
            VideoFormat::ProRes => Some("mov"),
            VideoFormat::WebM => Some("webm"),
        }
    }

    pub fn from_path(p: &Path) -> Self {
        match p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
            Some("mp4") | Some("m4v") => VideoFormat::Mp4,
            Some("mov") => VideoFormat::ProRes,
            Some("webm") => VideoFormat::WebM,
            _ => VideoFormat::PngSequence,
        }
    }
}

/// True if an `ffmpeg` executable is available.
pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

pub enum VideoSink {
    Png { dir: PathBuf, w: u32, h: u32 },
    Ffmpeg { child: Child, stdin: Option<ChildStdin>, w: u32, h: u32 },
}

impl VideoSink {
    pub fn new(path: &Path, format: VideoFormat, w: u32, h: u32, fps: f64) -> std::io::Result<Self> {
        Self::with_audio(path, format, w, h, fps, None)
    }

    /// Like [`Self::new`], with the sound file `audio.0` from `audio.1` seconds on as the
    /// audio track (ignored for PNG sequences); the video ends with the shorter stream.
    pub fn with_audio(path: &Path, format: VideoFormat, w: u32, h: u32, fps: f64, audio: Option<(&Path, f64)>) -> std::io::Result<Self> {
        if format == VideoFormat::PngSequence {
            std::fs::create_dir_all(path)?;
            return Ok(VideoSink::Png { dir: path.to_path_buf(), w, h });
        }
        let codec: &[&str] = match format {
            VideoFormat::Mp4 => &["-c:v", "libx264", "-preset", "slow", "-crf", "15", "-pix_fmt", "yuv420p"],
            VideoFormat::ProRes => &["-c:v", "prores_ks", "-profile:v", "3", "-pix_fmt", "yuv422p10le"],
            VideoFormat::WebM => &["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "20", "-pix_fmt", "yuv420p"],
            VideoFormat::PngSequence => unreachable!(),
        };
        // H.264/VP9 with yuv420p need even dimensions.
        let size = format!("{w}x{h}");
        let mut cmd = Command::new("ffmpeg");
        cmd.args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba", "-s", &size, "-r", &format!("{fps}"), "-i", "-"]);
        if let Some((sound, start)) = audio {
            cmd.args(["-ss", &format!("{:.3}", start.max(0.0))]).arg("-i").arg(sound);
            let acodec = match format {
                VideoFormat::WebM => "libopus",
                VideoFormat::ProRes => "pcm_s16le",
                _ => "aac",
            };
            cmd.args(["-map", "0:v", "-map", "1:a", "-c:a", acodec, "-shortest"]);
        }
        cmd.args(["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"]);
        cmd.args(codec);
        cmd.arg(path);
        cmd.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| std::io::Error::other(format!("could not start ffmpeg: {e}")))?;
        let stdin = child.stdin.take();
        Ok(VideoSink::Ffmpeg { child, stdin, w, h })
    }

    pub fn write_frame(&mut self, index: u32, rgba: &[u8]) -> std::io::Result<()> {
        match self {
            VideoSink::Png { dir, w, h } => write_png(&dir.join(format!("frame_{index:05}.png")), *w, *h, rgba),
            VideoSink::Ffmpeg { stdin, w, h, .. } => {
                debug_assert_eq!(rgba.len(), (*w * *h * 4) as usize);
                stdin.as_mut().ok_or_else(|| std::io::Error::other("ffmpeg closed"))?.write_all(rgba)
            }
        }
    }

    pub fn finish(self) -> std::io::Result<()> {
        match self {
            VideoSink::Png { .. } => Ok(()),
            VideoSink::Ffmpeg { child, stdin, .. } => {
                drop(stdin);
                let out = child.wait_with_output()?;
                if out.status.success() {
                    Ok(())
                } else {
                    Err(std::io::Error::other(format!("ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr).trim())))
                }
            }
        }
    }
}
