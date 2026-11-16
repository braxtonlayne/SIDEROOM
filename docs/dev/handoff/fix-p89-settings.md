# Wave 89 settings: SEC-AUD-383 (D-896), each Cinema label wrapped with its select

T = 17:18 (commander). Clock: ready 17:16, lock taken 17:16, released 17:18, finished 17:20. Base: `integration/phase89` (wave 88 tree).

## Done
- Commit `6f9a83df`: in `CinemaDefaultsCard` (`ActivityHistory.tsx`) each `<label id>` and its `Select` sit in one bare `<div>` (three wrappers). Labels, ids, `aria-labelledby` unchanged. No class, string or CSS.
- Spec `e2e/tests/p89-settings-shots.spec.ts` (new). Base case = the three wrappers unwrapped in the page (served tree has the change).
- Not done: none. `p15Defaults.test.tsx` unchanged (5/5 pass, single file).

## Base ran (yes), figures (px; label to own select / label to control above; select width; card height)
| setting | base | after | Library pair (2nd/3rd) |
|---|---|---|---|
| 1280 Editorial | 21 / 14; 580; 432.6 | 7 / 14; 580; 390.6 | 6 / 14 |
| 375 Lounge Light | 20 / 14; 275; 508.6 | 6 / 14; 275; 466.6 | 6 / 14 |
| 375 Lounge Dark | 20 / 14; 275; 508.6 | 6 / 14; 275; 466.6 | 6 / 14 |
- Own-select distance now smaller than the distance above. At 375 it equals the Library's (6). At 1280 it is 7 vs 6: 1 px more, same 1 px offset the base had (21 vs 20); not a change of this task (label line-box rounding).
- Select widths unchanged; card height -42 in every setting; the Library card moved up exactly 42; no sideways scroll (0) at 375 and 1280; accessible names unchanged (all three equal their label text).
- Account theme read back before each shot: lounge-editorial, lounge-light, lounge-dark.

## Lazy chunk
`git grep -n ActivityHistory -- apps/web/src`: imported only by `settings/user/sections/Activity.tsx` and `settings/user/__tests__/p15Defaults.test.tsx`. Settings chunk only; no web build run.

## Checks
prettier, eslint (file), web tsc, e2e tsc, i18n-audit (TOTAL 0), check-doc-links: clean. Spec: 1 passed (chromium). Sweep seed printed the known `FAIL suspended app` (world usable).

## Shots (folder `visual-sweep-shots/p89-settings`, 4, all opened)
run-before-phone-light-lounge-light.png (base, Cinema and Library cards), run-after-desk-editorial-lounge-editorial.png, run-after-phone-light-lounge-light.png, run-after-phone-dark-lounge-dark.png. Plus run-measure.json.

## Commander's command (from `e2e/`, stack with the seeded world, Vite 5672, API 8672)
`SR_P89_SETTINGS_ON=1 SR_P89_SETTINGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p89-settings SR_E2E_BASE_URL=http://localhost:5672 npx playwright test tests/p89-settings-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (add `SR_P89_SETTINGS_BASE=1` for base only).

## Proposed inventory text
- SEC-AUD-383 (closed, D-896): "Settings, Activity: each Cinema defaults label and its select are wrapped in one `<div>` like the Library card's pairs; label to own select 20 px to 6 px at 375 (21 to 7 at 1280), above-gap 14 px unchanged; card 42 px shorter."
- SEC-AUD-381: drop the "Left:" clause (383 landed).

## Changelog sentence
In Settings, Activity & privacy, each Cinema default's label now sits next to its own selector instead of midway between two controls.

## Owner questions
None. Stashes: none touched. Cleanup: `sr_p89_settings_ui` dropped, `p89-settings-bin` deleted, ports 8672/5672/9372 free.
