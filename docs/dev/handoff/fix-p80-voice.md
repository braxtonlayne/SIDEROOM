# Wave 80 handoff: voice (SEC-AUD-325 / D-827, SEC-AUD-326 / D-828)

Branch `fix/p80-voice`. Web only: no Rust, no migration, no `.tsx`, no new string, no generated file.
Clock: T = 07:30 on 2026-11-16; no new item from 09:30, checkpoint 09:50, finished 10:15, hard limit 10:30. Lock round 08:28-08:44 (16 minutes), released. Finished well before the checkpoint.

## Call
Real one-member call (`SR_VOICE__ENABLED=true`, base binaries copied from `p80-base-bin`, Chromium fake devices). Connected in about 1 s each time (all three themes, plus the desktop calls).

## Item 1: SEC-AUD-325 (phone call page shift)
- **Base, natural path: does not reproduce.** 375x812 with touch, a real call, "Back to channels" and forward again, 3 tries each, Lounge Dark, Light and Editorial: every pane's left was a multiple of 375 (list 0 / 375 / 750, chat -375 / 0 / 375), visible pane 0, `scrollLeft` 0 on `scrollingElement`, `html`, `body`, `.lng-root` and every ancestor of `.lng-main`, page overflow 0, trap log of `focus()` / `scrollIntoView()` empty. Not measured without a call (every run had one). Note: pressing the connected voice channel while the route is already that channel does not change pane (the pane effect only runs on a channel change); the spec goes through the text channel to get "forward again".
- **Base, provoked: reproduces and names the element.** From the channel list, `scrollIntoView` (and a plain `focus()`) on a control of the off-screen call page leaves `div.lng-body` at `scrollLeft: 225`, `.lng-left` left -225, `.lng-main` 150, `.lng-right` 525. So wave 79's 16 px shift came from script (Playwright's own scroll-into-view before a click on the off-screen copy of a control); no real caller in the app was found (the trap saw none during normal use). The box that scrolls is `.lng-root[data-mobile] .lng-body` (`overflow: hidden`, the box the panes slide in), not `.lng-root`.
- **Fix** (`apps/web/src/lounge/lounge.css`): `.lng-root[data-mobile] .lng-body { overflow: hidden; overflow: clip; }` (a clipped box cannot be scrolled at all). The panes scroll inside themselves; nothing else changed. I did not add `clip` to `.lng-root` (not the scrolled box).
- **After:** the same provoked step in Dark, Light and Editorial: all `scrollLeft` 0, lefts 0 / 375 / 750, back to the call page lefts -375 / 0 / 375. Normal path unchanged. Menus, composer, tab bar, call bar all unchanged in the shots.
- No base shot of the shifted page was kept (my base-run2 shots were overwritten); the base figures above are from `measurements-base-run1.txt` (natural path) and the log lines quoted here (provoked).

## Item 2: SEC-AUD-326 (call bar menus)
- **Base:** per chevron: mic box 63.5,697 28x44, camera box 213.5,697 28x44; `pointer-events: auto`, `visibility: visible`, no `inert` ancestor, `elementFromPoint` at the centre is the control. `page.touchscreen.tap` at the centre opened the menu for both chevrons, in Dark and Light (camera turned on first; the camera chevron exists even with the camera off). **So the fault was wave 79's locator, not the page: nothing built** (as the task says). I could not tell why wave 79's spec failed (its non-touch context, first-match/on-screen filter, or the off-screen duplicate); not investigated further.
- **Menus measured (375x812):** mic panel 8..208 x 484..693, scroll 198/198, nothing past the panel, items Default / Fake Audio Input 1 / Fake Audio Input 2 / Voice settings; camera panel 128..328 x 577..693, scroll 198/198, Default / fake_device_0. A device pressed, menu closed, pane stayed at 0, in Dark and Light. The kit's Menu already limits itself to `min(320px, 100vw - 16px)` with ellipsis labels (not mine).
- **Chevron touch target is 28 x 44** (above the 24 x 24 floor, under the 44 x 44 target): left as is because the task builds only for a confirmed page fault. Ready if wanted: in `voice-p13.css`, `@media (max-width: 768px) { .vc-split .vc-ctl-arrow { inline-size: 44px; block-size: 44px } }` (the bar still fits, about 328 of 375 px). Saved at `SIDEROOM_wt/p80-voice-scratch.patch`. Owner question below.
- Desktop 1280x800, Dark and Light: the user panel's input device menu opens, nothing moved (shots).

## Files / imports
`lounge.css` is imported by `LoungeShell.tsx` and by `settings/server/ui.tsx`, `settings/user/parts.tsx` (all lazy). `VoiceBridge.tsx` and `IncomingCall` import neither css: no web build. No `.tsx` changed.

## Tests
- `e2e/tests/p80-voice-shots.spec.ts` (new, local only `SR_P80_VOICE=1`; `SR_P80_BASE=1` logs without asserting). Watched fail on the base: the provoked step reported `FAULTS: ... provoked scrollIntoView ... provoked focus` on the base and `FAULTS: none` after the CSS. The natural path passes on both.
- Single vitest files: `callUi.test.tsx` 26 passed, `lounge/__tests__/shell.test.tsx` 26 passed. `check-css-tokens` clean, `i18n-audit --max 0` = 0, prettier clean, e2e `tsc` clean. Eslint not run on css/spec beyond tsc.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-voice`: 25 PNGs, all opened by me: call page, after Back, forward again, provoked (Dark, Light, Editorial = 12); mic and camera chevron bar and open menu (Dark, Light = 8); desktop call and desktop device menu (Dark, Light = 4) plus the unlisted bar shots. `measurements.txt` (final run), `measurements-base-run1.txt` (base).

## Proposed inventory rows
- SEC-AUD-325 (web, Lounge): the phone Lounge's pane box (`.lng-body`) is `overflow: clip`, so script, focus or scrollIntoView on an off-screen pane cannot slide the panes (measured: `scrollLeft` 225 on the base); not reproduced on the natural path (D-827).
- SEC-AUD-326 (web, voice): the call bar's microphone and camera chevrons open by touch on a phone and their menus lie inside the screen with the devices listed; wave 79's miss was its spec's; chevron is 28 x 44 (D-828).

## Changelog sentences
- 325: The phone Lounge's pages can no longer be knocked sideways by a script or a focus change.
- 326: None a member would notice (the menus were already reachable).

## Owner questions
Raise the phone chevrons to 44 x 44 px? (default: no, left 28 x 44).

## Cleanup
Stack stopped (by PID and port), `sr_p80_voice_ui` dropped, `p80-voice-bin` deleted, lock released. Scratch in `SIDEROOM_wt/p80-voice-scratch` (untracked). No stash touched.
