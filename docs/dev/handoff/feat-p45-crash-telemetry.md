# Handoff: feat/p45-crash-telemetry (wave 45-A, OPS-CRASH-001 deterministic crash ring buffer)

Branch `feat/p45-crash-telemetry`, base `integration/phase44`. Migration 2160 only (block 2160-2169). Threat-model row T45, decisions D-430 to D-434. Inventory section "Wave 45" in `docs/inventory/21-feature-phase.md` (this branch writes its header; 45-B appends UX-SHOW-001 below it).

**An agent proposes; a human merges.** Nothing in this item deploys, merges or runs code. A card in `#crash-triage` is a bug report with a replay buffer; a Claude worktree agent reads it through a staff session, reproduces the bug and opens a pull request, and staff review and merge it like any other. The card has one read-only link and no action button, and the format has no free text, so there is nothing in it for an agent to mistake for an instruction (T45, D-434).

## Audit first (what existed)

| Box | State on `integration/phase44` | What this branch did |
|---|---|---|
| Client error reporting, error boundaries, `unhandledrejection` hooks | MISSING (the only boundary-like code is the UI kit's `State` primitive; no `componentDidCatch`, no `window` error hooks, no reporting) | `apps/web/src/crash/` |
| WASM panic reporting | MISSING: the `crates/*-wasm` crates set no panic hook (no `console_error_panic_hook`, no new dependency allowed), so a Rust panic reaches JS as a `WebAssembly.RuntimeError: unreachable` with `wasm://` frames | classified by type, name or frame (`report.ts::classify`); `reportWasmPanic` for code that catches one |
| Secret-pattern list | EXISTS: `scripts/secret-scan.mjs` `PATTERNS` | reused (imported) by `scripts/check-crash-card.mjs`, plus crash shapes in `scripts/crash-secret-patterns.json` |
| Staff Server channels | EXISTS: `StaffVoice` (`crates/sr-server/src/staff_feed.rs`) posts as the system user; channels are seeded by `sr-cli bootstrap` | `#crash-triage` created idempotently on first use and at server start; not added to the bootstrap list |
| Deterministic tick recorder | MISSING (no tick abstraction in the web client; Activities talk through the bridge in `activities/host.ts`) | `TickRing` + `recordTick`; the Activity bridge feeds it a command enum per call |
| CI e2e wiring | EXISTS: `@gate` specs run in the "Exit gates" step of the `playwright` job | `crash-telemetry.spec.ts` is tagged `@gate` |

## Done

- Migration `2160_crash_reports.sql`: one row per fingerprint with a count, the allow-listed card and replay as JSONB, and where its card was posted. No account id, no address.
- `crates/sr-api/src/routes/crash/` (`allow_list.rs`, `mod.rs`): the server-side allow-list `sanitize` (closed sets for kind, tick source, tick input and capability names; strict shapes for commit, route name, frames; file names normalized: host, query and fragment gone, the piece after `Users` or `home` and any id-like piece become `:id`, at most the last 4 pieces; every kept string goes through `suspicious` once more), the fingerprint (SHA-256 over the commit and the top 5 normalized frames), `POST /crash-reports` (session only, 32 KiB body cap, 6 per account per 10 minutes, 600 per hour for the platform, 60 new crash groups per hour), `GET /admin/crash-reports` and `GET /admin/crash-reports/{id}` (any appointed staff member), `CrashStore` and `CrashSink` traits (the Postgres store and the Staff Server poster are implemented in `sr-server`, because `sr-api` has `sr-db` only as a dev dependency).
- `crates/sr-db/src/crash_reports.rs` (`PgCrashReports`): upsert with a count, keeps the first card and replay, trims to the 2000 most recently seen.
- `crates/sr-server/src/staff_feed.rs`: `StaffVoice::ensure_crash_channel` (creates `#crash-triage` under Text Channels; overwrites made current on every call: `@everyone` denied the view, every role with a platform permission allowed to read, the system user allowed to post), `crash_card`/`crash_card_json` (the structured JSON in a code block, frames trimmed to fit, replay inputs always kept), `CrashSink for StaffVoice` (post a new card, rewrite the count in place at counts 2, 3, 5, 10, 25, 50 and every 100th). `crates/sr-server/src/crash_wiring.rs` adapts the table. `app.rs` builds the deps and spawns `ensure_crash_channel` at start.
- A bug the gate found and this branch fixed in code it shares: `StaffVoice::ensure_member` used a `OnceCell` that a server started before the Owner was bootstrapped used up on nothing, so the system user was never made a member of the Staff Server and every card was dropped with "system user cannot post here". It now waits for a Staff Server (regression test inside the PG test).
- `crates/sr-admin/src/service_ext.rs::authorize_crash_read`: `STAFF_BASIC` check (a refusal leaves the usual `denied` row) and the `crash.report.read` platform audit entry when a replay is opened.
- Web `apps/web/src/crash/`: `allowList.ts` (the client twin of the list), `ringBuffer.ts` (`TickRing`, 60 ticks, in memory only, `bridgeInput`), `capabilities.ts` (13 boolean flags and a four-word engine, never the user agent), `frames.ts` (V8 and Gecko stacks, wasm frames), `report.ts` (build, classify, send once per distinct crash and at most 5 per page load, never throws, never reports its own failure; `installCrashTelemetry` hooks `error`, `unhandledrejection` and the `sr-crash-tick` event for in-page runtimes), `routeName.ts` (route pattern to name; an unmatched page is `unmatched`, never its URL), `CrashBoundary.tsx` (top-level React boundary with a plain reload page). Wired in `app/main.tsx` (installed only by the gated app bundle, D-385), `router.tsx` (`defaultOnCatch`), `activities/host.ts` (command enum per bridge call, never arguments), `vite.config.ts` (`__SR_COMMIT__`: `SR_COMMIT_SHA`, then `GITHUB_SHA`, then `git rev-parse HEAD`).
- i18n fragment `p45-crash-1.ts` (3 keys). Route matrix rows (`POST /crash-reports` `self` bot-deny, the two admin reads `staff` `STAFF_BASIC`), authz matrix seed (`report_id`) and write body, OpenAPI, api-client types and MSW handlers regenerated.
- CI: `scripts/check-crash-card.mjs` (grep of a built card; `--self-test`), run by `node --test "scripts/*.test.mjs"` and by a named step in `repo-checks.yml`; `crash-telemetry.spec.ts` in the `@gate` step of `e2e.yml`.
- Docs: T45, D-430 to D-434, the inventory section header and OPS-CRASH-001.

## Tests run

See the inventory item for the list; the commands are in "Resume here". Proofs that ran on this branch, on a scratch Postgres (`sr_p45_crash`), one process at a time:

- `cargo test -p sr-api --lib crash`: 12 (the hostile-payload test among them).
- `cargo test -p sr-server --lib staff_feed -- --include-ignored --test-threads=1`: 16 (the new `crash_triage_is_created_once_staff_only_and_receives_allow_listed_cards` among them).
- `cargo test -p sr-api --test route_matrix`: 6; `cargo test -p sr-db --test secret_columns`.
- Vitest `src/crash` (18) and `src/router.test.tsx`, `src/activities`, `src/i18n`, `src/__tests__` (144 together); tsc, eslint, prettier, `scripts/i18n-audit.mjs` (0 missing).
- `node --test scripts/check-crash-card.test.mjs scripts/secret-scan.test.mjs` (18).
- Playwright Chromium `tests/crash-telemetry.spec.ts` against the Vite dev server and `sr-server` (storage `memory:`), twice in a row against the same database (the run uses a fresh commit and function name each time): 2 passed.

### Final gates (all run on this branch, one process at a time)

- `cargo test -p sr-server --test authz_matrix -- --ignored --test-threads=1`: 9 passed. `--test immutable_rules -- --include-ignored --test-threads=1`: 25 passed.
- `cargo fmt --check` clean; `cargo clippy -j 2 -p sr-api -p sr-db -p sr-admin -p sr-server --all-targets -- -D warnings` clean (it asked for two let-chain/`?` fixes, made).
- Full vitest `--maxWorkers=2`: 442 files, 4036 tests passed. tsc, eslint, prettier clean.
- `pnpm build` (web), `scripts/check-bundle-budget.mjs` passes. The first build was 0.1 KB over the app-chunk limit; the report builder (`crash/report.ts`, allow-list, capabilities, frames) is now a lazy chunk loaded on the first crash, only `crash/install.ts`, `crash/enums.ts` and the ring stay eager. `scripts/check-entrance-isolation.mjs`: every public surface clean.
- Playwright Chromium `crash-telemetry.spec.ts` re-run after the split on a fresh database with the server started before the Owner bootstrap (as in CI): 2 passed.
- `node --test scripts/threat-model.test.mjs scripts/check-crash-card.test.mjs scripts/secret-scan.test.mjs` (23 passed), `scripts/secret-scan.mjs` clean, `scripts/i18n-audit.mjs` 0 missing.

## Still open (with reasons)

- Firefox and WebKit runs of the new spec: it is skipped outside Chromium (the synthetic panic is a V8 error object); CI runs the step on all three engines and the skip is deliberate. The client code paths are engine independent and covered by Vitest.
- Emulator ticks: the emulator (`apps/web/src/emu`) does not feed the ring yet; the `emulator` source and the `sr-crash-tick` event exist for it and for the wave-48 starter pack, which ships "the 45-A deterministic tick recorder wired in".
- A WASM panic that app code catches itself is only reported if that code calls `reportWasmPanic`; unhandled ones are caught by the page hooks. No call site was changed in the wasm facade (`packages/wasm`).
- No admin page for crash reports: staff read the channel and the two JSON routes (an agent needs the routes). A page is a follow-up if staff want one.
- A channel created at runtime appears in staff clients on their next reload (no `CHANNEL_CREATE` is broadcast by `ensure_crash_channel`).
- Crashes from a browser that cannot reach the server are lost (no offline queue).

## Decisions I took (defaults, all in the decisions log as D-430 to D-434)

Drop rather than reject unknown fields; fingerprint over the top 5 frames; card refreshed at milestone counts; every signed-in browser reports with fixed limits (6 per account per 10 minutes, 600 per hour, 60 new groups per hour, 32 KiB); `#crash-triage` visible to roles that carry a platform permission, readable through the API by any appointed staff member, with an audit entry for opening a replay; nothing deploys.

## Owner questions

1. Opt-out: should a member (or the browser's Global Privacy Control) be able to switch crash reports off? Default (D-432): no, the report holds no personal data and is sent only after sign-in.
2. Should reading the whole list also be audited, or only opening one replay (default)?
3. The limits (6 per account per 10 minutes and so on) are constants; do they need to be platform settings in Admin > Limits?
4. Should a staff role without a platform permission (a cosmetic role) see `#crash-triage`? Default: no.

## Resume here

Nothing half-done on this branch. For the integrator:

1. Merge with 45-B: both add a section to `docs/inventory/21-feature-phase.md` (45-A writes the header, 45-B appends below it), rows to `docs/15-threat-model.md` (T45, then T46) and `docs/01-decisions-log.md` (D-430 to D-434, then D-435 to D-439); `app.rs` gets one `crash_deps` block and one `.extension(crash_deps)` line.
2. Regenerate OpenAPI, the api-client and the MSW handlers on the merged tree (`cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`, `pnpm --filter @sideroom/api-client generate`, `mocks:generate`).
3. Run `cargo test -p sr-server --test authz_matrix -- --ignored --test-threads=1` and `--test immutable_rules`, the Playwright `@gate` step, and `node scripts/check-crash-card.mjs --self-test`.
4. Message for the CHANGELOG: members see nothing new except a plain "Something went wrong" page with a Reload button if a screen breaks; staff get `#crash-triage`.
