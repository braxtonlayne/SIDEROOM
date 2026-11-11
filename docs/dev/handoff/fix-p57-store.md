# Handoff: fix/p57-store (wave 57 store: D-588, D-589)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p57-store`, branch `fix/p57-store`. Not pushed. No migration was needed (range 2540-2549;
`2540` is only the reserved id of the one-off job row). No UI, route or OpenAPI change, so no visual check, no api-client regeneration and no
route-matrix change.

## Done

### SEC-AUD-058 (D-588): the sha256 is taken by a job
- `disable_item` (`crates/sr-moderation/src/takedowns.rs`) no longer reads anything. A storage-object item records the catalog's tag and size
  (as before when the read failed) and, in the same transaction, enqueues one `storage.takedown_digest` job (`byte_digest::digest_job`, 14
  attempts: with the queue's backoff, 5 s doubling to an hour, about four hours before the last attempt).
- `ObjectDigests`, `Moderation::with_object_digests`, `read_digests`, `DIGEST_READ_CAP` and `MountObjectDigests` are gone.
- `crates/sr-moderation/src/byte_digest.rs`: `digest_target` (None when the item is not `disabled`, has its sha256 already, or is gone: a
  restored item is skipped) and `record_digest` (one transaction, row locked): sha256 added when the item's size equals the bytes read;
  otherwise (stale catalog) the item takes the bytes read as size and its tag is dropped, as the inline read did; an item with no catalog size
  gets sha256 + size. Derived items of that item whose size equals the bytes read get the same sha256. The indexer is unchanged.
- `crates/sr-server/src/takedown_digest_jobs.rs`: the handler streams the object (`sha256_object`, no time cap, concurrency 1) and refreshes
  the job's lock every 5 minutes (the stale-job reaper returns jobs idle for 30). Transient error (backend error): `Err`, the queue retries.
  Definite answer (mount not found, object not found, invalid path) or a transient error on the last attempt (`job.attempts >= job.max_attempts`):
  ONE staff alert (new `StaffAlertKind::TakedownBytes`, "Takedown file check needs staff") naming `TK-<notice>` and the item; the job ends `Ok`; the
  item keeps tag and size.

### SEC-AUD-059 (D-589): one-off boot backfill
- `sr_db::jobs::schedule_once` (new): inserts a job row when none has the id, never re-arms a finished one. `takedown_digest_jobs::schedule`
  arms `storage.takedown_backfill` (id 2540, 90 s after boot) at every boot; it runs once. It is idempotent anyway (it only touches what is
  missing), so a pruned row would only re-run harmlessly.
- `Moderation::backfill_byte_takedowns`: (1) every `disabled` derived storage-object item with no `subject_account_id` gets the D-584 resolver's
  owner (`sr_db::storage_legal::owner_of_object`, now `pub`: mount owner; on a server bucket the uploader per the ledger, else the server owner),
  never a `subject_person_id` (no strike); then `notify_byte_matches` runs until it tells nobody (the removal DM once, `notified_at`, label tail
  stripped as built); an item whose owner cannot be resolved is left and returned. (2) a digest job for every `disabled` storage-object item with a
  size and no sha256 that has no queued or running digest job. Restored and rejected items are not selected.
- The job sends one staff alert (`TakedownBytes`) counting and naming (up to 10) the unresolved items.

## Tests run
- `cargo fmt --all -- --check`; `cargo clippy -j 4 -p sr-db -p sr-events -p sr-moderation -p sr-server --all-targets -- -D warnings` (clean).
- `cargo test -p sr-moderation --test storage_takedown -- --include-ignored` (13 pass), `cargo test -p sr-db --lib jobs -- --include-ignored` (4 pass), `cargo test -p sr-server --lib -- storage_ takedown_ --include-ignored` (29 pass). The scratch DB `sr_p57_store` was dropped.
- Postgres tests added or changed: `crates/sr-moderation/tests/storage_takedown.rs` (uphold queues one job per item and records tag + size only;
  `record_digest` branches: same size, size mismatch drops the tag, no catalog tag, second result ignored; restored item skipped; sha256 copied to
  derived items; the backfill: owners, uploader on a server bucket, unresolved left, no Person, DM once and without the other member's key/bucket,
  jobs enqueued once, restored notice untouched, second run does nothing), `crates/sr-server/src/takedown_digest_jobs.rs` (transient failure retried,
  one alert on the last attempt; definite answers alert at once; a good read records the sha256 and is not read twice; restored item skipped
  without opening the bucket; backfill alert once; the one-off is armed once), `crates/sr-server/src/storage_index_pg_tests.rs` (end to end through
  the job, the indexer, the DM and the restore), `crates/sr-db/src/jobs.rs` (`schedule_once`).
- `node --test scripts/threat-model.test.mjs`: pass (5).

## Docs
- T31 in `docs/15-threat-model.md` updated with the wave 56 text and the residual reworded for D-588/D-589 (Fable's S4 wording).
- Inventory rows SEC-AUD-058 and SEC-AUD-059 added after SEC-AUD-056 in `docs/inventory/21-feature-phase.md`.

## Not done / limits
- Notices upheld before this change whose objects are no longer readable keep tag-only matching after their digest job gives up (staff alert).
- The Admin UI has no special view for the new alert kind; it shows in the staff feed like other alerts (title "Takedown file check needs staff",
  link `/admin/moderation`).
- `StaffAlertKind::TakedownBytes` was added to `crates/sr-events/src/event.rs` and `crates/sr-server/src/staff_feed.rs`: the media builder may
  add variants in the same two places (additive conflict).

## Owner questions (for the Fable agent)
1. On a size mismatch (the object changed between the catalog and the job) the item takes the bytes read as its size and drops the tag, as the
   inline read did. A job that runs hours later could instead be reading a file the member replaced with something innocent; should a mismatch
   instead leave the item with tag and size only and alert staff?
2. A digest job that finds the object gone alerts staff at once (definite answer). Should an object gone from a mount that is only `disabled` (not
   deleted) wait for the mount to come back rather than alert?
