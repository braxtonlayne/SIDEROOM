# fix-p105-card (wave 105, solo builder card; D-1000; SEC-AUD-489, 483, 490)

## Clock
Start 15:30; spec and base shoot 15:35; cure edits 15:36; build and after round done 15:50; handoff 15:52.

## Done (commits on fix/p105-card)
- spec `e2e/tests/p105-card-shots.spec.ts` (new; two later fixes: fixed item names so chip text width is equal in both phases, x hit filter limited to the chip buttons)
- 489: `cards.css` new rule after the field rule, `.sr-card input::placeholder, .sr-card textarea::placeholder { color: var(--ui-text-faint); opacity: 1 }`; `rooms.css` end block `.sr-room input::placeholder` the same.
- 483 (D-993): `background: var(--ui-bg-raised)` at `cards.css:33` and `rooms.css:24`.
- 490: `rooms.css` end block, `@media (max-width: 720px) { .sr-room-chip .sr-room-x { min-block-size: 44px; margin-block: -1px } }`. The button has class `sr-room-x` (`TierListTool.tsx:105`); scoped under `.sr-room-chip` because the same class is the tier-row remove button (`TierListTool.tsx:277`) and the poll option remove (`PollBuilderTool.tsx:74`), which must not change.

## Not done
Nothing. The row remove buttons (`Remove the row S..D`, `.sr-tier-head`) fail the 375 hit test in the base too (taker: `div.sr-tier-head`, 21 x 22 box); not mine, not in my rows: proposed new row below.

## Confirm (base, figures-base.json)
`--ui-text-faint` on `--ui-bg-raised`: Light 98,102,110 on 242,243,245 = 5.19; Dark 154,160,170 on 43,45,49 = 5.24; Editorial 163,157,139 on 28,27,23 = 6.36. All pass 4.5, so built.
Base placeholder (browser default rgb 117,117,117, no rule): 4.61 / 2.74 / 3.46 on the field's own ground. Base ground: Light 255, Dark 49,51,56, Editorial 35,34,29 (kit select 242,243,245 / 43,45,49 / 28,27,23).
Chip x at 375 (base): 21.3 x 22.4, hit test fail, taker the chip `li.sr-room-chip`.

## Figures against the lists
May change, and did: placeholder colour and field ground (Invite input, textarea, placeholder input; room item input; poll builder input at 1024 Light); at 375 chip x height 22.4 -> 44 (y 521.8 -> 511 = chip top), width 21.3 unchanged.
After: ground equals the kit select's beside it in all three themes (242,243,245 / 43,45,49 / 28,27,23). Placeholder 5.19 / 5.24 / 6.36. Text colour unchanged; text ratio 12.64 -> 11.39 (Light), 9.36 -> 10.21 (Dark), 12.84 -> 13.89 (Editorial), all above 4.5. Border colour unchanged. Kit select ground unchanged.
Must not change, equal to base within 0.5 px: every Invite control box (4 configs), the room item input box, the tier row heights (58 x 5), the chip boxes (44 tall at 375; 28.4 at 1024), the Move button (20 x 20; 44 x 44 at 375), x width and, at 1024, x height. Hit-test fails over the whole tool: 1024 0 -> 0; 375 23 -> 21 (the two chip x fails are fixed; the rest are earlier faults, same in both). Poll builder: item input box equal, ground raised. No sideways scroll (the spec logs it; none logged).
Users of the two rules (`git grep -n "sr-card\b\|sr-room\b"`): `.sr-card` is the Invite card only (`InviteTool.tsx`); `.sr-room` is `RoomShell`, used by `TierListTool` and `PollBuilderTool`: one input of each measured.

## Shots (all opened after job 0060)
- base-invite-1024-light: fields white, select grey, one placeholder "/cinema/t/123" in default grey.
- invite-1024-light: every field now the select's grey 242; same boxes; placeholder darker.
- invite-1024-dark: fields match the select and the panel's darker ground; placeholder readable; borders still show.
- invite-1024-editorial: fields dark brown like the select; placeholder readable; same layout.
- invite-375-light: fields grey like the select; no sideways scroll; same layout as 1024.
- room-1024-light: Item and Emoji inputs grey, same ground as the Unranked zone above (border keeps them apart); chips and x unchanged.
- room-1024-dark: inputs match kit buttons' ground; chips unchanged.
- base-tier-375-light / tier-375-light: chips identical (44 tall, Move 44 x 44, x glyph in the same place); the x's target is now 44 tall and invisible; inputs grey in the after.

## JOBS (job, class, rc; times are epoch s: submitted/started/ended)
- 0024 shoot base: 1795033971/1795034079/1795034129 rc 0
- 0031 build: 1795034159/1795034237/1795034259 rc 0
- 0036 shoot after: rc 1 (spec problem, mine: chip text width differed between phases by run id)
- 0044 shoot base (re-shot): 1795034402/1795034514/1795034563 rc 0
- 0048 shoot after: rc 3 stale build (the spec commit changed the tree); 0056 build rc 0 (1795034678/1795034861/1795034880)
- 0060 shoot after: 1795034884/1795034989/1795035037 rc 0, "problems 0"

## Who imports what
`cards.css` by `InviteTool.tsx:9`; `rooms.css` by `RoomShell.tsx:7`; both lazy tools chunks (`tools/registry.ts`). Nothing first paint or public loads them.

## Checks (on my last commit)
prettier --check on the spec and both css files: pass; e2e `tsc --noEmit`: exit 0; `node scripts/check-css-tokens.mjs`: clean, exit 0 (15:51); web eslint: the spec is outside its base path (warning only, not linted); web tsc/vitest: not run (CSS and e2e only).

## Closing text
- 489: the card and room inputs state the kit's placeholder colour (`--ui-text-faint`, opacity 1); placeholders now measure 5.19 / 5.24 / 6.36:1 (Light / Dark / Editorial), were 4.61 / 2.74 / 3.46.
- 483: the card and room inputs draw the kit's field ground `--ui-bg-raised`, equal to the kit selects beside them in all three themes; text stays above 11:1 / 10:1 / 13:1, placeholder above 5:1 (Light's 4.15 of the wave-104 trial came from the browser grey, which is replaced).
- D-993 should now read: built in wave 105 after D-1000 lifted the placeholder in the area's sheets; no token changed.
- 490: the tier chip's remove button is a 44 px tall target at 375 (the hit test passes; width and the chip's box are unchanged).
Proposed new row: the tier row remove buttons (`.sr-tier-head`, `Remove the row S..D`, 21 x 22 at 375) fail the hit test in the base (taker `div.sr-tier-head`).
Changelog: Tool card and room inputs use the kit's field ground and placeholder colour, and the tier chip's remove button is a full-size touch target on phones.

## Owner questions
None (default: yes, as decided).

## PROCESSES
`strays.sh p105-card`: strays: none found. No job of mine queued or running.
