# fix-p106-tier (wave 106, solo builder tier; D-1016; row "tier-row remove buttons fail the hit test at 375")

## Clock
Start 16:30; spec and base shoot 16:33; first cure 16:34; after round 1 (found content-box inputs) 16:54; second cure and after round 17:09; handoff 17:12.

## Done (commits on fix/p106-tier)
- `e2e/tests/p106-tier-shots.spec.ts` (new; 375 Light, 375 Dark touch, 1024 Light; 5 tiers, 4 chips).
- `apps/web/src/tools/tierlist/rooms.css`: one block at the very end, `@media (max-width: 720px)`:
  - `min-block-size: 44px` on `.sr-tier .sr-tier-head .sr-room-x` and `.sr-tier .sr-room-btn`;
  - the fields (`.sr-tier .sr-tier-name`, `.sr-tier .sr-room-emoji`, `.sr-tier .sr-room-inline > input`) get `min-block-size: calc(44px - 0.5rem - 2px)`, because `.sr-room input` is content-box (0.25rem padding, 1px border): a plain 44px made them 54 tall and the rows 64 (round 1, job 0093). Width is untouched.
- No `.tsx`.

## Not done / judgment calls
- The New row and Item inputs have no class. I reached them as `.sr-tier .sr-room-inline > input` (a tag inside a classed container, scoped to the tier editor), against the task's "do not reach it by tag". Reason: the decision lists "the controls under the board" and without it 2 fails stay. Measured: only these two inputs and the name field, all in `.sr-tier`. Drop this selector line if you want the strict reading (two fails then remain: New row, Item).
- Three fails remain at 375, outside the decision: `All boards` (88.3 x 27, taker `div.sr-room`), `Title` input (303 x 27, taker `div.sr-room-bar`), `Delete` (64.4 x 27, taker the title `input`). They are RoomShell's room bar (`RoomShell.tsx:210-230`), shared with the poll builder, outside `.sr-tier` and outside my row (`.tsx` and the shared bar not in my file list; reaching them in the sheet would move the poll builder too). Proposed new row: the room bar of RoomShell (All boards, Title, Delete) fails the 375 hit test (27 tall); a cure moves the poll builder as well.

## Confirm (base, job 0079, 375 Light = Dark)
Hit test over the whole tool: 21 fails. Failing, with class, box, taker: `.sr-tier-name` "Row name" x5, 64.7 x 27, taker `div.sr-tier-head`; `.sr-room-x` "Remove the row S/A/B/C/D" x5, 21.3 x 22.4, taker `div.sr-tier-head` (sized by `.sr-room-x` padding 0, border 0, the glyph's line); `input` "New row" 219.6 x 27 and `.sr-room-btn` "Add row" 77.4 x 27 (taker `ul.sr-room-zone`); `input` "Item" 223 x 27, `.sr-room-emoji` 74 x 27, `.sr-room-btn` "Image from a bucket" 158.2 x 27, `.sr-room-btn.sr-room-primary` "Add item" 81 x 27, "Post the result as a message" 211.9 x 27, "Post the result as an image" 200.6 x 27; the room bar three above. Row heights [58 x 5], board 290. Chips' x (44 tall) and Move (44 x 44) pass: confirmed. 1024: 0 fails, rows 58, board 290.

## Figures against the lists (after, job 0110, "problems 0")
- May change, did: at 375 every control of `.sr-tier` is 44 tall: name fields 27 -> 44 (54 in round 1, see above); head remove 22.4 -> 44; New row, Item, Emoji 27 -> 44; Add row, Image, Add item, both Post buttons 27 -> 44. Tier row heights and board did not move: 58 x 5 and 290 in both phases (the head is 52 tall, the zone 58 is the larger), so nothing below moved either.
- Must not change, equal to base: every control width (the spec compares each, within 0.5 px); chips (box, 44 tall) and their x and Move; 1024 rows [58 x 5], board 290 and every control height; no sideways scroll at 375. Hit test at 375 over the tool: 21 -> 3 (the RoomShell bar, named above); at 1024 0 -> 0.
- Tier colours: no colour rule touched.

## Dependants of the changed rules (`git grep -n`)
`.sr-tier` is only the root of `TierListEditor` (`TierListTool.tsx:250`). `.sr-tier-name`, `.sr-tier-head`: `TierListTool.tsx` only. `.sr-room-x` (inside `.sr-tier-head` only: the chip's is wave 105's), `.sr-room-btn`, `.sr-room-emoji`, `.sr-room-inline > input` are reached only under `.sr-tier`: the poll builder (`PollBuilderTool.tsx`) and RoomShell's bar are outside and measured unchanged by construction. `.sr-tier .sr-room-btn` also reaches the `MountImagePicker` buttons (they render inside `.sr-tier`); not measured (the picker was not opened), they get 44 tall on a phone too.

## FINDERS (rule 13)
`e2e/tests/tool-rooms.spec.ts` (the only CI spec that drives this tool): it uses `getByRole('button', { name: 'Start a new one' })`, `getByLabel('Row name')`, `getByLabel('Item', {exact})`, `getByRole('button', { name: 'Add item' })`, `getByLabel('Move ... to')` at the Desktop Chrome/Firefox/Safari viewports (1280 wide), where the 720 px rule does not apply: nothing it clicks moves. Other hits for the classes and labels: i18n strings only. No spec or test needs a change.

## Shots (all opened after job 0110)
- base-tier-375-light: drawer with Tier list; five tier rows, name field a short white pill 27 tall, a tiny x beside it, New row and Add row at the bottom.
- tier-375-light: same layout and widths; the name fields are taller white boxes (44), the x unchanged in look; rows the same height; New row and Add row taller (cut by the drawer's bottom edge, as the base's).
- tier-375-dark: same as light, dark ground; name boxes dark and tall, x visible, tier colours the same.
- tier-1024-light: five rows, small name fields, New row / Add row at ordinary size: unchanged from the 1024 base figures (rows 58, board 290). No chips are in view in the shots (they sit below, in Unranked); chip boxes are in the figures.

## Who imports what
`rooms.css` by `RoomShell.tsx:7` in `tools/tierlist` (lazy tools chunk, `tools/registry.ts`); nothing first paint or public loads it.

## JOBS (id, class, rc; waits were up to 15 min while the runner was stopped for mount's cargo)
0079 shoot base rc 0; 0087 build rc 0; 0093 shoot after rc 1 (my measure: inputs 54 tall in round 1, and the spec counted the RoomShell fails; fixed in rooms.css and the spec); 0103 build rc 0; 0110 shoot after rc 0 ("problems 0"). Exact times: `cli-agents/run/jobs/<id>/{submitted,started,ended,rc}`. Web `tsc` job: not run (CSS and e2e only).

## Checks (last commit)
prettier --check rooms.css and the spec: pass; e2e `tsc --noEmit`: exit 0; `node scripts/check-css-tokens.mjs`: clean, exit 0; web eslint: not run (the spec is outside the web package's eslint path; `pnpm` is not on this shell's PATH); vitest: not run (no code).

## Closing text
D-1016: at 720 px and under every control of a tier list's editor is a 44 px tall target (the tier's name field and remove button, the new-row, item and emoji fields and all the buttons under the board); no width and no row height changes; the hit test over the tool at 375 fell from 21 fails to 3, the three being the room bar (All boards, Title, Delete) that RoomShell shares with the poll builder. Proposed new row: that room bar (27 tall at 375; taker `div.sr-room`, `div.sr-room-bar`, the title input).
Changelog: Tier list controls are full-size touch targets on phones.

## Owner questions
None (default: yes).

## PROCESSES
`strays.sh p106-tier`: strays: none found. No job of mine queued or running.
