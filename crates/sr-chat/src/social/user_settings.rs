//! Per-user settings (LNG-USET-002, LNG-USET-012..014): the synced settings blob,
//! the global default notification level and DM / friend-request privacy.
//!
//! The blob is opaque JSON owned by the clients (appearance, notification toggles,
//! keybinds, ...). The server only enforces its shape and size: a JSON object of at
//! most [`MAX_SETTINGS_BYTES`] bytes serialised, nested at most [`MAX_DEPTH`] levels.
//! Updates are JSON Merge Patches (RFC 7386): objects merge key by key, `null`
//! deletes a key, anything else replaces.

use super::notifications::NotificationLevel;
use super::relationships::FriendRequestSources;
use super::repo::SocialRepository;
use super::safety::{MAX_SERVER_DM_OVERRIDES, MediaFilter, SpamFilter};
use super::service::SocialService;
use super::{SocialError, SocialResult};
use serde_json::{Map, Value};
use sr_core::UserId;
use std::collections::BTreeMap;

/// Largest synced blob, serialised.
pub const MAX_SETTINGS_BYTES: usize = 64 * 1024;
/// Deepest object/array nesting accepted in the blob.
pub const MAX_DEPTH: usize = 8;
/// Most top-level keys in the blob.
pub const MAX_TOP_LEVEL_KEYS: usize = 256;
/// Optimistic-concurrency retries before an update gives up.
const WRITE_ATTEMPTS: usize = 4;

/// The stored blob and its version (0 means never saved).
#[derive(Debug, Clone, PartialEq)]
pub struct UserSettings {
    pub data: Value,
    pub version: i64,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            data: Value::Object(Map::new()),
            version: 0,
        }
    }
}

/// Apply an RFC 7386 merge patch to `target`.
pub fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(p) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    if let Value::Object(t) = target {
        for (k, v) in p {
            if v.is_null() {
                t.remove(k);
            } else {
                merge_patch(t.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
    }
}

fn depth(v: &Value) -> usize {
    match v {
        Value::Object(m) => 1 + m.values().map(depth).max().unwrap_or(0),
        Value::Array(a) => 1 + a.iter().map(depth).max().unwrap_or(0),
        _ => 0,
    }
}

/// Merge `patch` into `current` and check the result against the caps.
pub fn apply_patch(current: &Value, patch: &Value) -> SocialResult<Value> {
    if !patch.is_object() {
        return Err(SocialError::invalid(
            "settings",
            "BASE_TYPE_BAD_FORMAT",
            "must be a JSON object",
        ));
    }
    let mut next = current.clone();
    merge_patch(&mut next, patch);
    check_blob(&next)?;
    Ok(next)
}

/// Shape and size rules for a stored blob.
pub fn check_blob(v: &Value) -> SocialResult<()> {
    let Value::Object(map) = v else {
        return Err(SocialError::invalid(
            "settings",
            "BASE_TYPE_BAD_FORMAT",
            "must be a JSON object",
        ));
    };
    if map.len() > MAX_TOP_LEVEL_KEYS {
        return Err(SocialError::invalid(
            "settings",
            "BASE_TYPE_MAX_LENGTH",
            format!("at most {MAX_TOP_LEVEL_KEYS} top-level keys"),
        ));
    }
    if depth(v) > MAX_DEPTH {
        return Err(SocialError::invalid(
            "settings",
            "BASE_TYPE_MAX_DEPTH",
            format!("nested at most {MAX_DEPTH} levels deep"),
        ));
    }
    let size = serde_json::to_vec(v).map_or(usize::MAX, |b| b.len());
    if size > MAX_SETTINGS_BYTES {
        return Err(SocialError::invalid(
            "settings",
            "BASE_TYPE_MAX_LENGTH",
            format!("at most {} KiB", MAX_SETTINGS_BYTES / 1024),
        ));
    }
    Ok(())
}

/// Partial update of the privacy settings; `None` leaves a field alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrivacyUpdate {
    pub friend_everyone: Option<bool>,
    pub friend_of_friends: Option<bool>,
    pub friend_server_members: Option<bool>,
    pub allow_server_member_dms: Option<bool>,
    pub message_requests: Option<bool>,
    pub media_friends: Option<MediaFilter>,
    pub media_others: Option<MediaFilter>,
    pub media_servers: Option<MediaFilter>,
    pub dm_spam_filter: Option<SpamFilter>,
    /// Guild id to override: `Some(allowed)` sets, `None` removes.
    pub server_dm_overrides: Option<BTreeMap<u64, Option<bool>>>,
}

/// Most highlight keywords per user.
pub const MAX_KEYWORDS: usize = 50;
/// Longest keyword or phrase, in characters.
pub const MAX_KEYWORD_CHARS: usize = 100;

/// Clean up a keyword list; see [`SocialService::set_notification_keywords`].
pub fn normalize_keywords(raw: &[String]) -> SocialResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for k in raw {
        let k = k.split_whitespace().collect::<Vec<_>>().join(" ");
        if k.is_empty() || out.iter().any(|o| o.to_lowercase() == k.to_lowercase()) {
            continue;
        }
        if k.chars().count() > MAX_KEYWORD_CHARS || !k.chars().any(char::is_alphanumeric) {
            return Err(SocialError::invalid(
                "keywords",
                "BASE_TYPE_BAD_LENGTH",
                format!(
                    "each keyword needs a letter or digit and at most {MAX_KEYWORD_CHARS} characters"
                ),
            ));
        }
        out.push(k);
    }
    if out.len() > MAX_KEYWORDS {
        return Err(SocialError::invalid(
            "keywords",
            "BASE_TYPE_MAX_LENGTH",
            format!("at most {MAX_KEYWORDS} keywords"),
        ));
    }
    Ok(out)
}

impl<R: SocialRepository> SocialService<R> {
    /// The synced settings blob (empty, version 0, when never saved).
    pub async fn user_settings(&self, user: UserId) -> SocialResult<UserSettings> {
        Ok(self
            .repo()
            .get_user_settings(user)
            .await?
            .unwrap_or_default())
    }

    /// Merge `patch` into the blob. Concurrent writers are serialised by the version
    /// column; a lost race re-reads and re-applies the patch.
    pub async fn patch_user_settings(
        &self,
        user: UserId,
        patch: &Value,
    ) -> SocialResult<UserSettings> {
        for _ in 0..WRITE_ATTEMPTS {
            let current = self.user_settings(user).await?;
            let next = apply_patch(&current.data, patch)?;
            if let Some(version) = self
                .repo()
                .put_user_settings(user, &next, current.version)
                .await?
            {
                return Ok(UserSettings {
                    data: next,
                    version,
                });
            }
        }
        Err(SocialError::Conflict)
    }

    /// The user's own default notification level (`None` = follow each server's default).
    pub async fn default_notification_level(
        &self,
        user: UserId,
    ) -> SocialResult<Option<NotificationLevel>> {
        Ok(self.repo().get_default_notification_level(user).await?)
    }

    pub async fn set_default_notification_level(
        &self,
        user: UserId,
        level: Option<NotificationLevel>,
    ) -> SocialResult<()> {
        Ok(self
            .repo()
            .put_default_notification_level(user, level)
            .await?)
    }

    /// The user's highlight keywords (empty by default).
    pub async fn notification_keywords(&self, user: UserId) -> SocialResult<Vec<String>> {
        Ok(self.repo().get_notification_keywords(user).await?)
    }

    /// Replace the keyword list: trimmed, whitespace collapsed, blank and duplicate (any case)
    /// entries dropped. Returns what was stored.
    pub async fn set_notification_keywords(
        &self,
        user: UserId,
        raw: &[String],
    ) -> SocialResult<Vec<String>> {
        let clean = normalize_keywords(raw)?;
        self.repo().put_notification_keywords(user, &clean).await?;
        Ok(clean)
    }

    /// Who may send friend requests and DMs.
    pub async fn privacy(&self, user: UserId) -> SocialResult<super::repo::Privacy> {
        Ok(self.repo().get_privacy(user).await?)
    }

    pub async fn update_privacy(
        &self,
        user: UserId,
        up: PrivacyUpdate,
    ) -> SocialResult<super::repo::Privacy> {
        let mut p = self.repo().get_privacy(user).await?;
        let fr = &mut p.friend_requests;
        *fr = FriendRequestSources {
            everyone: up.friend_everyone.unwrap_or(fr.everyone),
            friends_of_friends: up.friend_of_friends.unwrap_or(fr.friends_of_friends),
            server_members: up.friend_server_members.unwrap_or(fr.server_members),
        };
        if let Some(v) = up.allow_server_member_dms {
            p.dms.allow_server_member_dms = v;
        }
        if let Some(v) = up.message_requests {
            p.dms.message_requests = v;
        }
        let s = &mut p.safety;
        if let Some(v) = up.media_friends {
            s.sensitive_media.friends = v;
        }
        if let Some(v) = up.media_others {
            s.sensitive_media.others = v;
        }
        if let Some(v) = up.media_servers {
            s.sensitive_media.servers = v;
        }
        if let Some(v) = up.dm_spam_filter {
            s.dm_spam_filter = v;
        }
        for (guild, change) in up.server_dm_overrides.iter().flatten() {
            match change {
                Some(allowed) => {
                    s.server_dm_overrides.insert(*guild, *allowed);
                }
                None => {
                    s.server_dm_overrides.remove(guild);
                }
            }
        }
        if s.server_dm_overrides.len() > MAX_SERVER_DM_OVERRIDES {
            return Err(SocialError::invalid(
                "server_dm_overrides",
                "BASE_TYPE_MAX_LENGTH",
                format!("at most {MAX_SERVER_DM_OVERRIDES} servers"),
            ));
        }
        self.repo().put_privacy(user, &p).await?;
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_patch_follows_rfc_7386() {
        let mut doc = json!({"a": "b", "c": {"d": "e", "f": "g"}});
        merge_patch(&mut doc, &json!({"a": "z", "c": {"f": null}}));
        assert_eq!(doc, json!({"a": "z", "c": {"d": "e"}}));
        // Arrays replace, scalars replace objects, null removes.
        merge_patch(&mut doc, &json!({"c": [1, 2], "a": null}));
        assert_eq!(doc, json!({"c": [1, 2]}));
        merge_patch(&mut doc, &json!({"c": {"x": 1}}));
        assert_eq!(doc, json!({"c": {"x": 1}}));
    }

    #[test]
    fn patch_must_be_an_object() {
        let cur = json!({});
        for bad in [json!(1), json!("x"), json!([1]), json!(null)] {
            assert!(apply_patch(&cur, &bad).is_err(), "{bad}");
        }
        assert_eq!(
            apply_patch(&cur, &json!({"theme": "ash"})).unwrap(),
            json!({"theme": "ash"})
        );
    }

    #[test]
    fn caps_size_depth_and_keys() {
        let big = json!({"blob": "x".repeat(MAX_SETTINGS_BYTES)});
        assert!(apply_patch(&json!({}), &big).is_err());

        let mut deep = json!(1);
        for _ in 0..=MAX_DEPTH {
            deep = json!({ "n": deep });
        }
        assert!(apply_patch(&json!({}), &deep).is_err());
        let mut ok = json!(1);
        for _ in 0..MAX_DEPTH - 1 {
            ok = json!({ "n": ok });
        }
        assert!(apply_patch(&json!({}), &ok).is_ok());

        let many: Map<String, Value> = (0..=MAX_TOP_LEVEL_KEYS)
            .map(|i| (format!("k{i}"), json!(i)))
            .collect();
        assert!(apply_patch(&json!({}), &Value::Object(many)).is_err());
    }

    #[test]
    fn deleting_keys_can_bring_a_blob_back_under_the_cap() {
        let cur = json!({"a": "x".repeat(MAX_SETTINGS_BYTES - 100)});
        let next = apply_patch(&cur, &json!({"a": null, "b": 1})).unwrap();
        assert_eq!(next, json!({"b": 1}));
    }
}
