//! Community, onboarding and welcome screen (LNG-SSET-063..067) plus the
//! guild-level safety flags (invite pause, raid alerts, age restriction).
//!
//! * `GET/PATCH /guilds/{id}/sr/community`: SIDE ROOM settings object.
//! * `GET/PUT /guilds/{id}/onboarding`: Discord shape.
//! * `GET/PATCH /guilds/{id}/welcome-screen`: Discord shape.

use super::deps::{GuildAccess, SettingsDeps, actor, id, invalid, re, se};
use super::events::SettingsEvent;
use crate::routes::Ext;
use crate::routes::guilds::dto::GuildObject;
use crate::routes::messages::error::ApiJson;
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, routing::get};
use serde::{Deserialize, Deserializer, Serialize};
use sr_chat::settings::community::{
    CommunitySettings, Onboarding, OnboardingOption, OnboardingPrompt, SafetyInputs,
    WelcomeChannel, WelcomeScreen,
};
use sr_core::permissions::{OverwriteKind, Permissions};
use sr_core::{ChannelId, ErrorCode, GuildId, RoleId};
use std::collections::HashSet;
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds/{guild_id}/sr/community",
            get(get_community).patch(modify_community),
        )
        .route(
            "/guilds/{guild_id}/onboarding",
            get(get_onboarding).put(put_onboarding),
        )
        .route(
            "/guilds/{guild_id}/welcome-screen",
            get(get_welcome_screen).patch(modify_welcome_screen),
        )
}

fn double_option<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

fn opt_id<T: std::str::FromStr>(field: &str, raw: &Option<String>) -> ApiResult<Option<T>> {
    raw.as_deref().map(|s| id(field, s)).transpose()
}

/// Every channel id must belong to the guild.
pub(super) async fn require_guild_channels(
    deps: &SettingsDeps,
    guild: GuildId,
    field: &str,
    channels: impl IntoIterator<Item = ChannelId>,
) -> ApiResult<()> {
    for ch in channels {
        let ok = deps
            .chat
            .get_channel(ch)
            .await
            .map_err(re)?
            .is_some_and(|c| c.guild_id == Some(guild));
        if !ok {
            return Err(invalid(field, "UNKNOWN_CHANNEL", "Unknown channel."));
        }
    }
    Ok(())
}

/// Tell the guild's gateway sessions that community, onboarding, welcome-screen
/// or safety settings changed, as a `GUILD_UPDATE` with the full guild object
/// (core fields plus the community fields clients read from the guild).
async fn publish_guild_update(deps: &SettingsDeps, acc: &GuildAccess) {
    let gid = acc.guild.id;
    let settings = match deps.service.community(acc.perms, gid).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "GUILD_UPDATE skipped: community settings unreadable");
            return;
        }
    };
    // The welcome screen and onboarding carry their own enabled flags, set by their
    // own endpoints, besides the switches in the community settings.
    let welcome_on = settings.welcome_screen_enabled
        || deps
            .service
            .welcome_screen(gid)
            .await
            .is_ok_and(|w| w.enabled);
    let onboarding_on = settings.onboarding_enabled
        || deps
            .service
            .onboarding(acc.perms, gid)
            .await
            .is_ok_and(|o| o.enabled);
    let mut payload = serde_json::to_value(GuildObject::new(&acc.guild, Some(&acc.roles), None))
        .unwrap_or_default();
    if let Some(o) = payload.as_object_mut() {
        let ch = |c: Option<ChannelId>| c.map(|c| c.to_string());
        let mut features = Vec::new();
        if settings.community_enabled {
            features.push("COMMUNITY");
        }
        if welcome_on {
            features.push("WELCOME_SCREEN_ENABLED");
        }
        if onboarding_on {
            features.push("GUILD_ONBOARDING");
        }
        o.insert("features".into(), serde_json::json!(features));
        o.insert(
            "rules_channel_id".into(),
            serde_json::json!(ch(settings.rules_channel_id)),
        );
        o.insert(
            "public_updates_channel_id".into(),
            serde_json::json!(ch(settings.updates_channel_id)),
        );
        o.insert(
            "safety_alerts_channel_id".into(),
            serde_json::json!(ch(settings.safety_alerts_channel_id)),
        );
        o.insert(
            "preferred_locale".into(),
            serde_json::json!(settings.preferred_locale),
        );
        if settings.description.is_some() {
            o.insert(
                "description".into(),
                serde_json::json!(settings.description),
            );
        }
    }
    deps.publish(SettingsEvent::CommunityChanged {
        guild: gid,
        payload,
    })
    .await;
}

// ---- community -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CommunityObject {
    pub guild_id: String,
    pub community_enabled: bool,
    pub rules_channel_id: Option<String>,
    pub public_updates_channel_id: Option<String>,
    pub safety_alerts_channel_id: Option<String>,
    pub description: Option<String>,
    pub preferred_locale: String,
    pub onboarding_enabled: bool,
    pub welcome_screen_enabled: bool,
    pub guide_enabled: bool,
    /// Security Actions: invites are paused for everyone (LNG-SSET-038).
    pub invites_paused: bool,
    /// Alert when more than this many members join per minute; 0 = off.
    pub raid_alert_joins_per_minute: u32,
    pub age_restricted: bool,
}

impl CommunityObject {
    pub fn of(s: &CommunitySettings) -> Self {
        let ch = |c: Option<ChannelId>| c.map(|c| c.to_string());
        Self {
            guild_id: s.guild_id.to_string(),
            community_enabled: s.community_enabled,
            rules_channel_id: ch(s.rules_channel_id),
            public_updates_channel_id: ch(s.updates_channel_id),
            safety_alerts_channel_id: ch(s.safety_alerts_channel_id),
            description: s.description.clone(),
            preferred_locale: s.preferred_locale.clone(),
            onboarding_enabled: s.onboarding_enabled,
            welcome_screen_enabled: s.welcome_screen_enabled,
            guide_enabled: s.guide_enabled,
            invites_paused: s.invites_paused,
            raid_alert_joins_per_minute: s.raid_alert_joins_per_minute,
            age_restricted: s.age_restricted,
        }
    }
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyCommunityBody {
    pub community_enabled: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub rules_channel_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub public_updates_channel_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub safety_alerts_channel_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    pub preferred_locale: Option<String>,
    pub onboarding_enabled: Option<bool>,
    pub welcome_screen_enabled: Option<bool>,
    pub guide_enabled: Option<bool>,
    pub invites_paused: Option<bool>,
    pub raid_alert_joins_per_minute: Option<u32>,
    pub age_restricted: Option<bool>,
}

/// SIDE ROOM community and safety settings (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/sr/community",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = CommunityObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_community(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<CommunityObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let s = deps.service.community(acc.perms, gid).await.map_err(se)?;
    Ok(Json(CommunityObject::of(&s)))
}

/// Edit community and safety settings (`MANAGE_GUILD`). Enabling community
/// needs a rules and an updates channel plus the verification and content-filter
/// levels from the safety checklist.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/sr/community",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = ModifyCommunityBody,
    responses((status = 200, body = CommunityObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_community(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<ModifyCommunityBody>,
) -> ApiResult<Json<CommunityObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let mut s = deps.service.community(acc.perms, gid).await.map_err(se)?;
    if let Some(v) = body.community_enabled {
        s.community_enabled = v;
    }
    if let Some(v) = &body.rules_channel_id {
        s.rules_channel_id = opt_id("rules_channel_id", v)?;
    }
    if let Some(v) = &body.public_updates_channel_id {
        s.updates_channel_id = opt_id("public_updates_channel_id", v)?;
    }
    if let Some(v) = &body.safety_alerts_channel_id {
        s.safety_alerts_channel_id = opt_id("safety_alerts_channel_id", v)?;
    }
    if let Some(v) = body.description {
        s.description = v.filter(|d| !d.is_empty());
    }
    if let Some(v) = body.preferred_locale {
        s.preferred_locale = v;
    }
    if let Some(v) = body.onboarding_enabled {
        s.onboarding_enabled = v;
    }
    if let Some(v) = body.welcome_screen_enabled {
        s.welcome_screen_enabled = v;
    }
    if let Some(v) = body.guide_enabled {
        s.guide_enabled = v;
    }
    if let Some(v) = body.invites_paused {
        s.invites_paused = v;
    }
    if let Some(v) = body.raid_alert_joins_per_minute {
        s.raid_alert_joins_per_minute = v;
    }
    if let Some(v) = body.age_restricted {
        s.age_restricted = v;
    }
    require_guild_channels(
        &deps,
        gid,
        "channels",
        [
            s.rules_channel_id,
            s.updates_channel_id,
            s.safety_alerts_channel_id,
        ]
        .into_iter()
        .flatten(),
    )
    .await?;
    let safety = SafetyInputs {
        verification_level: acc.guild.verification_level,
        explicit_content_filter: acc.guild.explicit_content_filter,
        has_rules_channel: s.rules_channel_id.is_some(),
        has_updates_channel: s.updates_channel_id.is_some(),
    };
    let saved = deps
        .service
        .save_community(acc.perms, user, s, safety)
        .await
        .map_err(se)?;
    publish_guild_update(&deps, &acc).await;
    Ok(Json(CommunityObject::of(&saved)))
}

// ---- onboarding ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OptionDto {
    /// Omit on create; the server assigns ids.
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub channel_ids: Vec<String>,
    #[serde(default)]
    pub role_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PromptDto {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub single_select: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default = "yes")]
    pub in_onboarding: bool,
    pub options: Vec<OptionDto>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OnboardingDto {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub default_channel_ids: Vec<String>,
    #[serde(default)]
    pub prompts: Vec<PromptDto>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OnboardingObject {
    pub guild_id: String,
    pub enabled: bool,
    pub default_channel_ids: Vec<String>,
    pub prompts: Vec<PromptDto>,
}

fn strings<T: ToString>(v: &[T]) -> Vec<String> {
    v.iter().map(ToString::to_string).collect()
}

impl OnboardingObject {
    pub fn of(o: &Onboarding) -> Self {
        Self {
            guild_id: o.guild_id.to_string(),
            enabled: o.enabled,
            default_channel_ids: strings(&o.default_channel_ids),
            prompts: o
                .prompts
                .iter()
                .map(|p| PromptDto {
                    id: Some(p.id.to_string()),
                    title: p.title.clone(),
                    single_select: p.single_select,
                    required: p.required,
                    in_onboarding: p.in_onboarding,
                    options: p
                        .options
                        .iter()
                        .map(|o| OptionDto {
                            id: Some(o.id.to_string()),
                            title: o.title.clone(),
                            description: o.description.clone(),
                            emoji: o.emoji.clone(),
                            channel_ids: strings(&o.channel_ids),
                            role_ids: strings(&o.role_ids),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

fn ids_of<T: std::str::FromStr>(field: &str, raw: &[String]) -> ApiResult<Vec<T>> {
    raw.iter().map(|s| id(field, s)).collect()
}

/// Everything `@everyone` may do in each channel: the channels it can send in.
async fn everyone_writable(
    deps: &SettingsDeps,
    acc: &GuildAccess,
) -> ApiResult<HashSet<ChannelId>> {
    let gid = acc.guild.id;
    let base = acc
        .roles
        .iter()
        .find(|r| r.id.get() == gid.get())
        .map_or_else(sr_chat::perms::default_everyone_permissions, |r| {
            r.permissions
        });
    let mut out = HashSet::new();
    for ch in deps.chat.list_guild_channels(gid).await.map_err(re)? {
        let ow = deps.chat.get_overwrites(ch.id).await.map_err(re)?;
        let mut perms = base;
        if let Some(o) = ow
            .iter()
            .find(|o| o.kind == OverwriteKind::Role && o.id.get() == gid.get())
        {
            perms = (perms & !o.deny) | o.allow;
        }
        if perms.contains(Permissions::VIEW_CHANNEL) && perms.contains(Permissions::SEND_MESSAGES) {
            out.insert(ch.id);
        }
    }
    Ok(out)
}

/// Onboarding settings (any member; the answer options are what new members see).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/onboarding",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = OnboardingObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_onboarding(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<OnboardingObject>> {
    use sr_chat::settings::repo::CommunityRepository;
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    deps.guild_access(user, gid).await?;
    let o = deps.service.repo().get_onboarding(gid).await.map_err(re)?;
    Ok(Json(OnboardingObject::of(&o)))
}

/// Replace the onboarding configuration (`MANAGE_GUILD`). Enabling it needs 7
/// default channels, 5 of them open to `@everyone`; at most 15 questions with 50
/// answers each. Ids you omit are assigned.
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/onboarding",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = OnboardingDto,
    responses((status = 200, body = OnboardingObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put_onboarding(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<OnboardingDto>,
) -> ApiResult<Json<OnboardingObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let mut referenced: Vec<ChannelId> = ids_of("default_channel_ids", &body.default_channel_ids)?;
    let mut prompts = Vec::with_capacity(body.prompts.len());
    for p in &body.prompts {
        let mut options = Vec::with_capacity(p.options.len());
        for o in &p.options {
            let channel_ids: Vec<ChannelId> = ids_of("channel_ids", &o.channel_ids)?;
            referenced.extend(channel_ids.iter().copied());
            let role_ids: Vec<RoleId> = ids_of("role_ids", &o.role_ids)?;
            if role_ids
                .iter()
                .any(|r| !acc.roles.iter().any(|x| x.id == *r))
            {
                return Err(invalid("role_ids", "UNKNOWN_ROLE", "Unknown role."));
            }
            options.push(OnboardingOption {
                id: match &o.id {
                    Some(s) => id("options", s)?,
                    None => deps.service.new_id(),
                },
                title: o.title.clone(),
                description: o.description.clone().filter(|d| !d.is_empty()),
                emoji: o.emoji.clone().filter(|d| !d.is_empty()),
                channel_ids,
                role_ids,
            });
        }
        prompts.push(OnboardingPrompt {
            id: match &p.id {
                Some(s) => id("prompts", s)?,
                None => deps.service.new_id(),
            },
            title: p.title.clone(),
            single_select: p.single_select,
            required: p.required,
            in_onboarding: p.in_onboarding,
            options,
        });
    }
    let default_channel_ids: Vec<ChannelId> =
        ids_of("default_channel_ids", &body.default_channel_ids)?;
    require_guild_channels(&deps, gid, "channel_ids", referenced).await?;
    let writable = everyone_writable(&deps, &acc).await?;
    let saved = deps
        .service
        .save_onboarding(
            acc.perms,
            user,
            Onboarding {
                guild_id: gid,
                enabled: body.enabled,
                default_channel_ids,
                prompts,
            },
            |c| writable.contains(&c),
        )
        .await
        .map_err(se)?;
    publish_guild_update(&deps, &acc).await;
    Ok(Json(OnboardingObject::of(&saved)))
}

// ---- welcome screen --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WelcomeChannelDto {
    pub channel_id: String,
    pub description: String,
    #[serde(default)]
    pub emoji_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WelcomeScreenObject {
    pub enabled: bool,
    pub description: Option<String>,
    pub welcome_channels: Vec<WelcomeChannelDto>,
}

impl WelcomeScreenObject {
    pub fn of(w: &WelcomeScreen) -> Self {
        Self {
            enabled: w.enabled,
            description: w.description.clone(),
            welcome_channels: w
                .channels
                .iter()
                .map(|c| WelcomeChannelDto {
                    channel_id: c.channel_id.to_string(),
                    description: c.description.clone(),
                    emoji_name: c.emoji.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ModifyWelcomeScreenBody {
    pub enabled: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub description: Option<Option<String>>,
    pub welcome_channels: Option<Vec<WelcomeChannelDto>>,
}

/// The welcome screen. Members see it while it is enabled; managers always.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/welcome-screen",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = WelcomeScreenObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_welcome_screen(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<WelcomeScreenObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let w = deps.service.welcome_screen(gid).await.map_err(se)?;
    if !w.enabled && !acc.perms.has(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok(Json(WelcomeScreenObject::of(&w)))
}

/// Edit the welcome screen (`MANAGE_GUILD`): up to 5 channel cards.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/welcome-screen",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = ModifyWelcomeScreenBody,
    responses((status = 200, body = WelcomeScreenObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_welcome_screen(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<ModifyWelcomeScreenBody>,
) -> ApiResult<Json<WelcomeScreenObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let mut w = deps.service.welcome_screen(gid).await.map_err(se)?;
    w.guild_id = gid;
    if let Some(v) = body.enabled {
        w.enabled = v;
    }
    if let Some(v) = body.description {
        w.description = v.filter(|d| !d.is_empty());
    }
    if let Some(chs) = body.welcome_channels {
        let mut out = Vec::with_capacity(chs.len());
        for c in chs {
            out.push(WelcomeChannel {
                channel_id: id("welcome_channels", &c.channel_id)?,
                description: c.description,
                emoji: c.emoji_name.filter(|e| !e.is_empty()),
            });
        }
        let ids: Vec<ChannelId> = out.iter().map(|c| c.channel_id).collect();
        require_guild_channels(&deps, gid, "welcome_channels", ids).await?;
        w.channels = out;
    }
    let saved = deps
        .service
        .save_welcome_screen(acc.perms, w)
        .await
        .map_err(se)?;
    publish_guild_update(&deps, &acc).await;
    Ok(Json(WelcomeScreenObject::of(&saved)))
}
