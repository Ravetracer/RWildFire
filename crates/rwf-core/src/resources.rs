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

//! External resources of variations (`RessourceManager`): image files, loaded once and cached.
//!
//! JWildfire stores resource values in `.flame` files as upper-case hex strings of the raw bytes
//! (`Tools.byteArrayToHexString`); [`hex_encode`] / [`hex_decode`] convert them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

/// `Tools.byteArrayToHexString`.
pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 15) as usize] as char);
    }
    out
}

/// `Tools.hexStringToByteArray`. `None` for malformed input.
pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16);
    let bytes: Option<Vec<u8>> = s.as_bytes().chunks(2).map(|p| Some((digit(p[0])? * 16 + digit(p[1])?) as u8)).collect();
    bytes
}

/// An 8-bit RGBA image with pixels packed as `r << 16 | g << 8 | b` (exact in `f32` as well)
/// and a separate alpha plane.
#[derive(Debug)]
pub struct ImageMap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
    pub alpha: Vec<u8>,
}

impl ImageMap {
    pub fn from_rgba8(img: &image::RgbaImage) -> Self {
        let (width, height) = img.dimensions();
        let pixels = img.pixels().map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
        let alpha = img.pixels().map(|p| p[3]).collect();
        Self { width, height, pixels, alpha }
    }

    fn from_dynamic(img: image::DynamicImage) -> Option<Self> {
        let img = img.into_rgba8();
        (img.width() > 0 && img.height() > 0).then(|| Self::from_rgba8(&img))
    }

    /// `GradientCreator.createImage(256, 256)` with its default corner colors (red, yellow,
    /// cyan, blue): the fallback image of the image-based variations.
    pub fn default_gradient() -> Arc<Self> {
        static IMG: OnceLock<Arc<ImageMap>> = OnceLock::new();
        IMG.get_or_init(|| {
            const N: i32 = 256;
            const SPREC: i32 = 10;
            let (ul, ur, ll, lr) = ([255, 0, 0], [255, 255, 0], [0, 255, 255], [0, 0, 255]);
            let slope = |a: [i32; 3], b: [i32; 3], n: i32| [0, 1, 2].map(|k| ((b[k] - a[k]) << SPREC) / (n - 1));
            let (lm, rm) = (slope(ul, ll, N), slope(ur, lr, N));
            let mut pixels = vec![0u32; (N * N) as usize];
            let pack = |c: [i32; 3]| (c[0] as u32 & 255) << 16 | (c[1] as u32 & 255) << 8 | (c[2] as u32 & 255);
            for i in 0..N {
                let (l, r) = match i {
                    0 => (ul, ur),
                    _ if i == N - 1 => (ll, lr),
                    _ => ([0, 1, 2].map(|k| ((i * lm[k]) >> SPREC) + ul[k]), [0, 1, 2].map(|k| ((i * rm[k]) >> SPREC) + ur[k])),
                };
                let hm = slope(l, r, N);
                let row = &mut pixels[(i * N) as usize..((i + 1) * N) as usize];
                row[0] = pack(l);
                let mut t = [0; 3];
                for px in row.iter_mut().take(N as usize - 1).skip(1) {
                    t = [0, 1, 2].map(|k| t[k] + hm[k]);
                    *px = pack([0, 1, 2].map(|k| (t[k] >> SPREC) + l[k]));
                }
                row[N as usize - 1] = pack(r);
            }
            Arc::new(ImageMap { width: N as u32, height: N as u32, pixels, alpha: vec![255; (N * N) as usize] })
        })
        .clone()
    }
}

type Cache<T> = Mutex<HashMap<PathBuf, (Option<SystemTime>, Arc<T>)>>;
type ContentCache = HashMap<(u64, usize), Arc<ImageMap>>;

fn mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Loads a PNG or JPEG image (`RessourceManager.getImage`). Cached per path; reloaded when the
/// file changed on disk.
pub fn image(path: &str) -> Option<Arc<ImageMap>> {
    static CACHE: OnceLock<Cache<ImageMap>> = OnceLock::new();
    if path.is_empty() {
        return None;
    }
    let key = PathBuf::from(path);
    let modified = mtime(&key);
    let cache = CACHE.get_or_init(Default::default);
    if let Some((m, img)) = cache.lock().unwrap().get(&key)
        && *m == modified
    {
        return Some(img.clone());
    }
    let img = Arc::new(ImageMap::from_dynamic(image::open(&key).ok()?)?);
    cache.lock().unwrap().insert(key, (modified, img.clone()));
    Some(img)
}

/// Decodes an image file embedded in a flame (`RessourceManager.getImage(hash, bytes)`), cached
/// by content.
pub fn image_from_bytes(bytes: &[u8]) -> Option<Arc<ImageMap>> {
    use std::hash::{Hash, Hasher};
    // key: (content hash, length)
    static CACHE: OnceLock<Mutex<ContentCache>> = OnceLock::new();
    if bytes.is_empty() {
        return None;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    let key = (h.finish(), bytes.len());
    let cache = CACHE.get_or_init(Default::default);
    if let Some(img) = cache.lock().unwrap().get(&key) {
        return Some(img.clone());
    }
    let img = Arc::new(ImageMap::from_dynamic(image::load_from_memory(bytes).ok()?)?);
    cache.lock().unwrap().insert(key, img.clone());
    Some(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_gradient_corners() {
        let img = ImageMap::default_gradient();
        let px = |x: usize, y: usize| img.pixels[y * 256 + x];
        assert_eq!([px(0, 0), px(255, 0), px(0, 255), px(255, 255)], [0xFF0000, 0xFFFF00, 0x00FFFF, 0x0000FF]);
        // halfway down the left edge: between red and cyan
        assert_eq!(px(0, 128) >> 16 & 255, 127);
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(hex_encode(b"/tmp/a.png"), "2F746D702F612E706E67");
        assert_eq!(hex_decode("2F746D702F612E706E67").as_deref(), Some(&b"/tmp/a.png"[..]));
        assert_eq!(hex_decode("2f746d70").as_deref(), Some(&b"/tmp"[..]));
        assert_eq!(hex_decode(""), Some(Vec::new()));
        assert_eq!(hex_decode("2F7"), None);
        assert_eq!(hex_decode("ZZ"), None);
    }
}
