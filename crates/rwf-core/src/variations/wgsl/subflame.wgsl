// Sub-flame variations (variations/subflame.rs). The GPU renderer runs every sub-flame as its
// own chaos game in a separate kernel that refills `subpts` before each iteration pass; these
// variations pick random points from it. Three vec4 per point: (x, y, z, color),
// (r, g, b, rgbc), (hide, valid, 0, 0). The renderer replaces the sub-flame id of the prepared
// data by the slot of the sub-flame (negative = none).
@group(0) @binding(9) var<storage, read> subpts: array<vec4<f32>>;
const SUBFLAME_POINTS: u32 = 262144u;

struct SubQ { p: Pt, valid: bool }

// Next point of the sub-flame whose parameters start at prm[base + o] (`next_point`): scaled,
// rotated and moved; `colorscale_z` adds the color to z.
fn subflame_point(base: u32, o: u32) -> SubQ {
  var q = Pt(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
  let slot = prm[base + o + 12u];
  if (slot < 0.0) { return SubQ(q, false); }
  let i = (u32(slot) * SUBFLAME_POINTS + (rnd_u32() % SUBFLAME_POINTS)) * 3u;
  let a = subpts[i];
  let c = subpts[i + 1u];
  let h = subpts[i + 2u];
  if (h.y == 0.0) { return SubQ(q, false); }
  let sc = prm[base + o];
  let ang = prm[base + o + 1u] * PI / 180.0;
  let sa = sin(ang);
  let ca = cos(ang);
  let x = sc * a.x;
  let y = sc * a.y;
  q.x = x * ca - y * sa + prm[base + o + 2u];
  q.y = x * sa + y * ca + prm[base + o + 3u];
  q.z = sc * a.z + prm[base + o + 4u] + prm[base + o + 5u] * a.w;
  q.color = a.w;
  q.r = c.x;
  q.g = c.y;
  q.b = c.z;
  q.rgbc = c.w;
  q.hide = h.x;
  return SubQ(q, true);
}

// `setColor` of the sub-flame variations.
fn subflame_set_color(q: Pt, mode: i32, v_in: Pt) -> Pt {
  var v = v_in;
  if (q.hide != 0.0) { v.hide = 1.0; return v; }
  v.hide = 0.0;
  if (mode == -1) { return v; }
  if (q.rgbc != 0.0 || mode == -2) {
    v.rgbc = 1.0;
    v.r = q.r;
    v.g = q.g;
    v.b = q.b;
    v.color = q.color;
  }
  switch mode {
    case 0: { v.color = q.color; }
    case 1: { v.color = q.r / 255.0; }
    case 2: { v.color = q.g / 255.0; }
    case 3: { v.color = q.b / 255.0; }
    case 4: { v.color = (0.2990 * q.r + 0.5880 * q.g + 0.1130 * q.b) / 255.0; }
    default: {}
  }
  return v;
}
