# Wave 91, measure: SEC-AUD-399 (D-911), the two unmeasured cases measured; nothing built

T = 10:48 (2026-11-17). Spec written and typechecked by 10:48; waited for the lock (members had it); lock taken 10:56; one stack round 10:56 to 10:58 (stack up, world seeded with no `FAIL` line, spec ran once, 55 s, passed); lock released 10:58. Cleanup done at about 11:00. No source file changed; no colour ratio asked for. Spec: `e2e/tests/p91-measure-shots.spec.ts` (new). Wave 90's never-run case 1a is case 1 here.

| case | verdict | proposed owner file |
|---|---|---|
| 1 equal-width chips in one column | **fault** against D-856 as written (the 21 px point below a button in an upper row returns the lower row's button: 6 of 9 buttons fail "down"); no button's centre is covered; a tap at a button's drawn box removes the right chip | `packages/ui/src/primitives/Select/Select.module.css` (chip row pitch, in the `max-width: 720px` block) |
| 2 second Settings switch stack | no fault in function (a different rule: `.us-stack` gap 14 px, not Sounds' 10 px); the 4 px difference is recorded | none (owner question below) |

Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-measure`, 3 PNGs plus `measure.json`; **all 3 opened by me** after releasing the lock (`data-theme` read back `lounge-light` each time; none of the pages is inside a room; sideways scroll 375/375).

## 1. Equal-width chips (Server settings > Community > Onboarding "Default channels"; `Select.module.css` `.chipRemove` and its `::after`, lines 197-209, D-879)
Channels `p91-r1` to `p91-r9` made through the API; every other chip removed and the nine chosen. 375 x 812, touch, Lounge Light: control 343 x 124, 3 rows of 3 chips (76 to 79 wide, 20 high), row pitch 24, gap between rows 4. Drawn button 16.6 x 16; target (the `::after`: absolute, top 8 px, left -24 px, 44.6 x 44, margin-top -22) 44.6 x 44, so it reaches 22 px above and below the button's centre.

**Columns:** rows 1 and 2 do not line up to 1 px (button x = 80.7 / 163.7 / 246.7 against 83.8 / 166.6 / 249.7: 3 px apart, the chips differ by up to 3 px in width); rows 2 and 3 do (83.8 against 82.8, 1.0 px). Either way the targets of adjacent rows overlap by 41.5 to 43.9 px horizontally and 20 px vertically (pitch 24, target 44).

Per button (all 9 centres return their own button):
- Row 1 (r1, r2, r3): up returns itself, centre itself, **down returns the row-2 button** (r4, r5, r6): fail.
- Row 2 (r4, r5, r6): up and centre themselves, **down returns the row-3 button** (r7, r8, r9): fail.
- Row 3 (r7, r8, r9): up, centre, down return themselves: pass. "Up" of a lower row returns itself for all six (and a 1 px walk shows each button owns 22 px above its centre).
- A 1 px walk down from an upper button's centre hands the point to the lower row's button after 1 px: the lower row's target is drawn later and wins every point of each of the 6 overlaps (820 to 880 points each).
- Is any button's **centre** covered by another chip's target? **No** (0 of 9: the next row's target reaches 22 px above its own centre, which is 24 px below). A touch tap at the drawn box of the middle button of the middle row (`Remove #p91-r5`, 174.9, 349.2) removed exactly `#p91-r5` (9 chips to 8). So a tap at a button's own box removes the right chip.
- Chevron 32 x 124: the three points return it.

**Verdict against D-856 as written: fault.** Every button in rows 1 and 2 fails "21 px below"; wave 90's prediction (pitch 24, target 44, later row wins) is confirmed. The cost is a missed or mis-routed tap 2 to 22 px under an upper button (it removes the lower chip), never a wrong removal at the centre.

**Proposed fix (wave 92, one builder):** `packages/ui/src/primitives/Select/Select.module.css`, in the `@media (max-width: 720px)` block (line 197): give the chip rows a pitch of at least 44 px on a phone (chip height 20 plus a 24 px row gap on the control's chip wrapper) so adjacent targets stop overlapping (it adds about 40 px of height for three rows); or shrink `.chipRemove::after` to the 24 px pitch and amend D-856 for chip buttons.
**Row to open:** "SEC-AUD-401: kit Select chips (multi-select) on a phone: with chips of equal width the remove buttons of adjacent rows share a column, the 44 px targets (pitch 24) overlap and the 21 px point below an upper row's button returns the lower row's button (6 of 9 at 375); centres are safe. Fix: row pitch 44, or an amended D-856 for chip buttons (`Select.module.css`)."

## 2. Second Settings switch stack (user Settings; `settings/user/user-settings.css`)
Read first: switch counts per section (Voice & Video 10, Notifications 8, Activity 7, Text & Images 7, Accessibility 5, Streamer 4, Privacy 4), then measured every user page (375, Lounge Light). Stacks of two or more switch rows:
- **Sounds (Voice & Video, the reference):** direct children of `.us-card`; 4 rows of 44, gap 10, rule `user-settings.css:581` (`margin-block-start: 10px`). Audio processing (2 rows), Appearance's Server themes (2) and Support's "Leaderboards and badge" (4 switch rows) use the same line 581 rule: gap 10.
- **Notifications "Unread indicators" (`#us-badges`, the card with the most switch rows after Sounds, 4 rows of 77.2, 77.2, 92.8, 97.2 high because they carry descriptions): gap 14 between each pair.** The rows sit in `div.us-stack` (`Notifications.tsx:220`), a flex column whose `gap: 14px` (`user-settings.css:55-59`) spaces every child; line 581 does not apply (the rows are not direct children of `.us-card`), the rows' own margin is 0. Notifications' "Sounds" stack (3 rows of 44) is the same: 14. Activity (`#us-activity`, 4 switches among 7 children), Status, Cinema defaults, Text & Images, Accessibility, Streamer Mode (4 rows) and Privacy "Who can reach you" use `.us-stack` too: 14.
- Shot: `2-switch-stack-375-lounge-light.png` (Notifications, Unread indicators): an even list, one gap between rows, nothing touching.

**Verdict: no fault in function.** The gap differs from Sounds' (14 against 10) but it is another layout: a `.us-stack` spaces all its children (switches, selects, hints) at 14, and wave 84's line 581 exists for switches that were direct children of a card with nothing between them (D-854). The 4 px difference is recorded; whether they should match is an owner question.

## Re-shoot command (commander, from `e2e/`, on a stack with the sweep world, API 8694, Vite 5694)
`SR_E2E_BASE_URL=http://localhost:5694 SR_P91_MEASURE_ON=1 SR_P91_MEASURE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-measure npx playwright test tests/p91-measure-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(`SR_P91_MEASURE_ONLY=1` or `=2` runs one case.) It needs the sweep world (see the spec header). It creates channels `p91-r1` to `p91-r9` in the world's guild and removes the guild's other default-channel chips in the form (never saved).

## Rows
- SEC-AUD-399: "The two cases wave 90 left unmeasured, measured (wave 91): equal-width chips: fault against D-856 (21 px below fails for rows 1 and 2, centres safe; opened as SEC-AUD-401); second switch stack: no fault in function (`.us-stack` 14 px against Sounds' 10 px, recorded)."
- New open row: SEC-AUD-401 (text above).

No changelog sentence: nothing built.
Owner questions (my default): (1) chip rows on a phone: pitch 44 (taller control) or D-856 allowing 24 for chip buttons? Default: pitch 44. (2) Should the `.us-stack` switch gap match Sounds' 10 px? Default: no, keep 14.
Cleanup: stack stopped by PID tree (8694 / 5694 / 9394 free), `sr_p91_measure_ui` dropped, `p91-measure-bin` and the scratch folder deleted (`p80-base-bin` untouched), lock released. No stash touched.
