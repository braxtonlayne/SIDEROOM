# Handoff: feat/p27-byos-scopes (wave 27, BYOS-SCP-001..002)

## Status
- Slice 1: migration 1410 and `sr_storage::scopes` (decision logic, `MemoryScopeFacts`, 22 unit tests).
- Slice 2: `sr_db::scopes_repo::PgScopeFacts` (facts, share and grant CRUD, shelves summary), unfriend/block hook inside `PgSocialRepo::apply`, Postgres tests `crates/sr-db/tests/scopes_repo.rs` (2 pass).
- Routes (SCP-001 and SCP-002) not started.

## Resume here
Next, in order:
1. Routes in `crates/sr-api/src/routes/storage/{scopes,objects}.rs` plus `mod.rs` (shared with byos-core: add only my modules; register in routes/mod.rs `all()` and openapi). Wire shapes agreed with byos-ui (commander message):
   - `GET /users/@me/storage/mounts/{mount_id}/shares` -> `[{friend_id, cinema, library, music}]` (owner only); `PUT .../shares/{user_id}` body `{cinema,library,music}` returns the stored share (mutual friends only, else 403/404; client sends DELETE when all off); `DELETE` 204.
   - `GET /users/@me/storage/shares` -> incoming (`PgScopeFacts::shares_to`).
   - `GET /users/@me/shelves/summary` -> `{cinema, library, music, catalog?: {cinema, library}}`; `PgScopeFacts::shelves_summary` gives the first three; `catalog` (instance catalog counts the viewer can see) still needs a query on cinema/library published counts.
   - `PUT /guilds/{gid}/storage/grants` body `{role_id, grants:[view_cinema|view_library|view_music|upload|manage]}` -> 204; needs MANAGE_ROLES plus `manage`; role payload carries `sr_storage_grants` (use `grants_of_guild`; roles.rs is another agent's file: expose via a small helper, one call).
   - `GET /storage/mounts/{id}/objects?prefix=&delimiter=/&cursor=` -> `{folders: [], objects:[{key,size,last_modified|null,content_type?}], next_cursor|null}`; `GET .../objects/{*key}` metadata; `POST .../presign` body `{key, method:"GET", range?}` -> `{url, expires_at}`; 409 `CORS_NOT_READY` when `cors_ok_at` is None; 404 other's mount, 403 `Refused::NoGrant` (`Refused::disclosure()`); ids strings, timestamps ISO 8601.
   - Key check: `sr_storage::path::safe_key` is byos-core's and not in my base; write `scopes`-side check or wait (NFC needs unicode-normalization, not a dep of sr-storage).
   - Presign TTL: `byos.presign_ttl_secs` 900, cap `byos.presign_ttl_cap_secs` 3600, hard 86400, and never past the session's remaining time. Staff caller: `Scope::Staff` must write `staff_access_log` (action `storage.read`, kind `read`).
2. route_matrix rows (new AUTHZ values `storage_scope` and `mount_owner` in `route_matrix.rs` AUTHZ_VALUES; byos-core may add `mount_owner` too, merge by hand), authz_matrix actors `friend_shared`, `friend_unshared`, member with/without grants, cross-tenant sweep over `{mount_id}`.
3. Docs: 07 section 2 grants paragraph, 05 storage group. `storage.yml` Garage range case (206, readable content-range).
Env: CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p27-byos-scopes-target, CARGO_PROFILE_DEV_DEBUG=0, `-j 2`, one cargo at a time. Scratch DB `sr_p27_byos_scopes` exists (drop at the end); Postgres tests: `cargo test -p sr-db --test scopes_repo -j 2 -- --include-ignored --test-threads=1` with SR_TEST_DATABASE_URL and DATABASE_URL both pointing at it.

## Decisions / deviations
- `guild_storage_grants` column is `grant_name` (GRANT is reserved); values `view_cinema|view_library|view_music|upload|manage`.
- Key kind comes from the file extension (`Kind::of_key`); companion files (covers, subtitles, unknown) follow any share or view grant. No wire change.
- `manage` reads all kinds; `MANAGE_GUILD` implies `manage` only (not `upload`); owner/ADMINISTRATOR hold all five. Pending members are not members.
- Disabled mount, inactive owner and delisted keys are refused for staff too (fail closed). `failing` mounts stay readable.
- Only `Refused::NoGrant` is 403; every other refusal is 404.
- Instance mount (Platform owner) is reachable only through attached guilds (`attached_guilds`, empty until byos-instance's attach table) or staff. `delisted` returns empty until BYOS-LEG-001.
- The unfriend hook lives in `sr-db/src/social_repo.rs::apply` (that is where relationships are written), not sr-chat.
