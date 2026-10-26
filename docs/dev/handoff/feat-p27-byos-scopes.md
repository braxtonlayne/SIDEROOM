# Handoff: feat/p27-byos-scopes (wave 27, BYOS-SCP-001..002)

## Status
BYOS-SCP-001 and BYOS-SCP-002 are built and proven except the items under "Not done". Commits on `feat/p27-byos-scopes` from tag `p27-day1`.

## Done
- Migration `1410_storage_shares_grants.sql`: `storage_mount_shares`, `guild_storage_grants` (column `grant_name`, GRANT is reserved).
- `sr_storage::scopes` (`resolve`, `access`, `may_upload`, `may_manage`, `guild_grants`, `Kind::of_key`, `presign_ttl_secs`, `check_range`, `ScopeFacts`, `ScopeStore`, `MemoryScopeFacts`): every refusal of roadmap section 2 is a unit test (22 + 2).
- `sr_storage::browse`: `ObjectStoreStorage::list_dir` (folders and objects, cursor pages of 1000), `head_meta`, `check_key` with an exhaustive small-alphabet fuzz.
- `sr_db::scopes_repo::PgScopeFacts` (facts, shares, grants, `shelves_summary`), unfriend and block revoke inside `PgSocialRepo::apply`.
- Routes (`crates/sr-api/src/routes/storage/{mod,scopes,objects}.rs`): shares (list, put, delete, incoming), `GET /users/@me/shelves/summary`, `PUT /guilds/{id}/storage/grants`, objects listing, object metadata, presign. Shapes are the ones byos-ui agreed (commander message). `sr_storage_grants` is on `GET /guilds/{id}/roles`. Wired in `sr-server/src/app.rs` (`storage_scope_deps`), OpenAPI fragment merged.
- Settings `byos.presign_ttl_secs` (900) and `byos.presign_ttl_cap_secs` (3600) in `sr-admin/src/settings.rs`; hard ceiling 86400 in code; capped by the session's remaining time.
- Staff read: `OPERATOR` (or owner) caller gets `Scope::Staff`, written to the access log as kind `read`, action `storage.read`, target `storage_mount` (the table's kind check has no `storage.read` kind, so it is the action). A failed log write refuses the read.
- route_matrix rows (new AUTHZ values `storage_scope`, `mount_owner`), policy denies bots and bearers on mount routes.
- Authz: `authz_matrix.rs` seeds a mount per tenant (`mount_id` is in TENANT: cross-tenant sweep, outsider, anon, bot, bearer all proven; group "rest": 0 failures, 16 weak (ceiling 16), writes admitted). New test `storage_scopes_hold_over_http` proves `friend_shared`, `friend_unshared`, member with and without grants, staff (logged), disabled, no CORS (409 `CORS_NOT_READY`), banned owner, unfriend revokes, TTL 900.
- Docs: 05 section 5.3a, 07 "Storage grants". `storage.yml`: new live step runs `crates/sr-storage/tests/s3_mount_live.rs` (listing under a prefix, presigned ranged GET gives 206 and `content-range`).

## Tests run (all on this machine, -j 2)
`cargo test -p sr-storage` (93 lib tests incl. scopes and browse), `-p sr-admin`, `-p sr-api --lib` (687), `-p sr-api --test route_matrix` (6) and `--test openapi_names`, `-p sr-db --test scopes_repo --test social_repo` (Postgres), `-p sr-server --test authz_matrix storage_scopes` and `every_route` with `SR_AUTHZ_GROUPS=5`; fmt and clippy `-D warnings` on sr-storage, sr-db, sr-api, sr-admin, sr-server.

## Not done / open
- `catalog?` in the shelves summary (instance catalog counts the viewer can see) is omitted; needs a query over published cinema titles and library series. byos-ui must treat it as absent.
- The Garage live case (`s3_mount_live`) has not run here (no Garage); first run is in CI after the owner pushes.
- The presign p95 under 5 ms benchmark was not added (no S3 round trip, but two or three indexed queries).
- `attached_guilds` (instance mount as a Guild Shelf) returns empty until byos-instance's attach table exists; `delisted` returns empty until BYOS-LEG-001. Both are single methods in `scopes_repo.rs`.
- Moderation-case staff reads (a case touching the key) are not wired; only `OPERATOR`/owner.
- `safe_key` NFC: `browse::check_key` has the other rules; NFC needs `unicode-normalization` (not a dependency of sr-storage). byos-core owns `path::safe_key`; swap the call in `objects.rs`/`browse.rs` when it lands.
- OpenAPI/api-client/MSW regeneration left to the integrator.
- Authz group "rest" watch-party rows failed once on this loaded machine (member staleness is 90 s); they pass on rerun and are unrelated.

## Decisions / deviations
- Key kind comes from the file extension (`Kind::of_key`); companion files (covers, subtitles, unknown) follow any share or view grant. No wire change.
- `manage` reads every kind; `MANAGE_GUILD` implies `manage` only; owner/ADMINISTRATOR hold all five; pending members are not members.
- Disabled mount, inactive owner and delisted keys are refused for staff too. `failing` mounts stay readable.
- Only `Refused::NoGrant` is 403; every other refusal is 404.
- 409 `CORS_NOT_READY` is `SrConflict` (900010) with message and field error `mount` both `CORS_NOT_READY` (no new ErrorCode, to avoid clashing with byos-core). A bucket that does not answer in 8 s is 409 `BUCKET_UNREACHABLE`.
- The unfriend hook lives in `sr-db/src/social_repo.rs::apply` (where relationships are written), not sr-chat.

## Merge notes for the integrator
- `routes/storage/mod.rs` is mine; byos-core's `mounts.rs` module and its deps merge into it. `route_matrix.rs` AUTHZ_VALUES gained `storage_scope`, `mount_owner` (byos-core may add `mount_owner` too). `sr-server/src/app.rs` gets one block and one `.extension(...)`.
- Needed from byos-core: nothing blocking. Wanted: `Mount::storage` should set a short object_store retry/timeout (it retries a dead endpoint for minutes; the routes wrap calls in 8 s as a guard), and `path::safe_key` to replace `browse::check_key`.
