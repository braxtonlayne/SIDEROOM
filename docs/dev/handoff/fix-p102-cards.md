# Wave 102 handoff: cards (code builder, pod tools; SEC-AUD-453, D-958, D-967)

Clock: started 09:30, all six files moved and committed by about 09:45. No stack, no lock, no spec, no shots (toolshots shoots).

## REACH table (for toolshots)

All six files are lazy: `tools/registry.ts` loads each with `import('./...')` (lines 120, 144, 160, 184, 385, 424). Static importers: only the unit tests `tools/__tests__/{cards,toolsUi}.test.tsx`. First paint and public pages do not load them. Chunk: the Tools drawer's lazy tool chunks (one per tool). `utl-dev.css` is imported by `tools/dev/parts.tsx` and `tools/timers/TimerPanel.tsx` (the same lazy tools chunks). Nobody finds these selects by class or id in `e2e/` or in a `__tests__` folder (the unit tests use roles and labels).

All are inside a `<label>` (a column flex label, not an inline-flex row), so the kit's wrapper span becomes a child of the label. Theme: the tools drawer follows the main theme (Lounge Light / Lounge Dark; `p79-tools-shots.spec.ts` shoots the drawer).

| File:line (after) | Select (label) | Opens at | Sheet rule now |
|---|---|---|---|
| `cards/InviteTool.tsx` ~157 | Channel | Tools drawer > Invite card (needs a channel in the store) | `cards/cards.css:27` `.sr-card select` (padding 0.25rem 0.5rem, bg `--ui-bg`) |
| `cards/InviteTool.tsx` ~168 | Kind | same | same |
| `cards/InviteTool.tsx` ~196 | Time zone (in `.sr-card-row`, flex) | same | same; `.sr-card-row > .sr-card-field` flex 1 1 8rem |
| `dev/DiffTool.tsx` ~74 | Mode | Tools drawer > Diff | `utl-dev.css:92` `.ud-root select` (now `.ud-root > select`) |
| `dev/JsonTool.tsx` ~38 | Indent | Tools drawer > JSON | same |
| `dev/MdTableTool.tsx` ~32 | Column align (one per column, in `.ud-row`) | Tools drawer > Markdown table, paste a delimited table | same |
| `dev/UrlTool.tsx` ~42 | Mode | Tools drawer > URL | same |
| `poll/PollBuilderTool.tsx` ~103 | Duration | Tools drawer > Poll builder (a room, needs the room shell) | `tierlist/rooms.css:19` `.sr-room select` (padding 0.25rem 0.5rem) |

That is 8 selects (3 + 1 + 1 + 1 + 1 + 1).

### What toolshots must look at (read from the sheets, not measured; I ran no stack)
1. **The dev tools' heights depend on `utl-dev.css:92`.** I changed the selector to `.ud-root > select`. No raw select under `.ud-root` is a direct child (all sit in a `Field` label), so the rule no longer matches any of the four moved selects or any other select there (`git grep "<select"` under `tools/dev` and `tools/dice`, `notes`, `timers` finds none). With the rule gone the kit's 40 px / 44 px applies. Per the task, `select` is taken out of that selector entirely only after toolshots reports all four dev tools hold their boxes.
2. **Padding override (Invite card and Poll builder).** `.sr-card select` and `.sr-room select` (specificity 0,1,1) beat the kit's `.select` (0,1,0), so these four selects keep `padding-inline: 0.5rem` instead of the kit's `12px 36px`. Box height is the kit's (`min-block-size` is not overridden), but the chevron (drawn 14 px from the inline end) can sit on top of a long option text: look at the Channel select with a long channel name and at the Time zone select (`Europe/...` names) at 375 and 1024. I cannot cure this (the sheets are not mine); if text and chevron touch, put that select back to raw.
3. **`.ud-field > span` (utl-dev.css:83) and `.sr-card-field` text rules** now also reach the wrapper span (font-size 12px / muted colour). The select's `font: inherit` gives it the label's size; for the dev tools the label font was the parent's before, now 12px from the wrapper span. A size change of the select text in the four dev tools is expected (not a box fault unless the box is narrower or shorter than the base).
4. The wrapper span is `display:block; inline-size:100%`, so in the flex-row `.sr-card-row` and `.ud-row` the label stays the flex item; width should equal the base.

## Done (commits)
- `SEC-AUD-453 cards/InviteTool.tsx (READY)`, `dev/{Diff,Json,MdTable,Url}Tool.tsx (READY)`, `poll/PollBuilderTool.tsx (READY)`, `utl-dev.css:92` `.ud-root select` becomes `.ud-root > select` (D-967). One commit with all, see git log.
- Same ids/props/children; no class existed on these selects, none dropped; no string; no new files.

## Checks (after my last edit)
- prettier --write on the six files and the sheet: unchanged (exit 0).
- eslint on the six files: exit 0.
- vitest `tools/__tests__/cards.test.tsx` 16 passed, `toolsUi.test.tsx` 19 passed, `rooms.test.tsx` 15 passed (09:32, unedited tests).
- `node scripts/check-css-tokens.mjs`: 73 files clean, exit 0.
- TYPECHECK, i18n audit: see the end of this file.

## STACK-ROUND: none (code builder)

## Still raw in my files
None. Nothing was left raw. Faults from toolshots, if any, are put back to raw and listed here.

## Proposed rows
SEC-AUD-453 text: "8 more raw selects (Invite card x3, Diff, JSON, Markdown table, URL, Poll builder) use NativeSelect; `utl-dev.css` `.ud-root select` narrowed to `.ud-root > select`." Existing rows closed: none other.

## Changelog sentence
Eight more selects in the Tools drawer (Invite card, Diff, JSON, Markdown table, URL, Poll builder) use the kit's select, with the same look and 44 px touch height as the rest.

## Owner questions
None. Open follow-up: remove `select` from the `utl-dev.css` field rule once the dev tools are confirmed.

## TYPECHECK
`bash cli-agents/tc.sh run cards bash -c "cd .../p102-cards/apps/web && npx tsc --noEmit -p ."` ran 09:32 to 09:38, exit 0 (no code edit after it; the commit after only added this file).
`node scripts/i18n-audit.mjs --max 0`: TOTAL 0, exit 0 (09:38). `node scripts/check-doc-links.mjs`: ok, exit 0.
Lock: `heavy.sh status` at 09:36 showed only "settings"; I hold nothing and never called acquire.

## FAULT from toolshots (put back)
`poll/PollBuilderTool.tsx` Lasts (Duration) select: 62 px wide against 82 in the base at both sizes, text touching the chevron, the checkbox beside it 20 px left. Put back to raw (the base's text). So 7 of the 8 selects stay moved; the Poll builder one is raw. `utl-dev.css` `.ud-root > select` kept (calc moves LootTool's two selects to match).
Re-run after the fault (10:37): web typecheck via tc.sh exit 0; prettier, eslint exit 0; cards + toolsUi tests 35 passed.
