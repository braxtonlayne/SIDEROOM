# Handoff: fix/firefox-e2ee (voice E2EE on Firefox in CI)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\fix-firefox-e2ee`, branch `fix/firefox-e2ee` (base main `3d353e8`). Local commits only: nothing pushed, no PR. No migrations, no Rust or web client code changed. **The E2EE defaults were NOT flipped.**

## Resume here

**State:** the Firefox failure in CI run 39157431844 is an ICE failure between Firefox and the SFU's `127.0.0.1` candidate. It is not in E2EE. The fix is in the CI step: the SFU now announces the runner's default-route address. Nothing here could run Firefox: it does not start on this Windows host ("side-by-side configuration is incorrect"). So the fix is untested on Firefox until CI runs it.

**Next steps (owner or coordinator):**

1. Push `fix/firefox-e2ee` and let `e2e.yml` run. Look at the `live` job's `Voice E2EE (Firefox, WebKit)` step. Its log prints `SFU announces ["10.x.x.x"]`.
2. Re-run the `live` job until Firefox passes **5 times in a row**. Locally the equivalent is `--repeat-each=5`; in CI, add `--repeat-each=5` to that step's command for one push, or re-run the job five times.
3. Then flip the defaults per D-249, in a separate change:
   - `crates/sr-core/src/config.rs`: `voice.e2ee` default on, plus its doc comment.
   - `apps/web/src/voice/e2ee/flag.ts`: default on, plus its doc comment.
   - Spec 5.x and doc 12. Tick VOX-E2E-003.
   - In `.github/workflows/e2e.yml`, drop `continue-on-error` for Firefox. Split the step if WebKit still skips, so the Firefox run is required on its own.
4. If Firefox still fails, read "If it still fails" below first.

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
