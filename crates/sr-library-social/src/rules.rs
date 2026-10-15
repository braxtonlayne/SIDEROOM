//! Validation and pure logic (unit tested without a database).

use crate::error::{SocialError, SocialResult};

pub const MAX_LIMIT: u32 = 100;
pub const DEFAULT_LIMIT: u32 = 24;
pub const MAX_LISTS: i64 = 100;
pub const MAX_LIST_ITEMS: i64 = 1000;
pub const MAX_REVIEW: usize = 8000;
pub const MAX_NAME: usize = 100;
pub const MAX_DESCRIPTION: usize = 500;
pub const MAX_NOTE: usize = 500;

pub fn limit(raw: Option<u32>) -> u32 {
    raw.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// A 1 to 10 score.
pub fn score(raw: i32) -> SocialResult<i16> {
    i16::try_from(raw)
        .ok()
        .filter(|s| (1..=10).contains(s))
        .ok_or_else(|| SocialError::Invalid("score must be between 1 and 10".into()))
}

/// Trim `raw` and check its length in characters. `allow_empty` permits an empty result.
pub fn clean_text(raw: &str, max: usize, field: &str, allow_empty: bool) -> SocialResult<String> {
    let t = raw.trim();
    if t.is_empty() && !allow_empty {
        return Err(SocialError::Invalid(format!("{field} must not be empty")));
    }
    if t.chars().count() > max {
        return Err(SocialError::Invalid(format!(
            "{field} must be at most {max} characters"
        )));
    }
    if t.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(SocialError::Invalid(format!(
            "{field} contains control characters"
        )));
    }
    Ok(t.to_owned())
}

/// Opaque keyset cursor over a list of numbers.
pub fn encode_cursor(parts: &[i64]) -> String {
    parts
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(".")
}

pub fn decode_cursor(raw: &str, n: usize) -> SocialResult<Vec<i64>> {
    let parts: Vec<i64> = raw
        .split('.')
        .map(str::parse)
        .collect::<Result<_, _>>()
        .map_err(|_| SocialError::Invalid("before is not a valid cursor".into()))?;
    if parts.len() != n {
        return Err(SocialError::Invalid("before is not a valid cursor".into()));
    }
    Ok(parts)
}

/// Round an average to one decimal.
pub fn round1(a: f64) -> f64 {
    (a * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_are_one_to_ten() {
        assert!(score(0).is_err());
        assert!(score(11).is_err());
        assert!(score(-3).is_err());
        assert_eq!(score(1).unwrap(), 1);
        assert_eq!(score(10).unwrap(), 10);
    }

    #[test]
    fn text_is_trimmed_and_bounded() {
        assert_eq!(clean_text("  hi \n", 10, "body", false).unwrap(), "hi");
        assert!(clean_text("   ", 10, "body", false).is_err());
        assert_eq!(clean_text("", 10, "note", true).unwrap(), "");
        assert!(clean_text("abcdef", 5, "body", false).is_err());
        // Characters, not bytes.
        assert!(clean_text("ああああ", 4, "body", false).is_ok());
        assert!(clean_text("a\u{7}b", 10, "body", false).is_err());
        assert!(clean_text("a\nb", 10, "body", false).is_ok());
    }

    #[test]
    fn cursors_round_trip() {
        let c = encode_cursor(&[12, 345]);
        assert_eq!(decode_cursor(&c, 2).unwrap(), vec![12, 345]);
        assert!(decode_cursor("1", 2).is_err());
        assert!(decode_cursor("a.b", 2).is_err());
    }

    #[test]
    fn limits_are_clamped() {
        assert_eq!(limit(None), DEFAULT_LIMIT);
        assert_eq!(limit(Some(0)), 1);
        assert_eq!(limit(Some(9999)), MAX_LIMIT);
    }

    #[test]
    fn averages_round_to_one_decimal() {
        assert_eq!(round1(7.449), 7.4);
        assert_eq!(round1(7.45), 7.5);
    }
}
