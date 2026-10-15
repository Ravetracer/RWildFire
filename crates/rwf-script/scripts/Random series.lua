--- Random series
--- Creates a series of random flames (shown in Explore › File, saved by the CLI).
--@param count integer 6 1 50            Number of flames
--@param generator choice All|Classic|Bubbles|Brokat|Galaxies|Julians|Spirals|Synth  Random generator
--@param mutate bool false               Add a random mutation to each flame
--@param seed integer 0 0 100000         Seed (0 = different every run)

if params.seed > 0 then rwf.seed(params.seed) end
for i = 1, params.count do
  local f = rwf.random_flame(params.generator)
  if params.mutate then rwf.mutate(f) end
  f.name = ("%s %d"):format(params.generator, i)
  rwf.output(f)
  print(f)
end
