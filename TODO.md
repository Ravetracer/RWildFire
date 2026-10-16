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
smoothed. Options:

1. **Density-aware blend** instead of the single global blend: the renderer knows the sample
   density per pixel. Converged areas (high density, the lines) keep the original, noisy areas
   (low density) take the denoised image.
2. **Review the albedo input**: the albedo comes from the same noisy samples and is prefiltered
   by OIDN (`cleanAux`). The prefilter blurs it, and OIDN then treats the blurred albedo as
   clean edge information. Test without albedo, or with an albedo from more samples.
3. **HDR float input**: pass the linear float image (`hdr=true`) instead of 8-bit sRGB, which
   keeps fine and bright structures.
4. **Sharpening after denoising**: AMD FidelityFX Contrast Adaptive Sharpening (one cheap GPU
   pass, few halos), or add back the high-frequency detail of the original image only where
   the noise is low.
5. **Pseudo-normals** from density gradients for non-solid flames (as the normals of solid
   flames), giving OIDN edges to keep. Experimental.
6. **Denoise at a higher resolution and downscale** (quality option): clearly sharper lines,
   more render time.

Suggested order: start with 1, 2 and 4 (small effort), then 3 if needed; 5 and 6 only if the
result is still not good enough.
