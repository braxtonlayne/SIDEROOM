# Handoff: integration/phase15 (wave 15 merges, conflicts, fixes and gates)

Main checkout `C:\Users\braxt\Desktop\SIDEROOM_repo`, branch `integration/phase15` (base = main `c65792cf`). Local commits only: nothing pushed, no PR. The owner asked to shut down before Playwright, so this handoff is a stopping point, not the finish.

## Resume here

**State:** all nine branches are merged. Every Rust and repo gate passed. The web gates passed except the final confirmation run of the full web vitest suite: it was stopped by the shutdown, and its first run had 1 failure, which is now fixed. Playwright and the K-22 measurement have **not** been run. No servers or test processes are running. Scratch database `sr_p15_int` and the template it produced (`sr_tpl_139_1021_e5c7de15`) were dropped.

**Next steps, in order** (one cargo or test process at a time, `-j 2`, `CARGO_PROFILE_DEV_DEBUG=0`):

1. Full web vitest: `cd apps/web && npx vitest run --maxWorkers=2`. Expect 2316 tests, all passing. Then run the package tests: `pnpm -r --filter "./packages/*" test`. They have not been run in this pass.
2. Web build plus budget: `pnpm --filter @sideroom/web build && node scripts/check-bundle-budget.mjs`. Not run in this pass; the wasm dist is freshly built.
3. Playwright, against a fresh scratch DB. Use `storage.endpoint = "memory:"`, since there is no Garage on Windows. Restart sr-server between long runs: registration is limited to 20 per day per IP, held in memory.
   - a. `tests/` on chromium, then webkit. Firefox does not launch on this host.
   - b. The live run, with `SR_VOICE__ENABLED=true`, `SR_VOICE__E2EE=true`, `SR_ACTIVITIES__ENABLED=true`, `SR_ADMIN_REQUIRE_REAUTH=0`, `VITE_ACTIVITIES_ENABLED=true`, `SR_E2E_VOICE=1`, `SR_E2E_ACTIVITIES=1` and `SR_E2E_LIVE=1`. Run `tests/voice-live.spec.ts`, `tests/activities-live.spec.ts` and `tests/message-components.spec.ts` on chromium.
   - c. `-c playwright.e2ee.config.ts --project=chromium` (same server env; the config sets `VITE_SR_VOICE_E2EE=1`). This is the first run with K-21: the observer must still declare `sr_e2ee` and send `hello`, or it is dropped after 10 s.
   - d. K-22: `SR_E2E_K22=1 ... playwright test tests/ready-many-guilds.spec.ts --project=chromium`. The spec is new and unrun (committed as `wip:`). It creates 60 servers for a fresh account, then times READY to the first paint of the channel's message in the page, over 3 loads. It prints the numbers, and the median must be under 2 s. Record the result here.
4. **Do not flip the E2EE defaults.** They wait for a Firefox or WebKit pass in CI (D-249).

## Merges (all `--no-ff`, in this order)

| Branch | Conflicts and resolution |
|---|---|
| feat/p15-answers | none |
| feat/p15-jassub | none. Docs only: JASSUB is blocked on GPL-3.0 `rvfc-polyfill`. Kept. |
| feat/p15-rest-a | none |
| feat/p15-browser | none |
| feat/p15-devplat | none |
| feat/p15-rest-b | none |
| feat/p15-rest-c | `docs/05` table: kept devplat's Packages and Profile pages rows, plus rest-c's Application commands row. `answers-followups-p15.md`: HEAD's text with rest-c's ticks for K-14 and K-19. MSW handlers regenerated. |
| feat/p15-k | `useAppliedTheme.ts` and `UserSettings.tsx`: both sides combined (Theme Store overlay from devplat, platform default themes from K-12). `route_matrix.tsv`: kept both rows. `10-admin.md` and the follow-ups were tick-unioned. **Semantic fix in the merge commit:** `party_cards.rs` (rest-c) called `rewrite_system_card` without the `content` argument that K-05 added; it now passes `""`, which is the content the card is posted with. OpenAPI, client and mocks regenerated (no diff). |
| feat/p15-k2 | `staff_feed_pg_tests.rs`: kept both new tests. MSW handlers regenerated. |

Migrations after the merges: 0970, 0971, 0990, 1000, 1001, 1020, 1021, all unique (139 files in total).

## Fixes after the merges (commits on integration/phase15)

1. **0990 (rest-b) vs 1021 (k2), a real conflict.** Both added `external_identities.unlinked_at`, so 1021 would have failed on any database. 1021 is not on main, so it was edited: it now adds only `moved_to_account_id`. The two meanings are reconciled:
   - a person's own unlink keeps holding the identity (D-030);
   - a staff move leaves a history row.

   Code changes:
   - `link_discord` picks the holder: the live row, or else the newest row that was not moved. It relinks that row only. Before, it updated every row of that Discord id, which would violate the new partial unique index after a move.
   - Registration matching sees soft-unlinked rows again. k2 had filtered them out, which reopened the ban-evasion path rest-b closed.
   - The staff account search skips moved history rows instead of every unlinked row.
   - New Postgres test `p15_rest_b::a_moved_identity_belongs_to_the_account_it_moved_to`.
2. **Item (a), `messages.test.tsx` "renders attachments, voice, ...". Root cause:** the Lounge is a lazy route. The first render in a test file waits for the cold transform of the whole Lounge module graph (measured 9.3 s with a cargo build running), which uses up the 5 s `findBy` budget. Later tests in the file are fast. **Item (b)**, voiceUi's first test and lounge shell's "renders rail", is the same mechanism. Fix: `src/test/warmRoutes.ts` `warmLoungeRoute()` imports `routes/LoungeRoot` in a `beforeAll` with a 120 s hook timeout. It runs after any env stubs, in the 23 test files that render `/channels` through the router (`firstpaint.test.tsx` is left out on purpose). After the fix the first test takes 1.4 s in the full run.
3. **Item (c), the p13entrance hero upload flake. Root cause found:**
   - The failing `/admin/me` fetch never reached MSW (no `request:start`). It failed with ECONNREFUSED on localhost:3000.
   - MSW 3 intercepts at the socket and attributes a socket to the fetch whose async context opened it. Under load, undici's pool sometimes opens one outside that context, and the request goes to the real network.
   - Fix: `src/test/setup.ts` resends a body-less request that failed this way (twice at most). Nothing listens on localhost:3000 under vitest, so such a request never reached a handler.
   - The MANAGE_FEATURES test also answers the dashboard's requests now.
   - Result for the admin suite: 5 of 5 runs green after the fix; before it, failures in 2 of 4 runs.
4. **`p15graph` canvas test** (rest-c) flaked in 1 of 3 runs: `layoutGraph` runs in an effect that can land after the canvas is in the DOM. The assertion now waits.
5. **`gatewayDocs.generated.ts`** was stale after the doc 05 merges (the op 8 note). Regenerated with `node scripts/lib/gateway-docs.mjs`.

## Gate results

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok (run before fixes 1 to 5; fix 1 touched sr-db only, and the sr-db tests compile with it) |
| wasm32 `cargo check --tests`, all six `crates/*-wasm` | ok |
| OpenAPI, client and MSW regenerated after each conflicting merge | no diff; `mocks:check`: 880 operations up to date |
| `route_matrix` (regenerated with `SR_UPDATE_ROUTE_MATRIX=1`) | 5 passed, no diff, no `?` or `other` cells |
| `openapi_names` | 2 passed, `KNOWN_CLASHES` empty |
| `cargo test --workspace -j 2 --no-fail-fast` | 2657 passed, 0 failed, 419 ignored |
| Postgres suites (13 crates with ignored tests, `--include-ignored --test-threads=1`, fresh scratch DB) | 1505 passed, 0 failed. The 7 S3 and 3 Esplora tests return early without `SR_TEST_S3_ENDPOINT` or `SR_TEST_ESPLORA_URL`, so they were not exercised. |
| `cargo deny check` | advisories, bans, licenses and sources ok |
| `pnpm install --offline --frozen-lockfile` | ok; apps/web runs vitest 3.2.7 (the packages pin vitest 2, as on main) |
| wasm build | ok |
| `pnpm -r typecheck`, `pnpm -r lint` | ok |
| `prettier --check apps packages docs scripts deploy` | ok |
| vitest apps/web `--maxWorkers=2` | 2315 of 2316 passed; the failure was `gatewayDocs` (fix 5). The rerun was stopped by the shutdown, so it is **unconfirmed**. |
| package tests, web build and bundle budget | **not run** |
| i18n: `i18n-verify-diff.mjs main HEAD` | MISSING 0. `i18n-audit.mjs` TOTAL 427 is a heuristic count, not compared with main. |
| doc-links, ops-files, dependabot | ok |
| `node --test scripts/*.test.mjs` | 12 passed |
| Playwright, K-22 | **not run** |

## Overlaps checked

- **System-card closing.** browser's `close_card_at`, k's `rewrite_system_card` and the K-05 sweeper, and k2's PackageCard all compile together. The staff_feed Postgres tests pass, including the resume-invites and package-card tests.
- **Activities serving gate** (browser vs K-04): auto-merged; the sr-activities Postgres tests pass.
- **Voice E2EE observer** (browser hooks vs K-21): k2 was built on browser and keeps the observer declaring and sending `hello`. Not verified in a browser (step 3c).
- **Lounge store** (K-22 READY coalescing vs browser's MESSAGE_UPDATE components and rest-c's party card): auto-merged; the store vitest is green in the full run.
- **Profile documents:** k2 contains devplat; no conflict.
- **Patron flag** on user objects: the workspace compiles, and the patron SQL test passes.

## Owner questions (new)

- None from the integration itself. The open questions in the branch handoffs stand: k2's package-card Approve button and profile-media grace, devplat's 9 decisions, rest-b's relink policy, JASSUB.
