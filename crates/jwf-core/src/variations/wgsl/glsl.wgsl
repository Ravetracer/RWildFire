// ---- glsl.wgsl: js.glsl.G helpers WGSL lacks (mirrors glsl.rs, same names) -----------------
// Builtins with GLSL semantics (length, dot, fract, mix, step, smoothstep, clamp, floor, sign,
// min, max, abs, normalize, cross, reflect, refract, sinh, cosh, tanh, ...) are used directly.

// G.mod: x - y * floor(x / y)
fn glsl_mod(x: f32, y: f32) -> f32 { return x - y * floor(x / y); }
fn glsl_mod2(x: vec2<f32>, y: vec2<f32>) -> vec2<f32> { return x - y * floor(x / y); }
fn glsl_mod3(x: vec3<f32>, y: vec3<f32>) -> vec3<f32> { return x - y * floor(x / y); }
fn glsl_mod4(x: vec4<f32>, y: vec4<f32>) -> vec4<f32> { return x - y * floor(x / y); }

// G.atan(n, d) = atan(n / d) (no quadrant correction).
fn glsl_atan(n: f32, d: f32) -> f32 { return atan(n / d); }

// G.atan2(y, x): fast approximation (max. error ~0.07). Use atan2() for Math.atan2.
fn glsl_atan2(y: f32, x: f32) -> f32 {
  let c1 = PI / 4.0;
  let c2 = 3.0 * c1;
  let ay = abs(y);
  var angle: f32;
  if (x >= 0.0) {
    angle = c1 - c1 * ((x - ay) / (x + ay));
  } else {
    angle = c2 - c1 * ((x + ay) / (ay - x));
  }
  return select(angle, -angle, y < 0.0);
}

// G.round = Math.round (ties towards +inf).
fn glsl_round(x: f32) -> f32 { return java_round(x); }
fn glsl_round2(v: vec2<f32>) -> vec2<f32> { return vec2<f32>(java_round(v.x), java_round(v.y)); }
fn glsl_round3(v: vec3<f32>) -> vec3<f32> { return vec3<f32>(java_round(v.x), java_round(v.y), java_round(v.z)); }

// G.smoothstep with the Java formula; also defined for e0 >= e1 (WGSL smoothstep is not).
fn glsl_smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
  let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
  return t * t * (3.0 - 2.0 * t);
}
fn glsl_smoothstep2(e0: vec2<f32>, e1: vec2<f32>, x: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(glsl_smoothstep(e0.x, e1.x, x.x), glsl_smoothstep(e0.y, e1.y, x.y));
}
fn glsl_smoothstep3(e0: vec3<f32>, e1: vec3<f32>, x: vec3<f32>) -> vec3<f32> {
  return vec3<f32>(glsl_smoothstep(e0.x, e1.x, x.x), glsl_smoothstep(e0.y, e1.y, x.y), glsl_smoothstep(e0.z, e1.z, x.z));
}

// G.invSqrt(float): bit hack + one Newton step.
fn glsl_inv_sqrt(x: f32) -> f32 {
  let xhalf = 0.5 * x;
  let y = bitcast<f32>(0x5f3759dfi - (bitcast<i32>(x) >> 1u));
  return y * (1.5 - xhalf * y * y);
}

// G.sinh/cosh/tanh(vec2): component-wise exp, then complex arithmetic (Java quirk).
fn glsl_sinh2(v: vec2<f32>) -> vec2<f32> {
  let ep = exp(v);
  return c_sub(ep, c_div(c_one(), ep)) / 2.0;
}
fn glsl_cosh2(v: vec2<f32>) -> vec2<f32> {
  let ep = exp(v);
  return c_add(ep, c_div(c_one(), ep)) / 2.0;
}
fn glsl_tanh2(v: vec2<f32>) -> vec2<f32> {
  let ep = exp(v);
  return c_div(c_sub(ep, c_div(c_one(), ep)), c_add(ep, c_div(c_one(), ep)));
}

fn glsl_plot(st: vec2<f32>, pct: f32) -> f32 {
  return smoothstep(pct - 0.02, pct, st.y) - smoothstep(pct, pct + 0.02, st.y);
}
fn glsl_circle(st: vec2<f32>, radius: f32) -> f32 {
  let dist = st - 0.5;
  return 1.0 - glsl_smoothstep(radius - radius * 0.01, radius + radius * 0.01, dot(dist, dist) * 4.0);
}
fn glsl_rotate2d(st: vec2<f32>, angle: f32) -> vec2<f32> {
  return mat2x2<f32>(cos(angle), -sin(angle), sin(angle), cos(angle)) * (st - 0.5) + 0.5;
}
fn glsl_tile(st: vec2<f32>, zoom: f32) -> vec2<f32> { return fract(st * zoom); }
fn glsl_brick_tile(st: vec2<f32>, zoom: f32) -> vec2<f32> {
  var t = st * zoom;
  t.x += step(1.0, glsl_mod(t.y, 2.0)) * 0.5;
  return fract(t);
}
fn glsl_truchet_pattern(st: vec2<f32>, index_in: f32) -> vec2<f32> {
  let index = fract((index_in - 0.5) * 2.0);
  if (index > 0.75) { return 1.0 - st; }
  if (index > 0.5) { return vec2<f32>(1.0 - st.x, st.y); }
  if (index > 0.25) { return 1.0 - vec2<f32>(1.0 - st.x, st.y); }
  return st;
}
fn glsl_box(st: vec2<f32>, size_in: vec2<f32>, smooth_edges: f32) -> f32 {
  let size = 0.5 - size_in * 0.5;
  let aa = vec2<f32>(smooth_edges * 0.5);
  var uv = glsl_smoothstep2(size, size + aa, st);
  uv = uv * glsl_smoothstep2(size, size + aa, 1.0 - st);
  return uv.x * uv.y;
}
fn glsl_skew(st: vec2<f32>) -> vec2<f32> {
  let x = 1.1547 * st.x;
  return vec2<f32>(x, st.y + 0.5 * x);
}
fn glsl_simplex_grid(st: vec2<f32>) -> vec3<f32> {
  let p = fract(glsl_skew(st));
  if (p.x > p.y) {
    let t = 1.0 - vec2<f32>(p.x, p.y - p.x);
    return fract(vec3<f32>(t.x, t.y, p.y));
  }
  let t = 1.0 - vec2<f32>(p.x - p.y, p.y);
  return fract(vec3<f32>(p.x, t.x, t.y));
}

// Hashes / noise. Note: sin() of large arguments in f32 differs from the f64 CPU version, so
// the patterns are statistically similar but not identical.
fn glsl_random(st: vec2<f32>) -> f32 {
  return fract(sin(dot(st, vec2<f32>(12.9898, 78.233))) * 43758.5453123);
}
fn glsl_random2(p: vec2<f32>) -> vec2<f32> {
  return fract(sin(vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)))) * 43758.5453);
}
fn glsl_hash3(p: vec2<f32>) -> vec3<f32> {
  let q = vec3<f32>(dot(p, vec2<f32>(127.1, 311.7)), dot(p, vec2<f32>(269.5, 183.3)), dot(p, vec2<f32>(419.2, 371.9)));
  return fract(sin(q) * 43758.5453);
}
fn glsl_iqnoise(x: vec2<f32>, u: f32, v: f32) -> f32 {
  let p = floor(x);
  let f = fract(x);
  let k = 1.0 + 63.0 * pow(1.0 - v, 4.0);
  var va = 0.0;
  var wt = 0.0;
  for (var j = -2; j <= 2; j++) {
    for (var i = -2; i <= 2; i++) {
      let g = vec2<f32>(f32(i), f32(j));
      let o = glsl_hash3(p + g) * vec3<f32>(u, u, 1.0);
      let r = g - f + o.xy;
      let d = dot(r, r);
      let ww = pow(1.0 - smoothstep(0.0, 1.414, sqrt(d)), k);
      va += o.z * ww;
      wt += ww;
    }
  }
  return va / wt;
}
fn glsl_noise(st: vec2<f32>) -> f32 {
  let i = floor(st);
  let f = fract(st);
  let a = glsl_random(i);
  let b = glsl_random(i + vec2<f32>(1.0, 0.0));
  let c = glsl_random(i + vec2<f32>(0.0, 1.0));
  let d = glsl_random(i + vec2<f32>(1.0, 1.0));
  let u = f * f * (3.0 - 2.0 * f);
  return mix(a, b, u.x) + (c - a) * u.y * (1.0 - u.x) + (d - b) * u.x * u.y;
}
fn glsl_fbm(st_in: vec2<f32>) -> f32 {
  var st = st_in;
  var value = 0.0;
  var amplitude = 0.5;
  for (var i = 0; i < 6; i++) {
    value += amplitude * glsl_noise(st);
    st = st * 2.0;
    amplitude *= 0.5;
  }
  return value;
}
fn glsl_fbm2(st_in: vec2<f32>) -> f32 {
  var st = st_in;
  var v = 0.0;
  var a = 0.5;
  let rot = mat2x2<f32>(cos(0.5), sin(0.5), -sin(0.5), cos(0.5));
  for (var i = 0; i < 5; i++) {
    v += a * glsl_noise(st);
    st = rot * st * 2.0 + 100.0;
    a *= 0.5;
  }
  return v;
}

fn glsl_fractal(p: vec2<f32>) -> vec3<f32> {
  var z = vec2<f32>(0.0);
  for (var i = 0; i < 50; i++) {
    z = vec2<f32>(z.x * z.x - z.y * z.y, 2.0 * z.x * z.y) + p;
    if (dot(z, z) > 4.0) {
      let s = 0.125662 * f32(i);
      return vec3<f32>(cos(s + 0.9), cos(s + 0.3), cos(s + 0.2)) * 0.4 + 0.6;
    }
  }
  return vec3<f32>(0.0);
}
fn glsl_b(a: vec2<f32>) -> vec2<f32> { return vec2<f32>(log(length(a)), glsl_atan2(a.y, a.x) - 6.3); }
// G.F(E, time)
fn glsl_big_f(e: vec2<f32>, time: f32) -> vec3<f32> {
  var e_ = e;
  var c = 6.0;
  for (var i = 0; i < 30; i++) {
    e_ = glsl_b(vec2<f32>(e_.x, abs(e_.y))) + vec2<f32>(0.1 * sin(time / 3.0) - 0.1, 5.0 + 0.1 * cos(time / 5.0));
    c += length(e_);
  }
  let d = log2(log2(c * 0.05)) * 6.0;
  return vec3<f32>(0.7 + tan(0.7 * cos(d)), 0.5 + 0.5 * cos(d - 0.7), 0.7 + sin(0.7 * cos(d - 0.7)));
}

// Colors. hsb2rgb is iq's smoothed HSV -> RGB.
fn glsl_rgb2hsb(c: vec3<f32>) -> vec3<f32> {
  let K = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
  let p = mix(vec4<f32>(c.z, c.y, K.w, K.z), vec4<f32>(c.y, c.z, K.x, K.y), step(c.z, c.y));
  let q = mix(vec4<f32>(p.x, p.y, p.w, c.x), vec4<f32>(c.x, p.y, p.z, p.x), step(p.x, c.x));
  let d = q.x - min(q.w, q.y);
  let e = 1.0e-10;
  return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}
fn glsl_hsb2rgb(c: vec3<f32>) -> vec3<f32> {
  var rgb = clamp(abs(glsl_mod3(vec3<f32>(c.x * 6.0) + vec3<f32>(0.0, 4.0, 2.0), vec3<f32>(6.0)) - 3.0) - 1.0,
    vec3<f32>(0.0), vec3<f32>(1.0));
  rgb = rgb * rgb * (3.0 - 2.0 * rgb);
  return c.z * mix(vec3<f32>(1.0), rgb, c.y);
}
fn glsl_rgb2hsv(c: vec4<f32>) -> vec4<f32> {
  let h = glsl_rgb2hsb(c.xyz);
  return vec4<f32>(h, c.w);
}
fn glsl_hsv2rgb(c: vec4<f32>) -> vec4<f32> {
  let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
  let p = abs(fract(vec3<f32>(c.x) + K.xyz) * 6.0 - K.www);
  return vec4<f32>(c.z * mix(K.xxx, clamp(p - K.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y), c.w);
}
fn glsl_colorize(t: f32, a: vec3<f32>, b: vec3<f32>, c: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
  let t1 = cos(vec3<f32>(0.4 * PI) * (c * t + d));
  return 2.5 * a * b * t1;
}

// G.ccon/cmul/cdiv/cabs/carg/cpow/cexp/clog (carg = atan(y / x)).
fn glsl_ccon(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(z.x, -z.y); }
fn glsl_cmul(u: vec2<f32>, v: vec2<f32>) -> vec2<f32> { return vec2<f32>(u.x * v.x - u.y * v.y, u.y * v.x + u.x * v.y); }
fn glsl_cdiv(u: vec2<f32>, v: vec2<f32>) -> vec2<f32> {
  return vec2<f32>(u.x * v.x + u.y * v.y, u.y * v.x - u.x * v.y) * (1.0 / (v.x * v.x + v.y * v.y));
}
fn glsl_cabs(z: vec2<f32>) -> f32 { return length(z); }
fn glsl_carg(z: vec2<f32>) -> f32 { return atan(z.y / z.x); }
fn glsl_cpow(b: vec2<f32>, e: vec2<f32>) -> vec2<f32> {
  let ab = glsl_carg(b);
  let lgb = log(b.x * b.x + b.y * b.y) / 2.0;
  let lr = exp(lgb * e.x - ab * e.y);
  let cis = lgb * e.y + ab * e.x;
  return vec2<f32>(cos(cis) * lr, sin(cis) * lr);
}
fn glsl_cexp(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(cos(z.y), sin(z.y)) * exp(z.x); }
fn glsl_clog(z: vec2<f32>) -> vec2<f32> { return vec2<f32>(log(z.x * z.x + z.y * z.y) / 2.0, glsl_carg(z)); }
// G.f(p, time)
fn glsl_f(p_in: vec2<f32>, time: f32) -> vec2<f32> {
  let p = 2.0 * glsl_cpow(p_in, vec2<f32>(3.0, 0.0)) - 0.1 * p_in
    + vec2<f32>(0.04 + 0.03 * sin(time * 0.2), 0.02 * cos(time * 0.46));
  return glsl_cmul(p, glsl_cexp(vec2<f32>(0.0, time)));
}

// G.Kscope(uv, k): kaleidoscope fold using the glsl_atan2 approximation.
fn glsl_kscope(uv: vec2<f32>, k: f32) -> vec2<f32> {
  let angle = abs(glsl_mod(glsl_atan2(uv.y, uv.x), 2.0 * k) - k);
  return length(uv) * vec2<f32>(cos(angle), sin(angle));
}
fn glsl_v(coord: vec2<f32>, k: f32, s: f32, rot: f32) -> f32 {
  return 0.5 * cos((cos(rot) * coord.x + sin(rot) * coord.y) * k + s);
}
fn glsl_rot(s: vec3<f32>) -> mat3x3<f32> {
  let sa = sin(s.x);
  let ca = cos(s.x);
  let sb = sin(s.y);
  let cb = cos(s.y);
  let sc = sin(s.z);
  let cc = cos(s.z);
  return mat3x3<f32>(
    vec3<f32>(cb * cc, -cb * sc, sb),
    vec3<f32>(sa * sb * cc + ca * sc, -sa * sb * sc + ca * cc, -sa * cb),
    vec3<f32>(-ca * sb * cc + sa * sc, ca * sb * sc + sa * cc, ca * cb));
}
fn glsl_app(v_in: vec3<f32>, k: f32, m: mat3x3<f32>) -> vec3<f32> {
  var v = v_in;
  let m1 = m * k;
  for (var i = 0; i < 50; i++) {
    v = abs((m1 * v) / dot(v, v) * 0.5 - 0.5) * 2.0 - 1.0;
  }
  return v;
}
// Java vec4.times(mat4) with its mixed-up indices (m[col][row]).
fn glsl_vec4_times_mat4(v: vec4<f32>, m: mat4x4<f32>) -> vec4<f32> {
  return vec4<f32>(
    v.x * m[0][0] + v.y * m[1][0] + v.z * m[2][0] + v.w * m[3][0],
    v.x * m[1][0] + v.y * m[1][1] + v.z * m[2][1] + v.w * m[3][1],
    v.x * m[2][0] + v.y * m[1][2] + v.z * m[2][2] + v.w * m[3][2],
    v.x * m[3][0] + v.y * m[1][3] + v.z * m[2][3] + v.w * m[3][3]);
}

// ---- GLSLFunc / DC_BaseFunc helpers -----------------------------------------------------

// dbl2int: 0..1 color -> 0..255 (integral values as f32).
fn glsl_dbl2int(c: vec3<f32>) -> vec3<f32> { return clamp(floor(c * 256.0), vec3<f32>(0.0), vec3<f32>(255.0)); }
// DC_BaseFunc.greyscale of a 0..255 color.
fn dc_greyscale(t: vec3<f32>) -> f32 {
  return (trunc(t.x * 0.299) + trunc(t.y * 0.587) + trunc(t.z * 0.114)) / 255.0;
}
// DC_BaseFunc.findKey: closest gradient color to the 0..255 color t.
fn dc_find_key(t: vec3<f32>) -> vec3<f32> {
  var best = vec3<f32>(0.0);
  var diff = 1.0e9;
  for (var i = 0; i < 256; i++) {
    let c = floor(palette_rgb(i) + 0.5);
    let dv = dot(t - c, t - c);
    if (diff > dv) {
      diff = dv;
      best = c;
    }
  }
  return best;
}
// Tail of DC_BaseFunc.transform (see glsl::dc_output).
fn dc_output(v: ptr<function, Pt>, amount: f32, uv: vec2<f32>, color: vec3<f32>, gradient: i32,
             scale_z: f32, offset_z: f32, reset_z: i32) {
  let t = glsl_dbl2int(color);
  let z = dc_greyscale(t);
  if (gradient == 0) {
    (*v).rgbc = 1.0;
    (*v).r = t.x;
    (*v).g = t.y;
    (*v).b = t.z;
  } else if (gradient == 1) {
    let k = dc_find_key(t);
    (*v).rgbc = 1.0;
    (*v).r = k.x;
    (*v).g = k.y;
    (*v).b = k.z;
  } else {
    (*v).color = z;
  }
  (*v).x += amount * uv.x;
  (*v).y += amount * uv.y;
  let dz = z * scale_z + offset_z;
  if (reset_z == 1) {
    (*v).z = dz;
  } else {
    (*v).z += dz;
  }
}
