# RWildFire

A fractal flame editor in Rust, ported from [JWildfire](https://github.com/thargor6/JWildfire), with a GPU renderer (wgpu compute shaders) and a redesigned, flat UI (egui).

## Run

```sh
cargo run --release -p rwf-app                      # editor
cargo run --release -p rwf-app -- some.flame        # open a file
cargo run --release -p rwf-cli -- render in.flame -o out.png -w 1920 -h 1080 -q 1000 --gpu   # also .jpg, .hdr, .exr; --zbuffer adds a 16-bit depth PNG
cargo run --release -p rwf-cli -- random out_dir 8  # random flames (.flame + .png)
cargo run --release -p rwf-cli -- render-anim in.flame -o out.mp4 --gpu   # animation (mp4/mov/webm via ffmpeg, or a folder for PNGs)
cargo run --release -p rwf-cli -- batch --size 3840 -q 2000 -o out_dir flames/   # all flames of files/folders (GPU); --size flame|N|WxH, -f png|jpg|hdr|exr, --animations mp4
cargo run --release -p rwf-cli -- quilt in.flame -o big.png -w 30720 -q 1000   # very large images in tiles (--tiles 8x5 to choose the grid)
```

## Workspace

| Crate      | Purpose |
|------------|---------|
| `rwf-core` | Flame model, `.flame` reader/writer (JWildfire + flam3 compatible), variations, multi-threaded CPU renderer, log-density filter + gamma (ported 1:1 from `LogDensityFilter` / `GammaCorrectionFilter`), random generators |
| `rwf-gpu`  | wgpu renderer. A WGSL iteration kernel is generated per flame *structure*; all numbers live in a storage buffer, so parameter edits never recompile |
| `rwf-app`  | Desktop editor (eframe/egui, shares the wgpu device with the renderer) |
| `rwf-cli`  | Headless rendering and random flame generation |

### Variations

Each variation is a `VarDef` with a CPU function and a WGSL snippet that read the same prepared data array (`crates/rwf-core/src/variations/`). 1014 variations are ported, all of them with GPU support (1015 with the optional `taprats` feature). Pre/post variations of priority 2 (`prepost_*`, `ringtile`) run their inverse in the pre slot and their forward transform in the post slot, as in JWildfire. Unknown variations in loaded files are kept on save and reported in the UI. Variations with external resources (`plane_wf` color/displacement maps, `colormap_wf`, `displacemap_wf`, `post_bumpmap_wf`, `kaleidoimg`, `metaballs3d_wf`, `brush_stroke_wf` with the 245 built-in brushes, `wangtiles` with JWildfire's 103 tile sets, `obj_mesh_wf` / `obj_mesh_primitive_wf` / `dla3d_wf` meshes; files, embedded images, image sequences) load images and OBJ files once into a cache and pack them into the prepared data array, so the GPU reads them like ordinary parameters; resource values are stored hex-encoded in `.flame` files, as in JWildfire. Formula plots (`parplot2d_wf`, `polarplot2d_wf`, `polarplot3d_wf`, `yplot2d_wf`, `yplot3d_wf`, `isosfplot3d_wf`, `knots3D`, `sattractor3D`, with JWildfire's presets) compile their Java-expression formulas to a small bytecode that the CPU and a WGSL interpreter both run (`crates/rwf-core/src/formula.rs`). `svg_wf` (resvg) and `text_wf` (system fonts) render their shape into an image whose pixels become the points. `taprats` (optional, GPL, see below) builds Craig S. Kaplan's Islamic star patterns (Taprats: 49 tilings from `assets/taprats/tilings.txt`, rosettes, stars and inferred figures, plain/outline/interlace/emboss/filled/sketch styles) once per parameter set and samples the resulting lines and triangles. `subflame_wf`, `pre_subflame_wf`, `ringsubflame` and `glynns3subfl` run the chaos game of a complete embedded flame; on the GPU every sub-flame runs as its own kernel that refills a point buffer before each iteration pass, and the variations pick random points from it (`crates/rwf-gpu/src/subflames.rs`). `iflames_wf` places up to six embedded flames on the edges of an image (JWildfire's image filters, seeded placement and particle motion, so the same parameters place the same flames) and runs them as sub-flames; on the GPU, flames with per-instance parameter values share at most 24 sub-flames with quantized values.
`cargo test -p rwf-gpu` validates every WGSL snippet with naga.

## Performance

`testdata/benchmark1.flame`, 1920×1080, quality 1000 (16-thread laptop CPU, RTX A2000 Laptop GPU):

| Renderer | Time |
|----------|------|
| JWildfire 9.20 (Java, CPU) | 256.7 s |
| rwf-cli (CPU) | 151.5 s |
| rwf-cli `--gpu` | 3.3 s |

## UI concept

- **Canvas first:** the flame takes the center; transforms are edited directly with on-canvas triangles (drag the origin/body to move, the X/Y handles to reshape, Shift-drag to rotate, Ctrl-drag to scale; drag empty space to pan, scroll wheel to zoom at the cursor).
- **Left:** transform list (with gradient color swatch) and the variations of the selected transform, with a searchable variation picker.
- **Right:** one inspector with four tabs: *Transform*, *Camera*, *Look* (tone, background, gradient), *Render* (renderer, quality, filter, symmetry).
- **Explore strip:** batches of random flames, mutations of the current flame, or all flames of a multi-flame file, as clickable thumbnails.
- Tone changes (brightness, gamma, vibrancy, background, ...) update instantly without restarting the render.
- Unlimited undo/redo, drag & drop of `.flame` files, keyboard shortcuts (Ctrl+O/S/E/Z, Space = random flame, F = fit, Del = delete transform).

## Rendering features

- Density estimation (`de_radius`, `de_curve`, comparison line), CPU and GPU, enabled by default as in JWildfire
- Depth of field: legacy z-plane and focus-point modes, bokeh shapes (bubble plus the variation-based JWildfire shapes)
- Layers: add, duplicate, delete, hide, weight and density per layer
- Post symmetry, 3D camera, anti-aliasing, filter kernels
- Adaptive filter (`MITCHELL_SINEPOW`): per-pixel kernel choice by edge strength and density, with JWildfire's indicator view
- Post blur (radius, fade, falloff)
- AI post denoiser: Intel Open Image Denoise (OIDN 2.5) on exported images and video frames, on the CPU or on NVIDIA (CUDA) and AMD (HIP) GPUs. GPU renders give it helper images (albedo = the average sample color per pixel; for solid flames the object color and the surface normals), which keep edges and colors better. "Denoised" in the canvas toolbar and "Show denoised" in the interactive renderer preview the result; saving reports what the denoiser did or why it did not run. Edit › Preferences › "Always denoise" gives every flame without a denoiser a preferred denoiser and blend (new, random, Explore, MutaGen, opened files); the canvas shows it only with "Denoised" (saved in `preferences.txt` of the config folder). The library is loaded at runtime, so RWildFire builds and runs without it. `python3 tools/fetch_oidn.py` downloads the official release (SHA-256 checked) into `oidn/` (about 72 MB: core with the network weights, CPU, CUDA and HIP devices, oneTBB, licenses). Search order: `RWF_OIDN_DIR`, next to the executable (`lib/` on Linux and macOS, the executable's folder on Windows; `tools/fetch_oidn.py --dest <app dir>` fills it for a package), `oidn/` of the source tree, the system library path. Without the library the `oidnDenoise` tool is used if it is in `PATH` (or `JWF_OIDN_DENOISE`). AMD GPUs need the ROCm HIP runtime installed; NVIDIA GPUs only the driver
- Solid rendering, CPU and GPU: z-buffer with surface normals, distant lights, materials (per-transform material index, blended between neighbours, optional PNG/JPEG reflection maps), ambient occlusion, fast or smooth shadow maps, and post DOF with bokeh. Material, light color and AO changes re-shade without restarting the render. `plane_wf` can act as a shadow-only ground plane; light altitude/azimuth/intensity are animatable (JWildfire light curves are imported)
- Stereo 3D images as in JWildfire: anaglyph (red/cyan), side by side, or interpolated pairs at growing eye distance; each eye is rendered with its share of the quality and turned around the focal point. In exported images, `rwf-cli render` and the batch renderer (PNG/JPEG; the canvas shows the normal view)
- Background image behind the flame (stretched to the image size; background xforms add on top), depth fade towards a color, and layer gradient maps (an image as a two-dimensional gradient for diffusion/cyclic coloring, mirror-tiled, with offset, scale and color index influence)

## Gradients

- JWildfire's gradient tools: modifications (shift, frequency, blur, hue, saturation, brightness, contrast, gamma, red/green/blue, RGB swap), range operations (copy/paste, erase, fade, monochrome, reverse, sort, negative, fade all), hue/saturation/luminosity curve editor (JWildfire's curves are imported), gradients from images (median cut, or an image's first row in the library), from flame files, `.map`/`.ugr` export, and JWildfire's random gradient generators
- Key-point editor: gradients are fitted to a few color stops that can be dragged, added (click the bar) and recolored
- Library with ~1600 bundled gradients (flam3 classics, UltraFractal collections), user gradients in `~/.config/rwildfire/gradients`, import of `.ugr` / `.map` / flam3 palette files
- Relative adjustments (shift, hue, saturation, lightness), random gradients

## Animation

- Keyframe tracks on any numeric property (camera, tone, layers, gradient color cycle, transform coefficients, variation amounts and parameters) with smooth, linear or hold interpolation, or JWildfire's spline and Bezier
- Timeline with playback, scrubbing, per-track keyframe lanes, auto-key (edits at the current frame create keyframes) and quick keying of camera / transform / tone
- Motion blur (JWildfire-compatible sub-frames)
- Export as MP4 (H.264), MOV (ProRes), WebM (VP9) via ffmpeg, or PNG sequences, rendered in the background on the GPU or CPU
- JWildfire motion curves are imported when opening animated `.flame` files: flame, layer, transform (incl. relative rotate/scale), variation amount and parameter curves and solid light curves, with JWildfire's spline/Bezier evaluation; parent curves and relative rotate/scale are baked into a key per frame

## Status / roadmap

Done: core editor, file compatibility, CPU and GPU renderer, 1014 variations (incl. prepost, image and mesh resources, formula plots and curves, SVG and text, sub-flames; `taprats` as an optional GPL feature), gradient editing, 54 of JWildfire's random flame generators (all but Duckies, Color map, Wikimedia Commons and the script generator), JWildfire's random gradient, symmetry and weighting field generators (top bar, Explore strip and `rwf-cli random --symmetry/--weighting`), random batches in the Explore strip (size, quality, replace/insert/append, snapshots of the editor flame, cancel), quick mutation strip, resolution and quality profiles (editable in Preferences, remembered by flames as JWildfire's `resolution_profile`/`quality_profile`, chosen in the export dialog and the batch renderer, defaults for new flames), preferences for default folders of the file dialogs, settings of new flames (title, filter, oversampling, DE, white level, animation length), preview quality and time limit, free CPU cores and the style of the transform controls (triangles, rectangles, axes, crosshairs; colors, numbers, shadows), MutaGen with all 19 mutation types (no user lists, no morph amount), PNG export, density estimation, DOF, layers, animation and video export, movie maker (flames that fade or morph into each other, camera and transform scripts, motion blur; JWildfire `.jwfmovie` files; Render workspace and `rwf-cli movie`), dancing flames (a pool of flames moved by music in real time: FFT-band, sine and saw-tooth motions linked to any property or to xform rotate/move/zoom, flame switches with morphing, recorded shows rendered to video with the music; `.rwfdance` projects; Render workspace and `rwf-cli dance`), mesh generator (3D flames to triangle meshes: voxel sampling, Surface Nets, Taubin smoothing; OBJ and PLY with vertex colors; Render workspace and `rwf-cli mesh`), curve editor for animation tracks with envelopes from audio files (MP3, WAV, FLAC, Ogg: loudness of a frequency band per frame), adaptive filter, post blur, AI denoiser, solid rendering, xform weighting fields (parameter modulation on CPU and GPU, animated settings incl. JWildfire motion curves), channel mixer, background xforms, JPEG/HDR/OpenEXR and z-buffer output (CPU and GPU), interactive renderer (long progressive GPU renders with pause, resume and saved render state), batch renderer (render queue in the app and `rwf-cli batch`), quilt renderer (images larger than the GPU can render at once, in seamless tiles; app and `rwf-cli quilt`), flame browser (the flames of a folder and its sub-folders as thumbnails, grouped by month and day; open, to batch or mesh, rename, copy, move, delete).

Missing compared with JWildfire (from a feature comparison, by priority):

- Niche:
  - Mesh generator: point cloud output, slice export / import as PNG stack, dilate pre-filter, Sunflow export
  - Dancing flames: spline motion, balancing preprocessor, export of a recorded show as `.flame` files
  - Movie maker: random movie generators; animation output as `.flame` sequence or ANB
  - CLI: `random` with gradient generator, size, quality and GPU; denoiser override for `render`
  - Help/FAQ window, choice of random number generator, denoiser for external images

Optional / later:

- Scripting (e.g. Lua or Rhai in place of JWildfire's Java scripts)
- Ready-to-run downloads for Windows and Linux: a build script that compiles `rwf-app` and `rwf-cli` in release mode, fetches OIDN for the target platform (`tools/fetch_oidn.py --platform … --dest …`; into `lib/` on Linux, next to the executable on Windows) and packs everything with the license files into one zip per platform, so users only unzip and run
- User manual: complete documentation of all app functions (editor, random generators and Explore, MutaGen, animation and curve editor, render workspace with interactive, batch, quilt, movie, dance and mesh, export, denoiser, preferences, CLI) with screenshots, written in a source format kept in the repository and built to a PDF that ships with the release zips

Not planned: variations that run user-written Java/GLSL code or formula engines (`custom_wf`, `c_var`, `dc_code`, `glsl_code`, `f_complex`, `colordomain`, `cut_c`, `ducks`, `fract_formula_*`), JWildfire's classic image-processing module, LeapMotion, applet and launcher.

## License

RWildFire is a port of [JWildfire](https://github.com/thargor6/JWildfire) (Copyright (C) 1995-2026 Andreas Maschke, LGPL 2.1 or later) and is licensed under the [GNU Lesser General Public License v3.0 or later](https://www.gnu.org/licenses/lgpl-3.0.html) (`LGPL-3.0-or-later`). See [LICENSE](LICENSE) (LGPL v3), [COPYING](COPYING) (GPL v3, on which the LGPL v3 builds) and [NOTICE](NOTICE).

The `taprats` variation is a port of Taprats (Copyright (C) 2000 Craig S. Kaplan, 2010 Pierre Baillargeon), which is licensed under the GPL v3 or later. It is therefore not part of the default build: `cargo build --release -p rwf-app --features taprats` (likewise `-p rwf-cli`) adds it, and a build with it is covered by the GPL v3 as a whole. Without the feature, flames that use `taprats` keep the variation on save and report it as unknown.
