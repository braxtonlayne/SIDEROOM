//! Validation of embeds sent by bots and users (Discord limits, LNG-MSG-002).
//!
//! Link previews are produced by the server (`messages.unfurl`), never taken from
//! a client: whatever a client sends is normalised to a plain `rich` embed.

use crate::error::{ChatError, ChatResult};
use crate::message::{Embed, MAX_EMBED_FIELDS, MAX_EMBEDS};

pub const MAX_TITLE: usize = 256;
pub const MAX_DESCRIPTION: usize = 4096;
pub const MAX_FIELD_NAME: usize = 256;
pub const MAX_FIELD_VALUE: usize = 1024;
pub const MAX_FOOTER_TEXT: usize = 2048;
pub const MAX_AUTHOR_NAME: usize = 256;
/// Characters across all embeds of one message.
pub const MAX_TOTAL_CHARS: usize = 6000;
const MAX_URL: usize = 2048;

fn chars(s: &str) -> usize {
    s.chars().count()
}

fn too_long(field: &str, max: usize) -> ChatError {
    ChatError::invalid(
        field,
        "BASE_TYPE_MAX_LENGTH",
        format!("Must be {max} or fewer in length."),
    )
}

/// `http`/`https` URLs, plus `attachment://name` for embedded images.
fn check_url(field: &str, url: &str, allow_attachment: bool) -> ChatResult<()> {
    let ok = url.len() <= MAX_URL
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
        && (url.starts_with("http://")
            || url.starts_with("https://")
            || (allow_attachment && url.starts_with("attachment://")));
    if ok {
        Ok(())
    } else {
        Err(ChatError::invalid(
            field,
            "INVALID_URL",
            "Scheme must be one of (http, https).",
        ))
    }
}

/// Loose RFC 3339 check: `YYYY-MM-DDTHH:MM:SS` followed by an optional fraction
/// and `Z` or an offset.
fn is_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    if !s.is_ascii() || b.len() < 20 || b.len() > 40 {
        return false;
    }
    let digit = |i: usize| b[i].is_ascii_digit();
    let shape = digit(0)
        && digit(1)
        && digit(2)
        && digit(3)
        && b[4] == b'-'
        && digit(5)
        && digit(6)
        && b[7] == b'-'
        && digit(8)
        && digit(9)
        && (b[10] == b'T' || b[10] == b't' || b[10] == b' ')
        && digit(11)
        && digit(12)
        && b[13] == b':'
        && digit(14)
        && digit(15)
        && b[16] == b':'
        && digit(17)
        && digit(18);
    let month: u32 = s[5..7].parse().unwrap_or(0);
    let day: u32 = s[8..10].parse().unwrap_or(0);
    let hour: u32 = s[11..13].parse().unwrap_or(99);
    let min: u32 = s[14..16].parse().unwrap_or(99);
    let sec: u32 = s[17..19].parse().unwrap_or(99);
    shape
        && (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && hour < 24
        && min < 60
        && sec < 61
        && (s.ends_with('Z') || s.ends_with('z') || s[19..].contains('+') || s[19..].contains('-'))
}

/// Validate `embeds` in place and return the total number of counted characters.
/// Normalisation: type is `rich`, `provider` and `video` are dropped (only the
/// server sets those), and empty strings become absent.
pub fn validate_embeds(embeds: &mut [Embed]) -> ChatResult<usize> {
    if embeds.len() > MAX_EMBEDS {
        return Err(too_long("embeds", MAX_EMBEDS));
    }
    let mut total = 0usize;
    for (i, e) in embeds.iter_mut().enumerate() {
        let p = |s: &str| format!("embeds.{i}.{s}");
        e.kind = Some("rich".into());
        e.provider = None;
        e.sr_spoiler = false;
        e.video = None;
        for text in [&mut e.title, &mut e.description] {
            if text.as_deref().is_some_and(str::is_empty) {
                *text = None;
            }
        }
        if let Some(t) = &e.title {
            if chars(t) > MAX_TITLE {
                return Err(too_long(&p("title"), MAX_TITLE));
            }
            total += chars(t);
        }
        if let Some(d) = &e.description {
            if chars(d) > MAX_DESCRIPTION {
                return Err(too_long(&p("description"), MAX_DESCRIPTION));
            }
            total += chars(d);
        }
        if let Some(u) = &e.url {
            check_url(&p("url"), u, false)?;
        }
        if let Some(ts) = &e.timestamp
            && !is_timestamp(ts)
        {
            return Err(ChatError::invalid(
                &p("timestamp"),
                "DATE_TIME_TYPE_PARSE",
                "Could not parse the timestamp.",
            ));
        }
        if e.color > 0x00ff_ffff {
            return Err(ChatError::invalid(
                &p("color"),
                "NUMBER_TYPE_MAX",
                "Must be between 0 and 16777215.",
            ));
        }
        if e.fields.len() > MAX_EMBED_FIELDS {
            return Err(too_long(&p("fields"), MAX_EMBED_FIELDS));
        }
        for (j, f) in e.fields.iter().enumerate() {
            let fp = |s: &str| format!("embeds.{i}.fields.{j}.{s}");
            if f.name.trim().is_empty() {
                return Err(ChatError::invalid(
                    &fp("name"),
                    "BASE_TYPE_REQUIRED",
                    "This field is required",
                ));
            }
            if f.value.trim().is_empty() {
                return Err(ChatError::invalid(
                    &fp("value"),
                    "BASE_TYPE_REQUIRED",
                    "This field is required",
                ));
            }
            if chars(&f.name) > MAX_FIELD_NAME {
                return Err(too_long(&fp("name"), MAX_FIELD_NAME));
            }
            if chars(&f.value) > MAX_FIELD_VALUE {
                return Err(too_long(&fp("value"), MAX_FIELD_VALUE));
            }
            total += chars(&f.name) + chars(&f.value);
        }
        if let Some(foot) = &e.footer {
            if foot.text.trim().is_empty() {
                return Err(ChatError::invalid(
                    &p("footer.text"),
                    "BASE_TYPE_REQUIRED",
                    "This field is required",
                ));
            }
            if chars(&foot.text) > MAX_FOOTER_TEXT {
                return Err(too_long(&p("footer.text"), MAX_FOOTER_TEXT));
            }
            if let Some(u) = &foot.icon_url {
                check_url(&p("footer.icon_url"), u, true)?;
            }
            total += chars(&foot.text);
        }
        if let Some(a) = &e.author {
            if a.name.trim().is_empty() {
                return Err(ChatError::invalid(
                    &p("author.name"),
                    "BASE_TYPE_REQUIRED",
                    "This field is required",
                ));
            }
            if chars(&a.name) > MAX_AUTHOR_NAME {
                return Err(too_long(&p("author.name"), MAX_AUTHOR_NAME));
            }
            for (k, u) in [("url", &a.url), ("icon_url", &a.icon_url)] {
                if let Some(u) = u {
                    check_url(&p(&format!("author.{k}")), u, k == "icon_url")?;
                }
            }
            total += chars(&a.name);
        }
        for (k, m) in [("image", &e.image), ("thumbnail", &e.thumbnail)] {
            if let Some(url) = m.as_ref().and_then(|m| m.url.as_ref()) {
                check_url(&p(&format!("{k}.url")), url, true)?;
            }
        }
    }
    if total > MAX_TOTAL_CHARS {
        return Err(ChatError::invalid(
            "embeds",
            "EMBED_SIZE_EXCEEDS_MAXIMUM",
            format!("Embed size exceeds maximum size of {MAX_TOTAL_CHARS}."),
        ));
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{EmbedAuthor, EmbedField, EmbedFooter, EmbedMedia, EmbedProvider};

    fn embed() -> Embed {
        Embed {
            title: Some("t".into()),
            ..Embed::default()
        }
    }

    fn field_of(r: ChatResult<usize>) -> String {
        match r {
            Err(ChatError::Invalid { field, .. }) => field,
            other => panic!("expected an Invalid error, got {other:?}"),
        }
    }

    #[test]
    fn valid_embed_is_normalised() {
        let mut v = vec![Embed {
            kind: Some("link".into()),
            provider: Some(EmbedProvider::default()),
            video: Some(EmbedMedia::default()),
            sr_spoiler: true,
            title: Some("hello".into()),
            description: Some(String::new()),
            timestamp: Some("2026-01-02T03:04:05.678Z".into()),
            color: 0xabcdef,
            url: Some("https://example.com".into()),
            image: Some(EmbedMedia {
                url: Some("attachment://cat.png".into()),
                ..EmbedMedia::default()
            }),
            ..Embed::default()
        }];
        assert_eq!(validate_embeds(&mut v).unwrap(), 5);
        assert_eq!(v[0].kind.as_deref(), Some("rich"));
        assert!(v[0].provider.is_none() && v[0].video.is_none());
        assert!(!v[0].sr_spoiler);
        assert!(v[0].description.is_none());
        assert!(!v[0].is_unfurled());
    }

    #[test]
    fn count_and_field_limits() {
        let mut eleven = vec![embed(); 11];
        assert_eq!(field_of(validate_embeds(&mut eleven)), "embeds");
        let mut e = embed();
        e.fields = vec![
            EmbedField {
                name: "n".into(),
                value: "v".into(),
                inline: false
            };
            26
        ];
        assert_eq!(field_of(validate_embeds(&mut [e])), "embeds.0.fields");
    }

    #[test]
    fn length_limits_table() {
        let long = |n: usize| "x".repeat(n);
        let cases: Vec<(&str, Embed)> = vec![
            (
                "embeds.0.title",
                Embed {
                    title: Some(long(257)),
                    ..embed()
                },
            ),
            (
                "embeds.0.description",
                Embed {
                    description: Some(long(4097)),
                    ..embed()
                },
            ),
            (
                "embeds.0.fields.0.name",
                Embed {
                    fields: vec![EmbedField {
                        name: long(257),
                        value: "v".into(),
                        inline: false,
                    }],
                    ..embed()
                },
            ),
            (
                "embeds.0.fields.0.value",
                Embed {
                    fields: vec![EmbedField {
                        name: "n".into(),
                        value: long(1025),
                        inline: false,
                    }],
                    ..embed()
                },
            ),
            (
                "embeds.0.footer.text",
                Embed {
                    footer: Some(EmbedFooter {
                        text: long(2049),
                        icon_url: None,
                    }),
                    ..embed()
                },
            ),
            (
                "embeds.0.author.name",
                Embed {
                    author: Some(EmbedAuthor {
                        name: long(257),
                        url: None,
                        icon_url: None,
                    }),
                    ..embed()
                },
            ),
        ];
        for (want, e) in cases {
            assert_eq!(field_of(validate_embeds(&mut [e])), want);
        }
        // Boundaries pass.
        let mut ok = Embed {
            title: Some(long(256)),
            description: Some(long(4096)),
            ..Embed::default()
        };
        ok.fields = vec![EmbedField {
            name: long(256),
            value: long(1024),
            inline: true,
        }];
        // 256 + 4096 + 1280 = 5632 <= 6000
        assert_eq!(validate_embeds(&mut [ok]).unwrap(), 5632);
    }

    #[test]
    fn total_size_is_capped_across_embeds() {
        let mut v = vec![
            Embed {
                description: Some("x".repeat(4000)),
                ..Embed::default()
            },
            Embed {
                description: Some("y".repeat(2001)),
                ..Embed::default()
            },
        ];
        assert_eq!(field_of(validate_embeds(&mut v)), "embeds");
        v[1].description = Some("y".repeat(2000));
        assert_eq!(validate_embeds(&mut v).unwrap(), 6000);
    }

    #[test]
    fn urls_timestamps_and_colors() {
        for bad in [
            "javascript:alert(1)",
            "data:text/html,x",
            "ftp://x.test",
            "//x.test",
            "https://a b.test",
        ] {
            let e = Embed {
                url: Some(bad.into()),
                ..embed()
            };
            assert_eq!(field_of(validate_embeds(&mut [e])), "embeds.0.url", "{bad}");
        }
        let e = Embed {
            thumbnail: Some(EmbedMedia {
                url: Some("javascript:x".into()),
                ..EmbedMedia::default()
            }),
            ..embed()
        };
        assert_eq!(
            field_of(validate_embeds(&mut [e])),
            "embeds.0.thumbnail.url"
        );
        let e = Embed {
            timestamp: Some("yesterday".into()),
            ..embed()
        };
        assert_eq!(field_of(validate_embeds(&mut [e])), "embeds.0.timestamp");
        let e = Embed {
            timestamp: Some("2026-13-40T25:61:00Z".into()),
            ..embed()
        };
        assert_eq!(field_of(validate_embeds(&mut [e])), "embeds.0.timestamp");
        let e = Embed {
            color: 0x0100_0000,
            ..embed()
        };
        assert_eq!(field_of(validate_embeds(&mut [e])), "embeds.0.color");
        let mut e = embed();
        e.timestamp = Some("2026-01-02T03:04:05+02:00".into());
        assert!(validate_embeds(&mut [e]).is_ok());
    }

    #[test]
    fn required_subfields() {
        let e = Embed {
            fields: vec![EmbedField {
                name: " ".into(),
                value: "v".into(),
                inline: false,
            }],
            ..embed()
        };
        assert_eq!(
            field_of(validate_embeds(&mut [e])),
            "embeds.0.fields.0.name"
        );
        let e = Embed {
            footer: Some(EmbedFooter::default()),
            ..embed()
        };
        assert_eq!(field_of(validate_embeds(&mut [e])), "embeds.0.footer.text");
    }
}
