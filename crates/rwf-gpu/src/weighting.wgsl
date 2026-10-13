// Xform weighting fields (mirrors rwf_core::fastnoise / rwf_core::weighting).
// WF_TABLES: start of the gradient (16 x 3) and cell (256 x 3) tables in prm.
// A field block at prm[b]: type, seed, frequency, octaves, lacunarity, gain, fractal bounding,
// fractal type, cellular distance, cellular return, image x/y centre, x/y size, width, height,
// pixel start (relative to the block).

// Tools.FTOI
fn wf_ftoi(v: f32) -> i32 {
  if (v > 0.0) { return i32(v + 0.5); }
  if (v < 0.0) { return i32(v - 0.5); }
  return 0;
}

fn wf_floor(f: f32) -> i32 {
  if (f >= 0.0) { return i32(f); }
  return i32(f) - 1;
}

fn wf_round(f: f32) -> i32 {
  if (f >= 0.0) { return i32(f + 0.5); }
  return i32(f - 0.5);
}

fn wf_hash3(seed: i32, x: i32, y: i32, z: i32) -> i32 {
  var h = seed;
  h = h ^ (1619 * x);
  h = h ^ (31337 * y);
  h = h ^ (6971 * z);
  h = h * h * h * 60493;
  return (h >> 13u) ^ h;
}

fn wf_val_coord3(seed: i32, x: i32, y: i32, z: i32) -> f32 {
  var n = seed;
  n = n ^ (1619 * x);
  n = n ^ (31337 * y);
  n = n ^ (6971 * z);
  return f32(n * n * n * 60493) / 2147483648.0;
}

fn wf_grad3(i: i32) -> vec3<f32> {
  let k = WF_TABLES + 3u * u32(i);
  return vec3<f32>(prm[k], prm[k + 1u], prm[k + 2u]);
}

fn wf_cell3(i: i32) -> vec3<f32> {
  let k = WF_TABLES + 48u + 3u * u32(i);
  return vec3<f32>(prm[k], prm[k + 1u], prm[k + 2u]);
}

fn wf_grad_coord3(seed: i32, x: i32, y: i32, z: i32, xd: f32, yd: f32, zd: f32) -> f32 {
  let g = wf_grad3(wf_hash3(seed, x, y, z) & 15);
  return xd * g.x + yd * g.y + zd * g.z;
}

fn wf_quintic(t: f32) -> f32 {
  return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

fn wf_lerp(a: f32, b: f32, t: f32) -> f32 {
  return a + t * (b - a);
}

fn wf_cubic_lerp(a: f32, b: f32, c: f32, d: f32, t: f32) -> f32 {
  let p = (d - c) - (a - b);
  return t * t * t * p + t * t * ((a - b) - p) + t * (c - a) + b;
}

fn wf_value3(seed: i32, x: f32, y: f32, z: f32) -> f32 {
  let x0 = wf_floor(x);
  let y0 = wf_floor(y);
  let z0 = wf_floor(z);
  let xs = wf_quintic(x - f32(x0));
  let ys = wf_quintic(y - f32(y0));
  let zs = wf_quintic(z - f32(z0));
  let xf00 = wf_lerp(wf_val_coord3(seed, x0, y0, z0), wf_val_coord3(seed, x0 + 1, y0, z0), xs);
  let xf10 = wf_lerp(wf_val_coord3(seed, x0, y0 + 1, z0), wf_val_coord3(seed, x0 + 1, y0 + 1, z0), xs);
  let xf01 = wf_lerp(wf_val_coord3(seed, x0, y0, z0 + 1), wf_val_coord3(seed, x0 + 1, y0, z0 + 1), xs);
  let xf11 = wf_lerp(wf_val_coord3(seed, x0, y0 + 1, z0 + 1), wf_val_coord3(seed, x0 + 1, y0 + 1, z0 + 1), xs);
  return wf_lerp(wf_lerp(xf00, xf10, ys), wf_lerp(xf01, xf11, ys), zs);
}

fn wf_perlin3(seed: i32, x: f32, y: f32, z: f32) -> f32 {
  let x0 = wf_floor(x);
  let y0 = wf_floor(y);
  let z0 = wf_floor(z);
  let xs = wf_quintic(x - f32(x0));
  let ys = wf_quintic(y - f32(y0));
  let zs = wf_quintic(z - f32(z0));
  let xd0 = x - f32(x0);
  let yd0 = y - f32(y0);
  let zd0 = z - f32(z0);
  let xd1 = xd0 - 1.0;
  let yd1 = yd0 - 1.0;
  let zd1 = zd0 - 1.0;
  let xf00 = wf_lerp(wf_grad_coord3(seed, x0, y0, z0, xd0, yd0, zd0), wf_grad_coord3(seed, x0 + 1, y0, z0, xd1, yd0, zd0), xs);
  let xf10 = wf_lerp(wf_grad_coord3(seed, x0, y0 + 1, z0, xd0, yd1, zd0), wf_grad_coord3(seed, x0 + 1, y0 + 1, z0, xd1, yd1, zd0), xs);
  let xf01 = wf_lerp(wf_grad_coord3(seed, x0, y0, z0 + 1, xd0, yd0, zd1), wf_grad_coord3(seed, x0 + 1, y0, z0 + 1, xd1, yd0, zd1), xs);
  let xf11 = wf_lerp(wf_grad_coord3(seed, x0, y0 + 1, z0 + 1, xd0, yd1, zd1), wf_grad_coord3(seed, x0 + 1, y0 + 1, z0 + 1, xd1, yd1, zd1), xs);
  return wf_lerp(wf_lerp(xf00, xf10, ys), wf_lerp(xf01, xf11, ys), zs);
}

fn wf_simplex_corner(seed: i32, xd: f32, yd: f32, zd: f32, ci: i32, cj: i32, ck: i32) -> f32 {
  var t = 0.6 - xd * xd - yd * yd - zd * zd;
  if (t < 0.0) { return 0.0; }
  t = t * t;
  return t * t * wf_grad_coord3(seed, ci, cj, ck, xd, yd, zd);
}

fn wf_simplex3(seed: i32, x: f32, y: f32, z: f32) -> f32 {
  let F3 = 1.0 / 3.0;
  let G3 = 1.0 / 6.0;
  let G33 = G3 * 3.0 - 1.0;
  var t = (x + y + z) * F3;
  let i = wf_floor(x + t);
  let j = wf_floor(y + t);
  let k = wf_floor(z + t);
  t = f32(i + j + k) * G3;
  let x0 = x - (f32(i) - t);
  let y0 = y - (f32(j) - t);
  let z0 = z - (f32(k) - t);
  var o1 = vec3<i32>(0, 0, 1);
  var o2 = vec3<i32>(0, 1, 1);
  if (x0 >= y0) {
    if (y0 >= z0) { o1 = vec3<i32>(1, 0, 0); o2 = vec3<i32>(1, 1, 0); }
    else if (x0 >= z0) { o1 = vec3<i32>(1, 0, 0); o2 = vec3<i32>(1, 0, 1); }
    else { o1 = vec3<i32>(0, 0, 1); o2 = vec3<i32>(1, 0, 1); }
  } else {
    if (y0 < z0) { o1 = vec3<i32>(0, 0, 1); o2 = vec3<i32>(0, 1, 1); }
    else if (x0 < z0) { o1 = vec3<i32>(0, 1, 0); o2 = vec3<i32>(0, 1, 1); }
    else { o1 = vec3<i32>(0, 1, 0); o2 = vec3<i32>(1, 1, 0); }
  }
  let n0 = wf_simplex_corner(seed, x0, y0, z0, i, j, k);
  let n1 = wf_simplex_corner(seed, x0 - f32(o1.x) + G3, y0 - f32(o1.y) + G3, z0 - f32(o1.z) + G3, i + o1.x, j + o1.y, k + o1.z);
  let n2 = wf_simplex_corner(seed, x0 - f32(o2.x) + F3, y0 - f32(o2.y) + F3, z0 - f32(o2.z) + F3, i + o2.x, j + o2.y, k + o2.z);
  let n3 = wf_simplex_corner(seed, x0 + G33, y0 + G33, z0 + G33, i + 1, j + 1, k + 1);
  return 32.0 * (n0 + n1 + n2 + n3);
}

fn wf_cubic_row(seed: i32, x1: i32, yi: i32, zi: i32, tx: f32) -> f32 {
  return wf_cubic_lerp(wf_val_coord3(seed, x1 - 1, yi, zi), wf_val_coord3(seed, x1, yi, zi), wf_val_coord3(seed, x1 + 1, yi, zi), wf_val_coord3(seed, x1 + 2, yi, zi), tx);
}

fn wf_cubic_plane(seed: i32, x1: i32, y1: i32, zi: i32, tx: f32, ty: f32) -> f32 {
  return wf_cubic_lerp(wf_cubic_row(seed, x1, y1 - 1, zi, tx), wf_cubic_row(seed, x1, y1, zi, tx), wf_cubic_row(seed, x1, y1 + 1, zi, tx), wf_cubic_row(seed, x1, y1 + 2, zi, tx), ty);
}

fn wf_cubic3(seed: i32, x: f32, y: f32, z: f32) -> f32 {
  let x1 = wf_floor(x);
  let y1 = wf_floor(y);
  let z1 = wf_floor(z);
  let tx = x - f32(x1);
  let ty = y - f32(y1);
  let tz = z - f32(z1);
  return wf_cubic_lerp(wf_cubic_plane(seed, x1, y1, z1 - 1, tx, ty), wf_cubic_plane(seed, x1, y1, z1, tx, ty), wf_cubic_plane(seed, x1, y1, z1 + 1, tx, ty), wf_cubic_plane(seed, x1, y1, z1 + 2, tx, ty), tz) * (1.0 / (1.5 * 1.5 * 1.5));
}

// base: 0 value, 1 perlin, 2 simplex, 3 cubic
fn wf_single(base: i32, seed: i32, x: f32, y: f32, z: f32) -> f32 {
  switch base {
    case 0: { return wf_value3(seed, x, y, z); }
    case 1: { return wf_perlin3(seed, x, y, z); }
    case 2: { return wf_simplex3(seed, x, y, z); }
    default: { return wf_cubic3(seed, x, y, z); }
  }
}

fn wf_fractal(b: u32, base: i32, x_in: f32, y_in: f32, z_in: f32) -> f32 {
  var seed = i32(prm[b + 1u]);
  let octaves = i32(prm[b + 3u]);
  let lacunarity = prm[b + 4u];
  let gain = prm[b + 5u];
  let ftype = i32(prm[b + 7u]);
  var x = x_in;
  var y = y_in;
  var z = z_in;
  var amp = 1.0;
  var sum = 0.0;
  if (ftype == 0) {
    sum = wf_single(base, seed, x, y, z);
  } else if (ftype == 1) {
    sum = abs(wf_single(base, seed, x, y, z)) * 2.0 - 1.0;
  } else {
    sum = 1.0 - abs(wf_single(base, seed, x, y, z));
  }
  for (var i = 1; i < octaves; i++) {
    x *= lacunarity;
    y *= lacunarity;
    z *= lacunarity;
    amp *= gain;
    seed += 1;
    if (ftype == 0) {
      sum += wf_single(base, seed, x, y, z) * amp;
    } else if (ftype == 1) {
      sum += (abs(wf_single(base, seed, x, y, z)) * 2.0 - 1.0) * amp;
    } else {
      sum -= (1.0 - abs(wf_single(base, seed, x, y, z))) * amp;
    }
  }
  if (ftype == 2) { return sum; }
  return sum * prm[b + 6u];
}

fn wf_cell_distance(seed: i32, dist_fn: i32, xi: i32, yi: i32, zi: i32, x: f32, y: f32, z: f32) -> f32 {
  let c = wf_cell3(wf_hash3(seed, xi, yi, zi) & 255);
  let v = vec3<f32>(f32(xi) - x + c.x, f32(yi) - y + c.y, f32(zi) - z + c.z);
  if (dist_fn == 0) { return v.x * v.x + v.y * v.y + v.z * v.z; }
  if (dist_fn == 1) { return abs(v.x) + abs(v.y) + abs(v.z); }
  return (abs(v.x) + abs(v.y) + abs(v.z)) + (v.x * v.x + v.y * v.y + v.z * v.z);
}

// return types: 0 cell value, 1 distance, 2 distance2, 3 add, 4 sub, 5 mul, 6 div
fn wf_cellular(b: u32, x: f32, y: f32, z: f32) -> f32 {
  let seed = i32(prm[b + 1u]);
  let dist_fn = i32(prm[b + 8u]);
  let ret = i32(prm[b + 9u]);
  let xr = wf_round(x);
  let yr = wf_round(y);
  let zr = wf_round(z);
  var d1 = 999999.0;
  var d2 = 999999.0;
  var c = vec3<i32>(0);
  for (var xi = xr - 1; xi <= xr + 1; xi++) {
    for (var yi = yr - 1; yi <= yr + 1; yi++) {
      for (var zi = zr - 1; zi <= zr + 1; zi++) {
        let nd = wf_cell_distance(seed, dist_fn, xi, yi, zi, x, y, z);
        if (ret <= 1) {
          if (nd < d1) { d1 = nd; c = vec3<i32>(xi, yi, zi); }
        } else {
          d2 = max(min(d2, nd), d1);
          d1 = min(d1, nd);
        }
      }
    }
  }
  switch ret {
    case 0: { return wf_val_coord3(0, c.x, c.y, c.z); }
    case 1: { return d1 - 1.0; }
    case 2: { return d2 - 1.0; }
    case 3: { return d2 + d1 - 1.0; }
    case 4: { return d2 - d1 - 1.0; }
    case 5: { return d2 * d1 - 1.0; }
    default: { return d1 / d2 - 1.0; }
  }
}

fn wf_image_red(b: u32, ix: i32, iy: i32) -> f32 {
  let w = i32(prm[b + 14u]);
  let h = i32(prm[b + 15u]);
  if (ix < 0 || ix >= w || iy < 0 || iy >= h) { return 0.0; }
  return prm[b + u32(prm[b + 16u]) + u32(iy * w + ix)];
}

// types: 1 cellular, 2 cubic, 3 cubic fractal, 4 perlin, 5 perlin fractal, 6 simplex,
// 7 simplex fractal, 8 value, 9 value fractal, 10 white, 11 image map
fn wf_value(b: u32, x_in: f32, y_in: f32, z_in: f32) -> f32 {
  let t = i32(prm[b]);
  if (t == 11) {
    let xs = prm[b + 12u];
    let ys = prm[b + 13u];
    let xc = (x_in + xs * 0.5 - prm[b + 10u]) / xs * prm[b + 14u];
    let yc = (y_in + ys * 0.5 - prm[b + 11u]) / ys * prm[b + 15u];
    let ix = i32(xc);
    let iy = i32(yc);
    let px = xc - trunc(xc);
    let py = yc - trunc(yc);
    let top = wf_lerp(wf_image_red(b, ix, iy), wf_image_red(b, ix + 1, iy), px);
    let bottom = wf_lerp(wf_image_red(b, ix, iy + 1), wf_image_red(b, ix + 1, iy + 1), px);
    return 0.5 - wf_lerp(top, bottom, py) / 255.0;
  }
  let f = prm[b + 2u];
  let x = x_in * f;
  let y = y_in * f;
  let z = z_in * f;
  let seed = i32(prm[b + 1u]);
  switch t {
    case 1: { return wf_cellular(b, x, y, z); }
    case 2: { return wf_cubic3(seed, x, y, z); }
    case 3: { return wf_fractal(b, 3, x, y, z); }
    case 4: { return wf_perlin3(seed, x, y, z); }
    case 5: { return wf_fractal(b, 1, x, y, z); }
    case 6: { return wf_simplex3(seed, x, y, z); }
    case 7: { return wf_fractal(b, 2, x, y, z); }
    case 8: { return wf_value3(seed, x, y, z); }
    case 9: { return wf_fractal(b, 0, x, y, z); }
    case 10: {
      let xb = bitcast<i32>(x);
      let yb = bitcast<i32>(y);
      let zb = bitcast<i32>(z);
      return wf_val_coord3(seed, xb ^ (xb >> 16u), yb ^ (yb >> 16u), zb ^ (zb >> 16u));
    }
    default: { return 0.0; }
  }
}
