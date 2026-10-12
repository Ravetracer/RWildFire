//! Basic variations, ported from the JWildfire sources. They also serve as the reference
//! for how CPU code and WGSL snippets are written.

use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
use crate::math::*;
use crate::model::Affine;

pub static DEFS: &[VarDef] = &[
    simple(
        "linear",
        false,
        |_, ctx, a, v, amount| {
            v.x += amount * a.x;
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * a.x;
         v.y += AMOUNT * a.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "linear3D",
        true,
        |_, _, a, v, amount| {
            v.x += amount * a.x;
            v.y += amount * a.y;
            v.z += amount * a.z;
        },
        "v.x += AMOUNT * a.x;
         v.y += AMOUNT * a.y;
         v.z += AMOUNT * a.z;",
    ),
    simple(
        "sinusoidal",
        false,
        |_, ctx, a, v, amount| {
            v.x += amount * a.x.sin();
            v.y += amount * a.y.sin();
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * sin(a.x);
         v.y += AMOUNT * sin(a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "spherical",
        false,
        |_, ctx, a, v, amount| {
            let r = amount / (a.x * a.x + a.y * a.y + SMALL_EPSILON);
            v.x += a.x * r;
            v.y += a.y * r;
            ctx.pass_z(a, v, amount);
        },
        "let r = AMOUNT / (a.x * a.x + a.y * a.y + EPS);
         v.x += a.x * r;
         v.y += a.y * r;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "swirl",
        false,
        |_, ctx, a, v, amount| {
            let r2 = a.x * a.x + a.y * a.y;
            let (c1, c2) = r2.sin_cos();
            v.x += amount * (c1 * a.x - c2 * a.y);
            v.y += amount * (c2 * a.x + c1 * a.y);
            ctx.pass_z(a, v, amount);
        },
        "let r2 = a.x * a.x + a.y * a.y;
         let c1 = sin(r2);
         let c2 = cos(r2);
         v.x += AMOUNT * (c1 * a.x - c2 * a.y);
         v.y += AMOUNT * (c2 * a.x + c1 * a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "julia",
        false,
        |_, ctx, a, v, amount| {
            let ang = a.atan_xy() * 0.5 + M_PI * ((2.0 * ctx.random()) as i32) as f64;
            let (sina, cosa) = ang.sin_cos();
            let r = amount * (a.x * a.x + a.y * a.y).sqrt().sqrt();
            v.x += r * cosa;
            v.y += r * sina;
            ctx.pass_z(a, v, amount);
        },
        "let ang = atan_xy(a) * 0.5 + PI * floor(2.0 * rnd());
         let r = AMOUNT * sqrt(sqrt(a.x * a.x + a.y * a.y));
         v.x += r * cos(ang);
         v.y += r * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "popcorn",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            let mut dx = (3.0 * a.y).tan();
            if dx.is_nan() {
                dx = 0.0;
            }
            let mut dy = (3.0 * a.x).tan();
            if dy.is_nan() {
                dy = 0.0;
            }
            let nx = a.x + d[0] * dx.sin();
            let ny = a.y + d[1] * dy.sin();
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        // d[0], d[1] = xform translation (c20, c21)
        prepare: Some(|_, c: &Affine| vec![c.c20, c.c21]),
        wgsl: "var dx = tan(3.0 * a.y);
         if (dx != dx) { dx = 0.0; }
         var dy = tan(3.0 * a.x);
         if (dy != dy) { dy = 0.0; }
         v.x += AMOUNT * (a.x + D(0) * sin(dx));
         v.y += AMOUNT * (a.y + D(1) * sin(dy));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "pre_blur",
        params: &[],
        kind: VarKind::Pre,
        is_3d: false,
        apply: |_, ctx, a, _v, amount| {
            let ang = ctx.random() * M_2PI;
            let (sina, cosa) = ang.sin_cos();
            let mut g = -3.0;
            for _ in 0..6 {
                g += ctx.random();
            }
            let r = amount * g;
            a.x += r * cosa;
            a.y += r * sina;
        },
        prepare: None,
        wgsl: "let ang = rnd() * TWO_PI;
         let r = AMOUNT * (rnd() + rnd() + rnd() + rnd() + rnd() + rnd() - 3.0);
         a.x += r * cos(ang);
         a.y += r * sin(ang);",
    },
    simple(
        "ztranslate",
        true,
        |_, _, _, v, amount| {
            v.z += amount;
        },
        "v.z += AMOUNT;",
    ),
    VarDef {
        name: "julian",
        params: &[ParamDef::int("power", 2.0, -12.0, 12.0), ParamDef::range("dist", 1.0, -4.0, 4.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [power, dist, abs_power, cn]
            let t_rnd = (d[2] * ctx.random()) as i32 as f64;
            let tmpr = (a.atan_yx() + M_2PI * t_rnd) / d[0];
            let r = amount * a.sumsq().powf(d[3]);
            let (sina, cosa) = tmpr.sin_cos();
            v.x += r * cosa;
            v.y += r * sina;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _| {
            let power = if p[0] == 0.0 { 1.0 } else { p[0].round() };
            let abs_power = power.abs();
            let cn = p[1] / power / 2.0;
            vec![power, p[1], abs_power, cn]
        }),
        wgsl: "let t_rnd = floor(D(2) * rnd());
         let tmpr = (atan_yx(a) + TWO_PI * t_rnd) / D(0);
         let r = AMOUNT * pow(sumsq(a), D(3));
         v.x += r * cos(tmpr);
         v.y += r * sin(tmpr);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];

#[allow(dead_code)]
fn _assert_types(_: &Pt, _: &mut Ctx) {}
