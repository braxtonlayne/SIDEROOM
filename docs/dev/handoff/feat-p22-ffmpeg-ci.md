# Handoff: feat/p22-ffmpeg-ci (RDY-MED-003, 004, 005)

Branch `feat/p22-ffmpeg-ci` (worktree `p22-ffmpeg-ci`), base `integration/phase22`. Nothing pushed or merged. No migrations used (1250 to 1259 untouched). No dependencies added (the driver is plain Node, the workflow uses the same actions as `storage.yml`).

**Status of the three items: written, not yet run.** None of them is ticked in `docs/inventory/20-launch-readiness.md` (a note was added to each line). They tick on the first green `media.yml` run, which needs the owner's push (the integration pull request triggers it through its `pull_request` paths).

## Resume here

1. Push the integration branch (owner's word) and open the pull request; `media.yml` runs because the pull request touches `crates/sr-transcoder/**`, `deploy/scripts/media-*` and the workflow. Or dispatch it by hand.
2. Read the first run top down. Each driver line is `PASS`/`FAIL name -> detail`; on failure the job prints the tails of the worker logs and uploads `media-smoke-logs` (server log, worker logs).
3. Fix what it shows, keeping the rule "every bug it exposes gets a unit test that reproduces it without ffmpeg where possible": add to `crates/sr-transcoder/src/ci_pins.rs` (arguments, plan, playlist shapes) or `worker/tests.rs`.
4. Tick RDY-MED-003 on the first green run of stages `pipeline`; RDY-MED-004 for `retranscode`, `download` and `tv`; RDY-MED-005 for `resilience`. Stages can be run alone: `node deploy/scripts/media-smoke.mjs pipeline`.
5. Optional: once ffmpeg is on the owner's machine (human action 5, D-269), `docs/dev/media-pipeline-test.md` says how to run all of it locally.

## What was done

| File | What |
|---|---|
| `.github/workflows/media.yml` (new) | `ubuntu-24.04` (ffmpeg 6.1), weekly (Wednesday 03:29 UTC), dispatch, and `pull_request` on `sr-transcoder`, `sr-media`, `sr-tv`, `sr-cli/src/tv.rs`, `deploy/compose/**`, `deploy/scripts/media-*`, the workflow. `apt-get install ffmpeg`, Postgres and Garage from the dev compose, build `sr-server`, `sr-cli`, `sr-transcoder`, `cargo test -p sr-transcoder`, `sr-transcoder --check --force-cpu`, scratch database `sr_media`, bootstrap the Owner, start `sr-server`, run the driver; log artifacts on failure. The `e2e-stack` composite action was **not** reused: it also installs pnpm, wasm-pack and the WASM bundle, which this job does not need; the compose and server steps are copied from `storage.yml` instead |
| `deploy/scripts/media-smoke.mjs` (new) | The stage driver: `pipeline`, `retranscode`, `download`, `tv`, `resilience` (details in `docs/dev/media-pipeline-test.md`). Starts and kills real `sr-transcoder` processes itself (own process group, so a kill takes ffmpeg too) |
| `deploy/scripts/media-lib.mjs`, `media-lib.test.mjs` (new) | HLS parsing, the Live TV numbering check, MP4 box order, WebVTT; `node --test`, including a test that the workflow only names files and stages that exist |
| `deploy/scripts/media-fakes.mjs` (new) | The dry walk (`MEDIA_SMOKE_DRY=1`): in-memory server and fake ffmpeg, ffprobe, psql, sr-cli and workers. Also runs in the workflow, so a driver bug fails in seconds |
| `crates/sr-transcoder/src/ci_pins.rs` (new, test only) | Pins for the generated source: ladder plan, step list, exact filter graph and argument tails, second-audio `-map 0:a:1`, SubRip extraction, sprite, proxy, remux arguments, a real-shaped ffmpeg VOD playlist parsing into the segment index, and that optional ffprobe fields do not change the plan |
| `docs/dev/media-pipeline-test.md` (new), `deploy/TRANSCODER.md` | How it works and how to run it; TRANSCODER.md status corrected (the admin endpoints exist; the worker is covered by CI once it has run) and the lease/retry timing documented |

Code review of `args.rs`, `worker/mod.rs`, `encoders.rs`, `progress.rs`, the HTTP client, the planner and the slate command found **no bug I could reproduce without ffmpeg**: the code is already covered by exact-argument tests, ffmpeg's `out_time_ms` microsecond quirk is handled, the bad-input classification and the NVENC fallback are tested. So no production code changed; the first real run is expected to find the rest.

## Run locally (done on this machine)

- `cargo test -j 2 -p sr-transcoder`: 56 passed (6 new in `ci_pins`).
- `cargo clippy -j 2 -p sr-transcoder --all-targets -- -D warnings`: clean. `cargo fmt -p sr-transcoder --check`: clean.
- `node --test deploy/scripts/media-lib.test.mjs`: 6 passed. `node --check` on all four scripts. Prettier (the main checkout's) applied to `deploy/scripts/media-*.mjs`.
- `MEDIA_SMOKE_DRY=1 node deploy/scripts/media-smoke.mjs`: all stages walk green against the fakes.
- `media.yml` parses (js-yaml); `scripts/secret-scan.mjs`, `scripts/check-ops-files.mjs`, `scripts/check-doc-links.mjs` clean.

## Not run (waits for CI)

Everything that needs ffmpeg, a server, Postgres or Garage: every assertion about what the real ffmpeg writes, the worker over the real Worker API, playback, the slate, Live TV, the Patron download and the three resilience scenarios.

## Most likely first-run failures

1. **Live TV numbering across the slate boundary** (`tv`). D-267 says a programme "chains" from the slate segment on air at its start, but the bases are assigned when the block is first viewed; if that happens after the slate was already being viewed, the check "every number from first to last was listed" or "one number names one object" will fail. This would be a real `sr-tv` bug (my file ownership allows fixing it with a test).
2. **Fragmented-MP4 durations and ffprobe on `init + segment`** (`pipeline`). The driver falls back to packet spans when the container reports no duration, but the tolerances (1.5 s total, 4 segments of at most 6.5 s, sprite sheet 1600x900) are guesses until seen.
3. **ffmpeg HLS muxer details**: the init file's location with an absolute `-hls_segment_filename`, `tile` emitting the last partial sheet, `-fps_mode vfr` for the sprite step. Symptom: the ladder job fails with "was not produced" or "expected N sprite sheets".
4. **Remux (`download`)**: ffmpeg reading local `index.m3u8` playlists with `.m4s` segments (extension allow-list, protocol whitelist). Symptom: the remux job fails; the log tail has ffmpeg's message. Candidate fix: `-allowed_extensions ALL` before `-i` in `remux_steps`, with a pin.
5. **Patron grant**: `POST /admin/donations/entitlements` may want more than a reason; the driver falls back to a SQL insert, which needs `psql` and `SR_TEST_DATABASE_URL` (both present in the job).
6. **Assumed response shapes**: `Block`, `Tune`, `SlateStatus`, `JobSummary.progress.percent`, `WorkerView.capabilities.nvenc`, `DownloadJob`. They were read from the Rust types, not from a live server.
7. **Kill scenario length**: the lease is 120 s and the reaper runs every 30 s (not configurable), so that scenario waits 2 to 2.5 minutes; the ladder of the 120 s source must still be running when the driver sees 8 % progress (it fails with a clear message if the ladder finished first; raise `MEDIA_SMOKE_LONG_SECS`).
8. **Corrupt scenario** assumes the probe only reads the original once, in the server, before the worker exists; replacing the stored bytes of the same size then makes the worker's ffmpeg say "Invalid data found". If the server probes again before the worker claims, the asset fails at the probe instead.
9. Job time: the full run is expected at 20 to 30 minutes (timeout 60). The CPU-only encodes of the 120 s source and the slate dominate.

## Decisions

- Runner pinned to `ubuntu-24.04` rather than `ubuntu-latest`: the argument builder needs ffmpeg 5.1 or newer.
- The worker runs with `--force-cpu` so the job is the CPU path even if a runner ever had NVENC.
- The driver spawns and kills the workers itself, so one script owns the scenarios; the workflow stays a thin shell around it.
- The weekly schedule (Wednesday) is staggered from `prod-smoke` (Tuesday), `backup-drill` and the others.

## Open owner questions

None new. Still open from the roadmap: a GPU rental to run NVENC once (optional); ffmpeg on a development machine (optional, human action 5).
