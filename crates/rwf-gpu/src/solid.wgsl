// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// Ported from JWildfire - an image and animation processor written in Java
// Copyright (C) 1995-2026 Andreas Maschke
//
// This program is free software: you can redistribute it and/or modify it under the terms of
// the GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: LGPL-3.0-or-later

// Solid rendering passes (port of rwf-core render/solid.rs): normals, ambient occlusion,
// shadow visibility, shading + gamma, post DOF.

struct SolidU {
    bg_ul: vec4<f32>, bg_ur: vec4<f32>, bg_ll: vec4<f32>, bg_lr: vec4<f32>, bg_cc: vec4<f32>, bg_single: vec4<f32>,
    bal: vec4<f32>,
    raster_w: u32, raster_h: u32, image_w: u32, image_h: u32,
    os: u32, border: u32, fsize: u32, fhalf: i32,
    light_count: u32, mat_count: u32, ao_enabled: u32, shadows: u32,
    ao_int: f32, ao_affect: f32, gamma: f32, alpha_scale: f32,
    mod_sat: f32, with_alpha: u32, bg_kind: u32, seed: u32,
    ao_rsamples: u32, ao_asamples: u32, sphere_radius: f32, radius_step: f32,
    azimuth_step: f32, r_jitter: f32, ao_falloff: f32, ao_blur: u32,
    ao_bsize: u32, ao_bcenter: i32, smooth_on: u32, smooth_r: i32,
    smooth_dl: i32, bias: f32, map_size: u32, dof_on: u32,
    dof_amount: f32, bokeh_int: f32, bokeh_bright: f32, bokeh_size: f32,
    bokeh_act: f32, bokeh_rmax: f32, img_diag: f32, bokeh_support: f32,
    light_off: u32, mat_off: u32, filt_off: u32, aob_off: u32,
    smooth_off: u32, bokeh_off: u32, bg_off: u32, _p2: u32,
    // Background region: offset x, y and size of the full image (quilt tiles).
    region: vec4<u32>,
}

const ZMIN: f32 = -3.4028235e38;
const EPS: f32 = 1.0e-8;
const LIGHT_STRIDE: u32 = 24u;
const MAT_STRIDE: u32 = 16u;
const BOKEH_TABLE: u32 = 2048u;
const DOF_SCALE: f32 = 256.0;
const PI: f32 = 3.14159265358979;

@group(0) @binding(0) var<uniform> S: SolidU;
@group(0) @binding(1) var<storage, read_write> zbuf: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> sattr: array<f32>;
@group(0) @binding(3) var<storage, read_write> smap: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read_write> normals: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> ao_raw: array<f32>;
@group(0) @binding(6) var<storage, read_write> ao: array<f32>;
@group(0) @binding(7) var<storage, read_write> accel: array<f32>;
@group(0) @binding(8) var<storage, read_write> vis: array<f32>;
@group(0) @binding(9) var<storage, read> sp: array<f32>;
@group(0) @binding(10) var<storage, read_write> shaded: array<u32>;
@group(0) @binding(11) var<storage, read_write> dofpix: array<f32>;
@group(0) @binding(12) var<storage, read_write> dofacc: array<atomic<i32>>;
@group(0) @binding(13) var out_tex: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(14) var<storage, read> refl: array<u32>;

var<private> rng: u32;

fn hash(v: u32) -> u32 {
    var x = v * 747796405u + 2891336453u;
    x = ((x >> ((x >> 28u) + 4u)) ^ x) * 277803737u;
    return (x >> 22u) ^ x;
}

fn rnd() -> f32 {
    rng = hash(rng);
    return f32(rng >> 8u) / 16777216.0;
}

fn cells() -> u32 { return S.raster_w * S.raster_h; }

fn zdecode(k: u32) -> f32 {
    if (k == 0u) { return ZMIN; }
    if ((k & 0x80000000u) != 0u) { return bitcast<f32>(k & 0x7fffffffu); }
    return bitcast<f32>(~k);
}

fn has_z(i: u32) -> bool { return atomicLoad(&zbuf[i]) != 0u; }

fn origin(i: u32) -> vec3<f32> { return vec3<f32>(sattr[i * 8u], sattr[i * 8u + 1u], sattr[i * 8u + 2u]); }

// ---- normals (NormalsCalculator, 8 samples) ----

fn nb(k: u32) -> vec4<i32> {
    switch k {
        case 0u: { return vec4<i32>(0, 1, 1, 0); }
        case 1u: { return vec4<i32>(1, 0, 0, -1); }
        case 2u: { return vec4<i32>(0, -1, -1, 0); }
        case 3u: { return vec4<i32>(-1, 0, 0, 1); }
        case 4u: { return vec4<i32>(-1, 1, 1, 1); }
        case 5u: { return vec4<i32>(1, 1, 1, -1); }
        case 6u: { return vec4<i32>(1, -1, -1, -1); }
        case 7u: { return vec4<i32>(-1, -1, -1, 1); }
        case 8u: { return vec4<i32>(0, 1, 1, 1); }
        case 9u: { return vec4<i32>(1, 1, 1, 0); }
        case 10u: { return vec4<i32>(1, 0, 1, -1); }
        case 11u: { return vec4<i32>(1, -1, 0, -1); }
        case 12u: { return vec4<i32>(0, -1, -1, -1); }
        case 13u: { return vec4<i32>(-1, -1, -1, 0); }
        case 14u: { return vec4<i32>(-1, 0, -1, 1); }
        default: { return vec4<i32>(-1, 1, 0, 1); }
    }
}

fn inside(x: i32, y: i32) -> bool { return x >= 0 && y >= 0 && x < i32(S.raster_w) && y < i32(S.raster_h); }

fn cidx(x: i32, y: i32) -> u32 { return u32(y) * S.raster_w + u32(x); }

@compute @workgroup_size(16, 16)
fn sld_normals(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.raster_w || gid.y >= S.raster_h) { return; }
    let x = i32(gid.x);
    let y = i32(gid.y);
    let i = cidx(x, y);
    normals[i] = vec4<f32>(0.0);
    if (!has_z(i)) { return; }
    let c = origin(i);
    var n = vec3<f32>(0.0);
    var samples = 0u;
    for (var k = 0u; k < 16u; k++) {
        let o = nb(k);
        if (inside(x + o.x, y + o.y) && inside(x + o.z, y + o.w)) {
            let ia = cidx(x + o.x, y + o.y);
            let ib = cidx(x + o.z, y + o.w);
            if (has_z(ia) && has_z(ib)) {
                let a = c - origin(ia);
                let b = c - origin(ib);
                samples++;
                n += cross(a, b);
            }
        }
        if (samples >= 8u) { break; }
    }
    let r = length(n);
    if (samples > 0u && r > EPS) {
        normals[i] = vec4<f32>(n / r, 1.0);
    }
}

// ---- ambient occlusion (AOCalculator) ----

fn ao_z(x: f32, y: f32) -> f32 {
    let xi = clamp(i32(round_half_away(x)), 0, i32(S.raster_w) - 1);
    let yi = clamp(i32(round_half_away(y)), 0, i32(S.raster_h) - 1);
    return zdecode(atomicLoad(&zbuf[cidx(xi, yi)]));
}

fn round_half_away(v: f32) -> f32 {
    if (v > 0.0) { return trunc(v + 0.5); }
    if (v < 0.0) { return -trunc(-v + 0.5); }
    return 0.0;
}

@compute @workgroup_size(16, 16)
fn sld_ao(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.raster_w || gid.y >= S.raster_h) { return; }
    let i = gid.y * S.raster_w + gid.x;
    ao_raw[i] = 0.0;
    let nn = normals[i];
    if (!has_z(i) || nn.w < 0.5) { return; }
    rng = hash(i ^ (S.seed * 0x9E3779B9u));
    let z0 = zdecode(atomicLoad(&zbuf[i]));
    let x0 = f32(gid.x);
    let y0 = f32(gid.y);
    let nx = nn.x; let ny = nn.y; let nz = nn.z;
    var sum = 0.0;
    var angle = (0.5 - rnd()) * S.azimuth_step / 4.0;
    for (var k = 0u; k < S.ao_asamples; k++) {
        let dx = cos(angle);
        let dy = sin(angle);
        var r0 = S.radius_step;
        var prev_h = 0.0;
        for (var l = 0u; l < S.ao_rsamples; l++) {
            let r = r0 + (0.5 - rnd()) * S.r_jitter;
            let x = x0 + r * dx + (0.5 - rnd()) * S.r_jitter;
            let y = y0 + r * dy + (0.5 - rnd()) * S.r_jitter;
            let h = atan2(ao_z(x, y) - z0, r) + 0.001;
            let rr = sqrt(x * x + y * y);
            let rx = x / rr; let ry = y / rr;
            let tt = nx * rx + ry * ny;
            let tx = -(ry * nx * ny - rx * ny * ny - rx * nz * nz) / tt;
            let ty = (ry * nx * nx + ry * nz * nz - nx * rx * ny) / tt;
            let tz = (-ry * ny * nz - nx * rx * nz) / tt;
            let ta = atan2(tz, sqrt(tx * tx + ty * ty));
            let a = sin(h) - sin(ta);
            if (a > prev_h) {
                prev_h = a;
                let dist = r / S.sphere_radius;
                sum += a * exp(-dist * dist * S.ao_falloff) * 0.5;
            }
            r0 += S.radius_step;
        }
        angle += S.azimuth_step;
    }
    ao_raw[i] = sum;
}

@compute @workgroup_size(16, 16)
fn sld_ao_blur(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.raster_w || gid.y >= S.raster_h) { return; }
    let i = gid.y * S.raster_w + gid.x;
    if (S.ao_blur == 0u) { ao[i] = ao_raw[i]; return; }
    var v = 0.0;
    let size = i32(S.ao_bsize);
    for (var k = 0; k < size; k++) {
        let px = i32(gid.x) + k - S.ao_bcenter;
        if (px < 0 || px >= i32(S.raster_w)) { continue; }
        for (var l = 0; l < size; l++) {
            let py = i32(gid.y) + l - S.ao_bcenter;
            if (py >= 0 && py < i32(S.raster_h)) {
                v += ao_raw[cidx(px, py)] * sp[S.aob_off + u32(k * size + l)];
            }
        }
    }
    ao[i] = v * 0.1;
}

// ---- shadows (ShadowCalculator) ----

fn lp(l: u32, k: u32) -> f32 { return sp[S.light_off + l * LIGHT_STRIDE + k]; }

@compute @workgroup_size(16, 16)
fn sld_shadow_accel(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.raster_w || gid.y >= S.raster_h || gid.z >= S.light_count) { return; }
    let l = gid.z;
    let i = gid.y * S.raster_w + gid.x;
    let o = l * cells() + i;
    accel[o] = -1.0;
    if (lp(l, 8u) < 0.5 || !has_z(i)) { return; }
    let p = origin(i);
    let lx = lp(l, 10u) * p.x + lp(l, 11u) * p.y + lp(l, 12u) * p.z;
    let ly = lp(l, 13u) * p.x + lp(l, 14u) * p.y + lp(l, 15u) * p.z;
    let lz = lp(l, 16u) * p.x + lp(l, 17u) * p.y + lp(l, 18u) * p.z;
    let mx = i32(lp(l, 19u) * lx + lp(l, 21u) + 0.5);
    let my = i32(lp(l, 20u) * ly + lp(l, 22u) + 0.5);
    let ms = i32(S.map_size);
    if (mx < 0 || my < 0 || mx >= ms || my >= ms) { return; }
    let m = u32(lp(l, 9u));
    let sz = zdecode(atomicLoad(&smap[m * S.map_size * S.map_size + u32(mx) * S.map_size + u32(my)]));
    accel[o] = select(1.0, 0.0, lz < sz - S.bias);
}

@compute @workgroup_size(16, 16)
fn sld_shadow_vis(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.raster_w || gid.y >= S.raster_h || gid.z >= S.light_count) { return; }
    let l = gid.z;
    let i = gid.y * S.raster_w + gid.x;
    let base = l * cells();
    var v = 1.0;
    if (lp(l, 8u) > 0.5) {
        if (S.smooth_on != 0u) {
            var total = 0.0;
            let r = S.smooth_r;
            let n = 2 * r + 1;
            for (var k = -r; k <= r; k += S.smooth_dl) {
                let x = i32(gid.x) + k;
                if (x < 0 || x >= i32(S.raster_w)) { continue; }
                for (var m = -r; m <= r; m += S.smooth_dl) {
                    let y = i32(gid.y) + m;
                    if (y >= 0 && y < i32(S.raster_h)) {
                        let a = accel[base + cidx(x, y)];
                        if (a >= 0.0) {
                            let f = sp[S.smooth_off + u32((k + r) * n + m + r)];
                            total += f;
                            v += a * f;
                        }
                    }
                }
            }
            if (total > EPS) { v /= total; }
        } else {
            let a = accel[base + i];
            if (a >= 0.0) { v = a; }
        }
        v = clamp(v + lp(l, 23u), 0.0, 1.0);
    }
    vis[base + i] = v;
}

// ---- shading (LogDensityFilter.addSolidColors + GammaCorrectionFilter) ----

struct MatSel { valid: bool, a: u32, b: u32, t: f32, exact: bool }

fn mat_sel(idx: f32) -> MatSel {
    let n = S.mat_count;
    if (idx < 0.0 || n == 0u) { return MatSel(false, 0u, 0u, 0.0, false); }
    let m0 = u32(idx);
    if (m0 >= n) { return MatSel(true, n - 1u, n - 1u, 0.0, true); }
    var m1 = m0 + 1u;
    if (m1 >= n) { m1 = 0u; }
    let t = idx - floor(idx);
    if (t < 0.01) { return MatSel(true, m0, m0, 0.0, true); }
    if (t > 0.99) { return MatSel(true, m1, m1, 0.0, true); }
    return MatSel(true, m0, m1, t, false);
}

fn mprop(m: MatSel, k: u32) -> f32 {
    let a = sp[S.mat_off + m.a * MAT_STRIDE + k];
    if (m.t == 0.0) { return a; }
    return a + (sp[S.mat_off + m.b * MAT_STRIDE + k] - a) * m.t;
}

fn ldiff_f(f: u32, c: f32) -> f32 {
    switch f {
        case 1u: { return c * c; }
        case 2u: { return c * 0.5 + 0.5; }
        case 3u: { let v = c * 0.5 + 0.5; return v * v; }
        default: { return c; }
    }
}

fn ldiff(m: MatSel, c: f32) -> f32 {
    let a = ldiff_f(u32(sp[S.mat_off + m.a * MAT_STRIDE + 7u]), c);
    if (m.t == 0.0) { return a; }
    return a + (ldiff_f(u32(sp[S.mat_off + m.b * MAT_STRIDE + 7u]), c) - a) * m.t;
}

fn refl_color(m: u32, u: f32, v: f32) -> vec3<f32> {
    let b = S.mat_off + m * MAT_STRIDE;
    let w = u32(sp[b + 10u]);
    let h = u32(sp[b + 11u]);
    let x = clamp(u32(f32(w) * u), 0u, w - 1u);
    let y = clamp(u32(f32(h) * v), 0u, h - 1u);
    let c = refl[u32(sp[b + 9u]) + y * w + x];
    return vec3<f32>(f32(c & 255u), f32((c >> 8u) & 255u), f32((c >> 16u) & 255u)) / 255.0;
}

// Adds the lit color of raster cell `i`; false if it has no surface.
fn add_cell(i: u32, scale: f32, bg: vec3<f32>, acc: ptr<function, vec3<f32>>) -> bool {
    let nn = normals[i];
    if (nn.w < 0.5) { return false; }
    let n = nn.xyz;
    let nl = S.light_count;
    let cs = cells();
    var avg_vis = 1.0;
    if (S.shadows != 0u && nl > 0u) {
        var s = 0.0;
        for (var l = 0u; l < nl; l++) { s += vis[l * cs + i]; }
        avg_vis = s / f32(nl);
    }
    var raw = vec3<f32>(0.0);
    let m = mat_sel(sattr[i * 8u + 6u]);
    if (!m.valid) {
        var v = 0.0;
        for (var l = 0u; l < nl; l++) {
            v += select(avg_vis, vis[l * cs + i], lp(l, 7u) > 0.5 && S.shadows != 0u);
        }
        raw = bg / 255.0 * clamp(v, 0.0, 1.0);
    } else {
        let aoi = clamp(S.ao_int, 0.0, 4.0);
        let a = ao[i];
        var ambient = mprop(m, 0u);
        var diffuse = select(mprop(m, 8u), mprop(m, 1u), m.t == 0.0);
        var refl_int = mprop(m, 8u);
        if (S.ao_enabled != 0u) {
            ambient = ambient - a * aoi;
            diffuse = diffuse - a * aoi * S.ao_affect;
            refl_int = refl_int - a * aoi * S.ao_affect;
        }
        ambient = max(ambient, 0.0);
        diffuse = max(diffuse, 0.0);
        refl_int = max(refl_int, 0.0);
        let specular = mprop(m, 2u);
        let phong_size = mprop(m, 3u);
        let phong = vec3<f32>(mprop(m, 4u), mprop(m, 5u), mprop(m, 6u));
        let obj = vec3<f32>(sattr[i * 8u + 3u], sattr[i * 8u + 4u], sattr[i * 8u + 5u]) * S.bal.xyz / 255.0;
        raw = obj * ambient * avg_vis;
        let has_refl = m.exact && sp[S.mat_off + m.a * MAT_STRIDE + 9u] >= 0.0;
        for (var l = 0u; l < nl; l++) {
            let dir = vec3<f32>(lp(l, 0u), lp(l, 1u), lp(l, 2u));
            let v = select(avg_vis, vis[l * cs + i], lp(l, 7u) > 0.5 && S.shadows != 0u);
            let li = lp(l, 6u);
            let cosa = dot(dir, n);
            if (cosa > EPS) {
                raw += (vec3<f32>(lp(l, 3u), lp(l, 4u), lp(l, 5u)) + obj * ambient / 3.0) * (ldiff(m, cosa) * v * diffuse * li);
            }
            if (specular > EPS) {
                let rv = reflect(dir, n);
                let vr = rv.z;
                if (vr < EPS) {
                    raw += phong * (pow(ldiff(m, -vr), phong_size) * v * specular * li);
                }
            }
            if (has_refl) {
                let rv = reflect(vec3<f32>(0.0, 0.0, 1.0), n);
                var uv: vec2<f32>;
                if (sp[S.mat_off + m.a * MAT_STRIDE + 12u] < 0.5) {
                    let mm = 2.0 * sqrt(rv.x * rv.x + rv.y * rv.y + (rv.z + 1.0) * (rv.z + 1.0));
                    uv = clamp(vec2<f32>(rv.x / mm + 0.5, rv.y / mm + 0.5), vec2<f32>(0.0), vec2<f32>(1.0));
                } else {
                    uv = clamp(vec2<f32>((atan(rv.x / rv.z) + PI) / (2.0 * PI), (asin(clamp(rv.y, -1.0, 1.0)) + PI * 0.5) / PI), vec2<f32>(0.0), vec2<f32>(1.0));
                }
                raw += refl_color(m.a, uv.x, uv.y) * S.bal.xyz * (v * refl_int);
            }
        }
    }
    *acc += raw * scale * 255.0;
    return true;
}

fn background(px: u32, py: u32) -> vec3<f32> {
    if (S.bg_kind == 0u) { return S.bg_single.xyz; }
    if (S.bg_kind == 3u) {
        let i = S.bg_off + (py * S.image_w + px) * 3u;
        return vec3<f32>(f32(bitcast<i32>(refl[i])), f32(bitcast<i32>(refl[i + 1u])), f32(bitcast<i32>(refl[i + 2u])));
    }
    // Gradients span the full image.
    let w = S.region.z;
    let h = S.region.w;
    let gx = px + S.region.x;
    let gy = py + S.region.y;
    if (S.bg_kind == 1u) {
        let fx = f32(gx) / f32(max(w, 2u) - 1u);
        let fy = f32(gy) / f32(max(h, 2u) - 1u);
        let c = mix(mix(S.bg_ul.xyz, S.bg_ur.xyz, fx), mix(S.bg_ll.xyz, S.bg_lr.xyz, fx), fy);
        return floor(c + 0.5);
    }
    let w2 = f32(max(i32(w) / 2 - 1, 1));
    let h2 = f32(max(i32(h) / 2 - 1, 1));
    let x = f32(gx);
    let y = f32(gy);
    var fx: f32; var fy: f32;
    var qul: vec3<f32>; var qur: vec3<f32>; var qll: vec3<f32>; var qlr: vec3<f32>;
    let ul = S.bg_ul.xyz; let ur = S.bg_ur.xyz; let ll = S.bg_ll.xyz; let lr = S.bg_lr.xyz; let cc = S.bg_cc.xyz;
    if (x <= w2) {
        fx = x / w2;
        if (y <= h2) { fy = y / h2; qul = ul; qur = mix(ul, ur, 0.5); qll = mix(ll, ul, 0.5); qlr = cc; }
        else { fy = (y - h2) / h2; qul = mix(ul, ll, 0.5); qur = cc; qll = ll; qlr = mix(ll, lr, 0.5); }
    } else {
        fx = (x - w2) / w2;
        if (y <= h2) { fy = y / h2; qul = mix(ul, ur, 0.5); qur = ur; qll = cc; qlr = mix(ur, lr, 0.5); }
        else { fy = (y - h2) / h2; qul = cc; qur = mix(ur, lr, 0.5); qll = mix(ll, lr, 0.5); qlr = lr; }
    }
    fx = min(fx, 1.0);
    fy = min(fy, 1.0);
    return floor(mix(mix(qul, qur, fx), mix(qll, qlr, fx), fy) + 0.5);
}

fn rgb_to_hsl(c: vec3<f32>) -> vec3<f32> {
    let mx = max(c.x, max(c.y, c.z));
    let mn = min(c.x, min(c.y, c.z));
    let l = (mx + mn) / 2.0;
    if (abs(mx - mn) < 1.0e-8) { return vec3<f32>(0.0, 0.0, l); }
    let d = mx - mn;
    let s = select(d / (mx + mn), d / (2.0 - mx - mn), l > 0.5);
    var h: f32;
    if (mx == c.x) { h = (c.y - c.z) / d + select(0.0, 6.0, c.y < c.z); }
    else if (mx == c.y) { h = (c.z - c.x) / d + 2.0; }
    else { h = (c.x - c.y) / d + 4.0; }
    return vec3<f32>(h / 6.0, s, l);
}

fn hsl_to_rgb(hsl: vec3<f32>) -> vec3<f32> {
    let h = clamp(hsl.x, 0.0, 1.0);
    let s = clamp(hsl.y, 0.0, 1.0);
    let l = clamp(hsl.z, 0.0, 1.0);
    let v = select(l + s - l * s, l * (1.0 + s), l <= 0.5);
    if (v <= 0.0) { return vec3<f32>(0.0); }
    let h6 = clamp(h * 6.0, 0.0, 6.0);
    let y = l + l - v;
    let f = h6 - trunc(h6);
    let x = y + (v - y) * f;
    let z = v - (v - y) * f;
    switch i32(h6) {
        case 0: { return vec3<f32>(v, x, y); }
        case 1: { return vec3<f32>(z, v, y); }
        case 2: { return vec3<f32>(y, v, x); }
        case 3: { return vec3<f32>(y, z, v); }
        case 4: { return vec3<f32>(x, y, v); }
        case 5: { return vec3<f32>(v, y, z); }
        default: { return vec3<f32>(v, x, y); }
    }
}

fn pack(c: vec4<f32>) -> u32 {
    let q = vec4<u32>(clamp(c, vec4<f32>(0.0), vec4<f32>(255.0)));
    return q.x | (q.y << 8u) | (q.z << 16u) | (q.w << 24u);
}

fn unpack(v: u32) -> vec4<f32> {
    return vec4<f32>(f32(v & 255u), f32((v >> 8u) & 255u), f32((v >> 16u) & 255u), f32(v >> 24u));
}

// Filtered shading of one output pixel: color, coverage intensity and DOF distance.
struct Shade { col: vec3<f32>, intensity: f32, dof: f32 }

fn shade_pixel(gid: vec3<u32>, bg: vec3<f32>) -> Shade {
    let os = i32(S.os);
    let b = i32(S.border);
    let px = i32(gid.x);
    let py = i32(gid.y);
    var col = vec3<f32>(0.0);
    var intensity = 0.0;
    var dof = 0.0;
    if (S.fsize > 1u) {
        let fs = i32(S.fsize);
        let w0 = 1.0 / f32(os * os);
        for (var i = 0; i < fs; i++) {
            for (var j = 0; j < fs; j++) {
                let rx = px * os + j - S.fhalf + b;
                let ry = py * os + i - S.fhalf + b;
                if (inside(rx, ry)) {
                    let c = cidx(rx, ry);
                    let w = sp[S.filt_off + u32(i * fs + j)] * w0;
                    if (add_cell(c, w, bg, &col)) {
                        dof += w * abs(sattr[c * 8u + 7u] * S.dof_amount);
                        intensity += w;
                    }
                }
            }
        }
    } else {
        var count = 0;
        for (var sx = 0; sx < os; sx++) {
            for (var sy = 0; sy < os; sy++) {
                let rx = px * os + sx + b;
                let ry = py * os + sy + b;
                if (inside(rx, ry)) {
                    let c = cidx(rx, ry);
                    if (add_cell(c, 1.0, bg, &col)) {
                        dof += abs(sattr[c * 8u + 7u] * S.dof_amount);
                        count++;
                    }
                }
            }
        }
        if (count > 0) {
            let d = 1.0 / f32(count);
            dof *= d;
            col *= d;
            intensity = f32(count) / f32(os * os);
        }
    }
    return Shade(col, intensity, dof);
}

@compute @workgroup_size(16, 16)
fn sld_shade(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.image_w || gid.y >= S.image_h) { return; }
    let bg = background(gid.x, gid.y);
    let sh = shade_pixel(gid, bg);
    let col = sh.col;
    let intensity = sh.intensity;
    let dof = sh.dof;
    var rgb: vec3<f32>;
    var alpha: f32;
    if (intensity > 0.0) {
        let a = pow(intensity, 1.0 + S.gamma);
        let alpha_int = clamp(floor(a * 255.0 * S.alpha_scale + 0.5), 0.0, 255.0);
        let inv = 255.0 - alpha_int;
        alpha = select(255.0, alpha_int, S.with_alpha != 0u);
        rgb = clamp(floor(col + 0.5) + floor(inv * bg / 256.0), vec3<f32>(0.0), vec3<f32>(255.0));
    } else {
        rgb = clamp(bg, vec3<f32>(0.0), vec3<f32>(255.0));
        alpha = select(255.0, 0.0, S.with_alpha != 0u);
    }
    if (S.mod_sat != 0.0) {
        let hsl = rgb_to_hsl(rgb / 255.0);
        rgb = clamp(floor(hsl_to_rgb(vec3<f32>(hsl.x, hsl.y + S.mod_sat, hsl.z)) * 255.0 + 0.5), vec3<f32>(0.0), vec3<f32>(255.0));
    }
    let pi = gid.y * S.image_w + gid.x;
    shaded[pi] = pack(vec4<f32>(rgb, alpha));
    dofpix[pi] = dof;
}

// Linear float RGB without quantization (`transformPointHDR`); no post DOF.
@group(0) @binding(15) var<storage, read_write> hdr_out: array<f32>;

@compute @workgroup_size(16, 16)
fn sld_shade_hdr(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.image_w || gid.y >= S.image_h) { return; }
    let bg = background(gid.x, gid.y);
    let sh = shade_pixel(gid, bg);
    var rgb = bg / 255.0;
    if (sh.intensity > 0.0) {
        let inv = 255.0 - clamp(pow(sh.intensity, 1.0 + S.gamma) * 255.0 * S.alpha_scale, 0.0, 255.0);
        rgb = max(sh.col + inv * bg / 255.0, vec3<f32>(0.0)) / 255.0;
    }
    let i = (gid.y * S.image_w + gid.x) * 3u;
    hdr_out[i] = rgb.x;
    hdr_out[i + 1u] = rgb.y;
    hdr_out[i + 2u] = rgb.z;
}

// Denoiser helper images (6 floats per pixel): albedo (object color without lighting,
// background where there is no surface) and the view space normal, averaged over the
// pixel's raster cells.
@compute @workgroup_size(16, 16)
fn sld_aux(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.image_w || gid.y >= S.image_h) { return; }
    let bg = background(gid.x, gid.y) / 255.0;
    let os = i32(S.os);
    let b = i32(S.border);
    var albedo = vec3<f32>(0.0);
    var normal = vec3<f32>(0.0);
    for (var sx = 0; sx < os; sx++) {
        for (var sy = 0; sy < os; sy++) {
            let rx = i32(gid.x) * os + sx + b;
            let ry = i32(gid.y) * os + sy + b;
            var a = bg;
            if (inside(rx, ry)) {
                let c = cidx(rx, ry);
                let nn = normals[c];
                if (nn.w >= 0.5) {
                    a = vec3<f32>(sattr[c * 8u + 3u], sattr[c * 8u + 4u], sattr[c * 8u + 5u]) * S.bal.xyz / 255.0;
                    normal += nn.xyz;
                }
            }
            albedo += a;
        }
    }
    albedo = clamp(albedo / f32(os * os), vec3<f32>(0.0), vec3<f32>(1.0));
    let len = length(normal);
    if (len > EPS) { normal /= len; }
    let i = (gid.y * S.image_w + gid.x) * 6u;
    hdr_out[i] = albedo.x;
    hdr_out[i + 1u] = albedo.y;
    hdr_out[i + 2u] = albedo.z;
    hdr_out[i + 3u] = normal.x;
    hdr_out[i + 4u] = normal.y;
    hdr_out[i + 5u] = normal.z;
}

@compute @workgroup_size(16, 16)
fn sld_output(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.image_w || gid.y >= S.image_h) { return; }
    textureStore(out_tex, vec2<i32>(gid.xy), unpack(shaded[gid.y * S.image_w + gid.x]) / 255.0);
}

// ---- post DOF (PostDOFCalculator) ----

fn bokeh(r: f32) -> f32 {
    if (r >= S.bokeh_rmax) { return 0.0; }
    let f = r / S.bokeh_rmax * f32(BOKEH_TABLE - 1u);
    let k = u32(f);
    let a = sp[S.bokeh_off + k];
    let c = sp[S.bokeh_off + min(k + 1u, BOKEH_TABLE - 1u)];
    return mix(a, c, f - f32(k));
}

fn dof_step(radius: f32) -> f32 {
    var s = 1.0;
    if (radius < 2.0) { s = 0.0625; }
    else if (radius < 3.0) { s = 0.125; }
    else if (radius < 4.0) { s = 0.25; }
    else if (radius < 5.0) { s = 0.375; }
    else if (radius < 6.0) { s = 0.5; }
    else if (radius < 7.0) { s = 0.625; }
    else if (radius < 8.0) { s = 0.75; }
    else if (radius < 9.0) { s = 0.875; }
    return s * 0.5;
}

@compute @workgroup_size(16, 16)
fn sld_dof_splat(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.image_w || gid.y >= S.image_h) { return; }
    let pi = gid.y * S.image_w + gid.x;
    var c = unpack(shaded[pi]).xyz;
    if (c.x <= 0.0 && c.y <= 0.0 && c.z <= 0.0) { return; }
    rng = hash(pi ^ (S.seed * 0x85EBCA6Bu));
    let d = abs(dofpix[pi]);
    let plain = d * 10.0;
    var radius = max(round_half_away(d * 10.0), 2.0);
    if (rnd() > 1.0 - S.bokeh_int && dot(c, vec3<f32>(0.299, 0.588, 0.113)) / 255.0 >= S.bokeh_act) {
        let it = (rnd() + 1.0) / 5.0;
        let f = it * radius * radius * S.bokeh_bright;
        c = vec3<f32>(round_half_away(c.x * f), round_half_away(c.y * f), round_half_away(c.z * f));
        radius *= S.bokeh_size * (1.0 + rnd() * S.bokeh_size);
    } else {
        radius *= 1.0 + (0.5 - rnd()) * 0.1;
    }
    let inv = 1.0 / plain * S.bokeh_support;
    var step = dof_step(radius);
    let max_steps = floor(S.img_diag);
    if (radius / step > max_steps) { step = radius / max_steps; }
    var sum = 0.0;
    var di = -radius;
    loop {
        if (di >= radius + EPS) { break; }
        let i2 = (di * inv) * (di * inv);
        var dj = -radius;
        loop {
            if (dj >= radius + EPS) { break; }
            let k = bokeh(sqrt(i2 + (dj * inv) * (dj * inv)));
            if (k > EPS) { sum += k; }
            dj += step;
        }
        di += step;
    }
    let w = i32(S.image_w);
    let h = i32(S.image_h);
    if (sum <= 0.0) {
        let o = pi * 3u;
        atomicAdd(&dofacc[o], i32(c.x * DOF_SCALE));
        atomicAdd(&dofacc[o + 1u], i32(c.y * DOF_SCALE));
        atomicAdd(&dofacc[o + 2u], i32(c.z * DOF_SCALE));
        return;
    }
    di = -radius;
    loop {
        if (di >= radius + EPS) { break; }
        let i2 = (di * inv) * (di * inv);
        let tx = i32(round_half_away(f32(gid.x) + di));
        var dj = -radius;
        loop {
            if (dj >= radius + EPS) { break; }
            let k = bokeh(sqrt(i2 + (dj * inv) * (dj * inv)));
            let ty = i32(round_half_away(f32(gid.y) + dj));
            if (k > EPS && tx >= 0 && ty >= 0 && tx < w && ty < h) {
                let o = (u32(ty) * S.image_w + u32(tx)) * 3u;
                let f = k / sum * DOF_SCALE;
                atomicAdd(&dofacc[o], i32(c.x * f));
                atomicAdd(&dofacc[o + 1u], i32(c.y * f));
                atomicAdd(&dofacc[o + 2u], i32(c.z * f));
            }
            dj += step;
        }
        di += step;
    }
}

@compute @workgroup_size(16, 16)
fn sld_dof_resolve(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= S.image_w || gid.y >= S.image_h) { return; }
    let pi = gid.y * S.image_w + gid.x;
    let o = pi * 3u;
    let rgb = clamp(floor(vec3<f32>(f32(atomicLoad(&dofacc[o])), f32(atomicLoad(&dofacc[o + 1u])), f32(atomicLoad(&dofacc[o + 2u]))) / DOF_SCALE + 0.5), vec3<f32>(0.0), vec3<f32>(255.0));
    let a = unpack(shaded[pi]).w;
    textureStore(out_tex, vec2<i32>(gid.xy), vec4<f32>(rgb, a) / 255.0);
}
