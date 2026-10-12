//! WGSL side of the variations: the shared prelude (types, RNG, math helper libraries) and the
//! translation of a variation snippet into a WGSL function.
//!
//! A snippet may start with module-scope helper functions, separated from the body by a line
//! containing only `@body`. Helper names must be prefixed with the variation name
//! (e.g. `fn crackle_cell(...)`) because all variations share one shader module.

use std::fmt::Write as _;

use super::VarDef;

/// Types, RNG and basic helpers shared by all generated shaders.
pub const PRELUDE_CORE: &str = r#"
const PI: f32 = 3.14159265358979;
const TWO_PI: f32 = 6.28318530717959;
const HALF_PI: f32 = 1.5707963267949;
const EPS: f32 = 1.0e-20;
const EPSILON: f32 = 1.0e-8;

// rgbc != 0 means the variation set a true color in r/g/b (0..255 scale, JWildfire `rgbColor`).
// hide != 0 means the point is not plotted (JWildfire `doHide`); it propagates like in JWildfire.
struct Pt { x: f32, y: f32, z: f32, color: f32, r: f32, g: f32, b: f32, rgbc: f32, hide: f32 }
struct PtPair { a: Pt, v: Pt }

var<private> rng_state: u32;
// Start of the current layer's 256-entry color map in `cmap` (set by the iteration kernel).
var<private> cmap_base: u32;

@group(0) @binding(3) var<storage, read> cmap: array<vec4<f32>>;

fn rnd_u32() -> u32 {
    rng_state = rng_state * 747796405u + 2891336453u;
    let w = ((rng_state >> ((rng_state >> 28u) + 4u)) ^ rng_state) * 277803737u;
    return (w >> 22u) ^ w;
}
fn rnd() -> f32 { return f32(rnd_u32() >> 8u) * (1.0 / 16777216.0); }

fn sqr(x: f32) -> f32 { return x * x; }
fn sumsq(p: Pt) -> f32 { return p.x * p.x + p.y * p.y; }
fn rad(p: Pt) -> f32 { return sqrt(p.x * p.x + p.y * p.y) + EPS; }
fn atan_xy(p: Pt) -> f32 { return atan2(p.x, p.y); }
fn atan_yx(p: Pt) -> f32 { return atan2(p.y, p.x); }
fn log10(x: f32) -> f32 { return log(x) * 0.434294481903252; }
fn fmod(a: f32, b: f32) -> f32 { return a - b * trunc(a / b); }
fn sgn(x: f32) -> f32 { return sign(x); }

// Gradient color i (0..255) of the current layer, in 0..255 units (like RGBPalette.getColor).
fn palette_rgb(i: i32) -> vec3<f32> {
    let c = cmap[cmap_base + u32(clamp(i, 0, 255))];
    return c.xyz * (256.0 / 200.0);
}
"#;

/// Complete prelude: core + helper libraries (GLSL emulation, complex numbers, noise).
pub fn prelude() -> String {
    let mut s = String::from(PRELUDE_CORE);
    for lib in super::wgsl_libs::LIBS {
        s.push_str(lib);
        s.push('\n');
    }
    s
}

/// Replaces `D(k)` with `prm[base + k]` in a snippet.
pub fn expand_snippet(src: &str) -> String {
    let mut out = String::with_capacity(src.len() + 64);
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let is_ident_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if !is_ident_before && b[i] == b'D' && i + 1 < b.len() && b[i + 1] == b'(' {
            if let Some(end) = src[i + 2..].find(')') {
                let inner = src[i + 2..i + 2 + end].trim();
                if !inner.is_empty() && inner.bytes().all(|c| c.is_ascii_digit()) {
                    let _ = write!(out, "prm[base + {inner}u]");
                    i += 2 + end + 1;
                    continue;
                }
            }
        }
        // Copy the full UTF-8 character.
        let ch = src[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Splits a snippet into (module-scope helpers, body).
pub fn split_snippet(src: &str) -> (&str, &str) {
    for (pos, line) in line_offsets(src) {
        if line.trim() == "@body" {
            return (&src[..pos], &src[pos + line.len()..]);
        }
    }
    ("", src)
}

fn line_offsets(s: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut pos = 0;
    s.split_inclusive('\n').map(move |l| {
        let p = pos;
        pos += l.len();
        (p, l)
    })
}

/// WGSL function name for a variation.
pub fn var_fn_name(def: &VarDef) -> String {
    let clean: String = def.name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("var_{clean}")
}

/// Emits helpers + `fn var_x(a_in, v_in, base, AMOUNT) -> PtPair` for a variation.
pub fn variation_function(def: &VarDef, preserve_z: bool) -> String {
    let (helpers, body) = split_snippet(def.wgsl);
    let pz = if preserve_z { "true" } else { "false" };
    let body = expand_snippet(body).replace("PRESERVE_Z", pz);
    let helpers = helpers.replace("PRESERVE_Z", pz);
    format!(
        "{helpers}\nfn {}(a_in: Pt, v_in: Pt, base: u32, AMOUNT: f32) -> PtPair {{\n  var a = a_in;\n  var v = v_in;\n  {{\n{}\n  }}\n  return PtPair(a, v);\n}}\n",
        var_fn_name(def),
        body
    )
}

/// A small compute shader calling the given variations, used to validate snippets.
pub fn test_shader(defs: &[&VarDef]) -> String {
    let mut w = prelude();
    w.push_str("@group(0) @binding(0) var<storage, read_write> prm: array<f32>;\n");
    for d in defs {
        w.push_str(&variation_function(d, true));
    }
    w.push_str("@compute @workgroup_size(1)\nfn main() {\n  rng_state = 1u;\n  cmap_base = 0u;\n");
    w.push_str("  let a = Pt(prm[0], prm[1], prm[2], 0.5, 0.0, 0.0, 0.0, 0.0, 0.0);\n  var acc = 0.0;\n");
    for d in defs {
        let _ = writeln!(
            w,
            "  {{ let r = {}(a, Pt(0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0), 4u, prm[3]); acc += r.v.x + r.v.y + r.v.z + r.a.x + r.v.r + r.v.hide; }}",
            var_fn_name(d)
        );
    }
    w.push_str("  prm[0] = acc;\n}\n");
    w
}

#[cfg(test)]
pub(crate) fn validate(src: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(src).map_err(|e| e.emit_to_string(src))?;
    let mut validator = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all());
    validator.validate(&module).map_err(|e| e.emit_to_string(src))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::variations;

    #[test]
    fn prelude_is_valid() {
        let src = format!("{}\n@group(0) @binding(0) var<storage, read_write> prm: array<f32>;\n@compute @workgroup_size(1) fn main() {{ prm[0] = rnd(); }}\n", prelude());
        validate(&src).unwrap();
    }

    /// Every snippet on its own.
    #[test]
    fn every_variation_snippet_is_valid_wgsl() {
        let mut failures = Vec::new();
        for def in variations::all() {
            if !def.supports_gpu() {
                continue;
            }
            if let Err(e) = validate(&test_shader(&[def])) {
                failures.push(format!("--- {} ---\n{}", def.name, e));
            }
        }
        assert!(failures.is_empty(), "invalid WGSL snippets ({}):\n{}", failures.len(), failures.join("\n"));
    }

    /// All snippets together (catches helper-name collisions between variations).
    #[test]
    fn all_variations_in_one_module() {
        let defs: Vec<&VarDef> = variations::all().iter().copied().filter(|d| d.supports_gpu()).collect();
        if let Err(e) = validate(&test_shader(&defs)) {
            panic!("combined module invalid:\n{}", e.lines().take(30).collect::<Vec<_>>().join("\n"));
        }
    }
}
