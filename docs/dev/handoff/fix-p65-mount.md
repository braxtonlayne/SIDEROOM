# Wave 65, mount builder: SEC-AUD-178..180 (D-644, D-645, two lag fixes)

Branch `fix/p65-mount` (base `integration/phase64`). No migration, no route added, no response shape changed, no UI. `docs/inventory/21-feature-phase.md` is not edited.

## Done (all three items)

Commits (see `git log`): `SEC-AUD-180`, `SEC-AUD-178`, `SEC-AUD-179`, then this handoff.

### SEC-AUD-180: two subscribers survive a lagged bus
- `integration_policy::spawn_subscriber` uses `subscribe_lagging`; on a gap it logs a warning with the count and calls `refresh` once, as for a matching `FeatureSettingsChanged`.
- `staff_feed::spawn` uses `subscribe_lagging` through a new `pump` loop (a `FeedSink` trait: `event`, `gap`; `StaffFeed` implements it, so the loop is testable without a database).
- **What the staff feed does on a gap: it says so, it cannot recover.** Alerts and feed events are stored only when the consumer turns them into cards (`record_alert` runs inside the consumer), so a lost event leaves no row to re-read; open reports/cases/takedowns have no "posted" marker that the feed owns (only review submissions have `review_queue_cards`, packages and alerts have none), so a partial re-read would have been a guess. On a gap it logs a warning with the count (every time) and posts one card "Some staff alerts may have been missed (the event queue was full). Check the queues in Admin." to `#alerts`, at most once per 10 minutes (`GapThrottle`), with an "Open in Admin" button to `/admin/system`.

### SEC-AUD-178: D-644
- `Mount::response_type_failed()` (`sr-storage/src/mounts.rs`): true when the stored test JSON has a row `id == "response_type"` with `status == "fail"`. Not failed: no test, unreadable JSON, no such row, pass/warn. **Finding:** the probe has no `skip` status; a skipped probe (the file could not be written) is stored as a `fail` row whose message starts with "Skipped"; the reader treats that one as not failed (the mount's other rows say why), so a read-only key does not lose its links. A timed-out, refused or unanswered `response_type` row IS read as failed (the recheck re-probes it daily and clears it on a pass).
- Refusal: nothing refuses a `failing` mount today (`failing` is a flag), so the API paths answer **409 with the new stable code `MOUNT_NOT_SAFE`** (`storage/mod.rs::mount_not_safe`). `/cdn/mount` answers a plain 409 (it is a redirect link, not JSON). Documentation on the 409s of `POST /storage/mounts/{id}/presign` and `GET /shelves/{id}/objects/{hash}/url` changed: **this changes the generated `openapi.json` description of those two 409 responses; I did not regenerate it, leave it to packup.**
- `MountStorages::response_type_failed(mount_id)` (new, for `/cdn/mount`, which has no `Mount` in hand).

Presign call sites found (`grep` for `presign`, `presign_get_as`, `PresignMethod`, `cors_ok_at`):

| site | kind | guarded |
|---|---|---|
| `cdn/mount.rs::redirect` (`/cdn/mount`) | client GET | yes (409 plain) |
| `storage/objects.rs::presign` (GET) | client GET | yes (409 `MOUNT_NOT_SAFE`; PUT not) |
| its delegates `guilds/scrapbook.rs:1075`, `music/tracks.rs:354`, `voice/jukebox.rs:902` | client GET (call `objects::presign`) | yes, through it |
| `storage/shelf_access.rs::presign_object_url` (shelf object URL, and the showcase through it) | client GET | yes |
| `packages/profiles.rs::showcase_url` | client GET | yes (404 like its CORS check, plus the guard in `presign_object_url`) |
| `storage/shelves.rs::Signer::url` (shelf artwork) | client GET | yes (no URL) |
| `guilds/scrapbook.rs::mount_reach` (the `playable` hint of a copy) | hint | follows the guard (not playable) |
| `storage/uploads.rs` (multipart parts), `messages/attachments.rs:195`, `cinema/service.rs:1292`, `library/uploads.rs:431`, `media/output_storage.rs`, `objects::presign` PUT | upload / PUT | no |
| `sr-media/src/mount_job.rs:229`, `worker_api.rs:900` (a transcoder worker's source URL), `sr-library/ingest/inspect.rs`, `sr-preflight/checks.rs:237`, `server/storage_jobs.rs`, `scrapbook_jobs.rs` | server-only (workers) | no |
| ring playback `userfeat/rings.rs` (`for_serving`) | the server streams the bytes with its own headers, no presign | no |

No client GET path outside D-644's list was found. The transcoder worker's presigned GET (`worker_api.rs:900`) goes to the member's own worker program, classed as a server worker per D-644.

### SEC-AUD-179: D-645
- `probe::check_response_type` (new; reuses `put_row`/`response_row`, so write, read back, signed GET with overrides and delete are the connection test's own code) returns `Honoured`, `Ignored` (the provider answered without the overrides) or `Unfinished` (write refused, no answer, timeout, refused override read: learn nothing).
- `recheck.rs`: the job is now two halves. CORS half as before, except that a rewrite of the CORS row keeps the stored `response_type` row (it used to replace the whole document) and a CORS recovery of a job-marked mount stays `failing` while a `response_type` row is failed. Response half (after the CORS half, whatever it found, so a mount whose CORS rules cannot be read is still probed): for a mount with no row or a failed one, one probe (20 s budget); `Ignored` merges the failed row into the stored test (other rows kept), sets `source: cors_recheck` and `failing`; `Honoured` merges the passed row and makes a mount that this job set `failing` `active` only when no other failed row remains (so a CORS failure stays); `Unfinished` changes nothing. A mount with a passed row is not probed. The endpoint guard is the one already used.
- `sr-storage` gains the `serde_json` dependency (workspace version, already in the lockfile; `Cargo.lock` has the one-line edge).
- Known limit, an owner question below: a mount whose failing status came from a user-pressed Test (no job marker) is not made `active` by the recheck when the provider is fixed; the owner presses Test.

## Not done
- No web change: the UI still lists six test row ids (`apps/web/src/settings/user/storage/types.ts`), as noted in the wave 63 handoff; the "owner sees why in Settings" part relies on the stored row message shown there.
- Tests for the delegate routes (scrapbook, music tracks, jukebox) and the profile showcase were not written: they call the guarded `objects::presign` / `presign_object_url` and their harnesses are heavy; one test per guard core is in `uploads_tests.rs` and `cdn/mount_tests.rs`.
- No PUT test (no emulator-save fixture, as in wave 63).

## Tests (heavy lock, `-j 4`, own target)
New, all pass:
- sr-storage lib: `a_stored_failed_response_type_row_is_read_as_failed`, `a_missing_row_or_unreadable_test_is_not_failed`, recheck helper tests.
- `s3_probe_mock`: `a_mount_without_the_row_gets_it_on_the_daily_run`, `a_provider_that_ignores_overrides_becomes_failing_and_presigns_no_get` (through the reader), `a_fixed_provider_recovers_on_a_later_run`, `a_probe_that_cannot_run_changes_nothing`, `a_mount_with_a_passed_row_is_not_probed`, `a_cors_failure_is_not_cleared_by_a_passed_response_type` (the existing recheck test still passes).
- sr-api: `a_mount_link_is_refused_on_a_mount_with_a_failed_response_type_row` (cdn), `the_storage_presign_get_is_refused_on_a_mount_with_a_failed_response_type_row`, `the_shelf_object_url_is_refused_on_a_mount_with_a_failed_response_type_row`, `shelf_artwork_has_no_url_on_such_a_mount`, `an_upload_still_starts_on_such_a_mount`, `a_mount_failing_for_another_reason_still_presigns`.
- sr-server: `a_lagged_policy_subscriber_rereads_the_policy`, `a_lagged_staff_feed_recovers_or_says_so`, `the_missed_alerts_card_is_posted_at_most_once_per_ten_minutes`.
- Watched fail without the change: see the end of this file.
- `cargo clippy -j 4 -p sr-storage -p sr-api -p sr-server --all-targets -- -D warnings`: see the end of this file. `rustfmt` run on my files.

## Proposed inventory rows (after SEC-AUD-169)
- **SEC-AUD-178** [BE] A storage mount whose stored connection test has a failed `response_type` row hands no presigned GET to a client: `/cdn/mount`, the storage presign GET and its delegates, the shelf object URL and the profile showcase answer 409 `MOUNT_NOT_SAFE` (404 on the showcase), shelf artwork has no URL. Uploads and server-only URLs are unaffected. D-644. Handoff `fix-p65-mount.md`.
- **SEC-AUD-179** [BE] The daily mount recheck also runs the `response_type` check on mounts whose stored test has no such row or a failed one: a provider that ignores the overrides makes the mount `failing`, a fixed one recovers; an unfinished probe changes nothing. The CORS half keeps the row. D-645.
- **SEC-AUD-180** [BE] The integration-policy subscriber and the staff feed use the lag-aware subscription: a gap re-reads the policy; the feed logs the count and posts one "alerts may have been missed" card per 10 minutes. D-628 applied, p63 answer 12.

## Proposed threat-model rows
- A provider that ignores response overrides: detected by the connection test (D-643) and, for mounts tested before wave 63 or never retested, by the daily recheck (D-645); a mount with a failed row hands no client link (D-644). Residual: links already handed out live until they expire; a mount last tested clean whose provider changes behaviour is not re-probed (a passed row is never probed again); a mount that this job did not mark is not auto-cleared.
- A lagged event-bus subscriber: the policy subscriber re-reads on a gap (no stale unfurl/GIF policy until restart); the staff feed cannot recover lost alerts and says so (one card per 10 minutes). Residual: a lost staff alert is never replayed.

## Changelog sentence
"A storage connection that cannot keep files from opening as pages no longer hands out links; its owner sees why in Settings."

## Owner questions (defaults taken)
1. Should the recheck also lift a `failing` that came from a user-pressed Test when the provider has been fixed and the probe passes? Default: no (only a failing the job set), as D-645 words it.
2. A `response_type` row that failed only because the probe timed out or was refused (not "ignores") is read as failed by D-644. Default: yes (conservative, self-heals daily). A "Skipped" row is not.
3. The staff-feed gap card has no recovery. Default: notice only.

## Cleanup
Scratch DB: none was created (all tests are in memory). `p65-mount-target` is deleted at the end (see below).

## Watched failing, clippy
- With `Mount::response_type_failed` forced to `false` (guard off), the four refusal tests (`/cdn/mount`, storage presign GET, shelf object URL, shelf artwork) FAILED (0 passed, 4 failed); with the change they pass. The two negative tests (`an_upload_still_starts...`, `a_mount_failing_for_another_reason_still_presigns`) pass either way by design.
- With the policy subscriber's gap arm made to do nothing (the old behaviour), `a_lagged_policy_subscriber_rereads_the_policy` FAILED; restored, it passes.
- `a_lagged_staff_feed_recovers_or_says_so` has no base to fail on (the base has neither `pump` nor a gap hook); it asserts the gap is reported with its count (3) and that the surviving events are still handled.
- The recheck tests are new behaviour (no base to run against).
- `cargo clippy -j 4 -p sr-storage -p sr-api -p sr-server --all-targets -- -D warnings`: clean.
- `p65-mount-target` deleted; no scratch DB was created. `.scratch/` in the worktree is untracked and not committed.
