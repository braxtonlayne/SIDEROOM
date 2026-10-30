//! UTL-COL-001: tool rooms, the shared documents behind the collaborative tier list and poll builder.
//!
//! A room is bound to one channel or DM and lives 7 days from its creation. Its `state` is a flat JSON
//! object: an edit is a [`Patch`] that sets or removes whole keys against the room's `version`. A patch on
//! a stale version is refused with the current room back ([`RoomError::Stale`], HTTP 409), so the client
//! re-applies its own keys on top of the fresh state and retries: last writer wins per key, with no CRDT.
//!
//! The rules here are pure; storage is the [`ToolRoomStore`] trait (Postgres: `sr_db::tool_rooms_repo`).
//! Images in a room are *references* (a mount and a key, or a same-origin path), never URLs: each viewer
//! asks the mount presign endpoint for their own link, so the storage scopes apply per viewer.

use crate::repo::StoreResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The kinds of room the first release knows.
pub const KINDS: [&str; 2] = ["tierlist", "poll"];
/// A room lives 7 days from its creation.
pub const LIFE_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Serialised size of a room's state.
pub const MAX_STATE_BYTES: usize = 64 * 1024;
/// Keys in a room's state.
pub const MAX_KEYS: usize = 600;
/// Keys one patch may touch.
pub const MAX_PATCH_KEYS: usize = 100;
/// Characters in a key.
pub const MAX_KEY_LEN: usize = 64;
/// Serialised size of one value.
pub const MAX_VALUE_BYTES: usize = 4 * 1024;
/// Characters in a title.
pub const MAX_TITLE_CHARS: usize = 100;
/// Live rooms per channel.
pub const MAX_ROOMS_PER_CHANNEL: usize = 20;
/// Characters in an image key or path.
pub const MAX_IMAGE_REF: usize = 1024;

/// A tool room as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolRoom {
    pub id: u64,
    pub channel_id: u64,
    pub guild_id: Option<u64>,
    pub kind: String,
    pub title: String,
    pub state: Map<String, Value>,
    pub version: i64,
    pub created_by: Option<u64>,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub expires_ms: i64,
}

/// An edit: whole keys set (or removed with `null`) against a version.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Patch {
    /// The version the client last saw.
    pub base_version: i64,
    /// Keys to set; a `null` value removes the key.
    #[serde(default)]
    pub set: Map<String, Value>,
    /// A new title.
    #[serde(default)]
    pub title: Option<String>,
}

/// Why a room operation failed.
#[derive(Debug, Clone, PartialEq)]
pub enum RoomError {
    /// The request is malformed (message names the field).
    Invalid(String),
    /// The patch was made on an older version; carries the current room.
    Stale(Box<ToolRoom>),
    NotFound,
    /// The channel already has [`MAX_ROOMS_PER_CHANNEL`] live rooms.
    TooMany,
    Store(String),
}

impl From<crate::repo::StoreError> for RoomError {
    fn from(e: crate::repo::StoreError) -> Self {
        Self::Store(e.0)
    }
}

pub fn validate_kind(kind: &str) -> Result<(), RoomError> {
    if KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(RoomError::Invalid("kind: unknown tool room kind".into()))
    }
}

pub fn validate_title(title: &str) -> Result<String, RoomError> {
    let t = title.trim();
    if t.chars().count() > MAX_TITLE_CHARS {
        return Err(RoomError::Invalid(format!(
            "title: at most {MAX_TITLE_CHARS} characters"
        )));
    }
    Ok(t.to_owned())
}

pub fn validate_key(key: &str) -> Result<(), RoomError> {
    let ok = !key.is_empty()
        && key.len() <= MAX_KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b':' | b'_' | b'-'));
    if ok {
        Ok(())
    } else {
        Err(RoomError::Invalid(format!(
            "set: key `{key}` must be 1 to {MAX_KEY_LEN} of letters, digits . : _ -"
        )))
    }
}

/// An image reference: `{"k":"mount","mount":"<snowflake>","key":"<object key>"}` (presigned per viewer) or
/// `{"k":"path","path":"/..."}` (a same-origin path such as an attachment of the app's own CDN route).
fn validate_image(img: &Value) -> Result<(), RoomError> {
    let bad = |m: &str| Err(RoomError::Invalid(format!("img: {m}")));
    let Some(o) = img.as_object() else {
        return bad("must be an object");
    };
    let str_of = |k: &str| o.get(k).and_then(Value::as_str);
    match str_of("k") {
        Some("mount") => {
            let id_ok = str_of("mount").is_some_and(|m| {
                !m.is_empty() && m.len() <= 20 && m.bytes().all(|b| b.is_ascii_digit())
            });
            let key_ok = str_of("key").is_some_and(|k| !k.is_empty() && k.len() <= MAX_IMAGE_REF);
            if id_ok && key_ok {
                Ok(())
            } else {
                bad("a mount image needs a numeric mount and a key")
            }
        }
        Some("path") => {
            let ok = str_of("path").is_some_and(|p| {
                p.starts_with('/')
                    && !p.starts_with("//")
                    && !p.contains('\\')
                    && p.len() <= MAX_IMAGE_REF
            });
            if ok {
                Ok(())
            } else {
                bad("a path image needs a same-origin path")
            }
        }
        _ => bad("k must be `mount` or `path`; URLs are not stored"),
    }
}

/// A value: any JSON up to [`MAX_VALUE_BYTES`]; an `img` member must be a valid image reference.
pub fn validate_value(key: &str, v: &Value) -> Result<(), RoomError> {
    if v.is_null() {
        return Ok(());
    }
    if serde_json::to_vec(v).map_or(usize::MAX, |b| b.len()) > MAX_VALUE_BYTES {
        return Err(RoomError::Invalid(format!(
            "set: value of `{key}` is over {MAX_VALUE_BYTES} bytes"
        )));
    }
    if let Some(img) = v.as_object().and_then(|o| o.get("img")) {
        if !img.is_null() {
            validate_image(img)?;
        }
    }
    Ok(())
}

/// Check a whole state (creation).
pub fn validate_state(state: &Map<String, Value>) -> Result<(), RoomError> {
    if state.len() > MAX_KEYS {
        return Err(RoomError::Invalid(format!(
            "state: at most {MAX_KEYS} keys"
        )));
    }
    for (k, v) in state {
        validate_key(k)?;
        validate_value(k, v)?;
    }
    if serde_json::to_vec(state).map_or(usize::MAX, |b| b.len()) > MAX_STATE_BYTES {
        return Err(RoomError::Invalid(format!(
            "state: over {MAX_STATE_BYTES} bytes"
        )));
    }
    Ok(())
}

/// The state and title after `patch`, or why it does not apply. A stale `base_version` is
/// [`RoomError::Stale`] with `room` itself, so the caller can answer 409 with the current state.
pub fn apply_patch(
    room: &ToolRoom,
    patch: &Patch,
) -> Result<(Map<String, Value>, String), RoomError> {
    if patch.base_version != room.version {
        return Err(RoomError::Stale(Box::new(room.clone())));
    }
    if patch.set.is_empty() && patch.title.is_none() {
        return Err(RoomError::Invalid("set: nothing to change".into()));
    }
    if patch.set.len() > MAX_PATCH_KEYS {
        return Err(RoomError::Invalid(format!(
            "set: at most {MAX_PATCH_KEYS} keys per edit"
        )));
    }
    let title = match &patch.title {
        Some(t) => validate_title(t)?,
        None => room.title.clone(),
    };
    let mut state = room.state.clone();
    for (k, v) in &patch.set {
        validate_key(k)?;
        validate_value(k, v)?;
        if v.is_null() {
            state.remove(k);
        } else {
            state.insert(k.clone(), v.clone());
        }
    }
    validate_state(&state)?;
    Ok((state, title))
}

/// Whether the room is past its life at `now_ms`.
pub fn expired(room: &ToolRoom, now_ms: i64) -> bool {
    room.expires_ms <= now_ms
}

/// A new room's expiry.
pub fn expires_at(created_ms: i64) -> i64 {
    created_ms.saturating_add(LIFE_MS)
}

/// Storage of tool rooms. Reads never return an expired room.
#[async_trait]
pub trait ToolRoomStore: Send + Sync {
    /// Create a room; [`RoomError::TooMany`] when the channel is full.
    async fn create(&self, room: ToolRoom, now_ms: i64) -> Result<ToolRoom, RoomError>;
    async fn get(&self, channel: u64, id: u64, now_ms: i64) -> StoreResult<Option<ToolRoom>>;
    /// Live rooms of a channel, newest first.
    async fn list(&self, channel: u64, now_ms: i64) -> StoreResult<Vec<ToolRoom>>;
    /// Apply `patch` atomically under the version rule. `Stale` carries the current room.
    async fn patch(
        &self,
        channel: u64,
        id: u64,
        patch: &Patch,
        now_ms: i64,
    ) -> Result<ToolRoom, RoomError>;
    async fn delete(&self, channel: u64, id: u64) -> StoreResult<bool>;
    /// Remove expired rooms; returns how many.
    async fn sweep(&self, now_ms: i64) -> StoreResult<u64>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn room() -> ToolRoom {
        ToolRoom {
            id: 1,
            channel_id: 2,
            guild_id: None,
            kind: "tierlist".into(),
            title: "Best pizza".into(),
            state: json!({"t.a": {"name": "S"}}).as_object().unwrap().clone(),
            version: 3,
            created_by: Some(9),
            created_ms: 0,
            updated_ms: 0,
            expires_ms: LIFE_MS,
        }
    }

    fn patch(v: i64, set: Value) -> Patch {
        Patch {
            base_version: v,
            set: set.as_object().unwrap().clone(),
            title: None,
        }
    }

    #[test]
    fn sets_and_removes_keys() {
        let p = patch(3, json!({"t.b": {"name": "A"}, "t.a": null}));
        let (s, _) = apply_patch(&room(), &p).unwrap();
        assert!(s.contains_key("t.b") && !s.contains_key("t.a"));
    }

    #[test]
    fn stale_version_returns_the_current_room() {
        match apply_patch(&room(), &patch(2, json!({"x": 1}))) {
            Err(RoomError::Stale(r)) => assert_eq!(r.version, 3),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bad_keys_and_values_are_refused() {
        for k in ["", "a b", "a/b", &"k".repeat(65)] {
            assert!(validate_key(k).is_err(), "{k}");
        }
        assert!(validate_key("item.12:x_y-z").is_ok());
        let big = json!({"x": "a".repeat(MAX_VALUE_BYTES)});
        assert!(apply_patch(&room(), &patch(3, big)).is_err());
        assert!(apply_patch(&room(), &patch(3, json!({}))).is_err());
    }

    #[test]
    fn images_are_references_never_urls() {
        let ok = json!({"i.1": {"img": {"k": "mount", "mount": "55", "key": "pics/a.png"}}});
        assert!(apply_patch(&room(), &patch(3, ok)).is_ok());
        let path = json!({"i.1": {"img": {"k": "path", "path": "/api/v1/attachments/1/2/a.png"}}});
        assert!(apply_patch(&room(), &patch(3, path)).is_ok());
        for bad in [
            json!({"k": "url", "url": "https://evil.example/a.png"}),
            json!({"k": "path", "path": "//evil.example/a.png"}),
            json!({"k": "path", "path": "https://evil.example/a.png"}),
            json!({"k": "mount", "mount": "abc", "key": "a"}),
            json!({"k": "mount", "mount": "5"}),
            json!("https://x"),
        ] {
            let p = patch(3, json!({"i.1": {"img": bad}}));
            assert!(apply_patch(&room(), &p).is_err());
        }
    }

    #[test]
    fn key_count_and_patch_size_are_capped() {
        let many: Map<String, Value> = (0..=MAX_PATCH_KEYS)
            .map(|i| (format!("k{i}"), json!(1)))
            .collect();
        let p = Patch {
            base_version: 3,
            set: many,
            title: None,
        };
        assert!(apply_patch(&room(), &p).is_err());
        let mut r = room();
        r.state = (0..MAX_KEYS).map(|i| (format!("k{i}"), json!(1))).collect();
        assert!(apply_patch(&r, &patch(3, json!({"one-more": 1}))).is_err());
    }

    #[test]
    fn titles_and_life() {
        assert!(validate_title(&"x".repeat(101)).is_err());
        assert_eq!(validate_title("  Tier  ").unwrap(), "Tier");
        assert!(validate_kind("poll").is_ok() && validate_kind("wheel").is_err());
        assert_eq!(expires_at(10), 10 + LIFE_MS);
        assert!(expired(&room(), LIFE_MS) && !expired(&room(), LIFE_MS - 1));
    }
}
