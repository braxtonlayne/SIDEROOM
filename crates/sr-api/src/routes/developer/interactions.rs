//! Interactions: a member invokes a command or component (`POST /interactions`), the bot's
//! gateway session receives `INTERACTION_CREATE`, and the bot answers through the token
//! routes (callback, follow-ups, edits).

use super::{
    CallbackBody, CommandIndex, CommandIndexApp, CreateInteractionBody, DevDeps, app_err, invalid,
    iso, parse_id,
};
use crate::{
    auth::SessionOnly,
    openapi::ErrorBody,
    routes::{
        Ext,
        guilds::dto::{ChannelObject, MemberObject, RoleObject},
        messages::{
            MessagesDeps,
            error::{ApiJson, MsgError, MsgResult},
            model::UserRef,
            render::{event_payload, render_one},
        },
        settings::webhooks::ExecuteWebhookBody,
    },
};
use axum::{
    Json,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use sr_apps::{
    AppsError,
    commands::option_type,
    interactions::{
        EPHEMERAL, MessageData, SUPPRESS_EMBEDS, callback_allowed, check_options, ctype, itype,
        parse_choices, parse_message_data, parse_modal,
    },
    model::{Command, InteractionRow},
    perms::{Invoker, can_invoke},
    service::NewInteraction,
};
use sr_chat::{
    SendMessage,
    message::{Component, Embed},
    message_rules::EditMessageInput,
    repo::MessageRepository,
};
use sr_core::{ApiError, ChannelId, ErrorCode, GuildId, MessageId, UserId};
use sr_events::DomainEvent;
use std::{collections::HashMap, sync::Arc, time::Duration};

fn uid(i: i64) -> UserId {
    UserId::new(i as u64)
}

async fn to_user(deps: &DevDeps, user: i64, name: &str, payload: Value) {
    deps.publish(DomainEvent::UserScoped {
        users: vec![uid(user)],
        name: name.to_owned(),
        payload,
    })
    .await;
}

fn forbidden(m: &str) -> MsgError {
    MsgError::Api(ApiError::with_message(
        ErrorCode::MissingAccess,
        m.to_owned(),
    ))
}

/// Does any component (at any depth) of `msg` carry this `custom_id`?
fn has_custom_id(components: &[Component], id: &str) -> bool {
    components
        .iter()
        .any(|c| c.custom_id.as_deref() == Some(id) || has_custom_id(&c.components, id))
}

// ---------------------------------------------------------------- creating

/// Use an application: run a command, press a component, ask for autocomplete, submit a modal.
#[utoipa::path(
    post, path = "/interactions", tag = "bot", request_body = CreateInteractionBody,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn create(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    ApiJson(body): ApiJson<CreateInteractionBody>,
) -> MsgResult<StatusCode> {
    let kind = i16::try_from(body.kind)
        .ok()
        .filter(|k| matches!(*k, itype::APPLICATION_COMMAND..=itype::MODAL_SUBMIT))
        .ok_or_else(|| invalid("type", "BASE_TYPE_CHOICES", "Must be one of (2, 3, 4, 5)."))?;
    let channel_id = parse_id(&body.channel_id, ErrorCode::UnknownChannel)?;
    let (acc, channel, perms) = deps
        .channel_access(p.user_id, channel_id)
        .await?
        .ok_or(MsgError::Api(ApiError::new(ErrorCode::UnknownChannel)))?;
    let app_id = if body.application_id.is_empty() && kind == itype::MESSAGE_COMPONENT {
        // A component names its message; the application is the one whose bot wrote it.
        let mid = body
            .message_id
            .as_deref()
            .map(|s| parse_id(s, ErrorCode::UnknownMessage))
            .transpose()?
            .ok_or_else(|| invalid("message_id", "BASE_TYPE_REQUIRED", "This field is required"))?;
        let msg = messages
            .visible_message(
                ChannelId::new(channel_id as u64),
                MessageId::new(mid as u64),
            )
            .await?;
        deps.apps
            .application_of_bot(msg.author_id.get() as i64)
            .await
            .map_err(app_err)?
            .ok_or_else(|| {
                invalid(
                    "message_id",
                    "BASE_TYPE_CHOICES",
                    "That message does not belong to an application.",
                )
            })?
            .id
    } else {
        parse_id(&body.application_id, ErrorCode::UnknownApplication)?
    };
    let guild = acc.guild.id.get() as i64;
    if let Some(g) = &body.guild_id
        && parse_id(g, ErrorCode::UnknownGuild)? != guild
    {
        return Err(invalid(
            "guild_id",
            "BASE_TYPE_CHOICES",
            "The channel is not in that server.",
        ));
    }
    let app = deps.apps.application(app_id).await.map_err(app_err)?;
    if !app.is_live() {
        return Err(app_err(AppsError::Suspended));
    }
    if !deps
        .apps
        .is_installed(app_id, guild)
        .await
        .map_err(app_err)?
    {
        return Err(forbidden(
            "This application is not installed in this server.",
        ));
    }
    let roles: Vec<i64> = acc.member.role_ids.iter().map(|r| r.get() as i64).collect();
    let invoker = Invoker {
        user_id: p.user_id,
        guild_id: guild,
        channel_id,
        roles: &roles,
        permissions: perms,
    };

    let mut data = json!({});
    let mut command_id = None;
    let mut source_message = None;
    let mut message_obj: Option<Value> = None;
    let mut referenced = sr_apps::interactions::Referenced::default();
    match kind {
        itype::APPLICATION_COMMAND | itype::AUTOCOMPLETE => {
            let cid = body
                .data
                .get("id")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<i64>().ok())
                .ok_or_else(|| {
                    invalid(
                        "data.id",
                        "SNOWFLAKE_TYPE_INVALID",
                        "Not a valid command id.",
                    )
                })?;
            let cmd = deps
                .apps
                .command_for_invocation(app_id, Some(guild), cid)
                .await
                .map_err(app_err)?;
            if kind == itype::AUTOCOMPLETE && cmd.kind != 1 {
                return Err(invalid(
                    "type",
                    "BASE_TYPE_CHOICES",
                    "Only chat input commands autocomplete.",
                ));
            }
            let (own, defaults) = deps
                .apps
                .invocation_permissions(app_id, guild, cmd.id)
                .await
                .map_err(app_err)?;
            if !can_invoke(invoker, &own, &defaults, cmd.default_member_permissions) {
                return Err(MsgError::Api(ApiError::new(ErrorCode::MissingPermissions)));
            }
            command_id = Some(cmd.id);
            data = command_data(
                &deps,
                &messages,
                &acc,
                channel_id,
                &cmd,
                &body.data,
                kind == itype::AUTOCOMPLETE,
                &mut referenced,
            )
            .await?;
        }
        itype::MESSAGE_COMPONENT => {
            let mid = body
                .message_id
                .as_deref()
                .map(|s| parse_id(s, ErrorCode::UnknownMessage))
                .transpose()?
                .ok_or_else(|| {
                    invalid("message_id", "BASE_TYPE_REQUIRED", "This field is required")
                })?;
            let msg = messages
                .visible_message(
                    ChannelId::new(channel_id as u64),
                    MessageId::new(mid as u64),
                )
                .await?;
            if msg.author_id != uid(app.bot_user_id) {
                return Err(invalid(
                    "message_id",
                    "BASE_TYPE_CHOICES",
                    "That message does not belong to this application.",
                ));
            }
            let custom_id = body
                .data
                .get("custom_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            if custom_id.is_empty()
                || custom_id.chars().count() > 100
                || !has_custom_id(&msg.components, custom_id)
            {
                return Err(invalid(
                    "data.custom_id",
                    "BASE_TYPE_CHOICES",
                    "Unknown component.",
                ));
            }
            let component_type = body
                .data
                .get("component_type")
                .and_then(Value::as_i64)
                .unwrap_or(2);
            if !(2..=8).contains(&component_type) {
                return Err(invalid(
                    "data.component_type",
                    "BASE_TYPE_CHOICES",
                    "Not a component type.",
                ));
            }
            data = json!({"custom_id": custom_id, "component_type": component_type});
            if let Some(vals) = body.data.get("values") {
                let ok = vals.as_array().is_some_and(|a| {
                    a.len() <= 25
                        && a.iter()
                            .all(|v| v.as_str().is_some_and(|s| s.chars().count() <= 100))
                });
                if !ok {
                    return Err(invalid(
                        "data.values",
                        "BASE_TYPE_MAX_LENGTH",
                        "At most 25 values of 100 characters.",
                    ));
                }
                data["values"] = vals.clone();
            }
            source_message = Some(mid);
            message_obj = Some(
                serde_json::to_value(render_one(&messages, uid(p.user_id), &msg).await?)
                    .unwrap_or_default(),
            );
        }
        _ => {
            let custom_id = body
                .data
                .get("custom_id")
                .and_then(Value::as_str)
                .unwrap_or("");
            if custom_id.is_empty() || custom_id.chars().count() > 100 {
                return Err(invalid(
                    "data.custom_id",
                    "BASE_TYPE_BAD_LENGTH",
                    "Must be between 1 and 100 in length.",
                ));
            }
            let rows = body.data.get("components").and_then(Value::as_array);
            let ok = rows.is_some_and(|rows| {
                rows.len() <= 5
                    && rows.iter().all(|r| {
                        r.get("components")
                            .and_then(Value::as_array)
                            .is_some_and(|cs| {
                                cs.len() <= 5
                                    && cs.iter().all(|c| {
                                        c.get("custom_id")
                                            .and_then(Value::as_str)
                                            .is_some_and(|s| s.chars().count() <= 100)
                                            && c.get("value")
                                                .and_then(Value::as_str)
                                                .is_none_or(|v| v.chars().count() <= 4000)
                                    })
                            })
                    })
            });
            if !ok {
                return Err(invalid(
                    "data.components",
                    "BASE_TYPE_BAD_LENGTH",
                    "Invalid modal submission.",
                ));
            }
            data = json!({"custom_id": custom_id, "components": body.data["components"]});
        }
    }

    let (row, token) = deps
        .apps
        .create_interaction(NewInteraction {
            application_id: app_id,
            kind,
            user_id: p.user_id,
            guild_id: Some(guild),
            channel_id,
            command_id,
            source_message_id: source_message,
            nonce: body.nonce.clone().filter(|n| n.chars().count() <= 64),
        })
        .await
        .map_err(app_err)?;

    // The invoker, the bot and the bot's own permissions for the payload.
    let bot_perms = deps
        .channel_access(app.bot_user_id, channel_id)
        .await?
        .map_or(0, |(_, _, p)| p.bits());
    let users = deps.users(&[p.user_id]).await?;
    let mut member =
        serde_json::to_value(MemberObject::new(&acc.member, false, &users)).unwrap_or_default();
    member["permissions"] = json!(perms.bits().to_string());
    let overwrites = deps
        .chat()
        .get_overwrites(channel.id)
        .await
        .map_err(super::re)?;
    let mut ch =
        serde_json::to_value(ChannelObject::new(&channel, &overwrites)).unwrap_or_default();
    if let Some(o) = ch.as_object_mut() {
        o.remove("permission_overwrites");
    }
    let mut payload = json!({
        "id": row.id.to_string(),
        "application_id": app_id.to_string(),
        "type": kind,
        "data": data,
        "guild_id": guild.to_string(),
        "channel_id": channel_id.to_string(),
        "channel": ch,
        "member": member,
        "token": token,
        "version": 1,
        "app_permissions": bot_perms.to_string(),
        "locale": "en-US",
        "guild_locale": "en-US",
        "entitlements": [],
        "context": 0,
        "authorizing_integration_owners": {"0": guild.to_string()},
    });
    if let Some(m) = message_obj {
        payload["message"] = m;
    }
    // An application with an HTTP interactions endpoint gets the signed POST instead of the
    // gateway event (DEV-BOT-005).
    match deps.apps.endpoint_target(app_id).await.map_err(app_err)? {
        Some(target) if deps.endpoints_available() => {
            deps.deliver_to_endpoint(messages.clone(), target, row.clone(), payload);
        }
        _ => to_user(&deps, app.bot_user_id, "INTERACTION_CREATE", payload).await,
    }

    // The 3 second window: tell the invoker when nobody answered.
    let (d, user, id, nonce) = (deps.clone(), p.user_id, row.id, row.nonce.clone());
    let delay = deps.ack_window();
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        if matches!(d.apps.is_acknowledged(id).await, Ok(false)) {
            to_user(
                &d,
                user,
                "INTERACTION_FAILURE",
                json!({"id": id.to_string(), "nonce": nonce, "reason_code": 2}),
            )
            .await;
        }
    });
    Ok(StatusCode::NO_CONTENT)
}

/// `data` of a command interaction, validated against the registered schema.
#[allow(clippy::too_many_arguments)]
async fn command_data(
    deps: &DevDeps,
    messages: &MessagesDeps,
    acc: &super::GuildAccess,
    channel_id: i64,
    cmd: &Command,
    submitted: &Value,
    autocomplete: bool,
    referenced: &mut sr_apps::interactions::Referenced,
) -> MsgResult<Value> {
    let mut data = json!({
        "id": cmd.id.to_string(),
        "name": cmd.name,
        "type": cmd.kind,
    });
    if let Some(g) = cmd.guild_id {
        data["guild_id"] = json!(g.to_string());
    }
    match cmd.kind {
        1 => {
            let checked = check_options(
                cmd,
                submitted.get("options").unwrap_or(&Value::Null),
                autocomplete,
            )
            .map_err(|ps| app_err(AppsError::Invalid(ps)))?;
            if !checked.options.is_empty() {
                data["options"] = Value::Array(checked.options);
            }
            *referenced = checked.referenced;
        }
        2 => {
            let target = target_id(submitted)?;
            if deps
                .chat()
                .get_member(acc.guild.id, uid(target))
                .await
                .map_err(super::re)?
                .is_none()
            {
                return Err(invalid(
                    "data.target_id",
                    "BASE_TYPE_CHOICES",
                    "That user is not in this server.",
                ));
            }
            data["target_id"] = json!(target.to_string());
            referenced.users.push(target);
        }
        _ => {
            let target = target_id(submitted)?;
            let msg = messages
                .visible_message(
                    ChannelId::new(channel_id as u64),
                    MessageId::new(target as u64),
                )
                .await
                .map_err(|_| {
                    invalid(
                        "data.target_id",
                        "BASE_TYPE_CHOICES",
                        "Unknown message in this channel.",
                    )
                })?;
            data["target_id"] = json!(target.to_string());
            data["resolved"] = json!({"messages": {target.to_string(): {
                "id": target.to_string(),
                "channel_id": channel_id.to_string(),
                "author": {"id": msg.author_id.to_string()},
                "content": msg.content,
            }}});
        }
    }
    resolve(deps, acc, referenced, &mut data).await?;
    Ok(data)
}

fn target_id(d: &Value) -> MsgResult<i64> {
    d.get("target_id")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            invalid(
                "data.target_id",
                "SNOWFLAKE_TYPE_INVALID",
                "Not a valid id.",
            )
        })
}

/// Fill `data.resolved` for the ids options referenced; everything must exist in this server.
async fn resolve(
    deps: &DevDeps,
    acc: &super::GuildAccess,
    r: &sr_apps::interactions::Referenced,
    data: &mut Value,
) -> MsgResult<()> {
    let mut resolved = data.get("resolved").cloned().unwrap_or_else(|| json!({}));
    let mut user_ids = r.users.clone();
    let mut role_ids = r.roles.clone();
    for m in &r.mentionables {
        let is_role = acc.roles.iter().any(|x| x.id.get() as i64 == *m);
        if is_role {
            role_ids.push(*m);
        } else {
            user_ids.push(*m);
        }
    }
    user_ids.sort_unstable();
    user_ids.dedup();
    if !user_ids.is_empty() {
        let users = deps.users(&user_ids).await?;
        let mut u = serde_json::Map::new();
        let mut m = serde_json::Map::new();
        for id in &user_ids {
            let member = deps
                .chat()
                .get_member(acc.guild.id, uid(*id))
                .await
                .map_err(super::re)?;
            let (Some(member), Some(_)) = (member, users.get(&uid(*id))) else {
                return Err(invalid(
                    "data.options",
                    "BASE_TYPE_CHOICES",
                    "An option names someone who is not in this server.",
                ));
            };
            u.insert(
                id.to_string(),
                serde_json::to_value(UserRef::new(uid(*id), &users)).unwrap_or_default(),
            );
            let mut mo =
                serde_json::to_value(MemberObject::new(&member, false, &users)).unwrap_or_default();
            if let Some(o) = mo.as_object_mut() {
                o.remove("user");
            }
            m.insert(id.to_string(), mo);
        }
        resolved["users"] = Value::Object(u);
        resolved["members"] = Value::Object(m);
    }
    role_ids.sort_unstable();
    role_ids.dedup();
    if !role_ids.is_empty() {
        let mut roles = serde_json::Map::new();
        for id in &role_ids {
            let Some(role) = acc.roles.iter().find(|x| x.id.get() as i64 == *id) else {
                return Err(invalid(
                    "data.options",
                    "BASE_TYPE_CHOICES",
                    "An option names a role that does not exist.",
                ));
            };
            roles.insert(
                id.to_string(),
                serde_json::to_value(RoleObject::from(role)).unwrap_or_default(),
            );
        }
        resolved["roles"] = Value::Object(roles);
    }
    if !r.channels.is_empty() {
        let mut chans = serde_json::Map::new();
        for id in &r.channels {
            let ch = deps
                .chat()
                .get_channel(ChannelId::new(*id as u64))
                .await
                .map_err(super::re)?
                .filter(|c| c.guild_id == Some(acc.guild.id));
            // Only channels the invoker may view are resolved; anything else is refused so an
            // application cannot be used to learn the name of a hidden channel.
            let visible = match &ch {
                Some(c) => deps
                    .channel_access(acc.member.user_id.get() as i64, c.id.get() as i64)
                    .await?
                    .is_some(),
                None => false,
            };
            let (Some(c), true) = (ch, visible) else {
                return Err(invalid(
                    "data.options",
                    "BASE_TYPE_CHOICES",
                    "An option names a channel you cannot see.",
                ));
            };
            chans.insert(
                id.to_string(),
                json!({"id": id.to_string(), "name": c.name, "type": c.kind.code()}),
            );
        }
        resolved["channels"] = Value::Object(chans);
    }
    if resolved.as_object().is_some_and(|o| !o.is_empty()) {
        data["resolved"] = resolved;
    }
    let _ = option_type::USER;
    Ok(())
}

// ---------------------------------------------------------------- callback

async fn bot_of(deps: &DevDeps, row: &InteractionRow) -> MsgResult<i64> {
    let app = deps
        .apps
        .application(row.application_id)
        .await
        .map_err(app_err)?;
    if !app.is_live() {
        return Err(app_err(AppsError::Suspended));
    }
    Ok(app.bot_user_id)
}

fn embeds_of(d: &MessageData) -> MsgResult<Vec<Embed>> {
    serde_json::from_value(Value::Array(d.embeds.clone()))
        .map_err(|_| invalid("data.embeds", "MODEL_TYPE_CONVERT", "Invalid embed."))
}

fn components_of(d: &MessageData) -> MsgResult<Vec<Component>> {
    serde_json::from_value(Value::Array(d.components.clone())).map_err(|_| {
        invalid(
            "data.components",
            "MODEL_TYPE_CONVERT",
            "Invalid component.",
        )
    })
}

/// The Discord message object of an ephemeral message (never stored in `messages`).
async fn ephemeral_object(
    deps: &DevDeps,
    row: &InteractionRow,
    bot: i64,
    id: i64,
    d: &MessageData,
    edited: bool,
) -> MsgResult<Value> {
    let users = deps.users(&[bot]).await?;
    Ok(json!({
        "id": id.to_string(),
        "channel_id": row.channel_id.to_string(),
        "guild_id": row.guild_id.map(|g| g.to_string()),
        "author": UserRef::new(uid(bot), &users),
        "content": d.content.clone().unwrap_or_default(),
        "embeds": d.embeds,
        "components": d.components,
        "attachments": [],
        "mentions": [],
        "mention_roles": [],
        "mention_everyone": false,
        "pinned": false,
        "tts": d.tts,
        "type": 0,
        "flags": EPHEMERAL | (d.flags & SUPPRESS_EMBEDS),
        "timestamp": iso(row.created_at_ms),
        "edited_timestamp": if edited { json!(iso(deps.now_ms())) } else { Value::Null },
        "application_id": row.application_id.to_string(),
    }))
}

/// Post `d` as a real message from the bot; returns its id.
async fn post_real(
    deps: &DevDeps,
    messages: &MessagesDeps,
    row: &InteractionRow,
    bot: i64,
    d: &MessageData,
) -> MsgResult<(sr_chat::Message, bool)> {
    let channel = ChannelId::new(row.channel_id as u64);
    let msg = messages
        .send(
            uid(bot),
            channel,
            SendMessage {
                content: d.content.clone().unwrap_or_default(),
                embeds: embeds_of(d)?,
                components: components_of(d)?,
                flags: (d.flags & SUPPRESS_EMBEDS)
                    | if d.tts {
                        sr_chat::message_flags::TTS
                    } else {
                        0
                    },
                ..SendMessage::default()
            },
        )
        .await?;
    let payload = event_payload(messages, &msg).await?;
    messages
        .publish(DomainEvent::MessageSent {
            guild: msg.guild_id,
            channel,
            message_id: msg.id.get(),
            author: uid(bot),
            data: payload,
        })
        .await;
    let _ = deps;
    Ok((msg, false))
}

/// Answer an interaction. Within 3 seconds of its creation, once.
#[utoipa::path(
    post, path = "/interactions/{interaction_id}/{token}/callback", tag = "bot",
    params(("interaction_id" = String, Path, description = "Interaction snowflake"),
        ("token" = String, Path, description = "Interaction token")),
    request_body = CallbackBody,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
        (status = 404, body = ErrorBody)),
)]
pub async fn callback(
    Ext(deps): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path((id, token)): Path<(String, String)>,
    ApiJson(body): ApiJson<CallbackBody>,
) -> MsgResult<StatusCode> {
    let id = parse_id(&id, ErrorCode::UnknownInteraction)?;
    let row = deps
        .apps
        .interaction_by_token(id, &token)
        .await
        .map_err(app_err)?;
    apply_callback(&deps, &messages, row, body).await
}

/// Apply a response to an interaction: the callback route and the HTTP endpoint reply both
/// end here.
pub(super) async fn apply_callback(
    deps: &DevDeps,
    messages: &MessagesDeps,
    row: InteractionRow,
    body: CallbackBody,
) -> MsgResult<StatusCode> {
    let bot = bot_of(deps, &row).await?;
    let ty = i16::try_from(body.kind)
        .ok()
        .filter(|t| callback_allowed(row.kind, *t))
        .ok_or_else(|| {
            invalid(
                "type",
                "INTERACTION_CALLBACK_TYPE",
                "That response type does not fit this interaction.",
            )
        })?;
    let data = body.data.clone().unwrap_or_else(|| json!({}));
    let success = |extra: Value| {
        let mut v = json!({"id": row.id.to_string(), "nonce": row.nonce});
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        v
    };
    match ty {
        ctype::CHANNEL_MESSAGE => {
            let d = parse_message_data(&data, "data", true)
                .map_err(|p| app_err(AppsError::Invalid(p)))?;
            deps.apps
                .acknowledge(&row, "responded", ty, d.ephemeral())
                .await
                .map_err(app_err)?;
            let result = if d.ephemeral() {
                let mid = messages.next_id().get() as i64;
                let obj = ephemeral_object(deps, &row, bot, mid, &d, false).await?;
                deps.apps
                    .attach_message(&row, mid, true, true, Some(obj.clone()))
                    .await
                    .map_err(app_err)?;
                to_user(deps, row.user_id, "MESSAGE_CREATE", obj).await;
                Ok(())
            } else {
                match post_real(deps, messages, &row, bot, &d).await {
                    Ok((msg, _)) => deps
                        .apps
                        .attach_message(&row, msg.id.get() as i64, false, true, None)
                        .await
                        .map_err(app_err),
                    Err(e) => Err(e),
                }
            };
            match result {
                Ok(()) => {
                    to_user(deps, row.user_id, "INTERACTION_SUCCESS", success(json!({}))).await
                }
                Err(e) => {
                    to_user(
                        deps,
                        row.user_id,
                        "INTERACTION_FAILURE",
                        success(json!({"reason_code": 2})),
                    )
                    .await;
                    return Err(e);
                }
            }
        }
        ctype::DEFERRED_CHANNEL_MESSAGE | ctype::DEFERRED_UPDATE_MESSAGE => {
            let d = parse_message_data(&data, "data", false)
                .map_err(|p| app_err(AppsError::Invalid(p)))?;
            let eph = ty == ctype::DEFERRED_CHANNEL_MESSAGE && d.ephemeral();
            deps.apps
                .acknowledge(&row, "deferred", ty, eph)
                .await
                .map_err(app_err)?;
            to_user(
                deps,
                row.user_id,
                "INTERACTION_SUCCESS",
                success(json!({"deferred": true, "ephemeral": eph, "update": ty == ctype::DEFERRED_UPDATE_MESSAGE})),
            )
            .await;
        }
        ctype::UPDATE_MESSAGE => {
            let d = parse_message_data(&data, "data", false)
                .map_err(|p| app_err(AppsError::Invalid(p)))?;
            let mid = row.source_message_id.ok_or_else(|| {
                invalid(
                    "type",
                    "INTERACTION_CALLBACK_TYPE",
                    "There is no message to update.",
                )
            })?;
            if !d.embeds.is_empty() || !d.components.is_empty() {
                return Err(invalid(
                    "data",
                    "UNSUPPORTED",
                    "Embeds and components of a posted message cannot be replaced; send a follow-up or edit the content.",
                ));
            }
            deps.apps
                .acknowledge(&row, "responded", ty, false)
                .await
                .map_err(app_err)?;
            edit_real(deps, messages, &row, bot, mid, &d).await?;
            to_user(deps, row.user_id, "INTERACTION_SUCCESS", success(json!({}))).await;
        }
        ctype::AUTOCOMPLETE_RESULT => {
            let choices = parse_choices(&data).map_err(|p| app_err(AppsError::Invalid(p)))?;
            deps.apps
                .acknowledge(&row, "responded", ty, false)
                .await
                .map_err(app_err)?;
            to_user(
                deps,
                row.user_id,
                "SR_INTERACTION_AUTOCOMPLETE",
                json!({"id": row.id.to_string(), "nonce": row.nonce, "choices": choices}),
            )
            .await;
        }
        ctype::MODAL => {
            let modal = parse_modal(&data).map_err(|p| app_err(AppsError::Invalid(p)))?;
            deps.apps
                .acknowledge(&row, "responded", ty, false)
                .await
                .map_err(app_err)?;
            let mut payload = json!({
                "id": row.id.to_string(),
                "nonce": row.nonce,
                "application_id": row.application_id.to_string(),
                "channel_id": row.channel_id.to_string(),
                "guild_id": row.guild_id.map(|g| g.to_string()),
            });
            if let (Some(o), Some(m)) = (payload.as_object_mut(), modal.as_object()) {
                o.extend(m.clone());
            }
            to_user(deps, row.user_id, "INTERACTION_MODAL_CREATE", payload).await;
        }
        _ => {
            return Err(invalid(
                "type",
                "INTERACTION_CALLBACK_TYPE",
                "Unsupported response type.",
            ));
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- edits and follow-ups

async fn edit_real(
    deps: &DevDeps,
    messages: &MessagesDeps,
    row: &InteractionRow,
    bot: i64,
    mid: i64,
    d: &MessageData,
) -> MsgResult<()> {
    let channel = ChannelId::new(row.channel_id as u64);
    let message = MessageId::new(mid as u64);
    messages
        .edit(
            uid(bot),
            channel,
            message,
            EditMessageInput {
                content: d.content.clone(),
                flags: Some(d.flags & SUPPRESS_EMBEDS),
            },
        )
        .await?;
    let msg = messages.visible_message(channel, message).await?;
    let data = event_payload(messages, &msg).await?;
    messages
        .publish(DomainEvent::MessageEdited {
            guild: msg.guild_id,
            channel,
            message_id: msg.id.get(),
            data,
        })
        .await;
    let _ = deps;
    Ok(())
}

fn body_data(b: &ExecuteWebhookBody, require_body: bool) -> MsgResult<MessageData> {
    let raw = json!({
        "content": b.content,
        "embeds": b.embeds.as_ref().map(|e| serde_json::to_value(e).unwrap_or_default()),
        "components": b.components.as_ref().map(|c| serde_json::to_value(c).unwrap_or_default()),
        "flags": b.flags,
        "tts": b.tts,
    });
    parse_message_data(&raw, "", require_body).map_err(|p| app_err(AppsError::Invalid(p)))
}

/// Create a message for the interaction (follow-up, or the first message after a deferral).
async fn create_followup(
    deps: &DevDeps,
    messages: &MessagesDeps,
    row: &InteractionRow,
    d: &MessageData,
    wait: bool,
) -> MsgResult<Response> {
    if row.state == "created" {
        return Err(MsgError::Api(ApiError::with_message(
            ErrorCode::SrConflict,
            "The interaction has not been acknowledged yet.",
        )));
    }
    let bot = bot_of(deps, row).await?;
    // The first message after a deferral is the original response and inherits its flags.
    let first = row.state == "deferred" && row.original_message_id.is_none();
    let eph = d.ephemeral() || (first && row.ephemeral);
    if eph {
        let mid = messages.next_id().get() as i64;
        let obj = ephemeral_object(deps, row, bot, mid, d, false).await?;
        deps.apps
            .attach_message(row, mid, true, first, Some(obj.clone()))
            .await
            .map_err(app_err)?;
        to_user(deps, row.user_id, "MESSAGE_CREATE", obj.clone()).await;
        return Ok(if wait {
            Json(obj).into_response()
        } else {
            StatusCode::NO_CONTENT.into_response()
        });
    }
    let (msg, _) = post_real(deps, messages, row, bot, d).await?;
    deps.apps
        .attach_message(row, msg.id.get() as i64, false, first, None)
        .await
        .map_err(app_err)?;
    if first {
        to_user(
            deps,
            row.user_id,
            "INTERACTION_SUCCESS",
            json!({"id": row.id.to_string(), "nonce": row.nonce}),
        )
        .await;
    }
    if wait {
        Ok(Json(render_one(messages, uid(bot), &msg).await?).into_response())
    } else {
        Ok(StatusCode::NO_CONTENT.into_response())
    }
}

/// `POST /webhooks/{application_id}/{interaction token}`: the follow-up message. Returns
/// `None` when the path is not an interaction webhook, so the normal webhook handler runs.
pub(crate) async fn try_followup(
    deps: &DevDeps,
    messages: &MessagesDeps,
    application: &str,
    token: &str,
    wait: bool,
    body: &ExecuteWebhookBody,
) -> MsgResult<Option<Response>> {
    let Ok(app) = application.parse::<i64>() else {
        return Ok(None);
    };
    let Some(row) = deps
        .apps
        .interaction_for_webhook(app, token)
        .await
        .map_err(app_err)?
    else {
        return Ok(None);
    };
    let d = body_data(body, true)?;
    Ok(Some(create_followup(deps, messages, &row, &d, wait).await?))
}

async fn row_for(deps: &DevDeps, app: &str, token: &str) -> MsgResult<InteractionRow> {
    let app = parse_id(app, ErrorCode::UnknownWebhook)?;
    deps.apps
        .interaction_for_webhook(app, token)
        .await
        .map_err(app_err)?
        .ok_or(MsgError::Api(ApiError::new(ErrorCode::InvalidWebhookToken)))
}

fn message_ref(row: &InteractionRow, raw: &str) -> MsgResult<Option<i64>> {
    if raw == "@original" {
        Ok(row.original_message_id)
    } else {
        Ok(Some(parse_id(raw, ErrorCode::UnknownMessage)?))
    }
}

/// Read a message of the interaction.
#[utoipa::path(
    get, path = "/webhooks/{application_id}/{token}/messages/{message_id}", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("token" = String, Path, description = "Interaction token"),
        ("message_id" = String, Path, description = "Message snowflake or @original")),
    responses((status = 200, body = Object), (status = 404, body = ErrorBody)),
)]
pub async fn get_message(
    Ext(deps): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path((app, token, mid)): Path<(String, String, String)>,
) -> MsgResult<Json<Value>> {
    let row = row_for(&deps, &app, &token).await?;
    let mid =
        message_ref(&row, &mid)?.ok_or(MsgError::Api(ApiError::new(ErrorCode::UnknownMessage)))?;
    let m = deps
        .apps
        .message_of(&row, mid)
        .await
        .map_err(|_| MsgError::Api(ApiError::new(ErrorCode::UnknownMessage)))?;
    if m.ephemeral {
        return Ok(Json(m.payload.unwrap_or_default()));
    }
    let bot = bot_of(&deps, &row).await?;
    let msg = messages
        .visible_message(
            ChannelId::new(row.channel_id as u64),
            MessageId::new(mid as u64),
        )
        .await?;
    Ok(Json(
        serde_json::to_value(render_one(&messages, uid(bot), &msg).await?).unwrap_or_default(),
    ))
}

/// Edit a message of the interaction. The first edit of `@original` after a deferral creates
/// the message.
#[utoipa::path(
    patch, path = "/webhooks/{application_id}/{token}/messages/{message_id}", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("token" = String, Path, description = "Interaction token"),
        ("message_id" = String, Path, description = "Message snowflake or @original")),
    request_body = Object,
    responses((status = 200, body = Object), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
)]
pub async fn edit_message(
    Ext(deps): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path((app, token, mid)): Path<(String, String, String)>,
    ApiJson(body): ApiJson<ExecuteWebhookBody>,
) -> MsgResult<Response> {
    let row = row_for(&deps, &app, &token).await?;
    let d = body_data(&body, false)?;
    let target = message_ref(&row, &mid)?;
    let Some(mid) = target else {
        // "@original" before any original exists: only a deferred interaction may create it.
        if mid == "@original" && row.state == "deferred" {
            let d = body_data(&body, true)?;
            return create_followup(&deps, &messages, &row, &d, true).await;
        }
        return Err(MsgError::Api(ApiError::new(ErrorCode::UnknownMessage)));
    };
    let m = deps
        .apps
        .message_of(&row, mid)
        .await
        .map_err(|_| MsgError::Api(ApiError::new(ErrorCode::UnknownMessage)))?;
    let bot = bot_of(&deps, &row).await?;
    if m.ephemeral {
        let mut stored = m.payload.clone().unwrap_or_else(|| json!({}));
        if let Some(c) = &d.content {
            stored["content"] = json!(c);
        }
        if body.embeds.is_some() {
            stored["embeds"] = Value::Array(d.embeds.clone());
        }
        if body.components.is_some() {
            stored["components"] = Value::Array(d.components.clone());
        }
        stored["edited_timestamp"] = json!(iso(deps.now_ms()));
        deps.apps
            .set_ephemeral_payload(mid, &stored)
            .await
            .map_err(app_err)?;
        to_user(&deps, row.user_id, "MESSAGE_UPDATE", stored.clone()).await;
        return Ok(Json(stored).into_response());
    }
    if body.embeds.is_some() || body.components.is_some() {
        return Err(invalid(
            "embeds",
            "UNSUPPORTED",
            "Embeds and components of a posted message cannot be replaced yet; edit the content or post a follow-up.",
        ));
    }
    edit_real(&deps, &messages, &row, bot, mid, &d).await?;
    let msg = messages
        .visible_message(
            ChannelId::new(row.channel_id as u64),
            MessageId::new(mid as u64),
        )
        .await?;
    Ok(Json(render_one(&messages, uid(bot), &msg).await?).into_response())
}

/// Delete a message of the interaction.
#[utoipa::path(
    delete, path = "/webhooks/{application_id}/{token}/messages/{message_id}", tag = "bot",
    params(("application_id" = String, Path, description = "Application snowflake"),
        ("token" = String, Path, description = "Interaction token"),
        ("message_id" = String, Path, description = "Message snowflake or @original")),
    responses((status = 204), (status = 404, body = ErrorBody)),
)]
pub async fn delete_message(
    Ext(deps): Ext<DevDeps>,
    Ext(messages): Ext<MessagesDeps>,
    Path((app, token, mid)): Path<(String, String, String)>,
) -> MsgResult<StatusCode> {
    let row = row_for(&deps, &app, &token).await?;
    let mid =
        message_ref(&row, &mid)?.ok_or(MsgError::Api(ApiError::new(ErrorCode::UnknownMessage)))?;
    let m = deps
        .apps
        .message_of(&row, mid)
        .await
        .map_err(|_| MsgError::Api(ApiError::new(ErrorCode::UnknownMessage)))?;
    if m.ephemeral {
        to_user(
            &deps,
            row.user_id,
            "MESSAGE_DELETE",
            json!({"id": mid.to_string(), "channel_id": row.channel_id.to_string(), "guild_id": row.guild_id.map(|g| g.to_string())}),
        )
        .await;
    } else {
        let bot = bot_of(&deps, &row).await?;
        let channel = ChannelId::new(row.channel_id as u64);
        messages
            .delete(uid(bot), channel, MessageId::new(mid as u64))
            .await?;
        messages
            .publish(DomainEvent::MessageDeleted {
                guild: row.guild_id.map(|g| GuildId::new(g as u64)),
                channel,
                message_id: mid as u64,
            })
            .await;
    }
    deps.apps.detach_message(mid).await.map_err(app_err)?;
    Ok(StatusCode::NO_CONTENT)
}

#[allow(dead_code)]
fn _keep(_: Arc<DevDeps>, _: HashMap<(), ()>, _: Duration, _: &dyn MessageRepository) {}

// ---------------------------------------------------------------- command picker index

/// Everything the command picker needs in one channel: the applications installed in its
/// server and the commands the caller may use there. Needs a session; not part of Discord's
/// public bot API (the web client's own index).
#[utoipa::path(
    get, path = "/channels/{channel_id}/application-commands", tag = "bot",
    params(("channel_id" = String, Path, description = "Channel snowflake")),
    responses((status = 200, body = CommandIndex), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn command_index(
    SessionOnly(p): SessionOnly,
    Ext(deps): Ext<DevDeps>,
    Path(channel): Path<String>,
) -> MsgResult<Json<CommandIndex>> {
    let channel_id = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let (acc, _channel, perms) = deps
        .channel_access(p.user_id, channel_id)
        .await?
        .ok_or(MsgError::Api(ApiError::new(ErrorCode::UnknownChannel)))?;
    let guild = acc.guild.id.get() as i64;
    let roles: Vec<i64> = acc.member.role_ids.iter().map(|r| r.get() as i64).collect();
    let invoker = Invoker {
        user_id: p.user_id,
        guild_id: guild,
        channel_id,
        roles: &roles,
        permissions: perms,
    };
    let mut applications = Vec::new();
    let mut commands = Vec::new();
    for entry in deps.apps.command_index(guild).await.map_err(app_err)? {
        let mut usable = Vec::new();
        for cmd in entry.commands {
            let (own, defaults) = deps
                .apps
                .invocation_permissions(entry.application.id, guild, cmd.id)
                .await
                .map_err(app_err)?;
            if can_invoke(invoker, &own, &defaults, cmd.default_member_permissions) {
                usable.push(cmd);
            }
        }
        if usable.is_empty() {
            continue;
        }
        applications.push(CommandIndexApp {
            id: entry.application.id.to_string(),
            name: entry.application.name.clone(),
            icon: entry.application.icon_key.clone(),
            bot_id: entry.application.bot_user_id.to_string(),
        });
        commands.extend(
            usable
                .iter()
                .map(|c| sr_apps::commands::command_json(c, false)),
        );
        if commands.len() >= 2000 {
            break;
        }
    }
    Ok(Json(CommandIndex {
        applications,
        application_commands: commands,
    }))
}
