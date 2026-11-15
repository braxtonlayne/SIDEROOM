# Wave 79 handoff: library (SEC-AUD-315; D-817, D-816)

Branch `fix/p79-library`, base `aa3ff9cf`. Web only: no Rust, no migration, no generated file, no i18n fragment, no `.tsx` outside the two the task names.

## Done (commits)
- `76c55aca` and `5f21d64b`, subject `SEC-AUD-315`:
  - `library/library.css`: on a phone (`.lib-frame[data-mobile]`) the brand and the slash are hidden, the bar's webkit scrollbar is hidden, and `scroll-padding-inline: 16px` keeps the area off the edge. (Wave 78 had already made the phone bar `overflow-x: auto` with `scrollbar-width: none` and `flex: none` children: that part was in place on the base.) The reader's dark block (bars and sync notice) also states `--ui-accent`, `--ui-accent-hover`, `--ui-accent-ink`, `--lib-fill`, `--lib-fill-ink` with the Side Room values (D-816).
  - `library/Shell.tsx` (`SectionBar` only): a ref on the `<header>` and one effect that calls `scrollIntoView({ block: 'nearest', inline: 'nearest' })` (guarded) on the areas group's `[aria-current='page']` link, on mount, when the pathname changes, when the phone layout switches on (`useMediaQuery(MOBILE_QUERY)`, already imported) and once `document.fonts.ready` resolves (a later font widens the links). No new import. The page never jumped, so no `scrollLeft` fallback was needed.
  - `review/NamingGuide.tsx`: the results table is inside `<div className="rv-table-wrap">`.
  - `review/review.css`: `.rv-table-wrap` is `position: relative` (see "which rule fixed which element").
  - `e2e/tests/p79-library-shots.spec.ts` (new, local only: `SR_P79_LIBRARY=1`, `SR_P79_BASE=1` for the base case).

## Base case (run: yes, files held back with `git checkout aa3ff9cf -- <3 files>`, then restored)
Phone 375x812, shipped / light, `document.documentElement.scrollWidth` (clientWidth 375):
| page | base shipped | base light | after shipped | after light |
|---|---|---|---|---|
| submission page with a chapter | 494 | 536 | 375 | 375 |
| submissions list with the naming table open | 679 | 716 | 375 | 375 |
| Library home | 375 | 375 | 375 | 375 |
| Browse | 375 | 375 | 375 | 375 |
| series page | 375 | 375 | 375 | 375 |
(Home, Browse and the series page did not overflow on the base: the bar already scrolled inside itself since wave 78. The faults were the two tables.) Elements past 375 on the base: `.lib-sectionbar-nav` and its links inside the bar (clipped by the bar, not the cause), `.rv-table` (naming table), and on the submission page the `SPAN.sr-only-label` at x 494 / 536. On the base "Reading" sat at x 470..516 (outside the viewport) on load; after: x 314..360 shipped, 310..359 light (inside, with the 16 px scroll padding).
Desktop 1280x800: scrollWidth 1280 everywhere before and after; the bar box `[0,0,1280,41]` and the submission page's table `[290,709,941,108]` (shipped) / `[290,703,941,105]` (light) are identical on the base and after (`measure-base.json`, `measure-after.json` in the shots folder).

## Which rule fixed which element
- Naming guide table (679/716): the `.rv-table-wrap` wrapper in `NamingGuide.tsx` (the wrapper scrolls inside itself: 633 content in a 297 box).
- Submission page (494/536): not the bar and not the table (its own wrapper at `Submissions.tsx:663` already scrolled, 543 in 309). The culprit is the `.sr-only-label` span in the table's cells: `position: absolute` with no positioned ancestor, so it sat at its static position past the wrapper's clip (x 494). `position: relative` on `.rv-table-wrap` makes the wrapper its containing block and clips it. This also removes the 14 px of wave 78's file input note. After it, the file input is inside its panel (`right == panelRight` 267 / 301 on a phone, 674 of 756 on desktop; asserted in the spec).
- Bar: brand and slash hidden, area scrolled into view with padding.
Contract change to flag: the task listed `review.css` "only if something is still wider"; this one rule was needed.

## Reader finding (item 2)
On the base in the light Lounge theme the reader's bar read `--ui-accent` = `#5865f2` (blurple); after, `#c2b08f` on both desktop and phone. The kit slider's thumb and the page count on the bars are the parts that read it (shots `reader-*-light.png`: cream thumb, gold count).

## Imports (no web build run)
`git grep`: `library/library.css` is imported only by `LibraryRoot.tsx`, a `lazyNamed` import in `library/routes.tsx`; `Shell.tsx` only by `LibraryRoot.tsx`. `review/review.css` and `NamingGuide.tsx`: Admin `IngestSources.tsx` (Admin chunk), `library/Submissions.tsx` (lazy), `cinema/submit/SubmitPage.tsx` (Cinema chunk). None is first paint or a public page. No web build, no bundle/entrance/staff checks.

## Cinema submit page and Admin > Review at 375
Both: scrollWidth 375 (the Cinema submit page's naming guide was collapsed in the shot, so its table was not drawn; the wrapper is harmless there). The Cinema's own bar (`.cin-sectionbar`) still has its links at x up to 518 inside its bar on the base: that is cinema's item (it uses its own rule). Admin's tab strip clips "Ab..." at the edge: that is the kit Tabs item.

## Tests run
- Single vitest: `review/__tests__/naming.test.tsx` (1 pass), `library/__tests__/chrome.test.tsx` (5 pass). No vitest renders `SectionBar`'s Library version otherwise.
- Web `tsc` and e2e `tsc` clean; eslint on `Shell.tsx`, `NamingGuide.tsx` clean; prettier --check on all five changed files clean; `i18n-audit --max 0` TOTAL 0; `check-css-tokens` clean.
- Shots spec: base run (`SR_P79_BASE=1`, on the base files: faults present, figures above) and four runs after; the final run passed (rc 0) with these assertions: scrollWidth <= clientWidth on every page, brand hidden and Reading inside the viewport on the phone, input inside its panel, the table inside `.rv-table-wrap`. The first three after runs failed on the real findings (the hidden label, then Reading cut by 1 px and by 12 px before the scroll padding and font re-centring): that is how the extra rules were found. Click-through done in the spec (phone, light): "Shelves" pressed and back, the naming table scrolled sideways in its wrapper, the bar dragged to its end and start, the chapter input found and inside its panel. I did not run `setInputFiles` on the chapter (wave 78's spec did it; not repeated).

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-library`: 34 PNGs (phone, each shipped and light: submission top, file input, naming table, table scrolled, home, Browse, series, bar start, bar end = 18; desktop, both themes: submission top, input, naming table, home, Browse, series = 12; the reader in light, desktop and phone = 2; the Cinema submit page and Admin > Review at 375 = 2). I opened and looked at all 34. `visual-sweep-shots/p79-library-base`: the 34 base shots (I opened one, `naming-table-phone-shipped.png`: the dark strip and cut table). Shipped (`sideroom`) and light (`lounge-light`) set explicitly for `library` and `cinema` section themes, `data-theme` read back from `.lib-root` before each library shot.
Observed, not changed: in the phone naming table the "Read as" column wraps word by word (a 633 px table in a 297 px wrapper; fine inside the wrapper but cramped). A `min-inline-size` on `.rv-table td` would help; left alone (review.css contract was "only what is needed").

## Not done
The tail (18+ card, the reader's settings dialog): not started (the lock came at 15:34).

## Docs
Searched `docs/`, README: only historical handoffs mention the section bar or the naming guide. Nothing to update.

## Proposed inventory row
SEC-AUD-315 (library, closes if landed): "No Library page scrolls sideways on a phone (D-817): the naming guide table sits in `.rv-table-wrap`, the wrapper is the containing block of the table's hidden labels, the phone section bar hides its brand and slash and scrolls the area into view; the reader's bars restate the accent (D-816)." Closed.
Threat-model rows: none.

## Changelog sentence
Phone: Library pages no longer slide sideways (the naming guide's table scrolls inside itself) and the bar at the top shows the area you are in; the reader's bars keep the gold accent under every theme.

## Owner questions
- Should the review table's "Read as" column get a minimum width so a phone does not wrap it word by word? (Default: leave.)

## Housekeeping
Stack stopped by PID (sr-server 37492, vite 31432), lock released. The scratch database `sr_p79_library_ui` is dropped and `p79-library-bin` deleted. No stash touched. Untracked scratch only in the scratchpad (`scratchpad/library`).
