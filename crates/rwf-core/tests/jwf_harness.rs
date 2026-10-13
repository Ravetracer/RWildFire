// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
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

//! Compares variations with JWildfire's reference outputs (`tools/jwf-harness`):
//!
//! ```sh
//! java -cp <jwildfire classes>:lib/*:resources:tools/jwf-harness VarHarness < spec.txt > ref.txt
//! JWF_SPEC=spec.txt JWF_REF=ref.txt cargo test -p rwf-core --test jwf_harness -- --ignored --nocapture
//! ```

use rwf_core::model::{Affine, VariationInstance};
use rwf_core::variations::{self, Ctx, Pt};

const AMOUNT: f64 = 0.8;
const POINTS: [[f64; 4]; 6] =
    [[0.3, 0.2, 0.1, 0.25], [-0.7, 0.4, -0.2, 0.5], [1.3, -0.9, 0.6, 0.75], [-0.05, -0.15, 0.03, 0.1], [2.1, 1.7, -1.1, 0.9], [-1.6, -2.3, 0.4, 0.35]];

fn fields(p: &Pt) -> [f64; 9] {
    [p.x, p.y, p.z, p.color, p.rgb_color as i32 as f64, p.red, p.green, p.blue, p.do_hide as i32 as f64]
}

const NAMES: [&str; 9] = ["x", "y", "z", "color", "rgb", "red", "green", "blue", "hide"];

fn close(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a == b || (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs()))
}

#[test]
#[ignore]
fn compare_with_jwildfire() {
    let spec = std::fs::read_to_string(std::env::var("JWF_SPEC").expect("JWF_SPEC")).unwrap();
    let reference = std::fs::read_to_string(std::env::var("JWF_REF").expect("JWF_REF")).unwrap();
    let (mut ok, mut bad) = (0, 0);
    for line in spec.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let mut tok = line.split_whitespace();
        let name = tok.next().unwrap();
        let rows: Vec<Vec<&str>> =
            reference.lines().map(|l| l.split_whitespace().collect::<Vec<_>>()).filter(|t| t.first() == Some(&name)).collect();
        if rows.iter().any(|r| r.get(1) == Some(&"ERROR")) || rows.is_empty() {
            println!("{name}: no JWildfire reference");
            continue;
        }
        let Some(def) = variations::find(name) else {
            println!("{name}: MISSING in RWildFire");
            bad += 1;
            continue;
        };
        let mut inst = VariationInstance::new(name, AMOUNT);
        for t in tok {
            let (k, v) = t.split_once('=').unwrap();
            assert!(inst.set_param(k, v.parse().unwrap()), "{name}: unknown parameter {k}");
        }
        let d = variations::prepare_instance(def, &inst, &Affine::IDENTITY, 1);
        let mut diff = None;
        for (k, p) in POINTS.iter().enumerate() {
            let mut ctx = Ctx::new(1000 + k as u64, false);
            let mut a = Pt { x: p[0], y: p[1], z: p[2], color: p[3], ..Pt::default() };
            let mut v = Pt { x: 0.1, y: -0.05, z: 0.02, color: p[3], ..Pt::default() };
            (def.apply)(&d, &mut ctx, &mut a, &mut v, AMOUNT);
            let row = rows.iter().find(|r| r[1] == k.to_string()).unwrap();
            let want: Vec<f64> = row[2..].iter().map(|s| s.parse::<f64>().unwrap_or(f64::NAN)).collect();
            let got: Vec<f64> = fields(&a).into_iter().chain(fields(&v)).collect();
            for (i, (g, w)) in got.iter().zip(&want).enumerate() {
                // colors only matter when the point has a true color
                if (i % 9 >= 5 && i % 9 <= 7) && want[i - i % 9 + 4] == 0.0 {
                    continue;
                }
                if !close(*g, *w) && diff.is_none() {
                    diff = Some(format!("point {k} {}.{}: rust {g} java {w}", if i < 9 { "a" } else { "v" }, NAMES[i % 9]));
                }
            }
        }
        match diff {
            None => ok += 1,
            Some(d) => {
                bad += 1;
                println!("{name}: MISMATCH {d}");
            }
        }
    }
    println!("harness: {ok} ok, {bad} differ");
}
