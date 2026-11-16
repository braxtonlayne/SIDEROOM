# Wave 86 settings builder: SEC-AUD-357 (D-883), the Library defaults card's labels

T = 16:10 on 2026-11-16. No new item from 16:50, checkpoint 17:05, finished 17:15, hard limit 17:25. Lock `settings` held 16:11 to 16:14 (one round of three minutes).

## Done
- Commit `SEC-AUD-357: Library defaults card controls get visible labels (D-883)` (+ this handoff and the spec fix in the second commit).
- **Base, run on the stack (before shots with the base file swapped in, then restored):** at 375 (Lounge Light and Dark) and 1280 (Editorial) the card `#us-library-defaults` had NO drawn label over Reading mode, Reading direction and Background (the kit's `SegmentedControl` only puts an `aria-label` on its radiogroup). The preload slider already draws its own `<label for>` (kit `Slider`), so it is no fault and is listed, not changed. Accessible names before: "Reading mode", "Reading direction", "Background" (aria-label), "Pages to load ahead" (label for). The fault was real.
- **Build:** `LibraryDefaultsCard` only: each of the three segmented controls is wrapped in a `<div>` with `<span className="us-seg-label" aria-hidden="true">` above it, the string being the control's own `label` (no new string), exactly as `Appearance.tsx` does for Cinema/Library navigation since wave 84. The control keeps its own `aria-label`, so it is named once (the visible span is aria-hidden). `us-seg-label` is an existing class in `user-settings.css` (the one Appearance uses), so no CSS file touched and no class wanted. (The task text named `us-muted`; `us-muted` would have differed from Appearance, so I followed Appearance. List: the Cinema card's labels use `us-muted`, the Appearance ones `us-seg-label`; the two cards now look different in label weight. Owner question below.)
- **After (probed on the stack):**
  - Reading mode: name "Reading mode", visible label "Reading mode" (96 px wide text, 17 px tall) directly above the control (23 px gap to its top edge), label text start = control start = card text start (33 px at 375, 357 px at 1280).
  - Reading direction: "Reading direction" (117 px). Background: "Background" (80.5 px; the string `readerBackground` is "Background"). Slider: "Pages to load ahead" (its own kit label, unchanged).
  - Each control's accessible name equals its label's text. All four labels above their controls at the card's text start (the slider's own label sits in the kit's label row; at 375 its 44 px touch box overlaps the row, as before).
  - Contrast of the labels on the card: Lounge Light 11.39:1 (rgb 49,51,56 on 242,243,245), Lounge Dark 10.21:1 (219,222,225 on 43,45,49), Editorial 13.89:1 (239,230,210 on 28,27,23). All over 4.5:1. 0 sideways scroll in all three.
  - Persistence: Reading mode paged -> strip, reload: strip kept; set back to paged, reload: paged.
  - What moved: the Library card is taller (347 -> 416 px at 375; 286 -> 355 at 1280, three labels of 17 px plus gaps); everything inside it moves down by the labels. **The Cinema defaults card: 0 px moved** (90 element boxes compared relative to the card, before vs after: identical to float rounding).
- Tests: vitest `settings/user/__tests__/p15Defaults.test.tsx` (the one file that renders the card), one case more (three radiogroups found by role and name, each with an aria-hidden visible label as sibling; the slider by role and name with a real `<label>`): 4 of 4 pass (ran it before the shots; the new case failed once on a wrong name, "Background" not "Reader background", then passed; I did not run it against the base). web `tsc` clean, eslint (package's own) clean, prettier clean, `check-css-tokens` clean, `i18n-audit --max 0` TOTAL 0, e2e `tsc` clean.
- Who imports `ActivityHistory.tsx`: only `settings/user/sections/Activity.tsx` (lazy Settings chunk); no web build run, first paint untouched.

## Shots
Folder `visual-sweep-shots/p86-settings`: 6 PNGs (before and after for phone-light, phone-dark, desk-editorial) plus `before-measure.json` and `after-measure.json`; **all six opened and looked at** after the lock was released. (The brief wanted before once at 375 Light; I shot it in all three: cheap.)

## Commander's command (from `e2e/`, on a stack with the sweep world, Vite 5643, API 8643)
`SR_P86_SETTINGS_BASE=1 SR_P86_SETTINGS_TAG=after SR_P86_SETTINGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-settings SR_E2E_BASE_URL=http://localhost:5643 npx playwright test tests/p86-settings-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
(plus `DATABASE_URL` and `SR_CLI_BIN` as the brief says.) The spec passes (24 s).

## Proposed inventory row
SEC-AUD-357 (closes the last open line): The Library defaults card's Reading mode, Reading direction and Background controls had no visible label (only an aria-label); each now has the visible label above it as Appearance does (D-883); the preload slider already had one. Contrast 10.2:1 to 13.9:1.
## Changelog sentence
Settings: the Library defaults card now shows a visible label over Reading mode, Reading direction and Background.

## Not done / owner questions
- Nothing left. Owner question (to Fable): should the Cinema card's `us-muted` labels and the Library/Appearance `us-seg-label` labels share one style? Default kept: follow Appearance.
- Stack stopped by PID tree, ports 5643/8643/9343 free, `sr_p86_settings_ui` dropped, `p86-settings-bin` deleted; `p80-base-bin` untouched. No git stash used or touched.
