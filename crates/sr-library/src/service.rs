//! The Library service: catalog reads for members, catalog writes for publishers.
//!
//! * Members ([`Viewer`]) see published, non-merged series, age-restricted ones
//!   included (D-184); [`AgeFilter`] and the viewer's own `hide_restricted`
//!   preference can narrow a listing. Opening the pages of a restricted series
//!   needs the account to have confirmed it is 18 or older, otherwise the answer
//!   is [`LibraryError::AgeConfirmationRequired`].
//! * Publishers ([`Actor`]) need the platform permissions of doc 07: `PUBLISH_READING`
//!   to create, edit and publish, `DELETE_CONTENT` to unpublish, delete and merge.
//! * Page and cover URLs are HMAC-signed `/cdn/library/...` links, rounded to the
//!   hour so a manifest fetched twice in a row yields cacheable, identical URLs.

use std::sync::Arc;

use sr_core::{Clock, PlatformPermissions as P};
use sr_storage::cdn::CdnSigner;

use crate::error::{LibraryError, LibraryResult};
use crate::model::*;
use crate::repo::{ChapterListQuery, LibraryRepository, NewSeries, UpdateRecord};
use crate::rules;
use crate::search::{IndexedSeries, SeriesIndex};

/// Snowflake source.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Hours a signed URL stays valid, at least (rounded up to the hour).
const URL_HOURS: u64 = 6;
/// Longest merge redirect chain followed.
const MAX_REDIRECTS: usize = 3;

/// A member reading the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewer {
    pub user_id: i64,
    /// The account confirmed it is 18 or older (`accounts.age_confirmed_at`).
    pub adult: bool,
    /// The viewer's `library.hide_age_restricted` preference: restricted series are
    /// left out of search, follows and updates (and of browse unless asked for).
    pub hide_restricted: bool,
}

impl Viewer {
    pub(crate) fn scope(self) -> Scope {
        Scope::Public
    }

    /// Whether search, follows and updates include restricted series.
    pub(crate) fn include_restricted(self) -> bool {
        !self.hide_restricted
    }
}

/// A publisher changing the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    pub user_id: i64,
    pub permissions: P,
}

impl Actor {
    fn need(self, perm: P, name: &'static str) -> LibraryResult<()> {
        if self.permissions.has(perm) {
            Ok(())
        } else {
            Err(LibraryError::MissingPermission(name))
        }
    }

    pub(crate) fn publish(self) -> LibraryResult<()> {
        self.need(P::PUBLISH_READING, "PUBLISH_READING")
    }

    fn delete(self) -> LibraryResult<()> {
        self.need(P::DELETE_CONTENT, "DELETE_CONTENT")
    }
}

#[derive(Clone)]
pub struct LibraryService {
    pub(crate) repo: Arc<dyn LibraryRepository>,
    index: Option<Arc<SeriesIndex>>,
    signer: CdnSigner,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) ids: IdSource,
    pub(crate) ingest: Option<crate::uploads::IngestWiring>,
    pub(crate) events: Option<Arc<dyn crate::reading::LibraryEvents>>,
    pub(crate) audit: Option<Arc<dyn crate::uploads::LibraryAudit>>,
}

impl LibraryService {
    pub fn new(
        repo: Arc<dyn LibraryRepository>,
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
            ingest: None,
            events: None,
            audit: None,
        }
    }

    /// Deliver `SR_LIBRARY_UPDATE` events (progress sync, follows, new chapters) to members.
    pub fn with_events(mut self, events: Arc<dyn crate::reading::LibraryEvents>) -> Self {
        self.events = Some(events);
        self
    }

    /// Attach the search index. Call [`Self::reindex_all`] after opening it.
    pub fn with_index(mut self, index: Arc<SeriesIndex>) -> Self {
        self.index = Some(index);
        self
    }

    // ---- presentation --------------------------------------------------

    pub(crate) fn expires_at(&self) -> u64 {
        let now = u64::try_from(self.clock.now_unix_ms() / 1000).unwrap_or(0);
        (now / 3600 + URL_HOURS + 1) * 3600
    }

    pub(crate) fn url(&self, key: &str, expires_at: u64) -> String {
        self.signer.signed_url(&format!("/cdn/{key}"), expires_at)
    }

    pub(crate) fn series(&self, r: SeriesRecord, exp: u64) -> Series {
        Series {
            id: r.id,
            kind: r.kind,
            title: r.title,
            alt_titles: r.alt_titles,
            authors: r.authors,
            artists: r.artists,
            status: r.status,
            year: r.year,
            synopsis: r.synopsis,
            genres: r.genres,
            tags: r.tags,
            reading_direction: r.reading_direction,
            age_restricted: r.age_restricted,
            cover_url: r.cover_key.as_deref().map(|k| self.url(k, exp)),
            published: r.published,
            published_at_ms: r.published_at_ms,
            merged_into: r.merged_into,
            chapter_count: r.chapter_count,
            last_chapter_at_ms: r.last_chapter_at_ms,
            follower_count: r.follower_count,
            created_at_ms: r.created_at_ms,
            updated_at_ms: r.updated_at_ms,
        }
    }

    pub(crate) fn chapter(c: &ChapterRecord) -> Chapter {
        Chapter {
            id: c.id,
            series_id: c.series_id,
            edition_id: c.edition_id,
            volume_id: c.volume_id,
            number: c.number,
            title: c.title.clone(),
            page_count: c.page_count,
            format: c.format,
            release_group: c.release_group.clone(),
            published: c.published,
            published_at_ms: c.published_at_ms,
            progress: c.progress,
        }
    }

    fn update(&self, u: UpdateRecord, exp: u64) -> UpdateItem {
        UpdateItem {
            chapter_id: u.chapter_id,
            chapter_number: u.chapter_number,
            chapter_title: u.chapter_title,
            published_at_ms: u.published_at_ms,
            release_group: u.release_group,
            language: u.language,
            series_id: u.series_id,
            series_title: u.series_title,
            kind: u.kind,
            age_restricted: u.age_restricted,
            cover_url: u.cover_key.as_deref().map(|k| self.url(k, exp)),
        }
    }

    // ---- members: catalog ---------------------------------------------

    /// Browse with filters, sort and pagination (LIB-DATA-003).
    pub async fn browse(&self, v: Viewer, q: BrowseQuery) -> LibraryResult<SeriesPage> {
        let mut q = rules::browse(q)?;
        q.published = None;
        q.title_contains = None;
        let rows = self.repo.browse(&q, v.scope()).await?;
        let exp = self.expires_at();
        Ok(SeriesPage {
            items: rows
                .items
                .into_iter()
                .map(|r| self.series(r, exp))
                .collect(),
            total: rows.total,
            limit: q.limit,
            offset: q.offset,
        })
    }

    pub(crate) async fn visible_series(
        &self,
        id: i64,
        scope: Scope,
    ) -> LibraryResult<SeriesRecord> {
        if let Some(r) = self.repo.get_series(id, scope).await? {
            return Ok(r);
        }
        if scope == Scope::Public {
            // A series merged into another redirects to it (the target is re-checked).
            let mut cur = id;
            for _ in 0..MAX_REDIRECTS {
                match self.repo.get_series(cur, Scope::Staff).await? {
                    Some(r) if r.merged_into.is_some() => {
                        cur = r.merged_into.unwrap_or(cur);
                        if let Some(t) = self.repo.get_series(cur, scope).await? {
                            return Ok(t);
                        }
                    }
                    _ => break,
                }
            }
        }
        Err(LibraryError::NotFound("series"))
    }

    pub async fn series_detail(&self, v: Viewer, id: i64) -> LibraryResult<SeriesDetail> {
        let rec = self.visible_series(id, v.scope()).await?;
        let editions = self.repo.list_editions(rec.id).await?;
        let follow = self.repo.get_follow(v.user_id, rec.id).await?;
        Ok(SeriesDetail {
            series: self.series(rec, self.expires_at()),
            editions,
            follow,
        })
    }

    /// Chapter list with volumes and the viewer's read marks. With `q.limit` the list is a
    /// keyset page (`next_cursor`), otherwise every chapter.
    pub async fn chapters(
        &self,
        v: Viewer,
        series: i64,
        q: ChapterQuery,
    ) -> LibraryResult<ChapterList> {
        let rec = self.visible_series(series, v.scope()).await?;
        self.chapter_page(rec.id, q, Some(v.user_id), v.scope())
            .await
    }

    async fn chapter_page(
        &self,
        series: i64,
        q: ChapterQuery,
        viewer: Option<i64>,
        scope: Scope,
    ) -> LibraryResult<ChapterList> {
        let limit = q.limit.map(|l| l.clamp(1, rules::MAX_CHAPTER_LIMIT));
        let after = q
            .after
            .as_deref()
            .map(rules::decode_chapter_cursor)
            .transpose()?;
        let query = ChapterListQuery {
            edition: q.edition,
            after,
            // One extra row tells whether there is a next page.
            limit: limit.map(|l| l + 1),
            descending: q.descending,
        };
        let mut list = self
            .repo
            .list_chapters(series, &query, viewer, scope)
            .await?;
        let mut next_cursor = None;
        if let Some(l) = limit
            && list.len() > l as usize
        {
            list.truncate(l as usize);
            next_cursor = list.last().map(rules::encode_chapter_cursor);
        }
        Ok(ChapterList {
            editions: self.repo.list_editions(series).await?,
            volumes: self.repo.list_volumes(series).await?,
            chapters: list.iter().map(Self::chapter).collect(),
            next_cursor,
        })
    }

    fn can_see(c: &ChapterRecord, scope: Scope) -> bool {
        match scope {
            Scope::Staff => true,
            Scope::Public => c.published && c.series_published && !c.series_merged,
        }
    }

    async fn manifest(
        &self,
        id: i64,
        viewer: Option<i64>,
        scope: Scope,
        adult: bool,
    ) -> LibraryResult<PageManifest> {
        let c = self
            .repo
            .get_chapter(id, viewer)
            .await?
            .filter(|c| Self::can_see(c, scope))
            .ok_or(LibraryError::NotFound("chapter"))?;
        if c.series_age_restricted && !adult {
            // Visible but not yet openable: the client shows the age dialog (D-032).
            return Err(LibraryError::AgeConfirmationRequired);
        }
        let exp = self.expires_at();
        let pages = self.repo.list_pages(id).await?;
        let neighbours = self.repo.neighbours(id).await?;
        Ok(PageManifest {
            chapter: Self::chapter(&c),
            reading_direction: c.series_direction,
            pages: pages
                .into_iter()
                .map(|p| PageEntry {
                    index: p.index,
                    width: p.width,
                    height: p.height,
                    is_double_spread: p.is_double_spread,
                    blurhash: p.blurhash,
                    url: self.url(&p.key, exp),
                    variants: p
                        .variants
                        .iter()
                        .map(|(w, k)| PageVariant {
                            width: *w,
                            url: self.url(k, exp),
                        })
                        .collect(),
                })
                .collect(),
            prev_chapter_id: neighbours.prev,
            next_chapter_id: neighbours.next,
            expires_at: exp,
            source_url: (c.format != ChapterFormat::Images)
                .then(|| c.source_key.as_deref().map(|k| self.url(k, exp)))
                .flatten(),
        })
    }

    /// The reader's page manifest with sized signed URLs.
    pub async fn page_manifest(&self, v: Viewer, chapter: i64) -> LibraryResult<PageManifest> {
        self.manifest(chapter, Some(v.user_id), v.scope(), v.adult)
            .await
    }

    /// Instant search for the top-bar dropdown: series plus matching authors,
    /// genres and tags (LIB-DATA-004). Every hit is re-read through the viewer's scope.
    pub async fn search(
        &self,
        v: Viewer,
        text: &str,
        limit: usize,
    ) -> LibraryResult<InstantSearch> {
        let index = self
            .index
            .as_ref()
            .ok_or_else(|| LibraryError::Search("search is not configured".into()))?;
        let limit = limit.clamp(1, 20);
        let hits = index.search(text, v.include_restricted(), limit)?;
        let ids: Vec<i64> = hits.iter().map(|h| h.id).collect();
        let rows = self.repo.series_by_ids(&ids, v.scope()).await?;
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
        let mut authors = Vec::new();
        let mut genres = Vec::new();
        let mut tags = Vec::new();
        let push = |dst: &mut Vec<String>, src: &[String]| {
            for s in src.iter().filter(|s| matches(s)) {
                if dst.len() < 5 && !dst.contains(s) {
                    dst.push(s.clone());
                }
            }
        };
        for r in &rows {
            push(&mut authors, &r.authors);
            push(&mut authors, &r.artists);
            push(&mut genres, &r.genres);
            push(&mut tags, &r.tags);
        }
        let exp = self.expires_at();
        Ok(InstantSearch {
            series: rows.into_iter().map(|r| self.series(r, exp)).collect(),
            authors,
            genres,
            tags,
        })
    }

    /// Latest published chapters, global or limited to followed series (LIB-DATA-006).
    pub async fn updates(
        &self,
        v: Viewer,
        followed_only: bool,
        before: Option<&str>,
        limit: u32,
    ) -> LibraryResult<UpdatesPage> {
        let before = before.map(rules::decode_cursor).transpose()?;
        let limit = limit.clamp(1, rules::MAX_LIMIT);
        let followed = followed_only.then_some(v.user_id);
        let mut rows = self
            .repo
            .latest_updates(v.include_restricted(), followed, before, limit + 1)
            .await?;
        let more = rows.len() > limit as usize;
        rows.truncate(limit as usize);
        let next_before = more
            .then(|| rows.last())
            .flatten()
            .map(|u| rules::encode_cursor(u.published_at_ms, u.chapter_id));
        let exp = self.expires_at();
        Ok(UpdatesPage {
            items: rows.into_iter().map(|u| self.update(u, exp)).collect(),
            next_before,
        })
    }

    // ---- members: follows ---------------------------------------------

    pub async fn follow(
        &self,
        v: Viewer,
        series: i64,
        shelf: Shelf,
        notify: bool,
    ) -> LibraryResult<Follow> {
        let rec = self.visible_series(series, v.scope()).await?;
        let f = self
            .repo
            .set_follow(v.user_id, rec.id, shelf, notify)
            .await?;
        self.emit(
            v.user_id,
            serde_json::json!({
                "type": "follow",
                "series_id": rec.id.to_string(),
                "shelf": f.shelf.as_str(),
                "notify": f.notify,
            }),
        )
        .await;
        Ok(f)
    }

    /// Returns whether a follow existed.
    pub async fn unfollow(&self, v: Viewer, series: i64) -> LibraryResult<bool> {
        let removed = self.repo.remove_follow(v.user_id, series).await?;
        if removed {
            self.emit(
                v.user_id,
                serde_json::json!({"type": "unfollow", "series_id": series.to_string()}),
            )
            .await;
        }
        Ok(removed)
    }

    pub async fn my_follows(
        &self,
        v: Viewer,
        shelf: Option<Shelf>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> LibraryResult<FollowPage> {
        let (limit, offset) = rules::paging(limit, offset);
        let rows = self
            .repo
            .list_follows(v.user_id, shelf, v.include_restricted(), limit, offset)
            .await?;
        let exp = self.expires_at();
        Ok(FollowPage {
            items: rows
                .items
                .into_iter()
                .map(|(s, f)| FollowedSeries {
                    series: self.series(s, exp),
                    follow: f,
                })
                .collect(),
            total: rows.total,
            limit,
            offset,
        })
    }

    // ---- index maintenance --------------------------------------------

    fn sync_index(&self, r: &SeriesRecord) {
        let Some(ix) = &self.index else { return };
        let res = if r.published && r.merged_into.is_none() {
            ix.upsert(&[IndexedSeries::from(r)])
        } else {
            ix.remove(r.id)
        };
        if let Err(e) = res {
            tracing::warn!(series = r.id, error = %e, "library search index update failed");
        }
    }

    fn drop_from_index(&self, id: i64) {
        if let Some(ix) = &self.index
            && let Err(e) = ix.remove(id)
        {
            tracing::warn!(series = id, error = %e, "library search index removal failed");
        }
    }

    /// Rebuild the index from the database. Run at startup.
    pub async fn reindex_all(&self) -> LibraryResult<usize> {
        let Some(ix) = &self.index else { return Ok(0) };
        ix.clear()?;
        let (mut after, mut n) = (0, 0);
        loop {
            let batch = self.repo.index_batch(after, 500).await?;
            let Some(last) = batch.last() else { break };
            after = last.id;
            n += batch.len();
            let docs: Vec<IndexedSeries> = batch.iter().map(IndexedSeries::from).collect();
            ix.upsert(&docs)?;
        }
        Ok(n)
    }

    // ---- publishers: series -------------------------------------------

    pub async fn admin_list(&self, a: Actor, q: BrowseQuery) -> LibraryResult<SeriesPage> {
        a.publish()?;
        let q = rules::browse(q)?;
        let rows = self.repo.browse(&q, Scope::Staff).await?;
        let exp = self.expires_at();
        Ok(SeriesPage {
            items: rows
                .items
                .into_iter()
                .map(|r| self.series(r, exp))
                .collect(),
            total: rows.total,
            limit: q.limit,
            offset: q.offset,
        })
    }

    pub async fn admin_series(&self, a: Actor, id: i64) -> LibraryResult<SeriesDetail> {
        a.publish()?;
        let rec = self
            .repo
            .get_series(id, Scope::Staff)
            .await?
            .ok_or(LibraryError::NotFound("series"))?;
        Ok(SeriesDetail {
            editions: self.repo.list_editions(id).await?,
            follow: None,
            series: self.series(rec, self.expires_at()),
        })
    }

    pub async fn admin_chapters(
        &self,
        a: Actor,
        series: i64,
        q: ChapterQuery,
    ) -> LibraryResult<ChapterList> {
        a.publish()?;
        self.repo
            .get_series(series, Scope::Staff)
            .await?
            .ok_or(LibraryError::NotFound("series"))?;
        self.chapter_page(series, q, None, Scope::Staff).await
    }

    pub async fn admin_page_manifest(&self, a: Actor, chapter: i64) -> LibraryResult<PageManifest> {
        a.publish()?;
        self.manifest(chapter, None, Scope::Staff, true).await
    }

    pub async fn create_series(&self, a: Actor, input: SeriesInput) -> LibraryResult<Series> {
        a.publish()?;
        let (input, direction) = rules::series_input(&input)?;
        let rec = self
            .repo
            .create_series(&NewSeries {
                id: (self.ids)(),
                input,
                direction,
                created_by: a.user_id,
            })
            .await?;
        Ok(self.series(rec, self.expires_at()))
    }

    pub async fn update_series(
        &self,
        a: Actor,
        id: i64,
        input: SeriesInput,
    ) -> LibraryResult<Series> {
        a.publish()?;
        let (input, direction) = rules::series_input(&input)?;
        let rec = self
            .repo
            .update_series(id, &input, direction)
            .await?
            .ok_or(LibraryError::NotFound("series"))?;
        self.sync_index(&rec);
        Ok(self.series(rec, self.expires_at()))
    }

    /// Publish (`PUBLISH_READING`) or unpublish (`PUBLISH_READING` or `DELETE_CONTENT`).
    pub async fn set_series_published(
        &self,
        a: Actor,
        id: i64,
        published: bool,
    ) -> LibraryResult<Series> {
        if published {
            a.publish()?;
        } else if a.publish().is_err() {
            a.delete()?;
        }
        let rec = self
            .repo
            .set_series_published(id, published)
            .await?
            .ok_or(LibraryError::NotFound("series"))?;
        self.sync_index(&rec);
        Ok(self.series(rec, self.expires_at()))
    }

    pub async fn delete_series(&self, a: Actor, id: i64) -> LibraryResult<()> {
        a.delete()?;
        if !self.repo.delete_series(id).await? {
            return Err(LibraryError::NotFound("series"));
        }
        self.drop_from_index(id);
        Ok(())
    }

    /// Merge `source` into `target`: editions, chapters, follows and progress move;
    /// the source is unpublished and redirects to the target.
    pub async fn merge_series(&self, a: Actor, source: i64, target: i64) -> LibraryResult<Series> {
        a.publish()?;
        a.delete()?;
        if source == target {
            return Err(LibraryError::Invalid(
                "cannot merge a series into itself".into(),
            ));
        }
        let rec = self.repo.merge_series(source, target).await?;
        self.drop_from_index(source);
        self.sync_index(&rec);
        Ok(self.series(rec, self.expires_at()))
    }

    // ---- publishers: editions, volumes, chapters ------------------------

    pub async fn create_edition(
        &self,
        a: Actor,
        series: i64,
        input: EditionInput,
    ) -> LibraryResult<Edition> {
        a.publish()?;
        let input = rules::edition_input(&input)?;
        self.repo.create_edition((self.ids)(), series, &input).await
    }

    pub async fn update_edition(
        &self,
        a: Actor,
        id: i64,
        input: EditionInput,
    ) -> LibraryResult<Edition> {
        a.publish()?;
        let input = rules::edition_input(&input)?;
        self.repo
            .update_edition(id, &input)
            .await?
            .ok_or(LibraryError::NotFound("edition"))
    }

    pub async fn delete_edition(&self, a: Actor, id: i64) -> LibraryResult<()> {
        a.delete()?;
        if self.repo.delete_edition(id).await? {
            Ok(())
        } else {
            Err(LibraryError::NotFound("edition"))
        }
    }

    pub async fn create_volume(
        &self,
        a: Actor,
        edition: i64,
        input: VolumeInput,
    ) -> LibraryResult<Volume> {
        a.publish()?;
        let input = rules::volume_input(&input)?;
        self.repo.create_volume((self.ids)(), edition, &input).await
    }

    pub async fn delete_volume(&self, a: Actor, id: i64) -> LibraryResult<()> {
        a.delete()?;
        if self.repo.delete_volume(id).await? {
            Ok(())
        } else {
            Err(LibraryError::NotFound("volume"))
        }
    }

    pub async fn create_chapter(
        &self,
        a: Actor,
        edition: i64,
        input: ChapterInput,
    ) -> LibraryResult<Chapter> {
        a.publish()?;
        let input = rules::chapter_input(&input)?;
        let rec = self
            .repo
            .create_chapter((self.ids)(), edition, &input, a.user_id)
            .await?;
        Ok(Self::chapter(&rec))
    }

    pub async fn update_chapter(
        &self,
        a: Actor,
        id: i64,
        input: ChapterInput,
    ) -> LibraryResult<Chapter> {
        a.publish()?;
        let input = rules::chapter_input(&input)?;
        let rec = self
            .repo
            .update_chapter(id, &input)
            .await?
            .ok_or(LibraryError::NotFound("chapter"))?;
        Ok(Self::chapter(&rec))
    }

    /// Publish (`PUBLISH_READING`, needs pages) or unpublish (`PUBLISH_READING` or `DELETE_CONTENT`).
    pub async fn set_chapter_published(
        &self,
        a: Actor,
        id: i64,
        published: bool,
    ) -> LibraryResult<Chapter> {
        let mut was_published = true;
        if published {
            a.publish()?;
            let c = self
                .repo
                .get_chapter(id, None)
                .await?
                .ok_or(LibraryError::NotFound("chapter"))?;
            let empty = match c.format {
                ChapterFormat::Images => c.page_count == 0,
                _ => c.source_key.is_none(),
            };
            if empty {
                return Err(LibraryError::Conflict(
                    "a chapter without pages cannot be published".into(),
                ));
            }
            was_published = c.published;
        } else if a.publish().is_err() {
            a.delete()?;
        }
        let rec = self
            .repo
            .set_chapter_published(id, published)
            .await?
            .ok_or(LibraryError::NotFound("chapter"))?;
        if published
            && !was_published
            && rec.series_published
            && !rec.series_merged
            && self.events.is_some()
        {
            // Fan out to followers off the request path; a failure only loses a live nudge.
            let svc = self.clone();
            let rec2 = rec.clone();
            tokio::spawn(async move {
                if let Err(e) = svc.notify_followers(&rec2).await {
                    tracing::warn!(chapter = rec2.id, error = %e, "notifying followers failed");
                }
            });
        }
        Ok(Self::chapter(&rec))
    }

    pub async fn delete_chapter(&self, a: Actor, id: i64) -> LibraryResult<()> {
        a.delete()?;
        if self.repo.delete_chapter(id).await? {
            Ok(())
        } else {
            Err(LibraryError::NotFound("chapter"))
        }
    }

    /// Replace the page list of a chapter: this is page reorder, replace and delete.
    pub async fn replace_pages(
        &self,
        a: Actor,
        chapter: i64,
        pages: Vec<PageInput>,
    ) -> LibraryResult<Chapter> {
        a.publish()?;
        let c = self
            .repo
            .get_chapter(chapter, None)
            .await?
            .ok_or(LibraryError::NotFound("chapter"))?;
        if c.format != ChapterFormat::Images {
            return Err(LibraryError::Conflict(
                "only image chapters have pages".into(),
            ));
        }
        let rows = rules::pages(c.series_id, chapter, &pages)?;
        if c.published && rows.is_empty() {
            return Err(LibraryError::Conflict(
                "unpublish the chapter before removing all its pages".into(),
            ));
        }
        let rec = self
            .repo
            .replace_pages(chapter, &rows)
            .await?
            .ok_or(LibraryError::NotFound("chapter"))?;
        Ok(Self::chapter(&rec))
    }
}
