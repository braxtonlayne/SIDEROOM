//! Block-level parsing: fenced code, quotes, headings, subtext, lists.

use crate::ast::{Node, merge_text};
use crate::inline::{MAX_DEPTH, parse_inline};

pub(crate) fn parse_document(input: &str) -> Vec<Node> {
    let mut out = Vec::new();
    let mut rest = input;
    loop {
        match find_fence(rest) {
            Some((open, body_start, close)) => {
                out.extend(parse_blocks(&rest[..open], 0, true));
                let (lang, code) = split_lang(&rest[body_start..close]);
                out.push(Node::CodeBlock { lang, code });
                rest = &rest[close + 3..];
            }
            None => {
                out.extend(parse_blocks(rest, 0, true));
                break;
            }
        }
    }
    merge_text(out)
}

/// Locate the first complete fenced block: (open index, body start, close index).
fn find_fence(s: &str) -> Option<(usize, usize, usize)> {
    let b = s.as_bytes();
    let mut i = 0;
    while i + 3 <= b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b'`' {
            let n = b[i..].iter().take_while(|&&c| c == b'`').count();
            if n >= 3 {
                let body = i + 3;
                // closing fence: next run of >= 3 backticks after non-empty body
                let mut j = body;
                while j + 3 <= b.len() {
                    if b[j] == b'`' {
                        let m = b[j..].iter().take_while(|&&c| c == b'`').count();
                        if m >= 3 && j > body {
                            return Some((i, body, j));
                        }
                        j += m;
                    } else {
                        j += 1;
                    }
                }
                return None;
            }
            i += n;
            continue;
        }
        i += 1;
    }
    None
}

fn split_lang(body: &str) -> (Option<String>, String) {
    let (first, rest) = match body.split_once('\n') {
        Some((f, r)) => (f, Some(r)),
        None => (body, None),
    };
    let is_lang = !first.is_empty()
        && first.len() <= 32
        && first
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'+' | b'#' | b'.' | b'-'));
    let (lang, code) = match (is_lang, rest) {
        (true, Some(r)) => (Some(first.to_ascii_lowercase()), r),
        _ => (None, body.strip_prefix('\n').unwrap_or(body)),
    };
    let code = code.strip_suffix('\n').unwrap_or(code);
    (lang, code.to_string())
}

struct ListEntry<'a> {
    indent: usize,
    ordered: bool,
    start: u64,
    text: &'a str,
}

fn list_entry(line: &str) -> Option<ListEntry<'_>> {
    let trimmed = line.trim_start_matches(' ');
    let indent = line.len() - trimmed.len();
    let b = trimmed.as_bytes();
    if b.len() >= 2 && (b[0] == b'-' || b[0] == b'*') && b[1] == b' ' {
        let text = trimmed[2..].trim_start_matches(' ');
        if text.is_empty() {
            return None;
        }
        return Some(ListEntry {
            indent,
            ordered: false,
            start: 0,
            text,
        });
    }
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits) && b.get(digits) == Some(&b'.') && b.get(digits + 1) == Some(&b' ')
    {
        let text = trimmed[digits + 2..].trim_start_matches(' ');
        if text.is_empty() {
            return None;
        }
        return Some(ListEntry {
            indent,
            ordered: true,
            start: trimmed[..digits].parse().ok()?,
            text,
        });
    }
    None
}

fn build_list(entries: &[ListEntry<'_>], pos: &mut usize, indent: usize, depth: u8) -> Node {
    let first = &entries[*pos];
    let ordered = first.ordered;
    let start = ordered.then_some(first.start);
    let mut items: Vec<Vec<Node>> = Vec::new();
    while *pos < entries.len() {
        let e = &entries[*pos];
        if e.indent < indent {
            break;
        }
        if e.indent > indent && !items.is_empty() && depth < 8 {
            let nested = build_list(entries, pos, e.indent, depth + 1);
            if let Some(last) = items.last_mut() {
                last.push(nested);
            }
            continue;
        }
        items.push(parse_inline(e.text, depth + 1, false));
        *pos += 1;
    }
    Node::List {
        ordered,
        start,
        items,
    }
}

fn parse_blocks(text: &str, depth: u8, allow_quote: bool) -> Vec<Node> {
    let mut out: Vec<Node> = Vec::new();
    if depth >= MAX_DEPTH {
        return parse_inline(text, depth, false);
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut para: Vec<&str> = Vec::new();
    let mut i = 0;

    fn flush(para: &mut Vec<&str>, out: &mut Vec<Node>, depth: u8) {
        if !para.is_empty() {
            out.extend(parse_inline(&para.join("\n"), depth + 1, false));
            para.clear();
        }
    }

    while i < lines.len() {
        let line = lines[i];
        let last = i + 1 == lines.len();
        // Quotes
        if allow_quote {
            if let Some(body) = line.strip_prefix(">>> ") {
                flush(&mut para, &mut out, depth);
                let mut rest = String::from(body);
                for l in &lines[i + 1..] {
                    rest.push('\n');
                    rest.push_str(l);
                }
                out.push(Node::Quote {
                    children: parse_blocks(&rest, depth + 1, false),
                });
                return merge_text(out);
            }
            if line.starts_with("> ") {
                flush(&mut para, &mut out, depth);
                let mut body: Vec<&str> = Vec::new();
                while i < lines.len() && lines[i].starts_with("> ") {
                    body.push(&lines[i][2..]);
                    i += 1;
                }
                out.push(Node::Quote {
                    children: parse_blocks(&body.join("\n"), depth + 1, false),
                });
                continue;
            }
        }
        // Headings and subtext
        let hashes = line.bytes().take_while(|&c| c == b'#').count();
        if (1..=3).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
            let content = line[hashes + 1..].trim();
            if !content.is_empty() {
                flush(&mut para, &mut out, depth);
                out.push(Node::Heading {
                    level: hashes as u8,
                    children: parse_inline(content, depth + 1, false),
                });
                i += 1;
                continue;
            }
        }
        if let Some(content) = line.strip_prefix("-# ") {
            let content = content.trim();
            if !content.is_empty() {
                flush(&mut para, &mut out, depth);
                out.push(Node::Subtext {
                    children: parse_inline(content, depth + 1, false),
                });
                i += 1;
                continue;
            }
        }
        // Lists
        if list_entry(line).is_some() {
            flush(&mut para, &mut out, depth);
            let mut entries = Vec::new();
            while i < lines.len() {
                match list_entry(lines[i]) {
                    Some(e) => entries.push(e),
                    None => break,
                }
                i += 1;
            }
            let mut pos = 0;
            while pos < entries.len() {
                let indent = entries[pos].indent;
                out.push(build_list(&entries, &mut pos, indent, 0));
            }
            continue;
        }
        para.push(line);
        let _ = last;
        i += 1;
    }
    flush(&mut para, &mut out, depth);
    merge_text(out)
}
