--- Spiral arms
--- Builds a new spiral flame: one transform winds the points inwards, a julian transform
--- seeds the arms.
--@param arms integer 3 1 8          Number of arms
--@param angle number 8 3 40         Turn per winding step (degrees)
--@param shrink number 0.93 0.8 0.99 Size per winding step
--@param blur number 0.08 0 0.5      Thickness of the arms

flame = rwf.new_flame()
flame.name = "Spiral arms " .. params.arms

local wind = flame:add_xform("linear3D", 1)
wind:scale(params.shrink)
wind:rotate(params.angle)
wind.weight = 6
wind.color_symmetry = 0.95

local seed = flame:add_xform("julian", 1)
seed:variation(1).power = params.arms
seed:scale(0.4)
seed:translate(1, 0)
seed.weight = 1
seed.color = 1
if params.blur > 0 then seed:add_variation("gaussian_blur", params.blur) end

flame.pixels_per_unit = 140 * flame.width / 800
flame:random_gradient()
