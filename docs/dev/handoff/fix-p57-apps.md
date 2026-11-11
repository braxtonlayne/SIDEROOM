# Handoff: fix/p57-apps (wave 57, SEC-AUD-057, D-587)

Branch `fix/p57-apps` (base `integration/phase55` plus the wave 56 integration commit `1b19e9bf`). Not pushed, no PR.

## Done (SEC-AUD-057, D-587)

- **`GET /admin/developers/apps`** (new). `REVIEW_APPS`; paged (`limit` 1 to 100, default 50, `before` = last application
  id, newest first); `q` searches the application name, the application id, or the owner (owner id, username or display
  name; `%` and `_` are literal); `state` filters by release state (`development`, `in_review`, `released`,
  `suspended`; anything else is a 400). A row is the staff view of the application (owner, `release_state`, no secrets, no
  owner-only fields) plus, while suspended, `suspended_reason`, `suspended_by` (the staff member) and `suspended_at`. Who
  and when come from the latest `app.suspend` row of `application_audit` (no new column, no migration).
  Each call is written to the staff access log as a `read` row (`developer.apps_list`; the detail holds the state filter,
  whether a search was used and the row count, never the search text).
  Code: `sr-apps` (`AppAdminRow`, `AppsRepository::list_applications_admin`, memory impl, `AppsService::admin_applications`),
  `sr-db` (`apps_repo.rs`), `sr-api` (`routes/developer/portal.rs::admin_apps`, `AppAdminObject`).
- **Owner DMs.** The service did not send any, so the two route handlers now send one system DM (the existing `SystemDm`,
  the Staff Server's voice) to the application's owner: on suspend (application name and the reason) and on unsuspend
  (the note if one was given). Refused requests (no reason, already suspended, not suspended) send nothing.
- **Admin "Applications" screen** in the Review hub (a new tab next to Apps, only for `REVIEW_APPS`): `Applications.tsx`
  using the shared `DataTable`: search box (name, application id or owner), release-state filter, load more, one row per
  application with the owner, a state badge, and for a suspended one the reason and "Suspended by @x on <date>". Suspend
  opens the reason dialog (reason required, 500 characters), Unsuspend the same dialog with the note optional. Both calls go
  through the admin `send` helper, which is `withReauth`: the D-581 second-factor prompt opens once per window and the call
  is retried. No bulk action. Created and the application id are columns the chooser can turn on. App build only (admin
  chunk; nothing public changed, no entrance files touched).
- Routes and contract: `route_matrix.tsv` row for the new GET (staff, `REVIEW_APPS`); `packages/api-client/openapi.json`,
  the generated schema and the MSW handlers regenerated (`mocks:check` green).
  **Note for the integrator:** at base `1b19e9bf` the committed `packages/api-client/openapi.json` is an EMPTY file (the
  wave 56 integration commit emptied it, probably a failed `> openapi.json` regeneration). My commit restores the whole
  document (diff against the last good version `1ea08c41` is only the new route and schema, 119 lines). The other two
  builders will see the same empty base and regenerate it too, so expect a conflict there: take any full regeneration.
- Docs: inventory row SEC-AUD-057 (after 056), doc 11 section 2 item 4 (one sentence).
- Tests that fail without the change: see below.

## Commits

- `5c400679` SEC-AUD-057: list route, DMs, screen, sweep rows (`git log --oneline 1b19e9bf..fix/p57-apps` lists all)
- e2e spec + inventory/doc 11; OpenAPI + client regeneration; phone layout fix (actions stay in the first cell);
  shorter "Development" label; this handoff.

## Tests run

- Rust: `cargo test -j 4 -p sr-apps` 67 passed; `cargo test -j 4 -p sr-api --lib developer` 54 passed, including the three new
  in `routes/developer/apps_list_tests.rs` (gate: anon 401, non-staff 403, staff 200 and no owner-only fields; search by
  name, id, owner name, owner id, wildcard text; state filter; bad state and bad `before` 400; paging; suspend and
  unsuspend DMs: exactly one each, to the owner and not to staff, refusals send none, reason and who and when on the
  row); `cargo test -j 4 -p sr-api --test route_matrix` 6 passed; Postgres (`sr_p57_apps`):
  `cargo test -p sr-db --test apps_repo -- --ignored` 20 passed, including
  `the_staff_list_searches_filters_pages_and_reads_who_suspended` (the SQL: owner join, escaping, filter, paging,
  suspended by and at).
  `cargo clippy -j 4 -p sr-apps -p sr-db -p sr-api --all-targets -- -D warnings` clean; `cargo fmt --all -- --check` ok.
  Authz matrix with Postgres (`cargo test -p sr-server --test authz_matrix -- --ignored every_route_enforces_its_authz_row`): passed on the second run
  (305 s). The first run failed on two unrelated write probes (`POST /channels/{id}/scheduled-messages` and
  `send-soundboard-sound`, "0 timeout or connection failure") while the machine was busy; nothing about the new route or
  the suspend routes failed. `cargo test -p sr-api --lib` 844 passed.
- Web: vitest 126 passed in 15 files (`p57apps.test.tsx` 5 new: list with suspended row, search and filter go to the
  server, suspend needs a reason then unsuspend with none, a refusal keeps the dialog and shows the error, tab hidden
  without `REVIEW_APPS`; `p55staffReauth.test.ts` gained `app suspend` and `app unsuspend` "prompts once and retries
  once"; reviewHub, review, reviewExtras, packagesReview, admin, i18n catalog all green). `tsc`, eslint and prettier on
  the changed files, `check-css-tokens`, `i18n-audit` (0), `check-doc-links` clean.
  `playwright test --list --project chromium` loads 963 tests.
- e2e `tests/admin-apps.spec.ts` (new, passes against my stack with the second factor ON): an App Reviewer with TOTP
  searches for a developer's application, Suspend asks for a reason, the real "Confirm it is you" dialog opens and
  takes a TOTP code, the row shows the reason and "Suspended by @staff on ...", the state filter finds it, the
  developer's system DM contains the reason, Unsuspend works.

## Visual check

Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p57-apps/` (new sweep rows in `e2e/tests/page-sweep.spec.ts`:
`admin-review-applications` (list with a released, a suspended and a development application), `-suspend` (the Suspend
dialog open) and `-unsuspend` (the Unsuspend dialog open); the sweep world gained a suspended app "Sketchy Relay"
(needs `SR_ADMIN_REQUIRE_REAUTH=0` on a development host, like the other staff seeds); each at desktop 1280x800 and phone
375x812, Lounge Light and Editorial dark). I looked at the list, both dialogs and the suspended row in all four
combinations. Fixed after looking: on the phone the first layout (separate owner, state and actions columns) pushed the
Suspend button off the right edge, so the owner, the reason line and the button moved into the first cell and the state
is the second column; the "In development" badge was clipped, so it reads "Development". Sweep flags: none for the
three new rows; `admin-review` desktop light was flagged "blank" once, a cold Vite first load (the same route shot
three more times is fine; it happened in two of the four runs), always the first capture after a cold start of the Vite dev server (no console or page errors, the dark app background only).

## Not done / notes

- The second-factor prompt is covered by the vitest case and the e2e flow (real TOTP); the sweep itself ran with the window
  off.
- Search is a substring match on the name and the owner's username or display name (ILIKE), plus exact id matches for a
  number; no full-text.
- The list is not in `SENSITIVE_ACTIONS` (a read, not an action); only the access-log read row.

## Owner questions

- Should the application owner also get the DM when staff suspend an application that is already blocked another way
  (for example its owner account is suspended)? Built as "always, the DM is only a message".
- The Applications tab sits next to the existing "Apps" tab (queues for developer requests, versions and Activities).
  Merge the two names or keep them (Apps = decisions waiting, Applications = every application)?
- "Suspended by @name": the name of the staff member is shown to every `REVIEW_APPS` holder (it is in the access log
  anyway); the owner's DM does not name the staff member. Say so if the DM should.

## Proposed threat-model rows

None: no new surface; the existing app suspend row (T58 via D-581) already covers the step-up. The new read is logged.
