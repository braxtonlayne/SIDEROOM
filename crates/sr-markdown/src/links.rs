//! Links that may get a server-side preview (LNG-MSG-014/015).

use crate::ast::{LinkKind, Node};

/// The http(s) links of a parsed message that are eligible for a preview, in
/// reading order and without duplicates. `<https://...>` links are skipped: the
/// author wrote them to suppress the embed. Links inside code never reach the AST
/// as links, so they are skipped too.
pub fn extract_preview_links(nodes: &[Node]) -> Vec<String> {
    let mut out = Vec::new();
    walk(nodes, &mut out);
    out
}

fn walk(nodes: &[Node], out: &mut Vec<String>) {
    for n in nodes {
        match n {
            Node::Link {
                url,
                kind,
                children,
            } => {
                let eligible = matches!(kind, LinkKind::Auto | LinkKind::Masked)
                    && (url.starts_with("http://") || url.starts_with("https://"));
                if eligible && !out.iter().any(|u| u == url) {
                    out.push(url.clone());
                }
                walk(children, out);
            }
            Node::List { items, .. } => items.iter().for_each(|i| walk(i, out)),
            Node::Bold { children }
            | Node::Italic { children }
            | Node::Underline { children }
            | Node::Strike { children }
            | Node::Spoiler { children }
            | Node::Quote { children }
            | Node::Heading { children, .. }
            | Node::Subtext { children } => walk(children, out),
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
}
