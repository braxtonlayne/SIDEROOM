//! Shared handler plumbing for the social routes.

use super::error::{MsgResult, invalid, parse_id};
use super::model::CustomStatusBody;
use crate::{AuthKind, Principal};
use sr_chat::social::presence::CustomStatus;
use sr_core::{ChannelId, ErrorCode, MessageId, UserId};

/// The acting user. OAuth bearer tokens never reach relationship, DM or settings
/// data (no scope covers it yet); sessions and bots do.
pub fn actor(p: &Principal) -> MsgResult<UserId> {
    match p.kind {
        AuthKind::Bearer { .. } => Err(ErrorCode::MissingAccess.into()),
        _ => Ok(UserId::new(u64::try_from(p.user_id).unwrap_or(0))),
    }
}

/// Parse a JSON body that may be empty (all fields then take their defaults).
pub fn optional_json<T: serde::de::DeserializeOwned + Default>(
    bytes: &axum::body::Bytes,
) -> MsgResult<T> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    serde_json::from_slice(bytes).map_err(|e| invalid("body", "INVALID_JSON", &e.to_string()))
}

/// A `{user_id}` path segment or body id; malformed means no such user.
pub fn user_id(raw: &str) -> MsgResult<UserId> {
    parse_id(raw, ErrorCode::UnknownUser)
}

pub fn channel_id(raw: &str) -> MsgResult<ChannelId> {
    parse_id(raw, ErrorCode::UnknownChannel)
}

pub fn message_id(raw: &str) -> MsgResult<MessageId> {
    parse_id(raw, ErrorCode::UnknownMessage)
}

/// Parse `YYYY-MM-DDTHH:MM:SS[.fraction](Z|+HH:MM|-HH:MM)` to unix milliseconds.
pub fn parse_iso(raw: &str) -> Option<i64> {
    let b = raw.as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let s = raw.get(from..from + len)?;
        s.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| s.parse().ok())?
    };
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    let (y, mo, d) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
    if b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (h, mi, s) = (num(11, 2)?, num(14, 2)?, num(17, 2)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    let mut i = 19;
    let mut millis = 0;
    if b[i] == b'.' {
        let start = i + 1;
        i = start;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let digits = &raw[start..i];
        if digits.is_empty() {
            return None;
        }
        let first3: String = digits.chars().chain("00".chars()).take(3).collect();
        millis = first3.parse::<i64>().ok()?;
    }
    let offset_s = match raw.get(i..)? {
        "Z" | "z" => 0,
        tz if tz.len() == 6
            && matches!(tz.as_bytes()[0], b'+' | b'-')
            && tz.as_bytes()[3] == b':' =>
        {
            let hh: i64 = tz.get(1..3)?.parse().ok()?;
            let mm: i64 = tz.get(4..6)?.parse().ok()?;
            let v = hh * 3600 + mm * 60;
            if tz.starts_with('-') { -v } else { v }
        }
        _ => return None,
    };
    // Days from civil (Howard Hinnant).
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2.rem_euclid(400);
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + h * 3600 + mi * 60 + s - offset_s) * 1000) + millis)
}

/// Convert a request custom status to the domain value.
pub fn custom_status(body: &CustomStatusBody) -> MsgResult<CustomStatus> {
    let emoji_id = match body.emoji_id.as_deref() {
        None => None,
        Some(raw) => Some(raw.parse::<u64>().map_err(|_| {
            invalid(
                "custom_status.emoji_id",
                "SNOWFLAKE_TYPE_INVALID",
                "Value is not snowflake.",
            )
        })?),
    };
    let expires_ms = match body.expires_at.as_deref() {
        None => None,
        Some(raw) => Some(parse_iso(raw).ok_or_else(|| {
            invalid(
                "custom_status.expires_at",
                "DATE_TYPE_PARSE",
                "Not a valid ISO 8601 timestamp.",
            )
        })?),
    };
    Ok(CustomStatus {
        text: body.text.clone(),
        emoji_name: body.emoji_name.clone(),
        emoji_id,
        expires_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::messages::model::iso;

    #[test]
    fn iso_round_trips() {
        for ms in [0, 1_800_000_000_123, 951_782_400_000, 1_709_251_199_999] {
            assert_eq!(parse_iso(&iso(ms)), Some(ms));
        }
        assert_eq!(parse_iso("2026-01-02T03:04:05Z"), Some(1_767_323_045_000));
        assert_eq!(
            parse_iso("2026-01-02T05:04:05+02:00"),
            parse_iso("2026-01-02T03:04:05Z")
        );
        assert_eq!(parse_iso("nope"), None);
        assert_eq!(parse_iso("2026-13-02T03:04:05Z"), None);
    }
}

/// Normalise a BCP 47 language tag (`pt_br` becomes `pt-BR`, `zh-hant-tw` becomes `zh-Hant-TW`).
/// `None` for malformed tags and for the layout-testing pseudo-locales `en-XA` and `ar-XB`.
pub fn normalize_locale(raw: &str) -> Option<String> {
    let tag = raw.trim().replace('_', "-");
    if tag.is_empty() || tag.len() > 35 {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for (i, part) in tag.split('-').enumerate() {
        let alpha = part.bytes().all(|b| b.is_ascii_alphabetic());
        let alnum = part.bytes().all(|b| b.is_ascii_alphanumeric());
        let cased = match (i, part.len()) {
            (0, 2 | 3) if alpha => part.to_ascii_lowercase(),
            (0, _) => return None,
            (_, 4) if alpha => {
                let l = part.to_ascii_lowercase();
                format!("{}{}", l[..1].to_ascii_uppercase(), &l[1..])
            }
            (_, 2) if alpha => part.to_ascii_uppercase(),
            (_, 3) if part.bytes().all(|b| b.is_ascii_digit()) => part.to_owned(),
            (_, 5..=8) if alnum => part.to_ascii_lowercase(),
            _ => return None,
        };
        out.push(cased);
    }
    let joined = out.join("-");
    (!matches!(joined.as_str(), "en-XA" | "ar-XB")).then_some(joined)
}

#[cfg(test)]
mod locale_tests {
    use super::normalize_locale;

    #[test]
    fn tags_are_normalised_and_junk_is_refused() {
        for (raw, want) in [
            ("en", "en"),
            ("de-de", "de-DE"),
            ("pt_br", "pt-BR"),
            ("zh-hant-tw", "zh-Hant-TW"),
            (" fr ", "fr"),
            ("es-419", "es-419"),
        ] {
            assert_eq!(normalize_locale(raw).as_deref(), Some(want), "{raw}");
        }
        for bad in [
            "",
            "e",
            "english",
            "en-",
            "en--US",
            "de-DE-@",
            "en-XA",
            "ar-xb",
            "x".repeat(40).as_str(),
        ] {
            assert_eq!(normalize_locale(bad), None, "{bad}");
        }
    }
}
