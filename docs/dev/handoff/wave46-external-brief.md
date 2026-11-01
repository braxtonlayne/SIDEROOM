# Wave 46 brief for an outside agent: 3D starter kit and Land.io polish

You are building wave 46 of SIDE ROOM's roadmap P28 (`docs/dev/roadmap-p28.md`, "Wave 46" and changes 11–12). This brief is self-contained: everything you need is here or in the two folders you own. You work only inside those two folders. Anything that needs a change elsewhere goes on the **follow-up list** in your report, and the project's own agents do it later.

SIDE ROOM is a self-hosted chat app. "Activities" are games that run in a sandboxed iframe inside a voice call and talk to the app only through the Embedded App SDK. Land.io is one of the built-in Activities, and the 3D starter will be another.

---

## 1. What you may touch (and nothing else)

| Path | What you do there |
|---|---|
| `packages/starter-packs/activities/arena3d/**` | **New.** The WebGPU 3D starter kit (46-B). |
| `packages/starter-packs/activities/landio/**` | Land.io polish (46-A). |
| `pnpm-lock.yaml` | Only the lines pnpm itself writes when you add `jolt-physics` to `arena3d`. Never edit it by hand. |
| `docs/dev/handoff/feat-p46-external.md` | **New.** Your report (section 7). |

**Do not touch anything else.** That includes `apps/`, `crates/`, `e2e/`, `scripts/`, `.github/`, `packages/embedded-app-sdk/`, the other activities, `docs/` (except your report), `pnpm-workspace.yaml` and the root `package.json`. If you find something outside your folders that needs changing, don't change it: write it on the follow-up list with the file, the change and why.

Git:
- Work on a branch named `feat/p46-external`, based on `main`.
- Write small commits. Each subject starts with `ACT-A3D-001` (the starter) or `ACT-LND-002` (Land.io).
- Don't merge, and don't push to `main`. Don't force-push anything.

---

## 2. Hard rules

1. **No new dependencies except one.** The only approved download is **`jolt-physics` 1.1.1** (npm, MIT), for `arena3d`. Pin it exactly with no caret: `"jolt-physics": "1.1.1"`. Install with `pnpm install` from the repo root; pnpm is `npx pnpm@10.12.1`, with `npm_config_manage_package_manager_versions=false` set.
   - Dev tools may only be versions the lockfile already has: `typescript ~5.6.3`, `vitest ^3`, `@types/node ^22`. Copy them from Land.io's `package.json`.
   - Not three.js, not gl-matrix, not `@webgpu/types`, nothing else. If you think you need another package, stop and put it on the owner-question list.
2. **Bundle rules.** The server refuses a bundle that breaks these; `landio/tools/bundle.mjs` enforces the same rules, so copy and adapt it.
   - At most 2000 files and 32 MiB. Paths at most 256 bytes, and each path segment at most 128.
   - Allowed extensions: `html js mjs css json map wasm png jpg jpeg webp gif svg ico woff2 mp3 ogg webm txt`. A `.glb` is **not** on the list, so ship models as `.json` with base64, or as `.bin` renamed to an allowed type. Simplest: put the glTF JSON in a `.json` file and its buffers base64 inside it.
   - **No absolute URLs anywhere** (`http://`, `https://`, `wss://`, protocol-relative `//`). Everything loads relative to the bundle.
   - The page's CSP (`crates/sr-activities/src/sandbox.rs`, `csp()`) is `sandbox allow-scripts allow-pointer-lock; default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; media-src 'self' blob:; worker-src 'none'; ...`. So WASM is allowed, but there is **no CDN, no `eval`/`new Function` and no Web Workers** (`worker-src 'none'`): physics and rendering run on the main thread. Pointer lock is allowed.
   - The iframe is a sandbox with an **opaque origin**: no `localStorage` (use the SDK's `storage` capability), no `SharedArrayBuffer`, no cross-origin isolation.
3. **Jolt flavour.** Use `jolt-physics/wasm-compat`, the single-threaded build with the WASM embedded. The multithreaded flavours need `SharedArrayBuffer`, which the sandbox doesn't have. Lazy-load it (dynamic `import()`) so the menu paints before physics loads.
4. **Determinism is the point.** Same inputs must give the same physics state on every client:
   - Fixed timestep (60 Hz) and integer tick numbers.
   - Inputs are small integer enums or quantised values, never raw floats from the mouse.
   - No `Math.random()` in the simulation; use a seeded PRNG.
   - No wall-clock time in the simulation.
   - Create, add and step bodies in a stable order.

   The npm Jolt build is compiled without Jolt's `CROSS_PLATFORM_DETERMINISTIC` switch. If your replay test shows Chromium, Firefox and WebKit disagreeing, **stop and report it**. Don't switch engines or work around it silently. The fix (self-building Jolt with that switch) needs the owner's yes.
5. **Guests.** Both games must work for a guest on a `/play/<token>` link. A guest's bridge has a reduced command set: only the data channel, `ready` and `storage` are safe to rely on. A guest must never see member data, so don't call `authorize`/`authenticate`/`getChannel` on the guest path. Land.io's `client/platform/sideroom-platform.js` shows how it handles this.
6. **Nothing that looks third-party.** No real-brand names, logos, fonts, models, textures or sounds. Make every asset yourself (procedural meshes, canvas textures, synthesised sound) or make it CC0 and record it in `NOTICE.md`. Bot names come from the roadmap: `Lumen bot` and `Tofu bot`.
7. **Never ask for, read or print secrets.** You don't need a database, a server or any credentials for this wave.

---

## 3. How Activities work here (just enough)

- Each built-in Activity is a folder under `packages/starter-packs/activities/<name>/`. A build (`node tools/bundle.mjs`) writes the bundle root to `dist/`: `index.html` and the manifest `sideroom.json`, both at the root, plus the JS and assets.
- **`sideroom.json`** (at most 8 KiB): `id`, `name`, `version`, `description`, `tier` (`TIER_1_ORIGINAL`), `capabilities`, `min_participants`, `max_participants`. Land.io's is a good model.
- **Capability flags (D-399).** Accepted names:
  - Active: `datachannel`, `storage`, `byos_read`, `guest_play`, `lobby_channel`.
  - Reserved (accepted, but refused at runtime): `ingame_voice`, `byos_write`, `asset_cache`, `external_launch`.

  `guest_play` needs `tier: TIER_1_ORIGINAL`.
- **The SDK** (`packages/embedded-app-sdk/src/`) is vendored into the bundle as `sdk/*.js` by the bundle tool. Read it, but don't edit it. What you'll use:
  - `new SideRoomSDK(applicationId)`, then `await sdk.ready()`, which returns the config, including `capabilities`.
  - `sdk.self`, `sdk.hostId` and `sdk.isHost`. The host is the member who launched the instance, and it is the authority.
  - The data channel: `sdk.state.get/set/all/delete` (only the host writes `host:*` keys), `sdk.input.send(bytes)` (peer to host) and `sdk.input.reply(member, bytes)` (host to peer).
  - `sdk.storage.install(window)` gives the page a `localStorage` backed by the host: 64 keys, 32 KiB per value.
- **The `applicationId`** is the first label of the iframe hostname (`<id>.activities.<host>`). Outside SIDE ROOM (plain `vite`/static server, tests) use a fake platform. Land.io has `client/platform/fake-platform.js` plus loopback and BroadcastChannel transports; copy that pattern.
- **Crash ticks (wave 45).** The app's crash reporter already records the bridge commands an Activity sends, so you don't wire anything into the app. Do expose your own input log and state hash, as in section 4.

---

## 4. 46-B: the WebGPU 3D starter kit (`arena3d`, ID `ACT-A3D-001`)

A turn-key template someone copies to start a low-poly multiplayer 3D game: the future flagship FPS. Build it small, clean and well commented, with a playable demo on top.

**Layout:**

```
arena3d/
  package.json           name "@sideroom/activity-arena3d", private, type module; scripts build / typecheck / test
  sideroom.json          tier TIER_1_ORIGINAL; capabilities ["datachannel","storage","guest_play","lobby_channel","byos_read"],
                         with ingame_voice and byos_write in a commented example (D-399 reference)
  NOTICE.md              provenance; every asset made here or CC0
  README.md              how to copy the starter into a new game, what each module does, how to run it
  tools/bundle.mjs       adapted from landio's (same checks)
  sim/                   deterministic simulation: no DOM, runs in Node and the browser
  net/                   host-authoritative networking over the SDK data channel + client-side prediction
  render/                WebGPU renderer, with a WebGL2 fallback when navigator.gpu is missing
  assets/                asset loading behind one interface
  client/                index.html, input, camera, HUD, menu
  platform/              sideroom + fake platform (copy landio's pattern)
  replay/                replay harness page + Node runner
  test/                  vitest
```

**Must have:**
1. **Simulation (`sim/`).**
   - Jolt world setup: static level geometry, the player as a capsule character controller with move, jump and crouch, and a few dynamic boxes to push around.
   - 60 Hz fixed step and a seeded PRNG.
   - `stateHash()`: a stable hash over the bodies' positions, rotations and velocities, read back from Jolt and quantised, plus the game state.
2. **Networking (`net/`).**
   - The host runs the authoritative sim. Peers send quantised input frames with `sdk.input.send`.
   - The host sends snapshots and acks with `reply` or `state`.
   - Peers predict locally and reconcile by rewinding to the acknowledged tick and re-simulating.
   - Interpolate remote players.
   - The wire codec is compact binary (`DataView`), with a version byte.
3. **Rendering (`render/`).**
   - A small WebGPU forward renderer: one directional light, flat or vertex-colour shading (low-poly look), a depth buffer and a basic camera.
   - The same scene works under WebGL2 when WebGPU is unavailable (headless CI usually has no WebGPU).
   - No third-party math library: write the few `mat4`/`vec3`/`quat` helpers you need.
4. **Assets (`assets/`).**
   - An `AssetSource` interface with a `BundledAssetSource` that loads from the bundle (relative URLs).
   - A minimal glTF 2.0 loader for meshes, node transforms and basic materials, with buffers embedded as base64 in a `.json`. Skins and animation are optional.
   - Textures: PNG or JPEG through `createImageBitmap`. KTX2/Basis is out of scope, because it needs a transcoder download; list it as a follow-up.
   - Reading `.glb`/`.ktx2` from a member's shelf (`byos_read`, through short-lived per-object URLs) **has no SDK command yet**. Write a `ShelfAssetSource` stub that throws "not available yet" behind the same interface, and put the SDK command on the follow-up list.
5. **Spatial voice hooks.** `ingame_voice` is reserved, so there is no voice. Export a small interface, `onListenerMove(pos, forward)` and `onSpeakerMove(memberId, pos)`, that the game calls every frame, with a no-op implementation. Document it as the hook a future voice capability plugs into.
6. **Replay and determinism (the gate proof).**
   - `replay/` has a recorded input script, at least 600 ticks with 2 players plus dynamic bodies colliding.
   - `test/replay.test.js` (Node) runs it in **two separate Jolt instances** and asserts identical `stateHash()` at every 60th tick and at the end. This test is the gate.
   - `replay/replay.html`, built into `dist/`, runs the same script in the browser and writes the final hash into the page (`<output id="hash">`). The project's agents will add a Playwright spec that opens it in Chromium, Firefox and WebKit and compares the hashes. That spec is outside your folders, so it goes on the follow-up list. Run the page by hand in whatever browsers you have, and report the hashes you see.
7. **Demo.** A small arena: walk, jump, push boxes, see other players. Menu, settings saved through `storage`, a pointer-lock mouse look that re-acquires lock on click after `Esc`, and a "how to make your own game from this" screen linking to the README section (text only, no external links).
8. **Tests (vitest, Node).**
   - The codec round-trips.
   - Prediction and reconciliation converge (simulated latency and loss, over a loopback transport).
   - Determinism over two instances (the gate).
   - The bundle builds and passes the bundle checks (copy landio's `test/bundle.test.js`).
   - The guest path never calls a member-only SDK command (use a fake SDK that throws on them).
   - `tsc --noEmit` with `checkJs` passes (copy landio's `tsconfig.json`).

---

## 5. 46-A: Land.io polish (`landio`, ID `ACT-LND-002`)

Land.io is the owner's own territory game (see `landio/NOTICE.md`). It already has modes (`shared/modes/`, including `draft.js` "Draft Night"), cards, bots (`shared/bots.js`), host and guest tests, and guest play. Read its tests first, and **don't break what exists**: every current test must still pass.

**Do:**
1. **Roguelike card-drafting mode, finished.** Build on `draft.js` into a full run: pick between rounds, card rarity and synergy that matter, a clear end-of-run summary, and balance. Play it against bots and tune the numbers. Add mode tests.
2. **Guest support everywhere.** Every mode and menu must work for a `/play/<token>` guest: touch controls on phones, no member-only SDK calls on the guest path, and readable layout at 375 px wide. Extend `test/host-guest-sdk.test.js`.
3. **Bot backfill.** When a lobby has fewer players than the mode needs, bots fill the empty seats and leave when people join. Name them `Lumen bot` and `Tofu bot`, then numbered variants. Keep bot choices on the seeded RNG so the sim stays deterministic.
4. **Host migration (D-388), the game half.** The platform doesn't reassign `hostId` yet (it's fixed at launch, and only the host writes `host:*` keys), so true migration needs a platform change; put that on the follow-up list. Inside the game, build everything else:
   - A deterministic successor rule: the longest-connected member, ties broken by member id.
   - A compact state snapshot the successor can resume from.
   - A `HostHandoff` interface the platform will call later.
   - Tests that a handoff between two simulated hosts resumes identically, using the fake or loopback transports.
5. **Polish.** Anything in the game that's clearly rough and cheap to fix: menu flow, HUD readability, sounds, tips. List what you changed.

**The gate for 46-A:** all Land.io tests pass, plus your new ones. `node tools/bundle.mjs` builds a bundle that passes its checks. The guest path still uses only `datachannel`/`storage`/`ready`. The project's existing browser spec for guest play (`e2e/tests/guest-play.spec.ts`) is run by the project's agents after you hand back.

---

## 6. Running things

- **Install:** from the repo root, `npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 install`. Use `--offline --frozen-lockfile` once the lockfile has `jolt-physics`.
- **Land.io tests:** `pnpm --filter @sideroom/activity-landio test`. Its `vitest.config.mjs` comment is out of date: Land.io is a workspace package now.
- **arena3d tests:** give it its own `package.json` with vitest and run `pnpm --filter @sideroom/activity-arena3d test`.
- **Typecheck:** `tsc --noEmit -p tsconfig.json` in each folder.
- **Bundles:** `node tools/bundle.mjs` in each folder, then check `dist/`.
- **Playing locally:** serve `dist/` with any static server; the fake platform takes over outside SIDE ROOM. Open two tabs to play together, since the BroadcastChannel transport links tabs.
- **Machine limits:** keep it gentle. Run one test process at a time and use `--maxWorkers=2`. Don't leave servers running. You don't need Rust, Postgres or the SIDE ROOM server for this wave.

---

## 7. Your report: `docs/dev/handoff/feat-p46-external.md`

Keep it short and factual. Sections:
1. **Done:** per item in sections 4 and 5, with the file and test that proves it.
2. **Not done:** with the reason for each.
3. **Tests run:** the exact commands and pass counts. Include the replay hashes you saw in each browser you tried.
4. **Follow-up list (outside my folders)**, each with file, change and why. Expect at least these:
   - Register `arena3d` in `scripts/build-activities.mjs` (`ACTIVITIES` map).
   - Add the Playwright spec that opens `replay/replay.html` in all three browsers and compares hashes, and wire it into CI.
   - Add an SDK/bridge command for `byos_read` shelf assets (short-lived per-object URLs).
   - Have the platform reassign `hostId` on host leave (D-388) and call the game's `HostHandoff`.
   - KTX2/Basis texture support (needs a transcoder: owner download decision).
   - Inventory rows `ACT-A3D-001` and `ACT-LND-002`, decisions, threat-model note, CHANGELOG.
5. **Owner questions:** anything you had to decide with a default.

When you're done, the project's commander merges your branch into a wave 46 integration branch. Its agents then run the full project gates (Rust, web, browser tests, CI), and do the follow-up list.
