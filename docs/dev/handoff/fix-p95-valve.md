# Wave 95, valve: SEC-AUD-432 (D-940) Valve's bevel and cabinet are whole when the skin scrolls

2026-11-17. Clock: started 14:52; lock `valve` 14:59 to 15:03 (one round: stack, seed with no FAIL line, my spec with base and after, wave 94's spec); released 15:03; finished 15:05. Ports 8733 / 5733 / 9433 free (netstat), `sr_p95_valve_ui` dropped (count 0), `p95-valve-bin` deleted, `.scratch` and `e2e/.state` removed. No stash touched; `p92-base-bin` and `p80-base-bin` untouched.

## What I saw on the base (ran it: my rule switched off in the loaded sheet, same run)
Owner, `lounge-dark`, frame read back `lounge-onyx`, three tone tracks, touch.
- 375 x 812: skin box 359 x 739, root `clientHeight` 737, `scrollHeight` 952. 700 x 812: 684 x 739, 737 / 1016.
- Bevel (`::before`) computed height 737 px, cabinet SVG and backdrop 737 px (content rows 1 to 738), at every scroll position. So they end 214 px (375) and 278 px (700) short of the end of the content. Cheeks 737 (plus the plain `local` cheeks of wave 94).
- Pixel probe (bevel's shaded side column against the column inside it, 8 rows): at the end of the scroll the lowest rows are `-4` at 375 and `0, -3, -3` at 700 (no bevel); after: all rows positive (375: min 4.5 end, 4.4 mid, 7 start; 700: min 6 end, 7.4 mid, 10.4 start). The 375 base also lost the cast noise at the end (spread 0 in the lower patches).
- Shot `base-700-end.png` shows the break plainly: the panel changes tone and the bevel stops at about two thirds of the way down.

## What I built (all in `valve.css`, one new `@media (max-width: 720px)` block at the end, comment names SEC-AUD-432, D-940, D-927)
The frame is drawn as the skin's own `background-attachment: local` layers, so it follows the content for the whole scrolled height (as the cheeks do since wave 94): the iron gradient (linear plus the lit radial), the cast noise (the same turbulence as an SVG data-URI tile, `overlay` blend, `stitchTiles`), the bevel's top light, foot line and shaded sides (inset 2.25rem, 0.875rem under 40rem), over the wave 94 cheek layers. `.sr-ply-valve::before` gets `content: none` and `.sr-ply-bd-backdrop` `display: none` (nothing in the flow, so no control box moves). The block sets `--v-frame-whole: 1` (the spec's base case deletes the media block that holds it). Nothing above 720 px changes. `ValveSkin.tsx` not touched; no string.
Known small differences from the old drawing: the bevel's side shade is a 6 px linear fade instead of a Gaussian inset shadow; the iron gradient and the lit corner are stretched over the whole scrolled height instead of the first screen's height (shots at the start look the same to me).

## After (ran it, the same run)
| | 375 | 700 |
| --- | --- | --- |
| root client / scroll height | 737 / 952 (base same) | 737 / 1016 (base same) |
| bevel pixel minimum drop start / mid / end | 7 / 4.4 / 4.5 (base 2.6 / -4 / -4) | 10.4 / 7.4 / 6 (base 6.5 / -3 / -3) |
| document, dock, skin scrollWidth vs clientWidth | equal at all three positions (dock 359/359, scroll 739/739; skin still the one scroller) | equal (684/684, 739/739) |
| hit test, `wrapper: '.v-knob'` (D-939) | 24 of 24, 0 not-reached | 24 of 24, 0 not-reached |
Wave 94's spec `p94-valve-shots.spec.ts`, unchanged, same round, its own variables, after case only: 1 passed (34 s): 24 of 24 at 375 and 700 with the wrapper option (17 of 24 bare), the title minimum above 4.5 at 375 and 1280, 1280 comparison not run (no BASE) . Reduced motion: no animation rule touched, not measured again.

## Circuit and Study (looked at, nothing built; 375, scrolled to the end; scroll 739/974 and 723/1250)
Opened `circuit-375-end.png` and `study-375-end.png` (and the starts). Circuit: the cyan outer border runs the whole height and the hazard stripe is at the foot; Study: the pink-cream outer frame is continuous to the end. I see no framing art that stops part-way in either. No row proposed. (Looking, not pixel-probed.)

## Shots (`visual-sweep-shots/p95-valve`, 12 files; the 6 the task lists are all opened by me, plus base-375-start, base-375-end, base-700-end: 9 opened. Not opened (extras): base-700-start, after-700-start, circuit-375-start, study-375-start.)

## Checks (own tree, after my last edit, 15:04)
e2e `npx tsc --noEmit -p tsconfig.json` exit 0; prettier --check on `valve.css` and the spec exit 0; `check-css-tokens.mjs` exit 0 (73 clean); `i18n-audit.mjs --max 0` TOTAL 0, exit 0; `check-doc-links.mjs` ok, exit 0. `ValveSkin.tsx` unchanged (no eslint, no web tsc). **The CSS in my last commit is the CSS I measured** (the only edit after the round was the spec's probe offsets, which ran in the same round).

## Who imports my files
`valve.css`: `ValveSkin.tsx` only (`git grep -n "valve.css"`); `ValveSkin.tsx`: `PlayerHost.tsx:34` (lazy) and a unit test. Lazy chunk only; no build run, no first-paint effect.

## Commander's command (from `e2e/`, stack with the sweep world, API 8733 / Vite 5733)
`SR_E2E_BASE_URL=http://localhost:5733 SR_P95_VALVE_ON=1 SR_P95_VALVE_BASE=1 SR_P95_VALVE_LOOK=1 SR_P95_VALVE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p95-valve npx playwright test tests/p95-valve-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

## Proposed row text (SEC-AUD-432, close)
"Valve's bevel and cabinet are whole at every scroll position on a phone: the frame is the skin's own `local` background layers, 737 / 952 (375) and 737 / 1016 (700) with the bevel present at the start, middle and end (base: gone for the last third); 24 of 24 controls still pass (wrapper option); Circuit and Study looked at, no frame ends part-way (D-940)."

## Changelog sentence
On a phone, Valve's panel frame no longer stops part-way down when the skin scrolls.

## Owner questions
None. (Choice made: the bevel's side shade is a linear fade, not a blur.)
