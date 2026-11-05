# feat/p47-sketch (ACT-SKS-001, wave 47): paused by the owner

## Resume here
Paused on 2026-11-05 at the owner's request, mid-build. Nothing was run yet: no sr-server, Vite, static server or
database was started (sr_p47_sketch was never created), and no test has been run.

**Done (committed, untested):** the new package `packages/starter-packs/activities/sketch` (workspace package
`@sideroom/activity-sketch`, lockfile updated with `pnpm install --offline`):
- `sideroom.json` declares datachannel, storage, guest_play, lobby_channel, host_handoff and byos_read.
- `shared/`: built-in prompts (about 190, original), fuzzy guess matching and hint mask (`match.js`), pack
  parsing/validation with a 64 KB cap and shelf reference parsing (`pack.js`), and the canvas, palette, ink ops,
  scoring and seeded RNG (`rules.js`).
- `host/engine.js`: the host-authoritative game (lobby, pick, draw, reveal, end, hints, timers, away/left players,
  packs kept in the host page, secrets sent only to the drawer, `resume()` for host_handoff).
- `client/`: index.html, style.css (phone-first, light and dark), main.js (all screens), pad.js (pointer-event
  drawing), net/session.js (host/guest wiring and handoff), channel/sdk-channel/bc-channel/loopback copied from
  Land.io, platform/ (fake and SIDE ROOM, with `loadPack` through `shelfObjectUrl`).
- `tools/bundle.mjs` (copied from Land.io, no origin literals). `node tools/bundle.mjs` builds a valid 22-file bundle.
- `test/engine.test.js` was written but has **not been run**.

**Next, in order:**
1. `pnpm --filter @sideroom/activity-sketch test` and `typecheck`; fix what fails.
2. Remaining tests: match, pack, a guest SDK that throws on member-only commands (as in landio's
   host-guest-sdk.test.js), handoff over the SDK LoopbackHub, and bundle.
3. Register `sketch` in `scripts/build-activities.mjs` (+ test), `e2e/support/activityBundle.ts`
   (`BuiltInActivity`), `docs/ops/test-deploy-quick-tunnel.md`, and a Rust manifest test in
   `crates/sr-activities/src/sandbox.rs` (see arena3d commit 5b45701d).
4. The e2e gate spec (member + guest phone context, like the real-stack part of `e2e/tests/guest-play.spec.ts`)
   and its own CI step in `.github/workflows/e2e.yml`.
5. Inventory section "## Wave 47 — Sketch & Slander (roadmap P28)" in `docs/inventory/21-feature-phase.md`,
   decisions D-460..D-464, threat model T50, then the visual check (AGENTS.md section 4) and the brief's gates.

## Findings so far
- The app already shows a guest-link QR in the Activity panel's Share link dialog (`ShareLinkDialog.tsx`). The game
  cannot show its own QR, because the link's `#key=` is made in the member's page, so the lobby points players to
  Share link instead.
- The guest bridge cuts a guest's capabilities to `datachannel` only (no storage), so the game guards all storage use.

## Owner questions (defaults taken)
1. Pack hosting: a presigned shelf URL is on the storage host, which the sandbox CSP blocks unless the manifest's
   `connect` list names it (byos-read-activities.md, option 1). A built-in game can't know that host. Default:
   no `connect` entry. Option 3 (bytes passed through the bridge) is recommended.
2. Choosing a pack: there's no shelf picker in the SDK, so the host pastes "shelf id / file hash". A
   `pickShelfObject` bridge command would be better.
3. Guild packs stay in the host's page and are not handed to a successor. After a handoff the game uses only the
   built-in prompts.
4. Nicknames: members start with their display name (never the handle) and guests with their callsign. Both can
   rename in the lobby.
