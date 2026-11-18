# fix-p106-dialog (D-1017): result, no fault found, nothing built

Clock: started 16:30, base rounds done 16:36, written 16:38 (2026-11-18).

## Done (commits)
- New spec `e2e/tests/p106-dialog-shots.spec.ts` (base figures and, for a later after round, a compare). No app file, no sheet touched.

## Not done (why)
No cure built, no `build`, no `tsc` web job, no after round: the fault in the task does not exist in the base. Nothing to ship.

## Cause (one sentence)
There is none: in the base bundle (jobs 0080, 0091) the Create Channel name field shows the kit's own look in every state (rest border `--ui-border`, hover `--ui-text-faint`, keyboard focus a 2 px `--ui-focus` outline); the p105 reading "text-coloured border, no outline" was `outline-color` of an unfocused field (`outline-style: none`, 3 px, colour `currentcolor` = the text colour) and a field that is not focused at open.

## Figures (Lounge Light / Lounge Dark, 1024; border-color, outline)
Create Channel name field (and the Create Event topic field, and the Server settings > Overview first text field as reference: identical in all three):
- rest (focus on another control): Light rgb(201,204,209), outline none; Dark rgb(63,65,71), outline none. Not `:focus-visible`, not hovered, `aria-invalid` null.
- hover: Light rgb(98,102,110), Dark rgb(154,160,170) (the kit's `--ui-text-faint`), outline none. `:hover` true.
- Tab to it: border stays at rest colour; outline solid 2px, Light rgb(0,104,201), Dark rgb(0,168,252) (`--ui-focus`); `:focus-visible` true, focused true.
- as the dialog opens: **the field is not focused**; `document.activeElement` is the dialog's Close button (`BUTTON aria-label Close`) in Create Channel and Create Event, so the field reads as rest (border faint-less, no ring). The task's candidate "focused as the dialog opens" is false: the kit Modal's focus handling puts focus on Close and wins over `autoFocus` on the `TextInput` (`CreateChannelDialog.tsx:216`). Not changed (the Modal is the kit's; moving focus to the name field would break "focus still lands where it lands").
- Invite dialog: has no text field (selects only), so no figure; Edit channel not opened (Create Event serves as the second dialog).

Candidates ruled out, with what I saw:
- A Lounge rule with `outline: none`: rules listed from `document.styleSheets` matching the field (in `figs` `rules` of `figures-base.json`); the only outline rules are the kit's `.control:focus-visible` and `.lng-root :focus-visible` (2 px `--ui-focus`, offset 1 px) which agree. `lounge.css:189, 893, 1249` (`.lng-resize`, `.lng-search input`, `.lng-textarea`) do not match a dialog field; `profile.css:15` likewise.
- Invalid state: `aria-invalid` is null on the empty required field (the form is `noValidate`, no error until submit).
- Hover under a resting pointer: not the case either; the pointer was not over the field and hover reads the kit faint border (above), a true hover.
- Modal focus handling: true as above (Close gets focus), but it is not a ring/border fault.

## FINDERS (rule 13)
No control changed. `git grep` of Create Channel in `e2e/` and `__tests__` not needed: none affected ("none").

## JOBS
- 0080-dialog-shoot: shoot base, waited about 20 s, ran 48 s, rc 0 (first spec; Tab state was wrong in my spec: Tab started from the Close button, so the field never took focus; fixed).
- 0091-dialog-shoot: shoot base, waited 133 s, ran 49 s, rc 0. Figures in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-dialog/figures-base.json`.
(times from `cli-agents/run/jobs/<id>/{submitted,started,ended,rc}`.)

## Rows
May change / must not change: nothing moved (no change made).

## Shots (base, opened after the job ended)
- base-dialog-open-1024-light.png: Create Channel as it opens; Text card with its ring, name field plain, no field ring; focus is on Close (not visible here).
- base-dialog-tab-focus-1024-light.png: name field with a blue 2 px kit ring after Tab.
- base-dialog-hover-1024-light.png: name field at rest border; Close button shows a ring (I had focused it as the "other" control); the pointer was on the field, hover border is faint and hard to see at this size, figure says rgb(98,102,110).
- base-dialog-open-1024-dark.png: dark dialog, name field plain faint border, no ring.
- base-dialog-open-375-light.png: phone dialog scrolled; the Close button has the ring (focus on open), name field plain; no sideways scroll.
- base-event-open-1024-light.png: Create Event with the topic field focused (left so after the Tab states): blue ring around it.

## Who imports my files
`CreateChannelDialog.tsx` unchanged. Spec only.

## Checks
e2e `tsc --noEmit -p tsconfig.json` (16:36, exit 0); prettier on the spec (exit 0). Web tsc, eslint, unit tests, check-css-tokens: not run (no app file changed).

## Closing text for the row "Create Channel dialog: text-coloured border, no ring"
Not a fault. The base shows the kit's rest, hover and focus ring in the dialog (jobs 0080, 0091); the p105 figures read `outline-color` of an unfocused field. Separate fact: the name field is not focused when the dialog opens (the kit Modal focuses Close).

## Proposed new row
Create Channel and Create Event: `autoFocus` on the first field is overridden by the Modal, which focuses Close; owner decision whether the name field should take focus (Discord does). Taker: kit Modal (not mine).

## Changelog sentence
None (no change).

## Owner questions (Fable)
Should the name field take focus when Create Channel opens? Default: yes (Discord-like), as a kit Modal change in a later wave.

## PROCESSES
strays: none found
