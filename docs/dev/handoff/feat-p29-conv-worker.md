# Handoff: feat/p29-conv-worker (BYOS-CNV-003)

Branch `feat/p29-conv-worker`, base `integration/phase29`. Migrations 1500 and 1501 (range 1500-1509). Nothing pushed.

## Resume here

Status at the last commit (see `git log`): **code complete for all of BYOS-CNV-003; proofs that need Postgres, a server or a
browser toolchain have not run** (no Postgres password file, no node_modules in this worktree). Open slices:

1. Run the Postgres tests (see "Tests run") and the authz matrix: `SR_TEST_DATABASE_URL=... cargo test -p sr-server --test authz_matrix
   -- --include-ignored --test-threads=1` and `storage_scopes_hold_over_http` / `every_route_enforces_its_authz_row`. The seeds
   (`worker_token_id`, two body overrides) are in their own block in `crates/sr-server/tests/authz_matrix.rs`; if a route answers
   unexpectedly there, fix the route or the seed, never add to `WRITE_OBSERVED`.
2. Regenerate OpenAPI, the api-client and the MSW mocks (the integrator does this at the end of the wave; commands in the brief).
3. Web: `pnpm install --offline --frozen-lockfile`, then vitest `src/admin src/i18n` (--maxWorkers=2), typecheck, prettier for the
   Bucket column in Admin > Transcoding (`apps/web/src/admin/cinema/TranscodingSection.tsx`, fragment `p29-conv-worker-1.ts`).
4. `media.yml` has not run (CI runs it): the new step is "Mount conversion smoke", stage `mount` of `deploy/scripts/media-smoke.mjs`.
5. Tick `docs/inventory/21-feature-phase.md` BYOS-CNV-003 only after 1 and 4 are green.

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

## Route shapes

All session-only, all under `/storage/mounts/{mount_id}`, AUTHZ class `mount_owner` (404 for a mount the caller cannot reach, 403
for one they reach but may not run conversions in; an operator with `MANAGE_TRANSCODE` is admitted on a mount that is not a member's).

| Route | Body | Answer |
|---|---|---|
| `POST /storage/mounts/{mount_id}/transcode` | `{ key, format: "hls"|"mp4", preset?: "low"|"standard"|"high" }` | 202 `{ job_id, operator_worker, output_prefix }`; 400 key not a video; 404 object missing; 409 `WORKER_NOT_ENABLED` |
| `GET /storage/mounts/{mount_id}/transcode` | none | 200 `JobSummary[]` (latest 50 of the mount) |
| `POST /storage/mounts/{mount_id}/worker-tokens` | `{ name }` | 201 `{ token, secret }` (secret once); 409 `TOO_MANY_TOKENS` at 5 live |
| `GET /storage/mounts/{mount_id}/worker-tokens` | none | 200 `WorkerToken[]` (scope `mount:<id>`, never the secret) |
| `DELETE /storage/mounts/{mount_id}/worker-tokens/{worker_token_id}` | none | 204; 404 for a token of another mount |

The worker-facing routes (`/api/v1/transcode/worker/*`, bearer worker token) are unchanged in shape: a `mount_transcode` claim returns
`source.download_url` as a presigned GET of the bucket, and `init_upload` returns one presigned PUT part per file. Setting added:
`byos.worker_for_members` (boolean, group storage, default off).

Files touched outside the Split row (small, mechanical): `crates/sr-admin/src/settings.rs` (one const, one def),
`crates/sr-api/src/routes/storage/{mod,objects,shelves}.rs` (module line, two `pub(super)`), `crates/sr-server/src/app.rs` (wiring),
`crates/sr-server/tests/authz_matrix.rs` (own block), `deploy/scripts/media-smoke.mjs` (the stage lives there, like every other
media stage).

## Tests run (2026-10-27, one cargo command at a time, -j 2)

- `cargo test -p sr-api --test route_matrix`: 6 passed (five new rows, class `mount_owner`; `authz_agrees_with_handler_extractors` green).
- `cargo test -p sr-api --lib --test openapi_names -- storage cinema_media openapi`: 52 + 1 passed.
- `cargo test -p sr-admin --lib settings`: 9 passed (`byos.worker_for_members`, default off).
- `cargo clippy -p sr-api -p sr-admin -p sr-server -p sr-media -p sr-db -p sr-transcoder --tests -- -D warnings` and `cargo fmt --all --check`: clean
  (this compiles the new authz_matrix seeds, which have **not run**).
- Web: vitest `src/admin/__tests__/cinema.test.tsx src/i18n` 70 passed (+ the new Bucket column test), `tsc --noEmit`, eslint on the touched
  files and prettier: clean. OpenAPI, api-client types and MSW handlers regenerated (`mocks:check` green).
- Not run: Postgres suites (no password file), `authz_matrix` / `immutable_rules` (Postgres), `media.yml` (CI), a real GPU/ffmpeg run.

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
