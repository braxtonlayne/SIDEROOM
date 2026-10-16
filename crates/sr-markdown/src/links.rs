//! Links that may get a server-side preview (LNG-MSG-014/015).

use crate::ast::{LinkKind, Node};

/// A link eligible for a preview and whether the author hid it behind a spoiler (`||...||`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewLink {
    pub url: String,
    /// True when every occurrence of the link sits inside a spoiler: its preview must stay
    /// hidden until the reader reveals it (LNG-MSG-015).
    pub spoiler: bool,
}

/// The http(s) links of a parsed message that are eligible for a preview, in
/// reading order and without duplicates. `<https://...>` links are skipped: the
/// author wrote them to suppress the embed. Links inside code never reach the AST
/// as links, so they are skipped too.
pub fn extract_preview_links(nodes: &[Node]) -> Vec<String> {
    extract_preview_links_marked(nodes)
        .into_iter()
        .map(|l| l.url)
        .collect()
}

/// Like [`extract_preview_links`], also telling which links are spoilered. A link written
/// both inside and outside a spoiler counts as visible.
pub fn extract_preview_links_marked(nodes: &[Node]) -> Vec<PreviewLink> {
    let mut out = Vec::new();
    walk(nodes, false, &mut out);
    out
}

fn walk(nodes: &[Node], hidden: bool, out: &mut Vec<PreviewLink>) {
    for n in nodes {
        match n {
            Node::Link {
                url,
                kind,
                children,
            } => {
                let eligible = matches!(kind, LinkKind::Auto | LinkKind::Masked)
                    && (url.starts_with("http://") || url.starts_with("https://"));
                if eligible {
                    match out.iter_mut().find(|l| l.url == *url) {
                        Some(seen) => seen.spoiler &= hidden,
                        None => out.push(PreviewLink {
                            url: url.clone(),
                            spoiler: hidden,
                        }),
                    }
                }
                walk(children, hidden, out);
            }
            Node::List { items, .. } => items.iter().for_each(|i| walk(i, hidden, out)),
            Node::Spoiler { children } => walk(children, true, out),
            Node::Bold { children }
            | Node::Italic { children }
            | Node::Underline { children }
            | Node::Strike { children }
            | Node::Quote { children }
            | Node::Heading { children, .. }
            | Node::Subtext { children } => walk(children, hidden, out),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(s: &str) -> Vec<String> {
        extract_preview_links(&crate::parse(s))
    }

    #[test]
    fn finds_bare_and_masked_links_once() {
        assert_eq!(
            links("see https://a.test/x and [b](https://b.test) and https://a.test/x again"),
            vec!["https://a.test/x", "https://b.test"]
        );
    }

    #[test]
    fn suppressed_and_code_links_are_skipped() {
        assert!(links("<https://a.test>").is_empty());
        assert!(links("`https://a.test`").is_empty());
        assert!(links("```\nhttps://a.test\n```").is_empty());
        assert_eq!(
            links("<https://a.test> https://b.test"),
            vec!["https://b.test"]
        );
    }

    #[test]
    fn links_in_formatting_and_quotes_count() {
        assert_eq!(
            links("**https://a.test**\n> https://b.test\n- https://c.test"),
            vec!["https://a.test", "https://b.test", "https://c.test"]
        );
    }

    #[test]
    fn non_http_schemes_are_ignored() {
        assert!(links("[x](javascript:alert(1)) ftp://a.test mailto:a@b.test").is_empty());
    }

    #[test]
    fn spoilered_links_are_marked_unless_also_written_openly() {
        let marked = |s: &str| {
            extract_preview_links_marked(&crate::parse(s))
                .into_iter()
                .map(|l| (l.url, l.spoiler))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            marked("||https://a.test|| https://b.test"),
            vec![
                ("https://a.test".into(), true),
                ("https://b.test".into(), false)
            ]
        );
        assert_eq!(
            marked("||**https://a.test**|| and https://a.test"),
            vec![("https://a.test".into(), false)]
        );
        assert_eq!(
            marked("https://a.test ||https://a.test||"),
            vec![("https://a.test".into(), false)]
        );
    }
}
