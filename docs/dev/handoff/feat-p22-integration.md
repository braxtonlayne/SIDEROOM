# Handoff: integration/phase22 (wave 22 merges, integrator work, gates)

Main checkout `C:\Users\braxt\Desktop\SIDEROOM_repo`, branch `integration/phase22` (base = main `679a05be`). Local commits only: nothing pushed, no PR, nothing merged to main. Cargo target dir: `C:/Users/braxt/Desktop/SIDEROOM_wt/p22-integ-target` (built fresh in this pass).

## Resume here

**State (2026-10-24): done locally; the wave's proofs wait for CI.** The three Phase 22 branches are merged, S-01 (D-367) is built, and every local gate below passes on the merged tree. No server, Vite, cargo, Vitest or Playwright process of this pass is running. The scratch databases `sr_p22_int` and `sr_p22_ie2e` and the leftover template `sr_tpl_159_1240_13e1ca97` were dropped (nothing was connected). The `postgres` database was not touched.

**What waits on the first CI run of the pull request.** Each item stays unticked until its job is green:

| Item | Waits on | Why it runs on the PR |
|---|---|---|
| RDY-MED-003 | `media.yml`, stage `pipeline` | `pull_request` paths: the PR touches `crates/sr-transcoder/**`, `deploy/scripts/media-*`, `deploy/compose/**` and the workflow |
| RDY-MED-004 | `media.yml`, stages `retranscode`, `download`, `tv` | same run |
| RDY-MED-005 | `media.yml`, stage `resilience` | same run |
| RDY-VOX-001 | `e2e.yml` job `live`, step "Voice through TURN relay (Chromium)" | `e2e.yml` runs on every pull request |
| RDY-MED-002 | `e2e.yml` job `live`, steps "Install ffmpeg and build sr-transcoder" and "Cinema submit over Garage (Chromium)" (both added here), all three tests including the reviewer proxy player (`SR_E2E_FFMPEG=1`) | same |
| RDY-STO-001, RDY-STO-002 | `storage.yml` jobs `live-s3` (section 13 of `storage-live.mjs`) and `composer` | `storage.yml` has `pull_request` paths (`e2e/support/**`, `e2e/tests/storage-composer.spec.ts`, `apps/web/src/lounge/msg/**`, `crates/sr-server/**`, ...) and `composer` has no `if:`, so it runs on this PR. **Intentional** (coordinator, 2026-10-24): that is how its first proof lands on this PR |

**What waits on a manual dispatch:**

| Item | Dispatch |
|---|---|
| RDY-VOX-002 | `voice-soak.yml` once on the merged tree with the defaults (10 members, 10 minutes, `camera` on). Attach its log to this handoff. It runs on `ubuntu-latest` (owner, 2026-10-24: no larger runner). If it struggles, dispatch again with `camera` off; the camera step is the first thing to drop. |

**The reviewer proxy player (RDY-MED-002) now runs in the `live` job** (follow-up, coordinator 2026-10-24, inside the owner's ffmpeg approval). Before the cinema step, the job installs ffmpeg with `apt-get`, the same package `media.yml` installs, and builds `sr-transcoder` (`cargo build`, from the lockfile like every other CI build). The cinema step exports `SR_FFPROBE=$(command -v ffprobe)`; `sr-server`, started by `e2e-run.sh`, inherits it and spawns that ffprobe. The step sets `SR_E2E_FFMPEG=1`, `SR_TRANSCODER_BIN` and `SR_E2E_WORKER_LOG`. The spec's third test then:

- writes a real four-second H.264/AAC MP4 with ffmpeg (lavfi);
- mints a worker token as the Owner (`POST /admin/transcode/tokens`);
- starts `sr-transcoder --force-cpu` for the length of the test;
- waits for "Ready" (the proxy and the ladder);
- sends the submission and opens the reviewer's proxy player.

The job timeout goes from 45 to 60 minutes, and `worker-cinema.log` joins the artifact. Not run locally (no ffmpeg on this machine). Checked here: YAML parses, `bash -n` passes on every workflow `run:` block and on the compose scripts, e2e `tsc` passes, and Playwright lists all three tests. Likely first-run snags:

- the draft may need more metadata before "Send for review" accepts it;
- the worker's `--server` is `http://127.0.0.1:8080` (`SR_E2E_WORKER_SERVER` overrides it);
- the 300 s wait for "Ready" covers the proxy plus the ladder on the CPU path.

**For the owner or the coordinator:**

1. Push `integration/phase22` and open the PR when the owner agrees. Then read, in order: `Media pipeline` (`media.yml`), `E2E / live` (relay and cinema steps), and `Storage (live S3)` (`live-s3` and `composer`). The branch handoffs list the likely first-run failures: `feat-p22-ffmpeg-ci.md` (9 items) and `feat-p22-voice-storage.md` (9 items).
2. Dispatch `voice-soak.yml` once the PR is green.
3. Tick RDY-MED-002 when the `live` job's cinema step is green with all three tests.

**How to rerun the browser tests locally:** as in `feat-p21-integration.md`, with these settings:

- a scratch DB migrated with `sr-cli migrate`;
- `sr-server` with `SR_STORAGE__ENDPOINT=memory:`, `SR_SERVER__PUBLIC_URL=http://localhost:5173`, `SR_VOICE__ENABLED=true`, `SR_ACTIVITIES__ENABLED=true` and `SR_ADMIN_REQUIRE_REAUTH=0`;
- Vite with `VITE_SR_E2E=1 VITE_ACTIVITIES_ENABLED=true`, started with the local binary (`node node_modules/vite/bin/vite.js`);
- Playwright with `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1`, `SR_CLI_BIN`, `SR_E2E_SERVER_LOG` and `DATABASE_URL`.

Run the E2EE spec on a **restarted** `sr-server`. The full suite uses up the daily registration throttle of 127.0.0.1, and the E2EE spec then fails at `POST /sr/register` with a 429. A restart clears it.

## Merges (all `--no-ff`, in this order)

| Branch (head) | Conflicts and resolution |
|---|---|
| feat/p22-ffmpeg-ci (`38e7b443`) | none |
| feat/p22-voice-storage (`d15e7167`) | none |
| feat/p22-cinema-submit (`d455665a`, "docs(p22): answer D-367") | `docs/inventory/20-launch-readiness.md` only: union of the notes. The cinema side gave the RDY-MED-001 tick and the RDY-MED-002 note; the ffmpeg-ci side gave the RDY-MED-003..005 notes. The D-367 docs (decisions log "Phase 22 answers", `open-question-answers.md` "Phase 22 cinema-submit round", `answers-followups-p16.md` "Phase 22 round") merged cleanly; every section is kept |

- **Generated files**, regenerated on the merged tree from the fresh `sr-server`:
  - OpenAPI (from bash);
  - the api-client schema;
  - the MSW mocks (970 operations);
  - `gatewayDocs.generated.ts` (`node scripts/lib/gateway-docs.mjs`);
  - the route matrix (`SR_UPDATE_ROUTE_MATRIX=1`).

  All were identical to the auto-merge. The matrix keeps its PLATFORM column (971 seven-column rows). The ten new cinema rows are `authenticated`/`author`, like the Library submission rows. There are no `?` or `other` cells, and `KNOWN_CLASHES` is empty.
- **Uniqueness:**
  - Migration `1240_cinema_submission_uploads.sql` is the only new one. No number prefix repeats (1250..1269 unused).
  - No new `*_JOB_ID`, audit action ids or settings keys came with this wave.

## Integrator work

- **S-01 (D-367)**, done on the integration branch (`312b2ebd`):
  - "My uploads" opens `/cinema/submit`. "Upload" opens `/cinema/submit?new=1` with the new-submission form open. This also works when the list is already open: an effect reacts to the search change, because the route does not remount.
  - The Library's My submissions passes `kind=library.series,library.items`.
  - The Lounge inbox's "For you" decisions open `/cinema/submit/{id}` for `cinema.*` kinds. The list rows already carry `kind`, so there was no server or OpenAPI change.
  - Spec `cinema-media.md` sections 1 and 2.5 name the targets. S-01 is ticked, and the cinema-submit handoff question is marked answered.
  - Tests:
    - `cinema/__tests__/submit.test.tsx`: `?new=1`, and both menu targets through the real router;
    - `library/__tests__/submissions.test.tsx`: the kind filter;
    - `lounge/__tests__/p13Inbox.test.tsx`: the kind-prefix link.
- **(a) Workflows.** `e2e.yml` `live` gains "Cinema submit over Garage (Chromium)" right after the TURN relay step (`if: always()`, keeps `server-cinema.log` in the artifact). Checked by reading the job:
  - the `live` job runs the relay step with `coturn-relay` up. The step renders the template, runs `docker compose --profile relay up -d coturn-relay`, sets `SR_VOICE__ANNOUNCE_IPS`, `SR_VOICE__TURN__SECRET` and `SR_VOICE__TURN__URLS`, and runs `voice-relay.spec.ts` with `SR_E2E_RELAY=1` (`SR_E2E_VOICE=1` is job-level). It removes the container before the preflight step, which needs port 3478 for the dev `coturn`;
  - `e2e-stack` starts Garage, so the cinema step has real S3. `SR_E2E_LIVE=1` is job-level;
  - triggers: see "Resume here".
- **(b) Fresh `sr-server` and the local browser proofs:**
  - the fresh build has voice E2EE, which the voice-storage agent's old binary lacked;
  - the results are in the gate table.
- **(c) coturn renderer under Git Bash.** MSYS rewrote openssl's `-subj "/CN=chat.example.org"` into a Windows path. The renderer now sets `MSYS2_ARG_CONV_EXCL="/CN="` on that one call; Linux ignores the variable. It was run locally and works: the rendered conf has `external-ip`, `static-auth-secret` and `allowed-peer-ip`, and the certificate subject is `CN=chat.example.org`. Nothing else in the script is shell-specific.
- **Owner decision (2026-10-24), soak runner.** `voice-soak.yml` stays on `ubuntu-latest`. It gains a boolean `camera` input (default on), passed to the spec as `SOAK_CAMERA`; with `0` the spec skips the camera step, and the test title says so. Noted in `voice-soak.yml`, the spec's header and `lounge-voice-video.md` section 4.12.
- **Two test fixes the merged-tree Postgres run found.** Neither came from this wave:
  - `sr-moderation` `takedowns.rs`: ten business days are 13 calendar days from a Saturday and 12 from a Sunday. The test asserted 14..=16 and failed on Saturday 2026-10-24.
  - `sr-loadtest` `arguments_have_the_inventory_defaults`: it asserted that `--database-url` is required, but clap's `SR_DATABASE__URL` fallback hides that in a shell that exports it. The assertion now runs only without that variable.
- `.gitignore` gains `/media-smoke-work/`, the scratch folder of `media-smoke.mjs`. It holds a worker token, and the dry walk leaves it in the repo root.

## Gate results (merged tree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok (and again on sr-loadtest and sr-moderation after the test fixes) |
| `cargo test --workspace -j 2 --no-fail-fast` (no database) | 2929 passed, 0 failed, 532 ignored |
| Postgres suites (`--include-ignored --test-threads=1`, own DB `sr_p22_int` as both `SR_TEST_DATABASE_URL` and `DATABASE_URL`) | 3459 passed, 2 failed (the two test fixes above, rerun: 20 of 20 and 17 of 17) across 189 test binaries. All 48 `sr-db` test files completed ok, including `cinema_submissions` and `migration_versions`. The authz matrix passed 2 of 2 in 127 s with the ten cinema routes. `sr-api` `submissions::tests_pg` passed 4 of 4 |
| `route_matrix`, `openapi_names`, `deploy_files`, `migration_versions` | 6, 2, passed, passed |
| `cargo deny --offline check` | advisories, bans, licences and sources ok. **Offline:** against the locally cached advisory database; nothing fetched |
| `pnpm install --offline --frozen-lockfile`, wasm build | ok, ok |
| `pnpm -r typecheck`, `pnpm -r lint`, e2e `tsc` | ok |
| `prettier --check .` | tracked files ok; only the git-ignored e2e artefacts are flagged |
| i18n (`i18n-verify-diff main HEAD`) | MISSING 0 |
| `mocks:check` | up to date (970 operations) |
| licences, `check-notices-copy.mjs` | ok, identical |
| `secret-scan.mjs` (tracked and `--diff main`) | clean, clean |
| `node --test scripts/*.test.mjs` (threat model and secret-scan tests included) | 35 of 35 |
| `node --test deploy/scripts/media-lib.test.mjs`; `MEDIA_SMOKE_DRY=1 media-smoke.mjs` | 6 of 6; every stage walks green against the fakes |
| vitest apps/web `--maxWorkers=2` | run 1: 284 files, 2523 of 2523; run 2: 2523 of 2523 |
| package tests | api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 279, wasm 14, sdk-compat 3: all passed |
| web build and bundle budget | ok (lounge 425.8 of 600 KB, entrance 229.3 of 300 KB, admin 365.0 of 600 KB) |
| `render-stickers --check`, doc links, ops files, dependabot | ok (16 stickers, 36 metric series, 5 ecosystems) |
| YAML: workflows, the `e2e-stack` action, `dev.yml` (js-yaml) | 18 files parse |
| Playwright `tests/`, Chromium, fresh migrated DB `sr_p22_ie2e`, `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1`, memory storage | 35 passed, 5 skipped (relay, soak, storage-composer, the ffmpeg proxy test, K-22), 2 failed: both `cinema-submit.spec.ts` tests at the upload, which needs real S3 (below) |
| Playwright `e2ee/voice-e2ee.spec.ts`, Chromium | 1 of 1 passed (23 s, on a restarted server with the TURN secret set) |
| Playwright `test:static` | 19 passed |

## Proven locally, and what waits for CI

- **Voice E2EE (Chromium):** passed on the fresh server.
- **`voice-relay.spec.ts`, the non-relay parts:**
  - Run as a temporary copy with `forceRelay` removed (deleted afterwards), against `sr-server` with `SR_VOICE__TURN__SECRET` and a `turn:127.0.0.1:3478` URL, with no coturn running.
  - Proven:
    - two members join an E2EE channel, and audio arrives;
    - the server hands out the TURN URL with REST credentials (`1792868304:107538189551403009`: expiry one hour ahead, then the user id), which the spec's `^\d{9,}:` check accepts;
    - the evidence collector reads the policy (`all`), the gathered candidates (all `host`) and the selected pair (`host`).
  - The real spec skips without `SR_E2E_RELAY`.
  - Waits for CI: a relay allocation on coturn running the rendered prod template, and the selected pair being `relay`.
- **Tiny soak**, against the fresh E2EE server (`SOAK_COUNT=2 SOAK_MINUTES=1`):
  - Passed in 1.4 min. Each member ended `secure` with 2 members, with 0 dropped frames sent or received, 2 voice sockets each (one reconnect), about 100 KB of inbound audio and 750 to 800 decrypted frames in the closing window.
  - The server log has no panic, and the same `sr-server` PID answered throughout.
  - Also passed with `SOAK_CAMERA=0` (2 members, 30 s, 42 s).
  - Waits for a dispatch: 10 members for 10 minutes on the runner.
- **`cinema-submit.spec.ts`, up to where Garage is required:** it cannot finish without real S3. Against memory storage both tests did the following:
  - started a series draft in the UI (`POST /submissions` 201, the draft page);
  - read season 1, episode 1 from the file name (`POST /submissions/naming/parse` 200; the spec's season and episode assertions passed).
  - They stopped at `POST /submissions/{id}/cinema/files`, which answered 503: "cinema upload could not be presigned", because memory storage has no multipart presign. `sr-cli storage ensure-cors` also fails without a bucket.
  - Waits for CI (`live` job): everything from the part PUT onwards.
- **Not run anywhere yet:** every ffmpeg stage of `media.yml`, Garage multipart (`live-s3` section 13 and `composer`), the TURN relay, and the reviewer proxy player (now in the `live` job with ffmpeg and a worker).

## Clean-up

- Stopped by PID, only processes of this pass: `sr-server` (two runs, 32664 and 20216), the Vite dev server (13812) and `vite preview` (19680).
- Dropped `sr_p22_int`, `sr_p22_ie2e` and the leftover template `sr_tpl_159_1240_13e1ca97` (nothing connected). The `postgres` database was not touched.

## Open issues

- RDY-MED-002's proxy-player test is now in the `live` job and waits for its first run (see "Resume here").
- The `storage.yml` `composer` job runs on pull requests on purpose (coordinator, 2026-10-24), so its first proof lands on this PR.
- The e2e registration throttle makes a local full suite followed by the E2EE spec on the same server fail with a 429. CI starts a new server per step, so this is local only.
- CI has not run on this tree.
