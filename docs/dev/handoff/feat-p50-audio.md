# Wave 50 handoff: audio (UTL-AUD-001)

Branch `feat/p50-audio`, worktree `p50-audio`. Not pushed.

## Done
- **Soundboard upload swap.** `SoundboardTab`'s upload dialog now embeds the shared `AudioTool` (`apps/web/src/tools/audio`) with the soundboard's limits (5 s, 512 KiB, WAV out). The old D-201 trimmer (`settings/server/sections/soundTrim.ts`, `SoundTrimmer.tsx`, their test and the `ss-trimmer` CSS and i18n keys) is deleted. The decode helpers the shared tool and the voice recorder used (`decodeAudioFile`, `peaks`, `durationOf`, `PcmSource`, `TRIM_SAMPLE_RATE`) moved to `tools/audio/pcm.ts` with their test. `AudioTool` gained the old trimmer's stereo-to-mono fallback when a WAV is over `maxBytes`, and an intro line when the file is longer than the limit (`p50-audio-1.ts`). Server contract unchanged (same `POST .../soundboard-sounds` data URI). Behaviour difference: the part is chosen with a start and an end handle (capped at 5 s) instead of one start slider, and the cut is named `<name>-trimmed.wav`.
- **Playwright case** `e2e/tests/audio-trim.spec.ts` (picked up by the a-f split; zero registrations; Chromium only; WAV generated in the test): drawer tool cuts a 12 s WAV to 3 s and exports exactly 44 + 3*48000*2 bytes; a recorded voice message (fake mic) is trimmed and sent, asserting the upload slot request is `*-trimmed.webm` / `audio/webm` (Chromium records WebM Opus; the voice path takes WebM since FU-7: `voice_meta.rs analyze_webm_opus`, `lounge/msg/voice.ts`), and with `SR_E2E_LIVE=1` (CI's Garage) that the voice message shows in the log; the soundboard upload trims to 5 s and gets 201 with a `data:audio/wav` body.
- UTL-AUD-001 ticked in `docs/inventory/21-feature-phase.md`.

Commit: `UTL-AUD-001: soundboard upload uses the shared audio tool; old D-201 trimmer removed` plus the spec/inventory/handoff commit.

## Not done / findings
- **Voice send was not proven end to end locally**: the bare stack uses `memory:` storage, which a browser cannot upload to, so the local run checks the trimmed WebM reaches the attachment request; the final "voice message appears" assertion runs only where `SR_E2E_LIVE=1` (the E2E workflow's Garage job). First CI run should confirm it.
- **Pre-existing bug, not fixed: the utility drawer has no theme root.** `DrawerHost` renders outside every section's `ThemeProvider`, so `--ui-*` tokens are undefined there: the Drawer panel is transparent with unthemed text over the Lounge (see `drawer-audio-*` shots). The same fix as the player dock (`ThemedDock` in `PlayerHost.tsx`: wrap in `ThemeProvider` with `useAppliedTheme(undefined,'lounge')`) crashed the page in Chromium when the tool opened (reverted), so it needs its own investigation. Suggest a separate task for the UTL-SHL drawer.
- The local Vite stack needed stub `packages/wasm/dist/music` and `dist/tools` modules (not in the copied dist); they are git-ignored, nothing committed.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p50-audio/`: `soundboard-upload-*` and `drawer-audio-*` for Editorial, Lounge Light and Lounge Dark, desktop 1280x800 and phone 375x812. The soundboard dialog reads correctly in all six; the drawer shows the transparency bug above (tool content itself is fine).

## Tests run
vitest: `src/tools`, `src/settings/server`, `src/lounge/msg`, `src/lounge/__tests__` (maxWorkers 2); web `tsc --noEmit`; eslint and prettier on changed files; `check-css-tokens`, `i18n-audit` clean; Playwright `audio-trim.spec.ts` 3/3 on Chromium; `playwright test --list --project chromium` loads all 906 tests in 68 files. No Rust touched, no build/entrance check needed (no public-surface code).

## Owner questions

**Answered: D-533 (keep the handles; the start handle slides the window at the limit: built) and D-534 (Opus for soundboard sounds: next round, not built).** Questions below are kept for the record.
- Soundboard part selection now uses start and end handles (shared tool) rather than a fixed 5 s window slider; keep, or add a "fixed window" mode?
- WAV is the only soundboard output (as before); should the soundboard accept Opus too (D-201 says sounds are stored as uploaded MP3/Ogg/WAV)?
