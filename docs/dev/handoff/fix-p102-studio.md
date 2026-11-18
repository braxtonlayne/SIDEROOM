# fix-p102-studio (code builder, pod tools; SEC-AUD-453, D-958)

Stack-free. No stack, lock, spec, shots, binaries or database.

## REACH table (committed 09:35)
Theme for all: the tools drawer/modal is a Lounge surface (main theme: Editorial, Lounge Light, Lounge Dark). Chunks: all five files are in lazy tools chunks (`tools/registry.ts` `load: () => import(...)`; StudioEditor via StudioTool/StudioModal/GifTool lazy). No first paint or public page loads any of them statically.

| file:line (base) | select | opens | sheet rule | flex label | found by class/id in tests | status |
|---|---|---|---|---|---|---|
| tierlist/RoomShell.tsx:50 | "Channel" (`tools.rooms.channel`) | Tools drawer > Tier list (or Poll builder, same RoomShell) > start a room panel; needs a text channel in the guild. Earlier specs: `e2e/tests/tool-rooms.spec.ts`, `p82-tools-shots.spec.ts` | `.sr-room select` rooms.css:19 (padding-inline 0.5rem, no min-block-size) | label is `.sr-room-field` flex column (block-size not a problem) | `rooms.test.tsx` by role/label only | MOVED (READY) |
| tierlist/MountImagePicker.tsx:58 | "Mount" (`tools.rooms.mount`) | Tier list > add images > pick from storage mount (needs a storage mount, may be unreachable in the seed) | `.sr-room select` rooms.css:19 | `.sr-room-field` flex column | none | MOVED (READY) |
| tierlist/TierListTool.tsx:63 | per-chip "Move <item>" (`.sr-room-move`) | Tier list with at least one item | `.sr-room-move` rooms.css:211 (max-inline-size 5rem, padding-block 0 !important) | inside `.sr-room-chip` display:flex row | none | LEFT RAW: the wrapper span becomes the flex item (inline-size 100%) inside a nowrap chip and the chip grows to 40/44 px; max-inline-size 5rem is on the select, not the span |
| studio/StudioEditor.tsx:441 | caption "Font" | Studio (Image studio tool/modal) > Caption mode | `.sr-st-field select` studio.css:65 (min-block-size 2rem, padding-block 0, padding-inline 0.5rem) | `.sr-st-field` flex column | editor.test.tsx (does not use it) | LEFT RAW: the sheet rule's specificity (0,1,1) beats the kit's min-block-size, so the moved select is 32 px on desktop, not the kit's 40 (rule: not equal, stays raw); it also clobbers the chevron's end padding |
| studio/StudioEditor.tsx:577 | export "Format" | Studio > Export mode | same rule | same | none | LEFT RAW: same reason |

Sheets are read only for me (studio.css and rooms.css are not in my row), so the rules above cannot be changed here.

## Done
- RoomShell.tsx and MountImagePicker.tsx moved to `NativeSelect` (same props, children, no class on either; nothing else changed).
- Checks: prettier --check (exit 0), eslint on both files (exit 0), `vitest run src/tools/__tests__/rooms.test.tsx` 15 passed. All 09:32. i18n-audit, doc links: see below.

## Left raw (SEC-AUD-453 text)
Still raw in my files: TierListTool.tsx chip "Move" select (flex chip, 5rem cap); StudioEditor.tsx caption Font and export Format (`.sr-st-field select` studio.css:65 sets min-block-size 2rem). Cure for a later wave: owner of studio.css drops `min-block-size`/padding from that rule, then both move.
Expected for the shots builder to measure on the two moved: `.sr-room select` padding-inline 0.5rem replaces the kit's 36 px end padding, so long option text can run under the chevron; if the shot shows that, put them back to raw.

## TYPECHECK
(pending)

## STACK-ROUND: none (code builder)

## Changelog sentence
The Tools tier-list/poll room's Channel and storage Mount pickers use the kit's NativeSelect.

## Owner questions
Should studio.css `.sr-st-field select` and rooms.css `.sr-room select`/`.sr-room-move` lose their own box rules so the remaining three move?
