# Wave 101, library: D-958, the Library's selects and the kit's `NativeSelect`

2026-11-18. Clock (machine clock): started 06:19; code and spec by 06:23, commit a018e3ec; lock rounds 07:06 to 07:11 (spec error: review label, series scope, figures lost on a throw), 07:14 to 07:19 (full run, which showed the layout problem below), 08:24 to 08:28 (final). Stack ports 8786 / 5786 / 9486, database `sr_p101_library_ui`. No Rust, no CSS, no kit file, no string.

## Done
- `apps/web/src/library/Browse.tsx`: the three Filters-drawer selects (Status, Rating, Age-restricted titles) are `<NativeSelect>` with the same value, onChange and children. Their label drops `lib-select` (keeps `lib-select-block`): `.lib-select select` in `library.css` (36 px, `padding: 0 10px`, `--sr-*` ground and border) would override the kit's padding (the chevron would sit on the text) and height. Nothing finds these three by class; the only other reader of `.lib-select select` is the old `p78-library-shots.spec.ts:300`, which reads the toolbar sort select (still raw, still `lib-select`).
- `e2e/tests/p101-library-shots.spec.ts` (new) and this handoff.

## The 7 selects
| select | file | result | base -> after (1280 / 375 touch) |
|---|---|---|---|
| Sort by (toolbar) | Browse | **left raw** | 152 x 36 (Light 158 x 36). Toolbar 36 px tall at 1280. What-if with the kit min height set on it: toolbar 36 -> 40 at 1280 and 74 -> 80 at 375 (bar 88 -> 92, 174 -> 180). Its neighbours are a 36 px icon button, a 30 px segmented control and a 28 px button. Fable's call (question 1). |
| Status | Browse drawer | moved | 380 x 36 -> 380 x 40 / 334 x 36 -> 334 x 44; x and width equal |
| Rating | Browse drawer | moved | same; moved down by exactly the Status difference (4 / 8 px), as allowed |
| Age-restricted titles | Browse drawer | moved | same |
| Your rating | SeriesSocial | **left raw** | In `label.lib-rate` (inline flex) the kit's wrapper span is a flex item 100 % wide, so the label text shrank to two lines ("Your / rating") and the select grew 102 -> 125 wide. With the label as `lib-select-block` (text above) it looked right, but the neighbours moved 14 px (1280) and 24 px (375) against 4 / 8 allowed. Put back raw per the rule. |
| Rating with this review | SeriesSocial | **left raw** | Same two results; the select grew to the form width (102 -> 640 at 1280) and the controls below moved 24 to 28 px (allowed 4). |
| Rate this chapter | SeriesSocial | **left raw** | Reached in round 3 (reader end card, 102 x 36 at 1280 and 375). Not moved: same label layout as the two above. |

`SeriesSocial.tsx` is unchanged from the base.

Drawer hit test (`openCollapsed`, then `hitTestAll` over the dialog): at 1280 0 pass / 21 fail / 2 skipped in both trees (the helper's 21 px points are made for 44 px phone controls; the takers are neighbouring controls, the same in both). At 375 touch: base 7 pass / 14 fail, after 10 pass / 11 fail (the three selects now pass at 44 px); counts equal. No sideways scroll in any of the four configs (scrollWidth equals clientWidth at 1280 and 375). Round 3 reported no problem for Browse and the drawer; the 28 problems it listed are the series, review and chapter views of the stacked variant that I then put back to raw (they are the evidence for the three rows above). The series and review figures in `figures-*.json` and `compare.json` in the shots folder are of that variant, not of the final tree.

Keyboard (1280, shipped theme): the drawer Status select takes `:focus-visible` after a key, ring `2px solid rgb(217, 201, 164)`, offset 1px; ArrowDown changed "" to "ongoing", ArrowUp put it back, nothing applied. "Your rating" on the series page (raw, as the base): ArrowDown rated 1 (server agreed), ArrowUp un-rated; own series, deleted afterwards.

Tokens: the kit field's ground, border and ring show right in both themes in the shots (shipped `sideroom`: dark raised ground and gold ring; `lounge-light`: light ground and dark ring); `data-theme` was read back from `.lib-root` before each shot. The probed computed values are in `figures-after.json` (`extra.tokens`) in the shots folder; I did not compare them numerically with the search field.

## Who imports
`Browse.tsx` is loaded by `library/routes.tsx` with `lazyNamed(() => import('./Browse'))`; `SeriesSocial.tsx` (unchanged) by `SeriesPage.tsx`, `Rankings.tsx` and the reader, all in the Library's lazy routes. Neither is first paint or a public page. No web build (lounge's). The kit sheet now has a second adopter in the Library chunk: I expect it in a shared CSS chunk or duplicated into the Library chunk; lounge's build shows which.

## Shots (`visual-sweep-shots/p101-library`, 29 PNG)
Opened: browse 1280 sideroom, 1280 lounge-light, 375 sideroom; filters 375 sideroom (after and base) and 375 lounge-light; focus 1280; series-social 1280 sideroom, 375 sideroom (after and base), 375 lounge-light; review 1280 and 375. The other files are base twins of these and the 375 lounge-light and 1280 filters pairs, not all opened. The series and review shots are of the stacked variant that was put back; the base twins show the final tree. The Browse shots equal their bases except the drawer.

## Checks (after the last edit, 08:31, all exit 0)
web `tsc --noEmit` 0; eslint on both TSX 0; prettier --check on the three files clean; e2e `tsc --noEmit` 0; vitest `library.test.tsx`, `social.test.tsx`, `reader.test.tsx` 49 passed (unedited); `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok. The spec's header was edited after round 3 (comment only).

## Commander's command (from `e2e/`, stack with the sweep's world: API 8786, Vite 5786, metrics 9486)
```
SR_E2E_BASE_URL=http://localhost:5786 SR_P101_LIBRARY_ON=1 \
SR_P101_LIBRARY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-library \
npx playwright test tests/p101-library-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
Delete `figures-base.json` first. The spec makes a published series `p101-lib-<digits>` and deletes it in a `finally`. Its `staff appoint` prints an error (the owner is already staff); harmless. After the merge the series selects are raw in both phases, so the series and review views only record.

## Changelog sentence
"The Library's Filters panel drop-downs (Status, Rating, Age-restricted titles) have the kit's field look and a 44 px height on touch."

## Owner questions
1. Should the Library's toolbar sort select take the kit field's height (40 px, 44 on touch)? It would make the toolbar 4 px (6 on a phone) taller than its 36 px neighbours. Left raw.
2. Series social's three rating selects: accept the kit field with its label above (it looked right, but the rating row grows by 24 px and the review rating spans the whole form), or keep them raw? Default taken: raw.

## Proposed rows
No inventory row. A note for D-958: a `NativeSelect` inside an inline-flex label wraps the label (the wrapper span is 100 % wide); the kit part would need `inline-size: auto` there. Not changed (the kit is fenced).

## Cleanup
The round script's exit trap stopped the server and Vite by PID tree and dropped the database; ports 8786 / 5786 / 9486 free. `p101-library-bin` deleted at the end. Scratch (`p101-library-scratch`) is untracked, outside the repo. No stash touched. `heavy.sh status` outputs: see the final report.
