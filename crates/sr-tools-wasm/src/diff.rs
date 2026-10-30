//! A longest-common-subsequence diff (UTL-DEV-001): lines, words or characters, for the text and JSON
//! diff tool. Ours rather than a crate: the algorithm is thirty lines and the size of the wasm matters.
//!
//! Common prefix and suffix are trimmed first (most edits are local), then the middle goes through the
//! classic dynamic-programming table. The table is bounded ([`MAX_CELLS`]); a larger middle is refused
//! with [`DiffError::TooLarge`] so the tool can say so instead of freezing the tab.

use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

/// Most cells in the LCS table (`(n + 1) * (m + 1)`), about 10 MB.
pub const MAX_CELLS: usize = 2_500_000;
/// Most input bytes per side.
pub const MAX_BYTES: usize = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Lines,
    Words,
    Chars,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffError {
    TooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Equal,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Op {
    pub kind: Kind,
    pub text: String,
}

/// Split `s` into tokens that concatenate back to `s`: lines keep their newline, words alternate with the
/// whitespace between them, characters are single characters.
pub fn tokens(s: &str, mode: Mode) -> Vec<&str> {
    match mode {
        Mode::Lines => s.split_inclusive('\n').collect(),
        Mode::Chars => s
            .char_indices()
            .map(|(i, c)| &s[i..i + c.len_utf8()])
            .collect(),
        Mode::Words => {
            let mut out = Vec::new();
            let mut start = 0;
            let mut prev: Option<bool> = None;
            for (i, c) in s.char_indices() {
                let ws = c.is_whitespace();
                if prev.is_some_and(|p| p != ws) {
                    out.push(&s[start..i]);
                    start = i;
                }
                prev = Some(ws);
            }
            if start < s.len() {
                out.push(&s[start..]);
            }
            out
        }
    }
}

/// The diff of two token lists as a sequence of equal / add / delete operations (one per token).
pub fn diff_tokens(a: &[&str], b: &[&str]) -> Result<Vec<Op>, DiffError> {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..]
        .iter()
        .rev()
        .zip(b[pre..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let am = &a[pre..a.len() - suf];
    let bm = &b[pre..b.len() - suf];
    let (n, m) = (am.len(), bm.len());
    if (n + 1).saturating_mul(m + 1) > MAX_CELLS {
        return Err(DiffError::TooLarge);
    }
    let w = m + 1;
    // lcs[i * w + j] = LCS length of am[i..] and bm[j..].
    let mut lcs = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * w + j] = if am[i] == bm[j] {
                lcs[(i + 1) * w + j + 1] + 1
            } else {
                lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
            };
        }
    }
    let op = |kind, t: &str| Op {
        kind,
        text: t.to_owned(),
    };
    let mut out: Vec<Op> = a[..pre].iter().map(|t| op(Kind::Equal, t)).collect();
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && am[i] == bm[j] {
            out.push(op(Kind::Equal, am[i]));
            i += 1;
            j += 1;
        } else if i < n && (j == m || lcs[(i + 1) * w + j] >= lcs[i * w + j + 1]) {
            // Deletions come before additions, as in a unified diff.
            out.push(op(Kind::Del, am[i]));
            i += 1;
        } else {
            out.push(op(Kind::Add, bm[j]));
            j += 1;
        }
    }
    out.extend(a[a.len() - suf..].iter().map(|t| op(Kind::Equal, t)));
    Ok(out)
}

/// Diff two strings.
pub fn diff_text(a: &str, b: &str, mode: Mode) -> Result<Vec<Op>, DiffError> {
    if a.len() > MAX_BYTES || b.len() > MAX_BYTES {
        return Err(DiffError::TooLarge);
    }
    diff_tokens(&tokens(a, mode), &tokens(b, mode))
}

/// Merge neighbouring operations of one kind, so a word diff reads as runs.
pub fn coalesce(ops: Vec<Op>) -> Vec<Op> {
    let mut out: Vec<Op> = Vec::new();
    for o in ops {
        match out.last_mut() {
            Some(last) if last.kind == o.kind => last.text.push_str(&o.text),
            _ => out.push(o),
        }
    }
    out
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Equal => "eq",
        Kind::Add => "add",
        Kind::Del => "del",
    }
}

fn ops_json(ops: &[Op]) -> Value {
    Value::Array(
        ops.iter()
            .map(|o| json!({ "op": kind_name(o.kind), "text": o.text }))
            .collect(),
    )
}

/// Counts of added and removed tokens.
pub fn stats(ops: &[Op]) -> (usize, usize) {
    let add = ops.iter().filter(|o| o.kind == Kind::Add).count();
    let del = ops.iter().filter(|o| o.kind == Kind::Del).count();
    (add, del)
}

/// `mode` is `"lines"`, `"words"` or `"chars"`. Returns `{ops: [{op, text}], added, removed}` as JSON
/// (words and characters are coalesced into runs). Throws `too-large` or `bad-mode`.
#[wasm_bindgen(js_name = diffText)]
pub fn diff_text_js(a: &str, b: &str, mode: &str) -> Result<String, JsError> {
    let mode = match mode {
        "lines" => Mode::Lines,
        "words" => Mode::Words,
        "chars" => Mode::Chars,
        _ => return Err(JsError::new("bad-mode")),
    };
    let ops = diff_text(a, b, mode).map_err(|_| JsError::new("too-large"))?;
    let (added, removed) = stats(&ops);
    let ops = if mode == Mode::Lines {
        ops
    } else {
        coalesce(ops)
    };
    Ok(json!({ "ops": ops_json(&ops), "added": added, "removed": removed }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(ops: &[Op]) -> (String, String) {
        let mut a = String::new();
        let mut b = String::new();
        for o in ops {
            match o.kind {
                Kind::Equal => {
                    a.push_str(&o.text);
                    b.push_str(&o.text);
                }
                Kind::Add => b.push_str(&o.text),
                Kind::Del => a.push_str(&o.text),
            }
        }
        (a, b)
    }

    #[test]
    fn identical_text_has_only_equal_ops() {
        let ops = diff_text("a\nb\nc\n", "a\nb\nc\n", Mode::Lines).unwrap();
        assert!(ops.iter().all(|o| o.kind == Kind::Equal));
        assert_eq!(ops.len(), 3);
        assert_eq!(stats(&ops), (0, 0));
    }

    #[test]
    fn an_insert_a_delete_and_a_change() {
        let ops = diff_text("a\nb\nc\n", "a\nx\nc\nd\n", Mode::Lines).unwrap();
        assert_eq!(stats(&ops), (2, 1));
        let (a, b) = apply(&ops);
        assert_eq!((a.as_str(), b.as_str()), ("a\nb\nc\n", "a\nx\nc\nd\n"));
    }

    #[test]
    fn empty_sides() {
        assert!(diff_text("", "", Mode::Lines).unwrap().is_empty());
        let ops = diff_text("", "a\nb", Mode::Lines).unwrap();
        assert_eq!(stats(&ops), (2, 0));
        let ops = diff_text("a\nb", "", Mode::Lines).unwrap();
        assert_eq!(stats(&ops), (0, 2));
    }

    #[test]
    fn the_diff_is_minimal_for_a_known_case() {
        // LCS of ABCBDAB and BDCABA has length 4: 3 deletions and 2 additions.
        let a: Vec<&str> = "ABCBDAB".split("").filter(|s| !s.is_empty()).collect();
        let b: Vec<&str> = "BDCABA".split("").filter(|s| !s.is_empty()).collect();
        let ops = diff_tokens(&a, &b).unwrap();
        assert_eq!(stats(&ops), (2, 3));
    }

    #[test]
    fn words_and_chars_round_trip() {
        let a = "the quick brown fox";
        let b = "the slow brown dog";
        for mode in [Mode::Words, Mode::Chars, Mode::Lines] {
            let ops = diff_text(a, b, mode).unwrap();
            let (ra, rb) = apply(&ops);
            assert_eq!((ra.as_str(), rb.as_str()), (a, b), "{mode:?}");
        }
        let ops = coalesce(diff_text(a, b, Mode::Words).unwrap());
        assert!(ops.iter().any(|o| o.kind == Kind::Del && o.text == "quick"));
        assert!(ops.iter().any(|o| o.kind == Kind::Add && o.text == "slow"));
    }

    #[test]
    fn unicode_characters_stay_whole() {
        let ops = diff_text("caf\u{e9} \u{1F600}", "caf\u{e9}s \u{1F600}", Mode::Chars).unwrap();
        let (a, b) = apply(&ops);
        assert_eq!(a, "caf\u{e9} \u{1F600}");
        assert_eq!(b, "caf\u{e9}s \u{1F600}");
        assert_eq!(stats(&ops), (1, 0));
    }

    #[test]
    fn last_line_without_newline_differs_from_one_with() {
        let ops = diff_text("a\nb", "a\nb\n", Mode::Lines).unwrap();
        assert_eq!(stats(&ops), (1, 1));
    }

    #[test]
    fn huge_middles_are_refused() {
        let a = (0..2000).map(|i| format!("a{i}\n")).collect::<String>();
        let b = (0..2000).map(|i| format!("b{i}\n")).collect::<String>();
        assert_eq!(diff_text(&a, &b, Mode::Lines), Err(DiffError::TooLarge));
        // But a long identical prefix and suffix are free.
        let common = "x\n".repeat(100_000);
        let ops = diff_text(
            &format!("{common}1\n{common}"),
            &format!("{common}2\n{common}"),
            Mode::Lines,
        )
        .unwrap();
        assert_eq!(stats(&ops), (1, 1));
        assert_eq!(
            diff_text(&"x".repeat(MAX_BYTES + 1), "", Mode::Chars),
            Err(DiffError::TooLarge)
        );
    }

    #[test]
    fn json_shape() {
        let ops = diff_text("a\n", "b\n", Mode::Lines).unwrap();
        let v = ops_json(&ops);
        assert_eq!(v[0]["op"], "del");
        assert_eq!(v[1]["op"], "add");
    }
}
