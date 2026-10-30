# Handoff: feat/p32-utl-calc (wave 32, UTL-CALC-001)

Branch `feat/p32-utl-calc`, base `integration/phase32` plus `feat/p32-utl-search` at 4527fc8b and `feat/p32-utl-shell` at cb33345e merged in. Item: UTL-CALC-001, the calculator, programmer mode, units, percent phrases, currency through the ECB rates job and the time zone tool. Migration 1620 `fx_rates`.

## Done

**Rust, `crates/sr-tools-wasm/src/{calc,units,percent}.rs`** (replaced the day-one stubs; nothing else in the crate touched)

- `calc.rs`: a scientific parser (`+ - * / ^ **`, `mod`, postfix `!` and `%`, parentheses, implicit multiplication, scientific notation, `ans`, 38 functions and the constants pi, e, tau, phi, degrees or radians) and a programmer-mode integer evaluator (8, 16, 32, 64 bits, signed two's complement or unsigned, `| ^ & << >> >>> ~ + - * / %`, `0x 0b 0o` and bare digits in base 2, 8, 10 or 16, wrap-around flag, the four-base views, a base converter). Errors are values with a stable `code`, a position and a detail; nesting is bounded (no stack overflow on a hostile 5000-deep input).
- `units.rs`: one table of 74 units in 8 categories (length, mass, volume, temperature, data with decimal and binary prefixes, speed, time, area), a spelling parser (`5 km to mi`, `98.6 F -> C`, `1/2 cup = ml`, plurals, case rules, `mb` is megabyte and `Mb` megabit), conversion to one unit or to all units of the category.
- `percent.rs`: phrases ("15% of 80", "80 + 15%", "80 + 15% tip", "20% off 50", "12 is what % of 60", "12 is 20% of what", "increase 80 by 15%", "from 50 to 65") into six kinds with the arithmetic, plus a form entry point (`percentCompute`).
- wasm-bindgen exports (flat names, all return JSON text): `calcEval`, `calcFormat`, `programmerEval`, `programmerConvertBase`, `unitsList`, `unitsConvert`, `unitsConvertAll`, `unitsQuery`, `percentPhrase`, `percentCompute`.

**Server**

- Migration `1620_fx_rates.sql`: `fx_rates` (one row per currency, EUR base, CHECKs) and the single-row `fx_fetch_state`.
- `crates/sr-server/src/fx_jobs.rs`: job `tools.fx_refresh` (reserved singleton id 1620, hourly tick, at most one fetch per 20 h after a success), the ECB XML parser (`quick-xml`, already in the lock; DTDs refused, size and event count bounded, at least 10 currencies, one reference date), the store, and the snapshot behind the route. The fetch uses `sr_net::SafeClient` built from the same live outbound policy as the RSS feeds (`feed_client(unfurl_policy)`), so scheme, deny and allow host lists, resolved addresses and redirect hops are all checked. (`sr-unfurl`'s `SafeFetcher` refuses XML content types, so the generic `SafeClient` that the RSS feeds use is the right one; both are the same `sr-net` policy.) Wired in `app.rs` (extension, job registration, schedule).
- `crates/sr-api/src/routes/tools/{mod,fx}.rs`: `GET /tools/fx` returns `{enabled, base, as_of, fetched_at, stale, rates}`. Authz row `GET /tools/fx authenticated user allow` in `route_matrix.tsv`; OpenAPI, client types and MSW handlers regenerated.
- `crates/sr-admin/src/settings.rs`: `tools.currency_rates` (boolean, default on) and `tools.currency_stale_hours` (24 to 720, default 96), group `tools`, in the Admin Limits catalog.
- `docs/05-api-and-gateway.md` section 5.3b.

**Web, `apps/web/src/tools/{calc,timezone}`**: six tools appended to `registry.ts` (`calculator`, `programmer`, `units`, `currency`, `percent`, `timezone`; ids have no hyphens, so catalog segments are the ids).

- Calculator (live answer, keypad that types at the caret, deg/rad, history of 20 in localStorage, `ans`), programmer (width, signed, input base, four views with copy, a clickable bit grid), units (category, two units, swap, quick `5 km to mi`, all-units table), percent (phrase box with examples, plus a form), currency (amount, two selects, swap, rate line, ECB date, stale note, "switched off" state; the only request is `GET /tools/fx`), time zones (overlap grid, work hours, skip weekends, best hour, people you add yourself, copy `<t:unix:F>`, `<t:unix:t>` and `<t:unix:R>`).
- `timezone/zones.ts` is pure `Intl.DateTimeFormat` arithmetic (offsets, local-to-instant with gap and repeated-hour rules, grid, runs, best column, tags). `pickedTime` (`zones.ts`) is the hand-off point for the wave-33 invite cards: `get`, `set` and `subscribe` on the instant last picked.
- Strings: `i18n/messages/en/p32-utl-calc-1.ts` (235 keys; unit names are `tools.units.u.<id>`). Styles: `tools/calc/calc.css` (tokens and logical properties only, loaded with the tool, not with the drawer).
- `calc/engine.ts` wraps the raw module that `loadTools()` returns; I did not edit the shared facade (`packages/wasm/src/index.*`), so there is no merge conflict there. `setToolsEngine` and `configureToolsWasm` are the test hooks.

## Tests run

| What | Result |
|---|---|
| `cargo test -p sr-tools-wasm` | 22 passed: 130+ scientific cases (precedence, associativity, functions, constants, errors, positions), 13 degree cases, 90+ programmer cases (bases, bitwise, shifts, widths, two's complement, base errors), 7 unit tests (74 units; table sound, round trips, 35+ text queries), 4 percent tests (74 phrases). That is more than the 200 cases the roadmap asks for. `wasm-pack test` (a headless browser) was not run: the crate is pure, so native `cargo test` runs the same code. |
| `cargo test -p sr-server --lib fx_jobs` | 9 passed: parser (hostile and wrong documents), due rule, and the fetcher through `SafeClient` with a stub resolver and transport (default policy fetches the ECB URL; each of three deny entries, an allow list that omits the ECB, and five private or metadata addresses are refused before any request; a 503 is a failure). |
| `cargo test -p sr-server --lib fx_pg_tests -- --include-ignored --test-threads=1` (own scratch database) | 8 passed: first run stores 14 currencies and serves them; second run inside the window makes no request; setting off makes no request and hides the rates, back on serves the kept rates; failure is recorded, old rates go stale at 96 h, threshold setting clears it, success resets the counter; garbled feed changes nothing; older set never replaces a newer one and a dropped currency goes; table CHECKs reject junk. |
| `cargo test -p sr-api` route matrix, `openapi_names`, `routes::tools`, `all_routes_build_without_overlap` | all pass (`KNOWN_CLASHES` stays empty) |
| `cargo test -p sr-admin settings` | 10 passed (new: `tools_currency_settings`) |
| `cargo fmt --check`, `cargo clippy -D warnings --all-targets` on sr-tools-wasm, sr-admin, sr-api, sr-server | clean (exit 0) |
| `vitest run src/tools src/i18n` (maxWorkers 2) | tools: 132 passed in 7 files (39 time zone cases with fixed zones and a fixed date; the six tools rendered against the real wasm engine; 26 currency/parse/registry cases; axe on each tool; currency with MSW including off, stale, empty, failed and "no request but /tools/fx"); i18n: 22 passed |
| `packages/wasm` `vitest run test/tools-calc.test.ts` | 4 passed |
| web `tsc --noEmit`, `eslint src/tools`, `prettier --check`, `scripts/check-css-tokens.mjs` | clean |
| `node scripts/build-web.mjs`, `check-entrance-isolation.mjs`, `check-bundle-budget.mjs` | all green; tool chunks and `tools_bg.wasm` are lazy (nothing reaches a public surface) |

Component tests need `packages/wasm/dist/tools` (they skip without it; `SR_WASM_REQUIRED=1` makes that an error, as for the other wasm tests). I built it with `wasm-pack build crates/sr-tools-wasm --target web --dev --out-dir ../../packages/wasm/dist/tools --out-name tools --no-pack --no-opt`; CI's `pnpm --filter @sideroom/wasm build` does the same for every `*-wasm` crate.

## Merge notes for the integrator

- `crates/sr-api/src/routes/tools/mod.rs` exists on `feat/p32-utl-dev` too. Take utl-dev's file and add `pub mod fx;`, `.route("/tools/fx", get(fx::rates))`, `fx::rates` in `paths(...)` and `fx::FxRatesObject` in `schemas(...)`. The three lines in `routes/mod.rs` are identical on both branches.
- `route_matrix.tsv`, `openapi.json`, `schema.d.ts` and `handlers.generated.ts` need a regenerate after the merge (`SR_UPDATE_ROUTE_MATRIX=1`, then the OpenAPI steps of the brief).
- `registry.ts`: my six entries are separate lines in `TOOLS`; keep one entry per line when merging the others.
- `crates/sr-server/src/app.rs`: four small additions (the `fx_fetcher`/`fx_deps` block after `feed_policy`, `.extension(fx_deps)`, `fx_jobs::register`, `fx_jobs::schedule`).
- `crates/sr-server/Cargo.toml` gains `quick-xml = { version = "0.42.0", default-features = false }` (already in the lock via sr-library and sr-userfeat).
- Job id 1620 is unique (the admin-jobs test greps for duplicates).

## CHANGELOG lines for "Phase 32 - 2026-10-30"

- Utility drawer: calculator, programmer calculator, unit converter, percentage calculator, currency converter and time zone meeting finder (UTL-CALC-001), computed in the browser by the new `sr-tools-wasm` crate.
- `GET /tools/fx` and the `tools.fx_refresh` job: the server keeps the ECB daily reference rates (migration 1620) and the browser never calls a third party; platform settings `tools.currency_rates` and `tools.currency_stale_hours`.

## Still open

- **People's time zones.** The server does not expose another member's `timezone` (the field exists on accounts but no API returns it to others), so the time zone tool's "people" are names and zones you type, kept in this browser. See the owner question.
- **Search phrases.** `apps/web/src/tools/corpus.json` belongs to utl-search; the six tools need their 10 to 30 task phrases there (for example "convert miles to km", "what is 15% of", "meeting across time zones", "hex to binary"). Until then they are found by name and aliases only.
- **`wasm-pack test`** in a browser was not run (see Tests); the native cargo tests cover the same code.
- No browser check was done (ports belong to others this wave); the tools are covered by jsdom component tests with axe, not by a screenshot.

## Decisions (conservative defaults, easy to change)

- `%` after a value is "divide by 100" (`50%`); `10 % 3` (an operand follows) is a modulo; `mod` is always a modulo. Implicit multiplication has the precedence of `*` (`1/2pi` is `(1/2)*pi`).
- Results show 12 significant digits; scientific notation outside 1e-6 to 1e15.
- A month is 30.4375 days and a year 365.25 days; gallons, cups, pints and fluid ounces are US (the imperial gallon is `impgal`); `mb` typed in lowercase is megabyte, `Mb` is megabit.
- The percent and quick-unit phrase grammar is English only, so the example phrases are data, not translated text; the label says "(in English)".
- Fetch cadence: one fetch per 20 hours after a success, one per hour while failing; stale after 96 hours (the ECB publishes on TARGET working days).
- When the converter is off the currency tool stays in the list and says it is switched off (hiding it from the registry would need the setting before the drawer loads).

## Open owner questions

1. Should members be able to share their time zone (a profile field with a visibility choice) so the time zone tool can pick "people" from a server instead of typing zones? Not built: it would expose a field that is private today.
2. Instances that use an outbound allow list must add `www.ecb.europa.eu` for the rates to refresh; otherwise the converter shows the stale note. Is a line in the operator runbook enough?
3. The percent tool understands English phrases only. Is that acceptable for beta, or should the phrase grammar be localised later?
