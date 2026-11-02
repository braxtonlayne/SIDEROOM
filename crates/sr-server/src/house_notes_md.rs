//! Restricted markdown for the `house-notes` Entrance copy (FU-22, D-239, D-385).
//!
//! The same subset the app's `EntranceMarkdown` draws for the other two copy keys: paragraphs, headings, one-level
//! lists, quotes, fenced code, pipe tables, bold, italic, inline code and links. The output is static markup (the
//! public page stays script-free). Every text node goes through [`escape_html`], so raw HTML, images and autolinks
//! stay visible text. `apps/web/vite/houseNotesMd.ts` is the line-for-line mirror used by the Vite dev server; the
//! fixtures in `tests/fixtures/house-notes` are read by both, so the two are proven to agree.

use crate::web::escape_html;

/// Render `copy` as HTML. Empty or blank copy gives an empty string.
pub fn render(copy: &str) -> String {
    let copy = copy.replace("\r\n", "\n").replace('\r', "\n");
    let mut r = Renderer::default();
    for raw in copy.split('\n') {
        r.line(raw);
    }
    r.finish()
}

#[derive(Default)]
struct List {
    ordered: bool,
    start: u64,
    items: Vec<String>,
}

#[derive(Default)]
struct Renderer {
    out: String,
    para: Vec<String>,
    quote: Vec<String>,
    table: Vec<String>,
    list: Option<List>,
    fence: Option<(usize, Vec<String>)>,
    ids: Vec<String>,
}

impl Renderer {
    fn flush_para(&mut self) {
        if !self.para.is_empty() {
            let lines: Vec<String> = self.para.drain(..).map(|l| inline(&l)).collect();
            self.out
                .push_str(&format!("<p>{}</p>\n", lines.join("<br>")));
        }
    }

    fn flush_quote(&mut self) {
        if !self.quote.is_empty() {
            let text = self.quote.drain(..).collect::<Vec<_>>().join(" ");
            self.out.push_str(&format!(
                "<blockquote><p>{}</p></blockquote>\n",
                inline(&text)
            ));
        }
    }

    fn flush_table(&mut self) {
        if self.table.is_empty() {
            return;
        }
        let rows: Vec<String> = self
            .table
            .drain(..)
            .filter(|r| !is_separator_row(r))
            .collect();
        let mut it = rows.iter();
        self.out.push_str("<table>\n");
        if let Some(head) = it.next() {
            self.out.push_str("<thead><tr>");
            for c in cells(head) {
                self.out
                    .push_str(&format!("<th scope=\"col\">{}</th>", inline(&c)));
            }
            self.out.push_str("</tr></thead>\n");
        }
        self.out.push_str("<tbody>\n");
        for r in it {
            self.out.push_str("<tr>");
            for c in cells(r) {
                self.out.push_str(&format!("<td>{}</td>", inline(&c)));
            }
            self.out.push_str("</tr>\n");
        }
        self.out.push_str("</tbody>\n</table>\n");
    }

    fn flush_list(&mut self) {
        if let Some(l) = self.list.take() {
            let tag = if l.ordered { "ol" } else { "ul" };
            if l.ordered && l.start != 1 {
                self.out.push_str(&format!("<ol start=\"{}\">\n", l.start));
            } else {
                self.out.push_str(&format!("<{tag}>\n"));
            }
            for it in &l.items {
                self.out.push_str(&format!("<li>{}</li>\n", inline(it)));
            }
            self.out.push_str(&format!("</{tag}>\n"));
        }
    }

    fn flush_all(&mut self) {
        self.flush_para();
        self.flush_quote();
        self.flush_table();
        self.flush_list();
    }

    fn heading_id(&mut self, text: &str) -> Option<String> {
        let slug = slug(text);
        if slug.is_empty() {
            return None;
        }
        let mut id = format!("op-{slug}");
        let mut n = 2;
        while self.ids.contains(&id) {
            id = format!("op-{slug}-{n}");
            n += 1;
        }
        self.ids.push(id.clone());
        Some(id)
    }

    fn line(&mut self, raw: &str) {
        let line = raw.trim_end();
        let lead = line.len() - line.trim_start().len();
        let fence_mark = line.trim_start().starts_with("```");
        if let Some((indent, lines)) = self.fence.as_mut() {
            if fence_mark {
                let code = lines.join("\n");
                self.fence = None;
                self.out
                    .push_str(&format!("<pre><code>{}</code></pre>\n", escape_html(&code)));
            } else {
                let cut = (*indent).min(lead);
                lines.push(line[cut..].to_owned());
            }
            return;
        }
        if fence_mark {
            self.flush_all();
            self.fence = Some((lead, Vec::new()));
            return;
        }
        let heading = heading_of(line);
        let bullet = bullet_of(line);
        let numbered = numbered_of(line);
        let quoted = line.strip_prefix('>');
        if line.trim().is_empty() {
            self.flush_para();
            self.flush_quote();
            self.flush_table();
        } else if let Some((level, text)) = heading {
            self.flush_all();
            let id = self.heading_id(text);
            let attr = id.map(|i| format!(" id=\"{i}\"")).unwrap_or_default();
            self.out
                .push_str(&format!("<h{level}{attr}>{}</h{level}>\n", inline(text)));
        } else if let Some(q) = quoted {
            self.flush_para();
            self.flush_table();
            self.flush_list();
            let q = q.strip_prefix(|c: char| c.is_whitespace()).unwrap_or(q);
            self.quote.push(q.trim().to_owned());
        } else if line.trim_start().starts_with('|') {
            self.flush_para();
            self.flush_quote();
            self.flush_list();
            self.table.push(line.to_owned());
        } else if bullet.is_some() || numbered.is_some() {
            self.flush_para();
            self.flush_quote();
            self.flush_table();
            let ordered = bullet.is_none();
            if self.list.as_ref().is_some_and(|l| l.ordered != ordered) {
                self.flush_list();
            }
            let (start, text) = match (bullet, numbered) {
                (Some(t), _) => (1, t),
                (None, Some((n, t))) => (n, t),
                (None, None) => (1, ""),
            };
            let l = self.list.get_or_insert_with(|| List {
                ordered,
                start,
                items: Vec::new(),
            });
            l.items.push(text.to_owned());
        } else if self.list.is_some() && lead > 0 {
            // An indented paragraph under a list item: shown after the list.
            self.flush_list();
            self.para.push(line.trim().to_owned());
        } else {
            self.flush_list();
            self.flush_quote();
            self.flush_table();
            self.para.push(line.trim().to_owned());
        }
    }

    fn finish(mut self) -> String {
        self.flush_all();
        if let Some((_, lines)) = self.fence.take() {
            self.out.push_str(&format!(
                "<pre><code>{}</code></pre>\n",
                escape_html(&lines.join("\n"))
            ));
        }
        self.out
    }
}

/// One to four `#` then whitespace then the text; `#` is `h2` (the page owns its `h1`) and four marks stop at `h4`.
fn heading_of(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if !(1..=4).contains(&hashes) {
        return None;
    }
    let rest = &line[hashes..];
    let text = rest.trim_start();
    (text.len() < rest.len()).then_some(((hashes + 1).min(4), text))
}

fn bullet_of(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = t.strip_prefix(['-', '*'])?;
    let text = rest.trim_start();
    (text.len() < rest.len() && !text.is_empty()).then_some(text)
}

fn numbered_of(line: &str) -> Option<(u64, &str)> {
    let t = line.trim_start();
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let rest = t[digits..].strip_prefix(['.', ')'])?;
    let text = rest.trim_start();
    (text.len() < rest.len() && !text.is_empty()).then(|| (t[..digits].parse().unwrap_or(1), text))
}

fn is_separator_row(row: &str) -> bool {
    !row.is_empty()
        && row
            .chars()
            .all(|c| c.is_whitespace() || c == ':' || c == '|' || c == '-')
}

fn cells(row: &str) -> Vec<String> {
    let t = row.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_owned()).collect()
}

/// ASCII lower case, runs of other characters become one dash, trimmed.
fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            dash = true;
        }
    }
    out
}

fn safe_href(href: &str) -> bool {
    let lower = href.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return true;
    }
    if href.starts_with('/') {
        return !href.starts_with("//");
    }
    href.strip_prefix('#').is_some_and(|r| {
        !r.is_empty()
            && r.chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    })
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn first(c: &[char], from: usize, want: char) -> Option<usize> {
    (from..c.len()).find(|&k| c[k] == want)
}

fn span(c: &[char], a: usize, b: usize) -> String {
    c[a..b].iter().collect()
}

/// One inline construct starting at `i`: the HTML and the index after it.
fn construct(c: &[char], i: usize) -> Option<(String, usize)> {
    match c[i] {
        '`' => {
            let k = first(c, i + 1, '`').filter(|&k| k > i + 1)?;
            Some((
                format!("<code>{}</code>", escape_html(&span(c, i + 1, k))),
                k + 1,
            ))
        }
        '*' if c.get(i + 1) == Some(&'*') => {
            let k = first(c, i + 2, '*').filter(|&k| k > i + 2)?;
            (c.get(k + 1) == Some(&'*')).then_some(())?;
            Some((
                format!("<strong>{}</strong>", inline(&span(c, i + 2, k))),
                k + 2,
            ))
        }
        '*' => {
            let n = *c.get(i + 1)?;
            (n != '*' && !n.is_whitespace()).then_some(())?;
            let k = first(c, i + 2, '*')?;
            Some((format!("<em>{}</em>", inline(&span(c, i + 1, k))), k + 1))
        }
        '_' => {
            (i == 0 || !word(c[i - 1])).then_some(())?;
            let n = *c.get(i + 1)?;
            (n != '_' && !n.is_whitespace()).then_some(())?;
            let k = first(c, i + 2, '_')?;
            c.get(k + 1).is_none_or(|a| !word(*a)).then_some(())?;
            Some((format!("<em>{}</em>", inline(&span(c, i + 1, k))), k + 1))
        }
        '[' => {
            (i == 0 || c[i - 1] != '!').then_some(())?; // an image stays text
            let k = first(c, i + 1, ']').filter(|&k| k > i + 1)?;
            (c.get(k + 1) == Some(&'(')).then_some(())?;
            let mut m = k + 2;
            while m < c.len() && c[m] != ')' && !c[m].is_whitespace() {
                m += 1;
            }
            (m > k + 2 && c.get(m) == Some(&')')).then_some(())?;
            let text = escape_html(&span(c, i + 1, k));
            let href = span(c, k + 2, m);
            let html = if safe_href(&href) {
                format!(
                    "<a href=\"{}\" rel=\"noopener noreferrer\">{text}</a>",
                    escape_html(&href)
                )
            } else {
                text
            };
            Some((html, m + 1))
        }
        _ => None,
    }
}

fn inline(text: &str) -> String {
    let c: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        if let Some((html, next)) = construct(&c, i) {
            out.push_str(&html);
            i = next;
        } else {
            out.push_str(&escape_html(&c[i].to_string()));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::render;

    fn fixture(name: &str) -> (String, String) {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/house-notes");
        let read = |ext: &str| {
            std::fs::read_to_string(dir.join(format!("{name}.{ext}")))
                .unwrap_or_else(|e| panic!("fixture {name}.{ext}: {e}"))
        };
        (read("md"), read("html"))
    }

    #[test]
    fn fixtures_match_the_expected_html() {
        for name in ["basic", "links", "headings"] {
            let (md, html) = fixture(name);
            assert_eq!(render(&md), html, "fixture {name}");
        }
    }

    #[test]
    fn empty_copy_is_empty() {
        assert_eq!(render(""), "");
        assert_eq!(render("  \n\n \n"), "");
    }

    #[test]
    fn a_single_newline_is_a_break_and_a_blank_line_a_paragraph() {
        assert_eq!(
            render("one\ntwo\n\nthree"),
            "<p>one<br>two</p>\n<p>three</p>\n"
        );
    }

    #[test]
    fn raw_html_and_placeholders_are_text() {
        let h = render("<script>alert(1)</script> {{operator_contact}}\n\n<img src=x>");
        assert!(!h.contains("<script"), "{h}");
        assert!(!h.contains("<img"), "{h}");
        assert!(
            h.contains("&lt;script&gt;alert(1)&lt;/script&gt; {{operator_contact}}"),
            "{h}"
        );
        assert!(h.contains("&lt;img src=x&gt;"), "{h}");
    }

    #[test]
    fn only_safe_links_become_anchors() {
        let h = render(
            "[a](javascript:alert(1)) [b](//evil.test/x) [c](data:text/html,x) ![d](https://x.test/i.png)",
        );
        assert!(!h.contains("<a "), "{h}");
        let ok =
            render("[a](https://x.test/p?q=1&r=2) [b](/rules) [c](#privacy) [d](HTTP://x.test)");
        assert_eq!(ok.matches("<a href=").count(), 4, "{ok}");
        assert!(ok.contains("rel=\"noopener noreferrer\""));
        assert!(!ok.contains("target="));
        assert!(ok.contains("href=\"https://x.test/p?q=1&amp;r=2\""));
    }

    #[test]
    fn heading_ids_are_prefixed_and_deduped() {
        let h = render("# Privacy\n## Privacy\n### Privacy!\n#### Ünï\n# Rules & Terms");
        assert!(h.contains("<h2 id=\"op-privacy\">Privacy</h2>"), "{h}");
        assert!(h.contains("<h3 id=\"op-privacy-2\">"), "{h}");
        assert!(h.contains("<h4 id=\"op-privacy-3\">"), "{h}");
        assert!(h.contains("<h2 id=\"op-rules-terms\">"), "{h}");
    }
}
