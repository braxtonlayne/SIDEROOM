//! Alert messages with moderation buttons (D-243, D-244, J-02, J-03).
//!
//! * An AutoMod rule with a `send_alert_message` action makes the system user post a message of
//!   type 24 into the rule's alert channel: embed fields for the rule, trigger, channel, member,
//!   matched content and whether the message was blocked, and the buttons Delete message,
//!   Timeout (a select with Discord's six durations), Kick, Ban and Mark resolved.
//! * The join-rate alert posts the same kind of message into the server's safety alerts channel,
//!   with a `Resume invites` button when it paused invites by itself.
//!
//! The buttons carry only ids (`sr:automod:<verb>:<alert id>`, `sr:incident:resume-invites:<guild
//! id>`). A press goes to `POST /guilds/{id}/alert-actions`, which checks the presser's own
//! permission and second factor at press time, runs the ordinary service, writes the ordinary
//! audit row naming the presser and rewrites the card without the used button. The gateway event
//! `AUTO_MODERATION_ACTION_EXECUTION` is unchanged and still goes to bots.

use super::automod_gate::Alert;
use super::deps::{SettingsDeps, id, invalid, re, se};
use crate::routes::Ext;
use crate::routes::guilds::safety::MfaGate;
use crate::routes::guilds::util::{Api, ce};
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, MsgError};
use crate::routes::messages::system::{post_system_card, rewrite_system_card};
use crate::{HttpError, error::ApiResult, openapi::ErrorBody};
use axum::{Json, extract::Path};
use serde::{Deserialize, Serialize};
use sr_chat::MessageType;
use sr_chat::message::{Component, Embed, EmbedField, SelectOption};
use sr_chat::settings::audit::AuditAction;
use sr_chat::settings::automod::AutoModAlert;
use sr_chat::settings::repo::{AutoModRepository, CommunityRepository};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, GuildId, MessageId, UserId};
use sr_events::DomainEvent;
use utoipa::ToSchema;

pub fn routes() -> axum::Router<crate::state::AppState> {
    axum::Router::new().route(
        "/guilds/{guild_id}/alert-actions",
        axum::routing::post(alert_action),
    )
}

/// Discord's six timeout durations in seconds (the Timeout select of an AutoMod alert).
pub const TIMEOUT_CHOICES: [u32; 6] = [60, 300, 600, 3600, 86_400, 604_800];
/// The longest `matched_content` an alert shows.
pub const MATCHED_MAX: usize = 1000;

const RED: u32 = 0xE74C3C;
const AMBER: u32 = 0xF1C40F;
const SECONDARY: u8 = 2;
const DANGER: u8 = 4;
const BUTTON: u8 = 2;
const STRING_SELECT: u8 = 3;
const ROW: u8 = 1;
const HANDLED: &str = "Handled";

/// What a button on an AutoMod alert asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoModPress {
    Delete,
    Timeout,
    Kick,
    Ban,
    Resolve,
}

impl AutoModPress {
    fn verb(self) -> &'static str {
        match self {
            Self::Delete => "delete",
            Self::Timeout => "timeout",
            Self::Kick => "kick",
            Self::Ban => "ban",
            Self::Resolve => "resolve",
        }
    }

    fn parse(verb: &str) -> Option<Self> {
        Some(match verb {
            "delete" => Self::Delete,
            "timeout" => Self::Timeout,
            "kick" => Self::Kick,
            "ban" => Self::Ban,
            "resolve" => Self::Resolve,
            _ => return None,
        })
    }

    /// The permission the presser needs, checked at press time.
    fn needs(self) -> Permissions {
        match self {
            Self::Delete => Permissions::MANAGE_MESSAGES,
            Self::Timeout => Permissions::MODERATE_MEMBERS,
            Self::Kick => Permissions::KICK_MEMBERS,
            Self::Ban => Permissions::BAN_MEMBERS,
            Self::Resolve => {
                Permissions::MANAGE_MESSAGES
                    | Permissions::MODERATE_MEMBERS
                    | Permissions::KICK_MEMBERS
                    | Permissions::BAN_MEMBERS
            }
        }
    }
}

/// A parsed alert button id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertButton {
    AutoMod(AutoModPress, u64),
    ResumeInvites(u64),
}

/// `sr:automod:<verb>:<alert id>` and `sr:incident:resume-invites:<guild id>`.
pub fn parse_alert_button(custom_id: &str) -> Option<AlertButton> {
    let rest = custom_id.strip_prefix("sr:")?;
    let (kind, rest) = rest.split_once(':')?;
    let (verb, raw) = rest.split_once(':')?;
    let n: u64 = raw.parse().ok().filter(|v| *v > 0)?;
    match (kind, verb) {
        ("automod", v) => Some(AlertButton::AutoMod(AutoModPress::parse(v)?, n)),
        ("incident", "resume-invites") => Some(AlertButton::ResumeInvites(n)),
        _ => None,
    }
}

/// "1 min", "5 min", "10 min", "1 h", "1 day", "1 week" for the six durations.
pub fn duration_label(secs: u32) -> String {
    match secs {
        60 => "1 min".into(),
        300 => "5 min".into(),
        600 => "10 min".into(),
        3600 => "1 h".into(),
        86_400 => "1 day".into(),
        604_800 => "1 week".into(),
        other => format!("{other} s"),
    }
}

fn long_duration_label(secs: u32) -> &'static str {
    match secs {
        60 => "1 minute",
        300 => "5 minutes",
        600 => "10 minutes",
        3600 => "1 hour",
        86_400 => "1 day",
        604_800 => "1 week",
        _ => "Timeout",
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}

fn field(name: &str, value: String, inline: bool) -> EmbedField {
    EmbedField {
        name: name.to_owned(),
        value,
        inline,
    }
}

fn button(style: u8, label: &str, custom_id: String) -> Component {
    Component {
        kind: BUTTON,
        style: Some(style),
        label: Some(label.to_owned()),
        custom_id: Some(custom_id),
        ..Component::default()
    }
}

fn row(components: Vec<Component>) -> Component {
    Component {
        kind: ROW,
        components,
        ..Component::default()
    }
}

/// The card for one AutoMod alert. `flagged` is whether the flagged message was stored (a
/// blocked message has nothing to delete).
pub fn automod_card(
    alert_id: u64,
    a: &Alert,
    blocked: bool,
    flagged: bool,
) -> (Vec<Embed>, Vec<Component>) {
    let embed = Embed {
        title: Some(clip(
            &format!("AutoMod flagged a message: {}", a.rule_name),
            256,
        )),
        kind: Some("rich".into()),
        color: AMBER,
        fields: vec![
            field("Rule", clip(&a.rule_name, 1024), true),
            field("Trigger", a.trigger.label().to_owned(), true),
            field("Channel", format!("<#{}>", a.source_channel_id.get()), true),
            field("Member", format!("<@{}>", a.user_id.get()), true),
            field("Matched content", clip(&a.content, MATCHED_MAX), false),
            field(
                "Blocked",
                if blocked { "Yes" } else { "No" }.to_owned(),
                true,
            ),
        ],
        ..Embed::default()
    };
    let id = |verb: &str| format!("sr:automod:{verb}:{alert_id}");
    let mut buttons = Vec::new();
    if flagged {
        buttons.push(button(DANGER, "Delete message", id("delete")));
    }
    buttons.push(button(DANGER, "Kick", id("kick")));
    buttons.push(button(DANGER, "Ban", id("ban")));
    buttons.push(button(SECONDARY, "Mark resolved", id("resolve")));
    let select = Component {
        kind: STRING_SELECT,
        custom_id: Some(id("timeout")),
        placeholder: Some("Timeout".into()),
        options: TIMEOUT_CHOICES
            .iter()
            .map(|s| SelectOption {
                label: long_duration_label(*s).to_owned(),
                value: s.to_string(),
                description: None,
            })
            .collect(),
        ..Component::default()
    };
    (vec![embed], vec![row(buttons), row(vec![select])])
}

/// The card of the join-rate alert. `resume` carries the button when invites were paused.
pub fn incident_card(
    guild: GuildId,
    joins: u64,
    threshold: u32,
    paused: bool,
) -> (Vec<Embed>, Vec<Component>) {
    let embed = Embed {
        title: Some("Possible raid: many members joined at once".into()),
        kind: Some("rich".into()),
        description: Some(if paused {
            "Invites were paused for 24 hours.".into()
        } else {
            "Invites stay open (automatic pause is off).".into()
        }),
        color: RED,
        fields: vec![
            field("Joins in the last minute", joins.to_string(), true),
            field("Alert threshold", threshold.to_string(), true),
        ],
        ..Embed::default()
    };
    let components = if paused {
        vec![row(vec![button(
            SECONDARY,
            "Resume invites",
            format!("sr:incident:resume-invites:{}", guild.get()),
        )])]
    } else {
        Vec::new()
    };
    (vec![embed], components)
}

fn drop_button(components: &mut Vec<Component>, custom_ids: &[String]) {
    for c in components.iter_mut() {
        c.components.retain(|b| {
            !b.custom_id
                .as_ref()
                .is_some_and(|id| custom_ids.contains(id))
        });
    }
    components.retain(|c| c.kind != ROW || !c.components.is_empty());
}

fn note(embeds: &mut [Embed], text: String) {
    let Some(e) = embeds.first_mut() else { return };
    match e.fields.iter_mut().find(|f| f.name == HANDLED) {
        Some(f) => {
            f.value = clip(&format!("{}\n{text}", f.value), 1024);
        }
        None => e.fields.push(field(HANDLED, clip(&text, 1024), false)),
    }
    e.color = 0x2ECC71;
}

/// The card after `who` used a button: the used button (and the ones it makes pointless) are
/// gone and a "Handled" line names the presser.
pub fn apply_automod_press(
    mut embeds: Vec<Embed>,
    mut components: Vec<Component>,
    alert_id: u64,
    press: AutoModPress,
    who: UserId,
    timeout_secs: Option<u32>,
) -> (Vec<Embed>, Vec<Component>) {
    let id = |p: AutoModPress| format!("sr:automod:{}:{alert_id}", p.verb());
    let who = format!("<@{}>", who.get());
    let (text, drop): (String, Vec<AutoModPress>) = match press {
        AutoModPress::Delete => (format!("Deleted by {who}"), vec![AutoModPress::Delete]),
        AutoModPress::Timeout => (
            format!(
                "Timed out by {who} for {}",
                duration_label(timeout_secs.unwrap_or(0))
            ),
            vec![AutoModPress::Timeout],
        ),
        AutoModPress::Kick => (
            format!("Kicked by {who}"),
            vec![AutoModPress::Kick, AutoModPress::Timeout],
        ),
        AutoModPress::Ban => (
            format!("Banned by {who}"),
            vec![AutoModPress::Ban, AutoModPress::Kick, AutoModPress::Timeout],
        ),
        AutoModPress::Resolve => (
            format!("Resolved by {who}"),
            vec![
                AutoModPress::Delete,
                AutoModPress::Timeout,
                AutoModPress::Kick,
                AutoModPress::Ban,
                AutoModPress::Resolve,
            ],
        ),
    };
    let ids: Vec<String> = drop.into_iter().map(id).collect();
    drop_button(&mut components, &ids);
    note(&mut embeds, text);
    (embeds, components)
}

/// The join-rate card after `Resume invites` was pressed.
pub fn apply_resume(
    mut embeds: Vec<Embed>,
    mut components: Vec<Component>,
    guild: GuildId,
    who: UserId,
) -> (Vec<Embed>, Vec<Component>) {
    drop_button(
        &mut components,
        &[format!("sr:incident:resume-invites:{}", guild.get())],
    );
    note(&mut embeds, format!("Invites resumed by <@{}>", who.get()));
    (embeds, components)
}

fn msg_err(e: MsgError) -> HttpError {
    match e {
        MsgError::Api(a) | MsgError::RetryAfter { api: a, .. } => HttpError(a),
    }
}

fn has_custom_id(components: &[Component], wanted: &str) -> bool {
    components
        .iter()
        .any(|c| c.custom_id.as_deref() == Some(wanted) || has_custom_id(&c.components, wanted))
}

/// Post the alert message for one `send_alert_message` action and remember it. Failures are
/// logged: the message the alert is about is already decided.
pub async fn post_automod_alert(
    settings: &SettingsDeps,
    messages: &MessagesDeps,
    guild: GuildId,
    alert: &Alert,
    blocked: bool,
    flagged: Option<MessageId>,
) {
    if let Err(e) = try_post_automod_alert(settings, messages, guild, alert, blocked, flagged).await
    {
        tracing::warn!(error = ?e, rule = alert.rule_id, "AutoMod alert message failed");
    }
}

async fn try_post_automod_alert(
    settings: &SettingsDeps,
    messages: &MessagesDeps,
    guild: GuildId,
    alert: &Alert,
    blocked: bool,
    flagged: Option<MessageId>,
) -> Result<(), MsgError> {
    // The alert channel must still belong to this server.
    let in_guild = messages
        .repo
        .get_channel(alert.channel_id)
        .await?
        .is_some_and(|c| c.guild_id == Some(guild));
    if !in_guild {
        return Ok(());
    }
    // The buttons carry the id of the message, which is only known once it is stored, so the
    // card is posted bare first and rewritten with its buttons.
    let (embeds, _) = automod_card(0, alert, blocked, flagged.is_some());
    let msg = post_system_card(messages, alert.channel_id, embeds, Vec::new()).await?;
    let (embeds, components) = automod_card(msg.id.get(), alert, blocked, flagged.is_some());
    let row = AutoModAlert {
        id: msg.id.get(),
        guild_id: guild,
        rule_id: alert.rule_id,
        rule_name: alert.rule_name.clone(),
        channel_id: alert.channel_id,
        target_user_id: alert.user_id,
        flagged_channel_id: alert.source_channel_id,
        flagged_message_id: flagged.map(MessageId::get),
        resolved_by: None,
    };
    settings
        .service
        .repo()
        .insert_alert(&row)
        .await
        .map_err(MsgError::from)?;
    rewrite_system_card(messages, alert.channel_id, msg.id, &embeds, &components).await?;
    Ok(())
}

/// After a member joined: evaluate the join-rate alert (D-243). When it fires the server records
/// `raid_detected_at`, pauses invites for 24 hours unless `raid_auto_pause_invites` is off, an
/// audit row names the system user, and the safety alerts channel gets the alert message.
pub async fn after_join(settings: &SettingsDeps, messages: &MessagesDeps, guild: GuildId) {
    if let Err(e) = try_after_join(settings, messages, guild).await {
        tracing::warn!(guild = guild.get(), error = ?e, "join-rate alert failed");
    }
}

async fn try_after_join(
    settings: &SettingsDeps,
    messages: &MessagesDeps,
    guild: GuildId,
) -> Result<(), HttpError> {
    let community = settings
        .service
        .repo()
        .get_community(guild)
        .await
        .map_err(re)?;
    if community.raid_alert_joins_per_minute == 0 {
        return Ok(());
    }
    let since = settings.now_ms() - 60_000;
    let joins = settings
        .chat
        .count_recent_joins(guild, since)
        .await
        .map_err(re)?;
    let Some(fired) = settings
        .service
        .register_join_rate(guild, joins)
        .await
        .map_err(se)?
    else {
        return Ok(());
    };
    if let Some(channel) = community.safety_alerts_channel_id {
        let (embeds, _) = incident_card(
            guild,
            joins,
            community.raid_alert_joins_per_minute,
            fired.paused_until_ms.is_some(),
        );
        let msg = post_system_card(messages, channel, embeds, Vec::new())
            .await
            .map_err(msg_err)?;
        let (embeds, components) = incident_card(
            guild,
            joins,
            community.raid_alert_joins_per_minute,
            fired.paused_until_ms.is_some(),
        );
        rewrite_system_card(messages, channel, msg.id, &embeds, &components)
            .await
            .map_err(msg_err)?;
    }
    // Clients learn about the pause through the guild object.
    if let Some(g) = settings.chat.get_guild(guild).await.map_err(re)? {
        super::community::publish_guild_update_for(settings, &g).await;
    }
    Ok(())
}

// ---- press ----------------------------------------------------------------------------

#[derive(Debug, Deserialize, ToSchema)]
pub struct AlertActionBody {
    pub channel_id: String,
    pub message_id: String,
    /// The `custom_id` of the pressed button or select (`sr:automod:...`, `sr:incident:...`).
    pub custom_id: String,
    /// The chosen value of the Timeout select (seconds).
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AlertActionResult {
    /// False when the press changed nothing ("already handled").
    pub ok: bool,
    pub message: String,
}

fn answer(ok: bool, message: &str) -> Json<AlertActionResult> {
    Json(AlertActionResult {
        ok,
        message: message.to_owned(),
    })
}

/// Press a button on an AutoMod alert or a join-rate alert. The message must be a system card of
/// this server in a channel the caller can see; the action runs with the caller's own
/// permission (checked now, not when the card was posted) and second factor, through the
/// ordinary service, and the card is rewritten without the used button.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/alert-actions",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = AlertActionBody,
    responses((status = 200, body = AlertActionResult), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn alert_action(
    crate::auth::UserOnly(p): crate::auth::UserOnly,
    gate: MfaGate,
    Ext(deps): Ext<SettingsDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Api(api): Api,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<AlertActionBody>,
) -> ApiResult<Json<AlertActionResult>> {
    let user = super::deps::actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let channel: ChannelId = id("channel_id", &body.channel_id)?;
    let mid: MessageId = id("message_id", &body.message_id)?;
    let acc = deps.guild_access(user, gid).await?;
    let access = messages
        .access(user, channel)
        .await
        .map_err(|_| HttpError::from(ErrorCode::UnknownChannel))?;
    if access.channel.guild_id != Some(gid) || access.require_view().is_err() {
        return Err(ErrorCode::UnknownChannel.into());
    }
    let msg = messages
        .visible_message(channel, mid)
        .await
        .map_err(|_| HttpError::from(ErrorCode::UnknownMessage))?;
    if msg.author_id.get() != sr_chat::SYSTEM_USER_ID
        || msg.kind != MessageType::AutoModerationAction
        || msg.guild_id != Some(gid)
    {
        return Err(ErrorCode::UnknownMessage.into());
    }
    let button = parse_alert_button(&body.custom_id)
        .ok_or_else(|| HttpError::from(ErrorCode::UnknownMessage))?;
    // A stale copy of a card whose button is gone gets the polite answer.
    if !has_custom_id(&msg.components, &body.custom_id) {
        return Ok(answer(false, "Already handled."));
    }
    match button {
        AlertButton::ResumeInvites(g) => {
            if g != gid.get() {
                return Err(ErrorCode::UnknownMessage.into());
            }
            if !acc.perms.has(Permissions::MANAGE_GUILD) {
                return Err(ErrorCode::MissingPermissions.into());
            }
            acc.require_mfa(&gate, user, Permissions::MANAGE_GUILD)
                .await?;
            let resumed = deps
                .service
                .resume_invites(acc.perms, user, gid)
                .await
                .map_err(se)?;
            let (embeds, components) =
                apply_resume(msg.embeds.clone(), msg.components.clone(), gid, user);
            rewrite_system_card(&messages, channel, mid, &embeds, &components)
                .await
                .map_err(msg_err)?;
            if let Some(g) = deps.chat.get_guild(gid).await.map_err(re)? {
                super::community::publish_guild_update_for(&deps, &g).await;
            }
            Ok(if resumed {
                answer(true, "Invites resumed.")
            } else {
                answer(false, "Invites were already open.")
            })
        }
        AlertButton::AutoMod(press, alert_id) => {
            if alert_id != mid.get() {
                return Err(ErrorCode::UnknownMessage.into());
            }
            let alert = deps
                .service
                .repo()
                .get_alert(alert_id)
                .await
                .map_err(re)?
                .filter(|a| a.guild_id == gid && a.channel_id == channel)
                .ok_or_else(|| HttpError::from(ErrorCode::UnknownMessage))?;
            if alert.resolved_by.is_some() {
                return Ok(answer(false, "Already handled."));
            }
            // Checked now, against what the presser holds today.
            let held = acc.perms & press.needs();
            if held.is_empty() {
                return Err(ErrorCode::MissingPermissions.into());
            }
            acc.require_mfa(&gate, user, held).await?;
            let reason = format!("AutoMod alert: {}", alert.rule_name);
            // The ordinary services write their audit rows with the request's reason; here it is the rule.
            let (timeout_secs, note_gone) =
                sr_chat::settings::audit::with_ambient_reason(reason.clone(), async {
                    let mut timeout_secs = None;
                    let mut note_gone = false;
                    match press {
                        AutoModPress::Delete => {
                            note_gone = delete_flagged(&messages, user, &alert).await?;
                        }
                        AutoModPress::Timeout => {
                            let secs: u32 = body
                                .values
                                .first()
                                .and_then(|v| v.parse().ok())
                                .filter(|s| TIMEOUT_CHOICES.contains(s))
                                .ok_or_else(|| {
                                    invalid(
                                        "values",
                                        "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                                        "Choose a duration.",
                                    )
                                })?;
                            let until = deps.now_ms() + i64::from(secs) * 1000;
                            api.edit_member(
                                user,
                                gid,
                                alert.target_user_id,
                                sr_chat::admin_rules::MemberPatch {
                                    communication_disabled_until_ms: Some(Some(until)),
                                    ..Default::default()
                                },
                            )
                            .await
                            .map_err(ce)?;
                            timeout_secs = Some(secs);
                        }
                        AutoModPress::Kick => {
                            api.kick_member_with_reason(
                                user,
                                gid,
                                alert.target_user_id,
                                Some(reason),
                            )
                            .await
                            .map_err(ce)?;
                        }
                        AutoModPress::Ban => {
                            api.ban_member(user, gid, alert.target_user_id, Some(reason), 0)
                                .await
                                .map_err(ce)?;
                        }
                        AutoModPress::Resolve => {
                            deps.service
                                .repo()
                                .resolve_alert(alert_id, user, deps.now_ms())
                                .await
                                .map_err(re)?;
                        }
                    }
                    Ok::<_, HttpError>((timeout_secs, note_gone))
                })
                .await?;
            let (embeds, components) = apply_automod_press(
                msg.embeds.clone(),
                msg.components.clone(),
                alert_id,
                press,
                user,
                timeout_secs,
            );
            rewrite_system_card(&messages, channel, mid, &embeds, &components)
                .await
                .map_err(msg_err)?;
            Ok(if note_gone {
                answer(false, "The message was already removed.")
            } else {
                answer(true, "Done.")
            })
        }
    }
}

/// Remove the flagged message with the ordinary delete service. `true` when it was already gone.
async fn delete_flagged(
    messages: &MessagesDeps,
    user: UserId,
    alert: &AutoModAlert,
) -> ApiResult<bool> {
    let Some(fid) = alert.flagged_message_id else {
        return Ok(true);
    };
    let fid = MessageId::new(fid);
    let channel = alert.flagged_channel_id;
    let exists = messages
        .repo
        .get_message(channel, fid)
        .await
        .map_err(re)?
        .is_some_and(|m| m.is_visible());
    if !exists {
        return Ok(true);
    }
    messages
        .delete(user, channel, fid)
        .await
        .map_err(|e| HttpError(e.to_api_error()))?;
    messages
        .publish(DomainEvent::MessageDeleted {
            guild: Some(alert.guild_id),
            channel,
            message_id: fid.get(),
        })
        .await;
    messages
        .audit(
            Some(alert.guild_id),
            user,
            AuditAction::MessageDelete,
            Some(alert.target_user_id.get()),
            serde_json::json!({ "channel_id": channel.to_string(), "count": 1 }),
        )
        .await;
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sr_chat::settings::automod::TriggerKind;

    fn alert() -> Alert {
        Alert {
            channel_id: ChannelId::new(9),
            rule_id: 7,
            rule_name: "No links".into(),
            trigger: TriggerKind::Links,
            matched: "evil.example".into(),
            user_id: UserId::new(42),
            source_channel_id: ChannelId::new(3),
            content: "look at http://evil.example/".into(),
        }
    }

    fn custom_ids(components: &[Component]) -> Vec<String> {
        components
            .iter()
            .flat_map(|r| r.components.iter())
            .filter_map(|c| c.custom_id.clone())
            .collect()
    }

    #[test]
    fn button_ids_round_trip() {
        assert_eq!(
            parse_alert_button("sr:automod:timeout:77"),
            Some(AlertButton::AutoMod(AutoModPress::Timeout, 77))
        );
        assert_eq!(
            parse_alert_button("sr:incident:resume-invites:5"),
            Some(AlertButton::ResumeInvites(5))
        );
        for bad in [
            "sr:automod:nuke:1",
            "sr:automod:ban:0",
            "sr:automod:ban:x",
            "sr:review-approve:4",
            "automod:ban:1",
            "sr:incident:ban:1",
        ] {
            assert_eq!(parse_alert_button(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_card_has_the_fields_the_buttons_and_the_six_durations() {
        let (embeds, components) = automod_card(100, &alert(), false, true);
        let names: Vec<&str> = embeds[0].fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Rule",
                "Trigger",
                "Channel",
                "Member",
                "Matched content",
                "Blocked"
            ]
        );
        assert_eq!(
            custom_ids(&components),
            [
                "sr:automod:delete:100",
                "sr:automod:kick:100",
                "sr:automod:ban:100",
                "sr:automod:resolve:100",
                "sr:automod:timeout:100"
            ]
        );
        let select = &components[1].components[0];
        let values: Vec<&str> = select.options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(values, ["60", "300", "600", "3600", "86400", "604800"]);
        // A blocked message has nothing to delete.
        let (_, blocked) = automod_card(100, &alert(), true, false);
        assert!(!custom_ids(&blocked).contains(&"sr:automod:delete:100".to_owned()));
    }

    #[test]
    fn matched_content_is_cut_at_a_thousand_characters() {
        let mut a = alert();
        a.content = "x".repeat(2500);
        let (embeds, _) = automod_card(1, &a, false, true);
        let f = embeds[0]
            .fields
            .iter()
            .find(|f| f.name == "Matched content")
            .unwrap();
        assert_eq!(f.value.chars().count(), MATCHED_MAX);
    }

    #[test]
    fn a_press_removes_its_button_and_names_the_presser() {
        let (embeds, components) = automod_card(100, &alert(), false, true);
        let (e, c) = apply_automod_press(
            embeds.clone(),
            components.clone(),
            100,
            AutoModPress::Delete,
            UserId::new(5),
            None,
        );
        assert!(!custom_ids(&c).contains(&"sr:automod:delete:100".to_owned()));
        assert!(custom_ids(&c).contains(&"sr:automod:ban:100".to_owned()));
        assert_eq!(e[0].fields.last().unwrap().value, "Deleted by <@5>");

        let (e, c) = apply_automod_press(
            embeds.clone(),
            components.clone(),
            100,
            AutoModPress::Timeout,
            UserId::new(5),
            Some(3600),
        );
        assert_eq!(
            e[0].fields.last().unwrap().value,
            "Timed out by <@5> for 1 h"
        );
        assert!(!custom_ids(&c).contains(&"sr:automod:timeout:100".to_owned()));
        // The select's row is gone with it.
        assert_eq!(c.len(), 1);

        let (e, c) = apply_automod_press(
            embeds,
            components,
            100,
            AutoModPress::Ban,
            UserId::new(5),
            None,
        );
        assert_eq!(e[0].fields.last().unwrap().value, "Banned by <@5>");
        assert_eq!(
            custom_ids(&c),
            ["sr:automod:delete:100", "sr:automod:resolve:100"]
        );

        let (e2, c2) = apply_automod_press(e, c, 100, AutoModPress::Resolve, UserId::new(6), None);
        assert!(c2.is_empty());
        assert_eq!(
            e2[0].fields.last().unwrap().value,
            "Banned by <@5>\nResolved by <@6>"
        );
    }

    #[test]
    fn the_incident_card_offers_resume_only_when_invites_were_paused() {
        let g = GuildId::new(9);
        let (_, with) = incident_card(g, 30, 10, true);
        assert_eq!(custom_ids(&with), ["sr:incident:resume-invites:9"]);
        let (_, without) = incident_card(g, 30, 10, false);
        assert!(without.is_empty());
        let (e, c) = incident_card(g, 30, 10, true);
        let (e, c) = apply_resume(e, c, g, UserId::new(3));
        assert!(c.is_empty());
        assert_eq!(e[0].fields.last().unwrap().value, "Invites resumed by <@3>");
    }

    #[test]
    fn permissions_per_button() {
        assert_eq!(AutoModPress::Delete.needs(), Permissions::MANAGE_MESSAGES);
        assert_eq!(AutoModPress::Timeout.needs(), Permissions::MODERATE_MEMBERS);
        assert_eq!(AutoModPress::Kick.needs(), Permissions::KICK_MEMBERS);
        assert_eq!(AutoModPress::Ban.needs(), Permissions::BAN_MEMBERS);
    }
}
