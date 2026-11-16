# Wave 84 admin builder: SEC-AUD-362 (D-867) and SEC-AUD-363 (D-868)

Branch `fix/p84-admin`. Files: `apps/web/src/admin/admin.css`, `e2e/tests/p84-admin-shots.spec.ts` (new, local only), this handoff. Web only, CSS only: no vitest, no web build, no cargo.

T = 13:20 on 2026-11-16: no new item 15:05, checkpoint 15:25, finished 15:50, hard limit 16:05. The lock was busy for about 35 minutes between my two rounds (guild, lounge, support and settings held it); round 1 ran 13:21-13:27, round 2 14:07-14:12. Finished before 14:30.

## Who loads `admin.css`
`admin/ui.tsx` (the staff chunk: `StaffDoor.tsx` loads `AdminApp` with `import('./AdminApp')`, the re-auth host is a lazy import in `router.tsx`; also `review/ReviewExtras.tsx` imports `admin/ui`) and `admin/SchemaForm.tsx` (the member's Settings > Modules, a server's Features, a channel's Modules: the lazy Settings chunk). No first-paint file and no public page imports either (git grep): changed, no build needed.

## Item 1: the unsaved bar (SEC-AUD-362, D-867)
**Base (round 1, `admin.css` as on the branch, 375x812 touch, Lounge Light):** channel Modules: buttons 235x62 and **40x62, the label in 4 lines**, the tab's scroller **8 px** wider than its box. Member's Modules: 111.9x36 and 163.1x36 (2 lines in the second). Server's Features and Admin > Feature modules: 28 to 36 px tall. Member's Modules start x of the first button against the card's text column: **13 px** (clean and dirty), at 1280 too.

**Rules (`admin.css`):** (a) `.us-card .adm-unsaved:not([data-dirty='true'])`: no inline padding and no inline border, so the clean bar sits on the card's text column; the dirty, raised bar keeps its padding. (b) `@media (max-width: 720px)`: `.adm-unsaved .adm-actions` is `flex: 1 1 auto; flex-wrap: wrap; min-inline-size: 0; max-inline-size: 100%`; both buttons `flex: 1 1 auto; min-block-size: 44px` (the `:first-child` `flex: none` is overridden). `.adm-actions` outside `.adm-unsaved` is untouched.

**After (375, Lounge Light and Lounge Dark, identical; clean and dirty; the four pages):**
| page | clean | dirty | scroller sideways | dirty bar covers the edited field (scrolled to mid-viewport) |
|---|---|---|---|---|
| channel Modules | 283x44 + 283x44, two rows, 1 line each, inside card | same | **0** (was 8) | no |
| member Modules | 113.6x44 + 187.4x44, one row; first button starts **0 px** from the text column (was 13) | two rows 283x44 each (the raised bar has its padding) | 0 | no |
| server Features | 135.2x44 + 139.8x44, one row | same | 0 | no |
| Admin > Feature modules (drawer) | 138.9x44 + 161.1x44, one row | same | 0 | no |
No label in more than one line; every button inside its card; document sideways scroll 0.
**Click-through at 375 (channel Modules, both themes, `bar.json`):** change a field, Discard: the field is back and the bar clean; change again, Save: value saved, bar clean. (The spec puts the value back afterwards.)
**At 1280 in Editorial** (boxes logged on the base and after in `bar.json`): channel Modules, server Features, Admin features unchanged (235x36 / 181x36 and 28 px buttons, same x); **the member's Modules tab moved 13 px** (first button x 0 vs 13 from the text column); the 2nd button moved the same 13 px to the left. The dirty bar at 1280 is as before. No logged box on the other three moved (item 2's base/after boxes compare at 1280: 0 px moved on every Admin page).
Note: in a server's settings card (`.ss-card`) the clean bar still keeps its 13 px: the task asked for `.us-card` only.
**Shots opened (all):** 24 base+after at 375 light (`base-bar-*` / `after-bar-*`), the dark ones, the three click shots, member Modules 1280 before/after: every one opened (contact sheets of 5 side by side at full size, `sheets/` in the shots folder); `after-bar-*-dirty-mid` taken with the field scrolled to the middle.

## Item 2: Admin looked at (SEC-AUD-363, D-868)
Spec: `e2e/tests/p84-admin-shots.spec.ts` (`SR_P84_ADMIN=1`; part `walk`). **34 pages** read from the navigation (the dashboard is the first), at 375 Lounge Light, 375 Lounge Dark and 1280 Editorial (`data-theme` read from Admin's `.ss-root` before each shot and written into the file name: it followed the account's main theme every time) = **102 page-settings, 162 shots in the first walk** (viewport + mid and end where taller than two viewports; 11 pages at 375, 8 at 1280), the second walk (before and after the fixes, `b2-`/`a2-`) 2 x 162 more. **All pages reached** (the stack ran with `SR_ADMIN_REQUIRE_REAUTH=0`; no re-auth step). Folder `visual-sweep-shots/p84-admin` (shots, `measure.json`, `bar.json`, `sheets/`, not committed).
**Opened:** every 375 shot of the first walk, light and dark, as contact sheets of 5 shots at full size (7 sheets each for viewport, 2 for mid, 2 for end, per theme); the 1280 viewport shots as 2x2 sheets at 78 % size (all 34) and the mid/end ones I did not open (their tall pages' first viewport and the phone's long shots were opened; the 1280 mid/end of Features, Limits, Entrance, System, Runbook, Access log: **not opened**, say so). After the fixes I opened the three settings of Runbook and Ingest sources, 375 Invoices, Entitlements, Donations, Invite graph, Dashboard.
**Whole pages (nothing in D-854's list but kit or own-design items): 22 of 34** at 375 (every page not named in the table below).
**Sideways scroll:** document 0 and scroller 0 on all 102.
**Kit controls failing the hit test:** **431** per phone setting (summed over pages, nav toggle and the like repeated on every page) at 375; unchanged by my rules. Ten worst of the others (not kit; 64 per setting after the fixes, 34 of them the off-screen skip link): Pricing and perks' per-tier table inputs (13 inputs 79x21 and a 13x13 checkbox), Donations' "Network" `<select>` (70x19), Invoices' search input and its 13x13 checkbox, Entitlements' 13x13 checkbox, the Dashboard's skip link.

### Faults (D-854)
| page, size, theme | fault and figure | node | file and rule | status |
|---|---|---|---|---|
| Runbook, all sizes, **Lounge Light** (and all themes: serif display face) | **Headings unreadable**: "Server down", "High error rate" ... in the Entrance's cream (about 1.2:1 on white), in the Entrance's serif | `.adm-runbook .ent-copy h2` | `components/entrance.css` `.ent-copy h2/h3/h4` (`var(--sr-text)`, display font) reaches Admin through `EntranceMarkdown` | **Fixed** in `admin.css` (`.adm-runbook .ent-copy h2,h3,h4` take `--ui-text`, the page's font, 600; the links `--ui-accent`) |
| Ingest sources, all | Title a bare `<h2>`: larger (about 1.5rem), 23 px lower than every other page's title, no gap before the lead | `h2` in `IngestSources.tsx:163` | global `h2`; no class | **Fixed**: `admin.css` `.ss-root h2:not([class]):has(+ p.ss-desc)` (1.25rem, 700, 4 px/24 px margins). Collateral: matches only this page (grep) |
| Donations, 375 | "unknown" tile text 127 px in a 115 px box (runs to the tile's edge) | `div.adm-stat-value` | `admin.css` `.adm-stat-value` 1.75rem | **Fixed** in the phone block: 1.375rem and `overflow-wrap: anywhere`. Side effect: every stat tile's number is smaller on a phone (Dashboard, System, Donations); the Dashboard's boxes moved up to 46 px (shorter tiles) |
| Invite graph, 375 | Tree node name button 20 px tall, fails the hit test | `button.adm-link` in `.adm-tree-row` | `admin.css` `.adm-link` | **Fixed** (phone only, 44 px) |
| Invoices, Entitlements, 375 | Toolbar `<select>`s (40 px) and the checkbox labels fail the hit test (7 controls) | `select`, `label` in `.ss-toolbar` | `admin.css` `.ss-root .ss-toolbar select:not([class])` min-height 40 | **Fixed** (phone only, 44 px; moves the toolbars by 4 to 12 px at 375) |
| Arcade, all | Title in `<h2 id="arc-admin">`, with a rule above and about 40 px more space than other pages; sub-headings bigger than other pages' | `h2#arc-admin` | `arcade/arcade.css`, `ArcadeAdmin.tsx:79` (not mine) | Listed: give the page `SectionHeader` or its sheet the Admin sizes |
| Accounts, Servers, Access log, Platform audit log, Crash reports, Cinema catalog, 375 | **Tables whose rows carry actions or columns off the screen** (Email cut, State/Joined/View off; Servers' Channels cut; Access log "What" and Hash cut; wrapped 5-line ids) | `table.adm-table` in `.adm-table-wrap` (scrolls inside) | `admin.css` `.adm-table-wrap { overflow-x: auto }` | Listed (D-864: stacked rows are a wave item of their own) |
| Pricing and perks, 375 | Per-tier table: 13 inputs 79x21 (fail the hit test), first column's checkbox half cut at the left, "Downloads/day" cut at the right | `input` in the perks table | `PricingAdmin` markup, bare `<input>`s; no class to hit | Listed (needs a class in the `.tsx`) |
| Donations, 375 | Network `<select>` 70x19, native-looking next to the kit's | `select` in Donations' wallet form | bare `<select>`, outside `.ss-toolbar` | Listed |
| Pricing, Invoices, Entitlements, Arcade, Entrance pages | Bare native `<select>`, checkbox and file input next to kit controls (look differs: arrow, height) | `select`, `input[type=checkbox]` | `admin.css:603-618` styles only those inside `.ss-toolbar` | Listed |
| Invoices, 375 and 1280 | Search field: its label sits above the State/Purpose line (not aligned with them) | `form.ss-toolbar` | `server-settings.css` `.ss-toolbar` | Listed (not mine) |
| Moderation, 375 | Tab strip scrolled to its end on load (first tabs cut at the left) | `.ss-tabs` | `server-settings.css` | Listed, probably a scroll-into-view of the active tab (own design?) |
| Private content, 375 | Segmented control's labels "Account DMs", "Server channels" wrap to 2 lines | kit `SegmentedControl` | kit (D-859, 353) | Listed (kit) |
| Library catalog, Invite graph, 375 | Text fields at 240 px beside full-width selects (widths differ within one form) | the text input | `server-settings.css` | Listed |
| Command line, 375 | `sr-cli ...` code blocks cut at the card's edge | `pre` in the command cards | scroll by design | not a fault (own design) |

Not faults, seen: dark/light contrast of muted text read well on every page; dark-only issues none beyond the Runbook (Lounge Dark and Editorial were fine); the offscreen skip link counts as a failed hit test on every page (hidden until focus).

### Fixes: rule, shots, 1280 check
Fixes 1 and 2 and 5 and the two hit-target ones, all in `admin.css` after item 1's rules. Walk before = `b2-*` (the file with item 1 only), after = `a2-*`, all three settings, every page (102 x 2), all compared:
- **At 1280 in Editorial: 0 of 34 pages have a logged card/table box that moved by more than 2 px** (and the document/scroller scroll stayed 0).
- At 375 (phone only rules), moved by cause: Dashboard 46 px (stat tiles shorter), System 35 boxes by up to 13 px (tiles), Donations 13 px (tiles), Invoices up to 12 px / Entitlements 4 px / Pricing 4 px (44 px selects and labels), Invite graph 23 px (the 44 px tree name), Runbook table 28 px (the headings), Access log 7 px (table). Nothing else moved.
- Hit test: others 71 -> 64 failing per phone setting; text wider than its box 2 -> 1 ("moderation.new_account_after_ban_days" label, 3 px, a kit label).
- Shots: `b2-adm-30-runbook-*` / `a2-adm-30-runbook-*` (3 settings and mid/end), `b2/a2-adm-14-ingest-sources-*`, `adm-20-donations`, `adm-21-invoices`, `adm-22-entitlements`, `adm-09-invite-graph`: the three after shots of each opened; faults gone.
- **Member pages that load the sheet:** the member's Modules tab and a server's Features at 1280 and 375, boxes in `bar.json` (the `b2`/`a2` lines carry `boxes`): 0 moved beyond item 1's (the member's Modules 13 px).
`node scripts/check-css-tokens.mjs` clean; prettier clean.

## Tests and checks
No vitest (CSS only). e2e `tsc` passes with the spec. Spec watched fail on the base: the bar part's figures on the base are the base (40x62, 4 lines, 8 px, 13 px): `bar.json` lines tagged `base`. Docs: `git grep -n -i unsaved -- docs` finds no sentence about the bar's layout: nothing to change.

## Proposed inventory rows
- **SEC-AUD-362** (closed): the unsaved bar whole on a phone and flush in a member's card (`admin.css`). Closes the Discard entry of SEC-AUD-350 and row 16 of SEC-AUD-349.
- **SEC-AUD-363** (open, with the list): Admin looked at: 34 pages, 102 settings, five fixes (runbook headings, Ingest title, a stat tile, two phone hit targets) and the unfixed list above (tables with actions off screen, the Arcade page's title, bare native controls, the Pricing tier table, Donations' select, kit controls).

## Changelog
- SEC-AUD-362: the unsaved-changes bar now keeps both its buttons whole and 44 px tall on a phone (on two rows where the labels do not fit) and lines up with the card's text in a member's Modules tab.
- SEC-AUD-363: Admin pages looked at on a phone and a desktop; the runbook's headings are readable in the light theme, the Ingest sources title matches the other pages, and a few small controls reach 44 px on a phone.

## Owner questions
- A page title as a bare `<h2>` (Ingest sources) and the Arcade page's own header: should both use `SectionHeader` (a `.tsx` change)? I fixed Ingest in CSS only.
- Smaller stat numbers on a phone (22 px instead of 28 px) was my price for a word-valued tile ("unknown"): keep, or shorten the word to "-" in the `.tsx`?

## Cleanup
Stack stopped by port/PID, `sr_p84_admin_ui` dropped, `p84-admin-bin` deleted (`p80-base-bin` untouched); lock released. No stash touched.
