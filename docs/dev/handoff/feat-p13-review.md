# Handoff: feat/p13-review (phase 13, Submissions and Review)

Branch `feat/p13-review`, based on `integration/phase13`. Migrations 0890 to 0892.

## Done

- **H-04 (REV-RV-005)**: the `#review-queue` card embeds the cover or poster (signed CDN URL, omitted when none; `reading_series.cover_key` for `library.*`, `video_titles.poster_key` for `cinema.*`) and has a third button `sr:review-changes:<id>`. `press_review_button` takes a `ReviewPress` enum; Request changes changes nothing and answers `link: /admin/review?submission=<id>&changes=1`; the web page claims and opens the request-changes dialog. The image URL is signed with the attachment signer (24 h), so a card older than a day shows a broken image until it is edited.
- **REV-UP-001 / 002 / 003**: multipart sessions for Library chapter files (migration 0890 `reading_upload_multipart`, `sr_library::multipart`, `S3Multipart::list_parts`), admin and member routes `parts` and `DELETE`, server-side completion from `ListParts`. Web: `uploadManager.ts` (IndexedDB ledger, 4 parallel parts, retries, pause/resume/cancel, resume after refresh), `UploadDock`, `FolderImport` (drop or pick a folder, names read by the naming parser, chapters created and uploaded). Chapter files of 64 MB and more use the manager.
- **REV-UP-004**: `sr_library::ingest::inspect` (ranged reads; the same hardened reader as the ingest) and two `inspect` routes.
- **REV-UP-005 / 006 / ADM-CNT-005**: `sr_library::sources` (scanner, planner, lister), migration 0892 (`ingest_sources`, `ingest_files`), `/admin/ingest-sources` routes, job `library.sources` (id 895), `IngestSources` admin section with dry-run report and the naming tester.
- **REV-ED-005**: five providers over `sr_net::SafeClient`, off by default (`[lookup]` plus platform setting `review.lookup.enabled`), per-field accept in `SeriesMetaEditor`.
- **REV-ED-006 / REV-RV-003**: `SubmissionKind::duplicates` (Library: titles, year, cover dHash with migration 0891 `library_cover_hashes`; Cinema: titles), report and resolution routes, duplicate panel, no credit for merge or replacement.
- **REV-RV-002 (parts)**: video proxy player with scrub and frame steps (`/admin/review/submissions/{id}/media`), `ManifestDiff`, sandboxed `PackagePreview`.
- Route matrix rows (reviewed, no `?` left), OpenAPI, api-client and MSW mocks regenerated, new i18n fragment `p13-review-1.ts`.

## Still open (inventory boxes left unticked)

- **REV-ED-001**: the file tree with drag to reorder, rename and exclude. The form half exists (`SeriesMetaEditor`).
- **REV-ED-002**: folder import proposes chapters but accept is all or nothing, no per-row adjust.
- **REV-ED-003**: artwork crop (WASM) and frame-grab from the video proxy.
- **REV-ED-004**: a real validation summary panel; today the duplicate warning plus the server problems on submit.
- **REV-RV-002**: metadata edit-in-place; the manifest diff and package preview only render when an app or package kind puts `manifest` or `package.theme_css` in its preview, and those kinds do not exist in the review pipeline yet.
- **REV-CORE-007**: system DM and Staff Server card updates beyond G-68 (untouched).
- Not done on purpose: perceptual hashes of chapter pages (covers only); RAR over 256 MB is not listed by the pre-inspection; ingest sources are Library only (Cinema originals need a layout decision); the Cinema browser upload still uses its own staff path (no member Cinema upload UI exists, so the manager serves Library only).

## Tests run

- Rust: `cargo test` on sr-core, sr-storage (mock S3 `list_parts`), sr-library (multipart, inspect, duplicates, sources), sr-review (42 + 2: duplicates, credits), sr-cinema, sr-admin, sr-api (531 lib + route_matrix + all integration targets), `sr-server --lib -- --include-ignored --test-threads=1` (88, includes `staff_feed_pg_tests` for H-04 image present/absent and the changes link), sr-db `library_wiring`, `review_repo`, `ingest_sources` on a scratch Postgres (created and dropped).
- `cargo fmt --all --check`, `cargo clippy -D warnings --all-targets` on sr-core sr-storage sr-library sr-review sr-cinema sr-admin sr-db sr-api sr-server.
- Web: `tsc --noEmit`, eslint on touched files, `prettier --check src`, full vitest (`--maxWorkers=2`; the only failure is `channelPerms.test.ts`, which needs the wasm build, now built), `vite build` and `pnpm budget` (admin 296 KB of 600, library 222.5 KB of 600).

## Decisions

- Ingest sources need `PUBLISH_READING` (not Operator); safety comes from `[ingest] roots` (empty = local folders off) and the `imports/` prefix for storage.
- The browser never reads part ETags: the server completes from `ListParts`.
- A duplicate resolution is an internal timeline event (`duplicate_resolved`), not a table; the latest wins.
- Lookup is gated twice: operator config (which providers exist) and a platform setting (whether staff may use them). Both default to off.
- Cover hashes are filled lazily, 25 covers per duplicate check; a backfill job would be better on a big catalog.

## Owner questions

- Should ingest sources require `OPERATOR` instead of `PUBLISH_READING` because they read the server's file system?
- Should the review-card cover be signed for longer than 24 hours (or edited when it expires)?
- Which folders go into `[ingest] roots` on the production box (the Storage Box mount)?
- Browser uploads to storage need CORS `PUT` on the bucket for the app origin; confirm it is allowed on Garage.
