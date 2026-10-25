# Handoff: feat/p23-first-run (wave 23: RDY-ONB-001 to 003, RDY-I18N-001, ReviewHub rename)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p23-first-run`, branch `feat/p23-first-run` (base `integration/phase23`). Local commits only. Target dir `p23-first-run-target`, scratch DB `sr_p23_firstrun` (dropped at the end). Migration: 1280 only (1281 to 1289 unused).

## Resume here

Everything below is built and its proof ran locally unless "Not run" says otherwise. Nothing is running now (the server on 8082 and Vite on 5175 of the Playwright run were stopped by PID).

## Done

### RDY-ONB-001 Platform first run

- **Stored on the account**: migration `1280_first_run_done.sql` adds `accounts.first_run_done boolean not null default false` and sets every account that exists when it runs to `true` (nobody who already found their way around is welcomed again). The roadmap says `users.first_run_done`; the table is `accounts` (there is no `users` table).
- **API**: `GET /users/@me` answers `first_run_done` for sessions; `PUT /sr/auth/first-run {done}` (204) sets or clears it (`crates/sr-api/src/routes/auth/first_run.rs`). `AuthStore` gets `first_run_done` and `set_first_run_done` with default bodies (`sr-auth`), implemented for Postgres (`sr-db`) and the in-memory test store. Route matrix row `self` / `deny`, OpenAPI, api-client types and MSW mocks regenerated, `KNOWN_CLASHES` empty.
- **The flow** (`apps/web/src/lounge/shell/Welcome.tsx`, replacing the 14-day `welcomeDone` dialog): intro (what the three sections are), look (theme buttons and the picture nudge), two-step sign-in with the existing authenticator wizard opened in place (lazy `TotpWizard`, exported from `Account.tsx`), the install prompt only where the browser offers one (`beforeinstallprompt`, or the iOS Share steps), notifications (told, never asked: the button opens Settings > Notifications, where the permission is requested on a click), and the home server's guide or onboarding questions when its managers wrote them (the first server on the list; opens the existing Server Guide or onboarding dialog).
- **Never a wall**: Skip for now, Escape and the dialog's close all store `first_run_done` and close it; a failed save changes nothing for the person. It is re-openable from `/help` ("Show the welcome again") and from the help menu.
- **E2E build**: `VITE_SR_E2E=1` still mutes the welcome for every other spec; the first-run spec opts in per page with `sessionStorage['sr-e2e-first-run']='1'`.

### RDY-ONB-002 The handbook at `/help`

- `apps/web/src/help/Handbook.tsx` (sections as data in `HANDBOOK_SECTIONS`), `HelpPage.tsx`. Words are in `i18n/messages/en/p23-first-run-1.ts`. Sections: joining and credits, sign-in with 2FA and passkeys, installing on iOS and Android, the Lounge for people who know Discord, submitting to Library and Cinema with the naming guide, watch parties, Live TV, privacy (stored, export, delete, report, the operator's country and contact), house notes. Each links its screen.
- `/help` is no longer a first-paint (core catalog) path, and the handbook lives in `src/help/`, not `routes/entrance/`, because the entrance check only allows keys of the three core fragments. The old self-service answers stay below the handbook. A signed-out visitor now waits a moment for the full catalog.
- Agreement with doc 09 is a test (`help/handbook.test.tsx` reads `docs/09-retention-and-privacy.md`). Doc 09 got one paragraph saying so.
- "The doc links checker covers in-app routes it names": `scripts/check-doc-links.mjs` only checks Markdown links to files. The handbook's in-app links are checked instead by the same Vitest (each `href` must resolve to a registered route). Extending the Markdown checker to a route list would mean a second route table; I did not build that.

### RDY-ONB-003 What's new

- `CHANGELOG.md` at the root, waves 16 to 22 written from the integration and branch handoffs, newest first. Rendered at `/help/whats-new` (lazy chunk, restricted Markdown renderer of the Entrance).
- The build embeds the version: `vite.config.ts` defines `__SR_VERSION__` as the newest `##` heading without its date ("Phase 22"). `lib/whatsNew.ts` keeps the last seen version in `localStorage` (`sr.whatsNew.seen`); the help button shows a dot (and the label "Help, there is news") when it differs, and opening the page clears it. A browser that has seen nothing records the current version without a badge.
- `AGENTS.md` section 4 gained the integrator's line.

### RDY-I18N-001

- `scripts/i18n-audit.mjs` no longer skips `routes/entrance`, and two false-positive classes were closed (the `>` of an arrow function such as `() => Promise<void>`, and text between two generics that contains `;`). Before: 462 in the script's own count, with the entrance not counted and heuristic noise inside; after: **0** in `routes/entrance`, `lounge/shell` and `settings`, **42** in the rest of the app (table below). The five real strings found were extracted (`p23-first-run-2.ts`). `i18n-verify-diff` against `integration/phase23`: MISSING 0.

### ReviewHub rename

- The submissions tab of Admin > Review queues is "Submissions" (it holds Library and Cinema submissions); the Cinema tab stays the reports queue. Tab id `submissions`; a `?submission=` card link opens it. New key `admin.reviewHub.tabSubmissions` in `p23-first-run-2.ts`; the old `tabLibrary` key in another agent's fragment is now unused and was left alone.

## Tests run

All against the scratch database `sr_p23_firstrun` (both `SR_TEST_DATABASE_URL` and `DATABASE_URL`), one cargo command at a time with `-j 2`.

- **Rust.** `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean on sr-auth, sr-db, sr-api. `cargo test -p sr-auth -p sr-db -- --include-ignored --test-threads=1`: 336 passed, 0 failed (includes the new `the_first_run_flag_starts_open_and_is_stored_per_account`, Postgres). `cargo test -p sr-api -- --include-ignored --test-threads=1`: 781 passed, 0 failed (includes `the_first_run_flag_is_reported_on_me_and_can_be_reopened`, `route_matrix` with the new row, `openapi_names` with `KNOWN_CLASHES` empty). `cargo build -p sr-server -p sr-cli` ok. Generated: `openapi.json`, the api-client schema and the MSW mocks (971 operations) and `route_matrix.tsv` (one new row, no `?`).
- **Web.** `tsc --noEmit`, eslint (`--max-warnings=0`) and prettier (local copy) clean. Full Vitest once with `--maxWorkers=2` after the wasm build: 287 files, 2557 tests passed. New: `lounge/__tests__/firstRun.test.tsx` (13: one per step, the guards, reopening), `help/handbook.test.tsx` (handbook, links against the router, doc 09 figures, What's new, version and badge), `lounge/__tests__/helpButton.test.tsx` (6), the `ReviewHub` tab tests (3). `i18n-verify-diff integration/phase23`: MISSING 0. `check-doc-links`: ok. `pnpm build` and `scripts/check-bundle-budget.mjs`: all budgets ok (entrance 230.5 of 300 KB gzip, lounge 428.3 of 600).
- **Playwright** `e2e/tests/first-run.spec.ts` (5 tests: register, skip, first message and no return after a reload; Escape from a later step; the full walk with two-step sign-in set up in place and no notification permission requested; the handbook button; What's new from the help menu): 10 passed, 5 on Chromium and 5 on WebKit, against `sr-server` on 8082 (metrics 9182) and Vite on 5175, both stopped by PID afterwards. Firefox does not launch on this machine (VC runtime), so it is not run here; CI's `e2e.yml` runs all three projects. The run exposed one real defect, fixed: the authenticator wizard asks the server for a new secret every time it opens, and mounting it already open sent that request twice under StrictMode, so the key on screen was not the one the server held. The welcome now mounts it closed and opens it from the button.

### i18n audit, before and after

| Where | Before | After |
|---|---|---|
| `routes/entrance` (was skipped by the script) | 5 | 0 |
| `lounge/shell` and `settings` together | 105 (5 real strings, the rest the script reading TypeScript generics and arrows as text) | 0 |
| rest of the app | 42 | 42 (admin 16, lounge outside shell 11, cinema 7, developers 7, watchparty 1) |

## Still open / not done

- Nothing from the four items is left open. Not done on purpose: the audit's `--ts` mode (plain `.ts` label tables) is not part of the roadmap acceptance and was not touched.
- The old `settings.whatsNew.*` dialog in User Settings (an in-app "What's New" with three lines about phase 9) is untouched; it could link to `/help/whats-new`. That file is outside this row.
- iOS web push after installing: the handbook says notifications work once the app is on the Home Screen; not verified on a device.

## Decisions

- The first run asks for nothing and is never required; the flag only decides whether it shows by itself.
- Existing accounts count as done (migration backfill), the opposite of a 14-day rule.
- `/help` moved out of the first-paint set (see above). The roadmap said content lives in i18n catalogs; the alternative, putting the handbook into the core `entrance.ts` fragment, would grow every entrance page's first paint.

## Open owner questions

- Is it right that a signed-out visitor to `/help` now waits for the full catalog (a moment of a blank page) instead of seeing the short help instantly? The other choice is a second, tiny core-catalog help page for signed-out visitors.
- Handbook wording on credits says "five, unless your operator changed it" (doc 06 default). Confirm the owner wants credits mentioned there at all.
