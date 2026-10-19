# Handoff: integration/phase16 (wave 16 merges, conflicts, fixes and gates)

Main checkout `C:\Users\braxt\Desktop\SIDEROOM_repo`, branch `integration/phase16` (base = main `1b852677`). Local commits only: nothing pushed, no PR, nothing merged to main.

## Resume here

**State (2026-10-19): done.** All five Phase 16 branches are merged, `feat/p16-backend` included (it was finished during this pass and merged last). Every gate passes on the fully merged tree, Playwright included (results below). No servers or test processes of this pass are running. The scratch databases `sr_p16_int` and `sr_p16_ie2e` were dropped.

**What is left for the owner or CI:**

1. Push `integration/phase16` and let CI run. New CI from this wave that has never run on GitHub:
   - `rust.yml`: the `sr-sdk` no-default-features steps (D-268, L-13). They passed locally: clippy `-D warnings` and 39 tests.
   - `e2e.yml`: the `visual-baselines` dispatch job and the `.github/actions/e2e-stack` composite action (L-20). The first linux baselines need a manual dispatch and a commit of the artifact; L-21 and L-22 wait on that.
   - The two workflow files touch different files and do not clash. All three YAML files parse.
2. Owner questions from the branch handoffs stand:
   - the `/support` donation copy (owner item 12);
   - ffmpeg on a development machine (human action 5, D-269);
   - JASSUB from earlier rounds.
3. Not verified anywhere: the chat multipart upload against real S3, and the composer reattach over the network. Local storage is `memory:`, which has no multipart. The IndexedDB ledger itself was checked in a real Chromium (see the gate table).

**How to rerun the browser tests locally** (same as phase 15, plus the live flags):

- Database and server:
  - Use a scratch database, migrated with `sr-cli migrate`.
  - Start `sr-server` with:
    - `SR_STORAGE__ENDPOINT=memory:`
    - `SR_SERVER__PUBLIC_URL=http://localhost:5173`
    - `SR_VOICE__ENABLED=true`
    - `SR_ACTIVITIES__ENABLED=true`
    - `SR_ADMIN_REQUIRE_REAUTH=0`
  - Send the server log to a file and point `SR_E2E_SERVER_LOG` at it. `entrance-journey` reads mailed links from it.
- Vite, started directly: `node apps/web/node_modules/vite/bin/vite.js --host 127.0.0.1 --port 5173 --strictPort` with `VITE_SR_E2E=1` and `VITE_ACTIVITIES_ENABLED=true`.
- Run Playwright with:
  - `SR_E2E_VOICE=1`, `SR_E2E_ACTIVITIES=1`, `SR_E2E_LIVE=1` and `SR_E2E_VISUAL=1`;
  - `SR_CLI_BIN` set to the built `sr-cli.exe`;
  - `DATABASE_URL` set to the scratch database.
- Use a fresh database and a fresh server for each full run:
  - The `exit-gate` bootstrap test skips itself once the Owner exists.
  - Registration also allows only 20 accounts a day per address. That limit is in memory, so a server restart resets it.

## Merges (all `--no-ff`, in this order)

| Branch | Conflicts and resolution |
|---|---|
| feat/p16-lounge | none |
| feat/p16-ext (based on lounge `f2688762`) | Docs only. `01-decisions-log.md` and `open-question-answers.md`: each branch appends its own section at the same place. Rebuilt as main plus every branch's block, in D-number order (script, no hand edits). `answers-followups-p16.md` (add/add): one shared header, then each branch's round in L-number order, each keeping its own introduction. Its headings moved down one level. Inventory 06: lounge's LNG-SSET-051 line (D-264 wording) and ext's LNG-SSET-050 tick. |
| feat/p16-uploads | Same three docs, rebuilt the same way. `open-questions.md` auto-merged (owner item 12). |
| feat/p16-e2e | Same three docs. |
| feat/p16-backend (head `c0299f29`) | Same three docs. Backend's D-266 to D-269 and D-285 to D-286 sit as its own section after lounge's. Its L-10 to L-17 "Backend" round sits between lounge and e2e. `open-questions.md` auto-merged (human action 5). |

**Order of the merged docs:**

- Decisions: lounge (D-264, D-265), backend (D-266 to D-269, D-285, D-286), uploads (D-270, D-271), e2e (D-275), ext (D-280 to D-283).
- Follow-ups: L-01, L-10, L-20, L-30, L-40.

**Generated files:** after the four-branch merge and again after the backend merge, these were all regenerated from the merged tree:

- OpenAPI (`cargo run -p sr-server -- openapi`);
- the api-client schema and the MSW mocks (886 operations);
- the route matrix (`SR_UPDATE_ROUTE_MATRIX=1`);
- `gatewayDocs.generated.ts`.

The auto-merged copies were already identical each time. The only diff was the one from the D-282 fix.

**Migrations after the merges:** 1030 (uploads), 1040 (backend) and 1060 (ext). All 142 files are unique.

**i18n fragments:** `p16-lounge-1..4`, `p16-ext-1..2` and `p16-uploads-1`. File names are distinct, and `i18n-verify-diff main HEAD` reports MISSING 0.

## Fixes (commits on integration/phase16)

1. **D-282: `PUT /guilds/{id}/packages/{pkg}` for a pack** (`77b28435`).
   - `enable_for_guild` now checks `require_manage_or_expressions` first. It then reads the package kind: an emoji, sticker or sound pack is enough for `MANAGE_GUILD_EXPRESSIONS` alone, and any other kind still needs `MANAGE_GUILD`. A recipe stays a 409, and only for a caller who holds `MANAGE_GUILD`.
   - This matches import and removal.
   - The test `settings/tests/packages_pg.rs` was extended:
     - a plain member gets 403;
     - an expressions-only member gets 200 on the pack;
     - an expressions-only member gets 403 on a theme and on a recipe.
   - The OpenAPI summary, the api-client schema, the L-42 note and the ext handoff were updated. The route-matrix rows are unchanged (`membership`).
2. **e2e: registration rate limit in UI sign-ups** (`24286d85`).
   - **Root cause:** the first full Chromium run failed `lounge-shell` "friends" with "Too many tries". `POST /sr/register` allows 5 accounts per minute per address. The new `entrance-journey` spec added a sixth UI registration within the same minute.
   - The API helper `registerUser` already waited this out; the UI flows did not.
   - **Fix:** the new `e2e/support/register.ts` `openTheDoor()` waits out a 429 that carries `retry_after` of 70 s or less, then presses again. It extends the test's timeout by the wait. All seven UI registrations use it.
   - The next two full runs had no failures.

## Gate results (fully merged tree, five branches)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok |
| `sr-sdk --no-default-features` (the new CI steps) | clippy ok, 39 tests passed |
| wasm32 `cargo check --tests`, all `crates/*-wasm` | ok |
| `cargo test --workspace -j 2 --no-fail-fast` | 2709 passed, 0 failed, 425 ignored. Before the backend merge: 2663 passed. |
| Postgres suites: 13 crates with ignored tests, `--include-ignored --test-threads=1`, own scratch DB `sr_p16_int` | 1544 passed, 0 failed. The S3 and Esplora tests return early without their env vars. No test runs ffmpeg. |
| `route_matrix` (regenerated), `openapi_names` | 5 and 2 passed, no diff, no `?` or `other` cells, `KNOWN_CLASHES` empty |
| `cargo deny check` | advisories, bans, licenses and sources ok |
| `pnpm install --offline --frozen-lockfile`, wasm build | ok |
| `pnpm -r typecheck` (with e2e), `pnpm -r lint` | ok |
| `prettier --check .` | tracked files ok. It flags three git-ignored local artifacts (`e2e/.state/bootstrap.json` and old `e2e/*-e2ee` report files), which CI does not have. |
| vitest apps/web `--maxWorkers=2` | 257 files, 2385 of 2385 passed, first run |
| package tests | api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 279, wasm 14: all passed; `node --test` scripts 13 passed |
| web build and bundle budget | ok, every budget; lounge 417.2 of 600 KB gzip |
| `mocks:check`, doc-links, ops-files, dependabot | ok (886 operations) |
| Playwright `tests/`, Chromium, fresh DB, live flags and `SR_E2E_VISUAL=1` | **35 passed, 1 skipped** (K-22, which needs `SR_E2E_K22` and takes 25 min). Includes `entrance-journey`, the 9 visual baselines, `voice-live`, `activities-live` and `message-components`, with E2EE on by default. |
| Playwright `e2ee/voice-e2ee.spec.ts`, Chromium | 1 of 1 passed |
| Earlier runs | Without the live flags: run 1 had 29 passed and 1 failed (the rate limit, fix 2). Run 2, after the fix, had 29 passed, 0 failed and 7 skipped. |
| Real-browser chat upload ledger (D-271) | Ran in Chromium against the Vite dev server: `ChatUploads` over `IdbLedger('chat')`. Six rows were added; the cap kept five and aborted the oldest. After `setDone` and a page reload, the rows came back with the File content, size, description and confirmed parts. `clear()` emptied the store. A full composer resume needs S3 multipart and was not run (see above). |

## Overlaps checked

- **Server look (ext) vs webcam bubbles and settings (lounge):** both edit `UserSettings` and the prefs. These auto-merged, and the vitest and typecheck are green.
- **Live TV slate (backend) vs Admin Live TV:** the new `GET /admin/tv/slate` row is in the route matrix, OpenAPI and mocks. `livetv.test.tsx` is green.
- **CI:** backend's `rust.yml` steps and e2e's composite action are separate files with no shared step.

## Owner questions (new)

- None from the integration itself.
