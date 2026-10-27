# Handoff: feat/p29-tv-byos (wave 29, BYOS-TV-001 and BYOS-NAV-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p29-tv-byos`, branch `feat/p29-tv-byos`, base `integration/phase29`
(a382cd52). Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p29-tv-byos-target`,
`CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo command at a time. Migration used: 1510. No database ran (the
Postgres password file `%TEMP%\sr_pg_pw.txt` was missing), nothing pushed, no PR, no server started.

## Resume here

Done and committed: BYOS-NAV-001 (all of it) and the sr-tv half of BYOS-TV-001 (pool source, scoped pools,
refusal, watch-access port). See "Still open" for what is left, in order.

## Done

BYOS-NAV-001:
- `DomainEvent::StorageMountUpdated` (sr-events) is the guild-scoped event, mapped to the gateway dispatch
  `STORAGE_MOUNT_UPDATE` in `sr-gateway/src/hub.rs` (no intent gate; payload `{mount_id, owner_kind, guild_id?, change}`,
  ids as strings, never a secret). A member's mount and a share change go to the member's sessions (and the friend's)
  as `UserScoped` with the same name.
- `crates/sr-api/src/routes/storage/events.rs` (`StorageEvents`, an extension registered in `sr-server/src/app.rs`)
  publishes it from mount create/patch/delete (user and server routes), share put/delete, and the instance bucket's
  attach/detach. A failed publish only logs.
- Web: `apps/web/src/storage/navModes.ts` `useShelvesSummary`/`useNavVisibility` fetch the summary while a tab is on
  Auto, and again after `READY` and on `STORAGE_MOUNT_UPDATE`; a failed refetch keeps the last answer (fail visible).
  `ShelfEmpty.tsx` is the shared empty-shelf state ("Nothing here yet", mount a bucket / ask a friend links); used by
  Cinema Discover and by Library Browse when no filter is set. Music has no screen until wave 30 (`kind="music"`
  is ready for it).
- Strings in `apps/web/src/i18n/messages/en/p29-tv-byos-1.ts`.

BYOS-TV-001 (sr-tv):
- Migration `1510_tv_channel_pool_source.sql`: `tv_channels.pool_source` (`instance|guild|personal`), `guild_id`,
  `owner_user_id`, a consistency CHECK, and channel numbers unique per source (partial unique indexes; the old global
  unique constraint is dropped by its default name).
- `sr_tv::model::PoolSource` (in `pool_source.rs`), a field of `ChannelInput` (default `instance`, so existing
  staff clients keep working) and of the wire `Channel`.
- `TvCatalog::mounts_of` / `pool_in` / `admits`; `CatalogItem` gained `mount_id` and `playable`. The service builds
  pools through `pool_in`, drops items that are not `playable` for everyone alike (one guide per server), and the
  editor (`place_block`, `set_now`) refuses an item off the channel's shelves with the reason ("never a member's
  personal shelf").
- `ChannelAccess` port + `TvService::with_access`, `may_watch`, `require_watch`, `visible_channels`,
  `visible_guide`. `tune` and the live playlists answer 404 to someone who may not watch. A server channel with no
  access port wired is watched by nobody (fail closed). A personal channel is watched only by its owner.
- Tests: `crates/sr-tv/src/shelf_tests.rs` (9), `pool_source` (2), `model` (1).

## Still open

See the final section "Open items" (kept current at each commit).

## Tests run

- `cargo test -j 2 -p sr-events`, `-p sr-tv --lib` (99 passed), `-p sr-api --lib storage::` (28 passed).
- Web: vitest `src/storage/__tests__/navModes.test.tsx` and `shelfEmpty.test.tsx` (12 passed), eslint on the touched
  files, prettier.

## Open items

1. Server wiring (sr-server): a `ChannelAccess` over the storage grants, `mounts_of` over the mount repo and
   `attached_guilds`, `mount_id`/`playable` on the two `CatalogItem` literals in `tv_catalog.rs`.
2. Member TV routes: filter `/tv/channels` and `/tv/guide` through `visible_channels` / `visible_guide`, and
   `require_watch` on `/tv/channels/{id}` and `/now`.
3. OpenAPI, api-client and MSW mocks regenerated; web types and the "Server shelf" chip in the guide, with a Vitest.
4. Not built (needs the owner or a later wave): see the report.
