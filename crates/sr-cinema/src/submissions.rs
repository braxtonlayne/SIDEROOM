//! The Cinema kinds of the review pipeline (CIN-ING-010): `cinema.title` (a new film, or a new
//! series with its seasons and episodes) and `cinema.episodes` (a batch of episodes for a series
//! that is already published).
//!
//! The submitter's payload describes the catalog entry; creating the submission writes it as an
//! unpublished draft through [`CinemaService`] with a *sponsored* actor (publisher bits, valid
//! only inside this module's checked paths), so what reviewers look at is exactly what publishing
//! will make visible. Videos are referenced by the id of an asset the submitter uploaded
//! ([`AssetLookup`]); a submission is not ready until every item has a playable asset. Publishing
//! (after a reviewer approved) publishes the approved episodes and then the title, with the same
//! sponsored actor, and is safe to repeat.
//!
//! Payload of `cinema.title`: the fields of `TitleInput` (artwork keys are ignored: members
//! cannot point at storage objects), plus `asset_id` for a film and `seasons` for a series:
//! `[{ "number": 1, "name": "..", "episodes": [{ "number": 1, "title": "..", "asset_id": ".." }] }]`.
//! Payload of `cinema.episodes`: `{ "title_id": "..", "season": 1, "episodes": [..] }`.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sr_core::PlatformPermissions as P;
use sr_review::model::{DuplicateMatch, MediaRef, Submission, SubmissionItem};
use sr_review::repo::NewItem;
use sr_review::{KindDraft, ReviewError, ReviewResult, SubmissionKind};

use crate::error::CinemaError;
use crate::model::*;
use crate::service::{Actor, CinemaService};
use std::sync::Arc;

pub const KIND_TITLE: &str = "cinema.title";
pub const KIND_EPISODES: &str = "cinema.episodes";

/// Most episodes one submission may create.
pub const MAX_EPISODES: usize = 500;

impl From<CinemaError> for ReviewError {
    fn from(e: CinemaError) -> Self {
        match e {
            CinemaError::NotFound(w) => Self::NotFound(w),
            CinemaError::MissingPermission(p) => Self::MissingPermission(p),
            CinemaError::Invalid(m) => Self::Invalid(m),
            CinemaError::Conflict(m) => Self::Conflict(m),
            CinemaError::AgeConfirmationRequired => {
                Self::Forbidden("age confirmation required".into())
            }
            CinemaError::Storage(m) | CinemaError::Search(m) => Self::Storage(m),
        }
    }
}

/// What the kinds need to know about an uploaded video.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetFacts {
    /// The ladder is published: the asset can be played.
    pub playable: bool,
    pub uploaded_by: Option<i64>,
}

/// Port to the media pipeline (`sr-media` assets), implemented by the composition root.
#[async_trait]
pub trait AssetLookup: Send + Sync {
    async fn asset(&self, id: i64) -> Result<Option<AssetFacts>, String>;

    /// Give up the unpublished assets of a rejected or abandoned submission: queue their
    /// transcode output for collection and mark them removed, so the retention sweep deletes
    /// the original and the review proxy after its grace period. Published assets are never
    /// touched. Idempotent.
    async fn discard(&self, _ids: &[i64]) -> Result<(), String> {
        Ok(())
    }
}

/// Nothing is playable (tests, installations without the media pipeline).
pub struct NoAssets;

#[async_trait]
impl AssetLookup for NoAssets {
    async fn asset(&self, _id: i64) -> Result<Option<AssetFacts>, String> {
        Ok(None)
    }
}

fn sponsor(user_id: i64) -> Actor {
    Actor {
        user_id,
        permissions: P::PUBLISH_MEDIA | P::DELETE_CONTENT,
    }
}

fn parse_err(what: &str, e: serde_json::Error) -> ReviewError {
    ReviewError::Invalid(format!("invalid {what}: {e}"))
}

#[derive(Debug, Deserialize)]
struct EpisodeDraft {
    #[serde(flatten)]
    input: EpisodeInput,
    #[serde(default, deserialize_with = "crate::model::opt_id::deserialize")]
    asset_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SeasonDraft {
    number: i32,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    synopsis: Option<String>,
    #[serde(default)]
    episodes: Vec<EpisodeDraft>,
}

#[derive(Debug, Deserialize)]
struct TitleDraft {
    #[serde(flatten)]
    input: TitleInput,
    #[serde(default, deserialize_with = "crate::model::opt_id::deserialize")]
    asset_id: Option<i64>,
    #[serde(default)]
    seasons: Vec<SeasonDraft>,
}

#[derive(Debug, Deserialize)]
struct EpisodesDraft {
    #[serde(deserialize_with = "crate::model::id::deserialize")]
    title_id: i64,
    #[serde(default = "first_season")]
    season: i32,
    episodes: Vec<EpisodeDraft>,
}

fn first_season() -> i32 {
    1
}

fn label(e: &Episode) -> String {
    let base = format!("S{:02}E{:02}", e.season_number, e.number);
    if e.title.trim().is_empty() {
        base
    } else {
        format!("{base} {}", e.title)
    }
}

fn episode_facts(e: &Episode) -> Value {
    json!({
        "id": e.id.to_string(),
        "season": e.season_number,
        "number": e.number,
        "title": e.title,
        "synopsis": e.synopsis,
        "air_date": e.air_date,
        "runtime_min": e.runtime_min,
        "has_asset": e.asset_id.is_some(),
        "published": e.published,
    })
}

fn title_facts(t: &Title) -> Value {
    json!({
        "kind": t.kind,
        "title": t.title,
        "original_title": t.original_title,
        "year": t.year,
        "runtime_min": t.runtime_min,
        "content_rating": t.content_rating,
        "synopsis": t.synopsis,
        "genres": t.genres,
        "tags": t.tags,
        "languages": t.languages,
        "subtitle_languages": t.subtitle_languages,
        "cast": t.cast,
        "directors": t.directors,
        "age_restricted": t.age_restricted,
        "has_asset": t.asset_id.is_some(),
        "published": t.published,
    })
}

/// `cinema.title` and `cinema.episodes`.
#[derive(Clone)]
pub struct CinemaKind {
    svc: CinemaService,
    assets: Arc<dyn AssetLookup>,
    new_title: bool,
}

impl CinemaKind {
    /// `cinema.title`: a new film or series.
    pub fn title(svc: CinemaService, assets: Arc<dyn AssetLookup>) -> Self {
        Self {
            svc,
            assets,
            new_title: true,
        }
    }

    /// `cinema.episodes`: new episodes for a published series.
    pub fn episodes(svc: CinemaService, assets: Arc<dyn AssetLookup>) -> Self {
        Self {
            svc,
            assets,
            new_title: false,
        }
    }

    /// The asset must exist and be the submitter's.
    async fn check_asset(&self, submitter: i64, asset: i64) -> ReviewResult<()> {
        match self
            .assets
            .asset(asset)
            .await
            .map_err(ReviewError::Storage)?
        {
            Some(f) if f.uploaded_by.is_none() || f.uploaded_by == Some(submitter) => Ok(()),
            _ => Err(ReviewError::Invalid(format!("unknown video {asset}"))),
        }
    }

    async fn is_playable(&self, asset: Option<i64>) -> ReviewResult<bool> {
        let Some(a) = asset else { return Ok(false) };
        Ok(self
            .assets
            .asset(a)
            .await
            .map_err(ReviewError::Storage)?
            .is_some_and(|f| f.playable))
    }

    fn title_id(s: &Submission) -> ReviewResult<i64> {
        s.ref_id
            .ok_or_else(|| ReviewError::Storage("the submission has no title".into()))
    }

    async fn detail(&self, s: &Submission) -> ReviewResult<AdminTitleDetail> {
        Ok(self.svc.admin_title(sponsor(0), Self::title_id(s)?).await?)
    }

    /// Create the draft episodes of one season, returning them in order. Members cannot set
    /// artwork; the asset link is applied when the asset is theirs.
    async fn create_episodes(
        &self,
        submitter: i64,
        season: i64,
        drafts: Vec<EpisodeDraft>,
    ) -> ReviewResult<Vec<Episode>> {
        let a = sponsor(submitter);
        let mut out = Vec::new();
        for d in drafts {
            let mut input = d.input;
            input.still_key = None;
            if let Some(asset) = d.asset_id {
                self.check_asset(submitter, asset).await?;
            }
            let ep = self.svc.create_episode(a, season, input).await?;
            let ep = match d.asset_id {
                Some(asset) => self.svc.link_episode_asset(a, ep.id, Some(asset)).await?,
                None => ep,
            };
            out.push(ep);
        }
        Ok(out)
    }
}

#[async_trait]
impl SubmissionKind for CinemaKind {
    fn id(&self) -> &'static str {
        if self.new_title {
            KIND_TITLE
        } else {
            KIND_EPISODES
        }
    }

    fn label(&self) -> &'static str {
        if self.new_title {
            "Cinema title"
        } else {
            "Cinema episodes"
        }
    }

    fn review_permission(&self) -> P {
        P::REVIEW_MEDIA
    }

    fn artwork_slots(&self) -> &'static [&'static str] {
        if self.new_title {
            &["poster", "backdrop"]
        } else {
            &[]
        }
    }

    async fn set_artwork(
        &self,
        s: &Submission,
        user: i64,
        slot: &str,
        image: &[u8],
    ) -> ReviewResult<Option<String>> {
        let slot = match slot {
            "poster" if self.new_title => ArtworkSlot::Poster,
            "backdrop" if self.new_title => ArtworkSlot::Backdrop,
            _ => return Err(ReviewError::Invalid(format!("this kind has no {slot}"))),
        };
        let id = s
            .ref_id
            .ok_or_else(|| ReviewError::Storage("the submission has no title".into()))?;
        let t = self
            .svc
            .set_title_artwork(sponsor(user), id, slot, image)
            .await?;
        Ok(match slot {
            ArtworkSlot::Poster => t.poster_url,
            _ => t.backdrop_url,
        })
    }

    fn publish_permission(&self) -> P {
        P::PUBLISH_MEDIA
    }

    async fn create(&self, submitter: i64, payload: &Value) -> ReviewResult<KindDraft> {
        let a = sponsor(submitter);
        if self.new_title {
            let mut draft: TitleDraft =
                serde_json::from_value(payload.clone()).map_err(|e| parse_err("title", e))?;
            let n: usize = draft.seasons.iter().map(|s| s.episodes.len()).sum();
            if n > MAX_EPISODES {
                return Err(ReviewError::Invalid(format!(
                    "at most {MAX_EPISODES} episodes per submission"
                )));
            }
            // A member cannot point at storage objects; artwork is set by staff.
            draft.input.poster_key = None;
            draft.input.backdrop_key = None;
            draft.input.logo_key = None;
            if draft.asset_id.is_some() && !draft.seasons.is_empty() {
                return Err(ReviewError::Invalid(
                    "a title has either a video of its own or seasons, not both".into(),
                ));
            }
            if let Some(asset) = draft.asset_id {
                self.check_asset(submitter, asset).await?;
            }
            for e in draft.seasons.iter().flat_map(|s| &s.episodes) {
                if let Some(asset) = e.asset_id {
                    self.check_asset(submitter, asset).await?;
                }
            }
            let title = self.svc.create_title(a, draft.input).await?;
            let built: ReviewResult<()> = async {
                if let Some(asset) = draft.asset_id {
                    self.svc.link_title_asset(a, title.id, Some(asset)).await?;
                }
                for s in draft.seasons {
                    let season = self
                        .svc
                        .create_season(
                            a,
                            title.id,
                            SeasonInput {
                                number: s.number,
                                name: s.name,
                                synopsis: s.synopsis,
                                poster_key: None,
                            },
                        )
                        .await?;
                    self.create_episodes(submitter, season.id, s.episodes)
                        .await?;
                }
                Ok(())
            }
            .await;
            if let Err(e) = built {
                // Do not leave a half-built draft behind.
                let _ = self.svc.delete_title(a, title.id).await;
                return Err(e);
            }
            Ok(KindDraft {
                title: title.title.clone(),
                ref_id: Some(title.id),
                payload: json!({ "title_id": title.id.to_string() }),
            })
        } else {
            let draft: EpisodesDraft =
                serde_json::from_value(payload.clone()).map_err(|e| parse_err("episodes", e))?;
            if draft.episodes.is_empty() || draft.episodes.len() > MAX_EPISODES {
                return Err(ReviewError::Invalid(format!(
                    "send between 1 and {MAX_EPISODES} episodes"
                )));
            }
            let detail = self.svc.admin_title(a, draft.title_id).await?;
            if !detail.title.published
                || !matches!(detail.title.kind, TitleKind::Series | TitleKind::Anime)
            {
                return Err(ReviewError::Invalid(
                    "episodes can only be added to a published series".into(),
                ));
            }
            for e in &draft.episodes {
                if let Some(asset) = e.asset_id {
                    self.check_asset(submitter, asset).await?;
                }
                if detail
                    .episodes
                    .iter()
                    .any(|x| x.season_number == draft.season && x.number == e.input.number)
                {
                    return Err(ReviewError::Invalid(format!(
                        "episode {} of season {} already exists",
                        e.input.number, draft.season
                    )));
                }
            }
            let season = match detail.seasons.iter().find(|s| s.number == draft.season) {
                Some(s) => s.id,
                None => {
                    self.svc
                        .create_season(
                            a,
                            draft.title_id,
                            SeasonInput {
                                number: draft.season,
                                name: None,
                                synopsis: None,
                                poster_key: None,
                            },
                        )
                        .await?
                        .id
                }
            };
            let created = self
                .create_episodes(submitter, season, draft.episodes)
                .await;
            let created = match created {
                Ok(c) => c,
                Err(e) => return Err(e),
            };
            Ok(KindDraft {
                title: format!("New episodes for {}", detail.title.title),
                ref_id: Some(draft.title_id),
                payload: json!({
                    "title_id": draft.title_id.to_string(),
                    "season": draft.season,
                    "episode_ids": created.iter().map(|e| e.id.to_string()).collect::<Vec<_>>(),
                }),
            })
        }
    }

    async fn items(
        &self,
        s: &Submission,
        _current: &[SubmissionItem],
    ) -> ReviewResult<Vec<NewItem>> {
        let detail = self.detail(s).await?;
        let mut rows: Vec<(i64, String)> = Vec::new();
        if self.new_title {
            if detail.episodes.is_empty() {
                rows.push((detail.title.id, detail.title.title.clone()));
            } else {
                rows.extend(detail.episodes.iter().map(|e| (e.id, label(e))));
            }
        } else {
            let ids: Vec<i64> = s
                .payload
                .get("episode_ids")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().and_then(|x| x.parse().ok()))
                        .collect()
                })
                .unwrap_or_default();
            rows.extend(
                detail
                    .episodes
                    .iter()
                    .filter(|e| ids.contains(&e.id))
                    .map(|e| (e.id, label(e))),
            );
        }
        Ok(rows
            .into_iter()
            .enumerate()
            .map(|(i, (ref_id, label))| NewItem {
                ref_id,
                label,
                position: i32::try_from(i).unwrap_or(i32::MAX),
            })
            .collect())
    }

    async fn validate(&self, s: &Submission, items: &[NewItem]) -> ReviewResult<Vec<String>> {
        let detail = self.detail(s).await?;
        let mut problems = Vec::new();
        if self.new_title && detail.title.published {
            return Err(ReviewError::Conflict(
                "the title is already published".into(),
            ));
        }
        if items.is_empty() {
            problems.push("add at least one video".to_owned());
        }
        for it in items {
            let asset = if it.ref_id == detail.title.id {
                detail.title.asset_id
            } else {
                detail
                    .episodes
                    .iter()
                    .find(|e| e.id == it.ref_id)
                    .and_then(|e| e.asset_id)
            };
            if detail
                .episodes
                .iter()
                .any(|e| e.id == it.ref_id && e.published)
            {
                problems.push(format!("{} is already published", it.label));
            }
            if asset.is_none() {
                problems.push(format!("{} has no video yet; upload it first", it.label));
            } else if !self.is_playable(asset).await? {
                problems.push(format!("{} is still being processed", it.label));
            }
        }
        Ok(problems)
    }

    async fn preview(&self, s: &Submission) -> ReviewResult<Value> {
        let detail = self.detail(s).await?;
        let episodes: Vec<&Episode> = if self.new_title {
            detail.episodes.iter().collect()
        } else {
            // The episodes of this submission are the unpublished ones.
            detail.episodes.iter().filter(|e| !e.published).collect()
        };
        Ok(json!({
            "title_id": detail.title.id.to_string(),
            "new_title": self.new_title,
            "title": title_facts(&detail.title),
            "seasons": detail.seasons.iter().map(|x| json!({ "number": x.number, "name": x.name })).collect::<Vec<_>>(),
            "existing_episodes": detail.episodes.iter().filter(|e| e.published).count(),
            "episodes": episodes.into_iter().map(episode_facts).collect::<Vec<_>>(),
        }))
    }

    /// Only items that still exist in the catalog earn a credit: a reference whose row is gone
    /// (replaced or deleted since) is skipped (G-61).
    async fn credit_refs(
        &self,
        s: &Submission,
        published: &[SubmissionItem],
    ) -> ReviewResult<Vec<i64>> {
        let detail = match self.detail(s).await {
            Ok(d) => d,
            Err(ReviewError::NotFound(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        Ok(published
            .iter()
            .map(|i| i.ref_id)
            .filter(|r| *r == detail.title.id || detail.episodes.iter().any(|e| e.id == *r))
            .collect())
    }

    /// Published titles with the same or a similar name, same year first (title, alternative
    /// titles and the original title). Only a new title can duplicate one.
    async fn duplicates(&self, s: &Submission) -> ReviewResult<Vec<DuplicateMatch>> {
        use sr_review::similar::title_score;
        if !self.new_title {
            return Ok(Vec::new());
        }
        let detail = self.detail(s).await?;
        let mine = &detail.title;
        let mut mine_names = vec![mine.title.clone()];
        mine_names.extend(mine.original_title.clone());
        mine_names.extend(mine.alt_titles.iter().cloned());
        let viewer = crate::service::Viewer {
            user_id: 0,
            adult: true,
            hide_restricted: false,
        };
        let mut out: Vec<DuplicateMatch> = Vec::new();
        for name in &mine_names {
            let found = self
                .svc
                .search(viewer, name, 20, Some(mine.kind))
                .await
                .map_err(ReviewError::from)?;
            for t in found.titles.iter().filter(|t| t.id != mine.id) {
                let theirs = std::iter::once(&t.title)
                    .chain(t.original_title.iter())
                    .chain(t.alt_titles.iter());
                let mut best = 0_u8;
                for a in &mine_names {
                    for b in theirs.clone() {
                        best = best.max(title_score(a, b));
                    }
                }
                if best == 0 {
                    continue;
                }
                let mut reasons = vec!["title".to_owned()];
                let mut score = best;
                if mine.year.is_some() && mine.year == t.year {
                    reasons.push("year".into());
                    score = score.saturating_add(5).min(100);
                }
                if out.iter().any(|m| m.ref_id == t.id) {
                    continue;
                }
                out.push(DuplicateMatch {
                    section: "cinema".into(),
                    ref_id: t.id,
                    title: t.title.clone(),
                    year: t.year.map(i32::from),
                    score,
                    reasons,
                    href: Some(format!("/cinema/title/{}", t.id)),
                });
            }
        }
        Ok(out)
    }

    /// The videos of the episodes (or the film) under review.
    async fn media(&self, s: &Submission) -> ReviewResult<Vec<MediaRef>> {
        let detail = self.detail(s).await?;
        let episodes: Vec<&Episode> = if self.new_title {
            detail.episodes.iter().collect()
        } else {
            detail.episodes.iter().filter(|e| !e.published).collect()
        };
        let mut out = Vec::new();
        if self.new_title && (episodes.is_empty() || detail.title.asset_id.is_some()) {
            out.push(MediaRef {
                ref_id: detail.title.id,
                label: detail.title.title.clone(),
                asset_id: detail.title.asset_id,
            });
        }
        for e in episodes {
            out.push(MediaRef {
                ref_id: e.id,
                label: format!(
                    "S{:02}E{:02}{}",
                    e.season_number,
                    e.number,
                    if e.title.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", e.title)
                    }
                ),
                asset_id: e.asset_id,
            });
        }
        Ok(out)
    }

    /// Delete the unpublished draft of a rejected or abandoned submission and discard the videos
    /// it uploaded (originals and transcode output); published content stays (G-61).
    async fn purge(&self, s: &Submission, items: &[SubmissionItem]) -> ReviewResult<()> {
        let by = sponsor(s.submitter_id.unwrap_or(0));
        let title_id = match s.ref_id {
            Some(t) => t,
            None => return Ok(()),
        };
        let detail = match self.svc.admin_title(by, title_id).await {
            Ok(d) => d,
            Err(CinemaError::NotFound(_)) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let mut assets: Vec<i64> = Vec::new();
        if self.new_title {
            if detail.title.published {
                return Ok(());
            }
            assets.extend(detail.title.asset_id);
            assets.extend(detail.episodes.iter().filter_map(|e| e.asset_id));
            match self.svc.delete_title(by, title_id).await {
                Ok(()) | Err(CinemaError::NotFound(_)) => {}
                Err(e) => return Err(e.into()),
            }
        } else {
            for it in items
                .iter()
                .filter(|i| i.state != sr_review::model::ItemState::Published)
            {
                let Some(ep) = detail.episodes.iter().find(|e| e.id == it.ref_id) else {
                    continue;
                };
                if ep.published {
                    continue;
                }
                assets.extend(ep.asset_id);
                match self.svc.delete_episode(by, ep.id).await {
                    Ok(()) | Err(CinemaError::NotFound(_)) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        if !assets.is_empty() {
            self.assets
                .discard(&assets)
                .await
                .map_err(ReviewError::Storage)?;
        }
        Ok(())
    }

    async fn publish(&self, s: &Submission, approved: &[SubmissionItem]) -> ReviewResult<()> {
        if approved.is_empty() {
            return Err(ReviewError::Conflict(
                "nothing was approved to publish".into(),
            ));
        }
        let by = sponsor(s.decided_by.or(s.submitter_id).unwrap_or(0));
        let title = Self::title_id(s)?;
        let detail = self.svc.admin_title(by, title).await?;
        if !self.new_title && !detail.title.published {
            return Err(ReviewError::Conflict(
                "the series is no longer published".into(),
            ));
        }
        for it in approved {
            if it.ref_id == title {
                continue;
            }
            self.svc.set_episode_published(by, it.ref_id, true).await?;
        }
        if self.new_title && !detail.title.published {
            self.svc.set_title_published(by, title, true).await?;
        }
        Ok(())
    }
}
