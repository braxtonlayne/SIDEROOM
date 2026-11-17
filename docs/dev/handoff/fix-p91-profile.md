# Wave 91, profile builder: SEC-AUD-393 (D-910), Profile's page-block selects at 375

T = 10:48 on 2026-11-17. Lock rounds: 11:00-11:02 (probe, then wave 90's own item 2 re-run on the same stack), 11:11-11:12 (closed-state confirmation). Handoff written 11:13.

## Cause (one sentence)
**There is no fault in the selects: wave 90's probe never opened the `<details className="us-advanced">` card, so it hit-tested the closed card's hidden content, which cannot be scrolled to and where `document.elementFromPoint` returns `null` (the "node that takes the point" is none: the point lies outside the viewport).** Case (d): nothing built.

## What I saw on the base (kind (d))
- Settings > Profile, 375 x 812, touch, Lounge Light and Lounge Dark, "Advanced" open, each select scrolled to the viewport middle: **all nine kit selects of the page pass the D-856 hit test above and below** (trigger and control): trigger 275 x 44, control 309 x 44 (the control is 44 high here as on the other pages), `elementsFromPoint` top node is the trigger itself at -21, 0 and +21 in all 9 x 2 themes. Fields: label 15 high, hint 2-3 lines, gap from one control to the next field 6 px (10 px after "Your page"), so no neighbour is near the point. Sticky/fixed nodes on the page: the settings overlay root and an empty toast region only; neither covers anything. No sideways scroll (375/375). Card 343 x 1774 px, "Advanced" 309 x 924 px.
- 1280 Editorial: control 40, trigger 38 (kit desktop size); the 21 px points leave a 40 px box, so "false" there means nothing (as wave 90 said).
- **Reproduction of wave 90's failure:** I ran `p90-measure-shots.spec.ts` item 2 unchanged (`SR_P90_MEASURE_ONLY=2`) on my stack: "Profile effect" (outside the card) passes; the eight selects inside "Advanced" fail with `got: null`, boxes at y 2259..2925 (off screen) after `scrollIntoView`. That spec never opens the details (`grep details|summary|Advanced` finds nothing).
- **Confirmed with the card closed (my spec's `closedProbe`)**: all eight selects keep their box after `scrollIntoView` (y 2093..2759, `inViewport: false`) and `elementFromPoint` at +21 returns `null`; the first select, outside the card, scrolls to y 414.6 and returns its trigger. A closed `<details>` hides its content, and a user cannot touch it either, so the real page has nothing to fix.
- Choose and put back (375 Lounge Light, the 3rd select "Links"): "Anyone on the web" -> "Members of this server" (shown, saved: still so after reload) -> put back ("Anyone on the web" after reload).

## Built
Nothing in `PrivacyPanel.tsx`, `user-settings.css`, nor the test. Only the new spec `e2e/tests/p91-profile-shots.spec.ts` (probe, local only, `SR_P91_PROFILE_ON=1`). No i18n, no markup, no source change, so no `PrivacyPanel.test.tsx` case was added; I did not run it (no change).

## Who imports my files
`PrivacyPanel.tsx`: only `settings/user/sections/Profile.tsx` (Settings chunk). `user-settings.css`: `settings/user/parts.tsx`, `support/SupportPage.tsx`, `support/Leaderboards.tsx` (both lazy in `support/routes.tsx`). Nothing first-paint or public. No web build run.

## Checks run
e2e `tsc`, prettier on the spec (clean), `node scripts/i18n-audit.mjs --max 0` (TOTAL 0), `node scripts/check-css-tokens.mjs` (clean). Spec run twice, passed both times (`--project=chromium`, 22 s, 19 s). The base case is the only case: nothing changed, so "after" = base.

## Shots (`visual-sweep-shots/p91-profile`, 5 PNGs, all 5 opened by me after the release)
`base-advanced-375-lounge-light.png`, `base-advanced-375-lounge-dark.png`, `base-open-375-lounge-light.png` (a select open), `base-advanced-1280-editorial.png`, `base-closed-outlined-375-lounge-light.png` (the closed card outlined in red: the node wave 90's probe was "inside"). Also `base-probe.json` (full probe: boxes, stacks, closed-state results) and `p90rerun/` (wave 90's spec re-run output, one PNG and `measure.json`). `data-theme` read back from the account (`lounge-light`, `lounge-dark`, `lounge-editorial`).

Incidental (not mine, not a hit-test item): at 375 the card's preset `SegmentedControl` ("Public / Members only / Friends only / Private") wraps to two rows with the selected option not visibly marked in the shot (preset is "custom": none selected, so probably expected). Not measured.

## Re-shoot command (from `e2e/`, stack with the sweep world on API 8693 / Vite 5693)
`SR_E2E_BASE_URL=http://localhost:5693 SR_P91_PROFILE_ON=1 SR_P91_PROFILE_BASE=1 SR_P91_PROFILE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-profile npx playwright test tests/p91-profile-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (drop `SR_P91_PROFILE_BASE=1` to name the files `after-`).

## Proposed inventory text
Close SEC-AUD-393: "Profile's page-block selects (Your page to Status rings) pass the 375 hit test (wave 91, D-910): the wave 90 failure was the probe's, which never opened the 'Advanced' card; a closed `<details>` hides its content and `elementFromPoint` returns null. Nothing to fix. Lesson: a probe opens collapsed cards (and `<details>`) before it hit-tests."
Close the wave 90 "Profile: page-block selects fail the 375 hit test; cause not found" row the same way. No threat-model row.

## Changelog sentence
Profile's audience selects were measured on a phone and pass the 44 px touch test; wave 90's failure was a probe that never opened the collapsed "Advanced" card (no code change).

## Owner questions
None.

## Cleanup
Stack stopped by PID tree (8693 / 5693 / 9393 free), lock released, `sr_p91_profile_ui` dropped, `p91-profile-bin` deleted (`p80-base-bin` untouched). No stash touched.
