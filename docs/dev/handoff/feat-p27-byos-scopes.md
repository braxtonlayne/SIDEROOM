# Handoff: feat/p27-byos-scopes (wave 27, BYOS-SCP-001..002)

## Status
Slice 1 done: migration 1410 and `sr_storage::scopes` (pure decision logic + memory facts + 22 unit tests).

## Resume here
Next, in order:
1. Postgres `ScopeFacts` impl: `sr_db::scopes_repo::PgScopeFacts` (owner_active: account state `active`; guild: exists and its owner account active; are_friends: both `relationships` rows type 1; share; guild_standing from `guild_members`/`member_roles`/`roles.permissions` + `guilds.owner_id` incl. the everyone role = guild id; role_grants from `guild_storage_grants`; attached_guilds returns empty until byos-instance's attach table exists; delisted returns empty until BYOS-LEG-001) + `shares`/`grants` repo methods + Postgres tests (share lifecycle, unfriend revokes).
2. Unfriend hook in sr-chat (one call deleting `storage_mount_shares` between the pair).
3. Routes `crates/sr-api/src/routes/storage/{scopes,objects}.rs` + `mod.rs` (shared with byos-core: add only my modules), route_matrix rows (new AUTHZ values `storage_scope`, `mount_owner` in route_matrix.rs AUTHZ_VALUES), authz_matrix actors, docs 05/07.
4. SCP-002: listing, metadata, presign (TTL rules), key check, staff access log `storage.read`.
Env: CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p27-byos-scopes-target, CARGO_PROFILE_DEV_DEBUG=0, `-j 2`, one cargo at a time; scratch DB `sr_p27_byos_scopes` (create/drop yourself).

## Decisions / deviations
- `guild_storage_grants` column is `grant_name` (GRANT is a reserved word); values `view_cinema|view_library|view_music|upload|manage`.
- Key kind comes from the file extension (`Kind::of_key`); companion files (covers, subtitles, unknown) follow any share/view grant. So no wire change for presign/listing.
- `manage` reads all kinds; `MANAGE_GUILD` implies `manage` only (not `upload`); owner/ADMINISTRATOR hold all five.
- Disabled mount, inactive owner and delisted keys are refused for staff too (fail closed). `failing` mounts stay readable.
- Disclosure: only `NoGrant` is 403; every other refusal is 404.
- Instance mount (Platform owner) is reachable only through attached guilds (`attached_guilds`) or staff.
