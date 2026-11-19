# Handoff: fix/p108-sectionshots (wave 108, shots builder of pod sections) - WORK IN PROGRESS

## Clock
Start 20:33. Spec committed 20:36 (e2e tsc 0, prettier ok). Base part 1 job 0251 ended about 20:43 (rc 0), part 2 job 0252 about 20:45 (rc 0); part 2 re-run 0258 (search dialog root fixed, Cinema home added).

## BASE FIGURES for chips (base bundle = integration/phase108, wave 107 in it; 375 touch; jobs 0251, 0252)
**In the base bundle no chip, no dropdown button and no Drawer close fails the 44 px hit test on any page the spec reaches.** Rows: Library Browse (chips 7 rows incl. the filters drawer: pass), Library Browse sort ascending, empty library, My library (chips), the Study (chips 6, and its empty folder), Cinema Browse (Drama chip, Room selection, its chevron, filters drawer Close: all pass). Wave 107 sectionshots' "chips fail" was a run on a tree without kitbits merged; staffshots' later run (kitbits merged) passed them too. Figures (from `figures-base-part<N>.json`, key `chipDiag`):
- `.chip` (every page): drawn 36 px tall, border 1/1/1/1, `::after` position absolute, top 17px, bottom -27px, height 44px, transform translateY(-22px) (the shipped rule is `top: 50%; height: 44px; translateY(-50%)`), so the area is centred on the border box and the border does not matter. No ancestor clips it: the only non-visible overflow is `body` (hidden) while a dialog is open. Row: `div._chips` 343 px wide, rows 12 px apart.
- Dropdown buttons (Room selection 83.5x36, chevron 32x36, border 0): `::after` top 18px, bottom -26px, height 44px, same transform; row `div._control` 139.5x34; no clipping.
- Drawer close (Library and Cinema filters drawers): drawn 32x32, border 0, `::after` top 16px, bottom -28px, height 44px; header `header._header` 374x65; no clipping. The modal/dialog close (Study edit, Cinema share, watch together) is a 44x44 box, no `::after`.
- Cinema party `.wp-controls` toggle (a `[role=group] > button[aria-pressed]`, 36x36): `::after` top -4px bottom -4px height 44px; clipped by `section.wp-stage-wrap` (overflow hidden) and `div.wp-body` (overflow auto); passes.
So there is nothing for chips to cure in this world; the only failing rows in the whole pod are listed in the next section.

## Base fails at 375 (jobs 0251, 0252) for the other two builders
- library: Study folder rows `button.stu-folder` "Docs" 56x25 and "Notes" 61x25 (taker: the list around them, a container): cure A.2. The three `a` tags of `ul.lib-chip-list` ("Drama" 36x16, series page, Reviews tab, review page): accepted by library (running text row). Everything else passes in the base: "Sorted descending" and "Sorted ascending" (36 px box, passes), the genre chips in the filters drawer, the empty library's links "Mount a bucket" and "Ask a friend" (21 px tall, pass: `.lib-frame .shv-empty-links a::after` already exists).
- cinema: nothing fails in the base on Cinema Browse (the card's Play 46x46 and Add to My list 44x44 pass), the title page, the player, the party. `.cin-link-btn` is not shown on the title page (no comment in the world); the search dialog's "See all results" (99.9x29) was not hit-tested in the first run (root error, fixed in 0258).
