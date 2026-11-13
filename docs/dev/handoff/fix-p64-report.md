# Wave 64 report: handoff (SEC-AUD-166, SEC-AUD-167: the beta bug reporter, server side)

Branch `fix/p64-report`, one commit `SEC-AUD-166, SEC-AUD-167: ...` on top of `integration/phase64` (9949a4ba). Nothing pushed.

## Done
**SEC-AUD-166**
- Switch `platform.beta_reports` (`BETA_REPORTS_ID`, `sr-features/src/wave_modules.rs`): platform scope only, client visible, off by default, section "Study" (where the Showroom is; there is no "Platform" section), the name and description of the task. `feature_registry.rs` row added; the web fixture `apps/web/src/admin/features/__fixtures__/registry.json` regenerated (`SR_UPDATE_FEATURE_CATALOG=1`, one new entry; it is outside my crates, the test demands it).
- Gate: `POST /bug-reports` has a `module_gate` rule that answers **404 `SR_NOT_FOUND`** while off (a `hidden()` rule, like the Requests board, the model the task names). The staff routes have no rule.
- Migration `2790_bug_reports.sql` (header says it is personal data and why). `reporter_id` FK `accounts ON DELETE CASCADE`, `closed_by` FK `ON DELETE SET NULL`; checks on status, description length 1..2000 and picture size; the two indexes.
- Allow-list `routes/bugs/allow_list.rs` (`sanitize(&Value, now) -> Result<Clean, Reason>`); `is_commit`, `clean_route`, `clean_caps` made `pub(crate)` in the crash allow-list and reused.
- Route `POST /bug-reports` (`routes/bugs/mod.rs`): `SessionOnly`, `DefaultBodyLimit` 3 MiB, `BugLimiter` (5 per account per hour, 100 per day platform), 201 `{id}`. The limiter is applied AFTER sanitizing, so a refused report (a bad picture) does not use up one of the five. Bots are denied in `auth/policy.rs`.
- Store trait `BugStore`, Postgres adapter `sr-db/src/bug_reports.rs` + `sr-server/src/bug_wiring.rs`, wired in `app.rs`.

**SEC-AUD-167**
- `sr-admin/src/service_ext.rs`: `authorize_bug_list` (`STAFF_BASIC`, no entry), `authorize_bug_read` (`bug.report.read`), `authorize_bug_screenshot` (`bug.screenshot.read`), `authorize_bug_close` (`bug.report.close`); target `("bug_report", id)`. The entry is written BEFORE the data is returned and `append_log(...).await?` fails the request if it cannot be written (same as the crash read; no switch).
- Staff routes as in the contract (see shapes below). The close route answers 204 and deletes the picture; it is idempotent (keeps the first closer and time).
- Retention: job `bugs.retention` (`sr-server/src/bug_report_jobs.rs`, singleton id **2550**, daily 03:40 UTC) calls `PgBugReports::purge(30, 90)`: pictures older than 30 days nulled, rows older than 90 days deleted.
- Erasure: `DELETE FROM bug_reports WHERE reporter_id = $1` in the personal-state list of `account_deletion.rs` (counts in `personal_rows_deleted`). Export: `bug_reports.json` (id, created_at, status, route, commit, description, bundle, `has_screenshot`; no bytes) plus a README line.
- OpenAPI and generated client regenerated (`openapi.json`, `schema.d.ts`, `handlers.generated.ts`); `route_matrix.tsv` has the five rows; `authz_matrix.rs` seeds a report (`{bug_id}`), turns the switch on, sends a valid body, and the kill-switch test has a `platform.beta_reports` case.

## Final request and response shapes (reportui part 2 reads this)
- `POST /api/v1/bug-reports` body as the plan's contract. Server-side leniency: `bundle` may be missing; a bad `commit` is stored as `0000000`, a bad `route` as `unknown`, a bad `locale` as `und`, a theme outside `editorial|lounge-light|lounge-dark` as `other`; `gateway.status` outside `connected|connecting|reconnecting|closed` drops the whole `gateway`; `gateway.reconnects` and `heartbeat_ms` optional; `caps.engine` accepts the crash set `chromium|firefox|webkit|other` (anything else, including `blink`, becomes `other`); `method` is `GET|POST|PUT|PATCH|DELETE|?` and an entry with another method is dropped; a failed request needs `method`, `path` and `status` 100..599, `ms` and `ago_s` default to 0. Stored bundle also holds `masked` when a picture was sent.
- Answers: 201 `{ "id": "<string>" }`; 400 (`50035` with field `description` or `screenshot`) for a missing/empty/over-2000-character description, a picture that is not `image/jpeg`, bad base64 or not a JPEG (start `FF D8 FF`, end `FF D9`); 404 while the switch is off; 413 for a body over 3 MiB or a decoded picture over 2 MiB; 429 over the limits.
- `GET /admin/bug-reports?status=open|closed&limit=` -> `{ "reports": [{ id, created_ms, status, route, commit, reporter: { id, username }, has_screenshot }], "open_count": n }` (an object, as `contract.ts` expects; the plan's "[...] plus open_count" is read that way). 400 for another status. `open_count` counts all open reports.
- `GET /admin/bug-reports/{bug_id}` -> the row plus `description`, `bundle`, `closed_ms` (number|null), `closed_by` (`{id, username}`|null).
- `GET /admin/bug-reports/{bug_id}/screenshot` -> bytes, `Content-Type: image/jpeg`, `Content-Disposition: attachment; filename="bug-<id>.jpg"`, `X-Content-Type-Options: nosniff`, `Cache-Control: no-store`; 404 when there is none.
- `POST /admin/bug-reports/{bug_id}/close` -> 204 no body (`apiFetch<void>` must tolerate 204; the web client sends `{}`).
- The path parameter is `{bug_id}` (not `report_id`) so the authz matrix, which keys ids by parameter name, does not hand it a crash id.

## Departures from the task text, and why
- 404 (hidden rule) rather than 403 `SR_FEATURE_DISABLED` while off (see above); change `.hidden()` in `module_gate.rs` if 403 is preferred.
- Failed-request paths: besides the regex and the 6-digit rule, any piece that `id_like_piece` flags (5 digits, long hex, long mixed runs) and any `@` other than a whole `@me` segment drops the entry (an e-mail shaped path passes the plain regex).
- No `require_reauth` on the close route (the crash routes have none and the plan names none).
- Not tested locally: the full `authz_matrix` main test (run only the kill-switch case), a failure to write the audit entry (the in-memory repo cannot fail an append; the code path is `?`).

## Not done
- Nothing from the task is dropped. Owner-facing docs (threat model, inventory, decisions log) are proposals below.

## Tests (all run, passing, with `-D warnings` clippy on sr-api, sr-db, sr-admin, sr-features, sr-server `--all-targets` clean)
- sr-api `routes::bugs::allow_list::tests` (9): the contract round trip, `a_bug_report_keeps_only_allow_listed_bundle_fields`, `a_failed_request_path_with_an_id_is_dropped`, `a_picture_that_is_not_a_jpeg_is_refused`, `a_picture_over_the_size_is_refused`, `a_description_is_required_and_capped`, clamps, caps on counts, no-picture.
- sr-api `routes::bugs::tests` (19, real router, in-memory store, in-memory admin repo with its access log): `a_bug_report_before_sign_in_is_refused`, `a_bug_report_is_refused_while_the_switch_is_off`, `a_report_without_a_picture_is_accepted`, `the_sixth_report_in_an_hour_is_refused`, the same names as above at route level, `staff_without_an_appointment_cannot_list_bug_reports`, `opening_a_bug_report_writes_an_audit_entry`, `opening_a_screenshot_writes_an_audit_entry_and_serves_a_download`, `closing_a_report_deletes_its_picture_and_writes_an_audit_entry`, `the_list_carries_no_description_and_no_picture`, `staff_can_read_reports_while_the_switch_is_off`, limiter and download-header tests.
- sr-admin `tests_ext`: 4 tests (authorize_bug_* and their entries, `denied` rows for a caller without the basic bit).
- sr-db `--ignored` (scratch DB): `tests/bug_reports.rs` (4: round trip and table checks, close, `retention_drops_pictures_after_30_days_and_reports_after_90`, `erasing_an_account_deletes_its_bug_reports`), `account_deletion` (existing test extended: a report with a picture, personal rows 9 -> 10), `data_export` (extended: `bug_reports.json`, own rows only, no bytes).
- sr-server: `feature_registry` (7), `authz_matrix module_kill_switches_stop_their_routes --ignored`; sr-api `route_matrix` (6) and `openapi_names` (2); sr-features (140).
- **Watched fail without the change:** with `sanitize` bypassed (the raw bundle merged into the stored one) `a_bug_report_keeps_only_allow_listed_bundle_fields` failed at both levels (allow-list and route: "a secret was stored"); restored afterwards. The first route-level runs also failed (404) until the router was mounted, which shows they reach the real routes.

## Proposed inventory rows
- SEC-AUD-166: Beta bug reporter, server: switch `platform.beta_reports` (off by default), table `bug_reports` (migration 2790), `POST /bug-reports` (session only, 3 MiB, 5/account/hour, 100/day), allow-list rebuild of description, bundle and JPEG (magic bytes checked, refused not dropped). Tests: `routes::bugs::*`, `feature_registry`.
- SEC-AUD-167: Beta bug reporter, staff and lifecycle: four `STAFF_BASIC` routes with audit entries `bug.report.read`, `bug.screenshot.read`, `bug.report.close` written before data is returned; picture deleted on close or after 30 days, report after 90 (job `bugs.retention`, id 2550); erased with the account; in the data export. Tests: `routes::bugs::tests`, `tests_ext`, sr-db `bug_reports`, `account_deletion`, `data_export`.

## Proposed threat-model rows
1. The allow-list: the server rebuilds every report from named fields (closed enums, clamped numbers, path templates with no query, id-like piece, e-mail shape or 6-digit run); unknown fields are never read; the description is the member's own text (control characters removed) and is the one free-text field, shown to staff as text only.
2. The picture is an inert download: only `image/jpeg` with JPEG markers is stored (never rendered server side), served as `attachment` with fixed `Content-Type`, `nosniff` and `no-store`; a non-JPEG is a 400, so the member knows.
3. Who reads, and the trail: any appointed staff (`STAFF_BASIC`); open, picture and close each write a non-switchable platform audit entry before the data is returned; a write failure fails the read; the list carries no description or picture. Staff routes stay open while the switch is off.
4. Retention and erasure: picture gone on close or at 30 days, row at 90, row deleted with the account, listed in the export (without bytes). Residual risk: a picture exists up to 30 days in Postgres (and in its backups); staff can download it.

## Changelog sentence
"Operators can turn on a beta bug reporter: members send staff a description, technical details and an optional screenshot they checked; staff read and close the reports in Admin, pictures are deleted when a report is closed or after 30 days."

## Owner questions
1. While the switch is off, should `POST /bug-reports` answer 404 (as built, like the Requests board) or 403 "feature disabled"?
2. Should closing a report require the second-factor window (`require_reauth`), as other staff writes do? Not built, to match crash reports.
3. Alerting: no card in the Staff Server (not in this build, as the plan says); the Admin badge uses `open_count`.

## Cleanup
Scratch database `sr_p64_report` dropped and `p64-report-target` deleted (see the final reply).
