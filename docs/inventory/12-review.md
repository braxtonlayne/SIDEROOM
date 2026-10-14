# 12 — Submissions & Review (`REV`)

Phase 4 (shared by Cinema, Library, apps, packages). Spec: [submissions-and-review](../specs/submissions-and-review.md), [ingest-naming-conventions](../specs/ingest-naming-conventions.md).

## REV-CORE — Pipeline
- [ ] **REV-CORE-001** [DB] submissions, submission_items, submission_events, contribution_credits, requests, request_votes *(partial, phase 5: `submissions`, `submission_items`, `submission_events`, `content_requests`, `content_request_votes` in migration 0290; `contribution_credits` is still missing, see REV-CORE-006)*
- [x] **REV-CORE-002** [BE] State machine with per-item states, transitions, permission checks per kind *(phase 5: `sr_review::machine` and the guarded writes; per-item states pending/approved/rejected/published; the spec `processing` state is not a state, Library conversion runs in the ingest job while the submission is a draft)*
- [x] **REV-CORE-003** [BE] Kind registry (`SubmissionKind` trait: validate payload, processing jobs, publish action, preview data) *(phase 5: `SubmissionKind` + `KindRegistry`; kinds `library.series`, `library.items` in `sr-library::submissions`)*
- [x] **REV-CORE-004** [BE] Claim/lock (30 min soft lock), release, reassign *(phase 5: 30 min lease, renew by claiming again, release, take-over by holders of the kind's publish permission)*
- [x] **REV-CORE-005** [BE] Decisions API (approve all / per item, request changes with note, reject with template), direct publish path *(phase 5: per-item approve/reject, request changes and reject with a required reason, approve publishes, publish-now for the submitter holding the publish permission; reject reason templates are chosen in the web UI and joined into the reason)*
- [ ] **REV-CORE-006** [BE] Contribution credit grant/revoke on publish/unpublish (unique per item) *(not done: no `contribution_credits` table yet)*
- [ ] **REV-CORE-007** [BE] Notifications to submitter (system DM + `SR_SUBMISSION_UPDATE`) and Staff Server card updates *(partial, phase 5: `SR_SUBMISSION_UPDATE` to the submitter and every decision in the staff access log; the system-user DM and the Staff Server card are not built)*
- [ ] **REV-CORE-008** [BE] Member rate limits (bytes/day, open submissions), staff exemption *(partial, phase 5: 5 open submissions per member, staff exempt; bytes per day is not enforced)*
- [ ] **REV-CORE-009** [JOB] Purge rejected submission files after 14 days; abandon stale drafts after 30 days (notify first)

## REV-UP — Uploads
- [ ] **REV-UP-001** [BE] Resumable multipart upload sessions (create, part URLs, complete, abort, list parts for resume) *(not done as specified: Library uploads use one presigned PUT per file, see the library-reader spec section 6; multipart sessions are still to do)*
- [ ] **REV-UP-002** [FE] Upload manager (IndexedDB ledger, parallel parts, retry, resume after refresh, pause/cancel, global progress dock)
- [ ] **REV-UP-003** [FE] Folder drag-and-drop + directory picker traversal
- [ ] **REV-UP-004** [BE] Archive pre-inspection (list entries without full extraction) for structure preview
- [ ] **REV-UP-005** [BE] Watched-folder scanner (polling, stability check, creates drafts)
- [ ] **REV-UP-006** [BE] Storage import (S3 prefix / local path) with dry-run structure report
- [ ] **REV-UP-007** [BE] Naming parsers (`sr-media::naming`, `sr-reader::naming`) + ≥ 300 fixture tests
- [ ] **REV-UP-008** [FE] Naming guide popover + tester (paste names → parsed table)

## REV-ED — Metadata editor
- [ ] **REV-ED-001** [FE] Two-pane editor: file/structure tree (drag to reorder/reassign, rename, exclude) + kind-specific form
- [ ] **REV-ED-002** [FE] Structure proposals from parser with accept/adjust
- [ ] **REV-ED-003** [FE] Artwork uploader with crop (WASM), frame-grab from video proxy
- [ ] **REV-ED-004** [FE] Validation summary panel (errors/warnings), Submit / Save draft / Publish now (publishers)
- [ ] **REV-ED-005** [BE] Metadata lookup providers (feature-flagged; TMDB, AniList, MangaDex/MangaUpdates, ComicVine) with per-field accept
- [ ] **REV-ED-006** [BE] Duplicate detection (title/year/episode match; perceptual hash `image_hasher` for pages/covers)

## REV-RV — Reviewer workspace
- [x] **REV-RV-001** [FE] Queue list (filters: kind, age, submitter, size, flags; sort; claim indicator) *(phase 5: Admin > Review queues; filters kind, state, claim, age, title; sort; claim indicator; paging. No submitter filter in the UI, the API has `submitter`)*
- [ ] **REV-RV-002** [FE] Review page: header (submitter, timeline), previews (video proxy player, page reader, package sandbox preview, manifest diff), per-item approve/reject toggles, metadata edit-in-place *(partial, phase 5: review page with series metadata, a diff since the previous submit, chapter page previews, per-item approve/reject, timeline and notes; video proxy player, package sandbox preview, manifest diff and metadata edit-in-place are not built)*
- [ ] **REV-RV-003** [FE] Duplicate panel (merge into existing, mark replacement)
- [x] **REV-RV-004** [FE] Decision bar (Approve selected, Request changes (note required), Reject (template + note)) *(phase 5: Approve, Request changes (note required), Reject (reason template + note), Release, Take over)*
- [ ] **REV-RV-005** [FE] Staff Server `#review-queue` card (summary, thumbnails, Approve/Request changes/Reject/Open buttons)

## REV-MY — Submitter views
- [x] **REV-MY-001** [FE] My submissions list (state chips, progress for processing, reviewer notes) *(phase 5: Library > Submissions, state filter, reviewer notes inline; no processing progress bar, conversion status shows per chapter)*
- [x] **REV-MY-002** [FE] Changes-requested flow (notes inline on items, resubmit) *(phase 5: the reviewer note banner, per-item notes, edit, upload again, "Send for review again")*
- [x] **REV-MY-003** [FE] Withdraw submission *(phase 5)*

## REV-REQ — Requests (non-bounty)
- [x] **REV-REQ-001** [BE] Requests CRUD + upvotes + fulfil link to submission + notify requesters *(phase 5 backend: create, upvote/unvote, fulfil with a published submission and notify voters, decline; `/requests`)*
- [ ] **REV-REQ-002** [FE] Requests board per section (sort by votes/new, status filters, "I'll upload this" claim note)
