# Wave 103, arcade: SEC-AUD-456 (D-981) and the Arcade part of SEC-AUD-464 (D-972)

Branch `fix/p103-arcade`. Clock: started 11:56, code and spec committed by 12:00, round 1 12:02 to 12:06, round 2 12:31 to 12:32 (failed: stack 429), round 3 13:36 to 13:38. The long gap was the lock queue.

## Done (commits, in order)
- `fd69a331` SEC-AUD-456 arcade.css: block at the very end, inside `@media (max-width: 40rem)`: `.arc-start-row { min-block-size: 44px; }`.
- `752ee5cd` SEC-AUD-464 arcade.css: `.arc-field > span` became `.arc-field > span:first-child`; `.arc-field select` and `.arc-field select:focus-visible` deleted; the wave 102 undo block (`.arc-field > span:has(> select)`, `.arc-field > span > select`, with its comment) deleted.
- Spec `e2e/tests/p103-arcade-shots.spec.ts` (three later commits: new, keyboard-run fix, 429 wait).
- Not changed: `StartInDialog.tsx` (no edit needed), `ArcadeAdmin.tsx`, `ArcadeSections.tsx`, the unit tests.

## The diff in words and every dependant
- `git grep -n "<select" -- apps/web/src/arcade` finds none: the three selects (`StartInDialog.tsx` server picker, `ArcadeSections.tsx` Kind and server picker) are all the kit's NativeSelect, so both element rules went.
- Every `.arc-field` (git grep): `ArcadeAdmin.tsx:113,153` (`div.arc-field`, label then the kit `Select`, whose root is a `div`, so `> span` never matched it), `ArcadeSections.tsx:557` (`label.arc-field`: label text span first, then the NativeSelect wrapper span) and `:701` (`label.arc-field.arc-server-pick`, same), `StartInDialog.tsx:256` (same). In every `label.arc-field` the label text span is the first child, so `span:first-child` keeps the label look and cannot match the wrapper (the second child). The kit TextInputs of the dialogs are not `.arc-field`; their labels are the kit's.
- Who imports `arcade.css`: `ArcadePage.tsx`, `ArcadeShell.tsx` (Arcade chunk, lazy through `emu/routes.tsx`), `showroom/PreviewArcade.tsx` (lazy from `ArcadePage`), `admin/AdminApp.tsx` (Admin chunk, lazy through `StaffDoor.tsx`). Nothing first paint or public loads it. No web build run.
- No class was dropped from any select (the Arcade selects carried none).

## Figures (base = `arcade.css` from a copy of the base text kept outside the repo, never a stash)
Start dialog on the owner world, with two servers of the spec's own: `p103-arcade-mid-*` (12 channels; 14 rows with the world's) and `p103-arcade-large-*` (25 channels; 27 rows), both removed in the spec's `finally`.

| case | rows base | rows after | hit-tested controls | row fails base | row fails after |
|---|---|---|---|---|---|
| 375 touch, shipped (editorial) | 14 x 36 px | 14 x 44 px | 17 | 14 (takers: the row above or below, or the group section) | 0 |
| 375 touch, light | 14 x 36 | 14 x 44 | 17 | 14 | 0 |
| 375 touch, large (27 rows) | 27 x 36 | 27 x 44 | 30 | 27 | 0 |
| 1024, shipped | 14 x 36 | 14 x 36 | 17 | 14 | 14 (identical to the base: 36 px at 1024, SEC-AUD-467, noted not a fault) |
| 1024, light | same | same | 17 | 14 | 14 |

- Large case: the dialog stays 782 px high (viewport 812); the text list scrolls inside (scrollHeight 924 over 294); Cancel and Close pass their hit rows (0 fails over 30 controls).
- Mid at 375: the dialog grows from 740.5 to 772.5 px and re-centres (top 35.8 to 19.8), so the picker sits 16 px higher; its box (303 x 44), padding (`12px 36px`), type (16 px, no text-transform), radius and border equal the base. At 1024 the dialog (562 x 714.8) and the picker (520 x 40) equal the base.
- Sideways scroll: 375/375 and 1024/1024 in every case, base and after.
- The three selects equal the base in box, padding, type, label and wrapper look at 1024 (shipped and light) and 375: cartridge dialog Kind 448 x 40 and 303 x 44; page server picker 384 x 40 and 337.5 x 44; selected text clear of the chevron (select scrollWidth equals clientWidth); keyboard run (focus, ArrowDown changes the value, ArrowUp puts it back) works on all three; the cartridge dialog's hit test is 7 controls with 6 fails at 1024 (base too) and 0 at 375, equal to the base.
- Every label of the List a cartridge dialog (kit TextInput labels Title, Description, Shelf path; the `.arc-field` span "Kind") and of Admin > Arcade (six labels) equals the base in transform (uppercase), size 12 px, weight 600, letter-spacing, colour and box.
- The large case skips the keyboard run by design (the large server is the last option). Round 1 ran it and moved the picker back to the mid server, so its large shot was wrong: a spec error, fixed in `858635b2` and shot again in round 3.

## STACK-ROUND
- Round 1: acquire 12:01, release 12:06 (`status` read `arcade 12:01`). The seed printed no FAIL. Base, after and `p102-arcadm-shots` (after) all passed. Round 2 (12:31 to 12:32): both phases failed on `POST /guilds` 429 (the stack's limit on server creation); the spec now waits 30 s and retries. Round 3: acquire 13:36 (`status` read `arcade 13:36`), base and after passed (1.2 m and 38 s), released 13:38. Final `heavy.sh status` read `free`.
- Ports 8804 / 5804 / 9504 free afterwards; database `sr_p103_arcade_ui` dropped; `p103-arcade-bin` deleted.

## TYPECHECK
Not needed: no `.ts` or `.tsx` under `apps/web` or `packages` was edited (CSS and an e2e spec only). e2e `tsc --noEmit -p tsconfig.json` from `e2e/`: exit 0 (13:39).

## Other checks (on the last commit)
prettier --check on `arcade.css` and the spec: clean. `node scripts/check-css-tokens.mjs`: exit 0. `node scripts/i18n-audit.mjs --max 0`: TOTAL 0. `node scripts/check-doc-links.mjs`: ok. Unit tests, single files, unedited: `startIn` 6, `arcade` 40, `arcadeAdmin` 5, `arcadePage` 7, `lobbies` 14: all passed. eslint: no `.tsx` edited.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-arcade`, 14 PNG)
Opened and looked at: `base-start-375` (rows tight at 36 px, list cut at the fold), `start-375` (taller rows, picker unchanged), `start-375-light` (Lounge Light, same layout), `start-375-large` and `base-start-375-large` (large server: voice then text lists, list scrolls inside the dialog, no overflow), `start-1024` (unchanged look), `base-cartridge-form-1024`, `cartridge-form-1024`, `cartridge-form-375` (identical to the base: small-caps labels, Kind picker with the chevron clear of the text), `admin-arcade-1024` (Released as pickers and labels as before). Not opened: `base-start-375-light`, `base-start-1024`, `base-cartridge-form-375`, `base-admin-arcade-1024` (their figures equal the after's). I opened the round 1 files of the non-large shots; round 3 rewrote them from the same states.

## STALE (p102-arcadm-shots, after phase, round 1)
It passed (`1 passed`); its folder holds no base file, so it compared nothing. Nothing stale to report.

## The commander's command whole (from `e2e/`, world seeded, ports 8804 / 5804 / 9504, database `sr_p103_arcade_ui`)
`SR_P103_ARCADE_ON=1 SR_E2E_BASE_URL=http://localhost:5804 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P103_ARCADE_OUT=<folder> npx playwright test tests/p103-arcade-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (add `SR_P103_ARCADE_PHASE=base` after putting back the base `arcade.css`). If a 429 shows on server creation, the spec waits 30 s and retries.

## Closing text
- SEC-AUD-456: closed. In the Start dialog at 375 touch every channel row is 44 px (was 36) and the hit test has 0 fails (was 14 of 14 rows, and 27 of 27 with a 25-channel server); above the phone width nothing changed.
- SEC-AUD-464 (Arcade part): closed. `arcade.css` no longer styles any select by element name, the label rule `.arc-field > span:first-child` cannot match the kit wrapper, and the wave 102 undo block is gone; the three selects and every label measure equal to the base.

## Proposed rows, changelog, questions
- Proposed rows: none new. Note for SEC-AUD-467: the Start dialog at 1024 still has 36 px rows, a 32 px Close and a 36 px Cancel; the base has them too.
- Changelog sentence: The Arcade's Start dialog channel rows are 44 px tall on a phone, and the Arcade sheet no longer styles selects by element name.
- Owner questions: none.
