# Handoff: fix/p107-library (wave 107, code builder of pod sections)

## Clock
Started 18:37; all four sheets committed (ALL READY) at about 18:41. No job submitted (code builder).

## Done (commits)
- `01e473ea` D-1022 library.css (READY)
- `4457f146` D-1022 study.css (READY)
- `7c9eae9f` D-1022 arcade.css (READY)
- `6ad4ca02` D-1022 emu.css (ALL READY)
Phone rule used: `@media (max-width: 48rem)` in library.css and study.css (= the kit's MOBILE_QUERY 768 px that sets `.lib-frame[data-mobile]`; the Study's page rule is the same 48rem), `@media (max-width: 40rem)` in arcade.css and emu.css (the Arcade's own phone rule; the emulator page is entered from the Arcade and its sheet has none). Library rules use the media query and not `.lib-frame[data-mobile]` because the Filters drawer is a portal outside the frame.

## Not done
- `.lib-chip-list a` (series/review genre tags): accepted, not cured. The link is 16 px inside an `li` of padding 3 px + border 1 px, and the list gap is 8 px: link-to-link is 16 px between wrapped lines, a 44 px area needs 14 px each way (28 px). A tap area would overlap the next line's. Text-line target in a tag row. No change.

## REACH table (rules by selector, lines in this tree before my change)
| control | rule now | page, how opened | cure | what moves (phone only) |
|---|---|---|---|---|
| `.lib-genre-chip` (38) | library.css:608 (gap 12 in `.lib-genre-chips` :600) | Library Browse, Filters drawer (portal) | A.1, `::after` -3px | nothing; the last line's lower 3 px stop falling to the "Add a genre" label |
| `.lib-btn` (40) | library.css:349 | Series "Start reading", Library me/list empty links, review page | A.1, `::after` -2px, scoped `.lib-frame` | nothing (gaps 12 in `.lib-actions`) |
| `.lib-icon-btn` sort (36) | library.css:206 | Browse toolbar, `.lib-browse-tools` | A.1, -4px, scoped `.lib-browse-tools` | nothing (row is the 44 px select's) |
| `.lib-chip-list a` (16) | library.css:546 | Series, review | accepted | none |
| `.lib-linklike` Newest first (21) | library.css:329 | Series, Chapters tab | A.1, -12px, scoped `.lib-chapters-head > .lib-linklike` | nothing (head margin 16) |
| `.shv-empty-links a` (20) | storage/shelves/shelves.css:143 (not my file) | Browse, empty library | A.1, -12px, from library.css as `.lib-frame .shv-empty-links a`; plus `.lib-frame .shv-empty-links { row-gap: 24px }` | row gap 16 to 24 only if the two links wrap (they do not at 375: 118+16+91) |
| `.arc-cta` (34) | arcade.css:466 | Games lobby, Emulator card | A.1, -5px; `.arc-actions:has(> .arc-cta)` row-gap 14 | row gap 8 to 14 only when that foot wraps |
| `.emu-back` (19) | emu.css:31 | /games/emulator head | A.1, -13px | nothing (page top padding 32, head gap 8, title is not a control) |
| `.stu-rail button` (25) | study.css:89 | Study, Recently added rail | A.2, min-block-size 44 | rail +19 px |
| `.stu-crumbs button` (23) | study.css:116 | Study breadcrumb | A.2, min-block-size 44 | crumb row +21 px |
| `.lib-search-input` (40) | library.css:248 | every Library page top bar | B + 44 on phone | top bar +4 px on a phone; rest bottom line, hover, ring, placeholder at every width |
| `.emu-search` (40) | emu.css:62 | /games/emulator list | B + 44 on phone | field +4 px on a phone; border, hover, placeholder at every width (ring already there) |

Field notes: `.lib-search-input` is an underline field (no box), so the contract is applied to `border-bottom-color` (rest: `color-mix(... 65%)` over the kept `1px solid var(--sr-line)`, and `--sr-line` is `var(--ui-border)`); hover and ring as the kit. `.emu-search` keeps `outline: 2px solid var(--ui-focus); outline-offset: 1px` (already there). No `outline: none`.

## Other users of what I changed (D-990)
- `.lib-btn`: Home, Lists, Pages, SeriesPage, Submissions, useProgressSync, `.lib-epub .lib-btn`, `.lib-pdf-bar .lib-btn` and the reader's end card use it if they render inside `.lib-frame`: they get a 2 px tap area only; no box moves. sectionshots should hit-test the reader's end card and epub bar once.
- `.lib-genre-chip`: BrowseFilters (drawer, two uses) and Home.tsx:354 (genre links on Library Home): tap area only.
- `.lib-search-input`: SearchBox.tsx:131 only (top bar). `.lib-topbar` grows 4 px at <=768 px; not the Browse toolbar row.
- `.stu-rail button` / `.stu-crumbs button`: the grouped rule also covers `.stu-mounts button` and `.stu-folder`, which I did not touch.
- `.arc-actions` row-gap changes only where a `.arc-cta` is a direct child (one place, ArcadeSections.tsx:291).

## FINDERS (rule 13)
`git grep` of every class in `e2e/` and `apps/web/src` (non-tsx): hits only in retired shots specs (p78, p80, p81, p101, p102, p103, p105 library/arcade shots: they read, never drive these as controls with a changed interface; nothing drives a changed control by selectOption or role). No unit test reads library.css, study.css, arcade.css or emu.css as text. No CI spec (non-shots) hits. None to fix.

## JOBS
None submitted (code builder).

## Figures against the task's lists
Not measured (no job by rule; sectionshots measures). Expected: the "must not change" rows do not move by construction (all tap areas are pseudo-elements; only the Study rows, the two search fields and the top bar change box, on a phone).

## Shots
None (code builder).

## Who imports my files, which chunk
library.css: library/Shell chunk (lazy Library section; `git grep` import in library/Shell.tsx). study.css: Study page (lazy). arcade.css: Arcade section (lazy). emu.css: emulator page (lazy). None is first paint or a public page; I changed none that first paint loads.

## Checks (on my last commit)
- prettier --check on the four sheets: pass, 18:40.
- `node scripts/check-css-tokens.mjs`: pass (73 files clean), 18:40.
- Unit test files: not run (no unit test renders these sheets; CSS is not applied in vitest).

## Closing text for my rows / proposed new rows
Faults the base had too: `.shv-empty-links a` rule belongs to storage/shelves/shelves.css (no owner this wave): I cured it from library.css scoped to `.lib-frame`; the same empty state in Cinema or Music is not changed. `.lib-chip-list a` accepted (tag row gap 8). Section bar links (Library 39, Arcade 33, Study 20) are a probe limit, not mine.

## Changelog sentence
On a phone the Library's genre chips, buttons, sort direction, order toggle and empty-library links, the Arcade's Emulator link and the emulator's back link have 44 px tap areas, the Study's rail and breadcrumb are 44 px tall, and the Library and emulator search fields take the kit field's border, hover and placeholder.

## Owner questions
None.

## PROCESSES
strays: none found
