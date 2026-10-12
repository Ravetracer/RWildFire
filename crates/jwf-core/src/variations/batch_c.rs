//! Variations ported from the JWildfire sources (batch C): Apophysis plugin-pack classics and
//! the complex-function family (exp, log, sin, cos, ... coth).

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

pub static DEFS: &[VarDef] = &[
    VarDef {
        name: "modulus",
        params: &[ParamDef::range("x", 0.20, 0.0, 2.0), ParamDef::range("y", 0.50, 0.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x, y, xr, yr]
            let (x, y, xr, yr) = (d[0], d[1], d[2], d[3]);
            if a.x > x {
                v.x += amount * (-x + fmod(a.x + x, xr));
            } else if a.x < -x {
                v.x += amount * (x - fmod(x - a.x, xr));
            } else {
                v.x += amount * a.x;
            }
            if a.y > y {
                v.y += amount * (-y + fmod(a.y + y, yr));
            } else if a.y < -y {
                v.y += amount * (y - fmod(y - a.y, yr));
            } else {
                v.y += amount * a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| vec![p[0], p[1], 2.0 * p[0], 2.0 * p[1]]),
        wgsl: "if (a.x > D(0)) {
           v.x += AMOUNT * (-D(0) + fmod(a.x + D(0), D(2)));
         } else if (a.x < -D(0)) {
           v.x += AMOUNT * (D(0) - fmod(D(0) - a.x, D(2)));
         } else {
           v.x += AMOUNT * a.x;
         }
         if (a.y > D(1)) {
           v.y += AMOUNT * (-D(1) + fmod(a.y + D(1), D(3)));
         } else if (a.y < -D(1)) {
           v.y += AMOUNT * (D(1) - fmod(D(1) - a.y, D(3)));
         } else {
           v.y += AMOUNT * a.y;
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "oscilloscope",
        params: &[
            ParamDef::range("separation", 1.0, 0.0, 4.0),
            ParamDef::range("frequency", M_PI, 0.0, 10.0),
            ParamDef::range("amplitude", 1.0, -4.0, 4.0),
            ParamDef::range("damping", 0.0, 0.0, 4.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [separation, frequency, amplitude, damping, tpf, no_damping]
            let t = if d[5] != 0.0 {
                d[2] * (d[4] * a.x).cos() + d[0]
            } else {
                d[2] * (-a.x.abs() * d[3]).exp() * (d[4] * a.x).cos() + d[0]
            };
            v.x += amount * a.x;
            if a.y.abs() <= t {
                v.y -= amount * a.y;
            } else {
                v.y += amount * a.y;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let tpf = 2.0 * M_PI * p[1];
            let no_damping = if p[3].abs() <= EPSILON { 1.0 } else { 0.0 };
            vec![p[0], p[1], p[2], p[3], tpf, no_damping]
        }),
        wgsl: "var t = D(2) * cos(D(4) * a.x) + D(0);
         if (D(5) == 0.0) {
           t = D(2) * exp(-abs(a.x) * D(3)) * cos(D(4) * a.x) + D(0);
         }
         v.x += AMOUNT * a.x;
         if (abs(a.y) <= t) {
           v.y -= AMOUNT * a.y;
         } else {
           v.y += AMOUNT * a.y;
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "polar2",
        false,
        |_, ctx, a, v, amount| {
            let p2v = amount / M_PI;
            v.x += p2v * a.atan_xy();
            v.y += p2v / 2.0 * a.sumsq().ln();
            ctx.pass_z(a, v, amount);
        },
        "let p2v = AMOUNT / PI;
         v.x += p2v * atan_xy(a);
         v.y += p2v / 2.0 * log(sumsq(a));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "popcorn2",
        params: &[
            ParamDef::range("x", 1.0, -2.0, 2.0),
            ParamDef::range("y", 0.5, -2.0, 2.0),
            ParamDef::range("c", 1.5, -5.0, 5.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * (a.x + d[0] * (a.y * d[2]).tan().sin());
            v.y += amount * (a.y + d[1] * (a.x * d[2]).tan().sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * (a.x + D(0) * sin(tan(a.y * D(2))));
         v.y += AMOUNT * (a.y + D(1) * sin(tan(a.x * D(2))));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "scry",
        false,
        |_, ctx, a, v, amount| {
            let t = a.x * a.x + a.y * a.y;
            let d = t.sqrt() * (t + 1.0 / amount);
            if d == 0.0 {
                return;
            }
            let r = 1.0 / d;
            v.x += a.x * r;
            v.y += a.y * r;
            ctx.pass_z(a, v, amount);
        },
        "let t = a.x * a.x + a.y * a.y;
         let d = sqrt(t) * (t + 1.0 / AMOUNT);
         if (d != 0.0) {
           let r = 1.0 / d;
           v.x += a.x * r;
           v.y += a.y * r;
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    VarDef {
        name: "separation",
        params: &[
            ParamDef::range("x", 0.5, -2.0, 2.0),
            ParamDef::range("xinside", 0.05, -2.0, 2.0),
            ParamDef::range("y", 0.25, -2.0, 2.0),
            ParamDef::range("yinside", 0.025, -2.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let sx2 = d[0] * d[0];
            let sy2 = d[2] * d[2];
            if a.x > 0.0 {
                v.x += amount * ((a.x * a.x + sx2).sqrt() - a.x * d[1]);
            } else {
                v.x -= amount * ((a.x * a.x + sx2).sqrt() + a.x * d[1]);
            }
            if a.y > 0.0 {
                v.y += amount * ((a.y * a.y + sy2).sqrt() - a.y * d[3]);
            } else {
                v.y -= amount * ((a.y * a.y + sy2).sqrt() + a.y * d[3]);
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sx2 = D(0) * D(0);
         let sy2 = D(2) * D(2);
         if (a.x > 0.0) {
           v.x += AMOUNT * (sqrt(a.x * a.x + sx2) - a.x * D(1));
         } else {
           v.x -= AMOUNT * (sqrt(a.x * a.x + sx2) + a.x * D(1));
         }
         if (a.y > 0.0) {
           v.y += AMOUNT * (sqrt(a.y * a.y + sy2) - a.y * D(3));
         } else {
           v.y -= AMOUNT * (sqrt(a.y * a.y + sy2) + a.y * D(3));
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "split",
        params: &[ParamDef::range("xsize", 0.40, -2.0, 2.0), ParamDef::range("ysize", 0.60, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            if (a.x * d[0] * M_PI).cos() >= 0.0 {
                v.y += amount * a.y;
            } else {
                v.y -= amount * a.y;
            }
            if (a.y * d[1] * M_PI).cos() >= 0.0 {
                v.x += amount * a.x;
            } else {
                v.x -= amount * a.x;
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let sx = select(-1.0, 1.0, cos(a.x * D(0) * PI) >= 0.0);
         let sy = select(-1.0, 1.0, cos(a.y * D(1) * PI) >= 0.0);
         v.y += sx * AMOUNT * a.y;
         v.x += sy * AMOUNT * a.x;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "splits",
        params: &[
            ParamDef::range("x", 0.40, -2.0, 2.0),
            ParamDef::range("y", 0.60, -2.0, 2.0),
            ParamDef::range("lshear", 0.0, -2.0, 2.0),
            ParamDef::range("rshear", 0.0, -2.0, 2.0),
            ParamDef::range("ushear", 0.0, -2.0, 2.0),
            ParamDef::range("dshear", 0.0, -2.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            if a.x >= 0.0 {
                v.x += amount * (a.x + d[0]);
                v.y += amount * d[3];
            } else {
                v.x += amount * (a.x - d[0]);
                v.y -= amount * d[2];
            }
            if a.y >= 0.0 {
                v.y += amount * (a.y + d[1]);
                v.x += amount * d[4];
            } else {
                v.y += amount * (a.y - d[1]);
                v.x -= amount * d[5];
            }
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "if (a.x >= 0.0) {
           v.x += AMOUNT * (a.x + D(0));
           v.y += AMOUNT * D(3);
         } else {
           v.x += AMOUNT * (a.x - D(0));
           v.y -= AMOUNT * D(2);
         }
         if (a.y >= 0.0) {
           v.y += AMOUNT * (a.y + D(1));
           v.x += AMOUNT * D(4);
         } else {
           v.y += AMOUNT * (a.y - D(1));
           v.x -= AMOUNT * D(5);
         }
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "stripes",
        params: &[ParamDef::range("space", 0.20, -2.0, 2.0), ParamDef::range("warp", 0.60, -4.0, 4.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let roundx = (a.x + 0.5).floor();
            let offsetx = a.x - roundx;
            v.x += amount * (offsetx * (1.0 - d[0]) + roundx);
            v.y += amount * (a.y + offsetx * offsetx * d[1]);
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let roundx = floor(a.x + 0.5);
         let offsetx = a.x - roundx;
         v.x += AMOUNT * (offsetx * (1.0 - D(0)) + roundx);
         v.y += AMOUNT * (a.y + offsetx * offsetx * D(1));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "wedge",
        params: &[
            ParamDef::range("angle", M_PI_2, -M_2PI, M_2PI),
            ParamDef::range("hole", 0.0, -2.0, 2.0),
            ParamDef::int("count", 1.0, 1.0, 50.0),
            ParamDef::range("swirl", 0.1, -2.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [angle, hole, count, swirl, comp_fac]
            let mut r = a.rad();
            let mut ang = a.atan_yx() + d[3] * r;
            let c = ((d[2] * ang + M_PI) * M_1_PI * 0.5).floor();
            ang = ang * d[4] + c * d[0];
            let (sa, ca) = ang.sin_cos();
            r = amount * (r + d[1]);
            v.x += r * ca;
            v.y += r * sa;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            // Tools.FTOI rounds half away from zero.
            let count = p[2].round();
            let comp_fac = 1.0 - p[0] * count * M_1_PI * 0.5;
            vec![p[0], p[1], count, p[3], comp_fac]
        }),
        wgsl: "let r = rad(a);
         var ang = atan_yx(a) + D(3) * r;
         let c = floor((D(2) * ang + PI) * (0.5 / PI));
         ang = ang * D(4) + c * D(0);
         let rr = AMOUNT * (r + D(1));
         v.x += rr * cos(ang);
         v.y += rr * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "wedge_julia",
        params: &[
            ParamDef::range("power", 7.0, -12.0, 12.0),
            ParamDef::range("dist", 0.20, -4.0, 4.0),
            ParamDef::range("count", 2.0, 0.0, 20.0),
            ParamDef::range("angle", 0.30, -M_2PI, M_2PI),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [power, dist, count, angle, cf, rN, cn]
            let r = amount * a.sumsq().powf(d[6]);
            let t_rnd = (d[5] * ctx.random()) as i32 as f64;
            let mut ang = (a.atan_yx() + 2.0 * M_PI * t_rnd) / d[0];
            let c = ((d[2] * ang + M_PI) * M_1_PI * 0.5).floor();
            ang = ang * d[4] + c * d[3];
            let (sa, ca) = ang.sin_cos();
            v.x += r * ca;
            v.y += r * sa;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let cf = 1.0 - p[3] * p[2] * M_1_PI * 0.5;
            let rn = p[0].abs();
            let cn = p[1] / p[0] / 2.0;
            vec![p[0], p[1], p[2], p[3], cf, rn, cn]
        }),
        wgsl: "let r = AMOUNT * pow(sumsq(a), D(6));
         let t_rnd = trunc(D(5) * rnd());
         var ang = (atan_yx(a) + TWO_PI * t_rnd) / D(0);
         let c = floor((D(2) * ang + PI) * (0.5 / PI));
         ang = ang * D(4) + c * D(3);
         v.x += r * cos(ang);
         v.y += r * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "wedge_sph",
        params: &[
            ParamDef::range("angle", 0.20, -M_2PI, M_2PI),
            ParamDef::range("hole", 0.20, -2.0, 2.0),
            ParamDef::range("count", 2.0, 0.0, 20.0),
            ParamDef::range("swirl", 0.30, -2.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [angle, hole, count, swirl, comp_fac]
            let mut r = 1.0 / (a.rad() + SMALL_EPSILON);
            let mut ang = a.atan_yx() + d[3] * r;
            let c = ((d[2] * ang + M_PI) * M_1_PI * 0.5).floor();
            ang = ang * d[4] + c * d[0];
            let (sa, ca) = ang.sin_cos();
            r = amount * (r + d[1]);
            v.x += r * ca;
            v.y += r * sa;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let comp_fac = 1.0 - p[0] * p[2] * M_1_PI * 0.5;
            vec![p[0], p[1], p[2], p[3], comp_fac]
        }),
        wgsl: "let r = 1.0 / (rad(a) + EPS);
         var ang = atan_yx(a) + D(3) * r;
         let c = floor((D(2) * ang + PI) * (0.5 / PI));
         ang = ang * D(4) + c * D(0);
         let rr = AMOUNT * (r + D(1));
         v.x += rr * cos(ang);
         v.y += rr * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "whorl",
        params: &[ParamDef::range("inside", 0.10, -2.0, 2.0), ParamDef::range("outside", 0.20, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let r = a.rad();
            let ang = if r < amount {
                a.atan_yx() + d[0] / (amount - r)
            } else {
                a.atan_yx() + d[1] / (amount - r)
            };
            let (sa, ca) = ang.sin_cos();
            v.x += amount * r * ca;
            v.y += amount * r * sa;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = rad(a);
         let ang = atan_yx(a) + select(D(1), D(0), r < AMOUNT) / (AMOUNT - r);
         v.x += AMOUNT * r * cos(ang);
         v.y += AMOUNT * r * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "waves2",
        params: &[
            ParamDef::range("scalex", 0.25, -2.0, 2.0),
            ParamDef::range("scaley", 0.5, -2.0, 2.0),
            ParamDef::range("freqx", M_PI / 2.0, -10.0, 10.0),
            ParamDef::range("freqy", M_PI / 4.0, -10.0, 10.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            v.x += amount * (a.x + d[0] * (a.y * d[2]).sin());
            v.y += amount * (a.y + d[1] * (a.x * d[3]).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * (a.x + D(0) * sin(a.y * D(2)));
         v.y += AMOUNT * (a.y + D(1) * sin(a.x * D(3)));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    // --- complex functions by cothe ---
    simple(
        "exp",
        false,
        |_, ctx, a, v, amount| {
            let expe = a.x.exp();
            let (expsin, expcos) = a.y.sin_cos();
            v.x += amount * expe * expcos;
            v.y += amount * expe * expsin;
            ctx.pass_z(a, v, amount);
        },
        "let expe = exp(a.x);
         v.x += AMOUNT * expe * cos(a.y);
         v.y += AMOUNT * expe * sin(a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "log",
        false,
        |_, ctx, a, v, amount| {
            v.x += amount * 0.5 * a.sumsq().ln();
            v.y += amount * a.atan_yx();
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * 0.5 * log(sumsq(a));
         v.y += AMOUNT * atan_yx(a);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "sin",
        false,
        |_, ctx, a, v, amount| {
            let (s, c) = a.x.sin_cos();
            v.x += amount * s * a.y.cosh();
            v.y += amount * c * a.y.sinh();
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * sin(a.x) * cosh(a.y);
         v.y += AMOUNT * cos(a.x) * sinh(a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "cos",
        false,
        |_, ctx, a, v, amount| {
            let (s, c) = a.x.sin_cos();
            v.x += amount * c * a.y.cosh();
            v.y -= amount * s * a.y.sinh();
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * cos(a.x) * cosh(a.y);
         v.y -= AMOUNT * sin(a.x) * sinh(a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "tan",
        false,
        |_, ctx, a, v, amount| {
            let (tansin, tancos) = (2.0 * a.x).sin_cos();
            let tansinh = (2.0 * a.y).sinh();
            let tancosh = (2.0 * a.y).cosh();
            let d = tancos + tancosh;
            if d == 0.0 {
                return;
            }
            let tanden = 1.0 / d;
            v.x += amount * tanden * tansin;
            v.y += amount * tanden * tansinh;
            ctx.pass_z(a, v, amount);
        },
        "let d = cos(2.0 * a.x) + cosh(2.0 * a.y);
         if (d != 0.0) {
           let den = 1.0 / d;
           v.x += AMOUNT * den * sin(2.0 * a.x);
           v.y += AMOUNT * den * sinh(2.0 * a.y);
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    simple(
        "sec",
        false,
        |_, ctx, a, v, amount| {
            let (secsin, seccos) = a.x.sin_cos();
            let secsinh = a.y.sinh();
            let seccosh = a.y.cosh();
            let d = (2.0 * a.x).cos() + (2.0 * a.y).cosh();
            if d == 0.0 {
                return;
            }
            let secden = 2.0 / d;
            v.x += amount * secden * seccos * seccosh;
            v.y += amount * secden * secsin * secsinh;
            ctx.pass_z(a, v, amount);
        },
        "let d = cos(2.0 * a.x) + cosh(2.0 * a.y);
         if (d != 0.0) {
           let den = 2.0 / d;
           v.x += AMOUNT * den * cos(a.x) * cosh(a.y);
           v.y += AMOUNT * den * sin(a.x) * sinh(a.y);
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    simple(
        "csc",
        false,
        |_, ctx, a, v, amount| {
            let (cscsin, csccos) = a.x.sin_cos();
            let cscsinh = a.y.sinh();
            let csccosh = a.y.cosh();
            let d = (2.0 * a.y).cosh() - (2.0 * a.x).cos();
            if d == 0.0 {
                return;
            }
            let cscden = 2.0 / d;
            v.x += amount * cscden * cscsin * csccosh;
            v.y -= amount * cscden * csccos * cscsinh;
            ctx.pass_z(a, v, amount);
        },
        "let d = cosh(2.0 * a.y) - cos(2.0 * a.x);
         if (d != 0.0) {
           let den = 2.0 / d;
           v.x += AMOUNT * den * sin(a.x) * cosh(a.y);
           v.y -= AMOUNT * den * cos(a.x) * sinh(a.y);
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    simple(
        "cot",
        false,
        |_, ctx, a, v, amount| {
            // No zero check in JWildfire: division by zero yields inf at the singular points.
            let (cotsin, cotcos) = (2.0 * a.x).sin_cos();
            let cotsinh = (2.0 * a.y).sinh();
            let cotcosh = (2.0 * a.y).cosh();
            let cotden = 1.0 / (cotcosh - cotcos);
            v.x += amount * cotden * cotsin;
            v.y += amount * cotden * -1.0 * cotsinh;
            ctx.pass_z(a, v, amount);
        },
        "let den = 1.0 / (cosh(2.0 * a.y) - cos(2.0 * a.x));
         v.x += AMOUNT * den * sin(2.0 * a.x);
         v.y -= AMOUNT * den * sinh(2.0 * a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "sinh",
        false,
        |_, ctx, a, v, amount| {
            let (s, c) = a.y.sin_cos();
            v.x += amount * a.x.sinh() * c;
            v.y += amount * a.x.cosh() * s;
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * sinh(a.x) * cos(a.y);
         v.y += AMOUNT * cosh(a.x) * sin(a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "cosh",
        false,
        |_, ctx, a, v, amount| {
            let (s, c) = a.y.sin_cos();
            v.x += amount * a.x.cosh() * c;
            v.y += amount * a.x.sinh() * s;
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * cosh(a.x) * cos(a.y);
         v.y += AMOUNT * sinh(a.x) * sin(a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "tanh",
        false,
        |_, ctx, a, v, amount| {
            let (tanhsin, tanhcos) = (2.0 * a.y).sin_cos();
            let tanhsinh = (2.0 * a.x).sinh();
            let tanhcosh = (2.0 * a.x).cosh();
            let d = tanhcos + tanhcosh;
            if d == 0.0 {
                return;
            }
            let tanhden = 1.0 / d;
            v.x += amount * tanhden * tanhsinh;
            v.y += amount * tanhden * tanhsin;
            ctx.pass_z(a, v, amount);
        },
        "let d = cos(2.0 * a.y) + cosh(2.0 * a.x);
         if (d != 0.0) {
           let den = 1.0 / d;
           v.x += AMOUNT * den * sinh(2.0 * a.x);
           v.y += AMOUNT * den * sin(2.0 * a.y);
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    simple(
        "sech",
        false,
        |_, ctx, a, v, amount| {
            let (sechsin, sechcos) = a.y.sin_cos();
            let sechsinh = a.x.sinh();
            let sechcosh = a.x.cosh();
            let d = (2.0 * a.y).cos() + (2.0 * a.x).cosh();
            if d == 0.0 {
                return;
            }
            let sechden = 2.0 / d;
            v.x += amount * sechden * sechcos * sechcosh;
            v.y -= amount * sechden * sechsin * sechsinh;
            ctx.pass_z(a, v, amount);
        },
        "let d = cos(2.0 * a.y) + cosh(2.0 * a.x);
         if (d != 0.0) {
           let den = 2.0 / d;
           v.x += AMOUNT * den * cos(a.y) * cosh(a.x);
           v.y -= AMOUNT * den * sin(a.y) * sinh(a.x);
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    simple(
        "csch",
        false,
        |_, ctx, a, v, amount| {
            let (cschsin, cschcos) = a.y.sin_cos();
            let cschsinh = a.x.sinh();
            let cschcosh = a.x.cosh();
            let d = (2.0 * a.x).cosh() - (2.0 * a.y).cos();
            if d == 0.0 {
                return;
            }
            let cschden = 2.0 / d;
            v.x += amount * cschden * cschsinh * cschcos;
            v.y -= amount * cschden * cschcosh * cschsin;
            ctx.pass_z(a, v, amount);
        },
        "let d = cosh(2.0 * a.x) - cos(2.0 * a.y);
         if (d != 0.0) {
           let den = 2.0 / d;
           v.x += AMOUNT * den * sinh(a.x) * cos(a.y);
           v.y -= AMOUNT * den * cosh(a.x) * sin(a.y);
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    simple(
        "coth",
        false,
        |_, ctx, a, v, amount| {
            let (cothsin, cothcos) = (2.0 * a.y).sin_cos();
            let cothsinh = (2.0 * a.x).sinh();
            let cothcosh = (2.0 * a.x).cosh();
            let d = cothcosh - cothcos;
            if d == 0.0 {
                return;
            }
            let cothden = 1.0 / d;
            v.x += amount * cothden * cothsinh;
            v.y += amount * cothden * cothsin;
            ctx.pass_z(a, v, amount);
        },
        "let d = cosh(2.0 * a.x) - cos(2.0 * a.y);
         if (d != 0.0) {
           let den = 1.0 / d;
           v.x += AMOUNT * den * sinh(2.0 * a.x);
           v.y += AMOUNT * den * sin(2.0 * a.y);
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    ),
    // --- misc ---
    VarDef {
        name: "auger",
        params: &[
            ParamDef::range("freq", 1.0, -10.0, 10.0),
            ParamDef::range("weight", 0.5, -2.0, 2.0),
            ParamDef::range("sym", 0.1, -2.0, 2.0),
            ParamDef::range("scale", 0.9, -2.0, 2.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [freq, weight, sym, scale]
            let s = (d[0] * a.x).sin();
            let t = (d[0] * a.y).sin();
            let dy = a.y + d[1] * (d[3] * s * 0.5 + a.y.abs() * s);
            let dx = a.x + d[1] * (d[3] * t * 0.5 + a.x.abs() * t);
            v.x += amount * (a.x + d[2] * (dx - a.x));
            v.y += amount * dy;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let s = sin(D(0) * a.x);
         let t = sin(D(0) * a.y);
         let dy = a.y + D(1) * (D(3) * s * 0.5 + abs(a.y) * s);
         let dx = a.x + D(1) * (D(3) * t * 0.5 + abs(a.x) * t);
         v.x += AMOUNT * (a.x + D(2) * (dx - a.x));
         v.y += AMOUNT * dy;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "flux",
        params: &[ParamDef::range("spread", 0.30, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let xpw = a.x + amount;
            let xmw = a.x - amount;
            let avgr = amount * (2.0 + d[0]) * ((a.y * a.y + xpw * xpw).sqrt() / (a.y * a.y + xmw * xmw).sqrt()).sqrt();
            let avga = (a.y.atan2(xmw) - a.y.atan2(xpw)) * 0.5;
            let (sa, ca) = avga.sin_cos();
            v.x += avgr * ca;
            v.y += avgr * sa;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let xpw = a.x + AMOUNT;
         let xmw = a.x - AMOUNT;
         let avgr = AMOUNT * (2.0 + D(0)) * sqrt(sqrt(a.y * a.y + xpw * xpw) / sqrt(a.y * a.y + xmw * xmw));
         let avga = (atan2(a.y, xmw) - atan2(a.y, xpw)) * 0.5;
         v.x += avgr * cos(avga);
         v.y += avgr * sin(avga);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "modulus", "oscilloscope", "polar2", "popcorn2", "scry", "separation", "split", "splits", "stripes", "wedge",
        "wedge_julia", "wedge_sph", "whorl", "waves2", "exp", "log", "sin", "cos", "tan", "sec", "csc", "cot", "sinh",
        "cosh", "tanh", "sech", "csch", "coth", "auger", "flux",
    ];

    #[test]
    fn all_names_present_and_have_gpu() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            let def = DEFS.iter().find(|d| d.name == *n).unwrap_or_else(|| panic!("missing {n}"));
            assert!(def.supports_gpu(), "{n} has no wgsl");
        }
    }

    #[test]
    fn defaults_produce_finite_results() {
        let pts = [(0.3, -0.7, 0.2), (1.2, 0.4, -0.5), (-0.9, -1.1, 1.0), (-0.05, 0.6, 0.0), (1.7, -1.9, 0.3)];
        let mut ctx = Ctx::new(42, true);
        for def in DEFS {
            let data = def.prepare_data(&[], &Affine::IDENTITY);
            for &(x, y, z) in &pts {
                for amount in [1.0, 0.5] {
                    for _ in 0..8 {
                        let mut a = Pt { x, y, z, ..Default::default() };
                        let mut v = Pt::default();
                        (def.apply)(&data, &mut ctx, &mut a, &mut v, amount);
                        assert!(!v.is_bad(), "{} produced non-finite output at ({x},{y})", def.name);
                        assert!(!a.is_bad(), "{} corrupted the affine point", def.name);
                    }
                }
            }
        }
    }

    #[test]
    fn complex_functions_match_reference() {
        // z = 0.3 - 0.7i; compare against closed forms.
        let (x, y) = (0.3_f64, -0.7_f64);
        let run = |name: &str| {
            let def = DEFS.iter().find(|d| d.name == name).unwrap();
            let mut ctx = Ctx::new(1, false);
            let mut a = Pt { x, y, ..Default::default() };
            let mut v = Pt::default();
            (def.apply)(&def.prepare_data(&[], &Affine::IDENTITY), &mut ctx, &mut a, &mut v, 1.0);
            (v.x, v.y)
        };
        let close = |p: (f64, f64), q: (f64, f64)| (p.0 - q.0).abs() < 1e-12 && (p.1 - q.1).abs() < 1e-12;
        // sin(z) = sin x cosh y + i cos x sinh y
        assert!(close(run("sin"), (x.sin() * y.cosh(), x.cos() * y.sinh())));
        // tan(z) = sin(2x)/(cos2x+cosh2y) + i sinh(2y)/(cos2x+cosh2y)
        let d = (2.0 * x).cos() + (2.0 * y).cosh();
        assert!(close(run("tan"), ((2.0 * x).sin() / d, (2.0 * y).sinh() / d)));
        // exp(log(z)) round trip via log: log|z|, arg z
        assert!(close(run("log"), ((x * x + y * y).sqrt().ln(), y.atan2(x))));
    }
}
