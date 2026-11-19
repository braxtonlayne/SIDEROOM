# Handoff: fix/p115-vad (wave 115, D-1048, D-1050, D-1051, D-1052, VOX-CLI-002)

## Clock times

Started 06:20 on 2026-11-19 (branch from `fix/p115-assets` at 1fe10d9e9); last job ended about 06:35.

## Done (commits, all on `fix/p115-vad`)

- `75a7e0ed8` the pieces: probability gate, worklet, worker, client, host, setting, controller and mic wiring, settings UI, strings.
- `613eae7db` the tests of the controller, host and client; the voice spec sentence.
- `0b6bb7329` a type cast in a test (found by the tsc job); then this handoff, `D-1050 ... (ALL READY)`.

### The nine pieces and their tests

1. `silero/probGate.ts` (`ProbGate`, open at 0.5, close after under 0.35 for `HANG_MS`; between the two thresholds an open gate is kept). Test `silero/__tests__/probGate.test.ts`: thresholds, hang, single spike, flutter around 0.4, dip shorter than the hang, reset.
2. `silero/frames.worklet.ts` + `silero/resampler.ts`. Resampler: **streaming area average** (a box filter then decimation, fractional edges weighted), to 16 kHz mono, 512-sample frames posted as transferable `Float32Array`. Why it is enough: a detector reads the speech band; the box filter lessens aliasing (measured: a 23 kHz tone comes out at about a third of the amplitude of a 500 Hz tone, so it does not remove it) and nothing is ever played or sent. Channels are averaged without allocating; the only allocation is each frame handed on. Tests `frames.test.ts`: frame size and count at 48 kHz and 44.1 kHz (no drift over 10 s), amplitude of a tone, fresh buffer per frame, and the worklet itself with stubbed globals (registers `sr-silero-frames`, averages a stereo input, transfers the buffer).
   **Loading:** `import workletUrl from './frames.worklet.ts?worker&url'` then `audioWorklet.addModule(workletUrl)`. Build (job 0360) emitted `app/frames.worklet-<hash>.js`, 1,334 B, a self-contained ES file ending in `registerProcessor("sr-silero-frames", ...)`, same origin, so `script-src 'self'` covers it (not run in a browser by me: see "not done").
3. `silero/vad.worker.ts` + `silero/runner.ts` + `silero/protocol.ts`. Worker: `init` -> `fetch` of our model URL, `import('onnxruntime-web/wasm')` (only here), `numThreads = 1`, `proxy = false`, `wasmPaths = { wasm: <our wasm url> }` set before the session is made, `InferenceSession.create(bytes)`, answers `ready` or `error` (`model-fetch`, `runtime-load`, `session`); each frame answers `{prob, ms, meanMs, maxMs}` or `frameError`; `reset` zeroes the state. `SileroRunner` (pure, session injected) serialises frames, carries `stateN` and the context. Test `runner.test.ts`: inputs and dims (`[1, 576]`, `[2,1,128]`, int64 `[]` = 16000n), carried state and context, order, stats, failure, reset.
   **Verified facts (the open question in the assets handoff):** the frame is **512 samples (32 ms)** and v5 **needs the 64-sample context (input 576)**. Checked in Node with onnxruntime-web 1.31.1 on the model, with two Zonos speech clips (not committed): with the context, speech frames above 0.5 were 127 of 162 (mean probability 0.77) and 144 of 256 (0.51); without it 99 of 162 (0.60) and 39 of 256 (0.21). In that run inference took 0.4 to 0.5 ms a frame on average (worst 30 ms, the first frame).
4. `silero/sileroVad.ts` (`SileroVad`, state `idle|loading|ready|fallback` with a reason, `onProb`, `onState`, `start(ctx, source)`, `stop()`, `reset()`), fallback reasons and how each is tested (`sileroVad.test.ts`, fakes for worker, node, context):
   | reason | when | test |
   |---|---|---|
   | `model-fetch`, `runtime-load`, `session` | the worker's `error` | one case loops all three |
   | `worker` | `createWorker` throws, `worker.onerror`, `onmessageerror` | case "worker dies" |
   | `worklet` | `addModule` rejects or throws, the node's `onprocessorerror`, the port's `onmessageerror` | case "worklet cannot be added" |
   | `slow` | inference `ms` above the frame length for 3 s (five in-time frames in a row forgive it), or 94 frames (3 s) waiting for the worker | two cases + the forgiveness case + the backlog case |
   | `inference` | 10 `frameError` in a row (a good frame resets the count) | one case |
   | `chunk` (the host, not the client) | the dynamic import of the client's chunk fails | `p115VadHost.test.ts` |
   `stop()` terminates the worker (frees the session) and disconnects the nodes; late messages are ignored (tested).
5. `settings.ts`: `vadEngine: 'energy' | 'silero'`, default `'energy'`, sanitised (anything else becomes `'energy'`), persisted `version` 1 -> 2 with a `migrate`. Tests in `voice/__tests__/p115VadHost.test.ts` ("vadEngine setting"): default, sanitising, a real version-1 store rehydrates as energy with its other values kept, persists at version 2.
6. Controller and microphone (see "What the detector hears"). Test `voice/__tests__/p115VadController.test.ts`: the engine gets `vadRole: 'call'`; while loading the energy gate decides as before; once ready the detector decides (loud meter but silence from the model stays closed; speech from the model with a silent meter stays open); on fallback the energy gate takes over at once and one notice shows, once per call (a second call may show it again); a settings-test fallback gives no call notice; Push to Talk is untouched by a ready detector.
7. `window.__srVadStats` (only when `import.meta.env.VITE_SR_E2E` is set): defined as a getter on `window` in `mic.ts`, so it is always current. Shape `{ engine, state, frames, speechFrames, meanMs, maxMs, fallbackReason, frameMs }`. **`frameMs` is present and is 32.** `engine` is the setting's value (`'silero'` stays while in fallback; `state` and `fallbackReason` say what is running). Figures are those of the last detector until the engine is switched off (then zeros). Not unit-tested as a window property (the snapshot is: `p115VadHost.test.ts`).
8. Settings UI, `#us-voice-mode`, voice activity mode only: a `SegmentedControl` labelled "Voice detection" with "Standard" and "Silero (on-device model)"; with Silero chosen three lines: the explanation ("Runs on this device. Nothing is sent anywhere. Downloads about 18 MB the first time."; 18 is the 17.3 MB raw sum of the model, wasm and chunk rounded up; gzip is about 6 MB), the status line `data-testid="vad-engine-status"` with `data-state` (no `role`, on purpose, so vadshots's notice selector does not take it), and "The model decides when you are speaking, so sensitivity does not apply." The automatic-sensitivity switch and the threshold slider are disabled; the meter stays. Status texts: idle "Not loaded yet. It loads when you join a call or press "Let's check"."; loading "Loading the model..."; ready "Ready. The model is listening to your microphone on this device."; fallback "The model could not start, so standard voice detection is in use. It tries again next time." Test `settings/user/__tests__/p115Vad.test.tsx`: the control and its options, absent in Push to Talk, choosing updates the setting and disables the energy controls, all four states of the status line, and the microphone test with Silero chosen (in jsdom there is no `Worker`, so the real client reports a fallback with reason `worker` and the test goes on).
9. Docs: `docs/specs/lounge-voice-video.md` line 14 (the audio paragraph) now describes the second engine: off by default, device-local, on-device, what it downloads (about 18 MB, own origin), the fallback, before the encoder and E2EE. **"The Handbook's voice page" does not exist in this tree**: `apps/web/src/help/Handbook.tsx` is the operator handbook and has no voice-activity sentence (checked with `git grep`), and `docs/inventory/**` is fenced; so only the spec changed. Proposed sentence for any user-facing help page: "Voice detection: Standard listens for loudness. Silero (on-device model) is a small speech model that runs on this device; it is off by default, sends nothing anywhere, and downloads about 18 MB the first time you join a call or press Let's check."

## What the detector hears (the tap) and why

The detector taps the **`MediaStreamSource` node, before the analyser, the input gain and the gate gain**. The energy gate hears the analyser, which is wired straight to that same source, so before the gain too: both read the unscaled microphone (after the browser's own echo cancellation and AGC), and neither depends on the member's input-volume slider. The worklet output goes to a gain of 0 into the context's destination (so the graph pulls the node, and nothing is heard).

## How it is wired (who owns what)

- `vad.ts` gains `VadEngineHost` (small, no detector code): it keeps the tap of the microphone graph, starts or stops the detector as the setting, the input mode and the tap say, reaches the client only through the dynamic `load()`, exposes `state`, `reason`, `drives`, `open`, `role`, `subscribe`, `snapshot`, and holds a fallback for the rest of the call (until `resetBlock()`, called by the controller when an engine starts, or the engine is switched off).
- `mic.ts`: one `vadHost` instance, `MicOptions.vadRole` (optional; the controller passes `'call'`, the settings test passes nothing = `'test'`; a call wins over the settings test), `setTap` after the graph is built, `clearTap` in `stop()`, a settings subscription that calls `vadHost.refresh()` on an engine or mode change, and the `__srVadStats` getter.
- `controller.ts`: `updateVadOpen` (the detector's verdict while `drives`, else the energy gate's, which keeps running), `onVadChange` (the notice, `t('voice.vad.fallbackNotice')`: "The Silero voice detector could not start, so this call is using standard voice detection."), `vadNoticed` reset in `leave()`, `micOptions` carries `vadRole: 'call'`.
- With engine `'energy'` the host's `wanted()` is false: `import('./silero/sileroVad')` is never called, so none of the Silero code is fetched.

## Fallback notice (for vadshots)

Text: "The Silero voice detector could not start, so this call is using standard voice detection." Shown through the call panel's existing notice: `lounge/voice/VoicePanel.tsx` renders `s.notice` as `<Notice variant="info" role="status">`, so it is a `[role=status]` containing "Silero", "detector" and "detection". It can be dismissed (existing behaviour).

## Privacy statement (D-1052), checked

`git grep -n "fetch(\|XMLHttpRequest\|WebSocket\|sendBeacon" -- apps/web/src/voice/silero` shows one line: `vad.worker.ts:25`, the worker's `fetch(modelUrl)` of our own hashed model file. The runtime fetches our own wasm from `wasmPaths`. No `console` call exists under `silero/`. Frames go worklet -> page -> worker inside the page; probabilities never leave it; the only thing that crosses to the controller is a boolean.

## Not done / left

- **Browser proof that a session creates under the CSP and that the worklet loads: not run by me** (I may not start a server or Playwright; no spec of mine). What is proven: the build (job 0360) emits the worker, worklet, runtime chunk, model and wasm; the same model and wasm run in Node with the same runtime (session created, probabilities as above); the CSP is unchanged and every URL is same-origin. vadshots' mic-test case (status `ready`) is the first browser proof; if the session fails there, the worker reports `session` or `runtime-load` and the call falls back, so the failure is visible in `__srVadStats.fallbackReason`.
- Vite dev server: `optimizeDeps.exclude` for `onnxruntime-web` was **not** added (`vite.config.ts` untouched). Built mode is what the specs use. If the dev pre-bundler breaks the worker's import, add `'onnxruntime-web'` to the exclude list in `vite.config.ts`.
- The mean and worst inference time and bytes in the browser: vadshots measures them. Node figures above only.
- No unit test drives the real worklet on a real audio thread or the real worker with the real runtime; the unit tests use fakes for those two (the logic around them is covered).
- Full voice and lounge test suites, eslint: left to the commander and CI. I ran the files below.

## Checks (own tree, last commit)

- tsc job 0362: rc 0 (the first, 0361, found one cast in a test; fixed).
- vitest jobs: 0359 (7 files, 48 tests) rc 0; 0363 (10 files, 111 tests: hooks, engine, engineE2ee, engineMusic, engineVideo, e2eeCall, stageController, p11userset, callUi, runner) rc 0; 0358 (the new tests plus voice.test, logic.test, controller.test, media.test) had 2 failures in my new tests (a tone threshold and a radio label), both fixed and re-run in 0359. The existing `logic.test.ts`, `controller.test.ts`, `voice.test.tsx` passed unedited; no existing test was edited.
- build job 0360 rc 0; `node scripts/check-vad-assets.mjs` on that dist: "VAD asset check ok". `check-bundle-budget.mjs` rc 0. First paint entry chunk `app/app-*.js`: 218,890 B gzip against 218,457 B at assets' base (+433 B: the host class and the setting). `sw.js` has no hit for silero or ort-wasm.
- `npx prettier --check` on the voice folder, VoiceVideo.tsx, the settings test and the fragment: clean. `node scripts/i18n-audit.mjs --max 0`: TOTAL 0.

## REACH table

| file | who imports it | chunk |
|---|---|---|
| `voice/silero/**` (client, worker, worklet, runner, gate, resampler) | only `mic.ts`'s dynamic `import('./silero/sileroVad')`; the worker and worklet through `new URL` and `?worker&url` from the client | own lazy chunks: `sileroVad-*.js` 4.3 KB, `vad.worker-*.js` 2.3 KB, `frames.worklet-*.js` 1.3 KB, plus assets' runtime chunk |
| `voice/vad.ts` (host class), `settings.ts`, `mic.ts`, `controller.ts` | the voice engine, controller, settings page | the app's existing voice chunk(s); `controller.ts` and `settings.ts` were already in the app shell; no entrance or public page imports them |
| `settings/user/sections/VoiceVideo.tsx` | the user settings route (lazy) | settings chunk |
| `i18n/messages/en/p115-vad-1.ts` | the i18n glob | with the other fragments |

## FINDERS (rule 13)

- Tests that build `VoiceSettings` or read the card: all use `DEFAULT_VOICE_SETTINGS`, `sanitizeVoiceSettings` or the store (`voice.test.tsx`, `p11userset.test.tsx`, `p15Advanced.test.tsx`, `logic.test.ts`, `controller.test.ts`, `soundboard.test.ts`, `stageController.test.ts`, `video.test.ts`, `videoPath.test.ts`, `callUi`, `voiceUi`, `incomingCall`, `p11ShellDevices`); none writes a literal full `VoiceSettings`, so none needed the new field. `logic.test.ts:210` (`sanitizeVoiceSettings(null)` equals the defaults) still holds.
- `voice.test.tsx` "Voice Activity: automatic sensitivity..." finds the switch and slider by name: still true with Standard chosen (passes).
- `e2e/`: no spec reads `#us-voice-mode`, "Input sensitivity", "Automatically determine..." or `sr.voice.settings` (`git grep`), apart from vadshots's new spec.
- Tests that read my files as text: `git grep -n "mic.ts\|vad.ts\|settings.ts"` found none in `scripts`, `packages` or `e2e` (the `git grep` of this wave returned nothing).

## JOBS

| id | class | rc | note |
|---|---|---|---|
| 0357-vad-vitest | vitest | 1 | paths given from the repo root; the runner runs in `apps/web` (use `src/...`) |
| 0358-vad-vitest | vitest | 1 | 2 failures in new tests, fixed |
| 0359-vad-vitest | vitest | 0 | 7 files, 48 tests |
| 0360-vad-build | build | 0 | about 20 s |
| 0361-vad-tsc | tsc | failed (rc file 2, wait 1) | one test cast, fixed |
| 0362-vad-tsc | tsc | 0 | |
| 0363-vad-vitest | vitest | 0 | 10 files, 111 tests |

(All started at once, no queue; the run folder holds `submitted/started/ended/rc`.)

## Threat-model row (proposed text)

| T-new | The member's microphone audio passes through a downloaded model and its runtime (Silero VAD in a worker), a place audio could be copied to if the code were wrong or the files were swapped | The audio stays in the page: the worklet posts 16 kHz frames to a dedicated worker over `postMessage`; the worker holds the session and sends back only a number; no code under `voice/silero` opens a socket, a beacon or a request with audio or a probability (only `fetch` of the model's own URL; the runtime fetches only its own wasm; the CSP stays `connect-src 'self'` with `script-src 'self' 'wasm-unsafe-eval'`); the model is pinned by SHA-256 and the runtime to one exact version, both served from our origin and never a CDN (`scripts/check-vad-assets.mjs`); nothing is fetched before a member who chose Silero joins a call or tests the microphone; the detector sits before the encoder and the E2EE transform and changes neither | Built in wave 115 (D-1049, D-1052) |

## Changelog sentence

Added an optional on-device voice detector (the Silero model, off by default) as a second way for Voice Activity mode to decide when you are speaking, with an automatic fall-back to the standard detector.

## Owner questions (to Fable)

1. The first use downloads about 18 MB raw (about 6 MB compressed), mostly the 14.8 MB runtime wasm. Default chosen: keep it, off by default, stated in the setting's own line.
2. While a call is in Push to Talk or the channel forces it, the detector still starts if the setting is Voice Activity + Silero and the forced mode applies only in the controller. Default chosen: accept the small waste (rare).

## PROCESSES

`strays: none found`. No file outside my row was edited (`vite.config.ts`, `hooks.ts`, `engine.ts` untouched; the settings-test path works through `mic.ts` alone).

## Addendum (Fable's answer to owner question 2)

The detector now starts only when the EFFECTIVE input mode is voice activity. `mic.ts` `wanted()` uses `effectiveInputMode(settings, session.pttForced)` instead of the raw setting, and `VoiceController.setPttForced` calls `vadHost.refresh()` so a change of the force starts or stops it. A call forced to Push to Talk, or a member on Push to Talk, never loads the client, downloads nothing and runs no model. Question 2 of the owner questions above is therefore closed (cured, not accepted).

- Commit `8ba847278`. New unit test in `voice/__tests__/p115VadController.test.ts`: "a call forced to Push to Talk never starts the client, and it starts when the force is lifted" (engine silero; zero loads while forced; one load after `setPttForced(false)`).
- Jobs: 0367-vad-vitest rc 0 (7 files, 126 tests: the new and old controller, host, engine, hooks, settings voice tests); 0369-vad-tsc rc 0. (0365 cancelled and replaced after a scope fix in the test; 0368 was never created.)
- The settings card's download line is unchanged ("about 18 MB"); the measured figure is to come from vadshots.

## Addendum 2 (two faults from vadshots's after round)

1. **Wide "Voice detection" bar: cured.** The bar was a direct child of `.us-form-stack` (a flex column), which stretches it; the Input mode bar above sits outside it. It now sits in a `<div className="us-row-field">` (a flex row), so it keeps its own width. Only the new control moved. Test: `p115Vad.test.tsx` first case checks it is inside `.us-row-field`. Job 0381-vad-vitest rc 0 (2 files, 21 tests). Commit: see git log ("the Voice detection bar keeps its own width").
2. **Speaking state on for about 0.3 s on the beep: no change made; not shown to be a start-up artifact.** I ran the model in Node (fresh zero state and zero context, as at start-up) on synthetic beeps at 440, 800 and 1000 Hz (0.1 s bursts each second, starting at 0 s, 0.5 s and after 0.3 s of silence): no frame reached even 0.3. So the first frames with an unprimed state do not misfire on a clean tone, and a warm-up skip would be a guess. The likely cause is in the real signal, not the model: Chromium's fake device after echo cancellation and AGC (a transient or ramp at the first beep) read by the model, and one frame at 0.5 or more plus the hang gives about 0.3 s. A fix would have to change the thresholds or the hang, which was ruled out. The thresholds and hang are untouched. If the owner wants it gone, the options are a short warm-up that ignores the first frames, or a second frame needed to open; both are Fable's call.
