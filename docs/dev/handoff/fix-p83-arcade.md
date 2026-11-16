# Wave 83 handoff: arcade (SEC-AUD-344, D-851)

Worktree `C:/Users/braxt/Desktop/SIDEROOM_wt/p83-arcade`, branch `fix/p83-arcade`. Web only, no cargo, no Rust, no migration.

## Clock
T = 12:10 (2026-11-16; my first `date` read 12:06). No new item from T+2h = 14:10; checkpoint T+2h20 = 14:30; finished T+2h45 = 14:55; hard limit T+3h = 15:10.
Actual: CSS and spec ready 12:09; waited for the lock 12:09 to 12:14 (developers and settings took it first); first round 12:14 to 12:16 (stack up, base run, after run, two spec fixes, shots opened after release); second round (the Showroom shot) see "Showroom". Finished far inside T+2h45.

## Done
- `apps/web/src/arcade/arcade.css`, inside the `max-width: 40rem` block only (comment rewritten: one compact hero, D-851, SEC-AUD-344). The cards are the direct children of `.arc-hero-cards` (`ArcadeSections.tsx`), so the rules I ended with:
  - `.arc-hero-cards > .arc-card[data-size='hero']:not(:first-child) { display: none; }` (replaces `.arc-hero-cards { display: none; }`)
  - `.arc-card[data-size='hero'] { min-block-size: 11rem; }`
  - `.arc-card[data-size='hero'] .arc-card-body { min-block-size: 11rem; max-inline-size: 100%; padding: 16px; }`
  - `.arc-card[data-size='hero'] :is(.arc-desc, .arc-flags) { display: none; }`
  - `.arc-card[data-size='hero'] .arc-card-title { font-size: 1.5rem; }`
  - `.arc-card[data-size='hero'] .arc-actions :is(button, a) { min-block-size: 44px; }` (the kit's small button is 28 px tall)
  No colour: the card stays dark through wave 82's D-844 block, untouched.
- `e2e/tests/p83-arcade-shots.spec.ts` (new, local only: `SR_P83_ARCADE=1`, `SR_P83_ARCADE_OUT`, `SR_P83_BASE=1`), copied from wave 82's spec. Three themes (`sectionThemes.arcade` set, `data-theme` read back from the room root) x 375 and 1280: visible hero count, heights, first row card's top, Play box, token probes, every card's box, sideways scroll, text colours and the pixels behind them (second shot, body hidden; hidden texts skipped), hero pixel hashes. The after run asserts: one hero at 375, at most 200 px, first row top under 812, Play at least 44 px and inside the card, nothing cut, light and dark probes equal Editorial's, every text at least 4.5:1, and at 1280 every card's box and both hero hashes equal the base. Also the click-through and a Showroom shot.
- Commits: `SEC-AUD-344: one compact Arcade hero card on a phone (D-851)` (css and spec); a later commit for the spec's Showroom test and this handoff.

## What I saw on the base (I ran it: arcade.css replaced by `git show HEAD`'s copy, `SR_P83_BASE=1`, same stack)
- 375, all three themes: no hero card shown (`visible: 0`); first "Playable now" row card's top at y = 431.9. 1280: both hero cards 274 px tall, first row top 552.
- The 1280 base data (boxes, hashes) is what the after run compared to.

## After (I ran it: 1 test passed)
| | Editorial | Light | Dark |
|---|---|---|---|
| `data-theme` read back | lounge-editorial | lounge-light | lounge-dark |
| hero cards shown at 375 | 1 | 1 | 1 |
| hero card height (375) | 178 px (card box 337.5 x 178; scrollHeight 176 = clientHeight, nothing cut) | 178 | 178 |
| first "Playable now" card's top (375) | 633.9 (base 431.9) | 633.9 | 633.9 |
| Play button (375) | 76.3 x 44, inside the card (35.8, 463.9 in a card at 18.8, 346.9) | same | same |
| title size | 24 px | 24 | 24 |
| `.arc-desc` display | none | none | none |
| `scrollWidth/clientWidth` 375 and 1280 | 375/375, 1280/1280 | same | same |

- The card's probed colour tokens (23 tokens, card colour, border, color-scheme) equal Editorial's in light and dark (the spec's `toEqual` passed).
- 1280, all three themes: every card's box equals the base and both hero cards' hashes equal the base (assertions passed). Editorial hashes `d5c7daf5...` and `e89d9bf5...`, as wave 82's.
- Contrast at 375, worst of 9 samples (left, middle, right x top, middle, bottom, text hidden in a second shot). The card shows kicker "Our flagship" and the title; Land.io has no meta line, so none is sampled at 375 (the second card, with a meta line, is hidden on a phone; at 1280 its meta is 7.1:1):

| text | Editorial | Light | Dark |
|---|---|---|---|
| kicker | 6.37 | 6.37 | 6.37 |
| title | 11.83 | 11.57 | 11.57 |

  1280 minimum is 4.6 (Arena 3D's title over the lit floor, Light and Dark; 4.97 Editorial), as wave 82 logged. `FAILURES(<4.5) none`.
- Click-through (light, 375): Tab reaches the hero's Play (outline `rgb(217,201,164) solid 2px`, Editorial's focus colour; shot shows the ring); pressing Play opens the "Start Land.io in..." dialog (1 dialog); Escape closes it (0).
- The flagship's painting reads at 11rem: the red block, its trail and the node are whole on the right, text on the left. `object-position` was not needed.
- Vitest `arcade/__tests__/arcade.test.tsx` single file: 40 passed. `node scripts/check-css-tokens.mjs`: 73 files clean. prettier clean, `node scripts/i18n-audit.mjs --max 0`: TOTAL 0. e2e `tsc --noEmit` clean.

## Imports
`git grep -n "arcade.css" -- apps/web/src`: `admin/AdminApp.tsx`, `arcade/ArcadePage.tsx`, `arcade/ArcadeShell.tsx`, `showroom/PreviewArcade.tsx` (plus a comment in `player/pageEnd.css`). All lazy chunks (wave 82 confirmed); no web build.
The Showroom's Arcade preview renders `ArcadeHero flags={[]}`, and `heroSlots` fills two slots from the built-in games, so it does render two hero cards at phone width and now shows one: see "Showroom".

## Shots (folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-arcade`)
- 10 from the first round, I opened all 10: base 375 Editorial and Light (2); after 375 Editorial, Light, Dark (3); after 1280 Editorial, Light, Dark (3); Light 375 Tab focus on the hero's Play (1) and pressed Play with the start dialog (1).
- Plus the Showroom shot (below); 11 in all. Reports `report-base.txt`, `report-after.txt`, `data-*.json` in the same folder.
- Seen: the hero is a dark painted card with the kicker, title and a gold Play button in all three themes; the rest of the page keeps the member's theme (blue Play buttons in Light and Dark); the title font follows the theme (D-831); at 1280 nothing differs from wave 82.

## Showroom
Second round 12:43 (the first acquire attempts waited 12:17 to 12:42 behind others). The Showroom preview (mocked platform.showroom flag, localStorage choice, Lounge Light Arcade) at 375: 2 hero cards in the DOM, 1 visible, 178 px tall, scrollWidth 375/375. Shot  (opened): one compact Land.io card with its meta line and no Play button (the preview has none); the grey bubble over the first row is the Showroom's own preview toggle popover, not this change. Total shots: 11, all opened.

## Docs
- `docs/10-design-system-and-theming.md` line 23, proposed (outside my files): "An Arcade hero card is one too, with Editorial's colours (D-844)." becomes "An Arcade hero card is one too, with Editorial's colours (D-844); on a phone only the first is shown, compact (D-851)."
- Nothing else states the hero's phone behaviour. `check-doc-links` not needed (no doc edited).

## Proposed inventory row (SEC-AUD-344)
"SEC-AUD-344: one compact Arcade hero card on a phone (D-851). The `max-width: 40rem` block no longer hides `.arc-hero-cards`: it shows the first hero card only (11rem, 178 px tall at 375), without description and flags, title 1.5rem, Play 44 px tall; the second house pick stays in Playable now. Still dark by D-844. Measured at 375 in Editorial, Light and Dark: one hero, 178 px, first row at y 634 (432 before), kicker 6.4:1 and title 11.6:1 or more, probes equal Editorial's, no sideways scroll; at 1280 every box and both hero hashes equal the base." Closes SEC-AUD-344.

## Changelog sentence
On a phone the Arcade now opens on one compact flagship card with its painting and a Play button, instead of a headline and a list.

## Owner questions
None. (Default kept: when only one live house pick exists the card shown is the first; with none, the grid is empty as before.)

## Not done / cleanup
- Nothing left undone. A stash was not used and none touched.
- Cleanup done: stack stopped by PID (server and Vite node), `sr_p83_arcade_ui` dropped, `p83-arcade-bin` deleted (`p80-base-bin` untouched), lock released. The scratch folder `C:/Users/braxt/Desktop/SIDEROOM_wt/p83-arcade-scratch` (css copies, start script, logs; outside the repo) can be deleted by the commander. `git status` clean; no `data/` folder in the worktree.
