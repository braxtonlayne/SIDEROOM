# Handoff: integration/phase15 (wave 15 merges, conflicts, fixes and gates)

Main checkout `C:UsersraxtDesktopSIDEROOM_repo`, branch `integration/phase15` (base = main `c65792cf`). Local commits only: nothing pushed, no PR.

## Resume here

**State (2026-10-18): done.** All nine branches are merged and every gate passes, Playwright included (results below). K-22 was measured and met after a fix: READY to first channel paint has a median of 1.16 s with 60 servers, against a 2 s target. No servers or test processes are running. The scratch databases `sr_p15_int` and `sr_p15_e2e`, and the migration template my runs created, were dropped.

**What is left for the owner or CI:**

1. Push and let CI run. The `live` job of `e2e.yml` was fixed here (`SR_VOICE__E2EE` is now set on its E2EE steps only, see fix 7). It has never run in CI.
2. **Do not flip the E2EE defaults.** They wait for a Firefox or WebKit pass in CI (D-249). Firefox does not launch on this host (`spawn UNKNOWN`), and Windows WebKit has no WebRTC, so the E2EE spec skips itself there.
3. Owner questions from the branch handoffs (below).

**How to rerun the browser tests locally:**

- Setup:
  - sr-server with `SR_STORAGE__ENDPOINT=memory:`.
  - Vite started directly: `node apps/web/node_modules/vite/bin/vite.js --host 127.0.0.1 --port 5173 --strictPort` with `VITE_SR_E2E=1`. `pnpm` is not on PATH, so Playwright's webServer command cannot start it; Playwright reuses a running server outside CI.
  - Restart sr-server between runs: the registration limits are 5 per minute and 20 per day per IP.
- Live run: voice and Activities on, **E2EE off** on the server.
- E2EE run: `SR_VOICE__E2EE=true`, and Vite with `VITE_SR_VOICE_E2EE=1`.
- K-22: `SR_E2E_K22=1`. A new account takes about 25 minutes, because server creation is rate limited. `SR_E2E_K22_USER=<username>` reuses an account that an earlier run filled.

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
6. **K-22: the page was busy about 46 s after READY with 60 servers.**
   - Measured in Chromium, and the CPU profile shows the cause. Each of about a hundred components reading a Lounge data hook rebuilt the whole snapshot in its own `useMemo`. That rebuild includes the WASM permission engine for every channel of every server. It happened once for each of the 60 PRESENCE_UPDATEs that follow READY (the person's own presence, once per server).
   - Fixes:
     - `lounge/live.tsx` keeps one snapshot shared by every hook instance, plus a per-server channel and permission cache. This took the busy time from 46 s to 4.6 s.
     - `store/connect.ts` collects PRESENCE_UPDATE into the READY burst. This brought READY to first paint down to 1.0 to 1.6 s.
     - `store/store.ts`: the deferred member and presence lists of a burst no longer overwrite a newer presence. This was a latent issue in K-22's deferral.
   - Tests: `readyManyGuilds.test.ts` (burst presences, stale-list protection) and `channelPerms.test.ts` (server reuse).
   - The `ready-many-guilds.spec.ts` e2e spec measures it. It waits out the rate limit and times READY by the socket frame event. A function-valued init script was dropped because Playwright's transform of it broke silently.
7. **CI `live` job vs K-21.** With `SR_VOICE__E2EE` set for the whole job, the server refuses the voice joins of the first step, whose web build has no E2EE flag. Locally, `voice-live` and `activities-live` failed with "This server requires encrypted calls; your browser cannot join." The flag now sits on the two E2EE steps; each step starts its own sr-server.
8. The follow-ups checklist (`answers-followups-p15.md`) records the K-21 and K-22 browser results.

## Gate results

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok (rerun after all Rust fixes) |
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
| vitest apps/web `--maxWorkers=2` | 2318 of 2318 passed (after every fix) |
| package tests | api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 279, wasm 14: all passed |
| web build + bundle budget | ok, every budget; lounge 410.5 of 600 KB gzip |
| i18n: `i18n-verify-diff.mjs main HEAD` | MISSING 0. `i18n-audit.mjs` TOTAL 427 is a heuristic count, not compared with main. |
| doc-links, ops-files, dependabot | ok |
| `node --test scripts/*.test.mjs` | 12 passed |
| Playwright `tests/`, Chromium | 19 passed, 6 skipped (the live and K-22 specs are gated) on a fresh DB. One failure in that run: `lounge-shell` friends registration hit the 5-per-minute registration limit; it passed on rerun together with `fixtures-scenario` (5 of 5). |
| Playwright `tests/`, WebKit | 12 passed, 14 skipped (Chromium-only specs), before and after the K-22 fix |
| Playwright, Firefox | does not launch on this host (`spawn UNKNOWN`) |
| Live: `voice-live`, `activities-live`, `message-components` (Chromium) | 5 of 5, twice (before and after the K-22 fix), with E2EE off on the server (fix 7) |
| `e2ee/voice-e2ee.spec.ts` (Chromium) | 9 of 10 over 3 batches. The observer declares `sr_e2ee`, sends `hello`, stays past the 10 s timeout, and every audio and video frame it sees carries the trailer. The one failure: the observer got no video frames within 30 s on a cold first run. WebKit: skipped (no WebRTC). |
| K-22 (Chromium, dev build, 60 servers) | READY to first channel paint 1.35, 1.18, 1.02, 1.60, 1.16, 1.13 s (median 1.16 s; before the fix about 46 s) |

## Overlaps checked

- **System-card closing.** browser's `close_card_at`, k's `rewrite_system_card` and the K-05 sweeper, and k2's PackageCard all compile together. The staff_feed Postgres tests pass, including the resume-invites and package-card tests.
- **Activities serving gate** (browser vs K-04): auto-merged; the sr-activities Postgres tests pass.
- **Voice E2EE observer** (browser hooks vs K-21): k2 was built on browser and keeps the observer declaring and sending `hello`. Verified in Chromium (see the E2EE row).
- **Lounge store** (K-22 READY coalescing vs browser's MESSAGE_UPDATE components and rest-c's party card): auto-merged; the store vitest is green in the full run.
- **Profile documents:** k2 contains devplat; no conflict.
- **Patron flag** on user objects: the workspace compiles, and the patron SQL test passes.

## Owner questions (new)

- None from the integration itself. The open questions in the branch handoffs stand: k2's package-card Approve button and profile-media grace, devplat's 9 decisions, rest-b's relink policy, JASSUB.
