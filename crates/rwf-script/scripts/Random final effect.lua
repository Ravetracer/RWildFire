--- Random final effect
--- Replaces the final transform of the current flame with a random effect.
--@param effect choice random|spherical|bubble|curl|ngon|juliascope|circlize  Effect variation
--@param amount number 1 0.1 2   Amount of the effect

local effects = {"spherical", "bubble", "curl", "ngon", "juliascope", "circlize"}
local name = params.effect
if name == "random" then name = effects[math.random(#effects)] end

while flame.final_count > 0 do flame:remove_final_xform(1) end
local fx = flame:add_final_xform(name, params.amount)
-- Let the effect start from a slightly turned, scaled copy of the image.
fx:rotate(rwf.random(-30, 30))
fx:scale(rwf.random(0.8, 1.2))
-- Randomize the effect's parameters inside their suggested ranges.
for _, p in ipairs(rwf.variation_params(name)) do
  local v = rwf.random(p.min, p.max)
  if p.integer then v = math.floor(v + 0.5) end
  fx:variation(1):set(p.name, v)
end
print("final effect: " .. name)
