# Handoff: feat/p25-gates (wave 25, the phase 6 and 7 exit gates)

Branch `feat/p25-gates`, worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p25-gates`, target dir `p25-gates-target`. No migrations. Nothing pushed, no downloads.

## Resume here

1. Push the branch and let the `playwright` job of `e2e.yml` run. The new step "Exit gates of phase 6 and 7" has never run on CI (no YAML runner here; the file parses with `js-yaml@4.3.2`). Expect it to need one look: it runs `--grep @gate` on a second `e2e-run.sh` server and writes its report to `playwright-report-gate` and `test-results-gate`.
2. When that step is green on Chromium, Firefox and WebKit, link the run from the phase 6 and 7 rows of `docs/13-roadmap-and-phases.md`, and flip the phase 7 row to Proven. Phase 6 stays Partly until the weekly `sdk-compat` run is green (RDY-DEV-003).
3. Firefox does not launch on this machine, so the specs have run on Chromium and WebKit only. Nothing in them is browser-specific.

## Done

### RDY-DEV-005, `e2e/tests/developer-gate.spec.ts`

One test, tagged `@gate`, 33 seconds locally:

- A member opens `/developers/applications`, creates an application in the dialog and reads the bot token out of "Save your credentials".
- The OAuth2 page (`/oauth2/authorize`, scopes `bot applications.commands`) adds the bot to the member's server.
- `e2e/support/bot.ts` starts the in-repo `@sideroom/bot` `Client` with that token (REST and gateway on `127.0.0.1:8080`); it registers a slash command at READY and answers it. The spec waits until the command is listed through the bot's own REST call, opens the channel (the composer caches an app's commands for a minute per channel), types the command and reads the reply.
- The member saves the policy links, submits the application for review in the Review tab, staff (the Owner, through `Person` of `e2e/sdk-compat/lib/provision.mjs`) approve the version, the application shows "Released", and a second member adds the bot to their own server through the same page. The bot is a member of that server.
- `provision.mjs` is reused, not copied; `e2e/sdk-compat/lib/provision.d.mts` is a new typing file next to it (the file itself is unchanged).
- Needs the gateway and the REST API only: no object storage, no voice. Locally it ran with `SR_STORAGE__ENDPOINT=memory:`.

### RDY-MOD-001, `e2e/tests/moderation-gate.spec.ts` and `crates/sr-moderation/tests/deadline_windows.rs`

One test, tagged `@gate`, 13 to 40 seconds locally. People: A (reporter and owner of a server), B (reported), B2 (B's second account), S (staff: Senior Staff, appointed with `sr-cli staff appoint`, TOTP enrolled, second factor proved through `POST /admin/reauth`), and the Owner as the other reviewer. The steps:

1. A reports B's message (`moderation.reports` switched on by the Owner). Harassment in a server goes to the server's moderators first (D-209), so A claims the case and escalates it; S then claims it, moves it to investigating and adds a note. Filing the report already froze the message as evidence; S holds all of B's messages in a time window on top.
2. S bans B from the case (second factor); the case is "actioned". B signs in through the browser and gets the ban notice with the appeal form, which the spec fills in. B2 is registered while B is banned.
3. S scans B2's Person: the scan names B ("new account after ban"); S confirms the signal.
4. The Owner decides the appeal (the staff member who banned may not, D-215): overturned, which unbans B and closes the case.
5. The Owner builds a Library series, edition, chapter and a page, publishes it. A reads the chapter (200). S registers a takedown notice, adds the chapter, upholds it (second factor): the chapter is gone for A. S records a counter-notice and restores: the chapter is back (200).
6. The Owner links B2 into B's Person; the signal is marked folded.
7. S enters a data export request for B, verifies it and runs it (second factor); B downloads the package. S enters an erasure with deletion of messages, verifies, runs it; B's messages disappear for A. The erasure flags both holds of the case: S keeps the reported message as evidence with a stated basis and releases the wider hold.
8. The Owner reads the access log: one `admin_action` row per step for S (eighteen named actions with their counts, for example exactly one `moderation.case.ban`, `moderation.takedown.uphold`, `data_request.run_erasure`), the `read` rows of the scan, the signal list and the requests, the Owner's own rows since the start of the run, and the hash chain verifies.

The pieces that are API calls rather than clicks (everything except the report-to-ban-notice appeal) are the routes the pages call; the browser parts of the staff pages are covered by Vitest already. The deadline-window half is Postgres, not Playwright:

- `deadline_windows.rs`, three `#[ignore]` tests run with `SR_TEST_DATABASE_URL`: the due date and the overdue flag to the millisecond (30 days from receipt, with the dashboard's overdue count), the single 60-day extension and the late marker, and an erasure run ten days late that completes, flags the held message, hides messages with a purge date thirty days after the deletion and no purge date for the held one, and keeps that date when the hold is released later. A manual clock moves the dates; nothing waits.

### Handler fix the gate found

- The developer portal's Review tab crashed on any application with a version ("Cannot read properties of undefined (reading 'length')"). The API sends a version's `diff` as an array (`[{key, old, new}]`, the generated schema says so), the page typed and read `diff.changed`. `apps/web/src/developers/types.ts`, `tabs/Review.tsx` and the test fixture `__tests__/helpers.tsx` now follow the API. The existing Vitest suite for the portal (122 tests) passes.

### CI wiring (`.github/workflows/e2e.yml`, `playwright` job)

- The main step now runs `--grep-invert @gate`; the new step runs `--grep @gate` on a fresh server. Reason: registration allows 20 accounts per address and day (counted in memory) and the gates register seven. The step runs even if the main one failed (`!cancelled()`), and reports to its own directories. Job timeout 30 to 45 minutes (WebKit took 26 minutes before this step).
- Staff reauth stays ON in this job; the moderation gate enrols a second factor for its staff member instead of switching it off.

### RDY-DOC-006 (doc 13 and `AGENTS.md`)

- Phase 2: the soak's CI data point (run 40258927429, eight members) and what is left (ten members on a real machine, the two-hour session). Phase 22: the soak is the only open half. Phase 23: Partly, accessibility and first run proven with the handoffs and the green `playwright` run 40262027228 on PR #50; RDY-DOC-003 and the screen-reader pass remain. Phase 24: Proven, load test run 40256129938. Phases 6 and 7: Partly, linking the specs, the Postgres test and this handoff.
- Not claimed: the ten-member owner voice run, the CI run of the two new specs, `sdk-compat`.
- One line added to section 4 of `AGENTS.md`: the wave 25 integration checklist.

## Checks run

- `cargo test -p sr-moderation --test deadline_windows -- --ignored --test-threads=1`: 3 passed. `cargo clippy -p sr-moderation --tests` and `cargo fmt` clean.
- `pnpm --filter @sideroom/e2e typecheck`, `pnpm --filter @sideroom/web typecheck`, `pnpm lint` (eslint and prettier) clean; web tests for `src/developers`: 122 passed.
- Playwright on a local stack (Windows, Postgres 17, `sr-server` debug build, memory storage): both specs pass on Chromium and WebKit, one server and one database for all four runs (`--grep @gate`). Firefox does not launch here.
- `node scripts/check-ops-files.mjs` and `node scripts/check-doc-links.mjs` clean.

## Notes for the next person

- The evasion scan raises at most ten signals per run. Before the integration it raised them lowest related Person id first, so a Person linked to more than a handful of banned Persons could miss the signal for the newest ban: a real moderation gap, not a local quirk. Fixed in the phase 25 integration (P25-SEC-1 (4)): `suspicion.rs::scan_person_in` spends the budget newest ban first, pinned by the Postgres test `the_signal_budget_keeps_the_newest_ban` in `crates/sr-moderation/tests/suspicion.rs`. Running the moderation gate twice on one local database no longer needs a fresh database for that reason.
- A banned person's reported message is held by the report itself, so a staff `holds` call on it answers "already under a hold"; the spec holds the person's messages in a window instead.
- A bot's `identifyGapMs` is set to 0 in `e2e/support/bot.ts` (one shard, one bot).
