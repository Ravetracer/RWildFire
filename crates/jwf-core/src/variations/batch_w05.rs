//! Variations ported from the JWildfire sources (batch w05): the `cut_*` family (2D "cut-out"
//! patterns by Jesus Sosa).
//!
//! All of them sample a point (the affine input for `mode == 0`, a random point otherwise),
//! evaluate a 2D pattern there and either output the point (`pVarTP.x = amount * x`, an
//! assignment, not `+=`) or hide it (`doHide`, coordinates set to 0).
//!
//! The WGSL snippets mirror this with the GPU hide flag (`v.hide`).

#[allow(unused_imports)]
use super::glsl::*;
#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

/// Random sampling range of `mode != 0`.
#[derive(Clone, Copy)]
enum Sample {
    /// `random() - 0.5`
    Centered,
    /// `random()`
    Unit,
    /// `2 * random() - 1`
    Wide,
}

/// The sampled point: the affine input for `mode == 0`, a random point otherwise.
#[inline]
fn cut_input(ctx: &mut Ctx, a: &Pt, mode: f64, s: Sample) -> (f64, f64) {
    if mode as i32 == 0 {
        return (a.x, a.y);
    }
    let x = ctx.random();
    let y = ctx.random();
    match s {
        Sample::Centered => (x - 0.5, y - 0.5),
        Sample::Unit => (x, y),
        Sample::Wide => (2.0 * x - 1.0, 2.0 * y - 1.0),
    }
}

/// The common tail: `pVarTP.doHide`, `pVarTP.x = amount * (x - center)` and z pass-through.
#[inline]
fn cut_output(ctx: &Ctx, a: &Pt, v: &mut Pt, amount: f64, (x, y): (f64, f64), center: f64, hide: bool) {
    v.do_hide = hide;
    let (x, y) = if hide { (0.0, 0.0) } else { (x, y) };
    v.x = amount * (x - center);
    v.y = amount * (y - center);
    ctx.pass_z(a, v, amount);
}

/// `invert == 0`: hide if `color > t`, otherwise hide if `color <= t`.
#[inline]
fn hide_gt(invert: f64, color: f64, t: f64) -> bool {
    if invert as i32 == 0 { color > t } else { color <= t }
}

/// `invert == 0`: hide if `color == 0`, otherwise hide if `color > 0`.
#[inline]
fn hide_zero(invert: f64, color: f64) -> bool {
    if invert as i32 == 0 { color == 0.0 } else { color > 0.0 }
}

/// `(int) Tools.limitValue(v, lo, hi)`
#[inline]
fn lim_i(v: f64, lo: f64, hi: f64) -> f64 {
    limit_value(v, lo, hi) as i32 as f64
}

/// `(int) v`
#[inline]
fn to_i(v: f64) -> f64 {
    v as i32 as f64
}

/// `java.util.Random` (for the deterministic `init()` offsets).
struct JavaRandom(i64);

impl JavaRandom {
    const MASK: i64 = (1 << 48) - 1;
    fn new(seed: i64) -> Self {
        Self((seed ^ 0x5_DEEC_E66D) & Self::MASK)
    }
    fn next(&mut self, bits: u32) -> i64 {
        self.0 = (self.0.wrapping_mul(0x5_DEEC_E66D).wrapping_add(0xB)) & Self::MASK;
        (self.0 >> (48 - bits)) as i32 as i64
    }
    fn next_double(&mut self) -> f64 {
        ((self.next(26) << 27) + self.next(27)) as f64 * (1.0 / (1i64 << 53) as f64)
    }
}

/// Splits an offset into integer and fractional part (for f32 precision on the GPU).
#[inline]
fn split(v: f64) -> [f64; 2] {
    let i = v.floor();
    [i, v - i]
}

/// The WGSL body shared by all `cut_*` snippets: sampling, `$hide` (a WGSL bool expression of
/// the sampled point `x`, `y`) and the output tail with the hide flag.
macro_rules! cut_wgsl {
    ($helpers:literal, mode = $mode:literal, sample = $sample:literal, center = $center:literal, hide = $hide:literal) => {
        concat!(
            $helpers,
            "
@body
var x = a.x;
var y = a.y;
if (i32(",
            $mode,
            ") != 0) {
  x = ",
            $sample,
            ";
  y = ",
            $sample,
            ";
}
v.hide = 0.0;
if (",
            $hide,
            ") {
  x = 0.0;
  y = 0.0;
  v.hide = 1.0;
}
v.x = AMOUNT * (x - ",
            $center,
            ");
v.y = AMOUNT * (y - ",
            $center,
            ");
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        )
    };
}

// ---------------------------------------------------------------------------------------------
// cut_mandala
// ---------------------------------------------------------------------------------------------

#[inline]
fn mandala_ss(width: f64, z: f64) -> f64 {
    smoothstep(1.0, 0.0, width * z.abs())
}

fn mandala_circle(p: Vec2, r: f64, width: f64) -> f64 {
    mandala_ss(width, p.x - r)
}

/// `rose` (`use_cos == false`) and `rose2` (`use_cos == true`).
fn mandala_rose(p: Vec2, t: f64, width: f64, use_cos: bool) -> f64 {
    let px = p.x * (7.0 + 8.0 * t);
    let s = if use_cos { (6.0 * p.y).cos() } else { (6.0 * p.y).sin() };
    mandala_ss(width, px - s) + mandala_ss(width, px - s.abs()) + mandala_ss(width, px.abs() - s) + mandala_ss(width, px.abs() - s.abs())
}

fn mandala_spiral(p: Vec2, width: f64) -> f64 {
    let q = 0.5 * p.y / M_PI;
    let qa = 0.5 * p.y.abs() / M_PI;
    mandala_ss(width, p.x - q) + mandala_ss(width, p.x - qa) + mandala_ss(width, p.x.abs() - q) + mandala_ss(width, p.x.abs() - qa)
}

fn mandala_color(x: f64, y: f64, time: f64, zoom: f64) -> f64 {
    let p = vec2(x * zoom, y * zoom);
    let mut f = vec2((p.x * p.x + p.y * p.y).sqrt(), glsl_atan2(p.y, p.x));
    let t0 = (0.3 * time).cos();
    let t1 = 0.5 + 0.5 * (0.3 * time).cos();
    let (mut m0, mut m1, mut m2, mut m3, mut m4) = (0.0, 0.0, 0.0, 0.0, 0.0);
    if f.x < 0.7325 {
        f.y += 0.1 * time;
        let c = vec2(0.225 - 0.1 * t0, M_PI / 4.0);
        if f.x < 0.25 {
            for _ in 0..2 {
                let f2 = glsl_mod(f, c) - c * 0.5;
                m0 += mandala_spiral(f2, 192.0);
            }
        }
        let c = vec2(0.225 + 0.1 * t0, M_PI / 4.0);
        if f.x > 0.43 {
            for _ in 0..2 {
                f.y += M_PI / 8.0;
                let f2 = glsl_mod(f, c) - c * 0.5;
                m1 += mandala_rose(f2 * (0.75 - 0.5 * t0), 0.4 * t1, 24.0, false);
                m1 += mandala_rose(f2 * (0.5 + 0.5 * t1), 0.2 + 0.2 * t0, 36.0, true);
            }
        }
        let c = vec2(0.6 - 0.2 * t0, M_PI / 4.0);
        if f.x > 0.265 {
            for _ in 0..2 {
                f.y += M_PI / 8.0;
                let f2 = glsl_mod(f, c) - c * 0.5;
                m2 += mandala_spiral(vec2((0.25 + 0.5 * t1) * f2.x, f2.y), 392.0);
                m2 += mandala_rose(f2 * (1.0 + 0.25 * t0), 0.5, 24.0, true);
            }
        }
        let c = vec2(0.4 + 0.23 * t0, M_PI / 4.0);
        if f.x < 0.265 {
            for _ in 0..2 {
                f.y += M_PI / 8.0;
                let f2 = glsl_mod(f, c) - c * 0.5;
                m3 += mandala_spiral(f2, 256.0);
                m3 += mandala_rose(f2, 1.5 * t1, 16.0, false);
            }
        }
        m4 += mandala_circle(f, 0.040, 192.0);
        m4 += mandala_circle(f, 0.265, 192.0);
        m4 += mandala_circle(f, 0.430, 192.0);
    }
    m4 += mandala_circle(f, 0.7325, 192.0);
    let z = m0 + m1 + m2 + m3 + m4;
    clamp(z * z, 0.0, 1.0)
}

/// Shared prepare of `[seed, mode, time, zoom, invert]` (cut_mandala, cut_pattern, cut_sincos,
/// cut_sqsplits). `seed` only re-seeds an unused `Random` (and resets `time` from the wall
/// clock when set interactively), so it has no effect here.
fn prepare_seed_mode_time_zoom_invert(p: &[f64], _: &Affine) -> Vec<f64> {
    vec![to_i(p[0]), lim_i(p[1], 0.0, 1.0), p[2], p[3], lim_i(p[4], 0.0, 1.0)]
}

// ---------------------------------------------------------------------------------------------
// cut_metaballs
// ---------------------------------------------------------------------------------------------

fn metaballs_color(x: f64, y: f64, zoom: f64, time: f64) -> f64 {
    let st = vec2(x * zoom, y * zoom);
    let i_st = floor(st);
    let f_st = fract(st);
    let mut m_dist = 1.0;
    for j in -1..=1 {
        for i in -1..=1 {
            let neighbor = vec2(i as f64, j as f64);
            let offset = glsl_random2(i_st + neighbor);
            let offset = sin(offset * 6.2831 + time) * 0.5 + 0.5;
            let pos = neighbor + offset - f_st;
            m_dist = min(m_dist, m_dist * length(pos));
        }
    }
    step(0.060, m_dist)
}

// ---------------------------------------------------------------------------------------------
// cut_pattern
// ---------------------------------------------------------------------------------------------

fn pattern_color(x: f64, y: f64, time: f64, zoom: f64) -> f64 {
    // rotateTilePattern(tile(st, zoom))
    let mut st = glsl_tile(vec2(x, y), zoom) * 2.0;
    let mut index = 0.0;
    index += step(1.0, glsl_mod(st.x, 2.0));
    index += step(1.0, glsl_mod(st.y, 2.0)) * 2.0;
    st = fract(st);
    let uv = if index == 0.0 || index == 2.0 {
        glsl_rotate2d(st, M_PI * 2.0 * 1.0 / 4.0)
    } else if index == 1.0 || index == 3.0 {
        glsl_rotate2d(st, M_PI * 2.0 * 3.0 / 4.0)
    } else {
        st
    };
    let pos = vec2(0.0, 5.0) - uv;
    let radius = length(pos);
    let arg = uv.y * M_PI * 5.0 + time + ((uv.x * M_PI * 3.0).sin() * M_PI * 2.0 + (uv.y * M_PI * 15.0).sin()).cos();
    let r = (radius * arg.sin() * 1.0 * (uv.y * M_PI + (uv.x * M_PI * 5.0).sin()).sin()).sin();
    (r * M_PI * 2.0 + M_PI * 2.0).cos() * 0.9 + (r * M_PI * 2.0 + M_PI * 2.0).sin() * 0.7
}

// ---------------------------------------------------------------------------------------------
// cut_randomtile
// ---------------------------------------------------------------------------------------------

fn randomtile_hash21(p: Vec2) -> f64 {
    fract((dot(p, vec2(127.183, 157.927))).sin() * 43758.5453)
}

/// `TilePattern`: `p` is the cell-local position (`p - (ip + 0.5)`), `ip` the cell id.
fn randomtile_pattern(p: Vec2, ip: Vec2) -> f64 {
    let rnd = randomtile_hash21(ip);
    let rnd2 = randomtile_hash21(ip + 27.93);
    let i_rnd = (rnd * 4.0).floor();
    let (s, c) = (i_rnd * 3.14159 / 2.0).sin_cos();
    let p = p * mat2(c, -s, s, c);
    let l = length(p - vec2(-0.5, 0.5));
    let d1 = (l - 0.25).abs();
    let d2 = (l - 0.5).abs();
    let mut d3 = (l - 0.75).abs();
    if rnd2 > 0.33 {
        d3 = (length(p - vec2(0.125, 0.5)) - 0.125).abs();
    }
    let mut d = java_min(java_min(d1, d2), d3);
    let d1 = 1e5;
    let d2 = (length(p - vec2(0.5, 0.125)) - 0.125).abs();
    let d3 = (length(p - vec2(0.5, -0.5)) - 0.25).abs();
    d = java_min(d, java_min(d1, java_min(d2, d3)));
    let l = length(p + 0.5);
    d = java_max(d, -(l - 0.75));
    let d1 = (l - 0.5).abs();
    let d1 = java_min(d1, (d1 - 0.25).abs());
    d = java_min(d, d1);
    d - 0.0625
}

fn randomtile_color(x: f64, y: f64, zoom: f64, x0: f64, y0: f64) -> f64 {
    let p = vec2(x * zoom, y * zoom) + vec2(x0, y0);
    let ip = floor(p);
    let d = randomtile_pattern(p - (ip + 0.5), ip);
    // The grunge texture, highlights and shadows of the original are computed but unused
    // (background 0, pattern color 1), so only the distance matters.
    let sf = 5.0 / 1000.0;
    let mut col = mix(0.0, 0.0, 1.0 - smoothstep(0.0, sf, d));
    col = mix(col, 1.0, 1.0 - smoothstep(0.0, sf, d + 0.025));
    sqrt(max(col, 0.0))
}

fn prepare_randomtile(p: &[f64], _: &Affine) -> Vec<f64> {
    // d = [seed, mode, zoom, invert, x0, y0, x0_int, x0_frac, y0_int, y0_frac]
    let seed = p[0] as i32;
    let mut r = JavaRandom::new(seed as i64);
    let x0 = seed as f64 * r.next_double();
    let y0 = seed as f64 * r.next_double();
    let [xi, xf] = split(x0);
    let [yi, yf] = split(y0);
    vec![seed as f64, lim_i(p[1], 0.0, 1.0), p[2], lim_i(p[3], 0.0, 1.0), x0, y0, xi, xf, yi, yf]
}

// ---------------------------------------------------------------------------------------------
// cut_rgrid
// ---------------------------------------------------------------------------------------------

fn rgrid_color(x: f64, y: f64, zoom: f64, angle: f64) -> f64 {
    let pi = (-1.0f64).acos();
    let mut p = vec2(x * zoom, y * zoom);
    let th = glsl_mod(angle * pi / 180.0, pi * 2.0);
    let gridsize = (0.5 + (th * 2.0).sin().abs() * (2f64.sqrt() / 2.0 - 0.5)) * 2.0;
    let mut flip = 0;
    if fract(th / pi + 0.25) > 0.5 {
        p = p - 0.5;
        flip = 1;
    }
    p = p * gridsize;
    let cp = floor(p / gridsize);
    p = glsl_mod(p, gridsize) - gridsize / 2.0;
    p = p * (glsl_mod(cp, 2.0) * 2.0 - 1.0);
    let (s, c) = th.sin_cos();
    p = p * mat2(c, s, -s, c);
    let w = zoom / 2000.0 * 1.5;
    let mut color = smoothstep(-w, w, max(p.x.abs(), p.y.abs()) - 0.5);
    if flip == 1 {
        color = 1.0 - color;
    }
    if flip == 1 && color < 0.5 && (p.x.abs() - p.y.abs()) * sign(fract(th / pi) - 0.5) > 0.0 {
        color = 0.4;
    }
    if flip == 0 && color < 0.5 && (glsl_mod(cp.x + cp.y, 2.0) - 0.5) > 0.0 {
        color = 0.4;
    }
    color
}

// ---------------------------------------------------------------------------------------------
// cut_shapes (signed distance functions by iq)
// ---------------------------------------------------------------------------------------------

fn shapes_hexagram(p: Vec2, r: f64) -> f64 {
    let k = vec4(-0.5, 0.86602540378, 0.57735026919, 1.73205080757);
    let mut p = abs(p);
    let kxy = vec2(k.x, k.y);
    let t1 = kxy * Vec2::splat(2.0) * min(dot(kxy, p), 0.0);
    p = p - t1;
    let kyx = vec2(k.y, k.x);
    let t1 = kyx * 2.0 * min(dot(kyx, p), 0.0);
    p = p - t1;
    p = p - vec2(clamp(p.x, r * k.z, r * k.w), r);
    length(p) * sign(p.y)
}

fn shapes_star(p: Vec2, r: f64, n: i32, m: f64) -> f64 {
    let an = 3.141593 / n as f64;
    let en = 6.283185 / m;
    let acs = vec2(an.cos(), an.sin());
    let ecs = vec2(en.cos(), en.sin());
    let bn = glsl_mod(glsl_atan2(p.x, p.y), 2.0 * an) - an;
    let mut p = vec2(bn.cos(), bn.sin().abs()) * length(p);
    p = p - acs * r;
    p = p + ecs * clamp(-dot(p, ecs), 0.0, r * acs.y / ecs.y);
    length(p) * sign(p.x)
}

fn shapes_cross(p: Vec2, b: Vec2, r: f64) -> f64 {
    let p = abs(p);
    let p = if p.y > p.x { p.yx() } else { p };
    let q = p - b;
    let k = max(q.y, q.x);
    let w = if k > 0.0 { q } else { vec2(b.y - p.x, -k) };
    sign(k) * length(max(w, 0.0)) + r
}

fn shapes_equilateral_triangle(mut p: Vec2) -> f64 {
    let k = 3f64.sqrt();
    p.x = p.x.abs() - 1.0;
    p.y += 1.0 / k;
    if p.x + k * p.y > 0.0 {
        p = vec2((p.x - k * p.y) / 2.0, (-k * p.x - p.y) / 2.0);
    }
    p.x -= clamp(p.x, -2.0, 0.0);
    -length(p) * sign(p.y)
}

fn shapes_octogon(p: Vec2, r: f64) -> f64 {
    let k = vec3(-0.9238795325, 0.3826834323, 0.4142135623);
    let mut p = abs(p);
    let a = vec2(k.x, k.y);
    p = p - a * (2.0 * min(dot(a, p), 0.0));
    let b = vec2(-k.x, k.y);
    p = p - b * (2.0 * min(dot(b, p), 0.0));
    p = p - vec2(clamp(p.x, -k.z * r, k.z * r), r);
    length(p) * sign(p.y)
}

fn shapes_hexagon(p: Vec2, r: f64) -> f64 {
    let k = vec3(-0.866025404, 0.5, 0.577350269);
    let mut p = abs(p);
    let a = vec2(k.x, k.y);
    p = p - a * (2.0 * min(dot(a, p), 0.0));
    p = p - vec2(clamp(p.x, -k.z * r, k.z * r), r);
    length(p) * sign(p.y)
}

fn shapes_pentagon(mut p: Vec2, r: f64) -> f64 {
    let k = vec3(0.809016994, 0.587785252, 0.726542528);
    p.y = -p.y;
    p.x = p.x.abs();
    let a = vec2(-k.x, k.y);
    p = p - a * (2.0 * min(dot(a, p), 0.0));
    let b = vec2(k.x, k.y);
    p = p - b * (2.0 * min(dot(b, p), 0.0));
    p = p - vec2(clamp(p.x, -r * k.z, r * k.z), r);
    length(p) * sign(p.y)
}

fn shapes_arc(p: Vec2, sca: Vec2, scb: Vec2, ra: f64, rb: f64) -> f64 {
    let mut p = mat2(sca.x, sca.y, -sca.y, sca.x) * p;
    p.x = p.x.abs();
    let k = if scb.y * p.x > scb.x * p.y { dot(p, scb) } else { length(p) };
    (dot(p, p) + ra * ra - 2.0 * ra * k).sqrt() - rb
}

fn shapes_vesica(p: Vec2, r: f64, d: f64) -> f64 {
    let p = abs(p);
    let b = (r * r - d * d).sqrt();
    if (p.y - b) * d > p.x * b { length(p - vec2(0.0, b)) } else { length(p - vec2(-d, 0.0)) - r }
}

fn shapes_trapezoid(mut p: Vec2, r1: f64, r2: f64, he: f64) -> f64 {
    let k1 = vec2(r2, he);
    let k2 = vec2(r2 - r1, 2.0 * he);
    p.x = p.x.abs();
    let ca = vec2(max(0.0, p.x - if p.y < 0.0 { r1 } else { r2 }), p.y.abs() - he);
    let t0 = dot(k1 - p, k2) / dot(k2, k2);
    let cb = p - k1 + k2 * clamp(t0, 0.0, 1.0);
    let s = if cb.x < 0.0 && ca.y < 0.0 { -1.0 } else { 1.0 };
    s * java_min(dot(ca, ca), dot(cb, cb)).sqrt()
}

fn shapes_trapezoid2(p: Vec2, a: Vec2, b: Vec2, ra: f64, rb: f64) -> f64 {
    let rba = rb - ra;
    let baba = dot(b - a, b - a);
    let papa = dot(p - a, p - a);
    let paba = dot(p - a, b - a) / baba;
    let x = (papa - paba * paba * baba).sqrt();
    let cax = java_max(0.0, x - if paba < 0.5 { ra } else { rb });
    let cay = (paba - 0.5).abs() - 0.5;
    let k = rba * rba + baba;
    let f = clamp((rba * (x - ra) + paba * baba) / k, 0.0, 1.0);
    let cbx = x - ra - f * rba;
    let cby = paba - f;
    let s = if cbx < 0.0 && cay < 0.0 { -1.0 } else { 1.0 };
    s * java_min(cax * cax + cay * cay * baba, cbx * cbx + cby * cby * baba).sqrt()
}

fn shapes_rhombus(p: Vec2, b: Vec2) -> f64 {
    let ndot = |a: Vec2, b: Vec2| a.x * b.x - a.y * b.y;
    let q = abs(p);
    let h = clamp((-2.0 * ndot(q, b) + ndot(b, b)) / dot(b, b), -1.0, 1.0);
    let d = length(q - b * vec2(1.0 - h, 1.0 + h) * 0.5);
    d * sign(q.x * b.y + q.y * b.x - b.x * b.y)
}

fn shapes_box(p: Vec2, b: Vec2) -> f64 {
    let d = abs(p) - b;
    length(max(d, Vec2::splat(0.0))) + min(max(d.x, d.y), 0.0)
}

fn shapes_color(d: &[f64], x: f64, y: f64) -> f64 {
    // d = [mode, type, contour, zoom, invert, n, thick, time]
    let zoom = d[3];
    let contour = d[2] as i32;
    let n = d[5] as i32;
    let thick = d[6];
    let time = d[7];
    let p = vec2(x * zoom, y * zoom);
    let mut color = if contour == 1 { 1.0 } else { 0.0 };
    let dist = match d[1] as i32 {
        0 => shapes_hexagram(p, 0.35),
        1 => shapes_equilateral_triangle(p),
        2 => {
            let a = fract(time / 3.0);
            let m = 4.0 + a * a * (n as f64 * 2.0 - 4.0);
            shapes_star(p, 0.7, n, m)
        }
        3 => {
            let mut si = cos(vec2(0.0, 1.57) + time) * 0.3 + 0.8;
            if si.x < si.y {
                si = si.yx();
            }
            let ra = 0.1 * (time * 1.2).sin();
            shapes_cross(p, si, ra)
        }
        4 => shapes_octogon(p, 0.5),
        5 => shapes_hexagon(p, 0.5),
        6 => shapes_pentagon(p, 0.5),
        7 => shapes_vesica(p, 1.1, 0.8 + 0.2 * time.sin()),
        8 => {
            let ra = 0.2 + 0.15 * (time * 1.3 + 0.0).sin();
            let rb = 0.2 + 0.15 * (time * 1.4 + 0.1).sin();
            let pa = vec2(-0.6, 0.0) + sin(vec2(0.0, 2.0) + time * 1.1) * 0.4;
            let pb = vec2(-0.6, 0.0) + sin(vec2(1.0, 2.5) + time * 1.2) * 0.4;
            let pc = vec2(0.8, 0.0);
            let dd = shapes_trapezoid(p - pc, ra, rb, 0.5 + 0.2 * (1.3 * time).sin());
            java_min(dd, shapes_trapezoid2(p, pa, pb, ra, rb))
        }
        9 => {
            let ta = 3.14 * (0.5 + 0.5 * (time * 0.52 + 2.0).cos());
            let tb = 3.14 * (0.5 + 0.5 * (time * 0.31 + 2.0).cos());
            let rb = 0.15 * (0.5 + 0.5 * (time * 0.41 + 3.0).cos());
            shapes_arc(p, vec2(ta.sin(), ta.cos()), vec2(tb.sin(), tb.cos()), 0.5, rb)
        }
        10 => shapes_rhombus(p, cos(vec2(0.0, 1.57) + time) * 0.3 + 0.4),
        11 => length(p) - 0.5,
        12 => shapes_box(p, cos(vec2(0.0, 1.57) + time) * 0.3 + 0.4),
        _ => 0.0,
    };
    if contour == 1 {
        color *= (120.0 * dist).cos();
    } else {
        color *= 1.0 - (-2.0 * dist.abs()).exp();
        color = mix(color, 1.0, 1.0 - smoothstep(0.0, thick, dist.abs()));
    }
    color
}

fn prepare_shapes(p: &[f64], _: &Affine) -> Vec<f64> {
    vec![
        lim_i(p[0], 0.0, 1.0),
        lim_i(p[1], 0.0, 12.0),
        lim_i(p[2], 0.0, 1.0),
        limit_value(p[3], 0.1, 50.0),
        lim_i(p[4], 0.0, 1.0),
        lim_i(p[5], 3.0, 9.0),
        p[6].abs(),
        p[7],
    ]
}

// ---------------------------------------------------------------------------------------------
// cut_sincos
// ---------------------------------------------------------------------------------------------

fn sincos_color(x: f64, y: f64, time: f64, zoom: f64) -> f64 {
    let mut uv = vec2(x * zoom, y * zoom);
    uv.x += 1.5;
    uv.y += (uv.x * 10.0).cos() * 0.15;
    uv.y *= 1.5;
    let r = 7.3;
    let e = uv.x - time * 0.5;
    let t = e + uv.y.abs();
    let f = (1.0 + (uv.x * 3.0).sin()) * 0.4 + 0.1;
    let d = f - (fract(t * r) - 0.5).abs() * 2.0;
    let s = smoothstep(0.0, 0.05 / max(d + 0.5, 0.0), d + 0.1);
    s.sqrt()
}

// ---------------------------------------------------------------------------------------------
// cut_snowflake
// ---------------------------------------------------------------------------------------------

fn snowflake_color(x: f64, y: f64, time: f64, level: i32, zoom: f64) -> f64 {
    let uv = vec2(x * zoom, y * zoom);
    let s = 0.25;
    let mut p = Vec3::from_vec2(uv, (time * 0.4).sin() * 0.5 - 0.5) * s;
    for _ in 0..level {
        p = abs(p) / dot(p, p) - 1.65;
        let k = length(p);
        p = p * k + k;
    }
    dot(p, p) * s
}

// ---------------------------------------------------------------------------------------------
// cut_spiralcb
// ---------------------------------------------------------------------------------------------

fn spiralcb_rotate(spin: f64) -> Mat2 {
    mat2(-spin.sin(), spin.cos(), spin.cos(), spin.sin())
}

fn spiralcb_hill(t: f64, w: f64, p: f64) -> f64 {
    java_min(step(t - w / 2.0, p), 1.0 - step(t + w / 2.0, p))
}

fn spiralcb_color(x: f64, y: f64, time: f64, zoom: f64) -> f64 {
    let mut uv = vec2(x * zoom, y * zoom);
    let fi = length(uv) * 50.0;
    let g = glsl_atan2(uv.y, uv.x);
    uv = spiralcb_rotate(time * 7.0) * uv;
    uv = uv * (g * 15.0).sin();
    uv = spiralcb_rotate(fi) * uv;
    mix(1.0, 0.0, java_min(step(0.0, uv.x), spiralcb_hill(0.0, fi / 25.0, uv.y)))
}

// ---------------------------------------------------------------------------------------------
// cut_spiral
// ---------------------------------------------------------------------------------------------

fn spiral_color(x: f64, y: f64, time: f64, zoom: f64) -> f64 {
    let mut uv = vec2(x * zoom, y * zoom);
    let mut l = 0;
    let (s, c) = time.sin_cos();
    let rot = mat2(1.1 * s, 1.1 * c, -1.1 * c, 1.1 * s);
    for _ in 0..64 {
        uv = rot * uv;
        if uv.y > 1.0 {
            break;
        }
        l ^= 1;
    }
    l as f64
}

/// Shared prepare of `[time, mode, zoom, invert]` (cut_spiralcb, cut_spiral).
fn prepare_time_mode_zoom_invert(p: &[f64], _: &Affine) -> Vec<f64> {
    vec![p[0], lim_i(p[1], 0.0, 1.0), p[2], lim_i(p[3], 0.0, 1.0)]
}

// ---------------------------------------------------------------------------------------------
// cut_spots
// ---------------------------------------------------------------------------------------------

fn spots_noise(st: Vec2) -> f64 {
    let i = floor(st);
    let f = fract(st);
    let a = glsl_random(i);
    let b = glsl_random(i + vec2(1.0, 0.0));
    let c = glsl_random(i + vec2(0.0, 1.0));
    let d = glsl_random(i + vec2(1.0, 1.0));
    let u = f * f * (Vec2::splat(3.0) - f * 2.0);
    mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y
}

fn spots_fbm(mut st: Vec2) -> f64 {
    let mut value = 0.0;
    let mut amplitude = 0.5;
    for _ in 0..6 {
        value += amplitude * spots_noise(st);
        st = st * 2.0;
        amplitude *= 0.5;
    }
    value
}

fn spots_color(x: f64, y: f64, zoom: f64, x0: f64, y0: f64) -> f64 {
    let uv = vec2(x * zoom * 43.0, y * zoom * 43.0) + vec2(x0, y0);
    let f = spots_fbm(uv);
    if f < 0.4 {
        // mix(white, white, remap(...)) is white; the second fbm can only set it to white
        // again, so it is skipped.
        let remap = 0.0 + ((f - 0.06) - 0.25) * (1.0 - 0.0) / (0.38 - 0.25);
        mix(1.0, 1.0, remap)
    } else {
        0.0
    }
}

fn prepare_spots(p: &[f64], _: &Affine) -> Vec<f64> {
    // d = [mode, seed, zoom, invert, x0, y0]
    let seed = p[1] as i32;
    let mut r = JavaRandom::new(seed as i64);
    let x0 = seed as f64 * r.next_double();
    let y0 = seed as f64 * r.next_double();
    vec![lim_i(p[0], 0.0, 1.0), seed as f64, p[2], lim_i(p[3], 0.0, 1.0), x0, y0]
}

// ---------------------------------------------------------------------------------------------
// cut_sqcir
// ---------------------------------------------------------------------------------------------

fn sqcir_color(x: f64, y: f64, zoom: f64, power: f64) -> f64 {
    let uv = vec2(x * zoom, y * zoom);
    let lhs = (uv.x - uv.y).abs().powf(power) + (uv.y + uv.x).abs().powf(power);
    if lhs <= 1.0 { 1.0 } else { 0.0 }
}

// ---------------------------------------------------------------------------------------------
// cut_sqsplits
// ---------------------------------------------------------------------------------------------

fn sqsplits_color(x: f64, y: f64, time: f64, zoom: f64) -> f64 {
    let u = vec2(x * zoom, y * zoom);
    let uf = fract(u) - 0.5;
    let ui = floor(glsl_mod(u, 2.0));
    let t = time * 0.35;
    let a = glsl_atan2(uf.x, uf.y) / 6.3;
    let r = fract(t + t + 2.0 * if ui.x != ui.y { a } else { 1.0 - a });
    let sharp = 180.0 * length(uf);
    let k = clamp((r - 0.5) * sharp, 0.0, 1.0) + clamp(1.0 - r * sharp, 0.0, 1.0);
    1.0 - k
}

// ---------------------------------------------------------------------------------------------
// cut_swarp
// ---------------------------------------------------------------------------------------------

fn swarp_dist(p: Vec2, kind: i32) -> f64 {
    let mut distance = 0.0;
    if kind == 0 {
        distance = length(p);
    }
    let p = abs(p);
    if kind == 1 {
        distance = java_max(p.x, p.y);
    }
    if kind == 2 {
        distance = java_max(p.x * 0.8660254 + p.y * 0.5, p.y);
    }
    if kind == 3 {
        distance = java_max(java_max(p.x, p.y), (p.x + p.y) * 0.7071);
    }
    distance
}

fn swarp_color(x: f64, y: f64, time: f64, kind: i32, zoom: f64) -> f64 {
    let mut uv = vec2(x * zoom, y * zoom);
    let v0 = uv * 5.0 + cos(uv * 11.0 + time) + time;
    let t0 = dot(sin(v0), Vec2::splat(0.5));
    uv = uv + t0 * 0.02;
    uv.y += (uv.x * 7.0 + (uv.x * 4.0 + time * 2.0).cos() * 1.0 + time).sin() * 0.05;
    let s = uv.x.ceil() * 2.0 - 1.0;
    let m = 6.0;
    uv.y += s / m;
    let n = 5.0;
    let k = (s * 3.14159 / 2.0 + swarp_dist(uv, kind) * n * m * 3.14159 / 2.0).sin();
    java_max(k, 0.0).sqrt()
}

// ---------------------------------------------------------------------------------------------
// cut_tileillusion
// ---------------------------------------------------------------------------------------------

#[inline]
fn tileillusion_f(x: f64) -> f64 {
    x + 0.1 * (1.6 * x).sin()
}

fn tileillusion_solve(mut x0: f64, mut x1: f64, y: f64) -> f64 {
    let mut y0 = tileillusion_f(x0);
    let mut y1 = tileillusion_f(x1);
    if y1 < y0 {
        std::mem::swap(&mut x0, &mut x1);
        std::mem::swap(&mut y0, &mut y1);
    }
    let mut xn = 0.0;
    for _ in 0..20 {
        xn = x0 + (x1 - x0) / (y1 - y0) * (y - y0);
        let yn = tileillusion_f(xn);
        if yn > y {
            x1 = xn;
            y1 = yn;
        } else {
            x0 = xn;
            y0 = yn;
        }
    }
    xn
}

fn tileillusion_color(x: f64, y: f64, time: f64, zoom: f64) -> f64 {
    let mut uv = vec2(x * zoom, y * zoom);
    let fy = tileillusion_f(uv.y).floor();
    let y0 = tileillusion_solve(uv.y - 3.0, uv.y, fy);
    let y1 = tileillusion_solve(uv.y, uv.y + 3.0, fy + 1.0);
    uv.y = tileillusion_f(uv.y);
    uv.x /= y1 - y0;
    let s = glsl_mod(uv.y.floor(), 2.0);
    let mut color = 0.0;
    if fract(uv.y) > 0.05 {
        uv.x += time * sign(s - 0.5);
        color = glsl_mod(uv.x.floor() + uv.y.floor(), 2.0);
    }
    color
}

// ---------------------------------------------------------------------------------------------
// cut_triantess
// ---------------------------------------------------------------------------------------------

/// Precalculated triangle group (`init()`): space type, mirror normals `nb`, `nc` and vertex `p`.
struct TriantessGeo {
    st: f64,
    nb: Vec3,
    nc: Vec3,
    p: Vec3,
    iters: i32,
    srad: f64,
}

#[inline]
fn triantess_hdott(a: Vec3, b: Vec3, st: f64) -> f64 {
    (a.x * b.x + a.y * b.y) * st + a.z * b.z
}

#[inline]
fn triantess_hdots(a: Vec3, b: Vec3, st: f64) -> f64 {
    (a.x * b.x + a.y * b.y) + st * a.z * b.z
}

#[inline]
fn triantess_hnormalizet(v: Vec3, st: f64) -> Vec3 {
    v * (1.0 / triantess_hdott(v, v, st).abs().sqrt())
}

fn triantess_dist2segment(g: &TriantessGeo, z: Vec3, n: Vec3, r: f64) -> f64 {
    let st = g.st;
    let p = g.p;
    let smat = Mat2::from_cols(vec2(1.0, -triantess_hdots(p, n, st)), vec2(-triantess_hdott(p, n, st), 1.0));
    let mut v = smat * vec2(triantess_hdott(z, p, st), triantess_hdots(z, n, st));
    v.y = java_min(0.0, v.y);
    let pmin = triantess_hnormalizet(p * v.x + n * v.y, st);
    let dm = pmin - z;
    let dp = pmin + z;
    let tha = triantess_hdots(dm, dm, st).abs().sqrt() / triantess_hdott(dp, dp, st).abs().sqrt();
    let tha = (tha - g.srad) / (1.0 + st * tha * g.srad);
    tha * (1.0 + st * r * r) / (1.0 + st * st * r * tha)
}

fn triantess_color(g: &TriantessGeo, pos: Vec2) -> f64 {
    let st = g.st;
    let r = length(pos);
    let background = 1.0;
    let aa_scale = 0.0005;
    if st == -1.0 && r >= 1.0 {
        return background;
    }
    let mut z = Vec3::from_vec2(pos * 2.0, 1.0 - st * r * r) * (1.0 / (1.0 + st * r * r));
    let sm = vec3(1.0, 1.0, st);
    for _ in 0..g.iters {
        z.x = z.x.abs();
        let t = -2.0 * java_min(0.0, dot(g.nb, z));
        z = z + g.nb * sm * t;
        let t = -2.0 * java_min(0.0, dot(g.nc, z));
        z = z + g.nc * sm * t;
    }
    let da = triantess_dist2segment(g, z, vec3(1.0, 0.0, 0.0), r);
    let db = triantess_dist2segment(g, z, g.nb, r);
    let dc = triantess_dist2segment(g, z, g.nc * sm, r);
    let ds = java_min(java_min(da, db), dc);
    let mut color = mix(0.0, background, smoothstep(-1.0, 1.0, ds * 0.5 / aa_scale));
    if st == -1.0 {
        color = mix(background, color, smoothstep(0.0, 1.0, (1.0 - r) * 0.5 / aa_scale));
    }
    color
}

fn prepare_triantess(p: &[f64], _: &Affine) -> Vec<f64> {
    // d = [mode, Iters, SRadius, pParam, qParam, rParam, uBaryc, vBaryc, wBaryc, zoom, invert,
    //      spaceType(11), nb(12..14), nc(15..17), p(18..20)]
    let mut d = vec![
        lim_i(p[0], 0.0, 1.0),
        lim_i(p[1], 0.0, 20.0),
        limit_value(p[2], 0.001, 0.1),
        lim_i(p[3], 1.0, 12.0),
        lim_i(p[4], 1.0, 12.0),
        lim_i(p[5], 1.0, 12.0),
        limit_value(p[6], 0.0, 1.0),
        limit_value(p[7], 0.0, 1.0),
        limit_value(p[8], 0.0, 1.0),
        p[9],
        lim_i(p[10], 0.0, 1.0),
    ];
    let (pp, qp, rp) = (d[3] as i32, d[4] as i32, d[5] as i32);
    let st = sign((qp * rp + pp * rp + pp * qp - pp * qp * rp) as f64);
    let (sinpip, cospip) = (M_PI / pp as f64).sin_cos();
    let cospiq = (M_PI / qp as f64).cos();
    let (sinpir, cospir) = (M_PI / rp as f64).sin_cos();
    let ncsincos = (cospiq + cospip * cospir) / sinpip;
    let nb = vec3(-cospip, sinpip, 0.0);
    let mut nc = vec3(-cospir, -ncsincos, ((ncsincos + sinpir) * (-ncsincos + sinpir)).abs().sqrt());
    if st == 0.0 {
        nc.z = 0.25;
    }
    let pa = vec3(nb.y * nc.z, -nb.x * nc.z, nb.x * nc.y - nb.y * nc.x);
    let pb = vec3(0.0, nc.z, -nc.y);
    let pc = vec3(0.0, 0.0, nb.y);
    let q = pa * d[6] + pb * d[7] + pc * d[8];
    let pv = triantess_hnormalizet(q, st);
    d.extend_from_slice(&[st, nb.x, nb.y, nb.z, nc.x, nc.y, nc.z, pv.x, pv.y, pv.z]);
    d
}

fn triantess_geo(d: &[f64]) -> TriantessGeo {
    TriantessGeo {
        st: d[11],
        nb: vec3(d[12], d[13], d[14]),
        nc: vec3(d[15], d[16], d[17]),
        p: vec3(d[18], d[19], d[20]),
        iters: d[1] as i32,
        srad: d[2],
    }
}

// ---------------------------------------------------------------------------------------------
// cut_triskel
// ---------------------------------------------------------------------------------------------

fn triskel_color(x: f64, y: f64, zoom: f64) -> f64 {
    let mut u = vec2(x * zoom, y * zoom);
    u.y += 0.1;
    let mut color: f64 = 0.0;
    let m = mat2(-0.5, 0.866, -0.866, -0.5);
    for _ in 0..3 {
        u = m * u;
        let a = u + vec2(0.03, -0.577);
        let l = 3.0 * length(a);
        let b = glsl_atan2(a.y, a.x);
        let c = if l + fract(b / 7.0 + 0.3) < 2.0 { 0.5 + 0.5 * (b + 6.24 * l).sin() } else { 0.0 };
        color = max(color, c);
    }
    smoothstep(0.0, 0.1, (color - 0.5).abs()) - smoothstep(0.8, 0.9, color)
}

// ---------------------------------------------------------------------------------------------
// cut_truchet
// ---------------------------------------------------------------------------------------------

fn truchet_color(x: f64, y: f64, kind: i32, zoom: f64, off: Vec2) -> f64 {
    let st = vec2(x * zoom, y * zoom);
    let ipos = floor(st + off);
    let fpos = fract(st + off);
    let tile = glsl_truchet_pattern(fpos, glsl_random(ipos));
    match kind {
        0 => smoothstep(tile.x - 0.3, tile.x, tile.y) - smoothstep(tile.x, tile.x + 0.3, tile.y),
        1 => {
            let l0 = length(tile);
            let l1 = length(tile - Vec2::splat(1.0));
            (step(l0, 0.6) - step(l0, 0.4)) + (step(l1, 0.6) - step(l1, 0.4))
        }
        2 => step(tile.x, tile.y),
        _ => 0.0,
    }
}

fn prepare_truchet(p: &[f64], _: &Affine) -> Vec<f64> {
    // d = [mode, type, seed, zoom, invert, off_x, off_y, off_x_int, off_x_frac, off_y_int, off_y_frac]
    let seed = to_i(p[2]);
    let h = Vec2::splat((7.0 * seed).floor());
    let off = h * glsl_random2(h);
    let [xi, xf] = split(off.x);
    let [yi, yf] = split(off.y);
    vec![lim_i(p[0], 0.0, 1.0), lim_i(p[1], 0.0, 2.0), seed, p[3], lim_i(p[4], 0.0, 1.0), off.x, off.y, xi, xf, yi, yf]
}

// ---------------------------------------------------------------------------------------------
// cut_truchetweaving
// ---------------------------------------------------------------------------------------------

const TW_I3: f64 = 0.333333333;
const TW_I6: f64 = 0.166666666;
const TW_R3: f64 = 1.732050807;
const TW_IR3: f64 = 0.577350269;
const TW_HIR3: f64 = 0.288675134;
const TW_S60: f64 = 0.866025404;
const TW_C60: f64 = 0.5;

fn truchetweaving_circ(uv: Vec2, radius: f64, thickness: f64) -> Vec4 {
    let st = vec2(glsl_atan2(uv.x, uv.y), length(uv));
    let t = thickness / 2.0;
    let w = 0.01;
    let mask = smoothstep(t + w, t, (radius - st.y).abs());
    let alpha = smoothstep(t + 0.1, t, (radius - st.y).abs());
    let alpha = alpha * alpha * mix(0.5, 1.0, mask);
    vec4(st.x * radius, st.y, mask, alpha)
}

fn truchetweaving_beam(uv: Vec2, thickness: f64) -> Vec4 {
    let t = thickness / 2.0;
    let w = 0.01;
    let mask = smoothstep(t + w, t, uv.y.abs());
    let alpha = smoothstep(t + 0.1, t, uv.y.abs());
    let alpha = alpha * alpha * (0.5 + 0.5 * mask);
    vec4(uv.x, uv.y, mask, alpha)
}

/// `Truchet(uv, n).b` (= the `z` component of the composited tile).
fn truchetweaving_tile(mut uv: Vec2, n: f64, cw: f64) -> f64 {
    uv = uv - 0.5;
    uv.x /= TW_R3;
    let r = (fract(n * 5.0) * 6.0).floor() / 6.0 * 6.28;
    let (s, c) = r.sin_cos();
    uv = uv * mat2(c, -s, s, c);
    let circ = truchetweaving_circ;
    let beam = truchetweaving_beam;
    let (v1, v2, v3) = if n < 0.1 {
        (
            circ(uv - vec2(0.0, TW_I3), TW_I6, cw),
            circ(uv - vec2(TW_HIR3, -TW_I6), TW_I6, cw),
            circ(uv - vec2(-TW_HIR3, -TW_I6), TW_I6, cw),
        )
    } else if n < 0.2 {
        (circ(uv - vec2(0.0, TW_I3), TW_I6, cw), circ(uv - vec2(0.0, -TW_I3), TW_I6, cw), beam(uv, cw))
    } else if n < 0.4 {
        (
            circ(uv - vec2(-TW_HIR3, -TW_I6), TW_I6, cw),
            circ(uv - vec2(TW_IR3, 0.0), 0.5, cw),
            circ(uv - vec2(TW_HIR3, 0.5), 0.5, cw),
        )
    } else if n < 0.8 {
        (circ(uv - vec2(TW_IR3, 0.0), 0.5, cw), circ(uv - vec2(-TW_IR3, 0.0), 0.5, cw), beam(uv, cw))
    } else {
        let rot60 = mat2(TW_C60, -TW_S60, TW_S60, TW_C60);
        let rot60i = mat2(TW_C60, TW_S60, -TW_S60, TW_C60);
        (beam(uv, cw), beam(uv * rot60, cw), beam(uv * rot60i, cw))
    };
    let d1 = fract(n * 10.0);
    let v = if d1 < 0.166 {
        mix(v1, mix(v2, v3, v3.w), max(v2.w, v3.w))
    } else if d1 < 0.333 {
        mix(v1, mix(v3, v2, v2.w), max(v2.w, v3.w))
    } else if d1 < 0.5 {
        mix(v2, mix(v1, v3, v3.w), max(v1.w, v3.w))
    } else if d1 < 0.666 {
        mix(v2, mix(v3, v1, v1.w), max(v1.w, v3.w))
    } else if d1 < 0.833 {
        mix(v3, mix(v1, v2, v2.w), max(v1.w, v2.w))
    } else {
        mix(v3, mix(v2, v1, v1.w), max(v1.w, v2.w))
    };
    v.z
}

fn truchetweaving_color(d: &[f64], x: f64, y: f64) -> f64 {
    // d = [randomize, mode, type, width, zoom, invert, uvx, uvy]
    let zoom = d[4];
    let mut u = vec2(x * zoom - d[6], y * zoom - d[7]);
    // type 1 overwrites the CURVE_WIDTH field on every call; equivalent to a local here.
    let cw = if d[2] as i32 == 1 {
        mix(0.05, 0.25, ((u.x + u.y.sin()).sin() + (u.y * 0.35).sin()) * 0.25 + 0.5)
    } else {
        d[3]
    };
    let s = vec2(1.0, TW_R3);
    let a = glsl_mod(u, s) * 2.0 - s;
    let b = glsl_mod(u + s * 0.5, s) * 2.0 - s;
    u = u / s;
    let id = if dot(a, a) < dot(b, b) {
        floor(u)
    } else {
        u = u + 0.5;
        floor(u) - 0.5
    };
    let n = fract((id.x * 324.23 + id.y * 5604.342).sin() * 87654.53);
    truchetweaving_tile(fract(u), n, cw)
}

fn prepare_truchetweaving(p: &[f64], _: &Affine) -> Vec<f64> {
    let seed = to_i(p[0]);
    let uv = seed * (seed * 180.0 / M_PI).sin();
    vec![
        seed,
        lim_i(p[1], 0.0, 1.0),
        lim_i(p[2], 0.0, 1.0),
        limit_value(p[3], 0.0, 0.25),
        limit_value(p[4], 0.1, 50.0),
        lim_i(p[5], 0.0, 1.0),
        uv,
        uv,
    ]
}

// ---------------------------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------------------------

const MODE: ParamDef = ParamDef::int("mode", 1.0, 0.0, 1.0);
const INVERT0: ParamDef = ParamDef::int("invert", 0.0, 0.0, 1.0);
const SEED1000: ParamDef = ParamDef::int("seed", 1000.0, 0.0, 1_000_000.0);

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "cut_mandala",
        params: &[
            SEED1000,
            MODE,
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::range("zoom", 2.0, 0.1, 2.0),
            ParamDef::int("invert", 1.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, time, zoom, invert]
            let xy = cut_input(ctx, a, d[1], Sample::Centered);
            let c = mandala_color(xy.0, xy.1, d[2], d[3]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[4], c, 0.0));
        },
        prepare: Some(prepare_seed_mode_time_zoom_invert),
        wgsl: cut_wgsl!(
            r#"
fn cut_mandala_ss(width: f32, z: f32) -> f32 { return glsl_smoothstep(1.0, 0.0, width * abs(z)); }
fn cut_mandala_circle(p: vec2<f32>, r: f32, width: f32) -> f32 { return cut_mandala_ss(width, p.x - r); }
fn cut_mandala_rose(p: vec2<f32>, t: f32, width: f32, use_cos: bool) -> f32 {
  let px = p.x * (7.0 + 8.0 * t);
  let s = select(sin(6.0 * p.y), cos(6.0 * p.y), use_cos);
  return cut_mandala_ss(width, px - s) + cut_mandala_ss(width, px - abs(s)) + cut_mandala_ss(width, abs(px) - s) + cut_mandala_ss(width, abs(px) - abs(s));
}
fn cut_mandala_spiral(p: vec2<f32>, width: f32) -> f32 {
  let q = 0.5 * p.y / PI;
  let qa = 0.5 * abs(p.y) / PI;
  return cut_mandala_ss(width, p.x - q) + cut_mandala_ss(width, p.x - qa) + cut_mandala_ss(width, abs(p.x) - q) + cut_mandala_ss(width, abs(p.x) - qa);
}
fn cut_mandala_hide(x: f32, y: f32, time: f32, zoom: f32, inv: i32) -> bool {
  let p = vec2<f32>(x * zoom, y * zoom);
  var f = vec2<f32>(length(p), glsl_atan2(p.y, p.x));
  let t0 = cos(0.3 * time);
  let t1 = 0.5 + 0.5 * cos(0.3 * time);
  var m0 = 0.0;
  var m1 = 0.0;
  var m2 = 0.0;
  var m3 = 0.0;
  var m4 = 0.0;
  if (f.x < 0.7325) {
    f.y += 0.1 * time;
    var c = vec2<f32>(0.225 - 0.1 * t0, PI / 4.0);
    if (f.x < 0.25) {
      for (var i = 0; i < 2; i++) {
        let f2 = glsl_mod2(f, c) - c * 0.5;
        m0 += cut_mandala_spiral(f2, 192.0);
      }
    }
    c = vec2<f32>(0.225 + 0.1 * t0, PI / 4.0);
    if (f.x > 0.43) {
      for (var i = 0; i < 2; i++) {
        f.y += PI / 8.0;
        let f2 = glsl_mod2(f, c) - c * 0.5;
        m1 += cut_mandala_rose(f2 * (0.75 - 0.5 * t0), 0.4 * t1, 24.0, false);
        m1 += cut_mandala_rose(f2 * (0.5 + 0.5 * t1), 0.2 + 0.2 * t0, 36.0, true);
      }
    }
    c = vec2<f32>(0.6 - 0.2 * t0, PI / 4.0);
    if (f.x > 0.265) {
      for (var i = 0; i < 2; i++) {
        f.y += PI / 8.0;
        let f2 = glsl_mod2(f, c) - c * 0.5;
        m2 += cut_mandala_spiral(vec2<f32>((0.25 + 0.5 * t1) * f2.x, f2.y), 392.0);
        m2 += cut_mandala_rose(f2 * (1.0 + 0.25 * t0), 0.5, 24.0, true);
      }
    }
    c = vec2<f32>(0.4 + 0.23 * t0, PI / 4.0);
    if (f.x < 0.265) {
      for (var i = 0; i < 2; i++) {
        f.y += PI / 8.0;
        let f2 = glsl_mod2(f, c) - c * 0.5;
        m3 += cut_mandala_spiral(f2, 256.0);
        m3 += cut_mandala_rose(f2, 1.5 * t1, 16.0, false);
      }
    }
    m4 += cut_mandala_circle(f, 0.040, 192.0);
    m4 += cut_mandala_circle(f, 0.265, 192.0);
    m4 += cut_mandala_circle(f, 0.430, 192.0);
  }
  m4 += cut_mandala_circle(f, 0.7325, 192.0);
  let z = m0 + m1 + m2 + m3 + m4;
  let color = clamp(z * z, 0.0, 1.0);
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(1)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_mandala_hide(x, y, D(2), D(3), i32(D(4)))"
        ),
    },
    VarDef {
        name: "cut_metaballs",
        params: &[MODE, ParamDef::range("zoom", 7.0, 0.1, 50.0), INVERT0, ParamDef::range("time", 0.0, 0.0, 100.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, zoom, invert, time]
            let xy = cut_input(ctx, a, d[0], Sample::Unit);
            let c = metaballs_color(xy.0, xy.1, d[1], d[3]);
            cut_output(ctx, a, v, amount, xy, 0.5, hide_zero(d[2], c));
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![lim_i(p[0], 0.0, 1.0), limit_value(p[1], 0.1, 50.0), lim_i(p[2], 0.0, 1.0), p[3]]),
        wgsl: cut_wgsl!(
            r#"
fn cut_metaballs_hide(x: f32, y: f32, zoom: f32, time: f32, inv: i32) -> bool {
  let st = vec2<f32>(x * zoom, y * zoom);
  let i_st = floor(st);
  let f_st = fract(st);
  var m_dist = 1.0;
  for (var j = -1; j <= 1; j++) {
    for (var i = -1; i <= 1; i++) {
      let neighbor = vec2<f32>(f32(i), f32(j));
      let offset = sin(glsl_random2(i_st + neighbor) * 6.2831 + time) * 0.5 + 0.5;
      let pos = neighbor + offset - f_st;
      m_dist = min(m_dist, m_dist * length(pos));
    }
  }
  let color = step(0.060, m_dist);
  if (inv == 0) { return color == 0.0; }
  return color > 0.0;
}"#,
            mode = "D(0)",
            sample = "rnd()",
            center = "0.5",
            hide = "cut_metaballs_hide(x, y, D(1), D(3), i32(D(2)))"
        ),
    },
    VarDef {
        name: "cut_pattern",
        params: &[
            SEED1000,
            MODE,
            ParamDef::range("time", 0.9, 0.0, 100.0),
            ParamDef::range("zoom", 0.5, 0.05, 1.05),
            INVERT0,
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, time, zoom, invert]
            let xy = cut_input(ctx, a, d[1], Sample::Centered);
            let c = pattern_color(xy.0, xy.1, d[2], d[3]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[4], c, 0.0));
        },
        prepare: Some(prepare_seed_mode_time_zoom_invert),
        wgsl: cut_wgsl!(
            r#"
fn cut_pattern_hide(x: f32, y: f32, time: f32, zoom: f32, inv: i32) -> bool {
  var st = glsl_tile(vec2<f32>(x, y), zoom) * 2.0;
  var idx = 0.0;
  idx += step(1.0, glsl_mod(st.x, 2.0));
  idx += step(1.0, glsl_mod(st.y, 2.0)) * 2.0;
  st = fract(st);
  var uv = st;
  if (idx == 0.0 || idx == 2.0) {
    uv = glsl_rotate2d(st, PI * 2.0 * 1.0 / 4.0);
  } else if (idx == 1.0 || idx == 3.0) {
    uv = glsl_rotate2d(st, PI * 2.0 * 3.0 / 4.0);
  }
  let radius = length(vec2<f32>(0.0, 5.0) - uv);
  let arg = uv.y * PI * 5.0 + time + cos(sin(uv.x * PI * 3.0) * PI * 2.0 + sin(uv.y * PI * 15.0));
  let r = sin(radius * sin(arg) * sin(uv.y * PI + sin(uv.x * PI * 5.0)));
  let color = cos(r * PI * 2.0 + PI * 2.0) * 0.9 + sin(r * PI * 2.0 + PI * 2.0) * 0.7;
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(1)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_pattern_hide(x, y, D(2), D(3), i32(D(4)))"
        ),
    },
    VarDef {
        name: "cut_randomtile",
        params: &[SEED1000, MODE, ParamDef::range("zoom", 5.0, 0.5, 20.5), INVERT0],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, zoom, invert, x0, y0, x0_int, x0_frac, y0_int, y0_frac]
            let xy = cut_input(ctx, a, d[1], Sample::Unit);
            let c = randomtile_color(xy.0, xy.1, d[2], d[4], d[5]);
            cut_output(ctx, a, v, amount, xy, 0.5, hide_gt(d[3], c, 0.0));
        },
        prepare: Some(prepare_randomtile),
        wgsl: cut_wgsl!(
            r#"
fn cut_randomtile_hash21(p: vec2<f32>) -> f32 { return fract(sin(dot(p, vec2<f32>(127.183, 157.927))) * 43758.5453); }
fn cut_randomtile_pattern(p_in: vec2<f32>, ip: vec2<f32>) -> f32 {
  let rn = cut_randomtile_hash21(ip);
  let rn2 = cut_randomtile_hash21(ip + 27.93);
  let an = floor(rn * 4.0) * 3.14159 / 2.0;
  let c = cos(an);
  let s = sin(an);
  let p = p_in * mat2x2<f32>(c, -s, s, c);
  var l = length(p - vec2<f32>(-0.5, 0.5));
  var d1 = abs(l - 0.25);
  var d2 = abs(l - 0.5);
  var d3 = abs(l - 0.75);
  if (rn2 > 0.33) { d3 = abs(length(p - vec2<f32>(0.125, 0.5)) - 0.125); }
  var d = min(min(d1, d2), d3);
  d1 = 1.0e5;
  d2 = abs(length(p - vec2<f32>(0.5, 0.125)) - 0.125);
  d3 = abs(length(p - vec2<f32>(0.5, -0.5)) - 0.25);
  d = min(d, min(d1, min(d2, d3)));
  l = length(p + 0.5);
  d = max(d, -(l - 0.75));
  d1 = abs(l - 0.5);
  d1 = min(d1, abs(d1 - 0.25));
  d = min(d, d1);
  return d - 0.0625;
}
fn cut_randomtile_hide(x: f32, y: f32, zoom: f32, xi: f32, xf: f32, yi: f32, yf: f32, inv: i32) -> bool {
  // offset split into integer and fractional part to keep the cell-local position precise
  let q = vec2<f32>(x * zoom + xf, y * zoom + yf);
  let fq = floor(q);
  let d = cut_randomtile_pattern(q - (fq + 0.5), fq + vec2<f32>(xi, yi));
  let sf = 5.0 / 1000.0;
  let col = 1.0 - glsl_smoothstep(0.0, sf, d + 0.025);
  let color = sqrt(max(col, 0.0));
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(1)",
            sample = "rnd()",
            center = "0.5",
            hide = "cut_randomtile_hide(x, y, D(2), D(6), D(7), D(8), D(9), i32(D(3)))"
        ),
    },
    VarDef {
        name: "cut_rgrid",
        params: &[MODE, ParamDef::range("zoom", 7.0, 0.1, 50.0), INVERT0, ParamDef::int("angle", 0.0, 0.0, 90.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, zoom, invert, angle]
            let xy = cut_input(ctx, a, d[0], Sample::Centered);
            let c = rgrid_color(xy.0, xy.1, d[1], d[3]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_zero(d[2], c));
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![lim_i(p[0], 0.0, 1.0), limit_value(p[1], 0.1, 50.0), lim_i(p[2], 0.0, 1.0), lim_i(p[3], 0.0, 90.0)]),
        wgsl: cut_wgsl!(
            r#"
fn cut_rgrid_hide(x: f32, y: f32, zoom: f32, angle: f32, inv: i32) -> bool {
  var p = vec2<f32>(x * zoom, y * zoom);
  let th = glsl_mod(angle * PI / 180.0, PI * 2.0);
  let gridsize = (0.5 + abs(sin(th * 2.0)) * (sqrt(2.0) / 2.0 - 0.5)) * 2.0;
  var flip = 0;
  if (fract(th / PI + 0.25) > 0.5) {
    p = p - 0.5;
    flip = 1;
  }
  p = p * gridsize;
  let cp = floor(p / gridsize);
  p = glsl_mod2(p, vec2<f32>(gridsize)) - gridsize / 2.0;
  p = p * (glsl_mod2(cp, vec2<f32>(2.0)) * 2.0 - 1.0);
  p = p * mat2x2<f32>(cos(th), sin(th), -sin(th), cos(th));
  let w = zoom / 2000.0 * 1.5;
  var color = glsl_smoothstep(-w, w, max(abs(p.x), abs(p.y)) - 0.5);
  if (flip == 1) { color = 1.0 - color; }
  if (flip == 1 && color < 0.5 && (abs(p.x) - abs(p.y)) * sign(fract(th / PI) - 0.5) > 0.0) { color = 0.4; }
  if (flip == 0 && color < 0.5 && (glsl_mod(cp.x + cp.y, 2.0) - 0.5) > 0.0) { color = 0.4; }
  if (inv == 0) { return color == 0.0; }
  return color > 0.0;
}"#,
            mode = "D(0)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_rgrid_hide(x, y, D(1), D(3), i32(D(2)))"
        ),
    },
    VarDef {
        name: "cut_shapes",
        params: &[
            MODE,
            ParamDef::int("type", 0.0, 0.0, 12.0),
            ParamDef::int("contour", 1.0, 0.0, 1.0),
            ParamDef::range("zoom", 1.0, 0.1, 50.0),
            INVERT0,
            ParamDef::int("n", 3.0, 3.0, 9.0),
            ParamDef::range("thick", 0.01, 0.0, 0.51),
            ParamDef::range("time", 0.5, 0.0, 100.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, type, contour, zoom, invert, n, thick, time]
            let xy = cut_input(ctx, a, d[0], Sample::Centered);
            let c = shapes_color(d, xy.0, xy.1);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[4], c, 0.0));
        },
        prepare: Some(prepare_shapes),
        wgsl: cut_wgsl!(
            r#"
fn cut_shapes_hexagram(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec4<f32>(-0.5, 0.86602540378, 0.57735026919, 1.73205080757);
  var p = abs(p_in);
  p = p - k.xy * 2.0 * min(dot(k.xy, p), 0.0);
  p = p - k.yx * 2.0 * min(dot(k.yx, p), 0.0);
  p = p - vec2<f32>(clamp(p.x, r * k.z, r * k.w), r);
  return length(p) * sign(p.y);
}
fn cut_shapes_star(p_in: vec2<f32>, r: f32, n: i32, m: f32) -> f32 {
  let an = 3.141593 / f32(n);
  let en = 6.283185 / m;
  let acs = vec2<f32>(cos(an), sin(an));
  let ecs = vec2<f32>(cos(en), sin(en));
  let bn = glsl_mod(glsl_atan2(p_in.x, p_in.y), 2.0 * an) - an;
  var p = vec2<f32>(cos(bn), abs(sin(bn))) * length(p_in);
  p = p - acs * r;
  p = p + ecs * clamp(-dot(p, ecs), 0.0, r * acs.y / ecs.y);
  return length(p) * sign(p.x);
}
fn cut_shapes_cross(p_in: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
  var p = abs(p_in);
  if (p.y > p.x) { p = p.yx; }
  let q = p - b;
  let k = max(q.y, q.x);
  var w = q;
  if (!(k > 0.0)) { w = vec2<f32>(b.y - p.x, -k); }
  return sign(k) * length(max(w, vec2<f32>(0.0))) + r;
}
fn cut_shapes_triangle(p_in: vec2<f32>) -> f32 {
  let k = sqrt(3.0);
  var p = vec2<f32>(abs(p_in.x) - 1.0, p_in.y + 1.0 / k);
  if (p.x + k * p.y > 0.0) { p = vec2<f32>((p.x - k * p.y) / 2.0, (-k * p.x - p.y) / 2.0); }
  p.x -= clamp(p.x, -2.0, 0.0);
  return -length(p) * sign(p.y);
}
fn cut_shapes_octogon(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec3<f32>(-0.9238795325, 0.3826834323, 0.4142135623);
  var p = abs(p_in);
  p = p - k.xy * (2.0 * min(dot(k.xy, p), 0.0));
  let b = vec2<f32>(-k.x, k.y);
  p = p - b * (2.0 * min(dot(b, p), 0.0));
  p = p - vec2<f32>(clamp(p.x, -k.z * r, k.z * r), r);
  return length(p) * sign(p.y);
}
fn cut_shapes_hexagon(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec3<f32>(-0.866025404, 0.5, 0.577350269);
  var p = abs(p_in);
  p = p - k.xy * (2.0 * min(dot(k.xy, p), 0.0));
  p = p - vec2<f32>(clamp(p.x, -k.z * r, k.z * r), r);
  return length(p) * sign(p.y);
}
fn cut_shapes_pentagon(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec3<f32>(0.809016994, 0.587785252, 0.726542528);
  var p = vec2<f32>(abs(p_in.x), -p_in.y);
  let a = vec2<f32>(-k.x, k.y);
  p = p - a * (2.0 * min(dot(a, p), 0.0));
  p = p - k.xy * (2.0 * min(dot(k.xy, p), 0.0));
  p = p - vec2<f32>(clamp(p.x, -r * k.z, r * k.z), r);
  return length(p) * sign(p.y);
}
fn cut_shapes_arc(p_in: vec2<f32>, sca: vec2<f32>, scb: vec2<f32>, ra: f32, rb: f32) -> f32 {
  var p = mat2x2<f32>(sca.x, sca.y, -sca.y, sca.x) * p_in;
  p.x = abs(p.x);
  var k = length(p);
  if (scb.y * p.x > scb.x * p.y) { k = dot(p, scb); }
  return sqrt(dot(p, p) + ra * ra - 2.0 * ra * k) - rb;
}
fn cut_shapes_vesica(p_in: vec2<f32>, r: f32, d: f32) -> f32 {
  let p = abs(p_in);
  let b = sqrt(r * r - d * d);
  if ((p.y - b) * d > p.x * b) { return length(p - vec2<f32>(0.0, b)); }
  return length(p - vec2<f32>(-d, 0.0)) - r;
}
fn cut_shapes_trapezoid(p_in: vec2<f32>, r1: f32, r2: f32, he: f32) -> f32 {
  let k1 = vec2<f32>(r2, he);
  let k2 = vec2<f32>(r2 - r1, 2.0 * he);
  let p = vec2<f32>(abs(p_in.x), p_in.y);
  let ca = vec2<f32>(max(0.0, p.x - select(r2, r1, p.y < 0.0)), abs(p.y) - he);
  let t0 = dot(k1 - p, k2) / dot(k2, k2);
  let cb = p - k1 + k2 * clamp(t0, 0.0, 1.0);
  let s = select(1.0, -1.0, cb.x < 0.0 && ca.y < 0.0);
  return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}
fn cut_shapes_trapezoid2(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, ra: f32, rb: f32) -> f32 {
  let rba = rb - ra;
  let baba = dot(b - a, b - a);
  let papa = dot(p - a, p - a);
  let paba = dot(p - a, b - a) / baba;
  let x = sqrt(papa - paba * paba * baba);
  let cax = max(0.0, x - select(rb, ra, paba < 0.5));
  let cay = abs(paba - 0.5) - 0.5;
  let k = rba * rba + baba;
  let f = clamp((rba * (x - ra) + paba * baba) / k, 0.0, 1.0);
  let cbx = x - ra - f * rba;
  let cby = paba - f;
  let s = select(1.0, -1.0, cbx < 0.0 && cay < 0.0);
  return s * sqrt(min(cax * cax + cay * cay * baba, cbx * cbx + cby * cby * baba));
}
fn cut_shapes_rhombus(p: vec2<f32>, b: vec2<f32>) -> f32 {
  let q = abs(p);
  let h = clamp((-2.0 * (q.x * b.x - q.y * b.y) + (b.x * b.x - b.y * b.y)) / dot(b, b), -1.0, 1.0);
  let d = length(q - b * vec2<f32>(1.0 - h, 1.0 + h) * 0.5);
  return d * sign(q.x * b.y + q.y * b.x - b.x * b.y);
}
fn cut_shapes_box(p: vec2<f32>, b: vec2<f32>) -> f32 {
  let d = abs(p) - b;
  return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}
fn cut_shapes_hide(x: f32, y: f32, kind: i32, contour: i32, zoom: f32, n: i32, thick: f32, time: f32, inv: i32) -> bool {
  let p = vec2<f32>(x * zoom, y * zoom);
  var color = select(0.0, 1.0, contour == 1);
  var d = 0.0;
  let ra = cos(vec2<f32>(0.0, 1.57) + time) * 0.3 + 0.4;
  switch (kind) {
    case 0: { d = cut_shapes_hexagram(p, 0.35); }
    case 1: { d = cut_shapes_triangle(p); }
    case 2: {
      let a = fract(time / 3.0);
      let m = 4.0 + a * a * (f32(n) * 2.0 - 4.0);
      d = cut_shapes_star(p, 0.7, n, m);
    }
    case 3: {
      var si = cos(vec2<f32>(0.0, 1.57) + time) * 0.3 + 0.8;
      if (si.x < si.y) { si = si.yx; }
      d = cut_shapes_cross(p, si, 0.1 * sin(time * 1.2));
    }
    case 4: { d = cut_shapes_octogon(p, 0.5); }
    case 5: { d = cut_shapes_hexagon(p, 0.5); }
    case 6: { d = cut_shapes_pentagon(p, 0.5); }
    case 7: { d = cut_shapes_vesica(p, 1.1, 0.8 + 0.2 * sin(time)); }
    case 8: {
      let r1 = 0.2 + 0.15 * sin(time * 1.3);
      let r2 = 0.2 + 0.15 * sin(time * 1.4 + 0.1);
      let pa = vec2<f32>(-0.6, 0.0) + sin(vec2<f32>(0.0, 2.0) + time * 1.1) * 0.4;
      let pb = vec2<f32>(-0.6, 0.0) + sin(vec2<f32>(1.0, 2.5) + time * 1.2) * 0.4;
      d = cut_shapes_trapezoid(p - vec2<f32>(0.8, 0.0), r1, r2, 0.5 + 0.2 * sin(1.3 * time));
      d = min(d, cut_shapes_trapezoid2(p, pa, pb, r1, r2));
    }
    case 9: {
      let ta = 3.14 * (0.5 + 0.5 * cos(time * 0.52 + 2.0));
      let tb = 3.14 * (0.5 + 0.5 * cos(time * 0.31 + 2.0));
      let rb = 0.15 * (0.5 + 0.5 * cos(time * 0.41 + 3.0));
      d = cut_shapes_arc(p, vec2<f32>(sin(ta), cos(ta)), vec2<f32>(sin(tb), cos(tb)), 0.5, rb);
    }
    case 10: { d = cut_shapes_rhombus(p, ra); }
    case 11: { d = length(p) - 0.5; }
    case 12: { d = cut_shapes_box(p, ra); }
    default: {}
  }
  if (contour == 1) {
    color *= cos(120.0 * d);
  } else {
    color *= 1.0 - exp(-2.0 * abs(d));
    color = mix(color, 1.0, 1.0 - glsl_smoothstep(0.0, thick, abs(d)));
  }
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(0)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_shapes_hide(x, y, i32(D(1)), i32(D(2)), D(3), i32(D(5)), D(6), D(7), i32(D(4)))"
        ),
    },
    VarDef {
        name: "cut_sincos",
        params: &[
            SEED1000,
            MODE,
            ParamDef::range("time", 0.9, 0.0, 100.0),
            ParamDef::range("zoom", 0.5, 0.1, 5.1),
            INVERT0,
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, time, zoom, invert]
            let xy = cut_input(ctx, a, d[1], Sample::Centered);
            let c = sincos_color(xy.0, xy.1, d[2], d[3]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[4], c, 0.0));
        },
        prepare: Some(prepare_seed_mode_time_zoom_invert),
        wgsl: cut_wgsl!(
            r#"
fn cut_sincos_hide(x: f32, y: f32, time: f32, zoom: f32, inv: i32) -> bool {
  var uv = vec2<f32>(x * zoom, y * zoom);
  uv.x += 1.5;
  uv.y += cos(uv.x * 10.0) * 0.15;
  uv.y *= 1.5;
  let t = uv.x - time * 0.5 + abs(uv.y);
  let f = (1.0 + sin(uv.x * 3.0)) * 0.4 + 0.1;
  let d = f - abs(fract(t * 7.3) - 0.5) * 2.0;
  // smoothstep(0, 0.05 / max(d + 0.5, 0), d + 0.1): an infinite edge gives 0
  let den = max(d + 0.5, 0.0);
  var s = 0.0;
  if (den > 0.0) { s = glsl_smoothstep(0.0, 0.05 / den, d + 0.1); }
  let color = sqrt(s);
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(1)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_sincos_hide(x, y, D(2), D(3), i32(D(4)))"
        ),
    },
    VarDef {
        name: "cut_snowflake",
        params: &[
            SEED1000,
            MODE,
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("level", 10.0, 1.0, 15.0),
            ParamDef::range("zoom", 10.0, 1.0, 51.0),
            INVERT0,
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, time, level, zoom, invert]
            let xy = cut_input(ctx, a, d[1], Sample::Centered);
            let c = snowflake_color(xy.0, xy.1, d[2], d[3] as i32, d[4]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[5], c, 0.5));
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![to_i(p[0]), lim_i(p[1], 0.0, 1.0), p[2], lim_i(p[3], 1.0, 15.0), p[4], lim_i(p[5], 0.0, 1.0)]),
        wgsl: cut_wgsl!(
            r#"
fn cut_snowflake_hide(x: f32, y: f32, time: f32, level: i32, zoom: f32, inv: i32) -> bool {
  let s = 0.25;
  var p = vec3<f32>(x * zoom, y * zoom, sin(time * 0.4) * 0.5 - 0.5) * s;
  for (var i = 0; i < level; i++) {
    p = abs(p) / dot(p, p) - 1.65;
    let k = length(p);
    p = p * k + k;
  }
  let color = dot(p, p) * s;
  if (inv == 0) { return color > 0.5; }
  return color <= 0.5;
}"#,
            mode = "D(1)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_snowflake_hide(x, y, D(2), i32(D(3)), D(4), i32(D(5)))"
        ),
    },
    VarDef {
        name: "cut_spiralcb",
        params: &[ParamDef::range("time", 0.0, 0.0, 9.0), MODE, ParamDef::range("zoom", 1.0, 0.01, 5.01), INVERT0],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [time, mode, zoom, invert]
            let xy = cut_input(ctx, a, d[1], Sample::Wide);
            let c = spiralcb_color(xy.0, xy.1, d[0], d[2]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[3], c, 0.0));
        },
        prepare: Some(prepare_time_mode_zoom_invert),
        wgsl: cut_wgsl!(
            r#"
fn cut_spiralcb_rotate(spin: f32) -> mat2x2<f32> { return mat2x2<f32>(-sin(spin), cos(spin), cos(spin), sin(spin)); }
fn cut_spiralcb_hide(x: f32, y: f32, time: f32, zoom: f32, inv: i32) -> bool {
  var uv = vec2<f32>(x * zoom, y * zoom);
  let fi = length(uv) * 50.0;
  let g = glsl_atan2(uv.y, uv.x);
  uv = cut_spiralcb_rotate(time * 7.0) * uv;
  uv = uv * sin(g * 15.0);
  uv = cut_spiralcb_rotate(fi) * uv;
  let w = fi / 25.0;
  let hill = min(step(0.0 - w / 2.0, uv.y), 1.0 - step(0.0 + w / 2.0, uv.y));
  let color = mix(1.0, 0.0, min(step(0.0, uv.x), hill));
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(1)",
            sample = "2.0 * rnd() - 1.0",
            center = "0.0",
            hide = "cut_spiralcb_hide(x, y, D(0), D(2), i32(D(3)))"
        ),
    },
    VarDef {
        name: "cut_spiral",
        params: &[ParamDef::range("time", 0.0, 0.0, M_2PI), MODE, ParamDef::range("zoom", 2.5, 0.01, 5.01), INVERT0],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [time, mode, zoom, invert]
            let xy = cut_input(ctx, a, d[1], Sample::Wide);
            let c = spiral_color(xy.0, xy.1, d[0], d[2]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[3], c, 0.0));
        },
        prepare: Some(prepare_time_mode_zoom_invert),
        wgsl: cut_wgsl!(
            r#"
fn cut_spiral_hide(x: f32, y: f32, time: f32, zoom: f32, inv: i32) -> bool {
  var uv = vec2<f32>(x * zoom, y * zoom);
  var l = 0;
  let s = sin(time);
  let c = cos(time);
  let rot = mat2x2<f32>(1.1 * s, 1.1 * c, -1.1 * c, 1.1 * s);
  for (var i = 0; i < 64; i++) {
    uv = rot * uv;
    if (uv.y > 1.0) { break; }
    l = l ^ 1;
  }
  let color = f32(l);
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(1)",
            sample = "2.0 * rnd() - 1.0",
            center = "0.0",
            hide = "cut_spiral_hide(x, y, D(0), D(2), i32(D(3)))"
        ),
    },
    VarDef {
        name: "cut_spots",
        params: &[MODE, ParamDef::int("seed", 0.0, 0.0, 1_000_000.0), ParamDef::range("zoom", 0.3, 0.1, 5.1), INVERT0],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, seed, zoom, invert, x0, y0]
            let xy = cut_input(ctx, a, d[0], Sample::Wide);
            let c = spots_color(xy.0, xy.1, d[2], d[4], d[5]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_zero(d[3], c));
        },
        prepare: Some(prepare_spots),
        wgsl: cut_wgsl!(
            r#"
fn cut_spots_noise(st: vec2<f32>) -> f32 {
  let i = floor(st);
  let f = fract(st);
  let a = glsl_random(i);
  let b = glsl_random(i + vec2<f32>(1.0, 0.0));
  let c = glsl_random(i + vec2<f32>(0.0, 1.0));
  let d = glsl_random(i + vec2<f32>(1.0, 1.0));
  let u = f * f * (vec2<f32>(3.0) - f * 2.0);
  return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;
}
fn cut_spots_hide(x: f32, y: f32, zoom: f32, x0: f32, y0: f32, inv: i32) -> bool {
  var st = vec2<f32>(x * zoom * 43.0, y * zoom * 43.0) + vec2<f32>(x0, y0);
  var f = 0.0;
  var amplitude = 0.5;
  for (var i = 0; i < 6; i++) {
    f += amplitude * cut_spots_noise(st);
    st = st * 2.0;
    amplitude *= 0.5;
  }
  let color = select(0.0, 1.0, f < 0.4);
  if (inv == 0) { return color == 0.0; }
  return color > 0.0;
}"#,
            mode = "D(0)",
            sample = "2.0 * rnd() - 1.0",
            center = "0.0",
            hide = "cut_spots_hide(x, y, D(2), D(4), D(5), i32(D(3)))"
        ),
    },
    VarDef {
        name: "cut_sqcir",
        params: &[MODE, ParamDef::range("zoom", 2.0, 0.1, 50.0), INVERT0, ParamDef::range("power", 2.0, 0.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, zoom, invert, power]
            let xy = cut_input(ctx, a, d[0], Sample::Centered);
            let c = sqcir_color(xy.0, xy.1, d[1], d[3]);
            if d[2] as i32 != 0 && c == 1.0 {
                // Java returns early here: the output is left untouched (but hidden).
                v.do_hide = true;
                return;
            }
            cut_output(ctx, a, v, amount, xy, 0.0, hide_zero(d[2], c));
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![lim_i(p[0], 0.0, 1.0), limit_value(p[1], 0.1, 50.0), lim_i(p[2], 0.0, 1.0), limit_value(p[3], 0.0, 2.0)]),
        wgsl: r#"
fn cut_sqcir_color(x: f32, y: f32, zoom: f32, power: f32) -> f32 {
  let uv = vec2<f32>(x * zoom, y * zoom);
  let lhs = java_pow(abs(uv.x - uv.y), power) + java_pow(abs(uv.y + uv.x), power);
  return select(0.0, 1.0, lhs <= 1.0);
}
@body
var x = a.x;
var y = a.y;
if (i32(D(0)) != 0) {
  x = rnd() - 0.5;
  y = rnd() - 0.5;
}
let color = cut_sqcir_color(x, y, D(1), D(3));
v.hide = 0.0;
if (i32(D(2)) == 0) {
  if (color == 0.0) {
    x = 0.0;
    y = 0.0;
    v.hide = 1.0;
  }
  v.x = AMOUNT * x;
  v.y = AMOUNT * y;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
} else if (color == 1.0) {
  // Java returns early: output untouched, but hidden
  v.hide = 1.0;
} else {
  v.x = AMOUNT * x;
  v.y = AMOUNT * y;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
}"#,
    },
    VarDef {
        name: "cut_sqsplits",
        params: &[
            SEED1000,
            MODE,
            ParamDef::range("time", 0.9, 0.0, 100.0),
            ParamDef::range("zoom", 2.0, 1.0, 26.0),
            INVERT0,
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, time, zoom, invert]
            let xy = cut_input(ctx, a, d[1], Sample::Centered);
            let c = sqsplits_color(xy.0, xy.1, d[2], d[3]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[4], c, 0.0));
        },
        prepare: Some(prepare_seed_mode_time_zoom_invert),
        wgsl: cut_wgsl!(
            r#"
fn cut_sqsplits_hide(x: f32, y: f32, time: f32, zoom: f32, inv: i32) -> bool {
  let u = vec2<f32>(x * zoom, y * zoom);
  let uf = fract(u) - 0.5;
  let ui = floor(glsl_mod2(u, vec2<f32>(2.0)));
  let t = time * 0.35;
  let a = glsl_atan2(uf.x, uf.y) / 6.3;
  let r = fract(t + t + 2.0 * select(1.0 - a, a, ui.x != ui.y));
  let sharp = 180.0 * length(uf);
  let k = clamp((r - 0.5) * sharp, 0.0, 1.0) + clamp(1.0 - r * sharp, 0.0, 1.0);
  let color = 1.0 - k;
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(1)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_sqsplits_hide(x, y, D(2), D(3), i32(D(4)))"
        ),
    },
    VarDef {
        name: "cut_swarp",
        params: &[
            SEED1000,
            MODE,
            ParamDef::range("time", 0.0, 0.0, 100.0),
            ParamDef::int("type", 0.0, 0.0, 3.0),
            ParamDef::range("zoom", 1.0, 0.5, 10.0),
            INVERT0,
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [seed, mode, time, type, zoom, invert]
            let xy = cut_input(ctx, a, d[1], Sample::Centered);
            let c = swarp_color(xy.0, xy.1, d[2], d[3] as i32, d[4]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[5], c, 0.5));
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![to_i(p[0]), lim_i(p[1], 0.0, 1.0), p[2], lim_i(p[3], 0.0, 3.0), p[4], lim_i(p[5], 0.0, 1.0)]),
        wgsl: cut_wgsl!(
            r#"
fn cut_swarp_dist(p_in: vec2<f32>, kind: i32) -> f32 {
  var dist = 0.0;
  if (kind == 0) { dist = length(p_in); }
  let p = abs(p_in);
  if (kind == 1) { dist = max(p.x, p.y); }
  if (kind == 2) { dist = max(p.x * 0.8660254 + p.y * 0.5, p.y); }
  if (kind == 3) { dist = max(max(p.x, p.y), (p.x + p.y) * 0.7071); }
  return dist;
}
fn cut_swarp_hide(x: f32, y: f32, time: f32, kind: i32, zoom: f32, inv: i32) -> bool {
  var uv = vec2<f32>(x * zoom, y * zoom);
  let v0 = uv * 5.0 + cos(uv * 11.0 + time) + time;
  let t0 = dot(sin(v0), vec2<f32>(0.5));
  uv = uv + t0 * 0.02;
  uv.y += sin(uv.x * 7.0 + cos(uv.x * 4.0 + time * 2.0) + time) * 0.05;
  let s = ceil(uv.x) * 2.0 - 1.0;
  let m = 6.0;
  uv.y += s / m;
  let n = 5.0;
  let k = sin(s * 3.14159 / 2.0 + cut_swarp_dist(uv, kind) * n * m * 3.14159 / 2.0);
  let color = sqrt(max(k, 0.0));
  if (inv == 0) { return color > 0.5; }
  return color <= 0.5;
}"#,
            mode = "D(1)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_swarp_hide(x, y, D(2), i32(D(3)), D(4), i32(D(5)))"
        ),
    },
    VarDef {
        name: "cut_tileillusion",
        params: &[ParamDef::range("time", 0.0, 0.0, 2.0), MODE, ParamDef::range("zoom", 15.0, 0.1, 50.0), INVERT0],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [time, mode, zoom, invert]
            let xy = cut_input(ctx, a, d[1], Sample::Centered);
            let c = tileillusion_color(xy.0, xy.1, d[0], d[2]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_zero(d[3], c));
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![p[0], lim_i(p[1], 0.0, 1.0), limit_value(p[2], 0.1, 50.0), lim_i(p[3], 0.0, 1.0)]),
        wgsl: cut_wgsl!(
            r#"
fn cut_tileillusion_f(x: f32) -> f32 { return x + 0.1 * sin(1.6 * x); }
fn cut_tileillusion_solve(x0_in: f32, x1_in: f32, y: f32) -> f32 {
  var x0 = x0_in;
  var x1 = x1_in;
  var y0 = cut_tileillusion_f(x0);
  var y1 = cut_tileillusion_f(x1);
  if (y1 < y0) {
    let x2 = x1; x1 = x0; x0 = x2;
    let y2 = y1; y1 = y0; y0 = y2;
  }
  var xn = 0.0;
  for (var i = 0; i < 20; i++) {
    xn = x0 + (x1 - x0) / (y1 - y0) * (y - y0);
    let yn = cut_tileillusion_f(xn);
    if (yn > y) { x1 = xn; y1 = yn; } else { x0 = xn; y0 = yn; }
  }
  return xn;
}
fn cut_tileillusion_hide(x: f32, y: f32, time: f32, zoom: f32, inv: i32) -> bool {
  var uv = vec2<f32>(x * zoom, y * zoom);
  let fy = floor(cut_tileillusion_f(uv.y));
  let y0 = cut_tileillusion_solve(uv.y - 3.0, uv.y, fy);
  let y1 = cut_tileillusion_solve(uv.y, uv.y + 3.0, fy + 1.0);
  uv.y = cut_tileillusion_f(uv.y);
  uv.x /= y1 - y0;
  let s = glsl_mod(floor(uv.y), 2.0);
  var color = 0.0;
  if (fract(uv.y) > 0.05) {
    uv.x += time * sign(s - 0.5);
    color = glsl_mod(floor(uv.x) + floor(uv.y), 2.0);
  }
  if (inv == 0) { return color == 0.0; }
  return color > 0.0;
}"#,
            mode = "D(1)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_tileillusion_hide(x, y, D(0), D(2), i32(D(3)))"
        ),
    },
    VarDef {
        name: "cut_triantess",
        params: &[
            MODE,
            ParamDef::int("Iters", 20.0, 0.0, 20.0),
            ParamDef::range("SRadius", 0.01, 0.001, 0.1),
            ParamDef::int("pParam", 3.0, 1.0, 12.0),
            ParamDef::int("qParam", 3.0, 1.0, 12.0),
            ParamDef::int("rParam", 4.0, 1.0, 12.0),
            ParamDef::range("uBaryc", 1.0, 0.0, 1.0),
            ParamDef::range("vBaryc", 1.0, 0.0, 1.0),
            ParamDef::range("wBaryc", 0.0, 0.0, 1.0),
            ParamDef::range("zoom", 2.0, 1.0, 2.0),
            INVERT0,
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, Iters, SRadius, p, q, r, u, v, w, zoom, invert, spaceType, nb(3), nc(3), p(3)]
            let xy = cut_input(ctx, a, d[0], Sample::Centered);
            let c = triantess_color(&triantess_geo(d), vec2(xy.0 * d[9], xy.1 * d[9]));
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[10], c, 0.0));
        },
        prepare: Some(prepare_triantess),
        wgsl: cut_wgsl!(
            r#"
fn cut_triantess_hdott(a: vec3<f32>, b: vec3<f32>, st: f32) -> f32 { return dot(a.xy, b.xy) * st + a.z * b.z; }
fn cut_triantess_hdots(a: vec3<f32>, b: vec3<f32>, st: f32) -> f32 { return dot(a.xy, b.xy) + st * a.z * b.z; }
fn cut_triantess_hnormt(v: vec3<f32>, st: f32) -> vec3<f32> { return v * (1.0 / sqrt(abs(cut_triantess_hdott(v, v, st)))); }
fn cut_triantess_seg(z: vec3<f32>, n: vec3<f32>, p: vec3<f32>, r: f32, st: f32, srad: f32) -> f32 {
  let smat = mat2x2<f32>(vec2<f32>(1.0, -cut_triantess_hdots(p, n, st)), vec2<f32>(-cut_triantess_hdott(p, n, st), 1.0));
  var v = smat * vec2<f32>(cut_triantess_hdott(z, p, st), cut_triantess_hdots(z, n, st));
  v.y = min(0.0, v.y);
  let pmin = cut_triantess_hnormt(p * v.x + n * v.y, st);
  let dm = pmin - z;
  let dp = pmin + z;
  let tha0 = sqrt(abs(cut_triantess_hdots(dm, dm, st))) / sqrt(abs(cut_triantess_hdott(dp, dp, st)));
  let tha = (tha0 - srad) / (1.0 + st * tha0 * srad);
  return tha * (1.0 + st * r * r) / (1.0 + st * st * r * tha);
}
fn cut_triantess_hide(x: f32, y: f32, zoom: f32, iters: i32, srad: f32, st: f32, nb: vec3<f32>, nc: vec3<f32>, p: vec3<f32>, inv: i32) -> bool {
  let pos = vec2<f32>(x * zoom, y * zoom);
  let r = length(pos);
  var color = 1.0;
  if (!(st == -1.0 && r >= 1.0)) {
    var z = vec3<f32>(pos * 2.0, 1.0 - st * r * r) * (1.0 / (1.0 + st * r * r));
    let sm = vec3<f32>(1.0, 1.0, st);
    for (var i = 0; i < iters; i++) {
      z.x = abs(z.x);
      z = z + nb * sm * (-2.0 * min(0.0, dot(nb, z)));
      z = z + nc * sm * (-2.0 * min(0.0, dot(nc, z)));
    }
    let da = cut_triantess_seg(z, vec3<f32>(1.0, 0.0, 0.0), p, r, st, srad);
    let db = cut_triantess_seg(z, nb, p, r, st, srad);
    let dc = cut_triantess_seg(z, nc * sm, p, r, st, srad);
    let ds = min(min(da, db), dc);
    color = mix(0.0, 1.0, glsl_smoothstep(-1.0, 1.0, ds * 0.5 / 0.0005));
    if (st == -1.0) { color = mix(1.0, color, glsl_smoothstep(0.0, 1.0, (1.0 - r) * 0.5 / 0.0005)); }
  }
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(0)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_triantess_hide(x, y, D(9), i32(D(1)), D(2), D(11), vec3<f32>(D(12), D(13), D(14)), vec3<f32>(D(15), D(16), D(17)), vec3<f32>(D(18), D(19), D(20)), i32(D(10)))"
        ),
    },
    VarDef {
        name: "cut_triskel",
        params: &[MODE, ParamDef::range("zoom", 1.0, 0.25, 1.5), INVERT0],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, zoom, invert]
            let xy = cut_input(ctx, a, d[0], Sample::Wide);
            let c = triskel_color(xy.0, xy.1, d[1]);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[2], c, 0.5));
        },
        prepare: Some(|p: &[f64], _: &Affine| vec![lim_i(p[0], 0.0, 1.0), p[1], lim_i(p[2], 0.0, 1.0)]),
        wgsl: cut_wgsl!(
            r#"
fn cut_triskel_hide(x: f32, y: f32, zoom: f32, inv: i32) -> bool {
  var u = vec2<f32>(x * zoom, y * zoom + 0.1);
  var color = 0.0;
  let m = mat2x2<f32>(-0.5, 0.866, -0.866, -0.5);
  for (var i = 0; i < 3; i++) {
    u = m * u;
    let a = u + vec2<f32>(0.03, -0.577);
    let l = 3.0 * length(a);
    let b = glsl_atan2(a.y, a.x);
    var c = 0.0;
    if (l + fract(b / 7.0 + 0.3) < 2.0) { c = 0.5 + 0.5 * sin(b + 6.24 * l); }
    color = max(color, c);
  }
  color = glsl_smoothstep(0.0, 0.1, abs(color - 0.5)) - glsl_smoothstep(0.8, 0.9, color);
  if (inv == 0) { return color > 0.5; }
  return color <= 0.5;
}"#,
            mode = "D(0)",
            sample = "2.0 * rnd() - 1.0",
            center = "0.0",
            hide = "cut_triskel_hide(x, y, D(1), i32(D(2)))"
        ),
    },
    VarDef {
        name: "cut_truchet",
        params: &[
            MODE,
            ParamDef::int("type", 0.0, 0.0, 2.0),
            SEED1000,
            ParamDef::range("zoom", 10.0, 1.0, 50.0),
            INVERT0,
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [mode, type, seed, zoom, invert, off_x, off_y, off_x_int, off_x_frac, off_y_int, off_y_frac]
            let xy = cut_input(ctx, a, d[0], Sample::Centered);
            let c = truchet_color(xy.0, xy.1, d[1] as i32, d[3], vec2(d[5], d[6]));
            cut_output(ctx, a, v, amount, xy, 0.0, hide_zero(d[4], c));
        },
        prepare: Some(prepare_truchet),
        wgsl: cut_wgsl!(
            r#"
fn cut_truchet_hide(x: f32, y: f32, kind: i32, zoom: f32, off_i: vec2<f32>, off_f: vec2<f32>, inv: i32) -> bool {
  // offset split into integer and fractional part to keep the tile-local position precise
  let q = vec2<f32>(x * zoom, y * zoom) + off_f;
  let ipos = floor(q) + off_i;
  let fpos = fract(q);
  let tile = glsl_truchet_pattern(fpos, glsl_random(ipos));
  var color = 0.0;
  if (kind == 0) {
    color = glsl_smoothstep(tile.x - 0.3, tile.x, tile.y) - glsl_smoothstep(tile.x, tile.x + 0.3, tile.y);
  } else if (kind == 1) {
    let l0 = length(tile);
    let l1 = length(tile - vec2<f32>(1.0));
    color = (step(l0, 0.6) - step(l0, 0.4)) + (step(l1, 0.6) - step(l1, 0.4));
  } else if (kind == 2) {
    color = step(tile.x, tile.y);
  }
  if (inv == 0) { return color == 0.0; }
  return color > 0.0;
}"#,
            mode = "D(0)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_truchet_hide(x, y, i32(D(1)), D(3), vec2<f32>(D(7), D(9)), vec2<f32>(D(8), D(10)), i32(D(4)))"
        ),
    },
    VarDef {
        name: "cut_truchetweaving",
        params: &[
            ParamDef::int("randomize", 0.0, 0.0, 1_000_000.0),
            MODE,
            ParamDef::int("type", 0.0, 0.0, 1.0),
            ParamDef::range("width", 0.15, 0.0, 0.25),
            ParamDef::range("zoom", 8.0, 0.1, 50.0),
            INVERT0,
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [randomize, mode, type, width, zoom, invert, uvx, uvy]
            let xy = cut_input(ctx, a, d[1], Sample::Centered);
            let c = truchetweaving_color(d, xy.0, xy.1);
            cut_output(ctx, a, v, amount, xy, 0.0, hide_gt(d[5], c, 0.0));
        },
        prepare: Some(prepare_truchetweaving),
        wgsl: cut_wgsl!(
            r#"
fn cut_truchetweaving_circ(uv: vec2<f32>, radius: f32, thickness: f32) -> vec4<f32> {
  let sty = length(uv);
  let t = thickness / 2.0;
  let mask = glsl_smoothstep(t + 0.01, t, abs(radius - sty));
  var alpha = glsl_smoothstep(t + 0.1, t, abs(radius - sty));
  alpha = alpha * alpha * mix(0.5, 1.0, mask);
  return vec4<f32>(glsl_atan2(uv.x, uv.y) * radius, sty, mask, alpha);
}
fn cut_truchetweaving_beam(uv: vec2<f32>, thickness: f32) -> vec4<f32> {
  let t = thickness / 2.0;
  let mask = glsl_smoothstep(t + 0.01, t, abs(uv.y));
  var alpha = glsl_smoothstep(t + 0.1, t, abs(uv.y));
  alpha = alpha * alpha * (0.5 + 0.5 * mask);
  return vec4<f32>(uv.x, uv.y, mask, alpha);
}
fn cut_truchetweaving_tile(uv_in: vec2<f32>, n: f32, cw: f32) -> f32 {
  let i3 = 0.333333333;
  let i6 = 0.166666666;
  let ir3 = 0.577350269;
  let hir3 = 0.288675134;
  var uv = uv_in - 0.5;
  uv.x /= 1.732050807;
  let r = floor(fract(n * 5.0) * 6.0) / 6.0 * 6.28;
  let s = sin(r);
  let c = cos(r);
  uv = uv * mat2x2<f32>(c, -s, s, c);
  var v1: vec4<f32>;
  var v2: vec4<f32>;
  var v3: vec4<f32>;
  if (n < 0.1) {
    v1 = cut_truchetweaving_circ(uv - vec2<f32>(0.0, i3), i6, cw);
    v2 = cut_truchetweaving_circ(uv - vec2<f32>(hir3, -i6), i6, cw);
    v3 = cut_truchetweaving_circ(uv - vec2<f32>(-hir3, -i6), i6, cw);
  } else if (n < 0.2) {
    v1 = cut_truchetweaving_circ(uv - vec2<f32>(0.0, i3), i6, cw);
    v2 = cut_truchetweaving_circ(uv - vec2<f32>(0.0, -i3), i6, cw);
    v3 = cut_truchetweaving_beam(uv, cw);
  } else if (n < 0.4) {
    v1 = cut_truchetweaving_circ(uv - vec2<f32>(-hir3, -i6), i6, cw);
    v2 = cut_truchetweaving_circ(uv - vec2<f32>(ir3, 0.0), 0.5, cw);
    v3 = cut_truchetweaving_circ(uv - vec2<f32>(hir3, 0.5), 0.5, cw);
  } else if (n < 0.8) {
    v1 = cut_truchetweaving_circ(uv - vec2<f32>(ir3, 0.0), 0.5, cw);
    v2 = cut_truchetweaving_circ(uv - vec2<f32>(-ir3, 0.0), 0.5, cw);
    v3 = cut_truchetweaving_beam(uv, cw);
  } else {
    let rot60 = mat2x2<f32>(0.5, -0.866025404, 0.866025404, 0.5);
    let rot60i = mat2x2<f32>(0.5, 0.866025404, -0.866025404, 0.5);
    v1 = cut_truchetweaving_beam(uv, cw);
    v2 = cut_truchetweaving_beam(uv * rot60, cw);
    v3 = cut_truchetweaving_beam(uv * rot60i, cw);
  }
  let d1 = fract(n * 10.0);
  var v: vec4<f32>;
  if (d1 < 0.166) {
    v = mix(v1, mix(v2, v3, v3.w), max(v2.w, v3.w));
  } else if (d1 < 0.333) {
    v = mix(v1, mix(v3, v2, v2.w), max(v2.w, v3.w));
  } else if (d1 < 0.5) {
    v = mix(v2, mix(v1, v3, v3.w), max(v1.w, v3.w));
  } else if (d1 < 0.666) {
    v = mix(v2, mix(v3, v1, v1.w), max(v1.w, v3.w));
  } else if (d1 < 0.833) {
    v = mix(v3, mix(v1, v2, v2.w), max(v1.w, v2.w));
  } else {
    v = mix(v3, mix(v2, v1, v1.w), max(v1.w, v2.w));
  }
  return v.z;
}
fn cut_truchetweaving_hide(x: f32, y: f32, kind: i32, width: f32, zoom: f32, uvx: f32, uvy: f32, inv: i32) -> bool {
  var u = vec2<f32>(x * zoom - uvx, y * zoom - uvy);
  var cw = width;
  if (kind == 1) { cw = mix(0.05, 0.25, (sin(u.x + sin(u.y)) + sin(u.y * 0.35)) * 0.25 + 0.5); }
  let s = vec2<f32>(1.0, 1.732050807);
  let a = glsl_mod2(u, s) * 2.0 - s;
  let b = glsl_mod2(u + s * 0.5, s) * 2.0 - s;
  u = u / s;
  var id = floor(u);
  if (!(dot(a, a) < dot(b, b))) {
    u = u + 0.5;
    id = floor(u) - 0.5;
  }
  let n = fract(sin(id.x * 324.23 + id.y * 5604.342) * 87654.53);
  let color = cut_truchetweaving_tile(fract(u), n, cw);
  if (inv == 0) { return color > 0.0; }
  return color <= 0.0;
}"#,
            mode = "D(1)",
            sample = "rnd() - 0.5",
            center = "0.0",
            hide = "cut_truchetweaving_hide(x, y, i32(D(2)), D(3), D(4), D(6), D(7), i32(D(5)))"
        ),
    },
];

#[cfg(test)]
mod tests {
    use super::super::{Ctx, Pt};
    use super::*;
    use crate::model::Affine;

    const NAMES: &[&str] = &[
        "cut_mandala",
        "cut_metaballs",
        "cut_pattern",
        "cut_randomtile",
        "cut_rgrid",
        "cut_shapes",
        "cut_sincos",
        "cut_snowflake",
        "cut_spiralcb",
        "cut_spiral",
        "cut_spots",
        "cut_sqcir",
        "cut_sqsplits",
        "cut_swarp",
        "cut_tileillusion",
        "cut_triantess",
        "cut_triskel",
        "cut_truchet",
        "cut_truchetweaving",
    ];

    const POINTS: &[(f64, f64, f64)] = &[
        (0.3, 0.2, 0.1),
        (-0.7, 0.5, -0.2),
        (1.3, -0.9, 0.4),
        (0.05, -0.02, 0.0),
        (-2.1, -1.7, 1.1),
        (0.6, 0.0, -0.5),
    ];

    fn run(def: &VarDef, data: &[f64], ctx: &mut Ctx, p: (f64, f64, f64)) -> (Pt, Pt) {
        let mut a = Pt { x: p.0, y: p.1, z: p.2, color: 0.5, ..Default::default() };
        let mut v = Pt { x: 0.1, y: -0.2, z: 0.05, color: 0.5, ..Default::default() };
        (def.apply)(data, ctx, &mut a, &mut v, 0.8);
        (a, v)
    }

    fn params_with(def: &VarDef, name: &str, value: f64) -> Vec<f64> {
        let mut params: Vec<f64> = def.params.iter().map(|p| p.default).collect();
        let i = def.params.iter().position(|p| p.name == name).unwrap();
        params[i] = value;
        params
    }

    #[test]
    fn all_names_present_and_unique() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert_eq!(DEFS.iter().filter(|d| d.name == *n).count(), 1, "{n}");
        }
        assert!(DEFS.iter().all(|d| d.supports_gpu()));
    }

    #[test]
    fn default_params_give_finite_results() {
        let mut ctx = Ctx::new(42, true);
        for def in DEFS {
            for mode in [0.0, 1.0] {
                let data = def.prepare_data(&params_with(def, "mode", mode), &Affine::IDENTITY);
                for &p in POINTS {
                    for _ in 0..20 {
                        let (a, v) = run(def, &data, &mut ctx, p);
                        assert!(!a.is_bad() && !v.is_bad(), "{} mode {mode} at {p:?}: v={v:?}", def.name);
                    }
                }
            }
        }
    }

    /// With random sampling every pattern (default parameters) both shows and hides points,
    /// and inverting swaps the two sets.
    #[test]
    fn patterns_hide_and_show() {
        for def in DEFS {
            let mut counts = [0usize; 2];
            for invert in [0.0, 1.0] {
                let data = def.prepare_data(&params_with(def, "invert", invert), &Affine::IDENTITY);
                let mut ctx = Ctx::new(7, false);
                let mut hidden = 0;
                for _ in 0..4000 {
                    let (_, v) = run(def, &data, &mut ctx, (0.0, 0.0, 0.0));
                    hidden += v.do_hide as usize;
                }
                assert!(hidden > 0 && hidden < 4000, "{} invert {invert}: {hidden} of 4000 hidden", def.name);
                counts[invert as usize] = hidden;
            }
            // same seed -> same samples -> complementary sets
            assert_eq!(counts[0] + counts[1], 4000, "{}", def.name);
        }
    }

    #[test]
    fn shapes_all_types_finite() {
        let def = DEFS.iter().find(|d| d.name == "cut_shapes").unwrap();
        let mut ctx = Ctx::new(3, false);
        for t in 0..=12 {
            for contour in [0.0, 1.0] {
                let data = def.prepare_data(&[1.0, t as f64, contour, 1.0, 0.0, 5.0, 0.1, 2.3], &Affine::IDENTITY);
                for _ in 0..200 {
                    let (_, v) = run(def, &data, &mut ctx, (0.0, 0.0, 0.0));
                    assert!(!v.is_bad(), "type {t}");
                }
            }
        }
    }

    #[test]
    fn java_random_matches() {
        let mut r = JavaRandom::new(0);
        assert!((r.next_double() - 0.730967787376657).abs() < 1e-15);
        let mut r = JavaRandom::new(42);
        assert!((r.next_double() - 0.7275636800328681).abs() < 1e-15);
    }

    #[test]
    fn hidden_points_are_zeroed() {
        let def = DEFS.iter().find(|d| d.name == "cut_metaballs").unwrap();
        let data = def.prepare_data(&[], &Affine::IDENTITY);
        let mut ctx = Ctx::new(11, false);
        for _ in 0..500 {
            let (_, v) = run(def, &data, &mut ctx, (0.0, 0.0, 0.0));
            if v.do_hide {
                // (0 - center) * amount
                assert!((v.x + 0.4).abs() < 1e-12 && (v.y + 0.4).abs() < 1e-12);
            } else {
                assert!(v.x.abs() <= 0.4 + 1e-12 && v.y.abs() <= 0.4 + 1e-12);
            }
        }
    }
}
