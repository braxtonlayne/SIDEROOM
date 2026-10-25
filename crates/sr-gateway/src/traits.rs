//! Seams that keep the gateway independent of sr-auth / sr-db / sr-api.

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::{ChannelId, GuildId, Permissions, UserId};
use thiserror::Error;

use crate::session::SessionHandle;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("invalid token")]
    Invalid,
    #[error("auth backend unavailable: {0}")]
    Unavailable(String),
}

/// Who a token belongs to.
#[derive(Debug, Clone)]
pub struct AuthedUser {
    pub user_id: UserId,
    pub bot: bool,
    /// Privileged intent bits this identity may use (bots only; users get all).
    pub allowed_privileged_intents: u64,
    /// User object placed in READY (`id` should be set).
    pub user: Value,
}

#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Resolve a gateway token (bot token or user session token).
    async fn authenticate(&self, token: &str) -> Result<AuthedUser, AuthError>;

    /// Resolve a browser `sr_session` cookie value. Used only when IDENTIFY or
    /// RESUME carries an empty token, the upgrade request has the cookie, and
    /// its `Origin` is in [`crate::GatewayConfig::allowed_origins`]. Default: refuse.
    async fn authenticate_session_cookie(&self, _cookie: &str) -> Result<AuthedUser, AuthError> {
        Err(AuthError::Invalid)
    }
}

/// A pluggable handler for client opcodes the gateway does not implement itself
/// (voice, member requests, watch parties, E2EE). Registered in
/// [`crate::GatewayConfig::handlers`]. Opcodes no handler claims are ignored.
#[async_trait]
pub trait OpHandler: Send + Sync {
    /// Opcodes this handler wants.
    fn ops(&self) -> &[u8];

    /// Called once per matching client op from an authenticated session.
    async fn handle(&self, session: &SessionHandle, op: u8, d: Value);
}

/// Guild membership and visibility lookups. The permission decision
/// (`VIEW_CHANNEL` after overwrites) lives behind `can_view_channel`; the
/// gateway never computes permissions itself.
#[async_trait]
pub trait GuildDirectory: Send + Sync {
    async fn guilds_for_user(&self, user: UserId) -> Vec<GuildId>;

    async fn can_view_channel(&self, user: UserId, guild: GuildId, channel: ChannelId) -> bool;

    /// Whether `user` holds all of `needed` at guild level (owner and
    /// administrator bypass applied). Gates moderation and configuration
    /// dispatches such as `GUILD_AUDIT_LOG_ENTRY_CREATE`; see
    /// [`crate::hub::required_guild_permission`]. Default: no (fail closed).
    async fn has_guild_permission(
        &self,
        _user: UserId,
        _guild: GuildId,
        _needed: Permissions,
    ) -> bool {
        false
    }

    /// Of `users`, those a guild dispatch may reach: each can view `channel` (when set) and holds `needed`
    /// at guild level (when set), exactly as [`Self::can_view_channel`] and [`Self::has_guild_permission`]
    /// decide. The hub asks once per dispatch. The default asks per user; a store should read the channel,
    /// guild, roles and overwrites once and the members in one query (RDY-PERF-002: the per-recipient
    /// lookups were about two thirds of the database work under load).
    async fn dispatch_audience(
        &self,
        guild: GuildId,
        channel: Option<ChannelId>,
        needed: Option<Permissions>,
        users: &[UserId],
    ) -> std::collections::HashSet<UserId> {
        let checks = users.iter().map(|&u| async move {
            let mut ok = true;
            if let Some(ch) = channel {
                ok = self.can_view_channel(u, guild, ch).await;
            }
            if ok && let Some(p) = needed {
                ok = self.has_guild_permission(u, guild, p).await;
            }
            ok.then_some(u)
        });
        futures::future::join_all(checks)
            .await
            .into_iter()
            .flatten()
            .collect()
    }

    /// Body of `GUILD_CREATE` for `user`. Default is a bare stub.
    async fn guild_payload(&self, _user: UserId, guild: GuildId) -> Value {
        json!({ "id": guild })
    }

    /// Body of `THREAD_LIST_SYNC` for `user` in `guild` (the active threads they can
    /// see and their own memberships), sent right after `GUILD_CREATE`. `None` when
    /// there is nothing to sync. Default: none.
    async fn thread_list_sync(&self, _user: UserId, _guild: GuildId) -> Option<Value> {
        None
    }

    /// Members of the staff guild; they receive `SR_STAFF_ALERT` dispatches
    /// (open question 142). Default: nobody.
    async fn staff_users(&self) -> Vec<UserId> {
        Vec::new()
    }

    /// Accepted friends of `user`, who receive their `PRESENCE_UPDATE`s outside any
    /// guild. Default: none.
    async fn friends_of(&self, _user: UserId) -> Vec<UserId> {
        Vec::new()
    }

    /// What `channel` is for voice purposes. `None` when it does not exist in `guild`.
    /// Default: unknown (voice joins are refused).
    async fn voice_channel(
        &self,
        _guild: GuildId,
        _channel: ChannelId,
    ) -> Option<crate::voice::VoiceChannelInfo> {
        None
    }

    /// The guild a channel belongs to. `None` for unknown channels and DMs. Default: unknown.
    async fn channel_guild(&self, _channel: ChannelId) -> Option<GuildId> {
        None
    }

    /// The soundboard sounds of `guild` as `SOUNDBOARD_SOUNDS` carries them. Default: none.
    async fn soundboard_sounds(&self, _guild: GuildId) -> Vec<Value> {
        Vec::new()
    }

    /// Per-guild voice settings (AFK channel and timeout). Default: none configured.
    async fn voice_guild(&self, _guild: GuildId) -> crate::voice::VoiceGuildInfo {
        crate::voice::VoiceGuildInfo::default()
    }

    /// Whether `user` answered yes to "Are you 18 or older?" (D-032), which age-restricted
    /// voice channels need. Default: no (fail closed).
    async fn age_confirmed(&self, _user: UserId) -> bool {
        false
    }

    /// The sensitive-media settings and the friends of `user` (D-230, H-33), cached per session
    /// until `SR_USER_UPDATE` or a `RELATIONSHIP_*` dispatch. `None` (the default, and for bots)
    /// leaves message attachments as they are.
    async fn sensitive_view(
        &self,
        _user: UserId,
    ) -> Option<(
        sr_chat::social::safety::SensitiveMedia,
        std::collections::HashSet<UserId>,
    )> {
        None
    }

    /// Whether `channel`, or the parent of a thread, is age-restricted (`nsfw`). The gateway
    /// withholds message, reaction, typing, pin and thread dispatches of such a channel from
    /// sessions whose account has not answered yes to the age gate (D-199). Default: no.
    async fn channel_age_restricted(&self, _channel: ChannelId) -> bool {
        false
    }

    /// The video allowance `user` has in `guild` (`None`: a DM call): sender and viewer caps
    /// from the server level, Patron exemption (D-050..D-052). Default: the free tier.
    async fn voice_limits(
        &self,
        _user: UserId,
        _guild: Option<GuildId>,
    ) -> crate::voice::VoiceLimits {
        crate::voice::VoiceLimits::free()
    }

    /// The members of the DM or group DM `channel` when `user` is one of them. `None`
    /// when the channel is not a DM they belong to. Default: none (calls are refused).
    async fn dm_call_members(&self, _channel: ChannelId, _user: UserId) -> Option<Vec<UserId>> {
        None
    }

    /// `user`'s effective permissions in `channel` (overwrites applied, owner and
    /// administrator bypass, timeouts restricted). `None` when they are not a member of
    /// `guild`. Default: not a member (fail closed).
    async fn channel_permissions(
        &self,
        _user: UserId,
        _guild: GuildId,
        _channel: ChannelId,
    ) -> Option<Permissions> {
        None
    }

    /// Guild member object for voice state payloads. Default: none.
    async fn member_payload(&self, _guild: GuildId, _user: UserId) -> Option<Value> {
        None
    }

    /// The user's last attached socket closed: they are offline now (LNG-INV-005 uses this to
    /// drop temporary memberships). Called once per transition, after the presence was
    /// broadcast. Default: nothing.
    async fn user_offline(&self, _user: UserId) {}

    /// The members of `guild` who can see `channel`, for the lazy member list (op 102,
    /// LNG-GLD-010). `None` when the directory cannot build one (default; the request is
    /// then ignored).
    async fn member_list(
        &self,
        _guild: GuildId,
        _channel: ChannelId,
    ) -> Option<crate::member_list::MemberListSource> {
        None
    }

    /// The members an op 8 `REQUEST_GUILD_MEMBERS` asks for. `None` when the directory cannot
    /// answer (default; the request is then ignored).
    async fn request_members(
        &self,
        _guild: GuildId,
        _request: &MemberRequest,
    ) -> Option<RequestedMembers> {
        None
    }

    /// Extra READY fields (`private_channels`, `relationships`, ...).
    async fn ready_extras(&self, _user: UserId) -> Value {
        json!({})
    }
}

/// What an op 8 `REQUEST_GUILD_MEMBERS` asks the directory for (D-242).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberRequest {
    /// Members whose nickname, username or display name contains `query` (the same search the
    /// REST route `GET /guilds/{id}/members/search` runs), ordered by user id. The empty query
    /// is everyone. `limit` 0 means no limit.
    Search { query: String, limit: usize },
    /// Exactly these users (at most 100).
    Ids(Vec<UserId>),
}

/// The directory's answer to a [`MemberRequest`].
#[derive(Debug, Clone, Default)]
pub struct RequestedMembers {
    /// Member objects as the REST search returns them, without `sr_join` and `sr_signals`.
    pub members: Vec<Value>,
    /// Requested ids that are not members of the guild.
    pub not_found: Vec<UserId>,
}
