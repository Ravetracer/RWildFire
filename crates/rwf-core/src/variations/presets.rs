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

//! Formula presets of the plot variations (`WFFuncPresets` and the `*_presets.txt` files).
//!
//! A preset sets the formula resources and numeric parameters of a variation; the `preset_id`
//! parameter selects it.

use std::sync::OnceLock;

use crate::model::VariationInstance;

/// A parsed preset: formula texts and numeric values, in the order of [`PresetSet::formulas`]
/// and [`PresetSet::values`].
#[derive(Clone, Debug)]
pub struct Preset {
    pub id: i32,
    /// First line after the id (e.g. "Borg cube").
    pub name: String,
    pub formulas: Vec<String>,
    pub values: Vec<f64>,
}

struct PresetSet {
    variation: &'static str,
    text: &'static str,
    /// Formula keys (also the resource names).
    formulas: &'static [&'static str],
    /// Numeric keys and the parameters they set; missing values are 0.0 as in JWildfire.
    values: &'static [(&'static str, &'static str)],
    /// Parameter holding the preset id.
    id_param: &'static str,
}

const P6: [&str; 6] = ["param_a", "param_b", "param_c", "param_d", "param_e", "param_f"];

macro_rules! keys {
    ($($k:literal),* ; params) => { &[$(($k, $k)),*, ("param_a", "param_a"), ("param_b", "param_b"), ("param_c", "param_c"), ("param_d", "param_d"), ("param_e", "param_e"), ("param_f", "param_f")] };
}

const PARAMS_AH: [(&str, &str); 8] = [
    ("param_a", "param_a"),
    ("param_b", "param_b"),
    ("param_c", "param_c"),
    ("param_d", "param_d"),
    ("param_e", "param_e"),
    ("param_f", "param_f"),
    ("param_g", "param_g"),
    ("param_h", "param_h"),
];
const fn with_ah<const N: usize, const M: usize>(head: [(&'static str, &'static str); N]) -> [(&'static str, &'static str); M] {
    let mut out = [("", ""); M];
    let mut i = 0;
    while i < N {
        out[i] = head[i];
        i += 1;
    }
    while i < M {
        out[i] = PARAMS_AH[i - N];
        i += 1;
    }
    out
}
static KNOTS_VALUES: [(&str, &str); 11] = with_ah([("steps", "steps"), ("radius", "radius"), ("facets", "facets")]);
static SATTRACTOR_VALUES: [(&str, &str); 14] = with_ah([
    ("startx", "start_x"),
    ("starty", "start_y"),
    ("startz", "start_z"),
    ("steps", "steps"),
    ("radius", "radius"),
    ("steptime", "stepTime"),
]);

static SETS: &[PresetSet] = &[
    PresetSet {
        variation: "parplot2d_wf",
        text: include_str!("../../assets/presets/parplot2d_wf_presets.txt"),
        formulas: &["xformula", "yformula", "zformula"],
        id_param: "preset_id",
        values: keys!("umin", "umax", "vmin", "vmax"; params),
    },
    PresetSet {
        variation: "polarplot2d_wf",
        text: include_str!("../../assets/presets/polarplot2d_wf_presets.txt"),
        formulas: &["formula"],
        id_param: "preset_id",
        values: keys!("tmin", "tmax"; params),
    },
    PresetSet {
        variation: "polarplot3d_wf",
        text: include_str!("../../assets/presets/polarplot3d_wf_presets.txt"),
        formulas: &["formula"],
        id_param: "preset_id",
        values: keys!("tmin", "tmax", "umin", "umax", "cylindrical"; params),
    },
    PresetSet {
        variation: "yplot2d_wf",
        text: include_str!("../../assets/presets/yplot2d_wf_presets.txt"),
        formulas: &["formula"],
        id_param: "preset_id",
        values: keys!("xmin", "xmax"; params),
    },
    PresetSet {
        variation: "yplot3d_wf",
        text: include_str!("../../assets/presets/yplot3d_wf_presets.txt"),
        formulas: &["formula"],
        id_param: "preset_id",
        values: keys!("xmin", "xmax", "zmin", "zmax"; params),
    },
    PresetSet {
        variation: "isosfplot3d_wf",
        text: include_str!("../../assets/presets/isosfplot3d_wf_presets.txt"),
        formulas: &["formula"],
        id_param: "preset_id",
        values: keys!("xmin", "xmax", "ymin", "ymax", "zmin", "zmax"; params),
    },
    PresetSet {
        variation: "knots3D",
        text: include_str!("../../assets/presets/knots3d_wf_presets.txt"),
        formulas: &["xformula", "yformula", "zformula"],
        id_param: "presetId",
        values: &KNOTS_VALUES,
    },
    PresetSet {
        variation: "sattractor3D",
        text: include_str!("../../assets/presets/sattractor3d_wf_presets.txt"),
        formulas: &["xformula", "yformula", "zformula"],
        id_param: "presetId",
        values: &SATTRACTOR_VALUES,
    },
];

/// `WFFuncPresets.parseToken`: the value of the first line starting with `token`, with a
/// trailing `---` comment removed.
fn token(block: &str, token: &str) -> Option<String> {
    let p1 = block.find(&format!("\n{token}"))?;
    let rest = &block[p1 + 1..];
    let line = rest.split('\n').next().unwrap_or("");
    let value = &line[line.find('=')? + 1..];
    let value = match value.find("---") {
        Some(p) if p > 0 => &value[..p],
        _ => value,
    };
    Some(value.trim().to_string())
}

fn parse(set: &PresetSet) -> Vec<Preset> {
    let mut out: Vec<Preset> = Vec::new();
    let text = set.text.replace("\r\n", "\n");
    let starts: Vec<usize> = text.match_indices("##").map(|(i, _)| i).collect();
    for (k, &start) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(text.len());
        let block = &text[start.saturating_sub(1)..end];
        let mut lines = block[block.find("##").unwrap_or(0) + 2..].split('\n');
        let Some(Ok(id)) = lines.next().map(|l| l.trim().parse::<i32>()) else {
            continue;
        };
        if out.iter().any(|p| p.id == id) {
            continue;
        }
        let name = lines.next().unwrap_or("").trim().to_string();
        let formulas = set.formulas.iter().map(|f| token(block, f).unwrap_or_default()).collect();
        let values = set.values.iter().map(|(k, _)| token(block, k).and_then(|s| crate::formula::eval_constant(&s)).unwrap_or(0.0)).collect();
        out.push(Preset { id, name, formulas, values });
    }
    out
}

fn set_of(variation: &str) -> Option<(usize, &'static PresetSet)> {
    SETS.iter().enumerate().find(|(_, s)| s.variation == variation)
}

/// The presets of a variation (sorted by id), if it has any.
pub fn presets(variation: &str) -> Option<&'static [Preset]> {
    static PARSED: OnceLock<Vec<Vec<Preset>>> = OnceLock::new();
    let (i, _) = set_of(variation)?;
    let all = PARSED.get_or_init(|| {
        SETS.iter()
            .map(|s| {
                let mut p = parse(s);
                p.sort_by_key(|p| p.id);
                p
            })
            .collect()
    });
    Some(&all[i])
}

/// Applies preset `id` (`refreshFormulaFromPreset`): sets the formulas, ranges and `param_*`
/// values and `preset_id`. Returns false if the variation has no such preset.
pub fn apply(inst: &mut VariationInstance, id: i32) -> bool {
    let Some((_, set)) = set_of(&inst.name) else {
        return false;
    };
    let Some(p) = presets(&inst.name).and_then(|ps| ps.iter().find(|p| p.id == id)) else {
        return false;
    };
    for (k, f) in set.formulas.iter().zip(&p.formulas) {
        inst.set_resource(k, f);
    }
    for ((_, param), v) in set.values.iter().zip(&p.values) {
        inst.set_param(param, *v);
    }
    inst.set_param(set.id_param, id as f64);
    true
}

/// Name of the parameter holding the preset id of a variation with presets.
pub fn id_param(variation: &str) -> Option<&'static str> {
    set_of(variation).map(|(_, s)| s.id_param)
}

/// Parameter names of the `param_*` group (for UIs).
pub fn param_names() -> &'static [&'static str] {
    &P6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_preset_files_parse_and_compile() {
        for set in SETS {
            let ps = presets(set.variation).unwrap();
            assert!(ps.len() >= 3, "{}", set.variation);
            let vars = ["u", "v", "t", "x", "y", "z", "delta_t"];
            for p in ps {
                let mut consts = std::collections::HashMap::new();
                consts.insert("pi".to_string(), std::f64::consts::PI);
                for ((k, _), v) in set.values.iter().zip(&p.values) {
                    consts.insert(k.to_string(), *v);
                }
                for f in &p.formulas {
                    crate::formula::compile(f, &vars, &consts).unwrap_or_else(|e| panic!("{} #{}: {f}: {e}", set.variation, p.id));
                }
            }
        }
    }

    #[test]
    fn preset_values() {
        let p = &presets("isosfplot3d_wf").unwrap()[0];
        assert_eq!((p.id, p.name.as_str()), (0, "Borg cube"));
        assert_eq!(p.formulas[0], "sin(param_a * x * y) + sin(param_b * y * z) + sin(param_c * z * x)");
        assert!((p.values[0] + std::f64::consts::PI).abs() < 1e-15);
    }
}
