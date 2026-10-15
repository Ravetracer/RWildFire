--- Spin animation
--- Animates the current flame: the camera turns once and every transform turns its own way.
--@param frames integer 100 10 1000  Length of the animation in frames
--@param turns number 1 0.25 4       Camera turns over the animation
--@param wobble number 10 0 90       Turn of each transform (degrees, back and forth)

flame.frame_count = params.frames
flame:clear_keys()
local roll = flame.cam_roll
-- Frames run from 0 to frame_count - 1.
local last = params.frames - 1
flame:set_key("cam_roll", 0, roll)
flame:set_key("cam_roll", last, roll + 360 * params.turns)

-- A transform's turn is keyed on its affine coefficients (paths start at 0).
for i = 1, flame.xform_count do
  local xf = flame:xform(i)
  local path = "layer0.xf" .. (i - 1) .. ".coefs."
  local start = {xf.c00, xf.c01, xf.c10, xf.c11}
  local dir = (i % 2 == 0) and 1 or -1
  for _, frame in ipairs({0, last // 2, last}) do
    local copy = flame:copy()
    local turned = copy:xform(i)
    turned:rotate(dir * params.wobble * math.sin(math.pi * frame / last))
    for _, c in ipairs({"c00", "c01", "c10", "c11"}) do
      flame:set_key(path .. c, frame, turned[c])
    end
  end
  xf.c00, xf.c01, xf.c10, xf.c11 = table.unpack(start)
end
print(("%d frames, %d transforms keyed"):format(params.frames, flame.xform_count))
