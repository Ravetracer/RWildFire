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

//! Dancing flames: a pool of flames moved by music in real time (after JWildfire's
//! `org.jwildfire.create.tina.dance`).
//!
//! Motions (an FFT band of the music, a sine or a saw tooth) produce a value per moment that is
//! *added* to linked properties of the pool flames: any animatable property, or a relative
//! transform of an xform (rotate, move, zoom; as JWildfire's virtual properties). Switching
//! between flames while playing is recorded as a [`Performance`] that can be rendered offline.
//! JWildfire's `.jwfdance` files are Java object streams and cannot be read.

use std::path::{Path, PathBuf};

use crate::animation::{Interp, Key, Track, get_prop, set_prop, xform_at_mut};
use crate::audio::{Audio, fft, hann};
use crate::io::xml::{self, Element};
use crate::model::{Flame, XFormKind};
use crate::movie::{MorphType, morph_flames};
use crate::random::java;

// ---- spectrum -------------------------------------------------------------------------------

/// Number of frequency bands of a [`Spectrum`].
pub const BANDS: usize = 32;
/// Lowest and highest band edge in Hz (bands are spaced logarithmically).
const LOW_HZ: f64 = 30.0;
const HIGH_HZ: f64 = 16_000.0;
/// Time between two spectrum rows in seconds.
const ROW_STEP: f64 = 0.01;
const FFT_SIZE: usize = 2048;

/// Lower edge of band `b` in Hz.
pub fn band_low_hz(b: usize) -> f64 {
    LOW_HZ * (HIGH_HZ / LOW_HZ).powf(b as f64 / BANDS as f64)
}

/// Short label of band `b`, e.g. `"5 · 120 Hz"`.
pub fn band_label(b: usize) -> String {
    let hz = band_low_hz(b);
    if hz >= 1000.0 { format!("{b} · {:.1} kHz", hz / 1000.0) } else { format!("{b} · {hz:.0} Hz") }
}

/// The loudness of [`BANDS`] frequency bands of a piece of music over time, each band scaled to
/// about 0..1 over the whole piece.
#[derive(Clone, Debug)]
pub struct Spectrum {
    rows: Vec<[f32; BANDS]>,
}

impl Spectrum {
    /// Analyses the audio (uses all cores).
    pub fn analyse(audio: &Audio) -> Spectrum {
        let rate = audio.rate.max(1) as f64;
        let n = (audio.duration() / ROW_STEP).ceil() as usize + 1;
        let window = hann(FFT_SIZE);
        let bins: Vec<usize> = (0..=BANDS).map(|b| ((band_low_hz(b) / rate * FFT_SIZE as f64).round() as usize).clamp(1, FFT_SIZE / 2)).collect();
        let threads = std::thread::available_parallelism().map(|t| t.get()).unwrap_or(4).min(16);
        let chunk = n.div_ceil(threads).max(1);
        let mut rows = vec![[0f32; BANDS]; n];
        std::thread::scope(|s| {
            for (ci, part) in rows.chunks_mut(chunk).enumerate() {
                let (window, bins) = (&window, &bins);
                s.spawn(move || {
                    let mut buf = vec![(0.0f32, 0.0f32); FFT_SIZE];
                    for (k, row) in part.iter_mut().enumerate() {
                        let r = ci * chunk + k;
                        let first = (r as f64 * ROW_STEP * rate) as i64 - FFT_SIZE as i64 / 2;
                        for (i, b) in buf.iter_mut().enumerate() {
                            let j = first + i as i64;
                            let v = if j >= 0 { audio.samples.get(j as usize).copied().unwrap_or(0.0) } else { 0.0 };
                            *b = (v * window[i], 0.0);
                        }
                        fft(&mut buf);
                        for (b, out) in row.iter_mut().enumerate() {
                            let (lo, hi) = (bins[b], bins[b + 1].max(bins[b] + 1));
                            let e: f32 = buf[lo..hi].iter().map(|(re, im)| re * re + im * im).sum::<f32>() / (hi - lo) as f32;
                            // Decibels; scaled per band below.
                            *out = 10.0 * (e + 1e-12).log10();
                        }
                    }
                });
            }
        });
        // Per band: map the 10th..99.5th percentile of the levels to 0..1, at most 60 dB below
        // the peak (nearly silent bands stay quiet instead of being stretched).
        for b in 0..BANDS {
            let mut v: Vec<f32> = rows.iter().map(|r| r[b]).collect();
            v.sort_by(f32::total_cmp);
            let at = |q: f64| v[((v.len() - 1) as f64 * q) as usize];
            let hi = at(0.995);
            let lo = at(0.10).max(hi - 60.0);
            let span = (hi - lo).max(20.0);
            for r in &mut rows {
                r[b] = ((r[b] - lo) / span).clamp(0.0, 1.0);
            }
        }
        Spectrum { rows }
    }

    /// Length in seconds.
    pub fn duration(&self) -> f64 {
        self.rows.len().saturating_sub(1) as f64 * ROW_STEP
    }

    /// The level of `band` at `t` seconds (0 outside the music).
    pub fn level(&self, band: usize, t: f64) -> f64 {
        let x = t / ROW_STEP;
        if x < 0.0 || band >= BANDS {
            return 0.0;
        }
        let i = x as usize;
        let (Some(a), Some(b)) = (self.rows.get(i), self.rows.get(i + 1)) else { return 0.0 };
        let f = (x - i as f64) as f32;
        (a[band] + (b[band] - a[band]) * f) as f64
    }

    /// All band levels at `t` (for display).
    pub fn levels(&self, t: f64) -> [f32; BANDS] {
        std::array::from_fn(|b| self.level(b, t) as f32)
    }
}

// ---- motions --------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MotionKind {
    /// `offset + amplitude * level` of a spectrum band; the level rises at once and falls
    /// back within `release` seconds.
    Fft { band: usize, amplitude: f64, offset: f64, release: f64 },
    /// Swings between `min` and `max`, `rpm` times per minute.
    Sine { min: f64, max: f64, rpm: f64 },
    /// Runs from `min` to `max`, `rpm` times per minute.
    SawTooth { min: f64, max: f64, rpm: f64 },
    /// `offset + amplitude * curve(t)` of the motion's curve (JWildfire's spline motion).
    Spline { amplitude: f64, offset: f64 },
}

impl MotionKind {
    pub const NAMES: [&str; 4] = ["Music (FFT band)", "Sine", "Saw tooth", "Spline"];

    pub fn index(&self) -> usize {
        match self {
            MotionKind::Fft { .. } => 0,
            MotionKind::Sine { .. } => 1,
            MotionKind::SawTooth { .. } => 2,
            MotionKind::Spline { .. } => 3,
        }
    }

    pub fn default_of(index: usize) -> MotionKind {
        match index {
            1 => MotionKind::Sine { min: -1.0, max: 1.0, rpm: 10.0 },
            2 => MotionKind::SawTooth { min: 0.0, max: 1.0, rpm: 10.0 },
            3 => MotionKind::Spline { amplitude: 1.0, offset: 0.0 },
            _ => MotionKind::Fft { band: 1, amplitude: 1.0, offset: 0.0, release: 0.15 },
        }
    }
}

/// A motion and the properties it moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Motion {
    pub name: String,
    pub kind: MotionKind,
    /// Active from/until this many seconds into the music (`end` 0 = to the end).
    pub start: f64,
    pub end: f64,
    pub links: Vec<Link>,
    /// The curve of a spline motion: keys at seconds into the music (`frame` = seconds).
    pub curve: Track,
}

/// A property of a pool flame moved by a motion: an animation property path (see
/// [`crate::animation::get_prop`]) or a relative xform transform `<xform>.@<op>` (see [`XFORM_OPS`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub flame: usize,
    pub target: String,
}

/// Relative xform transforms a motion can drive, with labels: the value is an angle in
/// degrees, a distance, or a relative zoom (`1 + value`).
pub const XFORM_OPS: [(&str, &str); 8] = [
    ("angle", "rotate"),
    ("move_x", "move x"),
    ("move_y", "move y"),
    ("zoom", "zoom"),
    ("post_angle", "post rotate"),
    ("post_move_x", "post move x"),
    ("post_move_y", "post move y"),
    ("post_zoom", "post zoom"),
];

impl Motion {
    pub fn new(name: impl Into<String>, kind: MotionKind) -> Self {
        Motion { name: name.into(), kind, start: 0.0, end: 0.0, links: Vec::new(), curve: default_curve() }
    }

    fn active(&self, t: f64) -> bool {
        t >= self.start && (self.end <= 0.0 || t <= self.end)
    }

    /// The value at `t` seconds (`spectrum` for FFT motions; 0 without music).
    pub fn value(&self, spectrum: Option<&Spectrum>, t: f64) -> f64 {
        if !self.active(t) {
            return 0.0;
        }
        match self.kind {
            MotionKind::Fft { band, amplitude, offset, release } => {
                let Some(s) = spectrum else { return offset };
                // Peak hold with an exponential release (stateless, so offline renders match).
                let mut level = s.level(band, t);
                if release > 1e-6 {
                    let steps = ((release * 3.0) / ROW_STEP).ceil().min(400.0) as usize;
                    for k in 1..=steps {
                        let dt = k as f64 * ROW_STEP;
                        level = level.max(s.level(band, t - dt) * (-dt / release).exp());
                    }
                }
                offset + amplitude * level
            }
            MotionKind::Sine { min, max, rpm } => {
                let x = rpm * t / 60.0 * std::f64::consts::TAU;
                min + (max - min) * (0.5 + 0.5 * x.sin())
            }
            MotionKind::SawTooth { min, max, rpm } => {
                let x = rpm * t / 60.0;
                min + (max - min) * x.rem_euclid(1.0)
            }
            MotionKind::Spline { amplitude, offset } => offset + amplitude * self.curve.eval(t).unwrap_or(0.0),
        }
    }
}

/// The curve a new spline motion starts with: up and down again over ten seconds.
pub fn default_curve() -> Track {
    let keys = [(0.0, 0.0), (5.0, 1.0), (10.0, 0.0)].map(|(frame, value)| Key { frame, value }).to_vec();
    Track { path: String::new(), interp: Interp::Smooth, keys }
}

/// Shifts the gradient colors of every flame while active (JWildfire's balancing
/// preprocessor: the palette's red, green and blue modifiers, -255..255).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Balancing {
    pub red: i32,
    pub green: i32,
    pub blue: i32,
    /// Active from/until this many seconds into the music (`end` 0 = to the end).
    pub start: f64,
    pub end: f64,
}

impl Balancing {
    fn active(&self, t: f64) -> bool {
        t >= self.start && (self.end <= 0.0 || t <= self.end)
    }

    fn apply(&self, flame: &mut Flame) {
        let mods = crate::gradient_ops::GradientMods { red: self.red, green: self.green, blue: self.blue, ..Default::default() };
        for layer in &mut flame.layers {
            layer.gradient = layer.gradient.with_mods(&mods);
        }
    }
}

/// Applies `value` to `target` of `flame` (added to the property, or a relative transform).
pub fn apply_target(flame: &mut Flame, target: &str, value: f64) -> bool {
    if let Some((xf_path, op)) = target.split_once(".@") {
        let Some(xf) = xform_at_mut(flame, xf_path) else { return false };
        match op {
            "angle" => java::rotate(xf, value, false),
            "move_x" => java::local_translate(xf, value, 0.0, false),
            "move_y" => java::local_translate(xf, 0.0, value, false),
            "zoom" => java::scale(xf, 1.0 + value, true, true, false),
            "post_angle" => java::rotate(xf, value, true),
            "post_move_x" => java::local_translate(xf, value, 0.0, true),
            "post_move_y" => java::local_translate(xf, 0.0, value, true),
            "post_zoom" => java::scale(xf, 1.0 + value, true, true, true),
            _ => return false,
        }
        return true;
    }
    match get_prop(flame, target) {
        Some(v) => set_prop(flame, target, v + value),
        None => false,
    }
}

/// The properties of `flame` a motion can be linked to: (target, label).
pub fn targets(flame: &Flame) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (li, layer) in flame.layers.iter().enumerate() {
        for kind in [XFormKind::Normal, XFormKind::Final] {
            for xi in 0..layer.xforms_of(kind).len() {
                let p = format!("layer{li}.{}{xi}", kind.path_prefix());
                for (op, label) in XFORM_OPS {
                    let t = format!("{p}.@{op}");
                    let base = crate::animation::label(flame, &format!("{p}.weight"));
                    let xf_label = base.rsplit_once(" · ").map_or(base.as_str(), |(a, _)| a).to_string();
                    out.push((t, format!("{xf_label} · {label}")));
                }
            }
        }
    }
    for (p, _) in crate::animation::all_props(flame) {
        let label = crate::animation::label(flame, &p);
        out.push((p, label));
    }
    out
}

// ---- performance ----------------------------------------------------------------------------

/// A switch to pool flame `flame` at `time` seconds of the music.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cue {
    pub time: f64,
    pub flame: usize,
}

/// A recorded show: which flame was shown when (cues sorted by time).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Performance {
    pub cues: Vec<Cue>,
    /// Start and end in seconds of the music.
    pub start: f64,
    pub end: f64,
}

impl Performance {
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty() || self.end <= self.start
    }

    /// The flame shown at `t`, and the flame it morphs from with the morph progress 0..1.
    pub fn state_at(&self, t: f64, morph_seconds: f64) -> Option<(usize, Option<(usize, f64)>)> {
        let i = self.cues.iter().rposition(|c| c.time <= t).unwrap_or(0);
        let cue = self.cues.get(i)?;
        let morph = (i > 0 && morph_seconds > 1e-6 && t - cue.time < morph_seconds)
            .then(|| (self.cues[i - 1].flame, ((t - cue.time) / morph_seconds).clamp(0.0, 1.0)));
        Some((cue.flame, morph))
    }
}

// ---- project --------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct DanceProject {
    pub flames: Vec<Flame>,
    pub motions: Vec<Motion>,
    pub sound: Option<PathBuf>,
    /// Frames per second of exported videos.
    pub fps: f64,
    /// Length of the morph when switching flames, in frames.
    pub morph_frames: u32,
    pub width: u32,
    pub height: u32,
    /// Sample density of exported frames.
    pub quality: f64,
    pub performance: Performance,
    /// Color balancing of all flames (applied in order while active).
    pub balancing: Vec<Balancing>,
}

impl Default for DanceProject {
    fn default() -> Self {
        DanceProject {
            flames: Vec::new(),
            motions: Vec::new(),
            sound: None,
            fps: 30.0,
            morph_frames: 15,
            width: 1280,
            height: 720,
            quality: 200.0,
            performance: Performance::default(),
            balancing: Vec::new(),
        }
    }
}

impl DanceProject {
    pub fn morph_seconds(&self) -> f64 {
        self.morph_frames as f64 / self.fps.max(1.0)
    }

    /// Pool flame `index` moved by its motions at `t` seconds.
    pub fn moved_flame(&self, index: usize, spectrum: Option<&Spectrum>, t: f64) -> Option<Flame> {
        let mut f = self.flames.get(index)?.clone();
        for m in &self.motions {
            let mut value = None;
            for l in m.links.iter().filter(|l| l.flame == index) {
                let v = *value.get_or_insert_with(|| m.value(spectrum, t));
                apply_target(&mut f, &l.target, v);
            }
        }
        for b in self.balancing.iter().filter(|b| b.active(t)) {
            b.apply(&mut f);
        }
        Some(f)
    }

    /// The flame to show at `t` for the flame `state` (see [`Performance::state_at`]).
    pub fn flame_at(&self, state: (usize, Option<(usize, f64)>), spectrum: Option<&Spectrum>, t: f64) -> Option<Flame> {
        let next = self.moved_flame(state.0, spectrum, t)?;
        match state.1 {
            Some((prev, x)) if x < 1.0 => {
                let Some(from) = self.moved_flame(prev, spectrum, t) else { return Some(next) };
                // Smoothstep: no jump in speed at the start and end of the morph.
                let s = x * x * (3.0 - 2.0 * x);
                Some(morph_flames(MorphType::Morph, &from, &next, s))
            }
            _ => Some(next),
        }
    }

    /// The flame of the recorded performance at `frame` (at [`Self::fps`]). Rendered at the
    /// project size, the flame keeps its framing (the view scales with its own size).
    pub fn performance_flame(&self, spectrum: Option<&Spectrum>, frame: u32) -> Option<Flame> {
        let t = self.performance.start + frame as f64 / self.fps.max(1.0);
        let state = self.performance.state_at(t, self.morph_seconds())?;
        self.flame_at(state, spectrum, t)
    }

    /// Writes every frame of the recorded performance as a flame file next to `path`:
    /// `<name>00001.flame`, `<name>00002.flame`, … (JWildfire's "save recorded flames"), at the
    /// project size with the framing kept. Stops early (with an error) when `cancel` is set;
    /// `on_frame(index, total)` after each file. Returns the number of files.
    pub fn write_performance_flames(
        &self,
        spectrum: Option<&Spectrum>,
        path: &Path,
        cancel: &std::sync::atomic::AtomicBool,
        mut on_frame: impl FnMut(u32, u32),
    ) -> Result<u32, String> {
        let total = self.performance_frames();
        if total == 0 {
            return Err("nothing recorded".into());
        }
        let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let stem = path.file_name().map(|n| n.to_string_lossy().trim_end_matches(".flame").to_string()).unwrap_or_else(|| "dance".into());
        for i in 0..total {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let Some(mut f) = self.performance_flame(spectrum, i) else { continue };
            // the project size, the same part of the flame in view
            // as the renderer scales a flame drawn at another size (`View::new`)
            f.pixels_per_unit *= (self.width as f64 / f.width.max(1) as f64 + self.height as f64 / f.height.max(1) as f64) * 0.5;
            (f.width, f.height) = (self.width, self.height);
            let file = dir.join(format!("{stem}{:05}.flame", i + 1));
            crate::io::write_flame_to_file(&f, &file).map_err(|e| format!("{}: {e}", file.display()))?;
            on_frame(i, total);
        }
        Ok(total)
    }

    /// Number of frames of the recorded performance.
    pub fn performance_frames(&self) -> u32 {
        if self.performance.is_empty() { 0 } else { ((self.performance.end - self.performance.start) * self.fps).round().max(1.0) as u32 }
    }

    /// Removes pool flame `index` with its links and cues; later indices move down.
    pub fn remove_flame(&mut self, index: usize) {
        if index >= self.flames.len() {
            return;
        }
        self.flames.remove(index);
        for m in &mut self.motions {
            m.links.retain(|l| l.flame != index);
            for l in &mut m.links {
                if l.flame > index {
                    l.flame -= 1;
                }
            }
        }
        self.performance.cues.retain(|c| c.flame != index);
        for c in &mut self.performance.cues {
            if c.flame > index {
                c.flame -= 1;
            }
        }
    }

    /// Replaces the motions by a default set that moves the first four transforms and the
    /// first final transform of every layer of every pool flame (after JWildfire's
    /// `DefaultMotionCreator`, retuned to the normalized bands).
    pub fn create_default_motions(&mut self) {
        // (xform index, final?, op, band, amplitude)
        const DEFAULTS: [(usize, bool, &str, usize, f64); 13] = [
            (0, false, "angle", 1, -20.0),
            (0, false, "move_x", 3, -0.06),
            (0, false, "move_y", 5, -0.06),
            (1, false, "angle", 7, -12.0),
            (1, false, "move_x", 9, 0.04),
            (1, false, "move_y", 11, -0.04),
            (2, false, "angle", 15, 15.0),
            (2, false, "move_x", 17, -0.05),
            (2, false, "move_y", 19, 0.04),
            (3, false, "angle", 23, 10.0),
            (3, false, "move_x", 25, 0.03),
            (3, false, "move_y", 27, 0.03),
            (0, true, "zoom", 2, 0.08),
        ];
        self.motions = DEFAULTS
            .iter()
            .map(|&(xi, fin, op, band, amplitude)| {
                let kind = MotionKind::Fft { band, amplitude, offset: 0.0, release: 0.15 };
                let mut m = Motion::new(format!("{} {} {op}", if fin { "Final" } else { "Transform" }, xi + 1), kind);
                for (fi, f) in self.flames.iter().enumerate() {
                    for (li, layer) in f.layers.iter().enumerate() {
                        let (kind, prefix) = if fin { (XFormKind::Final, "final") } else { (XFormKind::Normal, "xf") };
                        if layer.xforms_of(kind).len() > xi {
                            m.links.push(Link { flame: fi, target: format!("layer{li}.{prefix}{xi}.@{op}") });
                        }
                    }
                }
                m
            })
            .collect();
    }
}

// ---- .rwfdance files ------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DanceIoError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("not a dance project: {0}")]
    Format(String),
}

fn fmt(v: f64) -> String {
    format!("{v}")
}

pub fn write_project(p: &DanceProject) -> String {
    let mut s = String::new();
    let attr = |s: &mut String, k: &str, v: String| s.push_str(&format!(" {k}=\"{}\"", xml::escape(&v)));
    s.push_str("<rwf-dance");
    attr(&mut s, "application", format!("RWildFire {}", env!("CARGO_PKG_VERSION")));
    attr(&mut s, "version", "1".into());
    attr(&mut s, "fps", fmt(p.fps));
    attr(&mut s, "morph_frames", p.morph_frames.to_string());
    attr(&mut s, "width", p.width.to_string());
    attr(&mut s, "height", p.height.to_string());
    attr(&mut s, "quality", fmt(p.quality));
    if let Some(snd) = &p.sound {
        attr(&mut s, "sound", snd.display().to_string());
    }
    s.push_str(">\n  <pool>\n");
    for f in &p.flames {
        s.push_str(&crate::io::write_flame(f));
        s.push('\n');
    }
    s.push_str("  </pool>\n");
    for m in &p.motions {
        s.push_str("  <motion");
        attr(&mut s, "name", m.name.clone());
        match m.kind {
            MotionKind::Fft { band, amplitude, offset, release } => {
                attr(&mut s, "type", "fft".into());
                attr(&mut s, "band", band.to_string());
                attr(&mut s, "amplitude", fmt(amplitude));
                attr(&mut s, "offset", fmt(offset));
                attr(&mut s, "release", fmt(release));
            }
            MotionKind::Sine { min, max, rpm } | MotionKind::SawTooth { min, max, rpm } => {
                attr(&mut s, "type", if matches!(m.kind, MotionKind::Sine { .. }) { "sine" } else { "sawtooth" }.into());
                attr(&mut s, "min", fmt(min));
                attr(&mut s, "max", fmt(max));
                attr(&mut s, "rpm", fmt(rpm));
            }
            MotionKind::Spline { amplitude, offset } => {
                attr(&mut s, "type", "spline".into());
                attr(&mut s, "amplitude", fmt(amplitude));
                attr(&mut s, "offset", fmt(offset));
                attr(&mut s, "interp", m.curve.interp.as_str().into());
            }
        }
        attr(&mut s, "start", fmt(m.start));
        attr(&mut s, "end", fmt(m.end));
        s.push_str(">\n");
        if matches!(m.kind, MotionKind::Spline { .. }) {
            for k in &m.curve.keys {
                s.push_str("    <key");
                attr(&mut s, "time", fmt(k.frame));
                attr(&mut s, "value", fmt(k.value));
                s.push_str("/>\n");
            }
        }
        for l in &m.links {
            s.push_str("    <link");
            attr(&mut s, "flame", l.flame.to_string());
            attr(&mut s, "target", l.target.clone());
            s.push_str("/>\n");
        }
        s.push_str("  </motion>\n");
    }
    for b in &p.balancing {
        s.push_str("  <balancing");
        attr(&mut s, "red", b.red.to_string());
        attr(&mut s, "green", b.green.to_string());
        attr(&mut s, "blue", b.blue.to_string());
        attr(&mut s, "start", fmt(b.start));
        attr(&mut s, "end", fmt(b.end));
        s.push_str("/>\n");
    }
    let perf = &p.performance;
    if !perf.cues.is_empty() {
        s.push_str("  <performance");
        attr(&mut s, "start", fmt(perf.start));
        attr(&mut s, "end", fmt(perf.end));
        s.push_str(">\n");
        for c in &perf.cues {
            s.push_str("    <cue");
            attr(&mut s, "time", fmt(c.time));
            attr(&mut s, "flame", c.flame.to_string());
            s.push_str("/>\n");
        }
        s.push_str("  </performance>\n");
    }
    s.push_str("</rwf-dance>\n");
    s
}

pub fn read_project(src: &str) -> Result<DanceProject, DanceIoError> {
    let root = xml::parse(src).map_err(|e| DanceIoError::Format(e.to_string()))?;
    let mut all = Vec::new();
    root.find_all("rwf-dance", &mut all);
    let el: &Element =
        if root.name == "rwf-dance" { &root } else { all.first().copied().ok_or_else(|| DanceIoError::Format("no <rwf-dance>".into()))? };
    let num = |e: &Element, k: &str| e.attr(k).and_then(|v| v.trim().parse::<f64>().ok());
    let d = DanceProject::default();
    let mut p = DanceProject {
        fps: num(el, "fps").filter(|v| *v > 0.0).unwrap_or(d.fps),
        morph_frames: num(el, "morph_frames").map_or(d.morph_frames, |v| v.max(0.0) as u32),
        width: num(el, "width").map_or(d.width, |v| v.max(1.0) as u32),
        height: num(el, "height").map_or(d.height, |v| v.max(1.0) as u32),
        quality: num(el, "quality").filter(|v| *v > 0.0).unwrap_or(d.quality),
        sound: el.attr("sound").filter(|s| !s.is_empty()).map(PathBuf::from),
        ..d
    };
    if let Some(pool) = el.children_named("pool").next() {
        p.flames = pool.children.iter().filter(|c| c.name == "flame" || c.name == "jwf-flame").map(crate::io::parse_flame).collect();
    }
    for m in el.children_named("motion") {
        let n = |k: &str, def: f64| num(m, k).unwrap_or(def);
        let kind = match m.attr("type").unwrap_or("fft") {
            "sine" => MotionKind::Sine { min: n("min", -1.0), max: n("max", 1.0), rpm: n("rpm", 10.0) },
            "sawtooth" => MotionKind::SawTooth { min: n("min", 0.0), max: n("max", 1.0), rpm: n("rpm", 10.0) },
            "spline" => MotionKind::Spline { amplitude: n("amplitude", 1.0), offset: n("offset", 0.0) },
            _ => MotionKind::Fft {
                band: (n("band", 0.0).max(0.0) as usize).min(BANDS - 1),
                amplitude: n("amplitude", 1.0),
                offset: n("offset", 0.0),
                release: n("release", 0.15),
            },
        };
        let links = m
            .children_named("link")
            .filter_map(|l| Some(Link { flame: num(l, "flame")? as usize, target: l.attr("target")?.to_string() }))
            .filter(|l| l.flame < p.flames.len())
            .collect();
        let mut curve = default_curve();
        if matches!(kind, MotionKind::Spline { .. }) {
            curve.interp = Interp::parse(m.attr("interp").unwrap_or("smooth"));
            curve.keys = m.children_named("key").filter_map(|k| Some(Key { frame: num(k, "time")?, value: num(k, "value")? })).collect();
            curve.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        }
        p.motions.push(Motion { name: m.attr("name").unwrap_or_default().to_string(), kind, start: n("start", 0.0), end: n("end", 0.0), links, curve });
    }
    for b in el.children_named("balancing") {
        let n = |k: &str| num(b, k).unwrap_or(0.0);
        p.balancing.push(Balancing {
            red: (n("red") as i32).clamp(-255, 255),
            green: (n("green") as i32).clamp(-255, 255),
            blue: (n("blue") as i32).clamp(-255, 255),
            start: n("start"),
            end: n("end"),
        });
    }
    if let Some(perf) = el.children_named("performance").next() {
        p.performance = Performance {
            start: num(perf, "start").unwrap_or(0.0),
            end: num(perf, "end").unwrap_or(0.0),
            cues: perf
                .children_named("cue")
                .filter_map(|c| Some(Cue { time: num(c, "time")?, flame: num(c, "flame")? as usize }))
                .filter(|c| c.flame < p.flames.len())
                .collect(),
        };
        p.performance.cues.sort_by(|a, b| a.time.total_cmp(&b.time));
    }
    Ok(p)
}

pub fn read_project_file(path: impl AsRef<Path>) -> Result<DanceProject, DanceIoError> {
    read_project(&std::fs::read_to_string(path)?)
}

pub fn write_project_file(p: &DanceProject, path: impl AsRef<Path>) -> Result<(), DanceIoError> {
    Ok(std::fs::write(path, write_project(p))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::{Generator, generate};
    use crate::rng::Rng;

    /// A 220 Hz tone in the first second, a 3 kHz tone in the second.
    fn two_tones() -> Audio {
        let rate = 44_100;
        let samples = (0..2 * rate)
            .map(|i| {
                let t = i as f64 / rate as f64;
                let hz = if t < 1.0 { 220.0 } else { 3000.0 };
                (0.5 * (std::f64::consts::TAU * hz * t).sin()) as f32
            })
            .collect();
        Audio { samples, rate }
    }

    fn band_of(hz: f64) -> usize {
        (0..BANDS).rev().find(|b| band_low_hz(*b) <= hz).unwrap()
    }

    #[test]
    fn spectrum_follows_the_music() {
        let s = Spectrum::analyse(&two_tones());
        assert!((s.duration() - 2.0).abs() < 0.02);
        let (low, high) = (band_of(220.0), band_of(3000.0));
        assert!(s.level(low, 0.5) > 0.8 && s.level(high, 0.5) < 0.3, "{} {}", s.level(low, 0.5), s.level(high, 0.5));
        assert!(s.level(high, 1.5) > 0.8 && s.level(low, 1.5) < 0.3, "{} {}", s.level(high, 1.5), s.level(low, 1.5));
        assert_eq!(s.level(low, 5.0), 0.0);
    }

    #[test]
    fn fft_motion_releases_slowly() {
        let s = Spectrum::analyse(&two_tones());
        let band = band_of(220.0);
        let m = Motion::new("m", MotionKind::Fft { band, amplitude: 2.0, offset: 1.0, release: 0.3 });
        assert!((m.value(Some(&s), 0.5) - 3.0).abs() < 0.3);
        // The tone stopped at 1 s: still high shortly after, low later.
        assert!(m.value(Some(&s), 1.1) > 2.0);
        assert!(m.value(Some(&s), 1.9) < 1.5);
        assert_eq!(m.value(None, 0.5), 1.0);
        let sine = Motion::new("s", MotionKind::Sine { min: -1.0, max: 1.0, rpm: 60.0 });
        assert!((sine.value(None, 0.25) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn targets_move_flames() {
        let mut flame = generate(Generator::Classic, &mut Rng::new(3));
        let before = flame.layers[0].xforms[0].coefs;
        assert!(apply_target(&mut flame, "layer0.xf0.@angle", 90.0));
        assert_ne!(flame.layers[0].xforms[0].coefs, before);
        let b = flame.brightness;
        assert!(apply_target(&mut flame, "brightness", 0.5));
        assert!((flame.brightness - b - 0.5).abs() < 1e-12);
        assert!(!apply_target(&mut flame, "layer0.xf99.@angle", 1.0));
        let t = targets(&flame);
        assert!(t.iter().any(|(p, l)| p == "layer0.xf0.@angle" && l.contains("rotate")));
        assert!(t.iter().any(|(p, _)| p == "brightness"));
    }

    #[test]
    fn performance_morphs_between_cues() {
        let perf = Performance { cues: vec![Cue { time: 0.0, flame: 0 }, Cue { time: 2.0, flame: 1 }], start: 0.0, end: 4.0 };
        assert_eq!(perf.state_at(1.0, 0.5), Some((0, None)));
        assert_eq!(perf.state_at(2.25, 0.5), Some((1, Some((0, 0.5)))));
        assert_eq!(perf.state_at(3.0, 0.5), Some((1, None)));
    }

    #[test]
    fn project_round_trip() {
        let mut rng = Rng::new(5);
        let mut p =
            DanceProject { flames: vec![generate(Generator::Classic, &mut rng), generate(Generator::Classic, &mut rng)], ..Default::default() };
        p.create_default_motions();
        assert!(p.motions.iter().any(|m| m.links.iter().any(|l| l.flame == 1)));
        p.motions.push(Motion::new("spin", MotionKind::SawTooth { min: 0.0, max: 360.0, rpm: 4.0 }));
        let mut spline = Motion::new("wave", MotionKind::Spline { amplitude: 2.0, offset: 0.5 });
        spline.curve.interp = Interp::Linear;
        spline.curve.keys = vec![Key { frame: 1.0, value: -1.0 }, Key { frame: 4.0, value: 3.0 }];
        p.motions.push(spline);
        p.balancing.push(Balancing { red: 40, green: -20, blue: 0, start: 1.0, end: 0.0 });
        p.sound = Some(PathBuf::from("/music/song.mp3"));
        p.performance = Performance { cues: vec![Cue { time: 0.5, flame: 0 }, Cue { time: 3.0, flame: 1 }], start: 0.5, end: 6.0 };
        let q = read_project(&write_project(&p)).unwrap();
        assert_eq!(q.flames.len(), 2);
        assert_eq!(q.motions, p.motions);
        assert_eq!(q.performance, p.performance);
        assert_eq!(q.balancing, p.balancing);
        assert_eq!(q.sound, p.sound);
        assert_eq!(q.performance_frames(), (5.5 * 30.0) as u32);
        assert!(q.performance_flame(None, 100).is_some());

        p.remove_flame(0);
        assert!(p.motions.iter().all(|m| m.links.iter().all(|l| l.flame == 0)));
        assert_eq!(p.performance.cues, vec![Cue { time: 3.0, flame: 0 }]);
    }

    #[test]
    fn spline_motion_follows_its_curve() {
        let mut m = Motion::new("w", MotionKind::Spline { amplitude: 2.0, offset: 1.0 });
        m.curve.interp = Interp::Linear;
        m.curve.keys = vec![Key { frame: 0.0, value: 0.0 }, Key { frame: 10.0, value: 1.0 }];
        assert!((m.value(None, 5.0) - 2.0).abs() < 1e-9, "1 + 2 * 0.5");
        assert!((m.value(None, 20.0) - 3.0).abs() < 1e-9, "holds the last key");
    }

    #[test]
    fn balancing_shifts_the_gradient_while_active() {
        let mut rng = Rng::new(9);
        let mut p = DanceProject { flames: vec![generate(Generator::Classic, &mut rng)], ..Default::default() };
        p.balancing.push(Balancing { red: 255, green: 0, blue: 0, start: 2.0, end: 4.0 });
        let red = |f: &Flame| f.layers[0].gradient.colors.iter().map(|c| c[0] as u32).sum::<u32>();
        let before = red(&p.moved_flame(0, None, 1.0).unwrap());
        let during = red(&p.moved_flame(0, None, 3.0).unwrap());
        let after = red(&p.moved_flame(0, None, 5.0).unwrap());
        assert!(during > before && before == after, "{before} {during} {after}");
    }

    #[test]
    fn show_is_written_as_flame_files() {
        let mut rng = Rng::new(11);
        let mut p = DanceProject { flames: vec![generate(Generator::Classic, &mut rng)], fps: 10.0, width: 320, height: 180, ..Default::default() };
        p.performance = Performance { cues: vec![Cue { time: 0.0, flame: 0 }], start: 0.0, end: 0.5 };
        let dir = std::env::temp_dir().join(format!("rwf-dance-flames-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let n = p.write_performance_flames(None, &dir.join("show.flame"), &std::sync::atomic::AtomicBool::new(false), |_, _| {}).unwrap();
        assert_eq!(n, 5);
        let f = &crate::io::read_flames_from_file(dir.join("show00005.flame")).unwrap()[0];
        assert_eq!((f.width, f.height), (320, 180));
        assert!(!dir.join("show00006.flame").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
