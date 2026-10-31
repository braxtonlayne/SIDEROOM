# Handoff: feat/p34-prf-rings (PRF-RNG-001, 24-hour status rings)

Worktree `C:/Users/braxt/Desktop/SIDEROOM_wt/p34-prf-rings`, branch `feat/p34-prf-rings`, base `integration/phase34`. Local commits only (nothing pushed). Migration 1710 (range 1710 to 1719), job id 1710, ports 8083/5176 (no server was started).

## Done
- **Audience, in one place.** `status_ring_visible_to(ring, viewer)` (migration 1710) is the only audience rule: the ring is live (not expired, not deleted by its author, author active), nobody blocked either way, then the author, or mutual friends (`audience = friends`), or both parties full members (not pending screening) of a chosen, non-deleted server (`audience = guilds`). Every reader (feed, one ring, media, view, report, the profile cap) asks it, so a stranger, a blocked person and a member of an unchosen server get the same 404 as for a ring that does not exist.
- **Author's viewer list** (`status_ring_views`): kept with the ring, readable by the author only (`GET /users/@me/rings/{id}/viewers`; anybody else gets the missing-ring 404); the list of chosen servers is also the author's alone.
- **Hard expiry.** Job `rings.expire` (id 1710, every 10 minutes, `crates/sr-server/src/rings_jobs.rs`): deletes the row, queues the object (and any evidence copy) in `status_ring_purge`, then deletes the objects from the author's own mount or from the instance bucket under `rings/`; a bucket outage keeps the entry for 20 passes. The author can delete any time.
- **Moderation.** Report kind `status_ring` through the generic `POST /moderation/reports`: reporter must see the ring, own ring refused, private `target_summary` evidence (no viewer list), system hold (`HoldScope::StatusRing`, also placeable by staff), routed to platform staff. The held row and object stay after expiry (a DB trigger refuses deleting a held ring; the author's delete only hides it); a held file on a member's own mount is copied to the instance bucket (`rings/evidence/<id>`). Staff read the file through `GET /moderation/rings/{id}/media?reason=` (`MANAGE_CASES`, access-log row `moderation.ring.read` first). The case keeps its evidence and can still act after the ring is gone.
- **DSR.** Export section `rings.json` (active rings, audience, viewer list, rings the member opened). Erasure deletes every ring no hold freezes (objects queued), hides held ones, forgets the rings the member opened and their allowance rows; held rings are counted and their holds flagged like held messages (`crates/sr-db/src/rings_dsr.rs`, one registration line each in `account_deletion.rs`, `data_export.rs`, `admin_repo_dsr.rs`).
- **Limits.** Feature `lounge.rings` (platform scope only, default off since the beta checkpoint of 2026-10-30, P34-RNG-1): `max_per_day` 20 (deleting a ring does not refund it), clip 60 s, 50 MiB, text 280 characters, 10 servers.
- **For prf-profiles** (`ring_tier` cap): `sr_userfeat::rings::ring_audience(pool, author)` (audience of the newest live ring, `None` = "no ring") and `viewer_may_see_a_ring_of(pool, viewer, author)`. `authz_matrix.rs` was not edited.
- **Routes** (all session-only): `GET /rings`, `GET /rings/{id}`, `GET /rings/{id}/media` (Range, no-store, nosniff), `POST /rings/{id}/view`, `GET|POST /users/@me/rings`, `POST /users/@me/rings/media`, `DELETE /users/@me/rings/{id}`, `GET /users/@me/rings/{id}/viewers`, `GET /moderation/rings/{id}/media`. Ten rows in `route_matrix.tsv` (`self`, `visibility`, `staff` + `MANAGE_CASES`).
- **UI** (`apps/web/src/lounge/rings`): `RingTray` (one line in `HomeSidebar`), `RingAvatar` (decoration only inside lists and links, never a nested control; a button elsewhere), `RingViewer` (plays a person's rings, viewer list and delete for the author, Report for everybody else), `RingComposer`, `rings.css` (tokens and logical properties; the unseen pulse stops under `prefers-reduced-motion`), i18n `p34-rings-1.ts`. `status_ring` added to `ReportKind`/`TITLE_KEYS`, `TargetKind` and the admin target labels.
- **Docs and gate.** Doc 09 row; `e2e/tests/moderation-gate.spec.ts` step 3c (report a ring: held, 404 for a stranger, viewer list is the author's alone, evidence survives the deletion).

## Tests run
- `cargo test -p sr-userfeat --test rings -- --include-ignored --test-threads=1`: 11 Postgres tests (friend, one-way friend, shared-server-only, chosen vs unchosen server, pending member, blocked both ways, identical 404, author-only viewer list, hard expiry on instance bucket and author mount incl. outage retry, author delete, hold exception incl. trigger and evidence copy, limits and validation, profile-cap functions, export and erasure).
- `cargo test -p sr-moderation --test rings`: report, hold, private evidence, logged staff read, case acts after the ring is gone.
- `cargo test -p sr-server --lib rings -- --include-ignored`: HTTP-level Postgres test (`rings_pg_tests.rs`): stranger, blocked, other-server member, friend (on a servers ring) and chosen-server member (on a friends ring) get a status and body identical to a missing ring on all five ring routes and see nothing in `GET /rings`; rightful audience gets the ring and media headers; a viewer cannot read the viewer list or delete; plus the `rings_jobs` units.
- `cargo test -p sr-api --lib userfeat::rings` (clip sniff, range parse); `cargo test -p sr-api --test route_matrix` (6).
- Gates: `cargo fmt --check` and `cargo clippy -j 2 --all-targets -- -D warnings` on sr-db, sr-userfeat, sr-moderation, sr-api, sr-server.
- Web: vitest `src/lounge/rings` (11, with axe), `src/moderation`, `src/i18n`, `src/admin/moderation`, `HomeSidebar` flows; `tsc --noEmit`; eslint and prettier on the touched files.

## Decisions
- A clip's length is the client's claim, clamped to 60 s; the server sniffs the MP4/WebM container and enforces 50 MiB (no video decode on the server).
- Photos go through the profile-picture image pipeline (type sniffed, re-encoded, metadata stripped, SVG refused, 8 MiB).
- A ring on the author's own mount is stored there with their sealed credentials; if they have none (or none active) it goes to the instance bucket under `rings/`.
- Moderation constraint lists (`moderation_cases`/`moderation_reports` `target_kind`, `moderation_holds` `scope_kind`) are rebuilt in the migration from their current definition with `status_ring` appended, so they compose with other waves' additions whatever order the migrations land in.
- Real-time (gateway) events for rings were not added: the tray refreshes when it is a minute stale and after a post, delete or open.

## Still open / for the integrator
- A crash between storing an upload and writing the row leaves an orphan object under `rings/` (no sweep).
- `authz_matrix.rs` was not run by this agent (prf-profiles owns it this wave); the ten rows follow the vocabulary, and `route_matrix` passes.
- Erased members' mount objects stay in their own bucket (the mount is wiped in the same transaction), as for every BYOS object.

## Owner questions
- Should rings be on by default for every instance (`lounge.rings` is on), given they can store files on the instance bucket? The platform switch is the kill switch.
- Is the 24-hour limit of the daily allowance (rolling 24 hours, 20 rings) the right shape, or should it reset at midnight UTC?

## CHANGELOG lines (for "## Phase 34")
- Status rings: share a photo, short clip, line of text or track on your avatar for 24 hours with your mutual friends or the servers you choose. Rings are deleted for good when the day is over, and only you can see who opened yours.
- Rings can be reported; a reported ring is kept for moderators until the case is decided.
- Your data export lists your active rings; deleting your account deletes them.
