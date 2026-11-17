# Wave 94, tidepool: SEC-AUD-428 (D-934), the Tidepool skin passes the 44 px hit test between 625 and 720 px

2026-11-17. Clock: start 13:55; round 1 14:02 to 14:05; the commander looked at `after-640.png` and found the deck crowded, so round 2 14:10 to 14:14 (stack stopped, lock released 14:14); checks 14:14. Commits: `SEC-AUD-428: Tidepool wide layout phone targets`, then `SEC-AUD-428: Tidepool stacked layout up to 720 px`, plus handoffs. **The CSS in the last commit is the CSS measured in round 2.**

## Base (spec `e2e/tests/p94-tidepool-shots.spec.ts`; helper `hitTestAll`; touch; queue of three tone tracks, first title 57 characters; the phone block deleted from the live sheet; I ran it twice)
Layout by the player's width against 38rem (608 px): 700 -> 684 wide layout, 680 -> 664, 660 -> 644, 640 -> 624 wide; 620 -> 604 narrow. The band is about 625 to 720.

| Width | Layout | pass / fail / not reached | Skin height |
|---|---|---|---|
| 700, 680, 660, 640 | wide | 7 / 11 / 0 each | 498, 522, 522, 533 |
| 620 | narrow | 1 / 17 / 0 | 638 |
| 375 | narrow | 1 / 17 / 0 | 658 |

Wide layout failures (700; the others the same set): gems 26 x 26 (taker: the body), Seek 18 tall (the screen grid, the screen), Mute 38 (the deck, the volume span), Volume 16 tall (the Mute ball, the slot), Repeat/Stop/Drift off 20 tall (the deck, slot, strip), queue rows 27 tall (the wing, the next row).

**Crowding that was already on the base (the commander's finding, measured):** the wide layout's deck (five slots, about 442 px) does not fit the body at any width under 720. Overlapping control boxes on the base: 700: Shuffle x Previous 6 px, Previous x Pause 7, Pause x Next 15, Next x Volume 14; 680: 9, 11, 20, 18; 660: 12, 14, 25, 21; 640: 15, 18, 30, 25 and Next x Mute 4. At 640 the capsule title is cut ("Tidepoo...": name 103 px in 69) and the labels "PLAY / PAUSE" and "NEXT" run together. The screen title is a ticker (scrolls by design: 923 px of text in 66), not a fault.

## What I built (`tidepool.css`, the one `@media (max-width: 720px)` block at the end; one comment names SEC-AUD-428 and D-934)
Round 1 gave the wide layout its own 44 px rules and left the crowding. Round 2: **at 720 px and under the stacked layout (the narrow container query's own rules) applies at any player width**: those rules are repeated at the top of the media block inside `@container (min-width: 38.01rem)`; the 44 px rules that follow (they come last) no longer sit in a container query, they hold for both. The wide-only gem and capsule rules of round 1 are gone. Nothing changes above 720 px. The player is never in its wide layout under 720 now, so the 625 to 720 band looks as 620 and 375 do.

## After (round 2, same run)
| Width | Layout | pass / fail / not reached | Overlapping boxes | Skin height |
|---|---|---|---|---|
| 700 | stacked | 18 / 0 / 0 | none | 729 (base 498) |
| 680 | stacked | 18 / 0 / 0 | none | 729 (522) |
| 660 | stacked | 18 / 0 / 0 | none | 729 (522) |
| 640 | stacked | 18 / 0 / 0 | none | 729 (533) |
| 620 | stacked | 18 / 0 / 0 | none | 729 (638) |
| 375 | stacked | 18 / 0 / 0 | none | 747 (658) |

- At 640 against the base: overlaps 5 -> 0 (controls and etch labels checked pairwise), capsule "Tidepool Glass" whole (89 in 89), 18 of 18 (was 7).
- Sideways scroll: document, dock and skin `scrollWidth` equal `clientWidth` at 700, 680, 660, 640, 620 and 375. One scroller (D-927): nothing scrolls at 700 to 620 (dock 729/729); at 375 the skin scrolls 8 px (747 against 739) as in wave 93.
- Touch taps (centre, 20 px above, 20 px below): 46 at 700 and 46 at 375, 0 failed; Seek by the input's own `input` event 3 of 3, Volume by its value (D-935).
- Reduced motion: 16 allowed, 0 reduced (base 16 / 0) at 700; 15 / 0 at 375.
- **375 against wave 93's sheet** (swapped into the page in the same run): 18 controls, max box difference **0 px**, height 747 both.
- **1280 x 800:** 18 controls, max box difference **0 px** against the base, height 498 both.
- Cost: the player is taller in the band (729 against 498 to 533) because the wings stack above and below the body, as on a phone; it is not wider.

## Listed, not built
- Firefox track rules are written but not measured (Chromium only), as in wave 93.

## Lazy chunks
`tidepool.css` is imported only by `TidepoolSkin.tsx` (`git grep -n "tidepool.css"`), loaded lazily by `PlayerHost.tsx`. `TidepoolSkin.tsx` unchanged. No web build, no cargo.

## Shots (`visual-sweep-shots/p94-tidepool`, 8 PNGs, all opened, plus `measure.json`)
`base-700`, `base-680`, `base-660`, `base-640` (the crowding and overlaps), `after-700`, `after-660`, `after-640`, `after-375`. After: wings stacked above and below the body, deck in one row with the labels apart, Volume beside Next with a gap, capsule whole, nothing cut or overlapping. The title is shot mid-scroll (ticker).

## Checks (own tree, after my last edit, 14:14)
e2e `tsc` (`cd e2e && npx tsc --noEmit -p tsconfig.json`) exit 0; `npx prettier --check` css and spec exit 0; `check-css-tokens` clean; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok; the spec in the round: 1 passed (2.8 m). Not run: eslint on the spec (the e2e package has none), web `tsc` (no `.tsx` changed), web build.

## Commander's command (from `e2e/`, stack with the sweep world, API 8723, Vite 5723, metrics 9423, `SR_ADMIN_REQUIRE_REAUTH=0`)
`SR_E2E_BASE_URL=http://localhost:5723 SR_P94_TIDEPOOL_ON=1 SR_P94_TIDEPOOL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-tidepool npx playwright test tests/p94-tidepool-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`SR_P94_TIDEPOOL_BASE=1`: base passes only; leave `SR_P94_TIDEPOOL_W93CSS` unset on the merged tree, the comparison with wave 93 is then skipped).

## Proposed rows
- SEC-AUD-428 closes: "Tidepool between 625 and 720 px: 11 of 18 controls failed the hit test at 700 to 640 (gems 26, Seek 18, Mute 38, Volume 16, chips 20, rows 27) and the wide layout's deck overlapped (boxes by 4 to 30 px, labels ran together, capsule title cut); now at 720 px and under the stacked layout shows at any width: 18 of 18 pass at 700, 680, 660, 640, 620 and 375, no overlap, no sideways scroll, 92 touch taps do their job, 375 equal to wave 93 and 1280 equal to the base (0 px); the skin is 231 px taller at 700 (729 against 498) (D-934)."

Changelog sentence: The Tidepool player skin uses its stacked layout and full-size controls on a small tablet or a phone held sideways.

Owner questions: on a 700 px tablet the Tidepool player now stacks its wings above and below (729 px tall) instead of the side-by-side wide layout (498 px, crowded). Default taken: stacked, because the wide deck cannot hold 44 px controls in that width.

## Cleanup
Stack stopped by PID tree (8723, 5723, 9423 free), `sr_p94_tidepool_ui` dropped, `p94-tidepool-bin` deleted, lock released at 14:14. No stash. `p92-base-bin` and `p80-base-bin` untouched. `e2e/.state` untracked, not committed.
