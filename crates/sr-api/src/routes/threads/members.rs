//! Thread membership endpoints.

use super::deps::ThreadsDeps;
use super::dto::MembersQuery;
use crate::routes::Ext;
use crate::routes::guilds::dto::ThreadMemberObject;
use crate::routes::messages::error::{ApiQuery, MsgResult, invalid, parse_id};
use crate::routes::messages::render::actor;
use crate::{Principal, openapi::ErrorBody};
use axum::{Json, extract::Path, http::StatusCode};
use sr_core::{ChannelId, ErrorCode, UserId};

/// `@me` or a user snowflake.
fn target(raw: &str, me: UserId) -> MsgResult<UserId> {
    if raw == "@me" {
        Ok(me)
    } else {
        parse_id(raw, ErrorCode::UnknownUser)
    }
}

/// Add a member to a thread (`@me` joins). The thread must not be archived.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/thread-members/{user_id}",
    params(
        ("channel_id" = String, Path, description = "Thread snowflake"),
        ("user_id" = String, Path, description = "User snowflake or `@me`"),
    ),
    responses(
        (status = 204, description = "Added (or already a member)"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn add_member(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Path((channel_id, user_id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let me = actor(&p);
    let tid: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let who = target(&user_id, me)?;
    let change = t.ops.add_member(me, tid, who).await?;
    if change.changed
        && let Some(m) = &change.member
    {
        t.publish_members(&change.thread, std::slice::from_ref(m), &[])
            .await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Remove a member from a thread (`@me` leaves). Removing others needs
/// `MANAGE_THREADS`, or being the owner of a private thread.
#[utoipa::path(
    delete,
    path = "/channels/{channel_id}/thread-members/{user_id}",
    params(
        ("channel_id" = String, Path, description = "Thread snowflake"),
        ("user_id" = String, Path, description = "User snowflake or `@me`"),
    ),
    responses(
        (status = 204, description = "Removed (or was not a member)"),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn remove_member(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Path((channel_id, user_id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let me = actor(&p);
    let tid: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let who = target(&user_id, me)?;
    let change = t.ops.remove_member(me, tid, who).await?;
    if change.changed {
        t.publish_members(&change.thread, &[], &[who]).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// One thread member.
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/thread-members/{user_id}",
    params(
        ("channel_id" = String, Path, description = "Thread snowflake"),
        ("user_id" = String, Path, description = "User snowflake or `@me`"),
    ),
    responses(
        (status = 200, body = ThreadMemberObject),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_member(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Path((channel_id, user_id)): Path<(String, String)>,
) -> MsgResult<Json<ThreadMemberObject>> {
    let me = actor(&p);
    let tid: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let who = target(&user_id, me)?;
    let m = t.ops.get_member(me, tid, who).await?;
    Ok(Json(ThreadMemberObject::from(&m)))
}

/// Members of a thread, ordered by user id (`after` pages; `limit` 1-100).
#[utoipa::path(
    get,
    path = "/channels/{channel_id}/thread-members",
    params(
        ("channel_id" = String, Path, description = "Thread snowflake"),
        ("after" = Option<String>, Query, description = "Last user id of the previous page"),
        ("limit" = Option<usize>, Query, description = "1 to 100, default 100"),
        ("with_member" = Option<bool>, Query, description = "Accepted for compatibility"),
    ),
    responses(
        (status = 200, body = Vec<ThreadMemberObject>),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_members(
    p: Principal,
    Ext(t): Ext<ThreadsDeps>,
    Path(channel_id): Path<String>,
    ApiQuery(q): ApiQuery<MembersQuery>,
) -> MsgResult<Json<Vec<ThreadMemberObject>>> {
    let me = actor(&p);
    let tid: ChannelId = parse_id(&channel_id, ErrorCode::UnknownChannel)?;
    let after = q
        .after
        .as_deref()
        .map(|s| {
            s.parse::<UserId>()
                .map_err(|_| invalid("after", "SNOWFLAKE_TYPE_INVALID", "Value is not snowflake."))
        })
        .transpose()?;
    let rows = t
        .ops
        .list_members(me, tid, after, q.limit.unwrap_or(100))
        .await?;
    Ok(Json(rows.iter().map(ThreadMemberObject::from).collect()))
}
