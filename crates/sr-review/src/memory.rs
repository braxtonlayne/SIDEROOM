//! An in-memory [`SubmissionRepository`] for unit tests and tools. It applies the same guards
//! as the Postgres implementation (state set, lease), so the service tests exercise the same
//! races and refusals.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::ReviewResult;
use crate::model::*;
use crate::repo::*;

#[derive(Default)]
struct Inner {
    subs: BTreeMap<i64, Submission>,
    items: Vec<SubmissionItem>,
    events: Vec<(SubmissionEvent, i64)>,
    requests: BTreeMap<i64, ContentRequest>,
    votes: BTreeSet<(i64, i64)>,
    now_ms: i64,
}

/// Test double; `now_ms` is set by the test so created/updated stamps are deterministic.
#[derive(Default)]
pub struct MemoryReviewRepo {
    inner: Mutex<Inner>,
}

impl MemoryReviewRepo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_now(&self, ms: i64) {
        self.lock().now_ms = ms;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn push_event(g: &mut Inner, sub: i64, e: &NewEvent) -> SubmissionEvent {
    let ev = SubmissionEvent {
        id: e.id,
        submission_id: sub,
        kind: e.kind.clone(),
        actor_id: e.actor_id,
        body: e.body.clone(),
        internal: e.internal,
        data: e.data.clone(),
        created_at_ms: g.now_ms,
    };
    g.events.push((ev.clone(), sub));
    ev
}

#[async_trait]
impl SubmissionRepository for MemoryReviewRepo {
    async fn create(&self, s: &NewSubmission, event: &NewEvent) -> ReviewResult<Submission> {
        let mut g = self.lock();
        let sub = Submission {
            id: s.id,
            kind: s.kind.clone(),
            submitter_id: Some(s.submitter_id),
            state: SubmissionState::Draft,
            title: s.title.clone(),
            summary: s.summary.clone(),
            ref_id: s.ref_id,
            payload: s.payload.clone(),
            flags: Vec::new(),
            claimed_by: None,
            claim_expires_at_ms: None,
            decided_by: None,
            decision_reason: None,
            submitted_at_ms: None,
            decided_at_ms: None,
            published_at_ms: None,
            created_at_ms: g.now_ms,
            updated_at_ms: g.now_ms,
        };
        g.subs.insert(s.id, sub.clone());
        push_event(&mut g, s.id, event);
        Ok(sub)
    }

    async fn get(&self, id: i64) -> ReviewResult<Option<Submission>> {
        Ok(self.lock().subs.get(&id).cloned())
    }

    async fn update_fields(
        &self,
        id: i64,
        title: Option<&str>,
        summary: Option<&str>,
        event: &NewEvent,
    ) -> ReviewResult<Option<Submission>> {
        let mut g = self.lock();
        let now = g.now_ms;
        let Some(s) = g.subs.get_mut(&id) else {
            return Ok(None);
        };
        if !matches!(
            s.state,
            SubmissionState::Draft | SubmissionState::ChangesRequested
        ) {
            return Ok(None);
        }
        if let Some(t) = title {
            s.title = t.to_owned();
        }
        if let Some(t) = summary {
            s.summary = t.to_owned();
        }
        s.updated_at_ms = now;
        let out = s.clone();
        push_event(&mut g, id, event);
        Ok(Some(out))
    }

    async fn transition(
        &self,
        id: i64,
        t: &Transition,
        event: &NewEvent,
    ) -> ReviewResult<Option<Submission>> {
        let mut g = self.lock();
        let Some(s) = g.subs.get_mut(&id) else {
            return Ok(None);
        };
        if !t.from.contains(&s.state) {
            return Ok(None);
        }
        let ok = match t.lease {
            LeaseGuard::None => true,
            LeaseGuard::Holder(u) => s.claimed_by == Some(u),
            LeaseGuard::Live(u, now) => s.claimed_by == Some(u) && s.lease_live(now),
        };
        if !ok {
            return Ok(None);
        }
        s.state = t.to;
        if t.claim == ClaimPatch::Clear {
            s.claimed_by = None;
            s.claim_expires_at_ms = None;
        }
        if t.stamp_submitted {
            s.submitted_at_ms = Some(t.now_ms);
        }
        if let Some((by, reason)) = &t.decision {
            s.decided_by = Some(*by);
            s.decision_reason = reason.clone();
            s.decided_at_ms = Some(t.now_ms);
        }
        if t.stamp_published {
            s.published_at_ms = Some(t.now_ms);
        }
        s.updated_at_ms = t.now_ms;
        let out = s.clone();
        push_event(&mut g, id, event);
        Ok(Some(out))
    }

    async fn claim(
        &self,
        id: i64,
        user: i64,
        now_ms: i64,
        until_ms: i64,
        takeover: bool,
        event: Option<&NewEvent>,
    ) -> ReviewResult<Option<Submission>> {
        let mut g = self.lock();
        let Some(s) = g.subs.get_mut(&id) else {
            return Ok(None);
        };
        if !matches!(
            s.state,
            SubmissionState::Submitted | SubmissionState::InReview
        ) {
            return Ok(None);
        }
        let free = s.claimed_by.is_none() || s.claimed_by == Some(user) || !s.lease_live(now_ms);
        if !free && !takeover {
            return Ok(None);
        }
        s.state = SubmissionState::InReview;
        s.claimed_by = Some(user);
        s.claim_expires_at_ms = Some(until_ms);
        s.updated_at_ms = now_ms;
        let out = s.clone();
        if let Some(e) = event {
            push_event(&mut g, id, e);
        }
        Ok(Some(out))
    }

    async fn add_event(&self, submission: i64, e: &NewEvent) -> ReviewResult<SubmissionEvent> {
        Ok(push_event(&mut self.lock(), submission, e))
    }

    async fn events(&self, submission: i64) -> ReviewResult<Vec<SubmissionEvent>> {
        Ok(self
            .lock()
            .events
            .iter()
            .filter(|(_, s)| *s == submission)
            .map(|(e, _)| e.clone())
            .collect())
    }

    async fn items(&self, submission: i64) -> ReviewResult<Vec<SubmissionItem>> {
        let mut v: Vec<_> = self
            .lock()
            .items
            .iter()
            .filter(|i| i.submission_id == submission)
            .cloned()
            .collect();
        v.sort_by_key(|i| (i.position, i.id));
        Ok(v)
    }

    async fn sync_items(
        &self,
        submission: i64,
        items: &[(i64, NewItem)],
        reset_rejected: bool,
    ) -> ReviewResult<Vec<SubmissionItem>> {
        {
            let mut g = self.lock();
            let keep: BTreeSet<i64> = items.iter().map(|(_, i)| i.ref_id).collect();
            g.items
                .retain(|i| i.submission_id != submission || keep.contains(&i.ref_id));
            for (id, n) in items {
                if let Some(x) = g
                    .items
                    .iter_mut()
                    .find(|i| i.submission_id == submission && i.ref_id == n.ref_id)
                {
                    x.label = n.label.clone();
                    x.position = n.position;
                    if reset_rejected && x.state == ItemState::Rejected {
                        x.state = ItemState::Pending;
                    }
                } else {
                    g.items.push(SubmissionItem {
                        id: *id,
                        submission_id: submission,
                        ref_id: n.ref_id,
                        label: n.label.clone(),
                        position: n.position,
                        state: ItemState::Pending,
                        note: None,
                    });
                }
            }
        }
        self.items(submission).await
    }

    async fn add_item(&self, submission: i64, id: i64, item: &NewItem) -> ReviewResult<()> {
        let mut g = self.lock();
        if let Some(x) = g
            .items
            .iter_mut()
            .find(|i| i.submission_id == submission && i.ref_id == item.ref_id)
        {
            x.label = item.label.clone();
            x.position = item.position;
        } else {
            g.items.push(SubmissionItem {
                id,
                submission_id: submission,
                ref_id: item.ref_id,
                label: item.label.clone(),
                position: item.position,
                state: ItemState::Pending,
                note: None,
            });
        }
        Ok(())
    }

    async fn remove_item(&self, submission: i64, ref_id: i64) -> ReviewResult<bool> {
        let mut g = self.lock();
        let before = g.items.len();
        g.items
            .retain(|i| !(i.submission_id == submission && i.ref_id == ref_id));
        Ok(g.items.len() != before)
    }

    async fn set_item_state(
        &self,
        submission: i64,
        item: i64,
        state: ItemState,
        note: Option<&str>,
    ) -> ReviewResult<bool> {
        let mut g = self.lock();
        match g
            .items
            .iter_mut()
            .find(|i| i.submission_id == submission && i.id == item)
        {
            Some(i) => {
                i.state = state;
                i.note = note.map(str::to_owned);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn move_items(
        &self,
        submission: i64,
        from: ItemState,
        to: ItemState,
    ) -> ReviewResult<()> {
        for i in self
            .lock()
            .items
            .iter_mut()
            .filter(|i| i.submission_id == submission && i.state == from)
        {
            i.state = to;
        }
        Ok(())
    }

    async fn list(
        &self,
        q: &SubmissionQuery,
        now_ms: i64,
        exclude_submitter: Option<i64>,
    ) -> ReviewResult<(Vec<Submission>, i64)> {
        let g = self.lock();
        let mut rows: Vec<Submission> = g
            .subs
            .values()
            .filter(|s| q.kinds.is_empty() || q.kinds.contains(&s.kind))
            .filter(|s| q.states.is_empty() || q.states.contains(&s.state))
            .filter(|s| q.submitter.is_none() || s.submitter_id == q.submitter)
            .filter(|s| exclude_submitter.is_none() || s.submitter_id != exclude_submitter)
            .filter(|s| {
                q.claimed_by.is_none() || (s.claimed_by == q.claimed_by && s.lease_live(now_ms))
            })
            .filter(|s| !q.unclaimed || !s.lease_live(now_ms))
            .filter(|s| {
                q.submitted_before_ms
                    .is_none_or(|t| s.submitted_at_ms.is_some_and(|x| x <= t))
            })
            .filter(|s| {
                q.title_contains
                    .as_ref()
                    .is_none_or(|t| s.title.to_lowercase().contains(&t.to_lowercase()))
            })
            .cloned()
            .collect();
        match q.sort {
            SortKey::Submitted => {
                rows.sort_by_key(|s| (s.submitted_at_ms.unwrap_or(i64::MAX), s.id))
            }
            SortKey::Updated => rows.sort_by_key(|s| (s.updated_at_ms, s.id)),
            SortKey::Title => rows.sort_by_key(|s| (s.title.to_lowercase(), s.id)),
        }
        if q.descending {
            rows.reverse();
        }
        let total = i64::try_from(rows.len()).unwrap_or(i64::MAX);
        let rows = rows
            .into_iter()
            .skip(q.offset as usize)
            .take(q.limit.max(1) as usize)
            .collect();
        Ok((rows, total))
    }

    async fn count_open(&self, user: i64) -> ReviewResult<i64> {
        let n = self
            .lock()
            .subs
            .values()
            .filter(|s| s.submitter_id == Some(user) && s.state.is_open())
            .count();
        Ok(i64::try_from(n).unwrap_or(i64::MAX))
    }

    async fn create_request(
        &self,
        id: i64,
        section: &str,
        user: i64,
        input: &RequestInput,
    ) -> ReviewResult<ContentRequest> {
        let mut g = self.lock();
        let r = ContentRequest {
            id,
            section: section.to_owned(),
            requester_id: Some(user),
            title: input.title.clone(),
            creator: input.creator.clone(),
            year: input.year,
            link: input.link.clone(),
            note: input.note.clone(),
            status: RequestStatus::Open,
            vote_count: 1,
            voted: true,
            submission_id: None,
            created_at_ms: g.now_ms,
        };
        g.requests.insert(id, r.clone());
        g.votes.insert((id, user));
        Ok(r)
    }

    async fn get_request(&self, id: i64, viewer: i64) -> ReviewResult<Option<ContentRequest>> {
        let g = self.lock();
        Ok(g.requests.get(&id).map(|r| ContentRequest {
            voted: g.votes.contains(&(id, viewer)),
            ..r.clone()
        }))
    }

    async fn list_requests(
        &self,
        q: &RequestQuery,
        viewer: i64,
    ) -> ReviewResult<(Vec<ContentRequest>, i64)> {
        let g = self.lock();
        let mut rows: Vec<ContentRequest> = g
            .requests
            .values()
            .filter(|r| r.section == q.section)
            .filter(|r| q.status.is_none_or(|s| r.status == s))
            .map(|r| ContentRequest {
                voted: g.votes.contains(&(r.id, viewer)),
                ..r.clone()
            })
            .collect();
        if q.newest {
            rows.sort_by_key(|r| std::cmp::Reverse(r.id));
        } else {
            rows.sort_by_key(|r| (std::cmp::Reverse(r.vote_count), std::cmp::Reverse(r.id)));
        }
        let total = i64::try_from(rows.len()).unwrap_or(i64::MAX);
        let rows = rows
            .into_iter()
            .skip(q.offset as usize)
            .take(q.limit.max(1) as usize)
            .collect();
        Ok((rows, total))
    }

    async fn set_vote(
        &self,
        id: i64,
        user: i64,
        vote: bool,
    ) -> ReviewResult<Option<ContentRequest>> {
        let mut g = self.lock();
        match g.requests.get(&id) {
            Some(r) if r.status == RequestStatus::Open => {}
            _ => return Ok(None),
        }
        if vote {
            g.votes.insert((id, user));
        } else {
            g.votes.remove(&(id, user));
        }
        let n = g.votes.iter().filter(|(r, _)| *r == id).count();
        let voted = g.votes.contains(&(id, user));
        let Some(r) = g.requests.get_mut(&id) else {
            return Ok(None);
        };
        r.vote_count = i32::try_from(n).unwrap_or(i32::MAX);
        Ok(Some(ContentRequest { voted, ..r.clone() }))
    }

    async fn open_request_count(&self, user: i64) -> ReviewResult<i64> {
        let n = self
            .lock()
            .requests
            .values()
            .filter(|r| r.requester_id == Some(user) && r.status == RequestStatus::Open)
            .count();
        Ok(i64::try_from(n).unwrap_or(i64::MAX))
    }

    async fn close_request(
        &self,
        id: i64,
        status: RequestStatus,
        submission: Option<i64>,
        viewer: i64,
    ) -> ReviewResult<Option<ContentRequest>> {
        let mut g = self.lock();
        let voted = g.votes.contains(&(id, viewer));
        match g.requests.get_mut(&id) {
            Some(r) if r.status == RequestStatus::Open => {
                r.status = status;
                r.submission_id = submission;
                Ok(Some(ContentRequest { voted, ..r.clone() }))
            }
            _ => Ok(None),
        }
    }

    async fn request_voters(&self, id: i64) -> ReviewResult<Vec<i64>> {
        Ok(self
            .lock()
            .votes
            .iter()
            .filter(|(r, _)| *r == id)
            .map(|(_, u)| *u)
            .collect())
    }
}
