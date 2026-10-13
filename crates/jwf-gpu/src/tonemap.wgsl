// Histogram resolve + log-density filter + gamma correction.
// Port of LogDensityFilter / GammaCorrectionFilter, see jwf-core render/tonemap.rs.

struct ResolveU { cells: u32, color_scale: f32, row: u32, _p: u32 }

@group(0) @binding(0) var<uniform> R: ResolveU;
@group(0) @binding(1) var<storage, read_write> hist: array<atomic<i32>>;
@group(0) @binding(2) var<storage, read_write> accum: array<f32>;

@compute @workgroup_size(256)
fn resolve(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x + gid.y * R.row;
    if (i >= R.cells) { return; }
    let b = i * 4u;
    let c = atomicExchange(&hist[b], 0);
    if (c == 0) { return; }
    accum[b] += f32(c);
    accum[b + 1u] += f32(atomicExchange(&hist[b + 1u], 0)) / R.color_scale;
    accum[b + 2u] += f32(atomicExchange(&hist[b + 2u], 0)) / R.color_scale;
    accum[b + 3u] += f32(atomicExchange(&hist[b + 3u], 0)) / R.color_scale;
}

struct ToneU {
    image_w: u32, image_h: u32, raster_w: u32, raster_h: u32,
    os: u32, border: u32, fsize: u32, ssize: u32,
    fhalf: i32, shalf: i32, sharpen: u32, with_alpha: u32,
    k1: f32, k2: f32, bg_glow: f32, white_level: f32,
    bal: vec4<f32>,
    gamma: f32, gamma_threshold: f32, scl_gamma: f32, vib_int: f32,
    inv_vib_int: f32, alpha_scale: f32, mod_sat: f32, bg_kind: u32,
    bg_ul: vec4<f32>, bg_ur: vec4<f32>, bg_ll: vec4<f32>, bg_lr: vec4<f32>, bg_cc: vec4<f32>, bg_single: vec4<f32>,
    smooth_offset: u32, mb_scl: f32, adaptive: u32, ad_sharpness: f32,
    // Adaptive filter matrices (low density, soften, detail), all stored in `filt`.
    ad_low_density: f32, low_off: u32, soft_off: u32, detail_off: u32,
    low_size: u32, soft_size: u32, detail_size: u32, low_half: i32,
    soft_half: i32, detail_half: i32, low_radius: f32, soft_radius: f32,
    detail_radius: f32, prim_radius: f32, _q1: u32, _q2: u32,
}

// A filter matrix in `filt` plus its adaptive indicator color.
struct Filt { off: u32, size: u32, half: i32, sharpen: u32, radius: f32, ind: vec3<f32> }

@group(0) @binding(0) var<uniform> T: ToneU;
@group(0) @binding(1) var<storage, read> acc: array<f32>;
@group(0) @binding(2) var<storage, read> filt: array<f32>;
@group(0) @binding(3) var out_tex: texture_storage_2d<rgba8unorm, write>;

fn log_scale(count_raw: f32) -> f32 {
    let count = count_raw * T.mb_scl;
    return (T.k1 * log(1.0 + count * T.k2) * 0.434294481903252 + T.bg_glow / (count + 1.0)) / (T.white_level * count);
}

fn cell(rx: i32, ry: i32) -> vec4<f32> {
    let x = rx + i32(T.border);
    let y = ry + i32(T.border);
    if (x < 0 || y < 0 || x >= i32(T.raster_w) || y >= i32(T.raster_h)) { return vec4<f32>(0.0); }
    let i = (u32(y) * T.raster_w + u32(x)) * 4u;
    return vec4<f32>(acc[i], acc[i + 1u], acc[i + 2u], acc[i + 3u]);
}

// Raster cell without the border offset (the adaptive filter samples it this way in JWildfire).
fn raw_cell(x: i32, y: i32) -> vec4<f32> {
    if (x < 0 || y < 0 || x >= i32(T.raster_w) || y >= i32(T.raster_h)) { return vec4<f32>(0.0); }
    let i = (u32(y) * T.raster_w + u32(x)) * 4u;
    return vec4<f32>(acc[i], acc[i + 1u], acc[i + 2u], acc[i + 3u]);
}

fn lum3(c: vec3<f32>) -> f32 { return 0.299 * c.x + 0.588 * c.y + 0.113 * c.z; }

fn raw_lum(x: i32, y: i32) -> f32 { return lum3(raw_cell(x, y).yzw); }

// LogDensityFilter.getFilter
fn choose_filter(px: i32, py: i32) -> Filt {
    let primary = Filt(0u, T.fsize, T.fhalf, T.sharpen, T.prim_radius, vec3<f32>(1.0));
    if (T.adaptive == 0u) { return primary; }
    let x = px * i32(T.os);
    let y = py * i32(T.os);
    let c = raw_cell(x, y);
    if (c.x < 1.0) { return primary; }
    if (log_scale(c.x) * c.x * T.white_level < T.ad_low_density) {
        return Filt(T.low_off, T.low_size, T.low_half, 0u, T.low_radius, vec3<f32>(0.0, 0.2, 1.1));
    }
    // Scharr operator, including the (x + 1, y - 1) typo of LogDensityFilter.scharr_x.
    let sx = 3.0 * raw_lum(x - 1, y - 1) - 3.0 * raw_lum(x + 1, y - 1) + 10.0 * raw_lum(x - 1, y) - 10.0 * raw_lum(x + 1, y - 1)
        + 3.0 * raw_lum(x - 1, y + 1) - 3.0 * raw_lum(x + 1, y + 1);
    let sy = 3.0 * raw_lum(x - 1, y - 1) + 10.0 * raw_lum(x, y - 1) + 3.0 * raw_lum(x + 1, y - 1)
        - 3.0 * raw_lum(x - 1, y + 1) - 10.0 * raw_lum(x, y + 1) - 3.0 * raw_lum(x + 1, y + 1);
    if (log(1.0 + sqrt(sx * sx + sy * sy)) * 0.434294481903252 < T.ad_sharpness) {
        return Filt(T.soft_off, T.soft_size, T.soft_half, 0u, T.soft_radius, vec3<f32>(0.0, 0.9, 0.0));
    }
    return Filt(T.detail_off, T.detail_size, T.detail_half, 1u, T.detail_radius, vec3<f32>(1.0, 0.2, 0.0));
}

fn round_color(v: f32) -> f32 { return clamp(floor(v + 0.5), 0.0, 255.0); }

fn background(px: u32, py: u32) -> vec3<f32> {
    let w = T.image_w;
    let h = T.image_h;
    if (T.bg_kind == 0u) { return T.bg_single.xyz; }
    if (T.bg_kind == 1u) {
        let fx = f32(px) / f32(max(w, 2u) - 1u);
        let fy = f32(py) / f32(max(h, 2u) - 1u);
        let c = mix(mix(T.bg_ul.xyz, T.bg_ur.xyz, fx), mix(T.bg_ll.xyz, T.bg_lr.xyz, fx), fy);
        return floor(c + 0.5);
    }
    let w2 = f32(max(i32(w) / 2 - 1, 1));
    let h2 = f32(max(i32(h) / 2 - 1, 1));
    let x = f32(px);
    let y = f32(py);
    var fx: f32; var fy: f32;
    var qul: vec3<f32>; var qur: vec3<f32>; var qll: vec3<f32>; var qlr: vec3<f32>;
    let ul = T.bg_ul.xyz; let ur = T.bg_ur.xyz; let ll = T.bg_ll.xyz; let lr = T.bg_lr.xyz; let cc = T.bg_cc.xyz;
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

@compute @workgroup_size(16, 16)
fn tonemap(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= T.image_w || gid.y >= T.image_h) { return; }
    let px = i32(gid.x);
    let py = i32(gid.y);
    let os = i32(T.os);
    var col = vec3<f32>(0.0);
    var intensity = 0.0;
    let f = choose_filter(px, py);
    if (T.adaptive == 2u) {
        var scl = vec3<f32>(1.0);
        if (f.size > 1u) { scl = f.ind * f.radius; }
        for (var sx = 0; sx < os; sx++) {
            for (var sy = 0; sy < os; sy++) {
                let c = cell(px * os + sx, py * os + sy);
                if (c.x > 0.0) {
                    let ls = log_scale(c.x);
                    col += lum3(ls * c.yzw) * scl;
                    intensity += ls * c.x * T.white_level;
                }
            }
        }
    } else if (f.size <= 1u) {
        for (var sx = 0; sx < os; sx++) {
            for (var sy = 0; sy < os; sy++) {
                let c = cell(px * os + sx, py * os + sy);
                if (c.x > 0.0) {
                    let ls = log_scale(c.x);
                    col += ls * c.yzw * T.bal.xyz;
                    intensity += ls * c.x * T.white_level;
                }
            }
        }
    } else {
        let fs = i32(f.size);
        for (var i = 0; i < fs; i++) {
            for (var j = 0; j < fs; j++) {
                let c = cell(px * os + j - f.half, py * os + i - f.half);
                if (c.x > 0.0) {
                    let ls = log_scale(c.x) * filt[f.off + u32(i * fs + j)];
                    col += ls * c.yzw * T.bal.xyz;
                    if (f.sharpen == 0u) { intensity += ls * c.x * T.white_level; }
                }
            }
        }
        if (f.sharpen != 0u) {
            let ss = i32(T.ssize);
            for (var i = 0; i < ss; i++) {
                for (var j = 0; j < ss; j++) {
                    let c = cell(px * os + j - T.shalf, py * os + i - T.shalf);
                    if (c.x > 0.0) {
                        intensity += filt[T.smooth_offset + u32(i * ss + j)] * T.white_level * log_scale(c.x) * c.x;
                    }
                }
            }
        }
    }

    let bg = background(gid.x, gid.y);
    var rgb: vec3<f32>;
    var alpha_out: f32;
    if (intensity > 0.0) {
        var alpha: f32;
        if (intensity <= T.gamma_threshold) {
            let frac = intensity / T.gamma_threshold;
            alpha = (1.0 - frac) * intensity * T.scl_gamma + frac * pow(intensity, T.gamma);
        } else {
            alpha = pow(intensity, T.gamma);
        }
        let log_scl = T.vib_int * alpha / intensity;
        let alpha_int = clamp(floor(alpha * 255.0 * T.alpha_scale + 0.5), 0.0, 255.0);
        let inv_alpha = 255.0 - alpha_int;
        alpha_out = select(255.0, alpha_int, T.with_alpha != 0u);
        var raw = log_scl * col;
        if (T.inv_vib_int > 0.0) {
            raw += T.inv_vib_int * pow(max(col, vec3<f32>(0.0)), vec3<f32>(T.gamma));
        }
        rgb = clamp(floor(raw + 0.5) + floor(inv_alpha * bg / 256.0), vec3<f32>(0.0), vec3<f32>(255.0));
    } else {
        rgb = bg;
        alpha_out = select(255.0, 0.0, T.with_alpha != 0u);
    }
    if (T.mod_sat != 0.0) {
        var hsl = rgb_to_hsl(rgb / 255.0);
        hsl.y += T.mod_sat;
        rgb = vec3<f32>(round_color(hsl_to_rgb(hsl).x * 255.0), round_color(hsl_to_rgb(hsl).y * 255.0), round_color(hsl_to_rgb(hsl).z * 255.0));
    }
    textureStore(out_tex, vec2<i32>(px, py), vec4<f32>(rgb / 255.0, alpha_out / 255.0));
}

// ---- density estimation (port of DeCalculator) ----
struct DeU { raster_w: u32, raster_h: u32, radius: i32, show_splitter: u32, curve: f32, splitter_pos: i32, _p0: u32, _p1: u32 }

@group(0) @binding(0) var<uniform> DU: DeU;
@group(0) @binding(1) var<storage, read> de_src: array<f32>;
@group(0) @binding(2) var<storage, read_write> de_dst: array<f32>;

fn de_erf(z: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.5 * abs(z));
    let ans = 1.0 - t * exp(-z * z - 1.26551223 + t * (1.00002368 + t * (0.37409196 + t * (0.09678418 + t * (-0.18628806 + t * (0.27886807 + t * (-1.13520398 + t * (1.48851587 + t * (-0.82215223 + t * 0.17087277)))))))));
    return select(-ans, ans, z >= 0.0);
}

@compute @workgroup_size(16, 16)
fn density_estimate(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= DU.raster_w || gid.y >= DU.raster_h) { return; }
    let x = i32(gid.x);
    let y = i32(gid.y);
    let w = i32(DU.raster_w);
    let h = i32(DU.raster_h);
    let ci = (gid.y * DU.raster_w + gid.x) * 4u;
    let center_w = floor(de_src[ci] + 0.5);
    let m1 = sqrt(8.0 * center_w) + 5.0;
    let m2 = pow(center_w + 1.0, -0.25);
    var sum = vec4<f32>(0.0);
    var cnt = 0.0;
    let r = DU.radius;
    for (var dy = -r; dy <= r; dy++) {
        let ly = y + dy;
        if (ly < 0 || ly >= h) { continue; }
        for (var dx = -r; dx <= r; dx++) {
            let lx = x + dx;
            if (lx < 0 || lx >= w) { continue; }
            let li = (u32(ly) * DU.raster_w + u32(lx)) * 4u;
            let la = floor(de_src[li] + 0.5);
            let inv_dist = 1.0 / (f32(dx * dx + dy * dy) + 1.0);
            let thresh = pow(DU.curve, sqrt(1.0 / inv_dist)) * m2;
            if (abs(de_erf((la - center_w) / m1)) <= thresh) {
                sum += vec4<f32>(la, de_src[li + 1u], de_src[li + 2u], de_src[li + 3u]) * inv_dist;
                cnt += inv_dist;
            }
        }
    }
    if (DU.show_splitter != 0u && DU.splitter_pos >= x - 2 && DU.splitter_pos <= x + 2) {
        let c = center_w + 10.0;
        de_dst[ci] = c; de_dst[ci + 1u] = 180.0 * c; de_dst[ci + 2u] = 180.0 * c; de_dst[ci + 3u] = 0.0;
    } else if (DU.show_splitter != 0u && DU.splitter_pos < x - 2) {
        de_dst[ci] = center_w; de_dst[ci + 1u] = de_src[ci + 1u]; de_dst[ci + 2u] = de_src[ci + 2u]; de_dst[ci + 3u] = de_src[ci + 3u];
    } else if (cnt > 1.0e-8) {
        let m = sum / cnt;
        de_dst[ci] = floor(m.x + 0.5); de_dst[ci + 1u] = m.y; de_dst[ci + 2u] = m.z; de_dst[ci + 3u] = m.w;
    } else {
        de_dst[ci] = 0.0; de_dst[ci + 1u] = 0.0; de_dst[ci + 2u] = 0.0; de_dst[ci + 3u] = 0.0;
    }
}
