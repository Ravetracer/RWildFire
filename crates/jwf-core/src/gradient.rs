//! 256-entry color gradient ("palette" in JWildfire).

use crate::math::{hsl_to_rgb, rgb_to_hsl};
use crate::rng::Rng;

pub const GRADIENT_SIZE: usize = 256;

/// Color scale used by JWildfire's `RenderColor` (DFLT_WHITE_LEVEL / 256).
const RENDER_COLOR_SCALE: f64 = 200.0 / 256.0;

#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub colors: Vec<[u8; 3]>,
}

impl Default for Gradient {
    fn default() -> Self {
        // Neutral warm default so a fresh flame is visible.
        let mut g = Self::from_keys(&[(0.0, [20, 10, 60]), (0.35, [200, 60, 30]), (0.7, [255, 200, 80]), (1.0, [255, 255, 230])]);
        g.colors[0] = [20, 10, 60];
        g
    }
}

impl Gradient {
    pub fn from_colors(colors: Vec<[u8; 3]>) -> Self {
        let mut colors = colors;
        if colors.is_empty() {
            colors.push([0, 0, 0]);
        }
        // Stretch/sample to exactly 256 entries.
        if colors.len() != GRADIENT_SIZE {
            let n = colors.len();
            let src = colors.clone();
            colors = (0..GRADIENT_SIZE)
                .map(|i| {
                    let pos = i as f64 * (n - 1) as f64 / (GRADIENT_SIZE - 1) as f64;
                    let a = pos.floor() as usize;
                    let b = (a + 1).min(n - 1);
                    let t = pos - a as f64;
                    lerp_rgb(src[a], src[b], t)
                })
                .collect();
        }
        Self { colors }
    }

    /// Builds a gradient from (position 0..1, color) key points.
    pub fn from_keys(keys: &[(f64, [u8; 3])]) -> Self {
        let mut keys: Vec<(f64, [u8; 3])> = keys.to_vec();
        keys.sort_by(|a, b| a.0.total_cmp(&b.0));
        let colors = (0..GRADIENT_SIZE)
            .map(|i| {
                let t = i as f64 / (GRADIENT_SIZE - 1) as f64;
                if t <= keys[0].0 {
                    return keys[0].1;
                }
                for w in keys.windows(2) {
                    if t <= w[1].0 {
                        let span = (w[1].0 - w[0].0).max(1e-9);
                        return lerp_rgb(w[0].1, w[1].1, (t - w[0].0) / span);
                    }
                }
                keys[keys.len() - 1].1
            })
            .collect();
        Self { colors }
    }

    /// Render palette in JWildfire's internal color scale.
    pub fn render_colors(&self) -> Vec<[f64; 3]> {
        self.colors
            .iter()
            .map(|c| [c[0] as f64 * RENDER_COLOR_SCALE, c[1] as f64 * RENDER_COLOR_SCALE, c[2] as f64 * RENDER_COLOR_SCALE])
            .collect()
    }

    pub fn shift(&mut self, amount: i32) {
        let n = self.colors.len() as i32;
        let k = amount.rem_euclid(n) as usize;
        self.colors.rotate_right(k);
    }

    pub fn reverse(&mut self) {
        self.colors.reverse();
    }

    pub fn adjust_hsl(&mut self, hue_shift: f64, sat_add: f64, lum_add: f64) {
        for c in &mut self.colors {
            let (h, s, l) = rgb_to_hsl(c[0] as f64 / 255.0, c[1] as f64 / 255.0, c[2] as f64 / 255.0);
            let mut h = (h + hue_shift).fract();
            if h < 0.0 {
                h += 1.0;
            }
            let (r, g, b) = hsl_to_rgb(h, (s + sat_add).clamp(0.0, 1.0), (l + lum_add).clamp(0.0, 1.0));
            *c = [to_u8(r * 255.0), to_u8(g * 255.0), to_u8(b * 255.0)];
        }
    }

    /// Random gradient made of a few smoothly blended key colors.
    pub fn random(rng: &mut Rng) -> Self {
        let n = 3 + rng.random_int(6);
        let base_hue = rng.random();
        // Mostly analogous palettes, sometimes complementary or fully colorful ones.
        let hue_spread = match rng.random_int(3) {
            0 => rng.range(0.15, 0.35),
            1 => rng.range(0.35, 0.7),
            _ => rng.range(0.7, 1.6),
        };
        let mut keys = Vec::with_capacity(n);
        for i in 0..n {
            let pos = if i == 0 {
                0.0
            } else if i == n - 1 {
                1.0
            } else {
                rng.random()
            };
            let h = (base_hue + rng.range(-hue_spread, hue_spread) * 0.5).rem_euclid(1.0);
            let s = rng.range(0.45, 1.0);
            let l = if i % 2 == 0 { rng.range(0.1, 0.45) } else { rng.range(0.5, 0.92) };
            let (r, g, b) = hsl_to_rgb(h, s, l);
            keys.push((pos, [to_u8(r * 255.0), to_u8(g * 255.0), to_u8(b * 255.0)]));
        }
        Self::from_keys(&keys)
    }

    pub fn to_hex(&self) -> String {
        let mut s = String::with_capacity(GRADIENT_SIZE * 6 + GRADIENT_SIZE / 12 * 4);
        for (i, c) in self.colors.iter().enumerate() {
            if i % 12 == 0 {
                s.push_str("\n      ");
            }
            s.push_str(&format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]));
        }
        s.push_str("\n    ");
        s
    }
}

fn to_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    [
        to_u8(a[0] as f64 + (b[0] as f64 - a[0] as f64) * t),
        to_u8(a[1] as f64 + (b[1] as f64 - a[1] as f64) * t),
        to_u8(a[2] as f64 + (b[2] as f64 - a[2] as f64) * t),
    ]
}
