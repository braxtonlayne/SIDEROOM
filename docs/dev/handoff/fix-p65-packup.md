# Wave 65 packup: SEC-AUD-176..177 (D-654 upload half, D-655 as shown to the developer)

Branch `fix/p65-packup` (base `integration/phase64`, merged with `fix/p65-pack` at 82eefdd0).

## Done (commits)
- `SEC-AUD-177` web half: `apps/web/src/developers/packageUpload.ts` (path choice, limit line, resumable parts through the Library's
  `UploadManager` with a new `package` store in the shared ledger `lib/uploadLedger.ts` (DB version 4, cleared and aborted at sign-out),
  status poll every 2 s, cancel, resume after a refresh), `tabs/Activities.tsx`, fragment `p65-packup-1.ts`, docs sentence
  (`bundleIntro`, no number), `api.ts` comment. Vitest: `developers/__tests__/packageUpload.test.ts` (11) and the updated `activities.test.tsx`.
- `SEC-AUD-176`: migration `2830_activity_package_uploads.sql`; `crates/sr-activities/src/package_upload.rs` (service `PackageUploads`,
  ports `PackageIngest` (impl for `ActivityService`) and `PackageQueue`); routes `crates/sr-api/src/routes/activities/uploads.rs`;
  jobs `crates/sr-server/src/package_jobs.rs` (`activities.package_ingest`, concurrency 1; `activities.package_sweep`, singleton id 966, daily 04:40 UTC);
  wiring in `app.rs`; `route_matrix.tsv` rows, authz matrix body override; tests `routes/activities/uploads_tests.rs` (Postgres, `--ignored`).
- Shots spec `e2e/tests/p65-packup-shots.spec.ts`.

## Wire shapes as built
As the contract. Additions: the 409 for a store that cannot sign parts carries field error code `PACKAGE_PARTS_UNAVAILABLE`
(`errors.upload._errors[0].code`); a `complete` with missing parts is 409 with `PACKAGE_PARTS_MISSING` and the text "Parts 2, 4 of 5 have not been
uploaded."; 429 carries `Retry-After: 3600`; `size` is accepted as a string or a number; state `aborted` is reported as `expired`.
`POST uploads` aborts the older open upload beyond two; the sweep expires `uploading` and `checking` rows past 24 h and deletes rows after 7 days.

## Dev stack
The memory store (`SR_STORAGE__ENDPOINT=memory:`) cannot sign parts (as for the Library), so `package-limits` answers `resumable: false` and the live page
uses the single request. The resumable states are shot with `page.route` fakes.

## Not done / notes
- Rust is only partly verified:  passes with my code (fixes after the first check: dynamic SQL, a name clash).  and  do not compile on the merge with fix/p65-pack 82eefdd0 because of pack's  ( takes 6 arguments, called with 4 at line 302). Waiting for pack's follow-up; my sr-api and sr-server code (routes, tests, jobs, app.rs wiring) is therefore NOT compiled yet.
- Not run: clippy, the Postgres tests in uploads_tests.rs (none watched fail or pass), the end-to-end test through the real  (to write once sr-api compiles), OpenAPI and generated-client regeneration, the Playwright shots and the visual check, the live click-through. The cargo target  still exists (delete at the end); scratch DBs not created yet.

## Owner questions
- The limit line says "Reviewed applications can upload more." because the contract's limits answer carries only the application's own limit, not the
  reviewed one. Add a `reviewed_max_bytes` to the answer if the owner wants the number shown.
- "Under the Activities module gate like their neighbours": the neighbours (`bundles`, `submit`) have no `require_enabled`, so none was added.
- sr-activities now depends on sr-library (for `PartStore`, `check_parts`): no cycle, but it pulls sr-library into sr-cli's build.

## Proposed threat-model rows
- Presigned part URLs: scope one object key (`activities/{app}/uploads/{id}.zip`) and one part number, lifetime 6 h (the Library's `PART_URL_TTL`), minted only for the owner of the application.
- An upload session of another application: every route filters on `(id, application_id)` and answers 404.
- Abandoned parts: expire after 24 h, aborted at the store by the daily sweep; at most 2 open uploads per application.
- Upload rate: 6 per application per hour (429); one package check at a time per node.

## Changelog sentence
Developers can upload Activity packages of hundreds of megabytes in resumable parts, with progress and a check that names the offending file.
