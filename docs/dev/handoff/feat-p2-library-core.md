# Handoff: feat/p2-library-core (Library catalog backend; gate green)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p2-library-core, branch feat/p2-library-core (base integration/phase2).
Use CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p2-library-core-target and CARGO_PROFILE_DEV_DEBUG=0.
Scratch DB `sr_p2_library` on the local Postgres 17 (dropped at the end of the run; recreate with `createdb`, the tests migrate it themselves).

## Done
- Migrations 0200 (reading_series, reading_editions, reading_volumes, reading_chapters, reading_pages) and 0201
  (reading_progress, reading_bookmarks, reading_follows). docs/04 section 11 updated.
- New crate `crates/sr-library` (no HTTP, no SQL):
  - `model`, `rules` (validation/normalisation, 100% unit tested), `repo` (storage port `LibraryRepository`),
    `service` (`LibraryService`: member reads via `Viewer`, publisher writes via `Actor`), `search` (tantivy `SeriesIndex`,
    edge n-grams over titles/alt titles and authors/artists/genres/tags, title hits boosted).
  - Permissions: `PUBLISH_READING` create/edit/publish/pages; `DELETE_CONTENT` delete series/edition/chapter/volume;
    merge needs both; unpublish accepts either.
  - Visibility is enforced in the repository (`Scope`): published + not merged; age-restricted only for confirmed accounts
    AND only with `age=include|only`. A hidden object is always 404. Search hits are re-read from the DB with the viewer's scope.
  - Page manifest returns signed `/cdn/library/{series}/{chapter}/{NNNN}[_{w}].webp` URLs (HMAC, expiry rounded to the hour,
    6-7 h) with 720/1080/1600 variants, blurhash, spread flag, prev/next chapter ids.
  - Merge: moves editions (label collisions get ` (id)` suffix), chapters, follows (union), progress; source becomes a
    redirect (`merged_into`); public reads of the old id answer with the target.
  - `ingest` (LIB-ING): `image` (decode, lossless WebP, variants, blurhash, spread detection), `archive` (hardened ZIP/CBZ),
    `comicinfo`, `series_json`, `pipeline::ingest_archive` (CBZ -> uploaded pages -> `Vec<PageInput>`; deletes what it uploaded on failure).
- `sr-db::library_repo::PgLibraryRepo` (all queries runtime-checked, dynamic ones built with bound parameters only).
- `sr-api::routes::library`: member routes `/library/*` (browse, series, chapters, pages, search, updates, me/follows, follow PUT/DELETE)
  and publisher routes `/admin/library/*` (staff session, every change written to the staff access log through the new
  `AdminService::record_action`). New CDN kind `library` (signed, not public). Unique operationIds (`library_*`, `admin_library_*`).
- sr-server wiring: service over Postgres, shares the attachment `CdnSigner`, index in `<search dir>/library` rebuilt at boot
  (`reindex_all`), `PgAgeGate`.
- openapi.json and api-client types regenerated.
- Tests: sr-library 56 unit tests (rules, search, natural sort, image pipeline, archive hardening incl. bomb/traversal/oversize/
  budget, ComicInfo incl. DTD/entity refusal, series.json, CBZ pipeline with an in-memory object store); sr-api 9 router tests
  (401/403 matrices for every route, age gate reaching the repo, normalisation, access-log row, openapi paths);
  sr-db 10 Postgres tests (`cargo test -p sr-db --test library_repo -- --ignored --test-threads=1`).
- Inventory ticked: LIB-DATA-001..006, LIB-ING-003, LIB-ING-006. Partial (not ticked): LIB-ING-001, LIB-ING-002.

## Not done
- LIB-ING-001 gaps: WebP is lossless only (no q85), AVIF refused (open-questions 25, 26). No EXIF-orientation test fixture.
- LIB-ING-002: only ZIP/CBZ. RAR/CBR and 7z/CB7 not read (open-questions 27).
- LIB-ING-004/005/007/008/009 (PDF, EPUB, auto-crop, SubmissionKind, WASM pre-checks) untouched.
  There is no HTTP upload endpoint yet: `ingest_archive` is a library function for the submissions work (LIB-ING-008).
- LIB-RDR-012/013 (progress and bookmark APIs, SR_LIBRARY_UPDATE sync): tables exist, no endpoints. LIB-SOC-* untouched
  (no ratings, so no rating/trending sort; follows only store the shelf and notify flag, no notifications are sent on publish yet).
- Storage objects are not deleted when pages are replaced or a chapter/series is deleted (needs a GC job, like attachment_gc).
- No frontend (LIB-UI/HOME/BRW/SER/RDR/ADM).
- Reading the chapter list for large series returns all chapters (no pagination); fine for manga, revisit for 5000+ chapter series.

## How to resume
1. `createdb sr_p2_library` (or any scratch DB), `SR_TEST_DATABASE_URL=postgres://... cargo test -p sr-db --test library_repo -- --ignored --test-threads=1`.
2. Upload flow: call `sr_library::ingest::pipeline::ingest_archive(reader, storage, series_id, chapter_id, IngestLimits::default())`,
   then `LibraryService::replace_pages(actor, chapter_id, ingested.pages)`, then `set_chapter_published`.
3. Progress/bookmarks: add methods to `LibraryRepository` + `PgLibraryRepo` (tables ready), routes under `/library/chapters/{id}/progress`.
4. Gates: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo deny check`,
   `cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json` then `pnpm --filter @sideroom/api-client generate`.
