//! The one-line excerpt of a message shown in a preview: the Message Requests row (D-688) and the
//! thread row under a message (D-689). Nothing but the words and an attachment count leaves the
//! server this way: no embeds, no attachment names, no author object of its own.

use sr_core::{MessageId, UserId};

/// How many characters of a message a preview shows (D-688).
pub const EXCERPT_CHARS: usize = 140;

/// What a preview reads of one message, in one row. `content` is the raw text (mentions stay as written;
/// the client resolves them), cut by [`excerpt`] before it goes out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageGist {
    pub id: MessageId,
    pub author_id: UserId,
    pub content: String,
    /// The number of attachments on the message.
    pub attachments: u32,
}

/// Characters that continue the one before them (combining marks, joiners, variation selectors,
/// emoji modifiers, tag characters): a cut must not fall in front of one.
fn extends(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F
        | 0x0483..=0x0489
        | 0x0591..=0x05BD
        | 0x0610..=0x061A
        | 0x064B..=0x065F
        | 0x0900..=0x0903
        | 0x093A..=0x094F
        | 0x1AB0..=0x1AFF
        | 0x1DC0..=0x1DFF
        | 0x200C..=0x200D
        | 0x20D0..=0x20FF
        | 0xFE00..=0xFE0F
        | 0xFE20..=0xFE2F
        | 0x1F3FB..=0x1F3FF
        | 0xE0020..=0xE007F
        | 0xE0100..=0xE01EF)
}

/// The first `max` characters of `content` on one line: runs of white space (line breaks included)
/// become one space, the text is trimmed, a cut falls on a character boundary and never between a
/// character and the combining marks that follow it, and an ellipsis ends a cut text. `None` when
/// nothing but white space is left.
pub fn excerpt(content: &str, max: usize) -> Option<String> {
    let mut flat = String::new();
    let mut chars = 0usize;
    let mut pending_space = false;
    let mut cut = false;
    let mut kept: Vec<char> = Vec::new();
    for c in content.chars() {
        if c.is_whitespace() {
            pending_space = !kept.is_empty();
            continue;
        }
        if pending_space {
            pending_space = false;
            if chars == max {
                cut = true;
                break;
            }
            kept.push(' ');
            chars += 1;
        }
        if chars == max {
            if extends(c) {
                // The cut would split a cluster: take the character before it out too.
                while kept.last().is_some_and(|k| extends(*k)) {
                    kept.pop();
                }
                kept.pop();
            }
            cut = true;
            break;
        }
        kept.push(c);
        chars += 1;
    }
    if kept.is_empty() {
        return None;
    }
    while kept.last() == Some(&' ') {
        kept.pop();
    }
    flat.extend(kept);
    if cut {
        flat.push('\u{2026}');
    }
    Some(flat)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_kept_whole() {
        assert_eq!(excerpt("hello there", 140).as_deref(), Some("hello there"));
    }

    #[test]
    fn exactly_the_limit_is_not_cut() {
        let s = "a".repeat(140);
        assert_eq!(excerpt(&s, 140).as_deref(), Some(s.as_str()));
        let s = "a".repeat(141);
        let got = excerpt(&s, 140).unwrap();
        assert_eq!(got.chars().count(), 141);
        assert!(got.ends_with('\u{2026}'));
        assert!(got.starts_with(&"a".repeat(140)));
    }

    #[test]
    fn a_cut_inside_a_multi_byte_character_stays_on_a_boundary() {
        // Each char is 3 bytes; a byte cut at 140 would fall inside one.
        let s = "\u{3042}".repeat(200);
        let got = excerpt(&s, 140).unwrap();
        assert_eq!(got.chars().count(), 141);
        assert_eq!(got.chars().filter(|c| *c == '\u{3042}').count(), 140);
        // Four-byte characters too.
        let s = "\u{1F600}".repeat(150);
        let got = excerpt(&s, 140).unwrap();
        assert_eq!(got.chars().filter(|c| *c == '\u{1F600}').count(), 140);
    }

    #[test]
    fn a_combining_sequence_is_not_split_at_the_cut() {
        // 139 plain characters, then "e" + a combining acute: the cut would leave the mark behind.
        let s = format!("{}e\u{0301}tail", "a".repeat(139));
        let got = excerpt(&s, 140).unwrap();
        assert!(
            !got.contains('\u{0301}'),
            "the mark must not be orphaned: {got:?}"
        );
        assert_eq!(got, format!("{}\u{2026}", "a".repeat(139)));
        // When the whole sequence fits, it stays whole.
        let s = format!("{}e\u{0301}", "a".repeat(138));
        assert_eq!(excerpt(&s, 140).unwrap(), s);
    }

    #[test]
    fn only_white_space_or_nothing_gives_none() {
        assert_eq!(excerpt("", 140), None);
        assert_eq!(excerpt("   \n\t \r\n  ", 140), None);
        assert_eq!(excerpt("\u{00A0}\u{2003}", 140), None);
    }

    #[test]
    fn line_breaks_and_runs_of_white_space_become_one_space() {
        assert_eq!(
            excerpt("  one\n\n\ntwo \t three\r\nfour  ", 140).as_deref(),
            Some("one two three four")
        );
    }

    #[test]
    fn white_space_at_the_cut_leaves_no_trailing_space() {
        let s = format!("{} {}", "a".repeat(139), "b".repeat(10));
        // 139 + the space = 140 characters, then "b" would be the 141st.
        let got = excerpt(&s, 140).unwrap();
        assert_eq!(got, format!("{}\u{2026}", "a".repeat(139)));
    }

    #[test]
    fn markup_is_left_as_written() {
        let s = "<img src=x onerror=alert(1)> [a](http://x) **b** <@123>";
        assert_eq!(excerpt(s, 140).as_deref(), Some(s));
    }
}
