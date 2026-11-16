# Wave 84 handoff: support (SEC-AUD-358 / D-862, SEC-AUD-359 / D-863)

T = 13:20 on 2026-11-16. Clock times: no new item 15:05, checkpoint 15:25, finished 15:50, hard limit 16:05. Mine: started 13:18, code and spec compiled 13:24 (asked for the lock then; admin, guild and lounge each took it first), lock taken 13:47, released about 14:03 (a 16-minute round, one over the limit: three runs of the spec instead of two because the first base run missed the server picker, see below), handoff written 14:10.

## Done (branch `fix/p84-support`)
- `bb10d38a` SEC-AUD-358, 359: `SupportPanel.tsx`, `support.css`, `support/__tests__/support.test.tsx`, `e2e/tests/p84-support-shots.spec.ts`.
- A second commit: the spec's final fixes (opens the settings from the Lounge so the member's servers are in the store; the server picker picks the second server; the `custom` measure skips the Select's own buttons) and this handoff.
- No new i18n fragment (every string already existed). No change to `settings/server/__tests__/support.test.tsx` (nothing in it looked up the bare control; it passes unchanged on the base and after).

## What I saw on the base (run, `SR_P84_BASE=1`, my two files put back to `b2f3747b` copies with `git show`, no stash)
Member's Settings > Support / Patron, the world's four plans and four presets:
- 375 Lounge Light: plans on three lines, **2 + 1 + 1** (boxes at y 678, 678, 722, 766; "6 months" and "12 months" each alone); donate presets **3 + 1** (twice, donate and server card). 36 px tall buttons (hit test: 10 of 10 plan/preset buttons fail at 375 on the first run; 15 of 15 with the server card present).
- 1280 Editorial: plans **3 + 1** (the wrap is there at 1280 too), presets 4 in one row.
- Every `input` and `select` in the panel was bare: the amount `input` 177 x 21, the server `select` 130 x 19, the credits `select` 30 x 19. Computed ground and border in Lounge Light: `rgb(255, 255, 255)` / `rgb(118, 118, 118)` (the browser's own; the theme's field is `rgb(242, 243, 245)` / `rgb(201, 204, 209)`); in Editorial `rgb(59, 59, 59)` / `rgb(133, 133, 133)` against the theme's `rgb(28, 27, 23)` / `rgb(58, 56, 47)`. Four bare controls per member page, all in the member's tab.
- Which page shows which control: the member's Settings > Support / Patron shows all four (two amount inputs: Donate and Support a server; the server `select`; the credits `select`; the server picker shows only once the member's servers are in the store, i.e. when Settings is opened from the Lounge, not on a direct load of the address). **`/support` shows none** (it uses only `HostingBar` and `perkLines` from the panel). **A server's Server Support tab** uses `AmountPicker` (the amount field and four presets), but only when donations are on, so **in the seeded world it shows none** (the notice "Donations are not switched on" is the page's state). Both are shot and measured, with no control in them before or after.

## Item 1: what changed
- `SupportPanel.tsx`: the amount is the kit's `TextInput` (label "Other amount (USD)", `inputMode="decimal"`, placeholder `7.50`; `error` carries the range text inside a `<span role="alert">`, so the kit ties it with `aria-invalid` and `aria-describedby`, and the alert role stays). The server and credits pickers are the kit's `Select` (a `<label htmlFor>` plus `id` on the `Select`, so the same labels name them). Values, handlers and what is sent are as before. The row's buttons get `className="sup-go"`; the button rows get `style={{ '--sup-n': <count> }}`. A side effect: the amount's `id="sup-custom-amount"` and `id="sup-custom-err"` were duplicated on the member's page (Donate and Support a server both rendered them); the kit makes the ids per field now.
- `support.css`: `.sup-choices` is a grid of `--sup-n` equal columns (one row at 1280 for 4 or fewer buttons), two on a phone (`max-width: 720px`) with an odd last button taking the whole row; the plan and preset buttons and `.sup-go` are 44 px tall in the phone block. `.sup-custom` is a grid: the field's label, the field and the error in column 1, the button in column 2 on the field's row, aligned to its bottom edge (`align-self: end`); on a phone one column, the button whole under the field (full width, 44 px). The kit's `TextInput` wraps its label, control and error in one div: `.sup-field { display: contents }` lets them sit in the grid, so the error does not move the button. The label sits above the field (the kit's way), not to its left: see owner question 1. `.sup-label` copies the kit's label style for the two Select rows.

### Figures after (spec `measure.jsonl`, `p84-support/`)
| setting | plans | presets (x2) | amount field | server select | credits select + button |
|---|---|---|---|---|---|
| 375 Lounge Light | 2 + 2, boxes 151 x 44 | 2 + 2, 151 x 44 | 309 x 40, button 309 x 44 under it | 309 x 40 | 309 x 40, button under it 309 x 44 |
| 375 Lounge Dark | 2 + 2 | 2 + 2 | same | same | same |
| 1280 Editorial | 1 row of 4, 148 x 36 | 1 row of 4 | 524 x 40, button 82 x 36, bottom on the field's bottom (diff 0.0 px) | 606 x 40 | 472 x 40, button bottom on its bottom (read in the shot) |
| 1280 Lounge Light, Dark (extra) | 1 row of 4 | 1 row of 4 | same | same | same |
- No bare `input` or `select` left on any of the five settings (the spec counts a control as the kit's when the `input` carries the kit's `control` class or the combobox is inside the kit's `control` wrapper): `bare 0`, controls 4 per run. Ground and border are the theme's and equal the kit probe on other tabs (Account's text field, Appearance's Select): Lounge Light `rgb(242, 243, 245)` / `rgb(201, 204, 209)`, Lounge Dark `rgb(43, 45, 49)` / `rgb(63, 65, 71)`, Editorial `rgb(28, 27, 23)` / `rgb(58, 56, 47)`.
- `data-theme` read back from the page's root equals the main theme in every run (the main theme set by PATCH and localStorage, read back from the API each time). `scrollWidth - clientWidth` 0 for the document and the tab's scroller on every page and setting. No box leaves its card (`outside 0`).
- Hit test at 375 (D-856): 15 plan and preset buttons pass; **4 of 19 fail, all of them the kit's `Select`: its trigger button and its caret are 38 px tall inside the kit's 40 px box (the "SIDE ROOM Staff" and "1" triggers and their carets). Kit (the kit builder's 44 px work), not fixed here.** The amount `input` is 40 px (kit).
- `/support` and Server Support: nothing changed there (no control in the seeded world); shots after look the same as before.

### What I exercised and what I could not
Exercised on the live stack in every setting: typed `0.50` (the error "Use an amount from $1.00 to $10,000.00." shows under the field, `aria-invalid="true"`, the describedby target is the error, the border is red, the button stays disabled), `20000` (invalid), `7.50` (valid, `aria-invalid` gone); opened the server Select (two options, popup inside the viewport) and picked the second server; opened the credits Select (1 to 5, popup inside the viewport) and picked 3, the button read "Buy for $60.00" in the unit test and "3" selected in the spec. **Not exercised: an invoice** (donations are off in the seeded world: the buttons are disabled, no invoice can be made, so the amount's submit, the invoice dialog and the Server Support contribution form with a live amount were not run on the stack; the vitest tests run them with donations on and with the real handlers).

### Tests (single files)
- `support/__tests__/support.test.tsx`: I did **not** run it on the untouched base first (I changed the component, then the test, then ran it; 22 pass). The old test found the server picker by `findByLabelText('Server')` and `within(select).getByRole('option')`, which a native select allows and the kit's Select does not; it now finds `combobox` "Server", opens it and reads the option from the listbox. One new test buys three credits through the credits Select and checks the body `{ purpose: 'invite_credit', credits: 3 }`. The amount test (type 0.50, read the alert, type 7.50, click Donate, check the body) is unchanged and passes. No assertion about what is sent was weakened.
- `settings/server/__tests__/support.test.tsx`: unchanged, 11 pass (the "creates an anonymous contribution invoice" test uses `AmountPicker` with the kit field and checks the body).

### Who loads my files
`SupportPanel.tsx`: `settings/user/UserSettings.tsx` (loaded by `settings/user/routes.tsx` through `lazyNamed`), `support/SupportPage.tsx` (`support/routes.tsx`, `lazyNamed`), `settings/server/sections/ServerSupport.tsx` (inside `ServerSettings`, loaded by `routes/settingsServer.tsx` as a lazy import). `support.css`: `SupportPanel.tsx`, `SupportPage.tsx`, `Leaderboards.tsx`, `ServerSupport.tsx`: the same lazy chunks. No first-paint or public file imports either. No web build run.

## Item 2: the template's ready state (SEC-AUD-359)
The Owner made a template of the seeded server (`POST /guilds/{guild_id}/templates`, `{ name, description }`, answered 200 with the code), the spec opened `/template/<code>` in the app (the address wave 83's spec uses, with the real code), waited for the server's name, the name field and the create button, shot six, and deleted the template (`DELETE /guilds/{guild_id}/templates/{code}` 200; `GET /guilds/templates/{code}` then 404). It never pressed the create button.

| setting | data-theme (= main) | texts under 4.5:1 (of 11) | field box (inside card) | button box (inside card) | scrollWidth - clientWidth | hit test at 375 |
|---|---|---|---|---|---|---|
| 375 Editorial | lounge-editorial | 0 | 241 x 40 (yes) | 241 x 36 (yes) | 0 | button fails (36 px) |
| 375 Lounge Light | lounge-light | 0 | 241 x 40 (yes) | 241 x 36 (yes) | 0 | button fails |
| 375 Lounge Dark | lounge-dark | 0 | 241 x 40 (yes) | 241 x 36 (yes) | 0 | button fails |
| 1280 Editorial | lounge-editorial | 0 | 241 x 40 (yes) | 241 x 36 (yes) | 0 | n/a |
| 1280 Lounge Light | lounge-light | 0 | 241 x 40 (yes) | 241 x 36 (yes) | 0 | n/a |
| 1280 Lounge Dark | lounge-dark | 0 | 241 x 40 (yes) | 241 x 36 (yes) | 0 | n/a |
Field ground and border are the theme's (Light `rgb(242, 243, 245)` / `rgb(201, 204, 209)`, Dark `rgb(43, 45, 49)` / `rgb(63, 65, 71)`, Editorial `rgb(28, 27, 23)` / `rgb(58, 56, 47)`); the card's ground differs from the page's in each theme.

### Faults (listed, nothing fixed: `components/entrance.css` is a public sheet)
1. **The form is narrower than the card's text column.** `.ent-invite-card` (`entrance.css:557-570`) is `display: flex; flex-direction: column; align-items: center`, so the template's `<form>` (inline style `display: flex; flex-direction: column`) shrinks to its widest content: the name field and the button are 241 px wide in a 327 px (375) or 440 px (1280) card, while the channel list above spans the card. Node: `.ent-invite-card > form`. Change I would make: `.ent-invite-card > form { align-self: stretch; }` (the field and button then fill the card's content width). Same shrink affects `.ent-invite-card ul` (centered, text-aligned start).
2. **At 375 the create button is 36 px and the name field 40 px, and the button fails the hit test** (`elementFromPoint` at 21 px above and below its centre does not return it). Both are the kit's `Button` and `TextInput`: the kit builder's 44 px rule is inside `max-width: 720px` for `Settings.module.css`, `Controls.module.css`, `Modal.module.css` only, so these two are not covered. Node: `.ent-invite-card form button[type=submit]` and its input. Change: the kit's phone block should give `Button` (`md`) and the text field a 44 px minimum on a phone (kit), or `entrance.css` could add `.ent-invite-card form button, .ent-invite-card form input { min-height: 44px }` in a `max-width: 720px` block (that sheet loads on the public pages, so no app section or theme name).
No text is under 4.5:1 in any of the six, and no field has a wrong ground in a light theme.

## Other things I saw (outside my files)
- A server's Server Support tab at 375 (before and after, unchanged): the "Compare levels" table is cut at the card's right edge ("LE", "25"): it scrolls inside its own wrapper (`scrollerOver` is 0 and the document does not scroll), but nothing hints that it scrolls. File: `settings/server/sections/ServerSupport.tsx` (the `sup-compare` table), not mine.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p84-support/` (after: 46 PNGs, `measure.jsonl`, `template.jsonl`) and `p84-support-base/` (before: 14 PNGs, `measure.jsonl`). **I opened every PNG in both folders** (60), after reading the numbers: member's tab top / patron / donate / server / credits (375 Lounge Light, 375 Lounge Dark, 1280 Editorial, plus the top, donate and the two open Selects at 1280 Lounge Light and Dark), the amount-under error shot in each of the five settings, the open server and credits Selects in each of the five, `/support` and Server Support at the three settings (after) and at 375 Lounge Light and 1280 Editorial (before), the six template shots. I deleted the base's extra shots of the settings the task does not ask a base for (375 Lounge Dark, 1280 Lounge Light and Dark: 11 files) unopened. What I looked at: before, plans 2 + 1 + 1 at 375 and 3 + 1 at 1280, a white native-looking box and select; after, 2 + 2 and 4 in a row, the theme's fields, the red error under the field, the button on the field's bottom edge at 1280 and under the field at 375, the Select popups whole inside the viewport.
- The spec: `e2e/tests/p84-support-shots.spec.ts`, local only (`SR_P84_SUPPORT=1`; `SR_P84_BASE=1` for the base; out `SR_P84_SUPPORT_OUT`). Test 1 is item 1, test 2 is item 2.

## Checks
Web typecheck, eslint (package's own) and prettier on my three web files: clean. `node scripts/i18n-audit.mjs --max 0`: 0. e2e `tsc`: clean. Vitest: both files pass (22 and 11). `node scripts/check-doc-links.mjs`: see below. Spec run after: member's tab (all assertions pass except the one I fixed after the last full run: `sup-guild changed`, a spec mistake that picked the already selected server; fixed, re-run, passes) and the template test passed (6 shots, deleted).

## Docs
`git grep -n -i "other amount\|invite credit" -- docs apps/web/src/help`: no sentence about the donation form's fields (only the invite-credit allowance in D-024, D-372 and the handbook, which my change does not touch). Nothing to correct.

## Cleanup
Stack stopped by PID (ports 8621, 5621, 9321 free), `sr_p84_support_ui` dropped, `p84-support-bin` deleted (`p80-base-bin` untouched), lock released, nothing queued or running. No stash used or touched.

## Proposed inventory rows
- **SEC-AUD-358** (D-862): Support / Patron on the kit's fields; plan and preset buttons in an even grid. Fixed: the amount, server and credits controls are the kit's `TextInput` and `Select`; plans 2 + 2 at 375 and one row of four at 1280; field ground and border are the theme's in Editorial, Lounge Light and Lounge Dark; no sideways scroll. Closes rows 9 and 10 of SEC-AUD-349's table (the plan buttons wrap and the bare field and selects). Left: the kit's Select trigger is 38 px (kit, SEC-AUD-351's rule).
- **SEC-AUD-359** (D-863): the template page's ready state, shot in the member's theme: `data-theme` equals the main theme in all six, no text under 4.5:1, no scroll. Two faults listed against `components/entrance.css` (form narrower than the card; 36 px button and 40 px field at 375), not fixed (public sheet).

## Changelog sentence
Support / Patron's amount, server and credits fields now look like every other field and the plan buttons sit in an even grid (two by two on a phone).

## Owner questions
1. The plan said the label, the field and the button on one row at 1280. The kit's `TextInput` draws its label above the field, so the label sits above and the button stands on the field's bottom edge (as the kit does everywhere). Keep that, or ask the kit for a label-beside variant?
2. The template page's form is narrower than its card (fault 1): fix in `entrance.css` (one line) in wave 85?
3. The 375 hit test fails for the kit's `Select` trigger (38 px) and the template's button and field: for the kit builder's list.
