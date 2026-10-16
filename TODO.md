# TODO

## Post-render image effects

Adjustments applied to the final image after tone mapping and denoising, to improve image
quality.

Pipeline: render → tone mapping (GPU) → OIDN (read back) → upload → effects pass (GPU). Effects
run on the GPU in float precision with a fixed order; the CPU path stays the reference and is
used by the CLI. Settings are stored in the flame as RWildFire-only attributes (check that
JWildfire ignores unknown attributes).

| Effect | Effort | Approach |
|---|---|---|
| Exposure, contrast, whites, blacks | easy | Per-pixel math in one shader pass. Partly overlaps with brightness, gamma and white level; the UI must make the difference clear. |
| Curves | easy | Lookup table in the parameter buffer; reuse `curve_editor`. |
| Highlights, shadows | medium | Per pixel gives flat results; use a blurred luminance image as mask to avoid halos. |
| Vignette, grain | easy | Per pixel; grain from a seeded noise hash so renders are reproducible. |
| Texture, clarity | medium | Local contrast: original minus blurred copy, added back (small radius for texture, large for clarity). Needs a separable GPU blur pass. |
| Dehaze | hard | Dark channel prior assumes photographic haze; on flames with a black background it mostly lifts contrast. Do it last, or approximate with clarity plus a black point shift. |

## Sharper lines after the OIDN denoiser

OIDN is trained on path-traced images; thin, faint flame lines look like noise and get
smoothed. Measured with `crates/rwf-gpu/tests/denoise_sharpness.rs` (ignored test: low quality
renders, denoised, against a high quality render).

Done:

- **Density-aware blend** ("Keep detail", default 0.6): per pixel sample counts from the
  renderer (GPU and CPU); pixels with enough samples keep the original image, but only near
  edges of the denoised image, so smooth areas keep no grain. Large gains on fine-detail
  flames (TINA0007: edge error 37 → 15 at 0.6, → 3 at 1.0), no losses elsewhere.
- **Albedo input reviewed**: without albedo, or with the raw (not prefiltered) albedo, gives
  no consistent improvement (raw albedo: lower error overall at very low quality, but worse
  edges). Kept as it is.
- **Sharpening** ("Sharpen", default 0): contrast adaptive sharpening of the denoised image.
  Raises the edge contrast but also the error against the reference, so it stays optional.

Open (only if the result is still not good enough):

1. **HDR float input**: pass the linear float image (`hdr=true`) instead of 8-bit sRGB, which
   keeps fine and bright structures.
2. **Pseudo-normals** from density gradients for non-solid flames (as the normals of solid
   flames), giving OIDN edges to keep. Experimental.
3. **Denoise at a higher resolution and downscale** (quality option): clearly sharper lines,
   more render time.
