# Handoff: feat/p30-music-lib (wave 30, MUS-LIB-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p30-music-lib`, branch `feat/p30-music-lib`, base `integration/phase30`.
Migration 1550 only. No new dependency, nothing pushed, no PR, no server was started (8081/5174 unused).

## Resume here

Everything below under "Done" is committed. Two things are written but could not run here: the Postgres suite
`crates/sr-db/tests/music_repo.rs` (the password file `%TEMP%\sr_pg_pw.txt` is missing) and the glue to the
player (needs the player-shell branch). Next steps, in order:

1. Merge with `feat/p30-player-shell` and `feat/p30-music-index`; apply the "Integration steps" below.
2. Run `SR_TEST_DATABASE_URL=<own scratch db> cargo test -p sr-db --test music_repo -- --include-ignored --test-threads=1`.
3. Run the full authorization matrix (`every_route_enforces_its_authz_row`) and `immutable_rules.rs`.

## Done

Backend
- `crates/sr-db/migrations/1550_music_library.sql`: `music_playlists` (owner is a member or a server, exactly one,
  checked by the table), `music_playlist_items` (dense `position`, a track may appear twice), `music_favorites`,
  `music_recent`. Track FKs cascade: when the index deletes a `music_tracks` row the items, favorites and history go
  with it. Account and server deletion cascade too. A `vanished` track is hidden from every read but its rows stay.
- `crates/sr-storage/src/music_lib.rs`: the `MusicLibStore` trait, row types and `MemoryMusicLib` (the in-memory
  store the route tests use). `crates/sr-db/src/music_repo.rs`: `PgMusicLib`. Caps are enforced in the store inside a
  transaction (advisory lock per owner for playlists and per member for favorites; row lock per playlist for items),
  so two concurrent requests cannot pass a cap. `record_recent` upserts and prunes to 200 in one transaction.
- Catalog settings `music.max_playlists` (default 100, 1 to 1000) and `music.max_items` (default 5000, 10 to 50000) in
  `sr-admin` `settings.rs`. `music.max_items` also caps a member's favorites (the roadmap names no favorites cap).
- `crates/sr-api/src/routes/music/{mod,playlists,favorites,recent}.rs` (+ `tests.rs`). Routes:
  `GET|POST /users/@me/music/playlists`, `GET|POST /guilds/{id}/music/playlists`,
  `GET|PATCH|DELETE /music/playlists/{id}`, `POST /music/playlists/{id}/items`,
  `PATCH|DELETE /music/playlists/{id}/items/{item_id}`, `GET /users/@me/music/favorites`,
  `PUT|DELETE /users/@me/music/favorites/{track_id}`, `GET|POST|DELETE /users/@me/music/recent`.
  Rules: a member's playlist is the owner's alone (404 for anyone else, never 403); a server playlist is readable by
  members (404 for non-members) and editable with the storage `upload` or `manage` grant (403 for a member without it;
  owner and administrator hold all grants); adding a track (to a list, favorites or history) needs read access to its
  mount now, else 404 (same answer as "no such track"); every read drops tracks whose mount the viewer cannot reach and
  reports the count as `hidden`, so a server playlist never names a friend's track to the rest of the server.
  Caps answer 409 with the field codes `PLAYLIST_LIMIT`, `PLAYLIST_FULL`, `FAVORITES_FULL` under `errors.music`.
  The reach and grant lookup is the `MusicReach` trait; `ShelfReach` is the real implementation over `ScopeDeps` and
  `ShelfDeps` (the same `reachable` call the shelves use, and `grants_for`).
- Wiring: `routes/mod.rs` (module, router, OpenAPI merge), `sr-server/src/app.rs` (`MusicLibDeps` extension),
  `auth/policy.rs` (bots are denied on `/users/@me/music`, `/music/playlists`, `/guilds/*/music/`), 16 rows in
  `crates/sr-api/tests/route_matrix.tsv` (reviewed: `self` for the member's own data, `membership` for the playlist and
  server routes; BOT deny).

Frontend (`apps/web/src/music/`)
- `queue/queue.ts`: the pure smart-queue rules: album, artist, genre, "more like this" (tags 4, genre 3, artist 2,
  album artist 1, same era within 5 years 1; ties keep input order), shuffle with no repeat (a permutation; the chosen
  track first; `nextCycle` never opens with the track that just ended; `shuffleUpcoming` keeps what has played),
  `moveTrack`, `dedupe`. `queue/smart.ts`: builders that fetch from `GET /music/library` (server filters album, artist,
  mount; genre and likeness are decided client-side over a scan of at most 1000 tracks).
- `queue/gapless.ts`: `GaplessDecks`, two `<audio>` decks. The next track is prefetched into the idle deck; a 25 ms
  timer watches the remaining time on the Web Audio clock and starts the incoming deck one measured `play()` latency
  early; the hand-over is scheduled on the audio clock (`setValueAtTime` for a gapless cut, equal-power
  `setValueCurveAtTime` for a crossfade of 0 to 12 s, never more than half of either track). A throttled timer falls
  back to the `ended` event. Pausing or seeking inside a crossfade completes the hand-over.
- `eq/presets.ts`, `eq/eqGraph.ts`: ten bands (31 Hz to 16 kHz, low shelf, eight peaking, high shelf), twelve presets,
  clamp to +-12 dB, a preamp that cuts the signal by the largest boost, 30 ms glide; `createEqChain(ctx)`.
- `panel/LibraryManagerPanel.tsx` (+ `sections.tsx`, `TrackList.tsx`, `panel.css`): the content of the "Full Library
  Manager" Library tab: Library (search through `GET /music/library?q=`, 250 ms debounce), Playlists (own or a
  server's, create, rename, delete, move, remove), Favorites, Recent, Equalizer (presets, ten sliders, crossfade).
  Row actions: play, favorite, play next, add to queue, play album/artist/genre, more like this, add to playlist.
  A Shuffle toggle applies to what is started next.
- `queue/port.ts`: `PlayerPort`, the one door to the player (see below). `queue/recent.ts`: records a play after the
  track has been current for 5 s. `playlists/api.ts`, `search/api.ts`: the API calls.
- i18n: `apps/web/src/i18n/messages/en/p30-musiclib-1.ts` (keys `music.lib.*`).

## Integration steps (the player-shell and music-index branches are needed)

music-lib never imports `player/*` (the branch was not there). The panel talks to a `PlayerPort`. At app start, once:

```ts
// e.g. in player/PlayerGate.tsx or main.tsx, after the player store exists
import { registerLibraryPanel, usePlayerStore, play, enqueue, setEq, setEqEnabled, setCrossfade } from './player/store';
import { setPlayerPort } from './music/queue/port';
import { startRecentTracker } from './music/queue/recent';
import { LibraryManagerPanel } from './music/panel/LibraryManagerPanel';
import { recordRecent } from './music/playlists/api';

let snap = null;
const read = () => {
  const s = usePlayerStore.getState();
  if (!snap || snap.queue !== s.queue || snap.index !== s.index || snap.eq !== s.eq
      || snap.eqEnabled !== s.eqEnabled || snap.crossfadeSec !== s.crossfadeSec) {
    snap = { queue: s.queue, index: s.index, eq: s.eq, eqEnabled: s.eqEnabled, crossfadeSec: s.crossfadeSec,
      state: { queueIds: s.queue.map((t) => t.id), index: s.index, eq: s.eq, eqEnabled: s.eqEnabled, crossfadeSec: s.crossfadeSec } };
  }
  return snap.state;
};
setPlayerPort({ play, enqueue, setEq, setEqEnabled, setCrossfade, getState: read, subscribe: usePlayerStore.subscribe });
registerLibraryPanel(LibraryManagerPanel);
startRecentTracker({ record: recordRecent });   // after setPlayerPort
```

`PortTrack` has the same shape as `PlayerTrack` (`src: ''` plus `resolveSrc`, which presigns through music-index's
`POST /music/tracks/{id}/presign`), so the store takes it as is. The panel must sit inside the app's `QueryClientProvider`
(the lazy player host is mounted in the root layout, so it is) .

Gapless and true crossfade need the engine (player-shell's `engine.ts` / `graph.ts`, not touched here):
1. Create a second `<audio>` and `MediaElementSource`, each through its own `GainNode`, both into the EQ input
   (`graph.bands[0]`; or build the EQ with `createEqChain(ctx)` and connect to `chain.input`).
2. `new GaplessDecks({ clock: ctx, decks: [{audio: a, gain: ga}, {audio: b, gain: gb}], crossfadeSec: () => store.crossfadeSec, onAdvance, onEnded })`.
   `start(track)` on `play`, `setNext(queue[nextIndex])` on every queue/index change and after `onAdvance` (which should
   call the store's `next(true)` without reloading the element), `pause`/`resume`/`seek` on the matching store changes.
3. Replace `rampFade` dips by the decks' gain scheduling when `crossfadeSec > 0`.
4. `createEqChain` is a drop-in for `buildGraph`'s inline filters and adds the preamp and the glide.
Until that is done, playback is the player-shell's single-element engine with its fade dip; everything else works.

OpenAPI, api-client and MSW mocks must be regenerated by the integrator after the three branches merge (see "Gates").

## Tests run

- `cargo test -p sr-api --lib routes::music` : 10 passed (ownership, the server `upload` grant, caps, reach filter,
  order/move/remove, vanished tracks, favorites, recent pruned to 200, router and OpenAPI build).
- `cargo test -p sr-api --test route_matrix` : see "Gates".
- Vitest (`--maxWorkers=2`): `src/music` 72 tests in 6 files (queue rules 18, smart queue 9, gapless swap with a fake clock
  17, EQ graph and presets 9, recent tracker 4, panel 15 incl. axe on search, playlists and equalizer);
  `src/i18n` and `src/__tests__` (catalog gates) green. `tsc --noEmit`, eslint on `src/music`, prettier,
  `scripts/check-css-tokens.mjs` on the panel files: clean.

## Not run (and why)

- `crates/sr-db/tests/music_repo.rs` (8 tests, `#[ignore]`, written to compile against the same API; `cargo test --no-run`
  was run): ownership and the per-owner cap, the table refusing zero or two owners, item order through add/move/remove,
  the item cap refusing a whole batch, unknown and vanished tracks, vanish and delete cascades, favorites cap and
  idempotence, recent pruning to 200, the server grant over the real `PgScopeFacts` (owner, `upload` role, plain member,
  non-member; revoking the grant; deleting the server). Skipped: the Postgres password file is missing. CI runs them.
- The authorization matrix over HTTP (`sr-server` `authz_matrix.rs`) and `immutable_rules.rs`: need Postgres. The new
  routes use `{playlist_id}` and `{item_id}`, which the harness fills with its BOGUS id (404 for everyone); it does not
  seed a playlist, so the rightful-actor calls only observe. If the integrator wants the positive case there, seed one
  playlist for `member` and one server playlist for tenant A.
- No browser run of the panel (8081/5174 not started; the player-shell owns the full stack run). No Playwright case.

## Still open

- Gapless/true crossfade are an engine module with a fake-clock test, not yet wired into the player's engine (above).
- "More like this" uses `tags` when `GET /music/library` returns them (music-index's `TrackView` currently lists
  `genres` but no `tags`); without tags it ranks by genre, artist and era. Genre and likeness scan at most 1000 tracks;
  wave 32's client-side search replaces the scan.
- EQ and crossfade preferences persist in the player store's `localStorage` (player-shell); they are not in the synced
  account prefs. The roadmap says favorites and recently played sync "through the existing user settings sync": they are
  server tables (migration 1550), so they follow the account on every device without that.
- Playlists are not shareable by link and have no cover art or reorder by drag (move up/down buttons only).
- The playlist picker in a row offers the member's own playlists plus the server chosen in the Playlists tab.
- Search is server-side ILIKE only (music-index); no fuzzy or typo-tolerant match.

## Gates

- `cargo fmt --check` on `sr-db sr-storage sr-admin sr-api sr-server`; clippy and the rest are listed in the final
  report with their results.
- Route matrix rows reviewed, no `?`. OpenAPI/api-client/mocks regeneration: see the last commit message of this branch
  or "Resume here" if it is not done.

## Decisions

- Playlists reference `music_tracks` by FK (cascade), not by snapshot: the roadmap's jukebox note stores mount id and
  key, but a guild playlist must keep working when the file is re-tagged, and the index owns the row.
- Favorites cap = `music.max_items`; history cap fixed at 200 (roadmap).
- Server playlists: members read, the storage `upload`/`manage` grant edits (roadmap: "editable with `upload`"); no
  separate "who may create" setting.

## Owner questions

1. Should favorites have their own cap setting instead of sharing `music.max_items` (default 5000)?
2. Should a server playlist show a track only the editor can reach as a greyed row ("not shared with you") instead of
   hiding it and counting it? The jukebox (wave 31) will say "not shared with you"; hiding is the conservative default.
3. Bucket CORS for `GET` with `Range` from the app origin is needed for Web Audio playback (player-shell's note): confirm
   the Garage/instance mount rule includes it.
