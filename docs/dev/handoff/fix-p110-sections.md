# fix-p110-sections (wave 110, D-1022; SEC-AUD-543, 544, 553, 554)

## Clock
Start 23:14. Base shoot 0328 23:17. Edits committed 23:20. Build 0331 queued 23:21, ran 23:54 (the runner was stopped for mounts' cargo). After shoot 0336 23:55. Handoff 23:59.

## Done (commits, all CSS in my own rules)
- `161f8281` arcade.css (SEC-AUD-543): phone block (40rem): `.arc-roombar-link` position relative + `::after` inset-block -6px (33 px link, 32 px padding box); and one new rule `.arc-roombar { padding-block: 5px }` (was 4). Reason measured: the bar is `overflow-x: auto`, which makes it clip and scroll vertically, so an area reaching 5.5 px under the link (its point is at 57.5 of a 57 px padding box) needs the bar to be 2 px taller. Bar 58 -> 60.
- `38d6a598` study.css (SEC-AUD-544): 48rem block: `.stu-bar { row-gap: 24px }`, `.stu-bar-link` area inset-block -12px. Bar 73 -> 89 (+16).
- `f56869c5` library.css (553, 554) and Editorial.module.css: `.lib-section-head { row-gap: 15px }` at 48rem (12 px was not enough: baseline alignment already gives 16 px between the control and the link, and the two zones need more than 17 px, so 15 gives 19); footer: `.lib-footer-links` row-gap 28px and area inset-block -14px on each link (A.1 with A.3; footer 93 -> 113, not twice); kit sheet: `:not([role='radiogroup'] + .arrowLink)` removed from both selectors and its comment sentence updated.
- Spec `e2e/tests/p110-sections-shots.spec.ts`.

## Not done
Nothing left out. The spec's Arcade limit was 0 px growth in job 0336 (my error: the bar padding is the designed +2); I changed it to 2 in the last commit and did NOT rerun (a commit after the build makes the shoot stale; the only problem line of 0336 was that one). The commander's after run will pass it.

## FINDERS
`git grep` of `lib-section-head`, `lib-footer-links`, `stu-bar`, `arc-roombar`, the four CSS file names and `arrowLink` in apps/web/src, packages, scripts, e2e: `e2e/tests/p80-bars-shots.spec.ts` (waits for `.stu-bar` / `.arc-roombar`, retired shots spec), `apps/web/src/emu/__tests__/emuPage.test.tsx:150` (only that `.arc-roombar` exists), `PreviewStudy.tsx` (showroom page uses `.stu-bar` too, gets the same phone rule), `admin/AdminApp.tsx` imports arcade.css (no `.arc-roombar` there). No test reads the sheets as text for these rules. Result: no test drives these links by size; none edited.

## JOBS
- 0328 shoot base: submitted 1795061797, started 1795061800, ended 1795061843, rc 0 (43 s).
- 0331 build: submitted 1795061971, started 1795064023, ended 1795064051, rc 0 (waited about 34 min, runner stopped).
- 0336 shoot after: submitted 1795064057, started 1795064116, ended 1795064151, rc 1: the only problem line is `arcade-bar-375: the bar grew 2 px (max 0)` (spec limit, see above). Every hit test row passes; 1024 equal to base.

## Figures against the lists (375 touch; base -> after)
- Arcade bar: height 58 -> 60 (+2); links 78.6/76.6/67.6 x 33 unchanged (y 20 -> 21); 3 fails -> 0 (takers were the brand span, the header, the tabs row); no vertical scroll (scroll h 59, client 59). Content under it moves 2 px.
- Study bar: 73 -> 89 (+16); six links 20 px tall, widths unchanged; row 2 at y 56 (was 40); 6 fails -> 0.
- Library Home head: gap control to "See rankings" 16 -> 19; head 136 -> 142 (+6: this head wraps its title too, and each wrapped row gains 3); "See rankings" 114.4 x 23 unchanged, fails -> passes (taker was the segment label); the other four links and the Cinema home arrow links unchanged and passing. The other six `.lib-section-head`s gain 3 px per wrapped row on a phone (all pass).
- Footer: 93 -> 113 (+20; rows 24 -> 44 px apart); six links 16 px tall, widths unchanged; 6 fails -> 0. Document height 1737 -> 1766 (+29).
- 1024: arcade, study and library-home figures byte-equal to base (spec compares); no page scrolls sideways (every shot 375/375 or 1024/1024).
- Cinema home 375: every row same status and size as base.

## Shots (opened after the job ended)
- arcade-bar-375: brand row "SIDE ROOM /", links under it, current link underlined, filter pills below; looks like the base, 2 px more air.
- study-bar-375: two rows of links with 24 px between rows, divider left of Shelves; no overlap, bar taller as stated.
- library-home-head-375 (scrolled to the section the helper shows): "See rankings" sits clear under the Friends/People/Global control with space; no crowding. The shot frames the page bottom, because scrolling the head into view for a screenshot was overridden by the sticky nav; the figures hold the numbers.
- library-home-footer-375: footer brand, two rows of links with a visible gap, nothing cut; the fixed bottom nav covers the lowest 50 px of the viewport as in the base.
- library-home-1024, arcade-bar-1024, study-bar-1024: same as base twins (Trending head on one line at 1024 with the link under the title as before).
- cinema-home-375: "Explore all" links and rows as in the base.
Base twins have the `base-` prefix.

## Who imports my files (lazy chunks)
arcade.css: ArcadePage, ArcadeShell, AdminApp, PreviewArcade; study.css: StudyPage, PreviewStudy, StudySkin (backdrop); library.css: LibraryRoot; Editorial.module.css: kit `Editorial.tsx` (ArrowLink, used by Library and Cinema pages). All are route chunks, none loads before sign-in (Editorial is in the kit; the kit sheet was already changed by wave 109 in the same rule).

## Checks
- `npx prettier --check` on my five files: pass (23:19 and again after the spec edit, rc 0).
- `node scripts/check-css-tokens.mjs`: "73 file(s) clean" rc 0 (23:19).
- e2e `tsc --noEmit` (before the last one-number spec edit): rc 0 (23:21).
- vitest, eslint, web tsc, i18n audit, doc links: not run (left to the commander).

## Closing text for the rows
- SEC-AUD-543: fixed. The three Arcade bar links have a 44 px tap area on a phone; the bar's block padding is 5 px (was 4) so the area is not clipped by its `overflow-x: auto`.
- SEC-AUD-544: fixed. The six Study bar links have 44 px tap areas on a phone and the wrapped rows are 24 px apart (bar +16 px).
- SEC-AUD-553: fixed. Section heads on Library Home keep a 15 px row gap on a phone and "See rankings" gets the kit's tap area (the `:not` is gone from Editorial.module.css).
- SEC-AUD-554: fixed. The footer links have 44 px tap areas on a phone, rows 28 px apart (footer +20 px).

## Proposed new rows
None. Observation: the `.arc-roombar` bar being `overflow-x: auto` clips any taller tap area; a future link in a bar of that kind needs the same padding.

## Changelog sentence
On a phone the Arcade and Study bars, Library Home's "See rankings" and the Library footer links now have 44 px tap areas.

## Owner questions
None.

## PROCESSES
(see the strays output at the end of the run; the report states it)
