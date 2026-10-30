# Integration handoff: integration/phase32 (wave 32, the utility drawer)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p32-integ`, branch `integration/phase32`, base e7f6f797 (wave 30b).
Waves 32 and 33 of roadmap-p27 (sections 10 and 11) are one wave by the owner's decision. Env:
`CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p32-utl-cards-target`, `CARGO_PROFILE_DEV_DEBUG=0`, `-j 2`, one
cargo command at a time. Every Postgres suite below ran locally against this integration's own scratch database
`sr_p32_integ` (created and dropped). Nothing pushed, no PR, nothing merged to main.

## Resume here

Done: merges, wiring, gates (table below), docs. Nothing is half-finished. What is left is CI (the list under "What CI
must prove") and the owner's answers (P32-UTL-1 to P32-UTL-5 and the questions at the end).

## Merges (each `--no-ff`, its own commit)

1. `origin/integration/phase30b` (dd24f8a2, the 18 CI-fix commits past e7f6f797): clean.
2. `feat/p32-utl-search` (aabfb408, UTL-SRC-001): clean.
3. `feat/p32-utl-shell` (cb33345e, UTL-SHL-001): clean.
4. `feat/p32-utl-audio` (01f2d176, UTL-AUD-001): clean.
5. `feat/p32-utl-collab` (651118fa, UTL-COL-001, migration 1660): clean.
6. `feat/p32-utl-dev` (8c8cc854, UTL-DEV-001, migration 1630): `registry.ts` (kept every entry) and
   `routes/tools/mod.rs` (add/add: utl-dev's inline router plus `.merge(rooms::router())` and
   `doc.merge(rooms::openapi())`). Git also auto-merged a second `.merge(tools::router())` and
   `doc.merge(tools::openapi())` into `routes/mod.rs` (both branches added the line at different places), which would
   have registered every tool route twice; removed in the follow-up commit (and again after the cards merge).
7. `feat/p32-utl-calc` (00459c72, UTL-CALC-001, migration 1620): `registry.ts`, `tools/mod.rs` (`fx` added to the
   router, `paths` and `schemas`), `app.rs` (both extensions kept).
8. `feat/p32-utl-image` (d676b422, UTL-IMG-001): `registry.ts`. The two hyphen-to-camelCase helpers (utl-image's
   `toolKeySegment`, utl-dev's `catalogId`) are now one, `toolKeySegment`, defined above its first use (the
   `devTool` calls run at module load, so a definition below them would have thrown) and used by every caller.
9. `feat/p32-utl-cards` (26a4ab30, UTL-CRD-001, migration 1670): `registry.ts`, `auth/policy.rs` (both bot blocks
   kept, each closed), `tools/mod.rs` (`cards`), `app.rs` (tool rooms and cards deps, jobs and schedules).
10. `origin/main` (779bcdc7: PR #62, wave 31, and PR #61, wave 30b): `route_matrix.tsv` (union, then regenerated),
    `sr-gateway/src/protocol.rs` (`VOICE_TIMER_UPDATE` and `LISTENING_ROOM_UPDATE` both under
    `GUILD_VOICE_STATES`), `docs/05` (the jukebox and the utility drawer dispatch sentences both kept).

Migrations 1620, 1630, 1660 and 1670 have no `DO $$` blocks. Job ids 1620 (`tools.fx_refresh`), 1660
(`tool_rooms.sweep`) and 1670 (`cards.remind`) are unique; utl-dev has no job (1630 is only a migration).

## Wiring (the integration's own work)

- **The tools facade** (`packages/wasm`): `loadTools()` returns `{raw, search, gif, palette, dev}`. utl-dev and
  utl-calc were written against the day-one facade that returned the raw module; `dev` is new (typed
  `ToolsDevApi`), `tools/dev/wasmTools.ts` reads `.dev`, `tools/calc/engine.ts` wraps `.raw`, and both
  `packages/wasm` tests follow.
- **Search in the drawer** (`tools/drawer/wasmSearch.ts`): the Tools tab loads the wasm index on first show and ranks
  with it (prefix, typo, task phrase), then appends the pure filter's other matches (a translated description); the
  pure `searchTools` answers until the index is ready or if it cannot load. "Did you mean ...?" shows above the list.
- **Corpus** (`apps/web/src/tools/corpus.json`): calc's six tools and `invite` were already there; renamed
  `tier-list` to `tierlist` and `audio-trimmer` to `audio` (the registry ids), added `guide` (10 phrases): 32 tools,
  384 phrases. `check-tools-corpus.mjs` now also proves that the corpus ids equal the registry ids both ways (with a
  test), and runs as its own step in `repo-checks.yml`.
- **Audio tool in the registry** (`tools/audio/AudioDrawerTool.tsx`, id `audio`, group `media`): utl-audio had no
  registry to append to.
- **Post as image** (tier list): "Post the result as an image" renders the tiers with `toPngBlob` and sends the PNG
  through `uploadFiles` and an ordinary message with the attachment; both modules load on the press.
- **Voice timer listener**: `startVoiceTimerListener()` runs in `lounge/voice/VoiceBridge.tsx` (wave 31 is in the
  tree), so a shared timer reaches every member.
- **Invite form and the time zone tool**: when the time zone tool has a pick (`pickedTime`), the invite form shows
  "Use the time picked in the time zone tool", which fills the start in the form's zone.
- **Data export**: the "Request my data" package gains `tools.json` (scratchpad ciphertext as base64, the account's
  own loot tables, claimed invite card seats with the card title, time and reminder choice); README line and the
  `sr-db --test data_export` assertion added. Server loot tables are the server's and are not exported.
- **Authorization matrix** (the agents had not run it; prf-profiles owned it in their slot). It found:
  - `PUT /channels/{id}/voice-timer` answered 204 to an outsider, a member of another tenant and a member without
    the private channel, when nothing was shared (the early "nothing to stop" return skipped the checks). Fixed in
    `sr-gateway/src/voice_timer.rs`: that path now runs the status line's checks (clearing an empty status changes
    and dispatches nothing).
  - The new write routes 404'd for lack of seeds and pushed the malformed count over its ceiling: the matrix now seeds
    a tool room and an invite card per tenant (through the API), a server loot table, an account loot table and a
    note, has hand-made write bodies for the twelve new write routes (`{future_ms}` for the card start), maps the
    voice timer onto the tenant's voice channel, and an overlay for the server's `table_id`.
  - The hourly ECB job writes `fx_rates`/`fx_fetch_state` in the background; both are in `NOISY`.
- **Gateway docs**: `TOOL_ROOM_UPDATE` and `VOICE_TIMER_UPDATE` added to the dispatch list of `docs/05` section 6.3;
  the portal's `gatewayDocs.generated.ts` regenerated.
- **e2e typecheck** (`pnpm -r --if-present typecheck` includes e2e): `drawer.spec.ts` called the old one-argument
  `openMockedApp`; since D-385 it needs the signed-in Owner (`../support/fixtures`).
- Lint: `voiceSync.test.tsx` (utl-dev) had unused `_` parameters; `GuideTool.tsx` had already been fixed on utl-shell's
  branch. Prettier on `tool-rooms.spec.ts`.

Not done (and why):

- **Soundboard upload with AudioTool**: not a one-call change. The soundboard dialog's D-201 trimmer
  (`SoundTrimmer`) takes decoded PCM and owns the J-06 five-second window; `AudioTool` takes a file. Left as is.
- **Playwright** `studio.spec.ts`: needs real object storage (see the gate table).

## Gates (the merged tree, including origin/main)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --workspace --exclude sr-sdk --all-targets -D warnings` (after every merge, last on the main-merged tree) | clean (`sr-sdk` needs cmake locally; CI builds it) |
| `cargo test -p sr-tools-wasm -p sr-chat -p sr-userfeat -p sr-gateway -p sr-events` | 74 + 375 + 35 + 197 + 16 pass |
| `cargo test -p sr-api --lib --test route_matrix --test openapi_names` (route matrix regenerated with `SR_UPDATE_ROUTE_MATRIX=1` on the final tree: no change, no `?`) | 757 + 6 + 2 pass, `KNOWN_CLASHES` empty |
| `cargo test -p sr-admin`, `-p sr-server --lib` | 67, 95 pass |
| Postgres (`--include-ignored --test-threads=1`, scratch DB): `sr-db` `tool_rooms_repo` 6, `cards_repo` 9, `migration_versions` 1, `secret_columns` 1, `account_deletion` 4, `data_export` 4 (with `tools.json`); `sr-userfeat --test tools_db` 5; `sr-server --lib` `tool_rooms_pg_tests`, `fx_pg_tests`, `cards_pg_tests` 31 | all pass |
| Postgres: `sr-server --test authz_matrix` (6) and `--test immutable_rules` (12) | pass after the fixes above (first run: 13 failures) |
| OpenAPI, `schema.d.ts`, MSW (`mocks:check`, 1083 operations), gateway docs | regenerated on the final tree; only the gateway docs changed (the two dispatch names) |
| `pnpm -r --if-present typecheck` (every package, e2e included) | clean (after the `drawer.spec.ts` fix) |
| `pnpm -r --if-present lint` and `prettier --check .` | clean |
| vitest, every package (`apps/web` with `--maxWorkers=2`, `SR_WASM_REQUIRED=1`) | other packages all pass; web 383 of 384 files, 3464 of 3465 tests: `help/handbook.test.tsx` failed because the CHANGELOG heading changed while the run was in progress; it passes on rerun |
| `node scripts/build-web.mjs`, `check-bundle-budget.mjs`, `check-entrance-isolation.mjs` | pass; every public surface clean |
| `check-css-tokens` (50 files), `i18n-audit` (3 format names, see Notes), `check-notices-copy`, `check-doc-links`, `check-tools-corpus` (32 tools, 384 phrases, registry ids equal), `node --test "scripts/*.test.mjs"` (88), workflow YAML parse (js-yaml, every file) | pass |
| Tools wasm (`wasm-pack --release --no-opt`) | 654,339 bytes, 205,823 brotli (limit 300 KB) |
| Playwright Chromium, local stack (sr-server on 8080 with `memory:` storage and a scratch DB, Vite on 5173): `drawer.spec.ts` (4) and `tool-rooms.spec.ts` (2) | 6 pass |
| Playwright Chromium: `studio.spec.ts` (2) | fail locally at the upload step: the studio editing steps pass, but `memory:` storage hands the browser `memory://` upload URLs, so the send cannot happen without Garage; CI's S3 must prove both cases. No cards or timers spec exists. |

## What CI must prove

- Playwright on three engines: `drawer.spec.ts` (changed here to sign in), `tool-rooms.spec.ts` (two contexts, never
  run by anyone), `studio.spec.ts` (never run; selectors in the send and Lightbox steps unverified), plus the existing
  suites over the new drawer (Mod+K moved from the Lounge's quick switcher to the drawer; `shell.test` was updated,
  e2e specs that press Ctrl+K should still find "Go to").
- The authz matrix and `immutable_rules` on Linux (green here).
- The `wasm` build in CI with `wasm-opt` (here `--no-opt`: 654 KB raw, 205 KB brotli, under the 300 KB rule).
- `wasm-pack test` was not run by anyone except utl-image's `tests/gif_wasm.rs` (`--node`); the same logic runs
  natively in `cargo test -p sr-tools-wasm`.
- The fx job's real fetch of the ECB feed (only stubbed transports ran here).

## Owner questions (collected from the handoffs)

1. Drawer key: Mod+K only, or a second binding that opens straight on Tools (utl-shell)?
2. Go to outside the Lounge is short until the store has loaded (Settings, Cinema after a reload): leave it, or boot
   the guild/channel store at the signed-in root (touches the gateway connection) (utl-shell)?
3. Should the voice-message path accept WebM so Chromium can record and trim voice messages (utl-audio)?
4. Lost scratchpad key: notes are gone (the UI says so). Offer to store the passphrase-wrapped recovery bundle on the
   server (P32-UTL-2, utl-dev)?
5. `/roll` as a bot app command and a "roll in secret" form (utl-dev)?
6. Server loot tables readable by every member (P32-UTL-4, utl-dev)?
7. Let members share their time zone (a profile field with a visibility choice) for the time zone tool (utl-calc)?
8. Is a runbook line enough for instances with an outbound allow list (`www.ecb.europa.eu`) (utl-calc)?
9. English-only percent phrases and corpus for the beta (P32-UTL-5, utl-calc)?
10. Card reminders for a member who turned off DMs from server members (P32-UTL-3, utl-cards)?
11. Purge finished or canceled invite cards and their claimant lists after some time; today they stay with the channel
    (utl-cards; also a retention row for wave 40)?
12. "Open in studio" also in the message "more" menu; "Save as a copy" to a mount instead of a download (utl-image)?

## Notes

- `scripts/i18n-audit.mjs` counts 3 literals in `tools/studio/StudioEditor.tsx`: the export format names (PNG, JPEG,
  WebP), which are not translated.
- Local Rust gates exclude `sr-sdk` (its `opus` dependency needs cmake, which this machine lacks); CI builds it.
- New jobs (Admin > Jobs lists them; `deploy/OPERATOR.md` keeps no per-job list): `tools.fx_refresh` (hourly tick,
  one fetch per 20 h after a success), `tool_rooms.sweep` (hourly) and `cards.remind` (every 30 s).
