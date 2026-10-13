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

//! WGSL helper libraries appended to the prelude (see `wgsl/*.wgsl` and `PORTING.md`).
//!
//! | File                     | Prefix                          | Rust counterpart              |
//! |--------------------------|---------------------------------|-------------------------------|
//! | `wgsl/math.wgsl`         | `math_`, `java_`, `gfx_`, `ftoi`, `limit_value`, `erf`, `lgamma` | `crate::math` |
//! | `wgsl/glsl_complex.wgsl` | `c_` (+ `glsl_arg`)             | `glsl::c_*` (`G.c_*`)         |
//! | `wgsl/glsl.wgsl`         | `glsl_`, `dc_`                  | `glsl` (`G.*`, `DC_BaseFunc`) |
//! | `wgsl/complex.wgsl`      | `cx_`                           | `complex::Complex` methods    |
//! | `wgsl/noise.wgsl`        | `noise_`, `NOISE_`              | `noise` (`NoiseTools`, `VoronoiTools`) |
//! | `wgsl/resource.wgsl`     | `res_`                          | `resource_vars` (images, meshes) |

pub static LIBS: &[&str] = &[
    include_str!("wgsl/math.wgsl"),
    include_str!("wgsl/glsl_complex.wgsl"),
    include_str!("wgsl/glsl.wgsl"),
    include_str!("wgsl/complex.wgsl"),
    include_str!("wgsl/noise.wgsl"),
    include_str!("wgsl/resource.wgsl"),
];

#[cfg(test)]
mod tests {
    use super::LIBS;
    use crate::variations::wgsl::{prelude, validate};

    /// Calls every library function with runtime arguments, so the bodies are type-checked
    /// with realistic argument types.
    const CALLS: &str = r#"
  let x = prm[0];
  let y = prm[1];
  let z = prm[2];
  let n = i32(prm[3]);
  let v2 = vec2<f32>(x, y);
  let w2 = vec2<f32>(y, z);
  let v3 = vec3<f32>(x, y, z);
  let v4 = vec4<f32>(x, y, z, prm[3]);
  var acc = 0.0;
  // math.wgsl
  acc += math_inf() + math_nan() + f32(ftoi(x)) + limit_value(x, y, z) + f32(limit_value_i(n, 0, 3));
  acc += math_jacobi_elliptic(x, y).x;
  acc += java_round(x) + java_pow(x, y) + gfx_lerp(x, y, z) + gfx_blerp(x, y, z, x, y, z);
  acc += gfx_smootherstep(x, y, z) + gfx_smootheststep(x, y, z) + erf(x) + lgamma(y);
  // glsl_complex.wgsl
  acc += c_one().x + c_i().y + c_ni().y + glsl_arg(v2) + c_conj(v2).y + c_from_polar(x, y).x;
  acc += c_to_polar(v2).y + c_exp(v2).x + c_exp_base(x, v2).y + c_ln(v2).x + c_log(v2, z).y;
  acc += c_sqrt(v2).x + c_pow(v2, z).x + c_pow_c(v2, w2).y + c_add(v2, w2).x + c_sub(v2, w2).y;
  acc += c_mul(v2, w2).x + c_div(v2, w2).y + c_sin(v2).x + c_cos(v2).y + c_tan(v2).x;
  if (c_eq(v2, w2)) { acc += 1.0; }
  acc += c_atan(v2).x + c_asin(v2).y + c_acos(v2).x + c_sinh(v2).y + c_cosh(v2).x + c_tanh(v2).y;
  acc += c_asinh(v2).x + c_acosh(v2).y + c_atanh(v2).x + c_rem(v2, w2).y + c_inv(v2).x;
  // glsl.wgsl
  acc += glsl_mod(x, y) + glsl_mod2(v2, w2).x + glsl_mod3(v3, v3).y + glsl_mod4(v4, v4).z;
  acc += glsl_atan(x, y) + glsl_atan2(x, y) + glsl_round(x) + glsl_round2(v2).y + glsl_round3(v3).z;
  acc += glsl_smoothstep(x, y, z) + glsl_smoothstep2(v2, w2, v2).x + glsl_smoothstep3(v3, v3, v3).y;
  acc += glsl_inv_sqrt(x) + glsl_sinh2(v2).x + glsl_cosh2(v2).y + glsl_tanh2(v2).x;
  acc += glsl_plot(v2, x) + glsl_circle(v2, y) + glsl_rotate2d(v2, z).x + glsl_tile(v2, z).y;
  acc += glsl_brick_tile(v2, z).x + glsl_truchet_pattern(v2, z).y + glsl_box(v2, w2, z);
  acc += glsl_skew(v2).x + glsl_simplex_grid(v2).y + glsl_random(v2) + glsl_random2(v2).x;
  acc += glsl_hash3(v2).z + glsl_iqnoise(v2, x, y) + glsl_noise(v2) + glsl_fbm(v2) + glsl_fbm2(v2);
  acc += glsl_fractal(v2).x + glsl_b(v2).y + glsl_big_f(v2, z).z;
  acc += glsl_rgb2hsb(v3).x + glsl_hsb2rgb(v3).y + glsl_rgb2hsv(v4).z + glsl_hsv2rgb(v4).w;
  acc += glsl_colorize(x, v3, v3, v3, v3).x;
  acc += glsl_ccon(v2).y + glsl_cmul(v2, w2).x + glsl_cdiv(v2, w2).y + glsl_cabs(v2) + glsl_carg(v2);
  acc += glsl_cpow(v2, w2).x + glsl_cexp(v2).y + glsl_clog(v2).x + glsl_f(v2, z).y;
  acc += glsl_kscope(v2, z).x + glsl_v(v2, x, y, z);
  let m3 = glsl_rot(v3);
  acc += m3[1].y + glsl_app(v3, x, m3).z;
  acc += glsl_vec4_times_mat4(v4, mat4x4<f32>(v4, v4, v4, v4)).w;
  acc += glsl_dbl2int(v3).x + dc_greyscale(v3) + dc_find_key(v3).y;
  var pt = Pt(x, y, z, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
  dc_output(&pt, x, v2, v3, n, y, z, n);
  acc += pt.x + pt.z + pt.r + pt.color;
  // complex.wgsl
  acc += cx_flip(v2).x + cx_conj(v2).y + cx_neg(v2).x + cx_sig(v2) + cx_sig2(v2);
  acc += cx_mag2(v2) + cx_mag2eps(v2) + cx_mag_inv(v2);
  acc += cx_sqr(v2).x + cx_recip(v2).y + cx_scale(v2, z).x + cx_mul(v2, w2).y + cx_div(v2, w2).x;
  acc += cx_div_r(v2, w2).y + cx_add(v2, w2).x + cx_sub(v2, w2).y + cx_sub_r(v2, w2).x;
  acc += cx_inc(v2).x + cx_dec(v2).x + cx_radius(v2) + cx_arg(v2) + cx_to_p(v2).y + cx_un_p(v2).x;
  acc += cx_norm(v2).y + cx_sqrt(v2).x + cx_pow(v2, z).y + cx_exp(v2).x + cx_log(v2).y;
  acc += cx_cpow(v2, w2).x + cx_next_pow(v2, w2).y + cx_sinh(v2).x + cx_cosh(v2).y;
  acc += cx_sin(v2).x + cx_cos(v2).y + cx_atanh(v2).x + cx_asinh(v2).y + cx_acosh(v2).x;
  acc += cx_acoth(v2).y + cx_asech(v2).x + cx_acosech(v2).y + cx_atan(v2).x + cx_asin(v2).y;
  acc += cx_acos(v2).x + cx_a_mean(v2, w2).y + cx_root_mean_s(v2, w2).x + cx_g_mean(v2, w2).y;
  acc += cx_heronian(v2, w2).x + cx_h_mean(v2, w2).y + cx_l_mean(v2, w2).x;
  if (glsl_is_nan(x)) { acc += 1.0; }
  // noise.wgsl
  acc += f32(noise_perm(n)) + noise_simplex_corner(v3, n) + noise_simplex3d(v3);
  acc += noise_perlin3d(v3, x, y, n) + noise_vratio(v2, w2, v2);
  var pts: array<vec2<f32>, NOISE_VORONOI_MAXPOINTS>;
  pts[0] = v2;
  pts[1] = w2;
  acc += f32(noise_closest(pts, n, v2)) + noise_voronoi(pts, n, 0, v2);
  // resource.wgsl
  let ub = u32(n);
  acc += res_pixel(ub, 1u, n, n).x + res_sample(ub, 1u, x > 0.0, x, y).w + res_image_color(ub, 1u, true, x, y).y;
  acc += res_image_displ(ub, 2u, false, x, y) + res_uv_color_idx(v3);
  var mp: Pt;
  acc += res_mesh(ub, 3u, mp, x).x;
  acc += f32(res_wrap_tile(n, 3)) + res_cm_rgba(ub, n, n, x > 0.0).w + res_nearest_color_idx(v3);
  acc += res_colormap(ub, x, y, mp).x + res_displacemap(ub, x, y, z, mp).y;
  acc += res_discret_noise(n) + res_brush(ub, mp, mp, x, y, z, x > 0.0).x;
  acc += res_metaballs(ub, mp, x).z;
  acc += res_shape_point(ub, 2u, x).w;
  acc += res_ngon_point(n, x).x + res_triangle_point(v2, w2, v2).y;
  acc += res_formula(ub, 4u, v4);
  acc += res_f(ub, 2u, v4) + res_normal(v3, v3).x + res_plot_displ(ub, 1u, x > 0.0, x, y) + res_plot_color(ub, 1u, true, x, y, mp).r;
  prm[4] = acc;
"#;

    fn lib_function_names() -> Vec<String> {
        let mut names = Vec::new();
        for lib in LIBS {
            for line in lib.lines() {
                if let Some(rest) = line.strip_prefix("fn ") {
                    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                    names.push(name);
                }
            }
        }
        names
    }

    #[test]
    fn every_library_function_compiles_when_called() {
        let src = format!(
            "{}\n@group(0) @binding(0) var<storage, read_write> prm: array<f32>;\n@compute @workgroup_size(1)\nfn main() {{\n  rng_state = 1u;\n  cmap_base = 0u;\n{}}}\n",
            prelude(),
            CALLS
        );
        if let Err(e) = validate(&src) {
            panic!("library test shader invalid:\n{e}");
        }
    }

    #[test]
    fn test_shader_calls_every_library_function() {
        let names = lib_function_names();
        assert!(names.len() > 150, "found only {} library functions", names.len());
        let missing: Vec<&String> = names.iter().filter(|n| !CALLS.contains(&format!("{n}("))).collect();
        assert!(missing.is_empty(), "library functions not exercised by CALLS: {missing:?}");
    }

    /// Library names must carry a library prefix so they cannot clash with variation helpers.
    #[test]
    fn library_function_names_are_prefixed() {
        const PREFIXES: &[&str] = &["glsl_", "c_", "cx_", "dc_", "noise_", "math_", "java_", "gfx_", "res_"];
        const PLAIN: &[&str] = &["ftoi", "limit_value", "limit_value_i", "erf", "lgamma"];
        let bad: Vec<String> = lib_function_names()
            .into_iter()
            .filter(|n| !PREFIXES.iter().any(|p| n.starts_with(p)) && !PLAIN.contains(&n.as_str()))
            .collect();
        assert!(bad.is_empty(), "unprefixed library functions: {bad:?}");
    }
}
