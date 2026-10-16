# Handoff: feat/p13-voice (sharded SFU, web video path, soundboard, stage, DM call view, channel status)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p13-voice, branch feat/p13-voice (base integration/phase13). Local commits only, nothing pushed. Scope: `docs/inventory/08-lounge-voice.md` except the VOX-E2E-* MLS items. Migration used: `0860_voice_channel_status_permission.sql`.

## Done

| Item | What | Where |
|---|---|---|
| VOX-SFU-011 | The media actor is sharded: `SfuConfig::shards` / `voice.shards` (default 0 = half the cores, at most 8, never more than the bound UDP sockets). Rooms are spread over the actors by a hash of the channel id; each actor owns its own `SfuCore`, a disjoint slice of the sockets, a participant id space (`shard + 1 + k * n`) and `1/n` of the egress budget. ICE-TCP, RESUME (session ids start with the shard), a user joining a channel on another shard, the node-wide `max_participants` and the gauges are handled in the shell. Load harness: `run_sharded` and `--shards N`. | `crates/sr-voice/src/server.rs`, `core.rs` (`with_id_space`), `loadtest.rs`, `examples/loadtest.rs`, `tests/e2e.rs` (4 new tests), `docs/dev/server.md`, spec 4.1 and 4.12 |
| VOX-CLI-006 / 007 | `VIDEO_PUBLISH` is on. The engine puts the camera on a `sendonly` line with three simulcast encodings (`f`,`h`,`q`) and the screen (and its audio) on their own lines, names each mid to the SFU with op 12 `publish`, republishes after a reconnect, and receives `<user>-camera` / `<user>-screen` streams into the tiles. Tile sizes become `subscribe` viewport hints (debounced), "Disable video" turns a camera off for you, screens are subscribed with Watch Stream (op 20). | `apps/web/src/voice/engine.ts`, `controller.ts`, `video.ts`, `protocol.ts`, `streams.ts`, `lounge/voice/Tiles.tsx`, `useTileViewport.ts` |
| VOX-CLI-008 | Preview thumbnail uploader (480 px JPEG of the shared screen after 3 s, then every 30 s) and the idle stream tile shows it. | `voice/preview.ts`, `lounge/voice/useStreamPreview.ts` |
| VOX-UI-004 / 008 / 012 | Viewer count (STREAM_CREATE/UPDATE `viewer_ids`), preview, quality from op 12 `published` + `layer`, stream follows the person volume and pops out; user menu gets Call and Disable video. | `Tiles.tsx`, `PopOut.tsx`, `useVoiceUserMenu.ts` |
| VOX-SB-002 / 003 | Soundboard popover (search, favorites, this server, default sounds, preview that only you hear, volume) in the call controls and the voice panel; playing announces (REST), fetches, decodes and mixes the file into the outgoing track past the voice gate, and the sender hears it. | `lounge/voice/SoundboardPopover.tsx`, `voice/soundboard.ts`, `mic.ts` (`playSound`, gate gain), `soundPlayer.ts` |
| VOX-STG-002 | Stage view (stage channels no longer open as text channels): topic, speakers as tiles, audience with raised hands first, join / raise hand / step down / become a speaker, moderators start, edit and end the stage and invite or move to the audience. A suppressed member keeps the microphone shut. | `lounge/voice/StageView.tsx`, `voice/stage.ts` |
| VOX-UI-010 / 011 | DM call stage above the conversation with tiles, controls, a splitter to resize it, "Calling ..." tiles for people still ringing, ringback and Cancel call. | `lounge/voice/DmCallStage.tsx`, `voice/calls.ts` |
| VOX-UI-015 | Voice channel status, with the backend (it did not exist): `PUT /channels/{id}/voice-status`, `VOICE_CHANNEL_STATUS_UPDATE`, `status` on `GUILD_CREATE` channels, new permission `SET_VOICE_CHANNEL_STATUS` (bit 48), migration 0860 backfills @everyone. Pencil under the header, status line in the sidebar. | `crates/sr-gateway/src/voice_status.rs`, `sr-api/src/routes/voice_live.rs`, `sr-core/src/permissions.rs`, `lounge/voice/VoiceStatusBar.tsx`, `voice/channelStatus.ts` |

## Load test numbers (VOX-SFU-011)
Release build, `cargo run --release -p sr-voice --example loadtest`, Windows dev box (32 logical cores) shared with seven other agents, so CPU figures are pessimistic and noisy:

| Run | Egress | CPU |
|---|---|---|
| 1 actor, 5 publishers x 1.5 Mbit/s, 40 viewers | 312 Mbit/s | 58 % of one core (the P10 run on a quieter box said 110 %: the machine, not the code, moved) |
| `--shards 4`, same load per shard | 1248 Mbit/s | 78 % of each shard core while all four ran at once, 3.1 cores in total |
| `--shards 4 --publishers 3 --viewers 17` (rooms of 20) | 319 Mbit/s | 8.5 % of each of four cores, 0.34 cores in all |

The last row is the realistic shape (a node carries many small rooms): 300 Mbit/s costs about 8 % of each of four shard cores, well under the 50 % target. The harness times the core only, no sockets or kernel. The AX42 run that P10-VBE-1 asked for is still to be done.

## Tests run
- Rust: `cargo test -p sr-voice` (all, including 4 new sharded e2e tests over real loopback sockets: audio in two shards, cross-shard rejoin, RESUME and control commands by channel, ICE-TCP routing); `cargo test -p sr-gateway --lib voice` (77, 4 new status tests); `cargo test -p sr-api --lib routes::voice` (12, status route) and `--test route_matrix` (new row reviewed, `?` removed); `cargo test -p sr-core -p sr-chat --lib`; `cargo test -p sr-db --test p13_voice -- --include-ignored` against the local Postgres (TestDb clones its own throwaway databases; migration 0860 and the new-server default); `cargo clippy --all-targets -D warnings` on sr-core, sr-chat, sr-gateway, sr-api, sr-server and sr-voice; `cargo fmt --check` on the touched crates.
- Web (`apps/web`): the whole vitest suite, 167 files and 1864 tests, `--maxWorkers=2`; `tsc --noEmit`; eslint and prettier on every changed file; `vite build` and `scripts/check-bundle-budget.mjs` (lounge 371 KB gzip of 600 KB, all budgets ok); OpenAPI, `schema.d.ts` and the MSW handlers regenerated (only the new route is added); `scripts/check-doc-links.mjs`.
- Not run: Playwright (shared ports, per the brief), real browsers (see below), `packages/wasm` tests (the wasm package was not rebuilt; the web tests used a copy of the main checkout's `dist`, which does not know permission bit 48 yet, so `pnpm --filter @sideroom/wasm build` is needed before the permission editor shows the new row with its Rust-side name check).
- The fakes are in `voice/__tests__/fakes.ts` (extended with transceivers, streams that fire `removetrack`, buffer sources and `decodeAudioData`).

## Still open, with reasons
- **VOX-E2E-001..008** left as asked (MLS and the frame encryptor). The soundboard mix happens before encoding, so it will go through the same ScriptTransform when that exists.
- **VOX-UI-001 / 005**: only the Activities button is missing and no Activities backend exists (spec: Phase 6), so it stays out.
- **VOX-UI-003**: no per-tile quality for remote people (needs inbound stats mapped to a user) and no E2EE shield (VOX-E2E-007).
- **VOX-UI-016**: no separate bottom sheet for the controls on mobile; the scrolling bar stays.
- **Not seen by real browsers**: simulcast `sendEncodings` with rids, `transceiver.stop()` renegotiation, `removetrack` on the receiving side, `getDisplayMedia` audio as a second line, Document PiP with a remote stream. All follow the P10 SFU contract; a manual pass in Chrome and Firefox against a running server is the first thing to do.
- Nobody is told on screen that a soundboard sound played (only heard); `VOICE_CHANNEL_EFFECT_SEND` is not shown yet.
- The soundboard is hidden in DM calls (the gateway announces sounds per server channel only).

## Decisions made (conservative defaults, flagged)
- **Sharding now, against the P10-VBE-1 answer** ("later, measure on the AX42 first"): the task asked for it. The default `voice.shards = 0` shards a multi-core host automatically; set `voice.shards = 1` for the old single-actor behaviour. The egress budget is split evenly per shard (an approximation; a busy shard may start capping a little early).
- **`SET_VOICE_CHANNEL_STATUS` is on for @everyone** (new servers and, by migration 0860, existing ones), as Discord has it. Anyone in a channel can then set its status; managers can set and clear it from anywhere.
- A voice channel status lives in memory and ends with the last member leaving (Discord behaviour), like stage instances.
- The soundboard volume and favorites are per browser (`localStorage`), not synced.
- "Call" in the voice user menu uses the existing DM call start (opens the DM, joins, rings).

## Open owner questions
1. Is sharding on by default (auto) acceptable before the AX42 measurement, or should the default be 1 until then?
2. Should `SET_VOICE_CHANNEL_STATUS` be off by default for @everyone (griefing risk in big servers) with a role to grant it?
3. Should favorites and the soundboard volume sync across devices (user settings) instead of staying per browser?
