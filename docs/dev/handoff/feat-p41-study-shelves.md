# Handoff: feat/p41-study-shelves (STU-SHF-001, wave 41-B)

## Resume here

Status at the last commit: the 41-B scope below is built and tested except where "Still open" says otherwise.
Next step if more time: run the final gates listed under "Tests run" that are marked pending, then the integrator
swaps the sidebar's data source (see "Decisions", item 1) when 41-C lands.
Scratch database to keep: `sr_p41_shelves` (drop it after integration).

## Done

- **The Study page** (`apps/web/src/study/`, route `/study`, registered in `router.tsx`; the navigation rail that
  links to it belongs to 41-A and is untouched).
  - Sidebar by source: My Files, Friends' Shelves (one entry per sharing friend), Guild Shelves (one per guild with a
    view grant). `sources.ts` (`buildSidebar`, `legacyVisible`).
  - Provenance badge on every card (`@handle's bucket`, `Guild: <Name>`), `StudyFiles.tsx`.
  - Type filters Video, Audio, Reading (EPUB/CBZ/PDF), Game & Mod Packs, Images, Other, with counts (`fileTypes.ts`).
  - Covers strictly from the file, client side (`covers.ts`, `useCover.ts`): ID3/MP4/FLAC art through the existing
    `sr-music-wasm` facade (`loadMusic`, range reads), page 1 of a CBZ and the cover image of an EPUB through the
    existing `sr-library-wasm` page lister, a frame captured from a `<video>` for web-native video, the image itself
    for images. Two extractions at a time, each file once. No metadata provider is called.
  - Owner edits: a custom title, cover (an image of the same bucket) and note. Server: `GET|PUT
    /storage/mounts/{mount_id}/notes` (`crates/sr-api/src/routes/storage/study.rs`), allowed for `scopes::may_manage`
    (owner; on a guild shelf a manager), reads filtered by `Access::permits` and delisting.
  - Personal rails: Continue Watching, Continue Reading (local progress, browser only), Recently Added to Shelves You
    Can See (newest files across up to six reachable shelves), Pinned Folders (local). `rails.ts`.
  - Viewer: plays video/audio (resume position), reads CBZ (resume page), EPUB/PDF, shows images, else offers the
    file's own short-lived link. Reuses `ShelfPlayer`, `CbzReader`, `PresignedReader` (small optional props added:
    `startAt/onTime`, `startPage/onPage`).
- **Screening Room**: `lounge/voice/ScreeningRoom.tsx` replaces the bare party strip in the voice channel view. It
  holds Watch Together (start/join), a link to the shelves, the Live TV guide link and the existing party strip. The
  Cinema routes (`/cinema/party/..`, `/cinema/tv`) and all their code are kept.
- **Migration 2010** `shelf_objects` (see Decisions 2).

## Still open

- Guild Live TV guide is linked from the Screening Room, not re-hosted: scheduling UI still lives under `/cinema/tv`
  (41-A decides whether the Cinema routes stay mounted; if they are removed, mount `TvGuidePage` under the Screening
  Room instead).
- Covers presign each visible file (one `POST .../presign` per card near the viewport). 41-C's per-object URL mint
  replaces this call; a batch mint would be kinder to the rate limit.
- EPUB cover is a heuristic: the archive entry named like a cover, else the first image in reading order (not the
  OPF-declared cover). PDF has no cover.
- Playwright spec `e2e/tests/study-shelves.spec.ts` is written and typechecked only (ports belong to 41-A).
- The Recently Added rail lists one page (1000 objects) per shelf.

## Decisions

1. **Data-source seam.** The sidebar is built from `VisibleSource` (`study/sources.ts`), rows shaped like 41-C's
   `GET /shelves/visible` (`{shelf_id, kind own|friend|guild, label, owner_id?, guild_id?}`). Today
   `study/api.ts#visibleShelves` derives them from `GET /users/@me/storage/mounts`, `/users/@me/storage/shares`,
   `/shelves/{cinema,library,music}` (sources) and `/users/@me/games/roms`. **Swap when 41-C lands:** make
   `visibleShelves` fetch `/shelves/visible` and return the rows; nothing else changes (`StudyPage` takes the source
   as a prop, tests pass lists directly).
2. **Table name aligned with 41-C.** Migration `2010_shelf_objects.sql` creates `shelf_objects(shelf_id,
   object_hash, path, size, kind, owner_title, owner_cover, owner_note, delisted_at, delist_strike_id, updated_at)`
   with `CREATE TABLE IF NOT EXISTS`. Today `shelf_id` is the storage mount id and `object_hash` is the SHA-256 hex of
   `path` (computed by Postgres); a row exists only when the owner typed something. 41-C should either keep this
   migration and add its columns/indexes with `ALTER`, or drop it and create the table itself (the code only uses
   `shelf_id, object_hash, path, owner_title, owner_cover, owner_note, delisted_at`). Delisted rows are never listed.
3. Edit rights are `may_manage` (same as the existing shelf metadata editor), so a guild manager can annotate a guild
   shelf; the brief said "bucket owner".
4. Type buckets: `emu/` is Games whatever the extension; ROM/disc/mod-container extensions are Games; a plain `.zip`
   is Other.
5. Continue/Pinned are localStorage per account, never sent to the server.

## Tests run

- Vitest (`--maxWorkers=2`): `src/study` (27: sidebar grouping, filters, cover extraction with generated ZIP and MP3
  fixtures, frame capture, owner edits, rails, page flows with MSW), `src/lounge/voice`, `src/storage`,
  `src/watchparty`, `src/cinema/tv`, `src/i18n`: green. `tsc --noEmit`, eslint (study, storage/shelves), prettier,
  `check-css-tokens`, `i18n-audit` (TOTAL 0): clean. e2e `tsc --noEmit`: clean.
- Rust: `cargo check -p sr-api --tests` clean; `sr-db --test shelf_repo` (4, incl. the new `shelf_objects` test),
  `secret_columns`, `migration_versions`: pass; `sr-api --test route_matrix`: pass (rows added: GET
  `storage_scope`, PUT `mount_owner`); unit tests of `study.rs` in `sr-api`.
- Pending items are recorded below when run.

## Open owner questions

- Should a guild manager (not only the bucket owner) edit titles/covers/notes on a guild shelf? (Implemented yes.)
- Should Continue Watching/Reading follow the account across devices (server-side) rather than stay in the browser?
