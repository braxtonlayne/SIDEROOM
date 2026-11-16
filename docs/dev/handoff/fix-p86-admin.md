# Wave 86 admin handoff: SEC-AUD-375 (D-881), Admin's three hand-made tables as stacked rows on a phone

T = 16:10 on 2026-11-16 (I started at 15:55 by `date`, the commander allowed it). Clock times: no new item 16:50, checkpoint 17:05, finished 17:15, hard limit 17:25. Finished at about 16:05.
Branch `fix/p86-admin` (base fa213ab3). Web only, no cargo, no Rust, no migration, no new string, no web build.

## Done (commits)
- `SEC-AUD-375: Access log, Crash reports and Cinema catalog as stacked rows on a phone (D-881)`: `adm-table-stack` on the three `<table>`s (the Crash list only; a report's frames and replay tables unchanged), `data-label` on each body cell with the string its `<th>` renders (`admin.accessLog.col*`, `admin.crash.col*`, `admin.catalog.col.*`; the first cell of each row is the row's title and has no label, as in wave 85), `data-actions` on the Crash list's and the Catalog's actions cell (Access log has no actions). Nothing else in the three `.tsx`. One vitest case in `admin/__tests__/crashReports.test.tsx`. The spec `e2e/tests/p86-admin-shots.spec.ts`.
- `admin.css` (second commit, see below): two rules inside wave 85's phone block.

## Who loads what
- `AccessLog.tsx`, `CrashReportsSection.tsx`, `CatalogSection.tsx` are Admin sections (`AdminApp`, loaded by `StaffDoor.tsx` with a dynamic `import()`): staff chunk, not first paint, not a public page. `admin.css` is loaded by `admin/ui.tsx` and `admin/SchemaForm.tsx` (lazy settings chunks); my two new rules are scoped to `.adm-table.adm-table-stack`, which no member page has.

## The one CSS finding (needed, beyond wave 85's block)
`crash/crash.css` (not mine, loaded later) keeps `.adm-crash-table .adm-mono` on one line (`white-space: nowrap`) and the last cell `text-align: end; white-space: nowrap`. First run: Crash reports after the class still had overflow 42 px, 7 cells and 1 action past the edge. Added inside the phone query, with higher specificity (`.adm-table.adm-table-stack`): `.adm-mono { white-space: normal; overflow-wrap: anywhere }` and `td[data-actions] { text-align: start; white-space: normal }`. After that, 0 everywhere. `crash.css` was not touched (the label column stays 6.5rem).

## Base (confirmed, and that I ran it)
Base case = the same build with the class taken off the tables (the spec does it per page load; every rule is scoped to that class). Ran on my stack (8644/5644/9344), sweep world seeded, 375x812 touch, Lounge Light and Dark. The world's Access log had 16 to 26 rows, the Crash list 8 rows (I made one more with one `POST /crash-reports` call; the others came from the sweep world), the Cinema catalog 1 row ("A11y Film"); Invoices and Transcoding are empty states (no table).

| Page (375) | wrapper overflow before | cells past right edge before | actions past edge / hit fails before | after: overflow / cells off / actions off / hit fails | min label contrast light / dark |
|---|---|---|---|---|---|
| Access log | 178-181 px | 48-78 (rows 16-26) | no actions | 0 / 0 / - / - | 6.47 / 5.41 |
| Crash reports | 578 px | 26 | 1 of 4 Open buttons off / 0 | 0 / 0 / 0 / 0 (4 actions) | 6.47 / 5.41 |
| Cinema catalog | 83 px | 1 | 2 of 4 off / 3 fail | 0 / 0 / 0 / 0 (4 actions, all 44 px) | 6.47 / 5.41 |

Both themes gave the same figures. Contrast is computed (label colour over the first opaque ground), all above 4.5.
Long values wrap inside the row: the Access log's 19-digit actor id was 5 lines before (a narrow column) and is one line after; the Crash route (`lounge.:guildId.channels.:channelId`) wraps in two lines inside its row. The Catalog's first cell (the poster, a drawn box without text) counts as an "empty line" in the spec's figure; it is the poster image, not an empty label.
Click-through, both themes: Open on the first Crash row opens the report (card, stack frames table), "Back to the list" returns to 8 rows.
1280 Editorial: first six rows' cell boxes and table box compared class off against class on: max 0 px moved on Access log, Crash reports, Cinema catalog and Accounts. (The Crash list's wrapper is 93 px wider than its box at 1280 on the base too and unchanged: the route's nowrap; not this item.)
Other `.adm-table` users: Cinema transcoding and Donation invoices are empty states in the world (no table to compare), 0 px moved there at 375 and 1280; Invoices shot at 375 Dark.

## Caption probe (for the commander, nothing built)
1280, Editorial, Accounts: there is no `<caption>` element in `DataTable`'s table (the name is an `aria-label`). The `.adm-vh` span (the actions column's header text "Actions") is `position: absolute`, a 1 x 1 box at (1101, 419), `clip: rect(0px, 0px, 0px, 0px)`, `overflow: hidden`: hidden to the eye as wave 85 meant.

## What is left of SEC-AUD-363
The three tables landed. Left: the Arcade page's heading, the bare controls of the Entrance and Arcade admin pages, the Invoices label, Invoices/Entitlements/Transcoding tables when they have rows (empty in the world, so not seen), and the Crash list's 93 px sideways scroll at 1280 (a desktop item, wrapper scrolls inside its box, the page does not).

## Vitest
`admin/__tests__/crashReports.test.tsx`, single file: 5 pass (4 before + 1 new: the table has `adm-table-stack`, each body cell's `data-label` is its column header, the first cell has none, the last has `data-actions`). I did not run the new case against the base file (the spec's base case is the base check).

## Checks
web `tsc --noEmit`, eslint (the package's) and prettier on my files, `check-css-tokens` (clean), `i18n-audit --max 0` (0), e2e `tsc`, all clean. The spec passed twice with `--project=chromium` (the second time the full run that made the shots). `check-doc-links`: run after writing this file.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-admin`: 15 png, **all 15 opened by me**, after the lock was released: the three pages at 375 Dark before (3), at 375 Dark and Light after (6), at 1280 Editorial after (3), the click-through (Dark and Light, 2), Invoices empty at 375 Dark (1). Plus `measure.json` and `totals.json`. Not in any shot: a Crash row's top-frames line stacked (the 375 shots are the top of the page; the frames rows measured 0 overflow).

## Commander's command for the spec
From `e2e/`, on a stack with the sweep world seeded (ports 8644/5644):
`SR_P86_ADMIN=1 SR_E2E_BASE_URL=http://localhost:5644 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<url> DATABASE_URL=<url> SR_P86_ADMIN_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-admin npx playwright test tests/p86-admin-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (also in the spec's header; optional `SR_P86_ADMIN_ONLY`, `SR_P86_ADMIN_SETTINGS`, `SR_P86_ADMIN_BASE=1`). Note: the spec posts one crash report; the server rate-limits that (429), which the spec logs and ignores.

## Proposed inventory row
SEC-AUD-375 (D-881): "Admin's Access log, Crash reports list and Cinema catalog are stacked rows on a phone (same class and labels as the shared table). Were 178, 578 and 83 px wider than their box at 375; now 0, every action 44 px and inside the viewport. Closed." SEC-AUD-363 stays open with what is listed above.

## Changelog sentence
Admin's Access log, Crash reports and Cinema catalog now show as stacked rows on a phone, with each value labelled and the actions on their own line, instead of tables that scrolled sideways.

## Cleanup
Stack stopped by PID tree (server 33140, Vite 6176 and children; ports 8644, 5644, 9344 free), `sr_p86_admin_ui` dropped, `p86-admin-bin` deleted (`p80-base-bin` untouched), lock released. No stash touched.

## Owner questions
- The Crash list at 1280 scrolls sideways inside its wrapper (93 px, route text kept on one line by `crash.css`, unchanged since before). Let it wrap on a desktop too?
