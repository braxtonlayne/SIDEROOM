# Wave 71 ops handoff (SEC-AUD-266..272; D-745, D-747)

Branch `fix/p71-ops`. A first builder ran out of room; a second finished it. Nothing of mine is running: the stack is stopped, the lock is released, the scratch databases and the cargo target and binaries are deleted (see the end).

## Done (commits, oldest first)

| Commit | Item |
|---|---|
| `f9041f9e` | SEC-AUD-270 installer: resumes, single-quotes every value, keeps every backup (D-747) |
| `51c95a44` | SEC-AUD-271 staff chunk list is not a member's file (D-652) |
| `60a95316` | SEC-AUD-266, 267, 268, 269 (Rust, web, local-only shots spec). One commit: the four items share `routes/bugs/mod.rs`, `allow_list.rs` and the staged tests, so they could not be split cleanly |
| `eae2de9e` | SEC-AUD-272 docs: the wave 71 sections of `CHANGELOG.md` and `deploy/UPGRADE.md` (ops clauses), `deploy/*`, `docs/*` |
| `e818c601` | `packages/api-client/openapi.json` and the generated client regenerated (the list gains `before`, `has_more`) |
| (next after it) | SEC-AUD-267: `bugs.css` wraps a long reporter name so the Open button stays in the table; the shots reach "Load more" and the lower half of a report |
| (this file) | handoff |

## What each item does and what was proved

**266, a report with parts missing.** The server (`allow_list.rs`) always stores every key of the contract type, with a neutral value (`viewport` 0/0/0, `gateway` `{status:"unknown"}`, `voice` `{connected:false}`, `sw` "none", `online` true, `caps` with `engine`, `failed_requests` and `crashes` empty). The page (`BugReportsSection.tsx`) guards every read, because rows stored before this change stay as they are. Fields that can be absent in an OLD row: `viewport`, `gateway`, `voice`, and (guarded the same way) `caps`, `crashes`, `failed_requests`, `online`, `sw`. Tests: route test (a body of only `{"description":"x"}` is stored with every key), allow-list test (`BUNDLE_KEYS`), vitest `p71BugReports.test.tsx` (a minimal and an old-shape report render; a full one renders as before). Not run on the base by me (the first builder wrote the tests; I did not re-run them against base code).

**267, the list pages.** `GET /api/v1/admin/bug-reports?before=<id>`: the repo orders id descending with `id < before` and reads `limit + 1` rows; the answer carries `has_more` (a new boolean; the total alone cannot say it for a status filter). The page has "Load more" under the list (appends; the status filter starts the list again; a failed load shows a message and keeps the rows). Tests: repo test (Postgres, `--ignored`, three pages of 60 reach every report once, with a status filter too: passes), route test `the_list_pages_with_before_and_says_when_more_exist`, vitest for Load more and for the filter reset. Regenerated `openapi.json` and the client.

**268, no name or code in a path.** `allow_list/path_words.rs` holds the sorted fixed words of the known API paths (from the first columns of `route_matrix.tsv`); any other segment becomes `{}`; a query string or fragment is cut first. Tests: `every_path_of_the_matrix_survives_as_its_own_template`, a test that every fixed segment of the matrix is in the list and nothing else is, `/profile-pages/alice` and a lower-case invite code become `{}`. The web (`pathTemplate.ts`) keeps what it already knew and leaves the rest to the server. Old rows are not rewritten; I did not count the kinds of path the old rule could store (any lower-case word in any position, so: any username, invite code or slug).

**269, limiter before decode; audit after found.** `allow_report` runs before the base64 decode; a body the allow-list then refuses gives its slot back (`release_report`), so a refusal costs nothing against the account's hour or the platform's day (tests `a_refused_report_does_not_use_up_the_hour`, `a_refused_picture_gives_its_slot_back...`, `the_sixth_report_in_an_hour_is_refused`). In `sr-admin`, `require_bug_access` only checks permission; the route looks the report up and then calls `record_bug_access`, which writes the entry before anything is returned. If the entry cannot be written the read fails (as before: the old `bug_entry` did the same). An id that is not there, or a picture the report does not have, leaves no entry (`reading_or_closing_a_report_that_is_not_there_leaves_no_audit_entry`; a successful read writes exactly one entry: `closing_a_report_deletes_its_picture_and_writes_an_audit_entry` and the sr-admin tests).

**270, installer** (`scripts/install.test.mjs`, 20 tests, all pass; nothing installed or downloaded). Resume: with the env files present and no `--force` the script reads `KEY=value` (never sources), writes nothing, and runs every step again. Detection of "done": pull (a second pull of an unchanged image downloads nothing), Postgres/Garage (`up -d --wait` leaves a healthy container), the Garage bootstrap (layout only when the node has none, key and bucket only when missing, permission re-granted), migrate (only unrecorded migrations), ensure-cors (same rule again), starter packs (safe to repeat), final `up -d`, the health wait and preflight (only read or start what is not running). Only the Owner account has state: `sr-cli bootstrap` refusing a finished install is read as "done"; library's half-done fix is not depended on and not fought. `--force`: `<file>.bak.<UTC yyyymmddThhmmssZ>`, mode 600, never over an earlier backup. `envq` single-quotes every value; a single quote or line break is refused naming the answer. The Owner password length is checked before any file is written (same constant as `commands.rs`, with a test that keeps the two equal). The env template is filled by one checked step at a time (no pipe). `garage/bootstrap.sh` reads the three `GARAGE_*` keys instead of sourcing the file (the file is also always safely quoted now). What a missing web root does to the server: `crates/sr-server/src/web.rs` returns `WebError::MissingCsp` when `[web] root` is set and `app/csp.txt` is missing, so the server REFUSES TO START (it is not only a failed health wait); the installer now checks `$SR_WEB_DIR/app/csp.txt` before it writes or starts anything.

**271, staff chunk list.** The plugin used to write `dist/app/staff-chunks.json`, which `sr-server` sent to any signed-in member (only `app/staff/` is staff-only). It now writes `apps/web/staff-chunks.json` (beside `dist/`, untracked, `.gitignore`d), `check-staff-delivery.mjs` reads it there and gains rule h (a `staff-chunks*` file under `dist/app/` fails). A member asking for the old address gets the ordinary `/app/` 404 (no file there). After my build: `dist/app/` has no `staff-chunks*`; `apps/web/staff-chunks.json` exists.

**272, docs.** See the docs list below; `node scripts/check-doc-links.mjs` passes.

## Checks run in the heavy lock (this session)

- `cargo test -p sr-api --lib -- routes::bugs`: 41 passed. `cargo clippy -p sr-api --tests -- -D warnings`: clean (the two errors in `allow_list/tests.rs` fixed). Earlier round (first builder's log): sr-admin tests 0, sr-db ignored bug_reports tests 0, sr-server/sr-cli build 0, openapi regenerated 0.
- Web: vitest `p71BugReports.test.tsx` and `src/bugreport/__tests__` (8 files, 42 tests) pass; `tsc --noEmit` clean; prettier clean on my files; `node scripts/i18n-audit.mjs --max 0`: TOTAL 0.
- Web build, then `check-bundle-budget` (ok), `check-entrance-isolation` (every public surface clean), `check-staff-delivery` and its test (ok), `node scripts/install.test.mjs` (20 pass).
- **First-paint figure:** `app/app-DbQTZbiI.js` 215813 bytes gzip, room 40187 bytes to the 250 KB limit (needs 8 KB). I have no "before" build of the base to compare with (no base build was available and I did not make one); this wave's web changes are in lazy Admin files and the report helpers, and the staff chunk list change moves no chunk's content, so it should not have moved. The commander can compare against the integration build's figure.
- `cargo fmt` and the full crate suites are left to CI.

## Shots

`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p71-ops`, 56 PNGs from `e2e/tests/p71-ops-shots.spec.ts` (local only; one test, passes; 60 reports seeded straight into the scratch table with `psql` because the limiter allows five an hour, plus one minimal report posted with `page.request` and two made through the dialog): list first page (top and the "Load more" end), after Load more (end of list, button gone), closed filter (empty), the minimal report, the old-shape report, the dialog report (top), and the lower half of the minimal and old-shape reports; each at desktop 1280x800 and phone 375x812 in Editorial, Lounge Light and Lounge Dark, plus the "Report sent" dialog (desktop and phone, Editorial).

I opened every PNG of the first run (38) and fixed what I saw: the shots did not reach "Load more" or the end of the list (the admin overlay scrolls inside itself, so full-page shots show the top), and the Open button was cut off at the right of the table on desktop (a 16-character reporter name did not wrap). After the fix I re-ran and opened the new or changed shots: 36 of the 56 (the "Load more" end, after Load more, lower halves, closed filter and the list top at all themes and both sizes). The 20 detail-top and dialog shots of the second run were not opened again (same code and CSS as the 20 I opened in the first run). All looked right: "Unknown" and "none" wording for missing parts, the Load more button centred under the list, no horizontal scroll on desktop, a phone list that scrolls inside its wrapper as before.

## Docs updated (all in `eae2de9e`, plus `51c95a44` for the entrance doc)

`CHANGELOG.md` (Phase 71 section), `deploy/UPGRADE.md`, `deploy/FAQ.md`, `deploy/GO-LIVE.md`, `deploy/INSTALL.md`, `deploy/OPERATOR.md`, `docs/04-data-model.md`, `docs/05-api-and-gateway.md`, `docs/07-permissions-and-staff.md`, `docs/09-retention-and-privacy.md`, `docs/specs/admin-staff-server.md`, `docs/specs/lounge-voice-video.md` (D-745: music is sent again after `connect()` on a rejoin), `docs/dev/entrance-isolation.md`.

Changelog clauses: the **Staff** and **For operators** bullets and the voice-tests bullet came from the ops work itself (each checked against the code above). **Still missing, from the other builders' handoffs (none had arrived; nothing was written from the plan alone):** library's sentences (migration 3050, the Library's language codes, the bootstrap command), signals' (typing, badge and notification across a block, a role and a person's default), panels' (preview closing its markup, pins as messages, search on a phone), and library's "what an operator should look for" sentence for `deploy/UPGRADE.md`. `docs/dev/owner-checklist.md` needed no change (its soak row already says a green run closes MUS-JB-001 and that `voice-live.spec.ts` covers a rejoin). `node scripts/check-doc-links.mjs`: "doc links ok" (re-run at the end).

## Dead keys

None added by me. The old admin bug strings are still used.

## Proposed inventory rows (the commander adds them after SEC-AUD-252)

- SEC-AUD-266 (Med): A bug report with missing parts broke the staff page. Fixed: every contract key is always stored with a neutral value; the page guards every read for older rows. Test: route, allow-list and vitest.
- SEC-AUD-267 (Med): The staff list showed only the newest 200 at most and the page showed 50. Fixed: `before` and `has_more`; "Load more". Test: repo (Postgres), route, vitest.
- SEC-AUD-268 (Med): A failed request's path could carry a name or invite code. Fixed: only fixed words of the known API paths are kept; everything else is `{}`. Old rows not rewritten.
- SEC-AUD-269 (Low): The limiter ran after the picture was decoded; a read of nothing wrote an audit entry. Fixed: limiter first (a refused body gives its slot back); the entry is written once the report or picture is found, and a failed write fails the read.
- SEC-AUD-270 (Med): The installer could not be run again, wrote unquoted values (a mail password with `;` ran as a command when the env file was sourced), overwrote its backup, and checked the Owner password only after writing. Fixed (D-747).
- SEC-AUD-271 (Low): `app/staff-chunks.json` was served to any signed-in member. Fixed (D-652): written beside `dist/`; `check-staff-delivery` rule h.
- SEC-AUD-272 (docs): D-745 and the fixed rejoin; wave 71 changelog and upgrade notes.
- MUS-JB-001 wording: "Music is sent again after a full rejoin (the controller republishes after `connect()`, whose teardown drops outgoing streams); `voice-live.spec.ts` covers an ICE restart and a full rejoin. A green soak dispatch run (the `music` input) closes it." Replace any text that says music is lost on a rejoin or "not yet sent again".

## Proposed threat-model rows (number after T77)

1. A member's bug report as a way to break the staff page and hide other reports (a body with parts missing crashed the detail view; a list that could not page hid reports beyond 200). Mitigation: always-complete stored report, guarded page, paging.
2. A name or a code in a stored failed-request path (a username in `/profile-pages/<name>`, an invite code): mitigation: fixed-word allow-list, `{}` for the rest.
3. The audit log padded with reads of nothing (a staff member, or a script with a staff token, asking for ids that do not exist): mitigation: the entry is written only once the report is found; the audit entry itself is never optional.
4. An env file that runs a command when sourced (a mail password with `;`, a backtick or `&` in an unquoted value): mitigation: every value single-quoted, never sourced by the installer, `garage/bootstrap.sh` reads three keys.
5. The staff chunk list readable by members (names and module paths of every staff chunk): mitigation: not in `dist/app/`; a build check fails if it returns.

## Owner questions (for the Fable agent)

1. `online`: when the browser sends no `online` value the server stores `true` (the neutral value). The Admin page then shows "Yes" for a report whose browser said nothing, while old rows show "Unknown". Is `true` right, or should a missing value be stored as unknown (needs a nullable field)? I kept the plan's contract.
2. A handle equal to a route word survives in a stored path: a member named like a fixed API word (for example a profile page at `/profile-pages/library`) is indistinguishable from the word and is kept. It names no one who is not already public, so I left it; say if the web should also blank every segment after a known parameter position.
3. `has_more` is a new response field (the total alone cannot answer it under a status filter). Accepted as the smallest change; say if the plan wanted something else.

## Cleanup

Dropped `sr_p71_ops` and `sr_p71_ops_ui`; deleted `p71-ops-target` and `p71-ops-bin`; stopped my sr-server and Vite by PID; released the heavy lock. Nothing of mine is queued or running.
