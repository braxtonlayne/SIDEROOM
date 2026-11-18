# fix-p103-rooms (SEC-AUD-464, 465; D-972)

Status at 11:57: REACH table written; sheet edits drafted in the tree (not committed: they would collapse the raw Poll Lasts select until the kit's `fit` prop is merged); waiting for `fix/p103-kit` (READY).

## REACH table
Roots: `.sr-room` (`RoomShell.tsx:138`, hosts the tier list and the poll builder) and `.sr-card` (`InviteTool.tsx:142,154`). Area theme: Tools pages follow the main theme (Lounge Light / Lounge Dark / Editorial). Tools are lazy chunks (`tools/registry.ts:385,414,424` `import()`), none loaded by first paint.

| file:line | select (label) | where / how to open | sheet rule now | raw or kit | fit | found by in tests |
|---|---|---|---|---|---|---|
| `tierlist/TierListTool.tsx:63` | Move chip (aria-label "Move <item>") | Tools, Tier list room, any item chip (`li.sr-room-chip`) | `.sr-room select` (`rooms.css:19`, `:30`) + `.sr-room-move` (`:211`) | raw | stays raw (decision below), own box by class | none found (`git grep sr-room-move` in e2e and `__tests__`: none) |
| `poll/PollBuilderTool.tsx:102` | Lasts (`tools.poll.duration`) | Tools, Poll builder room, row `.sr-room-inline` beside the multi checkbox | `.sr-room select` | raw -> kit | `fit="content"` | `e2e/tests/p82-tools-shots.spec.ts:261` finds it as `.sr-room-inline select` (still a select: unaffected) |
| `tierlist/RoomShell.tsx:51` | Channel | room shell header, both rooms, `label.sr-room-field` (column) | `.sr-room select` | kit (wave 102) | fill | none |
| `tierlist/MountImagePicker.tsx:59` | Mount | tier list, "add image from a mount" picker, `label.sr-room-field` in `.sr-room-picker` | `.sr-room select` | kit | fill | none |
| `cards/InviteTool.tsx:157` | Channel | Invite card tool, `label.sr-card-field` | `.sr-card select` | kit | fill | `__tests__/cards.test.tsx` renders InviteTool |
| `cards/InviteTool.tsx:168` | Kind | same, next label | `.sr-card select` | kit | fill | same |
| `cards/InviteTool.tsx:196` | Zone | same, in `.sr-card-row` (cell `flex: 1 1 8rem`) | `.sr-card select` | kit | fill | same |

No other component renders inside `.sr-room` or `.sr-card` (`git grep sr-room|sr-card` over `apps/web/src`: RoomShell, TierListTool, PollBuilderTool, MountImagePicker, RoomImage, InviteTool, press.tsx; press.tsx only a link class). Neither sheet has a rule on the label's `span`, so the kit's wrapper span is not caught by a label rule.

## Decision: the Move chip stays raw
`.sr-room-chip` is `display:flex; align-items:center; padding-block:0.125rem` and the chip is about 24 px tall. The kit's field is 40 px (44 on a phone), so the chip, and every tier row, would grow to about 44 px. D-972 allows "its rule stays for it": the select stays raw and `.sr-room-move` now carries the box the `.sr-room select` rule gave it, by class (padding-block 0, padding-inline 0.5rem, border, radius, background, colour, `font: inherit`, size 0.75rem, max 5rem), minus `!important`. Look the same as the base. Owner question below.
