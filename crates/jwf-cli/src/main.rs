//! Command line renderer: `jwf-cli render <file.flame> -o out.png [options]`.

use std::path::PathBuf;
use std::time::Instant;

use jwf_core::io::read_flames_from_file;
use jwf_core::render::{CpuRender, RenderOptions};

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
        "usage: jwf-cli render <file.flame> [-o out.png] [-w width] [-h height] [-q quality] [-i flame-index] [--gpu] [-t threads]"
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

fn main() {
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
        match jwf_gpu::render_blocking(flame, w, h, quality, flame.bg_transparency) {
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
    let dt = t0.elapsed();
    if let Err(e) = write_png(&args.output, w, h, &rgba) {
        eprintln!("failed to write {}: {e}", args.output.display());
        std::process::exit(1);
    }
    eprintln!("rendered {}x{} @ quality {} in {:.2}s -> {}", w, h, quality, dt.as_secs_f64(), args.output.display());
}
