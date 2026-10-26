# Integration handoff: wave 27 (BYOS core) on `integration/phase27`

Integrator: Opus 5.5, 2026-10-26. Base `5e11943d`. Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p27-integ`,
`CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p27-integ-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`,
`CMAKE` = the VS 2019 Build Tools copy
(`C:/Program Files (x86)/Microsoft Visual Studio/2019/BuildTools/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe`,
needed by opusic-sys in sr-sdk for clippy and tests). Nothing pushed, no PR, nothing merged to main. No server was
started. node_modules and the wasm build are in place in the worktree.

## Resume here

Stopped at 18:45 on 2026-10-26 (laptop off at 19:00), at a clean checkpoint: everything is committed.

1. **Merges: all four done** (table below), with the merge work, the OpenAPI / api-client / MSW regeneration and
   two merged-tree test fixes.
2. **Gates done and green:** fmt, clippy workspace `-D warnings`, sr-storage, sr-db (mounts_repo, scopes_repo,
   social_repo on Postgres), sr-api lib + route_matrix + openapi_names, sr-admin, sr-cli, sr-server lib
   (admin_jobs, instance_mount incl. Postgres), authz `storage_scopes_hold_over_http`, cargo deny --offline, web
   typecheck, lint (eslint + prettier --check), i18n-audit (0), vitest on src/settings, src/storage and src/lounge,
   web build + bundle budget, check-css-tokens, mocks:check.
3. **Gates still to run:** `every_route_enforces_its_authz_row` with `SR_AUTHZ_GROUPS=5` (the "rest" group, where
   every storage route falls; groups 0 to 4 have no storage route) on the merged tree; then the full workspace
   `cargo test` (about 30 min; skipped today on the commander's word). After they pass, tick BYOS-SCP-002 and
   BYOS-INS-001 in `docs/inventory/21-feature-phase.md`.
4. **Scratch DB `sr_p27_integ` still exists** (created 18:13; drop it at the end of the wave). Env for the Postgres
   suites: SR_TEST_DATABASE_URL and DATABASE_URL both pointing at `sr_p27_integ`, password from
   `%TEMP%\sr_pg_pw.txt`, `-- --include-ignored --test-threads=1`.
5. Records done: `docs/inventory/21-feature-phase.md` (created, ticked where the proof ran), CHANGELOG wave-27
   section, the AGENTS.md checklist line, this handoff.

## Merges

| # | Branch | Head taken | Conflicts / merge work |
|---|---|---|---|
| 1 | feat/p27-byos-core | 058822d8 | none |
| 2 | feat/p27-byos-instance | ee7101c3 | `sr-storage/src/lib.rs` module list; app.rs: one repo, one sealer, one `CachedMountResolver` shared by `MountStorages`, `MountsDeps` (`with_resolver`) and the CORS recheck job |
| 3 | feat/p27-byos-ui | 8077e05f | none |
| 4 | feat/p27-byos-scopes | 0c345f07 (incl. the catalog-counts slice, merged after the commander said it was done) | `routes/storage/mod.rs` add/add (scopes' file, core's `mounts`/`mount_ops` merged in); `route_matrix.tsv` rows of both kept; `routes/mod.rs` had the storage router and OpenAPI merged twice, now once |

Merge work:
- One mount repo and one sealer for everything; the 60 s `CachedMountResolver` serves `MountStorages` and the mount
  routes (which invalidate it). `ScopeDeps` (listing and presign) reads the repo **uncached** on purpose: a
  disabled mount or a cleared CORS flag (recheck job, sr-cli, a direct write) must stop presigning at once.
  `storage_scopes_hold_over_http` caught this (it failed with the cached resolver). One `PgScopeFacts` is shared by
  `ScopeDeps` and `MountsDeps`.
- AUTHZ: the user mount `{mount_id}` rows (PATCH, DELETE, test, cors/check, cors/apply) are `mount_owner`; list
  and create stay `self`; the guild mount rows stay `membership` (no shared value means "server manager").
  `PUT /guilds/{guild_id}/storage/grants` is BOT deny (the merged `auth::policy` refuses bots on every storage
  route).
- Guild manage check: `guild_scope` decides through the new `sr_storage::scopes::may_manage_guild` (same rule as
  `may_manage`: owner, ADMINISTRATOR, MANAGE_GUILD or a role with the `manage` grant). Unit tests without facts fall
  back to MANAGE_GUILD.
- Key check: `browse::check_key` replaced by `sr_storage::path::safe_key` in browse.rs and objects.rs (browse
  re-exports `safe_key`, `KeyError`, `MAX_KEY_BYTES`). **NFC normalisation is still missing** (no
  `unicode-normalization` dependency).
- 409s: `CORS_NOT_READY` and `BUCKET_UNREACHABLE` now use `mounts::conflict` (SrConflict 900010, field error
  `mount`), the same shape as `MOUNT_CAP_REACHED` and `MOUNT_LABEL_TAKEN`.
- `rate_limit` test: `Abuse::ALL` has ten limits since `abuse.storage_tests_per_hour` (core's branch still said 9).
- UI: wire types in `settings/user/storage/types.ts` and `storage/api.ts` match the OpenAPI (MountObject,
  CreateMountBody, TestResultObject, CorsResultObject, ShareView, ObjectList, PresignView, GrantsBody); the
  summary's `catalog` is typed optional and nullable. byos-ui's hand-written `mocks.ts` stays (its tests use it).
- Duplicate checks: job id 134 / `storage.cors_recheck` unique; `byos.presign_ttl_secs`,
  `byos.presign_ttl_cap_secs`, `abuse.storage_tests_per_hour` unique. (Job id 1150 appears twice, but that is one
  job re-exported, pre-existing.)

## Gates

| Gate | Result |
|---|---|
| cargo fmt --check | green |
| clippy --workspace --all-targets -D warnings | green |
| cargo test -p sr-storage | green (110 lib + integration; live Garage/S3 cases ignored, CI only) |
| sr-db mounts_repo, scopes_repo, social_repo (Postgres) | green (3 + 3 + 7) |
| sr-api --lib, route_matrix, openapi_names | green (693, 6, 2) after the two fixes above |
| sr-admin, sr-cli | green |
| sr-server --lib admin_jobs, instance_mount (incl. Postgres) | green (8) |
| authz_matrix storage_scopes_hold_over_http | green after the uncached-presign fix |
| authz_matrix every_route group 5 | **not run** |
| cargo deny --offline check | green |
| OpenAPI regen, api-client generate, mocks:generate, mocks:check | green (996 operations) |
| web typecheck, eslint, prettier --check | green |
| i18n-audit | 0 |
| vitest src/settings src/storage src/lounge (--maxWorkers=2) | green (146 files, 1162 tests) |
| web build + bundle budget | green |
| check-css-tokens | green (16 files) |
| full workspace cargo test | **not run** (skipped today) |

## Wave-28 follow-ups

- byos-instance: `POST /admin/storage/mounts/1/attach|detach` (mount 1 as a Guild Shelf) needs byos-scopes'
  `guild_storage_grants`; not built in wave 27.
- byos-instance: per-row mount threading (`mount_id` on the media structs) and per-mount CDN URLs.
- NFC normalisation of keys (needs `unicode-normalization`, a download).
- The mount caps are constants (`DEFAULT_MAX_MOUNTS_PER_*`); the `byos.max_mounts_per_*` settings named in the
  comments are not in the catalog.
- The Garage live cases of `storage.yml` and the first CI run after the owner pushes.

## Suggested PR title

`Wave 27: BYOS core (BYOS-MNT-001..003, BYOS-SCP-001/002, BYOS-INS-001, BYOS-UI-001)`
