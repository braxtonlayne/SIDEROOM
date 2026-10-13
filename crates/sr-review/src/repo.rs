//! The storage port of the review pipeline. `sr-db` implements it over Postgres,
//! [`crate::memory`] in memory for tests.
//!
//! Every mutating method is a single guarded write (the `from` states and the lease are part
//! of the `WHERE`), so two reviewers racing on one submission cannot both win; a guarded
//! write that matched nothing returns `None` and the service explains why.

use async_trait::async_trait;

use crate::error::ReviewResult;
use crate::model::*;

#[derive(Debug, Clone, PartialEq)]
pub struct NewSubmission {
    pub id: i64,
    pub kind: String,
    pub submitter_id: i64,
    pub title: String,
    pub summary: String,
    pub ref_id: Option<i64>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    pub id: i64,
    pub kind: String,
    pub actor_id: Option<i64>,
    pub body: Option<String>,
    pub internal: bool,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewItem {
    pub ref_id: i64,
    pub label: String,
    pub position: i32,
}

/// What a transition does with the reviewer lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimPatch {
    Keep,
    Clear,
}

/// Who must hold the lease for a transition to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseGuard {
    None,
    /// `claimed_by = user`, expired or not (release).
    Holder(i64),
    /// `claimed_by = user` and the lease is still running at `now_ms` (decisions).
    Live(i64, i64),
}

/// A guarded change of state.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub from: Vec<SubmissionState>,
    pub to: SubmissionState,
    pub claim: ClaimPatch,
    pub lease: LeaseGuard,
    /// Stamp `submitted_at` (submit and resubmit).
    pub stamp_submitted: bool,
    /// Record the deciding reviewer and reason, and stamp `decided_at`.
    pub decision: Option<(i64, Option<String>)>,
    /// Stamp `published_at`.
    pub stamp_published: bool,
    pub now_ms: i64,
}

#[async_trait]
pub trait SubmissionRepository: Send + Sync {
    async fn create(&self, s: &NewSubmission, event: &NewEvent) -> ReviewResult<Submission>;
    async fn get(&self, id: i64) -> ReviewResult<Option<Submission>>;
    /// Title and summary of an open (draft / changes requested) submission.
    async fn update_fields(
        &self,
        id: i64,
        title: Option<&str>,
        summary: Option<&str>,
        event: &NewEvent,
    ) -> ReviewResult<Option<Submission>>;
    /// Apply `t` and append `event`, atomically. `None` when the submission is not in one of
    /// the `from` states (or the lease guard fails).
    async fn transition(
        &self,
        id: i64,
        t: &Transition,
        event: &NewEvent,
    ) -> ReviewResult<Option<Submission>>;
    /// Take the lease for `user` until `until_ms` when the submission is submitted or in
    /// review and the lease is free, expired, the user's own, or `takeover`. Moves a
    /// submitted submission to in review. `event` is appended when given.
    async fn claim(
        &self,
        id: i64,
        user: i64,
        now_ms: i64,
        until_ms: i64,
        takeover: bool,
        event: Option<&NewEvent>,
    ) -> ReviewResult<Option<Submission>>;
    async fn add_event(&self, submission: i64, e: &NewEvent) -> ReviewResult<SubmissionEvent>;
    async fn events(&self, submission: i64) -> ReviewResult<Vec<SubmissionEvent>>;

    async fn items(&self, submission: i64) -> ReviewResult<Vec<SubmissionItem>>;
    /// Make the item list equal to `items`: new refs are added as pending, refs that are gone
    /// are removed, existing ones keep their decision. With `reset_rejected`, rejected items
    /// become pending again (a resubmit).
    async fn sync_items(
        &self,
        submission: i64,
        items: &[(i64, NewItem)],
        reset_rejected: bool,
    ) -> ReviewResult<Vec<SubmissionItem>>;
    /// Decide one item. `false` when the item is not part of the submission.
    async fn set_item_state(
        &self,
        submission: i64,
        item: i64,
        state: ItemState,
        note: Option<&str>,
    ) -> ReviewResult<bool>;
    /// Move every item in `from` to `to`.
    async fn move_items(&self, submission: i64, from: ItemState, to: ItemState)
    -> ReviewResult<()>;

    async fn list(
        &self,
        q: &SubmissionQuery,
        now_ms: i64,
        exclude_submitter: Option<i64>,
    ) -> ReviewResult<(Vec<Submission>, i64)>;
    /// Open (not final) submissions of a member.
    async fn count_open(&self, user: i64) -> ReviewResult<i64>;

    // ---- requests -----------------------------------------------------
    async fn create_request(
        &self,
        id: i64,
        section: &str,
        user: i64,
        input: &RequestInput,
    ) -> ReviewResult<ContentRequest>;
    async fn get_request(&self, id: i64, viewer: i64) -> ReviewResult<Option<ContentRequest>>;
    async fn list_requests(
        &self,
        q: &RequestQuery,
        viewer: i64,
    ) -> ReviewResult<(Vec<ContentRequest>, i64)>;
    /// Add (`true`) or remove (`false`) the vote of `user` on an open request and return the
    /// request with the new count. `None` when the request is unknown or no longer open.
    async fn set_vote(
        &self,
        id: i64,
        user: i64,
        vote: bool,
    ) -> ReviewResult<Option<ContentRequest>>;
    async fn open_request_count(&self, user: i64) -> ReviewResult<i64>;
    /// Close an open request. `None` when it is unknown or already closed.
    async fn close_request(
        &self,
        id: i64,
        status: RequestStatus,
        submission: Option<i64>,
        viewer: i64,
    ) -> ReviewResult<Option<ContentRequest>>;
    async fn request_voters(&self, id: i64) -> ReviewResult<Vec<i64>>;
}
