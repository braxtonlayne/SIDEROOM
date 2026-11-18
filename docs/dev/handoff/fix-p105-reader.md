# Wave 105, reader (D-1001; SEC-AUD-486, 487, 488)

## Clock
Start 15:30. Spec committed 15:33. Base shoot 0025 15:33-15:36 (figures read). CSS written and built 15:37-15:40. Base re-run 0037 (spec fix: the order of grounds for the select) with build 0038, then after 0047 done 15:45. Handoff 15:47. Nothing queued.

## Done (commits on `fix/p105-reader`)
- `D-1001 reader shots spec`: `e2e/tests/p105-reader-shots.spec.ts` (new).
- `D-1001 library.css ...` (the select-ground fix in the spec is in the same commit): three blocks at the very end of `library.css`. The D-998 blocks and `.lib-icon-btn` are untouched; `Reader.tsx` unchanged; no string, no other file.
  1. 487: `@media (max-width: 480px) { .lib-reader-top .lib-icon-btn, .lib-reader-bottom .lib-icon-btn { min-block-size: 44px } }`.
  2. 486: `@media (max-width: 360px) { .lib-reader-top label.lib-chapter-select { min-inline-size: 56px } }`.
  3. 488: under `.lib-root[data-theme^='sideroom']` with a white or sepia stage: `.lib-endcard :is(.lib-eyebrow, .lib-btn:not(.lib-btn-gold)) { color: inherit }`, plus `--sr-bg-2: rgb(0 0 0 / 0.08)` on the card (the buttons' hover ground: the dark `--sr-bg-2` would sit under dark ink). Two rules, not one, because of the hover.
  The stage defines no ink TOKEN: `.lib-reader[data-bg='white'|'sepia']` set `color` directly (`library.css:933-940`). The card inherits that colour and the back link and the rating label already read it, so I took the stage's `color` by `inherit` rather than restating hex values.

## Not done
Nothing left out. The 96 px floor at `library.css:1756` is left as wave 104 wrote it; my 360 block comes later and wins.

## Step 1, confirmed (base, 0025 then 0037; shipped theme `sideroom`)
- Bars at 375: every bar icon button 36 x 36 (x 16, 191, 235, 279, 323); top bar 60 tall, bottom bar 72 tall (the slider is 56); picker 115; scroll 375/375. Hit test (default reach): top 1 pass / 5 fail, taker `header.lib-reader-top` above and below each; bottom 1 pass, 1 skipped (disabled previous), 1 fail (next chapter, taker `footer.lib-reader-bottom`).
- Bars at 320: top bar scrollWidth 340 against clientWidth 320 at base (picker at its 96 floor; settings button ends at 340); same fails.
- End card ratios at base (1024 and 375 identical). Black: eyebrow 9.9, next 8.2, back 16.9, comments 16.9, rating label 16.9, select 16.9. Grey: 5.4, 8.2, 9.1, 9.1, 9.1, 9.1. White: **eyebrow 1.9, comments button 1.1** (light Side Room text on the light stage); back link and rating label 15.5; next 8.2; select 13.6 (a dark field with light text). Sepia: **eyebrow 1.6, comments 1.1**; back and rating 9.6; select 13.6. `lounge-light` white: eyebrow 5.7, next 4.6, comments 11.2, select 11.4 (the theme's own dark tokens read on a light stage). The first base run (0025) showed the select at 1.1: the spec laid the stage over the field in the wrong order; fixed and re-run (0037). The rating field is not a fault.

## Figures against the task's two lists (`compare.json`; last run `problems: []`)
May change, and did:
- At 375 and 320: every bar icon button 36 -> 44 tall; y 12 -> 8 (top bar), 758 -> 754 (bottom bar); x and width unchanged.
- At 320: picker 96 -> 60 (the room left; floor 56); the four buttons after it x 172, 216, 260, 304 -> 136, 180, 224, 268. Bar scrollWidth 340/320 -> 320/320; nothing past the viewport.
- End card under `sideroom` on white: eyebrow 1.9 -> 15.5, comments 1.1 -> 15.5; sepia: eyebrow 1.6 -> 9.6, comments 1.1 -> 9.6. Every other part: colour and background equal.

Must not change, all held within 0.5 px: both bars' heights at 1024 (56 / 60), 1280, 375 (60 / 72) and 320 (60 / 72); every button's width (36); the bars at 1024 and 1280 (every child's x, y, w, h, top and bottom); at 375 the picker 115 and the title; every box of the end card on all four stages, in `sideroom` at 1024 and 375 and in `lounge-light`; the end card's colours on black and grey and under `lounge-light`; `.lib-icon-btn` outside the reader (series page and Browse, 375 and 1024: the shell's menu and bell buttons, boxes equal). Hit tests after: 375 top 6 pass / 0 fail, bottom 2 / 0 (one skipped, disabled); 320 the same; 1024 and 1280 (box reach) 6/0 and 2/0, equal to the base. Picker at 320 shows "C" and its chevron, the chevron clear of the text.

## REACH
CSS only. Reached: `.lib-reader-top .lib-icon-btn` (back link, bookmark, comments, bookmarks, settings: `Reader.tsx:615-665`), `.lib-reader-bottom .lib-icon-btn` (previous, next chapter: `:763`, `:787`), `.lib-reader-top label.lib-chapter-select`, and in the end card (`Reader.tsx:878-896`) the `.lib-eyebrow` and the `.lib-btn` that is not `.lib-btn-gold` (back link, comments; `ChapterDownload` when the member is entitled, not in the seed). The p101, p102 and p104 library specs read `.lib-endcard` and the bars as containers only; none finds a button by its size.

## JOBS
| id | class | waited | ran | rc |
|---|---|---|---|---|
| 0025-reader-shoot | shoot base | about 125 s | 72 s | 0 |
| 0037-reader-shoot | shoot base (spec fix) | about 121 s | 76 s | 0 |
| 0038-reader-build | build | 9 s | 20 s | 0 |
| 0047-reader-shoot | shoot after | about 171 s | 74 s | 0 |

(`cat cli-agents/run/jobs/<id>/{submitted,started,ended,rc}`.) The `sr-cli staff appoint` line in each log is the non-fatal line earlier specs print too. No tsc job: no `.ts` or `.tsx` under `apps/web` changed.

## Shots (all opened after the jobs ended; `data-theme` read back by the spec: `sideroom`, and `lounge-light` for the last)
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-reader`.
- `base-reader-bars-375.png`, `reader-bars-375.png`: look the same: back arrow, picker "Chapter ..." with chevron, bookmark, comments, list, settings, all whole; bottom bar with slider. The taller targets do not show.
- `base-reader-bars-320.png`: the page is scrolled sideways, the arrow cut at the left, settings cut at the right. `reader-bars-320.png`: the whole bar: arrow, picker showing "C" and its chevron, four buttons, nothing cut.
- `reader-bars-1024.png`: title left, picker 220 wide with the whole chapter name, four buttons; as the base.
- `base-reader-end-white-1024.png`: pale gold eyebrow barely visible on cream, the comments button's text and icon near invisible. `reader-end-white-1024.png`: eyebrow dark, "0 comments" dark with its icon, the rest unchanged.
- `reader-end-sepia-375.png`: eyebrow and comments button dark on sepia, readable; gold Next button, back link and dark rating field as before.
- `reader-end-default-1024.png` (black stage): gold eyebrow, light text, as at base. `reader-end-light-1024.png` (`lounge-light`, black stage): light text and gold eyebrow on black as wave 104 left it.
- Also taken, not needed for the verdict: `reader-end-white-375.png` and the `base-reader-end-*` twins.

## Who imports
`library.css`: `LibraryRoot.tsx:9`, the Library chunk (lazy, not first paint). `Reader.tsx` unchanged (Library chunk through `routes.tsx`).

## Checks (on the last commit)
- `npx prettier --check` on `library.css` and the spec: clean (15:40).
- e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (15:40, after the last spec edit).
- `node scripts/check-css-tokens.mjs`: exit 0, 73 files clean (15:40).
- Web eslint, web tsc, vitest: not run (no app `.ts`/`.tsx` changed; the web eslint config ignores `e2e/`).

## Closing texts
- SEC-AUD-486: fixed. At 360 px and under the chapter picker's floor is 56 px; at 320 the top bar is 320/320 with no control past the edge.
- SEC-AUD-487: fixed. At 480 px and under the reader's bar icon buttons are 36 wide and 44 tall (`min-block-size`, scoped to the two bars); neither bar changed height; hit test 0 fails on both bars at 375 and 320.
- SEC-AUD-488: fixed for Side Room on a white or sepia stage: the eyebrow and the comments button take the stage's ink (15.5:1 on white, 9.6:1 on sepia, from 1.9 and 1.1, and 1.6 and 1.1). The stage defines its ink as a plain `color`, not a token.

Proposed new rows: none.

## Changelog sentence
The reader's bar buttons are now 44 px tall targets on phones, the bar fits a 320 px screen, and the end card reads on the white and sepia stages.

## Owner questions
None (the Discord-like default is D-1001's: no second row, no menu).

## PROCESSES
strays: none found
