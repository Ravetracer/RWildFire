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
