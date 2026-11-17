# Wave 94 lounge: SEC-AUD-426 (D-932)

Builder: lounge. Started 13:55; one stack round 13:57 to 14:00 (lock `lounge`, released after the stack was stopped); handoff 14:01. Web only, one CSS file and one spec: no cargo, no web build, no vitest.

## Base (ran it, same run: the SEC-AUD-426 media rule switched off in the loaded sheet, `mediaText = 'not all'`, same tree, same DB)
Seeded world, no `FAIL` line. dan (a second guild member) holds two roles, "Garden Wardens" and "Night Stewards" (14 characters each); a third, "Spare Keepers", is left to add. 375 touch Lounge Light, dan's popout from the channel's member list, `hitTestAll` on `.lng-pop` (`openCollapsed` opened 0, 0 not reached):
- Close 44 x 44 pass; Note 308 x 44, Message 308 x 44, View Full Profile 120.7 x 44 pass (wave 93's).
- Remove role Garden Wardens 16 x 16 FAIL (above: `h3.lng-pop-h` "Roles"; below: `ul.lng-pop-roles`). Remove role Night Stewards 16 x 16 FAIL (same takers).
- Add role 34 x 22 FAIL (above: `span.lng-pop-role-dot`; below: `div.lng-pop-body`).
- The two chips sit on one line (each about 150 px wide); "Add role" is on a second line; the gap between the lines is 4 px. Popout 342 x 517, inside the viewport, no sideways scroll.

## Built (`profile-p11.css`, one `@media (max-width: 768px)` block at the end, comment names SEC-AUD-426 and D-932; every selector under `.lng-pop`)
- `.lng-pop-roles { row-gap: 24px }`; `.lng-pop-role-add { justify-content: center; min-block-size: 44px; min-inline-size: 44px }`; `.lng-pop-role-x { position: relative }` with a centred 44 px `::after`. Chips' drawn size unchanged (remove button still 16 x 16).
- Note: the row-gap is 24 because the remove ::after reaches 22 px below the chip centre and the add button needs its own 44 px above; measured, not guessed after the fact (the round passed with it).

## After (375 Lounge Light and Dark)
- Every control passes, each its own taker, 0 fail, 0 not reached: Close 44, both remove buttons (16 x 16 drawn, pass), Add role 44 x 44, Note 308 x 44, Message 308 x 44, View Full Profile 120.7 x 44. Light and Dark the same figures.
- Popout 342 x 558 (+41 px against the base 517), bottom inside the viewport, does not scroll inside itself, no sideways scroll (0). Line gap 4 -> 24.
- Flow (Light, real touch): "Add role" opened its menu, choosing "Spare Keepers" added it (API: held 3, has the third); a touch tap at the centre of the first chip's remove button removed "Garden Wardens", "Night Stewards" stayed (held 2).
- Kit: the open menu's items are `role="menuitem"` and `hitTestAll` finds no `button` in it (0 rows), so I measured the boxes: 4 items (Night Shift, Regulars, Channel Keepers, Spare Keepers) each 40 px tall. Listed as "kit" (40 px, under 44 by the box; not ruled, not hit-tested).
- 1280 Editorial: every control box and the popout box equal to the base (asserted in the spec, passed).
- **The CSS in my commit is the CSS I measured** (written before the round, nothing changed after).

## Who imports the file
`git grep -n profile-p11.css`: only `apps/web/src/lounge/profile/ProfilePopout.tsx:20` (the Lounge chunk, lazy). No first-paint or public file touched, no import added, no web build.

## Spec and checks (all on my tree after the last edit)
`e2e/tests/p94-lounge-shots.spec.ts` (new; variables `SR_P94_LOUNGE_ON`, `_OUT`, `_BASE`; header has the command). Run: 1 passed (45 s). First run: failed on the flow only (a spec error: the menu's first item is the world's "Night Shift", not my role; fixed by picking "Spare Keepers" by name, same round). The base figures above are the "watched fail without the change" (rule off in the same run).
- e2e `tsc` (from `e2e/`, `npx tsc --noEmit -p tsconfig.json`): 14:00, exit 0. prettier --check on both files: clean. `check-css-tokens`: exit 0. `i18n-audit --max 0`: exit 0. `check-doc-links`: exit 0. Web `tsc` and eslint: not run (no `.tsx` changed).

## Shots (`visual-sweep-shots/p94-lounge`, 5, all opened by me)
`base-popout-phone-lounge-light`, `popout-phone-lounge-light`, `popout-phone-lounge-dark`, `add-role-menu-phone-lounge-light`, `popout-desktop-lounge-editorial`. (Extra, not opened, figures in `measure.json`: `base-popout-desktop-lounge-editorial`.) What I saw: base, the small plus button under the chips; after, a 44 px square plus button with air above and below, the chips unchanged; Dark the same; the menu opens under the button and stays inside the popout/screen; 1280 shows the chips and the 34 x 22 plus as before.

## Commander's command
In the spec header: `SR_E2E_BASE_URL=http://localhost:5721 SR_P94_LOUNGE_ON=1 SR_P94_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-lounge npx playwright test tests/p94-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` from `e2e/`, with `SR_CLI_BIN`, `SR_DATABASE__URL`, `DATABASE_URL`; delete `ids.json` in the shots folder on a fresh database. The roles and dan stay in the database (no delete helper).

## Proposed rows
- SEC-AUD-426: closed (D-932): on a phone the guild popout's "Add role" (34 x 22 -> 44 x 44) and a role's remove button (16 px, 44 px ::after) pass the hit test; popout +41 px; 1280 unchanged. `profile-p11.css`. Handoff `fix-p94-lounge.md`.
- New (number from SEC-AUD-430): the kit's `DropdownMenu` items are 40 px tall at 375 (the role menu's four items); the shared hit test finds no `button` in it. Kit, not fixed here.

## Changelog sentence
On a phone the guild profile popout's "Add role" and role remove buttons now have 44 px touch targets.

## Owner questions
None.

## Cleanup
Stack stopped by PID tree (8721, 5721, 9421 free), `sr_p94_lounge_ui` dropped, lock released, `p94-lounge-bin` and `p94-lounge-scratch` to be deleted (see the commit time). `p92-base-bin` and `p80-base-bin` untouched. No stash touched.
