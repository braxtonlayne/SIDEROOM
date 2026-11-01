//! The crash card allow-list (OPS-CRASH-001, wave 45-A, threat model T45).
//!
//! The browser sends a crash report; the server NEVER stores or forwards it. It builds a new card from nothing but the
//! fields below, each checked against a closed set or a strict shape, and drops everything else. A message body, an
//! E2EE key, a presigned URL, a bearer token, an address or an e-mail cannot appear by omission: there is no field for
//! them, no free text is copied, and every string that is kept is limited to a small character set and then tested
//! once more against the secret detector.
//!
//! The allow-list, in full:
//!
//! * `commit`: 7 to 40 lowercase hex characters.
//! * `kind`: one of [`KINDS`].
//! * `route`: the route NAME (a static route pattern such as `lounge.:guildId`), never a URL or query.
//! * `frames`: up to [`MAX_FRAMES`] stack frames, each a normalized file (host, query and fragment removed, id-like
//!   path pieces replaced by `:id`), a line, a column and a function name from a restricted character set.
//! * `caps`: the browser capability flags of [`CAPS`], booleans only, and the coarse `engine` of [`ENGINES`].
//! * `ticks`: up to [`MAX_TICKS`] deterministic ticks, each `{t, src, input}` where `src` is one of [`TICK_SOURCES`]
//!   and `input` one of [`TICK_INPUTS`]; an input outside the set becomes `other`, its text is never copied.
//! * `at`: a timestamp in milliseconds (the server's clock when the client's is implausible).
//!
//! The client has the same list (`apps/web/src/crash/allowList.ts`); the shared hostile fixture
//! `e2e/fixtures/crash-hostile.json` is fed to both so they cannot drift apart unnoticed.

use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The largest request body.
pub const MAX_BODY_BYTES: usize = 32 * 1024;
pub const MAX_FRAMES: usize = 24;
/// The ring buffer holds the last 60 ticks.
pub const MAX_TICKS: usize = 60;
/// How many frames go into the fingerprint.
pub const FINGERPRINT_FRAMES: usize = 5;
/// The card format version.
pub const CARD_VERSION: u32 = 1;

pub const KINDS: [&str; 5] = [
    "error",
    "unhandled_rejection",
    "react_boundary",
    "wasm_panic",
    "webgpu_panic",
];

pub const TICK_SOURCES: [&str; 3] = ["activity", "game", "emulator"];

/// Opaque tick inputs. Bridge commands of an Activity are named by the command only, never by its arguments.
pub const TICK_INPUTS: [&str; 37] = [
    "none",
    "up",
    "down",
    "left",
    "right",
    "a",
    "b",
    "x",
    "y",
    "start",
    "select",
    "primary",
    "secondary",
    "jump",
    "fire",
    "interact",
    "pause",
    "resume",
    "join",
    "leave",
    "state_sync",
    "host_change",
    "bridge_authorize",
    "bridge_authenticate",
    "bridge_get_channel",
    "bridge_get_participants",
    "bridge_open_link",
    "bridge_set_activity",
    "bridge_data_send",
    "bridge_storage_set",
    "bridge_storage_remove",
    "bridge_save_file",
    "bridge_subscribe",
    "bridge_unsubscribe",
    "bridge_other",
    "tick",
    "other",
];

pub const CAPS: [&str; 13] = [
    "webgpu",
    "wasm",
    "wasm_simd",
    "shared_array_buffer",
    "cross_origin_isolated",
    "webrtc",
    "web_codecs",
    "service_worker",
    "offscreen_canvas",
    "web_audio",
    "indexed_db",
    "touch",
    "secure_context",
];

pub const ENGINES: [&str; 4] = ["chromium", "firefox", "webkit", "other"];

/// Why a report was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    NotAnObject,
    BadCommit,
    BadKind,
}

impl Reject {
    pub const fn message(self) -> &'static str {
        match self {
            Self::NotAnObject => "A crash report is a JSON object.",
            Self::BadCommit => "commit must be 7 to 40 lowercase hex characters.",
            Self::BadKind => "kind is not one of the known crash kinds.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Frame {
    pub file: String,
    pub line: u32,
    pub col: u32,
    #[serde(rename = "fn", skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tick {
    pub t: u64,
    pub src: &'static str,
    pub input: &'static str,
}

/// The card exactly as it is stored and shown in `#crash-triage` (without the replay buffer, which is kept beside it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CrashCard {
    pub v: u32,
    pub commit: String,
    pub kind: &'static str,
    pub route: String,
    pub frames: Vec<Frame>,
    pub caps: BTreeMap<&'static str, bool>,
    pub engine: &'static str,
    pub at: i64,
}

/// The result of [`sanitize`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanitized {
    pub card: CrashCard,
    pub ticks: Vec<Tick>,
    /// Hex SHA-256 over the commit and the normalized top frames.
    pub fingerprint: String,
}

impl Sanitized {
    pub fn card_json(&self) -> Value {
        serde_json::to_value(&self.card).unwrap_or(Value::Null)
    }

    pub fn replay_json(&self) -> Value {
        serde_json::to_value(&self.ticks).unwrap_or(Value::Null)
    }
}

/// Build the card from a raw report. Fields off the allow-list are not read at all.
pub fn sanitize(raw: &Value, now_ms: i64) -> Result<Sanitized, Reject> {
    let obj = raw.as_object().ok_or(Reject::NotAnObject)?;
    let commit = obj
        .get("commit")
        .and_then(Value::as_str)
        .filter(|c| is_commit(c))
        .ok_or(Reject::BadCommit)?
        .to_owned();
    let kind = obj
        .get("kind")
        .and_then(Value::as_str)
        .and_then(|k| KINDS.iter().find(|x| **x == k).copied())
        .ok_or(Reject::BadKind)?;
    let route = obj
        .get("route")
        .and_then(Value::as_str)
        .and_then(clean_route)
        .unwrap_or_else(|| "unknown".to_owned());
    let frames: Vec<Frame> = obj
        .get("frames")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(clean_frame).take(MAX_FRAMES).collect())
        .unwrap_or_default();
    let (caps, engine) = clean_caps(obj.get("caps"));
    let ticks = clean_ticks(obj.get("ticks"));
    let at = obj
        .get("at")
        .and_then(Value::as_i64)
        .filter(|t| (t - now_ms).abs() <= 24 * 3600 * 1000)
        .unwrap_or(now_ms);
    let fingerprint = fingerprint(&commit, kind, &route, &frames);
    Ok(Sanitized {
        card: CrashCard {
            v: CARD_VERSION,
            commit,
            kind,
            route,
            frames,
            caps,
            engine,
            at,
        },
        ticks,
        fingerprint,
    })
}

fn is_commit(c: &str) -> bool {
    (7..=40).contains(&c.len()) && c.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Hex SHA-256 over the commit and the normalized top frames (file, line, function). A crash without any frame is told
/// apart by its kind and route instead, so all stackless crashes of a build do not collapse into one.
pub fn fingerprint(commit: &str, kind: &str, route: &str, frames: &[Frame]) -> String {
    let mut h = Sha256::new();
    h.update(b"sr-crash/v1\n");
    h.update(commit.as_bytes());
    h.update(b"\n");
    if frames.is_empty() {
        h.update(kind.as_bytes());
        h.update(b"\n");
        h.update(route.as_bytes());
    }
    for f in frames.iter().take(FINGERPRINT_FRAMES) {
        h.update(f.file.as_bytes());
        h.update(b"|");
        h.update(f.line.to_string().as_bytes());
        h.update(b"|");
        h.update(f.function.as_deref().unwrap_or("").as_bytes());
        h.update(b"\n");
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

// ---- strings ------------------------------------------------------------------------------------------------------

fn clean_route(r: &str) -> Option<String> {
    let ok = !r.is_empty()
        && r.len() <= 64
        && r.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
        && !r.split(['.', '-', '_', ':']).any(id_like_piece)
        && !suspicious(r);
    ok.then(|| r.to_owned())
}

fn clean_function(f: &str) -> Option<String> {
    let ok = !f.is_empty()
        && f.len() <= 80
        && f.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'_' | b'$' | b'.' | b'<' | b'>' | b':' | b'[' | b']' | b'-'
                )
        })
        && !f
            .split(['.', '-', '_', ':', '<', '>', '[', ']', '$'])
            .any(id_like_piece)
        && !suspicious(f);
    ok.then(|| f.to_owned())
}

fn clean_frame(v: &Value) -> Option<Frame> {
    let o = v.as_object()?;
    let file = normalize_file(o.get("file")?.as_str()?)?;
    let num = |k: &str| {
        o.get(k)
            .and_then(Value::as_u64)
            .filter(|n| *n <= 100_000_000)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    Some(Frame {
        file,
        line: num("line"),
        col: num("col"),
        function: o.get("fn").and_then(Value::as_str).and_then(clean_function),
    })
}

/// A frame's file name without anything that identifies a person, a host or a session: no scheme or host, no query or
/// fragment, no drive letter or home folder, at most the last four path pieces, and an id-like piece (a long number, a
/// hex or base64 run, a UUID, a token prefix) is replaced by `:id`. `wasm://wasm/<hash>` becomes `wasm`.
pub fn normalize_file(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 512 {
        return None;
    }
    let cut = raw.find(['?', '#']).map_or(raw, |i| &raw[..i]);
    let mut path = cut.replace('\\', "/");
    if let Some(i) = path.find("://") {
        let scheme = path[..i].to_ascii_lowercase();
        if scheme == "wasm" {
            return Some("wasm".to_owned());
        }
        let rest = &path[i + 3..];
        // The authority (user:pass@host:port) is dropped whole.
        path = match rest.find('/') {
            Some(j) => rest[j..].to_owned(),
            None => return None,
        };
    }
    let segs: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect();
    if segs.is_empty() {
        return None;
    }
    // A home folder's owner is a person: the piece after `Users` or `home` is never kept.
    let segs: Vec<String> = segs
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let home =
                i > 0 && matches!(segs[i - 1].to_ascii_lowercase().as_str(), "users" | "home");
            if home {
                ":id".to_owned()
            } else {
                (*s).to_owned()
            }
        })
        .collect();
    let tail = &segs[segs.len().saturating_sub(4)..];
    let out: Vec<String> = tail.iter().map(|s| clean_segment(s)).collect();
    let joined = out.join("/");
    (!suspicious(&joined)).then_some(joined)
}

fn clean_segment(s: &str) -> String {
    let charset_ok = s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !charset_ok || is_uuid(s) {
        return ":id".to_owned();
    }
    let mut out = String::new();
    let mut piece = String::new();
    let flush = |piece: &mut String, out: &mut String| {
        if !piece.is_empty() {
            if id_like_piece(piece) {
                out.push_str(":id");
            } else {
                out.push_str(piece);
            }
            piece.clear();
        }
    };
    for c in s.chars() {
        if matches!(c, '.' | '_' | '-') {
            flush(&mut piece, &mut out);
            out.push(c);
        } else {
            piece.push(c);
        }
    }
    flush(&mut piece, &mut out);
    out
}

fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// A piece of a name that looks like an identifier or a secret: a number of five digits or more, a hex run of 16 or
/// more, an alphanumeric run of 20 or more that mixes letters and digits, or a known token prefix.
pub fn id_like_piece(p: &str) -> bool {
    if p.is_empty() {
        return false;
    }
    let all_digits = p.bytes().all(|b| b.is_ascii_digit());
    if all_digits && p.len() >= 5 {
        return true;
    }
    if p.len() >= 16 && p.bytes().all(|b| b.is_ascii_hexdigit()) {
        return true;
    }
    let has_digit = p.bytes().any(|b| b.is_ascii_digit());
    let has_alpha = p.bytes().any(|b| b.is_ascii_alphabetic());
    if p.len() >= 20 && has_digit && has_alpha {
        return true;
    }
    const PREFIXES: [&str; 8] = ["eyJ", "AKIA", "ASIA", "ghp", "gho", "srg", "xox", "sk-"];
    p.len() >= 12 && has_digit && PREFIXES.iter().any(|x| p.starts_with(x))
}

/// The detector every kept string goes through once more (defense in depth: the character sets already exclude most of
/// this). It is a deliberate over-approximation: a hit drops the string.
pub fn suspicious(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    if l.contains("x-amz-") || l.contains("signature=") || l.contains("begin private") {
        return true;
    }
    if s.contains('@') || s.contains("://") {
        return true;
    }
    if s.contains("eyJ") && s.len() >= 20 {
        return true;
    }
    has_ipv4(s)
        || s.split(|c: char| !c.is_ascii_alphanumeric())
            .any(id_like_piece)
}

fn has_ipv4(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let mut j = i;
            let mut groups = 0;
            loop {
                let start = j;
                while j < b.len() && b[j].is_ascii_digit() && j - start < 3 {
                    j += 1;
                }
                if j == start {
                    break;
                }
                groups += 1;
                if groups == 4 {
                    return true;
                }
                if j < b.len() && b[j] == b'.' {
                    j += 1;
                } else {
                    break;
                }
            }
        }
        i += 1;
    }
    false
}

// ---- caps and ticks -----------------------------------------------------------------------------------------------

fn clean_caps(v: Option<&Value>) -> (BTreeMap<&'static str, bool>, &'static str) {
    let mut caps = BTreeMap::new();
    let mut engine = "other";
    if let Some(o) = v.and_then(Value::as_object) {
        for name in CAPS {
            if let Some(b) = o.get(name).and_then(Value::as_bool) {
                caps.insert(name, b);
            }
        }
        if let Some(e) = o.get("engine").and_then(Value::as_str) {
            engine = ENGINES
                .iter()
                .find(|x| **x == e)
                .copied()
                .unwrap_or("other");
        }
    }
    (caps, engine)
}

fn clean_ticks(v: Option<&Value>) -> Vec<Tick> {
    let Some(a) = v.and_then(Value::as_array) else {
        return Vec::new();
    };
    // The newest 60: a longer list is cut from the front, never from the end.
    let start = a.len().saturating_sub(MAX_TICKS);
    a[start..]
        .iter()
        .filter_map(|t| {
            let o = t.as_object()?;
            let src = o
                .get("src")
                .and_then(Value::as_str)
                .and_then(|s| TICK_SOURCES.iter().find(|x| **x == s).copied())?;
            let input = o
                .get("input")
                .and_then(Value::as_str)
                .and_then(|s| TICK_INPUTS.iter().find(|x| **x == s).copied())
                .unwrap_or("other");
            let t = o
                .get("t")
                .and_then(Value::as_u64)
                .filter(|n| *n < (1 << 40))
                .unwrap_or(0);
            Some(Tick { t, src, input })
        })
        .collect()
}

/// The card plus its replay buffer as one JSON document, the form an agent reads.
pub fn full_json(card: &Value, replay: &Value) -> Value {
    let mut m = Map::new();
    m.insert("card".into(), card.clone());
    m.insert("replay".into(), replay.clone());
    json!(m)
}

#[cfg(test)]
mod tests;
