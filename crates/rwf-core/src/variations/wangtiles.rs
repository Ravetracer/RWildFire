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

//! `wangtiles` (Jesus Sosa): a random field of Wang tiles (JWildfire's tile sets, packed into
//! `assets/wangtiles.bin.zz`) used as a color map. The prepared data has the layout of
//! `colormap_wf`, so the CPU and the GPU share its lookup.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::resource_vars::{CM_IMG, IMG, colormap_apply, push_image};
use super::{ParamDef, VarDef, VarKind};
use crate::math::ftoi;
use crate::resources::ImageMap;
use crate::rng::JavaRandom;

/// `WangTilesFunc.Id`: tile set per `id`.
const IDS: [&str; 105] = [
    "block2/bloc",
    "block2/bowtie",
    "block2/box",
    "block2/diag",
    "block2/pool",
    "block2/spiral",
    "block2/square",
    "block2/steps",
    "block2/1edge2a",
    "block2/1corn2a",
    "block2/1corn2b",
    "block4/arrow",
    "block4/box",
    "block4/brench",
    "block4/bubble",
    "block4/braid",
    "block4/dublin",
    "block4/fence",
    "block4/floor1",
    "block4/floor2",
    "block4/floor3",
    "block4/fold",
    "block4/geo",
    "block4/madrid",
    "block4/oslo",
    "block4/panel",
    "block4/rome",
    "block4/wall",
    "block4/wedge",
    "block4/1edge4a",
    "block4/1edge4b",
    "edge2/angular",
    "edge2/border",
    "edge2/brench",
    "edge2/brickwall",
    "edge2/bridge",
    "edge2/celtic",
    "edge2/circuit",
    "edge2/dual",
    "edge2/drawn",
    "edge2/dropbox",
    "edge2/glob",
    "edge2/greek",
    "edge2/groove",
    "edge2/laser",
    "edge2/ledge",
    "edge2/line",
    "edge2/molecule",
    "edge2/pipe",
    "edge2/pipe2",
    "edge2/rail",
    "edge2/road",
    "edge2/tilt",
    "edge2/trench",
    "edge2/urban",
    "edge2/walkway",
    "edge2/arrow",
    "edge2/balls",
    "edge2/lattice",
    "edge2/octal",
    "edge2/quad",
    "edge2/wang2e",
    "edge3/flow",
    "edge3/celtic3e",
    "edge3/puzzle",
    "edge3/walkways",
    "edge3/wang3e",
    "corn2/border",
    "corn2/glob",
    "corn2/path",
    "corn2/roof",
    "corn2/beam",
    "corn2/cliff",
    "corn2/ground",
    "corn2/island",
    "corn2/lido",
    "corn2/patch",
    "corn2/pcb",
    "corn2/sandgrass",
    "corn2/terrain",
    "corn2/wang2c",
    "corn3/campus",
    "corn3/puzzle",
    "corn3/wang3c",
    "truch/tru1",
    "truch/tru2",
    "truch/tru3",
    "truch/tru4",
    "truch/tru5",
    "truch/tru6",
    "truch/tru7",
    "truch/tru8",
    "twin/brench",
    "twin/pipes",
    "twin/routes",
    "twin/wang",
    "blob0/bridge",
    "blob0/commune",
    "blob0/dungeon",
    "blob0/trench",
    "blob0/wang",
    "blob1/commune",
    "blob1/dungeon",
    "blob1/trench",
    "blob1/wang",
];

const TILE: usize = 32;

/// The tile sets: "edge/angular" → tile index → 32 × 32 RGB.
fn tile_sets() -> &'static HashMap<String, HashMap<u16, Vec<u8>>> {
    static SETS: OnceLock<HashMap<String, HashMap<u16, Vec<u8>>>> = OnceLock::new();
    SETS.get_or_init(|| {
        use std::io::Read;
        let mut raw = Vec::new();
        let packed: &[u8] = include_bytes!("../../assets/wangtiles.bin.zz");
        if flate2::read::ZlibDecoder::new(packed).read_to_end(&mut raw).is_err() {
            return HashMap::new();
        }
        let mut sets = HashMap::new();
        let mut pos = 0usize;
        let mut take = |n: usize| {
            let s = raw.get(pos..pos + n).unwrap_or(&[]);
            pos += n;
            s.to_vec()
        };
        let u16_of = |b: Vec<u8>| u16::from_le_bytes([b.first().copied().unwrap_or(0), b.get(1).copied().unwrap_or(0)]);
        let count = u16_of(take(2));
        for _ in 0..count {
            let len = take(1).first().copied().unwrap_or(0) as usize;
            let name = String::from_utf8_lossy(&take(len)).into_owned();
            let tiles = u16_of(take(2));
            let mut set = HashMap::new();
            for _ in 0..tiles {
                let idx = u16_of(take(2));
                set.insert(idx, take(TILE * TILE * 3));
            }
            sets.insert(name, set);
        }
        sets
    })
}

/// `WangTilesFunc.Tiles`: the tile layout of one tile set (12 × 8 tiles).
struct Tiles {
    rnd: JavaRandom,
    /// "block", "edge", "corn", "twin", "blob" or "truch".
    kind: &'static str,
    /// Blob variant (`blob1` sets it; it selects `blob0_tiles`, as in JWildfire).
    blob: bool,
    field: Vec<Vec<i32>>,
    piece: i32,
    order: i32,
    matrix: [[i32; 8]; 12],
}

const WIDE: usize = 12;
const HIGH: usize = 8;
const FWIDE: usize = 2 * WIDE + 1;
const FHIGH: usize = 2 * HIGH + 1;

impl Tiles {
    fn rand_int(&mut self, n: i32) -> i32 {
        (self.rnd.next_double() * n as f64).floor() as i32
    }

    fn block_tiles(&mut self) {
        for row in (1..FHIGH).step_by(2) {
            for col in (1..FWIDE).step_by(2) {
                self.field[col - 1][row - 1] = self.rand_int(self.piece);
            }
        }
        for row in (3..FHIGH).step_by(2) {
            for col in (3..FWIDE).step_by(2) {
                let f = &mut self.field;
                let index = f[col - 1][row - 3] + 2 * f[col - 3][row - 3] + 4 * f[col - 3][row - 1];
                match index {
                    0 | 5 => f[col - 1][row - 1] = 1,
                    2 | 7 => f[col - 1][row - 1] = 0,
                    _ => {}
                }
            }
        }
    }

    fn edge_tiles(&mut self) {
        for row in 0..FHIGH - 1 {
            for col in (0..FWIDE - 1).step_by(2) {
                let v = self.rand_int(self.order);
                self.field[col + (row + 1) % 2][row] = v;
            }
        }
        self.clear_border();
    }

    fn corn_tiles(&mut self) {
        for row in (0..FHIGH - 1).step_by(2) {
            for col in (0..FWIDE - 1).step_by(2) {
                self.field[col][row] = self.rand_int(self.order);
            }
        }
        self.clear_border();
    }

    fn twin_tiles(&mut self) {
        for row in (1..FHIGH - 1).step_by(2) {
            for col in (1..FWIDE - 1).step_by(4) {
                let dd = (row + 1) % 4;
                let val = self.rand_int(2);
                self.field[col + dd + 1][row] = val;
                self.field[col + dd - 1][row] = val;
                let val = self.rand_int(2);
                self.field[col + dd][row + 1] = val;
                self.field[col + dd][row - 1] = val;
            }
        }
        self.clear_border();
        self.clear_border2();
    }

    fn truch_tiles(&mut self) {
        for row in (1..FHIGH).step_by(2) {
            for col in (1..FWIDE).step_by(2) {
                self.field[col - 1][row - 1] = self.rand_int(self.piece);
            }
        }
    }

    fn blob_tiles(&mut self) {
        self.clear_cells(0);
        if self.blob { self.blob0_tiles() } else { self.blob1_tiles() }
    }

    fn blob1_tiles(&mut self) {
        for row in 1..FHIGH - 1 {
            for col in 1..FWIDE - 1 {
                if self.rand_int(5) != 0 {
                    self.field[col][row] = 1;
                }
            }
        }
        for row in (1..FHIGH).step_by(2) {
            for col in (1..FWIDE).step_by(2) {
                let f = &mut self.field;
                if f[col][row - 1] == 0 {
                    f[col - 1][row - 1] = 0;
                    f[col + 1][row - 1] = 0;
                }
                if f[col - 1][row] == 0 {
                    f[col - 1][row - 1] = 0;
                    f[col - 1][row + 1] = 0;
                }
            }
        }
        self.remove_holes();
    }

    fn blob0_tiles(&mut self) {
        for row in (1..FHIGH).step_by(2) {
            for col in (1..FWIDE).step_by(2) {
                if self.rand_int(5) != 0 {
                    self.field[col][row] = 1;
                }
            }
        }
        let f = &mut self.field;
        for row in (2..FHIGH - 2).step_by(2) {
            for col in (2..FWIDE - 2).step_by(2) {
                if f[col - 1][row - 1] == 1 && f[col - 1][row + 1] == 1 {
                    f[col - 1][row] = 1;
                }
                if f[col - 1][row - 1] == 1 && f[col + 1][row - 1] == 1 {
                    f[col][row - 1] = 1;
                }
            }
        }
        for row in (2..FHIGH - 2).step_by(2) {
            let col = FWIDE - 1;
            if f[col - 1][row - 1] == 1 && f[col - 1][row + 1] == 1 {
                f[col - 1][row] = 1;
            }
        }
        for col in (2..FWIDE - 2).step_by(2) {
            let row = FHIGH - 1;
            if f[col - 1][row - 1] == 1 && f[col + 1][row - 1] == 1 {
                f[col][row - 1] = 1;
            }
        }
        self.remove_holes();
    }

    fn clear_border(&mut self) {
        for col in 0..FWIDE {
            self.field[col][0] = 0;
            self.field[col][FHIGH - 1] = 0;
        }
        for row in 1..FHIGH - 1 {
            self.field[0][row] = 0;
            self.field[FWIDE - 1][row] = 0;
        }
    }

    fn clear_border2(&mut self) {
        for col in (3..FWIDE - 1).step_by(4) {
            self.field[col][2] = 0;
        }
        for col in (1..FWIDE - 1).step_by(4) {
            self.field[col][FHIGH - 3] = 0;
        }
        for row in (3..FHIGH - 1).step_by(4) {
            self.field[2][row] = 0;
        }
        for row in (1..FHIGH - 1).step_by(4) {
            self.field[FWIDE - 3][row] = 0;
        }
    }

    fn remove_holes(&mut self) {
        for row in (2..FHIGH - 2).step_by(2) {
            for col in (2..FWIDE - 2).step_by(2) {
                if self.rand_int(10) == 0 {
                    continue;
                }
                let f = &mut self.field;
                if f[col][row] == 0 && f[col - 1][row] == 1 && f[col + 1][row] == 1 && f[col][row - 1] == 1 && f[col][row + 1] == 1 {
                    f[col][row] = 1;
                }
            }
        }
    }

    fn clear_cells(&mut self, v: i32) {
        for row in 0..FHIGH - 1 {
            for col in 0..FWIDE - 1 {
                self.field[col][row] = v;
            }
        }
    }

    fn value(&self, col: usize, row: usize) -> i32 {
        let f = &self.field;
        let o = self.order;
        match self.kind {
            "block" => f[col - 1][row - 1],
            "edge" => o.pow(3) * f[col - 1][row] + o.pow(2) * f[col][row + 1] + o * f[col + 1][row] + f[col][row - 1],
            "corn" => o.pow(3) * f[col - 1][row - 1] + o.pow(2) * f[col - 1][row + 1] + o * f[col + 1][row + 1] + f[col + 1][row - 1],
            "twin" => {
                let vert = if col % 4 == 3 { 2 } else { 1 };
                let horz = if row % 4 == 3 { 2 } else { 1 };
                horz * 27 * f[col - 1][row] + vert * 9 * f[col][row + 1] + horz * 3 * f[col + 1][row] + vert * f[col][row - 1]
            }
            "blob" => {
                128 * f[col - 1][row - 1]
                    + 64 * f[col - 1][row]
                    + 32 * f[col - 1][row + 1]
                    + 16 * f[col][row + 1]
                    + 8 * f[col + 1][row + 1]
                    + 4 * f[col + 1][row]
                    + 2 * f[col + 1][row - 1]
                    + f[col][row - 1]
            }
            _ => {
                // truchet
                let parity = ((0.5 * (col - 1) as f64 + 0.5 * (row - 1) as f64) % 2.0) as i32;
                self.piece * parity + f[col - 1][row - 1]
            }
        }
    }

    /// `setRandom`
    fn set_random(&mut self) {
        match self.kind {
            "block" => self.block_tiles(),
            "edge" => self.edge_tiles(),
            "corn" => self.corn_tiles(),
            "twin" => self.twin_tiles(),
            "blob" => self.blob_tiles(),
            _ => self.truch_tiles(),
        }
        for row in (1..FHIGH).step_by(2) {
            for col in (1..FWIDE).step_by(2) {
                let v = self.value(col, row);
                self.field[col][row] = v;
                self.matrix[col / 2][row / 2] = v;
            }
        }
    }
}

/// The color map of tile set `id` and `seed` (`initOnce`): 12 × 8 tiles of 32 × 32 pixels.
fn tile_image(id: usize, seed: i32) -> Arc<ImageMap> {
    type Cache = Mutex<HashMap<(usize, i32), Arc<ImageMap>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(img) = cache.lock().expect("wangtiles cache").get(&(id, seed)) {
        return img.clone();
    }
    let option = IDS[id.min(IDS.len() - 1)];
    let (group, name) = option.split_once('/').unwrap_or((option, ""));
    let (kind, piece, order, blob): (&'static str, i32, i32, bool) = match group {
        "block2" => ("block", 2, 2, false),
        "block4" => ("block", 4, 2, false),
        "edge2" => ("edge", 2, 2, false),
        "edge3" => ("edge", 2, 3, false),
        "corn2" => ("corn", 2, 2, false),
        "corn3" => ("corn", 2, 3, false),
        "twin" => ("twin", 2, 3, false),
        "blob0" => ("blob", 2, 3, false),
        "blob1" => ("blob", 2, 3, true),
        _ => ("truch", if name == "tru7" || name == "tru8" { 4 } else { 2 }, 2, false),
    };
    let mut t = Tiles { rnd: JavaRandom::new(seed as i64), kind, blob, field: vec![vec![0; FHIGH]; FWIDE], piece, order, matrix: [[0; HIGH]; WIDE] };
    t.set_random();
    let set = tile_sets().get(&format!("{kind}/{name}"));
    let (w, h) = (WIDE * TILE, HIGH * TILE);
    let mut pixels = vec![0u32; w * h];
    for i in 0..HIGH {
        for j in 0..WIDE {
            // Missing tiles stay black (JWildfire fails on them).
            let Some(rgb) = set.and_then(|s| u16::try_from(t.matrix[j][i]).ok().and_then(|k| s.get(&k))) else { continue };
            for row in 0..TILE {
                for col in 0..TILE {
                    let o = (row * TILE + col) * 3;
                    pixels[(i * TILE + row) * w + j * TILE + col] = (rgb[o] as u32) << 16 | (rgb[o + 1] as u32) << 8 | rgb[o + 2] as u32;
                }
            }
        }
    }
    let img = Arc::new(ImageMap { width: w as u32, height: h as u32, pixels, alpha: Vec::new() });
    cache.lock().expect("wangtiles cache").insert((id, seed), img.clone());
    img
}

/// Index of the jitter width (1, or 1.5 when not `square`) in the prepared data (a slot
/// `colormap_wf` uses for image sequences only).
const RECT: usize = 12;

pub static DEFS: &[VarDef] = &[VarDef {
    name: "wangtiles",
    params: &[
        ParamDef::int("id", 0.0, 0.0, 104.0),
        ParamDef::int("seed", 1000.0, -1.0e9, 1.0e9),
        ParamDef::int("square", 1.0, 0.0, 1.0),
        ParamDef::new("scale_x", 1.0),
        ParamDef::new("scale_y", 1.0),
        ParamDef::new("scale_z", 0.0),
        ParamDef::new("offset_x", 0.0),
        ParamDef::new("offset_y", 0.0),
        ParamDef::new("offset_z", 0.0),
        ParamDef::int("tile_x", 1.0, 0.0, 1.0),
        ParamDef::int("tile_y", 1.0, 0.0, 1.0),
        ParamDef::int("reset_z", 1.0, 0.0, 1.0),
    ],
    kind: VarKind::Normal,
    is_3d: true,
    apply: |d, ctx, a, v, amount| {
        v.x += amount * d[RECT] * (ctx.random() - 0.5);
        v.y += amount * (ctx.random() - 0.5);
        let (x, y) = (v.x, v.y);
        colormap_apply(d, ctx, v, x, y);
        ctx.pass_z(a, v, amount);
    },
    prepare: Some(|p, _| {
        // p = [id, seed, square, scale_x, scale_y, scale_z, offset_x, offset_y, offset_z,
        //      tile_x, tile_y, reset_z]; data as colormap_wf: [scale_x, scale_y, scale_z,
        //      offset_x, offset_y, offset_z, tile_x, tile_y, reset_z, dc_color (2: both),
        //      blend (0), with_alpha (0), rect, 0, 0, image (3), alpha (3), ..]
        let id = (p[0] as i64).clamp(0, IDS.len() as i64 - 1) as usize;
        let rect = if (p[2] as i64).clamp(0, 1) == 0 { 1.5 } else { 1.0 };
        let mut d =
            vec![p[3], p[4], p[5], p[6], p[7], p[8], ftoi(p[9]) as f64, ftoi(p[10]) as f64, ftoi(p[11]) as f64, 2.0, 0.0, 0.0, rect, 0.0, 0.0];
        d.extend([0.0; 2 * IMG]);
        push_image(&mut d, CM_IMG, &tile_image(id, p[1] as i32));
        d
    }),
    wgsl: "v.x += AMOUNT * D(12) * (rnd() - 0.5);
v.y += AMOUNT * (rnd() - 0.5);
v = res_colormap(base, v.x, v.y, v);
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tile_set_is_packed() {
        for id in IDS {
            let (group, name) = id.split_once('/').unwrap();
            let kind = match group {
                "block2" | "block4" => "block",
                "edge2" | "edge3" => "edge",
                "corn2" | "corn3" => "corn",
                "blob0" | "blob1" => "blob",
                g => g,
            };
            assert!(tile_sets().contains_key(&format!("{kind}/{name}")), "{id} missing");
        }
    }

    #[test]
    fn images_are_filled_and_seeded() {
        for id in [0, 15, 31, 62, 67, 81, 84, 90, 92, 96, 101] {
            let img = tile_image(id, 1000);
            assert_eq!((img.width, img.height), (384, 256));
            let distinct: std::collections::HashSet<u32> = img.pixels.iter().copied().collect();
            assert!(distinct.len() > 1, "{} is blank", IDS[id]);
        }
        assert_ne!(tile_image(31, 1).pixels, tile_image(31, 2).pixels);
        assert_eq!(tile_image(31, 7).pixels, tile_image(31, 7).pixels);
    }
}
