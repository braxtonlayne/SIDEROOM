# The media pipeline test (RDY-MED-003, 004, 005)

Status: **written, not yet run.** The workflow, the driver and the argument pins exist; nothing has run against a real ffmpeg yet, because the authoring machine has none. The first green `media.yml` run on the integration pull request is the proof (see "What waits for CI").

`.github/workflows/media.yml` runs the whole media path with a real ffmpeg: a member-visible upload, the server's probe, the review proxy, the planner, a real `sr-transcoder` worker (CPU path) over the Worker API, playback, the slate, Live TV, a re-transcode, the Patron download, and three resilience scenarios. It runs weekly (Wednesday 03:29 UTC), on dispatch, and on pull requests that touch `sr-transcoder`, `sr-media`, `sr-tv`, `sr-cli`'s `tv.rs`, `deploy/compose/**`, `deploy/scripts/media-*` or the workflow itself.

ffmpeg is installed on the runner with `apt-get install ffmpeg` (owner-approved download, wave 22). The runner is pinned to `ubuntu-24.04` (ffmpeg 6.1): the transcoder uses `-fps_mode`, which needs ffmpeg 5.1 or newer.

## What the job does

The stage logic is `deploy/scripts/media-smoke.mjs` (plain Node, no dependencies); the workflow only builds, starts the stack and calls it.

| Step | What runs |
|---|---|
| Self-checks | `node --test deploy/scripts/media-lib.test.mjs` and a dry walk of the driver against fakes (no ffmpeg needed), so a bug in the driver fails in seconds |
| Stack | Postgres and Garage from `deploy/compose/dev.yml`, `sr-cli migrate`, `bootstrap` (Owner `owner`), `sr-server` on 127.0.0.1:8092 (same start as `storage.yml`) |
| Unit pins | `cargo test -p sr-transcoder`, including `ci_pins.rs` |
| `sr-transcoder --check --force-cpu` | prints what the worker sees in this ffmpeg |
| Driver | the five stages below |

### Stages (`node deploy/scripts/media-smoke.mjs [stage ...]`)

1. **pipeline** (RDY-MED-003). Generates a 20 s Matroska with `lavfi`: `testsrc2` 1280x720 at 25 fps, two sine tracks tagged `eng` and `spa`, one SubRip subtitle with three cues. Creates a series, a season and an episode, uploads the file through `/admin/cinema/assets` (multipart to Garage), and waits for the probe, the review proxy and the ladder job. Asserts:
   - the probe summary (container, 1280x720, h264, languages `eng,spa`, one subtitle, 20 s);
   - the proxy job succeeded and the asset has a proxy; the ladder job succeeded on attempt 1 on `libx264` with outputs;
   - the master playlist: variants 720p and 480p only, codecs and bandwidth present, two audio renditions (`eng`, `spa`, one default), one subtitle rendition;
   - the WebVTT has the three cues; the sprite VTT has two cues with `#xywh=x,y,160,90`; the sheet is a JPEG of 1600x900 (ffprobe);
   - for the 720p video rendition and the default audio rendition: the media playlist is VOD with an init segment, four segments of at most 6 s, **every segment fetched and decoded with ffprobe** (init plus segment), the durations add up to 20 s, and the joined bytes are one 20 s stream.
2. **retranscode** (RDY-MED-004). `POST /admin/transcode/retranscode` for the asset; waits for the new ladder job; asserts two succeeded ladder jobs, `libx264`, outputs, still playable, and runs the playback checks of stage 1 again.
3. **download** (RDY-MED-004). Before the grant the picker answers 403. Grants the Owner a permanent Patron entitlement (`POST /admin/donations/entitlements`; falls back to a SQL insert when that route refuses), lists renditions, starts a download, waits for the remux job, follows `/cinema/downloads/{id}/file` (307, `dl=1`), then checks the file response: 200, `video/mp4`, `Content-Disposition: attachment` with a `.mp4` name, `nosniff`, `Content-Length`, `Accept-Ranges`, and a `Range` request answers 206 with `Content-Range`. The file has `moov` before `mdat` (faststart), H.264 video, two AAC tracks and the episode's length (ffprobe). The remux job is recorded as succeeded and the daily allowance went down.
4. **tv** (RDY-MED-004). `sr-cli tv slate --seconds 12` (real ffmpeg, real transcode by the worker, registers `tv.slate_asset`); `/admin/tv/slate` reports it playable; a manual channel with `filler_policy: slate` gets the episode placed 45 s ahead. The driver samples `/tv/channels/{id}/tune` and the 720 live video playlist every 2 s from before the programme to 30 s after it, and fetches and decodes the segment on air of each kind. It asserts the channel went `off_air` with slate, `live`, `off_air` with slate; the kinds in number order are slate, episode, slate; and the **numbering rule (D-267, D-285)** over all samples (`checkLiveNumbering` in `media-lib.mjs`):
   - one number always names the same object, whatever else is in the window;
   - the newest number and the discontinuity counter never go back;
   - every number from the first to the last seen was listed (no jump across either boundary);
   - wherever the kind changes between neighbouring numbers, a discontinuity tag introduces the new one;
   - the audio playlist is within one number of the video playlist.
5. **resilience** (RDY-MED-005). Kills all workers first.
   - *Corrupt source*: uploads a real 4 s clip, waits until both jobs are queued, replaces the stored original with random bytes of the **same size** (SigV4 PUT to Garage; the probe already ran in the server), then starts a worker. Asserts both jobs failed on attempt 1 (a bad file is not retried), the error says "could not be decoded", the log tail has the `$ ffmpeg` command line and `Invalid data found`, the asset is `failed` with a reason, and `/admin/transcode/queue` counts the failures. The same job detail is what Admin > Transcoding shows.
   - *Two workers*: starts `ci-a` and `ci-b` (concurrency 1 each), uploads four clips at once (8 jobs), asserts all 8 succeeded on attempt 1, each ran on exactly one of the two workers, and a worker never ran two jobs at the same time (start and finish times). It logs, but does not fail, when one fast worker took every job.
   - *Killed worker*: uploads a 120 s source (`MEDIA_SMOKE_LONG_SECS`), waits until the ladder job is encoding (progress of 8% or more), `SIGKILL`s the worker's process group (ffmpeg included), and waits for the lease (120 s, extended by every progress report) to expire and the reaper (every 30 s) to requeue the job. Asserts the job returned to `queued` only after at least 45 s, still on attempt 1 with no worker; starts a new worker and asserts the retry ran on it as attempt 2 and the asset became playable.

## Run it locally

Needs ffmpeg and ffprobe on `PATH` (6.x), Node 20 or newer, the built binaries, Postgres, Garage (or any S3-compatible store with the dev bucket) and a running `sr-server` whose Owner you know. The simplest way is to copy the "Start Postgres and Garage" to "start sr-server" steps of the workflow:

```sh
docker compose -f deploy/compose/dev.yml up -d --wait postgres && docker compose -f deploy/compose/dev.yml up -d garage
./deploy/compose/garage/bootstrap.sh
cargo build -p sr-server -p sr-cli -p sr-transcoder
# a scratch database, then migrate, bootstrap and start the server exactly as media.yml does
export SR_OWNER_PASSWORD=...               # the Owner you bootstrapped (never commit it)
export SR_TEST_S3_ENDPOINT=http://127.0.0.1:3900 SR_TEST_S3_BUCKET=... SR_TEST_S3_ACCESS_KEY=... SR_TEST_S3_SECRET_KEY=...
export SR_TEST_DATABASE_URL=postgres://...   # only for the Patron-grant fallback
export SR_DATABASE__URL=$SR_TEST_DATABASE_URL SR_STORAGE__ENDPOINT=... SR_STORAGE__BUCKET=...   # for `sr-cli tv slate`
node deploy/scripts/media-smoke.mjs                 # all stages
node deploy/scripts/media-smoke.mjs pipeline        # one stage (retranscode, download and tv need pipeline first)
```

Scratch files, the worker token (mode 0600) and the worker logs go to `MEDIA_SMOKE_DIR` (default `./media-smoke-work`). The workers are stopped when the driver exits. The `tv` stage replaces the instance's slate; run it on a scratch instance. Do not point the driver at a production server: it creates a title, a channel on number 901, a Patron grant and several assets.

### Dry walk (no ffmpeg, no server)

```sh
MEDIA_SMOKE_DRY=1 node deploy/scripts/media-smoke.mjs
node --test deploy/scripts/media-lib.test.mjs
```

`MEDIA_SMOKE_DRY=1` starts `media-fakes.mjs`: an in-memory stand-in for the endpoints the driver calls plus fake `ffmpeg`, `ffprobe`, `psql`, `sr-cli` and worker processes. It proves the driver's own logic (stage order, polling, playlist parsing, the numbering check, the kill and retry bookkeeping). It proves nothing about the real pipeline: the fake answers are what a correct server and ffmpeg are expected to say.

## Unit pins (no ffmpeg)

`crates/sr-transcoder/src/ci_pins.rs` pins, for the exact source the driver generates (its ffprobe JSON is in the file): the ladder plan (720p and 480p, 50-frame GOP, 6 s segments, two audio directories, one subtitle, one sprite sheet), the step list, the exact `-filter_complex`, the second audio track's `-map 0:a:1`, the SubRip extraction, the sprite and proxy arguments, the remux argument list, and that a typical ffmpeg 6 VOD fMP4 playlist parses into the stored segment index. When the real ffmpeg disagrees with a pin, fix the code and the pin together.

## What waits for CI

Everything above that needs ffmpeg, a server or Garage. Locally run: the YAML parses, `node --check` and `node --test` on the scripts, the dry walk, `cargo test` and clippy on `sr-transcoder`. See `docs/dev/handoff/feat-p22-ffmpeg-ci.md` for the likely first-run failures.
