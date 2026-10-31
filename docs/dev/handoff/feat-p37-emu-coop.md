# Handoff, wave 37 emu-coop (ACT-EMU-002): co-op by input forwarding

Branch `feat/p37-emu-coop` (base `integration/phase37`). Not pushed. No migration (1810-1819 unused), no download, no new dependency.

## What was built

In a voice call one member **hosts** the game running in their window and shares the canvas through the existing screen share (voice video path, E2EE). Up to three others **take a seat** (players 2 to 4); their pad goes to the host as small binary packets over the encrypted activity data channel from wave 36 and the host feeds it to the core's ports 1 to 3. No netplay rollback and no lockstep: the UI says so ("Play together" dialog notice).

Code (all new unless noted), `apps/web/src/emu/coop/`:

| File | Role |
|---|---|
| `protocol.ts` | The wire format (10 message types, layout documented at the top), `encode` / `decode`, `COOP_INSTANCE`. |
| `input.ts` | RetroPad bit layout (bit n = libretro joypad button n, same indices as EmulatorJS `simulate_input`), default keyboard layout, standard Gamepad API mapping (left stick = d-pad), opposite-direction cancel, `diffMasks`. |
| `seats.ts` | Port assignment: lowest free of ports 1, 2, 3; a returning guest keeps its seat; port 0 is never assignable. |
| `host.ts` | `CoopHost`: announces itself every 2 s, answers DISCOVER, seats guests, applies pads, watchdog (stuck pad released after 3 s, silent guest dropped after 15 s), kick, stop. |
| `guest.ts` | `CoopGuest`: discovers hosts, JOIN, sends the whole pad on change (keepalive 1 s while held), PING every 2 s and smooths the PONG round trip. |
| `transport.ts` | `CoopTransport` interface; `dataPortTransport()` over wave 36's `activityDataPort`; `CoopCore` / `HostableGame` (the "running core that accepts input for port N" interface). |
| `controls.ts` | `GuestControls`: window key capture (ignored in text fields and with Ctrl/Meta/Alt; blur releases everything) and gamepad polling. |
| `session.ts`, `useCoop.ts` | One co-op role per window (`coopSession`), a `useSyncExternalStore` hook. |
| `source.ts` | The seam to the loader: `offerCoopGame(game)` / `withdrawCoopGame(game?)`. |
| `CoopButton.tsx`, `CoopDialog.tsx`, `CoopStatus.tsx` | Voice panel entry "Play together" (lazy dialog), host/guest dialog, status chip with controls switch and measured latency. |

Also: `apps/web/src/lounge/voice/VoicePanel.tsx` (button next to Activities/Soundboard, status chip under the action row; guild calls only, like Activities), i18n fragment `apps/web/src/i18n/messages/en/p37-emucoop-1.ts`, the live case `e2e/tests/emulator-coop.spec.ts`.

### Design decisions

- **Data channel instance.** The SFU scopes the relay by (room, client-chosen instance id) and never checks it against the gateway, so co-op uses the fixed instance `9223372036854775809` (2^63 + 1; top bit set so it cannot meet an activity snowflake). No gateway/activity record is involved and no migration. It costs one of the member's 8 instance slots only while the dialog is open or a game is hosted/played.
- **Who sent a packet** is the SFU-authenticated sender, never a field in the packet. The port a pad drives is decided by the host's seat table, never named by the guest, so a guest cannot reach player 1 or another seat (test "never lets a guest reach port 0").
- **Whole pad every time** (16-bit mask + 16-bit sequence), so a dropped packet (the SFU drops above `voice.datachannel_kbps`) heals on the next one; wrap-around-safe sequence check drops duplicates and stale packets.
- **Latency shown** = the measured guest -> SFU -> host -> SFU -> guest round trip of an application PING/PONG on the same channel ("Input round trip N ms, plus the video delay"); the host sees its own SFU ping. The picture's own delay (encode, SFU, decode) is not measurable from the pad path and is stated as an extra.
- **Squatting:** any member can announce a game; guests pick the host explicitly (the list shows the member's name and the title), and only the chosen host's SEAT / PONG / KICK / CLOSE are accepted (tests with a stranger).
- **Host leaving:** the host's CLOSE (or `stop()` on leaving the call) ends the session; if the host crashes the guest's chip stays until they leave (no host heartbeat timeout on the guest side while seated; see open).

## The integration seam for emu-shell (what to wire)

emu-shell's loader had no code on its branch when this was written (only its Rust commits), so nothing from it is cherry-picked. The loader needs exactly two calls:

```ts
import { offerCoopGame, withdrawCoopGame } from '../emu/coop/source';
const game = { title, setInput: (port, button, value) => core.simulateInput(port, button, value) };
offerCoopGame(game);      // when a core starts running
withdrawCoopGame(game);   // when it stops
```

`setInput(port, button, value)`: `port` 0-based (0 = player 1), `button` = RetroPad index 0..15 (B Y Select Start Up Down Left Right A X L R L2 R2 L3 R3), `value` 0|1. For the EmulatorJS-style glue in emu-ops' format notes this is `Module.cwrap('simulate_input', null, ['number','number','number'])(port, button, value)`. The loader should keep `{ title, setInput }` free of any import from coop so the loader stays lazy (D-385: nothing emulator-related before sign-in; this code is only reached from the signed-in lounge, the dialog chunk is lazy, `check-entrance-isolation` result below).

Co-op code reads nothing from the loader, so if the loader changes shape only the 3-line adapter above changes.

## Cherry-picks from feat/p36-act-channel (in order, no merge)

`6beb30a2`, `fbbe151a` (SDK), `16981029` (sr-e2ee DataFrame + wasm export), `b81d2c60` (`createDataSender/Receiver`), `9960a3de` (web host wiring: `voice/dataPort.ts`, engine, E2EE worker), `11a5e2be` (lint). Not picked: the sr-voice relay, the capability migration 1760, sr-sdk, the p36 live spec. The integrator merging p36 first (or after) will see these as already-applied identical changes; the sr-voice SFU relay (`c2bb8106`) is **required at runtime** for co-op to work (it is the part of p36 this branch does not carry), so p36 must be in the integration branch for the live case.

## Live case (not run: ports 8080/5173 are another agent's)

`e2e/tests/emulator-coop.spec.ts`, typechecked (`e2e`: `tsc --noEmit` clean), eslint-clean sources, prettier applied. Two contexts, one voice channel, host shares its canvas through the real screen share path (the test replaces `getDisplayMedia` by `canvas.captureStream` so no OS picker is needed), the guest joins as player 2, holds X (= RetroPad A), the host canvas turns from red to green, **and** the picture the guest receives through the SFU turns green; release goes back to red; the latency chip shows "Input round trip N ms"; leaving frees the seat.

- Default (probe) mode: the host offers a tiny in-page core through `window.__srCoopOffer` (only defined when `VITE_SR_E2E=1`, which the Playwright dev server sets; folded away in production builds). Runs with the voice stack: `SR_E2E_VOICE=1` and sr-server with `SR_VOICE__ENABLED=true`, Chromium, dev web server.
- `SR_E2E_COOP_ROM=1`: uses emu-shell's self-written ROM through the loader. `startTestRom()` in the spec is the one place to adapt (it guesses a Games link and a `canvas[data-testid="emu-canvas"]`). The ROM must read the SECOND controller (NES: `$4017`) and draw a different background colour (red -> green, the spec's `RED` / `GREEN` constants) while its A button is held. If emu-shell's ROM only reacts to pad 1, add that behaviour to the ROM.
- Integrator: run it once emu-shell and p36 are merged: `SR_E2E_VOICE=1 ... playwright test emulator-coop.spec.ts --project=chromium`.

## Tests run

- Vitest (`apps/web`, `--maxWorkers=2`): `src/emu/coop` 43 tests: `protocol.test.ts` (message format: round trips, byte layout, malformed input, u64 ids, title clamp), `input.test.ts` (keyboard and gamepad mapping, opposite cancel, diff), `session.test.ts` (port assignment, host+guest over a relay: seating, pad application to the right port only, no port 0, stale/duplicate/wrapped sequence, strangers cannot kick/seat/close, watchdog, keepalive, kick/stop, ping/pong RTT, discover, free seats), `ui.test.tsx` (dialog as host and guest, status chip, keyboard to port 1, controls switch, axe). With `src/i18n` and `src/lounge/voice` together: 189 passed.
- `tsc --noEmit` (apps/web, e2e): clean. eslint on `src/emu` and `VoicePanel.tsx`: clean. prettier applied.
- `apps/web/src/voice/__tests__/dataPort.test.ts` (p36's) needs the wasm package build (`packages/wasm/dist`), see below.

## Not done / open

- Wasm build, `build-web`, `check-entrance-isolation`, `check-bundle-budget`: see the "Build gates" note at the end of this file (filled in after the last run).
- Host crash while guests are seated: no guest-side host timeout yet (the stale host leaves the list after 7 s; the seated chip stays). A tiny addition: treat 3 missed ANNOUNCEs as CLOSE. Left out to keep the seat stable through a brief SFU drop; owner/integrator call.
- Guests cannot rebind keys (default layout only); the layout is `DEFAULT_KEYS` in `input.ts`.
- Touch controls for phones are not built.
- The Play together button needs a game offered by the loader to host; a guest needs nothing but the call.
- The shared canvas is the host's screen share at the presets of `ScreenSharePicker`; no emulator-specific preset (a 60 fps / "motion" hint is the best choice for games; the picker already offers it).

## Owner questions

1. Should the host be able to restrict who may take a seat (for example only people with `view_games`)? Today any member of the call can.
2. Is a "co-op needs the host's `view_games` grant only" model right (guests never see the ROM; they only see the stream)? That is how it works now and it avoids any ROM transfer.
3. Fighting games over this path: the plan says not to promise them; the UI says "slow and turn-based games work best". OK as worded?

## Changelog line

- Play together: host a running game in a voice call, share the screen, and let up to three others play on players 2 to 4 with their keyboard or gamepad, encrypted end to end; no rollback, so slow and turn-based games work best (ACT-EMU-002).
