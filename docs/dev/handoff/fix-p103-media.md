# Wave 103, media (code builder, pod media): SEC-AUD-462, 464, 465; D-972

Started 11:55 (real clock). Stack-free: no stack, lock, spec, shots, binaries or database.

## REACH table (first version, 12:00; updated at the end)
How in: the p102 spec (`e2e/tests/p102-media-shots.spec.ts`) reached all five. Music: `openPlayer` (line 543) fakes `GET **/api/v1/music/library*` (three tracks) and the playlist-items POST, makes one own playlist through `POST /users/@me/music/playlists` (line 244), then `musicTab(page, c, ids, first, 'playlists' | 'eq' | 'search')` (line 570). Study: fakes `shelves/visible`, `storage/mounts/*/objects*`, `shelves/<n>/objects` (lines 586-616), then Edit details of a file, the `Cover` combobox. Live TV: fakes `PATCH **/api/v1/users/@me/settings` (line 733), waits for `.tv-reminders select`.

| file:line | select (label) | page / how to open | sheet rule now | theme | status | fit | found by class/id |
|---|---|---|---|---|---|---|---|
| music/panel/sections.tsx:256 | Playlists "whose" (`music.lib.playlists.whose`) | player library panel > Playlists tab (`musicTab 'playlists'`); in `label.sr-mlib-lbl` (inline-flex) | `.sr-mlib-select` panel.css:50 (box) | player/music panel | MOVED | content | no (none in e2e or `__tests__`) |
| music/panel/sections.tsx:484 | Equalizer preset (`music.lib.eq.presetLabel`) | library panel > Equalizer tab (`'eq'`); in `label.sr-mlib-lbl` | `.sr-mlib-select` | same | MOVED | content | no |
| music/panel/TrackList.tsx:89 | Add {title} to a playlist (aria-label, no id) | library panel > Search with the faked library and one own playlist; a row's "..." button; last child of `.sr-mlib-more` (flex-wrap, panel.css:144) | `.sr-mlib-select` | same | MOVED | content | no (`panel.test.tsx` uses it by role) |
| cinema/tv/RemindersCard.tsx:62 | Reminder lead time (`cinema.reminders.lead`) | Live TV guide, bottom "Reminders" card; in `label.tv-reminders-lead` (flex-wrap) | `.tv-reminders-lead select` tv.css:780 | `sideroom-broadcast` | MOVED | content | spec locator `.tv-reminders select` (element, still matches) |
| study/StudyDialogs.tsx:199 | Cover (`study.edit.fieldCover`) | Study > a file > Edit details; in `label.stu-field` (grid, label above) | `.stu-field > select` study.css:311 (removed) | Study | MOVED (d579e093) | fill (none) | no (spec by role `combobox` "Cover") |

Sheet rules and what each reaches (`git grep -n "sr-mlib-select\|tv-reminders-lead\|stu-field" -- apps/web/src e2e`):
- `.stu-field > span` (study.css:304): reaches the span of the two `.stu-field` labels in StudyDialogs.tsx (Note, Cover); the label span is the first child in both. Narrowed to `span:first-child`. No other file uses `stu-field`.
- `.stu-field > select` and its focus rule: only the Cover select; removed (no raw select is left as a direct child of a `.stu-field`).
- `.tv-reminders-lead select`: only RemindersCard.tsx's one select.
- `.sr-mlib-select` (a selector list with `.sr-mlib-btn` and `.sr-mlib-input`): named by sections.tsx x2 and TrackList.tsx only.

## Who imports (all lazy, none loaded by first paint or a public page)
`TrackList` <- `sections.tsx` <- `LibraryManagerPanel` <- `player/musicWiring.ts` (dynamic `import()`); `StudyDialogs` <- `StudyFiles` <- `StudyPage` (Study route); `RemindersCard` <- `cinema/tv/Guide.tsx` <- `cinema/tv/routes.ts` (`lazyNamed`) (read in wave 102; re-check at the end).

## Progress (real clock)
- 11:58 Study done (no fit): StudyDialogs.tsx (d579e093), then study.css (40754d8c).
- 11:58 `fix/p103-kit` showed "SEC-AUD-462 NativeSelect fit="content" (READY)"; merged into this branch.
- 11:59 sections.tsx x2, TrackList.tsx, RemindersCard.tsx moved, all `fit="content"`, each its own commit. 12:00 sheet rules: tv.css, then panel.css (ALL READY).

## What each sheet's diff is, and its dependants
- study.css: `.stu-field > span` -> `.stu-field > span:first-child` (the label span is the first child of both `.stu-field` labels, Note and Cover; nothing else uses the class). `.stu-field > select` and `.stu-field > select:focus-visible` deleted: the only raw select under a `.stu-field` was Cover, now the kit's. Dependants: the Note textarea's `.stu-field > .stu-textarea` and the Edit details dialog (read, not changed).
- tv.css: `.tv-reminders-lead select` (box) deleted; `.tv-reminders-lead` and its comment stay. Only RemindersCard's one select was reached.
- panel.css: `.sr-mlib-select` taken out of the `.sr-mlib-btn, .sr-mlib-input` list (no .tsx names it any more; `git grep sr-mlib-select` finds nothing). The buttons and the input keep the rule untouched. `panel.css:83` (`.sr-mlib select:focus-visible`, not mine) stays and now also matches the kit's select: outline 2px with offset 2px next to the kit's own ring (shots builder: look at the focus ring).
- Class dropped from the three music selects (no test, spec or e2e file finds them by it; `panel.test.tsx` uses the playlist add by role).

## Chunks
Unchanged from the table: all lazy, nothing loaded by first paint or a public page. TrackList <- sections.tsx <- LibraryManagerPanel <- player/musicWiring.ts (dynamic import); StudyDialogs <- StudyFiles.tsx <- StudyPage (Study route); RemindersCard <- cinema/tv/Guide.tsx <- cinema/tv/routes.ts (lazyNamed). Re-read 12:03 with `git grep`. Not mine, left alone: the Stack skin's EQ presets (D-855).

## TYPECHECK
Each file after its edit: prettier --check, eslint, exit 0. `vitest run src/study src/storage/shelves/__tests__/shelves.test.tsx` 5 files, 57 tests passed; `vitest run src/music/panel src/cinema` 20 files, 262 tests passed (11:57-11:58, after the last edit of each file; the css edits after them touch no test). `node scripts/check-css-tokens.mjs` 11:58 exit 0; `node scripts/i18n-audit.mjs --max 0` 12:02 exit 0; `node scripts/check-doc-links.mjs` 12:02 exit 0.
Web tsc: via `tc.sh run media` (STEP TYPECHECK) started 11:59 (I had first run one unlocked tsc by mistake at 11:58-11:59, no errors printed; it is not the reported one): waited in the queue, ran to 12:07, exit 0 (no errors).

## STACK-ROUND: none (code builder)

## Text for the rows
- SEC-AUD-462: not mine (kit); used by all four `fit="content"` selects here.
- SEC-AUD-464: study.css, tv.css and panel.css no longer style a select by element name or class (the `.stu-field > span` label rule is narrowed to `:first-child`).
- SEC-AUD-465: five raw selects (music Playlists whose, Equalizer preset, track-row Add to playlist; Live TV reminders lead time; Study Cover) are the kit's NativeSelect; the Stack skin's stays raw (D-855). The track row's select is the one wave 102 put back (506 px wide); `fit="content"` is the cure to measure.
- Changelog: the music panel's selects, Live TV's reminder lead time and the Study cover picker use the kit's select.

## Owner questions
None. For mediashots: the TrackList select is 40 px beside smaller buttons (accepted); measure it first.
