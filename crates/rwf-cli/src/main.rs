//! Command line renderer: `rwf-cli render <file.flame> -o out.png [options]`.

use std::path::PathBuf;
use std::time::Instant;

use rwf_core::io::read_flames_from_file;
use rwf_core::render::{CpuRender, RenderOptions};

struct Args {
    input: PathBuf,
    output: PathBuf,
    width: Option<u32>,
    height: Option<u32>,
    quality: Option<f64>,
    index: usize,
    gpu: bool,
    threads: usize,
}

fn usage() -> ! {
    eprintln!(
        "usage: rwf-cli render <file.flame> [-o out.png] [-w width] [-h height] [-q quality] [-i flame-index] [--gpu] [-t threads]"
    );
    std::process::exit(2);
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    if it.next().as_deref() != Some("render") {
        usage();
    }
    let mut args = Args {
        input: PathBuf::new(),
        output: PathBuf::new(),
        width: None,
        height: None,
        quality: None,
        index: 0,
        gpu: false,
        threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4),
    };
    while let Some(a) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| usage());
        match a.as_str() {
            "-o" => args.output = PathBuf::from(val()),
            "-w" => args.width = val().parse().ok(),
            "-h" => args.height = val().parse().ok(),
            "-q" => args.quality = val().parse().ok(),
            "-i" => args.index = val().parse().unwrap_or(0),
            "-t" => args.threads = val().parse().unwrap_or(args.threads),
            "--gpu" => args.gpu = true,
            _ if args.input.as_os_str().is_empty() => args.input = PathBuf::from(a),
            _ => usage(),
        }
    }
    if args.input.as_os_str().is_empty() {
        usage();
    }
    if args.output.as_os_str().is_empty() {
        args.output = args.input.with_extension("png");
    }
    args
}

fn write_png(path: &PathBuf, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    writer.write_image_data(rgba).map_err(std::io::Error::other)?;
    Ok(())
}

/// `rwf-cli random <out-dir> [count]`: generates random flames (.flame + preview .png).
fn random_cmd() {
    let mut it = std::env::args().skip(2);
    let dir = PathBuf::from(it.next().unwrap_or_else(|| usage()));
    let count: usize = it.next().and_then(|s| s.parse().ok()).unwrap_or(8);
    std::fs::create_dir_all(&dir).expect("create output dir");
    let mut rng = rwf_core::rng::Rng::from_entropy();
    for i in 0..count {
        let generator = rwf_core::random::Generator::ALL[i % rwf_core::random::Generator::ALL.len()];
        let flame = rwf_core::random::generate(generator, &mut rng);
        let base = dir.join(format!("random_{i:02}_{}", generator.label().replace(' ', "_")));
        rwf_core::io::write_flame_to_file(&flame, base.with_extension("flame")).expect("write flame");
        let (w, h) = (480, 270);
        let img = rwf_core::render::render_image(&flame, w, h, 200.0, false);
        write_png(&base.with_extension("png"), w, h, &img).expect("write png");
        eprintln!("{}", base.display());
    }
}

/// `rwf-cli render-anim <file.flame> -o out.mp4|out_dir [-w W] [-h H] [-q Q] [--gpu] [--from F] [--to T]`
fn render_anim_cmd() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let mut input = None;
    let mut output = None;
    let (mut w, mut h, mut q, mut gpu, mut from, mut to) = (None, None, None, false, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().unwrap_or_else(|| usage());
        match a.as_str() {
            "-o" => output = Some(PathBuf::from(val())),
            "-w" => w = val().parse::<u32>().ok(),
            "-h" => h = val().parse::<u32>().ok(),
            "-q" => q = val().parse::<f64>().ok(),
            "--from" => from = val().parse::<u32>().ok(),
            "--to" => to = val().parse::<u32>().ok(),
            "--gpu" => gpu = true,
            _ => input = Some(PathBuf::from(a)),
        }
    }
    let (Some(input), Some(output)) = (input, output) else { usage() };
    let flame = read_flames_from_file(&input).ok().and_then(|f| f.into_iter().next()).unwrap_or_else(|| {
        eprintln!("could not read {}", input.display());
        std::process::exit(1);
    });
    let w = w.unwrap_or(flame.width);
    let h = h.unwrap_or_else(|| (w as f64 * flame.height as f64 / flame.width as f64).round() as u32);
    let q = q.unwrap_or(flame.sample_density);
    let from = from.unwrap_or(0);
    let to = to.unwrap_or(flame.animation.frame_count.saturating_sub(1));
    let format = rwf_core::video::VideoFormat::from_path(&output);
    let mut sink = rwf_core::video::VideoSink::new(&output, format, w, h, flame.animation.fps).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let mut gpu_renderer = gpu.then(|| rwf_gpu::GpuRenderer::new(rwf_gpu::GpuContext::new_headless().expect("GPU")));
    let t0 = Instant::now();
    for frame in from..=to {
        let mut img = match &mut gpu_renderer {
            Some(r) => {
                r.set_with_alpha(false);
                r.render_frame(&flame, frame as f64, w, h, q).unwrap_or_else(|e| {
                    eprintln!("{e}");
                    std::process::exit(1);
                })
            }
            None => {
                let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
                CpuRender::start_frame(&flame, frame as f64, RenderOptions { width: w, height: h, threads, max_density: Some(q), seed: 42 + frame as u64 })
                    .wait()
                    .image(false)
            }
        };
        if let Err(e) = rwf_core::render::denoise::apply_post_denoiser(&flame, &mut img, w, h, gpu) {
            eprintln!("warning: denoiser failed: {e}");
        }
        sink.write_frame(frame - from, &img).expect("write frame");
        eprintln!("frame {frame}/{to}");
    }
    sink.finish().expect("finish video");
    eprintln!("rendered {} frames in {:.1}s -> {}", to - from + 1, t0.elapsed().as_secs_f64(), output.display());
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("render-anim") {
        render_anim_cmd();
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("variations") {
        for d in rwf_core::variations::all() {
            println!("{}\t{}\t{}", d.name, if d.supports_gpu() { "gpu" } else { "cpu" }, if d.is_3d { "3d" } else { "2d" });
        }
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("random") {
        random_cmd();
        return;
    }
    let args = parse_args();
    let flames = match read_flames_from_file(&args.input) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("failed to read {}: {e}", args.input.display());
            std::process::exit(1);
        }
    };
    let Some(flame) = flames.get(args.index) else {
        eprintln!("flame index {} out of range ({} flames)", args.index, flames.len());
        std::process::exit(1);
    };
    let (w, h) = match (args.width, args.height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, (w as f64 * flame.height as f64 / flame.width as f64).round() as u32),
        (None, Some(h)) => (((h as f64) * flame.width as f64 / flame.height as f64).round() as u32, h),
        (None, None) => (flame.width, flame.height),
    };
    let quality = args.quality.unwrap_or(flame.sample_density);
    let t0 = Instant::now();
    let rgba = if args.gpu {
        match rwf_gpu::render_blocking(flame, w, h, quality, flame.bg_transparency) {
            Ok(img) => img,
            Err(e) => {
                eprintln!("GPU render failed: {e}");
                std::process::exit(1);
            }
        }
    } else {
        let r = CpuRender::start(flame, RenderOptions { width: w, height: h, threads: args.threads, max_density: Some(quality), seed: 42 });
        if !r.missing_variations().is_empty() {
            eprintln!("warning: unsupported variations ignored: {}", r.missing_variations().join(", "));
        }
        let r = r.wait();
        eprintln!("{} samples", r.samples());
        r.image(flame.bg_transparency)
    };
    let mut rgba = rgba;
    match rwf_core::render::denoise::apply_post_denoiser(flame, &mut rgba, w, h, args.gpu) {
        Ok(true) => eprintln!("denoised ({})", flame.ai_post_denoiser.label()),
        Ok(false) => {}
        Err(e) => eprintln!("warning: denoiser failed: {e}"),
    }
    let dt = t0.elapsed();
    if let Err(e) = write_png(&args.output, w, h, &rgba) {
        eprintln!("failed to write {}: {e}", args.output.display());
        std::process::exit(1);
    }
    eprintln!("rendered {}x{} @ quality {} in {:.2}s -> {}", w, h, quality, dt.as_secs_f64(), args.output.display());
}
