//! Reading previews out of HTML (`<title>`, OpenGraph, Twitter cards, the oEmbed
//! discovery link) and oEmbed JSON. Parsing uses `scraper` (html5ever), a pure
//! Rust HTML5 parser; only the document head is parsed.

use scraper::{Html, Selector};
use std::collections::HashMap;
use url::Url;

/// What a page says about itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageMeta {
    pub title: Option<String>,
    pub description: Option<String>,
    pub site_name: Option<String>,
    pub image: Option<String>,
    pub image_width: Option<u32>,
    pub image_height: Option<u32>,
    pub video: Option<String>,
    pub video_width: Option<u32>,
    pub video_height: Option<u32>,
    pub og_type: Option<String>,
    pub twitter_card: Option<String>,
    pub theme_color: Option<u32>,
    /// `href` of `<link rel="alternate" type="application/json+oembed">`, resolved.
    pub oembed_json: Option<String>,
}

/// The oEmbed fields the preview uses. `html` is deliberately not read: embedding
/// third-party markup is never safe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OEmbed {
    pub kind: Option<String>,
    pub title: Option<String>,
    pub author_name: Option<String>,
    pub provider_name: Option<String>,
    pub provider_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub thumbnail_width: Option<u32>,
    pub thumbnail_height: Option<u32>,
    pub url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Parse only the head: `</head>` when it is in the document, else a bounded prefix.
/// This also keeps pathological body nesting away from the DOM builder.
const HEAD_FALLBACK_BYTES: usize = 128 * 1024;

/// Decode `bytes` using the `Content-Type` charset, a BOM, a `<meta charset>` in the
/// first kilobyte, or UTF-8, in that order. Invalid sequences become U+FFFD.
pub fn decode_html(bytes: &[u8], content_type: Option<&str>) -> String {
    let from_header = content_type.and_then(|ct| {
        ct.split(';').skip(1).find_map(|p| {
            let (k, v) = p.split_once('=')?;
            k.trim()
                .eq_ignore_ascii_case("charset")
                .then(|| v.trim().trim_matches('"').to_owned())
        })
    });
    let label = from_header.or_else(|| sniff_meta_charset(bytes));
    let encoding = encoding_rs::Encoding::for_bom(bytes)
        .map(|(e, _)| e)
        .or_else(|| {
            label
                .as_deref()
                .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        })
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(bytes).0.into_owned()
}

fn sniff_meta_charset(bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
    let at = head.find("charset")?;
    let rest = head[at + "charset".len()..].trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let rest = rest.trim_start_matches(['"', '\'']);
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| rest[..end].to_owned())
}

/// Most tags the head slice may contain. Real heads have a few hundred; the cap
/// bounds the DOM builder's work on adversarial nesting (it is superlinear).
const HEAD_MAX_TAGS: usize = 3000;

fn head_slice(html: &str) -> &str {
    let bytes = html.as_bytes();
    let mut end = bytes.len().min(HEAD_FALLBACK_BYTES);
    let mut tags = 0;
    let mut i = 0;
    while i < end {
        if bytes[i] == b'<' {
            tags += 1;
            let rest = &bytes[i..end.min(i + 6)];
            // The head ends at `</head` or where the body starts.
            if tags > HEAD_MAX_TAGS
                || rest.len() >= 6 && rest[..6].eq_ignore_ascii_case(b"</head")
                || rest.len() >= 5 && rest[..5].eq_ignore_ascii_case(b"<body")
            {
                end = i;
                break;
            }
        }
        i += 1;
    }
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    &html[..end]
}

/// Collapse whitespace, drop control characters, trim, and cut to `max` characters
/// (with an ellipsis). `None` when nothing is left.
pub fn clean_text(raw: &str, max: usize) -> Option<String> {
    let mut out = String::with_capacity(raw.len().min(max * 4));
    let mut pending_space = false;
    for c in raw.chars() {
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if c.is_control() {
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(c);
    }
    if out.is_empty() {
        return None;
    }
    if out.chars().count() > max {
        let cut: String = out.chars().take(max.saturating_sub(1)).collect();
        return Some(format!("{}\u{2026}", cut.trim_end()));
    }
    Some(out)
}

/// `raw` resolved against `base`, kept only when it is a plain http(s) URL.
pub fn absolute_url(base: &Url, raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > crate::policy::MAX_URL_LEN {
        return None;
    }
    let u = base.join(raw).ok()?;
    (matches!(u.scheme(), "http" | "https")
        && u.username().is_empty()
        && u.password().is_none()
        && u.host().is_some()
        && u.as_str().len() <= crate::policy::MAX_URL_LEN)
        .then(|| u.into())
}

fn parse_dim(v: Option<&String>) -> Option<u32> {
    v?.trim()
        .parse::<u32>()
        .ok()
        .filter(|n| (1..=20_000).contains(n))
}

fn parse_color(v: &str) -> Option<u32> {
    let h = v.trim().strip_prefix('#')?;
    let full = match h.len() {
        6 => h.to_owned(),
        3 => h.chars().flat_map(|c| [c, c]).collect(),
        _ => return None,
    };
    u32::from_str_radix(&full, 16).ok()
}

/// Read the preview-relevant metadata of an HTML document. `base` is the URL the
/// document was fetched from; relative URLs resolve against it.
pub fn parse_html(html: &str, base: &Url) -> PageMeta {
    let doc = Html::parse_document(head_slice(html));
    let meta_sel = Selector::parse("meta").expect("static selector");
    let title_sel = Selector::parse("title").expect("static selector");
    let link_sel = Selector::parse("link").expect("static selector");

    // First occurrence wins, like most scrapers: pages put the canonical tags first.
    let mut tags: HashMap<String, String> = HashMap::new();
    for m in doc.select(&meta_sel) {
        let el = m.value();
        let key = el
            .attr("property")
            .or_else(|| el.attr("name"))
            .or_else(|| el.attr("itemprop"));
        if let (Some(k), Some(v)) = (key, el.attr("content")) {
            tags.entry(k.trim().to_ascii_lowercase())
                .or_insert_with(|| v.to_owned());
        }
    }
    let get = |keys: &[&str]| keys.iter().find_map(|k| tags.get(*k));

    let title_tag = doc
        .select(&title_sel)
        .next()
        .map(|t| t.text().collect::<String>());
    let image = get(&[
        "og:image",
        "og:image:url",
        "og:image:secure_url",
        "twitter:image",
        "twitter:image:src",
    ])
    .and_then(|v| absolute_url(base, v));
    let video = get(&[
        "og:video:secure_url",
        "og:video:url",
        "og:video",
        "twitter:player:stream",
    ])
    .and_then(|v| absolute_url(base, v));
    let oembed_json = doc.select(&link_sel).find_map(|l| {
        let el = l.value();
        let rel = el.attr("rel")?.to_ascii_lowercase();
        let ty = el.attr("type")?.to_ascii_lowercase();
        (rel.split_whitespace().any(|r| r == "alternate") && ty == "application/json+oembed")
            .then(|| absolute_url(base, el.attr("href")?))
            .flatten()
    });

    PageMeta {
        title: get(&["og:title", "twitter:title"])
            .map(String::as_str)
            .or(title_tag.as_deref())
            .and_then(|t| clean_text(t, 256)),
        description: get(&["og:description", "twitter:description", "description"])
            .and_then(|d| clean_text(d, 350)),
        site_name: get(&["og:site_name", "application-name"]).and_then(|s| clean_text(s, 100)),
        image,
        image_width: parse_dim(tags.get("og:image:width")),
        image_height: parse_dim(tags.get("og:image:height")),
        video,
        video_width: parse_dim(tags.get("og:video:width")),
        video_height: parse_dim(tags.get("og:video:height")),
        og_type: tags.get("og:type").map(|s| s.trim().to_ascii_lowercase()),
        twitter_card: tags
            .get("twitter:card")
            .map(|s| s.trim().to_ascii_lowercase()),
        theme_color: tags.get("theme-color").and_then(|c| parse_color(c)),
        oembed_json,
    }
}

/// Read an oEmbed JSON document. Unknown or mistyped fields are ignored.
pub fn parse_oembed(json: &[u8], base: &Url) -> Option<OEmbed> {
    let v: serde_json::Value = serde_json::from_slice(json).ok()?;
    let s = |k: &str, max: usize| v.get(k)?.as_str().and_then(|t| clean_text(t, max));
    let u = |k: &str| v.get(k)?.as_str().and_then(|t| absolute_url(base, t));
    let n = |k: &str| {
        let x = v.get(k)?;
        x.as_u64()
            .or_else(|| x.as_str()?.parse().ok())
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| (1..=20_000).contains(n))
    };
    Some(OEmbed {
        kind: s("type", 16).map(|t| t.to_ascii_lowercase()),
        title: s("title", 256),
        author_name: s("author_name", 256),
        provider_name: s("provider_name", 100),
        provider_url: u("provider_url"),
        thumbnail_url: u("thumbnail_url"),
        thumbnail_width: n("thumbnail_width"),
        thumbnail_height: n("thumbnail_height"),
        url: u("url"),
        width: n("width"),
        height: n("height"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/articles/1").unwrap()
    }

    #[test]
    fn opengraph_page() {
        let html = r##"<!doctype html><html><head>
            <title>Fallback title</title>
            <meta property="og:title" content="  The   Title ">
            <meta property="og:description" content="A &amp; B">
            <meta property="og:site_name" content="Example">
            <meta property="og:image" content="/img/a.png">
            <meta property="og:image:width" content="800">
            <meta property="og:image:height" content="400">
            <meta name="theme-color" content="#ff8800">
            <meta property="og:title" content="Second wins nothing">
            <link rel="alternate" type="application/json+oembed" href="/oembed?u=1">
            </head><body><p>hi</p></body></html>"##;
        let m = parse_html(html, &base());
        assert_eq!(m.title.as_deref(), Some("The Title"));
        assert_eq!(m.description.as_deref(), Some("A & B"));
        assert_eq!(m.site_name.as_deref(), Some("Example"));
        assert_eq!(m.image.as_deref(), Some("https://example.com/img/a.png"));
        assert_eq!((m.image_width, m.image_height), (Some(800), Some(400)));
        assert_eq!(m.theme_color, Some(0xff8800));
        assert_eq!(
            m.oembed_json.as_deref(),
            Some("https://example.com/oembed?u=1")
        );
    }

    #[test]
    fn twitter_card_and_title_fallback() {
        let html = r#"<head><title>Just a title</title>
            <meta name="twitter:card" content="summary_large_image">
            <meta name="twitter:image" content="https://cdn.example.net/x.jpg">
            <meta name="twitter:description" content="tw desc"></head>"#;
        let m = parse_html(html, &base());
        assert_eq!(m.title.as_deref(), Some("Just a title"));
        assert_eq!(m.description.as_deref(), Some("tw desc"));
        assert_eq!(m.twitter_card.as_deref(), Some("summary_large_image"));
        assert_eq!(m.image.as_deref(), Some("https://cdn.example.net/x.jpg"));
    }

    #[test]
    fn unsafe_urls_are_dropped() {
        let html = r#"<head>
            <meta property="og:image" content="javascript:alert(1)">
            <meta property="og:video" content="data:video/mp4;base64,AAAA">
            <link rel="alternate" type="application/json+oembed" href="file:///etc/passwd"></head>"#;
        let m = parse_html(html, &base());
        assert_eq!(m.image, None);
        assert_eq!(m.video, None);
        assert_eq!(m.oembed_json, None);
        assert_eq!(absolute_url(&base(), "https://u:p@x.test/"), None);
    }

    #[test]
    fn body_is_not_parsed_and_garbage_does_not_panic() {
        let deep = format!(
            "<head><title>t</title></head><body>{}</body>",
            "<div>".repeat(50_000)
        );
        assert_eq!(parse_html(&deep, &base()).title.as_deref(), Some("t"));
        let _ = parse_html("<<<>>><meta <title", &base());
        let _ = parse_html("", &base());
        let no_head_close = format!("<title>x</title>{}", "<div>".repeat(100_000));
        assert_eq!(
            parse_html(&no_head_close, &base()).title.as_deref(),
            Some("x")
        );
    }

    #[test]
    fn text_is_cleaned_and_truncated() {
        assert_eq!(clean_text("  a\n\n b\u{0}c  ", 10).as_deref(), Some("a bc"));
        assert_eq!(clean_text("   \n", 10), None);
        let long = "x".repeat(400);
        let t = clean_text(&long, 350).unwrap();
        assert_eq!(t.chars().count(), 350);
        assert!(t.ends_with('\u{2026}'));
    }

    #[test]
    fn charsets() {
        let latin1 = b"<head><meta charset=\"iso-8859-1\"><title>caf\xe9</title></head>";
        assert!(decode_html(latin1, None).contains("caf\u{e9}"));
        let header = decode_html(
            b"<title>caf\xe9</title>",
            Some("text/html; charset=ISO-8859-1"),
        );
        assert!(header.contains("caf\u{e9}"));
        assert!(decode_html("<title>é</title>".as_bytes(), None).contains('é'));
    }

    #[test]
    fn oembed_json() {
        let j = br#"{"type":"video","title":"Clip","author_name":"Me","provider_name":"Tube",
            "provider_url":"https://tube.example/","thumbnail_url":"https://tube.example/t.jpg",
            "thumbnail_width":"480","thumbnail_height":360,"html":"<iframe src=x>"}"#;
        let o = parse_oembed(j, &base()).unwrap();
        assert_eq!(o.kind.as_deref(), Some("video"));
        assert_eq!(o.title.as_deref(), Some("Clip"));
        assert_eq!(o.thumbnail_width, Some(480));
        assert_eq!(o.thumbnail_height, Some(360));
        assert!(parse_oembed(b"not json", &base()).is_none());
    }
}
