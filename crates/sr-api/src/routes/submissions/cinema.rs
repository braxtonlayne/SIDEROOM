//! The draft behind a Cinema submission and the member's video uploads (RDY-MED-001,
//! `/submissions/{id}/cinema/*`, spec `docs/specs/cinema-media.md` section 8.7).
//!
//! A member creates a `cinema.title` or `cinema.episodes` submission (`POST /submissions`), then:
//!
//! 1. edits the draft title and its episodes (`title`, `episodes`);
//! 2. for every video file, `POST files` with the file name and size: the server reads the season
//!    and episode from the name (`sr_media::naming`, the same rules as the naming tester) or from
//!    the request, creates the draft episode when it is new, reserves a media asset with the same
//!    multipart sessions the Library uses (the daily upload allowance is charged here) and returns
//!    the part URLs;
//! 3. `PUT`s the parts, and `complete` finishes the upload from what the store holds (the browser
//!    never needs the part ETags) and attaches the video to the film or episode. After a refresh
//!    `parts` says which parts arrived;
//! 4. the media pipeline probes and transcodes the video; `draft` and `uploads/{asset}` show the
//!    asset state. A submission is ready when every video has a published ladder (spec 11.7).
//!
//! All of it runs with the sponsored actor of the Cinema kinds, inside the one title of the
//! submission. Uploads are the submitter's alone; metadata edits are also open to the reviewer who
//! holds the claim (D-255), which shows on the timeline.

use crate::{
    error::{ApiResult, HttpError},
    openapi::ErrorBody,
    routes::{
        Ext,
        cinema_media::{self, MediaDeps},
        review::{MemberCtx, id, map_err},
    },
    state::AppState,
};
use axum::{
    Json,
    extract::{FromRequestParts, Path},
    http::{StatusCode, request::Parts},
};
use serde::{Deserialize, Serialize};
use sr_cinema::drafts::{DraftOps, SubmissionDraft, Target, UploadSession, UploadSessions};
use sr_cinema::model::{Episode, EpisodeInput, Title, TitleInput, TitleKind};
use sr_core::{ApiError, ErrorCode};
use sr_library::multipart::{
    PART_URL_TTL, PartStore, PartUrl, ReceivedPart, StoredPart, UploadParts, check_parts,
};
use sr_media::assets::{Asset, AssetState};
use sr_media::naming::{Role, parse_path};
use sr_media::service::{CompleteUploadRequest, UploadRequest};
use sr_media::worker_api::PartDone;
use sr_review::EditAs;
use sr_review::model::{Submission, SubmissionItem};
use sr_review::repo::NewItem;
use std::sync::Arc;
use utoipa::ToSchema;

/// Collaborators of the Cinema submission routes.
pub struct CinemaSubmitDeps {
    pub ops: DraftOps,
    pub sessions: Arc<dyn UploadSessions>,
    /// The object store's multipart operations (list, presign, abort). Absent when the storage
    /// cannot do multipart (`SR_MEDIA_SINGLE_PUT`): uploads are then one `PUT` and cannot resume.
    pub parts: Option<Arc<dyn PartStore>>,
}

impl CinemaSubmitDeps {
    pub fn new(ops: DraftOps, sessions: Arc<dyn UploadSessions>) -> Self {
        Self {
            ops,
            sessions,
            parts: None,
        }
    }

    pub fn with_parts(mut self, parts: Arc<dyn PartStore>) -> Self {
        self.parts = Some(parts);
        self
    }
}

/// The member context with the Cinema submission collaborators.
pub(crate) struct CinemaCtx {
    m: MemberCtx,
    deps: Arc<CinemaSubmitDeps>,
    media: Arc<MediaDeps>,
}

impl FromRequestParts<AppState> for CinemaCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let m = MemberCtx::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<CinemaSubmitDeps>::from_request_parts(parts, state).await?;
        let Ext(media) = Ext::<MediaDeps>::from_request_parts(parts, state).await?;
        Ok(Self { m, deps, media })
    }
}

fn cinema_kind(s: Submission) -> Result<Submission, HttpError> {
    if s.kind.starts_with("cinema.") {
        Ok(s)
    } else {
        Err(ApiError::with_message(
            ErrorCode::SrConflict,
            "This submission is not a Cinema submission",
        )
        .into())
    }
}

fn bad(m: impl Into<String>) -> HttpError {
    ApiError::with_message(ErrorCode::InvalidFormBody, m.into()).into()
}

fn unavailable(m: &str) -> HttpError {
    ApiError::with_message(ErrorCode::SrConflict, m.to_owned()).into()
}

fn gone(what: &str) -> HttpError {
    ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {what}")).into()
}

fn internal(what: &str, e: impl std::fmt::Display) -> HttpError {
    tracing::error!(error = %e, "{what}");
    ErrorCode::SrInternal.into()
}

impl CinemaCtx {
    fn user(&self) -> i64 {
        self.m.user()
    }

    async fn readable(&self, raw: &str) -> Result<Submission, HttpError> {
        cinema_kind(
            self.m
                .svc()
                .readable_for(self.m.caller, id(raw, "submission")?)
                .await
                .map_err(map_err)?,
        )
    }

    async fn editable_mine(&self, raw: &str) -> Result<Submission, HttpError> {
        cinema_kind(
            self.m
                .svc()
                .editable_mine(self.user(), id(raw, "submission")?)
                .await
                .map_err(map_err)?,
        )
    }

    /// The submitter while it is a draft or has changes requested, or the reviewer holding the claim.
    async fn editable_any(&self, raw: &str) -> Result<(Submission, EditAs), HttpError> {
        let (s, who) = self
            .m
            .svc()
            .editable_for(self.m.caller, id(raw, "submission")?)
            .await
            .map_err(map_err)?;
        Ok((cinema_kind(s)?, who))
    }

    async fn note(&self, s: &Submission, who: EditAs, field: &str) -> Result<(), HttpError> {
        if who == EditAs::Reviewer {
            self.m
                .svc()
                .note_reviewer_edit(self.m.caller, s.id, &[field])
                .await
                .map_err(map_err)?;
        }
        Ok(())
    }

    /// The items registered on the submission, for the ownership checks of the episodes kind.
    async fn items(&self, s: &Submission) -> Result<Vec<SubmissionItem>, HttpError> {
        let d = if s.submitter_id == Some(self.user()) {
            self.m.svc().get_mine(self.user(), s.id).await
        } else {
            self.m.svc().get_for_review(self.m.caller, s.id).await
        };
        Ok(d.map_err(map_err)?.items)
    }

    async fn register(&self, s: &Submission, item: NewItem) -> Result<(), HttpError> {
        self.m
            .svc()
            .register_item_for(self.m.caller, s.id, item)
            .await
            .map_err(map_err)
    }

    async fn asset(&self, id: i64) -> Result<Asset, HttpError> {
        self.media
            .media
            .get(id)
            .await
            .map_err(cinema_media::map_err)
    }
}

fn item_label(season: i32, number: i32, title: &str) -> String {
    let base = format!("S{season:02}E{number:02}");
    if title.trim().is_empty() {
        base
    } else {
        format!("{base} {title}")
    }
}

fn episode_item(e: &Episode) -> NewItem {
    NewItem {
        ref_id: e.id,
        label: item_label(e.season_number, e.number, &e.title),
        position: e
            .season_number
            .saturating_mul(1000)
            .saturating_add(e.number),
    }
}

// ---- wire types -------------------------------------------------------------------------------

/// A video of the draft and where its processing stands.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubmissionVideo {
    pub asset_id: String,
    /// `awaiting_upload`, `uploaded`, `probing`, `proxy_ready`, `in_review`, `approved`,
    /// `transcoding`, `published` (the ladder is ready), or `failed`, `rejected`, `removed`, `expired`.
    pub state: AssetState,
    /// Why it `failed`, in words for the member.
    pub error: Option<String>,
    pub filename: String,
    pub size_bytes: i64,
    pub duration_ms: Option<i64>,
    /// A review proxy exists: the reviewers can already watch it.
    pub has_proxy: bool,
    /// The ladder is published; the video can be played.
    pub playable: bool,
}

impl SubmissionVideo {
    fn of(a: &Asset) -> Self {
        Self {
            asset_id: a.id.to_string(),
            state: a.state,
            error: a.error.clone(),
            filename: a.filename.clone(),
            size_bytes: a.original_size,
            duration_ms: a.duration_ms,
            has_proxy: a.has_proxy,
            playable: a.playable,
        }
    }
}

/// One video slot of the draft: the film, or one episode.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubmissionVideoItem {
    /// `title` (the film) or `episode`.
    pub target_kind: String,
    pub target_id: String,
    pub season_number: Option<i32>,
    pub number: Option<i32>,
    pub label: String,
    /// The attached video, once an upload completed.
    pub video: Option<SubmissionVideo>,
}

/// An upload that has not completed yet.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubmissionPendingUpload {
    pub video: SubmissionVideo,
    pub target_kind: String,
    pub target_id: String,
    pub part_size: u64,
    pub part_count: u32,
}

/// The whole draft with the state of every video.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubmissionDraftView {
    pub draft: SubmissionDraft,
    pub items: Vec<SubmissionVideoItem>,
    pub pending: Vec<SubmissionPendingUpload>,
}

async fn video_of(ctx: &CinemaCtx, asset: Option<i64>) -> Option<SubmissionVideo> {
    match asset {
        Some(a) => ctx.asset(a).await.ok().map(|a| SubmissionVideo::of(&a)),
        None => None,
    }
}

async fn build_view(ctx: &CinemaCtx, s: &Submission) -> Result<SubmissionDraftView, HttpError> {
    let items = ctx.items(s).await?;
    let draft = ctx.deps.ops.view(s, &items).await.map_err(map_err)?;
    let mut out = Vec::new();
    if draft.new_title && draft.title.kind == TitleKind::Movie {
        out.push(SubmissionVideoItem {
            target_kind: "title".into(),
            target_id: draft.title.id.to_string(),
            season_number: None,
            number: None,
            label: draft.title.title.clone(),
            video: video_of(ctx, draft.title.asset_id).await,
        });
    } else {
        for e in &draft.episodes {
            out.push(SubmissionVideoItem {
                target_kind: "episode".into(),
                target_id: e.id.to_string(),
                season_number: Some(e.season_number),
                number: Some(e.number),
                label: item_label(e.season_number, e.number, &e.title),
                video: video_of(ctx, e.asset_id).await,
            });
        }
        out.sort_by_key(|i| (i.season_number, i.number));
    }
    let mut pending = Vec::new();
    for sess in ctx
        .deps
        .sessions
        .for_submission(s.id)
        .await
        .map_err(|e| internal("upload sessions", e))?
    {
        if let Ok(a) = ctx.asset(sess.asset_id).await
            && a.uploaded_by == Some(ctx.user())
        {
            pending.push(SubmissionPendingUpload {
                video: SubmissionVideo::of(&a),
                target_kind: sess.target.as_str().into(),
                target_id: sess.target.id().to_string(),
                part_size: sess.part_size,
                part_count: sess.part_count,
            });
        }
    }
    Ok(SubmissionDraftView {
        draft,
        items: out,
        pending,
    })
}

/// The draft: its title, seasons, draft episodes, and every video with its processing state.
/// Readable by the submitter and by reviewers of the kind.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/cinema/draft", operation_id = "submissions_cinema_draft",
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = SubmissionDraftView), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn draft(
    ctx: CinemaCtx,
    Path(submission_id): Path<String>,
) -> ApiResult<Json<SubmissionDraftView>> {
    let s = ctx.readable(&submission_id).await?;
    Ok(Json(build_view(&ctx, &s).await?))
}

/// Edit the metadata of the draft title (new titles only). Artwork has its own route
/// (`POST /submissions/{id}/artwork/{kind}`); the stored artwork stays.
#[utoipa::path(
    put, path = "/submissions/{submission_id}/cinema/title", operation_id = "submissions_cinema_update_title", request_body = TitleInput,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 200, body = Title), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_title(
    ctx: CinemaCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<TitleInput>,
) -> ApiResult<Json<Title>> {
    let (s, who) = ctx.editable_any(&submission_id).await?;
    let t = ctx
        .deps
        .ops
        .update_title(&s, ctx.user(), b)
        .await
        .map_err(map_err)?;
    ctx.note(&s, who, "title").await?;
    Ok(Json(t))
}

/// An episode to add to the draft.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmissionEpisodeInput {
    /// Season number; default 1. A season that does not exist yet is created.
    pub season: Option<i32>,
    pub number: i32,
    pub absolute_number: Option<i32>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub synopsis: String,
    /// `YYYY-MM-DD`.
    pub air_date: Option<String>,
    pub runtime_min: Option<i32>,
}

impl SubmissionEpisodeInput {
    fn input(&self) -> EpisodeInput {
        EpisodeInput {
            number: self.number,
            absolute_number: self.absolute_number,
            title: self.title.clone(),
            synopsis: self.synopsis.clone(),
            air_date: self.air_date.clone(),
            runtime_min: self.runtime_min,
            still_key: None,
        }
    }
}

/// Add a draft episode (series only). Upload its video next, or let `files` create the episode from
/// the file name.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/cinema/episodes", operation_id = "submissions_cinema_add_episode", request_body = SubmissionEpisodeInput,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 201, body = Episode), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn add_episode(
    ctx: CinemaCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<SubmissionEpisodeInput>,
) -> ApiResult<(StatusCode, Json<Episode>)> {
    let s = ctx.editable_mine(&submission_id).await?;
    let items = ctx.items(&s).await?;
    let ep = ctx
        .deps
        .ops
        .add_episode(&s, ctx.user(), &items, b.season.unwrap_or(1), b.input())
        .await
        .map_err(map_err)?;
    ctx.register(&s, episode_item(&ep)).await?;
    Ok((StatusCode::CREATED, Json(ep)))
}

/// Edit a draft episode of the submission; its season and number may change within the season.
#[utoipa::path(
    put, path = "/submissions/{submission_id}/cinema/episodes/{episode_id}", operation_id = "submissions_cinema_update_episode", request_body = EpisodeInput,
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 200, body = Episode), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn update_episode(
    ctx: CinemaCtx,
    Path((submission_id, episode_id)): Path<(String, String)>,
    Json(b): Json<EpisodeInput>,
) -> ApiResult<Json<Episode>> {
    let (s, who) = ctx.editable_any(&submission_id).await?;
    let items = ctx.items(&s).await?;
    let ep = ctx
        .deps
        .ops
        .update_episode(&s, ctx.user(), &items, id(&episode_id, "episode")?, b)
        .await
        .map_err(map_err)?;
    ctx.register(&s, episode_item(&ep)).await?;
    ctx.note(&s, who, "episodes").await?;
    Ok(Json(ep))
}

/// Remove a draft episode and give up its video.
#[utoipa::path(
    delete, path = "/submissions/{submission_id}/cinema/episodes/{episode_id}", operation_id = "submissions_cinema_delete_episode",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("episode_id" = String, Path, description = "Episode snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn delete_episode(
    ctx: CinemaCtx,
    Path((submission_id, episode_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let s = ctx.editable_mine(&submission_id).await?;
    let items = ctx.items(&s).await?;
    let eid = id(&episode_id, "episode")?;
    ctx.deps
        .ops
        .delete_episode(&s, ctx.user(), &items, eid)
        .await
        .map_err(map_err)?;
    ctx.m
        .svc()
        .unregister_item_for(ctx.m.caller, s.id, eid)
        .await
        .map_err(map_err)?;
    // An upload still open for this episode has nowhere to go.
    for sess in ctx
        .deps
        .sessions
        .for_submission(s.id)
        .await
        .map_err(|e| internal("upload sessions", e))?
    {
        if sess.target == Target::Episode(eid) {
            drop_session(&ctx, &sess).await;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// A video file to upload.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SubmissionVideoRequest {
    /// The file name on the member's disk. For a series the season and episode are read from it
    /// (`Show - s01e02.mkv`, the naming tester's rules) unless `season` and `episode` are given.
    pub filename: String,
    pub size_bytes: u64,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    /// Attach to this draft episode of the submission instead of reading the name.
    pub episode_id: Option<String>,
}

/// The reserved upload.
#[derive(Debug, Serialize, ToSchema)]
pub struct SubmissionVideoSlot {
    /// The film or episode the video will be attached to.
    pub item: SubmissionVideoItem,
    /// The draft episode was created for this file.
    pub created: bool,
    pub asset_id: String,
    /// Every part but the last has exactly this size.
    pub part_size: u64,
    pub part_count: u32,
    /// One presigned `PUT` per part.
    pub parts: Vec<PartUrl>,
    pub expires_in_secs: u64,
    /// Things the file name made unclear (for example a file that covers two episodes).
    pub warnings: Vec<String>,
    /// The storage resumes uploads: `parts` can be asked again after a refresh.
    pub resumable: bool,
}

struct Picked {
    target: Target,
    season: Option<i32>,
    number: Option<i32>,
    label: String,
    created: bool,
    warnings: Vec<String>,
}

async fn pick_target(
    ctx: &CinemaCtx,
    s: &Submission,
    items: &[SubmissionItem],
    b: &SubmissionVideoRequest,
) -> Result<Picked, HttpError> {
    let ops = &ctx.deps.ops;
    if let Some(raw) = b.episode_id.as_deref() {
        let eid = id(raw, "episode")?;
        let view = ops.view(s, items).await.map_err(map_err)?;
        let ep = view
            .episodes
            .iter()
            .find(|e| e.id == eid)
            .ok_or_else(|| gone("episode"))?;
        return Ok(Picked {
            target: Target::Episode(eid),
            season: Some(ep.season_number),
            number: Some(ep.number),
            label: item_label(ep.season_number, ep.number, &ep.title),
            created: false,
            warnings: vec![],
        });
    }
    let view = ops.view(s, items).await.map_err(map_err)?;
    let parsed = parse_path(&b.filename);
    if matches!(parsed.role, Role::Subtitle | Role::Artwork) {
        return Err(bad(
            "send a video file; subtitles and artwork are not uploaded here",
        ));
    }
    if view.title.kind == TitleKind::Movie {
        return Ok(Picked {
            target: Target::Title(view.title.id),
            season: None,
            number: None,
            label: view.title.title.clone(),
            created: false,
            warnings: vec![],
        });
    }
    let mut warnings = Vec::new();
    let number = b
        .episode
        .or_else(|| parsed.episodes.first().and_then(|n| i32::try_from(*n).ok()));
    let season = b
        .season
        .or_else(|| parsed.season.and_then(|n| i32::try_from(n).ok()))
        .or(if parsed.special { Some(0) } else { None })
        .or(number.map(|_| 1));
    let (Some(season), Some(number)) = (season, number) else {
        return Err(bad(
            "could not read the season and episode from the file name; name it like \
             \"Show - s01e02.mkv\" or send season and episode",
        ));
    };
    if b.episode.is_none() && parsed.episodes.len() > 1 {
        warnings.push(format!(
            "this file covers episodes {}; it is attached to episode {number}",
            parsed
                .episodes
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let hint = parsed.episode_title.as_deref().filter(|t| !t.is_empty());
    let (ep, created) = ops
        .episode_for_file(s, ctx.user(), items, season, number, hint)
        .await
        .map_err(map_err)?;
    Ok(Picked {
        target: Target::Episode(ep.id),
        season: Some(season),
        number: Some(number),
        label: item_label(season, number, &ep.title),
        created,
        warnings,
    })
}

/// Reserve the upload of one video file and get the presigned part URLs.
///
/// The file must be a known video container within the operator's size limit
/// (`cinema.upload.max_gb`), and it counts against the daily upload allowance (50 GB for members,
/// staff exempt). `PUT` every part to its URL, then call `complete`. A refreshed page asks `parts`
/// which parts arrived and sends only the missing ones.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/cinema/files", operation_id = "submissions_cinema_begin_upload", request_body = SubmissionVideoRequest,
    params(("submission_id" = String, Path, description = "Submission snowflake")),
    responses((status = 201, body = SubmissionVideoSlot), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody, description = "Not editable, the name clashes with a published episode, or the daily allowance is used up."),
              (status = 503, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn begin_upload(
    ctx: CinemaCtx,
    Path(submission_id): Path<String>,
    Json(b): Json<SubmissionVideoRequest>,
) -> ApiResult<(StatusCode, Json<SubmissionVideoSlot>)> {
    let s = ctx.editable_mine(&submission_id).await?;
    let items = ctx.items(&s).await?;
    let picked = pick_target(&ctx, &s, &items, &b).await?;
    let (title_id, episode_id) = match picked.target {
        Target::Title(t) => (Some(t), None),
        Target::Episode(e) => (s.ref_id, Some(e)),
    };
    let slot = ctx
        .media
        .media
        .begin_upload(
            ctx.user(),
            UploadRequest {
                filename: b.filename.clone(),
                size_bytes: b.size_bytes,
                title_id,
                episode_id,
                auto_ladder: None,
            },
        )
        .await;
    let slot = match slot {
        Ok(slot) => slot,
        Err(e) => {
            if picked.created {
                let _ = ctx
                    .deps
                    .ops
                    .drop_new_episode(&s, ctx.user(), picked.target.id())
                    .await;
            }
            return Err(cinema_media::map_err(e));
        }
    };
    // The file passed the media checks; now count it against the daily allowance. Refused here, the
    // slot is never handed out and the reserved asset is given up.
    if let Err(e) = ctx.m.svc().charge_upload(ctx.m.caller, b.size_bytes).await {
        let _ = ctx.deps.ops.discard(&[slot.asset.id]).await;
        if picked.created {
            let _ = ctx
                .deps
                .ops
                .drop_new_episode(&s, ctx.user(), picked.target.id())
                .await;
        }
        return Err(map_err(e));
    }
    let part_count = u32::try_from(slot.parts.len()).unwrap_or(u32::MAX);
    ctx.deps
        .sessions
        .put(&UploadSession {
            asset_id: slot.asset.id,
            submission_id: s.id,
            target: picked.target,
            s3_upload_id: slot.upload_id.clone(),
            part_size: slot.part_size,
            part_count,
            size_bytes: b.size_bytes,
        })
        .await
        .map_err(|e| internal("upload session", e))?;
    // The film or episode is an item of the submission from now on.
    match picked.target {
        Target::Title(t) => {
            ctx.register(
                &s,
                NewItem {
                    ref_id: t,
                    label: picked.label.clone(),
                    position: 0,
                },
            )
            .await?;
        }
        Target::Episode(e) => {
            if picked.created {
                ctx.register(
                    &s,
                    NewItem {
                        ref_id: e,
                        label: picked.label.clone(),
                        position: picked
                            .season
                            .unwrap_or(0)
                            .saturating_mul(1000)
                            .saturating_add(picked.number.unwrap_or(0)),
                    },
                )
                .await?;
            }
        }
    }
    Ok((
        StatusCode::CREATED,
        Json(SubmissionVideoSlot {
            item: SubmissionVideoItem {
                target_kind: picked.target.as_str().into(),
                target_id: picked.target.id().to_string(),
                season_number: picked.season,
                number: picked.number,
                label: picked.label,
                video: None,
            },
            created: picked.created,
            asset_id: slot.asset.id.to_string(),
            part_size: slot.part_size,
            part_count,
            parts: slot
                .parts
                .iter()
                .map(|p| PartUrl {
                    part_number: p.part_number,
                    url: p.url.clone(),
                })
                .collect(),
            expires_in_secs: slot.expires_in_secs,
            warnings: picked.warnings,
            resumable: ctx.deps.parts.is_some(),
        }),
    ))
}

/// The upload's session and asset, checked to be the caller's and this submission's.
async fn own_upload(
    ctx: &CinemaCtx,
    s: &Submission,
    raw: &str,
) -> Result<(Option<UploadSession>, Asset), HttpError> {
    let aid = id(raw, "upload")?;
    let asset = ctx.asset(aid).await.map_err(|_| gone("upload"))?;
    if asset.uploaded_by != Some(ctx.user()) {
        return Err(gone("upload"));
    }
    let session = ctx
        .deps
        .sessions
        .get(aid)
        .await
        .map_err(|e| internal("upload session", e))?;
    match &session {
        Some(sess) if sess.submission_id == s.id => {}
        Some(_) => return Err(gone("upload")),
        None => {
            // A finished upload: it is the video of an item of this submission.
            let items = ctx.items(s).await?;
            let view = ctx.deps.ops.view(s, &items).await.map_err(map_err)?;
            let attached = view.title.asset_id == Some(aid)
                || view.episodes.iter().any(|e| e.asset_id == Some(aid));
            if !attached {
                return Err(gone("upload"));
            }
        }
    }
    Ok((session, asset))
}

/// Poll an upload of my submission: the asset state, from the upload through the probe and the
/// transcode.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/cinema/uploads/{asset_id}", operation_id = "submissions_cinema_upload_status",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 200, body = SubmissionVideo), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn upload_status(
    ctx: CinemaCtx,
    Path((submission_id, asset_id)): Path<(String, String)>,
) -> ApiResult<Json<SubmissionVideo>> {
    let s = cinema_kind(
        ctx.m
            .svc()
            .own(ctx.user(), id(&submission_id, "submission")?)
            .await
            .map_err(map_err)?,
    )?;
    let (_, asset) = own_upload(&ctx, &s, &asset_id).await?;
    Ok(Json(SubmissionVideo::of(&asset)))
}

/// Resume an upload: which parts arrived and fresh URLs for the missing ones.
#[utoipa::path(
    get, path = "/submissions/{submission_id}/cinema/uploads/{asset_id}/parts", operation_id = "submissions_cinema_upload_parts",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 200, body = UploadParts), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn upload_parts(
    ctx: CinemaCtx,
    Path((submission_id, asset_id)): Path<(String, String)>,
) -> ApiResult<Json<UploadParts>> {
    let s = ctx.editable_mine(&submission_id).await?;
    let (session, asset) = own_upload(&ctx, &s, &asset_id).await?;
    let (Some(sess), AssetState::AwaitingUpload) = (session, asset.state) else {
        return Err(unavailable("This upload is already complete"));
    };
    let store = ctx
        .deps
        .parts
        .as_ref()
        .ok_or_else(|| unavailable("Resumable uploads are not available on this server"))?;
    let stored = store
        .list_parts(&asset.original_key, &sess.s3_upload_id)
        .await
        .map_err(|e| internal("listing parts", e))?;
    let want = |n: u32| {
        if n == sess.part_count {
            sess.size_bytes
                .saturating_sub(sess.part_size * u64::from(sess.part_count - 1))
        } else {
            sess.part_size
        }
    };
    let mut received = Vec::new();
    let mut missing = Vec::new();
    for n in 1..=sess.part_count {
        match stored.iter().find(|p| p.number == n) {
            Some(p) if p.size == want(n) => received.push(ReceivedPart {
                part_number: n,
                size: p.size,
            }),
            _ => missing.push(PartUrl {
                part_number: n,
                url: store
                    .presign_part(&asset.original_key, &sess.s3_upload_id, n, PART_URL_TTL)
                    .map_err(|e| internal("presigning a part", e))?,
            }),
        }
    }
    Ok(Json(UploadParts {
        part_size: sess.part_size,
        part_count: sess.part_count,
        received,
        missing,
        expires_in_secs: PART_URL_TTL.as_secs(),
    }))
}

/// The parts are uploaded: complete the upload from what the store holds, attach the video to its
/// film or episode (replacing, and giving up, a video it had) and queue the probe. Repeating the call
/// for a finished upload returns its state.
#[utoipa::path(
    post, path = "/submissions/{submission_id}/cinema/uploads/{asset_id}/complete", operation_id = "submissions_cinema_complete_upload",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 202, body = SubmissionVideo), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn complete_upload(
    ctx: CinemaCtx,
    Path((submission_id, asset_id)): Path<(String, String)>,
) -> ApiResult<(StatusCode, Json<SubmissionVideo>)> {
    let s = ctx.editable_mine(&submission_id).await?;
    let (session, asset) = own_upload(&ctx, &s, &asset_id).await?;
    let Some(sess) = session else {
        return Ok((StatusCode::ACCEPTED, Json(SubmissionVideo::of(&asset))));
    };
    if asset.state == AssetState::AwaitingUpload {
        let parts: Vec<PartDone> = match &ctx.deps.parts {
            Some(store) => {
                let stored = store
                    .list_parts(&asset.original_key, &sess.s3_upload_id)
                    .await
                    .map_err(|e| internal("listing parts", e))?;
                let checked: Vec<StoredPart> =
                    check_parts(&stored, sess.size_bytes, sess.part_size, sess.part_count)
                        .map_err(|m| unavailable(&m))?;
                checked
                    .into_iter()
                    .map(|p| PartDone {
                        part_number: p.number,
                        etag: p.etag,
                    })
                    .collect()
            }
            None => vec![PartDone {
                part_number: 1,
                etag: String::new(),
            }],
        };
        let done = ctx
            .media
            .media
            .complete_upload(
                asset.id,
                CompleteUploadRequest {
                    upload_id: sess.s3_upload_id.clone(),
                    parts,
                },
            )
            .await;
        if let Err(e) = done {
            // A refused file (empty, over the limit, wrong size) is failed for good: forget its session.
            if let Ok(a) = ctx.asset(asset.id).await
                && a.state != AssetState::AwaitingUpload
            {
                let _ = ctx.deps.sessions.remove(asset.id).await;
            }
            return Err(cinema_media::map_err(e));
        }
    }
    let items = ctx.items(&s).await?;
    ctx.deps
        .ops
        .attach(&s, ctx.user(), &items, sess.target, asset.id)
        .await
        .map_err(map_err)?;
    let _ = ctx.deps.sessions.remove(asset.id).await;
    let asset = ctx.asset(asset.id).await?;
    Ok((StatusCode::ACCEPTED, Json(SubmissionVideo::of(&asset))))
}

async fn drop_session(ctx: &CinemaCtx, sess: &UploadSession) {
    if let (Some(store), Ok(a)) = (&ctx.deps.parts, ctx.asset(sess.asset_id).await) {
        let _ = store.abort(&a.original_key, &sess.s3_upload_id).await;
    }
    let _ = ctx.deps.ops.discard(&[sess.asset_id]).await;
    let _ = ctx.deps.sessions.remove(sess.asset_id).await;
}

/// Give an upload up: the parts the store holds are freed and the reserved video is discarded.
#[utoipa::path(
    delete, path = "/submissions/{submission_id}/cinema/uploads/{asset_id}", operation_id = "submissions_cinema_abort_upload",
    params(("submission_id" = String, Path, description = "Submission snowflake"),
           ("asset_id" = String, Path, description = "Asset snowflake")),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    security(("session" = []))
)]
pub async fn abort_upload(
    ctx: CinemaCtx,
    Path((submission_id, asset_id)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    let s = ctx.editable_mine(&submission_id).await?;
    let (session, _) = own_upload(&ctx, &s, &asset_id).await?;
    let Some(sess) = session else {
        return Err(unavailable(
            "This upload is already complete; upload another video to replace it",
        ));
    };
    drop_session(&ctx, &sess).await;
    Ok(StatusCode::NO_CONTENT)
}
