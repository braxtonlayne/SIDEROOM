# Handoff: feat/p22-cinema-submit (wave 22: RDY-MED-001, RDY-MED-002)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p22-cinema-submit`, branch `feat/p22-cinema-submit` (base `integration/phase22`). Local commits only. Target dir `p22-cinema-submit-target`, scratch DB `sr_p22_cinema` (dropped at the end). Migration: 1240 only.

## Resume here

Backend and screen are built. RDY-MED-001 is ticked (its Postgres proof ran). RDY-MED-002 is **not** ticked: Vitest ran, but the Playwright spec (`e2e/tests/cinema-submit.spec.ts`) has not run anywhere (no Docker/Garage on this machine); it waits for the first CI `live` run. Nothing is running now.

## Done

- `crates/sr-cinema/src/drafts.rs` (new): `DraftOps` (draft view, title edit keeping artwork, add/edit/delete draft episodes, file-to-episode placement, attach/replace a video) and the `UploadSessions` port. `service.rs` gains `update_title_keep_artwork`; `submissions.rs`: `cinema.episodes` may start with an empty list and takes episodes registered later as items.
- Migration `1240_cinema_submission_uploads.sql` + `sr_db::cinema_uploads::PgUploadSessions` (multipart upload id behind a draft video).
- `crates/sr-api/src/routes/submissions/{mod,cinema}.rs` (new): `/submissions/{id}/cinema/` `draft`, `title`, `episodes[/id]`, `files`, `uploads/{asset}[/parts|/complete]`. Parts come from `ListParts` (no ETags in the browser); the daily allowance is charged in `files`; byte limit is the media service's `cinema.upload.max_gb`; season/episode come from `sr_media::naming::parse_path` (sr-media untouched). Spec section 8.7 written.
- Web: `apps/web/src/cinema/submit/*` (list + new form, draft page, picker with naming tester, upload manager over the Cinema routes with its own IndexedDB database `sideroom-cinema-uploads`), route `/cinema/submit[/id]`, nav item, strings `p22-cinema-submit-1.ts`.

## Tests run

- Postgres (scratch DB, both env vars): `cargo test -p sr-api --lib submissions:: -- --include-ignored --test-threads=1` 4 passed: draft to published series, film with replacement, limits/names/ownership/abort, new episodes of a published series.
- `route_matrix` 6 passed (10 new rows, AUTHZ `author`), `openapi_names` passed (KNOWN_CLASHES empty); OpenAPI, api-client schema and MSW mocks regenerated.
- fmt and clippy `-D warnings` clean on sr-cinema, sr-db, sr-api, sr-server (all targets).
- Web: tsc, eslint, prettier clean; Vitest `submit.test.tsx` 12 passed; full run (--maxWorkers=2) 284 files, 2519 tests passed; build and bundle budget ok. sr-server `authz_matrix` (Postgres) 2 passed with the new routes; `cinema_submissions` (sr-db, Postgres) 5 passed; sr-cinema 20 passed.

## Not run / open

- Playwright spec: not run locally (needs Garage). It adds the bucket CORS rule itself (`sr-cli storage ensure-cors`). Its reviewer-proxy test is gated on `SR_E2E_FFMPEG=1` (the live job has no ffprobe, so the video stops at Failed or Checking). It must be added to the `live` step of `.github/workflows/e2e.yml` (not in my Split row; I did not edit it).
- Reviewer metadata edits are noted on the timeline, not written to the staff access log (the review routes already log decisions).
- Files outside my row touched, all small: `sr-server/src/app.rs` (wiring), `sr-api/src/routes/mod.rs`, `sr-db/src/lib.rs`, `cinema/routes.tsx`, `cinema/Shell.tsx` (nav + avatar menu now point at /cinema/submit).
- Owner question: should Library-style "My uploads"/"Upload" avatar menu entries in Cinema permanently point to /cinema/submit (done here)?
