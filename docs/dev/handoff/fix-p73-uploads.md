# Wave 73 handoff: uploads (SEC-AUD-289, 290; D-771, D-772)

Branch `fix/p73-uploads` (commit: both items in one commit, see `git log`) (base `integration/phase73`, 1a0ae040). No route, response shape, migration or generated file changed: nothing to regenerate. No stack, no page, so no screenshots.

## Done

1. **SEC-AUD-289 (D-771): a start that lost the wait for another start is `Busy`.**
   - `PackageUploadError::Busy` (doc: "429: another start of this application holds the turn") is returned for SQLSTATE 55P03 in `PackageUploads::begin` instead of `RateLimited`. `up_err` (`crates/sr-api/src/routes/activities/uploads.rs`) answers it `TooManyRequests`, "Another upload for this application is starting. Try again in a moment.", retry-after 5,000 ms. `RateLimited` keeps its text and its hour. The comment at the lock timeout follows.
   - The wait is an `AtomicU64` on the service (default `START_WAIT_MS` = 20,000, `SET LOCAL lock_timeout = <ms>`), with `#[doc(hidden)] pub fn set_start_wait_ms` that only tests call.
   - Every `match` / comparison on `PackageUploadError` (`git grep "PackageUploadError::"`): the only exhaustive `match` is `up_err` (the one arm added); `package_upload_db.rs` and `uploads_tests.rs` compare single variants (`RateLimited`, `NoParts`, `PartsMissing`) and are unchanged in meaning; `concurrent_starts_cannot_pass_the_hourly_limit` expects `RateLimited` for losers that are over the limit (their count is 6, no lock timeout: the wait is 20 s and the store delay 200 ms) and passes unchanged.
   - Confirm test (kept): `a_start_that_waited_for_the_turn_is_busy_not_rate_limited` (`crates/sr-activities/tests/package_upload_db.rs`): another connection of the pool holds `pg_advisory_xact_lock(1580, hashtext(app))`, the start with a 1 s wait answers `Busy`, the hour's count is 0, the fake store `create` was never called; after the holder rolls back the same start succeeds and leaves no orphan. **Watched it fail on the base behaviour** (built once with the 55P03 arm still mapped to `RateLimited`: it failed at the `Busy` assertion), then passed with the fix.
   - Route test: `busy_says_a_moment_and_rate_limited_says_an_hour` (`uploads_tests.rs`, no database): Busy is 429 with the new text and 5,000 ms; RateLimited is 429 with "Too many package uploads. Try again in an hour." and 3,600,000 ms.
   - Docs: `docs/05-api-and-gateway.md` (package-upload row) and `docs/specs/developer-platform.md` (resumable upload paragraph): one sentence each.

2. **SEC-AUD-290 (D-772): the Library's resumable uploads end on an oversized or surplus part.**
   - `LibraryService::end_if_oversized` (private, `uploads_multipart.rs`), called after `list_parts` in `upload_parts` and in `assemble_multipart` (which `complete_upload` calls for a multipart row, so `complete_upload` itself is untouched; its single-`PUT` path and the Queued/Running/Done/Failed/Expired early returns are unchanged). On `oversized_part(...)` it: logs `warn!` (upload id, part number, stored and allowed size; no file name), aborts the store's upload (a failing abort is logged, as in `abort_upload`), calls `fail_upload`, returns `Conflict("A part of this upload was larger than announced. The upload has ended; start a new one.")`.
   - **State an ended upload gets: `failed`** (via `fail_upload` -> `finish_upload`, whose `WHERE status IN ('awaiting_upload','queued','running')` is the guarded transition: two calls at once end it once). Why `failed` and not `expired`: `fail_upload` stores the sentence in the row's `error` (the uploader's page shows it), writes the `library.chapter.ingest_failed` audit row and notifies the uploader; `expired` (what `abort_upload` gives) carries no message. The only code change outside `uploads_multipart.rs` is `fail_upload` becoming `pub(crate)` (`uploads.rs`, one word). A later call answers "this upload is failed; start a new one" (existing text).
   - A too-small part stays missing (re-sent; the upload then completes). A part number 0 or beyond the count is "surplus" (same end, same text).
   - Tests (kept; Postgres, `--ignored`, `crates/sr-db/tests/library_wiring.rs`, on `MemoryPartStore`): `an_oversized_part_ends_the_upload_when_its_parts_are_asked_for`, `an_oversized_part_ends_the_upload_at_complete`, `a_surplus_part_ends_the_upload_the_same_way` (both paths), `a_too_small_part_can_be_sent_again_and_the_upload_completes`; each ended case asserts the Conflict text, the row `failed` with the sentence in `error`, `MemoryPartStore::aborted()` holds the upload, and that `upload_parts` and `complete_upload` then answer "this upload is failed; start a new one". **Watched the three oversized/surplus tests fail on the base behaviour** (built with `end_if_oversized` returning early: all three failed at the `unwrap_err`, the too-small test and `a_multipart_upload_is_remembered_resumed_and_completed` passed), then pass with the fix. "An upload without a multipart row is untouched": covered by code (the check lives only where `get_multipart` returned a row; `complete_upload` for a single `PUT` never reaches it) and by the existing single-`PUT` tests, not by a new test. `check_parts`' own unit tests are unchanged.
   - Docs: `docs/specs/submissions-and-review.md` section 10 (the Library's resumable-uploads paragraph): one sentence. `docs/05-api-and-gateway.md` does not list the Library upload's conflicts (searched), nothing added.

## Still open for oversized parts
- The bytes exist from the `PUT` until the next `upload_parts`/`complete` call; an upload nobody asks about keeps them until it lapses and `run_gc` runs.
- The signed part URL binds no size (needs a Garage/S3 check; none on this machine, nothing downloaded).
- SEC-AUD-281's open remainder keeps "signed size in the part URL" only.

## What the Library's web uploader does with the Conflict
`apps/web/src/review/uploadManager.ts:199-205` (`api.parts` at resume) and `:252` (`api.complete`) both sit in the `try` whose `catch` (`:253-259`) sets the task to `failed` with the error message and settles it; there is no retry of a 409 (`apiFetch` retries only 429, `apps/web/src/api/client.ts:114`). So the Conflict shows as a failed upload with the sentence, not a loop. Not edited. (Unverified: how `UploadDock.tsx` renders `error`; I did not open it.)

## Report only (not edited)
- Developer portal: `apps/web/src/developers/**` has no string repeating "an hour" for 429 on package uploads (grep). `packageUpload.ts:82-94` passes `retries: 0` on every upload call, so the portal shows the server text of the 429 as it is: the new "Try again in a moment." reaches the developer, and nothing retries by itself (a developer-side retry on the 5 s retry-after would be a web change; not made).

## Not done
Nothing dropped. Not run locally (CI): whole-crate suites, `sr-api` library route tests (`routes/library/tests.rs` use `MemoryPartStore` but not oversized parts), route matrix.

## Proposed inventory rows
- SEC-AUD-289: a start of a package upload that could not get its turn within 20 s answers 429 "Another upload for this application is starting. Try again in a moment." with a 5 s retry-after (`PackageUploadError::Busy`), not the hourly limit's text. Closed by `a_start_that_waited_for_the_turn_is_busy_not_rate_limited` and `busy_says_a_moment_and_rate_limited_says_an_hour`.
- SEC-AUD-290: the Library's resumable uploads end (store abort, row `failed` with the sentence, 409) on a stored part larger than announced or numbered beyond the count, at `parts` or `complete`. Closed by the four Library tests above.
- SEC-AUD-281 remainder, reworded: only "the signed part URL binds no size" stays open (blocked on a Garage/S3 download); drop "the Library's own multipart uploads".

## Proposed threat-model changes
- T83: the residual "a lock wait over 20 s answers 429" becomes "answers 429 with 'Another upload for this application is starting. Try again in a moment.' and a 5 s retry-after; only the hourly limit says an hour".
- T84: remove "the Library's own uploads have the same gap" (closed by SEC-AUD-290); keep the bytes-until-next-call and unsigned-size residuals, now for both kinds.

## Changelog sentences (for status)
- A start of an Activity package upload that had to wait for another start of the same application now says "Try again in a moment" (retry after 5 seconds) instead of telling you to wait an hour.
- A resumable Library upload that receives a part larger than announced (or a part beyond the announced count) now ends with a clear message ("A part of this upload was larger than announced. The upload has ended; start a new one.") instead of staying open.

## Owner questions
1. The ended Library upload is `failed` (message shown, audit row `library.chapter.ingest_failed` and a notification to the uploader), not `expired` (silent). Default chosen because the uploader should read why; say if it should be silent.
2. Should the Library's web uploader offer "start again" on this message? Today it shows the failed state like any other failure.

## Clean-up
Scratch database `sr_p73_uploads` dropped; `p73-uploads-target` deleted; `.scratch/` (untracked, never committed) removed. No stash touched (`git stash list` not used).
