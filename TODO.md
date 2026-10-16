# TODO

## Post-render image effects

Adjustments applied to the final image after tone mapping and denoising, to improve image
quality.

- Run on the GPU (CPU path as reference/fallback, like the renderers).
- Light: exposure, contrast, highlights, shadows, whites, blacks, curves.
- Detail and effects: texture, clarity, dehaze, grain, vignette.
- Sharpening after the OIDN denoiser: some images get slightly blurry after denoising. Keep
  sharp lines sharp (e.g. edge-aware sharpening or detail recovery from the undenoised image),
  adjustable by the user.
