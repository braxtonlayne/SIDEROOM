# Handoff: feat/p18-mod-cases (M-25 (0) and (a), D-315)

Base `integration/phase18` (= main at `03fa69ed`). Migration range used: **1090** only (one file).

## Done

- **(0) Inventory.** `docs/inventory/19-moderation.md` lists every Phase 7 item (80 ids: `MOD-PERM`, `MOD-RPT`, `MOD-CASE`, `MOD-HOLD`, `MOD-APL`, `MOD-TKD`, `MOD-DSR`, `MOD-AGE`, `MOD-BAN`, `MOD-DASH`, `MOD-SUS`; the takedown and data-request ids are all there for the parallel DSR agent and for step (b); dashboards and suspicion are marked later). Linked from `docs/inventory/README.md`, doc 13's Phase 7 section and `XC-MOD-003`. 41 items are ticked with a short italic note naming the file or test.
- **(a) Cases and reports.**
  - **Database** `1090_moderation_cases.sql`: `moderation_cases`, `moderation_reports`, `moderation_case_evidence`, `moderation_case_notes`, `moderation_case_events` (evidence, notes and events are append-only by trigger), `moderation_appeals`, and new columns on `moderation_holds` (`case_id`, `scope_kind`, `scope_id`, `released_at`, `released_by`, `release_reason`, `reviewed_at`). The task text names the tables `moderation_cases`, `reports`, `case_actions`, `case_notes`; I used `moderation_reports` for reports and made actions timeline events in `moderation_case_events`.
  - **Crate `sr-moderation`** (new workspace member, depends on `sr-db` for the hash-chained log append, on `sr-admin` for `Caller`/`AuditEntry`): `rules` (routing D-209, access, lifecycle, appeal reviewer rule; pure and unit tested), `reports` (file a report: target resolution and visibility, rate limit, duplicate attach, immutable evidence snapshot of the message plus ten before and after, system hold on a reported message), `cases` (queue, detail, open, state changes, reopen window, notes, claim, escalate, warn, record an action, record a platform ban), `holds`, `appeals`.
  - **Hold API for step (b)**: `holds::place_hold_in(conn, ids, &HoldRequest)` and `release_hold_in(...)` take the caller's transaction and say who acted (`None` is the system); they check no permission and write the case event. `Moderation::place_hold` / `release_hold` add the platform-staff check (`MANAGE_CASES`, D-213), a required reason and the access-log row in the same transaction. Scopes: message (with attachments), attachment, submission, a user's messages in a window. A hold that would freeze nothing new is not created (`hold_id: None`). Releasing a message deleted while held gives it a `purge_after` so the next purge run takes it.
  - **Routes** (`crates/sr-api/src/routes/moderation/`): `POST /moderation/reports` (flag `moderation.reports`), `GET /moderation/reports/mine`, `GET|POST /moderation/cases`, `GET /moderation/guilds/{id}/cases`, `GET /moderation/cases/{id}`, `.../state`, `.../reopen`, `.../notes`, `PUT|DELETE .../assignee`, `.../escalate`, `.../actions`, `.../warn`, `.../ban`, `.../holds`, `GET /moderation/holds`, `POST /moderation/holds/{id}/release|reviewed`, `GET /moderation/appeals`, `POST /moderation/appeals` (public, credentials), `POST /moderation/appeals/{id}/decision`. Route matrix rows reviewed (no `?`), OpenAPI/client/mocks regenerated, `KNOWN_CLASHES` still empty (two of my schema names clashed and were renamed `CaseBanBody` and `CaseNoteBody`).
  - **Wiring** in `sr-server/src/app.rs` (`ModerationDeps`, the system DM adapter on `StaffVoice`); three platform settings added to the Admin catalog: `moderation.reports_per_day` (20), `moderation.hold_review_days` (90), `moderation.case_reopen_days` (30).
  - **Retention guard**: the review housekeeping (abandon of stale drafts, purge of rejected submissions) now skips held submissions (`review_repo.rs`), with a test in `crates/sr-db/tests/review_repo.rs`; `sr-server` `jobs::moderation_release_tests` proves a hold placed and released through `sr-moderation` is honoured by the message purge.
  - **Web**: the report dialog replaces the "reporting is not available" dialog (`moderation/ReportDialog.tsx`; category, note, "server moderators may see my name", "also block"); Report entries for a message (not your own), a person (with the server it was opened in), a channel and a server; Admin > Moderation (`admin/moderation/`: queue with filters and claim, case page with reports, snapshots, holds, appeal, notes, timeline and the actions, Holds tab with the review flag, Appeals tab); the ban screen appeal form on the sign-in page (`routes/entrance/BanAppeal.tsx`).
- **Docs**: spec status line, section 2 rows, section 13 step 1 and a new section 15 "Implementation notes, step 1"; doc 09 retention line; doc 13; M-25 in `answers-followups-p16.md` gets a "partly done" note (box left unticked).

## Decisions

- **Two hats, one set of routes.** `rules::pick_who` chooses between platform staff (`MANAGE_CASES`) and a server moderator (`MANAGE_MESSAGES` in the case's server). D-209 is applied literally: platform staff do not see a guild-routed case until its moderators escalate; after that the moderators keep a list row and lose the detail. Reviewers (`REVIEW_*` / `DELETE_CONTENT`) get `work` access to cases routed to reviewers through the API only; no UI.
- **The report hold is placed by the system** (`created_by` empty). D-213 says holds are placed by platform staff; I read it as the manual tool and kept spec 3.2's automatic freeze. Flag for the owner if that is wrong (it makes a mass false report able to freeze messages; the daily report limit bounds it).
- **Case events never carry the reporter**, so moderators can read the timeline; the reporter appears only in the report list, subject to D-209.
- **Entrance strings** for the appeal form went into the core fragment `entrance.ts`, not my own fragment, because the sign-in page paints before the rest of the catalog loads (`en.ts` only bundles `entrance`, `a11y`, `pwa` for first paint).
- `sr-api` now depends (through `sr-moderation`) on `sr-db`, which it only had as a dev-dependency. Other feature crates (`sr-userfeat`, `sr-cinema-social`) already carry sqlx; the new link is only for `append_in`, the hash-chain writer. A trait in `sr-admin` would avoid it if the layering matters.
- Staff-case writes go to the platform access log; a **server moderator's** case actions are case events only (the guild audit log has no case actions).

## Still open (with reasons)

- `MOD-CASE-009` delete/restore content, remove an attachment, time out, kick, guild ban from the case: needs the chat services; today the moderator does it with the server's own tools and records it (`record_action`).
- No Admin form to open a case without a report (`POST /moderation/cases` exists); no reviewers' queue UI; no reporter status page in Settings (`MOD-RPT-013`); the Accounts/Bans ban dialogs still use the severity-only choice (`MOD-BAN-002`); the configured report channel of spec 3.4.
- Steps (b) takedown register (`MOD-TKD-*`) and (c) data requests (`MOD-DSR-*`) are not mine; dashboards and suspicion are later. Spec section 14 items 4 to 6 stay "still open".
- Not exercised in a browser: no server on 8080/5173 and no Playwright in this wave (brief). The HTTP flow is tested over the router with Postgres; the web pages by Vitest.

## Tests run

- `cargo fmt --check`; `cargo clippy -j 2 -p sr-moderation -p sr-admin -p sr-db -p sr-api -p sr-server --all-targets -- -D warnings` clean.
- `cargo test -p sr-moderation` (9 unit) and `--test db -- --include-ignored --test-threads=1` (10 Postgres: report/snapshot/immutability, duplicates and rate limit, reporter privacy and escalation, DM reports with the logged snapshot, child-safety routing, lifecycle and reopen, staff cases/warn/ban note, holds and release, user-message window and submission holds, appeals).
- `cargo test -p sr-api -- --include-ignored --test-threads=1` (629 lib incl. 5 moderation route tests and the new `appeal_account` auth test, route_matrix 5, openapi_names, api/guilds/lounge), `-p sr-db -- --include-ignored --test-threads=1` (all suites), `-p sr-server -- --include-ignored --test-threads=1` (116), `-p sr-admin`.
- Web: `typecheck`, `eslint`, `prettier --check` on changed files, full `vitest run --maxWorkers=2` (264 files, all passing after `pnpm --filter @sideroom/wasm build`), `vite build` and `check-bundle-budget.mjs` (admin 349 KB gzip of 600).
- Scratch Postgres: a private database `sr_p18_modcases` (dropped at the end) and the `sr_tpl_*` template databases `TestDb` leaves behind (dropped).

## Owner questions

1. Should the automatic hold on a reported message stay a system action (spec 3.2), or wait for a staff member (D-213 read strictly)?
2. Reporting is still off by default (`moderation.reports`). Turn it on per pilot guild when ready; there is no per-guild switch, the flag is platform-wide (D-216).

Answered 2026-10-20 by the owner's proxy: D-328 to D-331 in `docs/01-decisions-log.md` ("Phase 18 moderation answers"), P18-MOD-1 to P18-MOD-3 in `docs/open-question-answers.md`; follow-ups N-20 to N-27 under "Phase 18 moderation round" in `docs/dev/answers-followups-p16.md`. The "still open" list above is triaged there: nothing closed, the reviewers' queue folds into Admin > Moderation, guild audit rows are for outcomes only.
