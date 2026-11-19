# fix/p108-chips (wave 108, code builder, sections pod)

## Clock
Started 20:44, both sheets committed by about 20:48 (REACH and ALL READY). No job (sheets only).

## Done
- `D-1022 FilterChips.module.css (READY)`, `D-1022 Drawer.module.css (ALL READY)`: the phone tap area (`@media (max-width: 720px)`) goes from 44 to 46 px tall on `.chip`, `.select > button` (the dropdown's trigger and chevron) and the Drawer `.close`. Nothing at pointer widths; no drawn box changes.

## Not done
Nothing measured (no job by rule); the cure is a guess until `sectionshots` shoots it.

## Cause (read, not measured)
- (a) Border: the wave 107 area was already centred (`top: 50%` + translate on the padding box), so the border does not shift it: its total is exactly 44, which equals the brief's formula for a 36 px chip with a 1 px border (-5 px from the padding box). The border is not the cause here.
- (b) Section rules: none. `git grep` of library.css, study.css, cinema.css and storage finds no rule that styles the kit `.chip`, `.dropdown` or `.select` (the `.lib-*` and `.cin-*` chips are their own controls); the Study only sets `--sr-*` variables.
- (c) Clipping: no `overflow` on `.chips`, `.lib-browse-bar`, `.lib-browse-tools` or the Drawer header. The Drawer `.body` (overflow auto) is a later sibling of the header and does not paint over a positioned `::after`.
- (d) Drawer close: same answer; 32 px, no border, header padding 16 px.
- Most likely cause (not provable without a shoot): an exactly 44 px area has no margin: a chip at a fractional y, or a hit-test edge rule that treats the edge pixel as outside (probe's D-1025 is about that edge rule), loses the point 21 px from the centre. That fits "one chip of a row fails, the others pass" (Cinema Drama) and the library chips' earlier "1 px short". Cure: 2 px of margin (46 px). If sectionshots still shows these failing after the build, the cause is a taker I cannot see by reading: it will be named in their figures.

## REACH table
| control | rule now (file:line) | page / how opened | cure | will move |
|---|---|---|---|---|
| `.chip` | FilterChips.module.css `.chip` :7 (min-height 36, border 1, row gap 12) | Library Browse, My library, Study files, Cinema Browse/Discover, shelves; open the page at 375 | A.1: `::after` 46 tall centred on the padding box | tap area only: 5 px above and below the border box, inside the 12 px gap (2 px clear) |
| `.dropdown` trigger and chevron | `.dropdown` :43 wraps the kit Select; `.select > button` | Cinema Browse "Room selection" | A.1 on `.select > button`, same 46 | tap area only, centred on each button |
| Drawer `.close` | Drawer.module.css `.close` :63 (32 x 32, no border) | Library and Cinema Filters drawers, any Drawer | A.1, 46 tall, centred | tap area only: 7 px above and below, inside the header's 16 px padding |

## FINDERS (rule 13)
- `git grep` for the class names in `e2e/` and tests: none by class (role and name only). Tests that read the files as text: `packages/ui/src/primitives/layers.test.ts` reads `Drawer.module.css` (`.backdrop` z-index only; untouched). Imported by `Drawer.tsx`, `FilterChips.tsx`. Result: no test needs a change.

## JOBS
none.

## Figures
Not measured. Expected: every drawn box unchanged; tap areas 46 tall at 375 only.

## Who imports, which chunk
Kit sheets: Drawer and FilterChips CSS were lazy chunks in wave 107 (`Drawer-*.css`, `FilterChips-*.css`). FilterChips is imported by apps/web/src/{cinema,library,study,storage/shelves}. No public page changed.

## Checks
- `npx prettier --check` on both files: pass (after `--write` for line wrapping, 20:47). `node scripts/check-css-tokens.mjs`: pass (73 files clean). vitest, eslint, tsc, i18n audit: left to the commander.

## Closing text / proposed rows
Closing: "On a phone the filter chips, the dropdown chip and the Drawer close have a 46 px tap area without changing size (D-1022)." Proposed note for D-1022: a tap area should be 45 to 46 px, not exactly 44, so the edge pixel is inside.

## Changelog
On a phone the filter chips and drawer close buttons are easier to tap.

## Owner questions
none.

## PROCESSES
strays: none found.
