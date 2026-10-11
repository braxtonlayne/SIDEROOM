//! Friends, requests, blocks, nicknames and ignores (LNG-REL-*, Q110).

use super::deps::SocialDeps;
use super::error::{ApiJson, MsgResult, invalid};
use super::events::SocialEvent;
use super::model::{NicknameBody, PutRelationshipBody, RelationshipObject, UserObject};
use super::util::{actor, optional_json, user_id};
use crate::{Principal, openapi::ErrorBody, routes::Ext};
use axum::{Json, body::Bytes, extract::Path, http::StatusCode};
use serde_json::json;
use sr_chat::social::relationships::{Plan, RelOp, Relationship, RelationshipKind};
use sr_chat::social::repo::RelationshipRepository;
use sr_core::UserId;

const KIND_BLOCKED: i16 = 2;
const KIND_FRIEND: i16 = 1;

/// All relationships of the caller: friends, blocks and pending requests.
#[utoipa::path(
    get,
    path = "/users/@me/relationships",
    responses((status = 200, body = Vec<RelationshipObject>), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<Vec<RelationshipObject>>> {
    let me = actor(&p)?;
    let rels = deps.service.list_relationships(me).await?;
    let users = deps.summaries(rels.iter().map(|r| r.target)).await?;
    Ok(Json(
        rels.iter()
            .map(|r| RelationshipObject::new(r, &users))
            .collect(),
    ))
}

/// One relationship of the caller; 404 when there is none.
#[utoipa::path(
    get,
    path = "/users/@me/relationships/{user_id}",
    params(("user_id" = String, Path, description = "Other user's snowflake")),
    responses((status = 200, body = RelationshipObject), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_one(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
) -> MsgResult<Json<RelationshipObject>> {
    let me = actor(&p)?;
    let them = user_id(&target)?;
    let rel = deps
        .service
        .repo()
        .get_relationship(me, them)
        .await?
        .ok_or(sr_core::ErrorCode::UnknownUser)?;
    let users = deps.summaries([them]).await?;
    Ok(Json(RelationshipObject::new(&rel, &users)))
}

/// Send a friend request, accept an incoming one (`type` 1 or omitted), or block
/// (`type` 2).
#[utoipa::path(
    put,
    path = "/users/@me/relationships/{user_id}",
    params(("user_id" = String, Path, description = "Other user's snowflake")),
    request_body = PutRelationshipBody,
    responses(
        (status = 204, description = "Done"),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody)
    ),
    security(("authorization" = []), ("session" = []))
)]
pub async fn put(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
    body: Bytes,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    let them = user_id(&target)?;
    let kind = optional_json::<PutRelationshipBody>(&body)?
        .kind
        .unwrap_or(KIND_FRIEND);
    let before = snapshot(&deps, me, them).await?;
    let plan = match kind {
        KIND_FRIEND => deps.service.send_friend_request(me, them).await?,
        KIND_BLOCKED => deps.service.block(me, them).await?,
        _ => {
            return Err(invalid(
                "type",
                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                "Must be 1 (friend) or 2 (blocked).",
            ));
        }
    };
    announce(&deps, me, them, &before, &plan).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove a friend, cancel or decline a request, or unblock.
#[utoipa::path(
    delete,
    path = "/users/@me/relationships/{user_id}",
    params(("user_id" = String, Path, description = "Other user's snowflake")),
    responses((status = 204, description = "Removed"), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn delete(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    let them = user_id(&target)?;
    let before = snapshot(&deps, me, them).await?;
    let plan = if before.mine.as_ref().map(|r| r.kind) == Some(RelationshipKind::Blocked) {
        deps.service.unblock(me, them).await?
    } else {
        deps.service.remove_relationship(me, them).await?
    };
    announce(&deps, me, them, &before, &plan).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Set or clear a friend's private nickname (max 32 characters).
#[utoipa::path(
    patch,
    path = "/users/@me/relationships/{user_id}",
    params(("user_id" = String, Path, description = "Friend's snowflake")),
    request_body = NicknameBody,
    responses((status = 200, body = RelationshipObject), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn set_nickname(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
    ApiJson(body): ApiJson<NicknameBody>,
) -> MsgResult<Json<RelationshipObject>> {
    let me = actor(&p)?;
    let them = user_id(&target)?;
    let before = snapshot(&deps, me, them).await?;
    let plan = deps
        .service
        .set_friend_nickname(me, them, body.nickname.as_deref())
        .await?;
    announce(&deps, me, them, &before, &plan).await?;
    let rel = deps
        .service
        .repo()
        .get_relationship(me, them)
        .await?
        .ok_or(sr_core::ErrorCode::UnknownUser)?;
    let users = deps.summaries([them]).await?;
    Ok(Json(RelationshipObject::new(&rel, &users)))
}

/// Users the caller has ignored (Q110: independent of friendship).
#[utoipa::path(
    get,
    path = "/users/@me/ignores",
    responses((status = 200, body = Vec<UserObject>)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_ignores(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
) -> MsgResult<Json<Vec<UserObject>>> {
    let me = actor(&p)?;
    let ids = deps.service.list_ignores(me).await?;
    let users = deps.summaries(ids.iter().copied()).await?;
    Ok(Json(
        ids.iter().map(|u| UserObject::new(*u, &users)).collect(),
    ))
}

/// Ignore a user: their messages, activity and DM notifications are hidden, the
/// friendship stays.
#[utoipa::path(
    put,
    path = "/users/@me/ignores/{user_id}",
    params(("user_id" = String, Path, description = "User snowflake")),
    responses((status = 204, description = "Ignored"), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn ignore(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    deps.service.ignore(me, user_id(&target)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Stop ignoring a user.
#[utoipa::path(
    delete,
    path = "/users/@me/ignores/{user_id}",
    params(("user_id" = String, Path, description = "User snowflake")),
    responses((status = 204, description = "No longer ignored"), (status = 400, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn unignore(
    p: Principal,
    Ext(deps): Ext<SocialDeps>,
    Path(target): Path<String>,
) -> MsgResult<StatusCode> {
    let me = actor(&p)?;
    deps.service.unignore(me, user_id(&target)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Both rows of a pair before a mutation, to describe removals afterwards.
struct Snapshot {
    mine: Option<Relationship>,
    theirs: Option<Relationship>,
}

async fn snapshot(deps: &SocialDeps, me: UserId, them: UserId) -> MsgResult<Snapshot> {
    let repo = deps.service.repo();
    Ok(Snapshot {
        mine: repo.get_relationship(me, them).await?,
        theirs: repo.get_relationship(them, me).await?,
    })
}

/// Turn the applied ops into gateway events: `RELATIONSHIP_ADD` for new or changed
/// rows, `RELATIONSHIP_UPDATE` for nickname changes, `RELATIONSHIP_REMOVE` for
/// deleted rows (each to the row's owner only).
async fn announce(
    deps: &SocialDeps,
    me: UserId,
    them: UserId,
    before: &Snapshot,
    plan: &Plan,
) -> MsgResult<()> {
    if plan.ops.is_empty() {
        return Ok(());
    }
    let users = deps.summaries([me, them]).await?;
    let prior = |owner: UserId| {
        if owner == me {
            before.mine.as_ref()
        } else {
            before.theirs.as_ref()
        }
    };
    for op in &plan.ops {
        match op {
            RelOp::Upsert {
                user,
                target,
                kind,
                since_ms,
            } => {
                let nickname = prior(*user)
                    .filter(|r| r.kind == *kind)
                    .and_then(|r| r.nickname.clone());
                let payload = json!({
                    "id": target.to_string(),
                    "type": kind.code(),
                    "nickname": nickname,
                    "since": crate::routes::messages::model::iso(*since_ms),
                    "user": UserObject::new(*target, &users),
                });
                deps.emit(SocialEvent::new("RELATIONSHIP_ADD", vec![*user], payload))
                    .await;
            }
            RelOp::Delete { user, target } => {
                let kind = prior(*user).map_or(0, |r| r.kind.code());
                let payload = json!({ "id": target.to_string(), "type": kind, "nickname": null });
                deps.emit(SocialEvent::new(
                    "RELATIONSHIP_REMOVE",
                    vec![*user],
                    payload,
                ))
                .await;
            }
            RelOp::SetNickname {
                user,
                target,
                nickname,
            } => {
                let payload = json!({
                    "id": target.to_string(),
                    "type": KIND_FRIEND,
                    "nickname": nickname,
                });
                deps.emit(SocialEvent::new(
                    "RELATIONSHIP_UPDATE",
                    vec![*user],
                    payload,
                ))
                .await;
            }
        }
    }
    Ok(())
}
