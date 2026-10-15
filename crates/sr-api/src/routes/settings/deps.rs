//! Collaborators of the settings endpoints. Registered on the composition root as
//! an [`AppState`](crate::AppState) extension and read with [`crate::routes::Ext`].

use super::events::{NoopSettingsEvents, SettingsEvent, SettingsEvents};
use super::join::{MemberJoiner, UnwiredMemberJoiner};
use super::presence::PresenceCounter;
use super::repo::DynRepo;
use super::timeout::{MemberTimeouter, UnwiredMemberTimeouter};
use crate::{AuthKind, error::ApiResult};
use crate::{HttpError, Principal};
use sr_chat::perms::{channel_permissions, guild_permissions};
use sr_chat::repo::{ChatRepository, UserSummary};
use sr_chat::settings::SettingsError;
use sr_chat::settings::SettingsService;
use sr_chat::settings::repo::SettingsRepository;
use sr_chat::{Channel, ChatError, Guild, Member, RepoError, Role};
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ChannelId, Clock, ErrorCode, GuildId, SnowflakeGenerator, UserId};
use sr_storage::ObjectStorage;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

/// Source of 64-bit entropy for invite codes and webhook tokens. Production
/// wiring must back it with a CSPRNG.
pub type Entropy = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct SettingsDeps {
    pub service: SettingsService<DynRepo>,
    pub chat: Arc<dyn ChatRepository>,
    pub storage: Arc<dyn ObjectStorage>,
    clock: Arc<dyn Clock>,
    events: Arc<dyn SettingsEvents>,
    joiner: Arc<dyn MemberJoiner>,
    timeouter: Arc<dyn MemberTimeouter>,
    presence: Option<Arc<dyn PresenceCounter>>,
}

/// A guild resolved for one acting user.
pub struct GuildAccess {
    pub guild: Guild,
    pub member: Member,
    pub roles: Vec<Role>,
    /// Guild-level permissions (owner and administrator bypass applied).
    pub perms: Permissions,
}

/// A guild channel resolved for one acting user.
pub struct ChannelAccess {
    pub channel: Channel,
    pub guild: GuildAccess,
    /// Permissions in this channel (overwrites applied).
    pub perms: Permissions,
}

impl SettingsDeps {
    /// `worker`/`process` identify this node's snowflake generator; use a process
    /// id no other service on the node uses.
    pub fn new(
        settings: Arc<dyn SettingsRepository>,
        chat: Arc<dyn ChatRepository>,
        storage: Arc<dyn ObjectStorage>,
        clock: Arc<dyn Clock>,
        entropy: Entropy,
        worker: u8,
        process: u8,
    ) -> Result<Self, sr_core::snowflake::SnowflakeError> {
        let ids = SnowflakeGenerator::new(clock.clone(), worker, process)?;
        Ok(Self {
            service: SettingsService::new(DynRepo(settings), clock.clone(), ids, entropy),
            chat,
            storage,
            clock,
            events: Arc::new(NoopSettingsEvents),
            joiner: Arc::new(UnwiredMemberJoiner),
            timeouter: Arc::new(UnwiredMemberTimeouter),
            presence: None,
        })
    }

    /// Where mutation events go (see [`super::events`]).
    pub fn with_events(mut self, events: Arc<dyn SettingsEvents>) -> Self {
        self.events = events;
        self
    }

    /// Observe every audit entry the settings service stores (see
    /// [`super::audit::bus_audit_observer`]).
    pub fn with_audit_observer(mut self, observer: sr_chat::settings::AuditObserver) -> Self {
        self.service = self.service.with_audit_observer(observer);
        self
    }

    /// How an accepted invite adds the member (see [`super::join`]).
    pub fn with_joiner(mut self, joiner: Arc<dyn MemberJoiner>) -> Self {
        self.joiner = joiner;
        self
    }

    /// How AutoMod's `timeout` action reaches the member row (see [`super::timeout`]).
    pub fn with_timeouter(mut self, timeouter: Arc<dyn MemberTimeouter>) -> Self {
        self.timeouter = timeouter;
        self
    }

    /// Where invite previews get `approximate_presence_count` (see [`super::presence`]).
    pub fn with_presence(mut self, presence: Arc<dyn PresenceCounter>) -> Self {
        self.presence = Some(presence);
        self
    }

    pub(crate) fn presence(&self) -> Option<&dyn PresenceCounter> {
        self.presence.as_deref()
    }

    /// Replace the service (limits, retention, presets, spam window).
    pub fn with_service(mut self, service: SettingsService<DynRepo>) -> Self {
        self.service = service;
        self
    }

    pub fn now_ms(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    pub(super) fn timeouter(&self) -> &dyn MemberTimeouter {
        &*self.timeouter
    }

    pub(crate) fn joiner(&self) -> &dyn MemberJoiner {
        &*self.joiner
    }

    pub async fn publish(&self, ev: SettingsEvent) {
        self.events.publish(ev).await;
    }

    /// The guild and the member's guild-level permissions. 404 for an unknown
    /// guild, 403 for a non-member.
    pub async fn guild_access(&self, user: UserId, guild: GuildId) -> ApiResult<GuildAccess> {
        let g = self
            .chat
            .get_guild(guild)
            .await
            .map_err(re)?
            .ok_or(ErrorCode::UnknownGuild)?;
        let member = self
            .chat
            .get_member(guild, user)
            .await
            .map_err(re)?
            .ok_or(ErrorCode::MissingAccess)?;
        let roles = self.chat.list_roles(guild).await.map_err(re)?;
        let perms = guild_permissions(&g, &member, &roles);
        Ok(GuildAccess {
            guild: g,
            member,
            roles,
            perms,
        })
    }

    /// A guild channel and the member's permissions in it. DM channels have no
    /// server settings, so they answer 403 `Cannot execute action on a DM channel`.
    pub async fn channel_access(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> ApiResult<ChannelAccess> {
        let ch = self
            .chat
            .get_channel(channel)
            .await
            .map_err(re)?
            .ok_or(ErrorCode::UnknownChannel)?;
        let gid = ch.guild_id.ok_or(ErrorCode::CannotExecuteOnDm)?;
        let guild = self.guild_access(user, gid).await?;
        let overwrites = self.chat.get_overwrites(channel).await.map_err(re)?;
        let perms = channel_permissions(
            &guild.guild,
            &guild.member,
            &guild.roles,
            &overwrites,
            self.now_ms(),
        );
        if !perms.has(Permissions::VIEW_CHANNEL) {
            return Err(ErrorCode::MissingAccess.into());
        }
        Ok(ChannelAccess {
            channel: ch,
            guild,
            perms,
        })
    }

    /// Display fields for `ids`, keyed by id. Unknown ids are absent.
    pub async fn users(
        &self,
        ids: impl IntoIterator<Item = UserId>,
    ) -> ApiResult<HashMap<UserId, UserSummary>> {
        let mut ids: Vec<UserId> = ids.into_iter().collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let found = self.chat.users(&ids).await.map_err(re)?;
        Ok(found.into_iter().map(|u| (u.id, u)).collect())
    }
}

/// Map a repository failure to a logged 500.
pub fn re(e: RepoError) -> HttpError {
    tracing::error!(error = %e, "repository failure");
    ErrorCode::SrInternal.into()
}

/// Map a settings-domain error to its Discord error body.
pub fn se(e: SettingsError) -> HttpError {
    if let SettingsError::Repo(r) = &e {
        tracing::error!(error = %r, "settings repository failure");
    }
    HttpError(e.to_api_error())
}

/// Map a chat-domain error to its Discord error body.
pub fn ce(e: ChatError) -> HttpError {
    if matches!(e, ChatError::Repo(_)) {
        tracing::error!(error = %e, "chat repository failure");
    }
    HttpError(e.to_api_error())
}

/// Parse a snowflake-typed path segment or body field (50035 on failure).
pub fn id<T: FromStr>(field: &str, raw: &str) -> ApiResult<T> {
    raw.parse().map_err(|_| {
        HttpError(ApiError::invalid_form().field(
            field,
            "SNOWFLAKE_TYPE_INVALID",
            "Value is not snowflake.",
        ))
    })
}

/// A single-field 50035.
pub fn invalid(field: &str, code: &str, message: &str) -> HttpError {
    HttpError(ApiError::invalid_form().field(field, code, message))
}

/// The acting user. OAuth bearer tokens cannot administer servers; sessions and
/// bots can.
pub fn actor(p: &Principal) -> ApiResult<UserId> {
    match p.kind {
        AuthKind::Bearer { .. } => Err(ErrorCode::MissingAccess.into()),
        _ => Ok(UserId::new(u64::try_from(p.user_id).unwrap_or(0))),
    }
}

/// `X-Audit-Log-Reason` (percent-decoding is the client's job; Discord clients
/// send it URL-encoded and servers decode it, so decode `%XX` here).
pub fn audit_reason(headers: &axum::http::HeaderMap) -> Option<String> {
    let raw = headers.get("x-audit-log-reason")?.to_str().ok()?;
    Some(percent_decode(raw))
}

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(v) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_header_is_percent_decoded() {
        assert_eq!(percent_decode("spam%20link%21"), "spam link!");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%F0%9F%91%8D"), "\u{1F44D}");
    }
}

impl GuildAccess {
    /// Second-factor requirement of the server (D-223): when the caller holds `needs` here
    /// and the server's `mfa_level` is 1, a browser session must have proved a second factor
    /// in the last 15 minutes (`900004`, or `900123` without any), and a bot's application
    /// owner must have one. Callers without the permission pass through to the usual 403.
    pub async fn require_mfa(
        &self,
        gate: &crate::routes::guilds::safety::MfaGate,
        user: UserId,
        needs: Permissions,
    ) -> ApiResult<()> {
        if self.perms.has(needs) {
            gate.check_level(user, self.guild.mfa_level).await?;
        }
        Ok(())
    }
}
