// RWildFire - a fractal flame editor written in Rust
// Copyright (C) 2026 Christian Nielebock
//
// Ported from JWildfire - an image and animation processor written in Java
// Copyright (C) 1995-2026 Andreas Maschke
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

//! `lsystem_js` (Jesus Sosa): a 2D L-system drawn by a turtle; the variation samples points on
//! its line segments. The grammar is a resource (JWildfire's syntax, including its parsing
//! quirks) or one of JWildfire's 26 presets (`presetId`).

use super::resource_vars::text;
use super::{Ctx, ParamDef, Pt, ResDef, ResKind, ResourceVar, VarDef, VarKind};
use crate::math::{M_2PI, ftoi};

/// `Prefs.tinaLSystemMaxLength`
const MAX_LENGTH: usize = 25_000;

/// JWildfire's default grammar of new instances.
const DEFAULT_GRAMMAR: &str = "; Small set of Commands for the Lsystem_js variation en JWF \n\
; F, or d : Draw segment forward (Step) size \n\
; G, or m : move forward, without drawing (Step) Size \n\
; + : rotate the turtle 90 Degress counterclockwise \n\
; - : rotate the turtle 90 Degress clockwise \n\
; | : rotate the turtle 180 Degress \n\
; ! : Switch the rotatation angle commands '+' clockwise, '-' = counterclockwise \n\
; [ : push the Turtle state on stack \n\
; ] : pop the Turtle Stack from Stack \n\
ADH155 { ; Anthony Hanmer 2000\n\
Angle 45 ; Cesaro variation\n\
Axiom f\n\
f=f++++++++++f--------------------f++++++++++f\n\
}\n";

/// `getGrammarId`: the grammar after marker `##NN` (the last one if `id` is not found).
pub(crate) fn preset_grammar(file: &str, id: i32) -> String {
    let mut grammar = String::new();
    let bytes = file.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i] == b'#' && bytes[i + 1] == b'#' && bytes[i + 2].is_ascii_digit() && bytes[i + 3].is_ascii_digit() {
            let number = ((bytes[i + 2] - b'0') * 10 + (bytes[i + 3] - b'0')) as i32;
            let rest = &file[i + 4..];
            grammar = match rest.find('}') {
                Some(end) => rest[..=end].to_string(),
                None => String::new(),
            };
            if number == id {
                break;
            }
            i += 4;
        } else {
            i += 1;
        }
    }
    grammar
}

/// `LSystem.processFile`: the expanded string and the angle (`360 / Angle`), `None` where
/// JWildfire's parser throws (then nothing is drawn).
pub(crate) fn expand(grammar: &str, iterations: i32) -> (Option<String>, f64) {
    let mut angle = 90.0;
    let mut axiom: Option<Vec<char>> = None;
    let mut rules: Vec<(char, Vec<char>)> = Vec::new();
    let parse = |angle: &mut f64, axiom: &mut Option<Vec<char>>, rules: &mut Vec<(char, Vec<char>)>| -> Option<()> {
        for raw in grammar.split('\n') {
            let raw = raw.strip_suffix('\r').unwrap_or(raw);
            let mut line = raw.to_string();
            if let Some(pos) = line.find(';') {
                if pos == 0 {
                    continue;
                }
                // JWildfire drops the character before the ';' as well.
                line = line.chars().take(line[..pos].chars().count() - 1).collect();
            }
            let line = line.to_uppercase();
            if line.contains('{') || line.contains('}') {
                continue;
            }
            let mut tokens = line.split_whitespace();
            if line.contains('=') || line.contains("AXIOM") {
                while let Some(token) = tokens.next() {
                    if token == "AXIOM" {
                        *axiom = Some(tokens.next()?.chars().collect());
                    } else {
                        // Every other token of the line adds the line's rule (once per token).
                        let eq = line.find('=')?;
                        let lhs: String = line[..eq].chars().filter(|c| !c.is_whitespace()).collect();
                        let rhs: Vec<char> = line[eq + 1..].chars().filter(|c| !c.is_whitespace()).collect();
                        rules.push((lhs.chars().next()?, rhs));
                    }
                }
            } else {
                while let Some(token) = tokens.next() {
                    if token == "ANGLE" {
                        let mut t = tokens.next()?;
                        if t == "=" {
                            t = tokens.next()?;
                        }
                        *angle = 360.0 / t.trim().parse::<f64>().ok()?;
                    }
                }
            }
        }
        Some(())
    };
    if parse(&mut angle, &mut axiom, &mut rules).is_none() {
        return (None, angle);
    }
    // `Productions.iterate`
    let Some(mut current) = axiom else { return (None, angle) };
    let mut result: Option<String> = if iterations > 0 { Some(String::new()) } else { None };
    for _ in 0..iterations.max(0) {
        let rule = |c: char| rules.iter().find(|(l, _)| *l == c).map(|(_, r)| r);
        // JWildfire rewrites from the end and stops once the (partly rewritten) string is
        // longer than the limit; the string of that iteration is then dropped.
        let mut suffix = 0usize;
        let mut too_long = false;
        for (i, &c) in current.iter().enumerate().rev() {
            suffix += rule(c).map_or(1, |r| r.len());
            if i + suffix > MAX_LENGTH {
                too_long = true;
                break;
            }
        }
        if too_long {
            break;
        }
        current = current.iter().flat_map(|&c| rule(c).cloned().unwrap_or_else(|| vec![c])).collect();
        result = Some(current.iter().collect());
    }
    (result, angle)
}

/// A drawn segment: start, end and color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Segment {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub color: f64,
}

#[derive(Clone, Copy)]
struct Turtle {
    x: f64,
    y: f64,
    angle: f64,
    step: f64,
    color: f64,
}

/// The number at the start of `s` (`[0-9]*\.?[0-9]*`) and its length; `None` if it does not
/// parse (JWildfire stops drawing there).
fn leading_number(s: &[char]) -> Option<(f64, usize)> {
    let mut n = s.iter().take_while(|c| c.is_ascii_digit()).count();
    if s.get(n) == Some(&'.') {
        n += 1;
        n += s[n..].iter().take_while(|c| c.is_ascii_digit()).count();
    }
    let text: String = s[..n].iter().collect();
    text.parse::<f64>().ok().map(|v| (v, n))
}

/// `Render.render`: the segments the turtle draws (until a command JWildfire cannot execute).
pub(crate) fn draw(s: &str, angle: f64, step: f64) -> Vec<Segment> {
    let s: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut t = Turtle { x: 0.0, y: 0.0, angle: 0.0, step, color: 0.0 };
    let mut render_step = step;
    let mut stack: Vec<Turtle> = Vec::new();
    let mut sign = 1.0;
    let mut i = 0usize;
    let forward = |t: &mut Turtle, step: f64| {
        t.step = step;
        let a = t.angle.to_radians();
        t.x += step * a.cos();
        t.y += step * a.sin();
    };
    while i < s.len() {
        match s[i] {
            'D' | 'F' => {
                let (ox, oy) = (t.x, t.y);
                forward(&mut t, render_step);
                out.push(Segment { a: [ox, oy], b: [t.x, t.y], color: t.color });
                i += 1;
            }
            'M' | 'G' => {
                forward(&mut t, render_step);
                i += 1;
            }
            '+' => {
                t.angle += angle * sign;
                i += 1;
            }
            '-' => {
                t.angle -= angle * sign;
                i += 1;
            }
            '|' => {
                t.angle += 180.0;
                i += 1;
            }
            '!' => {
                sign = -sign;
                i += 1;
            }
            '[' => {
                stack.push(t);
                i += 1;
            }
            ']' => {
                let Some(top) = stack.pop() else { break };
                t = top;
                render_step = t.step;
                i += 1;
            }
            c @ ('/' | '\\') => {
                let Some((v, n)) = leading_number(&s[i + 1..]) else { break };
                if c == '\\' {
                    t.angle += v * sign;
                } else {
                    t.angle -= v * sign;
                }
                i += n + 1;
            }
            '@' => {
                let mut k = i + 1;
                let mut comb = String::new();
                for _ in 0..2 {
                    match s.get(k) {
                        Some(&c @ ('Q' | 'I')) => {
                            comb.push(c);
                            k += 1;
                            i += 1;
                        }
                        Some(_) => {}
                        None => return out,
                    }
                }
                let Some((v, n)) = leading_number(&s[k..]) else { break };
                let scaled = match comb.as_str() {
                    "" => Some(render_step * v),
                    "Q" => Some(render_step * v.sqrt()),
                    "I" => Some(render_step / v),
                    "IQ" => Some(render_step / v.sqrt()),
                    "QI" => Some(render_step * v.sqrt() / v),
                    _ => None,
                };
                if let Some(st) = scaled {
                    render_step = st;
                    t.step = st;
                }
                i += n + 1;
            }
            c @ ('<' | '>' | 'C') => {
                let Some((v, n)) = leading_number(&s[i + 1..]) else { break };
                let v = v / 256.0;
                t.color = match c {
                    '<' => t.color + v,
                    '>' => t.color - v,
                    _ => v,
                };
                i += n + 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// Data: [iters, step, angle, presetId, show_lines, line_thickness, show_points,
/// point_thickness, color_mode, line threshold, point threshold, line thickness / 100,
/// point thickness / 100, segment count, segment start, segments (x1, y1, x2, y2, color) ..].
const SEGS: usize = 13;

fn load(d: &mut Vec<f64>, res: &[Vec<u8>]) {
    let preset = d[3] as i32;
    let grammar = if preset != -1 { preset_grammar(include_str!("../../assets/lsystems.txt"), preset) } else { text(&res[0]) };
    let (expanded, file_angle) = expand(&grammar, d[0] as i32);
    let angle = if d[2] == 0.0 { file_angle } else { d[2] };
    let segments = expanded.map(|s| draw(&s, angle, d[1])).unwrap_or_default();
    d[SEGS] = segments.len() as f64;
    d[SEGS + 1] = d.len() as f64;
    d.reserve(5 * segments.len());
    for s in segments {
        d.extend([s.a[0], s.a[1], s.b[0], s.b[1], s.color]);
    }
}

/// `plotLine` / `plotPoint` on a random segment.
fn apply(d: &[f64], ctx: &mut Ctx, a: &Pt, v: &mut Pt, amount: f64) {
    let n = d[SEGS] as usize;
    if n == 0 {
        return;
    }
    let o = d[SEGS + 1] as usize + 5 * ctx.random_int(n);
    let (x1, y1, x2, y2, c) = (d[o], d[o + 1], d[o + 2], d[o + 3], d[o + 4]);
    let (mut ox, mut oy, mut oc) = (0.0, 0.0, 0.0);
    let r = ctx.random();
    if r < d[9] {
        let (xdiff, ydiff) = (x2 - x1, y2 - y1);
        let m = if xdiff == 0.0 { 10_000.0 } else { ydiff / xdiff };
        let len = (xdiff * xdiff + ydiff * ydiff).sqrt();
        let dist = ctx.random() * len;
        let mut xo = dist / (1.0 + m * m).sqrt();
        if x2 < x1 {
            xo = -xo;
        }
        let mut yo = (m * xo).abs();
        if y2 < y1 {
            yo = -yo;
        }
        if d[11] != 0.0 {
            xo += (ctx.random() - 0.5) * d[11];
            yo += (ctx.random() - 0.5) * d[11];
        }
        (ox, oy, oc) = (x1 + xo, y1 + yo, c);
    } else if r <= d[10] {
        let (mut xo, mut yo) = (0.0, 0.0);
        if d[12] != 0.0 {
            let ro = ctx.random() * d[12];
            let ra = ctx.random() * M_2PI;
            (xo, yo) = (ro * ra.cos(), ro * ra.sin());
        }
        (ox, oy, oc) = (x1 + xo, y1 + yo, c);
    }
    v.x += amount * ox;
    v.y += amount * oy;
    ctx.pass_z(a, v, amount);
    if d[8] != 0.0 {
        v.color = oc;
    }
}

pub static DEFS: &[VarDef] = &[VarDef {
    name: "lsystem_js",
    params: &[
        ParamDef::int("iters", 3.0, 0.0, 20.0),
        ParamDef::new("step", 0.1),
        ParamDef::new("angle", 0.0),
        ParamDef::int("presetId", -1.0, -1.0, 99.0),
        ParamDef::int("show_lines_param", 1.0, 0.0, 1.0),
        ParamDef::new("line_thickness", 0.5),
        ParamDef::int("show_points_param", 0.0, 0.0, 1.0),
        ParamDef::new("point_thickness_param", 3.0),
        ParamDef::int("color_mode", 0.0, 0.0, 1.0),
    ],
    kind: VarKind::Normal,
    is_3d: false,
    apply: |d, ctx, a, v, amount| apply(d, ctx, a, v, amount),
    prepare: Some(|p, _| {
        let mut d = p.to_vec();
        d[0] = (p[0] as i64) as f64;
        d[3] = (p[3] as i64) as f64;
        let (lines, points) = ((p[4] as i64) as f64, (p[6] as i64) as f64);
        d[8] = ftoi(p[8]).clamp(0, 1) as f64;
        let sum = lines + points;
        let (lt, pt) = (lines / sum, lines / sum + points / sum);
        d.extend([lt, pt, p[5] / 100.0, p[7] / 100.0, 0.0, 0.0]);
        d
    }),
    wgsl: "let n = u32(D(13));
if (n > 0u) {
  let o = u32(D(14)) + 5u * (rnd_u32() % n);
  let x1 = prm[base + o]; let y1 = prm[base + o + 1u]; let x2 = prm[base + o + 2u]; let y2 = prm[base + o + 3u]; let c = prm[base + o + 4u];
  var ox = 0.0; var oy = 0.0; var oc = 0.0;
  let r = rnd();
  if (r < D(9)) {
    let xdiff = x2 - x1; let ydiff = y2 - y1;
    var m = 10000.0;
    if (xdiff != 0.0) { m = ydiff / xdiff; }
    let len = sqrt(xdiff * xdiff + ydiff * ydiff);
    let dist = rnd() * len;
    var xo = dist / sqrt(1.0 + m * m);
    if (x2 < x1) { xo = -xo; }
    var yo = abs(m * xo);
    if (y2 < y1) { yo = -yo; }
    if (D(11) != 0.0) { xo += (rnd() - 0.5) * D(11); yo += (rnd() - 0.5) * D(11); }
    ox = x1 + xo; oy = y1 + yo; oc = c;
  } else if (r <= D(10)) {
    var xo = 0.0; var yo = 0.0;
    if (D(12) != 0.0) { let ro = rnd() * D(12); let ra = rnd() * TWO_PI; xo = ro * cos(ra); yo = ro * sin(ra); }
    ox = x1 + xo; oy = y1 + yo; oc = c;
  }
  v.x += AMOUNT * ox;
  v.y += AMOUNT * oy;
  if (PRESERVE_Z) { v.z += AMOUNT * a.z; }
  if (D(8) != 0.0) { v.color = oc; }
}",
}];

pub static RESOURCES: &[ResourceVar] = &[ResourceVar {
    name: "lsystem_js",
    resources: &[ResDef { name: "grammar", kind: ResKind::Text, default: DEFAULT_GRAMMAR }],
    load: |d, res, _| load(d, res),
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_parse_and_draw() {
        let file = include_str!("../../assets/lsystems.txt");
        for id in 1..=26 {
            let g = preset_grammar(file, id);
            assert!(g.contains('}'), "preset {id}");
            let (s, angle) = expand(&g, 3);
            let s = s.unwrap_or_else(|| panic!("preset {id} does not parse"));
            assert!(angle > 0.0);
            assert!(!draw(&s, angle, 0.1).is_empty(), "preset {id} draws nothing");
        }
    }

    #[test]
    fn koch_curve() {
        let (s, angle) = expand("Koch{\nAngle 6 ; 60 degrees\nAxiom F ;\nF=F+F--F+F ;\n}\n", 1);
        assert_eq!(s.as_deref(), Some("F+F--F+F"));
        assert!((angle - 60.0).abs() < 1e-12);
        let segs = draw("F+F--F+F", angle, 1.0);
        assert_eq!(segs.len(), 4);
        let end = segs[3].b;
        assert!((end[0] - 3.0).abs() < 1e-9 && end[1].abs() < 1e-9, "{end:?}");
    }

    #[test]
    fn jwildfire_quirks() {
        // A spaced rule adds itself three times (harmless); "angle = 4" is read as a rule.
        let (s, angle) = expand("X {\nangle = 4\naxiom AF\nF = FF\n}\n", 1);
        assert_eq!(angle, 90.0);
        assert_eq!(s.as_deref(), Some("4FF"), "the angle line became the rule A -> 4");
        // Iteration 0 gives the empty string, a missing axiom no string at all.
        assert_eq!(expand("X {\naxiom F\n}\n", 0).0, None);
        assert_eq!(expand("X {\nF=FF\n}\n", 2).0, None);
        // The default grammar uses a lower-case axiom and rule: upper-cased by the parser.
        let (s, _) = expand(DEFAULT_GRAMMAR, 1);
        assert_eq!(s.unwrap().len(), 44);
    }

    #[test]
    fn turtle_commands() {
        // '\\90' turns left by 90, '@2' doubles the step, '[ ]' restores the turtle.
        let segs = draw("F\\90F@2F[+F]F", 90.0, 1.0);
        let ends: Vec<[f64; 2]> = segs.iter().map(|s| [(s.b[0] * 1e9).round() / 1e9, (s.b[1] * 1e9).round() / 1e9]).collect();
        assert_eq!(ends, vec![[1.0, 0.0], [1.0, 1.0], [1.0, 3.0], [-1.0, 3.0], [1.0, 5.0]]);
        // A number that does not parse stops drawing.
        assert_eq!(draw("F/F", 90.0, 1.0).len(), 1);
    }
}
