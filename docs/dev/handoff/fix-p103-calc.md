# Wave 103 calc (code builder, pod tools, stack-free)

Branch `fix/p103-calc`. Clock: started 11:55, all code and checks done by about 12:00 (the shell clock). Rows: SEC-AUD-463, 464, 465; D-972, D-973.

## Done (commits)
- `SEC-AUD-465 ... ProgrammerTool.tsx (READY)`, `UnitsTool.tsx (READY)`: three raw selects (Programmer Input base; Units From, To) are `NativeSelect`, no `fit`, `className="sr-calc-pick"`.
- `TimezoneTool.tsx (READY)`: Work starts and Work ends are `NativeSelect`, no `fit`, `className="sr-calc-pick"`; the import was added.
- `CurrencyTool.tsx (READY)`, `PercentTool.tsx (READY)`: one `className="sr-calc-pick"` on each `NativeSelect` (CurrencyTool `:93`, `:111`; PercentTool `:130`) and nothing else (prettier broke the lines; no other change). UnitsTool's Category `NativeSelect` (wave 102) got the class too (`:138`).
- `SEC-AUD-463 calc.css`: `.sr-calc select,` taken out of the list at `:25-28` and `.sr-calc-field select,` out of the list at `:54-55`; the `input` selectors keep both rules untouched. New block at the very end: `.sr-calc-field:has(select) { min-inline-size: 10rem }` (the floor, D-973) and `.sr-calc-field .sr-calc-pick { font-size: 0.875rem }` (two classes, wins over the kit's one; keeps the 14 px the old rule gave the select while the label stays 0.75rem, D-974).
- `SEC-AUD-464 utl-dev.css`: `.ud-root > select,` taken out. No raw `select` is left under `.ud-root`: `.ud-root` is rendered only by `tools/dev/parts.tsx`, and `git grep "<select" -- apps/web/src/tools` finds none in `tools/dev` (the remaining raw ones are poll, studio, tierlist: not under `.ud-root`).
- Final commit: `(ALL READY)`.

## REACH table
All fill a `.sr-calc-field` (label above, select below): none takes `fit="content"`. Theme for all: the Tools drawer / page follows the main theme (Lounge Light / Dark). Open: Tools drawer, pick the tool.

| file:line | label | tool | sheet rule that styled it before | was | now | found by class/id in e2e or tests |
|---|---|---|---|---|---|---|
| UnitsTool.tsx:138 | Category | Units | `.sr-calc-field select` font only (kit box) | kit | kit + `sr-calc-pick` | tests use `getByLabelText('Category')` |
| UnitsTool.tsx:163 | From | Units | `.sr-calc select` + `.sr-calc-field select` | raw | kit + pick | `getByLabelText('From')` (currency test shares labels) |
| UnitsTool.tsx:184 | To | Units | same | raw | kit + pick | `getByLabelText('To')` |
| ProgrammerTool.tsx:92 | Digits are read as (Input base) | Programmer | same | raw | kit + pick | `getByLabelText('Digits are read as')` |
| TimezoneTool.tsx:108 | Work starts | Time zones | same (the tool imports calc.css) | raw | kit + pick | none by class |
| TimezoneTool.tsx:123 | Work ends | Time zones | same | raw | kit + pick | none by class |
| CurrencyTool.tsx:93, :111 | From, To | Currency | `.sr-calc-field select` font only | kit | kit + pick | `getByLabelText` |
| PercentTool.tsx:130 | Calculation | Percent | same | kit | kit + pick | `getByLabelText` |

No test finds a select by a class. Nothing else in `apps/web/src` uses `.sr-calc` or `.sr-calc-field` (`git grep`); CalculatorTool has no select.

## Sheet diff and dependants
- `.sr-calc select` (box: font, colour, ground, border, radius, padding, min size): dependants = every select under `.sr-calc`: the eight above, all moved to the kit in this branch. No raw select is left, so the rule is not needed for any.
- `.sr-calc-field select` (14 px, text colour): same eight. The kit sets `color: var(--ui-text)` itself; the size now comes from `.sr-calc-pick`.
- `.ud-root > select`: no raw select under `.ud-root` (above).
- Floor: `.sr-calc-field:has(select)` is 10rem (160 px; raised from 7.5rem after toolshots FAULT: Kilometre (km) was under the chevron at 375) so a select field never gets narrower than text plus the kit's 36 px end padding; `.sr-calc-opts` and `.sr-calc-convert` already wrap. The base had Units From/To at 77.8 px at 375; with the floor the row wraps. toolshots measures whether 120 px is enough for the longest unit name; if not, the cure is a larger floor in the same block.

## Who imports (lazy chunks)
UnitsTool, ProgrammerTool, TimezoneTool, CurrencyTool, PercentTool are loaded by `import()` in `tools/registry.ts` (`:277`, `:288`, `:321`): lazy tool chunks. `calc.css` is imported by those tools only. `utl-dev.css` is the dev tools' sheet, loaded with the tools. None is in first paint or a public page. No web build run.

## TYPECHECK
`tc.sh run calc bash -c "cd .../p103-calc/apps/web && npx tsc --noEmit -p ."`, 11:58 shell clock, exit 0.

## Other checks (after the last edit)
- prettier --check on calc.css, utl-dev.css; prettier --write on the five tsx (clean): exit 0.
- eslint (the web package's own) on the five tsx: exit 0.
- vitest `src/tools/calc/__tests__/tools.test.tsx` (31 tests, renders Units, Programmer, Percent, Currency): passed. Timezone has only `zones.test.ts` (logic, no render). Base not run for these.
- `node scripts/check-css-tokens.mjs`: exit 0. `node scripts/i18n-audit.mjs --max 0`: TOTAL 0. `node scripts/check-doc-links.mjs`: ok.

## STACK-ROUND
none (code builder).

## Proposed inventory text
- SEC-AUD-463 (narrow select fields): closed in my files by the floor (pending toolshots' measuring at 375).
- SEC-AUD-464 (sheet styles selects by element name): closed in `calc.css` and `utl-dev.css:92`; open elsewhere in other builders' sheets.
- SEC-AUD-465 (raw selects): the five calc/timezone raw selects moved; the Currency/Percent/Category selects already were.

## Changelog sentence
The Tools drawer's Units, Programmer and Time zones selects are the shared select now, with a width floor so their text sits clear of the chevron, and `calc.css` no longer styles selects by element name.

## Owner questions
- The floor is 10rem for all; one floor for the longest unit name. Confirm if a unit name needs more (toolshots measures).
- Stash: none touched.
