# Wave 103, media (code builder, pod media): SEC-AUD-462, 464, 465; D-972

Started 11:55 (real clock). Stack-free: no stack, lock, spec, shots, binaries or database.

## REACH table (first version, 12:00; updated at the end)
How in: the p102 spec (`e2e/tests/p102-media-shots.spec.ts`) reached all five. Music: `openPlayer` (line 543) fakes `GET **/api/v1/music/library*` (three tracks) and the playlist-items POST, makes one own playlist through `POST /users/@me/music/playlists` (line 244), then `musicTab(page, c, ids, first, 'playlists' | 'eq' | 'search')` (line 570). Study: fakes `shelves/visible`, `storage/mounts/*/objects*`, `shelves/<n>/objects` (lines 586-616), then Edit details of a file, the `Cover` combobox. Live TV: fakes `PATCH **/api/v1/users/@me/settings` (line 733), waits for `.tv-reminders select`.

| file:line | select (label) | page / how to open | sheet rule now | theme | status | fit | found by class/id |
|---|---|---|---|---|---|---|---|
| music/panel/sections.tsx:256 | Playlists "whose" (`music.lib.playlists.whose`) | player library panel > Playlists tab (`musicTab 'playlists'`); in `label.sr-mlib-lbl` (inline-flex) | `.sr-mlib-select` panel.css:50 (box) | player/music panel | raw until kit | content | no (none in e2e or `__tests__`) |
| music/panel/sections.tsx:484 | Equalizer preset (`music.lib.eq.presetLabel`) | library panel > Equalizer tab (`'eq'`); in `label.sr-mlib-lbl` | `.sr-mlib-select` | same | raw until kit | content | no |
| music/panel/TrackList.tsx:89 | Add {title} to a playlist (aria-label, no id) | library panel > Search with the faked library and one own playlist; a row's "..." button; last child of `.sr-mlib-more` (flex-wrap, panel.css:144) | `.sr-mlib-select` | same | raw until kit | content | no (`panel.test.tsx` uses it by role) |
| cinema/tv/RemindersCard.tsx:62 | Reminder lead time (`cinema.reminders.lead`) | Live TV guide, bottom "Reminders" card; in `label.tv-reminders-lead` (flex-wrap) | `.tv-reminders-lead select` tv.css:780 | `sideroom-broadcast` | raw until kit | content | spec locator `.tv-reminders select` (element, still matches) |
| study/StudyDialogs.tsx:199 | Cover (`study.edit.fieldCover`) | Study > a file > Edit details; in `label.stu-field` (grid, label above) | `.stu-field > select` study.css:311 (removed) | Study | MOVED (d579e093) | fill (none) | no (spec by role `combobox` "Cover") |

Sheet rules and what each reaches (`git grep -n "sr-mlib-select\|tv-reminders-lead\|stu-field" -- apps/web/src e2e`):
- `.stu-field > span` (study.css:304): reaches the span of the two `.stu-field` labels in StudyDialogs.tsx (Note, Cover); the label span is the first child in both. Narrowed to `span:first-child`. No other file uses `stu-field`.
- `.stu-field > select` and its focus rule: only the Cover select; removed (no raw select is left as a direct child of a `.stu-field`).
- `.tv-reminders-lead select`: only RemindersCard.tsx's one select.
- `.sr-mlib-select` (a selector list with `.sr-mlib-btn` and `.sr-mlib-input`): named by sections.tsx x2 and TrackList.tsx only.

## Who imports (all lazy, none loaded by first paint or a public page)
`TrackList` <- `sections.tsx` <- `LibraryManagerPanel` <- `player/musicWiring.ts` (dynamic `import()`); `StudyDialogs` <- `StudyFiles` <- `StudyPage` (Study route); `RemindersCard` <- `cinema/tv/Guide.tsx` <- `cinema/tv/routes.ts` (`lazyNamed`) (read in wave 102; re-check at the end).

## Progress
- 11:58 Study done (no `fit`): StudyDialogs.tsx (d579e093) then study.css (40754d8c).
- Kit prop (`fit`) not in the base at 11:58 and no p103 READY on `fix/p103-kit` yet: the other four selects wait.
