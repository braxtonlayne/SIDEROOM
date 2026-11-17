# Wave 90, measure: SEC-AUD-392 (D-902), the re-measure list measured; nothing built

T = 05:50 (2026-11-17). Spec written 05:55; lock taken 06:32 (after a long wait: other builders took it in turn); one stack round 06:32 to about 06:35 (stack up, world seeded, the spec ran once, 1.8 min, passed); my session then stalled and was interrupted; cleanup was done at about 10:25, well past the hard limit. **The second pass I had patched in (item 5's other stacks, and an "aligned chips" case) was never run: those two are "not measured".** No source file changed. No colour ratio was taken (none asked). `FAIL suspended app` appeared in the seed as known (the seed builder's item).

| item | verdict | proposed owner file |
|---|---|---|
| 1 wrapped chip rows | no fault in the case measured (chips on separate rows, no area overlap); the aligned case (buttons stacked in the same column) **not measured** | `packages/ui/src/primitives/Select/Select.module.css` if a fault shows |
| 2 `Select` trigger height | no fault in height (0 px difference); **possible fault, unexplained**: several Profile selects fail the 375 hit test | `settings/user/sections/Profile.tsx` or `user-settings.css` (to be found) |
| 3 template page | no fault | none |
| 4 Private content segmented | no fault | none |
| 5 Sounds stack gap | no fault in the Sounds stack; "one other section's stack" **not measured** | none |

Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-measure`, 7 PNGs plus `measure.json`; **all 7 opened by me** after the lock was released (`data-theme` read back: `lounge-light` for the 375 shots, `lounge-editorial` for the 1280 ones; none of the five pages is inside a room).

## 1. Wrapped chips (Server settings > Community > Onboarding "Default channels"; `Select.module.css` `.chipRemove` and `::after`, D-879)
Extra channels were made through the API (they stay in the dropped scratch DB). 375, Lounge Light: 7 chips on 7 rows (each long chip takes its own row; two are two lines), control 343 x 228; row gaps 4, 4, 4, 4, 4, 19 px. Every remove button: drawn 16.6 x 16, hit area 44.6 x 44 (read from the `::after` rule), and the centre and the points 21 px above and below each return the button (7 x 3 pass). Areas of different rows overlap vertically (pitch 24 against 44) but never horizontally (the buttons sit at different x), so no overlap region exists; no button's centre is inside another's area; a scan of each area (2 px steps) gave every point to its own button. Chevron box 32 x 228, hit passes. Sideways scroll 0.
1280, Editorial: 3 rows, gaps 4 and 8; the media query does not apply, so the buttons are the 16.6 x 16 drawn box (no touch target is asked on a desktop).
**Not measured:** chips whose buttons sit in the same column on adjacent rows (equal-width chips). By the rule's numbers the upper button's +21 px point would lie in the lower row's area (pitch 24, area 44 tall, later row wins) and "down" would fail for it, while its centre stays its own. That needs the equal-width run (channels `p90-r1` to `p90-r9`, case `1a` in the spec). Verdict: **no fault in the case measured; open question**.

## 2. `Select` trigger against the kit text field
375: the Select's control is 44 high, its trigger 44 (275 wide), the kit text field 44: difference 0 on Support ("Credits" against "Other amount"), Profile ("Profile effect" above "Custom status", same card, 79 px apart), Notifications, and Activity (four Selects, no text field on the page). 1280: control 40 with the trigger 38 inside it, field 40: difference 0. At 375 the first select of each page, and all of Notifications' and Activity's, pass the hit test up and down; the fields pass too. **Profile's later selects ("Your page", "Links", "Bio", "Pictures", "Top friends", "Now playing", "Guestbook", "Status rings") fail up and down at 375**; I did not find why (a sticky bar or an overlay is a guess, not measured). Verdict: **heights no fault; Profile's page-block selects: possible fault, needs a look**. The 1280 hit results (all false) are the probe's 21 px points leaving a 40 px box and mean nothing.

## 3. Template page (`/template/<code>`, `AppServerTemplate.tsx`, `components/entrance.css` `.ent-invite-card`)
375: card 327 x 695; form 263 wide (the card's content width); field 263 x 44, button 263 x 36, stacked (not one row), 12 px between; both inside the card; the hit test passes for both (the button's 36 px drawn box has a 44 px target); no truncated text; sideways scroll 0. Verdict: **no fault** (wave 84's two faults are gone).

## 4. Private content (Admin; `admin/sections/PrivateContent.tsx`, kit `SegmentedControl`, `Slider.module.css` `.segmented`)
375: control 343 x 82, **2 rows**: "Channel" (82.5 x 32) and "Account DMs" (117.3 x 32) on the first, "Server channels" (135.2 x 32) on the second; row gap 12 px; every label on one line; every option passes up and down. The selected option has the accent ground (rgb(88, 101, 242)) and stays clear on the first row (shot). Verdict: **no fault** (the wrap is the designed wrap, SEC-AUD-353).

## 5. Sounds stack (Settings > Voice & Video; `settings/user/user-settings.css`)
Sounds card: four switch rows, each 44 high, 10 px between them (rule `.us-card > div:has(> input[role='switch']) + div:has(> input[role='switch'])`, line 581); last switch to the first slider 10 px (line 605); the two sliders 0 px apart and the slider to "Play a sample" 0 px (boxes; sliders are 43 high, the 44 px target comes from a negative margin; no rule sets a gap). The neighbouring card on the same page, Audio processing, has two switches 10 px apart (same line 581 rule). **Not measured:** another section's stack (the first probe found no card with two switches directly in `.us-card` on Notifications, Streamer Mode or Accessibility; the widened probe was never run). Verdict: **no fault** for the Sounds switch gap.

## Re-shoot command (commander, from `e2e/`, on a stack with the sweep world, API 8687, Vite 5687)
`SR_E2E_BASE_URL=http://localhost:5687 SR_P90_MEASURE_ON=1 SR_P90_MEASURE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-measure npx playwright test tests/p90-measure-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(`SR_P90_MEASURE_ONLY=1a,5` runs just the two unmeasured cases.) The committed spec contains those two patches; **they have not been run**.

## Rows
- SEC-AUD-392: "The five re-measure items measured (wave 90): no fault in items 3 and 4, in 5's Sounds gap, in 2's heights, or in 1's measured case. Open: chip rows with aligned buttons; Profile's page-block selects at 375; another Settings switch stack."
- Proposed new open row: "Profile (Settings): the page-block selects (Your page to Status rings) fail the 375 hit test; cause not found."

No changelog sentence: nothing built.
Owner questions: none.
Cleanup: stack stopped by PID tree (8687 / 5687 / 9387 free), `sr_p90_measure_ui` dropped, `p90-measure-bin` deleted (`p80-base-bin` untouched). I did not touch any stash.
