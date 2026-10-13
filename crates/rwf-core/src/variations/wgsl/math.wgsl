// ---- math.wgsl: Java / MathLib / Tools / GfxMathLib helpers (mirrors crate::math) ----------

// +inf / NaN produced at runtime (const-expressions must not evaluate to inf or NaN).
fn math_inf() -> f32 {
  var bits = 0x7f800000u;
  return bitcast<f32>(bits);
}
fn math_nan() -> f32 {
  var bits = 0x7fc00000u;
  return bitcast<f32>(bits);
}

// Tools.FTOI: round half away from zero.
fn ftoi(v: f32) -> i32 {
  if (v > 0.0) { return i32(v + 0.5); }
  if (v < 0.0) { return i32(-(abs(v) + 0.5)); }
  return 0;
}

// Tools.limitValue (double / int overloads).
fn limit_value(v: f32, lo: f32, hi: f32) -> f32 {
  if (v < lo) { return lo; }
  if (v > hi) { return hi; }
  return v;
}
fn limit_value_i(v: i32, lo: i32, hi: i32) -> i32 {
  if (v < lo) { return lo; }
  if (v > hi) { return hi; }
  return v;
}

// Math.round / MathLib.round: ties towards +inf (WGSL round() rounds ties to even).
fn java_round(x: f32) -> f32 {
  let f = floor(x);
  return select(f, f + 1.0, x - f >= 0.5);
}

// Math.pow semantics for negative bases (WGSL pow() is undefined for x < 0):
// integer exponents give the signed result, other exponents NaN.
fn java_pow(x: f32, y: f32) -> f32 {
  if (x > 0.0) { return pow(x, y); }
  if (y == 0.0) { return 1.0; }
  if (x == 0.0) {
    if (y > 0.0) { return 0.0; }
    return math_inf();
  }
  if (y != floor(y)) { return math_nan(); }
  let r = pow(-x, y);
  return select(r, -r, fract(y * 0.5) != 0.0);
}

// GfxMathLib.lerp / blerp (s + (e - s) * t).
fn gfx_lerp(s: f32, e: f32, t: f32) -> f32 { return s + (e - s) * t; }
fn gfx_blerp(c00: f32, c10: f32, c01: f32, c11: f32, tx: f32, ty: f32) -> f32 {
  return gfx_lerp(gfx_lerp(c00, c10, tx), gfx_lerp(c01, c11, tx), ty);
}
fn gfx_smootherstep(edge0: f32, edge1: f32, x_in: f32) -> f32 {
  let x = clamp((x_in - edge0) / (edge1 - edge0), 0.0, 1.0);
  return x * x * x * (x * (x * 6.0 - 15.0) + 10.0);
}
fn gfx_smootheststep(edge0: f32, edge1: f32, x_in: f32) -> f32 {
  let x = clamp((x_in - edge0) / (edge1 - edge0), 0.0, 1.0);
  return x * x * x * x * (x * (x * (x * -20.0 + 70.0) - 84.0) + 35.0);
}

// MathLib.erf (Chebyshev approximation).
fn erf(z: f32) -> f32 {
  let t = 1.0 / (1.0 + 0.5 * abs(z));
  let ans = 1.0 - t * exp(-z * z - 1.26551223 + t * (1.00002368 + t * (0.37409196 + t * (0.09678418
    + t * (-0.18628806 + t * (0.27886807 + t * (-1.13520398 + t * (1.48851587
    + t * (-0.82215223 + t * 0.17087277)))))))));
  return select(-ans, ans, z >= 0.0);
}

// MathLib.lgamma
fn lgamma(x: f32) -> f32 {
  let tmp = (x - 0.5) * log(x + 4.5) - (x + 4.5);
  let ser = 1.0 + 76.18009173 / x - 86.50532033 / (x + 1.0) + 24.01409822 / (x + 2.0)
    - 1.231739516 / (x + 3.0) + 0.00120858003 / (x + 4.0) - 0.00000536382 / (x + 5.0);
  return tmp + log(ser * sqrt(TWO_PI));
}
