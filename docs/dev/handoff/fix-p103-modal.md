# Wave 103 handoff: modal (SEC-AUD-460, D-979)

Clock: started 11:56, stack round 11:58 to 12:02, commit 12:04. Branch `fix/p103-modal`, commit `SEC-AUD-460 kit Modal: the phone Close button is raised above the body`.

## Verdict: both, by width
- **Result A (real, at 375).** In the base the Close button (44 x 44, `margin-block: -6px`) sticks 6 px out of the header's bottom edge. The body comes later in the dialog and lies over those 6 px: `elementFromPoint` at the button's lower point returns `div._body`, and a real tap on its lowest row does not close the dialog. Seen with the Lounge's Create Poll (both Lounge themes) and the long-body case. The cause in one sentence: the negative margin pushes the button past the header into the body, which paints later and is not under it.
- **Result B (at 1024, not a Modal fault).** The button is 32 x 32 there; the probe's points are 21 px from its centre, outside the box, so the taker is the header (above) and the body or dialog (below). Same in base and after, in all three modals. Nothing built for 1024 (SEC-AUD-467 is probe's). A real mouse click at the centre closes the dialog in base and after.
- The Arcade's List-a-cartridge dialog **passes at 375 in the base too** (its two-line title makes the header 60 px, so the button's box stays inside the header, overlap 0); its fail is B only.

## Item 1 figures (base; phone = 375 touch, Lounge Light and Dark equal)
| modal | width | Close box | header box (y, h) | body box (y, h) | body over Close | hit test Close | taker | real tap on lowest row |
|---|---|---|---|---|---|---|---|---|
| Create Poll | 375 | 301,135.5 44x44 | 121.5, 52 | 173.5, 448 | 6 px | fail (below) | `div._body` | not closed |
| Create Poll | 1024 | 740,123.5 32x32 | 103.5, 52 | 155.5, 440 | 0 | fail (above, below) | header, body | closed (centre click) |
| List a cartridge | 375 | 301,128.4 44x44 | 110.4, 60 | 243.6, 458 | 0 | pass | none | closed |
| List a cartridge | 1024 | 740,138.8 32x32 | 118.8, 52 | 222.3, 427 | 0 | fail (above, below) | header, dialog | closed |
| Long body (Create Poll in 420 / 400 px tall viewport; body 267 px, scrollHeight 448) | 375 | 301,31 44x44 | 17, 52 | 69, 267 | 6 px | fail (below); scrolled to the end: fail, taker the poll's fieldset in the body | body, fieldset | not closed |
| Long body | 1024 | 740,37 32x32 | 17, 52 | 69, 247 | 0 | fail (above, below); scrolled: same | header, body/fieldset | closed |

Computed (base): header, body, button all `overflow` visible/auto/visible, `position: static`, `z-index: auto` (the body `overflow-y: auto`).

## Item 2: the diff in words
`Modal.module.css`, inside the existing `@media (max-width: 720px)` block only: `.close` gets `position: relative; z-index: 1`, so the button is above the body and the whole 44 px box is the button's. No other rule, no markup, nothing above 720 px, no app section or theme named (the sheet was already the kit's). Header, title, body and dialog boxes are equal to the base to the pixel in all nine configs (the spec compares them with `figures-base.json`: `problems: []`).

After (same configs): at 375 every Close hit test **pass**, 0 takers, in Light and Dark, all three modals; a real tap on the lowest row (y = bottom - 1.5 px) closes the dialog in all six 375 configs; with the long body scrolled to the end the Close still passes (Light and Dark). At 1024 the figures equal the base (fail from the 21 px reach, taker header/body; result B).

## Item 3: build
NOT RUN. `heavy.sh run modal ... pnpm --filter @sideroom/web build` was called 7 times between 12:04 and 13:30 and answered exit 75 (busy) every time (the other slot was held by other builders each time, or slot 2 was refused by the memory gate); I did not hold a slot for it. So no first-paint figure after, no bundle-budget, entrance-isolation or staff-delivery result. Expected effect: the change is 3 lines of CSS inside an existing `@media` block, no bytes of JS; the Modal sheet is part of the kit stylesheet, which public pages already load. The commander runs: `bash cli-agents/heavy.sh run commander bash -c "cd <tree> && npx pnpm@10.12.1 --filter @sideroom/web build"`, then `node scripts/check-bundle-budget.mjs`, `node scripts/check-entrance-isolation.mjs`, `node scripts/check-staff-delivery.mjs` (before: 218 419 B gzip first paint, room 37 581 B). This is the brief's "a miss is listed, not chased".

## STACK-ROUND
One round, 11:58 to 12:02 (slot 1; `heavy.sh status` after acquire: `modal 11:58`), database `sr_p103_modal_ui`, ports 8807 / 5807 / 9507, sweep world seeded with no FAIL line. Base phase twice (the first base run had a wrong body selector in the spec's own facts, fixed, base re-run), then the fixed CSS copied in and the after phase. Stack stopped by PID (the server PID that was listening, 48716, and Vite), ports 8807 / 5807 / 9507 free, database dropped, released at 12:01.

## TYPECHECK
Not needed: no `.ts`/`.tsx` under `apps/web` or `packages` edited (CSS only). e2e `tsc --noEmit -p tsconfig.json` (from `e2e/`): exit 0, 12:04.

## Checks (on the tree as committed)
- `npx prettier --check` on the CSS and the spec: clean after `--write` (12:04). Kit eslint: no `.ts/.tsx` of mine.
- Kit unit tests that render the Modal, unedited: `zoom.test.tsx` 8, `labels.test.tsx` 4, `overlayStack.test.tsx` 3, `overlays.test.tsx` 17: all passed (12:04).
- `node scripts/check-css-tokens.mjs`: exit 0 (73 files clean). `node scripts/check-doc-links.mjs`: ok.

## Shots (`visual-sweep-shots/p103-modal`, 15 PNG, all opened and looked at)
After: `poll|cart|long-375-lounge-light.png`, `...-375-lounge-dark.png`, `...-1024.png` (9). Base: `base-poll|cart|long-375-lounge-light.png` and `base-...-1024.png` (6; the base Dark ones are not kept, the figures hold them). Seen: the header, title and close glyph are in the same place in every base/after pair; the dialog is whole and the Close glyph sits at the top right; nothing draws over it; the long dialogs at 420 / 400 px show the body cut by the footer as before. The cartridge dialog is in the Arcade's shipped theme (`lounge-editorial`, read back), so its Light and Dark shots are the same page (the section does not follow the main theme). No sideways scroll in any config (scrollWidth equals clientWidth).

## STALE
No earlier spec expects the Close to pass or to fail by name (grep of `e2e/tests/p10*`/`p9*` for Close fails: none). `fix-p102-cmd.md` and `fix-p102-arcadm.md` record it as a base fail; both are history, now answered.

## The commander's command
Stack as in the spec header (ports 8807 / 5807 / 9507, `SR_ADMIN_REQUIRE_REAUTH=0`, sweep world seeded), from `e2e/`:
`SR_P103_MODAL_ON=1 SR_E2E_BASE_URL=http://localhost:5807 SR_P103_MODAL_OUT=<folder> npx playwright test tests/p103-modal-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(`SR_P103_MODAL_PHASE=base` first only with the base CSS; with no `figures-base.json` the after run just asserts the Close on the phone.)

## Closing text for SEC-AUD-460
The kit Modal's Close button on a phone (44 x 44, `margin-block: -6px`) reached 6 px below the header and the body lay over it: a tap on its lowest pixels hit the body (Create Poll, long dialogs; the Arcade's cartridge dialog was spared by its taller header). `.close` is now `position: relative; z-index: 1` inside the 720 px block. Hit test 0 fails at 375 in Light and Dark, a real tap on the lowest row closes, header/title/body boxes unchanged. At 1024 the "fail" is the probe's 21 px reach on a 32 px button (SEC-AUD-467), not the Modal.

## Proposed rows
- None new. Note for SEC-AUD-467 (probe): 1024 Close fails are reach on a 32 px control.

## Changelog sentence
The dialog Close button on a phone is now a whole 44 px target: it sits above the dialog body, so a tap on its lower edge no longer lands on the body.

## Owner questions
None.

## Cleanup
Database dropped, `p103-modal-bin` deleted, scratch folder `SIDEROOM_wt/scratch-modal` is outside the repo. `heavy.sh status` at hand-back is read in the reply; no hold in my name (my only hold was the stack round, released 12:01).
