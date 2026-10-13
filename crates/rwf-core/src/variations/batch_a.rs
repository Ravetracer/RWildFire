//! Variations ported from the JWildfire sources (batch A: the classic flam3 set).

#[allow(unused_imports)]
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
#[allow(unused_imports)]
use crate::math::*;
#[allow(unused_imports)]
use crate::model::Affine;

/// JWildfire `PolarFunc.R_PI` (an approximation of 1/pi, kept for fidelity).
const R_PI: f64 = 0.31830989;

pub static DEFS: &[VarDef] = &[
    simple(
        "horseshoe",
        false,
        |_, ctx, a, v, amount| {
            let sin_a = a.sin_a();
            let cos_a = a.cos_a();
            v.x += amount * (sin_a * a.x - cos_a * a.y);
            v.y += amount * (cos_a * a.x + sin_a * a.y);
            ctx.pass_z(a, v, amount);
        },
        "let r = rad(a);
         let sin_a = a.x / r;
         let cos_a = a.y / r;
         v.x += AMOUNT * (sin_a * a.x - cos_a * a.y);
         v.y += AMOUNT * (cos_a * a.x + sin_a * a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "polar",
        false,
        |_, ctx, a, v, amount| {
            let ny = a.sumsq().sqrt() - 1.0;
            v.x += amount * (a.atan_xy() * R_PI);
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        "let ny = sqrt(sumsq(a)) - 1.0;
         v.x += AMOUNT * (atan_xy(a) * 0.31830989);
         v.y += AMOUNT * ny;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "handkerchief",
        false,
        |_, ctx, a, v, amount| {
            let angle = a.atan_xy();
            let r = a.sumsq().sqrt();
            v.x += amount * ((angle + r).sin() * r);
            v.y += amount * ((angle - r).cos() * r);
            ctx.pass_z(a, v, amount);
        },
        "let angle = atan_xy(a);
         let r = sqrt(sumsq(a));
         v.x += AMOUNT * (sin(angle + r) * r);
         v.y += AMOUNT * (cos(angle - r) * r);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "heart",
        false,
        |_, ctx, a, v, amount| {
            let r = a.sumsq().sqrt();
            let angle = a.atan_xy();
            let (sinr, cosr) = (r * angle).sin_cos();
            let r = r * amount;
            v.x += r * sinr;
            v.y -= r * cosr;
            ctx.pass_z(a, v, amount);
        },
        "let r0 = sqrt(sumsq(a));
         let angle = atan_xy(a);
         let sinr = sin(r0 * angle);
         let cosr = cos(r0 * angle);
         let r = r0 * AMOUNT;
         v.x += r * sinr;
         v.y -= r * cosr;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "disc",
        false,
        |_, ctx, a, v, amount| {
            let r_pi = M_PI * a.sumsq().sqrt();
            let (sinr, cosr) = r_pi.sin_cos();
            let r = amount * a.atan_xy() / M_PI;
            v.x += sinr * r;
            v.y += cosr * r;
            ctx.pass_z(a, v, amount);
        },
        "let r_pi = PI * sqrt(sumsq(a));
         let r = AMOUNT * atan_xy(a) / PI;
         v.x += sin(r_pi) * r;
         v.y += cos(r_pi) * r;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "spiral",
        false,
        |_, ctx, a, v, amount| {
            let sin_a = a.sin_a();
            let cos_a = a.cos_a();
            let r = a.rad();
            let (sinr, cosr) = r.sin_cos();
            let r = amount / r;
            v.x += (cos_a + sinr) * r;
            v.y += (sin_a - cosr) * r;
            ctx.pass_z(a, v, amount);
        },
        "let r0 = rad(a);
         let sin_a = a.x / r0;
         let cos_a = a.y / r0;
         let r = AMOUNT / r0;
         v.x += (cos_a + sin(r0)) * r;
         v.y += (sin_a - cos(r0)) * r;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "hyperbolic",
        false,
        |_, ctx, a, v, amount| {
            let r = a.rad();
            v.x += amount * a.sin_a() / r;
            v.y += amount * a.cos_a() * r;
            ctx.pass_z(a, v, amount);
        },
        "let r = rad(a);
         v.x += AMOUNT * (a.x / r) / r;
         v.y += AMOUNT * (a.y / r) * r;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "diamond",
        false,
        |_, ctx, a, v, amount| {
            let length = a.rad();
            let (sinr, cosr) = length.sin_cos();
            v.x += amount * a.sin_a() * cosr;
            v.y += amount * a.cos_a() * sinr;
            ctx.pass_z(a, v, amount);
        },
        "let len0 = rad(a);
         v.x += AMOUNT * (a.x / len0) * cos(len0);
         v.y += AMOUNT * (a.y / len0) * sin(len0);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "ex",
        false,
        |_, ctx, a, v, amount| {
            let r = a.sumsq().sqrt();
            let n0 = (a.atan_xy() + r).sin();
            let n1 = (a.atan_xy() - r).cos();
            let m0 = n0 * n0 * n0;
            let m1 = n1 * n1 * n1;
            let r = r * amount;
            v.x += r * (m0 + m1);
            v.y += r * (m0 - m1);
            ctx.pass_z(a, v, amount);
        },
        "let r0 = sqrt(sumsq(a));
         let n0 = sin(atan_xy(a) + r0);
         let n1 = cos(atan_xy(a) - r0);
         let m0 = n0 * n0 * n0;
         let m1 = n1 * n1 * n1;
         let r = r0 * AMOUNT;
         v.x += r * (m0 + m1);
         v.y += r * (m0 - m1);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "bent",
        false,
        |_, ctx, a, v, amount| {
            let mut nx = a.x;
            let mut ny = a.y;
            if nx < 0.0 {
                nx += nx;
            }
            if ny < 0.0 {
                ny *= 0.5;
            }
            v.x += amount * nx;
            v.y += amount * ny;
            ctx.pass_z(a, v, amount);
        },
        "let nx = select(a.x, a.x + a.x, a.x < 0.0);
         let ny = select(a.y, a.y * 0.5, a.y < 0.0);
         v.x += AMOUNT * nx;
         v.y += AMOUNT * ny;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "waves",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [c10, c11, c20, c21]
            v.x += amount * (a.x + d[0] * (a.y / (d[2] * d[2] + SMALL_EPSILON)).sin());
            v.y += amount * (a.y + d[1] * (a.x / (d[3] * d[3] + SMALL_EPSILON)).sin());
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|_, c: &Affine| vec![c.c10, c.c11, c.c20, c.c21]),
        wgsl: "v.x += AMOUNT * (a.x + D(0) * sin(a.y / (D(2) * D(2) + EPS)));
         v.y += AMOUNT * (a.y + D(1) * sin(a.x / (D(3) * D(3) + EPS)));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "fisheye",
        false,
        |_, ctx, a, v, amount| {
            let r = a.sumsq().sqrt();
            let r = 2.0 * r / (r + 1.0);
            v.x += amount * r * a.cos_a();
            v.y += amount * r * a.sin_a();
            ctx.pass_z(a, v, amount);
        },
        "let r0 = sqrt(sumsq(a));
         let r = 2.0 * r0 / (r0 + 1.0);
         let rr = rad(a);
         v.x += AMOUNT * r * (a.y / rr);
         v.y += AMOUNT * r * (a.x / rr);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "exponential",
        false,
        |_, ctx, a, v, amount| {
            let (sinr, cosr) = (M_PI * a.y).sin_cos();
            let d = amount * (a.x - 1.0).exp();
            v.x += cosr * d;
            v.y += sinr * d;
            ctx.pass_z(a, v, amount);
        },
        "let r = PI * a.y;
         let dd = AMOUNT * exp(a.x - 1.0);
         v.x += cos(r) * dd;
         v.y += sin(r) * dd;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "power",
        false,
        |_, ctx, a, v, amount| {
            let sin_a = a.sin_a();
            let r = amount * a.rad().powf(sin_a);
            v.x += r * a.cos_a();
            v.y += r * sin_a;
            ctx.pass_z(a, v, amount);
        },
        "let r0 = rad(a);
         let sin_a = a.x / r0;
         let r = AMOUNT * pow(r0, sin_a);
         v.x += r * (a.y / r0);
         v.y += r * sin_a;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "cosine",
        false,
        |_, ctx, a, v, amount| {
            let (sinr, cosr) = (a.x * M_PI).sin_cos();
            v.x += amount * cosr * a.y.cosh();
            v.y -= amount * sinr * a.y.sinh();
            ctx.pass_z(a, v, amount);
        },
        "let r = a.x * PI;
         v.x += AMOUNT * cos(r) * cosh(a.y);
         v.y -= AMOUNT * sin(r) * sinh(a.y);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "rings",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        // Note: like JWildfire's CPU code, the result is not scaled by the amount.
        apply: |d, ctx, a, v, amount| {
            // d = [c20]
            let dx = d[0] * d[0] + SMALL_EPSILON;
            let r = a.rad();
            let r = r + dx - (((r + dx) / (2.0 * dx)) as i32 as f64) * 2.0 * dx - dx + r * (1.0 - dx);
            v.x += r * a.cos_a();
            v.y += r * a.sin_a();
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|_, c: &Affine| vec![c.c20]),
        wgsl: "let dx = D(0) * D(0) + EPS;
         let r0 = rad(a);
         let r = r0 + dx - trunc((r0 + dx) / (2.0 * dx)) * 2.0 * dx - dx + r0 * (1.0 - dx);
         v.x += r * (a.y / r0);
         v.y += r * (a.x / r0);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "fan",
        params: &[],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [c20, c21]
            let dx = M_PI * d[0] * d[0] + SMALL_EPSILON;
            let dx2 = dx / 2.0;
            let atan = a.atan_xy();
            let t = atan + d[1] - (((atan + d[1]) / dx) as i32 as f64) * dx;
            let ang = if t > dx2 { atan - dx2 } else { atan + dx2 };
            let (sinr, cosr) = ang.sin_cos();
            let r = amount * a.sumsq().sqrt();
            v.x += r * cosr;
            v.y += r * sinr;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|_, c: &Affine| vec![c.c20, c.c21]),
        wgsl: "let dx = PI * D(0) * D(0) + EPS;
         let dx2 = dx / 2.0;
         let at = atan_xy(a);
         let t = at + D(1) - trunc((at + D(1)) / dx) * dx;
         let ang = select(at + dx2, at - dx2, t > dx2);
         let r = AMOUNT * sqrt(sumsq(a));
         v.x += r * cos(ang);
         v.y += r * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "blob",
        params: &[
            ParamDef::range("low", 0.3, -1.0, 2.0),
            ParamDef::range("high", 1.2, -1.0, 3.0),
            ParamDef::range("waves", 6.0, 0.0, 30.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [low, high, waves]
            let ang = a.atan_xy();
            let r = a.sumsq().sqrt();
            let r = r * (d[0] + (d[1] - d[0]) * (0.5 + 0.5 * (d[2] * ang).sin()));
            let (sina, cosa) = ang.sin_cos();
            v.x += amount * sina * r;
            v.y += amount * cosa * r;
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let ang = atan_xy(a);
         let r = sqrt(sumsq(a)) * (D(0) + (D(1) - D(0)) * (0.5 + 0.5 * sin(D(2) * ang)));
         v.x += AMOUNT * sin(ang) * r;
         v.y += AMOUNT * cos(ang) * r;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "pdj",
        params: &[
            ParamDef::range("a", 1.0, -6.0, 6.0),
            ParamDef::range("b", 2.0, -6.0, 6.0),
            ParamDef::range("c", 3.0, -6.0, 6.0),
            ParamDef::range("d", 4.0, -6.0, 6.0),
        ],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [a, b, c, d]
            v.x += amount * ((d[0] * a.y).sin() - (d[1] * a.x).cos());
            v.y += amount * ((d[2] * a.x).sin() - (d[3] * a.y).cos());
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "v.x += AMOUNT * (sin(D(0) * a.y) - cos(D(1) * a.x));
         v.y += AMOUNT * (sin(D(2) * a.x) - cos(D(3) * a.y));
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "fan2",
        params: &[ParamDef::range("x", 0.5, -2.0, 2.0), ParamDef::range("y", 1.2, -4.0, 4.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [x, y]
            let r = a.sumsq().sqrt();
            let angle = if a.x < -SMALL_EPSILON || a.x > SMALL_EPSILON || a.y < -SMALL_EPSILON || a.y > SMALL_EPSILON {
                a.atan_xy()
            } else {
                0.0
            };
            let dy = d[1];
            let dx = M_PI * (d[0] * d[0]) + SMALL_EPSILON;
            let dx2 = dx * 0.5;
            let t = angle + dy - (((angle + dy) / dx) as i32 as f64) * dx;
            let ang = if t > dx2 { angle - dx2 } else { angle + dx2 };
            v.x += amount * r * ang.sin();
            v.y += amount * r * ang.cos();
            ctx.pass_z(a, v, amount);
        },
        prepare: None,
        wgsl: "let r = sqrt(sumsq(a));
         let angle = select(0.0, atan_xy(a), a.x < -EPS || a.x > EPS || a.y < -EPS || a.y > EPS);
         let dy = D(1);
         let dx = PI * (D(0) * D(0)) + EPS;
         let dx2 = dx * 0.5;
         let t = angle + dy - trunc((angle + dy) / dx) * dx;
         let ang = select(angle + dx2, angle - dx2, t > dx2);
         v.x += AMOUNT * r * sin(ang);
         v.y += AMOUNT * r * cos(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    VarDef {
        name: "rings2",
        params: &[ParamDef::range("val", 0.01, -1.0, 1.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [val, dx]
            let l = a.rad();
            if d[1] == 0.0 || l == 0.0 {
                return;
            }
            let r = amount * (2.0 - d[1] * ((((l / d[1] + 1.0) / 2.0) as i32 as f64) * 2.0 / l + 1.0));
            v.x += r * a.x;
            v.y += r * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _: &Affine| vec![p[0], p[0] * p[0] + EPSILON]),
        wgsl: "let l = rad(a);
         if (D(1) != 0.0 && l != 0.0) {
           let r = AMOUNT * (2.0 - D(1) * (trunc((l / D(1) + 1.0) / 2.0) * 2.0 / l + 1.0));
           v.x += r * a.x;
           v.y += r * a.y;
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    },
    simple(
        "eyefish",
        false,
        |_, ctx, a, v, amount| {
            let r = 2.0 * amount / (a.sumsq().sqrt() + 1.0);
            v.x += r * a.x;
            v.y += r * a.y;
            ctx.pass_z(a, v, amount);
        },
        "let r = 2.0 * AMOUNT / (sqrt(sumsq(a)) + 1.0);
         v.x += r * a.x;
         v.y += r * a.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "bubble",
        true,
        |_, _, a, v, amount| {
            let r = a.sumsq() / 4.0 + 1.0;
            let t = amount / r;
            v.x += t * a.x;
            v.y += t * a.y;
            v.z += amount * (2.0 / r - 1.0);
        },
        "let r = sumsq(a) / 4.0 + 1.0;
         let t = AMOUNT / r;
         v.x += t * a.x;
         v.y += t * a.y;
         v.z += AMOUNT * (2.0 / r - 1.0);",
    ),
    simple(
        "cylinder",
        false,
        |_, ctx, a, v, amount| {
            v.x += amount * a.x.sin();
            v.y += amount * a.y;
            ctx.pass_z(a, v, amount);
        },
        "v.x += AMOUNT * sin(a.x);
         v.y += AMOUNT * a.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "perspective",
        params: &[ParamDef::range("angle", 0.62, -2.0, 2.0), ParamDef::range("dist", 2.2, 0.0, 10.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [angle, dist, vsin, vfcos]
            let dd = d[1] - a.y * d[2];
            if dd == 0.0 {
                return;
            }
            let t = 1.0 / dd;
            v.x += amount * d[1] * a.x * t;
            v.y += amount * d[3] * a.y * t;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _: &Affine| {
            let ang = p[0] * M_PI / 2.0;
            vec![p[0], p[1], ang.sin(), p[1] * ang.cos()]
        }),
        wgsl: "let dd = D(1) - a.y * D(2);
         if (dd != 0.0) {
           let t = 1.0 / dd;
           v.x += AMOUNT * D(1) * a.x * t;
           v.y += AMOUNT * D(3) * a.y * t;
           if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
         }",
    },
    simple(
        "noise",
        false,
        |_, ctx, a, v, amount| {
            let (sinr, cosr) = (ctx.random() * 2.0 * M_PI).sin_cos();
            let r = amount * ctx.random();
            v.x += a.x * r * cosr;
            v.y += a.y * r * sinr;
            ctx.pass_z(a, v, amount);
        },
        "let ang = rnd() * TWO_PI;
         let r = AMOUNT * rnd();
         v.x += a.x * r * cos(ang);
         v.y += a.y * r * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "juliascope",
        params: &[ParamDef::int("power", 2.0, -12.0, 12.0), ParamDef::range("dist", 1.0, -4.0, 4.0)],
        kind: VarKind::Normal,
        is_3d: false,
        apply: |d, ctx, a, v, amount| {
            // d = [power, dist, abs_power, c_power]
            let rnd = ctx.random_int(d[2] as usize);
            let t = a.atan_yx();
            let ang = if rnd & 1 == 0 { (M_2PI * rnd as f64 + t) / d[0] } else { (M_2PI * rnd as f64 - t) / d[0] };
            let (sina, cosa) = ang.sin_cos();
            let r = amount * a.sumsq().powf(d[3]);
            v.x += r * cosa;
            v.y += r * sina;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _: &Affine| {
            let power = if p[0].round() == 0.0 { 1.0 } else { p[0].round() };
            vec![power, p[1], power.abs(), p[1] / power * 0.5]
        }),
        wgsl: "let rn = u32(rnd() * D(2));
         let t = atan_yx(a);
         let ang = (TWO_PI * f32(rn) + select(-t, t, (rn & 1u) == 0u)) / D(0);
         let r = AMOUNT * pow(sumsq(a), D(3));
         v.x += r * cos(ang);
         v.y += r * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
    simple(
        "blur",
        false,
        |_, ctx, a, v, amount| {
            let (sina, cosa) = (ctx.random() * (M_PI + M_PI)).sin_cos();
            let r2 = amount * ctx.random();
            v.x += r2 * cosa;
            v.y += r2 * sina;
            ctx.pass_z(a, v, amount);
        },
        "let ang = rnd() * TWO_PI;
         let r2 = AMOUNT * rnd();
         v.x += r2 * cos(ang);
         v.y += r2 * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    simple(
        "gaussian_blur",
        false,
        |_, ctx, a, v, amount| {
            let (sina, cosa) = (ctx.random() * 2.0 * M_PI).sin_cos();
            let r = amount * (ctx.random() + ctx.random() + ctx.random() + ctx.random() - 2.0);
            v.x += r * cosa;
            v.y += r * sina;
            ctx.pass_z(a, v, amount);
        },
        "let ang = rnd() * TWO_PI;
         let r = AMOUNT * (rnd() + rnd() + rnd() + rnd() - 2.0);
         v.x += r * cos(ang);
         v.y += r * sin(ang);
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    ),
    VarDef {
        name: "radial_blur",
        params: &[ParamDef::range("angle", 0.5, -2.0, 2.0)],
        kind: VarKind::Normal,
        is_3d: false,
        // JWildfire keeps a ring buffer of 4 random numbers per instance; here the gaussian
        // value is drawn fresh from 4 uniform randoms (same distribution).
        apply: |d, ctx, a, v, amount| {
            // d = [angle, sin(angle*pi/2), cos(angle*pi/2)]
            let rnd_g = ctx.random() + ctx.random() + ctx.random() + ctx.random() - 2.0;
            let spin = amount * d[1];
            let zoom = amount * d[2];
            let ra = a.sumsq().sqrt();
            let alpha = a.atan_yx() + spin * rnd_g;
            let (sina, cosa) = alpha.sin_cos();
            let rz = zoom * rnd_g - 1.0;
            v.x += ra * cosa + rz * a.x;
            v.y += ra * sina + rz * a.y;
            ctx.pass_z(a, v, amount);
        },
        prepare: Some(|p, _: &Affine| {
            let ang = p[0] * M_PI * 0.5;
            vec![p[0], ang.sin(), ang.cos()]
        }),
        wgsl: "let rnd_g = rnd() + rnd() + rnd() + rnd() - 2.0;
         let spin = AMOUNT * D(1);
         let zoom = AMOUNT * D(2);
         let ra = sqrt(sumsq(a));
         let alpha = atan_yx(a) + spin * rnd_g;
         let rz = zoom * rnd_g - 1.0;
         v.x += ra * cos(alpha) + rz * a.x;
         v.y += ra * sin(alpha) + rz * a.y;
         if (PRESERVE_Z) { v.z += AMOUNT * a.z; }",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] = &[
        "horseshoe", "polar", "handkerchief", "heart", "disc", "spiral", "hyperbolic", "diamond", "ex", "bent",
        "waves", "fisheye", "exponential", "power", "cosine", "rings", "fan", "blob", "pdj", "fan2", "rings2",
        "eyefish", "bubble", "cylinder", "perspective", "noise", "juliascope", "blur", "gaussian_blur", "radial_blur",
    ];

    #[test]
    fn all_names_present() {
        assert_eq!(DEFS.len(), NAMES.len());
        for n in NAMES {
            assert!(DEFS.iter().any(|d| d.name == *n), "missing {n}");
        }
    }

    #[test]
    fn finite_on_sample_points() {
        let coefs = [
            Affine::default(),
            Affine { c00: 0.8, c01: -0.3, c10: 0.4, c11: 0.9, c20: 0.35, c21: -0.6 },
        ];
        let pts = [(0.3, -0.7, 0.2), (1.5, 2.0, -0.5), (-0.1, 0.05, 0.0), (-2.2, -0.4, 1.0)];
        for def in DEFS {
            for c in &coefs {
                let data = def.prepare_data(&[], c);
                for preserve_z in [false, true] {
                    let mut ctx = Ctx::new(42, preserve_z);
                    for &(x, y, z) in &pts {
                        for _ in 0..8 {
                            let mut a = Pt { x, y, z, ..Default::default() };
                            let mut v = Pt::default();
                            (def.apply)(&data, &mut ctx, &mut a, &mut v, 0.75);
                            assert!(!a.is_bad() && !v.is_bad(), "{}: ({x},{y},{z}) -> {:?}", def.name, v);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn spot_values() {
        let find = |n: &str| DEFS.iter().find(|d| d.name == n).unwrap();
        let run = |n: &str, x: f64, y: f64| {
            let def = find(n);
            let data = def.prepare_data(&[], &Affine::default());
            let mut ctx = Ctx::new(1, false);
            let mut a = Pt { x, y, ..Default::default() };
            let mut v = Pt::default();
            (def.apply)(&data, &mut ctx, &mut a, &mut v, 1.0);
            (v.x, v.y)
        };
        // bent: negative x doubled, negative y halved
        let (x, y) = run("bent", -1.0, -1.0);
        assert!((x + 2.0).abs() < 1e-12 && (y + 0.5).abs() < 1e-12);
        // eyefish at (3,4): r = 2/(5+1)
        let (x, y) = run("eyefish", 3.0, 4.0);
        assert!((x - 1.0).abs() < 1e-12 && (y - 4.0 / 3.0).abs() < 1e-12);
        // juliascope with power 2 maps onto a circle of radius sqrt(r)
        let (x, y) = run("juliascope", 0.0, 4.0);
        assert!(((x * x + y * y).sqrt() - 2.0).abs() < 1e-12);
    }
}
