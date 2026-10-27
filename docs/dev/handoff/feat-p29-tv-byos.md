# Handoff: feat/p29-tv-byos (wave 29, BYOS-TV-001 and BYOS-NAV-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p29-tv-byos`, branch `feat/p29-tv-byos`, base `integration/phase29`
(a382cd52). Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p29-tv-byos-target`,
`CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one cargo command at a time. Migration used: 1510. No database ran (the
Postgres password file `%TEMP%\sr_pg_pw.txt` was missing), nothing pushed, no PR, no server started.

## Resume here

BYOS-NAV-001 is complete. BYOS-TV-001 is complete except the parts listed under "Not done" (they need a runtime probe
and the byte-range renderer, which no wave-29 agent owns). Everything is committed; the working tree is clean.

If you continue: (1) run the Postgres tests listed under "Not run" with a scratch DB `sr_p29_tv_byos`; (2) the
admin channel editor's source picker; (3) the byte-range renderer. Nothing else is half-finished.

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

## Server wiring and routes (BYOS-TV-001, sr-server and sr-api)

- `sr-server/src/tv_catalog.rs`: `ShelfMounts` (a server's live mounts plus the platform mounts attached to it; a
  member's own live mounts; a friend's mount is never a source; a failing store is an error, not an empty answer),
  `GuildChannelAccess` (member of the server holding the `view_cinema` storage grant; owner/ADMINISTRATOR hold all),
  `RepoTvCatalog::with_shelves` / `mounts_of`. Catalog rows (the instance catalog) carry `mount_id = 1` and
  `playable = true`. `app.rs` wires both into the `TvService`.
- `routes/tv/member.rs`: `/tv/channels` and `/tv/guide` list only what the caller may watch; `/tv/channels/{id}` and
  `/now` answer 404 to anyone else; `tune` and the live playlists check inside the service.
- No new route, so no route_matrix row or authz-matrix seed is due; the existing rows (session) are unchanged.
  `STORAGE_MOUNT_UPDATE` is an event, documented in `docs/05-api-and-gateway.md`.
- OpenAPI, api-client and MSW handlers regenerated (1017 operations; the diff is only `pool` / `TvPoolSource`).
- Web: `cinema/tv/PoolChip.tsx` ("Server shelf" with the server name in the tooltip, "Your shelf") in the guide's
  channel cell; `TvChannel.pool`.
- Spec: `docs/specs/cinema-live-tv.md` section 9.

## Not done (and why)

- **Mount-backed items on air.** Items found in a server's own or a member's mount are not offered to the scheduler:
  (a) the indexer stores no runtime for them (`storage_objects` has no duration; a block needs one), and (b) the
  `EXT-X-BYTERANGE` renderer over a presigned MP4 (an MP4 sample-table reader that cuts byte-range segments at
  keyframes, then per-viewer signing under the TTL rule) is a piece of work of its own. Until both exist a server
  channel airs the instance catalog when the instance bucket is attached (mount 1), exactly like the beta needs.
  `CatalogItem.playable` and `TvCatalog::pool_in` are the seams: a shelf-aware catalog returns the mount items there.
  This is the roadmap's own risk 3 (hls.js with byte ranges over presigned URLs) and needs the `media.yml` stage
  (`media.yml` is conv-worker's file this wave).
- **The "left out with a guide note"** is the quiet drop of unplayable items; there is no per-channel note in the
  guide payload yet.
- **Admin channel editor source picker.** `pool` is accepted and returned by the staff API, but the admin editor
  (`apps/web/src/admin`) has no control for it, so server channels are created through the API for now.
- **Member-facing creation of a server or personal channel** (who may create one, quotas): no route. Owner question 1.
- **Imported-guide channels** still match show names against the whole instance catalog (`title_refs`), whatever the
  pool; a server channel of kind `imported_guide` should be refused or scoped. Owner question 2.
- **Watch parties on server channels** fail closed (the party gate tunes with no viewer, so the access port answers
  no). Instance channels are unchanged.
- `media.yml` stage that airs a mount-backed MP4 across the slate boundary: not written (conv-worker owns the file).
- Playwright and axe on the real page: not run (no browser allowed on 8080/5173 for this agent; ports 8087/5180 unused).

## Owner questions

1. Who may create a server channel (`MANAGE_GUILD`? the `manage` storage grant? staff only for the beta) and how many
   per server? Today: staff only, through `/admin/tv/channels` with `pool`.
2. Should `imported_guide` (scraped schedule grids) be allowed on a server channel? Conservative default: allowed but
   unscoped (see above); recommend refusing it.
3. Numbers are now unique per source (two servers can both have channel 1; migration 1510 drops the global unique
   constraint by its default name `tv_channels_number_key`). Confirm that the global ordering by number in the member
   guide is still what you want when a member sees instance and server channels together.

## Tests run

- `cargo test -j 2 -p sr-events` (15), `-p sr-tv --lib` (99, nine of them new in `shelf_tests`), `-p sr-gateway --lib`
  (186), `-p sr-api --lib storage::` (28) and `tv::` (9), `-p sr-api --test route_matrix --test openapi_names` (6 and 2).
- `cargo clippy -j 2 -p sr-events -p sr-gateway -p sr-tv -p sr-api -p sr-server --all-targets -- -D warnings`: clean.
  `cargo fmt --all`.
- Web: vitest `src/cinema src/library src/storage src/i18n` (46 files passed, --maxWorkers=2), `tsc --noEmit`, eslint on
  the touched folders, prettier, `scripts/i18n-audit.mjs` (0).
- `cargo test -j 2 -p sr-server --lib tv_catalog` (3 new: `ShelfMounts`, the catalog's `mounts_of`, `GuildChannelAccess`).

## Not run

- Postgres: `cargo test -p sr-tv --test pg -- --include-ignored --test-threads=1` (new test
  `pool_sources_round_trip_and_numbers_are_unique_per_source`; it compiles) and the migration itself (1510; the
  password file was missing). Run it with `sr_p29_tv_byos` as scratch DB. The authz matrix and
  `shelves_hold_over_http` were not run either.
- bundle budget (`vite build`): not run; the new web code is a small chip, a small hook change and a shared empty state.
