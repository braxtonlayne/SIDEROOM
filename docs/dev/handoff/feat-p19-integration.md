# Handoff: integration/phase19 (wave 19 merges, integrator follow-ups, gates)

Main checkout `C:\Users\braxt\Desktop\SIDEROOM_repo`, branch `integration/phase19` (base = main `d6100cb1`). Local commits only: nothing pushed, no PR, nothing merged to main. Cargo target dir: `C:/Users/braxt/Desktop/SIDEROOM_wt/p19-integ-target`.

## Resume here

**State (2026-10-22): done.** The three Phase 19 branches are merged (`feat/p19-mod-dash`, `feat/p19-mod-sus`, `feat/p19-mod-age`). Both integrator tasks are done: (a) the `staff_feed_pg_tests::platform_events_become_cards_mail_and_revocation_pushes` flake is root-caused and fixed in the code, and (b) the retention interplay is made consistent and documented. Two cross-branch bugs the merge exposed are fixed as well. Every gate below passes on the merged tree, `cargo deny` included (main already has the RUSTSEC-2026-0381/0382/0383 lockfile fix from p18). No server, Vite, cargo, Vitest or Playwright process of this pass is running. The scratch databases `sr_p19_int` and `sr_p19_ie2e` and one leftover template, `sr_tpl_157_1161_7be7a166`, were dropped. Nothing else was connected to them. The `postgres` database was not touched.

**What is left for the owner or the coordinator:**

1. Push `integration/phase19` and open the PR when the owner agrees.
2. Migration **1152** is new, written by the integrator in the sus range (1150 to 1159). Fable reviewed it (D-349 to D-351); the follow-ups P-20 and P-21 are built on this branch (see "Integration round").
3. Still open from the branches, by design: item-page age buttons (Library series, Cinema title), "move a message" for a misplaced-NSFW report, mandatory external reporting (waits on the lawyer, D-336 h), and the lawyer checks named in D-341, D-343 and D-347. Also open: the Reports count in the Review hub, and search indexes that carry the age flag only after their next rebuild.

**How to rerun the browser tests locally:** as in `feat-p18-integration.md`. The scratch DB is migrated with `sr-cli migrate`. Start `sr-server` with these settings:

- `SR_DATABASE__URL` and `DATABASE_URL` pointing at the scratch DB
- `SR_STORAGE__ENDPOINT=memory:`
- `SR_SERVER__PUBLIC_URL=http://localhost:5173`
- `SR_VOICE__ENABLED=true`
- `SR_ACTIVITIES__ENABLED=true`
- `SR_ADMIN_REQUIRE_REAUTH=0`

Start Vite directly with `VITE_SR_E2E=1 VITE_ACTIVITIES_ENABLED=true`. Run Playwright with:

- `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1`
- `SR_CLI_BIN`
- `SR_E2E_SERVER_LOG`
- `DATABASE_URL`

The E2EE config needs `SR_E2E_VOICE=1` too, or it skips. `test:static` needs a `pnpm.cmd` shim on PATH (`npx -y pnpm@10.12.1 %*`).

## Merges (all `--no-ff`, in this order)

| Branch | Conflicts and resolution |
|---|---|
| feat/p19-mod-dash | none |
| feat/p19-mod-sus | `ModerationSection.tsx`: one rule for every tab (below). `moderation/api.ts`, `routes/moderation/mod.rs` (routes, paths, schemas), `route_matrix.tsv`: union. `routes/moderation/tests.rs`: both branches appended tests after the same test, so the file was rebuilt from the merged head plus each side's tail (14 + 3 + 4 = 21 tests). Spec: see below. Generated files: regenerated (below). |
| feat/p19-mod-age | `CasePage.tsx` (event placeholders), `labels.ts` (event keys), `moderation/api.ts`: union. `tests.rs` was rebuilt the same way (22 tests). Decisions log: age's "Phase 19 answers" (D-339 to D-343) before sus's "Phase 19 suspicion answers" (D-344 to D-348). `open-question-answers.md`: the "Phase 19 round" section (P19-1 to P19-5, dash and age) before the "Phase 19 suspicion round" (P19-SUS-1 to -5), each section with its own status table. `answers-followups-p16.md`: P-01 to P-06 before P-10 to P-16. **P-01 and P-02 ticked** (built on mod-dash, commits `97dc59e8` and `54e981af`). |

- **Admin > Moderation, consistent:** every tab follows one platform bit:
  - Dashboard: `MANAGE_CASES` or `MANAGE_TAKEDOWNS`. `PLATFORM_BAN` adds the ban figures; that bit alone still does not see the section (D-340).
  - Cases: `MANAGE_CASES` and the reviewer bits (D-331).
  - Holds and Appeals: `MANAGE_CASES`.
  - Signals: `MANAGE_ACCOUNTS` (P-16). `?tab=signals` deep-links to it from the Review hub.
  - Takedowns: `MANAGE_TAKEDOWNS`.

  A viewer with a single tab gets its content without a tab bar. That covers a reviewer's queue and `MANAGE_ACCOUNTS` alone. The default tab is signals when deep-linked, else cases, then takedowns, then signals. The section's `visible` in `AdminApp.tsx` is exactly the union of those bits. The Review hub was touched by the sus branch only (Signals tile); no conflict.
- **Spec section 17** is one section, "Implementation notes, P19":
  - 17.1 step 4, dashboards
  - 17.2 step 5, suspicion
  - 17.3 age tools and the last three data-subject rights
  - 17.4 how the purge jobs of the three meet (integration)

  Section 13 marks steps 3 to 5 as built. Dash's age-metrics note no longer says MOD-AGE-003 is unbuilt.
- **Inventory `19-moderation.md`:** the auto-merge kept every tick of the three branches (union checked: 80 ticked ids, none open on the merged file that a branch ticked). The MOD-DASH-009 note names the integration additions.
- **Generated files:** OpenAPI was regenerated from the built `sr-server`. The auto-merged copy was missing about 900 lines. The api-client schema and the MSW mocks (957 operations) were regenerated too. The route matrix (`SR_UPDATE_ROUTE_MATRIX=1`) did not change: no `?` or `other` cells. `gatewayDocs.generated.ts` is unchanged after prettier. `KNOWN_CLASHES` is empty.
- **Uniqueness:** migrations 1140, 1150, 1151, 1152 (new), 1160 and 1161 are unique, with no duplicate number prefixes in `crates/sr-db/migrations`. The 32 `JOB_ID` constants have no duplicates; 1150 is `moderation.suspicion_housekeeping`. Settings catalog keys have no duplicates. No branch added numeric audit action ids.

## Integrator work

- **Person links vs suspicion rows (task 1, "age touches account linking")** (`eb017f32`). The age branch made `link_in` (`sr-db/src/admin_repo_ext.rs`) re-point an emptied Person's cases, requests and takedown rows before deleting it. The sus branch added two things it did not know about. The first is `evasion_signals` (`person_id`, `related_person_id`, both FKs to `persons`), which would make every link of a Person that has a signal fail with an FK error. The second is the unique index of one open automatic suspicion case per Person, which fails when both Persons have one.
  - **Migration 1152** adds `evasion_signals.folded_at` and `folded_from_person_id`. A signal is allowed to become a pair of one Person only when folded, and the once-per-pair unique index applies to unfolded rows. The scan's `ON CONFLICT` names that predicate.
  - `link_in` now marks as folded (kept, never deleted) the signals between the two merged Persons and the ones that duplicate the target's for the same banned Person and kind. It then re-points both columns.
  - When both Persons have an open automatic case, the emptied Person's case becomes an ordinary case linked to the target's, with a `suspicion_folded` timeline event (new catalog fragment `p19-integ-1.ts`).
  - Suspicion events were already re-pointed.
  - Test: Postgres `a_person_link_carries_the_evasion_signals_and_automatic_cases` (`sr-db/tests/p13_settings_admin.rs`).
- **Staff age reset counted** (`b8082b8f`). The dashboard counts age-confirmation resets from `age_gate_resets` (dash, migration 1140), but the staff reset of MOD-AGE-003 (age branch) did not write it. It now inserts `staff` in the same statement as its reset and lock. Test: added to `sr-moderation/tests/age.rs`. Also verified in the browser: one `staff` row after the manual check.
- **(b) Retention interplay** (`d7e9633b`, `25066e27`). The sus branch's 180-day `login_events` purge (D-347) has no hold exception, so it never skips a row and cannot appear among the "rows skipped by a hold".
  - The retention check (MOD-DASH-009) now shows it separately:
    - the rows still older than the window, capped at 10 000 like the other counts and read on `login_events_created_idx`;
    - the count, time and backlog flag of the last run. `Moderation::purge_login_events` writes these on its own job row (`jobs.payload` of singleton 1150). `sr_moderation::HOUSEKEEPING_JOB_ID` is the shared constant, and `sr-server` asserts it equals its `JOB_ID`.
  - The check also shows the holds a deletion request met that still wait for staff (MOD-DSR-005, age branch).
  - Nothing in the age or dash branches reads old sign-ins. The dashboard counts none. The age tools do not use `login_events`. Exports are taken at run time, and a restriction does not stop storage limitation. The confirmed-signal snapshot has no address.
  - Docs: spec 17.4, the doc 09 job note and the inventory line. Tests: Postgres `the_retention_check_counts_what_a_hold_keeps_from_the_purge` (old sign-in counted, purge run, payload read back) and Vitest `p19dashboard.test.tsx`. In the browser, the dashboard showed the live job's first run ("Last run deleted 0 rows").
- **(a) The `platform_events_become_cards_mail_and_revocation_pushes` flake** (`1c21bcf9`). Root cause: `StaffFeed::on_alert` posted the #alerts card first, and only then read the Owner-email setting, took the per-kind throttle and sent the mail. That gave two races:
  1. The test waited for the third card and read the mailer at once. A mail still on its way failed `throttled to one mail per kind` (left 0).
  2. The first alert's handler could still read the setting after the test had turned the email on, and mail an alert that should not be mailed.

  Reproduced deterministically: with a mailer that takes 1 s and the old code and test, the test fails with exactly "left: 0, right: 1". The fix is in the code, not a longer sleep. The mail decision (setting and throttle) is taken before the card is posted, so a visible card means the decision is final. The mail is sent after the card, so a slow relay does not delay it. The test now waits for the mail explicitly and always runs with a 300 ms mailer, so the ordering is exercised on every run. The `staff_feed_pg_tests` suite passed three times in a row, then again in the full Postgres run.
- Small: an unused import in the new test (clippy), and the single-tab branch of `ModerationSection` reads `items[0]?.content` (`noUncheckedIndexedAccess`).

## Integration round (D-349 to D-351, P-20, P-21)

- **P-20:** `link_in` closes an open folded signal (self-pair `confirmed`, duplicate `dismissed` with "superseded by signal n"), no reviewer, snapshot or evidence; reviewed rows untouched. `SignalView.folded_from_person_id`; `signal_detail` skips the address read for a self-pair (privacy). "Folded (Person n)" tag on the signal rows (Signals tab and panel).
- **P-21:** the panel lists a demoted automatic case (`suspicion_folded` event), `SuspicionCaseRef.auto_kind` optional, `folded_into_case_id`; "folded into case n" links to the new `?case=n` deep link of Admin > Moderation.
- OpenAPI, the api-client schema and the mocks regenerated (957 operations). Catalog fragment `p19-integ-2.ts`.
- Gates after this round: `cargo fmt --check`; clippy `-D warnings` on sr-db, sr-moderation, sr-api, sr-server (all targets); Postgres `sr-moderation` suspicion (12) and dashboard and age, `sr-db` p13_settings_admin (7), `sr-api` moderation routes and `openapi_names`; full Vitest; web build and budget; Playwright Chromium suite. Results in the reply to the coordinator and the commit.

## Gate results (merged tree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok |
| `cargo test --workspace -j 2 --no-fail-fast` | 2824 passed, 0 failed, 516 ignored |
| Postgres suites (`--ignored --test-threads=1`, own DB `sr_p19_int`, TestDb clones) | 516 of 516 passed (about 20 min of test time); all 47 `sr-db` test binaries ran to completion (account_deletion through thread_repo) |
| `migration_versions`, `route_matrix`, `openapi_names`, `deploy_files` | passed (1+5 ignored, 5, 2, 13); route matrix unchanged |
| `cargo deny check` | advisories, bans, licences, sources ok |
| `pnpm install --offline --frozen-lockfile`, wasm build | ok, ok |
| `pnpm -r typecheck`, `pnpm -r lint` | ok (typecheck after `3c8727a2`) |
| `prettier --check .` | tracked files ok; only the three git-ignored local e2e artefacts are flagged, as in p17/p18 |
| i18n (`i18n-verify-diff main HEAD`) | MISSING 0 |
| `mocks:check` | up to date (957 operations) |
| licence allow-list, `check-notices-copy.mjs` | ok, identical |
| vitest apps/web `--maxWorkers=2` | run 1: 276 files, 2489 of 2489 passed; run 2: 2489 of 2489 passed |
| package tests | api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 279, wasm 14, sdk-compat `node --test` 3: all passed |
| `node --test` scripts (ci-scripts, install) | 15 passed |
| web build and bundle budget | ok (lounge 425.4 of 600 KB, entrance 229.1 of 300 KB, admin 363.0 of 600 KB) |
| `render-stickers --check`, doc links, ops files, dependabot | ok (16 stickers) |
| Playwright `tests/`, Chromium, fresh migrated DB `sr_p19_ie2e`, `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1` | 35 passed, 1 skipped (K-22) in 3.3 min |
| Playwright `e2ee/voice-e2ee.spec.ts`, Chromium | 1 of 1 passed |
| Playwright `test:static` | 19 passed |
| Manual browser check, Admin > Moderation (Owner, local) | See below. |

The manual check covered:

- **Tabs:** Dashboard, Cases, Holds, Appeals, Signals and Takedowns all show for the Owner.
- **Dashboard:** every section renders, including the new retention rows and the live sign-in purge line.
- **Signals:** shows its empty state.
- **Person drawer:** the suspicion panel (level, thresholds, history, banned descendants) renders for a test account.
- **Age restriction:** opened a case from "New case", then took "Age restriction" > under 18. The timeline shows the underage event, the account is locked, and `age_gate_resets` has one `staff` row.

The check did not cover signals with data (none were raised on a fresh DB), the channel kind of the age restriction (covered by the Postgres and route tests), or the under-13 path.

## Open issues

- ANSWERED (D-349, D-350; P-20 done): migration 1152 and the fold rule stand; a folded signal still open is closed by the fold (self-pair `confirmed`, duplicate `dismissed` naming the survivor), folded rows are marked "Folded (Person n)", and the detail of a self-pair reads no addresses.
- ANSWERED (D-351; P-21 done): a demoted automatic case stays in the panel with "folded into case n", a link to `/admin/moderation?case=n`.
- The sign-in purge figures read the job row, so they show nothing until the job has run once ("The purge job has not run yet.").
- `i18n-audit.mjs` (informational) was not rerun.
