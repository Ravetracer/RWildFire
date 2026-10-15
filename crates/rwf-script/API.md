# RWildFire Lua scripting

Scripts are Lua 5.4. They change the current flame (global `flame`) and can create new flames.
Indices start at 1 (Lua convention); property paths for `get`/`set`/`set_key` are the timeline
paths and start at 0 (`layer0.xf1.weight`).

## Script header

The comment block at the top names the script and declares its parameters. The app shows a
form for them, `rwf-cli script` takes them as `--param name=value`.

    --- Title of the script
    --- Description (any number of lines).
    --@param count  integer 3 1 10       Number of transforms
    --@param scale  number  0.5 0.1 2    Scale of the new transforms
    --@param clear  bool    false        Remove the old transforms first
    --@param style  choice  rings|dots   Look (the first option is the default)

Format: `--@param <name> <type> <default> [min max] [help text]`; for `choice` the options
separated by `|` take the place of the default. The values are in `params.<name>`.

## Globals

- `flame` – the current flame. Assign another flame (`flame = rwf.random_flame()`) to replace it.
- `params` – the parameter values.
- `print(...)` – writes a line to the script output.
- The Lua standard libraries (`math`, `string`, `table`, `os`, `io`, …).

## rwf module

- `rwf.new_flame()` – an empty flame with one layer.
- `rwf.random_flame([generator], [{symmetry=, weighting=, gradient=}])` – a random flame, as the
  Randomize button makes it (default generator "All"). Names: `rwf.generators()`,
  `rwf.symmetries()`, `rwf.weightings()`, `rwf.gradient_generators()`.
- `rwf.mutate(f, [type], [strength])` – applies a MutaGen mutation in place (default "All",
  strength 1). Names: `rwf.mutations()`.
- `rwf.seed(n)` – makes `rwf.random`, `random_flame`, `mutate` and `random_gradient` repeatable.
- `rwf.random()` 0..1, `rwf.random(max)`, `rwf.random(min, max)`.
- `rwf.variations([filter])` – sorted variation names (containing `filter`).
- `rwf.variation_params(name)` – list of `{name, default, min, max, integer}`.
- `rwf.load_flames(path)` – list of the flames in a file.
- `rwf.save_flame(f, path)`, `rwf.save_flames(list, path)` – relative paths are relative to the
  script's folder.
- `rwf.output(f)` – hands a copy of a flame back as a result: the app shows them in
  Explore › File, the CLI saves them.
- `rwf.version` – version string.

## Flame

Fields (read/write): `name`, `width`, `height`, `sample_density`, `spatial_filter_radius`,
`spatial_oversampling`, `filter_sharpness`, `filter_low_density`, `post_blur_radius`,
`post_blur_fade`, `post_blur_falloff`, `de_radius`, `de_curve`, `antialias_amount`,
`antialias_radius`, `frame_count`, `fps`, `bg_transparency` (bool), `solid` (bool) and every
animatable setting: `centre_x`, `centre_y`, `pixels_per_unit`, `cam_zoom`, `cam_roll`,
`cam_pitch`, `cam_yaw`, `cam_bank`, `cam_perspective`, `cam_pos_x/y/z`, `cam_z`, `cam_dof`,
`focus_x/y/z`, `brightness`, `contrast`, `gamma`, `gamma_threshold`, `vibrancy`, `white_level`,
`saturation`, `low_density_brightness`, `balance_red/green/blue`, `fg_opacity`,
`post_symmetry_order`, …
Read-only: `layer_count`, `xform_count`, `final_count` (first layer).

Methods:
- `f:layer(i)`, `f:add_layer()`, `f:remove_layer(i)`
- First layer shortcuts: `f:xform(i)`, `f:final_xform(i)`, `f:add_xform([variation, amount])`,
  `f:add_final_xform([variation, amount])`, `f:remove_xform(i)`, `f:remove_final_xform(i)`,
  `f:random_gradient([generator])`
- `f:copy()` – an independent copy.
- `f:get(path)`, `f:set(path, value)`, `f:paths()` – any animatable property by its path.
- `f:set_key(path, frame, value)`, `f:clear_keys([path])` – animation keys; frames run from 0
  to `frame_count - 1`.
- `f:set_background(r, g, b)` – plain background color (0..255).

## Layer

Fields: `name`, `visible`, `weight`, `density`, `gradient_shift`; read-only `xform_count`,
`final_count`, `gradient_size`, `index`.

Methods: `xform(i)`, `final_xform(i)`, `add_xform([variation, amount])`,
`add_final_xform([variation, amount])`, `remove_xform(i)`, `remove_final_xform(i)`,
`color(i)` → r, g, b, `set_color(i, r, g, b)`, `set_gradient(keys)` with keys
`{{pos, r, g, b}, …}` (pos 0..1) or `{{r, g, b}, …}` (spread evenly),
`random_gradient([generator])`.

## Transform

Fields: `name`, `weight`, `color`, `color_symmetry`, `opacity`, `material`, `material_speed`,
`mod_gamma`, `mod_gamma_speed`, `mod_contrast`, `mod_contrast_speed`, `mod_saturation`,
`mod_saturation_speed`, `mod_hue`, `mod_hue_speed`, `draw_mode` ("normal", "opaque", "hidden"),
the affine coefficients `c00 c01 c10 c11 c20 c21` (JWildfire's coeff00…; `c20`/`c21` = offset),
`post_c00`…`post_c21`, `yz_c00`…, `zx_c00`…, `yz_post_c00`…, `zx_post_c00`…, weighting field
settings `wfield.*` via `f:set`; read-only `var_count`, `index`.

Methods:
- `variation(i)`, `find_variation(name)` (nil if missing), `add_variation(name, [amount])`,
  `remove_variation(i)`, `clear_variations()`
- `rotate(degrees, [post])`, `scale(factor, [post])`, `translate(dx, dy, [post])`,
  `reset([post])` – change the affine (`post = true`: the post affine).
- `chaos(to)`, `set_chaos(to, value)` – xaos weight to transform `to`.
- `duplicate()` – appends a copy and returns it.

Removing a transform shifts the following ones; handles to them then point at their
neighbour, a handle past the end reports an error.

## Variation

Fields: `name` (read-only), `amount`, `priority` (-1 pre, 0 normal, 1 post), and every
parameter by name (`v.power = 3`). Methods: `get(name)`, `set(name, value)`, `params()` → table.

## Example

    --- Spiral arms
    --@param arms integer 3 2 8  Number of arms
    flame = rwf.new_flame()
    for i = 1, params.arms do
      local xf = flame:add_xform("julian", 0.8)
      xf.weight = 1 / params.arms
      xf.color = (i - 1) / params.arms
      xf:rotate(360 * i / params.arms)
      xf:variation(1).power = params.arms
    end
    flame:random_gradient()
