# Wave 102 handoff: arcadm (SEC-AUD-449 / D-963, SEC-AUD-453 / D-958 for the Arcade)

Branch `fix/p102-arcadm`. Commits: `74e6f1ea` SEC-AUD-449 (the `arcade.css` block and the new spec), `591c82c9` SEC-AUD-453
(`ArcadeSections.tsx`, `StartInDialog.tsx`, a second block at the end of `arcade.css`).
Clock: started 09:30, STACK-ROUND 09:59 to 10:03, both items committed 10:04.

## Item 1: SEC-AUD-449 (done)
Confirm first (base, same round and database): at 375 touch, Lounge Light and Dark, the two seeded entries' Release and Remove
buttons were 28 px tall and Release's lower point was taken by Remove (3 fails, taker `button Remove ...`; the buttons were
8 px apart, but a 28 px box puts the +21 px point past the gap).

| 375 x 812, touch | base | after |
|---|---|---|
| Release / Remove height | 28 / 28 | 44 / 44 |
| gap Release to Remove | 8 px (existing flex gap) | 8 px |
| "Released as" field box | 305.9 x 63 | 305.9 x 63 (kit Select, unchanged) |
| entry 1 block (li) | 164.8 | 196.8 (+32 = the two buttons' growth) |
| entry 2 block (li) | 184.8 | 216.8 (+32) |
| hit rows over `.arc-admin`: fails / not-reached | 3 fails (Release, taker Remove) / 0 | 0 / 0 (the one other row is `skipped`: "Add to the showcase", disabled) |
| sideways scroll | 375/375 | 375/375 |

At 1024 every box equals the base (buttons 28 px, li 124.8, form 95, field 384 x 59). Lounge Dark at 375 equals Light. The 1024
hit rows list the same fails in base and after (pointer-width test of 28 px buttons; D-963 changes nothing above the phone
width). The candidates' Feature button (same rule, `.arc-admin-list button`) was **not reached**: the world has no released
Activity with a tier ("No released Activity declares a tier yet"), so that list is empty; the selector covers it, it is not
measured. Release and Remove were never clicked.

Diff in words: one block at the end of `arcade.css`: `@media (max-width: 40rem)` with `.arc-admin-list button { min-block-size: 44px }`,
`.arc-admin-list .arc-release { gap: 8px }`, `.arc-admin-list > li + li { margin-block-start: 8px }`. No markup, no string, no
test change (`arcadeAdmin.test.tsx` passes unedited, 5/5).

Closing text for SEC-AUD-449: "Fixed (D-963): at phone width every button of an Admin > Arcade list entry is 44 px tall and 8 px
apart; at 375 the hit test has 0 fails (base: 3, Release taken by Remove); 1024 unchanged."

## Item 2: the three selects, SEC-AUD-453 (done, all three moved)
| file | select | reached how | 1024 base to after | 375 base to after | hit test |
|---|---|---|---|---|---|
| `ArcadeSections.tsx` | server picker on the page (`.arc-server-pick`, shows from 2 servers) | `/games`, section `#cartridges`; the spec makes a second server of its own | 384x40 to 384x40 | 337.5x40 to 337.5x44 | page at 375: 1 fail to 0 |
| `ArcadeSections.tsx` | Kind in the List-a-cartridge dialog | button "List a cartridge" | 448x40 to 448x40 | 303x40 to 303x44 | dialog at 375: 1 fail to 0 |
| `StartInDialog.tsx` | server picker in the Start dialog (2 servers) | Play on the first card | 520x40 to 520x40 | 303x40 to 303x44 | at 375 the select row passes now; 10 fails to 9, the 9 are the channel rows (36 px, not mine) |

Keyboard (all three, both phases): focus, ArrowDown changed the value, ArrowUp put it back. No sideways scroll (1024 and 375, both
phases). Labels: equal at 1024; at 375 the controls below move by the select's 4 px growth only (`measure.json`). Themes: Arcade
`lounge-editorial` (read back from the Arcade root) at 1024 and 375, and `lounge-light` at 1024.

**Sheet rules that reach the moved selects by element name** (`arcade.css`): `.arc-field select` (:903; min-block-size 40, padding-inline
12px, border, background) and `.arc-field > span` (:892; the small-caps label rule). The kit's wrapper span is a direct child of
`.arc-field`, so the second would have drawn the wrapper as a label (the select inherits its font) and the first overwrites the kit's
`padding-inline: 12px 36px` (text under the chevron). Both are undone inside my block: `.arc-field > span:has(> select)` resets
colour, font, letter-spacing and text-transform; `.arc-field > span > select` puts `padding-inline: 12px 36px` back. Measured on all
three: computed `text-transform none`, `font-size 16px`, `padding-inline 12px 36px`. The cleaner cure (commander or wave 103) is to
narrow the two existing selectors, as lounge does with `.lng-field > select`, and drop my two rules; I did not touch them because they
are outside my block. No class was kept on the selects (nothing in `e2e/` or `__tests__` finds them by class).

Who imports (git grep): `ArcadeAdmin.tsx` is in the lazy Admin chunk (`StaffDoor` `import()`s `AdminApp`); `ArcadeSections.tsx` and
`StartInDialog.tsx` are in `ArcadePage` (lazy in `emu/routes.tsx`), and `ArcadeSections` is also in `showroom/PreviewArcade.tsx` (lazy from
`ArcadePage`); `arcade.css` is imported by `ArcadePage`, `ArcadeShell` (lazy), `AdminApp` and `PreviewArcade`. None is first paint or public.

## Checks (after the last edit, tree at `591c82c9`, 10:05)
- e2e `tsc` exit 0; prettier --check on the spec, the css and both tsx: clean; eslint on both tsx exit 0; `check-css-tokens` clean;
  `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok.
- Unit tests, single files: arcadeAdmin 5/5, startIn 6/6, arcade 40/40, lobbies 14/14, arcadePage 7/7.
- The base was run in the same round and database (`arcade.css`, `ArcadeSections.tsx`, `StartInDialog.tsx` put back from copies in
  the scratch folder, never a stash) and watched fail as in the tables.

### STACK-ROUND
One round, slot 2 (`status` showed "pageshots 09:44" and "arcadm 09:59"), 09:59 to 10:03: stack on 8784 / 5784 / 9484 with
`SR_ADMIN_REQUIRE_REAUTH=0`, `rm -rf e2e/.state`, seed (no FAIL line), the spec twice (base, then after; about 31 s each). Stack
stopped by the listening PIDs, ports checked free, released.

### TYPECHECK
`bash cli-agents/tc.sh run arcadm bash -c "cd C:/Users/braxt/Desktop/SIDEROOM_wt/p102-arcadm/apps/web && npx tsc --noEmit -p ."`, 10:04, exit 0.

## Shots (`visual-sweep-shots/p102-arcadm`, 23 PNG; I opened all 23)
Item 1: `arcadm-375-lounge-light`, `arcadm-375-lounge-dark`, `arcadm-1024-lounge-light`, and `base-` twins of the first and third.
Item 2: `carts-page-`, `carts-dialog-` and `start-` at 1024-editorial, 375-editorial and 1024-light (names like
`carts-page-carts-375-editorial.png`), each with a `base-` twin. The base Start 1024-editorial shot caught the dialog while its
channels loaded (a spinner): timing, not a fault. Plus `measure.json` (every box and hit row).

## The commander's command (from `e2e/`, world seeded on the same database)
`SR_P102_ARCADM_ON=1 SR_E2E_BASE_URL=http://localhost:5784 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P102_ARCADM_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-arcadm npx playwright test tests/p102-arcadm-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(add `SR_P102_ARCADM_PHASE=base` for a base run). It registers nobody (owner session only). It makes two Coming soon entries and one
second server under its own names and removes them in a `finally`.

## Text for 453 (Arcade part)
"The Arcade's three raw selects (the server picker and the cartridge Kind in `ArcadeSections.tsx`, the server picker in
`StartInDialog.tsx`) are the kit's `NativeSelect`: 40 px at 1024 (unchanged), 44 px at 375 (was 40), hit test 0 fails at 375."

## Proposed rows
- SEC-AUD-455 (proposed): the Start dialog's channel rows (`.arc-start-row`) are 36 px tall on a phone and fail the hit test (the same in the base); not mine this wave.
- Note: at 1024 (pointer width) the hit test also lists 28 px buttons and the cartridge dialog's Close as fails, base and after alike; the 44 px rule is a phone/touch rule.
- Threat model: none.

## Changelog sentence
Admin > Arcade's Feature, Release and Remove buttons are 44 px tall and 8 px apart on a phone, and the Arcade's server and kind drop-downs have the kit's look and a 44 px touch size.

## Owner questions
None.

## Clean-up and lock
Database `sr_p102_arcadm_ui` dropped; folder `p102-arcadm-bin` deleted; no stack running. `heavy.sh status` before the reply: see the last line of this file.

`heavy.sh status` at 10:07, before the reply: "library 10:06" and "settings 10:06" (no hold in my name; my slot was released at 10:03 with "released").
