# Wave 105, music (solo): D-1009; SEC-AUD-475, 478

Branch `fix/p105-music`. Spec `e2e/tests/p105-music-shots.spec.ts`; shots and figures `visual-sweep-shots/p105-music`.

## Clock
Start 15:30; spec first shot 15:35; CSS commits about 15:45; final base, build, after (0065-0067) done 15:54.

## Done (commits)
- `D-1009 music shots spec` and four fixes of it (rail button opens the library in built mode, group name "Actions for", theme read, fixed playlist name).
- `D-1009 panel.css: the panel leaves the kit select's focus ring alone (SEC-AUD-475)`: `.sr-mlib select:focus-visible` out of the list.
- `D-1009 panel.css: the action row centres its items (SEC-AUD-478)`: `align-items: center` on `.sr-mlib-more`.

## Result
- **478 works.** Base 1024: five buttons 27 tall at y 569, "More like this" 40 tall at y 600 (stretched to the select). After: all six 27 tall, "More like this" at y 606.5 (centre 620 = select centre 620). Select box unchanged (156 x 40 here, 44 at 375; the width is its longest option, the spec's playlist name is now fixed so base and after agree), row 506 x 75 and the row below (y +80) unchanged, buttons not on the select's line unchanged. At 375 the select is on its own line in both, buttons 27, nothing moved. (The task says 29 px buttons and a 224 select; measured 27 and, with the older name, 222. Same story.)
- **475 does not take effect: the fault is not as the task says.** Base and after both: the panel's three selects have `outline 2px solid`, offset **2px**, box-shadow none; the forum Sort has offset 1px; the panel's buttons and input 2px (unchanged). Removing the panel's rule changes nothing because the panel lives inside `.sr-ply` and `apps/web/src/player/player.css:55` has the same rule: `.sr-ply select:focus-visible { outline: 2px solid var(--ui-focus); outline-offset: 2px; }`. That file is in no builder's row. The `panel.css` removal is kept (it is the task's step and is needed for the cure), but alone is a no-op. The spec records the mismatch as `RING-NOT-MET` lines (not a problem line, since it cannot pass without player.css).
- Cure to ask for (new row): in `player.css:55` take `.sr-ply select:focus-visible,` out of the list. One raw select remains under `.sr-ply`: `player/skins/backdrop/StackSkin.tsx:343` (`.ss-presets`, EQ presets of the Stack skin). It needs the ring: give it a narrower selector (`.sr-ply .ss-presets:focus-visible`). Rows it may move: the focus-ring offset of the panel's three selects (2 to 1 px) and of the Stack skin's preset select only if left unnamed. Must not change: ring of every button, input and tab under `.sr-ply`.

## Not done
The 475 effect (above): needs a file outside this row.

## Dependants of the rules (D-990)
- `.sr-mlib-more`: `git grep` finds it only in `music/panel/TrackList.tsx:63` (one use) and `panel.css:142`. Measured: its six buttons, the select, the row, the row below.
- The removed selector reached `.sr-mlib select`: only the three kit `NativeSelect`s (`sections.tsx:257`, `:485`, `TrackList.tsx:90`); no raw `<select` under `apps/web/src/music` (`git grep -n "<select"`: none).
- Who imports: `panel.css` is the lazy music panel's sheet (loaded with the player host, `LibraryManagerPanel`); no first-paint or public-page file is changed.

## JOBS (id, class, waited, ran, rc)
0023 shoot base, 102 s, 60 s, 0 (but spec error: no `__srPlayer` in built mode; figures empty); 0032 cancelled by me (broken spec); 0034 shoot base 81 s, 106 s, rc 1 (wrong group name); 0046 shoot base 175 s, 40 s, 0 (first good base, playlist name had the stamp); 0055 build 164 s, 20 s, 0; 0059 shoot after 95 s, 35 s, 1 (ring offset and a 2 px select width from the per-run playlist name); 0062 shoot after rc 3 stale build (my spec commit); 0063 build 0; 0064 shoot after rc 1 (select width 222 to 210, the name again); 0065 shoot base (fixed name) 0; 0066 build 0; 0067 shoot after, waited 9 s, ran 36 s, **rc 0** (`problems: []`, nine `RING-NOT-MET` lines). Final figures are 0065 base vs 0067 after.

## Shots (all opened after the jobs ended)
- `base-music-row-1024-dark.png`: five buttons in a line, "More like this" below left drawn taller than them, beside "Add to playlist...".
- `music-row-1024-dark.png`: same row, "More like this" now as tall as the buttons above, centred on the select's line; select unchanged.
- `music-row-1024-light.png`: same in Lounge Light, "More like this" centred, light select with grey fill.
- `music-row-375-light.png`: buttons wrap onto two lines, select below (cut by the viewport bottom, as its base twin `base-music-row-375-light.png`, which looks the same); nothing moved.
- `base-music-focus-1024-dark.png` / `music-focus-1024-dark.png`: Playlists "Show" select focused, blue 2 px ring with a gap; the two are identical (the offset is still 2 px, see above).

## Checks
prettier on panel.css and the spec: clean, exit 0 (15:54). e2e `tsc --noEmit` exit 0 (after the last spec edit). `node scripts/check-css-tokens.mjs`: exit 0. eslint on the spec: not run (the web package's eslint ignores `e2e/`). No web tsc job: no TS file changed.

## Closing texts
- 475: "The panel no longer names the kit select in its focus rule. The select still shows a 2 px offset because `player.css:55` names `.sr-ply select:focus-visible`; open until that rule is narrowed (proposed row)."
- 478: "In a track's action row the buttons keep their own height (27 px at 1024) and sit centred beside the taller select; 'More like this' is no longer stretched to 40 px."
- Proposed row: player.css:55 `.sr-ply select:focus-visible` gives every kit select inside the player its 2 px offset (kit 1 px); taker: music panel selects, with `StackSkin.tsx:343` the only raw select that needs it.

## Changelog sentence
In the music panel, a track's action row keeps its buttons at their own height beside the select, and the panel no longer styles the kit select's focus ring.

## Owner questions
None. Default taken: the panel's selects should look like every other kit select (offset 1 px).

## PROCESSES
`strays: none found`. Status at the end: runner alive, none of my jobs queued or running.
