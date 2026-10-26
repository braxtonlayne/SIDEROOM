# Integration handoff: wave 27 (BYOS core) on `integration/phase27`

Integrator: Opus 5.5, 2026-10-26. Base `5e11943d`. Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p27-integ`,
`CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p27-integ-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`,
`CMAKE` = the VS 2019 Build Tools copy (`C:/Program Files (x86)/Microsoft Visual Studio/2019/BuildTools/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe`, needed by opusic-sys in sr-sdk),
scratch DB `sr_p27_integ`. Nothing pushed, no PR, nothing merged to main. No server was started.

## Resume here

1. All four merges are done and committed (table below), plus the merge work, the OpenAPI / api-client / MSW
   regeneration (`mocks:check` green) and `cargo fmt`.
2. Gates done: fmt, clippy `-D warnings` on the workspace, `cargo test -p sr-storage`.
3. Gates still to run, in order: sr-api lib + route_matrix + openapi_names (was running at 17:55), sr-admin,
   sr-cli, the Postgres suites (sr-db, sr-server admin_jobs and instance_mount, authz matrix
   `storage_scopes_hold_over_http` and every_route for the storage groups), cargo deny --offline, the web gates
   (typecheck, eslint, prettier --check, i18n-audit, vitest on src/settings, src/storage, src/lounge, build +
   bundle budget, check-css-tokens).
4. The full workspace cargo test is NOT run (skipped today on the commander's word; the next session runs it).
5. Records still to finish: tick `docs/inventory/21-feature-phase.md` where the proof ran, CHANGELOG wave-27
   section, the AGENTS.md checklist line.

## Merges

| # | Branch | Head taken | Conflicts / merge work |
|---|---|---|---|
| 1 | feat/p27-byos-core | 058822d8 | none |
| 2 | feat/p27-byos-instance | ee7101c3 | `sr-storage/src/lib.rs` module list; app.rs: one repo, one sealer, one `CachedMountResolver` shared by `MountStorages`, `MountsDeps` (`with_resolver`) and the CORS recheck job |
| 3 | feat/p27-byos-ui | 8077e05f | none |
| 4 | feat/p27-byos-scopes | 0c345f07 (incl. the catalog-counts slice, after the commander's word) | `routes/storage/mod.rs` add/add (scopes' file, core's `mounts`/`mount_ops` merged in); `route_matrix.tsv` rows of both kept; `routes/mod.rs` had the storage router and OpenAPI merged twice, now once |

Merge work after the scopes merge (commit f58d3aed):
- `ScopeDeps` uses the same resolver and sealer; one `PgScopeFacts` shared by `ScopeDeps` and `MountsDeps`.
- AUTHZ: the user mount `{mount_id}` rows (PATCH, DELETE, test, cors/check, cors/apply) are `mount_owner`; the
  list/create rows stay `self`; the guild mount rows stay `membership` (no shared value means "server manager").
- Guild manage check: `guild_scope` decides through `sr_storage::scopes::may_manage_guild` (new, same rule as
  `may_manage`: owner, ADMINISTRATOR, MANAGE_GUILD or a role with the `manage` grant); unit tests without facts
  fall back to MANAGE_GUILD.
- Key check: `browse::check_key` replaced by `sr_storage::path::safe_key` (browse re-exports `safe_key`,
  `KeyError`, `MAX_KEY_BYTES`). NFC normalisation is still missing (no `unicode-normalization` dependency).
- 409s: `CORS_NOT_READY` and `BUCKET_UNREACHABLE` now use `mounts::conflict` (SrConflict 900010, field error
  `mount`), the same shape as `MOUNT_CAP_REACHED` and `MOUNT_LABEL_TAKEN`.
- UI: wire types in `settings/user/storage/types.ts` and `storage/api.ts` match the OpenAPI (MountObject,
  CreateMountBody, TestResultObject, CorsResultObject, ShareView, ObjectList, PresignView); the summary's
  `catalog` is typed optional and nullable. byos-ui's hand-written `mocks.ts` stays (its tests use it).

## Gates

| Gate | Result |
|---|---|
| cargo fmt --check | green (after `cargo fmt`) |
| clippy --workspace --all-targets -D warnings | green |
| cargo test -p sr-storage | green (110 lib + 32 integration; the live Garage/S3 cases are ignored, they run in CI) |
| OpenAPI regen, api-client generate, mocks:generate, mocks:check | green (996 operations) |

## Wave-28 follow-ups

- byos-instance: `POST /admin/storage/mounts/1/attach|detach` (mount 1 as a Guild Shelf) needs byos-scopes'
  `guild_storage_grants`; not built in wave 27.
- byos-instance: per-row mount threading (`mount_id` on the media structs) and per-mount CDN URLs.
- NFC normalisation of keys (needs `unicode-normalization`, a download).
