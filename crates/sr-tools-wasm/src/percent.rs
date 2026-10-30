//! Percentage phrases (UTL-CALC-001): "15% of 80", "80 + 15%", "20% off 50", "12 is what % of 60",
//! "increase 80 by 15%", "from 50 to 65".
//!
//! `parse` reads the phrase into one of six kinds with its two operands and `compute` does the arithmetic, so the
//! browser can offer both a phrase box and a form (`percentCompute`). Operands are calculator expressions
//! (`15% of 20 * 4` works), thousands commas and currency signs are ignored, and `percent`, `percentage` and
//! `pct` mean `%`.

use crate::calc::{CalcError, Options, err, error_json, eval, format_number};
use serde_json::json;
use wasm_bindgen::prelude::*;

/// What the phrase asks for. `a` and `b` are the operands in the order the doc of each kind gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `a`% of `b`.
    Of,
    /// `a` is what percent of `b` (the answer is a percent).
    WhatPercent,
    /// `a` is `b`% of what (the answer is the whole).
    OfWhat,
    /// `a` plus `b`%.
    Add,
    /// `a` minus `b`%.
    Sub,
    /// The percent change from `a` to `b` (the answer is a percent).
    Change,
}

impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Kind::Of => "of",
            Kind::WhatPercent => "whatpercent",
            Kind::OfWhat => "ofwhat",
            Kind::Add => "add",
            Kind::Sub => "sub",
            Kind::Change => "change",
        }
    }

    pub fn from_id(id: &str) -> Option<Kind> {
        Some(match id {
            "of" => Kind::Of,
            "whatpercent" => Kind::WhatPercent,
            "ofwhat" => Kind::OfWhat,
            "add" => Kind::Add,
            "sub" => Kind::Sub,
            "change" => Kind::Change,
            _ => return None,
        })
    }

    /// The answer is itself a percent.
    pub fn is_percent(self) -> bool {
        matches!(self, Kind::WhatPercent | Kind::Change)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Answer {
    pub kind: Kind,
    pub a: f64,
    pub b: f64,
    pub value: f64,
}

pub fn compute(kind: Kind, a: f64, b: f64) -> Result<Answer, CalcError> {
    if !a.is_finite() || !b.is_finite() {
        return Err(err("overflow", 0));
    }
    let value = match kind {
        Kind::Of => a * b / 100.0,
        Kind::WhatPercent => {
            if b == 0.0 {
                return Err(err("divide_by_zero", 0));
            }
            a / b * 100.0
        }
        Kind::OfWhat => {
            if b == 0.0 {
                return Err(err("divide_by_zero", 0));
            }
            a / (b / 100.0)
        }
        Kind::Add => a * (1.0 + b / 100.0),
        Kind::Sub => a * (1.0 - b / 100.0),
        Kind::Change => {
            if a == 0.0 {
                return Err(err("divide_by_zero", 0));
            }
            (b - a) / a.abs() * 100.0
        }
    };
    if !value.is_finite() {
        return Err(err("overflow", 0));
    }
    Ok(Answer { kind, a, b, value })
}

fn is_thousands_comma(chars: &[char], i: usize) -> bool {
    chars[i] == ','
        && i > 0
        && chars[i - 1].is_ascii_digit()
        && chars
            .get(i + 1..i + 4)
            .is_some_and(|d| d.iter().all(char::is_ascii_digit))
        && !chars.get(i + 4).is_some_and(char::is_ascii_digit)
}

fn normalise(input: &str) -> String {
    let lower = input.trim().to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut s = String::new();
    for (i, c) in chars.iter().enumerate() {
        match c {
            '$' | '\u{20ac}' | '\u{00a3}' | '\u{00a5}' => {}
            ',' if is_thousands_comma(&chars, i) => {}
            '\u{2212}' => s.push('-'),
            '\u{00d7}' => s.push('*'),
            _ => s.push(*c),
        }
    }
    let mut s = s.trim_end_matches(['?', '.', '!']).to_owned();
    for (from, to) in [
        ("percentage", "%"),
        ("per cent", "%"),
        ("percent", "%"),
        ("pct", "%"),
        ("what%", "what %"),
    ] {
        s = s.replace(from, to);
    }
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut s = format!(" {s} ");
    for (from, to) in [
        (" plus ", " + "),
        (" minus ", " - "),
        (" less ", " - "),
        (" times ", " * "),
    ] {
        s = s.replace(from, to);
    }
    s.trim().to_owned()
}

/// The text before a trailing `%` (spaces allowed before it), or `None`.
fn strip_pct(s: &str) -> Option<&str> {
    s.trim().strip_suffix('%').map(str::trim)
}

fn operand(s: &str) -> Result<f64, CalcError> {
    let t = s.trim();
    if t.is_empty() {
        return Err(err("no_phrase", 0));
    }
    eval(t, &Options::default())
}

fn pct_operand(s: &str) -> Result<f64, CalcError> {
    operand(strip_pct(s).ok_or_else(|| err("no_phrase", 0))?)
}

/// The position of the last binary `+` or `-` outside parentheses.
fn last_additive(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut found = None;
    let mut prev: Option<char> = None; // previous non-space character
    let mut prev2: Option<char> = None;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '+' | '-' if depth == 0 => {
                let after_operator = matches!(prev, None | Some('+' | '-' | '*' | '/' | '^' | '('));
                let exponent_sign =
                    prev == Some('e') && prev2.is_some_and(|q| q.is_ascii_digit() || q == '.');
                let unary = after_operator || exponent_sign;
                if !unary {
                    found = Some(i);
                }
            }
            _ => {}
        }
        if !c.is_whitespace() {
            prev2 = prev;
            prev = Some(c);
        }
    }
    found
}

fn last_multiplicative(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut found = None;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            '*' if depth == 0 => found = Some(i),
            _ => {}
        }
    }
    found
}

const TRAILING_WORDS: &[&str] = &[
    "tip", "tax", "vat", "gst", "markup", "gratuity", "fee", "interest", "discount", "increase",
    "decrease",
];

/// Reads a phrase.
pub fn parse(text: &str) -> Result<Answer, CalcError> {
    let mut s = normalise(text);
    if s.is_empty() {
        return Err(err("empty", 0));
    }
    for prefix in [
        "what is ",
        "what's ",
        "whats ",
        "calculate ",
        "calc ",
        "how much is ",
        "find ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim().to_owned();
            break;
        }
    }
    let ab = |kind: Kind, a: &str, b: &str, pct_b: bool| -> Result<Answer, CalcError> {
        let a = operand(a)?;
        let b = if pct_b { pct_operand(b)? } else { operand(b)? };
        compute(kind, a, b)
    };

    // increase 80 by 15% / decrease 80 by 15%
    for (verbs, kind) in [
        (["increase ", "raise ", "add "], Kind::Add),
        (["decrease ", "reduce ", "lower "], Kind::Sub),
    ] {
        for v in verbs {
            if let Some(rest) = s.strip_prefix(v)
                && let Some((a, b)) = rest.rsplit_once(" by ")
            {
                return ab(kind, a, b, true);
            }
        }
    }
    if let Some(rest) = s.strip_prefix("discount ")
        && let Some((a, b)) = rest.rsplit_once(" by ")
    {
        return ab(Kind::Sub, a, b, true);
    }

    // change from 50 to 65 / percent change from 50 to 65 / % change 50 to 65
    for prefix in [
        "% change from ",
        "% change in ",
        "% change ",
        "change from ",
        "change ",
        "difference from ",
        "from ",
    ] {
        if let Some(rest) = s.strip_prefix(prefix)
            && let Some((a, b)) = rest.split_once(" to ")
        {
            return ab(Kind::Change, a, b, false);
        }
    }

    // 12 is 20% of what / 20% of what is 12
    if let Some(rest) = s.strip_suffix(" of what")
        && let Some((a, b)) = rest.split_once(" is ")
    {
        return ab(Kind::OfWhat, a, b, true);
    }
    if let Some((p, x)) = s.split_once(" of what is ") {
        return ab(Kind::OfWhat, x, p, true);
    }

    // 12 is what % of 60 / what % is 12 of 60 / 12 as a % of 60 / what % of 60 is 12
    for sep in [
        " is what % of ",
        " as a % of ",
        " as % of ",
        " is what % from ",
    ] {
        if let Some((a, b)) = s.split_once(sep) {
            return ab(Kind::WhatPercent, a, b, false);
        }
    }
    if let Some(rest) = s.strip_prefix("what % is ")
        && let Some((a, b)) = rest.split_once(" of ")
    {
        return ab(Kind::WhatPercent, a, b, false);
    }
    if let Some(rest) = s.strip_prefix("what % of ")
        && let Some((b, a)) = rest.split_once(" is ")
    {
        return ab(Kind::WhatPercent, a, b, false);
    }

    // 20% off 50
    if let Some((p, a)) = s.split_once(" off ")
        && strip_pct(p).is_some()
    {
        return ab(Kind::Sub, a, p, true);
    }
    // 15% of 80
    if let Some((p, b)) = s.split_once(" of ")
        && strip_pct(p).is_some()
    {
        let a = pct_operand(p)?;
        return compute(Kind::Of, a, operand(b)?);
    }

    // 80 + 15% / 80 - 15% / 80 * 15% (a trailing word such as "tip" is allowed)
    let mut body = s.clone();
    if let Some(at) = body.rfind('%') {
        let tail = body[at + 1..].trim();
        if !tail.is_empty() && TRAILING_WORDS.contains(&tail) {
            body.truncate(at + 1);
        }
    }
    if let Some(inner) = strip_pct(&body) {
        if let Some(at) = last_additive(inner) {
            let kind = if inner[at..].starts_with('+') {
                Kind::Add
            } else {
                Kind::Sub
            };
            return ab(kind, &inner[..at], &inner[at + 1..], false);
        }
        if let Some(at) = last_multiplicative(inner) {
            let p = operand(&inner[at + 1..])?;
            return compute(Kind::Of, p, operand(&inner[..at])?);
        }
    }
    Err(err("no_phrase", 0))
}

fn answer_json(a: &Answer) -> String {
    json!({
        "ok": true,
        "kind": a.kind.id(),
        "a": a.a,
        "b": a.b,
        "value": a.value,
        "text": format_number(a.value),
        "percent": a.kind.is_percent(),
    })
    .to_string()
}

/// A phrase such as "15% of 80": `{ok, kind, a, b, value, text, percent}` or an error.
#[wasm_bindgen(js_name = percentPhrase)]
pub fn percent_phrase(text: &str) -> String {
    match parse(text) {
        Ok(a) => answer_json(&a),
        Err(e) => error_json(&e),
    }
}

/// The form: `kind` is one of of, whatpercent, ofwhat, add, sub, change.
#[wasm_bindgen(js_name = percentCompute)]
pub fn percent_compute(kind: &str, a: f64, b: f64) -> String {
    let Some(kind) = Kind::from_id(kind) else {
        return error_json(&err("no_phrase", 0));
    };
    match compute(kind, a, b) {
        Ok(a) => answer_json(&a),
        Err(e) => error_json(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> String {
        match parse(text) {
            Ok(a) => format!("{}:{}", a.kind.id(), format_number(a.value)),
            Err(e) => format!("!{}", e.code),
        }
    }

    #[test]
    fn phrases() {
        let cases: &[(&str, &str)] = &[
            // percent of
            ("15% of 80", "of:12"),
            ("15 % of 80", "of:12"),
            ("what is 15% of 80", "of:12"),
            ("What's 15% of 80?", "of:12"),
            ("15 percent of 80", "of:12"),
            ("15 percentage of 80", "of:12"),
            ("15pct of 80", "of:12"),
            ("200% of 50", "of:100"),
            ("0.5% of 1000", "of:5"),
            ("15% of 20 * 4", "of:12"),
            ("15% of (50 + 30)", "of:12"),
            ("10% of $1,000", "of:100"),
            ("10% of 1,234.50", "of:123.45"),
            ("7.5% of 200", "of:15"),
            ("100% of 7", "of:7"),
            ("0% of 7", "of:0"),
            ("15% of -80", "of:-12"),
            ("80 * 15%", "of:12"),
            ("80 times 15%", "of:12"),
            // add and subtract
            ("80 + 15%", "add:92"),
            ("80 plus 15%", "add:92"),
            ("80+15%", "add:92"),
            ("80 - 15%", "sub:68"),
            ("80-15%", "sub:68"),
            ("80 minus 15 percent", "sub:68"),
            ("80 less 15%", "sub:68"),
            ("$80 + 15% tip", "add:92"),
            ("50 + 8.25% tax", "add:54.125"),
            ("100 - 20% discount", "sub:80"),
            ("(100 + 20) - 10%", "sub:108"),
            ("100 + (20 - 5)%", "add:115"),
            ("1e3 + 10%", "add:1100"),
            ("1e-3 + 100%", "add:0.002"),
            ("-50 + 10%", "add:-55"),
            ("100 + 0%", "add:100"),
            // off
            ("20% off 50", "sub:40"),
            ("20% off $50", "sub:40"),
            ("30 percent off 19.99", "sub:13.993"),
            // verbs
            ("increase 80 by 15%", "add:92"),
            ("raise 80 by 25%", "add:100"),
            ("decrease 80 by 15%", "sub:68"),
            ("reduce 200 by 50 percent", "sub:100"),
            ("discount 50 by 20%", "sub:40"),
            // what percent
            ("12 is what % of 60", "whatpercent:20"),
            ("12 is what percent of 60", "whatpercent:20"),
            ("what % is 12 of 60", "whatpercent:20"),
            ("what percent is 12 of 60?", "whatpercent:20"),
            ("12 as a % of 60", "whatpercent:20"),
            ("12 as a percent of 60", "whatpercent:20"),
            ("what % of 60 is 12", "whatpercent:20"),
            ("1 is what % of 3", "whatpercent:33.3333333333"),
            ("90 is what % of 60", "whatpercent:150"),
            ("0 is what % of 60", "whatpercent:0"),
            // of what
            ("12 is 20% of what", "ofwhat:60"),
            ("20% of what is 12", "ofwhat:60"),
            ("what is 20% of what is 12", "ofwhat:60"),
            ("25 is 50 percent of what", "ofwhat:50"),
            // change
            ("from 50 to 65", "change:30"),
            ("change from 50 to 65", "change:30"),
            ("percent change from 50 to 65", "change:30"),
            ("% change from 80 to 60", "change:-25"),
            ("percentage change from 100 to 100", "change:0"),
            ("difference from 4 to 5", "change:25"),
            ("from -50 to -25", "change:50"),
            // errors
            ("", "!empty"),
            ("hello world", "!no_phrase"),
            ("15%", "!no_phrase"),
            ("12 is what % of 0", "!divide_by_zero"),
            ("from 0 to 5", "!divide_by_zero"),
            ("12 is 0% of what", "!divide_by_zero"),
            ("what is 15% of", "!no_phrase"),
            ("15% of foo", "!unknown_name"),
            ("increase 80 by", "!no_phrase"),
            ("80 + %", "!no_phrase"),
        ];
        let mut bad = Vec::new();
        for (q, want) in cases {
            let got = run(q);
            if got != *want {
                bad.push(format!("{q:?}: got {got}, want {want}"));
            }
        }
        assert!(
            bad.is_empty(),
            "{} of {}:\n{}",
            bad.len(),
            cases.len(),
            bad.join("\n")
        );
    }

    #[test]
    fn compute_form() {
        let v: serde_json::Value =
            serde_json::from_str(&percent_compute("of", 15.0, 80.0)).unwrap();
        assert_eq!(v["text"], "12");
        assert_eq!(v["percent"], false);
        let v: serde_json::Value =
            serde_json::from_str(&percent_compute("whatpercent", 12.0, 60.0)).unwrap();
        assert_eq!(v["text"], "20");
        assert_eq!(v["percent"], true);
        let v: serde_json::Value =
            serde_json::from_str(&percent_compute("change", 0.0, 5.0)).unwrap();
        assert_eq!(v["error"]["code"], "divide_by_zero");
        let v: serde_json::Value =
            serde_json::from_str(&percent_compute("nope", 1.0, 2.0)).unwrap();
        assert_eq!(v["error"]["code"], "no_phrase");
        let v: serde_json::Value =
            serde_json::from_str(&percent_compute("add", f64::INFINITY, 2.0)).unwrap();
        assert_eq!(v["error"]["code"], "overflow");
    }

    #[test]
    fn thousands_commas_only_in_groups_of_three() {
        assert_eq!(normalise("1,000"), "1000");
        assert_eq!(normalise("1,234,567"), "1234567");
        assert_eq!(normalise("max(1,2)"), "max(1,2)");
        assert_eq!(normalise("1,2345"), "1,2345");
    }

    #[test]
    fn json_shape() {
        let v: serde_json::Value = serde_json::from_str(&percent_phrase("15% of 80")).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["kind"], "of");
        assert_eq!(v["a"], 15.0);
        assert_eq!(v["b"], 80.0);
        assert_eq!(v["value"], 12.0);
    }
}
