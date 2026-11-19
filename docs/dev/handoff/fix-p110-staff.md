# fix-p110-staff (wave 110, SEC-AUD-555, 556)

## Clock
Start 23:15; spec committed 23:19; base shoot 23:55 (runner stopped for mounts' cargo about 23:20 to 23:54); touch re-run 23:57; handoff about 00:00.

## Done (commits on fix/p110-staff)
- SEC-AUD-555: the comment at `server-settings.css:759` now says `.ss-select` is used by three elements, all in Server settings (Members' two selects, the forum reaction input in `ChannelTabs.tsx`); Admin does not use it. Comment only, no rule changed.
- `e2e/tests/p110-staff-shots.spec.ts` (measure only, base phase run).

## Not done, and why
- No rule changed, so no build, no `shoot after`, no token check, no FINDERS beyond "none" (only a comment changed; the sheet's name is read by no test that cares about comments).
- SEC-AUD-556: not a fault, nothing built (figures below).

## 555, the Audit log's "Action" field: not a fault (no cure)
Admin `/admin/audit-log` (the page censusb shot) has three kit `TextInput`s: all `input._control_10o4w_13`, 241 x 40. Server settings audit log has two kit `NativeSelect` `select._select_dyhrb_20` (165 and 522 wide, 40 high). Computed, 1024:
- Light: every field bg `rgb(242,243,245)` (= `--ui-bg-raised` #f2f3f5), border 1 px mixed colour (same for all), text `rgb(49,51,56)`.
- Dark: every field bg `rgb(43,45,49)` (= `--ui-bg-raised` #2b2d31), same border, text `rgb(219,222,225)`.
The ground is set by the kit's field rule, the same for all; no restyle in `server-settings.css` or elsewhere. "Darker" in the shots is the Action field's placeholder text ("accounts.suspend"), which the other two fields do not have. Closing text: SEC-AUD-555 comment fixed; the Action field's ground is the same as its neighbours (not a fault).

## 556, the tool drawer at 375 touch: not a fault, the drawer scrolls
Drawer box 345 x 654 at 375x812 (y 79 to 733), 345 x 637 at 375x667; unchanged. The tool's own scroller is `.sr-tl-view-body` (`overflow-y: auto`, `max-block-size` 22rem = 352 px): Dice scrollHeight 387 / clientHeight 352 (max 35); Wheel 511 / 352 (max 159). At 667 the modal's body (`div._body`, `overflow-y: auto`, kit) also scrolls (600 / 583, max 17).
Base, before scrolling: 812: Dice detail line 653 to 741 (cut by the scroller's fold at 716); Wheel Spin 759 to 803, hit test of its centre false (clipped), winner below. 667: Wheel Spin at 696 to 740, off screen.
After a real touch swipe (raw CDP touch events, inside the tool body) in all four cases (Dice and Wheel, 812 and 667): Spin on screen and hit (812 Dice y 380, Wheel 622; 667 Dice 317, Wheel 559), the roll detail and the winner on screen. `scrollTop` set and read back: Dice 35 of 35, Wheel 159 of 159. (A first run with a CDP `synthesizeScrollGesture` moved nothing: that was the harness, the raw touch events scroll.) So a member can reach the detail and Spin by touch. Closing text: SEC-AUD-556 not a fault, the drawer scrolls (the detail and Spin start under the tool body's fold and a swipe brings them in); nothing built. Possible later polish (a row, not mine): the Wheel's Spin starts below the fold, so a first-time user has to scroll to find it.

## FINDERS
none (no rule changed).

## JOBS
- 0330-staff-shoot (shoot base): ran in 20 s, rc 0 (first look; the dark theme was not set: the account setting overrode it; spec fixed).
- 0334-staff-shoot (shoot base): queued 23:20 to 23:54 (runner stopped), rc 0; touch swipe did nothing (synthesizeScrollGesture); spec changed.
- 0338-staff-shoot (shoot base): about 1 min, rc 0; raw touch swipe works. Figures in `visual-sweep-shots/p110-staff/figures-base.json` (the last run).

## Shots (all opened after the job ended; base- prefix, `visual-sweep-shots/p110-staff`)
- base-audit-filters-1024-light.png: three fields, same grey ground, Action shows placeholder text.
- base-audit-filters-1024-dark.png: same, dark ground, same for all three.
- base-dice-rolled-375.png: drawer full height, roll 14, detail "1d20 = 14" at the body's fold.
- base-dice-rolled-scrolled-375.png: body scrolled to the end: detail and history pill visible.
- base-wheel-375.png: names box and wheel, Spin below the fold.
- base-wheel-scrolled-375.png: (from the first run) Spin and "0 names" visible after scrolling.
- Extra: base-audit-server-1024-light/dark.png (two selects, same ground; not opened beyond the figures).

## Checks
- e2e `tsc --noEmit -p tsconfig.json`: exit 0, 23:5x. prettier --check on the spec: pass after --write; token script: not run (no rule changed). vitest, eslint, web tsc: left to the commander.

## Who imports my files
`server-settings.css`: the Server settings chunk (lazy). Spec: e2e only.

## Changelog sentence
None (comment only).

## Owner questions
None.

## PROCESSES
strays: none found (strays.sh p110-staff)
