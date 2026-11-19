# fix-p115-vadshots handoff (ALL READY)

## Clock
06:09 start; spec 06:15; base 06:17; merge of fix/p115-vad (c32b06c35) about 06:36; after parts 1 to 5 done 06:53.

## Done (commits on fix/p115-vadshots)
`e2e/tests/p115-vad-shots.spec.ts` (five parts), the merge of fix/p115-vad (holds assets), this handoff. e2e `tsc` clean and prettier clean on the spec (06:52). No app file touched. Not run: vitest, eslint, web tsc (left to the commander).
Setup note: my worktree lacked `onnxruntime-web` in node_modules (first build 0364 failed: cannot resolve onnxruntime-web/wasm). I ran `corepack pnpm install --offline --frozen-lockfile` (1.6 s, nothing downloaded) and it built. pnpm is not on PATH here.

## JOBS (id, class, rc)
0364 build rc 1 (missing dependency); 0366 build rc 0; 0368 shoot part 1 rc 0 (spec bug: the radio inputs are hidden, clicks go to the label); 0370 shoot rc 1 (stale build: I committed after building); 0371 build; 0372, 0373, 0374 part 1; 0375, 0376 part 2 (0375 counted the service worker's copy of each file as a second request: spec bug); 0377 part 3 (1.8 min); 0378 part 4 (routes missed: the service worker answered first: spec bug); 0379 part 4 (service workers blocked); 0380 part 5. Final results: 0374 (part 1), 0376 (2), 0377 (3), 0379 (4), 0380 (5). Times: `cat cli-agents/run/jobs/<id>/{submitted,started,ended,rc}`.

## Results
### Part 1, settings UI (0374: 0 hit-test fails, 0 cut text, keyboard ok)
Options "Standard" and "Silero (on-device model)"; Standard is the default; no status line with Standard. With Silero: status `idle` ("Not loaded yet. It loads when you join a call or press "Let's check"."), switch and slider disabled, explanation lines whole (also at 375). Hit test 6 rows, 0 fail at 1280 (box reach) and at 375 (touch). Keyboard: one Tab from the Input mode control reaches "Voice detection"; ArrowRight chooses Silero. Silero and back restores the card.
After vs base, Standard chosen: Input device, volumes, Let's check, the mode control are unchanged (dx, dy, dw, dh all 0). Moved: "Automatically determine input sensitivity" and the "Input sensitivity" slider, down 50 px at 1280 (light and dark) and 58 px at 375: the new row's height. Expected (the task's "boxes equal" cannot hold for rows below a new row); a figure, not a fault.

### Part 2, nothing loads early (0376, 0 problems)
With Standard, from the Lounge through the settings, the microphone test and a joined call (622 requests): none names silero_vad, ort-wasm or the runtime chunk. Choosing Silero alone requests nothing. First request of any of them: Silero's microphone test.

| file | origin | first requested | transfer = decoded bytes | times |
|---|---|---|---|---|
| silero_vad-Dt9-bTDn.onnx | page origin (localhost:8841) | mic test with Silero, 22.6 s | 2,327,524 (raw) | once |
| ort.wasm.bundle.min-Ba0FTpUY.js | same | same | 77,326 | once |
| ort-wasm-simd-threaded-qh98TCA_.wasm | same | same | 14,848,852 | once |

Total 17.25 MB, matching "about 18 MB" in the UI. The server sends all three uncompressed here. Each also shows once more as the service worker's own fetch of the page's request (`serviceWorkerCopies` in the figures); the spec counts page requests. Requests to another origin: 0. CSP violations (securitypolicyviolation events and console): 0. Status after the test: `ready`.

### Part 3, the detector on the fake microphone (0377; Chromium's beep and generated silence, 20 s each)

| input | engine | state | frames | speech frames | speaking | mean ms | max ms |
|---|---|---|---|---|---|---|---|
| beep | Standard | idle | n/a (energy counts none) | 0 | 78.7 % of samples | n/a | n/a |
| beep | Silero | ready | 628 | 1 (0.16 %) | 1.1 % (one 0.3 s blip at 0.4 s) | 0.56 | 34.3 |
| silence | Standard | idle | n/a | 0 | 0 % | n/a | n/a |
| silence | Silero | ready | 625 | 0 | 0 % | 0.56 | 31 |

Frame is 32 ms; the mean is 1.7 % of it (limit 25 %). The model does not take the beep for a voice (1 frame in 628, limit 5 %); the energy gate does (78.7 %): the feature's difference is shown. Problem lines: (a) beep/Silero the speaking state came on once for about 0.3 s early in the run (one frame at 0.5 or more plus the hang); the task says it should stay off. (b) worst frame 34.3 ms against 32 ms: one outlier in 628 frames; the 3-second rule did not trigger.
JS heap (usedJSHeapSize, rounded by Chrome to 100 kB; the worker's heap is not in it): settings page 13.4 MB (Standard run) and 12.7 MB (Silero run), unchanged in the call after 20 s. No usable before/after difference; the wasm memory is in the worker.

### Part 4, falling back (0379, 0 problems)
Model 404: settings status `fallback` during the microphone test; in a call state `fallback`, `fallbackReason` "model-fetch", exactly 1 notice, the speaking state on for 76 % of samples on the beeps (the energy gate took over), state stayed `fallback`. Wasm aborted: `fallback`, reason "session", 1 notice, speaking 84 %. Un-routed, leave and join again: `ready` (the setting was kept). The loading state was caught with a model delayed by 6 s. The spec blocks service workers for this part, because the app's service worker otherwise answers before Playwright's routes.

### Part 5, E2EE (0380, 0 problems)
Standard and Silero (ready) calls read the same: `__srE2ee` active, status "secure", call code present, 1 member, no worker error, same connection panel text. Caveat: with Silero the speaking state stays off on the beeps (the model rejects them), so "speaking works with Silero in an E2EE call" is not shown with the beep; with Standard it works (75 %). A speech fixture would close this.

## FAULTS (none blocks)
1. vad, the card `#us-voice-mode` in `apps/web/src/settings/user/sections/VoiceVideo.tsx`: the "Voice detection" segmented control stretches the full card width (about 612 px, both options at the left, empty right) while the "Input mode" control above is compact (about 234 px). Seen in every shot. Cure asked: give it the same box as the mode control (same wrapper or class). Rows that may move: the new control only. Cosmetic.
2. vad: the early speaking-state blip on the beep (part 3). Minor; a cure would be to ignore the first frames after `ready` or need two frames to open.
3. The three files go out uncompressed in this serve (17.25 MB on the wire); not mine. Whether the production proxy compresses wasm and js was not checked.

## Shots (all opened; folder visual-sweep-shots/p115-vad)
- base-voice-mode-standard-1280-light, -1280-dark, -375-light: the card without the new control; clean, nothing cut.
- voice-mode-standard-1280-light, -1280-dark, -375-light: Standard chosen, the new bar under the mode control, other rows unchanged but pushed down; text whole; the wide bar of FAULT 1.
- voice-mode-silero-1280-light, -1280-dark, -375-light: Silero chosen, three explanation lines (download size, "Not loaded yet", "model decides"), switch dimmed, readable in dark; at 375 the lines wrap whole.
- voice-mode-silero-loading-1280-light: "Loading the model..." with the microphone test running.
- voice-mode-silero-fallback-1280-light: "The model could not start, so standard voice detection is in use. It tries again next time."
- call-fallback-notice-1280-light: in the call, the info notice in the left panel, whole, wrapped to six narrow lines, close button present.

## Not proven
Speech with Silero (no fixture); a phone's CPU; Firefox and Safari (the spec runs on Chromium only); gzip sizes; the heap effect.

## FINDERS, PROCESSES
FINDERS: none (no app file changed). PROCESSES: strays: none found.

## Commands for a re-shoot (build first; one per part, PART 1 to 5; parts 2 to 5 need the server env)
bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p115-integ/apps/web/dist --server-env SR_VOICE__ENABLED=true --env SR_P115_VAD_ON=1 --env SR_P115_VAD_PHASE=after --env SR_P115_VAD_PART=<n> --env SR_P115_VAD_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p115-vad -- tests/p115-vad-shots.spec.ts

Changelog sentence: none (test only). Owner questions: none.
