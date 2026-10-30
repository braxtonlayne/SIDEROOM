# Integration handoff: integration/phase30 (wave 30)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p30-integ`, branch `integration/phase30`, base 6f7e48ef (main at PR #58).
Env: `CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p30-integ-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one
cargo command at a time. No Postgres (the password file `%TEMP%\sr_pg_pw.txt` is missing). Nothing pushed, no PR,
nothing merged to main; the branch has no upstream.

## Resume here

Everything that needs no database is done and committed (see Gates). What remains is CI: the Postgres suites and the
browser specs under "Left for CI". Nothing is half-finished; the follow-ups below are new work.

## Merges (each `--no-ff`, its own commit)

1. `feat/p30-music-wasm` (843386d5, MUS-WASM-001, lofty =0.25.4 already in `Cargo.lock`): clean.
2. `feat/p30-player-shell` (9a4047bc, MUS-PLY-001): clean.
3. `feat/p30-music-index` (53ea137c, MUS-IDX-001, migration 1540): clean.
4. `feat/p30-music-lib` (7f9892d5, MUS-LIB-001, migration 1550): conflicts, resolved in the merge commit 1d6e7ffe:
   - `crates/sr-api/src/routes/music/mod.rs` (add/add): one module with both halves, the index's `MusicDeps`,
     `reaches`, `may_write` and the library's `MusicLibDeps`, `ShelfReach`, caps and views; one router and one
     OpenAPI doc (`MusicApiDoc`). The two `not_found()` helpers became one (the library's, with a message).
   - `crates/sr-db/src/music_repo.rs` and `crates/sr-db/tests/music_repo.rs` (add/add, two different stores): the
     index keeps `music_repo` (`PgMusicStore`); music-lib's store moved to `music_lib_repo.rs` (`PgMusicLib`) and its
     Postgres suite to `tests/music_lib_repo.rs`. `sr-server` and the doc references follow.
   - `crates/sr-server/src/app.rs`: both extensions.
   - `crates/sr-api/tests/route_matrix.tsv`: both row sets (21 music rows), sorted.
   - `auth/policy.rs` merged without a conflict but with two copies of the denial; now one `/music/` prefix plus
     `/users/@me/music` and `/guilds/*/music/`.
   - OpenAPI, api-client and MSW: taken from one side, then regenerated after the last merge (1043 operations,
     `mocks:check` green).
   - `THIRD_PARTY_NOTICES.md` (both copies identical), the i18n fragments and the CHANGELOG merged without conflicts.

## Wiring (the integration's own work)

- **Library panel and port** (`apps/web/src/player/musicWiring.ts`, imported by the host chunk only):
  `createStorePort()` forwards music-lib's `PlayerPort` to the store's actions (a stable snapshot for
  `useSyncExternalStore`); `registerLibraryPanel` gets `LibraryManagerPanel` as its own lazy chunk (the Library skin
  wraps it in `Suspense`); `startRecentTracker` starts once per page load. All three run when the player host first
  mounts, which happens only on a signed-in app route. Tests: `player/__tests__/musicWiring.test.tsx`.
- **Gapless and crossfade** (`player/engine.ts`, `player/graph.ts`, `music/queue/gapless.ts`): the host renders a
  second `<audio>` (`data-testid="player-audio-next"`). With Web Audio the engine runs `GaplessDecks`: the next track is
  resolved (`resolveSrc` presign) and prefetched into the idle deck, the hand-over is scheduled on the audio clock (a
  cut, or the equal-power crossfade of `crossfadeSec`, 0 to 12 s, never more than half a track), and the store follows
  a hand-over without reloading the element. player-shell's fade-out/fade-in (`rampFade`, the `fade` node) is gone.
  Without Web Audio the one-element path and its `ended` cut are unchanged. `GaplessDecks.start` gained
  `{ paused }` (a parked queue after a reload) and returns the `play()` promise, so a refused autoplay still pauses.
  The PWA update deferral: nothing is lined up behind a deferred update, and an update deferred after the next track
  was lined up reloads at the hand-over with the queue parked at the new track. Media Session is untouched (it uses
  the store's actions). `__srPlayer.audio` (dev/test handle) is the deck playing now. Tests:
  `player/__tests__/gapless-engine.test.ts` (fake Web Audio context and clock: the decks into one chain, prefetch,
  hand-over without reload, the curves, a queue change, pause/resume, both update cases, the EQ, the end).
- **One EQ.** The graph is deck gains -> music-lib's `createEqChain` (preamp headroom, ten bands, 30 ms glide) ->
  master -> analyser -> destination. The store's `eq`/`eqEnabled` are the one source of truth (the skins, the
  Equalizer section through the port, the saved settings); `player/types.ts` `EQ_BANDS`/`EQ_RANGE_DB` are
  `music/eq/presets.ts`'s `EQ_FREQS`/`EQ_MAX_DB`.
- **Tagger** (`music/panel/tagging.tsx`): the library panel's first listing with a track starts
  `getTagger().start()` once per page load (every reachable mount; the tagger's own paging is by mount and album) and
  refetches the music queries when it wrote tags; `RescanTagsButton` is shown under the listing for every bucket whose
  tracks carry `can_edit` (a "Bucket n" label when there are several). `music/index/types.ts` now imports
  `MusicTags`, `MusicCover`, `MusicNeedMoovError` and `MusicApi` from `@sideroom/wasm` (types only); the loader is
  `const { loadMusic } = await import('@sideroom/wasm')`. Tests: `music/panel/__tests__/tagging.test.tsx`.
- **Bundle budget** (`scripts/check-bundle-budget.mjs`, `bundle-budget.json` `musicWasmMaxBrotliKB: 600`): a `music`
  row (exactly one `music_bg-*.wasm`, not reachable from first paint, within 600 KB brotli: 197.9 KB here, built
  `--no-opt`; CI's wasm-opt build is smaller) and a `player` row (no player or library code in the entrance, by
  string markers; it fails on the player-shell branch's build, which had the store in the entrance).
- **D-385, the entrance.** player-shell's `PlayerGate` imported `player/store` statically, and so did the sign-out
  path in `settings/user/api.ts`, which put the store into the entrance (+1.78 KB gzip). Now: the router renders the
  gate as its own lazy chunk only when the path is not public (`isPublicPath`), the gate reads a tiny presence flag
  (`player/presence.ts`, kept current by the store) and forgets the queue on unmount, sign-out imports the presence
  module dynamically, and the dev-only `__srPlayerActions` handle loads the store by dynamic import. Nothing of the
  recent tracker, the tagger or the panel is reachable from the entrance (the `player` row and a static import guard
  in `musicWiring.test.tsx`). Entrance, same machine and build: **247.90 KB gzip at 6f7e48ef (253 853 B), 249.68 KB
  on the player-shell branch, 248.35 KB now (254 311 B, +458 B)**. The remaining bytes are the gate's lazy stub, the
  public-path rule and player-shell's PWA update guard in `pwa/state.ts` (needed by the update toast); the full split
  is the later wave.
- **Bucket CORS** (`sr-storage` `s3admin.rs`, `cors_check.rs`): the written rule (Apply CORS, `sr-cli storage
  ensure-cors`) now allows the request headers `content-type` and `range`; `Content-Range`, `Accept-Ranges` and
  `Content-Length` were already exposed and `GET` allowed. The check and the daily `storage.cors_recheck` require
  `content-type` only (`CORS_REQUIRED_HEADERS`), so mounts set up before wave 30 are not marked failing: the media
  element's own ranges and the tagger's `bytes=a-b` reads are CORS-safelisted. Manual steps name the full rule.
  Documented in `deploy/GARAGE.md` and `docs/02-architecture.md`. Tests: two new `cors_check` unit tests.
- **Decision P30-MUS-1** in `docs/01-decisions-log.md` (hand edits win, JPEG covers at `covers/<hash>.jpg`, the rescan
  route, hidden-count server playlists, favorites under `music.max_items`, skin and dock per browser).
- **e2e.** `e2e/support/playerAudio.ts` did not typecheck (`for...of` over a `NodeListOf` without `DOM.Iterable`;
  `pnpm -r typecheck` in web.yml would have failed): `Array.from`. `player.spec.ts` expects two `<audio>` elements
  (the decks). `e2e/tools/player-preview.mjs` mocks the music endpoints with a sample library and saves the panel shots.
- **Migrations 1540 and 1550** read by eye: no `$$`, no function bodies, only `ALTER TABLE`, `CREATE TABLE` and
  `CREATE INDEX`; the 1540 `CHECK` regex is a plain single-quoted literal. No gateway event and no job was added, so
  the gateway docs need no regeneration.
- **Found in Chromium and fixed** (commit 44fe2f47): (1) the engine's resume branch ran on every clock report while
  playing, so at a hand-over the outgoing deck (paused by the browser at its end) was played again and the AbortError
  paused the whole player; resume now runs only when the store turns to playing and an AbortError is not a pause;
  (2) the gate's unmount cleanup cleared a queue started right after it mounted (strict mode); a history listener,
  installed once per router, now ends the music on a public page; (3) a second engine on the same elements (strict
  mode) failed `createMediaElementSource` and fell back to one deck; the graph is now reused per element pair. Each
  has a Vitest. Also the library search field was 10 rem tall (a flex basis in a column; panel.css).
- **cargo deny**: `paste` (RUSTSEC-2024-0436, unmaintained, no vulnerability) arrived with lofty and failed
  `cargo deny check`; `deny.toml` ignores it with a reason, like the MLS provider's proc-macro. Owner: confirm.
- Inventory: the five wave-29 rows ticked on PR #58's green CI (30 checks, merge 6f7e48ef); the wave-30 section added,
  all four rows open. CHANGELOG: `## Phase 30 - 2026-10-29`.

## Gates (no database)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -- -D warnings` (`-j 2`) | ok |
| `cargo test -p sr-music-wasm` | 21 passed |
| `cargo test -p sr-storage --lib` | 145 passed (incl. the two new CORS cases) |
| `cargo test -p sr-admin --lib` | 66 passed |
| `cargo test -p sr-api --lib` | 736 passed, 16 ignored (Postgres) |
| `cargo test -p sr-api --test route_matrix --test openapi_names` | 6 + 2 passed |
| `cargo test -p sr-db --tests --no-run` | compiles (music_repo, music_lib_repo) |
| `cargo deny --offline check` | ok after the RUSTSEC-2024-0436 ignore (paste via lofty; it failed before) |
| OpenAPI, api-client, MSW (`mocks:check`) | regenerated, 1043 operations, up to date |
| web `tsc` (all packages, `pnpm -r typecheck`) | ok after the e2e `playerAudio.ts` fix |
| `pnpm -r lint` (eslint), `prettier --check .` | ok |
| `i18n-audit`, `check-css-tokens` (50 files), `check-notices-copy` | ok |
| vitest src/player, src/music, src/pwa, src/i18n, src/settings, src/__tests__, src/help, router | 88 files, 736 passed (then src/player + src/music again after the browser fixes: 198 passed) |
| vitest `packages/wasm` | 5 files, 19 passed |
| `vite build` + `check-bundle-budget.mjs` | ok; entrance 248.35 KB; `music` 197.9 KB brotli lazy; `player` none in entrance |
| licence allow-list | ok |
| `node --test "scripts/*.test.mjs"` | 58 passed |
| repo checks: PR title, doc links, ops files, secret scan (tree and diff), inventory progress | ok |
| workflow YAML parse (js-yaml) | all ok |
| Playwright `player-layout.spec.ts`, Chromium, local (scratch config without the DB global setup, deleted) | 6 passed |
| Chromium by hand (preview tool, 4 s tones, 1 s crossfade) | three tracks hand over on alternating decks, index follows, no reload |

## Left for CI

- Postgres: `cargo test -p sr-db --test music_repo -- --include-ignored --test-threads=1` (3, MUS-IDX-001) and
  `--test music_lib_repo` (8, MUS-LIB-001); the authz matrix `every_route_enforces_its_authz_row` (21 new rows; the
  playlist routes use the harness's bogus ids, so the rightful-actor calls only observe) and `immutable_rules.rs`;
  migrations 1540 and 1550 applied on a real database.
- `wasm-pack test --headless --chrome` for `sr-music-wasm` (wasm.yml).
- E2E (e2e.yml main job, Chromium, Firefox and WebKit, dev server on 5173 against the full stack):
  `player.spec.ts` (signed in as the Owner through `solo`) and `player-layout.spec.ts` (mocked session at
  `/preview-host`, no API use; the global setup still runs). Neither was ever run in Firefox or WebKit (player-shell
  could not launch them locally): the tone is an 8-bit PCM WAV served by `page.route`, the player builds a Web Audio
  graph after the first click, and WebKit's AudioContext may start suspended (the element still advances). If one of
  them fails only there, the case can be skipped for that engine with a reason; the playback logic is covered by
  Vitest. `player-layout.spec.ts` passed locally in Chromium; `player.spec.ts` needs the signed-in stack.

## Follow-ups

- **No way into the library with an empty queue.** The Library Manager lives inside the player, which mounts only
  once something is queued, and nothing outside the player queues music yet (the shelves have no Music section;
  `createShelfRoutes` excludes `music`). A launcher (a Music entry in Library or the user menu that opens the player
  in the library skin) is new UI and needs a place decided.
- The next track is presigned when the current one starts (to prefetch it). A presign that expires before a very long
  current track ends leaves the prefetched deck with a dead URL for its later range reads; re-presigning a lined-up
  track older than half the presign TTL would close that.
- "More like this" has no `tags` from `GET /music/library` (music-index's `TrackView` lists genres only).
- The wave-28 upload modal for the music prefix is not wired (the cover upload uses the manager directly).
- Ogg and MP3 durations without TLEN are not available from a head read (music-wasm).
- Positive authz cases for playlists need a seeded playlist in the matrix harness (music-lib's note).
- Skin, dock and EQ are per browser (P30-MUS-1 (6)); syncing them needs the prefs schema.
- The entrance still carries player-shell's PWA update guard; D-385's split (later wave) may move it.

## Owner questions (from the handoffs, defaults recorded in P30-MUS-1)

1. Favorites with their own cap instead of sharing `music.max_items`?
2. Server playlists: hide a track the viewer cannot reach (now) or show it greyed as "not shared with you"?
3. Confirm hand edits should always win over "Rescan tags".
4. The `deny.toml` ignore of RUSTSEC-2024-0436 (paste, through lofty).
5. Where should the music library be opened from (see the first follow-up)?

## Screenshots

`C:UsersraxtDesktopSIDEROOMscreenshotsp30-player3` (deviceScaleFactor 2, Lounge Dark, mocked session and sample library, `node e2e/tools/player-preview.mjs` against the dev server on 5173, stopped after):

- `player-retro.png`
- `player-retro-fullviewport-1280x800.png`
- `player-pill.png`
- `player-library.png`
- `library-panel-search.png`
- `library-panel-playlists.png`
- `library-panel-playlist-open.png`
- `library-panel-equalizer.png`
- `library-panel-fullviewport-1280x800.png`
- `player-retro-360.png`
- `player-retro-fullviewport-360x740.png`
