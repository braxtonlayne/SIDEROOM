# Wave 102, media (code builder, pod media): D-958, 6 raw selects of 5 files: 0 moved, all 6 left raw

Started 09:30 (real clock). Stack-free: no stack, lock, spec, shots, binaries or database.

## REACH table
Chunks: all five files are lazy. `git grep` importers: `TrackList` <- `sections.tsx` <- `LibraryManagerPanel` <- `player/musicWiring.ts` (dynamic `import()`); `StudyDialogs` <- `StudyFiles` <- `StudyPage` (Study route); `RemindersCard` <- `cinema/tv/Guide.tsx` <- `cinema/tv/routes.ts` (`lazyNamed`); `StackSkin` <- `PlayerHost.tsx` (`lazy(() => import(...))`). None is loaded by first paint or a public page. No test or e2e file finds any of the six by class or id (only `panel.test.tsx` uses the playlist add by role; passes, 15/15).

| file:line | select | page and how to open | sheet rule now | theme | flex label | status |
|---|---|---|---|---|---|---|
| music/panel/TrackList.tsx:89 | "Add to playlist" (aria-label `music.lib.row.addToPlaylist`, no id) | Music library panel (player's library manager), a track list (Search or playlist detail) with at least one own playlist: click a row's "..." button, the select is last in the `.sr-mlib-more` group. Needs one playlist made through the API. | `.sr-mlib-select` panel.css:50 (dropped) | player/music panel, main theme | no, but its parent `.sr-mlib-more` (panel.css:144) is `display:flex; flex-wrap:wrap` | put back to raw (FAULT from mediashots: 214 -> 506 px wide at 1024, 321 at 375; took a full-width line; rows below moved 42 px against 11 allowed) |
| music/panel/sections.tsx:256 | "Whose" playlists (`music.lib.playlists.whose`) | Library manager > Playlists tab | `.sr-mlib-select` panel.css:50 | same | yes: `label.sr-mlib-lbl` is inline-flex (panel.css:90) | left raw |
| music/panel/sections.tsx:484 | EQ preset (`music.lib.eq.presetLabel`) | Library manager > Equalizer tab | `.sr-mlib-select` | same | yes: `label.sr-mlib-lbl` | left raw |
| study/StudyDialogs.tsx:199 | Cover (`study.edit.fieldCover`) | Study > a file > Edit details dialog | `.stu-field > select` study.css:311 | Study | label.stu-field is grid | left raw |
| cinema/tv/RemindersCard.tsx:62 | reminder lead time | Live TV guide, bottom "Reminders" card (needs the reminders route available) | `.tv-reminders-lead select` tv.css:780 | `sideroom-broadcast` | yes: `label.tv-reminders-lead` is flex, wrap | left raw |
| player/skins/backdrop/StackSkin.tsx:343 | EQ presets (`.ss-presets`) | Stack skin's EQ window | `.ss-presets` stack.css:324 (fixed 1.375rem block size, steel gradient) | skin | no | left raw: part of the skin's look (D-855) |

## Why raw (all read from the code; not measured, no stack)
- sections.tsx x2 and RemindersCard: select inside an inline-flex or flex label: the kit's wrapper span (100 % wide) becomes the flex item and wrapped the label in wave 101 (fix-p101-library.md, "Your rating"). Rule says leave raw and list.
- StudyDialogs: `.stu-field > span` (study.css:304) would match the kit's wrapper span and give it the small uppercase muted label type, which the select inherits (`font: inherit`); `.stu-field > select` (study.css:311) stops matching (the wrapper is the child), which is fine, but the first rule is a sheet this row does not own. Left raw; the cure is a sheet change (`.stu-field > label > span` or `:not(.wrap)`).
- StackSkin: styled by the skin's sheet at a fixed 22 px; the kit's 40 px would change the skin.

## Prediction for mediashots (to measure)
The moved TrackList select sits in a flex-wrap row: the kit wrapper is `inline-size:100%`, so it will likely take its own full-width line below the buttons (neighbours do not move; the select grows wider and a line taller). If that exceeds the rule's tolerances, put it back raw (3 lines: the tag, its close and the import).

## TYPECHECK
`tc.sh run media ... tsc --noEmit -p .` at 09:35: exit 0 (once, after the last edit). prettier --check, eslint (TrackList.tsx), `panel.test.tsx` (15/15), `i18n-audit --max 0`, `check-doc-links`: all exit 0, 09:32 to 09:35.

SEC-AUD-453 text: all six still raw (TrackList put back after the fault); the others: sections.tsx x2, StudyDialogs, RemindersCard (flex label or sheet rule), StackSkin (skin). Changelog: the music library track row's "add to playlist" menu uses the kit select. Owner question: may the kit take an `inline-size:auto` option for selects in flex labels (would free 3 of these)?

## STACK-ROUND: none (code builder)
