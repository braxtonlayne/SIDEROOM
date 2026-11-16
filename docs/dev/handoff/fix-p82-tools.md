# Wave 82, tools builder: SEC-AUD-341 (D-845)

T = 11:10 on 2026-11-16. No new item from T+2h = 13:10; checkpoint T+2h20 = 13:30; finished T+2h45 = 13:55; hard limit T+3h = 14:10. Work started 11:06 (machine clock), finished about 11:15: one lock round (taken 11:08, released about 11:12).

## Done
- `apps/web/src/tools/tierlist/rooms.css`: `.sr-room-check input { accent-color: var(--ui-accent); }` and `.sr-room-check input:focus-visible { outline: 2px solid var(--ui-focus); outline-offset: 2px; }`. No gap change (measured 7 px, not under 6). `audio-tool.css` unchanged: no measured fault there. No `.tsx`.
- `e2e/tests/p82-tools-shots.spec.ts` (new, local-only `SR_P82_TOOLS=1`, `SR_P82_PHASE=base|after`).

## What I saw on the base (run, rooms.css unchanged; the stack was real: sr-server from a copy of `p80-base-bin`, Vite, scratch DB)
The Poll builder and the Audio tool were opened from the tools drawer by the option's own name ("Poll builder ...", "Audio trimmer ..."); the Poll needs a channel and a started room (the spec makes a server and clicks "Start a new one"). Only these two tools render `.sr-room-check` / `.utl-aud-check` (git grep); the tier list (the other `RoomShell` tool) renders neither (0).

| tool | box | gap (box to label text) | accent | keyboard ring | dy from label centre |
|---|---|---|---|---|---|
| Poll, base, all 3 themes, both sizes | 13 x 13 | 7 px | `auto` (the browser's blue; fault) | 2 px solid, offset 1 px (the global rule) | 0.0 |
| Poll, after | 13 x 13 | 7 px | Lounge Dark and Light rgb(88,101,242), Editorial rgb(194,176,143): equal to `--ui-accent` computed | 2 px solid `--ui-focus`, offset 2 px (Dark rgb(0,168,252), Light rgb(0,104,201), Editorial rgb(217,201,164)) | 0.0 |
| Audio, base and after (unchanged) | 13 x 13 | 11 px | the theme's (same values) | 2 px solid, offset 2 px | 0.0 |

So the Poll's box was whole in size, line and gap and had a ring; the one fault was the accent (`auto`). Built that and the stated ring. Every box at most 20 px, within 3 px of its label's centre, in all 3 themes at 1280 and 375. No sideways overflow in either tool at 375 (document scroll width minus client width 0 in every run); no field cut (looked at the shots). No other fault found in the two tools at 375.

Controls: all 22 inputs/selects/buttons of the open Poll form logged (box left/top/width/height) on the base and after, six theme/size combinations: all equal. The checkbox's own box did not move.

## rooms.css and the Lounge
`tools/tierlist/rooms.css` is imported only by `tools/tierlist/RoomShell.tsx`, which the registry loads with `import()` (Poll and Tier list): a lazy tools chunk. The plan's premise was wrong: `lounge/rooms/ListeningRoomCard.tsx` imports its own `lounge/rooms/rooms.css` (a different file, `lng-room-*` classes); nothing in `lounge/` uses `sr-room-*`. So the Lounge's card does not load this file and renders no `.sr-room-check`. First paint and public pages load nothing I changed: no web build run.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-tools`, plus `measurements.txt` and `controls-*.json`)
15 PNGs, I opened all 15: Poll after x6 (ticked and keyboard-focused; 3 themes x 2 sizes), Poll base Lounge Dark 1280 (blue default tick), Audio after x6, tier list Lounge Light x2. Seen: box on the label's line, tick in the theme's accent, ring visible. Other base shots made by the spec were deleted unopened-by-need; their figures are in `measurements.txt`.
Click-through (Lounge Light, 1280 and 375): the Poll's box ticked and unticked with the pointer and with Space, both ways correct.

## Tests
`node scripts/check-css-tokens.mjs` clean; prettier clean; `i18n-audit --max 0` TOTAL 0; e2e `tsc` clean; `src/tools/__tests__/rooms.test.tsx` 15 passed (CSS only; no test names the rule). The shots spec run on the base measured the fault (accent `auto`); after, it passes (14 shots, nothing outside the target, no control moved).

## Cleanup
Stack stopped by PID (4 listener PIDs), `sr_p82_tools_ui` dropped, `p82-tools-bin` deleted, lock released. `git stash list` not touched.

## Proposed inventory row
SEC-AUD-341 (D-845): the Poll builder's checkbox took the browser's blue (`accent-color: auto`); `.sr-room-check` now takes `--ui-accent` and a 2 px `--ui-focus` ring (rooms.css). The Audio tool's was already whole. Closes the "Poll and Audio checkboxes not reached" open item of wave 81.

## Changelog sentence
The Poll builder's "more than one answer" checkbox is now ticked in the theme's accent colour and shows a clear focus ring, matching the Audio tool's.

## Owner questions
None. Note for the commander: the plan/brief say `rooms.css` is also loaded by the Lounge's listening-room card; it is a different file of the same name.
