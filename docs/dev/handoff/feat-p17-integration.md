# Handoff: integration/phase17 (wave 17 merges, Phase 17 round M-20 to M-24, gates)

Main checkout `C:\Users\braxt\Desktop\SIDEROOM_repo`, branch `integration/phase17` (base = main `fce0f821` plus the three owner-docs commits that were already on the branch). Local commits only: nothing pushed, no PR, nothing merged to main.

## Resume here

**State (2026-10-20): done.** All five Phase 17 branches are merged; M-20 to M-24 are built and ticked; M-25 (Phase 7 moderation backend, size L) is left for the next wave as asked. Every gate below passes on the merged tree, and both SDK libraries pass 7 of 7 against a local `sr-server` (section "serenity follow-up" below). None of this pass's servers, Vite or cargo processes are running. All scratch databases of this pass (`sr_p17_int`, `sr_p17_int2`, `sr_p17_ie2e`, `sr_p17_sdk`) were dropped, and so were the leftover `sr_tpl_*` migration templates (nothing else was connected).

**What is left for the owner or CI:**

1. Push `integration/phase17` and let CI run. CI that has never run on GitHub: `sdk-compat.yml` (now builds and deny-checks the nested serenity project, installing `cargo-deny` with `taiki-e/install-action`).
2. **SDK compatibility:** serenity 0.12.5 and discord.js 14.27.0 both pass 7 of 7 locally. DEV-SDK-003 is unticked again, with the note that it is ticked when the first scheduled sdk-compat run is green, as the follow-up item says.
3. Owner actions that stand: pin the electrs image digest and run or point at a node (M-22, D-312); the lawyer notes of D-293 to D-295 and D-311; D-314's lawyer check of the JASSUB reading.
4. Notices: the libass, FreeType and GNU FriBidi copyright lines in `THIRD_PARTY_NOTICES.md` follow upstream and were not checked against a local copy (the jassub package ships no licence file for its wasm components). The NTP and Zlib texts are the SPDX templates with their placeholders left in. Fill these in from the upstream sources before the instance is opened up.

**How to rerun the browser tests locally:** as in `feat-p16-integration.md`, with these changes:

- The serenity bot is built with `cargo build -j 2 --manifest-path e2e/sdk-compat/serenity-bot/Cargo.toml`.
- For the SDK driver, start `sr-server` with `SR_SERVER__PUBLIC_URL=http://127.0.0.1:8080`. Then run `node e2e/sdk-compat/run.mjs --lib discord.js`; for serenity, also set `SDK_COMPAT_SERENITY_BIN` to the built `sideroom-serenity-bot.exe`.
- The driver signs in as the e2e Owner (`e2e-owner-Passw0rd!` by default), so run it after Playwright's global setup has bootstrapped that database.

## Merges (all `--no-ff`, in this order)

| Branch | Conflicts and resolution |
|---|---|
| feat/p17-web | none |
| feat/p17-jassub | none (`web.yml`, doc 03 and the p16 follow-ups auto-merged) |
| feat/p17-server-a | `docs/inventory/17-developer-platform.md` DEV-APP-007: union (server-a's `integration_types` note plus web's M-04 tick and note). `entrance.ts` auto-merged with both branches' keys (web's notices keys and server-a's age keys). |
| feat/p17-server-b | none |
| feat/p17-sdk (head `dea4e912`, D-310 to D-315) | Inventory DEV-SDK-003: sdk's tick and note plus web's M-04 note. `pnpm-lock.yaml`: took the merged side and regenerated it with `pnpm install --offline` (see below). |

- Docs: the decisions log and `open-question-answers.md` changed on `feat/p17-sdk` only (D-310 to D-315 appended in order).
- `answers-followups-p16.md`: the owner round M-01 to M-18 with every branch's ticks, then the Phase 17 round M-20 to M-25, in ID order inside each section.
- Generated files: OpenAPI (from the built `sr-server`), the api-client schema, the MSW mocks (889 operations), the route matrix (`SR_UPDATE_ROUTE_MATRIX=1`) and `gatewayDocs.generated.ts` were all regenerated. Every auto-merged copy was already identical; the only diff was prettier on the gateway docs file. No `?` or `other` cells, `KNOWN_CLASHES` empty.
- Migrations: 1070 (server-a) and 1080 (server-b) are unique, and `migration_versions` passes.

## Lockfile findings

- **Merged lockfile versus main:** the diff is additions only. It adds the jassub 2.5.16 tree (abslink, lfa-ponyfill, throughput, rvfc-polyfill 1.0.8) and the `e2e/sdk-compat` importer with discord.js 14.27.0 and its tree.
- **The vitest peer suffix:** `feat/p17-sdk` had moved the `vitest@2.1.9` and `msw@3.0.3` peer suffix of `packages/api-client` from vite 5.4.21 to vite 6.4.4. The offline re-resolution on the merged tree did not reproduce that, so the merged lockfile kept main's suffix. It was an artefact of the branch's own install, not forced by resolution.
- **Who pulled vitest 2.1.9:** six packages declared `vitest ^2.1.8`: api-client, bot, embedded-app-sdk, markdown-react, ui and wasm. `apps/web` declares `^3.0.0` (resolved 3.2.7), and the root declares no vitest. Two vitest majors in one store is the "vitest 2.x resolved" failure of `feat-p13-foundation.md`: any run that picks up the hoisted or wrong binary gets 2.1's broken `.rejects.toThrow(text)`.
- **The vitest fix (commit `544664a1`):** all six packages now declare `^3.0.0`. It resolved offline from the store with nothing downloaded. vitest 2.1.9, vite 5.4.21, esbuild 0.21.5 and their unique trees left the lockfile.
- **Tests on vitest 3.2.7:** api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 279 and wasm 14, all passed. `pnpm -r typecheck` ok.
- **Cargo:** after M-20 the root `Cargo.lock` is byte-identical to main's. The sdk branch had also switched `cssparser-macros`' `syn` from 2 to 3 (both allowed by `>=2,<4`); restoring main's lock undid that.

## Phase 17 round (ticked in `answers-followups-p16.md`)

- **M-20 (D-310):** the serenity bot is its own Cargo project, `e2e/sdk-compat/serenity-bot/`.
  - Its `Cargo.lock` came from `cargo generate-lockfile --offline` (185 packages). Its `deny.toml` holds the six RUSTSEC ignores and the `serenity` wrapper. The root-only MLS ignore and the other wrappers were left out, because cargo-deny warns on unused entries.
  - The root `Cargo.toml` excludes the project. `sr-server` lost the dependency, the feature and the example.
  - The root `deny.toml` and `Cargo.lock` equal main's again.
  - The workflow, `run.mjs`, docs and the inventory were updated.
- **M-21 (D-311):** the age confirmation is a separate checkbox with its own error. `confirmed_min_age` is sent only when that box is ticked.
  - Unit tests: both errors appear on an empty submit, and the age error alone appears with nothing posted.
  - E2E: `tickConfirmations()` in `e2e/support/register.ts` is used by all seven UI registrations. These include passkeys and voice-live, which the item did not list.
  - Docs: doc 06 and moderation-compliance section 14 item 5.
- **M-22 (D-312):**
  - Doc 12 has a new section 2.1, "Running electrs".
  - `prod.yml` already had `extra_hosts` from M-02; it gained a comment pointing at 2.1.
  - New env rows in donations.md.
  - `deploy_files.rs` asserts the `extra_hosts` entry and the RPC default.
- **M-23 (D-313):** the pack's `LICENSE` now ends with the verbatim CC0 1.0 legal code from `constant_time_eq-0.4.2/LICENSE-CC0`. `cmp` showed it is identical, with LF line endings. `render-stickers --check`: 16 stickers.
- **M-24 (D-314) and the pending notices:**
  - JASSUB: the D-314 sentence, the MIT wrapper text, libass ISC, FreeType FTL, MIT-Modern-Variant, NTP, Zlib, BSL-1.0 and the full LGPL-2.1, which also covers FriBidi.
  - Liberation Sans 2.00.5 with its OFL 1.1 text.
  - rvfc-polyfill 1.0.8 with the package's GPL-3.0 text.
  - Also updated: the `check-licenses.mjs` comment, the jassub handoff and the doc 03 row. `check-notices-copy.mjs --write` was run.
- **M-25:** not started (next wave).

## Fixes (commits on integration/phase17)

1. **`d4e13582` fix(sdk-compat):**
   - **Root cause:** the first local discord.js run passed 5 of 7 checks; guild-create and message-content-delivered timed out. discord.js emits `guildCreate` only for a guild it did not know at READY. The driver installs the bot before connecting, so the GUILD_CREATE that follows surfaces as `guildAvailable`.
   - **Fix:** the bot reports guilds from `guildAvailable`, `guildCreate` and the READY cache.
   - **Result:** the next run passed 7 of 7.
2. **`5dd264b6` fix(gateway):** READY carries `resume_gateway_url` (`ws(s)://<public origin>/gateway`) instead of null. serenity's `Ready` requires the field, and Discord always sends it. A unit test covers it. The full Chromium suite and voice E2EE were rerun after this change and passed (below).

3. **`81f256aa`, `eab7d678`: the serenity follow-up** (see the section below).

Flakes: one run of the `gateway_pg` Postgres tests failed right after a test edit and passed in the four runs that followed. Its output was not kept, so the cause is unknown. Every other gate passed the first time, apart from the sdk-compat driver runs described here.

## serenity follow-up (coordinator request, after the first handoff)

- **Logging.** `tracing-subscriber` was added to `e2e/sdk-compat/serenity-bot`.
  - It is 0.3.23 with `env-filter`, the root lockfile's version, resolved `--offline`; nothing was downloaded.
  - The bot logs to stderr (default `warn,serenity=info`, `RUST_LOG` overrides) and keeps stdout for the JSON-lines protocol. The subscriber stays in the bot so that refused frames show up in the CI logs.
- **What serenity refused, in the order the logs showed it.** serenity drops any frame that does not fit its typed models. Each place below is where SIDE ROOM differed from Discord, and each fix has a unit test:
  1. **IDENTIFY.** serenity sends `presence.since` as serde's `SystemTime` object, and the server closed with 4002 "decode error". `PresenceUpdate.since` now reads a number, null or that object (`sr-gateway` `protocol::tests`).
  2. **GUILD_CREATE.** The payload lacked fields serenity's `Guild` requires: `emojis`, `stickers`, `premium_tier`, `nsfw_level`, `premium_progress_bar_enabled`, `joined_at`, `large`, `member_count` and `guild_scheduled_events`.
     - The guild's real emojis and stickers are now included.
     - The others get Discord's values for a server without boosts; scheduled events stay an empty list because they come over REST.
     - Tests: a unit test, and the `gateway_pg` Postgres test now asserts the fields.
  3. **Command objects.** `integration_types: null` failed serenity's `Vec`. A command registered without the field now answers `[0]`, guild install (D-305; `sr-apps` test).
  4. **INTERACTION_CREATE.** `attachment_size_limit` was missing. It is now the invoker's effective upload limit (`flow_tests` assertion).
- **How the fields were found.** I wrote a throwaway serde probe in the scratchpad, a separate Cargo project on the cached serenity. It decoded the captured READY, GUILD_CREATE, command and interaction payloads into serenity's own types.
- **Result:** serenity 7 of 7, discord.js 7 of 7. The OpenAPI document did not change; it was regenerated and checked.
- **Gates rerun after the server changes:**
  - fmt and workspace clippy `-D warnings`;
  - the nested bot: clippy, fmt and cargo deny;
  - `sr-gateway`, `sr-apps`, `sr-api` and `sr-server` with `--include-ignored --test-threads=1` on a scratch DB: 1079 passed, 0 failed;
  - Playwright Chromium on a fresh DB with the live and visual flags: 35 passed, 1 skipped;
  - voice E2EE: 1 passed.
- **DEV-SDK-003:** unticked again. The note says it is ticked when the first scheduled sdk-compat run is green.

## Gate results (merged tree)

| Gate | Result |
|---|---|
| `cargo fmt --all --check` | ok |
| `cargo clippy --workspace --all-targets -j 2 -- -D warnings` | ok (and `-p sr-server` again after fix 2) |
| `sr-sdk --no-default-features` | clippy ok, 39 tests passed |
| `cargo test --workspace -j 2 --no-fail-fast` | 2727 passed, 0 failed, 430 ignored |
| Postgres suites (`--ignored --test-threads=1`, own DB `sr_p17_int`) | 430 of 430 passed. These are all the ignored tests; the non-ignored ones passed in the run above. The S3 and Esplora tests return early without their env vars. |
| `migration_versions`, `route_matrix`, `openapi_names` | 1, 5 and 2 passed; no diff after regeneration |
| `cargo deny check` (root) | advisories, bans, licenses and sources ok; no serenity entry |
| `cargo deny check` (nested serenity project), nested `cargo build --offline` | ok, ok |
| `pnpm install --offline --frozen-lockfile`, wasm build | ok |
| `pnpm -r typecheck`, `pnpm -r lint` | ok |
| `prettier --check .` | tracked files ok. It flags only the three git-ignored local artefacts from p16 (`e2e/.state`, the old e2ee report files). |
| i18n (`i18n-verify-diff main HEAD`) | MISSING 0 |
| `mocks:check` | up to date (889 operations) |
| licence allow-list (`pnpm licenses list --json --prod`, then `check-licenses.mjs`) | ok |
| `check-notices-copy.mjs` | identical |
| vitest apps/web `--maxWorkers=2`, quiet machine | 262 files, 2404 of 2404 passed, first run |
| package tests (vitest 3.2.7) and sdk-compat `node --test` | api-client 6, bot 18, embedded-app-sdk 8, markdown-react 45, ui 279, wasm 14, sdk-compat 3: all passed |
| `node --test` scripts (ci-scripts, install) | 15 passed |
| web build and bundle budget | ok, every budget (lounge 418.7 of 600 KB, entrance 226.7 of 300 KB) |
| `render-stickers --check`, doc links, ops files, dependabot | ok (16 stickers) |
| `deploy_files` (install and deploy files) | 13 passed |
| Playwright `test:static` | 19 passed |
| Playwright `tests/`, Chromium, fresh migrated DB, `SR_E2E_VOICE/ACTIVITIES/LIVE/VISUAL=1`, win32 baselines | 35 passed, 1 skipped (K-22). Includes `entrance-journey` with the two confirmations, the 9 visual baselines and `voice-live`. Run twice: before fix 2 (3.6 min) and after it on a fresh DB (5.2 min). |
| Playwright `e2ee/voice-e2ee.spec.ts`, Chromium | 1 of 1 passed, before and after fix 2 |
| sdk-compat driver against the local server | discord.js 14.27.0: **7 of 7**; serenity 0.12.5: **7 of 7**, both after the serenity follow-up |

## Owner questions (new)

- None. The coordinator approved `tracing-subscriber` for the nested bot because it was already in the root lockfile.
