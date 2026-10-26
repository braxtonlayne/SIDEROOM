# Handoff: feat/p27-byos-core (wave 27, BYOS-MNT-001..003)

## Status
Phase A (day-one foundation) done and tagged `p27-day1`. Routes, connection test, CORS work and the CLI are not started.

## Done in phase A
- Migration `crates/sr-db/migrations/1400_storage_mounts.sql` (`storage_mounts`).
- `sr_storage::mounts` (`crates/sr-storage/src/mounts.rs`): types, `Sealer`, `SealedSecret`, `Secret`, `MountResolver`, `MountRepo`, `MemoryMountRepo`, `Mount::storage`.
- `sr_db::mounts_repo::PgMountRepo` (Postgres impl of both traits; the SQL lives in sr-db because sr-db -> sr-media -> sr-storage means sr-storage cannot depend on sr-db). sr-db now has sr-storage as a normal dependency (was dev-only).
- `impl sr_storage::mounts::Sealer for sr_db::sealed::SecretBox` (AAD = `mount:<id>` via `mount_aad`).
- Tests: 7 unit tests in sr-storage (`cargo test -p sr-storage --lib mounts`), `sealed::tests::mount_secrets_seal_with_the_row_id_as_aad`, Postgres `crates/sr-db/tests/mounts_repo.rs` (round trip, labels, ids, caps, concurrent cap race, update, soft delete); `secret_columns` passes; clippy -D warnings clean on sr-storage and sr-db.

## Resume here
Next, in order:
1. BYOS-MNT-001 rest: moka cache (60 s, bumped on write) around a resolver (not done; add `CachedMountResolver` in mounts.rs; moka is a workspace dep, add to sr-storage Cargo.toml); `crates/sr-api/src/routes/storage/mounts.rs` (user and guild groups, caps `byos.max_mounts_per_user` 3 / `byos.max_mounts_per_guild` 5, guild needs the `manage` grant: byos-scopes owns `scopes::may_manage`, until it lands use guild owner/ADMINISTRATOR/MANAGE_GUILD); endpoint SSRF validation on create/patch (https only, refuse private/loopback hosts in production, allow http://localhost for tests); JSON mapping (ids as strings, ISO timestamps, `endpoint`/`region` null when defaulted: store the provider default in the row); `sr-cli rotate-secret-key` re-seal of `storage_mounts` via `SealedSecret::reseal` (needs a `list_all`/`set_sealed` method on PgMountRepo); the no-secret-in-any-body test; route_matrix rows; OpenAPI regen (byos-ui does it too, coordinate).
2. BYOS-MNT-002: `POST .../test`, rows head_bucket, list, put, multipart, cors, list_buckets; add `record_test(id, at_ms, json, status)` to MountRepo/PgMountRepo; rate limit `abuse.storage_tests_per_hour` 20; provider guidance in an i18n fragment.
3. BYOS-MNT-003: `cors/check`, `cors/apply`, job `storage.cors_recheck` (grep for duplicate ids), `sr-cli storage ensure-cors --mount <id>|--all`; add `set_cors_ok(id, Option<ms>)` to the repo; presign gate 409 `CORS_NOT_READY` is byos-scopes' route, it reads `Mount::cors_ok_at`.
4. Doc 12 section 3.1 row + rotation bullet; Garage cases in `storage.yml`.
Scratch DB `sr_p27_byos_core` exists (drop at wave end). Env: CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p27-byos-core-target, CARGO_PROFILE_DEV_DEBUG=0, `-j 2`, one cargo at a time.

## Decisions / deviations
- `Mount` in Rust holds timestamps as unix ms and `last_test` as JSON text; the route layer maps to the wire shape.
- `key_hint` is the last four characters of the ACCESS KEY ID, not of the secret (showing secret characters weakens it, and the migration/roadmap define it as a hint beside the key id). byos-ui should show it as such.
- No new dependencies in phase A.
