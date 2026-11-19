# Handoff: fix/p108-sectionshots (wave 108, shots builder of pod sections)

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
- Base part 2 (job 0258) also shows, in no builder's task: Cinema home "Explore all" arrow links (`a._arrowLink` in `div._rowTools`, 82x19, 3 rows; kit sheet, taker a container) and, in the Cinema search dialog, "See all results" 100x29 and "Filters" 45x29 (`.cin-link-btn` in `p.cin-search-foot`; cinema's 526 reaches them).
- Study folder rows (`.stu-folder`, "Docs" 56x25, "Notes" 61x25) fail: library's A.2.

## Final (21:09)
Merged chips 2cab1cda, library 31f7117a then 022494c6, cinema 4766a8bb into my branch; build 0275 (after the cure), after part 1 job 0268 (build 0263, before the cure; library's cure touches only the Study, part 2), after part 2 job 0276. Base part 2 was re-run as 0270 (spec gained `.stu-entry`).

## Fails at 375 per builder (base to after)
- chips: 0 to 0. No chip, dropdown button or Drawer close failed in the base bundle on any page reached; the 46 px area changed no result and no drawn box (no problem line, no moved box at 1024). Wave 107's "chips fail" came from a tree without kitbits merged.
- library: Library part 1: 3 to 3, the three accepted `.lib-chip-list a` "Drama" tags; everything else passed in the base (sort both states, genre chips, empty-library links). Study (part 2): 2 to 0 after the second cure: `.stu-folder` "Docs", "Notes" (56x25, 61x25) failed after library's first commit (its `.stu-entry` rule is not the failing control); FAULT sent 21:03, cured by library 022494c6 (`.stu-folder` in the 44 px rule), measured in 0276: 56.4x44, 60.8x44, 0 fails, 0 problem lines. NOTE: library's handoff (fix-p108-library.md) does not mention this cure.
- cinema: part 2: the search dialog's "See all results" (100x29) and "Filters" (45x29) failed in the base, pass after (their boxes unchanged: 29 tall, tap area). Card Play 46x46 / Add to My list 44x44: pass in base and after (already wave 107). Spoiler and comment `.cin-link-btn`: not measured (no comment in the world); built by rule.
- Not in any pod, same in base and after (rows): Cinema home "Explore all" kit arrow links (`a._arrowLink` in `div._rowTools`, 82x19, 3 rows; taker a container). `.shv-empty-links` in Cinema Discover (outside `.lib-frame`) uncured, as library said; the spec did not reach it with an empty Cinema.

## Table against the lists (375, base to after; 1024: no problem line, nothing moved)
- Moved, as allowed: Study `.stu-entry` "My Files" 27 to 44 (the Study page shifts down 17 px below it); `.stu-folder` 25 to 44 and its row. Nothing else moved: chips, dropdown, close, sort button, genre chips, empty-state links, Browse toolbar, poster/card sizes, quick actions, title page. No sideways scroll on any page (scrollWidth equals clientWidth in both parts).
- The `.stu-open` file cards did not change (282.3 tall); the "file rows" the task meant are the folder pills.

## Shots opened after the jobs ended (base twin where noted)
cin-browse-search-375 (dialog with "ab", "See all results" and "Filters" under the note; identical to the base twin); lib-browse-filters-375 (genre chips five lines wide as base, no overlap, Reset and Show results below); lib-browse-empty-375 (two links under the message); cin-home-375 (cards with Play and plus circles, "Explore all" arrow links at the right, 19 px tall); cin-browse-filters-375 (year, runtime, selects, Reset and Apply; no change); lib-browse-375 and lib-browse-asc-375 (chips, sort chevron down and up, same layout); study-375 (My Files entry 44 tall, Recently added pills, Docs and Notes pills 44 tall, no overlap); study-empty-375 (empty message under the chips); cin-title-375 (action buttons as base). Not opened: the other shots (lib-series, lib-me, lib-list, lib-review, lib-reader, cin-party*, cin-player, cin-title tabs/dialogs, the 1024 shots): their figures have no problem line and no sideways scroll.

## FINDERS
None: no app file changed; the new spec is read by nothing.

## JOBS (id, class, submitted to started, rc)
0251 base p1 (waited 285 s) 0; 0252 base p2 (378 s) 0 (spec lacked search root); 0258 base p2 (78 s) 0; 0263 build (21 s) 0; 0268 after p1 (315 s) 0; 0270 base p2 (388 s) 0; 0271 after p2 (500 s) rc 1 (the two Study folder rows, the FAULT); 0275 build (136 s) 0; 0276 after p2 (5 s) 0. 0269 cancelled (a base re-run had to come first).
Commander's command, whole: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p108-integ/apps/web/dist --env SR_P108_SECTION_ON=1 --env SR_P108_SECTION_PHASE=after --env SR_P108_SECTION_PART=<1|2> --env SR_P108_SECTION_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-sectionshots -- tests/p108-section-shots.spec.ts

## Who imports my files
e2e/tests/p108-section-shots.spec.ts: Playwright only.

## Checks (last spec commit)
prettier --check on the spec: pass. e2e tsc: exit 0 (about 20:51). Not run: vitest, eslint, web tsc, i18n audit, doc links (left to the commander).

## Closing text, rows, changelog, owner questions
Closing: filter chips, dropdown chip, Drawer close, Library sort and genre chips and empty-state links, Cinema card links and search dialog buttons, Study folder pills and entries pass the 44 px test on a phone. Proposed rows: Cinema home "Explore all" arrow links (kit, 82x19); `.shv-empty-links` in Cinema Discover; `.cin-link-btn` comment actions and spoiler (needs a seeded comment). Changelog: none of mine. Owner questions: none.

## PROCESSES
strays.sh lists only the harness's own bash shells for this session (not mine). No process of mine left.
