# Handoff: feat/p24-devplat (wave 24, developer platform)

Branch `feat/p24-devplat`, worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p24-devplat`. No migrations, no new dependency (one dev-dependency edge inside the workspace: `sr-events` for `sr-sdk` tests). Nothing pushed.

## Resume here

1. RDY-DEV-001 (TypeScript voice) is not started: it needs `werift`, a download. The coordinator's message said the owner approved it, but that approval did not come from the owner in chat, so I did not run `pnpm add`. Needed from the owner: a yes in chat for `werift` in `packages/bot` only (exact version, newest release at least two weeks old), then licence check (`scripts/check-licenses.mjs`), then implement per `docs/specs/developer-platform.md` section 3.13.1.
2. RDY-DEV-002: push the branch and let the `live` job of `e2e.yml` run the new step "sr-sdk voice bot (one minute, live server and SFU)". Tick the inventory box when it is green.
3. RDY-DEV-003: waits for the first scheduled `sdk-compat` run (Monday).

## Done

### RDY-DEV-002 (gateway resume and voice reconnect in sr-sdk)

- The gateway shard already resumed (HELLO, RESUME, backoff, resume-URL allow list). Added: backoff resets on `RESUMED` (it only reset on `READY`, so repeated resumes crept up to 60 s); the last op 4 that joined a channel is remembered and sent again after a fresh `READY` (the gateway forgets voice state with a session it cannot resume), and a leave clears it. `crates/sr-sdk/src/gateway.rs`.
- Voice: `VoiceSession::resume()` (SFU RESUME on a new WebSocket, media untouched), `abandon_signalling()`, and `VoiceLink` + `ReconnectPolicy` + `close_is_retryable` in `crates/sr-sdk/src/voice_transport.rs`. A lost socket resumes; when the SFU answers 4006 or the media was not up, it joins again with the same token and keeps the frame transform; 4014 (removed), 4016, 4004 (token expired), 4017 are final and surface as `Closed(code)`; `VoiceLink::rejoin(update)` takes a fresh `VOICE_SERVER_UPDATE`. New events `VoiceEvent::Resumed` and `Reconnected` (additive).
- `examples/voice_bot.rs` uses `VoiceLink`, waits for `READY` instead of sleeping 3 s, takes `SR_RUN_SECONDS`, `SR_DROP_AFTER_SECONDS` and `SR_GATEWAY_URL`.
- Driver `e2e/sdk-compat/voice-bot-live.mjs` (+ `provision()` gained a `permissions` option) and a step in the `live` job of `.github/workflows/e2e.yml`.
- Specs/docs: `docs/specs/developer-platform.md` 3.13 (reconnect rules) and 3.13.1 (TypeScript voice design, "pending the owner's choice of transport", no dependency added), `docs/dev/sdk-compat.md` (first-party SDK table).

### RDY-DEV-004 (developer docs pass)

- `docsContent.ts` and `docsContent.sdks.ts` corrected against the code: sharding is supported (`shard: [id, count]`, `/gateway/bot` returns one shard per 1000 installs; the page said "not supported"), RESUME flow, bots in voice (the page said the transport was not built).
- `apps/web/src/developers/__tests__/docsRoutes.test.ts`: every route (method and path) and gateway event/opcode the docs name exists in `packages/api-client/openapi.json` / the generated gateway reference. Checked that it fails on a wrong path.
- `docs/dev/sdk-compat.md`: Tier 1 rows 2 and 4 were stale (sharding), "no HTTP interactions endpoint" removed, and a status section: no scheduled run exists yet, so no per-library results are recorded after the phase-17 local walk (7 of 7).

## What was run

- `cargo test -j 2 -p sr-sdk`: all green (44 unit, `gateway_resume` 2, `op8_real_gateway` 1, `voice_e2ee` 5, `voice_reconnect` 3, `voice_transport` 5). `--no-default-features`: 40 unit + gateway_resume + op8 green.
- `cargo fmt -p sr-sdk --check`, `cargo clippy -p sr-sdk --all-targets -- -D warnings` (default and `--no-default-features`), `cargo deny check`: clean.
- Live run, local: scratch DB `sr_p24_devplat` (dropped afterwards), sr-server on 8080 with `SR_VOICE__ENABLED=true SR_VOICE__E2EE=false SR_STORAGE__ENDPOINT=memory:`, `node e2e/sdk-compat/voice-bot-live.mjs --seconds 60`: bot joined, `connected`, dropped its SFU socket at 20 s, `resumed (0 in the channel)`, `done after 60.03s: connected=true resumed=true`, exit 0. My server was stopped by PID.
- Web: `tsc --noEmit`, eslint and prettier on `src/developers`, vitest `src/developers` (11 files, 122 tests); `packages/bot` tests (18); `node --test` in `e2e/sdk-compat` (5).

## Not run

- The new `e2e.yml` step on CI (written, never run there). The YAML was not parsed by a tool (no YAML parser in the tree); it follows the shape of the `sdk-compat.yml` server start.
- Playwright, bundle budget (no web bundle change beyond docs strings), OpenAPI regeneration (no route changed).

## Findings and decisions

- An encrypted call removes a member that sends no `hello` (op 101) within 10 s (D-263). The example has no MLS handshake, so the live run uses `SR_VOICE__E2EE=false`. A bot run in a real encrypted call (using `voice::E2eeCall`) is not done; the encrypted media path is covered by `voice_e2ee.rs` and `voice_transport.rs`.
- Handler calls in `Client` run in separate tasks, so event order seen by an `EventHandler` is not guaranteed; the resume test compares as a set.
- After a gateway session loss the SDK re-sends the join, the server answers with a new `VOICE_SERVER_UPDATE`; `voice_bot.rs` handles it with `VoiceLink::rejoin`. This path is unit tested at the gateway layer (fake server) but not in an end-to-end run.

## Open owner questions

1. RDY-DEV-001: approve `werift` (see Resume here)? The coordinator relayed an approval; I need it from the owner directly.
2. Should the live bot run also cover an encrypted call (needs the MLS loop in the example)?
