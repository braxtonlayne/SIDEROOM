# Handoff: fix/firefox-e2ee (voice E2EE on Firefox in CI, then E2EE on by default)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\fix-firefox-e2ee`, branch `fix/firefox-e2ee` (base main `3d353e8`), draft PR #38. No migrations. **The E2EE defaults are now ON** (D-249), in local commits on top of the pushed ones: not pushed, the PR not edited.

## Resume here

**State:**
- Pass 1 (pushed): the Firefox failure in CI run 39157431844 was ICE, not E2EE: Firefox never paired with the SFU's `127.0.0.1` candidate. The `live` job now announces the runner's default-route address. Root cause and evidence below.
- Proof: CI run 39224043323, job `live`: the Firefox voice-e2ee spec passed **5 of 5** with `--repeat-each=5 --retries=0`; the required Chromium E2EE step passed; WebKit skipped (Linux WebKit has no `RTCRtpScriptTransform`).
- Pass 2 (local commit `feat(VOX-E2E-003): turn voice E2EE on by default`): the defaults flipped, the Firefox step is required and runs once, and WebKit is its own `continue-on-error` step. See "Pass 2" below.

**Next steps (owner or coordinator):**
1. Push the branch and check the `live` job: `Voice, Activities and message components (Chromium)` now runs voice and Activities calls with E2EE on (defaults), and `Voice E2EE (Firefox)` is required.
2. Owner review points: the new `sr_e2ee` field in `VOICE_SERVER_UPDATE` (the server decides whether a call is E2EE; see "Pass 2"), and the bot lockout (DEV-BOT-007).

## Pass 2: E2EE on by default

| What | Where |
|---|---|
| `voice.e2ee` defaults to `true`; config test covers the default and `e2ee = false` | `crates/sr-core/src/config.rs` |
| `VITE_SR_VOICE_E2EE` is on unless `0` or `false` | `apps/web/src/voice/e2ee/flag.ts` |
| `VOICE_SERVER_UPDATE` carries `sr_e2ee` (the server's `voice.e2ee`); gateway tests assert both values | `crates/sr-gateway/src/voice.rs`, `voice_e2ee_tests.rs` |
| The controller runs E2EE only when the server says `sr_e2ee: true`; otherwise it joins DTLS-SRTP only and sends no op 101 | `apps/web/src/voice/controller.ts`, `voice/e2ee/serverFlag.ts` |
| Refusal copy (D-263): "Your browser can't join encrypted calls; use a current Chrome, Edge, Firefox or Safari." A client that declared E2EE and is still refused (no `hello` in 10 s) gets "End-to-end encryption could not start..." instead | `i18n/messages/en/p15-k2-1.ts`, `p14-e2ee-1.ts`, `controller.ts` |
| Settings > Advanced shows "encrypted in transit" once a call showed the server has E2EE off | `settings/user/sections/AdvancedVoice.tsx` |
| CI: Firefox E2EE a single required run (report, traces and server.log kept as `*-firefox`); WebKit a separate `continue-on-error` step | `.github/workflows/e2e.yml` |
| Docs: server.md row; spec section 5 status and "Browsers"; doc 05 `VOICE_SERVER_UPDATE`; doc 12 operator note (how to turn it off); threat model T16; VOX-E2E-003 ticked with the CI evidence | `docs/` |

**Why `sr_e2ee` in `VOICE_SERVER_UPDATE`.** Without it, a server with `voice.e2ee = false` and a web build with E2EE on (now the default) would run the transform with no MLS group, and the fail-closed transform drops every frame: silent calls. Now the server switch alone turns E2EE off, with no rebuild. Trust: the server serves the web app, so it could already ship a build without E2EE; following its word adds no power. The user sees it (no shield, no codes). Recorded in T16.

**Browsers** (MDN browser-compat-data, checked 2026-10-19): `RTCRtpScriptTransform` in Chrome/Edge 141+, Firefox 117+, Safari 15.4+ (macOS and iOS); older Chromium uses `createEncodedStreams`. Chromium and Firefox are proven in CI; **Safari is untested**.

**Can lock users out:**
- Browsers with neither API (old Chromium without `createEncodedStreams` is unlikely; old Safari < 15.4; some embedded webviews) are refused with the message above. Intended by D-263.
- **Bots in voice:** any bot or Discord-library voice client that does not speak MLS gets `e2ee_required` (DEV-BOT-007: the SDK has the MLS half but no transport). Operators who need voice bots must set `voice.e2ee = false`.
- A build made with `VITE_SR_VOICE_E2EE=0` against a server with `voice.e2ee` on: every join refused. Documented in doc 12.
- Safari: has the API but untested; a Safari-specific transform bug would make its calls silent or fail.

### Pass 2 verification (Windows host)

| What | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy -D warnings --all-targets` on sr-core, sr-gateway, sr-e2ee | ok |
| `cargo test`: sr-core (62), sr-gateway (177), sr-voice + sr-e2ee (all pass, incl. loopback), sr-api (613 lib + integration targets), sr-server (53 + the voice Postgres tests with `--include-ignored` on scratch DB `sr_ff_default`) | all pass |
| web typecheck, eslint, prettier on touched files | ok |
| vitest `--maxWorkers=2`, voice/lounge-voice/i18n/settings subset | 358 passed |
| vitest full suite | 2319 of 2320; the one failure (`admin/__tests__/p10.test.tsx`, a 5 s timeout under load) passes alone and is unrelated |
| web build + bundle budget | ok |
| `node --test scripts/*.test.mjs` | 13 passed |
| e2e `tsc --noEmit`; e2e.yml parsed (js-yaml) | ok |
| Playwright Chromium, sr-server with **no** `SR_VOICE__E2EE` and Vite with **no** `VITE_SR_VOICE_E2EE` (both defaults): `tests/voice-live.spec.ts` | 1 passed |
| same defaults: `e2ee/voice-e2ee.spec.ts` (MLS group, codes, encrypted frames, observer sees only ciphertext) | 1 passed |
| sr-server with `SR_VOICE__E2EE=false`, same default-on web build: `voice-live.spec.ts` | 1 passed (audio arrives: the off switch works without a rebuild) |
| Firefox, WebKit | not run locally (Firefox does not start here; Windows WebKit has no WebRTC); CI covers them |

Servers and Vite I started were stopped by PID; scratch DBs `sr_ff_default` and `sr_ff_e2e` dropped.

---

# Pass 1 (history)

## Root cause

| Evidence (run 39157431844, `live` job, artifact `e2e-live`) | What it shows |
|---|---|
| Both Firefox tries time out in `joinVoice` (`e2e/support/voice.ts:138`), 45 s. | The first client never reaches "Voice Connected". |
| Trace console: `WebRTC: ICE failed, add a STUN server and see about:webrtc for more details`. It comes about 5 s after the engine starts (try 1: click 9.0 s, ICE failed 15.5 s; retry: click 67.4 s, ICE failed 73.9 s). | ICE failed. 5 s is Firefox's `media.peerconnection.ice.trickle_grace_period` (5000 in this build's `greprefs.js`). Firefox fails at that point when it has **no candidate pair at all**. Failing checks would take longer (7 transmits). |
| A `pageError` 15 s after engine start: `Closed before resume completed`, from `mic.stop` ← `engine.teardown` ← `failConnect` ← the `connectTimer`. | The engine's 15 s `CONNECT_TIMEOUT_MS` fired: the peer connection never reached `connected`. The controller then rejoins and gives up; the screencast ends on "Couldn't join voice / You were disconnected from voice." |
| The network log has exactly one `ws://localhost:5173/voice/ws`, the E2EE WASM (`e2ee_bg.wasm`) and the worker (`worker.ts?worker_file&type=module`). There is no E2EE error in the console. | The gateway join, the D-263 gate (the call got as far as `startEngine`, which runs only after `E2eeCall.start()` sent `hello`), the WASM and the module worker all worked. The stall is in WebRTC transport, before any frame reaches the transform. |
| `voice.announce_ips` defaults to `["127.0.0.1"]`. The CI step did not change it. | The SFU's only candidate is loopback. |

Why Firefox gets no pair with `127.0.0.1`:
- Firefox's ICE stack (nICEr) pairs a loopback remote candidate only with a loopback local candidate.
- It gathers loopback candidates only with `media.peerconnection.ice.loopback` (the config sets it), and only when it gathers on all interfaces.
- When the page has no camera or microphone permission (Playwright cannot grant those to Firefox) and Firefox does not count it as capturing, Firefox gathers only the default-route address.

The last point is my reading of Firefox's behaviour, not something I observed. The timing shows that no pair formed. Announcing a routable address works whichever of these applies: the default-route address is what Firefox gathers in every mode.

Chromium is unaffected: `--allow-loopback-in-peer-connection` makes it gather loopback, and it pairs loopback with loopback.

## The fix

- **`scripts/ice-announce.mjs`** (new) prints `["<default-route IPv4>"]`, or `["127.0.0.1"]` with a warning when there is none. It finds the address the same way Firefox does: it connects a UDP socket to TEST-NET-1 (nothing is sent) and reads the source address. Tested in `scripts/ci-scripts.test.mjs`.
- **`.github/workflows/e2e.yml`, `Voice E2EE (Firefox, WebKit)`:**
  - Exports `SR_VOICE__ANNOUNCE_IPS="$(node scripts/ice-announce.mjs)"` before starting sr-server. The SFU binds `0.0.0.0`, so it receives on that address.
  - Adds `SR_LOG__FILTER=info,sr_voice=debug,str0m=debug`, so the next `server.log` has SFU signalling and ICE detail.
  - Starts a PulseAudio null sink, as a best effort (see below).
  - The required `Voice E2EE (Chromium)` step is unchanged.
- **WebKit probe** (`e2e/e2ee/voice-e2ee.spec.ts`):
  - It now runs on the app's origin, a secure context like the call, instead of `about:blank`.
  - It records the user agent, `isSecureContext` and each capability as a `capabilities` annotation in the HTML report.
  - The CI skip reason was "webkit here has no encoded-frame transform API": RTCPeerConnection exists, but RTCRtpScriptTransform does not. Playwright's Linux WebKit (WebKitGTK/WPE) most likely does not expose it. The annotation will tell. If it still skips there, WebKit on Linux lacks the API and the skip is correct; WebKit proof then needs macOS Safari.
- **Docs and comments:**
  - `docs/dev/server.md` (`voice.announce_ips`): the Firefox note and the local command.
  - A comment on the Firefox prefs in `e2e/playwright.e2ee.config.ts`.

### The audio side (a second suspect, guarded)

`Closed before resume completed` means the mic pipeline's `AudioContext.resume()` was still pending after 15 s. The console shows no autoplay-blocked warning (Firefox would log one), so the likely cause is no audio output device on the runner. Firefox completes `resume()` only once its audio callback driver starts.

The mic graph should still run on Firefox's fallback clock, so this may not matter. But if it does, no audio frames would be encrypted. The step therefore installs `pulseaudio` and loads a null sink. If that fails, it prints "PulseAudio null sink not available" and goes on.

## If it still fails

- **ICE failed again:**
  - Check the `SFU announces` line. It must be a `10.x`/`172.x` address, not `127.0.0.1`.
  - Read `server.log` (str0m debug) for STUN binding requests from the runner address.
  - Next lever: Firefox prefs `permissions.default.microphone` and `permissions.default.camera` = 1, so Firefox treats the page as permitted and gathers on every interface. Unverified.
- **"Voice Connected" reached, but the frame counters stay 0:**
  - Audio side: check the null sink line in the step log.
  - Transform side: `workerError` is in the poll message.
  - Video: Firefox may not report `mimeType` in frame metadata; the transform falls back to sniffing VP8/H.264.
- **The observer (`eve`) sees no `SR` trailer:** the observer uses its own RTCRtpScriptTransform. Firefox supports it, so this would be a real finding.

## Local verification (Windows host)

The local setup:
- debug sr-server: `SR_VOICE__ENABLED=true`, `SR_VOICE__E2EE=true`, `SR_STORAGE__ENDPOINT=memory:`;
- `SR_VOICE__ANNOUNCE_IPS="$(node scripts/ice-announce.mjs)"`, which gave the host's LAN address;
- the same `SR_LOG__FILTER` as CI;
- Vite with `VITE_SR_E2E=1 VITE_SR_VOICE_E2EE=1`, on a scratch database.

| What | Result |
|---|---|
| `e2ee/voice-e2ee.spec.ts`, Chromium, SFU on the LAN address | 4 of 4 (1 + `--repeat-each=3`). `server.log` shows `ICE nominated send from: 192.168.x.9:5000x to: 192.168.x.9:...` and DTLS connected for all three clients. Media really went over the routable address, and figment parsed the env list. |
| same, WebKit (Windows) | skipped: no WebRTC on this host. The new `capabilities` annotation was written to the report. |
| same, Firefox | **not run**: Firefox does not start on this host. |
| `node --test "scripts/*.test.mjs"` | 13 passed (includes the new ice-announce test) |
| e2e `tsc --noEmit`; prettier on every touched file | ok |

Not run: Rust gates and web vitest. No Rust and no `apps/web` code changed.

Housekeeping:
- My first Playwright attempt lacked `SR_CLI_BIN`/`DATABASE_URL`, so `global-setup` fell back to `cargo run -p sr-cli` and built `sr-cli` into this worktree's `./target` (gitignored, about 12 GB). Delete `C:/Users/braxt/Desktop/SIDEROOM_wt/fix-firefox-e2ee/target` to reclaim it; the real build dir is `fix-firefox-e2ee-target`.
- All servers I started are stopped. The scratch DB `sr_fix_ff` was dropped.
