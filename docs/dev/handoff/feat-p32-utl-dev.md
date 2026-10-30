# Handoff: feat/p32-utl-dev (wave 32, UTL-DEV-001)

Agent utl-dev. Base `integration/phase32`, merged `feat/p32-utl-search` at 4527fc8b and `feat/p32-utl-shell` (registry, d7579bca). Migration 1630.

## Done

**Rust, `crates/sr-tools-wasm` (my three modules only)**
- `dice.rs`: RPG notation (`2d20kh1+5`, `4d6dl1`, `4d6dh1`, `kl`, `d%`, `dF` fudge, exploding `!` and `!>=n`, success counting `>= > <= < =`, `+ - *` and brackets), a seeded SplitMix64 (the browser passes 64 bits from `crypto.getRandomValues`, so a roll is reproducible from its seed and the UI can show it), unbiased `below(n)`, coin flip, weighted pick. Everything is bounded: 1000 dice per group, 1,000,000 sides, 100 explosions per die, 20,000 rolls and 200 characters per expression, 16 brackets deep; errors are codes (`syntax:7`, `too-many-dice`, ...).
- `diff.rs`: LCS diff over lines, words or characters (prefix and suffix trimmed first, a 2.5 M cell table cap and 1 MB per side, "too-large" instead of a freeze). Deletions come before additions.
- `hash.rs`: MD5 (RFC 1321 vectors) and CRC-32, hand-written, so `Cargo.toml` and the lock did not change (no `md-5`). SHA-1/256/384/512 go through `SubtleCrypto` in the browser as the roadmap says.
- 26 native unit tests (`cargo test -p sr-tools-wasm`); `packages/wasm/test/tools-dev.test.ts` runs 27 dice range cases x 40 seeds, 15 error cases, the diff and the hashes through the built wasm and the `loadTools` facade (6 tests). No `wasm-bindgen-test` (see "Still open").

**Backend**
- Migration `1630_tools_notes_loot.sql`: `scratch_notes` (ciphertext only, 1..65536 bytes by CHECK, revision, per-user primary key, cascade on account delete) and `loot_tables` (per account or per guild, 1..200 rows, unique names ignoring case).
- `crates/sr-userfeat/src/tools.rs` (`impl UserFeatures`, no feature toggle): notes (list, put with revision check, delete; 500 notes per account; every query binds `user_id`, so there is no cross-account read and no staff read path to refuse) and loot tables (50 per owner, label/weight validation).
- `crates/sr-api/src/routes/tools/{mod,notes,loot,timer}.rs`: `GET /users/@me/tools/notes`, `PUT|DELETE /users/@me/tools/notes/{note_id}`, `GET|POST /users/@me/tools/loot-tables`, `PATCH|DELETE .../{table_id}`, the same under `/guilds/{guild_id}/tools/loot-tables` (members read, `MANAGE_GUILD` writes), `PUT /channels/{channel_id}/voice-timer`. Bots are kept out of `/users/@me/tools` (one line in `auth/policy.rs` and its test). Authz rows added to `route_matrix.tsv` (no `?`); OpenAPI, `schema.d.ts` and the MSW handlers regenerated; the notes schema is named `ScratchNoteObject` because `NoteObject` is taken (`openapi_names` green, `KNOWN_CLASHES` still empty).
- `crates/sr-gateway/src/voice_timer.rs` plus one intent line in `protocol.rs`: `VOICE_TIMER_UPDATE {id, guild_id, user_id, server_ms, timer}` (intent `GUILD_VOICE_STATES`). Publishing a timer also sets the channel's status line ("Focus 24:12"), so the status rules apply unchanged (`SET_VOICE_CHANNEL_STATUS` and a seat, or `MANAGE_CHANNELS`) and members who never open the tool still see it. The server rebuilds the timer from known fields only, keeps no copy and stamps `server_ms` so receivers correct clock skew. `VoiceBackend::voice_timer_set` (default `Unavailable`) is implemented in `crates/sr-server/src/voice.rs`. Documented in `docs/05-api-and-gateway.md`.

**Web (`apps/web/src/tools/{dev,dice,notes,timers}`, registry entries appended, catalog fragments `p32-utl-dev-1..5.ts`)**
- Dev tools: JSON (validate with line and column, format, minify, sort keys), regex tester (browser `RegExp`, flavour note, capped input and matches), Base64, URL encode (part or whole, query parameters), JWT decode (banner "does not check the signature", never verifies), Markdown table builder (CSV/TSV/semicolons, alignment), counters, hashes (text or file), text/JSON diff.
- Dice, coin, wheel (the pick is made first; reduced motion, by the OS or `data-reduced-motion`, lands without spinning), loot tables (account and server scope, `MANAGE_GUILD` gating from `useViewer`).
- `/roll` in the built-in command set (`lounge/msg/slashCommands.ts`, a `roll` result kind, and a small `case 'roll'` in `Composer.tsx`): posts `**2d20kh1+5** = **17** (2d20kh1: [12, ~~5~~])` as an ordinary message; a bad expression is a toast.
- Encrypted notes: AES-GCM with a non-extractable `CryptoKey` in IndexedDB (one record pair per account id), note id as AAD, `0x01 || iv || ct`; a passphrase-wrapped recovery bundle (PBKDF2-SHA256, 600,000 rounds, random salt) is made at setup and offered as a download; restore on a new browser needs the file and the passphrase; a conflict (409) keeps the other browser's version and saves the local text as a copy; "start over" deletes the unreadable server notes. The UI says the operator cannot recover a lost key.
- Timers: pure state machine (`timerCore.ts`, no drift: elapsed is `accumulated + now - startedAt`; a tab that slept through segments settles to the right place), shared store per kind persisted in `localStorage`, "share with my voice channel" control, and the receiving side (`voiceTimer.ts`, a zustand store keyed by channel, cleared when the status line clears).
- **For future player skins (owner note):** embed `useFocusTimer(kind)` from `tools/timers/useFocusTimer.ts` (returns `clock`, `progress`, `running`, `toggle`, `reset`, `skip`, `setConfig`; the same shared timer the drawer tool uses, so a skin shows the drawer's pomodoro, not a second one) and `useNotes()` from `tools/notes/useNotes.ts` (returns `{state, store}` with `store.create/save/remove`; the same encrypted store). Both are plain hooks over framework-free stores (`createTimerStore`, `createNotesStore`) with no drawer dependency.

## Tests run
- `cargo test -p sr-tools-wasm` 26 ok; `-p sr-gateway` 194 ok (4 new timer unit + 4 new timer integration); `-p sr-userfeat` lib 35 ok and `--test tools_db -- --include-ignored --test-threads=1` 5 ok against my own scratch database (`sr_p32_utl_dev`, dropped at the end); `-p sr-api --lib` 741 ok (4 new); `--test route_matrix` 6 ok; `--test openapi_names` ok; `-p sr-db` `migration_versions`, `secret_columns`, `account_deletion`, `data_export` ok.
- `cargo fmt --check` clean; `cargo clippy -p sr-api -p sr-gateway -p sr-userfeat -p sr-server -p sr-tools-wasm -p sr-db --all-targets -D warnings` clean.
- Web: `tsc --noEmit` clean; eslint clean for my files; prettier clean; vitest `src/tools` (5 files, 69 tests: timers, dev logic, dice, notes crypto and store, every tool rendered and axe-checked, registry and catalog), plus `src/i18n` and `src/lounge`; `packages/wasm` `tools-dev.test.ts` 6 ok.

## Decisions
1. The roadmap names the table `user_notes`, but that name already holds the profile notes on other people (migration 0071). The scratchpad table is **`scratch_notes`**; the skins README's "user_notes" means this table.
2. The roadmap says "non-extractable key" and "passphrase-wrapped export". A non-extractable key cannot be exported, so the key is made from random bytes, wrapped with the passphrase at setup (the recovery bundle), then imported as non-extractable for daily use. The passphrase is therefore asked once at setup, not at each export; "download recovery file" re-downloads the stored bundle.
3. Catalog keys cannot hold hyphens (`catalog.test.ts`), but the registry required `tools.<id>.name` with `md-table`. I added `catalogId()` to `registry.ts` (hyphens become camelCase: `tools.mdTable.name`) and used it in the entry helper and in `registryProblems`. utl-shell should keep that if they touch the file.
4. Roll error codes and note problem codes are camelCase for the same reason.
5. MD5 and CRC-32 are hand-written (about 60 lines) rather than adding `md-5` to `Cargo.toml` (shared with two other agents).
6. Bots are denied on `/users/@me/tools` (a person's notes), via `auth/policy.rs`.

## Still open (and why)
- **`wasm-bindgen-test` / `wasm-pack test`** was not used: the crate has no such dev-dependency and `Cargo.toml` is shared. The same dice (50+ cases), diff and hash checks run natively (`cargo test`) and through the built wasm in Vitest.
- **Wire `startVoiceTimerListener()`** (in `tools/timers/voiceTimer.ts`) into the app root next to `applyStatusEvent` in `lounge/voice/VoiceBridge.tsx` (wave 31's area, so I did not touch it): until then a member sees another's shared timer as the channel status line only, and the live clock appears once the listener is mounted. One line: `useEffect(() => startVoiceTimerListener(), [])`. A receiving view (a small clock in the voice channel list or the pomodoro tool reading `useVoiceTimers`) is not built; the status line is the visible part.
- A joiner sees the status line, not the live timer, until the owner pauses or restarts (the server keeps no copy, by design).
- The data export (`sr-db/src/data_export.rs`) does not yet include the member's own ciphertext notes or loot tables.
- Regex tester cannot interrupt a catastrophic pattern (no worker); the tool says so and caps the input at 100,000 characters.
- Playwright was not run (ports belong to others). Mod+K, pop-out and pin are utl-shell's; the tools implement `ToolProps` and take no props they do not use.
- Web build, bundle budget and `check-entrance-isolation.mjs`: see the last line of this file.
- Merge notes for the integrator: `registry.ts` (every agent appends to `TOOLS`; mine is the `utlDev` block and a `catalogId` helper), `packages/api-client/openapi.json`, `schema.d.ts` and `handlers.generated.ts` are regenerated files (regenerate rather than merge), `lib.rs` of `sr-gateway` gained `mod voice_timer;`, and `voice_tests.rs` gained a `#[path]` module line.

## Open owner questions
1. The lost-key rule: today a lost key and passphrase mean the notes are gone (the UI says so, "start over" deletes the old ciphertext). Is that acceptable, or should the recovery bundle also be offered to be stored on the server (the operator would then hold something a weak passphrase could expose)?
2. Should `/roll` also exist as an app command for bots and `/roll` results be hideable (a "roll in secret, only I see it" form like Discord bots have)? Not built.
3. Per-server loot tables are plain text readable by every member; confirm that is wanted (the account tables are private to the member).

## CHANGELOG lines (Phase 32 - 2026-10-30)
- Utility drawer tools: JSON, regex tester, Base64, URL encode, JWT decoder, Markdown table, counters, hashes (MD5, SHA, CRC-32) and diff, all in the browser (UTL-DEV-001).
- Dice (RPG notation), coin, wheel and loot tables (per account or per server), and `/roll` in chat (UTL-DEV-001).
- Private notes encrypted in the browser with a passphrase-wrapped recovery file; the server stores only ciphertext (UTL-DEV-001, migration 1630).
- Pomodoro, stopwatch and countdown timers that can be shared with a voice channel, with `VOICE_TIMER_UPDATE` and `PUT /channels/{id}/voice-timer` (UTL-DEV-001).

## Build gates (run last)
`node scripts/build-web.mjs` ok; `node scripts/check-entrance-isolation.mjs` green (every public surface clean; none of my code is imported by a public entry, the dice engine is a dynamic import from `Composer.tsx`); `node scripts/check-bundle-budget.mjs` green (all budgets). Full web Vitest: 358 of 360 files passed on the first full run; the two failures were the catalog tests catching a key I added while the run was in progress, and both pass on rerun (`src/i18n`, `src/tools`). Eslint reports one error outside my files: `tools/drawer/GuideTool.tsx` (utl-shell's `_props`). The scratch database is dropped. `packages/wasm/dist/tools` was built locally with `wasm-pack build crates/sr-tools-wasm --target web --release --no-opt` (the integrator's `pnpm --filter @sideroom/wasm build` does the same).
