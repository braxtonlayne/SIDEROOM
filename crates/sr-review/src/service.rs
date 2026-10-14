//! The review service: submitter actions (create, edit, submit, withdraw, resubmit),
//! reviewer actions (claim, release, comment, decide, publish) and the listings. Permission
//! checks, the state machine and the lease rules live here; kinds only know their content.

use std::sync::Arc;

use serde_json::{Value, json};
use sr_core::{Clock, PlatformPermissions as P};

use crate::error::{ReviewError, ReviewResult};
use crate::kind::{KindRegistry, SubmissionKind};
use crate::machine::{self, Action};
use crate::model::*;
use crate::repo::{
    ClaimPatch, LeaseGuard, NewEvent, NewItem, NewSubmission, SubmissionRepository, Transition,
};

/// Snowflake source.
pub type IdSource = Arc<dyn Fn() -> i64 + Send + Sync>;

/// Reviewer lease length (spec section 5: soft lock, 30 minutes).
pub const LEASE_MS: i64 = 30 * 60 * 1000;
/// Open submissions a member may have at once; staff are exempt (D-082).
pub const MAX_OPEN_PER_MEMBER: i64 = 5;
/// Open requests a member may have at once.
pub const MAX_OPEN_REQUESTS: i64 = 10;
const MAX_REASON: usize = 4000;

/// Delivers `SR_SUBMISSION_UPDATE` to the sessions of some accounts (implemented over the
/// event bus by `sr-api`). Delivery is best effort: the REST reads are the truth.
#[async_trait::async_trait]
pub trait ReviewEvents: Send + Sync {
    async fn emit(&self, users: Vec<i64>, payload: Value);
}

/// The acting account and its platform permissions (empty for a plain member).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    pub user_id: i64,
    pub permissions: P,
}

impl Caller {
    pub fn member(user_id: i64) -> Self {
        Self {
            user_id,
            permissions: P::empty(),
        }
    }

    pub fn is_staff(self) -> bool {
        !self.permissions.is_empty()
    }
}

#[derive(Clone)]
pub struct ReviewService {
    repo: Arc<dyn SubmissionRepository>,
    kinds: KindRegistry,
    clock: Arc<dyn Clock>,
    ids: IdSource,
    events: Option<Arc<dyn ReviewEvents>>,
}

fn reason_text(raw: Option<&str>, required: bool, what: &str) -> ReviewResult<Option<String>> {
    let t = raw.map(str::trim).filter(|t| !t.is_empty());
    match t {
        None if required => Err(ReviewError::Invalid(format!("{what} needs a reason"))),
        None => Ok(None),
        Some(t) if t.chars().count() > MAX_REASON => Err(ReviewError::Invalid(format!(
            "the reason is limited to {MAX_REASON} characters"
        ))),
        Some(t) => Ok(Some(t.to_owned())),
    }
}

impl ReviewService {
    pub fn new(
        repo: Arc<dyn SubmissionRepository>,
        kinds: KindRegistry,
        clock: Arc<dyn Clock>,
        ids: IdSource,
    ) -> Self {
        Self {
            repo,
            kinds,
            clock,
            ids,
            events: None,
        }
    }

    pub fn with_events(mut self, e: Arc<dyn ReviewEvents>) -> Self {
        self.events = Some(e);
        self
    }

    pub fn kinds(&self) -> &KindRegistry {
        &self.kinds
    }

    pub fn kind_infos(&self) -> Vec<KindInfo> {
        self.kinds.list()
    }

    fn now(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    fn event(
        &self,
        kind: &str,
        actor: i64,
        body: Option<String>,
        internal: bool,
        data: Value,
    ) -> NewEvent {
        NewEvent {
            id: (self.ids)(),
            kind: kind.to_owned(),
            actor_id: Some(actor),
            body,
            internal,
            data,
        }
    }

    async fn load(&self, id: i64) -> ReviewResult<Submission> {
        self.repo
            .get(id)
            .await?
            .ok_or(ReviewError::NotFound("submission"))
    }

    /// The submission when `user` is its submitter; anyone else gets "unknown".
    async fn load_own(&self, user: i64, id: i64) -> ReviewResult<Submission> {
        let s = self.load(id).await?;
        if s.submitter_id == Some(user) {
            Ok(s)
        } else {
            Err(ReviewError::NotFound("submission"))
        }
    }

    /// The submission and its kind when `c` may review that kind.
    async fn load_for_review(
        &self,
        c: Caller,
        id: i64,
    ) -> ReviewResult<(Submission, Arc<dyn SubmissionKind>)> {
        let s = self.load(id).await?;
        let kind = self.kinds.get(&s.kind)?.clone();
        need(c, kind.review_permission())?;
        Ok((s, kind))
    }

    /// A reviewer never decides their own submission.
    fn not_own(c: Caller, s: &Submission) -> ReviewResult<()> {
        if s.submitter_id == Some(c.user_id) {
            Err(ReviewError::Forbidden(
                "you cannot review your own submission".into(),
            ))
        } else {
            Ok(())
        }
    }

    async fn notify(&self, s: &Submission, kind: &str, extra: Value) {
        let (Some(ev), Some(user)) = (&self.events, s.submitter_id) else {
            return;
        };
        let mut p = json!({
            "type": kind,
            "submission_id": s.id.to_string(),
            "kind": s.kind,
            "state": s.state.as_str(),
            "title": s.title,
            "at_ms": self.now(),
        });
        if let (Some(m), Value::Object(x)) = (p.as_object_mut(), extra) {
            m.extend(x);
        }
        ev.emit(vec![user], p).await;
    }

    async fn detail(
        &self,
        s: Submission,
        include_internal: bool,
    ) -> ReviewResult<SubmissionDetail> {
        let mut events = self.repo.events(s.id).await?;
        if !include_internal {
            events.retain(|e| !e.internal);
        }
        let kind_label = self
            .kinds
            .get(&s.kind)
            .map(|k| k.label().to_owned())
            .unwrap_or_else(|_| s.kind.clone());
        Ok(SubmissionDetail {
            items: self.repo.items(s.id).await?,
            events,
            kind_label,
            submission: s,
        })
    }

    /// Why a guarded write matched nothing: the state moved, or the lease is not the caller's.
    async fn why_not(&self, id: i64, a: Action) -> ReviewError {
        match self.repo.get(id).await {
            Ok(Some(s)) => match machine::next(s.state, a) {
                Err(e) => e,
                Ok(_) => ReviewError::Conflict(
                    "you do not hold the review claim on this submission".into(),
                ),
            },
            Ok(None) => ReviewError::NotFound("submission"),
            Err(e) => e,
        }
    }

    // ---- submitter ----------------------------------------------------

    /// Start a submission. `c.permissions` only matters for the rate limit (staff exempt).
    pub async fn create(
        &self,
        c: Caller,
        input: CreateSubmission,
    ) -> ReviewResult<SubmissionDetail> {
        let kind = self.kinds.get(&input.kind)?.clone();
        if !c.is_staff() && self.repo.count_open(c.user_id).await? >= MAX_OPEN_PER_MEMBER {
            return Err(ReviewError::Conflict(format!(
                "you can have {MAX_OPEN_PER_MEMBER} open submissions at a time; finish or withdraw one first"
            )));
        }
        let summary = input.summary.trim().to_owned();
        if summary.chars().count() > 4000 {
            return Err(ReviewError::Invalid(
                "the summary is limited to 4000 characters".into(),
            ));
        }
        let draft = kind.create(c.user_id, &input.payload).await?;
        let title = input
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map_or(draft.title.clone(), str::to_owned);
        if title.is_empty() || title.chars().count() > 200 {
            return Err(ReviewError::Invalid(
                "the title must be 1 to 200 characters".into(),
            ));
        }
        let new = NewSubmission {
            id: (self.ids)(),
            kind: input.kind.clone(),
            submitter_id: c.user_id,
            title,
            summary,
            ref_id: draft.ref_id,
            payload: draft.payload,
        };
        let ev = self.event("created", c.user_id, None, false, json!({}));
        let s = self.repo.create(&new, &ev).await?;
        self.detail(s, false).await
    }

    pub async fn update(
        &self,
        user: i64,
        id: i64,
        patch: UpdateSubmission,
    ) -> ReviewResult<SubmissionDetail> {
        let s = self.load_own(user, id).await?;
        if !machine::editable(s.state) {
            return Err(ReviewError::Conflict(format!(
                "a {} submission cannot be edited",
                s.state.as_str().replace('_', " ")
            )));
        }
        let title = match patch.title.as_deref().map(str::trim) {
            Some(t) if t.is_empty() || t.chars().count() > 200 => {
                return Err(ReviewError::Invalid(
                    "the title must be 1 to 200 characters".into(),
                ));
            }
            other => other,
        };
        let summary = patch.summary.as_deref().map(str::trim);
        if summary.is_some_and(|t| t.chars().count() > 4000) {
            return Err(ReviewError::Invalid(
                "the summary is limited to 4000 characters".into(),
            ));
        }
        let ev = self.event("edited", user, None, false, json!({}));
        let s = self
            .repo
            .update_fields(id, title, summary, &ev)
            .await?
            .ok_or_else(|| {
                ReviewError::Conflict("the submission can no longer be edited".into())
            })?;
        self.detail(s, false).await
    }

    /// Items and blocking problems of a submission, from its kind.
    async fn check_ready(
        &self,
        s: &Submission,
        kind: &dyn SubmissionKind,
    ) -> ReviewResult<Vec<NewItem>> {
        let current = self.repo.items(s.id).await?;
        let items = kind.items(s, &current).await?;
        let problems = kind.validate(s, &items).await?;
        if problems.is_empty() {
            Ok(items)
        } else {
            Err(ReviewError::NotReady(problems))
        }
    }

    async fn sync_items(
        &self,
        id: i64,
        items: Vec<NewItem>,
        reset_rejected: bool,
    ) -> ReviewResult<()> {
        let rows: Vec<(i64, NewItem)> = items.into_iter().map(|i| ((self.ids)(), i)).collect();
        self.repo.sync_items(id, &rows, reset_rejected).await?;
        Ok(())
    }

    /// Submit a draft, or resubmit one with requested changes. The kind's checks run first.
    pub async fn submit(&self, user: i64, id: i64) -> ReviewResult<SubmissionDetail> {
        let s = self.load_own(user, id).await?;
        let kind = self.kinds.get(&s.kind)?.clone();
        let action = if s.state == SubmissionState::ChangesRequested {
            Action::Resubmit
        } else {
            Action::Submit
        };
        machine::next(s.state, action)?;
        let items = self.check_ready(&s, kind.as_ref()).await?;
        self.sync_items(id, items, action == Action::Resubmit)
            .await?;
        let snapshot = kind.preview(&s).await?;
        let ev = self.event(
            if action == Action::Resubmit {
                "resubmitted"
            } else {
                "submitted"
            },
            user,
            None,
            false,
            json!({ "snapshot": snapshot }),
        );
        let t = Transition {
            from: machine::sources(action).to_vec(),
            to: machine::next(s.state, action)?,
            claim: ClaimPatch::Clear,
            lease: LeaseGuard::None,
            stamp_submitted: true,
            decision: None,
            stamp_published: false,
            now_ms: self.now(),
        };
        let s = match self.repo.transition(id, &t, &ev).await? {
            Some(s) => s,
            None => return Err(self.why_not(id, action).await),
        };
        self.detail(s, false).await
    }

    /// Skip review: a submitter holding the kind's publish permission publishes at once.
    pub async fn publish_now(&self, c: Caller, id: i64) -> ReviewResult<SubmissionDetail> {
        let s = self.load_own(c.user_id, id).await?;
        let kind = self.kinds.get(&s.kind)?.clone();
        need(c, kind.publish_permission())?;
        machine::next(s.state, Action::PublishNow)?;
        let items = self.check_ready(&s, kind.as_ref()).await?;
        self.sync_items(id, items, true).await?;
        self.repo
            .move_items(id, ItemState::Pending, ItemState::Approved)
            .await?;
        self.repo
            .move_items(id, ItemState::Rejected, ItemState::Approved)
            .await?;
        let approved = self.approved_items(id).await?;
        kind.publish(&s, &approved).await?;
        self.repo
            .move_items(id, ItemState::Approved, ItemState::Published)
            .await?;
        let ev = self.event(
            "published",
            c.user_id,
            None,
            false,
            json!({ "direct": true }),
        );
        let t = Transition {
            from: machine::sources(Action::PublishNow).to_vec(),
            to: SubmissionState::Published,
            claim: ClaimPatch::Clear,
            lease: LeaseGuard::None,
            stamp_submitted: true,
            decision: Some((c.user_id, None)),
            stamp_published: true,
            now_ms: self.now(),
        };
        let s = match self.repo.transition(id, &t, &ev).await? {
            Some(s) => s,
            None => return Err(self.why_not(id, Action::PublishNow).await),
        };
        self.detail(s, false).await
    }

    pub async fn withdraw(&self, user: i64, id: i64) -> ReviewResult<SubmissionDetail> {
        let s = self.load_own(user, id).await?;
        let to = machine::next(s.state, Action::Withdraw)?;
        let ev = self.event("withdrawn", user, None, false, json!({}));
        let t = Transition {
            from: machine::sources(Action::Withdraw).to_vec(),
            to,
            claim: ClaimPatch::Clear,
            lease: LeaseGuard::None,
            stamp_submitted: false,
            decision: None,
            stamp_published: false,
            now_ms: self.now(),
        };
        let s = match self.repo.transition(id, &t, &ev).await? {
            Some(s) => s,
            None => return Err(self.why_not(id, Action::Withdraw).await),
        };
        self.detail(s, false).await
    }

    /// The submission when it is the caller's and still with them (draft or changes
    /// requested). Kind specific edit routes call this before touching the content.
    pub async fn editable_mine(&self, user: i64, id: i64) -> ReviewResult<Submission> {
        let s = self.load_own(user, id).await?;
        if machine::editable(s.state) {
            Ok(s)
        } else {
            Err(ReviewError::Conflict(format!(
                "a {} submission cannot be edited",
                s.state.as_str().replace('_', " ")
            )))
        }
    }

    /// The caller's own submission, in any state (for kind specific reads).
    pub async fn own(&self, user: i64, id: i64) -> ReviewResult<Submission> {
        self.load_own(user, id).await
    }

    /// The submission when the caller may review its kind (for kind specific reads).
    pub async fn reviewable(&self, c: Caller, id: i64) -> ReviewResult<Submission> {
        Ok(self.load_for_review(c, id).await?.0)
    }

    /// Register an item the submitter added (a chapter). Only while the submission is editable.
    pub async fn register_item(&self, user: i64, id: i64, item: NewItem) -> ReviewResult<()> {
        self.editable_mine(user, id).await?;
        self.repo.add_item(id, (self.ids)(), &item).await
    }

    pub async fn unregister_item(&self, user: i64, id: i64, ref_id: i64) -> ReviewResult<()> {
        self.editable_mine(user, id).await?;
        self.repo.remove_item(id, ref_id).await?;
        Ok(())
    }

    /// The submitter's view: their own submission without internal notes.
    pub async fn get_mine(&self, user: i64, id: i64) -> ReviewResult<SubmissionDetail> {
        let s = self.load_own(user, id).await?;
        self.detail(s, false).await
    }

    pub async fn preview_mine(&self, user: i64, id: i64) -> ReviewResult<Value> {
        let s = self.load_own(user, id).await?;
        self.kinds.get(&s.kind)?.preview(&s).await
    }

    pub async fn list_mine(
        &self,
        user: i64,
        mut q: SubmissionQuery,
    ) -> ReviewResult<SubmissionPage> {
        q.submitter = Some(user);
        self.page(q, None).await
    }

    async fn page(&self, q: SubmissionQuery, exclude: Option<i64>) -> ReviewResult<SubmissionPage> {
        let (items, total) = self.repo.list(&q, self.now(), exclude).await?;
        Ok(SubmissionPage {
            items,
            total,
            limit: q.limit,
            offset: q.offset,
        })
    }

    /// A note on the timeline. The submitter's notes are public; a reviewer may keep theirs
    /// internal. Final submissions are closed for notes.
    pub async fn comment(
        &self,
        c: Caller,
        id: i64,
        input: CommentInput,
    ) -> ReviewResult<SubmissionDetail> {
        let body = input.body.trim();
        if body.is_empty() || body.chars().count() > MAX_REASON {
            return Err(ReviewError::Invalid(format!(
                "a note must be 1 to {MAX_REASON} characters"
            )));
        }
        let s = self.load(id).await?;
        let own = s.submitter_id == Some(c.user_id);
        let reviewer = if own {
            false
        } else {
            let kind = self.kinds.get(&s.kind)?;
            need(c, kind.review_permission())?;
            true
        };
        if s.state.is_final() {
            return Err(ReviewError::Conflict(format!(
                "a {} submission is closed for notes",
                s.state.as_str()
            )));
        }
        let internal = reviewer && input.internal;
        let ev = self.event(
            "comment",
            c.user_id,
            Some(body.to_owned()),
            internal,
            json!({}),
        );
        self.repo.add_event(id, &ev).await?;
        if reviewer && !internal {
            self.notify(&s, "comment", json!({ "reason": body })).await;
        }
        self.detail(s, reviewer).await
    }

    // ---- reviewer -----------------------------------------------------

    pub async fn get_for_review(&self, c: Caller, id: i64) -> ReviewResult<SubmissionDetail> {
        let (s, _) = self.load_for_review(c, id).await?;
        self.detail(s, true).await
    }

    pub async fn preview_for_review(&self, c: Caller, id: i64) -> ReviewResult<Value> {
        let (s, kind) = self.load_for_review(c, id).await?;
        kind.preview(&s).await
    }

    /// The review queue: kinds the caller may review, submitted and in review by default,
    /// never the caller's own submissions.
    pub async fn queue(&self, c: Caller, mut q: SubmissionQuery) -> ReviewResult<SubmissionPage> {
        let allowed = self.kinds.reviewable_by(c.permissions);
        if q.kinds.is_empty() {
            q.kinds = allowed;
        } else {
            q.kinds.retain(|k| allowed.contains(k));
        }
        if q.kinds.is_empty() {
            return Ok(SubmissionPage {
                items: Vec::new(),
                total: 0,
                limit: q.limit,
                offset: q.offset,
            });
        }
        if q.states.is_empty() {
            q.states = vec![SubmissionState::Submitted, SubmissionState::InReview];
        }
        self.page(q, Some(c.user_id)).await
    }

    /// Take the review lease (30 minutes). Taking a lease somebody else still holds needs the
    /// kind's publish permission (`takeover`, curators reassigning work).
    pub async fn claim(
        &self,
        c: Caller,
        id: i64,
        takeover: bool,
    ) -> ReviewResult<SubmissionDetail> {
        let (s, kind) = self.load_for_review(c, id).await?;
        Self::not_own(c, &s)?;
        machine::next(s.state, Action::Claim)?;
        if takeover {
            need(c, kind.publish_permission())?;
        }
        let now = self.now();
        let renewal = s.claimed_by == Some(c.user_id) && s.lease_live(now);
        let ev = (!renewal).then(|| {
            let from = s.claimed_by.filter(|_| s.lease_live(now));
            self.event(
                "claimed",
                c.user_id,
                None,
                false,
                json!({ "took_over_from": from.map(|u| u.to_string()) }),
            )
        });
        let updated = self
            .repo
            .claim(id, c.user_id, now, now + LEASE_MS, takeover, ev.as_ref())
            .await?;
        let Some(updated) = updated else {
            let cur = self.load(id).await?;
            machine::next(cur.state, Action::Claim)?;
            return Err(ReviewError::Conflict(
                "another reviewer holds the claim on this submission".into(),
            ));
        };
        if !renewal {
            self.notify(&updated, "claimed", json!({})).await;
        }
        self.detail(updated, true).await
    }

    /// Give the lease back; the submission returns to the queue.
    pub async fn release(&self, c: Caller, id: i64) -> ReviewResult<SubmissionDetail> {
        let (s, _) = self.load_for_review(c, id).await?;
        let to = machine::next(s.state, Action::Release)?;
        let ev = self.event("released", c.user_id, None, false, json!({}));
        let t = Transition {
            from: machine::sources(Action::Release).to_vec(),
            to,
            claim: ClaimPatch::Clear,
            lease: LeaseGuard::Holder(c.user_id),
            stamp_submitted: false,
            decision: None,
            stamp_published: false,
            now_ms: self.now(),
        };
        match self.repo.transition(id, &t, &ev).await? {
            Some(s) => self.detail(s, true).await,
            None => Err(self.why_not(id, Action::Release).await),
        }
    }

    async fn decide(
        &self,
        c: Caller,
        id: i64,
        action: Action,
        event_kind: &str,
        reason: Option<String>,
    ) -> ReviewResult<Submission> {
        let (s, _) = self.load_for_review(c, id).await?;
        Self::not_own(c, &s)?;
        let to = machine::next(s.state, action)?;
        let ev = self.event(event_kind, c.user_id, reason.clone(), false, json!({}));
        let t = Transition {
            from: machine::sources(action).to_vec(),
            to,
            claim: ClaimPatch::Clear,
            lease: LeaseGuard::Live(c.user_id, self.now()),
            stamp_submitted: false,
            decision: Some((c.user_id, reason.clone())),
            stamp_published: false,
            now_ms: self.now(),
        };
        match self.repo.transition(id, &t, &ev).await? {
            Some(s) => {
                self.notify(&s, event_kind, json!({ "reason": reason }))
                    .await;
                Ok(s)
            }
            None => Err(self.why_not(id, action).await),
        }
    }

    /// Send the submission back to the submitter with a note they can see.
    pub async fn request_changes(
        &self,
        c: Caller,
        id: i64,
        input: DecisionInput,
    ) -> ReviewResult<SubmissionDetail> {
        let reason = reason_text(input.reason.as_deref(), true, "requesting changes")?;
        let s = self
            .decide(c, id, Action::RequestChanges, "changes_requested", reason)
            .await?;
        self.detail(s, true).await
    }

    pub async fn reject(
        &self,
        c: Caller,
        id: i64,
        input: DecisionInput,
    ) -> ReviewResult<SubmissionDetail> {
        let reason = reason_text(input.reason.as_deref(), true, "rejecting")?;
        let s = self
            .decide(c, id, Action::Reject, "rejected", reason)
            .await?;
        self.repo
            .move_items(id, ItemState::Pending, ItemState::Rejected)
            .await?;
        self.detail(s, true).await
    }

    /// Approve: pending items become approved (rejected ones stay out), then the kind
    /// publishes. A failing publish leaves the submission approved with a `publish_failed`
    /// note on the timeline; `publish` retries it.
    pub async fn approve(
        &self,
        c: Caller,
        id: i64,
        input: DecisionInput,
    ) -> ReviewResult<SubmissionDetail> {
        let reason = reason_text(input.reason.as_deref(), false, "approving")?;
        let (s0, _) = self.load_for_review(c, id).await?;
        if s0.state == SubmissionState::InReview {
            let items = self.repo.items(id).await?;
            let usable = items
                .iter()
                .any(|i| matches!(i.state, ItemState::Pending | ItemState::Approved));
            if !items.is_empty() && !usable {
                return Err(ReviewError::Conflict(
                    "every item is rejected; reject the submission instead".into(),
                ));
            }
        }
        let s = self
            .decide(c, id, Action::Approve, "approved", reason)
            .await?;
        self.repo
            .move_items(id, ItemState::Pending, ItemState::Approved)
            .await?;
        if let Err(e) = self.publish_approved(c, s).await {
            tracing::warn!(submission = id, error = %e, "publishing an approved submission failed");
        }
        let s = self.load(id).await?;
        self.detail(s, true).await
    }

    /// Publish an approved submission (the retry after a failed publish).
    pub async fn publish(&self, c: Caller, id: i64) -> ReviewResult<SubmissionDetail> {
        let (s, _) = self.load_for_review(c, id).await?;
        machine::next(s.state, Action::Publish)?;
        self.publish_approved(c, s).await?;
        let s = self.load(id).await?;
        self.detail(s, true).await
    }

    async fn approved_items(&self, id: i64) -> ReviewResult<Vec<SubmissionItem>> {
        Ok(self
            .repo
            .items(id)
            .await?
            .into_iter()
            .filter(|i| i.state == ItemState::Approved)
            .collect())
    }

    async fn publish_approved(&self, c: Caller, s: Submission) -> ReviewResult<()> {
        let kind = self.kinds.get(&s.kind)?.clone();
        let approved = self.approved_items(s.id).await?;
        if let Err(e) = kind.publish(&s, &approved).await {
            let msg = e.to_string();
            let ev = self.event("publish_failed", c.user_id, Some(msg), true, json!({}));
            self.repo.add_event(s.id, &ev).await?;
            return Err(e);
        }
        self.repo
            .move_items(s.id, ItemState::Approved, ItemState::Published)
            .await?;
        let ev = self.event("published", c.user_id, None, false, json!({}));
        let t = Transition {
            from: machine::sources(Action::Publish).to_vec(),
            to: SubmissionState::Published,
            claim: ClaimPatch::Keep,
            lease: LeaseGuard::None,
            stamp_submitted: false,
            decision: None,
            stamp_published: true,
            now_ms: self.now(),
        };
        match self.repo.transition(s.id, &t, &ev).await? {
            Some(done) => {
                self.notify(&done, "published", json!({})).await;
                Ok(())
            }
            None => Err(self.why_not(s.id, Action::Publish).await),
        }
    }

    /// Approve, reject or reset one item while holding the lease.
    pub async fn decide_item(
        &self,
        c: Caller,
        id: i64,
        item: i64,
        input: ItemDecisionInput,
    ) -> ReviewResult<SubmissionDetail> {
        let (s, _) = self.load_for_review(c, id).await?;
        Self::not_own(c, &s)?;
        if s.state != SubmissionState::InReview {
            return Err(ReviewError::Conflict(
                "claim the submission before deciding on items".into(),
            ));
        }
        if s.claimed_by != Some(c.user_id) || !s.lease_live(self.now()) {
            return Err(ReviewError::Conflict(
                "you do not hold the review claim on this submission".into(),
            ));
        }
        if input.state == ItemState::Published {
            return Err(ReviewError::Invalid(
                "an item is published by approving the submission".into(),
            ));
        }
        let note = reason_text(
            input.note.as_deref(),
            input.state == ItemState::Rejected,
            "rejecting an item",
        )?;
        if !self
            .repo
            .set_item_state(id, item, input.state, note.as_deref())
            .await?
        {
            return Err(ReviewError::NotFound("item"));
        }
        let ev = self.event(
            "item_decision",
            c.user_id,
            note,
            false,
            json!({ "item_id": item.to_string(), "state": input.state.as_str() }),
        );
        self.repo.add_event(id, &ev).await?;
        self.detail(s, true).await
    }

    // ---- requests -----------------------------------------------------

    fn section_ok(&self, section: &str) -> ReviewResult<()> {
        match self.kinds.for_section(section) {
            Some(_) => Ok(()),
            None => Err(ReviewError::Invalid(format!("unknown section {section}"))),
        }
    }

    pub async fn create_request(
        &self,
        user: i64,
        section: &str,
        mut input: RequestInput,
    ) -> ReviewResult<ContentRequest> {
        self.section_ok(section)?;
        input.title = input.title.trim().to_owned();
        input.note = input.note.trim().to_owned();
        input.creator = input
            .creator
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty());
        input.link = input
            .link
            .map(|l| l.trim().to_owned())
            .filter(|l| !l.is_empty());
        if input.title.is_empty() || input.title.chars().count() > 200 {
            return Err(ReviewError::Invalid(
                "the title must be 1 to 200 characters".into(),
            ));
        }
        if input.note.chars().count() > 2000 {
            return Err(ReviewError::Invalid(
                "the note is limited to 2000 characters".into(),
            ));
        }
        if input
            .creator
            .as_deref()
            .is_some_and(|c| c.chars().count() > 200)
        {
            return Err(ReviewError::Invalid(
                "the creator is limited to 200 characters".into(),
            ));
        }
        if input.year.is_some_and(|y| !(1000..=3000).contains(&y)) {
            return Err(ReviewError::Invalid("the year is out of range".into()));
        }
        if let Some(l) = &input.link
            && (l.chars().count() > 500 || !(l.starts_with("https://") || l.starts_with("http://")))
        {
            return Err(ReviewError::Invalid(
                "the link must be an http(s) address of at most 500 characters".into(),
            ));
        }
        if self.repo.open_request_count(user).await? >= MAX_OPEN_REQUESTS {
            return Err(ReviewError::Conflict(format!(
                "you can have {MAX_OPEN_REQUESTS} open requests at a time"
            )));
        }
        self.repo
            .create_request((self.ids)(), section, user, &input)
            .await
    }

    pub async fn list_requests(
        &self,
        viewer: i64,
        mut q: RequestQuery,
    ) -> ReviewResult<RequestPage> {
        self.section_ok(&q.section)?;
        q.limit = q.limit.clamp(1, 100);
        let (items, total) = self.repo.list_requests(&q, viewer).await?;
        Ok(RequestPage {
            items,
            total,
            limit: q.limit,
            offset: q.offset,
        })
    }

    pub async fn vote_request(
        &self,
        user: i64,
        id: i64,
        vote: bool,
    ) -> ReviewResult<ContentRequest> {
        self.repo
            .set_vote(id, user, vote)
            .await?
            .ok_or_else(|| ReviewError::Conflict("the request is unknown or closed".into()))
    }

    /// Link a published submission to an open request and tell everybody who voted. Allowed
    /// to the submitter of that submission and to reviewers of the section.
    pub async fn fulfil_request(
        &self,
        c: Caller,
        id: i64,
        submission: i64,
    ) -> ReviewResult<ContentRequest> {
        let req = self
            .repo
            .get_request(id, c.user_id)
            .await?
            .ok_or(ReviewError::NotFound("request"))?;
        let s = self.load(submission).await?;
        let kind = self.kinds.get(&s.kind)?.clone();
        if kind.section() != Some(req.section.as_str()) {
            return Err(ReviewError::Invalid(
                "the submission belongs to another section".into(),
            ));
        }
        if s.submitter_id != Some(c.user_id) {
            need(c, kind.review_permission())?;
        }
        if s.state != SubmissionState::Published {
            return Err(ReviewError::Conflict(
                "only a published submission can fulfil a request".into(),
            ));
        }
        let done = self
            .repo
            .close_request(id, RequestStatus::Fulfilled, Some(submission), c.user_id)
            .await?
            .ok_or_else(|| ReviewError::Conflict("the request is already closed".into()))?;
        if let Some(ev) = &self.events {
            let voters = self.repo.request_voters(id).await?;
            ev.emit(
                voters,
                json!({
                    "type": "request_fulfilled",
                    "request_id": id.to_string(),
                    "submission_id": submission.to_string(),
                    "title": done.title,
                    "section": done.section,
                    "at_ms": self.now(),
                }),
            )
            .await;
        }
        Ok(done)
    }

    /// Close a request without fulfilling it (reviewers of the section).
    pub async fn decline_request(&self, c: Caller, id: i64) -> ReviewResult<ContentRequest> {
        let req = self
            .repo
            .get_request(id, c.user_id)
            .await?
            .ok_or(ReviewError::NotFound("request"))?;
        let kind = self
            .kinds
            .for_section(&req.section)
            .ok_or(ReviewError::NotFound("request"))?;
        need(c, kind.review_permission())?;
        self.repo
            .close_request(id, RequestStatus::Declined, None, c.user_id)
            .await?
            .ok_or_else(|| ReviewError::Conflict("the request is already closed".into()))
    }
}

fn need(c: Caller, perm: P) -> ReviewResult<()> {
    if c.permissions.has(perm) {
        Ok(())
    } else {
        Err(ReviewError::MissingPermission(perm_name(perm)))
    }
}

fn perm_name(p: P) -> &'static str {
    [
        (P::REVIEW_MEDIA, "REVIEW_MEDIA"),
        (P::REVIEW_READING, "REVIEW_READING"),
        (P::PUBLISH_MEDIA, "PUBLISH_MEDIA"),
        (P::PUBLISH_READING, "PUBLISH_READING"),
        (P::REVIEW_APPS, "REVIEW_APPS"),
        (P::REVIEW_PACKAGES, "REVIEW_PACKAGES"),
    ]
    .into_iter()
    .find(|(b, _)| *b == p)
    .map_or("STAFF_BASIC", |(_, n)| n)
}
