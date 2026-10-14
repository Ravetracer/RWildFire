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

//! Saved render state (`.rwfrender`): the flame, the render settings and the accumulation
//! buffers of a [`crate::GpuRenderer`], so a long render can be continued later
//! (JWildfire's `.jwfrender`, own format).
//!
//! Layout: the magic `RWFRENDER`, a format version (`u32`), then a zlib stream with the flame
//! XML, the settings and the named buffers. All numbers are little endian.

use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

use rwf_core::Flame;

const MAGIC: &[u8; 9] = b"RWFRENDER";
const VERSION: u32 = 1;

/// Everything needed to continue a render.
#[derive(Clone, Debug)]
pub struct RenderState {
    pub flame: Flame,
    pub width: u32,
    pub height: u32,
    pub with_alpha: bool,
    pub zbuffer: bool,
    pub samples: u64,
    pub packets: u32,
    pub frame_seed: u32,
    /// Render time so far (not counting pauses), maintained by the caller.
    pub elapsed_ms: u64,
    /// Raw contents of the GPU buffers, by name.
    pub buffers: Vec<(String, Vec<u8>)>,
}

impl RenderState {
    pub fn buffer(&self, name: &str) -> Option<&[u8]> {
        self.buffers.iter().find(|(n, _)| n == name).map(|(_, d)| d.as_slice())
    }

    /// Writes the state; buffers are compressed with the fastest zlib level (they are large).
    pub fn write(&self, path: &Path) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut out = BufWriter::new(file);
        out.write_all(MAGIC).and_then(|_| out.write_all(&VERSION.to_le_bytes())).map_err(|e| e.to_string())?;
        let mut z = flate2::write::ZlibEncoder::new(out, flate2::Compression::fast());
        self.write_body(&mut z).map_err(|e| e.to_string())?;
        z.finish().and_then(|mut w| w.flush()).map_err(|e| e.to_string())
    }

    fn write_body(&self, w: &mut impl Write) -> std::io::Result<()> {
        write_bytes(w, rwf_core::io::write_flame(&self.flame).as_bytes())?;
        w.write_all(&self.width.to_le_bytes())?;
        w.write_all(&self.height.to_le_bytes())?;
        w.write_all(&[self.with_alpha as u8, self.zbuffer as u8])?;
        w.write_all(&self.samples.to_le_bytes())?;
        w.write_all(&self.packets.to_le_bytes())?;
        w.write_all(&self.frame_seed.to_le_bytes())?;
        w.write_all(&self.elapsed_ms.to_le_bytes())?;
        w.write_all(&(self.buffers.len() as u32).to_le_bytes())?;
        for (name, data) in &self.buffers {
            write_bytes(w, name.as_bytes())?;
            write_bytes(w, data)?;
        }
        Ok(())
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut input = BufReader::new(file);
        let mut magic = [0u8; 9];
        input.read_exact(&mut magic).map_err(|_| "not a render state file".to_string())?;
        if &magic != MAGIC {
            return Err("not a render state file".into());
        }
        let version = read_u32(&mut input).map_err(|e| e.to_string())?;
        if version != VERSION {
            return Err(format!("unsupported render state version {version}"));
        }
        let mut z = flate2::read::ZlibDecoder::new(input);
        Self::read_body(&mut z).map_err(|e| format!("damaged render state file: {e}"))
    }

    fn read_body(r: &mut impl Read) -> std::io::Result<Self> {
        let invalid = |msg: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_string());
        let xml = String::from_utf8(read_bytes(r)?).map_err(|_| invalid("flame is not UTF-8"))?;
        let flame = rwf_core::io::read_flames(&xml).ok().and_then(|f| f.into_iter().next()).ok_or_else(|| invalid("no flame"))?;
        let width = read_u32(r)?;
        let height = read_u32(r)?;
        let mut flags = [0u8; 2];
        r.read_exact(&mut flags)?;
        let samples = read_u64(r)?;
        let packets = read_u32(r)?;
        let frame_seed = read_u32(r)?;
        let elapsed_ms = read_u64(r)?;
        let count = read_u32(r)?;
        let mut buffers = Vec::new();
        for _ in 0..count {
            let name = String::from_utf8(read_bytes(r)?).map_err(|_| invalid("buffer name"))?;
            buffers.push((name, read_bytes(r)?));
        }
        Ok(Self { flame, width, height, with_alpha: flags[0] != 0, zbuffer: flags[1] != 0, samples, packets, frame_seed, elapsed_ms, buffers })
    }
}

fn write_bytes(w: &mut impl Write, data: &[u8]) -> std::io::Result<()> {
    w.write_all(&(data.len() as u64).to_le_bytes())?;
    w.write_all(data)
}

fn read_bytes(r: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let len = read_u64(r)?;
    let mut data = Vec::new();
    // `take` instead of a preallocated buffer: a damaged length must not allocate gigabytes.
    r.take(len).read_to_end(&mut data)?;
    if data.len() as u64 != len {
        return Err(std::io::ErrorKind::UnexpectedEof.into());
    }
    Ok(data)
}

fn read_u32(r: &mut impl Read) -> std::io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> std::io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut flame = Flame::default();
        flame.brightness = 3.5;
        let st = RenderState {
            flame,
            width: 640,
            height: 360,
            with_alpha: true,
            zbuffer: false,
            samples: 123_456_789_012,
            packets: 1,
            frame_seed: 7,
            elapsed_ms: 98_765,
            buffers: vec![("accum".into(), (0..=255u8).cycle().take(100_000).collect()), ("states".into(), vec![1, 2, 3])],
        };
        let dir = std::env::temp_dir().join(format!("rwf-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("t.rwfrender");
        st.write(&path).unwrap();
        let back = RenderState::read(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back.flame.brightness, 3.5);
        assert_eq!((back.width, back.height, back.with_alpha, back.zbuffer), (640, 360, true, false));
        assert_eq!((back.samples, back.packets, back.frame_seed, back.elapsed_ms), (st.samples, 1, 7, 98_765));
        assert_eq!(back.buffers, st.buffers);
    }
}
