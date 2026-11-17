# Wave 93, study: SEC-AUD-420 (D-926) the Study skin passes the 44 px hit test on a phone; SEC-AUD-424 (D-928) its time readout reaches 4.5

2026-11-17. Round 1 only: lock `study` 13:05 to 13:12 (stack up, sweep world seeded with no FAIL line, base run, then four runs of the after case: the first three were spec or CSS fixes, see below). Stack stopped by PID tree (8717 / 5717 / 9417 free), `sr_p93_study_ui` dropped, lock released 13:12.

## Done (commits: `fe90d259` draft, then the final commit with this handoff)

- `apps/web/src/player/skins/backdrop/study.css`: one `@media (max-width: 720px)` block at the end (its comment names SEC-AUD-420 and D-926), and the readout colour.
- `e2e/tests/p93-study-shots.spec.ts` (new, local only: `SR_P93_STUDY_ON=1`).
- `StudySkin.tsx`: not touched. No string, no fragment.
- **The CSS in the last commit is the CSS measured.** The timer-button row gap was changed once after the first after run (`gap: 0.5rem` made Start and "Take a break" wrap into three rows, +110 px; `gap: 0.5rem 0.375rem` keeps the two on one row); the three runs after that change are the figures below. The first after run (skin 1302 px high) is not used.

## Base (ran it: the same tree with the phone block taken out of the loaded sheet in the page, readout colour put back), 375 x 812 touch, skin `study`, theme read back `lounge-light`

`openCollapsed` then `hitTestAll` on `.sr-ply[data-skin="study"]`: 22 controls, **11 fail, 0 not reached**, as wave 92:

| control | box (px) | taker of the failing points |
| --- | --- | --- |
| Skin / Move / Close (corner) | 28 x 28 | the title bar (`div.st-title`) above and below |
| Seek | 327 x 36 | the body grid above (`div.st-body`), the ribbon below |
| Start | 44 x 32 | the timer card above, the button row below |
| Take a break | 87 x 32 | same |
| Reset timer | 137 x 32 | the button row above, the card below |
| queue rows 1, 2, 3 | 305 x 36 | the queue card or the `ol.st-queue` (the neighbouring row's gap) |
| Volume | 143 x 20 | its own `span.sr-ply-volume` wrapper above and below |

Pass: Rain, Crackle, Café 94 x 72; Prev, Pause, Stop, Next 52 x 52; the notes; Shuffle, Loop 52 x 52; Mute 40 x 52. No failing point was taken by another control (the takers are layout containers), so no tap acts on a neighbour. Gaps to the nearest control above, below and beside: `measure-*.json` (`targets375.gaps`).
Document 375/375, dock 359/359, skin 343/343; skin box 359 x 796, scroll height **1192**. 1280 x 800: 22 control boxes in `boxes1280-base.json`.

## Rules as built (all under `.sr-ply-study`, `@media (max-width: 720px)`)

- Corner buttons (`.st-dot`): drawn 28 x 28 kept; `position: relative` and a centred 44 x 44 `::after` (logical insets); the group gap 0.375rem to 1rem so the three targets (pitch 44) do not overlap. The title text gets 20 px less room (it already ellipsises).
- Seek: the rail 2.25rem to 2.75rem (the range fills it); the drawn art (`.st-ribbon-art`) inset 0.25rem block so the tube and bunny look as before; the transparent thumb 2.75rem.
- Timer buttons (`.st-chip`, the round Reset): `min-block-size` 2.75rem, Reset 2.75rem wide (the container rule makes it fill a row, as before); row gap 0.5rem, column gap unchanged.
- Queue rows: `min-block-size` 2.75rem; the empty-slot stripes under the list re-timed to the new 3.125rem pitch.
- Volume range: `block-size` 2.75rem, track and thumb drawn as before (they sit in the middle of the taller input).
- Not changed (already pass): transport, ambience pads, notes, Shuffle, Loop, Mute.

## After (ran it; same world, same stack, three runs the same)

- Hit test: **22 controls, 22 pass, 0 fail, 0 not reached.** Boxes: corner 28 x 28 (target 44), Seek 327 x 44, Start / Take a break / Reset 44 high, queue rows 305 x 44, Volume 143 x 44.
- No two effective 44 px targets overlap (`targets375.overlaps` empty; the base reports six).
- Real touch taps at the centre, 20 px above and 20 px below: play and pause (alternates each tap); queue row 1 and row 3 each play (the current row becomes 0 and 2); Seek at 30, 60, 85 percent moves to 11, 25, 36 s of 40; Volume at 25, 50, 75 percent gives 21, 50, 79; the skin button changes the skin; move cycles the four dock places; close closes and reopening brings the skin back. All 21 taps worked.
- Sideways scroll: document 375/375, dock 359/359, skin 343/343; nothing leaves the player's box (`fit.escaping` empty). Skin height **1192 to 1250 px (+58)**: 8 px seek, 12 px Start row, 8 px between the button rows plus 12 px Reset row, 24 px queue. Same width (359).
- Queue tail: the third row whole inside the dock after two scrollers move (as on the base; the dock builder's cap makes it one).
- Reduced motion: running animations 6 (motion allowed) to 0.
- 1280 x 800: **22 control boxes equal to the base, max difference 0 px**; document 1280/1280.

## SEC-AUD-424: the time readout

- Rule: `.st-lcd .sr-ply-time`, colour `var(--st-olive)` at every width, now `color-mix(in oklab, var(--st-olive) 65%, var(--ui-text))`: a little more of the skin's ink; still olive, still lighter than the title and the other readouts.
- Grid (21 samples, text hidden, through `support/contrast.ts`; the computed colour is `oklab(0.4127 -0.0204 0.0277)` = sRGB 70, 79, 58): 375 minimum **4.05 to 5.02** (20th percentile 4.49 to 5.57, median 4.99 to 6.17); 1280 minimum **4.28 to 5.30**. Title minimum 8.20 (unchanged); other readouts 10.08, 10.08, 7.70 (minimums, none lower than the base).
- Note: the readout is 24 px (375) and 28 px (1280) bold, so D-928's 3.0 line already held; the task asked for 4.5 and it is met.

## Not done / listed

Nothing left for SEC-AUD-420 on this skin: no control needed a new layout. By eye: the corner buttons sit closer to the title text (20 px less room for it); Reset still fills its own row (pre-existing).

## Spec notes (errors fixed in the round; spec errors, not page faults)

The tap section first used the store state wrongly (the spec paused the audio element directly, so the first Play tap was a no-op), read the audio element's volume (the store's volume is not the element's), and read the skin before the lazy Stack chunk had compiled in the dev server (cold chunk, more than 10 s): it now syncs on the button's label, reads the range value, and warms the next skin once. Chromium's touch adjustment lets a tap 20 px outside a small control still hit it (on the base, taps 20 px above the 28 px corner button changed the skin), so `elementFromPoint` through `hitTestAll`, not a tap, is the measure; the taps show the controls still work.

## Who imports my files

`study.css` (the skin's): `git grep -n "study.css"` shows `StudySkin.tsx:48` only (the other `study.css` files are `apps/web/src/study/` and the showroom's own). `StudySkin` is loaded by `PlayerHost.tsx:37` with `lazy(() => import(...))`. A lazy chunk: no web build run by me.

## Shots (`visual-sweep-shots/p93-study`, 6 PNG, all 6 opened after the release)

`base-start-375`, `base-end-375`, `after-start-375`, `after-end-375`, `after-1280`, plus `base-1280` (the base case's own, not opened as a required shot). Looked at: the corner buttons, the ribbon (bunny and tube where they were), the timer buttons (two rows), the queue (rows taller, whole), the volume (thumb on its track), nothing cut or overlapping at 375; the 1280 deck as before. `measure-base.json`, `measure-after.json`, `boxes1280-*.json` are in the folder.

## The commander's command (from `e2e/`, on a stack with the sweep world: API 8717, Vite 5717, metrics 9417, `SR_ADMIN_REQUIRE_REAUTH=0`; with the postgres login also `DATABASE_URL` and `SR_CLI_BIN`)

`SR_E2E_BASE_URL=http://localhost:5717 SR_P93_STUDY_ON=1 SR_P93_STUDY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-study npx playwright test tests/p93-study-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

The same with `SR_P93_STUDY_BASE=1` is the base case (no old tree needed). The after case fails on any control that is not a pass.

## Checks

prettier clean on both files; e2e `tsc --noEmit` clean; `check-css-tokens` clean; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok; the spec passes (after case, 28 s). Not run: eslint and web tsc (no `.tsx` changed), web build (kit's).

## Docs

No doc states these sizes. AGENTS.md: no sentence needed.

## Proposed rows

- SEC-AUD-420 closes: "Study player skin on a phone (375 x 812 touch, `hitTestAll` on the skin's root): 11 of 22 controls failed (corner buttons 28, Seek 36, Start / Take a break / Reset 32, queue rows 36, Volume 20); now 22 of 22 pass, 0 not reached, taps at the centre and 20 px above and below all work, no sideways scroll, no change at 1280 (0 px on 22 boxes), the skin is 58 px taller (1192 to 1250); corner buttons keep their size with a 44 px `::after` (D-926)."
- SEC-AUD-424 closes: "Study's time readout on its gradient: the grid minimum was 4.05 (375) and 4.28 (1280); now 5.02 and 5.30 (D-928)."
- Threat-model rows: none. New faults: none.

## Changelog sentence

The Study player skin has full-size touch targets on a phone (the corner buttons, the seek ribbon, the timer buttons, the queue rows and the volume bar), and its time readout is easier to read.

## For wave 94 (Valve and Stack)

- Worked: measure the base by deleting the phone `@media` rule from the loaded sheet in the page (`CSSMediaRule`, `deleteRule`), so base and after are the same tree in one round; grow what has room, give a crowded corner button a 44 px `::after` with a gap of 44 minus its size; `hitTestAll` on the skin's root gave every row with its takers in about 30 s.
- Avoid: judging by a tap (Chromium's touch adjustment forgives a miss); a wider `flex-wrap` gap (2 px pushed a button onto a new row, +52 px); tapping a lazy skin's switch without warming its chunk; pausing the audio element directly and then tapping Play (the store disagrees).

## Owner questions

None.

## Cleanup

Stack stopped, `sr_p93_study_ui` dropped, `p93-study-bin` deleted, lock released, no stash touched, `p92-base-bin` and `p80-base-bin` untouched. Start and stop scripts and logs are in the scratchpad folder `study`, not committed.
