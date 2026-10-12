//! Validates full generated programs with naga (variation snippets are validated in jwf-core).

use jwf_core::render::PreparedFlame;
use jwf_gpu::codegen::{Program, validate_wgsl};

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
