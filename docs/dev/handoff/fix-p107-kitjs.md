# Handoff: fix/p107-kitjs (D-1021, D-1022)

## Clock
Started 18:37; code committed 18:46 (4b7f8206), spec fix 19:13; after shoot ended 19:38. Replied 19:40.

## Done (commits)
- `4b7f8206` D-1021 D-1022: Modal first focus; Select 44 px rows on a phone; unit tests; spec.
- `bounded menu clicks` spec commit, then this handoff.
- D-1021: `Overlay/overlayStack.ts` `useFocusTrap` got `preferFields` (Modal passes it; **Drawer does not, so Drawer is unchanged**). Order: `initialFocusRef`; the dialog's own active element if inside and not Close (autoFocus); `[autofocus]`; `[data-autofocus]`; first field (input/textarea/select not disabled or hidden, contenteditable); first focusable that is not `[data-sr-close]`; Close. `Modal.tsx`: Close button carries `data-sr-close`. Confirm dialogs: Cancel has `data-autofocus` (typed confirm: the input), so a destructive confirm still starts on Cancel (unit test).
- D-1022: `Select.tsx` `OptionList` reads `matchMedia('(max-width: 720px)')` once on open (the kit's Button breakpoint) and uses `max(rowHeight, 44)`. Row and list heights come from the same value (rows are sized inline), so `Select.module.css` needed no change and is untouched.

## Not done
- Confirm and Add a Server could not be measured: the owner has no "Leave" menu item (owner of the guild) and the "Add a Server" dialog did not open by that name in the spec (base too: both timeouts in base and after). Unit test covers the confirm (Cancel focused). Not measured in the real app.
- `Select.module.css` `.option`: no change needed (height is inline).

## REACH
Modal: every kit `Modal` with a field. Select: every kit `Select`/`Combobox`/`MultiSelect` list at max-width 720px. Moved: focus on open; option and list height on phone (36 to 44, list rows x 44).

## FINDERS (rule 13)
- e2e specs that press/type after a dialog opens or assert focus (keyboard-4-overlays, drawer, lounge-shell, lounge-gaps, lounge-server-flows, polish-*, first-run, entrance-journey, tool-rooms, studio, study-shelves, members-hit-test, audio-trim, bridge, emulator-coop): read. keyboard-4-overlays.spec.ts:30-60 (Add a Server: Tab/Shift+Tab stay inside, Escape closes, focus returns to Add): unaffected (checks containment only). Quick switcher `:76-84` (types after Ctrl+K): not a kit-Modal field change (it already focuses its input). Others press Escape or type into a composer, not into a dialog. **None needs a change; no CI spec edited.**
- Unit tests: packages/ui overlayStack, overlays, select, misc, lounge-kit, menus: pass. App: guide, live, messages, p11ShellDm, p71Panels, profile pass (single files). shell, threads, people, prefsSections, support: NOT run (stopped on memory order); drawer.test not run.
- Tests reading my files as text: `packages/ui/src/primitives/layers.test.ts` reads Modal.module.css and Select.module.css (untouched); none reads the .tsx.

## JOBS
0185 shoot base: wrongly timed out (spec waited on a missing menuitem), rc 1. 0186 build rc 0 (stale after spec commit). 0205 shoot base rc 0 (queued ~17 min, ran 57 s). 0206 build rc 0. 0211 shoot after rc 0 (ran 58 s). Times from `cli-agents/run/jobs/<id>/`.

## Figures (base -> after), 1024 dialogs
- Create Channel, by click and by Enter on the opener: focus at open `button "Close"` -> `input "Channel name"`; first Tab: unnamed input -> Private Channel input.
- Create Event (1024 and 375): `Close` -> `input "Event topic"`; first Tab -> `Starts`.
- Escape closes in all. Focus after Escape: body in base AND after for Create Channel (opener not refocused; the "Create Channel in" button is not what is focused afterwards): a base fault, same in both. Unit test shows return of focus to the opener for a normal opener.
- Select at 375 (Admin Bans, Accounts, Permissions "Add a role"): option height 36 -> 44; list 108 -> 132 (3 x 44); hit-test fails 3 -> 0 in each (the base fails were the 36 px options and their takers); lists inside the viewport (bottom 354, 337, 747 of 812). At 1024: heights 36 and list 108 identical to base in all three.
- Must not change: dialog boxes (shots compared by eye), trap, Escape, trigger.

## Shots (opened after the job)
- base-create-channel-open-1024: dialog, no ring on Name; Close focused (no visible ring on X).
- create-channel-open-1024: same dialog, ring and caret on Channel name; nothing moved.
- create-event-open-375: ring on Event topic, dialog fits.
- base-select-open-375: three rows about 36 px. select-open-375: three taller rows, list ends well above bottom. select-open-1024: unchanged 36 px rows.
- `base-select-open-1024` seen: same as select-open-1024. Missing shots: confirm-open-1024 (confirm not reachable).

## Chunks
Modal, Select, overlayStack are in `@sideroom/ui`, used by first paint and lazy chunks alike (kit; imported via `packages/ui/src/index.ts`). I changed only kit files, and no app file.

## Checks
prettier on my files clean; eslint (packages/ui) on Modal.tsx, overlayStack.ts, Select.tsx clean; e2e tsc clean (19:45); vitest ui files above pass. TYPECHECK: not run, by the planner's order (memory). Build rc 0.

## Closing text
D-1021 row: a kit Modal's focus on open goes to initialFocusRef, else what the dialog asked for (autoFocus, data-autofocus), else the first field, else the first control that is not Close, else Close; Drawer unchanged. G1: the kit Select's rows are 44 px on a phone (max-width 720), 36 elsewhere.
Changelog: A dialog now opens with the cursor in its first field, and dropdown lists have 44 px rows on a phone.

## Owner questions
None (Discord-like default taken: focus the first field).

## PROCESSES
strays: none found (my stopped vitest workers were killed by PID).
