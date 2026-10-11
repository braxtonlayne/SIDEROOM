//! `GET /channels/{id}/messages` (before/after/around) and `GET /channels/{id}/messages/{id}`
//! (LNG-MSG-003).

use super::deps::MessagesDeps;
use super::error::{ApiQuery, MsgResult, invalid, parse_id};
use super::model::{ListMessagesQuery, MessageObject};
use super::render::{actor, render_many, render_one};
use super::store::Page;
use crate::{Principal, openapi::ErrorBody};
use axum::{Extension, Json, extract::Path};
use sr_chat::Message;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, ErrorCode, MessageId};
use std::sync::Arc;

pub const DEFAULT_LIMIT: u32 = 50;
pub const MAX_LIMIT: u32 = 100;

fn page_id(raw: Option<&String>, field: &str) -> MsgResult<Option<MessageId>> {
    raw.map(|s| {
        s.parse::<MessageId>()
            .map_err(|_| invalid(field, "NUMBER_TYPE_COERCE", "Value is not snowflake."))
    })
    .transpose()
}

/// Fetch a page and return it newest first (Discord order).
pub async fn fetch_page(
    deps: &MessagesDeps,
    channel: ChannelId,
    q: &ListMessagesQuery,
) -> MsgResult<Vec<Message>> {
    let limit = match q.limit {
        None => DEFAULT_LIMIT,
        Some(l) if (1..=MAX_LIMIT).contains(&l) => l,
        Some(_) => {
            return Err(invalid(
                "limit",
                "NUMBER_TYPE_MAX",
                "Must be between 1 and 100.",
            ));
        }
    } as usize;
    let before = page_id(q.before.as_ref(), "before")?;
    let after = page_id(q.after.as_ref(), "after")?;
    let around = page_id(q.around.as_ref(), "around")?;
    if [before, after, around].iter().flatten().count() > 1 {
        return Err(invalid(
            "before",
            "MUTUALLY_EXCLUSIVE",
            "Only one of before, after and around may be set.",
        ));
    }
    let page = |before, after, limit, ascending| Page {
        before,
        after,
        limit,
        ascending,
    };
    let mut out = if let Some(mid) = around {
        // Up to ceil(limit/2) messages at or before `mid`, then the rest after it.
        let older_n = limit.div_ceil(2);
        let upper = MessageId::new(mid.get().saturating_add(1));
        let mut older = deps
            .store
            .fetch(channel, page(Some(upper), None, older_n, false))
            .await?;
        let newer = deps
            .store
            .fetch(channel, page(None, Some(mid), limit - older_n, true))
            .await?;
        older.extend(newer.into_iter().rev());
        older.sort_by_key(|m| std::cmp::Reverse(m.id));
        older
    } else if let Some(aid) = after {
        let mut v = deps
            .store
            .fetch(channel, page(None, Some(aid), limit, true))
            .await?;
        v.reverse();
        v
    } else {
        deps.store
            .fetch(channel, page(before, None, limit, false))
            .await?
    };
    out.truncate(limit);
    Ok(out)
}

/// Get channel messages.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/messages",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("limit" = Option<u32>, Query, description = "1-100, default 50"),
        ("before" = Option<String>, Query, description = "Messages before this id"),
        ("after" = Option<String>, Query, description = "Messages after this id"),
        ("around" = Option<String>, Query, description = "Messages around this id"),
    ),
    responses(
        (status = 200, body = Vec<MessageObject>),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_messages(
    p: Principal,
    Extension(deps): Extension<Arc<MessagesDeps>>,
    Path(channel_id): Path<String>,
    ApiQuery(q): ApiQuery<ListMessagesQuery>,
) -> MsgResult<Json<Vec<MessageObject>>> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let acc = deps.access(user, channel_id).await?;
    acc.require_view()?;
    // Without history access Discord returns an empty list rather than an error.
    if !acc.perms.has(Permissions::READ_MESSAGE_HISTORY) {
        return Ok(Json(Vec::new()));
    }
    let msgs = fetch_page(&deps, channel_id, &q).await?;
    Ok(Json(render_many(&deps, user, &msgs).await?))
}

/// Get a single channel message.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/messages/{message_id}",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    responses(
        (status = 200, body = MessageObject),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_message(
    p: Principal,
    Extension(deps): Extension<Arc<MessagesDeps>>,
    Path((channel_id, message_id)): Path<(String, String)>,
) -> MsgResult<Json<MessageObject>> {
    let user = actor(&p);
    let channel_id: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let message_id: MessageId = parse_id(&message_id, ErrorCode::UnknownMessage)?;
    let acc = deps.access(user, channel_id).await?;
    acc.require(Permissions::READ_MESSAGE_HISTORY)?;
    let msg = deps.visible_message(channel_id, message_id).await?;
    Ok(Json(render_one(&deps, user, &msg).await?))
}
