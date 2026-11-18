# Handoff: fix/p106-fields (wave 106, census builder "fields"; D-1018 follow-up; measure only)

Report (the deliverable): `C:/Users/braxt/Desktop/SIDEROOM_wt/p106-fields-report.md`. Figures: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-fields/figures-part{1,2,3,4,5}.json`; shots `base-*.png` in the same folder. No app file is changed; no `build` job; every shoot is `shoot base`.

## Clock times
Started 16:30. Spec first committed 16:33, first figures 16:37. Checkpoints committed 16:55 (stub) and 17:25. Last job (0138) ended 17:32. Report and handoff done 17:36. Nothing queued or running of mine at reply time.

## Done (commits on fix/p106-fields)
- `e2e/tests/p106-fields-shots.spec.ts` (new, the only code): measures every non-kit text field a member reaches: border colour, field ground, ground outside, B/out, B/fld, F/out, placeholder and text ratios, height, the stylesheet rules that matched, the wrapper that draws the box when the field has no border (header search, composer, emoji search), hover and keyboard-focus reads, and at 375 touch the height and `hitTest`. A kit field on the same page is read the same way. Parts by `SR_P106_FIELDS_PART`: 1 Lounge, forum, inline edit, settings at 1024 in Light and Dark; 2 the Drawer's tools in Light; 5 the same in Dark; 4 Library, Emulator, Developers, Admin in the shipped section theme and in lounge-light; 3 the lot at 375 touch. It makes one text channel and one forum channel with its own names (`p106-fields-<id>`, `p106-fields-forum-<id>`) and deletes both in a `finally`.
- This handoff. The report is outside the repo (the row names that path).

## Not done (why)
- Not reached in the seeded world (so no figures, sheet rule from the css only): Poll dialog and Upload tray fields, profile popout, DM panel, bridged member search, scrapbook (operator-gated feature; switching it on is a world change), Study dialogs and notes (`/study` shows no field at rest), Server settings > Channels fields, music panel, Jukebox, Saved messages, Watch Party start dialog, developer secret field, admin perks table, Image Studio and GIF fields, Notes and Timers tools (no Drawer option for them), Cinema search overlay. Listed in the report, section 1.
- Entrance and public-profile fields listed apart, not measured (D-385).
- Dark figures are `lounge-dark` only, not ash, onyx, editorial, sideroom.
- The phone run (part 3) predates a spec fix that reads a single-side border (the Library search has only a bottom line): its Library line says "no border"; the 1024 figures (part 4, job 0138) are right: 1.32 on `sideroom`, 1.61 on lounge-light.
- eslint on the spec: not run (no eslint config covers `e2e/`: `eslint` from `apps/web` says "File ignored because outside of base path", from `e2e/` it finds no config).

## FINDERS (rule 13)
None: the census changes no control. For the planner the report (section 3, "Finders by class") counts, per class, CI specs in `e2e/`, unit tests in `apps/web/src` and retired shots specs that name it: no CI spec and no unit test finds any of these fields by class. The only hits are retired `p<NN>-*-shots` specs (not edited). Fields found by role, label or placeholder are not counted.

## JOBS (id, class, waited, ran, rc; times from `cli-agents/run/jobs/<id>/{submitted,started,ended,rc}`)
| id | what | waited | ran | rc |
|---|---|---|---|---|
| 0090 | shoot base, part 1 (first spec) | 136 s | 63 s | 0 |
| 0098 | shoot base, part 2 (tools Light+Dark and section pages in one run) | 1111 s | 557 s | 1: the test hit its 540 s timeout; the Notes tool stop waited for a Drawer option that does not exist. Fixed in the spec (6 s click timeout, Notes and Timers dropped, run split in parts 2, 5, 4); figures are now written per stop |
| 0100 | part 1 (second spec) | 1253 s | 106 s | 0 |
| 0101 | part 3 submitted too early | cancelled | | 143 |
| 0115 | part 2 (tools Light) | 315 s | 109 s | 0 |
| 0116 | part 4 (sections, first) | 589 s | 38 s | 0 |
| 0117 | part 3 (375 touch) | 655 s | 143 s | 0 |
| 0118 | part 5 (tools Dark) | 803 s | 102 s | 0 |
| 0121 | part 1 (wrapper search widened, forum, inline edit) | 734 s | 105 s | 0 |
| 0130 | part 4 (with Cinema search) | 599 s | 44 s | 0 |
| 0138 | part 4 (single-side border read) | 98 s | 44 s | 0 |

Waits were long because the runner was stopped for mount's cargo build (`runner: NOT running` from about 16:41 to 16:53) and then jobs queued behind eleven builders. A fault of a script: none seen. Each failed job was read before the next was submitted (only 0098 failed; its cause is above). Part 1 ran three times after spec fixes; no flaky results.

## Figures against the task's two lists
No "may change" or "must not change" list: a census, no layout change. Headline figures (full tables in the report):
- No app field reaches 3:1 at rest by border (1.09 to 1.61 in Light and Dark across the rules; the one at or above 3:1 is the tier-row name field on its coloured head, 3.15 on red).
- App fields never change border on hover (0 of 138 reads); the kit's field does (16 of 16).
- No focus indicator at all: the header search, the composer and the inline message edit (Light and Dark: six entries). The cause is `.lng-search input:focus-visible` and `.lng-textarea:focus-visible` setting `outline: none`; the wrappers show nothing either.
- Placeholder under 4.5:1: the Lounge's own searches and the composer in Light (3.65 to 4.15); in Dark 2.74 to 3.58 on the Drawer input, the calculator-family fields, the settings search and the Lounge searches.
- 375 touch: every tool field and the Lounge emoji search are 25 to 32 px high and fail the default hit test (a neighbour takes the point 21 px from the centre); the composer (56), forum search (44) and the settings fields (44) pass.

## Shots (98, `base-*.png`; looked at as contact sheets of four to six shots at 640 x 480 each; I did not open each 1024 x 768 original one by one)
- `base-light/dark-channel`: header search pill and composer are flat grounds with no border; no ring visible in either.
- `base-light/dark-emoji-picker`: the picker's search is a flat ground, no border; composer behind it flat.
- `base-light/dark-threads`: Threads popover: "Search threads" has a faint 1 px border; hard to see in Dark.
- `base-light/dark-inbox`: the popover has no text field (nothing of its own to measure).
- `base-light/dark-switcher`: Drawer just opened: one 44 px field with a faint border, quiet in Dark.
- `base-light/dark-forum`: empty forum: search and Sort toolbar, the search a faint box.
- `base-light/dark-forum-new-post`: New Post dialog: Title (kit) and Message (app) are the same faint boxes.
- `base-light/dark-inline-edit`: inline edit of a message: a plain ground, no border, "escape to cancel, enter to save".
- `base-light/dark-members-popout`: my click did not open a popout; the channel page only (no field of its own).
- `base-light/dark-user-settings`, `-user-profile`: settings search (app, bordered) beside kit fields (Password rows, profile fields): same look; Light clearer than Dark.
- `base-light/dark-server-members`: Members page: kit search, date inputs (app class) and selects all alike, faint borders.
- `base-light-tool-*` (21), `base-dark-tool-*` (21): each tool in the Drawer. Fields are pale boxes on the drawer ground; in Light visible but quiet; in Dark the poll rows, the invite title and the loot name are nearly invisible. Audio shows only a file chooser (no text field). Tier list: coloured row heads with white name fields.
- `base-shipped-/base-seclight-` library-browse, emulator, developers, admin-donations: Library search is an underline only (hard to see in the shipped dark theme), Emulator search a faint box (almost invisible in dark); Developers and Admin donations show only kit textareas (the same faint box).
- `base-phone-*` (24): at 375 the Lounge shell differs (no header search; composer in a pill); tool fields small and tight; the Drawer input shows a blue ring; Library is the dark section with a different search layout; settings pages stack.

## Who imports my files and which chunk
Only the spec (e2e). It imports `../support/{cli,fixtures,hit-test,contrast}` read only. No app file changed, so no first-paint question.

## Checks (own tree, last commit)
- prettier on the spec: run, clean (17:26).
- e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: run 17:26, exit 0.
- eslint on the spec: not run (see Not done).
- Not run: web tsc, build, vitest, i18n audit, css tokens check, doc links (no app, css or doc change).

## Closing text for my rows and proposed new rows
Closing text: "D-1018 follow-up: census done. 163 distinct non-kit fields (240 raw reads across configurations) over 17 sheet rules; none reaches 3:1 at rest by border; the header search, composer and inline edit show no focus indicator; the tools' sheets draw the kit's own box and will read as the quiet ones after the kit's change; report `p106-fields-report.md`."

Proposed rows (faults the base has, with takers):
1. Header search and composer: no focus indicator (`.lng-search input:focus-visible`, `.lng-textarea:focus-visible`, both `outline: none`; taker: lounge.css, of which header owns only :810-895).
2. Placeholder under 4.5:1 on app fields in Lounge Dark (2.74 to 3.58) and on the Lounge searches and composer in Light (3.65 to 4.15).
3. Tool, poll and tier fields 25 to 32 px high at 375 (`.sr-room input`, `.sr-calc-input`, `.sr-card input`, `.ud-root input`); the touch census of the tools rooms covers the same page.
4. App fields have no hover change (the kit's has).
5. The Library search is an underline only, 1.32 to 1.61:1.

## One changelog sentence
Measured, without changing anything, every text field the kit does not draw: 17 sheet rules, none with a 3:1 resting boundary, three fields with no focus indicator.

## Owner questions (go to Fable; Discord-like default)
1. Should the header search and composer show a focus ring? Default: yes, a 2 px ring on the search pill and a subtle one on the composer box.
2. Should the app's own plain labelled fields (tools, date inputs, New Post message) become the kit's `TextInput`, and the others (icon searches, composer, inline edit, tier names) keep their look and take the kit's border mix? Default: yes.
3. Dark placeholders at 2.74 to 3.58:1: raise to 4.5:1? Default: yes, to the kit's placeholder level.

## PROCESSES
`strays.sh p106-fields` at 17:33: `strays: none found`. No job of mine queued or running (`job.sh status` 17:33).
