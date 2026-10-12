//! Validates the WGSL of every variation and of full generated programs with naga.

use jwf_core::render::PreparedFlame;
use jwf_core::variations;
use jwf_gpu::codegen::{Program, validate_wgsl, variation_test_shader};

#[test]
fn every_variation_snippet_is_valid_wgsl() {
    let mut failures = Vec::new();
    for def in variations::all() {
        if !def.supports_gpu() {
            continue;
        }
        let src = variation_test_shader(def);
        if let Err(e) = validate_wgsl(&src) {
            failures.push(format!("--- {} ---\n{}", def.name, e));
        }
    }
    assert!(failures.is_empty(), "invalid WGSL snippets:\n{}", failures.join("\n"));
}

#[test]
fn sample_flames_generate_valid_programs() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata");
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|s| s.to_str()) != Some("flame") {
            continue;
        }
        for flame in jwf_core::io::read_flames_from_file(&path).unwrap() {
            let prepared = PreparedFlame::new(&flame);
            let prog = Program::new(&flame, &prepared);
            if let Err(err) = validate_wgsl(&prog.wgsl) {
                panic!("{}: {}", path.display(), err);
            }
        }
    }
}
