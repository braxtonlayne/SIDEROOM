# fix-p102-toolshots (shots builder, pod tools; SEC-AUD-453, D-958)

Spec: `e2e/tests/p102-tools-shots.spec.ts`. Shots and figures: `visual-sweep-shots/p102-tools` (`measure-base.json`, `measure-after.json`, `report-after.json`, `base-*.png`, `*.png`, `pairs/*.png` base beside after). I changed no app file; the pod's three branches are merged in (calc, cards, studio, no conflict).

## Clock
Started 09:30. Base round 09:38-09:44 (spec errors found and fixed). Base+after round 10:08-10:12 (Vite died before the last configuration of after). After-only round 10:29-10:32. Typecheck 10:34. Finished about 10:45.

## STACK-ROUND
1. 09:38-09:44, `status` showed `toolshots 09:38` and `shade 09:38`: base on a fresh DB with the seed (no FAIL line); 12 targets reached; json, poll and tierlist were spec errors (option name, Poll has 2 selects, rows button), fixed.
2. 10:01-10:06 (`toolshots 10:01`, `arcadm 09:59`): base for those three plus after; the measure read scroll offsets wrongly (spec error, fixed).
3. 10:08-10:12 (`toolshots 10:08`, `settings 10:06`): full base (the pod's app files put back with `git checkout b308d5e2 -- apps/web/src/tools`, restored with `git checkout HEAD -- apps/web/src/tools`, `git status` clean afterwards), then after; Vite crashed (ws proxy ECONNABORTED) before the third configuration of after. The base figures are complete.
4. 10:29-10:32 (`toolshots 10:29`, `lounge 10:21`): after only, three configurations, 27 figures, nothing unreached.
Base and after share one database (`sr_p102_toolshots_ui`, dropped). The spec's own run ends red because of the faults below. The comparison rules were corrected after round 4 (compare in the drawer's own coordinates, since the drawer is centred and a taller body moves the whole dialog; a neighbour in the select's row may follow the row's growth); I re-ran them offline over `measure-base.json` and `measure-after.json` with a port of the same logic, and the spec now holds the same logic. The corrected spec was not run again (no slot was needed for it to be right; a re-run is the commander's).

## TYPECHECK
`tc.sh run toolshots bash -c "cd .../p102-toolshots/apps/web && npx tsc --noEmit -p ."` at 10:34, exit 0. e2e `npx tsc --noEmit -p tsconfig.json` exit 0 and prettier --check on the spec exit 0, after the last edit.

## Table (all reached; hit-test counts equal and no worse everywhere; no sideways scroll)
| file | builder | select | verdict |
|---|---|---|---|
| UnitsTool | calc | Category 504x33 -> 40 (1024), 287x33 -> 44 (375) | ok |
| UnitsTool | calc | From, To 150x33 -> 40 (1024) ok; at 375 77.8 wide, 44 tall, text under the chevron | FAULT (375) |
| CurrencyTool | calc | From, To 229 / 120 wide, 40 / 44 | ok |
| PercentTool | calc | Calculation 486 / 269 wide, 40 / 44 | ok |
| ProgrammerTool | calc | Input base 157x33 -> 40 (1024) ok; at 375 84 wide, "Decimal" touches the chevron (the base clipped it too) | FAULT (375, crowding) |
| TimezoneTool | calc | Work starts / ends 120 wide at 1024 ok; at 375 48 wide, "9:" and "17" under the chevron (base also 48 wide and clipped) | FAULT (375) |
| LootTool | calc (raw) with cards' sheet line | Tables from, Table: still raw, 496x32 -> 496x19 (1024), 279x32 -> 279x19 (375) | FAULT (cards' css) |
| InviteTool | cards | Post in, What is it? 520x27 -> 40 (1024), 303x27 -> 44 (375); Time zone 256 -> 520 wide at 1024 (fills its row, no overlap) | ok |
| DiffTool, JsonTool, UrlTool, MdTableTool (3 selects) | cards | 40 / 44, widths equal | ok |
| PollBuilderTool | cards | Lasts (duration): 82x27 -> 62x40 (1024), 82x27 -> 62x44 (375), "1 day" touches the chevron, "Allow more than one answer" moves 20 px left | FAULT |
| RoomShell Channel | studio | 520x27 -> 40, 303 -> 44 | ok |
| MountImagePicker Mount (label "Bucket") | studio | 502x27 -> 40, 285x27 -> 44 | ok |
| TierListTool Move chips; StudioEditor Font and Format | studio (left raw) | boxes equal to base (80x21, 107x32, 75x32) | unchanged |
Keyboard (1024, every select with two or more options): focus-visible, the arrow key changes the value, the way back restores it: no failure. Hit test: many desktop fails exist in the base too (21 px points on 33 px rows); counts and statuses are not worse after. Cards' `.ud-root > select` rule: Diff, JSON, URL and MdTable kit selects reach 40 / 44, so it did not hold the kit's height back there.

## FAULTS (commander: pass to the builders named; each is a put-back to raw or the cure named)
1. **cards `tools/utl-dev.css:92` (`.ud-root > select`) with calc `tools/dice/LootTool.tsx` :29 and :141:** the two Loot selects were left raw because the old rule styled them. The new rule no longer reaches them (they sit in `Field` labels), so they fall to the browser default: 32 -> 19 px tall at both sizes, and the controls below move up 13 and 26 px. Cure: calc moves both Loot selects to `NativeSelect` (they then match the Diff, JSON and URL selects, which pass), or cards puts the selector back to `.ud-root select`. Loot needs a re-shot either way.
2. **cards `PollBuilderTool.tsx`, Lasts select:** narrower than base (62 < 82 at both sizes), text touches the chevron, the checkbox beside it moves 20 px. Put that select back to raw.
3. **calc `UnitsTool.tsx` From and To, `TimezoneTool.tsx` Work starts and Work ends, `ProgrammerTool.tsx` Input base (all at 375 only):** same inline size as base, but the kit's chevron padding leaves no room for the text (77.8, 48 and 84 px wide). Put those five back to raw (the 1024 boxes are fine; a cure by a sheet in calc.css is not in calc's row).
Not faults: Percent (long option, fits at 269 wide), Invite Time zone, Channel, Bucket, the dev tools.

## Shots
Folder `visual-sweep-shots/p102-tools`: 36 shots after (`<tool>-<config>.png`), 36 base (`base-...`), 36 pairs in `pairs/`. Configs: 1024 Lounge Editorial (the shipped theme), 375 touch Editorial, 1024 Lounge Light (units, invite, json, poll, studio caption, tier list); `data-theme` is read back each time. **I opened all 36 pairs, base beside after.** What I saw: the kit selects are taller (40 / 44) with the chevron at the end and the page below shifts down by the difference; the raw selects left alone (studio Font and Format, tier-list Move chips) look the same; Loot shows flat grey default-looking selects; Poll Lasts, Units From / To at 375, Timezone start / end at 375 and Programmer Decimal at 375 show the value text touching or under the chevron. Invite shows a focus ring on "What is it?" from the keyboard case, not a fault. Light theme pairs look like their dark ones.

## Notes
- The spec leaves in the world one guild `P102 tools shots`, per-run channels `p102-<phase>-<run>-<config>` and their poll and tier list rooms (the database is dropped).
- Commander's command, from e2e/, on a stack with the sweep world: `SR_E2E_BASE_URL=http://localhost:5797 SR_P102_TOOLS_ON=1 SR_P102_TOOLS_PHASE=base SR_P102_TOOLS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-tools npx playwright test tests/p102-tools-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` on the unmerged pod files, then the same with `SR_P102_TOOLS_PHASE=after` on the merged tree. A Vite stack can die mid-run (ws proxy error): the spec writes its figures after each configuration.
- Proposed rows: SEC-AUD-455 the Loot selects lose their size when `.ud-root select` narrows to `.ud-root > select`; SEC-AUD-456 the kit select's chevron padding leaves narrow selects (48 to 84 px at 375) with text under the chevron.
- Second after round: not run (needs the builders' fixes first).

## Cleanup
Database `sr_p102_toolshots_ui` dropped; `p102-toolshots-bin` deleted; ports 8797, 5797 and 9497 free; no shell of mine alive. `heavy.sh status` at hand-back showed `mediashots 10:32` only: no hold in my name (I released after every round).
