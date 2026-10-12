//! AutoMod rule CRUD (LNG-SSET-059/061). Discord-shaped rule objects; the engine
//! is `sr_chat::settings::automod`, evaluation hooks into message create through
//! [`super::automod_gate`].
//!
//! Trigger numbers follow Discord: 1 keyword, 3 spam, 4 keyword preset, 5 mention
//! spam, 6 member profile (a keyword rule on `MEMBER_UPDATE`). SIDE ROOM adds 100,
//! links (see open question 136), configured through `link_*` metadata fields.

use super::deps::{SettingsDeps, actor, id, invalid, re, se};
use super::events::SettingsEvent;
use crate::routes::Ext;
use crate::routes::messages::error::ApiJson;
use crate::{Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{Json, Router, extract::Path, http::StatusCode, routing::get};
use serde::{Deserialize, Serialize};
use sr_chat::settings::SettingsError;
use sr_chat::settings::automod::{Action, AutoModRule, EventType, KeywordPreset, Trigger};
use sr_chat::settings::repo::AutoModRepository;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, GuildId, RoleId, UserId};
use utoipa::ToSchema;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/guilds/{guild_id}/auto-moderation/rules",
            get(list_rules).post(create_rule),
        )
        .route(
            "/guilds/{guild_id}/auto-moderation/rules/{rule_id}",
            get(get_rule).patch(modify_rule).delete(delete_rule),
        )
}

pub const TRIGGER_KEYWORD: u8 = 1;
pub const TRIGGER_SPAM: u8 = 3;
pub const TRIGGER_KEYWORD_PRESET: u8 = 4;
pub const TRIGGER_MENTION_SPAM: u8 = 5;
pub const TRIGGER_MEMBER_PROFILE: u8 = 6;
/// SIDE ROOM extension: link filtering.
pub const TRIGGER_LINKS: u8 = 100;

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct TriggerMetadata {
    #[serde(default)]
    pub keyword_filter: Vec<String>,
    #[serde(default)]
    pub regex_patterns: Vec<String>,
    /// 1 profanity, 2 sexual content, 3 slurs.
    #[serde(default)]
    pub presets: Vec<u8>,
    #[serde(default)]
    pub allow_list: Vec<String>,
    pub mention_total_limit: Option<u32>,
    #[serde(default)]
    pub mention_raid_protection_enabled: bool,
    #[serde(default)]
    pub link_block_all: bool,
    #[serde(default)]
    pub link_block_domains: Vec<String>,
    #[serde(default)]
    pub link_allow_domains: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct ActionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionObject {
    /// 1 block message, 2 send alert message, 3 timeout.
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub metadata: ActionMetadata,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RuleObject {
    pub id: String,
    pub guild_id: String,
    pub name: String,
    pub creator_id: String,
    /// 1 message send, 2 member update.
    pub event_type: u8,
    pub trigger_type: u8,
    pub trigger_metadata: TriggerMetadata,
    pub actions: Vec<ActionObject>,
    pub enabled: bool,
    pub exempt_roles: Vec<String>,
    pub exempt_channels: Vec<String>,
}

/// Body of create (all of `name`, `event_type`, `trigger_type`, `actions` required)
/// and modify (every field optional; `trigger_type` cannot change).
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct RuleBody {
    pub name: Option<String>,
    pub event_type: Option<u8>,
    pub trigger_type: Option<u8>,
    pub trigger_metadata: Option<TriggerMetadata>,
    pub actions: Option<Vec<ActionObject>>,
    pub enabled: Option<bool>,
    pub exempt_roles: Option<Vec<String>>,
    pub exempt_channels: Option<Vec<String>>,
}

fn preset_code(p: KeywordPreset) -> u8 {
    match p {
        KeywordPreset::Profanity => 1,
        KeywordPreset::SexualContent => 2,
        KeywordPreset::Slurs => 3,
    }
}

fn preset_from(code: u8) -> Option<KeywordPreset> {
    Some(match code {
        1 => KeywordPreset::Profanity,
        2 => KeywordPreset::SexualContent,
        3 => KeywordPreset::Slurs,
        _ => return None,
    })
}

pub fn trigger_type_of(rule: &AutoModRule) -> u8 {
    match (&rule.trigger, rule.event_type) {
        (Trigger::Keyword { .. }, EventType::MemberUpdate) => TRIGGER_MEMBER_PROFILE,
        (Trigger::Keyword { .. }, _) => TRIGGER_KEYWORD,
        (Trigger::Spam, _) => TRIGGER_SPAM,
        (Trigger::KeywordPreset { .. }, _) => TRIGGER_KEYWORD_PRESET,
        (Trigger::MentionSpam { .. }, _) => TRIGGER_MENTION_SPAM,
        (Trigger::Links { .. }, _) => TRIGGER_LINKS,
    }
}

impl RuleObject {
    pub fn of(r: &AutoModRule) -> Self {
        let mut md = TriggerMetadata::default();
        match &r.trigger {
            Trigger::Keyword {
                keywords,
                regex_patterns,
                allow_list,
            } => {
                md.keyword_filter.clone_from(keywords);
                md.regex_patterns.clone_from(regex_patterns);
                md.allow_list.clone_from(allow_list);
            }
            Trigger::KeywordPreset {
                presets,
                allow_list,
            } => {
                md.presets = presets.iter().map(|p| preset_code(*p)).collect();
                md.allow_list.clone_from(allow_list);
            }
            Trigger::MentionSpam {
                total_limit,
                raid_protection,
            } => {
                md.mention_total_limit = Some(*total_limit);
                md.mention_raid_protection_enabled = *raid_protection;
            }
            Trigger::Links {
                block_all,
                block_domains,
                allow_domains,
            } => {
                md.link_block_all = *block_all;
                md.link_block_domains.clone_from(block_domains);
                md.link_allow_domains.clone_from(allow_domains);
            }
            Trigger::Spam => {}
        }
        Self {
            id: r.id.to_string(),
            guild_id: r.guild_id.to_string(),
            name: r.name.clone(),
            creator_id: r.creator_id.to_string(),
            event_type: match r.event_type {
                EventType::MessageSend => 1,
                EventType::MemberUpdate => 2,
            },
            trigger_type: trigger_type_of(r),
            trigger_metadata: md,
            actions: r.actions.iter().map(action_object).collect(),
            enabled: r.enabled,
            exempt_roles: r.exempt_role_ids.iter().map(ToString::to_string).collect(),
            exempt_channels: r
                .exempt_channel_ids
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }
}

fn action_object(a: &Action) -> ActionObject {
    match a {
        Action::Block { custom_message } => ActionObject {
            kind: 1,
            metadata: ActionMetadata {
                custom_message: custom_message.clone(),
                ..Default::default()
            },
        },
        Action::SendAlert { channel_id } => ActionObject {
            kind: 2,
            metadata: ActionMetadata {
                channel_id: Some(channel_id.to_string()),
                ..Default::default()
            },
        },
        Action::Timeout { duration_secs } => ActionObject {
            kind: 3,
            metadata: ActionMetadata {
                duration_seconds: Some(*duration_secs),
                ..Default::default()
            },
        },
    }
}

fn parse_action(a: &ActionObject) -> ApiResult<Action> {
    Ok(match a.kind {
        1 => Action::Block {
            custom_message: a.metadata.custom_message.clone().filter(|m| !m.is_empty()),
        },
        2 => Action::SendAlert {
            channel_id: id(
                "actions",
                a.metadata.channel_id.as_deref().ok_or_else(|| {
                    invalid("actions", "BASE_TYPE_REQUIRED", "Alert needs a channel.")
                })?,
            )?,
        },
        3 => Action::Timeout {
            duration_secs: a.metadata.duration_seconds.ok_or_else(|| {
                invalid("actions", "BASE_TYPE_REQUIRED", "Timeout needs a duration.")
            })?,
        },
        _ => {
            return Err(invalid(
                "actions",
                "ENUM_TYPE_BAD_VALUE",
                "Unknown action type.",
            ));
        }
    })
}

fn parse_ids<T: std::str::FromStr>(field: &str, raw: &[String]) -> ApiResult<Vec<T>> {
    raw.iter().map(|s| id(field, s)).collect()
}

fn build_trigger(kind: u8, md: &TriggerMetadata) -> ApiResult<(Trigger, Option<EventType>)> {
    Ok(match kind {
        TRIGGER_KEYWORD | TRIGGER_MEMBER_PROFILE => (
            Trigger::Keyword {
                keywords: md.keyword_filter.clone(),
                regex_patterns: md.regex_patterns.clone(),
                allow_list: md.allow_list.clone(),
            },
            (kind == TRIGGER_MEMBER_PROFILE).then_some(EventType::MemberUpdate),
        ),
        TRIGGER_SPAM => (Trigger::Spam, None),
        TRIGGER_KEYWORD_PRESET => {
            let presets = md
                .presets
                .iter()
                .map(|c| {
                    preset_from(*c).ok_or_else(|| {
                        invalid("trigger_metadata", "ENUM_TYPE_BAD_VALUE", "Unknown preset.")
                    })
                })
                .collect::<ApiResult<Vec<_>>>()?;
            (
                Trigger::KeywordPreset {
                    presets,
                    allow_list: md.allow_list.clone(),
                },
                None,
            )
        }
        TRIGGER_MENTION_SPAM => (
            Trigger::MentionSpam {
                total_limit: md.mention_total_limit.ok_or_else(|| {
                    invalid(
                        "trigger_metadata",
                        "BASE_TYPE_REQUIRED",
                        "mention_total_limit is required.",
                    )
                })?,
                raid_protection: md.mention_raid_protection_enabled,
            },
            None,
        ),
        TRIGGER_LINKS => (
            Trigger::Links {
                block_all: md.link_block_all,
                block_domains: md.link_block_domains.clone(),
                allow_domains: md.link_allow_domains.clone(),
            },
            None,
        ),
        _ => {
            return Err(invalid(
                "trigger_type",
                "ENUM_TYPE_BAD_VALUE",
                "Unknown trigger type.",
            ));
        }
    })
}

/// Merge `body` onto `existing` (modify) or build a new rule (create).
pub fn apply_body(
    guild: GuildId,
    existing: Option<&AutoModRule>,
    body: RuleBody,
) -> ApiResult<AutoModRule> {
    let missing = |f: &str| invalid(f, "BASE_TYPE_REQUIRED", "This field is required");
    let mut rule = match existing {
        Some(r) => r.clone(),
        None => {
            let trigger_type = body.trigger_type.ok_or_else(|| missing("trigger_type"))?;
            let event = match body.event_type.ok_or_else(|| missing("event_type"))? {
                1 => EventType::MessageSend,
                2 => EventType::MemberUpdate,
                _ => {
                    return Err(invalid(
                        "event_type",
                        "ENUM_TYPE_BAD_VALUE",
                        "Unknown event type.",
                    ));
                }
            };
            let md = body.trigger_metadata.clone().unwrap_or_default();
            let (trigger, forced_event) = build_trigger(trigger_type, &md)?;
            AutoModRule {
                id: 0,
                guild_id: guild,
                name: body.name.clone().ok_or_else(|| missing("name"))?,
                creator_id: UserId::default(),
                event_type: forced_event.unwrap_or(event),
                trigger,
                actions: Vec::new(),
                enabled: false,
                exempt_role_ids: Vec::new(),
                exempt_channel_ids: Vec::new(),
            }
        }
    };
    if existing.is_some() {
        if let Some(t) = body.trigger_type
            && existing.map(trigger_type_of) != Some(t)
        {
            return Err(invalid(
                "trigger_type",
                "AUTO_MODERATION_INVALID",
                "The trigger type cannot be changed.",
            ));
        }
        if let Some(md) = &body.trigger_metadata {
            let kind = existing.map(trigger_type_of).unwrap_or(TRIGGER_KEYWORD);
            rule.trigger = build_trigger(kind, md)?.0;
        }
        if let Some(n) = body.name {
            rule.name = n;
        }
    }
    match (body.actions, existing.is_some()) {
        (Some(actions), _) => {
            rule.actions = actions.iter().map(parse_action).collect::<ApiResult<_>>()?;
        }
        (None, false) => return Err(missing("actions")),
        (None, true) => {}
    }
    if let Some(e) = body.enabled {
        rule.enabled = e;
    }
    if let Some(r) = &body.exempt_roles {
        rule.exempt_role_ids = parse_ids::<RoleId>("exempt_roles", r)?;
    }
    if let Some(c) = &body.exempt_channels {
        rule.exempt_channel_ids = parse_ids::<ChannelId>("exempt_channels", c)?;
    }
    Ok(rule)
}

/// Alert channels and channel exemptions must belong to the guild.
async fn check_channels(deps: &SettingsDeps, guild: GuildId, rule: &AutoModRule) -> ApiResult<()> {
    let alert_channels = rule.actions.iter().filter_map(|a| match a {
        Action::SendAlert { channel_id } => Some(("actions", *channel_id)),
        _ => None,
    });
    let exempt = rule
        .exempt_channel_ids
        .iter()
        .map(|c| ("exempt_channels", *c));
    for (field, ch) in alert_channels.chain(exempt) {
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

fn need_manage(perms: Permissions) -> ApiResult<()> {
    if perms.has(Permissions::MANAGE_GUILD) {
        Ok(())
    } else {
        Err(ErrorCode::MissingPermissions.into())
    }
}

/// List the guild's AutoMod rules (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/auto-moderation/rules",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = Vec<RuleObject>), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_rules(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<Vec<RuleObject>>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    need_manage(deps.guild_access(user, gid).await?.perms)?;
    let rules = deps.service.repo().list_rules(gid).await.map_err(re)?;
    Ok(Json(rules.iter().map(RuleObject::of).collect()))
}

/// Get one rule (`MANAGE_GUILD`).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/auto-moderation/rules/{rule_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("rule_id" = String, Path, description = "Rule snowflake"),
    ),
    responses((status = 200, body = RuleObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_rule(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, rule_id)): Path<(String, String)>,
) -> ApiResult<Json<RuleObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let rid: u64 = id("rule_id", &rule_id)?;
    need_manage(deps.guild_access(user, gid).await?.perms)?;
    let rule = deps
        .service
        .repo()
        .get_rule(gid, rid)
        .await
        .map_err(re)?
        .ok_or_else(|| se(SettingsError::UnknownRule))?;
    Ok(Json(RuleObject::of(&rule)))
}

/// Create a rule (`MANAGE_GUILD`). Per-type quotas follow Discord: 6 keyword rules,
/// 3 link rules, 1 of every other type.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/auto-moderation/rules",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = RuleBody,
    responses((status = 201, body = RuleObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn create_rule(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<RuleBody>,
) -> ApiResult<(StatusCode, Json<RuleObject>)> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let acc = deps.guild_access(user, gid).await?;
    need_manage(acc.perms)?;
    let rule = apply_body(gid, None, body)?;
    check_channels(&deps, gid, &rule).await?;
    let saved = deps
        .service
        .put_automod_rule(acc.perms, user, rule)
        .await
        .map_err(se)?;
    let out = RuleObject::of(&saved);
    deps.publish(SettingsEvent::AutoModRuleCreated {
        guild: gid,
        rule: serde_json::to_value(&out).unwrap_or_default(),
    })
    .await;
    Ok((StatusCode::CREATED, Json(out)))
}

/// Modify a rule (`MANAGE_GUILD`).
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/auto-moderation/rules/{rule_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("rule_id" = String, Path, description = "Rule snowflake"),
    ),
    request_body = RuleBody,
    responses((status = 200, body = RuleObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn modify_rule(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, rule_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<RuleBody>,
) -> ApiResult<Json<RuleObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let rid: u64 = id("rule_id", &rule_id)?;
    let acc = deps.guild_access(user, gid).await?;
    need_manage(acc.perms)?;
    let existing = deps
        .service
        .repo()
        .get_rule(gid, rid)
        .await
        .map_err(re)?
        .ok_or_else(|| se(SettingsError::UnknownRule))?;
    let rule = apply_body(gid, Some(&existing), body)?;
    check_channels(&deps, gid, &rule).await?;
    let saved = deps
        .service
        .put_automod_rule(acc.perms, user, rule)
        .await
        .map_err(se)?;
    let out = RuleObject::of(&saved);
    deps.publish(SettingsEvent::AutoModRuleUpdated {
        guild: gid,
        rule: serde_json::to_value(&out).unwrap_or_default(),
    })
    .await;
    Ok(Json(out))
}

/// Delete a rule (`MANAGE_GUILD`).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/auto-moderation/rules/{rule_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("rule_id" = String, Path, description = "Rule snowflake"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete_rule(
    p: Principal,
    Ext(deps): Ext<SettingsDeps>,
    Path((guild_id, rule_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let rid: u64 = id("rule_id", &rule_id)?;
    let acc = deps.guild_access(user, gid).await?;
    need_manage(acc.perms)?;
    let existing = deps.service.repo().get_rule(gid, rid).await.map_err(re)?;
    deps.service
        .delete_automod_rule(acc.perms, user, gid, rid)
        .await
        .map_err(se)?;
    if let Some(rule) = existing {
        deps.publish(SettingsEvent::AutoModRuleDeleted {
            guild: gid,
            rule: serde_json::to_value(RuleObject::of(&rule)).unwrap_or_default(),
        })
        .await;
    }
    Ok(StatusCode::NO_CONTENT)
}
