# Wave 92, invoices builder: SEC-AUD-405 (D-918), Admin Invoices' "Search" button; the Invoices and Entitlements tables with rows

T = 11:47 on 2026-11-17. Locks: round 1 12:02 to 12:04 (base and after, found the first CSS did not fit and the table data), round 2 12:09 to 12:11 (after only, the final CSS). Finished 12:15. Both rounds were under 10 minutes; the lock was released each time and `heavy.sh status` showed `invoices` before each start.

## Item 1: the "Search" button

### Base (ran it: `SR_P92_INVOICES_BASE=1`, `donations.css` as the base has it)
1280 Editorial and Lounge Light (identical): toolbar content width 828. Items: State 270 (label above, control 155-195), Purpose 145, search field 241 (label 134, control 155-195, hint below, box height 82), checkbox row 131 (margin-block-start 21, 155-195), Search button 81 x 36 (margin-block-start 23). The five need 270+145+241+131+81 = 868 plus four gaps of 8 = 900 px: 72 px more than the row has. Search wrapped to row two (top 247) and kept its 23 px offset: hanging low. Toolbar height 149.
1024 Editorial: content 585, State and Purpose on row one, field + checkbox + button on row two (the 23 px offset put the button on the controls' line there).
375 Lounge Light and Dark: the figures of `fix-p90-invoices.md:19` (controls 44 px, labels above, hit test passes, toolbar 284 high, no sideways scroll).

### Why the five cannot share the row
With the search field at its 200 px floor the five still need 270+145+200+131+81+32 = 859 px against 828 at 1280: 31 px short (the State select is 270 px wide because of its longest option; shrinking it would clip that option). The brief's fallback applies: **"Search" takes row two at the start edge with no block offset.**

### What I built (`donations.css`, `.inv-toolbar`, 721 px and up only)
The field takes the rest of the first row (`flex: 1 1 200px; min-inline-size: 0`), a zero-height `::after` (flex-basis 100%, `order: 1`) breaks the line, the checkbox row and the button (`order: 2`) share row two at the start edge with no block offset, and the button gets `min-block-size: 40px` (the controls' height). The old 21 px and 23 px offsets are removed. One comment naming SEC-AUD-405 and D-918. Not touched: the 720 px and under rules.
Also one rule in the same file (table item, below): `.adm-audit li { overflow-wrap: anywhere }`.

### After (ran it; measure.json)
- 1280 Editorial and Lounge Light: row one State 270 / Purpose 145 / field 397 (200 or more), labels all at 134 and controls all at 155, 40 high (D-900 kept, to the pixel). Row two at y 232: checkbox row at x 340 (the start edge) and Search at x 478.7, both 40 high, tops equal, margin-block-start 0. Toolbar height 138 (base 149). Hint under the field whole. No sideways scroll.
- 1024 x 800 Editorial: row one State + Purpose, row two the field full width (585), row three checkbox + Search at the start edge, 40 high, no offset. Three rows, nothing overlaps, no sideways scroll of the page.
- 375 Lounge Light and Dark: every item's box, control box and hit result equal to the base (compared in the JSON: equal, 0 px); toolbar 284 high; selects and the field 44 px and pass the hit test; Search (36 high) and the checkbox row pass the hit test as they did on the base (the kit's 44 px on a phone does not apply to this button; unchanged).
- Row gap: the rows end up 16 px apart (the empty break line sits between two 8 px gaps). Removed a `margin-block-end: -8px` from the pseudo after the round (a flex line's cross size is never below zero, so it did nothing: the measured figures are with and without it the same); the last CSS was not re-measured after that removal.

## Item 2: the tables with rows

### The way (reading `crates/sr-api/src/routes/donations`)
There is no manual invoice route. A real invoice is `POST /donations/invoices` (a member), which needs: a wallet (staff `PUT /admin/donations/wallet`), donations on (`PATCH /admin/donations/config {enabled:true}`), a chain URL (`SR_PAYMENTS_ESPLORA_URL`, only needs to be set) and a price quote (`SR_PAYMENTS_RATE_SOURCES` with one public price API). No payment provider, no payment is made. My stack used `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` (the watcher cannot reach it; nothing leaves the machine) and Coinbase's keyless public spot price from D-288 (one read-only GET per minute, no account, nothing of ours sent). The spec makes everything through the API: signet wallet (the BIP-84 test vector as a tpub), donations on, three new accounts, five invoices (patron, donate x2, invite credits x2), one resolved by staff, three grants (patron 30 days with a long reason, patron permanent, a server pool for a server with a very long name). Seed log: `visual-sweep-shots/p92-invoices/seed-log.txt`. The display name cannot be longer than 32 (the API refuses it), so the longest name is a 32-character username and a 32-character display name.

### What I saw and measured (1280 Editorial; 375 Lounge Light and Dark; 1024 as well)
- Invoices at 1280: a real table, five rows, no sideways scroll of the page (0), no cut text, long name wraps in two lines. 1024: the table scrolls in its own box (wrapper 585, table 683), the Created column is cut at the box edge with no cue. 375: page does not scroll sideways (0); the table is 683 px wide in a 343 px box that scrolls sideways: Invoice and Payer show, For is half cut, Amount, State and Created are off screen with no cue. It does not stack (the stacked-rows rule of D-874 is for `DataTable` only; this is a bare `.adm-table`). Row controls (the invoice id link buttons, 15 px boxes) pass the hit test at 375 in Lounge Light and Dark (0 faults; their boxes are 15 px high, the hit test is the criterion, D-856).
- Invoice detail (opened): 1280 and 375 whole, no sideways scroll, transactions ("None seen." no payment was made) and audit trail shown. At 375 the audit line is JSON with no spaces and ran 746 px in a 303 px line: cut. **Fixed** (`.adm-audit li { overflow-wrap: anywhere }`, in `donations.css`; the shot after shows it wrapped). Buttons (close, "Resolve by hand") pass the hit test at 375 (36 px high). At 1280 "Resolve by hand" sits right under the last audit line with no space.
- Entitlements (a list of rows, not a table): 1280 whole, no cut text; 375 page does not scroll sideways, "Revoke" buttons (28 px high) pass the hit test. The first two rows' second badge ("ADMIN") is cut at 375: the row's name line shows "ACTIVE ..." (ellipsis).

### Fixed / listed
Fixed: the audit line wrap (one small rule). Listed, not built (each larger than one rule or outside the file):
1. Invoices table at 375 and 1024 scrolls sideways in its own box with Amount, State and Created off screen and no cue. Proposed SEC-AUD-408: stack the Invoices rows on a phone as `DataTable` does (D-874), or give the wrapper a visible scroll cue; touches `donations.css` and probably `admin.css`.
2. Entitlements row at 375: `.ss-name` (server-settings.css) clips the second badge with an ellipsis. Proposed SEC-AUD-409: let the name line wrap in this list.
3. Invoice detail at 1280: `.adm-actions` has no space above it ("Resolve by hand" touches the audit line). Proposed SEC-AUD-410: a 12 px margin-block-start (one rule, not built: not shot after).

## Who imports my files
`git grep`: `InvoicesSection.tsx` and `EntitlementsSection.tsx` only by `admin/AdminApp.tsx`, which only `admin/StaffDoor.tsx` imports lazily (`import('./AdminApp')`); `donations.css` only by the four `admin/donations/*Section.tsx`. Admin chunk only: no first paint, no public page, no web build run. `.inv-toolbar` is carried only by the Invoices form; `.adm-audit` is used only in the Invoices detail.

## Tests run
e2e `tsc`; prettier --check on the CSS and the spec; `node scripts/check-css-tokens.mjs` (clean); `node scripts/i18n-audit.mjs --max 0` (TOTAL 0); `node scripts/check-doc-links.mjs` (ok); no `.tsx` changed (no web tsc, eslint, vitest). The spec `p92-invoices-shots.spec.ts` ran: base (`SR_P92_INVOICES_BASE=1`, passed, nothing asserted) and after (round 1: failed with the first CSS (button still low) and with my probe's over-strict 44 px height check on row controls, both fixed; round 2: passed, `problems []`). So the spec failed on the first CSS, not on the base.

## Shots (`visual-sweep-shots/p92-invoices`, 12, all 12 opened by me after the release)
`before-invoices-toolbar-desk-lounge-editorial.png`, `invoices-toolbar-desk-lounge-editorial.png`, `invoices-toolbar-desk-light-lounge-light.png`, `invoices-toolbar-tab-lounge-editorial.png` (1024), `invoices-toolbar-phone-light-lounge-light.png`, `invoices-rows-desk-lounge-editorial.png`, `invoices-rows-phone-light-lounge-light.png`, `invoices-rows-phone-dark-lounge-dark.png`, `entitlements-rows-desk-lounge-editorial.png`, `entitlements-rows-phone-light-lounge-light.png`, `invoice-detail-desk-lounge-editorial.png`, `invoice-detail-phone-light-lounge-light.png`. The shots of round 2 are the final CSS; the "before" shot is the base.

## The commander's command (from `e2e/`, after the sweep world is seeded on the same database; the stack needs `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` and `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`)
`SR_P92_INVOICES_ON=1 SR_E2E_BASE_URL=http://localhost:5704 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P92_INVOICES_OUT=<folder> npx playwright test tests/p92-invoices-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (add `SR_P92_INVOICES_BASE=1` for the base case: measures the toolbar only, shots `before-`; run it with the base `donations.css`, the spec does not switch the CSS).

## Docs
No doc states the toolbar's layout; searched `docs/`, `README.md`: nothing to change.

## Proposed inventory text
SEC-AUD-405 (closed, wave 92, D-918): on Admin's Invoices page at 721 px and up the search field takes the rest of the first row (200 px or more) and the "Needs attention" row and the Search button take the second row at the start edge, 40 px high, no block offset (five items need 859 px, the row has 828 at 1280); phone unchanged; the Invoices table and Entitlements list were seen with real rows at 1280, 1024 and 375, and the audit line of the invoice detail wraps. Opened: SEC-AUD-408 to 410 as above. D-900's row: "the Search button: D-918".

## Threat-model rows
None.

## Changelog sentence
Admin's Invoices filters now keep Search with the "Needs attention" box on a row of its own under the filters, and an invoice's audit trail wraps on a phone instead of being cut off.

## Owner questions
None. Default chosen: the fallback (a second row) rather than shrinking the State select to fit five items on one row.

## Cleanup
Stack stopped by PID tree (a stray Vite listener on 5704 killed by PID after each round), ports 8704/5704/9404 free, `sr_p92_invoices_ui` dropped (checked), `p92-invoices-bin` deleted (`p80-base-bin` untouched), lock released. `git stash` not used and not touched. Scratch files are in my scratchpad folder, none in the repo.
