# Handoff: integration/phase23 (wave 23 merges, integrator work, gates)

Main checkout `C:\Users\braxt\Desktop\SIDEROOM_repo`, branch `integration/phase23` (base `c1b6ede5`, the phase-22 tick commit on top of main `3e631f57`). Local commits only: nothing pushed, no PR, nothing merged to main. Cargo target dir: `C:/Users/braxt/Desktop/SIDEROOM_wt/p23-integ-target` (built fresh in this pass).

## Resume here

**State (2026-10-25): done locally; the cross-browser and storage proofs wait for CI.** The three Phase 23 branches are merged, the integrator T-items T-02, T-03, T-04, T-07 and T-08 are built (T-01, T-05 and T-06 came built on the branches), and every local gate below passes on the merged tree. No server, Vite, cargo, Vitest or Playwright process of this pass is running. The scratch databases `sr_p23_int` and `sr_p23_ie2e` were dropped, and so were the leftover templates (see Clean-up). The `postgres` database was not touched.

**What waits on the first CI run of the pull request** (each stays as it is until its job is green):

| What | Job | Why it cannot run here |
|---|---|---|
| RDY-A11Y-001, 003 and 004 specs on **Firefox** | `e2e.yml` `playwright` (matrix `firefox`) | Firefox does not launch on this machine (VC runtime) |
| The same specs on **Linux WebKit** | `e2e.yml` `playwright` (matrix `webkit`) | Here only the Windows WebKit build ran: 40 passed, 8 skipped. The skips are the keyboard specs that cross a link, because this build does not Tab to links (the Safari default); on Linux they run |
| `keyboard-5-attachment` to the end ("Uploaded ...", the message in the list, focus back in the composer) | `e2e.yml` `playwright`, over Garage from `e2e-stack` | Memory storage has no upload completion; locally it stops after "Uploading keyboard-note.txt" (the keyboard path, tray and announcement up to there are proven) |
| `keyboard-6-voice` in CI | `e2e.yml` `live`, first step | **Proven locally** for the first time in this pass (4 of 4 after the spec fix below); CI is the confirmation on Linux |
| `cinema-submit.spec.ts` (from wave 22) | `e2e.yml` `live`, cinema step | Needs Garage and ffmpeg, as in `feat-p22-integration.md` |
| Every wave-22 item still listed in `feat-p22-integration.md` "Resume here" | as listed there | unchanged |

**What waits on the owner:**

- **The human screen-reader pass** (NVDA on Windows is free; VoiceOver needs a Mac or an iPhone; TalkBack). Every pair is listed as unheard in `docs/dev/a11y-checklist.md`; the owner checklist has the row.
- **The voice soak dispatch** (`voice-soak.yml`, RDY-VOX-002, from wave 22): once the PR is green, with the defaults; drop `camera` first if it struggles on `ubuntu-latest`.
- **The lawyer** (RDY-OWN-001): the CSAM playbook's steps 6 and 7 are marked LAWYER and undefined until D-336 (h) and N-35 are answered; the brief is `docs/legal/lawyer-brief.md`.
- **RDY-PAY-001's funded signet wallet** (new owner-checklist row), wave 24.

**RDY-DOC-003 stays unticked: the playbook walk.** Every playbook in `deploy/RUNBOOK.md` says "written against prod-smoke's documented stack; walk pending". The acceptance asks for each to be walked once against the wave-20 smoke stack. That needs the prod-smoke stack (Docker, Garage, Postgres with WAL archiving), which this Windows machine cannot run. Whoever walks them (the owner during the GO-LIVE rehearsal, or a CI-side agent with the prod-smoke stack): replace the status line in `RUNBOOK.md` with the date and the stack, and tick RDY-DOC-003. What *is* proven: `check-ops-files.mjs` passes (every alert rule names a playbook, every tile anchor exists, no playbook names a missing metric), and the in-app runbook renders all 22 required anchors (`p23runbook.test.tsx`, and by hand below).

**For the owner or the coordinator:**

1. Push `integration/phase23` and open the PR when the owner agrees. Then read `E2E` (`playwright` firefox and webkit for the a11y and keyboard specs; `live` for `keyboard-6-voice`).
2. The keyboard and a11y specs touch no workflow file; the new specs live in `e2e/tests/` and so run in the existing `playwright` matrix.
3. Wave 24 starts from the roadmap section 7 as amended here (RDY-PAY-001, RDY-LIB-001, RDY-OBS-001, RDY-SCALE-002).

**How to rerun the browser tests locally (mirror CI, not one big run).** The registration throttle (`POST /sr/register`: 5 a minute and **20 a day per address**, in memory) is per `sr-server` process. With the wave-23 specs, the main suite alone registers 16 accounts; adding the voice, activities, message-components and live specs to the same server run goes past 20, and the later specs fail with `429 ... retry_after: 86000`. That is what CI avoids with one server per step. So:

1. Fresh DB migrated with `sr-cli migrate`; `sr-server` with `SR_STORAGE__ENDPOINT=memory:`, `SR_SERVER__PUBLIC_URL=http://localhost:5173`, `SR_VOICE__ENABLED=true`, `SR_ACTIVITIES__ENABLED=true`, `SR_ADMIN_REQUIRE_REAUTH=0`; Vite from the local binary with `VITE_SR_E2E=1 VITE_ACTIVITIES_ENABLED=true`.
2. Main suite: `playwright test --project=chromium` with **no** `SR_E2E_*` flags (as the CI `playwright` job).
3. Restart `sr-server`; the live set with `SR_E2E_VOICE/ACTIVITIES/LIVE=1`: `tests/voice-live.spec.ts tests/activities-live.spec.ts tests/message-components.spec.ts tests/keyboard-6-voice.spec.ts`.
4. `SR_E2E_VISUAL=1 ... tests/visual.spec.ts`.
5. Restart `sr-server`; `-c playwright.e2ee.config.ts --project=chromium` with `SR_E2E_VOICE=1`.
6. `test:static` against `node apps/web/node_modules/vite/bin/vite.js preview apps/web --host 127.0.0.1 --port 4173 --strictPort`.

`SR_VOICE__TURN__URLS` must be a list in the environment form the config loader accepts; a plain string (`turn:127.0.0.1:3478`) stops `sr-server` at start with "expected a sequence". It is not needed for the runs above.

## Merges (all `--no-ff`, in this order)

| Branch (head) | Conflicts and resolution |
|---|---|
| feat/p23-operator-handbook (`c28824e4`) | none |
| feat/p23-first-run (`f0420e42`) | none |
| feat/p23-a11y (`6c6cc37a`) | none |

- Every docs section is kept: the decisions log D-368..D-373, `open-question-answers.md` "Phase 23" (P23-1 to P23-6), `answers-followups-p16.md` "Phase 23 round" (T-01..T-08), all three handoffs. The inventory merged as the union of the ticks (RDY-A11Y-001..004, RDY-ONB-001..003, RDY-I18N-001, RDY-DOC-002, 004, 005 ticked; RDY-DOC-003 open, above).
- **Generated files**, regenerated on the merged tree from the fresh `sr-server`: OpenAPI (from bash), the api-client schema, the MSW mocks (971 operations), `gatewayDocs.generated.ts` (then prettier), the route matrix (`SR_UPDATE_ROUTE_MATRIX=1`). All identical to the auto-merge. The matrix keeps its PLATFORM column (971 seven-column rows); the one new row is `PUT /sr/auth/first-run` `authenticated`/`self`. No `?` or `other` cells; `KNOWN_CLASHES` is empty.
- **Uniqueness:** migration `1280_first_run_done.sql` is the only new one; no number prefix repeats. No new job ids, audit action ids or settings keys came with this wave.

## Integrator work

- **T-02 (D-368):** roadmap-p20 section 7 gains RDY-PAY-001 and RDY-LIB-001; the Split gives RDY-LIB-001 (and `e2e/tests/library-gate.spec.ts`) to the parity agent; "Needs the owner" names the funded signet wallet; inventory rows under Wave 24; owner-checklist row "Fund a signet wallet and run RDY-PAY-001 once before the first paid invite".
- **T-03 (D-369), Admin > Runbook:**
  - `apps/web/src/admin/sections/Runbook.tsx`, a lazy chunk (`Runbook-*.js`, 14.9 KB gzip), section `runbook` under Platform, visible with OPERATOR like System; keys in `p23-runbook-1.ts`.
  - `deploy/RUNBOOK.md?raw` rendered by the restricted renderer `/help/whats-new` uses (`routes/entrance/entranceCopy.tsx` `EntranceMarkdown`). The renderer gained what the runbook needs and nothing raw: a `headingId` option (the runbook passes `runbookSlug`, a port of `scripts/lib/playbooks.mjs` `slug`), fenced code, block quotes, simple tables, `_emphasis_`, same-page `#anchor` links and the first number of an ordered list. Links to other files of the checkout stay text.
  - On mount and on a hash change (router or browser), the named heading scrolls into view and takes focus.
  - `System.tsx`: `PlaybookLink` is a router link to `/admin/runbook#<anchor>` derived from the `deploy/RUNBOOK.md#...` literal, same tab; `REPO_BLOB`, `target` and `rel` are gone; the literals and the check are unchanged (`check-ops-files.mjs` passes).
  - Tests: `admin/__tests__/p23runbook.test.tsx` (slug parity with `playbooks.mjs` over every heading of the file and edge cases; every heading id; all 22 `REQUIRED_PLAYBOOKS`; code, quote, table rendering; focus on the hash; hidden without OPERATOR; the five tile links); `p23playbooks.test.tsx` and `admin.test.tsx` (the Owner's nav) updated.
  - `deploy/OPERATOR.md` section 4 says Playbook opens the in-app copy, and the file is also in the install checkout.
  - Budget: admin 369.5 of 600 KB gzip; the runbook chunk is outside the route budget (lazy).
- **T-04 (D-370):** RDY-OBS-001 written into roadmap-p20 section 7 with the eleven gauges and the eight rules of D-370; the scale agent's Owns gain `observability.rs`, `alerts.yml`, `prometheus.yml` and `check-ops-files.mjs`; inventory row "RDY-OBS".
- **T-07 (D-373):** `settings/user/whatsNew.ts` and the dialog removed; the sidebar entry navigates to `/help/whats-new` (dropping the settings return target) and marks the version seen; the four `settings.whatsNew.*` keys removed from `p9-1.ts`; `prefsSections.test.tsx` asserts the navigation and the cleared badge.
- **T-08:** "Answered: P23-n (D-36x), follow-up T-nn" lines in `feat-p23-operator-handbook.md` and `feat-p23-first-run.md`. T-01..T-08 ticked with notes.
- **(a)** `deploy/OPERATOR.md` linked from `deploy/INSTALL.md`, `deploy/GO-LIVE.md`, `README.md` and `docs/README.md`, one line each.
- **(b)** `admin/sections/graph/GraphCanvas.tsx` honours reduced motion (the in-app setting wins over the OS one, as in ThemeProvider): it redraws synchronously on each change and never calls `requestAnimationFrame`; the layout is the static WASM one. Tests in `p15graph.test.tsx` (Settings "on"; OS reduce with "system"; animated frames otherwise).
- **(c)** RDY-SCALE-002 written (roadmap section 7 and inventory, scale agent): one `GET /users/@me/guilds/{id}/settings` per server on every load (`lounge/shell/notifSettings.ts`) can hit 429s; batch or load lazily. Not built.
- **(d)** `admin.reviewHub.tabLibrary` deleted (nothing referenced it; the Submissions tab replaced it).
- **(e)** `CHANGELOG.md` "Phase 23 - 2026-10-25" (the version the build now embeds).
- **Two test fixes found by the gates** (neither is a product bug):
  - `sr-voice` `tests/e2e.rs` sharded start: a parallel test can take *both* ports of the pair, and `start` then answers `NoUdpPort`, which the test unwrapped. It now retries that case like the shard-count case (`d86ce691`; 13 of 13 three times).
  - `e2e/tests/keyboard-6-voice.spec.ts` had never run against a live voice server. Once connected, the call view has a second "Mute" (a strict-mode double match that `tabTo` read as "not focused"); the toggle is renamed "Unmute" while muted; and 30 Tabs forward from it leave focus out of reach of Disconnect. The spec now takes the user panel's toggle, follows the rename (and checks `aria-pressed`), and reaches Disconnect with Shift+Tab (`4d835bcb`; 4 of 4).

## Gate results (merged tree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok (and on sr-voice after its test fix) |
| `cargo test --workspace -j 2 --no-fail-fast` (no database) | 2935 passed, 1 failed (the sr-voice port race above, fixed, then 13 of 13 x3), 533 ignored, 189 binaries |
| Postgres suites (`--include-ignored --test-threads=1`, own DB `sr_p23_int` as both `SR_TEST_DATABASE_URL` and `DATABASE_URL`) | **3469 passed, 0 failed**, 189 binaries. All 48 `sr-db` test files completed ok (`auth_repo` includes the first-run flag test). Authz matrix 2 of 2 (140 s) |
| `route_matrix`, `openapi_names`, `deploy_files`, `migration_versions` | 6, 2, 15, 1: all passed |
| `cargo deny --offline check` | advisories, bans, licences, sources ok. **Offline**, against the locally cached advisory database: an online run would fetch the advisory database, and this wave allows no downloads |
| `pnpm install --offline --frozen-lockfile`, wasm build | ok, ok |
| `pnpm -r typecheck` (e2e included), `pnpm -r lint` | ok |
| `prettier --check .` | tracked files ok; only git-ignored e2e artefacts flagged |
| i18n (`i18n-verify-diff main HEAD`) | MISSING 0 |
| `i18n-audit.mjs` | 0 in `routes/entrance`, `lounge/shell`, `settings`; 41 in the rest of the app (42 at the first-run branch) |
| `mocks:check` | up to date (971 operations) |
| licences (`pnpm licenses list --json --prod`), `check-notices-copy.mjs` | ok, identical |
| `secret-scan.mjs` (tracked and `--diff main`) | clean, clean |
| `node --test scripts/*.test.mjs` (threat model, secret scan, `check-ops-files.test.mjs` included) | 45 of 45 |
| `check-ops-files.mjs`, doc links, dependabot | ok (36 metric series), ok, ok (5 ecosystems) |
| `render-stickers --check` | 16 stickers |
| vitest apps/web `--maxWorkers=2` | run 1: 2597 of 2598 (the Owner nav list lacked Runbook: a real miss in my T-03 change, fixed); run 2: **298 files, 2598 of 2598**; run 3: **2598 of 2598** |
| package tests | api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 321, wasm 14, sdk-compat 3. markdown-react and ui first timed out (5 s test timeouts) while workspace clippy ran beside them; rerun alone, all green. Root cause: CPU contention, not the code; one heavy process at a time |
| web build and bundle budget | ok (entrance 232.4 of 300, lounge 430.1 of 600, admin 369.5 of 600, settings 299.5 of 600 KB gzip) |
| YAML: workflows, the `e2e-stack` action, `alerts.yml`, `prometheus.yml` (js-yaml) | 20 files parse |
| Playwright `tests/`, Chromium, fresh migrated DB `sr_p23_ie2e`, no `SR_E2E_*` flags (as the CI `playwright` job) | 72 passed, 22 skipped (live, voice, visual and relay specs), 1 failed: `keyboard-5-attachment` at "Uploaded", which needs Garage (CI) |
| Playwright live set, restarted server, `SR_E2E_VOICE/ACTIVITIES/LIVE=1` | 6 of 6 (voice-live, activities-live, message-components, keyboard-6-voice) after the keyboard-6 fix |
| Playwright `visual.spec.ts`, `SR_E2E_VISUAL=1` | 9 of 9 |
| Playwright `e2ee/voice-e2ee.spec.ts`, Chromium, restarted server | 1 of 1 |
| Playwright WebKit (Windows build): `a11y-signed-in`, `a11y-modes`, `keyboard-1..8` | 40 passed, 8 skipped (link-crossing keyboard specs; see "Resume here") |
| Playwright `test:static` | 19 passed |

The first Chromium attempt ran everything with all `SR_E2E_*` flags on one server: 82 passed, 8 failed. Four were the daily registration limit (20 x 201, then 15 x 429 in the server log), two `cinema-submit` and `keyboard-5` need Garage, and one was the keyboard-6 spec bug. Hence the CI-shaped reruns above.

## Manual checks (local, Owner, Vite without `VITE_SR_E2E` so the welcome is not suppressed)

- **First-run welcome:** after sign-in the dialog "Welcome to SIDE ROOM" opens: step 1 "Three rooms, one door", step 2 the look (Sync with computer, Dark, Light, Ash, Onyx, Side Room) and a profile picture, step 3 two-step sign-in ("Set it up here"), step 4 notifications ("Nothing will ask for permission now") with "Start exploring". Skip is on every step. No install step showed (the browser offered none). After "Start exploring" a reload does not show it again; Help > "Show the welcome again" reopens it.
- **`/help`:** the handbook with its nine sections, the welcome section, and the credits sentence read live ("every account starts with 5 invite credits"); links to Invites, Account and Notifications settings.
- **`/help/whats-new`:** shows "Phase 23 - 2026-10-25" first, then the earlier waves.
- **`/admin/runbook` and the tiles:** Admin > System on the local stack has three red tiles (no backup, no restore check, the watched disk). Their links are `/admin/runbook#backup-missing-or-stale`, `#restore-drill-failed`, `#disk-pressure`. Each opens Admin > Runbook in the same tab with that heading at the top and focused. The page has 29 heading ids, 4 code blocks and the settings table; an in-page link (`#lost-owner-second-factor`) moves and focuses the same way; the sidebar lists Runbook under Platform.
- Cosmetic: the disk tile's link is labelled "Playbook for ." on a dev server, because `ops.disk_path` is empty and the probe reports the working directory as `.`.

## Clean-up

- Stopped by PID, only processes of this pass: `sr-server` (runs 9512, 6940, 31812, 17732, 4000), the Vite dev server (6312, then 28852 without the e2e flag), `vite preview` (15964). No process was stopped by name; the Codex and godot `node.exe` processes were left alone.
- Dropped `sr_p23_int`, `sr_p23_ie2e`, and the leftover templates `sr_tpl_159_1240_13e1ca97` and `sr_tpl_160_1280_fe052043` (nothing connected). The `postgres` database was not touched.

## Open issues

- RDY-DOC-003: the playbook walk on the prod-smoke stack (above).
- Firefox and Linux WebKit for the a11y and keyboard specs, `keyboard-5` over Garage: first CI run.
- The registration throttle leaves the main Chromium suite 4 accounts of headroom (16 of 20 per server run). The next spec that registers accounts should use the `solo` fixture or the Owner, or the e2e server needs a test-only raise of the daily limit (an owner call; not changed here).
- RDY-SCALE-002 and RDY-OBS-001 are written, not built (wave 24).
- The a11y handoff's two moderate findings (the Lounge resize handle outside a landmark; the watch party "not available" state) are still open, with owners named there.
- CI has not run on this tree.
