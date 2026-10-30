# feat/p32-utl-image: UTL-IMG-001 (image and meme studio, video to GIF)

Branch `feat/p32-utl-image`, base `integration/phase32`, with `feat/p32-utl-search` (4527fc8b) and `feat/p32-utl-shell` (cb33345e) merged first.

## Done

- **Rust (`crates/sr-tools-wasm`)**
  - `gif.rs`: the GIF maker's encoder. Uses the `gif` crate (already in the lock; `default-features = false, features = ["std"]`, so no new package, only a new edge in `Cargo.lock`) with our own quantiser: an exact palette when the frames use 256 colours or fewer (lossless round trip), otherwise a NeuQuant-style self-organising map plus a few Lloyd steps. Caps enforced in Rust, not only in the UI: 15 s total delay (`GIF_TOO_LONG`), 20 MiB output (`GIF_TOO_BIG`), 2048 px edge, 256 MiB of buffered frames. Wasm API: `GifBuilder` (`addFrame`, `finish`, `frameCount`, `maxFrames`) and `gifLimits()`.
  - `palette.rs`: k-means (deterministic farthest-point seeding, no RNG, at most 40k sampled pixels, k up to 16, transparent pixels ignored); wasm `extractPalette(rgba, k)` returns JSON.
- **Facade** (`packages/wasm/src/index.{js,d.ts}`): `loadTools()` now also returns `gif` (`limits`, `create(w, h, delayCs)`) and `palette.extract(rgba, k)`.
- **Studio** (`apps/web/src/tools/studio/`)
  - `bitmap.ts`: pure RGBA operations: crop, resize (area-average then bilinear), pad, pad to aspect, rotate, flip, pixelate, redact, flatten.
  - `caption.ts`: top and bottom captions, stroked (white fill, dark outline), three fonts read from the theme tokens (`--ui-font-display|body|mono`), word wrap, auto-shrink.
  - `session.ts`: one working bitmap plus an undo history; no I/O. `canvasIo.ts`: decode, paint, burn captions, export PNG/JPEG/WebP via canvas.
  - UI: `StudioEditor` (crop, resize, pad, caption, redact, palette, export), `ImageLoader` (file, paste, drop), `StudioTool`, `RedactTool`, `PaletteTool`, `GifTool` (+ `videoFrames.ts`: seek-and-draw frames from a `<video>`), `StudioModal`, `studio.css`.
  - Registry: `image-studio`, `redact`, `palette`, `gif` appended to `tools/registry.ts` (group `media`). The corpus entries for these ids were already in utl-search's `corpus.json`.
- **Entry points**: the drawer (the four tools); the composer's attachment tray (a new "Edit image" button on still images, `lounge/msg/UploadTray.tsx`, which replaces the pending file with the edited one and recomputes its meta); the media viewer (`lounge/msg/Attachments.tsx` `Lightbox`, a new "Open in studio" button that fetches a COPY and offers "Save as a copy" as a download; nothing writes to the message).
- **Post as image API for utl-collab** (`apps/web/src/tools/studio/toImage.ts`): `toPngBlob(source, { scale?, background? })` takes a `<canvas>`, an `OffscreenCanvas` or a DOM element and returns a PNG `Blob`. A DOM node is rasterised through an SVG `foreignObject` with computed styles inlined (no library). Limits: cross-origin images inside the node taint the canvas (rejects `tainted`), web fonts only if already loaded, canvases inside the node are copied as their current bitmap. Usage for the integrator: `new File([await toPngBlob(el, { scale: 2, background: '#16161a' })], 'tier-list.png', { type: 'image/png' })`, then the composer's normal upload path. It is NOT wired into utl-collab's code; the integrator does that.
- **i18n**: `apps/web/src/i18n/messages/en/p32-utl-image-1.ts`.

## Privacy (redaction and pixelation)

Redact and pixelate overwrite pixels of the single working bitmap (`redact` sets opaque black, `pixelate` averages blocks; the block is raised to at least 4 px and a sixth of the region's short side so text cannot stay readable). Export encodes only that bitmap, through a canvas re-encode (EXIF and other metadata of the source are dropped). Nothing in the studio calls the network.
Tests: `session.test.ts` (the exported bytes, from a fake canvas whose `toBlob` writes the raw pixels, do not contain the original byte pattern after redact, after pixelate, and after crop/resize/caption then redact; `fetch`, `XMLHttpRequest`, `sendBeacon` and `WebSocket` are never touched), `editor.test.tsx` (a pointer-drag selection covers the pixels and the file handed to `onUse` lacks them). Real Chromium check: covered pixel read back as `[0,0,0,255]` and the area outside unchanged, 0 POST or /api requests during edit and export.
The undo history keeps earlier bitmaps in memory only so Undo can bring a redaction back; the Redact panel has "Remove undo history" for a user who wants that guarantee in the open tab.

## Still open / not done

- WebCodecs frame source for the GIF maker: not built; frames come from seeking a `<video>` (works everywhere, slower).
- `e2e/tests/studio.spec.ts` (paste in composer, Edit, send; Open in studio on a message) is written, parses, type-checks and lists, but was NOT run: it needs sr-server, Postgres and Garage, and the brief gave no cargo slot for a server build. Selectors in the send/Lightbox steps are unverified against a live message list. Run it with: `SR_E2E_BASE_URL=http://localhost:5175 SR_E2E_SERVER_PORT=8082 pnpm --filter e2e exec playwright test studio --project=chromium`.
- `storage-composer.spec.ts` before/after run (roadmap Risks): not run (needs Garage and a restartable server). The UploadTray change is additive (one lazy import, one button, one modal) and `p13UploadTray.test.tsx` still passes.
- GIF maker frame size is capped at 1024 px wide in the UI and 2048 in Rust; no dithering.
- Edit image is offered for PNG, JPEG, WebP and BMP only (an animated GIF would lose its motion; SVG is not a bitmap).

## Tests run

- `cargo test -j 2 -p sr-tools-wasm` (26 pass: 6 GIF incl. the 10-frame exact round trip decoded with `gif`, the 15 s cap, the 20 MiB cap by 24 incompressible 1024 px frames, the quantiser error bound, plus 5 palette tests, plus utl-search's), `cargo clippy -j 2 -p sr-tools-wasm --all-targets -D warnings`, `cargo fmt --check`.
- `wasm-pack test --node` in `crates/sr-tools-wasm`: `tests/gif_wasm.rs`, 2 pass (10 generated frames through `GifBuilder` decode with the `gif` crate to the same frames; the 151st frame at 10 cs is refused).
- `packages/wasm` vitest `test/tools-image.test.ts` (3 pass, against a local `wasm-pack build` of the tools crate).
- Web vitest `src/tools`, `src/i18n`, `p13UploadTray`, `p13sensitiveMedia`, `gifAutoplay` (all pass): 36 tests under `tools/studio` for each operation on a bitmap fixture, captions, session, editor, the tray Edit button and the Lightbox hook. `tsc --noEmit`, eslint on `src/tools` and `src/lounge/msg`, prettier on everything touched, `scripts/check-css-tokens.mjs` clean.
- `node scripts/build-web.mjs`, `scripts/check-entrance-isolation.mjs` (every public surface clean), `scripts/check-bundle-budget.mjs` (all budgets pass; the studio is lazy chunks `StudioEditor`, `StudioModal`, `GifTool`, `RedactTool`, `StudioTool`).
- Real-browser smoke in Chromium (a throwaway harness page on port 5175, removed): load PNG, redact, caption, palette (wasm), JPEG export and download; and Video to GIF from an in-page recorded WebM (1.5 s at 8 fps gave a valid 120x68 GIF89a with 11 frames).

## Decisions and things the integrator should know

- **Catalog key for hyphenated tool ids.** `catalog.test.ts` forbids hyphens in key segments, but the shell's registry rule was `tools.<id>.name`. I added `toolKeySegment(id)` to `tools/registry.ts` (utl-shell's file): `image-studio` uses `tools.imageStudio.*`; ids without a hyphen are unchanged. Other agents with hyphenated ids (for example `audio-trimmer`) must use `tools.audioTrimmer.*`.
- `loadTools` in `packages/wasm/src/index.js` gets two keys and two helper functions; expect a trivial merge with the other agents' edits there and in `index.d.ts` (`ToolsApi`).
- `TOOLS` in the registry is now `[guide, ...imageTools]`; the other agents' appends will touch the same line.
- "Open in studio" is in the Lightbox (the media viewer), not in the message context menu.
- Cargo.lock: `sr-tools-wasm` gained `gif` and (dev) `wasm-bindgen-test`, both already in the lock.

## CHANGELOG lines (Phase 32)

- Image Studio, Redact and Pixelate, Palette Extractor and Video to GIF in the utility drawer; all editing happens on the device.
- "Edit image" in the attachment tray and "Open in studio" (on a copy) in the media viewer.
- `sr-tools-wasm`: GIF encoder (15 s and 20 MiB caps) and k-means palette; `toPngBlob` renders a canvas or DOM node to PNG for "post as image".

## Open owner questions

- None blocking. Optional: should "Open in studio" also be in the message "more" menu, and should "Save as a copy" upload to a mount instead of downloading?
