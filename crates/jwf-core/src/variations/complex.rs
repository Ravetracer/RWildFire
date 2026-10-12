//! Complex numbers used by JWildfire variations.
//!
//! * [`Complex`]: port of the *mutable* `org.jwildfire.base.mathlib.Complex` (dark-beam).
//!   Methods modify `self` in place, exactly like the Java `void` methods, so
//!   `z.Mul(w)` becomes `z.mul(&w)`.
//! * [`CdComplex`]: port of the *immutable* `js.colordomain.Complex` (value semantics, every
//!   method returns a new number).
//!
//! WGSL counterparts of the [`Complex`] methods live in `wgsl/complex.wgsl` as pure functions on
//! `vec2<f32>` named `cx_<rust name>` (e.g. `z.atanh()` -> `z = cx_atanh(z);`).
//!
//! # `org.jwildfire.base.mathlib.Complex` -> [`Complex`]
//!
//! | Java                      | Rust                        | WGSL                        |
//! |---------------------------|-----------------------------|-----------------------------|
//! | `new Complex()`           | `Complex::default()` / `Complex::zero_new()` | `vec2<f32>(0.0)` |
//! | `new Complex(re)`         | `Complex::real(re)`         | `vec2<f32>(re, 0.0)`        |
//! | `new Complex(re, im)`     | `Complex::new(re, im)`      | `vec2<f32>(re, im)`         |
//! | `new Complex(z)`          | `Complex::copy_of(&z)`      | `z`                         |
//! | `z.re`, `z.im`            | `z.re`, `z.im`              | `z.x`, `z.y`                |
//! | `One() ImOne() Zero()`    | `one() im_one() zero()`     | `vec2(1,0)`, `vec2(0,1)`, `vec2(0)` |
//! | `Copy(w)`                 | `copy(&w)`                  | `z = w;`                    |
//! | `Flip() Conj() Neg()`     | `flip() conj() neg()`       | `cx_flip cx_conj cx_neg`    |
//! | `Sig() Sig2()`            | `sig() sig2()`              | `cx_sig cx_sig2`            |
//! | `Mag2() Mag2eps() MagInv()` | `mag2() mag2eps() mag_inv()` | `cx_mag2 cx_mag2eps cx_mag_inv` |
//! | `Save() Restore() Switch()` | `save() restore() switch()` | local variables          |
//! | `Keep(w) Recall()`        | `keep(&w) recall()`         | local variables             |
//! | `NextPow()`               | `next_pow()`                | `cx_mul(z, saved)`          |
//! | `Sqr() Recip() Scale(s)`  | `sqr() recip() scale(s)`    | `cx_sqr cx_recip cx_scale`  |
//! | `Mul(w) Div(w) DivR(w)`   | `mul(&w) div(&w) div_r(&w)` | `cx_mul cx_div cx_div_r`    |
//! | `Add(w) Sub(w) SubR(w)`   | `add(&w) sub(&w) sub_r(&w)` | `cx_add cx_sub cx_sub_r`    |
//! | `Inc() Dec()`             | `inc() dec()`               | `cx_inc cx_dec`             |
//! | `AMean GMean HMean RootMeanS Heronian LMean` | `a_mean g_mean h_mean root_mean_s heronian l_mean` | `cx_a_mean ...` |
//! | `PerFix(v)`               | `per_fix(v)`                | not supported (unused)      |
//! | `Pow(e) CPow(w)`          | `pow(e) cpow(&w)`           | `cx_pow cx_cpow`            |
//! | `Radius() Arg()`          | `radius() arg()`            | `cx_radius cx_arg`          |
//! | `ToP() UnP() Norm()`      | `to_p() un_p() norm()`      | `cx_to_p cx_un_p cx_norm`   |
//! | `Exp() Log() Sqrt()`      | `exp() log() sqrt()`        | `cx_exp cx_log cx_sqrt`     |
//! | `Sin() Cos() SinH() CosH()` | `sin() cos() sinh() cosh()` | `cx_sin cx_cos cx_sinh cx_cosh` (Java `Cos()` = `i*cos(z)`, kept) |
//! | `AtanH AsinH AcosH AcotH AsecH AcosecH` | `atanh asinh acosh acoth asech acosech` | `cx_atanh ...` |
//! | `Atan() Asin() Acos()`    | `atan() asin() acos()`      | `cx_atan cx_asin cx_acos`   |
//!
//! Notes:
//! * `Complex` is `Copy`. Java *aliasing* (`Complex a = b;` followed by `a.Mul(..)`, which also
//!   changes `b`) must be ported by mutating the original, not a copy.
//! * Methods take the argument by reference, so `z.Mul(z)` must be written as
//!   `let w = z; z.mul(&w);` (or `z.sqr()`).
//! * `new Complex(z)` copies only `re`/`im` (not the saved state / `per_fix`); `Copy`-ing the
//!   Rust struct copies everything. Use [`Complex::copy_of`] where the difference matters.

use std::f64::consts::PI;

/// Mutable complex number with save slot (`org.jwildfire.base.mathlib.Complex`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
    pub save_re: f64,
    pub save_im: f64,
    per_fix: f64,
}

impl Complex {
    /// `new Complex(re, im)`
    #[inline]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im, save_re: 0.0, save_im: 0.0, per_fix: 0.0 }
    }

    /// `new Complex()`
    #[inline]
    pub const fn zero_new() -> Self {
        Self::new(0.0, 0.0)
    }

    /// `new Complex(re)` (also the `int` overload).
    #[inline]
    pub const fn real(re: f64) -> Self {
        Self::new(re, 0.0)
    }

    /// `new Complex(zz)`: copies only `re` and `im`.
    #[inline]
    pub const fn copy_of(zz: &Complex) -> Self {
        Self::new(zz.re, zz.im)
    }

    // --- basic, utils ---------------------------------------------------------------------

    /// `One()`
    #[inline]
    pub fn one(&mut self) {
        self.re = 1.0;
        self.im = 0.0;
    }

    /// `ImOne()`
    #[inline]
    pub fn im_one(&mut self) {
        self.re = 0.0;
        self.im = 1.0;
    }

    /// `Zero()`
    #[inline]
    pub fn zero(&mut self) {
        self.re = 0.0;
        self.im = 0.0;
    }

    /// `Copy(zz)`: copies `re`/`im` of `zz`.
    #[inline]
    pub fn copy(&mut self, zz: &Complex) {
        self.re = zz.re;
        self.im = zz.im;
    }

    /// `Flip()`: swaps re and im.
    #[inline]
    pub fn flip(&mut self) {
        std::mem::swap(&mut self.re, &mut self.im);
    }

    /// `Conj()`
    #[inline]
    pub fn conj(&mut self) {
        self.im = -self.im;
    }

    /// `Neg()`
    #[inline]
    pub fn neg(&mut self) {
        self.re = -self.re;
        self.im = -self.im;
    }

    /// `Sig()`: sign of the real part (0 for 0).
    #[inline]
    pub fn sig(&self) -> f64 {
        if self.re == 0.0 {
            0.0
        } else if self.re > 0.0 {
            1.0
        } else {
            -1.0
        }
    }

    /// `Sig2()`: sign of the real part, never 0.
    #[inline]
    pub fn sig2(&self) -> f64 {
        if self.re >= 0.0 { 1.0 } else { -1.0 }
    }

    /// `Mag2()`: `re² + im²`
    #[inline]
    pub fn mag2(&self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    /// `Mag2eps()`: `re² + im² + 1e-20`
    #[inline]
    pub fn mag2eps(&self) -> f64 {
        self.re * self.re + self.im * self.im + 1e-20
    }

    /// `MagInv()`: `1 / Mag2()`, or 1 if `Mag2() < 1e-100`.
    #[inline]
    pub fn mag_inv(&self) -> f64 {
        let m2 = self.mag2();
        if m2 < 1e-100 { 1.0 } else { 1.0 / m2 }
    }

    // --- save / restore -------------------------------------------------------------------

    /// `Save()`
    #[inline]
    pub fn save(&mut self) {
        self.save_re = self.re;
        self.save_im = self.im;
    }

    /// `Restore()`
    #[inline]
    pub fn restore(&mut self) {
        self.re = self.save_re;
        self.im = self.save_im;
    }

    /// `Switch()`: swaps current and saved state.
    #[inline]
    pub fn switch(&mut self) {
        std::mem::swap(&mut self.re, &mut self.save_re);
        std::mem::swap(&mut self.im, &mut self.save_im);
    }

    /// `Keep(zz)`: saves `zz`.
    #[inline]
    pub fn keep(&mut self, zz: &Complex) {
        self.save_re = zz.re;
        self.save_im = zz.im;
    }

    /// `Recall()`: the saved state as a new number.
    #[inline]
    pub fn recall(&self) -> Complex {
        Complex::new(self.save_re, self.save_im)
    }

    /// `NextPow()`: multiplies by the saved state (`z^n -> z^(n+1)` after `save()`).
    #[inline]
    pub fn next_pow(&mut self) {
        let r = self.recall();
        self.mul(&r);
    }

    // --- arithmetic -----------------------------------------------------------------------

    /// `Sqr()`
    #[inline]
    pub fn sqr(&mut self) {
        let r2 = self.re * self.re - self.im * self.im;
        let i2 = 2.0 * self.re * self.im;
        self.re = r2;
        self.im = i2;
    }

    /// `Recip()`: `1 / z` (via [`Self::mag_inv`], so `1/0 = 0`).
    #[inline]
    pub fn recip(&mut self) {
        let mi = self.mag_inv();
        self.re *= mi;
        self.im = -self.im * mi;
    }

    /// `Scale(mul)`
    #[inline]
    pub fn scale(&mut self, mul: f64) {
        self.re *= mul;
        self.im *= mul;
    }

    /// `Mul(zz)` (a purely real `zz` only scales).
    #[inline]
    pub fn mul(&mut self, zz: &Complex) {
        if zz.im == 0.0 {
            self.scale(zz.re);
            return;
        }
        let r2 = self.re * zz.re - self.im * zz.im;
        let i2 = self.re * zz.im + self.im * zz.re;
        self.re = r2;
        self.im = i2;
    }

    /// `Div(zz)`: `z / zz` (via `zz.mag_inv()`).
    #[inline]
    pub fn div(&mut self, zz: &Complex) {
        let r2 = self.im * zz.im + self.re * zz.re;
        let i2 = self.im * zz.re - self.re * zz.im;
        let m2 = zz.mag_inv();
        self.re = r2 * m2;
        self.im = i2 * m2;
    }

    /// `DivR(zz)`: `zz / z` (reverse division).
    #[inline]
    pub fn div_r(&mut self, zz: &Complex) {
        let r2 = zz.im * self.im + zz.re * self.re;
        let i2 = zz.im * self.re - zz.re * self.im;
        let m2 = self.mag_inv();
        self.re = r2 * m2;
        self.im = i2 * m2;
    }

    /// `Add(zz)`
    #[inline]
    pub fn add(&mut self, zz: &Complex) {
        self.re += zz.re;
        self.im += zz.im;
    }

    /// `Sub(zz)`
    #[inline]
    pub fn sub(&mut self, zz: &Complex) {
        self.re -= zz.re;
        self.im -= zz.im;
    }

    /// `SubR(zz)`: `zz - z`
    #[inline]
    pub fn sub_r(&mut self, zz: &Complex) {
        self.re = zz.re - self.re;
        self.im = zz.im - self.im;
    }

    /// `Inc()`
    #[inline]
    pub fn inc(&mut self) {
        self.re += 1.0;
    }

    /// `Dec()`
    #[inline]
    pub fn dec(&mut self) {
        self.re -= 1.0;
    }

    /// `AMean(zz)`: arithmetic mean.
    pub fn a_mean(&mut self, zz: &Complex) {
        self.add(zz);
        self.scale(0.5);
    }

    /// `RootMeanS(zz)`: root mean square.
    pub fn root_mean_s(&mut self, zz: &Complex) {
        let mut pf = Complex::copy_of(zz);
        pf.sqr();
        self.sqr();
        self.add(&pf);
        self.scale(0.5);
        self.pow(0.5);
    }

    /// `GMean(zz)`: geometric mean.
    pub fn g_mean(&mut self, zz: &Complex) {
        self.mul(zz);
        self.pow(0.5);
    }

    /// `Heronian(zz)`: Heronian mean.
    pub fn heronian(&mut self, zz: &Complex) {
        let mut hm = Complex::copy_of(self);
        hm.g_mean(zz);
        self.add(zz);
        self.add(&hm);
        self.scale(1.0 / 3.0); // Java: 0.333333333333333333333333 (same double)
    }

    /// `HMean(zz)`: harmonic mean (0 if one term is 0).
    pub fn h_mean(&mut self, zz: &Complex) {
        let mut p2 = zz.re + self.re;
        let mut q2 = zz.im + self.im;
        let mut d = 0.5 * (p2 * p2 + q2 * q2);
        if d == 0.0 {
            self.zero();
            return;
        }
        d = 1.0 / d;
        p2 = self.mag2();
        q2 = zz.mag2();
        if p2 * q2 == 0.0 {
            self.zero();
            return;
        }
        self.re = d * (self.re * q2 + zz.re * p2);
        self.im = d * (self.im * q2 + zz.im * p2);
    }

    /// `PerFix(v)`: adds `PI * v` to [`Self::arg`]; a negative value also selects the negative
    /// square root in [`Self::sqrt`].
    #[inline]
    pub fn per_fix(&mut self, v: f64) {
        self.per_fix = PI * v;
    }

    /// `Pow(exp)`: real power (fast paths for 0, +-0.5, +-1, +-2).
    pub fn pow(&mut self, exp: f64) {
        if exp == 0.0 {
            self.one();
            return;
        }
        let ex = exp.abs();
        if exp < 0.0 {
            self.recip();
        }
        if ex == 0.5 {
            self.sqrt();
            return;
        }
        if ex == 1.0 {
            return;
        }
        if ex == 2.0 {
            self.sqr();
            return;
        }
        let mut pf = self.to_p();
        pf.re = pf.re.powf(ex);
        pf.im *= ex;
        let u = pf.un_p();
        self.copy(&u);
    }

    // --- transcendental functions ---------------------------------------------------------

    /// `Radius()`: `hypot(re, im)`
    #[inline]
    pub fn radius(&self) -> f64 {
        self.re.hypot(self.im)
    }

    /// `Arg()`: `per_fix + atan2(im, re)`
    #[inline]
    pub fn arg(&self) -> f64 {
        self.per_fix + self.im.atan2(self.re)
    }

    /// `ToP()`: polar form `(radius, arg)` as a new number.
    #[inline]
    pub fn to_p(&self) -> Complex {
        Complex::new(self.radius(), self.arg())
    }

    /// `UnP()`: interprets `(re, im)` as `(radius, angle)` and returns the rectangular form.
    #[inline]
    pub fn un_p(&self) -> Complex {
        Complex::new(self.re * self.im.cos(), self.re * self.im.sin())
    }

    /// `Norm()`: scales to unit length (tiny numbers are left unnormalized).
    #[inline]
    pub fn norm(&mut self) {
        let s = self.mag_inv().sqrt();
        self.scale(s);
    }

    /// `Exp()`
    #[inline]
    pub fn exp(&mut self) {
        self.re = self.re.exp();
        let u = self.un_p();
        self.copy(&u);
    }

    /// `SinH()`
    pub fn sinh(&mut self) {
        self.re = self.re.exp();
        let er = 1.0 / self.re;
        let rr = 0.5 * (self.re - er);
        let ri = rr + er;
        self.re = self.im.cos() * rr;
        self.im = self.im.sin() * ri;
    }

    /// `Sin()`: `flip, sinh, flip`
    pub fn sin(&mut self) {
        self.flip();
        self.sinh();
        self.flip();
    }

    /// `CosH()`
    pub fn cosh(&mut self) {
        self.re = self.re.exp();
        let er = 1.0 / self.re;
        let rr = 0.5 * (self.re - er);
        let ri = rr + er;
        self.re = self.im.cos() * ri;
        self.im = self.im.sin() * rr;
    }

    /// `Cos()`: `flip, cosh, flip`. Java quirk kept: this evaluates to `i * cos(z)`, not `cos(z)`.
    pub fn cos(&mut self) {
        self.flip();
        self.cosh();
        self.flip();
    }

    /// `Sqrt()`: principal square root (negated if `per_fix < 0`).
    pub fn sqrt(&mut self) {
        let rad = self.radius();
        let sb = if self.im < 0.0 { -1.0 } else { 1.0 };
        self.im = sb * (0.5 * (rad - self.re)).sqrt();
        self.re = (0.5 * (rad + self.re)).sqrt();
        if self.per_fix < 0.0 {
            self.neg();
        }
    }

    /// `Log()`: `(0.5 * ln(mag2eps), arg)`
    pub fn log(&mut self) {
        let l = Complex::new(0.5 * self.mag2eps().ln(), self.arg());
        self.copy(&l);
    }

    /// `LMean(zz)`: logarithmic mean `(z - zz) / log(z / zz)`.
    pub fn l_mean(&mut self, zz: &Complex) {
        let mut dab = Complex::copy_of(self);
        let mut lab = Complex::copy_of(self);
        dab.sub(zz);
        lab.div(zz);
        lab.log();
        dab.div(&lab);
        self.copy(&dab);
    }

    /// `AtanH()`: `0.5 * log((1 + z) / (1 - z))`
    pub fn atanh(&mut self) {
        let mut d = Complex::copy_of(self);
        d.dec();
        d.neg();
        self.inc();
        self.div(&d);
        self.log();
        self.scale(0.5);
    }

    /// `AsinH()`: `log(z + sqrt(z² + 1))`
    pub fn asinh(&mut self) {
        let mut d = Complex::copy_of(self);
        d.sqr();
        d.inc();
        d.pow(0.5);
        self.add(&d);
        self.log();
    }

    /// `AcosH()`: `log(z + sqrt(z² - 1))`
    pub fn acosh(&mut self) {
        let mut d = Complex::copy_of(self);
        d.sqr();
        d.dec();
        d.pow(0.5);
        self.add(&d);
        self.log();
    }

    /// `AcotH()`: `atanh(1 / z)`
    pub fn acoth(&mut self) {
        self.recip();
        self.atanh();
    }

    /// `AsecH()`: `asinh(1 / z)` (sic, as in Java)
    pub fn asech(&mut self) {
        self.recip();
        self.asinh();
    }

    /// `AcosecH()`: `acosh(1 / z)` (sic, as in Java)
    pub fn acosech(&mut self) {
        self.recip();
        self.acosh();
    }

    /// `Atan()`: `flip, atanh, flip`
    pub fn atan(&mut self) {
        self.flip();
        self.atanh();
        self.flip();
    }

    /// `Asin()`: `flip, asinh, flip`
    pub fn asin(&mut self) {
        self.flip();
        self.asinh();
        self.flip();
    }

    /// `Acos()`: `PI/2 - asin(z)`
    pub fn acos(&mut self) {
        self.flip();
        self.asinh();
        self.flip();
        self.re = PI / 2.0 - self.re;
        self.im = -self.im;
    }

    /// `CPow(ex)`: complex power `exp(log(z) * ex)` (real `ex` uses [`Self::pow`]).
    pub fn cpow(&mut self, ex: &Complex) {
        if ex.im == 0.0 {
            self.pow(ex.re);
            return;
        }
        self.log();
        self.mul(ex);
        self.exp();
    }
}

// =============================================================================================
// js.colordomain.Complex
// =============================================================================================

/// Immutable complex number (`js.colordomain.Complex`).
///
/// The Java class has many aliases per operation; they map to one Rust method each:
///
/// | Java                                           | Rust                    |
/// |------------------------------------------------|-------------------------|
/// | `new Complex(u, v)`                            | `CdComplex::new(u, v)`  |
/// | `real re Re Real` / `imag Imag Im im`          | `re()` / `im()`         |
/// | `mag`                                          | `mag()`                 |
/// | `mod modulus Modulus Mod`                      | `modulus()`             |
/// | `arg argument Arg Argument`                    | `arg()`                 |
/// | `conj conjugate Conjugate Conj`                | `conj()`                |
/// | `plus add Plus Add`                            | `add(w)`                |
/// | `minus Minus sub Subtract`                     | `sub(w)`                |
/// | `times mul mult multiply Times Mul Mult Multiply` | `mul(w)`             |
/// | `div divide Div Divide`                        | `div(w)`                |
/// | `scale Scale`                                  | `scale(s)`              |
/// | `exp Exp`, `log Log`, `sqrt Sqrt`              | `exp()`, `log()`, `sqrt()` |
/// | `sin Sin`, `cos Cos`, `tan Tan`                | `sin()`, `cos()`, `tan()` |
/// | `sinh Sinh`, `cosh Cosh`, `tanh`               | `sinh()`, `cosh()`, `tanh()` |
/// | `asin acos atan atanh asinh acosh acoth asech acosech` | same names       |
/// | `One ImOne Zero`                               | `CdComplex::one()/im_one()/zero()` |
/// | `Neg`, `chs`                                   | `neg()`                 |
/// | `Mag2 Mag2eps MagInv Recip Inc Dec Sqr`        | `mag2 mag2eps mag_inv recip inc dec sqr` |
/// | `Radius ToP UnP`                               | `radius to_p un_p`      |
/// | `Pow(e)`, `CPow(w)`                            | `pow(e)`, `cpow(w)` (take `&mut self`, see below) |
/// | `Complex.norm(z)` (static, `re² + im²`)        | `CdComplex::norm(z)`    |
/// | `sinhcosh(x)`                                  | `sinhcosh(x)`           |
/// | `Norm()`                                       | no-op in Java: drop the call |
///
/// Java quirk kept: `Pow(e)` with `e < 0` replaces the receiver by its reciprocal before
/// computing the power (so `pow`/`cpow` take `&mut self`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CdComplex {
    pub x: f64,
    pub y: f64,
}

// Method names mirror the Java API (`add`, `mul`, `neg`, ...), not the std operator traits.
#[allow(clippy::should_implement_trait)]
impl CdComplex {
    #[inline]
    pub const fn new(u: f64, v: f64) -> Self {
        Self { x: u, y: v }
    }
    #[inline]
    pub const fn one() -> Self {
        Self::new(1.0, 0.0)
    }
    #[inline]
    pub const fn im_one() -> Self {
        Self::new(0.0, 1.0)
    }
    #[inline]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    #[inline]
    pub const fn re(self) -> f64 {
        self.x
    }
    #[inline]
    pub const fn im(self) -> f64 {
        self.y
    }
    /// `mag()`: `sqrt(x² + y²)`
    #[inline]
    pub fn mag(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    /// `mod()`: `sqrt(x² + y²)` (0 for 0)
    #[inline]
    pub fn modulus(self) -> f64 {
        if self.x != 0.0 || self.y != 0.0 { (self.x * self.x + self.y * self.y).sqrt() } else { 0.0 }
    }
    /// `arg()`: `atan2(y, x)`
    #[inline]
    pub fn arg(self) -> f64 {
        self.y.atan2(self.x)
    }
    #[inline]
    pub fn conj(self) -> Self {
        Self::new(self.x, -self.y)
    }
    #[inline]
    pub fn add(self, w: Self) -> Self {
        Self::new(self.x + w.x, self.y + w.y)
    }
    #[inline]
    pub fn sub(self, w: Self) -> Self {
        Self::new(self.x - w.x, self.y - w.y)
    }
    #[inline]
    pub fn mul(self, w: Self) -> Self {
        Self::new(self.x * w.x - self.y * w.y, self.x * w.y + self.y * w.x)
    }
    #[inline]
    pub fn scale(self, s: f64) -> Self {
        Self::new(self.x * s, self.y * s)
    }
    /// `div(w)`: denominator `pow(w.mod(), 2)`.
    #[inline]
    pub fn div(self, w: Self) -> Self {
        let m = w.modulus();
        let den = m * m;
        Self::new((self.x * w.x + self.y * w.y) / den, (self.y * w.x - self.x * w.y) / den)
    }
    #[inline]
    pub fn exp(self) -> Self {
        Self::new(self.x.exp() * self.y.cos(), self.x.exp() * self.y.sin())
    }
    /// `log()`: `(ln(mod), arg)`
    #[inline]
    pub fn log(self) -> Self {
        Self::new(self.modulus().ln(), self.arg())
    }
    #[inline]
    pub fn sqrt(self) -> Self {
        let r = self.modulus().sqrt();
        let theta = self.arg() / 2.0;
        Self::new(r * theta.cos(), r * theta.sin())
    }
    #[inline]
    fn rcosh(t: f64) -> f64 {
        (t.exp() + (-t).exp()) / 2.0
    }
    #[inline]
    fn rsinh(t: f64) -> f64 {
        (t.exp() - (-t).exp()) / 2.0
    }
    #[inline]
    pub fn sin(self) -> Self {
        Self::new(Self::rcosh(self.y) * self.x.sin(), Self::rsinh(self.y) * self.x.cos())
    }
    #[inline]
    pub fn cos(self) -> Self {
        Self::new(Self::rcosh(self.y) * self.x.cos(), -Self::rsinh(self.y) * self.x.sin())
    }
    #[inline]
    pub fn sinh(self) -> Self {
        Self::new(Self::rsinh(self.x) * self.y.cos(), Self::rcosh(self.x) * self.y.sin())
    }
    #[inline]
    pub fn cosh(self) -> Self {
        Self::new(Self::rcosh(self.x) * self.y.cos(), Self::rsinh(self.x) * self.y.sin())
    }
    #[inline]
    pub fn tan(self) -> Self {
        self.sin().div(self.cos())
    }
    /// `Neg()` / `chs()`
    #[inline]
    pub fn neg(self) -> Self {
        Self::new(-self.x, -self.y)
    }
    #[inline]
    pub fn mag2(self) -> f64 {
        self.x * self.x + self.y * self.y
    }
    #[inline]
    pub fn mag2eps(self) -> f64 {
        self.x * self.x + self.y * self.y + 1e-20
    }
    #[inline]
    pub fn mag_inv(self) -> f64 {
        let m2 = self.mag2();
        if m2 < 1e-100 { 1.0 } else { 1.0 / m2 }
    }
    #[inline]
    pub fn recip(self) -> Self {
        let mi = self.mag_inv();
        Self::new(self.x * mi, -self.y * mi)
    }
    #[inline]
    pub fn inc(self) -> Self {
        Self::new(self.x + 1.0, self.y)
    }
    #[inline]
    pub fn dec(self) -> Self {
        Self::new(self.x - 1.0, self.y)
    }
    #[inline]
    pub fn sqr(self) -> Self {
        Self::new(self.x * self.x - self.y * self.y, 2.0 * self.x * self.y)
    }
    /// `atanh()`: `0.5 * log((1 + z) / -(z - 1))`
    pub fn atanh(self) -> Self {
        let d = self.dec().neg();
        let e = self.inc();
        e.div(d).log().scale(0.5)
    }
    pub fn acoth(self) -> Self {
        self.recip().atanh()
    }
    #[inline]
    pub fn radius(self) -> f64 {
        self.x.hypot(self.y)
    }
    #[inline]
    pub fn to_p(self) -> Self {
        Self::new(self.radius(), self.arg())
    }
    #[inline]
    pub fn un_p(self) -> Self {
        Self::new(self.x * self.y.cos(), self.x * self.y.sin())
    }
    /// `Pow(exp)`. Quirk: for `exp < 0` the receiver itself is replaced by its reciprocal.
    pub fn pow(&mut self, exp: f64) -> Self {
        if exp == 0.0 {
            return Self::one();
        }
        let ex = exp.abs();
        if exp < 0.0 {
            *self = self.recip();
        }
        if ex == 0.5 {
            return self.sqrt();
        }
        if ex == 1.0 {
            return *self;
        }
        if ex == 2.0 {
            return self.sqr();
        }
        let mut pf = self.to_p();
        pf.x = pf.x.powf(ex);
        pf.y *= ex;
        pf.un_p()
    }
    /// `CPow(ex)`
    pub fn cpow(&mut self, ex: Self) -> Self {
        if ex.y == 0.0 {
            return self.pow(ex.x);
        }
        self.log().mul(ex).exp()
    }
    /// `asinh()`: `log(z + sqrt(z² + 1))`
    pub fn asinh(self) -> Self {
        let x = self.sqr().inc().pow(0.5);
        self.add(x).log()
    }
    /// `acosh()`: `log(z + sqrt(z² - 1))`
    pub fn acosh(self) -> Self {
        let x = self.sqr().dec().pow(0.5);
        self.add(x).log()
    }
    pub fn asech(self) -> Self {
        self.recip().asinh()
    }
    pub fn acosech(self) -> Self {
        self.recip().acosh()
    }
    /// Static `Complex.norm(z)`: `re² + im²`.
    #[inline]
    pub fn norm(z: Self) -> f64 {
        z.x * z.x + z.y * z.y
    }
    /// `sinhcosh(x)`: `((e^x + e^-x) / 2, (e^x - e^-x) / 2)` (ignores the receiver).
    #[inline]
    pub fn sinhcosh(self, x: f64) -> Self {
        let ex = x.exp();
        let emx = (-x).exp();
        Self::new(ex + emx, ex - emx).scale(0.5)
    }
    pub fn acos(self) -> Self {
        let t1 = Self::new(self.y * self.y - self.x * self.x + 1.0, -2.0 * self.x * self.y).sqrt();
        let t2 = Self::new(t1.x - self.y, t1.y + self.x).log();
        Self::new(PI / 2.0 - t2.y, t2.x)
    }
    pub fn asin(self) -> Self {
        let t1 = Self::new(self.y * self.y - self.x * self.x + 1.0, -2.0 * self.x * self.y).sqrt();
        let t2 = Self::new(t1.x - self.y, t1.y + self.x).log();
        Self::new(t2.y, -t2.x)
    }
    pub fn tanh(self) -> Self {
        let ez = self.exp();
        let emz = self.scale(-1.0).exp();
        ez.sub(emz).div(ez.add(emz))
    }
    /// `atan()`; the Java code computes the denominator as `float`.
    pub fn atan(self) -> Self {
        let d = (self.x * self.x + (1.0 - self.y) * (1.0 - self.y)) as f32 as f64;
        let t1 = Self::new(1.0 - self.y * self.y - self.x * self.x, -2.0 * self.x).scale(1.0 / d).log();
        Self::new(-t1.y, t1.x).scale(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &Complex, re: f64, im: f64) -> bool {
        (a.re - re).abs() < 1e-9 && (a.im - im).abs() < 1e-9
    }

    #[test]
    fn arithmetic_is_in_place() {
        let mut z = Complex::new(1.0, 2.0);
        let w = Complex::new(3.0, -1.0);
        z.mul(&w);
        assert!(close(&z, 5.0, 5.0));
        z.div(&w);
        assert!(close(&z, 1.0, 2.0));
        let mut r = Complex::new(3.0, -1.0);
        r.div_r(&Complex::new(5.0, 5.0));
        assert!(close(&r, 1.0, 2.0));
        z.sqr();
        assert!(close(&z, -3.0, 4.0));
        z.recip();
        assert!(close(&z, -3.0 / 25.0, -4.0 / 25.0));
        let mut s = Complex::new(1.0, 2.0);
        s.sub_r(&Complex::new(0.0, 0.0));
        assert!(close(&s, -1.0, -2.0));
        let mut f = Complex::new(1.0, 2.0);
        f.flip();
        f.conj();
        assert!(close(&f, 2.0, -1.0));
        assert_eq!(Complex::new(-0.0, 1.0).sig(), 0.0);
        assert_eq!(Complex::new(0.0, 1.0).sig2(), 1.0);
        let mut zero = Complex::default();
        zero.recip();
        assert!(close(&zero, 0.0, 0.0));
    }

    #[test]
    fn save_restore_and_powers() {
        let mut z = Complex::new(0.5, 0.25);
        z.save();
        z.next_pow();
        z.next_pow();
        let mut p = Complex::new(0.5, 0.25);
        p.pow(3.0);
        assert!(close(&z, p.re, p.im));
        z.restore();
        assert!(close(&z, 0.5, 0.25));
        z.keep(&Complex::new(7.0, 8.0));
        z.switch();
        assert!(close(&z, 7.0, 8.0) && z.save_re == 0.5);
        assert!(close(&z.recall(), 0.5, 0.25));
        let mut q = Complex::new(0.3, -0.7);
        q.pow(-2.0);
        let mut q2 = Complex::new(0.3, -0.7);
        q2.sqr();
        q2.recip();
        assert!(close(&q, q2.re, q2.im));
        let mut c = Complex::new(0.3, -0.7);
        c.cpow(&Complex::new(2.0, 0.0));
        let mut c2 = Complex::new(0.3, -0.7);
        c2.cpow(&Complex::new(2.0, 1e-300));
        assert!(close(&c, c2.re, c2.im));
    }

    #[test]
    fn transcendental_identities() {
        let base = Complex::new(0.6, 0.35);
        let check = |f: fn(&mut Complex), inv: fn(&mut Complex)| {
            let mut z = base;
            f(&mut z);
            inv(&mut z);
            assert!(close(&z, base.re, base.im), "{z:?}");
        };
        check(Complex::exp, Complex::log);
        check(Complex::log, Complex::exp);
        check(Complex::sinh, Complex::asinh);
        check(Complex::cosh, Complex::acosh);
        check(Complex::sin, Complex::asin);
        // Java quirk: Cos() = flip/cosh/flip evaluates to i * cos(z).
        let mut c = base;
        c.cos();
        let (cr, ci) = (base.re.cos() * base.im.cosh(), -base.re.sin() * base.im.sinh());
        assert!(close(&c, -ci, cr), "{c:?}");
        // Acos() = PI/2 - Asin()
        let mut a = base;
        a.acos();
        let mut s = base;
        s.asin();
        assert!(close(&a, PI / 2.0 - s.re, -s.im));
        let mut ac = Complex::new(cr, ci);
        ac.acos();
        assert!(close(&ac, base.re, base.im), "{ac:?}");
        // atanh: tanh(z) = sinh/cosh
        let mut z = base;
        z.atanh();
        let mut s = z;
        s.sinh();
        let mut c = z;
        c.cosh();
        s.div(&c);
        assert!((s.re - base.re).abs() < 1e-9 && (s.im - base.im).abs() < 1e-9);
        let mut t = base;
        t.atan();
        let (mut ts, mut tc) = (t, t);
        ts.sin();
        tc.cos(); // = i * cos(t)
        let true_cos = Complex::new(tc.im, -tc.re);
        ts.div(&true_cos);
        assert!(close(&ts, base.re, base.im));
        let mut r = Complex::new(-4.0, 0.0);
        r.sqrt();
        assert!(close(&r, 0.0, 2.0));
        let mut r = Complex::new(3.0, -4.0);
        r.sqrt();
        assert!(close(&r, 2.0, -1.0));
        r.per_fix(-1.0);
        r.sqr();
        r.sqrt();
        assert!(close(&r, -2.0, 1.0));
        let mut n = Complex::new(3.0, 4.0);
        n.norm();
        assert!(close(&n, 0.6, 0.8));
        assert_eq!(Complex::new(3.0, 4.0).radius(), 5.0);
        let p = Complex::new(3.0, 4.0).to_p().un_p();
        assert!(close(&p, 3.0, 4.0));
    }

    #[test]
    fn means() {
        let mut a = Complex::new(1.0, 1.0);
        a.a_mean(&Complex::new(3.0, -1.0));
        assert!(close(&a, 2.0, 0.0));
        let mut g = Complex::new(2.0, 0.0);
        g.g_mean(&Complex::new(8.0, 0.0));
        assert!(close(&g, 4.0, 0.0));
        let mut h = Complex::new(2.0, 0.0);
        h.h_mean(&Complex::new(6.0, 0.0));
        assert!(close(&h, 3.0, 0.0));
        let mut q = Complex::new(1.0, 0.0);
        q.root_mean_s(&Complex::new(7.0, 0.0));
        assert!(close(&q, 5.0, 0.0));
        let mut he = Complex::new(1.0, 0.0);
        he.heronian(&Complex::new(4.0, 0.0));
        assert!(close(&he, 7.0 / 3.0, 0.0));
        let mut l = Complex::new(4.0, 0.0);
        l.l_mean(&Complex::new(1.0, 0.0));
        assert!(close(&l, 3.0 / 4f64.ln(), 0.0));
    }

    #[test]
    fn colordomain_complex() {
        let z = CdComplex::new(0.6, 0.35);
        let w = CdComplex::new(-1.2, 0.5);
        let c = |a: CdComplex, b: CdComplex| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9;
        assert!(c(z.mul(w).div(w), z));
        assert!(c(z.exp().log(), z));
        assert!(c(z.sqrt().mul(z.sqrt()), z));
        assert!(c(z.asin().sin(), z));
        assert!(c(z.acos().cos(), z));
        assert!(c(z.tanh(), z.sinh().div(z.cosh())));
        assert!(c(z.tan(), z.sin().div(z.cos())));
        assert!(c(z.atanh().tanh(), z));
        assert!(c(z.asinh().sinh(), z));
        assert!((z.atan().tan().x - z.x).abs() < 1e-6 && (z.atan().tan().y - z.y).abs() < 1e-6);
        let mut p = z;
        assert!(c(p.pow(2.0), z.sqr()));
        let mut n = z;
        let r = n.pow(-3.0);
        assert!(c(n, z.recip()), "Pow(<0) replaces the receiver");
        assert!(c(r, z.sqr().mul(z).recip()));
        assert!(c(z.add(w).sub(w), z));
        assert_eq!(CdComplex::norm(CdComplex::new(3.0, 4.0)), 25.0);
        assert_eq!(CdComplex::new(3.0, 4.0).modulus(), 5.0);
        assert_eq!(CdComplex::zero().modulus(), 0.0);
        assert!(c(z.sinhcosh(0.5), CdComplex::new(0.5f64.cosh(), 0.5f64.sinh())));
    }
}
