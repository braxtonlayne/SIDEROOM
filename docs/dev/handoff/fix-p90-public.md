# Wave 90, public builder: SEC-AUD-384 (D-897), the enabled guestbook "Sign" button follows the profile; the shared contrast helper

T = 05:50 (2026-11-17). Ready 05:57 (spec and CSS drafted; binaries copied 05:51), lock taken 05:54 (round 1, wasted: a regex bug in the helper that I had not run offline; released 05:56), 06:21 (round 2, wasted: the sweep seed returned at once because a stale `e2e/.state/sweep` was left by round 1; released 06:21), 06:30 (round 3, good; released 06:32). Web build: see "Build and checks". Finished: see the end.

## What I saw on the base (I ran it: `/u/owner`, sweep world, `SR_P90_PUBLIC_BASE=1`, round 3)
- The enabled button is `button._button._primary._sm` (a direct child of `form.pp-sign`): Lounge Light and Dark white rgb(255,255,255) on blurple rgb(88,101,242), **4.61:1**, border 1px transparent, opacity 1; Editorial dark rgb(27,26,21) on tan rgb(194,176,143), **8.22:1**; Editorial hover tan rgb(209,193,162) 9.84:1.
- Focus ring (kit, `outline: 2px solid var(--ui-focus)`, offset 2): Lounge Light rgb(0,104,201), **2.51:1** on the panel rgb(43,45,49) (under 3:1); Lounge Dark rgb(0,168,252) 5.26:1; Editorial rgb(217,201,164) 8.43:1.
- The disabled button (wave 89's rule) is the profile's: rgb(242,243,245) on 45% accent over the panel, 7.72:1. So in Editorial the button went from the profile's blue to the app's tan when a note was typed (shot `before-form-enabled-desk-editorial.png`).

## The rules as built (`packages/profile-blocks/src/profile-blocks.css`, commit `e6cca0c8`, next to wave 89's `:disabled` rule)
```css
/* SEC-AUD-384, D-897: the enabled button takes the profile's colours too (the accent, a little of the profile's bg so the text reads). */
.pp-sign > button:not(.pp-link):not(:disabled) { color: var(--pp-text); background: color-mix(in srgb, var(--pp-accent) 85%, var(--pp-bg)); }
.pp-sign > button:not(.pp-link):not(:disabled):hover { background: color-mix(in srgb, color-mix(in srgb, var(--pp-accent) 85%, var(--pp-bg)) 85%, var(--pp-text)); }
.pp-sign > button:not(.pp-link):not(:disabled):focus-visible { outline: 2px solid var(--pp-text); outline-offset: 2px; }
```
Only `--pp-*` variables; no `--ui-*`, no theme or section name, no new variable. Specificity 0,3,1 / 0,4,1 / 0,4,1 against the kit's 0,1,0 / 0,4,0 / 0,2,0. They do not match `button.pp-link` (signed-out) or `:disabled`. No border rule: the kit draws none (transparent).
**The pairing is not the plain accent.** `--pp-text` on the plain `--pp-accent` is 4.13:1 by my arithmetic (rgb(242,243,245) on rgb(88,101,242)), under 4.5. So the ground is 85% accent with 15% of the profile's `--pp-bg` (a package variable): 5.04:1. A profile whose accent is lighter or darker lands differently (the accent is the member's own choice); the mix always darkens toward `--pp-bg`.

## Figures after (from `e2e/support/contrast.ts`; same in all three settings because the profile's own colours do not change with the app theme)
| | 375 Lounge Light | 375 Lounge Dark | 1280 Editorial |
|---|---|---|---|
| enabled: text / ground | rgb(242,243,245) on 85% accent+15% bg = rgb(79,90,211), **5.04:1** | same, **5.04:1** | same, **5.04:1** |
| enabled, base | white on blurple 4.61 | 4.61 | dark on tan 8.22 |
| hover (mouse, 1280 only) | n/a (touch) | n/a | ground rgb(104,113,216), **3.82:1** |
| keyboard focus ring (2px, `--pp-text`) on the panel | **12.42:1** (kit was 2.51) | 12.42 (kit 5.26) | 12.42 (kit 8.43) |
| disabled button | 7.72, computed styles equal to the base | 7.72, equal | 7.72, equal |
Enabled computed colour and ground are equal in all three. The hover figure is under 4.5:1 (the hover mixes in `--pp-text`, which lightens the ground): it is above 3:1 and the shift is plain (rgb(79,90,211) to rgb(104,113,216)); see owner question 1.
Boxes, base against after: the form (28,607 141.5x117.8 at 375; 28,511 594x117.8 at 1280) and the button (28,696.8 141.5x28; 28,600.8 594x28) identical, 0 px moved; `scrollWidth - clientWidth` is 0 at 375 and 1280.
Signed-out `/~owner` at 375: label rgb(242,243,245) opacity 1 (12.42), span `.pp-muted` opacity 0.75 (3.93), button `.pp-link` opacity 0.6, border rgb(88,101,242) (2.27): equal to the base, value for value.

## The helper
`e2e/support/contrast.ts`: `parseColor` (rgb, rgba, `color(srgb r g b)` with and without `/ a`, hex; anything else throws), `over`, `luminance`, `ratio`, `textRatio`. Self-check in the spec, on wave 89's colours: label **7.70** (target 7.70), disabled button **7.73** (target 7.72; within 0.05; it runs in every run and fails the spec if off by more than 0.05). My first version had its regexes written with doubled escapes and failed to parse any colour; the self-check test caught it (and it threw rather than returning a number). Nobody else imports it this wave.

## Build and checks
(filled in below)

## Tests run
`packages/profile-blocks` `blocks.test.tsx`: 21 passed, as it is (it asserts no colour; the base/after run is the check). The spec ran on the stack, base and after (both passed: self-check plus measure). I did not watch a test fail for the change itself: the spec asserts only the helper's self-check, the base case is the evidence (the figures above). prettier clean (css, helper, spec), `check-css-tokens` clean, `check-doc-links` ok, e2e `tsc` clean.

## Shots (`visual-sweep-shots/p90-public`: 14 PNG, 2 measure JSON, `round.log`; the eight required, **all eight opened by me after releasing the lock**)
Base: `before-form-enabled-desk-editorial.png`, `before-form-enabled-phone-light.png`. After: `after-form-enabled-phone-light.png`, `after-form-enabled-phone-dark.png`, `after-form-enabled-desk-editorial.png`, `after-form-focus-desk-editorial.png` (keyboard focus; the form's own screenshot clips the ring's outer edge, the figure is from the computed outline), `after-form-disabled-desk-editorial.png`, `after-signedout-form-phone-light.png`. The other six `before-*` files (disabled, focus, Dark, signed-out) were not opened; their numbers are in `before-measure.json`. What I saw: after, the enabled button is the profile's blue in all three settings (no tan, no brighter blurple), text clearly readable, the disabled one is the dimmer blue; signed-out form as before.

## Docs
No doc states these colours. Nothing updated; `check-doc-links` ok.

## Commander's command (also in the spec header), from `e2e/`, on a stack with the sweep world seeded (Vite 5681, API 8681, metrics 9381; `rm -rf e2e/.state` first on a new database, or the seed step returns at once)
`SR_P90_PUBLIC_ON=1 SR_P90_PUBLIC_TAG=after SR_P90_PUBLIC_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-public SR_E2E_BASE_URL=http://localhost:5681 npx playwright test tests/p90-public-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`SR_P90_PUBLIC_BASE=1` for the base case, on code without the CSS commit).

## Proposed row for SEC-AUD-384 (closing text)
Closed (wave 90, D-897): on a profile's guestbook form the enabled "Sign" button takes the profile's colours in the package sheet (85% `--pp-accent` with `--pp-bg` as ground, `--pp-text` as text 5.04:1; hover and a `--pp-text` focus ring of 12.42:1, the kit's was 2.51:1 in Lounge Light), so it no longer turns tan in Editorial or blurple in the Lounge themes; the disabled button, the form's size and the signed-out form are unchanged; the kit is untouched. Shared helper `e2e/support/contrast.ts` added (reads `color(srgb ...)`).

## Changelog sentence
The enabled "Sign" button on a profile's guestbook form now takes the profile's own colours, with a clearer focus ring, in every app theme.

## Owner questions
1. The hover is 3.82:1 (above 3:1, under 4.5:1) because it lightens toward `--pp-text`. Should hover darken instead (more `--pp-bg`) so it keeps 4.5:1? Default taken: lighten, as the task said.
2. The ground is a mix of 85% accent and `--pp-bg`, not the plain accent (4.13:1 on the seeded profile). Say if the plain accent is preferred.

## Clean-up
(filled in below)
