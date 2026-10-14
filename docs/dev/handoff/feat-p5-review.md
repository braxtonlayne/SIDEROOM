# Handoff: feat/p5-review (review pipeline foundation: submissions, Library kinds, reviewer workspace, My submissions)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p5-review, branch feat/p5-review (base integration/phase4).
Built with CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p4-library-wiring-target and CARGO_PROFILE_DEV_DEBUG=0.
Scratch DB `sr_p5_review` on the local Postgres 17 (dropped at the end of the run). Migration used: `0290_review_submissions.sql` (only).
The behaviour is documented in `docs/specs/submissions-and-review.md` section 9; `SR_SUBMISSION_UPDATE` is in `docs/05-api-and-gateway.md` section 6.3.

## Done
- **`sr-review` (was an empty crate).** `machine` (pure transition table), `model` (wire types, ids as strings), `kind` (`SubmissionKind` trait + `KindRegistry`),
  `repo` (storage port), `service` (`ReviewService`: create, edit, submit/resubmit, withdraw, publish-now, claim/release/take-over with a 30 minute lease,
  comments (public and internal), per-item decisions, request changes, reject, approve, publish retry, listings, requests), `memory` (in-memory repo for tests).
- **Postgres.** Migration 0290 (`submissions`, `submission_items`, `submission_events`, `content_requests`, `content_request_votes`); `sr-db/src/review_repo.rs`
  (`PgReviewRepo`): every change is one guarded `UPDATE` (state set and lease in the `WHERE`) plus its timeline event in one transaction.
- **Library kinds (LIB-ING-008).** `sr-library/src/submissions.rs`: `LibraryKind::series()` = `library.series`, `LibraryKind::items()` = `library.items`, and
  `LibraryDraftOps` (member draft operations: edit the draft series, add/edit/delete draft chapters, begin/complete/poll uploads, page manifests).
  Members upload through the existing presigned-PUT + `library.ingest` job, scoped to the one series of their submission. Approving publishes the series and
  the approved chapters through `LibraryService` (so followers are notified as for any publish).
- **REST (`sr-api/src/routes/review/`).** Member: `/submissions*`, `/submissions/{id}/library/*`, `/requests*`. Reviewer (staff session + kind permission): `/admin/review/*`.
  Every reviewer action is written to the staff access log (`review.claim|release|item|request_changes|reject|approve|publish|publish_now|request.decline`).
  `BusReviewEvents` sends `SR_SUBMISSION_UPDATE` (user-scoped) to the submitter, and to the voters of a fulfilled request.
- **Server wiring.** `sr-server/src/app.rs` registers both kinds and the deps (extension `ReviewDeps`).
- **Web.** Admin > Review queues is now the reviewer workspace (`admin/sections/Review.tsx`): queue with filters/sort/paging/claim indicator; review page with
  metadata, "what changed" diff since the previous submit, per-chapter approve/reject (note required to reject), chapter page previews, timeline, public and
  internal notes, claim/release/take-over, approve, request changes (reason required), reject (reason template + note), retry publish.
  Library > Submissions (the sidebar entry) is now "My submissions" (`library/Submissions.tsx`): list with state filter and reviewer notes, start a draft
  (new series, or new chapters for a series; `?series=ID` prefills), submission page with details, chapters, file upload with conversion status, submit/resubmit,
  withdraw, timeline and notes, live refresh on `SR_SUBMISSION_UPDATE`. Shared pieces in `apps/web/src/review/`.
- openapi.json and api-client types regenerated (the diff is large because the generated document was stale; it also contains the new paths).
- Docs: spec section 9, doc 05 (`SR_SUBMISSION_UPDATE`), doc 04, library-reader spec 6.6, inventory 12 and 13.

## Not done
- **REV-CORE-001** is partial (no `contribution_credits`), **REV-CORE-006** (credits grant/revoke) not started.
- **REV-CORE-007** partial: no system-user DM and no Staff Server `#review-queue` card (REV-RV-005). **REV-CORE-008** partial: only the open-submission cap (5), no bytes/day.
- **REV-CORE-009** (purge rejected files after 14 days, abandon stale drafts after 30) not built.
- **REV-UP-***: uploads keep the single presigned PUT (decision of the Library wiring); no multipart/resume (REV-UP-001/002), folder drop, archive pre-inspection, watched folders, storage import, naming tester.
- **REV-RV-002** partial (no video/package previews, no metadata edit-in-place by the reviewer, no cover setting by a reviewer in this UI), **REV-RV-003** (duplicate panel) and **REV-ED-*** (two-pane editor, artwork, lookup, duplicate detection) untouched.
- **REV-REQ-002**: the requests board UI is not built (the API is: list, create, vote, fulfil, decline). The Library `Requests` entry is still the placeholder.
- Cinema, app and package kinds (the trait is ready for them).
- The Playwright e2e flow was not run (shared ports 8080/5173).
- A member cannot set a series cover; the cover is left to a reviewer/publisher through the existing admin Library routes (no UI for it yet).

## Tests
- sr-review: 30 unit tests (7 state machine incl. every state x action pair; 23 service tests: full life, illegal transitions, permission failures, own-submission block,
  lease race/expiry/take-over/renew/release, internal note visibility, per-item decisions, failed publish + retry, withdraw, publish-now, rate limit, queue filters/sort/paging, requests).
- sr-api: 11 router tests in `routes/review/tests.rs` (401 matrix for all 28 routes, bot refusal, 403/empty-queue matrix, whole review over HTTP incl. access-log trail
  and gateway events, withdraw, requests board, openapi). Whole sr-api lib: 321.
- Postgres-backed (ignored): `cargo test -p sr-db --test review_repo -- --ignored --test-threads=1`, 6 tests: member series uploaded (real CBZ) -> reviewed -> published and
  visible; new chapters for a published series through changes requested and resubmit; draft ops scoped to the submission's own series; atomic claim race, expiry,
  take-over, reject; queue filters/sort/paging/wildcard search/open limit; requests voted and fulfilled. All 17 groups of sr-db ignored tests green.
- Web: 28 new tests (16 in `admin/__tests__/review.test.tsx`, 12 in `library/__tests__/submissions.test.tsx`; MSW + Testing Library, axe on the queue, review page,
  submissions list and submission page). Whole web suite: 55 files, 655 tests green; typecheck, eslint, prettier, vite build green.
- Gates: `cargo fmt --check`, `clippy --workspace --all-targets -D warnings`, `cargo test --workspace`, `cargo deny check` green.

## Decisions taken
1. A kind is a trait object in `sr-review`; the Library kinds live in `sr-library` (depends on `sr-review`), so `sr-review` stays free of Library types.
2. Members never get publisher rights. `LibraryDraftOps` acts with a sponsored actor only after the HTTP layer proved the submission is the caller's and editable, and
   every call checks the chapter/upload belongs to the submission's series. Cover keys, volume ids and source keys supplied by members are dropped.
3. Approve = decision + publish in one call; if publish fails the submission stays `approved` with a reviewer-only `publish_failed` note and `POST .../publish` retries.
4. The spec's `processing` state is not modelled: conversion runs while the submission is a draft and `submit` refuses chapters without pages (field errors `problems.N`).
5. Reviewers can never review their own submission; decisions need the caller's live claim; take-over needs the kind's publish permission.
6. Reject "reason templates" are UI-side (a select) and joined into the stored reason; the API only requires a non-empty reason.
7. `library.items` membership is explicit: chapters the member created through the submission are registered as items (so two submissions to one series do not collide).
8. The rate limit exemption for staff works by resolving the account's staff standing in the member extractor (best effort, no staff = member).

## Notes for the other agents
- Gateway: `SR_SUBMISSION_UPDATE` is a `DomainEvent::UserScoped`; the web store already forwards every dispatch to `onDispatch`, nothing else to register.
- Cinema/app/package kinds: implement `SubmissionKind`, register it in `sr-server/src/app.rs`, add the permission pair; the queue, claim and decision routes are generic.
- Admin permission table: the sidebar entry "Review queues" is visible with any REVIEW_* bit, but the queue only returns kinds the caller can review (today only Library kinds exist, so
  REVIEW_MEDIA/APPS/PACKAGES holders see an empty queue).
