# feat/p21-threat-refresh: RDY-SEC-004, 005, 006

## Resume here
Branch `feat/p21-threat-refresh` is complete locally (no migrations used: 1220 to 1229 stay unused, because a settings key with no row takes its catalog default). The integrator should: (1) regenerate OpenAPI, client, mocks and the route matrix on the merged tree (this branch adds no route and changes no schema, so nothing to regenerate here); (2) run Playwright once; the e2e risk review is below; (3) when RDY-SEC-001 lands, update threat-model row T7 (its residual names RDY-SEC-001/002/003) and likewise T2/T10/T11 when RDY-SEC-007/010 land, T4 for RDY-SEC-009, T9 for RDY-SEC-008.

## Done
- **RDY-SEC-005** `scripts/secret-scan.mjs`, `scripts/secret-scan.allow.json` (each entry has a path glob, a pattern and a reason), `scripts/secret-scan.test.mjs` (13 tests: every pattern caught, placeholders ignored, allow-list, value never in a finding, current tree clean). Patterns: private key blocks, AWS key ids, GitHub and Discord token shapes, `SR_*` secrets assigned a literal, stray `.env` files, passwords in URLs and query strings. Reports only `file:line  pattern`. `repo-checks.yml` runs it over tracked files and, on pull requests, over the added lines (`--diff <base sha>`). It passes on the tree; the 14 first hits were CI service-container credentials, test fixtures and the AWS docs key, now allow-listed with reasons.
- **RDY-SEC-006** nine HTTP limits are live platform settings in a new `abuse` group of the Admin > Limits catalog (`crates/sr-admin/src/settings.rs`), applied by `crates/sr-api/src/rate_limit.rs` (`Abuse`, `ABUSE_ROUTES`, `RateConfig::abuse`, `check_scoped`): friend requests, DM opens, webhook execute (per webhook token across callers), OAuth2 authorize, interaction callbacks, watch-party joins, emoji uploads, sticker uploads, appeal attempts. Two more are read from the database per request: data-package cooldown (`crates/sr-db/src/data_export.rs`) and counter-notices per staff per day (`crates/sr-moderation`). Reports per day (`moderation.reports_per_day`), appeals per ban (D-215, structural) and invite-code checks (fixed 10/min, 50/day) already existed and are asserted and listed. Doc 05 "Abuse limits" has the table; `crates/sr-api/tests/abuse_limits.rs::doc_05_table_matches_the_catalog_and_its_tests_exist` keeps it honest. Admin shows the group (`apps/web/src/i18n/messages/en/p21-threat-refresh-1.ts`, `p21abuse.test.tsx`).
- **RDY-SEC-004** `docs/15-threat-model.md` rewritten: 27 rows (T1 to T18 reworked, T19 to T27 new: payments, bot gateway and OAuth2, member-writable abuse, webhook execute, Web Push, SDK voice bot, moderation register and takedown notifier, data-subject exports, Person graph and suspicion signals); new assets and actors. `scripts/threat-model.test.mjs` checks each named file exists, each `file::test` finds the test, and every "open" names an RDY item (about 100 file::test references, all resolved). Stale claims corrected: ENT-INV-010 is done, XC-SEC-003's shared client exists (two fetchers, `sr-net` and `sr-unfurl`), T6 is an accepted owner decision (answer 3).

## Edits outside my files (small, flag for the integrator)
- `crates/sr-server/src/app.rs`: the starting `RateConfig` now uses `..d` and loads the abuse figures from the stored settings.
- `crates/sr-watchparty/src/service.rs`: the join backstop went from 20 to 120 a minute so the setting (max 120, default 20) is the effective limit.
- `crates/sr-moderation` (Limits struct, counter-notice count) and `crates/sr-db/src/data_export.rs` (cooldown from the setting); tests added in each crate.
- The default static routes for `POST /oauth2/authorize` and the interaction callback were removed from `RateLimitConfig::default()`; the live figures reproduce the same defaults (20 a minute; 60 a minute as a burst of 5 per 5 s).

## Tests run
- `cargo test -p sr-api -p sr-admin -- --include-ignored --test-threads=1` (scratch DB `sr_p21_threat`, both env vars): all pass (sr-api lib 661, `abuse_limits` 4, route_matrix, openapi_names).
- `cargo test -p sr-moderation`, `-p sr-watchparty`, `-p sr-db --test data_export` with `--include-ignored --test-threads=1`: pass (new tests: counter-notice limit, cooldown setting).
- `cargo fmt --all`, `cargo clippy -j 2 -p sr-admin -p sr-api -p sr-moderation -p sr-db -p sr-watchparty -p sr-server --all-targets -- -D warnings`: clean.
- `node --test scripts/secret-scan.test.mjs scripts/threat-model.test.mjs`, `node scripts/secret-scan.mjs`, `node scripts/check-doc-links.mjs`: pass.
- Web: typecheck, eslint, prettier (local binary) clean; full vitest and build/budget: see the last section.

## Playwright risk (the integrator runs it)
Low. Reviewed specs: `lounge-shell.spec.ts` sends one friend request; `message-components.spec.ts` only lists DMs; `activities-live.spec.ts` makes one authorize call; `lounge-gaps.spec.ts` creates a webhook but never executes it; emoji uploads appear only in `storage-live.mjs` (a handful). Defaults (friend 60/h, DM 40/h, authorize 20/min, emoji 50/h, sticker 20/h) are far above that, and counters are per session. Limiter identity is per session or token, so each spec login starts fresh. Retries cannot exhaust them.

## Not done / decisions
- Appeals per case is not a setting: one appeal per ban is decision D-215 (unique `ban_id`); the attempts-per-address limit is the setting. Invite-code, login and registration throttles remain code constants (`routes/auth`, not my files); listed in the table as fixed.
- Limits are per process and per credential, documented; no cross-node sharing.
- OpenAPI regeneration not run: no routes or schemas changed.
- gitleaks stays an optional owner addition.

## Web gates (run)
Full vitest once (`--maxWorkers=2`): 2501 pass, 1 fail: `src/lounge/__tests__/msgActions.test.tsx` "lists members, roles and channels in the select menus" (MSW unhandled request; Lounge code I did not touch; also fails when run alone, so it is not load. Not checked against the base commit). Build and `scripts/check-bundle-budget.mjs`: pass. Typecheck, eslint, prettier: clean.
