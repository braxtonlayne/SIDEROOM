# Follow-up wave, builder `arena3d` (`ACT-A3D-002`): touch controls, host handoff, GPU recovery, hidden-tab host

Branch `fix/fu-arena3d`, based on `main` `a3fbfd3a`. Work is inside `packages/starter-packs/activities/arena3d/**`, plus
two e2e specs. The replay is unchanged: `6fdea5719b5383d6` / exact `e35977c2a4cddeed` (`node replay/run-node.mjs --json`
after every change; also equal in Chromium and WebKit through `e2e/arena3d/replay.spec.ts`). Nothing in `sim/` changed.

## 1. Done (commits)

| Task | What | Where |
|---|---|---|
| 2. Host handoff | Declares `host_handoff`. `channel.onHostChange` (from `sdk.on('host')`). The new host rebuilds a `HostSession` from its newest `host:snap` and the new `host:seats` table (member id to slot, plus the host's slot), drops the old host's player, keeps every other slot and acked input seq; the other peers say hello again and carry on predicting. A host the platform moves away from steps down. No snapshot ever heard: a fresh match on the same world. | `net/handoff.js`, `net/seats.js`, `net/host.js` (`resume`), `net/client.js` (`hostChanged`, `takeover`), `net/sdk-channel.js`, `net/loopback.js` (`changeHost`, `disconnect`), `client/game.js`, `sideroom.json` |
| 4. Hidden-tab host | A watchdog timer runs the loop whenever no animation frame ran for 100 ms; each call counts real elapsed time and runs that many fixed ticks (cap 5 s / 300), one snapshot per burst. Peer messages also drive a stalled host. A burst runs when the tab comes back. No workers. Documented in the README ("Hidden tabs": what the host's players see). | `client/pacer.js`, `client/game.js`, `net/host.js` (`tick(input, publish)`) |
| 3. GPU recovery | `createRenderer` returns a wrapper that rebuilds after WebGPU `device.lost` (same canvas, then WebGL2 on a fresh canvas; a second loss in 30 s goes to WebGL2 directly), WebGL `webglcontextlost` (wait 3 s for restore, else fresh canvas) and 3 throwing frames; every upload is replayed under the same handle ids; the page shows a notice. Also fixes the old behaviour where our own `device.destroy()` (a refused pipeline) reported "GPU lost". | `render/renderer.js`, `render/webgpu.js`, `render/webgl2.js`, `client/main.js`, `client/index.html`, `client/style.css` |
| 1. Touch controls | Move stick (floating, 8 directions, dead zone), look by dragging anywhere else, Jump (held) and Crouch (toggle), Menu button; shown on `(pointer: coarse)`, `?touch=1`, or on the first real touch; no pointer lock on touch; HUD moves out from under the thumbs. Goes through `InputState.addLook` and `setTouchButtons`, the same integer path as keyboard and mouse. Menu text explains touch and hides the keyboard line on touch. | `client/touch.js`, `client/input.js`, `client/main.js`, `client/index.html`, `client/style.css` |

Commits: `ee5c82e0` (handoff and hidden-tab), `3c75e0c6` (GPU loss), `f74b3c5f` (touch and the GPU notice), plus the
follow-up commit that holds the stalled-frame watchdog in `client/game.js` (replacing the `document.hidden` timer), the README, the two e2e specs, the empty-HUD fix and this file (`bd6613c5`). `dist/` is rebuilt
(`node tools/bundle.mjs`) and `node scripts/build-activities.mjs arena3d` passes (55 files).

## 2. Not done, and why

- **No live two-browser handoff through the real platform.** The game half is proven over the loopback hub and over the
  real Embedded App SDK (`ActivityData.changeHost`); the platform half (server picks the successor, client sends
  `HOST_CHANGED`) is wave 46's and has its own tests. A full run needs the voice stack, which I did not start.
- **The local stand-ins never move the host** (`bc-channel.js`, fake platform): in two local tabs, closing the first
  ends the match for the other. Not worth building (the real platform does it).
- **Firefox replay not run:** Firefox cannot launch on this machine (`spawn UNKNOWN`, nothing to do with the game).
  Chromium and WebKit pass. CI runs it.
- **WebGPU loss in a real browser:** a WebGPU device cannot be lost on demand, so that path is proven against the
  recording fakes only (`test/gpu-loss.test.js`). The WebGL path was also lost and restored for real
  (`WEBGL_lose_context`) in Chromium.
- **Real phone:** touch was checked with Chromium's touch emulation (real CDP touch events, pointer type `touch`), not a
  physical device.
- **Rust test `the_arena3d_manifest_survives_review_with_connect_self`** (`crates/sr-activities/src/sandbox.rs`) lists the
  capabilities arena3d must have; it does not mention `host_handoff`. It still passes (it only checks inclusion), but
  should name it. Outside my folders and I had no cargo, so I left it.
- **Docs outside the folder:** inventory row `ACT-A3D-002`, the CHANGELOG, and `integration-phase46.md` item "arena3d
  host handoff" are for the integrator.

## 3. Routes shot (the arena3d game is not an app route; there is no page-sweep entry for it)

Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/fu-arena3d/` (I looked at all four):

- `arena3d-touch-portrait.png`, `arena3d-touch-landscape.png`: the game at 375x812 and 812x375 on the static server
  (`e2e/arena3d/touch.spec.ts`; the spec asserts every control and HUD box is on screen and nothing overlaps).
- `arena3d-guest-touch-portrait.png`, `arena3d-guest-touch-landscape.png`: the same, as a **guest on a `/play/<token>`
  link** inside the real sandboxed iframe with the Activity CSP, against the Vite stack on 5192 / sr-server on 8192
  (`e2e/tests/arena3d-guest-touch.spec.ts`: the join route and relay are mocked as in `guest-play.spec.ts` part 1).
  It asserts no CSP violation or console error from the game frame, and that a real touch reaches the game.

Fixed after looking: the "Move" ring stayed visible under the floating stick (now hidden while a stick is down); an empty
HUD player list showed as a tiny empty pill before the roster arrived (`.hud-box:empty` is hidden). Before/after: none
of the touch layout needed changing after the first look at 375x812 and 812x375 (HUD moved to the top corners and
the buttons stacked on the right by design).

## 4. Tests run

- `npx vitest run --maxWorkers=2` in the package: **13 files, 84 passed** (was 8 files, 55). New: `handoff.test.js` (8),
  `pacer.test.js` (4), `gpu-loss.test.js` (8), `touch.test.js` (6); `bundle.test.js` now expects `host_handoff`.
- `npx tsc --noEmit -p tsconfig.json` (arena3d): clean. `node tools/bundle.mjs`: every check passes (55 files, 1.0 MB
  zipped). `node scripts/build-activities.mjs arena3d`: passes.
- `node replay/run-node.mjs --json`: `6fdea5719b5383d6` / `e35977c2a4cddeed`.
- Playwright (`e2e`, Chromium, workers 1): `playwright.arena3d.config.ts` all 5 (replay, smoke, touch portrait and
  landscape, WebGL context loss) pass; replay also passes in WebKit; `tests/arena3d-guest-touch.spec.ts` 2 pass against
  the stack; `playwright test --list --project chromium` loads all 890 tests in 66 files; `tsc` on `e2e` clean;
  prettier clean on the two new specs.
- By hand in the Browser pane (WebGPU present): 60 ticks a second on a host tab whose animation frames were paused
  (before the watchdog it ran 0 per second there); a second tab joined it; touch events through the DOM glue gave the
  expected input log (`1`, `17`, `33`); WebGL context loss and restore recovered with the notice.
- Not run (per the brief, they are for the web app, not this folder): web typecheck, vitest on `apps/web`,
  `check-css-tokens`, `i18n-audit`, bundle budget, entrance isolation (no `apps/web` or `packages/ui` file was touched).
- Stack stopped (Vite 5192, sr-server 8192, the static server 5195) and `sr_fu_arena3d` dropped.

## 5. Owner questions (each decided with a default)

1. **Crouch on touch is a toggle** (a thumb is busy looking); Jump is held. Alternative: hold-to-crouch.
2. **The stick is 8-way** (it fills the same button bits as WASD; the simulation takes no analogue vector). Walking speed
   is the same in every direction a keyboard gives. Alternative: add an analogue move input to `sim/` (changes the
   replay and the wire format, so it needs your yes).
3. **Look sensitivity on touch** is the mouse sensitivity setting times 1.8 (a finger drags less far than a mouse). A
   separate touch slider is not added.
4. **No pause on touch:** there is no pointer lock to lose, so the match never pauses; Menu opens the menu over the live
   match (the player stands still). Alternative: freeze the player while the menu is open.
5. **Successor choice is the platform's** (longest-connected, as Land.io). arena3d takes whoever the SDK names; it does not
   re-check that it is the "right" one.
6. **The new host loses its own last few ticks** of walking (it rewinds to the last snapshot it heard, 3 ticks apart plus
   latency), and the old host's player vanishes. Alternative: keep the old host's body for a grace period (it would
   stand still, un-owned).
7. **A hidden host with nobody sending** is driven only by the throttled timer: capped at 5 s of catch-up per call. For a
   long idle (a once-a-minute throttle) it runs behind real time. Alternative: pause the match when the host's tab has
   been hidden for over a minute and nobody else is connected.
8. **After a WebGPU loss the first answer is another WebGPU device**, then WebGL2; a second loss within 30 s goes to
   WebGL2 straight away (a flaky driver). Change `REPEAT_LOSS_MS` in `render/renderer.js` if you want it stickier.
9. **`host:seats` is JSON** next to the binary roster (the wire protocol and `PROTOCOL_VERSION` are unchanged). Alternative:
   a binary `ROSTER` v2 with member ids (a protocol bump).
10. **The `gpu-notice` banner strings are English literals** in `client/main.js` (the game has no i18n, like the rest of its
    UI text).
