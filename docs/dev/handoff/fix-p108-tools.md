# Wave 108 tools (code builder): SEC-AUD-518, 524

Clock: started 20:44; REACH 20:47; all three files ready 20:48 (`date`).

## Done (commits)
- `D-1022 apps/web/src/tools/tools.css (READY)`, `D-1022 apps/web/src/tools/utl-dev.css (READY)`, `D-1022 apps/web/src/tools/calc/calc.css (ALL READY)`.

## Not done
- 528 accepted by the planner: nothing. No spec, shots or job (code builder). Nothing built: no job ran.

## REACH table
| control | rule now | page / how opened | cure | will move |
|---|---|---|---|---|
| `.sr-tl-drawer .lng-switcher-input` | tools.css :16 (border), :116 (ring, offset 2) | the tool drawer's search field, opened from the rail | B: 65 mix after plain border, hover `--ui-text-faint`, ring offset 1 | colour and ring offset only, all widths |
| `.ud-btn` | utl-dev.css :47 (min 32 px) | Base64 Encode/Decode, countdown Start/Reset/Set | A.2 `min-block-size: 44px` in a new `max-width: 720px` block | phone only: every `.ud-btn` (also Hash, Regex, URL, Dice, Coin, Loot, Wheel, Notes, timers, programmer) grows to 44 |
| dev checkbox labels | utl-dev.css :127 (row, centred) | JSON "Sort keys", Base64 "URL-safe alphabet" | A.2 label `min-block-size: 44px`, box stays centred (align-items: center) | phone only, those label rows |
| `.sr-calc-hist-btn` | calc.css :172, :198 (min 0) | Percent tool's five examples | A.2 scoped `.sr-calc-examples .sr-calc-hist-btn` (calculator history untouched); the examples list's `max-block-size: 11rem` lifted on a phone so the 5 x 44 rows are not scrolled out of reach | phone only: examples 5 x 44 |
| `.sr-calc-bit` | calc.css :249 (24 x 28) | Programmer calculator bits | A.2 min-block-size 2.75rem, width kept | phone only: bit rows 28 -> 44 (about 1.5x grid height, under the 2x limit) |
| `.sr-tz-pick` | calc.css :341 | Time zone tool's hour cells | A.2 min-block-size 2.75rem; columns unchanged (table) | phone only: each row 44; tool longer |

## FINDERS
- `e2e/`: `.lng-switcher-input` only in p107-list-shots (retired shots spec) and `.ud-btn` in p79-tools-shots (retired); `.sr-calc-history` selectors in p107-tool-shots (retired, class untouched). No CI spec drives a changed control.
- Tests reading my files as text: `tools/__tests__/overlayZoom.test.tsx` reads `tools.css` for `.sr-tl-rail-wrap[data-open]` z-index: untouched.
- Dependants of `.ud-btn` (phone): see REACH; toolshots should measure those pages (Dice, Coin, Loot, Wheel, Notes, Hash, Regex, URL).
- Chunks: tools are lazy (drawer/tool chunks); `tools.css` is imported by `drawer/DrawerHost.tsx` (not first paint).

## JOBS
none.

## Checks
- `npx prettier --check` on the three files: passed. `node scripts/check-css-tokens.mjs`: clean (73 files). vitest, eslint, tsc, i18n: left to the commander.

## Closing text / changelog
Tool drawer's search field takes the kit field's border, hover and ring; the developer, percent, programmer and time zone tools' small controls are 44 px tall on a phone.

## Owner questions
none.

## PROCESSES
strays lists only the harness shells of this session (not mine, left running); no process of mine.
