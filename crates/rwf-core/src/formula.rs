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

//! Formulas of the plot variations (`parplot2d_wf`, `yplot3d_wf`, ...).
//!
//! JWildfire compiles formulas as Java expressions (Janino, with `MathLib.*` statically
//! imported). Here they are parsed with Java's static typing (so `1/3` is an integer division),
//! constant-folded and compiled to a small stack bytecode. The bytecode is stored in the
//! prepared data of a variation and run by [`eval`] on the CPU and by `res_formula` in
//! `variations/wgsl/resource.wgsl` on the GPU, so both renderers evaluate the same program.
//!
//! Bytecode: a sequence of `f64` values; `OP_CONST` and `OP_VAR` are followed by an operand.

use std::collections::HashMap;

pub const OP_CONST: u32 = 0;
pub const OP_VAR: u32 = 1;
pub const OP_NEG: u32 = 2;
pub const OP_ADD: u32 = 3;
pub const OP_SUB: u32 = 4;
pub const OP_MUL: u32 = 5;
pub const OP_DIV: u32 = 6;
/// Integer division (truncating; 0 for a zero divisor where Java would throw).
pub const OP_IDIV: u32 = 7;
/// `%` (truncated remainder, Java semantics for `double` and `int`).
pub const OP_REM: u32 = 8;
pub const OP_LT: u32 = 9;
pub const OP_LE: u32 = 10;
pub const OP_GT: u32 = 11;
pub const OP_GE: u32 = 12;
pub const OP_EQ: u32 = 13;
pub const OP_NE: u32 = 14;
pub const OP_AND: u32 = 15;
pub const OP_OR: u32 = 16;
pub const OP_NOT: u32 = 17;
/// `c ? a : b` with the stack `[c, a, b]` (both branches are evaluated; formulas have no
/// side effects).
pub const OP_SELECT: u32 = 18;
/// `(int)` cast: truncation towards zero.
pub const OP_TOINT: u32 = 19;
// Functions (argument count in `FUNCS`).
pub const OP_SIN: u32 = 20;
pub const OP_COS: u32 = 21;
pub const OP_TAN: u32 = 22;
pub const OP_ASIN: u32 = 23;
pub const OP_ACOS: u32 = 24;
pub const OP_ATAN: u32 = 25;
pub const OP_ATAN2: u32 = 26;
pub const OP_SINH: u32 = 27;
pub const OP_COSH: u32 = 28;
pub const OP_TANH: u32 = 29;
pub const OP_EXP: u32 = 30;
pub const OP_LOG: u32 = 31;
pub const OP_LOG10: u32 = 32;
pub const OP_SQRT: u32 = 33;
pub const OP_SQR: u32 = 34;
pub const OP_POW: u32 = 35;
pub const OP_ABS: u32 = 36;
pub const OP_FLOOR: u32 = 37;
pub const OP_CEIL: u32 = 38;
pub const OP_ROUND: u32 = 39;
pub const OP_RINT: u32 = 40;
pub const OP_TRUNC: u32 = 41;
pub const OP_FRAC: u32 = 42;
pub const OP_MIN: u32 = 43;
pub const OP_MAX: u32 = 44;
pub const OP_SIGN: u32 = 45;
pub const OP_ACOSH: u32 = 46;
pub const OP_ERF: u32 = 47;
pub const OP_LGAMMA: u32 = 48;
pub const OP_CBRT: u32 = 49;
pub const OP_HYPOT: u32 = 50;

/// Stack size of the GPU interpreter; deeper formulas are rejected.
pub const MAX_STACK: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ty {
    Int,
    Double,
    Bool,
}

/// `(name, opcode, arguments)`; `Math.` / `MathLib.` prefixes are accepted as well.
const FUNCS: &[(&str, u32, usize)] = &[
    ("sin", OP_SIN, 1),
    ("cos", OP_COS, 1),
    ("tan", OP_TAN, 1),
    ("asin", OP_ASIN, 1),
    ("acos", OP_ACOS, 1),
    ("atan", OP_ATAN, 1),
    ("atan2", OP_ATAN2, 2),
    ("sinh", OP_SINH, 1),
    ("cosh", OP_COSH, 1),
    ("tanh", OP_TANH, 1),
    ("exp", OP_EXP, 1),
    ("log", OP_LOG, 1),
    ("log10", OP_LOG10, 1),
    ("sqrt", OP_SQRT, 1),
    ("sqr", OP_SQR, 1),
    ("pow", OP_POW, 2),
    ("fabs", OP_ABS, 1),
    ("abs", OP_ABS, 1),
    ("iabs", OP_ABS, 1),
    ("floor", OP_FLOOR, 1),
    ("ceil", OP_CEIL, 1),
    ("round", OP_ROUND, 1),
    ("rint", OP_RINT, 1),
    ("trunc", OP_TRUNC, 1),
    ("frac", OP_FRAC, 1),
    ("min", OP_MIN, 2),
    ("max", OP_MAX, 2),
    ("iMin", OP_MIN, 2),
    ("iMax", OP_MAX, 2),
    ("fmod", OP_REM, 2),
    ("sign", OP_SIGN, 1),
    ("signum", OP_SIGN, 1),
    ("acosh", OP_ACOSH, 1),
    ("erf", OP_ERF, 1),
    ("lgamma", OP_LGAMMA, 1),
    ("cbrt", OP_CBRT, 1),
    ("hypot", OP_HYPOT, 2),
];

/// `MathLib` / `Math` constants.
fn constant(name: &str) -> Option<f64> {
    use std::f64::consts::*;
    Some(match name {
        "M_PI" | "Math.PI" | "PI" => PI,
        "M_PI_2" => PI * 0.5,
        "M_PI_4" => PI * 0.25,
        "M_1_PI" => 1.0 / PI,
        "M_2_PI" => 2.0 / PI,
        "M_2PI" => 2.0 * PI,
        "M_1_2PI" => 1.0 / PI / 2.0,
        "M_SQRT2" => SQRT_2,
        "M_E" | "Math.E" => E,
        "M_LOG2E" => LOG2_E,
        "EPSILON" => 0.00000001,
        "SMALL_EPSILON" => 1.0e-300,
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64, Ty),
    Ident(String),
    Op(&'static str),
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    const OPS: &[&str] = &["&&", "||", "<=", ">=", "==", "!=", "+", "-", "*", "/", "%", "(", ")", ",", "?", ":", "<", ">", "!"];
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() || c == ';' {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            let mut float = false;
            while i < b.len() && ((b[i] as char).is_ascii_digit() || b[i] == b'.') {
                float |= b[i] == b'.';
                i += 1;
            }
            if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                let mut j = i + 1;
                if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                if j < b.len() && (b[j] as char).is_ascii_digit() {
                    float = true;
                    i = j;
                    while i < b.len() && (b[i] as char).is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text = &src[start..i];
            let mut ty = if float { Ty::Double } else { Ty::Int };
            match b.get(i).map(|c| c.to_ascii_lowercase()) {
                Some(b'd') | Some(b'f') => {
                    ty = Ty::Double;
                    i += 1;
                }
                Some(b'l') => i += 1,
                _ => {}
            }
            let v: f64 = text.parse().map_err(|_| format!("invalid number '{text}'"))?;
            out.push(Tok::Num(v, ty));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            out.push(Tok::Ident(src[start..i].to_string()));
        } else if let Some(op) = OPS.iter().find(|op| src[i..].starts_with(*op)) {
            out.push(Tok::Op(op));
            i += op.len();
        } else {
            return Err(format!("unexpected character '{c}'"));
        }
    }
    Ok(out)
}

/// Expression tree with static Java types.
#[derive(Clone, Debug)]
enum Node {
    Const(f64, Ty),
    Var(usize),
    Unary(u32, Box<Node>, Ty),
    Binary(u32, Box<Node>, Box<Node>, Ty),
    Call(u32, Vec<Node>, Ty),
    Select(Box<Node>, Box<Node>, Box<Node>, Ty),
}

impl Node {
    fn ty(&self) -> Ty {
        match self {
            Node::Const(_, t) | Node::Unary(_, _, t) | Node::Binary(_, _, _, t) | Node::Call(_, _, t) | Node::Select(_, _, _, t) => *t,
            Node::Var(_) => Ty::Double,
        }
    }
}

struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    vars: &'a [&'a str],
    consts: &'a HashMap<String, f64>,
    /// All literals are `double` (JEP semantics of preset files).
    all_double: bool,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.pos += 1;
            return true;
        }
        false
    }
    fn expect(&mut self, op: &str) -> Result<(), String> {
        if self.eat(op) { Ok(()) } else { Err(format!("expected '{op}'")) }
    }

    fn ternary(&mut self) -> Result<Node, String> {
        let c = self.binary(0)?;
        if self.eat("?") {
            let a = self.ternary()?;
            self.expect(":")?;
            let b = self.ternary()?;
            let ty = if a.ty() == Ty::Int && b.ty() == Ty::Int {
                Ty::Int
            } else if a.ty() == Ty::Bool && b.ty() == Ty::Bool {
                Ty::Bool
            } else {
                Ty::Double
            };
            return Ok(fold(Node::Select(Box::new(c), Box::new(a), Box::new(b), ty)));
        }
        Ok(c)
    }

    fn binary(&mut self, min_prec: u8) -> Result<Node, String> {
        const LEVELS: &[&[(&str, u32)]] = &[
            &[("||", OP_OR)],
            &[("&&", OP_AND)],
            &[("==", OP_EQ), ("!=", OP_NE)],
            &[("<=", OP_LE), (">=", OP_GE), ("<", OP_LT), (">", OP_GT)],
            &[("+", OP_ADD), ("-", OP_SUB)],
            &[("*", OP_MUL), ("/", OP_DIV), ("%", OP_REM)],
        ];
        if min_prec as usize >= LEVELS.len() {
            return self.unary();
        }
        let mut lhs = self.binary(min_prec + 1)?;
        'outer: loop {
            for &(op, code) in LEVELS[min_prec as usize] {
                if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
                    self.pos += 1;
                    let rhs = self.binary(min_prec + 1)?;
                    lhs = binary_node(code, lhs, rhs);
                    continue 'outer;
                }
            }
            return Ok(lhs);
        }
    }

    fn unary(&mut self) -> Result<Node, String> {
        if self.eat("-") {
            let e = self.unary()?;
            let ty = if e.ty() == Ty::Int { Ty::Int } else { Ty::Double };
            return Ok(fold(Node::Unary(OP_NEG, Box::new(e), ty)));
        }
        if self.eat("+") {
            return self.unary();
        }
        if self.eat("!") {
            let e = self.unary()?;
            return Ok(fold(Node::Unary(OP_NOT, Box::new(e), Ty::Bool)));
        }
        // casts
        if matches!(self.peek(), Some(Tok::Op("(")))
            && let Some(Tok::Ident(t)) = self.toks.get(self.pos + 1)
            && matches!(t.as_str(), "int" | "long" | "double" | "float" | "short")
            && matches!(self.toks.get(self.pos + 2), Some(Tok::Op(")")))
        {
            let t = t.clone();
            self.pos += 3;
            let e = self.unary()?;
            return Ok(match t.as_str() {
                "double" | "float" => match e {
                    Node::Const(v, _) => Node::Const(v, Ty::Double),
                    e => Node::Unary(OP_ADD, Box::new(e), Ty::Double),
                },
                _ => fold(Node::Unary(OP_TOINT, Box::new(e), Ty::Int)),
            });
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Node, String> {
        match self.toks.get(self.pos).cloned() {
            Some(Tok::Num(v, ty)) => {
                self.pos += 1;
                Ok(Node::Const(v, if self.all_double { Ty::Double } else { ty }))
            }
            Some(Tok::Op("(")) => {
                self.pos += 1;
                let e = self.ternary()?;
                self.expect(")")?;
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                self.pos += 1;
                if self.eat("(") {
                    let mut args = Vec::new();
                    if !self.eat(")") {
                        loop {
                            args.push(self.ternary()?);
                            if self.eat(")") {
                                break;
                            }
                            self.expect(",")?;
                        }
                    }
                    let base = name.strip_prefix("Math.").or_else(|| name.strip_prefix("MathLib.")).unwrap_or(&name);
                    let &(_, op, n) = FUNCS.iter().find(|f| f.0 == base).ok_or_else(|| format!("unknown function '{name}'"))?;
                    if args.len() != n {
                        return Err(format!("{name} expects {n} argument(s)"));
                    }
                    let ints = args.iter().all(|a| a.ty() == Ty::Int);
                    let ty = match op {
                        OP_SIGN => Ty::Int,
                        OP_ABS | OP_MIN | OP_MAX if ints => Ty::Int,
                        OP_ROUND if base == "round" && name.starts_with("Math.") => Ty::Int,
                        _ => Ty::Double,
                    };
                    return Ok(fold(Node::Call(op, args, ty)));
                }
                if let Some(i) = self.vars.iter().position(|v| *v == name) {
                    return Ok(Node::Var(i));
                }
                if let Some(&v) = self.consts.get(&name) {
                    return Ok(Node::Const(v, Ty::Double));
                }
                if let Some(v) = constant(&name) {
                    return Ok(Node::Const(v, Ty::Double));
                }
                match name.as_str() {
                    "true" => Ok(Node::Const(1.0, Ty::Bool)),
                    "false" => Ok(Node::Const(0.0, Ty::Bool)),
                    _ => Err(format!("unknown name '{name}'")),
                }
            }
            Some(t) => Err(format!("unexpected {t:?}")),
            None => Err("unexpected end of formula".into()),
        }
    }
}

fn binary_node(op: u32, a: Node, b: Node) -> Node {
    let ints = a.ty() == Ty::Int && b.ty() == Ty::Int;
    let (op, ty) = match op {
        OP_DIV if ints => (OP_IDIV, Ty::Int),
        OP_ADD | OP_SUB | OP_MUL | OP_REM => (op, if ints { Ty::Int } else { Ty::Double }),
        OP_DIV => (op, Ty::Double),
        _ => (op, Ty::Bool),
    };
    fold(Node::Binary(op, Box::new(a), Box::new(b), ty))
}

/// Evaluates constant subtrees.
fn fold(n: Node) -> Node {
    let all_const = match &n {
        Node::Unary(_, a, _) => matches!(**a, Node::Const(..)),
        Node::Binary(_, a, b, _) => matches!(**a, Node::Const(..)) && matches!(**b, Node::Const(..)),
        Node::Call(_, args, _) => args.iter().all(|a| matches!(a, Node::Const(..))),
        Node::Select(c, a, b, _) => matches!(**c, Node::Const(..)) && matches!(**a, Node::Const(..)) && matches!(**b, Node::Const(..)),
        _ => false,
    };
    if !all_const {
        return n;
    }
    let ty = n.ty();
    let mut code = Vec::new();
    emit(&n, &mut code);
    let v = eval(&code, &[]);
    // int results wrap like Java ints
    let v = if ty == Ty::Int && v.is_finite() { (v as i64 as i32) as f64 } else { v };
    Node::Const(v, ty)
}

fn emit(n: &Node, out: &mut Vec<f64>) {
    match n {
        Node::Const(v, _) => out.extend([OP_CONST as f64, *v]),
        Node::Var(i) => out.extend([OP_VAR as f64, *i as f64]),
        Node::Unary(op, a, _) => {
            emit(a, out);
            if *op == OP_ADD {
                return; // (double) cast
            }
            out.push(*op as f64);
        }
        Node::Binary(op, a, b, _) => {
            emit(a, out);
            emit(b, out);
            out.push(*op as f64);
        }
        Node::Call(op, args, _) => {
            for a in args {
                emit(a, out);
            }
            out.push(*op as f64);
        }
        Node::Select(c, a, b, _) => {
            emit(c, out);
            emit(a, out);
            emit(b, out);
            out.push(OP_SELECT as f64);
        }
    }
}

/// Maximum stack depth of a program.
fn stack_depth(code: &[f64]) -> usize {
    let (mut sp, mut max, mut pc) = (0i32, 0i32, 0);
    while pc < code.len() {
        let op = code[pc] as u32;
        pc += 1;
        sp += match op {
            OP_CONST | OP_VAR => {
                pc += 1;
                1
            }
            OP_NEG | OP_NOT | OP_TOINT => 0,
            OP_SELECT => -2,
            OP_ADD..=OP_OR => -1,
            _ => 1 - FUNCS.iter().find(|f| f.1 == op).map_or(1, |f| f.2) as i32,
        };
        max = max.max(sp);
    }
    max as usize
}

/// A compiled formula.
#[derive(Clone, Debug, PartialEq)]
pub struct Program {
    pub code: Vec<f64>,
}

/// Compiles a Java expression. `vars` are the runtime inputs (in [`eval`] order), `consts`
/// named values substituted at compile time (`pi`, `param_a`, ...). An empty formula is `0.0`
/// (as in JWildfire).
pub fn compile(src: &str, vars: &[&str], consts: &HashMap<String, f64>) -> Result<Program, String> {
    let src = src.trim();
    let toks = lex(if src.is_empty() { "0.0" } else { src })?;
    let mut p = Parser { toks, pos: 0, vars, consts, all_double: false };
    let tree = p.ternary()?;
    if p.pos < p.toks.len() {
        return Err(format!("unexpected {:?}", p.toks[p.pos]));
    }
    if tree.ty() == Ty::Bool {
        return Err("formula is a condition, not a number".into());
    }
    let mut code = Vec::new();
    emit(&tree, &mut code);
    if stack_depth(&code) > MAX_STACK {
        return Err("formula is nested too deeply".into());
    }
    Ok(Program { code })
}

/// Evaluates a constant expression with all numbers as `double` (JWildfire evaluates preset
/// values with JEP, so `1/2` is 0.5 there). `pi` is predefined.
pub fn eval_constant(src: &str) -> Option<f64> {
    let mut consts = HashMap::new();
    consts.insert("pi".to_string(), std::f64::consts::PI);
    let mut p = Parser { toks: lex(src).ok()?, pos: 0, vars: &[], consts: &consts, all_double: true };
    let tree = p.ternary().ok()?;
    if p.pos < p.toks.len() {
        return None;
    }
    let mut code = Vec::new();
    emit(&tree, &mut code);
    Some(eval(&code, &[]))
}

fn b(v: bool) -> f64 {
    if v { 1.0 } else { 0.0 }
}

/// `Math.round`: `floor(x + 0.5)`.
fn java_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Runs a program (see the module docs).
pub fn eval(code: &[f64], vars: &[f64]) -> f64 {
    let mut st = [0.0f64; MAX_STACK];
    let mut sp = 0usize;
    let mut pc = 0;
    while pc < code.len() {
        let op = code[pc] as u32;
        pc += 1;
        macro_rules! un {
            ($f:expr) => {{
                let a = st[sp - 1];
                st[sp - 1] = $f(a);
            }};
        }
        macro_rules! bin {
            ($f:expr) => {{
                let (a, bb) = (st[sp - 2], st[sp - 1]);
                sp -= 1;
                st[sp - 1] = $f(a, bb);
            }};
        }
        match op {
            OP_CONST | OP_VAR => {
                st[sp] = if op == OP_CONST { code[pc] } else { vars.get(code[pc] as usize).copied().unwrap_or(0.0) };
                sp += 1;
                pc += 1;
            }
            OP_NEG => un!(|a: f64| -a),
            OP_NOT => un!(|a: f64| b(a == 0.0)),
            OP_TOINT => un!(|a: f64| if a.is_nan() { 0.0 } else { (a as i32) as f64 }),
            OP_ADD => bin!(|a, c| a + c),
            OP_SUB => bin!(|a, c| a - c),
            OP_MUL => bin!(|a, c| a * c),
            OP_DIV => bin!(|a, c| a / c),
            OP_IDIV => bin!(|a: f64, c: f64| if c == 0.0 { 0.0 } else { (a / c).trunc() }),
            OP_REM => bin!(|a: f64, c: f64| a % c),
            OP_LT => bin!(|a, c| b(a < c)),
            OP_LE => bin!(|a, c| b(a <= c)),
            OP_GT => bin!(|a, c| b(a > c)),
            OP_GE => bin!(|a, c| b(a >= c)),
            OP_EQ => bin!(|a, c| b(a == c)),
            OP_NE => bin!(|a, c| b(a != c)),
            OP_AND => bin!(|a, c| b(a != 0.0 && c != 0.0)),
            OP_OR => bin!(|a, c| b(a != 0.0 || c != 0.0)),
            OP_SELECT => {
                let (c, a, e) = (st[sp - 3], st[sp - 2], st[sp - 1]);
                sp -= 2;
                st[sp - 1] = if c != 0.0 { a } else { e };
            }
            OP_SIN => un!(f64::sin),
            OP_COS => un!(f64::cos),
            OP_TAN => un!(f64::tan),
            OP_ASIN => un!(f64::asin),
            OP_ACOS => un!(f64::acos),
            OP_ATAN => un!(f64::atan),
            OP_ATAN2 => bin!(f64::atan2),
            OP_SINH => un!(f64::sinh),
            OP_COSH => un!(f64::cosh),
            OP_TANH => un!(f64::tanh),
            OP_EXP => un!(f64::exp),
            OP_LOG => un!(f64::ln),
            OP_LOG10 => un!(f64::log10),
            OP_SQRT => un!(f64::sqrt),
            OP_SQR => un!(|a: f64| a * a),
            OP_POW => bin!(f64::powf),
            OP_ABS => un!(f64::abs),
            OP_FLOOR => un!(f64::floor),
            OP_CEIL => un!(f64::ceil),
            OP_ROUND => un!(java_round),
            OP_RINT => un!(f64::round_ties_even),
            OP_TRUNC => un!(f64::trunc),
            OP_FRAC => un!(|a: f64| a - a.trunc()),
            OP_MIN => bin!(f64::min),
            OP_MAX => bin!(f64::max),
            OP_SIGN => un!(|a: f64| if a > 0.0 {
                1.0
            } else if a < 0.0 {
                -1.0
            } else {
                0.0
            }),
            OP_ACOSH => un!(|d: f64| ((d * d - 1.0).sqrt() + d).ln()),
            OP_ERF => un!(crate::math::erf),
            OP_LGAMMA => un!(crate::math::lgamma),
            OP_CBRT => un!(f64::cbrt),
            OP_HYPOT => bin!(f64::hypot),
            _ => return f64::NAN,
        }
    }
    if sp > 0 { st[sp - 1] } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str, vars: &[&str], vals: &[f64]) -> f64 {
        let mut c = HashMap::new();
        c.insert("pi".to_string(), std::f64::consts::PI);
        c.insert("param_a".to_string(), 2.0);
        eval(&compile(src, vars, &c).unwrap().code, vals)
    }

    #[test]
    fn java_semantics() {
        assert_eq!(run("1/3", &[], &[]), 0.0);
        assert_eq!(run("1.0/4", &[], &[]), 0.25);
        assert_eq!(run("pow(x, 1/2)", &["x"], &[9.0]), 1.0);
        assert_eq!(run("7 % 3 + -7 % 3", &[], &[]), 0.0);
        assert_eq!(run("2 + 3 * 4 - 6 / 4", &[], &[]), 13.0);
        assert_eq!(run("(int)2.7 + (int)-2.7", &[], &[]), 0.0);
        assert_eq!(run("1e-3 * 1000", &[], &[]), 1.0);
        assert_eq!(run("2.5f * 2", &[], &[]), 5.0);
    }

    #[test]
    fn variables_functions_and_conditions() {
        assert_eq!(run("x>0?pow(x,param_a):-pow(-x,param_a)", &["x"], &[-3.0]), -9.0);
        assert_eq!(run("-param_a*z + param_a*((x>0)?1:0)*x*x", &["x", "z"], &[2.0, 1.0]), 6.0);
        assert!((run("sin(u)*cos(v) + sqr(fabs(-2)) + Math.min(1, 2)", &["u", "v"], &[0.5, 0.25]) - (0.5f64.sin() * 0.25f64.cos() + 5.0)).abs() < 1e-12);
        assert!((run("2 * pi", &[], &[]) - std::f64::consts::TAU).abs() < 1e-15);
        assert_eq!(run("x > 1 && x < 3 || !(x != 5) ? 1 : 0", &["x"], &[5.0]), 1.0);
        assert_eq!(run("", &[], &[]), 0.0);
    }

    #[test]
    fn constant_values_are_floating_point() {
        assert_eq!(eval_constant("1/2"), Some(0.5));
        assert!((eval_constant("2.0 * pi").unwrap() - std::f64::consts::TAU).abs() < 1e-15);
        assert_eq!(eval_constant("0.0;"), Some(0.0));
    }

    #[test]
    fn errors() {
        let c = HashMap::new();
        assert!(compile("foo(1)", &[], &c).is_err());
        assert!(compile("x +", &["x"], &c).is_err());
        assert!(compile("y", &["x"], &c).is_err());
        assert!(compile("x > 1", &["x"], &c).is_err());
    }

    #[test]
    fn constants_are_folded() {
        let c = HashMap::new();
        assert_eq!(compile("2 * 3 + sin(0)", &[], &c).unwrap().code, vec![OP_CONST as f64, 6.0]);
    }
}
