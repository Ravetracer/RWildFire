# Introduction

RWildFire is a program for creating, editing and rendering *fractal flames*: images that
come from the repeated application of a handful of simple geometric transformations to millions
of points. It is a port of [JWildfire](https://github.com/thargor6/JWildfire), the well-known
Java flame editor by Andreas Maschke, to the Rust programming language. RWildFire reads and
writes the same flame files, contains the same variations (more than 1000 of them), the same
random generators and most of JWildfire's tools, but it renders on the graphics card (GPU) and
has a redesigned, calmer user interface.

This manual describes RWildFire version **{{version}}**. It explains every part of the
program: the editor with its panels, the random flame tools, animation, the render workspace
(interactive renderer, batch renderer, very large images, movies, dancing flames, 3D meshes),
scripting, the preferences and the command line program.

![RWildFire with a flame loaded in the editor](images/main-window.png)

## What RWildFire can do

- **Edit flames** interactively: drag the transforms on the preview, change any of more than
  1000 variations and their parameters, adjust colors, camera, tone mapping and lighting, and
  watch the preview refine within seconds.
- **Create flames** with 55 random flame generators, random gradients, symmetries and weighting
  fields, browse batches of random flames and mutations, or steer a flame step by step with
  MutaGen.
- **Animate** any property with keyframes and motion curves, generate curves from music, make
  movies from flames that morph into each other, and let flames dance to music in real time.
- **Render** images of any size (up to gigapixel images in tiles), in PNG, JPEG, HDR and
  OpenEXR, with depth maps, stereo 3D, depth of field, solid (lit and shadowed) rendering and an
  AI denoiser; render queues of flames in the batch renderer.
- **Turn 3D flames into meshes** (OBJ, PLY, Sunflow scenes, point clouds, slice images).
- **Automate** with Lua scripts and the command line program `rwf-cli`.

## RWildFire and JWildfire

RWildFire opens JWildfire flames completely: layers, motion curves, weighting fields, solid
rendering, background transforms, resources such as images, meshes and embedded sub-flames.
Flames saved by RWildFire use the same XML format, so JWildfire opens them as long as they only
use variations both programs know.

What is different:

- **GPU rendering.** The preview and nearly all renders run on the graphics card. A typical
  image renders dozens of times faster than on the CPU. The CPU renderer (itself two to three
  times faster than JWildfire's) is still there: it is the exact reference and the fallback when there is no suitable GPU.
- **A single editor window** with panels instead of many tabs and dialogs. Tools that
  replace the editor (render workspace, flame browser) are reached from the top bar and
  return to the same flame.
- **Lua scripts** in place of JWildfire's Java scripts (see chapter "Scripting").
- **Not included**: JWildfire's image processing module, variations that run user-written
  Java or GLSL code (`custom_wf`, `dc_code`, `glsl_code` and similar), the choice of random
  number generator, LeapMotion support and the applet/launcher.

## System requirements

| Component | Requirement |
|-----------|-------------|
| Operating system | Linux (x86-64) or Windows 10/11 (64 bit) |
| Graphics | A GPU with Vulkan (Linux, Windows) or DirectX 12 (Windows) support. Without one, RWildFire uses its CPU renderer, which works but is much slower. |
| Memory | 4 GB; large renders (8K and more) and mesh generation need more |
| Optional: ffmpeg | For MP4, MOV and WebM videos. It must be installed and in the `PATH`. Without it, animations can still be saved as PNG sequences, ANB files or flame sequences. |
| Optional: NVIDIA GPU | The AI denoiser can run on NVIDIA graphics cards (CUDA); otherwise it runs on the CPU. |

The AI denoiser, Intel® Open Image Denoise, is part of the download. If its library is missing,
RWildFire still works, only without the denoiser; the program says so wherever a denoised image
would be saved.

## Installation and first start

Download the zip file for your system and unpack it into any folder. Start

- `rwf-app` (Linux) or `rwf-app.exe` (Windows) for the editor,
- `rwf-cli` / `rwf-cli.exe` in a terminal for the command line program.

No installation is needed. You can open a flame directly by passing its file name, e.g.
`rwf-app myflame.flame`, or by dropping the file onto the open window.

RWildFire starts with a random flame from the "Classic" generator. Press <kbd>Space</kbd> for
another one, or open a flame with <kbd>Ctrl</kbd>+<kbd>O</kbd>.

### Building from source

RWildFire is open source (LGPL 3.0 or later). With a Rust toolchain installed:

```
cargo run --release -p rwf-app            # the editor
cargo run --release -p rwf-cli -- render in.flame -o out.png --gpu  # the command line program
python3 tools/fetch_oidn.py               # optional: the AI denoiser library
```

The optional `taprats` variation is licensed under the GPL and therefore not in the default
build; `cargo build --release -p rwf-app --features taprats` adds it.

### Where RWildFire keeps its files

| What | Linux | Windows |
|------|-------|---------|
| Preferences | `~/.config/rwildfire/preferences.txt` | `%APPDATA%\rwildfire\preferences.txt` |
| Your gradients | `~/.config/rwildfire/gradients/` | `%APPDATA%\rwildfire\gradients\` |
| Your scripts (default) | `~/.config/rwildfire/scripts/` | `%APPDATA%\rwildfire\scripts\` |
| Quick saves (default) | `~/RWildFire/` | `%USERPROFILE%\RWildFire\` |

The default folders of the file dialogs and the quick save folder can be changed in the
preferences.

## About this manual

- Menu commands are written as **File › Export image…**: the menu, then the command.
- Keys are shown as <kbd>Ctrl</kbd>+<kbd>S</kbd>. On macOS <kbd>Ctrl</kbd> is <kbd>Cmd</kbd>.
- Almost every control in RWildFire shows a short explanation when you hold the mouse over it.
  This manual explains the same controls in more depth: what they do, what changes when you
  move them and how they work together.
- Chapter 2 explains the ideas behind fractal flames. If you are new to flames, read it first;
  the rest of the manual builds on its terms (transform, variation, affine, weight, xaos, …).
