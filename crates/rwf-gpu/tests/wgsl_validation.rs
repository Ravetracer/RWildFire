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

//! Validates full generated programs with naga (variation snippets are validated in rwf-core).

use rwf_core::render::PreparedFlame;
use rwf_gpu::codegen::{Program, validate_wgsl};

#[test]
fn sample_flames_generate_valid_programs() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata");
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("flame") {
            continue;
        }
        for flame in rwf_core::io::read_flames_from_file(&path).unwrap() {
            let prepared = PreparedFlame::new(&flame);
            let prog = Program::new(&flame, &prepared);
            if let Err(err) = validate_wgsl(&prog.wgsl) {
                panic!("{}: {}", path.display(), err);
            }
        }
    }
}

#[test]
fn solid_programs_and_passes_are_valid() {
    validate_wgsl(include_str!("../src/solid.wgsl")).unwrap();
    validate_wgsl(include_str!("../src/tonemap.wgsl")).unwrap();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/TINA0001.flame");
    let mut flame = rwf_core::io::read_flames_from_file(std::path::Path::new(path)).unwrap().remove(0);
    flame.solid.enabled = true;
    for shadows in [rwf_core::solid::ShadowType::Off, rwf_core::solid::ShadowType::Smooth] {
        flame.solid.shadow_type = shadows;
        for dof in [0.0, 0.3] {
            flame.cam_dof = dof;
            let prepared = PreparedFlame::new(&flame);
            let prog = Program::new(&flame, &prepared);
            if let Err(err) = validate_wgsl(&prog.wgsl) {
                panic!("solid {shadows:?} dof {dof}: {err}");
            }
        }
    }
}
