# Handoff: feat/p18-mod-tkd (M-25 (b), D-293, D-315)

Base `feat/p18-mod-cases` (step 1: cases, reports, holds). Migration range used: **1120** only (one file). Job id used: **1120** (`moderation.takedown_restore`; the highest existing singleton id was 965, `grep -rn "JOB_ID: i64" crates` shows no other 1120).

## Done

- **Inventory.** `MOD-TKD-001` to `-011` ticked in `docs/inventory/19-moderation.md` with a note naming the file or test each (`MOD-DASH-005`, the dashboard view of the register, stays later).
- **Database** `1120_takedown_register.sql`: `takedown_notices`, `takedown_items`, `takedown_events`, `takedown_strike_reviews`, `blocked_objects`; `hold_id` on `reading_series`, `reading_chapters`, `video_titles`, `video_episodes` and four new `moderation_holds.scope_kind` values. Triggers: a held catalogue row (and the pages of a held chapter) cannot be deleted, notices and events cannot be deleted or edited (kept forever, D-212).
- **`sr-moderation`**: `takedown_rules` (pure: business days by operator country, calendar-month strike window, state moves, restore-due test, incomplete-notice reply, CSV, ISO time; 14 unit tests), `takedowns` (the service: intake, triage, hold, reject, uphold, counter-notice, court filing, restore, withdraw, the job's `run_due_restores`, export, `item_status`), `blocklist` (`MediaBlocklist`, a 30-second cached copy of `blocked_objects`), `PolicySource` (strike threshold, window, operator country and contact from the `compliance` feature module). `holds::place_hold_in` and `release_hold_in` gained the four catalogue scopes (a series freezes its chapters, a title its episodes).
- **Routes** (`crates/sr-api/src/routes/moderation/takedowns.rs`, all `MANAGE_TAKEDOWNS`, a case moderator is refused by name and the denial is logged): `GET|POST /moderation/takedowns`, `GET /moderation/takedowns/{id}`, `GET /moderation/takedowns/export?format=csv|json`, `POST .../items`, `DELETE .../items/{item_id}`, `POST .../hold|reject|uphold|counter-notice|court-filing|restore|withdraw`, and `GET /moderation/item-status` (any signed-in person; answers only `none`, `held` or `taken_down`). Route matrix rows reviewed (`staff`, and `user` for item-status), OpenAPI, client and mocks regenerated, `KNOWN_CLASHES` still empty.
- **Media path**: `CdnDeps::with_blocklist`; a blocked object answers 451 after the signature check (test in `cdn/tests.rs`).
- **Server wiring**: `takedown_jobs.rs` (the hourly restore job, `TakedownPolicySource`), `app.rs` builds one `Moderation` with the notifier, the policy source and the blocklist and shares it with the routes, the CDN and the job.
- **Web**: Admin > Moderation > Takedowns (`admin/moderation/TakedownsTab.tsx`: register with state filter, intake form, notice page with content, hold, uphold, reject with the templated reply, counter-notice, court filing, restore, withdraw, strikes, timeline, CSV and JSON export); the section opens for `MANAGE_CASES` or `MANAGE_TAKEDOWNS`, each tab follows its own bit; the Library reader and Cinema player show a banner while an item is held (`moderation/ItemHold.tsx`) and their own message once it is taken down; the fragment is `i18n/messages/en/p18-tkd-1.ts`.
- **Docs**: spec status line, section 2 row, section 13 step 2, **section 14 item 4 now "built"**, new section 16 "Implementation notes, step 2"; doc 09 (retention of the register and the held catalogue rows); doc 13 Phase 7 line; M-25 in `answers-followups-p16.md` gets "(b) built" (box left unticked, (c) is open).

## Decisions

- **Strike = an upheld notice still in force.** Counted: state `removed` or `counter_noticed` with `removed_at` inside the window (calendar months back from now), per Person, over the uploader or author of its items. A restored or withdrawn notice stops counting; a counter-noticed one still counts until its content is restored (owner question 1). It is one line in `strike_counts`.
- **Both kinds open a case with category `illegal_content`.** The case vocabulary is the ban-reason list and has no copyright value; adding one would reach the member report dialog. The notice `kind` carries the distinction.
- **The restore job is attributed to the staff member who recorded the counter-notice** in the access log and the hold release (the hold table requires `released_by` whenever `released_at` is set), marked `housekeeping` in the log detail and the notice event.
- **A notice can be withdrawn at any point** (triage included); it then lifts holds and ends the case.
- **No holiday calendar, weekend by country** (Saturday and Sunday by default; Friday and Saturday for SA, KW, QA, BH, OM, EG, JO, IQ, YE, SY, LY, DZ, BD, MV, IL, SD; Friday for IR and AF; Friday and Sunday for BN). Times are UTC.
- **Library and cinema "reader and player pages show the hold"** is a separate small endpoint instead of a field on every catalogue record, so no existing DTO changed; the cost is one cached request per reader or player open.
- **Held catalogue rows refuse deletion at the database** (trigger). An admin who tries to delete a held series gets a storage error, not a polished message.
- The Admin Moderation section is now visible to `MANAGE_TAKEDOWNS` alone. `ModerationSection.tsx` and `AdminApp.tsx` (one line) are the shared files the data-request branch will also touch.

## Still open (with reasons)

- No gateway event for a message removed by a takedown (clients see it gone on the next fetch); the chat services are not reachable from `sr-moderation`.
- Cinema `transcode/` outputs and the Patron download paths are not blocked by asset (the item is unpublished, so no new links are minted; links already signed stay valid until they expire).
- The templated reply to an incomplete notice is shown to staff to copy; nothing is mailed to the notifier (no mail path to third parties exists).
- `MOD-DASH-005` (the register on the dashboard) and a Setup Wizard clause (dropped by D-315).
- Not exercised in a browser: no server on 8080/5173 and no Playwright in this wave. The HTTP flow is tested over the router with Postgres, the pages with Vitest.

## Tests run

- `cargo fmt --check`; `cargo clippy -j 2 -p sr-moderation -p sr-db -p sr-api -p sr-server --all-targets -- -D warnings` clean.
- `cargo test -p sr-moderation -- --include-ignored --test-threads=1`: 25 unit (14 new: business days, weekend by country, month arithmetic, strike window, state moves, restore-due, CSV, reply, blocklist), the 10 step-1 Postgres tests, and 12 new Postgres lifecycle tests (`tests/takedowns.rs`: intake and permissions, overdue triage, reject with reply, uphold with hold/disable/block/DM/access log, hold first, counter-notice and job restore with the restored state of every item, court filing stops the job, restore and withdraw, strike review at the threshold without suspension, strikes outside the window, CSV export and logged read, unknown content).
- `cargo test -p sr-api -- --include-ignored --test-threads=1` (633 lib including the takedown HTTP flow, the CDN 451 test and the permission tests; route_matrix 5; openapi_names 2), `-p sr-db` (all suites), `-p sr-server` (118, including `retention_tests::a_takedown_hold_blocks_the_purge_until_the_notice_ends`, the retention guard: the purge skips a message the takedown holds and takes it at the next run once the notice is withdrawn).
- Web: `typecheck`, `eslint .`, `prettier --check src`, full `vitest run --maxWorkers=2` (266 files, 2426 tests, after `pnpm --filter @sideroom/wasm build`), `vite build` and `check-bundle-budget.mjs` (admin 352 KB gzip of 600).
- Scratch Postgres: a private database `sr_p18_tkd` (dropped at the end) and the `sr_tpl_*` template databases `TestDb` leaves behind (dropped).

## Owner questions

1. Should a counter-noticed takedown (content still down, counter-notice pending) count as a strike, as it does now? Counting only after the wait would delay a review.
2. Is `illegal_content` the right case category for a copyright notice, or should the case vocabulary get a `copyright` value (it would then also have to be hidden from the member report dialog)?
3. D-293's legal points stand: designated-agent registration, DSA articles 16 and 17 (a statement of reasons to the uploader is the DM; the notifier gets only the staff-sent reply), and whether ten business days matches local law. Not legal advice.
