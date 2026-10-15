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

//! JWildfire's gradient modifications (`RGBPalette.transformColors` with the
//! `BalancingTransformer`, `HSLTransformer` and `SwapRGBTransformer`) and the operations on a
//! range of gradient entries (`RGBPalette` / `GradientOverlay`).

use crate::gradient::Gradient;
use crate::math::round_color;

/// `Tools.VPREC` / `Tools.SPREC`: fixed point precision of the balancing transformer.
const VPREC: i32 = 1024;
const SPREC: i32 = 10;
const EPS: f64 = 1.0e-8;

/// The `mod*` values of a JWildfire gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GradientMods {
    /// Entries the colors move up (-255..255).
    pub shift: i32,
    /// Repeats of the gradient (1 = once).
    pub frequency: i32,
    /// Blur radius in entries.
    pub blur: i32,
    pub red: i32,
    pub green: i32,
    pub blue: i32,
    pub brightness: i32,
    pub contrast: i32,
    pub gamma: i32,
    pub saturation: i32,
    pub hue: i32,
    /// Channel order: 0 = unchanged, then RBG, GRB, GBR, BRG, BGR over 0..255.
    pub swap_rgb: i32,
}

impl Default for GradientMods {
    fn default() -> Self {
        Self { shift: 0, frequency: 1, blur: 0, red: 0, green: 0, blue: 0, brightness: 0, contrast: 0, gamma: 0, saturation: 0, hue: 0, swap_rgb: 0 }
    }
}

/// `HSLTransformer.rgb2hsl` (hue, saturation, luminosity 0..1).
pub fn jwf_rgb2hsl(c: [i32; 3]) -> (f64, f64, f64) {
    let (r, g, b) = (c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0);
    let (mut hue, mut sat) = (1.0, 0.0);
    let max = r.max(g.max(b));
    let min = r.min(g.min(b));
    let lum = (min + max) / 2.0;
    if lum.abs() <= EPS {
        return (hue, sat, lum);
    }
    sat = max - min;
    if sat.abs() <= EPS {
        return (hue, sat, lum);
    }
    sat /= if lum <= 0.5 { min + max } else { 2.0 - max - min };
    hue = if (r - max).abs() < EPS {
        if g == min { 5.0 + (max - b) / (max - min) } else { 1.0 - (max - g) / (max - min) }
    } else if (g - max).abs() < EPS {
        if b == min { 1.0 + (max - r) / (max - min) } else { 3.0 - (max - b) / (max - min) }
    } else if r == min {
        3.0 + (max - g) / (max - min)
    } else {
        5.0 - (max - r) / (max - min)
    };
    (hue / 6.0, sat, lum)
}

/// `HSLTransformer.hsl2rgb`.
pub fn jwf_hsl2rgb(hue: f64, sat: f64, lum: f64) -> [i32; 3] {
    let v = if lum <= 0.5 { lum * (1.0 + sat) } else { lum + sat - lum * sat };
    if v <= 0.0 {
        return [0, 0, 0];
    }
    let hue = (hue * 6.0).clamp(0.0, 6.0);
    let y = lum + lum - v;
    let f = hue - (hue as i32) as f64;
    let x = y + (v - y) * f;
    let z = v - (v - y) * f;
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

/// `BalancingTransformer`, applied to one color.
fn balance(c: &mut [i32; 3], m: &GradientMods) {
    const BR_MAX: f64 = 256.0;
    let clamp = |v: i32| v.clamp(0, 255);
    if m.saturation != 0 {
        let (rs, gs, bs) = (2990 * VPREC / 10000, 5880 * VPREC / 10000, 1130 * VPREC / 10000);
        let scl = (m.saturation as f64 / 255.0 * 1024.0 + 0.5) as i32;
        let avg = (rs * c[0] + gs * c[1] + bs * c[2]) >> SPREC;
        for v in c.iter_mut() {
            *v = clamp(*v + (((*v - avg) * scl) >> SPREC));
        }
    }
    if m.gamma != 0 {
        let g = 512.0 / (512.0 + m.gamma as f64);
        let mut table = [0i32; 256];
        let da: f64 = 1.0 / 255.0;
        let mut aa = da;
        for t in table.iter_mut().skip(1) {
            *t = (255.0 * aa.powf(g) + 0.5) as i32;
            aa += da;
        }
        for v in c.iter_mut() {
            *v = table[*v as usize];
        }
    }
    let add = |c: &mut i32, amount: i32| {
        let tt = (amount as f64 / BR_MAX * 255.0 + 0.5) as i32;
        *c = clamp(*c + tt);
    };
    if m.red != 0 {
        add(&mut c[0], m.red);
    }
    if m.green != 0 {
        add(&mut c[1], m.green);
    }
    if m.blue != 0 {
        add(&mut c[2], m.blue);
    }
    if m.brightness != 0 {
        for v in c.iter_mut() {
            add(v, m.brightness);
        }
    }
    if m.contrast != 0 {
        let sc = (m.contrast as f64 / BR_MAX * VPREC as f64 + 0.5) as i32;
        for v in c.iter_mut() {
            let dc = (((*v - 127) * sc) >> SPREC).clamp(-255, 255);
            if m.contrast > 0 {
                *v = clamp(*v + dc);
            } else {
                let val = *v + dc;
                *v = if *v < 127 { val.min(127) } else { val.max(127) };
            }
        }
    }
}

impl Gradient {
    /// The gradient with JWildfire's modifications applied (`RGBPalette.transformColors`).
    pub fn with_mods(&self, m: &GradientMods) -> Gradient {
        let n = self.colors.len();
        if n == 0 {
            return self.clone();
        }
        let mut t: Vec<[i32; 3]> = vec![[0; 3]; n];
        for (i, c) in self.colors.iter().enumerate() {
            let mut idx = i as i32 + m.shift % 255;
            if idx < 0 {
                idx += n as i32;
            } else if idx >= n as i32 {
                idx -= n as i32;
            }
            t[idx.clamp(0, n as i32 - 1) as usize] = [c[0] as i32, c[1] as i32, c[2] as i32];
        }
        if m.frequency > 1 {
            let f = m.frequency as usize;
            let k = n / f;
            let src = t.clone();
            for j in 0..f {
                for i in 0..k {
                    let idx = i + j * k;
                    if idx < n && i * f < n {
                        t[idx] = src[i * f];
                    }
                }
            }
        }
        if m.blur > 0 {
            let b = m.blur as i64;
            let src = t.clone();
            for i in 0..n as i64 {
                let mut sum = [0i32; 3];
                let mut cnt = -1;
                for j in i - b..=i + b {
                    cnt += 1;
                    let k = (n as i64 + j).rem_euclid(n as i64);
                    if k != i {
                        for ch in 0..3 {
                            sum[ch] += src[k as usize][ch];
                        }
                    }
                }
                if cnt != 0 {
                    t[i as usize] = sum.map(|s| (s / cnt).clamp(0, 255));
                }
            }
        }
        let balancing = m.red != 0 || m.green != 0 || m.blue != 0 || m.contrast != 0 || m.gamma != 0 || m.brightness != 0 || m.saturation != 0;
        let phue = (m.hue as f64 / 255.0).clamp(-1.0, 1.0);
        let modes: [[usize; 3]; 5] = [[0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        let swap = (m.swap_rgb != 0).then(|| modes[((m.swap_rgb.abs() as f64 / 255.0 * 4.0) as usize).min(4)]);
        let colors = t
            .into_iter()
            .map(|mut c| {
                if balancing {
                    balance(&mut c, m);
                }
                if m.hue != 0 {
                    let (mut h, s, l) = jwf_rgb2hsl(c);
                    h += phue;
                    if h < 0.0 {
                        h += 1.0;
                    } else if h > 1.0 {
                        h -= 1.0;
                    }
                    c = jwf_hsl2rgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
                }
                if let Some(o) = swap {
                    c = [c[o[0]], c[o[1]], c[o[2]]];
                }
                c.map(|v| v.clamp(0, 255) as u8)
            })
            .collect();
        Gradient { colors }
    }

    fn range(&self, from: usize, to: usize) -> Option<(usize, usize)> {
        let n = self.colors.len();
        (n > 0 && from <= to).then(|| (from.min(n - 1), to.min(n - 1)))
    }

    /// Sorts the colors of `from..=to` by hue (in 3.5 steps), then value
    /// (`RGBColor.compareToRGBColor`).
    pub fn sort_range(&mut self, from: usize, to: usize) {
        let Some((a, b)) = self.range(from, to) else { return };
        let key = |c: &[u8; 3]| {
            let (h, v) = hue_value(*c);
            (crate::math::ftoi(3.5 * h), v)
        };
        self.colors[a..=b].sort_by(|x, y| {
            let (kx, ky) = (key(x), key(y));
            kx.0.cmp(&ky.0).then(kx.1.partial_cmp(&ky.1).unwrap_or(std::cmp::Ordering::Equal))
        });
    }

    pub fn negative_range(&mut self, from: usize, to: usize) {
        let Some((a, b)) = self.range(from, to) else { return };
        for c in &mut self.colors[a..=b] {
            *c = c.map(|v| 255 - v);
        }
    }

    pub fn reverse_range(&mut self, from: usize, to: usize) {
        let Some((a, b)) = self.range(from, to) else { return };
        self.colors[a..=b].reverse();
    }

    /// Straight color transition from entry `from` to entry `to` (`RGBPalette.fadeRange`).
    pub fn fade_range(&mut self, from: usize, to: usize) {
        let Some((a, b)) = self.range(from, to) else { return };
        if b <= a {
            return;
        }
        let (l, r) = (self.colors[a], self.colors[b]);
        let scl = 1.0 / (b - a) as f64;
        for i in a + 1..b {
            let x = scl * (i - a) as f64;
            self.colors[i] = [0, 1, 2].map(|k| round_color(l[k] as f64 + (r[k] as f64 - l[k] as f64) * x) as u8);
        }
    }

    /// Gives `from..=to` their average hue (`RGBPalette.monochrome`).
    pub fn monochrome_range(&mut self, from: usize, to: usize) {
        let Some((a, b)) = self.range(from, to) else { return };
        let as_i = |c: [u8; 3]| c.map(|v| v as i32);
        let avg = self.colors[a..=b].iter().map(|c| jwf_rgb2hsl(as_i(*c)).0).sum::<f64>() / (b - a + 1) as f64;
        for c in &mut self.colors[a..=b] {
            let (_, s, l) = jwf_rgb2hsl(as_i(*c));
            *c = jwf_hsl2rgb(avg, s, l).map(|v| v as u8);
        }
    }

    /// Black in `from..=to` (`GradientOverlay.eraseRange`).
    pub fn erase_range(&mut self, from: usize, to: usize) {
        let Some((a, b)) = self.range(from, to) else { return };
        self.colors[a..=b].fill([0, 0, 0]);
    }

    /// The colors of `from..=to`.
    pub fn copy_range(&self, from: usize, to: usize) -> Vec<[u8; 3]> {
        self.range(from, to).map_or_else(Vec::new, |(a, b)| self.colors[a..=b].to_vec())
    }

    /// `clip` into `from..=to` (as far as both reach).
    pub fn paste_range(&mut self, from: usize, to: usize, clip: &[[u8; 3]]) {
        let Some((a, b)) = self.range(from, to) else { return };
        for (dst, src) in self.colors[a..=b].iter_mut().zip(clip) {
            *dst = *src;
        }
    }

    /// Fades between the non-black colors, filling the black gaps (`GradientOverlay.fadeAll`).
    pub fn fade_all(&mut self) {
        let n = self.colors.len();
        let mut start = 0;
        for i in 0..n {
            let c = self.colors[i];
            if (c != [0, 0, 0] || i == n - 1) && start < i {
                self.fade_range(start, i);
                start = i;
            }
        }
    }
}

/// `RGBColor.toHSV`: hue and value.
fn hue_value(c: [u8; 3]) -> (f64, f64) {
    let (r, g, b) = (c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0);
    let min = r.min(g).min(b);
    let max = r.max(g).max(b);
    let delta = max - min;
    if delta.abs() <= EPS {
        return (0.0, max);
    }
    let dr = ((max - r) / 6.0 + delta / 2.0) / delta;
    let dg = ((max - g) / 6.0 + delta / 2.0) / delta;
    let db = ((max - b) / 6.0 + delta / 2.0) / delta;
    let mut h = if (r - max).abs() < EPS {
        db - dg
    } else if (g - max).abs() < EPS {
        1.0 / 3.0 + dr - db
    } else {
        2.0 / 3.0 + dg - dr
    };
    if h < -EPS {
        h += 1.0;
    }
    if h > 1.0 + EPS {
        h -= 1.0;
    }
    (h, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Gradient {
        Gradient { colors: (0..256).map(|i| [i as u8, (255 - i) as u8, 128]).collect() }
    }

    #[test]
    fn mods_follow_jwildfire() {
        let g = ramp();
        assert_eq!(g.with_mods(&GradientMods::default()), g);
        // shift moves colors up
        let s = g.with_mods(&GradientMods { shift: 3, ..Default::default() });
        assert_eq!(s.colors[3], g.colors[0]);
        // brightness +128: tt = (int)(128 / 256 * 255 + 0.5) = 128
        let b = g.with_mods(&GradientMods { brightness: 128, ..Default::default() });
        assert_eq!(b.colors[10], [138, 255, 255]);
        // swap 255 -> BGR
        let w = g.with_mods(&GradientMods { swap_rgb: 255, ..Default::default() });
        assert_eq!(w.colors[0], [128, 255, 0]);
        // frequency 2: the gradient twice
        let f = g.with_mods(&GradientMods { frequency: 2, ..Default::default() });
        assert_eq!((f.colors[1], f.colors[129]), (g.colors[2], g.colors[2]));
        // hue 0 keeps, full turn returns (rounding aside)
        let h = g.with_mods(&GradientMods { hue: 255, ..Default::default() });
        for (a, b) in h.colors.iter().zip(&g.colors) {
            assert!(a.iter().zip(b).all(|(x, y)| (*x as i32 - *y as i32).abs() <= 1), "{a:?} {b:?}");
        }
        // negative contrast pulls towards 127 without crossing it
        let c = g.with_mods(&GradientMods { contrast: -255, ..Default::default() });
        assert!(c.colors.iter().all(|c| (126..=128).contains(&c[2])));
    }

    #[test]
    fn hsl_round_trip() {
        // JWildfire's quirk: pure red gets hue 1.0, which hsl2rgb reads as magenta
        assert_eq!(jwf_rgb2hsl([255, 0, 0]).0, 1.0);
        for c in [[12, 200, 99], [128, 128, 128], [0, 0, 0], [250, 250, 10], [200, 30, 40]] {
            let (h, s, l) = jwf_rgb2hsl(c);
            let back = jwf_hsl2rgb(h, s, l);
            assert!(back.iter().zip(&c).all(|(x, y)| (x - y).abs() <= 1), "{c:?} -> {back:?}");
        }
    }

    #[test]
    fn range_operations() {
        let mut g = ramp();
        let clip = g.copy_range(0, 9);
        g.erase_range(10, 19);
        assert_eq!(g.colors[15], [0, 0, 0]);
        g.paste_range(10, 19, &clip);
        assert_eq!(g.colors[10], clip[0]);
        g.negative_range(0, 0);
        assert_eq!(g.colors[0], [255, 0, 127]);
        g.reverse_range(0, 9);
        assert_eq!(g.colors[9], [255, 0, 127]);
        let mut e = Gradient { colors: vec![[0, 0, 0]; 256] };
        e.colors[0] = [255, 0, 0];
        e.colors[255] = [0, 0, 255];
        e.fade_all();
        assert_eq!(e.colors[128][0], 127);
        e.sort_range(0, 255);
        e.monochrome_range(0, 255);
    }
}

/// JWildfire's gradient curve editor (`Layer.refreshGradientFromCurves`): hue, saturation and
/// luminosity (0..255) as spline curves over the 256 gradient entries.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientCurves {
    /// Keys `(entry, value)` of the hue, saturation and luminosity curves.
    pub curves: [Vec<(f64, f64)>; 3],
}

/// Points per curve of a new curve set (`DFLT_CURVE_POINTS`).
pub const CURVE_POINTS: usize = 12;

impl GradientCurves {
    /// Curves through the hue, saturation and luminosity of `g` at 12 evenly spaced entries.
    pub fn from_gradient(g: &Gradient) -> Self {
        let n = g.colors.len().max(1);
        let dx = 255.0 / (CURVE_POINTS - 1) as f64;
        let mut curves: [Vec<(f64, f64)>; 3] = Default::default();
        for i in 0..CURVE_POINTS {
            let x = if i == CURVE_POINTS - 1 { 255.0 } else { crate::math::ftoi(i as f64 * dx) as f64 };
            let c = g.colors.get(((x as usize) * n / 256).min(n - 1)).copied().unwrap_or([0; 3]);
            let (h, s, l) = jwf_rgb2hsl(c.map(|v| v as i32));
            // a gray's hue 1.0 means "none": 0 keeps the curve calm
            let h = if s.abs() <= EPS { 0.0 } else { h };
            for (k, v) in [h, s, l].into_iter().enumerate() {
                curves[k].push((x, (v * 255.0).clamp(0.0, 255.0)));
            }
        }
        Self { curves }
    }

    /// The gradient of the curves (spline curves as JWildfire's, `HSLRGBConverter.fromHsl`).
    pub fn to_gradient(&self) -> Gradient {
        use crate::animation::{Interp, Key, Track};
        let tracks: Vec<Track> = self
            .curves
            .iter()
            .map(|c| {
                let mut keys: Vec<Key> = c.iter().map(|&(frame, value)| Key { frame, value }).collect();
                keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
                Track { path: String::new(), interp: Interp::Spline, keys }
            })
            .collect();
        let colors = (0..256)
            .map(|i| {
                let v = |k: usize| tracks[k].eval(i as f64).unwrap_or(0.0) / 255.0;
                let rgb = curves_hsl_to_rgb(v(0), v(1), v(2));
                rgb.map(|c| (crate::math::ftoi(c * 255.0)).clamp(0, 255) as u8)
            })
            .collect();
        Gradient { colors }
    }

    /// Reads `h x,y x,y ...|s ...|l ...` (RWildFire's `gradient_curves` attribute).
    pub fn parse(s: &str) -> Option<Self> {
        let mut curves: [Vec<(f64, f64)>; 3] = Default::default();
        for (k, part) in s.split('|').enumerate().take(3) {
            for p in part.split_whitespace().skip(1) {
                let (x, y) = p.split_once(',')?;
                curves[k].push((x.parse().ok()?, y.parse().ok()?));
            }
        }
        curves.iter().all(|c| c.len() >= 2).then_some(Self { curves })
    }

    pub fn to_attr(&self) -> String {
        let f = crate::io::fmt_f;
        ["h", "s", "l"]
            .iter()
            .zip(&self.curves)
            .map(|(n, c)| format!("{n} {}", c.iter().map(|(x, y)| format!("{},{}", f(*x), f(*y))).collect::<Vec<_>>().join(" ")))
            .collect::<Vec<_>>()
            .join("|")
    }
}

/// `HSLRGBConverter.fromHsl` (its fallback case differs from `HSLTransformer`'s).
fn curves_hsl_to_rgb(hue: f64, sat: f64, lum: f64) -> [f64; 3] {
    let (hue, sat, lum) = (hue.clamp(0.0, 1.0), sat.clamp(0.0, 1.0), lum.clamp(0.0, 1.0));
    let v = if lum <= 0.5 { lum * (1.0 + sat) } else { lum + sat - lum * sat };
    if v <= 0.0 {
        return [0.0; 3];
    }
    let hue = (hue * 6.0).clamp(0.0, 6.0);
    let y = lum + lum - v;
    let f = hue - (hue as i32) as f64;
    let x = y + (v - y) * f;
    let z = v - (v - y) * f;
    match hue as i32 {
        1 => [z, v, y],
        2 => [y, v, x],
        3 => [y, z, v],
        4 => [x, y, v],
        5 => [v, y, z],
        _ => [v, x, y],
    }
}

#[cfg(test)]
mod curve_tests {
    use super::*;

    #[test]
    fn curves_rebuild_the_gradient() {
        let g = Gradient { colors: (0..256).map(|i| [(i as u8).saturating_add(20), 60, (255 - i) as u8]).collect() };
        let c = GradientCurves::from_gradient(&g);
        assert_eq!(c.curves[0].len(), CURVE_POINTS);
        let back = c.to_gradient();
        // at the keys the colors come back (12 points cannot follow every detail in between)
        for x in [0usize, 255] {
            assert!(back.colors[x].iter().zip(&g.colors[x]).all(|(a, b)| (*a as i32 - *b as i32).abs() <= 3), "{x}: {:?} vs {:?}", back.colors[x], g.colors[x]);
        }
        assert_eq!(GradientCurves::parse(&c.to_attr()).map(|p| p.curves[2].len()), Some(CURVE_POINTS));
    }
}

// ---------------------------------------------------------------------------------------------
// Gradients from images

/// `ImgPaletteReader`: the first row of an image stretched over 256 entries
/// (`RGBPalette.setColorsNonInterpolate`).
pub fn gradient_from_image_row(img: &crate::resources::ImageMap) -> Gradient {
    let src: Vec<[f64; 3]> = (0..img.width as usize).map(|x| rgb_of(img.pixels[x])).collect();
    let n = src.len();
    let colors = (0..256)
        .map(|i| {
            let coord = (i * (n.max(1) - 1)) as f64 / 255.0;
            let ci = coord as usize;
            let get = |k: usize| src.get(k).copied().unwrap_or([0.0; 3]);
            let (l, r, x) = (get(ci), get(ci + 1), coord - ci as f64);
            [0, 1, 2].map(|k| round_color(l[k] + (r[k] - l[k]) * x) as u8)
        })
        .collect();
    Gradient { colors }
}

fn rgb_of(c: u32) -> [f64; 3] {
    [((c >> 16) & 255) as f64, ((c >> 8) & 255) as f64, (c & 255) as f64]
}

/// `MedianCutQuantizer.createPalette` (median cut after jquant2.c by Thomas G. Lane): the 256
/// colors that represent the image best, sorted by hue.
pub fn gradient_from_image(img: &crate::resources::ImageMap) -> Gradient {
    let mut q = MedianCut::new();
    for &c in &img.pixels {
        let (r, g, b) = (((c >> 16) & 255) as usize, ((c >> 8) & 255) as usize, (c & 255) as usize);
        let h = &mut q.hist[r >> C0_SHIFT][g >> C1_SHIFT][b >> C2_SHIFT];
        *h = h.saturating_add(1);
    }
    let mut colors = q.select_colors(256);
    colors.resize(256, [0, 0, 0]);
    let mut g = Gradient { colors };
    g.sort_range(0, 255);
    g
}

const C0_SHIFT: usize = 3;
const C1_SHIFT: usize = 2;
const C2_SHIFT: usize = 3;
const C0_SCALE: i64 = 2;
const C1_SCALE: i64 = 3;
const C2_SCALE: i64 = 1;

#[derive(Clone, Copy, Default)]
struct CutBox {
    c0: (usize, usize),
    c1: (usize, usize),
    c2: (usize, usize),
    volume: i64,
    colorcount: i64,
}

struct MedianCut {
    hist: Vec<Vec<Vec<i32>>>,
}

impl MedianCut {
    fn new() -> Self {
        Self { hist: vec![vec![vec![0; 32]; 64]; 32] }
    }

    fn h(&self, c0: usize, c1: usize, c2: usize) -> i32 {
        self.hist[c0][c1][c2]
    }

    /// Shrinks the box to its nonzero cells and computes volume and population (`update_box`).
    fn update_box(&self, b: &mut CutBox) {
        let any = |c0: (usize, usize), c1: (usize, usize), c2: (usize, usize)| {
            (c0.0..=c0.1).any(|x| (c1.0..=c1.1).any(|y| (c2.0..=c2.1).any(|z| self.h(x, y, z) != 0)))
        };
        if b.c0.1 > b.c0.0 {
            if let Some(v) = (b.c0.0..=b.c0.1).find(|&v| any((v, v), b.c1, b.c2)) {
                b.c0.0 = v;
            }
            if let Some(v) = (b.c0.0..=b.c0.1).rev().find(|&v| any((v, v), b.c1, b.c2)) {
                b.c0.1 = v;
            }
        }
        if b.c1.1 > b.c1.0 {
            if let Some(v) = (b.c1.0..=b.c1.1).find(|&v| any(b.c0, (v, v), b.c2)) {
                b.c1.0 = v;
            }
            if let Some(v) = (b.c1.0..=b.c1.1).rev().find(|&v| any(b.c0, (v, v), b.c2)) {
                b.c1.1 = v;
            }
        }
        if b.c2.1 > b.c2.0 {
            if let Some(v) = (b.c2.0..=b.c2.1).find(|&v| any(b.c0, b.c1, (v, v))) {
                b.c2.0 = v;
            }
            if let Some(v) = (b.c2.0..=b.c2.1).rev().find(|&v| any(b.c0, b.c1, (v, v))) {
                b.c2.1 = v;
            }
        }
        let d0 = (((b.c0.1 - b.c0.0) << C0_SHIFT) as i64) * C0_SCALE;
        let d1 = (((b.c1.1 - b.c1.0) << C1_SHIFT) as i64) * C1_SCALE;
        let d2 = (((b.c2.1 - b.c2.0) << C2_SHIFT) as i64) * C2_SCALE;
        b.volume = d0 * d0 + d1 * d1 + d2 * d2;
        let mut count = 0;
        for x in b.c0.0..=b.c0.1 {
            for y in b.c1.0..=b.c1.1 {
                for z in b.c2.0..=b.c2.1 {
                    if self.h(x, y, z) != 0 {
                        count += 1;
                    }
                }
            }
        }
        b.colorcount = count;
    }

    fn select_colors(&self, desired: usize) -> Vec<[u8; 3]> {
        let mut boxes = vec![CutBox::default(); desired];
        boxes[0] = CutBox { c0: (0, 255 >> C0_SHIFT), c1: (0, 255 >> C1_SHIFT), c2: (0, 255 >> C2_SHIFT), volume: 0, colorcount: 0 };
        self.update_box(&mut boxes[0]);
        let mut num = 1;
        // median_cut: split by population for the first half, then by volume
        while num < desired {
            let pick = if num * 2 <= desired {
                let mut best: Option<usize> = None;
                let mut maxc = 0;
                for (i, b) in boxes.iter().enumerate() {
                    if b.colorcount > maxc && b.volume > 0 {
                        best = Some(i);
                        maxc = b.colorcount;
                    }
                }
                best
            } else {
                let mut best: Option<usize> = None;
                let mut maxv = 0;
                for (i, b) in boxes.iter().enumerate() {
                    if b.volume > maxv {
                        best = Some(i);
                        maxv = b.volume;
                    }
                }
                best
            };
            let Some(i1) = pick else { break };
            let mut b1 = boxes[i1];
            let mut b2 = CutBox { c0: b1.c0, c1: b1.c1, c2: b1.c2, ..Default::default() };
            let c0 = (((b1.c0.1 - b1.c0.0) << C0_SHIFT) as i64) * C0_SCALE;
            let c1 = (((b1.c1.1 - b1.c1.0) << C1_SHIFT) as i64) * C1_SCALE;
            let c2 = (((b1.c2.1 - b1.c2.0) << C2_SHIFT) as i64) * C2_SCALE;
            // ties: green, then red, blue last
            let (mut cmax, mut n) = (c1, 1);
            if c0 > cmax {
                cmax = c0;
                n = 0;
            }
            if c2 > cmax {
                n = 2;
            }
            match n {
                0 => {
                    let lb = (b1.c0.1 + b1.c0.0) / 2;
                    b1.c0.1 = lb;
                    b2.c0.0 = lb + 1;
                }
                1 => {
                    let lb = (b1.c1.1 + b1.c1.0) / 2;
                    b1.c1.1 = lb;
                    b2.c1.0 = lb + 1;
                }
                _ => {
                    let lb = (b1.c2.1 + b1.c2.0) / 2;
                    b1.c2.1 = lb;
                    b2.c2.0 = lb + 1;
                }
            }
            self.update_box(&mut b1);
            self.update_box(&mut b2);
            boxes[i1] = b1;
            boxes[num] = b2;
            num += 1;
        }
        // compute_color: mean weighted by pixels
        boxes[..num]
            .iter()
            .map(|b| {
                let (mut total, mut t0, mut t1, mut t2) = (0i64, 0i64, 0i64, 0i64);
                for x in b.c0.0..=b.c0.1 {
                    for y in b.c1.0..=b.c1.1 {
                        for z in b.c2.0..=b.c2.1 {
                            let count = self.h(x, y, z) as i64;
                            if count != 0 {
                                total += count;
                                t0 += (((x << C0_SHIFT) + ((1 << C0_SHIFT) >> 1)) as i64) * count;
                                t1 += (((y << C1_SHIFT) + ((1 << C1_SHIFT) >> 1)) as i64) * count;
                                t2 += (((z << C2_SHIFT) + ((1 << C2_SHIFT) >> 1)) as i64) * count;
                            }
                        }
                    }
                }
                if total == 0 {
                    return [0, 0, 0];
                }
                [t0, t1, t2].map(|t| ((t + (total >> 1)) / total).clamp(0, 255) as u8)
            })
            .collect()
    }
}

#[cfg(test)]
mod image_tests {
    use super::*;
    use crate::resources::ImageMap;

    #[test]
    fn gradients_from_images() {
        // left half red, right half blue
        let pixels: Vec<u32> = (0..64 * 8).map(|i| if i % 64 < 32 { 0xC81E1E } else { 0x1E1EC8 }).collect();
        let img = ImageMap { width: 64, height: 8, pixels, alpha: vec![255; 64 * 8] };
        let row = gradient_from_image_row(&img);
        assert_eq!((row.colors[0], row.colors[255]), ([200, 30, 30], [30, 30, 200]));
        let q = gradient_from_image(&img);
        // two colors found (the rest stays black)
        let mut distinct: Vec<[u8; 3]> = q.colors.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(distinct.len(), 3, "{distinct:?}");
        assert!(distinct.iter().any(|c| c[0] > 190 && c[2] < 40));
    }
}
