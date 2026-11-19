# fix-p107-tools (wave 107, code builder of pod tools)

## Clock times
Started 18:37, sheets done and committed 18:40. No job submitted (code builder).

## Done (commits)
- `D-1022 apps/web/src/tools/tools.css (READY)`, `D-1022 apps/web/src/tools/utl-dev.css (READY)`, `D-1022 apps/web/src/tools/studio/studio.css (ALL READY)`; handoff in a fourth commit.

## Not done
Nothing built or measured (no jobs for a code builder): every figure is for `toolshots`. Python is not installed here, so the edits were made with a node script (deleted).

## REACH table
| control | rule now | page / how opened | cure | what moves |
|---|---|---|---|---|
| `.sr-tl-tab` (Go to, Tools) | tools.css `.sr-tl-tab` (padding 0.25rem; ~30 px) | tool drawer (quick key / Tools button), `Drawer.tsx:150` | A.2 | `min-block-size: 44px` at max-width 720px, inside `.sr-tl-drawer`; tab row +14 px |
| `.sr-tl-btn` Back / Pop out / Pin | tools.css `.sr-tl-btn` | drawer, after opening a tool, `.sr-tl-view-head` (`Drawer.tsx:183-205`) | A.2 | 44 px; the head (flex-wrap, gap .5rem) grows ~14 px per line |
| `.sr-tl-btn` Swap the units, Copy | same class (`UnitsTool.tsx:181,215`; `.sr-calc-swap` in calc.css is calc's, not touched) | Units converter in the drawer | A.2 via the same drawer-scoped rule | those buttons 44 px; calc builder's rule, if any, must not fight it |
| `.ud-root input`, `.ud-root textarea`, `.ud-timer input` (15 fields) | utl-dev.css `.ud-root input, .ud-root textarea, .ud-timer input` (min-block-size 32px) | dev tools (`dev/parts.tsx`) and timers (`TimerPanel.tsx`) in the drawer or a gadget | B whole + A.2 | all widths: rest border colour, hover, ring offset 1px, placeholder. Phone: one-line inputs 44 px (checkbox and radio excluded); textareas keep 120 px |
| Studio rows: Crop, Resize, Pad, Rotate left/right | studio.css `.sr-st-bar, .sr-st-row, .sr-st-panel` (gap .375rem) | Studio modal / tool | A.3 | `row-gap: 16px` at max-width 720px |

Phone rule: tools has none of its own; I used `max-width: 720px` (the kit's and rooms.css's).

Dependants, named: `.sr-tl-btn` scoped as `.sr-tl-drawer .sr-tl-btn` also reaches every other `.sr-tl-btn` inside the drawer (calc Copy/clear history, Currency, Percent, Programmer, Timezone buttons: `git grep -n sr-tl-btn apps/web/src`); they grow to 44 px on a phone only. A gadget or rail is not inside `.sr-tl-drawer`: unchanged. `.sr-st-panel` also holds the Studio's fields (labels) and GifTool's panel/row (`GifTool.tsx:135,174`): the 16 px row gap reaches them on a phone. `.ud-timer input`: TimerPanel. The `:focus-visible` ring of `.ud-*` fields moves from offset 2px to 1px.

## FINDERS
Classes `sr-tl-drawer`, `sr-st-*`, `ud-root`, `ud-timer` in `e2e/` and tests: only old shots specs `p79-tools-shots`, `p80-tools-shots`, `p103-tools-shots` (retired, read only; they select by `.ud-root input...` but nothing asserts the rest border or 32 px). File names: `overlayZoom.test.tsx` reads the tools sheets (ran, 12 passed, unedited); `entranceDangerText.test.ts` bans `color-mix(... --ui-danger`: I use `--ui-text-faint` (ran, 3 passed). No CI spec drives a changed control.

## JOBS
none.

## Figures
Not measured (toolshots). May change on a phone only: drawer tabs/Back/Pop out/Pin/Swap/Copy and other drawer `.sr-tl-btn` to 44 px; dev-tool one-line inputs 44 px; Studio/Gif row gap 16 px. All widths: dev-tool field border/hover/ring/placeholder.

## Imports / chunk
tools.css imported by tools drawer chunk, utl-dev.css by `dev/parts.tsx`/TimerPanel, studio.css by the Studio components: all lazy tool chunks (not first paint; to be confirmed with `git grep -n "tools.css" apps/web/src` by toolshots).

## Checks
prettier --check on 3 files: pass; `node scripts/check-css-tokens.mjs`: clean; vitest `overlayZoom.test.tsx` 12 pass, `entranceDangerText.test.ts` 3 pass (18:38-18:39, exit 0). tsc/build/shoot: not run.

## Closing text / changelog
D-1022 tools drawer header and dev fields 44 px on a phone; D-1024 dev-tool fields follow the kit field. Changelog: "Tool drawer controls, developer-tool fields and Studio button rows are easier to tap on a phone, and developer-tool fields show hover and focus like the rest."

## Owner questions
none. Risk to note: the `.sr-tl-drawer .sr-tl-btn` scope is broader than the header; narrow to `.sr-tl-view-head .sr-tl-btn` if toolshots shows unwanted moves.

## PROCESSES
strays: none found
