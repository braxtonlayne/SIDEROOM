# Spec — Submissions & Review Pipeline (shared)

Phases 4–6 · Crate `sr-review` · Decisions: D-081–D-083, D-062–D-064, D-074.

One pipeline handles **Cinema** content, **Library** content, **apps**, and **declarative packages**. This spec defines the shared mechanics. Section specs define the per-kind metadata.

## 1. Roles
| Actor | Can |
|---|---|
| Member | Create submissions, upload files, edit metadata while in `draft` or `changes_requested`, submit, withdraw |
| Direct publisher (`PUBLISH_MEDIA` / `PUBLISH_READING`) | Everything a member can, plus **Publish now** (skip review), plus edit/replace any published item |
| Reviewer (`REVIEW_*`) | Claim, preview, edit metadata, approve/reject individual items, request changes, add notes |

## 2. State machine
```
draft ──submit──► processing ──(jobs done)──► pending_review ──claim──► in_review
  ▲                   │ (job failure → draft with errors)          │
  │                   ▼                                            ├─approve all──► approved ──► published
  └──changes_requested◄────────────────request changes────────────┤
                                                                   ├─partial approve──► approved items published; rest → changes_requested or rejected
withdrawn ◄── submitter (any time before approved)                 └─reject──► rejected (files purged after 14 days)
```
- `processing` runs kind-specific jobs: probe, thumbnails, page conversion, preview transcodes (a low-res 480p proxy so reviewers can watch before the full ladder), checksum, duplicate detection.
- **Per-item decisions:** each `submission_item` (episode, chapter, file) has its own state, so reviewers can approve 9 chapters and reject 1 (D-081).
- **Direct publish:** for publishers, `submit` with `publish_now = true` → processing → auto-approved, published, and logged.

## 3. Upload mechanics
- **Browser:** resumable multipart uploads (tus-like protocol implemented in Rust on top of S3 multipart; 64 MB parts, parallelism 4, resume after refresh using the IndexedDB upload ledger). Folder drag-and-drop uses `webkitdirectory` / DataTransferItem directory traversal.
- **Batch:** drop a whole season or series folder. The naming convention parser (see ingest-naming-conventions) proposes structure, which the user reviews in a tree editor before submitting.
- **Watched folders (staff):** `sr-server` polls configured paths (on the server or a mounted Storage Box). New files create draft submissions owned by a configured staff account.
- **Storage import (staff):** point at an S3 bucket/prefix or local path, preview the detected structure, then import.
- Rate limits for regular members (configurable): e.g. 50 GB/day and 5 open submissions. Staff are unlimited (D-082).

## 4. Metadata editor (submitter and reviewer)
Two-pane layout: file tree (left) and form (right). Kind-specific fields (see the cinema and library specs). Common features:
- Auto-fill from filenames. Optional **metadata lookup** providers behind a feature flag and operator API keys (TMDB, AniList, MangaDex/MangaUpdates/ComicVine): "Fetch details" fills empty fields, and the user confirms each change.
- Artwork upload with crop (canvas crop dialog; the server re-encodes and caps the image), plus "pick frame from video" for stills (deferred). Slots: `cover` (Library series), `poster` and `backdrop` (Cinema title). Library series have no banner in this release.
- Validation summary (missing required fields, duplicates, unsupported files).

## 5. Reviewer workspace
- Queue filters: kind, age, submitter, size, flags (possible duplicate, age-restricted, large).
- **Claim** (soft lock, 30 min, visible to other reviewers).
- Preview: video proxy player with frame scrub; reader preview of pages; package preview (theme applied live in a sandbox frame); app manifest diff.
- Duplicate panel: perceptual-hash matches (pages) and title/year/episode matches (video), with "merge into existing" and "mark as replacement."
- **Edit in place (D-255).** The reviewer who holds the live claim on a submission in `in_review` can fix the draft metadata (series fields, cover, chapter number, title and volume, remove a draft chapter) through the same submission routes the submitter uses (`PUT /submissions/{id}/library/...`, `POST /submissions/{id}/artwork/{kind}`); a stale claim is a 409 that names the holder, the submitter cannot edit while it is in review, and every reviewer write posts a `metadata_edited` timeline event naming the fields. `PUBLISH_*` stays required for the `/admin/library` and `/admin/cinema` writes to published content; credits stay the submitter's. The review page shows the editor as "Editing as reviewer".
- Decision: approve all / per item; request changes (requires a note, visible to the submitter); reject (reason template + note).
- Every decision posts a timeline event, notifies the submitter (DM from the system user + `SR_SUBMISSION_UPDATE`), and updates the Staff Server card.

## 6. Credits and leaderboards (D-074)
On publish, `contribution_credits` rows are inserted per **new** movie / episode / chapter, credited to the submitter. Replacements (same item id) and items merged as duplicates insert nothing. Unpublishing for cause revokes the credit.

## 7. Requests (non-bounty)
"Request a title" (Cinema) and "Request a series" (Library): title, year/author, link, note, and upvotes from other members. Requests appear in a public Requests board inside each section. Fulfilling a request through a submission links them and notifies requesters. (Bounties are future-only, see future-bounties.)

## 8. Acceptance criteria
- [ ] A 40 GB season upload survives a browser refresh and resumes.
- [ ] Reviewers can approve a subset of items and the rest return to the submitter with notes.
- [ ] A direct publisher's upload is published without entering the queue, and appears in the audit log.
- [ ] Re-uploading an existing episode as a replacement grants no contribution credit.
- [ ] A rejected submission's files are purged from storage after 14 days (job test).

## 9. Implementation notes (phase 5 foundation)
What `sr-review` ships today, so the other kinds can plug in. Status per inventory item is in `docs/inventory/12-review.md`.

**Tables (migration 0290).** `submissions` (kind, submitter, state, title, summary, `ref_id`, payload, reviewer lease, decision), `submission_items` (per chapter/episode state: `pending`, `approved`, `rejected`, `published`), `submission_events` (timeline: state changes, public and internal notes, with the reason of each decision), `content_requests` and `content_request_votes`. `contribution_credits` is not created yet.

**States.** `draft`, `submitted`, `in_review`, `changes_requested`, `approved`, `rejected`, `published`, `withdrawn`. (`processing` is not a state: Library file conversion runs in the existing `library.ingest` job while the submission is still a draft, and a submission cannot be submitted until its chapters have pages.) The transition table is `sr_review::machine`; every change goes through it and through a guarded `UPDATE` (state set and lease in the `WHERE`).

| Action | From | To | Who |
|---|---|---|---|
| submit | draft | submitted | submitter (kind checks must pass) |
| resubmit | changes_requested | submitted | submitter |
| publish now | draft, changes_requested | published | submitter holding the kind's publish permission |
| withdraw | draft, submitted, in_review, changes_requested | withdrawn | submitter |
| claim | submitted, in_review (free or expired lease) | in_review | reviewer of the kind, never the submitter |
| release | in_review | submitted | the claim holder |
| request changes / reject | in_review | changes_requested / rejected | claim holder, reason required |
| approve | in_review | approved, then published by the kind | claim holder |
| publish (retry) | approved | published | reviewer of the kind |

**Roles (doc 07).** A kind names its review permission and its publish permission: `library.*` use `REVIEW_READING` (Library Curator, Owner) and `PUBLISH_READING`. A reviewer cannot review their own submission. A live claim lasts 30 minutes; the holder renews it by claiming again; anyone with the publish permission can take over a live claim. Decisions need the caller's live claim.

**Kinds.** `sr_review::SubmissionKind`: `create` (validate the payload, create the draft content), `items`, `validate` (problems block submitting), `preview` (what reviewers see; also stored with every submit so a resubmit can be diffed), `publish` (idempotent). Library kinds live in `sr-library::submissions`: `library.series` (a new series: payload is the series fields plus optional `language`, `edition_label`) and `library.items` (new chapters for a published series: payload `{ series_id, edition_id? }`). Members build the draft with `LibraryDraftOps`: the same catalog writes and the same presigned-PUT upload and `library.ingest` job as publishers, scoped to the submission's own series and only while it is a draft or has changes requested. A member cannot set a cover or a storage key; a reviewer or publisher sets the cover afterwards. Approving publishes the series (new series) and every approved chapter through the Library service; if that fails the submission stays `approved`, a reviewer-only `publish_failed` note is added and `publish` retries.

**REST.** Member: `GET|POST /submissions`, `GET|PATCH /submissions/{id}`, `/preview`, `/submit`, `/withdraw`, `/comments`, `GET /submissions/kinds`; Library drafts under `/submissions/{id}/library/*` (series, chapters, pages, uploads). Reviewer (staff session): `GET /admin/review/queue`, `GET /admin/review/submissions/{id}` (with internal notes), `/preview`, `/claim`, `/release`, `/comments`, `PUT .../items/{item}`, `/request-changes`, `/reject`, `/approve`, `/publish`, `/publish-now`. Queue filters: `kind`, `state`, `claimed=mine|unclaimed`, `submitter`, `q`, `min_age_hours`, `sort=submitted|updated|title`, `order`, `limit`, `offset`. Requests: `GET|POST /requests`, `PUT|DELETE /requests/{id}/vote`, `POST /requests/{id}/fulfil`, `POST /admin/review/requests/{id}/decline`.

**Audit.** Every reviewer action (`review.claim`, `review.release`, `review.item`, `review.request_changes`, `review.reject`, `review.approve`, `review.publish`, `review.publish_now`, `review.request.decline`) is appended to the staff access log with the submission as target and the kind and reason as detail. The timeline (`submission_events`) carries the same decisions for the submitter.

**Limits.** A member may have 5 open submissions (staff are exempt) and 10 open requests. Bytes per day (REV-CORE-008) is not enforced yet; uploads are bounded by `library.ingest.max_upload_mb`.

**Not built yet.** Cinema, app and package kinds; contribution credits (section 6); the system-user DM on decisions and the Staff Server `#review-queue` card; the purge and stale-draft jobs; resumable multipart uploads; duplicate detection; metadata lookup; the requests board UI.

## 10. Phase 13 additions
- **Resumable uploads (section 3).** `UploadRequest.multipart = true` opens a multipart session (parts of 64 MB, parallelism 4 in the browser, URLs valid 6 hours). `GET /admin/library/uploads/{id}/parts` and `GET /submissions/{id}/library/uploads/{id}/parts` report what arrived and give fresh URLs for the rest; `DELETE` cancels. `complete` asks the store for its parts, checks their sizes against the announced size and assembles the object, so the browser never reads ETags. The browser ledger is IndexedDB (`review/uploadManager.ts`). Off with `SR_MEDIA_SINGLE_PUT=1`.
- **Archive pre-inspection.** `GET .../uploads/{id}/inspect` lists an archive without converting it (`sr_library::ingest::inspect`).
- **Ingest sources.** `[ingest] roots` lists the folders staff may point a source at; `/admin/ingest-sources` (CRUD, `dry-run`, `scan`); job `library.sources` polls watched folders every 30 s (each source has its own interval). Imports create draft submissions for the source owner; nothing is published.
- **Metadata lookup.** `[lookup]` turns providers on (`anilist`, `mangadex`, `mangaupdates` need no key; `tmdb_api_key`, `comicvine_api_key`), the platform setting `review.lookup.enabled` gates them at run time; `GET /submissions/lookup/providers`, `GET /submissions/lookup`.
- **Duplicates.** `GET /submissions/{id}/duplicates`, `GET /admin/review/submissions/{id}/duplicates`, `PUT /admin/review/submissions/{id}/duplicate`.
- **Review media.** `GET /admin/review/submissions/{id}/media` gives the signed 480p proxy per video.
- **Review card.** The `#review-queue` card shows the cover or poster and has a Request changes button (`sr:review-changes:<id>`).
