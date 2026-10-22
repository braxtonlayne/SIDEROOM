# Handoff: integration/phase18 (wave 18 merges, integrator follow-ups, gates)

Main checkout `C:\Users\braxt\Desktop\SIDEROOM_repo`, branch `integration/phase18` (base = main `03fa69ed`). Local commits only: nothing pushed, no PR, nothing merged to main. Cargo target dir: `C:/Users/braxt/Desktop/SIDEROOM_wt/p18-integ-target`.

## Resume here

**State (2026-10-22): done.** All five Phase 18 branches are merged, Fable's D-337/D-338 answers included (`feat/p18-mod-restore` head `93db23cc`). The integrator items N-10, the D-324 case note, N-40 and N-41 are built, tested and ticked, and the `cinema.test.tsx` load flake is root-caused and fixed. Every gate below passes on the merged tree, except `cargo deny` advisories (three advisories published after main, see "Open issues"). No server, Vite or cargo process of this pass is running. The scratch databases `sr_p18_int` and `sr_p18_ie2e` and the two leftover `sr_tpl_*` templates were dropped (nothing else was connected).

**What is left for the owner or the coordinator:**

1. **cargo deny advisories (new, not caused by this wave):** RUSTSEC-2026-0381 (`webpki-roots`), RUSTSEC-2026-0382 (`rustls-webpki`, "may accept distrusted intermediates") and RUSTSEC-2026-0383 (`webpki-root-certs`). The fix is `cargo update -p rustls-webpki -p webpki-roots -p webpki-root-certs` (patched: `rustls-webpki >=0.103.16`, the two root crates `>=1.0.10`). That downloads crates, so it was not done; it needs the owner's OK. Main has the same lockfile entries, so main fails the same check today. Bans, licences and sources pass.
2. Push `integration/phase18` and open the PR when the owner agrees.
3. Still open by design: N-35 (mail the templated takedown reply, forward a counter-notice) waits on the lawyer (D-336 (e)); MOD-DSR-005 (flag the hold's case from an erasure that meets a hold), MOD-DSR-006 (restriction/objection tool) and MOD-DSR-007 (rectification) are not built; step 4 (dashboards) and step 5 (suspicion review) are later.

**How to rerun the browser tests locally:** as in `feat-p16-integration.md` (scratch DB migrated with `sr-cli migrate`, `sr-server` with `SR_STORAGE__ENDPOINT=memory:`, `SR_SERVER__PUBLIC_URL=http://localhost:5173`, `SR_VOICE__ENABLED=true`, `SR_ACTIVITIES__ENABLED=true`, `SR_ADMIN_REQUIRE_REAUTH=0`, log to a file named by `SR_E2E_SERVER_LOG`; Vite started directly with `VITE_SR_E2E=1 VITE_ACTIVITIES_ENABLED=true`; Playwright with `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1`, `SR_CLI_BIN`, `DATABASE_URL`). One addition: `test:static` starts `vite preview` through `pnpm`, which is not on PATH here; a `pnpm.cmd` shim (`npx -y pnpm@10.12.1 %*`) on PATH is enough.

## Merges (all `--no-ff`, in this order)

| Branch | Conflicts and resolution |
|---|---|
| feat/p18-modules | none |
| feat/p18-mod-dsr | `sr-db/tests/data_export.rs`: both assertion blocks kept (module settings, reactions and poll votes). Docs: see below. |
| feat/p18-mod-cases | `Privacy.tsx`: both cards (Your reports, Delete messages). `sr-api/routes/auth/tests.rs`: both tests (self-delete, appeal account). Spec item 6: dsr's text plus section 15 from cases. Docs: see below. |
| feat/p18-mod-tkd (based on an older mod-cases head) | Code, by hand: `AdminApp.tsx` (Moderation visible to `MANAGE_CASES`, `MANAGE_TAKEDOWNS`, `REVIEW_READING`, `REVIEW_MEDIA`, `DELETE_CONTENT`), `ModerationSection.tsx` (tabs per bit, below), `CaseQueue.tsx` (New case dialog plus the takedown source filter in the list key), `CasePage.tsx`, `labels.ts`, `moderation/api.ts` (union), `routes/moderation/cases.rs` (both query params), `sr-moderation` `lib.rs` and `service.rs` (guild audit, case actions, policy, removals and blocklist all on `Moderation`), `sr-server/app.rs` (one `Moderation` with every adapter). `routes/moderation/tests.rs`: rebuilt from the cases side with the takedown clerk as account 14, role 5, the takedown routes in the refusal list and the three takedown tests appended. Spec: step 1 and 2 "Built" lines, section 16 kept. |
| feat/p18-mod-restore (head `93db23cc`, D-337/D-338) | `routes/moderation/tests.rs`: the restore branch's `message_actions_from_a_case_reach_the_services` inserted after the actions test, takedown tests kept. Spec section 15: restore's bullets replace the old "Not built" line, section 16 kept. Docs: see below. |

- **Admin > Moderation, consistent:** one section in the People group. Tabs follow the bits: Cases for `MANAGE_CASES` and the reviewer bits (D-331), Holds and Appeals for `MANAGE_CASES`, Takedowns for `MANAGE_TAKEDOWNS`. A reviewer with no other bit sees the queue alone, as on the cases branch; a takedown clerk opens on Takedowns. **Data requests** is a group neighbour: its own page right after Moderation in the People group (it was alone in a group of its own at the end of the nav). The `admin.dataRequests.group` key went with it; the nav-order test was updated.
- **Docs:** decisions log sections in order of their first number (D-316..D-318/D-324..D-327/D-332 data requests, D-319..D-323 modules, D-328..D-331 cases, D-333..D-336 takedowns, D-337..D-338 restore); D-332 stays in the data-request section where its branch put it. `open-question-answers.md` and `answers-followups-p16.md` keep every section in the same order (N-01..N-15, N-10..N-12, N-20..N-27, N-30..N-35, N-40..N-41). M-25 is ticked (steps 1 to 3 are built; N-35 waits on the lawyer).
- **Inventory:** `19-moderation.md` gained MOD-DSR-001, 002, 003, 004 and 008 from the dsr handoff (005 to 007 stay open). Doc 13's Phase 7 line says steps 1 to 3 are built.
- **Generated files:** OpenAPI from the built `sr-server` (the auto-merged copy had lost dsr's `Delivered` schema and the case `source` parameter), the api-client schema, the MSW mocks (941 operations) and the route matrix (`SR_UPDATE_ROUTE_MATRIX=1`, no change, no `?`/`other`). `gatewayDocs.generated.ts` regenerated, unchanged. `KNOWN_CLASHES` empty.
- **Uniqueness:** migrations 1090, 1100 to 1103, 1120 and 1130 (modules used none); job ids 1105 and 1120 unique (`JOB_ID` constants checked for duplicates); audit actions 210 to 215 unique.

## Integrator work (commits `0e001534`, `697b3a12`, `69530422`)

- **N-10 (D-323):** `settings.channelTabs.featuresNote` deleted from `srvset-5.ts`; nothing referenced it.
- **D-324 case note:** `Moderation::note_member_self_delete` (`sr-moderation/src/cases.rs`) writes a system `member_self_delete` event, counts only (messages, reactions, poll votes, run id, who ran it), on every open case (`open`, `triage`, `investigating`) about the member by account or Person, and on the case that recorded an active platform ban. It is an event rather than a `moderation_case_notes` row because notes need an author account. `userset_jobs.rs` calls it after the access-log row, best effort. The case page renders it. Test: `a_self_delete_is_noted_on_the_members_open_cases_and_their_ban_case` (Postgres).
- **N-40 (D-337), task 2(d):** the interaction was real: a takedown uphold makes the message `staff_removed`, and platform staff could restore it from the case, bypassing the register. Now `Moderation::perform_action` asks `Moderation::message_takedown` first and answers 409 "The message was removed by takedown notice TK-n. Restore or withdraw the notice in the register." writing nothing. The check sits in `sr-moderation`, one layer above `ChatCaseActions` (which has no pool); the `staff_removed` platform rule in `validate_restore` is unchanged and its doc comment points up. The case page lifts that refusal out of the dialog onto the page with an "Open takedown notice n" button for `MANAGE_TAKEDOWNS`. Tests: Postgres `a_case_restore_refuses_a_message_a_takedown_holds` (uphold, 409 naming the notice, no event, register restore still works) and Vitest in `moderation.test.tsx`.
- **N-41 (D-338):** `hub.rs` sets `restored: true` on the `MessageRestored` dispatch only; reducer skips the mention bump and the channel `last_message_id` move; `desktopDecision`, `shouldAlert` and `DmToasts` return early; `Message.restored` typed; doc 05 section 6.3 documents the field. Tests: hub `a_restored_message_is_marked_on_its_message_create`, `lounge/__tests__/p18restored.test.tsx`, `gatewayDocs.test.ts`.
- **cinema.test.tsx flake ("deletes only after the title is typed"):** root cause, not a timeout. `usePaged.reload` empties the rows until the refetch answers. The test clicked the Merge button as soon as the DELETE reached the mock; under load the response landed and the reload started in between, so it clicked a row button the reload then unmounted and the merge dialog never opened. Reproduced by delaying only the post-delete reload by 300 ms: the old test failed every time with "Unable to find role=dialog /Merge Akira/", the new one passed. The test now waits for the "Ghost deleted." notice, the delete dialog to close and the Merge button to come back.
- **Bug caught by the gates:** the case page's refusal regex was written `TK-(d+)` by a scripted edit (the first full Vitest run failed the new test); fixed in `69530422` with the SQL line breaks of `message_takedown`.

## Gate results (merged tree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok (and `-p sr-moderation -p sr-server` again after `69530422`) |
| `cargo test --workspace -j 2 --no-fail-fast` | 2805 passed, 0 failed, 486 ignored (17.5 min) |
| Postgres suites (`--ignored --test-threads=1`, own DB `sr_p18_int`) | 486 of 486 passed (13.7 min); every `sr-db` test binary ran to completion (account_deletion through vectors); the new N-40 test again after `69530422`: passed |
| `migration_versions`, `route_matrix`, `openapi_names` | passed; route matrix unchanged |
| `cargo deny check` | **advisories FAILED** (RUSTSEC-2026-0381/0382/0383, new upstream, see above); bans, licenses, sources ok |
| `pnpm install --offline --frozen-lockfile`, wasm build | ok, ok |
| `pnpm -r typecheck`, `pnpm -r lint` | ok |
| `prettier --check .` | tracked files ok; only the three git-ignored local e2e artefacts are flagged, as in p17 |
| i18n (`i18n-verify-diff main HEAD`) | MISSING 0 |
| `mocks:check` | up to date (941 operations) |
| licence allow-list, `check-notices-copy.mjs` | ok, identical |
| vitest apps/web `--maxWorkers=2` | run 1: 1 failure (the new N-40 test, the regex bug above); after the fix, run 2: 273 files, 2463 of 2463 passed; run 3: 2463 of 2463 passed. `cinema.test.tsx` passed in all three |
| package tests | api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 279, wasm 14, sdk-compat `node --test` 3: all passed |
| `node --test` scripts (ci-scripts, install) | 15 passed |
| web build and bundle budget | ok (lounge 425.1 of 600 KB, entrance 228.8 of 300 KB, admin 356.0 of 600 KB) |
| `render-stickers --check`, doc links, ops files, dependabot | ok (16 stickers) |
| `deploy_files` | 13 passed |
| Playwright `tests/`, Chromium, fresh migrated DB `sr_p18_ie2e`, `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1`, win32 baselines | 35 passed, 1 skipped (K-22) in 3.7 min |
| Playwright `e2ee/voice-e2ee.spec.ts`, Chromium | 1 of 1 passed |
| Playwright `test:static` | 19 passed |
| Manual browser check, Admin > Moderation (Owner, local) | Cases, Holds, Appeals and Takedowns tabs; entered a test notice, which opened its case; the case page shows the "Copyright notice TK-n" badge; Data requests page next to Moderation renders its empty queue. The queue had no reported cases, so the case-with-report view was only checked by the Vitest suite. |

## Open issues

- cargo deny advisories (above): needs `cargo update` of three crates, a download, owner's call.
- Data requests is a neighbour page of Moderation, not a tab. A tab would mean moving the page under `ModerationSection` and gating the tab by `MANAGE_TAKEDOWNS`; say so if the owner prefers it.
- The D-324 note is a case event, not a note row (notes need an author). If staff want it in the Notes list, a system author account is needed.
- `i18n-audit.mjs` reports 456 hard-coded strings overall (informational, not a gate).
