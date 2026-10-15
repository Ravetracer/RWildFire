--- Gradient from hues
--- Gives the current flame a gradient that runs through a few hues.
--@param hue number 0.6 0 1          First hue (0 red, 0.33 green, 0.66 blue)
--@param spread number 0.25 0 1      Hue distance between the colors
--@param colors integer 4 2 12       Number of colors
--@param dark bool true              Start with a dark color

local function hsl(h, s, l)
  local function f(n)
    local k = (n + h * 12) % 12
    local a = s * math.min(l, 1 - l)
    return 255 * (l - a * math.max(-1, math.min(k - 3, 9 - k, 1)))
  end
  return f(0), f(8), f(4)
end

local keys = {}
if params.dark then keys[1] = {0, 0, 0} end
for i = 0, params.colors - 1 do
  local r, g, b = hsl((params.hue + i * params.spread) % 1, 0.85, 0.35 + 0.3 * i / params.colors)
  keys[#keys + 1] = {r, g, b}
end
for l = 1, flame.layer_count do
  flame:layer(l):set_gradient(keys)
end
