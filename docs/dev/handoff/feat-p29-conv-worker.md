# Handoff: feat/p29-conv-worker (BYOS-CNV-003)

Branch `feat/p29-conv-worker`, base `integration/phase29`. Migrations 1500 and 1501 (range 1500-1509). Nothing pushed.

## Resume here

Status at the last commit (see `git log`): **worker side done and tested; Postgres store done and compiled but its tests have
not run (no Postgres password file on this machine); API routes, server wiring, matrix rows, OpenAPI, media.yml stage,
TRANSCODER.md and the Admin column are the open slices.**

Next slices, in order:

1. `crates/sr-api/src/routes/storage/transcode.rs` (new) with the five routes below, `mod.rs` merge + OpenAPI merge,
   `byos.worker_for_members` in `crates/sr-admin/src/settings.rs` (boolean, group `storage`, default off).
2. `crates/sr-server/src/app.rs`: `mounts: Some(Arc::new(MountBucket::new(mount_repo, mount_sealer).with_guard(mount_guard)))`
   on the `WorkerService`, and the `TranscodeDeps` extension.
3. `route_matrix.tsv` rows (separate block), `authz_matrix.rs` seeds in their own block (body override, a mount-scoped
   `worker_token_id` per tenant), regenerate OpenAPI, api-client and mocks.
4. `.github/workflows/media.yml` one stage, `deploy/TRANSCODER.md` member section, Admin > Transcoding "Mount" column
   (`JobSummary.mount_id`) and token `scope`.
5. Fill the "Tests run" section below.

## Design (what is built)

- **Job kind `mount_transcode`** (`sr_media::worker_api::JobKind::MountTranscode`, spec `JobSpec::MountTranscode { mount_id, key, format
  (hls|mp4), preset (low|standard|high), operator_worker }`). The server has no probe of a mount object (probing would mean reading the whole
  file, which the zero-proxy rule forbids), so **the worker plans**: after it downloads the source through the presigned GET it runs
  `ffprobe` (`WorkerConfig.ffprobe`, found beside `--ffmpeg`), `sr_media::mount_job::plan_for` builds the ladder or single-MP4 plan, and the
  existing GPU/CPU pipeline runs unchanged (NVENC with libx264 fallback stays).
- **Presigned I/O, never a proxy.** `WorkerService.mounts: Option<Arc<dyn MountTransfer>>` (`sr_media::mount_job`). `MountBucket` is the
  production implementation over the mount resolver and sealer (waves 27-28): `presign_get` for the source on claim, one `presign_put` per
  output file on `init_upload` (single part, key `transcoded/<source key>/<path>`), and a `head` on `finish_upload` that must find the object
  with the announced size. The worker never holds bucket credentials. No `mounts` => mount jobs are never handed out.
- **Token scope (migration 1500).** `worker_tokens.scope` is `instance` or `mount:<id>`; `TokenScope` in `worker_api`. The claim query
  (`PgMediaRepo::claim_next`) and the in-memory store both filter: an instance token takes every ordinary job and the mount jobs whose
  `operator_worker` is true; a `mount:<id>` token takes only `mount_transcode` jobs of that mount. The service re-checks after the claim.
  An unknown scope string authenticates nobody.
- **Catalog row (migration 1501).** `storage_object_transcodes (mount_id, key, format, preset, prefix, entry, outputs, size_bytes, job_id)`
  is written when the job completes. conv-remux's `playable_key` (migration 1480) is not on this branch, so the integrator should point
  `playable_key` at `prefix || entry` (or read this table); I did not touch another agent's columns.
- **Who may queue (interpretation, see owner questions).** User mount: its owner, always, but `operator_worker` is only true when
  `byos.worker_for_members` is on; with the setting off the owner needs at least one active mount-scoped token (BYOC) or gets 409
  `WORKER_NOT_ENABLED`. Guild and platform mounts: the operator (`MANAGE_TRANSCODE`), `operator_worker` always true.
- `JobSummary.mount_id` and `WorkerToken.scope` are new fields (the Admin > Transcoding column).

## Route shapes (planned; section filled when the routes land)

See the "Done" list in the final report of this branch.

## Tests run

- `cargo test -p sr-media -j 2`: 122 passed (new: mount job claim/upload/complete, scope isolation, sandbox, wire format, planning).
- `cargo test -p sr-transcoder -j 2`: 61 passed (new: mount job MP4, HLS, unreadable file, ffprobe lookup).
- `cargo clippy -p sr-media -p sr-db -p sr-transcoder --tests -j 2 -- -D warnings`: clean.
- Postgres: `crates/sr-db/tests/media_repo.rs` gained `a_members_worker_token_claims_only_its_own_mounts_jobs` and
  `the_database_refuses_a_malformed_token_scope_and_a_mount_job_without_a_mount` (compile-checked, **not run**: no Postgres password file).
  Run: `SR_TEST_DATABASE_URL=... DATABASE_URL=... cargo test -p sr-db --test media_repo -- --include-ignored --test-threads=1 mount`.

## Open / owner questions

- Queueing with `byos.worker_for_members` off: the roadmap text says the owner may queue "when the setting is on". The BYOC variant needs
  jobs to queue regardless, so the owner may queue with their own token and the setting only decides whether the operator's workers serve
  the job. Say if the stricter reading (no queueing at all with the setting off) is wanted.
- The GPU path stays the owner's optional run (not exercised here: no ffmpeg on this machine).
