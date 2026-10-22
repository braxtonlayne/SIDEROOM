# Handoff: feat/p19-mod-dash (moderation step 4, staff dashboards)

Base: integration/phase19 (= main). Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p19-mod-dash`, cargo target `p19-mod-dash-target`. Migration used: `1140_moderation_dashboards.sql` (range 1140-1149). No new job ids, no new audit action ids (the new access-log actions are strings: `moderation.dashboard`, `moderation.inbox.list`, `moderation.inbox.dismiss` for denials; dismissals write the existing `moderation.case.state` row per case). No new dependencies, no downloads.

## Done

All ticked in `docs/inventory/19-moderation.md`; the design is in the new spec section 17 (`docs/specs/moderation-compliance.md`).

- **MOD-BAN-003** Bans list filters. `GET /admin/bans` takes `category` (severity), `reason_category` (a spec 8 value, or `none` for bans made without one; unknown value = 400), `older_than_days`, `newer_than_days`. `BanRecord` now carries `reason_category` (it was stamped on the table by the case ban flow but never returned). Admin > Bans has three filters and shows the reason badge. The repository trait takes a `BanFilter` (Postgres and in-memory implementations).
- **MOD-DASH-002** Reports inbox. `GET /moderation/dashboard/inbox` (cursor, oldest first) and `POST /moderation/dashboard/inbox/dismiss {case_ids, reason}` (max 100, reason required, each case through `set_state` so it gets its timeline event, report-hold release and access-log row; cases that left the inbox are returned as `skipped`). `MANAGE_CASES`. Never returns the reporter, the note or the reported person.
- **MOD-DASH-005 / -006** Dashboard cards for the takedown register (to decide, past triage target, counter-notices pending and restorable, received/removed in window) and the data requests (open exports/erasures, overdue honouring the extension, due in 7 days, completed in window), `MANAGE_TAKEDOWNS`, with links to the existing register tab and queue page.
- **MOD-DASH-007** Bans in force by reason (incl. `none`), severity and four age buckets, lifted in window, `PLATFORM_BAN`.
- **MOD-DASH-008** Health: reports per day (UTC, zero days filled), cases opened, median seconds to first action, cases still waiting, closed and reopened (reopen rate 0..1), appeal outcomes and appeals waiting. Totals only, no per-staff figure.
- **MOD-DASH-009** Retention check: due messages kept by a hold or a held attachment, removed attachments kept, large attachments past expiry kept, held draft/rejected submissions, holds in force and holds due for review. Each count capped at 10 000 and flagged `capped` ("at least").
- **MOD-DASH-010** Age and child-safety metrics: age-restricted channels/series/titles, age-confirmation resets in window (new `age_gate_resets` counter, account-free, written in the same statement as `reset_age_confirmed`), open child-safety cases, child-safety reports in window, active child-safety bans.
- **Dashboard tab** in Admin > Moderation. One endpoint, `GET /moderation/dashboard?days=` (1-90): the sections follow the caller's bits (`MANAGE_CASES`: inbox, health, retention, age; `PLATFORM_BAN`: bans; `MANAGE_TAKEDOWNS`: takedowns, data requests); a caller with none gets 403 naming `MANAGE_CASES` and the denial is logged. The tab is shown to `MANAGE_CASES` and `MANAGE_TAKEDOWNS` holders.

## Cheap and private by construction

Migration 1140 adds the indexes the aggregates need (cases `opened_at`/`closed_at`, partial inbox index, reports `created_at`, appeals `created_at`/`decided_at`, bans `created_at`, case-event `kind`+`created_at`, `takedown_notices.removed_at`, `data_requests.completed_at`, partial indexes on `channels.nsfw`, `reading_series.age_restricted`, `video_titles.age_restricted`) and the `age_gate_resets` table. Every query is bounded: a window of at most 90 days on an indexed timestamp, a small partial set, or a capped count. Health and the inbox cover only cases platform staff may see (routing platform/reviewers, D-209). A Postgres test searches the inbox answer for the reporters' ids and notes, and another that the health figures contain no staff id.

## Decisions (conservative defaults, flag if you disagree)

1. **"Unattached reports"** has no literal meaning (every report has a case). Read as: reports in state `received` whose case is still `open` with nobody assigned. Duplicate grouping = one group per case (a case is one target), group size = number of reports.
2. **Dismiss reuses the case state change**, so reporters' report rows are not closed (same as dismissing one case by hand today).
3. **Submissions in the retention check** count all held draft/rejected submissions, not only those past the idle limit (the limits are settings read elsewhere).
4. **Moderation stays hidden from `PLATFORM_BAN` alone** (an existing test pins this). So a ban officer without a case or takedown bit does not get the Dashboard tab; they have the filtered Bans page. The ban figures show on the Dashboard for anyone who also holds `MANAGE_CASES` or `MANAGE_TAKEDOWNS`. If you want ban figures on the Bans page itself, it is one `getDashboard()` call away.
5. The Dashboard tab is first in the tab bar but the default tab is unchanged (Cases for case staff, Takedowns for clerks).

## Still open

- `MOD-AGE-003` (staff reset of the age confirmation) is not built; the counter already accepts source `staff`.
- Retention check does not count guilds waiting on held messages, or library/cinema rows held by a takedown (those are not purge-job skips).
- No Playwright/browser check (not allowed in this wave); the UI is covered by Vitest.

## Tests run

- `cargo fmt --all --check` ok; `cargo clippy -p sr-admin -p sr-db -p sr-moderation -p sr-api --all-targets -j 2 -- -D warnings` ok.
- Postgres (own scratch DB `sr_p19_moddash` and TestDb clones, all dropped afterwards): `sr-moderation` all suites (28 unit, 8 dashboard, 14 db, 16 takedowns), `sr-db` `admin_repo` (10, with the filter signature), `p13_settings_admin` (6, incl. the age-reset counter), `migration_versions` (6).
- `sr-moderation --test dashboard` (Postgres, own TestDb databases): 8 passed (inbox grouping/privacy/paging/claim, bulk dismiss, sections per bit, health with first action and reopen, retention with a hold and review flag, bans by reason/severity/age plus the `PgAdminRepo` filters, age metrics, register and data requests). `sr-moderation` unit tests, `sr-admin` (61, incl. `the_bans_list_filters_by_severity_reason_and_age`).
- `sr-api --lib routes::moderation routes::admin --include-ignored --test-threads=1`: 41 passed (new: access matrix rows, denial log, bad inputs, `the_dashboard_sections_follow_the_bits_over_http`, `/admin/bans` filters).
- `route_matrix` (5, no `?`), `openapi_names` (`KNOWN_CLASHES` empty): passed.
- Web: `pnpm -r typecheck`, `pnpm -r lint`, prettier on the touched files, full Vitest once (`--maxWorkers=2`): 274 files, 2468 tests passed (new `p19dashboard.test.tsx`, 5 tests); `mocks:check` up to date (944 operations); web build and `check-bundle-budget` ok (admin 358.9 of 600 KB gzip).
- Not run: the whole-workspace `cargo test`/`clippy` (limited to touched crates by the brief), Playwright (not allowed this wave).

## Open owner questions

1. Is the reading of "unattached reports" (open and unassigned) what you meant?
2. Should `PLATFORM_BAN` alone see the Moderation section (with only the Dashboard tab)? Today an existing test says no.
