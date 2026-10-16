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
    validate_wgsl(include_str!("../src/effects.wgsl")).unwrap();
    validate_wgsl(include_str!("../src/effects_blur.wgsl")).unwrap();
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

#[test]
fn weighting_field_programs_are_valid() {
    use rwf_core::model::VariationInstance;
    use rwf_core::weighting::{FieldInput, FieldType, VarParamWeight};
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/TINA0001.flame");
    let mut flame = rwf_core::io::read_flames_from_file(std::path::Path::new(path)).unwrap().remove(0);
    for t in FieldType::ALL {
        let xf = &mut flame.layers[0].xforms[0];
        xf.variations.push(VariationInstance::new("pdj", 0.5));
        xf.weighting.field_type = t;
        xf.weighting.input = if t == FieldType::WhiteNoise { FieldInput::Position } else { FieldInput::Affine };
        xf.weighting.color_intensity = 1.0;
        xf.weighting.var_amount_intensity = 0.5;
        xf.weighting.jitter_intensity = 0.2;
        xf.weighting.var_params[0] = VarParamWeight { intensity: 1.0, var_name: "pdj".into(), param_name: "a".into() };
        xf.weighting.var_params[1] = VarParamWeight { intensity: 0.5, var_name: "pdj".into(), param_name: "amount".into() };
        let prepared = PreparedFlame::new(&flame);
        let prog = Program::new(&flame, &prepared);
        assert_eq!(prog.wf_tables.is_some(), t != FieldType::None, "{t:?}");
        if let Err(err) = validate_wgsl(&prog.wgsl) {
            panic!("weighting field {t:?}: {err}");
        }
        let p = prog.pack_params(&flame, &prepared);
        assert_eq!(p.len(), prog.param_len.max(1));
    }
}

#[test]
fn zbuffer_programs_are_valid() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/TINA0001.flame");
    let mut flame = rwf_core::io::read_flames_from_file(std::path::Path::new(path)).unwrap().remove(0);
    flame.cam_pitch = 0.7;
    for (dof, post_blur) in [(0.0, 0), (0.3, 0), (0.0, 2)] {
        flame.cam_dof = dof;
        flame.post_blur_radius = post_blur;
        let prepared = PreparedFlame::new(&flame);
        let prog = Program::new_scaled(&flame, &prepared, 1.0, true);
        assert!(prog.zbuffer && prog.wgsl.contains("zbuf_add(ix, iy, cam_z * U.bws"), "z-buffer hook missing");
        if let Err(err) = validate_wgsl(&prog.wgsl) {
            panic!("z-buffer dof {dof} post blur {post_blur}: {err}");
        }
    }
}
