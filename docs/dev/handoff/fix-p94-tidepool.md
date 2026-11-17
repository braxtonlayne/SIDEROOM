# Wave 94, tidepool: SEC-AUD-428 (D-934), the Tidepool skin passes the 44 px hit test between 625 and 720 px

2026-11-17. Clock: start 13:55, lock taken 14:02, round 14:02 to 14:05 (one round, stack stopped, lock released 14:05), checks 14:06. Commits: `SEC-AUD-428: Tidepool wide layout phone targets`, then this handoff. **The CSS in the last commit is the CSS measured in the round** (no rule changed after the run).

## Base (spec `e2e/tests/p94-tidepool-shots.spec.ts`; helper `hitTestAll`; touch; queue of three tone tracks, first title 57 characters; the phone block deleted from the live sheet; I ran it)
Which layout shows: the player's width against 38rem (608 px): 700 -> 684 px wide layout, 640 -> 624 wide layout, 620 -> 604 narrow layout (so the narrow layout takes over between 640 and 620; the band is about 625 to 720). Document, dock and skin all equal at every width.

| Width | Layout | pass / fail / not reached |
|---|---|---|
| 700 | wide | 7 / 11 / 0 |
| 640 | wide | 7 / 11 / 0 |
| 620 | narrow | 1 / 17 / 0 (base of wave 93: the block only applies in the narrow layout and the sheet is stripped here) |
| 375 | narrow | 1 / 17 / 0 |

Wide layout failures (700; 640 the same set, boxes slightly narrower): Move and Close gems 26 x 26 (taker: the body above and below); Seek 256 x 18 (the screen grid above, the screen below); Mute 38 x 38 (the deck above, the volume span below); Volume 80 x 16 (the Mute ball above, the slot below); Repeat 115 x 20, Stop 74 x 20, Drift off 122 x 20 (the deck, the slot, the strip, the body); queue rows 164 x 27 (the wing above, the next row below, the wing below). Passing already in the wide layout: the three wing pills (110 x 57, 106 x 46, 110 x 57, rotated), Shuffle 56, Previous 76 x 52, Next 76 x 52, Pause 84. Skin height 498 at 700 and 533 at 640.

## What I built (`tidepool.css`, the one `@media (max-width: 720px)` block at the end; one comment names SEC-AUD-428 and D-934)
- The rules that are word for word the same in both layouts moved from the narrow container query to the media block's top level: crown 44 min, the gem and ball 44 px `::after`, Seek 44 px, the Mute/Volume 8 px gap, the Volume input and its track, the strip and the chips (44 px high, drawn 22 px), the queue rows 44 px. Measured: 375 is equal to wave 93 to the pixel (below).
- The narrow layout's own rules stay in `@container (max-width: 38rem)`: pills 44 min, gems gap 1.25rem, capsule margin 4.75rem, Shuffle and Prev/Next sizes, queue `max-block-size` 11.5rem.
- New `@container (min-width: 38.01rem)` (the wide layout): gems 1.125rem apart (26 px gems: centres 44 px apart), capsule `margin-inline-end: 5.5rem` so it never runs under the gems. Nothing else was needed: the pills, Shuffle, Prev, Next and Pause are 44 px or more in the wide layout.

## After (same round, same run)
| Width | Layout | pass / fail / not reached | Skin height (base) |
|---|---|---|---|
| 700 | wide | 18 / 0 / 0 | 548 (498, +50) |
| 640 | wide | 18 / 0 / 0 | 579 (533, +46) |
| 620 | narrow | 18 / 0 / 0 | 729 (638) |
| 375 | narrow | 18 / 0 / 0 | 747 (658, +89 as wave 93) |

Boxes at 700: gems 26 x 26 (target 44, 18 px apart), Seek 256 x 44, Mute 38 (target 44), Volume 80 x 44, chips 44 high, queue rows 164 x 44; 640 the same with narrower widths. Gaps between neighbouring boxes: Mute to Volume 8 px, rows 2 px, chips 12 px beside, Next to Mute 7 px (the Mute target reaches 3 px past its ball: no overlap).
- Sideways scroll: document, dock and skin scrollWidth equal clientWidth at 700, 640, 620 and 375 (700/700, 684/684, 684/684; 640/640, 624/624; 620/620, 604/604; 375/375, 359/359).
- One scroller (D-927): at 700, 640 and 620 nothing scrolls (the dock 548/548, 579/579, 729/729); at 375 the skin scrolls by 8 px (747 against 739), the one scroller, as in wave 93.
- Touch taps (centre, 20 px above, 20 px below): 46 at 700 and 46 at 375, 0 failed (Pause, queue rows, Prev, Next, Shuffle, Mute, Repeat, Drift off, EQ, Move, Skin pill, Close and reopen, Stop). Seek by the input's own `input` event: 3 of 3 at both widths (last values 0.23, 0.78, 0.50 for targets 0.25, 0.75, 0.5); Volume by its value: 0.20, 0.87, 0.53 (D-935).
- Reduced motion: 16 running animations with motion allowed, 0 reduced at 700 (base 16 / 0); 15 / 0 at 375.
- 375 against wave 93's sheet (the spec swaps the live sheet for a copy of wave 93's, in the same run): 18 controls each, **max box difference 0 px**, skin height 747 both, 18 of 18 both.
- 1280 x 800: 18 controls, **max box difference 0 px** against the base, height 498 both.

## Listed, not built
- At 640 the deck is crowded (the "PLAY / PAUSE" etch label touches "NEXT", the Mute ball overlaps the deck's groove) and the capsule title reads "Tidepoo..." (the capsule's `max-inline-size` is the wide layout's own, unchanged). I did not shoot the base at 640, so I cannot say it was different before; the deck rules are untouched except the Mute/Volume gap (6 to 8 px). Not a hit-test fault; a polish item for a later wave if the owner cares.
- Firefox track rules are written but not measured (Chromium only), as in wave 93.

## Lazy chunks
`tidepool.css` is imported only by `TidepoolSkin.tsx` (`git grep -n "tidepool.css"`; also named in docs and the spec), which `PlayerHost.tsx` loads lazily. `TidepoolSkin.tsx` unchanged. No web build run, no cargo, no Rust.

## Shots (`visual-sweep-shots/p94-tidepool`, 4 PNGs, all opened, plus `measure.json`)
`base-700`, `after-700`, `after-640`, `after-375`. Base 700: the gems, Seek, chips and rows small, the shell the same shape. After 700: same look, taller by 50 px, rows and chips roomier, nothing cut or overlapping at 700 or 375. After 640: all in place; the deck crowding noted above. After 375: as wave 93.

## Checks (on my own tree, after my last edit; clock 14:06)
- e2e `tsc` (`cd e2e && npx tsc --noEmit -p tsconfig.json`): exit 0.
- `npx prettier --check` on the css and the spec: exit 0.
- `node scripts/check-css-tokens.mjs`: clean, exit 0. `node scripts/i18n-audit.mjs --max 0`: TOTAL 0, exit 0. `node scripts/check-doc-links.mjs`: ok, exit 0.
- The spec itself, `--project=chromium --reporter=line`: 1 passed (2.4 m), in the round.
- Not run: eslint on the spec (the e2e package has no eslint of its own), web `tsc` (no `.tsx` changed), web build (none this wave).

## Commander's command (from `e2e/`, stack with the sweep world, API 8723, Vite 5723, metrics 9423, `SR_ADMIN_REQUIRE_REAUTH=0`)
`SR_E2E_BASE_URL=http://localhost:5723 SR_P94_TIDEPOOL_ON=1 SR_P94_TIDEPOOL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-tidepool npx playwright test tests/p94-tidepool-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`SR_P94_TIDEPOOL_BASE=1` runs the base passes only; leave `SR_P94_TIDEPOOL_W93CSS` unset on the merged tree: it points at a copy of wave 93's sheet and skips the comparison without it).

## Proposed rows
- SEC-AUD-428 closes: "Tidepool between 625 and 720 px (wide layout): 11 of 18 controls failed the hit test at 700 and 640 (gems 26, Seek 18, Mute 38, Volume 16, chips 20, queue rows 27; takers the body, the deck, the screen, the neighbour); now 18 of 18 pass at 700, 640, 620 and 375, 0 not reached, 92 touch taps do their job, no sideways scroll, 375 equal to wave 93 and 1280 equal to the base (0 px); the skin is 50 px taller at 700 (D-934)."
- Open (new, optional): Tidepool's deck is crowded at about 640 px (label "PLAY / PAUSE" meets "NEXT"), and the capsule title is cut to an ellipsis there; not measured against the base.

Changelog sentence: The Tidepool player skin keeps its full-size controls on a small tablet or a phone held sideways.

Owner questions: none.

## Cleanup
Stack stopped by PID tree (8723, 5723, 9423 have no listener), `sr_p94_tidepool_ui` dropped, `p94-tidepool-bin` deleted, lock released at 14:05 (it went to the next builder). No stash. `p92-base-bin` and `p80-base-bin` untouched. `e2e/.state` is untracked and not committed.
