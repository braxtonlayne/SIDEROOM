# 12 — Submissions & Review (`REV`)

Phase 4 (shared by Cinema, Library, apps, packages). Spec: [submissions-and-review](../specs/submissions-and-review.md), [ingest-naming-conventions](../specs/ingest-naming-conventions.md).

## REV-CORE — Pipeline
- [ ] **REV-CORE-001** [DB] submissions, submission_items, submission_events, contribution_credits, requests, request_votes
- [ ] **REV-CORE-002** [BE] State machine with per-item states, transitions, permission checks per kind
- [ ] **REV-CORE-003** [BE] Kind registry (`SubmissionKind` trait: validate payload, processing jobs, publish action, preview data)
- [ ] **REV-CORE-004** [BE] Claim/lock (30 min soft lock), release, reassign
- [ ] **REV-CORE-005** [BE] Decisions API (approve all / per item, request changes with note, reject with template), direct publish path
- [ ] **REV-CORE-006** [BE] Contribution credit grant/revoke on publish/unpublish (unique per item)
- [ ] **REV-CORE-007** [BE] Notifications to submitter (system DM + `SR_SUBMISSION_UPDATE`) and Staff Server card updates
- [ ] **REV-CORE-008** [BE] Member rate limits (bytes/day, open submissions), staff exemption
- [ ] **REV-CORE-009** [JOB] Purge rejected submission files after 14 days; abandon stale drafts after 30 days (notify first)

## REV-UP — Uploads
- [ ] **REV-UP-001** [BE] Resumable multipart upload sessions (create, part URLs, complete, abort, list parts for resume)
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
- [ ] **REV-RV-001** [FE] Queue list (filters: kind, age, submitter, size, flags; sort; claim indicator)
- [ ] **REV-RV-002** [FE] Review page: header (submitter, timeline), previews (video proxy player, page reader, package sandbox preview, manifest diff), per-item approve/reject toggles, metadata edit-in-place
- [ ] **REV-RV-003** [FE] Duplicate panel (merge into existing, mark replacement)
- [ ] **REV-RV-004** [FE] Decision bar (Approve selected, Request changes (note required), Reject (template + note))
- [ ] **REV-RV-005** [FE] Staff Server `#review-queue` card (summary, thumbnails, Approve/Request changes/Reject/Open buttons)

## REV-MY — Submitter views
- [ ] **REV-MY-001** [FE] My submissions list (state chips, progress for processing, reviewer notes)
- [ ] **REV-MY-002** [FE] Changes-requested flow (notes inline on items, resubmit)
- [ ] **REV-MY-003** [FE] Withdraw submission

## REV-REQ — Requests (non-bounty)
- [ ] **REV-REQ-001** [BE] Requests CRUD + upvotes + fulfil link to submission + notify requesters
- [ ] **REV-REQ-002** [FE] Requests board per section (sort by votes/new, status filters, "I'll upload this" claim note)
