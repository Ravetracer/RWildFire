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
  let t = vec3<f32>(f32(ftoi(rgb.x)), f32(ftoi(rgb.y)), f32(ftoi(rgb.z)));
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
