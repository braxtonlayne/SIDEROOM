# Wave 86 donations builder: SEC-AUD-376 (D-882)

T = 16:10 on 2026-11-16 (clock times: no new item 16:50, checkpoint 17:05, finished 17:15, hard limit 17:25). I started early (15:55) with the commander's go and took the free lock at 15:57; round 15:57 to 16:00 (stack, seed, base run, after run). Finished at about 16:02.

## Done
- `PricingSection.tsx:105`: `className="adm-check-row don-check-row"` (nothing else).
- `donations.css`, phone block (`max-width: 720px`): `.don-check-row { display: flex; align-items: center; gap: 8px; min-height: 44px; }` (8px is the gap `label.adm-check-row` has in `admin.css:622`; that rule is untouched) and `.adm-perk-input { font-size: 16px; }`. The kit's `Input.module.css` `.control` sets no font size (`font: inherit`), so 16px is stated explicitly. Nothing outside the phone block changed.
- Imports checked: `admin/donations/**` is imported only by `AdminApp.tsx` (lazy Admin chunk); no web build run.

## Base (ran it; my change reverted in the tree, `SR_P86_DONATIONS_BASE=1`)
375 touch, Lounge Light and Dark: "Donations are on" checkbox 13 x 13, its label row 20 px high, hit test FAILS; 30 perk fields 14 px, 44 px tall. Fault confirmed. (A tap on the label text already toggled it, as the label wraps the box.) 1280 Editorial: fields 14 px, 40 px tall.

## After (same spec, change applied)
- 375 Lounge Light and Dark: label row 309 x 44, hit test passes; a touch tap on the label text toggles it and a second tap restores it (nothing saved); 30 of 30 perk fields 16 px and 44 px tall; page and tier-block sideways scroll 0.
- 1280 Editorial: perk fields 14 px / 40 px, label row and layout identical to the base (shots compared by eye).
- Other checkboxes on the Donations pages, measured at 375, not changed: Entitlements "Only active" (`EntitlementsSection.tsx:78`): label row 44 px, passes; Invoices "Needs attention" (`InvoicesSection.tsx:271`): label row 44 px, passes (both come from the kit's/Admin's filter-row rules); Entitlements grant dialog "No end date" (`:201`): box 13 px, label row 20 px, FAILS the hit test. Left of SEC-AUD-363 on the Donations pages: only that "No end date" checkbox (a dialog, `EntitlementsSection.tsx`, not mine). At 1280 all of these are 40 px label rows / 13 px boxes (desktop, not covered by D-856).

## Tests
- `admin/__tests__/donations.test.tsx`, single file: 12/12 passed with my change applied (run at 15:57); I did not run it on the base (a class name and CSS only).
- web tsc clean, eslint on PricingSection.tsx clean, prettier clean on the three files, `check-css-tokens` clean, `i18n-audit --max 0` TOTAL 0, e2e tsc clean, my spec passes (`problems []`).
- `check-doc-links` ok.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-donations`: 12 PNGs (before: top and tiers at 375 Light/Dark and 1280; after: the same 6), plus `measure*.json`, `totals*.json`. I opened all 12 after releasing the lock: the checkbox row is taller and the text sits level with the box; fields look the same size at 16 px; tiers blocks are whole; 1280 before and after identical.

## Commander's command (also in the spec header)
From `e2e/`, after seeding the sweep world on the same DB:
`SR_P86_DONATIONS=1 SR_E2E_BASE_URL=http://localhost:5645 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> npx playwright test tests/p86-donations-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (add `SR_P86_DONATIONS_BASE=1` for the base case).

## Proposed inventory row
SEC-AUD-376 | donations | On a phone "Donations are on" is a 44 px target and perk fields are 16 px (D-882). Fixed; left: the grant dialog's "No end date" checkbox (20 px row) at 375.

## Changelog sentence
"On a phone, Admin's Pricing and perks 'Donations are on' row is a 44 px tap target and the per-tier perk fields use 16 px text so iOS no longer zooms the page when one is focused."

## Owner questions
None new. Should the grant dialog's "No end date" row get the same 44 px treatment (EntitlementsSection, not in this wave's files)?

## Cleanup
Stack stopped by PID tree, ports 8645/5645/9345 free, `sr_p86_donations_ui` dropped, `p86-donations-bin` deleted, lock released. No stash touched.
