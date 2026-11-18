# Wave 103 handoff: probe (SEC-AUD-467, D-983)

Clock: started 11:56; code and spec committed 12:20 (`7f9199ef`); waited for a slot from 12:20 to 12:58 (every `acquire` before that answered 75); STACK-ROUND 12:58 to 13:01; handoff 13:03.

## Done
- `e2e/support/hit-test.ts`: `hitTest` and `hitTestAll` take `reach?: 'touch' | 'box' | number`.
  - `'touch'` (default) is today's behaviour to the pixel: points 21 px above and below the centre (D-856).
  - `'box'`: offset `max(1, min(21, floor(height / 2) - 1))`, so the points lie inside the control's own box.
  - A number is still accepted (the old numeric `reach` had no caller in any spec: `grep` found none); it is a fixed offset.
  - Every `HitResult` now carries `offset` (the offset used); the height is `box.height`. The header says what `'box'` is for, when to use which (375 touch: default, always; 1024: `'box'`), and that a pass with `'box'` never shows a control is 44 px.
  - Nothing else changed: scrolling, `openCollapsed`, the label rule, takers, `not-reached`, `skipped`, `also`.
- `e2e/tests/p103-probe-shots.spec.ts` (new). At the start of the run it writes the unedited helper (`git show 050be81d:e2e/support/hit-test.ts`) to `e2e/.state/p103-probe/` and runs both helpers over the same roots.
- Commits: `7f9199ef SEC-AUD-467 hit test: reach 'box' option (D-983), spec not yet run` (code and spec; the spec was run afterwards and needed no edit) and the handoff commit.

## The option and the lines a later spec writes
Signature: `reach?: 'touch' | 'box' | number` in `HitOptions` (default `'touch'`).
```ts
await openCollapsed(page, root);
const rows = await hitTestAll(page, root, { reach: 'box' }); // at 1024 (pointer width); at 375 omit `reach`
expect(rows.filter((r) => r.status === 'fail')).toEqual([]); // a fail has its taker named
```

## Tables (figures: `visual-sweep-shots/p103-probe/figures.json`, every row with height, offset and takers)
Default against `'box'` at 1024 x 768 (same page state, same root):

| root | controls | unedited helper fails | new default fails | `'box'` fails | height:offset pairs `'box'` used |
|---|---|---|---|---|---|
| Admin > Arcade | 18 | 17 | 17 | 0 | 28:13, 38:18, 40:19 |
| Notification Settings dialog | 11 | 11 | 11 | 0 | 18:8, 22:10, 32:15, 36:17, 40:19 |

The 28 px Release and Remove buttons are among the 17 default fails (takers: the form, the next entry's heading, a chip) and pass with `'box'`. The dialog's 40 px select fails by default at 1024, as in wave 102, and passes with `'box'`.

Unedited helper against the new default, compared row by row (index, tag, name, status, takers): **equal in all four runs** (Admin > Arcade and the Notification dialog, each at 1024 and 375); the spec records a problem if one differs.

At 375 x 812 touch: Admin > Arcade 18 controls, 0 fails (unedited, default and `'box'`); Notification dialog 11 controls, 0 fails in all three. `'box'` against the default on controls 44 px or taller: the same rows, offset 21 for both (44:21). The shorter controls at 375 (18, 22 and 36 px checkbox, radio and toggle rows) get offsets 8, 10 and 17 with `'box'` and pass in both. Sideways scroll 375/375.

Forced overlap (spec only, by `evaluate`, removed again): a fixed element `aria-label="p103-probe-overlay"` over the lower half of the 28 px button "Release p100-seed-4fg24u-reimpl" (rect 366.7, 369.7, 239.9 x 28). Without it `'box'` passes (offset 13). With it `'box'` fails: `centre` and `below` are taken by `div "p103-probe-overlay"`. After removal the button passes again. So the option did not just make everything pass.

p102 arcadm spec re-run on the same world (after phase, output `visual-sweep-shots/p103-probe/p102-arcadm/`, 1 passed): Admin > Arcade at 375 hit 18 controls, 0 fails (wave 102: 0); Start dialog at 375 12 rows, 9 fails (wave 102: "10 fails to 9", the 36 px channel rows); cartridge page and dialog at 375: 0 fails; the 1024 scenarios list the same kinds of fails as wave 102's note (28 px buttons, Close, 36 px rows). Wave 102's handoff gives no 1024 count, so 1024 is shown equal by the unedited-helper comparison above, not by a figure from that handoff. The default did not move, as intended.

## STACK-ROUND
- `acquire probe` exit 0; `status` showed `probe 12:58`. Round 12:58 to 13:01: seed the sweep world (no FAIL line), my spec (1 passed, 25.8 s), p102 arcadm (1 passed, 33.8 s).
- Stack on 8808 / 5808 / 9508, database `sr_p103_probe_ui`, `SR_ADMIN_REQUIRE_REAUTH=0`. My first stop used the shell's PIDs (not the Windows ones) and missed; I then found the listeners by port, stopped them by Windows PID tree (14456 and 28172) and checked that 8808, 5808 and 9508 are free. My slip: I released the lock before the stack was really stopped, so it ran idle for about 10 s after `release`; it served nobody.
- After the release `heavy.sh status` printed `toolshots 13:01`; no hold in my name.

## Shots (`visual-sweep-shots/p103-probe`, 4, all opened)
- `admin-arcade-1024.png`: the translucent red overlay with a dark outline lies over the lower half of the first Release button and strikes through its text, as intended.
- `admin-arcade-1024-clean.png`: the entries and the form, no overlay; buttons 28 px, selects 38 to 40 px, nothing overlaps.
- `notification-dialog-1024.png`: Notification Settings with the kit select "For 1 Hour" whole and its chevron clear; radios, toggles and Done in place; Close focused.
- `admin-arcade-375.png`: phone, lower Admin > Arcade entries and form; 44 px buttons, no sideways scroll.
All in Lounge Light. The probe test needs no theme variants. The 12 shots of the p102 spec in `p102-arcadm/` were not re-read: only its rows were compared.

## TYPECHECK and checks
- web typecheck: not needed (no file under `apps/web` or `packages`).
- e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: 11:58, exit 0 (no edit to either file afterwards).
- prettier --write on both files: clean. eslint (the web package's binary, `--no-warn-ignored`, run from `apps/web` over the two e2e files): no output; the e2e files may be outside its config, so this proves little.
- `node scripts/check-doc-links.mjs`: ok, exit 0, 13:01.

## The commander's command (after the sweep world is seeded on the same database)
From `e2e/`, ports 8808 / 5808 / 9508, database `sr_p103_probe_ui`:
```
SR_P103_PROBE_ON=1 SR_E2E_BASE_URL=http://localhost:5808 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
SR_P103_PROBE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-probe \
npx playwright test tests/p103-probe-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
The spec needs `git` in the repository (it reads `050be81d:e2e/support/hit-test.ts`).

## Closing text for SEC-AUD-467
"`hitTest` and `hitTestAll` take `reach: 'box'` (D-983): probe points inside the control's own top and bottom edge, offset `min(21, floor(height / 2) - 1)`, for pointer widths. The default stays the 44 px rule (21 px) for 375 with touch and did not change (rows equal to the unedited helper in four runs). Proven in `p103-probe-shots.spec.ts`: the Arcade's 28 px buttons and the Notification dialog's selects fail by default at 1024 (17 and 11 rows) and pass with `'box'`; an element laid over a button's lower half still fails with `'box'` and is named as the taker."

## For AGENTS.md section 4 (one sentence)
At 375 with touch a hit test uses the default reach; at pointer widths `reach: 'box'`, and a fail with a taker is a fault at either.

## Proposed rows and notes
- Later waves: specs at 1024 pass `reach: 'box'` and keep the default at 375. Counts from earlier waves at 1024 are not comparable with the new ones (the old counts include 28 to 40 px controls failing the 44 px rule).
- Still open, not mine: the Start dialog's 36 px channel rows fail the touch rule at 375 (9 rows, base and after alike, takers are the neighbouring rows).
- Owner questions: none.

## Cleanup
Scratch database `sr_p103_probe_ui` dropped (`DROP DATABASE` printed). `p103-probe-bin` deleted. No stash touched. `e2e/.state` is git-ignored and not committed.
