# Roadmap after the inventory: waves 20 to 24 (D-352)

Written 2026-10-23 by the owner's delegate, on `docs/p20-roadmap` (main after phase 19). The owner delegated the product decision of what to build next; this file is that decision and its plan. It records the decision as D-352 in [01-decisions-log.md](../01-decisions-log.md). The new work items live in [inventory/20-launch-readiness.md](../inventory/20-launch-readiness.md) with the prefix `RDY`.

A small polish wave is already running on another branch (age buttons on the Library series and Cinema title pages, the search-index age refresh, a Reports count tile in the Review hub, the "move a message" decision). Nothing below repeats it.

## 1. Where the product stands

**Built.** Every inventory file from 01 to 19 is ticked except three items, and those three are closed or waiting on a run rather than on code: `DON-PAY-009` (won't build in CI, D-257), `XC-CI-005` (won't build, D-261) and `DEV-SDK-003` (built; ticked when the first scheduled `sdk-compat` run is green). Phase 7 moderation is built through step 5 (cases, reports, holds, appeals, the takedown register, the data-request queue, dashboards, suspicion review, age tools). The merged tree at phase 19 passes 2,824 Rust tests, 516 Postgres-backed tests, 2,489 web unit tests, the three-browser Playwright suite in CI and the Chromium live suite locally.

**Closed as won't-build (stays closed).** Server preview/lurk mode (D-251), super reactions (D-231), user-installed apps (D-305), avatar decorations outside reviewed packages (D-290), AVIF and pdfium image mode (P13-FND-2, D-217), `rtc_region` (one node), the Garage per-test helper and PR preview deployments (D-261), regtest payments in CI (D-257), teams, role connections and SKUs in the developer platform, the Lottie sticker kind, free CSS in themes, the Board, bounties.

**Blocked on the owner (not on code).**

| Item | What only the owner can do |
|---|---|
| The lawyer list: D-287, D-293 to D-295, D-297/D-309, D-311, D-314, D-336, D-341, D-343, D-347 | Hand the questions to a lawyer; then set `accounts.minimum_age`, `compliance.operator_country`, `compliance.operator_contact`, publish the notice address; say whether N-35 (mail the templated takedown reply) and D-336 (h) (mandatory external reporting) are wanted |
| electrs and a Bitcoin node (D-288, D-312) | Pin the image digest, run or point at a non-pruned node, or keep the mempool.space interim |
| Discord redirect URI (D-292) | Register `<public_url>/api/v1/connections/discord/callback` on the existing application |
| Production host | Provision, DNS, first real install from `prod.yml`, `/srv/sideroom/ingest`, VAPID keys, Storage Box, branch protection, the first green runs of the workflows that have never run on GitHub |

**What the inventory did not measure.** The inventory counts components. The phase exit gates in [13-roadmap-and-phases.md](../13-roadmap-and-phases.md) are proofs, and most of them have never been run for real:

- `prod.yml`, `install.sh`, the systemd units, `Caddyfile.prod` and the coturn template were written and linted but never run (no Docker on the agent machines).
- ffmpeg has never run anywhere in this project: probe, review proxy, the transcode planner, `sr-transcoder`, re-transcode and `sr-cli tv slate` are tested on canned output. The phase 5 gate (an episode uploaded in the browser, transcoded, played, scheduled on Live TV, watched in a party) has not happened.
- The member video upload and draft-edit routes (`/submissions/{id}/cinema/*`) are not built; only staff-uploaded assets reach the Cinema. The vision says members contribute media through review.
- Chat multipart upload against real S3 and the composer reattach over the network are unverified (local storage is `memory:`, which has no multipart).
- The full load target (2,000 sessions, 200 msg/s, REST p95 under 100 ms) was missed on a shared laptop with debug builds; the release-build CI run has not been dispatched.
- XC-SEC-005 is a heuristic route matrix with a 401 test per route, not a proof of each permission bit per handler.
- The manual screen-reader pass has never been done; browser axe covers public routes only; roving tabindex stops at the server rail and channel list.
- The threat model describes the system "as built through phase 4" and has open rows (secret scanner, per-route authz proof, SVG, load).
- The README says "Specification phase. No application code exists yet." Doc 12 section 5 says `sr-cli backup` and `restore` are not written; they are.
- Safari voice E2EE and the installed iOS PWA have never been tried on a real device. `THIRD_PARTY_NOTICES.md` carries upstream placeholders (`<year>`) in the FreeType, NTP and Zlib texts.

So the next waves build proofs and the few product gaps the proofs expose, in the order a launch needs them: install, security, media and voice, people, scale.

## 2. The waves in order

| Wave | Goal | Agents | Migrations |
|---|---|---|---|
| 20 | First real install and the owner's packet | ops-smoke, preflight, owner-packet | 1180 to 1209 |
| 21 | Security proof before strangers arrive | authz-matrix, threat-refresh, sessions-uploads | 1210 to 1239 |
| 22 | Media and voice for real (the exit gates that never ran) | cinema-submit, ffmpeg-ci, voice-storage | 1240 to 1269 |
| 23 | People readiness: accessibility, first run, the two handbooks | a11y, first-run, operator-handbook | 1270 to 1299 |
| 24 | Scale proof, Discord parity remainders, developer platform remainders (may follow launch) | scale, parity, devplat | 1300 to 1329 |

Waves 20 to 23 are what must be true before real people use the instance. Wave 24 can run after the first invites go out.

### Rules every wave follows

- **Worktrees:** `C:\Users\braxt\Desktop\SIDEROOM_wt\p<wave>-<agent>` on branch `feat/p<wave>-<agent>`, own `CARGO_TARGET_DIR` `p<wave>-<agent>-target`, own scratch database `sr_p<wave>_<agent>`, own ports. One cargo build at a time per agent (`-j 2`). Only one agent per wave may hold ports 8080 and 5173 (named below); the others run servers on other ports or not at all.
- **Migrations:** each agent gets ten numbers, listed below; unused numbers stay unused. Never edit a merged migration. Job ids, audit action ids and settings keys are checked for duplicates by the integrator as in phase 19.
- **Shared files:** `docs/01-decisions-log.md`, `docs/open-question-answers.md`, `docs/dev/answers-followups-p16.md` and `docs/inventory/20-launch-readiness.md` are append-only per agent (own section, own ticks). Generated files (OpenAPI, the api-client schema, the MSW mocks, `route_matrix.tsv`, `gatewayDocs.generated.ts`) are regenerated by the integrator on the merged tree; agents regenerate them on their own branch but do not hand-resolve conflicts in them. i18n strings go in a new fragment per agent (`p<wave>-<agent>-<n>.ts`).
- **Handoff:** `docs/dev/handoff/feat-p<wave>-<agent>.md` with a "Resume here", what was run and what was not, and open questions. Integration on `integration/phase<wave>` with the full gate list of `feat-p19-integration.md`. Nothing is pushed, merged or downloaded without the owner's word.
- **Downloads:** none are assumed. Each one is flagged in "Needs the owner" and the work is written so it degrades to a documented manual step when the download is refused.
- **Proof over claims:** an item is done when its proof runs (a CI job, a Postgres or browser test, or a recorded manual run with the command and the output in the handoff). "Written but never run" does not tick.

## 3. Wave 20: first real install and the owner's packet

**Goal.** The production stack boots from the shipped files on a machine nobody prepared by hand, an operator can tell before the first invite whether the box is ready, and the owner holds one packet of everything only they can do. The stale docs stop lying.

**Why first.** Every later proof (media, voice, security playbooks, operator docs) runs on this stack, and the production host is the owner's largest pending action; a rehearsal in CI removes most of the risk from it.

### Work items

**RDY-OPS-001 [OPS][TEST] Production compose smoke in CI.** `prod-smoke.yml` (manual dispatch and weekly) builds the server image as `release-dry-run.yml` does, writes `.env.prod` from `.env.prod.example` with generated secrets, renders the Caddyfile for `https://localhost` (Caddy's internal CA), runs `deploy/install.sh --yes` against `deploy/compose/prod.yml` on the runner, waits for `/healthz` through Caddy, runs `sr-cli migrate` and `bootstrap`, and signs in through Caddy with the Playwright `entrance-smoke` spec. Acceptance: the job is green on the merged tree; every change it forced in `prod.yml`, `install.sh`, the Caddyfile or the Dockerfile is listed in the handoff; `deploy_files.rs` still passes; the job's runtime is under 20 minutes.

**RDY-OPS-002 [OPS][TEST] Backup, restore and WAL drill against the smoke stack.** In the same job after RDY-OPS-001: `backup.sh`, `restore.sh` into a second database, `wal-drill.sh` to a chosen moment, then `sr-cli backup` and `sr-cli restore --force` and `sr-cli upgrade --dry-run`. Acceptance: all five succeed against the running Compose stack; Admin > System shows the last backup time afterwards; the differences between the script path and the `sr-cli` path are written into `deploy/BACKUP.md`.

**RDY-OPS-003 [OPS] systemd path verified.** `systemd-analyze verify` over `deploy/systemd/*` on the runner, the static-binary path of `INSTALL.md` walked by a script that installs the built binaries under a temp prefix, writes `/etc/sideroom/config.toml` from the documented template and starts `sr-server` as a user service long enough to answer `/healthz`. Acceptance: the script runs in `prod-smoke.yml`; the unit files pass verify; the docs match what the script did.

**RDY-OPS-004 [BE][FE] `sr-cli preflight`.** One command, one shared function (`sr_core::preflight` or a new small crate) that checks: database reachable and migrations current; storage put, get, presign, multipart create and abort; SMTP connect (no send) with an optional `--send-to`; public URL reachable from the box with the expected headers (HSTS, CSP, `frame-ancestors`); TURN: credentials derive and an allocation against the configured coturn succeeds; VAPID key present when push is on; disk free on the data paths; clock skew against the database; secret files exist with mode 0600 or 0400; esplora reachable when payments are on; voice node reachable when voice is on. Output is a table with pass, warn, fail and a sentence per row; exit code non-zero on any fail. Acceptance: unit tests per check with fakes; a Postgres test over the database checks; the command runs in `prod-smoke.yml` and passes; `install.sh` runs it as its last step and prints the table.

**RDY-OPS-005 [FE][BE] Preflight in Admin.** Admin > System gains a "Preflight" card that runs the same checks server-side (`POST /admin/system/preflight`, `OPERATOR` (D-355)) and shows the table; the Setup Wizard runs it as its final step. Acceptance: route in the matrix and OpenAPI; a Vitest for the card; the access log records the run.

**RDY-OPS-006 [BE][TEST] Alert path test.** `sr-cli preflight --alert-test` posts a test card to the Staff Server `#alerts` channel and, when the Owner email is on, sends the mail, through the real `StaffFeed` path. Acceptance: a Postgres test; the card names the host and time; documented in `RUNBOOK.md`.

**RDY-OWN-001 [DOC] The lawyer brief.** `docs/legal/lawyer-brief.md`: every open legal question as a numbered item with: the question in plain words, the project's default and where it is set (setting key, screen, document), what changes if the answer differs, and the decision it came from (D-287, D-293, D-294, D-295, D-297 and D-309, D-311, D-314, D-336 (a) to (h), D-341, D-343, D-347). A short "about the instance" preamble (private, invitation-only, operator's country, what is stored and for how long from doc 09). Marked "not legal advice; questions for a lawyer" at the top. Acceptance: every listed decision appears once; the doc links checker passes; `open-questions.md` points to it instead of repeating the list.

**RDY-OWN-002 [DOC] The owner's action checklist.** `docs/dev/owner-checklist.md`: every pending owner or integrator action in one ordered list with a checkbox, grouped as accounts (Discord redirect URI), host (ingest folder, VAPID keys, electrs digest and node or the interim, Storage Box, DNS, firewall), GitHub (branch protection, required checks, the first runs of `release.yml`, `openapi.yml`, `storage.yml`, `e2e.yml`, `sdk-compat.yml`, `load-test.yml`, `backup-drill.yml`, the `visual-baselines` dispatch), legal (the brief, then the three settings and the notice address), and downloads waiting for a yes. Each row says what unblocks when it is done. Acceptance: `open-questions.md` sections "Owner and integrator actions" and "Human actions" are replaced by a link to it (the decisions stay in the log).

**RDY-OWN-003 [DOC][OPS] `deploy/GO-LIVE.md`.** The go-live checklist in order: provision and harden (doc 12 section 8), DNS, install, `sr-cli preflight` green, backups verified by a restore, monitoring profile up and the alert test seen in `#alerts`, the Setup Wizard done, lawyer settings set, the first invites. Each step names the command and what "done" looks like. Acceptance: linked from `INSTALL.md`, the README and the owner checklist; `check-ops-files.mjs` knows the file.

**RDY-DOC-001 [DOC] Stale statements fixed.** README "Status" rewritten to the truth (what is built, what is proven, where the roadmap is); doc 12 section 5 says `sr-cli backup`, `restore` and `upgrade` exist and WAL archiving is scripted; docs/README links the inventory as delivered and this roadmap; `THIRD_PARTY_NOTICES.md`: list the placeholders (FreeType year, NTP and Zlib holders) in a "to complete" note with the upstream file each text comes from, so the owner can approve fetching them. Acceptance: the three files changed; the doc links checker passes; no other wording of the README changes without a reason in the handoff.

### Split

| Agent | Worktree / branch | Items | Migrations | Owns |
|---|---|---|---|---|
| **ops-smoke** | `p20-ops-smoke` / `feat/p20-ops-smoke` | RDY-OPS-001, 002, 003 | 1180 to 1189 (none expected) | `.github/workflows/prod-smoke.yml`, `deploy/compose/prod.yml`, `deploy/install.sh`, `deploy/scripts/*`, `deploy/systemd/*`, `deploy/docker/*`, `deploy/compose/caddy/*`, `deploy/BACKUP.md`, `scripts/install.test.mjs`, `crates/sr-core/tests/deploy_files.rs` |
| **preflight** | `p20-preflight` / `feat/p20-preflight` | RDY-OPS-004, 005, 006 | 1190 to 1199 | a new `crates/sr-preflight` (or a module in `sr-core`), `crates/sr-cli/src/ops.rs` (new subcommand only), `crates/sr-api/src/routes/admin/system.rs` (new route only), `apps/web/src/admin` (one new card component and the wizard's last step), `docs/dev/server.md`, `deploy/RUNBOOK.md` (one new section) |
| **owner-packet** | `p20-owner-packet` / `feat/p20-owner-packet` | RDY-OWN-001, 002, 003, RDY-DOC-001 | 1200 to 1209 (none) | `docs/legal/*` (new), `docs/dev/owner-checklist.md` (new), `deploy/GO-LIVE.md` (new), `README.md`, `docs/README.md`, `docs/12-deployment-and-operations.md` section 5, `docs/open-questions.md`, `THIRD_PARTY_NOTICES.md` (note only), `deploy/INSTALL.md` (one link) |

`install.sh` calls `sr-cli preflight` (ops-smoke's file, preflight's command): preflight lands the command behind a flag first; ops-smoke adds the call in the integration branch, not on either feature branch. Ports 8080 and 5173: nobody in this wave needs them for long; preflight uses them for its browser check of the Admin card and says so in its handoff. No agent in this wave needs a download.

### Needs the owner

- Nothing to start the wave.
- After it: the real install on the host from `GO-LIVE.md`; the first dispatch of `prod-smoke.yml` (a push); the lawyer brief handed over; the notices texts fetched from upstream if the owner wants the placeholders filled (a download of licence text; the alternative is to keep the note).
- Optional: a secret scanner action (gitleaks) in CI. Wave 21 ships a no-download scanner instead; gitleaks would be an addition.

### Risks

- The runner is not a Hetzner box: no public DNS, Caddy on an internal CA, no UFW. The smoke proves Compose, the installer, migrations, backups and headers; it does not prove ACME or the firewall. `GO-LIVE.md` says so.
- `install.sh` was tested against a recording fake Docker; the first real run will find prompts and ordering bugs. That is the purpose; budget for two or three iterations of the job.
- The preflight TURN check needs a coturn to talk to in CI: `dev.yml` already runs `coturn/coturn:4.6` in the e2e stack, so the check runs there; against `prod.yml` the check reports "no TURN configured" as a warn, not a fail.
- The lawyer brief must not drift from the decisions log: it quotes decision ids and never restates a decision in new words.

## 4. Wave 21: security proof before strangers arrive

**Goal.** Every permission claim in the API is a test, the threat model describes the system as built through phase 19 with a file and a test per control, secrets cannot land in the repo unnoticed, and abuse limits exist on every member-writable surface the moderation and developer work added.

### Work items

**RDY-SEC-001 [TEST][BE] Per-route authorization proof.** `crates/sr-api/tests/route_matrix.tsv` loses every `?` and `other` cell; a generated test (`authz_matrix.rs`, Postgres, `#[ignore]` like its neighbours) builds one cast once (anonymous; a member of no guild; a member; a channel-overwrite-denied member; a server moderator with and without the needed guild bit; staff with and without the platform bit; a bot token with and without the scope; a bearer token with and without the scope; the Owner) and calls every route in the TSV with placeholder ids from a seeded world, asserting the status class the AUTHZ column promises for each actor. Acceptance: every route row has an explicit expectation; the test passes; each cell that differed from the heuristic is either a code fix (listed in the handoff with its test) or a TSV correction with a reason column; doc 05 gains a short section "How authorization is tested".

**RDY-SEC-002 [TEST][BE] Cross-tenant id sweep.** For every route with a `{guild_id}`, `{channel_id}`, `{message_id}`, `{submission_id}`, `{case_id}`, `{request_id}`, `{application_id}` segment, the matrix test also calls it with an id that belongs to another guild, member or application and asserts 403 or 404 and no write. Acceptance: the sweep is part of RDY-SEC-001's run; findings fixed with tests.

**RDY-SEC-003 [BE][TEST] Moderation and developer routes first.** The order inside RDY-SEC-001: `/moderation/*`, `/admin/*`, `/developers` and `/applications/*`, `/oauth2/*`, `/submissions/*`, then the rest. Acceptance: the handoff lists the groups in the order they were proven, so an interrupted wave still leaves the most sensitive routes proven.

**RDY-SEC-004 [DOC] Threat model as built.** `docs/15-threat-model.md` rewritten to phase 19: assets gain the Person graph, the moderation register, data-subject exports and the suspicion signals; actors gain the takedown notifier and the data subject; each row names the file and the test that enforce it; rows T6 (invite codes at rest), T7 (per-route proof, now RDY-SEC-001), T13 (secret scanner, now RDY-SEC-005), T17 (load, wave 24) are updated; a new row per outward-facing feature shipped since phase 4 (payments watcher, bot gateway and OAuth2, activities, webhooks execute, Web Push, the SDK voice bot). Acceptance: no row says "open" without naming the RDY item that closes it; reviewed against the code by grep, not from memory.

**RDY-SEC-005 [OPS] Secret scan without a download.** `scripts/secret-scan.mjs`: a regex set (private key blocks, `AKIA`, GitHub and Discord token shapes, `SR_*` secret keys assigned a non-placeholder value, `.env` files that are not `*.example`, `password=` in URLs) over tracked files and over the diff in CI, with an allow-list file and a test. Wired into `repo-checks.yml`. Acceptance: the scan passes on main; a test proves it catches each pattern; the threat model row T13 points to it. gitleaks stays an optional addition for the owner.

**RDY-SEC-006 [BE][TEST] Abuse limits on the new surfaces.** Per-account and per-IP limits with tests on: reports per day, appeals per case, data requests per account, counter-notices, invite-code checks (exists; asserted), DM requests, friend requests, webhook execute per token, OAuth2 authorize attempts, interaction callbacks, watch-party joins, sticker and emoji uploads per hour. Each limit is a platform setting with a default and appears in doc 05's limits table and in Admin > Limits. Acceptance: a table in doc 05 lists every limit and its test; the gateway and REST limiters keep their per-process scope (one node; documented, not changed).

**RDY-SEC-007 [BE][TEST] Rotation and compromise drills run.** `sr-cli rotate-secret-key`, `verify-access-log`, `user reset-2fa`, session revocation from Admin and bot-token rotation are each run in a Postgres test that performs the drill end to end (rotate, restart the config, verify every TOTP secret and sealed seed still decrypts; tamper a log row and see it detected). Acceptance: the tests exist and pass; `RUNBOOK.md` "Lost or suspect secret key" and a new "Compromised staff account" section quote the commands the tests ran.

**RDY-SEC-008 [BE][TEST] Upload hygiene fuzz.** Generated (not downloaded) corpora for the archive inspector, the image re-encoder and the sticker and sound validators: nested archives, zip bombs at the limit and one byte over, polyglots (PNG with a trailing ZIP, GIF with HTML), truncated files, names with every forbidden segment. A `proptest` or hand-rolled generator; limits per surface asserted. Acceptance: tests in `sr-library`, `sr-storage` and `sr-packages`; any crash or over-limit acceptance fixed; the per-surface limit table in doc 12 "Upload hygiene" matches the code.

**RDY-SEC-009 [BE][FE] CSP violation reporting.** `report-to` and `report-uri` pointing at `POST /internal/csp-report` (unauthenticated, rate limited, body capped), stored as a counter per directive and top blocked origins, shown on Admin > System. Acceptance: Caddyfile and the Vite dev headers emit the directive; a test posts a report and reads the counter; no personal data is stored (the document URL is reduced to its path).

**RDY-SEC-010 [BE][TEST] Sensitive-action reauth list.** One list in code of the actions that require a fresh second factor or password (staff appointment, platform ban, data-request outcome, takedown uphold, secret rotation, owner transfer, application secret reset, payout descriptor change), a test that every listed route enforces it, and the list rendered in doc 07. Acceptance: the test passes; `SR_ADMIN_REQUIRE_REAUTH=0` remains the e2e escape hatch and is refused when `public_url` is not localhost.

### Split

| Agent | Worktree / branch | Items | Migrations | Owns |
|---|---|---|---|---|
| **authz-matrix** | `p21-authz-matrix` / `feat/p21-authz-matrix` | RDY-SEC-001, 002, 003 | 1210 to 1219 (none expected) | `crates/sr-api/tests/route_matrix.tsv`, `crates/sr-server/tests/authz_matrix.rs` (new; built under `sr-api`, moved by D-359), `crates/sr-api/tests/route_matrix.rs`, the handler fixes it needs (each named in the handoff), `docs/05-api-and-gateway.md` (one new section) |
| **threat-refresh** | `p21-threat-refresh` / `feat/p21-threat-refresh` | RDY-SEC-004, 005, 006 | 1220 to 1229 (settings rows for the new limits) | `docs/15-threat-model.md`, `scripts/secret-scan.mjs` and its test, `.github/workflows/repo-checks.yml`, `crates/sr-api/src/rate_limit.rs` and the limit sites it adds, `crates/sr-admin` limits catalog, `apps/web/src/admin` Limits page rows, doc 05 limits table |
| **sessions-uploads** | `p21-sessions-uploads` / `feat/p21-sessions-uploads` | RDY-SEC-007, 008, 009, 010 | 1230 to 1239 (the CSP report counters) | `crates/sr-cli` drill tests, `deploy/RUNBOOK.md` (two sections), `crates/sr-library/tests`, `crates/sr-storage/tests`, `crates/sr-packages/tests`, `crates/sr-api/src/routes/internal` (new report route), `crates/sr-api/src/security_headers.rs`, `deploy/compose/caddy/*` CSP line, `apps/web/src/security/csp.ts`, Admin > System one new card, doc 07 reauth list, doc 12 upload table |

Conflict watch: authz-matrix and threat-refresh both touch `sr-api`; authz-matrix owns tests and handler fixes, threat-refresh owns `rate_limit.rs` and the new limit calls. A handler that needs both a fix and a limit is fixed by authz-matrix and the limit added by threat-refresh on the integration branch. Ports 8080/5173: sessions-uploads, for the CSP report check in a browser.

### Needs the owner

- Nothing to start.
- Optional: gitleaks in CI (an action download on GitHub's side); the owner decides after seeing the no-download scanner's results.
- The branch protection rules (owner checklist) make CODEOWNERS and the new checks binding.

### Risks

- The matrix will find real authorization bugs. That is its purpose, but the count is unknown; RDY-SEC-003's order keeps the sensitive groups first if time runs out.
- A generated test over 994 routes with ten actors is about 10,000 in-process requests; it must stay under five minutes on the Postgres test database or be split per route group.
- New rate limits can break existing e2e flows (the registration limit did in phase 16); every new limit is tested against the Playwright suite before merge.

## 5. Wave 22: media and voice for real

**Goal.** The phase 5 exit gate finally runs: a member uploads an episode in the browser, staff review it, a real ffmpeg transcodes it, it plays from HLS, it airs on a Live TV channel with the slate around it, and the result is proven weekly in CI. Chat attachments are proven over real S3 multipart. Voice is proven through a TURN relay.

### Work items

**RDY-MED-001 [DB][BE] Member video submissions.** `/submissions/{id}/cinema/*`: begin an upload as a multipart session (the REV-UP-001 sessions, charged by REV-CORE-008), attach the uploaded asset to a draft title or episode list (season and episode numbers from `sr_media::naming`), edit the draft metadata the kind already defines, submit for review; staff-sponsored actor rules and artwork handling as spec section 8 writes them. Acceptance: Postgres tests through `sr-review` from draft to published title; the route matrix and OpenAPI updated; byte limits enforced; the access log records each decision.

**RDY-MED-002 [FE] Cinema > Submit.** The Library submissions screen pattern for video: pick files or a folder, the naming tester, the upload manager dock, the draft editor for title and episodes, status and reviewer notes, resume after refresh through the IndexedDB ledger. Acceptance: Vitest for the screen; a Playwright spec that uploads a small generated MP4 through the UI against the Garage stack (the `live` job), and the review page's proxy player opens it.

**RDY-MED-003 [TEST][OPS] The media pipeline in CI with a real ffmpeg.** `media.yml` (weekly and on dispatch): installs ffmpeg on the runner, starts Postgres, Garage and `sr-server`, generates a 20-second source with lavfi (video, two audio tracks, one subtitle), runs probe, the review proxy, the planner and `sr-transcoder` (CPU path) as a real worker against the Worker API, asserts the ladder, the audio tracks, the WebVTT subtitle, the sprite and its VTT, then fetches the master and media playlists and every segment of one rendition and checks them with `ffprobe`. Acceptance: green; every bug it exposes in the transcoder or planner fixed with a unit test that reproduces it without ffmpeg where possible; `docs/dev/media-pipeline-test.md` says how to run it locally when ffmpeg is present.

**RDY-MED-004 [TEST] Slate, re-transcode, Patron download in the same job.** `sr-cli tv slate` produces the slate; a channel with a schedule airs the transcoded episode and falls back to the slate before and after; `POST /admin/transcode/retranscode` reruns one asset; the Patron download object is produced and served with the right headers. Acceptance: asserted in `media.yml`; the Live TV numbering rules (D-267, D-285) checked across the slate boundary with the real segments.

**RDY-MED-005 [BE][TEST] Transcoder resilience.** With the real worker running: kill the worker mid-job and see the lease expire and the job retried; feed a corrupt source and see a failure with logs in Admin > Transcoding; two workers claim without overlap. Acceptance: in `media.yml` or a Postgres test using the real `sr-transcoder` binary.

**RDY-VOX-001 [TEST] Voice through TURN relay.** A `voice-live` variant with `iceTransportPolicy: 'relay'` against the e2e stack's coturn (`dev.yml` already runs `coturn/coturn:4.6`), with `turnserver.prod.conf.example` rendered and used by that container, the REST secret path (`voice.turn.secret`) included. Acceptance: the Chromium live job runs it; the relay is proven by the selected candidate pair type in `getStats`; the prod coturn template is the one tested.

**RDY-VOX-002 [TEST] Voice soak.** Ten Chromium contexts in one voice channel for ten minutes with mute, unmute, camera on and off and one reconnect each, E2EE on, asserting no SFU restart, no frame-transform failures and every member still hears a tone at the end (the existing loopback assertion). Acceptance: a dispatch-only job (`voice-soak.yml`) that is green once on the merged tree with its log attached to the handoff; the number and duration are inputs so the owner can run the phase 2 gate (ten people, two hours) by hand.

**RDY-STO-001 [TEST] Chat multipart over real S3.** `e2e/support/storage-live.mjs` and `storage.yml` gain a chat attachment larger than the part size through the composer's upload path, an abort, and a resume from the IndexedDB ledger after a page reload, against Garage. Acceptance: green in `storage.yml`; the ledger's confirmed parts match S3's `ListParts`.

**RDY-STO-002 [TEST] Composer reattach over the network.** The reattach after refresh with a slow network (Playwright route throttling) and a server restart in the middle. Acceptance: part of RDY-STO-001's run; failures fixed in `lounge/uploads` with unit tests.

### Split

| Agent | Worktree / branch | Items | Migrations | Owns |
|---|---|---|---|---|
| **cinema-submit** | `p22-cinema-submit` / `feat/p22-cinema-submit` | RDY-MED-001, 002 | 1240 to 1249 | `crates/sr-cinema` submission kinds and routes, `crates/sr-api/src/routes/submissions/cinema.rs` (new), `apps/web/src/cinema/submit/*` (new), `apps/web/src/review` shared pieces by extension only, `docs/specs/cinema-media.md` section 8, `e2e/tests/cinema-submit.spec.ts` (new) |
| **ffmpeg-ci** | `p22-ffmpeg-ci` / `feat/p22-ffmpeg-ci` | RDY-MED-003, 004, 005 | 1250 to 1259 (none expected) | `.github/workflows/media.yml` (new), `crates/sr-transcoder`, `crates/sr-media` probe and planner, `crates/sr-tv` as the slate test needs, `crates/sr-cli/src/tv.rs`, `docs/dev/media-pipeline-test.md` (new), `deploy/TRANSCODER.md` |
| **voice-storage** | `p22-voice-storage` / `feat/p22-voice-storage` | RDY-VOX-001, 002, RDY-STO-001, 002 | 1260 to 1269 (none expected) | `e2e/tests/voice-live.spec.ts` and a new `voice-relay.spec.ts`, `.github/workflows/voice-soak.yml` (new), `deploy/compose/dev.yml` coturn service, `deploy/compose/coturn/*`, `e2e/support/storage-live.mjs`, `.github/workflows/storage.yml`, `apps/web/src/lounge/uploads/*` fixes, `crates/sr-voice` fixes the soak needs |

Conflict watch: cinema-submit and ffmpeg-ci both read `sr-media::naming` and the review proxy; only ffmpeg-ci changes `sr-media`, cinema-submit consumes it. Ports 8080/5173: voice-storage (browser runs); cinema-submit uses 8081/5174 for its Playwright spec locally and says so.

### Needs the owner

- **Download, recommended:** ffmpeg on the GitHub runner (`apt-get install ffmpeg`, a workflow step, nothing on the owner's machine). Without it RDY-MED-003 to 005 cannot run; this is the one download the roadmap depends on.
- **Download, optional:** ffmpeg on a development machine (human action 5, D-269) so an agent can run the pipeline locally instead of only in CI. Not required.
- **Money, optional:** one GPU rental to run the NVENC path of `sr-transcoder` once (`deploy/TRANSCODER.md`); the CPU path in CI is the proof the wave needs. Record the run in the handoff if done.
- A Mac or iPhone for Safari voice E2EE and the installed PWA stays a human action; the wave records "untested on Safari" where that is true.

### Risks

- The transcoder has never met a real ffmpeg; expect surprises in argument order, codec names and HLS flags. The first green `media.yml` may take most of ffmpeg-ci's time; RDY-MED-005 is cut first if needed.
- The member upload path crosses review, storage and cinema and is the largest product item in this roadmap; its spec is complete (cinema-media section 8), which keeps it buildable.
- Voice soak in CI competes for CPU; the job is dispatch-only, and its pass criterion is "no failure", not latency.

## 6. Wave 23: people readiness

**Goal.** A new member arrives, understands the place and can use it with a keyboard or a screen reader; the operator has one handbook, with a playbook behind every alert.

### Work items

**RDY-A11Y-001 [TEST] Browser axe on signed-in routes.** `e2e/tests/a11y.spec.ts`: axe (already in `e2e/package.json`) over a fixture world (Lounge channel with messages and a thread, Settings sections, Library series and reader, Cinema title and player, watch party, Admin home, Developers, Support), including contrast and landmarks, on Chromium, Firefox and WebKit. Acceptance: zero serious or critical violations; moderate ones listed with an owner per file in the handoff and fixed where under an hour each.

**RDY-A11Y-002 [FE] Roving tabindex and skip links everywhere.** The member list, the message list, the Library and Cinema grids and the TV guide get roving tabindex through `a11y/useRovingTabindex.ts`; each section shell (Lounge, Cinema, Library, Admin, Developers) gets a skip link to its main region. Acceptance: unit tests per list; the keyboard map in `docs/dev/a11y-checklist.md` has no follow-ups left.

**RDY-A11Y-003 [TEST] The keyboard-only walk, automated.** The eight flows of the manual checklist walked by Playwright with the keyboard only (Tab, arrows, Enter, Escape), asserting focus position and the live-region text at each step. Acceptance: one spec per flow, green on three browsers; the manual reader pass (NVDA, VoiceOver, TalkBack) stays a human action and the checklist says which browser pairs are still unheard.

**RDY-A11Y-004 [FE] Reduced motion and high contrast audit.** Every animation in the Lounge, Cinema and Library honours the reduced-motion preference; the high-contrast setting is checked against the design tokens for every section. Acceptance: a test that walks the token set; the Ladle stories render both modes.

**RDY-ONB-001 [FE][BE] Platform first run.** After registration and the first sign-in: a short welcome (what the three sections are, in the Entrance style), pick the appearance, a 2FA nudge with the setup in place, the PWA install prompt where the browser allows it, the notification permission asked only when the person opens Settings > Notifications or clicks a bell, and the home server's own onboarding (LNG-SSET community onboarding) if the operator set one. Stored as `users.first_run_done`; skippable; re-openable from `/help`. Acceptance: a Vitest for each step; a Playwright spec from register to first message; nothing in it blocks a person who closes it.

**RDY-ONB-002 [FE][DOC] The member handbook at `/help`.** `/help` becomes a short handbook in the Entrance style: joining and invites and credits, sign-in with 2FA and passkeys, installing the PWA on iOS and Android, the Lounge in one page for people who know Discord (and the differences from the decisions log), submitting to the Library and the Cinema with the naming guide, watch parties, Live TV, privacy (what is stored, export, delete, reports, the operator's country and contact), the house notes. Content lives in i18n catalogs, not JSX text. Acceptance: every section links to the screen it describes; the privacy page and the handbook agree with doc 09; the doc links checker covers in-app routes it names.

**RDY-ONB-003 [FE] "What's new".** `CHANGELOG.md` at the repo root (kept by the integrator per wave from the handoffs), rendered at `/help/whats-new`, with a one-time badge on the help button when the version changes. Acceptance: the build embeds the version; the badge test; the integrator's checklist in `AGENTS.md` gains the changelog line.

**RDY-I18N-001 [FE] Hard-coded strings in the Entrance and the Lounge shell.** The `i18n-audit.mjs` count (456 at phase 18) brought to zero for `apps/web/src/routes/entrance`, `apps/web/src/lounge/shell` and `apps/web/src/settings`, so a second locale is possible without touching those files. Acceptance: the audit reports zero in those paths and the number for the rest; `i18n-verify-diff` MISSING 0.

**RDY-DOC-002 [DOC][OPS] One operator handbook.** `deploy/OPERATOR.md` as the single entry: a map of INSTALL, GO-LIVE, UPGRADE, BACKUP, RUNBOOK, GARAGE, TRANSCODER and FAQ, the daily, weekly and monthly checks, the settings an operator must know (`accounts.minimum_age`, `compliance.*`, limits), what Admin > System's tiles mean. Acceptance: every deploy doc is reachable from it; `check-ops-files.mjs` knows it.

**RDY-DOC-003 [DOC][OPS] A playbook behind every alert.** Each rule in `deploy/monitoring/alerts.yml` names a `RUNBOOK.md` anchor; `check-ops-files.mjs` fails when an alert has no playbook or a playbook names a metric that does not exist; Admin > System's red tiles link to the same anchors. Playbooks added: job backlog, voice egress, invoice stuck, storage usage, WAL archive lag, restore drill failed, CSP reports spike, suspicion case opened. Acceptance: the check passes; each playbook was walked once against the wave-20 smoke stack and says so.

**RDY-DOC-004 [DOC][OPS] Incident playbooks.** Takedown notice received, data-subject request received, suspected compromised staff account, lost owner second factor, disk full, restore from backup, a member reports CSAM (the preservation and reporting steps the lawyer confirms, D-336 (h), with a placeholder until then). Each playbook names the screens, the `sr-cli` commands and the settings. Acceptance: reviewed against the spec for each process; the lawyer-dependent steps are marked.

**RDY-DOC-005 [DOC] Exit gates as a status table.** `docs/13-roadmap-and-phases.md` gets, per phase, the exit gate and where it was proven (a CI job, a handoff, a recorded run) or "not yet, see RDY-…". Acceptance: no gate is marked proven without a link.

### Split

| Agent | Worktree / branch | Items | Migrations | Owns |
|---|---|---|---|---|
| **a11y** | `p23-a11y` / `feat/p23-a11y` | RDY-A11Y-001 to 004 | 1270 to 1279 (none) | `e2e/tests/a11y*.spec.ts` and `keyboard-*.spec.ts` (new), `apps/web/src/a11y/*`, the list components it retrofits (member list, message list, Library and Cinema grids, TV guide), `packages/ui` tokens for contrast, `docs/dev/a11y-checklist.md` |
| **first-run** | `p23-first-run` / `feat/p23-first-run` | RDY-ONB-001 to 003, RDY-I18N-001 | 1280 to 1289 | `apps/web/src/routes/entrance/*` (Static, the welcome), `apps/web/src/lounge/header/HelpButton.tsx`, `apps/web/src/i18n/messages/en/*` for its areas, `crates/sr-api` the `first_run_done` field and route, `CHANGELOG.md` (new), `AGENTS.md` (one line) |
| **operator-handbook** | `p23-operator-handbook` / `feat/p23-operator-handbook` | RDY-DOC-002 to 005 | 1290 to 1299 (none) | `deploy/OPERATOR.md` (new), `deploy/RUNBOOK.md`, `deploy/monitoring/alerts.yml`, `scripts/check-ops-files.mjs`, `apps/web/src/admin` System page tile links only, `docs/13-roadmap-and-phases.md` |

Conflict watch: a11y and first-run both touch `apps/web`; a11y stays out of `routes/entrance` and the help button, first-run stays out of the list components and `a11y/`. Ports 8080/5173: a11y.

### Needs the owner

- The human screen-reader pass (NVDA on Windows is free; VoiceOver needs a Mac or iPhone). Not a download the agents make.
- Nothing else.

### Risks

- Axe on signed-in routes will report many moderate findings at once; the acceptance limits the wave to serious and critical plus the cheap moderates, the rest become listed follow-ups.
- The first-run flow must not become a wall; every step is skippable and the spec from register to first message is the guard.
- The playbooks depend on the wave-20 smoke stack to be walked; if wave 20 slipped, they are written against the dev stack and marked.

## 7. Wave 24: scale, parity remainders, developer platform remainders

**Goal.** The sizing claim (20,000 accounts, 2,000 concurrent) is measured on a release build over a seeded dataset and the top findings fixed; the Discord parity holes the inventory notes are closed; the SDK story ends where doc 11 said it would. This wave may run after the first invites.

### Work items

**RDY-PERF-001 [BE] `sr-cli seed --scale`.** Generates 20,000 accounts, 500 guilds with realistic membership skew, 5 million messages with attachments and reactions, 2,000 library chapters and 300 cinema titles, through `COPY`, in under 30 minutes on CI hardware; refuses non-local instances without `--force`. Acceptance: idempotent per seed name; a test over a tiny scale; documented in `load-testing.md`.

**RDY-PERF-002 [TEST][OPS] Release-build load test on CI.** `load-test.yml` runs against the seeded dataset with release builds: 2,000 sessions, 200 msg/s, 300 REST/s, REST p95 under 100 ms, fan-out p95 under 250 ms, first paint with 60 servers (K-22) under 1.5 s. Acceptance: the numbers are recorded in `load-testing.md` with the run link; each miss has a profile (`tokio-console` is not added; `tracing` spans and `EXPLAIN` are) and a fix or a documented limit.

**RDY-PERF-003 [BE][JOB] Jobs at scale.** Search index rebuild over 5 million messages, the retention purges, the suspicion scan, the trending aggregates and the backup over the seeded database, each timed and bounded (batch sizes, `SKIP LOCKED`, statement timeouts). Acceptance: no job holds a lock longer than one second at a time; each job's runtime at scale is in `load-testing.md`; indexes added as migrations where `EXPLAIN` demands.

**RDY-PERF-004 [BE] Gateway memory at 2,000 sessions.** Measured resident memory per session and per guild subscription; the `READY` payload size for a member of 100 servers; lazy guild member chunks checked. Acceptance: numbers recorded; a budget test fails when the per-session figure doubles.

**RDY-PAR-001 [BE][FE] Group DM invite link.** `POST /channels/{id}/invites` for group DMs, the copy-link in the Create DM picker (LNG-HOME-005), join by link, Discord's limits. Acceptance: Postgres and Vitest; OpenAPI.

**RDY-PAR-002 [FE] Emoji picker category sidebar.** The categories sidebar with server icons and category glyphs, scroll sync, keyboard navigation (LNG-EXP-007 remainder). Acceptance: Vitest; the picker's axe check stays green.

**RDY-PAR-003 [DB][BE][FE] Profile theme, server tag, markdown in About Me.** Two-colour profile theme (free, declarative), the server tag display choice, markdown in About Me through `sr-markdown` with the profile subset (LNG-USET remainder). Acceptance: the shared profile card renders the same from the popout and the full profile; tests.

**RDY-PAR-004 [BE][FE] DM spam filter and per-server DM override.** The "filter DMs from unknown members" setting backed by the existing link and spam heuristics, and the per-server "allow DMs from members of this server" override (LNG-USET privacy remainder). Acceptance: Postgres tests on the request routing; Vitest.

**RDY-PAR-005 [FE] View Server As Role as a Lounge mode.** The dialog of J-09 becomes a Lounge-wide preview with the exit banner (LNG-SSET-036 as specified). Acceptance: the permission engine stays `sr-permissions` WASM; Vitest and one Playwright spec.

**RDY-PAR-006 [FE] GIF autoplay preference.** "Automatically play GIFs when the client is focused" with the stickers and emoji animation preferences. Acceptance: Vitest; respects reduced motion.

**RDY-DEV-001 [FE] TypeScript SDK voice.** `packages/bot` gains voice with its own WebRTC transport mirroring `sr_sdk::voice` names, E2EE through `sr-e2ee-wasm`. Acceptance: a Node test against the real SFU in the `live` job; the developer docs page updated.

**RDY-DEV-002 [BE] `sr-sdk` reconnect and resume, and a live voice-bot run.** Gateway resume and voice reconnect in `sr-sdk`; `examples/voice_bot.rs` run in CI against the real server and SFU for one minute. Acceptance: tests in `crates/sr-sdk/tests`; the example's run in `e2e.yml` live job.

**RDY-DEV-003 [TEST] SDK compatibility, next pair.** When the first scheduled `sdk-compat` run is green and useful (D-306): discord.py and twilight as the next two libraries, in the same weekly job. Acceptance: 7 of 7 each, or a documented gap list.

**RDY-DEV-004 [DOC] Developer docs pass.** The developer portal's docs content (`developers/docsContent.ts`) checked against the built API for every route and event it names; the SDK compatibility list (`docs/dev/sdk-compat.md`) updated from the weekly run. Acceptance: a test that every route the docs name exists in OpenAPI.

### Split

| Agent | Worktree / branch | Items | Migrations | Owns |
|---|---|---|---|---|
| **scale** | `p24-scale` / `feat/p24-scale` | RDY-PERF-001 to 004 | 1300 to 1309 (indexes) | `crates/sr-cli/src/seed*.rs`, `crates/sr-loadtest`, `.github/workflows/load-test.yml`, `docs/dev/load-testing.md`, the job batching it changes (one job per commit, named), `crates/sr-gateway` memory budget test |
| **parity** | `p24-parity` / `feat/p24-parity` | RDY-PAR-001 to 006 | 1310 to 1319 | `crates/sr-chat` invites and DM routing, `crates/sr-userfeat` profile fields, `apps/web/src/lounge/components/EmojiPicker.tsx`, `apps/web/src/lounge/profile/*`, `apps/web/src/settings/user/*` (privacy, profiles, accessibility sections), `apps/web/src/lounge/settings/ViewAsRole.tsx` |
| **devplat** | `p24-devplat` / `feat/p24-devplat` | RDY-DEV-001 to 004 | 1320 to 1329 (none expected) | `packages/bot`, `crates/sr-sdk`, `e2e/sdk-compat/*`, `.github/workflows/sdk-compat.yml`, `apps/web/src/developers/docsContent.ts`, `docs/dev/sdk-compat.md`, `docs/specs/developer-platform.md` |

Conflict watch: scale touches job code in several crates; it names each job it changes and parity stays out of jobs. Ports 8080/5173: devplat (live SDK runs); scale runs its server on 8082.

### Needs the owner

- Pushes and dispatches: `load-test.yml` (release build on CI), the first `sdk-compat` run.
- **Downloads, optional:** discord.py (`pip`, into `e2e/sdk-compat/`) and twilight (a crate in the nested Cargo project), only if D-306's condition holds. Both are flagged as the only new libraries this roadmap proposes.
- Nothing legal.

### Risks

- Scale numbers on CI hardware are a floor, not the Hetzner figure; the doc says which machine produced them.
- Parity items are the easy place to overrun; each is an inventory remainder with a known shape, and none is launch-blocking.
- TypeScript SDK voice needs WebRTC in Node (`werift` or a native binding), which is a download and a licence check; if the owner declines, RDY-DEV-001 is closed as browser-only (the Embedded App SDK path) and the decision recorded.

## 8. What the owner needs, in one list

Nothing below starts a wave; each unblocks a tick.

1. **Legal:** the lawyer brief (RDY-OWN-001) to a lawyer; then the three settings, the notice address, and the answers to N-35 and D-336 (h).
2. **Accounts:** the Discord redirect URI (D-292).
3. **Host:** the real install from `GO-LIVE.md` after wave 20; `/srv/sideroom/ingest`; VAPID keys; the electrs digest and node or the mempool.space interim; the Storage Box; branch protection and required checks.
4. **Pushes and dispatches:** each integration branch; `prod-smoke.yml`, `media.yml`, `voice-soak.yml`, `load-test.yml`, `sdk-compat.yml`, `visual-baselines`.
5. **Downloads, recommended:** ffmpeg on the GitHub runner (wave 22). That is the only one the plan depends on.
6. **Downloads, optional:** ffmpeg on a development machine; gitleaks in CI; upstream licence texts for the notices placeholders; discord.py and twilight; a Node WebRTC library for the TypeScript SDK voice.
7. **Money, optional:** one GPU rental for the NVENC run.
8. **Human tests:** the screen-reader pass; Safari voice E2EE and the iOS PWA on a real device; the two-week daily-chat and two-hour voice gates with real friends.

## 9. What this roadmap does not do

- It does not reopen anything closed as won't-build.
- It does not plan native clients, Lightning or Monero, AV1, multi-node (`PgNotifyBus`) or bounties; they stay in doc 13 "Later".
- It does not add a non-Rust service, and the only new crate it suggests is a small preflight crate.
- It does not decide the legal questions; it packages them.
