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

//! The channel mixer of a flame (`ChannelMixerMode`, `*ColorFunc`): curves that remap the
//! average color of every histogram cell before tone mapping. The curves are JWildfire
//! envelopes (`org.jwildfire.envelope.Envelope`), evaluated exactly like in JWildfire.

/// `ChannelMixerCurves.FILTER_SCALE`: the curves work on colors scaled to 0..25600.
pub const FILTER_SCALE: f64 = 100.0;
const SCALE: f64 = 256.0 * FILTER_SCALE;
const BORDER: f64 = SCALE / 10.0 * 2.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MixerMode {
    #[default]
    Off,
    /// One curve (rr) on the brightness.
    Brightness,
    /// One curve per channel (rr, gg, bb).
    Rgb,
    /// All nine curves: every output channel is the sum of curves of the three input channels.
    Full,
}

impl MixerMode {
    pub const ALL: [MixerMode; 4] = [MixerMode::Off, MixerMode::Brightness, MixerMode::Rgb, MixerMode::Full];

    pub fn as_str(self) -> &'static str {
        match self {
            MixerMode::Off => "OFF",
            MixerMode::Brightness => "BRIGHTNESS",
            MixerMode::Rgb => "RGB",
            MixerMode::Full => "FULL",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MixerMode::Off => "Off",
            MixerMode::Brightness => "Brightness",
            MixerMode::Rgb => "RGB",
            MixerMode::Full => "Full",
        }
    }

    /// Indices (into [`ChannelMixer::curves`]) of the curves the mode uses.
    pub fn curves(self) -> &'static [usize] {
        match self {
            MixerMode::Off => &[],
            MixerMode::Brightness => &[0],
            MixerMode::Rgb => &[0, 4, 8],
            MixerMode::Full => &[0, 1, 2, 3, 4, 5, 6, 7, 8],
        }
    }
}

/// `Envelope.Interpolation`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CurveInterpolation {
    #[default]
    Spline,
    Bezier,
    Linear,
}

impl CurveInterpolation {
    fn as_str(self) -> &'static str {
        match self {
            CurveInterpolation::Spline => "SPLINE",
            CurveInterpolation::Bezier => "BEZIER",
            CurveInterpolation::Linear => "LINEAR",
        }
    }
}

/// A JWildfire `MotionCurve` used as mixer curve (integer x, double y).
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub x: Vec<i32>,
    pub y: Vec<f64>,
    pub interpolation: CurveInterpolation,
    pub enabled: bool,
    pub view_xmin: i32,
    pub view_xmax: i32,
    pub view_ymin: f64,
    pub view_ymax: f64,
    pub selected_idx: i32,
    pub locked: bool,
}

impl Curve {
    /// `ChannelMixerCurves.DEFAULT` (identity) or `ZERO`.
    pub fn standard(identity: bool) -> Self {
        let ftoi = |v: f64| crate::math::ftoi(v);
        let x = vec![0, ftoi(SCALE / 4.0), ftoi(SCALE / 2.0), ftoi(SCALE * 3.0 / 4.0), ftoi(SCALE)];
        let y = if identity { vec![0.0, SCALE / 4.0, SCALE / 2.0, SCALE * 3.0 / 4.0, SCALE] } else { vec![0.0; 5] };
        Self {
            x,
            y,
            interpolation: CurveInterpolation::Spline,
            enabled: false,
            view_xmin: ftoi(-BORDER / 4.0),
            view_xmax: ftoi(SCALE + BORDER / 4.0),
            view_ymin: -BORDER,
            view_ymax: SCALE + BORDER,
            selected_idx: 0,
            locked: false,
        }
    }
}

/// The channel mixer of a flame. Curve order: rr, rg, rb, gr, gg, gb, br, bg, bb (`xy` = the
/// contribution of input channel `y` to output channel `x`).
#[derive(Clone, Debug, PartialEq)]
pub struct ChannelMixer {
    pub mode: MixerMode,
    pub curves: [Curve; 9],
}

impl Default for ChannelMixer {
    /// `Flame.resetMixerCurves`: identity curves on the diagonal, zero curves elsewhere.
    fn default() -> Self {
        Self { mode: MixerMode::Off, curves: std::array::from_fn(|i| Curve::standard(i % 4 == 0)) }
    }
}

const CURVE_NAMES: [&str; 9] = ["rr", "rg", "rb", "gr", "gg", "gb", "br", "bg", "bb"];

impl ChannelMixer {
    /// Reads `mixer_mode` and the curves of the mode (`mixer_<xy>_curve_*`) and removes them from
    /// `attrs`.
    pub fn take_from_attrs(attrs: &mut Vec<(String, String)>) -> Self {
        let mut m = Self::default();
        let get = |k: &str| attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.trim().to_string());
        if let Some(v) = get("mixer_mode") {
            m.mode = MixerMode::ALL.into_iter().find(|t| t.as_str() == v).unwrap_or(MixerMode::Off);
        }
        for &i in m.mode.curves() {
            let p = format!("mixer_{}_curve_", CURVE_NAMES[i]);
            let c = &mut m.curves[i];
            let int = |k: &str| get(&format!("{p}{k}")).and_then(|v| v.parse::<f64>().ok()).map(|v| v as i32);
            let float = |k: &str| get(&format!("{p}{k}")).and_then(|v| v.parse::<f64>().ok());
            let flag = |k: &str| get(&format!("{p}{k}")).map(|v| v == "true");
            if let Some(v) = flag("enabled") {
                c.enabled = v;
            }
            if let Some(v) = int("view_xmin") {
                c.view_xmin = v;
            }
            if let Some(v) = int("view_xmax") {
                c.view_xmax = v;
            }
            if let Some(v) = float("view_ymin") {
                c.view_ymin = v;
            }
            if let Some(v) = float("view_ymax") {
                c.view_ymax = v;
            }
            if let Some(v) = get(&format!("{p}interpolation")) {
                c.interpolation = match v.as_str() {
                    "LINEAR" => CurveInterpolation::Linear,
                    "BEZIER" => CurveInterpolation::Bezier,
                    _ => CurveInterpolation::Spline,
                };
            }
            if let Some(v) = int("selected_idx") {
                c.selected_idx = v;
            }
            if let Some(v) = flag("locked") {
                c.locked = v;
            }
            if let Some(n) = int("point_count") {
                let n = n.max(0) as usize;
                c.x = (0..n).map(|k| int(&format!("x{k}")).unwrap_or(0)).collect();
                c.y = (0..n).map(|k| float(&format!("y{k}")).unwrap_or(0.0)).collect();
            }
        }
        attrs.retain(|(k, _)| !(k == "mixer_mode" || CURVE_NAMES.iter().any(|n| k.starts_with(&format!("mixer_{n}_curve_")))));
        m
    }

    /// Writes the attributes read by [`Self::take_from_attrs`] (as JWildfire: the mode always,
    /// the curves the mode uses).
    pub fn write_attrs(&self, mut attr: impl FnMut(&str, String)) {
        let f = crate::io::fmt_f;
        attr("mixer_mode", self.mode.as_str().to_string());
        for &i in self.mode.curves() {
            let p = format!("mixer_{}_curve_", CURVE_NAMES[i]);
            let c = &self.curves[i];
            attr(&format!("{p}enabled"), c.enabled.to_string());
            attr(&format!("{p}view_xmin"), c.view_xmin.to_string());
            attr(&format!("{p}view_xmax"), c.view_xmax.to_string());
            attr(&format!("{p}view_ymin"), f(c.view_ymin));
            attr(&format!("{p}view_ymax"), f(c.view_ymax));
            attr(&format!("{p}interpolation"), c.interpolation.as_str().to_string());
            attr(&format!("{p}selected_idx"), c.selected_idx.to_string());
            attr(&format!("{p}locked"), c.locked.to_string());
            attr(&format!("{p}point_count"), c.x.len().to_string());
            for k in 0..c.x.len() {
                attr(&format!("{p}x{k}"), c.x[k].to_string());
                attr(&format!("{p}y{k}"), f(c.y.get(k).copied().unwrap_or(0.0)));
            }
        }
    }

    pub fn is_active(&self) -> bool {
        self.mode != MixerMode::Off
    }

    /// The color function (`ChannelMixerMode.getColorFunc`), `None` if off.
    pub fn color_func(&self) -> Option<ColorFunc> {
        if !self.is_active() {
            return None;
        }
        let env: [Envelope; 9] = std::array::from_fn(|i| if self.mode.curves().contains(&i) { Envelope::new(&self.curves[i]) } else { Envelope::default() });
        Some(ColorFunc { mode: self.mode, env })
    }
}

/// Samples per curve of [`ColorFunc::lut`].
pub const LUT_SIZE: usize = 4096;

impl ColorFunc {
    /// The nine curves sampled at `LUT_SIZE` points over 0..25600 (for the GPU, which
    /// interpolates linearly between them).
    pub fn lut(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(9 * LUT_SIZE);
        for e in &self.env {
            for k in 0..LUT_SIZE {
                out.push(e.evaluate(k as f64 * SCALE / (LUT_SIZE - 1) as f64) as f32);
            }
        }
        out
    }

    /// 1 brightness, 2 RGB, 3 full.
    pub fn mode_index(&self) -> u32 {
        match self.mode {
            MixerMode::Off => 0,
            MixerMode::Brightness => 1,
            MixerMode::Rgb => 2,
            MixerMode::Full => 3,
        }
    }
}

/// `BrightnessColorFunc` / `RGBColorFunc` / `FullColorFunc`.
#[derive(Clone, Debug)]
pub struct ColorFunc {
    mode: MixerMode,
    env: [Envelope; 9],
}

impl ColorFunc {
    /// Maps a cell color (`rgb` = accumulated channel sums of `count` samples).
    pub fn map_cell(&self, count: f64, rgb: [f64; 3]) -> [f64; 3] {
        let raw = rgb.map(|c| c * FILTER_SCALE / count);
        self.map(raw).map(|c| c * count / FILTER_SCALE)
    }

    /// `mapRGBToR` / `G` / `B` of colors in 0..25600.
    pub fn map(&self, [r, g, b]: [f64; 3]) -> [f64; 3] {
        let e = &self.env;
        match self.mode {
            MixerMode::Off => [0.0; 3],
            MixerMode::Brightness => {
                let brightness = 0.2990 * r + 0.5880 * g + 0.1130 * b;
                let f = e[0].evaluate(brightness) / brightness;
                [f * r, f * g, f * b]
            }
            MixerMode::Rgb => [e[0].evaluate(r), e[4].evaluate(g), e[8].evaluate(b)],
            MixerMode::Full => [
                e[0].evaluate(r) + e[1].evaluate(g) + e[2].evaluate(b),
                e[3].evaluate(r) + e[4].evaluate(g) + e[5].evaluate(b),
                e[6].evaluate(r) + e[7].evaluate(g) + e[8].evaluate(b),
            ],
        }
    }
}

/// An `Envelope` with its interpolated points (`InterpolatedPoints`), evaluated with bisection.
#[derive(Clone, Debug, Default)]
pub struct Envelope {
    y: Vec<f64>,
    xmin: f64,
    xmax: f64,
    /// Interpolated points; like JWildfire the whole buffer is searched, including the unused
    /// zero tail.
    vsx: Vec<f64>,
    vsy: Vec<f64>,
}

impl Envelope {
    pub fn new(c: &Curve) -> Self {
        let n = c.x.len().min(c.y.len());
        let (x, y) = (&c.x[..n], &c.y[..n]);
        let (mut xmin, mut xmax) = (0.0, 0.0);
        if n > 0 {
            xmin = x[0] as f64;
            xmax = x[0] as f64;
            for &v in &x[1..] {
                let v = v as f64;
                if v < xmin {
                    xmin = v;
                } else if v > xmax {
                    xmax = v;
                }
            }
        }
        let (vsx, vsy) = if n >= 2 {
            let subdiv = calc_subdiv(x);
            let xs: Vec<f64> = x.iter().map(|&v| v as f64).collect();
            let interp = if n > 2 { c.interpolation } else { CurveInterpolation::Linear };
            (interpolate(&xs, subdiv, interp), interpolate(y, subdiv, interp))
        } else {
            (Vec::new(), Vec::new())
        };
        Self { y: y.to_vec(), xmin, xmax, vsx, vsy }
    }

    /// The interpolated curve points without the unused tail (for drawing the curve).
    pub fn points(&self) -> Vec<(f64, f64)> {
        let mut out: Vec<(f64, f64)> = Vec::new();
        for (&x, &y) in self.vsx.iter().zip(&self.vsy) {
            // Segment joints overlap by rounding errors; the tail drops back to zero.
            if out.last().is_some_and(|&(px, _)| x < px - 1.0) {
                break;
            }
            out.push((x, y));
        }
        out
    }

    /// `Envelope.evaluate` (with bisection).
    pub fn evaluate(&self, t: f64) -> f64 {
        let n = self.y.len();
        if n == 0 {
            return 0.0;
        } else if n == 1 {
            return self.y[0];
        } else if t <= self.xmin {
            return self.y[0];
        } else if t >= self.xmax {
            return self.y[n - 1];
        }
        let (vsx, vsy) = (&self.vsx, &self.vsy);
        let (mut indl, mut indr): (i64, i64) = (-1, -1);
        let (mut low, mut high) = (0i64, vsx.len() as i64 - 1);
        while low <= high {
            let mid = ((low + high) as u64 >> 1) as i64;
            let v = vsx[mid as usize];
            if v < t {
                low = mid + 1;
                indl = mid;
            } else if v > t {
                indr = mid;
                high = mid - 1;
            } else {
                return vsy[mid as usize];
            }
        }
        if indl >= 0 && indr >= 0 {
            let (l, r) = (indl as usize, indr as usize);
            let xdist = vsx[r] - vsx[l];
            if xdist < 0.00000001 { vsy[l] } else { vsy[l] + (t - vsx[l]) / xdist * (vsy[r] - vsy[l]) }
        } else if indl >= 0 {
            vsy[indl as usize]
        } else if indr >= 0 {
            vsy[indr as usize]
        } else {
            0.0
        }
    }
}

/// `Interpolation.calcSubDivPRV`.
fn calc_subdiv(x: &[i32]) -> usize {
    let mut xdistmax = x[1] - x[0];
    for i in 1..x.len() - 1 {
        let d = x[i + 1] - x[i];
        if d > xdistmax {
            xdistmax = d;
        }
    }
    ((1.6 * xdistmax as f64 + 0.5) as i32).max(3) as usize
}

/// `SplineInterpolation` / `BezierInterpolation` / `LinearInterpolation`: the full destination
/// buffer of `snum * (subdiv + 5)` values (unused entries stay 0).
fn interpolate(src: &[f64], subdiv: usize, interp: CurveInterpolation) -> Vec<f64> {
    const SMALL: f64 = 1.0e-5;
    let snum = src.len();
    let mut dest = vec![0.0; snum * (subdiv + 5)];
    let du = 1.0 / subdiv as f64;
    let mut j = 0;
    match interp {
        CurveInterpolation::Linear => {
            for i in 0..snum - 1 {
                let mut x0 = src[i];
                let dx = (src[i + 1] - x0) / (subdiv as f64 - 1.0);
                for _ in 0..subdiv {
                    dest[j] = x0;
                    j += 1;
                    x0 += dx;
                }
            }
        }
        CurveInterpolation::Spline => {
            // Catmull-Rom (B = 0.5)
            let eval = |u: f64, xa: f64, xb: f64, xc: f64, xd: f64| {
                let b = 0.5;
                let mut c = u * u * u * (-b * xa + (2.0 - b) * xb + (b - 2.0) * xc + b * xd);
                c += u * u * (2.0 * b * xa + (b - 3.0) * xb + (3.0 - 2.0 * b) * xc - b * xd);
                c += u * (-b * xa + b * xc);
                c + xb
            };
            for i in 0..snum - 1 {
                let mut u = 0.0;
                loop {
                    dest[j] = if i == 0 {
                        eval(u, src[i], src[i], src[i + 1], src[i + 2])
                    } else if i == snum - 2 {
                        eval(u, src[i - 1], src[i], src[i + 1], src[i + 1])
                    } else {
                        eval(u, src[i - 1], src[i], src[i + 1], src[i + 2])
                    };
                    u += du;
                    j += 1;
                    if u >= 1.0 + SMALL {
                        break;
                    }
                }
            }
        }
        CurveInterpolation::Bezier => {
            let eval = |u: f64, xa: f64, xb: f64, xc: f64| {
                let mut h = u * u;
                let mut c = xa - xb - xb + xc;
                c *= h;
                h = -xa - xa + xb + xb;
                h *= u;
                c += h;
                c + xa
            };
            for i in 1..snum - 1 {
                let mut u = 0.0;
                loop {
                    let xm1 = if i == 1 { src[i - 1] } else { 0.5 * (src[i - 1] + src[i]) };
                    let xm3 = if i == snum - 2 { src[i + 1] } else { 0.5 * (src[i] + src[i + 1]) };
                    dest[j] = eval(u, xm1, src[i], xm3);
                    u += du;
                    j += 1;
                    if u >= 1.0 + SMALL {
                        break;
                    }
                }
            }
        }
    }
    dest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_curves_keep_colors() {
        let mut m = ChannelMixer { mode: MixerMode::Rgb, ..Default::default() };
        let f = m.color_func().unwrap();
        let c = f.map_cell(4.0, [400.0, 200.0, 100.0]);
        for (a, b) in c.iter().zip([400.0, 200.0, 100.0]) {
            assert!((a - b).abs() < 1e-6, "{c:?}");
        }
        m.mode = MixerMode::Full;
        let c = m.color_func().unwrap().map_cell(2.0, [10.0, 20.0, 30.0]);
        assert!((c[0] - 10.0).abs() < 1e-6 && (c[1] - 20.0).abs() < 1e-6 && (c[2] - 30.0).abs() < 1e-6, "{c:?}");
    }

    #[test]
    fn attrs_round_trip() {
        let mut m = ChannelMixer { mode: MixerMode::Full, ..Default::default() };
        m.curves[1].y[2] = 1234.5;
        m.curves[1].interpolation = CurveInterpolation::Bezier;
        let mut attrs = Vec::new();
        m.write_attrs(|k, v| attrs.push((k.to_string(), v)));
        let r = ChannelMixer::take_from_attrs(&mut attrs);
        assert_eq!(r, m);
        assert!(attrs.is_empty());
    }
}
