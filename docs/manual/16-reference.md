# Reference

## Keyboard shortcuts

| Keys | Action |
|------|--------|
| <kbd>F1</kbd> | Help |
| <kbd>Ctrl</kbd>+<kbd>N</kbd> / <kbd>Ctrl</kbd>+<kbd>O</kbd> | New flame / open a file |
| <kbd>Ctrl</kbd>+<kbd>S</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> | Save / save as |
| <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>S</kbd> | Quick save |
| <kbd>Ctrl</kbd>+<kbd>E</kbd> | Export an image or animation |
| <kbd>Ctrl</kbd>+<kbd>Q</kbd> | Quit |
| <kbd>Ctrl</kbd>+<kbd>Z</kbd> | Undo |
| <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd>, <kbd>Ctrl</kbd>+<kbd>Y</kbd> | Redo |
| <kbd>Ctrl</kbd>+<kbd>C</kbd> / <kbd>Ctrl</kbd>+<kbd>V</kbd> | Copy / paste the flame as text |
| <kbd>Space</kbd> | Random flame; play/pause while the timeline is open |
| <kbd>F</kbd> | Fit the flame into the view |
| <kbd>Delete</kbd> | Remove the selected transform |
| Arrow keys | Move the selected transform (<kbd>Ctrl</kbd>: ×0.1, <kbd>Shift</kbd>: ×10) |
| <kbd>Alt</kbd>+<kbd>←</kbd>/<kbd>→</kbd> | Turn the selected transform |
| <kbd>Alt</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd> | Scale the selected transform |
| <kbd>Ctrl</kbd>+<kbd>Enter</kbd> | Run the script (scripts window) |
| Browser: arrows, <kbd>Enter</kbd>, <kbd>Delete</kbd> | Select, open, delete a flame |
| Dance: <kbd>1</kbd> … <kbd>9</kbd>, <kbd>Space</kbd> | Switch to pool flame 1 … 9, play/pause |

## Mouse on the canvas

| Action | Result |
|--------|--------|
| Drag a triangle | Move the transform |
| Drag its X or Y corner | Turn and scale |
| <kbd>Shift</kbd>+drag a corner / <kbd>Ctrl</kbd>+drag a corner | Only turn / only scale |
| Drag the background | Move the view (any mouse button) |
| Mouse wheel | Zoom around the pointer |

## File formats

| Extension | Content | Read | Write |
|-----------|---------|------|-------|
| `.flame`, `.xml` | Flames (RWildFire, JWildfire, flam3/Apophysis XML); one or several per file | ✓ | ✓ |
| `.jwfmovie` | JWildfire movie project | ✓ | ✓ |
| `.rwfdance` | RWildFire dance project | ✓ | ✓ |
| `.rwfrender` | Interactive render state | ✓ | ✓ |
| `.lua` | Script | ✓ | ✓ |
| `.ugr`, `.map` | Gradients (Ultra Fractal, Fractint/JWildfire); flam3 palettes are also read | ✓ | ✓ |
| `.png`, `.jpg` | Images | ✓ | ✓ |
| `.hdr`, `.exr` | HDR images (32-bit float) | | ✓ |
| `.mp4`, `.mov`, `.webm` | Videos (with ffmpeg) | | ✓ |
| `.anb` | JWildfire animation | | ✓ |
| `.obj`, `.ply`, `.sc` | Meshes, point clouds, Sunflow scenes | `.obj` (mesh variations) | ✓ |
| `.mp3`, `.wav`, `.flac`, `.ogg` | Music for envelopes and dances | ✓ | |
| `.svg` | Shapes for `svg_wf` | ✓ | |

JWildfire's dance projects (`.jwfdance`) are Java-serialized files and cannot be read; rebuild
the dance in RWildFire with the same flames and music.

## Troubleshooting

**The canvas stays empty or black.**
The flame may have no visible transform (all weights or opacities 0), its points may fly off
(an affine that does not shrink), or the view may be off: press <kbd>F</kbd> to fit the flame.
Check the tone: brightness 0 or a very high white level make the image black.

**The preview says "Not supported yet (ignored)" or a variation name is orange.**
The flame uses a variation this version does not have (for example variations that run
user-written Java code). It is kept in the file but ignored when rendering.

**A variation looks different on the GPU.**
A few variations run on the CPU only, and color modulation is CPU-only. Switch the preview to
the CPU renderer (Render tab) or export with *CPU (exact)*.

**No GPU is found.**
RWildFire needs Vulkan (Linux, Windows) or DirectX 12 (Windows). Update the graphics driver.
Without a GPU everything works with the CPU renderer, but the batch, quilt and interactive
renderers need a GPU.

**The denoiser is not available.**
The Render tab and the preferences say why, e.g. that the Open Image Denoise library was not
found. It is searched next to the program (`lib/` on Linux), in the folder given by the
environment variable `RWF_OIDN_DIR`, and in the system's library paths.

**The denoiser leaves grain.**
Raise the quality a little: at very low qualities, sparse areas have too few points for the
denoiser to tell structure from noise. Check that Blend is 0 or small (Render tab). A high
**Keep detail** keeps the original grain along edges at low qualities; lower it.

**The denoiser softens thin lines.**
Raise **Keep detail** (Render tab) or the quality: lines with enough samples per pixel then
keep the original image. **Sharpen** helps when the render cannot be made longer.

**Video formats are disabled.**
MP4, MOV and WebM are written by ffmpeg. Install it and make sure it is in the `PATH`, then
restart RWildFire. PNG frames and ANB work without it.

**The text variation shows no text or the wrong font.**
Fonts installed after RWildFire started are found after a restart. The font name must match the
name the system knows; unknown names fall back to a sans-serif font.

**Rendering is slow.**
Lower the preview resolution (Render tab › Resolution ½) or the target quality; heavy
variations (formula plots, sub-flames, meshes) cost more per point. Exports on the GPU are much
faster than on the CPU.

**Where are my settings?**
In the preferences file (`~/.config/rwildfire/preferences.txt` on Linux,
`%APPDATA%\rwildfire\preferences.txt` on Windows). Deleting it resets all preferences.

## Glossary

| Term | Meaning |
|------|---------|
| Affine | The linear part of a transform: move, turn, scale, shear. Shown as a triangle. |
| Attractor | The shape the chaos game converges to: the visible flame. |
| Chaos game | The random process that draws a flame: a point jumps from transform to transform. |
| Color speed (color symmetry) | How strongly a transform pulls a point's color towards its own. |
| Density estimation (DE) | Adaptive smoothing of sparse areas. |
| Final transform | A transform applied to every point before drawing, without feeding back. |
| Gradient | The 256 colors that the points' color index refers to. |
| Layer | An independent set of transforms with its own gradient, drawn into the same image. |
| Post affine | An affine applied after the variations. |
| Quality (sample density) | Points per pixel; more is smoother and slower. |
| Tone mapping | Turning hit counts into brightness: brightness, gamma, vibrancy, white level. |
| Transform (xform) | One rule of the chaos game: affine, variations, post affine, color, weight. |
| Variation | A non-linear function that bends the points; the source of a flame's character. |
| Weight | How often a transform is chosen. |
| Weighting field | A noise pattern or image that modulates a transform's effect by position. |
| Xaos | Weights per pair of transforms: which transform may follow which. |

## Credits and license

RWildFire is a port of JWildfire (Copyright © 1995–2026 Andreas Maschke, LGPL 2.1 or later) and
is licensed under the GNU Lesser General Public License version 3 or later. It contains the work
of many authors of flame variations, random generators and scripts from the JWildfire
community, the gradient collections distributed with JWildfire, Intel® Open Image Denoise
(Apache 2.0) and Lua (MIT). The optional `taprats` variation is a port of Taprats by Craig S.
Kaplan (GPL). See the files `LICENSE`, `COPYING` and `NOTICE` that come with RWildFire.
