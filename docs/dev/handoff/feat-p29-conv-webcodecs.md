# feat/p29-conv-webcodecs (BYOS-CNV-002)

Web only, no migrations (1490-1499 unused), no UI strings (so no i18n fragment), no new dependencies (mp4-muxer 5.2.1
and webm-muxer 5.0.3 were already in the tree). Base integration/phase29. Nothing pushed.

## Resume here

Everything except the Playwright case is built and unit-tested. Left for a later slice or the integrator:
1. The Chromium Playwright case (below): needs conv-remux's `e2e/tests/convert.spec.ts` to exist first, and ports
   5173/8080 belong to conv-remux this wave, so it was not written into the spec or run.
2. Wire-up in conv-remux's Converter (below), then the integrator runs web typecheck/lint/vitest/build + bundle budget.
3. `scripts/check-bundle-budget.mjs`: the muxers are `import()`ed lazily inside `webcodecs/muxers.ts`, so they become
   their own small chunks once something imports `convert/webcodecs`. Nothing imports it yet, so no chunk exists today
   and the budget was not touched (the script is conv-remux's). Check it after the join.

## What was built (all in `apps/web/src/convert/`)
- `decide.ts`: `decide(info, caps)` is the table: native > remux (mp4 then webm) > WebCodecs (mp4 with AAC, else webm
  with VP9/VP8 + Opus) > worker (with `estimateMinutes` for the "about N minutes in this tab" line). Also
  `detectCapabilities()` (the only code touching globals; absent APIs answer false), `describeDecision(engine, d)`
  (one line for logs and Playwright annotations: "gecko:worker - ..."), `pickLadderStep`, `ladderSize`,
  `estimateWasmMinutes`, `codecFamily`.
- `webcodecs/types.ts`: the contract (SourceInfo, Capabilities, ConvertPath, DemuxedSource, TranscodeOptions, WebCodecsApi).
- `webcodecs/fmp4.ts`: our moof/trun demuxer (no library) for what ffmpeg.wasm writes with `-c copy -f mp4 -movflags
  frag_keyframe+empty_moov+default_base_moof`. Reads avc1/avc3, hvc1/hev1, vp09, av01, mp4a (esds), Opus (dOps ->
  OpusHead), ac-3, ec-3, fLaC.
- `webcodecs/transcode.ts`: `transcodeWithWebCodecs()`; back-pressure on both queues, key frame every 2 s, scaling
  by OffscreenCanvas when the frame size differs from the ladder size, abort via AbortSignal, errors as
  `WebCodecsError` (`unsupported | decode | encode | mux | aborted`).
- `webcodecs/audio.ts`: 5.1/7.1 -> stereo downmix, mono, linear resampler (WebCodecs encoders do not resample).
- `webcodecs/muxers.ts`: lazy adapters over mp4-muxer (fastStart in-memory, or fragmented when streaming) and
  webm-muxer. Finding: both libraries `instanceof`-check `EncodedVideoChunk` and demand a 2-argument `onData`.
- `webcodecs/index.ts`: public surface plus `FRAGMENT_ARGS`.
- `webcodecs/audio-dom.d.ts`: TS 5.6's lib.dom lacks the audio half of the WebCodecs types; delete when TS ships them.
- `THIRD_PARTY_NOTICES.md` (+ `apps/web/public` copy, `check-notices-copy.mjs` passes): both muxers, MIT, full text.

## How conv-remux's Converter calls us
```ts
import { decide, detectCapabilities, demuxFmp4, chunked, transcodeWithWebCodecs, FRAGMENT_ARGS, WebCodecsError }
  from '@/convert/webcodecs';            // decide() itself lives in convert/decide.ts and is re-exported

const info: SourceInfo = { container, video: { codec, width, height, frameRate }, audio: { codec, sampleRate, channels },
                           durationSec, sizeBytes };   // codec strings are WebCodecs ones (avc1.640028, hvc1.1.6.L120.90, mp4a.40.2, ac-3 ...)
const plan = await decide(info, detectCapabilities());
switch (plan.path) {
  case 'native': /* play the original */ break;
  case 'remux':  /* ffmpeg.wasm -c copy into plan.container (BYOS-CNV-001) */ break;
  case 'worker': /* ffmpeg.wasm transcode with the honest estimate plan.estimateMinutes, or the CNV-003 worker */ break;
  case 'webcodecs': {
    // 1. ffmpeg.wasm: ['-i', input, '-map', '0:v:0?', '-map', '0:a:0?', ...FRAGMENT_ARGS, 'frag.mp4']
    // 2. const bytes = await ffmpeg.readFile('frag.mp4') as Uint8Array
    const source = await demuxFmp4(chunked(bytes), info.durationSec);
    const out = await transcodeWithWebCodecs({ source, plan, signal, onProgress });   // out.blob, out.mime, out.container
    // upload out.blob as the converted object; or pass output: { kind: 'stream', onData } to write to a file stream
  }
}
```
- Probe: conv-remux's probe must produce the WebCodecs codec strings above (ffmpeg.wasm `-i` log or the same
  fMP4 header via `Fmp4Demuxer.tracks`, which already yields them: that is the cheapest probe for MP4 sources).
- Only the first video and first audio track are converted. Subtitles stay conv-remux's.
- A `WebCodecsError` should fall through to the `worker` path in the UI, not be shown raw.
- `byos.require_client_remux` (wave 28) is enforced by the upload flow, not here.

## Decisions and limits
- Ladder: <=480p source -> 480p step 1.5 Mbps, <=720p -> 3 Mbps, else 1080p at 6 Mbps; never upscales; 4K is
  downscaled to 1080p. H.264 High profile level 3.0/3.1/4.0.
- The muxers can only write AAC and Opus, so MP3/FLAC/Vorbis audio that would be playable is still re-encoded on the
  WebCodecs path (the remux path keeps it as is).
- Audio is always encoded at 48 kHz, at most 2 channels (96 kbps mono, 160 kbps stereo).
- AAC encoding is Chromium/Safari in practice; Firefox lands on the WebM branch (VP9/VP8 + Opus) if the encoder exists
  there, else on the worker path. The matrix per engine is recorded by `describeDecision` in the Playwright case.
- HDR/10-bit sources are not tone-mapped; rotation metadata is not applied. Open owner questions: none.

## Playwright case to add after conv-remux lands `e2e/tests/convert.spec.ts`
Not written (spec file and ports belong to conv-remux). Plan, one `test()` appended to that spec, project chromium:
1. Fixture: `ffmpeg -f lavfi -i testsrc=duration=10:size=640x360:rate=24 -f lavfi -i sine=duration=10 -c:v libx265
   -tag:v hvc1 -c:a aac -movflags +frag_keyframe+empty_moov+default_base_moof h265.mp4` (libx265 from apt on CI).
2. `page.evaluate` imports the module through Vite (`/src/convert/webcodecs/index.ts`), builds `info` from
   `Fmp4Demuxer.tracks`, calls `decide(info, detectCapabilities())`, asserts `path === 'webcodecs'` when
   `canPlayType('video/mp4; codecs="hvc1.1.6.L93.B0"') === ''` (else records `remux`/`native`), runs
   `transcodeWithWebCodecs`, loads `URL.createObjectURL(out.blob)` into a `<video>`, awaits `canplay`, asserts
   `videoWidth === 640` and `currentTime` advances after `play()`.
3. `test.info().annotations.push({ type: 'path', description: describeDecision(caps.engine, plan) })` on all engines;
   firefox and webkit projects only record, they do not assert the WebCodecs path.

## Tests run
- `vitest run src/convert --maxWorkers=2`: 4 files, 35 tests pass (decision table with fake capabilities incl. Chromium,
  Firefox and WebKit shapes; fMP4 demuxer on a synthetic moov/moof file fed in 7-byte pieces; pipeline with fake codecs
  incl. scaling, key frames, copy, 5.1 44.1 kHz conversion, decode error, abort; the REAL mp4-muxer and webm-muxer
  fed fake chunks, memory and stream output).
- `tsc --noEmit -p apps/web`, `eslint src/convert`, `prettier --check src/convert`: clean.
- `pnpm licenses list --json --prod | node scripts/check-licenses.mjs`: ok. `check-notices-copy.mjs`: identical.
- NOT run: real WebCodecs in a browser (no Playwright allowed on 5173/8080 this wave), web build + bundle budget
  (no importer yet), full vitest suite.
