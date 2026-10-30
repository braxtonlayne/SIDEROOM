//! The scrapbook vault routes (PRF-SCR-001, module `lounge.scrapbook`, migration 1690).
//!
//! * `GET /guilds/{id}/scrapbook/status`: whether the module is offered and on, what the viewer may do, the
//!   shelf and its health, and (managers) how many bucket copies could not be removed.
//! * `GET /guilds/{id}/scrapbook`: the gallery, filterable by date, channel, tag and member.
//! * `PUT /channels/{id}/messages/{id}/scrapbook`: "Send to Scrapbook".
//! * `PATCH|DELETE /guilds/{id}/scrapbook/{entry}`: tags; remove (the cascade).
//! * `POST /guilds/{id}/scrapbook/{entry}/objects/{object}/presign`: a presigned `GET` for one copy.
//! * `PUT|DELETE /guilds/{id}/scrapbook/target`: choose or drop the shelf (the delegated `upload` grant).
//! * `GET /guilds/{id}/scrapbook/removals`, `POST .../{removal}/retry`, `DELETE .../{removal}`: the copies
//!   that could not be removed from a bucket, visible to the server's managers.
//!
//! Who sees what: an entry is listed to a member only while they hold `VIEW_CHANNEL` of its source channel
//! (the same check as reading the channel). The files are a separate question: each copy is signed only for a
//! viewer whose own storage scopes reach the shelf (`sr_storage::scopes`: a role with a view grant for the
//! file's kind), so a member without the grant sees the text and "not shared with you" in place of the file.
//! The module is hidden (404) until the operator enables it at platform scope, and 403 `SR_FEATURE_DISABLED`
//! while the server has it off. Removing an entry is allowed whatever the module's state: data about a
//! person must stay deletable.

use crate::AppState;
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::admin::AdminDeps;
use crate::routes::messages::error::{ApiJson, MsgError, MsgResult, invalid};
use crate::routes::settings::deps::{GuildAccess, SettingsDeps};
use crate::routes::settings::features::GuildFeatures;
use crate::routes::storage::objects::{PresignBody, PresignView};
use crate::routes::storage::{ScopeDeps, uid};
use crate::routes::{Ext, MaybeExt};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{delete, get, post, put},
};
use serde::{Deserialize, Serialize};
use sr_chat::repo::{ChatRepository, MessageStore};
use sr_chat::{ChannelType, MessageType, perms::channel_permissions};
use sr_core::permissions::Permissions;
use sr_core::{ApiError, ChannelId, ErrorCode, GuildId, MessageId, UserId};
use sr_features::modules::scrapbook::{SCRAPBOOK_ID, ScrapbookConfig};
use sr_features::{FeatureService, ScopeRef};
use sr_storage::mounts::{MountOwner, MountStatus};
use sr_storage::scopes::{ScopeFacts, Viewer};
use sr_storage::scrapbook::{
    ArchiveError, ArchiveOutcome, ArchiveRequest, Archiver, DeleteScope, EntryWithObjects, Filter,
    Object, Refusal, SourceAttachment, SourceMessage, Target, Via, clean_tags,
};
use std::collections::HashMap;
use std::sync::Arc;
use utoipa::ToSchema;

/// Largest gallery page.
const MAX_PAGE: i64 = 50;
/// Default gallery page.
const DEFAULT_PAGE: i64 = 24;

/// Collaborators of the scrapbook routes.
pub struct ScrapbookDeps {
    /// Archives and cascades.
    pub archiver: Arc<Archiver>,
    /// Attachments of messages.
    pub messages: Arc<dyn MessageStore>,
}

/// The routes, relative to the API prefix.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/guilds/{guild_id}/scrapbook", get(list))
        .route("/guilds/{guild_id}/scrapbook/status", get(status))
        .route(
            "/guilds/{guild_id}/scrapbook/target",
            put(set_target).delete(clear_target),
        )
        .route("/guilds/{guild_id}/scrapbook/removals", get(removals))
        .route(
            "/guilds/{guild_id}/scrapbook/removals/{removal_id}",
            delete(dismiss_removal),
        )
        .route(
            "/guilds/{guild_id}/scrapbook/removals/{removal_id}/retry",
            post(retry_removal),
        )
        .route(
            "/guilds/{guild_id}/scrapbook/{entry_id}",
            axum::routing::patch(edit_entry).delete(remove_entry),
        )
        .route(
            "/guilds/{guild_id}/scrapbook/{entry_id}/objects/{object_id}/presign",
            post(presign),
        )
        .route(
            "/channels/{channel_id}/messages/{message_id}/scrapbook",
            put(send),
        )
}

// ---- wire types ------------------------------------------------------------------------------------

/// One copied file of an entry.
#[derive(Debug, Serialize, ToSchema)]
pub struct ScrapbookObjectView {
    /// Snowflake.
    pub id: String,
    /// The file name the author gave it.
    pub filename: String,
    /// Content type, when known.
    pub content_type: String,
    /// Size in bytes.
    pub size: i64,
    /// `image`, `video`, `audio` or `file`.
    pub kind: String,
    /// The viewer's storage scopes reach this file now. `false` shows "not shared with you"; the text of the
    /// entry is unaffected.
    pub playable: bool,
}

/// One archived message.
#[derive(Debug, Serialize, ToSchema)]
pub struct ScrapbookEntryView {
    /// Snowflake.
    pub id: String,
    /// The channel the message was sent in.
    pub channel_id: String,
    /// The message.
    pub message_id: String,
    /// Its author.
    pub author_id: String,
    /// The author's name when it was archived.
    pub author_name: String,
    /// The message text.
    pub content: String,
    /// When the message was sent, unix ms.
    pub sent_at: i64,
    /// Tags.
    pub tags: Vec<String>,
    /// `reaction` or `manual`.
    pub via: String,
    /// Who sent it by hand; absent for a reaction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_by: Option<String>,
    /// Attachments that were not copied (too large or past the limit).
    pub skipped: i32,
    /// The copied files.
    pub objects: Vec<ScrapbookObjectView>,
    /// The viewer may change tags and remove the entry.
    pub can_edit: bool,
}

/// A gallery page and the filter choices.
#[derive(Debug, Serialize, ToSchema)]
pub struct ScrapbookPage {
    /// Entries, newest first.
    pub entries: Vec<ScrapbookEntryView>,
    /// Pass as `before` for the next page; absent on the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_before: Option<String>,
    /// Filter choices over what the viewer may see; present when `facets=true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facets: Option<ScrapbookFacets>,
}

/// Filter choices.
#[derive(Debug, Serialize, ToSchema)]
pub struct ScrapbookFacets {
    /// Tags with the number of entries.
    pub tags: Vec<FacetTag>,
    /// Members with the number of entries.
    pub authors: Vec<FacetAuthor>,
    /// Channels with the number of entries.
    pub channels: Vec<FacetChannel>,
}

/// A tag choice.
#[derive(Debug, Serialize, ToSchema)]
pub struct FacetTag {
    /// The tag.
    pub tag: String,
    /// Entries carrying it.
    pub count: i64,
}

/// A member choice.
#[derive(Debug, Serialize, ToSchema)]
pub struct FacetAuthor {
    /// Member snowflake.
    pub id: String,
    /// Name when archived.
    pub name: String,
    /// Entries.
    pub count: i64,
}

/// A channel choice.
#[derive(Debug, Serialize, ToSchema)]
pub struct FacetChannel {
    /// Channel snowflake.
    pub id: String,
    /// Entries.
    pub count: i64,
}

/// Query of `GET /guilds/{id}/scrapbook`.
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Entries older than this entry id.
    pub before: Option<String>,
    /// Page size, 1 to 50 (default 24).
    pub limit: Option<i64>,
    /// Only this channel.
    pub channel_id: Option<String>,
    /// Only messages by this member.
    pub author_id: Option<String>,
    /// Only this tag.
    pub tag: Option<String>,
    /// Sent on or after this day, `YYYY-MM-DD` (UTC).
    pub from: Option<String>,
    /// Sent on or before this day, `YYYY-MM-DD` (UTC).
    pub to: Option<String>,
    /// Include the filter choices.
    pub facets: Option<bool>,
}

/// The shelf the module writes to.
#[derive(Debug, Serialize, ToSchema)]
pub struct ScrapbookTargetView {
    /// The mount.
    pub mount_id: String,
    /// Its label.
    pub label: String,
    /// The member who delegated `upload` to the module.
    pub granted_by: String,
    /// The module can write to it now.
    pub ok: bool,
    /// Why not, when `ok` is false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// The module in this server, for this viewer.
#[derive(Debug, Serialize, ToSchema)]
pub struct ScrapbookStatus {
    /// The operator has made the module available (platform scope). When `false` everything else is empty.
    pub offered: bool,
    /// This server has it on.
    pub enabled: bool,
    /// Trigger emoji.
    pub emojis: Vec<String>,
    /// Reactions needed.
    pub threshold: u32,
    /// The viewer sees "Send to Scrapbook".
    pub can_send: bool,
    /// The viewer may choose the shelf and read the removal list.
    pub can_manage: bool,
    /// The shelf, for managers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<ScrapbookTargetView>,
    /// Copies that could not be removed from a bucket (managers; 0 for others).
    pub failed_removals: i64,
    /// Channels where reactions do not archive (so the banner is not shown there).
    pub ignored_channels: Vec<String>,
}

/// Body of `PUT /channels/{id}/messages/{id}/scrapbook`.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct SendBody {
    /// Tags to start with.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Body of `PATCH /guilds/{id}/scrapbook/{entry}`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TagsBody {
    /// The new tags (replaces the old ones).
    pub tags: Vec<String>,
}

/// Body of `PUT /guilds/{id}/scrapbook/target`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TargetBody {
    /// A mount of this server.
    pub mount_id: String,
}

/// What removing an entry did.
#[derive(Debug, Serialize, ToSchema)]
pub struct RemovedView {
    /// Copies taken out of the bucket.
    pub copies_removed: u64,
    /// Copies the bucket refused to delete; they are listed for the server's managers and retried.
    pub copies_failed: u64,
}

/// A copy that could not be removed from a bucket.
#[derive(Debug, Serialize, ToSchema)]
pub struct RemovalView {
    /// Snowflake.
    pub id: String,
    /// The shelf.
    pub mount_id: String,
    /// The copy's key in the shelf.
    pub key: String,
    /// The message it was made from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Tries so far.
    pub attempts: i32,
    /// Why the last try failed.
    pub last_error: String,
    /// When the next try is due, unix ms.
    pub next_attempt_at: i64,
    /// When it was first queued, unix ms.
    pub queued_at: i64,
}

/// The copies that could not be removed.
#[derive(Debug, Serialize, ToSchema)]
pub struct RemovalList {
    /// Oldest first.
    pub removals: Vec<RemovalView>,
}

/// Outcome of "retry now".
#[derive(Debug, Serialize, ToSchema)]
pub struct RetryView {
    /// The copy is gone now.
    pub removed: bool,
}

// ---- helpers ---------------------------------------------------------------------------------------

fn not_found() -> MsgError {
    MsgError::Api(ApiError::with_message(
        ErrorCode::SrNotFound,
        "Unknown scrapbook entry",
    ))
}

fn conflict(code: &str, message: &str) -> MsgError {
    MsgError::Api(
        ApiError::with_message(ErrorCode::SrConflict, message).field("scrapbook", code, message),
    )
}

fn internal(what: &str, e: impl std::fmt::Display) -> MsgError {
    tracing::error!(error = %e, "scrapbook: {what}");
    ErrorCode::SrInternal.into()
}

fn row(raw: &str) -> MsgResult<i64> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(not_found)
}

fn parse_id<T: std::str::FromStr>(raw: &str) -> MsgResult<T> {
    raw.parse::<T>().map_err(|_| not_found())
}

fn archive_error(e: ArchiveError) -> MsgError {
    match e {
        ArchiveError::NoTarget => conflict(
            "SCRAPBOOK_NO_SHELF",
            "This server has not chosen a storage shelf for its scrapbook yet.",
        ),
        ArchiveError::TargetRefused(r) => {
            let msg = format!("The scrapbook shelf cannot be written to: {r}.");
            conflict("SCRAPBOOK_SHELF_REFUSED", &msg)
        }
        ArchiveError::Empty => invalid(
            "message",
            "SCRAPBOOK_NOTHING_TO_KEEP",
            "That message has no text and no file that can be kept.",
        ),
        ArchiveError::Copy { filename, detail } => {
            tracing::warn!(%filename, %detail, "scrapbook copy failed");
            conflict(
                "SCRAPBOOK_COPY_FAILED",
                "The shelf's bucket did not accept the copy. Check that it is online and reachable.",
            )
        }
        ArchiveError::Store(e) => internal("archive", e),
    }
}

/// Parse `YYYY-MM-DD` into unix ms at 00:00 UTC.
fn parse_day(raw: &str) -> Option<i64> {
    let mut it = raw.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if it.next().is_some()
        || !(1..=12).contains(&m)
        || !(1..=31).contains(&d)
        || !(1970..=9999).contains(&y)
    {
        return None;
    }
    // Howard Hinnant's days-from-civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400_000)
}

/// The module's settings for a server: its own and the platform's.
struct Module {
    cfg: ScrapbookConfig,
    platform: ScrapbookConfig,
}

async fn read_config(features: &FeatureService, guild: GuildId) -> MsgResult<Module> {
    let guild_values = features
        .effective_settings(SCRAPBOOK_ID, &ScopeRef::Guild { guild })
        .await
        .map_err(|e| internal("guild settings", e))?;
    let platform_values = features
        .effective_settings(SCRAPBOOK_ID, &ScopeRef::Platform)
        .await
        .map_err(|e| internal("platform settings", e))?;
    Ok(Module {
        cfg: ScrapbookConfig::from_values(&guild_values),
        platform: ScrapbookConfig::from_values(&platform_values),
    })
}

/// 404 while the operator has not made the module available, 403 `SR_FEATURE_DISABLED` while the server has
/// it off.
async fn gate(features: &FeatureService, guild: GuildId) -> MsgResult<Module> {
    if !features
        .offered(SCRAPBOOK_ID)
        .await
        .map_err(|e| internal("offered", e))?
    {
        return Err(MsgError::Api(ApiError::with_message(
            ErrorCode::SrNotFound,
            "Unknown resource",
        )));
    }
    let on = features
        .is_enabled(SCRAPBOOK_ID, &ScopeRef::Guild { guild })
        .await
        .map_err(|e| internal("enabled", e))?;
    if !on {
        return Err(MsgError::Api(ApiError::with_message(
            ErrorCode::SrFeatureDisabled,
            "The scrapbook is not turned on for this server.",
        )));
    }
    read_config(features, guild).await
}

fn role_ids(acc: &GuildAccess) -> Vec<u64> {
    acc.member.role_ids.iter().map(|r| r.get()).collect()
}

/// Whether the viewer may send a message by hand: a configured role, or `MANAGE_MESSAGES` (channel-level when
/// `channel_perms` is given).
fn may_send(cfg: &ScrapbookConfig, acc: &GuildAccess, channel_perms: Option<Permissions>) -> bool {
    channel_perms
        .unwrap_or(acc.perms)
        .contains(Permissions::MANAGE_MESSAGES)
        || cfg.role_allowed(&role_ids(acc))
}

/// The channels (threads included) of the server the viewer may view.
async fn viewable_channels(
    deps: &SettingsDeps,
    acc: &GuildAccess,
    guild: GuildId,
) -> MsgResult<Vec<i64>> {
    let chat: &dyn ChatRepository = &*deps.chat;
    let channels = chat
        .list_guild_channels(guild)
        .await
        .map_err(|e| internal("channels", e.0))?;
    let threads = chat
        .list_guild_threads(guild)
        .await
        .map_err(|e| internal("threads", e.0))?;
    let ids: Vec<ChannelId> = channels.iter().map(|c| c.id).collect();
    let overwrites = chat
        .get_overwrites_of(&ids)
        .await
        .map_err(|e| internal("overwrites", e.0))?;
    let now = deps.now_ms();
    let mut seen: HashMap<ChannelId, bool> = HashMap::new();
    let mut out = Vec::new();
    for c in &channels {
        if c.kind == ChannelType::Category {
            continue;
        }
        let ow = overwrites.get(&c.id).map(Vec::as_slice).unwrap_or(&[]);
        let ok = channel_permissions(&acc.guild, &acc.member, &acc.roles, ow, now)
            .contains(Permissions::VIEW_CHANNEL);
        seen.insert(c.id, ok);
        if ok {
            out.push(c.id.get() as i64);
        }
    }
    for t in &threads {
        if t.parent_id.and_then(|p| seen.get(&p).copied()) == Some(true) {
            out.push(t.id.get() as i64);
        }
    }
    Ok(out)
}

/// Whether the viewer may view `channel` of this server.
async fn may_view(deps: &SettingsDeps, acc: &GuildAccess, channel: i64) -> bool {
    let id = ChannelId::new(channel as u64);
    let Ok(Some(c)) = deps.chat.get_channel(id).await else {
        return false;
    };
    // A thread follows its parent.
    let target = c.parent_id.filter(|_| c.thread.is_some()).unwrap_or(c.id);
    let Ok(ow) = deps.chat.get_overwrites(target).await else {
        return false;
    };
    channel_permissions(&acc.guild, &acc.member, &acc.roles, &ow, deps.now_ms())
        .contains(Permissions::VIEW_CHANNEL)
}

/// Read a message and its attachments as the archiver wants them. `None` when the message is not there, is
/// deleted, or is not a plain member message of that server.
pub async fn load_source(
    chat: &dyn ChatRepository,
    messages: &dyn MessageStore,
    guild: GuildId,
    channel: ChannelId,
    message: MessageId,
) -> Result<Option<SourceMessage>, String> {
    let Some(m) = chat.get_message(channel, message).await.map_err(|e| e.0)? else {
        return Ok(None);
    };
    if !m.is_visible()
        || m.deleted_at_ms.is_some()
        || m.guild_id != Some(guild)
        || !matches!(m.kind, MessageType::Default | MessageType::Reply)
    {
        return Ok(None);
    }
    let author = chat.users(&[m.author_id]).await.map_err(|e| e.0)?;
    let author_name = author
        .first()
        .map(|u| u.display_name.clone().unwrap_or_else(|| u.username.clone()))
        .unwrap_or_default();
    let atts = messages
        .attachments_of(&[m.id])
        .await
        .map_err(|e| e.0)?
        .remove(&m.id)
        .unwrap_or_default();
    Ok(Some(SourceMessage {
        guild_id: guild.get() as i64,
        channel_id: channel.get() as i64,
        message_id: m.id.get() as i64,
        author_id: m.author_id.get() as i64,
        author_name,
        content: m.content.clone(),
        sent_at_ms: m.id.0.unix_ms(),
        attachments: atts
            .into_iter()
            .map(|a| SourceAttachment {
                key: a.key,
                filename: a.filename,
                content_type: a
                    .content_type
                    .or(a.declared_content_type)
                    .unwrap_or_default(),
                size: a.size,
            })
            .collect(),
    }))
}

/// Which mounts the viewer reaches, and whether each file is readable by them.
struct Reach {
    cache: HashMap<i64, Option<(sr_storage::scopes::Access, bool)>>,
}

impl Reach {
    fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    async fn playable(&mut self, d: &ScopeDeps, user: i64, o: &Object) -> bool {
        let reach = match self.cache.get(&o.mount_id) {
            Some(r) => r.clone(),
            None => {
                let r = mount_reach(d, user, o.mount_id).await;
                self.cache.insert(o.mount_id, r.clone());
                r
            }
        };
        match reach {
            Some((a, cors)) => cors && a.permits(&o.key).is_ok(),
            None => false,
        }
    }
}

/// How the viewer reaches one mount (`None`: not at all), and whether the mount passed its CORS check.
async fn mount_reach(
    d: &ScopeDeps,
    user: i64,
    mount_id: i64,
) -> Option<(sr_storage::scopes::Access, bool)> {
    let mount = d.mounts.by_id(mount_id).await.ok().flatten()?;
    let facts: &dyn ScopeFacts = &*d.store;
    let access = sr_storage::scopes::access(
        facts,
        Viewer {
            id: user,
            staff: false,
        },
        &mount,
    )
    .await
    .ok()?;
    Some((access, mount.cors_ok_at.is_some()))
}

fn object_view(o: &Object, playable: bool) -> ScrapbookObjectView {
    ScrapbookObjectView {
        id: o.id.to_string(),
        filename: o.filename.clone(),
        content_type: o.content_type.clone(),
        size: o.size,
        kind: o.kind.as_str().into(),
        playable,
    }
}

async fn entry_view(
    x: &EntryWithObjects,
    reach: &mut Reach,
    d: &ScopeDeps,
    user: i64,
    can_edit: bool,
) -> ScrapbookEntryView {
    let mut objects = Vec::with_capacity(x.objects.len());
    for o in &x.objects {
        let ok = reach.playable(d, user, o).await;
        objects.push(object_view(o, ok));
    }
    let e = &x.entry;
    ScrapbookEntryView {
        id: e.id.to_string(),
        channel_id: e.channel_id.to_string(),
        message_id: e.message_id.to_string(),
        author_id: e.author_id.to_string(),
        author_name: e.author_name.clone(),
        content: e.content.clone(),
        sent_at: e.sent_at_ms,
        tags: e.tags.clone(),
        via: e.via.as_str().into(),
        archived_by: e.archived_by.map(|u| u.to_string()),
        skipped: e.skipped,
        objects,
        can_edit,
    }
}

/// The author, the one who sent it, or someone who manages messages.
fn can_edit_entry(user: i64, x: &EntryWithObjects, manages: bool) -> bool {
    manages || x.entry.author_id == user || x.entry.archived_by == Some(user)
}

// ---- handlers --------------------------------------------------------------------------------------

/// The module in this server, for the viewer.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/scrapbook/status",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = ScrapbookStatus), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn status(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Ext(sb): Ext<ScrapbookDeps>,
    Ext(d): Ext<ScopeDeps>,
    Path(guild_id): Path<String>,
) -> MsgResult<Json<ScrapbookStatus>> {
    let user = UserId::new(uid(&p) as u64);
    let gid: GuildId = parse_id(&guild_id)?;
    let acc = settings.guild_access(user, gid).await?;
    let features = &gf.ctx.settings;
    let offered = features
        .offered(SCRAPBOOK_ID)
        .await
        .map_err(|e| internal("offered", e))?;
    let empty = || ScrapbookStatus {
        offered: false,
        enabled: false,
        emojis: vec![],
        threshold: 0,
        can_send: false,
        can_manage: false,
        target: None,
        failed_removals: 0,
        ignored_channels: vec![],
    };
    if !offered {
        return Ok(Json(empty()));
    }
    let enabled = features
        .is_enabled(SCRAPBOOK_ID, &ScopeRef::Guild { guild: gid })
        .await
        .map_err(|e| internal("enabled", e))?;
    let module = read_config(features, gid).await?;
    let can_manage = acc.perms.contains(Permissions::MANAGE_GUILD);
    let mut out = ScrapbookStatus {
        offered: true,
        enabled,
        emojis: module.cfg.emojis.clone(),
        threshold: module.cfg.threshold,
        can_send: enabled && may_send(&module.cfg, &acc, None),
        can_manage,
        target: None,
        failed_removals: 0,
        ignored_channels: module.cfg.ignored_channels.clone(),
    };
    if can_manage && enabled {
        let g = gid.get() as i64;
        let (target, refusal) = sb
            .archiver
            .target_health(g)
            .await
            .map_err(|e| internal("target", e))?;
        if let Some(t) = target {
            let label = match d.mounts.by_id(t.mount_id).await {
                Ok(Some(m)) => m.label,
                _ => String::new(),
            };
            out.target = Some(target_view(&t, refusal.as_ref(), &label));
        }
        out.failed_removals = sb
            .archiver
            .store()
            .failed_removals(g)
            .await
            .map_err(|e| internal("removals", e))?
            .len() as i64;
    }
    Ok(Json(out))
}

fn target_view(t: &Target, refusal: Option<&Refusal>, label: &str) -> ScrapbookTargetView {
    ScrapbookTargetView {
        mount_id: t.mount_id.to_string(),
        label: label.to_owned(),
        granted_by: t.granted_by.to_string(),
        ok: refusal.is_none(),
        problem: refusal.map(ToString::to_string),
    }
}

/// The gallery.
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/scrapbook",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("before" = Option<String>, Query, description = "Entries older than this entry id"),
        ("limit" = Option<i64>, Query, description = "1 to 50, default 24"),
        ("channel_id" = Option<String>, Query, description = "Only this channel"),
        ("author_id" = Option<String>, Query, description = "Only messages by this member"),
        ("tag" = Option<String>, Query, description = "Only this tag"),
        ("from" = Option<String>, Query, description = "Sent on or after this day, YYYY-MM-DD (UTC)"),
        ("to" = Option<String>, Query, description = "Sent on or before this day, YYYY-MM-DD (UTC)"),
        ("facets" = Option<bool>, Query, description = "Include the filter choices"),
    ),
    responses((status = 200, body = ScrapbookPage), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn list(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Ext(sb): Ext<ScrapbookDeps>,
    Ext(d): Ext<ScopeDeps>,
    Path(guild_id): Path<String>,
    Query(q): Query<ListQuery>,
) -> MsgResult<Json<ScrapbookPage>> {
    let me = uid(&p);
    let gid: GuildId = parse_id(&guild_id)?;
    let acc = settings.guild_access(UserId::new(me as u64), gid).await?;
    gate(&gf.ctx.settings, gid).await?;

    let viewable = viewable_channels(&settings, &acc, gid).await?;
    let limit = q.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let opt_id = |raw: &Option<String>, field: &str| -> MsgResult<Option<i64>> {
        match raw.as_deref().filter(|s| !s.is_empty()) {
            None => Ok(None),
            Some(s) => s
                .parse::<i64>()
                .ok()
                .filter(|v| *v > 0)
                .map(Some)
                .ok_or_else(|| invalid(field, "SNOWFLAKE_TYPE", "Expected an id.")),
        }
    };
    let day = |raw: &Option<String>, field: &str| -> MsgResult<Option<i64>> {
        match raw.as_deref().filter(|s| !s.is_empty()) {
            None => Ok(None),
            Some(s) => parse_day(s)
                .map(Some)
                .ok_or_else(|| invalid(field, "DATE_INVALID", "Use YYYY-MM-DD.")),
        }
    };
    let filter = Filter {
        viewable: viewable.clone(),
        channel_id: opt_id(&q.channel_id, "channel_id")?,
        author_id: opt_id(&q.author_id, "author_id")?,
        tag: q
            .tag
            .as_deref()
            .map(|t| clean_tags(&[t.to_owned()]))
            .and_then(|mut v| v.pop()),
        from_ms: day(&q.from, "from")?,
        // `to` is inclusive: the day after is the exclusive bound.
        to_ms: day(&q.to, "to")?.map(|ms| ms + 86_400_000),
    };
    let before = opt_id(&q.before, "before")?;
    let g = gid.get() as i64;
    // One more than asked for tells whether there is a next page.
    let mut rows = sb
        .archiver
        .store()
        .list(g, &filter, before, limit + 1)
        .await
        .map_err(|e| internal("list", e))?;
    let next_before = if rows.len() as i64 > limit {
        rows.truncate(limit as usize);
        rows.last().map(|x| x.entry.id.to_string())
    } else {
        None
    };
    let manages = acc.perms.contains(Permissions::MANAGE_MESSAGES);
    let mut reach = Reach::new();
    let mut entries = Vec::with_capacity(rows.len());
    for x in &rows {
        entries.push(entry_view(x, &mut reach, &d, me, can_edit_entry(me, x, manages)).await);
    }
    let facets = if q.facets.unwrap_or(false) {
        let f = sb
            .archiver
            .store()
            .facets(g, &viewable)
            .await
            .map_err(|e| internal("facets", e))?;
        Some(ScrapbookFacets {
            tags: f
                .tags
                .into_iter()
                .map(|(tag, count)| FacetTag { tag, count })
                .collect(),
            authors: f
                .authors
                .into_iter()
                .map(|(id, name, count)| FacetAuthor {
                    id: id.to_string(),
                    name,
                    count,
                })
                .collect(),
            channels: f
                .channels
                .into_iter()
                .map(|(id, count)| FacetChannel {
                    id: id.to_string(),
                    count,
                })
                .collect(),
        })
    } else {
        None
    };
    Ok(Json(ScrapbookPage {
        entries,
        next_before,
        facets,
    }))
}

/// "Send to Scrapbook": archive one message by hand.
#[utoipa::path(
    put,
    path = "/channels/{channel_id}/messages/{message_id}/scrapbook",
    params(
        ("channel_id" = String, Path, description = "Channel snowflake"),
        ("message_id" = String, Path, description = "Message snowflake"),
    ),
    request_body = SendBody,
    responses(
        (status = 200, body = ScrapbookEntryView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "SCRAPBOOK_NO_SHELF, SCRAPBOOK_SHELF_REFUSED or SCRAPBOOK_COPY_FAILED"),
    ),
    security(("session" = []))
)]
pub async fn send(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Ext(sb): Ext<ScrapbookDeps>,
    Ext(d): Ext<ScopeDeps>,
    Path((channel_id, message_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<SendBody>,
) -> MsgResult<Json<ScrapbookEntryView>> {
    let me = uid(&p);
    let channel: ChannelId = parse_id(&channel_id)?;
    let message: MessageId = parse_id(&message_id)?;
    let ca = settings
        .channel_access(UserId::new(me as u64), channel)
        .await?;
    let gid = ca.channel.guild_id.ok_or_else(not_found)?;
    let module = gate(&gf.ctx.settings, gid).await?;
    if !may_send(&module.cfg, &ca.guild, Some(ca.perms)) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let src = load_source(&*settings.chat, &*sb.messages, gid, channel, message)
        .await
        .map_err(|e| internal("load message", e))?
        .ok_or_else(not_found)?;
    let outcome = sb
        .archiver
        .archive(ArchiveRequest {
            message: src,
            by: Some(me),
            via: Via::Manual,
            tags: body.tags,
            max_object_bytes: module.platform.max_object_bytes(),
            now_ms: settings.now_ms(),
        })
        .await
        .map_err(archive_error)?;
    let x = match outcome {
        ArchiveOutcome::Archived(x) | ArchiveOutcome::AlreadyArchived(x) => x,
    };
    let manages = ca.perms.contains(Permissions::MANAGE_MESSAGES);
    let mut reach = Reach::new();
    Ok(Json(
        entry_view(&x, &mut reach, &d, me, can_edit_entry(me, &x, manages)).await,
    ))
}

async fn load_entry(
    sb: &ScrapbookDeps,
    guild: GuildId,
    entry_id: &str,
) -> MsgResult<EntryWithObjects> {
    sb.archiver
        .store()
        .get(guild.get() as i64, row(entry_id)?)
        .await
        .map_err(|e| internal("entry", e))?
        .ok_or_else(not_found)
}

/// Change an entry's tags.
#[utoipa::path(
    patch,
    path = "/guilds/{guild_id}/scrapbook/{entry_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("entry_id" = String, Path, description = "Entry snowflake"),
    ),
    request_body = TagsBody,
    responses((status = 200, body = ScrapbookEntryView), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn edit_entry(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Ext(sb): Ext<ScrapbookDeps>,
    Ext(d): Ext<ScopeDeps>,
    Path((guild_id, entry_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<TagsBody>,
) -> MsgResult<Json<ScrapbookEntryView>> {
    let me = uid(&p);
    let gid: GuildId = parse_id(&guild_id)?;
    let acc = settings.guild_access(UserId::new(me as u64), gid).await?;
    gate(&gf.ctx.settings, gid).await?;
    let x = load_entry(&sb, gid, &entry_id).await?;
    if !may_view(&settings, &acc, x.entry.channel_id).await {
        return Err(not_found());
    }
    let manages = acc.perms.contains(Permissions::MANAGE_MESSAGES);
    if !can_edit_entry(me, &x, manages) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let tags = clean_tags(&body.tags);
    sb.archiver
        .store()
        .set_tags(gid.get() as i64, x.entry.id, &tags)
        .await
        .map_err(|e| internal("tags", e))?;
    let mut x = x;
    x.entry.tags = tags;
    let mut reach = Reach::new();
    Ok(Json(entry_view(&x, &mut reach, &d, me, true).await))
}

/// Remove an entry and its copies.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/scrapbook/{entry_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("entry_id" = String, Path, description = "Entry snowflake"),
    ),
    responses((status = 200, body = RemovedView), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn remove_entry(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(sb): Ext<ScrapbookDeps>,
    Path((guild_id, entry_id)): Path<(String, String)>,
) -> MsgResult<Json<RemovedView>> {
    let me = uid(&p);
    let gid: GuildId = parse_id(&guild_id)?;
    let acc = settings.guild_access(UserId::new(me as u64), gid).await?;
    let x = load_entry(&sb, gid, &entry_id).await?;
    let manages = acc.perms.contains(Permissions::MANAGE_MESSAGES);
    if !can_edit_entry(me, &x, manages) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let report = sb
        .archiver
        .cascade(
            DeleteScope::Entry {
                guild_id: gid.get() as i64,
                id: x.entry.id,
            },
            settings.now_ms(),
        )
        .await
        .map_err(|e| internal("cascade", e))?;
    Ok(Json(RemovedView {
        copies_removed: report.removed,
        copies_failed: report.failed,
    }))
}

/// A presigned `GET` of one copy, for the viewer only.
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/scrapbook/{entry_id}/objects/{object_id}/presign",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("entry_id" = String, Path, description = "Entry snowflake"),
        ("object_id" = String, Path, description = "Object snowflake"),
    ),
    responses(
        (status = 200, body = PresignView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
        (status = 409, body = ErrorBody, description = "CORS_NOT_READY"),
    ),
    security(("session" = []))
)]
#[allow(clippy::too_many_arguments)]
pub async fn presign(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Ext(sb): Ext<ScrapbookDeps>,
    Ext(d): Ext<ScopeDeps>,
    MaybeExt(admin): MaybeExt<AdminDeps>,
    headers: HeaderMap,
    Path((guild_id, entry_id, object_id)): Path<(String, String, String)>,
) -> MsgResult<Json<PresignView>> {
    let me = uid(&p);
    let gid: GuildId = parse_id(&guild_id)?;
    let acc = settings.guild_access(UserId::new(me as u64), gid).await?;
    gate(&gf.ctx.settings, gid).await?;
    let x = load_entry(&sb, gid, &entry_id).await?;
    // The entry is as visible as its source channel.
    if !may_view(&settings, &acc, x.entry.channel_id).await {
        return Err(not_found());
    }
    let oid = row(&object_id)?;
    let o = x
        .objects
        .iter()
        .find(|o| o.id == oid)
        .ok_or_else(not_found)?;
    // The mount id and key come from our row, never from the client; the storage route then applies the
    // viewer's own scopes (role grants, delisting, CORS) and the presign lifetime rules.
    crate::routes::storage::objects::presign(
        State(state),
        SessionOnly(p),
        Ext(d),
        MaybeExt(admin),
        headers,
        Path(o.mount_id.to_string()),
        ApiJson(PresignBody {
            key: o.key.clone(),
            method: "GET".into(),
            range: None,
        }),
    )
    .await
}

/// Choose the shelf the module writes to. The caller delegates `upload` to the module's service account, so
/// they must hold it on that mount; the mount must be this server's own.
#[utoipa::path(
    put,
    path = "/guilds/{guild_id}/scrapbook/target",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    request_body = TargetBody,
    responses((status = 200, body = ScrapbookTargetView), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn set_target(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(gf): Ext<GuildFeatures>,
    Ext(sb): Ext<ScrapbookDeps>,
    Ext(d): Ext<ScopeDeps>,
    Path(guild_id): Path<String>,
    ApiJson(body): ApiJson<TargetBody>,
) -> MsgResult<Json<ScrapbookTargetView>> {
    let me = uid(&p);
    let gid: GuildId = parse_id(&guild_id)?;
    let g = gid.get() as i64;
    settings.guild_access(UserId::new(me as u64), gid).await?;
    gate(&gf.ctx.settings, gid).await?;
    let viewer = Viewer {
        id: me,
        staff: false,
    };
    let facts: &dyn ScopeFacts = &*d.store;
    if !sr_storage::scopes::may_manage_guild(facts, viewer, g)
        .await
        .map_err(|e| internal("grants", e))?
    {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let mount_id = body
        .mount_id
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| invalid("mount_id", "SNOWFLAKE_TYPE", "Expected a mount id."))?;
    let mount = d
        .mounts
        .by_id(mount_id)
        .await
        .map_err(|e| internal("mount", e))?
        .filter(|m| m.owner == MountOwner::Guild(g))
        .ok_or_else(not_found)?;
    if mount.status != MountStatus::Active {
        return Err(conflict(
            "SCRAPBOOK_SHELF_REFUSED",
            "That shelf is switched off.",
        ));
    }
    // The grant being delegated: the caller must hold `upload` on this mount themselves.
    if !sr_storage::scopes::may_upload(facts, viewer, &mount)
        .await
        .map_err(|e| internal("upload grant", e))?
    {
        return Err(ErrorCode::MissingPermissions.into());
    }
    let target = Target {
        guild_id: g,
        mount_id,
        granted_by: me,
    };
    sb.archiver
        .store()
        .set_target(&target)
        .await
        .map_err(|e| internal("set target", e))?;
    let (_, refusal) = sb
        .archiver
        .target_health(g)
        .await
        .map_err(|e| internal("target", e))?;
    Ok(Json(target_view(&target, refusal.as_ref(), &mount.label)))
}

/// Drop the shelf. Existing entries keep their copies; nothing new is archived until a shelf is chosen.
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/scrapbook/target",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn clear_target(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(sb): Ext<ScrapbookDeps>,
    Path(guild_id): Path<String>,
) -> MsgResult<axum::http::StatusCode> {
    let me = uid(&p);
    let gid: GuildId = parse_id(&guild_id)?;
    let acc = settings.guild_access(UserId::new(me as u64), gid).await?;
    if !acc.perms.contains(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    sb.archiver
        .store()
        .clear_target(gid.get() as i64)
        .await
        .map_err(|e| internal("clear target", e))?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn require_manager(settings: &SettingsDeps, me: i64, gid: GuildId) -> MsgResult<GuildAccess> {
    let acc = settings.guild_access(UserId::new(me as u64), gid).await?;
    if !acc.perms.contains(Permissions::MANAGE_GUILD) {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok(acc)
}

/// The copies that could not be removed from a bucket (managers).
#[utoipa::path(
    get,
    path = "/guilds/{guild_id}/scrapbook/removals",
    params(("guild_id" = String, Path, description = "Guild snowflake")),
    responses((status = 200, body = RemovalList), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn removals(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(sb): Ext<ScrapbookDeps>,
    Path(guild_id): Path<String>,
) -> MsgResult<Json<RemovalList>> {
    let gid: GuildId = parse_id(&guild_id)?;
    require_manager(&settings, uid(&p), gid).await?;
    let rows = sb
        .archiver
        .store()
        .failed_removals(gid.get() as i64)
        .await
        .map_err(|e| internal("removals", e))?;
    Ok(Json(RemovalList {
        removals: rows
            .into_iter()
            .map(|r| RemovalView {
                id: r.id.to_string(),
                mount_id: r.mount_id.to_string(),
                key: r.key,
                message_id: r.message_id.map(|m| m.to_string()),
                attempts: r.attempts,
                last_error: r.last_error,
                next_attempt_at: r.next_attempt_ms,
                queued_at: r.created_at_ms,
            })
            .collect(),
    }))
}

/// Try a failed removal again now (managers).
#[utoipa::path(
    post,
    path = "/guilds/{guild_id}/scrapbook/removals/{removal_id}/retry",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("removal_id" = String, Path, description = "Removal snowflake"),
    ),
    responses((status = 200, body = RetryView), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn retry_removal(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(sb): Ext<ScrapbookDeps>,
    Path((guild_id, removal_id)): Path<(String, String)>,
) -> MsgResult<Json<RetryView>> {
    let gid: GuildId = parse_id(&guild_id)?;
    require_manager(&settings, uid(&p), gid).await?;
    let id = row(&removal_id)?;
    let now = settings.now_ms();
    if !sb
        .archiver
        .store()
        .retry_now(gid.get() as i64, id, now)
        .await
        .map_err(|e| internal("retry", e))?
    {
        return Err(not_found());
    }
    let removed = sb
        .archiver
        .try_removal(id, now)
        .await
        .map_err(|e| internal("removal", e))?;
    Ok(Json(RetryView { removed }))
}

/// Forget a failed removal because the copy was deleted another way (managers).
#[utoipa::path(
    delete,
    path = "/guilds/{guild_id}/scrapbook/removals/{removal_id}",
    params(
        ("guild_id" = String, Path, description = "Guild snowflake"),
        ("removal_id" = String, Path, description = "Removal snowflake"),
    ),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn dismiss_removal(
    SessionOnly(p): SessionOnly,
    Ext(settings): Ext<SettingsDeps>,
    Ext(sb): Ext<ScrapbookDeps>,
    Path((guild_id, removal_id)): Path<(String, String)>,
) -> MsgResult<axum::http::StatusCode> {
    let gid: GuildId = parse_id(&guild_id)?;
    require_manager(&settings, uid(&p), gid).await?;
    if sb
        .archiver
        .store()
        .dismiss(gid.get() as i64, row(&removal_id)?)
        .await
        .map_err(|e| internal("dismiss", e))?
    {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

/// OpenAPI fragment of the scrapbook routes.
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        status,
        list,
        send,
        edit_entry,
        remove_entry,
        presign,
        set_target,
        clear_target,
        removals,
        retry_removal,
        dismiss_removal,
    ),
    components(schemas(
        ErrorBody,
        ScrapbookObjectView,
        ScrapbookEntryView,
        ScrapbookPage,
        ScrapbookFacets,
        FacetTag,
        FacetAuthor,
        FacetChannel,
        ScrapbookTargetView,
        ScrapbookStatus,
        SendBody,
        TagsBody,
        TargetBody,
        RemovedView,
        RemovalView,
        RemovalList,
        RetryView,
        PresignView,
    ))
)]
pub struct ScrapbookApiDoc;

/// The OpenAPI fragment of the scrapbook routes.
pub fn openapi() -> utoipa::openapi::OpenApi {
    <ScrapbookApiDoc as utoipa::OpenApi>::openapi()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_parse_to_midnight_utc() {
        assert_eq!(parse_day("1970-01-01"), Some(0));
        assert_eq!(parse_day("2026-06-21"), Some(1_782_000_000_000));
        assert_eq!(parse_day("2026-13-01"), None);
        assert_eq!(parse_day("2026-06"), None);
        assert_eq!(parse_day("x"), None);
        assert_eq!(parse_day("2026-06-21-1"), None);
    }
}
