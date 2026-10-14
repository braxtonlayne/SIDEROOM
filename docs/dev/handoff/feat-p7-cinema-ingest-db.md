# Handoff: feat/p7-cinema-ingest-db (Cinema media pipeline on the server)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p7-cinema-ingest-db, branch feat/p7-cinema-ingest-db (base integration/phase6).
Built with CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p3-library-ingest-target and CARGO_PROFILE_DEV_DEBUG=0.
Spec: `docs/specs/cinema-media.md` section 11 ("as built"), written with the code.

## Read this first: what was and was not exercised
**Nothing has run against real ffmpeg, ffprobe or a real S3/Garage.** ffprobe and ffmpeg are fakes (`FakeRunner`, the
transcoder's fake ffmpeg), storage is `object_store`'s in-memory backend, and the S3 multipart signer is checked against
the AWS documentation vectors (presigned URL and GET Object examples) plus an in-process fake S3 that only checks shapes,
not signatures. First contact with a real Garage needs: create multipart upload, a worker PUT of a part, complete
(`S3Multipart`), the `ETag` header handling in the transcoder's `HttpTransfer`, and a real `ffprobe` on a sample.
`SR_MEDIA_SINGLE_PUT=1` falls back to one presigned PUT per output if part presigning misbehaves.

## Done
- **Migrations 0420..0423**: assets, renditions, audio and subtitle tracks; worker tokens, workers, jobs, uploads; Patron
  download grants, cache and requests; media GC queue. Details in spec 11.1.
- **sr-storage `s3multipart`**: `CreateMultipartUpload`, query-string SigV4 `UploadPart` URLs, `CompleteMultipartUpload`
  (handles the 200-with-`<Error>` case), `AbortMultipartUpload`; built on the existing hand-written signer (helpers made
  `pub(crate)` in `s3admin.rs`).
- **sr-media**: `JobKind::Remux` + `remux` module (plan, validation, caps), `ClaimedJob.inputs`, `output_storage`
  (`S3OutputStorage`, `SinglePutStorage`), `assets` (domain + `MediaRepository` port), `service` (`MediaService`: upload
  slot, completion, probe job, planning, approve, re-transcode, tokens, workers, queue, retention sweep, GC),
  `downloads` (`DownloadService`, entitlement and repository ports), `playback` (grant, subtitle links, playlist
  rendering). `complete` now also demands a segment index for every audio output.
- **sr-transcoder**: runs `remux` jobs (downloads the signed inputs into `in/`, `ffmpeg -c copy` over the local playlists
  into `out/remux/video.mp4`, uploads only the MP4).
- **sr-db `media_repo`**: `PgMediaRepo` implements `JobStore` (claim with `FOR UPDATE SKIP LOCKED`, progress extends the
  lease, `fail`/`reap_expired` with 3 attempts, effects of a finished job on the asset, tracks and download cache in the
  same transaction), `MediaRepository`, `DownloadRepository`, `AssetLookup`; `PgDownloadEntitlements`.
- **sr-api `cinema_media`**: 24 routes (staff upload path, transcoding admin, Patron downloads), CDN kinds `originals`
  and `hls` (signed only), `PlaylistRenderer` hook, `?dl=1`, content types for `.m4s` and `.vtt`.
- **sr-server**: Worker API mounted at `/api/v1/transcode/worker` (outside the rate limiter), `media_jobs.rs`
  (`cinema.probe`, `cinema.media.sweep` hourly, `cinema.transcode.reap` every 30 s, settings, probe queue, playlist
  renderer), setting `cinema.upload.max_gb` in the Admin catalog, Cinema kinds registered with the review service.
- **sr-cinema `submissions`**: `cinema.title` and `cinema.episodes` kinds.
- openapi.json and the api-client `schema.d.ts` regenerated (the generator was run from the main clone's
  node_modules: `node SIDEROOM_repo/packages/api-client/node_modules/openapi-typescript/bin/cli.js`, nothing installed).
- Inventory: CIN-ING-010 and CIN-DL-001 ticked; notes on 004, 009, 011 updated.

## Decisions (please review)
1. **No promotion copy.** Outputs stay at `transcode/<job>/<attempt>/` and the asset records the prefix; the `hls` CDN kind
   maps to the `transcode/` key prefix. A re-transcode swaps the prefix and queues the old one for GC.
2. **Playlists are rendered by the CDN**, because the stored ones are unsigned and relative. The hook is
   `routes::cdn::HlsDeps(Arc<dyn PlaylistRenderer>)`; without it the stored files are served as they are (useless to a player).
3. **Originals upload is multipart** (128 MiB parts) so files beyond S3's 5 GiB single-PUT limit work; the web agent must
   send parts and keep ETags.
4. **Probe on the server** (as asked): the original is streamed to a spool file. Needs disk for the largest original and
   FFmpeg on the server host. Without ffprobe the asset fails with a message naming `SR_FFPROBE`; "Probe again" retries.
5. **ffprobe path and spool directory are environment variables** (`SR_FFPROBE`, `SR_MEDIA_SPOOL`) because the Admin
   settings catalog only has integer and boolean kinds. `cinema.upload.max_gb` is a normal setting.
6. **Remux downloads read the stored rendition** through per-file signed URLs in the claim (a movie rung is a few thousand
   files, ~0.5 MB of JSON), because ffmpeg cannot fetch our relative-segment playlists from a signed CDN. Subtitles are not
   in the MP4.
7. **Patron perk source**: `cinema_download_grants` is a stand-in until doc 08 exists. The daily limit counts requests, not
   completed downloads. Download links are session-checked redirects, not shareable bearer links.
8. **Download tests use `Stub`** of `routes/cinema/tests.rs`, which I made `pub(crate)` (two lines in `cinema/mod.rs` and
   `cinema/tests.rs`): expect a trivial conflict with the wiring agent there.
9. Retention: cold-tier moves are counted, not done (no second bucket). Facts are read for the first 5000 assets only.
10. `PlaybackSource` (spec section 10 of the wiring agent) did not exist on their branch when this was written; the
    stored shape is `AssetPlayback` (spec 11.3) so an adapter is one `playback_grant` call.

## Not done / gaps
- Member-side upload and draft-edit HTTP routes for the review kinds (`/submissions/{id}/cinema/*`); only staff-uploaded
  assets can be referenced by a submission.
- No admin UI; no live test of the worker over the real router with the Postgres store (the sr-media/transcoder end-to-end
  tests use the memory store).
- No auto-approve of a title's artwork or markers in the kinds.
- Worker routes are not rate limited and not in OpenAPI.
- Proxy playback for reviewers (serving `proxy_key`) has no route; the key is stored (`video_assets.proxy_key`).
- Remux jobs for assets whose ladder was replaced while the job runs read a prefix that may be collected after the GC grace
  period (10 minutes); retry the download.

## Tests
See the final report in the task result for counts. Postgres-backed (ignored):
`cargo test -p sr-db --test media_repo -- --ignored --test-threads=1` (8) and
`cargo test -p sr-db --test cinema_submissions -- --ignored --test-threads=1` (3).
