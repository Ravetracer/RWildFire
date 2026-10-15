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

//! The suggested z-buffer parameters spread the depths of a render over the gray range.


//! JWildfire's `resolution_profile` and `quality_profile` flame attributes.

use rwf_core::io::{read_flames, write_flame};

#[test]
fn profiles_round_trip_and_null_is_empty() {
    let src = r#"<flame size="640 480" resolution_profile="1920x1080" quality_profile="High quality"><xform weight="1" coefs="1 0 0 1 0 0" linear="1"/></flame>"#;
    let f = &read_flames(src).unwrap()[0];
    assert_eq!((f.resolution_profile.as_str(), f.quality_profile.as_str()), ("1920x1080", "High quality"));
    let back = &read_flames(&write_flame(f)).unwrap()[0];
    assert_eq!((back.resolution_profile.as_str(), back.quality_profile.as_str()), ("1920x1080", "High quality"));
    assert!(back.extra_attrs.iter().all(|(k, _)| !k.ends_with("_profile")));

    let jwf = r#"<flame size="640 480" resolution_profile="512x512" quality_profile="null"><xform weight="1" coefs="1 0 0 1 0 0" linear="1"/></flame>"#;
    let f = &read_flames(jwf).unwrap()[0];
    assert_eq!(f.quality_profile, "");
    assert!(!write_flame(f).contains("quality_profile"));
}
