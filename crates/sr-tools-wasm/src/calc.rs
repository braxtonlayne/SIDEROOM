//! The calculator (UTL-CALC-001): a scientific expression evaluator and a programmer-mode integer evaluator.
//!
//! Scientific mode (`eval`): `+ - * / ^ **`, `mod`, postfix `!` and `%`, parentheses, implicit multiplication
//! (`2pi`, `3(4+1)`), the usual functions and constants, scientific notation (`1.5e3`), degrees or radians and
//! an `ans` variable. `%` is the postfix "divide by 100" unless an operand follows it directly (`10 % 3` is a
//! modulo); `mod` is always a modulo. Results are `f64`, shown to 12 significant digits by [`format_number`].
//!
//! Programmer mode (`eval_int`): integers of 8, 16, 32 or 64 bits, signed (two's complement) or unsigned, C-like
//! operators (`| ^ & << >> >>> + - * / % ~`), literals `0x`, `0b`, `0o` or bare digits in the chosen input base.
//! Every operation wraps to the width, like the machine does; the result is shown in all four bases.
//!
//! Both are pure and total: bad input is an error value with a stable `code` and a character position, never a
//! panic. The browser localises the code.

use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

/// What went wrong and where (a character index into the input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalcError {
    /// Stable, English, lowercase with underscores: the key the UI localises.
    pub code: &'static str,
    pub pos: usize,
    /// A name or detail some codes carry (the unknown function, the bad digit).
    pub detail: String,
}

pub(crate) fn err(code: &'static str, pos: usize) -> CalcError {
    CalcError {
        code,
        pos,
        detail: String::new(),
    }
}

fn err_d(code: &'static str, pos: usize, detail: impl Into<String>) -> CalcError {
    CalcError {
        code,
        pos,
        detail: detail.into(),
    }
}

pub(crate) fn error_json(e: &CalcError) -> String {
    json!({ "ok": false, "error": { "code": e.code, "pos": e.pos, "detail": e.detail } }).to_string()
}

// ---------------------------------------------------------------------------------------------------------
// Number formatting
// ---------------------------------------------------------------------------------------------------------

/// A number to at most 12 significant digits, no trailing zeros, scientific notation outside 1e-6..1e15.
pub fn format_number(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return if x.is_nan() {
            "NaN".into()
        } else if x > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    let a = x.abs();
    if !(1e-6..1e15).contains(&a) {
        let s = format!("{x:.11e}");
        let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
        let mant = trim_zeros(mant);
        return format!("{mant}e{exp}");
    }
    let before = a.log10().floor() as i32 + 1;
    let decimals = (12 - before).clamp(0, 15) as usize;
    let s = format!("{x:.decimals$}");
    let t = trim_zeros(&s);
    if t == "-0" { "0".into() } else { t }
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s.to_owned()
    }
}

// ---------------------------------------------------------------------------------------------------------
// Scientific mode
// ---------------------------------------------------------------------------------------------------------

/// Evaluation options.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Trigonometric functions take and return degrees.
    pub degrees: bool,
    /// The value of `ans`, when there is a previous result.
    pub ans: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Bang,
    Percent,
    LParen,
    RParen,
    Comma,
}

/// Maps the typographic characters people paste to the ASCII the lexer knows.
fn normalise(c: char) -> char {
    match c {
        '\u{00d7}' | '\u{00b7}' | '\u{22c5}' => '*',
        '\u{00f7}' => '/',
        '\u{2212}' | '\u{2013}' => '-',
        '\u{ff08}' => '(',
        '\u{ff09}' => ')',
        other => other,
    }
}

fn is_symbol_ident(c: char) -> bool {
    matches!(c, '\u{221a}' | '\u{03c0}' | '\u{03c4}')
}

fn lex(input: &str) -> Result<Vec<(Tok, usize)>, CalcError> {
    let chars: Vec<char> = input.chars().map(normalise).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let mut s = String::new();
            let mut seen_dot = false;
            while i < chars.len() {
                let d = chars[i];
                if d.is_ascii_digit() {
                    s.push(d);
                } else if d == '_' && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
                    // digit grouping: 1_000_000
                } else if d == '.' && !seen_dot {
                    seen_dot = true;
                    s.push(d);
                } else {
                    break;
                }
                i += 1;
            }
            // Exponent: e/E, optional sign, digits. A lone `e` is the constant, so look ahead.
            if matches!(chars.get(i), Some('e' | 'E')) {
                let mut j = i + 1;
                if matches!(chars.get(j), Some('+' | '-')) {
                    j += 1;
                }
                if chars.get(j).is_some_and(char::is_ascii_digit) {
                    s.push('e');
                    if chars.get(i + 1) == Some(&'-') {
                        s.push('-');
                    }
                    i = j;
                    while chars.get(i).is_some_and(char::is_ascii_digit) {
                        s.push(chars[i]);
                        i += 1;
                    }
                }
            }
            let v: f64 = s.parse().map_err(|_| err("bad_number", start))?;
            if !v.is_finite() {
                return Err(err("overflow", start));
            }
            out.push((Tok::Num(v), start));
        } else if c.is_alphabetic() || c == '_' {
            let name = match c {
                '\u{221a}' => {
                    i += 1;
                    "sqrt".to_owned()
                }
                '\u{03c0}' => {
                    i += 1;
                    "pi".to_owned()
                }
                '\u{03c4}' => {
                    i += 1;
                    "tau".to_owned()
                }
                _ => {
                    let mut s = String::new();
                    while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') && !is_symbol_ident(chars[i])
                    {
                        s.extend(chars[i].to_lowercase());
                        i += 1;
                    }
                    s
                }
            };
            out.push((Tok::Ident(name), start));
        } else if c == '\u{221a}' {
            out.push((Tok::Ident("sqrt".into()), start));
            i += 1;
        } else {
            let t = match c {
                '+' => Tok::Plus,
                '-' => Tok::Minus,
                '*' => {
                    if chars.get(i + 1) == Some(&'*') {
                        i += 1;
                        Tok::Caret
                    } else {
                        Tok::Star
                    }
                }
                '/' => Tok::Slash,
                '^' => Tok::Caret,
                '!' => Tok::Bang,
                '%' => Tok::Percent,
                '(' => Tok::LParen,
                ')' => Tok::RParen,
                ',' => Tok::Comma,
                other => return Err(err_d("unexpected_char", start, other.to_string())),
            };
            out.push((t, start));
            i += 1;
        }
    }
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<(Tok, usize)>,
    i: usize,
    end: usize,
    opts: &'a Options,
    depth: usize,
}

const MAX_DEPTH: usize = 200;

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|(t, _)| t)
    }
    fn peek_at(&self, n: usize) -> Option<&Tok> {
        self.toks.get(self.i + n).map(|(t, _)| t)
    }
    fn pos(&self) -> usize {
        self.toks.get(self.i).map_or(self.end, |(_, p)| *p)
    }
    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.i).map(|(t, _)| t.clone());
        if t.is_some() {
            self.i += 1;
        }
        t
    }
    fn enter(&mut self) -> Result<(), CalcError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(err("too_deep", self.pos()));
        }
        Ok(())
    }

    fn starts_operand(t: Option<&Tok>) -> bool {
        matches!(t, Some(Tok::Num(_) | Tok::Ident(_) | Tok::LParen))
    }

    fn expr(&mut self) -> Result<f64, CalcError> {
        self.enter()?;
        let mut v = self.mul()?;
        loop {
            match self.peek() {
                Some(Tok::Plus) => {
                    self.i += 1;
                    v += self.mul()?;
                }
                Some(Tok::Minus) => {
                    self.i += 1;
                    v -= self.mul()?;
                }
                _ => break,
            }
        }
        self.depth -= 1;
        check(v, self.pos())
    }

    fn mul(&mut self) -> Result<f64, CalcError> {
        let mut v = self.unary()?;
        loop {
            let at = self.pos();
            match self.peek() {
                Some(Tok::Star) => {
                    self.i += 1;
                    v *= self.unary()?;
                }
                Some(Tok::Slash) => {
                    self.i += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return Err(err("divide_by_zero", at));
                    }
                    v /= d;
                }
                Some(Tok::Ident(w)) if w == "mod" => {
                    self.i += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return Err(err("divide_by_zero", at));
                    }
                    v %= d;
                }
                Some(Tok::Percent) if Self::starts_operand(self.peek_at(1)) => {
                    self.i += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return Err(err("divide_by_zero", at));
                    }
                    v %= d;
                }
                // Implicit multiplication: `2pi`, `3(4+1)`, `(1+2)(3+4)`.
                Some(Tok::Num(_) | Tok::LParen | Tok::Ident(_)) => v *= self.unary()?,
                _ => break,
            }
            check(v, at)?;
        }
        Ok(v)
    }

    fn unary(&mut self) -> Result<f64, CalcError> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.i += 1;
                self.enter()?;
                let v = self.unary()?;
                self.depth -= 1;
                Ok(-v)
            }
            Some(Tok::Plus) => {
                self.i += 1;
                self.enter()?;
                let v = self.unary()?;
                self.depth -= 1;
                Ok(v)
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<f64, CalcError> {
        let base = self.postfix()?;
        if self.peek() == Some(&Tok::Caret) {
            let at = self.pos();
            self.i += 1;
            self.enter()?;
            let exp = self.unary()?; // right associative, and `2^-3` works
            self.depth -= 1;
            return pow(base, exp, at);
        }
        Ok(base)
    }

    fn postfix(&mut self) -> Result<f64, CalcError> {
        let mut v = self.primary()?;
        loop {
            let at = self.pos();
            match self.peek() {
                Some(Tok::Bang) => {
                    self.i += 1;
                    v = factorial(v, at)?;
                }
                Some(Tok::Percent) if !Self::starts_operand(self.peek_at(1)) => {
                    self.i += 1;
                    v /= 100.0;
                }
                _ => break,
            }
        }
        Ok(v)
    }

    fn primary(&mut self) -> Result<f64, CalcError> {
        let at = self.pos();
        match self.bump() {
            Some(Tok::Num(v)) => Ok(v),
            Some(Tok::LParen) => {
                let v = self.expr()?;
                match self.bump() {
                    Some(Tok::RParen) => Ok(v),
                    _ => Err(err("unbalanced", at)),
                }
            }
            Some(Tok::Ident(name)) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.i += 1;
                    self.enter()?;
                    let args = self.args(at)?;
                    self.depth -= 1;
                    return call(&name, &args, self.opts, at);
                }
                if let Some(c) = constant(&name) {
                    return Ok(c);
                }
                if name == "ans" {
                    return self.opts.ans.ok_or_else(|| err("no_answer", at));
                }
                if is_function(&name) {
                    // `sin 30`, `\u{221a}9`: a function applied to the operand that follows it.
                    if !Self::starts_operand(self.peek()) && !matches!(self.peek(), Some(Tok::Minus | Tok::Plus)) {
                        return Err(err("missing_argument", at));
                    }
                    self.enter()?;
                    let arg = self.unary()?;
                    self.depth -= 1;
                    return call(&name, &[arg], self.opts, at);
                }
                Err(err_d("unknown_name", at, name))
            }
            Some(Tok::RParen) => Err(err("unbalanced", at)),
            Some(_) => Err(err("unexpected_token", at)),
            None => Err(err("unexpected_end", self.end)),
        }
    }

    fn args(&mut self, at: usize) -> Result<Vec<f64>, CalcError> {
        let mut out = Vec::new();
        if self.peek() == Some(&Tok::RParen) {
            self.i += 1;
            return Ok(out);
        }
        loop {
            out.push(self.expr()?);
            match self.bump() {
                Some(Tok::Comma) => {}
                Some(Tok::RParen) => return Ok(out),
                _ => return Err(err("unbalanced", at)),
            }
            if out.len() > 16 {
                return Err(err("too_many_arguments", at));
            }
        }
    }
}

fn check(v: f64, pos: usize) -> Result<f64, CalcError> {
    if v.is_nan() {
        Err(err("domain", pos))
    } else if v.is_infinite() {
        Err(err("overflow", pos))
    } else {
        Ok(v)
    }
}

fn pow(base: f64, exp: f64, pos: usize) -> Result<f64, CalcError> {
    if base == 0.0 && exp < 0.0 {
        return Err(err("divide_by_zero", pos));
    }
    check(base.powf(exp), pos)
}

fn factorial(v: f64, pos: usize) -> Result<f64, CalcError> {
    if v < 0.0 || v.fract() != 0.0 || v > 170.0 {
        return Err(err("domain", pos));
    }
    let mut r = 1.0_f64;
    let mut k = 2.0;
    while k <= v {
        r *= k;
        k += 1.0;
    }
    Ok(r)
}

fn constant(name: &str) -> Option<f64> {
    Some(match name {
        "pi" => std::f64::consts::PI,
        "e" => std::f64::consts::E,
        "tau" => std::f64::consts::TAU,
        "phi" => 1.618_033_988_749_895,
        _ => return None,
    })
}

const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
    "sqrt", "cbrt", "ln", "log", "log2", "log10", "exp", "abs", "floor", "ceil", "round", "trunc", "sign", "min",
    "max", "pow", "hypot", "fact", "root", "deg", "rad", "gcd", "lcm", "sum", "avg",
];

fn is_function(name: &str) -> bool {
    FUNCTIONS.contains(&name)
}

fn gcd(a: f64, b: f64) -> f64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0.0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Values within 1e-15 of zero (a rounding residue of sin/cos) are zero.
fn snap(x: f64) -> f64 {
    if x.abs() < 1e-15 { 0.0 } else { x }
}

fn call(name: &str, a: &[f64], o: &Options, pos: usize) -> Result<f64, CalcError> {
    let arity = |n: usize| -> Result<(), CalcError> {
        if a.len() == n { Ok(()) } else { Err(err_d("arity", pos, name)) }
    };
    let to_rad = |x: f64| if o.degrees { x.to_radians() } else { x };
    let from_rad = |x: f64| if o.degrees { x.to_degrees() } else { x };
    let unary = |f: fn(f64) -> f64| -> Result<f64, CalcError> {
        arity(1)?;
        check(f(a[0]), pos)
    };
    match name {
        "sin" => {
            arity(1)?;
            check(snap(to_rad(a[0]).sin()), pos)
        }
        "cos" => {
            arity(1)?;
            check(snap(to_rad(a[0]).cos()), pos)
        }
        "tan" => {
            arity(1)?;
            let x = to_rad(a[0]);
            if snap(x.cos()) == 0.0 {
                return Err(err("domain", pos));
            }
            check(snap(x.tan()), pos)
        }
        "asin" | "acos" => {
            arity(1)?;
            if !(-1.0..=1.0).contains(&a[0]) {
                return Err(err("domain", pos));
            }
            check(from_rad(if name == "asin" { a[0].asin() } else { a[0].acos() }), pos)
        }
        "atan" => {
            arity(1)?;
            check(from_rad(a[0].atan()), pos)
        }
        "atan2" => {
            arity(2)?;
            check(from_rad(a[0].atan2(a[1])), pos)
        }
        "sinh" => unary(f64::sinh),
        "cosh" => unary(f64::cosh),
        "tanh" => unary(f64::tanh),
        "asinh" => unary(f64::asinh),
        "acosh" => {
            arity(1)?;
            if a[0] < 1.0 {
                return Err(err("domain", pos));
            }
            check(a[0].acosh(), pos)
        }
        "atanh" => {
            arity(1)?;
            if a[0].abs() >= 1.0 {
                return Err(err("domain", pos));
            }
            check(a[0].atanh(), pos)
        }
        "sqrt" => {
            arity(1)?;
            if a[0] < 0.0 {
                return Err(err("domain", pos));
            }
            check(a[0].sqrt(), pos)
        }
        "cbrt" => unary(f64::cbrt),
        "ln" | "log" | "log10" | "log2" => {
            arity(1)?;
            if a[0] <= 0.0 {
                return Err(err("domain", pos));
            }
            check(
                match name {
                    "ln" => a[0].ln(),
                    "log2" => a[0].log2(),
                    _ => a[0].log10(),
                },
                pos,
            )
        }
        "exp" => unary(f64::exp),
        "abs" => unary(f64::abs),
        "floor" => unary(f64::floor),
        "ceil" => unary(f64::ceil),
        "trunc" => unary(f64::trunc),
        "round" => {
            if a.len() == 2 {
                let k = a[1].round().clamp(-15.0, 15.0);
                let m = 10f64.powf(k);
                return check((a[0] * m).round() / m, pos);
            }
            unary(f64::round)
        }
        "sign" => {
            arity(1)?;
            Ok(if a[0] == 0.0 { 0.0 } else { a[0].signum() })
        }
        "fact" => {
            arity(1)?;
            factorial(a[0], pos)
        }
        "deg" => {
            arity(1)?;
            Ok(a[0].to_degrees())
        }
        "rad" => {
            arity(1)?;
            Ok(a[0].to_radians())
        }
        "pow" => {
            arity(2)?;
            pow(a[0], a[1], pos)
        }
        "root" => {
            arity(2)?;
            if a[1] == 0.0 {
                return Err(err("divide_by_zero", pos));
            }
            if a[0] < 0.0 && (a[1].fract() != 0.0 || (a[1] as i64) % 2 == 0) {
                return Err(err("domain", pos));
            }
            let r = a[0].abs().powf(1.0 / a[1]);
            check(if a[0] < 0.0 { -r } else { r }, pos)
        }
        "hypot" => {
            arity(2)?;
            check(a[0].hypot(a[1]), pos)
        }
        "gcd" | "lcm" => {
            arity(2)?;
            if a[0].fract() != 0.0 || a[1].fract() != 0.0 {
                return Err(err("domain", pos));
            }
            let g = gcd(a[0], a[1]);
            if name == "gcd" {
                Ok(g)
            } else if g == 0.0 {
                Ok(0.0)
            } else {
                check((a[0] / g * a[1]).abs(), pos)
            }
        }
        "min" | "max" | "sum" | "avg" => {
            if a.is_empty() {
                return Err(err_d("arity", pos, name));
            }
            Ok(match name {
                "min" => a.iter().copied().fold(f64::INFINITY, f64::min),
                "max" => a.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                "sum" => a.iter().sum(),
                _ => a.iter().sum::<f64>() / a.len() as f64,
            })
        }
        _ => Err(err_d("unknown_function", pos, name)),
    }
}

/// Evaluate a scientific expression.
pub fn eval(expr: &str, opts: &Options) -> Result<f64, CalcError> {
    let toks = lex(expr)?;
    if toks.is_empty() {
        return Err(err("empty", 0));
    }
    let end = expr.chars().count();
    let mut p = Parser {
        toks,
        i: 0,
        end,
        opts,
        depth: 0,
    };
    let v = p.expr()?;
    if p.i < p.toks.len() {
        let at = p.pos();
        return Err(match p.peek() {
            Some(Tok::RParen) => err("unbalanced", at),
            _ => err("unexpected_token", at),
        });
    }
    check(v, end)
}

/// The JSON answer of [`eval`]: `{ok, value, text}` or `{ok:false, error:{code,pos,detail}}`.
pub fn eval_json(expr: &str, degrees: bool, ans: Option<f64>) -> String {
    match eval(expr, &Options { degrees, ans }) {
        Ok(v) => json!({ "ok": true, "value": v, "text": format_number(v) }).to_string(),
        Err(e) => error_json(&e),
    }
}

/// `ans` is NaN when there is no previous result.
#[wasm_bindgen(js_name = calcEval)]
pub fn calc_eval(expr: &str, degrees: bool, ans: f64) -> String {
    eval_json(expr, degrees, if ans.is_nan() { None } else { Some(ans) })
}

/// A number as the calculator shows it (12 significant digits).
#[wasm_bindgen(js_name = calcFormat)]
pub fn calc_format(x: f64) -> String {
    format_number(x)
}

// ---------------------------------------------------------------------------------------------------------
// Programmer mode
// ---------------------------------------------------------------------------------------------------------

/// Integer width and signedness, and the base bare digits are read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntMode {
    /// 8, 16, 32 or 64.
    pub bits: u32,
    pub signed: bool,
    /// 2, 8, 10 or 16.
    pub base: u32,
}

impl IntMode {
    fn mask(self) -> u128 {
        (1u128 << self.bits) - 1
    }
    fn wrap(self, v: u128) -> u128 {
        v & self.mask()
    }
    fn signed_of(self, v: u128) -> i128 {
        let sign = 1u128 << (self.bits - 1);
        if v & sign != 0 { (v | !self.mask()) as i128 } else { v as i128 }
    }
    fn wide(self, v: u128) -> i128 {
        if self.signed { self.signed_of(v) } else { v as i128 }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ITok {
    /// The value, and whether it is a bare decimal literal.
    Num(u128, bool),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    Ushr,
    LParen,
    RParen,
}

fn ilex(input: &str, mode: IntMode) -> Result<Vec<(ITok, usize)>, CalcError> {
    let chars: Vec<char> = input.chars().map(normalise).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_alphanumeric() {
            let mut base = mode.base;
            let mut prefixed = false;
            if c == '0' {
                match chars.get(i + 1).map(char::to_ascii_lowercase) {
                    Some('x') => {
                        (base, prefixed) = (16, true);
                        i += 2;
                    }
                    Some('b') if mode.base != 16 => {
                        (base, prefixed) = (2, true);
                        i += 2;
                    }
                    Some('o') => {
                        (base, prefixed) = (8, true);
                        i += 2;
                    }
                    _ => {}
                }
            }
            let digits_start = i;
            let mut v: u128 = 0;
            let mut overflow = false;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                let d = chars[i];
                i += 1;
                if d == '_' {
                    continue;
                }
                let Some(dv) = d.to_digit(36).filter(|dv| *dv < base) else {
                    return Err(err_d("bad_digit", i - 1, d.to_string()));
                };
                match v
                    .checked_mul(u128::from(base))
                    .and_then(|x| x.checked_add(u128::from(dv)))
                {
                    Some(n) => v = n,
                    None => overflow = true,
                }
            }
            if i == digits_start {
                return Err(err("bad_number", start));
            }
            // A bare signed decimal may reach 2^(bits-1) so that `-128` reads; the parser refuses it
            // when no minus sign precedes it.
            let bare_decimal = !prefixed && base == 10;
            let limit = if bare_decimal && mode.signed {
                1u128 << (mode.bits - 1)
            } else {
                mode.mask()
            };
            if overflow || v > limit {
                return Err(err("literal_too_large", start));
            }
            out.push((ITok::Num(v, bare_decimal), start));
            continue;
        }
        let t = match c {
            '+' => ITok::Plus,
            '-' => ITok::Minus,
            '*' => ITok::Star,
            '/' => ITok::Slash,
            '%' => ITok::Percent,
            '&' => ITok::Amp,
            '|' => ITok::Pipe,
            '^' => ITok::Caret,
            '~' => ITok::Tilde,
            '(' => ITok::LParen,
            ')' => ITok::RParen,
            '<' if chars.get(i + 1) == Some(&'<') => {
                i += 1;
                ITok::Shl
            }
            '>' if chars.get(i + 1) == Some(&'>') => {
                i += 1;
                if chars.get(i + 1) == Some(&'>') {
                    i += 1;
                    ITok::Ushr
                } else {
                    ITok::Shr
                }
            }
            other => return Err(err_d("unexpected_char", start, other.to_string())),
        };
        out.push((t, start));
        i += 1;
    }
    Ok(out)
}

struct IParser {
    toks: Vec<(ITok, usize)>,
    i: usize,
    end: usize,
    mode: IntMode,
    depth: usize,
    /// Set when an operation lost bits (the value no longer fits the width).
    wrapped: bool,
}

impl IParser {
    fn peek(&self) -> Option<&ITok> {
        self.toks.get(self.i).map(|(t, _)| t)
    }
    fn pos(&self) -> usize {
        self.toks.get(self.i).map_or(self.end, |(_, p)| *p)
    }
    fn enter(&mut self) -> Result<(), CalcError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(err("too_deep", self.pos()));
        }
        Ok(())
    }

    /// Wraps `wide` (an exact signed result) into the width; notes when bits were lost.
    fn fit(&mut self, wide: i128) -> u128 {
        let m = self.mode;
        let w = m.wrap(wide as u128);
        if m.wide(w) != wide {
            self.wrapped = true;
        }
        w
    }

    fn or(&mut self) -> Result<u128, CalcError> {
        self.enter()?;
        let mut v = self.xor()?;
        while self.peek() == Some(&ITok::Pipe) {
            self.i += 1;
            v |= self.xor()?;
        }
        self.depth -= 1;
        Ok(v)
    }
    fn xor(&mut self) -> Result<u128, CalcError> {
        let mut v = self.and()?;
        while self.peek() == Some(&ITok::Caret) {
            self.i += 1;
            v ^= self.and()?;
        }
        Ok(v)
    }
    fn and(&mut self) -> Result<u128, CalcError> {
        let mut v = self.shift()?;
        while self.peek() == Some(&ITok::Amp) {
            self.i += 1;
            v &= self.shift()?;
        }
        Ok(v)
    }
    fn shift(&mut self) -> Result<u128, CalcError> {
        let mut v = self.add()?;
        loop {
            let at = self.pos();
            let op = match self.peek() {
                Some(t @ (ITok::Shl | ITok::Shr | ITok::Ushr)) => t.clone(),
                _ => break,
            };
            self.i += 1;
            let n = self.add()?;
            let m = self.mode;
            let count = m.wide(n);
            if count < 0 || count >= i128::from(m.bits) {
                return Err(err("shift_range", at));
            }
            let count = count as u32;
            v = match op {
                ITok::Shl => {
                    let shifted = (v << count) & m.mask();
                    if (shifted >> count) != v {
                        self.wrapped = true;
                    }
                    shifted
                }
                ITok::Shr if m.signed => m.wrap((m.signed_of(v) >> count) as u128),
                _ => v >> count,
            };
        }
        Ok(v)
    }
    fn add(&mut self) -> Result<u128, CalcError> {
        let mut v = self.mul()?;
        loop {
            let m = self.mode;
            let plus = match self.peek() {
                Some(ITok::Plus) => true,
                Some(ITok::Minus) => false,
                _ => break,
            };
            self.i += 1;
            let r = self.mul()?;
            let (x, y) = (m.wide(v), m.wide(r));
            v = self.fit(if plus { x + y } else { x - y });
        }
        Ok(v)
    }
    fn mul(&mut self) -> Result<u128, CalcError> {
        let mut v = self.unary()?;
        loop {
            let at = self.pos();
            let kind = match self.peek() {
                Some(ITok::Star) => 0,
                Some(ITok::Slash) => 1,
                Some(ITok::Percent) => 2,
                _ => break,
            };
            self.i += 1;
            let r = self.unary()?;
            let m = self.mode;
            let (x, y) = (m.wide(v), m.wide(r));
            v = match kind {
                0 => match x.checked_mul(y) {
                    Some(p) => self.fit(p),
                    None => {
                        self.wrapped = true;
                        m.wrap(x.wrapping_mul(y) as u128)
                    }
                },
                _ if y == 0 => return Err(err("divide_by_zero", at)),
                1 => self.fit(x / y),
                _ => self.fit(x % y),
            };
        }
        Ok(v)
    }
    fn unary(&mut self) -> Result<u128, CalcError> {
        let m = self.mode;
        match self.peek() {
            Some(ITok::Minus) => {
                self.i += 1;
                // `-128` in 8 bits signed is a literal, not an overflow.
                if let Some(ITok::Num(n, _)) = self.peek().cloned() {
                    self.i += 1;
                    return Ok(self.fit(-(n as i128)));
                }
                self.enter()?;
                let v = self.unary()?;
                self.depth -= 1;
                Ok(self.fit(-m.wide(v)))
            }
            Some(ITok::Plus) => {
                self.i += 1;
                self.enter()?;
                let v = self.unary()?;
                self.depth -= 1;
                Ok(v)
            }
            Some(ITok::Tilde) => {
                self.i += 1;
                self.enter()?;
                let v = self.unary()?;
                self.depth -= 1;
                Ok(m.wrap(!v))
            }
            _ => self.primary(),
        }
    }
    fn primary(&mut self) -> Result<u128, CalcError> {
        let at = self.pos();
        let Some((t, _)) = self.toks.get(self.i).cloned() else {
            return Err(err("unexpected_end", self.end));
        };
        self.i += 1;
        match t {
            ITok::Num(v, bare_decimal) => {
                if bare_decimal && self.mode.signed && v == 1u128 << (self.mode.bits - 1) {
                    return Err(err("literal_too_large", at));
                }
                Ok(self.mode.wrap(v))
            }
            ITok::LParen => {
                let v = self.or()?;
                if self.peek() == Some(&ITok::RParen) {
                    self.i += 1;
                    Ok(v)
                } else {
                    Err(err("unbalanced", at))
                }
            }
            ITok::RParen => Err(err("unbalanced", at)),
            _ => Err(err("unexpected_token", at)),
        }
    }
}

/// The value of a programmer-mode evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntResult {
    /// The bit pattern, masked to the width.
    pub bits: u128,
    /// Some operation lost bits (the exact result did not fit the width).
    pub wrapped: bool,
}

/// Evaluate an integer expression.
pub fn eval_int(expr: &str, mode: IntMode) -> Result<IntResult, CalcError> {
    if !matches!(mode.bits, 8 | 16 | 32 | 64) {
        return Err(err("bad_width", 0));
    }
    if !matches!(mode.base, 2 | 8 | 10 | 16) {
        return Err(err("bad_base", 0));
    }
    let toks = ilex(expr, mode)?;
    if toks.is_empty() {
        return Err(err("empty", 0));
    }
    let end = expr.chars().count();
    let mut p = IParser {
        toks,
        i: 0,
        end,
        mode,
        depth: 0,
        wrapped: false,
    };
    let v = p.or()?;
    if p.i < p.toks.len() {
        let at = p.pos();
        return Err(match p.peek() {
            Some(ITok::RParen) => err("unbalanced", at),
            _ => err("unexpected_token", at),
        });
    }
    Ok(IntResult {
        bits: v,
        wrapped: p.wrapped,
    })
}

fn grouped(s: &str, size: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (k, c) in chars.iter().enumerate() {
        if k > 0 && (chars.len() - k) % size == 0 {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

/// A bit pattern in the four bases (`bin` zero-padded to the width and grouped by four).
pub fn int_views(bits: u128, mode: IntMode) -> Value {
    let width = mode.bits as usize;
    json!({
        "signed": mode.signed_of(bits).to_string(),
        "unsigned": bits.to_string(),
        "hex": format!("{bits:X}"),
        "oct": format!("{bits:o}"),
        "bin": grouped(&format!("{bits:0width$b}"), 4),
        "bits": mode.bits,
    })
}

/// The JSON answer of [`eval_int`]: `{ok, views:{signed,unsigned,hex,oct,bin}, text, wrapped}` or an error.
pub fn eval_int_json(expr: &str, bits: u32, signed: bool, base: u32) -> String {
    let mode = IntMode { bits, signed, base };
    match eval_int(expr, mode) {
        Ok(r) => {
            let views = int_views(r.bits, mode);
            let text = if signed {
                views["signed"].clone()
            } else {
                views["unsigned"].clone()
            };
            json!({ "ok": true, "views": views, "text": text, "wrapped": r.wrapped }).to_string()
        }
        Err(e) => error_json(&e),
    }
}

#[wasm_bindgen(js_name = programmerEval)]
pub fn programmer_eval(expr: &str, bits: u32, signed: bool, base: u32) -> String {
    eval_int_json(expr, bits, signed, base)
}

/// Re-reads `text` written in `from` and writes it in `to` (2, 8, 10 or 16), at `bits` wide.
pub fn convert_base(text: &str, from: u32, to: u32, bits: u32, signed: bool) -> Result<String, CalcError> {
    let mode = IntMode { bits, signed, base: from };
    let r = eval_int(text, mode)?;
    Ok(match to {
        2 => format!("{:b}", r.bits),
        8 => format!("{:o}", r.bits),
        16 => format!("{:X}", r.bits),
        10 if signed => mode.signed_of(r.bits).to_string(),
        10 => r.bits.to_string(),
        _ => return Err(err("bad_base", 0)),
    })
}

#[wasm_bindgen(js_name = programmerConvertBase)]
pub fn programmer_convert_base(text: &str, from: u32, to: u32, bits: u32, signed: bool) -> String {
    match convert_base(text, from, to, bits, signed) {
        Ok(t) => json!({ "ok": true, "text": t }).to_string(),
        Err(e) => error_json(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sci(expr: &str) -> String {
        match eval(
            expr,
            &Options {
                degrees: false,
                ans: Some(7.0),
            },
        ) {
            Ok(v) => format_number(v),
            Err(e) => format!("!{}", e.code),
        }
    }

    fn deg(expr: &str) -> String {
        match eval(
            expr,
            &Options {
                degrees: true,
                ans: None,
            },
        ) {
            Ok(v) => format_number(v),
            Err(e) => format!("!{}", e.code),
        }
    }

    fn int(expr: &str, bits: u32, signed: bool, base: u32) -> String {
        let m = IntMode { bits, signed, base };
        match eval_int(expr, m) {
            Ok(r) => {
                if signed {
                    m.signed_of(r.bits).to_string()
                } else {
                    r.bits.to_string()
                }
            }
            Err(e) => format!("!{}", e.code),
        }
    }

    /// Scientific cases: arithmetic, precedence, associativity, functions, constants, errors.
    #[test]
    fn scientific_cases() {
        let cases: &[(&str, &str)] = &[
            // arithmetic and precedence
            ("1+2", "3"),
            ("1 + 2 * 3", "7"),
            ("(1 + 2) * 3", "9"),
            ("2 * 3 + 4", "10"),
            ("10 - 4 - 3", "3"),
            ("100 / 10 / 5", "2"),
            ("2 ^ 3 ^ 2", "512"),
            ("(2 ^ 3) ^ 2", "64"),
            ("2 ** 10", "1024"),
            ("-2 ^ 2", "-4"),
            ("(-2) ^ 2", "4"),
            ("2 ^ -2", "0.25"),
            ("-3 + 5", "2"),
            ("--3", "3"),
            ("+4", "4"),
            ("3 - -3", "6"),
            ("2 * -3", "-6"),
            ("1 / 4", "0.25"),
            ("1 / 3", "0.333333333333"),
            ("2 / 3", "0.666666666667"),
            ("0.1 + 0.2", "0.3"),
            ("0.3 - 0.1", "0.2"),
            ("1.1 * 1.1", "1.21"),
            ("7 / 2", "3.5"),
            ("1e3", "1000"),
            ("1.5e3", "1500"),
            ("2.5E-3", "0.0025"),
            ("1e-7", "1e-7"),
            ("1e15", "1e15"),
            ("1e14", "100000000000000"),
            ("123456789 * 1000", "123456789000"),
            ("6.02e23", "6.02e23"),
            ("1.6e-19", "1.6e-19"),
            ("3e2 + 1", "301"),
            (".5 + .5", "1"),
            ("5.", "5"),
            ("1_000_000 + 1", "1000001"),
            ("1_0", "10"),
            ("2e", "5.43656365692"),
            // implicit multiplication
            ("2(3 + 4)", "14"),
            ("(1 + 2)(3 + 4)", "21"),
            ("2pi", "6.28318530718"),
            ("3 e", "8.15484548538"),
            ("2 3", "6"),
            // modulo and percent
            ("10 mod 3", "1"),
            ("10 % 3", "1"),
            ("-7 mod 3", "-1"),
            ("5.5 mod 2", "1.5"),
            ("50%", "0.5"),
            ("50% * 2", "1"),
            ("200 * 15%", "30"),
            ("200 + 10%", "200.1"),
            ("5 mod 0", "!divide_by_zero"),
            ("5 % 0", "!divide_by_zero"),
            // factorial
            ("5!", "120"),
            ("0!", "1"),
            ("10!", "3628800"),
            ("3!!", "720"),
            ("2 * 3!", "12"),
            ("(2+1)!", "6"),
            ("170!", "7.25741561531e306"),
            ("171!", "!domain"),
            ("-1!", "-1"),
            ("(-1)!", "!domain"),
            ("2.5!", "!domain"),
            ("fact(6)", "720"),
            // constants
            ("pi", "3.14159265359"),
            ("PI", "3.14159265359"),
            ("\u{03c0}", "3.14159265359"),
            ("e", "2.71828182846"),
            ("tau", "6.28318530718"),
            ("phi", "1.61803398875"),
            ("ans", "7"),
            ("ans * 2", "14"),
            // functions
            ("sqrt(16)", "4"),
            ("sqrt 16", "4"),
            ("\u{221a}9", "3"),
            ("\u{221a}(9 + 16)", "5"),
            ("sqrt(2)", "1.41421356237"),
            ("sqrt(-1)", "!domain"),
            ("cbrt(27)", "3"),
            ("cbrt(-8)", "-2"),
            ("abs(-5)", "5"),
            ("abs(5)", "5"),
            ("floor(2.7)", "2"),
            ("floor(-2.1)", "-3"),
            ("ceil(2.1)", "3"),
            ("ceil(-2.9)", "-2"),
            ("round(2.5)", "3"),
            ("round(-2.5)", "-3"),
            ("round(3.14159, 2)", "3.14"),
            ("round(1234, -2)", "1200"),
            ("trunc(-2.9)", "-2"),
            ("sign(-9)", "-1"),
            ("sign(0)", "0"),
            ("sign(4)", "1"),
            ("ln(e)", "1"),
            ("ln(1)", "0"),
            ("ln(0)", "!domain"),
            ("log(1000)", "3"),
            ("log10(100)", "2"),
            ("log2(8)", "3"),
            ("log(-1)", "!domain"),
            ("exp(0)", "1"),
            ("exp(1)", "2.71828182846"),
            ("pow(2, 8)", "256"),
            ("pow(0, -1)", "!divide_by_zero"),
            ("0 ^ -1", "!divide_by_zero"),
            ("root(27, 3)", "3"),
            ("root(-27, 3)", "-3"),
            ("root(-4, 2)", "!domain"),
            ("hypot(3, 4)", "5"),
            ("gcd(12, 18)", "6"),
            ("lcm(4, 6)", "12"),
            ("gcd(1.5, 3)", "!domain"),
            ("min(3, 1, 2)", "1"),
            ("max(3, 1, 2)", "3"),
            ("sum(1, 2, 3, 4)", "10"),
            ("avg(2, 4, 9)", "5"),
            ("max()", "!arity"),
            ("atan2(1, 1)", "0.785398163397"),
            ("sin(0)", "0"),
            ("sin(pi)", "0"),
            ("cos(pi)", "-1"),
            ("cos(0)", "1"),
            ("tan(0)", "0"),
            ("sin(pi / 2)", "1"),
            ("asin(1)", "1.57079632679"),
            ("acos(1)", "0"),
            ("atan(1)", "0.785398163397"),
            ("asin(2)", "!domain"),
            ("acos(-2)", "!domain"),
            ("sinh(0)", "0"),
            ("cosh(0)", "1"),
            ("tanh(0)", "0"),
            ("asinh(0)", "0"),
            ("acosh(1)", "0"),
            ("acosh(0.5)", "!domain"),
            ("atanh(0)", "0"),
            ("atanh(1)", "!domain"),
            ("deg(pi)", "180"),
            ("rad(180)", "3.14159265359"),
            ("sqrt(4, 5)", "!arity"),
            ("sqrt()", "!arity"),
            // nesting and spacing
            ("((((1))))", "1"),
            ("  1   +   1  ", "2"),
            ("2*(3+(4*(5-1)))", "38"),
            ("sqrt(abs(-16)) + 1", "5"),
            ("max(1, min(5, 3), 2)", "3"),
            ("\u{00d7}", "!unexpected_token"),
            ("6 \u{00d7} 7", "42"),
            ("8 \u{00f7} 2", "4"),
            ("5 \u{2212} 2", "3"),
            // errors
            ("", "!empty"),
            ("   ", "!empty"),
            ("1 / 0", "!divide_by_zero"),
            ("1 / (2 - 2)", "!divide_by_zero"),
            ("(1 + 2", "!unbalanced"),
            ("1 + 2)", "!unbalanced"),
            ("1 +", "!unexpected_end"),
            ("* 3", "!unexpected_token"),
            ("1 + * 2", "!unexpected_token"),
            ("3 $ 4", "!unexpected_char"),
            ("foo", "!unknown_name"),
            ("foo(1)", "!unknown_function"),
            ("sin", "!missing_argument"),
            ("1e400", "!overflow"),
            ("10 ^ 400", "!overflow"),
            ("()", "!unbalanced"),
            ("1,2", "!unexpected_token"),
        ];
        let mut bad = Vec::new();
        for (expr, want) in cases {
            let got = sci(expr);
            if got != *want {
                bad.push(format!("{expr:?}: got {got:?}, want {want:?}"));
            }
        }
        assert!(bad.is_empty(), "{} of {} failed:\n{}", bad.len(), cases.len(), bad.join("\n"));
        assert!(cases.len() >= 130, "{}", cases.len());
    }

    #[test]
    fn degrees_mode() {
        let cases: &[(&str, &str)] = &[
            ("sin(30)", "0.5"),
            ("sin(90)", "1"),
            ("sin(180)", "0"),
            ("cos(60)", "0.5"),
            ("cos(90)", "0"),
            ("cos(180)", "-1"),
            ("tan(45)", "1"),
            ("tan(90)", "!domain"),
            ("asin(1)", "90"),
            ("acos(0)", "90"),
            ("atan(1)", "45"),
            ("atan2(1, 1)", "45"),
            ("sin 30", "0.5"),
        ];
        for (e, w) in cases {
            assert_eq!(deg(e), *w, "{e}");
        }
    }

    #[test]
    fn no_previous_answer() {
        assert_eq!(deg("ans"), "!no_answer");
    }

    #[test]
    fn error_positions() {
        let e = eval("1 + $", &Options::default()).unwrap_err();
        assert_eq!((e.code, e.pos), ("unexpected_char", 4));
        let e = eval("2 * (3 + 4", &Options::default()).unwrap_err();
        assert_eq!((e.code, e.pos), ("unbalanced", 4));
        let e = eval("1 + foo", &Options::default()).unwrap_err();
        assert_eq!((e.code, e.pos, e.detail.as_str()), ("unknown_name", 4, "foo"));
        let e = eval("8 / 0", &Options::default()).unwrap_err();
        assert_eq!((e.code, e.pos), ("divide_by_zero", 2));
    }

    #[test]
    fn deeply_nested_input_is_refused_not_overflowed() {
        let deep = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
        assert_eq!(sci(&deep), "!too_deep");
        let minus = format!("{}1", "-".repeat(5000));
        assert_eq!(sci(&minus), "!too_deep");
        let funcs = format!("{}1", "sin ".repeat(5000));
        assert_eq!(sci(&funcs), "!too_deep");
        let ints = format!("{}1", "~".repeat(5000));
        assert_eq!(int(&ints, 32, true, 10), "!too_deep");
        let parens = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
        assert_eq!(int(&parens, 32, true, 10), "!too_deep");
    }

    #[test]
    fn formatting() {
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(1.0), "1");
        assert_eq!(format_number(-1.5), "-1.5");
        assert_eq!(format_number(1.0 / 3.0), "0.333333333333");
        assert_eq!(format_number(123_456_789.123_456_789), "123456789.123");
        assert_eq!(format_number(1e20), "1e20");
        assert_eq!(format_number(1.5e-9), "1.5e-9");
        assert_eq!(format_number(-2.5e30), "-2.5e30");
        assert_eq!(format_number(f64::NAN), "NaN");
    }

    #[test]
    fn json_shapes() {
        let v: Value = serde_json::from_str(&calc_eval("2+2", false, f64::NAN)).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["text"], "4");
        assert_eq!(v["value"], 4.0);
        let v: Value = serde_json::from_str(&calc_eval("2+", false, f64::NAN)).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"]["code"], "unexpected_end");
        let v: Value = serde_json::from_str(&calc_eval("ans+1", true, 4.0)).unwrap();
        assert_eq!(v["text"], "5");
    }

    /// Programmer cases: bases, bitwise, shifts, widths, two's complement, errors.
    #[test]
    fn programmer_cases() {
        // (expression, bits, signed, base, expected)
        let cases: &[(&str, u32, bool, u32, &str)] = &[
            ("1 + 2", 32, true, 10, "3"),
            ("0xFF", 32, true, 10, "255"),
            ("0xff", 32, true, 10, "255"),
            ("0b1010", 32, true, 10, "10"),
            ("0o17", 32, true, 10, "15"),
            ("0xFF + 1", 8, false, 10, "0"),
            ("0xFF + 1", 8, true, 10, "0"),
            ("127 + 1", 8, true, 10, "-128"),
            ("127 + 1", 8, false, 10, "128"),
            ("-128 - 1", 8, true, 10, "127"),
            ("-128", 8, true, 10, "-128"),
            ("-1", 8, true, 10, "-1"),
            ("-1", 8, false, 10, "255"),
            ("-1", 16, false, 10, "65535"),
            ("-1", 32, false, 10, "4294967295"),
            ("-1", 64, false, 10, "18446744073709551615"),
            ("-1", 64, true, 10, "-1"),
            ("0 - 1", 8, false, 10, "255"),
            ("200", 8, false, 10, "200"),
            ("200", 8, true, 10, "!literal_too_large"),
            ("128", 8, true, 10, "!literal_too_large"),
            ("256", 8, false, 10, "!literal_too_large"),
            ("0x100", 8, false, 10, "!literal_too_large"),
            ("0xFF", 8, true, 10, "-1"),
            ("0x80", 8, true, 10, "-128"),
            ("0x7F", 8, true, 10, "127"),
            ("0xFFFF", 16, true, 10, "-1"),
            ("0xFFFFFFFF", 32, true, 10, "-1"),
            ("0xFFFFFFFFFFFFFFFF", 64, true, 10, "-1"),
            ("0xFFFFFFFFFFFFFFFF", 64, false, 10, "18446744073709551615"),
            ("0x1FFFFFFFFFFFFFFFF", 64, false, 10, "!literal_too_large"),
            ("9223372036854775807 + 1", 64, true, 10, "-9223372036854775808"),
            ("-9223372036854775808", 64, true, 10, "-9223372036854775808"),
            ("9223372036854775808", 64, true, 10, "!literal_too_large"),
            ("18446744073709551616", 64, false, 10, "!literal_too_large"),
            ("5 & 3", 32, true, 10, "1"),
            ("5 | 3", 32, true, 10, "7"),
            ("5 ^ 3", 32, true, 10, "6"),
            ("~0", 8, false, 10, "255"),
            ("~0", 8, true, 10, "-1"),
            ("~5", 8, true, 10, "-6"),
            ("~5", 8, false, 10, "250"),
            ("~0xF0", 8, false, 10, "15"),
            ("0xF0 & 0x3C", 8, false, 10, "48"),
            ("0xF0 | 0x0F", 8, false, 10, "255"),
            ("0xFF ^ 0x0F", 8, false, 10, "240"),
            ("1 << 4", 32, true, 10, "16"),
            ("1 << 7", 8, true, 10, "-128"),
            ("1 << 7", 8, false, 10, "128"),
            ("1 << 8", 8, false, 10, "!shift_range"),
            ("1 << 31", 32, false, 10, "2147483648"),
            ("1 << 63", 64, false, 10, "9223372036854775808"),
            ("1 << 64", 64, false, 10, "!shift_range"),
            ("256 >> 4", 32, true, 10, "16"),
            ("-16 >> 2", 8, true, 10, "-4"),
            ("0xF0 >> 4", 8, false, 10, "15"),
            ("0xF0 >> 4", 8, true, 10, "-1"),
            ("0xF0 >>> 4", 8, true, 10, "15"),
            ("-1 >>> 1", 8, true, 10, "127"),
            ("1 << -1", 8, true, 10, "!shift_range"),
            ("2 + 3 * 4", 32, true, 10, "14"),
            ("(2 + 3) * 4", 32, true, 10, "20"),
            ("1 | 2 & 3", 32, true, 10, "3"),
            ("1 ^ 3 & 2", 32, true, 10, "3"),
            ("1 << 2 + 1", 32, true, 10, "8"),
            ("7 / 2", 32, true, 10, "3"),
            ("-7 / 2", 32, true, 10, "-3"),
            ("7 % 3", 32, true, 10, "1"),
            ("-7 % 3", 32, true, 10, "-1"),
            ("7 / 0", 32, true, 10, "!divide_by_zero"),
            ("7 % 0", 32, true, 10, "!divide_by_zero"),
            ("100 * 100", 8, false, 10, "16"),
            ("100 * 100", 16, false, 10, "10000"),
            ("65536 * 65536", 32, false, 10, "0"),
            ("-(5)", 8, true, 10, "-5"),
            ("-(-5)", 8, true, 10, "5"),
            ("FF", 32, true, 16, "255"),
            ("ff + 1", 32, true, 16, "256"),
            ("1010", 32, true, 2, "10"),
            ("17", 32, true, 8, "15"),
            ("2", 32, true, 2, "!bad_digit"),
            ("9", 32, true, 8, "!bad_digit"),
            ("G", 32, true, 16, "!bad_digit"),
            ("12a", 32, true, 10, "!bad_digit"),
            ("0xZ", 32, true, 10, "!bad_digit"),
            ("0x", 32, true, 10, "!bad_number"),
            ("", 32, true, 10, "!empty"),
            ("(1 + 2", 32, true, 10, "!unbalanced"),
            ("1 +", 32, true, 10, "!unexpected_end"),
            ("1.5", 32, true, 10, "!unexpected_char"),
            ("1 $ 2", 32, true, 10, "!unexpected_char"),
            ("1 2", 32, true, 10, "!unexpected_token"),
            ("1_000", 32, true, 10, "1000"),
            ("0b1111_0000", 8, false, 10, "240"),
            ("1", 12, true, 10, "!bad_width"),
            ("1", 32, true, 3, "!bad_base"),
        ];
        let mut bad = Vec::new();
        for (e, bits, signed, base, want) in cases {
            let got = int(e, *bits, *signed, *base);
            if got != *want {
                bad.push(format!("{e:?} ({bits},{signed},{base}): got {got:?}, want {want:?}"));
            }
        }
        assert!(bad.is_empty(), "{} of {} failed:\n{}", bad.len(), cases.len(), bad.join("\n"));
        assert!(cases.len() >= 90, "{}", cases.len());
    }

    #[test]
    fn programmer_wrap_flag() {
        let m = IntMode {
            bits: 8,
            signed: true,
            base: 10,
        };
        assert!(eval_int("127 + 1", m).unwrap().wrapped);
        assert!(!eval_int("100 + 20", m).unwrap().wrapped);
        assert!(eval_int("100 * 2", m).unwrap().wrapped);
        assert!(!eval_int("-128", m).unwrap().wrapped);
        let u = IntMode {
            bits: 8,
            signed: false,
            base: 10,
        };
        assert!(eval_int("0 - 1", u).unwrap().wrapped);
        assert!(eval_int("255 << 1", u).unwrap().wrapped);
        assert!(!eval_int("128 >> 1", u).unwrap().wrapped);
    }

    #[test]
    fn programmer_views() {
        let v: Value = serde_json::from_str(&programmer_eval("0xFF", 8, true, 10)).unwrap();
        assert_eq!(v["views"]["signed"], "-1");
        assert_eq!(v["views"]["unsigned"], "255");
        assert_eq!(v["views"]["hex"], "FF");
        assert_eq!(v["views"]["oct"], "377");
        assert_eq!(v["views"]["bin"], "1111 1111");
        assert_eq!(v["text"], "-1");
        let v: Value = serde_json::from_str(&programmer_eval("5", 16, false, 10)).unwrap();
        assert_eq!(v["views"]["bin"], "0000 0000 0000 0101");
        assert_eq!(v["text"], "5");
    }

    #[test]
    fn base_conversion() {
        assert_eq!(convert_base("255", 10, 16, 32, true).unwrap(), "FF");
        assert_eq!(convert_base("FF", 16, 2, 32, true).unwrap(), "11111111");
        assert_eq!(convert_base("11111111", 2, 10, 8, false).unwrap(), "255");
        assert_eq!(convert_base("11111111", 2, 10, 8, true).unwrap(), "-1");
        assert_eq!(convert_base("377", 8, 16, 16, false).unwrap(), "FF");
        assert_eq!(convert_base("12", 10, 3, 8, false).unwrap_err().code, "bad_base");
        assert_eq!(convert_base("2", 2, 10, 8, false).unwrap_err().code, "bad_digit");
    }
}
