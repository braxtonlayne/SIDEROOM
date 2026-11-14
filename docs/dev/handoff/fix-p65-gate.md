# Wave 65 gate (SEC-AUD-170..172; D-651, D-652, D-653)

Branch `fix/p65-gate` (base `integration/phase64`). All five items done. No route, no OpenAPI change, no migration.

## Done (commits, oldest first)
- SEC-AUD-170..172 main commit: `apps/web/vite/staffChunks.ts` (+ `.test.ts`), `vite.config.ts` (one plugin line, `chunkFileNames` and `assetFileNames` functions), `scripts/check-staff-delivery.mjs` (+ `.test.mjs`), `scripts/check-bundle-budget.mjs` (the staff rule), `web.yml` and `repo-checks.yml` (one step each), `crates/sr-server/src/web.rs` and `tests/web_gate.rs`, `public/sw.js` and `swFetch.test.ts`, `admin/routes.tsx` + `admin/StaffDoor.tsx` + `admin/__tests__/staffDoor.test.tsx`.
- Docs: `docs/dev/entrance-isolation.md` section 11 "Staff code".
- `scripts/bundle-budget.json`: the `admin` route row removed (see below); `e2e/tests/p65-gate.spec.ts`.
- `admin/__tests__/admin.test.tsx`: the two tests for a 403 / 401 from `/admin/me` now expect the not-found page (the door answers before the Admin app runs).

## What was built
1. **Build sorting.** The plugin walks the module graph in `buildEnd` (`getModuleInfo`), from the entry, not following a dynamic import of a staff entry (`STAFF_ENTRIES = ['src/admin/AdminApp.tsx']`, one list, a comment says how to add one). Modules not reached are staff-only; a chunk made only of them is `staff/[hash].js`. A static import of a staff entry from member code is a build error naming the importer; a chunk that mixes staff-only and member modules is an error too. It writes `dist/app/staff-chunks.json` (`version`, `entries`, `staff`, `staffModules`, `memberModulesUnderAdmin`, plus the extra `memberModulesStaffLooking` that feeds the report).
2. **CSS was sorted.** `assetFileNames` matches the asset's `originalFileNames` against the staff modules, so the Admin stylesheet is `staff/<hash>.css`. (`admin/admin.css` itself is imported by member code too and stays in member chunks.)
3. **Check** `check-staff-delivery.mjs` rules a-e as specified. Markers (rule d): `donation-entitlements`, `cinema-transcoding`, `invite-graph` (section ids that exist only in `AdminApp.tsx`; not catalog keys, because the catalog is a member chunk until wave 66). It also fails when no marker is inside `staff/` (stale list). Report (not a failure): staff-looking modules in member chunks.
4. **Server.** `session_account` (and `has_session` as a wrapper), `is_staff` (`AdminDeps` from `api.extension`, `AdminService::resolve`, every request, no cache; no admin deps or a lookup error is "no", the error logged). `staff/` without an appointment is the `404` of an absent `/app/` file (same headers: `no-store`, `nosniff`, `Vary: Cookie`, `X-Robots-Tag`) and is decided before the file system is asked. A first segment that folds to `staff` but is not exactly `staff` (`STAFF`, `staff.`, `STAFF~1`) is `404` for everyone. Staff files: `private, no-store`.
5. **Worker.** `sw.js` skips `c.put` when `Cache-Control` has `no-store`; names no path.
6. **Door.** `routes.tsx` points both routes at a lazy `StaffDoor` (own small chunk, keeps `admin/api` out of first paint). `StaffDoor` calls `getMe()`; on success imports `./AdminApp`; on any rejection of either step it renders the app's lazy `NotFoundPage` (the catch is inside the door, so no crash card). `AdminApp.tsx` is imported only from the door, which the plugin confirms.

## The numbers
- Staff files: 4 (3 JS, 1 CSS), 517 KB on disk (511.1 KB raw; the big one is 472 KB, about 109 KB gzip).
- First-paint chunk (`app-*.js`): before (base, same env) 214,339 bytes gzip, room 41,661 bytes; after 214,249 bytes gzip, room 41,751 bytes (90 bytes smaller; limit 250 KB, needs 8 KB). Budget, staff delivery and entrance checks all pass.
- **Bundle budget row removed:** `bundle-budget.json` had a route row `admin` (`routeChunk: AdminApp`, 600 KB). The chunk no longer has a name, and D-653 says there is no budget on `/app/staff/`, so the row is deleted (no number of any other row changed). Say so at integration.

## Report: staff-looking modules still in member chunks (wave 66)
`src/lounge/msg/staffActions.ts`, `src/lounge/shell/platformStaff.ts`, `src/i18n/messages/en/p55-staff-1.ts` (catalog fragment, D-658). Shared `admin/` modules that stay member code: `ReauthHost.tsx`, `SchemaForm.tsx`, `StaffDoor.tsx`, `UnsavedBar.tsx`, `admin.css`, `api.ts`, `permissions.ts`, `reauth.ts`, `routes.tsx`. Note `admin/api.ts` (every staff API call, URL and type) is a member file: the gate covers the Admin app code, not that client.

## Wave 64 chunks
The classification is graph-based, so `admin/bugs/BugReportsSection.tsx` and anything only Admin imports become staff on merge with no edit. `BugReportGate` (a lazy chunk): it is staff-only only if just Admin reaches it; if member code reaches it (the beta reporter), it stays a member chunk, correctly. If wave 64 adds a static import of a staff entry from a member file, the build fails naming the importer.

## Tests
- Rust `web_gate` (31 pass, includes the six asked for plus `a_staff_file_is_refused_when_there_is_no_admin_service`). **Watched fail with the appointment check disabled:** `a_staff_file_to_a_member_is_404...`, `..._refused_on_the_request_after_the_appointment_ends`, `..._no_admin_service`. The path test passes without the gate by design (the lookalike rule and absent files refuse by themselves); it covers `%2F`, `%2f`, `./`, `x/../`, `%2e/`, `STAFF`, `Staff`, `staff.`, `STAFF~1`, `staff%20`, `%5C`, `:` for a member and for appointed staff.
- vitest: `swFetch.test.ts` (no-store not stored, second request on the network, ordinary file still stored, 404 does not purge; **the no-store test watched fail without the sw change**), `staffDoor.test.tsx` (non-staff: not-found and importer never called; staff: page; rejecting importer: not-found; waiting: status), `staffChunks.test.ts` (graph walk, shared helper stays member, static import error names the importer), `admin.test.tsx` (25 pass). `src/admin`, `src/pwa`, `vite`, `firstpaint`, `session` suites green after the two test edits.
- `node --test scripts/*.test.mjs`: 124 pass (8 of them for `check-staff-delivery`: a pass fixture and one failing fixture per rule a-e).
- Playwright `e2e/tests/p65-gate.spec.ts` (chromium, built dist served by sr-server on 8431, `SR_E2E_WEB=server`): member `/admin` shows not-found, no `/app/staff/` request, `GET /admin/me` was made, a real staff file by URL and an absent one both `404` with an empty body; Owner loads `/admin/access-log`, every `/app/staff/` response is `200` and `no-store`, Cache Storage holds no `/app/staff/` entry. 3 passed.
- clippy `-p sr-server --all-targets -D warnings` clean; tsc, eslint, prettier clean on changed files; `cargo fmt` on the crate.

## Not done / not checked
- A revoked staff member with Admin open was not clicked live (no appoint/revoke flow in the spec). By reading: `useStaffRevoked` navigates to the Lounge, which unmounts the door; nothing re-imports in a loop; a later `/admin` visit asks `/admin/me`, is refused and shows not-found. The server side of it is the Rust test.
- The worker's `no-store` rule is covered by the harness test, not by a real browser with the worker installed (the spec checks Cache Storage on the page, which passes).
- A 401 from `/admin/me` (session ended) also shows not-found now (before: the Admin app's own "Sign in" card). Owner question below.

## Shots
`visual-sweep-shots/p65-gate` (next to the worktrees): `member-notfound-{desktop,phone}-{lounge-editorial,lounge-light,lounge-dark}.png`, `owner-first-...` (Dashboard) and `owner-accesslog-...` (a lazy section) in the same six combinations. Looked at: member not-found in all three themes on both sizes; Owner Admin in Editorial, Light and Dark on desktop and phone (layout, sidebar and phone section picker, tokens as before; the staff stylesheet loads from `/app/staff/`). Not every one of the 18 PNGs was opened; the ones not opened are the same code path with another theme.

## Proposed inventory rows (after SEC-AUD-169)
- SEC-AUD-170: the app build sorts staff-only chunks (graph-based, from the staff entries) into `dist/app/staff/<hash>` and writes `staff-chunks.json`; `check-staff-delivery.mjs` (CI after the bundle check) fails on an empty staff directory, a preload or static import of a staff file, a staff marker outside it or a named staff file; the first-paint closure may hold no staff file.
- SEC-AUD-171: `sr-server` serves `/app/staff/*` only to a session with an active staff appointment (`AdminService::resolve` on every request), the plain `404` otherwise (absent and present look the same), `private, no-store`; case, dot and 8.3 variants of the directory are refused.
- SEC-AUD-172: the worker never stores a `no-store` response; `/admin` enters through a staff door that asks `/admin/me` before importing Admin and shows the not-found page otherwise.

## Proposed threat-model rows
1. A member reads staff code by URL: `404` identical to an absent file; nothing to enumerate; names are hashes; tests in `web_gate`.
2. A revoked staff member's cached code: `no-store` plus the worker's no-store rule; the next request is refused; residual: code already in the memory of an open page until it closes (`SR_STAFF_ALERT` closes it).
3. The oracle (does this staff file exist): the member answer is decided before the file system and matches an ordinary absent `/app/` file in status, body and headers.
4. Path tricks (`%2F`, `./`, `..`, case, trailing dot or space, `~1`, backslash, colon): refused by `clean_relative` and the lookalike rule on every file system.
5. Residual, by design: the Admin labels (catalog, D-658), `admin/api.ts` and the staff-looking Lounge modules still reach members.

## Changelog sentence
Admin's code is now sent only to appointed staff: other members who open /admin see the "Page not found" page.

## Owner questions
- A signed-out `/admin` (a 401 from `/admin/me`) shows the not-found page, where it used to show the Admin "Sign in to continue" card. Prefer a redirect to the login page instead? (Default kept: not-found, the simplest and the same for everyone.)

## Cleanup done
Server stopped, heavy lock released, databases `sr_p65_gate_ui` and `sr_p65_gate` dropped, `p65-gate-target` and the binary copies deleted. `apps/web/dist` in the worktree is untracked build output.
