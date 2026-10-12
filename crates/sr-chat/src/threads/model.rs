//! Thread and forum state carried on [`crate::domain::Channel`] (docs/04 `thread_metadata`,
//! `thread_members`, forum columns of `channels`).
//!
//! Timestamps are Unix milliseconds. Tag and emoji ids are snowflakes, serialised as
//! strings the way Discord does.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sr_core::{ChannelId, UserId};

/// Valid `auto_archive_duration` values, in minutes (1 hour, 1 day, 3 days, 7 days).
pub const AUTO_ARCHIVE_DURATIONS: [u32; 4] = [60, 1440, 4320, 10_080];
/// Default when neither the request nor the parent channel names one.
pub const DEFAULT_AUTO_ARCHIVE_MINUTES: u32 = 1440;
/// `message_count` stops counting here, as in Discord.
pub const MESSAGE_COUNT_CAP: u32 = 50;
/// Members per thread.
pub const MAX_THREAD_MEMBERS: usize = 1000;
/// Tags on one forum post.
pub const MAX_APPLIED_TAGS: usize = 5;
/// Tags a forum channel can define.
pub const MAX_AVAILABLE_TAGS: usize = 20;
pub const TAG_NAME_MAX: usize = 20;
pub const THREAD_NAME_MAX: usize = 100;

/// Channel `flags` bits that concern threads and forums (Discord positions).
pub mod channel_flags {
    /// A pinned post in a forum or media channel.
    pub const PINNED: u32 = 1 << 1;
    /// Forum channel: a post needs at least one tag.
    pub const REQUIRE_TAG: u32 = 1 << 4;
    /// Media channel: hide the download button.
    pub const HIDE_MEDIA_DOWNLOAD_OPTIONS: u32 = 1 << 15;
    /// Bits a client may change on a forum or media channel.
    pub const FORUM_SETTABLE: u32 = REQUIRE_TAG | HIDE_MEDIA_DOWNLOAD_OPTIONS;
}

/// `default_sort_order` values.
pub mod sort_order {
    pub const LATEST_ACTIVITY: u8 = 0;
    pub const CREATION_DATE: u8 = 1;
}

/// `default_forum_layout` values.
pub mod forum_layout {
    pub const NOT_SET: u8 = 0;
    pub const LIST: u8 = 1;
    pub const GALLERY: u8 = 2;
}

mod id_string {
    use super::{Deserialize, Deserializer, Serializer};

    #[allow(clippy::ref_option)]
    pub fn serialize<S: Serializer>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(n) => s.serialize_str(&n.to_string()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Str(String),
            Num(u64),
        }
        Ok(match Option::<Raw>::deserialize(d)? {
            Some(Raw::Str(s)) => s.parse().ok(),
            Some(Raw::Num(n)) => Some(n),
            None => None,
        })
    }
}

fn ser_id<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}

fn de_id<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Str(String),
        Num(u64),
    }
    match Raw::deserialize(d)? {
        Raw::Str(s) => s.parse().map_err(serde::de::Error::custom),
        Raw::Num(n) => Ok(n),
    }
}

/// A forum tag (`available_tags` entry).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForumTag {
    #[serde(serialize_with = "ser_id", deserialize_with = "de_id")]
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub moderated: bool,
    #[serde(default, with = "id_string")]
    pub emoji_id: Option<u64>,
    #[serde(default)]
    pub emoji_name: Option<String>,
}

/// `default_reaction_emoji`: shown as the add-reaction button on posts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultReaction {
    #[serde(default, with = "id_string")]
    pub emoji_id: Option<u64>,
    #[serde(default)]
    pub emoji_name: Option<String>,
}

/// Forum and media channel settings, plus the thread defaults every threadable
/// channel carries. Post guidelines live in `Channel::topic`; the required-tag
/// switch is the `REQUIRE_TAG` bit of `Channel::flags`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForumConfig {
    pub available_tags: Vec<ForumTag>,
    pub default_reaction_emoji: Option<DefaultReaction>,
    pub default_sort_order: Option<u8>,
    pub default_forum_layout: Option<u8>,
    /// Slowmode (seconds) new posts start with.
    pub default_thread_rate_limit: Option<u32>,
    /// `auto_archive_duration` new threads start with.
    pub default_auto_archive_duration: Option<u32>,
}

impl ForumConfig {
    pub fn tag(&self, id: u64) -> Option<&ForumTag> {
        self.available_tags.iter().find(|t| t.id == id)
    }
}

/// A tag as sent by a client: the id is minted by the server when absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagInput {
    pub id: Option<u64>,
    pub name: String,
    pub moderated: bool,
    pub emoji_id: Option<u64>,
    pub emoji_name: Option<String>,
}

/// Forum and thread-default changes carried by create/modify channel requests.
/// `None` leaves a setting alone; `Some(None)` clears it where clearing makes sense.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ForumPatch {
    pub available_tags: Option<Vec<TagInput>>,
    pub default_reaction_emoji: Option<Option<DefaultReaction>>,
    pub default_sort_order: Option<Option<u8>>,
    pub default_forum_layout: Option<u8>,
    pub default_thread_rate_limit: Option<u32>,
    pub default_auto_archive_duration: Option<u32>,
    /// Only [`channel_flags::FORUM_SETTABLE`] bits are applied.
    pub flags: Option<u32>,
}

impl ForumPatch {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Give every tag without an id one from `next`.
    pub fn assign_tag_ids(&mut self, mut next: impl FnMut() -> u64) {
        for t in self.available_tags.iter_mut().flatten() {
            if t.id.is_none() {
                t.id = Some(next());
            }
        }
    }
}

/// Thread-only state (`thread_metadata` plus the counters of the channel object).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadInfo {
    pub archived: bool,
    pub auto_archive_duration: u32,
    /// When `archived` last changed (creation time for a fresh thread).
    pub archive_timestamp_ms: i64,
    pub locked: bool,
    pub invitable: bool,
    pub create_timestamp_ms: i64,
    /// Last message or unarchive; the auto-archive clock runs from here.
    pub last_activity_ms: i64,
    /// Messages sent since creation minus deletions; reported capped at
    /// [`MESSAGE_COUNT_CAP`].
    pub message_count: u32,
    /// Every message ever sent, deletions included.
    pub total_message_sent: u32,
    pub member_count: u32,
    /// Forum tag ids applied to this post.
    pub applied_tags: Vec<u64>,
}

impl ThreadInfo {
    pub fn new(now_ms: i64, auto_archive_duration: u32) -> Self {
        Self {
            archived: false,
            auto_archive_duration,
            archive_timestamp_ms: now_ms,
            locked: false,
            invitable: true,
            create_timestamp_ms: now_ms,
            last_activity_ms: now_ms,
            message_count: 0,
            total_message_sent: 0,
            member_count: 0,
            applied_tags: Vec::new(),
        }
    }

    /// `message_count` as reported to clients.
    pub fn reported_message_count(&self) -> u32 {
        self.message_count.min(MESSAGE_COUNT_CAP)
    }

    /// Whether the auto-archive timer has run out at `now_ms`.
    pub fn is_idle(&self, now_ms: i64) -> bool {
        !self.archived
            && self.last_activity_ms + i64::from(self.auto_archive_duration) * 60_000 <= now_ms
    }
}

/// A row of `thread_members`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadMember {
    pub thread_id: ChannelId,
    pub user_id: UserId,
    pub joined_at_ms: i64,
    pub flags: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_json_uses_string_ids() {
        let t = ForumTag {
            id: 7,
            name: "bug".into(),
            moderated: true,
            emoji_id: Some(9),
            emoji_name: None,
        };
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v["id"], "7");
        assert_eq!(v["emoji_id"], "9");
        assert_eq!(serde_json::from_value::<ForumTag>(v).unwrap(), t);
        let numeric: ForumTag = serde_json::from_str(r#"{"id":5,"name":"x"}"#).unwrap();
        assert_eq!(numeric.id, 5);
        assert!(!numeric.moderated);
    }

    #[test]
    fn idle_clock_runs_from_last_activity() {
        let mut t = ThreadInfo::new(0, 60);
        assert!(!t.is_idle(59 * 60_000));
        assert!(t.is_idle(60 * 60_000));
        t.last_activity_ms = 30 * 60_000;
        assert!(!t.is_idle(60 * 60_000));
        t.archived = true;
        assert!(!t.is_idle(10_000 * 60_000));
    }

    #[test]
    fn message_count_is_capped() {
        let mut t = ThreadInfo::new(0, 60);
        t.message_count = 400;
        assert_eq!(t.reported_message_count(), 50);
    }
}
