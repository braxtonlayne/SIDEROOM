//! The Library kinds of the review pipeline (LIB-ING-008): `library.series` (a new series
//! with its chapters) and `library.items` (new chapters for a series that is already
//! published).
//!
//! A member builds the content as an unpublished draft through [`LibraryDraftOps`]: the same
//! catalog tables and the same upload path (presigned PUT, `library.ingest` job) that staff
//! use, but scoped to the one series of the submission. The service never hands the member's
//! session publisher rights; the ops act with a *sponsored* actor only after the caller
//! (the HTTP layer) proved the submission is theirs and still editable, and every call checks
//! that the chapter belongs to the submission's series. Publishing happens in the kind's
//! `publish` hook, after a reviewer approved, with the same sponsored actor.

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::PlatformPermissions as P;
use sr_review::model::{DuplicateMatch, ItemState, Submission, SubmissionItem};
use sr_review::repo::NewItem;
use sr_review::{KindDraft, ReviewError, ReviewResult, SubmissionKind};

use crate::error::LibraryError;
use crate::model::*;
use crate::service::{Actor, LibraryService};

pub const KIND_SERIES: &str = "library.series";
pub const KIND_ITEMS: &str = "library.items";

/// Largest cover image a member may send.
pub const MAX_COVER_BYTES: usize = 2 * 1024 * 1024;

impl From<LibraryError> for ReviewError {
    fn from(e: LibraryError) -> Self {
        match e {
            LibraryError::NotFound(w) => Self::NotFound(w),
            LibraryError::MissingPermission(p) => Self::MissingPermission(p),
            LibraryError::Invalid(m) => Self::Invalid(m),
            LibraryError::Conflict(m) => Self::Conflict(m),
            LibraryError::AgeConfirmationRequired => {
                Self::Forbidden("age confirmation required".into())
            }
            LibraryError::Storage(m) | LibraryError::Search(m) => Self::Storage(m),
        }
    }
}

/// The actor the kinds work as: the submitter's id with the publisher bits, valid only inside
/// this module's checked paths.
fn sponsor(user_id: i64) -> Actor {
    Actor {
        user_id,
        permissions: P::PUBLISH_READING | P::DELETE_CONTENT,
    }
}

fn series_of(s: &Submission) -> ReviewResult<i64> {
    s.ref_id
        .ok_or_else(|| ReviewError::Storage("the submission has no series".into()))
}

fn payload_id(p: &Value, key: &str) -> Option<i64> {
    match p.get(key)? {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

fn parse_id(v: &Value, key: &str) -> ReviewResult<i64> {
    payload_id(v, key)
        .filter(|i| *i > 0)
        .ok_or_else(|| ReviewError::Invalid(format!("{key} is required")))
}

fn chapter_label(number: f64, title: Option<&str>) -> String {
    let n = if number.fract() == 0.0 {
        format!("{number:.0}")
    } else {
        number.to_string()
    };
    match title.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => format!("Chapter {n}: {t}"),
        None => format!("Chapter {n}"),
    }
}

/// The metadata the reviewer compares between submissions (no signed URLs, no counters).
fn series_facts(s: &Series) -> Value {
    json!({
        "kind": s.kind.as_str(),
        "title": s.title,
        "alt_titles": s.alt_titles,
        "authors": s.authors,
        "artists": s.artists,
        "status": s.status.as_str(),
        "year": s.year,
        "synopsis": s.synopsis,
        "genres": s.genres,
        "tags": s.tags,
        "reading_direction": s.reading_direction.as_str(),
        "age_restricted": s.age_restricted,
        "has_cover": s.cover_url.is_some(),
    })
}

fn chapter_facts(c: &Chapter) -> Value {
    json!({
        "id": c.id.to_string(),
        "edition_id": c.edition_id.to_string(),
        "number": c.number,
        "title": c.title,
        "format": c.format.as_str(),
        "page_count": c.page_count,
        "release_group": c.release_group,
        "published": c.published,
    })
}

/// What a member can do to the draft content of one submission.
#[derive(Clone)]
pub struct LibraryDraftOps {
    svc: LibraryService,
}

impl LibraryDraftOps {
    pub fn new(svc: LibraryService) -> Self {
        Self { svc }
    }

    async fn own_chapter(&self, s: &Submission, chapter: i64) -> ReviewResult<ChapterRecord> {
        let series = series_of(s)?;
        match self.svc.repo.get_chapter(chapter, None).await? {
            Some(c) if c.series_id == series => Ok(c),
            _ => Err(ReviewError::NotFound("chapter")),
        }
    }

    /// Edit the metadata of the draft series (`library.series` only; an existing series is
    /// not the member's to edit).
    pub async fn update_series(
        &self,
        s: &Submission,
        user: i64,
        mut input: SeriesInput,
    ) -> ReviewResult<Series> {
        if s.kind != KIND_SERIES {
            return Err(ReviewError::Conflict(
                "only a new series can be edited here".into(),
            ));
        }
        input.cover_key = None;
        Ok(self
            .svc
            .update_series(sponsor(user), series_of(s)?, input)
            .await?)
    }

    /// Set the cover of the draft series from an image the member uploaded (G-78): one JPEG or
    /// PNG of at most [`MAX_COVER_BYTES`], sniffed from its content and re-encoded (metadata
    /// stripped, at most 600 x 900) before it is stored. A member cannot name storage keys, so
    /// this is the only way a cover gets into a member's draft. `library.series` only.
    pub async fn set_cover(&self, s: &Submission, user: i64, image: &[u8]) -> ReviewResult<Series> {
        use sr_storage::img::{self, Fit, ImgFormat, Limits};
        if s.kind != KIND_SERIES {
            return Err(ReviewError::Conflict(
                "only a new series has a cover to set".into(),
            ));
        }
        if image.is_empty() || image.len() > MAX_COVER_BYTES {
            return Err(ReviewError::Invalid(
                "the cover must be a JPEG or PNG of at most 2 MiB".into(),
            ));
        }
        if !matches!(
            ImgFormat::sniff(image),
            Some(ImgFormat::Jpeg | ImgFormat::Png)
        ) {
            return Err(ReviewError::Invalid(
                "the cover must be a JPEG or PNG image".into(),
            ));
        }
        let limits = Limits {
            max_bytes: MAX_COVER_BYTES as u64,
            ..Limits::default()
        };
        let out = img::normalize(image, Fit::Bound { w: 600, h: 900 }, &limits)
            .map_err(|e| ReviewError::Invalid(format!("the cover could not be read: {e}")))?;
        let storage = self
            .svc
            .ingest
            .as_ref()
            .map(|i| i.storage.clone())
            .ok_or_else(|| ReviewError::Conflict("uploads are not available".into()))?;
        let series = series_of(s)?;
        let key = format!(
            "library/{series}/cover-{}.{}",
            (self.svc.ids)(),
            out.format.ext()
        );
        storage
            .put(&key, bytes::Bytes::from(out.bytes))
            .await
            .map_err(|e| ReviewError::Storage(e.to_string()))?;
        Ok(self
            .svc
            .set_series_cover(sponsor(user), series, &key)
            .await?)
    }

    /// Add a draft chapter. Volumes and source keys cannot be chosen by the member (the upload
    /// fills the source key).
    pub async fn create_chapter(
        &self,
        s: &Submission,
        user: i64,
        edition: Option<i64>,
        mut input: ChapterInput,
    ) -> ReviewResult<Chapter> {
        let series = series_of(s)?;
        let editions = self.svc.repo.list_editions(series).await?;
        let edition = match edition.or_else(|| payload_id(&s.payload, "edition_id")) {
            Some(e) => e,
            None => editions
                .iter()
                .find(|e| e.is_default)
                .or(editions.first())
                .map(|e| e.id)
                .ok_or_else(|| ReviewError::Conflict("the series has no edition".into()))?,
        };
        if !editions.iter().any(|e| e.id == edition) {
            return Err(ReviewError::NotFound("edition"));
        }
        input.volume_id = None;
        input.source_key = None;
        let ch = self
            .svc
            .create_chapter(sponsor(user), edition, input)
            .await?;
        Ok(ch)
    }

    pub async fn update_chapter(
        &self,
        s: &Submission,
        user: i64,
        chapter: i64,
        mut input: ChapterInput,
    ) -> ReviewResult<Chapter> {
        let c = self.own_chapter(s, chapter).await?;
        if c.published {
            return Err(ReviewError::Conflict(
                "a published chapter cannot be edited here".into(),
            ));
        }
        input.volume_id = None;
        input.source_key = None;
        Ok(self
            .svc
            .update_chapter(sponsor(user), chapter, input)
            .await?)
    }

    /// Delete a draft chapter of the submission.
    pub async fn delete_chapter(
        &self,
        s: &Submission,
        user: i64,
        chapter: i64,
    ) -> ReviewResult<()> {
        let c = self.own_chapter(s, chapter).await?;
        if c.published {
            return Err(ReviewError::Conflict(
                "a published chapter cannot be deleted here".into(),
            ));
        }
        Ok(self.svc.delete_chapter(sponsor(user), chapter).await?)
    }

    pub async fn begin_upload(
        &self,
        s: &Submission,
        user: i64,
        chapter: i64,
        req: UploadRequest,
    ) -> ReviewResult<UploadSlot> {
        let c = self.own_chapter(s, chapter).await?;
        if c.published {
            return Err(ReviewError::Conflict(
                "a published chapter cannot be re-uploaded here".into(),
            ));
        }
        Ok(self.svc.begin_upload(sponsor(user), chapter, req).await?)
    }

    async fn own_upload(&self, s: &Submission, user: i64, upload: i64) -> ReviewResult<Upload> {
        let u = self.svc.upload_status(sponsor(user), upload).await?;
        if Some(u.series_id) == s.ref_id {
            Ok(u)
        } else {
            Err(ReviewError::NotFound("upload"))
        }
    }

    pub async fn complete_upload(
        &self,
        s: &Submission,
        user: i64,
        upload: i64,
    ) -> ReviewResult<Upload> {
        self.own_upload(s, user, upload).await?;
        Ok(self.svc.complete_upload(sponsor(user), upload).await?)
    }

    pub async fn upload_status(
        &self,
        s: &Submission,
        user: i64,
        upload: i64,
    ) -> ReviewResult<Upload> {
        self.own_upload(s, user, upload).await
    }

    /// Resume a multipart upload: the parts that arrived and URLs for the rest.
    pub async fn upload_parts(
        &self,
        s: &Submission,
        user: i64,
        upload: i64,
    ) -> ReviewResult<crate::multipart::UploadParts> {
        self.own_upload(s, user, upload).await?;
        Ok(self.svc.upload_parts(sponsor(user), upload).await?)
    }

    /// List the archive of an upload without converting it.
    pub async fn inspect_upload(
        &self,
        s: &Submission,
        user: i64,
        upload: i64,
    ) -> ReviewResult<crate::ingest::inspect::ArchiveReport> {
        self.own_upload(s, user, upload).await?;
        Ok(self.svc.inspect_upload(sponsor(user), upload).await?)
    }

    /// Cancel an upload that was not completed.
    pub async fn abort_upload(
        &self,
        s: &Submission,
        user: i64,
        upload: i64,
    ) -> ReviewResult<Upload> {
        self.own_upload(s, user, upload).await?;
        Ok(self.svc.abort_upload(sponsor(user), upload).await?)
    }

    pub async fn chapter_uploads(
        &self,
        s: &Submission,
        user: i64,
        chapter: i64,
    ) -> ReviewResult<UploadList> {
        self.own_chapter(s, chapter).await?;
        Ok(self.svc.chapter_uploads(sponsor(user), chapter).await?)
    }

    /// The chapters of the submission's series, drafts included.
    pub async fn chapters(&self, s: &Submission, user: i64) -> ReviewResult<ChapterList> {
        Ok(self
            .svc
            .admin_chapters(sponsor(user), series_of(s)?, ChapterQuery::default())
            .await?)
    }

    /// Signed page URLs of a chapter of the submission, for the submitter or a reviewer.
    pub async fn pages(
        &self,
        s: &Submission,
        viewer: i64,
        chapter: i64,
    ) -> ReviewResult<PageManifest> {
        self.own_chapter(s, chapter).await?;
        Ok(self
            .svc
            .admin_page_manifest(sponsor(viewer), chapter)
            .await?)
    }
}

/// `library.series` and `library.items` share everything but how they start and publish.
#[derive(Clone)]
pub struct LibraryKind {
    svc: LibraryService,
    new_series: bool,
}

impl LibraryKind {
    /// `library.series`: a new series with its chapters.
    pub fn series(svc: LibraryService) -> Self {
        Self {
            svc,
            new_series: true,
        }
    }

    /// `library.items`: new chapters for a published series.
    pub fn items(svc: LibraryService) -> Self {
        Self {
            svc,
            new_series: false,
        }
    }

    async fn chapters_of(&self, series: i64) -> ReviewResult<Vec<Chapter>> {
        Ok(self
            .svc
            .admin_chapters(sponsor(0), series, ChapterQuery::default())
            .await?
            .chapters)
    }
}

#[async_trait]
impl SubmissionKind for LibraryKind {
    fn id(&self) -> &'static str {
        if self.new_series {
            KIND_SERIES
        } else {
            KIND_ITEMS
        }
    }

    fn label(&self) -> &'static str {
        if self.new_series {
            "Library series"
        } else {
            "Library chapters"
        }
    }

    fn review_permission(&self) -> P {
        P::REVIEW_READING
    }

    fn publish_permission(&self) -> P {
        P::PUBLISH_READING
    }

    async fn create(&self, submitter: i64, payload: &Value) -> ReviewResult<KindDraft> {
        let a = sponsor(submitter);
        if self.new_series {
            let mut input: SeriesInput = serde_json::from_value(payload.clone())
                .map_err(|e| ReviewError::Invalid(format!("invalid series: {e}")))?;
            // A member cannot point at storage objects; covers are set by staff.
            input.cover_key = None;
            let language = payload
                .get("language")
                .and_then(Value::as_str)
                .unwrap_or("en");
            let label = payload
                .get("edition_label")
                .and_then(Value::as_str)
                .unwrap_or("Default");
            let series = self.svc.create_series(a, input).await?;
            let edition = self
                .svc
                .create_edition(
                    a,
                    series.id,
                    EditionInput {
                        language: language.to_owned(),
                        label: label.to_owned(),
                        is_default: true,
                    },
                )
                .await;
            let edition = match edition {
                Ok(e) => e,
                Err(e) => {
                    // Do not leave a series without an edition behind.
                    let _ = self.svc.delete_series(a, series.id).await;
                    return Err(e.into());
                }
            };
            Ok(KindDraft {
                title: series.title.clone(),
                ref_id: Some(series.id),
                payload: json!({
                    "series_id": series.id.to_string(),
                    "edition_id": edition.id.to_string(),
                }),
            })
        } else {
            let series_id = parse_id(payload, "series_id")?;
            let detail = self.svc.admin_series(a, series_id).await?;
            if !detail.series.published || detail.series.merged_into.is_some() {
                return Err(ReviewError::Invalid(
                    "chapters can only be added to a published series".into(),
                ));
            }
            let edition = match payload_id(payload, "edition_id") {
                Some(e) if detail.editions.iter().any(|x| x.id == e) => e,
                Some(_) => return Err(ReviewError::Invalid("unknown edition".into())),
                None => detail
                    .editions
                    .iter()
                    .find(|e| e.is_default)
                    .or(detail.editions.first())
                    .map(|e| e.id)
                    .ok_or_else(|| ReviewError::Invalid("the series has no edition".into()))?,
            };
            Ok(KindDraft {
                title: format!("New chapters for {}", detail.series.title),
                ref_id: Some(series_id),
                payload: json!({
                    "series_id": series_id.to_string(),
                    "edition_id": edition.to_string(),
                }),
            })
        }
    }

    async fn items(
        &self,
        s: &Submission,
        current: &[SubmissionItem],
    ) -> ReviewResult<Vec<NewItem>> {
        let chapters = self.chapters_of(series_of(s)?).await?;
        let mut out = Vec::new();
        if self.new_series {
            for c in &chapters {
                out.push((c.id, chapter_label(c.number, c.title.as_deref())));
            }
        } else {
            // Only the chapters the member added through this submission, and only the ones
            // that still exist.
            for it in current {
                if let Some(c) = chapters.iter().find(|c| c.id == it.ref_id) {
                    out.push((c.id, chapter_label(c.number, c.title.as_deref())));
                }
            }
        }
        Ok(out
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
        let series = series_of(s)?;
        let detail = self.svc.admin_series(sponsor(0), series).await?;
        let mut problems = Vec::new();
        if self.new_series && detail.series.published {
            return Err(ReviewError::Conflict(
                "the series is already published".into(),
            ));
        }
        if items.is_empty() {
            problems.push(if self.new_series {
                "add at least one chapter".to_owned()
            } else {
                "add at least one new chapter".to_owned()
            });
        }
        let chapters = self.chapters_of(series).await?;
        for it in items {
            let Some(c) = chapters.iter().find(|c| c.id == it.ref_id) else {
                continue;
            };
            let label = chapter_label(c.number, c.title.as_deref());
            if c.published {
                problems.push(format!("{label} is already published"));
            }
            let ready = match c.format {
                ChapterFormat::Images => c.page_count > 0,
                _ => self
                    .svc
                    .repo
                    .get_chapter(c.id, None)
                    .await?
                    .is_some_and(|r| r.source_key.is_some()),
            };
            if !ready {
                problems.push(format!("{label} has no pages yet; upload its file first"));
            }
        }
        Ok(problems)
    }

    async fn preview(&self, s: &Submission) -> ReviewResult<Value> {
        let series = series_of(s)?;
        let detail = self.svc.admin_series(sponsor(0), series).await?;
        let chapters = self.chapters_of(series).await?;
        let items = if self.new_series {
            chapters.iter().collect::<Vec<_>>()
        } else {
            // The chapters of this submission are the unpublished ones (published ones are
            // the series' existing catalog).
            chapters.iter().filter(|c| !c.published).collect()
        };
        Ok(json!({
            "series_id": series.to_string(),
            "new_series": self.new_series,
            "series": series_facts(&detail.series),
            "editions": detail.editions.iter().map(|e| json!({
                "id": e.id.to_string(), "language": e.language, "label": e.label,
            })).collect::<Vec<_>>(),
            "existing_chapters": chapters.iter().filter(|c| c.published).count(),
            "chapters": items.into_iter().map(chapter_facts).collect::<Vec<_>>(),
        }))
    }

    async fn publish(&self, s: &Submission, approved: &[SubmissionItem]) -> ReviewResult<()> {
        let series = series_of(s)?;
        if approved.is_empty() {
            return Err(ReviewError::Conflict(
                "no approved chapters to publish".into(),
            ));
        }
        let by = sponsor(s.decided_by.or(s.submitter_id).unwrap_or(0));
        if self.new_series {
            let detail = self.svc.admin_series(by, series).await?;
            if !detail.series.published {
                self.svc.set_series_published(by, series, true).await?;
            }
        } else {
            let detail = self.svc.admin_series(by, series).await?;
            if !detail.series.published {
                return Err(ReviewError::Conflict(
                    "the series is no longer published".into(),
                ));
            }
        }
        for it in approved {
            self.svc.set_chapter_published(by, it.ref_id, true).await?;
        }
        Ok(())
    }

    /// Published series with the same or a similar title, or the same cover artwork.
    async fn duplicates(&self, s: &Submission) -> ReviewResult<Vec<DuplicateMatch>> {
        if !self.new_series {
            return Ok(Vec::new());
        }
        Ok(self.svc.find_duplicates(series_of(s)?).await?)
    }

    /// Delete what a rejected or abandoned submission left: the draft series of a new
    /// series, or the unpublished chapters of an existing one. Published content stays. The
    /// storage collector (`library.gc`) then removes the unreferenced files.
    async fn purge(&self, s: &Submission, items: &[SubmissionItem]) -> ReviewResult<()> {
        let by = sponsor(s.submitter_id.unwrap_or(0));
        if self.new_series {
            let series = series_of(s)?;
            match self.svc.admin_series(by, series).await {
                Ok(d) if !d.series.published => {
                    self.svc.delete_series(by, series).await?;
                }
                Ok(_) | Err(LibraryError::NotFound(_)) => {}
                Err(e) => return Err(e.into()),
            }
            return Ok(());
        }
        for it in items.iter().filter(|i| i.state != ItemState::Published) {
            match self.svc.delete_chapter(by, it.ref_id).await {
                Ok(()) | Err(LibraryError::NotFound(_)) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}
