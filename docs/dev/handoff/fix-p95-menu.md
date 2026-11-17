# Wave 95 menu: SEC-AUD-430 (D-938), the kit's menu items are 44 px on a phone

Clock: started 14:52, lock 14:54 to 14:57 (one round, second run of the spec inside it), handoff 14:59.

## Done
- `packages/ui/src/primitives/Menu/Menu.module.css`: at the end, in the kit's phone condition (`@media (max-width: 720px)`, the one Button, Select, Input and Modal use), `.item { box-sizing: border-box; min-block-size: 44px; }`, one comment naming SEC-AUD-430 and D-938.
- Why `box-sizing` is there (one line beyond the task's rule): `.item` is content-box. The task's bare `min-block-size: 44px` measured 52 px (44 + 2 x 4 padding), my first run. With `border-box` the row is exactly 44 px. Nothing else changed (padding, font, gap untouched). The sheet has one row class (`.item`) for every kind of row (menuitem, checkbox, radio, submenu trigger); label and separator have their own classes and are untouched.
- Spec `e2e/tests/p95-menu-shots.spec.ts` (new): base (rule switched off in the loaded sheet, same tree) and after in one run.

## Base (375 touch, Lounge Light, ran it; takers from the shared `hitTest` per `[role=menuitem]`, because `hitTestAll` takes buttons only)
- Role menu (Add role in dan's popout): 4 items, each 186 x 40, all fail: first item's "above" is the menu itself, every other point is the neighbouring item (Regulars, Channel Keepers, ...). Menu box 200 x 174 at (42, 521), inside the viewport.
- Long menu (a message's "More" menu, 14 items): each 186 x 40, all 14 fail, takers the neighbour item (first: the menu).
- 1280 Editorial role menu: 4 items, 186 x 40.

## After (same run)
| case | items | box | hit test | menu vs viewport | sideways |
|---|---|---|---|---|---|
| role menu 375 Light | 4 | 186 x 44 each | 4 pass | 200 x 190, inside | 0 |
| role menu 375 Dark | 4 | 186 x 44 each | 4 pass | inside | 0 |
| long menu 375 Light | 14 | 186 x 44 each | 14 pass | 200 x 643 at y 161 to 804 of 812, inside, does not scroll inside itself | 0 |
| role menu 1280 Editorial | 4 | 186 x 40, equal to the base | (phone rule; not ruled) | inside | 0 |
- Flows: choosing "Spare Keepers" adds the role (checked through the API, then reset); in the long menu "Copy Text" closes the menu.
- The 1280 shot shows menu rows 40 px (unchanged), no crowding. At 375 the after menus are taller by 4 x 4 = 16 px (role) and 14 x 4 = 56 px (long); nothing touches or is cut; I opened the base and after of the role menu, they differ only in row height.

## Is the CSS in the last commit the CSS measured?
Yes: the second run (14:56 to 14:57, 1.1 min, passed 1 of 1) used the final CSS (HMR; fresh contexts). The first run (rule without `box-sizing`) is not the commit.

## Who loads the sheet
`Menu.module.css` is imported only by `packages/ui/src/primitives/Menu/Menu.tsx`, which `packages/ui/src/index.ts` re-exports; 26 files in `apps/web/src` use DropdownMenu or ContextMenu. `git grep` finds no use in `apps/web/src/public` or `components/entrance*`; the commander's build on the merged tree gives the first-paint figure (I ran no build). The rule names no app section and no theme.

## Shots (`visual-sweep-shots/p95-menu`, 5, all opened)
role-menu-375-light-base, role-menu-375-light-after, role-menu-375-dark-after, long-menu-375-light-after, role-menu-1280-after. (`measure.json` holds every figure; `ids.json` is the spec's state.)

## Checks (on this tree, after my last edit)
- e2e `npx tsc --noEmit -p tsconfig.json`: 14:58, exit 0.
- prettier --check on both files: exit 0. `check-css-tokens.mjs`: clean, exit 0. `i18n-audit.mjs --max 0`: TOTAL 0. `check-doc-links.mjs`: ok.
- `packages/ui` vitest `menus.test.tsx`: 10 of 10 passed (14:54, before the `box-sizing` edit; the edit touches a phone-only rule jsdom does not apply).
- eslint on the spec: not run (the web package's eslint ignores `e2e/`).
- Not run: a web build (not mine).

## Commander's command (from `e2e/`, the world seeded, SR_CLI_BIN, DATABASE_URL set; delete `ids.json` in the shots folder on a fresh database)
`SR_E2E_BASE_URL=http://localhost:5731 SR_P95_MENU_ON=1 SR_P95_MENU_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p95-menu npx playwright test tests/p95-menu-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

## Docs
None state the menu row height beyond the decisions log (not mine). Nothing to change.

## Proposed rows
- SEC-AUD-430 (close): the kit's menu items (`DropdownMenu`, `ContextMenu`) were 40 px tall at 375 and each failed the 21 px hit test against its neighbour item; now `min-block-size: 44px` with `border-box` under `max-width: 720px`; every item 44 px and passes (role menu and the 14-item message menu), desktop unchanged (40 px). D-938.
- Changelog sentence: On a phone, menu rows are 44 px tall, so a tap lands on the row you aim at.

## Owner questions
None. Note for the commander: the 1280 desktop row is 40 px (content-box 32 + 8 padding), not 32 as the plan says; unchanged.

## Cleanup
Stack stopped by PID tree (ports 8731, 5731, 9431 free), lock released, `sr_p95_menu_ui` and `p95-menu-bin` removed (see the reply for the final state). No stash touched.
