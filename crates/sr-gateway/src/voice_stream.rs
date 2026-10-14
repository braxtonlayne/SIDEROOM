//! Go Live streams (VOX-SIG-010) and the allowances behind them (VOX-SIG-009).
//!
//! # Streams
//!
//! A stream is a screen share inside a voice channel. Its **stream key** is
//! `guild:{guild}:{channel}:{user}` (`call:{channel}:{user}` in a DM call), as on Discord.
//!
//! | Client op | Meaning | Dispatches |
//! |---|---|---|
//! | 18 `STREAM_CREATE` `{channel_id, guild_id?, height?, fps?}` | start sharing | `VOICE_STATE_UPDATE` (`self_stream`), `STREAM_CREATE` to the streamer |
//! | 19 `STREAM_DELETE` `{stream_key}` | the owner stops, a viewer stops watching | `STREAM_DELETE` to the owner and viewers (or the viewer), `STREAM_UPDATE` |
//! | 20 `STREAM_WATCH` `{stream_key}` | start watching | `STREAM_CREATE` to the viewer, `STREAM_UPDATE` to the owner and viewers |
//! | 22 `STREAM_SET_PAUSED` `{stream_key, paused}` | owner pauses | `STREAM_UPDATE` |
//!
//! Refusals arrive as `SR_VOICE_ERROR` (`video_limit_reached`, `stream_quality_not_allowed`,
//! `stream_full`, ...) with the numbers the toast needs (`limit`, `count`).
//!
//! # Allowances (D-050..D-052)
//!
//! * A channel holds at most `video_senders` simultaneous sends (a camera and a screen
//!   share each count as one). Patrons never count (`exempt`) and are never refused.
//! * A stream has at most `stream_viewers` viewers; the next one is refused with
//!   `stream_full`.
//! * Limits are re-read when the server's level changes (`SR_ENTITLEMENT_UPDATE`). A lower
//!   limit only blocks new senders and viewers: nobody is removed.
//!
//! # Previews
//!
//! The streamer uploads a JPEG thumbnail now and then (`POST /streams/{key}/preview`).
//! The newest one is kept in memory and served to everyone who may see the channel.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, Permissions, UserId};
use sr_voice_proto::VoiceCommand;
use tokio::time::Instant;

use crate::hub::Gateway;
use crate::session::Session;
use crate::voice::{JoinDenied, VoiceState, allowance_ok, media_flags, unix_ms};

/// Largest preview we keep.
pub const MAX_PREVIEW_BYTES: usize = 256 * 1024;
/// A streamer may replace the preview at most this often.
pub const PREVIEW_MIN_INTERVAL: Duration = Duration::from_secs(5);
/// Default stream quality when a client does not say (720p30 is allowed to everyone).
const DEFAULT_QUALITY: (u16, u8) = (720, 30);

/// `guild:{g}:{c}:{u}` or, in a DM call, `call:{c}:{u}`.
pub fn stream_key(guild: Option<GuildId>, channel: ChannelId, owner: UserId) -> String {
    match guild {
        Some(g) => format!("guild:{g}:{channel}:{owner}"),
        None => format!("call:{channel}:{owner}"),
    }
}

/// A stream that is on.
#[derive(Debug, Clone)]
pub(crate) struct StreamState {
    pub key: String,
    pub guild_id: Option<GuildId>,
    pub channel_id: ChannelId,
    pub owner: UserId,
    pub height: u16,
    pub fps: u8,
    pub paused: bool,
    pub viewers: BTreeSet<UserId>,
    /// Viewer cap taken from the owner's limits when the stream started or the level changed.
    pub max_viewers: u16,
    pub preview: Option<Preview>,
}

#[derive(Debug, Clone)]
pub(crate) struct Preview {
    pub jpeg: Arc<Vec<u8>>,
    pub uploaded_ms: i64,
    pub at: Instant,
}

impl StreamState {
    fn viewer_ids(&self) -> Vec<UserId> {
        self.viewers.iter().copied().collect()
    }

    fn body(&self) -> Value {
        json!({
            "stream_key": self.key,
            "rtc_server_id": self.channel_id,
            "region": "auto",
            "viewer_ids": self.viewer_ids(),
            "paused": self.paused,
            "max_viewers": self.max_viewers,
            "quality": { "height": self.height, "fps": self.fps },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewError {
    Unavailable,
    /// No such stream, or the caller may not see it.
    NotFound,
    /// Only the streamer uploads.
    NotOwner,
    NotJpeg,
    TooLarge,
    /// Uploaded again before [`PREVIEW_MIN_INTERVAL`] passed.
    RateLimited,
}

/// `true` when `data` starts like a JPEG.
pub fn looks_like_jpeg(data: &[u8]) -> bool {
    data.len() > 4 && data.starts_with(&[0xFF, 0xD8, 0xFF])
}

#[derive(Debug, Deserialize)]
struct StreamCreateIn {
    guild_id: Option<GuildId>,
    channel_id: ChannelId,
    #[serde(default)]
    height: Option<u16>,
    #[serde(default)]
    fps: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct StreamKeyIn {
    stream_key: String,
    #[serde(default)]
    paused: Option<bool>,
}

impl Gateway {
    /// Tell the SFU who may receive the screen of `s` (it forwards it to nobody else).
    async fn relay_viewers(&self, s: &StreamState, live: bool) {
        self.relay(VoiceCommand::StreamViewers {
            channel: s.channel_id,
            owner: s.owner,
            viewers: s.viewers.iter().copied().collect(),
            live,
        })
        .await;
    }

    /// Users who should hear about `s`: the owner and every viewer.
    fn stream_audience(s: &StreamState) -> Vec<UserId> {
        let mut v = vec![s.owner];
        v.extend(s.viewers.iter().copied());
        v
    }

    async fn stream_tell(&self, users: &[UserId], name: &str, body: &Value) {
        for u in users {
            self.dispatch_user(*u, name, body.clone()).await;
        }
    }

    /// The streams of `channel`, newest data, for READY-style syncs and tests.
    pub fn stream_keys(&self, channel: ChannelId) -> Vec<String> {
        let mut keys: Vec<String> = self
            .st()
            .vx
            .streams
            .values()
            .filter(|s| s.channel_id == channel)
            .map(|s| s.key.clone())
            .collect();
        keys.sort();
        keys
    }

    /// Viewers of a stream (tests and the REST layer).
    pub fn stream_viewers(&self, key: &str) -> Vec<UserId> {
        self.st()
            .vx
            .streams
            .get(key)
            .map(StreamState::viewer_ids)
            .unwrap_or_default()
    }

    /// Ops 18, 19, 20 and 22.
    pub(crate) async fn stream_op(&self, s: &Arc<Session>, op: u8, d: Value) {
        use crate::protocol::op as o;
        match op {
            o::STREAM_CREATE => self.stream_create(s, d).await,
            o::STREAM_DELETE => self.stream_delete(s, d).await,
            o::STREAM_WATCH => self.stream_watch(s, d).await,
            o::STREAM_SET_PAUSED => self.stream_set_paused(s, d).await,
            _ => {}
        }
    }

    /// Op 18.
    pub(crate) async fn stream_create(&self, s: &Arc<Session>, d: Value) {
        let Ok(inp) = serde_json::from_value::<StreamCreateIn>(d) else {
            self.voice_error(s, None, None, JoinDenied::BadRequest);
            return;
        };
        if let Err((e, extra)) = self.stream_start(s.user, inp).await {
            self.voice_error_with(s, None, None, e, extra);
        }
    }

    async fn stream_start(
        &self,
        user: UserId,
        inp: StreamCreateIn,
    ) -> Result<(), (JoinDenied, Value)> {
        let plain = |e| (e, Value::Null);
        if self.voice_cfg().is_none() {
            return Err(plain(JoinDenied::Unavailable));
        }
        let cur = self
            .voice_state(user)
            .filter(|c| c.channel_id == inp.channel_id && c.guild_id == inp.guild_id)
            .ok_or_else(|| plain(JoinDenied::NotInVoice))?;
        let perms = match cur.guild_id {
            Some(g) => self
                .inner
                .dir
                .channel_permissions(user, g, cur.channel_id)
                .await
                .ok_or_else(|| plain(JoinDenied::NotMember))?,
            None => crate::voice::DM_PERMS,
        };
        if !perms.contains(Permissions::STREAM) {
            return Err(plain(JoinDenied::MissingPermission));
        }
        if cur.suppress || cur.afk {
            // The stage audience and the AFK channel only listen.
            return Err(plain(JoinDenied::MissingPermission));
        }
        if cur.self_stream {
            return Err(plain(JoinDenied::AlreadyStreaming));
        }
        let limits = self.inner.dir.voice_limits(user, cur.guild_id).await;
        let (height, fps) = match (inp.height, inp.fps) {
            (Some(h), Some(f)) => (h, f),
            (Some(h), None) => (h, DEFAULT_QUALITY.1),
            (None, Some(f)) => (DEFAULT_QUALITY.0, f),
            (None, None) => DEFAULT_QUALITY,
        };
        if !limits.allows(height, fps) {
            let best = limits
                .profiles
                .iter()
                .map(|(h, f)| json!({ "height": h, "fps": f }))
                .collect::<Vec<_>>();
            return Err((
                JoinDenied::StreamQualityNotAllowed,
                json!({ "allowed": best }),
            ));
        }
        let others = self.sender_count(cur.channel_id, user);
        let mine_after = u16::from(cur.self_video) + 1;
        if !allowance_ok(others, mine_after, limits.video_senders, limits.exempt) {
            return Err((
                JoinDenied::VideoLimitReached,
                json!({ "limit": limits.video_senders, "count": others }),
            ));
        }

        let key = stream_key(cur.guild_id, cur.channel_id, user);
        let stream = StreamState {
            key: key.clone(),
            guild_id: cur.guild_id,
            channel_id: cur.channel_id,
            owner: user,
            height,
            fps,
            paused: false,
            viewers: BTreeSet::new(),
            max_viewers: limits.stream_viewers,
            preview: None,
        };
        let mut new = cur.clone();
        new.self_stream = true;
        new.exempt = limits.exempt;
        new.tier = limits.tier;
        {
            // Re-check under the lock: two requests must not both slip in.
            let mut st = self.st();
            match st.voice.get(&user) {
                Some(live) if live.session_id == cur.session_id && !live.self_stream => {}
                _ => return Err(plain(JoinDenied::AlreadyStreaming)),
            }
            st.voice.insert(user, new.clone());
            st.vx.streams.insert(key, stream.clone());
        }
        self.voice_touch(user);
        self.relay_flags(&new, media_flags(perms, &new)).await;
        self.relay_viewers(&stream, true).await;
        let member = match new.guild_id {
            Some(g) => self.inner.dir.member_payload(g, user).await,
            None => None,
        };
        self.dispatch_state(&new, new.to_json(true, member), &[new.channel_id], &[user])
            .await;
        self.stream_tell(&[user], "STREAM_CREATE", &stream.body())
            .await;
        Ok(())
    }

    /// Op 19: the owner stops the stream, or a viewer stops watching.
    pub(crate) async fn stream_delete(&self, s: &Arc<Session>, d: Value) {
        let Ok(inp) = serde_json::from_value::<StreamKeyIn>(d) else {
            self.voice_error(s, None, None, JoinDenied::BadRequest);
            return;
        };
        let (owner, watching) = {
            let st = self.st();
            match st.vx.streams.get(&inp.stream_key) {
                Some(x) => (x.owner == s.user, x.viewers.contains(&s.user)),
                None => (false, false),
            }
        };
        if owner {
            self.stream_end(&inp.stream_key, "user_requested").await;
        } else if watching {
            self.stream_unwatch(&inp.stream_key, s.user).await;
        } else {
            self.voice_error(s, None, None, JoinDenied::UnknownStream);
        }
    }

    /// Op 20.
    pub(crate) async fn stream_watch(&self, s: &Arc<Session>, d: Value) {
        let Ok(inp) = serde_json::from_value::<StreamKeyIn>(d) else {
            self.voice_error(s, None, None, JoinDenied::BadRequest);
            return;
        };
        if let Err((e, extra)) = self.stream_join_viewer(s.user, &inp.stream_key).await {
            self.voice_error_with(s, None, None, e, extra);
        }
    }

    async fn stream_join_viewer(&self, user: UserId, key: &str) -> Result<(), (JoinDenied, Value)> {
        let plain = |e| (e, Value::Null);
        let Some(me) = self.voice_state(user) else {
            return Err(plain(JoinDenied::NotInVoice));
        };
        let after = {
            let mut st = self.st();
            let Some(s) = st.vx.streams.get_mut(key) else {
                return Err(plain(JoinDenied::UnknownStream));
            };
            // Watching happens from inside the room the stream is in.
            if s.channel_id != me.channel_id || s.guild_id != me.guild_id {
                return Err(plain(JoinDenied::NotInVoice));
            }
            if s.owner == user {
                return Err(plain(JoinDenied::BadRequest));
            }
            if s.viewers.contains(&user) {
                return Ok(());
            }
            if s.viewers.len() >= usize::from(s.max_viewers) {
                return Err((
                    JoinDenied::StreamFull,
                    json!({ "limit": s.max_viewers, "count": s.viewers.len() }),
                ));
            }
            s.viewers.insert(user);
            s.clone()
        };
        self.relay_viewers(&after, true).await;
        self.stream_tell(&[user], "STREAM_CREATE", &after.body())
            .await;
        self.stream_tell(
            &Self::stream_audience(&after),
            "STREAM_UPDATE",
            &after.body(),
        )
        .await;
        Ok(())
    }

    async fn stream_unwatch(&self, key: &str, viewer: UserId) {
        let after = {
            let mut st = self.st();
            let Some(s) = st.vx.streams.get_mut(key) else {
                return;
            };
            if !s.viewers.remove(&viewer) {
                return;
            }
            s.clone()
        };
        self.relay_viewers(&after, true).await;
        self.stream_tell(
            &[viewer],
            "STREAM_DELETE",
            &json!({ "stream_key": key, "reason": "user_requested", "unavailable": false }),
        )
        .await;
        self.stream_tell(
            &Self::stream_audience(&after),
            "STREAM_UPDATE",
            &after.body(),
        )
        .await;
    }

    /// Op 22.
    pub(crate) async fn stream_set_paused(&self, s: &Arc<Session>, d: Value) {
        let Ok(inp) = serde_json::from_value::<StreamKeyIn>(d) else {
            self.voice_error(s, None, None, JoinDenied::BadRequest);
            return;
        };
        let after = {
            let mut st = self.st();
            match st.vx.streams.get_mut(&inp.stream_key) {
                Some(x) if x.owner == s.user => {
                    x.paused = inp.paused.unwrap_or(true);
                    Some(x.clone())
                }
                _ => None,
            }
        };
        match after {
            Some(a) => {
                self.stream_tell(&Self::stream_audience(&a), "STREAM_UPDATE", &a.body())
                    .await;
            }
            None => self.voice_error(s, None, None, JoinDenied::UnknownStream),
        }
    }

    /// End a stream: tell the owner and viewers, clear `self_stream`.
    pub(crate) async fn stream_end(&self, key: &str, reason: &str) {
        let Some(stream) = self.st().vx.streams.remove(key) else {
            return;
        };
        self.relay_viewers(&stream, false).await;
        let body = json!({ "stream_key": key, "reason": reason, "unavailable": false });
        self.stream_tell(&Self::stream_audience(&stream), "STREAM_DELETE", &body)
            .await;
        // The owner's voice state no longer shows a stream, unless they are gone already.
        let cur = self
            .voice_state(stream.owner)
            .filter(|c| c.channel_id == stream.channel_id && c.self_stream);
        if let Some(cur) = cur {
            let mut new = cur.clone();
            new.self_stream = false;
            let perms = match cur.guild_id {
                Some(g) => self
                    .inner
                    .dir
                    .channel_permissions(cur.user_id, g, cur.channel_id)
                    .await
                    .unwrap_or_else(Permissions::empty),
                None => crate::voice::DM_PERMS,
            };
            self.commit_state_inplace(&cur, new, perms).await;
        }
    }

    /// `old` left its channel: end the stream it owned and stop the ones it watched.
    pub(crate) async fn stream_member_left(&self, old: &VoiceState) {
        let (owned, watched): (Vec<String>, Vec<String>) = {
            let st = self.st();
            let owned = st
                .vx
                .streams
                .values()
                .filter(|s| s.owner == old.user_id && s.channel_id == old.channel_id)
                .map(|s| s.key.clone())
                .collect();
            let watched = st
                .vx
                .streams
                .values()
                .filter(|s| s.channel_id == old.channel_id && s.viewers.contains(&old.user_id))
                .map(|s| s.key.clone())
                .collect();
            (owned, watched)
        };
        for k in owned {
            self.stream_end(&k, "user_requested").await;
        }
        for k in watched {
            self.stream_unwatch(&k, old.user_id).await;
        }
    }

    // ---- previews ---------------------------------------------------------------------------

    /// `POST /streams/{key}/preview`: the streamer replaces the thumbnail.
    pub fn stream_preview_put(
        &self,
        user: UserId,
        key: &str,
        jpeg: Vec<u8>,
    ) -> Result<i64, PreviewError> {
        if self.voice_cfg().is_none() {
            return Err(PreviewError::Unavailable);
        }
        if jpeg.len() > MAX_PREVIEW_BYTES {
            return Err(PreviewError::TooLarge);
        }
        if !looks_like_jpeg(&jpeg) {
            return Err(PreviewError::NotJpeg);
        }
        let mut st = self.st();
        let s = st.vx.streams.get_mut(key).ok_or(PreviewError::NotFound)?;
        if s.owner != user {
            // Someone else's stream is none of their business.
            return Err(PreviewError::NotFound);
        }
        let now = Instant::now();
        if s.preview
            .as_ref()
            .is_some_and(|p| now.saturating_duration_since(p.at) < PREVIEW_MIN_INTERVAL)
        {
            return Err(PreviewError::RateLimited);
        }
        let ms = unix_ms();
        s.preview = Some(Preview {
            jpeg: Arc::new(jpeg),
            uploaded_ms: ms,
            at: now,
        });
        Ok(ms)
    }

    /// `GET /streams/{key}/preview`: the thumbnail, for anyone who can see the channel.
    pub async fn stream_preview_get(
        &self,
        user: UserId,
        key: &str,
    ) -> Result<(Arc<Vec<u8>>, i64), PreviewError> {
        let (guild, channel, preview) = {
            let st = self.st();
            let s = st.vx.streams.get(key).ok_or(PreviewError::NotFound)?;
            (s.guild_id, s.channel_id, s.preview.clone())
        };
        let allowed = match guild {
            Some(g) => self.inner.dir.can_view_channel(user, g, channel).await,
            None => self.call_members(channel).contains(&user),
        };
        if !allowed {
            return Err(PreviewError::NotFound);
        }
        let p = preview.ok_or(PreviewError::NotFound)?;
        Ok((p.jpeg, p.uploaded_ms))
    }

    // ---- entitlement changes -----------------------------------------------------------------

    /// The server's level changed: re-read the limits of everyone in its voice channels.
    /// Lower limits block new senders and viewers only; nobody is removed (spec section 3).
    pub(crate) async fn allowance_refresh(&self, guild: GuildId) {
        let states: Vec<VoiceState> = self
            .st()
            .voice
            .values()
            .filter(|v| v.guild_id == Some(guild))
            .cloned()
            .collect();
        for v in states {
            let limits = self.inner.dir.voice_limits(v.user_id, Some(guild)).await;
            if let Some(live) = self.st().voice.get_mut(&v.user_id) {
                live.exempt = limits.exempt;
            }
            let streams: Vec<String> = self
                .st()
                .vx
                .streams
                .values()
                .filter(|s| s.owner == v.user_id)
                .map(|s| s.key.clone())
                .collect();
            for k in streams {
                if let Some(s) = self.st().vx.streams.get_mut(&k) {
                    s.max_viewers = limits.stream_viewers;
                }
            }
            self.dispatch_user(
                v.user_id,
                "SR_VOICE_LIMITS_UPDATE",
                json!({
                    "guild_id": guild,
                    "video_senders": limits.video_senders,
                    "stream_viewers": limits.stream_viewers,
                    "exempt": limits.exempt,
                }),
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_keys_follow_discords_shape() {
        assert_eq!(
            stream_key(Some(GuildId::new(1)), ChannelId::new(2), UserId::new(3)),
            "guild:1:2:3"
        );
        assert_eq!(
            stream_key(None, ChannelId::new(2), UserId::new(3)),
            "call:2:3"
        );
    }

    #[test]
    fn previews_must_be_jpegs() {
        assert!(looks_like_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]));
        assert!(!looks_like_jpeg(&[0x89, b'P', b'N', b'G', 0, 0]));
        assert!(!looks_like_jpeg(&[0xFF, 0xD8, 0xFF]));
        assert!(!looks_like_jpeg(b""));
    }

    #[test]
    fn the_allowance_counts_senders_and_spares_patrons() {
        // Five senders already in a channel with a limit of five: no sixth.
        assert!(!allowance_ok(5, 1, 5, false));
        // Four others plus my camera is exactly the limit.
        assert!(allowance_ok(4, 1, 5, false));
        // A camera and a screen share are two sends.
        assert!(!allowance_ok(4, 2, 5, false));
        // A Patron is accepted at any count (the sixth camera in the spec).
        assert!(allowance_ok(5, 1, 5, true));
        assert!(allowance_ok(40, 2, 5, true));
    }
}
