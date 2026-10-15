//! Service tests over the in-memory repository: the whole life of a submission, every refusal.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI64, Ordering},
};

use async_trait::async_trait;
use serde_json::{Value, json};
use sr_core::{Clock, PlatformPermissions as P};

use crate::error::{ReviewError, ReviewResult};
use crate::kind::{KindDraft, KindRegistry, SubmissionKind};
use crate::memory::MemoryReviewRepo;
use crate::model::*;
use crate::repo::NewItem;
use crate::service::{Caller, LEASE_MS, MAX_OPEN_PER_MEMBER, ReviewEvents, ReviewService};

struct TestClock(AtomicI64);

impl Clock for TestClock {
    fn now_unix_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct FakeKind {
    items: Mutex<Vec<NewItem>>,
    fail_publish: AtomicBool,
    published: Mutex<Vec<i64>>,
    replacements: Mutex<Vec<i64>>,
    purged: Mutex<Vec<i64>>,
}

#[async_trait]
impl SubmissionKind for FakeKind {
    fn id(&self) -> &'static str {
        "library.series"
    }
    fn label(&self) -> &'static str {
        "Library series"
    }
    fn review_permission(&self) -> P {
        P::REVIEW_READING
    }
    fn publish_permission(&self) -> P {
        P::PUBLISH_READING
    }
    async fn create(&self, _: i64, payload: &Value) -> ReviewResult<KindDraft> {
        let title = payload["title"].as_str().unwrap_or("").to_owned();
        if title.is_empty() {
            return Err(ReviewError::Invalid("title is required".into()));
        }
        Ok(KindDraft {
            title,
            ref_id: Some(900),
            payload: payload.clone(),
        })
    }
    async fn items(&self, _: &Submission, _: &[SubmissionItem]) -> ReviewResult<Vec<NewItem>> {
        Ok(self.items.lock().unwrap().clone())
    }
    async fn validate(&self, _: &Submission, items: &[NewItem]) -> ReviewResult<Vec<String>> {
        Ok(if items.is_empty() {
            vec!["add at least one chapter".into()]
        } else {
            vec![]
        })
    }
    async fn preview(&self, s: &Submission) -> ReviewResult<Value> {
        Ok(json!({ "title": s.title }))
    }
    async fn publish(&self, _: &Submission, approved: &[SubmissionItem]) -> ReviewResult<()> {
        if self.fail_publish.load(Ordering::SeqCst) {
            return Err(ReviewError::Conflict("a chapter has no pages".into()));
        }
        self.published
            .lock()
            .unwrap()
            .extend(approved.iter().map(|i| i.ref_id));
        Ok(())
    }
    async fn credit_refs(
        &self,
        _: &Submission,
        published: &[SubmissionItem],
    ) -> ReviewResult<Vec<i64>> {
        let rep = self.replacements.lock().unwrap();
        Ok(published
            .iter()
            .map(|i| i.ref_id)
            .filter(|r| !rep.contains(r))
            .collect())
    }
    async fn purge(&self, s: &Submission, _: &[SubmissionItem]) -> ReviewResult<()> {
        self.purged.lock().unwrap().push(s.id);
        Ok(())
    }
}

#[derive(Default)]
struct Sink(Mutex<Vec<(Vec<i64>, Value)>>);

#[async_trait]
impl ReviewEvents for Sink {
    async fn emit(&self, users: Vec<i64>, payload: Value) {
        self.0.lock().unwrap().push((users, payload));
    }
}

struct World {
    svc: ReviewService,
    repo: Arc<MemoryReviewRepo>,
    kind: Arc<FakeKind>,
    clock: Arc<TestClock>,
    sink: Arc<Sink>,
}

const MEMBER: i64 = 10;
const OTHER: i64 = 11;
const REVIEWER: i64 = 20;
const REVIEWER2: i64 = 21;
const CURATOR: i64 = 22;

fn reviewer(id: i64) -> Caller {
    Caller {
        user_id: id,
        permissions: P::STAFF_BASIC | P::REVIEW_READING,
    }
}

fn curator() -> Caller {
    Caller {
        user_id: CURATOR,
        permissions: P::STAFF_BASIC | P::REVIEW_READING | P::PUBLISH_READING,
    }
}

fn world() -> World {
    let kind = Arc::new(FakeKind::default());
    kind.items.lock().unwrap().extend([
        NewItem {
            ref_id: 1,
            label: "Chapter 1".into(),
            position: 0,
        },
        NewItem {
            ref_id: 2,
            label: "Chapter 2".into(),
            position: 1,
        },
    ]);
    let clock = Arc::new(TestClock(AtomicI64::new(1_000)));
    let sink = Arc::new(Sink::default());
    let next = Arc::new(AtomicI64::new(100));
    let repo = Arc::new(MemoryReviewRepo::new());
    repo.set_now(1_000);
    let svc = ReviewService::new(
        repo.clone(),
        KindRegistry::new().with(kind.clone()),
        clock.clone(),
        Arc::new(move || next.fetch_add(1, Ordering::SeqCst)),
    )
    .with_events(sink.clone());
    World {
        svc,
        repo,
        kind,
        clock,
        sink,
    }
}

fn new_sub(title: &str) -> CreateSubmission {
    CreateSubmission {
        kind: "library.series".into(),
        payload: json!({ "title": title }),
        title: None,
        summary: String::new(),
    }
}

fn reason(t: &str) -> DecisionInput {
    DecisionInput {
        reason: Some(t.into()),
    }
}

async fn draft(w: &World) -> i64 {
    w.svc
        .create(Caller::member(MEMBER), new_sub("My Series"))
        .await
        .unwrap()
        .submission
        .id
}

async fn in_review(w: &World, by: Caller) -> i64 {
    let id = draft(w).await;
    w.svc.submit(MEMBER, id).await.unwrap();
    w.svc.claim(by, id, false).await.unwrap();
    id
}

#[tokio::test]
async fn full_life_to_published() {
    let w = world();
    let id = draft(&w).await;
    let d = w.svc.get_mine(MEMBER, id).await.unwrap();
    assert_eq!(d.submission.state, SubmissionState::Draft);
    assert_eq!(d.submission.title, "My Series");
    assert_eq!(d.kind_label, "Library series");

    let d = w.svc.submit(MEMBER, id).await.unwrap();
    assert_eq!(d.submission.state, SubmissionState::Submitted);
    assert_eq!(d.items.len(), 2);
    assert!(d.submission.submitted_at_ms.is_some());

    let d = w.svc.claim(reviewer(REVIEWER), id, false).await.unwrap();
    assert_eq!(d.submission.state, SubmissionState::InReview);
    assert_eq!(d.submission.claimed_by, Some(REVIEWER));
    assert_eq!(d.submission.claim_expires_at_ms, Some(1_000 + LEASE_MS));

    let d = w
        .svc
        .approve(reviewer(REVIEWER), id, DecisionInput::default())
        .await
        .unwrap();
    assert_eq!(d.submission.state, SubmissionState::Published);
    assert_eq!(d.submission.decided_by, Some(REVIEWER));
    assert!(d.items.iter().all(|i| i.state == ItemState::Published));
    assert_eq!(*w.kind.published.lock().unwrap(), vec![1, 2]);
    assert!(d.submission.claimed_by.is_none());

    let kinds: Vec<_> = d.events.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        kinds,
        ["created", "submitted", "claimed", "approved", "published"]
    );
    // The submitter heard about the claim, the approval and the publication.
    let types: Vec<String> = w
        .sink
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|(u, p)| {
            assert_eq!(u, &vec![MEMBER]);
            p["type"].as_str().unwrap().to_owned()
        })
        .collect();
    assert_eq!(types, ["claimed", "approved", "published"]);
}

#[tokio::test]
async fn create_validates_kind_and_title() {
    let w = world();
    let mut bad = new_sub("x");
    bad.kind = "cinema.movie".into();
    assert!(matches!(
        w.svc.create(Caller::member(MEMBER), bad).await,
        Err(ReviewError::Invalid(_))
    ));
    assert!(matches!(
        w.svc.create(Caller::member(MEMBER), new_sub("")).await,
        Err(ReviewError::Invalid(_))
    ));
    let mut long = new_sub("x");
    long.title = Some("t".repeat(201));
    assert!(matches!(
        w.svc.create(Caller::member(MEMBER), long).await,
        Err(ReviewError::Invalid(_))
    ));
}

#[tokio::test]
async fn open_submission_limit_exempts_staff() {
    let w = world();
    for _ in 0..MAX_OPEN_PER_MEMBER {
        draft(&w).await;
    }
    assert!(matches!(
        w.svc
            .create(Caller::member(MEMBER), new_sub("one more"))
            .await,
        Err(ReviewError::Conflict(_))
    ));
    // Withdrawing frees a slot.
    let page = w
        .svc
        .list_mine(
            MEMBER,
            SubmissionQuery {
                limit: 50,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    w.svc.withdraw(MEMBER, page.items[0].id).await.unwrap();
    w.svc
        .create(Caller::member(MEMBER), new_sub("fits now"))
        .await
        .unwrap();
    // Staff are unlimited.
    let staff = reviewer(MEMBER);
    for _ in 0..MAX_OPEN_PER_MEMBER + 2 {
        w.svc.create(staff, new_sub("staff")).await.unwrap();
    }
}

#[tokio::test]
async fn only_the_submitter_sees_and_edits_a_draft() {
    let w = world();
    let id = draft(&w).await;
    assert_eq!(
        w.svc.get_mine(OTHER, id).await.unwrap_err(),
        ReviewError::NotFound("submission")
    );
    assert!(w.svc.submit(OTHER, id).await.is_err());
    assert!(w.svc.withdraw(OTHER, id).await.is_err());
    assert!(
        w.svc
            .update(OTHER, id, UpdateSubmission::default())
            .await
            .is_err()
    );
    let d = w
        .svc
        .update(
            MEMBER,
            id,
            UpdateSubmission {
                title: Some("Renamed".into()),
                summary: Some("About it".into()),
            },
        )
        .await
        .unwrap();
    assert_eq!(d.submission.title, "Renamed");
    assert!(matches!(
        w.svc
            .update(
                MEMBER,
                id,
                UpdateSubmission {
                    title: Some("  ".into()),
                    summary: None
                }
            )
            .await,
        Err(ReviewError::Invalid(_))
    ));
    // Once submitted the content is locked.
    w.svc.submit(MEMBER, id).await.unwrap();
    assert!(matches!(
        w.svc.update(MEMBER, id, UpdateSubmission::default()).await,
        Err(ReviewError::Conflict(_))
    ));
}

#[tokio::test]
async fn submit_needs_a_ready_submission() {
    let w = world();
    let id = draft(&w).await;
    w.kind.items.lock().unwrap().clear();
    assert_eq!(
        w.svc.submit(MEMBER, id).await.unwrap_err(),
        ReviewError::NotReady(vec!["add at least one chapter".into()])
    );
    assert_eq!(
        w.svc.get_mine(MEMBER, id).await.unwrap().submission.state,
        SubmissionState::Draft
    );
}

#[tokio::test]
async fn illegal_transitions_are_conflicts() {
    let w = world();
    let id = draft(&w).await;
    let r = reviewer(REVIEWER);
    // A draft is not in the queue.
    assert!(matches!(
        w.svc.claim(r, id, false).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc.approve(r, id, DecisionInput::default()).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc.publish(r, id).await,
        Err(ReviewError::Conflict(_))
    ));
    w.svc.submit(MEMBER, id).await.unwrap();
    // Cannot submit twice, cannot decide without a claim.
    assert!(matches!(
        w.svc.submit(MEMBER, id).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc.approve(r, id, DecisionInput::default()).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc.reject(r, id, reason("no")).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc.release(r, id).await,
        Err(ReviewError::Conflict(_))
    ));
    w.svc.claim(r, id, false).await.unwrap();
    w.svc.reject(r, id, reason("no")).await.unwrap();
    // Rejected is final.
    assert!(matches!(
        w.svc.withdraw(MEMBER, id).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc.submit(MEMBER, id).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc.claim(r, id, false).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc
            .comment(
                Caller::member(MEMBER),
                id,
                CommentInput {
                    body: "hi".into(),
                    internal: false
                }
            )
            .await,
        Err(ReviewError::Conflict(_))
    ));
}

#[tokio::test]
async fn reviewers_need_the_permission_of_the_kind() {
    let w = world();
    let id = draft(&w).await;
    w.svc.submit(MEMBER, id).await.unwrap();
    let media = Caller {
        user_id: 30,
        permissions: P::STAFF_BASIC | P::REVIEW_MEDIA,
    };
    let none = Caller::member(31);
    for c in [media, none] {
        assert!(matches!(
            w.svc.claim(c, id, false).await,
            Err(ReviewError::MissingPermission("REVIEW_READING"))
        ));
        assert!(matches!(
            w.svc.get_for_review(c, id).await,
            Err(ReviewError::MissingPermission(_))
        ));
        assert!(matches!(
            w.svc
                .comment(
                    c,
                    id,
                    CommentInput {
                        body: "x".into(),
                        internal: true
                    }
                )
                .await,
            Err(ReviewError::MissingPermission(_))
        ));
        let q = w
            .svc
            .queue(
                c,
                SubmissionQuery {
                    limit: 10,
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(q.total, 0);
    }
    // The Owner bit implies everything.
    let owner = Caller {
        user_id: 32,
        permissions: P::OWNER,
    };
    w.svc.claim(owner, id, false).await.unwrap();
}

#[tokio::test]
async fn takeover_needs_the_publish_permission() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    // Another reviewer cannot grab a live lease, nor take it over.
    assert!(matches!(
        w.svc.claim(reviewer(REVIEWER2), id, false).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc.claim(reviewer(REVIEWER2), id, true).await,
        Err(ReviewError::MissingPermission("PUBLISH_READING"))
    ));
    let d = w.svc.claim(curator(), id, true).await.unwrap();
    assert_eq!(d.submission.claimed_by, Some(CURATOR));
    // The first reviewer lost the lease: their decision is refused.
    assert!(matches!(
        w.svc
            .approve(reviewer(REVIEWER), id, DecisionInput::default())
            .await,
        Err(ReviewError::Conflict(_))
    ));
}

#[tokio::test]
async fn lease_expiry_frees_the_submission() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    w.clock.0.store(1_000 + LEASE_MS + 1, Ordering::SeqCst);
    // The expired lease cannot decide...
    assert!(matches!(
        w.svc
            .approve(reviewer(REVIEWER), id, DecisionInput::default())
            .await,
        Err(ReviewError::Conflict(_))
    ));
    // ...and somebody else may claim it.
    let d = w.svc.claim(reviewer(REVIEWER2), id, false).await.unwrap();
    assert_eq!(d.submission.claimed_by, Some(REVIEWER2));
}

#[tokio::test]
async fn renewing_a_claim_extends_the_lease_quietly() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    w.clock.0.store(10_000, Ordering::SeqCst);
    let d = w.svc.claim(reviewer(REVIEWER), id, false).await.unwrap();
    assert_eq!(d.submission.claim_expires_at_ms, Some(10_000 + LEASE_MS));
    assert_eq!(d.events.iter().filter(|e| e.kind == "claimed").count(), 1);
}

#[tokio::test]
async fn release_returns_it_to_the_queue_and_only_the_holder_may() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    assert!(matches!(
        w.svc.release(reviewer(REVIEWER2), id).await,
        Err(ReviewError::Conflict(_))
    ));
    let d = w.svc.release(reviewer(REVIEWER), id).await.unwrap();
    assert_eq!(d.submission.state, SubmissionState::Submitted);
    assert!(d.submission.claimed_by.is_none());
    w.svc.claim(reviewer(REVIEWER2), id, false).await.unwrap();
}

#[tokio::test]
async fn nobody_reviews_their_own_submission() {
    let w = world();
    let id = draft(&w).await;
    w.svc.submit(MEMBER, id).await.unwrap();
    let me = reviewer(MEMBER);
    assert!(matches!(
        w.svc.claim(me, id, false).await,
        Err(ReviewError::Forbidden(_))
    ));
    let q = w
        .svc
        .queue(
            me,
            SubmissionQuery {
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(q.total, 0);
}

#[tokio::test]
async fn changes_requested_needs_a_reason_and_resubmit_loops_back() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    let r = reviewer(REVIEWER);
    for bad in [None, Some("   ".to_owned())] {
        assert!(matches!(
            w.svc
                .request_changes(r, id, DecisionInput { reason: bad })
                .await,
            Err(ReviewError::Invalid(_))
        ));
    }
    let d = w
        .svc
        .request_changes(r, id, reason("Chapter 2 is missing pages"))
        .await
        .unwrap();
    assert_eq!(d.submission.state, SubmissionState::ChangesRequested);
    assert_eq!(
        d.submission.decision_reason.as_deref(),
        Some("Chapter 2 is missing pages")
    );
    // The submitter sees the reason in the timeline and was notified.
    let mine = w.svc.get_mine(MEMBER, id).await.unwrap();
    let ev = mine.events.last().unwrap();
    assert_eq!(ev.kind, "changes_requested");
    assert_eq!(ev.body.as_deref(), Some("Chapter 2 is missing pages"));
    assert!(
        w.sink
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|(_, p)| p["type"] == "changes_requested"
                && p["reason"] == "Chapter 2 is missing pages")
    );
    // Editable again; resubmitting goes back to the queue.
    w.svc
        .update(
            MEMBER,
            id,
            UpdateSubmission {
                title: Some("Fixed".into()),
                summary: None,
            },
        )
        .await
        .unwrap();
    let d = w.svc.submit(MEMBER, id).await.unwrap();
    assert_eq!(d.submission.state, SubmissionState::Submitted);
    assert_eq!(d.events.last().unwrap().kind, "resubmitted");
    assert_eq!(d.events.last().unwrap().data["snapshot"]["title"], "Fixed");
}

#[tokio::test]
async fn internal_notes_stay_with_reviewers() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    let r = reviewer(REVIEWER);
    w.svc
        .comment(
            r,
            id,
            CommentInput {
                body: "looks like a re-scan".into(),
                internal: true,
            },
        )
        .await
        .unwrap();
    w.svc
        .comment(
            r,
            id,
            CommentInput {
                body: "please add a cover".into(),
                internal: false,
            },
        )
        .await
        .unwrap();
    w.svc
        .comment(
            Caller::member(MEMBER),
            id,
            CommentInput {
                body: "will do".into(),
                internal: true,
            },
        )
        .await
        .unwrap();
    let mine = w.svc.get_mine(MEMBER, id).await.unwrap();
    let bodies: Vec<_> = mine
        .events
        .iter()
        .filter_map(|e| e.body.as_deref())
        .collect();
    assert_eq!(bodies, ["please add a cover", "will do"]);
    let theirs = w.svc.get_for_review(r, id).await.unwrap();
    assert_eq!(theirs.events.iter().filter(|e| e.internal).count(), 1);
    // A stranger cannot comment.
    assert!(
        w.svc
            .comment(
                Caller::member(OTHER),
                id,
                CommentInput {
                    body: "hi".into(),
                    internal: false
                }
            )
            .await
            .is_err()
    );
    // Empty and oversized notes are refused.
    for body in ["", "  ", &"x".repeat(4001)] {
        assert!(matches!(
            w.svc
                .comment(
                    r,
                    id,
                    CommentInput {
                        body: body.into(),
                        internal: false
                    }
                )
                .await,
            Err(ReviewError::Invalid(_))
        ));
    }
}

#[tokio::test]
async fn per_item_decisions_publish_only_the_approved() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    let r = reviewer(REVIEWER);
    let items = w.svc.get_for_review(r, id).await.unwrap().items;
    // A rejection needs a note.
    assert!(matches!(
        w.svc
            .decide_item(
                r,
                id,
                items[1].id,
                ItemDecisionInput {
                    state: ItemState::Rejected,
                    note: None
                }
            )
            .await,
        Err(ReviewError::Invalid(_))
    ));
    w.svc
        .decide_item(
            r,
            id,
            items[1].id,
            ItemDecisionInput {
                state: ItemState::Rejected,
                note: Some("blurry".into()),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        w.svc
            .decide_item(
                r,
                id,
                9999,
                ItemDecisionInput {
                    state: ItemState::Approved,
                    note: None
                }
            )
            .await,
        Err(ReviewError::NotFound("item"))
    ));
    assert!(matches!(
        w.svc
            .decide_item(
                r,
                id,
                items[0].id,
                ItemDecisionInput {
                    state: ItemState::Published,
                    note: None
                }
            )
            .await,
        Err(ReviewError::Invalid(_))
    ));
    // Another reviewer cannot decide items without the lease.
    assert!(matches!(
        w.svc
            .decide_item(
                reviewer(REVIEWER2),
                id,
                items[0].id,
                ItemDecisionInput {
                    state: ItemState::Approved,
                    note: None
                }
            )
            .await,
        Err(ReviewError::Conflict(_))
    ));
    let d = w
        .svc
        .approve(r, id, DecisionInput::default())
        .await
        .unwrap();
    assert_eq!(*w.kind.published.lock().unwrap(), vec![items[0].ref_id]);
    assert_eq!(d.items[0].state, ItemState::Published);
    assert_eq!(d.items[1].state, ItemState::Rejected);
    assert_eq!(d.items[1].note.as_deref(), Some("blurry"));
}

#[tokio::test]
async fn approving_with_every_item_rejected_is_refused() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    let r = reviewer(REVIEWER);
    for i in w.svc.get_for_review(r, id).await.unwrap().items {
        w.svc
            .decide_item(
                r,
                id,
                i.id,
                ItemDecisionInput {
                    state: ItemState::Rejected,
                    note: Some("no".into()),
                },
            )
            .await
            .unwrap();
    }
    assert!(matches!(
        w.svc.approve(r, id, DecisionInput::default()).await,
        Err(ReviewError::Conflict(_))
    ));
}

#[tokio::test]
async fn a_failing_publish_leaves_it_approved_and_can_be_retried() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    let r = reviewer(REVIEWER);
    w.kind.fail_publish.store(true, Ordering::SeqCst);
    let d = w
        .svc
        .approve(r, id, DecisionInput::default())
        .await
        .unwrap();
    assert_eq!(d.submission.state, SubmissionState::Approved);
    let failed = d
        .events
        .iter()
        .find(|e| e.kind == "publish_failed")
        .unwrap();
    assert!(failed.internal);
    assert_eq!(failed.body.as_deref(), Some("a chapter has no pages"));
    // The retry reports the error while it still fails...
    assert!(w.svc.publish(r, id).await.is_err());
    // ...and finishes once fixed. The submitter never saw the failure.
    w.kind.fail_publish.store(false, Ordering::SeqCst);
    let d = w.svc.publish(r, id).await.unwrap();
    assert_eq!(d.submission.state, SubmissionState::Published);
    assert!(d.submission.published_at_ms.is_some());
    let mine = w.svc.get_mine(MEMBER, id).await.unwrap();
    assert!(mine.events.iter().all(|e| e.kind != "publish_failed"));
    // Publishing again is an illegal move.
    assert!(matches!(
        w.svc.publish(r, id).await,
        Err(ReviewError::Conflict(_))
    ));
    // A reviewer without the permission cannot retry either.
    assert!(w.svc.publish(Caller::member(OTHER), id).await.is_err());
}

#[tokio::test]
async fn withdraw_works_until_approved() {
    let w = world();
    for stage in 0..3 {
        let id = draft(&w).await;
        if stage >= 1 {
            w.svc.submit(MEMBER, id).await.unwrap();
        }
        if stage >= 2 {
            w.svc.claim(reviewer(REVIEWER), id, false).await.unwrap();
        }
        let d = w.svc.withdraw(MEMBER, id).await.unwrap();
        assert_eq!(d.submission.state, SubmissionState::Withdrawn);
        assert!(d.submission.claimed_by.is_none());
        assert!(matches!(
            w.svc.withdraw(MEMBER, id).await,
            Err(ReviewError::Conflict(_))
        ));
    }
    // Not after approval.
    let id = in_review(&w, reviewer(REVIEWER)).await;
    w.kind.fail_publish.store(true, Ordering::SeqCst);
    w.svc
        .approve(reviewer(REVIEWER), id, DecisionInput::default())
        .await
        .unwrap();
    assert!(matches!(
        w.svc.withdraw(MEMBER, id).await,
        Err(ReviewError::Conflict(_))
    ));
}

#[tokio::test]
async fn publish_now_is_for_publishers_only() {
    let w = world();
    let c = Caller {
        user_id: MEMBER,
        permissions: P::STAFF_BASIC | P::REVIEW_READING,
    };
    let id = w
        .svc
        .create(c, new_sub("Direct"))
        .await
        .unwrap()
        .submission
        .id;
    assert!(matches!(
        w.svc.publish_now(c, id).await,
        Err(ReviewError::MissingPermission("PUBLISH_READING"))
    ));
    let publisher = Caller {
        user_id: MEMBER,
        permissions: P::STAFF_BASIC | P::PUBLISH_READING,
    };
    // Only the submitter may publish their own submission directly.
    let other = Caller {
        user_id: OTHER,
        ..publisher
    };
    assert_eq!(
        w.svc.publish_now(other, id).await.unwrap_err(),
        ReviewError::NotFound("submission")
    );
    let d = w.svc.publish_now(publisher, id).await.unwrap();
    assert_eq!(d.submission.state, SubmissionState::Published);
    assert_eq!(d.submission.decided_by, Some(MEMBER));
    assert!(d.items.iter().all(|i| i.state == ItemState::Published));
    assert_eq!(*w.kind.published.lock().unwrap(), vec![1, 2]);
    assert!(matches!(
        w.svc.publish_now(publisher, id).await,
        Err(ReviewError::Conflict(_))
    ));
}

#[tokio::test]
async fn queue_filters_sorts_and_pages() {
    let w = world();
    let mut ids = Vec::new();
    for (i, t) in ["Charlie", "Alpha", "Bravo"].into_iter().enumerate() {
        let id = w
            .svc
            .create(Caller::member(MEMBER), new_sub(t))
            .await
            .unwrap()
            .submission
            .id;
        w.clock.0.store(2_000 + i as i64 * 1_000, Ordering::SeqCst);
        w.svc.submit(MEMBER, id).await.unwrap();
        ids.push(id);
    }
    let r = reviewer(REVIEWER);
    w.svc.claim(r, ids[1], false).await.unwrap();
    let q = |f: &dyn Fn(&mut SubmissionQuery)| {
        let mut q = SubmissionQuery {
            limit: 10,
            ..Default::default()
        };
        f(&mut q);
        q
    };
    // Oldest waiting first.
    let page = w.svc.queue(r, q(&|_| {})).await.unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.items.iter().map(|s| s.id).collect::<Vec<_>>(), ids);
    // Newest first, paged.
    let page = w
        .svc
        .queue(
            r,
            q(&|q| {
                q.descending = true;
                q.limit = 2;
                q.offset = 1;
            }),
        )
        .await
        .unwrap();
    assert_eq!(
        page.items.iter().map(|s| s.id).collect::<Vec<_>>(),
        [ids[1], ids[0]]
    );
    assert_eq!(page.total, 3);
    // By title.
    let page = w
        .svc
        .queue(r, q(&|q| q.sort = SortKey::Title))
        .await
        .unwrap();
    assert_eq!(
        page.items
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Bravo", "Charlie"]
    );
    // Unclaimed, claimed by me, title search, age.
    assert_eq!(
        w.svc
            .queue(r, q(&|q| q.unclaimed = true))
            .await
            .unwrap()
            .total,
        2
    );
    assert_eq!(
        w.svc
            .queue(r, q(&|q| q.claimed_by = Some(REVIEWER)))
            .await
            .unwrap()
            .items[0]
            .id,
        ids[1]
    );
    assert_eq!(
        w.svc
            .queue(r, q(&|q| q.title_contains = Some("brav".into())))
            .await
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        w.svc
            .queue(r, q(&|q| q.submitted_before_ms = Some(3_000)))
            .await
            .unwrap()
            .total,
        2
    );
    // Other states and kinds the reviewer cannot see.
    assert_eq!(
        w.svc
            .queue(r, q(&|q| q.states = vec![SubmissionState::Published]))
            .await
            .unwrap()
            .total,
        0
    );
    assert_eq!(
        w.svc
            .queue(r, q(&|q| q.kinds = vec!["cinema.movie".into()]))
            .await
            .unwrap()
            .total,
        0
    );
    // My submissions list only my own, all states.
    assert_eq!(w.svc.list_mine(MEMBER, q(&|_| {})).await.unwrap().total, 3);
    assert_eq!(w.svc.list_mine(OTHER, q(&|_| {})).await.unwrap().total, 0);
}

#[tokio::test]
async fn requests_vote_and_fulfil() {
    let w = world();
    let input = RequestInput {
        title: "Dungeon Meshi".into(),
        creator: Some("Kui".into()),
        year: Some(2014),
        link: Some("https://example.com/x".into()),
        note: "please".into(),
    };
    let req = w
        .svc
        .create_request(MEMBER, "library", input.clone())
        .await
        .unwrap();
    assert_eq!((req.vote_count, req.voted), (1, true));
    for (section, bad) in [
        ("tv", input.clone()),
        (
            "library",
            RequestInput {
                title: " ".into(),
                ..input.clone()
            },
        ),
        (
            "library",
            RequestInput {
                link: Some("javascript:alert(1)".into()),
                ..input.clone()
            },
        ),
        (
            "library",
            RequestInput {
                year: Some(5),
                ..input.clone()
            },
        ),
    ] {
        assert!(matches!(
            w.svc.create_request(MEMBER, section, bad).await,
            Err(ReviewError::Invalid(_))
        ));
    }
    let voted = w.svc.vote_request(OTHER, req.id, true).await.unwrap();
    assert_eq!(voted.vote_count, 2);
    // Voting twice does not count twice; un-voting does.
    assert_eq!(
        w.svc
            .vote_request(OTHER, req.id, true)
            .await
            .unwrap()
            .vote_count,
        2
    );
    assert_eq!(
        w.svc
            .vote_request(OTHER, req.id, false)
            .await
            .unwrap()
            .vote_count,
        1
    );
    w.svc.vote_request(OTHER, req.id, true).await.unwrap();

    let page = w
        .svc
        .list_requests(
            OTHER,
            RequestQuery {
                section: "library".into(),
                limit: 10,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].id, req.id);
    assert!(page.items[0].voted);

    // A stranger cannot fulfil; an unpublished submission cannot either.
    let sid = in_review(&w, reviewer(REVIEWER)).await;
    assert!(
        w.svc
            .fulfil_request(Caller::member(OTHER), req.id, sid)
            .await
            .is_err()
    );
    assert!(matches!(
        w.svc
            .fulfil_request(Caller::member(MEMBER), req.id, sid)
            .await,
        Err(ReviewError::Conflict(_))
    ));
    w.svc
        .approve(reviewer(REVIEWER), sid, DecisionInput::default())
        .await
        .unwrap();
    w.sink.0.lock().unwrap().clear();
    let done = w
        .svc
        .fulfil_request(Caller::member(MEMBER), req.id, sid)
        .await
        .unwrap();
    assert_eq!(done.status, RequestStatus::Fulfilled);
    assert_eq!(done.submission_id, Some(sid));
    // Both voters were told, and a closed request takes no more votes.
    let sent = w.sink.0.lock().unwrap().clone();
    assert_eq!(sent.len(), 1);
    let mut users = sent[0].0.clone();
    users.sort();
    assert_eq!(users, [MEMBER, OTHER]);
    assert_eq!(sent[0].1["type"], "request_fulfilled");
    assert!(w.svc.vote_request(31, req.id, true).await.is_err());
    assert!(matches!(
        w.svc
            .fulfil_request(Caller::member(MEMBER), req.id, sid)
            .await,
        Err(ReviewError::Conflict(_))
    ));
}

#[tokio::test]
async fn declining_a_request_needs_the_review_permission() {
    let w = world();
    let req = w
        .svc
        .create_request(
            MEMBER,
            "library",
            RequestInput {
                title: "X".into(),
                creator: None,
                year: None,
                link: None,
                note: String::new(),
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        w.svc.decline_request(Caller::member(MEMBER), req.id).await,
        Err(ReviewError::MissingPermission(_))
    ));
    let d = w
        .svc
        .decline_request(reviewer(REVIEWER), req.id)
        .await
        .unwrap();
    assert_eq!(d.status, RequestStatus::Declined);
}

#[tokio::test]
async fn items_can_be_registered_only_by_the_submitter_while_editable() {
    let w = world();
    let id = draft(&w).await;
    let item = |ref_id| NewItem {
        ref_id,
        label: format!("Chapter {ref_id}"),
        position: 0,
    };
    w.svc.register_item(MEMBER, id, item(5)).await.unwrap();
    // Registering again updates the label, it does not duplicate.
    w.svc.register_item(MEMBER, id, item(5)).await.unwrap();
    assert_eq!(w.svc.get_mine(MEMBER, id).await.unwrap().items.len(), 1);
    assert_eq!(
        w.svc.register_item(OTHER, id, item(6)).await.unwrap_err(),
        ReviewError::NotFound("submission")
    );
    w.svc.unregister_item(MEMBER, id, 5).await.unwrap();
    assert!(w.svc.get_mine(MEMBER, id).await.unwrap().items.is_empty());
    w.svc.submit(MEMBER, id).await.unwrap();
    assert!(matches!(
        w.svc.register_item(MEMBER, id, item(7)).await,
        Err(ReviewError::Conflict(_))
    ));
    assert!(matches!(
        w.svc.unregister_item(MEMBER, id, 1).await,
        Err(ReviewError::Conflict(_))
    ));
}

const DAY: i64 = 86_400_000;

impl World {
    fn advance(&self, ms: i64) {
        let t = self.clock.0.fetch_add(ms, Ordering::SeqCst) + ms;
        self.repo.set_now(t);
    }
}

#[tokio::test]
async fn publishing_credits_new_items_once_and_replacements_not_at_all() {
    let w = world();
    w.kind.replacements.lock().unwrap().push(2);
    let id = in_review(&w, reviewer(REVIEWER)).await;
    w.svc
        .approve(reviewer(REVIEWER), id, DecisionInput::default())
        .await
        .unwrap();
    let credits = w.svc.credits(Caller::member(MEMBER), id).await.unwrap();
    assert_eq!(credits.len(), 1, "chapter 2 replaces existing content");
    assert_eq!(credits[0].ref_id, 1);
    assert_eq!(credits[0].user_id, Some(MEMBER));
    assert_eq!(w.svc.credit_total(MEMBER).await.unwrap(), 1);
    // A second submission publishing the same item earns nothing more.
    let id2 = in_review(&w, reviewer(REVIEWER)).await;
    w.svc
        .approve(reviewer(REVIEWER), id2, DecisionInput::default())
        .await
        .unwrap();
    assert_eq!(w.svc.credit_total(MEMBER).await.unwrap(), 1);
}

#[tokio::test]
async fn direct_publish_credits_the_publisher() {
    let w = world();
    let c = curator();
    let id = w
        .svc
        .create(c, new_sub("Mine"))
        .await
        .unwrap()
        .submission
        .id;
    w.svc.publish_now(c, id).await.unwrap();
    assert_eq!(w.svc.credit_total(CURATOR).await.unwrap(), 2);
}

#[tokio::test]
async fn revoking_credits_needs_the_publish_permission_and_a_reason() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    w.svc
        .approve(reviewer(REVIEWER), id, DecisionInput::default())
        .await
        .unwrap();
    assert!(matches!(
        w.svc
            .revoke_credits(reviewer(REVIEWER), id, None, "spam")
            .await,
        Err(ReviewError::MissingPermission(_))
    ));
    assert!(matches!(
        w.svc.revoke_credits(curator(), id, None, "  ").await,
        Err(ReviewError::Invalid(_))
    ));
    assert_eq!(
        w.svc
            .revoke_credits(curator(), id, Some(vec![1]), "stolen scans")
            .await
            .unwrap(),
        1
    );
    assert_eq!(w.svc.credit_total(MEMBER).await.unwrap(), 1);
    assert_eq!(
        w.svc
            .revoke_credits(curator(), id, None, "spam")
            .await
            .unwrap(),
        1
    );
    assert_eq!(w.svc.credit_total(MEMBER).await.unwrap(), 0);
    // Revoking again finds nothing; the revoked rows are kept.
    assert_eq!(
        w.svc
            .revoke_credits(curator(), id, None, "spam")
            .await
            .unwrap(),
        0
    );
    assert_eq!(w.svc.credits(curator(), id).await.unwrap().len(), 2);
    assert!(matches!(
        w.svc.credits(Caller::member(OTHER), id).await,
        Err(ReviewError::MissingPermission(_))
    ));
}

#[tokio::test]
async fn upload_allowance_is_per_day_and_staff_are_exempt() {
    let w = world();
    let m = Caller::member(MEMBER);
    w.svc.charge_upload(m, 30_000_000_000).await.unwrap();
    assert!(matches!(
        w.svc.charge_upload(m, 30_000_000_000).await,
        Err(ReviewError::Conflict(_))
    ));
    let a = w.svc.upload_allowance(m).await.unwrap();
    assert_eq!(a.used_bytes, 30_000_000_000);
    assert_eq!(a.remaining_bytes, Some(20_000_000_000));
    // A refused reservation counts for nothing.
    w.svc.charge_upload(m, 20_000_000_000).await.unwrap();
    // The next UTC day starts afresh; one file larger than the whole allowance never fits.
    w.advance(DAY);
    assert!(w.svc.charge_upload(m, 60_000_000_000).await.is_err());
    w.svc.charge_upload(m, 1).await.unwrap();
    // Staff are not limited.
    w.svc
        .charge_upload(reviewer(REVIEWER), u64::MAX)
        .await
        .unwrap();
    assert_eq!(
        w.svc
            .upload_allowance(reviewer(REVIEWER))
            .await
            .unwrap()
            .limit_bytes,
        None
    );
}

#[tokio::test]
async fn idle_drafts_are_warned_then_abandoned_and_purged() {
    let w = world();
    let id = draft(&w).await;
    let busy = draft(&w).await;
    w.advance(22 * DAY);
    assert_eq!(w.svc.housekeeping().await.unwrap(), SweepReport::default());
    w.advance(DAY + 3_600_000); // just past day 23
    // The second draft is edited on day 23, so it is not idle.
    w.svc
        .update(
            MEMBER,
            busy,
            UpdateSubmission {
                title: Some("Edited".into()),
                summary: None,
            },
        )
        .await
        .unwrap();
    let r = w.svc.housekeeping().await.unwrap();
    assert_eq!((r.warned, r.abandoned), (1, 0));
    let d = w.svc.get_mine(MEMBER, id).await.unwrap();
    assert!(d.events.iter().any(|e| e.kind == "stale_warning"));
    let heard: Vec<String> = w
        .sink
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|(_, p)| p["type"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(heard, ["stale_warning"]);
    // Warned once only.
    assert_eq!(w.svc.housekeeping().await.unwrap(), SweepReport::default());
    // Day 30: warned 7 days ago and idle for 30 days, so it goes.
    w.advance(7 * DAY);
    let r = w.svc.housekeeping().await.unwrap();
    assert_eq!(r.abandoned, 1);
    let d = w.svc.get_mine(MEMBER, id).await.unwrap();
    assert_eq!(d.submission.state, SubmissionState::Withdrawn);
    assert_eq!(*w.kind.purged.lock().unwrap(), vec![id]);
    assert_eq!(
        w.svc.get_mine(MEMBER, busy).await.unwrap().submission.state,
        SubmissionState::Draft
    );
    assert_eq!(w.svc.housekeeping().await.unwrap().abandoned, 0);
}

#[tokio::test]
async fn a_draft_first_seen_after_30_days_is_warned_before_it_is_abandoned() {
    let w = world();
    let id = draft(&w).await;
    w.advance(40 * DAY);
    let r = w.svc.housekeeping().await.unwrap();
    assert_eq!((r.warned, r.abandoned), (1, 0));
    w.advance(2 * DAY);
    assert_eq!(w.svc.housekeeping().await.unwrap().abandoned, 0);
    w.advance(DAY);
    assert_eq!(w.svc.housekeeping().await.unwrap().abandoned, 1);
    assert_eq!(
        w.svc.get_mine(MEMBER, id).await.unwrap().submission.state,
        SubmissionState::Withdrawn
    );
}

#[tokio::test]
async fn rejected_submissions_are_purged_after_14_days_once() {
    let w = world();
    let id = in_review(&w, reviewer(REVIEWER)).await;
    w.svc
        .reject(reviewer(REVIEWER), id, reason("not allowed"))
        .await
        .unwrap();
    w.advance(13 * DAY);
    assert_eq!(w.svc.housekeeping().await.unwrap().purged, 0);
    w.advance(2 * DAY);
    assert_eq!(w.svc.housekeeping().await.unwrap().purged, 1);
    assert_eq!(*w.kind.purged.lock().unwrap(), vec![id]);
    assert_eq!(w.svc.housekeeping().await.unwrap().purged, 0);
}
