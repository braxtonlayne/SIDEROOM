# Handoff: fix/p107-sectionshots (wave 107, shots builder of pod sections)

## Clock
Started 18:38. Spec committed 18:44. Base round 18:45-18:56 (0174-0176). Merged library 75e27700 and cinema b023bade 18:44, build 0187, first after round 0196-0198 finished 19:28 (runner queue about 20 min deep). FAULTS 19:34 (commit 8a627224). Cures merged 19:31 (library d9b1e1b7, cinema 46a926ad), build 0216, cure round 0224 and 0225 finished 19:50. Shots opened 19:51. Nothing of mine queued or running.

## Done
- `e2e/tests/p107-section-shots.spec.ts` (from the p106 census spec): parts 1 Library, 2 Cinema, 3 Live TV + Study + Arcade + emulator. Per page at 375 touch: openCollapsed + hitTestAll (default reach), boxes of the named controls and of their rows; at 1024 x 768: boxes (x, y, w, h) and the box-reach hit test per named control; fields (.lib-search-input, .emu-search) at 1024 in the shipped theme, lounge-light and lounge-dark (rest, hover, focus read against the kit values, ratios, placeholder when the field has one); scrollWidth. After compares with figures-base-part<N>.json (one file per part, not one figures-base.json).
- Commits: the spec, the spec fixes, 8a627224 FAULTS, the merges of the two code branches, this handoff.

## FAULTS (first after round) and how each ended
Cause: a `::after` tap area is positioned against the padding box, so a 1 px border costs 1 px of reach; the point 21 px below the centre fell in the gap. Affected (taker always a container, never a control): `.lib-genre-chip` (10 chips), `.lib-frame .lib-btn` (5 links), `.tv-chip` (8), `.arc-cta` (1). Cure (b), one px more each, pseudo-element only: landed by the commander (library 5fc41a41 and d9b1e1b7, cinema 46a926ad). **After the cures: 0 named controls fail at 375 and 0 problem lines in parts 1 and 3.** The old figures in fix-p107-library.md and fix-p107-cinema.md (-3px, -2px, -5px, -4px) are superseded by -4px, -3px, -6px, -5px.
Spec faults of mine, found and fixed: the field reader read the top border of an underline field (library search); the placeholder check ran on `.emu-search`, which has no placeholder attribute; the allowances for the poster actions overlay row (+14) and the 44 px volume hit area (28 px of negative-margin padding) were missing.

## Fails at 375 per builder (base to after; hit-test rows over the census roots and overlays)
- library, part 1: 34 to 16. The 16 left are not this pod: FilterChips `.chip` Browse 7 and My library 5, Drawer `.close` 1 (kitbits), and the accepted `.lib-chip-list a` tag 3. Named controls failing: 0.
- library, part 3 (Study, Arcade, emulator) with cinema Live TV: 27 to 6; the 6 are the Study FilterChips (kitbits). Named failing: 0.
- cinema, part 2 (round-1 figures; the only later change is tv.css, covered by part 3): 46 to 4: Cinema Browse Drama chip, Room selection, its chevron, Drawer close (kitbits). Named failing: 0.

## Table against the task lists (375, base to after; at 1024 nothing moved, no problem line)
- Drawn sizes unchanged: `.lib-genre-chip`, `.lib-btn`, `.lib-icon-btn` 36, `.lib-linklike`, `.arc-cta`, `.emu-back`, `.tv-chip`, `.tv-nav-btn`, `.tv-fav`, `.wp-btn` 36, `.cpl-btn`.
- Grew, as allowed: Library top bar 61 to 65 (search field 40 to 44); emulator search 40 to 44 (its row 59 to 63); Study rail pills 25 to 44 and the rail 50 to 69; crumb buttons 23 to 44 and the crumb row +21; watch party bar 141 to 149 (+8); poster quick actions 30 to 44 square (overlay row 32 to 46); `.wp-vol` box 16 to 44 (padding 14, margin -14: the bar grew only the +8).
- Not moved: the Browse toolbar (`.lib-browse-tools`) height, the reader bars (lib-reader 8 pass, 0 fail, 1 skipped, as base), Live TV rows, `.arc-actions`, `.lib-actions`.
- **Probes the commander asked for:** (1) `.wp-vol` alone on a wrapped line: not reproducible in this world (at 375 the slider shares line two with the reactions and Mute; line three holds the link and grid buttons); no neighbour failed with the slider as taker. (2) The spoiler button and Search "open all" links (`.cin-link-btn`, uncured): the spoiler checkbox on the title page passes; `.cin-link-btn` is not shown on the page state (no comment in the seeded world); Search "open all" is not in the spec. (3) `.lib-btn` on reader surfaces (epub/pdf bars, end card): not shown in this world, so NOT hit-tested. (4) `.lib-topbar` grows 4 px at 375 on Browse (61 to 65) and no other box there grew; the other Library pages' top bars share the rule but were not each measured.

## FIELDS (1024, keyboard focus, from the figures)
- `.lib-search-input` (underline field; the line is the bottom border): rest equals color-mix 65 exactly, hover equals --ui-text-faint, focus-visible outline solid 2px --ui-focus offset 1px, placeholder faint, opacity 1. Rest line ratio against the ground: sideroom 3.51, lounge-light 3.48, lounge-dark 3.13; ring 11.55, 5.49, 4.82; placeholder 5.68, 5.76, 4.81. Size unchanged at 1024 (623 x 40).
- `.emu-search`: rest and hover as above; ring solid 2px offset 1px; rest ratio against the outside ground 3.69, 3.48, 3.13 and against the field 4.00, 3.13, 3.42 (lounge-editorial, lounge-light, lounge-dark); ring 9.74, 5.49, 4.82. No placeholder attribute in the page, so no placeholder rule is visible. Size unchanged at 1024 (320 x 40).

## Shots (opened after the jobs ended; base twins carry the base- prefix) in visual-sweep-shots/p107-sectionshots
- lib-browse-375: top bar with the search underline, chips, sort and Filters, same layout as base, bar 4 px taller. lib-browse-filters-375: genre chips drawn as base (38 px), the label under the last line unchanged. lib-series-375: tag, rating and actions as base, no overlap. lib-browse-1024-light: search with the 2 px blue ring (keyboard focus), nothing moved against the figures of base.
- study-375: rail pills are 44 px tall, breadcrumb row taller, search and chips as base. tv-guide-375: channel card as base (the chips are below the shot; boxes in the figures). arcade-lobby-375: Play buttons and cards as base. cin-browse-375: poster quick actions are 44 px circles inside the poster, covering nothing else; base-cin-browse-375 opened beside it. cin-party-375: the bar is 8 px taller, tabs and chat pushed down 8 px (base twin opened earlier). cin-player-375: error state, Back button as base. cin-title-comments-tab-375: form as base (no comment in the seeded world).
- Not opened: the other overlay shots, the emulator 375 and 1024 shots, lib-list, lib-me, lib-review, the lib-browse 1024 shipped shot. The emulator dark 1024 shot is not written (the focus shot is taken in the shipped and light themes only).

## Not this pod's (same in base and after)
FilterChips `.chip` (Browse 7, My library 5, Study 6, Cinema Browse Drama), Cinema Browse Room selection and chevron, Drawer `.close` (Library and Cinema filters): kitbits (pod staff). Accepted by library: `.lib-chip-list a` (3 rows).

## Not covered (the world)
Epub and pdf reader surfaces; the empty-library links (a series exists); Live TV agenda rows and `.tv-agenda-tune` (the channel is off the air); a comment's action links on the page state; the player control bar (no video). Part 2 was not rerun after the cures; its three problem lines in the round-1 file (poster overlay row +14, volume box 16 to 44) are by-design changes that the final spec allows.

## JOBS (id, class, waited = started minus submitted, ran, rc)
0174 shoot base p1 (waited 500 s, ran 82 s) rc 0; 0175 base p2 (596 s, 89 s) rc 0; 0176 base p3 (690 s, 72 s) rc 0; 0187 build (121 s, 28 s) rc 0; 0196 after p1 (1565 s, 75 s) rc 1; 0197 after p2 (1704 s, 76 s) rc 1; 0198 after p3 (1787 s, 67 s) rc 1 (problem lines above); 0216 build (681 s, 17 s) rc 0; 0224 after p1 (231 s, 69 s) rc 0; 0225 after p3 (366 s, 66 s) rc 0. 0181 build cancelled (a commit made it stale).
Commander's command, whole: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p107-integ/apps/web/dist --env SR_P107_SECTION_ON=1 --env SR_P107_SECTION_PHASE=after --env SR_P107_SECTION_PART=<1|2|3> --env SR_P107_SECTION_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-sectionshots -- tests/p107-section-shots.spec.ts

## FINDERS
None: I changed no app file; the spec is new and nothing reads it.

## Who imports my files
`e2e/tests/p107-section-shots.spec.ts`: Playwright only.

## Checks (my last commit)
prettier --check on the spec: pass. e2e tsc (`npx tsc --noEmit -p tsconfig.json` in e2e): exit 0, 19:50. Web eslint: not run (`pnpm` is not on PATH in this shell).

## Closing text and proposed rows
Proposed note for D-1022: a `::after` tap area on a bordered control needs an inset of the reach plus the border width, because absolute insets measure from the padding box.

## Changelog sentence
None of mine; the pod's sentences are in the two code handoffs.

## Owner questions
None.

## PROCESSES
strays: none found
