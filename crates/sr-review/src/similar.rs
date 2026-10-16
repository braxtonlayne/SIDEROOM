//! Title similarity shared by the kinds' duplicate detection (REV-ED-006).

/// A title reduced for comparing: lower case letters and digits only, bracketed tags removed.
pub fn normalize_title(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0_u32;
    for c in s.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            c if depth == 0 && c.is_alphanumeric() => out.extend(c.to_lowercase()),
            _ => {}
        }
    }
    out
}

/// How alike two titles are, 0 to 100: equal after normalising is 100; one containing the other
/// scores by how much of the longer one it covers (between 40 and 90).
pub fn title_score(a: &str, b: &str) -> u8 {
    let (a, b) = (normalize_title(a), normalize_title(b));
    if a.is_empty() || b.is_empty() {
        return 0;
    }
    if a == b {
        return 100;
    }
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    if short.chars().count() >= 4 && long.contains(short.as_str()) {
        let ratio = short.chars().count() * 100 / long.chars().count();
        return u8::try_from(ratio.clamp(40, 90)).unwrap_or(40);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_compare_without_case_punctuation_or_tags() {
        assert_eq!(title_score("Blue Hour", "blue-hour!"), 100);
        assert_eq!(title_score("Blue Hour [Digital]", "Blue Hour (2019)"), 100);
        assert_eq!(title_score("Berserk", "Berserk of Gluttony"), 41);
        let part = title_score("Blue Hour", "Blue Hour Side Stories");
        assert!((40..=90).contains(&part), "{part}");
        assert_eq!(title_score("Berserk", "Vagabond"), 0);
        // Very short names do not match inside longer ones.
        assert_eq!(title_score("Go", "Gone Girl"), 0);
        assert_eq!(title_score("", "x"), 0);
    }

    #[test]
    fn normalising_keeps_non_latin_letters() {
        assert_eq!(normalize_title("進撃の巨人 (2013)"), "進撃の巨人");
    }
}
