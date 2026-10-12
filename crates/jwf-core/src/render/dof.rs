//! Depth of field (port of `FlameRendererView` DOF + `render.dof.*BlurShape`).
//! Most bokeh shapes delegate to a variation, exactly like JWildfire does.

use crate::math::{EPSILON, M_PI};
use crate::model::{Affine, Flame};
use crate::variations::{self, Ctx, Pt, VarDef};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DofShape {
    #[default]
    Bubble,
    BrushStroke,
    CannabisCurve,
    Cloverleaf,
    Flower,
    PerlinNoise,
    Rect,
    SineBlur,
    StarBlur,
    Taurus,
    NBlur,
    Heart,
    Snowflake,
    SubFlame,
}

/// How the shape feeds the variation (see the individual Java shape classes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShapeMode {
    /// `s = d = src; d += f(s, dr)`; rotate around src.
    Offset,
    /// Like `Offset`, without rotation (sine blur).
    OffsetNoRotate,
    /// Rect: offset, then x scaled by `width`.
    Rect,
    /// `s` = random point in [-1,1]² (times fade for some shapes); `d = src + f(s, dr)`.
    RandomSource { scale_by_fade: bool, dr_factor: f64, refade: bool },
    /// Taurus: `s` = random point in [-2π, 2π]² * fade, dr * 0.25.
    Taurus,
}

impl DofShape {
    pub const ALL: &'static [DofShape] = &[
        DofShape::Bubble,
        DofShape::Rect,
        DofShape::Flower,
        DofShape::Heart,
        DofShape::NBlur,
        DofShape::StarBlur,
        DofShape::SineBlur,
        DofShape::Taurus,
        DofShape::Cloverleaf,
        DofShape::CannabisCurve,
        DofShape::Snowflake,
        DofShape::PerlinNoise,
        DofShape::BrushStroke,
        DofShape::SubFlame,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            DofShape::Bubble => "BUBBLE",
            DofShape::BrushStroke => "BRUSH_STROKE",
            DofShape::CannabisCurve => "CANNABISCURVE",
            DofShape::Cloverleaf => "CLOVERLEAF",
            DofShape::Flower => "FLOWER",
            DofShape::PerlinNoise => "PERLIN_NOISE",
            DofShape::Rect => "RECT",
            DofShape::SineBlur => "SINEBLUR",
            DofShape::StarBlur => "STARBLUR",
            DofShape::Taurus => "TAURUS",
            DofShape::NBlur => "NBLUR",
            DofShape::Heart => "HEART",
            DofShape::Snowflake => "SNOWFLAKE",
            DofShape::SubFlame => "SUB_FLAME",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            DofShape::Bubble => "Bubble",
            DofShape::BrushStroke => "Brush stroke",
            DofShape::CannabisCurve => "Cannabis curve",
            DofShape::Cloverleaf => "Cloverleaf",
            DofShape::Flower => "Flower",
            DofShape::PerlinNoise => "Perlin noise",
            DofShape::Rect => "Rectangle",
            DofShape::SineBlur => "Sine blur",
            DofShape::StarBlur => "Star",
            DofShape::Taurus => "Taurus",
            DofShape::NBlur => "N-gon",
            DofShape::Heart => "Heart",
            DofShape::Snowflake => "Snowflake",
            DofShape::SubFlame => "Sub flame",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.as_str() == s)
    }

    /// (variation name, mode, shape parameter names -> variation parameter names).
    pub fn variation(&self) -> Option<(&'static str, ShapeMode, &'static [&'static str])> {
        Some(match self {
            DofShape::Bubble | DofShape::BrushStroke | DofShape::SubFlame => return None,
            DofShape::Rect => ("square", ShapeMode::Rect, &["width"]),
            DofShape::Heart => ("xheart_blur_wf", ShapeMode::Offset, &["angle", "ratio"]),
            DofShape::Flower => ("flower", ShapeMode::RandomSource { scale_by_fade: false, dr_factor: 2.0, refade: false }, &["holes", "petals"]),
            DofShape::NBlur => ("nBlur", ShapeMode::Offset, &["numEdges", "numStripes", "ratioStripes", "ratioHole", "circumCircle"]),
            DofShape::StarBlur => ("starblur", ShapeMode::Offset, &["power", "range"]),
            DofShape::SineBlur => ("sineblur", ShapeMode::OffsetNoRotate, &["power"]),
            DofShape::Taurus => ("taurus", ShapeMode::Taurus, &["r", "n", "inv", "sor"]),
            DofShape::Cloverleaf => ("cloverleaf_wf", ShapeMode::RandomSource { scale_by_fade: true, dr_factor: 1.0, refade: true }, &[]),
            DofShape::CannabisCurve => ("cannabiscurve_wf", ShapeMode::RandomSource { scale_by_fade: true, dr_factor: 1.0, refade: false }, &[]),
            DofShape::Snowflake => ("snowflake_wf", ShapeMode::Offset, &["max_iter", "bg_freeze_level", "fg_freeze_speed", "diffusion_speed", "rnd_bg_noise", "seed"]),
            DofShape::PerlinNoise => ("dc_perlin", ShapeMode::Offset, &["shape", "amps", "freqs", "centre", "range"]),
        })
    }

    /// Shape parameter labels shown in the UI (param1..param6).
    pub fn param_names(&self) -> &'static [&'static str] {
        self.variation().map(|v| v.2).unwrap_or(&[])
    }

    /// JWildfire `setDefaultParams`: (fade, scale, angle, params).
    pub fn defaults(&self) -> (f64, f64, f64, [f64; 6]) {
        match self {
            DofShape::Bubble => (1.0, 1.0, 0.0, [0.0; 6]),
            DofShape::Rect => (0.0, 1.0, 0.0, [0.67, 0.0, 0.0, 0.0, 0.0, 0.0]),
            DofShape::Heart => (0.0, 1.0, 0.0, [0.0; 6]),
            DofShape::Flower => (0.0, 1.0, 30.0, [0.4, 7.0, 0.0, 0.0, 0.0, 0.0]),
            DofShape::NBlur => (0.0, 1.0, 0.0, [3.0, 0.0, 1.0, 0.0, 0.0, 0.0]),
            DofShape::StarBlur => (0.0, 1.0, 0.0, [5.0, 0.401_622_831_772_454_6, 0.0, 0.0, 0.0, 0.0]),
            DofShape::SineBlur => (0.0, 1.0, 0.0, [1.4, 0.0, 0.0, 0.0, 0.0, 0.0]),
            DofShape::Taurus => (0.0, 1.0, 30.0, [3.0, 5.0, 1.5, 1.0, 0.0, 0.0]),
            DofShape::Snowflake => (0.0, 1.0, 0.0, [500.0, 0.5, 0.0005, 0.01, 0.25, 333.0]),
            DofShape::PerlinNoise => (0.0, 1.0, 30.0, [0.0, 0.2, 2.0, 0.25, 0.5, 0.0]),
            _ => (0.0, 1.0, 0.0, [0.0; 6]),
        }
    }
}

/// Prepared DOF state for one render.
#[derive(Clone)]
pub struct Dof {
    pub legacy: bool,
    pub cam_dof_10: f64,
    pub scale: f64,
    pub fade: f64,
    pub rotate: Option<(f64, f64)>,
    pub cam_z: f64,
    pub focus: [f64; 3],
    pub area: f64,
    pub fade_dist: f64,
    pub area_minus_fade: f64,
    pub exponent: f64,
    pub width: f64,
    pub mode: Option<ShapeMode>,
    pub var: Option<(&'static VarDef, Vec<f64>)>,
}

fn smootherstep(e0: f64, e1: f64, x: f64) -> f64 {
    let x = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

impl Dof {
    pub fn new(flame: &Flame) -> Option<Self> {
        if flame.cam_dof.abs() <= EPSILON {
            return None;
        }
        let shape = flame.cam_dof_shape;
        let p = [flame.cam_dof_param1, flame.cam_dof_param2, flame.cam_dof_param3, flame.cam_dof_param4, flame.cam_dof_param5, flame.cam_dof_param6];
        let (mode, var, width) = match shape.variation() {
            Some((name, mode, pnames)) => match variations::find(name) {
                Some(def) => {
                    let mut inst = crate::model::VariationInstance::new(name, 1.0);
                    let mut width = 1.0;
                    for (i, pn) in pnames.iter().enumerate() {
                        if mode == ShapeMode::Rect && *pn == "width" {
                            width = p[i];
                        } else {
                            inst.set_param(pn, p[i]);
                        }
                    }
                    // Filled variants and fixed buffer size, as in the Java shapes.
                    inst.set_param("filled", 1.0);
                    inst.set_param("buffer_size", 256.0);
                    let data = def.prepare_data(&inst.params, &Affine::IDENTITY);
                    (Some(mode), Some((def, data)), width)
                }
                // Shape variation not available yet: fall back to bubble.
                None => (None, None, 1.0),
            },
            None => (None, None, 1.0),
        };
        let rot = flame.cam_dof_angle.to_radians();
        Some(Self {
            legacy: !flame.new_cam_dof,
            cam_dof_10: 0.1 * flame.cam_dof,
            scale: flame.cam_dof_scale,
            fade: flame.cam_dof_fade.clamp(0.0, 1.0),
            rotate: if flame.cam_dof_angle.abs() > EPSILON { Some(rot.sin_cos()) } else { None },
            cam_z: flame.cam_z,
            focus: [flame.focus_x, flame.focus_y, flame.focus_z],
            area: flame.cam_dof_area,
            fade_dist: flame.cam_dof_area / 2.25,
            area_minus_fade: flame.cam_dof_area - flame.cam_dof_area / 2.25,
            exponent: flame.cam_dof_exponent,
            width,
            mode,
            var,
        })
    }

    fn do_fade(&self, ctx: &mut Ctx) -> f64 {
        if self.fade <= EPSILON {
            1.0
        } else if self.fade >= 1.0 - EPSILON {
            ctx.random()
        } else if ctx.random() <= self.fade {
            ctx.random()
        } else {
            1.0
        }
    }

    /// Blur distance for a camera-space point, `None` if the point is in focus.
    pub fn distance(&self, cx: f64, cy: f64, cz: f64) -> Option<f64> {
        if self.legacy {
            let zdist = self.cam_z - cz;
            (zdist > 0.0).then_some(zdist)
        } else {
            let (dx, dy, dz) = (cx - self.focus[0], cy - self.focus[1], cz - self.focus[2]);
            let dist = (dx * dx + dy * dy + dz * dz).powf(1.0 / self.exponent);
            if dist > self.area {
                Some(dist)
            } else if dist > self.area_minus_fade {
                Some(smootherstep(0.0, 1.0, (dist - self.area_minus_fade) / self.fade_dist) * dist)
            } else {
                None
            }
        }
    }

    /// Applies the bokeh shape to camera-space `(x, y)` for blur distance `zdist`.
    pub fn apply(&self, ctx: &mut Ctx, x: f64, y: f64, zdist: f64) -> (f64, f64) {
        let (Some(mode), Some((def, data))) = (self.mode, &self.var) else {
            // Bubble
            let dr = self.do_fade(ctx) * self.cam_dof_10 * zdist * self.scale;
            let a = 2.0 * M_PI * ctx.random();
            return (x + dr * a.cos(), y + dr * a.sin());
        };
        let src = Pt { x, y, ..Pt::default() };
        let mut d = src;
        let rotate_around;
        match mode {
            ShapeMode::Offset | ShapeMode::OffsetNoRotate | ShapeMode::Rect => {
                let dr = self.cam_dof_10 * zdist * self.scale * self.do_fade(ctx);
                let dr = if matches!(def.name, "xheart_blur_wf") { dr.abs() } else { dr };
                let mut s = src;
                (def.apply)(data, ctx, &mut s, &mut d, dr);
                if mode == ShapeMode::Rect {
                    d.x = s.x + (d.x - s.x) * self.width;
                }
                rotate_around = if mode == ShapeMode::OffsetNoRotate { None } else { Some(s) };
            }
            ShapeMode::RandomSource { scale_by_fade, dr_factor, refade } => {
                let fade = self.do_fade(ctx);
                let dr_fade = if refade { self.do_fade(ctx) } else { fade };
                let dr = (self.cam_dof_10 * zdist * self.scale * dr_fade).abs() * dr_factor;
                let f = if scale_by_fade { fade } else { 1.0 };
                let mut s = Pt { x: (1.0 - 2.0 * ctx.random()) * f, y: (1.0 - 2.0 * ctx.random()) * f, ..Pt::default() };
                (def.apply)(data, ctx, &mut s, &mut d, dr);
                rotate_around = Some(src);
            }
            ShapeMode::Taurus => {
                let fade = self.do_fade(ctx);
                let dr = (self.cam_dof_10 * zdist * self.scale * fade).abs() * 0.25;
                let mut s = Pt {
                    x: (2.0 * M_PI - 4.0 * ctx.random() * M_PI) * fade,
                    y: (2.0 * M_PI - 4.0 * ctx.random() * M_PI) * fade,
                    ..Pt::default()
                };
                (def.apply)(data, ctx, &mut s, &mut d, dr);
                rotate_around = Some(src);
            }
        }
        if let (Some((sina, cosa)), Some(o)) = (self.rotate, rotate_around) {
            let (dx, dy) = (d.x - o.x, d.y - o.y);
            d.x = o.x + dx * cosa - dy * sina;
            d.y = o.y + dx * sina + dy * cosa;
        }
        (d.x, d.y)
    }
}

impl std::fmt::Debug for Dof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dof").field("legacy", &self.legacy).field("mode", &self.mode).field("variation", &self.var.as_ref().map(|v| v.0.name)).finish()
    }
}
