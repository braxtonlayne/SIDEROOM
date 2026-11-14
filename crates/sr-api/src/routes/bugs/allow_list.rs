//! The bug report allow-list (SEC-AUD-166, wave 64, D-649, threat model T45 extended).
//!
//! The browser sends a description, a debug bundle and optionally a picture. The server NEVER stores or forwards that
//! body: [`sanitize`] builds a new description, bundle and picture from the named fields below and drops everything
//! else, so a token, a message, a URL, an address or an id of another person cannot appear by omission. There is no
//! field for them. The shapes follow the wire contract of the wave 64 plan:
//!
//! * `description`: the member's own words, control characters other than newline and tab removed, trimmed, 1 to
//!   [`MAX_DESCRIPTION`] characters. It is stored as written otherwise (it is text a person chose to send to staff).
//! * `bundle.commit`, `bundle.route`, `bundle.caps`: the crash allow-list's checks (`crash::allow_list`), reused, not
//!   copied. A commit or route that fails becomes `0000000` or `unknown` (the report is still worth having).
//! * `bundle.at`, `viewport`, `locale`, `theme`, `online`, `gateway`, `sw`, `voice`: closed sets and clamped numbers.
//! * `bundle.failed_requests`: at most [`MAX_FAILED`] of `{method, path, status, ms, ago_s}`; a `path` is a path
//!   template (`^/[a-z0-9_@:./-]{1,120}$`, no run of 6 digits, no `?`, nothing id-like); an entry that fails is dropped,
//!   and a segment that is not a fixed word of a known API path (`path_words`, from the route matrix) is stored as `{}`
//!   (SEC-AUD-268: a name or a code is never kept).
//! * Every key of the stored bundle is always present (SEC-AUD-266): a part the body lacked, or sent invalid, is stored
//!   with a neutral value (`viewport` 0 by 0, `gateway` `unknown`, `voice` not connected, `online` null, `sw` `none`).
//!   `online` is `true`, `false` or `null`: a browser that said nothing (or not a boolean) is stored as `null`, which
//!   the staff page reads as "Unknown" (SEC-AUD-277, D-758).
//! * `bundle.crashes`: at most [`MAX_CRASHES`] fingerprints of 64 lowercase hex characters.
//! * `screenshot`: `image/jpeg` only, base64, at most [`MAX_PICTURE_BYTES`] decoded, with the JPEG start and end
//!   markers. A picture that does not pass is REFUSED (the member must know it did not go), never silently dropped.

use crate::routes::crash::allow_list::{clean_caps, clean_route, id_like_piece, is_commit};
use data_encoding::BASE64;
use serde_json::{Map, Value, json};

/// The largest request body (the route's `DefaultBodyLimit`).
pub const MAX_BODY_BYTES: usize = 3 * 1024 * 1024;
pub const MAX_DESCRIPTION: usize = 2000;
/// The largest picture, decoded.
pub const MAX_PICTURE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_FAILED: usize = 20;
pub const MAX_CRASHES: usize = 5;
/// `masked` (regions blacked out in the picture) is a count in this range.
pub const MAX_MASKED: u64 = 64;

const THEMES: [&str; 3] = ["editorial", "lounge-light", "lounge-dark"];
const GATEWAY_STATUSES: [&str; 4] = ["connected", "connecting", "reconnecting", "closed"];
const SW_STATES: [&str; 4] = ["none", "installing", "waiting", "active"];
const METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "?"];
/// The commit and route a report carries when the client's did not pass the crash allow-list's checks.
const NO_COMMIT: &str = "0000000";
const NO_ROUTE: &str = "unknown";

/// Why a report was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    NotAnObject,
    NoDescription,
    DescriptionTooLong,
    PictureType,
    PictureEncoding,
    PictureNotJpeg,
    PictureTooLarge,
}

impl Reason {
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotAnObject => "A bug report is a JSON object.",
            Self::NoDescription => "Describe the problem in a few words.",
            Self::DescriptionTooLong => "The description is at most 2000 characters.",
            Self::PictureType => "The picture must be a JPEG (image/jpeg).",
            Self::PictureEncoding => "The picture is not valid base64.",
            Self::PictureNotJpeg => "The picture is not a JPEG file.",
            Self::PictureTooLarge => "The picture is over 2 MiB.",
        }
    }

    /// Which field of the body the message is about.
    pub const fn field(self) -> &'static str {
        match self {
            Self::NotAnObject => "report",
            Self::NoDescription | Self::DescriptionTooLong => "description",
            _ => "screenshot",
        }
    }
}

/// What the server keeps of a report.
#[derive(Debug, Clone, PartialEq)]
pub struct Clean {
    pub description: String,
    pub commit: String,
    pub route: String,
    /// The rebuilt bundle (its own `commit` and `route` repeat the columns, so the detail read is one object).
    pub bundle: Value,
    /// The decoded JPEG, checked.
    pub screenshot: Option<Vec<u8>>,
}

/// Build what is stored from a raw report. Fields off the allow-list are not read at all.
pub fn sanitize(raw: &Value, now_ms: i64) -> Result<Clean, Reason> {
    let obj = raw.as_object().ok_or(Reason::NotAnObject)?;
    let description = clean_description(obj.get("description"))?;
    let screenshot = obj.get("screenshot").filter(|v| !v.is_null());
    let (picture, masked) = match screenshot {
        Some(s) => {
            let (bytes, masked) = clean_picture(s)?;
            (Some(bytes), Some(masked))
        }
        None => (None, None),
    };
    let empty = Map::new();
    let b = obj
        .get("bundle")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    let commit = b
        .get("commit")
        .and_then(Value::as_str)
        .filter(|c| is_commit(c))
        .unwrap_or(NO_COMMIT)
        .to_owned();
    let route = b
        .get("route")
        .and_then(Value::as_str)
        .and_then(clean_route)
        .unwrap_or_else(|| NO_ROUTE.to_owned());
    let at = b
        .get("at")
        .and_then(Value::as_i64)
        .filter(|t| (t - now_ms).abs() <= 24 * 3600 * 1000)
        .unwrap_or(now_ms);

    let mut out = Map::new();
    out.insert("commit".into(), json!(commit));
    out.insert("route".into(), json!(route));
    out.insert("at".into(), json!(at));
    // SEC-AUD-266: every key of the contract type is always written, with a neutral value when the body had none (or an
    // invalid one), so the staff page never meets a report with a part missing.
    out.insert(
        "viewport".into(),
        clean_viewport(b.get("viewport")).unwrap_or_else(|| json!({ "w": 0, "h": 0, "dpr": 0 })),
    );
    let (caps, engine) = clean_caps(b.get("caps"));
    let mut caps_out = Map::new();
    caps_out.insert("engine".into(), json!(engine));
    for (k, v) in caps {
        caps_out.insert(k.to_owned(), json!(v));
    }
    out.insert("caps".into(), Value::Object(caps_out));
    out.insert("locale".into(), json!(clean_locale(b.get("locale"))));
    out.insert("theme".into(), json!(clean_theme(b.get("theme"))));
    out.insert(
        "online".into(),
        // SEC-AUD-277 (D-758): nothing said, or not a boolean, is `null` (unknown), never a made-up `true`.
        b.get("online").and_then(Value::as_bool).map_or(Value::Null, Value::Bool),
    );
    out.insert(
        "gateway".into(),
        clean_gateway(b.get("gateway")).unwrap_or_else(|| json!({ "status": "unknown" })),
    );
    out.insert(
        "sw".into(),
        json!(pick(b.get("sw"), &SW_STATES).unwrap_or("none")),
    );
    let voice = b
        .get("voice")
        .and_then(Value::as_object)
        .and_then(|v| v.get("connected"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    out.insert("voice".into(), json!({ "connected": voice }));
    out.insert(
        "failed_requests".into(),
        Value::Array(clean_failed(b.get("failed_requests"))),
    );
    out.insert("crashes".into(), json!(clean_crashes(b.get("crashes"))));
    if let Some(m) = masked {
        out.insert("masked".into(), json!(m));
    }
    Ok(Clean {
        description,
        commit,
        route,
        bundle: Value::Object(out),
        screenshot: picture,
    })
}

// ---- the description ----------------------------------------------------------------------------------------------

fn clean_description(v: Option<&Value>) -> Result<String, Reason> {
    let raw = v.and_then(Value::as_str).ok_or(Reason::NoDescription)?;
    let kept: String = raw
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    let text = kept.trim();
    if text.is_empty() {
        return Err(Reason::NoDescription);
    }
    if text.chars().count() > MAX_DESCRIPTION {
        return Err(Reason::DescriptionTooLong);
    }
    Ok(text.to_owned())
}

// ---- the picture --------------------------------------------------------------------------------------------------

/// Check the picture and decode it. Returns the bytes and the `masked` count.
fn clean_picture(v: &Value) -> Result<(Vec<u8>, u64), Reason> {
    let o = v.as_object().ok_or(Reason::PictureType)?;
    if o.get("type").and_then(Value::as_str) != Some("image/jpeg") {
        return Err(Reason::PictureType);
    }
    let data = o
        .get("data")
        .and_then(Value::as_str)
        .ok_or(Reason::PictureEncoding)?;
    // Base64 is 4 characters for 3 bytes: refuse before decoding what could not fit.
    if data.len() > MAX_PICTURE_BYTES / 3 * 4 + 8 {
        return Err(Reason::PictureTooLarge);
    }
    let bytes = BASE64
        .decode(data.as_bytes())
        .map_err(|_| Reason::PictureEncoding)?;
    if bytes.len() > MAX_PICTURE_BYTES {
        return Err(Reason::PictureTooLarge);
    }
    // The JPEG start-of-image marker (followed by the next marker's 0xFF) and the end-of-image marker.
    if bytes.len() < 5
        || bytes[..3] != [0xFF, 0xD8, 0xFF]
        || bytes[bytes.len() - 2..] != [0xFF, 0xD9]
    {
        return Err(Reason::PictureNotJpeg);
    }
    let masked = o
        .get("masked")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .map_or(0, |n| (n.round().max(0.0) as u64).min(MAX_MASKED));
    Ok((bytes, masked))
}

// ---- numbers and enums --------------------------------------------------------------------------------------------

/// A finite number clamped to `lo..=hi`, rounded to an integer; `None` when it is not a number.
fn int(v: Option<&Value>, lo: i64, hi: i64) -> Option<i64> {
    let n = v.and_then(Value::as_f64).filter(|n| n.is_finite())?;
    Some((n.round() as i64).clamp(lo, hi))
}

/// `v` when it is exactly one of `set`.
fn pick(v: Option<&Value>, set: &[&'static str]) -> Option<&'static str> {
    let s = v.and_then(Value::as_str)?;
    set.iter().find(|x| **x == s).copied()
}

fn clean_viewport(v: Option<&Value>) -> Option<Value> {
    let o = v?.as_object()?;
    let w = int(o.get("w"), 1, 16384)?;
    let h = int(o.get("h"), 1, 16384)?;
    // The pixel ratio keeps two decimals.
    let dpr = o
        .get("dpr")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .map_or(1.0, |n| (n.clamp(0.5, 8.0) * 100.0).round() / 100.0);
    Some(json!({ "w": w, "h": h, "dpr": dpr }))
}

/// A BCP 47 style tag of at most 35 characters; anything else is `und`.
fn clean_locale(v: Option<&Value>) -> String {
    let ok = |s: &str| {
        let mut parts = s.split('-');
        let first = parts.next().unwrap_or("");
        s.len() <= 35
            && (2..=3).contains(&first.len())
            && first.bytes().all(|b| b.is_ascii_alphabetic())
            && parts.clone().count() <= 3
            && parts
                .all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
    };
    match v.and_then(Value::as_str) {
        Some(s) if ok(s) => s.to_owned(),
        _ => "und".to_owned(),
    }
}

fn clean_theme(v: Option<&Value>) -> &'static str {
    pick(v, &THEMES).unwrap_or("other")
}

fn clean_gateway(v: Option<&Value>) -> Option<Value> {
    let o = v?.as_object()?;
    let status = pick(o.get("status"), &GATEWAY_STATUSES)?;
    let mut g = Map::new();
    g.insert("status".into(), json!(status));
    if let Some(n) = int(o.get("reconnects"), 0, 10_000) {
        g.insert("reconnects".into(), json!(n));
    }
    if let Some(n) = int(o.get("heartbeat_ms"), 0, 600_000) {
        g.insert("heartbeat_ms".into(), json!(n));
    }
    Some(Value::Object(g))
}

// ---- failed requests and crashes ----------------------------------------------------------------------------------

/// A path template: `^/[a-z0-9_@:./-]{1,120}$`, no run of 6 digits, nothing id-like, then every segment that is not a
/// fixed word of an API path becomes `{}` (`template_segments`). A query string, a host, an upper case letter or a
/// percent escape cannot pass the character set.
fn clean_path(p: &str) -> Option<String> {
    let rest = p.strip_prefix('/')?;
    let ok = (1..=120).contains(&rest.len())
        && rest.bytes().all(
            |b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'@' | b':' | b'.' | b'/' | b'-'),
        )
        && !has_digit_run(p, 6)
        // `@` is for `/@me` alone: anything else with one in it looks like an address.
        && !p.split('/').any(|s| s.contains('@') && s != "@me")
        && !p
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(id_like_piece);
    if !ok {
        return None;
    }
    template_segments(p)
}

/// SEC-AUD-268: keep a segment only when it is a fixed word of a known API path ([`path_words::PATH_WORDS`], from the
/// route matrix), the `/api/v1` prefix, the client's own `:id` placeholder or `@me`; any other segment (a handle, an
/// invite code, a slug: a lower case word passes the character set) becomes `{}`. A path that does not start with
/// `/api` is dropped. Rows stored before this rule may hold such words and are not rewritten.
fn template_segments(p: &str) -> Option<String> {
    let mut parts = p.split('/');
    parts.next(); // the empty piece before the leading slash
    let mut out = String::with_capacity(p.len());
    for (i, seg) in parts.enumerate() {
        let keep = match i {
            0 => {
                if seg != "api" {
                    return None;
                }
                true
            }
            1 if seg == "v1" => true,
            _ => seg == ":id" || path_words::PATH_WORDS.binary_search(&seg).is_ok(),
        };
        out.push('/');
        out.push_str(if keep { seg } else { "{}" });
    }
    Some(out)
}

fn has_digit_run(s: &str, n: usize) -> bool {
    let mut run = 0;
    for b in s.bytes() {
        if b.is_ascii_digit() {
            run += 1;
            if run >= n {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn clean_failed(v: Option<&Value>) -> Vec<Value> {
    let Some(a) = v.and_then(Value::as_array) else {
        return Vec::new();
    };
    a.iter()
        .filter_map(|e| {
            let o = e.as_object()?;
            let method = pick(o.get("method"), &METHODS)?;
            let path = clean_path(o.get("path")?.as_str()?)?;
            // A status the browser cannot have seen is dropped, not clamped to a lie.
            let raw = o.get("status").and_then(Value::as_f64)?;
            if !(100.0..=599.0).contains(&raw) {
                return None;
            }
            let status = raw.round() as i64;
            Some(json!({
                "method": method,
                "path": path,
                "status": status,
                "ms": int(o.get("ms"), 0, 600_000).unwrap_or(0),
                "ago_s": int(o.get("ago_s"), 0, 86_400).unwrap_or(0),
            }))
        })
        .take(MAX_FAILED)
        .collect()
}

fn clean_crashes(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|s| {
                    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
                })
                .map(str::to_owned)
                .take(MAX_CRASHES)
                .collect()
        })
        .unwrap_or_default()
}

mod path_words;

#[cfg(test)]
mod tests;
