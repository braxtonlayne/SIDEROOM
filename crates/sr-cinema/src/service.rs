//! The Cinema catalog service: reads for members, writes for publishers.
//!
//! * Members ([`Viewer`]) see published, non-merged titles, age-restricted ones included
//!   (D-184); [`AgeFilter`] and the viewer's own `hide_restricted` preference can narrow a
//!   listing. Opening a restricted title ([`CinemaService::resolve_play`]) needs the account
//!   to have confirmed it is 18 or older, otherwise the answer is
//!   [`CinemaError::AgeConfirmationRequired`].
//! * Publishers ([`Actor`]) need the platform permissions of doc 07: `PUBLISH_MEDIA` to create,
//!   edit and publish, `DELETE_CONTENT` to delete (unpublish takes either), both to merge,
//!   `MANAGE_COLLECTIONS` for collections.
//! * A published title always has something playable: removing the last playable thing
//!   (unlinking the asset, unpublishing or deleting the last published episode) unpublishes it.
//! * Artwork URLs are HMAC-signed `/cdn/cinema/...` links, rounded to the hour so a page
//!   fetched twice in a row yields cacheable, identical URLs.

use std::collections::BTreeMap;
use std::sync::Arc;

use sr_core::{Clock, PlatformPermissions as P};
use sr_storage::cdn::CdnSigner;
use sr_storage::{ObjectStorage, PresignMethod};

use crate::error::{CinemaError, CinemaResult};
use crate::model::*;
use crate::repo::{CinemaRepository, EpisodeQuery, NewTitle};
use crate::rows::{
    self, CatalogObserver, KEY_ANIMATION_BANNER, KEY_ANIME_ROOM, KEY_BECAUSE_YOU_WATCHED,
    KEY_CONTINUE_WATCHING, KEY_FILMS, KEY_FRIENDS_WATCHING, KEY_HERO, KEY_NEW_THIS_WEEK,
    KEY_ONE_MORE_EPISODE, KEY_RECENTLY_ADDED, KEY_RECENTLY_EXPLORED, KEY_REQUESTS_FULFILLED,
    KEY_TRENDING, RowContext, RowProvider,
};
use crate::rules;
use crate::search::{IndexedTitle, TitleIndex};

/// Snowflake source.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Hours a signed URL stays valid, at least (rounded up to the hour).
const URL_HOURS: u64 = 6;
/// Longest merge redirect chain followed.
const MAX_REDIRECTS: usize = 3;
/// Titles per Discover row.
const DISCOVER_ROW: u32 = 20;
/// Titles in the fanned collage of the animation banner.
const BANNER_ROW: u32 = 5;
/// Titles in the hero carousel.
const HERO_ROW: u32 = 8;
/// How long a presigned artwork upload URL works.
const ARTWORK_URL_TTL: std::time::Duration = std::time::Duration::from_secs(3600);
/// Longest unpaged episode list.
const ALL_EPISODES: u32 = 5000;

/// A member reading the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    pub user_id: i64,
    /// The account confirmed it is 18 or older (`accounts.age_confirmed_at`).
    pub adult: bool,
    /// The viewer's `cinema.hide_age_restricted` preference: restricted titles are left out of
    /// search, rows and (by default) browse.
    pub hide_restricted: bool,
}

impl Viewer {
    /// Whether search, rows and facets include restricted titles.
    pub fn include_restricted(self) -> bool {
        !self.hide_restricted
    }

    /// The `age` filter used when the request names none.
    pub fn default_age(self) -> AgeFilter {
        if self.hide_restricted {
            AgeFilter::Hide
        } else {
            AgeFilter::Include
        }
    }
}

/// A publisher changing the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub user_id: i64,
    pub permissions: P,
}

impl Actor {
    fn need(self, perm: P, name: &'static str) -> CinemaResult<()> {
        if self.permissions.has(perm) {
            Ok(())
        } else {
            Err(CinemaError::MissingPermission(name))
        }
    }

    pub(crate) fn publish(self) -> CinemaResult<()> {
        self.need(P::PUBLISH_MEDIA, "PUBLISH_MEDIA")
    }

    fn delete(self) -> CinemaResult<()> {
        self.need(P::DELETE_CONTENT, "DELETE_CONTENT")
    }

    fn collections(self) -> CinemaResult<()> {
        self.need(P::MANAGE_COLLECTIONS, "MANAGE_COLLECTIONS")
    }

    /// Publish or unpublish: `PUBLISH_MEDIA` for either, `DELETE_CONTENT` also unpublishes.
    fn toggle(self, published: bool) -> CinemaResult<()> {
        if published {
            self.publish()
        } else if self.publish().is_err() {
            self.delete()
        } else {
            Ok(())
        }
    }
}

/// What to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayRef {
    /// A title (film, or the first playable episode of an episodic title) and optionally a cut.
    Title {
        id: i64,
        variant: Option<i64>,
    },
    Episode(i64),
}

/// What [`CinemaService::authorize_asset`] answers: the catalog owner of an asset the viewer
/// may open, and the skip windows that go with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetAccess {
    pub title_id: i64,
    pub episode_id: Option<i64>,
    pub markers: Vec<Marker>,
}

#[derive(Clone)]
pub struct CinemaService {
    pub(crate) repo: Arc<dyn CinemaRepository>,
    index: Option<Arc<TitleIndex>>,
    signer: CdnSigner,
    clock: Arc<dyn Clock>,
    ids: IdSource,
    providers: Vec<Arc<dyn RowProvider>>,
    observers: Vec<Arc<dyn CatalogObserver>>,
    storage: Option<Arc<dyn ObjectStorage>>,
}

impl CinemaService {
    pub fn new(
        repo: Arc<dyn CinemaRepository>,
        signer: CdnSigner,
        clock: Arc<dyn Clock>,
        ids: IdSource,
    ) -> Self {
        Self {
            repo,
            index: None,
            signer,
            clock,
            ids,
            providers: Vec::new(),
            observers: Vec::new(),
            storage: None,
        }
    }

    /// Attach the search index. Call [`Self::reindex_all`] after opening it.
    pub fn with_index(mut self, index: Arc<TitleIndex>) -> Self {
        self.index = Some(index);
        self
    }

    /// Register the source of a user-data row (see [`crate::rows`]).
    pub fn with_provider(mut self, provider: Arc<dyn RowProvider>) -> Self {
        self.providers.retain(|p| p.key() != provider.key());
        self.providers.push(provider);
        self
    }

    /// Be told about merges and deletions that other tables must follow.
    pub fn with_observer(mut self, observer: Arc<dyn CatalogObserver>) -> Self {
        self.observers.push(observer);
        self
    }

    /// Object storage for artwork uploads (presigned `PUT`).
    pub fn with_storage(mut self, storage: Arc<dyn ObjectStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    // ---- presentation --------------------------------------------------

    fn expires_at(&self) -> u64 {
        let now = u64::try_from(self.clock.now_unix_ms() / 1000).unwrap_or(0);
        (now / 3600 + URL_HOURS + 1) * 3600
    }

    fn url(&self, key: &str, expires_at: u64) -> String {
        self.signer.signed_url(&format!("/cdn/{key}"), expires_at)
    }

    pub(crate) fn title(&self, r: &TitleRecord, exp: u64, staff: bool) -> Title {
        Title {
            id: r.id,
            kind: r.kind,
            title: r.title.clone(),
            original_title: r.original_title.clone(),
            alt_titles: r.alt_titles.clone(),
            year: r.year,
            runtime_min: r.runtime_min,
            content_rating: r.content_rating.clone(),
            synopsis: r.synopsis.clone(),
            tagline: r.tagline.clone(),
            genres: r.genres.clone(),
            tags: r.tags.clone(),
            languages: r.languages.clone(),
            subtitle_languages: r.subtitle_languages.clone(),
            cast: r.cast.clone(),
            directors: r.directors.clone(),
            writers: r.writers.clone(),
            studios: r.studios.clone(),
            crew: r.crew.clone(),
            age_restricted: r.age_restricted,
            poster_url: r.poster_key.as_deref().map(|k| self.url(k, exp)),
            backdrop_url: r.backdrop_key.as_deref().map(|k| self.url(k, exp)),
            logo_url: r.logo_key.as_deref().map(|k| self.url(k, exp)),
            external_ids: r.external_ids.clone(),
            season_count: r.season_count,
            episode_count: r.episode_count,
            has_trailer: r.has_trailer,
            playable: r.asset_id.is_some() || r.episode_count > 0,
            published: r.published,
            published_at_ms: r.published_at_ms,
            last_content_at_ms: r.last_content_at_ms,
            created_at_ms: r.created_at_ms,
            updated_at_ms: r.updated_at_ms,
            asset_id: if staff { r.asset_id } else { None },
        }
    }

    fn titles(&self, recs: &[TitleRecord], staff: bool) -> Vec<Title> {
        let exp = self.expires_at();
        recs.iter().map(|r| self.title(r, exp, staff)).collect()
    }

    fn season(&self, r: &SeasonRecord, exp: u64) -> Season {
        Season {
            id: r.id,
            title_id: r.title_id,
            number: r.number,
            name: r.name.clone(),
            synopsis: r.synopsis.clone(),
            poster_url: r.poster_key.as_deref().map(|k| self.url(k, exp)),
            episode_count: r.episode_count,
        }
    }

    fn episode_wire(&self, r: &EpisodeRecord, exp: u64, staff: bool) -> Episode {
        Episode {
            id: r.id,
            title_id: r.title_id,
            season_id: r.season_id,
            season_number: r.season_number,
            number: r.number,
            absolute_number: r.absolute_number,
            title: r.title.clone(),
            synopsis: r.synopsis.clone(),
            air_date: r.air_date.clone(),
            runtime_min: r.runtime_min,
            still_url: r.still_key.as_deref().map(|k| self.url(k, exp)),
            playable: r.asset_id.is_some(),
            published: r.published,
            published_at_ms: r.published_at_ms,
            asset_id: if staff { r.asset_id } else { None },
        }
    }

    fn variant_wire(r: &VariantRecord, staff: bool) -> Variant {
        Variant {
            id: r.id,
            title_id: r.title_id,
            label: r.label.clone(),
            notes: r.notes.clone(),
            runtime_min: r.runtime_min,
            playable: r.asset_id.is_some(),
            position: r.position,
            asset_id: if staff { r.asset_id } else { None },
        }
    }

    /// `gated`: the title is restricted and the viewer has not confirmed their age, so the
    /// asset is withheld from members. Staff always see it.
    fn trailer_wire(r: &TrailerRecord, gated: bool, staff: bool) -> Trailer {
        let show = staff || !gated;
        Trailer {
            id: r.id,
            title_id: r.title_id,
            label: r.label.clone(),
            kind: r.kind,
            language: r.language.clone(),
            playable: r.asset_id.is_some(),
            age_gated: gated && !staff,
            asset_id: if show { r.asset_id } else { None },
        }
    }

    fn marker_wire(r: &MarkerRecord, staff: bool) -> Marker {
        Marker {
            kind: r.kind,
            start_ms: r.start_ms,
            end_ms: r.end_ms,
            id: staff.then_some(r.id),
            source: staff.then(|| r.source.clone()),
        }
    }

    fn collection_wire(r: &CollectionRecord) -> Collection {
        Collection {
            id: r.id,
            slug: r.slug.clone(),
            name: r.name.clone(),
            eyebrow: r.eyebrow.clone(),
            blurb: r.blurb.clone(),
            display: r.display,
            surfaces: r.surfaces.clone(),
            position: r.position,
            published: r.published,
            item_count: r.item_count,
        }
    }

    // ---- members: catalog ---------------------------------------------

    /// Browse with filters, sort and keyset pagination (CIN-DATA-003).
    pub async fn browse(
        &self,
        _v: Viewer,
        q: BrowseQuery,
        cursor: Option<&str>,
    ) -> CinemaResult<TitlePage> {
        let mut q = rules::browse(q)?;
        q.published = None;
        q.title_contains = None;
        q.offset = 0;
        let desc = q.is_descending();
        q.after = cursor
            .map(|c| rules::decode_cursor(c, q.sort, desc))
            .transpose()?;
        let limit = q.limit;
        // One extra row tells whether there is a next page.
        q.limit = limit + 1;
        let mut rows = self.repo.browse(&q, Scope::Public).await?;
        let mut next_cursor = None;
        if rows.items.len() > limit as usize {
            rows.items.truncate(limit as usize);
            next_cursor = rows
                .items
                .last()
                .and_then(|r| r.sort_value.as_ref().map(|s| (s, r.id)))
                .map(|(s, id)| rules::encode_cursor(q.sort, desc, s, id));
        }
        Ok(TitlePage {
            items: self.titles(&rows.items, false),
            total: rows.total,
            next_cursor,
        })
    }

    /// A title visible to members, following a merge redirect.
    async fn visible_title(&self, id: i64) -> CinemaResult<TitleRecord> {
        if let Some(r) = self.repo.get_title(id, Scope::Public).await? {
            return Ok(r);
        }
        let mut cur = id;
        for _ in 0..MAX_REDIRECTS {
            match self.repo.get_title(cur, Scope::Staff).await? {
                Some(r) if r.merged_into.is_some() => {
                    cur = r.merged_into.unwrap_or(cur);
                    if let Some(t) = self.repo.get_title(cur, Scope::Public).await? {
                        return Ok(t);
                    }
                }
                _ => break,
            }
        }
        Err(CinemaError::NotFound("title"))
    }

    pub async fn title_detail(&self, v: Viewer, id: i64) -> CinemaResult<TitleDetail> {
        let rec = self.visible_title(id).await?;
        let exp = self.expires_at();
        let gated = rec.age_restricted && !v.adult;
        let seasons = self.repo.list_seasons(rec.id, Scope::Public).await?;
        let variants = self.repo.list_variants(rec.id).await?;
        let trailers = self.repo.list_trailers(rec.id).await?;
        let markers = self.repo.list_markers(rec.id, None).await?;
        Ok(TitleDetail {
            title: self.title(&rec, exp, false),
            seasons: seasons.iter().map(|s| self.season(s, exp)).collect(),
            variants: variants
                .iter()
                .filter(|v| v.asset_id.is_some())
                .map(|v| Self::variant_wire(v, false))
                .collect(),
            trailers: trailers
                .iter()
                .filter(|t| t.asset_id.is_some())
                .map(|t| Self::trailer_wire(t, gated, false))
                .collect(),
            markers: markers
                .iter()
                .map(|m| Self::marker_wire(m, false))
                .collect(),
        })
    }

    /// Seasons and a keyset page of episodes (all when `limit` is `None`).
    pub async fn episodes(
        &self,
        title: i64,
        season: Option<i64>,
        limit: Option<u32>,
        after: Option<&str>,
    ) -> CinemaResult<EpisodeList> {
        let rec = self.visible_title(title).await?;
        let limit = limit
            .map(|l| l.clamp(1, rules::MAX_EPISODE_LIMIT))
            .unwrap_or(ALL_EPISODES);
        let q = EpisodeQuery {
            season,
            after: after.map(rules::decode_episode_cursor).transpose()?,
            limit: Some(limit + 1),
        };
        let mut eps = self.repo.list_episodes(rec.id, &q, Scope::Public).await?;
        let mut next_cursor = None;
        if eps.len() > limit as usize {
            eps.truncate(limit as usize);
            next_cursor = eps
                .last()
                .map(|e| rules::encode_episode_cursor(e.season_number, e.number, e.id));
        }
        let exp = self.expires_at();
        let seasons = self.repo.list_seasons(rec.id, Scope::Public).await?;
        Ok(EpisodeList {
            seasons: seasons.iter().map(|s| self.season(s, exp)).collect(),
            episodes: eps
                .iter()
                .map(|e| self.episode_wire(e, exp, false))
                .collect(),
            next_cursor,
        })
    }

    async fn visible_episode(&self, id: i64) -> CinemaResult<EpisodeRecord> {
        self.repo
            .get_episode(id)
            .await?
            .filter(|e| e.published && e.title_published && !e.title_merged)
            .ok_or(CinemaError::NotFound("episode"))
    }

    pub async fn episode(&self, id: i64) -> CinemaResult<Episode> {
        let e = self.visible_episode(id).await?;
        Ok(self.episode_wire(&e, self.expires_at(), false))
    }

    pub async fn title_markers(&self, title: i64) -> CinemaResult<MarkerList> {
        let rec = self.visible_title(title).await?;
        let m = self.repo.list_markers(rec.id, None).await?;
        Ok(MarkerList {
            markers: m.iter().map(|m| Self::marker_wire(m, false)).collect(),
        })
    }

    pub async fn episode_markers(&self, id: i64) -> CinemaResult<MarkerList> {
        let e = self.visible_episode(id).await?;
        let m = self.repo.list_markers(e.title_id, Some(e.id)).await?;
        Ok(MarkerList {
            markers: m.iter().map(|m| Self::marker_wire(m, false)).collect(),
        })
    }

    /// What a player needs to open a title or an episode (CIN-DATA-006). A restricted title
    /// answers [`CinemaError::AgeConfirmationRequired`] until the account confirmed its age.
    /// Other cinema backends (playback manifests, downloads) call this before handing out media.
    pub async fn resolve_play(&self, v: Viewer, what: PlayRef) -> CinemaResult<PlayTarget> {
        match what {
            PlayRef::Title { id, variant } => {
                let rec = self.visible_title(id).await?;
                if rec.age_restricted && !v.adult {
                    return Err(CinemaError::AgeConfirmationRequired);
                }
                if let Some(vid) = variant {
                    let var = self
                        .repo
                        .get_variant(vid)
                        .await?
                        .filter(|x| x.title_id == rec.id && x.asset_id.is_some())
                        .ok_or(CinemaError::NotFound("variant"))?;
                    let asset = var.asset_id.ok_or(CinemaError::NotFound("asset"))?;
                    let markers = self.repo.list_markers(rec.id, None).await?;
                    return Ok(PlayTarget {
                        title_id: rec.id,
                        episode_id: None,
                        variant_id: Some(var.id),
                        asset_id: asset,
                        title: rec.title,
                        episode_title: None,
                        season_number: None,
                        episode_number: None,
                        runtime_min: var.runtime_min.or(rec.runtime_min),
                        age_restricted: rec.age_restricted,
                        markers: markers
                            .iter()
                            .map(|m| Self::marker_wire(m, false))
                            .collect(),
                        next_episode_id: None,
                        previous_episode_id: None,
                    });
                }
                if let Some(asset) = rec.asset_id {
                    let markers = self.repo.list_markers(rec.id, None).await?;
                    return Ok(PlayTarget {
                        title_id: rec.id,
                        episode_id: None,
                        variant_id: None,
                        asset_id: asset,
                        title: rec.title,
                        episode_title: None,
                        season_number: None,
                        episode_number: None,
                        runtime_min: rec.runtime_min,
                        age_restricted: rec.age_restricted,
                        markers: markers
                            .iter()
                            .map(|m| Self::marker_wire(m, false))
                            .collect(),
                        next_episode_id: None,
                        previous_episode_id: None,
                    });
                }
                // An episodic title plays through its first playable episode.
                let first = self
                    .repo
                    .first_playable_episode(rec.id)
                    .await?
                    .ok_or(CinemaError::NotFound("asset"))?;
                Box::pin(self.resolve_play(v, PlayRef::Episode(first))).await
            }
            PlayRef::Episode(id) => {
                let e = self.visible_episode(id).await?;
                if e.title_age_restricted && !v.adult {
                    return Err(CinemaError::AgeConfirmationRequired);
                }
                let asset = e.asset_id.ok_or(CinemaError::NotFound("asset"))?;
                let title = self
                    .repo
                    .get_title(e.title_id, Scope::Public)
                    .await?
                    .ok_or(CinemaError::NotFound("title"))?;
                let (prev, next) = self.repo.episode_neighbours(e.id).await?;
                let markers = self.repo.list_markers(e.title_id, Some(e.id)).await?;
                Ok(PlayTarget {
                    title_id: e.title_id,
                    episode_id: Some(e.id),
                    variant_id: None,
                    asset_id: asset,
                    title: title.title,
                    episode_title: (!e.title.is_empty()).then(|| e.title.clone()),
                    season_number: Some(e.season_number),
                    episode_number: Some(e.number),
                    runtime_min: e.runtime_min.or(title.runtime_min),
                    age_restricted: e.title_age_restricted,
                    markers: markers
                        .iter()
                        .map(|m| Self::marker_wire(m, false))
                        .collect(),
                    next_episode_id: next,
                    previous_episode_id: prev,
                })
            }
        }
    }

    /// The catalog play check for a raw asset id (the playback and refresh routes): which
    /// published title, episode, cut or trailer uses it, and whether this viewer may open it.
    /// Unknown, unpublished or merged-away owners answer `NotFound`; a restricted title answers
    /// [`CinemaError::AgeConfirmationRequired`] until the account confirmed its age (D-184).
    pub async fn authorize_asset(&self, v: Viewer, asset_id: i64) -> CinemaResult<AssetAccess> {
        let owner = self
            .repo
            .asset_owner(asset_id)
            .await?
            .ok_or(CinemaError::NotFound("asset"))?;
        let (title_id, restricted) = match owner.episode_id {
            Some(ep) => {
                let e = self.visible_episode(ep).await?;
                (e.title_id, e.title_age_restricted)
            }
            None => {
                let t = self
                    .repo
                    .get_title(owner.title_id, Scope::Public)
                    .await?
                    .ok_or(CinemaError::NotFound("asset"))?;
                (t.id, t.age_restricted)
            }
        };
        if restricted && !v.adult {
            return Err(CinemaError::AgeConfirmationRequired);
        }
        let markers = match owner.usage {
            crate::repo::AssetUse::Feature => self
                .repo
                .list_markers(title_id, owner.episode_id)
                .await?
                .iter()
                .map(|m| Self::marker_wire(m, false))
                .collect(),
            _ => Vec::new(),
        };
        Ok(AssetAccess {
            title_id,
            episode_id: owner.episode_id,
            markers,
        })
    }

    /// Facets for the genre chips and the filters drawer.
    pub async fn facets(&self, v: Viewer, kind: Option<TitleKind>) -> CinemaResult<Facets> {
        self.repo.facets(kind, v.include_restricted()).await
    }

    /// Instant search for the top-bar overlay: titles plus matching people, genres and tags
    /// (CIN-DATA-004). Every hit is re-read through the public scope.
    pub async fn search(
        &self,
        v: Viewer,
        text: &str,
        limit: usize,
        kind: Option<TitleKind>,
    ) -> CinemaResult<InstantSearch> {
        let index = self
            .index
            .as_ref()
            .ok_or_else(|| CinemaError::Search("search is not configured".into()))?;
        let limit = limit.clamp(1, 50);
        let hits = index.search(text, kind, v.include_restricted(), limit)?;
        let ids: Vec<i64> = hits.iter().map(|h| h.id).collect();
        let rows = self
            .repo
            .titles_by_ids(&ids, Scope::Public, v.include_restricted())
            .await?;
        let wanted: Vec<String> = text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect();
        let matches = |s: &str| {
            let low = s.to_lowercase();
            low.split(|c: char| !c.is_alphanumeric())
                .any(|w| wanted.iter().any(|q| w.starts_with(q.as_str())))
        };
        let (mut people, mut genres, mut tags) = (Vec::new(), Vec::new(), Vec::new());
        let push = |dst: &mut Vec<String>, src: &[String]| {
            for s in src.iter().filter(|s| matches(s)) {
                if dst.len() < 5 && !dst.contains(s) {
                    dst.push(s.clone());
                }
            }
        };
        for r in &rows {
            push(&mut people, &r.cast);
            push(&mut people, &r.directors);
            push(&mut people, &r.writers);
            push(&mut people, &r.studios);
            push(&mut genres, &r.genres);
            push(&mut tags, &r.tags);
        }
        Ok(InstantSearch {
            titles: self.titles(&rows, false),
            people,
            genres,
            tags,
        })
    }

    // ---- members: collections and rows ----------------------------------

    pub async fn collections(&self, surface: Option<Surface>) -> CinemaResult<CollectionList> {
        let cs = self.repo.list_collections(surface, Scope::Public).await?;
        Ok(CollectionList {
            items: cs.iter().map(Self::collection_wire).collect(),
        })
    }

    fn row_items(&self, recs: &[TitleRecord]) -> Vec<RowItem> {
        let exp = self.expires_at();
        recs.iter()
            .map(|r| RowItem {
                title: self.title(r, exp, false),
                episode_id: None,
                progress_percent: None,
                reason: None,
            })
            .collect()
    }

    async fn generated_row(&self, v: Viewer, key: &str, limit: u32) -> CinemaResult<Option<Row>> {
        let now = self.clock.now_unix_ms();
        let Some(g) = rows::generated(key, now, v.default_age()) else {
            return Ok(None);
        };
        let q = BrowseQuery { limit, ..g.query };
        let recs = self.repo.browse(&q, Scope::Public).await?.items;
        Ok(Some(Row {
            key: g.key,
            kind: RowKind::Generated,
            display: g.display,
            title: g.title,
            eyebrow: g.eyebrow,
            subtitle: g.subtitle,
            collection_id: None,
            explore: Some(g.explore),
            items: self.row_items(&recs),
        }))
    }

    async fn curated_row(&self, v: Viewer, c: &CollectionRecord, limit: u32) -> CinemaResult<Row> {
        let q = BrowseQuery {
            collection: Some(c.id),
            sort: SortKey::Curated,
            age: v.default_age(),
            limit,
            ..BrowseQuery::default()
        };
        let recs = self.repo.browse(&q, Scope::Public).await?.items;
        Ok(Row {
            key: format!("collection:{}", c.id),
            kind: RowKind::Curated,
            display: c.display,
            title: c.name.clone(),
            eyebrow: c.eyebrow.clone(),
            subtitle: c.blurb.clone(),
            collection_id: Some(c.id),
            explore: Some(format!("collection={}&sort=curated", c.id)),
            items: self.row_items(&recs),
        })
    }

    /// A provider row; `None` when no provider is registered for the key.
    async fn provided_row(
        &self,
        v: Viewer,
        key: &str,
        limit: u32,
        params: BTreeMap<String, String>,
    ) -> CinemaResult<Option<Option<Row>>> {
        let Some(p) = self.providers.iter().find(|p| p.key() == key) else {
            return Ok(None);
        };
        let ctx = RowContext {
            viewer: v,
            limit,
            params,
        };
        let Some(pr) = p.row(&ctx).await? else {
            return Ok(Some(None));
        };
        let ids: Vec<i64> = pr.entries.iter().map(|e| e.title_id).collect();
        let recs = self
            .repo
            .titles_by_ids(&ids, Scope::Public, v.include_restricted())
            .await?;
        let exp = self.expires_at();
        let items: Vec<RowItem> = pr
            .entries
            .iter()
            .filter_map(|e| {
                let r = recs.iter().find(|r| r.id == e.title_id)?;
                Some(RowItem {
                    title: self.title(r, exp, false),
                    episode_id: e.episode_id,
                    progress_percent: e.progress_percent.map(|p| p.clamp(0.0, 100.0)),
                    reason: e.reason.clone(),
                })
            })
            .take(limit as usize)
            .collect();
        let (def_title, def_eyebrow) =
            rows::provided_defaults(key).unwrap_or((key_title(key), None));
        Ok(Some(Some(Row {
            key: key.to_owned(),
            kind: RowKind::Provided,
            display: CollectionDisplay::Row,
            title: pr.title.unwrap_or_else(|| def_title.to_owned()),
            eyebrow: pr.eyebrow.or_else(|| def_eyebrow.map(str::to_owned)),
            subtitle: pr.subtitle,
            collection_id: None,
            explore: pr.explore,
            items,
        })))
    }

    /// One row by key (`GET /cinema/rows/{key}`).
    pub async fn row(
        &self,
        v: Viewer,
        key: &str,
        limit: Option<u32>,
        params: BTreeMap<String, String>,
    ) -> CinemaResult<Row> {
        let limit = limit.unwrap_or(DISCOVER_ROW).clamp(1, rules::MAX_ROW);
        if self.providers.iter().any(|p| p.key() == key) {
            return match self.provided_row(v, key, limit, params).await? {
                Some(Some(row)) => Ok(row),
                _ => {
                    let (title, eyebrow) =
                        rows::provided_defaults(key).unwrap_or((key_title(key), None));
                    Ok(Row {
                        key: key.to_owned(),
                        kind: RowKind::Provided,
                        display: CollectionDisplay::Row,
                        title: title.to_owned(),
                        eyebrow: eyebrow.map(str::to_owned),
                        subtitle: None,
                        collection_id: None,
                        explore: None,
                        items: Vec::new(),
                    })
                }
            };
        }
        if let Some(raw) = key.strip_prefix("collection:") {
            let id: i64 = raw
                .parse()
                .ok()
                .filter(|i| *i > 0)
                .ok_or(CinemaError::NotFound("row"))?;
            let c = self
                .repo
                .get_collection(id, Scope::Public)
                .await?
                .ok_or(CinemaError::NotFound("row"))?;
            return self.curated_row(v, &c, limit).await;
        }
        self.generated_row(v, key, limit)
            .await?
            .ok_or(CinemaError::NotFound("row"))
    }

    /// The Discover page: hero, then the rows in page order (empty rows left out).
    pub async fn discover(&self, v: Viewer) -> CinemaResult<Discover> {
        let collections = self
            .repo
            .list_collections(Some(Surface::Discover), Scope::Public)
            .await?;
        let hero = self.hero(v, &collections).await?;
        let mut out: Vec<Row> = Vec::new();
        out.extend(self.discover_provided(v, KEY_CONTINUE_WATCHING).await);
        out.extend(self.discover_provided(v, KEY_RECENTLY_EXPLORED).await);
        for c in collections
            .iter()
            .filter(|c| c.display != CollectionDisplay::Hero)
        {
            let row = self.curated_row(v, c, DISCOVER_ROW).await?;
            if !row.items.is_empty() {
                out.push(row);
            }
        }
        for (key, limit) in [
            (KEY_ANIMATION_BANNER, BANNER_ROW),
            (KEY_ANIME_ROOM, DISCOVER_ROW),
            (KEY_ONE_MORE_EPISODE, DISCOVER_ROW),
            (KEY_FILMS, DISCOVER_ROW),
        ] {
            if let Some(row) = self.generated_row(v, key, limit).await?
                && !row.items.is_empty()
            {
                out.push(row);
            }
        }
        out.extend(self.discover_provided(v, KEY_TRENDING).await);
        out.extend(self.discover_provided(v, KEY_BECAUSE_YOU_WATCHED).await);
        if let Some(row) = self
            .generated_row(v, KEY_NEW_THIS_WEEK, DISCOVER_ROW)
            .await?
            && !row.items.is_empty()
        {
            out.push(row);
        }
        out.extend(self.discover_provided(v, KEY_FRIENDS_WATCHING).await);
        out.extend(self.discover_provided(v, KEY_REQUESTS_FULFILLED).await);
        if let Some(row) = self
            .generated_row(v, KEY_RECENTLY_ADDED, DISCOVER_ROW)
            .await?
            && !row.items.is_empty()
        {
            out.push(row);
        }
        Ok(Discover { hero, rows: out })
    }

    /// A provider row for Discover. A failing provider must not take the page down.
    async fn discover_provided(&self, v: Viewer, key: &'static str) -> Option<Row> {
        match self
            .provided_row(v, key, DISCOVER_ROW, BTreeMap::new())
            .await
        {
            Ok(Some(Some(row))) if !row.items.is_empty() => Some(row),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(key, error = %e, "cinema row provider failed");
                None
            }
        }
    }

    /// Tonight's feature: the staff-curated hero collection, else the newest titles that have
    /// a backdrop.
    async fn hero(&self, v: Viewer, collections: &[CollectionRecord]) -> CinemaResult<Row> {
        let mut row = Row {
            key: KEY_HERO.to_owned(),
            kind: RowKind::Generated,
            display: CollectionDisplay::Hero,
            title: "Tonight's feature".to_owned(),
            eyebrow: Some("The evening selection".to_owned()),
            subtitle: None,
            collection_id: None,
            explore: None,
            items: Vec::new(),
        };
        if let Some(c) = collections
            .iter()
            .find(|c| c.display == CollectionDisplay::Hero)
        {
            let curated = self.curated_row(v, c, HERO_ROW).await?;
            row.kind = RowKind::Curated;
            row.collection_id = Some(c.id);
            row.items = curated.items;
        }
        if row.items.is_empty() {
            let q = BrowseQuery {
                sort: SortKey::Newest,
                with_backdrop: true,
                age: v.default_age(),
                limit: HERO_ROW,
                ..BrowseQuery::default()
            };
            let recs = self.repo.browse(&q, Scope::Public).await?.items;
            row.kind = RowKind::Generated;
            row.collection_id = None;
            row.items = self.row_items(&recs);
        }
        Ok(row)
    }

    // ---- index maintenance --------------------------------------------

    fn sync_index(&self, r: &TitleRecord) {
        let Some(ix) = &self.index else { return };
        let res = if r.published && r.merged_into.is_none() {
            ix.upsert(&[IndexedTitle::from(r)])
        } else {
            ix.remove(r.id)
        };
        if let Err(e) = res {
            tracing::warn!(title = r.id, error = %e, "cinema search index update failed");
        }
    }

    fn drop_from_index(&self, id: i64) {
        if let Some(ix) = &self.index
            && let Err(e) = ix.remove(id)
        {
            tracing::warn!(title = id, error = %e, "cinema search index removal failed");
        }
    }

    /// Rebuild the index from the database. Run at startup (CIN-DATA-004).
    pub async fn reindex_all(&self) -> CinemaResult<usize> {
        let Some(ix) = &self.index else { return Ok(0) };
        ix.clear()?;
        let (mut after, mut n) = (0, 0);
        loop {
            let batch = self.repo.index_batch(after, 500).await?;
            let Some(last) = batch.last() else { break };
            after = last.id;
            n += batch.len();
            let docs: Vec<IndexedTitle> = batch.iter().map(IndexedTitle::from).collect();
            ix.upsert(&docs)?;
        }
        Ok(n)
    }

    // ---- publishers: titles ----------------------------------------------

    fn staff_title(&self, r: &TitleRecord) -> Title {
        self.title(r, self.expires_at(), true)
    }

    async fn staff_get(&self, id: i64) -> CinemaResult<TitleRecord> {
        self.repo
            .get_title(id, Scope::Staff)
            .await?
            .ok_or(CinemaError::NotFound("title"))
    }

    pub async fn admin_list(&self, a: Actor, q: BrowseQuery) -> CinemaResult<TitlePage> {
        a.publish()?;
        let mut q = rules::browse(q)?;
        q.after = None;
        let rows = self.repo.browse(&q, Scope::Staff).await?;
        let exp = self.expires_at();
        Ok(TitlePage {
            items: rows
                .items
                .iter()
                .map(|r| self.title(r, exp, true))
                .collect(),
            total: rows.total,
            next_cursor: None,
        })
    }

    pub async fn admin_title(&self, a: Actor, id: i64) -> CinemaResult<AdminTitleDetail> {
        a.publish()?;
        let rec = self.staff_get(id).await?;
        let exp = self.expires_at();
        let seasons = self.repo.list_seasons(id, Scope::Staff).await?;
        let episodes = self
            .repo
            .list_episodes(id, &EpisodeQuery::default(), Scope::Staff)
            .await?;
        let variants = self.repo.list_variants(id).await?;
        let trailers = self.repo.list_trailers(id).await?;
        let markers = self.repo.list_markers(id, None).await?;
        Ok(AdminTitleDetail {
            title: self.title(&rec, exp, true),
            seasons: seasons.iter().map(|s| self.season(s, exp)).collect(),
            episodes: episodes
                .iter()
                .map(|e| self.episode_wire(e, exp, true))
                .collect(),
            variants: variants
                .iter()
                .map(|v| Self::variant_wire(v, true))
                .collect(),
            trailers: trailers
                .iter()
                .map(|t| Self::trailer_wire(t, false, true))
                .collect(),
            markers: markers.iter().map(|m| Self::marker_wire(m, true)).collect(),
        })
    }

    /// Skip windows of an episode, drafts included (the editor reads them before publishing).
    pub async fn admin_episode_markers(&self, a: Actor, id: i64) -> CinemaResult<MarkerList> {
        a.publish()?;
        let e = self
            .repo
            .get_episode(id)
            .await?
            .ok_or(CinemaError::NotFound("episode"))?;
        let m = self.repo.list_markers(e.title_id, Some(e.id)).await?;
        Ok(MarkerList {
            markers: m.iter().map(|m| Self::marker_wire(m, true)).collect(),
        })
    }

    pub async fn create_title(&self, a: Actor, input: TitleInput) -> CinemaResult<Title> {
        a.publish()?;
        let input = rules::title_input(&input)?;
        let sort_title = rules::sort_title(&input.title);
        let rec = self
            .repo
            .create_title(&NewTitle {
                id: (self.ids)(),
                input,
                sort_title,
                created_by: a.user_id,
            })
            .await?;
        Ok(self.staff_title(&rec))
    }

    pub async fn update_title(&self, a: Actor, id: i64, input: TitleInput) -> CinemaResult<Title> {
        a.publish()?;
        let input = rules::title_input(&input)?;
        let rec = self
            .repo
            .update_title(id, &input, &rules::sort_title(&input.title))
            .await?
            .ok_or(CinemaError::NotFound("title"))?;
        self.sync_index(&rec);
        Ok(self.staff_title(&rec))
    }

    /// Publish (`PUBLISH_MEDIA`, needs something playable) or unpublish (`PUBLISH_MEDIA` or
    /// `DELETE_CONTENT`). A title without an asset or a published episode stays a draft.
    pub async fn set_title_published(
        &self,
        a: Actor,
        id: i64,
        published: bool,
    ) -> CinemaResult<Title> {
        a.toggle(published)?;
        if published {
            let rec = self.staff_get(id).await?;
            if rec.merged_into.is_some() {
                return Err(CinemaError::Conflict(
                    "a merged title cannot be published".into(),
                ));
            }
            if rec.asset_id.is_none() && rec.episode_count == 0 {
                return Err(CinemaError::Conflict(
                    "a title without a playable asset or a published episode can only stay a draft"
                        .into(),
                ));
            }
        }
        let rec = self
            .repo
            .set_title_published(id, published)
            .await?
            .ok_or(CinemaError::NotFound("title"))?;
        self.sync_index(&rec);
        Ok(self.staff_title(&rec))
    }

    /// Unpublish `title` when nothing playable is left in it; returns the unpublished title.
    async fn settle(&self, title: i64) -> CinemaResult<Option<TitleRecord>> {
        if let Some(t) = self.repo.get_title(title, Scope::Staff).await?
            && t.published
            && t.asset_id.is_none()
            && t.episode_count == 0
        {
            tracing::info!(title, "cinema title unpublished: nothing playable is left");
            if let Some(rec) = self.repo.set_title_published(title, false).await? {
                self.sync_index(&rec);
                return Ok(Some(rec));
            }
        }
        Ok(None)
    }

    /// Link (or unlink, with `None`) the playable asset of a film. Unlinking the last playable
    /// thing of a published title unpublishes it.
    pub async fn link_title_asset(
        &self,
        a: Actor,
        id: i64,
        asset: Option<i64>,
    ) -> CinemaResult<Title> {
        a.publish()?;
        if matches!(asset, Some(x) if x <= 0) {
            return Err(CinemaError::Invalid(
                "asset_id must be a positive id".into(),
            ));
        }
        let rec = self
            .repo
            .set_title_asset(id, asset)
            .await?
            .ok_or(CinemaError::NotFound("title"))?;
        let rec = self.settle(id).await?.unwrap_or(rec);
        Ok(self.staff_title(&rec))
    }

    pub async fn delete_title(&self, a: Actor, id: i64) -> CinemaResult<()> {
        a.delete()?;
        let episodes = self
            .repo
            .delete_title(id)
            .await?
            .ok_or(CinemaError::NotFound("title"))?;
        self.drop_from_index(id);
        for o in &self.observers {
            o.title_deleted(id, &episodes).await;
        }
        Ok(())
    }

    /// Merge `source` into `target`: seasons, episodes, variants, trailers, markers and
    /// collection places move (the target wins on a clash, and fills its gaps from the source);
    /// the source is unpublished and redirects to the target.
    pub async fn merge_titles(&self, a: Actor, source: i64, target: i64) -> CinemaResult<Title> {
        a.publish()?;
        a.delete()?;
        if source == target {
            return Err(CinemaError::Invalid(
                "cannot merge a title into itself".into(),
            ));
        }
        let out = self.repo.merge_titles(source, target).await?;
        self.drop_from_index(source);
        self.sync_index(&out.target);
        for o in &self.observers {
            o.title_merged(source, target, &out.episode_map).await;
        }
        Ok(self.staff_title(&out.target))
    }

    /// Reserve an artwork object of `title` and hand out a presigned `PUT` URL. The client
    /// uploads the image, then saves the returned key in the title, season or episode.
    pub async fn artwork_upload(
        &self,
        a: Actor,
        title: i64,
        req: ArtworkRequest,
    ) -> CinemaResult<ArtworkUpload> {
        a.publish()?;
        let ty = ArtworkType::parse(req.content_type.trim()).ok_or_else(|| {
            CinemaError::Invalid("content_type must be image/webp, image/jpeg or image/png".into())
        })?;
        let storage = self
            .storage
            .as_ref()
            .ok_or_else(|| CinemaError::Storage("artwork uploads are not configured".into()))?;
        self.staff_get(title).await?;
        let key = format!(
            "{}{title}/{}-{}.{}",
            rules::KEY_PREFIX,
            req.slot.as_str(),
            (self.ids)(),
            ty.extension()
        );
        let url = storage
            .presign(PresignMethod::Put, &key, ARTWORK_URL_TTL)
            .await
            .map_err(|e| CinemaError::Storage(e.to_string()))?;
        Ok(ArtworkUpload {
            key,
            upload_url: url,
            content_type: ty.as_str().to_owned(),
            expires_in_secs: ARTWORK_URL_TTL.as_secs(),
        })
    }

    // ---- publishers: seasons and episodes ---------------------------------

    pub async fn create_season(
        &self,
        a: Actor,
        title: i64,
        input: SeasonInput,
    ) -> CinemaResult<Season> {
        a.publish()?;
        let input = rules::season_input(&input)?;
        let rec = self.repo.create_season((self.ids)(), title, &input).await?;
        Ok(self.season(&rec, self.expires_at()))
    }

    pub async fn update_season(
        &self,
        a: Actor,
        id: i64,
        input: SeasonInput,
    ) -> CinemaResult<Season> {
        a.publish()?;
        let input = rules::season_input(&input)?;
        let rec = self
            .repo
            .update_season(id, &input)
            .await?
            .ok_or(CinemaError::NotFound("season"))?;
        Ok(self.season(&rec, self.expires_at()))
    }

    pub async fn delete_season(&self, a: Actor, id: i64) -> CinemaResult<()> {
        a.delete()?;
        let (title, episodes) = self
            .repo
            .delete_season(id)
            .await?
            .ok_or(CinemaError::NotFound("season"))?;
        self.settle(title).await?;
        for o in &self.observers {
            o.episodes_deleted(title, &episodes).await;
        }
        Ok(())
    }

    pub async fn create_episode(
        &self,
        a: Actor,
        season: i64,
        input: EpisodeInput,
    ) -> CinemaResult<Episode> {
        a.publish()?;
        let input = rules::episode_input(&input)?;
        let rec = self
            .repo
            .create_episode((self.ids)(), season, &input)
            .await?;
        Ok(self.episode_wire(&rec, self.expires_at(), true))
    }

    pub async fn update_episode(
        &self,
        a: Actor,
        id: i64,
        input: EpisodeInput,
    ) -> CinemaResult<Episode> {
        a.publish()?;
        let input = rules::episode_input(&input)?;
        let rec = self
            .repo
            .update_episode(id, &input)
            .await?
            .ok_or(CinemaError::NotFound("episode"))?;
        Ok(self.episode_wire(&rec, self.expires_at(), true))
    }

    /// Publish (needs an asset) or unpublish an episode.
    pub async fn set_episode_published(
        &self,
        a: Actor,
        id: i64,
        published: bool,
    ) -> CinemaResult<Episode> {
        a.toggle(published)?;
        if published {
            let e = self
                .repo
                .get_episode(id)
                .await?
                .ok_or(CinemaError::NotFound("episode"))?;
            if e.asset_id.is_none() {
                return Err(CinemaError::Conflict(
                    "an episode without a playable asset can only stay a draft".into(),
                ));
            }
        }
        let rec = self
            .repo
            .set_episode_published(id, published)
            .await?
            .ok_or(CinemaError::NotFound("episode"))?;
        if !published {
            self.settle(rec.title_id).await?;
        }
        Ok(self.episode_wire(&rec, self.expires_at(), true))
    }

    /// Link (or unlink, with `None`) the playable asset of an episode. Unlinking it unpublishes
    /// the episode.
    pub async fn link_episode_asset(
        &self,
        a: Actor,
        id: i64,
        asset: Option<i64>,
    ) -> CinemaResult<Episode> {
        a.publish()?;
        if matches!(asset, Some(x) if x <= 0) {
            return Err(CinemaError::Invalid(
                "asset_id must be a positive id".into(),
            ));
        }
        let mut rec = self
            .repo
            .set_episode_asset(id, asset)
            .await?
            .ok_or(CinemaError::NotFound("episode"))?;
        if asset.is_none() && rec.published {
            rec = self
                .repo
                .set_episode_published(id, false)
                .await?
                .unwrap_or(rec);
            self.settle(rec.title_id).await?;
        }
        Ok(self.episode_wire(&rec, self.expires_at(), true))
    }

    pub async fn delete_episode(&self, a: Actor, id: i64) -> CinemaResult<()> {
        a.delete()?;
        let title = self
            .repo
            .delete_episode(id)
            .await?
            .ok_or(CinemaError::NotFound("episode"))?;
        self.settle(title).await?;
        for o in &self.observers {
            o.episodes_deleted(title, &[id]).await;
        }
        Ok(())
    }

    // ---- publishers: variants, trailers, markers -----------------------------

    pub async fn create_variant(
        &self,
        a: Actor,
        title: i64,
        input: VariantInput,
    ) -> CinemaResult<Variant> {
        a.publish()?;
        let input = rules::variant_input(&input)?;
        let rec = self
            .repo
            .create_variant((self.ids)(), title, &input)
            .await?;
        Ok(Self::variant_wire(&rec, true))
    }

    pub async fn update_variant(
        &self,
        a: Actor,
        id: i64,
        input: VariantInput,
    ) -> CinemaResult<Variant> {
        a.publish()?;
        let input = rules::variant_input(&input)?;
        let rec = self
            .repo
            .update_variant(id, &input)
            .await?
            .ok_or(CinemaError::NotFound("variant"))?;
        Ok(Self::variant_wire(&rec, true))
    }

    pub async fn delete_variant(&self, a: Actor, id: i64) -> CinemaResult<()> {
        a.publish()?;
        if self.repo.delete_variant(id).await? {
            Ok(())
        } else {
            Err(CinemaError::NotFound("variant"))
        }
    }

    pub async fn create_trailer(
        &self,
        a: Actor,
        title: i64,
        input: TrailerInput,
    ) -> CinemaResult<Trailer> {
        a.publish()?;
        let input = rules::trailer_input(&input)?;
        let rec = self
            .repo
            .create_trailer((self.ids)(), title, &input)
            .await?;
        Ok(Self::trailer_wire(&rec, false, true))
    }

    pub async fn update_trailer(
        &self,
        a: Actor,
        id: i64,
        input: TrailerInput,
    ) -> CinemaResult<Trailer> {
        a.publish()?;
        let input = rules::trailer_input(&input)?;
        let rec = self
            .repo
            .update_trailer(id, &input)
            .await?
            .ok_or(CinemaError::NotFound("trailer"))?;
        Ok(Self::trailer_wire(&rec, false, true))
    }

    pub async fn delete_trailer(&self, a: Actor, id: i64) -> CinemaResult<()> {
        a.publish()?;
        if self.repo.delete_trailer(id).await? {
            Ok(())
        } else {
            Err(CinemaError::NotFound("trailer"))
        }
    }

    /// Replace the skip windows of a title (`episode` None) or of one episode.
    pub async fn replace_markers(
        &self,
        a: Actor,
        title: Option<i64>,
        episode: Option<i64>,
        markers: Vec<MarkerInput>,
    ) -> CinemaResult<MarkerList> {
        a.publish()?;
        let markers = rules::markers(&markers)?;
        let title_id = match (title, episode) {
            (_, Some(ep)) => {
                self.repo
                    .get_episode(ep)
                    .await?
                    .ok_or(CinemaError::NotFound("episode"))?
                    .title_id
            }
            (Some(t), None) => t,
            (None, None) => return Err(CinemaError::NotFound("title")),
        };
        let ids: Vec<i64> = markers.iter().map(|_| (self.ids)()).collect();
        let recs = self
            .repo
            .replace_markers(title_id, episode, &markers, &ids, a.user_id)
            .await?
            .ok_or(CinemaError::NotFound(if episode.is_some() {
                "episode"
            } else {
                "title"
            }))?;
        Ok(MarkerList {
            markers: recs.iter().map(|m| Self::marker_wire(m, true)).collect(),
        })
    }

    // ---- publishers: collections ---------------------------------------------

    pub async fn admin_collections(&self, a: Actor) -> CinemaResult<CollectionList> {
        a.collections()?;
        let cs = self.repo.list_collections(None, Scope::Staff).await?;
        Ok(CollectionList {
            items: cs.iter().map(Self::collection_wire).collect(),
        })
    }

    pub async fn admin_collection(&self, a: Actor, id: i64) -> CinemaResult<CollectionDetail> {
        a.collections()?;
        self.collection_detail(id).await
    }

    async fn collection_detail(&self, id: i64) -> CinemaResult<CollectionDetail> {
        let c = self
            .repo
            .get_collection(id, Scope::Staff)
            .await?
            .ok_or(CinemaError::NotFound("collection"))?;
        let q = BrowseQuery {
            collection: Some(id),
            sort: SortKey::Curated,
            limit: rules::MAX_COLLECTION_ITEMS as u32,
            ..BrowseQuery::default()
        };
        let recs = self.repo.browse(&q, Scope::Staff).await?.items;
        Ok(CollectionDetail {
            collection: Self::collection_wire(&c),
            items: self.titles(&recs, true),
        })
    }

    pub async fn create_collection(
        &self,
        a: Actor,
        input: CollectionInput,
    ) -> CinemaResult<Collection> {
        a.collections()?;
        let input = rules::collection_input(&input)?;
        let rec = self
            .repo
            .create_collection((self.ids)(), &input, a.user_id)
            .await?;
        Ok(Self::collection_wire(&rec))
    }

    pub async fn update_collection(
        &self,
        a: Actor,
        id: i64,
        input: CollectionInput,
    ) -> CinemaResult<Collection> {
        a.collections()?;
        let input = rules::collection_input(&input)?;
        let rec = self
            .repo
            .update_collection(id, &input)
            .await?
            .ok_or(CinemaError::NotFound("collection"))?;
        Ok(Self::collection_wire(&rec))
    }

    pub async fn delete_collection(&self, a: Actor, id: i64) -> CinemaResult<()> {
        a.collections()?;
        if self.repo.delete_collection(id).await? {
            Ok(())
        } else {
            Err(CinemaError::NotFound("collection"))
        }
    }

    pub async fn set_collection_items(
        &self,
        a: Actor,
        id: i64,
        titles: Vec<i64>,
    ) -> CinemaResult<CollectionDetail> {
        a.collections()?;
        let titles = rules::collection_items(&titles)?;
        self.repo
            .set_collection_items(id, &titles)
            .await?
            .ok_or(CinemaError::NotFound("collection"))?;
        self.collection_detail(id).await
    }
}

/// Fallback heading of a provider row nobody has default copy for.
fn key_title(_key: &str) -> &'static str {
    "More to watch"
}
