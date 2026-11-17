# Wave 93, kit: SEC-AUD-416 and the control of SEC-AUD-414 (D-922), the segmented control and slider keep their phone target inside their own box; one spec fix; the wave's one web build

Clock: started 12:54 (T = 13:15; the lock was free early). Spec written, typechecked and committed 13:01. Lock taken 13:02 behind dock; one stack round 13:02 to 13:06 (seed, base run, rule written, after run, old spec case c, stack stopped by PID tree, ports 8711/5711/9411 free, lock released 13:06). Web build: see the last section.

## What I saw on the base (375 touch, Lounge Light, `e2e/tests/p93-kit-shots.spec.ts`, shared helper only)
Every radio and range input of Voice & Video and Profiles' "view as" through `hitTest` (the radio with the helper's default wrapper, its label; the range with `wrapper: '[class*="slider"]'`), the group's box, the three points, the taker and its box.
- **Voice & Video, "Optimise for", Motion pill (73.7 x 32 at 36,421):** the point 21 px below is taken by the next control's input, a switch input (`input._input < div._choice < section.us-card`, box 38 x 22 at 33,467; its own target reaches above its box). The pill's group ends 2 px above the point. A real fault: a tap there would flip "Share audio by default". The Detail pill passes.
- **Voice & Video, "Sound volume" slider (309 x 44 at 33,414.8):** below is taken by the next slider's input (309 x 44 at 33,457.8), 11 px outside the slider's own box (group 403.8 to 446.8; the range reaches 12 px past it). The other five sliders pass (what follows them is not a later control).
- **Profiles' "view as", second row ("Shares a server" 131.5 x 32, "Friend" 69.7 x 32):** below is taken by the preview block (`div.pp-root < div.us-view-as`, which starts at 455.6, where the group ends; the point is 457.6). The first row passes.
- No taker was the pill's own label or `::before`: every taker is another control or the preview block, so none was dropped.
- 20-page pass on the base (`base-pass.json`): the kit controls failing are those 4 (Voice & Video 2, Profiles 2). The other fails are not the kit's: My Account "Edit username" and "Edit display name", Profiles "Edit your profile page", the three public-page link rows Bitcoin, Ko-fi and Blog inside the view-as preview, Support / Patron "View leaderboards" (settings: D-923, D-921).

## What changed
`packages/ui/src/primitives/Slider/Slider.module.css`, inside the 720 px block only (commit `6dabde35`):
- `.input { height: 44px; margin-block-start: -12px; margin-block-end: 0; }` (was `margin-block: -12px`): the 12 px at the bottom no longer hang out over the next control. The top 12 px still lie over the slider's own label row (same group, painted by the input), so a slider grows 12 px, not 24: the plan's `margin-block: 0` form would have cost 24 px per slider and nothing above the range takes a point.
- `.segmented { row-gap: 12px; margin-block: 4px; }` (new `margin-block`): the margin box holds each pill's 44 px target (it reaches 6 px past the 32 px pill; border and padding cover 3, the margin the rest). Inline-flex margins do not collapse; the wrapper-padding form was not needed.
- The pill's `::before` and the input's 44 px do not change. Nothing changes above 720 px.
- **The CSS in the last commit is the CSS I measured** (written once, measured on the same stack, not touched after).

## After (same round, same stack)
- 375 Lounge Light and Lounge Dark, Voice & Video: all 10 kit controls (6 sliders, 4 pills) pass, each its own taker (range: itself; pill: itself at the centre, its own label above and below). "view as": all 4 pills pass, takers their own label.
- 20-page pass after (`after-pass.json`): **no kit control fails, and no control that passed on the base fails now** (the spec's compare printed `worse: []`). The seven remaining fails are settings'.
- Page heights (content pane scrollHeight, base to after): Voice & Video 2898 to 2986 (+88: 6 sliders x 12, 2 pill groups x 8); Appearance 3127 to 3199 (+72); Accessibility 1442 to 1486 (+44); Activity Privacy 2557 to 2593 (+36); Privacy & Safety 3549 to 3581 (+32); Profiles 4208 to 4232 (+24); Text & Images 1837 to 1853 (+16); Language 815 to 823 (+8); every other page 0. No control grows more than 24 px. No sideways scroll (scrollWidth = clientWidth = 375 on every page).
- A slider still drags (a touch tap at 80% of the first Voice & Video range moved it 100 to 165); a pill selects by tap ("view as": the second pill is checked after the tap).
- 1280 Editorial, Voice & Video: the boxes and groups of the 10 controls equal to the base, 0 px (pane height 2223 both).
- Outside Settings: Library home at 375 (Trending's Friends / People you know / Global): passes, each its own label, 4 px lower, nothing cut (shot).

## Users of the two controls
`<SegmentedControl` 36 uses in 19 files, `<Slider` 24 uses in 12 files (`git grep -n`, tests left out): Library (Home, Browse, Rankings, Pages, SeriesSocial, the two readers), the Lounge (device popover, soundboard, tiles), Settings, Admin, `profilepage/ProfileEditor.tsx` (the signed-in editor) and `voice/jukebox`. **No public page uses either** (nothing in `public/`, `components/entrance*` or the first-paint code names them); the sheet is the kit package's.

## The spec fix
`e2e/tests/p92-kit-shots.spec.ts`, case c only: `page.keyboard.type('p92-r10')` and a 300 ms wait before the option is clicked. Ran case c once in the round (`SR_P92_KIT_ON=1 SR_P92_KIT_ONLY=c`): 1 passed (15.9 s), the tenth chip added, Backspace removed it (10 to 9).

## Shots (`visual-sweep-shots/p93-kit`, 10 files; the 7 below opened by me after the release)
Opened: `base-voice-optimise-375-lounge-light`, `after-voice-optimise-375-lounge-light`, `base-view-as-375-lounge-light`, `after-view-as-375-lounge-light`, `after-voice-optimise-375-lounge-dark`, `after-voice-1280-editorial`, `after-library-375-lounge-light`. Not opened (the spec also writes them, the brief did not list them): `base-voice-optimise-375-lounge-dark`, `base-voice-1280-editorial`, `base-library-375-lounge-light`. `data-theme` read back before each (lounge-light, lounge-dark, lounge-editorial; the Library page: sideroom). What I saw after: the "Optimise for" pills sit 4 px lower with the switch clear below; "view as" has 4 px more above the preview; nothing cut or overlapping; sliders at 1280 as before.
The commander's command (from `e2e/`, on a stack with the sweep's world, API 8711 / Vite 5711 / metrics 9411, `SR_ADMIN_REQUIRE_REAUTH=0`): `SR_E2E_BASE_URL=http://localhost:5711 SR_P93_KIT_ON=1 SR_P93_KIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-kit npx playwright test tests/p93-kit-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`SR_P93_KIT_BASE=1` names the files `base-*`). After the merge with settings, settings' 20-page pass must show 0 failures.

## Checks
prettier on the CSS and both specs, e2e `tsc`, `node scripts/check-css-tokens.mjs` ("73 file(s) clean"), `node scripts/check-doc-links.mjs` ("doc links ok"); my spec with `--project=chromium --reporter=line`: 1 passed twice (55.7 s base, 53.9 s after). No vitest (CSS only). No Rust, cargo, migration or `openapi.json`.

## Proposed row text (SEC-AUD-416)
SEC-AUD-416, closed (wave 93, D-922): the kit's segmented pill and range input had their 44 px phone target (D-856) reaching outside the control's own box (3 to 4 px for a pill group, 12 px at the bottom of a range), where a later control took the tap: Voice & Video's "Optimise for" Motion pill (taker: the next switch's input), one slider (taker: the next slider) and Profiles' "view as" second row (taker: the preview block). Fixed in `Slider.module.css`: on a phone the range's bottom margin is 0 and the segmented control has a 4 px block margin; nothing changes above 720 px. Cost: 12 px per slider, 8 px per pill group. SEC-AUD-414's "view as" control closes with it.
Changelog sentence: On a phone the kit's segmented control and slider keep their 44 px touch target inside their own box, so a tap near them no longer changes the neighbouring control (Voice & Video's "Optimise for" and one slider, Profiles' "view as"); each grows 8 to 12 px.
Docs outside my files: D-856's decisions-log row gets "the target inside the control's box: D-922" (the plan has it).
Faults found, not mine: none new. Owner questions: none.

## The web build and cleanup
Build: lock taken 13:40 (free; all seven other handoffs were already in), `npx pnpm@10.12.1 --filter @sideroom/web build` exit 0, 13:40 to 13:41, lock released 13:41. It ran on my branch (kit CSS only), not on the merged tree.
- `node scripts/check-bundle-budget.mjs`: all ok. First paint `app/app-BFsznGzQ.js`: **218 351 B gzip, room 37 649 B** (needs 8 KB). Before (wave 92): 218 359 B and 37 641 B, so -8 B (a CSS-only change; the difference comes from the base, not from me).
- `node scripts/check-entrance-isolation.mjs`: every public surface is clean. `node scripts/check-staff-delivery.mjs`: ok. Nothing the build wrote was committed.
- Cleanup: `sr_p93_kit_ui` dropped, `p93-kit-bin` deleted, stack stopped by PID tree (ports free), no stash touched, nothing running.
