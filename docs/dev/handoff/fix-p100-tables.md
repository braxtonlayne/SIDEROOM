# Wave 100, tables: SEC-AUD-441 (D-949), every Admin table wrapper is a named keyboard stop while it scrolls sideways

Clock: started 04:53; lock round 1 (base, unedited tree) 05:07 to 05:11; round 2 (after) 05:19 to 05:24; round 2b (375 only, a spec fix: the nav `waitFor` needed `state: 'attached'` at 375) 05:26 to 05:28. Each time `heavy.sh status` showed `tables` before the stack started. `p99-reach-shots` and `p98-cue-shots` are not yet run (a third round, not before 06:30; see "Older specs").

## Base (unedited tree, Lounge Light; scrollWidth/clientWidth; S = scrolls; stop = number of Tab stops on the wrapper from the control before it; R = has role region)
Figures: `visual-sweep-shots/p100-tables/base-light-{1024,900,375}.json`. 12 wrappers were reachable in the world (the other pages list no rows or no wrapper: "no rows: not measured").

| table | 1024 | 900 | 375 | last column holds a control |
|---|---|---|---|---|
| Crash reports, groups | 692/585 S, stop 0 | 692/496 S, stop 0 | 343/343 | yes (Open) |
| Crash reports, stack frames (detail) | 551/551 | 462/462 | 309/309 | no |
| Accounts | 862/585 S, stop 0 | 862/496 S, stop 0 | 343/343 | yes (View) |
| Invites | 585/585 | 508/496 S, stop 1 (native, see below) | 508/343 S, stop 1 | no |
| Servers | 585/585 | 543/496 S, stop 0 | 343/343 | yes |
| Review queues, Applications (DataTable) | 585/585 | 496/496 | 343/343 | no |
| Cinema catalog, Titles | 585/585 | 496/496 | 343/343 | yes |
| Cinema catalog, Skip windows (detail tab) | 551/551 | 462/462 | 333/309 S, stop 0 | no |
| Invoices | 637/585 S, stop 1 (R) | 637/496 S, stop 1 (R) | 343/343, stop 1 (R) | no |
| Pricing and perks | 706/551 S, stop 0 | 706/462 S, stop 0 | 309/309 | yes (inputs) |
| Platform audit log | 636/585 S, stop 1 (native) | 636/496 S, stop 1 (native) | 343/343 | no |
| Access log | 585/585 | 525/496 S, stop 1 (native) | 343/343 | no |

So the fault is as the plan says, and wider than wave 99 measured: 4 wrappers scroll at 1024 (Crash, Accounts, Pricing, Audit) and 8 at 900. **Chromium makes a scrolling container with no focusable child a keyboard stop on its own** (Invites, Audit log, Access log: "stop 1" with no role and no name, so a screen reader announces nothing useful). A scrolling wrapper that holds a button or an input (Crash, Accounts, Servers, Pricing) is no stop, and its last columns cannot be reached by keyboard except through a control inside them. Not reached or no rows in the world: Bug reports (2 tables), Cinema transcoding queue, Seasons episodes, Moderation subtree-case table, Invite graph list, Library catalog, Jobs (System): all moved the same way (below); the part measures them itself when they have rows.

## What changed (the diff in words)
- New `apps/web/src/admin/TableRegion.tsx`: `TableRegion({ label, className, children })` renders `div.adm-table-wrap`; a `ResizeObserver` on the wrapper and on the first `table` inside it (Seasons has a Banner before its table, so "first child" was not enough) keeps `scrollWidth > clientWidth`; when true the div has `role="region"`, `aria-label={label}`, `tabIndex={0}`, when false none of the three. No `ResizeObserver` (the unit-test DOM) means false. No key handler, no script, no new string, no fragment.
- All 17 wrappers moved (16 bare, plus Invoices, whose three attributes were replaced): `DataTable.tsx` (`label`), `CrashReportsSection.tsx` x3 (`tableLabel`, `framesLabel`, `replayLabel`), `PricingSection.tsx` (`perksPerTier`), `InvoicesSection.tsx` (same title string; `inv-table` is still the wrapper's direct child, so cue's `:has(> .inv-table)` shade still matches), `BugReportsSection.tsx` x2 (`tableLabel`, `failedLabel`), `cinema/{CatalogSection,MarkersPanel,SeasonsPanel,TranscodingSection}.tsx` (`catalog.table`, `markersPanel.windows`, `seasonsPanel.ep.tableAria`, `transcoding.queue.table`), `SubtreeCases.tsx` (a table with no name: the nearest heading's own text, derived from the same `<h3>`, no new key), `AccessLog`, `InviteGraph`, `Invites`, `System` (their tables' `aria-label` strings). No wrapper was left bare.
- `admin.css` is not changed: the ring rule was NOT needed (below). `donations.css` not touched.

## After (round 2, same database; Light 1024 and 900, Dark 1024, Light 375 touch; figures `after-*.json`)
- Every wrapper that scrolls is exactly one Tab stop, `role="region"`, `aria-label` = the string passed (read from the attributes; the spec records it and compares it with the table's own name). 1024: Crash, Accounts, Invoices, Pricing, Audit. 900: those plus Invites, Servers, Access log. 375: Invites (508/343) and Skip windows (333/309) still scroll there, so they are regions; no other wrapper is a stop at 375, Invoices included (343/343, no role, 0 stops), and the page does not scroll sideways (0).
- Every wrapper that fits: no role, no tabindex, 0 stops (Crash frames, Servers at 1024, Applications, Cinema titles, Skip windows at 1024 and 900, Invites at 1024, Access log at 1024 and 375, and so on).
- ArrowRight from the focused wrapper reaches `scrollWidth - clientWidth` for all 5 at 1024 (Crash 107, Accounts 277, Invoices 52, Pricing 155, Audit 51 to 54; both themes) and all 8 at 900 (Accounts 366, Pricing 244, and so on), the last header's box inside the wrapper each time; ArrowLeft returns it to 0 (the spec records a problem otherwise; none).
- Boxes: every header and row box and each wrapper's clientWidth, scrollWidth and height equal the base (0 moved) for every table at 1024, 900 and 375 **except the two live logs**, Platform audit log and Access log. The base was taken before the run opened crash reports and pages, which write new audit and access rows (the audit shot shows new `crash.report.read` rows timed 5:20), so their row count (175 to 189 cells at 1024; 147 to 203 at 900) and some column widths (by 0.1 px, or by longer text: Access log 525 to 510 at 900; Audit log 2317 to 2495 px high) differ. The first 42 cells of the Audit log at 1024 are equal. This is data, not the change: the part adds attributes only.
- The ring: no CSS. A keyboard-focused region (`:focus-visible`) has the browser's `auto 1px` outline, Light `rgb(16, 16, 16)`, Dark `rgb(238, 238, 238)`, offset 0; the shots show a clear ring round the wrapper in both. So there is no rule at the end of `admin.css`.

## Who imports `TableRegion`
Only files under `apps/web/src/admin/` (`git grep -n TableRegion -- apps/web/src`): `DataTable.tsx` and the 13 section files. They load through `AdminApp`, which only `import('./AdminApp')` in `StaffDoor.tsx` loads (wave 98's and 99's read of the same chain). Admin chunk only; not first paint, not a public page. No web build run (the commander's).

## Older specs
See the end of this file (updated after the third round, or "not run").

## Shots (`visual-sweep-shots/p100-tables`, 6, all 6 opened by me after the release)
`accounts-1024-focus-lounge-light.png`, `accounts-1024-focus-end-lounge-light.png`, `crash-1024-focus-lounge-dark.png`, `pricing-900-focus-lounge-light.png`, `audit-1024-focus-lounge-light.png`, `invoices-375-lounge-light.png`. Each focus shot shows a clear ring round the region (black in Light, light grey in Dark) and the table cut only by its own scroll; the end shot shows State, Joined and View reached. Invoices at 375 shows stacked rows, no frame.

## The commander's command (from `e2e/`, sweep world seeded on the same database; stack ports 8781 / 5781 / 9481, `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9`, `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`)
`SR_P100_TABLES_ON=1 SR_P100_TABLES_PHASE=after SR_E2E_BASE_URL=http://localhost:5781 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P100_TABLES_OUT=<folder> npx playwright test tests/p100-tables-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
On a fresh database run `SR_P100_TABLES_PHASE=base` first (it seeds the five invoices; `SR_P100_TABLES_NOSEED=1` skips that). `after` compares with `base-*.json` in the out folder when they exist. On a database whose logs have grown, the Audit log and Access log boxes will differ from the base (see above): expected, not a fault. `SR_P100_TABLES_SETTINGS=light-1024,light-900,dark-1024,light-375` picks settings.

## Checks (after the last edit)
- web `tsc --noEmit -p tsconfig.json`: exit 0 (05:13). eslint (package's own) on the 13 changed TSX files, `TableRegion.tsx` and the unit test: exit 0 (05:13). prettier on them: no change. No web file edited since.
- e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (05:24, after the last spec edit); `prettier --check` on the spec: clean (05:24).
- vitest, single files: `p100-table-region.test.tsx` 3 passed (05:13); `p10.test.tsx` (renders `DataTable`, unedited) 23 passed at 04:55 (before the change) and at 05:18 (after); one run at 05:15 failed on the 5 s timeout of "account drawer extras" (5097 ms) while other builders' stacks ran; `crashReports` 5, `bugReports` 3, `cinema` 23, `donations` 12, `moderation` 10, `p39invites` 4, `p15graph` 11: all passed.
- `node scripts/i18n-audit.mjs --max 0`: TOTAL 0, exit 0 (05:28). `node scripts/check-doc-links.mjs`: ok (05:28). `check-css-tokens.mjs`: not run (no CSS added).
- The spec: the base run passed (the 1024 setting was rerun once because the first page had not loaded its nav in time; a 20 s `waitFor` was added); the after run failed only on the live-log comparisons and, at 375, on a nav `waitFor` (visible instead of attached; fixed); the 375 rerun failed only on the Audit log and Access log boxes (live data). No other entry in `problems`.

## Proposed rows
- SEC-AUD-441 (low, Admin, accessibility), closing text: Admin tables that scroll sideways could not be reached by keyboard (Crash reports, Accounts, Pricing and perks at 1024; Servers too at 900) or were an unnamed stop (Platform audit log, Invites, Access log: Chromium focuses a scroller with no focusable child, with no role or name). All 17 wrappers now go through `TableRegion`: while the table is wider than its wrapper the wrapper is `role="region"`, named with the table's own string and `tabIndex={0}`, and the arrow keys reach the last column; while it fits it is a plain wrapper and no stop (Invoices on a phone included). The browser's focus ring shows in Light and Dark.
- No row for the shade on other tables: the shots do not ask for it.

## Changelog sentence
Every Admin table that scrolls sideways can be focused with the keyboard (it announces its name) and scrolled with the arrow keys, and a table that fits is no longer an extra stop.

## Owner questions
None.

## Cleanup
Written at the end of the work.
