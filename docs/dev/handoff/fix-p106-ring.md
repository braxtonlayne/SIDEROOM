# Wave 106, ring (solo): D-1015; SEC-AUD-475

Branch `fix/p106-ring`. Spec `e2e/tests/p106-ring-shots.spec.ts`; shots and figures `visual-sweep-shots/p106-ring`.

## Clock
Start 16:30; base job done 16:32; CSS commit and build 16:34; after job done 16:35; handoff 16:37.

## Done (commits)
- `D-1015 ring shots spec`
- `D-1015 player.css: a kit select keeps its own focus ring; only the raw presets select is named (SEC-AUD-475)`: `.sr-ply select` becomes `.sr-ply .ss-presets` in both lists (`player.css:49`, `:55`).

## Not done
Nothing. Not run: web `tsc` (no TS file changed), eslint on the spec (the web eslint ignores `e2e/`, as in wave 105).

## Result: 475 works
Base (Lounge Dark 1024, focused by keyboard), kit selects inside the player (Playlists, Equalizer, Add to playlist): outline 2px solid, offset **2px** (from `player.css:55`), box-shadow none. The forum Sort outside the player: offset 1px (the kit). The rule at `:49-53` (`font: inherit; color: inherit; cursor: pointer`) also reached them: base text colour of Playlists and Equalizer rgb(164,170,179) (inherited muted) against the kit's rgb(219,222,225); Add to playlist already 219 (parent colour), cursor pointer against the kit's default; font family and size equal.
After: all three kit selects equal the reference in outline (2px solid, offset 1px, colour, no shadow), and in font, size, colour and cursor (default). **Look changes a member sees (both are the kit's own, D-972):** the Playlists and Equalizer select text is brighter (muted to normal text colour; legible on the panel's dark ground, see shot), and the cursor over a select is the default arrow, not the hand. Boxes unchanged (164x40, 151x40, 156x40).
Must not change, verified equal: Stack presets (`.ss-presets`, every computed value: outline 2px offset 2px, IBM Plex Mono 13px, colour rgb(227,229,232), cursor pointer, box 112x22); the focus ring of a panel button and input (2px offset 2px, equal); the page does not scroll sideways. Only one `select` is in `.sr-ply` per skin shown.
Rule order check: `.sr-ply .ss-presets` (0,2,0) beats `stack.css:324 .ss-presets` (0,1,0) as `.sr-ply select` (0,1,1) did, so the presets' computed values did not move.

## Dependants (D-990)
`git grep -n "ss-presets"`: `player.css:49,55` (changed), `stack.css:324` and `:575` (the skin's own rules; unchanged), `StackSkin.tsx:344` (the one raw select), `settings/server/sections/BannerPresets.tsx:59` (a `div`, outside `.sr-ply`, not reached) and `server-settings.css:442` (own rule). Other `select` in `.sr-ply`: the three kit `NativeSelect`s of the music panel (`sections.tsx:257,485`, `TrackList.tsx:90`). No other skin has a select (`git grep -n "<select\|NativeSelect" -- apps/web/src/player apps/web/src/music`).

## FINDERS (rule 13)
`e2e/`: only retired `p9x/p10x` shot specs and `p105-music-shots.spec.ts` (its `RING-NOT-MET` line prints, no assertion; never edited). `__tests__`: `music/panel/__tests__/panel.test.tsx`, `player/__tests__/backdrop-skins.test.tsx`, `skins.test.tsx` read selects by role/label under jsdom, which does not apply this CSS. No CI spec drives a changed rule: none to fix.

## JOBS
0078 shoot base: submitted 1795037503, started 1795037503, ended 1795037534, rc 0 (first run, good). 0081 build: waited 11 s, ran 18 s, rc 0. 0086 shoot after: waited 130 s, ran 35 s, rc 0, `problems: []`.

## Shots (all opened after the jobs ended)
- `base-ring-playlists-1024-dark.png`: Playlists "Show" select focused, blue ring with a gap, grey text.
- `ring-playlists-1024-dark.png`: same, ring a little closer to the box, select text brighter.
- `ring-row-1024-dark.png`: track action row, "Add to playlist..." select focused, ring close; buttons unchanged. (No base twin: the row's ring figures are in `figures-base.json`.)
- `base-ring-stack-presets-1024.png` / `ring-stack-presets-1024.png`: Stack skin, the "Flat" presets select focused, blue ring with the 2 px gap; the two look identical.

## Who imports
`player.css` is imported by the lazy player host (`PlayerHost` chunk), loaded after sign-in; no first-paint or public file is changed.

## Checks
prettier on player.css and the spec: clean (16:37, 0). e2e `tsc --noEmit`: 0 (after the last spec edit). `node scripts/check-css-tokens.mjs`: 0.

## Closing text and changelog
- 475: "player.css names a select only for the raw Stack presets select; a kit select inside the player shows the kit's ring (offset 1 px), font, colour and cursor."
- Changelog: In the player, the music panel's selects now show the same focus ring and look as selects elsewhere.

## Owner questions
None. Default taken: kit selects in the player look like every other kit select.

## PROCESSES
`strays: none found`.
