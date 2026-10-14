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

//! JWildfire motion curves of weighting field settings become animation tracks that change the
//! field per frame and survive a save/load round trip.

use rwf_core::io::{read_flames, write_flame};

const FLAME: &str = r#"<flame name="wf" size="320 240" center="0.0 0.0" scale="80" oversample="1" filter="1.0" quality="50" background="0 0 0" brightness="4.0" gamma="4.0">
  <xform weight="1.0" color="0.0" symmetry="0.0" coefs="0.5 0 0 0.5 0 0" julian="1.0" julian_power="3" julian_dist="1.0"
    wfield_type="PERLIN_NOISE" wfield_input="AFFINE" wfield_color_intensity="0.5" wfield_var_amount_intensity="0.0"
    wfield_var_param1_intensity="1.0" wfield_var_param1_var_name="julian" wfield_var_param1_param_name="dist"
    wfield_noise_seed="1337" wfield_noise_frequency="1.0"
    weightingFieldNoiseFrequencyCurve_enabled="true" weightingFieldNoiseFrequencyCurve_interpolation="LINEAR"
    weightingFieldNoiseFrequencyCurve_point_count="2" weightingFieldNoiseFrequencyCurve_x0="1" weightingFieldNoiseFrequencyCurve_y0="1.0"
    weightingFieldNoiseFrequencyCurve_x1="11" weightingFieldNoiseFrequencyCurve_y1="3.0"
    weightingFieldVarParam1IntensityCurve_enabled="true" weightingFieldVarParam1IntensityCurve_point_count="2"
    weightingFieldVarParam1IntensityCurve_x0="1" weightingFieldVarParam1IntensityCurve_y0="1.0"
    weightingFieldVarParam1IntensityCurve_x1="11" weightingFieldVarParam1IntensityCurve_y1="0.0" />
  <palette count="256" format="RGB">
FFFFFF
  </palette>
</flame>"#;

#[test]
fn weighting_field_curves_animate() {
    let flame = read_flames(FLAME).unwrap().remove(0);
    let xf = &flame.layers[0].xforms[0];
    assert!(!xf.extra_attrs.iter().any(|(k, _)| k.starts_with("weightingField")), "curve attributes left over: {:?}", xf.extra_attrs);
    let freq = flame.animation.track("layer0.xf0.wfield.noise_frequency").expect("frequency track");
    assert_eq!(freq.keys.len(), 2);
    assert!(flame.animation.track("layer0.xf0.wfield.param1_intensity").is_some());

    let mid = flame.at_frame(6.0);
    let w = &mid.layers[0].xforms[0].weighting;
    assert!((w.noise_frequency - 2.0).abs() < 1e-9, "{}", w.noise_frequency);
    assert!(w.var_params[0].intensity < 1.0 && w.var_params[0].intensity > 0.0);

    // Own format round trip keeps the tracks.
    let again = read_flames(&write_flame(&flame)).unwrap().remove(0);
    assert_eq!(again.animation.track("layer0.xf0.wfield.noise_frequency").map(|t| t.keys.len()), Some(2));
}
