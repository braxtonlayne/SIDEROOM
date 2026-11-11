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
- `cargo fmt --all -- --check`; `cargo clippy -j 4 -p sr-db -p sr-moderation -p sr-server --all-targets -- -D warnings` (clean); `cargo test -p sr-moderation --test storage_takedown -- --include-ignored` (13 pass); `cargo test -p sr-db --test mounts_repo --test mount_attachments -- --include-ignored` (4 pass); `cargo test -p sr-server --lib -- takedown_ storage_ --include-ignored` (all pass after fixing the alert text whitespace; the 9 `takedown_digest` tests re-run green).

## Second pass: D-595, D-596, D-597 (SEC-AUD-065, commit 8dc4e087)
- **D-597:** the sequence is `jobs_sql_id_seq` (migration 2560 edited in place); `sr_db::jobs::SQL_ID_SEQUENCE` holds the name and documents it as the one id
  source for jobs enqueued from SQL; `enqueue_mount_digests` uses it; test `the_sql_id_sequence_starts_at_the_singleton_limit` (sr-db, jobs).
- **D-596:** `run_test` (via the pure `status_after_test`, `crates/sr-api/src/routes/storage/mount_ops.rs`) passes no status for a `disabled` mount (unit test).
  `PgMountRepo::update` and `record_test` queue the waiting digests when a `disabled` mount becomes `active` or `failing`; `soft_delete` and `erase_for`
  also treat `delisted` with `delist_prior = 'disabled'` as switched off. Tests (sr-server, `takedown_digest_jobs`): disabled -> failing queues once; relist
  back to disabled queues nothing; delete of a delisted-from-disabled mount gives one alert; `erase_for` queues once per waiting item and each job alerts once;
  `record_test` path; the old "failing queues nothing" assertion was changed. Correction to the first pass: a disabled mount CAN go `failing` through the API
  (the Test button did it); that no longer happens.
- **D-595:** `TakedownItem.digest_state` (`recorded`, `pending`, `waiting_mount`, `tag_only`; absent for other kinds and non-disabled items) is computed in
  `ITEM_SELECT` (`crates/sr-moderation/src/takedowns.rs`), nothing stored. `openapi.json`, the generated schema and the MSW handlers regenerated (route matrix
  unchanged: no new route). `TakedownsTab` shows a short line per item (fragment `apps/web/src/i18n/messages/en/p58-store-1.ts`). Tests: `the_notice_detail_says_what_the_fingerprint_of_each_item_is`
  (one assertion per state, plus a deleted mount and a restored notice), vitest `says what the fingerprint of each delisted file is (D-595)`.
- **Visual check:** `e2e/tests/p58-takedown-digest-shots.spec.ts` (opt-in, `SR_E2E_SWEEP=1`) answers the notice endpoints in the page (a real storage takedown needs a
  bucket) with five items covering every state, as the Owner against a real stack (API 8362, Vite 5362, scratch DB dropped). Shots in
  `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p58-store/takedown-detail-{desktop,phone}-{lounge-light,lounge-editorial}.png`: I looked at desktop and phone: the lines read
  correctly and wrap on the phone, no overflow. The admin area rendered identically in both themes (the Editorial pass is not visibly darker here, so a distinct dark
  check was not possible with this setup).
- Gates: clippy clean (sr-db, sr-moderation, sr-api, sr-server); `cargo test`: storage_takedown 14, sr-db jobs 5 + mounts 4, sr-api mount_ops 1 + route_matrix 6 + openapi_names 2, sr-server `takedown_ storage_` 37; vitest takedowns + catalog;
  web tsc, eslint, prettier, `i18n-audit`, `check-css-tokens`; `mocks:check`. Not run: `playwright --list`, `check-doc-links`.

## Docs
- T31 in `docs/15-threat-model.md`: residual now says a changed file is never fingerprinted (D-591) and that a switched-off mount waits and is queued again (D-592).
- Inventory rows SEC-AUD-061, 062 and 065 after SEC-AUD-060 in `docs/inventory/21-feature-phase.md`.

## Not done / limits
- `erase_for`'s enqueue has no dedicated test (it is the same function the delete path tests).
- A `delisted` mount that was `disabled` before and is relisted returns to `disabled`: nothing is queued (not a re-enable).

## Threat-model rows to propose
No new row: T31's residual was reworded (above).

## Owner questions (for the Fable agent)
None open (questions 1 and 2 were answered by D-595 and D-596).
