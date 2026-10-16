# TODO

## Post-render image effects

Done: Effects tab (exposure, contrast, highlights, shadows, whites, blacks, texture, clarity,
dehaze, tone curves, vignette, grain), stored as `effects_*` flame attributes.
`rwf_core::render::effects` is the reference (saved images, after the denoiser, via
`render::post::process`); `rwf-gpu` `effects.wgsl` computes the same for the live preview
(parity test: at most 1/255 apart). Dehaze is a simplified dark channel prior (blurred dark
channel, no airlight estimation). Not applied to HDR files and quilts.

Open:

- Check that JWildfire ignores the `effects_*` and `post_denoiser_*` attributes (the Java
  source was not available when this was written; other RWildFire-only attributes such as
  `gradient_curves` already rely on it).
- Effects in the quilt renderer: per-pixel effects could run per tile, blurs need the whole
  image or overlapping tiles.

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

Also done (options in the Render tab, off by default):

- **Supersample ×2**: final images are rendered at twice the size, denoised and scaled down
  (`post::supersample`, `post::process`). Against a 2× reference the edge error falls to a
  third to a half (TINA0007 15.6 → 5.6, TINA0012 3.3 → 1.6 at quality 150); four times the
  render time. Not for the canvas, the interactive renderer, stereo images and quilts.
- **Float input**: the unfiltered image before rounding (GPU `tonemap_float`, CPU
  `gamma_pixel_float`). No measurable difference on the test flames; linear values in OIDN's
  HDR mode were worse and are not used.
- **Density normals**: normals from the log sample density of flat flames
  (`denoise::pseudo_normals`). Mixed results (better on TINA0020 at low quality, softer on
  TINA0001), so experimental. The CPU renderer now gives OIDN an albedo too.
