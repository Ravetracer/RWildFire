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

//! Audio envelopes: the loudness of a frequency band per animation frame, for motion curves
//! that follow music (JWildfire's "MP3 import" of the envelope editor).

use std::path::Path;

use crate::animation::{Interp, Key, Track};

#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    #[error("cannot read {0}: {1}")]
    Io(String, std::io::Error),
    #[error("cannot decode {0}: {1}")]
    Decode(String, String),
    #[error("{0} has no audio track")]
    NoTrack(String),
}

/// Decoded audio, mixed down to one channel.
#[derive(Clone, Debug)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub rate: u32,
}

/// Decoded audio with all channels (interleaved), for playback.
#[derive(Clone, Debug)]
pub struct Pcm {
    pub samples: Vec<f32>,
    pub channels: usize,
    pub rate: u32,
}

impl Pcm {
    /// Mixes the channels down to one.
    pub fn mono(&self) -> Audio {
        let c = self.channels.max(1);
        Audio { samples: self.samples.chunks(c).map(|f| f.iter().sum::<f32>() / c as f32).collect(), rate: self.rate }
    }

    /// Length in seconds.
    pub fn duration(&self) -> f64 {
        self.samples.len() as f64 / self.channels.max(1) as f64 / self.rate.max(1) as f64
    }
}

/// What an envelope measures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Band {
    /// Overall loudness (RMS).
    Level,
    /// Spectral energy between two frequencies (Hz).
    Range(f32, f32),
}

/// Named frequency bands.
pub const BANDS: &[(&str, Band)] = &[
    ("Level (all frequencies)", Band::Level),
    ("Bass (20–250 Hz)", Band::Range(20.0, 250.0)),
    ("Low mids (250–500 Hz)", Band::Range(250.0, 500.0)),
    ("Mids (500–2000 Hz)", Band::Range(500.0, 2000.0)),
    ("High mids (2–6 kHz)", Band::Range(2000.0, 6000.0)),
    ("Treble (6–20 kHz)", Band::Range(6000.0, 20000.0)),
];

/// FFT size; about 46 ms at 44.1 kHz.
const FFT_SIZE: usize = 2048;

impl Audio {
    /// Decodes an audio file (MP3, WAV, FLAC, Ogg Vorbis).
    pub fn load(path: impl AsRef<Path>) -> Result<Audio, AudioError> {
        Ok(Pcm::load(path)?.mono())
    }
}

impl Pcm {
    /// Decodes an audio file (MP3, WAV, FLAC, Ogg Vorbis).
    pub fn load(path: impl AsRef<Path>) -> Result<Pcm, AudioError> {
        use symphonia::core::audio::SampleBuffer;
        use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
        use symphonia::core::errors::Error;
        use symphonia::core::formats::FormatOptions;
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;
        use symphonia::core::probe::Hint;

        let path = path.as_ref();
        let name = path.display().to_string();
        let file = std::fs::File::open(path).map_err(|e| AudioError::Io(name.clone(), e))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let decode_err = |e: Error| AudioError::Decode(name.clone(), e.to_string());
        let probed =
            symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).map_err(decode_err)?;
        let mut format = probed.format;
        let track = format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or_else(|| AudioError::NoTrack(name.clone()))?;
        let track_id = track.id;
        let mut rate = track.codec_params.sample_rate.unwrap_or(44_100);
        let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(decode_err)?;
        let mut samples = Vec::new();
        let mut channels = 0usize;
        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(Error::ResetRequired) => break,
                Err(e) => return Err(decode_err(e)),
            };
            if packet.track_id() != track_id {
                continue;
            }
            match decoder.decode(&packet) {
                Ok(buf) => {
                    let spec = *buf.spec();
                    rate = spec.rate;
                    let c = spec.channels.count().max(1);
                    let mut sb = SampleBuffer::<f32>::new(buf.capacity() as u64, spec);
                    sb.copy_interleaved_ref(buf);
                    if channels == 0 {
                        channels = c;
                    }
                    if c == channels {
                        samples.extend_from_slice(sb.samples());
                    } else {
                        // Channel layout changed mid-stream: mix down / spread to the first layout.
                        for f in sb.samples().chunks(c) {
                            let m = f.iter().sum::<f32>() / c as f32;
                            samples.extend(std::iter::repeat_n(m, channels));
                        }
                    }
                }
                // Skip damaged frames, as players do.
                Err(Error::DecodeError(_)) => continue,
                Err(e) => return Err(decode_err(e)),
            }
        }
        Ok(Pcm { samples, channels: channels.max(1), rate })
    }
}

impl Audio {
    /// Length in seconds.
    pub fn duration(&self) -> f64 {
        self.samples.len() as f64 / self.rate.max(1) as f64
    }

    /// The raw loudness of `band` for `frames` animation frames at `fps`, starting at
    /// `start` seconds into the audio (silence beyond its end).
    pub fn envelope(&self, band: Band, fps: f64, start: f64, frames: usize) -> Vec<f64> {
        let rate = self.rate.max(1) as f64;
        let window = hann(FFT_SIZE);
        let mut buf = vec![(0.0f32, 0.0f32); FFT_SIZE];
        (0..frames)
            .map(|f| {
                let centre = ((start + f as f64 / fps.max(1e-6)) * rate) as i64;
                let first = centre - FFT_SIZE as i64 / 2;
                let sample = |i: usize| {
                    let j = first + i as i64;
                    if j >= 0 { self.samples.get(j as usize).copied().unwrap_or(0.0) } else { 0.0 }
                };
                match band {
                    Band::Level => ((0..FFT_SIZE).map(|i| sample(i).powi(2) as f64).sum::<f64>() / FFT_SIZE as f64).sqrt(),
                    Band::Range(lo, hi) => {
                        for (i, b) in buf.iter_mut().enumerate() {
                            *b = (sample(i) * window[i], 0.0);
                        }
                        fft(&mut buf);
                        let bin = |hz: f32| ((hz as f64 / rate * FFT_SIZE as f64).round() as usize).clamp(1, FFT_SIZE / 2);
                        let (a, b) = (bin(lo), bin(hi).max(bin(lo) + 1));
                        let energy: f64 = buf[a..b].iter().map(|(re, im)| (re * re + im * im) as f64).sum();
                        (energy / (b - a) as f64).sqrt() / FFT_SIZE as f64
                    }
                }
            })
            .collect()
    }
}

pub(crate) fn hann(n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / n as f32).cos()).collect()
}

/// In-place iterative radix-2 FFT (`buf.len()` must be a power of two).
pub(crate) fn fft(buf: &mut [(f32, f32)]) {
    let n = buf.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            buf.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        let (ws, wc) = ang.sin_cos();
        for start in (0..n).step_by(len) {
            let (mut wr, mut wi) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (ar, ai) = buf[start + k];
                let (br, bi) = buf[start + k + len / 2];
                let (tr, ti) = (br * wr - bi * wi, br * wi + bi * wr);
                buf[start + k] = (ar + tr, ai + ti);
                buf[start + k + len / 2] = (ar - tr, ai - ti);
                (wr, wi) = (wr * wc - wi * ws, wr * ws + wi * wc);
            }
        }
        len <<= 1;
    }
}

/// Shaping of a raw envelope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shaping {
    /// Release time in frames: the curve rises with the music at once and falls back slowly
    /// (0 = follows it exactly).
    pub release: f64,
    /// Values below this fraction of the loudest frame count as silence (0..1).
    pub threshold: f64,
    /// Output range: silence maps to `min`, the loudest frame to `max`.
    pub min: f64,
    pub max: f64,
}

impl Default for Shaping {
    fn default() -> Self {
        Self { release: 3.0, threshold: 0.0, min: 0.0, max: 1.0 }
    }
}

/// Normalizes `raw` to 0..1 (by its loudest frame), applies threshold and release and maps it
/// to `min..max`.
pub fn shape(raw: &[f64], s: &Shaping) -> Vec<f64> {
    let peak = raw.iter().copied().fold(0.0, f64::max);
    let t = s.threshold.clamp(0.0, 0.99);
    let fall = if s.release > 0.0 { (-1.0 / s.release).exp() } else { 0.0 };
    let mut y = 0.0;
    raw.iter()
        .map(|&v| {
            let n = if peak > 0.0 { v / peak } else { 0.0 };
            let n = ((n - t) / (1.0 - t)).max(0.0);
            y = if n >= y { n } else { n + (y - n) * fall };
            s.min + y * (s.max - s.min)
        })
        .collect()
}

/// Replaces the keys of `track` by one key per frame from `first_frame` on (linear
/// interpolation).
pub fn apply_to_track(track: &mut Track, values: &[f64], first_frame: f64) {
    track.interp = Interp::Linear;
    track.keys = values.iter().enumerate().map(|(i, &value)| Key { frame: first_frame + i as f64, value }).collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, rate: u32, secs: f32) -> Vec<f32> {
        (0..(rate as f32 * secs) as usize).map(|i| (std::f32::consts::TAU * hz * i as f32 / rate as f32).sin() * 0.5).collect()
    }

    #[test]
    fn bands_follow_the_spectrum() {
        // One second of 100 Hz, then one second of 8 kHz.
        let rate = 44_100;
        let mut samples = tone(100.0, rate, 1.0);
        samples.extend(tone(8000.0, rate, 1.0));
        let audio = Audio { samples, rate };
        let bass = audio.envelope(Band::Range(20.0, 250.0), 10.0, 0.0, 20);
        let treble = audio.envelope(Band::Range(6000.0, 20000.0), 10.0, 0.0, 20);
        let level = audio.envelope(Band::Level, 10.0, 0.0, 20);
        assert!(bass[3] > 20.0 * bass[15] && treble[15] > 20.0 * treble[3], "bass {bass:?}\ntreble {treble:?}");
        assert!((level[3] - level[15]).abs() < 0.05 * level[3], "{level:?}");
    }

    #[test]
    fn fft_finds_a_sine() {
        let mut buf: Vec<(f32, f32)> = (0..64).map(|i| ((std::f32::consts::TAU * 5.0 * i as f32 / 64.0).cos(), 0.0)).collect();
        fft(&mut buf);
        let mags: Vec<f32> = buf.iter().map(|(r, i)| (r * r + i * i).sqrt()).collect();
        let peak = (0..32).max_by(|a, b| mags[*a].total_cmp(&mags[*b])).unwrap();
        assert_eq!(peak, 5);
        assert!((mags[5] - 32.0).abs() < 1e-3);
    }

    #[test]
    fn shaping_normalizes_and_releases() {
        let s = Shaping { release: 2.0, threshold: 0.0, min: 1.0, max: 3.0 };
        let v = shape(&[0.0, 2.0, 0.0, 0.0, 1.0], &s);
        assert_eq!(v[0], 1.0);
        assert_eq!(v[1], 3.0);
        assert!(v[2] < 3.0 && v[2] > v[3] && v[3] > 1.0, "{v:?}");
        assert!((v[4] - 2.0).abs() < 1e-9 || v[4] > 2.0);
    }

    #[test]
    fn decodes_wav() {
        // 16-bit stereo PCM WAV with a 440 Hz tone on both channels.
        let rate = 22_050u32;
        let pcm: Vec<i16> = tone(440.0, rate, 0.5).iter().flat_map(|s| [(*s * 32767.0) as i16; 2]).collect();
        let mut wav = Vec::new();
        let data_len = (pcm.len() * 2) as u32;
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&rate.to_le_bytes());
        wav.extend_from_slice(&(rate * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for s in &pcm {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        let path = std::env::temp_dir().join(format!("rwf-audio-test-{}.wav", std::process::id()));
        std::fs::write(&path, &wav).unwrap();
        let audio = Audio::load(&path);
        std::fs::remove_file(&path).ok();
        let audio = audio.unwrap();
        assert_eq!(audio.rate, rate);
        assert_eq!(audio.samples.len(), pcm.len() / 2);
        assert!((audio.duration() - 0.5).abs() < 0.01);
        let peak = audio.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.01, "{peak}");
    }

    #[test]
    fn writes_one_key_per_frame() {
        let mut t = Track { path: "brightness".into(), interp: Interp::Smooth, keys: Vec::new() };
        apply_to_track(&mut t, &[1.0, 2.0, 3.0], 10.0);
        assert_eq!(t.interp, Interp::Linear);
        assert_eq!(t.keys.len(), 3);
        assert_eq!(t.eval(11.5), Some(2.5));
    }
}
