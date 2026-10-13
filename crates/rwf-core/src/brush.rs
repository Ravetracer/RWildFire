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

//! Brush point sets of the `brushstroke_wf` variations (`AbstractBrushStrokeWFFunc`): the 245
//! built-in brushes, the image operations used to vary them (`RotateTransformer`,
//! `ScaleTransformer`) and the generation of weighted point lists.
//!
//! The image code mirrors the Java integer/rounding behaviour (including its quirks) so the
//! point sets match JWildfire.

use std::sync::{Arc, Mutex, OnceLock};

use crate::math::{EPSILON, M_PI, ftoi};
use crate::resources::{self, ImageMap};
use crate::rng::Marsaglia;

/// Number of built-in brushes (`PainterlyStyleMutation.MAX_BRUSH`).
pub const MAX_BRUSH: i32 = 245;

macro_rules! brushes {
    ($($n:literal),*) => {
        [$(include_bytes!(concat!("../assets/brushes/brush", $n, ".png")).as_slice()),*]
    };
}

static BRUSH_PNGS: [&[u8]; 245] = brushes!(
    "001", "002", "003", "004", "005", "006", "007", "008", "009", "010", "011", "012", "013", "014", "015", "016", "017", "018", "019", "020", "021",
    "022", "023", "024", "025", "026", "027", "028", "029", "030", "031", "032", "033", "034", "035", "036", "037", "038", "039", "040", "041", "042",
    "043", "044", "045", "046", "047", "048", "049", "050", "051", "052", "053", "054", "055", "056", "057", "058", "059", "060", "061", "062", "063",
    "064", "065", "066", "067", "068", "069", "070", "071", "072", "073", "074", "075", "076", "077", "078", "079", "080", "081", "082", "083", "084",
    "085", "086", "087", "088", "089", "090", "091", "092", "093", "094", "095", "096", "097", "098", "099", "100", "101", "102", "103", "104", "105",
    "106", "107", "108", "109", "110", "111", "112", "113", "114", "115", "116", "117", "118", "119", "120", "121", "122", "123", "124", "125", "126",
    "127", "128", "129", "130", "131", "132", "133", "134", "135", "136", "137", "138", "139", "140", "141", "142", "143", "144", "145", "146", "147",
    "148", "149", "150", "151", "152", "153", "154", "155", "156", "157", "158", "159", "160", "161", "162", "163", "164", "165", "166", "167", "168",
    "169", "170", "171", "172", "173", "174", "175", "176", "177", "178", "179", "180", "181", "182", "183", "184", "185", "186", "187", "188", "189",
    "190", "191", "192", "193", "194", "195", "196", "197", "198", "199", "200", "201", "202", "203", "204", "205", "206", "207", "208", "209", "210",
    "211", "212", "213", "214", "215", "216", "217", "218", "219", "220", "221", "222", "223", "224", "225", "226", "227", "228", "229", "230", "231",
    "232", "233", "234", "235", "236", "237", "238", "239", "240", "241", "242", "243", "244", "245"
);

/// Working ARGB image.
#[derive(Clone)]
struct Img {
    w: i32,
    h: i32,
    px: Vec<[i32; 4]>,
}

impl Img {
    fn from_map(m: &ImageMap) -> Self {
        let px = m.pixels.iter().zip(&m.alpha).map(|(&p, &a)| [(p >> 16 & 255) as i32, (p >> 8 & 255) as i32, (p & 255) as i32, a as i32]).collect();
        Self { w: m.width as i32, h: m.height as i32, px }
    }
    #[inline]
    fn get(&self, x: i32, y: i32) -> [i32; 4] {
        self.px[(y * self.w + x) as usize]
    }
    #[inline]
    fn set_rgb(&mut self, x: i32, y: i32, r: i32, g: i32, b: i32) {
        self.px[(y * self.w + x) as usize] = [r, g, b, 255];
    }

    /// `Mesh2DTransformer.readSrcPixels`: the pixel at the rounded position and its right,
    /// lower and lower-right neighbours (clamped), all transparent black outside.
    fn read4(&self, px: f64, py: f64) -> [[i32; 4]; 4] {
        let (x, y) = ((px + 0.5) as i32, (py + 0.5) as i32);
        if x < 0 || x >= self.w || y < 0 || y >= self.h {
            return [[0; 4]; 4];
        }
        let p = self.get(x, y);
        let q = if x < self.w - 1 { self.get(x + 1, y) } else { p };
        let (r, s) = if y < self.h - 1 { (self.get(x, y + 1), if x < self.w - 1 { self.get(x + 1, y + 1) } else { q }) } else { (p, q) };
        [p, q, r, s]
    }
}

fn round_color(v: f64) -> i32 {
    ((v + 0.5) as i32).clamp(0, 255)
}

/// `Tools.fmod33`
fn fmod33(v: f64) -> f64 {
    v - (v as i32) as f64
}

fn bilinear(c: &[[i32; 4]; 4], k: usize, xi: f64, yi: f64) -> f64 {
    let [p, q, r, s] = c;
    (1.0 - yi) * ((1.0 - xi) * p[k] as f64 + xi * q[k] as f64) + yi * ((1.0 - xi) * r[k] as f64 + xi * s[k] as f64)
}

/// `RotateTransformer` (with `Mesh2DTransformer.applySmoothing`).
fn rotate(img: &mut Img, zoom: f64, centre: (f64, f64), radius: f64, amount: f64, smoothing: i32) {
    let src = img.clone();
    let alpha = -amount * M_PI / 180.0;
    let (sina, cosa) = alpha.sin_cos();
    let (cx, cy) = (centre.0 - 0.5, centre.1 - 0.5);
    let zoom = 1.0 / zoom.max(0.01);
    let radius = radius * radius;
    let (w1, h1) = (img.w as f64 - 1.0, img.h as f64 - 1.0);
    let bg = src.read4(1.0, 1.0)[0];
    for i in 0..img.h {
        let dyq = i as f64 - cy;
        let y0 = dyq * zoom;
        let dyq = dyq * dyq;
        for j in 0..img.w {
            let mut x0 = j as f64 - cx;
            if x0 * x0 + dyq <= radius {
                x0 *= zoom;
                let x = cosa * x0 + sina * y0 + cx;
                let y = -sina * x0 + cosa * y0 + cy;
                let (xi, yi) = (fmod33(x), fmod33(y));
                let c = if x < 0.0 || x > w1 || y < 0.0 || y > h1 {
                    [bg[0], bg[1], bg[2]]
                } else {
                    let s = src.read4(x, y);
                    [0, 1, 2].map(|k| round_color(bilinear(&s, k, xi, yi)))
                };
                img.set_rgb(j, i, c[0], c[1], c[2]);
            }
        }
    }
    for _ in 0..smoothing {
        for i in 1..img.h - 1 {
            for j in 1..img.w - 1 {
                let (p1, p2, p3, p) = (img.get(j, i - 1), img.get(j + 1, i), img.get(j, i + 1), img.get(j, i));
                let sum = |c: [i32; 4]| c[0] + c[1] + c[2];
                let av = sum(p);
                let (d1, d2, d3) = ((sum(p1) - av).abs(), (sum(p2) - av).abs(), (sum(p3) - av).abs());
                let mixed = |a: [i32; 4], da: i32, b: [i32; 4], db: i32| [0, 1, 2].map(|k| ((a[k] * da + b[k] * db) / (da + db) + p[k]) / 2);
                if d1 > 64 && d2 > 64 {
                    let c = mixed(p1, d1, p2, d2);
                    img.set_rgb(j, i, c[0], c[1], c[2]);
                } else if d2 > 64 && d3 > 64 {
                    let c = mixed(p2, d2, p3, d3);
                    img.set_rgb(j, i, c[0], c[1], c[2]);
                }
            }
        }
    }
}

/// `ScaleTransformer` (keeping the aspect ratio) to `width` x `height`. JWildfire downscales
/// with `scale_down_down`, whose averaging loops re-read the same source pixel; reproduced.
fn scale(img: &Img, width: i32, height: i32) -> Img {
    let (sw, sh) = (img.w, img.h);
    let scl_x = if width > 1 { (sw - 1) as f64 / (width - 1) as f64 } else { 1.0 };
    let scl_y = if height > 1 { (sh - 1) as f64 / (height - 1) as f64 } else { 1.0 };
    let (cxd, cyd) = ((width - 1) as f64 * 0.5, (height - 1) as f64 * 0.5);
    let (cxs, cys) = ((sw - 1) as f64 * 0.5, (sh - 1) as f64 * 0.5);
    let mut out = Img { w: width, h: height, px: vec![[0, 0, 0, 255]; (width * height) as usize] };
    // int += double (Java compound assignment truncates)
    let add = |c: &mut i32, v: f64| *c = (*c as f64 + v) as i32;
    for i in 0..height {
        let y0 = i as f64 - cyd;
        for j in 0..width {
            let x0 = j as f64 - cxd;
            let x = x0 * scl_x + cxs;
            let mut xn = (x0 + 1.0) * scl_x + cxs;
            if xn >= sw as f64 - 0.5 {
                xn = sw as f64 - 1.0;
            }
            let mut y = y0 * scl_y + cys;
            let mut yn = (y0 + 1.0) * scl_y + cys;
            if yn >= sh as f64 - 0.5 {
                yn = sh as f64 - 1.0;
            }
            let c = if (x as i32) < 0 || (x as i32) >= sw || (y as i32) < 0 || (y as i32) >= sh {
                [0; 3]
            } else {
                let (xi, yi) = (fmod33(x), fmod33(y));
                let s = img.read4(x, y);
                let mut cv = [0, 1, 2].map(|k| round_color(bilinear(&s, k, xi, yi)));
                let mut xc = x - xi + 1.0;
                let mut cnt = 1;
                while xc < xn {
                    let s = img.read4(x, y);
                    for k in 0..3 {
                        add(&mut cv[k], (1.0 - yi) * s[0][k] as f64 + yi * s[2][k] as f64);
                    }
                    xc += 1.0;
                    cnt += 1;
                }
                y = y - yi + 1.0;
                while y < yn {
                    let s = img.read4(x, y);
                    for k in 0..3 {
                        add(&mut cv[k], (1.0 - xi) * s[0][k] as f64 + xi * s[1][k] as f64);
                    }
                    xc = x - xi + 1.0;
                    cnt += 1;
                    while xc < xn {
                        let s = img.read4(x, y);
                        for k in 0..3 {
                            cv[k] += s[0][k];
                        }
                        xc += 1.0;
                        cnt += 1;
                    }
                    y += 1.0;
                }
                cv.map(|c| round_color(c as f64 / cnt as f64))
            };
            out.set_rgb(j, i, c[0], c[1], c[2]);
        }
    }
    out
}

/// Settings of the point generation (the `brushstroke_wf` parameters that affect it).
#[derive(Clone, Debug, PartialEq)]
pub struct BrushSettings {
    pub color_channel: i32,
    pub threshold: f64,
    pub invert: i32,
    pub batch_size: i32,
    pub zoom: f64,
    pub position: f64,
    pub rotation: f64,
    pub erosion: f64,
    pub seed: i32,
    pub image_max_size: i32,
    pub rnd_brush_seed: i32,
    pub rnd_brush_min_count: i32,
    pub rnd_brush_max_count: i32,
    pub brush_list: String,
    pub filenames: Vec<String>,
}

/// Point list of one generation: unique points and their exclusive cumulative repeat counts
/// (`distributePoints` repeats every point by its intensity).
#[derive(Debug, Default)]
pub struct PointSet {
    pub points: Vec<(f64, f64)>,
    pub cum: Vec<u64>,
}

impl PointSet {
    pub fn total(&self) -> u64 {
        self.cum.last().copied().unwrap_or(0)
    }
}

fn builtin_brush(id: &str) -> Option<Arc<ImageMap>> {
    let n: usize = id.trim().parse().ok()?;
    let png = BRUSH_PNGS.get(n.checked_sub(1)?)?;
    resources::image_from_bytes(png).filter(|b| b.width >= 16)
}

/// `loadColorMaps`
fn source_images(s: &BrushSettings) -> Vec<Arc<ImageMap>> {
    let mut maps = Vec::new();
    if s.rnd_brush_seed > 0 {
        let mut rnd = Marsaglia::new(s.rnd_brush_seed as i64);
        let count = ftoi(s.rnd_brush_min_count as f64 + rnd.random() * (s.rnd_brush_max_count - s.rnd_brush_min_count) as f64);
        for _ in 0..count {
            let id = MAX_BRUSH.min(ftoi(1.0 + rnd.random() * MAX_BRUSH as f64));
            maps.extend(builtin_brush(&id.to_string()));
        }
    } else {
        maps.extend(s.brush_list.split(',').filter_map(builtin_brush));
    }
    // As in JWildfire, the sixth file name slot loads the fifth file.
    for (i, f) in s.filenames.iter().enumerate() {
        if !f.is_empty() {
            let name = if i == 5 { &s.filenames[4] } else { f };
            maps.extend(resources::image(name));
        }
    }
    if maps.is_empty() {
        maps.push(ImageMap::default_gradient());
    }
    maps
}

fn color_value(s: &BrushSettings, c: [i32; 4]) -> f64 {
    let v = match s.color_channel {
        1 => c[1],
        2 => c[2],
        3 => c[3],
        _ => c[0],
    };
    let n = (v as f64 / 255.0).clamp(0.0, 1.0);
    if s.invert != 0 { 1.0 - n } else { n }
}

/// `createPointsMap`
fn create_point_sets(s: &BrushSettings) -> Vec<PointSet> {
    let maps = source_images(s);
    let mut rnd = Marsaglia::new(s.seed as i64);
    let mut sets = Vec::new();
    let n = maps.len() as i32;
    for (img_id, map) in maps.iter().enumerate() {
        let mut base = Img::from_map(map);
        if s.image_max_size > 64 && (base.w > s.image_max_size || base.h > s.image_max_size) {
            let (w, h) = if base.w > s.image_max_size {
                (s.image_max_size, s.image_max_size as f64 / base.w as f64 * base.h as f64)
            } else {
                ((s.image_max_size as f64 / base.h as f64 * base.w as f64 + 0.5) as i32, s.image_max_size as f64)
            };
            base = scale(&base, w.max(1), ((h + 0.5) as i32).max(1));
        }
        let mut batch = 1.max(s.batch_size / n);
        if img_id == 0 {
            batch = s.batch_size - batch * (n - 1);
        }
        for generation in 0..=batch {
            let (img, dx, dy, erosion) = if generation == 0 {
                (base.clone(), 0, 0, 0.0)
            } else {
                let zoom = (1.0 - s.zoom * rnd.random()).max(0.2);
                let angle = s.rotation * rnd.random() * 180.0 / M_PI;
                let dx = ftoi((0.5 - rnd.random()) * s.position * base.w as f64);
                let dy = ftoi((0.5 - rnd.random()) * s.position * base.h as f64);
                let erosion = s.erosion * rnd.random();
                let mut img = base.clone();
                let radius = ((img.w * img.w + img.h * img.h) as f64).sqrt() / 2.0 + 1.0;
                let centre = ((img.w / 2) as f64, (img.h / 2) as f64);
                rotate(&mut img, zoom, centre, radius, angle, 1);
                (img, dx, dy, erosion)
            };
            let (xs, ys) = (img.w, img.h);
            let max_size = xs.max(ys) as f64;
            let mut pts: Vec<(f64, f64, f64)> = Vec::new();
            for row in 0..ys {
                // As in JWildfire, the x delta shifts the rows and the y delta the columns.
                let mrow = row + dx;
                if mrow < 0 || mrow >= ys {
                    continue;
                }
                for col in 0..xs {
                    let mcol = col + dy;
                    if mcol < 0 || mcol >= xs {
                        continue;
                    }
                    let cv = color_value(s, img.get(mcol, mrow));
                    if cv >= s.threshold {
                        let x = (col as f64 - xs as f64 / 2.0) / max_size;
                        let y = (row as f64 - ys as f64 / 2.0) / max_size;
                        if erosion <= EPSILON || erosion <= rnd.random() {
                            pts.push((x, y, (cv * cv + s.threshold).min(1.0)));
                        }
                    }
                }
            }
            sets.push(distribute(&pts));
        }
    }
    sets
}

/// `distributePoints`
fn distribute(pts: &[(f64, f64, f64)]) -> PointSet {
    let total_size = 64000.max(pts.len() * 2) as f64;
    let total_weight: f64 = pts.iter().map(|p| p.2).sum();
    let mut total = 0u64;
    let mut set = PointSet::default();
    for &(x, y, w) in pts {
        total += ftoi(total_size * w / total_weight).max(1) as u64;
        set.points.push((x, y));
        set.cum.push(total);
    }
    set
}

/// The point sets for `s` (cached, like JWildfire's `RessourceManager` entry).
pub fn point_sets(s: &BrushSettings) -> Arc<Vec<PointSet>> {
    type Entry = (BrushSettings, Arc<Vec<PointSet>>);
    static CACHE: OnceLock<Mutex<Vec<Entry>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((_, v)) = cache.lock().unwrap().iter().find(|(k, _)| k == s) {
        return v.clone();
    }
    let sets = Arc::new(create_point_sets(s));
    let mut c = cache.lock().unwrap();
    if c.len() >= 16 {
        c.remove(0);
    }
    c.push((s.clone(), sets.clone()));
    sets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marsaglia_matches_java() {
        // Values of JWildfire's MarsagliaRandomGenerator with randomize(12345).
        let mut r = Marsaglia::new(12345);
        let v: Vec<f64> = (0..3).map(|_| r.random()).collect();
        assert_eq!(v, [0.3173465464810592, 0.11897752439555596, 0.43708398865400067]);
    }

    #[test]
    fn all_builtin_brushes_decode() {
        for i in 1..=MAX_BRUSH {
            let b = builtin_brush(&format!("{i:03}")).unwrap_or_else(|| panic!("brush {i}"));
            assert!(b.width >= 16 && b.height >= 16);
        }
    }

    #[test]
    fn default_settings_give_point_sets() {
        let s = BrushSettings {
            color_channel: 0,
            threshold: 0.1,
            invert: 1,
            batch_size: 12,
            zoom: 0.8,
            position: 0.5,
            rotation: 2.0 * M_PI,
            erosion: 0.1,
            seed: 12345,
            image_max_size: 320,
            rnd_brush_seed: 0,
            rnd_brush_min_count: 1,
            rnd_brush_max_count: 5,
            brush_list: "001, 005, 012".into(),
            filenames: vec![String::new(); 6],
        };
        let sets = point_sets(&s);
        // 3 brushes with 12 / 3 = 4 variations each (+ the original): 3 * 5 point sets
        assert_eq!(sets.len(), 15);
        assert!(sets.iter().all(|p| p.total() > 60000 && p.points.iter().all(|&(x, y)| x.abs() <= 0.5 && y.abs() <= 0.5)));
    }
}
