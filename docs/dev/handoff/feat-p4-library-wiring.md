# Handoff: feat/p4-library-wiring (Library backend wiring: progress, uploads, ingest job, GC; gate green)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p4-library-wiring, branch feat/p4-library-wiring (base integration/phase3).
Use CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p4-library-wiring-target and CARGO_PROFILE_DEV_DEBUG=0 (add `-j 3` when other agents are compiling: rustc died once with OS error 1450).
Scratch DB `sr_p4_library` on the local Postgres 17 (dropped at the end of the run).
The route shapes are documented in `docs/specs/library-reader.md` section 6 (written first, kept in step with the code).

## Done
- **Progress, read marks, history, bookmarks (LIB-RDR-012, backend of LIB-RDR-013).** `ReadingStateRepository` (sr-library) over the
  existing tables 0201, `PgLibraryRepo` implementation (`sr-db/src/library_state_repo.rs`), `LibraryService` methods
  (`reading_service.rs`), routes in `sr-api/src/routes/library/reading.rs`:
  `GET|PUT|DELETE /library/chapters/{id}/progress`, `POST /library/chapters/{id}/read` ("mark read up to here"),
  `GET /library/me/progress` (history, latest per series by default, keyset), `GET|PUT /library/chapters/{id}/bookmarks`,
  `DELETE /library/bookmarks/{id}`, `GET /library/me/bookmarks`. Conflict prompt: `PUT` takes `base_updated_at_ms`; a newer
  stored row is kept and returned with `applied: false`, `force: true` overwrites (guard is the WHERE of the upsert, no race).
  `ChapterList` chapters now carry `progress.updated_at_ms`. Restricted series need age confirmation here too (900011).
  200 bookmarks per chapter.
- **Gateway sync.** `SR_LIBRARY_UPDATE` is a `DomainEvent::UserScoped` published to the account's own topic through
  `BusLibraryEvents` (sr-api). Types: progress, progress_cleared, read_marks, bookmark_set/removed, follow/unfollow, upload,
  chapter_published (to followers with the bell on, fanned out in a spawned task, 1000 per page, 50 pages max). Documented in doc 05 section 6.3.
- **Upload path (LIB-ING-002/004/005 wiring).** Presigned PUT (the attachment pattern): `POST /admin/library/chapters/{id}/uploads`
  -> PUT bytes -> `POST /admin/library/uploads/{id}/complete` -> job -> poll `GET /admin/library/uploads/{id}` (and
  `GET .../chapters/{id}/uploads`). Table `reading_uploads` (0211) is the status. Extension must fit the chapter format; size is
  checked at begin (request) and again at complete (HEAD of the real object; an oversized object is deleted and the upload fails).
  One upload per chapter in flight; an unused slot is superseded by the next begin.
- **Ingest job `library.ingest`** (`sr-server/src/library_jobs.rs`, `LibraryService::run_ingest`): spools the object to a temp file,
  runs `ingest_archive_rev` (CBZ/CB7/CBR, RAR switch applied) or `epub::inspect` / `pdf::validate`, replaces pages through the
  repository (same path as the publisher service), records warnings and ComicInfo/EPUB metadata, deletes the transient file, emits
  `upload` to the uploader. Bad files fail the upload with the reason; infrastructure errors retry (3 attempts) then fail the upload.
  Staff access log: `library.upload.begin` / `library.upload.complete` (HTTP, as the publisher) and `library.chapter.ingest` /
  `library.chapter.ingest_failed` (job, recorded as the uploading user through `AdminLibraryAudit`).
- **Operator settings** registered in the Admin settings catalog (group `library`): `library.ingest.rar_enabled` (bool, default on,
  read into `ArchiveLimits::rar_enabled`) and `library.ingest.max_upload_mb` (1..4096, default 512, read into
  `ArchiveLimits::max_archive_bytes` and the request check). Read from `platform_settings` per upload and per job.
- **Revisioned page keys.** Ingest from an upload stores pages under `library/{series}/{chapter}/r{upload}/NNNN.webp`
  (`image::page_key(.., Some(rev), ..)`), so a failed re-upload cannot delete live pages (the pipeline deletes what it uploaded on
  failure) and CDN caches never see changed content under one URL. `ingest_archive` still uses the old flat keys (tests, tools).
- **Storage GC (`library.gc`, hourly singleton id 150).** `reading_gc_keys` (0210) is filled inside the same transaction as the
  change: replace_pages (keys that drop out), delete chapter / edition / volume / series (pages, variants, EPUB/PDF sources, covers,
  unfinished upload files), cover change, source replaced. The sweep waits 10 minutes, re-checks references (a re-used key is dropped
  from the queue, not deleted), deletes the object, expires upload slots older than a day and queues their objects. Partial indexes
  on `cover_key` / `source_key` keep the check cheap.
- **Keyset chapter pagination.** `limit` (1..500), `after`, `order=asc|desc` on both chapter list routes, cursor `edition_number_id`,
  `next_cursor` in `ChapterList`. Without `limit` the old all-chapters behaviour is unchanged.
- **EPUB/PDF reading hook.** `PageManifest.source_url` (signed `/cdn/library/...` link) for non-image chapters. `rules::chapter_input`
  now lets an EPUB/PDF draft exist without a `source_key` (the upload fills it; publishing still needs one). `update_chapter` no longer
  wipes an ingested `source_key` when the body has none.
- openapi.json and api-client types regenerated (the generator ran from the main clone's node_modules because the worktree has none;
  `pnpm install` was not run here).
- Docs: library-reader spec section 6, doc 05 6.3, doc 04 section 11, inventory 13 (LIB-RDR-012 ticked, notes on 013, 004, 008, SOC-007).

## Not done
- **LIB-ING-008 `SubmissionKind` impls.** `sr-review` is an empty crate and there is no `submissions` table, so there is no framework to
  implement against. Delivered the publish half that the kinds will call (upload endpoints, ingest job, publish routes, access-log
  entries); documented as a gap in the spec (6.6) and inventory. Needs the review framework first.
- Volume bundles (one archive of many chapters) are not split; one archive per chapter.
- PDF page rasterization (owner decision pending), EPUB cover is reported in metadata (`has_cover`) but not stored as a series cover.
- LIB-SOC-007 is partial: live gateway nudge only; no stored notification list, push or Lounge feed module.
- LIB-RDR-016/017, LIB-SOC-001..006 untouched (ratings, trending, lists, downloads, presence).
- Not tested: a live S3 presigned PUT (in-memory storage uses a fake signer); the CDN content type for `.epub`/`.pdf` source URLs
  (CDN module is not mine); RAR decode of a real compressed archive.
- Known limit: a failed ingest of a re-upload leaves the previous pages intact, but objects of an ingest that crashes mid-way (process
  killed) are not tracked; they sit under `library/{s}/{c}/r{upload}/` and are not collected (rare; a prefix sweep could be added).

## Tests
- sr-library: 106 unit tests in the lib (was 99): reading validation, event payloads, cursors, upload file names, settings defaults.
- sr-api: 298 in the lib (was 291); 7 new library router tests: auth matrices of the 13 new routes, progress conflict/force/validation,
  age gate on reading state, marks/history/bookmarks round trip, paging parameter errors, the upload flow with the access-log
  entries, openapi paths and schemas.
- sr-server: 2 new unit tests (settings snapshot parsing; settings exist in the Admin catalog with matching defaults).
- Postgres-backed (ignored): `cargo test -p sr-db --test library_wiring -- --ignored --test-threads=1`, 11 tests: progress conflict and
  events, hidden/restricted gates, read marks + history keyset + bookmarks, bookmark cap, keyset paging both directions across editions,
  real CBZ upload -> job -> pages -> replacement -> GC, refusals (extension, size, oversize object, junk archive, RAR switch), PDF
  source + replace + GC reference re-check, delete queues (chapter, pages, cover, series), expiry/supersede of slots, follower notification.
  The 10 existing `library_repo` tests and the rest of sr-db's ignored tests still pass (all 17 groups green).
- Gates: `cargo fmt --check`, `clippy --workspace --all-targets -D warnings`, `cargo test --workspace`, `cargo deny check` all green.
  No new crates beyond dev-dependencies already in the lockfile (`zip`, `image`, `bytes`) and `futures`/tokio features for sr-library.

## Decisions taken
1. Presigned PUT instead of multipart through the API (no new axum feature, no body-limit exemptions, same pattern as attachments).
2. Upload targets an existing chapter whose format decides the file type (no format sniffing to pick the chapter type).
3. Conflict prompt carried by `applied: false` in a 200 answer instead of a 409 (the 409 error body cannot carry the server position).
4. Revisioned page keys (above) and a 10 minute GC grace; GC re-checks references instead of trusting the queue.
5. Metadata edits never clear an ingested `source_key`.
6. Operator limit is `library.ingest.max_upload_mb` (a new key), default 512 MiB, matching `ArchiveLimits`/PDF defaults.

## Notes for the other agents
- Admin UI: the new settings use group `library`; check the Limits page renders unknown groups.
- Library UI: use `GET /library/series/{id}/chapters?limit=&after=&order=`, the progress `base_updated_at_ms` handshake, and
  `SR_LIBRARY_UPDATE` (doc 05 6.3). `PageManifest.source_url` is set for EPUB/PDF chapters instead of pages.
