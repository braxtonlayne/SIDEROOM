# Wave 94, valve: SEC-AUD-417 (D-930, D-934) the Valve skin passes the 44 px hit test on a phone, at 375 and at 700; SEC-AUD-425 (D-931) its title reaches 4.5

2026-11-17. Clock: started 14:00; lock `valve` round 1 14:20 to 14:27 (stack up, sweep world seeded with no FAIL line, base and after in one run, then one CSS fix and a second after run); round 2 14:29 to 14:31 (the cheek change below). Stack stopped by PID (8725 / 5725 / 9425: no listener, checked with `netstat`), lock released both times. All work finished by 14:35.

## Done (commits `cf8606bf`, `1c838d14`, and the handoff commit)

- `apps/web/src/player/skins/backdrop/valve.css`: the title's own rule (`.sr-ply-valve .v-plate .sr-ply-ticker`, after the nameplate rules) and one `@media (max-width: 720px)` block at the end (comment names SEC-AUD-417, D-930, D-934).
- `e2e/tests/p94-valve-shots.spec.ts` (new, local only: `SR_P94_VALVE_ON=1`).
- `ValveSkin.tsx`: not touched. No string, no fragment.
- **The CSS in the last commit is the CSS measured** (round 2 ran the working tree that was then committed as `1c838d14`; `cf8606bf` holds what round 1 measured, including its one fix, the corner strip from 8 px to 9 px).

## What I saw on the base (ran it: the same tree with the phone block and the title rule taken out of the loaded sheet, in the same run)

375 x 812 touch (narrow layout, skin 359 x 739, scroll height 836) and 700 x 812 (wide layout, skin 684 x 739, scroll height 900), Owner, theme read back `lounge-dark`, skin read back `valve` (its frame wears `lounge-onyx`), three tone tracks, first title 57 characters. `openCollapsed` then `hitTestAll` on the skin's root: **24 controls, 24 fail, 0 not reached**, at both widths.

| control | box 375 (px) | box 700 | taker of the failing points |
| --- | --- | --- | --- |
| Skin / Move / Close (`.v-cap`) | 28 x 28 | 28 x 28 | the mains plate (`div.v-mains`), above and below |
| Seek | 293 x 28 | 562 x 28 | the body grid (`div.v-body`) above and below |
| queue rows 1, 2, 3 | 285 x 22 | 554 x 22 | the folio title above row 1, the neighbouring row's text between rows, the folio below row 3 |
| 62 Hz to 16 kHz bands, Volume, Balance | 40 x 40 | 40 x 40 | `div.v-knob` (see "the knobs" below) |
| Previous, Pause, Stop, Next | 73 x 40 | 140 x 40 | the keys' group (`div.v-keys`) above and below |
| Tone, A-B loop, Shuffle, Repeat, Mute | 28 x 32 | 28 x 32 | the toggle's own cell (`div.v-tg`) above, its legend below |
| Tape speed | 309 x 26 | 578 x 26 | the toggle bank below |

No failing point was taken by another control (the takers are layout containers), so no tap acts on a neighbour. Gap between controls above and below one another: 1 px at the smallest (base and after). Document, dock and skin `scrollWidth` equal `clientWidth` on the base. 1280 x 800: every control's box recorded in `measure.json` (`base.desktopBoxes`).

Title (SEC-AUD-425), the grid as wave 92 took it (text hidden, ground from the screenshot, 21 samples, `support/contrast.ts`): **375: minimum 4.11, 20th percentile 4.34; 1280: minimum 4.16, 20th percentile 4.22** (wave 92's figures reproduce).

## What changed (all under `.sr-ply-valve`; every rule is a minimum or a drawing kept, nothing is smaller than it was and nothing is wider)

- Corner buttons (28 px): keep their drawing; `position: relative` and a `::after` 44 tall and 32 wide (`inset-block: -9px; inset-inline: -3px`: the button has a 1 px border, so the first try with 8 px gave 42 and one point missed; the neighbours are 5 px apart, so the strips do not touch).
- Seek: `block-size: 44px` on the input and its track, the thumb keeps 28 px (`margin-block-start: 8px`); the slot stays centred.
- Queue rows: `min-block-size: 44px`, `align-items: center`; the folio's `max-block-size: 12rem` (narrow layout) moves to the queue (`20rem`) and its grid rows are `auto`, so three rows are whole with no second scroller (the narrow layout's wide twin had the same problem at 700).
- Knobs: input and art `min` 44 (the wide layout's are larger already); in the narrow layout alone (a container query nested inside the media block) the seven columns are equal, so 44 px knobs fit 319 px.
- Transport keys and toggle levers: `min-block-size: 44px` (toggle width stays 28).
- Tape speed: slot 44 tall, the cap and the detent keep their 26 px and 22 px.
- The walnut cheeks are drawn the height of the skin's box, so a scrolled skin ends without them (a hand's width on the base, wider now); the same cheeks continue, plain, behind them with `background-attachment: local` (round 2). A seam from the drawn grain to the plain strip is visible only on close looking.
- Title: its own ground, a strip of the plate's own brass gradient (`--v-brass-hi` to `--v-brass-mid`), `padding-inline: 0.375rem`, radius 2 px, at every width. The ink is unchanged.

## After (ran it, round 2, `measure.json`)

| | 375 | 700 |
| --- | --- | --- |
| controls / pass / fail / not reached | 24 / **24** / 0 / 0 | 24 / **24** / 0 / 0 |
| smallest control box | 28 x 44 (toggles), 44 x 44 (knobs, corner buttons keep 28 x 28 with a 32 x 44 strip) | same |
| document / dock / skin `scrollWidth` vs `clientWidth` | 375/375, 359/359, 357/357 | 700/700, 684/684, 682/682 |
| skin box, scroll height | 359 x 739, **952** (base 836, +116) | 684 x 739, **1016** (base 900, +116) |
| dock scroll height | 739/739 (does not scroll); the skin is the one scroller | same |
| third queue row at the end of the scroll | bottom 722 inside the skin (747) and the viewport; the queue's own scroll 135/135 (no second scroller) | same |
| reduced motion, running animations | allowed 3 (base 4), reduced **0** | not measured |

- Title: **375: minimum 4.11 to 8.01, 20th percentile 4.34 to 8.07; 1280: minimum 4.16 to 8.08, 20th percentile 4.22 to 8.17.** The other readouts (5.92, 9.56) were not measured again: their rules are untouched. **The nixie digits are not measured** (the helper samples the dark glass tube they sit on; the shots show them plainly legible).
- 1280 x 800: every control's box equal to the base, **0 px** (24 of 24 compared). The phone block and the cheek rule are inside the 720 px media; the title's strip changes no control box.
- Taps (real touch) at the centre, 20 px above and 20 px below, each did its job: play and pause (3 flips), a queue row plays (rows 2, 3, 1), Seek: the input's own `input` event carried 33, 3 and 24 of 40 for taps at 80, 10 and 60 percent (D-935: the audio's time is not asserted), tape speed: input values 1.21, 0.79, 1.05 for taps at 90, 10 and 60 percent, knobs (62 Hz, Volume, Balance): the tap focuses the input and a mouse drag turns it (0 to 7, 80 to 100, 0 to 0.55), five toggles flip (Loop and Repeat by their label), Move goes round the four docks and back, Skin cycles to Tidepool (put back by the handle), Close closes the player (opened again with the queue).

### The knobs (read this one)
A knob's range input is `pointer-events: none` by design: the knob's wrapper `div.v-knob` takes the finger and turns the knob by dragging (a tap alone changes nothing, on the base too). The helper's default test therefore names `div.v-knob` as the taker of the centre of every knob, 17 of 24 pass and the 7 knobs "fail" by that default at both widths. **My spec takes the table twice and its assertions use `hitTestAll(page, root, { wrapper: '.v-knob' })`** (the helper's own option, nothing edited): the wrapper is the control the finger turns, and its box is 44 px or more. Both tables are in `measure.json` (`controls`, `strictControls`). If the commander prefers the strict reading, the fix is to let the input take the pointer (`pointer-events: auto; z-index: 1`) and then watch that a tap does not set the knob through Chromium's own slider touch handling: not tried, because a tap that jumps a tone knob would be a behaviour change. Owner question below.

## Shots (`visual-sweep-shots/p94-valve`, **7, all opened by me after the release**)
`base-375-start`, `base-375-end`, `after-375-start`, `after-375-end`, `after-700-start`, `after-700-end` (one more than the task lists, to see the cheeks at the end), `after-1280`. Seen: the skin keeps its look, order and palette; the knobs, keys and levers are a little larger and nothing overlaps or is cut; the title strip is a lighter brass band that still reads as Valve's own (dark ink on brass); the cheeks run to the end of the scroll at both widths. The title in the shots shows its first 30 or so characters, because the ticker pans a 57-character title (its text is `Midnight Drive With The Windows Down (Extended Club Mix) — Neon Coast Collective`, read from the page); it is not cut. Not looked at: Lounge Light and Editorial (the skin wears Onyx whatever the main theme is; the spec sets `lounge-dark` and reads `lounge-onyx` back from the frame).

## Checks (own tree, after my last edit, 14:32)
- `npx tsc --noEmit -p tsconfig.json` from `e2e/`: exit 0 (also at 14:29 after the last spec edit; the spec has not changed since).
- `npx prettier --check` on `valve.css` and the spec: clean. `ValveSkin.tsx` unchanged, so no eslint or web `tsc` needed.
- `node scripts/check-css-tokens.mjs`: 73 files clean, exit 0. `node scripts/i18n-audit.mjs --max 0`: TOTAL 0. `node scripts/check-doc-links.mjs`: see the commit note below.
- `npx vitest run src/player/__tests__/backdrop-skins.test.tsx --maxWorkers=1` (single file): 63 passed.
- The spec: `1 passed (58 s)` in round 2, the three soft assertions per width, the title minimum above 4.5 at 375 and 1280 and the 1280 box comparison all green. The first after run of round 1 failed on the three corner buttons (a 1 px border I had not counted: the strip was 42 tall); it was the CSS, fixed in the round.

## Who imports my files
`valve.css`: `ValveSkin.tsx` only (`git grep -n "valve.css"`). `ValveSkin.tsx`: `PlayerHost.tsx:34` (`lazy(() => import('./skins/backdrop/ValveSkin'))`) and the unit test. Lazy chunk only; no web build run.

## Not done / listed
- Nothing is in part: every control passes at 375 and 700 (24 of 24 by the helper with the knob wrapper option; 17 of 24 by its bare default, for the reason above).
- The nixie digits' contrast: not measured (unchanged).
- Seek's thumb on Firefox: I set the track height for WebKit only (`::-webkit-slider-runnable-track`); Firefox centres a thumb on a taller track by itself. Not run in Firefox (not installed).
- Valve in the wide layout at 700: the reel window shows the LCD over the reels' dark sides (the base does the same, unchanged).

## Proposed rows
- SEC-AUD-417 (close): "Valve passes on a phone: at 375 and at 700 (wide layout) 24 of 24 controls pass the 44 px hit test (base 0 of 24); knobs counted through their wrapper `div.v-knob`; the skin is 116 px taller, with one scroller; nothing changes at 1280 (D-930, D-934)."
- SEC-AUD-425 (close): "Valve's title reaches 4.5: the lowest grid sample 4.11 to 8.01 at 375 and 4.16 to 8.08 at 1280, on a strip of the plate's own brass (D-931). The nixie digits stay not measured."
- New row, low: the walnut cheeks and the panel's bevel are drawn the height of the skin's box in every scrolling skin (`.v-cheek` is absolutely placed in the scroller); Valve now continues its cheeks behind with `background-attachment: local`; the panel bevel (`.sr-ply-valve::before`) and the cabinet SVG still end at the first screen's height (Circuit and Study were not checked).
- Threat-model rows: none.

## Changelog sentence
The Valve player skin has full-size controls on a phone and a small tablet (every control is 44 px to tap), and its track title is easier to read.

## AGENTS.md
Nothing beyond the commander's planned sentence.

## Owner questions
- Should a knob's range input take the finger itself (so the bare hit test names the input) instead of the knob's wrapper? Default chosen: no, the wrapper is the control (it turns the knob; no behaviour change).

## Re-shoot (from `e2e/`, on a stack with the sweep world: API 8725, Vite 5725, metrics 9425, `SR_ADMIN_REQUIRE_REAUTH=0`)

`SR_E2E_BASE_URL=http://localhost:5725 SR_P94_VALVE_ON=1 SR_P94_VALVE_BASE=1 SR_P94_VALVE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-valve npx playwright test tests/p94-valve-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (without `SR_P94_VALVE_BASE=1` only the after case runs). Seeding on a stack with the `postgres` login needs `DATABASE_URL` and `SR_CLI_BIN`.

## Cleanup
Stack stopped (8725 / 5725 / 9425 free), `sr_p94_valve_ui` dropped, `p94-valve-bin` deleted, `.scratch` (untracked scripts and logs) removed, lock released. No stash touched. `p92-base-bin` and `p80-base-bin` untouched.
