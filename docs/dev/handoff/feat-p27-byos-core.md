# Handoff: feat/p27-byos-core (wave 27, BYOS-MNT-001..003)

## Status
Phase A (tag `p27-day1`) plus the rest of BYOS-MNT-001, 002 and 003 are built and tested locally. Open: regenerating `packages/api-client/openapi.json` and the generated client and mocks (needs a built server and `pnpm install`; left to the integrator or byos-ui), the `authz_matrix.rs` groups (byos-scopes owns that file), the two `byos.max_mounts_*` settings, and the Garage live run (runs in CI only).

## Done
- Migration `crates/sr-db/migrations/1400_storage_mounts.sql`.
- `sr_storage::mounts`: `Mount`, `MountOwner`, `Provider`, `MountStatus`, `Sealer`, `SealedSecret`, `Secret`, `MountResolver`, `MountRepo`, `MemoryMountRepo`, `Mount::storage`. Additive since `p27-day1`: `MountRepo::{record_test, set_cors_ok, list_enabled, list_all, set_sealed}` (memory and Postgres).
- `sr_storage::mount_cache::CachedMountResolver` (moka, 60 s, `invalidate(id)`; misses are not cached).
- `sr_db::mounts_repo::PgMountRepo` (cap under an advisory lock), `impl Sealer for SecretBox`.
- Routes (`crates/sr-api/src/routes/storage/{mod,mounts,mount_ops}.rs`, tests in `mounts_tests.rs`): `GET|POST /users/@me/storage/mounts`, `PATCH|DELETE .../{mount_id}`, the same under `/guilds/{guild_id}/storage/mounts`, `POST .../{mount_id}/test`, `POST .../{mount_id}/cors/check`, `POST .../{mount_id}/cors/apply`. Shapes as byos-ui's `types.ts`, with the clarifications the commander approved (ids as strings, ISO times, `key_hint` = last four of the access key id, stable 409 codes `MOUNT_CAP_REACHED` and `MOUNT_LABEL_TAKEN` at `errors.mount._errors[0].code`). Another owner's mount is 404, a server member without `MANAGE_GUILD` is 403, bots and bearers 403 (`auth::policy::bot_denied` covers `/users/@me/storage*` and `/guilds/*/storage/*`).
- `MountsDeps` is registered in `sr-server/src/app.rs` (`byos_mount_repo`, `byos_mount_sealer`; byos-instance names its own `mount_repo`/`mount_sealer`: unify at integration). Caps are the constants 3 and 5 for now.
- Connection test: `sr_storage::probe::run` (six rows, 20 s budget) over `S3Admin` (now a general signed client: `head_bucket`, `list_one`, `put/get/delete_object`, `create/abort_multipart`, `list_buckets`, `preflight_allows`). Result stored as `last_test`; any failed row sets `failing`; a clean run on a `failing` mount sets `active`. Rate limit `abuse.storage_tests_per_hour` (20; `Abuse::StorageTests`, catalog entry, doc 05 row).
- CORS: `sr_storage::cors_check` (`check`, `apply`, `compare`, `manual_steps`): ok, missing, different, manual. A service without the CORS API (501/405) is verified with a live preflight before falling back to `manual`. The required rule is `CORS_METHODS` + `content-type` + exposed `ETag, Content-Range, Accept-Ranges, Content-Length` for `server.public_url`. `ok` stamps `cors_ok_at`, everything else clears it; editing a connection field of a mount clears it too.
- Daily job `storage.cors_recheck` (singleton id 134, 05:40 UTC): `sr_storage::recheck::recheck_all`, read-only; marks `failing` with a reason, recovers its own marks.
- `sr-cli`: `rotate-secret-key` also re-seals mounts; `storage ensure-cors --mount <id>|--all` (stored mounts, one origin; no flags keeps the old config-bucket behaviour); `storage set-instance-mount` (repoints mount 1 at `storage.*`, seals the new secret, clears `cors_ok_at`).
- Docs: doc 12 section 3.1 (stored credential row, rotation text), doc 05 abuse row. CI: `storage.yml` step running `crates/sr-storage/tests/s3_mounts_live.rs` against Garage (not run locally: no Garage on this machine).

## Tests run (all local, one cargo at a time, `-j 2`)
- sr-storage: lib (75), `s3_probe_mock` (11: every probe row, the warning, budget, each CORS state, apply then check, manual, recheck), `s3_cors_mock` (3, unchanged and green).
- sr-db: `mounts_repo` Postgres suite (3), `secret_columns`, `sealed` unit tests.
- sr-api: `storage::` (6 route tests: CRUD, caps, cross-tenant 404, guild 403, bots, validation and SSRF refusals, no-secret-in-any-body, test, CORS), `route_matrix` (6), `abuse_limits` (4), `openapi_names` (2).
- sr-cli bin (27), sr-admin (62), sr-server `admin_jobs` (5: id and kind uniqueness).
- clippy `-D warnings` and fmt on sr-storage, sr-db, sr-api, sr-cli, sr-server, sr-admin.

## Resume here
Remaining, in order:
1. Settings `byos.max_mounts_per_user` (3) and `byos.max_mounts_per_guild` (5) in `sr-admin/src/settings.rs`, read in the composition root (the integrator checks duplicates). The constants work meanwhile.
2. Regenerate OpenAPI: from bash `cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, then `pnpm --filter @sideroom/api-client generate` (and `mocks:generate` if it exists) after `pnpm install --offline --frozen-lockfile`. Coordinate with byos-ui.
3. When byos-scopes' `scopes::may_manage` lands, swap it into `mounts::guild_scope` (today: `MANAGE_GUILD`, owner and `ADMINISTRATOR` bypass included).
4. A Postgres-backed route flow test (the sr-api tests use `MemoryMountRepo`; the repo itself has Postgres tests).
5. When Garage runs in CI: read the `s3_mounts_live` result; if the preflight assertion is red, check Garage's OPTIONS handling.
Scratch DB `sr_p27_byos_core` exists: drop it when the wave is finished.

## Decisions and deviations
- The Postgres repo lives in sr-db (crate cycle), the traits in sr-storage.
- `key_hint` is the last four of the ACCESS KEY ID (approved).
- 409 codes are field-error codes under `errors.mount` on `SrConflict` (900010), not new numeric codes, to avoid touching `sr-core`.
- `S3Admin` is path-style only (`Mount::storage` honours the `path_style` flag, the admin calls do not). Fine for Garage, MinIO, Wasabi, R2, B2; AWS buckets in newer regions may need virtual-hosted style (untested).
- SSRF: the endpoint is checked on create/patch and again before every test or CORS call with `sr_net::resolve_validated`; the S3 client then resolves the name itself, so a DNS rebind between check and call is not closed.
- Providers untested here: R2 (region `auto`), B2 (the 501 path is covered by the mock only), Wasabi, AWS S3. Garage is covered in CI only.

## Open owner questions
None new.

## Added after byos-scopes finished
- `sr_storage::path::{safe_key(key, allow_folder), KeyError, MAX_KEY_BYTES}` has landed. It has the same rules as scopes' `browse::check_key`, so the integrator can swap one for the other. It does NOT normalise NFC: that needs `unicode-normalization`, which the wave may not add. Callers that own the dependency normalise first.
- `Mount::storage` now sets a 5 s connect timeout, an 8 s request timeout and one retry (10 s retry window), so a dead endpoint fails in seconds.
- Route AUTHZ cells: my 14 mount rows use `self` (user scope) and `membership` (guild scope). Scopes added `mount_owner` and `storage_scope` to `AUTHZ_VALUES` on its branch; at merge, switch the mount rows to `mount_owner` if the integrator wants one name for them.
