# Packs JWildfire's wangtiles tile GIFs (resources/org/jwildfire/icons/<type>/<set>/<n>.gif)
# into one zlib stream: u16 set count; per set: u8 name length, name ("edge/angular"),
# u16 tile count; per tile: u16 index, 32*32 RGB bytes.
import glob, os, struct, sys, zlib
from PIL import Image
src, out = sys.argv[1], sys.argv[2]
sets = {}
for f in sorted(glob.glob(os.path.join(src, '*', '*', '*.gif'))):
    stem = os.path.splitext(os.path.basename(f))[0]
    if not stem.isdigit():
        continue
    t, s = f.split(os.sep)[-3], f.split(os.sep)[-2]
    sets.setdefault(t + '/' + s, []).append((int(stem), f))
buf = bytearray(struct.pack('<H', len(sets)))
for name, tiles in sorted(sets.items()):
    n = name.encode()
    buf += struct.pack('<B', len(n)) + n + struct.pack('<H', len(tiles))
    for idx, f in sorted(tiles):
        im = Image.open(f).convert('RGB')
        assert im.size == (32, 32), f
        buf += struct.pack('<H', idx) + im.tobytes()
open(out, 'wb').write(zlib.compress(bytes(buf), 9))
print(len(sets), 'sets', sum(len(t) for t in sets.values()), 'tiles', len(buf), '->', os.path.getsize(out), 'bytes')
