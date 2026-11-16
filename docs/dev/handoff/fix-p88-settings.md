# Wave 88 settings: SEC-AUD-381 (D-890 built)

T = 16:55. Ready 16:56, lock taken 16:57, released 16:59, finished about 17:02.

## Done
- `settings/user/sections/ActivityHistory.tsx`, `CinemaDefaultsCard` only: the three labels (`us-sub-lang-label`, `us-audio-lang-label`, `us-quality-cap-label`) are `us-seg-label` instead of `us-muted`. Each stays a `<label>` with its id; the Selects keep `aria-labelledby` pointing at it. No CSS, no string, no new import, Library card untouched.
- Vitest `settings/user/__tests__/p15Defaults.test.tsx`: no existing case broke; one case added (labels are `<label>.us-seg-label`, not aria-hidden, each combobox named by its label). 5/5 pass (single file).
- Commit d6cc60e3 (code, test, spec); this handoff in a second commit.

## Base (I ran it: the spec puts `us-muted` back on the three labels in the page, so it is the base exactly)
Cinema labels before: muted colour, weight 400, 14 px, margin-top 4 px, margin-bottom 0, gap to control 14-15 px.
Library labels: text colour, weight 500, 14 px, margin-bottom 6 px, gap 6 px. So they did differ.

| theme | Cinema before colour/weight | Library & Cinema after colour/weight |
|---|---|---|
| Editorial 1280 | rgb(179,172,154) / 400 | rgb(239,230,210) / 500 |
| Lounge Light 375 | rgb(92,94,102) / 400 | rgb(49,51,56) / 500 |
| Lounge Dark 375 | rgb(164,170,179) / 400 | rgb(219,222,225) / 500 |

After: colour, weight (500) and size (14 px) equal the Library's in all three. All three controls' accessible names equal their label text (read in the spec, all true).

## Gap, left as is (no CSS this wave)
Label-to-control gap: Library 6 px; Cinema now 20-21 px (before 14-15). Cause: the Cinema labels are direct children of `.us-stack` (flex, gap 14 px), so the `us-seg-label` 6 px bottom margin adds to the gap, while the Library labels sit in their own `<div>`. Effect: each Cinema label is now 14 px from the control above and 20 px from its own, so grouping reads slightly wrong. Fix (not done): wrap label+Select in a `<div>` like the Library card, or a CSS rule in `user-settings.css`.

## What moved
Cinema card height +6 px at 1280 (426.6 to 432.6) and at 375 (502.6 to 508.6) (three labels, -4 top +6 bottom margin each = +2 each). Library card and everything below moved down by the same 6 px; nothing else. No horizontal scroll (doc width diff 0 in all).

## Imports
`ActivityHistory.tsx` is imported only by `settings/user/sections/Activity.tsx` and the vitest file (`git grep`); Settings sections are lazy; I added no import.

## Checks
prettier and eslint on my files clean; web tsc clean; e2e tsc clean; vitest single file 5/5; spec passed on the stack (sweep world seeded; the seed step printed one unrelated `FAIL suspended app` 2FA message, the world was usable).

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p88-settings`: 4 PNGs (before 1280 Editorial; after 1280 Editorial, 375 Lounge Light, 375 Lounge Dark) plus `run-measure.json`. All 4 opened and looked at after releasing the lock. Viewport made tall for the shot only so both cards are whole.

## Commander's command (from `e2e/`, stack with sweep world, Vite 5663, API 8663)
`SR_P88_SETTINGS_ON=1 SR_P88_SETTINGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p88-settings SR_E2E_BASE_URL=http://localhost:5663 npx playwright test tests/p88-settings-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`

## Proposed inventory row
SEC-AUD-381: Cinema defaults card labels used `us-muted` (muted, 400) while the Library card and Appearance use `us-seg-label` (text colour, 500); fixed in `ActivityHistory.tsx` (wave 88, D-890). Residual: label-to-control gap 20 px vs 6 px in the Library card.

## Changelog sentence
Settings, Activity & privacy: the Cinema defaults card's three labels now match the Library defaults card's label style.

## Owner questions
Wrap each Cinema label and its select in a `<div>` (as the Library card) so the gap is 6 px? (suggest yes, next wave.)

## Cleanup
Scratch DB `sr_p88_settings_ui` dropped, `p88-settings-bin` deleted (not `p80-base-bin`), ports 8663/5663/9363 checked free, lock released. No stash touched.
