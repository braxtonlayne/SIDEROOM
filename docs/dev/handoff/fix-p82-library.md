# Wave 82 handoff: library (SEC-AUD-339; D-843)

T = 11:10 on 2026-11-16. Clock times: no new item from 13:10 (T+2h), checkpoint 13:30, finished 13:55, hard limit 14:10. Actual (`date`): started 11:06; lock requested 11:10 (developers held it from 11:11, so I waited and kept retrying `acquire`); lock held 11:28 to 11:39 (one round: base run, fix, after run, stack stopped); vitest, lint, handoff 11:40 to 11:50. One round only.

Branch `fix/p82-library`, base `8818b4e8`. Web only: no Rust, no cargo, no build, no migration, no generated file, no new string.

## Imports (no web build)
`epub/settings.ts` and `epub/EpubReader.tsx` are imported by `library/reader/Reader.tsx` (lazy: `library/routes.tsx:15`, `lazyNamed`) and by `storage/shelves/PresignedReader.tsx` (reached by `ShelfItemPage`, lazy in `storage/shelves/routes.tsx:7`, and by `study/StudyDialogs.tsx` in the Study chunk). Nothing first paint or a public page loads: no build, bundle, entrance or staff check. Side effect, intended: the Study and Storage shelves' EPUB viewers get the same fix (same component).

## What I saw on the base (run: yes)
Spec `e2e/tests/p82-library-shots.spec.ts` (local only, `SR_P82_LIBRARY=1`; `SR_P82_BASE=1` is the base case; `settings.ts` held back with `git show HEAD:...`, my fixed copy kept in a scratch folder and put back after). `attachShadow` is forced open by an init script in the spec so the renderer's closed shadow root can be walked.
- **Opened in Dark (the reader theme saved before the page loads), light Library, paginated, 1280 and 375: no band.** Pixel lines along the top, bottom, left and right of the pane and down the middle (the gap) are all exactly the theme's `bg` (max deviation 0). So the fault is not "Dark always has a frame".
- **The band appears when the theme is changed in the settings panel while a section is open** (wave 81's flow: open in Light, choose Dark; the usual way to reach Dark). Base, light Library, panel closed, 1280 and 375: Light to Dark: top, bottom, left, right all deviate by 233 (white, 255 against `#16161a`); the gap too. Dark to Sepia: 39 (white against `#f4ecd8`). Then Light again: 0 (back to the colour the section had at load). It stays through page turns inside the same section (375: still 233 after three forward turns and one back; the book is one section there); it goes away when a turn loads another section (1280: after turn 1 the sample is 0).
- **The node that paints the band:** the renderer's (`<foliate-paginator>`, closed shadow root) `div#background` (`part="filter"`), which holds one `div` per column, each with an inline `background` set by `#replaceBackground` in `paginator.js`. The value is the book document's computed background **at the moment the section loaded** (`docBackground`), so after a live theme change the inline value is the old theme's. The walk on the base after the change: the two column divs (`box=0,55,640,691` and `640,55,640,691`) read `rgb(255, 255, 255)` while the iframe's `html` and `body` and the root `.lib-epub` read `rgb(22, 22, 26)`. The `#background` grid spans the whole element (`grid-column: 1 / -1; grid-row: 1 / -1`), which is why the band is on all four sides and in the gap. Not the host, not `.lib-epub-view`, not the iframe's `color-scheme`: the walk shows `.lib-epub-host`, `foliate-view`, `foliate-paginator`, `#top`, `#container` and the iframe element transparent. Paginated flow only: the scrolled flow draws no columns (`columnCount` is never set there) and the container scrolls over the root's own ground: no band on the base (scrolled, Dark, 1280 and 375: 0; the scrolled flow's bottom line crosses text, so the spec excludes it).
- Sepia: same mechanism (39 above).

## Which fix held
Fix (3), `settings.ts`'s `epubCss`. (1) cannot hold: `::part(filter)` reaches the `#background` div, but the columns are its children with inline backgrounds, and hiding the part would also hide the iframe (it carries `part="filter"` too). (2) cannot hold: the paginator reads no attribute or property for the ground; the only hook it reads is a CSS custom property of the **book's document**, `--theme-bg-color` on `<html>`, read from the computed style by `#replaceBackground`, which substitutes it into the stale background value each time the styles change (`setStyles`, then the next frame). So `epubCss` now states `html { ...; --theme-bg-color: <themeColors(theme).bg>; }`. One source (`themeColors`), no second literal.

## After (run: yes, 1 passed in 2.5 min)
Every sampled line is within 2 per channel of the theme's `bg` (max 0 in every one). Pixel table (max deviation over the lines top, bottom, left, right, gap):

| reader theme | opened with it (base and after) | changed live, base | changed live, after |
|---|---|---|---|
| Light | 0 | 0 (the stale white equals Light) | 0 |
| Sepia | 0 | 39 | 0 |
| Dark | 0 | 233 | 0 |

After, all combinations: Light, Sepia and Dark, shipped (`sideroom`) and light (`lounge-light`) Library, 1280 and 375, paginated and scrolled: 0 (scrolled bottom line excluded: text). Live click-through (light Library, 1280 and 375, settings panel closed before each sample): Light, Dark, Sepia, Light, Dark: 0 each time, no reload; then three pages forward and one back in Dark: 0 each time (labels at 1280: `The lamp 33%, The margins 67%, The end 100%, The end 100%, The margins 67%`; at 375: `The end 78%, 89%, 100%, 100%, 89%`, the phone's book resumed at the saved position). 100 ms after "Next" (clicked until the section changed, sampled 100 ms after each click, Dark, light Library, both sizes): worst 0. **Light equals the base:** Light's lines are 0 on both. **Text column and margins did not move:** the first paragraph's box (page coordinates) paginated, base against after: 1280 `[64,190.5,544,51.2]` both; 375 `[19,191.2,337.5,76.8]` both; the pane's ink bounding box is identical too (`[63,48,1211,584]`, `[19,78,356,646]`). The scrolled flow's first paragraph differs in y only (1280: 155.1 base, 127.1 after; x and width equal, 280 and 720): the chapter's saved reading position carries over between runs (the scrolled 375 shot opens at "The end 67%"), so the scroll offset differs. I did not re-run the base's scrolled flow to prove it; nothing in the fix touches layout or scroll.

Caveat on my first spec: samples taken with the settings drawer open read its scrim (values like 93 and 150), not the pane; the final spec closes the panel (Escape) before sampling. The first live numbers in my scratch logs are not evidence.

## Tail (done): the table of contents and settings panels over a Dark book in a light Library
`after-toc-panel-dark-desktop-light.png` and `after-settings-panel-dark-phone-light.png`: both panels are light panels (the Library's theme) over the dimmed dark book; the title, the three contents links and the settings controls are whole and readable (dark text on white, accent segments); nothing cut at 375. Findings only: nothing to change (the panels follow the Library's theme and the book follows the reader theme, by design).

## Shots (all opened by me)
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-library`: 26 PNG and `measure-after.json`: paginated Dark, Sepia, Light, shipped and light Library, 1280 and 375 (12); scrolled, Dark, light Library, both sizes (2); the live change in the light Library, both sizes: Dark, Sepia, Light, Dark (8); 100 ms after the turn that changed section, both sizes (2); the tail's two panels (2). `visual-sweep-shots/p82-library-base`: 12 PNG and `measure-base.json`: the base's Dark 1280 shot (1), the base live change (8; the white frame is in the Dark and Sepia ones), the base after-Next (2), the settings panel (1). **Total 38 PNG, all 38 opened.** `data-theme` is read back from `.lib-root` and asserted before every sample (matches the section theme in every one), and `scrollWidth <= clientWidth` is measured and asserted before each (all pass at 375 and 1280).

## Tests run
- vitest `library/__tests__/epub.test.tsx`: 4 passed (new case `hands the reader theme background to the renderer through --theme-bg-color`, one assertion per theme against `themeColors(theme).bg`). I did not run that case against the base file; the e2e base run is the failing-without-the-change evidence (233 and 39 above).
- web `tsc --noEmit` clean; eslint (the package's own) clean on `settings.ts` and `epub.test.tsx`; prettier clean on both and on the spec; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; `node scripts/check-css-tokens.mjs` clean; `node scripts/check-doc-links.mjs` ok; e2e `tsc` clean.
- the shots spec: base run (1 passed) and after run (1 passed, 2.5 min); my first two runs were skipped (a leftover wave 81 skip line, fixed) or measured the drawer's scrim (above).

## Docs
Searched `docs/` for the EPUB reader's theme or background: nothing states the fault or the fix. Nothing to update.

## Proposed inventory row
- SEC-AUD-339: "The EPUB reader is one ground (D-843): changing the reader theme (Light, Sepia, Dark) in the settings panel while a book is open no longer leaves a band of the previous theme's colour around the book (margins, gap between columns); the book's stylesheet states `--theme-bg-color`, the one property foliate's paginator reads for the ground behind the text columns (`library/epub/settings.ts`, vitest case in `epub.test.tsx`, spec `p82-library-shots.spec.ts`)." Closed. Threat-model rows: none.

## Changelog sentence
The EPUB reader no longer leaves a band of the old colour around the book when you change its Light, Sepia or Dark theme while reading.

## Owner questions
None blocking. A note for the commander: wave 81's finding said Dark always showed a white frame; on the base it appears only after a live theme change (a book opened already in Dark, or after a turn into another section, is clean), so the fix covers the case the owner reaches by choosing Dark in the panel. Suggested sentence for `docs/10-design-system-and-theming.md` (D-843), if wanted: "The EPUB reader's ground is the reader theme's background, margins and column gap included; the book states it as `--theme-bg-color` for foliate's paginator."

## Housekeeping
Stack stopped by PID (found by port: sr-server 35100, vite 38220), lock released, `sr_p82_library_ui` dropped, `p82-library-bin` and the scratch folder `p82-library-scratch` deleted, `p80-base-bin` untouched. No stash touched. `git status` before the commit showed only my files.
