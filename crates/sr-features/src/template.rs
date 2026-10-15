//! Message templates with `{placeholder}` substitution, shared by the welcome,
//! auto-thread, stat-channel and quote modules.

/// Replace `{name}` with the matching value. Unknown placeholders stay as typed, and
/// values are never re-scanned, so a user name containing `{server}` stays literal.
pub fn render(template: &str, vars: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = &after[..close];
                match vars.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// Make user-controlled text (a display name) unable to mention anybody: a zero width
/// space after every `@` defuses `@everyone`, `@here` and raw mention syntax.
pub fn defuse(text: &str) -> String {
    text.replace('@', "@\u{200b}")
}

/// Cut `s` to at most `max` characters, ending in an ellipsis when it was cut.
pub fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_known_and_keeps_unknown() {
        let v = [("user", "<@1>".to_owned()), ("server", "Club".to_owned())];
        assert_eq!(
            render("Hi {user}, welcome to {server}! {nope}", &v),
            "Hi <@1>, welcome to Club! {nope}"
        );
    }

    #[test]
    fn values_are_not_rescanned() {
        let v = [("user", "{server}".to_owned()), ("server", "X".to_owned())];
        assert_eq!(render("{user}", &v), "{server}");
    }

    #[test]
    fn unbalanced_brace_is_literal() {
        assert_eq!(render("a { b", &[]), "a { b");
        assert_eq!(render("{", &[]), "{");
    }

    #[test]
    fn defuse_breaks_mentions() {
        assert!(!defuse("@everyone").contains("@everyone"));
    }

    #[test]
    fn clip_counts_chars() {
        assert_eq!(clip("abc", 3), "abc");
        assert_eq!(clip("abcd", 3), "ab\u{2026}");
    }
}
