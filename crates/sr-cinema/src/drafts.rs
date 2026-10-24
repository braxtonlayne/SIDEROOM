//! What a member may do to the draft behind a Cinema submission (RDY-MED-001): edit the draft
//! title, add, edit and delete its draft episodes, and attach an uploaded video to the film or to an
//! episode. Everything runs with the same *sponsored* actor as [`crate::submissions`] and stays inside
//! the one title of the submission: an episode that is published, or that the submission does not own,
//! is not reachable, and a video must be an upload of the person asking.
//!
//! The HTTP routes (`/submissions/{id}/cinema/*`, crate `sr-api`) are thin: they pick the submission
//! with the review service (who may write, which state) and call these functions. Upload sessions
//! (the multipart upload id behind a draft video) are kept behind [`UploadSessions`].

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use sr_review::model::{Submission, SubmissionItem};
use sr_review::{ReviewError, ReviewResult};
use utoipa::ToSchema;

use crate::model::*;
use crate::service::CinemaService;
use crate::submissions::{AssetLookup, KIND_EPISODES, KIND_TITLE, MAX_EPISODES, sponsor};

/// The draft behind a submission, as its submitter edits it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SubmissionDraft {
    /// A new title (`cinema.title`) rather than new episodes for a published series.
    pub new_title: bool,
    pub title: Title,
    pub seasons: Vec<Season>,
    /// The draft episodes of this submission, by season and number.
    pub episodes: Vec<Episode>,
    /// Published episodes the title already had (new episodes for a series only).
    pub published_episodes: u32,
}

/// What a draft video belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The film itself.
    Title(i64),
    Episode(i64),
}

impl Target {
    pub fn id(self) -> i64 {
        match self {
            Self::Title(i) | Self::Episode(i) => i,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Title(_) => "title",
            Self::Episode(_) => "episode",
        }
    }

    pub fn parse(kind: &str, id: i64) -> Option<Self> {
        match kind {
            "title" => Some(Self::Title(id)),
            "episode" => Some(Self::Episode(id)),
            _ => None,
        }
    }
}

/// The multipart upload behind a draft video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadSession {
    pub asset_id: i64,
    pub submission_id: i64,
    pub target: Target,
    /// The object store's multipart upload id.
    pub s3_upload_id: String,
    pub part_size: u64,
    pub part_count: u32,
    pub size_bytes: u64,
}

/// Where upload sessions are remembered (`sr-db` implements it over Postgres).
#[async_trait]
pub trait UploadSessions: Send + Sync {
    async fn put(&self, s: &UploadSession) -> Result<(), String>;
    async fn get(&self, asset_id: i64) -> Result<Option<UploadSession>, String>;
    async fn for_submission(&self, submission_id: i64) -> Result<Vec<UploadSession>, String>;
    async fn remove(&self, asset_id: i64) -> Result<(), String>;
}

/// Sessions in memory (tests).
#[derive(Default)]
pub struct MemorySessions(Mutex<BTreeMap<i64, UploadSession>>);

#[async_trait]
impl UploadSessions for MemorySessions {
    async fn put(&self, s: &UploadSession) -> Result<(), String> {
        self.0.lock().unwrap().insert(s.asset_id, s.clone());
        Ok(())
    }

    async fn get(&self, asset_id: i64) -> Result<Option<UploadSession>, String> {
        Ok(self.0.lock().unwrap().get(&asset_id).cloned())
    }

    async fn for_submission(&self, submission_id: i64) -> Result<Vec<UploadSession>, String> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.submission_id == submission_id)
            .cloned()
            .collect())
    }

    async fn remove(&self, asset_id: i64) -> Result<(), String> {
        self.0.lock().unwrap().remove(&asset_id);
        Ok(())
    }
}

fn ids_in_payload(s: &Submission) -> Vec<i64> {
    s.payload
        .get("episode_ids")
        .and_then(serde_json::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().and_then(|x| x.parse().ok()))
                .collect()
        })
        .unwrap_or_default()
}

/// Draft operations of the Cinema kinds.
#[derive(Clone)]
pub struct DraftOps {
    svc: CinemaService,
    assets: Arc<dyn AssetLookup>,
}

impl DraftOps {
    pub fn new(svc: CinemaService, assets: Arc<dyn AssetLookup>) -> Self {
        Self { svc, assets }
    }

    fn new_title(s: &Submission) -> ReviewResult<bool> {
        match s.kind.as_str() {
            KIND_TITLE => Ok(true),
            KIND_EPISODES => Ok(false),
            _ => Err(ReviewError::Conflict(
                "this submission is not a Cinema submission".into(),
            )),
        }
    }

    fn title_id(s: &Submission) -> ReviewResult<i64> {
        s.ref_id
            .ok_or_else(|| ReviewError::Storage("the submission has no title".into()))
    }

    async fn detail(&self, s: &Submission) -> ReviewResult<AdminTitleDetail> {
        Ok(self.svc.admin_title(sponsor(0), Self::title_id(s)?).await?)
    }

    /// The draft episodes this submission owns. For a new title that is every episode; for new
    /// episodes of a published series it is the unpublished ones the submission created.
    fn owned<'a>(
        s: &Submission,
        items: &[SubmissionItem],
        detail: &'a AdminTitleDetail,
        new_title: bool,
    ) -> Vec<&'a Episode> {
        if new_title {
            return detail.episodes.iter().collect();
        }
        let mut ids = ids_in_payload(s);
        ids.extend(items.iter().map(|i| i.ref_id));
        detail
            .episodes
            .iter()
            .filter(|e| !e.published && ids.contains(&e.id))
            .collect()
    }

    /// The draft as the submitter sees it.
    pub async fn view(
        &self,
        s: &Submission,
        items: &[SubmissionItem],
    ) -> ReviewResult<SubmissionDraft> {
        let new_title = Self::new_title(s)?;
        let detail = self.detail(s).await?;
        let owned: Vec<Episode> = Self::owned(s, items, &detail, new_title)
            .into_iter()
            .cloned()
            .collect();
        let published_episodes = if new_title {
            0
        } else {
            u32::try_from(detail.episodes.iter().filter(|e| e.published).count()).unwrap_or(0)
        };
        Ok(SubmissionDraft {
            new_title,
            title: detail.title,
            seasons: detail.seasons,
            episodes: owned,
            published_episodes,
        })
    }

    /// Edit the metadata of the draft title (new titles only). Artwork keys stay as they are.
    pub async fn update_title(
        &self,
        s: &Submission,
        user: i64,
        mut input: TitleInput,
    ) -> ReviewResult<Title> {
        if !Self::new_title(s)? {
            return Err(ReviewError::Conflict(
                "the series of this submission is already published; only its new episodes can be edited".into(),
            ));
        }
        // A title keeps the kind it was created with in a way members can see: a film stays a
        // film, a series stays a series. Changing it would strand the videos already attached.
        let detail = self.detail(s).await?;
        let has_videos =
            detail.title.asset_id.is_some() || detail.episodes.iter().any(|e| e.asset_id.is_some());
        let is_film = |k: TitleKind| matches!(k, TitleKind::Movie);
        if has_videos && is_film(input.kind) != is_film(detail.title.kind) {
            return Err(ReviewError::Conflict(
                "remove the uploaded videos before changing between a film and a series".into(),
            ));
        }
        input.poster_key = None;
        input.backdrop_key = None;
        input.logo_key = None;
        Ok(self
            .svc
            .update_title_keep_artwork(sponsor(user), detail.title.id, input)
            .await?)
    }

    async fn season_id(
        &self,
        user: i64,
        detail: &AdminTitleDetail,
        number: i32,
    ) -> ReviewResult<i64> {
        if let Some(x) = detail.seasons.iter().find(|x| x.number == number) {
            return Ok(x.id);
        }
        Ok(self
            .svc
            .create_season(
                sponsor(user),
                detail.title.id,
                SeasonInput {
                    number,
                    name: None,
                    synopsis: None,
                    poster_key: None,
                },
            )
            .await?
            .id)
    }

    fn check_series(detail: &AdminTitleDetail) -> ReviewResult<()> {
        if matches!(detail.title.kind, TitleKind::Series | TitleKind::Anime) {
            Ok(())
        } else {
            Err(ReviewError::Conflict(
                "only a series has episodes; a film has one video of its own".into(),
            ))
        }
    }

    /// Add a draft episode. `Conflict` when the season already has that number.
    pub async fn add_episode(
        &self,
        s: &Submission,
        user: i64,
        items: &[SubmissionItem],
        season: i32,
        input: EpisodeInput,
    ) -> ReviewResult<Episode> {
        let new_title = Self::new_title(s)?;
        let detail = self.detail(s).await?;
        Self::check_series(&detail)?;
        if detail.episodes.len() >= MAX_EPISODES {
            return Err(ReviewError::Invalid(format!(
                "at most {MAX_EPISODES} episodes per submission"
            )));
        }
        let _ = (items, new_title);
        if detail
            .episodes
            .iter()
            .any(|e| e.season_number == season && e.number == input.number)
        {
            return Err(ReviewError::Conflict(format!(
                "episode {} of season {season} already exists",
                input.number
            )));
        }
        let season_id = self.season_id(user, &detail, season).await?;
        let mut input = input;
        input.still_key = None;
        Ok(self
            .svc
            .create_episode(sponsor(user), season_id, input)
            .await?)
    }

    /// The draft episode a file belongs to: the one this submission already has at that place,
    /// or a new one named after the file's own title. A published episode is a conflict.
    pub async fn episode_for_file(
        &self,
        s: &Submission,
        user: i64,
        items: &[SubmissionItem],
        season: i32,
        number: i32,
        title_hint: Option<&str>,
    ) -> ReviewResult<(Episode, bool)> {
        let new_title = Self::new_title(s)?;
        let detail = self.detail(s).await?;
        Self::check_series(&detail)?;
        if let Some(e) = detail
            .episodes
            .iter()
            .find(|e| e.season_number == season && e.number == number)
        {
            let mine = Self::owned(s, items, &detail, new_title)
                .iter()
                .any(|o| o.id == e.id);
            return if mine {
                Ok((e.clone(), false))
            } else {
                Err(ReviewError::Conflict(format!(
                    "episode {number} of season {season} already exists in the catalog"
                )))
            };
        }
        let ep = self
            .add_episode(
                s,
                user,
                items,
                season,
                EpisodeInput {
                    number,
                    absolute_number: None,
                    title: title_hint.unwrap_or_default().to_owned(),
                    synopsis: String::new(),
                    air_date: None,
                    runtime_min: None,
                    still_key: None,
                },
            )
            .await?;
        Ok((ep, true))
    }

    async fn owned_episode(
        &self,
        s: &Submission,
        items: &[SubmissionItem],
        id: i64,
    ) -> ReviewResult<Episode> {
        let new_title = Self::new_title(s)?;
        let detail = self.detail(s).await?;
        Self::owned(s, items, &detail, new_title)
            .into_iter()
            .find(|e| e.id == id)
            .cloned()
            .ok_or(ReviewError::NotFound("episode"))
    }

    /// Edit the metadata of a draft episode; its number and season stay.
    pub async fn update_episode(
        &self,
        s: &Submission,
        user: i64,
        items: &[SubmissionItem],
        id: i64,
        mut input: EpisodeInput,
    ) -> ReviewResult<Episode> {
        let current = self.owned_episode(s, items, id).await?;
        let detail = self.detail(s).await?;
        if input.number != current.number
            && detail.episodes.iter().any(|e| {
                e.id != id && e.season_number == current.season_number && e.number == input.number
            })
        {
            return Err(ReviewError::Conflict(format!(
                "episode {} of season {} already exists",
                input.number, current.season_number
            )));
        }
        input.still_key = None;
        Ok(self.svc.update_episode(sponsor(user), id, input).await?)
    }

    /// Remove a draft episode and give up the video it had.
    pub async fn delete_episode(
        &self,
        s: &Submission,
        user: i64,
        items: &[SubmissionItem],
        id: i64,
    ) -> ReviewResult<()> {
        let ep = self.owned_episode(s, items, id).await?;
        self.svc.delete_episode(sponsor(user), id).await?;
        if let Some(a) = ep.asset_id {
            self.assets
                .discard(&[a])
                .await
                .map_err(ReviewError::Storage)?;
        }
        Ok(())
    }

    /// The film itself, or one of the draft episodes, checked to belong to the submission.
    pub async fn resolve_target(
        &self,
        s: &Submission,
        items: &[SubmissionItem],
        target: Target,
    ) -> ReviewResult<()> {
        match target {
            Target::Title(id) => {
                let detail = self.detail(s).await?;
                if !Self::new_title(s)? || detail.title.id != id {
                    return Err(ReviewError::NotFound("title"));
                }
                if matches!(detail.title.kind, TitleKind::Series | TitleKind::Anime) {
                    return Err(ReviewError::Conflict(
                        "a series has its videos on its episodes".into(),
                    ));
                }
                Ok(())
            }
            Target::Episode(id) => self.owned_episode(s, items, id).await.map(|_| ()),
        }
    }

    /// The asset currently attached to a target, if any.
    pub async fn attached(
        &self,
        s: &Submission,
        items: &[SubmissionItem],
        target: Target,
    ) -> ReviewResult<Option<i64>> {
        self.resolve_target(s, items, target).await?;
        let detail = self.detail(s).await?;
        Ok(match target {
            Target::Title(_) => detail.title.asset_id,
            Target::Episode(id) => detail
                .episodes
                .iter()
                .find(|e| e.id == id)
                .and_then(|e| e.asset_id),
        })
    }

    /// Attach an uploaded video to the film or an episode. It must be the person's own upload; the
    /// video it replaces, if any, is given up (an unpublished asset is collected by the retention
    /// sweep).
    pub async fn attach(
        &self,
        s: &Submission,
        user: i64,
        items: &[SubmissionItem],
        target: Target,
        asset: i64,
    ) -> ReviewResult<()> {
        let old = self.attached(s, items, target).await?;
        match self
            .assets
            .asset(asset)
            .await
            .map_err(ReviewError::Storage)?
        {
            Some(f) if f.uploaded_by == Some(user) => {}
            _ => return Err(ReviewError::Invalid(format!("unknown video {asset}"))),
        }
        let a = sponsor(user);
        match target {
            Target::Title(id) => {
                self.svc.link_title_asset(a, id, Some(asset)).await?;
            }
            Target::Episode(id) => {
                self.svc.link_episode_asset(a, id, Some(asset)).await?;
            }
        }
        if let Some(o) = old.filter(|o| *o != asset) {
            self.assets
                .discard(&[o])
                .await
                .map_err(ReviewError::Storage)?;
        }
        Ok(())
    }
}

impl DraftOps {
    /// Give up unpublished videos (an aborted upload, a replaced video): the retention sweep deletes
    /// their files after the usual grace period. Published assets are never touched.
    pub async fn discard(&self, ids: &[i64]) -> ReviewResult<()> {
        self.assets.discard(ids).await.map_err(ReviewError::Storage)
    }
}

impl DraftOps {
    /// Take back an episode that [`Self::episode_for_file`] just created when the upload it was
    /// created for could not start. Only a draft episode without a video of this title goes.
    pub async fn drop_new_episode(&self, s: &Submission, user: i64, id: i64) -> ReviewResult<()> {
        let detail = self.detail(s).await?;
        match detail.episodes.iter().find(|e| e.id == id) {
            Some(e) if !e.published && e.asset_id.is_none() => {
                self.svc.delete_episode(sponsor(user), id).await?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
