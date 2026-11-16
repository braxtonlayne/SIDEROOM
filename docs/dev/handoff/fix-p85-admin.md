# Wave 85 admin handoff: SEC-AUD-371 (D-874), Admin's shared table as stacked rows on a phone

T = 15:00 on 2026-11-16. No new item from 16:00, checkpoint 16:15, finished 16:30, hard limit 16:45. Finished at about 15:45.
Branch `fix/p85-admin` (base 774d3073). Web only, no cargo, no Rust, no migration, no new string.

## Done (commits)
- `SEC-AUD-371: Admin's shared table is stacked rows on a phone (D-874)`: `admin/DataTable.tsx` (table class `adm-table adm-table-stack`, `data-label={c.header}` on each body cell, `data-actions` on the actions cell; nothing else), `admin/admin.css` (one block in a `max-width: 720px` media query, every selector under `.adm-table-stack`), one vitest case in `admin/__tests__/p10.test.tsx`, the spec `e2e/tests/p85-admin-shots.spec.ts`.
- `SEC-AUD-371: a button in any cell of a stacked row is 44 px tall`: found by the first round (Applications' Suspend button sits under the name, in the first cell, not in the actions cell, and was 28 px tall).
- `SEC-AUD-371: .us-sr visually hidden in Admin` (the commander's optional item, see below).

The CSS: head hidden to the eye and kept for a screen reader; each row a block with a rule between rows; the first shown cell is the row's title (text colour, weight 700); other cells have the column's name before them from `attr(data-label)` in a fixed label column (`7rem` padding, label absolutely placed, muted text token), so values line up and wrap inside the value column; an empty cell is `display: none`; the actions cell is a wrapping flex line of its own; buttons in any cell and every control of the actions cell `min-block-size: 44px`; `overflow-wrap: anywhere`; the cells' bottom border is removed. Only tokens read from the sheet (`--ui-border`, `--ui-text`, `--ui-text-muted`); no colour value named.

## Who loads what
- `admin/DataTable.tsx`: imported by `library/LibraryCatalogSection.tsx`, `sections/Accounts.tsx`, `Applications.tsx`, `AuditLog.tsx`, `Servers.tsx`, all under `AdminApp`, which `StaffDoor.tsx` loads with a dynamic `import()` (staff chunk). Not in first paint, not public.
- `admin/admin.css`: loaded by `admin/ui.tsx` (Admin) and `admin/SchemaForm.tsx` (member settings: Modules, a server's Features and Channel modules; all lazy settings chunks). Every rule of mine is scoped to `.adm-table-stack` (no member page has it), except the `.us-sr` rule (identical to `settings/user/user-settings.css`'s own).
- No web build run (not needed by the brief).

## Base (confirmed, and that I ran it)
Base case = the same build with the class `adm-table-stack` taken off the tables in the page (the spec does this per page load; every rule is scoped to that class, so the page draws as the base tree did). Run on my stack (ports 8634/5634) with the sweep world.

| Page (375, Lounge Dark and Light) | wrapper overflow before | cells past the right edge before | actions past the edge before | after: overflow / cells / actions off / hit test fails | min label contrast (light / dark) |
|---|---|---|---|---|---|
| Accounts | 467 px | 14 | 1 (View) | 0 / 0 / 0 / 0 (6 actions) | 6.47 / 5.41 |
| Servers | 200 px | 5 | 1 | 0 / 0 / 0 / 0 (2) | 6.47 / 5.41 |
| Applications | 1 px | 0 | 0 | 0 / 0 / 0 / 0 (3 Suspend buttons, 44 px after the second fix; 28 px before it) | 6.47 / 5.41 |
| Platform audit log | 290 px | 48 | none | 0 / 0 / 0 / 0 | 6.47 / 5.41 |
| Library catalog | empty in the seeded world (0 rows): "No series" empty state shot | | | 0 | |

Contrast is computed (label colour composited over the first opaque ground behind it, canvas pixel to RGBA), not read from pixels; all labels are above 4.5.
Column chooser (Accounts, both themes): Email hidden gives 0 Email cells and the first row's lines are Account, State, Joined with 0 empty lines; shown again gives 6 Email cells and the Email line is back. "Load more" did not show in the world (6, 2, 3, 16 rows, one page); the Columns and Export CSV buttons are at the same box before and after (147x36 and 111x36 at the same place).
Click-through on the phone (both themes): View on the first Accounts row opens the account (a second dialog with the account's facts and buttons), Escape returns to the list with 6 rows.
No value in more than three lines except: Accounts rows 4 to 6 (a bot's email, `bot-<19-digit id>@bots.invalid`, 3 lines, plus the "Unverified" note line = 4) and Applications' title cell (name, owner, button). The first is an id inside an email; listed, not fixed (see owner questions).
1280 Editorial, five pages (first round, boxes of table, wrapper and the first six rows of cells, class off against class on): 0 px moved on all five (and on Access log and Invoices).
Not changed, plain `.adm-table`: Access log at 375 is the same before and after (wrapper overflow 181 and 174 px, as before; the spec toggles only tables that carry the class at load). Invoices has no rows in the world (empty state), so it has no table to compare: its empty-state shots are identical before and after.

## Vitest
`admin/__tests__/p10.test.tsx` (the file that renders `DataTable` with rows; `DataTable` describe). New case: the table has `adm-table` and `adm-table-stack` and each body cell's `data-label` is its column header. Run as a single file before (3 DataTable cases pass) and after (23 pass). **Watched the new case fail** with `DataTable.tsx` restored to the base (copied aside, never a stash): 1 failed, then 4 passed with the change.

## Checks
web `tsc`, eslint on my two TS files, prettier on all my files (CSS included), `check-css-tokens` (clean), `i18n-audit --max 0` (0), e2e `tsc`: all clean. `check-doc-links` not re-run after the handoff was written (run it at integration). The spec was run twice with `--project=chromium` (round 1: 3 settings x 7 pages; round 2: the two phone settings), both passed.

## Shots
Folder `visual-sweep-shots/p85-admin`: 24 png from the second round; **Opened by me: all 32 shots of round 1, and from round 2 the two Applications shots (the only page whose drawing changed in round 2); the other 22 of round 2 are re-runs of the same pages with the same CSS and I did not open them again.** (Round 1 also had the five 1280 Editorial "after" shots, Accounts 1280 "before" and Invoices/Library 1280, which I opened and which looked unchanged; my second run cleared the folder, so those 1280 files and the first round's `measure.json` are gone; the figures above come from the first round's console log). If the commander wants the 1280 files back, run `SR_P85_ADMIN=1 SR_P85_SETTINGS=desk` on a seeded world (about 1 minute).
In the folder: five pages at 375 in Dark and Light after (library catalog and invoices as empty states), Accounts and Servers at 375 Dark before, Access log and Invoices at 375 Dark before and Light/Dark after, the chooser with a column hidden (Light and Dark), the click-through (Light and Dark), and "rows" shots of Accounts (rows 4 to 6) in both themes.

## Optional item: `.us-sr` in Admin
`.us-sr` (the donations tables' visually-hidden captions) is defined only in `settings/user/user-settings.css`, not loaded in Admin. Added to `admin.css` as `.adm-vh, .us-sr { ... }` (same declarations as the member sheet's rule, so the member pages are unchanged). **Not verified in a browser**: my one try at a quick check hung and I stopped it; the rule is a copy of the existing one. The donations builder should confirm the captions are gone at 1280.

## Proposed inventory row
SEC-AUD-371 (D-874): "Admin's shared table (`DataTable`: Accounts, Servers, Applications, Platform audit log, Library catalog) is stacked rows on a phone: first shown cell as title, label/value lines from `data-label`, actions on a line of their own with 44 px targets; no sideways scroll (Accounts was 467 px wider than its box, Servers 200, audit log 290). Closed." SEC-AUD-363 stays open with: the three hand-made tables (Access log, Crash reports, Cinema catalog, plus Invoices/Entitlements when they have rows), the Arcade page's heading, the bare controls of Entrance and Arcade, the Invoices label.

## Changelog sentence
Admin's Accounts, Servers, Applications, Platform audit log and Library catalog lists now show as stacked rows on a phone, with each value labelled and the row's actions on their own line, instead of a table that scrolled sideways.

## Cleanup
Stack stopped by PID (the server, Vite and their children), scratch database `sr_p85_admin_ui` dropped, `p85-admin-bin` deleted (`p80-base-bin` untouched), lock released. No stash touched.

## Owner questions
- A bot's email with its 19-digit id wraps to 3 lines plus "Unverified" (4). Accept, or widen the value column by narrowing the label column from 6.5rem?
- Access log, Crash reports, Cinema catalog (and Invoices/Entitlements once they have rows) stay as they are; do they get the same class in a later wave?
