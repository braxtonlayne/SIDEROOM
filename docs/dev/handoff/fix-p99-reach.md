# Wave 99, reach: SEC-AUD-440 (D-948), the Invoices wrapper is a named, focusable region

Clock: started 16:55, lock round 17:01 to 17:03 (`heavy.sh status` showed `reach` before the start; one spec run), released 17:03. Finished 17:05.

## Base (the three attributes taken out of the DOM in the page, same run; Light and Dark give the same figures)
- 1024 x 768, 5 seeded rows: wrapper clientWidth 585, scrollWidth 636 (scrolls by 51), height 355.
- Tab from the filter form onwards: 5 stops in the wrapper, all invoice id buttons; the wrapper is never `document.activeElement`.
- Focus on the last id button, ArrowRight twice: `scrollLeft` stays 0 of 51; the last header ("Created") is out of the wrapper. So the fault is as the plan says.
- Hit test (shared tool, `openCollapsed` then `hitTestAll`): 5 controls, all `fail`, taker `th < tr < tbody` (the row's header cell), 0 not-reached. (Cue's 10 was with 10 invoices; this world makes 5, as cue's first run did.)

## What changed (the diff in words)
`InvoicesSection.tsx`: the `div.adm-table-wrap` got `role="region"`, `aria-label={t('admin.donations.invoices.title')}` and `tabIndex={0}`. Nothing else. No new string, no fragment, no script. `donations.css` is NOT changed: the CSS rule for the ring was not needed (below).

## After (same run, Light and Dark at 1024 x 768; Light at 375 x 812 touch; the spec's `problems` is `[]`)
- Tab order: the wrapper is exactly one stop, before the first id button (stops: wrapper, then the 5 buttons). Role `region`, name "Invoices" (attributes read, and `getByRole('region', {name: 'Invoices'})` finds 1, i.e. the accessibility tree).
- With focus on the wrapper: ArrowRight (4 presses) takes `scrollLeft` 0 -> 51 = `scrollWidth - clientWidth`; the last header is inside the wrapper's box; ArrowLeft brings it to 0. The wrapper keeps focus.
- Shade (cue's, D-945): the computed `background-image` is cue's four layers, unchanged (Light `rgb(255,255,255)` covers, Dark `rgb(49,51,56)`); in the end shot the start edge has the shade and the end edge does not.
- Ring: no CSS needed. With the keyboard-focused wrapper (`:focus-visible` true) the computed outline is the browser's `auto 1px` (Light `rgb(16,16,16)`, Dark `rgb(238,238,238)`), offset 0. Both shots show a clear dark/light ring round the wrapper, not cut by the modal.
- Boxes at 1024: 0 of 6 headers moved, every row and cell box equal, wrapper clientWidth, scrollWidth and height equal (both themes).
- Hit test after = base (same 5 rows, same statuses and takers), both themes. The wrapper is not in the control list.
- 375: stacked rows, all 30 row and cell boxes equal, wrapper scroll = client = 343, page does not scroll sideways. The region is one more tab stop on a phone (the wrapper does not scroll there); not handled, no width switch.

## Who imports my files
`InvoicesSection.tsx` is imported by the Admin donations section, which `AdminApp.tsx` loads; `AdminApp` only through `import('./AdminApp')` in `StaffDoor.tsx` (wave 98's read of the same chain). Admin chunk only: not first paint, not a public page. No web build run (the commander's).

## Shots (`visual-sweep-shots/p99-reach`, 4, all 4 opened by me after the release)
`invoices-1024-focus-lounge-light.png`, `invoices-1024-focus-end-lounge-light.png`, `invoices-1024-focus-lounge-dark.png`, `invoices-375-lounge-light.png`. `measure.json`, `totals.json`, `seed-log.txt` beside them.

## The commander's command (from `e2e/`, sweep world seeded on the same database; stack ports 8772 / 5772 / 9472, `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9`, `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`)
`SR_P99_REACH_ON=1 SR_E2E_BASE_URL=http://localhost:5772 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P99_REACH_OUT=<folder> npx playwright test tests/p99-reach-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
Note: the spec's base takes the attributes out of the DOM, so it also passes on a tree without the change only in its "after" half (role check fails there, as it should).

## Checks (after the last edit; no edit since 16:57)
- `npx prettier --write` on the spec and the TSX: unchanged, exit 0 (16:56).
- e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (16:56). Web `tsc --noEmit -p tsconfig.json`: exit 0 (16:57). eslint (package's own) on `InvoicesSection.tsx`: exit 0 (16:57).
- `node scripts/i18n-audit.mjs --max 0`: TOTAL 0, exit 0 (17:02). `node scripts/check-doc-links.mjs`: ok (17:02). `check-css-tokens.mjs`: not run (no CSS added).
- The spec: 1 passed (43.9 s), the seed printed no `FAIL`.

## Proposed rows
- SEC-AUD-440 (low, Admin, accessibility; opened and closed in wave 99, D-948): from 721 px to 1199 px the Invoices table scrolls in its wrapper, and the last columns could be reached with a pointer but not by keyboard (the wrapper was no tab stop; only the id buttons were). The wrapper is now `role="region"`, named with the Invoices title, `tabIndex={0}`; arrow keys scroll it to the last column; the browser's focus ring shows in Light and Dark.
- Proposed (number from SEC-AUD-441), the other Admin tables: `git grep -n "adm-table-wrap"` finds 17 more wrappers. Measured in the seeded world at 1024 (scrollWidth/clientWidth): Crash reports 690/585, Accounts 861/585, Pricing and perks 706/551, Platform audit log 632/585 scroll sideways; Invites, Servers, Cinema catalog, Access log fit (585/585). Apply D-948 to those that scroll and hold columns with no control (check which hold controls in the last columns), probably through `DataTable.tsx`; measure at 1024 and 900 first. Not done: not this wave.

## Changelog sentence
Admin's Invoices table can now be focused with the keyboard and scrolled sideways with the arrow keys.

## Owner questions
None.

## Cleanup
Stack stopped by PID tree (server, Vite and children), ports 8772 / 5772 / 9472 free, lock released 17:03. Scratch DB and `p99-reach-bin` removal: see the reply. `git stash` not used or touched.
