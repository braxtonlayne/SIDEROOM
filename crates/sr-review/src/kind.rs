//! The kind registry: what differs between a Library series, Cinema titles, an app or a
//! package. The pipeline (states, leases, decisions, timeline) is shared; a kind validates its
//! payload, lists its items, previews itself for reviewers and publishes.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use sr_core::PlatformPermissions as P;

use crate::error::{ReviewError, ReviewResult};
use crate::model::{DuplicateMatch, KindInfo, MediaRef, Submission, SubmissionItem};
use crate::repo::NewItem;

/// What a kind derives from the creation payload.
#[derive(Debug, Clone, PartialEq)]
pub struct KindDraft {
    pub title: String,
    pub ref_id: Option<i64>,
    /// The payload as stored (a kind may drop or normalise fields).
    pub payload: Value,
}

#[async_trait]
pub trait SubmissionKind: Send + Sync {
    /// Stable id, `section.name`, e.g. `library.series`.
    fn id(&self) -> &'static str;
    /// Display name.
    fn label(&self) -> &'static str;
    /// The request board section this kind answers (`library`, `cinema`), when it does.
    fn section(&self) -> Option<&'static str> {
        self.id().split('.').next()
    }
    /// Platform permission that lets staff claim and decide this kind (doc 07).
    fn review_permission(&self) -> P;
    /// Platform permission that lets the submitter skip review.
    fn publish_permission(&self) -> P;
    /// Validate the payload and create what the submission works on (a draft series).
    async fn create(&self, submitter: i64, payload: &Value) -> ReviewResult<KindDraft>;
    /// The items under review, in display order.
    /// `current` are the items registered so far (chapters the submitter added); a kind
    /// that derives its items from the content ignores it.
    async fn items(&self, s: &Submission, current: &[SubmissionItem])
    -> ReviewResult<Vec<NewItem>>;
    /// Problems that block submitting; empty when the submission is ready.
    async fn validate(&self, s: &Submission, items: &[NewItem]) -> ReviewResult<Vec<String>>;
    /// The metadata a reviewer sees. Stored with each submit so a resubmit can be diffed.
    async fn preview(&self, s: &Submission) -> ReviewResult<Value>;
    /// Publish the approved items. Idempotent: a retry after a partial failure must finish.
    async fn publish(&self, s: &Submission, approved: &[SubmissionItem]) -> ReviewResult<()>;
    /// Which of the published items earn a contribution credit (D-074). Default: all of them,
    /// since a submission only registers new content. A kind that lets a submission replace
    /// existing content returns only the new items here.
    async fn credit_refs(
        &self,
        _s: &Submission,
        published: &[SubmissionItem],
    ) -> ReviewResult<Vec<i64>> {
        Ok(published.iter().map(|i| i.ref_id).collect())
    }
    /// Existing catalog entries this submission may duplicate, best match first (REV-ED-006).
    /// The default finds none.
    async fn duplicates(&self, _s: &Submission) -> ReviewResult<Vec<DuplicateMatch>> {
        Ok(Vec::new())
    }
    /// The videos under review, for the reviewer's proxy player (REV-RV-002). The default has none.
    async fn media(&self, _s: &Submission) -> ReviewResult<Vec<MediaRef>> {
        Ok(Vec::new())
    }
    /// Delete the files and draft content of a rejected or abandoned submission (the 14 day
    /// purge, REV-CORE-009). Idempotent; the default keeps everything.
    async fn purge(&self, _s: &Submission, _items: &[SubmissionItem]) -> ReviewResult<()> {
        Ok(())
    }
}

/// The kinds the server knows.
#[derive(Clone, Default)]
pub struct KindRegistry {
    kinds: BTreeMap<&'static str, Arc<dyn SubmissionKind>>,
}

impl KindRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, k: Arc<dyn SubmissionKind>) -> Self {
        self.kinds.insert(k.id(), k);
        self
    }

    pub fn get(&self, id: &str) -> ReviewResult<&Arc<dyn SubmissionKind>> {
        self.kinds
            .get(id)
            .ok_or_else(|| ReviewError::Invalid(format!("unknown submission kind {id}")))
    }

    pub fn list(&self) -> Vec<KindInfo> {
        self.kinds
            .values()
            .map(|k| KindInfo {
                id: k.id().to_owned(),
                label: k.label().to_owned(),
            })
            .collect()
    }

    /// Ids of the kinds `perms` may review.
    pub fn reviewable_by(&self, perms: P) -> Vec<String> {
        self.kinds
            .values()
            .filter(|k| perms.has(k.review_permission()))
            .map(|k| k.id().to_owned())
            .collect()
    }

    /// The kind answering a request board section.
    pub fn for_section(&self, section: &str) -> Option<&Arc<dyn SubmissionKind>> {
        self.kinds.values().find(|k| k.section() == Some(section))
    }
}
