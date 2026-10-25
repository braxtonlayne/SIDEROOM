# Handoff: feat/p24-devplat (wave 24, developer platform)

Branch `feat/p24-devplat`, worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p24-devplat`. No migrations. Nothing pushed. One download: `werift` 0.25.0 in `packages/bot` (exact pin, approved by the owner through the coordinator, commit ef8e70d7; licence check ok, MIT). One workspace link added to the same package (`@sideroom/wasm`, no download) and one workspace dev-dependency edge for `sr-sdk` tests (`sr-events`).

## Resume here

1. Push the branch and let the `live` job of `e2e.yml` run the step "First-party voice bots". It is written, with the same server settings as the local runs below, but has never run on CI (no YAML parser here either, so only the shape was checked).
2. RDY-DEV-003 waits for the first scheduled `sdk-compat` run (Monday).
3. Build note: `packages/bot` voice tests that need the MLS bundle skip without `packages/wasm/dist/e2ee`. CI builds it (the `e2e-stack` action); locally I built only that crate: `wasm-pack build crates/sr-e2ee-wasm --target web --dev --out-dir ../../packages/wasm/dist/e2ee --out-name e2ee --no-pack --no-opt`.

## Done

### RDY-DEV-002 (gateway resume and voice reconnect in sr-sdk)

- Gateway shard (`crates/sr-sdk/src/gateway.rs`): resume already existed; added the backoff reset on `RESUMED` and the memory of the last op 4 join, sent again after a fresh `READY` (a leave clears it).
- `crates/sr-sdk/src/voice_transport.rs`: `VoiceSession::resume`, `abandon_signalling`, `VoiceLink`, `ReconnectPolicy`, `close_is_retryable`, `VoiceEvent::Resumed` / `Reconnected`, `SharedMedia` (an `E2eeMedia` the key agreement and the session share). Rules in `docs/specs/developer-platform.md` 3.13.
- `GatewayHandle::e2ee_mls` (op 101) in `ops.rs`.
- `examples/voice_bot.rs` now does the whole encrypted flow: op 4, `hello` within 10 s of `VOICE_SERVER_UPDATE`, every `E2EE_MLS` dispatch into `E2eeCall`, actions back as op 101, keys into `E2eeMedia`, test frames under the epoch key, and `SR_RUN_SECONDS`, `SR_DROP_AFTER_SECONDS`, `SR_GATEWAY_URL`, `SR_IDENTITY_SECRET`. The example needs both default features.
- Tests in `crates/sr-sdk/tests`: `gateway_resume.rs` (real gateway state machine: replay, second loss, fresh IDENTIFY after overflow), `voice_reconnect.rs` (real SFU: resume with audio before and after, rejoin with transform kept, removal is final); unit tests for the backoff reset, the voice rejoin and op 101.

### RDY-DEV-001 (TypeScript voice, owner-approved werift)

- `packages/bot/src/voice.ts`: `Identity` (node:crypto ECDSA P-256), `E2eeCall`, `E2eeMedia`, over `sr-e2ee-wasm` (`@sideroom/wasm` `loadE2ee`), mirroring `sr_sdk::voice`.
- `packages/bot/src/voiceTransport.ts`: `voiceStateBody`, `VoiceServerUpdate`, `VoiceSession`, `VoiceLink`, `ReconnectPolicy`, `closeIsRetryable`, on werift. The speaker of an incoming line is read from the `a=msid` of the SFU's offer (werift does not put it on the track; found when the first live run heard 0 frames).
- Entry point `@sideroom/bot/voice` (separate so bots without voice do not load WebRTC); `Client.voiceStateUpdate` (op 4) and `Client.e2eeMls` (op 101).
- Tests: `voice.test.ts` (three `E2eeCall` members through a relay on the real wasm bundle: same keys, frames both ways, nothing sent before `execute`, recovery), `voiceTransport.test.ts`, and `voice.live.test.ts` (`SR_VOICE_LIVE=1`).
- Driver `e2e/sdk-compat/voice-bot-live.mjs --ts`: starts the Rust bot as a partner in the call, then runs the live test (vitest). Each implementation has to decrypt the other's frames; the TypeScript one also resumes after dropping its own SFU socket.
- Docs: Bot SDKs page (`docsContent.sdks.ts`), spec 3.13.1, `docs/dev/sdk-compat.md`.

### RDY-DEV-004 (developer docs pass)

- `docsContent.ts`/`docsContent.sdks.ts` corrected against the code (sharding is supported, RESUME flow, bots in voice in both SDKs).
- `apps/web/src/developers/__tests__/docsRoutes.test.ts`: every route and gateway event the docs name exists in OpenAPI / the generated gateway reference (checked that it fails on a wrong path).
- `docs/dev/sdk-compat.md`: stale Tier 1 rows fixed; no scheduled run exists yet, so it records that instead of results.

## What was run

Local, own scratch DB `sr_p24_devplat` (dropped), sr-server on 8080 with `SR_VOICE__ENABLED=true SR_STORAGE__ENDPOINT=memory:` and voice E2EE at its default (on); my server stopped by PID.

- `node e2e/sdk-compat/voice-bot-live.mjs --seconds 60` (Rust bot, E2EE on):
  `joining ws://127.0.0.1:8080/voice/ws (e2ee: true)`, `encrypting under epoch 0`, `connected`, `dropping the SFU WebSocket`, `resumed (0 in the channel)`, `done after 60.0139172s: connected=true resumed=true encrypted_epochs=1 frames=1907`, `voice bot run ok`.
- `node e2e/sdk-compat/voice-bot-live.mjs --ts --seconds 60` (TypeScript bot with the Rust partner): TypeScript `done: connected=true resumed=true encrypted_epochs=1 frames=3000 heard=4477 undecryptable=3` (1 vitest test passed); partner `done after 120.0205522s: connected=true resumed=false encrypted_epochs=3 frames=6420` with lines `<ts bot id> spoke 60 bytes`; `voice bot run ok`.
- `cargo test -j 2 -p sr-sdk` (default: 45 unit, `gateway_resume` 2, `op8_real_gateway` 1, `voice_e2ee` 5, `voice_reconnect` 3, `voice_transport` 5) and `--no-default-features` (41 unit and the rest): green. `cargo fmt -p sr-sdk --check`, `cargo clippy -p sr-sdk --all-targets -- -D warnings` (default and `--no-default-features`), `cargo deny check`: clean.
- `packages/bot`: vitest (28 passed, the live test skips without `SR_VOICE_LIVE`), `tsc --noEmit`, eslint: clean. Licence check (`pnpm licenses list --json --prod | node scripts/check-licenses.mjs`): ok; `scripts/check-notices-copy.mjs` ok. No `THIRD_PARTY_NOTICES.md` entry: `packages/bot` is a private package that ships in no web bundle.
- Web (`apps/web/src/developers`): `tsc --noEmit`, eslint, prettier, vitest (11 files, 122 tests): green. `node --test` in `e2e/sdk-compat`: green. Prettier on the touched docs, workflow and package files: clean.

## Not run

- The `e2e.yml` live step on CI, Playwright, the bundle budget (no web bundle change beyond docs strings), OpenAPI regeneration (no route changed).

## Findings and decisions

- An encrypted call removes a member that sends no op 101 `hello` within 10 s (D-263). Both bots answer with the MLS handshake, so the live runs keep E2EE on.
- `Client` runs each handler call in its own task, so event order seen by a Rust `EventHandler` is not guaranteed; the resume test compares as a set.
- werift, as used here: no audio-level RTP header extension is negotiated, so the speaking indicator follows `setSpeaking`; `undecryptable=3` in the TypeScript run is the few frames around an epoch switch.
- The TypeScript shard resumes the gateway session but does not remember a join to send it again after a fresh session (the Rust shard does). Documented on the Bot SDKs page.
- No Opus encoder in either SDK; the bots send fixed 60 byte test frames.

## Open owner questions

None blocking. For information: the first `live` run on CI decides whether the written step needs adjusting (both bots bind 127.0.0.1, as the SFU announces).
