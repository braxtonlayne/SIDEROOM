# fix/p107-kitbits (wave 107, code builder, staff pod)

## Clock
Started 18:37, three sheets done and committed by 18:39. Nothing queued, no job submitted (sheets only).

## Done
- `D-1022 Drawer.module.css (READY)`, `D-1022 Feedback.module.css (READY)`, `D-1022 FilterChips.module.css (READY)` (the last is ALL READY: the three files are all committed).
- Each adds one `@media (max-width: 720px)` block at the end of its sheet (the kit's own breakpoint, as `Button.module.css` :127). No drawn box changes; nothing at pointer widths.

## Not done
Nothing measured: no job by rule (staffshots measures). Cure A.1 is a guess until its shoot.

## REACH table
| control | rule now (this tree) | page / how opened | cure | will move |
|---|---|---|---|---|
| Drawer close | `Drawer.module.css` `.close` :63 (32 x 32, header padding 16 px) | any Drawer (cinema/library filters drawer, tool drawers); open by its trigger | A.1 `.close::after` 44 tall, `inset-inline:0` (32 wide), `top:50%` + translateY | tap area only; reaches 6 px above/below into the header's padding |
| Notice dismiss | `Feedback.module.css` `.dismiss` :50 (24 x 24, notice padding 12 px) | any dismissible Notice | A.1, same pattern | tap area only; 10 px above/below, over the notice's padding and the line below it in a short notice (no other control sits in the notice's right column) |
| FilterChips `.chip` | `FilterChips.module.css` `.chip` :7 (min-height 36, row gap 12) | Library browse/pages, Study files, Cinema browse/discover, shelves; open the page | A.1 | tap area only; 4 px above/below inside the 12 px gap; no row grows |
| Dropdown chip trigger and chevron | `.dropdown` min-height 36 wraps the kit `Select` control (`.select` lands on its root); trigger and chevron are the root's two button children | Cinema browse "Room selection" | A.1 on `.select > button` (child combinator, no box, no element name for a box: D-972) | tap area only; the two pseudo-elements are placed against the Select's own `position: relative` control and the buttons are made relative, so each is centred on its own box; the dropdown's drawn box is unchanged |

Rows closer than 8 px: not checked here; the chip rows are 12 px apart by the sheet (`.chips` gap 12), but a page that places two chip rows closer than 8 px (or the dropdown next to chips in a flex row with a smaller gap) would overlap by a tap area: staffshots should look at that.

## FINDERS (rule 13)
- Classes touched are CSS-module classes (`.close`, `.dismiss`, `.chip`, `.dropdown`, `.select`): `git grep` in `e2e/` finds none by class. e2e drives the Drawer, Notice and chips by role and name only (`e2e/tests/drawer.spec.ts` uses `getByRole('button', { name: 'Close Drawer guide' })`: no change in role, name or tag).
- Files read as text: `packages/ui/src/primitives/layers.test.ts` reads `Drawer.module.css` and `Feedback.module.css` (`.backdrop`, `.region` z-index only: untouched). Imported by `Drawer.tsx`, `Feedback.tsx`, `FilterChips.tsx`.
- Result: no spec or test needs a change.

## JOBS
none.

## Figures
Not measured (no job). Expected: every drawn box unchanged; tap areas 44 tall.

## Shots
none (not mine).

## Who imports, which chunk
Kit sheets (`@sideroom/ui`): Drawer and Feedback are in the kit bundle; Notice and Drawer are used in lazy chunks, but the kit sheet itself may be in first paint (the kit CSS is shared; the added rules are media-gated and tiny). FilterChips: imported by `apps/web/src/{cinema,library,study,storage/shelves}` (lazy chunks) and exported from `packages/ui/src/index.ts`. Not a file a public page changes.

## Checks (my tree, last commit)
- `npx prettier --check` on the three files: pass.
- `node scripts/check-css-tokens.mjs`: pass (73 files clean).
- `npx vitest run src/primitives/misc.test.tsx` (packages/ui): 29 passed, ~10 s. `layers.test.ts`: 10 passed.
- web `tsc`, build, shoot: not run (not mine).

## Closing text / new rows
Closing text: "Drawer close, Notice dismiss, filter chips and the dropdown chip get a 44 px tap area on a phone from a pseudo-element; their drawn boxes do not change (D-1022)." No new rows.

## Changelog sentence
On a phone the Drawer's close, the Notice's dismiss and the filter chips have a 44 px tall tap area without changing size.

## Owner questions
none.

## PROCESSES
strays: none found (strays.sh p107-kitbits). No job of mine queued or running.
