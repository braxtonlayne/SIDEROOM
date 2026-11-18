# Wave 100, seek: SEC-AUD-429 (D-954), a real server file through the seek fix. NOT RUN: needs an S3 bucket

2026-11-18. Clock: start 04:53, reading done 04:57, no lock taken, no stack started, no spec written. Files: this handoff only. No app file, no support file, no other spec touched.

## Result

**Not run: needs an S3-compatible bucket (Garage, as CI has, or any other).** The local test stack (`SR_STORAGE__ENDPOINT=memory:`, the stack of the brief) cannot take a member's upload, so there is no real server file to play. Nothing was measured; the "not tested" note of SEC-AUD-429 stays true on a local stack. I wrote no spec: one that cannot run here would be committed unrun, and the plan says to take out what is not run.

## The path, with file and line (what a member's upload would be, and where it stops)

1. A member's file reaches a URL of the server in one of two ways, and both need a bucket that signs real URLs:
   - **Chat attachment** (composer "Add to message" > "Upload a File"): `POST /channels/{id}/attachments` answers a slot with `upload_url`, made by `deps.storage.presign(PresignMethod::Put, ...)` (`crates/sr-api/src/routes/messages/attachments.rs:196`); the browser then PUTs the file to that URL (`apps/web/src/lounge/msg/upload.ts:497-498`, `putFile`). Over `memory:` storage the signer is `FakeSigner` (`crates/sr-storage/src/storage.rs:250-265`), whose URL is `memory://bucket/<key>?method=...`: a browser cannot PUT to it, so the upload never lands. (Large files would use S3 multipart, which is switched off for `memory:` too: `crates/sr-server/src/app.rs:1273-1275`.) The comment at `app.rs:303-304` says it: "presigned URLs do not work". `e2e/tests/keyboard-5-attachment.spec.ts` skips for the same reason.
   - **Music library** (what the floating player normally plays): a track lives on a storage mount (BYOS) and its URL is `POST /music/tracks/{id}/presign` (`crates/sr-api/src/routes/music/tracks.rs:359-370`, `apps/web/src/music/search/api.ts:33`, `apps/web/src/music/queue/types.ts:75-91` `resolveSrc`). A presigned URL of a bucket: needs a bucket too.
2. The URL that does serve ranges from the server's own origin is `GET /cdn/{*path}` (route `crates/sr-api/src/routes/cdn/mod.rs:166`, handler `serve::serve`, `serve.rs:99`): one `bytes` range answers `206` with `Content-Range`, `Content-Length` and `Accept-Ranges: bytes` (`serve.rs:259, 300-340`); an unsatisfiable range 416; Vite proxies `/cdn` (`apps/web/vite.config.ts:191`). An attachment's `a.url` is such a link. So the server side of the question is answered by reading: range requests are supported, as a single range with 206.
3. The attachment's own `<audio controls>` (`Attachments.tsx:245`) is the browser's native control, not the floating player, so it does not exercise `engine.ts`. The floating player takes a track list only from the music library, a listening room (`lounge/rooms/join.ts:47`) or the dev handle `window.__srPlayerActions.play([...])` (`apps/web/src/player/PlayerGate.tsx:16`, dev server only), whose `PlayerTrack.src` is any URL (`player/types.ts:10`). The shortest real-file path on a stack that has a bucket is therefore: upload the WAV as a chat attachment in the composer, read the message's `audio[src]`, queue `{id, title, artist, src}` through the dev handle, open the Seek bar (`.sr-ply input[type="range"][aria-label="Seek"]`, `controls.tsx:232-250`, step 1, `min 0`, `max floor(duration)`), as `p94-seek-shots.spec.ts` does with the tone.
4. No activity-bundle or other substitute was used: those are developer uploads behind a review, served by another handler (`activities/proxy.rs`), and would not answer the question that was asked (a member's file).

## What would run it (for the commander, if a bucket is ever available)

A stack with `SR_STORAGE__ENDPOINT`, `SR_STORAGE__BUCKET`, access key and secret set to a Garage or MinIO (the CI e2e stack has Garage), not `memory:`. Then the spec of the task as written: PCM WAV made as bytes (mono, 16 bit, 8 kHz, 90 s, pitch stepping every 10 s), named `p100-seek-<random>.wav`, uploaded in the composer, queued as in point 3, Lounge Light 1024 x 768, counting `loadstart`, `emptied`, `waiting`, `seeking`, `seeked`, `playing` and recording each `/cdn/` request with its `Range` and status; seeks by pointer to about 60 s, by keyboard forwards then back to before 10 s, and to 85 s after a reload and a fresh start; shots `player-after-pointer-seek.png`, `player-after-key-seek.png`, `player-after-far-seek.png`. Nothing in the tree has to change for that; I did not build it because it cannot be run or checked here.

## The four seeks

Not measured (table intentionally empty). Passed or failed: **neither, not run**.

## Proposed text

- SEC-AUD-429 (stays closed; replace "Real server file not tested."): "A real server file was not run in wave 100: the local stack stores in memory, so a member's upload (chat attachment or music-library track) has no browser-usable URL (`memory://`), and a real run needs an S3-compatible bucket. The server's `/cdn/{*path}` handler answers a single byte range with 206 (read, not run). The fix is proved by the engine tests and the wave 94 seek spec against the mock."
- No new inventory row, no threat-model row, no changelog sentence.

## Owner questions

- Is a Garage (or MinIO) on this machine wanted so that upload-dependent record items (this one, the keyboard attachment spec) can run locally? It is a download and an install, so I did not do it.

## Checks

- `node scripts/check-doc-links.mjs` at 04:57 after the last edit: "doc links ok", exit 0.
- e2e `tsc`, prettier: not applicable (no spec, no code written).
- Lock: never taken. `heavy.sh status` at 04:56: `free` (no hold in my name).

## Cleanup

No stack, no database `sr_p100_seek_ui` created (nothing to drop), no WAV written, ports 8786 / 5786 / 9486 never used. `p100-seek-bin` deleted (`p92-base-bin` and `p80-base-bin` untouched). No stash touched.
