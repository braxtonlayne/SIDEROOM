//! The Server Directory (LNG-GLD-013, D-012): the platform-internal list of Community servers
//! that staff approved, visible only to logged-in members.
//!
//! * `GET /directory/categories`, `GET /directory` (search), `GET /directory/{guild_id}`,
//!   `POST /directory/{guild_id}/join`: for every signed-in member.
//! * `GET|PUT|DELETE /guilds/{guild_id}/directory`: a server's managers (`MANAGE_GUILD`) ask
//!   to be listed, watch the state and withdraw.
//! * `GET /admin/directory`, `PATCH /admin/directory/{guild_id}`: staff with
//!   `MANAGE_DIRECTORY` work the queue; decisions are written to the platform access log.
//!
//! Wiring: register [`DirectoryDeps`] next to the settings dependencies; a listing's server
//! data comes from the chat repository inside [`SettingsDeps`].

use crate::routes::admin::{StaffCtx, map_err as admin_err};
use crate::routes::guilds::dto::{MemberObject, iso8601};
use crate::routes::guilds::util::{actor, id};
use crate::routes::messages::error::{ApiJson, ApiQuery};
use crate::routes::settings::deps::{SettingsDeps, re, se};
use crate::routes::{Ext, MaybeExt};
use crate::{HttpError, Principal, error::ApiResult, openapi::ErrorBody, state::AppState};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sr_chat::settings::directory::{
    self, CATEGORIES, Decision, DirectoryEntry, DirectoryRepository, DirectoryStatus,
};
use sr_chat::settings::repo::CommunityRepository;
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ErrorCode, GuildId, PlatformPermissions as P};
use sr_features::{FeatureService, HIDDEN_CATEGORIES_ID, ScopeRef, hidden_from};
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};

/// Collaborators of the directory endpoints.
pub struct DirectoryDeps {
    pub repo: Arc<dyn DirectoryRepository>,
    features: Option<FeatureService>,
}

impl DirectoryDeps {
    pub fn new(repo: Arc<dyn DirectoryRepository>) -> Self {
        Self {
            repo,
            features: None,
        }
    }

    /// Read the `directory.hidden_categories` platform setting from the feature service.
    pub fn with_features(mut self, features: FeatureService) -> Self {
        self.features = Some(features);
        self
    }

    /// Category ids the operator hides (G-29). A failing lookup hides nothing.
    pub async fn hidden_categories(&self) -> Vec<String> {
        let Some(f) = &self.features else {
            return Vec::new();
        };
        match f
            .effective_settings(HIDDEN_CATEGORIES_ID, &ScopeRef::Platform)
            .await
        {
            Ok(v) => hidden_from(&v),
            Err(_) => Vec::new(),
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/sr/emojis/{emoji_id}", get(get_emoji_info))
        .route("/sr/emojis/{emoji_id}/url", get(get_emoji_url))
        .route("/directory/categories", get(list_categories))
        .route("/directory", get(search))
        .route("/directory/{guild_id}", get(get_listed))
        .route("/directory/{guild_id}/join", post(join))
        .route(
            "/guilds/{guild_id}/directory",
            get(get_own_listing)
                .put(request_listing)
                .delete(withdraw_listing),
        )
        .route("/admin/directory", get(admin_list))
        .route(
            "/admin/directory/{guild_id}",
            axum::routing::patch(admin_decide),
        )
}

// ------------------------------------------------------------------ objects

/// A server card in the directory.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DirectoryGuildObject {
    pub id: String,
    pub name: String,
    /// The listing's blurb, else the server description.
    pub description: Option<String>,
    pub icon: Option<String>,
    pub banner: Option<String>,
    pub splash: Option<String>,
    pub category: String,
    /// Staff pinned this server.
    pub featured: bool,
    pub approximate_member_count: u64,
    /// Visibly online members; only while the gateway can tell (small servers).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
    pub preferred_locale: String,
    pub vanity_url_code: Option<String>,
    /// The caller already is a member.
    pub joined: bool,
}

/// A server's own listing request.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ListingObject {
    pub guild_id: String,
    /// `pending`, `approved`, `rejected` or `removed`.
    pub status: &'static str,
    pub category: String,
    pub description: String,
    pub featured: bool,
    pub requested_at: String,
    pub decided_at: Option<String>,
    /// Staff's note on the decision.
    pub decision_note: Option<String>,
}

impl From<&DirectoryEntry> for ListingObject {
    fn from(e: &DirectoryEntry) -> Self {
        Self {
            guild_id: e.guild_id.to_string(),
            status: e.status.as_str(),
            category: e.category.clone(),
            description: e.description.clone(),
            featured: e.featured,
            requested_at: iso8601(e.requested_at_ms),
            decided_at: e.decided_at_ms.map(iso8601),
            decision_note: e.decision_note.clone(),
        }
    }
}

/// A listing in the staff queue.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AdminListingObject {
    #[serde(flatten)]
    pub listing: ListingObject,
    pub guild_name: Option<String>,
    pub requested_by: String,
    pub approximate_member_count: u64,
    /// Community is still on; a listing of a server that switched it off stays hidden.
    pub community_enabled: bool,
}

// ------------------------------------------------------------------ helpers

async fn card(
    deps: &SettingsDeps,
    entry: &DirectoryEntry,
    viewer: sr_core::UserId,
) -> ApiResult<Option<DirectoryGuildObject>> {
    let community = deps
        .service
        .repo()
        .get_community(entry.guild_id)
        .await
        .map_err(re)?;
    if !directory::is_visible(entry, community.community_enabled) {
        return Ok(None);
    }
    let Some(g) = deps.chat.get_guild(entry.guild_id).await.map_err(re)? else {
        return Ok(None);
    };
    let members = deps.chat.count_members(g.id).await.map_err(re)?;
    let joined = deps
        .chat
        .get_member(g.id, viewer)
        .await
        .map_err(re)?
        .is_some();
    let presence = match deps.presence() {
        Some(p) if members <= 1000 => {
            let page = deps.chat.page_members(g.id, None, 1000).await.map_err(re)?;
            let ids: Vec<_> = page.iter().map(|m| m.user_id).collect();
            Some(p.count_online(&ids))
        }
        _ => None,
    };
    Ok(Some(DirectoryGuildObject {
        id: g.id.to_string(),
        name: g.name.clone(),
        description: Some(entry.description.clone())
            .filter(|d| !d.is_empty())
            .or(g.description.clone()),
        icon: g.icon_key.clone(),
        banner: g.banner_key.clone(),
        splash: g.splash_key.clone(),
        category: entry.category.clone(),
        featured: entry.featured,
        approximate_member_count: members,
        approximate_presence_count: presence,
        preferred_locale: g.preferred_locale.clone(),
        vanity_url_code: g.vanity_code.clone(),
        joined,
    }))
}

fn need(perms: Permissions, flag: Permissions) -> ApiResult<()> {
    if perms.has(flag) {
        Ok(())
    } else {
        Err(ErrorCode::MissingPermissions.into())
    }
}

fn unknown_listing() -> HttpError {
    HttpError(ApiError::with_message(
        ErrorCode::UnknownGuild,
        "This server is not listed in the directory.",
    ))
}

// ------------------------------------------------------------------ emoji popout

/// What the emoji popout shows (D-232).
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EmojiInfoObject {
    pub id: String,
    pub name: String,
    pub animated: bool,
    /// The owning server, only when the caller is a member or it has an approved Server Directory
    /// listing; otherwise `null` and the popout says the emoji is from a private server.
    pub guild: Option<EmojiGuildObject>,
}

/// The server an emoji belongs to.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EmojiGuildObject {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    /// The id `POST /directory/{id}/join` takes, when the server is listed; `null` for a member's
    /// own unlisted server.
    pub listing: Option<String>,
}

async fn find_emoji(
    deps: &SettingsDeps,
    emoji_id: &str,
) -> ApiResult<sr_chat::settings::expressions::Emoji> {
    use sr_chat::settings::repo::ExpressionRepository;
    let eid: sr_core::EmojiId = id("emoji_id", emoji_id)?;
    deps.service
        .repo()
        .find_emoji(eid)
        .await
        .map_err(re)?
        .ok_or_else(|| ErrorCode::UnknownEmoji.into())
}

/// An emoji by id, for any signed-in caller (the popout on a cross-server custom emoji, D-232).
#[utoipa::path(
    get,
    path = "/sr/emojis/{emoji_id}",
    params(("emoji_id" = String, Path, description = "Emoji snowflake")),
    responses((status = 200, body = EmojiInfoObject), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_emoji_info(
    p: Principal,
    Ext(dir): Ext<DirectoryDeps>,
    Ext(deps): Ext<SettingsDeps>,
    Path(emoji_id): Path<String>,
) -> ApiResult<Json<EmojiInfoObject>> {
    let user = actor(&p)?;
    let emoji = find_emoji(&deps, &emoji_id).await?;
    let joined = deps
        .chat
        .get_member(emoji.guild_id, user)
        .await
        .map_err(re)?
        .is_some();
    let listed = match dir.repo.get_listing(emoji.guild_id).await.map_err(re)? {
        Some(entry) if !dir.hidden_categories().await.contains(&entry.category) => {
            let community = deps
                .service
                .repo()
                .get_community(entry.guild_id)
                .await
                .map_err(re)?;
            directory::is_visible(&entry, community.community_enabled)
        }
        _ => false,
    };
    let guild = if joined || listed {
        deps.chat
            .get_guild(emoji.guild_id)
            .await
            .map_err(re)?
            .map(|g| EmojiGuildObject {
                id: g.id.to_string(),
                name: g.name.clone(),
                icon: g.icon_key.clone(),
                listing: listed.then(|| g.id.to_string()),
            })
    } else {
        None
    };
    Ok(Json(EmojiInfoObject {
        id: emoji.id.to_string(),
        name: emoji.name,
        animated: emoji.animated,
        guild,
    }))
}

/// A short-lived signed CDN URL for an emoji image, for any signed-in caller: custom emoji are
/// drawable wherever they appear (D-232).
#[utoipa::path(
    get,
    path = "/sr/emojis/{emoji_id}/url",
    params(
        ("emoji_id" = String, Path, description = "Emoji snowflake"),
        ("size" = Option<u32>, Query, description = "Power of two, 16 to 4096"),
        ("format" = Option<String>, Query, description = "png, jpg, webp or gif"),
    ),
    responses((status = 200, body = crate::routes::settings::expressions::SignedUrlObject),
              (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_emoji_url(
    p: Principal,
    State(state): State<AppState>,
    Ext(deps): Ext<SettingsDeps>,
    Ext(cdn): Ext<crate::routes::cdn::CdnDeps>,
    Path(emoji_id): Path<String>,
    axum::extract::Query(q): axum::extract::Query<crate::routes::settings::expressions::UrlQuery>,
) -> ApiResult<Json<crate::routes::settings::expressions::SignedUrlObject>> {
    actor(&p)?;
    let emoji = find_emoji(&deps, &emoji_id).await?;
    Ok(Json(
        crate::routes::settings::expressions::signed_media_url(&state, &cdn, &emoji.media_key, &q)?,
    ))
}

// ------------------------------------------------------------------ member side

/// The categories a listing may use.
#[utoipa::path(
    get,
    path = "/directory/categories",
    responses((status = 200, body = Vec<String>), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn list_categories(
    p: Principal,
    Ext(dir): Ext<DirectoryDeps>,
) -> ApiResult<Json<Vec<&'static str>>> {
    actor(&p)?;
    let hidden = dir.hidden_categories().await;
    Ok(Json(
        CATEGORIES
            .iter()
            .copied()
            .filter(|c| !hidden.iter().any(|h| h == c))
            .collect(),
    ))
}

/// Query of `GET /directory`.
#[derive(Debug, Default, Deserialize)]
pub struct SearchQuery {
    /// Words that must all appear in the server name or description (case-insensitive).
    pub query: Option<String>,
    pub category: Option<String>,
    /// 1 to 50, default 24.
    pub limit: Option<usize>,
    /// Entries to skip, default 0.
    pub offset: Option<usize>,
}

/// Search the directory: approved listings of servers that still are Community, featured
/// first, then by member count. Visible to every signed-in member (D-012).
#[utoipa::path(
    get,
    path = "/directory",
    params(
        ("query" = Option<String>, Query, description = "Words in the name or description"),
        ("category" = Option<String>, Query, description = "One of GET /directory/categories"),
        ("limit" = Option<usize>, Query, description = "1 to 50, default 24"),
        ("offset" = Option<usize>, Query, description = "Entries to skip"),
    ),
    responses((status = 200, body = Vec<DirectoryGuildObject>), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn search(
    p: Principal,
    Ext(dir): Ext<DirectoryDeps>,
    Ext(deps): Ext<SettingsDeps>,
    ApiQuery(q): ApiQuery<SearchQuery>,
) -> ApiResult<Json<Vec<DirectoryGuildObject>>> {
    let user = actor(&p)?;
    let hidden = dir.hidden_categories().await;
    let category = match q
        .category
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        Some(c) if CATEGORIES.contains(&c) && !hidden.iter().any(|h| h == c) => Some(c),
        Some(_) => {
            return Err(HttpError(ApiError::invalid_form().field(
                "category",
                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                "Not one of the directory categories.",
            )));
        }
        None => None,
    };
    let terms: Vec<String> = q
        .query
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .take(8)
        .map(str::to_lowercase)
        .collect();
    let entries = dir
        .repo
        .list_listings(Some(DirectoryStatus::Approved), category)
        .await
        .map_err(re)?;
    let mut cards = Vec::new();
    for e in &entries {
        if hidden.contains(&e.category) {
            continue;
        }
        if let Some(c) = card(&deps, e, user).await? {
            let hay = format!(
                "{} {} {}",
                c.name,
                c.description.as_deref().unwrap_or(""),
                e.description
            )
            .to_lowercase();
            if terms.iter().all(|t| hay.contains(t.as_str())) {
                cards.push(c);
            }
        }
    }
    cards.sort_by(|a, b| {
        b.featured
            .cmp(&a.featured)
            .then(b.approximate_member_count.cmp(&a.approximate_member_count))
            .then(a.id.len().cmp(&b.id.len()))
            .then(a.id.cmp(&b.id))
    });
    let offset = q.offset.unwrap_or(0);
    let limit = q.limit.unwrap_or(24).clamp(1, 50);
    Ok(Json(cards.into_iter().skip(offset).take(limit).collect()))
}

/// One listed server (the card behind a preview). 404 unless it is approved and Community.
#[utoipa::path(
    get,
    path = "/directory/{guild_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = DirectoryGuildObject), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_listed(
    p: Principal,
    Ext(dir): Ext<DirectoryDeps>,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<DirectoryGuildObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let entry = dir
        .repo
        .get_listing(gid)
        .await
        .map_err(re)?
        .ok_or_else(unknown_listing)?;
    if dir.hidden_categories().await.contains(&entry.category) {
        return Err(unknown_listing());
    }
    card(&deps, &entry, user)
        .await?
        .map(Json)
        .ok_or_else(unknown_listing)
}

/// Join a listed server straight from the directory, without an invite. Banned users,
/// servers with invites paused and unlisted servers answer 403 or 404 like an invite would.
/// A server with membership screening admits the member as `pending`.
#[utoipa::path(
    post,
    path = "/directory/{guild_id}/join",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = MemberObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn join(
    crate::auth::UserOnly(p): crate::auth::UserOnly,
    Ext(dir): Ext<DirectoryDeps>,
    Ext(deps): Ext<SettingsDeps>,
    MaybeExt(messages): MaybeExt<crate::routes::messages::MessagesDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<MemberObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let entry = dir
        .repo
        .get_listing(gid)
        .await
        .map_err(re)?
        .ok_or_else(unknown_listing)?;
    if dir.hidden_categories().await.contains(&entry.category) {
        return Err(unknown_listing());
    }
    if card(&deps, &entry, user).await?.is_none() {
        return Err(unknown_listing());
    }
    let joined = deps
        .joiner()
        .join_listed(gid, user, deps.now_ms())
        .await
        .map_err(HttpError)?;
    let member = match joined {
        Some(m) => {
            if let Some(msgs) = &messages {
                crate::routes::messages::system::post_user_join(msgs, gid, user).await;
            }
            m
        }
        None => deps
            .chat
            .get_member(gid, user)
            .await
            .map_err(re)?
            .ok_or(ErrorCode::UnknownGuild)?,
    };
    let users = deps.users([user]).await?;
    Ok(Json(MemberObject::new(&member, false, &users)))
}

// ------------------------------------------------------------------ manager side

/// The listing of the caller's server and where it stands (`MANAGE_GUILD`). 404 when the
/// server never asked.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/directory",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = ListingObject), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn get_own_listing(
    p: Principal,
    Ext(dir): Ext<DirectoryDeps>,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<Json<ListingObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    need(
        deps.guild_access(user, gid).await?.perms,
        Permissions::MANAGE_GUILD,
    )?;
    let entry = dir
        .repo
        .get_listing(gid)
        .await
        .map_err(re)?
        .ok_or_else(unknown_listing)?;
    Ok(Json(ListingObject::from(&entry)))
}

/// Body of `PUT /guilds/{id}/directory`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ListingBody {
    /// One of `GET /directory/categories`.
    pub category: String,
    /// The card's blurb, up to 300 characters; empty falls back to the server description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Ask to be listed in the directory, or edit the request (`MANAGE_GUILD`). The server needs
/// Community; a new request waits for staff, and changing an approved listing sends it back for
/// review (D-012).
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/directory",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = ListingBody,
    responses((status = 200, body = ListingObject), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn request_listing(
    p: Principal,
    Ext(dir): Ext<DirectoryDeps>,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<ListingBody>,
) -> ApiResult<Json<ListingObject>> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    need(
        deps.guild_access(user, gid).await?.perms,
        Permissions::MANAGE_GUILD,
    )?;
    if dir.hidden_categories().await.contains(&body.category) {
        return Err(HttpError(ApiError::invalid_form().field(
            "category",
            "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
            "That category is not available in the directory.",
        )));
    }
    let community = deps.service.repo().get_community(gid).await.map_err(re)?;
    let existing = dir.repo.get_listing(gid).await.map_err(re)?;
    let entry = directory::request_listing(
        existing,
        gid,
        user,
        &body.category,
        body.description.as_deref().unwrap_or(""),
        community.community_enabled,
        deps.now_ms(),
    )
    .map_err(se)?;
    dir.repo.put_listing(&entry).await.map_err(re)?;
    Ok(Json(ListingObject::from(&entry)))
}

/// Take the server out of the directory, or cancel the request (`MANAGE_GUILD`).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/directory",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 204, description = "Withdrawn"), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("authorization" = []), ("session" = []))
)]
pub async fn withdraw_listing(
    p: Principal,
    Ext(dir): Ext<DirectoryDeps>,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
) -> ApiResult<StatusCode> {
    let user = actor(&p)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    need(
        deps.guild_access(user, gid).await?.perms,
        Permissions::MANAGE_GUILD,
    )?;
    if !dir.repo.delete_listing(gid).await.map_err(re)? {
        return Err(unknown_listing());
    }
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ staff side

/// Query of `GET /admin/directory`.
#[derive(Debug, Default, Deserialize)]
pub struct AdminListQuery {
    /// `pending` (default), `approved`, `rejected` or `removed`.
    pub status: Option<String>,
}

/// The directory queue for staff with `MANAGE_DIRECTORY`, oldest request first.
#[utoipa::path(
    get,
    path = "/admin/directory",
    params(("status" = Option<String>, Query, description = "pending (default), approved, rejected or removed")),
    responses((status = 200, body = Vec<AdminListingObject>), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_list(
    ctx: StaffCtx,
    Ext(dir): Ext<DirectoryDeps>,
    Ext(deps): Ext<SettingsDeps>,
    ApiQuery(q): ApiQuery<AdminListQuery>,
) -> ApiResult<Json<Vec<AdminListingObject>>> {
    ctx.svc()
        .authorize(&ctx.caller, P::MANAGE_DIRECTORY, "directory.list")
        .await
        .map_err(admin_err)?;
    let status = match q.status.as_deref() {
        None => DirectoryStatus::Pending,
        Some(s) => DirectoryStatus::parse(s).ok_or_else(|| {
            HttpError(ApiError::invalid_form().field(
                "status",
                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                "Must be pending, approved, rejected or removed.",
            ))
        })?,
    };
    let entries = dir
        .repo
        .list_listings(Some(status), None)
        .await
        .map_err(re)?;
    let mut out = Vec::with_capacity(entries.len());
    for e in &entries {
        let guild = deps.chat.get_guild(e.guild_id).await.map_err(re)?;
        let community = deps
            .service
            .repo()
            .get_community(e.guild_id)
            .await
            .map_err(re)?;
        out.push(AdminListingObject {
            listing: ListingObject::from(e),
            guild_name: guild.map(|g| g.name),
            requested_by: e.requested_by.to_string(),
            approximate_member_count: deps.chat.count_members(e.guild_id).await.map_err(re)?,
            community_enabled: community.community_enabled,
        });
    }
    Ok(Json(out))
}

/// Body of `PATCH /admin/directory/{guild_id}`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct DecisionBody {
    /// `approve`, `reject` or `remove`.
    pub decision: String,
    /// Pin the server to the top (approved listings only).
    pub featured: Option<bool>,
    /// A note to the server's managers, up to 500 characters.
    pub note: Option<String>,
}

/// Decide a listing: approve or reject a request, take an approved listing down, and
/// feature approved ones (`MANAGE_DIRECTORY`). The decision is written to the platform access
/// log.
#[utoipa::path(
    patch,
    path = "/admin/directory/{guild_id}",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = DecisionBody,
    responses((status = 200, body = ListingObject), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn admin_decide(
    ctx: StaffCtx,
    Ext(dir): Ext<DirectoryDeps>,
    Ext(deps): Ext<SettingsDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<DecisionBody>,
) -> ApiResult<Json<ListingObject>> {
    ctx.svc()
        .authorize(&ctx.caller, P::MANAGE_DIRECTORY, "directory.decide")
        .await
        .map_err(admin_err)?;
    let gid: GuildId = id("guild_id", &guild_id)?;
    let decision = match body.decision.as_str() {
        "approve" => Decision::Approve,
        "reject" => Decision::Reject,
        "remove" => Decision::Remove,
        _ => {
            return Err(HttpError(ApiError::invalid_form().field(
                "decision",
                "ENUM_TYPE_BASE_TYPE_BAD_VALUE",
                "Must be approve, reject or remove.",
            )));
        }
    };
    let entry = dir
        .repo
        .get_listing(gid)
        .await
        .map_err(re)?
        .ok_or_else(unknown_listing)?;
    let staff = sr_core::UserId::new(u64::try_from(ctx.caller.user_id).unwrap_or(0));
    let decided = directory::decide(
        entry,
        decision,
        body.featured,
        body.note.as_deref(),
        staff,
        deps.now_ms(),
    )
    .map_err(se)?;
    dir.repo.put_listing(&decided).await.map_err(re)?;
    if let Err(e) = ctx
        .svc()
        .record_action(
            &ctx.caller,
            "directory.decide",
            Some(("guild", i64::try_from(gid.get()).unwrap_or(0))),
            json!({
                "decision": body.decision,
                "status": decided.status.as_str(),
                "featured": decided.featured,
                "note": decided.decision_note,
            }),
        )
        .await
    {
        tracing::warn!(error = %e, "directory decision was not written to the access log");
    }
    Ok(Json(ListingObject::from(&decided)))
}

/// OpenAPI fragment for these routes; merge into [`crate::openapi::base`].
#[derive(OpenApi)]
#[openapi(
    paths(
        list_categories,
        search,
        get_listed,
        get_emoji_info,
        get_emoji_url,
        join,
        get_own_listing,
        request_listing,
        withdraw_listing,
        admin_list,
        admin_decide,
    ),
    components(schemas(
        DirectoryGuildObject,
        EmojiInfoObject,
        EmojiGuildObject,
        ListingObject,
        AdminListingObject,
        ListingBody,
        DecisionBody,
    ))
)]
pub struct DirectoryApiDoc;

pub fn openapi() -> utoipa::openapi::OpenApi {
    DirectoryApiDoc::openapi()
}
