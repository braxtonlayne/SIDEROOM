//! Text normalisation for the search: lowercase, fold diacritics, strip punctuation, and a small
//! English suffix stripper (no stemming library). `stem` is only used by the BM25 stage.

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// Longest query the engine looks at, in characters. Longer input is cut: the fuzzy stage is
/// quadratic in the length and the drawer's input never needs more.
pub const MAX_QUERY_CHARS: usize = 200;

/// Words that carry no task meaning; dropped before BM25 (on both sides).
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "any", "are", "as", "at", "be", "by", "can", "do", "for", "from", "how", "i",
    "in", "into", "is", "it", "me", "my", "of", "on", "or", "out", "some", "that", "the", "this",
    "to", "up", "want", "what", "with", "you", "your",
];

/// Lowercase, NFD-decompose, drop combining marks, and turn everything that is not a letter or
/// digit into a single space. Leading and trailing spaces are trimmed.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_space = false;
    for c in s.nfd().filter(|c| !is_combining_mark(*c)) {
        if c.is_alphanumeric() {
            if pending_space && !out.is_empty() {
                out.push(' ');
            }
            pending_space = false;
            out.extend(c.to_lowercase());
        } else {
            pending_space = true;
        }
    }
    out
}

/// The normalised words of `s`.
pub fn words(s: &str) -> Vec<String> {
    normalize(s)
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A 20-line English suffix stripper: plural, `-ing`, `-ed`, `-er`, `-ly`. It is not linguistically
/// correct (it does not need to be): it only has to map "converting", "converts" and "converted" to
/// one key, on both the corpus and the query.
pub fn stem(w: &str) -> String {
    let n = w.chars().count();
    let strip = |suffix: &str, min_rest: usize| -> Option<String> {
        let rest = w.strip_suffix(suffix)?;
        (rest.chars().count() >= min_rest).then(|| rest.to_owned())
    };
    let undouble = |mut s: String| {
        let b: Vec<char> = s.chars().collect();
        if b.len() >= 3 && b[b.len() - 1] == b[b.len() - 2] && !"lsz".contains(b[b.len() - 1]) {
            s.pop();
        }
        s
    };
    if n <= 3 {
        return w.to_owned();
    }
    if let Some(r) = strip("ies", 2) {
        return r + "y";
    }
    if w.ends_with("sses") {
        return w[..w.len() - 2].to_owned();
    }
    if let Some(r) = strip("ing", 3) {
        return undouble(r);
    }
    if let Some(r) = strip("ed", 3) {
        return undouble(r);
    }
    if let Some(r) = strip("ly", 4) {
        return r;
    }
    if let Some(r) = strip("er", 4) {
        return undouble(r);
    }
    if w.ends_with("ss") || w.ends_with("us") || w.ends_with("is") {
        return w.to_owned();
    }
    if let Some(r) = strip("es", 3) {
        // "boxes" -> "box", but "types" -> "type" is handled by the plain `s` below.
        if r.ends_with(['x', 's']) || r.ends_with("ch") || r.ends_with("sh") {
            return r;
        }
    }
    if let Some(r) = strip("s", 3) {
        return r;
    }
    w.to_owned()
}

/// Stemmed, stopword-free tokens of `s` for BM25.
pub fn tokens(s: &str) -> Vec<String> {
    words(s)
        .into_iter()
        .filter(|w| !STOPWORDS.contains(&w.as_str()))
        .map(|w| stem(&w))
        .collect()
}

/// Cut `s` to [`MAX_QUERY_CHARS`] characters.
pub fn clamp(s: &str) -> &str {
    match s.char_indices().nth(MAX_QUERY_CHARS) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}
