# Wave 81 handoff: arcade (SEC-AUD-336, D-839)

**T = 09:35 on 2026-11-16.** Clock times: no new item from 11:35, checkpoint 11:55, finished 12:20, hard limit 12:35. Actual: started 09:34, lock taken 09:36, round done and lock released 09:41, handoff committed about 09:45.

## Done
- **SEC-AUD-336 (D-839).** `apps/web/src/arcade/arcade.css` only (7 lines added, 1 changed): the text column of every non-hero card (`.arc-card:not([data-size='hero']) .arc-card-heading`) is `container-type: inline-size`; the title is `font-size: clamp(1.125rem, 16cqi, 1.5rem)`. `overflow-wrap: anywhere` stays. The hero title rule (`:512`, more specific) and the phone's 1.25rem (`@media`, later in the file) win over it and are unchanged.
- Spec `e2e/tests/p81-arcade-shots.spec.ts` (new, local only: `SR_P81_ARCADE=1`, `SR_P81_BASE=1` names the run `base`, out `SR_P81_ARCADE_OUT`). Typechecks, prettier clean.

## What I saw on the base (run: yes, real stack, same stack and spec for base and after)
- The broken card is a **row card**, not a list card: the Playable-now cards at 1280 have a 130 px text column (the art takes 2fr). Light theme, 1280: **"Whiteboard" needs 131.4 px at 24 px, column 130: 2 lines, "Whiteboar / d"** (shot `base-light-desktop-playable-now.png`). In the light 960 px log it is also two lines (needs 131.4, column 131). Shipped theme: needs 117.8, fits. No other built-in name breaks at 1280, 960 or 375, shipped or light.
- On a phone, a list card with a 34-letter name breaks in light (needs 309.7 at 20 px, column 304; shipped needs 266, fits); the 45-letter name breaks in both themes.

## Title table (light theme, Playable now; font px / column px / lines; card height)
| title | 1280 base | 1280 after | 375 base and after |
|---|---|---|---|
| Land.io | 24 / 130 / 1; 247 | 20.8 / 130 / 1; 244 | 20 / 192 / 1 |
| Arena 3D | 24 / 130 / 1 | 20.8 / 130 / 1 | 20 / 192 / 1 |
| Hold'em | 24 / 130 / 1; 247 | 20.8 / 130 / 1; 244 | 20 / 192 / 1 |
| **Whiteboard** | 24 / 130 / **2, broken**; 247 | 20.8 / 130 / **1**, needs 113.9; 244 | 20 / 192 / 1 |
| Tabletop | 24 / 130 / 1 | 20.8 / 130 / 1 | 20 / 192 / 1 |
| Sketch & Slander | 24 / 130 / 2; 274 | 20.8 / 130 / 2; 267 | 20 / 192 / 1 |
| Emulator | 24 / 130 / 1 | 20.8 / 130 / 1 | 20 / 192 / 1 |
List cards (server cartridges) at 1280: 24 px, one line, unchanged (column 958). Cards no taller anywhere: every row card at 1280 is 3 to 7 px shorter, every other card equal (full per-card boxes in `report-base.txt` / `report-after.txt`). Sideways scroll: document `scrollWidth == clientWidth` at 375, 960 and 1280, shipped and light, all four sections.

## Rule chosen and why
First choice in the task: the column steps the title down continuously (16cqi: at the 130 px column 20.8 px, at 192 px 24 px cap; floor 1.125rem). 16cqi because the widest built-in word measured is "Whiteboard" at 5.475 em in the light face: 18.3cqi is the break-even, 16cqi leaves 12% room (113.9 of 130). The task's 13cqi would have shrunk every title in the grid to the 18 px floor. Only the row cards at desktop widths change size (20.8 instead of 24 px); phone and list cards are as before.
- Long names: a 45-letter cartridge name still breaks (at 1.125rem it needs more than the column) and nothing overflows its card (`overflow=false` in the log; shots `*-longname.png`).
- Known limit: a CSS rule cannot know the word, so a 34-letter name on a phone list card in a light theme still breaks at 20 px although it would fit at 18 px. No built-in or realistic name is affected.

## Shots and click-through
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-arcade/`: 20 `after-*` (four section links x shipped/light x 1280/375, plus the long-name cards in each of the four combinations), 20 `base-*`, plus `report-*.txt`, `measure-*.json`. **Opened and looked at by me: all 20 after shots and the base light 1280 Playable now (21); the other 19 base shots not each opened (the log covers them).** `data-theme` read back (`sideroom` / `lounge-light`) before every run. Click-through at every size and theme: Play on the first card opens the "Start in" dialog (1 dialog), Escape closes it (0); the four section links pressed in turn. (The "filters" are in-page links to the sections, not filters.)

## Checks
`node scripts/check-css-tokens.mjs` clean; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; `arcade/__tests__/arcade.test.tsx` 40/40; prettier clean; e2e `tsc` clean. Who imports `arcade.css`: `ArcadePage.tsx`, `ArcadeShell.tsx` (both through `emu/routes.tsx` lazy imports), `admin/AdminApp.tsx` (loaded by `StaffDoor.tsx` with `import()`), `showroom/PreviewArcade.tsx` (lazy in `ArcadePage.tsx`): all lazy, no web build.

## Proposed inventory row
SEC-AUD-336: a game's name on an Arcade card steps down with its text column (clamp 1.125rem..1.5rem, container query) and no longer breaks inside a word that fits ("Whiteboar / d" in a light theme at 1280). Closed; base and after logged on the real stack.

## Changelog sentence
"Whiteboard" (and any game name that fits) is one word on the Arcade's cards in every theme.

## Owner questions
1. A 30+ letter name on a phone list card in a light theme still breaks at 20 px although it would fit at 1.125rem. Want phone list titles at 1.125rem too (smaller for all of them)? Default: no.
2. The light Arcade's hero cards (the featured painting fades into a pale veil under the title): not looked at in detail this wave (the shots show only the small Land.io card); left alone as planned.
3. The Arcade nav "filters" are anchor links that scroll; the task called them filters.

## Cleanup
Stack stopped by PID (server 33084, Vite 14716), `sr_p81_arcade_ui` dropped, `p81-arcade-bin` deleted (not `p80-base-bin`), lock released. No `git stash` used or seen.
