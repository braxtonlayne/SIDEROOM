# fix-p102-calc: raw `<select>` to the kit's `NativeSelect` (SEC-AUD-453, D-958), code builder of pod tools

Stack-free: no stack, no lock, no spec, no shots. The shots builder is `toolshots`. Written 09:40 (T = 10:00 plan clock; `date` read 09:31 at start).

## Count
The plan says 12 selects; `git grep -n "<select"` over my six files finds **11** (UnitsTool 3, CurrencyTool 2, PercentTool 1, ProgrammerTool 1, TimezoneTool 2, LootTool 2). 9 moved, 2 left raw (LootTool).

## REACH table
All six tools are floating tool windows in the Lounge. Open any of them as the p80 spec does (`openTool` in `e2e/tests/p80-tools-shots.spec.ts` line 93): `goto('/channels/@me')`, `ControlOrMeta+k`, type `>` plus the query, Enter, wait for the tool's root. Earlier specs that visit them: `p79-tools-shots`, `p80-tools-shots` (loot is there, `slug: 'loot'`), `p81-tools-shots`. The area's theme is the Lounge's main theme (Lounge Light/Dark at 1024, Light at 375); tool windows are not a Library/Cinema section.

| # | File:line (base) | Select (label) | Open with (query) | Sheet rule now | In flex label? | Found by class/id in tests | Status |
|---|---|---|---|---|---|---|---|
| 1 | `tools/calc/UnitsTool.tsx:137` | Category | `>unit converter` | `calc.css:26` `.sr-calc select` and `calc.css:54` `.sr-calc-field select` | label is `display:flex; flex-direction:column` (`.sr-calc-field`), select is a column child: span stretches | `getByLabelText('Category')` (`calc/__tests__/tools.test.tsx:200`) | moved |
| 2 | `UnitsTool.tsx:162` | From (unit) | same | same | same, inside `.sr-calc-convert` (flex wrap row, fields `flex:1`) | label text only | moved |
| 3 | `UnitsTool.tsx:183` | To (unit) | same | same | same | label text only | moved |
| 4 | `tools/calc/CurrencyTool.tsx:92` | From | `>currency converter`; needs `GET /tools/fx` to answer `enabled:true` with rates (`CurrencyTool.tsx:35`); on a stack with the rate source off the tool shows "off" and has no select: the shots builder routes `/tools/fx` with `page.route` | same | same, `.sr-calc-convert` | `getByLabelText('From')` (`tools.test.tsx:349`) | moved |
| 5 | `CurrencyTool.tsx:110` | To | same | same | same | `getByLabelText('To')` (`:350`) | moved |
| 6 | `tools/calc/PercentTool.tsx:129` | Calculation | `>percentage calculator` | same | `.sr-calc-field` column label inside a fieldset | `getByLabelText('Calculation')` (`:276`, `:280`) | moved |
| 7 | `tools/calc/ProgrammerTool.tsx:92` | Digits are read as | `>programmer calculator` | same | `.sr-calc-field` in `.sr-calc-opts` (flex wrap row) | `getByLabelText('Digits are read as')` (`:163`, `:172`) | moved |
| 8 | `tools/timezone/TimezoneTool.tsx:108` | Work starts | `>time zones` | same | `.sr-calc-field`, `.sr-calc-opts` | none | moved |
| 9 | `TimezoneTool.tsx:123` | Work ends | same | same | same | none | moved |
| 10 | `tools/dice/LootTool.tsx:29` | Tables from | `>loot tables` | `utl-dev.css:92` `.ud-root select` | label is `.ud-field` (`dev/parts.tsx:45`) | none | **left raw, see below** |
| 11 | `LootTool.tsx:141` | Table | same, appears once a table list has loaded | same | same | none | **left raw, see below** |


## Moved (9)
Only the tag name changed (`<select` to `<NativeSelect`, `</select>` to `</NativeSelect>`) plus one added line, `import { NativeSelect } from '@sideroom/ui';`, after the first import line, in the five files. Diff read line by line. No class was on any of them, so none was dropped; no string, no sheet.

## Left raw (2): LootTool, both selects (rows 10 and 11)
Reason, from reading the CSS (no stack, so no measurement; the shots builder may measure to confirm): `utl-dev.css:92-101` `.ud-root select` sets `min-block-size: 32px`, `padding: 4px 8px` and `inline-size: 100%`. It has specificity (0,1,1) and beats the kit's `.select` (0,1,0), so the moved select would be 32 px tall (not 40, and not 44 at 375) and its padding would take the chevron's 36 px end padding away. By the rule ("measure the moved select's height: equal to the kit's it moves, else it stays raw and is listed with the rule's file and line") it stays raw: **`apps/web/src/tools/utl-dev.css:92` (`.ud-root select`)**. The sheet is not in my row. Cure for a later wave: the sheet's owner drops `select` from that rule or the kit part gets a stronger selector.

## Risk the shots builder should measure on the 9 moved selects
`calc.css:26` (`.sr-calc select`) and `calc.css:54` (`.sr-calc-field select`) still match the moved select by element name (specificity (0,1,1), over the kit's (0,1,0)). They set `padding-block: 0.375rem`, `padding-inline: 0.5rem`, `border-radius: calc(var(--ui-radius)/2)`, `background: var(--ui-bg)` and `font-size: 0.875rem`. The kit's `min-block-size` (40; 44 at 375) is not set by those rules, so the height should equal the kit's. But **the end padding drops from 36 px to 8 px: the chevron (at 14 px from the inline end, 6 px wide) can sit on top of a long option text** (currency codes are short; unit names, "Calculation" labels such as "The whole from a part and a percent" are long at 280 px window width). Please check the selected text against the chevron at the narrowest window (`minWidth` 280 for percent, units and the others) and at 375. If the text runs under the chevron, the fault is: **file `tools/calc/PercentTool.tsx` row 6 (and any other row that shows it), put back to raw**. Also check the select's width equals its base inside `.sr-calc-convert` (the span is now the flex child's child; the label is a column flex so the span should stretch).

## Who imports the files, chunks
`git grep -n` finds each of the six files only in `apps/web/src/tools/registry.ts` as a dynamic `() => import('./calc/UnitsTool')` (lines 216, 266-321): each is its own lazy tool chunk, opened from the command palette or the Tools window. No static import from `main.tsx`, `router.tsx` or first paint, and no public page. `LootTool` is untouched. `TimezoneTool` also imports `../calc/hooks` and `../calc/calc.css`. `@sideroom/ui` is already in first paint's graph, so the import adds no new package to a chunk of its own beyond `NativeSelect`.

## TYPECHECK
`bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/tc.sh run calc bash -c "cd C:/Users/braxt/Desktop/SIDEROOM_wt/p102-calc/apps/web && npx tsc --noEmit -p ."`, 09:32 to 09:34, exit 0.

## Checks run
- prettier `--check` on the five files: exit 0 (all matched files use Prettier code style).
- eslint on the five files: exit 0.
- Unit tests `src/tools/calc/__tests__/tools.test.tsx`: **not run**. Both attempts (`npx vitest run ...` and `pnpm --filter @sideroom/web exec vitest run ...`) were refused by the session's permission classifier. The test finds all seven of its selects by `getByLabelText`, which the wrapper span does not change.
- `node scripts/i18n-audit.mjs --max 0`, `node scripts/check-doc-links.mjs`: both run 09:33 after the last edit: exit 0 (i18n TOTAL 0; doc links ok).

## STACK-ROUND
none (code builder).

## For SEC-AUD-453 (text)
Still raw in calc's files: `tools/dice/LootTool.tsx` "Tables from" and "Table" (2 selects), because `utl-dev.css:92` `.ud-root select` gives `min-block-size: 32px` and wins over the kit's 40 px by specificity. Moved: 9 selects in `UnitsTool` (3), `CurrencyTool` (2), `PercentTool` (1), `ProgrammerTool` (1), `TimezoneTool` (2); their sheet rules `calc.css:26` and `calc.css:54` still match by element name (end padding 8 px instead of 36 px: check the chevron).

## Proposed inventory rows
- Proposed (from SEC-AUD-455): the LootTool selects and every other select under `.ud-root` stay raw until `utl-dev.css` stops sizing `.ud-root select`.
- Proposed: `calc.css:26` and `:54` style the kit select by element name; trim them once the shots show whether the chevron overlaps.

## Changelog sentence
The calculator family's nine dropdowns (Units, Currency, Percent, Programmer, Time zones) now use the kit's field look with a chevron.

## Owner questions
None.
