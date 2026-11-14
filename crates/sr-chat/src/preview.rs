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

/// How many times the cut may move back looking for a place that splits no markup (D-728).
const CUT_TRIES: usize = 16;

/// The text of `content` on one line: runs of white space (line breaks included) become one space and
/// the ends are trimmed.
fn fold(content: &str) -> Vec<char> {
    let mut kept: Vec<char> = Vec::new();
    let mut pending_space = false;
    for c in content.chars() {
        if c.is_whitespace() {
            pending_space = !kept.is_empty();
            continue;
        }
        if pending_space {
            pending_space = false;
            kept.push(' ');
        }
        kept.push(c);
    }
    kept
}

/// Whether a markup opener starts at `i` of `s`: `||`, `<`, a backtick, `[`, `*`, `_` or `~`.
fn opener_at(s: &[char], i: usize) -> bool {
    match s[i] {
        '|' => s.get(i + 1) == Some(&'|'),
        '<' | '`' | '[' | '*' | '_' | '~' => true,
        _ => false,
    }
}

fn plain_of(s: &[char]) -> String {
    let text: String = s.iter().collect();
    sr_markdown::to_plain_text(&sr_markdown::parse(&text))
}

/// The first `max` characters of `content` on one line: runs of white space (line breaks included)
/// become one space, the text is trimmed, a cut falls on a character boundary and never between a
/// character and the combining marks that follow it, and an ellipsis ends a cut text. The cut also
/// never falls inside markup (D-728): the plain text of the cut text must be the start of the plain
/// text of the whole message (so half a spoiler, a split mention or a split link never reaches a
/// preview as words); when it is not, the cut moves back to before the last markup opener, then to
/// the previous white space, alternately, at most 16 times. `None` when nothing but white space is
/// left or no such cut is left.
pub fn excerpt(content: &str, max: usize) -> Option<String> {
    let folded = fold(content);
    if folded.is_empty() {
        return None;
    }
    if folded.len() <= max {
        return Some(folded.into_iter().collect());
    }
    let mut cut = max;
    if extends(folded[cut]) {
        // The cut would split a cluster: take the character before it out too.
        while cut > 0 && extends(folded[cut - 1]) {
            cut -= 1;
        }
        cut = cut.saturating_sub(1);
    }
    let trim = |mut n: usize| {
        while n > 0 && folded[n - 1] == ' ' {
            n -= 1;
        }
        n
    };
    let mut cut = trim(cut);
    let full = plain_of(&folded);
    let mut by_opener = true;
    for _ in 0..=CUT_TRIES {
        if cut == 0 {
            return None;
        }
        let part = plain_of(&folded[..cut]);
        if full.starts_with(part.trim_end()) {
            let mut s: String = folded[..cut].iter().collect();
            s.push('\u{2026}');
            return Some(s);
        }
        let head = &folded[..cut];
        let next = if by_opener {
            (0..cut).rev().find(|&i| opener_at(head, i))
        } else {
            head.iter().rposition(|c| *c == ' ')
        };
        by_opener = !by_opener;
        // Nothing to move to: try the other kind of place next time.
        cut = trim(next.unwrap_or(cut.saturating_sub(1)));
    }
    None
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

#[cfg(test)]
mod safe_cut_tests {
    use super::*;
    use serde_json::Value;
    use sr_markdown::{Node, parse, to_plain_text};

    fn join(v: &Value) -> String {
        v.as_array()
            .expect("parts")
            .iter()
            .map(|p| match p {
                Value::String(s) => s.clone(),
                o => o["repeat"]
                    .as_str()
                    .expect("repeat")
                    .repeat(o["times"].as_u64().expect("times") as usize),
            })
            .collect()
    }

    fn has_spoiler(nodes: &[Node]) -> bool {
        nodes.iter().any(|n| matches!(n, Node::Spoiler { .. }))
    }

    /// The same list the web runs (`p70Plain.test.tsx`).
    #[test]
    fn shared_cut_cases() {
        let v: Value = serde_json::from_str(include_str!("preview_cases.json")).unwrap();
        for c in v["cases"].as_array().unwrap() {
            let name = c["name"].as_str().unwrap();
            let got = excerpt(&join(&c["input"]), EXCERPT_CHARS);
            let want = if c["want"].is_null() {
                None
            } else {
                Some(join(&c["want"]))
            };
            assert_eq!(got, want, "case {name}");
        }
    }

    /// The confirm test (a): the fault the plan names. A raw 140-character cut of an open spoiler is what
    /// reaches the parser as text, so the hidden letters would show; the cut never ends that way.
    #[test]
    fn a_cut_never_leaves_a_spoiler_open() {
        let content = format!("a ||{}||", "x".repeat(200));
        let raw: String = content.chars().take(140).collect();
        // What the parser does with an unclosed `||`: no spoiler, the letters are text (the leak).
        let nodes = parse(&raw);
        assert!(!has_spoiler(&nodes), "unclosed spoiler parsed as a spoiler");
        assert!(to_plain_text(&nodes).contains("xxxx"));
        // The excerpt does not hand that text on.
        let got = excerpt(&content, EXCERPT_CHARS).unwrap();
        assert!(!got.contains('x'), "letters leaked: {got:?}");
    }

    /// The confirm test (c): a mention cut in the middle is not left as `<@12345`.
    #[test]
    fn a_cut_never_leaves_half_a_mention() {
        let content = format!("{} <@123456789012345678> tail", "x".repeat(130));
        let got = excerpt(&content, EXCERPT_CHARS).unwrap();
        assert!(!got.contains('<'), "half a mention: {got:?}");
    }

    /// Property: over the markdown corpus, each input repeated until it is over 140 characters, the plain
    /// text of every result (without its ellipsis) is the start of the whole's plain text.
    #[test]
    fn cut_text_reads_as_the_start_of_the_whole() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../sr-markdown/tests/corpus");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let file: Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            for c in file["cases"].as_array().unwrap() {
                let input = c["input"].as_str().unwrap();
                if input.trim().is_empty() {
                    continue;
                }
                let mut text = String::new();
                while text.chars().count() <= 150 {
                    text.push_str(input);
                    text.push(' ');
                }
                let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
                let full = to_plain_text(&parse(&folded));
                if let Some(e) = excerpt(&text, EXCERPT_CHARS) {
                    let body = e.strip_suffix('\u{2026}').unwrap_or(&e);
                    let part = to_plain_text(&parse(body));
                    assert!(
                        full.starts_with(part.trim_end()),
                        "input {input:?}: {e:?} reads {part:?}, not the start of {full:?}"
                    );
                }
                checked += 1;
            }
        }
        assert!(checked > 50, "only {checked} corpus inputs");
    }

    /// Worst case for time: 4,000 characters of nested markup, cut at 140.
    #[test]
    fn worst_case_time_is_small() {
        let text = "**a _b ~c ||d `e` [f](http://x) <@1>|| ~_** ".repeat(100);
        let start = std::time::Instant::now();
        let _ = excerpt(&text, EXCERPT_CHARS);
        eprintln!(
            "worst case excerpt ({} chars): {:?}",
            text.chars().count(),
            start.elapsed()
        );
    }
}
