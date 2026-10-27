# Handoff: feat/p29-conv-remux (BYOS-CNV-001)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p29-conv-remux`, base integration/phase29. Nothing pushed.

## Resume here

Slice 1 done (engine, converter, probe/plan, types, tests; commit "BYOS-CNV-001: converter core").
Next, in order:
1. i18n fragment `apps/web/src/i18n/messages/en/p29-conv-remux-1.ts` and the `ConvertStep` component + progress bar
   (`apps/web/src/convert/ConvertStep.tsx`), with Vitest.
2. Wire the step into the Cinema submit VideoPicker, Library Submissions, the bucket browser UploadPanel (and Music
   when it exists).
3. Migration 1480 (`storage_objects.playable_key`, converter columns) and shelf_repo `COALESCE(playable_key, key)`.
4. `vite build` + `scripts/check-bundle-budget.mjs`, THIRD_PARTY_NOTICES.md section, `e2e/tests/convert.spec.ts`.

## Interfaces conv-webcodecs must match

All in `apps/web/src/convert/types.ts` (conv-remux owns it; do not edit, ask in your handoff).
- `Decider = (probe, plan, caps, file) => Decision`; install with `setDecider()` from `convert/index.ts`.
- `Transcoder { id, run(file, probe, opts, engine: FfmpegEngine) => ConvertedFile }`; install with `registerTranscoder()`.
- `setCapabilities(ConvertCapabilities)`; routes are `native | remux | webcodecs | ffmpeg-transcode | worker`.
- `FfmpegEngine` gives `mountInput`, `exec(args, {onLog})`, `readFile`: demux with `-c copy -f mp4 -movflags
  frag_keyframe+empty_moov+default_base_moof` into a file and read it back.
- `engineFactory.make()` / `getConverter()` give the shared engine/converter; `decide(probe, file)` runs the table.

## Decisions

- Probe parses `ffmpeg -i` log text (`probe.ts`); no ffprobe.
- Remux output is plain MP4 with `+faststart` (best for `<video>` range seeks); `fragmented: true` gives fMP4 with sidx.
- Input is mounted with WORKERFS so the source is not copied into the wasm heap.

## Tests run

- `vitest run src/convert` (21 passed), `tsc --noEmit` clean.
