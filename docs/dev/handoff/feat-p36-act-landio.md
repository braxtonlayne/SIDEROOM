# feat/p36-act-landio handoff (ACT-LND-001: Land.io as a built-in activity)

Branch `feat/p36-act-landio` (worktree `p36-act-landio`, base `integration/phase36`). Everything is under
`packages/starter-packs/activities/landio/` (new) plus this file. Nothing was pushed. The `activity-landio`
repository was not modified; `.env` was never read, opened or referenced.

Also on the branch: the cherry-pick of **act-channel's `6beb30a2`** (the SDK's `sdk.state` / `sdk.input` /
`sdk.on` and its `LoopbackHub`; files `packages/embedded-app-sdk/src/data.ts`, `data.test.ts`, `index.ts`,
`sdk.ts`). The tests below run against that real SDK. When act-channel's branch merges, the identical patch
should drop out cleanly; if it conflicts, take act-channel's version.

## What was copied, from which commit

From `activity-landio` commit **63c6c93d7be843e1fc89738741e9ef85bc271d54** (working tree clean when copied):

| Copied | To | Changes |
|---|---|---|
| `shared/**` (sim, rules, codec, bots, grid, cards, maps, modes, rng, room-code) | `shared/` | none, byte for byte |
| `client/**` (renderer, HUD, menu, input, predict, world, audio, sfx, fx, modes, css, html) | `client/` | `main.js`, `transport.js`, `platform.js`, `sideroom-platform.js`, `fake-platform.js`, `index.html`, `style.css` edited (below); the rest untouched |
| `server/arena.js` | `host/arena.js` | Node `randomBytes` replaced by the browser's `crypto.getRandomValues`; new `Arena.you(seat)` split out of `poll` (same fields). Logic unchanged. |
| `tools/pack.js` | `tools/bundle.mjs` | adds `host/`, the vendored SDK (`sdk/*.js`), `activity.json`; rewrites client imports (below); same bundle.rs and CSP checks |
| `test/*.test.js` for shared/, bots, cards, maps, modes, property, pack | `test/` | `node:test` import lines removed (Vitest globals); `pack.test.js` became `bundle.test.js` |
| licence/notice | `NOTICE.md` | the source repo has **no licence file or header**; the notice records provenance and that the owner authored it |

Deliberately **not** copied: `.env`, `.env.example`, `server/auth.js`, `logs/`, `activity-landio.zip`,
`server/main.js`, `server/static.js`, `server/log.js`, `server/api.js` (its logic moved, see below),
`server/rooms.js`, `client/platform/discord-platform.js`, `web-platform.js`, `sign-in.js`, the browser
(Playwright) scripts, `tools/serve.js` and the measurement tools, and `docs/`. `client/net/http-poll.js` and the
old `Transport` typedef are kept (unused) next to the new transport, as the task asked.

## How it works

Host-authoritative in the browser of the member who launched the activity. No server, no HTTP, no sign-in.

- `host/engine.js` (`LandioHost`) is `server/api.js` + `rooms.js` reduced to one room: the same `Arena`, a
  drift-corrected tick loop, per-member seats keyed by the SDK member id, a 24-person cap (`MAX_PEOPLE`), the
  host-only settings and host tools (kick / hand over / restart / end), a flood limit (60 messages per member per
  second), seat timeouts (10 s, as the server had) and removal the moment a member leaves the call. It knows
  nothing of the SDK: it is given `send(memberId, msg)` and `publish(frame)`.
- Guest to host: `sdk.input.send(bytes)` carrying JSON (`hello`, `in` inputs with the same `[seq, dir, at]` the
  HTTP POST carried, `play`, `settings`, `host`, `resync`, `hb`, `diag`, `bye`). The sender is the SDK's
  authenticated `from`; a message cannot name its own identity or display name (tested).
- Host to everyone, once per tick: one frame published through `sdk.state` under `host:tick` (the tick's record,
  the version numbers, the roster / round / settings only when their version moved, stats every other tick, and
  per seat the last input applied). Host to one member, addressed (`sdk.input.reply`): the welcome (a full
  snapshot, config), request results, and the member's own state (`you`: pid, hand, death) **only when it
  changes**. Measured (tests print it): 12 people + 20 bots on 100x100 is 1.9 KB/s broadcast + 1.2 KB/s
  addressed; 24 people + 30 bots on 200x200 is 9.6 KB/s of host uplink, biggest frame 9.3 KB. The budget is
  256 kbps = 32 KB/s per member.
- Envelopes over the SDK's 16 KiB limit (a 200x200 snapshot, a big capture record) are split into ordered
  pieces by `client/net/channel.js` (`chunkMessage` / `Reassembler`, 3000 characters per piece, so base64 stays
  under the limit). A missed piece or frame is noticed by version numbers and a gap in ticks; the guest asks for
  a fresh snapshot (`resync`, at most once a second).
- `client/net/sdk-transport.js` implements the unchanged `Transport` shape. On the host's page it talks to the
  engine in process; on a guest's page through the channel. `client/net/sdk-channel.js` makes the `Channel`
  (`client/net/channel.js`) from a real `SideRoomSDK`. `client/net/loopback.js` (in-process hub, with latency
  and drop) and `client/net/bc-channel.js` (BroadcastChannel between tabs, the first tab hosts) are the test and
  development stand-ins. The page (`main.js`) cannot tell any of them from the HTTP transport.
- Identity: from the SDK. `sideroom-platform.js` takes the participant list from
  `getInstanceConnectedParticipants` and its update event; the member id and the host id come from the SDK (see
  "to reconcile"). The standalone game's sign-in, tickets, guests and room codes are gone; one instance is one
  room and the Room tab shows only "who is here" and the host's Remove / Make host tools (CSS `body.hosted`).
- Bots and every mode, map and card work unchanged (the same simulation; 248 ported tests prove it).
- When the host's member leaves the call, guests get "The host left, so this game has ended." (no migration).

## Tests (all green; run from the landio folder)

```
cd packages/starter-packs/activities/landio
node ../../../embedded-app-sdk/node_modules/vitest/vitest.mjs run --root . --maxWorkers=2
node ../../../embedded-app-sdk/node_modules/typescript/bin/tsc --noEmit -p tsconfig.json   # ts-check of the JS
```

(The starter packs are not workspace packages and have no `package.json`, so no lockfile change; the config is
`vitest.config.mjs`, globals on. Wiring `pnpm -r test` to it is the integrator's call: either add the folder as a
workspace package, or add the command to CI.)

- **299 tests in 35 files pass** (1 skipped: the opt-in draft numbers run). Ported from the game: sim, property
  (200 random games), cards (12 files), bots, terrain, maps (8), modes (7), plus bundle rules (23).
- New: `host-guest.test.js` (11: over the in-process loopback: join, SDK-supplied identity, play, input and ack,
  settings, a dropped frame healed by resync, host leaving, member leaving, kick and ban, 24-person cap, flood
  limit, a timed-out seat), `host-guest-sdk.test.js` (5: the same game over the **real** SDK `LoopbackHub`:
  envelope sizes under 16 KiB, tiny chunk size forcing reassembly, a guest cannot write `host:*`), `host-engine.test.js`
  (11: frame content, redundancy, "you" only on change, malformed input, the two traffic budgets),
  `bc-channel.test.js` (1), and a bundle check that no import climbs out of the bundle root.
- `tsc --noEmit` over `shared/ host/ client/` is clean.
- Real browser (Browser pane, Vite on **5177 only**, static serve of `node tools/bundle.mjs --dir <out>`): two tabs,
  `?user=alice&instance=t2` / `?user=bob&instance=t2`, played with bots, kills, the host's tab closing. Confirmed
  both tabs in step (tick counter equal), the guest's input acknowledged, about 4 KB/s per guest, hosted UI
  (no Invite / code), and the "host left" card. Vite was stopped afterwards. **No Playwright.**
- **Not run:** the live spec (`e2e/landio-live.spec.ts` in the landio folder; it needs act-channel's relay and the
  client host, and ports 8080/5173). Copy it to `e2e/tests/activities-landio.spec.ts`.

## To reconcile with act-channel (ACT-CH-001), in one place each

1. **Who I am and who hosts.** The SDK stub exposes `sdk.isHost` but not the member ids (`DataTransport.self` /
   `.host` are private to `ActivityData`). `client/platform/sideroom-platform.js` `memberIds()` looks for
   `sdk.selfId`/`sdk.self`/`config.user_id` and `sdk.hostId`/`sdk.host`/`config.host_id`, and fails with a clear
   message if there is no self id. Ask for `sdk.selfId` and `sdk.hostId` (two getters) or the ids in
   `ReadyConfig`. This is the one thing that blocks the live run.
2. **The manifest capability.** `tools/bundle.mjs` writes `activity.json` with `capabilities: ["datachannel"]`
   (id, name, version, entry, participant limits). act-channel's `bundle.rs` capability may name or locate it
   differently; change `ACTIVITY_MANIFEST` there.
3. **Registration as a built-in.** How the shelf installs a starter-pack activity (the sticker packs go through
   `sr-cli packages`) is not defined for activities yet. The deliverable is `node tools/bundle.mjs` ->
   `dist/landio.zip` (190 KB, 90 entries, passes every `bundle.rs` and CSP rule in the checker); `--dir <out>`
   writes the served tree.
4. **Delivery assumptions.** The channel is assumed reliable and ordered (SCTP default). The engine's `redundancy`
   option (frames repeat the newest N records) is 1; set it higher if act-channel makes the channel unordered.
   Gaps, lost pieces and missed shared parts already recover through `resync`.
5. **`sdk.state` write rate.** The tick stream is one `sdk.state.set({'host:tick': frame})` per tick (7 per
   second, 1 to 9 KB). If the state document costs more than a plain broadcast in the client host, a dedicated
   broadcast would be better; only `sdk-channel.js` `publish` would change.

## Decisions

- Host leaving ends the game (no migration): the roadmap defines the host as the launcher. Migration would mean
  serialising the arena (the state is plain typed arrays, so it is feasible later).
- One room per instance, no codes or invite links: the instance is the room (Discord parity: an activity is
  joined from the channel).
- Fixed arena defaults are the standalone game's; the host changes mode, map, size, bots and rules from the menu as
  before, and only the host may (`hostOnly` stays on by default).
- Client imports are rewritten at bundle time (`../shared/` becomes `./shared/`): the game relied on browsers
  clamping `../` above the root, which Vite and other tools do not.
- The default `activity.json` participant limit is 24 (the standalone game's `MAX_PEOPLE`).

## Not done / risks (honest)

- **Live run unproven**: the two-browser SIDE ROOM case is written but not executed; the SDK-in-iframe path
  (`sdk/index.js` vendored, `memberIds`, participants) is covered by unit tests with the SDK's loopback and by the
  BroadcastChannel stand-in in a real browser, not by a real SIDE ROOM client.
- **A hidden host tab can slow the game.** The tick loop is a `setTimeout` loop on the host's page; a backgrounded
  tab is throttled by the browser (pages with an active call or audio are normally exempt, which is the usual
  case here). If that proves not enough, move the scheduler into a Worker (the CSP allows same-origin workers).
- No host migration; no persistence of the arena across a host reload (a reload of the host's page closes the
  game, guests see "host left").
- The standalone game's server metrics endpoint and playtest log are gone; `LandioHost.metrics()` and the
  per-seat `diag` reports are kept in memory for a future dev panel.
- A modified guest client can read the host's frames (everyone holds the room key; not cheat-proof against
  tampering, which is also true of the standalone game's client prediction). The host's engine does validate and
  rate-limit every message.
- The stale lobby copy in `index.html` ("Land.io is also a Discord Activity", the plain-browser lobby markup) is
  unused but left, to keep the copy close to upstream.
- Prettier: `packages/` is ignored by `.prettierignore`, so the copied code keeps the owner's formatting.

## Owner questions

1. Is a host leaving (closing the activity, a crash) ending the game acceptable for v1, or do you want host
   migration (the next member takes over the arena)?
2. Should Land.io appear on the shelf as a built-in activity named "Land.io" with the launch in any voice channel,
   or only when an instance admin enables it? (The bundle is ready; the registration is act-channel's/integrator's.)

## CHANGELOG lines

- Land.io, the territory game, is now a built-in activity: one member starts it from a voice call and it runs in
  their browser; everyone else plays through the call's data channel, with bots, all modes and maps (ACT-LND-001).

## Commits

All subjects start with `ACT-LND-001` and end with the Co-Authored-By line (plus the cherry-picked act-channel
commit, which keeps its own subject).
