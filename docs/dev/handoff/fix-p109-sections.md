# fix-p109-sections (wave 109, sections: D-1022 with D-1025; SEC-AUD-530 Library and Cinema, 531, 532, 533)

## Clock times
Started 21:36 (the machine's clock; T was 21:50). Last job ended 22:14. Handoff 22:15.

## Done (commits on fix/p109-sections, base integration/phase109)
- `e2e/tests/p109-sections-shots.spec.ts` (new): measures, then compares (see its header, which holds the commander's command).
- 530: `library.css`, `cinema.css`: on a phone each section bar link gets a tap area, and the bar 4 px of padding below the links.
- 531: `packages/ui/src/sideroom/Editorial.module.css`: `.arrowLink` gets an exactly 44 px tap area on a phone (kit breakpoint 720 px).
- 532: `storage/shelves/shelves.css`: the `.shv-empty-links` phone rules moved there from `library.css` (unscoped); the `.lib-frame` copy is deleted.
- 533: nothing to cure (measured, see figures).
(Commit subjects 2 to 5 of the first batch lost the blank line before the Co-Authored-By trailer: the trailer sits in the subject paragraph. No other effect.)

## Not done
- Library Home's "See rankings" link (the one under the segmented control) is left out of the kit's area on purpose, see 531. It fails as it did in the base.
- Nothing at pointer widths was touched.

## What step 1 confirmed (base, 375 touch; figures-base.json)
- Bars (Library Browse, Cinema Movies): each of the 6 links 39 px tall at y 0.5 to 39.5, the bar 41 px (content-box 40 + 1 border) with the content (`main`) at y 106. The 6 links of each bar FAIL (`edge: 'top'`): `below@41` and `span@43`, taker a container (`div.lib-topbar`, `div.cin-header`). Under the bar within 6 px: a container only, no control. Both bars scroll inside themselves (`overflow-x: auto`), so a `::after` below the bar's padding box is clipped: that is why the bar gets padding.
- Cinema home (`/cinema`, storefront switched on by the spec): 3 "Explore all" arrow links, each 82 x 19, in a `rowHead` (first 56 px tall, the others 28 px). All 3 FAIL: above and below points taken by containers (`div.cin-page`, the row head, the section).
- Library Home arrow links (outside the Cinema): 5 links 23 px tall, all FAIL (the first's above point taken by the segmented control's label, a control; the others by containers).
- Cinema Discover empty (the spec answers `/cinema/discover` with `{hero: null, rows: []}`, the way a member with nothing mounted sees it): "Mount a bucket" and "Ask a friend", 21 px tall, both FAIL (container takers). The Library's twin (Browse, the world's Library is empty) PASSES in the base: that is the `.lib-frame` rule.
- Comment with the spoiler (title `extras.spoilerCommentTitleId`): "Show spoiler" 44 px, "Like (0)" 29, "Reply" 39.8 x 29 all PASS. "Contains spoilers" is a text span in the line ("This contains spoilers."), not a control. Line before reveal 44 px; the same line without the button's `min-block-size` is 33.6 px.

## REACH table
| control | where | sheet and rule | cure | what the area reaches |
|---|---|---|---|---|
| Library section bar links x6 | Library, phone | `library.css` `.lib-frame[data-mobile] .lib-sectionbar-link::after` (`inset-block: 0 -5px`) + `header.lib-sectionbar { padding-block-end: 4px }` | A.1 with the bar's own padding | link 39 px at y 0.5 to 39.5; the area (5 px from its padding box, 1 px border) reaches 43.5 px from the top of the screen; nothing above. Under the bar: the bar's own 4 px padding, then `lib-topbar` (4 px lower than the base) |
| Cinema section bar links x6 | Cinema, phone | `cinema.css` same rules with `.cin-` (one `@media (max-width: 48rem)` block holds the two) | same | same |
| ArrowLink | Cinema rows' "Explore all" (3), Library Home (4 of 5) | `Editorial.module.css` `.arrowLink:not([role='radiogroup'] + .arrowLink)::after` at `max-width: 720px` | A.1, centred, 44 px: `inset-block-start: calc(50% - 21.5px); block-size: 44px` (border counted) | 11.5 px above and below a 19 px link; 10.5 px above a 23 px one |
| `.shv-empty-links a` | Cinema Discover, Library Browse empty | `shelves.css` (media 48rem) | A.1 (as wave 108 wrote it, `inset-block: -12px`) + row gap 24 | 12 px above and below |

Why the bar's area is `inset-block: 0 -5px`, not `-4px` and not `inset-block-end` alone: an `::after` with only a bottom inset has height 0 (it never took a point); and the link's 1 px bottom border is outside the padding box the inset is measured from (the brief's formula): 4 px below the link needs 5 px.

## Figures against the task's lists (375 touch; base then after)
May change (all within): section bar `padding-block-end` 0 to 4 px on the phone bars (bar 41 to 45 px high; the links did not move: x, y, width, height equal to 0.2 px, checked by the spec); content under each bar moved down 4 px (limit 5; `main` top 106 to 110, both bars). Tap areas as above. Spoiler line: the 44 px button makes the line 10.4 px taller than its natural 33.6 px (limit 15): nothing to cure, the CSS is unchanged (the wave 108 estimate of 16 was a different baseline). Revealed text is a two-line paragraph (51.2 px), so reveal makes the comment 7.2 px taller (139 to 146.2).
Must not change: 1024: both bars' rows (box, status, takers) are equal to the base (spec compares the whole `bar-*-1024` figure; no problem line). Drawn links: equal. No sideways scroll: `scrollWidth/clientWidth` 375/375 on every shot, 1024/1024 for the 1024 shot. Nothing that passed fails: the spec compares every control of each root by name (Cinema home 24 controls, Library Home 21, empty states, comment): no problem line in the last run (job 0322).
Hit tests after: 6 of 6 links pass on each bar (edge top, 3 points inside); 3 of 3 Cinema "Explore all" pass; Cinema empty 2 of 2 pass (Library twin still 2 of 2); Library Home 4 of 5 arrow links pass (base 0 of 5), "See rankings" still fails with the segmented control's label as taker (as in the base); comment 3 of 3 pass.

## The two things step 2 found that the task did not say
1. The first try at 531 (a symmetric 14 px area) made Library Home's "Friends" (segmented control, passing in the base) FAIL: the link under it takes its lower point (the two 44 px zones overlap; the head has a 1 to 5 px gap). Rule: nothing that passes may fail, so the one link that follows a `role='radiogroup'` is left out of the kit's area (`:not([role='radiogroup'] + .arrowLink)`); its own failure stays as in the base. The cure for that pair is a row gap in `.lib-section-head` (`library.css`, not mine): proposed row below.
2. The bars scroll inside themselves, so a tap area below the bar is clipped unless the bar has the padding.

## FINDERS (rule 13)
`git grep -n` in `e2e/` and every `__tests__` for `sectionbar-link`, `lib-sectionbar`, `cin-sectionbar`, `shv-empty-links`, `arrowLink`, `Explore all`, `library.css`, `cinema.css`, `shelves.css`, `Editorial.module`: no CI spec or test drives or reads any of them; the only hits are earlier waves' shots specs (read only) and a comment in `p79-cinema-shots.spec.ts`. The `.cin-link-btn` and `.cin-spoiler` rules were not changed. Nothing in `scripts/` or `packages/` reads these files as text. A unit test of `packages/ui` for ArrowLink (`display.test.tsx`) checks the DOM, not the sheet.

## JOBS
| id | class | submitted | started | ended | rc | note |
|---|---|---|---|---|---|---|
| 0286 | shoot base | 1795055900 | 1795055903 | 1795055973 | 1 | first measure; my spoiler read was wrong (spec) |
| 0290, 0294, 0300, 0305, 0312, 0316, 0319, 0321 | build | see `cli-agents/run/jobs/<id>/` | | | 0 | each after a commit |
| 0291, 0295 | shoot base | 1795056104, 1795056403 | | 1795056365, 1795056507 | 0 | base figures; 0295 holds the final base (natural line height) |
| 0296, 0301, 0309, 0314, 0318 | shoot after | | | | 1 | problem lines, each read: 0296 bar areas had no height (so the 5 px / `0 -5px` fix), 0301 span@43 failed (border), 0309 and 0314 Library Home "Friends" regression, 0318 my own check flagged the left-out link |
| 0306 | shoot after | 1795056828 | 1795057044 | 1795057045 | 3 | stale build: a small job passed the queued build; I waited for the build before the next submit |
| 0320, 0322 | shoot after | 1795057879, 1795057998 | | 1795057930, 1795058057 | 0 | no problem lines; 0322 is the final one (shots) |
Waiting was up to 4 minutes (the queue); running 25 to 70 s.

## Shots (opened after job 0322; `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-sections`, base twins `base-`)
- `bar-library-375.png`: Library Browse, bar at the top with six links, the Reading link underlined gold, the page 4 px lower than in `base-bar-library-375.png` (checked side by side: the links are at the same place); no crowding.
- `bar-cinema-375.png`: Cinema Movies, bar scrolled to Media (underlined), same look as the base; content 4 px lower.
- `bar-library-1024.png`: Library at 1024 x 768, the bar across the top ("SIDE ROOM / The Lounge ... Reading"), sidebar and body as before; figures equal to the base.
- `cinema-home-375.png`: Cinema home scrolled to "Recently added": the "Explore all" link at the right of the heading with its arrow; no change in drawing (the area is invisible).
- `cinema-discover-empty-375.png`: Cinema Discover with its API answered empty: "Nothing here yet" and the two underlined links, drawn as in the Library's twin (`library-empty-375.png`, `base-library-empty-375.png`).
- `cinema-comment-375.png`: Comments tab: "This contains spoilers. Show spoiler" row, then "Like (0)" and "Reply", the next comment; nothing crowded. `cinema-comment-revealed-375.png`: the spoiler text shown on two lines, Like and Reply under it.

## Who imports my files (which chunk; gzip, base to after, from `build`)
- `library.css`: `LibraryRoot.tsx` (lazy chunk `LibraryRoot-*.css`): 5 977 to 5 987 bytes.
- `cinema.css`: `CinemaRoot.tsx` (lazy `CinemaRoot-*.css`): 4 501 to 4 551.
- `shelves.css`: `ShelfEmpty.tsx` and the shelves pages (lazy `shelves-*.css`; Library's chunk also carried the old rule): 850 to 907. The rule left `LibraryRoot`'s sheet.
- `Editorial.module.css`: `Editorial.tsx` in `@sideroom/ui`, in the app's main sheet `app-*.css` (first paint): 8 393 to 8 440 (+47 bytes gz). The budget is not a blocker; it is a kit rule.
- Rendered `ArrowLink` / `.arrowLink`: Cinema Discover (the banner link and every PosterRow's "Explore all"), Library Home (6 links). `ShelfEmptyState` renders in Cinema Discover and Library Browse only (the `music` kind has no caller).

## Checks
- `npx prettier --check` on my 6 files: pass (22:14), exit 0.
- `node scripts/check-css-tokens.mjs`: "73 file(s) clean", exit 0 (22:12).
- e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 before each submit.
- vitest, eslint, web tsc, i18n audit, doc-link check: not run (left to the commander).

## Closing texts
- SEC-AUD-530 (Library, Cinema): "Closed in wave 109 (D-1022, D-1025): on a phone the Library's and the Cinema's section bar links get a tap area that reaches 43.5 px from the top of the screen, and the bars get 4 px of padding below the links (the content under them moves down 4 px); the links are drawn as before. 12 of 12 links pass the hit test with the edge rule. Handoff `fix-p109-sections.md`."
- SEC-AUD-531: "Closed for the Cinema in wave 109: `.arrowLink` gets a 44 px tap area on a phone in the kit sheet (3 of 3 Cinema 'Explore all' links pass; 4 of 5 Library Home links go from fail to pass). One Library Home link (see rankings), which sits directly under a segmented control, is left out and still fails: needs a row gap in `.lib-section-head`."
- SEC-AUD-532: "Closed in wave 109: the empty-shelf links' tap areas moved from `.lib-frame` to `shelves.css`; Cinema Discover's empty state passes (2 of 2)."
- SEC-AUD-533: "Measured in wave 109, no fault: Show spoiler (44), Like and Reply (29) pass; the 44 px reveal button makes the spoiler line 10.4 px taller than it would be (limit 15). 'Contains spoilers' is text, not a control."

## Proposed new rows
- [QA] Low: Library Home, `.lib-section-head` (`library.css`): the segmented control ("Friends / People you know / Global") sits 1 to 5 px above "See rankings" in a wrapped head; the two 44 px zones overlap (taker pair: the link's above point on the control's label). Needs a row gap of about 12 px on the phone head, then the `:not([role='radiogroup'] + .arrowLink)` in `Editorial.module.css` can go.
- [QA] Low: Library Home footer links (About, Help, Top contributors, Image credits, Privacy, Terms; 16 px tall, in running rows 24 px apart) fail at 375 in the base and after (takers: containers and the next row's links); not in this wave's rows.

## Changelog sentence
On a phone the Library's and Cinema's section bar links, the Cinema's "Explore all" links and the empty-shelf links now have a 44 px tap area.

## Owner questions
None. (Default taken: the bars grow 4 px at the bottom rather than the links growing, so the drawn links do not change.)

## PROCESSES
strays: none found (22:14)
