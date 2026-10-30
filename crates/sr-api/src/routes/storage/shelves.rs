//! The shelves (BYOS-CAT-002): what the indexer found in the mounts a viewer may see, in Cinema, Library and
//! Music; the metadata editor; the Re-index button's two routes.
//!
//! * `GET /shelves/{section}`: a page of titles, series, books or tracks from every reachable mount, with the
//!   "Shelves" filter (`mount`) and the sources the viewer reaches (the chips).
//! * `GET /shelves/{section}/{shelf_item_id}`: one item with its episodes or chapters.
//! * `PATCH /shelves/{section}/{shelf_item_id}` and `.../parts/{shelf_part_id}`: the editor. Whoever may manage the mount
//!   (its owner; on a guild shelf a manager) edits; every edited field becomes an override a re-index keeps.
//! * `POST` and `GET /storage/mounts/{mount_id}/index`: queue a run and read the last one.
//!
//! The decision of what a viewer sees is `sr_storage::index::shelf::reachable` (one `scopes::access` per mount)
//! and `drop_delisted`; the rows themselves are unpublished, so the instance catalog never shows them. Artwork
//! and the file to play are presigned by this server (local signing, no bucket round trip) or by the existing
//! `POST /storage/mounts/{mount_id}/presign`; the server never streams a mount's bytes.

use super::{ScopeDeps, facts_failed, parse_row, refusal, uid};
use crate::AppState;
use crate::auth::SessionOnly;
use crate::openapi::ErrorBody;
use crate::routes::Ext;
use crate::routes::messages::error::{ApiJson, ApiQuery, MsgError, MsgResult, invalid};
use crate::routes::messages::model::iso;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Deserializer, Serialize};
use sr_core::ErrorCode;
use sr_storage::index::shelf::{
    PartPatch, PatchError, Reach, Section, ShelfDetail, ShelfFilter, ShelfItem, ShelfPatch,
    ShelfStore, drop_delisted, reachable,
};
use sr_storage::index::{IndexControl, RunStatus};
use sr_storage::mounts::{INSTANCE_MOUNT_ID, Mount, MountOwner};
use sr_storage::path::safe_key;
use sr_storage::scopes::{Scope, ScopeFacts, Viewer};
use sr_storage::{ObjectStorage, PresignMethod};
use std::collections::{BTreeMap, HashMap};
use std::time::Duration;
use utoipa::ToSchema;

/// Collaborators of the shelf routes (the mount lookup, facts and sealer come from [`ScopeDeps`]).
pub struct ShelfDeps {
    /// Shelf rows.
    pub store: std::sync::Arc<dyn ShelfStore>,
    /// Queue and status of index runs.
    pub index: std::sync::Arc<dyn IndexControl>,
}

/// The index hooks of the bucket browser's delete and move (wave-28 integration): optional, so a deployment or
/// a test without the catalog still deletes and moves; the next `storage.index` run reaches the same rows.
pub struct IndexHooks {
    /// Hide or re-key the rows of changed objects.
    pub changes: std::sync::Arc<dyn sr_storage::index::ObjectChanges>,
    /// Queue a run after a move (a series keyed by a moved folder is found again by it).
    pub index: std::sync::Arc<dyn IndexControl>,
}

/// Lifetime of the artwork URLs a listing carries.
const ART_TTL: Duration = Duration::from_secs(900);
const MAX_TAGS: usize = 30;

/// How the viewer reaches a mount: the chip on a card.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ShelfSourceView {
    /// Mount snowflake.
    pub mount_id: String,
    /// The mount's label.
    pub label: String,
    /// `own`, `friend`, `guild` or `staff`.
    pub scope: String,
    /// The friend who owns the mount (`friend`), or the owner of a user mount.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    /// The server (`guild`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<String>,
}

pub(crate) fn source_of(r: &Reach) -> ShelfSourceView {
    let (scope, guild_id) = match r.access.scope {
        Scope::Own => ("own", None),
        Scope::Friend { .. } => ("friend", None),
        Scope::Guild(g) => ("guild", Some(g.to_string())),
        Scope::Staff => ("staff", None),
    };
    ShelfSourceView {
        mount_id: r.mount.id.to_string(),
        label: r.mount.label.clone(),
        scope: scope.into(),
        owner_id: match r.mount.owner {
            MountOwner::User(u) => Some(u.to_string()),
            _ => None,
        },
        guild_id,
    }
}

/// One card.
#[derive(Debug, Serialize, ToSchema)]
pub struct ShelfItemView {
    /// Row snowflake.
    pub id: String,
    /// `cinema`, `library` or `music`.
    pub section: String,
    /// `movie`, `series`, `anime`, `manga`, `comic`, `book` or `track`.
    pub kind: String,
    /// Title.
    pub title: String,
    /// Line under the title (a track's artist and album).
    pub subtitle: String,
    /// Year.
    pub year: Option<i16>,
    /// Where it lives.
    pub source: ShelfSourceView,
    /// Short-lived URL of the poster or cover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster_url: Option<String>,
    /// Short-lived URL of the backdrop.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop_url: Option<String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Genres.
    pub genres: Vec<String>,
    /// Synopsis.
    pub synopsis: String,
    /// The metadata provider's attribution, to show next to fetched details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// Provider ids.
    pub external_ids: BTreeMap<String, String>,
    /// Fields the member edited.
    pub overridden: Vec<String>,
    /// Episodes or chapters.
    pub parts: i64,
    /// The file of a film, track or single book (mount-relative), for `POST /storage/mounts/{id}/presign`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_key: Option<String>,
    /// Its container.
    pub container: String,
    /// Plays in a browser as is; otherwise it is greyed with "Convert to play".
    pub web_native: bool,
}

/// An episode, chapter or volume.
#[derive(Debug, Serialize, ToSchema)]
pub struct ShelfPartView {
    /// Row snowflake.
    pub id: String,
    /// Season (episodes).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<i32>,
    /// Episode, chapter or volume number.
    pub number: f64,
    /// Title.
    pub title: String,
    /// The file (mount-relative).
    pub object_key: String,
    /// Container.
    pub container: String,
    /// Plays in a browser as is.
    pub web_native: bool,
    /// Size in bytes.
    pub size: i64,
}

/// A page of cards and the sources behind the chips.
#[derive(Debug, Serialize, ToSchema)]
pub struct ShelfListView {
    /// Cards.
    pub items: Vec<ShelfItemView>,
    /// All that match.
    pub total: i64,
    /// The mounts the viewer reaches for this section (the "Shelves" chips), even when this page shows none.
    pub sources: Vec<ShelfSourceView>,
}

/// One item with its parts.
#[derive(Debug, Serialize, ToSchema)]
pub struct ShelfDetailView {
    /// The card.
    pub item: ShelfItemView,
    /// Episodes or chapters, in order.
    pub parts: Vec<ShelfPartView>,
    /// The viewer may edit the metadata.
    pub can_edit: bool,
}

/// Query of the listing.
#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    /// Title substring.
    pub q: Option<String>,
    /// Kind filter.
    pub kind: Option<String>,
    /// Only this mount (snowflake).
    pub mount: Option<String>,
    /// The server whose section is showing (snowflake): its own shelves, plus the viewer's and friends' shelves
    /// unless `member_shelves_in_guild` is off for it.
    pub guild: Option<String>,
    /// Page size (at most 50).
    pub limit: Option<u32>,
    /// Offset.
    pub offset: Option<u32>,
}

fn de_opt<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Which provider the details came from.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DetailsBody {
    /// `tmdb`, `anilist`, `mangadex`, `mangaupdates` or `comicvine`.
    pub provider: String,
    /// The provider's id for the entry.
    pub external_id: String,
}

/// Body of the editor. Every field present is written and becomes an override.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct ShelfPatchBody {
    /// Title (1 to 300 characters).
    pub title: Option<String>,
    /// Year; `null` clears.
    #[serde(default, deserialize_with = "de_opt")]
    #[schema(value_type = Option<i16>)]
    pub year: Option<Option<i16>>,
    /// `movie`, `series`, `anime` (Cinema); `manga`, `comic`, `book` (Library).
    pub kind: Option<String>,
    /// Tags (lower-cased, at most 30).
    pub tags: Option<Vec<String>>,
    /// Genres.
    pub genres: Option<Vec<String>>,
    /// Synopsis (at most 4000 characters).
    pub synopsis: Option<String>,
    /// Poster or cover: a key on the same mount; `null` clears.
    #[serde(default, deserialize_with = "de_opt")]
    #[schema(value_type = Option<String>)]
    pub poster_key: Option<Option<String>>,
    /// Backdrop (Cinema); `null` clears.
    #[serde(default, deserialize_with = "de_opt")]
    #[schema(value_type = Option<String>)]
    pub backdrop_key: Option<Option<String>>,
    /// Artist (tracks).
    pub artist: Option<String>,
    /// Album (tracks).
    pub album: Option<String>,
    /// Track number (tracks); `null` clears.
    #[serde(default, deserialize_with = "de_opt")]
    #[schema(value_type = Option<i32>)]
    pub track_no: Option<Option<i32>>,
    /// Details fetched from a provider through `GET /submissions/lookup`: stores the provider's id and its
    /// attribution text.
    pub details: Option<DetailsBody>,
}

/// Body of the part editor.
#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct PartPatchBody {
    /// Title.
    pub title: Option<String>,
    /// Episode, chapter or volume number.
    pub number: Option<f64>,
    /// Season (episodes).
    pub season: Option<i32>,
}

/// Last index run of a mount.
#[derive(Debug, Serialize, ToSchema)]
pub struct IndexStatusView {
    /// A job was queued by this call (`false`: one is already queued or running).
    pub queued: bool,
    /// `idle`, `running`, `done`, `failed`, or `never` before the first run.
    pub state: String,
    /// Started (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// Finished (ISO 8601).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    /// Objects listed.
    pub objects: i32,
    /// New or changed.
    pub added: i32,
    /// Vanished.
    pub vanished: i32,
    /// The listing stopped at the cap or the time budget.
    pub truncated: bool,
    /// The failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn status_view(queued: bool, s: Option<RunStatus>) -> IndexStatusView {
    match s {
        None => IndexStatusView {
            queued,
            state: "never".into(),
            started_at: None,
            finished_at: None,
            objects: 0,
            added: 0,
            vanished: 0,
            truncated: false,
            error: None,
        },
        Some(s) => IndexStatusView {
            queued,
            state: s.state,
            started_at: s.started_ms.map(iso),
            finished_at: s.finished_ms.map(iso),
            objects: s.objects,
            added: s.added,
            vanished: s.vanished,
            truncated: s.truncated,
            error: s.error,
        },
    }
}

fn section_of(raw: &str) -> MsgResult<Section> {
    Section::parse(raw).ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))
}

pub(crate) fn store_failed(e: impl std::fmt::Display) -> MsgError {
    tracing::error!(error = %e, "shelf store failed");
    ErrorCode::SrInternal.into()
}

fn attribution(provider: &str) -> Option<&'static str> {
    Some(match provider {
        "tmdb" => "This product uses the TMDB API but is not endorsed or certified by TMDB.",
        "anilist" => "Metadata from AniList.",
        "mangadex" => "Metadata and covers from MangaDex.",
        "mangaupdates" => "Metadata from MangaUpdates.",
        "comicvine" => "Metadata from Comic Vine.",
        _ => return None,
    })
}

fn clean_list(field: &str, v: &[String]) -> MsgResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for t in v {
        let t = t.trim().to_lowercase();
        if t.is_empty() {
            continue;
        }
        if t.chars().count() > 32 || t.contains(['/', '\\']) {
            return Err(invalid(
                field,
                "TOO_LONG",
                "Use short words (32 characters at most).",
            ));
        }
        if !out.contains(&t) {
            out.push(t);
        }
    }
    if out.len() > MAX_TAGS {
        return Err(invalid(field, "TOO_MANY", "At most 30 entries."));
    }
    Ok(out)
}

fn text(field: &str, v: &str, min: usize, max: usize) -> MsgResult<String> {
    let t = v.trim();
    let n = t.chars().count();
    if n < min || n > max {
        return Err(invalid(
            field,
            "BASE_TYPE_BAD_LENGTH",
            &format!("Use {min} to {max} characters."),
        ));
    }
    Ok(t.to_owned())
}

fn art_key(field: &str, k: &Option<Option<String>>) -> MsgResult<Option<Option<String>>> {
    match k {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(k)) => {
            safe_key(k, false).map_err(|e| invalid(field, "KEY_INVALID", &e.to_string()))?;
            Ok(Some(Some(k.clone())))
        }
    }
}

fn patch_from(section: Section, b: ShelfPatchBody) -> MsgResult<ShelfPatch> {
    let mut p = ShelfPatch::default();
    if let Some(t) = &b.title {
        p.title = Some(text("title", t, 1, 300)?);
    }
    if let Some(y) = b.year {
        if let Some(y) = y
            && !(1000..=2200).contains(&y)
        {
            return Err(invalid(
                "year",
                "BASE_TYPE_BAD_RANGE",
                "Use a year from 1000 to 2200.",
            ));
        }
        p.year = Some(y);
    }
    if let Some(k) = &b.kind {
        let ok = match section {
            Section::Cinema => ["movie", "series", "anime"].contains(&k.as_str()),
            Section::Library => ["manga", "comic", "book"].contains(&k.as_str()),
            Section::Music => false,
        };
        if !ok {
            return Err(invalid(
                "kind",
                "ENUM_TYPE_COERCE",
                "That kind does not exist in this section.",
            ));
        }
        p.kind = Some(k.clone());
    }
    if let Some(t) = &b.tags {
        p.tags = Some(clean_list("tags", t)?);
    }
    if let Some(g) = &b.genres {
        p.genres = Some(clean_list("genres", g)?);
    }
    if let Some(s) = &b.synopsis {
        p.synopsis = Some(text("synopsis", s, 0, 4000)?);
    }
    p.poster_key = art_key("poster_key", &b.poster_key)?;
    if section == Section::Cinema {
        p.backdrop_key = art_key("backdrop_key", &b.backdrop_key)?;
    }
    if section == Section::Music {
        if let Some(a) = &b.artist {
            p.artist = Some(text("artist", a, 0, 300)?);
        }
        if let Some(a) = &b.album {
            p.album = Some(text("album", a, 0, 300)?);
        }
        if let Some(n) = b.track_no {
            if let Some(n) = n
                && !(0..=9999).contains(&n)
            {
                return Err(invalid("track_no", "BASE_TYPE_BAD_RANGE", "Use 0 to 9999."));
            }
            p.track_no = Some(n);
        }
    }
    if let Some(d) = &b.details {
        let a = attribution(&d.provider)
            .ok_or_else(|| invalid("details", "ENUM_TYPE_COERCE", "Unknown metadata provider."))?;
        let id = text("details", &d.external_id, 1, 128)?;
        p.external_ids = Some(serde_json::json!({ d.provider.as_str(): id }).to_string());
        p.attribution = Some(a.to_owned());
    }
    Ok(p)
}

fn patch_error(e: PatchError) -> MsgError {
    match e {
        PatchError::NotFound => ErrorCode::SrNotFound.into(),
        PatchError::Invalid(m) => invalid("number", "ALREADY_TAKEN", &m),
        PatchError::Store(e) => store_failed(e),
    }
}

/// Presigned artwork URLs for a page, one client per mount.
pub(crate) struct Signer<'a> {
    d: &'a ScopeDeps,
    reach: &'a [Reach],
    clients: HashMap<i64, Option<sr_storage::ObjectStoreStorage>>,
}

impl<'a> Signer<'a> {
    pub(crate) fn new(d: &'a ScopeDeps, reach: &'a [Reach]) -> Self {
        Self {
            d,
            reach,
            clients: HashMap::new(),
        }
    }
    /// A URL for `key` on `mount_id`, or `None` when the mount has not passed its CORS check yet (the same rule as
    /// the presign route) or cannot sign.
    pub(crate) async fn url(&mut self, mount_id: i64, key: &str) -> Option<String> {
        let reach = self.reach;
        if !self.clients.contains_key(&mount_id) {
            let client = reach
                .iter()
                .find(|r| r.mount.id == mount_id && r.mount.cors_ok_at.is_some())
                .and_then(|r| r.mount.storage(&*self.d.sealer).ok());
            self.clients.insert(mount_id, client);
        }
        let mount = reach.iter().find(|r| r.mount.id == mount_id)?;
        let client = self.clients.get(&mount_id)?.as_ref()?;
        client
            .presign(PresignMethod::Get, &mount.mount.bucket_key(key), ART_TTL)
            .await
            .ok()
    }
}

async fn item_view(
    it: ShelfItem,
    reach: &[Reach],
    signer: &mut Signer<'_>,
) -> Option<ShelfItemView> {
    let r = reach.iter().find(|r| r.mount.id == it.mount_id)?;
    let poster_url = match &it.poster_key {
        Some(k) => signer.url(it.mount_id, k).await,
        None => None,
    };
    let backdrop_url = match &it.backdrop_key {
        Some(k) => signer.url(it.mount_id, k).await,
        None => None,
    };
    let external_ids: BTreeMap<String, String> =
        serde_json::from_str(&it.external_ids).unwrap_or_default();
    Some(ShelfItemView {
        id: it.id.to_string(),
        section: it.section.as_str().into(),
        kind: it.kind,
        title: it.title,
        subtitle: it.subtitle,
        year: it.year,
        source: source_of(r),
        poster_url,
        backdrop_url,
        tags: it.tags,
        genres: it.genres,
        synopsis: it.synopsis,
        attribution: it.attribution,
        external_ids,
        overridden: it.overridden,
        parts: it.parts,
        object_key: it.object_key,
        container: it.container,
        web_native: it.web_native,
    })
}

async fn reaches(
    d: &ScopeDeps,
    sh: &ShelfDeps,
    user: i64,
    section: Section,
) -> MsgResult<Vec<Reach>> {
    let facts: &dyn ScopeFacts = &*d.store;
    reachable(
        facts,
        &*d.mounts,
        &*sh.store,
        Viewer {
            id: user,
            staff: false,
        },
        section,
    )
    .await
    .map_err(store_failed)
}

/// A page of the section's shelves.
#[utoipa::path(
    get,
    path = "/shelves/{section}",
    params(
        ("section" = String, Path, description = "cinema, library or music"),
        ("q" = Option<String>, Query, description = "Title substring"),
        ("kind" = Option<String>, Query, description = "movie, series, anime, manga, comic or book"),
        ("mount" = Option<String>, Query, description = "Only this mount"),
        ("guild" = Option<String>, Query, description = "The server whose section is showing: its shelves, plus members' own shelves unless the server turned them off"),
        ("limit" = Option<u32>, Query, description = "At most 50"),
        ("offset" = Option<u32>, Query, description = "Offset"),
    ),
    responses(
        (status = 200, body = ShelfListView),
        (status = 400, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn list(
    State(state): State<AppState>,
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(sh): Ext<ShelfDeps>,
    Path(section): Path<String>,
    ApiQuery(q): ApiQuery<ListQuery>,
) -> MsgResult<Json<ShelfListView>> {
    let section = section_of(&section)?;
    let mut reach = reaches(&d, &sh, uid(&p), section).await?;
    if let Some(g) = q.guild.as_deref() {
        let guild = parse_row(g)?;
        let members = super::flags::member_shelves_allowed(&state, guild).await;
        reach.retain(|r| shown_in_guild(&r.access.scope, guild, members));
    }
    let sources: Vec<ShelfSourceView> = reach.iter().map(source_of).collect();
    let mount = match q.mount.as_deref() {
        Some(m) => Some(parse_row(m)?),
        None => None,
    };
    let ids: Vec<i64> = reach
        .iter()
        .map(|r| r.mount.id)
        .filter(|id| mount.is_none_or(|m| m == *id))
        .collect();
    let filter = ShelfFilter {
        q: q.q.map(|s| s.chars().take(100).collect()),
        kind: q.kind.filter(|k| {
            ["movie", "series", "anime", "manga", "comic", "book"].contains(&k.as_str())
        }),
        mount,
        limit: q.limit.unwrap_or(24).clamp(1, 50),
        offset: q.offset.unwrap_or(0).min(100_000),
    };
    let page = sh
        .store
        .list(section, &ids, &filter)
        .await
        .map_err(store_failed)?;
    let facts: &dyn ScopeFacts = &*d.store;
    let items = drop_delisted(facts, &*sh.store, section, page.items)
        .await
        .map_err(store_failed)?;
    let mut signer = Signer::new(&d, &reach);
    let mut views = Vec::with_capacity(items.len());
    for it in items {
        if let Some(v) = item_view(it, &reach, &mut signer).await {
            views.push(v);
        }
    }
    Ok(Json(ShelfListView {
        items: views,
        total: page.total,
        sources,
    }))
}

/// Whether a reach shows inside server `guild`'s section: the server's own (and attached) shelves always,
/// the viewer's and friends' shelves only when `member_shelves_in_guild` (combined) is on, other servers' never.
fn shown_in_guild(scope: &Scope, guild: i64, member_shelves: bool) -> bool {
    match scope {
        Scope::Guild(g) => *g == guild,
        Scope::Own | Scope::Friend { .. } => member_shelves,
        Scope::Staff => false,
    }
}

async fn detail_view(
    d: &ScopeDeps,
    reach: &[Reach],
    detail: ShelfDetail,
    can_edit: bool,
) -> MsgResult<ShelfDetailView> {
    let facts: &dyn ScopeFacts = &*d.store;
    let mount_id = detail.item.mount_id;
    // Parts the viewer's scope or a delisting hides are left out.
    let r = reach.iter().find(|r| r.mount.id == mount_id);
    let keys: Vec<String> = detail.parts.iter().map(|p| p.object_key.clone()).collect();
    let gone = facts
        .delisted(mount_id, &keys)
        .await
        .map_err(facts_failed)?;
    let parts: Vec<ShelfPartView> = detail
        .parts
        .into_iter()
        .filter(|p| {
            !gone.contains(&p.object_key)
                && r.is_some_and(|r| r.access.permits(&p.object_key).is_ok())
        })
        .map(|p| ShelfPartView {
            id: p.id.to_string(),
            season: p.season,
            number: p.number,
            title: p.title,
            object_key: p.object_key,
            container: p.container,
            web_native: p.web_native,
            size: p.size,
        })
        .collect();
    let mut signer = Signer::new(d, reach);
    let item = item_view(detail.item, reach, &mut signer)
        .await
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))?;
    Ok(ShelfDetailView {
        item,
        parts,
        can_edit,
    })
}

/// Whether the caller may edit rows of this mount (owner; on a guild shelf a manager).
async fn may_edit(d: &ScopeDeps, user: i64, mount: &Mount) -> MsgResult<bool> {
    let facts: &dyn ScopeFacts = &*d.store;
    sr_storage::scopes::may_manage(
        facts,
        Viewer {
            id: user,
            staff: false,
        },
        mount,
    )
    .await
    .map_err(facts_failed)
}

/// One item with its episodes or chapters.
#[utoipa::path(
    get,
    path = "/shelves/{section}/{shelf_item_id}",
    params(
        ("section" = String, Path, description = "cinema, library or music"),
        ("shelf_item_id" = String, Path, description = "Row snowflake"),
    ),
    responses(
        (status = 200, body = ShelfDetailView),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn get_item(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(sh): Ext<ShelfDeps>,
    Path((section, item_id)): Path<(String, String)>,
) -> MsgResult<Json<ShelfDetailView>> {
    let section = section_of(&section)?;
    let id = parse_row(&item_id)?;
    let reach = reaches(&d, &sh, uid(&p), section).await?;
    let ids: Vec<i64> = reach.iter().map(|r| r.mount.id).collect();
    let detail = sh
        .store
        .get(section, id, &ids)
        .await
        .map_err(store_failed)?
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))?;
    // A delisted single file is gone for the viewer.
    if let Some(k) = &detail.item.object_key {
        let facts: &dyn ScopeFacts = &*d.store;
        let hidden = facts
            .delisted(detail.item.mount_id, std::slice::from_ref(k))
            .await
            .map_err(facts_failed)?;
        if !hidden.is_empty() {
            return Err(ErrorCode::SrNotFound.into());
        }
    }
    let mount = reach
        .iter()
        .find(|r| r.mount.id == detail.item.mount_id)
        .map(|r| r.mount.clone())
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))?;
    let can_edit = may_edit(&d, uid(&p), &mount).await?;
    Ok(Json(detail_view(&d, &reach, detail, can_edit).await?))
}

/// The mount of an item the caller may edit; 404 for an item they cannot see, 403 when they can see it but not edit.
async fn editable(
    d: &ScopeDeps,
    sh: &ShelfDeps,
    user: i64,
    section: Section,
    id: i64,
) -> MsgResult<(Vec<Reach>, Mount)> {
    let reach = reaches(d, sh, user, section).await?;
    let mount_id = sh
        .store
        .mount_of(section, id)
        .await
        .map_err(store_failed)?
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))?;
    let mount = reach
        .iter()
        .find(|r| r.mount.id == mount_id)
        .map(|r| r.mount.clone())
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))?;
    if !may_edit(d, user, &mount).await? {
        return Err(ErrorCode::MissingPermissions.into());
    }
    Ok((reach, mount))
}

async fn reload(
    d: &ScopeDeps,
    sh: &ShelfDeps,
    reach: Vec<Reach>,
    section: Section,
    id: i64,
) -> MsgResult<Json<ShelfDetailView>> {
    let ids: Vec<i64> = reach.iter().map(|r| r.mount.id).collect();
    let detail = sh
        .store
        .get(section, id, &ids)
        .await
        .map_err(store_failed)?
        .ok_or_else(|| MsgError::from(ErrorCode::SrNotFound))?;
    Ok(Json(detail_view(d, &reach, detail, true).await?))
}

/// Edit an item's metadata. The edited fields become overrides a re-index leaves alone.
#[utoipa::path(
    patch,
    path = "/shelves/{section}/{shelf_item_id}",
    params(
        ("section" = String, Path, description = "cinema, library or music"),
        ("shelf_item_id" = String, Path, description = "Row snowflake"),
    ),
    request_body = ShelfPatchBody,
    responses(
        (status = 200, body = ShelfDetailView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn patch_item(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(sh): Ext<ShelfDeps>,
    Path((section, item_id)): Path<(String, String)>,
    ApiJson(body): ApiJson<ShelfPatchBody>,
) -> MsgResult<Json<ShelfDetailView>> {
    let section = section_of(&section)?;
    let id = parse_row(&item_id)?;
    let patch = patch_from(section, body)?;
    let (reach, _mount) = editable(&d, &sh, uid(&p), section, id).await?;
    sh.store
        .patch(section, id, &patch)
        .await
        .map_err(patch_error)?;
    reload(&d, &sh, reach, section, id).await
}

/// Edit one episode or chapter (title, number, season).
#[utoipa::path(
    patch,
    path = "/shelves/{section}/{shelf_item_id}/parts/{shelf_part_id}",
    params(
        ("section" = String, Path, description = "cinema or library"),
        ("shelf_item_id" = String, Path, description = "Series snowflake"),
        ("shelf_part_id" = String, Path, description = "Episode or chapter snowflake"),
    ),
    request_body = PartPatchBody,
    responses(
        (status = 200, body = ShelfDetailView),
        (status = 400, body = ErrorBody),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn patch_part(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(sh): Ext<ShelfDeps>,
    Path((section, item_id, part_id)): Path<(String, String, String)>,
    ApiJson(body): ApiJson<PartPatchBody>,
) -> MsgResult<Json<ShelfDetailView>> {
    let section = section_of(&section)?;
    if section == Section::Music {
        return Err(ErrorCode::SrNotFound.into());
    }
    let id = parse_row(&item_id)?;
    let part = parse_row(&part_id)?;
    let patch = PartPatch {
        title: match &body.title {
            Some(t) => Some(text("title", t, 0, 300)?),
            None => None,
        },
        number: match body.number {
            Some(n) if !(0.0..=9999.0).contains(&n) || !n.is_finite() => {
                return Err(invalid("number", "BASE_TYPE_BAD_RANGE", "Use 0 to 9999."));
            }
            n => n,
        },
        season: match body.season {
            Some(s) if !(0..=999).contains(&s) => {
                return Err(invalid("season", "BASE_TYPE_BAD_RANGE", "Use 0 to 999."));
            }
            s => s,
        },
    };
    let (reach, mount) = editable(&d, &sh, uid(&p), section, id).await?;
    sh.store
        .patch_part(section, id, part, mount.id, &patch)
        .await
        .map_err(patch_error)?;
    reload(&d, &sh, reach, section, id).await
}

/// The mount, if the caller manages it. 404 for a mount they cannot reach, 403 for one they can but do not manage.
pub(super) async fn managed_mount(d: &ScopeDeps, user: i64, mount_id: &str) -> MsgResult<Mount> {
    let id = parse_row(mount_id)?;
    let mount = match d.mounts.by_id(id).await {
        Ok(Some(m)) => m,
        Ok(None) => return Err(ErrorCode::SrNotFound.into()),
        Err(e) => return Err(store_failed(e)),
    };
    if mount.id == INSTANCE_MOUNT_ID || mount.status == sr_storage::mounts::MountStatus::Disabled {
        return Err(ErrorCode::SrNotFound.into());
    }
    let facts: &dyn ScopeFacts = &*d.store;
    let viewer = Viewer {
        id: user,
        staff: false,
    };
    if may_edit(d, user, &mount).await? {
        return Ok(mount);
    }
    match sr_storage::scopes::access(facts, viewer, &mount).await {
        Ok(_) => Err(ErrorCode::MissingPermissions.into()),
        Err(sr_storage::scopes::ResolveError::Refused(r)) => Err(refusal(r)),
        Err(sr_storage::scopes::ResolveError::Facts(e)) => Err(facts_failed(e)),
    }
}

/// Queue an index run of a mount (the Re-index button). Answers with the last run; `queued` is false when a run
/// is already queued or running.
#[utoipa::path(
    post,
    path = "/storage/mounts/{mount_id}/index",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses(
        (status = 202, body = IndexStatusView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn start_index(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(sh): Ext<ShelfDeps>,
    Path(mount_id): Path<String>,
) -> MsgResult<(StatusCode, Json<IndexStatusView>)> {
    let mount = managed_mount(&d, uid(&p), &mount_id).await?;
    let queued = sh.index.request(mount.id).await.map_err(store_failed)?;
    let status = sh.index.status(mount.id).await.map_err(store_failed)?;
    Ok((StatusCode::ACCEPTED, Json(status_view(queued, status))))
}

/// The last index run of a mount.
#[utoipa::path(
    get,
    path = "/storage/mounts/{mount_id}/index",
    params(("mount_id" = String, Path, description = "Mount snowflake")),
    responses(
        (status = 200, body = IndexStatusView),
        (status = 403, body = ErrorBody),
        (status = 404, body = ErrorBody),
    ),
    security(("session" = []))
)]
pub async fn index_status(
    SessionOnly(p): SessionOnly,
    Ext(d): Ext<ScopeDeps>,
    Ext(sh): Ext<ShelfDeps>,
    Path(mount_id): Path<String>,
) -> MsgResult<Json<IndexStatusView>> {
    let mount = managed_mount(&d, uid(&p), &mount_id).await?;
    let status = sh.index.status(mount.id).await.map_err(store_failed)?;
    Ok(Json(status_view(false, status)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn member_shelves_in_guild_filters_the_server_section() {
        use super::shown_in_guild;
        use sr_storage::scopes::Scope;
        let g = 7;
        assert!(shown_in_guild(&Scope::Guild(7), g, false));
        assert!(!shown_in_guild(&Scope::Guild(8), g, true));
        assert!(shown_in_guild(&Scope::Own, g, true));
        assert!(shown_in_guild(&Scope::Friend { owner: 3 }, g, true));
        assert!(!shown_in_guild(&Scope::Own, g, false));
        assert!(!shown_in_guild(&Scope::Friend { owner: 3 }, g, false));
        assert!(!shown_in_guild(&Scope::Staff, g, true));
    }

    use super::*;

    #[test]
    fn attribution_is_server_side_and_known_providers_only() {
        assert!(attribution("tmdb").unwrap().contains("TMDB"));
        assert!(attribution("evil").is_none());
        let body = ShelfPatchBody {
            details: Some(DetailsBody {
                provider: "tmdb".into(),
                external_id: "603".into(),
            }),
            ..ShelfPatchBody::default()
        };
        let p = patch_from(Section::Cinema, body).unwrap();
        assert_eq!(p.external_ids.as_deref(), Some(r#"{"tmdb":"603"}"#));
        assert!(p.attribution.unwrap().contains("not endorsed"));
        let bad = ShelfPatchBody {
            details: Some(DetailsBody {
                provider: "nope".into(),
                external_id: "1".into(),
            }),
            ..ShelfPatchBody::default()
        };
        assert!(patch_from(Section::Cinema, bad).is_err());
    }

    #[test]
    fn the_editor_validates_fields_per_section() {
        let k = |s: &str| ShelfPatchBody {
            kind: Some(s.into()),
            ..ShelfPatchBody::default()
        };
        assert!(patch_from(Section::Cinema, k("anime")).is_ok());
        assert!(patch_from(Section::Cinema, k("manga")).is_err());
        assert!(patch_from(Section::Library, k("comic")).is_ok());
        assert!(patch_from(Section::Music, k("movie")).is_err());
        let t = ShelfPatchBody {
            title: Some("  ".into()),
            ..ShelfPatchBody::default()
        };
        assert!(patch_from(Section::Cinema, t).is_err());
        let tags = ShelfPatchBody {
            tags: Some(vec![" Sci-Fi ".into(), "sci-fi".into(), "".into()]),
            ..ShelfPatchBody::default()
        };
        assert_eq!(
            patch_from(Section::Cinema, tags).unwrap().tags.unwrap(),
            vec!["sci-fi".to_owned()]
        );
        let key = ShelfPatchBody {
            poster_key: Some(Some("../etc/passwd".into())),
            ..ShelfPatchBody::default()
        };
        assert!(patch_from(Section::Cinema, key).is_err());
        let year: ShelfPatchBody = serde_json::from_str(r#"{"year":null}"#).unwrap();
        assert_eq!(year.year, Some(None));
        let none: ShelfPatchBody = serde_json::from_str("{}").unwrap();
        assert_eq!(none.year, None);
        let far = ShelfPatchBody {
            year: Some(Some(5000)),
            ..ShelfPatchBody::default()
        };
        assert!(patch_from(Section::Cinema, far).is_err());
        // Music keeps artist and track number, and ignores a backdrop.
        let m = ShelfPatchBody {
            artist: Some("A".into()),
            track_no: Some(Some(3)),
            backdrop_key: Some(Some("b.jpg".into())),
            ..ShelfPatchBody::default()
        };
        let p = patch_from(Section::Music, m).unwrap();
        assert_eq!(
            (p.artist.as_deref(), p.track_no),
            (Some("A"), Some(Some(3)))
        );
        assert!(p.backdrop_key.is_none());
    }
}
