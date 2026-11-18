# fix-p102-reader (code builder, pod pages; SEC-AUD-453, D-958)

Clock: started 09:30 (`date`), REACH and Reader move committed 09:35.

## REACH table
All five files are loaded only through `apps/web/src/library/routes.tsx` `lazyNamed(() => import(...))` (Lists, Rankings, Pages, SeriesPage, Reader): the Library lazy chunk. None is imported statically by first paint or a public page. Home.tsx imports only `./social` from Rankings' neighbours, not Rankings. Theme: Library shipped `sectionThemes.library = 'sideroom'`, once `'lounge-light'`. None sits in an inline-flex label (`lib-rate`).

| # | File:line | Select | Page / how to open | Sheet rule now | Status |
|---|---|---|---|---|---|
| 1 | `library/reader/Reader.tsx:844` | chapter picker (label `lib-chapter-select`, sr-only name "library.reader.chapterSelect") | `/library/r/$chapterId/1`, a series with 2+ chapters (the component returns null under 2). Reader chrome (dark). | `library.css:1040` `.lib-chapter-select select` (max-width 220, height 32, hard-coded dark colours/border). Sets no padding, so the kit's end padding for the chevron stays. | **PUT BACK TO RAW (FAULT from pageshots)**: at 1024 220x32 became 220x40 (fine), but at 375 the select shrank from 218 px to 115 px wide, so Bookmark, Comments, Bookmarks and Reader settings all moved 103 px left. The 100 % wrapper span inside the label sizes to content. Left raw; file equals b308d5e2. |
| 2 | `library/SeriesPage.tsx:182` | Edition (`label.lib-select.lib-edition`) | `/library/s/$seriesId`, a series with 2+ editions | `library.css:737` `.lib-select select` | **LEFT RAW** |
| 3 | `library/Rankings.tsx:53` | Kind (`label.lib-select`, sr-only name) | `/library/rankings` | `library.css:737` | **LEFT RAW** |
| 4 | `library/Pages.tsx:186` | Sort (`label.lib-select`) | `/library/my` (My library) | `library.css:737` | **LEFT RAW** |
| 5 | `library/Lists.tsx:53` | Who can see it (`label.lib-select`) | `/library/lists` (new-list form) | `library.css:737` | **LEFT RAW** |

Why 2 to 5 stay raw: `library.css:737` `.lib-select select { height: 36px; padding: 0 10px; background: var(--sr-bg-1); border: ...; font-size: 14px }` has specificity (0,1,1), above the kit's `.select` (0,1,0). It would override the kit's end padding (36 px for the chevron: the text would run under the chevron), the colours and the font, and the sheet is the library builder's (D-966 covers library's four selects only and moves them to `lib-select-block`). Taking the class off the label would change layout, which "nothing else changes" forbids. Not measured (stack-free); the cascade is the reason. The cure is a sheet change (`.lib-select > select` narrowed or the labels moved to `lib-select-block`) for the next wave, in library's file.

Found by class in tests: `e2e/tests/p78-library-shots.spec.ts:300` `.lib-select select` (Browse, not mine). Unit tests find mine by label text. No test finds `.lib-chapter-select`.

## Done
- Moved 0 of 5: Reader.tsx was moved, then put back to raw after the pageshots FAULT (see table).

## Not done
- Four selects left raw, above.

## TYPECHECK
Command: tc.sh run reader (web tsc --noEmit), 09:32-09:33, exit 0. prettier --check and eslint on Reader.tsx clean; vitest reader.test.tsx 16/16; i18n-audit exit 0; check-doc-links exit 0 (09:33).

## STACK-ROUND
none (code builder)

## SEC-AUD-453 text
Raw `<select>` still in reader's files: all five. Four behind `library.css:737`; the Reader chapter picker shrinks 218 to 115 px at 375 inside its label (wrapper span).

## Proposed rows / changelog / questions
- Proposed row: SEC-AUD-455 `.lib-select select` (library.css:737) blocks the kit select for 4 selects in SeriesPage, Rankings, Pages, Lists.
- Changelog: the Reader's chapter picker uses the kit's select.
- Owner question: let the next wave narrow `.lib-select select` so the remaining four can move?
