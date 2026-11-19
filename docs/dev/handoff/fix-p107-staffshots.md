# fix/p107-staffshots (shots builder, pod staff)

## Clock
Start 18:37. Spec and base round done 18:52 (jobs 0169-0171). Merge of nav, kitbits, server 18:42. Build 0192 18:54. The queue wait for the after round was about 27 min; after round ended 19:23. Handoff 19:30.

## Done
- `e2e/tests/p107-staff-shots.spec.ts` (new, 3 parts): both phases; 375 touch hit test and 1024 box hit test of every control, boxes, field reads (rest, hover, keyboard focus, outline, placeholder, contrast.ts ratios) at 1024 Light and Dark, scrollWidth against clientWidth per shot. Writes `figures-<phase>-part<N>.json` (per part, not one `figures-<phase>.json`: three parts would overwrite each other); after compares with `figures-base-part<N>.json`.
- Merged fix/p107-nav 932a461f, fix/p107-kitbits 6c24fe17, fix/p107-server 5c7114c0 (clean octopus merge).

## Result (375 fails: base -> after)
| part | pages | base | after | PROBLEM lines |
|---|---|---|---|---|
| 1 | User settings, Friends, Library Browse and My library, Study, Cinema Browse | 67 | 22 | 0 |
| 2 | Server settings | 25 | 0 | 1 (not a fault, see FAULTS) |
| 3 | Admin | 38 | 0 | 0 |

Part 1's 22 remaining fails are all unnamed and in no pod task: Friends "Back to channels" and "New Group DM" (taker the section bar), Library "Sorted descending", the empty-state links ("Mount a bucket", "Ask a friend", "Browse the library"), the Library filters drawer's genre chips (`.lib-genre-chip`, library's rule), Study file rows and crumb (taker h2/main), Cinema card "Play" and "Add to My list" (taker the fallback art span).

By builder:
- nav: sections list fails 21 (User settings), 19 (Server), 34 (Admin) -> 0, plus "Search settings". Items 44 px, pitch 46. The sidebar items at 1024 equal the base (no MOVED at 1024 line). Search 44 px at 375.
- kitbits: Notice Dismiss (Friends), chips in Library Browse (7), My library (5), Study (6), Cinema Browse (Drama, Room selection and its chevron), Drawer Close of both Filters drawers and of Library catalog "New series": all fixed. Every drawn box at 375 and 1024 equals the base (no MOVED line on those pages). Chip rows are 12 px apart (y 170-206 and 218-254), the dropdown row 16 px under the chips: nowhere closer than 8 px. No tap area took another control's point (no NEW FAIL line).
- server: Default permissions, role chip (Channels > Permissions), Watch Parties and Library links, Members bulk bar buttons, Bans chips, Live TV daypart select: all pass. Moves the task allows: Bans 9 boxes shift at most 8 px (row gap), Live TV new channel form 3 boxes shift 8 px (daypart gap), the role chip line 12 px shift and 24 px height on one box.
- Fields (cure B) at 1024: the settings search and Members "Joined after/before" `.ss-select` inputs: rest border ratio 3.13 to 3.48 (Light) and 3.13 to 3.42 (Dark) against both grounds; hover border = --ui-text-faint; ring solid 2px --ui-focus offset 1px on the field itself, ratio 4.82 to 5.49; search placeholder = faint, opacity 1, ratio 5.76 Light and 4.81 Dark. Search 36 px at 1024 (unchanged), 44 at 375; Members inputs 40 px at 1024, 44 at 375. The date inputs have no placeholder.

## FAULTS
None. One PROBLEM line is my spec's false alarm: server-roles "Default permissions" 154.7x20@16,427 -> 154.7x44@16,415. That is the cure itself (padding 12 px with margin -12 px, as in `.ss-row > button.ss-name`): the drawn border box is larger, but it is the only box that changed on the page, the row did not grow and x is unchanged. Not a fault.

Notes for the commander:
- `.ss-select` is a shared class: the Admin selects that use it (Accounts, Limits, audit log) also changed rest border and hover. I did not measure them (no such page in part 3).
- moderation/DashboardTab.tsx is unaffected by `.adm-dash-section > .adm-actions[role='group']`: its section holds `.ss-toolbar[role=group]` and no `.adm-actions` (`git grep`). Bans.tsx:82 is the only match. Other `.ss-toolbar` users (CaseQueue, DashboardTab, TakedownsTab) have role=group, so server's `.ss-toolbar[role=toolbar]` gap does not reach them.

## FINDERS
None: I changed no app file; one new spec only.

## JOBS
- 0169, 0170, 0171 shoot base parts 1-3: waited 437, 504, 596 s; ran 64, 78, 50 s; rc 0.
- 0172 build: waited 53, ran 31, rc 0 (superseded: I committed after it, then rebuilt).
- 0192 build: waited 20, ran 32, rc 0.
- 0193, 0194, 0195 shoot after parts 1-3: waited 1611, 1698, 1857 s; ran 72, 72, 42 s; rc 0.

## Commands (whole)
bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p107-integ/apps/web/dist --env SR_P107_STAFF_ON=1 --env SR_P107_STAFF_PHASE=after --env SR_P107_STAFF_PART=<1|2|3> --env SR_P107_STAFF_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-staffshots -- tests/p107-staff-shots.spec.ts

Base figures per part are in that folder as figures-base-part<N>.json; the spec compares only when it finds them.

## CSS files (build 0192, gzip)
- `app/useSettingsLayoutLabels-DlzsH2Kz.css` (Settings.module.css `.navItem`): 1682 B, lazy (not linked from app.html).
- `app/user-settings-BeBZUX1v.css` (`.us-search-input`): 2358 B, lazy.
- `app/Drawer-C9jUCObf.css` 801 B and `app/FilterChips-B8Q1A53Y.css` 651 B: lazy.
- Feedback `.dismiss`: inside `app/app-voXoNvgg.css`, 8428 B, FIRST PAINT (the only one of the set that app.html links).
Base sizes not measured (no base build of these files).

## Shots (visual-sweep-shots/p107-staffshots; base twins `base-`)
Opened after the jobs ended:
- user-account-375-sections: drawer rows tall and evenly spaced, close button visible, What's New and Log Out below.
- user-account-1024-light-focus-settings-search: sidebar layout as before; search with a 2 px blue ring offset 1 px, placeholder readable.
- lib-browse-375 and base-lib-browse-375: identical; chips 12 px row gap, dropdown, chevron, Filters button.
- lib-browse-375-tap-Filters: drawer with Close top right, genre chips.
- friends-375: Notice with the dismiss x at the right, same size as base.
- server-channel-permissions-375 and server-roles-375: lists as before, "Default permissions" row same height, no sideways scroll.
- server-members-375-bulk-bar: Add role beside the count, Remove role, Kick, Ban on the next line with a visible gap.
- server-members-1024-dark-focus-joined-after: bright 2 px ring around the focused date input.
- admin-bans-375: four chips in two rows with a visible gap.
- admin-cinema-live-tv-375-new-channel-form: top of the form only; the daypart is below the fold, covered by the figures (0 fails).
Not opened one by one: the other shots (study, cin-browse, lib-me, server-overview, watch-library, admin-dashboard, library-catalog, the dark twins, the other base twins). Their figures are in the json; no page scrolls sideways (sideways list empty in all three parts).

## Checks
- prettier --check on the spec: ok, 19:26. e2e `tsc --noEmit`: exit 0, 19:26.
- eslint: not run (pnpm not on PATH in this shell).
- Unit tests of nav, kitbits, server files: not run by me.

## Closing text and proposed new rows
Closing: the sections drawer rows, Notice dismiss, filter chips, drawer close, role chip, card links, wrapped toolbars, daypart gap and two fields now pass at 375 and are unchanged at 1024.
Proposed rows (still fail the 44 px test, in no pod): Friends "Back to channels" and "New Group DM" (taker the section bar); Library "Sorted descending"; Library genre chips in the filters drawer; Library and Study empty-state links and Study file rows; Cinema card art links.
Changelog: Settings, filter chips, drawer close buttons and notices are easier to tap on a phone; some settings fields show clearer borders and focus.
Owner questions: none.

## PROCESSES
strays: none found
