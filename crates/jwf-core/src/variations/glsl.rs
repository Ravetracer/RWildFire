//! Port of JWildfire's GLSL emulation (`js.glsl.{G, vec2, vec3, vec4, mat2, mat3, mat4}`).
//!
//! Vectors and matrices are small `Copy` value types on `f64`; the Java methods map to operators:
//!
//! | Java                                   | Rust                                  |
//! |----------------------------------------|---------------------------------------|
//! | `new vec2(x, y)` / `new vec2(s)`       | `vec2(x, y)` / `Vec2::splat(s)`       |
//! | `new vec3(v2, z)` / `new vec4(v3, w)`  | `Vec3::from_vec2(v2, z)` / `Vec4::from_vec3(v3, w)` |
//! | `a.plus(b)` / `a.add(b)`, `a.minus(b)` | `a + b`, `a - b` (vector or `f64`)    |
//! | `a.multiply(b)`, `a.division(b)`       | `a * b`, `a / b` (component-wise or `f64`) |
//! | `m.times(v)` (mat * vec)               | `m * v`                               |
//! | `v.times(m)` (vec * mat)               | `v * m`                               |
//! | `m.add(f)/minus(f)/times(f)/division(f)` | `m + f`, `m - f`, `m * f`, `m / f`  |
//!
//! `G.*` functions are free functions here. Functions whose name and semantics match a WGSL
//! builtin keep the GLSL name and are generic over [`GenType`] (`f64`, [`Vec2`], [`Vec3`],
//! [`Vec4`]): `length dot distance normalize cross reflect refract abs sign floor ceil fract
//! trunc sqrt sin cos exp exp2 log2 pow min max clamp mix step smoothstep sinh cosh tanh`.
//! Scalar arguments are splatted where Java has a `(vecN, double)` overload, e.g.
//! `G.max(v, 0.0)` -> `max(v, 0.0)`, `G.step(0.5, v)` -> `step(0.5, v)`.
//!
//! Everything else carries the `glsl_` prefix (G's `c_*` complex helpers keep their `c_`
//! prefix), and the WGSL library `wgsl/glsl.wgsl` / `wgsl/glsl_complex.wgsl` uses exactly the
//! same names. Notable quirks reproduced from Java:
//! * [`glsl_atan2`] is a fast *approximation* of `atan2` (`G.atan2`), [`glsl_atan`] is
//!   `atan(n / d)` (no quadrant correction). `G.carg`, `G.arg` and `c_to_polar` (and therefore
//!   `c_ln`, `c_sqrt`, `c_pow`, ...) use [`glsl_atan`].
//! * [`glsl_round`] is `Math.round` (ties towards +inf), [`trunc`] is an `(int)` cast.
//! * `Vec4 * Mat4` and the scalar ops of [`Mat4`] reproduce the index/transposition bugs of the
//!   Java `vec4.times(mat4)` / `mat4.add(...)` etc.
//! * Java `vec3`/`vec4` also carry separate `r,g,b(,a)` fields that are only set by
//!   constructors. Here [`Vec3::r`] etc. simply alias `x,y,z,w`; see `PORTING.md`.

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::math::{java_max, java_min};
use crate::variations::{Ctx, Pt};

// ---------------------------------------------------------------------------------------------
// Vector types
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec4 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

/// `new vec2(x, y)`
#[inline]
pub const fn vec2(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

/// `new vec3(x, y, z)`
#[inline]
pub const fn vec3(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3 { x, y, z }
}

/// `new vec4(x, y, z, w)`
#[inline]
pub const fn vec4(x: f64, y: f64, z: f64, w: f64) -> Vec4 {
    Vec4 { x, y, z, w }
}

impl Vec2 {
    #[inline]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    /// `new vec2(s)`
    #[inline]
    pub const fn splat(s: f64) -> Self {
        Self { x: s, y: s }
    }
    #[inline]
    pub const fn yx(self) -> Vec2 {
        vec2(self.y, self.x)
    }
    /// `new vec3(this, z)`
    #[inline]
    pub const fn extend(self, z: f64) -> Vec3 {
        vec3(self.x, self.y, z)
    }
    #[inline]
    pub fn length(self) -> f64 {
        length(self)
    }
    #[inline]
    pub fn dot(self, o: Vec2) -> f64 {
        dot(self, o)
    }
}

impl Vec3 {
    #[inline]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
    /// `new vec3(s)`
    #[inline]
    pub const fn splat(s: f64) -> Self {
        Self { x: s, y: s, z: s }
    }
    /// `new vec3(v, z)`
    #[inline]
    pub const fn from_vec2(v: Vec2, z: f64) -> Self {
        Self { x: v.x, y: v.y, z }
    }
    /// `new vec4(this, w)`
    #[inline]
    pub const fn extend(self, w: f64) -> Vec4 {
        vec4(self.x, self.y, self.z, w)
    }
    #[inline]
    pub const fn r(self) -> f64 {
        self.x
    }
    #[inline]
    pub const fn g(self) -> f64 {
        self.y
    }
    #[inline]
    pub const fn b(self) -> f64 {
        self.z
    }
    #[inline]
    pub const fn xy(self) -> Vec2 {
        vec2(self.x, self.y)
    }
    #[inline]
    pub const fn xz(self) -> Vec2 {
        vec2(self.x, self.z)
    }
    #[inline]
    pub const fn yz(self) -> Vec2 {
        vec2(self.y, self.z)
    }
    #[inline]
    pub const fn yx(self) -> Vec2 {
        vec2(self.y, self.x)
    }
    #[inline]
    pub const fn zx(self) -> Vec2 {
        vec2(self.z, self.x)
    }
    #[inline]
    pub const fn zy(self) -> Vec2 {
        vec2(self.z, self.y)
    }
    #[inline]
    pub const fn xzy(self) -> Vec3 {
        vec3(self.x, self.z, self.y)
    }
    #[inline]
    pub const fn yxz(self) -> Vec3 {
        vec3(self.y, self.x, self.z)
    }
    #[inline]
    pub const fn yzx(self) -> Vec3 {
        vec3(self.y, self.z, self.x)
    }
    #[inline]
    pub const fn zxy(self) -> Vec3 {
        vec3(self.z, self.x, self.y)
    }
    #[inline]
    pub const fn zyx(self) -> Vec3 {
        vec3(self.z, self.y, self.x)
    }
    #[inline]
    pub fn length(self) -> f64 {
        length(self)
    }
    #[inline]
    pub fn dot(self, o: Vec3) -> f64 {
        dot(self, o)
    }
}

impl Vec4 {
    #[inline]
    pub const fn new(x: f64, y: f64, z: f64, w: f64) -> Self {
        Self { x, y, z, w }
    }
    /// `new vec4(s)`
    #[inline]
    pub const fn splat(s: f64) -> Self {
        Self { x: s, y: s, z: s, w: s }
    }
    /// `new vec4(v, z, w)`
    #[inline]
    pub const fn from_vec2(v: Vec2, z: f64, w: f64) -> Self {
        Self { x: v.x, y: v.y, z, w }
    }
    /// `new vec4(a, b)`
    #[inline]
    pub const fn from_vec2s(a: Vec2, b: Vec2) -> Self {
        Self { x: a.x, y: a.y, z: b.x, w: b.y }
    }
    /// `new vec4(v, w)`
    #[inline]
    pub const fn from_vec3(v: Vec3, w: f64) -> Self {
        Self { x: v.x, y: v.y, z: v.z, w }
    }
    #[inline]
    pub const fn r(self) -> f64 {
        self.x
    }
    #[inline]
    pub const fn g(self) -> f64 {
        self.y
    }
    #[inline]
    pub const fn b(self) -> f64 {
        self.z
    }
    #[inline]
    pub const fn a(self) -> f64 {
        self.w
    }
    #[inline]
    pub const fn xy(self) -> Vec2 {
        vec2(self.x, self.y)
    }
    #[inline]
    pub const fn zw(self) -> Vec2 {
        vec2(self.z, self.w)
    }
    #[inline]
    pub const fn xz(self) -> Vec2 {
        vec2(self.x, self.z)
    }
    #[inline]
    pub const fn yw(self) -> Vec2 {
        vec2(self.y, self.w)
    }
    #[inline]
    pub const fn xyz(self) -> Vec3 {
        vec3(self.x, self.y, self.z)
    }
    #[inline]
    pub const fn yzw(self) -> Vec3 {
        vec3(self.y, self.z, self.w)
    }
    #[inline]
    pub fn length(self) -> f64 {
        length(self)
    }
    #[inline]
    pub fn dot(self, o: Vec4) -> f64 {
        dot(self, o)
    }
}

/// GLSL `genType`: `f64` and the vector types. Component-wise helpers used by the generic
/// `G.*` functions.
pub trait GenType: Copy {
    fn splat(v: f64) -> Self;
    fn map(self, f: impl Fn(f64) -> f64) -> Self;
    fn zip(self, o: Self, f: impl Fn(f64, f64) -> f64) -> Self;
    fn zip3(self, a: Self, b: Self, f: impl Fn(f64, f64, f64) -> f64) -> Self;
    /// Sum of the component products (in x, y, z, w order, like the Java code).
    fn dot(self, o: Self) -> f64;
}

impl GenType for f64 {
    #[inline]
    fn splat(v: f64) -> Self {
        v
    }
    #[inline]
    fn map(self, f: impl Fn(f64) -> f64) -> Self {
        f(self)
    }
    #[inline]
    fn zip(self, o: Self, f: impl Fn(f64, f64) -> f64) -> Self {
        f(self, o)
    }
    #[inline]
    fn zip3(self, a: Self, b: Self, f: impl Fn(f64, f64, f64) -> f64) -> Self {
        f(self, a, b)
    }
    #[inline]
    fn dot(self, o: Self) -> f64 {
        self * o
    }
}

macro_rules! vec_impl {
    ($V:ident { $first:ident $(, $f:ident)* }) => {
        impl GenType for $V {
            #[inline]
            fn splat(v: f64) -> Self {
                $V::splat(v)
            }
            #[inline]
            fn map(self, f: impl Fn(f64) -> f64) -> Self {
                $V { $first: f(self.$first) $(, $f: f(self.$f))* }
            }
            #[inline]
            fn zip(self, o: Self, f: impl Fn(f64, f64) -> f64) -> Self {
                $V { $first: f(self.$first, o.$first) $(, $f: f(self.$f, o.$f))* }
            }
            #[inline]
            fn zip3(self, a: Self, b: Self, f: impl Fn(f64, f64, f64) -> f64) -> Self {
                $V { $first: f(self.$first, a.$first, b.$first) $(, $f: f(self.$f, a.$f, b.$f))* }
            }
            #[inline]
            fn dot(self, o: Self) -> f64 {
                self.$first * o.$first $(+ self.$f * o.$f)*
            }
        }

        impl From<f64> for $V {
            #[inline]
            fn from(v: f64) -> Self {
                $V::splat(v)
            }
        }

        impl Neg for $V {
            type Output = $V;
            #[inline]
            fn neg(self) -> $V {
                $V { $first: -self.$first $(, $f: -self.$f)* }
            }
        }

        vec_impl!(@op $V, Add, add, AddAssign, add_assign, +, $first $(, $f)*);
        vec_impl!(@op $V, Sub, sub, SubAssign, sub_assign, -, $first $(, $f)*);
        vec_impl!(@op $V, Mul, mul, MulAssign, mul_assign, *, $first $(, $f)*);
        vec_impl!(@op $V, Div, div, DivAssign, div_assign, /, $first $(, $f)*);
    };
    (@op $V:ident, $Tr:ident, $m:ident, $TrA:ident, $ma:ident, $op:tt, $($f:ident),+) => {
        impl $Tr for $V {
            type Output = $V;
            #[inline]
            fn $m(self, o: $V) -> $V {
                $V { $($f: self.$f $op o.$f),+ }
            }
        }
        impl $Tr<f64> for $V {
            type Output = $V;
            #[inline]
            fn $m(self, o: f64) -> $V {
                $V { $($f: self.$f $op o),+ }
            }
        }
        impl $Tr<$V> for f64 {
            type Output = $V;
            #[inline]
            fn $m(self, o: $V) -> $V {
                $V { $($f: self $op o.$f),+ }
            }
        }
        impl $TrA for $V {
            #[inline]
            fn $ma(&mut self, o: $V) {
                *self = *self $op o;
            }
        }
        impl $TrA<f64> for $V {
            #[inline]
            fn $ma(&mut self, o: f64) {
                *self = *self $op o;
            }
        }
    };
}

vec_impl!(Vec2 { x, y });
vec_impl!(Vec3 { x, y, z });
vec_impl!(Vec4 { x, y, z, w });

// ---------------------------------------------------------------------------------------------
// Matrix types (column-major like GLSL/Java: `aRC` = row R, column C)
// ---------------------------------------------------------------------------------------------

/// `mat2`; constructor arguments are column-major: `mat2(a00, a10, a01, a11)`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mat2 {
    pub a00: f64,
    pub a10: f64,
    pub a01: f64,
    pub a11: f64,
}

/// `mat3`; constructor arguments are column-major.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mat3 {
    pub a00: f64,
    pub a10: f64,
    pub a20: f64,
    pub a01: f64,
    pub a11: f64,
    pub a21: f64,
    pub a02: f64,
    pub a12: f64,
    pub a22: f64,
}

/// `mat4`; constructor arguments are column-major.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mat4 {
    pub a00: f64,
    pub a10: f64,
    pub a20: f64,
    pub a30: f64,
    pub a01: f64,
    pub a11: f64,
    pub a21: f64,
    pub a31: f64,
    pub a02: f64,
    pub a12: f64,
    pub a22: f64,
    pub a32: f64,
    pub a03: f64,
    pub a13: f64,
    pub a23: f64,
    pub a33: f64,
}

/// `new mat2(a00, a10, a01, a11)` (column-major, same argument order as Java and WGSL).
#[inline]
pub const fn mat2(a00: f64, a10: f64, a01: f64, a11: f64) -> Mat2 {
    Mat2 { a00, a10, a01, a11 }
}

/// `new mat3(a00, a10, a20, a01, a11, a21, a02, a12, a22)` (column-major).
#[allow(clippy::too_many_arguments)]
#[inline]
pub const fn mat3(a00: f64, a10: f64, a20: f64, a01: f64, a11: f64, a21: f64, a02: f64, a12: f64, a22: f64) -> Mat3 {
    Mat3 { a00, a10, a20, a01, a11, a21, a02, a12, a22 }
}

/// `new mat4(a00, a10, a20, a30, a01, ..., a33)` (column-major).
#[allow(clippy::too_many_arguments)]
#[inline]
pub const fn mat4(
    a00: f64,
    a10: f64,
    a20: f64,
    a30: f64,
    a01: f64,
    a11: f64,
    a21: f64,
    a31: f64,
    a02: f64,
    a12: f64,
    a22: f64,
    a32: f64,
    a03: f64,
    a13: f64,
    a23: f64,
    a33: f64,
) -> Mat4 {
    Mat4 { a00, a10, a20, a30, a01, a11, a21, a31, a02, a12, a22, a32, a03, a13, a23, a33 }
}

impl Mat2 {
    /// `new mat2(c0, c1)` (columns).
    #[inline]
    pub const fn from_cols(c0: Vec2, c1: Vec2) -> Self {
        mat2(c0.x, c0.y, c1.x, c1.y)
    }
    /// `new mat2(vec4)` = `mat2(v.x, v.y, v.z, v.w)`.
    #[inline]
    pub const fn from_vec4(v: Vec4) -> Self {
        mat2(v.x, v.y, v.z, v.w)
    }
    fn map(self, f: impl Fn(f64) -> f64) -> Self {
        mat2(f(self.a00), f(self.a10), f(self.a01), f(self.a11))
    }
}

impl Mat3 {
    /// `new mat3(c0, c1, c2)` (columns).
    #[inline]
    pub const fn from_cols(c0: Vec3, c1: Vec3, c2: Vec3) -> Self {
        mat3(c0.x, c0.y, c0.z, c1.x, c1.y, c1.z, c2.x, c2.y, c2.z)
    }
    fn map(self, f: impl Fn(f64) -> f64) -> Self {
        let m = self;
        mat3(f(m.a00), f(m.a10), f(m.a20), f(m.a01), f(m.a11), f(m.a21), f(m.a02), f(m.a12), f(m.a22))
    }
}

impl Mat4 {
    /// `new mat4(c0, c1, c2, c3)` (columns).
    #[inline]
    pub const fn from_cols(c0: Vec4, c1: Vec4, c2: Vec4, c3: Vec4) -> Self {
        mat4(c0.x, c0.y, c0.z, c0.w, c1.x, c1.y, c1.z, c1.w, c2.x, c2.y, c2.z, c2.w, c3.x, c3.y, c3.z, c3.w)
    }
    /// The Java scalar ops (`add`, `minus`, `times(double)`, `division`) pass the elements to the
    /// constructor in row-major order, so their result is *transposed*. Reproduced here.
    fn map_transposed(self, f: impl Fn(f64) -> f64) -> Self {
        let m = self;
        mat4(
            f(m.a00),
            f(m.a01),
            f(m.a02),
            f(m.a03),
            f(m.a10),
            f(m.a11),
            f(m.a12),
            f(m.a13),
            f(m.a20),
            f(m.a21),
            f(m.a22),
            f(m.a23),
            f(m.a30),
            f(m.a31),
            f(m.a32),
            f(m.a33),
        )
    }
}

/// `mat2.times(vec2)`: M * v.
impl Mul<Vec2> for Mat2 {
    type Output = Vec2;
    #[inline]
    fn mul(self, v: Vec2) -> Vec2 {
        vec2(self.a00 * v.x + self.a01 * v.y, self.a10 * v.x + self.a11 * v.y)
    }
}

/// `vec2.times(mat2)`: v * M.
impl Mul<Mat2> for Vec2 {
    type Output = Vec2;
    #[inline]
    fn mul(self, m: Mat2) -> Vec2 {
        vec2(m.a00 * self.x + m.a10 * self.y, self.x * m.a01 + self.y * m.a11)
    }
}

/// Matrix product (not in the Java API, provided for GLSL code written with `*`).
impl Mul for Mat2 {
    type Output = Mat2;
    #[inline]
    fn mul(self, o: Mat2) -> Mat2 {
        Mat2::from_cols(self * vec2(o.a00, o.a10), self * vec2(o.a01, o.a11))
    }
}

/// `mat3.times(vec3)`: M * v.
impl Mul<Vec3> for Mat3 {
    type Output = Vec3;
    #[inline]
    fn mul(self, v: Vec3) -> Vec3 {
        let m = self;
        vec3(
            m.a00 * v.x + m.a01 * v.y + m.a02 * v.z,
            m.a10 * v.x + m.a11 * v.y + m.a12 * v.z,
            m.a20 * v.x + m.a21 * v.y + m.a22 * v.z,
        )
    }
}

/// `vec3.times(mat3)`: v * M.
impl Mul<Mat3> for Vec3 {
    type Output = Vec3;
    #[inline]
    fn mul(self, m: Mat3) -> Vec3 {
        let v = self;
        vec3(
            v.x * m.a00 + v.y * m.a10 + v.z * m.a20,
            v.x * m.a01 + v.y * m.a11 + v.z * m.a21,
            v.x * m.a02 + v.y * m.a12 + v.z * m.a22,
        )
    }
}

/// Matrix product (not in the Java API).
impl Mul for Mat3 {
    type Output = Mat3;
    #[inline]
    fn mul(self, o: Mat3) -> Mat3 {
        Mat3::from_cols(self * vec3(o.a00, o.a10, o.a20), self * vec3(o.a01, o.a11, o.a21), self * vec3(o.a02, o.a12, o.a22))
    }
}

/// `mat4.times(vec4)`: M * v.
impl Mul<Vec4> for Mat4 {
    type Output = Vec4;
    #[inline]
    fn mul(self, v: Vec4) -> Vec4 {
        let m = self;
        vec4(
            m.a00 * v.x + m.a01 * v.y + m.a02 * v.z + m.a03 * v.w,
            m.a10 * v.x + m.a11 * v.y + m.a12 * v.z + m.a13 * v.w,
            m.a20 * v.x + m.a21 * v.y + m.a22 * v.z + m.a23 * v.w,
            m.a30 * v.x + m.a31 * v.y + m.a32 * v.z + m.a33 * v.w,
        )
    }
}

/// `vec4.times(mat4)`, reproducing the mixed-up indices of the Java implementation
/// (WGSL: `glsl_vec4_times_mat4`).
impl Mul<Mat4> for Vec4 {
    type Output = Vec4;
    #[inline]
    fn mul(self, m: Mat4) -> Vec4 {
        let v = self;
        vec4(
            v.x * m.a00 + m.a01 * v.y + v.z * m.a02 + v.w * m.a03,
            v.x * m.a01 + m.a11 * v.y + v.z * m.a12 + v.w * m.a13,
            v.x * m.a02 + m.a21 * v.y + v.z * m.a22 + v.w * m.a23,
            v.x * m.a03 + m.a31 * v.y + v.z * m.a32 + v.w * m.a33,
        )
    }
}

macro_rules! mat_scalar_ops {
    ($M:ident, $map:ident) => {
        impl Add<f64> for $M {
            type Output = $M;
            #[inline]
            fn add(self, f: f64) -> $M {
                self.$map(|a| a + f)
            }
        }
        impl Sub<f64> for $M {
            type Output = $M;
            #[inline]
            fn sub(self, f: f64) -> $M {
                self.$map(|a| a - f)
            }
        }
        impl Mul<f64> for $M {
            type Output = $M;
            #[inline]
            fn mul(self, f: f64) -> $M {
                self.$map(|a| a * f)
            }
        }
        impl Div<f64> for $M {
            type Output = $M;
            #[inline]
            fn div(self, f: f64) -> $M {
                self.$map(|a| a / f)
            }
        }
    };
}

mat_scalar_ops!(Mat2, map);
mat_scalar_ops!(Mat3, map);
mat_scalar_ops!(Mat4, map_transposed);

// ---------------------------------------------------------------------------------------------
// G: functions with WGSL builtin counterparts (generic over GenType)
// ---------------------------------------------------------------------------------------------

/// `G.length`
#[inline]
pub fn length<T: GenType>(v: T) -> f64 {
    v.dot(v).sqrt()
}

/// `G.dot`
#[inline]
pub fn dot<T: GenType>(a: T, b: T) -> f64 {
    a.dot(b)
}

/// `G.distance`
#[inline]
pub fn distance<T: GenType + Sub<Output = T>>(a: T, b: T) -> f64 {
    length(a - b)
}

/// `G.normalize` (no zero check, like Java).
#[inline]
pub fn normalize<T: GenType>(v: T) -> T {
    let l = length(v);
    v.map(|c| c / l)
}

/// `G.cross`
#[inline]
pub fn cross(x: Vec3, y: Vec3) -> Vec3 {
    vec3(x.y * y.z - y.y * x.z, x.z * y.x - y.z * x.x, x.x * y.y - y.x * x.y)
}

/// `G.Reflect(incident, normal)`: `I - N * 2 * dot(I, N)`.
#[inline]
pub fn reflect<T: GenType + Sub<Output = T> + Mul<f64, Output = T>>(i: T, n: T) -> T {
    i - n * (2.0 * dot(i, n))
}

/// `G.refract(I, N, eta)`
#[inline]
pub fn refract<T: GenType + Sub<Output = T> + Mul<f64, Output = T>>(i: T, n: T, eta: f64) -> T {
    let k = 1.0 - eta * eta * (1.0 - dot(n, i) * dot(n, i));
    if k < 0.0 { T::splat(0.0) } else { i * eta - n * (eta * dot(n, i) + k.sqrt()) }
}

/// `G.abs` (`v >= 0 ? v : -v`)
#[inline]
pub fn abs<T: GenType>(v: T) -> T {
    v.map(|c| if c >= 0.0 { c } else { -c })
}

/// `G.sign` (`Math.signum`: keeps +-0.0 and NaN)
#[inline]
pub fn sign<T: GenType>(v: T) -> T {
    v.map(|c| {
        if c > 0.0 {
            1.0
        } else if c < 0.0 {
            -1.0
        } else {
            c
        }
    })
}

/// `G.floor`
#[inline]
pub fn floor<T: GenType>(v: T) -> T {
    v.map(f64::floor)
}

/// `G.ceil`
#[inline]
pub fn ceil<T: GenType>(v: T) -> T {
    v.map(f64::ceil)
}

/// `G.fract`: `x - floor(x)`
#[inline]
pub fn fract<T: GenType>(v: T) -> T {
    v.map(|c| c - c.floor())
}

/// `G.trunc`: `(double)(int)x` (saturates at the `int` range, NaN -> 0).
#[inline]
pub fn trunc<T: GenType>(v: T) -> T {
    v.map(|c| c as i32 as f64)
}

/// `G.round`: `Math.round` (ties towards +inf). WGSL's `round` rounds ties to even, hence the
/// prefix.
#[inline]
pub fn glsl_round<T: GenType>(v: T) -> T {
    v.map(crate::math::java_round)
}

/// `G.sqrt`
#[inline]
pub fn sqrt<T: GenType>(v: T) -> T {
    v.map(f64::sqrt)
}

/// `G.sin`
#[inline]
pub fn sin<T: GenType>(v: T) -> T {
    v.map(f64::sin)
}

/// `G.cos`
#[inline]
pub fn cos<T: GenType>(v: T) -> T {
    v.map(f64::cos)
}

/// `G.exp`
#[inline]
pub fn exp<T: GenType>(v: T) -> T {
    v.map(f64::exp)
}

/// `G.exp2`: `pow(2, x)`
#[inline]
pub fn exp2<T: GenType>(v: T) -> T {
    v.map(|c| 2f64.powf(c))
}

/// `G.log2`: `log(x) / log(2)`
#[inline]
pub fn log2<T: GenType>(v: T) -> T {
    v.map(|c| c.ln() / 2f64.ln())
}

/// `G.pow` (component-wise; a scalar exponent is splatted)
#[inline]
pub fn pow<T: GenType, U: Into<T>>(x: T, y: U) -> T {
    x.zip(y.into(), f64::powf)
}

/// `G.min`: `(y < x) ? y : x` (a scalar `y` is splatted)
#[inline]
pub fn min<T: GenType, U: Into<T>>(x: T, y: U) -> T {
    x.zip(y.into(), |a, b| if b < a { b } else { a })
}

/// `G.max`: `(x < y) ? y : x` (a scalar `y` is splatted)
#[inline]
pub fn max<T: GenType, U: Into<T>>(x: T, y: U) -> T {
    x.zip(y.into(), |a, b| if a < b { b } else { a })
}

/// `G.clamp`: `Math.min(Math.max(x, lo), hi)`
#[inline]
pub fn clamp<T: GenType, B: Into<T>>(x: T, lo: B, hi: B) -> T {
    x.zip3(lo.into(), hi.into(), |c, l, h| java_min(java_max(c, l), h))
}

/// `G.mix`: `x * (1 - a) + y * a` (a scalar `a` is splatted)
#[inline]
pub fn mix<T: GenType, A: Into<T>>(x: T, y: T, a: A) -> T {
    x.zip3(y, a.into(), |x, y, a| x * (1.0 - a) + y * a)
}

/// `G.step(edge, x)`: `x < edge ? 0 : 1` (a scalar edge is splatted)
#[inline]
pub fn step<T: GenType, E: Into<T>>(edge: E, x: T) -> T {
    x.zip(edge.into(), |x, e| if x < e { 0.0 } else { 1.0 })
}

#[inline]
fn smoothstep1(e0: f64, e1: f64, x: f64) -> f64 {
    let t = java_min(java_max((x - e0) / (e1 - e0), 0.0), 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `G.smoothstep(e0, e1, x)` (scalar edges are splatted)
#[inline]
pub fn smoothstep<T: GenType, E: Into<T>>(e0: E, e1: E, x: T) -> T {
    x.zip3(e0.into(), e1.into(), |x, a, b| smoothstep1(a, b, x))
}

/// `G.sinh(double)`: `(e^x - e^-x) / 2` via `exp`.
#[inline]
pub fn sinh(val: f64) -> f64 {
    let tmp = val.exp();
    (tmp - 1.0 / tmp) / 2.0
}

/// `G.cosh(double)`
#[inline]
pub fn cosh(val: f64) -> f64 {
    let tmp = val.exp();
    (tmp + 1.0 / tmp) / 2.0
}

/// `G.tanh(double)`
#[inline]
pub fn tanh(val: f64) -> f64 {
    let tmp = val.exp();
    (tmp - 1.0 / tmp) / (tmp + 1.0 / tmp)
}

// ---------------------------------------------------------------------------------------------
// G: functions without (identical) WGSL builtin, `glsl_` prefix in Rust and WGSL
// ---------------------------------------------------------------------------------------------

/// `G.mod`: GLSL `mod`, `x - y * floor(x / y)` (a scalar `y` is splatted).
/// WGSL: `glsl_mod` (f32), `glsl_mod2/3/4` (vectors).
#[inline]
pub fn glsl_mod<T: GenType, U: Into<T>>(x: T, y: U) -> T {
    x.zip(y.into(), |x, y| x - y * (x / y).floor())
}

/// `G.atan(n, d)`: `atan(n / d)` -- *not* a quadrant-correct `atan2`.
#[inline]
pub fn glsl_atan(n: f64, d: f64) -> f64 {
    (n / d).atan()
}

/// `G.atan2(y, x)`: JWildfire's fast piecewise *approximation* of `atan2` (max. error ~0.07).
/// Use `y.atan2(x)` for `Math.atan2` / `MathLib.atan2`.
#[inline]
pub fn glsl_atan2(y: f64, x: f64) -> f64 {
    let coeff_1 = std::f64::consts::PI / 4.0;
    let coeff_2 = 3.0 * coeff_1;
    let abs_y = y.abs();
    let angle = if x >= 0.0 {
        let r = (x - abs_y) / (x + abs_y);
        coeff_1 - coeff_1 * r
    } else {
        let r = (x + abs_y) / (abs_y - x);
        coeff_2 - coeff_1 * r
    };
    if y < 0.0 { -angle } else { angle }
}

/// `G.invSqrt(double)`: the "fast inverse square root" bit hack (one Newton step).
#[inline]
pub fn glsl_inv_sqrt(x: f64) -> f64 {
    let xhalf = 0.5 * x;
    let i = 0x5fe6_ec85_e7de_30da_i64 - ((x.to_bits() as i64) >> 1);
    let x = f64::from_bits(i as u64);
    x * (1.5 - xhalf * x * x)
}

/// `G.invSqrt(float)`
#[inline]
pub fn glsl_inv_sqrt_f32(x: f32) -> f32 {
    let xhalf = 0.5 * x;
    let i = 0x5f37_59df_i32 - ((x.to_bits() as i32) >> 1);
    let x = f32::from_bits(i as u32);
    x * (1.5 - xhalf * x * x)
}

/// `G.sinh(vec2)`: component-wise `exp`, then complex `(ep - 1/ep) / 2`.
#[inline]
pub fn glsl_sinh2(val: Vec2) -> Vec2 {
    let ep = exp(val);
    let t = c_sub(ep, c_div(c_one(), ep));
    vec2(t.x / 2.0, t.y / 2.0)
}

/// `G.cosh(vec2)`: component-wise `exp`, then complex `(ep + 1/ep) / 2`.
#[inline]
pub fn glsl_cosh2(val: Vec2) -> Vec2 {
    let ep = exp(val);
    let t = c_add(ep, c_div(c_one(), ep));
    vec2(t.x / 2.0, t.y / 2.0)
}

/// `G.tanh(vec2)`
#[inline]
pub fn glsl_tanh2(val: Vec2) -> Vec2 {
    let ep = exp(val);
    let t1 = c_sub(ep, c_div(c_one(), ep));
    let t2 = c_add(ep, c_div(c_one(), ep));
    c_div(t1, t2)
}

/// `G.plot(st, pct)`
pub fn glsl_plot(st: Vec2, pct: f64) -> f64 {
    smoothstep(pct - 0.02, pct, st.y) - smoothstep(pct, pct + 0.02, st.y)
}

/// `G.circle(st, radius)`
pub fn glsl_circle(st: Vec2, radius: f64) -> f64 {
    let dist = st - Vec2::splat(0.5);
    1.0 - smoothstep(radius - radius * 0.01, radius + radius * 0.01, dot(dist, dist) * 4.0)
}

/// `G.rotate2D(st, angle)`: rotation around (0.5, 0.5).
pub fn glsl_rotate2d(st: Vec2, angle: f64) -> Vec2 {
    let tmp = st - Vec2::splat(0.5);
    let r = mat2(angle.cos(), -angle.sin(), angle.sin(), angle.cos()) * tmp;
    r + Vec2::splat(0.5)
}

/// `G.tile(st, zoom)`
pub fn glsl_tile(st: Vec2, zoom: f64) -> Vec2 {
    fract(st * zoom)
}

/// `G.brickTile(st, zoom)`
pub fn glsl_brick_tile(st: Vec2, zoom: f64) -> Vec2 {
    let mut tmp = st * zoom;
    tmp.x += step(1.0, glsl_mod(tmp.y, 2.0)) * 0.5;
    fract(tmp)
}

/// `G.truchetPattern(st, index)`
pub fn glsl_truchet_pattern(st: Vec2, index: f64) -> Vec2 {
    let index = fract((index - 0.5) * 2.0);
    if index > 0.75 {
        Vec2::splat(1.0) - st
    } else if index > 0.5 {
        vec2(1.0 - st.x, st.y)
    } else if index > 0.25 {
        Vec2::splat(1.0) - vec2(1.0 - st.x, st.y)
    } else {
        st
    }
}

/// `G.box(st, size, smoothEdges)`
pub fn glsl_box(st: Vec2, size: Vec2, smooth_edges: f64) -> f64 {
    let size = Vec2::splat(0.5) - size * 0.5;
    let aa = Vec2::splat(smooth_edges * 0.5);
    let mut uv = smoothstep(size, size + aa, st);
    uv *= smoothstep(size, size + aa, Vec2::splat(1.0) - st);
    uv.x * uv.y
}

/// `G.skew(st)`
pub fn glsl_skew(st: Vec2) -> Vec2 {
    let x = 1.1547 * st.x;
    vec2(x, st.y + 0.5 * x)
}

/// `G.simplexGrid(st)`
pub fn glsl_simplex_grid(st: Vec2) -> Vec3 {
    let p = fract(glsl_skew(st));
    let xyz = if p.x > p.y {
        let tmp = Vec2::splat(1.0) - vec2(p.x, p.y - p.x);
        vec3(tmp.x, tmp.y, p.y)
    } else {
        let tmp = Vec2::splat(1.0) - vec2(p.x - p.y, p.y);
        vec3(p.x, tmp.x, tmp.y)
    };
    fract(xyz)
}

/// `G.random(st)`: `fract(sin(dot(st, (12.9898, 78.233))) * 43758.5453123)`
#[inline]
pub fn glsl_random(st: Vec2) -> f64 {
    fract(dot(st, vec2(12.9898, 78.233)).sin() * 43758.5453123)
}

/// `G.random2(p)`
#[inline]
pub fn glsl_random2(p: Vec2) -> Vec2 {
    fract(sin(vec2(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)))) * 43758.5453)
}

/// `G.fractal(p)`: Mandelbrot escape-time color.
pub fn glsl_fractal(p: Vec2) -> Vec3 {
    let mut z = Vec2::splat(0.0);
    for i in 0..50 {
        z = vec2(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + p;
        if dot(z, z) > 4.0 {
            let s = 0.125662 * i as f64;
            let tmp = vec3((s + 0.9).cos(), (s + 0.3).cos(), (s + 0.2).cos());
            return tmp * 0.4 + 0.6;
        }
    }
    Vec3::splat(0.0)
}

/// `G.B(a)`: `(log(length(a)), glsl_atan2(a.y, a.x) - 6.3)`
pub fn glsl_b(a: Vec2) -> Vec2 {
    vec2(length(a).ln(), glsl_atan2(a.y, a.x) - 6.3)
}

/// `G.F(E, time)` (named `glsl_big_f` to distinguish it from `G.f`).
pub fn glsl_big_f(e: Vec2, time: f64) -> Vec3 {
    let mut e_ = e;
    let mut c = 6.0;
    for _ in 0..30 {
        let tmp = glsl_b(vec2(e_.x, e_.y.abs()));
        e_ = tmp + vec2(0.1 * (time / 3.0).sin() - 0.1, 5.0 + 0.1 * (time / 5.0).cos());
        c += length(e_);
    }
    let d = log2(log2(c * 0.05)) * 6.0;
    vec3(0.7 + (0.7 * d.cos()).tan(), 0.5 + 0.5 * (d - 0.7).cos(), 0.7 + (0.7 * (d - 0.7).cos()).sin())
}

/// `G.hash3(p)` (Inigo Quilez)
#[inline]
pub fn glsl_hash3(p: Vec2) -> Vec3 {
    let q = vec3(dot(p, vec2(127.1, 311.7)), dot(p, vec2(269.5, 183.3)), dot(p, vec2(419.2, 371.9)));
    fract(sin(q) * 43758.5453)
}

/// `G.iqnoise(x, u, v)`: Inigo Quilez' "voronoise".
pub fn glsl_iqnoise(x: Vec2, u: f64, v: f64) -> f64 {
    let p = floor(x);
    let f = fract(x);
    let k = 1.0 + 63.0 * (1.0 - v).powf(4.0);
    let mut va = 0.0;
    let mut wt = 0.0;
    for j in -2..=2 {
        for i in -2..=2 {
            let g = vec2(i as f64, j as f64);
            let o = glsl_hash3(p + g) * vec3(u, u, 1.0);
            let r = g - f + vec2(o.x, o.y);
            let d = dot(r, r);
            let ww = (1.0 - smoothstep(0.0, 1.414, d.sqrt())).powf(k);
            va += o.z * ww;
            wt += ww;
        }
    }
    va / wt
}

/// `G.noise(st)`: 2D value noise based on [`glsl_random`] (range 0..1).
pub fn glsl_noise(st: Vec2) -> f64 {
    let i = floor(st);
    let f = fract(st);
    let a = glsl_random(i);
    let b = glsl_random(i + vec2(1.0, 0.0));
    let c = glsl_random(i + vec2(0.0, 1.0));
    let d = glsl_random(i + vec2(1.0, 1.0));
    let u = f * f * (Vec2::splat(3.0) - Vec2::splat(2.0) * f);
    mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y
}

/// `G.fbm(st)`: 6 octaves of [`glsl_noise`].
pub fn glsl_fbm(st: Vec2) -> f64 {
    let mut st = st;
    let mut value = 0.0;
    let mut amplitude = 0.5;
    for _ in 0..6 {
        value += amplitude * glsl_noise(st);
        st *= 2.0;
        amplitude *= 0.5;
    }
    value
}

/// `G.fbm2(st)`: 5 rotated octaves of [`glsl_noise`].
pub fn glsl_fbm2(st: Vec2) -> f64 {
    let mut st = st;
    let mut v = 0.0;
    let mut a = 0.5;
    let shift = Vec2::splat(100.0);
    let rot = mat2(0.5f64.cos(), 0.5f64.sin(), -(0.5f64.sin()), 0.5f64.cos());
    for _ in 0..5 {
        v += a * glsl_noise(st);
        st = (rot * st) * Vec2::splat(2.0) + shift;
        a *= 0.5;
    }
    v
}

/// `G.rgb2hsb(c)`
pub fn glsl_rgb2hsb(c: Vec3) -> Vec3 {
    let k = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4(c.z, c.y, k.w, k.z), vec4(c.y, c.z, k.x, k.y), step(c.z, c.y));
    let q = mix(vec4(p.x, p.y, p.w, c.x), vec4(c.x, p.y, p.z, p.x), step(p.x, c.x));
    let d = q.x - java_min(q.w, q.y);
    let e = 1.0e-10;
    vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x)
}

/// `G.hsb2rgb(c)` (Inigo Quilez)
pub fn glsl_hsb2rgb(c: Vec3) -> Vec3 {
    let rgb = clamp(abs(glsl_mod(Vec3::splat(c.x * 6.0) + vec3(0.0, 4.0, 2.0), 6.0) - 3.0) - 1.0, 0.0, 1.0);
    let rgb = rgb * rgb * (Vec3::splat(3.0) - Vec3::splat(2.0) * rgb);
    Vec3::splat(c.z) * mix(Vec3::splat(1.0), rgb, c.y)
}

/// `G.rgb2hsv(c)` (alpha passed through)
pub fn glsl_rgb2hsv(c: Vec4) -> Vec4 {
    let k = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4(c.z, c.y, k.w, k.z), vec4(c.y, c.z, k.x, k.y), step(c.z, c.y));
    let q = mix(vec4(p.x, p.y, p.w, c.x), vec4(c.x, p.y, p.z, p.x), step(p.x, c.x));
    let d = q.x - java_min(q.w, q.y);
    let e = 1.0e-10;
    vec4((q.z + (q.w - q.y) / (6.0 * d + e)).abs(), d / (q.x + e), q.x, c.w)
}

/// `G.hsv2rgb(c)` (alpha passed through)
pub fn glsl_hsv2rgb(c: Vec4) -> Vec4 {
    let k = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(Vec3::splat(c.x) + vec3(k.x, k.y, k.z)) * 6.0 - Vec3::splat(k.w));
    Vec4::from_vec3(Vec3::splat(c.z) * mix(Vec3::splat(k.x), clamp(p - Vec3::splat(k.x), 0.0, 1.0), c.y), c.w)
}

/// `G.ccon(z)`: conjugate
#[inline]
pub fn glsl_ccon(z: Vec2) -> Vec2 {
    vec2(z.x, -z.y)
}

/// `G.cmul(u, v)`
#[inline]
pub fn glsl_cmul(u: Vec2, v: Vec2) -> Vec2 {
    vec2(u.x * v.x - u.y * v.y, u.y * v.x + u.x * v.y)
}

/// `G.cdiv(u, v)`
#[inline]
pub fn glsl_cdiv(u: Vec2, v: Vec2) -> Vec2 {
    vec2(u.x * v.x + u.y * v.y, u.y * v.x - u.x * v.y) * (1.0 / (v.x * v.x + v.y * v.y))
}

/// `G.cabs(z)`
#[inline]
pub fn glsl_cabs(z: Vec2) -> f64 {
    length(z)
}

/// `G.carg(z)`: uses [`glsl_atan`] (`atan(y / x)`).
#[inline]
pub fn glsl_carg(z: Vec2) -> f64 {
    glsl_atan(z.y, z.x)
}

/// `G.cpow(b, e)`
pub fn glsl_cpow(b: Vec2, e: Vec2) -> Vec2 {
    let ab = glsl_carg(b);
    let lgb = (b.x * b.x + b.y * b.y).ln() / 2.0;
    let lr = (lgb * e.x - ab * e.y).exp();
    let cis = lgb * e.y + ab * e.x;
    vec2(cis.cos() * lr, cis.sin() * lr)
}

/// `G.cexp(z)`
#[inline]
pub fn glsl_cexp(z: Vec2) -> Vec2 {
    vec2(z.y.cos(), z.y.sin()) * z.x.exp()
}

/// `G.clog(z)`
#[inline]
pub fn glsl_clog(z: Vec2) -> Vec2 {
    vec2((z.x * z.x + z.y * z.y).ln() / 2.0, glsl_carg(z))
}

/// `G.f(p, time)`: the animated complex function of the GLSL samples.
pub fn glsl_f(p: Vec2, time: f64) -> Vec2 {
    let p = Vec2::splat(2.0) * glsl_cpow(p, vec2(3.0, 0.0)) - Vec2::splat(0.1) * p
        + vec2(0.04 + 0.03 * (time * 0.2).sin(), 0.02 * (time * 0.46).cos());
    glsl_cmul(p, glsl_cexp(vec2(0.0, time)))
}

/// `G.Kscope(uv, k)`: kaleidoscope fold (uses the [`glsl_atan2`] approximation).
pub fn glsl_kscope(uv: Vec2, k: f64) -> Vec2 {
    let angle = (glsl_mod(glsl_atan2(uv.y, uv.x), 2.0 * k) - k).abs() + 0.1 * 0.0;
    Vec2::splat(length(uv)) * vec2(angle.cos(), angle.sin())
}

/// `G.colorize(t, a, b, c, d)`
pub fn glsl_colorize(t: f64, a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> Vec3 {
    let t1 = cos(Vec3::splat(0.4 * std::f64::consts::PI) * (c * t + d));
    Vec3::splat(2.5) * a * b * t1
}

/// `G.v(coord, k, s, rot)`
pub fn glsl_v(coord: Vec2, k: f64, s: f64, rot: f64) -> f64 {
    let cx = rot.cos();
    let sy = rot.sin();
    0.0 + 0.5 * ((cx * coord.x + sy * coord.y) * k + s).cos()
}

/// `G.rot(s)`: rotation matrix from three Euler angles.
pub fn glsl_rot(s: Vec3) -> Mat3 {
    let (sa, ca) = (s.x.sin(), s.x.cos());
    let (sb, cb) = (s.y.sin(), s.y.cos());
    let (sc, cc) = (s.z.sin(), s.z.cos());
    Mat3::from_cols(
        vec3(cb * cc, -cb * sc, sb),
        vec3(sa * sb * cc + ca * sc, -sa * sb * sc + ca * cc, -sa * cb),
        vec3(-ca * sb * cc + sa * sc, ca * sb * sc + sa * cc, ca * cb),
    )
}

/// `G.app(v, k, m)`: 50 Kali-style fold iterations.
pub fn glsl_app(v: Vec3, k: f64, m: Mat3) -> Vec3 {
    let mut v = v;
    for _ in 0..50 {
        let m1 = m * k;
        v = abs((m1 * v) / dot(v, v) * 0.5 - 0.5) * 2.0 - 1.0;
    }
    v
}

// ---------------------------------------------------------------------------------------------
// G: complex helpers on vec2 (`c_` prefix in Rust and WGSL)
// ---------------------------------------------------------------------------------------------

/// `G.c_one()`
#[inline]
pub const fn c_one() -> Vec2 {
    vec2(1.0, 0.0)
}

/// `G.c_i()`
#[inline]
pub const fn c_i() -> Vec2 {
    vec2(0.0, 1.0)
}

/// `G.c_ni()`
#[inline]
pub const fn c_ni() -> Vec2 {
    vec2(0.0, -1.0)
}

/// `G.arg(c)`: uses [`glsl_atan`] (`atan(y / x)`).
#[inline]
pub fn glsl_arg(c: Vec2) -> f64 {
    glsl_atan(c.y, c.x)
}

/// `G.c_conj(c)`
#[inline]
pub fn c_conj(c: Vec2) -> Vec2 {
    vec2(c.x, -c.y)
}

/// `G.c_from_polar(r, theta)`
#[inline]
pub fn c_from_polar(r: f64, theta: f64) -> Vec2 {
    vec2(r * theta.cos(), r * theta.sin())
}

/// `G.c_to_polar(c)`: `(length, atan(y / x))` -- note the missing quadrant correction.
#[inline]
pub fn c_to_polar(c: Vec2) -> Vec2 {
    vec2(length(c), glsl_atan(c.y, c.x))
}

/// `G.c_exp(c)`
#[inline]
pub fn c_exp(c: Vec2) -> Vec2 {
    c_from_polar(c.x.exp(), c.y)
}

/// `G.c_exp(double base, vec2 c)`: `base^c`
#[inline]
pub fn c_exp_base(base: f64, c: Vec2) -> Vec2 {
    c_from_polar(base.powf(c.x), c.y * base.ln())
}

/// `G.c_ln(c)`
#[inline]
pub fn c_ln(c: Vec2) -> Vec2 {
    let polar = c_to_polar(c);
    vec2(polar.x.ln(), polar.y)
}

/// `G.c_log(c, base)`
#[inline]
pub fn c_log(c: Vec2, base: f64) -> Vec2 {
    let polar = c_to_polar(c);
    vec2(polar.x.ln() / base.ln(), polar.y / base.ln())
}

/// `G.c_sqrt(c)`
#[inline]
pub fn c_sqrt(c: Vec2) -> Vec2 {
    let p = c_to_polar(c);
    c_from_polar(p.x.sqrt(), p.y / 2.0)
}

/// `G.c_pow(vec2 c, double e)`
#[inline]
pub fn c_pow(c: Vec2, e: f64) -> Vec2 {
    let p = c_to_polar(c);
    c_from_polar(p.x.powf(e), p.y * e)
}

/// `G.c_pow(vec2 c, vec2 e)`: complex exponent.
#[inline]
pub fn c_pow_c(c: Vec2, e: Vec2) -> Vec2 {
    let polar = c_to_polar(c);
    c_from_polar(polar.x.powf(e.x) * (-e.y * polar.y).exp(), e.x * polar.y + e.y * polar.x.ln())
}

/// `G.c_add(a, b)`
#[inline]
pub fn c_add(a: Vec2, b: Vec2) -> Vec2 {
    vec2(a.x + b.x, a.y + b.y)
}

/// `G.c_sub(a, b)`
#[inline]
pub fn c_sub(a: Vec2, b: Vec2) -> Vec2 {
    vec2(a.x - b.x, a.y - b.y)
}

/// `G.c_mul(a, b)`
#[inline]
pub fn c_mul(a: Vec2, b: Vec2) -> Vec2 {
    vec2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x)
}

/// `G.c_div(a, b)` (denominator computed as `length(b)²`, like Java)
#[inline]
pub fn c_div(a: Vec2, b: Vec2) -> Vec2 {
    let norm = length(b);
    vec2((a.x * b.x + a.y * b.y) / (norm * norm), (a.y * b.x - a.x * b.y) / (norm * norm))
}

/// `G.c_sin(c)`
#[inline]
pub fn c_sin(c: Vec2) -> Vec2 {
    vec2(c.x.sin() * cosh(c.y), c.x.cos() * sinh(c.y))
}

/// `G.c_cos(c)`
#[inline]
pub fn c_cos(c: Vec2) -> Vec2 {
    vec2(c.x.cos() * cosh(c.y), -c.x.sin() * sinh(c.y))
}

/// `G.c_tan(c)`
#[inline]
pub fn c_tan(c: Vec2) -> Vec2 {
    let c2 = c * 2.0;
    vec2(c2.x.sin(), sinh(c2.y)) / (c2.x.cos() + cosh(c2.y))
}

/// `G.c_eq(a, b)`
#[inline]
pub fn c_eq(a: Vec2, b: Vec2) -> bool {
    a.x == b.x && a.y == b.y
}

/// `G.c_atan(c)`: `(ln(1+iz) - ln(1-iz)) / 2i`, `+-i` map to `(0, +-inf)`.
pub fn c_atan(c: Vec2) -> Vec2 {
    let i = c_i();
    let one = c_one();
    let two = one + one;
    if c_eq(c, i) {
        return vec2(0.0, f64::INFINITY);
    } else if c_eq(c, c_ni()) {
        return vec2(0.0, f64::NEG_INFINITY);
    }
    c_div(c_sub(c_ln(c_add(one, c_mul(i, c))), c_ln(c_sub(one, c_mul(i, c)))), c_mul(two, i))
}

/// `G.c_asin(c)`: `-i ln(sqrt(1 - z²) + iz)`
pub fn c_asin(c: Vec2) -> Vec2 {
    c_mul(c_ni(), c_ln(c_add(c_sqrt(c_sub(c_one(), c_mul(c, c))), c_mul(c_i(), c))))
}

/// `G.c_acos(c)`: `-i ln(i sqrt(1 - z²) + z)`
pub fn c_acos(c: Vec2) -> Vec2 {
    c_mul(c_ni(), c_ln(c_add(c_mul(c_i(), c_sqrt(c_sub(c_one(), c_mul(c, c)))), c)))
}

/// `G.c_sinh(c)`
#[inline]
pub fn c_sinh(c: Vec2) -> Vec2 {
    vec2(sinh(c.x) * c.y.cos(), cosh(c.x) * c.y.sin())
}

/// `G.c_cosh(c)`
#[inline]
pub fn c_cosh(c: Vec2) -> Vec2 {
    vec2(cosh(c.x) * c.y.cos(), sinh(c.x) * c.y.sin())
}

/// `G.c_tanh(c)`
#[inline]
pub fn c_tanh(c: Vec2) -> Vec2 {
    let c2 = vec2(2.0 * c.x, 2.0 * c.y);
    let d = cosh(c2.x) + c2.y.cos();
    vec2(sinh(c2.x) / d, c2.y.sin() / d)
}

/// `G.c_asinh(c)`: `ln(z + sqrt(1 + z²))`
pub fn c_asinh(c: Vec2) -> Vec2 {
    c_ln(c_add(c, c_sqrt(c_add(c_one(), c_mul(c, c)))))
}

/// `G.c_acosh(c)`: `2 ln(sqrt((z+1)/2) + sqrt((z-1)/2))`
pub fn c_acosh(c: Vec2) -> Vec2 {
    let one = c_one();
    let two = c_add(one, one);
    c_mul(two, c_ln(c_add(c_sqrt(c_div(c_add(c, one), two)), c_sqrt(c_div(c_sub(c, one), two)))))
}

/// `G.c_atanh(c)`: `(ln(1+z) - ln(1-z)) / 2`, `+-1` map to `(+-inf, 0)`.
pub fn c_atanh(c: Vec2) -> Vec2 {
    let one = c_one();
    let two = one + one;
    if c_eq(c, one) {
        return vec2(f64::INFINITY, 0.0);
    } else if c_eq(c, -one) {
        return vec2(f64::NEG_INFINITY, 0.0);
    }
    c_div(c_sub(c_ln(c_add(one, c)), c_ln(c_sub(one, c))), two)
}

/// `G.c_rem(c, modulus)`
pub fn c_rem(c: Vec2, modulus: Vec2) -> Vec2 {
    let c0 = c_div(c, modulus);
    let c1 = vec2(c0.x - glsl_mod(c0.x, 1.0), c0.y - glsl_mod(c0.y, 1.0));
    c - c_mul(modulus, c1)
}

/// `G.c_inv(c)`
#[inline]
pub fn c_inv(c: Vec2) -> Vec2 {
    let norm = length(c);
    vec2(c.x / (norm * norm), -c.y / (norm * norm))
}

// ---------------------------------------------------------------------------------------------
// Base classes of the GLSL variations (`GLSLFunc`, `DC_BaseFunc`)
// ---------------------------------------------------------------------------------------------

/// `GLSLFunc.dbl2int` / `DC_BaseFunc.dbl2int`: 0..1 color -> 0..255 ints
/// (`max(0, min(255, (int) floor(c * 256)))`). WGSL: `glsl_dbl2int` (returns `vec3<f32>`).
#[inline]
pub fn glsl_dbl2int(color: Vec3) -> [i32; 3] {
    let f = |c: f64| ((c * 256.0).floor() as i32).clamp(0, 255);
    [f(color.x), f(color.y), f(color.z)]
}

/// `DC_BaseFunc.greyscale(r, g, b)` (0..1 luminance from 0..255 ints).
#[inline]
pub fn dc_greyscale(r: i32, g: i32, b: i32) -> f64 {
    let lum = (r as f64 * 0.299) as i32 + (g as f64 * 0.587) as i32 + (b as f64 * 0.114) as i32;
    lum as f64 / 255.0
}

/// `DC_BaseFunc.findKey(palette, r, g, b)`: the gradient color closest to (r, g, b).
pub fn dc_find_key(ctx: &Ctx, r: i32, g: i32, b: i32) -> [i32; 3] {
    let mut diff = 1_000_000_000;
    let mut ret = [0, 0, 0];
    for i in 0..256 {
        let c = ctx.palette_rgb(i);
        let (cr, cg, cb) = (c[0] as i32, c[1] as i32, c[2] as i32);
        let d = (r - cr).pow(2) + (g - cg).pow(2) + (b - cb).pow(2);
        if diff > d {
            diff = d;
            ret = [cr, cg, cb];
        }
    }
    ret
}

/// The common tail of `DC_BaseFunc.transform` (everything after `getRGBColor`):
/// sets the true color (`gradient == 0`), the closest gradient color (`gradient == 1`) or
/// `color` = greyscale (otherwise), adds `amount * uv` to x/y and sets/adds z.
/// WGSL: `dc_output(&v, AMOUNT, uv, color, gradient, scale_z, offset_z, reset_z)`.
#[allow(clippy::too_many_arguments)]
pub fn dc_output(
    ctx: &Ctx,
    v: &mut Pt,
    amount: f64,
    uv: Vec2,
    color: Vec3,
    gradient: i32,
    scale_z: f64,
    offset_z: f64,
    reset_z: i32,
) {
    let tcolor = glsl_dbl2int(color);
    let z = dc_greyscale(tcolor[0], tcolor[1], tcolor[2]);
    if gradient == 0 {
        v.rgb_color = true;
        v.red = tcolor[0] as f64;
        v.green = tcolor[1] as f64;
        v.blue = tcolor[2] as f64;
    } else if gradient == 1 {
        let col = dc_find_key(ctx, tcolor[0], tcolor[1], tcolor[2]);
        v.rgb_color = true;
        v.red = col[0] as f64;
        v.green = col[1] as f64;
        v.blue = col[2] as f64;
    } else {
        v.color = z;
    }
    v.x += amount * uv.x;
    v.y += amount * uv.y;
    let dz = z * scale_z + offset_z;
    if reset_z == 1 {
        v.z = dz;
    } else {
        v.z += dz;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12 * (1.0 + a.abs().max(b.abs()))
    }
    fn close2(a: Vec2, b: Vec2) -> bool {
        close(a.x, b.x) && close(a.y, b.y)
    }

    #[test]
    fn vector_operators_match_java_methods() {
        let a = vec2(1.0, 2.0);
        let b = vec2(3.0, -4.0);
        assert_eq!(a + b, vec2(4.0, -2.0));
        assert_eq!(a - 1.0, vec2(0.0, 1.0));
        assert_eq!(a * b, vec2(3.0, -8.0));
        assert_eq!(b / 2.0, vec2(1.5, -2.0));
        assert_eq!(2.0 * a, vec2(2.0, 4.0));
        assert_eq!(-a, vec2(-1.0, -2.0));
        let mut c = a;
        c += b;
        c *= 2.0;
        assert_eq!(c, vec2(8.0, -4.0));
        assert_eq!(Vec4::from_vec3(vec3(1.0, 2.0, 3.0), 4.0) + Vec4::splat(1.0), vec4(2.0, 3.0, 4.0, 5.0));
    }

    #[test]
    fn matrix_products_follow_java_times() {
        // mat2(a00, a10, a01, a11): columns (1,2) and (3,4) -> [[1,3],[2,4]]
        let m = mat2(1.0, 2.0, 3.0, 4.0);
        let v = vec2(5.0, 6.0);
        assert_eq!(m * v, vec2(1.0 * 5.0 + 3.0 * 6.0, 2.0 * 5.0 + 4.0 * 6.0));
        assert_eq!(v * m, vec2(1.0 * 5.0 + 2.0 * 6.0, 3.0 * 5.0 + 4.0 * 6.0));
        assert_eq!((m * 2.0).a01, 6.0);
        let m3 = Mat3::from_cols(vec3(1.0, 0.0, 0.0), vec3(0.0, 2.0, 0.0), vec3(1.0, 0.0, 3.0));
        assert_eq!(m3 * vec3(1.0, 1.0, 1.0), vec3(2.0, 2.0, 3.0));
        assert_eq!(vec3(1.0, 1.0, 1.0) * m3, vec3(1.0, 2.0, 4.0));
        // Mat4 scalar ops transpose (Java quirk).
        let m4 = Mat4::from_cols(Vec4::splat(0.0), vec4(7.0, 0.0, 0.0, 0.0), Vec4::splat(0.0), Vec4::splat(0.0));
        let t = m4 + 1.0;
        assert_eq!(t.a01, 1.0);
        assert_eq!(t.a10, 8.0);
        assert_eq!(m4 * vec4(0.0, 1.0, 0.0, 0.0), vec4(7.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn generic_functions_accept_scalars_and_vectors() {
        assert_eq!(fract(-0.25), 0.75);
        assert_eq!(fract(vec2(1.5, -0.5)), vec2(0.5, 0.5));
        assert_eq!(glsl_mod(-1.0, 3.0), 2.0);
        assert_eq!(glsl_mod(vec2(-1.0, 4.0), 3.0), vec2(2.0, 1.0));
        assert_eq!(min(vec2(1.0, 3.0), 2.0), vec2(1.0, 2.0));
        assert_eq!(max(vec3(1.0, 3.0, -1.0), vec3(2.0, 2.0, 2.0)), vec3(2.0, 3.0, 2.0));
        assert_eq!(step(0.5, vec2(0.4, 0.5)), vec2(0.0, 1.0));
        assert_eq!(step(0.5, 0.7), 1.0);
        assert_eq!(clamp(vec2(-1.0, 2.0), 0.0, 1.0), vec2(0.0, 1.0));
        assert_eq!(mix(vec2(0.0, 2.0), vec2(2.0, 4.0), 0.5), vec2(1.0, 3.0));
        assert_eq!(smoothstep(0.0, 1.0, 0.5), 0.5);
        assert_eq!(smoothstep(vec2(0.0, 0.0), vec2(1.0, 2.0), vec2(0.5, 1.0)), vec2(0.5, 0.5));
        assert_eq!(sign(vec3(-2.0, 0.0, 3.0)), vec3(-1.0, 0.0, 1.0));
        assert_eq!(abs(-3.0), 3.0);
        assert_eq!(trunc(vec2(-1.7, 1.7)), vec2(-1.0, 1.0));
        assert_eq!(glsl_round(vec2(-0.5, 2.5)), vec2(0.0, 3.0));
        assert!(close(length(vec3(2.0, 3.0, 6.0)), 7.0));
        assert!(close(distance(vec2(1.0, 1.0), vec2(4.0, 5.0)), 5.0));
        assert!(close(length(normalize(vec4(1.0, 2.0, 3.0, 4.0))), 1.0));
        assert_eq!(cross(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0)), vec3(0.0, 0.0, 1.0));
        assert_eq!(reflect(vec2(1.0, -1.0), vec2(0.0, 1.0)), vec2(1.0, 1.0));
        assert_eq!(pow(vec2(2.0, 3.0), 2.0), vec2(4.0, 9.0));
        assert!(close(log2(8.0), 3.0));
        assert_eq!(exp2(vec2(3.0, 0.0)), vec2(8.0, 1.0));
        assert!(close(tanh(0.5), 0.5f64.tanh()));
        assert!(close(refract(vec2(0.0, -1.0), vec2(0.0, 1.0), 1.0).y, -1.0));
    }

    #[test]
    fn atan_approximations() {
        assert!(close(glsl_atan(1.0, 1.0), std::f64::consts::FRAC_PI_4));
        // G.atan has no quadrant correction.
        assert!(close(glsl_atan(-1.0, -1.0), std::f64::consts::FRAC_PI_4));
        for i in 0..64 {
            let a = i as f64 / 64.0 * 2.0 * std::f64::consts::PI - std::f64::consts::PI + 0.01;
            let (s, c) = a.sin_cos();
            assert!((glsl_atan2(s, c) - s.atan2(c)).abs() < 0.075, "angle {a}");
        }
        assert!(close(glsl_atan2(1.0, 1.0), std::f64::consts::FRAC_PI_4));
        assert!((glsl_inv_sqrt(4.0) - 0.5).abs() < 2e-3);
        assert!((glsl_inv_sqrt_f32(4.0) - 0.5).abs() < 2e-3);
    }

    #[test]
    fn hash_and_noise_ranges() {
        for i in 0..200 {
            let p = vec2(i as f64 * 0.37 - 20.0, i as f64 * -0.61 + 7.0);
            let r = glsl_random(p);
            assert!((0.0..1.0).contains(&r));
            let r2 = glsl_random2(p);
            assert!((0.0..1.0).contains(&r2.x) && (0.0..1.0).contains(&r2.y));
            let h = glsl_hash3(p);
            assert!((0.0..1.0).contains(&h.x) && (0.0..1.0).contains(&h.z));
            let n = glsl_noise(p);
            assert!((0.0..=1.0).contains(&n), "noise {n}");
            let f = glsl_fbm(p);
            assert!((0.0..1.0).contains(&f), "fbm {f}");
            let f2 = glsl_fbm2(p);
            assert!((0.0..1.0).contains(&f2), "fbm2 {f2}");
            let q = glsl_iqnoise(p, 1.0, 1.0);
            assert!((0.0..=1.0).contains(&q), "iqnoise {q}");
        }
        // Value noise interpolates the lattice hashes.
        let p = vec2(3.0, -2.0);
        assert!(close(glsl_noise(p), glsl_random(p)));
        // Known value: fract(sin(0) * k) = 0.
        assert_eq!(glsl_random(vec2(0.0, 0.0)), 0.0);
    }

    #[test]
    fn color_conversions_roundtrip() {
        for &(r, g, b) in &[(1.0, 0.0, 0.0), (0.2, 0.7, 0.4), (0.9, 0.9, 0.1), (0.3, 0.1, 0.8)] {
            let c = vec3(r, g, b);
            // rgb2hsb == rgb2hsv; hsb2rgb is iq's *smoothed* HSV (cubic), so no exact roundtrip.
            assert_eq!(glsl_rgb2hsb(c), glsl_rgb2hsv(c.extend(1.0)).xyz());
            let c4 = vec4(r, g, b, 0.5);
            let back4 = glsl_hsv2rgb(glsl_rgb2hsv(c4));
            assert!((back4 - c4).length() < 1e-6, "{c4:?} -> {back4:?}");
        }
        // Pure hues are unaffected by the smoothing.
        let g = glsl_hsb2rgb(vec3(1.0 / 3.0, 1.0, 1.0));
        assert!((g - vec3(0.0, 1.0, 0.0)).length() < 1e-9);
        let m = glsl_hsb2rgb(vec3(5.0 / 6.0, 1.0, 0.5));
        assert!((m - vec3(0.5, 0.0, 0.5)).length() < 1e-9);
        // Smoothing: hue 1/12 (half way red -> yellow) gives green = smoothstep(0.5) = 0.5.
        let o = glsl_hsb2rgb(vec3(1.0 / 12.0, 1.0, 1.0));
        assert!((o - vec3(1.0, 0.5, 0.0)).length() < 1e-9);
        assert!((glsl_hsv2rgb(vec4(0.0, 0.0, 0.7, 1.0)) - vec4(0.7, 0.7, 0.7, 1.0)).length() < 1e-12);
    }

    #[test]
    fn complex_identities() {
        let z = vec2(0.7, 0.4);
        let w = vec2(-1.3, 0.25);
        assert!(close2(c_div(c_mul(z, w), w), z));
        assert!(close2(c_mul(z, c_inv(z)), c_one()));
        assert!(close2(c_exp(c_ln(z)), z));
        assert!(close2(c_mul(c_sqrt(z), c_sqrt(z)), z));
        assert!(close2(c_pow(z, 3.0), c_mul(z, c_mul(z, z))));
        assert!(close2(c_pow_c(z, vec2(2.0, 0.0)), c_mul(z, z)));
        assert!(close2(c_sin(c_asin(z)), z));
        assert!(close2(c_cos(c_acos(z)), z));
        assert!(close2(c_tan(c_atan(z)), z));
        assert!(close2(c_sinh(c_asinh(z)), z));
        // c_acosh needs Re(z) > 1 here: the Java c_to_polar has no quadrant correction.
        let z2 = vec2(1.7, 0.4);
        assert!(close2(c_cosh(c_acosh(z2)), z2));
        assert!(close2(c_tanh(c_atanh(z)), z));
        assert!(close2(glsl_cdiv(glsl_cmul(z, w), w), z));
        assert!(close2(glsl_cexp(glsl_clog(z)), z));
        assert!(close2(glsl_cpow(z, vec2(2.0, 0.0)), glsl_cmul(z, z)));
        assert!(close2(c_exp_base(2.0, vec2(3.0, 0.0)), vec2(8.0, 0.0)));
        assert!(close2(c_log(vec2(8.0, 0.0), 2.0), vec2(3.0, 0.0)));
        // G.sinh/cosh/tanh(vec2) apply a component-wise exp first (Java quirk).
        let v = vec2(0.3, -0.2);
        assert!(close2(glsl_cosh2(v) - glsl_sinh2(v), c_div(c_one(), exp(v))));
        assert!(close2(glsl_cosh2(v) + glsl_sinh2(v), exp(v)));
        assert!(close2(glsl_tanh2(v), c_div(glsl_sinh2(v), glsl_cosh2(v))));
        assert_eq!(c_atan(c_i()), vec2(0.0, f64::INFINITY));
        assert_eq!(c_atanh(-c_one()), vec2(f64::NEG_INFINITY, 0.0));
        let r = c_rem(vec2(5.5, 0.0), vec2(2.0, 0.0));
        assert!(close2(r, vec2(1.5, 0.0)));
        // Java quirk: c_to_polar uses atan(y/x), so (-1, 0) has angle -0 and sqrt(-1) = (1, -0).
        assert!(close2(c_sqrt(vec2(-1.0, 0.0)), vec2(1.0, 0.0)));
    }

    #[test]
    fn misc_helpers() {
        let k = std::f64::consts::PI / 6.0;
        let s = glsl_kscope(vec2(0.3, 0.8), k);
        assert!(close(s.length(), vec2(0.3, 0.8).length()));
        let a = s.y.atan2(s.x);
        assert!((-1e-12..=k + 1e-12).contains(&a));
        let r = glsl_rot(vec3(0.3, -0.2, 0.9));
        let v = vec3(0.1, 0.2, 0.3);
        assert!(close((r * v).length(), v.length()));
        assert!(close2(glsl_rotate2d(vec2(1.0, 0.5), std::f64::consts::FRAC_PI_2), vec2(0.5, 0.0)));
        assert_eq!(glsl_tile(vec2(0.75, 0.3), 2.0), vec2(0.5, 0.6));
        assert!(close2(glsl_brick_tile(vec2(0.1, 0.6), 2.0), vec2(0.7, 0.2)));
        assert!(close2(glsl_truchet_pattern(vec2(0.2, 0.3), 0.9), vec2(0.8, 0.7)));
        let g = glsl_simplex_grid(vec2(0.3, 0.1));
        assert!(g.x.is_finite() && g.y.is_finite() && g.z.is_finite());
        assert!(glsl_box(vec2(0.5, 0.5), vec2(0.5, 0.5), 0.01) > 0.99);
        assert!(glsl_circle(vec2(0.5, 0.5), 0.3) > 0.99);
        assert!(glsl_plot(vec2(0.0, 0.5), 0.5) <= 1.0);
        assert_eq!(glsl_fractal(vec2(0.0, 0.0)), Vec3::splat(0.0));
        assert!(glsl_fractal(vec2(2.0, 2.0)).x > 0.0);
        assert!(glsl_big_f(vec2(0.1, 0.2), 1.0).y.is_finite());
        assert!(glsl_f(vec2(0.1, 0.2), 1.0).x.is_finite());
        assert!(glsl_app(vec3(0.1, 0.2, 0.3), 1.1, r).x.is_finite());
        assert!(glsl_colorize(0.5, Vec3::splat(1.0), Vec3::splat(1.0), Vec3::splat(1.0), Vec3::splat(0.0)).x.is_finite());
        assert!(close(glsl_v(vec2(1.0, 0.0), 0.0, 0.0, 0.0), 0.5));
    }

    #[test]
    fn dc_helpers() {
        assert_eq!(glsl_dbl2int(vec3(-0.1, 0.5, 1.2)), [0, 128, 255]);
        assert!(close(dc_greyscale(255, 255, 255), 254.0 / 255.0));
        let mut ctx = Ctx::new(1, false);
        ctx.palette[17] = [200, 10, 10];
        assert_eq!(dc_find_key(&ctx, 190, 0, 0), [200, 10, 10]);
        let mut v = Pt::default();
        dc_output(&ctx, &mut v, 2.0, vec2(0.5, -0.25), vec3(1.0, 0.0, 0.0), 0, 1.0, 0.5, 1);
        assert!(v.rgb_color && v.red == 255.0 && v.green == 0.0);
        assert_eq!((v.x, v.y), (1.0, -0.5));
        assert!(close(v.z, 76.0 / 255.0 + 0.5));
        let mut v = Pt::default();
        dc_output(&ctx, &mut v, 1.0, vec2(0.0, 0.0), vec3(1.0, 1.0, 1.0), 2, 0.0, 0.0, 0);
        assert!(!v.rgb_color && close(v.color, 254.0 / 255.0));
    }
}
