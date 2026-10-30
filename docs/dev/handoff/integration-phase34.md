# Integration: phase 34 (profiles, scrapbook, unfurling, status rings, player skins)

Branch `integration/phase34` (base `main` 779bcdc7), worktree `SIDEROOM_wt/p34-integ`. Nothing pushed, no PR opened,
nothing merged to `main`: the lead does that. This is the last wave before the beta checkpoint (roadmap-p27 section 12).

## Merges (each `--no-ff`, its own commit, in this order)

| # | Branch | Head | Conflicts |
|---|---|---|---|
| 1 | `origin/main` (wave 32, PR #63, e48bb018) | | none |
| 2 | `feat/p34-prf-profiles` (PRF-PRO-001) | 9cbbebd9 | none |
| 3 | `feat/p34-prf-unfurl` (PRF-UNF-001) | adee6864 | none |
| 4 | `feat/p34-prf-rings` (PRF-RNG-001) | 10c46a23 | `sr-db/src/data_export.rs`: wave 32's `tools.json` and the `rings.json` section, both kept |
| 5 | `feat/p34-prf-scrapbook` (PRF-SCR-001) | db3b2b8b | `data_export.rs` (README line and section), `sr-server/src/{app,lib}.rs` (rings and scrapbook jobs and modules, both kept); `openapi.json`/`schema.d.ts` regenerated. Scrapbook's five `authz_matrix.rs` hunks merged cleanly next to prf-profiles' actors |
| 6 | `feat/p34-player-skins` (MUS-SKN-001) | af25881d | none |
| 7-10 | `feat/p34-skin-{valve,tidepool,circuit,study}` (MUS-SKN-002) | fc01aa15, 4b1129ea, 8fcb09f3, 3b9cb10f | none; each redesign's `<Name>Skin.tsx` and css verified identical to its branch (they win over the first versions); `stack` unchanged |

Checked after the merges: migrations 1680, 1681, 1690, 1700, 1710 (only 1710 has a `DO $$ ... END $$;`, both pairs
intact); 1710 rebuilds the moderation `target_kind`/`scope_kind` checks from their current definition with
`status_ring` appended, so it composes with anything earlier (wave 32 did not touch them). Job ids 1690, 1691, 1710 are
unique (no duplicate `*_ID: i64` constant in `sr-server`). No wave-34 branch touched `sr-events`/`sr-gateway`, so the
gateway docs needed no regeneration.

## Integration commits

- **OpenAPI, `schema.d.ts`** regenerated on the merged tree; MSW `mocks:check` up to date (1110 operations); route
  matrix unchanged (no `?`), `KNOWN_CLASHES` empty.
- **Skin extras bound to wave 32's tools** (`apps/web/src/player/toolsWiring.ts`). `PlayerHost` imports it lazily once
  (its own chunk `toolsWiring-*.js`, so the notes store and crypto stay out of the player's first chunk) and calls
  `registerSkinExtras` with `useFocusTimer('pomodoro')` (the drawer's shared timer) and a notes adapter over
  `useNotes()`: the pad edits the newest scratchpad note (the one the drawer's Notes tool opens first), creates one
  when the scratchpad is empty, saves after 800 ms and on unmount. Until the scratchpad has a key (or cannot be
  reached) the pad falls back to the tab-only pad, whose line now reads "Kept in this tab only. Set up Notes in the
  Drawer to save it, encrypted."; with a key it reads "Saved to your encrypted notes.". The Study redesign's two
  readers (`TimerExtra`, `NotesExtra`) both pick up the binding: `player/__tests__/toolsWiring.test.tsx` (4 tests)
  and a browser check (below).
- **One shuffle switch.** The library panel kept its own Shuffle state (applied only when building smart queues)
  while the store gained a shuffle mode; they could disagree. Now `PortState.shuffle` and `PlayerPort.setShuffle`
  carry the store's mode, the panel's button reads and sets it, and a queue started while it is on opens with the
  chosen track and shuffles the rest (each once, order restorable). `buildSmartQueue` is no longer asked to shuffle.
  Valve's A-B loop is local to that skin (no store state); the Valve and Circuit EQ knobs call the store's `setEq`.
- **Authz matrix:** a live text ring per tenant (`ring_id`, shared with the tenant's server) and a text-ring body;
  `rest`'s `SHAPE_CEILING` 45 -> 46 for `POST /users/@me/rings/media` (a raw file the matrix's JSON bodies cannot
  carry). Before this the rightful ring delete and view were 404 and the sweep failed (3 failures).
- `rings.json` added to the data package README.
- e2e `support/playerAudio.ts` `measureFit` now cuts each box by any clipping ancestor inside the panel: Tidepool's
  redesigned wings clip a wave twice their width, which the old check counted as escaping at every size. SVG elements
  are named by their `class` attribute.
- Docs: CHANGELOG `## Phase 34 - 2026-10-30`, decision rows P34-PRF-1/2, P34-RNG-1, P34-SCR-1, P34-UNF-1/2, P34-SKN-1
  (status Default), doc 09 scrapbook row next to the rings row, inventory section "Wave 34" (boxes open).

**Not done: `ring_tier` stays unread.** prf-rings landed `ring_audience` / `viewer_may_see_a_ring_of`, but nothing on
the profile page shows a ring today (no ring field in the page view, no ring in the header or the profile-blocks
renderer). Reading the cap means a new field in the page contract, the property test's table and an app-only
display; that is a feature, not a small wiring change, so it waits for a later wave. The stored tier is still clamped.
A public viewer can never see a ring anyway (ring audiences are friends or servers).

## Gates (this tree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | clean |
| `cargo clippy --workspace --all-targets -D warnings` **including `sr-sdk`** (CMAKE from the brief) | clean |
| `cargo test` sr-unfurl, sr-watchparty, sr-chat, sr-userfeat, sr-moderation, sr-storage, sr-features, sr-packages, sr-db, sr-api, sr-server, sr-admin, sr-events, sr-gateway | 2220 passed, 0 failed (635 ignored = the Postgres suites below and live tests) |
| `sr-api --test route_matrix --test openapi_names` | 6 + 2 pass |
| Postgres (scratch DB, `--include-ignored --test-threads=1`): `sr-packages` (lib 30, db 11, `profile_privacy` 3, 10) | pass |
| Postgres: `sr-db` `data_export` 4, `account_deletion` 4, `data_requests` 13, `self_delete` 4, `secret_columns` 1, `migration_versions` 1, lib 14 | pass |
| Postgres: `sr-userfeat --test rings` 11, `sr-moderation --test rings` 1, `sr-watchparty --test db` 30 | pass |
| Postgres: `sr-api --lib` rich_tests 12, watchparty 6, scrapbook 12, features 15 | pass |
| Postgres: `sr-server --lib` rings 3, scrapbook 13, watchparty 3; `--test web_gate` 19; `--test immutable_rules` 13 | pass |
| Postgres: `sr-server --test authz_matrix` (7, including `every_route_enforces_its_authz_row` and `profile_privacy_holds_over_http`) | pass after the ring seed (first run: 3 failures, see above) |
| `pnpm -r --if-present typecheck` (e2e included), `lint`, `prettier --check .` | clean |
| vitest `apps/web` whole suite, `--maxWorkers=2`, `SR_WASM_REQUIRED=1` | 397 files, 3632 tests pass |
| vitest `packages/profile-blocks` 6, `api-client` 6, `ui` 321, `wasm` 35 | pass |
| `node scripts/build-web.mjs`, `check-bundle-budget.mjs` | ok (app 240.4/300 KB; `player`: no player or music library code in the entrance) |
| `check-entrance-isolation.mjs` | every surface ok: landing 114.0/120, auth 119.6/150, invite 111.8/120, **profile 113.0/150 (2 files)**, legal, error, static. No `status_ring`, `/rings`, `scrapbook`, `skinx`, `BackdropFrame` in `dist/entrance` or the public shells. Also ok on the `VITE_SR_E2E=1` build |
| `check-css-tokens` (65 files), `check-notices-copy`, `check-doc-links`, `check-tools-corpus` (32 tools, 384 phrases), `node --test scripts/*.test.mjs` (86 pass, 2 skipped), workflow YAML parse (all), `render-caddyfile.mjs --check` | pass |
| `i18n-audit` | TOTAL 4: the 3 known studio format names (wave 32) and `placeholder="https://"` in `StartPartyDialog.tsx` (a URL scheme, not prose) |
| Playwright Chromium, dev mode (sr-server 8080, `memory:` storage, scratch DB, Vite 5173): `player-layout.spec.ts` (7: all 8 skins fit at 1280x800, 360x740, 320x640; axe with contrast; backdrop hidden art and reduced motion), `watch-together.spec.ts` (2), `moderation-gate.spec.ts` (with the rings step 3c) | 10 pass (player-layout after the `measureFit` fix; first run 3 failures, all Tidepool's clipped wave) |
| Playwright Chromium, server mode (sr-server serves `dist`, `SR_WEB__SERVE_PUBLIC=true`): `entrance-isolation.spec.ts`, `entrance-smoke.spec.ts`, `entrance-journey.spec.ts` | 37 pass (the journey needs `SR_E2E_SERVER_LOG` set locally) |
| `noindex.spec.ts` (`SR_E2E_NOINDEX=1`) against sr-server without Caddy | not a valid local run: it is written for Caddy (prod-smoke). Without Caddy 11 of 21 fail as expected (redirects and 401s carry no header from sr-server). `/~x` answers 200 with `X-Robots-Tag: noindex, nofollow`; `/api/v1/public/profiles/x` answers 404 with the header |
| Screenshots (dev server on 5185, `e2e/tools/player-skins-preview.mjs`) | 20 PNGs, axe clean for all five skins at both sizes |
| Study Deck real timer (browser check on 5185) | `toolsWiring.ts`, `useFocusTimer.ts`, `timerStore.ts`, `useNotes.ts` loaded; Start took the shared pomodoro from 25:00 to 24:57; the preview has no scratchpad key, so the pad showed the tab-only fallback line |

Scratch databases `sr_p34integ` and `sr_p34e2e` were dropped; no server is left running.

## What CI must prove

- Playwright on three engines: `player-layout.spec.ts` (8 skins, with the changed `measureFit`), `watch-together.spec.ts`
  (Chromium passed locally; Firefox and WebKit never ran), `moderation-gate.spec.ts` with the ring step, the built
  bundle's entrance specs (the `profile` surface's `/~sample` included).
- `noindex.spec.ts` through Caddy in prod-smoke (`/~x` and `/public/profiles/x`).
- `storage.yml`'s new case `s3_scrapbook_live` (Garage; compiled only, never run by anyone).
- The authz matrix and `immutable_rules` on Linux (green here).
- A real YouTube or Vimeo iframe has never been loaded by anyone (the spec stubs the provider); the first real-browser
  check is the owner's.

## Beta checkpoint: what the owner must decide or do before the beta

Gathered from the five handoffs and `owner-notes-2026-10-30.md`.

**Decide (wave 34, defaults in place, see the P34 decision rows):**
1. **Existing profiles become private (important).** No backfill was run. Leave it, add a notice in the profile
   editor, or run a one-time switch to Members only for customised profiles (loosens a privacy default)?
2. Behind Caddy a signed-in member opening `/~handle` sees the public view. Should Caddy proxy `/~*` to `sr-server`?
3. Presence-driven Now playing shows only to signed-in viewers; the public page shows only the typed line. OK?
4. Turn `platform.public_profiles` on for the beta, or keep the walled default?
5. Status rings on by default (`lounge.rings`)? The rolling 24-hour allowance of 20 (vs a midnight reset)? The
   client-claimed clip length (the server checks only the container and 50 MiB)?
6. Scrapbook: delegated upload grant or a real per-server service principal? Should reactions skip private channels
   unless a manager allows them? A per-server total cap besides 200 MiB per file?
7. Unfurling: `media-src https:` or operator-listed hosts only? The `strict-origin-when-cross-origin` referrer
   YouTube needs? PeerTube in Watch Together and hls.js for non-Safari HLS later? Authz-matrix rows for the party kinds?
8. Player skins: which extras next (an ambience mixer needs sound files and a licence; lyrics, sleep timer, tape
   speed)? The Study Deck pad edits your newest note: OK, or a dedicated "player" note?

**Decide (earlier waves, still open in the owner notes):** listening-room titles for members without music access;
member buckets as room sources; the music button switching skins; the house notes setting; `/app` remembering the
last section; jukebox vote-skip percent and hidden tracks; WebM voice messages from Chrome; a second drawer shortcut;
Go to outside the Lounge; storing the notes recovery file on the server; secret dice rolls; server loot tables
readable by every member; shared time zones; the ECB allow-list line; English-only calculator phrases; event card
reminders vs DM settings; deleting old event cards.

**Do:**
- Review and merge this branch (PR title below), then let CI run the items above.
- On a hosted instance: follow `deploy/UPGRADE.md` for the 30b hosting change (mount the web build, re-render the
  Caddyfile), set the same `SR_PEERTUBE_HOSTS` for `sr-server` and the web build if PeerTube should be offered, and
  offer `lounge.scrapbook` at platform scope if wanted.
- Try the real YouTube and Vimeo players once in a browser (nobody could here).
- Give the beta findings to the commander for `docs/dev/beta-findings-p27.md` (roadmap "Beta checkpoint").

## Suggested PR title

`PRF-PRO-001, PRF-SCR-001, PRF-UNF-001, PRF-RNG-001, MUS-SKN-001, MUS-SKN-002: wave 34, profiles, scrapbook, unfurling, status rings and player skins`
