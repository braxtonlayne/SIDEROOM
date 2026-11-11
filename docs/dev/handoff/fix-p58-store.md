# Handoff: fix/p58-store (wave 58 store: D-591, D-592)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p58-store`, branch `fix/p58-store`. Not pushed. One migration (2560, a sequence). No UI, route or
OpenAPI change, so no visual check, no api-client regeneration and no route-matrix change.

## Done

### SEC-AUD-061 (D-591): a size mismatch writes nothing
- `Moderation::record_digest` (`crates/sr-moderation/src/byte_digest.rs`): when the item has a recorded size and it differs from the bytes read,
  nothing is written (tag, size kept; no sha256 recorded or copied to derived items) and it returns the new `DigestRecorded::SizeChanged { recorded, read }`.
  Same size: sha256 added as before. No recorded size: sha256 and size, as built. `Written` lost its `tag_dropped` field.
- `run_digest` (`crates/sr-server/src/takedown_digest_jobs.rs`): on `SizeChanged` it sends ONE `TakedownBytes` alert (`changed_detail`: notice, item, both
  sizes, "changed after the catalog saw it", "restore the item and uphold it again once the index has run") and ends `Ok` (`Outcome::Changed`): no retry.

### SEC-AUD-062 (D-592): a disabled mount waits
- `run_digest`: when the mount cannot be opened (`NotFound`) and `Moderation::mount_is_disabled` says the mount row exists (not deleted) and is
  `disabled`, the job ends `Ok` with no alert and no write (`Outcome::Waiting`). Object gone on an enabled mount, a deleted or missing mount, and invalid
  paths still alert at once.
- `sr_db::storage_legal::enqueue_mount_digests(conn, mount_id)`: the D-589 part 2 selection scoped to one mount, inserted in SQL. Called, in the same
  transaction as the change, by `PgMountRepo::update` and `PgMountRepo::record_test` when the mount was `disabled` and the new status is `active`, by
  `PgMountRepo::soft_delete` when the mount was `disabled`, and by `storage_legal::erase_for` (account erasure soft-deletes the member's mounts) for
  each mount that was `disabled`. The row is locked first, so two concurrent changes cannot both see `disabled`.
- Migration `2560_takedown_digest_job_ids.sql`: sequence `takedown_digest_job_seq` starting at 2^40 (above the singleton limit, far below any Snowflake)
  for the job ids, because the repository has no id source. The job kind and attempts constants now live in `sr_db::storage_legal`; `byte_digest`
  re-exports them.

## Tests (Postgres-backed; scratch DB `sr_p58_store`, dropped afterwards)
- `crates/sr-moderation/tests/storage_takedown.rs`: the `record_digest` stale-size branch now leaves tag, size and sha256 untouched and the item still
  waiting for a digest.
- `crates/sr-server/src/takedown_digest_jobs.rs`: `a_file_of_another_size_writes_nothing_and_alerts_once`,
  `a_disabled_mount_ends_the_job_quietly_but_a_gone_mount_or_object_alerts`, `re_enabling_a_disabled_mount_queues_its_waiting_items_once` (update path;
  not for failing/disabled, other mounts, items with a sha256, or twice; then the job records the sha256), `a_passing_test_of_a_disabled_mount_queues_its_waiting_items`
  (`record_test` path), `deleting_a_disabled_mount_yields_the_one_alert` (delete queues once, the job alerts once; an enabled mount's delete queues nothing).
- `node --test scripts/threat-model.test.mjs`: pass (5).
- RESULTS_PLACEHOLDER

## Docs
- T31 in `docs/15-threat-model.md`: residual now says a changed file is never fingerprinted (D-591) and that a switched-off mount waits and is queued again (D-592).
- Inventory rows SEC-AUD-061 and SEC-AUD-062 after SEC-AUD-060 in `docs/inventory/21-feature-phase.md`.

## Not done / limits
- `erase_for`'s enqueue has no dedicated test (it is the same function the delete path tests).
- Only `disabled` -> `active` re-queues (the row's wording). A disabled mount that goes `failing` directly (not through the API today) would not.
- A `delisted` mount that was `disabled` before and is relisted returns to `disabled`: nothing is queued (not a re-enable).

## Threat-model rows to propose
No new row: T31's residual was reworded (above).

## Owner questions (for the Fable agent)
1. A disabled mount whose items wait: should staff see (e.g. in the takedown detail) that an item's sha256 is waiting on a switched-off mount? Today it is silent by the row's design.
2. Account erasure soft-deletes a member's mounts; for a disabled one this now queues the digest jobs, which find the mount gone and alert (one alert per waiting item), as D-592 specifies for a deleted mount. Fine for erasure too?
