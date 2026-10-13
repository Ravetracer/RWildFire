// Resource variations (variations/resource_vars.rs): images and meshes packed into `prm`.
// An image block at prm[base + o]: width, height, start of the packed r << 16 | g << 8 | b pixels
// (relative to base); width 0 = no image.

fn res_pixel(base: u32, o: u32, ix: i32, iy: i32) -> vec3<f32> {
  let w = i32(prm[base + o]);
  let p = u32(prm[base + u32(prm[base + o + 2u]) + u32(iy * w + ix)]);
  return vec3<f32>(f32((p >> 16u) & 255u), f32((p >> 8u) & 255u), f32(p & 255u));
}

// Pixel at trunc(iu, iv), optionally blended with its right/lower neighbours (wrapping);
// vec4.w = 0 outside the image.
fn res_sample(base: u32, o: u32, blend: bool, iu: f32, iv: f32) -> vec4<f32> {
  let w = i32(prm[base + o]);
  let h = i32(prm[base + o + 1u]);
  let ix = i32(trunc(iu));
  let iy = i32(trunc(iv));
  if (ix < 0 || ix >= w || iy < 0 || iy >= h) { return vec4<f32>(0.0); }
  if (!blend) { return vec4<f32>(res_pixel(base, o, ix, iy), 1.0); }
  var ix1 = ix + 1;
  if (ix1 >= w) { ix1 = 0; }
  var iy1 = iy + 1;
  if (iy1 >= h) { iy1 = 0; }
  let fu = iu - trunc(iu);
  let fv = iv - trunc(iv);
  let top = mix(res_pixel(base, o, ix, iy), res_pixel(base, o, ix1, iy), fu);
  let bottom = mix(res_pixel(base, o, ix, iy1), res_pixel(base, o, ix1, iy1), fu);
  return vec4<f32>(mix(top, bottom, fv), 1.0);
}

// ColorMapHolder.applyImageColor: RGB (0..255), black outside the image.
fn res_image_color(base: u32, o: u32, blend: bool, iu: f32, iv: f32) -> vec3<f32> {
  return res_sample(base, o, blend, iu, iv).xyz;
}

// DisplacementMapHolder.calculateImageDisplacement: green channel, 0..1.
fn res_image_displ(base: u32, o: u32, blend: bool, iu: f32, iv: f32) -> f32 {
  return res_sample(base, o, blend, iu, iv).y / 255.0;
}

// UVColorMapper.getUVColorIdx: gradient position of the nearest color of the current layer's
// color map (render palette scale 0..200, like cmap).
fn res_uv_color_idx(rgb: vec3<f32>) -> f32 {
  return res_nearest_color_idx(vec3<f32>(f32(ftoi(rgb.x)), f32(ftoi(rgb.y)), f32(ftoi(rgb.z))));
}

// Gradient position of the color map entry nearest to t (no rounding).
fn res_nearest_color_idx(t: vec3<f32>) -> f32 {
  var best = 0u;
  var best_d = 3.0e38;
  for (var i = 0u; i < 256u; i++) {
    let d = cmap[cmap_base + i].xyz - t;
    let dd = dot(d, d);
    if (dd < best_d) { best_d = dd; best = i; }
  }
  return f32(best) / 255.0;
}

// AbstractOBJMeshWFFunc.transform. p = index of scale_x; after the 14 shared parameters follow
// the colormap and displacement image blocks and the mesh header
// [vertices, faces, expanded face count, vertex start, face start];
// vertex = [x, y, z, u, v, has_uv], face = [v1, v2, v3, cumulative count].
fn res_mesh(base: u32, p: u32, v_in: Pt, amount: f32) -> Pt {
  var v = v_in;
  let o = base + p + 14u;
  let total = prm[o + 8u];
  if (total < 1.0) { return v; }
  let k = floor(rnd() * total);
  let fs = base + u32(prm[o + 10u]);
  let vs = base + u32(prm[o + 9u]);
  var lo = 0u;
  var hi = u32(prm[o + 7u]) - 1u;
  loop {
    if (lo >= hi) { break; }
    let mid = (lo + hi) / 2u;
    if (prm[fs + mid * 4u + 3u] > k) { hi = mid; } else { lo = mid + 1u; }
  }
  let f = fs + lo * 4u;
  let i1 = vs + u32(prm[f]) * 6u;
  let i2 = vs + u32(prm[f + 1u]) * 6u;
  let i3 = vs + u32(prm[f + 2u]) * 6u;
  let s = vec3<f32>(prm[base + p], prm[base + p + 1u], prm[base + p + 2u]);
  let off = vec3<f32>(prm[base + p + 3u], prm[base + p + 4u], prm[base + p + 5u]);
  let p1 = vec3<f32>(prm[i1], prm[i1 + 1u], prm[i1 + 2u]) * s + off;
  let p2 = vec3<f32>(prm[i2], prm[i2 + 1u], prm[i2 + 2u]) * s + off;
  let p3 = vec3<f32>(prm[i3], prm[i3 + 1u], prm[i3 + 2u]) * s + off;
  let sqrt_r1 = sqrt(rnd());
  let r2 = rnd();
  let a = 1.0 - sqrt_r1;
  let b = sqrt_r1 * (1.0 - r2);
  let c = r2 * sqrt_r1;
  let q = a * p1 + b * p2 + c * p3;
  v.x += amount * q.x;
  v.y += amount * q.y;
  v.z += amount * q.z;
  let color_on = prm[o] > 0.0;
  let displ_amount = prm[base + p + 11u];
  let displ_on = prm[o + 3u] > 0.0 && abs(displ_amount) > EPSILON;
  if ((color_on || displ_on) && prm[i1 + 5u] != 0.0) {
    let u = a * prm[i1 + 3u] + b * prm[i2 + 3u] + c * prm[i3 + 3u];
    let w = a * prm[i1 + 4u] + b * prm[i2 + 4u] + c * prm[i3 + 4u];
    if (color_on) {
      let iw = prm[o] - 1.0;
      let ih = prm[o + 1u] - 1.0;
      let rgb = res_image_color(base, p + 14u, prm[base + p + 10u] > 0.0, clamp(u * iw, 0.0, iw), clamp(ih - w * ih, 0.0, ih));
      v.rgbc = 1.0; v.r = rgb.x; v.g = rgb.y; v.b = rgb.z;
      v.color = res_uv_color_idx(rgb);
    }
    if (displ_on) {
      // As in JWildfire, the z components of the edge vectors repeat the y difference.
      let av = vec3<f32>(p2.x - p1.x, p2.y - p1.y, p2.y - p1.y);
      let bv = vec3<f32>(p3.x - p1.x, p3.y - p1.y, p3.y - p1.y);
      let cr = cross(av, bv);
      let n = cr / (length(cr) + 1.0e-16);
      let iw = prm[o + 3u] - 1.0;
      let ih = prm[o + 4u] - 1.0;
      let dd = res_image_displ(base, p + 17u, prm[base + p + 12u] > 0.0, clamp(u * iw, 0.0, iw), clamp(ih - w * ih, 0.0, ih)) * displ_amount;
      v.x += amount * n.x * dd;
      v.y += amount * n.y * dd;
      v.z += amount * n.z * dd;
    }
  }
  if (prm[base + p + 13u] == 1.0) { v.rso = 1.0; }
  return v;
}

// Image index tiling as in JWildfire (-n maps to n, outside the image).
fn res_wrap_tile(i: i32, n: i32) -> i32 {
  if (i < 0) { return i - (i / n - 1) * n; }
  if (i >= n) { return i - (i / n) * n; }
  return i;
}

// colormap_wf pixel: RGB + alpha (255 unless with_alpha, alpha plane at base + 18).
fn res_cm_rgba(base: u32, ix: i32, iy: i32, with_alpha: bool) -> vec4<f32> {
  var a = 255.0;
  if (with_alpha) { a = res_pixel(base, 18u, ix, iy).z; }
  return vec4<f32>(res_pixel(base, 15u, ix, iy), a);
}

// AbstractColorMapWFFunc.transform: params at base, image block at base + 15.
fn res_colormap(base: u32, inx: f32, iny: f32, v_in: Pt) -> Pt {
  var v = v_in;
  let w = i32(prm[base + 15u]);
  let h = i32(prm[base + 16u]);
  let x = (inx - (prm[base + 3u] + 0.5) + 1.0) / prm[base] * f32(w - 2);
  let y = (iny - (prm[base + 4u] + 0.5) + 1.0) / prm[base + 1u] * f32(h - 2);
  let blend = prm[base + 10u] > 0.0;
  var ix = ftoi(x);
  var iy = ftoi(y);
  if (blend) { ix = i32(trunc(x)); iy = i32(trunc(y)); }
  if (i32(prm[base + 6u]) == 1) { ix = res_wrap_tile(ix, w); }
  if (i32(prm[base + 7u]) == 1) { iy = res_wrap_tile(iy, h); }
  let with_alpha = prm[base + 11u] > 0.0;
  var c = vec4<f32>(0.0);
  if (ix >= 0 && ix < w && iy >= 0 && iy < h) {
    c = res_cm_rgba(base, ix, iy, with_alpha);
    if (blend) {
      var ru = vec4<f32>(0.0);
      var lb = vec4<f32>(0.0);
      var rb = vec4<f32>(0.0);
      if (ix < w - 1) { ru = res_cm_rgba(base, ix + 1, iy, with_alpha); }
      if (iy < h - 1) { lb = res_cm_rgba(base, ix, iy + 1, with_alpha); }
      if (ix < w - 1 && iy < h - 1) { rb = res_cm_rgba(base, ix + 1, iy + 1, with_alpha); }
      let fu = x - trunc(x);
      let fv = y - trunc(y);
      c = mix(mix(c, ru, fu), mix(lb, rb, fu), fv);
    }
  }
  let dc_color = i32(prm[base + 9u]);
  if (dc_color == 0) { v.rgbc = 1.0; v.r = c.x; v.g = c.y; v.b = c.z; }
  if (with_alpha && c.w <= 0.1) { v.hide = 1.0; } else { v.hide = 0.0; }
  var dz = prm[base + 5u];
  if (abs(prm[base + 2u]) > EPSILON) { dz += prm[base + 2u] * (0.299 * c.x + 0.588 * c.y + 0.113 * c.z) / 255.0; }
  if (i32(prm[base + 8u]) != 0) { v.z = dz; } else { v.z += dz; }
  if (dc_color > 0) { v.color = res_uv_color_idx(c.xyz); }
  return v;
}

// AbstractDisplacementMapWFFunc.transform: params at base, image block at base + 9.
fn res_displacemap(base: u32, inx: f32, iny: f32, amount: f32, v_in: Pt) -> Pt {
  var v = v_in;
  let w = i32(prm[base + 9u]);
  let h = i32(prm[base + 10u]);
  var ix = ftoi((inx - (prm[base + 5u] + 0.5) + 1.0) / prm[base + 3u] * f32(w - 1));
  var iy = ftoi((iny - (prm[base + 6u] + 0.5) + 1.0) / prm[base + 4u] * f32(h - 1));
  if (i32(prm[base + 7u]) == 1) { ix = res_wrap_tile(ix, w); }
  if (i32(prm[base + 8u]) == 1) { iy = res_wrap_tile(iy, h); }
  if (ix < 0 || ix >= w || iy < 0 || iy >= h) { return v; }
  let c = res_pixel(base, 9u, ix, iy) / 255.0;
  let intensity = 0.299 * c.x + 0.588 * c.y + 0.113 * c.z - prm[base + 2u];
  let mode = i32(prm[base]);
  if (mode == 1) {
    v.x += (c.x - 0.5) * amount;
    v.y += (c.y - 0.5) * amount;
  } else if (mode == 2) {
    if (intensity > 0.0) {
      let scl = 1.0 + (intensity - 0.5) * amount;
      v.x *= scl;
      v.y *= scl;
    }
  } else if (mode == 3) {
    let ax = (c.x - 0.5) * amount;
    let ay = (c.y - 0.5) * amount;
    let nx = v.x * ax * ay + v.y * ay;
    let ny = v.x * ay - v.y * ax * ay;
    v.x = nx;
    v.y = ny;
  } else if (intensity > 0.0) {
    let ang = intensity * TWO_PI * amount;
    let nx = v.x * cos(ang) - v.y * sin(ang);
    let ny = v.x * sin(ang) + v.y * cos(ang);
    v.x = nx;
    v.y = ny;
  }
  if (i32(prm[base + 1u]) == 1) {
    v.rgbc = 1.0; v.r = c.x * 255.0; v.g = c.y * 255.0; v.b = c.z * 255.0;
    v.color = res_nearest_color_idx(c);
  }
  return v;
}
