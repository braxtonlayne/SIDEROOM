# fix-p103-toolshots (shots builder, pod tools; SEC-AUD-463, 464, 465; D-972, D-973)

Spec: `e2e/tests/p103-tools-shots.spec.ts`. Shots and figures: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-tools` (`figures-base.json`, `figures-after.json`, `report-base.json`, `report-after.json`, `table-after.json`, `round2/` = round 2's report (`round1/` holds round 2's three files too: I overwrote round 1's copies by mistake; round 1's problem lines are quoted below), `base-*.png`, `*.png`, `pairs/*.png` base beside after). I changed no app file. My branch holds the merges of `fix/p103-calc` (7e0420bd: 7.5rem for every select field, 9.25rem only for the Units From and To labels), `fix/p103-studio` (e0e9a26a) and `fix/p103-rooms` (b38ba63c, with the chip font cure); no conflict.

## Clock
Started 11:56. Spec compiled 12:00. Round 1 (lock `toolshots 12:14`, base then after, one hold, 12:14 to 12:23). FAULTS sent 12:25. Round 2 (lock `toolshots 12:49`, after only on a fresh database and seed, 12:49 to 12:55). Typecheck 12:56. Round 3 (lock `toolshots 13:01`, after only on a fresh database and seed, 13:01 to 13:06), after the commander's word that calc's cure 7e0420bd was in.

## STACK-ROUND
1. 12:14-12:23, `status` read `toolshots 12:14 / loungeshots 12:14`. Stack 8809 / 5809 / 9509 with `SR_ADMIN_REQUIRE_REAUTH=0`, database `sr_p103_toolshots_ui`, seed with no FAIL line. Base: the pod's app files put back with `git checkout 49ca8946 -- apps/web/src/tools` (calc and studio had been merged at the commander's word before my slot came), 4.1 min, 60 figures, nothing unreached; then `git checkout HEAD -- apps/web/src/tools` (`git status` clean) and the after phase: 60 figures, 61 problem lines.
2. 12:49-12:55, `status` read `toolshots 12:49` alone. Fresh database, seed with no FAIL line, after phase against round 1's `figures-base.json` (a layout figure; the world differs only in names), 60 figures, nothing unreached, 116 problem lines (all explained below). Stack stopped by PID, ports free, released; `status` read `free`.

3. 13:01-13:06, `status` read `toolshots 13:01` alone. Fresh database, seed with no FAIL line, after phase against round 1's `figures-base.json`: **60 figures, unreached 0, problems 0, the spec passed (4.6 min)**. Stack stopped by PID, ports free, released, `status` read `free`.

## TYPECHECK
`tc.sh run toolshots bash -c "cd .../p103-toolshots/apps/web && npx tsc --noEmit -p ."` 12:56, exit 0 (merged tree; round 3's merge changed only `calc.css` and `UnitsTool.tsx`' class, which is not a type change; I did not run it again). e2e `npx tsc --noEmit -p tsconfig.json` exit 0 and prettier --check on the spec exit 0 after the last edit.

## What the commander asked to confirm
- **"Kilometre (km)" at 375: clear of the chevron.** Round 2: Units From is 160 x 44, text 97.1 px inside 110 px (round 1: 120.6 wide, 70.6 inside, text under the chevron). Opened `pairs/units-375-editorial.png`: the text reads whole. Timezone Work starts and ends 160 x 44 (30 and 35 px of text), Programmer Decimal fills its row.
- **Move chip: 80 x 21 again** at 1024 and 375 in every theme (round 1: 80 x 19). The tier-list lines of round 1 are gone.

## ROUND 3 RESULT (the commander's four checks)
- **"Kilometre (km)" at 375 is clear of the chevron:** Units From 148 x 44, text 97.1 px inside 98 px; `pairs/units-375-editorial.png` opened, the text reads whole. To wraps below at 287 x 44.
- **Currency From and To side by side at 375:** 120.6 x 44 each, at x 44 and x 210.4 (round 1 and base: the same); `pairs/currency-375-editorial.png` opened.
- **"Digits are read as" back on its row at 1024 in all three themes:** 156.9 x 40 at x 607.1 beside the buttons and Signed (base 156.9 x 33); `pairs/programmer-1024-dark.png` opened (the other two themes: figures only, the box is the same).
- **0 problem lines.** The Move chip is 80 x 21 at both sizes.
- Informational NOTE lines: a few options are wider than their field and are cut with an ellipsis when picked: Units From and To at 1024 (widest option 123.3 px in 100.1 px, the base clipped it as well) and at 375 (123.3 in 98), Percent Calculation at 375 (241.5 in 219). Not a rule of this wave; listed for the commander.

## FAULTS (history; items 1 is cured in round 3)
1. **(CURED in round 3 by 7e0420bd) calc, `tools/calc/calc.css` end block, floor `10rem` (the cost of round 2's cure), kind (b), new:** one floor for every select field now wraps rows that did not need it.
   - **Currency, 375:** From and To were 120.6 px each, side by side with the swap button; now From is 249.3 x 44 and To 287 x 44 on two lines (text "EUR" and "USD" is 28 px). The spec calls it "a kit select whose box changed although it was the kit's already". Pair `currency-375-editorial.png` opened: whole fields, no overlap, but the converter is one line taller.
   - **Programmer, 1024 (all three themes):** "Digits are read as" was 156.9 px beside the 8/16/32/64 buttons and "Signed"; now it is on its own line, 504 x 40, the Signed row moves 22 px up and everything below it (the bit grid, 32 buttons) moves 45 px down (allowed 7). Round 1 with 7.5rem did not wrap at 1024. Pair `programmer-1024-editorial.png` opened.
   - Cure (b), one: keep `min-inline-size: 7.5rem` for `.sr-calc-field:has(select)` and give only the Units From and To labels the larger floor (a class on those two labels; `9.25rem` = 148 px is the least that holds "Kilometre (km)": 97.1 + 12 + 36 + 2 = 147.1; the widest option of the Length list may need more: `table-after.json` has `maxOptW`, and the spec writes a NOTE line for every kit select whose widest option is wider than its field). The Units row then wraps as in round 2 and Currency and Programmer return to round 1's layout.
2. **rooms, Poll Lasts at 375, not a cure request:** 94 x 44 (base 82 x 27), clear of the chevron (28.9 px in 44 px), 94 x 40 at 1024. The checkbox "Allow more than one answer" no longer fits beside it at 375 and wraps below (dx -88, dy 42; "Post the poll" follows). Whole fields, no overlap, no sideways scroll. Listed so the commander knows the poll form is one line taller at 375.
3. **Observation, no builder named (the kit's look):** in Lounge Light the Invite card's Post in, What is it? and Time zone selects now draw the kit's raised ground (grey) where the base drew the sheet's `--ui-bg` (white), beside Title and Note inputs that stay white (`pairs/invite-1024-light.png`). This is the kit's own field look (D-972 removed the sheet's override on purpose); an owner question below.

Round 1's faults (calc floor 7.5rem too small for Units From; rooms chip 2 px short because `.sr-room select { font: inherit }` beat `.sr-room-move { font-size }`) are addressed by the two cures merged here: the first only partly (item 1 above), the second fully.
No kit fault: the chevron stays at the box end in every shot I opened, and no `fit="content"` field is wider than base + 36 px.

## Table (round 2, 1024 / 375; all reached; hit-test counts equal and no status worse in every hit-tested configuration; no new sideways scroll; keyboard 23 cases with two or more options, no failure)
| page | select | file, builder | base -> after at 1024 | at 375 | verdict |
|---|---|---|---|---|---|
| Units | Category | calc/UnitsTool (dependant, kit) | 504x40 -> same | 287x44 -> same | ok |
| Units | From, To | calc/UnitsTool, calc, kit, fill, 14 px | 150.1x33 -> 150.1x40 | 77.8x33 -> 160x44 / 287x44 (row wraps) | ok (floor; the cost is FAULT 1) |
| Currency | From, To | calc/CurrencyTool (dependant) | 229.1x40 -> same | 120.6x44 -> 249.3x44 / 287x44 | FAULT 1 |
| Percent | Calculation | calc/PercentTool (dependant) | 486x40 -> same | 269x44 -> same | ok |
| Programmer | Digits are read as | calc/ProgrammerTool, calc, kit, fill, 14 px | 156.9x33 -> 504x40 (own line) | 84.1x33 -> 287x44 | FAULT 1 (1024 wrap) |
| Time zones | Work starts, ends | timezone/TimezoneTool, calc, kit, 14 px | 120.4x33 -> 160x40 | 48.1x33 -> 160x44 (text 30 / 35 px in 110) | ok |
| Poll | Lasts | poll/PollBuilderTool, rooms, kit, fit="content" | 82x27 -> 94x40 | 82x27 -> 94x44 (+12 px) | ok (item 2) |
| Poll, Tier list | Channel | RoomShell (dependant, `.sr-room select` gone) | 520x40 -> same | 303x44 -> same | ok |
| Tier list | Mount ("Bucket") | MountImagePicker (dependant) | 502x40 -> same | 285x44 -> same | ok |
| Tier list | Move chip | TierListTool, rooms, left raw | 80x21 -> 80x21 | 80x21 -> 80x21 | ok (unchanged) |
| Invite card | Post in, What is it?, Time zone | InviteTool (dependants, `.sr-card select` gone) | 520x40 -> same | 303x44 -> same | ok (item 3) |
| Loot, JSON, Diff, URL, Markdown table (x3) | all | dev tools (dependants of `utl-dev.css`) | 40, widths equal | 44, widths equal | ok |
| Studio caption | Font | StudioEditor, studio, kit, fill | 107x32 -> 117x40 | 107x32 -> 117x44 | ok |
| Studio export | Format | StudioEditor, studio, kit, fill | 75x32 -> 84x40 | 75x32 -> 84x44 | ok |

Studio row: the kit field is 40 px beside 32 px inputs; the row bottom-aligns, the Font label sits 4 px from the Text size label (the base: the same distance the other way), nothing overlaps (`pairs/studio-caption-1024-editorial.png` opened).
End padding: in the base the wave 102 kit selects under `.sr-calc-field`, `.sr-room` and `.sr-card` had 8 px of end padding (the sheet rule beat the kit's 36); after, every kit select has 36 px (the spec checks it).

## Shots
Folder `visual-sweep-shots/p103-tools`: 60 pairs in `pairs/` (page x configuration), plus 60 base and 60 after PNGs. Configs: 1024 Lounge Editorial and 375 touch Editorial (every page), 375 Lounge Light, 1024 Lounge Light and 1024 Lounge Dark (ten pages each); `data-theme` is read back each time.
**Opened by me (base beside after), what I saw:** round 1: units-375-editorial (From cut as "Kilome", after "Kilometr..."), poll-375-editorial (Lasts wider, checkbox wrapped), studio-caption-1024-editorial (kit Font next to the slider), tierlist-1024-editorial (chip not in view: the spec now scrolls the changed select into view), timezone-375-editorial (whole "9:00" and "17:00", wrapped), programmer-375-editorial (whole "Decimal"), invite-1024-light (grey kit ground, item 3), loot-375-editorial (equal), currency-375-editorial (equal). Round 2: units-375-editorial (whole "Kilometre (km)"), tierlist-375-editorial (the chip at its base size, text "Unranked" cut by the chevron as in the base), programmer-1024-editorial (the select on its own line, FAULT 1), currency-375-editorial (From and To one under the other, FAULT 1).
Round 3: units-375-editorial, currency-375-editorial, programmer-1024-dark (see ROUND 3 RESULT).
**That is 16 opening events on 13 different pairs (of 60); the other 47 (the rest of the Light and Dark configurations, diff, url, mdtable, json, percent, studio-export and so on) I did not open.** The figures (box, text width, hit test, keyboard, scroll) cover them, but that is not looking: they are unopened.

## STALE
None found (I ran no earlier spec).

## Commander's commands (from `e2e/`, on a stack with the sweep world)
Base, with the pod's app files at wave 102's text (`git checkout 49ca8946 -- apps/web/src/tools`, restore with `git checkout HEAD -- apps/web/src/tools`):
`SR_E2E_BASE_URL=http://localhost:<vite> SR_P103_TOOLS_ON=1 SR_P103_TOOLS_PHASE=base SR_P103_TOOLS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-tools npx playwright test tests/p103-tools-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
then the same with `SR_P103_TOOLS_PHASE=after` on the merged tree. On the merged tree as it stands (calc 7e0420bd) the after run passes with 0 problem lines.

## Proposed rows
- SEC-AUD-463 (narrow fields): closed for the Units, Timezone and Programmer text; a per-field floor (7.5rem, and 9.25rem on the Units From and To) holds every text clear of the chevron with Currency and Programmer rows unchanged; the Units widest option is still cut when picked (NOTE).
- SEC-AUD-464 and 465: the tools pod's rules and selects are as the builders' handoffs say; the tier-list Move chip stays raw (80 x 21; at 375 "Unranked" is cut by the chevron, in the base too).
- Optional new row: Invite card selects in Lounge Light draw the kit's grey ground beside white inputs (item 3).

## Owner questions
- The Move chip: a raw 21 px field in a chip clips "Unranked" under the chevron (base too). A menu button instead of a select?
- Invite card selects: the kit's grey ground beside white inputs in Lounge Light: accept, or should the kit field and the card inputs share one ground?

## Cleanup
Database `sr_p103_toolshots_ui` dropped, `p103-toolshots-bin` deleted, ports 8809, 5809 and 9509 free, no process of mine alive. `heavy.sh status` read `free` after my release (I hold nothing).
