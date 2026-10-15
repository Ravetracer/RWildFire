# The command line

`rwf-cli` (`rwf-cli.exe` on Windows) does most of what the render workspace does, without a
window: for servers, scripts, scheduled jobs and batch work. Run it without arguments to see a
short summary of all commands. Every command reports errors with a clear message and a non-zero
exit code.

## render – an image

```
rwf-cli render <file.flame> [-o out.png|.jpg|.hdr|.exr] [-w width] [-h height] [-q quality]
               [-i index] [--gpu] [-t threads] [--zbuffer] [--jpeg-quality 1-100]
               [--denoiser off|oidn|optix] [--blend 0..1]
```

| Option | Meaning |
|--------|---------|
| `-o` | Output file; the extension chooses the format. Default: the flame file's name with `.png`. |
| `-w`, `-h` | Size. With only one of them, the other follows the flame's aspect ratio; without both, the flame's own size. |
| `-q` | Quality; default: the flame's quality. |
| `-i` | Which flame of a multi-flame file (0 = the first). |
| `--gpu` | Render on the GPU (much faster). Without it, the CPU renderer is used. |
| `-t` | CPU threads (default: all cores). |
| `--zbuffer` | Also write the z-buffer image. |
| `--jpeg-quality` | JPEG compression (default 95). |
| `--denoiser`, `--blend` | Override the flame's denoiser and blend (`optix` = OIDN on an NVIDIA GPU). |

```
rwf-cli render my.flame -o my.png -w 3840 -q 2000 --gpu
```

## render-anim – an animation

```
rwf-cli render-anim <file.flame> -o out.mp4|.mov|.webm|out-dir [-w width] [-h height]
                    [-q quality] [--from frame] [--to frame] [--gpu]
```

Renders the flame's animation as a video (MP4, MOV and WebM need ffmpeg) or as PNG frames into a
folder. `--from` and `--to` limit the frames.

## batch – many flames

```
rwf-cli batch [--size flame|N|WxH] [-q quality] [-f png|jpg|hdr|exr] [--jpeg-quality 1-100]
              [--zbuffer] [--animations mp4|mov|webm|png] [-o out-dir] [--overwrite]
              [--no-denoise] <files or folders...>
```

Renders every flame of the given files and folders on the GPU, like the batch renderer.
`--size` is `flame` (each flame's size), a number (long edge in pixels) or `WIDTHxHEIGHT`.
Without `-o`, images are written next to the flame files. Existing images are skipped unless
`--overwrite` is given. `--animations` renders animated flames as videos.

## quilt – very large images

```
rwf-cli quilt <file.flame> -o out.png -w width [-h height] [-q quality] [--tiles COLSxROWS]
```

Renders in tiles, as the quilt renderer. Run the same command again after an interruption to
continue with the finished tiles.

```
rwf-cli quilt poster.flame -o poster.png -w 30720 -q 1000
```

## random – random flames

```
rwf-cli random <out-dir> [count] [generator filter] [--symmetry NAME] [--weighting NAME]
               [--gradient NAME] [-w width] [-h height] [-q quality] [--gpu]
```

Creates `count` random flames (default 8), each saved as `.flame` with a `.png` preview (480
pixels wide and quality 200 by default). The optional filter chooses generators whose name
contains it, e.g. `bubbles` for Bubbles, Bubbles3D and Bubbles3D (experimental). The extras take
the names shown in the app; an unknown name lists the valid ones.

```
rwf-cli random out 20 julian --symmetry Point --gpu
```

## movie, dance – movies and dancing flames

```
rwf-cli movie <file.jwfmovie | --random [all|mandelbrots|bubbles|duckies]>
              -o out.mp4|.mov|.webm|.anb|out-dir|name.flame|name.jwfmovie [--cpu]
rwf-cli dance <file.rwfdance> -o out.mp4|.mov|.webm|out-dir|name.flame [--cpu]
```

`movie` renders a movie project (or a random movie) with the project's settings; with
`-o name.flame` it writes one flame file per frame, with `-o name.jwfmovie` it saves the
(random) movie project. `dance` renders the recorded show of a dance project, with the music in
video files. Both render on the GPU unless `--cpu` is given.

## mesh – 3D meshes

```
rwf-cli mesh <file.flame | --from-slices DIR> [-o out.obj|.ply|.sc] [--points out.ply|.obj]
             [--slices DIR] [--resolution N] [--quality Q] [--threshold T] [--dilate 3|5]
             [--blur B] [--smooth N]
```

Samples the flame and writes a mesh (OBJ, PLY or Sunflow scene), a point cloud (`--points`)
and/or slice images (`--slices`), with the settings of the mesh generator. `--from-slices`
builds the volume from an image stack instead of a flame.

## denoise – any image

```
rwf-cli denoise <image> [-o out.png|.jpg] [--denoiser oidn|optix] [--blend 0..1]
```

Runs the AI denoiser on a PNG or JPEG file (default output: `<name>_denoised.png`).

## script – Lua scripts

```
rwf-cli script <file.lua> [-i in.flame] [-o out.flame] [--flames out.flame]
               [--param name=value]...
rwf-cli script --info <file.lua>
rwf-cli script --api
rwf-cli script --examples DIR
```

Runs a script (chapter "Scripting") on the first flame of `-i` (or an empty flame) and saves the
changed flame with `-o`. Flames created with `rwf.output` go into one multi-flame file
(`--flames`, default `<script>-flames.flame`). `--param` sets parameters (repeat it for several).
`--info` lists the script's parameters, `--api` prints the scripting reference, `--examples`
copies the example scripts into a folder.

## variations – the variation list

```
rwf-cli variations
```

Lists all variations with whether they run on the GPU and whether they are 3D, one per line
(tab-separated, for further processing).
