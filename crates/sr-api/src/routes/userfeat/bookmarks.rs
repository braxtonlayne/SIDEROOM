//! FTR-BKM-001: bookmarks. Private to the owner; the list shows each saved message as the owner can see it now,
//! so a bookmark on a message they lost access to (or that was deleted) comes back without its message.

use super::{UserFeatDeps, body_id, me, raw, row_id, uerr};
use crate::routes::Ext;
use crate::routes::messages::MessagesDeps;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgResult, parse_id};
use crate::routes::messages::model::MessageObject;
use crate::routes::messages::render::render_one;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};
use sr_core::{ChannelId, ErrorCode, MessageId};
use sr_userfeat::bookmarks::Bookmark;
use utoipa::ToSchema;

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct BookmarkBody {
    /// A private note (up to 500 characters).
    #[serde(default)]
    pub note: Option<String>,
}

/// A bookmark and the message it points at, when the owner can still see it.
#[derive(Debug, Serialize, ToSchema)]
pub struct BookmarkEntry {
    pub bookmark: Bookmark,
    pub message: Option<MessageObject>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    /// Message id of the last row of the previous page.
    pub before: Option<String>,
    pub limit: Option<i64>,
    /// `false` skips loading the messages (the panel needs them, the message menus only need the ids).
    pub messages: Option<bool>,
}

/// Bookmark a message, or change the note of an existing bookmark.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/messages/{message_id}/bookmark",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    request_body = BookmarkBody,
    responses(
        (status = 200, body = Bookmark),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "Bookmark limit reached"),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn save(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(m): Ext<MessagesDeps>,
    Path((channel, message)): Path<(String, String)>,
    bytes: axum::body::Bytes,
) -> MsgResult<Json<Bookmark>> {
    let user = me(&p)?;
    let channel: ChannelId = parse_id(&channel, ErrorCode::UnknownChannel)?;
    let message: MessageId = parse_id(&message, ErrorCode::UnknownMessage)?;
    let acc = m.access(user, channel).await?;
    acc.require_view()?;
    m.visible_message(channel, message).await?;
    let body: BookmarkBody = crate::routes::social::util::optional_json(&bytes)?;
    let note = body.note.unwrap_or_default();
    let guild = acc
        .channel
        .guild_id
        .and_then(|g| i64::try_from(g.get()).ok());
    Ok(Json(
        d.uf.save_bookmark(
            raw(user),
            i64::try_from(message.get()).unwrap_or(0),
            i64::try_from(channel.get()).unwrap_or(0),
            guild,
            &note,
        )
        .await
        .map_err(uerr)?,
    ))
}

/// The caller's bookmarks, newest message first, with the messages.
#[utoipa::path(
    get,
    path = "/users/@me/bookmarks",
    params(
        ("before" = Option<String>, Query, description = "Message id to page from"),
        ("limit" = Option<i64>, Query, description = "1 to 100, default 50"),
        ("messages" = Option<bool>, Query, description = "false to leave the messages out"),
    ),
    responses((status = 200, body = Vec<BookmarkEntry>), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Ext(m): Ext<MessagesDeps>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> MsgResult<Json<Vec<BookmarkEntry>>> {
    let user = me(&p)?;
    let before = q
        .before
        .as_deref()
        .map(|b| body_id("before", b))
        .transpose()?;
    let rows =
        d.uf.list_bookmarks(raw(user), before, q.limit.unwrap_or(50))
            .await
            .map_err(uerr)?;
    let mut out = Vec::with_capacity(rows.len());
    for bookmark in rows {
        let message = if q.messages == Some(false) {
            None
        } else {
            visible(&m, user, &bookmark).await
        };
        out.push(BookmarkEntry { bookmark, message });
    }
    Ok(Json(out))
}

/// The message of a bookmark as `user` can see it now; `None` when it is gone or out of reach.
async fn visible(m: &MessagesDeps, user: sr_core::UserId, b: &Bookmark) -> Option<MessageObject> {
    let channel = ChannelId::new(b.channel_id.parse().ok()?);
    let message = MessageId::new(b.message_id.parse().ok()?);
    let acc = m.access(user, channel).await.ok()?;
    acc.require_view().ok()?;
    let msg = m.visible_message(channel, message).await.ok()?;
    render_one(m, user, &msg).await.ok()
}

/// Change a bookmark's note.
#[utoipa::path(
    patch,
    path = "/users/@me/bookmarks/{message_id}",
    params(("message_id" = String, Path, description = "Message snowflake")),
    request_body = BookmarkBody,
    responses((status = 200, body = Bookmark), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn edit(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Path(message): Path<String>,
    ApiJson(body): ApiJson<BookmarkBody>,
) -> MsgResult<Json<Bookmark>> {
    let user = me(&p)?;
    let message = row_id(&message, "bookmark")?;
    let current = d.uf.get_bookmark(raw(user), message).await.map_err(uerr)?;
    Ok(Json(
        d.uf.save_bookmark(
            raw(user),
            message,
            current.channel_id.parse().unwrap_or(0),
            current.guild_id.as_deref().and_then(|g| g.parse().ok()),
            body.note.as_deref().unwrap_or(""),
        )
        .await
        .map_err(uerr)?,
    ))
}

/// Remove a bookmark (removing one that is not there is fine).
#[utoipa::path(
    delete,
    path = "/users/@me/bookmarks/{message_id}",
    params(("message_id" = String, Path, description = "Message snowflake")),
    responses((status = 204, description = "Removed")),
    security(("authorization" = []), ("session" = []))
)]
pub async fn remove(
    p: Principal,
    Ext(d): Ext<UserFeatDeps>,
    Path(message): Path<String>,
) -> MsgResult<StatusCode> {
    let user = me(&p)?;
    let message = row_id(&message, "bookmark")?;
    d.uf.remove_bookmark(raw(user), message)
        .await
        .map_err(uerr)?;
    Ok(StatusCode::NO_CONTENT)
}
