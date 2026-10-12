//! Multi-threaded, progressive CPU renderer.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::JoinHandle;

use super::prepared::{NEXT_XFORM_TABLE_SIZE, PreparedFlame, PreparedLayer};
use super::tonemap::{Raster, ToneMapper};
use super::view::View;
use crate::math::{EPSILON, M_2PI, M_PI, hsl_to_rgb, rgb_to_hsl, round_color};
use crate::model::{Flame, PostSymmetryType};
use crate::variations::{Ctx, Pt};

const INITIAL_ITERATIONS: usize = 42;
/// Fixed-point scale of the color channels in the atomic histogram.
const COLOR_SCALE: f64 = 1024.0;

/// Shared accumulation raster: `[count, r, g, b]` per cell, colors in signed fixed point.
pub struct Histogram {
    width: u32,
    height: u32,
    cells: Vec<AtomicU64>,
}

impl Histogram {
    pub fn new(width: u32, height: u32) -> Self {
        let n = width as usize * height as usize * 4;
        let mut cells = Vec::with_capacity(n);
        cells.resize_with(n, || AtomicU64::new(0));
        Self { width, height, cells }
    }

    #[inline]
    pub fn add(&self, x: u32, y: u32, r: f64, g: f64, b: f64) {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.cells[i].fetch_add(1, Ordering::Relaxed);
        self.cells[i + 1].fetch_add((r * COLOR_SCALE) as i64 as u64, Ordering::Relaxed);
        self.cells[i + 2].fetch_add((g * COLOR_SCALE) as i64 as u64, Ordering::Relaxed);
        self.cells[i + 3].fetch_add((b * COLOR_SCALE) as i64 as u64, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        for c in &self.cells {
            c.store(0, Ordering::Relaxed);
        }
    }
}

impl Raster for Histogram {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    #[inline]
    fn cell(&self, x: u32, y: u32) -> [f64; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let ld = |k: usize| self.cells[i + k].load(Ordering::Relaxed) as i64 as f64 / COLOR_SCALE;
        [self.cells[i].load(Ordering::Relaxed) as f64, ld(1), ld(2), ld(3)]
    }
}

/// Plain f32 raster (e.g. read back from the GPU).
pub struct FloatRaster {
    pub width: u32,
    pub height: u32,
    pub data: Vec<f32>,
}

impl Raster for FloatRaster {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }
    #[inline]
    fn cell(&self, x: u32, y: u32) -> [f64; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.data[i] as f64, self.data[i + 1] as f64, self.data[i + 2] as f64, self.data[i + 3] as f64]
    }
}

/// Everything a render thread needs, shared read-only.
struct Shared {
    prepared: PreparedFlame,
    view: View,
    histogram: Histogram,
    samples: AtomicU64,
    stop: AtomicBool,
    target_samples: u64,
    antialias_amount: f64,
    antialias_radius: f64,
    symmetry: Symmetry,
    dimish_color: [f64; 3],
}

#[derive(Clone)]
struct Symmetry {
    kind: PostSymmetryType,
    centre: (f64, f64),
    /// Point symmetry rotations (sin, cos).
    rotations: Vec<(f64, f64)>,
    halve_dist: f64,
    rot: Option<(f64, f64)>,
}

impl Symmetry {
    fn new(flame: &Flame) -> Self {
        let ps = &flame.post_symmetry;
        let mut rotations = Vec::new();
        if ps.kind == PostSymmetryType::Point && ps.order > 1 {
            let order = ps.order.min(64);
            let da = M_2PI / order as f64;
            for i in 0..order {
                let a = da * i as f64;
                rotations.push(a.sin_cos());
            }
        }
        let a = ps.rotation * M_2PI / 180.0 / 2.0;
        Self {
            kind: if ps.kind == PostSymmetryType::Point && ps.order <= 1 { PostSymmetryType::None } else { ps.kind },
            centre: (ps.centre_x, ps.centre_y),
            rotations,
            halve_dist: ps.distance / 2.0,
            rot: if a.abs() > EPSILON { Some(a.sin_cos()) } else { None },
        }
    }
}

struct LayerState {
    p: Pt,
    xf: usize,
    valid: bool,
}

pub struct CpuRender {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
    flame: Flame,
}

pub struct RenderOptions {
    pub width: u32,
    pub height: u32,
    pub threads: usize,
    /// Stop after this sample density (flame quality); `None` renders until stopped.
    pub max_density: Option<f64>,
    pub seed: u64,
}

impl CpuRender {
    pub fn start(flame: &Flame, opts: RenderOptions) -> Self {
        let prepared = PreparedFlame::new(flame);
        let filter_size = flame.filter_kernel.filter_size(flame.spatial_filter_radius, flame.spatial_oversampling.max(1));
        let view = View::new(flame, opts.width.max(1), opts.height.max(1), filter_size);
        let raster_size = view.raster_width as u64 * view.raster_height as u64;
        let target_samples = match opts.max_density {
            Some(d) => (d * raster_size as f64 / view.oversample as f64 + 0.5) as u64,
            None => u64::MAX,
        };
        let shared = Arc::new(Shared {
            histogram: Histogram::new(view.raster_width, view.raster_height),
            view,
            samples: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            target_samples,
            antialias_amount: flame.antialias_amount,
            antialias_radius: flame.antialias_radius,
            symmetry: Symmetry::new(flame),
            dimish_color: [0.0; 3],
            prepared,
        });
        let n = opts.threads.max(1);
        let mut threads = Vec::with_capacity(n);
        if !shared.prepared.layers.is_empty() {
            for i in 0..n {
                let sh = shared.clone();
                let seed = opts.seed.wrapping_add((i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                threads.push(
                    std::thread::Builder::new()
                        .name(format!("jwf-render-{i}"))
                        .spawn(move || render_thread(sh, seed))
                        .expect("spawn render thread"),
                );
            }
        }
        Self { shared, threads, flame: flame.clone() }
    }

    pub fn samples(&self) -> u64 {
        self.shared.samples.load(Ordering::Relaxed)
    }

    /// Current sample density ("quality" in JWildfire terms).
    pub fn density(&self) -> f64 {
        let v = &self.shared.view;
        self.samples() as f64 / (v.raster_width as f64 * v.raster_height as f64) * v.oversample as f64
    }

    pub fn is_finished(&self) -> bool {
        self.threads.iter().all(|t| t.is_finished())
    }

    pub fn view(&self) -> &View {
        &self.shared.view
    }

    pub fn missing_variations(&self) -> &[String] {
        &self.shared.prepared.missing_variations
    }

    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
    }

    pub fn wait(mut self) -> Self {
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        self
    }

    /// Tonemaps the current state into an RGBA8 image.
    pub fn image(&self, with_alpha: bool) -> Vec<u8> {
        let v = &self.shared.view;
        let tm = ToneMapper::new(&self.flame, v.image_width, v.image_height, v.ppu, self.density(), with_alpha);
        tm.render(&self.flame, &self.shared.histogram, v.image_width, v.image_height)
    }
}

impl Drop for CpuRender {
    fn drop(&mut self) {
        self.stop();
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

/// Blocking convenience render to RGBA8.
pub fn render_image(flame: &Flame, width: u32, height: u32, quality: f64, with_alpha: bool) -> Vec<u8> {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let r = CpuRender::start(flame, RenderOptions { width, height, threads, max_density: Some(quality), seed: 0x5EED_1234 }).wait();
    r.image(with_alpha)
}

fn render_thread(sh: Arc<Shared>, seed: u64) {
    let mut ctx = Ctx::new(seed, sh.prepared.preserve_z);
    let layers = &sh.prepared.layers;
    let mut states: Vec<LayerState> = layers.iter().map(|_| LayerState { p: Pt::default(), xf: 0, valid: false }).collect();
    for (l, st) in layers.iter().zip(states.iter_mut()) {
        pre_fuse(l, st, &mut ctx);
    }
    const BATCH: u64 = 4096;
    let mut local: u64 = 0;
    loop {
        if sh.stop.load(Ordering::Relaxed) {
            break;
        }
        for (l, st) in layers.iter().zip(states.iter_mut()) {
            if !st.valid || st.p.is_bad() {
                pre_fuse(l, st, &mut ctx);
                if !st.valid {
                    continue;
                }
            }
            for _ in 0..BATCH {
                iterate(&sh, l, st, &mut ctx);
            }
        }
        local += BATCH * layers.len() as u64;
        let total = sh.samples.fetch_add(BATCH * layers.len() as u64, Ordering::Relaxed);
        if total >= sh.target_samples {
            break;
        }
        let _ = local;
    }
}

fn pre_fuse(layer: &PreparedLayer, st: &mut LayerState, ctx: &mut Ctx) {
    st.p = Pt { x: 2.0 * ctx.random() - 1.0, y: 2.0 * ctx.random() - 1.0, z: 0.0, color: ctx.random(), ..Pt::default() };
    st.xf = 0;
    st.valid = false;
    if layer.xforms.is_empty() {
        return;
    }
    st.p = layer.xforms[0].transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
    for _ in 0..=INITIAL_ITERATIONS {
        let Some(next) = &layer.xforms[st.xf].next else {
            return;
        };
        st.xf = next[ctx.random_int(NEXT_XFORM_TABLE_SIZE)] as usize;
        st.p = layer.xforms[st.xf].transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
    }
    st.valid = true;
}

#[inline]
fn iterate(sh: &Shared, layer: &PreparedLayer, st: &mut LayerState, ctx: &mut Ctx) {
    if layer.density < 1.0 && ctx.random() > layer.density * layer.density {
        return;
    }
    let Some(next) = &layer.xforms[st.xf].next else {
        st.valid = false;
        return;
    };
    st.xf = next[ctx.random_int(NEXT_XFORM_TABLE_SIZE)] as usize;
    let xf = &layer.xforms[st.xf];
    st.p = xf.transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
    if !xf.is_visible_sample(ctx) {
        return;
    }
    let q = if layer.finals.is_empty() {
        st.p
    } else {
        let mut q = layer.finals[0].transform(ctx, &layer.colormap, layer.smooth_gradient, &st.p);
        for f in &layer.finals[1..] {
            q = f.transform(ctx, &layer.colormap, layer.smooth_gradient, &q);
        }
        q
    };
    if q.do_hide {
        return;
    }
    let sym = &sh.symmetry;
    match sym.kind {
        PostSymmetryType::None => plot(sh, layer, &st.p, &q, q.x, q.y, ctx),
        PostSymmetryType::Point => {
            let (cx, cy) = sym.centre;
            let dx = q.x - cx;
            let dy = q.y - cy;
            plot(sh, layer, &st.p, &q, q.x, q.y, ctx);
            for &(s, c) in &sym.rotations {
                plot(sh, layer, &st.p, &q, cx + dx * c + dy * s, cy + dy * c - dx * s, ctx);
            }
        }
        PostSymmetryType::XAxis | PostSymmetryType::YAxis => {
            let (cx, cy) = sym.centre;
            let (mut ax, mut ay, mut bx, mut by) = (q.x, q.y, q.x, q.y);
            if sym.kind == PostSymmetryType::XAxis {
                let dx = q.x - cx;
                ax = cx + dx + sym.halve_dist;
                bx = cx - dx - sym.halve_dist;
            } else {
                let dy = q.y - cy;
                ay = cy + dy + sym.halve_dist;
                by = cy - dy - sym.halve_dist;
            }
            if let Some((s, c)) = sym.rot {
                let (dx, dy) = (ax - cx, ay - cy);
                ax = cx + dx * c + dy * s;
                ay = cy + dy * c - dx * s;
                let (dx, dy) = (bx - cx, by - cy);
                bx = cx + dx * c - dy * s;
                by = cy + dy * c + dx * s;
            }
            plot(sh, layer, &st.p, &q, ax, ay, ctx);
            plot(sh, layer, &st.p, &q, bx, by, ctx);
        }
    }
}

#[inline]
fn plot(sh: &Shared, layer: &PreparedLayer, p: &Pt, q: &Pt, x: f64, y: f64, ctx: &mut Ctx) {
    let Some((mut raw_x, mut raw_y, dz)) = sh.view.project(x, y, q.z) else {
        return;
    };
    if !(x.is_finite() && y.is_finite()) {
        return;
    }
    if sh.antialias_amount > EPSILON && sh.antialias_radius > EPSILON && ctx.random() > 1.0 - sh.antialias_amount {
        let dr = (sh.antialias_radius * (-ctx.random().ln()).sqrt()).exp() - 1.0;
        let da = ctx.random() * 2.0 * M_PI;
        raw_x += dr * da.cos();
        raw_y += dr * da.sin();
    }
    let sx = (raw_x + 0.5) as i64;
    let sy = (raw_y + 0.5) as i64;
    let v = &sh.view;
    if sx < 0 || sy < 0 || sx >= v.raster_width as i64 || sy >= v.raster_height as i64 {
        return;
    }
    let (mut r, mut g, mut b) = (q.red, q.green, q.blue);
    transform_plot_color(p, &mut r, &mut g, &mut b);
    let w = layer.weight;
    let d = sh.dimish_color;
    r = (d[0] + (r - d[0]) * dz) * w;
    g = (d[1] + (g - d[1]) * dz) * w;
    b = (d[2] + (b - d[2]) * dz) * w;
    sh.histogram.add(sx as u32, sy as u32, r, g, b);
}

#[inline]
fn transform_plot_color(p: &Pt, r: &mut f64, g: &mut f64, b: &mut f64) {
    if p.mod_gamma.abs() > EPSILON {
        let gamma = 4.2 / (4.2 - p.mod_gamma);
        let alpha = *r * 0.299 + *g * 0.588 + *b * 0.113;
        if alpha > EPSILON {
            let m = alpha.powf(gamma) / alpha;
            *r *= m;
            *g *= m;
            *b *= m;
        }
    }
    if p.mod_contrast.abs() > EPSILON {
        let gamma = 1.2 / (1.2 - p.mod_contrast * 0.5);
        *r = (*r - 127.5) * gamma + 127.5;
        *g = (*g - 127.5) * gamma + 127.5;
        *b = (*b - 127.5) * gamma + 127.5;
    }
    if p.mod_saturation.abs() > EPSILON {
        let avg = *r * 0.299 + *g * 0.588 + *b * 0.113;
        *r += (*r - avg) * p.mod_saturation;
        *g += (*g - avg) * p.mod_saturation;
        *b += (*b - avg) * p.mod_saturation;
    }
    if p.mod_hue.abs() > EPSILON {
        let (h, s, l) = rgb_to_hsl(*r / 255.0, *g / 255.0, *b / 255.0);
        let (r2, g2, b2) = hsl_to_rgb(h + p.mod_hue, s, l);
        *r = round_color(r2 * 255.0) as f64;
        *g = round_color(g2 * 255.0) as f64;
        *b = round_color(b2 * 255.0) as f64;
    }
}
