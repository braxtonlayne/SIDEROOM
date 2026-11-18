# Wave 102, library: D-966, the Library's sort select and rating selects (SEC-AUD-453)

2026-11-18. Branch `fix/p102-library`. Ports 8785 / 5785 / 9485, database `sr_p102_library_ui`. No Rust, no CSS, no kit file, no string, no web build.

## Done
- `apps/web/src/library/Browse.tsx`: the toolbar sort select is a `<NativeSelect>` (same value, onChange, children). Its label drops `lib-select` (`.lib-select select` in `library.css` would override the kit's height and padding); the label text stays the sr-only span, so the select keeps its name "Sort by".
- `apps/web/src/library/SeriesSocial.tsx`: "Your rating" (RatingBlock) and "Rate this chapter" (ChapterRate) are `<NativeSelect>` in a `label.lib-select-block` (label above, existing class, no sheet).
- `e2e/tests/p102-library-shots.spec.ts` (new) and this handoff.

## The four selects (base -> after; 1280 / 1024 / 375 touch; shipped theme `sideroom`)
| select | result | figures |
|---|---|---|
| Sort by (toolbar) | **moved** | Select 152 x 36 -> 176 x 40 (375: 176 x 44; light 158 x 36 -> 183 x 40). Bar 88 -> 92 at 1280 and 1024, 174 -> 180 at 375: **+4 / +6, exactly the allowance**. First grid row moved by exactly that (133 -> 137, 219 -> 225). Centre y of every control on each line: spread 0 px (at 375 the bar wraps as in the base: two lines, each spread 0). Inline size 176 >= 152. |
| Your rating | **moved, 4 px over the allowance** | Row (`.lib-rating`) 36 -> 64 at 1280 (+28, allowed 24), 78 -> 106 at 1024 (+28), 127 -> 159 at 375 (+32, allowed 28). Select 102 x 36 -> 110 x 40 (375: 110 x 44). Label on one line, is the select's name, not the form's width. See owner question 1. |
| Rate this chapter (reader end card) | **moved** | Select 102 x 36 -> 110 x 40 (375: 110 x 44). End card height 576 -> 576 (growth 0: the card is centred in the reader, so "Back to the series" and "comments" moved up 14 px at 1280, 16 at 375, the text and select sit below them). |
| Rating with this review | **put back to raw** | With the kit field the select is 640 px wide at 1280 and 1024 (base 102) and 343 at 375, and the form grew +28 / +32. No existing class in `library.css` keeps a `lib-select-block` label to its content width (the only `max-inline-size` classes are `.lib-related-item`, `.lib-rating-dist ul`, `.lib-review-form`), so per the task it is raw at every width: 102 x 36, unchanged from the base. `<label className="lib-select lib-rate">` and `<select>` as the base. Owner's call next wave (a `justify-self: start` or `inline-size: fit-content` rule on the label in `library.css`). |

Hit tests (`openCollapsed`, `hitTestAll`, equal or better in every view): bar 1280 and 1024 0 pass / 12 fail in both trees; bar 375 base 3/9, after 4/8; series 1280 and 1024 0/4 both, series 375 base 3/1, after 4/0; review unchanged (1/3, 375 3/1); end card 375 base 0/3, after 1/2. Takers are neighbouring controls, the same in both trees (the helper's 21 px points suit 44 px controls). No view scrolls sideways (scrollWidth = clientWidth at 1280, 1024, 375). Keyboard (1280): sort select takes `:focus-visible`, ArrowDown changed "updated" to "newest", URL became `?sort=newest`, ArrowUp put it back; "Your rating" ArrowDown rated 1 (server agreed), ArrowUp un-rated (own series, deleted at the end).
Tokens: sort select ground `rgb(28, 29, 24)`, border `rgb(42, 43, 36)` in `sideroom`; `rgb(242, 243, 245)` / `rgb(201, 204, 209)` in `lounge-light` (`data-theme` read back from `.lib-root` before each shot).

Spec result of round 3: all checks pass. Looked at (opened): toolbar 1280 and 375 (and base twins), light toolbar 1280, series rating 1280 and 375 (and base twins), reader end card 375 (and base twin), review form 1280 (raw; the base twin exists). Nothing touches or wraps. In the toolbar the sort select's text is 16 px against the base's 14 px (the kit's `font: inherit`), so the select looks bigger than its neighbours; this is the kit's look, listed for the owner.

## Who imports
`Browse.tsx` is loaded by `library/routes.tsx` (`lazyNamed`); `SeriesSocial.tsx` by `SeriesPage.tsx`, `Rankings.tsx` and the reader, all Library lazy routes. Neither is first paint nor a public page. No build (lounge's). Unit tests that render them, unedited: `library.test.tsx` 18, `social.test.tsx` and `reader.test.tsx` 31, all passed. `git grep` of `lib-select` / `lib-rate`: only `p78-library-shots.spec.ts:300` (`.lib-select select`, an old local spec, reads the toolbar sort select; it no longer matches it) and `p101-library-shots.spec.ts:454` (`.lib-rate select`, the series rating; no longer matches). Neither is run by CI.

## STACK-ROUND
- Round 1: 09:59 to 10:01 (slot shown: `library 09:59` with `arcadm 09:59`). Seed exit 0, no FAIL line. Base phase passed; after phase failed on spec checks only (label-name comparison too strict, review not yet put back, controls-above rule on the centred end card).
- Round 2: 10:06 to 10:08 (`library 10:06` with `settings 10:06`). Seed 0, base 0, after: one problem, a spec check that the raw review select be 44 px at 375 (spec error, fixed).
- Round 3: 10:26 to 10:28 (`library 10:26`, the other slot free). Seed 0, base 0, **after 0: `problems: []`** (spec passes). The 2-3 min rounds fit the 8 min limit; a leftover Vite node process on 5785 was killed by PID again.
Base and after run in one round: the two files are put back to `git show HEAD` text from copies in `p102-library-scratch/base`, then the edited ones. One leftover Vite node process on 5785 after round 1 (killed by PID; the script's tree kill missed it); ports were free after rounds 2 and 3.

## TYPECHECK
`bash cli-agents/tc.sh run library bash -c "cd .../p102-library/apps/web && npx tsc --noEmit -p ."` at 10:08, exit 0 (after the last code edit; the later edit was the spec only).
Other checks after the last edit: prettier `--check` on both TSX clean, eslint on both TSX exit 0, e2e `tsc --noEmit` exit 0, vitest 49 passed, `i18n-audit --max 0` and `check-doc-links`: exit 0 (10:2x).

## Commander's command (from `e2e/`, stack with the sweep's world: API 8785, Vite 5785, metrics 9485)
```
SR_E2E_BASE_URL=http://localhost:5785 SR_P102_LIBRARY_ON=1 \
SR_P102_LIBRARY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-library \
npx playwright test tests/p102-library-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
No base after the merge: delete `figures-base.json` first. The spec makes a published series `p102-lib-<digits>` and deletes it in a `finally`; it creates no rating that survives. It reports (`REPORT ... OVER`) a row that grows past the allowance instead of failing on it.

## Text for SEC-AUD-453 (library part)
Library: the toolbar sort select, "Your rating" and the reader's "Rate this chapter" are kit `NativeSelect` fields (D-966; the ratings with their label above; toolbar +4 px / +6 px at 375, grid moved by the same). "Rating with this review" is left raw (the kit select would span the 640 px form).

## Changelog sentence
"The Library's toolbar sort drop-down, the series rating and the reader's chapter rating have the kit's field look and a 44 px height on touch (the ratings carry their label above)."

## Owner questions
1. The series rating row grows 28 px (32 at 375), 4 px over D-966's 24 (28): the kit field is 40 px, the 14 px label above it 20 px, the gap 6. Keep it (default taken), or put "Your rating" back to raw (three-line revert in `SeriesSocial.tsx`)?
2. The review form's "Rating with this review": add a rule in `library.css` (a label that shrinks to its content, e.g. `justify-self: start`) so it can move; until then raw.
3. The kit select's text is 16 px where the Library's raw select was 14 px; the toolbar select looks larger than its neighbours. A `font-size` on the select in `library.css` would fix it.

## Cleanup
Scratch database dropped by the round script; `p102-library-bin` deleted; scratch (`p102-library-scratch`) is untracked, outside the repo. No stash touched. `heavy.sh status`: see the final report (status read `free` after round 3, no hold in my name).
