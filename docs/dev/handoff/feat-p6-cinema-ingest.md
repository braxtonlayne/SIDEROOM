# Handoff: feat/p6-cinema-ingest (Cinema media pipeline: probe, planner, transcoder, Worker API, playlists, subtitles, retention)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p6-cinema-ingest, branch feat/p6-cinema-ingest (base integration/phase5).
Build with CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p3-library-ingest-target and CARGO_PROFILE_DEV_DEBUG=0.
Only `crates/sr-media`, `crates/sr-transcoder`, `deploy/`, docs and the inventory changed. **No migrations, no sr-server
wiring, no route in `openapi.json`** (see "For the merge step"). No ffmpeg or ffprobe exists on the authoring machine
(`which` found neither); nothing was downloaded or installed.

## Read this first: what was and was not exercised
Every ffmpeg/ffprobe interaction is verified against **canned output and fake runners only**. Nothing here has run against
real FFmpeg. The things most likely to need a fix on first contact with a real build (spec section 4.7): the HLS muxer's
placement of `init.mp4` when `-hls_segment_filename` points into a subdirectory, the `-dump_attachment` font trick (needs an
output, so it decodes one frame to `-f null -`), the NVENC option set (`-forced-idr`, `-no-scenecut`, `-strict_gop`, `-tune hq`
need a recent FFmpeg), the zscale/tonemap and libplacebo chains, and `tile` emitting a padded last sprite sheet. First real
run: `sr-transcoder --check`, one proxy job and one ladder job on a short sample, compare the tree with spec 4.3.
The Dockerfiles were written, not built (no Docker here).

## Done
Crates: `sr-media` (library, pure logic, ports) and `sr-transcoder` (library + binary).

- **CIN-ING-001 probe** (`sr-media::probe`): `parse_ffprobe_json` into `ProbeResult` (format, video/audio/subtitle/attachment
  streams, chapters). 4 MiB JSON cap, 256-stream cap, strings sanitised and clipped, numbers read leniently (ffprobe prints
  strings), dimensions bounded, attachment file names reduced to a base name, languages normalised (`und` dropped).
  HDR: HDR10, HDR10+, HLG, Dolby Vision with profile and `dv_bl_signal_compatibility_id` (profile 5 flagged as having no
  fallback), interlace from `field_order`, SAR, bit depth, cover art ignored for the primary stream. `probe_file` runs
  `ffprobe -print_format json ...` through `CommandRunner` and refuses option-like or protocol paths. Tests use canned JSON for
  H.264 SDR, HEVC HDR10, DV 8.1 and 5, HLG, HDR10+, interlaced anamorphic DVD, multi-audio with commentary, PGS/ASS/SRT/VobSub/
  mov_text subs with fonts and chapters, cover art, and hostile input.
- **CIN-ING-002/003 planner** (`sr-media::plan`): pure `plan_proxy_job` / `plan_ladder_job(ProbeResult, TranscodeSettings)`.
  Rules in spec 4.2: rungs fit the source and never exceed 1080p or upscale, SAR applied, constant fps and 2 s closed GOP
  shared by every rung, H.264 level and RFC 6381 codec string computed, audio selection per language (+ optional 5.1),
  subtitle extraction plan (image subs recorded as `image_based_ocr_later`), fonts only with ASS, sprite sheet maths, poster
  candidates, every output path precomputed, `required_outputs()` for completion checks. `JobPlan` is serialisable and travels
  in the claim response, so the worker has no planning logic of its own.
- **CIN-ING-005 transcoder** (`sr-transcoder`): `encoders` (parses `-encoders`, `-hwaccels`, `-filters`, `-version`; NVENC
  counts as available only after a one-frame test encode), `args` (all argument vectors; one decode + `split` + one output per
  rung for the ladder; tone-map after scale per rung; libx264 and h264_nvenc option sets; fMP4 HLS 6 s; audio, subtitle, font,
  sprite, poster and proxy steps; every output path joined through a strict sandbox check), `progress` (`-progress pipe:1`
  parser), `worker` (state machine, see below), `http_client` (blocking reqwest/rustls `WorkerApi` and `Transfer`), `cli`, and a
  binary with `--check`. State machine: claim, download (size verified), steps with progress, per-step retry (2 attempts), GPU
  failure detection with automatic libx264 fallback, bad-input detection (not retried), cancel (progress response, heartbeat
  list, or kill of the running ffmpeg), lease loss (silent stop), post-processing (segment index from ffmpeg's own playlists,
  Rust ASS/SRT to WebVTT, sprite map), output walk (no symlinks, size budget), upload (segments before playlists), complete or
  fail with the stderr tail and the failing command line, work dir removed in every case.
- **CIN-ING-004 Worker API** (`sr-media::worker_api`, `sr-media::http`): wire types, `JobStore` and `OutputStorage` ports,
  `WorkerService` (auth, ownership, lease, output path sandbox, upload part validation, completion checks, log clipping),
  `MemoryJobStore` / `MemoryOutputStorage`, axum `router(Arc<WorkerService>)`. Routes and proposed tables: spec 4.4 and 4.5.
  `http_client` tests drive the real router, a real blocking client, fake storage endpoints and a fake ffmpeg end to end over
  127.0.0.1 (a full ladder job and a proxy job), including "no Authorization header ever reaches storage".
- **CIN-ING-007 playlists** (`sr-media::playlist`): master, video/audio media playlists, subtitle wrapper playlists, signed
  sprite VTT, `grant()` / `needs_refresh()`, `UrlSigner` trait implemented for `sr_storage::cdn::CdnSigner` (a test verifies the
  minted URL with the real signer), parser for ffmpeg's playlists (plain file names only).
- **CIN-ING-008 subtitles** (`sr-media::subtitles`): ASS/SSA to WebVTT (styles table, bold/italic/underline overrides and
  `\r` reset, `\an` and `\pos`, `\N` / `\n` / `\h`, `\t` animation tags ignored, vector drawings skipped, comments skipped,
  commas in text, SSA v4 alignment mapping, centisecond timing, ordering), SRT to WebVTT (BOM, UTF-16, CRLF, `,`/`.`,
  tags kept or dropped, `{\an8}`, broken blocks skipped), all text escaped, cue and size caps, `sprite_vtt`.
- **CIN-ING-009 retention** (`sr-media::retention`): policy defaults (keep originals, cold after 30 days, delete 30 days after
  removal or rejection, proxy deleted 24 h after publish, stale proxies after 14 days), per-asset decisions, sweep plan with
  bytes freed, hot-tier reclaim under disk pressure. Pinned originals (downloads enabled), active jobs and pending re-transcodes
  are never touched.
- **CIN-ING-011 re-transcode planning** (`sr-media::retranscode`): asset / title / all, priorities, skip reasons, "only older
  than profile N", batch cap 500, GPU-time estimate.
- **CIN-ING-006 ops** (text only, nothing built): `deploy/docker/transcoder.Dockerfile` (CUDA, now with build-time checks for
  libx264, h264_nvenc, zscale, tonemap, work dir, `--check` entrypoint), `deploy/docker/transcoder-cpu.Dockerfile`,
  `deploy/TRANSCODER.md` (run, compose, verify, rent-a-GPU checklist, security notes, troubleshooting), link from doc 12.
- Inventory ticked: CIN-ING-001..009 and 011, each with a note saying what is and is not real. CIN-ING-010 untouched.

## Decisions
- **No `ffmpeg-next`.** The `native-ffmpeg` feature exists in both crates, off by default, empty (reserved). The default path
  spawns the CLI tools. ADR-0005 says ffmpeg-next; this keeps builds working on machines without FFmpeg dev libraries and is
  trivially testable. Worth an ADR note when the owner decides whether an in-process path is still wanted.
- **Software decode, hardware encode.** The filter graph (yadif, scale, zscale/tonemap) runs on the CPU in both modes so NVENC and
  libx264 produce the same picture and identical GOP structure. `-hwaccel cuda` decode / `scale_cuda` would be faster but
  cannot tone-map with zscale; left as a later optimisation.
- **One ffmpeg per ladder** (decode once, split, N outputs); audio, subtitles, sprites and posters are separate small runs so a
  failure retries only that step. Audio renditions are separate audio-only fMP4 playlists in an HLS audio group.
- **Plan travels with the job.** The server stores the `JobPlan`/`ProxyPlan` JSON (`transcode_jobs.spec`) and the worker follows
  it; changing profiles needs only a `PROFILE_VERSION` bump.
- **Staging keys** `transcode/<job>/<attempt>/<path>`; promoting them to the asset prefix is the catalog's job on `complete`.
- Download URL in `ClaimedJob` is a server-relative signed path (`/cdn/originals/<key>?ex=&sig=`, 8 h); the worker resolves it
  against `--server`. Presigned part URLs are absolute.
- Worker identity: one worker row per (token, name). `drain` and cancel are server-to-worker via heartbeat and progress.
- Only one test spawns a real process: `cargo --version` through `ProcessRunner` (run, output cap, streaming); no ffmpeg is run.

## For the merge step (catalog work)
1. Add the tables from spec 4.5 (migrations 0420+ were reserved for this but **none were created**), then a Postgres `JobStore`
   (`claim_next` SQL in the spec) and an `OutputStorage` over S3/Garage. `ObjectStorage::presign` only signs whole-object PUTs;
   part-level presigning (SigV4 `UploadPart`) does not exist in sr-storage yet. Until it does, set `ServiceConfig.part_size`
   above the largest output so each upload is a single presigned PUT (the contract already allows one part).
2. Mount `sr_media::http::router(service)` in sr-server (`routes/mod.rs` / `app.rs`) and add the admin pages
   (tokens, drain, queue, logs). Because the routes are not mounted yet, `packages/api-client/openapi.json` and the generated
   client were not touched. The router is plain axum (not utoipa); if the owner wants these routes in OpenAPI, annotate them then.
3. Call `plan_ladder_job` / `plan_proxy_job` after `probe_file` when an asset is uploaded or approved, enqueue with
   `JobStore::enqueue`, create rendition rows from the completion's `outputs` and `segment_index`, and use
   `playlist::{render_master, render_video_playlist, render_audio_playlist, render_subtitle_playlist, render_sprite_vtt, grant}`
   behind the CDN/playback endpoints (permission check first). Run `retention::plan_cleanup` and `retranscode::plan_retranscode`
   from a job and an admin endpoint.
4. `sr-server` should serve `/cdn/originals/<key>` for the signed original downloads (the existing CDN signer middleware fits).

## Gates (this worktree)
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo deny check`: pass.
Tests: sr-media 91 unit tests, sr-transcoder 45 (including 3 end-to-end tests over real HTTP on 127.0.0.1 with a fake ffmpeg); `cargo test --workspace` all green.

## Known gaps
- No real ffmpeg run (above). Sprite sheet count is compared to the plan; an exact-multiple duration can make ffmpeg emit one
  extra sheet (harmless, uploaded under `sprites/`).
- Graceful shutdown on SIGTERM is not implemented (no signal crate added); an in-flight job simply loses its lease and is
  requeued by the server.
- HDR10 to SDR uses `tonemap=hable`; no per-title tuning. Dolby Vision profile 5 needs libplacebo (Vulkan) and is untested.
- Colour matrix: SD sources keep their BT.601 tags rather than being converted to BT.709 (tone-mapped output is tagged BT.709).
- OCR for PGS/VobSub is out of scope; those tracks are only recorded.
- Admin "rent a GPU" checklist UI, worker token management, and CIN-ING-010 are not part of this change.
