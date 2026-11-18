# fix-p107-rooms (wave 107, code builder rooms, pod tools; D-1022, D-1024)

## Clock
Start 18:37 (machine clock), CSS and checks 18:39. Sheets only: no job submitted.

## Done
- `apps/web/src/tools/tierlist/rooms.css`: one block at the very end (two parts), commit `D-1022 rooms.css (ALL READY)`.
  - All widths (D-1024): `.sr-room input:not([type='checkbox'])` rest `border-color: color-mix(in srgb, var(--ui-border), var(--ui-text-faint) 65%)` (the `1px solid var(--ui-border)` stays as fallback), hover `var(--ui-text-faint)`. The ring (`.sr-room input:focus-visible`, 2px/1px `--ui-focus`) and the placeholder rule (faint, opacity 1) already existed: kept.
  - `.sr-room input.sr-tier-name:focus-visible`: `box-shadow: 0 0 0 1px var(--ui-bg), 0 0 0 4px var(--ui-bg)`: a page-ground line inside and outside the 2 px ring (the head's 4 px padding holds 1 + 2 + 1).
  - Phone (`max-width: 720px`, the sheet's existing rule): `.sr-room-btn`, `.sr-room-x` `min-block-size: 44px`; `.sr-room input:not([type='checkbox'])` `min-block-size: calc(44px - 0.5rem - 2px)` (content-box, as wave 106 did, width unchanged); `.sr-room-check` `min-block-size: 44px`.
- Wave 106's `.sr-tier` lines are redundant where mine reach the same controls (btn, x, fields): left, as told.

## Not done
- No measurement (no job for a code builder). Nothing here was built or shot: `toolshots` measures. The tier name's ring figures below are from the tokens, not pixels.
- Cure A.2 for the room bar (grow, bar may wrap, it already has `flex-wrap`): no A.1 tap area was used, because the buttons and fields sit in wrapping rows with a 0.375rem gap.

## REACH table
| control | rule now | page / how opened | cure | will move (phone only) |
|---|---|---|---|---|
| room bar All boards, Retry, Delete `.sr-room-btn` | `rooms.css` `.sr-room-btn` (~l.43, padding .25rem, 27 px) | Tools drawer, Tier list or Poll, open a board (RoomShell.tsx:210) | A.2 | 27 -> 44, bar taller by 17 |
| room bar Title `.sr-room-title input` | `.sr-room input` (~l.21) + `.sr-room-title input` | same | A.2 + B | 27 -> 44 |
| `.sr-room-btn` Add row, Add item, Image from a bucket, Post as message / image, Add an answer, picker buttons | `.sr-room-btn` | tier list under the board; poll under the answers; image picker | A.2 | 27 -> 44 (tier ones already 44 by wave 106) |
| `.sr-room-x` poll answer's remove | `.sr-room-x` (~l.121, 21 x 22) | Poll builder, answers list | A.2 | 22 -> 44; the answer row 27 -> 44 |
| `.sr-room input`, `.sr-room-emoji`, Question, Answer, New row, Item | `.sr-room input`, `.sr-room-emoji` | tier list and poll | B (all widths) + A.2 | phone 27 -> 44 |
| poll checkbox row `.sr-room-check` | `.sr-room-check` (~l.105) | Poll builder, "Allow more than one answer" | A.2 on the label | row -> 44 tall; the box keeps its size |
| `.sr-tier-name` (coloured head) | `.sr-tier-name` (~l.152) + wave 106 | tier list | B ring + inner/outer line | 44 on phone is wave 106's; ring only |

## Tier-name ring against the head (from `themes.ts` tokens, WCAG ratio of `--ui-focus` to the head fill)
Lounge Light (focus #0068c9): neutral head 1.05, danger 1.08, warning 1.25, success 1.09, info 1.00, accent 1.19; against `--ui-bg` 5.49. Lounge Dark (focus #00a8fc): neutral 1.00, danger 1.74, warning 1.39, success 1.21, info 1.00, accent 1.76; against `--ui-bg` 4.82. All under 3:1 (the task's 1.08 and 1.74 confirmed), so the line is added unconditionally; the ring then borders a `--ui-bg` line (4.8 to 5.5:1) on both sides. Other themes: not computed (several tokens are not plain hex). toolshots should verify in pixels.

## FINDERS (rule 13)
- `git grep -n "rooms.css"`: imported by `tools/tierlist/RoomShell.tsx:7` (the lazy tools chunk). `lounge/rooms/ListeningRoomCard.tsx:11` imports a different `./rooms.css` (lounge/rooms/): not mine. No test reads the file as text.
- Classes (`sr-room*`, `sr-tier*`, `sr-poll*`) in `e2e/`: only retired `p82/p103/p104/p105/p106` shots specs. The CI spec `e2e/tests/tool-rooms.spec.ts` uses `getByLabel('Row name')`, `getByLabel('Question')`, role/name buttons at desktop viewports (1280): the 720 px rule does not apply, and the all-width change is border colour only. No test or spec needs a change.
- Users: `RoomShell.tsx`, `TierListTool.tsx`, `MountImagePicker.tsx`, `RoomImage.tsx`, `poll/PollBuilderTool.tsx`; lazy tools chunk only.

## JOBS
None (sheets only).

## Figures against the lists
Not measured. Expectation: may-change rows as in the REACH table; at 1024/1280 only the field border colour and hover change; chips, tier heads and kit selects (`NativeSelect` in the poll's duration, not under `.sr-room input`) untouched. Risk for toolshots to look at: the poll option row (emoji, answer, x) at 375 now 44 tall; `.sr-room-picker` buttons 44 tall in a 10rem scroller.

## Checks (last commit, 18:39)
`npx prettier --check apps/web/src/tools/tierlist/rooms.css`: pass, exit 0. `node scripts/check-css-tokens.mjs`: clean, exit 0. Unit tests: none render this sheet's classes (not run).

## Closing text
D-1022/D-1024: at 720 px and under every room control is 44 px tall (the room bar, every room button, the poll answer's remove, the fields, the checkbox row); at every width the rooms' fields take the kit's rest border and hover, and a tier's name field shows its focus ring on any head colour.
Changelog: Tier list and poll rooms have full-size touch targets on phones and clearer field outlines.

## Owner questions
None.

## PROCESSES
`strays.sh p107-rooms`: strays: none found. No job of mine.
