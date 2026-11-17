# Wave 93, circuit: SEC-AUD-419 (D-926), the Circuit player skin passes the 44 px hit test on a phone

Started 12:54 (T = 13:15 in the brief); waited for the lock from 12:58; lock taken 13:29, one stack round 13:29 to 13:35 (seed, spec run 1 to 5, the last run green), stack stopped by PID tree (8716 / 5716 / 9416 free), lock released 13:35. One round was enough: the CSS in the last commit is the CSS that was measured. Changed files: `apps/web/src/player/skins/backdrop/circuit.css` (one block at the end), `e2e/tests/p93-circuit-shots.spec.ts` (new). `CircuitSkin.tsx` is not changed.

## Base (ran it: the same tree with the phone block removed from the live sheet), 375 x 812 touch, skin read back `circuit` on `lounge-onyx`

26 controls, **15 pass, 11 fail, 0 not reached** (`hitTestAll` from `e2e/support/hit-test.ts`, `openCollapsed` first). Document, dock and skin 375/375, 359/359, 359/359. Skin box 359 x 796 (scroll height 855); the dock itself scrolls 796/739 (that is the dock builder's item).

| control | box (w x h) | taker of the failing points |
| --- | --- | --- |
| skin picker, move, close | 34 x 30 each | the title bar (`div // NC_AUDIO_OS ...`) above and below |
| Mute | 102 x 24 | the Gain/Tempo panel above, the cover caption below |
| EQ switch | 33 x 16 | the pads panel above, the knob row below |
| queue rows 1, 2, 3 | 315 x 24 | the table head above row 1; the next row's title below (rows 2 and 3 take each other's points) |
| Repeat, Shuffle, Lib | 77 / 54 / 32 x 13 | the flags row above, the cover caption below |

Passing and unchanged in size: Seek 315 x 56, four Cue pads 52 x 59, Prev 69 x 44, Pause 96 x 60, Stop 68 x 44, Next 69 x 44, Volume and Tempo 28 x 72, four knobs 44 x 44.

## The rules as built (one `@media (max-width: 720px)` block, every selector under `.sr-ply-circuit`, comment names SEC-AUD-419 and D-926)

The failing controls draw their shapes in `::before` and `::after` (so a 44 px `::after` was not free), and each has room, so each grows to a 44 px box and its shape is inset to the size it was drawn at:
- Corner buttons: box 40 x 44, the drawn shape inset 7 px block, 3 px inline (still 34 x 30); the title bar is 44 tall and the red title shape keeps its 34 px height, centred.
- Mute: box 102 x 44, shape inset 10 px block (drawn 24).
- EQ switch: box 44 x 44, shape inset 14 px block (drawn 16 high); the knob panel's top padding becomes 3rem so the switch box (2 to 46 px) is clear of the knobs.
- Repeat, Shuffle, Lib: `min-block-size` 44 px, 8 px inline padding (there is room: 150 of 315 px used), row gap 0.
- Queue rows: `min-block-size` 44 px; the list's cap 7.5rem becomes 11.75rem (four rows), so three rows do not scroll inside the list.
Nothing else changed: Seek, Cue pads, transport, faders and knobs are untouched.

## After (same round), 26 of 26 pass, 0 fail, 0 not reached

Corner buttons 40 x 44, Mute 102 x 44, EQ 44 x 44, rows 315 x 44, Repeat 89 x 44, Shuffle 66 x 44, Lib 44 x 44; the 15 that passed keep their sizes to the pixel (positions move down with the taller parts above: Seek +10, transport +10, knob row +33).
- `scrollWidth` equals `clientWidth`: document 375/375, dock 359/359, skin 359/359; `measureFit` escaping: none.
- Gaps (px, base to after) to the nearest control: EQ to the knobs 27.5 to 24.5; Mute above 23 (Volume), flags to the Mute/knob row 133 to 128; rows 2 px apart as before. No two boxes overlap.
- Taps (real touch, each at the centre, 20 px above, 20 px below): Pause toggles 3 of 3; the queue rows play (row 2, row 3, row 1); Seek (taps at 80, 10 and 60 percent: the audio's `currentTime` is 33, 3, 24 s of 40); Volume (86, 50, 14); Mute, EQ and Shuffle toggle on every tap (4 of 4); Repeat cycles off, all, one, off; Move goes bottom-end, bottom-start, top-end, top-start and back (4 taps); the skin button changes the skin 3 of 3 (to Study; put back by the dev handle); Close closes 3 of 3 and the player is opened again by the handle.
- Reduced motion: running animations 15 with motion allowed, 0 with `prefers-reduced-motion: reduce` (base and after).
- Height: the skin's content (scroll height) 855 px to 974 px (+119); the skin box is capped at 796 by the dock as before, so it scrolls longer. The skin is never wider.
- 1280 x 800: 26 controls, every box equal to the base, 0 px difference; skin box 896 x 582 both, no scroll.

Observation, not this item and not isolated: after a Seek tap the audio's `currentTime` is 33, 3, 24 s at once and 0 s 20 ms later, in this harness (the tone is served by a `page.route`, ranged). The tap does reach Seek and the seek is requested; I did not check whether the reset is the harness or the player (the same code is in the base). The spec asserts the value right after the tap and logs the series (`REPORT seek tap ...`).

## Lazy chunks

`git grep -n "circuit.css\|CircuitSkin"`: `circuit.css` is imported only by `CircuitSkin.tsx`; that is loaded by `PlayerHost.tsx` with `lazy(() => import(...))`, plus `__tests__/backdrop-skins.test.tsx`. No web build run.

## Checks

Prettier on the sheet and the spec, e2e `tsc`, `node scripts/check-css-tokens.mjs` (73 files clean), `node scripts/i18n-audit.mjs --max 0` (TOTAL 0), `node scripts/check-doc-links.mjs` (ok). The spec ran green with `--project=chromium --reporter=line` (39.9 s). I did not run the spec on the base tree through git: the base case removes the block from the live sheet, so it is the same tree with the block off (stated, not a second checkout). The spec failed in my first runs on the seek step only (a spec error: the first version read the input's value and then the audio after 250 ms; fixed in the spec).

## Shots (`visual-sweep-shots/p93-circuit`, 5, all opened by me after the release)

`base-375-start.png`, `base-375-end.png`, `after-375-start.png`, `after-375-end.png`, `after-1280.png`, plus `measure.json` (every control's box, status, takers, gaps; scroll figures; taps). What the eye saw: after, the title bar is 10 px taller with the three corner buttons larger and the same red shapes; Mute is a taller cyan bar; the EQ chip sits at the top of the knob panel with room under it; the flags row is a little more open; the three queue rows are 44 px tall and whole at the end of the scroll; nothing cut, nothing overlapping; the 1280 shot is as the base. (The shots are Lounge Onyx, which is the skin's own fixed theme; the skin does not follow the main theme, so the three main themes cannot differ.)

## Not built / listed

Nothing failing is left. The dock's own scroll (796/739 here) is the dock builder's item (SEC-AUD-422).

## Docs

No doc states these sizes; `git grep` of "Circuit" in `docs/` found nothing about the controls' sizes.

## Proposed rows, changelog, owner questions

- SEC-AUD-419 closes: "Circuit on a phone (375 x 812 touch): 26 controls, 11 failed (corner buttons 34 x 30, Mute 24, EQ 16, rows 24, Repeat/Shuffle/Lib 13; takers: the title bar, the panels around, the neighbouring rows); one 720 px block grows each to 44 px with its drawn shape unchanged; after: 26 of 26 pass, no sideways scroll, 1280 boxes unchanged, the skin 119 px taller (D-926)."
- Open (new, for the commander to number): after a Seek tap in the harness the audio returns to 0 s within 20 ms (see the observation above); needs a look at the player's seek path with a real file, not a faked route.
- Changelog sentence: "The Circuit player skin has full-size (44 px) controls on a phone: the corner buttons, Mute, the EQ switch, Repeat, Shuffle, Library and the queue rows."
- Threat-model rows: none. AGENTS.md: none.
- For wave 94 (Valve, Stack): what worked: a control that draws its shape in both `::before` and `::after` grows its box to 44 px and insets the shapes (`inset-block`), which passes the vertical hit test without a pseudo-element; hit tests are vertical, so a crowded row side by side is not a problem, only boxes stacked above and below one another. What to avoid: an absolutely placed small switch needs its container's padding raised, or its taller box will take the points of the control under it (later in the DOM wins); and a seek test that reads the input's value rather than the audio.
- Owner questions: none.

## Re-shoot (from `e2e/`, on a stack with the sweep world: API 8716, Vite 5716, metrics 9416, `SR_ADMIN_REQUIRE_REAUTH=0`)

`SR_E2E_BASE_URL=http://localhost:5716 SR_P93_CIRCUIT_ON=1 SR_P93_CIRCUIT_BASE=1 SR_P93_CIRCUIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-circuit npx playwright test tests/p93-circuit-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (without `SR_P93_CIRCUIT_BASE=1` only the after case runs). Seeding on a stack with the `postgres` login needs `DATABASE_URL` and `SR_CLI_BIN`.

## Cleanup

Stack stopped, ports free, `sr_p93_circuit_ui` dropped, `p93-circuit-bin` deleted, lock released. No stash. `p92-base-bin` and `p80-base-bin` untouched.
