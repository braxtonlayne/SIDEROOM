# Wave 98, cue: SEC-AUD-438 (D-945), the Invoices wrapper's edge shade, and the wave's one web build

Clock: started 16:30 (T moved up by the commander), lock round 16:33 to 16:35 (two spec runs in it, 16:34 and 16:35), build round 16:36 to 16:37. Finished 16:45. The heavy lock was released after each round; `heavy.sh status` showed `cue` before each start.

## Base (ran it: the rule switched off by an injected `background-image: none !important` style; Lounge Light and Dark give the same figures)
- 1024 x 768: wrapper clientWidth 585, scrollWidth 665 (scrolls by 80 px; 79 on the first run, 5 rows). Page scroll 0. Wrapper height 712 (10 rows; 372 with 5). Column headers (x, width): Invoice 326.7/161.8, Payer 488.5/153.8, For 642.3/79.3, Amount 721.6/77.8, State 799.4/99, Created 898.4/93.3. Out of sight at scroll start: Created (cut at the edge, shows as a "C"). Nothing marks it (base shot opened).
- 900 x 768: client 496, scroll 665 (+169). Out of sight: State (cut) and Created.
- The table fits from 1200 (1100: client 648, scroll 665; 1200: 748 = 748). So the fault is as the plan says.
- Grounds: wrapper, table, thead, header row, th, body rows and td all `rgba(0, 0, 0, 0)`, td background-image none: nothing paints over the wrapper. The ground under it is the first opaque ancestor, `div._root_ _wide_` (the Admin modal's content root): `--ui-bg` (Light rgb(255,255,255), Dark rgb(49,51,56)). `--ui-bg-overlay` is NOT it in Dark (43,45,49): my first guess was wrong and the spec's ground check caught it in round 1.
- Desktop hit test (the shared tool: `openCollapsed`, then `hitTestAll` over the wrapper at scroll start; not a touch size): 10 controls (the invoice id buttons, 15 px boxes), all `fail` in the base, taker `th < tr < tbody` (the row's own header cell), 0 not-reached. The same list after (equal, compared in the spec). D-856's test is for 375 touch, where the rows are stacked and nothing changed.

## What I built (`apps/web/src/admin/donations/donations.css`, one block at the end, under `@media (min-width: 721px)`, comment names SEC-AUD-438 and D-945)
`.adm-table-wrap:has(> .inv-table)`: four background layers. Two covers in `var(--ui-bg)` (`to right` / `to left`, opaque for the first 50% of 32 px, so the 16 px shade is fully hidden), `background-attachment: local`, at the two edges. Two shades (`color-mix(in srgb, var(--ui-text) 16%, transparent)` to transparent, 16 px), `scroll`, at the same two edges. `no-repeat`. No new element, no script, no `!important`, nothing in `admin.css` or the TSX. Round 1 (covers in `--ui-bg-overlay`, opaque for only 9.6 px) let the shade show through where it should be hidden and had the wrong colour in Dark; fixed to `--ui-bg` and 50%.
**The CSS in my last commit is the CSS I measured** (round 2, 16:35; the commit followed, no CSS edit after it) and the CSS the build ran on.

## After (round 2, both themes; the spec's `problems` is `[]`)
- The shade is measured by pixels: the two 16 px edge strips of the wrapper are cut from a screenshot with the rule off and on and compared byte for byte ("differs" = a shade). Light and Dark alike: scroll start: left same, right differs; middle: both differ; scroll end: left differs, right same. At 1200 (the table fits): neither differs.
- Headers 0 of 6 moved; every row and cell box equal to the base; wrapper clientWidth, scrollWidth and height equal at 1024 and at 900.
- Hit test after = base (same 10 rows, same statuses and takers). The shade is a background and takes no tap.
- Light and Dark shots both opened: the shade is a soft grey band at the edge, faint but seen in both (16% of `--ui-text`; no other alpha tried).
- 375 x 812 touch (Light): stacked rows, all 60 row and cell boxes equal to the base, wrapper scroll = client = 343, computed `background-image` `none`, page does not scroll sideways.
- Other Admin tables: `git grep -n "inv-table"` finds the class only in `InvoicesSection.tsx`, this sheet and earlier specs; the selector needs a direct `.inv-table` child. Crash reports' `.adm-table-wrap`, read in the page: `background-image: none`.
- RTL: one measurement (`dir="rtl"` on the root, scrollLeft 0, 1024): scrolls 585/665, the shade is on the left (where the table has more), none on the right: both edges alike. That one measurement only; no RTL shot.
- Crowding: none in the after shots; nothing moved against the base (0 px).

## Web build (16:36 to 16:37, `heavy.sh run cue`, on the tree of commit 013a0ff5)
`pnpm --filter @sideroom/web build`: done. `check-bundle-budget.mjs`: exit 0, first paint `app/app-BZMAy-Dn.js` **218 365 B gzip, room 37 635 B** (before: 218 362 B, room 37 638 B: +3 B, the chunk hash; no app code changed, only an Admin sheet). `check-entrance-isolation.mjs`: exit 0, "every public surface is clean". `check-staff-delivery.mjs`: exit 0.

## Who imports my sheet
`donations.css` is imported by `InvoicesSection`, `EntitlementsSection`, `OverviewSection` and `PricingSection` (all in `admin/donations`), which `AdminApp.tsx` loads; `AdminApp` only by `StaffDoor.tsx` through `import('./AdminApp')`. Admin chunk only: not first paint, not a public page. The block is scoped to the wrapper that holds `.inv-table`.

## Shots (`visual-sweep-shots/p98-cue`, 6, all 6 opened by me after the release)
`before-invoices-1024-start-lounge-light.png` (base), `invoices-1024-start-lounge-light.png`, `invoices-1024-end-lounge-light.png`, `invoices-1024-start-lounge-dark.png`, `invoices-fit-1200-lounge-light.png`, `invoices-375-lounge-light.png`. `measure.json`, `totals.json` and `seed-log.txt` are beside them. The world holds 10 invoices (the spec's seed ran twice: round 1 failed on my first CSS, round 2 reseeded); the first run's figures (5 rows) agree (scroll 664, +79).

## The commander's command (from `e2e/`, after the sweep world is seeded on the same database; stack with `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` and `SR_PAYMENTS_RATE_SOURCES` as in the spec's header; ports 8762 / 5762 / 9462)
`SR_P98_CUE_ON=1 SR_E2E_BASE_URL=http://localhost:5762 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P98_CUE_OUT=<folder> npx playwright test tests/p98-cue-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

## Proposed inventory row
SEC-AUD-438 (low, Admin; opened and closed in wave 98, D-945): from 721 px up to 1199 px (the table fits from 1200 in the seeded world) the Invoices table scrolls in its own wrapper with the last columns out of sight and nothing to show it. The wrapper now draws a soft shade on each inline edge that has more table beyond it (CSS scroll-shade, backgrounds only, `--ui-bg` covers and a 16% `--ui-text` shade); none at an edge that is at its end, none where the table fits, none on a phone.

## Changelog sentence
Admin's Invoices table now shows a soft shade at the edge where it scrolls sideways.

## Keyboard today (for "Left for later")
The wrapper is not focusable (no `tabindex`, no name). The only tab stops in it are the invoice id buttons in the first column, and tabbing to one scrolls the wrapper to it; the last columns (plain text) can be reached with a mouse, touchpad or wheel but not by the keyboard alone. A focusable, named region (TSX) would fix that.

## Owner questions
None. Defaults chosen: shade 16 px at 16% of `--ui-text`; covers in `--ui-bg`.

## Cleanup
Stack stopped by PID tree (server, Vite and children), ports 8762 / 5762 / 9462 free, `sr_p98_cue_ui` dropped, `p98-cue-bin` deleted (`p92-base-bin` and `p80-base-bin` untouched), no `acquire` loop of mine alive, `git stash` not used or touched. Final `heavy.sh status` is in the reply.

## Checks on the final tree (16:38, after the last edit; the spec and the CSS are unchanged since the commit)
- `npx prettier --check` on the spec and the CSS: exit 0.
- `npx tsc --noEmit -p tsconfig.json` from `e2e/`: exit 0.
- `node scripts/check-css-tokens.mjs`: clean, exit 0. `node scripts/i18n-audit.mjs --max 0`: TOTAL 0. `node scripts/check-doc-links.mjs`: ok, exit 0.
- The spec itself (`--project=chromium --reporter=line`): passed in round 2 (1 passed, 31.7 s).
