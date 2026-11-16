# Wave 81 handoff: voice (SEC-AUD-331 / D-830, SEC-AUD-332 / D-833)

Branch `fix/p81-voice`. Web only: no Rust, no cargo, no `.tsx`, no new string, no generated file.
Clock: T = 09:35 on 2026-11-16; no new item from 11:35, checkpoint 11:55, finished 12:20, hard limit 12:35. I waited for the heavy lock from 09:38; held it 10:09-10:29 (first round, base + after, stack stopped and released) and 10:29-10:30 (a short second round for one missing shot). Finished at about 10:35, far inside the limit.

## Calls
Real calls on a real stack (`SR_VOICE__ENABLED=true`, binaries copied from `p80-base-bin`, Chromium fake devices): each connected in 0.9-1.0 s, for the owner and for the second member (Bob). The first two-member design (a fresh context per theme and size, Bob staying in) did not work: from the second owner context on, the popover stayed at "Securing the call" (a new context is a new device key; Bob had pinned the old one). The spec now keeps ONE owner context and ONE Bob context for the whole run, changes the theme by a reload (account setting plus `sessionStorage`-driven prefs) and the size by `setViewportSize` (375x812 with touch, then 1280x800), and Bob rejoins after the owner each time. This is only the spec's business, not an app fault I could tell.

## Item 1: SEC-AUD-331 (chevrons 44 x 44)
- **Base (run, `SR_P81_BASE=1`, the two CSS files held back by copying the `HEAD` versions in, no stash):** at 375x812 both chevrons 28 x 44 (mic 63.5,697; camera 213.5,697), the other bar controls 48 x 48, bar scroll 375/375 (nothing hidden), pane left 0, page overflow 0. At 360x800: bar 366/360 (hidden 6). Desktop chevrons 20 x 28. Menus open by `touchscreen.tap`, whole inside the screen (mic panel 8..208 x 484..693, camera 128..328 x 577..693).
- **Build:** the wave 80 patch's rule at the end of `voice-p13.css`, reworded to SEC-AUD-331 / D-830. Specificity measured, not assumed: the two-class `.vc-split .vc-ctl-arrow` wins over `voice-p10.css`'s one-class `.vc-ctl-arrow` (the boxes read 44 x 44 on the phone).
- **The bar no longer fit with only that rule:** with 44 chevrons at the 12 px gap the bar was 390 wide at 375; Mute was pushed to -3 (the bar had scrolled) and Disconnect ended at 378 (cut). The task's own rule applies (reduce the gap, not the target): `@media (max-width: 768px) { div.vc-controls { gap: 6px } }` in the same file. `div.vc-controls` (0,1,1) outranks `.vc-controls` of `lounge.css:1994` (0,1,0) whatever the file order.
- **After (375x812, all three themes):** chevrons 44 x 44; controls: Mute 10..58, Mic options 60..104, Deafen 110..158, Camera 164..212, Camera options 214..258, More 264..312, Disconnect 318..366: all inside 0..375 at load; bar scroll 378/375 (3 px of right padding, no control hidden). At 360x800 (logged only): chevrons 44 x 44, bar 370/360, Mute 2..50 ... Disconnect 310..358, no control cut. Pane left 0, page overflow 0. Both menus open by tap, whole (mic 8..208 x 484..693; camera 136..336 x 577..693), the devices listed (Default / Fake Audio Input 1 / 2 / Voice settings; Default / fake_device_0), a device chosen, menu closed, pane still 0 (Dark and Light). Desktop 1280x800: chevrons 20 x 28, gap 12 (unchanged), bar 724/724.
- The 360 menus were not opened (the task logs only the bar at 360).

## Item 2: SEC-AUD-332 ("Verified" button)
- **Base (run):** the browser's default button: `2px outset` border (white in Dark/Editorial, black in Light), radius 0, `16px` font, bold-less, grey `rgb(107,107,107)` background in Dark and Editorial. **Text contrast on the base: Lounge Dark 3.95:1 (fails 4.5), Editorial 4.29:1 (fails), Light 11.09:1.** Same in both states (the base has no pressed look). Row boxes (phone, 375): name 187x20, code 333x38, button 138x26 (75x26 pressed), row 333x68. Desktop row 334x68. Hover = resting (no rule).
- **Build** (`voice-p14-e2ee.css`, tokens only, no literal colour): the plan's rule (`min-block-size: 32px; padding: 4px 12px; 1px solid var(--ui-border); radius var(--ui-radius); background var(--ui-bg-hover); color var(--ui-text); font: inherit; 0.8125rem; pointer`), hover `--ui-bg-raised`, `:focus-visible` 2px `--ui-focus` outline, `[aria-pressed='true']` border `--ui-success`, `min-block-size: 44px` at `max-width: 768px`.
- **After (measured in the call popover; ground = the popover's opaque background):**

| theme | state | background | text | text contrast | border vs popover | radius | hover background |
|---|---|---|---|---|---|---|---|
| Dark | not pressed | 53,55,60 | 219,222,225 | 8.82:1 | 1.35:1 (not required) | 8px | 43,45,49 (differs) |
| Dark | pressed | 53,55,60 | 219,222,225 | 8.82:1 (phone 10.21: after a tap the button stays hovered) | 4.34:1 | 8px | |
| Light | not pressed | 232,234,237 (phone 242,243,245) | 49,51,56 | 10.49:1 (phone 11.39) | 1.61:1 | 8px | 242,243,245 (differs) |
| Light | pressed | same | same | same | 5.04:1 | 8px | |
| Editorial | not pressed | 44,42,36 (phone 28,27,23) | 239,230,210 | 11.56:1 (phone 13.89) | 1.47:1 | 4px | 28,27,23 (differs) |
| Editorial | pressed | same | same | same | 5.70:1 | 4px | |

(The phone figures differ from the desktop's only where the pointer rested: a tap leaves the button hovered, so the resting colour and the hover colour trade places; both are listed in `measurements.txt`.) Font: the popover's family, `13px` (the popover is 16px: deliberate, the plan's `0.8125rem`). The verified shield beside it keeps wave 80's colour: Dark `rgb(0,168,252)`, Light `rgb(71,82,196)`, Editorial `rgb(194,176,143)` pressed; not pressed it is the plain icon colour.
- **Row layout** (before -> after): desktop row 334x68 -> 334x74 (+6, the button 138x26 -> 125x32; name, code and button keep their grid places); **phone row 333x68 -> 333x86 (+18)**: the 44 px phone height D-833 asks for puts the 44 px button in row 1 (name 20, gap 4, code 38). The task also says the row at most 8 px taller: the two cannot both hold on a phone with the grid as it is (changing the grid is a `.tsx`/layout change). I kept the 44 px target. Owner question below.

## Files and imports
`voice-p13.css` is imported by `DmCallStage.tsx`, `SoundboardPopover.tsx`, `StageView.tsx`, `Tiles.tsx`, `VoiceStatusBar.tsx`; `voice-p14-e2ee.css` by `PrivacyCodes.tsx`: all in the Lounge voice chunk. `VoiceBridge.tsx` (first paint) imports neither: no web build. No `.tsx` changed, no new string.

## Tests
- `lounge/voice/__tests__/callUi.test.tsx`: 26 passed. `privacyCodesUi.test.tsx` (the only `*.test.tsx` that renders `PrivacyCodes`): 4 passed. (`p15Advanced.test.tsx` only names it.)
- `e2e/tests/p81-voice-shots.spec.ts` (new, local only `SR_P81_VOICE=1`; `SR_P81_BASE=1` logs without asserting). **Watched fail on the base?** The base run (BASE=1) is log-only by design, so it did not "fail"; I did run the same checks against the base and the figures above are its. The after run (assertions on): the first after run FAILED (`cut at load Mute, Disconnect` in all three themes, chevrons 44 x 44), which led to the gap rule; the second passed (`FAULTS: none`, 1 passed). A third short run (Editorial only, for the missing bar shot) passed.
- `node scripts/check-css-tokens.mjs` clean, `node scripts/i18n-audit.mjs --max 0` = 0, prettier clean, e2e `tsc` clean.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-voice`: 21 PNGs, **all 21 opened by me**: phone bar Dark / Light / Editorial (3), desktop bar Dark (1), mic and camera menus Dark and Light (4), privacy popover not pressed and pressed in three themes at 1280 (6) and at 375 (6), Light phone pressed again (1). `base/`: the Dark phone bar and the Dark privacy popover not pressed and pressed (3 PNGs, opened); I deleted the other base PNGs I had not opened. `measurements.txt` in both folders. Click-through (Light, phone): the mic menu opened by tap and a device picked; the popover opened, "Mark as verified" pressed, pressed again (back to "Mark as verified"): all in the run.

## Proposed inventory rows
- SEC-AUD-331 (web, voice, D-830): on a phone the call bar's microphone and camera chevrons are 44 x 44 (were 28 x 44); the bar's gap is 6 px on a phone so every control is inside 375 and 360 at load; desktop chevrons unchanged (20 x 28).
- SEC-AUD-332 (web, voice, D-833): the privacy codes' "Mark as verified" / "Verified" button has a look from tokens (border, radius, `--ui-bg-hover`, hover `--ui-bg-raised`, focus ring, pressed border `--ui-success`, 44 px tall on a phone); text contrast 8.8:1 or more in the three themes (was 3.95:1 in Lounge Dark and 4.29:1 in Editorial).

## Changelog sentences
- 331: The call bar's microphone and camera menu buttons are now full-size touch targets on a phone.
- 332: The "Mark as verified" / "Verified" button in a call's privacy codes now looks like a button and is readable in every theme.

## Owner questions
- The phone row of the privacy codes grows from 68 to 86 px with the 44 px button (the task's "row at most 8 px taller" cannot hold with a 44 px button in the grid's first row). Keep the 44 px target (default, built) or cap the button at 32 px on a phone?
- The phone bar's gap is now 6 px (was 12) to keep all seven controls inside 375 and 360 at load. Fine?

## Docs
No doc states these figures (searched `docs/` for the chevron size and the verify button): none changed.

## Cleanup
Stack stopped by port (server, Vite), `sr_p81_voice_ui` dropped (the DB was created twice; dropped both times), `p81-voice-bin` deleted, lock released. Scratch in `SIDEROOM_wt/p81-voice-scratch` (untracked). No stash touched; `git stash list` not looked at.
