//! Friend Shelf shares, guild storage grants and the shelves summary (BYOS-SCP-001).

use super::{ScopeDeps, facts_failed, parse_row, uid};
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::messages::error::{ApiJson, MsgResult, invalid};
use crate::routes::{Ext, MaybeExt};
use axum::{Json, extract::Path, http::StatusCode};
use serde::{Deserialize, Serialize};
use sr_core::ErrorCode;
use sr_storage::mounts::MountOwner;
use sr_storage::scopes::{Grant, GrantSet, Share, grants_for};
use utoipa::ToSchema;

/// One friend's share of a mount.
#[derive(Debug, Serialize, ToSchema)]
pub struct ShareView {
    /// The friend (snowflake).
    pub friend_id: String,
    /// Cinema shared.
    pub cinema: bool,
    /// Library shared.
    pub library: bool,
    /// Music shared.
    pub music: bool,
}

/// Body of `PUT .../shares/{user_id}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ShareBody {
    /// Share the Cinema kind.
    #[serde(default)]
    pub cinema: bool,
    /// Share the Library kind.
    #[serde(default)]
    pub library: bool,
    /// Share the Music kind.
    #[serde(default)]
    pub music: bool,
}

/// A mount a friend shares with the caller.
#[derive(Debug, Serialize, ToSchema)]
pub struct IncomingShare {
    /// The mount (snowflake).
    pub mount_id: String,
    /// The friend who owns it (snowflake).
    pub owner_id: String,
    /// The mount's label.
    pub label: String,
    /// Cinema shared.
    pub cinema: bool,
    /// Library shared.
    pub library: bool,
    /// Music shared.
    pub music: bool,
}

/// Instance catalog counts the viewer can see.
#[derive(Debug, Serialize, ToSchema)]
pub struct CatalogCounts {
    /// Cinema titles.
    pub cinema: i64,
    /// Library series.
    pub library: i64,
}

/// Shelves the caller reaches, per kind.
#[derive(Debug, Serialize, ToSchema)]
pub struct ShelvesSummaryView {
    /// Cinema shelves.
    pub cinema: i64,
    /// Library shelves.
    pub library: i64,
    /// Music shelves.
    pub music: i64,
    /// Instance catalog counts (absent until the catalog reports them).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog: Option<CatalogCounts>,
}

/// Body of `PUT /guilds/{guild_id}/storage/grants`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantsBody {
    /// The role (snowflake).
    pub role_id: String,
    /// The grants the role holds afterwards: `view_cinema`, `view_library`, `view_music`, `upload`, `manage`.
    pub grants: Vec<String>,
}

fn share_view(friend: i64, s: Share) -> ShareView {
    ShareView {
        friend_id: friend.to_string(),
        cinema: s.cinema,
        library: s.library,
        music: s.music,
    }
}

/// The caller's own live user mount, else 404 (another user's mount looks like no mount).
async fn own_mount(d: &ScopeDeps, user: i64, mount_id: &str) -> MsgResult<i64> {
    let id = parse_row(mount_id)?;
    match d.mounts.by_id(id).await {
        Ok(Some(m)) if m.owner == MountOwner::User(user) => Ok(id),
        Ok(_) => Err(ErrorCode::SrNotFound.into()),
        Err(e) => {
            tracing::error!(error = %e, "mount lookup failed");
            Err(ErrorCode::SrInternal.into())
        }
    }
}

/// The shares the owner of a mount has given out.
#[utoipa::path(
    get,
    path = "/users/@me/storage/mounts/{mount_id}/shares",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses((status = 200, body = Vec<ShareView>), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_shares(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Path(mount_id): Path<String>,
) -> MsgResult<Json<Vec<ShareView>>> {
    let id = own_mount(&d, uid(&p), &mount_id).await?;
    let rows = d.store.shares_of_mount(id).await.map_err(facts_failed)?;
    Ok(Json(
        rows.into_iter().map(|(f, s)| share_view(f, s)).collect(),
    ))
}

/// Share a mount with a friend, per kind. Mutual friends only; the share ends when the friendship does.
#[utoipa::path(
    put,
    path = "/users/@me/storage/mounts/{mount_id}/shares/{user_id}",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("user_id" = String, Path, description = "Friend snowflake"),
    ),
    request_body = ShareBody,
    responses(
        (status = 200, body = ShareView),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn put_share(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    MaybeExt(events): MaybeExt<super::events::StorageEvents>,
    Path((mount_id, friend)): Path<(String, String)>,
    ApiJson(body): ApiJson<ShareBody>,
) -> MsgResult<Json<ShareView>> {
    let user = uid(&p);
    let id = own_mount(&d, user, &mount_id).await?;
    let friend = friend
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| {
            invalid(
                "user_id",
                "SNOWFLAKE_TYPE_INVALID",
                "Value is not snowflake.",
            )
        })?;
    if friend == user {
        return Err(invalid(
            "user_id",
            "SELF",
            "You cannot share with yourself.",
        ));
    }
    if !d
        .store
        .are_friends(user, friend)
        .await
        .map_err(facts_failed)?
    {
        return Err(invalid(
            "user_id",
            "NOT_FRIENDS",
            "Only friends can be given a share.",
        ));
    }
    let share = Share {
        cinema: body.cinema,
        library: body.library,
        music: body.music,
    };
    d.store
        .set_share(id, friend, share)
        .await
        .map_err(facts_failed)?;
    if let Some(ev) = events {
        let change = if share.any() { "shared" } else { "unshared" };
        ev.share_changed(id, user, friend, change).await;
    }
    Ok(Json(share_view(friend, share)))
}

/// Stop sharing a mount with a friend (idempotent).
#[utoipa::path(
    delete,
    path = "/users/@me/storage/mounts/{mount_id}/shares/{user_id}",
    params(
        ("mount_id" = String, Path, description = "Mount snowflake"),
        ("user_id" = String, Path, description = "Friend snowflake"),
    ),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_share(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    MaybeExt(events): MaybeExt<super::events::StorageEvents>,
    Path((mount_id, friend)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let id = own_mount(&d, uid(&p), &mount_id).await?;
    let friend = parse_row(&friend)?;
    d.store
        .delete_share(id, friend)
        .await
        .map_err(facts_failed)?;
    if let Some(ev) = events {
        ev.share_changed(id, uid(&p), friend, "unshared").await;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// What friends share with the caller.
#[utoipa::path(
    get,
    path = "/users/@me/storage/shares",
    responses((status = 200, body = Vec<IncomingShare>)),
    security(("session" = []))
)]
pub async fn incoming_shares(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
) -> MsgResult<Json<Vec<IncomingShare>>> {
    let rows = d.store.shares_to(uid(&p)).await.map_err(facts_failed)?;
    Ok(Json(
        rows.into_iter()
            .map(|r| IncomingShare {
                mount_id: r.mount_id.to_string(),
                owner_id: r.owner_id.to_string(),
                label: r.label,
                cinema: r.share.cinema,
                library: r.share.library,
                music: r.share.music,
            })
            .collect(),
    ))
}

/// How many shelves of each kind the caller reaches (the nav modes read this).
#[utoipa::path(
    get,
    path = "/users/@me/shelves/summary",
    responses((status = 200, body = ShelvesSummaryView)),
    security(("session" = []))
)]
pub async fn summary(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
) -> MsgResult<Json<ShelvesSummaryView>> {
    let s = d
        .store
        .shelves_summary(uid(&p))
        .await
        .map_err(facts_failed)?;
    Ok(Json(ShelvesSummaryView {
        cinema: s.cinema,
        library: s.library,
        music: s.music,
        catalog: d
            .store
            .catalog_counts()
            .await
            .ok()
            .map(|(cinema, library)| CatalogCounts { cinema, library }),
    }))
}

/// Replace the storage grants of a role. Needs `MANAGE_ROLES` and the `manage` storage grant (owner and
/// `ADMINISTRATOR` hold both; `MANAGE_GUILD` implies `manage`).
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/storage/grants",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = GrantsBody,
    responses(
        (status = 204),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn put_grants(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<GrantsBody>,
) -> MsgResult<StatusCode> {
    let guild = parse_row(&guild_id)?;
    let Some(standing) = d
        .store
        .guild_standing(guild, uid(&p))
        .await
        .map_err(facts_failed)?
    else {
        return Err(ErrorCode::SrNotFound.into());
    };
    let held = d
        .store
        .role_grants(guild, &standing.role_ids)
        .await
        .map_err(facts_failed)?;
    let mine = grants_for(&standing, &held);
    let may_roles = standing.is_owner || standing.administrator || standing.manage_roles;
    if !(may_roles && mine.has(Grant::Manage)) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let role = body
        .role_id
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| {
            invalid(
                "role_id",
                "SNOWFLAKE_TYPE_INVALID",
                "Value is not snowflake.",
            )
        })?;
    let mut set = GrantSet::default();
    for g in &body.grants {
        let Some(g) = Grant::parse(g) else {
            return Err(invalid(
                "grants",
                "ENUM_TYPE_COERCE",
                "Unknown storage grant.",
            ));
        };
        set.insert(g);
    }
    if !d
        .store
        .role_in_guild(guild, role)
        .await
        .map_err(facts_failed)?
    {
        return Err(ErrorCode::SrNotFound.into());
    }
    d.store
        .set_role_grants(guild, role, &set)
        .await
        .map_err(facts_failed)?;
    Ok(StatusCode::NO_CONTENT)
}
