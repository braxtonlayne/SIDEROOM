# Wave 87 donations builder: SEC-AUD-378 (D-888)

T = 16:40 on 2026-11-16 (clock times: no new item 17:10, checkpoint 17:22, finished 17:30, hard limit 17:40). I started early with the commander's go (16:26) and held the lock 16:28 to 16:31 (one round: stack, seed, base case, change, after run). Finished about 16:33.

## Done
- `EntitlementsSection.tsx`: the "No end date" `<label>` gets `className="don-check-row"` (nothing else in the `.tsx`).
- `donations.css`: not changed. Wave 86's phone rule reaches the label inside the kit Modal (the sheet is global CSS), and no second rule was needed: the dialog's own label rule does not win, the `{' '}` gap adds nothing visible (text sits level with the box).
- Imports: `admin/donations/EntitlementsSection.tsx` is imported only by `admin/AdminApp.tsx` (lazy Admin chunk); `donations.css` is imported only from inside `admin/donations/`. No web build run.

## Base (ran it: spec base case `SR_P87_DONATIONS_BASE=1`, tree unchanged)
375x812 touch, Lounge Light and Lounge Dark, Entitlements, grant dialog, kind "patron": checkbox box 13 x 13, label row 303 x 20, hit test FAILS. Fault confirmed. 1280 Editorial: box 13, label row 20 (desktop, not covered by D-856).

## After (same spec)
- 375 Lounge Light and Dark: label row 303 x 44 (class `don-check-row`), hit test passes, box still 13 x 13. A touch tap on the label text toggles it on, the "Days" field goes; a second tap restores it and the field returns (nothing granted, dialog closed with Escape).
- What moved at 375 (the dialog is centred): the row grew 20 -> 44 (+24); the dialog grew 480.2 -> 504.2 px (+24) and its top moved up 12 px, so everything above the row is 12 px higher and everything below is 12 px lower than before (net 0 relative to centre); every other control's height unchanged. Dialog at 375: y 153.9, h 504.2 in a 812 viewport: fits, its scroll height equals its client height (no scroll needed). Page sideways scroll 0.
- 1280 Editorial: all 14 measured controls of the dialog have identical boxes base vs after (0 px moved); the label row stays 20 px.
- Pricing "Donations are on" (wave 86), measured once per setting: 309 x 44 row at 375, hit test passes, 13 px box; unchanged. 1280: 794 x 20, unchanged.

## What is left of SEC-AUD-363 on the Donations pages
Nothing from the Donations pages' checkboxes at 375 (Pricing "Donations are on", Entitlements "Only active", Invoices "Needs attention" passed in wave 86; "No end date" now passes). The Entitlements and Invoices tables when they have rows were not looked at (no rows in the seeded world) and stay open as before.

## Tests
- `admin/__tests__/donations.test.tsx` single file: 12/12 pass with the change. Not run on the base (a class name only).
- web tsc clean; eslint (package's own) on EntitlementsSection.tsx clean; prettier clean on it and the spec; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok; e2e tsc clean; `check-css-tokens` not run (sheet unchanged).
- My spec `e2e/tests/p87-donations-shots.spec.ts`: passes with the change (`problems []`, 1 passed); on the base case it recorded the failing figures. (An earlier run with the change not yet applied failed on exactly the hit-test and 20 px row checks, as it should.)

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-donations`: 10 PNGs (before: 375 Light/Dark unchecked and checked, 1280 Editorial; after: the same 5), plus `measure*.json`, `totals*.json` (`measure.json` also holds the lines of the first, pre-change runs; the last line per setting is the real one). All 10 opened after releasing the lock: the row is taller, text level with the box, the Days field appears and goes, the dialog is whole in Light, Dark and at 1280, 1280 before and after identical.

## Commander's command (also in the spec header)
From `e2e/`, after seeding the sweep world on the same DB:
`SR_P87_DONATIONS_ON=1 SR_E2E_BASE_URL=http://localhost:5653 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> npx playwright test tests/p87-donations-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (add `SR_P87_DONATIONS_BASE=1` for the base case).

## Proposed inventory row
SEC-AUD-378 | donations | On a phone "No end date" in the Entitlements grant dialog is a 44 px row and its label is the target (D-888). Fixed: 13 px box / 20 px row before, 44 px row after; hit test passes. SEC-AUD-363 loses it.

## Changelog sentence
"On a phone, Admin's Entitlements grant dialog's 'No end date' option is a 44 px tap target."

## Owner questions
None. (The kit Checkbox later: D-890.)

## Cleanup
Stack stopped by PID tree, ports 8653/5653/9353 free, `sr_p87_donations_ui` dropped, `p87-donations-bin` deleted, lock released. No stash touched. Scratch folder `scratch-donations87` is outside the repo.
