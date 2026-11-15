# Wave 79 handoff: popovers (SEC-AUD-316, D-818)

Branch `fix/p79-popovers` (base `aa3ff9cf`). Web only: no Rust, no migration, no generated file, no new string, no `.tsx`.

## Done
- **Part 1, fold the limits back.** `header/header.css` `.lng-pins-pop` and `features/features.css` `.lng-saved-pop` now carry `box-sizing: border-box; max-inline-size: 100%` themselves; their `max-width: 90vw` / `92vw` lines were replaced (the same property as `max-inline-size`, so they were dead beside it). The doubled-class rule and its comment are gone from `lounge/lounge.css`.
  - Base run (rule removed, header/features held back, `SR_P79_BASE=1`, 375x812, Lounge Dark): Pins `div.lng-pins-pop 21..375` in a panel 8..367, scroll 366/357 => CUT; Saved items `.lng-saved-pop 21..382`, scroll 373/357, title, tabs, panel and text all past 367 => CUT. Fault confirmed (this is the base run; it passes only when something is cut).
  - After: Pins panel 8..367, scroll 357/357, cut [] (Dark and Light, and Editorial measured); Saved items panel 8..367, scroll 357/357, cut [] (the kit's tab strip is left out of the measure, it is kit's item: "Scheduled" is still half drawn in the shot).
- **Part 2, sticker and GIF grids.** Stickers: nine 96x96 PNGs made in the spec (zlib, no file) and added through `POST /guilds/{id}/stickers`. GIF source faked with `page.route` (`/gifs/config`, `/gifs/categories`, `/gifs/search` and `/trending`, images routed PNGs). **All whole already, nothing fixed** (wave 78's `minmax(0, 1fr)` columns hold): at 375 and 1280, three themes: sticker tiles 75.5 px, 9 cells in 29..349 (phone) / 681..1001 (desktop), cut 0, scroll 320/320; GIF category tiles 157 px, 6 cells, cut 0; GIF results 157 px, 8 cells, cut 0; picker inside its panel, page overflow 0. Click-through: a sticker picked (the message carries `sticker_items`, Dark both sizes), a GIF category pressed (results show). No `msg.css` change.
- **Part 3, voice (one real call, `SR_VOICE__ENABLED=true`, Chromium fake microphone, `p72-base-bin` server).** The call connected (`.vc-state[data-ok]`) within seconds; a second member (bobby) joined the same channel so a tile with a volume control exists. Three runs (about 6 minutes of lock time).

| popover | reached | result |
|---|---|---|
| Soundboard (`.vc-sb-popover`) | yes, from the channel list's voice panel (compact button) | panel 100..367, scroll 265/265, cut [] => whole (Dark, Light); a control inside was pressed |
| Privacy codes (`.vc-e2ee-popover`) | yes | panel 8..367, scroll 357/357, cut [] => whole (Dark, Light) |
| Per-user volume (`.vc-volume`) | yes (bobby's tile) | panel 66..367, scroll 299/299, cut [] => whole (Dark, Light) |
| Input / output device menus (`DevicePopover`, user panel) | NOT REACHED | the user panel is not on the phone's call page, and on the channel list it is off the screen; its desktop place is shown in the 1280 shots. Listed again |
| Microphone / Camera options (the chevrons of the phone's call bar) | NOT REACHED | the spec could not press them in the three runs (not investigated; no time) |

No voice CSS changed: nothing was cut. **Observation, not fixed:** in the shots taken after the spec pressed "Back to channels" the call page sits about 16 px left of the screen edge (`voice-list-*.png`): the phone shell slid sideways. Not a popover; whoever owns the phone shell may look. Not measured against the base.

## Who imports my files (no web build)
`header.css`: `FollowButton`, `PhoneSearch`, `PinsButton` (Lounge). `features.css`: `SavedButton`, `ScheduleDialog` (Lounge). `lounge.css`: Lounge. `msg.css` (untouched): `Composer`, `MessageList`. `voice-p13.css` and `voice-p14-e2ee.css` (untouched): `Tiles`, `StageView`, `DmCallStage`, `VoiceStatusBar`, `SoundboardPopover`, `PrivacyCodes`: all Lounge chunk; `main.tsx` imports `VoiceBridge` and `IncomingCall`, which import neither. No first-paint or public file touched.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-popovers`: **34 PNGs, all opened by me**: base Pins and Saved (2); Pins and Saved Dark and Light (4); stickers, GIF categories, GIF results at phone and desktop, Dark and Light (12); voice join and call, Dark and Light (4); soundboard, privacy codes, volume, Dark and Light (6); the `voice-list-*` shots of the unreached device menus (6). `measurements.txt` (last voice run), `measurements-run1.txt`, `measurements-run2.txt` (earlier voice runs, the part 1 and 2 numbers are in `measurements-run1.txt`'s sibling files: see run1). Theme set by `PATCH /users/@me/settings` and localStorage; `data-theme` read back from `.lng-root` and logged each time (`lounge-dark` / `lounge-light` matched).

## Tests
`e2e/tests/p79-popovers-shots.spec.ts` (new, local only): base case watched fail-on-base (it asserts the fault: passed with the CSS held back); parts 1 and 2 pass; the voice test passes with the unreached cases logged. e2e `tsc` clean, prettier clean on the four files, `check-css-tokens` clean, `i18n-audit --max 0` = 0. No vitest.

## Docs
No doc states these popovers' widths (searched `docs/`, apart from wave 78's handoff). Nothing updated.

## Proposed inventory row
`SEC-AUD-316` (web, Lounge): the Pins and Saved-items popovers carry their own `box-sizing: border-box; max-inline-size: 100%`; wave 78's raised-specificity rule is gone; the sticker and GIF grids and the soundboard, privacy-codes and volume popovers were measured whole on a phone in a real call (D-818). Device menus and the call bar's mic/camera menus not reached: open. Close on merge, with the open tail named.

## Changelog sentence
None a member would notice (the popovers look the same; their limits now live in their own files).

## Owner questions
None. Defaults: Saved items' kit tab strip left to kit's item.

## Cleanup
Stack stopped by PID/port, `sr_p79_popovers_ui` dropped, lock released. Scratch in `SIDEROOM_wt/p79-popovers-scratch` (untracked). No stash touched.
