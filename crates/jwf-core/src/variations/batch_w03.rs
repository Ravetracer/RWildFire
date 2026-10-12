//! Variations ported from the JWildfire sources (batch w03).

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
use super::complex::Complex;
use super::glsl::{Vec2, abs, clamp, dot, glsl_atan2, glsl_mod, length, max, min, sign, vec2};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

/// Iteration cap for the rejection loops of `crob` (unbounded `do..while` in Java).
const CROB_MAX_TRIES: usize = 10_000;

/// `Tools.limitValue` + `(int)` cast as used by the `crop_*` parameter setters.
fn crop_int_param(v: f64, lo: f64, hi: f64) -> f64 {
    limit_value(v, lo, hi) as i32 as f64
}

/// `limitIntVal(Tools.FTOI(v), lo, hi)`.
fn ftoi_limited(v: f64, lo: i32, hi: i32) -> f64 {
    limit_value(ftoi(v), lo, hi) as f64
}

/// Shared tail of the SDF based `crop_*` variations (Jesus Sosa): points outside the shape
/// (inside when `invert`) are hidden and moved to the origin.
fn crop_sdf_output(ctx: &Ctx, a: &Pt, v: &mut Pt, amount: f64, d: f64, invert: f64) {
    let hide = if invert == 0.0 { d > 0.0 } else { d <= 0.0 };
    v.do_hide = hide;
    let (x, y) = if hide { (0.0, 0.0) } else { (a.x, a.y) };
    v.x = amount * x;
    v.y = amount * y;
    ctx.pass_z(a, v, amount);
}

// ---------------------------------------------------------------------------------------------
// Signed distance functions
// ---------------------------------------------------------------------------------------------

fn cone3d_sd_cone(p: [f64; 3], c: Vec2, h: f64) -> f64 {
    let q = vec2(c.x, -c.y) * (h / c.y);
    let w = vec2(length(vec2(p[0], p[2])), p[1]);
    let a = w - q * clamp(dot(w, q) / dot(q, q), 0.0, 1.0);
    let b = w - q * vec2(clamp(w.x / q.x, 0.0, 1.0), 1.0);
    let k = sign(q.y);
    let d = min(dot(a, a), dot(b, b));
    let s = max(k * (w.x * q.y - w.y * q.x), k * (w.y - q.y));
    d.sqrt() * sign(s)
}

fn crop_box_sd(p: Vec2, b: Vec2) -> f64 {
    let d = abs(p) - b;
    length(max(d, Vec2::splat(0.0))) + min(max(d.x, d.y), 0.0)
}

fn crop_cross_sd(p: Vec2, b: Vec2, r: f64) -> f64 {
    let mut p = abs(p);
    p = if p.y > p.x { vec2(p.y, p.x) } else { vec2(p.x, p.y) };
    let q = p - b;
    let k = max(q.y, q.x);
    let w = if k > 0.0 { q } else { vec2(b.y - p.x, -k) };
    sign(k) * length(max(w, 0.0)) + r
}

fn crop_polygon_hexagram(p: Vec2, r: f64) -> f64 {
    let k = [-0.5, 0.86602540378, 0.57735026919, 1.73205080757];
    let mut p = abs(p);
    let kxy = vec2(k[0], k[1]);
    let t0 = dot(kxy, p);
    p = p - kxy * Vec2::splat(2.0) * min(t0, 0.0);
    let kyx = vec2(k[1], k[0]);
    let t0 = dot(kyx, p);
    p = p - kyx * 2.0 * min(t0, 0.0);
    p = p - vec2(clamp(p.x, r * k[2], r * k[3]), r);
    length(p) * sign(p.y)
}

fn crop_polygon_octogon(p: Vec2, r: f64) -> f64 {
    let k = [-0.9238795325, 0.3826834323, 0.4142135623];
    let mut p = abs(p);
    let k1 = vec2(k[0], k[1]);
    p = p - k1 * (2.0 * min(dot(k1, p), 0.0));
    let k2 = vec2(-k[0], k[1]);
    p = p - k2 * (2.0 * min(dot(k2, p), 0.0));
    p = p - vec2(clamp(p.x, -k[2] * r, k[2] * r), r);
    length(p) * sign(p.y)
}

fn crop_polygon_hexagon(p: Vec2, r: f64) -> f64 {
    let k = [-0.866025404, 0.5, 0.577350269];
    let mut p = abs(p);
    let k1 = vec2(k[0], k[1]);
    p = p - k1 * (2.0 * min(dot(k1, p), 0.0));
    p = p - vec2(clamp(p.x, -k[2] * r, k[2] * r), r);
    length(p) * sign(p.y)
}

fn crop_polygon_pentagon(p: Vec2, r: f64) -> f64 {
    let k = [0.809016994, 0.587785252, 0.726542528];
    let mut p = vec2(p.x.abs(), -p.y);
    let k1 = vec2(-k[0], k[1]);
    p = p - k1 * (2.0 * min(dot(k1, p), 0.0));
    let k2 = vec2(k[0], k[1]);
    p = p - k2 * (2.0 * min(dot(k2, p), 0.0));
    p = p - vec2(clamp(p.x, -r * k[2], r * k[2]), r);
    length(p) * sign(p.y)
}

fn crop_rhombus_ndot(a: Vec2, b: Vec2) -> f64 {
    a.x * b.x - a.y * b.y
}

fn crop_rhombus_sd(p: Vec2, b: Vec2) -> f64 {
    let q = abs(p);
    let h = clamp((-2.0 * crop_rhombus_ndot(q, b) + crop_rhombus_ndot(b, b)) / dot(b, b), -1.0, 1.0);
    let d = length(q - b * vec2(1.0 - h, 1.0 + h) * 0.5);
    d * sign(q.x * b.y + q.y * b.x - b.x * b.y)
}

fn crop_stars_sd(p: Vec2, r: f64, n: i32, m: f64) -> f64 {
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

fn crop_trapezoid_sd(p: Vec2, r1: f64, r2: f64, he: f64) -> f64 {
    let k1 = vec2(r2, he);
    let k2 = vec2(r2 - r1, 2.0 * he);
    let p = vec2(p.x.abs(), p.y);
    let ca = vec2(max(0.0, p.x - if p.y < 0.0 { r1 } else { r2 }), p.y.abs() - he);
    let t0 = dot(k1 - p, k2) / dot(k2, k2);
    let cb = p - k1 + k2 * clamp(t0, 0.0, 1.0);
    let s = if cb.x < 0.0 && ca.y < 0.0 { -1.0 } else { 1.0 };
    s * java_min(dot(ca, ca), dot(cb, cb)).sqrt()
}

fn crop_triangle_area(x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) -> f64 {
    ((x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2)) / 2.0).abs()
}

fn crop_vesica_sd(p: Vec2, r: f64, d: f64) -> f64 {
    let p = abs(p);
    let b = (r * r - d * d).sqrt();
    if (p.y - b) * d > p.x * b {
        length(p - vec2(0.0, b))
    } else {
        length(p - vec2(-d, 0.0)) - r
    }
}

fn crop_x_sd(p: Vec2, w: f64, r: f64) -> f64 {
    let p = abs(p);
    length(p - Vec2::splat(java_min(p.x + p.y, w) * 0.5)) - r
}

// ---------------------------------------------------------------------------------------------
// complex (ComplexFunc): sum of the weighted *2_bs functions
// ---------------------------------------------------------------------------------------------

const COMPLEX_PARAMS: &[ParamDef] = &[
    ParamDef::new("cospow", 1.0),
    ParamDef::new("cosx1", 1.0),
    ParamDef::new("cosx2", 1.0),
    ParamDef::new("cosy1", 1.0),
    ParamDef::new("cosy2", 1.0),
    ParamDef::new("coshpow", 0.0),
    ParamDef::new("coshx1", 1.0),
    ParamDef::new("coshx2", 1.0),
    ParamDef::new("coshy1", 1.0),
    ParamDef::new("coshy2", 1.0),
    ParamDef::new("cotpow", 0.0),
    ParamDef::new("cotx1", 2.0),
    ParamDef::new("cotx2", 2.0),
    ParamDef::new("coty1", 2.0),
    ParamDef::new("coty2", 2.0),
    ParamDef::new("cothpow", 0.0),
    ParamDef::new("cothx1", 2.0),
    ParamDef::new("cothx2", 2.0),
    ParamDef::new("cothy1", 2.0),
    ParamDef::new("cothy2", 2.0),
    ParamDef::new("cscpow", 0.0),
    ParamDef::new("cscx1", 1.0),
    ParamDef::new("cscx2", 1.0),
    ParamDef::new("cscy1", 1.0),
    ParamDef::new("cscy2", 1.0),
    ParamDef::new("cschpow", 0.0),
    ParamDef::new("cschx1", 1.0),
    ParamDef::new("cschx2", 1.0),
    ParamDef::new("cschy1", 1.0),
    ParamDef::new("cschy2", 1.0),
    ParamDef::new("exppow", 0.0),
    ParamDef::new("expx1", 1.0),
    ParamDef::new("expy1", 1.0),
    ParamDef::new("expy2", 1.0),
    ParamDef::new("secpow", 0.0),
    ParamDef::new("secx1", 1.0),
    ParamDef::new("secx2", 1.0),
    ParamDef::new("secy1", 1.0),
    ParamDef::new("secy2", 1.0),
    ParamDef::new("sechpow", 0.0),
    ParamDef::new("sechx1", 1.0),
    ParamDef::new("sechx2", 1.0),
    ParamDef::new("sechy1", 1.0),
    ParamDef::new("sechy2", 1.0),
    ParamDef::new("sinpow", 0.0),
    ParamDef::new("sinx1", 1.0),
    ParamDef::new("sinx2", 1.0),
    ParamDef::new("siny1", 1.0),
    ParamDef::new("siny2", 1.0),
    ParamDef::new("sinhpow", 0.0),
    ParamDef::new("sinhx1", 1.0),
    ParamDef::new("sinhx2", 1.0),
    ParamDef::new("sinhy1", 1.0),
    ParamDef::new("sinhy2", 1.0),
    ParamDef::new("tanpow", 0.0),
    ParamDef::new("tanx1", 2.0),
    ParamDef::new("tanx2", 2.0),
    ParamDef::new("tany1", 2.0),
    ParamDef::new("tany2", 2.0),
    ParamDef::new("tanhpow", 0.0),
    ParamDef::new("tanhx1", 2.0),
    ParamDef::new("tanhx2", 2.0),
    ParamDef::new("tanhy1", 2.0),
    ParamDef::new("tanhy2", 2.0),
];

fn complex_apply(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64) {
    let (px, py) = (a.x, a.y);
    let mut x = 0.0;
    let mut y = 0.0;
    // cos
    if d[0] != 0.0 {
        let cossin = (px * d[1]).sin();
        let coscos = (px * d[2]).cos();
        let cossinh = (py * d[3]).sinh();
        let coscosh = (py * d[4]).cosh();
        x += d[0] * coscos * coscosh;
        y -= d[0] * cossin * cossinh;
    }
    // cosh
    if d[5] != 0.0 {
        let coshsin = (py * d[8]).sin();
        let coshcos = (py * d[9]).cos();
        let coshsinh = (px * d[6]).sinh();
        let coshcosh = (px * d[7]).cosh();
        x += d[5] * coshcosh * coshcos;
        y += d[5] * coshsinh * coshsin;
    }
    // cot
    if d[10] != 0.0 {
        let cotsin = (d[11] * px).sin();
        let cotcos = (d[12] * px).cos();
        let cotsinh = (d[13] * py).sinh();
        let cotcosh = (d[14] * py).cosh();
        let cotden = 1.0 / (cotcosh - cotcos);
        x += d[10] * cotden * cotsin;
        y -= d[10] * cotden * cotsinh;
    }
    // coth
    if d[15] != 0.0 {
        let cothsin = (d[18] * py).sin();
        let cothcos = (d[19] * py).cos();
        let cothsinh = (d[16] * px).sinh();
        let cothcosh = (d[17] * px).cosh();
        let cothd = cothcosh - cothcos;
        if cothd == 0.0 {
            return;
        }
        let cothden = 1.0 / cothd;
        x += d[15] * cothden * cothsinh;
        y += d[15] * cothden * cothsin;
    }
    // csc
    if d[20] != 0.0 {
        let cscsin = (px * d[21]).sin();
        let csccos = (px * d[22]).cos();
        let cscsinh = (py * d[23]).sinh();
        let csccosh = (py * d[24]).cosh();
        let cscd = (2.0 * py).cosh() - (2.0 * px).cos();
        if cscd == 0.0 {
            return;
        }
        let cscden = 2.0 / cscd;
        x += d[20] * cscden * cscsin * csccosh;
        y -= d[20] * cscden * csccos * cscsinh;
    }
    // csch
    if d[25] != 0.0 {
        let cschsin = (py * d[28]).sin();
        let cschcos = (py * d[29]).cos();
        let cschsinh = (px * d[26]).sinh();
        let cschcosh = (px * d[27]).cosh();
        let cschd = (2.0 * px).cosh() - (2.0 * py).cos();
        if cschd == 0.0 {
            return;
        }
        let cschden = 2.0 / cschd;
        x += d[25] * cschden * cschsinh * cschcos;
        y -= d[25] * cschden * cschcosh * cschsin;
    }
    // exp
    if d[30] != 0.0 {
        let expe = (px * d[31]).exp();
        let expsin = (py * d[32]).sin();
        let expcos = (py * d[33]).cos();
        x += d[30] * expe * expcos;
        y += d[30] * expe * expsin;
    }
    // sec
    if d[34] != 0.0 {
        let secsin = (px * d[35]).sin();
        let seccos = (px * d[36]).cos();
        let secsinh = (py * d[37]).sinh();
        let seccosh = (py * d[38]).cosh();
        let secd = (2.0 * px).cos() + (2.0 * py).cosh();
        if secd == 0.0 {
            return;
        }
        let secden = 2.0 / secd;
        x += d[34] * secden * seccos * seccosh;
        y += d[34] * secden * secsin * secsinh;
    }
    // sech
    if d[39] != 0.0 {
        let sechsinh = (py * d[42]).sin();
        let sechcosh = (py * d[43]).cos();
        let sechsin = (px * d[40]).sinh();
        let sechcos = (px * d[41]).cosh();
        let sechd = (2.0 * py).cos() + (2.0 * px).cosh();
        if sechd == 0.0 {
            return;
        }
        let sechden = 2.0 / sechd;
        x += d[39] * sechden * sechcos * sechcosh;
        y -= d[39] * sechden * sechsin * sechsinh;
    }
    // tan
    if d[54] != 0.0 {
        let tansin = (d[55] * px).sin();
        let tancos = (d[56] * px).cos();
        let tansinh = (d[57] * py).sinh();
        let tancosh = (d[58] * py).cosh();
        let tand = tancos + tancosh;
        if tand == 0.0 {
            return;
        }
        let tanden = 1.0 / tand;
        x += d[54] * tanden * tansin;
        y += d[54] * tanden * tansinh;
    }
    // tanh
    if d[59] != 0.0 {
        let tanhsin = (py * d[62]).sin();
        let tanhcos = (py * d[63]).cos();
        let tanhsinh = (px * d[60]).sinh();
        let tanhcosh = (px * d[61]).cosh();
        let tanhd = tanhcos + tanhcosh;
        if tanhd == 0.0 {
            return;
        }
        let tanhden = 1.0 / tanhd;
        x += d[59] * tanhden * tanhsinh;
        y += d[59] * tanhden * tanhsin;
    }
    // sin
    if d[44] != 0.0 {
        let sinsin = (px * d[45]).sin();
        let sincos = (px * d[46]).cos();
        let sinsinh = (py * d[47]).sinh();
        let sincosh = (py * d[48]).cosh();
        x += d[44] * sinsin * sincosh;
        y += d[44] * sincos * sinsinh;
    }
    // sinh
    if d[49] != 0.0 {
        let sinhsin = (py * d[52]).sin();
        let sinhcos = (py * d[53]).cos();
        let sinhsinh = (px * d[50]).sinh();
        let sinhcosh = (px * d[51]).cosh();
        x += d[49] * sinhsinh * sinhcos;
        y += d[49] * sinhcosh * sinhsin;
    }
    v.x += amount * x;
    v.y += amount * y;
    ctx.pass_z(a, v, amount);
}

const COMPLEX_WGSL: &str = "let px = a.x;
         let py = a.y;
         var x = 0.0;
         var y = 0.0;
         if (D(0) != 0.0) {
           x += D(0) * cos(px * D(2)) * cosh(py * D(4));
           y -= D(0) * sin(px * D(1)) * sinh(py * D(3));
         }
         if (D(5) != 0.0) {
           x += D(5) * cosh(px * D(7)) * cos(py * D(9));
           y += D(5) * sinh(px * D(6)) * sin(py * D(8));
         }
         if (D(10) != 0.0) {
           let cotden = 1.0 / (cosh(D(14) * py) - cos(D(12) * px));
           x += D(10) * cotden * sin(D(11) * px);
           y -= D(10) * cotden * sinh(D(13) * py);
         }
         if (D(15) != 0.0) {
           let cothd = cosh(D(17) * px) - cos(D(19) * py);
           if (cothd == 0.0) { return PtPair(a, v); }
           let cothden = 1.0 / cothd;
           x += D(15) * cothden * sinh(D(16) * px);
           y += D(15) * cothden * sin(D(18) * py);
         }
         if (D(20) != 0.0) {
           let cscd = cosh(2.0 * py) - cos(2.0 * px);
           if (cscd == 0.0) { return PtPair(a, v); }
           let cscden = 2.0 / cscd;
           x += D(20) * cscden * sin(px * D(21)) * cosh(py * D(24));
           y -= D(20) * cscden * cos(px * D(22)) * sinh(py * D(23));
         }
         if (D(25) != 0.0) {
           let cschd = cosh(2.0 * px) - cos(2.0 * py);
           if (cschd == 0.0) { return PtPair(a, v); }
           let cschden = 2.0 / cschd;
           x += D(25) * cschden * sinh(px * D(26)) * cos(py * D(29));
           y -= D(25) * cschden * cosh(px * D(27)) * sin(py * D(28));
         }
         if (D(30) != 0.0) {
           let expe = exp(px * D(31));
           x += D(30) * expe * cos(py * D(33));
           y += D(30) * expe * sin(py * D(32));
         }
         if (D(34) != 0.0) {
           let secd = cos(2.0 * px) + cosh(2.0 * py);
           if (secd == 0.0) { return PtPair(a, v); }
           let secden = 2.0 / secd;
           x += D(34) * secden * cos(px * D(36)) * cosh(py * D(38));
           y += D(34) * secden * sin(px * D(35)) * sinh(py * D(37));
         }
         if (D(39) != 0.0) {
           let sechd = cos(2.0 * py) + cosh(2.0 * px);
           if (sechd == 0.0) { return PtPair(a, v); }
           let sechden = 2.0 / sechd;
           x += D(39) * sechden * cosh(px * D(41)) * cos(py * D(43));
           y -= D(39) * sechden * sinh(px * D(40)) * sin(py * D(42));
         }
         if (D(54) != 0.0) {
           let tand = cos(D(56) * px) + cosh(D(58) * py);
           if (tand == 0.0) { return PtPair(a, v); }
           let tanden = 1.0 / tand;
           x += D(54) * tanden * sin(D(55) * px);
           y += D(54) * tanden * sinh(D(57) * py);
         }
         if (D(59) != 0.0) {
           let tanhd = cos(py * D(63)) + cosh(px * D(61));
           if (tanhd == 0.0) { return PtPair(a, v); }
           let tanhden = 1.0 / tanhd;
           x += D(59) * tanhden * sinh(px * D(60));
           y += D(59) * tanhden * sin(py * D(62));
         }
         if (D(44) != 0.0) {
           x += D(44) * sin(px * D(45)) * cosh(py * D(48));
           y += D(44) * cos(px * D(46)) * sinh(py * D(47));
         }
         if (D(49) != 0.0) {
           x += D(49) * sinh(px * D(50)) * cos(py * D(53));
           y += D(49) * cosh(px * D(51)) * sin(py * D(52));
         }
         v.x += AMOUNT * x;
         v.y += AMOUNT * y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

// ---------------------------------------------------------------------------------------------
// Parameter tables of the *2_bs family
// ---------------------------------------------------------------------------------------------

const BS_PARAMS_1: &[ParamDef] = &[
    ParamDef::new("x1", 1.0),
    ParamDef::new("x2", 1.0),
    ParamDef::new("y1", 1.0),
    ParamDef::new("y2", 1.0),
];

const BS_PARAMS_2: &[ParamDef] = &[
    ParamDef::new("x1", 2.0),
    ParamDef::new("x2", 2.0),
    ParamDef::new("y1", 2.0),
    ParamDef::new("y2", 2.0),
];

/// `cpow2` / `cpow3_wf` precalculation: `[ang, c, d, half_c, half_d]`.
fn cpow2_coefs(r: f64, a: f64, divisor: f64) -> [f64; 5] {
    let ang = 2.0 * M_PI / divisor;
    let c = r * (M_PI / 2.0 * a).cos() / divisor;
    let d = r * (M_PI / 2.0 * a).sin() / divisor;
    [ang, c, d, c / 2.0, d / 2.0]
}

// ---------------------------------------------------------------------------------------------
// WGSL snippets of the doHide variations (cone3D, crop*)
// ---------------------------------------------------------------------------------------------

const CONE3D_WGSL: &str = "fn cone3D_sd_cone(p: vec3<f32>, c: vec2<f32>, h: f32) -> f32 {
  let q = vec2<f32>(c.x, -c.y) * (h / c.y);
  let w = vec2<f32>(length(vec2<f32>(p.x, p.z)), p.y);
  let a = w - q * clamp(dot(w, q) / dot(q, q), 0.0, 1.0);
  let b = w - q * vec2<f32>(clamp(w.x / q.x, 0.0, 1.0), 1.0);
  let k = sign(q.y);
  let d = min(dot(a, a), dot(b, b));
  let s = max(k * (w.x * q.y - w.y * q.x), k * (w.y - q.y));
  return sqrt(d) * sign(s);
}
@body
var found = false;
for (var i = 0; i < 50; i++) {
  let x = rnd() - 0.5;
  let y = rnd() - 0.5;
  let z = rnd() - 0.5;
  if (cone3D_sd_cone(vec3<f32>(x, y, z), vec2<f32>(D(0), D(1)), D(2)) < 0.0) {
    v.hide = 0.0;
    v.x += AMOUNT * x;
    v.y += AMOUNT * y;
    v.z += AMOUNT * z;
    found = true;
    break;
  }
}
if (!found) { v.hide = 1.0; }";

const CROP3D_WGSL: &str = "var x = a.x;
var y = a.y;
var z = a.z;
if ((x < D(8) || x > D(9) || y < D(10) || y > D(11) || z < D(12) || z > D(13)) && D(7) != 0.0) {
  v.x = 0.0;
  v.y = 0.0;
  v.z = 0.0;
  v.hide = 1.0;
  return PtPair(a, v);
}
v.hide = 0.0;
if (x < D(8)) { x = D(8) + rnd() * D(14); } else if (x > D(9)) { x = D(9) - rnd() * D(14); }
if (y < D(10)) { y = D(10) + rnd() * D(15); } else if (y > D(11)) { y = D(11) - rnd() * D(15); }
if (z < D(12)) { z = D(12) + rnd() * D(16); } else if (z > D(13)) { z = D(13) - rnd() * D(16); }
v.x = AMOUNT * x;
v.y = AMOUNT * y;
v.z = AMOUNT * z;";

const CROP_WGSL: &str = "var x = a.x;
var y = a.y;
if ((x < D(6) || x > D(7) || y < D(8) || y > D(9)) && D(5) != 0.0) {
  v.x = 0.0;
  v.y = 0.0;
  v.hide = 1.0;
  return PtPair(a, v);
}
v.hide = 0.0;
if (x < D(6)) { x = D(6) + rnd() * D(10); } else if (x > D(7)) { x = D(7) - rnd() * D(10); }
if (y < D(8)) { y = D(8) + rnd() * D(11); } else if (y > D(9)) { y = D(9) - rnd() * D(11); }
v.x = AMOUNT * x;
v.y = AMOUNT * y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

/// Shared tail of the SDF crop snippets (`sd` = signed distance, `D(inv)` = invert flag).
macro_rules! crop_sdf_tail {
    ($inv:literal) => {
        concat!(
            "var hide = sd > 0.0;
if (D(",
            $inv,
            ") != 0.0) { hide = sd <= 0.0; }
if (hide) {
  v.hide = 1.0;
  v.x = 0.0;
  v.y = 0.0;
} else {
  v.hide = 0.0;
  v.x = AMOUNT * a.x;
  v.y = AMOUNT * a.y;
}
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }"
        )
    };
}

const CROP_BOX_WGSL: &str = concat!(
    "fn crop_box_sd(p: vec2<f32>, b: vec2<f32>) -> f32 {
  let d = abs(p) - b;
  return length(max(d, vec2<f32>(0.0))) + min(max(d.x, d.y), 0.0);
}
@body
let sd = crop_box_sd(vec2<f32>(a.x, a.y), vec2<f32>(D(0), D(1)));
",
    crop_sdf_tail!("2")
);

const CROP_CROSS_WGSL: &str = concat!(
    "fn crop_cross_sd(p_in: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
  var p = abs(p_in);
  if (p.y > p.x) { p = p.yx; }
  let q = p - b;
  let k = max(q.y, q.x);
  var w = q;
  if (!(k > 0.0)) { w = vec2<f32>(b.y - p.x, -k); }
  return sign(k) * length(max(w, vec2<f32>(0.0))) + r;
}
@body
var si = vec2<f32>(D(0), D(1));
if (si.x < si.y) { si = si.yx; }
let sd = crop_cross_sd(vec2<f32>(a.x, a.y), si, 0.1 * sin(D(2) * 1.2));
",
    crop_sdf_tail!("3")
);

const CROP_POLYGON_WGSL: &str = concat!(
    "fn crop_polygon_hexagram(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec4<f32>(-0.5, 0.86602540378, 0.57735026919, 1.73205080757);
  var p = abs(p_in);
  p -= k.xy * 2.0 * min(dot(k.xy, p), 0.0);
  p -= k.yx * 2.0 * min(dot(k.yx, p), 0.0);
  p -= vec2<f32>(clamp(p.x, r * k.z, r * k.w), r);
  return length(p) * sign(p.y);
}
fn crop_polygon_octogon(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec3<f32>(-0.9238795325, 0.3826834323, 0.4142135623);
  var p = abs(p_in);
  p -= k.xy * (2.0 * min(dot(k.xy, p), 0.0));
  let k2 = vec2<f32>(-k.x, k.y);
  p -= k2 * (2.0 * min(dot(k2, p), 0.0));
  p -= vec2<f32>(clamp(p.x, -k.z * r, k.z * r), r);
  return length(p) * sign(p.y);
}
fn crop_polygon_hexagon(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec3<f32>(-0.866025404, 0.5, 0.577350269);
  var p = abs(p_in);
  p -= k.xy * (2.0 * min(dot(k.xy, p), 0.0));
  p -= vec2<f32>(clamp(p.x, -k.z * r, k.z * r), r);
  return length(p) * sign(p.y);
}
fn crop_polygon_pentagon(p_in: vec2<f32>, r: f32) -> f32 {
  let k = vec3<f32>(0.809016994, 0.587785252, 0.726542528);
  var p = vec2<f32>(abs(p_in.x), -p_in.y);
  let k1 = vec2<f32>(-k.x, k.y);
  p -= k1 * (2.0 * min(dot(k1, p), 0.0));
  p -= k.xy * (2.0 * min(dot(k.xy, p), 0.0));
  p -= vec2<f32>(clamp(p.x, -r * k.z, r * k.z), r);
  return length(p) * sign(p.y);
}
@body
let p = vec2<f32>(a.x, a.y);
let ty = i32(D(0));
var sd = 0.0;
if (ty == 4) {
  sd = crop_polygon_hexagram(p, D(1));
} else if (ty == 3) {
  sd = crop_polygon_octogon(p, D(1));
} else if (ty == 2) {
  sd = crop_polygon_hexagon(p, D(1));
} else if (ty == 1) {
  sd = crop_polygon_pentagon(p, D(1));
} else if (ty == 0) {
  sd = length(p) - D(1);
}
",
    crop_sdf_tail!("2")
);

const CROP_RHOMBUS_WGSL: &str = concat!(
    "fn crop_rhombus_ndot(a: vec2<f32>, b: vec2<f32>) -> f32 { return a.x * b.x - a.y * b.y; }
fn crop_rhombus_sd(p: vec2<f32>, b: vec2<f32>) -> f32 {
  let q = abs(p);
  let h = clamp((-2.0 * crop_rhombus_ndot(q, b) + crop_rhombus_ndot(b, b)) / dot(b, b), -1.0, 1.0);
  let d = length(q - b * vec2<f32>(1.0 - h, 1.0 + h) * 0.5);
  return d * sign(q.x * b.y + q.y * b.x - b.x * b.y);
}
@body
let sd = crop_rhombus_sd(vec2<f32>(a.x, a.y), vec2<f32>(D(0), D(1)));
",
    crop_sdf_tail!("2")
);

const CROP_STARS_WGSL: &str = concat!(
    "fn crop_stars_sd(p_in: vec2<f32>, r: f32, n: f32, m: f32) -> f32 {
  let an = 3.141593 / n;
  let en = 6.283185 / m;
  let acs = vec2<f32>(cos(an), sin(an));
  let ecs = vec2<f32>(cos(en), sin(en));
  let bn = glsl_mod(glsl_atan2(p_in.x, p_in.y), 2.0 * an) - an;
  var p = vec2<f32>(cos(bn), abs(sin(bn))) * length(p_in);
  p -= acs * r;
  p += ecs * clamp(-dot(p, ecs), 0.0, r * acs.y / ecs.y);
  return length(p) * sign(p.x);
}
@body
let m = 4.0 + D(2) * D(2) * (D(1) * 2.0 - 4.0);
let sd = crop_stars_sd(vec2<f32>(a.x, a.y), D(0), D(1), m);
",
    crop_sdf_tail!("3")
);

const CROP_TRAPEZOID_WGSL: &str = concat!(
    "fn crop_trapezoid_sd(p_in: vec2<f32>, r1: f32, r2: f32, he: f32) -> f32 {
  let k1 = vec2<f32>(r2, he);
  let k2 = vec2<f32>(r2 - r1, 2.0 * he);
  let p = vec2<f32>(abs(p_in.x), p_in.y);
  var rr = r2;
  if (p.y < 0.0) { rr = r1; }
  let ca = vec2<f32>(max(0.0, p.x - rr), abs(p.y) - he);
  let t0 = dot(k1 - p, k2) / dot(k2, k2);
  let cb = p - k1 + k2 * clamp(t0, 0.0, 1.0);
  var s = 1.0;
  if (cb.x < 0.0 && ca.y < 0.0) { s = -1.0; }
  return s * sqrt(min(dot(ca, ca), dot(cb, cb)));
}
@body
let sd = crop_trapezoid_sd(vec2<f32>(a.x, a.y), D(0), D(1), D(2));
",
    crop_sdf_tail!("3")
);

const CROP_TRIANGLE_WGSL: &str = "fn crop_triangle_area(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) -> f32 {
  return abs((x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2)) / 2.0);
}
@body
let x = a.x;
let y = a.y;
let area = crop_triangle_area(D(0), D(1), D(2), D(3), D(4), D(5));
let a1 = crop_triangle_area(x, y, D(2), D(3), D(4), D(5));
let a2 = crop_triangle_area(D(0), D(1), x, y, D(4), D(5));
let a3 = crop_triangle_area(D(0), D(1), D(2), D(3), x, y);
v.hide = 1.0;
if (area == a1 + a2 + a3) { v.hide = 0.0; }
v.x += AMOUNT * x;
v.y += AMOUNT * y;";

const CROP_VESICA_WGSL: &str = concat!(
    "fn crop_vesica_sd(p_in: vec2<f32>, r: f32, d: f32) -> f32 {
  let p = abs(p_in);
  let b = sqrt(r * r - d * d);
  if ((p.y - b) * d > p.x * b) {
    return length(p - vec2<f32>(0.0, b));
  }
  return length(p - vec2<f32>(-d, 0.0)) - r;
}
@body
let sd = crop_vesica_sd(vec2<f32>(a.x, a.y), 1.0 + D(1), 0.8 + D(0));
",
    crop_sdf_tail!("2")
);

const CROP_X_WGSL: &str = "let p = abs(vec2<f32>(a.x, a.y));
let sd = length(p - vec2<f32>(min(p.x + p.y, D(1)) * 0.5)) - D(0);
var hide = sd > 0.0;
if (D(2) != 0.0) { hide = sd <= 0.0; }
v.hide = select(0.0, 1.0, hide);
v.x = AMOUNT * a.x;
v.y = AMOUNT * a.y;
if (PRESERVE_Z) { v.z += AMOUNT * a.z; }";

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "combimirror",
        params: &[
            ParamDef::range("vmirror", 1.0, 0.0, 2.0),
            ParamDef::new("vmove", 0.05),
            ParamDef::range("hmirror", 0.5, 0.0, 2.0),
            ParamDef::new("hmove", 0.35),
            ParamDef::range("zmirror", 0.0, 0.0, 2.0),
            ParamDef::new("zmove", 0.0),
            ParamDef::range("pmirror", 0.0, 0.0, 2.0),
            ParamDef::new("pmovex", 0.05),
            ParamDef::new("pmovey", 0.0),
            ParamDef::range("vcolorshift", 0.0, -1.0, 1.0),
            ParamDef::range("hcolorshift", 0.0, -1.0, 1.0),
            ParamDef::range("zcolorshift", 0.0, -1.0, 1.0),
            ParamDef::range("pcolorshift", 0.0, -1.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [vmirror, vmove, hmirror, hmove, zmirror, zmove, pmirror, pmovex, pmovey,
            //      vcolorshift, hcolorshift, zcolorshift, pcolorshift]
            v.x = a.x * amount;
            v.y = a.y * amount;
            v.z = a.z * amount;
            if ctx.random() > d[6] / 2.0 {
                v.x = -v.x + d[7];
                v.y = -v.y + d[8];
                v.color = fmod(v.color + d[12], 1.0);
            }
            if ctx.random() < d[0] / 2.0 {
                v.x = -v.x + d[1];
                v.color = fmod(v.color + d[9], 1.0);
            }
            if ctx.random() < d[2] / 2.0 {
                v.y = -v.y + d[3];
                v.color = fmod(v.color + d[10], 1.0);
            }
            if ctx.random() < d[4] / 2.0 {
                v.z = -v.z + d[5];
                v.color = fmod(v.color + d[11], 1.0);
            }
        },
        prepare: None,
        wgsl: "v.x = a.x * AMOUNT;
         v.y = a.y * AMOUNT;
         v.z = a.z * AMOUNT;
         if (rnd() > D(6) / 2.0) {
           v.x = -v.x + D(7);
           v.y = -v.y + D(8);
           v.color = fmod(v.color + D(12), 1.0);
         }
         if (rnd() < D(0) / 2.0) {
           v.x = -v.x + D(1);
           v.color = fmod(v.color + D(9), 1.0);
         }
         if (rnd() < D(2) / 2.0) {
           v.y = -v.y + D(3);
           v.color = fmod(v.color + D(10), 1.0);
         }
         if (rnd() < D(4) / 2.0) {
           v.z = -v.z + D(5);
           v.color = fmod(v.color + D(11), 1.0);
         }",
    },
    VarDef {
        name: "complex",
        params: COMPLEX_PARAMS,
        kind: VarKind::Normal,
        is_3d: false,
        apply: complex_apply,
        prepare: None,
        wgsl: COMPLEX_WGSL,
    },
    VarDef {
        name: "cone3D",
        params: &[ParamDef::range("w", 0.25, 0.0, 1.0), ParamDef::range("h", 0.25, 0.0, 1.0), ParamDef::range("Ty", 0.10, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [w, h, Ty] (limited like the Java setter)
            for _ in 1..=50 {
                let x = ctx.random() - 0.5;
                let y = ctx.random() - 0.5;
                let z = ctx.random() - 0.5;
                if cone3d_sd_cone([x, y, z], vec2(d[0], d[1]), d[2]) < 0.0 {
                    v.do_hide = false;
                    v.x += amount * x;
                    v.y += amount * y;
                    v.z += amount * z;
                    return;
                }
            }
            v.do_hide = true;
        },
        prepare: Some(|p, _| vec![limit_value(p[0], 0.0, 1.0), limit_value(p[1], 0.0, 1.0), limit_value(p[2], -1.0, 1.0)]),
        wgsl: CONE3D_WGSL,
    },
    VarDef {
        name: "cone",
        params: &[
            ParamDef::new("radius1", 0.5),
            ParamDef::new("radius2", 1.0),
            ParamDef::new("size1", 0.5),
            ParamDef::new("size2", 2.0),
            ParamDef::new("ywave", 1.0),
            ParamDef::new("xwave", 1.0),
            ParamDef::new("height", 1.0),
            ParamDef::new("warp", 1.0),
            ParamDef::range("weight", 2.0, 0.0, 10.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [radius1, radius2, size1, size2, ywave, xwave, height, warp, weight]
            let r = amount / (a.x * a.x * d[7] + a.y * a.y + d[2]).sqrt() * d[3];
            let xx = a.atan_xy() * d[0] + M_PI * ((d[8] * ctx.random()) as i32) as f64 * d[1];
            let sina = (xx * d[4]).sin();
            let cosa = (xx * d[5]).cos();
            v.x += r * cosa;
            v.y += r * sina;
            v.z += r * d[6];
        },
        prepare: None,
        wgsl: "let r = AMOUNT / sqrt(a.x * a.x * D(7) + a.y * a.y + D(2)) * D(3);
         let xx = atan_xy(a) * D(0) + PI * f32(i32(D(8) * rnd())) * D(1);
         v.x += r * cos(xx * D(5));
         v.y += r * sin(xx * D(4));
         v.z += r * D(6);",
    },
    VarDef {
        name: "conicalSpiral",
        params: &[
            ParamDef::range("turns", 8.0, -25.0, 25.0),
            ParamDef::range("radius", 1.2, -8.0, 8.0),
            ParamDef::range("height", 1.0, -8.0, 8.0),
            ParamDef::int("mode", 0.0, 0.0, 3.0),
            ParamDef::range("z_wave_freq", 0.0, -8.0, 8.0),
            ParamDef::range("z_wave_amp", 0.0, -5.0, 5.0),
            ParamDef::range("twist", 0.0, -5.0, 5.0),
            ParamDef::range("thickness", 0.0, 0.0, 1.0),
            ParamDef::int("solid", 0.0, 0.0, 1.0),
            ParamDef::int("colorize", 0.0, 0.0, 1.0),
            ParamDef::int("colorMode", 0.0, 0.0, 4.0),
            ParamDef::range("colorSpeed", 1.0, 0.0, 3.0),
            ParamDef::range("colorOffset", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, a, v, amount| {
            // d = [turns, radius, height, mode, z_wave_freq, z_wave_amp, twist, thickness, solid,
            //      colorize, colorMode, colorSpeed, colorOffset]
            let mode = d[3] as i32;
            let t = if mode == 1 {
                fmod(a.y.abs(), 1.0)
            } else if mode == 2 {
                fmod(a.z.abs(), 1.0)
            } else {
                fmod(a.x.abs(), 1.0)
            };
            let angle = t * M_2PI * d[0];
            let current_radius = t * d[1];
            let mut z = t * d[2];
            if d[5] != 0.0 {
                z += (t * M_2PI * d[4]).sin() * d[5];
            }
            let mut final_angle = angle;
            if d[6] != 0.0 {
                final_angle += z * d[6];
            }
            let mut x = current_radius * final_angle.cos();
            let mut y = current_radius * final_angle.sin();
            if d[7] > 0.0 {
                if d[8] > 0.0 {
                    x += ctx.random() * d[7];
                    y += ctx.random() * d[7];
                    z += ctx.random() * d[7];
                } else {
                    x += a.x * d[7];
                    y += a.y * d[7];
                    z += a.z * d[7];
                }
            }
            v.x += x * amount;
            v.y += y * amount;
            v.z += z * amount;
            if d[9] > 0.0 {
                let color_driver = match d[10] as i32 {
                    0 => t,
                    1 => current_radius,
                    2 => fmod(angle, M_2PI) / M_2PI,
                    _ => z,
                };
                v.color = fmod((color_driver * d[11] + d[12]).abs(), 1.0);
            }
        },
        prepare: Some(|p, _| {
            let mut d = p.to_vec();
            d[3] = ftoi_limited(p[3], 0, 3);
            d[8] = ftoi_limited(p[8], 0, 1);
            d[9] = ftoi_limited(p[9], 0, 1);
            d[10] = ftoi_limited(p[10], 0, 4);
            d
        }),
        wgsl: "let mode = i32(D(3));
         var t = fmod(abs(a.x), 1.0);
         if (mode == 1) {
           t = fmod(abs(a.y), 1.0);
         } else if (mode == 2) {
           t = fmod(abs(a.z), 1.0);
         }
         let angle = t * TWO_PI * D(0);
         let cur_r = t * D(1);
         var z = t * D(2);
         if (D(5) != 0.0) { z += sin(t * TWO_PI * D(4)) * D(5); }
         var fa = angle;
         if (D(6) != 0.0) { fa += z * D(6); }
         var x = cur_r * cos(fa);
         var y = cur_r * sin(fa);
         if (D(7) > 0.0) {
           if (D(8) > 0.0) {
             x += rnd() * D(7);
             y += rnd() * D(7);
             z += rnd() * D(7);
           } else {
             x += a.x * D(7);
             y += a.y * D(7);
             z += a.z * D(7);
           }
         }
         v.x += x * AMOUNT;
         v.y += y * AMOUNT;
         v.z += z * AMOUNT;
         if (D(9) > 0.0) {
           let cm = i32(D(10));
           var cd = z;
           if (cm == 0) {
             cd = t;
           } else if (cm == 1) {
             cd = cur_r;
           } else if (cm == 2) {
             cd = fmod(angle, TWO_PI) / TWO_PI;
           }
           v.color = fmod(abs(cd * D(11) + D(12)), 1.0);
         }",
    },
    VarDef {
        name: "contour_map",
        params: &[ParamDef::range("bands", 5.0, 0.0, 15.0), ParamDef::range("pull", 0.7, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let band_r = java_round(r * d[0]) / d[0];
            let nr = r + d[1] * (band_r - r);
            let scale = if r > 0.001 { nr / r } else { 1.0 };
            v.x += amount * a.x * scale;
            v.y += amount * a.y * scale;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
         let band_r = java_round(r * D(0)) / D(0);
         let nr = r + D(1) * (band_r - r);
         var scale = 1.0;
         if (r > 0.001) { scale = nr / r; }
         v.x += AMOUNT * a.x * scale;
         v.y += AMOUNT * a.y * scale;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "copperplate",
        params: &[
            ParamDef::range("angle", 0.0, -M_PI, M_PI),
            ParamDef::range("freq", 5.0, 0.0, 20.0),
            ParamDef::range("width", 0.3, -2.0, 2.0),
            ParamDef::range("taper", 1.0, 0.0, 5.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [angle, freq, width, taper, cos(angle), sin(angle)]
            let (ca, sa) = (d[4], d[5]);
            let u = a.x * ca + a.y * sa;
            let w = -a.x * sa + a.y * ca;
            let env = (-u.abs() * d[3]).exp();
            let nv = w * (1.0 + d[2] * env * (d[1] * u).sin());
            v.x += amount * (u * ca - nv * sa);
            v.y += amount * (u * sa + nv * ca);
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], p[3], p[0].cos(), p[0].sin()]),
        wgsl: "let ca = D(4);
         let sa = D(5);
         let u = a.x * ca + a.y * sa;
         let w = -a.x * sa + a.y * ca;
         let env = exp(-abs(u) * D(3));
         let nv = w * (1.0 + D(2) * env * sin(D(1) * u));
         v.x += AMOUNT * (u * ca - nv * sa);
         v.y += AMOUNT * (u * sa + nv * ca);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "corners",
        params: &[
            ParamDef::range("x", 1.0, -2.5, 2.5),
            ParamDef::range("y", 1.0, -2.5, 2.5),
            ParamDef::range("mult_x", 1.0, 0.25, 1.5),
            ParamDef::range("mult_y", 1.0, 0.25, 1.5),
            ParamDef::range("x_power", 0.75, 0.25, 1.0),
            ParamDef::range("y_power", 0.75, 0.25, 1.0),
            ParamDef::range("xy_power_add", 0.0, -0.5, 0.5),
            ParamDef::int("log_mode", 0.0, 0.0, 1.0),
            ParamDef::range("log_base", 2.71828, 2.5, 3.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x, y, mult_x, mult_y, x_power, y_power, xy_power_add, log_mode, log_base]
            let xs = a.x * a.x;
            let ys = a.y * a.y;
            let (ex, ey) = if d[7] == 0.0 {
                (xs.powf(d[4] + d[6]) * d[2], ys.powf(d[5] + d[6]) * d[3])
            } else {
                let lb = d[8].ln();
                (
                    ((xs * d[2] + 3.0).ln() / lb).powf(d[4] + 2.25 + d[6]) - 1.33,
                    ((ys * d[3] + 3.0).ln() / lb).powf(d[5] + 2.25 + d[6]) - 1.33,
                )
            };
            if a.x > 0.0 {
                v.x += amount * ex + d[0];
            } else {
                v.x += amount * -ex - d[0];
            }
            if a.y > 0.0 {
                v.y += amount * ey + d[1];
            } else {
                v.y += amount * -ey - d[1];
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let xs = a.x * a.x;
         let ys = a.y * a.y;
         var ex = java_pow(xs, D(4) + D(6)) * D(2);
         var ey = java_pow(ys, D(5) + D(6)) * D(3);
         if (D(7) != 0.0) {
           let lb = log(D(8));
           ex = java_pow(log(xs * D(2) + 3.0) / lb, D(4) + 2.25 + D(6)) - 1.33;
           ey = java_pow(log(ys * D(3) + 3.0) / lb, D(5) + 2.25 + D(6)) - 1.33;
         }
         if (a.x > 0.0) {
           v.x += AMOUNT * ex + D(0);
         } else {
           v.x += AMOUNT * -ex - D(0);
         }
         if (a.y > 0.0) {
           v.y += AMOUNT * ey + D(1);
         } else {
           v.y += AMOUNT * -ey - D(1);
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "corona",
        params: &[ParamDef::range("n", 8.0, 1.0, 50.0), ParamDef::range("amp", 0.5, -3.0, 3.0), ParamDef::range("decay", 1.0, 0.1, 2.1)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [n, amp, decay, max(|decay|, 0.01)]
            let r = (a.x * a.x + a.y * a.y).sqrt();
            let theta = a.y.atan2(a.x);
            let jet = d[1] * (-d[3] * r).exp() * (d[0] * theta * 0.5).cos().abs();
            let new_r = r + jet;
            v.x += amount * new_r * theta.cos();
            v.y += amount * new_r * theta.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], java_max(p[2].abs(), 0.01)]),
        wgsl: "let r = sqrt(a.x * a.x + a.y * a.y);
         let theta = atan2(a.y, a.x);
         let jet = D(1) * exp(-D(3) * r) * abs(cos(D(0) * theta * 0.5));
         let new_r = r + jet;
         v.x += AMOUNT * new_r * cos(theta);
         v.y += AMOUNT * new_r * sin(theta);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cos2_bs",
        params: BS_PARAMS_1,
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x1, x2, y1, y2]
            let cossin = (a.x * d[0]).sin();
            let coscos = (a.x * d[1]).cos();
            let cossinh = (a.y * d[2]).sinh();
            let coscosh = (a.y * d[3]).cosh();
            v.x += amount * coscos * coscosh;
            v.y -= amount * cossin * cossinh;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * cos(a.x * D(1)) * cosh(a.y * D(3));
         v.y -= AMOUNT * sin(a.x * D(0)) * sinh(a.y * D(2));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cosh2_bs",
        params: BS_PARAMS_1,
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x1, x2, y1, y2]
            let coshsin = (a.y * d[2]).sin();
            let coshcos = (a.y * d[3]).cos();
            let coshsinh = (a.x * d[0]).sinh();
            let coshcosh = (a.x * d[1]).cosh();
            v.x += amount * coshcosh * coshcos;
            v.y += amount * coshsinh * coshsin;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * cosh(a.x * D(1)) * cos(a.y * D(3));
         v.y += AMOUNT * sinh(a.x * D(0)) * sin(a.y * D(2));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "coshq",
        true,
        |_, _, a, v, amount| {
            let abs_v = a.y.hypot(a.z);
            let s = abs_v.sin();
            let c = abs_v.cos();
            let sh = a.x.sinh();
            let ch = a.x.cosh();
            let cc = amount * sh * s / abs_v;
            v.x += amount * ch * c;
            v.y += cc * a.y;
            v.z += cc * a.z;
        },
        "let abs_v = length(vec2<f32>(a.y, a.z));
         let cc = AMOUNT * sinh(a.x) * sin(abs_v) / abs_v;
         v.x += AMOUNT * cosh(a.x) * cos(abs_v);
         v.y += cc * a.y;
         v.z += cc * a.z;",
    ),
    simple(
        "cosq",
        true,
        |_, _, a, v, amount| {
            let abs_v = a.y.hypot(a.z);
            let s = a.x.sin();
            let c = a.x.cos();
            let sh = abs_v.sinh();
            let ch = abs_v.cosh();
            let cc = -amount * s * sh / abs_v;
            v.x += amount * c * ch;
            v.y += cc * a.y;
            v.z += cc * a.z;
        },
        "let abs_v = length(vec2<f32>(a.y, a.z));
         let cc = -AMOUNT * sin(a.x) * sinh(abs_v) / abs_v;
         v.x += AMOUNT * cos(a.x) * cosh(abs_v);
         v.y += cc * a.y;
         v.z += cc * a.z;",
    ),
    VarDef {
        name: "cot2_bs",
        params: BS_PARAMS_2,
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x1, x2, y1, y2]
            let cotsin = (d[0] * a.x).sin();
            let cotcos = (d[1] * a.x).cos();
            let cotsinh = (d[2] * a.y).sinh();
            let cotcosh = (d[3] * a.y).cosh();
            let cotden = 1.0 / (cotcosh - cotcos);
            v.x += amount * cotden * cotsin;
            v.y += amount * cotden * -1.0 * cotsinh;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let cotden = 1.0 / (cosh(D(3) * a.y) - cos(D(1) * a.x));
         v.x += AMOUNT * cotden * sin(D(0) * a.x);
         v.y += AMOUNT * cotden * -1.0 * sinh(D(2) * a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "coth2_bs",
        params: BS_PARAMS_2,
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x1, x2, y1, y2]
            let cothsin = (d[2] * a.y).sin();
            let cothcos = (d[3] * a.y).cos();
            let cothsinh = (d[0] * a.x).sinh();
            let cothcosh = (d[1] * a.x).cosh();
            let dd = cothcosh - cothcos;
            if dd == 0.0 {
                return;
            }
            let cothden = 1.0 / dd;
            v.x += amount * cothden * cothsinh;
            v.y += amount * cothden * cothsin;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dd = cosh(D(1) * a.x) - cos(D(3) * a.y);
         if (dd == 0.0) { return PtPair(a, v); }
         let cothden = 1.0 / dd;
         v.x += AMOUNT * cothden * sinh(D(0) * a.x);
         v.y += AMOUNT * cothden * sin(D(2) * a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "cothq",
        true,
        |_, _, a, v, amount| {
            let abs_v = a.y.hypot(a.z);
            let s = abs_v.sin();
            let c = abs_v.cos();
            let sh = a.x.sinh();
            let ch = a.x.cosh();
            let sysz = sqr(a.y) + sqr(a.z);
            let ni = amount / (sqr(a.x) + sysz);
            let cc = ch * s / abs_v;
            let b = sh * s / abs_v;
            let stcv = sh * c;
            let nstcv = -stcv;
            let ctcv = ch * c;
            v.x += (stcv * ctcv + cc * b * sysz) * ni;
            v.y += (nstcv * b * a.y + cc * a.y * ctcv) * ni;
            v.z += (nstcv * b * a.z + cc * a.z * ctcv) * ni;
        },
        "let abs_v = length(vec2<f32>(a.y, a.z));
         let s = sin(abs_v);
         let c = cos(abs_v);
         let sh = sinh(a.x);
         let ch = cosh(a.x);
         let sysz = a.y * a.y + a.z * a.z;
         let ni = AMOUNT / (a.x * a.x + sysz);
         let cc = ch * s / abs_v;
         let b = sh * s / abs_v;
         let stcv = sh * c;
         let nstcv = -stcv;
         let ctcv = ch * c;
         v.x += (stcv * ctcv + cc * b * sysz) * ni;
         v.y += (nstcv * b * a.y + cc * a.y * ctcv) * ni;
         v.z += (nstcv * b * a.z + cc * a.z * ctcv) * ni;",
    ),
    simple(
        "cotq",
        true,
        |_, _, a, v, amount| {
            let abs_v = a.y.hypot(a.z);
            let s = a.x.sin();
            let c = a.x.cos();
            let sh = abs_v.sinh();
            let ch = abs_v.cosh();
            let sysz = sqr(a.y) + sqr(a.z);
            let ni = amount / (sqr(a.x) + sysz);
            let cc = c * sh / abs_v;
            let b = -s * sh / abs_v;
            let stcv = s * ch;
            let nstcv = -stcv;
            let ctcv = c * ch;
            v.x += (stcv * ctcv + cc * b * sysz) * ni;
            v.y -= (nstcv * b * a.y + cc * a.y * ctcv) * ni;
            v.z -= (nstcv * b * a.z + cc * a.z * ctcv) * ni;
        },
        "let abs_v = length(vec2<f32>(a.y, a.z));
         let s = sin(a.x);
         let c = cos(a.x);
         let sh = sinh(abs_v);
         let ch = cosh(abs_v);
         let sysz = a.y * a.y + a.z * a.z;
         let ni = AMOUNT / (a.x * a.x + sysz);
         let cc = c * sh / abs_v;
         let b = -s * sh / abs_v;
         let stcv = s * ch;
         let nstcv = -stcv;
         let ctcv = c * ch;
         v.x += (stcv * ctcv + cc * b * sysz) * ni;
         v.y -= (nstcv * b * a.y + cc * a.y * ctcv) * ni;
         v.z -= (nstcv * b * a.z + cc * a.z * ctcv) * ni;",
    ),
    VarDef {
        name: "cpow2",
        params: &[
            ParamDef::new("r", 1.0),
            ParamDef::new("a", 0.0),
            ParamDef::range("divisor", 1.0, -10.0, 10.0),
            ParamDef::int("range", 1.0, 1.0, 10.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [r, a, divisor, range, ang, c, d, half_c, half_d, inv_range, full_range]
            let mut ang = a.atan_yx();
            let mut n = ctx.random_int(d[3] as usize) as i32;
            if ang < 0.0 {
                n += 1;
            }
            ang += 2.0 * M_PI * n as f64;
            if (ang * d[9]).cos() < ctx.random() * 2.0 - 1.0 {
                ang -= d[10];
            }
            let lnr2 = a.sumsq().ln();
            let r = amount * (d[7] * lnr2 - d[6] * ang).exp();
            let th = d[5] * ang + d[8] * lnr2 + d[4] * (d[2] * ctx.random()).floor();
            v.x += r * th.cos();
            v.y += r * th.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let divisor = if p[2] == 0.0 { 1.0 } else { p[2] };
            let range = if p[3] < 1.0 { 1.0 } else { ftoi(p[3]) as f64 };
            let [ang, c, d, half_c, half_d] = cpow2_coefs(p[0], p[1], divisor);
            vec![p[0], p[1], divisor, range, ang, c, d, half_c, half_d, 0.5 / range, 2.0 * M_PI * range]
        }),
        wgsl: "var ang = atan_yx(a);
         var n = i32(rnd() * D(3));
         if (ang < 0.0) { n += 1; }
         ang += TWO_PI * f32(n);
         if (cos(ang * D(9)) < rnd() * 2.0 - 1.0) { ang -= D(10); }
         let lnr2 = log(sumsq(a));
         let r = AMOUNT * exp(D(7) * lnr2 - D(6) * ang);
         let th = D(5) * ang + D(8) * lnr2 + D(4) * floor(D(2) * rnd());
         v.x += r * cos(th);
         v.y += r * sin(th);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cpow3",
        params: &[
            ParamDef::new("r", 1.0),
            ParamDef::new("d", 1.0),
            ParamDef::range("divisor", 1.0, -10.0, 10.0),
            ParamDef::new("spread", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [r, d, divisor, spread, ang, tc, td, half_c, half_d, coeff]
            let mut ang = a.atan_yx();
            if ang < 0.0 {
                ang += 2.0 * M_PI;
            }
            if (ang / 2.0).cos() < ctx.random() * 2.0 - 1.0 {
                ang -= 2.0 * M_PI;
            }
            let turn = if ctx.random() < 0.5 { 2.0 * M_PI } else { -2.0 * M_PI };
            ang += turn * java_round(ctx.random().ln() * d[9]);
            let lnr2 = a.sumsq().ln();
            let r = amount * (d[7] * lnr2 - d[6] * ang).exp();
            let th = d[5] * ang + d[8] * lnr2 + d[4] * (d[2] * ctx.random()).floor();
            v.x += r * th.cos();
            v.y += r * th.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let (r, dd, spread) = (p[0], p[1], p[3]);
            let divisor = if p[2] == 0.0 { 1.0 } else { p[2] };
            let ang = 2.0 * M_PI / divisor;
            let p_a = ((if dd < 0.0 { -(-dd).ln() } else { dd.ln() }) * r).atan2(2.0 * M_PI);
            let tc = p_a.cos() * r * p_a.cos() / divisor;
            let td = p_a.cos() * r * p_a.sin() / divisor;
            let coeff = if td == 0.0 { 0.0 } else { -0.095 * spread / td };
            vec![r, dd, divisor, spread, ang, tc, td, tc / 2.0, td / 2.0, coeff]
        }),
        // log(0) guarded on the GPU (Java: round(-inf * coeff)).
        wgsl: "var ang = atan_yx(a);
         if (ang < 0.0) { ang += TWO_PI; }
         if (cos(ang / 2.0) < rnd() * 2.0 - 1.0) { ang -= TWO_PI; }
         var turn = -TWO_PI;
         if (rnd() < 0.5) { turn = TWO_PI; }
         ang += turn * java_round(log(max(rnd(), 1.0e-30)) * D(9));
         let lnr2 = log(sumsq(a));
         let r = AMOUNT * exp(D(7) * lnr2 - D(6) * ang);
         let th = D(5) * ang + D(8) * lnr2 + D(4) * floor(D(2) * rnd());
         v.x += r * cos(th);
         v.y += r * sin(th);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "cpow3_wf",
        params: &[
            ParamDef::new("r", 1.0),
            ParamDef::new("a", 0.1),
            ParamDef::range("divisor", 1.0, -10.0, 10.0),
            ParamDef::new("spread", 1.0),
            ParamDef::range("discrete_spread", 1.0, 0.0, 1.0),
            ParamDef::new("spread2", 0.0),
            ParamDef::new("offset2", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [r, a, divisor, spread, discrete_spread, spread2, offset2,
            //      ang, c, d, half_c, half_d, inv_spread, full_spread]
            let mut ai = a.atan_yx();
            let mut n = ctx.random() * d[3];
            if d[4] >= 1.0 {
                n = (n as i32) as f64;
            }
            if ai < 0.0 {
                n += 1.0;
            }
            ai += 2.0 * M_PI * n;
            if (ai * d[12]).cos() < ctx.random() * 2.0 - 1.0 {
                ai -= d[13];
            }
            let lnr2 = a.sumsq().ln();
            let ri = amount * (d[10] * lnr2 - d[9] * ai).exp();
            // Java quirk: the terms are multiplied, not added.
            let ang2 = d[8] * ai * d[11] * lnr2 * d[7] * (ctx.random() * d[5] + d[6]);
            v.x += ri * ang2.cos();
            v.y += ri * ang2.sin();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let [ang, c, d, half_c, half_d] = cpow2_coefs(p[0], p[1], p[2]);
            let mut out = p.to_vec();
            out.extend_from_slice(&[ang, c, d, half_c, half_d, 0.5 / p[3], 2.0 * M_PI * p[3]]);
            out
        }),
        wgsl: "var ai = atan_yx(a);
         var n = rnd() * D(3);
         if (D(4) >= 1.0) { n = trunc(n); }
         if (ai < 0.0) { n += 1.0; }
         ai += TWO_PI * n;
         if (cos(ai * D(12)) < rnd() * 2.0 - 1.0) { ai -= D(13); }
         let lnr2 = log(sumsq(a));
         let ri = AMOUNT * exp(D(10) * lnr2 - D(9) * ai);
         let ang2 = D(8) * ai * D(11) * lnr2 * D(7) * (rnd() * D(5) + D(6));
         v.x += ri * cos(ang2);
         v.y += ri * sin(ang2);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "crob",
        params: &[
            ParamDef::new("top", -1.0),
            ParamDef::new("bottom", 1.0),
            ParamDef::new("left", -1.0),
            ParamDef::new("right", 1.0),
            ParamDef::int("blur", 1.0, 0.0, 1.0),
            ParamDef::range("ratioBlur", 0.05, 0.0, 1.0),
            ParamDef::range("directBlur", 2.0, 0.0, 10.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [params(7), top 7, bottom 8, left 9, right 10, directblur 11, blur 12, xInt_2 13,
            //      yInt_2 14, minInt_2 15, x0c 16, y0c 17, setProb 18, setProbQ 19, setProbH 20,
            //      setProbTQ 21, setCompProbQ 22, setCompProbH 23, setCompProbTQ 24,
            //      top_border 25, bottom_border 26, left_border 27, right_border 28]
            let (top, bottom, left, right) = (d[7], d[8], d[9], d[10]);
            let (ratio, directblur) = (d[5], d[11]);
            let (x0c, y0c) = (d[16], d[17]);
            let mut xt = a.x;
            let mut yt = a.y;
            if a.x < d[27] || a.x > d[28] || a.y < d[25] || a.y > d[26] {
                if d[12] == 0.0 {
                    xt = 0.0;
                    yt = 0.0;
                } else {
                    let sec = ctx.random();
                    if sec < d[18] {
                        for _ in 0..CROB_MAX_TRIES {
                            yt = top + ctx.random() * d[14];
                            xt = right - ctx.random().powf(directblur) * ratio * d[15];
                            let grad = (yt - y0c) / (xt - x0c);
                            if !(grad < -1.0) {
                                break;
                            }
                        }
                        if sec < d[20] {
                            xt = left + right - xt;
                        }
                        if sec > d[19] && sec < d[21] {
                            yt = bottom + top - yt;
                        }
                    } else {
                        for _ in 0..CROB_MAX_TRIES {
                            xt = right - ctx.random() * d[13];
                            yt = top + ctx.random().powf(directblur) * ratio * d[15];
                            let grad = (yt - y0c) / (xt - x0c);
                            if !(grad <= 0.0 && grad > -1.0) {
                                break;
                            }
                        }
                        if sec > d[23] {
                            yt = bottom + top - yt;
                        }
                        if sec > d[22] && sec < d[24] {
                            xt = left + right - xt;
                        }
                    }
                }
            }
            v.x += xt;
            v.y += yt;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let (mut top, mut bottom, mut left, mut right) = (p[0], p[1], p[2], p[3]);
            let ratioblur = p[5];
            if top > bottom {
                std::mem::swap(&mut top, &mut bottom);
            }
            if top == bottom {
                top = -1.0;
                bottom = 1.0;
            }
            if left > right {
                std::mem::swap(&mut left, &mut right);
            }
            if left == right {
                left = -1.0;
                right = 1.0;
            }
            let directblur = if p[6] < 0.0 { 0.0 } else { p[6] };
            let blur = if ftoi(p[4]) != 0 { 1.0 } else { 0.0 };
            let x_interval = (right - left).abs();
            let y_interval = (bottom - top).abs();
            let x_int_2 = x_interval / 2.0;
            let y_int_2 = y_interval / 2.0;
            let min_int_2 = if x_int_2 > y_int_2 { y_int_2 } else { x_int_2 };
            let x0 = right - x_int_2;
            let y0 = top + y_int_2;
            let (x0c, y0c) = if x_int_2 > y_int_2 {
                (right - min_int_2, y0)
            } else if x_int_2 < y_int_2 {
                (x0, top + min_int_2)
            } else {
                (x0, y0)
            };
            let set_prob = y_interval / (x_interval + y_interval);
            let set_comp_prob = 1.0 - set_prob;
            let (tb, bb, lb, rb) = if blur == 0.0 {
                (top, bottom, left, right)
            } else {
                (
                    top + min_int_2 * ratioblur,
                    bottom - min_int_2 * ratioblur,
                    left + min_int_2 * ratioblur,
                    right - min_int_2 * ratioblur,
                )
            };
            let mut d = p.to_vec();
            d.extend_from_slice(&[
                top,
                bottom,
                left,
                right,
                directblur,
                blur,
                x_int_2,
                y_int_2,
                min_int_2,
                x0c,
                y0c,
                set_prob,
                0.25 * set_prob,
                0.50 * set_prob,
                0.75 * set_prob,
                set_prob + 0.25 * set_comp_prob,
                set_prob + 0.50 * set_comp_prob,
                set_prob + 0.75 * set_comp_prob,
                tb,
                bb,
                lb,
                rb,
            ]);
            d
        }),
        wgsl: "let top = D(7);
         let bottom = D(8);
         let left = D(9);
         let right = D(10);
         let ratio = D(5);
         let directblur = D(11);
         let x0c = D(16);
         let y0c = D(17);
         var xt = a.x;
         var yt = a.y;
         if (a.x < D(27) || a.x > D(28) || a.y < D(25) || a.y > D(26)) {
           if (D(12) == 0.0) {
             xt = 0.0;
             yt = 0.0;
           } else {
             let sec = rnd();
             if (sec < D(18)) {
               for (var i = 0; i < 64; i++) {
                 yt = top + rnd() * D(14);
                 xt = right - java_pow(rnd(), directblur) * ratio * D(15);
                 let grad = (yt - y0c) / (xt - x0c);
                 if (!(grad < -1.0)) { break; }
               }
               if (sec < D(20)) { xt = left + right - xt; }
               if (sec > D(19) && sec < D(21)) { yt = bottom + top - yt; }
             } else {
               for (var i = 0; i < 64; i++) {
                 xt = right - rnd() * D(13);
                 yt = top + java_pow(rnd(), directblur) * ratio * D(15);
                 let grad = (yt - y0c) / (xt - x0c);
                 if (!(grad <= 0.0 && grad > -1.0)) { break; }
               }
               if (sec > D(23)) { yt = bottom + top - yt; }
               if (sec > D(22) && sec < D(24)) { xt = left + right - xt; }
             }
           }
         }
         v.x += xt;
         v.y += yt;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "crop3D",
        params: &[
            ParamDef::new("left", -1.0),
            ParamDef::new("right", 1.0),
            ParamDef::new("top", -1.0),
            ParamDef::new("bottom", 1.0),
            ParamDef::new("floor", -1.0),
            ParamDef::new("ceiling", 1.0),
            ParamDef::range("scatter_area", 0.0, -1.0, 1.0),
            ParamDef::int("zero", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [left, right, top, bottom, floor, ceiling, scatter_area, zero,
            //      xmin 8, xmax 9, ymin 10, ymax 11, zmin 12, zmax 13, w 14, h 15, l 16]
            let (mut x, mut y, mut z) = (a.x, a.y, a.z);
            if (x < d[8] || x > d[9] || y < d[10] || y > d[11] || z < d[12] || z > d[13]) && d[7] != 0.0 {
                v.x = 0.0;
                v.y = 0.0;
                v.z = 0.0;
                v.do_hide = true;
                return;
            }
            v.do_hide = false;
            if x < d[8] {
                x = d[8] + ctx.random() * d[14];
            } else if x > d[9] {
                x = d[9] - ctx.random() * d[14];
            }
            if y < d[10] {
                y = d[10] + ctx.random() * d[15];
            } else if y > d[11] {
                y = d[11] - ctx.random() * d[15];
            }
            if z < d[12] {
                z = d[12] + ctx.random() * d[16];
            } else if z > d[13] {
                z = d[13] - ctx.random() * d[16];
            }
            v.x = amount * x;
            v.y = amount * y;
            v.z = amount * z;
        },
        prepare: Some(|p, _| {
            let scatter = limit_value(p[6], -1.0, 1.0);
            let zero = ftoi_limited(p[7], 0, 1);
            let (xmin, xmax) = (java_min(p[0], p[1]), java_max(p[0], p[1]));
            let (ymin, ymax) = (java_min(p[2], p[3]), java_max(p[2], p[3]));
            let (zmin, zmax) = (java_min(p[4], p[5]), java_max(p[4], p[5]));
            vec![
                p[0],
                p[1],
                p[2],
                p[3],
                p[4],
                p[5],
                scatter,
                zero,
                xmin,
                xmax,
                ymin,
                ymax,
                zmin,
                zmax,
                (xmax - xmin) * 0.5 * scatter,
                (ymax - ymin) * 0.5 * scatter,
                (zmax - zmin) * 0.5 * scatter,
            ]
        }),
        wgsl: CROP3D_WGSL,
    },
    VarDef {
        name: "crop_box",
        params: &[ParamDef::new("width", 0.5), ParamDef::new("height", 0.5), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [width, height, invert]
            let sd = crop_box_sd(vec2(a.x, a.y), vec2(d[0], d[1]));
            crop_sdf_output(ctx, a, v, amount, sd, d[2]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: CROP_BOX_WGSL,
    },
    VarDef {
        name: "crop",
        params: &[
            ParamDef::new("left", -1.0),
            ParamDef::new("right", 1.0),
            ParamDef::new("top", -1.0),
            ParamDef::new("bottom", 1.0),
            ParamDef::range("scatter_area", 0.0, -1.0, 1.0),
            ParamDef::int("zero", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [left, right, top, bottom, scatter_area, zero, xmin 6, xmax 7, ymin 8, ymax 9, w 10, h 11]
            let (mut x, mut y) = (a.x, a.y);
            if (x < d[6] || x > d[7] || y < d[8] || y > d[9]) && d[5] != 0.0 {
                v.x = 0.0;
                v.y = 0.0;
                v.do_hide = true;
                return;
            }
            v.do_hide = false;
            if x < d[6] {
                x = d[6] + ctx.random() * d[10];
            } else if x > d[7] {
                x = d[7] - ctx.random() * d[10];
            }
            if y < d[8] {
                y = d[8] + ctx.random() * d[11];
            } else if y > d[9] {
                y = d[9] - ctx.random() * d[11];
            }
            v.x = amount * x;
            v.y = amount * y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let scatter = limit_value(p[4], -1.0, 1.0);
            let zero = ftoi_limited(p[5], 0, 1);
            let (xmin, xmax) = (java_min(p[0], p[1]), java_max(p[0], p[1]));
            let (ymin, ymax) = (java_min(p[2], p[3]), java_max(p[2], p[3]));
            vec![
                p[0],
                p[1],
                p[2],
                p[3],
                scatter,
                zero,
                xmin,
                xmax,
                ymin,
                ymax,
                (xmax - xmin) * 0.5 * scatter,
                (ymax - ymin) * 0.5 * scatter,
            ]
        }),
        wgsl: CROP_WGSL,
    },
    VarDef {
        name: "crop_cross",
        params: &[
            ParamDef::new("width", 0.5),
            ParamDef::new("Size", 0.5),
            ParamDef::new("round", 0.001),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [width, Size, round, invert]
            let mut si = vec2(d[0], d[1]);
            if si.x < si.y {
                si = vec2(si.y, si.x);
            }
            let ra = 0.1 * (d[2] * 1.2).sin();
            let sd = crop_cross_sd(vec2(a.x, a.y), si, ra);
            crop_sdf_output(ctx, a, v, amount, sd, d[3]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], crop_int_param(p[3], 0.0, 1.0)]),
        wgsl: CROP_CROSS_WGSL,
    },
    VarDef {
        name: "crop_polygon",
        params: &[ParamDef::int("type", 4.0, 0.0, 4.0), ParamDef::new("radius", 0.35), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [type, radius, invert]
            let p = vec2(a.x, a.y);
            let sd = match d[0] as i32 {
                4 => crop_polygon_hexagram(p, d[1]),
                3 => crop_polygon_octogon(p, d[1]),
                2 => crop_polygon_hexagon(p, d[1]),
                1 => crop_polygon_pentagon(p, d[1]),
                0 => length(p) - d[1],
                _ => 0.0,
            };
            crop_sdf_output(ctx, a, v, amount, sd, d[2]);
        },
        prepare: Some(|p, _| vec![crop_int_param(p[0], 0.0, 4.0), p[1], crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: CROP_POLYGON_WGSL,
    },
    VarDef {
        name: "crop_rhombus",
        params: &[ParamDef::new("width", 0.5), ParamDef::new("height", 0.5), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [width, height, invert]
            let sd = crop_rhombus_sd(vec2(a.x, a.y), vec2(d[0], d[1]));
            crop_sdf_output(ctx, a, v, amount, sd, d[2]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: CROP_RHOMBUS_WGSL,
    },
    VarDef {
        name: "crop_stars",
        params: &[
            ParamDef::new("radius", 0.35),
            ParamDef::int("n", 3.0, 3.0, 9.0),
            ParamDef::range("r2", 0.333, 0.0, 1.0),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [radius, n, r2, invert]
            let n = d[1] as i32;
            let m = 4.0 + d[2] * d[2] * (n as f64 * 2.0 - 4.0);
            let sd = crop_stars_sd(vec2(a.x, a.y), d[0], n, m);
            crop_sdf_output(ctx, a, v, amount, sd, d[3]);
        },
        prepare: Some(|p, _| vec![p[0], crop_int_param(p[1], 3.0, 9.0), limit_value(p[2], 0.0, 1.0), crop_int_param(p[3], 0.0, 1.0)]),
        wgsl: CROP_STARS_WGSL,
    },
    VarDef {
        name: "crop_trapezoid",
        params: &[
            ParamDef::new("Base Sup.", 0.5),
            ParamDef::new("Base Inf.", 0.2),
            ParamDef::new("height", 0.5),
            ParamDef::int("invert", 0.0, 0.0, 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [Base Sup., Base Inf., height, invert]
            let sd = crop_trapezoid_sd(vec2(a.x, a.y), d[0], d[1], d[2]);
            crop_sdf_output(ctx, a, v, amount, sd, d[3]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], crop_int_param(p[3], 0.0, 1.0)]),
        wgsl: CROP_TRAPEZOID_WGSL,
    },
    VarDef {
        name: "crop_triangle",
        params: &[
            ParamDef::new("x1", 0.0),
            ParamDef::new("y1", 0.5),
            ParamDef::new("x2", 0.5),
            ParamDef::new("y2", 0.5),
            ParamDef::new("x3", 0.0),
            ParamDef::new("y3", -0.5),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, _ctx, a, v, amount| {
            // d = [x1, y1, x2, y2, x3, y3]
            let (x1, y1, x2, y2, x3, y3) = (d[0], d[1], d[2], d[3], d[4], d[5]);
            let (x, y) = (a.x, a.y);
            let area = crop_triangle_area(x1, y1, x2, y2, x3, y3);
            let a1 = crop_triangle_area(x, y, x2, y2, x3, y3);
            let a2 = crop_triangle_area(x1, y1, x, y, x3, y3);
            let a3 = crop_triangle_area(x1, y1, x2, y2, x, y);
            // Java quirk: exact floating point comparison.
            v.do_hide = area != a1 + a2 + a3;
            v.x += amount * x;
            v.y += amount * y;
        },
        prepare: None,
        wgsl: CROP_TRIANGLE_WGSL,
    },
    VarDef {
        name: "crop_vesica",
        params: &[ParamDef::new("width", 0.2), ParamDef::new("height", 0.1), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [width, height, invert]
            let sd = crop_vesica_sd(vec2(a.x, a.y), 1.0 + d[1], 0.8 + d[0]);
            crop_sdf_output(ctx, a, v, amount, sd, d[2]);
        },
        prepare: Some(|p, _| vec![p[0], p[1], crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: CROP_VESICA_WGSL,
    },
    VarDef {
        name: "crop_x",
        params: &[ParamDef::new("radius", 0.1), ParamDef::range("w", 0.5, 0.0, 1.0), ParamDef::int("invert", 0.0, 0.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [radius, w, invert]
            let sd = crop_x_sd(vec2(a.x, a.y), d[1], d[0]);
            // Unlike the other crop_* variations, hidden points keep their position.
            v.do_hide = if d[2] == 0.0 { sd > 0.0 } else { sd <= 0.0 };
            v.x = amount * a.x;
            v.y = amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], limit_value(p[1], 0.0, 1.0), crop_int_param(p[2], 0.0, 1.0)]),
        wgsl: CROP_X_WGSL,
    },
    VarDef {
        name: "crown_js",
        params: &[ParamDef::range("a", 5.0, 1.25, 6.25), ParamDef::range("b", 0.630_929_753_571_457_4, 0.25, 1.0)],
        kind: VarKind::Normal,
        is_3d: true,
        apply: |d, ctx, _a, v, amount| {
            // d = [a, b]
            let t = -M_PI + 2.0 * M_PI * ctx.random();
            let mut wt = Complex::new(0.0, 0.0);
            for k in 1..15 {
                let denom = d[0].powf(d[1] * k as f64);
                let sign = if k % 2 == 0 { -1.0 } else { 1.0 };
                let th = d[0].powf(k as f64) * t * sign;
                wt.add(&Complex::new(th.sin() / denom, th.cos() / denom));
            }
            let z = wt.mag2().powf(2.0);
            v.x += wt.re * amount;
            v.y += wt.im * amount;
            v.z += z * amount;
            v.color = fmod((sqr(v.x) + sqr(v.y)).abs(), 1.0);
        },
        prepare: None,
        // f32 loses the phase of sin(a^k * t) for large k (as in the Java CUDA code).
        wgsl: "let t = -PI + TWO_PI * rnd();
         var wt = vec2<f32>(0.0, 0.0);
         for (var k = 1; k < 15; k++) {
           let denom = java_pow(D(0), D(1) * f32(k));
           var sgn_k = 1.0;
           if (k % 2 == 0) { sgn_k = -1.0; }
           let th = java_pow(D(0), f32(k)) * t * sgn_k;
           wt += vec2<f32>(sin(th) / denom, cos(th) / denom);
         }
         let m2 = wt.x * wt.x + wt.y * wt.y;
         v.x += wt.x * AMOUNT;
         v.y += wt.y * AMOUNT;
         v.z += m2 * m2 * AMOUNT;
         v.color = fmod(abs(v.x * v.x + v.y * v.y), 1.0);",
    },
    VarDef {
        name: "csc2_bs",
        params: BS_PARAMS_1,
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x1, x2, y1, y2]
            let cscsin = (a.x * d[0]).sin();
            let csccos = (a.x * d[1]).cos();
            let cscsinh = (a.y * d[2]).sinh();
            let csccosh = (a.y * d[3]).cosh();
            let dd = (2.0 * a.y).cosh() - (2.0 * a.x).cos();
            if dd == 0.0 {
                return;
            }
            let cscden = 2.0 / dd;
            v.x += amount * cscden * cscsin * csccosh;
            v.y -= amount * cscden * csccos * cscsinh;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dd = cosh(2.0 * a.y) - cos(2.0 * a.x);
         if (dd == 0.0) { return PtPair(a, v); }
         let cscden = 2.0 / dd;
         v.x += AMOUNT * cscden * sin(a.x * D(0)) * cosh(a.y * D(3));
         v.y -= AMOUNT * cscden * cos(a.x * D(1)) * sinh(a.y * D(2));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "csch2_bs",
        params: BS_PARAMS_1,
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x1, x2, y1, y2]
            let cschsin = (a.y * d[2]).sin();
            let cschcos = (a.y * d[3]).cos();
            let cschsinh = (a.x * d[0]).sinh();
            let cschcosh = (a.x * d[1]).cosh();
            let dd = (2.0 * a.x).cosh() - (2.0 * a.y).cos();
            if dd == 0.0 {
                return;
            }
            let cschden = 2.0 / dd;
            v.x += amount * cschden * cschsinh * cschcos;
            v.y -= amount * cschden * cschcosh * cschsin;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let dd = cosh(2.0 * a.x) - cos(2.0 * a.y);
         if (dd == 0.0) { return PtPair(a, v); }
         let cschden = 2.0 / dd;
         v.x += AMOUNT * cschden * sinh(a.x * D(0)) * cos(a.y * D(3));
         v.y -= AMOUNT * cschden * cosh(a.x * D(1)) * sin(a.y * D(2));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "cschq",
        true,
        |_, _, a, v, amount| {
            let abs_v = a.y.hypot(a.z);
            let s = abs_v.sin();
            let c = abs_v.cos();
            let sh = a.x.sinh();
            let ch = a.x.cosh();
            let ni = amount / (sqr(a.x) + sqr(a.y) + sqr(a.z));
            let cc = ni * ch * s / abs_v;
            v.x += sh * c * ni;
            v.y -= cc * a.y;
            v.z -= cc * a.z;
        },
        "let abs_v = length(vec2<f32>(a.y, a.z));
         let ni = AMOUNT / (a.x * a.x + a.y * a.y + a.z * a.z);
         let cc = ni * cosh(a.x) * sin(abs_v) / abs_v;
         v.x += sinh(a.x) * cos(abs_v) * ni;
         v.y -= cc * a.y;
         v.z -= cc * a.z;",
    ),
    simple(
        "cscq",
        true,
        |_, _, a, v, amount| {
            let abs_v = a.y.hypot(a.z);
            let s = a.x.sin();
            let c = a.x.cos();
            let sh = abs_v.sinh();
            let ch = abs_v.cosh();
            let ni = amount / (sqr(a.x) + sqr(a.y) + sqr(a.z));
            let cc = ni * c * sh / abs_v;
            v.x += s * ch * ni;
            v.y -= cc * a.y;
            v.z -= cc * a.z;
        },
        "let abs_v = length(vec2<f32>(a.y, a.z));
         let ni = AMOUNT / (a.x * a.x + a.y * a.y + a.z * a.z);
         let cc = ni * cos(a.x) * sinh(abs_v) / abs_v;
         v.x += sin(a.x) * cosh(abs_v) * ni;
         v.y -= cc * a.y;
         v.z -= cc * a.z;",
    ),
    VarDef {
        name: "csc_squared",
        params: &[
            ParamDef::new("csc_div", 1.0),
            ParamDef::new("cos_div", 1.0),
            ParamDef::new("tan_div", 1.0),
            ParamDef::new("csc_pow", 0.5),
            ParamDef::new("pi_mult", 0.5),
            ParamDef::new("csc_add", 0.25),
            ParamDef::new("scale_y", 1.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [csc_div, cos_div, tan_div, csc_pow, pi_mult, csc_add, scale_y]
            let (x, y) = (a.x, a.y);
            let csc = d[0] / (x / d[1]).cos() / (x / d[2]).tan();
            let fx = (csc * csc + M_PI * d[4]).powf(d[3]) + d[5];
            v.x += amount * x * fx;
            v.y += amount * y * fx * d[6];
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let csc = D(0) / cos(a.x / D(1)) / tan(a.x / D(2));
         let fx = java_pow(csc * csc + PI * D(4), D(3)) + D(5);
         v.x += AMOUNT * a.x * fx;
         v.y += AMOUNT * a.y * fx * D(6);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "csin",
        params: &[ParamDef::new("stretch", 1.5)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [stretch]
            let mut c = Complex::new(a.x * d[0], a.y * d[0]);
            c.sin();
            v.x += amount * c.re;
            v.y += amount * c.im;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let c = cx_sin(vec2<f32>(a.x * D(0), a.y * D(0)));
         v.x += AMOUNT * c.x;
         v.y += AMOUNT * c.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "c_symmetry",
        params: &[ParamDef::range("p1", 3.0, -10.0, 10.0), ParamDef::range("p2", 3.0, -10.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [p1, p2]
            let t = d[0] * a.y.atan2(a.x);
            let m = (a.x * a.x + a.y * a.y).sqrt().powf(d[1]);
            v.x = amount * (t.cos() * m);
            v.y = amount * (t.sin() * m);
            if ctx.preserve_z {
                v.z = amount * a.z;
            }
        },
        prepare: None,
        wgsl: "let t = D(0) * atan2(a.y, a.x);
         let m = java_pow(sqrt(a.x * a.x + a.y * a.y), D(1));
         v.x = AMOUNT * (cos(t) * m);
         v.y = AMOUNT * (sin(t) * m);
         if (PRESERVE_Z) { v.z = AMOUNT * a.z; }",
    },
    VarDef {
        name: "cumulonimbus",
        params: &[ParamDef::new("height", 1.0), ParamDef::new("spread", 0.5), ParamDef::new("turb", 0.15)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [height, spread, turb, max(|height|, 0.1)]
            let t = java_max(-1.5, java_min(1.5, a.y / d[3]));
            let anvil = d[1] * t * t.abs();
            let shake = d[2] * (a.y * 4.1 + a.x.abs() * 2.3).sin();
            v.x += amount * (a.x + anvil + shake * (1.0 - t.abs() * 0.5));
            v.y += amount * (a.y * (1.0 + 0.5 * t.abs()));
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], p[2], java_max(p[0].abs(), 0.1)]),
        wgsl: "let t = clamp(a.y / D(3), -1.5, 1.5);
         let anvil = D(1) * t * abs(t);
         let shake = D(2) * sin(a.y * 4.1 + abs(a.x) * 2.3);
         v.x += AMOUNT * (a.x + anvil + shake * (1.0 - abs(t) * 0.5));
         v.y += AMOUNT * (a.y * (1.0 + 0.5 * abs(t)));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "combimirror",
        "complex",
        "cone3D",
        "cone",
        "conicalSpiral",
        "contour_map",
        "copperplate",
        "corners",
        "corona",
        "cos2_bs",
        "cosh2_bs",
        "coshq",
        "cosq",
        "cot2_bs",
        "coth2_bs",
        "cothq",
        "cotq",
        "cpow2",
        "cpow3",
        "cpow3_wf",
        "crob",
        "crop3D",
        "crop_box",
        "crop",
        "crop_cross",
        "crop_polygon",
        "crop_rhombus",
        "crop_stars",
        "crop_trapezoid",
        "crop_triangle",
        "crop_vesica",
        "crop_x",
        "crown_js",
        "csc2_bs",
        "csch2_bs",
        "cschq",
        "cscq",
        "csc_squared",
        "csin",
        "c_symmetry",
        "cumulonimbus",
    ];

    fn def(name: &str) -> &'static VarDef {
        DEFS.iter().find(|d| d.name == name).unwrap_or_else(|| panic!("missing {name}"))
    }

    fn run(name: &str, params: &[f64], x: f64, y: f64, z: f64) -> Pt {
        let def = def(name);
        let data = def.prepare_data(params, &Affine::IDENTITY);
        let mut ctx = Ctx::new(7, true);
        let mut a = Pt { x, y, z, ..Default::default() };
        let mut v = Pt::default();
        (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
        v
    }

    #[test]
    fn all_names_present() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert!(def(n).supports_gpu(), "{n} has no wgsl");
        }
    }

    #[test]
    fn defaults_produce_finite_results() {
        let pts = [(0.3, -0.7, 0.2), (1.2, 0.4, -0.5), (-0.9, -1.1, 1.0), (-0.05, 0.6, 0.1), (1.7, -1.9, 0.3)];
        let mut ctx = Ctx::new(42, true);
        for def in DEFS {
            let data = def.prepare_data(&[], &Affine::IDENTITY);
            for &(x, y, z) in &pts {
                for amount in [1.0, 0.5] {
                    for _ in 0..8 {
                        let mut a = Pt { x, y, z, ..Default::default() };
                        let mut v = Pt::default();
                        (def.apply)(&data, &mut ctx, &mut a, &mut v, amount);
                        assert!(!v.is_bad(), "{} produced non-finite output at ({x},{y},{z})", def.name);
                        assert!(v.color.is_finite(), "{} produced a non-finite color", def.name);
                        assert!(!a.is_bad(), "{} corrupted the affine point", def.name);
                    }
                }
            }
        }
    }

    #[test]
    fn crop_shapes_hide_outside_points() {
        for name in ["crop_box", "crop_cross", "crop_polygon", "crop_rhombus", "crop_stars", "crop_trapezoid", "crop_vesica", "crop_x"] {
            let inside = run(name, &[], 0.01, 0.02, 0.0);
            assert!(!inside.do_hide, "{name}: centre should be visible");
            let outside = run(name, &[], 5.0, 4.0, 0.0);
            assert!(outside.do_hide, "{name}: far point should be hidden");
        }
        // crop: zero = 1 hides, zero = 0 clamps into the rectangle.
        assert!(run("crop", &[-1.0, 1.0, -1.0, 1.0, 0.0, 1.0], 2.0, 0.0, 0.0).do_hide);
        let clamped = run("crop", &[], 2.0, -3.0, 0.0);
        assert!(!clamped.do_hide && clamped.x == 1.0 && clamped.y == -1.0);
        // crop_triangle: inside test on the default triangle (0,0.5) (0.5,0.5) (0,-0.5).
        assert!(!run("crop_triangle", &[], 0.0, 0.5, 0.0).do_hide);
        assert!(run("crop_triangle", &[], -0.5, 0.0, 0.0).do_hide);
    }

    #[test]
    fn csin_matches_closed_form() {
        let (x, y) = (0.3_f64 * 1.5, -0.7_f64 * 1.5);
        let v = run("csin", &[], 0.3, -0.7, 0.0);
        assert!((v.x - x.sin() * y.cosh()).abs() < 1e-12);
        assert!((v.y - x.cos() * y.sinh()).abs() < 1e-12);
    }
}
