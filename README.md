# RWildFire

A fractal flame editor in Rust, ported from [JWildfire](https://github.com/thargor6/JWildfire), with a GPU renderer (wgpu compute shaders) and a redesigned, flat UI (egui).

## Run

```sh
cargo run --release -p rwf-app                      # editor
cargo run --release -p rwf-app -- some.flame        # open a file
cargo run --release -p rwf-cli -- render in.flame -o out.png -w 1920 -h 1080 -q 1000 --gpu
cargo run --release -p rwf-cli -- random out_dir 8  # random flames (.flame + .png)
cargo run --release -p rwf-cli -- render-anim in.flame -o out.mp4 --gpu   # animation (mp4/mov/webm via ffmpeg, or a folder for PNGs)
```

## Workspace

| Crate      | Purpose |
|------------|---------|
| `rwf-core` | Flame model, `.flame` reader/writer (JWildfire + flam3 compatible), variations, multi-threaded CPU renderer, log-density filter + gamma (ported 1:1 from `LogDensityFilter` / `GammaCorrectionFilter`), random generators |
| `rwf-gpu`  | wgpu renderer. A WGSL iteration kernel is generated per flame *structure*; all numbers live in a storage buffer, so parameter edits never recompile |
| `rwf-app`  | Desktop editor (eframe/egui, shares the wgpu device with the renderer) |
| `rwf-cli`  | Headless rendering and random flame generation |

### Variations

Each variation is a `VarDef` with a CPU function and a WGSL snippet that read the same prepared data array (`crates/rwf-core/src/variations/`). 686 variations are ported, nearly all of them with GPU support. Pre/post variations of priority 2 (`prepost_*`, `ringtile`) run their inverse in the pre slot and their forward transform in the post slot, as in JWildfire. Unknown variations in loaded files are kept on save and reported in the UI. Variations with external resources (`plane_wf` color/displacement maps, `colormap_wf`, `displacemap_wf`, `post_bumpmap_wf`, `kaleidoimg`, `metaballs3d_wf`, `brush_stroke_wf` with the 245 built-in brushes, `obj_mesh_wf` / `obj_mesh_primitive_wf` / `dla3d_wf` meshes; files, embedded images, image sequences) load images and OBJ files once into a cache and pack them into the prepared data array, so the GPU reads them like ordinary parameters; resource values are stored hex-encoded in `.flame` files, as in JWildfire. Formula plots (`parplot2d_wf`, `polarplot2d_wf`, `polarplot3d_wf`, `yplot2d_wf`, `yplot3d_wf`, `isosfplot3d_wf`, `knots3D`, `sattractor3D`, with JWildfire's presets) compile their Java-expression formulas to a small bytecode that the CPU and a WGSL interpreter both run (`crates/rwf-core/src/formula.rs`). `svg_wf` (resvg) and `text_wf` (system fonts) render their shape into an image whose pixels become the points. `subflame_wf` runs the chaos game of a complete embedded flame (CPU only, as in JWildfire).
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
- AI post denoiser: runs Intel Open Image Denoise's `oidnDenoise` (found in `PATH` or via `JWF_OIDN_DENOISE`) on exported images and video frames
- Solid rendering, CPU and GPU: z-buffer with surface normals, distant lights, materials (per-transform material index, blended between neighbours, optional PNG/JPEG reflection maps), ambient occlusion, fast or smooth shadow maps, and post DOF with bokeh. Material, light color and AO changes re-shade without restarting the render. `plane_wf` can act as a shadow-only ground plane; light altitude/azimuth/intensity are animatable (JWildfire light curves are imported)

## Gradients

- Key-point editor: gradients are fitted to a few color stops that can be dragged, added (click the bar) and recolored
- Library with ~1600 bundled gradients (flam3 classics, UltraFractal collections), user gradients in `~/.config/rwildfire/gradients`, import of `.ugr` / `.map` / flam3 palette files
- Relative adjustments (shift, hue, saturation, lightness), random gradients

## Animation

- Keyframe tracks on any numeric property (camera, tone, layers, transform coefficients, variation amounts and parameters) with smooth, linear or hold interpolation
- Timeline with playback, scrubbing, per-track keyframe lanes, auto-key (edits at the current frame create keyframes) and quick keying of camera / transform / tone
- Motion blur (JWildfire-compatible sub-frames)
- Export as MP4 (H.264), MOV (ProRes), WebM (VP9) via ffmpeg, or PNG sequences, rendered in the background on the GPU or CPU
- JWildfire motion curves are imported when opening animated `.flame` files

## Status / roadmap

Done: core editor, file compatibility, CPU and GPU renderer, 686 variations (incl. prepost, image and mesh resources, formula plots and curves, SVG and text, sub-flames), gradient editing, random generators, mutations, PNG export, density estimation, DOF, layers, animation and video export, adaptive filter, post blur, AI denoiser, solid rendering.

Next:
- remaining JWildfire variations (~500; 13 batch files are prepared but empty, see `variations/batch_w*.rs` and the `wNN` features)
- resource-based variations (images, formulas, subflames, SVG, text)

## License

RWildFire is a port of [JWildfire](https://github.com/thargor6/JWildfire) (Copyright (C) 1995-2026 Andreas Maschke, LGPL 2.1 or later) and is licensed under the [GNU Lesser General Public License v3.0 or later](https://www.gnu.org/licenses/lgpl-3.0.html) (`LGPL-3.0-or-later`). See [LICENSE](LICENSE) (LGPL v3), [COPYING](COPYING) (GPL v3, on which the LGPL v3 builds) and [NOTICE](NOTICE).
