# Wave 93, dock: SEC-AUD-422 (D-927), a tall skin scrolls once

Clock: round 12:56 to 12:58 (lock `dock`, released 12:57); finished 12:59. Lock was free when ready, so taken early.

## Base (ran it, same round, the old cap put back by an injected rule)
375 x 812 touch, Owner, three tone tracks, bottom-end dock. Lounge channel open (`--sr-composer-clear` 149 px, lift 157 px, dock cap 647 px) and Settings open (clear 0, lift 8 px, cap 796 px). Figures are scrollHeight / clientHeight.

| skin | channel base: dock, skin | channel after: dock, skin | settings (base = after): dock, skin |
|---|---|---|---|
| valve | 796/647, 836/794 | 647/647, 836/645 | 796/796, 836/794 |
| tidepool | 658/647, 658/658 | 647/647, 658/647 | 658/658, 658/658 |
| circuit | 796/647, 855/796 | 647/647, 855/647 | 796/796, 855/796 |
| study | 796/647, 1192/780 | 647/647, 1192/631 | 796/796, 1192/780 |
| stack | 542/542, 540/540 | 542/542, 540/540 | 528/528, 526/526 |

The nested scroll exists only with the composer's clearance (channel): in Settings the dock does not scroll and the skin already scrolls alone. `--sr-ply-lift` reaches the skin's box (same value on both; the theme wrapper is `display: contents`), so the plan's rule works from `backdrop.css` alone.

## Built
One rule in `apps/web/src/player/skins/backdrop/backdrop.css`: `.sr-ply-dock[data-dock^='bottom'] .sr-ply-bd { max-block-size: calc(100vh / var(--sr-overlay-zoom, 1) - var(--sr-ply-lift) - 0.5rem); }` with a comment naming SEC-AUD-422 and D-927. The CSS in the last code commit is the CSS measured.

## After
- Channel: the dock's scrollHeight equals clientHeight on all five (647/647, 542/542). The skin box is the one scroller (one scroller moved to reach the end; base needed two on Valve, Circuit, Study).
- The third queue row is whole inside the skin's box at its end (and inside the viewport) on all five. At the start the skin's top is at 8 px and the first three buttons are inside it on all five.
- The dock's bottom is 157 px above the viewport bottom, same as base (D-789); skin box heights 647 against the dock's 647.
- Tidepool: in the channel state it was 658 tall against 647 of room, so on the base the DOCK scrolled by 11 px; now the skin box does (same single scroll, moved one level). Its figures are otherwise unchanged. Stack: every figure equal. Settings: every figure equal on all five.
- Study in a top dock (`setDock('top-end')`): equal to base (skin 1192/780, dock 796/796). Study at 1280 x 800: skin box 896 x 592 at (376, 130), equal to base (max-block-size 784 px becomes 714 px, not reached).
- No sideways scroll: document 375/375 and 1280/1280, skin width scroll equal on all.

## Who imports it
`backdrop.css` is imported by `player/skins/backdrop/frame.tsx:25`, which only the five lazy skins import (`git grep -n "backdrop.css"`). No first-paint or public page loads it; no web build run.

## Shots (6, `visual-sweep-shots/p93-dock/`, all opened by me after the release)
`study-end-375-base.png`, `study-end-375-after.png`, `valve-end-375-after.png`, `circuit-end-375-after.png`, `study-start-375-after.png`, `study-1280-after.png`, plus `measure.json` (all rows; its `skin` field of each row is overwritten by the figures, rows are in the order valve, tidepool, circuit, study, stack). Study base shows the skin ending with the dock still scrollable; after shows the skin's own edge at the top and the queue whole. Only the Lounge Dark main theme was shot (the backdrop skins wear their own fixed theme; Study/Tidepool light, the others onyx, read back as `data-theme`).

## Checks
prettier (css, spec), e2e `tsc`, `check-css-tokens` clean. The spec passes (`--project=chromium --reporter=line`, 37 s). Not run: eslint (pnpm not on PATH here), `check-doc-links` run below at commit.

## Re-shoot command (from `e2e/`, stack API 8715, Vite 5715, sweep world seeded)
`SR_E2E_BASE_URL=http://localhost:5715 SR_P93_DOCK_ON=1 SR_P93_DOCK_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-dock npx playwright test tests/p93-dock-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(`SR_P93_DOCK_BASE=1` base only, `SR_P93_DOCK_ONLY=study,valve`, `SR_P93_DOCK_THEME`.)

## Proposed row (SEC-AUD-422, closes)
"Wave 93 dock: a tall backdrop skin in a bottom dock scrolled twice (dock and skin; with the composer's clearance Valve 796/647 then 836/794, Circuit 796/647 then 855/796, Study 796/647 then 1192/780). The skin's box now takes the dock's own cap (`backdrop.css`, one rule, D-927): the dock never scrolls, the skin is the one scroller, the third queue row is reached with one scroll; top dock, Settings and 1280 unchanged (D-927)."

## Changelog sentence
A tall player skin now scrolls once, inside itself, to reach the end of the queue, instead of scrolling the dock and then the skin.

## Not done / notes
Hit-test figures are not mine. Owner questions: none. Server was started from the repo scratch parent (`SIDEROOM_wt`), not inside `apps/web`; `SIDEROOM_wt/data/search` was touched by it (ignored, outside every repo tree). No stash touched.

## Cleanup
Stack stopped by PID tree (8715, 5715 free), `sr_p93_dock_ui` dropped, `p93-dock-bin` and the scratch folder deleted, lock released. `p92-base-bin` and `p80-base-bin` untouched.
