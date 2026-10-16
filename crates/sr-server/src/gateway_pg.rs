//! Postgres-backed [`Authenticator`] and [`GuildDirectory`] for the gateway.

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use sr_api::routes::guilds::dto::{ChannelObject, GuildObject, MemberObject, iso8601};
use sr_api::routes::social::model::DmChannelObject;
use sr_auth::{repo::SessionRepository, session::SessionPolicy, session::SessionToken};
use sr_chat::domain::ChannelType;
use sr_chat::repo::UserLookup;
use sr_chat::{
    repo::{ChannelRepository, GuildRepository},
    social::{
        dm::RequestState,
        relationships::RelationshipKind,
        repo::{DmRepository, ReadStateRepository, RelationshipRepository},
    },
    threads::access::guild_channel_permissions,
    threads::repo::ThreadRepository,
};
use sr_core::{ChannelId, Clock, GuildId, Permissions, UserId};
use sr_db::{auth_repo::PgAuthRepo, chat_repo::PgChatRepo, social_repo::PgSocialRepo};
use sr_gateway::{
    AuthError, AuthedUser, Authenticator, GuildDirectory, LARGE_THRESHOLD_DEFAULT,
    VoiceChannelInfo, VoiceChannelKind, VoiceGuildInfo, VoiceLimits,
};
use std::sync::Arc;

type BuildResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Resolves a gateway token or `sr_session` cookie to a user. A token with dots in it is a bot
/// token (`base64url(id).base64url(issued).base64url(secret)`, open question 56): it is checked
/// against the application store, which also decides the privileged intents it may use.
pub struct PgAuthenticator {
    auth: PgAuthRepo,
    pool: PgPool,
    clock: Arc<dyn Clock>,
    apps: Option<Arc<sr_apps::AppsService>>,
}

impl PgAuthenticator {
    pub fn new(auth: PgAuthRepo, pool: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self {
            auth,
            pool,
            clock,
            apps: None,
        }
    }

    /// Accept bot tokens (without it every bot IDENTIFY is refused).
    pub fn with_apps(mut self, apps: Arc<sr_apps::AppsService>) -> Self {
        self.apps = Some(apps);
        self
    }

    async fn resolve_bot(&self, token: &str) -> Result<AuthedUser, AuthError> {
        let apps = self.apps.as_ref().ok_or(AuthError::Invalid)?;
        let bot = apps
            .gateway_bot(token)
            .await
            .map_err(unavailable)?
            .ok_or(AuthError::Invalid)?;
        let row = sqlx::query(
            "SELECT username::text AS username, display_name, avatar_key FROM accounts \n             WHERE id = $1 AND state = 'active' AND is_bot",
        )
        .bind(bot.user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(unavailable)?
        .ok_or(AuthError::Invalid)?;
        let user_id = UserId::new(bot.user_id as u64);
        Ok(AuthedUser {
            user_id,
            bot: true,
            allowed_privileged_intents: bot.allowed_intents,
            user: json!({
                "id": user_id.to_string(),
                "username": row.try_get::<String, _>("username").map_err(unavailable)?,
                "global_name": row.try_get::<Option<String>, _>("display_name").map_err(unavailable)?,
                "avatar": row.try_get::<Option<String>, _>("avatar_key").map_err(unavailable)?,
                "discriminator": "0",
                "bot": true,
            }),
        })
    }

    async fn resolve(&self, token: &SessionToken) -> Result<AuthedUser, AuthError> {
        let rec = self
            .auth
            .find_by_hash(&token.hash())
            .await
            .map_err(unavailable)?
            .ok_or(AuthError::Invalid)?;
        let now = self.clock.now_unix_ms().div_euclid(1000);
        if !SessionPolicy::is_valid(rec.expires_at, now) {
            return Err(AuthError::Invalid);
        }
        let row = sqlx::query(
            "SELECT username::text AS username, display_name FROM accounts \
             WHERE id = $1 AND state = 'active'",
        )
        .bind(rec.user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(unavailable)?
        .ok_or(AuthError::Invalid)?;
        let username: String = row.try_get("username").map_err(unavailable)?;
        let display: Option<String> = row.try_get("display_name").map_err(unavailable)?;
        let user_id = UserId::new(rec.user_id as u64);
        Ok(AuthedUser {
            user_id,
            bot: false,
            allowed_privileged_intents: 0,
            user: json!({
                "id": user_id.to_string(),
                "username": username,
                "global_name": display,
                "bot": false,
            }),
        })
    }
}

fn unavailable(e: impl std::fmt::Display) -> AuthError {
    AuthError::Unavailable(e.to_string())
}

#[async_trait]
impl Authenticator for PgAuthenticator {
    async fn authenticate(&self, token: &str) -> Result<AuthedUser, AuthError> {
        // Libraries send the bare token; tolerate the REST header form too.
        let token = token.strip_prefix("Bot ").unwrap_or(token);
        if token.contains('.') {
            return self.resolve_bot(token).await;
        }
        let token = SessionToken::parse(token).ok_or(AuthError::Invalid)?;
        self.resolve(&token).await
    }

    async fn authenticate_session_cookie(&self, cookie: &str) -> Result<AuthedUser, AuthError> {
        let token = SessionToken::parse(cookie).ok_or(AuthError::Invalid)?;
        self.resolve(&token).await
    }
}

/// Guild membership, channel visibility and READY/GUILD_CREATE bodies from Postgres.
/// Visibility is decided by `sr_chat::perms::channel_permissions` (owner/admin bypass,
/// role and member overwrites, `VIEW_CHANNEL`).
pub struct PgGuildDirectory {
    pool: PgPool,
    chat: PgChatRepo,
    social: PgSocialRepo,
    clock: Arc<dyn Clock>,
    apps: Option<Arc<sr_apps::AppsService>>,
    offline: Option<Arc<TemporaryMembers>>,
    entitlements: Option<Arc<dyn sr_payments::entitlements::Entitlements>>,
}

impl PgGuildDirectory {
    pub fn new(pool: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self {
            chat: PgChatRepo::new(pool.clone()),
            social: PgSocialRepo::new(pool.clone()),
            pool,
            clock,
            apps: None,
            offline: None,
            entitlements: None,
        }
    }

    /// Drop temporary memberships when a user goes offline (LNG-INV-005).
    pub fn with_temporary_members(mut self, hook: Arc<TemporaryMembers>) -> Self {
        self.offline = Some(hook);
        self
    }

    /// Server levels and Patron status for the voice allowance (D-050..D-052). Without it
    /// everyone gets the free tier.
    pub fn with_entitlements(
        mut self,
        entitlements: Arc<dyn sr_payments::entitlements::Entitlements>,
    ) -> Self {
        self.entitlements = Some(entitlements);
        self
    }

    /// Lets READY describe a bot's application (`application: {id, flags}`).
    pub fn with_apps(mut self, apps: Arc<sr_apps::AppsService>) -> Self {
        self.apps = Some(apps);
        self
    }

    /// GUILD_CREATE body: guild object, `channels`, `roles`, `threads`, and `members`
    /// when the guild is at most the default large threshold. The hub adds
    /// `presences` for guilds small enough to carry them.
    async fn build_guild_create(&self, user: UserId, guild: GuildId) -> BuildResult<Value> {
        let Some(g) = self.chat.get_guild(guild).await? else {
            return Ok(json!({ "id": guild, "unavailable": true }));
        };
        let roles = self.chat.list_roles(guild).await?;
        let mut channels = Vec::new();
        for c in self.chat.list_guild_channels(guild).await? {
            let overwrites = self.chat.get_overwrites(c.id).await?;
            channels.push(ChannelObject::new(&c, &overwrites));
        }
        let mut body = serde_json::to_value(GuildObject::new(
            &g,
            Some(roles.as_slice()),
            Some(channels.as_slice()),
        ))?;
        // One row past the threshold tells whether the member list fits.
        let member_ids: Vec<i64> = sqlx::query_scalar(
            "SELECT gm.user_id FROM guild_members gm WHERE gm.guild_id = $1 \
             AND NOT EXISTS (SELECT 1 FROM accounts sys WHERE sys.id = gm.user_id \
                             AND (sys.flags & 1) <> 0) \
             ORDER BY gm.user_id LIMIT $2",
        )
        .bind(guild.get() as i64)
        .bind(LARGE_THRESHOLD_DEFAULT as i64 + 1)
        .fetch_all(&self.pool)
        .await?;
        if let Some(obj) = body.as_object_mut() {
            obj.insert(
                "threads".into(),
                json!(self.visible_threads(user, guild).await?.0),
            );
            if member_ids.len() as u64 <= LARGE_THRESHOLD_DEFAULT {
                let mut found = Vec::with_capacity(member_ids.len());
                for id in member_ids {
                    let user = UserId::new(id as u64);
                    if let Some(m) = self.chat.get_member(guild, user).await? {
                        found.push(m);
                    }
                }
                // Name fields for the member objects (open question 157).
                let ids: Vec<UserId> = found.iter().map(|m| m.user_id).collect();
                let users: std::collections::HashMap<UserId, sr_chat::repo::UserSummary> = self
                    .chat
                    .users(&ids)
                    .await?
                    .into_iter()
                    .map(|u| (u.id, u))
                    .collect();
                let members: Vec<MemberObject> = found
                    .iter()
                    .map(|m| MemberObject::new(m, false, &users))
                    .collect();
                obj.insert("members".into(), json!(members));
            }
        }
        Ok(body)
    }

    /// Active threads `user` can see in `guild` as channel objects (with their own
    /// `member`), and their membership rows.
    async fn visible_threads(
        &self,
        user: UserId,
        guild: GuildId,
    ) -> BuildResult<(Vec<Value>, Vec<Value>)> {
        let (Some(g), Some(member)) = (
            self.chat.get_guild(guild).await?,
            self.chat.get_member(guild, user).await?,
        ) else {
            return Ok((Vec::new(), Vec::new()));
        };
        let roles = self.chat.list_roles(guild).await?;
        let now = self.clock.now_unix_ms();
        let mut seen = Vec::new();
        for t in self.chat.list_active_threads(guild).await? {
            let perms = guild_channel_permissions(&self.chat, &g, &member, &roles, &t, now).await?;
            if perms.contains(Permissions::VIEW_CHANNEL) {
                seen.push(t);
            }
        }
        let ids: Vec<ChannelId> = seen.iter().map(|t| t.id).collect();
        let memberships = self.chat.thread_memberships(user, &ids).await?;
        let threads = seen
            .iter()
            .map(|t| {
                let me = memberships.iter().find(|m| m.thread_id == t.id);
                json!(ChannelObject::new(t, &[]).with_member(me))
            })
            .collect();
        let members = memberships
            .iter()
            .map(|m| {
                json!({
                    "id": m.thread_id, "user_id": m.user_id, "flags": m.flags,
                    "join_timestamp": iso8601(m.joined_at_ms), "guild_id": guild,
                })
            })
            .collect();
        Ok((threads, members))
    }

    /// READY extras from the social repositories: DM `private_channels`,
    /// `relationships` (friends, blocks, pending requests) and `read_state`.
    async fn build_ready_extras(&self, user: UserId) -> BuildResult<Value> {
        // A bot's READY carries its application and none of a person's private state.
        if let Some(apps) = &self.apps
            && let Some(app) = apps.application_of_bot(user.get() as i64).await?
        {
            return Ok(json!({
                "application": { "id": app.id.to_string(), "flags": app.flags() },
            }));
        }
        // Full `recipients` user objects (everyone but the viewer), the same shape as
        // `GET /users/@me/channels` and the DM `CHANNEL_CREATE` (D-181).
        let dms = self.social.list_dms(user, false).await?;
        let ids: Vec<UserId> = dms
            .iter()
            .flat_map(|d| d.recipients.iter().copied())
            .collect();
        let users: std::collections::HashMap<UserId, sr_chat::repo::UserSummary> = self
            .chat
            .users(&ids)
            .await?
            .into_iter()
            .map(|u| (u.id, u))
            .collect();
        // A conversation sitting in the viewer's Message Requests carries
        // `is_message_request: true`, as in `GET /users/@me/channels`.
        let mut private_channels: Vec<Value> = Vec::with_capacity(dms.len());
        for d in &dms {
            let mut obj = DmChannelObject::new(d, user, &users);
            if self.social.get_request_state(user, d.id).await? == Some(RequestState::Pending) {
                obj.is_message_request = Some(true);
                obj.sr_request_reason = self
                    .social
                    .get_request_reason(user, d.id)
                    .await?
                    .map(|r| r.as_str().to_owned());
            }
            private_channels.push(json!(obj));
        }
        let relationships: Vec<Value> = self
            .social
            .list_relationships(user)
            .await?
            .iter()
            .map(|r| {
                let kind = match r.kind {
                    RelationshipKind::Friend => 1,
                    RelationshipKind::Blocked => 2,
                    RelationshipKind::PendingIncoming => 3,
                    RelationshipKind::PendingOutgoing => 4,
                };
                json!({
                    "id": r.target,
                    "type": kind,
                    "nickname": r.nickname,
                    "since": iso8601(r.since_ms),
                })
            })
            .collect();
        let entries: Vec<Value> = self
            .social
            .list_read_states(user)
            .await?
            .iter()
            .map(|s| {
                json!({
                    "id": s.channel,
                    "last_message_id": s.last_read,
                    "mention_count": s.mention_count,
                    "last_pin_timestamp": s.last_pin_ms.map(iso8601),
                })
            })
            .collect();
        Ok(json!({
            "private_channels": private_channels,
            "relationships": relationships,
            "read_state": { "entries": entries },
        }))
    }
}

#[async_trait]
impl GuildDirectory for PgGuildDirectory {
    async fn user_offline(&self, user: UserId) {
        if let Some(hook) = &self.offline {
            hook.schedule(user);
        }
    }

    async fn member_list(
        &self,
        guild: GuildId,
        channel: ChannelId,
    ) -> Option<sr_gateway::member_list::MemberListSource> {
        use sr_gateway::member_list::{ListMember, MemberListSource};
        /// Lists beyond this many members are not served lazily; the sidebar falls back to
        /// the paged `GET /guilds/{id}/members`.
        const MAX_LIST_MEMBERS: usize = 20_000;
        let built = async {
            let g = self.chat.get_guild(guild).await?;
            let Some(g) = g else { return Ok(None) };
            let roles = self.chat.list_roles(guild).await?;
            let overwrites = self.chat.get_overwrites(channel).await?;
            let now = self.clock.now_unix_ms();
            let mut all = Vec::new();
            let mut after = None;
            loop {
                let page = self.chat.page_members(guild, after, 1000).await?;
                let full = page.len() == 1000;
                after = page.last().map(|m| m.user_id);
                all.extend(page);
                if !full {
                    break;
                }
                if all.len() > MAX_LIST_MEMBERS {
                    return Ok::<_, sr_chat::RepoError>(None);
                }
            }
            let total = all.len();
            let visible: Vec<_> = all
                .into_iter()
                .filter(|m| {
                    sr_chat::perms::channel_permissions(&g, m, &roles, &overwrites, now)
                        .contains(Permissions::VIEW_CHANNEL)
                })
                .collect();
            let mut users = std::collections::HashMap::new();
            for chunk in visible.chunks(1000) {
                let ids: Vec<UserId> = chunk.iter().map(|m| m.user_id).collect();
                for u in self.chat.users(&ids).await? {
                    users.insert(u.id, u);
                }
            }
            let members = visible
                .iter()
                .map(|m| {
                    let hoist = roles
                        .iter()
                        .filter(|r| r.hoist && m.role_ids.contains(&r.id))
                        .max_by_key(|r| r.position)
                        .map(|r| (r.id.to_string(), r.position));
                    let user = users.get(&m.user_id);
                    let sort_key = m
                        .nick
                        .clone()
                        .or_else(|| user.and_then(|u| u.display_name.clone()))
                        .or_else(|| user.map(|u| u.username.clone()))
                        .unwrap_or_default();
                    ListMember {
                        user: m.user_id,
                        member: serde_json::to_value(MemberObject::new(m, false, &users))
                            .unwrap_or(Value::Null),
                        hoist,
                        sort_key,
                    }
                })
                .collect::<Vec<_>>();
            Ok(Some(MemberListSource {
                everyone: members.len() == total,
                members,
            }))
        }
        .await;
        match built {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "building a member list failed");
                None
            }
        }
    }

    async fn guilds_for_user(&self, user: UserId) -> Vec<GuildId> {
        let rows = sqlx::query_scalar::<_, i64>(
            "SELECT guild_id FROM guild_members WHERE user_id = $1 ORDER BY guild_id",
        )
        .bind(user.get() as i64)
        .fetch_all(&self.pool)
        .await;
        match rows {
            Ok(v) => v.into_iter().map(|g| GuildId::new(g as u64)).collect(),
            Err(e) => {
                tracing::error!(error = %e, "guilds_for_user failed");
                Vec::new()
            }
        }
    }

    async fn friends_of(&self, user: UserId) -> Vec<UserId> {
        match self.social.list_relationships(user).await {
            Ok(rels) => rels
                .into_iter()
                .filter(|r| r.kind == RelationshipKind::Friend)
                .map(|r| r.target)
                .collect(),
            Err(e) => {
                tracing::error!(error = %e, "friends_of failed");
                Vec::new()
            }
        }
    }

    async fn can_view_channel(&self, user: UserId, guild: GuildId, channel: ChannelId) -> bool {
        let res: Result<bool, sr_chat::error::RepoError> = async {
            let Some(ch) = self.chat.get_channel(channel).await? else {
                return Ok(false);
            };
            if ch.guild_id != Some(guild) {
                return Ok(false);
            }
            let (Some(g), Some(member)) = (
                self.chat.get_guild(guild).await?,
                self.chat.get_member(guild, user).await?,
            ) else {
                return Ok(false);
            };
            let roles = self.chat.list_roles(guild).await?;
            // Threads inherit the parent's overwrites; private ones need membership.
            let perms = guild_channel_permissions(
                &self.chat,
                &g,
                &member,
                &roles,
                &ch,
                self.clock.now_unix_ms(),
            )
            .await?;
            Ok(perms.contains(Permissions::VIEW_CHANNEL))
        }
        .await;
        res.unwrap_or_else(|e| {
            tracing::error!(error = %e, "can_view_channel failed");
            false
        })
    }

    async fn has_guild_permission(
        &self,
        user: UserId,
        guild: GuildId,
        needed: Permissions,
    ) -> bool {
        let res: Result<bool, sr_chat::error::RepoError> = async {
            let (Some(g), Some(member)) = (
                self.chat.get_guild(guild).await?,
                self.chat.get_member(guild, user).await?,
            ) else {
                return Ok(false);
            };
            let roles = self.chat.list_roles(guild).await?;
            Ok(sr_chat::perms::guild_permissions(&g, &member, &roles).contains(needed))
        }
        .await;
        res.unwrap_or_else(|e| {
            tracing::error!(error = %e, "has_guild_permission failed");
            false
        })
    }

    async fn channel_guild(&self, channel: ChannelId) -> Option<GuildId> {
        match self.chat.get_channel(channel).await {
            Ok(c) => c.and_then(|c| c.guild_id),
            Err(e) => {
                tracing::error!(error = %e, "channel_guild failed");
                None
            }
        }
    }

    async fn voice_guild(&self, guild: GuildId) -> VoiceGuildInfo {
        match self.chat.get_guild(guild).await {
            Ok(Some(g)) => VoiceGuildInfo {
                afk_channel: g.afk_channel_id,
                afk_timeout_secs: g.afk_timeout,
            },
            Ok(None) => VoiceGuildInfo::default(),
            Err(e) => {
                tracing::error!(error = %e, "voice_guild failed");
                VoiceGuildInfo::default()
            }
        }
    }

    async fn age_confirmed(&self, user: UserId) -> bool {
        match sqlx::query_scalar::<_, bool>(
            "SELECT age_confirmed_at IS NOT NULL FROM accounts WHERE id = $1",
        )
        .bind(user.get() as i64)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(v) => v.unwrap_or(false),
            Err(e) => {
                tracing::warn!(error = %e, "age lookup failed; treating the account as unconfirmed");
                false
            }
        }
    }

    async fn sensitive_view(
        &self,
        user: UserId,
    ) -> Option<(
        sr_chat::social::safety::SensitiveMedia,
        std::collections::HashSet<UserId>,
    )> {
        use sr_chat::social::repo::RelationshipRepository;
        // Bots get their messages as they are.
        let is_bot = match self.chat.users(&[user]).await {
            Ok(v) => v.first().is_some_and(|u| u.bot),
            Err(_) => true,
        };
        if is_bot {
            return None;
        }
        let prefs = self
            .social
            .get_privacy(user)
            .await
            .ok()?
            .safety
            .sensitive_media;
        let friends = self
            .social
            .list_relationships(user)
            .await
            .ok()?
            .into_iter()
            .filter(|r| r.kind == RelationshipKind::Friend)
            .map(|r| r.target)
            .collect();
        Some((prefs, friends))
    }

    async fn channel_age_restricted(&self, channel: ChannelId) -> bool {
        match sqlx::query_scalar::<_, bool>(
            "SELECT c.nsfw OR COALESCE(p.nsfw, FALSE) FROM channels c \n             LEFT JOIN channels p ON p.id = c.parent_id AND c.type IN (10, 11, 12) \n             WHERE c.id = $1",
        )
        .bind(channel.get() as i64)
        .fetch_optional(&self.pool)
        .await
        {
            Ok(v) => v.unwrap_or(false),
            Err(e) => {
                tracing::warn!(error = %e, "age-restriction lookup failed; withholding the dispatch");
                true
            }
        }
    }

    async fn voice_limits(&self, user: UserId, guild: Option<GuildId>) -> VoiceLimits {
        let Some(ents) = &self.entitlements else {
            return VoiceLimits::free();
        };
        match ents
            .limits(user.get() as i64, guild.map(|g| g.get() as i64))
            .await
        {
            Ok(l) => VoiceLimits {
                video_senders: l.video_senders,
                stream_viewers: l.stream_viewers,
                exempt: l.exempt_from_sender_limit,
                profiles: l.video_profiles.iter().map(|p| (p.height, p.fps)).collect(),
                tier: sr_voice::governor::Tier::of(l.guild_level, l.patron).0,
            },
            Err(e) => {
                tracing::error!(error = %e, "voice_limits failed; using the free tier");
                VoiceLimits::free()
            }
        }
    }

    async fn dm_call_members(&self, channel: ChannelId, user: UserId) -> Option<Vec<UserId>> {
        match self.social.get_dm(channel).await {
            Ok(Some(dm)) if dm.has(user) => Some(dm.recipients),
            Ok(_) => None,
            Err(e) => {
                tracing::error!(error = %e, "dm_call_members failed");
                None
            }
        }
    }

    async fn soundboard_sounds(&self, guild: GuildId) -> Vec<Value> {
        use sr_chat::settings::repo::ExpressionRepository;
        let repo = sr_db::settings_repo::PgSettingsRepo::new(self.pool.clone());
        match repo.list_sounds(guild).await {
            Ok(list) => list.iter().map(|s| s.to_json(None)).collect(),
            Err(e) => {
                tracing::error!(error = %e, "soundboard_sounds failed");
                Vec::new()
            }
        }
    }

    async fn voice_channel(&self, guild: GuildId, channel: ChannelId) -> Option<VoiceChannelInfo> {
        match self.chat.get_channel(channel).await {
            Ok(Some(c)) if c.guild_id == Some(guild) => Some(VoiceChannelInfo {
                kind: match c.kind {
                    ChannelType::Voice => VoiceChannelKind::Voice,
                    ChannelType::Stage => VoiceChannelKind::Stage,
                    _ => VoiceChannelKind::Other,
                },
                user_limit: c.user_limit.unwrap_or(0),
                bitrate: c.bitrate,
                age_restricted: c.nsfw,
            }),
            Ok(_) => None,
            Err(e) => {
                tracing::error!(error = %e, "voice_channel failed");
                None
            }
        }
    }

    async fn channel_permissions(
        &self,
        user: UserId,
        guild: GuildId,
        channel: ChannelId,
    ) -> Option<Permissions> {
        let res: Result<Option<Permissions>, sr_chat::error::RepoError> = async {
            let Some(ch) = self.chat.get_channel(channel).await? else {
                return Ok(None);
            };
            if ch.guild_id != Some(guild) {
                return Ok(None);
            }
            let (Some(g), Some(member)) = (
                self.chat.get_guild(guild).await?,
                self.chat.get_member(guild, user).await?,
            ) else {
                return Ok(None);
            };
            let roles = self.chat.list_roles(guild).await?;
            Ok(Some(
                guild_channel_permissions(
                    &self.chat,
                    &g,
                    &member,
                    &roles,
                    &ch,
                    self.clock.now_unix_ms(),
                )
                .await?,
            ))
        }
        .await;
        res.unwrap_or_else(|e| {
            tracing::error!(error = %e, "channel_permissions failed");
            None
        })
    }

    async fn member_payload(&self, guild: GuildId, user: UserId) -> Option<Value> {
        let res: Result<Option<Value>, sr_chat::error::RepoError> = async {
            let Some(m) = self.chat.get_member(guild, user).await? else {
                return Ok(None);
            };
            let users: std::collections::HashMap<UserId, sr_chat::repo::UserSummary> = self
                .chat
                .users(&[user])
                .await?
                .into_iter()
                .map(|u| (u.id, u))
                .collect();
            Ok(Some(json!(MemberObject::new(&m, false, &users))))
        }
        .await;
        res.unwrap_or_else(|e| {
            tracing::error!(error = %e, "member_payload failed");
            None
        })
    }

    async fn staff_users(&self) -> Vec<UserId> {
        let rows = sqlx::query_scalar::<_, i64>(
            "SELECT gm.user_id FROM guild_members gm \
             JOIN guilds g ON g.id = gm.guild_id WHERE g.is_staff_guild ORDER BY gm.user_id",
        )
        .fetch_all(&self.pool)
        .await;
        match rows {
            Ok(v) => v.into_iter().map(|u| UserId::new(u as u64)).collect(),
            Err(e) => {
                tracing::error!(error = %e, "staff_users failed");
                Vec::new()
            }
        }
    }

    async fn guild_payload(&self, _user: UserId, guild: GuildId) -> Value {
        self.build_guild_create(_user, guild)
            .await
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "guild_payload failed");
                json!({ "id": guild, "unavailable": true })
            })
    }

    async fn thread_list_sync(&self, user: UserId, guild: GuildId) -> Option<Value> {
        match self.visible_threads(user, guild).await {
            Ok((threads, members)) if !threads.is_empty() => Some(json!({
                "guild_id": guild, "threads": threads, "members": members,
            })),
            Ok(_) => None,
            Err(e) => {
                tracing::error!(error = %e, "thread_list_sync failed");
                None
            }
        }
    }

    async fn ready_extras(&self, user: UserId) -> Value {
        self.build_ready_extras(user).await.unwrap_or_else(|e| {
            tracing::error!(error = %e, "ready_extras failed");
            json!({})
        })
    }
}

/// Temporary membership (LNG-INV-005): a member who joined through a temporary invite and holds
/// no role is removed once they have been offline for `grace`. The grace period spares them a
/// removal when the connection merely blinks (a reload, a flaky network).
///
/// The gateway is built after the settings state, so it is attached late with
/// [`TemporaryMembers::attach`].
pub struct TemporaryMembers {
    joiner: Arc<sr_api::routes::settings::InviteMemberJoiner>,
    gateway: std::sync::OnceLock<sr_gateway::Gateway>,
    grace: std::time::Duration,
}

impl TemporaryMembers {
    pub fn new(
        joiner: Arc<sr_api::routes::settings::InviteMemberJoiner>,
        grace: std::time::Duration,
    ) -> Self {
        Self {
            joiner,
            gateway: std::sync::OnceLock::new(),
            grace,
        }
    }

    pub fn attach(&self, gateway: sr_gateway::Gateway) {
        let _ = self.gateway.set(gateway);
    }

    /// Remove the temporary memberships of accounts that have no live session. Run once after
    /// boot (D-200): a restart ends every session without a `user_offline` call, so members who
    /// were offline for good would otherwise keep their seat. Returns how many accounts lost a
    /// membership.
    pub async fn sweep(&self, pool: &PgPool) -> usize {
        let users: Vec<i64> =
            match sqlx::query_scalar("SELECT DISTINCT user_id FROM guild_members WHERE temporary")
                .fetch_all(pool)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(error = %e, "temporary-member sweep failed");
                    return 0;
                }
            };
        let mut swept = 0;
        for id in users {
            let user = UserId::new(id as u64);
            let live = self
                .gateway
                .get()
                .and_then(|gw| gw.presence(user))
                .is_some_and(|p| p.status != "offline");
            if !live && !self.joiner.drop_temporary(user).await.is_empty() {
                swept += 1;
            }
        }
        swept
    }

    /// Run [`Self::sweep`] once, after the offline grace has let reconnecting clients return.
    pub fn schedule_startup_sweep(self: &Arc<Self>, pool: PgPool) {
        let me = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(me.grace).await;
            let n = me.sweep(&pool).await;
            if n > 0 {
                tracing::info!(accounts = n, "removed stale temporary memberships");
            }
        });
    }

    /// `user` just went offline: after the grace period, drop their temporary memberships
    /// unless they came back.
    pub fn schedule(self: &Arc<Self>, user: UserId) {
        let me = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(me.grace).await;
            let back = me
                .gateway
                .get()
                .and_then(|gw| gw.presence(user))
                .is_some_and(|p| p.status != "offline");
            if !back {
                me.joiner.drop_temporary(user).await;
            }
        });
    }
}

/// Presence for invite previews, read from the gateway's in-memory presence map.
/// The gateway is built after the settings state, so it is attached late with
/// [`GatewayPresence::attach`]; until then every user counts as offline.
#[derive(Default)]
pub struct GatewayPresence {
    gateway: std::sync::OnceLock<sr_gateway::Gateway>,
}

impl GatewayPresence {
    pub fn attach(&self, gateway: sr_gateway::Gateway) {
        let _ = self.gateway.set(gateway);
    }
}

impl sr_api::routes::settings::PresenceCounter for GatewayPresence {
    fn count_online(&self, users: &[sr_core::UserId]) -> u64 {
        let Some(gw) = self.gateway.get() else {
            return 0;
        };
        users
            .iter()
            .filter(|u| {
                gw.presence(**u)
                    .is_some_and(|p| p.visible_status() != "offline")
            })
            .count() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_chat::social::repo::DmRepository;
    use sr_core::{SnowflakeGenerator, SystemClock};

    async fn account(pool: &PgPool, ids: &SnowflakeGenerator<Arc<dyn Clock>>) -> UserId {
        let person = ids.generate().get() as i64;
        let acct = ids.generate();
        sqlx::query("INSERT INTO persons (id) VALUES ($1)")
            .bind(person)
            .execute(pool)
            .await
            .unwrap();
        let name = format!("u{}", acct.get());
        sqlx::query(
            "INSERT INTO accounts (id, person_id, username, email, password_hash) \
             VALUES ($1, $2, $3, $4, 'x')",
        )
        .bind(acct.get() as i64)
        .bind(person)
        .bind(&name[..name.len().min(32)])
        .bind(format!("{name}@example.test"))
        .execute(pool)
        .await
        .unwrap();
        UserId::new(acct.get())
    }

    /// READY `private_channels` carry full `recipients` and flag only the conversation that sits
    /// in the viewer's Message Requests (D-181).
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn ready_private_channels_flag_message_requests() {
        let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
        let pool = sr_db::connect(&sr_db::DbConfig::new(url))
            .await
            .expect("connect");
        sr_db::migrate(&pool).await.expect("migrate");
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        let w = ((std::process::id() % 16) * 2) as u8 + 1;
        let ids = SnowflakeGenerator::new(clock.clone(), w, w).expect("ids");
        let (me, friend, stranger) = (
            account(&pool, &ids).await,
            account(&pool, &ids).await,
            account(&pool, &ids).await,
        );
        let repo = PgSocialRepo::new(pool.clone());
        let accepted = ChannelId::new(ids.generate().get());
        let request = ChannelId::new(ids.generate().get());
        repo.insert_dm(accepted, me, friend).await.unwrap();
        repo.insert_dm(request, stranger, me).await.unwrap();
        repo.set_request_state(me, request, RequestState::Pending)
            .await
            .unwrap();

        let dir = PgGuildDirectory::new(pool, clock);
        let extras = dir.build_ready_extras(me).await.unwrap();
        let list = extras["private_channels"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        for c in list {
            let rid = c["recipients"][0]["id"].as_str().unwrap();
            let is_request = c["id"] == request.to_string();
            assert_eq!(rid, if is_request { stranger } else { friend }.to_string());
            assert_eq!(
                c["is_message_request"].as_bool().unwrap_or(false),
                is_request
            );
            assert!(c.get("recipient_ids").is_none());
        }
    }

    /// The lazy member list lists the members who can see the channel, with their hoisted
    /// role and display name, and says whether it is the `everyone` list.
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn member_list_has_the_channels_viewers_with_hoisted_roles() {
        use sr_chat::{ChatLimits, ChatService, role_rules::NewRole};
        use sr_core::permissions::{Overwrite, OverwriteKind};
        let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
        let pool = sr_db::connect(&sr_db::DbConfig::new(url))
            .await
            .expect("connect");
        sr_db::migrate(&pool).await.expect("migrate");
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        // w and w + 16 must both stay within the 0..=31 worker range for every pid.
        let w = ((std::process::id() % 4) * 4) as u8 + 3;
        let ids = SnowflakeGenerator::new(clock.clone(), w, w).expect("ids");
        let svc = ChatService::new(
            PgChatRepo::new(pool.clone()),
            clock.clone(),
            SnowflakeGenerator::new(clock.clone(), w + 16, w + 16).expect("ids"),
            ChatLimits::default(),
        );
        let (owner, regular, outsider) = (
            account(&pool, &ids).await,
            account(&pool, &ids).await,
            account(&pool, &ids).await,
        );
        let bp = svc.create_guild(owner, "List Club").await.unwrap();
        let (guild, general) = (bp.guild.id, bp.channels[2].id);
        for u in [regular, outsider] {
            sqlx::query("INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2)")
                .bind(guild.get() as i64)
                .bind(u.get() as i64)
                .execute(&pool)
                .await
                .unwrap();
        }
        let mods = svc
            .create_role(
                owner,
                guild,
                NewRole {
                    name: Some("mods".into()),
                    hoist: true,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        sqlx::query("INSERT INTO member_roles (guild_id, user_id, role_id) VALUES ($1, $2, $3)")
            .bind(guild.get() as i64)
            .bind(regular.get() as i64)
            .bind(mods.id.get() as i64)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE guild_members SET nick = 'Zoe' WHERE guild_id = $1 AND user_id = $2")
            .bind(guild.get() as i64)
            .bind(regular.get() as i64)
            .execute(&pool)
            .await
            .unwrap();

        let dir = PgGuildDirectory::new(pool.clone(), clock.clone());
        let open = dir.member_list(guild, general).await.expect("a list");
        assert!(open.everyone);
        assert_eq!(open.members.len(), 3);
        let zoe = open.members.iter().find(|m| m.user == regular).unwrap();
        assert_eq!(zoe.sort_key, "Zoe");
        assert_eq!(zoe.hoist, Some((mods.id.to_string(), mods.position)));
        assert_eq!(zoe.member["nick"], "Zoe");
        assert_eq!(zoe.member["roles"][0], mods.id.to_string());
        assert!(
            open.members
                .iter()
                .find(|m| m.user == owner)
                .unwrap()
                .hoist
                .is_none()
        );

        // Only the role may see the channel: the outsider drops out and the list is no
        // longer the `everyone` list.
        let deny_everyone = Overwrite {
            id: guild.snowflake(),
            kind: OverwriteKind::Role,
            allow: Permissions::empty(),
            deny: Permissions::VIEW_CHANNEL,
        };
        let allow_mods = Overwrite {
            id: mods.id.snowflake(),
            kind: OverwriteKind::Role,
            allow: Permissions::VIEW_CHANNEL,
            deny: Permissions::empty(),
        };
        svc.set_overwrite(owner, general, deny_everyone)
            .await
            .unwrap();
        svc.set_overwrite(owner, general, allow_mods).await.unwrap();
        let private = dir.member_list(guild, general).await.expect("a list");
        assert!(!private.everyone);
        let mut users: Vec<_> = private.members.iter().map(|m| m.user).collect();
        users.sort();
        let mut want = vec![owner, regular];
        want.sort();
        assert_eq!(users, want);
        // Unknown guilds have no list.
        assert!(
            dir.member_list(GuildId::new(1), ChannelId::new(1))
                .await
                .is_none()
        );
    }

    /// Age-restricted channels (and threads of them), GUILD_CREATE without the system user,
    /// and the startup sweep of stale temporary memberships (D-188, D-199, D-200).
    #[tokio::test]
    #[ignore = "needs SR_TEST_DATABASE_URL"]
    async fn age_gate_system_member_and_temporary_sweep() {
        use sr_chat::{ChatLimits, ChatService};
        use sr_events::InProcessBus;
        let url = std::env::var("SR_TEST_DATABASE_URL").expect("SR_TEST_DATABASE_URL must be set");
        let pool = sr_db::connect(&sr_db::DbConfig::new(url))
            .await
            .expect("connect");
        sr_db::migrate(&pool).await.expect("migrate");
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        // w and w + 16 must both stay within the 0..=31 worker range for every pid.
        let w = ((std::process::id() % 4) * 4) as u8 + 1;
        let ids = SnowflakeGenerator::new(clock.clone(), w, w).expect("ids");
        let svc = ChatService::new(
            PgChatRepo::new(pool.clone()),
            clock.clone(),
            SnowflakeGenerator::new(clock.clone(), w + 16, w + 16).expect("ids"),
            ChatLimits::default(),
        );
        let (owner, system, guest) = (
            account(&pool, &ids).await,
            account(&pool, &ids).await,
            account(&pool, &ids).await,
        );
        sqlx::query("UPDATE accounts SET flags = flags | 1 WHERE id = $1")
            .bind(system.get() as i64)
            .execute(&pool)
            .await
            .unwrap();
        let bp = svc.create_guild(owner, "Gate Club").await.unwrap();
        let (guild, general) = (bp.guild.id, bp.channels[2].id);
        for (u, temporary) in [(system, false), (guest, true)] {
            sqlx::query(
                "INSERT INTO guild_members (guild_id, user_id, temporary) VALUES ($1, $2, $3)",
            )
            .bind(guild.get() as i64)
            .bind(u.get() as i64)
            .bind(temporary)
            .execute(&pool)
            .await
            .unwrap();
        }
        let dir = PgGuildDirectory::new(pool.clone(), clock.clone());

        // The age gate: a channel and a thread under it (types 10..12 inherit the parent).
        assert!(!dir.channel_age_restricted(general).await);
        sqlx::query("UPDATE channels SET nsfw = TRUE WHERE id = $1")
            .bind(general.get() as i64)
            .execute(&pool)
            .await
            .unwrap();
        assert!(dir.channel_age_restricted(general).await);
        let thread = ids.generate();
        sqlx::query(
            "INSERT INTO channels (id, guild_id, type, name, parent_id) VALUES ($1, $2, 11, 't', $3)",
        )
        .bind(thread.get() as i64)
        .bind(guild.get() as i64)
        .bind(general.get() as i64)
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            dir.channel_age_restricted(ChannelId::new(thread.get()))
                .await
        );

        // GUILD_CREATE members leave the system user out.
        let body = dir.guild_payload(owner, guild).await;
        let listed: Vec<&str> = body["members"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|m| m.pointer("/user/id")?.as_str())
            .collect();
        assert!(listed.contains(&owner.to_string().as_str()));
        assert!(!listed.contains(&system.to_string().as_str()));

        // The sweep removes the temporary member nobody is connected as.
        let bus = Arc::new(InProcessBus::new());
        let joiner = Arc::new(sr_api::routes::settings::InviteMemberJoiner::new(
            Arc::new(sr_db::invite_join::PgInviteJoinRepo::new(pool.clone())),
            Arc::new(PgChatRepo::new(pool.clone())),
            bus,
        ));
        let tm = TemporaryMembers::new(joiner, std::time::Duration::from_secs(60));
        assert_eq!(tm.sweep(&pool).await, 1);
        let left: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM guild_members WHERE guild_id = $1 AND user_id = $2",
        )
        .bind(guild.get() as i64)
        .bind(guest.get() as i64)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(left, 0);
    }
}
