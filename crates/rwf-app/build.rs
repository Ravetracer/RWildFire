// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
// SPDX-License-Identifier: LGPL-3.0-or-later

//! Embeds the program icon into Windows executables (GNU toolchain: `windres`).

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu") {
        return;
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ico = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("assets").join("icon.ico");
    let rc = out.join("icon.rc");
    std::fs::write(&rc, format!("1 ICON \"{}\"\n", ico.display().to_string().replace('\\', "/"))).unwrap();
    let obj = out.join("icon.o");
    let windres = std::env::var("WINDRES").unwrap_or_else(|_| {
        if cfg!(windows) { "windres".into() } else { "x86_64-w64-mingw32-windres".into() }
    });
    match Command::new(&windres).arg(&rc).arg("-O").arg("coff").arg("-o").arg(&obj).status() {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", obj.display()),
        _ => println!("cargo:warning={windres} not found or failed: the executable gets no icon"),
    }
}
