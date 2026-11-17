# Wave 92 lounge: SEC-AUD-402 (D-915) and SEC-AUD-403 (D-916)

Builder: lounge. T = 11:47 on 2026-11-17; one stack round 11:50 to 11:54 (lock `lounge`, released 11:54); handoff written about 11:58. Web only, one CSS file and one spec: no cargo, no web build, no vitest.

## Done
- `apps/web/src/lounge/profile/profile-p11.css` (one commit with the spec): three additions at the end of the sheet.
  - `.lng-profile > section { margin-block-start: 12px }`, every width (D-916). The base showed 0 px between the action row and the label at 375 and at 1280, so it is not phone only. Only the group panel's invite block is a direct `section` child of `.lng-profile` (the one-to-one card's sections sit inside `.lng-profile-card`; `ProfilePopout.tsx` has no `.lng-profile`).
  - Phone block (`max-width: 768px`): `.prf-dm-panel .prf-dm-actions .lng-icon-btn { inline-size: 44px; block-size: 44px }` ("More actions" has `lng-icon-btn` inside `.prf-dm-actions`, so it IS reachable from this sheet: built, not listed) and `.prf-dm-panel .lng-profile-card > .lng-link-btn { display: inline-flex; align-items: center; min-block-size: 44px }` (the mutual link and "View Full Profile"). Real boxes, not `::after`: the card is a column with a 12 px gap, so no target reaches into another.
  - Phone block: `.prf-invite .lng-pop-input { min-block-size: 44px }`. Found by the check, not in the task: the invite field (33 px tall) fails the 44 px hit test once "Create invite link" has made it (on the base too). Scoped under `.prf-invite`.
  - Comments name SEC-AUD-402/D-915 and SEC-AUD-403/D-916.
- `e2e/tests/p92-lounge-shots.spec.ts` (new): tests `card` and `group`; header has the command; variables `SR_P92_LOUNGE_ON`, `_BASE`, `_OUT`. The hit test follows the hit-test rule (scroll to the middle, box and centre checked, a null throws `probe error`).
- **The CSS in the last commit is the CSS I measured**: the last run (11:54) was after the invite-field rule was added; nothing changed after it.

## Base (ran it: `profile-p11.css` restored from HEAD, same DB, same conversations)
Lounge Light 375 touch, one-to-one DM with a friend (0 shared servers and 0 mutual friends: the mutual link is drawn whenever the profile loaded). Card at (8, 137), 359 x 235.
- "More actions" 28 x 28, "0 Mutual Servers • 0 Mutual Friends" 273.5 x 20, "View Full Profile" 120.7 x 20. All three fail both hit points (above and below); the taker is `div.lng-profile-card` each time. Gaps: More to the next control 63, mutual to View Full 12.
- Other controls of the card: none (the friend action is an icon/state, not a button here; the card has no note field). Lounge Dark: same figures.
- 1280 Editorial card 224 x 255: More 28 x 28, the mutual link is 200 x 40 (stretched in the flex column), View Full 120.7 x 20; all fail the same way (not a phone case; not changed).
- Group panel: action row bottom to the "INVITE LINK" heading box 0 px (text top = box top = 257 at 375; 0 at 1280); gap above the action row 12 at 375 (0 at 1280); section margin `0 8px` at 375, 0 at 1280; heading margin `0 0 4px`. Every control of the group panel passes at 375 except the invite field after "Create invite link" (box 335 x 33, fails above and below).
- Popout on a phone (measured only; the group panel's member name opens it): Close 44 passes; Note field 308 x 33 and Message field 308 x 33 fail; "View Full Profile" 120.7 x 20 fails. It has no "More actions". Not changed (`profile-p11.css`'s popout rules are not under `.prf-dm-panel`, and the brief says measure only). Proposed row below.

## After (final CSS, 11:54)
- Item 1, 375 Lounge Light and Lounge Dark: More 44 x 44, mutual 273.5 x 44, View Full 120.7 x 44: all three pass (above and below). Gaps unchanged: 63 and 12 (no target reaches into another; their boxes are the targets). Card 359 x 299: **+64 px taller than the base** (the actions row +16, each link +24). No sideways scroll (0), nothing cut off (0). The menu opens from "More actions" (7 rows), "Copy Username" tapped closes it; "View Full Profile" and the mutual link each open the full profile (a dialog), Escape closes it.
- 1280 Editorial: every child box of the card and every control box equal to the base (0 px; asserted by the spec).
- Item 2, 375 Light and Dark: action row bottom to the label's box and to its text is 12 px (section margin `12px 8px 0`); gap above the action row 12; every control of the panel passes; "Create invite link" bottom 316, panel bottom 328 (not clipped; the base was 304 / 316); no sideways scroll. After "Create invite link": the field (x 20 to 355, now 335 x 44) and Copy (starts at x 20) have the gutter and all pass, the field included. Panel 299 tall (base 287): +12.
- 1280 Editorial: gap 12; the members box and the action row equal to the base; the section and the panel's bottom moved by exactly 12 (section top 263 to 275, panel height 257 to 269).

## Checks
Spec `--project=chromium --reporter=line`: 2 passed (after case, which compares with `base-measure.json`); the base case was run first with the change switched off (assertions off, figures above). Watched fail without the change: the card's hit tests and the 12 px gap read false/0 on the base run (figures), and the first after run failed on the invite field (which led to the third rule) and then on my own spec's too-strict panel-height compare (spec fixed, rerun passed). e2e `tsc` clean, prettier clean, `node scripts/check-css-tokens.mjs` clean.

## Who imports the file
`git grep -n profile-p11`: only `lounge/profile/ProfilePopout.tsx:20`, which `LoungeShell.tsx` imports (the Lounge chunk, lazy). No first-paint or public file touched, no import added.

## Shots (`visual-sweep-shots/p92-lounge`: the nine listed, all opened by me; 20 PNGs in the folder in all)
The nine (opened): `base-card-phone-lounge-light`, `card-phone-lounge-light`, `card-phone-lounge-dark`, `card-phone-lounge-light-menu`, `popout-phone-lounge-light`, `card-desktop-lounge-editorial`, `base-group-phone-lounge-light`, `group-phone-lounge-light`, `group-desktop-lounge-editorial`. Extra (not opened; figures in `measure.json`): `base-card-phone-lounge-dark`, `base-card-phone-lounge-light-menu`, `base-card-desktop-lounge-editorial`, `base-group-phone-lounge-dark`, `group-phone-lounge-dark`, `base-group-desktop-lounge-editorial`, `base-group-phone-lounge-light-invite`, `group-phone-lounge-light-invite`, `base-popout-phone-lounge-light`. What I saw: the card is stacked with room, the "..." button is a 44 px box with the menu under it, links have air between them, nothing cut off; the label "INVITE LINK" now sits 12 px under the two buttons, at 375 and at 1280.

## Commander's command (from `e2e/`, after the sweep world is seeded)
`SR_E2E_BASE_URL=http://localhost:5703 SR_P92_LOUNGE_ON=1 SR_P92_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-lounge npx playwright test tests/p92-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`-g card` or `-g group`; also set `SR_CLI_BIN`, `SR_DATABASE__URL`, `DATABASE_URL`). Base case: add `SR_P92_LOUNGE_BASE=1` with `profile-p11.css` restored from the base. `ids.json` in the shots folder keeps the made conversations between runs: delete it on a fresh database.

## Docs
Searched `docs/` for these classes and rules: no doc states the card's or panel's sizes. Nothing to update.

## Proposed inventory rows
- SEC-AUD-402: One-to-one profile card on a phone: "More actions" 28 px and the mutual and "View Full Profile" links 20 px failed the 44 px hit test. Fixed (D-915) in `profile-p11.css` (card +64 px tall).
- SEC-AUD-403: Group DM panel: the "INVITE LINK" label had 0 px above it (the base measure; the commander's shot read about 2 px). Fixed (D-916), `margin-block-start: 12px` at every width. Also the invite field (33 px) now passes the hit test on a phone.
- New fault (number from SEC-AUD-408): the profile popout's Note field and Message field (33 px tall) and "View Full Profile" (20 px) fail the 44 px hit test on a phone; "Close" passes. Fix: rules in `profile-p11.css` under `.lng-pop` in a phone block (the popout is a `.lng-pop` portal, not `.prf-dm-panel`). Not built (measure only).
- Also the 1280 one-to-one card's mutual link is stretched to 200 x 40 while "View Full Profile" is 120.7 x 20: odd but untouched.

## Changelog sentences
- On a phone the one-to-one profile card's "More actions" button, mutual servers and friends link and "View Full Profile" link now have 44 px touch targets.
- The group DM panel's "INVITE LINK" label has 12 px of room above it, and its invite field is a 44 px touch target on a phone.

## Owner questions
None.

## Cleanup
Stack stopped by PID tree (ports 8703, 5703, 9403 free), lock released at 11:54. Scratch database `sr_p92_lounge_ui` dropped and `p92-lounge-bin` deleted (`p80-base-bin` untouched). No stash touched.
