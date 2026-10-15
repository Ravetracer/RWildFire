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

//! Resolution and quality profiles (JWildfire's `ResolutionProfile`, `QualityProfile`):
//! named render sizes and qualities, kept in the preferences, chosen in the export dialog and
//! the batch renderer; a flame remembers the profiles last chosen for it.

use egui::Ui;

#[derive(Clone, Debug, PartialEq)]
pub struct ResolutionProfile {
    pub width: u32,
    pub height: u32,
    /// Size of new flames.
    pub default: bool,
}

impl ResolutionProfile {
    /// The name stored in flames (`1920x1080`).
    pub fn name(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }

    pub fn to_line(&self) -> String {
        format!("{}|{}", self.name(), self.default as u8)
    }

    pub fn from_line(v: &str) -> Option<Self> {
        let (size, default) = v.split_once('|').unwrap_or((v, "0"));
        let (w, h) = size.trim().split_once('x')?;
        Some(Self { width: w.parse().ok()?, height: h.parse().ok()?, default: default.trim() == "1" })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct QualityProfile {
    pub caption: String,
    pub quality: f64,
    /// Also selects HDR output (Radiance `.hdr`) in the export dialog.
    pub hdr: bool,
    pub zbuffer: bool,
    pub default: bool,
}

impl QualityProfile {
    pub fn to_line(&self) -> String {
        format!("{}|{}|{}|{}|{}", self.caption.replace('|', "/"), self.quality, self.hdr as u8, self.zbuffer as u8, self.default as u8)
    }

    pub fn from_line(v: &str) -> Option<Self> {
        let mut p = v.split('|');
        Some(Self {
            caption: p.next()?.to_string(),
            quality: p.next()?.trim().parse().ok()?,
            hdr: p.next() == Some("1"),
            zbuffer: p.next() == Some("1"),
            default: p.next() == Some("1"),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Profiles {
    pub resolutions: Vec<ResolutionProfile>,
    pub qualities: Vec<QualityProfile>,
    /// Flames remember their profiles and the export dialog starts with them
    /// (`tinaAssociateProfilesWithFlames`).
    pub associate: bool,
}

impl Default for Profiles {
    /// JWildfire's default profiles (with 1920x1080 as the default size of new flames).
    fn default() -> Self {
        let r = |w, h, default| ResolutionProfile { width: w, height: h, default };
        let q = |c: &str, quality, hdr, default| QualityProfile { caption: c.to_string(), quality, hdr, zbuffer: false, default };
        Self {
            resolutions: vec![r(640, 480, false), r(800, 600, false), r(1024, 768, false), r(1440, 900, false), r(1680, 1050, false), r(1920, 1080, true)],
            qualities: vec![
                q("Very low quality", 50.0, false, false),
                q("Low quality", 100.0, false, false),
                q("Medium quality", 200.0, false, false),
                q("High quality", 320.0, false, true),
                q("Very high quality", 500.0, true, false),
            ],
            associate: true,
        }
    }
}

impl Profiles {
    pub fn resolution(&self, name: &str) -> Option<&ResolutionProfile> {
        self.resolutions.iter().find(|r| r.name() == name)
    }

    pub fn quality(&self, caption: &str) -> Option<&QualityProfile> {
        self.qualities.iter().find(|q| q.caption == caption)
    }

    pub fn default_resolution(&self) -> Option<&ResolutionProfile> {
        self.resolutions.iter().find(|r| r.default)
    }
}

/// A resolution profile combo; returns the chosen profile. `current`: the size shown, which
/// selects the matching profile ("Custom" otherwise).
pub fn resolution_combo(ui: &mut Ui, id: &str, profiles: &Profiles, current: (u32, u32)) -> Option<ResolutionProfile> {
    let selected = profiles.resolutions.iter().find(|r| (r.width, r.height) == current).map_or("Custom".to_string(), ResolutionProfile::name);
    let mut picked = None;
    egui::ComboBox::from_id_salt(id).selected_text(selected).show_ui(ui, |ui| {
        for r in &profiles.resolutions {
            if ui.selectable_label((r.width, r.height) == current, r.name()).clicked() {
                picked = Some(r.clone());
            }
        }
    });
    picked
}

/// A quality profile combo; returns the chosen profile. `current`: the caption shown.
pub fn quality_combo(ui: &mut Ui, id: &str, profiles: &Profiles, current: Option<&str>) -> Option<QualityProfile> {
    let mut picked = None;
    egui::ComboBox::from_id_salt(id).selected_text(current.unwrap_or("Custom")).show_ui(ui, |ui| {
        for q in &profiles.qualities {
            if ui.selectable_label(current == Some(q.caption.as_str()), format!("{} ({})", q.caption, q.quality)).clicked() {
                picked = Some(q.clone());
            }
        }
    });
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_round_trip() {
        let p = Profiles::default();
        for r in &p.resolutions {
            assert_eq!(ResolutionProfile::from_line(&r.to_line()).as_ref(), Some(r));
        }
        for q in &p.qualities {
            assert_eq!(QualityProfile::from_line(&q.to_line()).as_ref(), Some(q));
        }
        assert_eq!(p.default_resolution().map(ResolutionProfile::name).as_deref(), Some("1920x1080"));
        assert!(ResolutionProfile::from_line("junk").is_none());
    }
}
