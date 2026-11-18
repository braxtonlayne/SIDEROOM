# fix-p104-chip (D-993, D-994; SEC-AUD-480, 483)

Solo stack builder. Started 14:12, last round ended 14:42, handoff 14:47.

## Done (commits on `fix/p104-chip`)

- `1efcaadf` D-994: the tier list's Move control is a menu button (kit `DropdownMenu`, one radio group) on the chip.
- `f039a5f9`, `9eadd97b` D-994: the spec `e2e/tests/p104-chip-shots.spec.ts` and its fixes.
- A later D-994 commit: the phone layout (`.sr-room-zone { min-inline-size: 0 }`, `.sr-room-chip { max-inline-size: 100% }` in the 720 px block).
- `3b1b5c52` D-993 (the card inputs on `--ui-bg-raised`) and `88e41568` its revert. **D-993's cure fails its own check (Lounge Light placeholder 4.61 to 4.15:1), so it was put back. The answer is "accept the two grounds"** (figures below).

## Item 1: D-994

Build, `TierListTool.tsx` `ItemChip`: a `DropdownMenu` whose trigger is `<button type="button" className="sr-room-move" aria-label="Move <name>">` holding `<Icon name="chevron-down" size={16} />` (no chip tier text, no new string). The menu is one `radio-group` (`value = item.tier`, `onValueChange = onMove`): `Unranked` first, then each tier's name; the kit draws the current one as the checked radio (a bullet). No kit change was needed. Drag guard: the button's `onPointerDown` sets a ref and the chip's `onDragStart` cancels the drag while it is set (the base needed none: a select swallows the press itself). Focus after a move: the chip is a new node in another row, so the editor keeps a 3 s `refocus` request and, when the item shows in its new tier, focuses **that chip's Move button** (the button, never `body`).

Sheet, `rooms.css`: `.sr-room-move` is the button's box, 20 x 20 (`1.25rem`), the chip's border and ground tokens, and the existing kit focus ring rule. End block, `max-width: 720px`: chip `padding-block: 0` and `max-inline-size: 100%`; button 44 x 44 with `margin-block: -1px` (the chip is exactly 44 px); zone `min-inline-size: 0` (without it the zone outgrew its row and clipped the chip: first phone round).

### Figures, base to after (`figures-base.json`, `figures-after.json`)

| what | base | after |
|---|---|---|
| chip box at 1024 and 1280 | 247.5 x 28.4 (width follows the name) | 181.8 x 28.4 (narrower, same height) |
| Move control at 1024 and 1280 | select 80 x 21 | button 20 x 20 |
| x button at 1024 and 1280 | 21.3 x 22.4 | 21.3 x 22.4 |
| tier rows' heights at 1024 and 1280 (5 rows) | 58 each | 58 each |
| tier label column | 96 | 96 |
| chip at 375 | 247.5 x 28.4, wider than its row (x cut by 46 px) | 201.5 x 44, inside its row (right edge 338.5 of 339) |
| Move at 375 | 80 x 21 | 44 x 44 |
| tier rows at 375 | 58 each | 58 each (the zone's 3 rem minimum holds the 44 px chip) |
| sideways scroll at 375 | none | none |
| hit test of the tool (`.sr-room`), 1024, `reach: 'box'` | 28 pass, 0 fail | 28 pass, 0 fail |
| same at 1280 | 28 / 0 | 28 / 0 |
| same at 375, default reach | 1 pass, 27 fail | 4 pass, 24 fail |
| open menu, `hitTestAll(page, menu, { also: MENU_ITEM_SELECTOR })` | none | 1024: 6 pass; 375: 6 pass (items 40 px at 1024, 44 px at 375) |

At 375 the 24 remaining fails are not the Move button. They are the room bar's buttons and title field, the tier name inputs and remove buttons, New row, Add row, Item, Emoji, Image, Add item and the two Post buttons, each 21 to 28 px tall, failing in the base too (takers named in `figures-after.json`, `375-lounge-light.hit.fails`: the neighbouring controls or rows, for example Delete takes points of the title field). **No Move button fails and none takes a point of another control.** The chip's x (22 px in a 44 px chip) still fails the 44 px rule at 375 as in the base; its taker is now its own chip, where the base's were the zone above and below.

Rows rule (D-990): what moved is the chip's width (narrower) and, at 375, its height (28.4 to 44). Rows, label column, x box, name field, Add, Channel and Mount did not move; the spec's compare reported no problem.

### Keyboard walk and drag (1024 Lounge Light, after only)

| step | result |
|---|---|
| Tab from the "New row" input twice (Add row, then) | focus on the Unranked chip's `Move <name>` button |
| Enter | menu opens, focus on its first item (Unranked, the checked one) |
| ArrowDown | focus moves down the list (the run landed on "C": the entry it recorded) |
| Enter | the item is in the chosen tier (list "C"); focus on that chip's Move button, in its new row |
| Space | menu opens |
| Escape | menu closes; focus on the same Move button |
| drag a chip by its text onto another tier | the chip moves (S to A) |
| press on the Move button and drag onto another tier | the chip stays in its tier; the menu is not open after the drag |
| click the button | opens the menu |

## Item 2: D-993

Confirm (computed `background-color`, `figures-base.json`; Invite card input, textarea and kit select; the room shell's item input beside its kit Channel and Mount selects):

| theme | input and textarea | kit select | card ground | room input | room kit select |
|---|---|---|---|---|---|
| Lounge Light | 255,255,255 | 242,243,245 | 255,255,255 | 255,255,255 | 242,243,245 |
| Lounge Dark | 49,51,56 | 43,45,49 | 43,45,49 | 49,51,56 | 43,45,49 |
| Editorial | 35,34,29 | 28,27,23 | 28,27,23 | 35,34,29 | 28,27,23 |

The select's ground is `--ui-bg-raised`. The mismatch is real in all three themes, in the card and in the room shell.

Ratios from `contrast.ts`, base to the trial with `--ui-bg-raised` (`figures-after-d993-trial.json`):

| theme | text | placeholder | border on the card |
|---|---|---|---|
| Light | 12.64 to 11.39 | **4.61 to 4.15** | 1.61 to 1.61 |
| Dark | 9.36 to 10.21 | 2.74 to 2.99 | 1.35 to 1.35 |
| Editorial | 12.84 to 13.89 | 3.46 to 3.74 | 1.47 to 1.47 |

Light's placeholder falls under 4.5:1, which the task calls a fault of this cure. **The token was put back** (`88e41568`): `cards.css` equals the base, and `rooms.css:24` was not changed (same mismatch, and the same ground would give the same 4.15 in Light). Dark's and Editorial's placeholders were under 4.5 in the base already. In the trial every Invite card box equalled the base within 0.5 px.
Card tools with an input or textarea under `.sr-card`: the Invite card only (`InviteTool.tsx`); `press.tsx` has a link class only.

## Who imports what

`TierListTool.tsx` is the lazy tools chunk (`tools/registry.ts:414`, `import('./tierlist/TierListTool')`); `DropdownMenu` and `Icon` come from the kit. `rooms.css` is imported by `RoomShell.tsx:7` and `lounge/rooms/ListeningRoomCard.tsx:11` imports its own `./rooms.css` (a different file); `cards.css` by `InviteTool.tsx:9`. Nothing first paint or a public page loads; no web build run.

## STACK-ROUND

Own database `sr_p104_chip_ui`, ports 8802, 5802, 9502, sweep world seeded each time with no FAIL line. Rounds: 14:17 to 14:25 (slot at once, `status` showed `chip 14:17`; base x2, after x3), 14:28 to 14:29 (the phone chip), 14:40 to 14:42 (the final one; I waited about 8 minutes for the slot, `acquire` answered 75 several times while `runner` and `loungeshots` held). Each ended with `release`; last `status`: `free`.

## TYPECHECK

`bash cli-agents/tc.sh run chip bash -c "cd C:/Users/braxt/Desktop/SIDEROOM_wt/p104-chip/apps/web && npx tsc --noEmit -p ."`, 14:16, exit 0, after the last `.tsx` edit (later edits were CSS and the spec only). Other checks after the last edit: prettier --check exit 0; eslint (the web config) on `TierListTool.tsx` and the spec exit 0; e2e `tsc` exit 0; `check-css-tokens` clean; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok; vitest `tools/__tests__/rooms.test.tsx` 15 passed and `cards.test.tsx` 16 passed (neither finds the Move control).

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-chip`, 10 PNGs, all opened)

- `base-tier-1024-lounge-light.png`: selects "S" and "A" in the chips.
- `tier-1024-lounge-light.png`: a small round chevron button in each chip, chips narrower, rows unchanged.
- `tier-1024-lounge-dark.png`: same; button border and ground legible.
- `tier-375-lounge-light.png`: 44 px buttons, 44 px chips wholly inside their rows, x beside the button, no sideways scroll.
- `tier-menu-1024-lounge-light.png`, `tier-menu-375-lounge-light.png`: menu with Unranked (bullet) and S to D, pointer on "B" (highlighted), no submenu.
- `base-invite-1024-lounge-light.png`: input and textarea white beside grey selects.
- `invite-1024-lounge-light.png`, `invite-375-lounge-light.png`, `invite-1024-lounge-dark.png`: same as the base (D-993 put back). The trial shot with one grey ground was looked at in the trial round and deleted.
No `room-` shot: `rooms.css:24` is unchanged.

## STALE

`e2e/tests/p103-tools-shots.spec.ts`: the `tierlist` target (`:269-283`, `selects: 3`, `shotSelect: 1`) and the expectation rows for `tierlist/TierListTool.tsx` (`:686`) count the Move chip as a select. With the chip a button the drawer holds two selects (Channel, Mount), so that target reports "2 selects, wanted 3". A stale expectation of D-972 that D-994 changed on purpose; not edited. That spec records no input colours.

## The commander's command

From `e2e/`, on a stack with the sweep world, on this branch: `SR_E2E_BASE_URL=http://localhost:<vite> SR_P104_CHIP_ON=1 SR_P104_CHIP_PHASE=after SR_P104_CHIP_OUT=<folder> npx playwright test tests/p104-chip-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`. For a base, first run it with `SR_P104_CHIP_PHASE=base` with the three files at `ccb3bacc`.

## Closing texts for the rows

- SEC-AUD-480 (the Move chip clips Unranked): closed by D-994. The Move control is a menu button (kit DropdownMenu, current tier checked) with no tier text on the chip, so nothing is clipped. At 1024 and 1280 the tier rows and the x box are unchanged and the chip is narrower; at 375 the button is 44 x 44, the chip 44 px and inside its row, no sideways scroll, no Move failure in the hit test.
- SEC-AUD-483 (the Invite card's selects draw a grey ground): not closed; accepted as two grounds for now. Measured in all three themes: field `--ui-bg` against select `--ui-bg-raised`. The one-token cure drops Light's placeholder from 4.61 to 4.15:1, so it was put back (D-993 reverted). Reopen when the placeholder colour is lifted.

## D-972 and D-973 wording

Remove from the list of raw selects that stay: "the tier list's Move chip, a compact control inside a 24 px chip, styled by `.sr-room-move` only". The chip is no longer a select (D-994: a menu button). The count of raw selects that can move stays at 31 minus one (the chip leaves the raw list).

## Changelog sentences

- D-994: The tier list's Move control is now a small menu button on each chip (a menu of Unranked and the tiers, the current one checked); on a phone it is a 44 px target and the chip fits its row.
- D-993: nothing shipped; the card fields keep their own ground, because the kit's field ground would drop the Lounge Light placeholder under 4.5:1.

## Proposed inventory rows

- Lift the placeholder colour of the tool cards' and rooms' inputs (Light 4.61, Dark 2.74, Editorial 3.46), then move their ground to `--ui-bg-raised` (`cards.css:26-35`, `rooms.css:18-24`): reopens SEC-AUD-483.
- The tier chip's x button is 22 px tall at 375 inside a 44 px chip and fails the 44 px rule (as in the base).

## Owner question

The placeholder colour: Light passes by 0.11, Dark and Editorial already fail. Should the kit's or the rooms' placeholder colour be lifted so the card field ground can match the kit's? (Default taken: no change.)

## Not done

No second cure. AGENTS.md needs nothing. The stash list was not touched. No 429 happened.

## Clean-up and PROCESSES

The stack was stopped by Windows PID tree each round; ports 8802, 5802 and 9502 had no LISTENING line afterwards; database `sr_p104_chip_ui` dropped; `p104-chip-bin` deleted. Final process listing and `heavy.sh status`: in the reply.
