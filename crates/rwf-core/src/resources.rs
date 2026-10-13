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

/// `Tools.byteArrayToHexString` of the UTF-8 bytes of `s`.
pub fn hex_encode(s: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 15) as usize] as char);
    }
    out
}

/// `Tools.hexStringToByteArray`, decoded as UTF-8 (lossy). `None` for malformed input.
pub fn hex_decode(s: &str) -> Option<String> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16);
    let bytes: Option<Vec<u8>> = s.as_bytes().chunks(2).map(|p| Some((digit(p[0])? * 16 + digit(p[1])?) as u8)).collect();
    Some(String::from_utf8_lossy(&bytes?).into_owned())
}

/// An 8-bit RGB image with pixels packed as `r << 16 | g << 8 | b` (exact in `f32` as well).
#[derive(Debug)]
pub struct ImageMap {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl ImageMap {
    pub fn from_rgb8(img: &image::RgbImage) -> Self {
        let (width, height) = img.dimensions();
        let pixels = img.pixels().map(|p| (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32).collect();
        Self { width, height, pixels }
    }
}

type Cache<T> = Mutex<HashMap<PathBuf, (Option<SystemTime>, Arc<T>)>>;

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
    let img = image::open(&key).ok()?.into_rgb8();
    if img.width() == 0 || img.height() == 0 {
        return None;
    }
    let img = Arc::new(ImageMap::from_rgb8(&img));
    cache.lock().unwrap().insert(key, (modified, img.clone()));
    Some(img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(hex_encode("/tmp/a.png"), "2F746D702F612E706E67");
        assert_eq!(hex_decode("2F746D702F612E706E67").as_deref(), Some("/tmp/a.png"));
        assert_eq!(hex_decode("2f746d70").as_deref(), Some("/tmp"));
        assert_eq!(hex_decode(""), Some(String::new()));
        assert_eq!(hex_decode("2F7"), None);
        assert_eq!(hex_decode("ZZ"), None);
    }
}
