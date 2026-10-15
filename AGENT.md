# AGENT.md – working on RWildFire

Notes for coding agents (and humans) working in this repository. RWildFire is a Rust port of
JWildfire, a fractal flame editor. The port is feature-complete; work from here on is
maintenance, fixes, new features and releases. User-facing documentation is the manual in
`docs/manual/`; this file is about how to work on the code.

## Owner and working style

- The owner (Christian) is an experienced developer. Keep answers short: a few bullet points,
  no essays. Report outcomes honestly (failed tests, skipped steps, limitations).
- **Use only half of the CPU cores** for builds, tests and heavy jobs (the machine has 16):
  `cargo build -j 8`, `cargo test -j 8 -- --test-threads=8`, `RAYON_NUM_THREADS=8`,
  `rwf-cli render -t 8`, `docker run --cpus=8`.
- Token cost matters: at most two parallel sub-agents, no large agent fan-outs without asking.
- Keep `target/` small: the dev profile already uses line tables only (it once grew to 38 GB).
  Run `cargo clean` after each finished milestone, never while a build or test is running.
- Milestone routine (no need to ask): commit, `git push origin main`, `cargo clean`, report.
- Versioning: X.Y.Z with X = major, Y = new features, Z = fixes. Bump all crates together
  (`Cargo.toml` workspace version and every `crates/*/Cargo.toml`, including the path
  dependency versions).
- Commit messages: plain, neutral English, what and why; end with the co-author line the
  session asks for. No persona or jokes in commits, code, comments or docs.
- Help texts matter to the owner: every control gets a hover text; actions report what they
  did or why they were skipped, never fail silently.
- JWildfire files are imported completely, but RWildFire does not export back to JWildfire
  formats other than the shared `.flame` XML.

## Layout

| Path | Content |
|------|---------|
| `crates/rwf-core` | Flame model (`model.rs`), `.flame` IO (`io/`), 1014 variations (`variations/`), CPU renderer (`render/`), tone mapping, DE, filters, denoiser glue (`render/denoise.rs`, `render/oidn.rs`), random generators and mutations (`random/`), animation (`animation.rs`), movie maker, dance, mesh generator, solid rendering, weighting fields, formulas, resources |
| `crates/rwf-gpu` | wgpu renderer: per-flame generated WGSL (`codegen.rs`), `renderer.rs`, `tonemap.wgsl`, `solid.wgsl`, batch, quilt, movie, dance, stereo, sub-flames, render state |
| `crates/rwf-script` | Lua 5.4 scripting (mlua, vendored): `api.rs`, header/params in `lib.rs`, `API.md` (reference, also shown in the app and the manual), example scripts in `scripts/` |
| `crates/rwf-app` | egui/eframe editor. `app.rs` (top level, actions), `doc.rs` (document, undo, multi-flame save), `canvas.rs`, `ui/` (transforms panel, inspector tabs, widgets, top bar), workspaces (`render_view`, `batch_view`, `quilt_view`, `movie_view`, `dance_view`, `mesh_view`, `browser`), `explorer.rs`, `mutagen.rs`, `script_view.rs`, `prefs.rs`/`settings.rs`, `help.rs`, `build.rs` (Windows exe icon) |
| `crates/rwf-cli` | Command line program (`render`, `render-anim`, `batch`, `quilt`, `random`, `movie`, `dance`, `mesh`, `denoise`, `script`, `variations`) |
| `docs/manual` | User manual: `NN-*.md` chapters, `manual.css`, `images/` (screenshots PNG, renders JPG) |
| `tools/` | `build_manual.py` (manual PDF), `fetch_oidn.py` (denoiser libraries), `release/` (release archives), `jwf-harness/` (Java reference harness), generators for data tables |
| `testdata/` | Example flames from JWildfire |
| `oidn/` | OIDN libraries for development (gitignored, `python3 tools/fetch_oidn.py`) |

## Build and test

```sh
cargo build -j 8 -p rwf-app -p rwf-cli
RAYON_NUM_THREADS=8 cargo test -j 8 --workspace -- --test-threads=8
cargo clippy -j 8 -p rwf-app --no-deps -- -A clippy::approx_constant -A clippy::eq_op
```

- `cargo clippy` on `rwf-core` reports deny-level lints in ported variation code
  (`approx_constant`, `eq_op`); allow them as above, do not "fix" ported constants.
- **Never run plain `cargo fmt`**: most of the code is not rustfmt-formatted and it would rewrite
  ~65 files. Format only new files: `rustfmt --edition 2024 --config
  max_width=150,use_small_heuristics=Max <file>`. Don't pass `lib.rs`/`mod.rs` (rustfmt recurses
  into modules). `app.rs`, `icons.rs`, `rwf-cli/src/main.rs`, `ui/topbar.rs`,
  `variations/batch_d.rs` are not rustfmt-clean: edit them by hand. Check `git diff --stat`
  after formatting.
- `rwf-core` has `overflow-checks = false` in the dev profile: ported Java code relies on
  wrapping int arithmetic (noise hashes, cell indices).
- `cargo test -p rwf-gpu` validates every WGSL snippet with naga and runs GPU tests.

## Releases

```sh
python3 tools/release/release.py              # tag v<version>, push: GitHub Actions publishes
python3 tools/release/release.py --local      # build here, upload with gh (fallback)
python3 tools/release/package.py --jobs 8     # only build dist/RWildFire-<ver>-{linux,windows}-x86_64.zip
python3 tools/build_manual.py                 # only the manual PDF
```

- Steps for a release: bump the version in all `Cargo.toml` files, update the manual, commit,
  push, then `release.py`. It refuses with uncommitted changes, an unpushed branch, an existing
  tag or mismatching crate versions. `.github/workflows/release.yml` (tag `v*`) builds both
  archives with `package.py` on `ubuntu-24.04` and creates the release with
  `tools/release/notes.md` (`{version}` is replaced) plus generated notes.
- Linux: an AppImage (`tools/release/AppRun`, `rwildfire.desktop`, icon; OIDN in `usr/lib` via
  `RWF_OIDN_DIR`, manual in `usr/share/doc/rwildfire`) built by appimagetool 1.9.0 (pinned
  SHA-256, cached in `target/release-tools`). `AppImage cli …` runs `rwf-cli`. The binaries are
  built in Docker (`tools/release/linux.Dockerfile`, Debian 12) and need glibc 2.35+.
- Windows: cross-compiled with mingw-w64 (`x86_64-pc-windows-gnu`, `+crt-static`), only system
  DLLs; `crates/rwf-app/build.rs` embeds `assets/icon.ico`; release builds use the GUI subsystem.
  OIDN DLLs next to the exe.
- Both zips contain the manual PDF, `README.txt` (written by `package.py`), LICENSE, COPYING,
  NOTICE and the OIDN license.
- Pushing changes to `.github/workflows/` over HTTPS is rejected (the gh token lacks the
  `workflow` scope): push those via `git@github.com:Ravetracer/RWildFire.git`.
- Test the Windows build with Wine in a **separate prefix** (`WINEPREFIX=<scratch dir>`): the
  owner's default `WINEPREFIX` belongs to another application.
- Keep the manual in step with features: update the chapter and screenshots, rebuild the PDF.

## Testing the app visually

- Xvfb runs on `:99` (1600×1000). Start the app with a flame path as argument; use `xdotool`
  for clicks and `import -window root` for screenshots. Use a separate `XDG_CONFIG_HOME` so the
  owner's preferences stay untouched.
- There is no window manager on `:99`: keyboard focus needs
  `xdotool windowfocus $(xdotool search --name RWildFire)`. File dialogs (rfd, portal) do not
  open there; test file IO through the CLI, tests or command line arguments (the app opens a
  `.flame` or `.rwfdance` given as first argument).
- Stop the app with `pkill -x rwf-app`; never `pkill -f target/debug/rwf-app` (it kills the
  calling shell). For test binaries use a pattern like `"[d]eps/..."`.
- egui drag hit-tests must use `pointer.press_origin()`, not the position where the drag
  threshold was crossed.

## JWildfire reference

The Java original lives in `../JWildfire` (same parent folder). It can be compiled for reference
renders and parity checks:

```sh
javac -encoding ISO-8859-1 -cp "lib/*" -d <out> @sources
java -cp "<out>:lib/*:resources" org.jwildfire.cli.FlameRenderer -f x.flame -w W -h H -q Q
```

- The Java CLI forces oversampling 1 and an adaptive filter radius of 0.75: set the same in the
  Rust flame for like-for-like comparisons.
- RWildFire writes RGBA with alpha = intensity, Java an opaque PNG. Compare RGB with
  `convert x.png -alpha off`, not `-flatten` onto black.
- Variation parity: `tools/jwf-harness/VarHarness.java` + `crates/rwf-core/tests/jwf_harness.rs`
  (`JWF_SPEC` / `JWF_REF`, ignored test). Variations using `Math.random()`, unseeded Java Random,
  table-based `MathLib.sin` or Java2D differ by design.

## Design notes and pitfalls

- **GPU first**: new features go to the GPU renderer; the CPU renderer stays the exact reference
  and fallback. GPU and CPU agree in brightness (within 0.3 %) at the same quality.
- Variations: a `VarDef` holds a CPU function and a WGSL snippet reading the same prepared data
  array. Per-thread CPU state via `Ctx.orbits` / `Ctx.states`, keyed by `d.as_ptr()` (variations
  without parameters need `prepare` → `vec![0.0]` for a unique key). Resource variations pack
  images/meshes into the data array (`resources.rs`, side table `RESOURCES`).
- WGSL: large constant arrays indexed dynamically hang the NVIDIA driver; put tables into the
  parameter buffer (`prm`). NaN tests must use bit checks (`glsl_is_nan`); `clamp`/`min` eat NaN.
- Weighting-field shaders are specialized per field type (driver compile times).
- Tone mapping follows JWildfire's `LogDensityFilter`: filter matrices have
  `half = size / 2 - 1`, i.e. the kernel is centered one cell after the pixel. The denoiser
  reads the **unfiltered** image (`ToneMapper::unfiltered`, `GpuRenderer::read_denoise_input`),
  shifted by `tonemap::unfiltered_shift` to line up; filtered input made OIDN keep the grain.
- OIDN is loaded at runtime (`libloading`), searched in `RWF_OIDN_DIR`, next to the exe
  (`lib/` on Linux), `./oidn`, then the system.
- `GpuRenderer::render_frame` twice with the same flame keeps accumulating: call
  `reset_accumulation` in between.
- Documents: `Document.source = (path, index)` tracks which flame of a file the editor flame is;
  Save writes it back into that place and keeps the other flames.
- Not planned: variations running user-written Java/GLSL code (`custom_wf`, `c_var`, `dc_code`,
  `glsl_code`, …), JWildfire's image-processing module, the RNG choice, LeapMotion, applet.
- `taprats` is GPL and therefore behind the cargo feature `taprats` (default build stays LGPL).

## Settings and data on the owner's machine

- App preferences: `~/.config/rwildfire/preferences.txt`; user gradients and scripts below
  `~/.config/rwildfire/`.
- The product name is **RWildFire** (keep "JWildfire" only for references to the Java
  original). The repository folder was called `jwildfire-rs` until version 1.1.0.
