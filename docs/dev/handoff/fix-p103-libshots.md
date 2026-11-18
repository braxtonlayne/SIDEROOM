# Wave 103 handoff: libshots (shots builder, pod library; SEC-AUD-465, 468, 469; D-972, D-974)

Spec: `e2e/tests/p103-library-shots.spec.ts` (mine, new). No app file, no sheet, no other spec touched.

## Clock and rounds
- Started 11:56 (session clock). Spec first committed 12:02. Binaries copied to `p103-libshots-bin`.
- Merges into `fix/p103-libshots`: `fix/p103-shelves` (12:06, its ALL READY) and `fix/p103-reader` (12:10, its ALL READY at `51a963a2`); both carry the kit merge.
- Queue: `heavy.sh acquire libshots` asked at 12:02, given at 12:38 (both slots were held by builders with no round yet).
- **STACK-ROUND 1** (base and after in one round, one database `sr_p103_libshots_ui`, ports 8812/5812/9512): slot `libshots 12:38`, released 12:49 (`free` after). Seed `1 passed`, no `FAIL` line.
  The base ran on the merged tree with the pod's 11 app files put back to the base's text by `git checkout 49ca8946 -- <files>` (never a stash), then restored by `git checkout HEAD -- <files>`; `git status` showed only the spec afterwards.
  The base ran twice: the first run found two spec errors (the bottom slider's +-21 px point lies off the page at 1024 x 768; a base overlap I had counted as a problem). Final base: `problems: []`, `1 passed (1.6m)`.
  The first after run listed 30 "problems", all spec errors (the expectation "an already-kit select is not wrapped" was wrong; browse-sort 176 -> 160 px is the 16 -> 14 px type of D-974; the keyboard compare used ids that change per run). Fixed; the after run then had `problems: []`, `1 passed (1.7m)`, ending 12:49.
  Afterwards I read all 60 shots and found: FAULT 1 below (the spec's text-room check was added after the round) and a spec error in the filter drawer's Rating select (it matched the toolbar's sort select behind the drawer; the spec now finds the drawer's selects inside the dialog). Both are in the spec now and have not been run on a stack yet.
- **Round 2 (after reader's cure, with the drawer fix): not run when I handed back** (see "Round 2" at the end).

## What the spec does
Eight pages (Browse toolbar, Browse filter drawer, series page, review form, Rankings, My library, new list, edit list) plus the Reader's bars and end card, in three configs: 1024 x 768 and 375 x 812 touch in `sectionThemes.library = 'sideroom'` (the shipped theme), and 1024 in `'lounge-light'`; `data-theme` is read back from `main`'s ancestor on every page and asserted.
Data it makes through the admin API under `p103-lib-<digits>` names (D-942), removed in a `finally`: one series, edition "Official" with three chapters (the second with a long title so the picker's widest option passes 220 px), edition "Fan edition" with one chapter, one private list.
Selects are found by label (`getByLabel` narrowed to the `select` tag; inside the dialog for the filter drawer), never by class; roots are xpath ancestors. Figures per select: box, wrapper box, label box and lines, computed font-size, padding, the width of the selected text (canvas), the room beside the chevron and in the text area, the chevron's colour and its ratio (`contrast.ts`) on the select's ground and on the bar's, the root's neighbours and its row, new overlaps, `scrollWidth` against `clientWidth`, the hit test (`openCollapsed`, `hitTestAll`) with takers, and the keyboard drive (one select per file at 1024).
Reader (SEC-AUD-468): a `mousemove` on the top bar every 400 ms keeps the bars shown (they hide 3000 ms after the last poke); `barsShown` reads opacity and pointer-events of both bars before and after the hit tests (true in all three configs); the VIEWPORT is shot, not a clip; `hitTestAll` over the top bar and over the bottom bar returned no "not reached" row (the slider's point 21 px below its centre is off the page at 1024 x 768: that row is re-tested with `reach: 12` and recorded in `offPage`; at 375 it is reached normally).
Phases: `SR_P103_LIBRARY_PHASE=base|after`; after compares when `figures-base.json` is in the folder and writes `compare.json`. A moved select must be the kit's after (`moved: true` in the spec's list); a select that was already the kit's must be unchanged in box (width may shrink by the text-width difference of the 16 -> 14 px change).

## TYPECHECK
- e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`, after the last spec edit (13:00): exit 0. prettier `--write` on the spec: clean. eslint does not cover `e2e/` here (the app package's eslint ignores it as outside its base path): not run.
- STEP TYPECHECK (web, `tc.sh`) on the merged tree: not run by me: I changed no `.ts` or `.tsx` under `apps/web` or `packages`; shelves and reader ran it on their trees.

## Table: every select and dependant of the pod (base -> after; 1024 sideroom / 375 touch / 1024 lounge-light)
| select | file | builder | reached | fit | 1024 | 375 | light 1024 | verdict |
|---|---|---|---|---|---|---|---|---|
| Edition | SeriesPage.tsx | shelves | yes | content | 164x36 -> 172x40 | 164x36 -> 172x44 | 175x36 -> 183x40 | pass |
| Your rating | SeriesSocial.tsx | already the kit | yes | fill | 110x40, unchanged | 110x44, unchanged | 112x40, unchanged | pass; the rating row grew 0 px |
| Rating with this review | SeriesSocial.tsx | shelves | yes | content | 102x36 -> 110x40 | 102x36 -> 110x44 | 104x36 -> 112x40 | pass; label not stretched, 14 px |
| Rankings Kind | Rankings.tsx | shelves | yes | content | 108x36 -> 116x40 | 108x36 -> 116x44 | 112x36 -> 120x40 | pass |
| My library Sort by | Pages.tsx | shelves | yes | content | 152x36 -> 160x40 | 152x36 -> 160x44 | 158x36 -> 166x40 | pass |
| Lists visibility (new) | Lists.tsx | shelves | yes | content | 101x36 -> 109x40 | 101x36 -> 109x44 | 105x36 -> 113x40 | pass |
| Lists visibility (edit) | Lists.tsx | shelves | yes | content | 101x36 -> 109x40 | 101x36 -> 109x44 | 105x36 -> 113x40 | pass |
| Browse toolbar sort | Browse.tsx | dependant (kit already) | yes | fill | 176x40 (16 px) -> 160x40 (14 px) | 176x44 -> 160x44 | 183x40 -> 166x40 | pass: 14 px (SEC-AUD-469); toolbar 92 -> 92 px (0 taller); one line; at least 152 px |
| Browse filters Status, Age | Browse.tsx | dependant | yes | fill | 380x40, unchanged | 334x44, unchanged | 380x40, unchanged | pass |
| Browse filters Rating | Browse.tsx | dependant | spec error in round 1 | fill | not measured (matched the toolbar sort) | | | the spec is fixed; shown unchanged in the shots; to be measured in round 2 |
| Reader chapter picker | reader/Reader.tsx | reader | yes | content | 220x32 -> 220x40 | 220x32 -> 220x44 | 220x32 -> 220x40 | **FAULT 1** (type 13.33 -> 16 px, text cut) |
| Rate this chapter (end card) | SeriesSocial.tsx | dependant (kit already) | yes | fill | 110x40, unchanged | 110x44, unchanged | 112x40, unchanged | pass; row 0 px taller |

Hit tests (pass / fail over the whole section or dialog, base -> after): identical at 1024 for every select (the 44 px rule is a phone and touch rule, SEC-AUD-467: 36 to 40 px controls fail the +-21 px point at 1024 in both phases, noted not a fault). At 375 touch the moved selects' sections improved: series header 9/3 -> 10/2, review form 3/1 -> 4/0, Rankings 6/1 -> 7/0, My library 0/1 -> 1/0, new list 3/1 -> 4/0, edit list 4/1 -> 5/0, reader top bar 0/6 -> 1/5. No row is worse. The failing rows' takers are the bar or card itself (a 32 px control in a 52 px bar: the points above and below land on the bar).
Neighbours: the Reader's bar buttons moved only vertically (2 px at 1024, 4 px at 375, because the bar grew 52 -> 56 and 60 px), no sideways move. At 375 the picker is 220 px wide in both phases (wave 102's 218 -> 115 px collapse does not happen with the class cap). The series header grew 4 px (8 at 375) from the Edition field, the review form 4 (8), Rankings 4 (8), the new and edit list forms 4 (8); My library's head 0 at 1024 and 8 at 375; none newly wraps a label. Nothing scrolls sideways at either size (`scrollWidth` = `clientWidth` on every page).
Keyboard (1024, one per file): Edition, Rankings Kind, My library Sort, new-list visibility, review rating, Browse sort, Reader picker: `:focus-visible` true after a key, ArrowDown changed the value, ArrowUp put it back; the picker's ArrowDown went to chapter 2 and ArrowUp back; "the page reacted" is equal to the base in every case.
Chevron (SEC-AUD-468): the kit chevron is `rgb(169, 164, 147)` (`#a9a493`, `--ui-text-muted`) on the select's own ground `rgb(28, 29, 24)`: **6.8:1** in `sideroom` and **6.8:1 in `lounge-light` too** (the sheet's `.lib-root:not([data-theme^='sideroom']) :is(.lib-reader-top, ...)` override gives the bars the Side Room values, so the reader handoff's hand figure of 2.6:1 under `lounge-light` does not apply). On the bar's composite ground: 7.99:1. All above 3:1.

## FAULTS
1. **reader, `apps/web/src/library/reader/Reader.tsx` and `library.css` (`.lib-chapter-select`), the chapter picker: the type size grew from 13.33 px (a raw select's default) to 16 px (the kit's `font: inherit`, from the bar), so the selected text is cut with an ellipsis.**
   Figures (1024 and 375, both 220 px wide, the cap): "Chapter 1: p103-lib first" was 138.6 px at 13.33 px and is 170.1 px at 16 px (175.1 px in the `lounge-light` run) against a text area of 170 px (220 - 2 border - 12 - 36): cut. Shots: `reader-bars-1024-sideroom.png` ("Chapter 1: p103-lib fir..."), `reader-end-375-sideroom.png`, `reader-bars-1024-light.png`.
   Cure asked: **(b)** `font-size: 14px` on `label.lib-chapter-select` (the Library's field size, D-974; the kit select inherits; the text is then about 149 px, 21 px to spare). Never on `select` by element name, never on the wrapper. It needs a re-shot (round 2).
2. No other builder has an entry. shelves: none. kit: none (`fit="content"` keeps the chevron inside the box in every shot; no moved select is wider than its base by more than 8 px).

## STALE (earlier specs' expectations that this pod's changes make untrue; I edit none)
- `e2e/tests/p102-pages-shots.spec.ts`: finds the five moved selects and the picker by `.lib-edition select`, `.lib-select select`, `.lib-page-head select`, `form.lib-review-form select` and `.lib-chapter-select select`; those still match (element selectors under the wrapper), but any expectation of the old 36 px height or the old widths is stale (36 -> 40).
- `e2e/tests/p102-library-shots.spec.ts`: its sort-select figures are the wave 102 base (176 px wide, 16 px type); the sort is now 160 px at 14 px (its own limit "no narrower than 152" still holds).

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-library`): 60 PNG (30 `base-` and 30 after); I opened all 60, base beside after
- series-1024-sideroom, series-375-sideroom, series-1024-light (and base): header, Your rating, Edition. After: the Edition field is 40 px (44 at 375), its label reads 14 px (was 16), the label still touches the field (the base also had no gap); nothing wraps; "Your rating" identical.
- review-1024-sideroom, review-375-sideroom, review-1024-light: "Rating with this review" is a kit field, label 14 px (was 13), shrink-wrapped, not stretched; the form 4 px taller (8 at 375), the rows below moved by exactly that.
- rankings-1024-sideroom, -375-sideroom, -1024-light: Kind field 40 px (44), chevron inside the box, chips unchanged; at 375 what lies below moved 8 px.
- mylibrary-1024-sideroom, -375-sideroom, -1024-light: "Sort by" field 40 px in the head, label 14 px; the head did not grow at 1024; at 375 the chips moved 8 px.
- lists-new-1024-sideroom, -375-sideroom, -1024-light and lists-edit-1024-sideroom, -375-sideroom, -1024-light: "Who can see it" field 40 (44) px, label 14 px, touching the field as in the base; the form moved by the height difference.
- browse-sort-1024-sideroom, -375-sideroom, -1024-light: the sort reads 14 px like Grid/List and Filters (it was 16 px), 160 px wide, the toolbar no taller, everything on one line.
- browse-filters-1024-sideroom, -375-sideroom, -1024-light: the drawer's Status, Rating and Age fields fill the drawer and are unchanged (the light shot is scrolled higher: only Status is in view).
- reader-bars-1024-sideroom, reader-bars-375-sideroom, reader-bars-1024-light: viewport shots with both bars shown. After: picker 40 px (44), 16 px type, **text cut "Chapter 1: p103-lib fir..." (FAULT 1)**, chevron clear of the box edge; the bar's buttons in place; at 375 the bar overflows the viewport to the right in both phases (proposed row P1).
- reader-end-1024-sideroom, reader-end-375-sideroom, reader-end-1024-light: "Rate this chapter" identical to the base; the picker as above. Under `lounge-light` the base end card already shows a "0 comments" button with near-invisible text and a white rating field (P2).

## Proposed rows (faults of the page that the base had too)
- P1 (reader, base): at 375 the reader's top bar (back arrow, title, 220 px picker, bookmark, comments, bookmarks, settings) is wider than the viewport: Bookmarks and Reader settings fall off the right edge (shots `base-reader-bars-375-sideroom.png`, `base-reader-end-375-sideroom.png`). Cure idea: let the picker shrink (`flex: 0 1 220px; min-inline-size: 0`) or hide the title below 480 px.
- P2 (reader end card, `lounge-light`, base): the "0 comments" button's text is near-invisible and "Not rated" sits in a white field on the black card: the card does not take the dark chrome tokens that the bars get. Contrast not measured; seen in `base-reader-end-1024-light.png`.
- P3 (Library `.lib-select`, base): the label text of Edition and of "Who can see it" touches the select box (no gap between the span and the field in an inline label). Cure: a margin on the span of `.lib-select` (shelves' file).

## SEC-AUD-468 closing text
"The Reader's bars are measured: in the wave 103 library spec both bars are held shown (a `mousemove` poke every 400 ms against the 3000 ms hide), the viewport is shot at 375 and 1024 in the shipped theme and in `lounge-light`, `openCollapsed` and `hitTestAll` run over the top bar and the bottom bar with no row not reached (the bottom slider's point lies off the page at 1024 x 768 and is re-tested with a 12 px reach), and the kit chevron on the picker reads 6.8:1 against the select's ground in both themes and 7.99:1 against the bar. Closed; P1 and P2 are opened."
SEC-AUD-465 item 1 and SEC-AUD-469 close when FAULT 1 is cured and re-shot.

## The commander's command (both phases), from `e2e/`, on a stack with the sweep's world (API 8812, Vite 5812, metrics 9512; `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set)
Base (the tree before the pod's merges; or the merged tree with `git checkout 49ca8946 -- <the 11 app files listed by git diff --name-only 49ca8946 HEAD -- apps packages>` first, then `git checkout HEAD -- <them>`):
`SR_E2E_BASE_URL=http://localhost:5812 SR_P103_LIBRARY_ON=1 SR_P103_LIBRARY_PHASE=base SR_P103_LIBRARY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-library npx playwright test tests/p103-library-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
After: the same without `SR_P103_LIBRARY_PHASE=base` (it compares with `figures-base.json` and writes `compare.json`; delete `figures-base.json` first if the folder holds one from another tree). Expect it to fail on the picker until FAULT 1 is cured.

## Owner questions
- None.

## Round 2, cleanup and lock
- Merged `fix/p103-reader` again at 13:12 (the cure `b82944d5`: `label.lib-chapter-select { font-size: 14px }`); e2e `tsc` exit 0.
- **STACK-ROUND 2**: slot `libshots 13:30`, fresh database, seed `1 passed` (no FAIL line). The first after run (13:32) had 6 problem lines, all one spec artefact: the Rating select's base figure was the wrong select's (round 1); the picker and everything else had no problem. I then ran the base once more (pod files put back with `git checkout 49ca8946 -- <files>`, restored with `git checkout HEAD -- <files>`; `problems: []`, 1.7 m) and the after again: **`problems: []`, `1 passed (1.7m)`, 0 problem lines**. Released 13:36: output `released`, then `free`. Stack stopped (only TIME_WAIT sockets on 5812), database `sr_p103_libshots_ui` dropped.
- FAULT 1 is cured: the picker reads 14 px, the selected text is 148.8 px (153.2 in lounge-light) in a 220 px field, room 21.2 px (16.8 in lounge-light), so it is uncut at 1024 and 375 in sideroom and at 1024 in lounge-light; chevron 6.8:1 on the select, 7.99:1 on the bar (all three runs). Re-opened `reader-bars-1024-sideroom`, `reader-bars-375-sideroom`, `reader-bars-1024-light`, `reader-end-1024-sideroom`, `reader-end-375-sideroom`, `reader-end-1024-light` (text "Chapter 1: p103-lib first" whole in all six; bars' buttons in place; at 375 the bar still overflows right, base fact P1; end card unchanged, P2 still seen in lounge-light) and `browse-filters-1024-sideroom`, `-375-sideroom`, `-1024-light` (drawer fields fill the drawer).
- The Browse filters "Rating" select is now measured in the drawer: 380x40 at 1024 (334x44 at 375, 380x40 light), identical to the base, 14 px, fill: pass (the table row is closed).
- Cleanup: `C:/Users/braxt/Desktop/SIDEROOM_wt/p103-libshots-bin` and `C:/Users/braxt/Desktop/SIDEROOM_wt/p103-libshots-appfiles.txt` deleted. `heavy.sh status` after release: `free`; no hold in my name; no `acquire` of mine alive (the 13:12 and 13:22 ones ended with exit 75 or were the one that gave me the slot).
- The "FAULTS" entry 1 above: cured by reader (b), re-shot, no problem line. SEC-AUD-465 item 1 and SEC-AUD-469 can close.
