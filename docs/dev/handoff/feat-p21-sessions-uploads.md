# Handoff: feat/p21-sessions-uploads (wave 21: RDY-SEC-007, 008, 009, 010)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p21-sessions-uploads`, branch `feat/p21-sessions-uploads` (base `integration/phase20`). Local commits only: nothing pushed, no PR, nothing merged. Cargo target dir `C:/Users/braxt/Desktop/SIDEROOM_wt/p21-sessions-uploads-target`, scratch database `sr_p21_sessions` (dropped at the end; the TestDb template `sr_tpl_158_1230_b23a52a3` it created is still on the server, because Postgres refuses to drop a template without `ALTER DATABASE ... is_template false`; it is reused by the next run). Migrations: 1230 only; 1231 to 1239 unused.

## Resume here

All four items are built, run and ticked in `docs/inventory/20-launch-readiness.md`. Nothing is half done. What is left is the integrator's work (regenerate the generated files on the merged tree) and three things that need a person (see "Open"). No server, Vite, cargo or Vitest process of this pass is running; the ports 8080 and 5173 were used once (PIDs stopped by number) and are free.

## Done

### RDY-SEC-010 Sensitive-action reauth list

- `crates/sr-api/src/sensitive.rs` (new): `SENSITIVE_ACTIONS`, 27 rows in the eight kinds of the roadmap (staff appointment, platform ban, data-request outcome, takedown uphold, secret rotation, owner transfer, application secret reset, payout descriptor change) plus the account-takeover and announcement routes that already asked for the window. Gate kinds: `StaffSecondFactor` (a route layer, `reauth_gate`, wired in `build_router`), `AccountPassword` (the developer portal's secret and token resets, which already ask for the password), `HostOperator` (`sr-cli rotate-secret-key`).
- Routes that did **not** enforce it before and now do: `POST /admin/accounts/{id}/ban`, `POST /admin/persons/{id}/ban`, `POST /moderation/cases/{id}/ban`, `POST /moderation/takedowns/{id}/uphold`, `POST /admin/guilds/{id}/transfer-ownership`, `PUT /admin/donations/wallet`, `POST /admin/data-requests/{id}/decline` (the `run` route asked only for exports and erasures; it now asks for every kind).
- `SR_ADMIN_REQUIRE_REAUTH=0` goes through `sensitive::require_reauth_from_env`, which reuses `sr_core::config::is_development_host` (D-357); `sr-server/src/app.rs` fails startup with a message naming the variable on any other host.
- Web: the moderation and donations admin clients now go through `withReauth` so the new refusals open the re-auth dialog instead of an error.
- Proof: `routes::admin::tests::reauth_list_tests` walks every staff row through the real router (closed window for an enrolled and a non-enrolled staff account, open window, expired window); `sensitive::tests` (every listed route is a `staff` or `owner` row of `route_matrix.tsv`, ids unique, the env rule on dev and non-dev hosts). Doc 07 section 6 carries the table, printed by `cargo test -p sr-api sensitive::tests::print_table -- --nocapture`.

### RDY-SEC-009 CSP violation reporting

- Migration `1230_csp_reports.sql`; `sr_db::csp_reports::PgCspReports` (counter per directive, blocked origin and document path; 200 keys per kind, the rest under `(other)`).
- `POST /internal/csp-report` (`crates/sr-api/src/routes/internal/`): unauthenticated, 30 reports a minute per address (burst 10) and 600 a minute in all, body capped at 8 KiB, both report formats, 204 on success, 400/413/415/429 otherwise. Stored: the directive name, the origin (scheme, host, port) of the blocked URL or a keyword, the document path with the query, fragment and host dropped and id-like segments folded to `:id`. Nothing else of the report is kept (not the referrer, sample, source file, user agent or address).
- `GET /admin/system/csp-reports` (`OPERATOR`) and the Admin > System card "Blocked content reports" (`CspReportsCard.tsx`, strings in `p21-sessions-uploads-1.ts`).
- `csp.ts` adds `report-uri /internal/csp-report; report-to csp-endpoint` and the `Reporting-Endpoints` header; `Caddyfile.prod` carries the same line plus `@cspReport` (POST to exactly that path is proxied; the rest of `/internal/*` stays a 404); the dev `Caddyfile` and both Vite proxies forward it. `INSTALL.md` states the one exception.
- Proof: `crates/sr-api/src/routes/admin/csp_report_tests.rs` (post as anonymous, read as operator, no personal data, caps, rate limit, 401/403), unit tests in `routes/internal/csp.rs`, `crates/sr-db/tests/csp_reports.rs` (Postgres), `csp.test.ts`, `p21cspReports.test.tsx`. Live: with `sr-server` on 8080 and Vite on 5173 the dev response carried the directive and the header, and a report POSTed through the Vite proxy landed in `csp_report_counters` (`/lounge/123456789012345?x=1` became `/lounge/:id`).

### RDY-SEC-007 Rotation and compromise drills

- `crates/sr-cli/tests/drills.rs` (Postgres, through the real `sr-cli` binary): `drill_lost_or_suspect_secret_key` (rotation refused without the old key and left the rows readable; rotation with it; every TOTP secret and the sealed interactions seed open under the new key alone and no longer under the old one; a second run is harmless), `drill_verify_access_log_detects_tampering` (an edited row and a deleted row each exit 1 with the row number, restoring the row makes it pass again), `drill_compromised_staff_account` (force-logout, `user reset-2fa`, `staff revoke`, bot-token rotation, `verify-access-log`). Each command is echoed as `DRILL $ sr-cli ...`.
- `deploy/RUNBOOK.md`: "Lost or suspect secret key" rewritten and a new "Compromised staff account", quoting those commands and their expected output.

### RDY-SEC-008 Upload hygiene fuzz

- Hand-rolled seeded generator (no `proptest` needed; it is only in `sr-core` and `sr-markdown`). Tests: `crates/sr-library/tests/upload_hygiene.rs` (archive inspector and page pipeline), `crates/sr-storage/tests/upload_hygiene.rs` (image re-encoder, surface allowlist, size policy), `crates/sr-packages/tests/upload_hygiene.rs` (package zip reader, emoji, sticker and sound pack validators), `crates/sr-chat/tests/sound_hygiene.rs` (the soundboard reader; it lives in `sr-chat`, so the test does too). They cover each limit at the limit and one over, zip bombs, nested archives, polyglots (PNG, JPEG, GIF, WebP with a ZIP or HTML behind them; archives behind a PNG, GIF or HTML), every truncation prefix, thousands of random corruptions, header lies about dimensions and sizes, every forbidden name segment.
- **Defect found and fixed:** `sr_chat::settings::soundboard::ogg_duration` multiplied an attacker-chosen granule position by 1000 unchecked. A debug build panicked; a release build wrapped, and a granule such as 18446744073709553 read as a one second sound. Now `checked_mul`. Regression test: `headers_that_overflow_the_arithmetic_are_refused_never_wrapped_into_a_short_sound`.
- No other crash or over-limit acceptance turned up, including RAR (the UnRAR C library) behind a valid magic with random bytes.
- Doc 12 section 3 "Upload hygiene" now has the per-surface limit table, and each crate's `the_doc_table_matches_the_code` test fails when a number in the table and the code drift.

## Files outside the Split row that had to change (all small)

`crates/sr-api/src/router.rs` (the layer), `crates/sr-api/src/lib.rs`, `crates/sr-api/src/routes/mod.rs`, `crates/sr-api/src/routes/admin/{mod,system,tests}.rs` (route, test hooks), `crates/sr-server/src/{app,lib}.rs` (env rule, counters wiring), new `crates/sr-server/src/csp_reports.rs`, `crates/sr-db` (migration, `csp_reports.rs`, `lib.rs`, test), `crates/sr-chat/src/settings/soundboard.rs` (the fix), `crates/sr-cli/Cargo.toml` (dev-dependency on the in-workspace `sr-apps`), `apps/web/vite.config.ts`, `apps/web/src/admin/{api.ts,donations/api.ts,sections/System.tsx}`, `apps/web/src/moderation/api.ts`, `deploy/INSTALL.md`, `deploy/compose/caddy/Caddyfile`. Generated files were regenerated on this branch (`route_matrix.tsv`: one new `staff` row and the `#undocumented` row for `/internal/csp-report`; OpenAPI, the api-client schema, the MSW mocks); the integrator regenerates them on the merged tree. `KNOWN_CLASHES` is still empty.

## Decisions (proposed for the log; the numbers are the integrator's)

1. **The reauth gate runs before the handler's own permission check.** A staff member without the permission for a listed route meets `900004` first, and `403` after re-authenticating. The alternative (a gate inside each handler after its `authorize`) cannot be proven by one test over the list. The private-content routes keep their own gate because it logs the refusal as `denied`.
2. **`/internal/csp-report` is public on the main listener**, rate limited by its own limiter (per process, like the others), and Caddy forwards only `POST` to that exact path.
3. _(Superseded by D-364 / R-12: the stripper now stops at the first end marker and drops the MPF index.)_ **JPEG trailers are kept by the chat-attachment metadata stripper** (multi-picture files keep their second image there); attachments are typed from their first bytes and served as downloads or under `nosniff`. Everything that is re-encoded loses its trailer. Documented in doc 12.
4. _(Superseded by D-363 / R-11: the larger of the header walk and the claim; the client clips at 5.25 s.)_ **Sound duration is the container's claim** (Ogg granule, WAV data size, MP3 bitrate or Xing count). Nothing is decoded, so a lying file can claim to be short; only decoding closes that, which is a new dependency.

## Tests run (all on this branch, scratch database `sr_p21_sessions`)

- `cargo test -p sr-api -- --include-ignored --test-threads=1`: lib 669 passed (including the Postgres ones), then 18, 23, 31, 2, 5 and 16 in the integration test binaries (`route_matrix`, `openapi_names` among them): all green.
- `cargo test -p sr-api --lib -- sensitive reauth_list csp_report internal`: 15 passed (before the full run).
- `cargo test -p sr-api --test route_matrix --test openapi_names`: 5 + 2 passed.
- `cargo test -p sr-db --test csp_reports --test migration_versions -- --include-ignored --test-threads=1`: 2 + 1 passed.
- `cargo test -p sr-cli --test drills -- --include-ignored --test-threads=1`: 3 passed (transcript with `--nocapture`).
- `cargo test -p sr-storage --test upload_hygiene`, `-p sr-library --test upload_hygiene`, `-p sr-packages --test upload_hygiene`, `-p sr-chat --test sound_hygiene`: 12, 11, 10 and 6 passed.
- `cargo test -p sr-core --test deploy_files`: 15 passed (the Caddyfile edits).
- `cargo test -- --include-ignored --test-threads=1` on sr-chat (325 + 6), sr-storage (54 + 3 + 7 + 6 + 12), sr-library (131 + 11), sr-packages (21 + 11 + 10, the 11 are the Postgres suite) and sr-cli (22 + 3 + 7), and `cargo test -p sr-server --lib` (63 passed, 65 Postgres tests ignored: not run, see Open 5): all green.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` on sr-api, sr-db, sr-server, sr-cli, sr-chat, sr-storage, sr-library, sr-packages: clean.
- Web: `tsc --noEmit`, `eslint` (admin, moderation, security, i18n, vite config), `prettier --check` on every touched file with the repo's own copy, `check-doc-links.mjs`, `check-ops-files.mjs`: clean. Full `vitest run --maxWorkers=2` (wasm built first): 2503 of 2504 passed in 281 files; the one failure was `p20preflight` finding two `alert` elements because the new card raised its own error in a test that does not mock its endpoint. The card now shows a quiet note on a failed read (no alert role, tested), and the whole `src/admin/__tests__`, `src/security` and `src/i18n` directories were re-run green (336 tests). `vite build` and `scripts/check-bundle-budget.mjs`: all budgets ok (admin 363.9 of 600 KB gzip).

## Open

1. _(Answered, D-365 / R-13: hand check written down in doc 12 section 7.)_ **Real-browser delivery of reports is not observed.** Playwright's headless Chromium, headless Edge and headed Edge (with the default flags that disable background networking removed) never sent a report within 90 seconds although the console showed the violations; the Reporting API is known to be flaky under automation. The server side was proved by posting both formats directly and through the Vite proxy. Someone with an ordinary browser should open the dev site once, trigger a violation (`document.body.insertAdjacentHTML('beforeend','<img src="https://blocked.example.test/x.png">')`) and read the card, or the table `csp_report_counters`.
2. _(Answered, D-365: documented with the hand check.)_ Dev mode is Report-Only and the dev page already logs inline-style violations of `style-src 'self'` in the console, so a browser that does deliver reports will fill the counters with those on a developer machine. The production policy is untouched by this.
3. _(Answered, D-362 / R-10: yes; sequence now tested and shown.)_ Owner question: is decision 1 (second factor first, permission second) acceptable? It leaks only that a route is gated.
4. _(Answered, D-363 / R-11: no decoder; header walk plus client clip.)_ Owner question: decode sounds for real durations (a new dependency) or keep the container's claim?
5. `crates/sr-db` full suite and `sr-server` Postgres suites were not re-run in full (the migration only adds a table); the integrator's full gate covers them.

## Round 2 (D-362 to D-365, R-10 to R-13)

- R-10: `reauth_list_tests::the_gate_comes_first_then_the_handlers_refusal_and_never_for_non_staff` (closed window `900004`, open window plain `403` plus a `denied` log row, non-staff never `900004`); `p21withReauth.test.ts` (prompt once, retry once, the retry's 403 propagates); doc 07 cites D-362.
- R-11: `mp3_walked_ms` and `opus_walked_ms` in `soundboard.rs`, `max(walked, claimed)`, saturating sums; tests for a Xing header claiming 10 frames over 300 and an Opus granule of 1 s over 400 packets; CBR MP3 now measures by the walk (192 frames = 5015 ms, was 5004 from the size); `src.start(0, 0, 5.25)` in `mic.ts` and `soundPlayer.ts`, asserted in `soundboard.test.ts` and `media.test.ts`.
- R-12: `strip_jpeg` stops at the first `FF D9` and drops the MPF APP2; unit test in `exif.rs`; `upload_hygiene.rs` JPEG half flipped, `jpeg+zip` and `jpeg+html` added; doc 12 sentence replaced.
- R-13: doc 12 section 7 bullet.
- Postgres suites skipped before were run: see the results line below.
- Round 2 results: sr-server `--include-ignored --test-threads=1` 128 passed; the full sr-db `--include-ignored` suite green (every binary ok); sr-api lib, sr-chat (325 + 8), sr-storage (55 + 12) green; clippy clean; full vitest 2506 of 2506 in 282 files. Scratch database dropped.
