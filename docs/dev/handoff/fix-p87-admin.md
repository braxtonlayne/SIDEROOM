# Wave 87 admin handoff: SEC-AUD-379 (D-889), the Crash reports list has no sideways scroll at 1280

T = 16:40 on 2026-11-16 (I started at about 16:27 by `date`, the commander allowed it). Clock times: no new item 17:10, checkpoint 17:22, finished 17:30, hard limit 17:40. Finished at about 16:40.
Branch `fix/p87-admin` (base 5a2b551f). Web only, no cargo, no Rust, no new string, no web build, no `.tsx`.

## Done
- `crash/crash.css`: one rule after the `.adm-crash-table .adm-mono` rule: `.adm-crash-table td.adm-mono:nth-last-child(2) { white-space: normal; overflow-wrap: anywhere; }`. The route cell is the second to last cell of a crash row, so it can be named without a new class. The frames row's single `td.adm-mono` is a lone child and does not match; ids, times, build and the actions cell stay on one line.
- `e2e/tests/p87-admin-shots.spec.ts` (new; header gives the commander's command).

## Who imports `crash.css`
Only `admin/crash/CrashReportsSection.tsx` (`import './crash.css'`); that section is loaded by `AdminApp.tsx` with a dynamic `import()`: staff chunk, not first paint, not a public page. No web build run.

## Base (confirmed, and that I ran it)
Base case = the same build with a style tag that gives the route cell `nowrap !important` back (what `crash.css` did before). Ran on my stack (8654/5654/9354), sweep world seeded, 1280x800 in Editorial and Lounge Light. The world's Crash list has 3 rows of its own; the spec makes one more with one `POST /crash-reports` call, with a longer route (`lounge.:guildId.channels.:channelId.threads.:threadId`). Wave 86's 93 px came from a shorter route; with this longer one the base overflow is 234 px. Both themes gave the same figures.

| 1280 | before | after |
|---|---|---|
| wrapper `scrollWidth - clientWidth` | 234 px | 0 |
| table width / wrapper width | 1061.8 / 828 | 828 / 828 |
| column widths (fingerprint, count, first, last, build, route, actions) | 181, 66.3, 94, 94, 113.6, 433.4, 79.5 | 181, 66.3, 115, 114.2, 113.6, 158.4, 79.5 |
| wide cell | route (433.4) | fingerprint (181) |
| Open button inside the wrapper | no (right edge 1392 > 1168) | yes |
| route lines (long row) | 1 | 4 |
| row heights (list rows and frames rows) | 67.5, 25.5, 67.5, 42.5, 67.5, 25.5, 67.5, 59.5 | 84.5, 25.5, 54.9, 42.5, 54.9, 25.5, 54.9, 59.5 |

The only taller row is the long-route one (+17 px, one line; it wraps to 4 short lines of the narrow cell but the row is held up by the badge and times). The other rows got shorter (the times can wrap less, since the table is now narrower and the columns share the width). No row is more than two lines taller. The document itself never scrolled sideways (0 before and after).
375, Lounge Dark: overflow 0, row heights, column heights and Open button identical with and without the rule (the phone rule in `admin.css`, `.adm-table.adm-table-stack .adm-mono`, sets the same two values, so the rule changes nothing there; "before" at 375 is therefore the page as it is, a 0 px move by construction, the spec compares the two measurements: identical). The rule does not out-rank the phone rules: it sets only `white-space: normal; overflow-wrap: anywhere`, the same as theirs.
Click-through (1280, Editorial and Light): Open on the first row opens the report (card, Stack frames table, Browser features, Replay buffer); the report's table boxes and cells are the same with and without the rule (detailMoved 0). The world's report has no replay ticks, so the replay table is an empty state; not measured with rows.

## What is left of SEC-AUD-363
After this item: the Arcade page's heading, the bare controls of the Entrance and Arcade admin pages, the Invoices label, the Invoices, Entitlements and Transcoding tables when they have rows ("No end date" is the donations builder's, 378). The Crash list's 1280 scroll leaves the list.

## Tests
`admin/__tests__/crashReports.test.tsx`, single file: 5 pass. prettier on my two files (clean), `check-css-tokens` (clean), e2e `tsc` (clean), my spec with `--project=chromium --reporter=line` (passed three times; the last run made the shots). `check-doc-links` run after writing this file.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-admin`: 5 png, **all 5 opened by me**, after the lock was released: 1280 Editorial before (1), after in Editorial and Lounge Light (2), 375 Lounge Dark after (1), the opened report at 1280 Editorial (1). Plus `measure.json` and `totals.json`. Note the final run's crash report call was rate limited (429, logged and ignored); the earlier run's row stayed in the database, so the long-route row is in the shots.

## Commander's command for the spec
From `e2e/`, on a stack with the sweep world seeded (ports 8654/5654):
`SR_P87_ADMIN_ON=1 SR_E2E_BASE_URL=http://localhost:5654 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<url> DATABASE_URL=<url> SR_P87_ADMIN_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-admin npx playwright test tests/p87-admin-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (also in the spec's header; optional `SR_P87_ADMIN_SETTINGS`, `SR_P87_ADMIN_BASE=1`).

## Proposed inventory row
SEC-AUD-379 (D-889): "The Crash reports list has no sideways scroll at 1280: the route cell wraps (`crash.css`, one rule); ids, times, build and the actions cell stay on one line. Was 234 px wider than its wrapper with a long route (93 px in wave 86), the Open button out of sight; now 0, the button inside. 375 unchanged. Closed."

## Changelog sentence
Admin's Crash reports list no longer scrolls sideways on a desktop: a long route wraps inside its cell and the Open buttons stay in view.

## Cleanup
Stack stopped by PID tree (sr-server 29064 and Vite 32092 and children; ports 8654, 5654, 9354 free), `sr_p87_admin_ui` dropped, `p87-admin-bin` deleted (`p80-base-bin` untouched), lock released. No stash touched.

## Owner questions
None.
