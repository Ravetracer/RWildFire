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

//! JWildfire's random gradient generators (`org.jwildfire.create.tina.randomgradient`) and
//! `SimilarGradientCreator`.

use crate::gradient::Gradient;
use crate::mixer::{Curve, Envelope};
use crate::rng::Rng;

const PALETTE_SIZE: usize = 256;

/// `Tools.roundColor`.
fn round_color(v: f64) -> u8 {
    ((v + 0.5) as i32).clamp(0, 255) as u8
}

/// `Tools.FTOI`.
fn ftoi(v: f64) -> i32 {
    v.round() as i32
}

/// `HSLTransformer.hsl2rgb` (components 0..1, colors rounded to 0..255).
pub fn hsl2rgb(hue: f64, saturation: f64, luminosity: f64) -> [u8; 3] {
    let v = if luminosity <= 0.5 { luminosity * (1.0 + saturation) } else { luminosity + saturation - luminosity * saturation };
    if v <= 0.0 {
        return [0, 0, 0];
    }
    let hue = (hue * 6.0).clamp(0.0, 6.0);
    let y = luminosity + luminosity - v;
    let frac = hue - hue.trunc();
    let x = y + (v - y) * frac;
    let z = v - (v - y) * frac;
    let (r, g, b) = match hue as i32 {
        0 => (v, x, y),
        1 => (z, v, y),
        2 => (y, v, x),
        3 => (y, z, v),
        4 => (x, y, v),
        _ => (v, y, z),
    };
    [round_color(r * 255.0), round_color(g * 255.0), round_color(b * 255.0)]
}

/// `HSLTransformer.rgb2hsl`: (hue, saturation, luminosity), each 0..1.
pub fn rgb2hsl(c: [u8; 3]) -> (f64, f64, f64) {
    hsl_of(c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0)
}

/// Shared by `HSLTransformer.rgb2hsl` and `HSLRGBConverter.fromRgb`.
pub(crate) fn hsl_of(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    const EPSILON: f64 = 1.0e-6;
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let l = (min + max) / 2.0;
    if l.abs() <= EPSILON {
        return (1.0, 0.0, l);
    }
    let mut s = max - min;
    if s.abs() <= EPSILON {
        return (1.0, s, l);
    }
    s /= if l <= 0.5 { min + max } else { 2.0 - max - min };
    let h = if (r - max).abs() < EPSILON {
        if g == min { 5.0 + (max - b) / (max - min) } else { 1.0 - (max - g) / (max - min) }
    } else if (g - max).abs() < EPSILON {
        if b == min { 1.0 + (max - r) / (max - min) } else { 3.0 - (max - b) / (max - min) }
    } else if r == min {
        3.0 + (max - g) / (max - min)
    } else {
        5.0 - (max - r) / (max - min)
    };
    (h / 6.0, s, l)
}

/// `GammaCorrectionFilter.HSLRGBConverter.fromHsl` (unrounded RGB 0..1; its fallback case
/// differs from `HSLTransformer`).
pub(crate) fn converter_from_hsl(hue: f64, saturation: f64, luminosity: f64) -> (f64, f64, f64) {
    let (hue, saturation, luminosity) = (hue.clamp(0.0, 1.0), saturation.clamp(0.0, 1.0), luminosity.clamp(0.0, 1.0));
    let v = if luminosity <= 0.5 { luminosity * (1.0 + saturation) } else { luminosity + saturation - luminosity * saturation };
    if v <= 0.0 {
        return (0.0, 0.0, 0.0);
    }
    let hue = (hue * 6.0).clamp(0.0, 6.0);
    let y = luminosity + luminosity - v;
    let frac = hue - hue.trunc();
    let x = y + (v - y) * frac;
    let z = v - (v - y) * frac;
    match hue as i32 {
        0 => (v, x, y),
        1 => (z, v, y),
        2 => (y, v, x),
        3 => (y, z, v),
        4 => (x, y, v),
        5 => (v, y, z),
        _ => (v, x, y),
    }
}

/// A key color; curve key frames carry their positions on the hue, saturation and
/// luminosity curves (`RGBColorKeyFrame`).
#[derive(Clone, Copy, Debug)]
enum KeyFrame {
    Plain([u8; 3]),
    Curve { hsl: (f64, f64, f64), x: [i32; 3] },
}

impl KeyFrame {
    fn rgb(&self) -> [u8; 3] {
        match self {
            KeyFrame::Plain(c) => *c,
            KeyFrame::Curve { hsl, .. } => {
                let (r, g, b) = converter_from_hsl(hsl.0, hsl.1, hsl.2);
                [round_color(r * 255.0), round_color(g * 255.0), round_color(b * 255.0)]
            }
        }
    }
}

/// The random gradient generators (`RandomGradientGeneratorList`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GradientGenerator {
    #[default]
    All,
    Bold,
    Monochrome,
    Smooth,
    Stripes,
    StrongHue,
    TwoColors,
    UnevenCurves,
    UniformCurves,
}

impl GradientGenerator {
    pub const ALL: &'static [GradientGenerator] = &[
        GradientGenerator::All,
        GradientGenerator::Bold,
        GradientGenerator::Monochrome,
        GradientGenerator::Smooth,
        GradientGenerator::Stripes,
        GradientGenerator::StrongHue,
        GradientGenerator::TwoColors,
        GradientGenerator::UnevenCurves,
        GradientGenerator::UniformCurves,
    ];

    pub fn by_label(label: &str) -> Option<GradientGenerator> {
        GradientGenerator::ALL.iter().copied().find(|g| g.label().eq_ignore_ascii_case(label))
    }

    pub fn label(self) -> &'static str {
        match self {
            GradientGenerator::All => "(All)",
            GradientGenerator::Bold => "Bold",
            GradientGenerator::Monochrome => "Monochrome",
            GradientGenerator::Smooth => "Smooth",
            GradientGenerator::Stripes => "Stripes",
            GradientGenerator::StrongHue => "Strong hue",
            GradientGenerator::TwoColors => "Two colors",
            GradientGenerator::UnevenCurves => "Uneven curves",
            GradientGenerator::UniformCurves => "Uniform curves",
        }
    }

    /// `generateKeyFrames`.
    fn key_frames(self, n: usize, r: &mut Rng) -> Vec<KeyFrame> {
        let plain = |v: Vec<[u8; 3]>| v.into_iter().map(KeyFrame::Plain).collect();
        match self {
            GradientGenerator::All => {
                // `AllRandomGradientGenerator`: duplicates weight the choice.
                use GradientGenerator::*;
                const LIST: [GradientGenerator; 26] = [
                    StrongHue,
                    StrongHue,
                    UnevenCurves,
                    UnevenCurves,
                    UnevenCurves,
                    StrongHue,
                    StrongHue,
                    UnevenCurves,
                    UniformCurves,
                    Stripes,
                    Stripes,
                    UnevenCurves,
                    UnevenCurves,
                    UnevenCurves,
                    Monochrome,
                    Smooth,
                    Bold,
                    UnevenCurves,
                    Bold,
                    UniformCurves,
                    UnevenCurves,
                    UniformCurves,
                    UnevenCurves,
                    UnevenCurves,
                    Bold,
                    TwoColors,
                ];
                LIST[((r.random() * LIST.len() as f64) as usize).min(LIST.len() - 1)].key_frames(n, r)
            }
            GradientGenerator::StrongHue => plain(strong_hue(n, r)),
            GradientGenerator::TwoColors => plain(strong_hue(2, r)),
            GradientGenerator::Smooth => plain(smooth(n, r)),
            GradientGenerator::Monochrome => {
                let s = r.random() * 0.3 + 0.6999;
                let h = r.random() * r.random();
                plain((0..n).map(|_| hsl2rgb(h, s, r.random() * 0.99)).collect())
            }
            GradientGenerator::Stripes => plain(stripes(n, r)),
            GradientGenerator::Bold => plain(bold(r)),
            GradientGenerator::UniformCurves => {
                let n = if n > 24 && r.random() > 0.25 { n / 2 } else { n };
                let mut out = vec![curve_key_frame([0; 3], r, 0.125, 0.0625)];
                if n > 2 {
                    let dx = 255.0 / (n - 1) as f64;
                    let mut x = 0.0;
                    for _ in 1..n - 1 {
                        x += dx * (0.25 + r.random() * 1.25);
                        if x > 254.0 - 1.0e-6 {
                            break;
                        }
                        let p = ftoi(x);
                        out.push(curve_key_frame([p; 3], r, 0.125, 0.0625));
                    }
                }
                out.push(curve_key_frame([255; 3], r, 0.125, 0.0625));
                out
            }
            GradientGenerator::UnevenCurves => {
                let n = if n > 18 && r.random() > 0.35 { n / 2 } else { n };
                let mut out = vec![curve_key_frame([0; 3], r, 0.10, 0.05)];
                if n > 2 {
                    let dx = 255.0 / (n - 1) as f64;
                    let mut xs = [0.0f64; 3];
                    'outer: for _ in 1..n - 1 {
                        for x in &mut xs {
                            *x += dx * (0.25 + r.random() * (0.75 + r.random() * 0.35));
                            if *x > 254.0 - 1.0e-6 {
                                break 'outer;
                            }
                        }
                        let p = xs.map(ftoi);
                        out.push(curve_key_frame(p, r, 0.10, 0.05));
                    }
                }
                out.push(curve_key_frame([255; 3], r, 0.10, 0.05));
                out
            }
        }
    }

    /// `generatePalette(keyFrameCount, fadeColors, uniformWidth)`.
    pub fn generate(self, key_frames: usize, fade: bool, uniform: bool, r: &mut Rng) -> Gradient {
        let kf = self.key_frames(key_frames.max(1), r);
        palette(&kf, fade, uniform, r)
    }
}

/// `StrongHueRandomGradientGenerator.generateKeyFrames`.
fn strong_hue(n: usize, r: &mut Rng) -> Vec<[u8; 3]> {
    let s = r.random() * 0.1 + 0.8999;
    (0..n)
        .map(|_| {
            let l = r.random() * 0.90;
            let h = strong_hue_value(r);
            hsl2rgb(h, s, l)
        })
        .collect()
}

/// The hue distribution of the "strong hue" colors.
fn strong_hue_value(r: &mut Rng) -> f64 {
    let rnd = r.random() + r.random() + r.random() + r.random() + r.random();
    if rnd > 4.0 {
        0.8 + (rnd - 4.0) / 7.0
    } else if rnd > 3.0 {
        0.2 + (rnd - 3.0) / 7.0
    } else if rnd > 2.0 {
        (rnd - 2.0) / 7.0
    } else if rnd > 1.0 {
        0.6 + (rnd - 1.0) / 7.0
    } else {
        0.4 + rnd / 7.0
    }
}

/// `SmoothRandomGradientGenerator.generateKeyFrames`.
fn smooth(n: usize, r: &mut Rng) -> Vec<[u8; 3]> {
    let mut last = [0i32; 3];
    (0..n)
        .map(|_| {
            loop {
                let c = [round_color(256.0 * r.random()), round_color(256.0 * r.random()), round_color(256.0 * r.random())];
                let d = (c[0] as i32 - last[0]).abs() as f64 * 0.299
                    + (c[1] as i32 - last[1]).abs() as f64 * 0.588
                    + (c[2] as i32 - last[2]).abs() as f64 * 0.1130;
                if d > 66.0 {
                    last = c.map(|v| v as i32);
                    break c;
                }
            }
        })
        .collect()
}

/// `StripesRandomGradientGenerator.generateKeyFrames`: thin light stripes between blends.
fn stripes(n: usize, r: &mut Rng) -> Vec<[u8; 3]> {
    let stripe = {
        let s = r.random() * 0.1 + 0.8999;
        let h = strong_hue_value(r);
        let l = r.random() * 0.1 + 0.8999;
        hsl2rgb(h, s, l)
    };
    let base = strong_hue(n.max(1), r);
    let stripe_width = 1 + r.random_int(2);
    let color_width = 3 + r.random_int(5);
    let mut out = Vec::new();
    let mut idx = 0;
    while out.len() < PALETTE_SIZE {
        out.extend(std::iter::repeat_n(stripe, stripe_width));
        let left = base[idx];
        idx = (idx + 1) % base.len();
        let right = base[idx];
        idx = (idx + 1) % base.len();
        out.push(left);
        for i in 1..color_width {
            let scl = i as f64 / (color_width - 1) as f64;
            out.push([0, 1, 2].map(|c| round_color(left[c] as f64 + (right[c] as f64 - left[c] as f64) * scl)));
        }
    }
    out.truncate(PALETTE_SIZE - 1);
    out
}

/// `BoldRandomGradientGenerator.generateKeyFrames`: wide stripes, sometimes black gaps.
fn bold(r: &mut Rng) -> Vec<[u8; 3]> {
    let base = strong_hue(128, r);
    let stripe_width = 12 + r.random_int(24);
    let black_width = if r.random() < 0.25 { 8 + r.random_int(16) } else { 0 };
    let mut out = Vec::new();
    let (mut stripe_idx, mut color_idx) = (0usize, 0usize);
    while out.len() < PALETTE_SIZE {
        let colored = stripe_idx.is_multiple_of(2) || black_width == 0;
        stripe_idx += 1;
        if colored {
            out.extend(std::iter::repeat_n(base[color_idx % base.len()], stripe_width));
            color_idx += 1;
        } else {
            out.extend(std::iter::repeat_n([0, 0, 0], black_width));
        }
    }
    out.truncate(PALETTE_SIZE - 1);
    out
}

/// A curve key frame with a random color (`createRandomKeyFrame` of the curve generators):
/// mostly strong hue, else smooth or a stripe color.
fn curve_key_frame(x: [i32; 3], r: &mut Rng, strong: f64, smooth_below: f64) -> KeyFrame {
    let rnd = r.random();
    let c = if rnd > strong {
        strong_hue(1, r)[0]
    } else if rnd < smooth_below {
        smooth(1, r)[0]
    } else {
        stripes(1, r)[0]
    };
    KeyFrame::Curve { hsl: hsl_of(c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0), x }
}

/// `RandomGradientGenerator.generatePalette(keyFrames, fade, uniform)`.
fn palette(kf: &[KeyFrame], fade: bool, uniform: bool, r: &mut Rng) -> Gradient {
    if let Some(KeyFrame::Curve { .. }) = kf.first() {
        return palette_from_curves(kf);
    }
    let colors: Vec<[u8; 3]> = kf.iter().map(KeyFrame::rgb).collect();
    classic_palette(&colors, fade, uniform, r)
}

/// `generateClassicPalette`: key colors with (random) widths, optionally blended.
pub fn classic_palette(colors: &[[u8; 3]], fade: bool, uniform: bool, r: &mut Rng) -> Gradient {
    if colors.len() <= 1 {
        return Gradient::from_colors(vec![colors.first().copied().unwrap_or([0, 0, 0]); PALETTE_SIZE]);
    }
    let n = colors.len() - usize::from(fade);
    let idx_scl = PALETTE_SIZE as f64 / n as f64;
    let mut count = vec![0i32; colors.len()];
    let (mut sum1, mut sum2) = (0i32, 0.0f64);
    for c in count.iter_mut().take(n) {
        *c = idx_scl as i32;
        sum1 += *c;
        sum2 += idx_scl;
        if sum2 - sum1 as f64 > 1.0 {
            *c += 1;
            sum1 += 1;
        }
    }
    count[n - 1] += PALETTE_SIZE as i32 - sum1;
    if !uniform && idx_scl > 3.0 {
        for i in 0..n {
            let j = ((r.random() * n as f64) as usize).min(n - 1);
            let min = count[i].min(count[j]);
            let d = (r.random() * min as f64 - (min / 2) as f64) as i32;
            count[i] += d;
            count[j] -= d;
        }
    }
    let mut out = Vec::with_capacity(PALETTE_SIZE);
    let (mut j, mut l) = (0i32, 0usize);
    for _ in 0..PALETTE_SIZE {
        let lc = colors[l];
        let c = if fade {
            let rel = j as f64 / count[l] as f64;
            let rc = colors[l + 1];
            [0, 1, 2].map(|k| round_color(lc[k] as f64 + (rc[k] as f64 - lc[k] as f64) * rel))
        } else {
            lc
        };
        out.push(c);
        j += 1;
        if j >= count[l] {
            j = 0;
            if l < n - 1 {
                l += 1;
            }
        }
    }
    Gradient::from_colors(out)
}

/// `createGradientByColorCurve` / `Layer.refreshGradientFromCurves`: hue, saturation and
/// luminosity as spline curves over the gradient.
fn palette_from_curves(kf: &[KeyFrame]) -> Gradient {
    let curve = |pick: fn(&KeyFrame) -> (i32, f64)| {
        let mut c = Curve::standard(true);
        (c.x, c.y) = kf.iter().map(pick).unzip();
        Envelope::new(&c)
    };
    let hue = curve(|k| match k {
        KeyFrame::Curve { hsl, x } => (x[0], hsl.0 * 255.0),
        KeyFrame::Plain(_) => (0, 0.0),
    });
    let sat = curve(|k| match k {
        KeyFrame::Curve { hsl, x } => (x[1], hsl.1 * 255.0),
        KeyFrame::Plain(_) => (0, 0.0),
    });
    let lum = curve(|k| match k {
        KeyFrame::Curve { hsl, x } => (x[2], hsl.2 * 255.0),
        KeyFrame::Plain(_) => (0, 0.0),
    });
    let colors = (0..PALETTE_SIZE)
        .map(|i| {
            let i = i as f64;
            let (r, g, b) = converter_from_hsl(hue.evaluate(i) / 255.0, sat.evaluate(i) / 255.0, lum.evaluate(i) / 255.0);
            [r, g, b].map(|v| ftoi(v * 255.0).clamp(0, 255) as u8)
        })
        .collect();
    Gradient::from_colors(colors)
}

/// A color with its HSL values (`SimilarGradientCreator.GradientColor`).
type ClusterColor = ([u8; 3], (f64, f64, f64));

/// `RandomGradientMutation`: a gradient of the "(All)" generator with random settings.
pub fn random_gradient(r: &mut Rng) -> Gradient {
    random_gradient_of(GradientGenerator::All, r)
}

/// A gradient of `generator` with random settings (the sampler's `palettePoints`,
/// `fadePaletteColors` and `uniformWidth`).
pub fn random_gradient_of(generator: GradientGenerator, r: &mut Rng) -> Gradient {
    let key_frames = 3 + r.random_int(56);
    let fade = r.random() > 0.33;
    let uniform = r.random() > 0.75;
    generator.generate(key_frames, fade, uniform, r)
}

/// `SimilarGradientMutation` / `SimilarGradientCreator`: clusters the colors of a gradient
/// by HSL and builds a new gradient from random members of the clusters.
pub fn similar_gradient(g: &Gradient, r: &mut Rng) -> Gradient {
    let key_frames = 20 + (r.random() * 10.0 + 0.5) as usize;
    let weighted = 3 + (r.random() * 3.0 + 0.5) as usize;
    let mut clusters: Vec<Vec<ClusterColor>> = g.colors.iter().map(|&c| vec![(c, rgb2hsl(c))]).collect();
    let avg = |cl: &[ClusterColor]| {
        let n = cl.len() as f64;
        cl.iter().fold((0.0, 0.0, 0.0), |a, (_, h)| (a.0 + h.0, a.1 + h.1, a.2 + h.2)).map3(|v| v / n)
    };
    while clusters.len() > key_frames {
        let avgs: Vec<(f64, f64, f64)> = clusters.iter().map(|c| avg(c)).collect();
        // JWildfire measures "closeness" with the product of the components.
        let (mut best, mut bi, mut bj) = (f64::MAX, 0, 1);
        for (i, a) in avgs.iter().enumerate() {
            for (j, b) in avgs.iter().enumerate() {
                if i != j {
                    let d = (a.0 * b.0 + a.1 * b.1 + a.2 * b.2).sqrt();
                    if d < best {
                        (best, bi, bj) = (d, i, j);
                    }
                }
            }
        }
        let mut merged = clusters[bi].clone();
        merged.extend(clusters[bj].iter().copied());
        let (hi, lo) = if bi > bj { (bi, bj) } else { (bj, bi) };
        clusters.remove(hi);
        clusters.remove(lo);
        clusters.push(merged);
    }
    let max_sub = clusters.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let mut out: Vec<[u8; 3]> = Vec::new();
    for sub in &mut clusters {
        let count = (weighted as f64 * sub.len() as f64 / max_sub as f64 + 0.5) as usize;
        for _ in 0..count {
            if sub.is_empty() {
                break;
            }
            let idx = ((r.random() * sub.len() as f64) as usize).min(sub.len() - 1);
            let (c, (h, s, l)) = sub[idx];
            out.push(if r.random() < 0.125 {
                hsl2rgb(r.random(), s, l)
            } else if r.random() < 0.25 {
                hsl2rgb(h, s, r.random())
            } else {
                c
            });
            sub.remove(idx);
        }
    }
    if r.random() < 0.25 {
        for i in (1..out.len()).rev() {
            out.swap(i, r.random_int(i + 1));
        }
    }
    while out.len() > 160 {
        let idx = ((r.random() * out.len() as f64) as usize).min(out.len() - 1);
        out.remove(idx);
    }
    classic_palette(&out, true, false, r)
}

trait Map3 {
    fn map3(self, f: impl Fn(f64) -> f64) -> Self;
}

impl Map3 for (f64, f64, f64) {
    fn map3(self, f: impl Fn(f64) -> f64) -> Self {
        (f(self.0), f(self.1), f(self.2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsl_round_trip() {
        // Hue exactly 1.0 (pure red) comes back as magenta, as in JWildfire's HSLTransformer.
        assert_eq!(hsl2rgb(1.0, 1.0, 0.5), [255, 0, 255]);
        for c in [[200u8, 100, 20], [12, 200, 77], [255, 255, 255], [90, 90, 200]] {
            let (h, s, l) = rgb2hsl(c);
            let back = hsl2rgb(h, s, l);
            assert!(back.iter().zip(c).all(|(a, b)| (*a as i32 - b as i32).abs() <= 1), "{c:?} -> {back:?}");
        }
    }

    #[test]
    fn generators_fill_the_palette() {
        let mut r = Rng::new(9);
        for &g in GradientGenerator::ALL {
            for (fade, uniform) in [(true, false), (false, true)] {
                let p = g.generate(12, fade, uniform, &mut r);
                assert_eq!(p.colors.len(), PALETTE_SIZE, "{}", g.label());
            }
        }
        let p = random_gradient(&mut r);
        assert_eq!(similar_gradient(&p, &mut r).colors.len(), PALETTE_SIZE);
    }
}
