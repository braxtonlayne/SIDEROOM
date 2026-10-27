# Handoff: feat/p29-conv-remux (BYOS-CNV-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p29-conv-remux`, base integration/phase29. Nothing pushed.

## Resume here

Web side is done and green; what is left is server wiring and the real-browser proof:
1. Server use of migration 1480: the indexer (`crates/sr-db/src/index_repo.rs`) should set `storage_objects.playable_key`
   (+ `converter='remux'`, `converted_at`) on the original when it sees `converted/<stem>.mp4|webm` for it, and
   `shelf_repo.rs` should play `COALESCE(o.playable_key, o.key)` and report `web_native` true for it. Not done (Rust
   outside this agent's file list; no Postgres password file today).
2. Settings `byos.client_convert_max_mib` (default 4096) and wiring `byos.require_client_remux` into
   `ConvertStep`'s `required` prop; both are props today (`maxMiB`, `required`), the callers use the defaults.
3. Run `e2e/tests/convert.spec.ts` on CI (needs ffmpeg from apt; not run here, ports and no ffmpeg).
4. Library and Music modals: Library takes books and comics (no video), Music arrives in wave 30; `ConvertStep` is
   generic, so Music only needs `needsConversion` widened to audio containers plus a `planRemux` audio-only branch.

## Done

- `apps/web/src/convert/`: `types.ts` (shared interfaces), `probe.ts` (parses `ffmpeg -i` log), `plan.ts` (remux plan,
  subtitle plan, `converted/<original>.mp4` and sidecar names), `converter.ts` (probe / remux / extractSubtitles over an
  injected `FfmpegEngine`, progress from `time=`, cancel, size guard), `ffmpeg/engine.ts` (lazy ffmpeg.wasm, WORKERFS
  input mount), `ConvertStep.tsx` + `ConvertProgress.tsx` + `convert.css`, `webcodecsBridge.ts`.
- Remux: H.264/HEVC/AV1/VP9 into MP4 (`-c copy`, `+faststart`, `hvc1` tag), VP8 into WebM; AAC/MP3/FLAC/Opus/ALAC kept,
  AC-3/E-AC-3/DTS dropped (no usable audio means the source goes to a transcoder). `fragmented: true` gives fMP4 with sidx.
- Subtitles: every text track to `.srt` (`.ass` for ASS/SSA) named `<stem>.<lang>.srt`, uploaded to
  `converted/<dir>/`; picture subtitles are listed as skipped.
- Wired: bucket browser `UploadPanel` (original + `converted/<key>.mp4` + subtitles) and Cinema submit `VideoPicker`
  (converted file replaces the original; no sidecar upload there so the subtitle box is hidden).
- Migration 1480 (additive columns); `THIRD_PARTY_NOTICES.md` ffmpeg.wasm section (GPL-2.0 text from xz's
  `COPYING.GPLv2`; the packages ship no licence file; component list is from upstream, marked unverified);
  `scripts/check-licenses.mjs` already listed `@ffmpeg/core` by name; vite `optimizeDeps.exclude` for @ffmpeg.
- Bundle: `vite build` + `check-bundle-budget.mjs` all ok (entrance 247.7 KB). Core JS and the 32 MB `.wasm` are
  separate files in `dist/assets` (`ffmpeg-core-*.js`, `ffmpeg-core-*.wasm`), not in any entry chunk, not precached.
  No new budget entry was needed.

## Interfaces conv-webcodecs must match (as integrated)

conv-webcodecs's own API (from the commander's message) is consumed ONLY by `convert/webcodecsBridge.ts`, which finds
`./webcodecs/index.ts` with `import.meta.glob` (absent now, present at integration; the `Wc` interface in the bridge
is the one place to fix if a signature differs). Used: `decide(info, detectCapabilities())` -> `plan.path`
(`native | remux | webcodecs | worker`), `FRAGMENT_ARGS`, `demuxFmp4(chunked(bytes), durationSec)`,
`transcodeWithWebCodecs({source, plan, signal, onProgress})` -> `{blob, mime, container}`; any throw (incl.
`WebCodecsError`) becomes the worker hint. `SourceInfo` is built from our probe with representative codec strings
(`avc1.640028`, `hev1.1.6.L120.B0`, `mp4a.40.2` ...): conv-webcodecs may prefer exact strings; `Wc.onProgress` is
accepted as a number or `{fraction|progress}`. The older seam in `types.ts`/`index.ts` (`setDecider`,
`registerTranscoder`, `Decider`, `Transcoder`) is now unused by the UI and can be deleted at integration.
conv-webcodecs's `convert/decide.ts` is not used by us (its `decide` lives in the webcodecs module); `convert/index.ts`
exports a different, synchronous `decide(probe, file)` (the remux-or-worker rule): rename if confusing.
`getConverter().withInput(file, opts, (engine, inputPath) => ...)` gives raw engine access.

## Decisions / owner questions

- Probe parses the `ffmpeg -i` log (no ffprobe). Plain MP4 + faststart by default (best for `<video>` seeking).
- A 4 GiB remux holds the output in the wasm heap; a wasm32 heap tops out near 2-4 GiB, so very large files may fail
  with an ENGINE error; the guard default (4096 MiB) is the roadmap's number. Owner may want a lower default.
- The wasm tree was copied from the main repo (`packages/wasm/dist`, gitignored) instead of rebuilt.

## Tests run

- `vitest run src/convert src/storage src/cinema/__tests__/submit.test.tsx`: all passed (convert 26, upload wiring 3,
  cinema submit 15, storage 83 earlier).
- `tsc --noEmit`, eslint on touched dirs, prettier --check, `scripts/i18n-audit.mjs` (0), `check-css-tokens`,
  `check-notices-copy`, `vite build` + `check-bundle-budget.mjs`: clean. `check-licenses.mjs` was started and did not finish
  within the time box (not confirmed).
- NOT run: Playwright `convert.spec.ts` (written), real ffmpeg.wasm core in a browser, Rust/Postgres (migration only).
