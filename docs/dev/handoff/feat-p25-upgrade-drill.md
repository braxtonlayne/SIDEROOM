# Handoff: feat/p25-upgrade-drill (RDY-OPS-007, RDY-OBS-002, RDY-OPS-008)

Worktree `p25-upgrade-drill`, base `integration/phase25`. Nothing is pushed.

## Resume here

1. Push the branch and open the PR; the `upgrade-base` and `upgrade` jobs of `.github/workflows/prod-smoke.yml` run on it (the PR touches `deploy/**`). That run is the only proof of RDY-OPS-007: there is no Docker on the build machine. Expect the first run to need small fixes (an API field name in `deploy/scripts/upgrade-check.mjs`, a timing in `prod-smoke.sh`); the failing step names the stage. Read the `Container state and logs` step.
2. Tick RDY-OPS-007 in `docs/dev/roadmap-p25.md` only when that run is green and its time is under 25 minutes. RDY-OBS-002 and RDY-OPS-008 are done with their tests run (below).
3. After merging with other wave-25 branches, regenerate OpenAPI again if they changed it (`cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, then `pnpm --filter @sideroom/api-client generate`).

## Done

### RDY-OPS-008 (stale `.partial` dumps)
- `deploy/scripts/backup.sh` removes `sideroom-*.dump.partial` older than 60 minutes (`find -mmin +60`) before it starts. `sr-cli backup` does the same (`remove_stale_partials`, `crates/sr-cli/src/ops.rs`). Retention globs `*.dump`, so it never counted a `.partial`; a test now pins that.
- Tests: `scripts/backup.test.mjs` (runs the real `backup.sh` with fake `pg_dump`/`psql`: stale removed, young kept, retention ignores partials); `ops::tests::stale_partial_dumps_are_removed_and_young_ones_are_not`.
- Docs: `deploy/BACKUP.md` (Retention, the sr-cli paragraph, the comparison table); U-05's note closed in `docs/dev/answers-followups-p16.md`.

### RDY-OBS-002 (`storage.measure`, U-04)
- Migration `1350_storage_figures.sql`: table `storage_figures (bucket pk, bytes, objects, truncated, measured_at)`.
- `ObjectStorage::walk(max_objects, budget, visit)` (new trait method with a default; the `object_store` implementation streams the listing and checks the cap and clock between objects). One walk of the bucket, classified by key prefix: `objects` (everything), `library` (`library/`), `cinema` (`cinema/`, `originals/`, `transcode/`).
- Job `storage.measure`, id 133, daily 05:20 UTC, in `crates/sr-server/src/admin_jobs.rs`: cap from the new setting `ops.storage_measure_max_objects` (default 200,000, group limits), 60 s budget plus a 5 s timeout around the walk; a walk that stops early stores its running total with `truncated = true`; a failed walk keeps yesterday's rows. It sets the existing gauge `sr_storage_bytes{bucket="objects"|"library"|"cinema"}` (the health pass still sets `bucket="attachments"`).
- Admin > System: a "Stored data" card reads the same rows through `GET /admin/system` (`counts.storage_figures`); catalog fragment `p25-upgrade-drill-1.ts`.
- Tests: `storage_measure_pg_tests.rs` (Postgres, memory store: rows, gauge text, `AdminRepository::counts`, replace-on-rerun, cap and budget flag), `walk_visits_every_object_and_stops_at_the_cap` (sr-storage), `the_storage_job_id_kind_and_setting_key_are_unique` (job id 133, kind and setting key each appear once), plus the existing `keys_are_unique_and_defaults_valid` for the catalog.
- `docs/dev/load-testing.md` "Jobs at scale" has the row; U-04 ticked.

### RDY-OPS-007 (upgrade drill) - written, NOT yet proven
- `.github/workflows/prod-smoke.yml`: two new jobs appended, nothing else changed. `upgrade-base` (40 min limit) builds the previous release image in parallel with `image`; `upgrade` (25 min limit) runs the drill. Reason for the split: the Dockerfile does `COPY . .` before the cargo build, so nothing is cached across commits and a release build inside the drill would eat its budget.
- `deploy/scripts/prod-smoke.sh`: stages `base-sha`, `upgrade-prepare`, `upgrade-install`, `upgrade-seed`, `upgrade-backup`, `upgrade-apply`, `upgrade-verify {before|after-upgrade|after-rollback}`, `upgrade-rollback`; `install` now goes through `install_with <image>`; the deploy files the stack runs from are overridable (`SMOKE_DEPLOY_DIR`). `deploy/scripts/upgrade-check.mjs` writes and reads the data through the API.
- Sequence in the job: previous release's own `deploy/` unpacked from git and installed with `install.sh --yes` and the previous image; `wait`; `sr-cli seed`; a draft Cinema title (Owner) and a marker message in `#general` (alice); `upgrade-verify before`; backup with the command read out of `UPGRADE.md`; `restore-drill.sh`; new deploy files + new `SR_IMAGE` + `sr-cli upgrade --dry-run` and `--no-backup` + `up -d sr-server`; `wait`; sign-in (`smoke-admin.mjs signin`), member alice, the marker, 100 seeded messages, Library series/chapter/page count, the Cinema title through the API, and a second marker; Playwright `entrance-smoke`; rollback (stop, drop and recreate the database, `restore.sh`, migration count equals the backup's, previous files and image, start); `wait`; the same checks plus "the second marker is gone"; `entrance-smoke` again.
- Previous release = merge base of the change with its target branch (`GITHUB_BASE_REF`, else main); the commit before it when there is none or the change is on the target (push to main, weekly run).
- Tested locally against the recording fake Docker (like `install.test.mjs`): `scripts/upgrade-drill.test.mjs` proves the order and arguments of base-sha, prepare, install (non-Windows), backup, apply, rollback, and the refusals (wrong migration count, a new `.env.prod` key that needs a secret). It proves nothing about real containers.
- `install.test.mjs` still passes (all `scripts/*.test.mjs`: 52 tests); `deploy_files.rs` passes (15 tests); `release.yml` untouched.

### UPGRADE.md fixes (each one a mismatch between the page and what the drill has to do)
1. Step 2 said `deploy/scripts/backup.sh`, which needs host `pg_dump` and a reachable database. On Compose it is the `docker compose exec` form of BACKUP.md; the page now has it (the drill reads that line out of `UPGRADE.md`).
2. No step said to update the deploy files (`prod.yml`, Caddyfile, `scripts/` which the postgres container mounts) or to add new `.env.prod` keys. New step 3.
3. "Rolling back" had no commands and said to restore the dump with the generic instructions. `SR_RESTORE_FORCE=1` only drops objects the dump contains, so tables a newer migration added stay and the next upgrade stops on "already exists". The page now says: restore into a freshly created database; commands given.
4. The `sr-cli upgrade` command in step 6 now says it runs from the new image (step 4 changes `SR_IMAGE` first) and that the old server keeps running meanwhile.
5. Step 5 (web build swap): on Compose the web directory is a bind mount, so swapping the directory itself leaves Caddy on the old one; the page now says to recreate Caddy. This one is from reasoning about Docker bind mounts; the drill does not exercise it (it keeps one web directory).
6. Healthz through Caddy added to the post-upgrade checks; `BACKUP.md` "Restoring" step 3 warns about the `--force` caveat in 3.
Items 1-4 are what the code path needs; their real-container behaviour is confirmed only when the job runs.

## Not done / limits
- RDY-OPS-007 acceptance (green on the PR, under 25 minutes) needs the CI run.
- OpenAPI and the api-client types were regenerated for `SystemCounts.storage_figures` and the new schema `StorageFigure` (`gen-msw.mjs --check` is clean); done with the main checkout's `openapi-typescript` binary since this worktree has no `node_modules`.
- Web gates (typecheck, lint, vitest, build, bundle budget) were not run: the worktree has no `node_modules` and installing was outside the resource limits. Prettier was run on the touched web files with the main checkout's binary.
- The drill uses this checkout's web bundle for both versions (building the old web bundle costs wasm-pack and a pnpm build); it does not test a web-bundle upgrade.
- The binary-only rollback (previous image on the new schema, the one-version rule) is not rehearsed; a later addition could do it before the dump rollback.
- Files outside the Split row that had to change: `crates/sr-storage/src/storage.rs` (the `walk` method), `crates/sr-db` (migration, `storage_figures.rs`, `admin_repo.rs` counts), `crates/sr-admin` (`model.rs`, `settings.rs`), `crates/sr-server/src/app.rs` (one registration line), `crates/sr-server/src/lib.rs` (test module). All small and additive.

## Tests run
- `node --test "scripts/*.test.mjs"`: 52 passed (includes install.test.mjs, backup.test.mjs, upgrade-drill.test.mjs).
- `cargo test -p sr-cli --bin sr-cli stale_partial`: passed. `cargo test -p sr-storage --lib walk_`: passed. `cargo test -p sr-server --lib admin_jobs`: 5 passed.
- Postgres (`sr_p25_upgrade_drill`, dropped afterwards): `cargo test -p sr-server --lib storage_measure -- --include-ignored --test-threads=1`: 2 passed.
- `cargo test -p sr-core --test deploy_files`: 15 passed. `cargo test -p sr-admin`: 62 passed. `cargo fmt` and `cargo clippy --all-targets -D warnings` on sr-storage, sr-db, sr-admin, sr-server, sr-cli: clean.
- Workflow parsed with js-yaml.

## Decisions (conservative defaults, mark for the owner)
- The previous release for a PR is the merge base with the PR's target branch (`GITHUB_BASE_REF`), not always `main`; for a PR into `integration/phase25` that is the integration branch's fork point. The brief said "merge base with main".
- Cinema "media" in the gauge includes `originals/` and `transcode/` (the transcoder's keys) as well as `cinema/` posters; Library is `library/`. Other prefixes (attachments `att/`, avatars...) only count in `objects`.
- Daily time 05:20 UTC, default cap 200,000 objects: a bucket with more shows a lower bound until the owner raises `ops.storage_measure_max_objects`.

## Open owner questions
1. Should the upgrade drill also rehearse the binary-only rollback (previous image on the upgraded schema)? It would turn the one-version rule into a CI gate, but a PR with a legitimate contracting migration (like 0803) would then need an escape hatch. Default: not included.
2. Should `upgrade` become a required check once green twice on the default branch (as the notes for the other prod-smoke jobs say)? Owner decision in branch protection.
