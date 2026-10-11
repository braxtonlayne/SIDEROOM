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
- Artwork upload with crop (WASM image tools), plus "pick frame from video" for stills.
- Validation summary (missing required fields, duplicates, unsupported files).

## 5. Reviewer workspace
- Queue filters: kind, age, submitter, size, flags (possible duplicate, age-restricted, large).
- **Claim** (soft lock, 30 min, visible to other reviewers).
- Preview: video proxy player with frame scrub; reader preview of pages; package preview (theme applied live in a sandbox frame); app manifest diff.
- Duplicate panel: perceptual-hash matches (pages) and title/year/episode matches (video), with "merge into existing" and "mark as replacement."
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
