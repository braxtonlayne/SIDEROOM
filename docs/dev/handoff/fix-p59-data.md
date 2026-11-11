# Wave 59 data builder handoff (fix/p59-data)

## Done (one commit each, subject starts with the inventory ID)
- SEC-AUD-089: `jobs::claim` returns `locked_by`; `complete`, `fail` and `complete_many` fence on worker plus attempt. Callers in `runner.rs` and `sr-server/src/search_jobs.rs` updated.
- SEC-AUD-090: `put_profile` only updates active accounts; 0 rows is an error.
- SEC-AUD-091: advisory transaction lock per (app, guild) at the start of `install`.
- SEC-AUD-092: the cache-row UPDATE in `retry_job` is guarded by NOT EXISTS and its error is propagated.
- SEC-AUD-093: `thumb_base` checks the suffix on the original name.
- SEC-AUD-094: a panicking producer becomes an `Archive` error and the cleanup runs.
- SEC-AUD-095: any error after the draft chapter exists aborts the upload, deletes the chapter and unregisters the item.
- SEC-AUD-096: Seek and Rate keep `buffer_wait_until_ms` while auto-paused.
- SEC-AUD-097: guest-link revoke compares against the current host (`COALESCE(host_id, started_by)`) or the link creator.
- SEC-AUD-098 (D-608): `AuthStore::link_discord(.., now)` and `unlink_discord(.., now)` write the audit row in the same transaction. The route no longer discards the audit result, and a failure answers 500 with the change rolled back.

## Files outside the task list (needed for 098, flag for the integrator)
`sr-auth/src/store.rs` (trait signature), `sr-api/src/routes/auth/memory.rs` and `tests.rs` (in-memory store and callers), `sr-db/src/auth_store.rs`, `sr-server/src/userset_pg_tests.rs` (new `Job.locked_by` field), `sr-db/tests/p15_rest_b.rs`.

## Tests run
- fmt clean; clippy `-D warnings --all-targets` clean on sr-db, sr-server, sr-media, sr-library, sr-watchparty, sr-activities, sr-api and sr-auth.
- `--lib` suites of those crates pass.
- Postgres `--ignored` against `sr_p59_data`: sr-db `db`, `social_repo`, `apps_repo`, `media_repo`, `ingest_sources`, `auth_repo`, `p15_rest_b`, and the `db` tests of sr-watchparty and sr-activities all pass. The new tests are in those files.
- One parallel run flaked in `social_repo` (`guild_memberships_peers_and_mutual_friends`, an account insert in the helper). It passed on a serial rerun, so it is unrelated to the change.
- Not run: the new tests against main to confirm they fail there.

## Not done
Nothing from the task. The `naming.rs` test is in `naming/fixtures.rs` (the module's test file). The SEC-AUD-095 test uses a trigger to force `begin_upload` to fail, instead of an oversized `size_bytes`.

## Not done by me
Inventory rows (after SEC-AUD-065) are left for the integrator.

## Owner questions
None. Choice made: `put_profile` on an erased account returns a generic repo error (500 at the API) rather than a typed NotFound.

## Threat-model rows proposed
- Job completion is fenced by worker and attempt (089).
- Discord link and unlink are atomically audited (098).
