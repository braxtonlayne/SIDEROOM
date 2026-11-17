# Wave 92, skins: SEC-AUD-406 (D-919), the five player skins on a phone, looked at and measured; nothing built

2026-11-17. Round 1 (spec, first figures) and round 2 (12:22 to 12:33: fixed contrast ground, queue tail, shots opened) both ran, in one lock (`skins`, released 12:32). Nothing is built (D-855 stands). Changed file: `e2e/tests/p92-skins-shots.spec.ts` only.

## What changed between the rounds (spec only)

- Contrast: the corner samples of a text box can lie outside a rounded or clipped ground (Circuit time 1.39 and the Valve nixie 1.38 in round 1 were such artefacts). `measure.json` now also holds `gridMedian` and `gridP20` (median and 20th percentile of the 21 grid samples). The figures below are the 20th percentile, with the minimum of the grid in brackets when it is lower. The helper still cannot read a gradient, an image or art.
- Queue tail: the check now also scrolls every scroller above the third row (the player's own box scrolls inside the dock in Valve, Circuit and Study) and reports `scrollersMoved`. A regex typo in the clipper label is fixed.
- Hit-test figures are unchanged from round 1 (same world, same stack).

## Figures, 375 x 812 touch, queue of three tone tracks, first title 57 characters

Hit test (D-856: centre and 21 px above and below; closed parts opened first):

| skin     | theme read back | controls | fail | what passes                                                                              | what fails (box in px)                                                                                                                                       |
| -------- | --------------- | -------- | ---- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| valve    | lounge-onyx     | 24       | 24   | none                                                                                     | 28 x 28 corner buttons; Seek 293 x 28; queue rows 285 x 22; knobs, bands, Volume, Balance 40 x 40; transport 73 x 40; toggles 28 x 32; speed 309 x 26        |
| tidepool | lounge-light    | 18       | 17   | Pause orb 62 x 62                                                                        | Library, EQ, Skin 92 x 36; corner 24 x 24; Seek 293 x 18; Shuffle 42; Prev/Next 52 x 40; Mute 30; Volume 56 x 16; Repeat, Stop, Drift 18 high; rows 27      |
| circuit  | lounge-onyx     | 26       | 11   | Seek, four Cue, Prev, Pause, Stop, Next, Volume, four knobs/faders (44 to 72)            | corner 34 x 30; Mute 102 x 24; EQ 33 x 16; rows 24; Repeat, Shuffle, Lib 13 high                                                                             |
| study    | lounge-light    | 22       | 11   | Rain, Crackle, Cafe 94 x 72; transport 52; notes; Shuffle, Loop; Mute 40 x 52            | corner 28 x 28; Seek 327 x 36; Start 44 x 32; Take a break, Reset 32 high; rows 36; Volume 143 x 20                                                          |
| stack    | lounge-onyx     | 36       | 26   | ten EQ sliders 18 x 104                                                                  | window buttons 20 x 18; Volume 220 x 8; Seek 335 x 16; transport 28 x 30; EQ, PL 42 x 22; rows 19; ADD, REM, SEL 22                                          |

No probe errors on any skin (every control had a box and its three points inside the viewport). No sideways scroll on any: document 375/375, dock 359/359, nothing leaves the player's box (`measureFit` escaping empty).

Contrast (ratio of the painted text colour on the ground read from the screenshot, text hidden; 20th percentile, grid minimum in brackets):

| skin     | title       | time readouts                                                                                                                                                       |
| -------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| valve    | 4.34 (4.11) | nixie digits 1.38 (1.05): not trusted, the glow digit sits on a dark glass tube the grid also samples; the shot shows it plainly legible. Other readouts 5.92, 9.56 |
| tidepool | 10.67       | 12.64 (0:08 / 0:40), "Total 0:40" 11.04 (round 1's 4.11 was a corner artefact)                                                                                      |
| circuit  | 13.6        | 8.28 (round 1's 1.39 was a corner artefact)                                                                                                                         |
| study    | 9.16        | "0:14 / 0:40" 4.48 (4.05); other readouts 7.7 to 10.1                                                                                                               |
| stack    | 7.98        | 7.98                                                                                                                                                                |

Valve title 4.34 (minimum 4.11) and Study's combined time readout 4.48 (minimum 4.05) are under 4.5 at the 20th percentile or the minimum of a gradient ground: borderline, not proven faults, and the helper cannot settle a gradient.

Reduced motion (`prefers-reduced-motion: reduce`): running CSS animations in the player 3/15/15/6/2 with motion allowed go to 0/0/0/0/0 on valve/tidepool/circuit/study/stack. Pixel difference 500 ms apart (title and time text hidden), motion allowed 7751/9513/13088/8225/188 and reduced 19/572/605/1466/229 of 194 to 265 thousand pixels: not zero on any skin; the audio is playing, so the progress bar and meters move; not isolated to a named element (unverified).

Queue tail (the third row, after every scroller above it is moved to its end): whole inside the dock on all five. On Valve, Circuit and Study it is reachable only by a second scroll: the dock scrolls to its end and the player's own box (`section.sr-ply`, overflow-y auto) scrolls again inside it (scroll heights 836/794, 855/796, 1192/780); with the dock alone at its end the row sits 12 to 90 px below the dock's bottom edge. Tidepool and Stack need no scroll.

Title: the title is the shared marquee ticker (`apps/web/src/player/player.css`, `.sr-ply-ticker`): with motion allowed an overflowing title pans and is cut by the clip box (the "hard cut" in the shots is a frame of the pan); under reduced motion the same rule gives an ellipsis. Not a fault.

## What the eye saw (all nine PNGs opened after the release)

`visual-sweep-shots/p92-skins/`: `valve|tidepool|circuit|study|stack-first-375.png` and `valve|circuit|study-expanded-375.png` (Tidepool and Stack do not scroll), plus `measure.json`.

- Valve: dense; the knob row and switches are the lower half; nothing cut. Corner buttons small.
- Tidepool: the glass player floats over a translucent queue wing; the page behind (conversation list, settings cog) shows through the wing under the queue; readable.
- Circuit: the deck fits in two screens; the three-row table is last. Clean.
- Study: tallest (player box 1192 px high); queue and ambience below the fold. Clean.
- Stack: shortest; three small windows and the smallest targets of the five (20 x 18 window buttons, 8 px Volume, 16 px Seek), though the whole player is on one screen.

## Proposed rows (the commander numbers them from SEC-AUD-408)

- SEC-AUD-406 closes: "five player skins on a phone, measured (375 x 812 touch, Owner, three tone tracks): no sideways scroll on any; hit test (D-856) fails/controls valve 24/24, tidepool 17/18, circuit 11/26, study 11/22, stack 26/36, all of them targets under 44 px; reduced motion stops every running animation; contrast of title and time readouts at least 4.3 where the helper can read the ground (Valve title 4.34 and Study time 4.48 borderline); the third queue row is reachable on all five, by a second scroll on three; nothing built (D-919, D-855)."
- Open (new, no number): per-skin phone touch targets under 44 px. Valve `apps/web/src/player/skins/backdrop/valve.css` (all 24 controls: knobs 40, buttons 28 to 32, Seek 28, rows 22); Tidepool `tidepool.css` (17 of 18); Circuit `circuit.css` (corner buttons, Mute, EQ/Repeat/Shuffle/Lib 13 to 24 high, rows 24); Study `study.css` (corner buttons 28, Seek 36, Start / Take a break / Reset 32, Volume 20, rows 36); Stack `stack.css` (window buttons 20 x 18, Volume 8, Seek 16, transport 28 x 30, rows 19; a Winamp-style fixed layout).
- Open (new, no number): Valve, Circuit and Study put the third queue row behind a second, nested scroll (dock, then `section.sr-ply`).
- Open (new, no number), commander's call: Valve title 4.34 and Study "0:14 / 0:40" 4.48 on gradient grounds (borderline under 4.5; a by-eye or palette check decides).
- Not proposed: Tidepool title ellipsis (it is the marquee; the ellipsis already applies under reduced motion). No contrast under 4.5 is confirmed. The reduced-motion pixel movement is a note, not a row.

## Owner lines

- On a phone Circuit is the best of the five (11 of 26 controls pass, including transport, Seek, cues and faders at 44 to 72 px) and Study next (11 of 22: transport 52, ambience pads 72). Valve (all 24 fail, nothing reaches 44 px) and Stack (26 of 36 fail, Volume 8 px, Seek 16 px) are the worst. Tidepool passes only its Pause orb. I do not pick the next skin (D-855).
- Owner questions: none.

## Re-shoot command (from `e2e/`, on a stack with the sweep world: API 8705, Vite 5705, metrics 9405, `SR_ADMIN_REQUIRE_REAUTH=0`)

`SR_E2E_BASE_URL=http://localhost:5705 SR_P92_SKINS_ON=1 SR_P92_SKINS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-skins npx playwright test tests/p92-skins-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

`SR_P92_SKINS_ONLY=valve,study` picks skins; `SR_P92_SKINS_THEME` sets the account's main theme (default `lounge-dark`). Seeding the world on a stack that uses the `postgres` login needs `DATABASE_URL` (same database URL as the server) and `SR_CLI_BIN` set, because the support helper's default database user does not exist there.

## Cleanup

Stack stopped by PID tree (8705 / 5705 / 9405 free), `sr_p92_skins_ui` dropped, `p92-skins-bin`, the start and stop scripts and the logs deleted, lock released. No stash. `p80-base-bin` untouched.
