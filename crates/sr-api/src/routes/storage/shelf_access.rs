//! Shelf access (STU-ZK-001; P28-SHELF-1/2, P28-LEG-1; D-393, D-394). A shelf is a storage mount: its
//! credentials stay sealed on the server and never reach a grantee, who gets only a presigned per-object URL
//! that lives ten minutes. Grants are rows tied to the account, so they work on any device; the catalog is
//! plaintext so a single object can be delisted. Every read decides through `sr_storage::scopes`.
//!
//! What this protects: other members (no grant, no listing, no URL) and grantees (no credential; a leaked URL is
//! one object for ten minutes). It does not protect against the server, the Operator or an attacker who gets
//! `sr-server`: they can read credentials and catalogs. The mitigation is the read-only, prefix-scoped key the
//! owner supplies. Object bodies never pass through the server.

use super::objects::{Reach, bucket_error, reach, session_left, storage_for};
use super::{ScopeDeps, facts_failed, parse_row, refusal, uid};
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::admin::AdminDeps;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgResult, invalid};
use crate::routes::messages::model::iso;
use crate::routes::{Ext, MaybeExt};
use crate::state::AppState;
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use sr_core::ErrorCode;
use sr_storage::mounts::MountOwner;
use sr_storage::scopes::{Refused, Scope, ScopeFacts, Viewer, presign_ttl_secs};
use sr_storage::shelf::{
    CatalogEntry, GrantError, GrantRow, ShelfAccessStore, URL_TTL_SECS, is_object_hash,
};
use sr_storage::{ObjectStorage, PresignMethod};
use std::sync::Arc;
use std::time::Duration;
use utoipa::ToSchema;

/// Collaborators of the shelf access routes.
pub struct ShelfAccessDeps {
    /// The catalog and grant rows.
    pub store: Arc<dyn ShelfAccessStore>,
}

/// Catalog page size.
const PAGE: i64 = 200;

/// One shelf the caller can see.
#[derive(Debug, Serialize, ToSchema)]
pub struct VisibleShelf {
    /// The shelf (mount snowflake).
    pub shelf_id: String,
    /// The mount's label.
    pub label: String,
    /// How it reaches the caller: `own`, `friend` or `guild`.
    pub via: String,
    /// The owning member (snowflake), absent for a server shelf.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    /// The owning member's handle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_handle: Option<String>,
    /// For `guild`: the server (snowflake).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    /// For `guild`: the server's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_name: Option<String>,
    /// Objects in the catalog that the caller can open.
    pub object_count: i64,
    /// Whether the caller owns it and can edit titles, covers, notes and grants.
    pub can_manage: bool,
}

/// `GET /shelves/visible`.
#[derive(Debug, Serialize, ToSchema)]
pub struct VisibleShelves {
    /// Own shelves first, then friends', then servers'.
    pub shelves: Vec<VisibleShelf>,
}

/// An object of a shelf catalog. Never carries a bucket, an endpoint or a credential.
#[derive(Debug, Serialize, ToSchema)]
pub struct CatalogObject {
    /// 64 lowercase hex characters.
    pub object_hash: String,
    /// Mount-relative key.
    pub path: String,
    /// Size in bytes.
    pub size: i64,
    /// `cinema`, `library` or `music` when the classifier knows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The classifier's class (`video`, `audio`, `reading`, `image`, ...).
    pub class: String,
    /// The owner's title, empty when none.
    pub title: String,
    /// The owner's cover (a key in the same shelf), empty when none.
    pub cover: String,
    /// The owner's note, empty when none.
    pub note: String,
    /// Last change (ISO 8601).
    pub updated_at: String,
}

impl From<CatalogEntry> for CatalogObject {
    fn from(e: CatalogEntry) -> Self {
        Self {
            object_hash: e.object_hash,
            path: e.path,
            size: e.size,
            kind: e.kind,
            class: e.class,
            title: e.owner_title,
            cover: e.owner_cover,
            note: e.owner_note,
            updated_at: iso(e.updated_ms),
        }
    }
}

/// A page of the catalog.
#[derive(Debug, Serialize, ToSchema)]
pub struct CatalogPage {
    /// Objects the caller may open, ordered by hash.
    pub objects: Vec<CatalogObject>,
    /// Cursor of the next page, `null` at the end.
    pub next_cursor: Option<String>,
}

/// Query of `GET /shelves/{shelf_id}/objects`.
#[derive(Debug, Default, Deserialize)]
pub struct CatalogQuery {
    /// The `next_cursor` of the previous page.
    pub cursor: Option<String>,
}

/// A short-lived URL for one object.
#[derive(Debug, Serialize, ToSchema)]
pub struct ObjectUrl {
    /// Presigned `GET`, valid for one object and ten minutes at most. Never stored or logged.
    pub url: String,
    /// When it stops working (ISO 8601).
    pub expires_at: String,
}

/// A grant, live or revoked.
#[derive(Debug, Serialize, ToSchema)]
pub struct GrantView {
    /// Grant snowflake.
    pub id: String,
    /// `friend` or `guild_role`.
    pub grantee_kind: String,
    /// The friend (snowflake) or the role (snowflake).
    pub grantee_id: String,
    /// For `guild_role`: the server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
    /// Who granted it.
    pub granted_by: String,
    /// When (ISO 8601).
    pub created_at: String,
    /// When revoked (ISO 8601), absent while live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<String>,
}

impl From<GrantRow> for GrantView {
    fn from(g: GrantRow) -> Self {
        Self {
            id: g.id.to_string(),
            grantee_kind: g.grantee_kind,
            grantee_id: g.grantee_id.to_string(),
            guild_id: g.guild_id.map(|x| x.to_string()),
            granted_by: g.granted_by.to_string(),
            created_at: iso(g.created_ms),
            revoked_at: g.revoked_ms.map(iso),
        }
    }
}

/// An older per-kind friend share (`PUT /users/@me/storage/mounts/{mount_id}/shares/{user_id}`), which keeps working.
#[derive(Debug, Serialize, ToSchema)]
pub struct LegacyShareView {
    /// The friend.
    pub friend_id: String,
    /// Cinema shared.
    pub cinema: bool,
    /// Library shared.
    pub library: bool,
    /// Music shared.
    pub music: bool,
}

/// Who can see a shelf: the grants with their history, and the older per-kind shares.
#[derive(Debug, Serialize, ToSchema)]
pub struct GrantList {
    /// Newest first, revoked ones included (the audit trail).
    pub grants: Vec<GrantView>,
    /// Per-kind friend shares that predate shelf grants.
    pub legacy_shares: Vec<LegacyShareView>,
}

/// Body of `POST /shelves/{shelf_id}/grants`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct GrantBody {
    /// `friend` or `guild_role`.
    pub grantee_kind: String,
    /// The friend (snowflake) or the role (snowflake).
    pub grantee_id: String,
    /// Required for `guild_role`: the server the role belongs to. You must manage that server.
    #[serde(default)]
    pub guild_id: Option<String>,
}

/// Body of `PATCH /shelves/{shelf_id}/objects/{object_hash}`; absent fields stay.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ObjectMetaBody {
    /// Title (up to 300 characters; empty clears).
    #[serde(default)]
    pub title: Option<String>,
    /// Cover: a key in this shelf (up to 1024 characters; empty clears).
    #[serde(default)]
    pub cover: Option<String>,
    /// Note (up to 2000 characters; empty clears).
    #[serde(default)]
    pub note: Option<String>,
}

fn store_failed(e: sr_storage::scopes::FactsError) -> crate::routes::messages::error::MsgError {
    facts_failed(e)
}

/// The mount, when the caller owns it (a member's own shelf); 404 otherwise, with no hint it exists.
async fn owned(d: &ScopeDeps, user: i64, shelf_id: &str) -> MsgResult<sr_storage::mounts::Mount> {
    let id = parse_row(shelf_id)?;
    match d.mounts.by_id(id).await {
        Ok(Some(m)) if m.owner == MountOwner::User(user) => Ok(m),
        Ok(_) => Err(ErrorCode::SrNotFound.into()),
        Err(e) => {
            tracing::error!(error = %e, "mount lookup failed");
            Err(ErrorCode::SrInternal.into())
        }
    }
}

/// The shelves the caller can see: their own, friends' shelves shared with them and server shelves their role
/// reaches. Each entry is decided by the same function as every read.
#[utoipa::path(
    get,
    path = "/shelves/visible",
    responses((status = 200, body = VisibleShelves), (status = 401, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn visible(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(s): Ext<ShelfAccessDeps>,
) -> MsgResult<Json<VisibleShelves>> {
    let user = uid(&p);
    let facts: &dyn ScopeFacts = &*d.store;
    let ids = s
        .store
        .candidate_shelves(user)
        .await
        .map_err(store_failed)?;
    let mut reached: Vec<(i64, Scope)> = Vec::new();
    for id in ids {
        let Ok(Some(mount)) = d.mounts.by_id(id).await else {
            continue;
        };
        match sr_storage::scopes::access(
            facts,
            Viewer {
                id: user,
                staff: false,
            },
            &mount,
        )
        .await
        {
            Ok(a) if a.unrestricted || a.games || a.kinds.any() => reached.push((id, a.scope)),
            Ok(_) | Err(sr_storage::scopes::ResolveError::Refused(_)) => {}
            Err(sr_storage::scopes::ResolveError::Facts(e)) => return Err(facts_failed(e)),
        }
    }
    let ids: Vec<i64> = reached.iter().map(|r| r.0).collect();
    let labels = s.store.labels(&ids).await.map_err(store_failed)?;
    let guild_ids: Vec<i64> = reached
        .iter()
        .filter_map(|(_, sc)| match sc {
            Scope::Guild(g) => Some(*g),
            _ => None,
        })
        .collect();
    let names = s
        .store
        .guild_names(&guild_ids)
        .await
        .map_err(store_failed)?;
    let rank = |v: &str| match v {
        "own" => 0,
        "friend" => 1,
        _ => 2,
    };
    let mut shelves: Vec<VisibleShelf> = Vec::new();
    for (id, scope) in reached {
        let Some(l) = labels.iter().find(|l| l.shelf_id == id) else {
            continue;
        };
        let (via, guild) = match scope {
            Scope::Own => ("own", None),
            Scope::Friend { .. } => ("friend", None),
            Scope::Guild(g) => ("guild", Some(g)),
            Scope::Staff => continue,
        };
        let user_owned = l.owner_kind == "user";
        shelves.push(VisibleShelf {
            shelf_id: id.to_string(),
            label: l.label.clone(),
            via: via.to_owned(),
            owner_id: user_owned.then(|| l.owner_id.to_string()),
            owner_handle: l.owner_handle.clone(),
            guild_id: guild.map(|g| g.to_string()),
            guild_name: guild.and_then(|g| names.iter().find(|n| n.0 == g).map(|n| n.1.clone())),
            object_count: l.object_count,
            can_manage: via == "own",
        });
    }
    shelves.sort_by_key(|s| rank(&s.via));
    Ok(Json(VisibleShelves { shelves }))
}

/// Per-object check shared by the catalog and the URL: the kind rule of the scope, then every delisting.
async fn object_allowed(
    d: &ScopeDeps,
    s: &ShelfAccessDeps,
    r: &Reach,
    e: &CatalogEntry,
) -> MsgResult<()> {
    r.access.permits(&e.path).map_err(refusal)?;
    if !e.listed || e.delisted {
        return Err(refusal(Refused::Delisted));
    }
    let gone = d
        .store
        .delisted(r.mount.id, std::slice::from_ref(&e.path))
        .await
        .map_err(facts_failed)?;
    if !gone.is_empty()
        || s.store
            .register_delisted(r.mount.id, &e.path)
            .await
            .map_err(store_failed)?
    {
        return Err(refusal(Refused::Delisted));
    }
    Ok(())
}

/// One page of the catalog of a shelf: only objects the caller may open and that nobody delisted.
#[utoipa::path(
    get,
    path = "/shelves/{shelf_id}/objects",
    params(
        ("shelf_id" = String, Path, description = "Shelf (mount) snowflake"),
        ("cursor" = Option<String>, Query, description = "next_cursor of the previous page"),
    ),
    responses(
        (status = 200, body = CatalogPage),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn catalog(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(s): Ext<ShelfAccessDeps>,
    Path(shelf_id): Path<String>,
    ApiQuery(q): ApiQuery<CatalogQuery>,
) -> MsgResult<Json<CatalogPage>> {
    if let Some(c) = &q.cursor
        && !is_object_hash(c)
    {
        return Err(invalid(
            "cursor",
            "CURSOR_INVALID",
            "That cursor is not valid.",
        ));
    }
    let r = reach(&d, None, uid(&p), None, &shelf_id).await?;
    let rows = s
        .store
        .catalog_page(r.mount.id, q.cursor.as_deref(), PAGE + 1)
        .await
        .map_err(store_failed)?;
    let more = rows.len() as i64 > PAGE;
    let mut objects = Vec::new();
    let mut last = None;
    for e in rows.into_iter().take(PAGE as usize) {
        last = Some(e.object_hash.clone());
        if object_allowed(&d, &s, &r, &e).await.is_ok() {
            objects.push(CatalogObject::from(e));
        }
    }
    Ok(Json(CatalogPage {
        objects,
        next_cursor: if more { last } else { None },
    }))
}

/// A presigned `GET` for one object of a shelf. Valid for that object and ten minutes at most (less when the
/// session ends sooner). The grant, the kind rule and every delisting are checked at the moment of minting, so
/// a revoked grant or a delisted object gets nothing; a URL minted earlier lives out its minutes.
#[utoipa::path(
    get,
    path = "/shelves/{shelf_id}/objects/{object_hash}/url",
    params(
        ("shelf_id" = String, Path, description = "Shelf (mount) snowflake"),
        ("object_hash" = String, Path, description = "Object hash from the catalog"),
    ),
    responses(
        (status = 200, body = ObjectUrl),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "CORS_NOT_READY"),
    ),
    security(("session" = []))
)]
pub async fn object_url(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(s): Ext<ShelfAccessDeps>,
    headers: HeaderMap,
    Path((shelf_id, object_hash)): Path<(String, String)>,
) -> MsgResult<Json<ObjectUrl>> {
    if !is_object_hash(&object_hash) {
        return Err(invalid(
            "object_hash",
            "HASH_INVALID",
            "That is not an object hash.",
        ));
    }
    let r = reach(&d, None, uid(&p), None, &shelf_id).await?;
    let e = s
        .store
        .catalog_object(r.mount.id, &object_hash)
        .await
        .map_err(store_failed)?
        .ok_or(ErrorCode::SrNotFound)?;
    object_allowed(&d, &s, &r, &e).await?;
    if r.mount.cors_ok_at.is_none() {
        return Err(super::cors_not_ready());
    }
    let left = session_left(&state, &headers).await;
    let Some(secs) = presign_ttl_secs(URL_TTL_SECS, URL_TTL_SECS, left) else {
        return Err(ErrorCode::Unauthorized.into());
    };
    let storage = storage_for(&d, &r.mount)?;
    let url = storage
        .presign(
            PresignMethod::Get,
            &r.mount.bucket_key(&e.path),
            Duration::from_secs(secs),
        )
        .await
        .map_err(|e| bucket_error(&e))?;
    let expires_ms = state
        .now_secs()
        .saturating_add(secs as i64)
        .saturating_mul(1000);
    Ok(Json(ObjectUrl {
        url,
        expires_at: iso(expires_ms),
    }))
}

/// Edit the title, cover and note of one object (the shelf's owner only).
#[utoipa::path(
    patch,
    path = "/shelves/{shelf_id}/objects/{object_hash}",
    params(
        ("shelf_id" = String, Path, description = "Shelf (mount) snowflake"),
        ("object_hash" = String, Path, description = "Object hash from the catalog"),
    ),
    request_body = ObjectMetaBody,
    responses(
        (status = 200, body = CatalogObject),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn patch_object(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(s): Ext<ShelfAccessDeps>,
    Path((shelf_id, object_hash)): Path<(String, String)>,
    ApiJson(body): ApiJson<ObjectMetaBody>,
) -> MsgResult<Json<CatalogObject>> {
    let m = owned(&d, uid(&p), &shelf_id).await?;
    if !is_object_hash(&object_hash) {
        return Err(invalid(
            "object_hash",
            "HASH_INVALID",
            "That is not an object hash.",
        ));
    }
    let long = |v: &Option<String>, max: usize| v.as_ref().is_some_and(|t| t.chars().count() > max);
    if long(&body.title, 300) {
        return Err(invalid(
            "title",
            "BASE_TYPE_MAX_LENGTH",
            "Up to 300 characters.",
        ));
    }
    if long(&body.cover, 1024) {
        return Err(invalid(
            "cover",
            "BASE_TYPE_MAX_LENGTH",
            "Up to 1024 characters.",
        ));
    }
    if long(&body.note, 2000) {
        return Err(invalid(
            "note",
            "BASE_TYPE_MAX_LENGTH",
            "Up to 2000 characters.",
        ));
    }
    if let Some(c) = body.cover.as_deref().filter(|c| !c.is_empty()) {
        sr_storage::path::safe_key(c, false).map_err(|e| super::objects::key_error("cover", e))?;
    }
    let found = s
        .store
        .set_owner_meta(
            m.id,
            &object_hash,
            body.title.as_deref().map(str::trim),
            body.cover.as_deref().map(str::trim),
            body.note.as_deref().map(str::trim),
        )
        .await
        .map_err(store_failed)?;
    if !found {
        return Err(ErrorCode::SrNotFound.into());
    }
    let e = s
        .store
        .catalog_object(m.id, &object_hash)
        .await
        .map_err(store_failed)?
        .ok_or(ErrorCode::SrNotFound)?;
    Ok(Json(e.into()))
}

/// Who can see a shelf: its grants with history (the audit trail) and the older per-kind shares. The owner only.
#[utoipa::path(
    get,
    path = "/shelves/{shelf_id}/grants",
    params(("shelf_id" = String, Path, description = "Shelf (mount) snowflake")),
    responses((status = 200, body = GrantList), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list_grants(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(s): Ext<ShelfAccessDeps>,
    Path(shelf_id): Path<String>,
) -> MsgResult<Json<GrantList>> {
    let m = owned(&d, uid(&p), &shelf_id).await?;
    let grants = s.store.grants_of(m.id).await.map_err(store_failed)?;
    let legacy = d.store.shares_of_mount(m.id).await.map_err(facts_failed)?;
    Ok(Json(GrantList {
        grants: grants.into_iter().map(Into::into).collect(),
        legacy_shares: legacy
            .into_iter()
            .map(|(f, sh)| LegacyShareView {
                friend_id: f.to_string(),
                cinema: sh.cinema,
                library: sh.library,
                music: sh.music,
            })
            .collect(),
    }))
}

/// Grant a friend, or a server role you manage, read access to the whole shelf. The grant is tied to the account,
/// so it works on every device, and is written to the audit log.
#[utoipa::path(
    post,
    path = "/shelves/{shelf_id}/grants",
    params(("shelf_id" = String, Path, description = "Shelf (mount) snowflake")),
    request_body = GrantBody,
    responses(
        (status = 201, body = GrantView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn create_grant(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(s): Ext<ShelfAccessDeps>,
    Path(shelf_id): Path<String>,
    ApiJson(body): ApiJson<GrantBody>,
) -> MsgResult<(StatusCode, Json<GrantView>)> {
    let user = uid(&p);
    let m = owned(&d, user, &shelf_id).await?;
    let grantee = parse_row(&body.grantee_id)?;
    let facts: &dyn ScopeFacts = &*d.store;
    let (kind, guild) = match body.grantee_kind.as_str() {
        "friend" => {
            if grantee == user
                || !facts
                    .are_friends(user, grantee)
                    .await
                    .map_err(facts_failed)?
                || !s
                    .store
                    .account_active(grantee)
                    .await
                    .map_err(store_failed)?
            {
                return Err(invalid(
                    "grantee_id",
                    "NOT_FRIENDS",
                    "You can only grant a friend access.",
                ));
            }
            ("friend", None)
        }
        "guild_role" => {
            let Some(g) = body.guild_id.as_deref() else {
                return Err(invalid(
                    "guild_id",
                    "BASE_TYPE_REQUIRED",
                    "A role grant names its server.",
                ));
            };
            let gid = parse_row(g)?;
            let viewer = Viewer {
                id: user,
                staff: false,
            };
            if !sr_storage::scopes::may_manage_guild(facts, viewer, gid)
                .await
                .map_err(facts_failed)?
            {
                return Err(ErrorCode::MissingPermissions.into());
            }
            if !d
                .store
                .role_in_guild(gid, grantee)
                .await
                .map_err(facts_failed)?
            {
                return Err(invalid(
                    "grantee_id",
                    "ROLE_UNKNOWN",
                    "That role is not in this server.",
                ));
            }
            ("guild_role", Some(gid))
        }
        _ => {
            return Err(invalid(
                "grantee_kind",
                "ENUM_TYPE_COERCE",
                "Use friend or guild_role.",
            ));
        }
    };
    let row = GrantRow {
        id: 0,
        shelf_id: m.id,
        grantee_kind: kind.to_owned(),
        grantee_id: grantee,
        guild_id: guild,
        granted_by: user,
        created_ms: 0,
        revoked_ms: None,
    };
    match s.store.create_grant(&row).await {
        Ok(g) => Ok((StatusCode::CREATED, Json(g.into()))),
        Err(GrantError::Duplicate) => Err(super::mounts::conflict(
            "ALREADY_GRANTED",
            "That friend or role can already see this shelf.",
        )),
        Err(GrantError::Missing) => Err(ErrorCode::SrNotFound.into()),
    }
}

/// Revoke a grant. It takes effect at the next URL mint; a URL minted earlier lives out its ten minutes.
#[utoipa::path(
    delete,
    path = "/shelves/{shelf_id}/grants/{shelf_grant_id}",
    params(
        ("shelf_id" = String, Path, description = "Shelf (mount) snowflake"),
        ("shelf_grant_id" = String, Path, description = "Grant snowflake"),
    ),
    responses((status = 204), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn revoke_grant(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(s): Ext<ShelfAccessDeps>,
    MaybeExt(_admin): MaybeExt<AdminDeps>,
    Path((shelf_id, grant_id)): Path<(String, String)>,
) -> MsgResult<StatusCode> {
    let user = uid(&p);
    let m = owned(&d, user, &shelf_id).await?;
    let gid = parse_row(&grant_id)?;
    if s.store
        .revoke_grant(m.id, gid, user)
        .await
        .map_err(store_failed)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ErrorCode::SrNotFound.into())
    }
}
