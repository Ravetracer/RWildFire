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

//! JWildfire motion curves of all kinds are imported as animation tracks: flame, layer,
//! variation amount, relative rotate/scale and parent curves, with JWildfire's interpolations.

use rwf_core::animation::Interp;
use rwf_core::io::{read_flames, write_flame};

/// A linear curve `<name>` from (1, a) to (11, b).
fn curve(name: &str, a: f64, b: f64) -> String {
    format!(
        r#" {name}_enabled="true" {name}_interpolation="LINEAR" {name}_point_count="2" {name}_x0="1" {name}_y0="{a}" {name}_x1="11" {name}_y1="{b}""#
    )
}

fn flame() -> String {
    let flame_curves = [curve("foregroundOpacityCurve", 1.0, 0.0), curve("postSymmetryOrderCurve", 2.0, 6.0), curve("camDOFParam3Curve", 0.0, 1.0)].concat();
    // a SPLINE curve with a parent: the values add up
    let parent = r#" brightnessCurve_enabled="true" brightnessCurve_interpolation="SPLINE" brightnessCurve_point_count="3"
        brightnessCurve_x0="1" brightnessCurve_y0="1.0" brightnessCurve_x1="6" brightnessCurve_y1="3.0" brightnessCurve_x2="11" brightnessCurve_y2="2.0"
        brightnessCurve_parent_curve="parent42Curve" parent42Curve_enabled="false" parent42Curve_point_count="2"
        parent42Curve_x0="1" parent42Curve_y0="10.0" parent42Curve_x1="11" parent42Curve_y1="20.0""#;
    let xf_curves = [curve("xyRotateCurve", 0.0, 90.0), curve("xyPostScaleCurve", 1.0, 2.0), curve("opacityCurve", 1.0, 0.5), curve("materialSpeedCurve", 0.0, 4.0)].concat();
    let amount = curve("julian_amountCurve", 1.0, 0.0);
    let layer = curve("weightCurve", 1.0, 0.25);
    format!(
        r#"<jwf-flame name="anim" size="320 240" center="0.0 0.0" scale="80" brightness="4.0" gamma="4.0" frame_count="11" post_symmetry_type="POINT"{flame_curves}{parent}>
  <layer weight="1.0" density="1.0" visible="1"{layer}>
    <xform weight="1.0" color="0.0" symmetry="0.0" coefs="1 0 0 1 0.5 0" post="1 0 0 1 0 0" julian="1.0" julian_power="3" julian_dist="1.0"{amount}{xf_curves} />
    <palette count="1" format="RGB">FFFFFF</palette>
  </layer>
  <layer weight="1.0" density="1.0" visible="1">
    <xform weight="1.0" color="0.0" symmetry="0.0" coefs="1 0 0 1 0 0" linear3D="1.0" />
    <palette count="1" format="RGB">FFFFFF</palette>
  </layer>
</jwf-flame>"#
    )
}

#[test]
fn all_curve_kinds_are_imported() {
    let flame = read_flames(&flame()).unwrap().remove(0);
    let left: Vec<&String> = flame.extra_attrs.iter().map(|(k, _)| k).filter(|k| k.contains("Curve_")).collect();
    assert!(left.is_empty(), "flame curve attributes left over: {left:?}");
    let xf = &flame.layers[0].xforms[0];
    assert!(!xf.extra_attrs.iter().any(|(k, _)| k.contains("Curve_")), "{:?}", xf.extra_attrs);
    assert!(!xf.variations[0].raw_params.iter().any(|(k, _)| k.contains("amountCurve")), "{:?}", xf.variations[0].raw_params);

    let at = |f: f64| flame.at_frame(f);
    // flame curves
    assert!((at(6.0).fg_opacity - 0.5).abs() < 1e-9);
    assert_eq!(at(11.0).post_symmetry.order, 6);
    assert_eq!(at(4.0).post_symmetry.order, 3); // 3.2 rounds like JWildfire
    assert!((at(11.0).cam_dof_param3 - 1.0).abs() < 1e-9);
    // spline plus parent, baked per frame
    let b = flame.animation.track("brightness").unwrap();
    assert_eq!(b.interp, Interp::Linear);
    assert!((at(1.0).brightness - 11.0).abs() < 1e-9);
    assert!((at(11.0).brightness - 22.0).abs() < 1e-9);
    // layer, variation amount, xform scalars
    assert!((at(11.0).layers[0].weight - 0.25).abs() < 1e-9);
    assert!((at(11.0).layers[0].xforms[0].variations[0].amount).abs() < 1e-9);
    assert!((at(11.0).layers[0].xforms[0].opacity - 0.5).abs() < 1e-9);
    assert!((at(11.0).layers[0].xforms[0].material_speed - 4.0).abs() < 1e-9);
    // relative rotate (90 degrees at frame 11) and post scale
    let c = at(11.0).layers[0].xforms[0].coefs;
    let arr = c.as_array();
    for (v, e) in arr.iter().zip([0.0, -1.0, 1.0, 0.0, 0.5, 0.0]) {
        assert!((v - e).abs() < 1e-9, "{arr:?}");
    }
    let p = at(11.0).layers[0].xforms[0].post.as_array();
    assert!((p[0] - 2.0).abs() < 1e-9 && (p[3] - 2.0).abs() < 1e-9, "{p:?}");
    assert_eq!(at(1.0).layers[0].xforms[0].coefs.as_array()[0], 1.0);

    // RWildFire's own format keeps all of it
    let again = read_flames(&write_flame(&flame)).unwrap().remove(0);
    for f in [1.0, 4.5, 11.0] {
        let (mut a, mut b) = (again.at_frame(f), flame.at_frame(f));
        // the writer adds attributes of its own (filter type, temporal samples)
        a.extra_attrs.clear();
        b.extra_attrs.clear();
        assert_eq!(a, b, "frame {f}");
    }
}

#[test]
fn spline_curves_keep_jwildfire_interpolation() {
    let src = r#"<flame name="s" size="320 240" center="0 0" scale="80" camZoomCurve_enabled="true" camZoomCurve_point_count="3"
        camZoomCurve_x0="1" camZoomCurve_y0="1.0" camZoomCurve_x1="20" camZoomCurve_y1="2.0" camZoomCurve_x2="40" camZoomCurve_y2="1.5"
        camZoomCurve_interpolation="BEZIER">
      <xform weight="1.0" color="0" coefs="1 0 0 1 0 0" linear3D="1.0" />
      <palette count="1" format="RGB">FFFFFF</palette>
    </flame>"#;
    let flame = read_flames(src).unwrap().remove(0);
    let t = flame.animation.track("cam_zoom").unwrap();
    assert_eq!(t.interp, Interp::Bezier);
    assert_eq!(t.keys.len(), 3);
    // the Bezier curve passes below the middle key
    assert!(t.eval(20.0).unwrap() < 2.0);
}
