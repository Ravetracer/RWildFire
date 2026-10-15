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
    zbuffer: bool,
    jpeg_quality: u8,
    /// Overrides the flame's AI post denoiser and its blend.
    denoiser: Option<rwf_core::render::denoise::Denoiser>,
    blend: Option<f64>,
}

fn usage() -> ! {
    eprintln!(
        "usage: rwf-cli render <file.flame> [-o out.png|.jpg|.hdr|.exr] [-w width] [-h height] [-q quality] [-i flame-index] [--gpu] [-t threads] [--zbuffer] [--jpeg-quality 1-100] [--denoiser off|oidn|optix] [--blend 0..1]\n       rwf-cli batch [options] <files or folders...>   (rwf-cli batch for the options)\n       rwf-cli quilt <file.flame> -o out.png -w width [-h height] [-q quality] [--tiles COLSxROWS]\n       rwf-cli mesh <file.flame | --from-slices DIR> [-o out.obj|.ply|.sc] [--points out.ply|.obj] [--slices DIR] [--resolution N] [--quality Q] [--threshold T] [--dilate 3|5] [--blur B] [--smooth N]\n       rwf-cli movie <file.jwfmovie | --random [all|mandelbrots|bubbles|duckies]> -o out.mp4|.mov|.webm|.anb|out-dir|name.flame|name.jwfmovie [--cpu]\n       rwf-cli dance <file.rwfdance> -o out.mp4|.mov|.webm|out-dir|name.flame [--cpu]\n       rwf-cli random <out-dir> [count] [generator filter] [--symmetry NAME] [--weighting NAME] [--gradient NAME] [-w width] [-h height] [-q quality] [--gpu]"
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
        zbuffer: false,
        jpeg_quality: 95,
        denoiser: None,
        blend: None,
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
            "--zbuffer" => args.zbuffer = true,
            "--jpeg-quality" => args.jpeg_quality = val().parse().unwrap_or(95),
            "--denoiser" => {
                let v = val().to_uppercase();
                args.denoiser = Some(match v.as_str() {
                    "OFF" | "NONE" => rwf_core::render::denoise::Denoiser::None,
                    _ => rwf_core::render::denoise::Denoiser::parse(&v).unwrap_or_else(|| {
                        eprintln!("unknown denoiser {v:?} (off, oidn, optix)");
                        std::process::exit(2);
                    }),
                });
            }
            "--blend" => args.blend = val().parse::<f64>().ok().map(|b| b.clamp(0.0, 1.0)),
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

/// `rwf-cli random <out-dir> [count] [generator filter] [--symmetry NAME] [--weighting NAME]`:
/// generates random flames (.flame + preview .png), cycling through the generators whose name
/// contains the filter. The symmetry and weighting field generators default to
/// "(All, sparse)".
fn random_cmd() {
    use rwf_core::random::gradients::GradientGenerator;
    use rwf_core::random::{Extras, SymmetryGen, WeightingGen};
    let mut extras = Extras::default();
    let mut positional = Vec::new();
    let (mut w, mut h, mut quality, mut gpu) = (480u32, None, 200.0, false);
    let mut args = std::env::args().skip(2);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--symmetry" | "--weighting" | "--gradient" => {
                let name = args.next().unwrap_or_else(|| usage());
                let ok = match a.as_str() {
                    "--symmetry" => SymmetryGen::by_label(&name).map(|g| extras.symmetry = g).is_some(),
                    "--weighting" => WeightingGen::by_label(&name).map(|g| extras.weighting = g).is_some(),
                    _ => GradientGenerator::by_label(&name).map(|g| extras.gradient = g).is_some(),
                };
                if !ok {
                    let names: Vec<&str> = match a.as_str() {
                        "--symmetry" => SymmetryGen::ALL.iter().map(|g| g.label()).collect(),
                        "--weighting" => WeightingGen::ALL.iter().map(|g| g.label()).collect(),
                        _ => GradientGenerator::ALL.iter().map(|g| g.label()).collect(),
                    };
                    eprintln!("unknown {a} {name:?}, one of: {}", names.join(", "));
                    std::process::exit(2);
                }
            }
            "-w" | "-h" | "-q" => {
                let v: f64 = args.next().and_then(|v| v.parse().ok()).filter(|v: &f64| *v > 0.0).unwrap_or_else(|| {
                    eprintln!("{a} needs a positive number");
                    std::process::exit(2);
                });
                match a.as_str() {
                    "-w" => w = v as u32,
                    "-h" => h = Some(v as u32),
                    _ => quality = v,
                }
            }
            "--gpu" => gpu = true,
            _ => positional.push(a),
        }
    }
    let mut it = positional.into_iter();
    let dir = PathBuf::from(it.next().unwrap_or_else(|| usage()));
    let count: usize = it.next().and_then(|s| s.parse().ok()).unwrap_or(8);
    let filter = it.next().unwrap_or_default().to_lowercase();
    let generators: Vec<_> = rwf_core::random::Generator::ALL.iter().copied().filter(|g| g.label().to_lowercase().contains(&filter)).collect();
    if generators.is_empty() {
        eprintln!("no generator matches {filter:?}");
        std::process::exit(2);
    }
    std::fs::create_dir_all(&dir).expect("create output dir");
    let mut renderer = if gpu {
        match rwf_gpu::GpuContext::new_headless() {
            Ok(c) => Some(rwf_gpu::GpuRenderer::new(c)),
            Err(e) => {
                eprintln!("no GPU ({e}), rendering on the CPU");
                None
            }
        }
    } else {
        None
    };
    let mut rng = rwf_core::rng::Rng::from_entropy();
    for i in 0..count {
        let generator = generators[i % generators.len()];
        let flame = rwf_core::random::generate_with(generator, extras, &mut rng);
        let base = dir.join(format!("random_{i:02}_{}", generator.label().replace([' ', '(', ')'], "_")));
        rwf_core::io::write_flame_to_file(&flame, base.with_extension("flame")).expect("write flame");
        let h = h.unwrap_or_else(|| (w as f64 * flame.height as f64 / flame.width.max(1) as f64).round().max(1.0) as u32);
        let img = match renderer.as_mut() {
            Some(r) => {
                r.set_flame(&flame, w, h);
                r.accumulate_frame(&flame, 0.0, w, h, quality).ok().and_then(|_| {
                    r.tonemap();
                    r.read_image()
                })
            }
            None => None,
        }
        .unwrap_or_else(|| rwf_core::render::render_image(&flame, w, h, quality, false));
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
        let mut denoised = None;
        let mut img = match &mut gpu_renderer {
            Some(r) => {
                r.set_with_alpha(false);
                let mut img = r.render_frame(&flame, frame as f64, w, h, q).unwrap_or_else(|e| {
                    eprintln!("{e}");
                    std::process::exit(1);
                });
                denoised = Some(r.apply_post_denoiser(&flame, &mut img, w, h));
                img
            }
            None => {
                let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
                CpuRender::start_frame(&flame, frame as f64, RenderOptions { width: w, height: h, threads, max_density: Some(q), seed: 42 + frame as u64, zbuffer: false })
                    .wait()
                    .image(false)
            }
        };
        if let Err(e) = denoised.unwrap_or_else(|| rwf_core::render::denoise::apply_post_denoiser(&flame, &mut img, w, h, gpu)) {
            eprintln!("warning: denoiser failed: {e}");
        }
        sink.write_frame(frame - from, &img).expect("write frame");
        eprintln!("frame {frame}/{to}");
    }
    sink.finish().expect("finish video");
    eprintln!("rendered {} frames in {:.1}s -> {}", to - from + 1, t0.elapsed().as_secs_f64(), output.display());
}

/// `rwf-cli batch [options] <files or folders...>`: renders every flame of the given files
/// (folders: all `.flame` files in them) on the GPU.
fn batch_cmd() {
    use rwf_gpu::batch::{BatchJob, BatchSettings, SizeMode};
    let batch_usage = || -> ! {
        eprintln!(
            "usage: rwf-cli batch [--size flame|N|WxH] [-q quality] [-f png|jpg|hdr|exr] [--jpeg-quality 1-100] [--zbuffer]\n                 [--animations mp4|mov|webm|png] [-o out-dir] [--overwrite] [--no-denoise] <files or folders...>"
        );
        std::process::exit(2);
    };
    let mut s = BatchSettings::default();
    let mut inputs = Vec::new();
    let mut it = std::env::args().skip(2);
    while let Some(a) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| batch_usage());
        match a.as_str() {
            "--size" => {
                let v = val();
                s.size = match v.split_once('x') {
                    _ if v == "flame" => SizeMode::Flame,
                    Some((w, h)) => SizeMode::Fixed(w.parse().unwrap_or_else(|_| batch_usage()), h.parse().unwrap_or_else(|_| batch_usage())),
                    None => SizeMode::LongEdge(v.parse().unwrap_or_else(|_| batch_usage())),
                };
            }
            "-q" => s.quality = val().parse().unwrap_or_else(|_| batch_usage()),
            "-f" => s.format = rwf_core::output::ImageFormat::from_path(std::path::Path::new(&format!("x.{}", val()))).unwrap_or_else(|| batch_usage()),
            "--jpeg-quality" => s.jpeg_quality = val().parse().unwrap_or(95),
            "--zbuffer" => s.zbuffer = true,
            "--animations" => {
                s.animations = true;
                s.video_format = rwf_core::video::VideoFormat::from_path(std::path::Path::new(&format!("x.{}", val())));
            }
            "-o" => s.out_dir = Some(PathBuf::from(val())),
            "--overwrite" => s.overwrite = true,
            "--no-denoise" => s.denoise = false,
            _ if a.starts_with('-') => batch_usage(),
            _ => inputs.push(PathBuf::from(a)),
        }
    }
    if inputs.is_empty() {
        batch_usage();
    }
    let mut jobs = Vec::new();
    for input in inputs {
        let files = if input.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&input)
                .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("flame"))).collect())
                .unwrap_or_default();
            files.sort();
            files
        } else {
            vec![input]
        };
        for f in files {
            match BatchJob::from_file(&f) {
                Ok(j) => jobs.extend(j),
                Err(e) => eprintln!("skipped: {e}"),
            }
        }
    }
    let mut r = rwf_gpu::GpuRenderer::new(rwf_gpu::GpuContext::new_headless().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    }));
    let t0 = Instant::now();
    let mut failed = 0;
    for (i, job) in jobs.iter().enumerate() {
        let t = Instant::now();
        match rwf_gpu::batch::render_job(&mut r, job, &s, &mut |_, _| true) {
            Ok(out) if out.skipped => eprintln!("[{}/{}] {}: exists, skipped ({})", i + 1, jobs.len(), job.name, out.path.display()),
            Ok(out) => {
                eprintln!("[{}/{}] {} -> {} ({:.1}s)", i + 1, jobs.len(), job.name, out.path.display(), t.elapsed().as_secs_f64());
                if let Some(w) = out.warning {
                    eprintln!("    warning: {w}");
                }
            }
            Err(e) => {
                failed += 1;
                eprintln!("[{}/{}] {}: {e}", i + 1, jobs.len(), job.name);
            }
        }
    }
    eprintln!("{} jobs in {:.1}s, {failed} failed", jobs.len(), t0.elapsed().as_secs_f64());
    if failed > 0 {
        std::process::exit(1);
    }
}

/// `rwf-cli quilt <file.flame> -o out.png -w W [-h H] [-q Q] [--tiles CxR] [-i index]`:
/// renders an image larger than the GPU can render at once, tile by tile.
fn quilt_cmd() {
    let quilt_usage = || -> ! {
        eprintln!("usage: rwf-cli quilt <file.flame> -o out.png -w width [-h height] [-q quality] [--tiles COLSxROWS] [-i flame-index]");
        std::process::exit(2);
    };
    let (mut input, mut output, mut w, mut h, mut q, mut tiles, mut index) = (None, None, None, None, None, None, 0usize);
    let mut it = std::env::args().skip(2);
    while let Some(a) = it.next() {
        let mut val = || it.next().unwrap_or_else(|| quilt_usage());
        match a.as_str() {
            "-o" => output = Some(PathBuf::from(val())),
            "-w" => w = val().parse::<u32>().ok(),
            "-h" => h = val().parse::<u32>().ok(),
            "-q" => q = val().parse::<f64>().ok(),
            "-i" => index = val().parse().unwrap_or(0),
            "--tiles" => tiles = val().split_once('x').and_then(|(c, r)| Some((c.parse::<u32>().ok()?, r.parse::<u32>().ok()?))),
            _ if a.starts_with('-') => quilt_usage(),
            _ => input = Some(PathBuf::from(a)),
        }
    }
    let (Some(input), Some(output), Some(w)) = (input, output, w) else { quilt_usage() };
    let flame = read_flames_from_file(&input).ok().and_then(|f| f.into_iter().nth(index)).unwrap_or_else(|| {
        eprintln!("could not read flame {} of {}", index + 1, input.display());
        std::process::exit(1);
    });
    let h = h.unwrap_or_else(|| (w as f64 * flame.height as f64 / flame.width.max(1) as f64).round() as u32);
    let (cols, rows) = tiles.unwrap_or_else(|| rwf_gpu::quilt::suggest_tiles(w, h, rwf_gpu::quilt::DEFAULT_TILE_SIZE));
    let quality = q.unwrap_or(flame.sample_density);
    let job = rwf_gpu::quilt::QuiltJob { flame, width: w, height: h, cols, rows, quality, output };
    let mut r = rwf_gpu::GpuRenderer::new(rwf_gpu::GpuContext::new_headless().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    }));
    eprintln!("{w}x{h} in {cols}x{rows} tiles, quality {quality}");
    let t0 = Instant::now();
    let mut last = Instant::now();
    let result = rwf_gpu::quilt::render_quilt(&mut r, &job, &mut |e, _| {
        match e {
            rwf_gpu::quilt::QuiltEvent::Progress { tile, tiles, fraction } if last.elapsed().as_secs_f64() >= 2.0 => {
                eprintln!("tile {}/{tiles}: {:.0}%", tile + 1, fraction * 100.0);
                last = Instant::now();
            }
            rwf_gpu::quilt::QuiltEvent::TileDone { rect, .. } => eprintln!("tile r{} c{} done", rect.row, rect.col),
            rwf_gpu::quilt::QuiltEvent::Merging => eprintln!("merging"),
            _ => {}
        }
        true
    });
    match result {
        Ok(p) => eprintln!("rendered in {:.1}s -> {}", t0.elapsed().as_secs_f64(), p.display()),
        Err(e) => {
            eprintln!("quilt failed: {e}");
            std::process::exit(1);
        }
    }
}

/// `rwf-cli movie <file.jwfmovie | --random [all|mandelbrots|bubbles|duckies]> -o out.mp4|.mov|
/// .webm|.anb|out-dir|name.flame|name.jwfmovie [--cpu]`: renders a movie (or JWildfire's random
/// movie), writes its frames as flame files, or saves the (random) movie.
fn movie_cmd() {
    const USAGE: &str = "usage: rwf-cli movie <file.jwfmovie | --random [all|mandelbrots|bubbles|duckies]> -o out.mp4|.mov|.webm|.anb|out-dir|name.flame|name.jwfmovie [--cpu]";
    let args: Vec<String> = std::env::args().skip(2).collect();
    let (mut input, mut output, mut cpu, mut random) = (None, None, false, None);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                output = args.get(i).map(PathBuf::from);
            }
            "--cpu" => cpu = true,
            "--random" => {
                let kind = match args.get(i + 1).map(|s| s.to_lowercase()) {
                    Some(k) if !k.starts_with('-') => {
                        i += 1;
                        k
                    }
                    _ => "all".into(),
                };
                random = Some(match kind.as_str() {
                    "all" => rwf_core::movie::RandomMovie::All,
                    k if k.starts_with("mandel") => rwf_core::movie::RandomMovie::RotatingMandelbrots,
                    k if k.starts_with("bubble") => rwf_core::movie::RandomMovie::TransformingBubbles,
                    k if k.starts_with("duck") => rwf_core::movie::RandomMovie::TransformingDuckies,
                    k => {
                        eprintln!("unknown random movie generator `{k}` (all, mandelbrots, bubbles, duckies)");
                        std::process::exit(2);
                    }
                });
            }
            a => input = Some(PathBuf::from(a)),
        }
        i += 1;
    }
    let Some(output) = output.filter(|_| input.is_some() || random.is_some()) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let movie = match (random, &input) {
        (Some(kind), _) => kind.generate(&mut rwf_core::rng::Rng::from_entropy()),
        (None, Some(input)) => rwf_core::movie::read_movie_file(input).unwrap_or_else(|e| {
            eprintln!("failed to read {}: {e}", input.display());
            std::process::exit(1);
        }),
        (None, None) => unreachable!("checked above"),
    };
    let ext = output.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
    if ext == "jwfmovie" {
        if let Err(e) = rwf_core::movie::write_movie_file(&movie, &output) {
            eprintln!("cannot write {}: {e}", output.display());
            std::process::exit(1);
        }
        eprintln!("{} ({})", output.display(), movie.name);
        return;
    }
    let started = std::time::Instant::now();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let r = if ext == "flame" {
        rwf_core::movie::write_flame_sequence(&output, movie.frame_count(), &cancel, |i| movie.frame_flame(i as f64), |i, total| eprint!("\rflame {}/{total}", i + 1))
            .map(|n| eprintln!("\n{n} flame files next to {}", output.display()))
    } else {
        let format = rwf_core::video::VideoFormat::from_path(&output);
        let gpu = if cpu { None } else { rwf_gpu::GpuContext::new_headless().map_err(|e| eprintln!("no GPU ({e}), rendering on the CPU")).ok() };
        let r = rwf_gpu::movie::render_movie(&movie, &output, format, gpu, &cancel, |i, total, _| {
            eprint!("\rframe {}/{total}  {:.0} s", i + 1, started.elapsed().as_secs_f64());
        });
        eprintln!();
        r.map(|_| eprintln!("{}", output.display()))
    };
    if let Err(e) = r {
        eprintln!("movie failed: {e}");
        std::process::exit(1);
    }
}

/// `rwf-cli dance <file.rwfdance> -o out.mp4|.mov|.webm|out-dir [--cpu]`: renders the recorded
/// performance of a dancing flames project, with the music as audio track.
fn dance_cmd() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let (mut input, mut output, mut cpu) = (None, None, false);
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                output = args.get(i).map(PathBuf::from);
            }
            "--cpu" => cpu = true,
            a => input = Some(PathBuf::from(a)),
        }
        i += 1;
    }
    let (Some(input), Some(output)) = (input, output) else {
        eprintln!("usage: rwf-cli dance <file.rwfdance> -o out.mp4|.mov|.webm|out-dir|name.flame [--cpu]  (name.flame: one flame file per frame)");
        std::process::exit(2);
    };
    let project = rwf_core::dance::read_project_file(&input).unwrap_or_else(|e| {
        eprintln!("failed to read {}: {e}", input.display());
        std::process::exit(1);
    });
    let spectrum = project.sound.as_ref().and_then(|p| match rwf_core::audio::Audio::load(p) {
        Ok(a) => Some(rwf_core::dance::Spectrum::analyse(&a)),
        Err(e) => {
            eprintln!("music not used: {e}");
            None
        }
    });
    if output.extension().is_some_and(|e| e.eq_ignore_ascii_case("flame")) {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        match project.write_performance_flames(spectrum.as_ref(), &output, &cancel, |i, total| eprint!("\rflame {}/{total}", i + 1)) {
            Ok(n) => eprintln!("\n{n} flame files next to {}", output.display()),
            Err(e) => {
                eprintln!("\ndance failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let format = rwf_core::video::VideoFormat::from_path(&output);
    let gpu = if cpu { None } else { rwf_gpu::GpuContext::new_headless().map_err(|e| eprintln!("no GPU ({e}), rendering on the CPU")).ok() };
    let started = std::time::Instant::now();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let r = rwf_gpu::dance::render_performance(&project, spectrum.as_ref(), &output, format, gpu, &cancel, |i, total, _| {
        eprint!("\rframe {}/{total}  {:.0} s", i + 1, started.elapsed().as_secs_f64());
    });
    eprintln!();
    if let Err(e) = r {
        eprintln!("dance failed: {e}");
        std::process::exit(1);
    }
    eprintln!("{}", output.display());
}

/// `rwf-cli mesh <file.flame | --from-slices DIR> [-o out.obj|.ply|.sc] [--points out.ply|.obj]
/// [--slices DIR] [--resolution N] [--quality Q] [--threshold T] [--dilate 3|5] [--blur B]
/// [--smooth PASSES] [--no-colors]`: a 3D mesh, point cloud or slice stack of the flame (mesh
/// generator); `--from-slices` builds the volume from an image stack instead.
fn mesh_cmd() {
    const USAGE: &str = "usage: rwf-cli mesh <file.flame | --from-slices DIR> [-o out.obj|out.ply|out.sc] [--points out.ply|out.obj] [--slices DIR] [--resolution N] [--quality Q] [--threshold T] [--dilate 3|5] [--blur B] [--smooth PASSES] [--no-colors]";
    let args: Vec<String> = std::env::args().skip(2).collect();
    let (mut input, mut output, mut colors) = (None, None, true);
    let (mut points, mut slices, mut from_slices) = (None, None, None);
    let mut opts: Vec<(String, f64)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            o @ ("-o" | "--points" | "--slices" | "--from-slices") => {
                i += 1;
                let Some(p) = args.get(i).map(PathBuf::from) else {
                    eprintln!("{o} needs a path");
                    std::process::exit(2);
                };
                *match o {
                    "-o" => &mut output,
                    "--points" => &mut points,
                    "--slices" => &mut slices,
                    _ => &mut from_slices,
                } = Some(p);
            }
            "--no-colors" => colors = false,
            o @ ("--resolution" | "--quality" | "--threshold" | "--blur" | "--smooth" | "--dilate") => {
                i += 1;
                let v = args.get(i).and_then(|v| v.parse().ok()).unwrap_or_else(|| {
                    eprintln!("{o} needs a number");
                    std::process::exit(2);
                });
                opts.push((o.to_string(), v));
            }
            a => input = Some(PathBuf::from(a)),
        }
        i += 1;
    }
    if (input.is_none() && from_slices.is_none()) || (output.is_none() && points.is_none() && slices.is_none()) {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    let flame = input.as_ref().map(|input| {
        let flame = read_flames_from_file(input).ok().and_then(|f| f.into_iter().next()).unwrap_or_else(|| {
            eprintln!("failed to read {}", input.display());
            std::process::exit(1);
        });
        flame.at_frame(0.0)
    });
    let bounds = flame.as_ref().map_or(rwf_core::meshgen::Bounds { min: [-1.0; 3], max: [1.0; 3] }, rwf_core::meshgen::auto_bounds);
    let mut s = rwf_core::meshgen::MeshSettings::new(bounds);
    for (o, v) in opts {
        match o.as_str() {
            "--resolution" => s.resolution = v.clamp(8.0, 2048.0) as u32,
            "--quality" => s.quality = v.max(0.1),
            "--threshold" => s.threshold = v.clamp(0.0, 1.0),
            "--blur" => s.blur = v.max(0.0),
            "--dilate" => s.dilate = if v >= 4.0 { 5 } else if v >= 2.0 { 3 } else { 0 },
            _ => s.smooth_passes = v.max(0.0) as u32,
        }
    }
    let started = std::time::Instant::now();
    let fail = |what: &str, e: String| -> ! {
        eprintln!("{what}: {e}");
        std::process::exit(1);
    };
    let volume = match (&from_slices, &flame) {
        (Some(dir), _) => {
            let files: Vec<PathBuf> = std::fs::read_dir(dir)
                .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| matches!(e.to_ascii_lowercase().to_str(), Some("png" | "jpg" | "jpeg")))).collect())
                .unwrap_or_default();
            rwf_core::meshgen::load_slices(&files, s.resolution).unwrap_or_else(|e| fail("cannot read the slices", e))
        }
        (None, Some(flame)) => rwf_core::meshgen::sample_volume(flame, &s, &std::sync::atomic::AtomicU64::new(0), &std::sync::atomic::AtomicBool::new(false)),
        (None, None) => unreachable!("checked above"),
    };
    let density = volume.density_filtered(s.dilate, s.blur);
    if let Some(dir) = &slices {
        let n = rwf_core::meshgen::save_slices(&volume, &density, dir).unwrap_or_else(|e| fail("cannot write the slices", e));
        eprintln!("{} ({n} slices)", dir.display());
    }
    if let Some(path) = &points {
        let pts = volume.point_cloud(&density, s.threshold);
        rwf_core::meshgen::save_points(&pts, path).unwrap_or_else(|e| fail(&format!("cannot write {}", path.display()), e.to_string()));
        eprintln!("{} ({} points)", path.display(), pts.len());
    }
    if let Some(output) = &output {
        let mut mesh = rwf_core::meshgen::surface_nets(&volume, &density, s.threshold as f32);
        if s.smooth_passes > 0 {
            mesh.taubin_smooth(s.smooth_passes, s.smooth_lambda, s.smooth_mu);
        }
        if !colors {
            mesh.colors.clear();
        }
        mesh.save(output, colors, true).unwrap_or_else(|e| fail(&format!("cannot write {}", output.display()), e.to_string()));
        eprintln!("{} ({} vertices, {} triangles)", output.display(), mesh.positions.len(), mesh.triangles.len());
    }
    eprintln!("done in {:.1} s", started.elapsed().as_secs_f64());
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("mesh") {
        mesh_cmd();
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("movie") {
        movie_cmd();
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("dance") {
        dance_cmd();
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("quilt") {
        quilt_cmd();
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("batch") {
        batch_cmd();
        return;
    }
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
    let mut flame = flame.clone();
    if let Some(d) = args.denoiser {
        flame.ai_post_denoiser = d;
    }
    if let Some(b) = args.blend {
        flame.post_denoiser_blend = b;
    }
    let flame = &flame;
    let (w, h) = match (args.width, args.height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, (w as f64 * flame.height as f64 / flame.width as f64).round() as u32),
        (None, Some(h)) => (((h as f64) * flame.width as f64 / flame.height as f64).round() as u32, h),
        (None, None) => (flame.width, flame.height),
    };
    let quality = args.quality.unwrap_or(flame.sample_density);
    let Some(format) = rwf_core::output::ImageFormat::from_path(&args.output) else {
        eprintln!("unsupported output format: {} (png, jpg, hdr, exr)", args.output.display());
        std::process::exit(1);
    };
    let gpu = args.gpu;
    let t0 = Instant::now();
    if flame.stereo3d.is_on() {
        // JWildfire renders stereo images without HDR and z-buffer
        if format.is_hdr() {
            eprintln!("stereo 3D images are written as PNG or JPEG, not {}", args.output.display());
            std::process::exit(1);
        }
        if args.zbuffer {
            eprintln!("warning: no z-buffer for stereo 3D images");
        }
        let ctx = if gpu {
            match rwf_gpu::GpuContext::new_headless() {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("GPU render failed: {e}");
                    std::process::exit(1);
                }
            }
        } else {
            None
        };
        let mut progress = |done: u32, total: u32| eprintln!("eye image {done}/{total}");
        match rwf_gpu::stereo::render_stereo_image(flame, w, h, quality, ctx, args.threads, &mut progress) {
            Ok(img) => {
                if let Err(e) = rwf_core::output::write_rgba8(&args.output, format, img.width, img.height, &img.rgba, args.jpeg_quality) {
                    eprintln!("failed to write {}: {e}", args.output.display());
                    std::process::exit(1);
                }
                eprintln!("{}", img.denoise_note);
                eprintln!("rendered {}x{} @ quality {} in {:.2}s -> {}", img.width, img.height, quality, t0.elapsed().as_secs_f64(), args.output.display());
            }
            Err(e) => {
                eprintln!("stereo render failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let mut zbuffer = None;
    let mut hdr = None;
    let mut gpu_denoise_note = None;
    let rgba = if gpu {
        let result = (|| -> Result<Vec<u8>, String> {
            let mut r = rwf_gpu::GpuRenderer::new(rwf_gpu::GpuContext::new_headless()?);
            gpu_denoise_note = None;
            r.set_with_alpha(flame.bg_transparency);
            r.set_zbuffer(args.zbuffer);
            r.set_flame(flame, w, h);
            if !r.unsupported_variations().is_empty() {
                eprintln!("warning: variations without GPU support ignored: {}", r.unsupported_variations().join(", "));
            }
            r.accumulate_frame(flame, 0.0, w, h, quality)?;
            if args.zbuffer {
                zbuffer = r.read_zbuffer();
            }
            if format.is_hdr() {
                hdr = Some(r.read_hdr().ok_or("HDR readback failed")?);
                return Ok(Vec::new());
            }
            r.tonemap();
            let mut img = r.read_image().ok_or_else(|| "readback failed".to_string())?;
            gpu_denoise_note = Some(r.apply_post_denoiser(flame, &mut img, w, h));
            Ok(img)
        })();
        match result {
            Ok(img) => img,
            Err(e) => {
                eprintln!("GPU render failed: {e}");
                std::process::exit(1);
            }
        }
    } else {
        let opts = RenderOptions { width: w, height: h, threads: args.threads, max_density: Some(quality), seed: 42, zbuffer: args.zbuffer };
        let r = CpuRender::start(flame, opts);
        if !r.missing_variations().is_empty() {
            eprintln!("warning: unsupported variations ignored: {}", r.missing_variations().join(", "));
        }
        let r = r.wait();
        eprintln!("{} samples", r.samples());
        if args.zbuffer {
            zbuffer = r.zbuffer_image();
        }
        if format.is_hdr() {
            hdr = Some(r.image_hdr());
            Vec::new()
        } else {
            r.image(flame.bg_transparency)
        }
    };
    let mut rgba = rgba;
    if hdr.is_none() {
        let outcome = gpu_denoise_note.take().unwrap_or_else(|| rwf_core::render::denoise::apply_post_denoiser(flame, &mut rgba, w, h, gpu));
        match outcome {
            Ok(o) if o.denoised() || flame.ai_post_denoiser != rwf_core::render::denoise::Denoiser::None => eprintln!("{}", o.describe()),
            Ok(_) => {}
            Err(e) => eprintln!("warning: denoiser failed: {e}"),
        }
    }
    let dt = t0.elapsed();
    let written = match &hdr {
        Some(rgb) => rwf_core::output::write_float_rgb(&args.output, format, w, h, rgb),
        None => rwf_core::output::write_rgba8(&args.output, format, w, h, &rgba, args.jpeg_quality),
    };
    if let Err(e) = written {
        eprintln!("failed to write {}: {e}", args.output.display());
        std::process::exit(1);
    }
    if let Some(z) = zbuffer {
        let zpath = flame.zbuffer_filename.path_for(&args.output);
        if let Err(e) = rwf_core::output::write_gray16_png(&zpath, w, h, &z) {
            eprintln!("failed to write {}: {e}", zpath.display());
            std::process::exit(1);
        }
        eprintln!("z-buffer -> {}", zpath.display());
    }
    eprintln!("rendered {}x{} @ quality {} in {:.2}s -> {}", w, h, quality, dt.as_secs_f64(), args.output.display());
}
