# Wave 89, public builder: SEC-AUD-382 (D-895), the guestbook form takes the profile's colours

T = 17:18. Ready 17:17, lock taken 17:18, stack round 17:18 to 17:19 (base and after in one run), lock released 17:20, web build 17:20 to 17:21 (one-shot, three checks), finished 17:25.

## What I saw on the base (I ran it: `/u/owner`, sweep world, `SR_P89_PUBLIC_BASE=1`)
- DOM path: `form.pp-sign > div._field > label._label` (the kit's label is one level down, inside the TextArea's field `div`, not a child of the form); `form.pp-sign > button._button._primary._sm` (a direct child). The signed-out form is `form.pp-sign > label > span.pp-muted` and `form.pp-sign > button.pp-link`.
- 375 Lounge Light: label rgb(92,94,102) on the panel rgb(43,45,49), **2.13:1** (as wave 88). Disabled "Sign": white on accent rgb(88,101,242) at opacity 0.5.
- 375 Lounge Dark: label 5.9:1 (kit colour follows the theme). 1280 Editorial: label 6.1:1; disabled button rgb(27,26,21) on tan rgb(194,176,143) at 0.5 (3.06:1 by my probe).
- The enabled "Sign" is the kit's: Light/Dark white on rgb(88,101,242); Editorial dark text on tan.

## The two rules as built (`packages/profile-blocks/src/profile-blocks.css`, commit `4c9bff52`, beside the `.pp-sign` rules)
```css
/* SEC-AUD-382, D-895: the form sits on the profile's own ground, so its label and its disabled button take the profile's colours, not the app theme's. */
.pp-sign :not(label) label { color: color-mix(in srgb, var(--pp-text) 75%, transparent); }
.pp-sign > button:not(.pp-link):disabled { opacity: 1; color: var(--pp-text); background: color-mix(in srgb, var(--pp-accent) 45%, transparent); }
```
Specificity of the label rule is 0,2,1 (beats the kit's one class). It does not match the signed-out form (its label has no non-label ancestor under the form). Only `--pp-*`; no `--ui-*`, no theme, no section name.

## Figures after (computed by hand from the probe's colours; the spec's own ratio cannot read `color(srgb ...)` and prints wrong numbers for the two rules: label 1.42, button 15.85; ignore those two columns)
The panel ground is rgb(43,45,49) and `--pp-text` rgb(242,243,245) in all three settings.
- Label: 75% text over the panel = rgb(192,194,196), **7.70:1** in 375 Light, 375 Dark and 1280 Editorial (was 2.13 / 5.9 / 6.1). The 75% mix passes, so no plain `--pp-text`.
- Disabled button: ground = 45% accent over panel = rgb(63,70,136); text rgb(242,243,245) on it **7.72:1** in all three (was faint at 0.5 opacity). It differs plainly from the enabled button (solid, brighter; in Editorial the enabled one is the app's tan, the disabled one the profile's blue).
- Form boxes, base against after: identical in all three (375: 28,607 141.5x117.8; 1280: 28,511 594x117.8), 0 px moved.
- Enabled button (a note typed, not sent): computed colour, background, opacity equal the base in all three.
- Signed-out `/~owner` at 375: label, span `.pp-muted` and `button.pp-link` computed colour, background, opacity, border equal the base (label rgb(242,243,245), span opacity 0.75, button opacity 0.6, border rgb(88,101,242)).

## Build and checks (17:20 to 17:21, exit 0)
`check-bundle-budget` exit 0: first paint `app/app-6LyMCkeo.js` **218 303 B gzip** (wave 88: 218 317 B; -14 B), room 37 697 B; `profile` entry 115.1 KB / 150 KB. `check-entrance-isolation` exit 0, every public surface clean. `check-staff-delivery` exit 0.

## Tests run
`packages/profile-blocks` `blocks.test.tsx` 21 passed (as it is; it asserts no colour, the spec's base and after runs are the check). prettier clean (css, spec), `check-css-tokens` clean, `check-doc-links` ok, e2e `tsc` clean, the spec ran on the stack (base and after, both passed). Did not run the web tsc separately (the build's first step passed).

## Shots (`visual-sweep-shots/p89-public`: 6 form shots with `after-`, 4 with `before-` plus the two measure JSON files and `build.log`)
Required six: base 375 Light close up (`before-form-disabled-phone-light.png`); after disabled at 375 Light, 375 Dark, 1280 Editorial (`after-form-disabled-*.png`); after enabled with a note at 375 Light (`after-form-enabled-phone-light.png`); signed-out form at 375 (`after-signedout-form-phone-light.png`). **I opened all six**, after releasing the lock. Close-ups are the form element only. The other `before-` shots (Dark, Editorial, enabled, signed-out) were not opened; their numbers are in `before-measure.json`.

## Commander's command (also in the spec header), from `e2e/`, on a stack with the seeded world (Vite 5671, API 8671, metrics 9371)
`SR_P89_PUBLIC_ON=1 SR_P89_PUBLIC_TAG=after SR_P89_PUBLIC_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p89-public SR_E2E_BASE_URL=http://localhost:5671 npx playwright test tests/p89-public-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`SR_P89_PUBLIC_BASE=1` for the base case).

## Docs
No doc states these colours. Nothing updated.

## Closing text for SEC-AUD-382
Closed (wave 89, D-895): on a profile's guestbook form the "Leave a message" label and the disabled "Sign" button take the profile's own colours (`--pp-text`, `--pp-accent`) in the package sheet, so the label reads 7.7:1 (was 2.13:1 in Lounge Light) and the disabled button's text 7.7:1 in every app theme; the enabled button, the form's size and the signed-out form are unchanged; the kit is untouched.

## Changelog sentence
The guestbook form on a profile now uses the profile's own text and accent colours for its label and its disabled "Sign" button, so both read clearly in every app theme.

## Owner questions
None. (Editorial note: the enabled button stays the app theme's colour, the disabled one is the profile's accent; say if the enabled one should follow the profile too.)

## Clean-up
Stack stopped by PID tree, ports 8671/5671/9371 free, `sr_p89_public_ui` dropped, `p89-public-bin` deleted (`p80-base-bin` untouched), lock released. No stash touched.
