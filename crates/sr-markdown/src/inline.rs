//! Inline-level parsing.

use crate::ast::{LinkKind, Node, TimestampStyle, merge_text};

pub(crate) const MAX_DEPTH: u8 = 16;
const MAX_SCAN: usize = 2048;

pub(crate) fn parse_inline(s: &str, depth: u8, in_link: bool) -> Vec<Node> {
    Parser::new(s, depth, in_link).run()
}

struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    depth: u8,
    in_link: bool,
    /// Per delimiter kind: start index from which no closer exists.
    fail: [usize; 8],
    /// Per backtick run length (1..=8): index from which no closing run exists.
    code_fail: [usize; 9],
}

fn is_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

fn char_before(s: &str, i: usize) -> Option<char> {
    s[..i].chars().next_back()
}

fn parse_u64(s: &str) -> Option<u64> {
    if s.is_empty() || s.len() > 20 || !s.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

fn run_len(b: &[u8], i: usize, c: u8) -> usize {
    b[i..].iter().take_while(|&&x| x == c).count()
}

impl<'a> Parser<'a> {
    fn new(s: &'a str, depth: u8, in_link: bool) -> Self {
        Self {
            s,
            b: s.as_bytes(),
            depth,
            in_link,
            fail: [usize::MAX; 8],
            code_fail: [usize::MAX; 9],
        }
    }

    fn run(mut self) -> Vec<Node> {
        let mut out: Vec<Node> = Vec::new();
        let mut text = String::new();
        let mut i = 0;
        while i < self.b.len() {
            let c = self.b[i];
            let mut handled: Option<(Node, usize)> = None;
            match c {
                b'\\' => {
                    if let Some(n) = self.s[i + 1..].chars().next()
                        && n.is_ascii_punctuation()
                    {
                        text.push(n);
                        i += 2;
                        continue;
                    }
                }
                b'\n' => handled = Some((Node::LineBreak, 1)),
                b'`' => {
                    handled = self.code(i);
                    if handled.is_none() {
                        let n = run_len(self.b, i, b'`');
                        text.push_str(&self.s[i..i + n]);
                        i += n;
                        continue;
                    }
                }
                b'<' => handled = self.angle(i),
                b'[' if !self.in_link => handled = self.masked(i),
                b'h' if !self.in_link => handled = self.bare_url(i),
                b'@' => handled = self.at(i),
                b'*' | b'_' | b'~' | b'|' => handled = self.emphasis(i),
                _ => {}
            }
            if let Some((node, len)) = handled {
                if !text.is_empty() {
                    out.push(Node::text(std::mem::take(&mut text)));
                }
                out.push(node);
                i += len;
            } else {
                let ch = self.s[i..].chars().next().unwrap_or('\u{fffd}');
                text.push(ch);
                i += ch.len_utf8().max(1);
            }
        }
        if !text.is_empty() {
            out.push(Node::text(text));
        }
        merge_text(out)
    }

    fn find_code_close(&mut self, from: usize, n: usize) -> Option<usize> {
        if n <= 8 && from >= self.code_fail[n] {
            return None;
        }
        let mut j = from;
        while j < self.b.len() {
            if self.b[j] == b'`' {
                let k = run_len(self.b, j, b'`');
                if k == n {
                    return Some(j);
                }
                j += k;
            } else {
                j += 1;
            }
        }
        if n <= 8 {
            self.code_fail[n] = self.code_fail[n].min(from);
        }
        None
    }

    fn code(&mut self, i: usize) -> Option<(Node, usize)> {
        let n = run_len(self.b, i, b'`');
        let close = self.find_code_close(i + n, n)?;
        let mut code = &self.s[i + n..close];
        if code.is_empty() {
            return None;
        }
        // Trim one padding space on both sides (so `` ` `` can be written).
        if code.len() > 2 && code.starts_with(' ') && code.ends_with(' ') && !code.trim().is_empty()
        {
            code = &code[1..code.len() - 1];
        }
        Some((
            Node::InlineCode {
                code: code.to_string(),
            },
            close + n - i,
        ))
    }

    fn at(&self, i: usize) -> Option<(Node, usize)> {
        let rest = &self.s[i..];
        if rest.starts_with("@everyone") {
            Some((Node::Everyone, 9))
        } else if rest.starts_with("@here") {
            Some((Node::Here, 5))
        } else {
            None
        }
    }

    fn angle(&self, i: usize) -> Option<(Node, usize)> {
        let rest = &self.s[i..];
        if rest.starts_with("<http://") || rest.starts_with("<https://") {
            if self.in_link {
                return None;
            }
            let end = rest.bytes().take(MAX_SCAN).position(|c| c == b'>')?;
            let url = &rest[1..end];
            if url
                .bytes()
                .any(|c| is_ws(c) || c == b'<' || c.is_ascii_control())
                || !valid_url(url)
            {
                return None;
            }
            return Some((
                Node::Link {
                    url: url.to_string(),
                    kind: LinkKind::AutoSuppressed,
                    children: vec![Node::text(url)],
                },
                end + 1,
            ));
        }
        let end = rest.bytes().take(200).position(|c| c == b'>')?;
        let inner = &rest[1..end];
        let node = parse_token(inner)?;
        Some((node, end + 1))
    }

    fn masked(&self, i: usize) -> Option<(Node, usize)> {
        // Find the matching ']'.
        let mut depth = 0usize;
        let mut j = i;
        let limit = (i + MAX_SCAN).min(self.b.len());
        let mut close = None;
        while j < limit {
            match self.b[j] {
                b'\\' => j += 1,
                b'[' => depth += 1,
                b']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        close = Some(j);
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        let close = close?;
        if self.b.get(close + 1) != Some(&b'(') {
            return None;
        }
        let label = &self.s[i + 1..close];
        if label.is_empty() {
            return None;
        }
        let u0 = close + 2;
        let lim = (u0 + MAX_SCAN).min(self.b.len());
        let (url, end) = if self.b.get(u0) == Some(&b'<') {
            let e = self.b[u0..lim].iter().position(|&c| c == b'>')? + u0;
            let mut k = e + 1;
            while self.b.get(k) == Some(&b' ') {
                k += 1;
            }
            if self.b.get(k) != Some(&b')') {
                return None;
            }
            (&self.s[u0 + 1..e], k + 1)
        } else {
            let mut k = u0;
            let mut pd = 0usize;
            while k < lim {
                match self.b[k] {
                    b'(' => pd += 1,
                    b')' => {
                        if pd == 0 {
                            break;
                        }
                        pd -= 1;
                    }
                    c if is_ws(c) => break,
                    _ => {}
                }
                k += 1;
            }
            let url = self.s.get(u0..k)?;
            let mut e = k;
            if self.b.get(e) == Some(&b' ') {
                // optional title: ` "title")`
                let t = self.s.get(e..lim)?.trim_start();
                let q = t.strip_prefix('"')?.find('"')? + 1;
                let after = t[q + 1..].trim_start();
                e = lim - after.len();
            }
            if self.b.get(e) != Some(&b')') {
                return None;
            }
            (url, e + 1)
        };
        if !valid_url(url) || url.bytes().any(|c| c.is_ascii_control()) {
            return None;
        }
        let children = if self.depth >= MAX_DEPTH {
            vec![Node::text(label)]
        } else {
            parse_inline(label, self.depth + 1, true)
        };
        Some((
            Node::Link {
                url: url.to_string(),
                kind: LinkKind::Masked,
                children,
            },
            end - i,
        ))
    }

    fn bare_url(&self, i: usize) -> Option<(Node, usize)> {
        let rest = &self.s[i..];
        if !(rest.starts_with("http://") || rest.starts_with("https://")) {
            return None;
        }
        if char_before(self.s, i).is_some_and(|c| c.is_alphanumeric()) {
            return None;
        }
        let mut end = rest
            .bytes()
            .take(MAX_SCAN)
            .position(|c| is_ws(c) || c == b'<' || c == b'|' || c.is_ascii_control())
            .unwrap_or_else(|| rest.len().min(MAX_SCAN));
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        loop {
            let u = &rest[..end];
            let Some(last) = u.chars().next_back() else {
                break;
            };
            // Q90: an autolink never ends in `>` or `.,:;"')]` (simple-markdown parity).
            let trim = matches!(
                last,
                '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | '*' | '_' | '~' | ')' | ']' | '>'
            );
            if trim {
                end -= last.len_utf8();
            } else {
                break;
            }
        }
        let url = &rest[..end];
        if !valid_url(url) {
            return None;
        }
        Some((
            Node::Link {
                url: url.to_string(),
                kind: LinkKind::Auto,
                children: vec![Node::text(url)],
            },
            end,
        ))
    }

    fn emphasis(&mut self, i: usize) -> Option<(Node, usize)> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        let c = self.b[i];
        let r = self.b[i..].iter().take(3).take_while(|&&x| x == c).count();
        let cands: &[usize] = match c {
            // Q90: for `*`, a triple run tries the single-star italic before the double-star
            // bold, so `***a** b*` is italic(bold(a), " b") (simple-markdown order).
            b'*' => match r {
                1 => &[1],
                2 => &[2, 1],
                _ => &[3, 1, 2],
            },
            b'_' => match r {
                1 => &[1],
                2 => &[2, 1],
                _ => &[3, 2, 1],
            },
            _ => {
                if r < 2 {
                    return None;
                }
                &[2]
            }
        };
        if c == b'_' && char_before(self.s, i).is_some_and(|p| p.is_alphanumeric()) {
            return None;
        }
        for &len in cands {
            let kind = match (c, len) {
                (b'*', 1) => 0,
                (b'*', 2) => 1,
                (b'*', _) => 2,
                (b'_', 1) => 3,
                (b'_', 2) => 4,
                (b'_', _) => 5,
                (b'~', _) => 6,
                _ => 7,
            };
            let start = i + len;
            // Q90: a single `*` opener cannot be followed by a lone `*` (`**a*` is a literal
            // `*` then italic `a`); only a `**` pair may start the italic content.
            if c == b'*'
                && len == 1
                && self.b.get(start) == Some(&b'*')
                && run_len(self.b, start, b'*') < 2
            {
                continue;
            }
            if start >= self.fail[kind] {
                continue;
            }
            match self.find_closer(c, len, start) {
                Some(close) => {
                    let inner = &self.s[start..close];
                    let children = parse_inline(inner, self.depth + 1, self.in_link);
                    let node = match (c, len) {
                        (b'*', 1) | (b'_', 1) => Node::Italic { children },
                        (b'*', 2) => Node::Bold { children },
                        (b'_', 2) => Node::Underline { children },
                        (b'*', _) => Node::Bold {
                            children: vec![Node::Italic { children }],
                        },
                        (b'_', _) => Node::Underline {
                            children: vec![Node::Italic { children }],
                        },
                        (b'~', _) => Node::Strike { children },
                        _ => Node::Spoiler { children },
                    };
                    return Some((node, close + len - i));
                }
                None => self.fail[kind] = self.fail[kind].min(start),
            }
        }
        None
    }

    /// Find the index of the closing delimiter of `len` copies of `c`.
    fn find_closer(&mut self, c: u8, len: usize, start: usize) -> Option<usize> {
        let spacey = matches!(c, b'*' | b'_');
        if start >= self.b.len() || (spacey && is_ws(self.b[start])) {
            return None;
        }
        let mut j = start;
        while j < self.b.len() {
            let x = self.b[j];
            if x == b'\\' {
                j += 2;
            } else if x == b'`' {
                let n = run_len(self.b, j, b'`');
                match self.find_code_close(j + n, n) {
                    Some(e) => j = e + n,
                    None => j += n,
                }
            } else if x == c {
                let k = run_len(self.b, j, c);
                if k >= len && (len > 1 || k == 1) {
                    let at = j + k - len;
                    if at > start
                        && !(spacey && is_ws(self.b[at - 1]))
                        && !(c == b'_' && len == 1 && self.alnum_after(at + 1))
                    {
                        return Some(at);
                    }
                }
                j += k;
            } else {
                j += 1;
            }
        }
        None
    }

    fn alnum_after(&self, i: usize) -> bool {
        self.s
            .get(i..)
            .and_then(|r| r.chars().next())
            .is_some_and(|c| c.is_alphanumeric())
    }
}

fn valid_url(u: &str) -> bool {
    let rest = u
        .strip_prefix("https://")
        .or_else(|| u.strip_prefix("http://"));
    match rest {
        Some(r) => !r.is_empty() && !r.starts_with('/') && !r.starts_with(' '),
        None => false,
    }
}

fn parse_token(inner: &str) -> Option<Node> {
    if let Some(r) = inner.strip_prefix("@&") {
        return parse_u64(r).map(|id| Node::RoleMention { id });
    }
    if let Some(r) = inner.strip_prefix('@') {
        let r = r.strip_prefix('!').unwrap_or(r);
        return parse_u64(r).map(|id| Node::UserMention { id });
    }
    if let Some(r) = inner.strip_prefix('#') {
        return parse_u64(r).map(|id| Node::ChannelMention { id });
    }
    if let Some(r) = inner.strip_prefix("t:") {
        let mut it = r.splitn(2, ':');
        let num = it.next()?;
        let digits = num.strip_prefix('-').unwrap_or(num);
        if digits.is_empty() || digits.len() > 15 || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let unix: i64 = num.parse().ok()?;
        let style = match it.next() {
            None => None,
            Some(s) => {
                let mut ch = s.chars();
                let c = ch.next()?;
                if ch.next().is_some() {
                    return None;
                }
                Some(TimestampStyle::from_char(c)?)
            }
        };
        return Some(Node::Timestamp { unix, style });
    }
    if let Some(r) = inner.strip_prefix("cinema:") {
        return valid_ref(r).then(|| Node::Cinema { id: r.to_string() });
    }
    if let Some(r) = inner.strip_prefix("library:") {
        return valid_ref(r).then(|| Node::Library { id: r.to_string() });
    }
    if let Some(r) = inner.strip_prefix('/') {
        let (name, id) = r.rsplit_once(':')?;
        let id = parse_u64(id)?;
        let n = name.chars().count();
        let ok = (1..=32).contains(&n)
            && !name.starts_with(' ')
            && !name.ends_with(' ')
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' '));
        return ok.then(|| Node::SlashCommand {
            name: name.to_string(),
            id,
        });
    }
    let parts: Vec<&str> = inner.split(':').collect();
    // custom emoji
    if parts.len() == 3 && matches!(parts[0], "" | "a") {
        let name = parts[1];
        if (2..=32).contains(&name.len())
            && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            return parse_u64(parts[2]).map(|id| Node::CustomEmoji {
                name: name.to_string(),
                id,
                animated: parts[0] == "a",
            });
        }
        return None;
    }
    // guild navigation
    if (parts.len() == 2 || parts.len() == 3) && parse_u64(parts[0]).is_some() {
        let kind = parts[1];
        if matches!(kind, "customize" | "browse" | "guide" | "linked-roles") {
            let id = match parts.get(2) {
                Some(p) if kind == "linked-roles" => Some(parse_u64(p)?),
                Some(_) => return None,
                None => None,
            };
            return Some(Node::GuildNav {
                kind: kind.to_string(),
                id,
            });
        }
    }
    None
}

fn valid_ref(r: &str) -> bool {
    (1..=64).contains(&r.len())
        && r.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
