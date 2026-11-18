# fix-p107-calc (wave 107, pod tools, code builder; D-1022, D-1024)

## Clock times
Started 18:37; both sheets done and committed about 18:50.

## Done (commits)
- `D-1022 apps/web/src/tools/calc/calc.css (READY)`
- `D-1022 apps/web/src/tools/cards/cards.css (ALL READY)`
No `.tsx`, no spec, no job (sheets only).

## Not done
- `.sr-calc-bit` (Programmer bit buttons, 24 x 28), `.sr-calc-swap`, `.sr-tz-pick`, `.sr-calc-hist-btn`: not in my task; unchanged. Flag for toolshots: bit buttons will still fail 44 px on a phone.
- Card textarea: not a one-line input, no phone height (field contract only).

## REACH table
| control | rule now | page / open | cure | what moves |
|---|---|---|---|---|
| keypad `.sr-calc-key` | calc.css `.sr-calc-key, .sr-calc-hist-btn` (min-block-size 2.25rem); grid `.sr-calc-keys` gap .25rem | Tools drawer > Calculator | A.2: `min-block-size: 2.75rem` at max-width 720px | 16 keys +8 px, each keypad row +8 px |
| `.sr-calc-input` (expression, value, quick conversion) | calc.css `.sr-calc-input, .sr-calc input[type=date|text]` | Calculator, Units, Currency, Percent, Programmer, Time zone | A.2 + B | field 44 px tall on phone; its row grows |
| date and text inputs `.sr-calc input[type='date'|'text']` | same rule | Percent/Time zone/Programmer | A.2 + B | same |
| `.sr-calc-seg-btn` (Deg / Rad) | calc.css, padding .375rem, ~28 px | Calculator | A.2 | button 44 px (group `.sr-calc-seg` grows with it) |
| checkbox "Show every unit" (and Programmer "Signed") `.sr-calc-check` | calc.css `.sr-calc-check` (flex, align-items center) | Units, Programmer | A.2 on the label | label row 44 px, box centred |
| `.sr-card input` | cards.css `.sr-card input, .sr-card textarea` | Tools drawer > Invite card | B (all widths) + A.2 for inputs | 5 inputs 44 px on phone (~+75 px), card ~100 px longer with the button |
| `.sr-card textarea` | same | Invite card | B only | none |
| `.sr-card-btn` (Post the card, also "Use" and the done-state button) | cards.css `.sr-card-btn` | Invite card | A.2 | 44 px on phone; row stays align-self flex-start |

Phone rule: `@media (max-width: 720px)`, the tools' own width (`tierlist/rooms.css` uses it; the calc and cards sheets had none). Field contract at every width: `border-color: color-mix(in srgb, var(--ui-border), var(--ui-text-faint) 65%)` after the existing `border`, hover `var(--ui-text-faint)`, `:focus-visible` 2px `--ui-focus` outline offset 1px (calc had a ring only on buttons; the card already had one), placeholder faint at opacity 1 (calc added; card kept its D-1000 rule). The card's ground and placeholder are untouched (wave 105 handoff kept).

## FINDERS
`git grep` for the classes and file names in `apps/web/src packages scripts e2e`: only `e2e/tests/p103-tools-shots.spec.ts` (retired wave 103 shots spec, text mention) and `p105-card-shots.spec.ts` (retired); the TSX that use the classes: CalculatorTool, CurrencyTool, PercentTool, ProgrammerTool, UnitsTool, InviteTool, press.tsx, TimezoneTool. No CI spec or unit test drives or reads these sheets as text. Verdict: none to fix.

## JOBS
None.

## Figures
Not measured (no build by rule); toolshots measures. Expected: only the phone items above move; 1024/1280 change only the border colour and ring of the fields.

## Importers
calc.css: calc tools and TimezoneTool (lazy tool chunks). cards.css: InviteTool/press.tsx (lazy). Not first paint.

## Checks
prettier --check both files: pass. `node scripts/check-css-tokens.mjs`: clean. Unit tests: none render these sheets; not run.

## Closing text and changelog
Changelog: the calculator, converters and Invite card fields get the kit's border, hover and focus ring, and 44 px targets on a phone.
Owner questions: none.

## PROCESSES
strays: none started by me (not run beyond the check below).
