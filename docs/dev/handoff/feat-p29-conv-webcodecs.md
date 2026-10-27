# feat/p29-conv-webcodecs (BYOS-CNV-002)

Web only, no migrations (1490-1499 unused). Base integration/phase29.

## Resume here

Slice 1 done: types, decision table, its Vitest. Next, in order:
1. `webcodecs/fmp4.ts` fragmented-MP4 demuxer (moov + moof/trun parser -> `DemuxedSource`).
2. `webcodecs/transcode.ts` pipeline (decode -> scale -> encode -> mux) with an injectable `WebCodecsApi`; `webcodecs/muxers.ts` real mp4-muxer/webm-muxer adapter.
3. THIRD_PARTY_NOTICES.md (mp4-muxer, webm-muxer, MIT).
4. Playwright case (day two, after conv-remux lands `e2e/tests/convert.spec.ts`).

## Done
- `apps/web/src/convert/webcodecs/types.ts`: the contract.
- `apps/web/src/convert/decide.ts`: `decide()`, `detectCapabilities()`, ladder, wasm estimate.
- `webcodecs/audio-dom.d.ts`: TS 5.6 lib.dom lacks the audio half of WebCodecs types.

## Tests run
- `vitest run src/convert --maxWorkers=2`, tsc --noEmit, eslint, prettier on `src/convert`.
