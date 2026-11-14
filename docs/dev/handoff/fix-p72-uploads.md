# Wave 72, uploads builder (SEC-AUD-280..282; D-761, D-762, D-751)

Branch `fix/p72-uploads`. All three items done. No page, no route, no response shape, no migration: nothing for the commander to regenerate.

## Done (commits)
- `f1ddf5e6` SEC-AUD-282: `series.json` default edition label from the stored language form (D-751).
- `49dfbfe7` SEC-AUD-280 and 281: the hourly limit under one lock (D-761); an oversized or surplus part ends the upload (D-762). Also one small fix found by a wave 71 test (below).
- This handoff.

## Confirm-then-fix (all confirm tests were run on the base, in the same build, and watched fail)
| Item | Test (kept) | On the base |
|---|---|---|
| 282 | `sr-library` `ingest::series_json::tests::the_default_edition_label_uses_the_stored_form_of_the_language` | FAILED: label `Edition (eng)`, stored `en` |
| 280 | `sr-activities/tests/package_upload_db.rs::concurrent_starts_cannot_pass_the_hourly_limit` (5 prior rows, 4 starts at once, store `create` waits 200 ms) | FAILED (more than one start passed) |
| 281 (a) | `complete_ends_an_upload_whose_part_is_larger_than_announced` | FAILED (answered "parts missing", row stayed `uploading`) |
| 281 (b) | `complete_ends_an_upload_with_a_part_beyond_the_count` | FAILED |
| 281 (c) | `the_missing_parts_call_ends_an_upload_whose_part_is_larger_than_announced` | FAILED (new URL for part 1, the 9 bytes left) |
| 281 | `a_too_small_part_can_be_sent_again`, `the_oversize_check_leaves_a_checking_or_ready_upload_alone` | pass on base and after (guards) |
| 280 | `starts_of_two_applications_do_not_block_each_other`, `a_single_start_still_aborts_the_older_open_sessions` | pass on base and after (guards) |

After the change: all 15 tests of `package_upload_db` (`--ignored --test-threads=1`, scratch DB `sr_p72_uploads`) pass, the 9 library tests (`series_json`, `multipart`) and 3 `package_upload` unit tests pass, `cargo clippy -j 4 -p sr-activities -p sr-library --all-targets -- -D warnings` is clean, `cargo fmt` run on the two crates, `check-doc-links` ok. Not run (CI): sr-api route tests (`uploads_tests.rs`), the rest of the crates.

Pre-existing failure found on the base: wave 71's `two_complete_calls_at_once_both_succeed_and_queue_one_check` FAILED on the base run (`Storage("NoSuchUpload")`): a second `complete` that listed the parts after the first had assembled the object got a storage error (a real S3 answers the same for a completed upload). Fixed in `complete`: when `list_parts` fails and the assembled object is there, it goes on to move the row. The test passes now. This is outside the plan; it is small and in my file.

## SEC-AUD-280 (D-761): the lock
- `begin` now runs in one transaction: `SET LOCAL lock_timeout = '20s'`, `pg_advisory_xact_lock(1580, hashtext(<application id>))`, the count, the mark of older sessions, the store's `create`, the insert, `commit`.
- Key: two-key form, class `1580`, key `hashtext(app id as text)` (32-bit; a collision between two applications only means a needless wait). Taken before: only the music library's `pg_advisory_xact_lock(1550, ..)` and `(1551, ..)` use the two-key form; all other locks in `crates/` use the one-key bigint form, which is a separate key space.
- **Wait, not try-lock.** A start is rare and a developer's action; waiting gives the loser the true answer (429 if the winner used the last place, success if it did not), where try-lock would answer 429 "too many uploads" to a start that was only unlucky. Bound: the wait is capped by `lock_timeout` 20 s, after which the start answers 429 `RateLimited` (SQLSTATE 55P03).
- Connection held: one pool connection per start from `BEGIN` to the commit: the store's `create` (one network round trip) plus a few queries, normally well under a second; at most 20 s of waiting plus that. Waiting starts of one application each hold a connection too (at most a handful: only the application's developer can start, and `OPEN_UPLOADS`/`UPLOADS_PER_HOUR` bound the rest). Acceptable for a developer action; the 20 s cap bounds a hanging store.
- The older sessions are marked `aborted` inside the transaction (`UPDATE ... RETURNING`) and what they hold at the store is freed after the commit. So the lock holder needs no second connection (the old `self.abort` used the pool and could have starved it), and a start that fails leaves the older ones open (the transaction rolls back). `abort` does not take the advisory lock: no deadlock.
- Insert fails: the store's upload is aborted (as before). Commit fails: the same. `create` fails or the limit is hit: the transaction is dropped and rolls back.

## SEC-AUD-281 (D-762)
- Pure function `sr_library::multipart::oversized_part(stored, size, part_size, part_count) -> Option<OversizedPart>` (number 0 or beyond the count, or larger than the part's expected size), with unit test `a_part_larger_than_allowed_or_beyond_the_count_is_oversized`. `PackageUploads::end_if_oversized` runs it in `complete` (after the listing, before `missing_parts`, only when the object is not assembled yet) and in `status` (the missing-parts call). On a hit: one `warn!` (application id, upload id, part number, stored and allowed bytes; no file name), then the existing `abort` (state `aborted`, parts aborted, assembled object deleted), then the existing conflict "This upload has ended. Start a new one." (`status` of an ended upload afterwards answers state `expired` with no URLs, as for any ended upload). No new state, no migration. A too-small part stays "missing". `checking`/`ready` uploads return before the check, so are not touched.
- **The Library's own uploads (`sr-library` `uploads.rs`) have the same gap and are unchanged** (as the task says).
- **Still open:** the bytes exist from the `PUT` until the next `complete` or `GET` call; an upload that is never completed or asked about keeps them until it lapses (24 h) and the sweep aborts it. Binding the size into the signed URL waits for a proof against Garage (plan, "not done").

## SEC-AUD-282 (D-751)
`series_json.rs`: the language is normalised with `rules::language` (the shared `sr_core::language::normalise_tag`) first, and the label is made from the result. A language the rules refuse gives the same error. An `edition` typed by the uploader is never changed. The "ingest twice" check is made at this level: `eng` and `en` give equal `EditionInput`s (same language and label, so one edition key); there is no database ingest test in `sr-library` that I could extend in the time, and the edition key is (series, language, label), which is now equal.

## Docs updated (same commits as the code)
`docs/05-api-and-gateway.md` (package-upload row: the limit as one step under a lock, the oversized part ends the upload), `docs/specs/developer-platform.md` (the resumable-upload paragraph, same), `docs/specs/ingest-naming-conventions.md` section 2.4 (stored language, default label). Not mine and not changed: the web developer-portal text (`apps/web/src/developers/...`) says nothing about parts larger than announced, so no sentence is wrong.

## Proposed inventory rows (after SEC-AUD-273)
- SEC-AUD-280: The hourly limit of package uploads holds under concurrent starts: a start counts, marks older sessions, creates at the store and inserts under a per-application advisory lock in one transaction (D-761). Test `package_upload_db.rs::concurrent_starts_cannot_pass_the_hourly_limit`. Closed.
- SEC-AUD-281: A package upload whose store holds a part larger than announced, or numbered beyond the announced count, ends when the server first sees it (`complete` or `GET`) (D-762). Tests `complete_ends_an_upload_*`, `the_missing_parts_call_ends_*`, `multipart::tests::a_part_larger_than_allowed_or_beyond_the_count_is_oversized`. Open remainder: signed size in the part URL; the Library's own multipart uploads.
- SEC-AUD-282: The default edition label of a `series.json` with a language and no `edition` uses the stored form (D-751). Test `series_json::tests::the_default_edition_label_uses_the_stored_form_of_the_language`. Closes the minor note in `fix-p71-library.md` (item on `series_json.rs:121`) and its question 4.
- Also (unnumbered): a `complete` that loses the race to another `complete` and finds the store without the upload no longer fails (`two_complete_calls_at_once_both_succeed_and_queue_one_check`, which failed on the base).

## Proposed threat-model rows (numbered by the commander after T82)
- Package upload limit passed by concurrent starts (T67, abuse): a developer starting many uploads at once to pass the 6 per hour. Control: per-application advisory lock in one transaction with count and insert; `package_upload.rs::begin`; test `concurrent_starts_cannot_pass_the_hourly_limit`. Residual: a lock wait over 20 s answers 429.
- Bytes parked beyond the announced size (T67, storage): a part `PUT` larger than announced or beyond the part count sits in the bucket. Control: ended and aborted at the first `complete` or `GET`, logged without a file name; the daily sweep aborts the rest after 24 h. Residual: the bytes exist from the `PUT` until the next call or until the upload lapses; the signed URL does not bind the size; the Library's own uploads have the same gap.

## Changelog sentences (for reports)
- Developers: starting several package uploads at the same moment can no longer pass the six-an-hour limit.
- Developers: a package upload with a part larger than announced ends at once ("This upload has ended. Start a new one.").
- Library: a `series.json` with a language and no edition gets the label `Edition (en)` for `eng`; existing labels are not rewritten.

## Not done
Nothing from the task. The Library's own uploads, the signed-size URL and rewriting stored labels are out of scope by the plan.

## Owner questions
- A start that waits more than 20 s for another start of the same application answers 429 "too many uploads". Is that the right answer, or should it be a different text? (Chosen: 429, Discord-like.)
- Should the Library's own multipart uploads get the same oversize end next wave? (They do not today.)

## Clean-up
Scratch database `sr_p72_uploads` dropped (the test harness's own per-test databases are dropped by `TestDb`), cargo target `p72-uploads-target` and the log folder deleted, the heavy lock released, nothing of mine running.
