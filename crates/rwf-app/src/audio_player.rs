// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
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

//! Music playback for dancing flames. The playback position is the clock of the show, so the
//! flames stay in sync with what is heard. Without an audio device a wall clock stands in.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rwf_core::audio::Pcm;

struct Shared {
    /// Interleaved samples at the device rate and channel count.
    samples: Vec<f32>,
    channels: usize,
    /// Next frame to play.
    pos: AtomicUsize,
    playing: AtomicBool,
    muted: AtomicBool,
}

impl Shared {
    fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }

    /// Fills an output buffer and advances the position.
    fn fill<T: cpal::SizedSample + cpal::FromSample<f32>>(&self, out: &mut [T]) {
        let c = self.channels;
        let silence = T::from_sample(0.0f32);
        if !self.playing.load(Ordering::Relaxed) {
            out.fill(silence);
            return;
        }
        let muted = self.muted.load(Ordering::Relaxed);
        let start = self.pos.load(Ordering::Relaxed);
        let frames = out.len() / c;
        for (k, frame) in out.chunks_mut(c).enumerate() {
            let src = (start + k) * c;
            for (ch, s) in frame.iter_mut().enumerate() {
                *s = match self.samples.get(src + ch) {
                    Some(v) if !muted => T::from_sample(*v),
                    _ => silence,
                };
            }
        }
        self.pos.store((start + frames).min(self.frames()), Ordering::Relaxed);
    }
}

/// Wall clock used without an audio device.
#[derive(Default)]
struct Clock {
    at: f64,
    since: Option<Instant>,
}

impl Clock {
    fn now(&self) -> f64 {
        self.at + self.since.map_or(0.0, |s| s.elapsed().as_secs_f64())
    }
}

pub struct Player {
    output: Option<(cpal::Stream, u32)>,
    shared: Arc<Shared>,
    clock: Clock,
    duration: f64,
    /// Why there is no audio output (if so).
    pub device_error: Option<String>,
}

/// Linear resampling to `rate` and mapping to `channels`.
fn convert(pcm: &Pcm, rate: u32, channels: usize) -> Vec<f32> {
    let pc = pcm.channels.max(1);
    let in_frames = pcm.samples.len() / pc;
    let out_frames = (in_frames as f64 * rate as f64 / pcm.rate.max(1) as f64) as usize;
    let step = pcm.rate as f64 / rate.max(1) as f64;
    let mut out = Vec::with_capacity(out_frames * channels);
    for f in 0..out_frames {
        let x = f as f64 * step;
        let i = x as usize;
        let t = (x - i as f64) as f32;
        for c in 0..channels {
            let sc = c % pc;
            let a = pcm.samples.get(i * pc + sc).copied().unwrap_or(0.0);
            let b = pcm.samples.get((i + 1) * pc + sc).copied().unwrap_or(a);
            out.push(a + (b - a) * t);
        }
    }
    out
}

fn open_output(pcm: &Pcm) -> Result<(cpal::Stream, u32, Arc<Shared>), String> {
    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no audio output device")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let channels = config.channels.max(1) as usize;
    let rate = config.sample_rate.0;
    let shared = Arc::new(Shared {
        samples: convert(pcm, rate, channels),
        channels,
        pos: AtomicUsize::new(0),
        playing: AtomicBool::new(false),
        muted: AtomicBool::new(false),
    });
    let err = |e: cpal::StreamError| eprintln!("audio output: {e}");
    let s = shared.clone();
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_output_stream(&config, move |d: &mut [f32], _: &_| s.fill(d), err, None),
        cpal::SampleFormat::I16 => device.build_output_stream(&config, move |d: &mut [i16], _: &_| s.fill(d), err, None),
        cpal::SampleFormat::U16 => device.build_output_stream(&config, move |d: &mut [u16], _: &_| s.fill(d), err, None),
        cpal::SampleFormat::I32 => device.build_output_stream(&config, move |d: &mut [i32], _: &_| s.fill(d), err, None),
        f => return Err(format!("unsupported sample format {f}")),
    }
    .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rate, shared))
}

impl Player {
    pub fn new(pcm: &Pcm) -> Self {
        let duration = pcm.duration();
        match open_output(pcm) {
            Ok((stream, rate, shared)) => Player { output: Some((stream, rate)), shared, clock: Clock::default(), duration, device_error: None },
            Err(e) => Player {
                output: None,
                shared: Arc::new(Shared {
                    samples: Vec::new(),
                    channels: 1,
                    pos: AtomicUsize::new(0),
                    playing: AtomicBool::new(false),
                    muted: AtomicBool::new(false),
                }),
                clock: Clock::default(),
                duration,
                device_error: Some(e),
            },
        }
    }

    pub fn duration(&self) -> f64 {
        self.duration
    }

    /// Playback position in seconds.
    pub fn position(&self) -> f64 {
        match &self.output {
            Some((_, rate)) => self.shared.pos.load(Ordering::Relaxed) as f64 / *rate as f64,
            None => self.clock.now().min(self.duration),
        }
    }

    pub fn is_playing(&self) -> bool {
        match &self.output {
            Some(_) => self.shared.playing.load(Ordering::Relaxed),
            None => self.clock.since.is_some(),
        }
    }

    pub fn at_end(&self) -> bool {
        self.position() >= self.duration - 1e-3
    }

    pub fn play(&mut self) {
        if self.at_end() {
            self.seek(0.0);
        }
        self.shared.playing.store(true, Ordering::Relaxed);
        if self.clock.since.is_none() {
            self.clock.since = Some(Instant::now());
        }
    }

    pub fn pause(&mut self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        self.clock.at = self.clock.now();
        self.clock.since = None;
    }

    pub fn seek(&mut self, t: f64) {
        let t = t.clamp(0.0, self.duration);
        if let Some((_, rate)) = &self.output {
            self.shared.pos.store((t * *rate as f64) as usize, Ordering::Relaxed);
        }
        self.clock.at = t;
        if self.clock.since.is_some() {
            self.clock.since = Some(Instant::now());
        }
    }

    pub fn set_muted(&self, muted: bool) {
        self.shared.muted.store(muted, Ordering::Relaxed);
    }
}

/// A clock for shows without music: plays, pauses and seeks like a [`Player`].
#[derive(Default)]
pub struct FreeClock(Clock);

impl FreeClock {
    pub fn position(&self) -> f64 {
        self.0.now()
    }

    pub fn is_playing(&self) -> bool {
        self.0.since.is_some()
    }

    pub fn play(&mut self) {
        if self.0.since.is_none() {
            self.0.since = Some(Instant::now());
        }
    }

    pub fn pause(&mut self) {
        self.0.at = self.0.now();
        self.0.since = None;
    }

    pub fn seek(&mut self, t: f64) {
        self.0.at = t.max(0.0);
        if self.0.since.is_some() {
            self.0.since = Some(Instant::now());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_resamples_and_maps_channels() {
        let pcm = Pcm { samples: vec![0.0, 1.0, 2.0, 3.0], channels: 1, rate: 100 };
        let out = convert(&pcm, 200, 2);
        assert_eq!(out.len(), 16);
        assert_eq!(&out[..6], &[0.0, 0.0, 0.5, 0.5, 1.0, 1.0]);
    }
}
