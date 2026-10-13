// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// This program is free software: you can redistribute it and/or modify it under the terms of
// the GNU Lesser General Public License as published by the Free Software Foundation, either
// version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: LGPL-3.0-or-later

import org.jwildfire.base.Prefs;
import org.jwildfire.create.tina.base.Flame;
import org.jwildfire.create.tina.base.Layer;
import org.jwildfire.create.tina.base.XForm;
import org.jwildfire.create.tina.base.XYZPoint;
import org.jwildfire.create.tina.random.AbstractRandomGenerator;
import org.jwildfire.create.tina.render.FlameRenderer;
import org.jwildfire.create.tina.variation.FlameTransformationContext;
import org.jwildfire.create.tina.variation.VariationFunc;
import org.jwildfire.create.tina.variation.VariationFuncList;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.util.Locale;

/**
 * Reference outputs of JWildfire variations for the RWildFire port.
 *
 * Reads lines "name [param=value ...]" from stdin and prints, for six fixed input points, the
 * affine and the accumulated point after one transform call. The random generator reproduces
 * RWildFire's xoshiro256+ (`rwf_core::rng::Rng`, including `random_int`), reseeded per point, so
 * random variations can be compared value by value. See `crates/rwf-core/tests/jwf_harness.rs`.
 */
public class VarHarness {
  static final double AMOUNT = 0.8;
  static final double[][] POINTS = {
      {0.3, 0.2, 0.1, 0.25}, {-0.7, 0.4, -0.2, 0.5}, {1.3, -0.9, 0.6, 0.75},
      {-0.05, -0.15, 0.03, 0.1}, {2.1, 1.7, -1.1, 0.9}, {-1.6, -2.3, 0.4, 0.35}};

  /** xoshiro256+ seeded with splitmix64, as `rwf_core::rng::Rng`. */
  static class Xoshiro extends AbstractRandomGenerator {
    private long s0, s1, s2, s3;

    Xoshiro(long seed) {
      randomize(seed);
    }

    @Override
    public void randomize(long seed) {
      long[] sm = {seed};
      s0 = splitmix(sm);
      s1 = splitmix(sm);
      s2 = splitmix(sm);
      s3 = splitmix(sm);
    }

    private static long splitmix(long[] sm) {
      sm[0] += 0x9E3779B97F4A7C15L;
      long z = sm[0];
      z = (z ^ (z >>> 30)) * 0xBF58476D1CE4E5B9L;
      z = (z ^ (z >>> 27)) * 0x94D049BB133111EBL;
      return z ^ (z >>> 31);
    }

    private long next() {
      long result = s0 + s3;
      long t = s1 << 17;
      s2 ^= s0;
      s3 ^= s1;
      s1 ^= s2;
      s0 ^= s3;
      s2 ^= t;
      s3 = Long.rotateLeft(s3, 45);
      return result;
    }

    @Override
    public double random() {
      return (next() >>> 11) * 0x1.0p-53;
    }

    @Override
    public int random(int max) {
      return (int) (((next() >>> 32) * (long) max) >>> 32);
    }

    @Override
    public void cleanup() {
    }
  }

  static XYZPoint point(double[] p) {
    XYZPoint r = new XYZPoint();
    r.x = p[0];
    r.y = p[1];
    r.z = p[2];
    r.color = p[3];
    return r;
  }

  static String fmt(XYZPoint p) {
    return String.format(Locale.ROOT, "%.17g %.17g %.17g %.17g %d %.17g %.17g %.17g %d", p.x, p.y, p.z, p.color,
        p.rgbColor ? 1 : 0, p.redColor, p.greenColor, p.blueColor, p.doHide ? 1 : 0);
  }

  public static void main(String[] args) throws Exception {
    BufferedReader in = new BufferedReader(new InputStreamReader(System.in));
    String line;
    while ((line = in.readLine()) != null) {
      line = line.trim();
      if (line.isEmpty() || line.startsWith("#")) {
        continue;
      }
      String[] tok = line.split("\\s+");
      String name = tok[0];
      try {
        VariationFunc func = VariationFuncList.getVariationFuncInstance(name, false);
        for (int i = 1; i < tok.length; i++) {
          int eq = tok[i].indexOf('=');
          func.setParameter(tok[i].substring(0, eq), Double.parseDouble(tok[i].substring(eq + 1)));
        }
        Flame flame = new Flame();
        Layer layer = flame.getFirstLayer();
        XForm xf = new XForm();
        layer.getXForms().add(xf);
        xf.addVariation(AMOUNT, func);
        FlameRenderer renderer = new FlameRenderer(flame, Prefs.getPrefs(), false, false);
        Xoshiro gen = new Xoshiro(1);
        FlameTransformationContext ctx = new FlameTransformationContext(renderer, gen, 1, 1);
        ctx.setPreserveZCoordinate(false);
        func.initOnce(ctx, layer, xf, AMOUNT);
        func.init(ctx, layer, xf, AMOUNT);
        for (int k = 0; k < POINTS.length; k++) {
          gen.randomize(1000 + k);
          XYZPoint a = point(POINTS[k]);
          XYZPoint v = point(new double[] {0.1, -0.05, 0.02, POINTS[k][3]});
          func.transform(ctx, xf, a, v, AMOUNT);
          System.out.println(name + " " + k + " " + fmt(a) + " " + fmt(v));
        }
      } catch (Throwable ex) {
        System.out.println(name + " ERROR " + ex);
      }
    }
  }
}
