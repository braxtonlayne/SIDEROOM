# Wave 103, members: SEC-AUD-457 (D-977), SEC-AUD-458 (D-975)

Branch `fix/p103-members`, commits `a2d7771b` (SEC-AUD-457) and `42c6681f` (SEC-AUD-458, with the spec), 11:56 to 12:17. No Rust, no cargo, no web build, no new string, no migration, no `packages/ui` file.

## Verdicts
- **SEC-AUD-457: real, and the task's cure alone cannot work.** Base at 1024: table 663.6 px in a 585 px wrapper (scrollWidth 664, clientWidth 585); the Actions head starts at x 880.3 and ends at 990 against the wrapper's right edge 911.7 (cut). Measured cause: every column was already at its minimum width, so wrapping the role chips gains nothing (a chip is as wide as its text). Roles 147 = the "Channel Keepers" chip 131 + padding; Signals 109 = the "New account" chip + padding; Actions 110 on one line; Name 110 = avatar + name; Since 72 = the head word "MEMBER"; Joined 83 = the code `ia5TYS7m`; checkbox 34. The sum, 664, is the table's minimum. I tried letting chip text wrap (the only thing that makes the world fit with no padding change): it breaks D-977's "each chip stays whole" and with the large case rows became 653 px tall against 128. Rejected. What ships stays closer to D-977 (role chips whole, Actions on one line, no column dropped).
- **SEC-AUD-458: real.** At 375 the head's box is 44 px with no text; the hit test passes in base and after.

## The diff in words
- `Members.tsx`: the table gets `ss-members-table`; the Signals chips' span gets `ss-signal-chips`; the head `Checkbox` has `label={t('settings.members.selectAll')}` instead of `label={null}` plus `aria-label` (one accessible name; no new string).
- `server-settings.css`, blocks at the very end, no rule above edited:
  - 457: above 720 px `.ss-table-scroll { container-type: inline-size }` and, while that wrapper is narrower than 700 px (`@container`), for `.ss-members-table`: cell padding-inline 4 px (was 8), chip padding-inline 6 px (was 8), and the text of a Signals chip may wrap (`white-space: normal`; radius 0.75rem keeps a one-line chip a pill). At 1280 (wrapper 828) nothing applies.
  - 458: above 720 px the head's `label` is clipped to 1 px (the `.ss-sr` pattern). At 720 px and under the label reads as plain text (no capitals, spacing or bold from the th) and the head's box keeps the row boxes' x (13 px of inline-start padding, as a lone kit checkbox has).

## Figures (`visual-sweep-shots/p103-members/figures-base.json` and `figures-after.json`; world: 2 members; large: 30 members, 32-character names, five roles each, 25 rows on page 1)
| Run | base scroll / client | after scroll / client | Actions head | rows (first two) base to after |
|---|---|---|---|---|
| 1024 Light | 664 / 585 | **585 / 585** | x 880 to 990 (cut) becomes x 810 to 912 (whole; buttons end 908, edge 911.7) | 59, 67.5 to 59, 50.5 |
| 1024 Dark | 664 / 585 | **585 / 585** | same as Light | same |
| 900 Light | 664 / 496 | 571 / 496 (still scrolls: the safety net) | cut, cut | 59, 67.5 to 59, 67.5 |
| 1280 Light | 828 / 828 | 828 / 828 | equal | equal (59, 50.5) |
| 1024 Light, large | 860 / 585 | 767 / 585 (scrolls) | | 128 to 128 (no row grows) |
| 900 Light, large | 860 / 496 | 767 / 496 (scrolls) | | 128 to 128 |
| 375 Light and Dark | 343 / 343 | 343 / 343 | stacked | table box, row heights (202, 180) and first row y (597) equal to base |

Columns at 1024 with the world's data, base to after: checkbox 34 to 26, Name 110 to 104, Since 72 to 66, Joined 83 to 78, Signals 109 to 69, Roles 147 to 140, Actions 110 to 102. A row grows only by the lines a chip needs: none grows (the "New account" chip takes two lines inside the 59 px row); row 2 shrinks 17 px because "Created the server" gets a wider column.

458 at 375 touch (Light, Dark): head line 44 px; box x 29, y 566, 18x18 (equal to the row boxes' x); label text "Select all members on this page", box 211 x 44 at x 57, whole on one line; contrast 12.64 (Light) and 9.36 (Dark) from `contrast.ts`; a `tap()` on the label text ticks 2 of 2 rows and a second tap clears them; the head box's own hit test (`hitTest` on `main thead input`) gives centre, above and below `control`, pass; the row boxes pass; first row y 597 = base (moved 0). `getByRole('checkbox', {name: 'Select all members on this page'})` finds exactly one box at 375 and at 1024 (asserted in every run). At 1024: label box 1x1 with clip-path `inset(50%)` (no text shows); the head checkbox box 18x18 equals the base (its cell is 8 px narrower, from the padding rule).

## Who imports `server-settings.css`
`apps/web/src/settings/server/ui.tsx` (the Server settings chunk, route lazy) and `apps/web/src/admin/ui.tsx` (the Admin chunk, `StaffDoor.tsx` lazy). `Members.tsx` is in the Server settings chunk. Neither is first paint nor a public page. `.ss-table-scroll` is used only by Members (git grep). No web build.

## STACK-ROUND
One round, slot 2: `heavy.sh acquire members` 12:00 (status then `modal 11:58`, `members 12:00`); stack API 8801 / Vite 5801 / metrics 9501 with `SR_ADMIN_REQUIRE_REAUTH=0`; fresh database `sr_p103_members_ui`; `rm -rf e2e/.state`; sweep seed passed (no `FAIL`). In the same round: base runs (the first with the unedited files; the last at 12:11 with `Members.tsx` and the sheet put back from `git show HEAD:` and restored from scratch copies, no stash), after runs, and `p102-settings-shots` after. Stack stopped by port, ports 8801 / 5801 / 9501 free, released 12:14. Last after run: `1 passed (21.9s)`, `problems: []`. The spec went through three versions in the round: its first head-box assertion was vacuous because `hit-test.ts` names a box by its aria-label only, so it now calls `hitTest` on the head box itself.

## TYPECHECK
`bash cli-agents/tc.sh run members bash -c "cd C:/Users/braxt/Desktop/SIDEROOM_wt/p103-members/apps/web && npx tsc --noEmit -p ."`, 12:15 to 12:16, exit 0 (no `.tsx` edited after). e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 after the last spec edit.

## Other checks (after the last edit)
prettier --check on the three files: clean; eslint on `Members.tsx`: clean; `node scripts/check-css-tokens.mjs`: clean; `node scripts/i18n-audit.mjs --max 0`: TOTAL 0; `node scripts/check-doc-links.mjs`: ok. Unit tests, unedited: `p14communityMore.test.tsx` 25 passed; `people.test.tsx` 16 passed and 1 failed (stale expectation, below).

## STALE (expectations that name what D-975 changed on purpose; not in my row, not edited)
- `apps/web/src/settings/server/__tests__/people.test.tsx:179` expects the select-all box to have no `<label>`, and `:181` expects an `aria-label` attribute on it. Replacement for both lines: `expect(all.closest('div')?.querySelector('label')).toHaveTextContent('Select all members on this page');` (the box is still found by name on the line above; line 178, the row box has no label, holds). **The commander must edit this test or CI fails.**
- `e2e/tests/p102-settings-shots.spec.ts:390-391` ("Select all" did not pass): `hit-test.ts` names a checkbox by `aria-label`, `aria-labelledby` or text content, not by its `<label>`, so the now-labelled head box is named "" and the spec finds no row named "Select all". That was the only problem line of `p102-settings-shots` run after mine (no base); nothing about the table's scroll at 1024 (that spec excludes the wrapper). My own spec shows the box passes. Not a page fault.

## Shots (`visual-sweep-shots/p103-members/`, 16 PNGs; I opened 10)
Opened and read: `base-members-1024-lounge-light` (table cut at "ACT", Actions buttons cut), `members-1024-lounge-light` (all seven columns whole, "Roles" and "..." whole, two-line "New account" chips, role chips whole), `members-1024-lounge-dark` (same on the dark ground), `members-900-lounge-light` (still scrolls, Actions cut at the right edge: the safety net), `members-1280-lounge-light` (unchanged, whole table), `members-1024-large-lounge-light` (scrolls, rows as in base, five role chips per row), `base-members-375-lounge-light` (unlabelled box above the first row), `members-375-lounge-light` (box on the rows' x, plain-text label beside it, first row in place), `members-375-lounge-dark` (same, dark). An earlier run showed the label in the head's capitals and bold: that led to the plain-text rule. On disk, not opened: `base-members-1024-lounge-dark`, `base-members-1280-lounge-light`, `base-members-375-lounge-dark`, `base-members-900-*`, `base-members-1024-large-*`, `members-900-large-lounge-light`, and the `p102-settings/` folder (that spec asserts its own).

## The commander's command
Seed the sweep world on a stack (API 8801, Vite 5801, `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL`, `SR_CLI_BIN` set), then from `e2e/`:
```
SR_E2E_BASE_URL=http://localhost:5801 SR_P103_MEMBERS_ON=1 \
SR_P103_MEMBERS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-members \
npx playwright test tests/p103-members-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
Delete `figures-base.json` in the out folder first on the merged tree (there is no base then). The spec makes five roles `p103-mem-role-*` through the API and removes them in a `finally`; the large case is faked in the browser.

## Closing texts
- SEC-AUD-457: closed, with the deviation in the owner questions. At 1024 the Members table (664 px minimum) was wider than its 585 px column, so its own scroll cut the Actions column. While the table's box is narrower than 700 px its cells and chips use less side padding and a Signals chip's text may wrap; at 1024 (Light and Dark) the table's `scrollWidth` equals its `clientWidth` and the Actions head and buttons are whole; at 1280 and 375 the boxes are the base's. At 900 and with 30 long-named members the table still scrolls inside its own box.
- SEC-AUD-458: closed. The Members "select all" box shows "Select all members on this page" beside it on a phone (the existing string; 12.6:1 Light, 9.4:1 Dark), a tap on the text ticks every row, the line passes the hit test and the first row did not move; above a phone the box stays bare with the same name.

## Proposed rows
- (low) At 900 (496 px column) the Members table (571 px minimum after this change) still scrolls inside its box. Fitting it would need the role chip text to wrap or a column to merge, which D-977 rules out. Not done.
- (low, probe) `e2e/support/hit-test.ts` names a checkbox by `aria-label` only, so a box with a `<label>` has the name "" in a row; it could name it from `input.labels`.

## Changelog sentence
Server settings: the Members table shows its Actions column without scrolling at tablet width, and the phone's "select all" box shows its label.

## Owner questions
- D-977 asked for the role chips to wrap whole and, if that was not enough, the Joined via and Signals text to wrap. Measured: every column was at its minimum, so that cannot fit 585 px. I let a Signals chip's text wrap and took 4 px (cells) and 2 px (chips) of side padding while the table's box is under 700 px; role chips stay whole. Alternative: let role chip text wrap too (it fits with no padding change, but rows grew by 30 px, and 5 times with long role names). Default: as shipped.
- Should the 700 px threshold follow the table's real need instead? Default: no.

## Clean-up and lock
Scratch database `sr_p103_members_ui` dropped; `p103-members-bin` deleted; stack stopped (ports 8801, 5801, 9501 free). No `git stash` used or touched. `heavy.sh status` at the end: no hold in my name (released 12:14); no `acquire` of mine is alive. The temporary probe spec was deleted and never committed.
