# fix-p106-touchb (census, wave 106): the 44 px census, Library, Cinema, Live TV, Study, Arcade

The product is `C:/Users/braxt/Desktop/SIDEROOM_wt/p106-touch-b-report.md` (the table of every fail, the groups by sheet rule, the control-taker fails, what passed, not covered). Figures: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-touchb/figures-part1..4.json` (one row per control) and the shots there.

## Clock times
Started 16:30. Spec first committed 16:34. Jobs queued 16:33 to 17:20 (the runner was stopped from about 16:42 to 16:55 for mount's cargo round). Last job ended 17:31. Report written 17:33.

## Done (commits on fix/p106-touchb)
Only `e2e/tests/p106-touchb-shots.spec.ts` (new, 5 small commits) and this handoff. No app file changed, no build job, no `git stash`.

## Not done
- The player page's control bar and Live TV program rows could not be shown by the seeded world (no media, no schedule): listed under "not covered" in the report.
- Part 1's first run (job 0092) failed because the Library section bar sits at the top edge (points off the page = `not-reached`); the spec now measures the section chrome at `reach: 'box'` and says so. Rerun 0125 passed.

## FINDERS (rule 13)
None. No control was changed, moved or replaced.

## JOBS (all `shoot base`, job name touchb)
| id | part | waited | ran | rc |
|---|---|---|---|---|
| 0092 | 1 Library | 1193 s | 69 s | 1 (the chrome rows were not-reached; figures written) |
| 0104 | 2 Cinema | 168 s | 50 s | 0 |
| 0113 | 3 Live TV, Study, Arcade | 590 s | 47 s | 0 |
| 0124 | 2 Cinema again (party tabs) | 503 s | 56 s | 0 |
| 0125 | 1 Library again (chrome at box reach) | 551 s | 60 s | 0 |
| 0126 | 4 section themes on lounge-light | 700 s | 34 s | 0 |
| 0136 | 2 Cinema (tab roots fall back to body: People and Queue tabs) | 644 s | 60 s | 0 |

## Figures against the task's lists
A census has no change lists. Result: 82 failing controls (80 in the shipped themes, 2 more in the Library's empty state), grouped into 17 rule groups A to Q in the report; all takers are containers except two (the genre chips Slice of life and Sci-fi lose a point to the "Add a genre" label). The reader's bars pass (wave 105 cure holds).

## Shots (opened after the jobs ended)
36 shots in `visual-sweep-shots/p106-touchb/`, each opened: lib-browse (kind chips, sort, one series card), lib-browse-filters (drawer: genre chips, tags, status select, footer buttons), lib-series (cover, tag, rating, Everyone switch), lib-series-reviews-tab (action buttons, tabs, Reviews empty), lib-reader (top bar with 5 controls, bottom bar with slider, bars shown), lib-me (Sort by select, shelf chips, empty state), lib-lists, lib-lists-new-list (form with Create list and Cancel), lib-list, lib-list-edit-list (Save changes form), lib-rankings (segmented switches, kind select, empty), lib-review (series page top; the review form is below the fold and was measured), cin-browse (chips, rooms dropdown, Filters, one poster with two round buttons), cin-browse-filters (fields and selects, Reset and Apply), cin-title (action buttons, tabs), cin-title-comments-tab (comment form), cin-title-watch-together and cin-title-share (dialogs), cin-player (the Playback stopped state, top bar only), cin-party and its People and Queue tabs (top bar, video stage "not ready", control bar in 3 lines, tabs, panels), tv-guide (Off the air card, Edit lineup), tv-watch (could not tune the channel), study (rail pills, breadcrumb, type chips, cards; lounge-light), study-edit-dialog, study-file-viewer (Could not open that file), arcade-lobby (Land.io card, Play), arcade-lobby-play (start dialog), arcade-emulator (No games yet), and the four light twins (lib-browse-light shows the empty library with two links; cin-browse-light, tv-guide-light, arcade-lobby-light, plus the two filters drawers). No crowding that was not already in the base: this is the base.

## Who imports my files
Nothing: an e2e spec and a report.

## Checks
- `npx prettier --check e2e/tests/p106-touchb-shots.spec.ts`: pass (17:33, exit 0).
- e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: pass, no output (17:01, after the last edit to the spec that matters; the last edit after it, a one-line tab-root fallback, was also run: pass).
- eslint: the web package's eslint ignores `e2e/` ("File ignored because outside of base path"); e2e has no eslint of its own: not run.

## Closing text for my rows and proposed new rows
Row (the 44 px census, Library, Cinema, Live TV, Study, Arcade): measured 17:31; 82 fails in 17 rule groups in `p106-touch-b-report.md`. Proposed rows, one per sheet, with takers all containers: FilterChips `.chip` (19 fails, packages/ui) and `.dropdown`; `tv.css` chips, nav, fav, tune; `watchparty.css` `.wp-btn` and `.wp-vol`; `library.css` genre chips, `.lib-btn`, `.lib-icon-btn`, tag links, `.lib-linklike`; `cinema.css` `.cin-qa` and `.cin-link-btn`; `Drawer.module.css` `.close`; `study.css` rail and crumbs; `arcade.css` `.arc-cta`; `emu.css` `.emu-back`, `.emu-search`. Also a probe limit: a control whose centre is within 21 px of the page top (the section bars, 20 to 39 px tall) cannot be tested at touch reach (`not-reached`); the helper is not mine to change.

## One changelog sentence
Added a 44 px touch-target census spec for the Library, Cinema, Live TV, the Study and the Arcade, and its report (no app change).

## Owner questions (for Fable; Discord-like default listed)
1. The kit Button gets a 44 px pseudo-element tap area under 720 px while keeping its drawn size. Should the cure for the sheet classes above be the same (a drawn-size-preserving tap area, the Discord-like default) rather than growing the boxes? The grouping table says what growing would move.
2. The section bars' links are 20 to 39 px tall at the very top edge: same cure (tap area) or taller bars? Default: tap area.

## PROCESSES
strays: none found (17:34, `strays.sh p106-touchb`). Jobs: none of mine queued or running (`status` at 17:34).
