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
