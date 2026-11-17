# Wave 90, entrance: SEC-AUD-389 (D-900), the Entrance page's hero image control

T = 05:50 on 2026-11-17. Code and vitest by 05:56 (commit `820b1a1f`). Lock: queued from about 05:57 (the first `acquire` answered
75 after 9 minutes, read as busy, tried again), taken 06:07, round 06:07 to 06:13 (three attempts, see below), released 06:13.
Branch `fix/p90-entrance`, nothing pushed.

## What the base showed (run, not assumed)
`EntranceHero.tsx:88-90` was a bare `<input type="file">` inside a `label.ss-field`: the browser's own "Choose File" button and
file-name text. At 375 (Lounge Light) its box is 309 x 38 and it **fails the hit test** (D-856); at 1280 Editorial it is 344 x 38.
Accessible name "Choose an image" (from its label).

**Every other bare native control of the page** (`Entrance.tsx` and `EntranceHero.tsx`: every `input`, `select`, `textarea` and
`button` without a kit class, found in the rendered page at 375 and 1280): **none.** The file input was the only one; the rest
are the kit's `Button`, `TextInput` and the copy editors' kit fields.

## What changed (commit `820b1a1f`, spec fix and handoff in the second commit)
- `admin/sections/EntranceHero.tsx`: the bare input is the kit `FileDrop` (button label "Choose an image", the existing
  `admin.entranceHero.fileLabel`; `accept` the same `image/png,image/jpeg,image/webp`; `maxSize` `HERO_MAX_BYTES`; the chosen
  file's name shown as the control's hint). The size check, the upload call, the preview and the remove control are unchanged;
  the input `useRef` is gone.
- **One behaviour difference:** `FileDrop` filters by `accept` itself, so a wrong type (a text file) is now refused on the page
  with a new message and Upload stays disabled. On the base the page accepted it and left the refusal to the server. An
  over-8-MiB file shows the same existing "That file is over 8 MiB."
- New fragment `apps/web/src/i18n/messages/en/p90-entrance-1.ts`: `admin.entranceHero.wrongType` = "Choose a PNG, JPEG or WebP
  image." (in `en/`; the commander's `staff-keys.mjs --write` moves it).
- `admin/__tests__/p13entrance.test.tsx`: the two existing hero cases found the input by `getByLabelText('Choose an image')`;
  the kit control carries its name on the button and hides its input, so both now reach the input through the button's sibling
  (`heroInput` helper). One new case: a wrong type is refused, then a PNG chosen is shown by name and enables Upload.
- `e2e/tests/p90-entrance-shots.spec.ts` (new).

## D-385: who imports the two files
`git grep -n "EntranceHero\|sections/Entrance" -- apps/web/src`: `EntranceHero.tsx` is imported by `admin/sections/Entrance.tsx`
only; `Entrance.tsx` by `admin/AdminApp.tsx` only (staff, the Admin chunk). `routes/entrance/Landing.tsx` imports
`entranceCopy.tsx`, not these files. `EntranceHero.tsx` itself imports `resetEntranceHero` from `routes/entrance/entranceCopy`
(existing, unchanged). No web build, no static import added anywhere.

## Figures (375 Lounge Light and Dark, 1280 Editorial; base run first, after run second, same round)
| | base | after |
|---|---|---|
| control box at 375 | input 309 x 38, hit test FAILS | button 171.6 x 44, hit test passes (light and dark) |
| control box at 1280 | input 344 x 38 | button 171.6 x 36 |
| accessible name | "Choose an image" (label) | "Choose an image" (button) |
| keyboard | n/a | the first Tab from the card heading reaches the button; Enter and Space each fire `filechooser` (asserted in all three settings) |
| wrong type (text file) | accepted, Upload enabled | refused with the new message, Upload disabled |
| PNG via `setInputFiles` | uploads, preview shows | same (all three settings) |
| hero card 375 light: no image / image / image + file chosen | 425 / 664.8 / 582.8 | 491 / 730.8 / 672.8 |
| hero card 375 dark: no image / image | 507 / 664.8 | 573 / 730.8 |
| hero card 1280: no image / image / image + file | 448 / 652 / 570 | 530 / 734 / 676 |
| sideways scroll | 0 | 0 at 375 and 1280 |

The card is 66 px taller on a phone (82 px at 1280) with the control in its place, from the drop zone's padding (D-890); the
"image + file chosen" figures differ more because the base measures the file name on the browser's own line. The no-image
heights differ between themes because the hint text wraps differently in each theme's font; base and after agree theme by theme.
Nothing else on the page moves but by the card's growth. "or drop a file here" is the kit's own hard-coded English
(`packages/ui/.../FileDrop.tsx`): listed for the kit, not mine.

## How the spec runs (and why the upload is faked)
The sweep's Owner has no second factor, and the hero upload and removal need a fresh one (`withReauth`): the first attempt (a real
upload) stopped at the re-auth dialog. The spec therefore answers the three hero routes with `page.route` (PUT checks a PNG data URI
and a reason, GET serves the stored bytes, DELETE clears): the page's own requests are real, the server's answer is not. Attempts:
(1) real upload, failed on re-auth; (2) a stale `e2e/.state` from a dropped database gave 401 (the round script now removes it before
seeding); (3) clean: base and after, three settings each, one lock round. The sweep seed printed `FAIL suspended app` (2FA), as the
brief says; the world was usable.

## Shots: `visual-sweep-shots/p90-entrance`, 5, all 5 opened by me after the release
`base-phone-light-lounge-light.png`, `base-desk-lounge-editorial.png`, `after-phone-light-lounge-light.png` (a second file chosen,
the first one's preview showing, its name inside the drop zone), `after-phone-dark-lounge-dark.png`,
`after-desk-lounge-editorial.png`. `data-theme` is read back and asserted per setting. The base 375 Lounge Dark setting is measured,
not shot. Measurements: `measure-base.json`, `measure-after.json` in the same folder.

Re-shoot (from `e2e/`, a stack on 8685 / 5685 with the sweep world seeded):
`SR_P90_ENTRANCE_ON=1 SR_P90_ENTRANCE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-entrance SR_E2E_BASE_URL=http://localhost:5685 npx playwright test tests/p90-entrance-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(the base case needs the old `EntranceHero.tsx` and `SR_P90_ENTRANCE_BASE=1`).

## Tests run
vitest `admin/__tests__/p13entrance.test.tsx`, 7 passed (two cases changed as above, one new; I did not run them against the base
file: the Playwright base run shows the base behaviour). Web `tsc`, eslint, prettier, `i18n-audit --max 0` (0), e2e `tsc`: clean.
The spec: base run and after run both passed.

## Docs
No doc states the hero's file control (searched `docs/` and `README.md`); nothing to change.

## Cleanup
`sr_p90_entrance_ui` dropped, `p90-entrance-bin` deleted (`p80-base-bin` untouched), ports 8685 / 5685 / 9385 free. A Vite child
survived its parent's tree kill twice; I killed it by PID, and the round script now kills the listeners of the three ports. Lock
released. No stash touched.

## For the commander
- Proposed row SEC-AUD-389: "The Entrance admin page's hero image is chosen with the kit `FileDrop` (named, 44 px on a phone,
  keyboard-operable, a wrong type refused on the page); the bare file input is gone. Closed (D-900)." SEC-AUD-363 loses its
  clause "the Entrance page's bare file input".
- Changelog: "Admin > Entrance pages: the hero image is chosen with the same labelled, 44 px file control as the kit's, and a file
  that is not a PNG, JPEG or WebP is refused on the page."
- Threat-model rows: none. AGENTS.md: nothing.
- Owner question (default taken): a wrong type is now refused on the page, with its own message, instead of by the server.
- Kit note: `FileDrop`'s "or drop a file here" is not translated (kit file, not mine).
