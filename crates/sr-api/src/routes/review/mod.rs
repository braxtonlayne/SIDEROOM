//! Submissions and review (REV-*): the member side (`/submissions`, `/requests`) and the
//! reviewer side (`/admin/review/*`, staff sessions with the kind's review permission).
//!
//! * A member creates a submission of a kind (`library.series`, `library.items`), builds its
//!   content (Library kinds: draft chapters and uploads through
//!   `/submissions/{id}/library/*`), submits, and follows it in "My submissions".
//! * Reviewers claim from the queue (a 30 minute lease), preview, comment, decide per item or
//!   as a whole, and approve to publish. Every reviewer decision is also written to the staff
//!   access log. Permission checks live in [`sr_review::ReviewService`]; the extractors only
//!   establish who is calling.
//! * The submitter hears about every reviewer action as `SR_SUBMISSION_UPDATE` (doc 05).
//!
//! Wiring, in the composition root:
//!
//! ```ignore
//! let review = ReviewService::new(Arc::new(PgReviewRepo::new(pool)), kinds, clock, ids)
//!     .with_events(Arc::new(BusReviewEvents::new(bus)));
//! let deps = ReviewDeps::new(review).with_library(LibraryDraftOps::new(library_service));
//! let state = AppState::builder().extension(Arc::new(deps)).build();
//! ```
//! Reviewers additionally need [`AdminDeps`](crate::routes::admin::AdminDeps).

mod artwork;
mod duplicates;
mod library;
mod lookup;
mod media;
pub use lookup::{
    AniList, ComicVine, LookupCandidate, LookupDeps, LookupError, LookupProvider, LookupSwitch,
    MangaDex, MangaUpdates, RawCandidate, Section, Tmdb,
};
pub use media::ReviewMedia;
mod member;
mod requests;
mod reviewer;

#[cfg(test)]
mod tests;

use crate::{
    auth::UserOnly,
    error::HttpError,
    openapi::ErrorBody,
    routes::{
        Ext, MaybeExt,
        admin::{AdminDeps, StaffCtx},
    },
    state::AppState,
};
use axum::{
    Router,
    extract::FromRequestParts,
    http::request::Parts,
    routing::{get, post, put},
};
use serde::Deserialize;
use sr_core::{ApiError, ErrorCode, UserId};
use sr_library::submissions::LibraryDraftOps;
use sr_review::{Caller, ReviewError, ReviewService};
use std::sync::Arc;
use utoipa::{IntoParams, OpenApi};

/// Collaborators of the review endpoints.
pub struct ReviewDeps {
    pub service: ReviewService,
    /// Draft content operations of the Library kinds (absent when the Library is off).
    pub library: Option<LibraryDraftOps>,
    /// Review proxies of Cinema videos (absent when Cinema media is off).
    pub media: Option<ReviewMedia>,
}

impl ReviewDeps {
    pub fn new(service: ReviewService) -> Self {
        Self {
            service,
            library: None,
            media: None,
        }
    }

    pub fn with_media(mut self, media: ReviewMedia) -> Self {
        self.media = Some(media);
        self
    }

    pub fn with_library(mut self, ops: LibraryDraftOps) -> Self {
        self.library = Some(ops);
        self
    }
}

/// A signed-in member. When the account also has staff standing, its platform permissions
/// ride along: they lift the open-submission limit and allow fulfilling requests of the
/// section they review.
pub(crate) struct MemberCtx {
    pub deps: Arc<ReviewDeps>,
    pub caller: Caller,
}

impl MemberCtx {
    pub fn svc(&self) -> &ReviewService {
        &self.deps.service
    }

    pub fn user(&self) -> i64 {
        self.caller.user_id
    }
}

impl FromRequestParts<AppState> for MemberCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let UserOnly(p) = UserOnly::from_request_parts(parts, state).await?;
        let Ext(deps) = Ext::<ReviewDeps>::from_request_parts(parts, state).await?;
        let MaybeExt(admin) = MaybeExt::<AdminDeps>::from_request_parts(parts, state)
            .await
            .unwrap_or(MaybeExt(None));
        let permissions = match (&p.kind, admin) {
            (crate::auth::AuthKind::Session { .. }, Some(a)) => a
                .service
                .resolve(p.user_id, None)
                .await
                .map(|c| c.permissions)
                .unwrap_or_default(),
            _ => sr_core::PlatformPermissions::empty(),
        };
        Ok(Self {
            deps,
            caller: Caller {
                user_id: p.user_id,
                permissions,
            },
        })
    }
}

/// A staff session next to the review service.
pub(crate) struct ReviewerCtx {
    pub staff: StaffCtx,
    pub review: Arc<ReviewDeps>,
}

impl ReviewerCtx {
    pub fn svc(&self) -> &ReviewService {
        &self.review.service
    }

    pub fn caller(&self) -> Caller {
        Caller {
            user_id: self.staff.caller.user_id,
            permissions: self.staff.caller.permissions,
        }
    }

    /// Record a successful decision in the staff access log. The decision already happened,
    /// so a logging failure is reported but does not fail the request.
    pub async fn audit(&self, action: &str, submission: i64, detail: serde_json::Value) {
        let admin: &AdminDeps = &self.staff.deps;
        if let Err(e) = admin
            .service
            .record_action(
                &self.staff.caller,
                action,
                Some(("submission", submission)),
                detail,
            )
            .await
        {
            tracing::error!(error = %e, action, "review decision was not written to the access log");
        }
    }
}

impl FromRequestParts<AppState> for ReviewerCtx {
    type Rejection = HttpError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let staff = StaffCtx::from_request_parts(parts, state).await?;
        let Ext(review) = Ext::<ReviewDeps>::from_request_parts(parts, state).await?;
        Ok(Self { staff, review })
    }
}

/// Parse a snowflake path segment; a malformed one names nothing.
pub(crate) fn id(raw: &str, what: &'static str) -> Result<i64, HttpError> {
    raw.parse::<i64>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| not_found(what))
}

fn not_found(what: &str) -> HttpError {
    ApiError::with_message(ErrorCode::SrNotFound, format!("Unknown {what}")).into()
}

pub(crate) fn map_err(e: ReviewError) -> HttpError {
    match e {
        ReviewError::NotFound(w) => not_found(w),
        ReviewError::MissingPermission(p) => ApiError::with_message(
            ErrorCode::MissingPermissions,
            format!("Missing platform permission {p}"),
        )
        .into(),
        ReviewError::Forbidden(m) => ApiError::with_message(ErrorCode::MissingAccess, m).into(),
        ReviewError::Invalid(m) => ApiError::with_message(ErrorCode::InvalidFormBody, m).into(),
        ReviewError::Conflict(m) => ApiError::with_message(ErrorCode::SrConflict, m).into(),
        ReviewError::NotReady(problems) => {
            let mut e = ApiError::invalid_form();
            for (i, p) in problems.iter().enumerate() {
                e = e.field(format!("problems.{i}"), "SUBMISSION_NOT_READY", p.clone());
            }
            e.into()
        }
        ReviewError::Storage(m) => {
            tracing::error!(error = %m, "review storage failure");
            ErrorCode::SrInternal.into()
        }
    }
}

/// Listing parameters shared by "my submissions" and the review queue.
#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListParams {
    /// Comma-separated kind ids (`library.series,library.items`).
    pub kind: Option<String>,
    /// Comma-separated states. Default: all for "my submissions", `submitted,in_review` for the queue.
    pub state: Option<String>,
    /// Queue only: `mine` (my live claims) or `unclaimed`.
    pub claimed: Option<String>,
    /// Queue only: submitter account snowflake.
    pub submitter: Option<String>,
    /// Title substring.
    pub q: Option<String>,
    /// `submitted` (default), `updated` or `title`.
    pub sort: Option<String>,
    /// `asc` or `desc`; default `asc` (oldest waiting first).
    pub order: Option<String>,
    /// Queue only: only submissions waiting at least this many hours.
    pub min_age_hours: Option<u32>,
    /// 1 to 100, default 25.
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl ListParams {
    pub(crate) fn into_query(
        self,
        me: i64,
        now_ms: i64,
    ) -> Result<sr_review::model::SubmissionQuery, HttpError> {
        use sr_review::model::*;
        let invalid = |m: &str| -> HttpError {
            ApiError::with_message(ErrorCode::InvalidFormBody, m.to_owned()).into()
        };
        let csv = |raw: &Option<String>| -> Vec<String> {
            raw.iter()
                .flat_map(|s| s.split(','))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        };
        let mut states = Vec::new();
        for s in csv(&self.state) {
            states.push(SubmissionState::parse(&s).ok_or_else(|| invalid("unknown state"))?);
        }
        let sort = match self.sort.as_deref() {
            None => SortKey::Submitted,
            Some(s) => SortKey::parse(s).ok_or_else(|| invalid("unknown sort"))?,
        };
        let descending = match self.order.as_deref() {
            None | Some("asc") => false,
            Some("desc") => true,
            Some(_) => return Err(invalid("order must be asc or desc")),
        };
        let (claimed_by, unclaimed) = match self.claimed.as_deref() {
            None => (None, false),
            Some("mine") => (Some(me), false),
            Some("unclaimed") => (None, true),
            Some(_) => return Err(invalid("claimed must be mine or unclaimed")),
        };
        let submitter = match &self.submitter {
            None => None,
            Some(s) => Some(
                s.parse::<i64>()
                    .ok()
                    .filter(|v| *v > 0)
                    .ok_or_else(|| invalid("submitter is not a valid id"))?,
            ),
        };
        Ok(SubmissionQuery {
            kinds: csv(&self.kind),
            states,
            submitter,
            claimed_by,
            unclaimed,
            submitted_before_ms: self
                .min_age_hours
                .map(|h| now_ms - i64::from(h) * 3_600_000),
            title_contains: self
                .q
                .map(|q| q.trim().to_owned())
                .filter(|q| !q.is_empty()),
            sort,
            descending,
            limit: self.limit.unwrap_or(25).clamp(1, 100),
            offset: self.offset.unwrap_or(0),
        })
    }
}

/// Unix milliseconds now.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Delivers `SR_SUBMISSION_UPDATE` to the sessions of an account over the event bus.
/// Late-bound sender of system DMs (the Staff Server voice is built after the review service).
pub type DmSlot = Arc<std::sync::OnceLock<Arc<dyn crate::routes::developer::SystemDm>>>;

pub struct BusReviewEvents {
    bus: Arc<dyn sr_events::EventBus>,
    dm: DmSlot,
}

impl BusReviewEvents {
    pub fn new(bus: Arc<dyn sr_events::EventBus>) -> Self {
        Self {
            bus,
            dm: DmSlot::default(),
        }
    }

    /// Share the slot the composition root fills with the DM sender once it exists.
    #[must_use]
    pub fn with_dm_slot(mut self, dm: DmSlot) -> Self {
        self.dm = dm;
        self
    }
}

#[async_trait::async_trait]
impl sr_review::ReviewEvents for BusReviewEvents {
    async fn dm(&self, user: i64, text: String) {
        if let Some(sender) = self.dm.get() {
            let card = crate::routes::developer::SystemCard {
                content: text,
                ..Default::default()
            };
            sender.dm(user, card).await;
        }
    }

    async fn card(&self, submission_id: i64, outcome: Option<&str>) {
        let ev = sr_events::DomainEvent::ReviewCard {
            submission_id,
            outcome: outcome.map(str::to_owned),
        };
        if let Err(e) = self.bus.publish(sr_events::Topic::Platform, ev).await {
            tracing::warn!(error = %e, "publishing a review card event failed");
        }
    }

    async fn emit(&self, users: Vec<i64>, payload: serde_json::Value) {
        let users: Vec<UserId> = users
            .into_iter()
            .filter_map(|u| u64::try_from(u).ok().map(UserId::new))
            .collect();
        let ev = sr_events::DomainEvent::UserScoped {
            users: users.clone(),
            name: sr_review::EVENT_NAME.to_owned(),
            payload,
        };
        for u in users {
            if let Err(e) = self
                .bus
                .publish(sr_events::Topic::User(u), ev.clone())
                .await
            {
                tracing::warn!(error = %e, "publishing SR_SUBMISSION_UPDATE failed");
            }
        }
    }
}

/// Review routes (relative to the API version prefix).
pub fn router() -> Router<AppState> {
    Router::new()
        // Members.
        .route("/submissions/kinds", get(member::kinds))
        .route("/submissions/quota", get(member::quota))
        .route("/submissions/naming/parse", post(member::naming_parse))
        .route("/submissions", get(member::list).post(member::create))
        .route(
            "/submissions/{submission_id}",
            get(member::get).patch(member::update),
        )
        .route("/submissions/{submission_id}/preview", get(member::preview))
        .route("/submissions/{submission_id}/credits", get(member::credits))
        .route(
            "/submissions/{submission_id}/duplicates",
            get(duplicates::mine),
        )
        .route("/submissions/{submission_id}/submit", post(member::submit))
        .route(
            "/submissions/{submission_id}/withdraw",
            post(member::withdraw),
        )
        .route(
            "/submissions/{submission_id}/comments",
            post(member::comment),
        )
        // Library draft content of a submission.
        .route(
            "/submissions/{submission_id}/library/series",
            put(library::update_series),
        )
        .route(
            "/submissions/{submission_id}/library/chapters",
            get(library::chapters).post(library::create_chapter),
        )
        .route(
            "/submissions/{submission_id}/library/chapters/{chapter_id}",
            put(library::update_chapter).delete(library::delete_chapter),
        )
        .route(
            "/submissions/{submission_id}/library/chapters/{chapter_id}/pages",
            get(library::pages),
        )
        .route(
            "/submissions/{submission_id}/library/chapters/{chapter_id}/uploads",
            get(library::uploads).post(library::begin_upload),
        )
        .route(
            "/submissions/{submission_id}/library/uploads/{upload_id}",
            get(library::upload_status).delete(library::abort_upload),
        )
        .route(
            "/submissions/{submission_id}/library/uploads/{upload_id}/parts",
            get(library::upload_parts),
        )
        .route(
            "/submissions/{submission_id}/library/uploads/{upload_id}/inspect",
            get(library::inspect_upload),
        )
        .route(
            "/submissions/{submission_id}/library/uploads/{upload_id}/complete",
            post(library::complete_upload),
        )
        .route(
            "/library/submissions/{submission_id}/cover",
            post(library::set_cover).layer(axum::extract::DefaultBodyLimit::max(
                library::COVER_BODY_LIMIT,
            )),
        )
        .route(
            "/submissions/{submission_id}/artwork/{kind}",
            post(artwork::set_artwork).layer(axum::extract::DefaultBodyLimit::max(
                artwork::ARTWORK_BODY_LIMIT,
            )),
        )
        // Requests.
        .route("/requests", get(requests::list).post(requests::create))
        .route(
            "/requests/{request_id}/vote",
            put(requests::vote).delete(requests::unvote),
        )
        .route("/requests/{request_id}/fulfil", post(requests::fulfil))
        .route(
            "/requests/{request_id}/claim",
            put(requests::claim).delete(requests::release),
        )
        // Reviewers.
        .route("/admin/review/queue", get(reviewer::queue))
        .route(
            "/admin/review/submissions/{submission_id}",
            get(reviewer::get),
        )
        .route(
            "/admin/review/submissions/{submission_id}/preview",
            get(reviewer::preview),
        )
        .route(
            "/admin/review/submissions/{submission_id}/library/chapters/{chapter_id}/pages",
            get(library::review_pages),
        )
        .route(
            "/admin/review/submissions/{submission_id}/claim",
            post(reviewer::claim),
        )
        .route(
            "/admin/review/submissions/{submission_id}/release",
            post(reviewer::release),
        )
        .route(
            "/admin/review/submissions/{submission_id}/comments",
            post(reviewer::comment),
        )
        .route(
            "/admin/review/submissions/{submission_id}/items/{item_id}",
            put(reviewer::decide_item),
        )
        .route(
            "/admin/review/submissions/{submission_id}/request-changes",
            post(reviewer::request_changes),
        )
        .route(
            "/admin/review/submissions/{submission_id}/reject",
            post(reviewer::reject),
        )
        .route(
            "/admin/review/submissions/{submission_id}/approve",
            post(reviewer::approve),
        )
        .route(
            "/admin/review/submissions/{submission_id}/publish",
            post(reviewer::publish),
        )
        .route(
            "/admin/review/submissions/{submission_id}/publish-now",
            post(reviewer::publish_now),
        )
        .route(
            "/admin/review/submissions/{submission_id}/credits",
            get(reviewer::credits),
        )
        .route(
            "/admin/review/submissions/{submission_id}/revoke-credits",
            post(reviewer::revoke_credits),
        )
        .route(
            "/admin/review/submissions/{submission_id}/media",
            get(media::list),
        )
        .route(
            "/admin/review/submissions/{submission_id}/duplicates",
            get(duplicates::review),
        )
        .route(
            "/admin/review/submissions/{submission_id}/duplicate",
            put(duplicates::resolve),
        )
        .route(
            "/admin/review/requests/{request_id}/decline",
            post(requests::decline),
        )
        .merge(lookup::router())
}

/// OpenAPI fragment for the review endpoints.
#[derive(OpenApi)]
#[openapi(
    paths(
        member::kinds,
        member::list,
        member::create,
        member::get,
        member::update,
        member::preview,
        member::submit,
        member::withdraw,
        member::comment,
        member::quota,
        member::naming_parse,
        member::credits,
        duplicates::mine,
        duplicates::review,
        duplicates::resolve,
        media::list,
        lookup::providers,
        lookup::search,
        library::update_series,
        library::set_cover,
        artwork::set_artwork,
        library::chapters,
        library::create_chapter,
        library::update_chapter,
        library::delete_chapter,
        library::pages,
        library::uploads,
        library::begin_upload,
        library::upload_status,
        library::upload_parts,
        library::inspect_upload,
        library::abort_upload,
        library::complete_upload,
        library::review_pages,
        requests::list,
        requests::create,
        requests::vote,
        requests::unvote,
        requests::fulfil,
        requests::claim,
        requests::release,
        requests::decline,
        reviewer::queue,
        reviewer::get,
        reviewer::preview,
        reviewer::claim,
        reviewer::release,
        reviewer::comment,
        reviewer::decide_item,
        reviewer::request_changes,
        reviewer::reject,
        reviewer::approve,
        reviewer::publish,
        reviewer::publish_now,
        reviewer::credits,
        reviewer::revoke_credits,
    ),
    components(schemas(
        ErrorBody,
        artwork::ArtworkBody,
        artwork::ArtworkResult,
        member::PreviewBody,
        member::NamingParseBody,
        member::NamingParseItem,
        member::NamingParseResult,
        sr_media::naming::ParsedVideoName,
        sr_media::naming::Role,
        sr_media::naming::Media,
        sr_media::naming::ArtworkKind,
        sr_reader::naming::ParsedReaderName,
        sr_reader::naming::ReaderRole,
        requests::CreateRequestBody,
        requests::FulfilBody,
        reviewer::ClaimBody,
        reviewer::RevokeCreditsBody,
        reviewer::RevokedCredits,
        sr_review::model::Credit,
        sr_review::model::UploadAllowance,
        sr_review::model::SubmissionState,
        sr_review::model::ItemState,
        sr_review::model::Submission,
        sr_review::model::SubmissionItem,
        sr_review::model::SubmissionEvent,
        sr_review::model::SubmissionDetail,
        sr_review::model::SubmissionPage,
        sr_review::model::CreateSubmission,
        sr_review::model::UpdateSubmission,
        sr_review::model::CommentInput,
        sr_review::model::DecisionInput,
        sr_review::model::ItemDecisionInput,
        sr_review::model::KindInfo,
        sr_review::model::RequestStatus,
        sr_review::model::ContentRequest,
        sr_review::model::RequestPage,
        sr_review::model::ClaimRequestInput,
        sr_library::model::SeriesInput,
        library::CoverBody,
        sr_library::model::Series,
        sr_library::model::ChapterInput,
        sr_library::model::Chapter,
        sr_library::model::ChapterList,
        sr_library::model::PageManifest,
        sr_library::model::Upload,
        sr_library::model::UploadRequest,
        sr_review::model::DuplicateMatch,
        sr_review::model::DuplicateAction,
        sr_review::model::DuplicateResolution,
        sr_review::model::DuplicateResolutionInput,
        sr_review::model::DuplicateReport,
        lookup::LookupCandidate,
        lookup::LookupProviderInfo,
        lookup::LookupProviders,
        lookup::LookupResults,
        media::MediaItem,
        media::MediaList,
        sr_review::model::MediaRef,
        sr_library::model::UploadSlot,
        sr_library::multipart::MultipartSlot,
        sr_library::multipart::PartUrl,
        sr_library::multipart::ReceivedPart,
        sr_library::multipart::UploadParts,
        sr_library::ingest::inspect::ArchiveReport,
        sr_library::ingest::inspect::FolderGroup,
        sr_library::ingest::inspect::ComicInfoFacts,
        sr_library::model::UploadList,
    ))
)]
pub struct ReviewApiDoc;

/// The OpenAPI document fragment for the review endpoints.
pub fn openapi() -> utoipa::openapi::OpenApi {
    ReviewApiDoc::openapi()
}
