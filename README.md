# JWildfire (Rust)

A Rust port of [JWildfire](https://github.com/thargor6/JWildfire)'s fractal flame editor, with a GPU renderer (wgpu compute shaders) and a redesigned, flat UI (egui).

## Run

```sh
cargo run --release -p jwf-app                      # editor
cargo run --release -p jwf-app -- some.flame        # open a file
cargo run --release -p jwf-cli -- render in.flame -o out.png -w 1920 -h 1080 -q 1000 --gpu
cargo run --release -p jwf-cli -- random out_dir 8  # random flames (.flame + .png)
cargo run --release -p jwf-cli -- render-anim in.flame -o out.mp4 --gpu   # animation (mp4/mov/webm via ffmpeg, or a folder for PNGs)
```

## Workspace

| Crate      | Purpose |
|------------|---------|
| `jwf-core` | Flame model, `.flame` reader/writer (JWildfire + flam3 compatible), variations, multi-threaded CPU renderer, log-density filter + gamma (ported 1:1 from `LogDensityFilter` / `GammaCorrectionFilter`), random generators |
| `jwf-gpu`  | wgpu renderer. A WGSL iteration kernel is generated per flame *structure*; all numbers live in a storage buffer, so parameter edits never recompile |
| `jwf-app`  | Desktop editor (eframe/egui, shares the wgpu device with the renderer) |
| `jwf-cli`  | Headless rendering and random flame generation |

### Variations

Each variation is a `VarDef` with a CPU function and a WGSL snippet that read the same prepared data array (`crates/jwf-core/src/variations/`). 140 variations are ported, all of them with GPU support. Unknown variations in loaded files are kept on save and reported in the UI.
`cargo test -p jwf-gpu` validates every WGSL snippet with naga.

## Performance

`testdata/benchmark1.flame`, 1920×1080, quality 1000 (16-thread laptop CPU, RTX A2000 Laptop GPU):

| Renderer | Time |
|----------|------|
| JWildfire 9.20 (Java, CPU) | 256.7 s |
| jwf-cli (CPU) | 151.5 s |
| jwf-cli `--gpu` | 3.3 s |

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

## Gradients

- Key-point editor: gradients are fitted to a few color stops that can be dragged, added (click the bar) and recolored
- Library with ~1600 bundled gradients (flam3 classics, UltraFractal collections), user gradients in `~/.config/jwildfire-rs/gradients`, import of `.ugr` / `.map` / flam3 palette files
- Relative adjustments (shift, hue, saturation, lightness), random gradients

## Animation

- Keyframe tracks on any numeric property (camera, tone, layers, transform coefficients, variation amounts and parameters) with smooth, linear or hold interpolation
- Timeline with playback, scrubbing, per-track keyframe lanes, auto-key (edits at the current frame create keyframes) and quick keying of camera / transform / tone
- Motion blur (JWildfire-compatible sub-frames)
- Export as MP4 (H.264), MOV (ProRes), WebM (VP9) via ffmpeg, or PNG sequences, rendered in the background on the GPU or CPU
- JWildfire motion curves are imported when opening animated `.flame` files

## Status / roadmap

Done (milestone 1): core editor, file compatibility, CPU and GPU renderer, 140 variations, gradient editing, random generators, mutations, PNG export.

Next:
- remaining JWildfire variations (~510; 13 batch files are prepared but empty, see `variations/batch_w*.rs` and the `wNN` features)
- "prepost" (priority 2) variations need inverse-transform support
- adaptive filter, solid rendering, post-blur, denoiser
