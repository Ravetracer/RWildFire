//! Variation registry.
//!
//! Every variation is described by a [`VarDef`]: its parameters, a CPU implementation and an
//! optional WGSL snippet for the GPU renderer. Both implementations operate on the same
//! prepared data array (`d`), produced by [`VarDef::prepare_data`] from the user parameters
//! (plus optional precalculated values), so the GPU can receive identical numbers.
//!
//! WGSL snippet conventions (see `rwf-gpu` for the generated environment):
//! * `a` / `v` are `var` points of type `Pt { x, y, z, color }` (affine input / variation output)
//! * `AMOUNT` is the variation amount, `D(k)` reads prepared data value `k`
//! * `PRESERVE_Z` is a bool constant (flame flag); 2D variations end with
//!   `if (PRESERVE_Z) { v.z += AMOUNT * a.z; }`
//! * helpers: `rnd()`, `sumsq(a)`, `rad(a)` (sqrt + epsilon), `atan_xy(a)` (=atan2(x,y)),
//!   `atan_yx(a)` (=atan2(y,x)), constants `PI`, `TWO_PI`, `HALF_PI`, `EPS`

use crate::model::Affine;
use crate::rng::Rng;

pub mod wgsl;
mod wgsl_libs;

// Shared helpers for variation ports (see `PORTING.md`).
pub mod complex;
pub mod glsl;
pub mod noise;

mod basic;
mod batch_a;
mod batch_b;
mod batch_c;
mod batch_d;
// Batches of the full port; each is behind its own feature while being ported.
#[cfg(feature = "w01")]
mod batch_w01;
#[cfg(feature = "w02")]
mod batch_w02;
#[cfg(feature = "w03")]
mod batch_w03;
#[cfg(feature = "w04")]
mod batch_w04;
#[cfg(feature = "w05")]
mod batch_w05;
#[cfg(feature = "w06")]
mod batch_w06;
#[cfg(feature = "w07")]
mod batch_w07;
#[cfg(feature = "w08")]
mod batch_w08;
#[cfg(feature = "w09")]
mod batch_w09;
#[cfg(feature = "w10")]
mod batch_w10;
#[cfg(feature = "w11")]
mod batch_w11;
#[cfg(feature = "w12")]
mod batch_w12;
#[cfg(feature = "w13")]
mod batch_w13;
#[cfg(feature = "w14")]
mod batch_w14;
#[cfg(feature = "w15")]
mod batch_w15;
#[cfg(feature = "w16")]
mod batch_w16;
#[cfg(feature = "w17")]
mod batch_w17;
#[cfg(feature = "w18")]
mod batch_w18;
#[cfg(feature = "w19")]
mod batch_w19;
#[cfg(feature = "w20")]
mod batch_w20;
#[cfg(feature = "w21")]
mod batch_w21;
#[cfg(feature = "w22")]
mod batch_w22;
#[cfg(feature = "w23")]
mod batch_w23;
#[cfg(feature = "w24")]
mod batch_w24;
#[cfg(feature = "w25")]
mod batch_w25;

/// Point flowing through the transform chain (`XYZPoint`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub color: f64,
    pub mod_gamma: f64,
    pub mod_contrast: f64,
    pub mod_saturation: f64,
    pub mod_hue: f64,
    pub rgb_color: bool,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub do_hide: bool,
    /// Solid rendering: material index (interpolated between materials).
    pub material: f64,
    /// Solid rendering: the point only receives shadows (e.g. `plane_wf` ground planes).
    pub receive_only_shadows: bool,
}

impl Pt {
    #[inline]
    pub fn sumsq(&self) -> f64 {
        self.x * self.x + self.y * self.y
    }
    /// `sqrt(x²+y²) + SMALL_EPSILON` (JWildfire `getPrecalcSqrt`).
    #[inline]
    pub fn rad(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt() + crate::math::SMALL_EPSILON
    }
    /// `atan2(x, y)` (JWildfire `getPrecalcAtan`).
    #[inline]
    pub fn atan_xy(&self) -> f64 {
        self.x.atan2(self.y)
    }
    /// `atan2(y, x)` (JWildfire `getPrecalcAtanYX`).
    #[inline]
    pub fn atan_yx(&self) -> f64 {
        self.y.atan2(self.x)
    }
    #[inline]
    pub fn sin_a(&self) -> f64 {
        self.x / self.rad()
    }
    #[inline]
    pub fn cos_a(&self) -> f64 {
        self.y / self.rad()
    }
    #[inline]
    pub fn is_bad(&self) -> bool {
        !(self.x.is_finite() && self.y.is_finite() && self.z.is_finite())
    }
}

/// Per-thread iteration context handed to every variation.
pub struct Ctx {
    pub rng: Rng,
    /// Flame flag `preserve_z`: 2D variations pass the z coordinate through when set.
    pub preserve_z: bool,
    /// Gradient (0..255 RGB) of the layer currently being iterated.
    pub palette: Vec<[u8; 3]>,
}

impl Ctx {
    pub fn new(seed: u64, preserve_z: bool) -> Self {
        Self { rng: Rng::new(seed), preserve_z, palette: vec![[0, 0, 0]; 256] }
    }
    /// Gradient color `i` (clamped to 0..255) as 0..255 floats (JWildfire `RGBPalette.getColor`).
    #[inline]
    pub fn palette_rgb(&self, i: i32) -> [f64; 3] {
        let c = self.palette[i.clamp(0, self.palette.len() as i32 - 1) as usize];
        [c[0] as f64, c[1] as f64, c[2] as f64]
    }
    /// Uniform random number in [0, 1).
    #[inline]
    pub fn random(&mut self) -> f64 {
        self.rng.random()
    }
    #[inline]
    pub fn random_int(&mut self, n: usize) -> usize {
        self.rng.random_int(n)
    }
    /// The common 2D tail: `if preserve_z { v.z += amount * a.z }`.
    #[inline]
    pub fn pass_z(&self, a: &Pt, v: &mut Pt, amount: f64) {
        if self.preserve_z {
            v.z += amount * a.z;
        }
    }
}

/// CPU implementation: `d` = prepared data, `a` = affine-transformed input, `v` = accumulator.
pub type ApplyFn = fn(d: &[f64], ctx: &mut Ctx, a: &mut Pt, v: &mut Pt, amount: f64);
/// Builds the prepared data array from user parameters and the owning xform's affine coefs.
pub type PrepareFn = fn(params: &[f64], coefs: &Affine) -> Vec<f64>;

/// Default execution slot of a variation function (JWildfire `getPriority`).
#[derive(Clone, Copy)]
pub enum VarKind {
    Normal,
    /// Runs before the normal variations and modifies the affine point in place.
    Pre,
    /// Runs after the normal variations and modifies the accumulated point in place.
    Post,
    /// JWildfire priority 2: the inverse (`invtransform`) runs in the pre slot, the forward
    /// transform in the post slot. The inverse is a separate, unregistered `VarDef` that shares
    /// the parameters and `prepare` function of the forward one.
    PrePost(&'static VarDef),
}

impl PartialEq for VarKind {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Eq for VarKind {}

impl std::fmt::Debug for VarKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VarKind::Normal => f.write_str("Normal"),
            VarKind::Pre => f.write_str("Pre"),
            VarKind::Post => f.write_str("Post"),
            VarKind::PrePost(inv) => write!(f, "PrePost({})", inv.name),
        }
    }
}

impl VarKind {
    pub fn default_priority(&self) -> i32 {
        match self {
            VarKind::Normal => 0,
            VarKind::Pre => -1,
            VarKind::Post => 1,
            VarKind::PrePost(_) => 2,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ParamDef {
    pub name: &'static str,
    pub default: f64,
    /// Suggested UI range.
    pub min: f64,
    pub max: f64,
    pub integer: bool,
}

impl ParamDef {
    pub const fn new(name: &'static str, default: f64) -> Self {
        Self { name, default, min: -2.0, max: 2.0, integer: false }
    }
    pub const fn range(name: &'static str, default: f64, min: f64, max: f64) -> Self {
        Self { name, default, min, max, integer: false }
    }
    pub const fn int(name: &'static str, default: f64, min: f64, max: f64) -> Self {
        Self { name, default, min, max, integer: true }
    }
}

pub struct VarDef {
    pub name: &'static str,
    pub params: &'static [ParamDef],
    pub kind: VarKind,
    pub is_3d: bool,
    pub apply: ApplyFn,
    pub prepare: Option<PrepareFn>,
    /// WGSL snippet, empty if the variation has no GPU implementation yet.
    pub wgsl: &'static str,
}

impl VarDef {
    pub fn prepare_data(&self, params: &[f64], coefs: &Affine) -> Vec<f64> {
        // Make sure every declared parameter has a value.
        let mut p: Vec<f64> = self.params.iter().map(|d| d.default).collect();
        for (i, v) in params.iter().enumerate().take(p.len()) {
            p[i] = *v;
        }
        match self.prepare {
            Some(f) => f(&p, coefs),
            None => p,
        }
    }

    pub fn supports_gpu(&self) -> bool {
        !self.wgsl.is_empty()
    }
}

/// Convenience constructor for parameterless variations.
pub const fn simple(name: &'static str, is_3d: bool, apply: ApplyFn, wgsl: &'static str) -> VarDef {
    VarDef { name, params: &[], kind: VarKind::Normal, is_3d, apply, prepare: None, wgsl }
}

use std::collections::HashMap;
use std::sync::OnceLock;

struct Registry {
    defs: Vec<&'static VarDef>,
    by_name: HashMap<String, usize>,
}

fn registry() -> &'static Registry {
    static REG: OnceLock<Registry> = OnceLock::new();
    REG.get_or_init(|| {
        let mut defs: Vec<&'static VarDef> = Vec::new();
        #[allow(unused_mut)]
        let mut lists: Vec<&'static [VarDef]> = vec![basic::DEFS, batch_a::DEFS, batch_b::DEFS, batch_c::DEFS, batch_d::DEFS];
        #[cfg(feature = "w01")]
        lists.push(batch_w01::DEFS);
        #[cfg(feature = "w02")]
        lists.push(batch_w02::DEFS);
        #[cfg(feature = "w03")]
        lists.push(batch_w03::DEFS);
        #[cfg(feature = "w04")]
        lists.push(batch_w04::DEFS);
        #[cfg(feature = "w05")]
        lists.push(batch_w05::DEFS);
        #[cfg(feature = "w06")]
        lists.push(batch_w06::DEFS);
        #[cfg(feature = "w07")]
        lists.push(batch_w07::DEFS);
        #[cfg(feature = "w08")]
        lists.push(batch_w08::DEFS);
        #[cfg(feature = "w09")]
        lists.push(batch_w09::DEFS);
        #[cfg(feature = "w10")]
        lists.push(batch_w10::DEFS);
        #[cfg(feature = "w11")]
        lists.push(batch_w11::DEFS);
        #[cfg(feature = "w12")]
        lists.push(batch_w12::DEFS);
        #[cfg(feature = "w13")]
        lists.push(batch_w13::DEFS);
        #[cfg(feature = "w14")]
        lists.push(batch_w14::DEFS);
        #[cfg(feature = "w15")]
        lists.push(batch_w15::DEFS);
        #[cfg(feature = "w16")]
        lists.push(batch_w16::DEFS);
        #[cfg(feature = "w17")]
        lists.push(batch_w17::DEFS);
        #[cfg(feature = "w18")]
        lists.push(batch_w18::DEFS);
        #[cfg(feature = "w19")]
        lists.push(batch_w19::DEFS);
        #[cfg(feature = "w20")]
        lists.push(batch_w20::DEFS);
        #[cfg(feature = "w21")]
        lists.push(batch_w21::DEFS);
        #[cfg(feature = "w22")]
        lists.push(batch_w22::DEFS);
        #[cfg(feature = "w23")]
        lists.push(batch_w23::DEFS);
        #[cfg(feature = "w24")]
        lists.push(batch_w24::DEFS);
        #[cfg(feature = "w25")]
        lists.push(batch_w25::DEFS);
        for list in lists {
            defs.extend(list.iter());
        }
        defs.sort_by(|a, b| a.name.cmp(b.name));
        defs.dedup_by(|a, b| a.name == b.name);
        let mut by_name = HashMap::new();
        for (i, d) in defs.iter().enumerate() {
            by_name.insert(d.name.to_string(), i);
        }
        // Aliases for names used by flam3/Apophysis and older JWildfire versions.
        for (alias, target) in ALIASES {
            if let Some(&i) = by_name.get(*target) {
                by_name.entry(alias.to_string()).or_insert(i);
            }
        }
        Registry { defs, by_name }
    })
}

const ALIASES: &[(&str, &str)] = &[
    ("Spherical3D", "spherical3D"),
    ("SphericalN", "sphericaln"),
    ("Epispiral", "epispiral"),
    ("cross2", "cross"),
];

pub fn find(name: &str) -> Option<&'static VarDef> {
    let reg = registry();
    reg.by_name.get(name).map(|&i| reg.defs[i])
}

pub fn all() -> &'static [&'static VarDef] {
    &registry().defs
}

pub fn names() -> impl Iterator<Item = &'static str> {
    all().iter().map(|d| d.name)
}
