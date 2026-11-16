# Wave 86, kit builder: SEC-AUD-373 (D-879)

T = 16:10 on 2026-11-16 (clock times: no new item 16:50, checkpoint 17:05, finished 17:15, hard limit 17:25). I started early (my `date` read 15:55 at the start) and finished by 16:11 by the machine's clock.

## Done
`FileDrop` button and a multi-select chip's remove button are 44 px touch targets on a phone. CSS only, two sheets, in the phone query (`max-width: 720px`) the kit already uses (`Select.module.css`'s D-856 block):
- `primitives/FileDrop/FileDrop.module.css`: `.button { min-height: 44px }`.
- `primitives/Select/Select.module.css`: `.chipRemove { position: relative }` and a `::after` hit area, `block-size: 44px` centred on the button, `inset-inline: -24px -4px` (24 px toward the chip's label, 4 px past the button's end, which is the chip's own padding). No `.tsx`, no token, no other sheet.

## What I saw on the base (the spec's base case, run: `SR_P86_KIT_BASE=1`, sheets held back)
375 x 812 touch, Lounge Light and Lounge Dark, server settings: Community > Onboarding "Default channels" multi-select (two chips: `#general` and a long channel name, which wraps to two lines) and Emoji & Stickers `FileDrop`.
- Chip remove button: 16.6 x 16 px box; hit test above, below, left and right all fail (false x4) on both chips.
- `FileDrop` button: 139 x 36; hit test above and below fail, left and right pass. Card 343 x 120.
- The multi-select's chevron (32 wide x 116 tall here: the row wraps): up/down pass, left/right fail (it is 32 wide; not in this task, unchanged).
The fault is as the plan says.

## After
- Lounge Light and Lounge Dark, 375: chip remove buttons pass above, below and left; **right fails** (centre + 21 px lies 12.7 px past the button's end, which is where the chevron or the next chip is; I did not cover those, the contract says not to). Hit area: 44 px tall, about 44 px wide (button 16.6 + 24 + 4 = 44.6) of which 4 px is past the button's end. `FileDrop` button 139 x 44: all four sides pass.
- A touch 20 px above the centre of the second chip's remove button removes the second chip only (`#general` stays), both themes. A touch on the chevron's centre opens the list, both themes.
- Boxes that moved at 375: the `FileDrop` card 120 -> 128 px (+8, the button's +8). The multi-select control (116 px, two chips) and the chips did not move (0 px). A wrapped chip row's neighbouring rows keep 4 px gaps, so hit areas of chips in different rows overlap each other's lower/upper part; the later row wins the overlap (the first row's button keeps its centre; its "down" 21 px may be taken by the row below). Not seen here (the two chips are on two rows: the first chip's hit test passed).
- 1280, Editorial: 0 px moved (chips, control, `FileDrop` button 36 px, card 120 px: identical to the base).

## First paint and the three checks (the wave's one web build, `heavy.sh run kit`)
- Before (wave 85): 218 321 B gzip. After: `app/app-Bh3fFr6l.js` 218 312 B gzip, room 37 688 B (9 B less: a hash change only; CSS of the kit does not go through this chunk's JS).
- `check-bundle-budget`, `check-entrance-isolation` ("every public surface is clean") and `check-staff-delivery`: all exit 0. Build exit 0.

## Tests run
`packages/ui/src/primitives/select.test.tsx` (8 passed) and `forms.test.tsx` (18 passed), single files. `check-css-tokens`, `i18n-audit --max 0` (0), `check-doc-links` ok, prettier on both sheets and the spec, e2e `tsc` clean. No eslint (no `.ts`/`.tsx` source changed apart from the spec, which is prettier-formatted and type-checked). The spec ran in base and after mode on the real stack; the base case failed the hit test as listed above.

## Shots
Folder `visual-sweep-shots/p86-kit`, 17 PNG files plus `measure.jsonl` and `base-measure.jsonl`; **I opened all 17**: base multi-select 375 light/dark and 1280 (3), base `FileDrop` 375 light/dark and 1280 (3), after multi-select 375 light/dark and 1280 (3), after the two taps (list open, one chip left) in light and dark (2), after `FileDrop` 375 light/dark and 1280 (3), plus one reopen of the 375-light after shot. Shot 375 as 375 x 812 touch; 1280 as 1280 x 800 (Lounge Editorial).

## Commander's command for the spec (from `e2e/`, on a stack with the sweep's world, API 8641, Vite 5641)
`SR_E2E_BASE_URL=http://localhost:5641 SR_P86_KIT_ON=1 SR_P86_KIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-kit npx playwright test tests/p86-kit-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (add `SR_P86_KIT_BASE=1` for the base case). Header of the spec says the same.

## Proposed inventory row
SEC-AUD-373: kit, phone: `FileDrop` button 36 -> 44 px and a multi-select chip's remove button gets a 44 px tall, about 44 px wide hit area (D-879); touch on its right side beyond 4 px past the button is not covered (it would reach the chevron or the next chip). Closes the wave 84 "not covered" pair.

## Changelog sentence
On a phone the file picker's button is taller, and removing a chosen option from a multi-select is easier to hit.

## Owner questions
1. The chip's hit area stops 4 px past the button's end so it never covers the next chip or the chevron; a tap just right of the "x" misses it (right-side test fails). Keep (default), or let it reach into the chevron's edge?
2. The `FileDrop` card is 8 px taller on a phone. Keep (default).

## Cleanup
Stack stopped by PID tree (ports 8641, 5641, 9341 free), `sr_p86_kit_ui` dropped, `p86-kit-bin` deleted (`p80-base-bin` untouched), lock released. No stash touched. `apps/web/dist` from the build is ignored by git.
