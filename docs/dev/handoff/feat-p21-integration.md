# Handoff: integration/phase21 (wave 21 merges, integrator work, gates)

Main checkout `C:\Users\braxt\Desktop\SIDEROOM_repo`, branch `integration/phase21` (base = main `0cae4005`, PR #46). Local commits only: nothing pushed, no PR, nothing merged to main. Cargo target dir: `C:/Users/braxt/Desktop/SIDEROOM_wt/p21-integ-target`.

## Resume here

**State (2026-10-23): done locally.** The three Phase 21 branches are merged. The integrator tasks (a) to (d) and R-07 (D-366) are built and tested. Every local gate below passes on the merged tree. No server, Vite, cargo, Vitest or Playwright process of this pass is running. The scratch databases `sr_p21_int` and `sr_p21_ie2e` were dropped, and so were the leftover templates (see Clean-up). The `postgres` database was not touched.

**What is left for the owner or the coordinator:**

1. Push `integration/phase21` and open the PR when the owner agrees. CI has not run on this tree.
2. R-04 and R-05 (D-360) are queued for a later wave by design: the allow side of write routes, and seeding stages, stickers, sounds and announcements in the matrix.
3. The CSP report delivery from an ordinary browser is still the hand check of D-365 (owner checklist). Here the server side was checked through the Vite proxy with a posted report, not with a browser-sent one.

**How to rerun the browser tests locally:** as in `feat-p20-integration.md`, with one change: `test:static` needs no `pnpm.cmd` shim. Start `vite preview` yourself with the local binary (`node apps/web/node_modules/vite/bin/vite.js preview --host 127.0.0.1 --port 4173 --strictPort`), and the static config reuses it. That avoids `npx pnpm@...`.

## Merges (all `--no-ff`, in this order)

| Branch | Conflicts and resolution |
|---|---|
| feat/p21-threat-refresh (`3c16e94a`) | none |
| feat/p21-sessions-uploads (`126891a4`) | `20-launch-readiness.md`: union of the ticks (RDY-SEC-004..006 from one side, 007..010 from the other) |
| feat/p21-authz-matrix (`79b93512`, head "docs(p21): answer D-366") | `route_matrix.tsv` (authz side taken, then regenerated); decisions log; doc 05; `answers-followups-p16.md`; `open-question-answers.md`; the inventory |

How each conflict was resolved:

- **Decisions log:** one "Phase 21 answers" section with D-359 to D-366 in order. The sessions-uploads rows D-362 to D-365 had been appended to the Phase 20 table and were moved. The intro names both handoffs.
- **`answers-followups-p16.md`:** one "Phase 21 round" heading with both intro paragraphs, R-01 to R-07 and then R-10 to R-13.
- **`open-question-answers.md`:** two rounds, kept as separate sections (authorization matrix with P21-1 to P21-3 and P21-8; sessions and uploads with P21-4 to P21-7). Each has its own count table, and the P21 ids are unique.
- **Doc 05:** both additions, the abuse-limits table (section 5.3) and then section 5.4 "How authorization is tested".
- **`rate_limit.rs` / `router.rs` / `sensitive.rs`:** merged without conflict. The layers are `rate_limit`, then `reauth_gate`, then `origin_check`. `/internal/csp-report` sits outside `/api/v1` and has its own limiter.
- **Generated files**, regenerated on the merged tree:
  - OpenAPI from the built `sr-server`, the api-client schema, and the MSW mocks (960 operations): no change from the auto-merge.
  - `gatewayDocs.generated.ts`: unchanged.
  - The route matrix (`SR_UPDATE_ROUTE_MATRIX=1`): see the R-07 notes below. No `?` or `other` cells.
  - `KNOWN_CLASHES` is empty.
- **Uniqueness:**
  - Migration 1230 (`1230_csp_reports.sql`) is the only new one, and no number prefix repeats.
  - No new `*_JOB_ID` constants.
  - The settings catalog has 11 new `abuse.*` keys with no duplicates.
  - No numeric audit action ids were added.

## Integrator work

- **(a) Threat model.** T7 now names `crates/sr-server/tests/authz_matrix.rs::every_route_enforces_its_authz_row` and the two grammar tests. Its residual lists R-04 and R-05 as queued follow-ups, not as open. T2, T4, T9, T10, T11 and T20 still said "open" for RDY-SEC-007 to 010, which the sessions-uploads branch closed. Each of these rows now names the test that proves it:
  - the drills;
  - the CSP report tests;
  - the upload-hygiene tests;
  - the reauth list;
  - the developer-portal password check.

  `scripts/threat-model.test.mjs`: 5 of 5. The abuse-limits doc test (`abuse_limits.rs::doc_05_table_matches_the_catalog_and_its_tests_exist`) passes.
- **(b) Abuse limits and Playwright.** The Chromium suite passes (35 passed, 1 skipped). The only 429s in the server log are 8 on `POST /sr/register`. That is the phase-16 registration throttle, which the e2e helpers retry by design. None of the new limits answered 429.
- **(c) msgActions vitest failures, root-caused** (`apps/web/src/lounge/__tests__/msgActions.test.tsx`):
  - **Cold start.** The file rendered the lazy Lounge route without `warmLoungeRoute()`. Run alone, the first test spent 5.5 s of its 5 s `findBy` budget on the module transform. The file now calls `warmLoungeRoute()`, and the first test takes 1.7 s alone.
  - **"MSW unhandled request" failure.** The real cause is a timing race in "lists members, roles and channels in the select menus". The test asserts `aria-busy` right after `userEvent.selectOptions`, with `interactionConfig.timeoutMs = 60`. Under load the 60 ms wait runs out first, so the control is already idle. It was reproduced deterministically with a 2 ms wait ("expected aria-busy=true"). The `[MSW] Error: intercepted a request without a matching request handler` lines are stderr noise from unmocked Lounge side requests (announcements, donations, guide...), printed in every test of the file. They are not the cause.
  - **Fix.** A new helper, `apps/web/src/test/interactions.ts` `holdInteractions()`, sets a long wait and captures the `POST /interactions` nonce. Its `fail()` delivers `INTERACTION_FAILURE` through `emitDispatch`, the path the gateway takes, and the previous wait is restored by `onTestFinished`. `messages.test.tsx` "renders app components" had the same race on the Approve button and uses the helper too.
- **(d) Authz matrix on the merged tree.** `sr-server` boots the new routes from sessions-uploads and the limits from threat-refresh.
  - The matrix now also lifts every catalog setting of group `abuse` to its maximum (the data-export cooldown to its minimum), next to the existing `limits.rate.*` lift. About twenty actors share 127.0.0.1, a webhook token's bucket and the per-address interaction-callback bucket. `abuse_limits.rs` proves the limits themselves.
  - Result: 22,616 calls, 0 failures, no 429 in the report. Every group was proven, in this order: moderation, admin, developers and applications, oauth2, submissions, rest.
  - Not checked: a run without the lift, to show it is needed.
- **R-07 (D-366), the integrator's since the branch was finished:**
  - `crates/sr-api/tests/support/platform_cell.rs` has `parse_platform`, returning `One`, `All` or `Any`. It refuses a mixed cell, a repeated name, an empty part, and `OWNER` or `STAFF_BASIC` inside a compound. Both test suites include it through `#[path]`, with no crate dependency.
  - `route_matrix.rs::platform_cell_grammar` and `authz_matrix.rs::admitted_grammar` pin the accepted and refused forms.
  - **Found on the way:** the `SR_UPDATE_ROUTE_MATRIX=1` writer still wrote six columns, so regenerating would have erased PLATFORM. It now writes the seventh column (the old value, or `-` for a new row), and the header names the cell forms.
  - TSV changes:
    - `GET /admin/system/csp-reports` is `staff` with `OPERATOR` (the handler authorizes `P::OPERATOR`);
    - `/internal/csp-report` is listed as undocumented;
    - the two merge rows become `PUBLISH_MEDIA+DELETE_CONTENT` and `PUBLISH_READING+DELETE_CONTENT`.
  - The sweep of two-check service functions in `sr-cinema`, `sr-library` and `sr-tv` found only the merges and the unpublish toggles (already `A|B`).
  - `WEAK_CEILING` admin goes from 27 to 28. The weak refusals there are one per appointed actor on `publish-now`, and the merge rows added the pair actor `staff_publish_media_and_delete_content`.
  - Doc 05 section 5.4 now says the forms are "never mixed".
- **Doc 12 tables.** The R-10..R-13 commit on sessions-uploads had padded every doc 12 table (a formatter run; `*.md` is prettier-ignored). The four `the_doc_table_matches_the_code` tests (sr-chat, sr-library, sr-packages, sr-storage) expect compact rows, so all four failed on the merged tree. The tables are compact again; only whitespace changed.

## Gate results (merged tree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok |
| `cargo test --workspace -j 2 --no-fail-fast` (no database) | 2919 passed, 4 failed (the doc 12 tables, fixed and rerun: 4 of 4), 528 ignored |
| Postgres suites (`--include-ignored --test-threads=1`, own DB `sr_p21_int` as both `SR_TEST_DATABASE_URL` and `DATABASE_URL`) | 3451 passed, 0 failed across 189 test binaries; all 48 `sr-db` binaries completed; authz matrix 2 of 2, sr-cli drills 3 of 3, csp_reports 2 of 2 |
| Authz matrix alone (twice) | first run 1 failure (admin weak ceiling, see R-07); second run green, 97 s |
| `migration_versions`, `route_matrix`, `openapi_names`, `deploy_files`, `abuse_limits` | 1, 6, 2, 15, 4 passed |
| `cargo deny --offline check` | advisories, bans, licences, sources ok (against the locally cached advisory database; nothing fetched) |
| `pnpm install --offline --frozen-lockfile`, wasm build | ok, ok |
| `pnpm -r typecheck`, `pnpm -r lint` | ok |
| `prettier --check .` | tracked files ok; only the git-ignored e2e artefacts are flagged |
| i18n (`i18n-verify-diff main HEAD`) | MISSING 0 |
| `mocks:check` | up to date (960 operations) |
| licences (`pnpm licenses list --json --prod \| check-licenses.mjs`), `check-notices-copy.mjs` | ok, identical |
| `secret-scan.mjs` (tracked and `--diff main`), `secret-scan.test.mjs` | clean, clean, 13 of 13 |
| `threat-model.test.mjs` | 5 of 5 |
| `node --test scripts/*.test.mjs` | 35 of 35 |
| vitest apps/web `--maxWorkers=2` | run 1: 283 files, 2507 of 2507; run 2: 2507 of 2507 |
| package tests | api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 279, wasm 14, sdk-compat 3: all passed |
| web build and bundle budget | ok (lounge 425.4 of 600 KB, entrance 229.0 of 300 KB, admin 363.8 of 600 KB) |
| `render-stickers --check`, doc links, ops files, dependabot | ok (16 stickers) |
| YAML: workflows and the `e2e-stack` action (js-yaml) | 15 files parse |
| Playwright `tests/`, Chromium, fresh migrated DB `sr_p21_ie2e`, `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1` | 35 passed, 1 skipped (K-22) in 3.9 min |
| Playwright `e2ee/voice-e2ee.spec.ts`, Chromium | 1 of 1 passed |
| Playwright `test:static` | 19 passed |

**Manual browser check (Owner, local, Chromium pane):**

- **CSP reports card** ("Blocked content reports", Admin > System). It read "No reports yet" at first. Then a report was posted to `POST /internal/csp-report` through the Vite proxy (204), with the document URL `/channels/@me?x=1`. After that, the card listed `img-src`, `https://blocked.example.test` and `/channels/@me`, each with 1 report. The table stored the path without the query. A second report and "Refresh counters" moved `img-src` to 2.
- **Re-auth prompt.** With `sr-server` restarted without `SR_ADMIN_REQUIRE_REAUTH=0`, publishing an announcement (a listed sensitive action) opened "Confirm it is you": "This admin action needs a second factor...". The test Owner has no second factor, so the dialog said to set one up first, and nothing was published. The code-entry path itself is covered by `p21withReauth.test.ts` and `reauth_list_tests.rs`; it was not entered by hand.
- **Admin > Limits:** the "Abuse limits" group lists the eleven `abuse.*` settings with their ranges and "Using the default".

## Clean-up

- Stopped by PID, only processes of this pass: `sr-server` (two runs), the Vite dev server and the `vite preview` server.
- Dropped `sr_p21_int` and `sr_p21_ie2e`. The leftover templates `sr_tpl_157_1161_7be7a166` and `sr_tpl_158_1230_b23a52a3` were dropped; nothing was connected to them. The `postgres` database was not touched.

## Open issues

- R-04 and R-05 (D-360): queued for later waves, as decided.
- CI has not run on this tree. `repo-checks.yml` gained the secret-scan steps; the PR's diff scan runs only on a pull request.
- The MSW "unhandled request" stderr noise in the Lounge tests is harmless but hides real messages. A shared handler set for the Lounge side requests (announcements, donations, guide, onboarding, appearance, scheduled events) would quiet it. Not done here because it touches many test files.
