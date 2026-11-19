# Wave 108 handoff: library (code builder, pod sections; SEC-AUD-525 Library and Study part)

## Clock times
Started 20:49; REACH table and the one edit committed about 20:55 (ALL READY).

## Done (commits)
- `D-1022 apps/web/src/study/study.css (ALL READY)`: on a phone (`max-width: 48rem`, the file's own phone block) `.stu-entry` gets `min-block-size: 44px` (A.2, directly after the `.stu-entry` rule).

## Not done (why)
`library.css` and `shelves.css` are unchanged: the tree already holds the cures, and I did not find a measured reason to move them (no shots in this role; `sectionshots` measures). Left as they are, not as "fixed".

## REACH table
| control | rule that sizes it now | page, how opened | cure | what moves |
|---|---|---|---|---|
| "Sorted descending" `.lib-icon-btn` (36, no border) | library.css:206 (size), :224-232 (phone tap area, `.lib-browse-tools .lib-icon-btn::after`, -4px, 36+8=44, border 0 so the formula gives 4) | Library, Browse, toolbar; only used there (Browse.tsx:141) | A.1 already in tree; formula checks; both labels ("asc"/"desc") are one element, so no other state differs | nothing |
| genre chips `.lib-genre-chip` | library.css:685 (38 border-box, 1 px border); phone area :700-708 at -4px | Browse filters drawer (portal), Library Home | A.1 already; formula (44-38)/2+1 = 4: matches. Wave 107's one-pixel-short finding is already applied here | nothing |
| empty-state links "Mount a bucket", "Ask a friend" `.shv-empty-links a` | storage/shelves/shelves.css:143 (gap 16), library.css:427-437 (`.lib-frame .shv-empty-links`, row-gap 24, ::after -12px) | Library Browse empty library (ShelfEmptyState in `.lib-frame`) | A.1 already (20 px link + 12/12, wrapped rows 24 apart); not mine to edit further | nothing. Cinema's Discover also renders this component outside `.lib-frame`: that is cinema's sheet, not covered here: flagged |
| "Browse the library" | it is a `.lib-btn` link (Pages.tsx:216, SeriesPage.tsx:38), library.css:400-415 `.lib-frame .lib-btn` -3px; formula 40 px + 1 px border = 3 | My library empty, series error | A.1 already | nothing |
| Study file rows `.stu-entry` | study.css:40 (about 31 px, stacked in `.stu-group ul`) | Study side list ("Mine", friends, servers) | A.2: `min-block-size: 44px` on a phone | each entry 44 px, the side list longer by about 13 px per entry; nothing at pointer widths |
| Study crumb `.stu-crumbs button` | study.css:116, phone rule 124-129 (wave 107, `min-block-size: 44px`) | Study files view | A.2 already | nothing |

## FINDERS
- `.stu-entry`: `apps/web/src/showroom/PreviewStudy.tsx:58` uses the class and imports study.css (showroom preview gets the phone height too; no test drives it). No e2e/test hits for `stu-entry`.
- `study.css`: imported by `study/StudyPage.tsx:36`, `showroom/PreviewStudy.tsx:5` and `player/skins/backdrop/StudySkin.tsx:48` imports its own `./study.css` (a different file). No test reads it as text.
- `.lib-icon-btn`, `.lib-genre-chip`, `.shv-empty-links`: only earlier-wave specs (`p105-reader-shots`, `p107-section-shots`), read only. No CI spec drives them.

## JOBS
None (code builder).

## Figures / shots
None measured. Lists: "may change" = the Study entries on a phone; "must not change": nothing else touched.

## Who imports
study.css: StudyPage.tsx (lazy Study chunk), PreviewStudy.tsx (showroom).

## Checks
- `npx prettier --check apps/web/src/study/study.css`: passed, 20:50.
- `node scripts/check-css-tokens.mjs`: 73 files clean, rc 0, 20:50. vitest, eslint, tsc, i18n audit: left to the commander.

## Closing text and proposed rows
SEC-AUD-525 Library part: sort button, genre chips, empty-state links and "Browse the library" are cured in the tree by wave 107 and agree with the formula; sectionshots must confirm (staffshots' list pre-dates the cures). Study: entries cured here; crumb cured in wave 107. Open: Cinema's empty-state links (same `.shv-empty-links`, outside `.lib-frame`), cinema builder's sheet.

Changelog: The Study's side list entries are 44 px tall on a phone.

## Owner questions
None.

## PROCESSES
`strays.sh p108-library`: only the harness shells of this session (not mine, left). No process of mine.
