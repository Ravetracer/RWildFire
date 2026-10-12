# Porting JWildfire variations

Mechanical rules for porting `org.jwildfire.create.tina.variation.*Func` to a `VarDef`
(CPU closure + WGSL snippet). Look at `batch_a.rs` .. `batch_d.rs` for complete examples.

## 1. Imports and helper modules

```rust
use super::{Ctx, ParamDef, Pt, VarDef, VarKind, simple};
use super::glsl::*;          // js.glsl.*  (Vec2/Vec3/Mat2/..., G.* functions)
use super::complex::Complex; // org.jwildfire.base.mathlib.Complex  (CdComplex: js.colordomain.Complex)
use super::noise;            // NoiseTools / VoronoiTools
use crate::math::*;          // MathLib constants, Tools.FTOI / limitValue, GfxMathLib
use crate::model::Affine;
```

| Module (Rust)       | WGSL library (always in the prelude)  | Java source                                  |
|---------------------|---------------------------------------|----------------------------------------------|
| `crate::math`       | `wgsl/math.wgsl`                      | `MathLib`, `GfxMathLib`, `Tools`             |
| `variations::glsl`  | `wgsl/glsl.wgsl`, `wgsl/glsl_complex.wgsl` | `js.glsl.G`, `vec*`, `mat*`, `GLSLFunc`, `DC_BaseFunc` |
| `variations::complex` | `wgsl/complex.wgsl` (`cx_*`)        | `mathlib.Complex`, `js.colordomain.Complex`  |
| `variations::noise` | `wgsl/noise.wgsl` (`noise_*`)         | `NoiseTools`, `VoronoiTools`                 |

**Naming rule:** a helper has the *same name* in Rust and WGSL. Functions that are WGSL builtins
with GLSL semantics keep their GLSL name (`fract`, `mix`, `length`, ...); everything else carries
a prefix (`glsl_`, `c_`, `cx_`, `noise_`, `dc_`, `java_`, `gfx_`, `math_`). WGSL has no
overloading, so vector variants of non-builtins get a size suffix (`glsl_mod2`, `glsl_round3`).

## 2. Variation skeleton

| Java                                          | Rust (`apply`)                     | WGSL snippet                       |
|-----------------------------------------------|------------------------------------|------------------------------------|
| `pAffineTP.x`, `pVarTP.y`, `pAmount`          | `a.x`, `v.y`, `amount`             | `a.x`, `v.y`, `AMOUNT`             |
| `pAffineTP.getPrecalcSqrt()` / `getPrecalcAtan()` / `getPrecalcAtanYX()` / `getPrecalcSumsq()` | `a.rad()` / `a.atan_xy()` / `a.atan_yx()` / `a.sumsq()` | `rad(a)` / `atan_xy(a)` / `atan_yx(a)` / `sumsq(a)` |
| `pContext.random()`, `Math.random()`          | `ctx.random()`                     | `rnd()`                            |
| `pContext.random(n)` (int)                    | `ctx.random_int(n)`                | `i32(rnd() * f32(n))`              |
| `if (pContext.isPreserveZCoordinate()) pVarTP.z += pAmount * pAffineTP.z;` | `ctx.pass_z(a, v, amount);` | `if (PRESERVE_Z) { v.z += AMOUNT * a.z; }` |
| parameter `k` (declaration order)             | `d[k]`                             | `D(k)`                             |
| `init()` precalc                              | `prepare: Some(\|p, coefs\| vec![...])` appending values after the params | `D(k)` |
| `pXForm.getCoeff20()` etc.                    | in `prepare`: `coefs.c20` (`c00 c01 c10 c11 c20 c21`) | `D(k)` |
| `pVarTP.color = ...`                          | `v.color = ...`                    | `v.color = ...;`                   |
| true color: `pVarTP.rgbColor = true; pVarTP.redColor = r;` (0..255) | `v.rgb_color = true; v.red = r; v.green = g; v.blue = b;` | `v.rgbc = 1.0; v.r = r; v.g = g; v.b = b;` |
| gradient color `layer.getPalette().getColor(i)` | `ctx.palette_rgb(i)` -> `[f64; 3]` (0..255) | `palette_rgb(i)` -> `vec3<f32>` |
| `getPriority()` -1 / 1                        | `kind: VarKind::Pre` / `VarKind::Post` | same snippet rules             |

* `d` is what `prepare` returns (defaults: the parameters). Comment the layout:
  `// d = [cellsize, power, sin_rot, cos_rot]`. The GPU receives the same `d`.
* `prepare` must be deterministic (no `ctx`); Java `init()` randomness has to become a parameter
  or a fixed value. Integer parameters arrive as `f64` (`d[3] as i32`).
* Java `int` arithmetic wraps; Rust panics on overflow in debug builds. Use `wrapping_mul` /
  `wrapping_add` for hashes. `(int) x` -> `x as i32` (both saturate); in WGSL `i32(x)`.

### WGSL snippet rules

* Module-scope helpers go *before* a line containing only `@body`; their names must start with
  the variation name (`fn crackle_cell(...)`). `D(k)` is only expanded in the body, so pass the
  values a helper needs as arguments (or pass `base` and read `prm[base + 3u]`).
* f32 only: replace `1e-300`/`SMALL_EPSILON` by `EPS` (1e-20), `1e-100`-ish guards by `1e-30`,
  `1e100`/`Double.MAX_VALUE` by `3.0e38`. `EPSILON` (1e-8) still works as a near-zero test.
* Constant expressions must not produce inf/NaN (`1.0 / 0.0` does not compile): use
  `math_inf()` / `math_nan()`.
* Builtins need matching types: `min(v, vec2<f32>(0.5))`, `clamp(v, vec3<f32>(0.0), vec3<f32>(1.0))`
  (`mix(vec, vec, scalar)` and `vec * scalar` are fine).
* `pow(x, y)` is undefined for `x < 0`: use `java_pow(x, y)` when the base can be negative.
  `smoothstep` is undefined for `e0 >= e1`: use `glsl_smoothstep[2|3]` for reversed/equal edges.
  `round()` rounds ties to even: `java_round` / `glsl_round` give `Math.round`.
* Arrays: `var P: array<vec2<f32>, 9>;` dynamic indexing is fine; `fmod` (prelude) is the
  Java `%` for floats, `glsl_mod` the GLSL `mod`.

## 3. js.glsl: vectors and matrices

| Java                                 | Rust                                | WGSL                              |
|--------------------------------------|-------------------------------------|-----------------------------------|
| `new vec2(x, y)` / `new vec2(s)`     | `vec2(x, y)` / `Vec2::splat(s)`     | `vec2<f32>(x, y)` / `vec2<f32>(s)` |
| `new vec3(v2, z)`, `new vec4(v3, w)` | `Vec3::from_vec2(v2, z)`, `Vec4::from_vec3(v3, w)` | `vec3<f32>(v2, z)`, `vec4<f32>(v3, w)` |
| `new vec4(v2a, v2b)`                 | `Vec4::from_vec2s(a, b)`            | `vec4<f32>(a, b)`                 |
| `a.plus(b)`, `a.add(b)`, `a.minus(b)` | `a + b`, `a - b` (`b` vector or `f64`) | `a + b`, `a - b`               |
| `a.multiply(b)`, `a.division(b)`     | `a * b`, `a / b`                    | `a * b`, `a / b`                  |
| `new vec2(v.y, v.x)`                 | `v.yx()` (also `xy xz yz zy xyz zw ...`) | `v.yx`                       |
| `new mat2(a00, a10, a01, a11)` (column-major) | `mat2(a00, a10, a01, a11)` | `mat2x2<f32>(a00, a10, a01, a11)` |
| `new mat3(c0, c1, c2)`               | `Mat3::from_cols(c0, c1, c2)`       | `mat3x3<f32>(c0, c1, c2)`         |
| `m.times(v)` / `v.times(m)`          | `m * v` / `v * m`                   | `m * v` / `v * m`                 |
| `m.times(f)`, `m.add(f)`             | `m * f`, `m + f`                    | `m * f`, `m + mat2x2<f32>(f, f, f, f)` |
| `vec4.times(mat4)` (buggy indices)   | `v * m` (bug reproduced)            | `glsl_vec4_times_mat4(v, m)`      |

Java `vec3`/`vec4` also have separate fields `r,g,b(,a)` that are **copies taken by the
constructor**. Assigning `p.x = ...` does not change `p.r`, and vectors built by
`G.sin/cos/floor/mix/min/max/step/mod/smoothstep/...` (`new vec3(0.0)` + field writes) have
`r = g = b = 0`. Rust/WGSL have only `x,y,z,w` (`v.r()` aliases `v.x`). If a variation reads
`.r/.g/.b` after mutating `.x/.y/.z`, keep the old values in locals:

```java
p = new vec3(t4.x, t4.z, t4.y);      // p.r = t4.x
if (iR == 1) p.x = 1.0 - p.x;        // p.r unchanged!
pVarTP.color = p.r * p.g;
```
```rust
p = vec3(t4.x, t4.z, t4.y);
let (pr, pg) = (p.x, p.y);           // Java p.r / p.g snapshot
if ir == 1 { p.x = 1.0 - p.x; }
v.color = pr * pg;
```

## 4. G.* functions

Builtin names (generic in Rust over `f64`/`Vec2`/`Vec3`/`Vec4`, builtins in WGSL):
`length dot distance normalize cross reflect (G.Reflect) refract abs sign floor ceil fract trunc
sqrt sin cos exp exp2 log2 pow min max clamp mix step smoothstep sinh cosh tanh`.
Scalar arguments are splatted like the Java overloads: `G.max(v, 0.0)` -> `max(v, 0.0)`,
`G.step(0.5, v)` -> `step(0.5, v)`, `G.mix(a, b, t)` -> `mix(a, b, t)`.

Prefixed (identical name in Rust and WGSL):

| Java `G.`                      | Rust / WGSL                     | Notes |
|--------------------------------|---------------------------------|-------|
| `mod(x, y)`                    | `glsl_mod` (WGSL `glsl_mod2/3/4` for vectors) | `x - y*floor(x/y)` |
| `atan2(y, x)`                  | `glsl_atan2`                    | **fast approximation** (error up to 0.07)! `Math.atan2` -> `y.atan2(x)` / `atan2(y, x)` |
| `atan(n, d)`                   | `glsl_atan`                     | `atan(n/d)`, no quadrant |
| `round`                        | `glsl_round` (WGSL `glsl_round2/3`) | `Math.round` |
| `invSqrt`                      | `glsl_inv_sqrt`                 | bit hack |
| `sinh/cosh/tanh(vec2)`         | `glsl_sinh2/cosh2/tanh2`        | component-wise exp, then complex ops |
| `random random2 hash3 noise fbm fbm2 iqnoise` | `glsl_random glsl_random2 glsl_hash3 glsl_noise glsl_fbm glsl_fbm2 glsl_iqnoise` | GPU hashes differ (f32 `sin`) |
| `rgb2hsb hsb2rgb rgb2hsv hsv2rgb` | `glsl_rgb2hsb glsl_hsb2rgb glsl_rgb2hsv glsl_hsv2rgb` | `hsb2rgb` = iq's smoothed HSV |
| `Kscope rot app rotate2D tile brickTile truchetPattern box skew simplexGrid plot circle fractal colorize v B F f` | `glsl_kscope glsl_rot glsl_app glsl_rotate2d glsl_tile glsl_brick_tile glsl_truchet_pattern glsl_box glsl_skew glsl_simplex_grid glsl_plot glsl_circle glsl_fractal glsl_colorize glsl_v glsl_b glsl_big_f glsl_f` | |
| `ccon cmul cdiv cabs carg cpow cexp clog` | `glsl_ccon glsl_cmul ...`  | `carg` = `atan(y/x)` |
| `c_add c_sub c_mul c_div c_inv c_conj c_exp c_ln c_log c_sqrt c_sin ... c_atanh c_rem c_eq c_one c_i c_ni` | same names | `c_to_polar` uses `atan(y/x)` |
| `c_exp(double, vec2)`, `c_pow(vec2, vec2)`, `arg(c)` | `c_exp_base`, `c_pow_c`, `glsl_arg` | |
| `texture`                      | not available                   | |
| `GLSLFunc/DC_BaseFunc.dbl2int`, `greyscale`, `findKey` | `glsl_dbl2int`, `dc_greyscale`, `dc_find_key` | |

Java variations often define their **own** `noise()`, `fbm()`, `hash()`... methods. An
unqualified call uses the variation's method, only `G.noise(...)` means the library. Port local
helpers as `fn <varname>_noise` (Rust: plain private fn; WGSL: `@body` helper).

## 5. Complex numbers

`org.jwildfire.base.mathlib.Complex` -> `Complex` (in place, snake_case; full table in
`complex.rs`), WGSL `cx_<name>` as pure functions:

```java
Complex z = new Complex(pAffineTP.x, pAffineTP.y);
Complex w = new Complex(z);  w.Sqr(); w.Dec();
z.Div(w); z.AtanH(); z.Scale(pAmount * M_2_PI);
pVarTP.x += z.re;  pVarTP.y += z.im;
```
```rust
let mut z = Complex::new(a.x, a.y);
let mut w = Complex::copy_of(&z); w.sqr(); w.dec();
z.div(&w); z.atanh(); z.scale(amount * M_2_PI);
v.x += z.re; v.y += z.im;
```
```wgsl
var z = vec2<f32>(a.x, a.y);
let w = cx_dec(cx_sqr(z));
z = cx_scale(cx_atanh(cx_div(z, w)), AMOUNT * 2.0 / PI);
v.x += z.x; v.y += z.y;
```

* `z.Mul(z)` -> `let t = z; z.mul(&t);` (borrow rules). Java aliasing (`Complex a = b;`) means one
  object: mutate the original instead of a Rust copy.
* `Save/Restore/Keep/Recall` exist on CPU; in WGSL keep the saved value in a local
  (`z.NextPow()` -> `z = cx_next_pow(z, saved)`).
* Kept Java quirks: `Cos()` returns `i*cos(z)`; `AsecH`/`AcosecH` are `asinh(1/z)`/`acosh(1/z)`.
* `js.colordomain.Complex` (immutable, many aliases) -> `CdComplex` (alias table in
  `complex.rs`); no WGSL library (its users compile formulas at runtime).

## 6. Noise and Voronoi

`NoiseTools.simplexNoise3D(V)` -> `noise::simplex_noise3d([x, y, z])` / `noise_simplex3d(vec3)`,
`perlinNoise3D` -> `noise::perlin_noise3d` / `noise_perlin3d`.
`VoronoiTools.closest(P, n, U)` -> `noise::closest(&p[..n], u)` / `noise_closest(P, n, U)` with
`var P: array<vec2<f32>, NOISE_VORONOI_MAXPOINTS>`; `voronoi(P, n, q, U)` likewise; points are
`[f64; 2]`.

## 7. MathLib / Tools / GfxMathLib (`crate::math`)

| Java | Rust | WGSL |
|------|------|------|
| `sin cos tan exp log sqrt atan2 floor fabs pow(x,y)` | `x.sin() ... y.atan2(x) x.abs() x.powf(y)` | builtins |
| `sinAndCos(a, s, c)` | `let (s, c) = a.sin_cos();` | `sin(a)`, `cos(a)` |
| `sqr(x)`, `fmod(a, b)` (`%`), `frac(x)` (`x - trunc(x)`) | `sqr`, `fmod`, `frac` | `sqr`, `fmod`, `x - trunc(x)` |
| `MathLib.round` / `Math.round` | `java_round` | `java_round` |
| `rint`, `trunc`, `sign` | `rint`, `x.trunc()`, `sgn` | `round`, `trunc`, `sign` |
| `Math.min/max` (NaN-propagating) | `java_min/java_max` (or `a.min(b)`) | `min/max` |
| `Tools.FTOI(x)` | `ftoi(x)` -> `i32` | `ftoi(x)` -> `i32` |
| `Tools.limitValue(v, lo, hi)` | `limit_value(v, lo, hi)` (f64 or i32) | `limit_value` / `limit_value_i` |
| `MathLib.erf/lgamma/acosh` | `erf/lgamma/acosh` | `erf/lgamma/acosh` |
| `GfxMathLib.lerp/blerp/clamp/smoothstep/smootherstep/smootheststep/step` | `gfx_lerp gfx_blerp gfx_clamp gfx_smoothstep gfx_smootherstep gfx_smootheststep gfx_step` | `gfx_lerp gfx_blerp clamp smoothstep gfx_smootherstep gfx_smootheststep step` |
| `M_PI M_2PI M_PI_2 M_PI_4 M_1_PI M_2_PI M_1_2PI M_SQRT2 M_E EPSILON SMALL_EPSILON` | same names | `PI TWO_PI HALF_PI ... EPSILON EPS` |

`VecMathLib` (`VectorD`, `Matrix3D`) has no port yet (3 users): inline the arithmetic.

## 8. Example: a GLSL variation (`GLSLFunc` pattern)

```java
int i = (int) (pContext.random() * resolutionX);
int j = (int) (pContext.random() * resolutionY);
vec3 color = getRGBColor(i, j);            // uses G.Kscope, G.mix, G.dot ...
int[] tcolor = dbl2int(color);
pVarTP.rgbColor = true; pVarTP.redColor = tcolor[0]; ...
pVarTP.x += pAmount * ((double) i / resolutionX - 0.5);
pVarTP.y += pAmount * ((double) j / resolutionY - 0.5);
```
```rust
fn glsl_example_rgb(scale: f64, sides: f64, i: i32, j: i32, res: f64) -> Vec3 {
    let uv = vec2(i as f64 + 0.5, j as f64 + 0.5) / res * 2.0 - 1.0;
    let k = M_PI / sides.floor();
    let s = glsl_kscope(uv * (5.5 - scale), k);
    let t = glsl_kscope(s, k);
    let u = mix(s, t, dot(t, s).cos());
    vec3(fract(u.x), fract(u.y), 0.5)
}
// apply: d = [resolution_x, scale, sides]
let res = d[0];
let i = (ctx.random() * res) as i32;
let j = (ctx.random() * res) as i32;
let c = glsl_dbl2int(glsl_example_rgb(d[1], d[2], i, j, res));
v.rgb_color = true;
v.red = c[0] as f64;
v.green = c[1] as f64;
v.blue = c[2] as f64;
v.x += amount * (i as f64 / res - 0.5);
v.y += amount * (j as f64 / res - 0.5);
```
```wgsl
fn glsl_example_rgb(scale: f32, sides: f32, i: f32, j: f32, res: f32) -> vec3<f32> {
  let uv = (vec2<f32>(i, j) + 0.5) / res * 2.0 - 1.0;
  let k = PI / floor(sides);
  let s = glsl_kscope(uv * (5.5 - scale), k);
  let t = glsl_kscope(s, k);
  let u = mix(s, t, cos(dot(t, s)));
  return vec3<f32>(fract(u.x), fract(u.y), 0.5);
}
@body
let res = D(0);
let i = floor(rnd() * res);
let j = floor(rnd() * res);
let c = glsl_dbl2int(glsl_example_rgb(D(1), D(2), i, j, res));
v.rgbc = 1.0;
v.r = c.x;
v.g = c.y;
v.b = c.z;
v.x += AMOUNT * (i / res - 0.5);
v.y += AMOUNT * (j / res - 0.5);
```
(The helper name must start with the variation name — here the variation is `glsl_example`.)
`resolutionX = 1000000` exceeds f32 precision for `i`; that is acceptable (GPU output is a
sampling of the same pattern), but don't rely on integer exactness above 2^24 on the GPU.

## 9. Example: `DC_BaseFunc` subclasses

Most `DC_*` variations only override `getRGBColor(x, y)`; the shared tail of `transform` is
`dc_output`. Their parameters are the variation's own followed by
`ColorOnly, Gradient, scale_z, offset_z, reset_z` (`joinArrays(additionalParamNames, paramNames)`).

```rust
// d = [<own params...>(n), color_only, gradient, scale_z, offset_z, reset_z]
let uv = if d[N] as i32 == 1 { vec2(a.x, a.y) } else { vec2(ctx.random() - 0.5, ctx.random() - 0.5) };
let color = dc_example_rgb(d, uv);
dc_output(ctx, v, amount, uv, color, d[N + 1] as i32, d[N + 2], d[N + 3], d[N + 4] as i32);
```
```wgsl
var uv = vec2<f32>(rnd() - 0.5, rnd() - 0.5);
if (i32(D(5)) == 1) { uv = vec2<f32>(a.x, a.y); }
dc_output(&v, AMOUNT, uv, dc_example_rgb(uv, D(0)), i32(D(6)), D(7), D(8), i32(D(9)));
```
Check each subclass's `transform`: some override it (e.g. different `uV` sampling ranges).
