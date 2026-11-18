# Wave 105 handoff: probe (D-1012, SEC-AUD-485)

## Clock
Started 15:30. Spec committed 15:33 (helper untouched), base job 15:33 to 15:34. Helper committed 15:35, after job 15:35 to 15:37. Handoff 15:38.

## Done (commits on `fix/p105-probe`)
- `D-1012 probe spec (helper untouched, base run pending)`: `e2e/tests/p105-probe-shots.spec.ts` (new).
- `D-1012 hit-test.ts: a name comes from the control's label (READY)`: the two `name` functions of `e2e/support/hit-test.ts` (the one in `inPage`, used for takers, and `nameOf` in `hitTestAll`, used for rows; the two bodies are identical) plus a "Names" paragraph in the header.
- Order: `aria-label`; `aria-labelledby`; text of the control's own `<label>`; own text; `placeholder`; `title`. Label of a labelable element is `labels[0]` with the control's own subtree left out (otherwise a select's label text would include all its options); a non-labelable element takes the closest `label`'s text; a `<label>` itself takes its own text. All folded to single spaces and cut at 40. Exports, `MENU_ITEM_SELECTOR`, reach rules and the wrapper/label taker rule are untouched.

## Not done
Nothing outstanding.

## JOBS
| id | class | submitted to started (waited) | ran | rc |
|---|---|---|---|---|
| 0022-probe-shoot | shoot base, PHASE=base (helper untouched) | 27 s | 26 s | 0 |
| 0028-probe-shoot | shoot base, PHASE=after (helper edited) | 149 s | 26 s | 0 |
Both `1 passed`, `problems: []`. Spec used the base bundle for both (no app file changed, no build).

## Figures: rows whose name changed (base -> after). Counts, status, takers, path, box and offset equal in every row of every root (the spec compares them)
Roots: members-375 (18 rows: 16 pass, 2 skipped disabled), entitlements-toolbar-375 (2 rows, pass), forum-toolbar-375 (5 rows: 2 pass, 3 fail, same 3 in base: Search posts 237x29, List view and Gallery view 28x28; not part of this task), members-1024 `reach: 'box'` (18 rows, same as 375).

members-375 and members-1024 (identical names in both):
- #0 search input: "Search by name" (placeholder) -> "Search members"
- #1 select: "All rolesNight ShiftRegularsChannel Keep" (its options) -> "Filter by role"
- #2 date input: "" -> "Joined after"
- #3 date input: "" -> "Joined before"
- #4 checkbox: "" -> "Unusual activity only"
- #5 select: "Name (A to Z)Joined (newest first)Joined" -> "Sort by"
- #7 head checkbox (the SEC-AUD-485 case): "" -> "Select all members on this page"
entitlements-toolbar-375:
- #0 select: "AllPatronServer pool" -> "Kind"
- #1 checkbox: "" -> "Only active"
forum-toolbar-375:
- #2 select: "Recent activityDate posted" -> "Sort by"
Takers show the same new names (the spec lists them as notes; no taker changed tag, role or path). Rows named "" in the base: 4 + 4 + 1 + 0 in members-375, members-1024, entitlements, forum (9); after: 0 in every root, so no element with no name source was found. Rows named "" and truly without label, text, placeholder or title: none.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-probe`, opened after the job ended; scrollWidth equals clientWidth, 375/375, in all three)
- `probe-members-375.png`: Admin-style "Sections: Members" picker on top, then (scrolled by the last hit test) "JOINED BEFORE" date field with the "Unusual activity only" checkbox, "SORT BY" select with Prune, "2 members", the checkbox with "Select all members on this page", the Juniper and Rowan cards with their checkboxes. The visible labels equal the new names in the figures.
- `probe-entitlements-375.png`: Entitlements page: Grant button, "Kind" select (All) and the checked "Only active" checkbox on one line, "No entitlements". Names "Kind" and "Only active" match.
- `probe-forum-375.png`: the forum with New Post, "Search posts" field, "Sort by" label with the "Recent activity" select, the two view buttons; "No posts yet". Name "Sort by" matches. The `base-` twins are on disk and were not opened (same page; the figures show equal boxes).

## Who imports the files
`e2e/support/hit-test.ts`: imported by e2e specs only (every spec that imports the helper compiles: e2e `tsc` over all of `e2e/`). No app file, no chunk.

## Checks (own tree, last commit)
- e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: 15:35, exit 0.
- prettier `--check` on both files: clean (15:35).
- eslint: `pnpm` is not on this shell's PATH; not run.
- Spec run twice as above (base, after): passed.

## Helper header sentence (already in the file)
"Names (wave 105, D-1012). A row's and a taker's `name` is, in order: `aria-label`; `aria-labelledby`; the text of the control's own `<label>` ...; its own text; `placeholder`; `title`. Folded to single spaces, 40 characters. The name never changes a status, a taker or a count."

## Closing text for SEC-AUD-485
"The hit-test helper names a control from its own `<label>` when it has no `aria-label` or `aria-labelledby` (D-1012; own text, `placeholder` and `title` follow). The Members 'select all' box, the date fields, 'Unusual activity only' and the Entitlements 'Only active' box, all '' before, now carry their label's text; selects carry their label instead of their options. Proven on four roots (Members 375 and 1024, Entitlements toolbar, forum toolbar): row counts, statuses, takers, boxes and offsets equal; no row is named ''."

## Proposed new rows
- The forum toolbar fails the 375 touch rule in the base: "Search posts" (237x29) and the two view buttons (28x28). Takers not read (not in scope); a later wave should name them.

## Owner questions
None.

## PROCESSES
strays: none found (15:37). No job of mine queued or running.
