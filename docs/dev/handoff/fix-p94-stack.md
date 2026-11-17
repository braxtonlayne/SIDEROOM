# Wave 94 handoff: stack (SEC-AUD-421, D-930, D-934)

Clock: started 13:55, lock `stack` 14:00 to 14:02 (one round), finished 14:03. Commit `SEC-AUD-421: Stack skin phone block (44 px targets) and its spec`.

## Base (measured in the same run: the phone block removed from the loaded sheet)
33 controls (the dock's skin button, move and close are counted with the skin), 10 pass, 23 fail at 375 and at 700, 0 not reached. Failing: window buttons 20 x 18 (taker: the window or section above), Mute 20 x 18, Volume 8 tall, Seek 16 tall, EQ and PL 22, the eight transport keys 30, the three window-hide buttons 18, EQ on and Preset 22, ADD, REM, SEL 22 (takers: the readouts, the previous window, the queue row's number and title). The ten EQ sliders pass. Skin box 359 x 542 at 375 (488 high at 700); scrollWidth equals clientWidth everywhere; the skin did not scroll.

## Built
One `@media (max-width: 720px)` block at the end of `apps/web/src/player/skins/backdrop/stack.css` (every selector under `.sr-ply-stack`, comment names SEC-AUD-421, D-930, D-934): `.ss-bar` 44 px; `.ss-tiny` gets a centred 44 px `::after`; Seek and Volume inputs 44 px tall (native track, look unchanged); `.ss-toggle`, `.ss-key`, `.ss-presets`, `.ss-small` 44 px tall; queue rows `min-block-size` 44 px, text centred; queue box `max-block-size` 11rem (so three rows are whole). No markup, no string, nothing above 720 px.

## After (same run), 375 and 700
33 of 33 pass, 0 not reached, at both widths. scrollWidth equals clientWidth for document, dock and skin at both. Skin height 542 to 739 at 375 (now scrolls once inside itself: scrollHeight 787, clientHeight 737; the dock does not scroll, 739/739); 488 to 737 at 700 (no scroll). Third queue row whole at the end (shot). Reduced motion: running animations 0 (2 with motion). 1280 x 800: 33 control boxes equal to the base, 0 px differ. The ten EQ sliders still pass and still move (the spec's hit test; no slider rule changed). Taps (real touch, centre, 20 px above and below): play and pause, rows select, Seek and Volume fire `input` with the tapped values (seek 33, 3, 24 of 40; volume 18, 50, 93; D-935, no currentTime assertion), Mute, EQ, PL, Shuffle flip, Repeat cycles, move goes round four docks, skin button leaves the skin, close closes. No tap error.

Not listed or left: nothing. All 33 pass, so row 421 can close.

## Does the CSS in the last commit equal the CSS measured
Yes (committed before the round, unchanged since).

## Who imports the files
`stack.css`: `StackSkin.tsx` only; `PlayerHost.tsx` loads that lazily (`git grep`). No first-paint or public page. No web build run. `StackSkin.tsx` not changed.

## Checks (after the last edit, 14:02 to 14:03)
e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (14:00, spec not edited since); prettier --check on the two files: clean; `check-css-tokens`: exit 0; `i18n-audit --max 0`: TOTAL 0; spec run with `--project=chromium --reporter=line`: 1 passed (55.9 s), base and after. `check-doc-links`: ok, exit 0.

## Shots (`visual-sweep-shots/p94-stack`, 9 PNG plus `measure.json`, all 9 opened)
base and after at 375 (start, end), base and after at 700 (start, end), after 1280. Lounge Dark only (the skin is dark in every theme). Nothing cut or overlapping; the 375 start view shows the playlist foot behind the bottom nav, reached by the skin's own scroll (end shot shows ADD, REM, SEL, MISC whole). Title "Midnight Drive With The Windows Down (Extended Club Mix)" is the seeded 57-character title, ellipsed by the ticker as before.

## Re-shoot (from `e2e/`, stack with sweep world: API 8726, Vite 5726, metrics 9426, `SR_ADMIN_REQUIRE_REAUTH=0`)
`SR_E2E_BASE_URL=http://localhost:5726 SR_P94_STACK_ON=1 SR_P94_STACK_BASE=1 SR_P94_STACK_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-stack npx playwright test tests/p94-stack-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (seeding on the `postgres` login needs `DATABASE_URL` and `SR_CLI_BIN`).

## Proposed rows
- SEC-AUD-421 (close): Stack passes the hit test on a phone: 33 of 33 at 375 and 700 (base 10 of 33), `stack.css`, taller windows (skin 542 to 739 px at 375, scrolls once).
- Changelog: On a phone, the Steel Stack player skin has full-size controls: taller title bars, Seek, Volume, keys, toggles and playlist rows.
- Threat-model rows: none. Owner questions: none. No fragment added.

## Cleanup
Stack stopped by PID tree (8726, 5726, 9426 free), `sr_p94_stack_ui` dropped, `p94-stack-bin` deleted, scratch folder removed, lock released. No stash. `p92-base-bin` and `p80-base-bin` untouched.
