//! Active and archived thread listings.

use super::deps::ThreadsDeps;
use super::dto::{ArchivedQuery, ThreadList};
use crate::routes::Ext;
use crate::routes::guilds::dto::{ChannelObject, ThreadMemberObject, parse_iso8601};
use crate::routes::messages::error::{ApiQuery, MsgResult, invalid, parse_id};
use crate::routes::messages::render::actor;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path};
use sr_chat::threads::{ArchivedFilter, ThreadListing};
use sr_core::{ChannelId, ErrorCode, GuildId, UserId};

fn render(l: &ThreadListing) -> ThreadList {
    ThreadList {
        threads: l
            .threads
            .iter()
            .map(|c| {
                let me = l.members.iter().find(|m| m.thread_id == c.id);
                ChannelObject::new(c, &[]).with_member(me)
            })
            .collect(),
        members: l.members.iter().map(ThreadMemberObject::from).collect(),
        has_more: l.has_more,
    }
}

fn before_ms(q: &ArchivedQuery) -> MsgResult<Option<i64>> {
    q.before
        .as_deref()
        .map(|s| {
            parse_iso8601(s).ok_or_else(|| {
                invalid(
                    "before",
                    "DATE_TYPE_PARSE",
                    "Not a valid ISO8601 timestamp.",
                )
            })
        })
        .transpose()
}

/// Unarchived threads of a guild that the caller can see, with their memberships.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/threads/active",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = ThreadList), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_active(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Path(guild_id): Path<String>,
) -> MsgResult<Json<ThreadList>> {
    let me = actor(&p);
    let gid: GuildId = parse_id(&guild_id, ErrorCode::UnknownGuild)?;
    let l = t.ops.list_active(me, gid).await?;
    Ok(Json(render(&l)))
}

async fn archived(
    p: Principal,
    t: &ThreadsDeps,
    channel_id: &str,
    q: ArchivedQuery,
    pick: impl FnOnce(UserId) -> ArchivedFilter,
) -> MsgResult<Json<ThreadList>> {
    let me = actor(&p);
    let parent: ChannelId = parse_id(channel_id, ErrorCode::UnknownChannel)?;
    let before = before_ms(&q)?;
    let l = t
        .ops
        .list_archived(me, parent, pick(me), before, q.limit.unwrap_or(50))
        .await?;
    Ok(Json(render(&l)))
}

/// Archived public threads (and forum posts) of a channel, newest archive first.
/// `before` is the ISO8601 `archive_timestamp` of the last thread of the previous page.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/threads/archived/public",
    params(
        ("channel_id" = String, Path, description = "Parent channel snowflake"),
        ("before" = Option<String>, Query, description = "ISO8601 archive timestamp cursor"),
        ("limit" = Option<usize>, Query, description = "1 to 100, default 50"),
    ),
    responses((status = 200, body = ThreadList), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn archived_public(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Path(channel_id): Path<String>,
    ApiQuery(q): ApiQuery<ArchivedQuery>,
) -> MsgResult<Json<ThreadList>> {
    archived(p, &t, &channel_id, q, |_| ArchivedFilter::Public).await
}

/// Every archived private thread of a channel. Needs `MANAGE_THREADS`.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/threads/archived/private",
    params(
        ("channel_id" = String, Path, description = "Parent channel snowflake"),
        ("before" = Option<String>, Query, description = "ISO8601 archive timestamp cursor"),
        ("limit" = Option<usize>, Query, description = "1 to 100, default 50"),
    ),
    responses((status = 200, body = ThreadList), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn archived_private(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Path(channel_id): Path<String>,
    ApiQuery(q): ApiQuery<ArchivedQuery>,
) -> MsgResult<Json<ThreadList>> {
    archived(p, &t, &channel_id, q, |_| ArchivedFilter::Private).await
}

/// Archived private threads of a channel that the caller joined.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/users/@me/threads/archived/private",
    params(
        ("channel_id" = String, Path, description = "Parent channel snowflake"),
        ("before" = Option<String>, Query, description = "ISO8601 archive timestamp cursor"),
        ("limit" = Option<usize>, Query, description = "1 to 100, default 50"),
    ),
    responses((status = 200, body = ThreadList), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn archived_joined_private(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Path(channel_id): Path<String>,
    ApiQuery(q): ApiQuery<ArchivedQuery>,
) -> MsgResult<Json<ThreadList>> {
    archived(p, &t, &channel_id, q, ArchivedFilter::JoinedPrivate).await
}
