# Handoff: feat/p30-music-index (MUS-IDX-001)

Branch `feat/p30-music-index`, base `integration/phase30`. Migration 1540 only.

## Done

- **Audit.** `music_tracks` already existed (migration 1450, wave 28) with the tag columns, `overridden` and the
  indexer's upsert; the indexer already lists audio under any prefix (`Music/<artist>/<album>/` names, anywhere
  else the extension decides). Nothing was rebuilt; a unit test in `audio.rs` now pins "audio under any prefix".
- **Migration 1540** (`crates/sr-db/migrations/1540_music_track_tags.sql`): `tags_state`
  (`pending|read|none|failed`), `tags_read_at`, `tagged` (the part of `overridden` that came from the file, so a
  rescan may overwrite it but never a hand edit), `cover_hash`, two indexes.
- **`crates/sr-storage/src/index/audio.rs`**: `Track`, `LibraryFilter`, `TagWrite`, `TagSource`, `TagsState`, the
  `MusicStore` trait, text/genre cleaning and the cover-key rule (exactly `covers/<hex hash>.jpg`).
- **`crates/sr-db/src/music_repo.rs`** (`PgMusicStore`): scoped listing (ILIKE with escaped wildcards, album,
  artist, `pending`), get, tag writes (a `file` write skips fields the member edited by hand), rescan.
- **Routes** (`crates/sr-api/src/routes/music/{mod,library,tracks}.rs`, wired in `routes/mod.rs` and
  `sr-server/src/app.rs`):
  `GET /music/library?mount=&album=&artist=&q=&pending=&limit=&offset=`, `POST /music/library/rescan`,
  `GET|PATCH /music/tracks/{id}`, `POST /music/tracks/{id}/presign`.
  - Reads: the viewer is resolved with `staff: false`, so staff never reach a member's bucket here; a friend sees
    only what the share admits; delisted keys are dropped.
  - Writes (PATCH, rescan): mount owner, or on a server's shelf `upload`/`manage` (`scopes::may_upload`).
    Friend who can listen: 403. Not reachable (stranger, staff): 404.
  - Presign calls the wave-27 `objects::presign` handler itself with the track's mount and key (one code path, same
    CORS gate and TTL rules). To allow that, `storage::objects` became `pub(crate)` and `shelves::{Signer,
    source_of, store_failed}` `pub(crate)` (visibility only).
- **Route matrix** rows (5) and the OpenAPI fragment (`music::openapi()`) added.
- **Web** (`apps/web/src/music/index/*`): `Tagger` (4 at a time via `runPool`, per-step `withBackoff` with
  jitter and `Retry-After`, presign per attempt), `readTrackTags` (first 1 MiB; MP3 ID3v1 tail fallback; M4A
  `NEED_MOOV`), cover upload to `covers/<sha256-prefix>.jpg` in the same mount through the wave-28 upload manager,
  de-duplicated per mount and hash, `RescanTagsButton`, i18n fragment `p30-music-index-1.ts`.
- **NEED_MOOV (coordinator request).** On `{ code: 'NEED_MOOV', offset }` the tagger walks top-level boxes with
  16-byte range reads from `offset` (at most 32 steps), fetches `moov` alone when it is at most 8 MiB, and calls
  `readTags(ftyp + moov)` (ftyp taken from the head already read). A bigger `moov` counts as untagged
  (`tags_state: none`) and is logged once; "not found in 32 boxes" likewise. The facade landed in
  `p30-music-wasm` (`MusicNeedMoovError`), so this matches it; the types are declared structurally in
  `music/index/types.ts` because that branch is not merged here, and `loadMusic` is loaded by a dynamic import of
  `@sideroom/wasm` (it errors cleanly if the build lacks it).

## Tests run

- `cargo test -p sr-storage --lib index::audio`: 4 passed.
- `cargo test -p sr-api --test route_matrix --test openapi_names`: see the commit log / below.
- `cargo check -p sr-api -p sr-server`, `cargo fmt`: clean.
- Vitest `src/music/index` (4 files, 24 tests): tagger concurrency (peak 4), backoff pauses and Retry-After, 404
  vs 503 handling, cover dedup, rescan, NEED_MOOV walk with a fake presigned fetch (16-byte reads, ftyp+moov
  passed to the facade, >8 MiB, 32-step cap), ID3v1 tail, RescanTagsButton + axe. `tsc --noEmit` and eslint
  clean on the touched folders; prettier applied.

## Not run (list for CI)

The Postgres password file `%TEMP%\sr_pg_pw.txt` is missing, so these were written and compile-checked only
(`cargo test -p sr-db --test music_repo -- --include-ignored --test-threads=1`):
1. `audio_under_any_prefix_is_listed_with_its_container_facts` (index + listing + filters),
2. `who_may_write_tags_owner_and_upload_role_yes_friend_and_staff_no` (patch by the owner and by an `upload` role;
   refused for a friend and a listen-only role; staff/strangers 404),
3. `the_tagger_writes_tags_and_never_replaces_a_hand_edit` (file write, hand edit, rescan, re-index, failed retry).

The authz decision in test 2 mirrors the route's rule with `MemoryScopeFacts`; there is no HTTP-level test of the
403/404 mapping (the shelves routes have none either; `sr-server` Postgres integration is the place for it).

## Still open

- No UI places `RescanTagsButton` or starts the tagger on first listing of an album: the Library Manager panel is
  music-lib's / player-shell's. Call `getTagger().start(mountId)` when a library view opens, and show the button
  where `TrackView.can_edit` is true.
- The wave-28 upload "modal" for the music prefix is not wired here (the cover upload uses the manager directly).
- `sr-music-wasm` dependency: when `p30-music-wasm` merges, `MusicApi` from `@sideroom/wasm` is assignable to
  `TagReader`; no change needed.
- Merge conflict watch: `routes/music/mod.rs` is created here; music-lib adds `pub mod` lines and routes to it.

## Decisions / owner questions

- Hand edits win over file tags (`tagged` vs `overridden`), and "Rescan tags" re-reads files without losing them.
  Confirm that is the wanted behaviour.
- A non-JPEG embedded cover is redrawn to JPEG in the browser (canvas) so the key is always `covers/<hash>.jpg`.
- `POST /music/library/rescan` is an extra route beyond the roadmap list.

## Resume here

Backend and tagger are complete. Next: run the three Postgres tests once a scratch DB is available, regenerate
`packages/api-client/openapi.json` and the client (`cargo run -q -p sr-server -- openapi > ...`, then
`pnpm --filter @sideroom/api-client generate` / `mocks:generate`) if not already done, and run the bundle budget.
