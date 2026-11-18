# fix-p105-sheet (D-1008, SEC-AUD-471)

## Clock
Started 15:30, code done 15:32, committed 15:3x. No job run ("not needed": no .ts/.tsx changed).

## Done
- `D-1008 lounge.css (ALL READY)`: one new rule `.lng-search-sheet .lng-search-chips` (sticky, inset-block-start 0, z-index 1, flex none, background `var(--bg-base-low)`), placed before `.lng-search-sheet .lng-chip`, inside my lines. No end block.

## Cause (from markup and rules, and the p103 shot; loungeshots' base figures not received, so the cause is a reading, unmeasured)
`SearchBox` renders `form.lng-search` (44 px, flex none) then `section.lng-search-panel` which holds the chips row, the suggestions, the count row and every hit group as direct children. In the sheet the panel is `flex:1; min-block-size:0` with the base rule's `overflow:auto`: it is the sheet's only scroller. The chips row is a plain child, so when the panel is scrolled (focus on the Sort select, a keyboard move, or the spec's scroll) the chips scroll up under the search box. The p103 shot shows exactly the rounded bottom edge of a chip row under the box, i.e. a scrolled chips row, not a squeezed one (the row is not inside the 44 px `.lng-search` box; that is a sibling). Cure: keep the row at the top of its scroller (sticky), whole at its natural height.

## Not done
Nothing built, no shots (loungeshots measures). If its base figures show the chips row is NOT scrolled but squeezed, this cure is wrong: revert to a put-back and measure.

## REACH table
| element | file:line | opened by | styled now by | found by class in e2e/__tests__ |
|---|---|---|---|---|
| `.lng-search-chips` (sheet only) | SearchBox.tsx:266; lounge.css new rule before :2311 | header search button at 375 (`.lng-phone-search`) | `.lng-search-chips` :1761 (flex, wrap, gap 4) + new sheet rule | p103-search-shots spec reads chips by role/button; none by this class that I changed |
| `.lng-chip` | SearchBox.tsx:268; :2311 | same | unchanged (44 px min) | unchanged |
| `.lng-search-panel` | :2300 | same | unchanged | unchanged |
Desktop panel at 1024 is not reached: selector needs `.lng-search-sheet`. `git grep` class `lng-search-chips`: only SearchBox.tsx and lounge.css.
Possible side effects to measure: the 8 px gap under a sticky row lets results scroll visibly 8 px beneath the chips (cosmetic); the chips row grows nothing (sticky does not change layout box when unscrolled), so no box moves at scroll 0.

## JOBS
none.

## Imports
`PhoneSearch.tsx` is imported by the lounge header (lazy Lounge chunk); lounge.css by the Lounge chunk. No first-paint or public file changed.

## Checks (on my last commit tree, 15:31)
prettier --check lounge.css: clean, 0. `node scripts/check-css-tokens.mjs`: clean, 0. vitest p71Panels.test.tsx 19 passed, p70Lines.test.tsx 25 passed (single files, 15:31). tsc, eslint, i18n audit: not run (no ts or string change).

## PROCESSES
strays: see reply (none found of mine).

## Closing text for SEC-AUD-471
Phone search sheet: the filter chips row stays at the top of the results scroller (sticky), whole at 44 px per line, instead of scrolling away to a sliver under the box. Awaiting loungeshots' after-shot confirmation.

## Changelog sentence
The phone search sheet now keeps its filter chips fully visible under the search box while results scroll.

## Owner questions
None.
