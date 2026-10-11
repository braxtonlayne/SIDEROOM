//! Parser for the Discord-style search box syntax.
//!
//! `hello "exact phrase" from:123 has:image before:2025-01-31 pinned:true`.
//! Repeating a filter ORs its values (`from:1 from:2`); different filters AND.
//! Unknown `key:value` tokens are treated as plain text.

use crate::messages::{AuthorType, HasKind};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    #[error("invalid value {value:?} for filter {filter}")]
    InvalidFilter { filter: &'static str, value: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedQuery {
    /// Free words (analyzed by the index).
    pub words: Vec<String>,
    /// Quoted phrases.
    pub phrases: Vec<String>,
    pub from: Vec<u64>,
    pub mentions: Vec<u64>,
    /// Every listed kind must be present.
    pub has: Vec<HasKind>,
    pub in_channels: Vec<u64>,
    pub author_types: Vec<AuthorType>,
    pub pinned: Option<bool>,
    /// Exclusive upper bound (ms): messages strictly before this instant.
    pub before_ms: Option<i64>,
    /// Inclusive lower bound (ms).
    pub after_ms: Option<i64>,
}

impl ParsedQuery {
    pub fn has_text(&self) -> bool {
        !self.words.is_empty() || !self.phrases.is_empty()
    }
}

const DAY_MS: i64 = 86_400_000;

/// Days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Start of a `YYYY-MM-DD` day in UTC, in ms.
fn parse_day(s: &str) -> Option<i64> {
    let mut it = s.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if it.next().is_some() || !(1..=12).contains(&m) || d < 1 {
        return None;
    }
    let first = days_from_civil(y, m, 1);
    let next = if m == 12 {
        days_from_civil(y + 1, 1, 1)
    } else {
        days_from_civil(y, m + 1, 1)
    };
    if d > next - first {
        return None;
    }
    Some((first + d - 1) * DAY_MS)
}

fn id(filter: &'static str, v: &str) -> Result<u64, QueryError> {
    v.trim_matches(|c| matches!(c, '<' | '>' | '@' | '#' | '&'))
        .parse()
        .map_err(|_| invalid(filter, v))
}

fn invalid(filter: &'static str, v: &str) -> QueryError {
    QueryError::InvalidFilter {
        filter,
        value: v.to_owned(),
    }
}

fn day(filter: &'static str, v: &str) -> Result<i64, QueryError> {
    parse_day(v).ok_or_else(|| invalid(filter, v))
}

/// Split on whitespace; `"quoted text"` becomes one token flagged as a phrase.
fn tokens(input: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for ch in input.chars() {
        match ch {
            '"' if in_quote => {
                out.push((std::mem::take(&mut cur), true));
                in_quote = false;
            }
            '"' => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), false));
                }
                in_quote = true;
            }
            c if c.is_whitespace() && !in_quote => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), false));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        // An unterminated quote is treated as a phrase.
        out.push((cur, in_quote));
    }
    out
}

pub fn parse_query(input: &str) -> Result<ParsedQuery, QueryError> {
    let mut q = ParsedQuery::default();
    for (tok, quoted) in tokens(input) {
        if quoted {
            if !tok.trim().is_empty() {
                q.phrases.push(tok);
            }
            continue;
        }
        let Some((key, value)) = tok.split_once(':') else {
            q.words.push(tok);
            continue;
        };
        match key.to_ascii_lowercase().as_str() {
            "from" => q.from.push(id("from", value)?),
            "mentions" => q.mentions.push(id("mentions", value)?),
            "in" => q.in_channels.push(id("in", value)?),
            "has" => q
                .has
                .push(HasKind::parse(value).ok_or_else(|| invalid("has", value))?),
            "authortype" => q
                .author_types
                .push(AuthorType::parse(value).ok_or_else(|| invalid("authorType", value))?),
            "pinned" => {
                q.pinned = Some(match value.to_ascii_lowercase().as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err(invalid("pinned", value)),
                })
            }
            "before" => {
                let ms = day("before", value)?;
                q.before_ms = Some(q.before_ms.map_or(ms, |c| c.min(ms)));
            }
            "after" => {
                // Discord: after:D means after the end of day D.
                let ms = day("after", value)? + DAY_MS;
                q.after_ms = Some(q.after_ms.map_or(ms, |c| c.max(ms)));
            }
            "during" => {
                let start = day("during", value)?;
                q.after_ms = Some(q.after_ms.map_or(start, |c| c.max(start)));
                let end = start + DAY_MS;
                q.before_ms = Some(q.before_ms.map_or(end, |c| c.min(end)));
            }
            _ => q.words.push(tok),
        }
    }
    Ok(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_filters_and_text() {
        let q = parse_query(
            r#"hello "exact phrase" from:12 from:<@13> mentions:5 has:image has:link in:9 pinned:true authorType:bot"#,
        )
        .unwrap();
        assert_eq!(q.words, vec!["hello"]);
        assert_eq!(q.phrases, vec!["exact phrase"]);
        assert_eq!(q.from, vec![12, 13]);
        assert_eq!(q.mentions, vec![5]);
        assert_eq!(q.has, vec![HasKind::Image, HasKind::Link]);
        assert_eq!(q.in_channels, vec![9]);
        assert_eq!(q.pinned, Some(true));
        assert_eq!(q.author_types, vec![AuthorType::Bot]);
    }

    #[test]
    fn date_filters() {
        let q = parse_query("before:2025-01-02 after:2025-01-01").unwrap();
        assert_eq!(q.before_ms, Some(1_735_776_000_000));
        assert_eq!(q.after_ms, Some(1_735_689_600_000 + DAY_MS));
        let q = parse_query("during:1970-01-02").unwrap();
        assert_eq!(q.after_ms, Some(DAY_MS));
        assert_eq!(q.before_ms, Some(2 * DAY_MS));
    }

    #[test]
    fn rejects_bad_values_and_keeps_unknown_keys_as_text() {
        assert!(parse_query("from:bob").is_err());
        assert!(parse_query("before:2025-02-30").is_err());
        assert!(parse_query("has:banana").is_err());
        assert!(parse_query("pinned:maybe").is_err());
        let q = parse_query("note:this").unwrap();
        assert_eq!(q.words, vec!["note:this"]);
    }
}
