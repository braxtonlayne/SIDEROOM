# Wave 93, admin: SEC-AUD-409, 410, 411, 423 (D-925, D-929): Invoices stacks on a phone, Entitlements badges whole, room above "Resolve by hand", a long payer name

Started 12:54 (T was 13:15), ready 13:00. Lock: taken 13:11 (behind lounge, kit and study, which went in ahead of the order), one stack round with base and after, released 13:15 (about 4 minutes; `heavy.sh status` showed `admin` before the stack started). Inside the round I fixed two rules and the spec and ran base and after again on the same stack (described below). Finished 13:25.

## The price stub (D-929): refused, so the Invoices list and detail are faked
Read only, `crates/sr-server/src/payments_jobs.rs`: `SR_PAYMENTS_RATE_SOURCES` always goes through `ReqwestGet::new(false)`, the strict policy (private and local hosts are refused; the private-host opt-in is for the Esplora node only, SEC-AUD-028, D-358). A stub on 127.0.0.1:9424 would not be read, so no quote and no real invoice. I did not start a stub and set no `SR_PAYMENTS_RATE_SOURCES`; the outside URL was never used and nothing outside was called. **The invoices list (`GET /admin/donations/invoices`) and the detail are faked with `page.route`**: five invoices with 18-digit ids like real ones; payers: a 32-character name with spaces and a hyphen, a 32-character name with no space, two short names, a detached payer; several states. The wallet, donations-on, three accounts and the three grants (Entitlements) are real.

## Base (ran it: `SR_P93_ADMIN_BASE=1` on the same code; the spec takes the wave's classes off the elements and sets the audit margin to 0, which is the base's CSS)
- **409, 375 Lounge Light:** wrapper 343, table 792 (with 18-digit ids; 701 with short ids), scrolling sideways inside the wrapper; Invoice and Payer show, For starts at x 448; Amount, State and Created (x 527 to 808) are off screen. 1280: wrapper 828 = table 828; 1024: wrapper 585, table 792 (scrolls in its box). Column boxes at 1280: Invoice 163.9, Payer 268.1, For 88.4, Amount 77.8, State 108.3, Created 121.5; rows 50.5 and 51 high.
- **410, 375:** first two rows' `.ss-name`: box 259.7 x 23.4, `scrollWidth` 280 and 277 against 260, `overflow: hidden`, `white-space: nowrap`: the "ADMIN" badge (57.7 wide, x 238.8) runs past the box and is cut; the third row is whole. 1280 and 1024: whole.
- **411:** gap between the last audit line and `.adm-actions`: 0 px at 1280, 1024 and 375.
- **423: not as the plan says.** At 1280 Editorial the payer cell (268.1 wide) holds both 32-character names on one line, whole: `overflow: visible`, `white-space: normal`, `overflow-wrap: normal`, `word-break: normal`, no ellipsis rule applies (`scrollWidth` = `clientWidth` = 268, the text range lies inside the cell). The commander's wave 92 shot (`p92-integ/invoices/invoices-toolbar-desk-lounge-editorial.png`) shows the name wrapped on two lines, whole, not cut. I could not reproduce a cut at 1280, 1024 or 375.
- Hit test (shared helper), 375 Light and Dark, base: Invoices 5 controls, 0 fail (the id buttons pass on the base); detail 3 controls: in my first run, with 5-digit ids, "Close" failed "below" (taker: the dialog body, `div._body`), on the base and after; in the final run with 18-digit ids, 0 fail on both (a kit Modal footer fact, see the proposed row); Entitlements 3 controls, 0 fail.

## Rules as built (`donations.css`, `InvoicesSection.tsx`, `EntitlementsSection.tsx`; `admin.css` unchanged)
- **409:** the table is `adm-table adm-table-stack inv-table`; every `td` has `data-label` with its header's own `t()` string (no new string): Payer, For, Amount, State, Created. The row's title is a `<th scope="row">` that holds the id button; `admin.css`'s D-874 rule reaches `td` only, so in `@media (max-width: 720px)` `.adm-table.inv-table th[scope='row']` is a block with the title's look (700, no uppercase, text colour), and its button has `min-block-size: 44px`. (The stacked id button, 15 px high, lost the point 21 px below to the next cell, the payer `td`: 5 of 5 failed with that taker until the 44 px was added.) Above 720 px nothing changes; between 721 px and the width where the table fits it scrolls in its wrapper as before (D-817).
- **410:** `ul.ss-list.ent-list`; `.ss-list.ent-list .ss-row .ss-name { overflow: visible; text-overflow: clip; white-space: normal; overflow-wrap: anywhere }`; the badges drop to a second line when they do not fit.
- **411:** `.adm-audit + .adm-actions { margin-block-start: 12px }`.
- **423:** `.adm-table.inv-table .inv-payer { overflow: visible; text-overflow: clip; white-space: normal; overflow-wrap: break-word; word-break: normal; hyphens: none }`. **Deviation from the contract:** `break-word` in place of `anywhere`. I built `anywhere` first and measured it: at 1024 it lowers the column's minimum to one letter, the table then fits its wrapper (585) and the Payer column collapses to 122.8 px with names on three lines, moving the other columns. `break-word` keeps every column box equal to the base and still breaks inside a word when a word is wider than its cell. It is a guard: nothing was cut on the base.

## After (ran it; `measure.json`, `totals.json`: `problems []`)
- **409, 375 Light and Dark:** wrapper 343, table `scrollWidth` 343, document 0; each row shows the id, then Payer, For, Amount, State, Created each with its label (all five labels on the first two rows, read in the JSON); hit test on the table 5 controls, 0 fail, 0 not-reached (Light; Dark the same). The payer name breaks at the hyphen or at the width (2 lines, whole). The table is 925 high (base 287: rows are stacked). **1280 and 1024:** column boxes equal to the base to the pixel, except For 88.4 to 88.5 and Created 121.5 to 121.4 at 1280 (the date text differs by a minute between the two runs); 1024 wrapper 585, scroll 792 as the base.
- **410, 375 Light and Dark:** both badges whole on the first two rows (`.ss-name` 259.7 x 46.8, `scrollWidth` 260 = `clientWidth`, "ADMIN" on a second line at x 16); "Revoke" passes (3 controls, 0 fail); no sideways scroll. 1280 and 1024: the same boxes as the base (744.7 x 23.4 and 502 x 23.4).
- **411:** 12 px at 1280, 1024 and 375 (margin read back 12px); "Resolve by hand" passes at 375 (detail: 3 controls, 0 fail); no sideways scroll.
- **423:** 1280 and 1024: every payer name whole and inside its cell, same cell boxes as the base (268.1 wide; rows 50.5 and 51), nothing moved.
- **Is the CSS in my last commit the CSS I measured? Yes.** Run 1 measured an earlier CSS (`anywhere`, no 44 px id button) and failed; I changed those two rules and ran base and after again on the same stack, still inside the lock; the figures above are of the committed CSS (`a9777ff6`).

## Who imports my files
`git grep -n`: `InvoicesSection.tsx` and `EntitlementsSection.tsx` only by `admin/AdminApp.tsx`, which `admin/StaffDoor.tsx` loads with `import('./AdminApp')`; `donations.css` by the four `admin/donations/*Section.tsx`. Admin chunk only: not first paint, not a public page. No web build run.

## Tests run
e2e `tsc`, web `tsc`, `eslint` on the two `.tsx`, `prettier --check` on all my files, `donations.test.tsx` (12 passed; nothing it reads changed), `node scripts/check-css-tokens.mjs` (clean), `node scripts/i18n-audit.mjs --max 0` (TOTAL 0), `node scripts/check-doc-links.mjs`. My spec: base (passed, nothing asserted), after run 1 (failed: 5 id-button hit-test fails with the payer cell as taker, a column squeezed at 1024 by `anywhere`), after run 2 (passed, `problems []`). So the spec watched the first CSS fail, and the base was measured with the same spec.

## Shots (`visual-sweep-shots/p93-admin`, 8, all 8 opened by me after the release)
`before-invoices-rows-phone-light-lounge-light.png`, `invoices-rows-phone-light-lounge-light.png`, `invoices-rows-phone-dark-lounge-dark.png`, `invoices-rows-desk-lounge-editorial.png`, `before-entitlements-rows-phone-light-lounge-light.png`, `entitlements-rows-phone-light-lounge-light.png`, `invoice-detail-desk-lounge-editorial.png`, `invoice-detail-phone-light-lounge-light.png`. Seen: the phone table is stacked rows with labels and whole names, Dark reads the same; the Entitlements base shows "ACTIVE ..." cut on two rows, the after shows ADMIN on its own line; the detail has 12 px above the button at both widths.

## The commander's command
From `e2e/`, after the sweep world is seeded on the same database. The stack needs `SR_ADMIN_REQUIRE_REAUTH=0` and `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9`, no rate source, ports 8714 / 5714 / 9414:
`SR_P93_ADMIN_ON=1 SR_E2E_BASE_URL=http://localhost:5714 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P93_ADMIN_OUT=<folder> npx playwright test tests/p93-admin-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(add `SR_P93_ADMIN_BASE=1` for the base case, which seeds; then the after run with `SR_P93_ADMIN_NOSEED=1`; or the after run alone on a fresh database).

## Docs
No doc states the Invoices table's layout or the Entitlements badges; searched `docs/` and `README.md`: nothing to change.

## Proposed inventory text
- SEC-AUD-409 (closed, wave 93, D-925): on a phone Admin's Invoices table is stacked rows (`adm-table-stack`, `data-label` from the header strings, the id button 44 px); at 375 the table's width equals its wrapper's (343), every column shows with its label, 0 hit-test fails; 1280 and 1024 unchanged.
- SEC-AUD-410 (closed): the Entitlements name line wraps in that list (`.ent-list`), so the second badge is whole at 375.
- SEC-AUD-411 (closed): 12 px above "Resolve by hand" in the invoice detail.
- SEC-AUD-423 (closed as not reproduced, with a guard): the payer name was whole on the base at 1280, 1024 and 375; the payer cell now breaks between words, and inside a word only when it must (`overflow-wrap: break-word`), and is never clipped. The contract's `anywhere` squeezes the Payer column to 123 px at 1024.
- D-874 and D-881's rows: "Invoices: D-925".
- New, from SEC-AUD-426, not built: the kit Modal's footer "Close" button failed the phone hit test "below" once, on the base and after (taker: the dialog body, `div._body`); it did not with 18-digit ids. Seen once, in one run's data; the kit's, listed.

## Threat-model rows
None.

## Changelog sentences
- Admin's Invoices table is stacked rows on a phone, so every column shows with its label instead of three being off screen.
- Entitlements badges wrap under the name on a phone instead of being cut off.
- The invoice detail has room above "Resolve by hand".
- A long payer name in the Invoices table breaks between words and is never clipped.

## Owner questions
None. Defaults chosen: `break-word` in place of `anywhere` for the payer cell (measured); a faked Invoices list and detail because the server's price client refuses a local source.

## Cleanup
Stack stopped by PID tree (server 4836, Vite 9244), ports 8714 / 5714 / 9414 / 9424 free (checked), lock released 13:15, `sr_p93_admin_ui` dropped, `p93-admin-bin` deleted (`p92-base-bin` and `p80-base-bin` untouched). `git stash` not used and not touched. Nothing under `data/` committed.
