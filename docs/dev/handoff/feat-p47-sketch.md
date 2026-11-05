# feat/p47-sketch: Sketch & Slander (ACT-SKS-001, wave 47)

Builder "sketch". Branch `feat/p47-sketch`, based on main `d7cf7380`. Not pushed, no PR.

## Status
Done, apart from what "Not done" lists. The owner paused the wave once (commit `3c9e2f44`) and then resumed it. The
coordinator lifted the 17:30 clock stop for today. My processes are stopped (sr-server, Vite, the scratch static
servers) and `sr_p47_sketch` is dropped.

## What was built (commits, oldest first)
- `3c9e2f44`: the `packages/starter-packs/activities/sketch` package (workspace `@sideroom/activity-sketch`, with a
  lockfile entry made by `pnpm install --offline`; no new dependency). It contains:
  - `shared/`: the original built-in prompts (about 190), fuzzy matching and the hint mask, pack validation, and the
    canvas, ink, scoring and RNG rules.
  - `host/engine.js`: the host-authoritative game with `resume()` for `host_handoff`.
  - `client/`: the screens, a pointer-event pad, the session (host or guest wiring, handoff), and the platform seam
    (fake and SIDE ROOM).
  - `tools/bundle.mjs`, copied from Land.io.
- `4b224fee`: tests and typecheck fixes.
- `8c956bd8`: **`shelfObjectBytes`** (D-460, the coordinator's default for the CSP question).
  - It is a new SDK command (`protocol.ts`, `sdk.ts`) with a member bridge in `apps/web/src/activities/host.ts`.
  - It has the same `byos_read` check, the same member yes (shared with `shelfObjectUrl`) and the same 41-C mint
    route.
  - The member's page then fetches the presigned URL with `credentials: 'omit'`, caps it at 1 MiB and hands the
    frame base64.
  - The game keeps `connect-src 'none'`. The command is not on the guest allow-list.
  - The app page fetching a presigned URL is the same thing the emulator's ROM fetch already does.
- `8fc72a5a`: registration in `scripts/build-activities.mjs` (+ test), `e2e/support/activityBundle.ts`,
  `docs/ops/test-deploy-quick-tunnel.md`, and the Rust test
  `sandbox::tests::the_sketch_manifest_survives_review_with_no_connect_hosts`.
- `8de4af2e`, `493e5cc9`: the gate spec `e2e/tests/sketch-gate.spec.ts`, its CI step "Sketch & Slander gate
  (Chromium)" in `.github/workflows/e2e.yml` (its own server run, log `server-sketch-gate.log`), and the shelf dialog
  text, which now says the game may get the file itself.
- `86b9d344`: decisions D-460 to D-464, threat row T50, the inventory section "Wave 47 — Sketch & Slander (roadmap
  P28)" (ACT-SKS-001 is left unticked until the CI gate runs), the byos-read design note (option 3 is now built),
  README and NOTICE.
- `cbd826ff`: fixes from the visual check: phone hint wrapping, the empty timer pill, the podium with fewer than 3
  players, the Skip label, the "Playing on a phone?" tip hidden for guests, and no guess box for the drawer while
  picking.
- `62e88444`: two fixes found by the gate spec on the real stack:
  - The Activity sandbox has no `allow-forms`, so the guest's guess never submitted. Every "form" is now a button
    plus Enter.
  - A member without a display name showed their handle. They now start as "Player" (D-462).
- `3efefdda`: the gate spec logs who drew.
- `505a2478`, `1a838e82`: the palette now fits a guest frame narrower than 360 px. The gate spec can also take
  opt-in screenshots of the app surfaces.

Arcade: not touched (another builder is redesigning it). Built-in Activities are uploaded as bundles, so no Arcade
listing data needed changing.

## Tests run (all green)
- Sketch: vitest 27 (engine 13, guest SDK round and pack path 3, matching and packs 7, bundle 4), tsc, and
  `tools/bundle.mjs` (22 files, every check).
- `node --test scripts/build-activities.test.mjs` (6). `node scripts/build-activities.mjs sketch`.
- SDK: vitest 37 and eslint. Web: vitest for `src/activities`, `src/public/guest-play` and `src/i18n` (206, including
  the 2 new `shelfObjectBytes` tests and `guest.test.ts`, which walks every protocol command). Also web tsc, eslint on
  the changed files, prettier, `check-css-tokens`, `i18n-audit` (0) and `check-ops-files`.
- Web build, then `check-entrance-isolation` (clean) and `check-bundle-budget` (all ok; guest-play 114.7 KB of 120).
- Rust (`sr-activities`): `cargo fmt --check`, `cargo clippy -j 2 --tests -D warnings`, and the sandbox tests (14).
  Nothing else in Rust changed.
- **The gate spec on the real stack** (my sr-server with voice, activities and the guest relay, plus Vite on 5201):
  1 + 3 + 4 runs passed, with both branches (the guest phone drew, the member drew). It also passed twice more with
  screenshots on.
- `guest-play.spec.ts` against this branch: 8/8. The ingame_voice case passed on a fresh server after my many runs
  hit the per-run registration cap.
- `playwright test --list` loads every spec (903 tests, 67 files). e2e tsc is clean.

## Visual check (AGENTS.md section 4)
Screenshots are in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p47-sketch/`:
- `game/`: the game's own screens (lobby as host and as guest, pick as drawer and as waiting player, draw as drawer
  and as guesser, reveal, end). Desktop 1280x800 and phone 375x812, light and dark, taken with a scratch driver over
  the fake platform. No page errors and no horizontal overflow. `game-before/` has the first pass, before the fixes.
  `game-narrow/` is the 335 px guest frame.
- `app/`: from the real stack through the gate spec (`SR_SKETCH_SHOTS=<dir> SR_SKETCH_THEME=light|dark`). Each in
  Lounge Light and the dark default:
  - the Activity panel with the game (lobby, draw, reveal) on desktop and phone;
  - the Share link dialog with its QR code on desktop and phone;
  - the guest join page and the game on a phone (lobby, draw, reveal).
- I did not run `page-sweep.spec.ts`. No app route or page component changed, only the Activity bridge and one
  dialog string. The app surfaces this wave touches are the ones the gate spec shoots. At 375 px the app clips the
  bottom of the Activity iframe in the call view. That is the app's layout, not this wave's.

## Not done
- The gate spec has not run on CI. It needs the voice stack and the guest relay, as `guest-play.spec.ts` does.
  ACT-SKS-001 stays unticked until it does.
- No live two-browser host handoff E2E. Handoff is covered by `engine.test.js` (resume, reprompt, skip).

## Owner questions (for Fable; defaults taken)
1. **D-460 `shelfObjectBytes`** is the coordinator's default over widening the Activity CSP. The defaults inside it:
   - a 1 MiB cap;
   - the member yes is shared with `shelfObjectUrl` (one dialog per object and frame);
   - the app page fetches under its own CSP, so the storage host must be same-origin or in the app's
     `extraConnect`, as for emulator ROMs.

   Is that the shape the owner wants?
2. Choosing a pack: there's no shelf picker in the SDK, so the host pastes "shelf id / file hash" (or a link holding
   both). A `pickShelfObject` bridge command would be friendlier.
3. Guild packs stay in the host's page and are not handed to a successor after a handoff. The successor plays the
   built-in set until a member loads the pack again (D-461).
4. Names (D-462): a member whose display name is empty or equals their handle starts as "Player". Everyone can rename
   in the lobby. Guests keep the relay's callsign as their default.
5. Game defaults (D-463): 3 rounds, 80 s to draw, 15 s to pick, 2 to 16 players, the scoring numbers, and wrong
   guesses shown to everyone.
6. Joining by phone (D-464): the Activity panel's existing Share link QR, not a QR in the game, because the game
   never sees the link's key.
7. The game's text is English in the bundle, like the other built-in Activities. The bundle has no i18n.

## Resume here
Nothing is pending on the branch. Next is integration: run the CI e2e job (the "Sketch & Slander gate" step), then
tick ACT-SKS-001 with the run link. The integrator re-shoots `app/` with `SR_SKETCH_SHOTS` on the merged tree.
