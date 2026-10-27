//! The real [`sr_tv::TvCatalog`]: Live TV builds its pools from the Cinema catalog repository.
//!
//! Rules:
//! * Only published, non-merged titles and published episodes are offered (the repository's `Public`
//!   scope), and only when they have an asset and a runtime: a film needs its own `asset_id` and
//!   `runtime_min`, an episode its own asset and `runtime_min` (falling back to the title's).
//! * Block length comes from the asset (`video_assets.duration_ms`, G-09) when the asset has been probed;
//!   the catalog `runtime_min` is the fallback. Pool runtime filters use the same length.
//! * Age-restricted titles are offered like any other (D-184); the pool filter decides whether a channel
//!   wants them, and opening the stream is what needs age confirmation.
//! * An anime without episodes is a film. Genres and tags are lower case (the catalog lower-cases both on
//!   write), and the item carries both.
//! * Order: the order of `title_ids` when given, else by title; episodes by season and number.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use sr_cinema::model::{
    AgeFilter, BrowseQuery, EpisodeRecord, Scope, SortKey, TitleKind, TitleRecord,
};
use sr_cinema::repo::{CinemaRepository, EpisodeQuery};
use sr_storage::mounts::{MountOwner, MountRepo, MountStatus};
use sr_storage::scopes::{Grant, ScopeFacts};
use sr_tv::model::{MediaKind, PoolFilter, PoolSource, Restricted};
use sr_tv::{CatalogItem, TitleRef, TvCatalog, TvError, TvResult};

/// Probed lengths of video assets, in milliseconds.
#[async_trait]
pub trait AssetDurations: Send + Sync {
    /// Asset id to duration for every asset that has a positive probed duration; others are absent.
    async fn durations(&self, asset_ids: &[i64]) -> TvResult<HashMap<i64, i64>>;
}

/// [`AssetDurations`] over `video_assets.duration_ms`.
pub struct PgAssetDurations {
    pool: sqlx::PgPool,
}

impl PgAssetDurations {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AssetDurations for PgAssetDurations {
    async fn durations(&self, asset_ids: &[i64]) -> TvResult<HashMap<i64, i64>> {
        if asset_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT id, duration_ms FROM video_assets WHERE id = ANY($1) AND duration_ms > 0",
        )
        .bind(asset_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| TvError::Catalog(e.to_string()))?;
        Ok(rows.into_iter().collect())
    }
}

/// The mount every catalog row has always lived in: the instance bucket (migration 1420).
pub const INSTANCE_MOUNT_ID: i64 = 1;

/// Which mounts a channel source may draw from (BYOS-TV-001): a server's own live mounts plus the platform
/// mounts attached to it, or a member's own live mounts. A friend's mount is never a source.
pub struct ShelfMounts {
    repo: Arc<dyn MountRepo>,
    facts: Arc<dyn ScopeFacts>,
}

impl ShelfMounts {
    pub fn new(repo: Arc<dyn MountRepo>, facts: Arc<dyn ScopeFacts>) -> Self {
        Self { repo, facts }
    }

    /// The mount ids, or an error (never a guess) when a store fails.
    pub async fn of(&self, source: PoolSource) -> TvResult<Vec<i64>> {
        let store = |e: String| TvError::Catalog(e);
        let owned = |owner: MountOwner| async move {
            self.repo
                .for_owner(owner)
                .await
                .map(|v| {
                    v.into_iter()
                        .filter(|m| m.status == MountStatus::Active)
                        .map(|m| m.id)
                        .collect::<Vec<_>>()
                })
                .map_err(|e| store(e.to_string()))
        };
        match source {
            PoolSource::Instance => Ok(Vec::new()),
            PoolSource::Personal { user_id } => owned(MountOwner::User(user_id)).await,
            PoolSource::Guild { guild_id } => {
                let mut ids = owned(MountOwner::Guild(guild_id)).await?;
                let platform = self
                    .repo
                    .list_enabled()
                    .await
                    .map_err(|e| store(e.to_string()))?;
                for m in platform
                    .into_iter()
                    .filter(|m| m.owner == MountOwner::Platform)
                {
                    let guilds = self
                        .facts
                        .attached_guilds(m.id)
                        .await
                        .map_err(|e| store(e.to_string()))?;
                    if guilds.contains(&guild_id) {
                        ids.push(m.id);
                    }
                }
                Ok(ids)
            }
        }
    }
}

/// Who may watch a server's channel (BYOS-TV-001): a member of the server whose roles hold the `view_cinema`
/// storage grant (the owner and `ADMINISTRATOR` hold every grant). One answer per server, so the guide is the
/// same for everyone who may watch.
pub struct GuildChannelAccess {
    facts: Arc<dyn ScopeFacts>,
}

impl GuildChannelAccess {
    pub fn new(facts: Arc<dyn ScopeFacts>) -> Self {
        Self { facts }
    }
}

#[async_trait]
impl sr_tv::ChannelAccess for GuildChannelAccess {
    async fn may_watch(&self, viewer: sr_tv::Viewer, pool: PoolSource) -> TvResult<bool> {
        let PoolSource::Guild { guild_id } = pool else {
            return Ok(false);
        };
        let grants = sr_storage::scopes::guild_grants(self.facts.as_ref(), guild_id, viewer.user_id)
            .await
            .map_err(|e| TvError::Catalog(e.to_string()))?;
        Ok(grants.has(Grant::ViewCinema))
    }
}

/// [`TvCatalog`] over the catalog repository.
pub struct RepoTvCatalog {
    repo: Arc<dyn CinemaRepository>,
    durations: Option<Arc<dyn AssetDurations>>,
    shelves: Option<ShelfMounts>,
}

/// Titles read per pool at most (ten pages of a hundred).
const MAX_TITLES: u32 = 1000;
const PAGE: u32 = 100;

impl RepoTvCatalog {
    pub fn new(repo: Arc<dyn CinemaRepository>) -> Self {
        Self {
            repo,
            durations: None,
            shelves: None,
        }
    }

    /// Lets server and personal channels find their mounts. Without it they draw from nothing.
    pub fn with_shelves(mut self, shelves: ShelfMounts) -> Self {
        self.shelves = Some(shelves);
        self
    }

    /// Takes block lengths from the probed assets instead of the catalog runtimes.
    pub fn with_asset_durations(mut self, durations: Arc<dyn AssetDurations>) -> Self {
        self.durations = Some(durations);
        self
    }

    /// Replaces each item's length with its asset's probed duration when there is one.
    async fn apply_durations(&self, items: &mut [CatalogItem]) -> TvResult<()> {
        let Some(d) = &self.durations else {
            return Ok(());
        };
        let ids: Vec<i64> = items.iter().map(|i| i.asset_id).collect();
        let found = d.durations(&ids).await?;
        for i in items {
            if let Some(ms) = found.get(&i.asset_id) {
                i.duration_ms = *ms;
            }
        }
        Ok(())
    }
}

fn failed(e: sr_cinema::CinemaError) -> TvError {
    TvError::Catalog(e.to_string())
}

fn kind_of(t: &TitleRecord) -> MediaKind {
    match t.kind {
        TitleKind::Movie => MediaKind::Movie,
        TitleKind::Series => MediaKind::Series,
        // An anime with seasons is played by episode; without, it is a film.
        TitleKind::Anime if t.episode_count > 0 || t.season_count > 0 => MediaKind::Anime,
        TitleKind::Anime => MediaKind::Movie,
    }
}

fn minutes_ms(m: Option<i32>) -> Option<i64> {
    m.filter(|m| *m > 0).map(|m| i64::from(m) * 60_000)
}

fn labels(t: &TitleRecord) -> (Vec<String>, Vec<String>) {
    let genres: Vec<String> = t.genres.iter().map(|g| g.to_lowercase()).collect();
    let mut tags: Vec<String> = genres
        .iter()
        .cloned()
        .chain(t.tags.iter().map(|g| g.to_lowercase()))
        .collect();
    tags.sort();
    tags.dedup();
    (genres, tags)
}

fn film_item(t: &TitleRecord) -> Option<CatalogItem> {
    let (genres, tags) = labels(t);
    Some(CatalogItem {
        title_id: t.id,
        episode_id: None,
        asset_id: t.asset_id?,
        title: t.title.clone(),
        episode_title: None,
        media_kind: MediaKind::Movie,
        season_number: None,
        episode_number: None,
        year: t.year,
        duration_ms: minutes_ms(t.runtime_min)?,
        age_restricted: t.age_restricted,
        genres,
        tags,
        mount_id: INSTANCE_MOUNT_ID,
        playable: true,
    })
}

fn episode_item(t: &TitleRecord, e: &EpisodeRecord) -> Option<CatalogItem> {
    if !(e.published && e.title_published && !e.title_merged) {
        return None;
    }
    let (genres, tags) = labels(t);
    Some(CatalogItem {
        title_id: t.id,
        episode_id: Some(e.id),
        asset_id: e.asset_id?,
        title: t.title.clone(),
        episode_title: (!e.title.is_empty()).then(|| e.title.clone()),
        media_kind: kind_of(t),
        season_number: Some(e.season_number),
        episode_number: Some(e.number),
        year: t.year,
        duration_ms: minutes_ms(e.runtime_min).or_else(|| minutes_ms(t.runtime_min))?,
        age_restricted: t.age_restricted,
        genres,
        tags,
        mount_id: INSTANCE_MOUNT_ID,
        playable: true,
    })
}

impl RepoTvCatalog {
    async fn items_of(&self, t: &TitleRecord) -> TvResult<Vec<CatalogItem>> {
        let mut items: Vec<CatalogItem> = if kind_of(t) == MediaKind::Movie {
            film_item(t).into_iter().collect()
        } else {
            self.repo
                .list_episodes(t.id, &EpisodeQuery::default(), Scope::Public)
                .await
                .map_err(failed)?
                .iter()
                .filter_map(|e| episode_item(t, e))
                .collect()
        };
        self.apply_durations(&mut items).await?;
        Ok(items)
    }

    async fn titles(&self, f: &PoolFilter, channel_restricted: bool) -> TvResult<Vec<TitleRecord>> {
        if !f.title_ids.is_empty() {
            return self
                .repo
                .titles_by_ids(&f.title_ids, Scope::Public, true)
                .await
                .map_err(failed);
        }
        let mut q = BrowseQuery {
            kind: match f.kinds.as_slice() {
                [MediaKind::Movie] => Some(TitleKind::Movie),
                [MediaKind::Series] => Some(TitleKind::Series),
                _ => None,
            },
            genres: f.genres.clone(),
            tags: f.tags.clone(),
            year_from: f.year_from,
            year_to: f.year_to,
            age: match f.restricted_rule(channel_restricted) {
                Restricted::Exclude => AgeFilter::Hide,
                Restricted::Include => AgeFilter::Include,
                Restricted::Only => AgeFilter::Only,
            },
            collection: f.collection_id,
            sort: SortKey::Title,
            limit: PAGE,
            ..BrowseQuery::default()
        };
        let mut out = Vec::new();
        while q.offset < MAX_TITLES {
            let page = self.repo.browse(&q, Scope::Public).await.map_err(failed)?;
            let n = page.items.len();
            out.extend(page.items);
            if n < PAGE as usize {
                break;
            }
            q.offset += PAGE;
        }
        Ok(out)
    }

    async fn collect(
        &self,
        f: &PoolFilter,
        channel_restricted: bool,
        limit: usize,
    ) -> TvResult<Vec<CatalogItem>> {
        let mut out = Vec::new();
        for t in self.titles(f, channel_restricted).await? {
            for item in self.items_of(&t).await? {
                if f.matches(&item, channel_restricted) {
                    out.push(item);
                }
            }
            if out.len() >= limit {
                break;
            }
        }
        out.truncate(limit);
        Ok(out)
    }
}

#[async_trait]
impl TvCatalog for RepoTvCatalog {
    /// The instance catalog is the only thing `pool` reads (the published rows, all in the instance bucket), so a
    /// server or personal channel gets it exactly when the instance bucket is among its mounts. Items found in a
    /// server's own or a member's mount are not offered yet: they need a probed runtime and the byte-range
    /// renderer (see the handoff), and an item that cannot air is never scheduled.
    async fn mounts_of(&self, source: PoolSource) -> TvResult<Option<Vec<i64>>> {
        if source == PoolSource::Instance {
            return Ok(None);
        }
        match &self.shelves {
            Some(s) => s.of(source).await.map(Some),
            None => Ok(Some(Vec::new())),
        }
    }

    async fn pool(
        &self,
        filter: &PoolFilter,
        channel_restricted: bool,
        limit: usize,
    ) -> TvResult<Vec<CatalogItem>> {
        self.collect(filter, channel_restricted, limit).await
    }

    async fn item(&self, title_id: i64, episode_id: Option<i64>) -> TvResult<Option<CatalogItem>> {
        let Some(t) = self
            .repo
            .get_title(title_id, Scope::Public)
            .await
            .map_err(failed)?
        else {
            return Ok(None);
        };
        let found = match episode_id {
            None => {
                if kind_of(&t) == MediaKind::Movie {
                    film_item(&t)
                } else {
                    None
                }
            }
            Some(id) => self
                .repo
                .get_episode(id)
                .await
                .map_err(failed)?
                .filter(|e| e.title_id == title_id)
                .and_then(|e| episode_item(&t, &e)),
        };
        let Some(item) = found else {
            return Ok(None);
        };
        let mut items = [item];
        self.apply_durations(&mut items).await?;
        let [item] = items;
        Ok(Some(item))
    }

    async fn title_refs(&self, limit: usize) -> TvResult<Vec<TitleRef>> {
        // Titles only (no episodes): enough to match a show name. Whether one can air is decided when a
        // slot is resolved.
        let all = PoolFilter {
            restricted: Some(Restricted::Include),
            ..PoolFilter::default()
        };
        let mut out: Vec<TitleRef> = self
            .titles(&all, true)
            .await?
            .iter()
            .map(|t| TitleRef {
                title_id: t.id,
                title: t.title.clone(),
                year: t.year,
                media_kind: kind_of(t),
            })
            .collect();
        out.truncate(limit);
        Ok(out)
    }

    async fn bumpers(&self, limit: usize) -> TvResult<Vec<CatalogItem>> {
        let filter = PoolFilter {
            tags: vec!["filler".into()],
            max_runtime_min: Some(10),
            restricted: Some(Restricted::Exclude),
            ..PoolFilter::default()
        };
        self.collect(&filter, false, limit).await
    }
}

/// The Live TV slate asset id (D-269): the platform setting `tv.slate_asset` (written by `sr-cli tv slate` or
/// Admin -> Limits) wins, the `SR_TV_SLATE_ASSET` environment value is the bootstrap fallback. Read through
/// [`sr_tv::TvService::with_slate_source`], so a change applies within seconds and needs no restart.
pub struct PgSlateSetting {
    pool: sqlx::PgPool,
    fallback: Option<i64>,
}

impl PgSlateSetting {
    pub fn new(pool: sqlx::PgPool, fallback: Option<i64>) -> Self {
        Self { pool, fallback }
    }
}

/// An asset id in text (digits); anything else, empty included, is no id.
pub fn parse_asset_id(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok().filter(|n| *n > 0)
}

#[async_trait]
impl sr_tv::SlateSource for PgSlateSetting {
    async fn asset_id(&self) -> Option<i64> {
        let stored = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT value FROM platform_settings WHERE key = $1",
        )
        .bind(sr_admin::settings::TV_SLATE_ASSET)
        .fetch_optional(&self.pool)
        .await;
        match stored {
            Ok(v) => v
                .as_ref()
                .and_then(|v| v.as_str())
                .and_then(parse_asset_id)
                .or(self.fallback),
            Err(e) => {
                tracing::warn!(error = %e, "could not read the tv.slate_asset setting");
                self.fallback
            }
        }
    }
}
