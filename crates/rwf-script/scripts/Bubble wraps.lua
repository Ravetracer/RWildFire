--- Bubble wraps
--- Adds a bwraps7, two julian2 and a bubble transform with crossed xaos to the current flame.
--- Port of the JWildfire script "Bwraps and bubbles" by Geoffrey Barnes, based on a script by
--- MickHogan and the Apophysis script "BD's Crackled JuliaN2-Rings-2" by Bart Doetsch.
--@param clear bool true  Remove the existing transforms first

local function rnd(lo, hi) return lo + math.random() * (hi - lo) end

if params.clear then
  while flame.xform_count > 0 do flame:remove_xform(1) end
end
local base = flame.xform_count
local s, s2, s3 = rnd(0.1, 0.4), rnd(0.1, 0.4), rnd(0.01, 0.5)

local t1 = flame:add_xform("bwraps7", rnd(1, 2.5))
t1.weight = 0.65
t1.color = math.random()
t1.color_symmetry = -1

local function julian(scale)
  local xf = flame:add_xform("julian2", rnd(1.8, 4.8))
  xf:scale(scale)
  xf.c20 = rnd(0.1, 0.9)
  local v = xf:variation(1)
  v.power = math.floor(rnd(4, 8.5))
  v.dist = -rnd(1, 1.8)
  xf.weight = 2
  return xf
end

local t2 = julian(0.4 + s)
local t3 = flame:add_xform("bubble", 0.4)
t3:scale(0.7 + s3)
t3.weight = 0.5
t3.color = 0.93
local t4 = julian(0.4 + s2)

-- xaos (JWildfire's modified weights), indices relative to the new transforms
local function xaos(xf, values)
  for i, v in ipairs(values) do xf:set_chaos(base + i, v) end
end
xaos(t1, {1, 0, rnd(10, 20), 0})
xaos(t2, {5, 1, 1, 1})
xaos(t4, {5, 1, 1, 1})

while flame.final_count > 0 do flame:remove_final_xform(1) end
flame.centre_x, flame.centre_y = 0, 0
flame.cam_roll, flame.cam_pitch, flame.cam_yaw, flame.cam_perspective = 0, 0, 0, 0
flame.pixels_per_unit = 50 * flame.width / 800
