// ---- glsl_complex.wgsl: G.c_* complex helpers on vec2 (mirrors glsl.rs) --------------------
// Same quirks as Java: c_to_polar (and c_ln, c_log, c_sqrt, c_pow, ...) uses atan(y / x).

fn c_one() -> vec2<f32> { return vec2<f32>(1.0, 0.0); }
fn c_i() -> vec2<f32> { return vec2<f32>(0.0, 1.0); }
fn c_ni() -> vec2<f32> { return vec2<f32>(0.0, -1.0); }
fn glsl_arg(c: vec2<f32>) -> f32 { return atan(c.y / c.x); }
fn c_conj(c: vec2<f32>) -> vec2<f32> { return vec2<f32>(c.x, -c.y); }
fn c_from_polar(r: f32, theta: f32) -> vec2<f32> { return vec2<f32>(r * cos(theta), r * sin(theta)); }
fn c_to_polar(c: vec2<f32>) -> vec2<f32> { return vec2<f32>(length(c), atan(c.y / c.x)); }
fn c_exp(c: vec2<f32>) -> vec2<f32> { return c_from_polar(exp(c.x), c.y); }
// G.c_exp(double base, vec2 c)
fn c_exp_base(base: f32, c: vec2<f32>) -> vec2<f32> { return c_from_polar(pow(base, c.x), c.y * log(base)); }
fn c_ln(c: vec2<f32>) -> vec2<f32> {
  let p = c_to_polar(c);
  return vec2<f32>(log(p.x), p.y);
}
fn c_log(c: vec2<f32>, base: f32) -> vec2<f32> {
  let p = c_to_polar(c);
  return vec2<f32>(log(p.x) / log(base), p.y / log(base));
}
fn c_sqrt(c: vec2<f32>) -> vec2<f32> {
  let p = c_to_polar(c);
  return c_from_polar(sqrt(p.x), p.y / 2.0);
}
// G.c_pow(vec2, double)
fn c_pow(c: vec2<f32>, e: f32) -> vec2<f32> {
  let p = c_to_polar(c);
  return c_from_polar(pow(p.x, e), p.y * e);
}
// G.c_pow(vec2, vec2)
fn c_pow_c(c: vec2<f32>, e: vec2<f32>) -> vec2<f32> {
  let p = c_to_polar(c);
  return c_from_polar(pow(p.x, e.x) * exp(-e.y * p.y), e.x * p.y + e.y * log(p.x));
}
fn c_add(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> { return a + b; }
fn c_sub(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> { return a - b; }
fn c_mul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}
fn c_div(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
  let n = length(b);
  return vec2<f32>((a.x * b.x + a.y * b.y) / (n * n), (a.y * b.x - a.x * b.y) / (n * n));
}
fn c_sin(c: vec2<f32>) -> vec2<f32> { return vec2<f32>(sin(c.x) * cosh(c.y), cos(c.x) * sinh(c.y)); }
fn c_cos(c: vec2<f32>) -> vec2<f32> { return vec2<f32>(cos(c.x) * cosh(c.y), -sin(c.x) * sinh(c.y)); }
fn c_tan(c: vec2<f32>) -> vec2<f32> {
  let c2 = c * 2.0;
  return vec2<f32>(sin(c2.x), sinh(c2.y)) / (cos(c2.x) + cosh(c2.y));
}
fn c_eq(a: vec2<f32>, b: vec2<f32>) -> bool { return a.x == b.x && a.y == b.y; }
fn c_atan(c: vec2<f32>) -> vec2<f32> {
  let i = c_i();
  let one = c_one();
  if (c_eq(c, i)) { return vec2<f32>(0.0, math_inf()); }
  if (c_eq(c, c_ni())) { return vec2<f32>(0.0, -math_inf()); }
  return c_div(c_sub(c_ln(c_add(one, c_mul(i, c))), c_ln(c_sub(one, c_mul(i, c)))), c_mul(one + one, i));
}
fn c_asin(c: vec2<f32>) -> vec2<f32> {
  return c_mul(c_ni(), c_ln(c_add(c_sqrt(c_sub(c_one(), c_mul(c, c))), c_mul(c_i(), c))));
}
fn c_acos(c: vec2<f32>) -> vec2<f32> {
  return c_mul(c_ni(), c_ln(c_add(c_mul(c_i(), c_sqrt(c_sub(c_one(), c_mul(c, c)))), c)));
}
fn c_sinh(c: vec2<f32>) -> vec2<f32> { return vec2<f32>(sinh(c.x) * cos(c.y), cosh(c.x) * sin(c.y)); }
fn c_cosh(c: vec2<f32>) -> vec2<f32> { return vec2<f32>(cosh(c.x) * cos(c.y), sinh(c.x) * sin(c.y)); }
fn c_tanh(c: vec2<f32>) -> vec2<f32> {
  let c2 = 2.0 * c;
  let d = cosh(c2.x) + cos(c2.y);
  return vec2<f32>(sinh(c2.x) / d, sin(c2.y) / d);
}
fn c_asinh(c: vec2<f32>) -> vec2<f32> { return c_ln(c_add(c, c_sqrt(c_add(c_one(), c_mul(c, c))))); }
fn c_acosh(c: vec2<f32>) -> vec2<f32> {
  let one = c_one();
  let two = one + one;
  return c_mul(two, c_ln(c_add(c_sqrt(c_div(c_add(c, one), two)), c_sqrt(c_div(c_sub(c, one), two)))));
}
fn c_atanh(c: vec2<f32>) -> vec2<f32> {
  let one = c_one();
  if (c_eq(c, one)) { return vec2<f32>(math_inf(), 0.0); }
  if (c_eq(c, -one)) { return vec2<f32>(-math_inf(), 0.0); }
  return c_div(c_sub(c_ln(c_add(one, c)), c_ln(c_sub(one, c))), one + one);
}
fn c_rem(c: vec2<f32>, modulus: vec2<f32>) -> vec2<f32> {
  let c0 = c_div(c, modulus);
  let c1 = vec2<f32>(c0.x - glsl_mod(c0.x, 1.0), c0.y - glsl_mod(c0.y, 1.0));
  return c - c_mul(modulus, c1);
}
fn c_inv(c: vec2<f32>) -> vec2<f32> {
  let n = length(c);
  return vec2<f32>(c.x / (n * n), -c.y / (n * n));
}
