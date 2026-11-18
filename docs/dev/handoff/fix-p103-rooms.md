# fix-p103-rooms (SEC-AUD-464, 465; D-972)

Code builder, pod tools, stack-free. Kit prop merged at 11:57 (`fix/p103-kit` READY). All of the row is in at 11:58 (`ALL READY`).

## Commits
- `c956180e` SEC-AUD-465 rooms: REACH table
- `285260a3` merge of `fix/p103-kit` (the `fit` prop)
- `8a1076a3` SEC-AUD-465 PollBuilderTool.tsx (READY): Lasts is `<NativeSelect fit="content">`
- `5516a851` SEC-AUD-464 rooms.css, cards.css (ALL READY)

## REACH table
Roots: `.sr-room` (`RoomShell.tsx:138`, hosts the tier list and the poll builder) and `.sr-card` (`InviteTool.tsx:142,154`). Area theme: Tools pages follow the main theme (Lounge Light / Lounge Dark / Editorial).

| file:line | select (label) | where / how to open | sheet rule in base | raw or kit | fit | found by in tests |
|---|---|---|---|---|---|---|
| `tierlist/TierListTool.tsx:63` | Move chip (aria-label "Move <item>") | Tools, Tier list room, any item chip `li.sr-room-chip` | `.sr-room select` (`rooms.css:19,30`) + `.sr-room-move` | raw, stays raw | n/a, own box by class | none in e2e or `__tests__` |
| `poll/PollBuilderTool.tsx:102` | Lasts | Tools, Poll builder room, `label.sr-room-field` in `.sr-room-inline` beside the multi checkbox | `.sr-room select` | raw -> kit | `content` | `p82-tools-shots.spec.ts:261` as `.sr-room-inline select` (still matches) |
| `tierlist/RoomShell.tsx:51` | Channel | both rooms, header, `label.sr-room-field` (column) | `.sr-room select` | kit (wave 102) | fill | none |
| `tierlist/MountImagePicker.tsx:59` | Mount | tier list, image-from-mount picker, `label.sr-room-field` in `.sr-room-picker` | `.sr-room select` | kit | fill | none |
| `cards/InviteTool.tsx:157` | Channel | Invite card tool, `label.sr-card-field` | `.sr-card select` | kit | fill | `__tests__/cards.test.tsx` renders it |
| `cards/InviteTool.tsx:168` | Kind | same | `.sr-card select` | kit | fill | same |
| `cards/InviteTool.tsx:196` | Zone | same, in `.sr-card-row` (cell `flex: 1 1 8rem`) | `.sr-card select` | kit | fill | same |

Nothing else renders inside `.sr-room` or `.sr-card` (`git grep` over `apps/web/src`). Neither sheet has a rule on the label's `span`, so the kit wrapper span is not caught by a label rule.

## Sheet diffs in words
- `rooms.css`: `.sr-room select` removed from the box list (`:19`, inputs keep their rule) and from the focus-ring list (`:30`, replaced by `.sr-room-move:focus-visible` so the raw chip keeps its ring; kit selects have their own). `.sr-room-move` (`:211`) now carries the box the removed rule gave it, by class: `min-inline-size:0`, `max-inline-size:5rem`, `padding-block:0` (no more `!important`: nothing out-specifies a class now), `padding-inline:0.5rem`, border, radius, background, colour, `font: inherit`, `font-size:0.75rem`. Same pixels as base.
- `cards.css`: `.sr-card select` removed from both lists (`:27`, `:43`); the three Invite selects now have the kit's own box.
- Dependants of `.sr-room select`: Move chip (raw, own class now), Lasts (moved), Channel and Mount (kit already). Of `.sr-card select`: the Invite card's three (kit already). `git grep` finds no other raw select under either root.

## Decision: the Move chip stays raw
`.sr-room-chip` is a flex row with `padding-block:0.125rem`, about 24 px tall. A kit field is 40 px (44 on a phone), so every chip and tier row would grow to about 44 px. D-972 lets the rule stay for it. The chip's rule is a class, not an element name.

## Who imports (lazy chunk)
`TierListTool`, `PollBuilderTool`, `InviteTool` are loaded by `import()` in `tools/registry.ts:385,414,424`: Tools lazy chunks. `RoomShell` and `MountImagePicker` are imported only by those. Not on first paint, not a public page.

## Checks (own tree, last commit 5516a851)
- prettier --check on the three changed files: all clean (11:57).
- eslint on `PollBuilderTool.tsx`: no output (11:57).
- vitest `src/tools/__tests__/cards.test.tsx` and `rooms.test.tsx`: 31 passed (11:58).
- `node scripts/check-css-tokens.mjs`: exit 0. `node scripts/i18n-audit.mjs --max 0`: TOTAL 0. `node scripts/check-doc-links.mjs`: doc links ok.
- TYPECHECK: `bash cli-agents/tc.sh run rooms bash -c "cd ./apps/web && npx tsc --noEmit -p ."`, 12:06, exit 0.
- STACK-ROUND: none (code builder).

## Proposed text
- SEC-AUD-464 (rooms part): `rooms.css` and `cards.css` no longer style a select by element name; the tier list's Move chip carries its box by class.
- SEC-AUD-465 (rooms part): the Poll builder's Lasts is the kit's NativeSelect with `fit="content"`; the chip's select is deliberately raw.
- Changelog: The Poll builder's Lasts field uses the shared select look, and the tools' room and card sheets no longer restyle selects by tag.

## Owner question
Should the tier list's Move chip become a 40 px kit field (chips and tier rows grow from about 24 to 44 px), or stay a compact raw select? Default taken: stay raw.

## For toolshots
Measure Lasts at 1024 and 375 (text clear of the chevron, no wider than base + 36 px); Channel, Mount and the three Invite selects for `fill` (not narrower than base); Move chip unchanged from base.
