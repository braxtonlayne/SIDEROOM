# feat/p32-utl-audio: UTL-AUD-001 (audio trimmer and normalizer)

## Done
- `apps/web/src/tools/audio/dsp.ts`: pure sample math: slice, linear fades, RMS (dBFS), sample peak, approximate true peak (4x Catmull-Rom), RMS normalization gain with a peak ceiling (-1 dB), our own 16-bit WAV writer, Opus through `MediaRecorder` (`opusMimeType`, `encodeOpus`).
- `apps/web/src/tools/audio/AudioTool.tsx` (+ `audio-tool.css`): waveform, start/end handles (range inputs), fade in/out, normalize with target, preview, export WAV or Opus (drawer mode), or "Use this" (embedded mode with `limits`: max seconds, max bytes, format `wav` or `ogg`). The UI says it is a simple RMS gain, not EBU R128.
- `apps/web/src/tools/audio/index.ts`: `audioTool` descriptor (id, i18n keys, search phrases, lazy component) for utl-shell's registry.
- Voice-message hook: `lounge/msg/VoiceRecorder.tsx` preview gains a "Trim" button that opens `AudioTool` (2 min cap, Ogg only) and replaces the clip (duration and waveform recomputed).
- Catalog fragment `i18n/messages/en/p32-utl-audio-1.ts`.
- Reuses `decodeAudioFile` and `peaks` from the existing soundboard trimmer (`settings/server/sections/soundTrim.ts`).

## Still open
- Registry entry: `tools/registry.ts` did not exist in `feat/p32-utl-shell` when I finished (no commit yet). Once it lands: `git merge feat/p32-utl-shell`, then add an entry importing `audioTool` from `tools/audio`. The documented shape was not available, so the descriptor fields are my guess; adapt to the registry's type. The `phrases` array should be moved into `corpus.json` (utl-search).
- Soundboard upload hook: not wired. The soundboard upload dialog already has the D-201 trimmer (J-06: 5 s window, WAV, 512 KiB, mono fallback). Swapping it for `AudioTool` (`limits={{maxSeconds:5,maxBytes:512*1024,format:'wav'}}`) is a one-call change in `SoundboardTab.tsx`, left alone to avoid regressing the J-06 tests and because that file is outside my row.
- Playwright case (trim a voice message before sending on Chromium): not written or run (no browser/ports allowed in this wave). Also, the voice recorder only exists where the browser records Ogg (`voiceMimeType`, Firefox); Chromium records WebM, so the Chromium case cannot pass until the server/recorder accept WebM.
- Opus export is real-time (takes as long as the audio) and was not exercised in a real browser.

## Tests run
- `vitest run src/tools/audio` (generated tone: trim lengths, clamping, RMS, fades, gain to target, peak guard, true-peak, WAV header; component test for the embedded limit), plus `src/lounge/msg`, composerGaps, messages, i18n suites; eslint, tsc, prettier on touched files. The full web suite, build and bundle budget were not run by me beyond this (see the final report).

## Changelog lines
- Audio trimmer and normalizer tool (trim, fades, RMS normalize with peak guard, WAV and Opus export); voice messages can be trimmed before sending.

## Open owner questions
- Should the voice-message path accept WebM so Chromium can record and trim voice messages? Today it is Ogg-only by server rule.
