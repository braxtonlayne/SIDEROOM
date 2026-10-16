//! Guild invites (LNG-SSET-038, open question 134): create, list, resolve,
//! revoke and accept.

use super::deps::{ChannelAccess, SettingsDeps, actor, id, re, se};
use super::events::SettingsEvent;
use crate::routes::guilds::dto::{UserRef, iso8601};
use crate::routes::guilds::safety::MfaGate;
use crate::routes::social::SocialDeps;
use crate::routes::social::group_invites;
use crate::routes::{Ext, MaybeExt};
use crate::{
    HttpError, Principal, auth::UserOnly, error::ApiResult, openapi::ErrorBody, state::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query},
    routing::get,
};
use serde::{Deserialize, Serialize};
use sr_chat::Channel;
use sr_chat::settings::invites::{GuildInvite, InviteOptions, InviteStatus};
use sr_chat::settings::repo::InviteRepository;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, GuildId};
use std::collections::{HashMap, hash_map::Entry};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/channels/{channel_id}/invites",
            get(list_channel_invites).post(create_invite),
        )
        .route("/guilds/{guild_id}/invites", get(list_guild_invites))
        .route(
            "/invites/{code}",
            get(get_invite).post(accept_invite).delete(delete_invite),
        )
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InviteGuild {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    /// Invite splash hash, same URL shape as the icon (LNG-INV-004).
    pub splash: Option<String>,
    /// Banner hash, same URL shape as the icon.
    pub banner: Option<String>,
    pub description: Option<String>,
    /// The server's vanity code, when it has one.
    pub vanity_url_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InviteChannel {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub kind: i16,
    /// Group DM icon hash (`/cdn/channel-icons/{id}/{hash}`); group invites only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Discord-shaped invite object.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InviteObject {
    /// 0 = guild invite, 1 = group DM invite (no `guild`).
    #[serde(rename = "type")]
    pub kind: u8,
    pub code: String,
    /// The server; `null` on group DM invites (D-221).
    #[schema(nullable = true)]
    pub guild: Option<InviteGuild>,
    pub channel: InviteChannel,
    pub inviter: UserRef,
    pub uses: u32,
    pub max_uses: u32,
    /// Seconds; 0 = never expires.
    pub max_age: u32,
    pub temporary: bool,
    pub created_at: String,
    pub expires_at: Option<String>,
    /// Members of the guild; only with `with_counts=true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    /// Members visibly online; only with `with_counts=true`, and only when the
    /// server has a live presence source (the gateway). Omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
}

impl InviteObject {
    pub fn build(
        inv: &GuildInvite,
        guild: &sr_chat::Guild,
        channel: &Channel,
        users: &HashMap<sr_core::UserId, sr_chat::repo::UserSummary>,
    ) -> Self {
        Self {
            kind: 0,
            code: inv.code.clone(),
            guild: Some(InviteGuild {
                id: guild.id.to_string(),
                name: guild.name.clone(),
                icon: guild.icon_key.clone(),
                splash: guild.splash_key.clone(),
                banner: guild.banner_key.clone(),
                description: guild.description.clone(),
                vanity_url_code: guild.vanity_code.clone(),
            }),
            channel: InviteChannel {
                id: channel.id.to_string(),
                name: channel.name.clone(),
                kind: channel.kind.code(),
                icon: None,
            },
            inviter: UserRef::of(inv.inviter_id, users),
            uses: inv.uses,
            max_uses: inv.max_uses,
            max_age: inv.max_age_secs,
            temporary: inv.temporary,
            created_at: iso8601(inv.created_at_ms),
            expires_at: inv.expires_at_ms().map(iso8601),
            approximate_member_count: None,
            approximate_presence_count: None,
        }
    }
}

/// Body of `POST /channels/{id}/invites`. Every field is optional.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct CreateInviteBody {
    /// Seconds until expiry, 0 to 604800 (default 86400).
    pub max_age: Option<u32>,
    /// 0 to 100 (default 0 = unlimited; 1 for `external`).
    pub max_uses: Option<u32>,
    pub temporary: Option<bool>,
    /// Accepted for Discord compatibility; every call creates a new invite.
    pub unique: Option<bool>,
    /// SIDE ROOM: the invite is for sharing outside the platform ("Copy link").
    /// It defaults to a single use (open question 134).
    pub external: Option<bool>,
}

async fn render_all(deps: &SettingsDeps, invites: &[GuildInvite]) -> ApiResult<Vec<InviteObject>> {
    let mut guilds: HashMap<GuildId, Option<sr_chat::Guild>> = HashMap::new();
    let mut channels: HashMap<ChannelId, Option<Channel>> = HashMap::new();
    for inv in invites {
        if let Entry::Vacant(slot) = guilds.entry(inv.guild_id) {
            slot.insert(deps.chat.get_guild(inv.guild_id).await.map_err(re)?);
        }
        if let Entry::Vacant(slot) = channels.entry(inv.channel_id) {
            slot.insert(deps.chat.get_channel(inv.channel_id).await.map_err(re)?);
        }
    }
    let users = deps.users(invites.iter().map(|i| i.inviter_id)).await?;
    Ok(invites
        .iter()
        .filter_map(|inv| {
            let g = guilds.get(&inv.guild_id)?.as_ref()?;
            let c = channels.get(&inv.channel_id)?.as_ref()?;
            Some(InviteObject::build(inv, g, c, &users))
        })
        .collect())
}

async fn render_one(deps: &SettingsDeps, inv: &GuildInvite) -> ApiResult<InviteObject> {
    render_all(deps, std::slice::from_ref(inv))
        .await?
        .pop()
        .ok_or_else(|| ErrorCode::UnknownInvite.into())
}

fn need(perms: Permissions, flag: Permissions) -> ApiResult<()> {
    if perms.has(flag) {
        Ok(())
    } else {
        Err(ErrorCode::MissingPermissions.into())
    }
}

/// Create an invite (`CREATE_INSTANT_INVITE`). With `external: true` the defaults
/// are single use; explicit `max_age`/`max_uses` always win.
#[utoipa::path(
    post,
    path = "/channels/{channel_id}/invites",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    request_body(content = CreateInviteBody, description = "Optional"),
    responses(
        (status = 200, body = InviteObject),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_invite(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(social): MaybeExt<SocialDeps>,
    Path(channel_id): Path<String>,
    body: Option<Json<CreateInviteBody>>,
) -> ApiResult<Json<InviteObject>> {
    let user = actor(&p)?;
    let channel_id: ChannelId = id("channel_id", &channel_id)?;
    let body = body.map(|Json(b)| b).unwrap_or_default();
    // A group DM: the owner makes a 24-hour, unlimited-use link (G-47).
    if let Some(social) = &social
        && let Some(out) = group_invites::create(social, user, channel_id, || {
            sr_chat::settings::invites::generate_code(|| deps.service.random_u64())
        })
        .await?
    {
        return Ok(Json(out));
    }
    let acc = deps.channel_access(user, channel_id).await?;
    // Ordinary invites to the Staff Server are refused; staff join by appointment.
    if acc.guild.guild.is_staff_guild {
        return Err(se(sr_chat::settings::SettingsError::StaffServerInvites));
    }
    let base = InviteOptions::for_request(body.external.unwrap_or(false));
    let opts = InviteOptions {
        max_age_secs: body.max_age.unwrap_or(base.max_age_secs),
        max_uses: body.max_uses.unwrap_or(base.max_uses),
        temporary: body.temporary.unwrap_or(base.temporary),
    };
    let inv = deps
        .service
        .create_invite(acc.perms, acc.guild.guild.id, channel_id, user, opts)
        .await
        .map_err(se)?;
    let users = deps.users([user]).await?;
    let out = InviteObject::build(&inv, &acc.guild.guild, &acc.channel, &users);
    deps.publish(SettingsEvent::InviteCreated {
        guild: inv.guild_id,
        channel: inv.channel_id,
        invite: serde_json::to_value(&out).unwrap_or_default(),
    })
    .await;
    Ok(Json(out))
}

/// Active invites of a channel (`MANAGE_CHANNELS`).
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/invites",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = Vec<InviteObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_channel_invites(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(social): MaybeExt<SocialDeps>,
    Path(channel_id): Path<String>,
) -> ApiResult<Json<Vec<InviteObject>>> {
    let user = actor(&p)?;
    let channel_id: ChannelId = id("channel_id", &channel_id)?;
    if let Some(social) = &social
        && let Some(out) = group_invites::list(social, user, channel_id).await?
    {
        return Ok(Json(out));
    }
    let acc: ChannelAccess = deps.channel_access(user, channel_id).await?;
    need(acc.perms, Permissions::MANAGE_CHANNELS)?;
    let now = deps.now_ms();
    let all = deps
        .service
        .repo()
        .list_guild_invites(acc.guild.guild.id)
        .await
        .map_err(re)?;
    let active: Vec<_> = all
        .into_iter()
        .filter(|i| i.channel_id == channel_id && i.status(now) == InviteStatus::Active)
        .collect();
    Ok(Json(render_all(&deps, &active).await?))
}

/// Active invites of a guild (`MANAGE_GUILD`), newest first.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/invites",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<InviteObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_guild_invites(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<InviteObject>>> {
    let user = actor(&p)?;
    let guild_id: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, guild_id).await?;
    need(acc.perms, Permissions::MANAGE_GUILD)?;
    let now = deps.now_ms();
    let all = deps
        .service
        .repo()
        .list_guild_invites(guild_id)
        .await
        .map_err(re)?;
    let active: Vec<_> = all
        .into_iter()
        .filter(|i| i.status(now) == InviteStatus::Active)
        .collect();
    Ok(Json(render_all(&deps, &active).await?))
}

/// Query of `GET /invites/{code}`.
#[derive(Debug, Default, Deserialize)]
pub struct InviteQuery {
    pub with_counts: Option<bool>,
}

/// Presence is counted over at most this many members; beyond that the member
/// count is still exact but the presence count is left out.
const PRESENCE_SCAN_LIMIT: u64 = 10_000;

async fn add_counts(deps: &SettingsDeps, guild: GuildId, out: &mut InviteObject) -> ApiResult<()> {
    let members = deps.chat.count_members(guild).await.map_err(re)?;
    out.approximate_member_count = Some(members);
    if let Some(presence) = deps.presence()
        && members <= PRESENCE_SCAN_LIMIT
    {
        let mut online = 0;
        let mut after = None;
        loop {
            let page = deps
                .chat
                .page_members(guild, after, 1000)
                .await
                .map_err(re)?;
            let ids: Vec<_> = page.iter().map(|m| m.user_id).collect();
            online += presence.count_online(&ids);
            match ids.last() {
                Some(last) if ids.len() == 1000 => after = Some(*last),
                _ => break,
            }
        }
        out.approximate_presence_count = Some(online);
    }
    Ok(())
}

/// The invite behind a server's vanity code (LNG-INV-004): the system channel, else the
/// first text or announcement channel, with the owner as inviter. Never expires.
async fn vanity_invite(deps: &SettingsDeps, code: &str) -> ApiResult<Option<GuildInvite>> {
    let Some(gid) = deps.chat.guild_by_vanity(code).await.map_err(re)? else {
        return Ok(None);
    };
    let Some(guild) = deps.chat.get_guild(gid).await.map_err(re)? else {
        return Ok(None);
    };
    let channel = match guild.system_channel_id {
        Some(c) => Some(c),
        None => {
            let mut channels = deps.chat.list_guild_channels(gid).await.map_err(re)?;
            channels.retain(|c| {
                matches!(
                    c.kind,
                    sr_chat::ChannelType::Text | sr_chat::ChannelType::Announcement
                )
            });
            channels.sort_by_key(|c| (c.position, c.id));
            channels.first().map(|c| c.id)
        }
    };
    let Some(channel) = channel else {
        return Ok(None);
    };
    let uses = deps.service.repo().vanity_uses(gid).await.map_err(re)?;
    Ok(Some(GuildInvite::vanity(
        guild.vanity_code.clone().unwrap_or_else(|| code.to_owned()),
        gid,
        channel,
        guild.owner_id,
        uses,
        deps.now_ms(),
    )))
}

/// A stored invite that is still usable (revoked, expired and used-up all read as
/// unknown or their own error).
async fn stored_invite(deps: &SettingsDeps, code: &str) -> ApiResult<GuildInvite> {
    if !sr_chat::settings::invites::is_valid_code(code) {
        return Err(ErrorCode::UnknownInvite.into());
    }
    let inv = deps
        .service
        .repo()
        .get_invite(code)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::UnknownInvite)?;
    inv.check_usable(deps.now_ms()).map_err(se)?;
    Ok(inv)
}

/// Like [`stored_invite`], and a server's vanity code resolves too when no invite has that
/// text.
async fn usable_invite(deps: &SettingsDeps, code: &str) -> ApiResult<GuildInvite> {
    if !sr_chat::settings::invites::is_valid_code(code) {
        return Err(ErrorCode::UnknownInvite.into());
    }
    let stored = deps.service.repo().get_invite(code).await.map_err(re)?;
    if stored.is_none()
        && let Some(v) = vanity_invite(deps, code).await?
    {
        return Ok(v);
    }
    stored_invite(deps, code).await
}

/// Resolve an invite code (any signed-in user). Revoked, expired and used-up
/// invites are all `Unknown Invite`. With `with_counts=true` the guild's
/// `approximate_member_count` and `approximate_presence_count` are included;
/// `expires_at` is always present (null for invites that never expire).
#[utoipa::path(
    get,
    path = "/invites/{code}",
    params(
        ("code" = String, Path, description = "Invite code"),
        ("with_counts" = Option<bool>, Query, description = "Include approximate member and presence counts"),
    ),
    responses((status = 200, body = InviteObject), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_invite(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(social): MaybeExt<SocialDeps>,
    Path(code): Path<String>,
    Query(q): Query<InviteQuery>,
) -> ApiResult<Json<InviteObject>> {
    actor(&p)?;
    if let Some(social) = &social
        && sr_chat::settings::invites::is_valid_code(&code)
        && deps
            .service
            .repo()
            .get_invite(&code)
            .await
            .map_err(re)?
            .is_none()
        && let Some(out) = group_invites::resolve(social, &code).await?
    {
        return Ok(Json(out));
    }
    let inv = usable_invite(&deps, &code).await?;
    let mut out = render_one(&deps, &inv).await?;
    if q.with_counts.unwrap_or(false) {
        add_counts(&deps, inv.guild_id, &mut out).await?;
    }
    Ok(Json(out))
}

/// Revoke an invite (`MANAGE_CHANNELS` on its channel or `MANAGE_GUILD`). Returns
/// the revoked invite.
#[utoipa::path(
    delete,
    path = "/invites/{code}",
    params(
        ("code" = String, Path, description = "Invite code"),
        ("X-Audit-Log-Reason" = Option<String>, Header, description = "Stored in the audit log"),
    ),
    responses((status = 200, body = InviteObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_invite(
    p: Principal,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(social): MaybeExt<SocialDeps>,
    headers: axum::http::HeaderMap,
    Path(code): Path<String>,
) -> ApiResult<Json<InviteObject>> {
    let user = actor(&p)?;
    if let Some(social) = &social
        && let Some(out) = group_invites::revoke(social, user, &code).await?
    {
        return Ok(Json(out));
    }
    let inv = stored_invite(&deps, &code).await?;
    let out = render_one(&deps, &inv).await?;
    let guild = deps.guild_access(user, inv.guild_id).await?;
    let mut perms = guild.perms;
    if !perms.has(Permissions::MANAGE_GUILD)
        && let Ok(ch) = deps.channel_access(user, inv.channel_id).await
        && ch.perms.has(Permissions::MANAGE_CHANNELS)
    {
        perms |= Permissions::MANAGE_GUILD;
    }
    if perms.has(Permissions::MANAGE_GUILD) {
        gate.check_level(user, guild.guild.mfa_level).await?;
    }
    let reason = super::deps::audit_reason(&headers);
    deps.service
        .revoke_invite(perms, user, inv.guild_id, &inv.code, reason.as_deref())
        .await
        .map_err(se)?;
    deps.publish(SettingsEvent::InviteDeleted {
        guild: inv.guild_id,
        channel: inv.channel_id,
        code: inv.code,
    })
    .await;
    Ok(Json(out))
}

/// Accept an invite and join the guild. The member is added and exactly one use
/// consumed atomically. Already being a member returns the invite without using
/// it up. Banned users get 403 `User banned`; paused invites 403. Bots cannot
/// join by invite.
#[utoipa::path(
    post,
    path = "/invites/{code}",
    params(("code" = String, Path, description = "Invite code")),
    responses(
        (status = 200, body = InviteObject),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn accept_invite(
    UserOnly(p): UserOnly,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(messages): MaybeExt<crate::routes::messages::MessagesDeps>,
    MaybeExt(social): MaybeExt<SocialDeps>,
    Path(code): Path<String>,
) -> ApiResult<Json<InviteObject>> {
    let user = actor(&p)?;
    if !sr_chat::settings::invites::is_valid_code(&code) {
        return Err(ErrorCode::UnknownInvite.into());
    }
    // A group DM link: add the caller to the group (G-47).
    if let Some(social) = &social
        && deps
            .service
            .repo()
            .get_invite(&code)
            .await
            .map_err(re)?
            .is_none()
        && let Some(out) = group_invites::accept(social, user, &code).await?
    {
        return Ok(Json(out));
    }
    let outcome = deps
        .joiner()
        .join_with_invite(&code, user, deps.now_ms())
        .await
        .map_err(HttpError)?;
    // The join pipeline (LNG-GLD-008): the member row, its `pending` screening state and the
    // `GUILD_MEMBER_ADD` are written by the joiner; the visible part is the notice in the
    // system channel.
    if let (sr_chat::settings::join::InviteJoin::Joined { member, .. }, Some(m)) =
        (&outcome, &messages)
    {
        crate::routes::messages::system::post_user_join(m, member.guild_id, member.user_id).await;
        super::alerts::after_join(&deps, m, member.guild_id).await;
    }
    Ok(Json(render_one(&deps, outcome.invite()).await?))
}
