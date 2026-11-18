# Wave 103, mediashots (shots builder, pod media): SEC-AUD-462, 464, 465; D-971, D-972

Branch `fix/p103-mediashots` = base + `fix/p103-watch` + `fix/p103-media` (both ALL READY, both already hold the kit's `fit`). Spec `e2e/tests/p103-media-shots.spec.ts`. Shots and figures: `visual-sweep-shots/p103-media` (`figures-base.json`, `figures-after.json`, `compare.json`, `reach-*.json`, `base-*.png`, `<scenario>-<config>.png`, `cmp-*.png` = base | after; 27 composites).

## FAULTS
None. All 9 selects of the pod moved as planned, and the spec ends with `problems: []` on the merged tree. No cure asked of watch, media or kit.

## What the commander asked to measure
- **TrackList "Add to playlist"**: stays in the button row at content width: 206 x 29 -> 224 x 40 (1024 Lounge dark and light), one row neighbour kept. At 375 it is on its own line in both base and after (224 x 44, base 206 x 29). The row below moves by the height difference only (11 px; 15 px at 375). Wave 102's 506 x 40 does not happen. Side effect: "More like this" beside it is drawn 40 px tall (stretched to the row) next to the 29 px buttons above.
- **Focus ring on the music selects**: `panel.css:83` `.sr-mlib select:focus-visible` still matches the kit select. Not a double ring: computed outline is `2px solid`, box-shadow none; only the offset is 2 px (the kit's is 1 px). Playlists, Equalizer and add-to-playlist show offset 2; every other select of the pod shows offset 1. Cosmetic; the rule is not media's row (cure for a later wave: take `select` out of that selector list).
- **Study Cover label**: Title, Note and Cover labels unchanged (the spec compares font-size, text-transform, letter-spacing, weight and colour of every label's first span, base vs after: equal). The kit's wrapper span is not styled as a label (text-transform none, letter-spacing normal).
- **Party panel readability**: all five party selects (People x2, Speed, Season, Start dialog) have text contrast 13.6:1 (11.4 in light) against the select's fill; the border against the ground is 1.27:1 (the kit's `--ui-border`; reported, not a fault of this wave). Cover 10.2:1.

## STACK-ROUND
- Round 1 (base, then after, one database): slot taken 12:33 (`status` "mediashots 12:33"), released 12:38. Base = the 13 changed app/ui files put back to `49ca8946` from git, then the merged text restored (git status clean after); after = merged. Base spec exit 1 only for the Live TV reminders keyboard step (a settle time of the spec; 1.6 s fixed it); after exit 1 for spec errors (names, a label figure, hit-test names), all fixed in the spec.
- Round 3 (after only, fresh database, base figures from round 1): 13:06 to 13:12 (`status` "mediashots 13:06"); spec exit 0, `problems: []`. Released 13:12. Ports 8810 / 5810 / 9510 checked free; database `sr_p103_mediashots_ui` dropped; `p103-mediashots-bin` deleted. Round 1 ran 5 minutes, round 3 6 minutes.
- Waiting for slots: 12:00 to 12:33 and 12:40 to 13:06.
- Final `heavy.sh status`: free, no hold in my name.

## TYPECHECK
`bash cli-agents/tc.sh run mediashots bash -c "cd .../p103-mediashots/apps/web && npx tsc --noEmit -p ."` 13:13 on the merged tree, exit 0. e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (13:11, after the last spec edit). prettier on the spec: clean. eslint: the web package's eslint ignores e2e ("File ignored because outside of base path"), so not run on the spec.

## The pod's selects (base -> after, 1024; at 375 the height is 44)
| file | builder | select | fit | box | verdict |
|---|---|---|---|---|---|
| watchparty/ControlBar.tsx | watch | Speed | content | 72x31 -> 90x40; text clear of chevron; row centred | pass |
| watchparty/TitlePicker.tsx | watch | Season | content | 46x31 -> 59x40; "1" visible beside chevron | pass |
| watchparty/PeoplePanel.tsx x2 | watch (dependant) | Who can play / add to queue | fill | 335x40 both (base already the kit's; the sheet rule's box is gone) | pass |
| watchparty/StartPartyDialog.tsx | watch (dependant) | Where to watch | fill | 520x40 equal (303x44 at 375) | pass |
| music/panel/sections.tsx | media | Playlists "Show" | content | 152x29 -> 164x40 | pass |
| music/panel/sections.tsx | media | Equalizer Preset | content | 139x29 -> 151x40 | pass |
| music/panel/TrackList.tsx | media | Add to playlist | content | 206x29 -> 224x40, stays in row at 1024 | pass |
| cinema/tv/RemindersCard.tsx | media | Reminder lead time | content | 150x36 -> 158x40 | pass |
| study/StudyDialogs.tsx | media | Cover | fill | 520x40 equal (303x40 -> 303x44 at 375) | pass |

All reached. Hit test: counts equal in every scenario, no row worse (the per-run playlist name is normalised in the spec). Document `scrollWidth` never above `clientWidth`; no overlaps. `data-theme` read back before each shot: Lounge dark and light for music; `sideroom`, `sideroom-broadcast` and `lounge-light` (light config) for the sections. Keyboard (focus-visible, ArrowDown changes, ArrowUp restores): Speed 1, 1.25, 1 (one PATCH); who-controls host, controllers, host; Where to watch; Reminders 5, 10, 5; Cover; EQ flat, bassBoost, flat; Playlists "Show"; add-to-playlist (one POST seen, faked). Season has one option (not changed).

## Shots: 27 composites (base | after), every one opened
Five were opened from round 1's identical run (music-tracks-1024-lounge-dark, watch-speed-1024-shipped, watch-picker-1024-shipped, study-edit-375-shipped, watch-theatre-1024-shipped); the other 22 from round 3.
- music-eq (3): preset select taller, chevron at the end, panel +11 px (+15 at 375); the 375 after shows a sliver of the player header above the panel (scroll clip).
- music-playlists (3): "Show" select taller, label beside it, list below +11.
- music-tracks (3): select stays in the button row at 1024 (dark and light), own line at 375 as in base; the neighbour "More like this" is stretched.
- watch-speed (3): Speed wider, chevron clear of "1x", bar row centred, bar +9 px.
- watch-picker (3): Season 59 px wide, "1" and chevron both visible (wave 102's 18 px collapse is gone).
- watch-theatre (3): People selects in the kit's look on the dark panel, readable; at 375 the "IN THIS PARTY" heading is clipped by the shot's scroll only.
- watch-start (3): Where select in the kit's look; in light the dialog is light with the kit's grey fill and border.
- tv-reminders (3): select beside its label at 1024, under it at 375, 40 / 44 tall, no sheet rule.
- study-edit (3): Title, Note and Cover labels identical small caps; Cover 40 (44) tall with the kit chevron. In the "light" config the Study dialog stays dark (same in base).

## STALE
None found (earlier specs not run).

## Proposed rows (faults of the page; the base had them)
- Music panel: `panel.css:83` `.sr-mlib select:focus-visible` gives the kit select a 2 px offset instead of 1 px.
- The kit's `--ui-border` on selects is 1.2 to 1.6:1 against the ground in every theme measured (under the 3:1 usually asked of a field boundary).
- The Study dialog does not follow the section's light theme.

## Spec notes
Roots for the music panel (`.sr-mlib`), Live TV (`.tv-reminders`) and the theatre (`.wp-root`, `.wp-stage-wrap`, `.wp-picker`) are found by class (the p102 ways in); selects are found by role, name or tag inside them. The Live TV keyboard step records but does not demand the restore (`noRestore`); it did restore in round 3.

## Commander's command (from e2e/; stack API 8810, Vite 5810, metrics 9510, sweep world seeded, `SR_ADMIN_REQUIRE_REAUTH=0`, DATABASE_URL and SR_CLI_BIN set)
`SR_E2E_BASE_URL=http://localhost:5810 SR_P103_MEDIA_ON=1 SR_P103_MEDIA_PHASE=base SR_P103_MEDIA_OUT=<folder> npx playwright test tests/p103-media-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` on the tree before the pod branches (the 13 changed files at `49ca8946`), then the same with `SR_P103_MEDIA_PHASE=after` on the merged tree. Without `figures-base.json` the after phase checks only `EXPECT` (moved, size, chevron, contrast, wrapper, focus ring).

## Owner questions
None.
