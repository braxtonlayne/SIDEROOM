# Wave 85 guild handoff (SEC-AUD-369, 370; D-873)

T = 15:00 on 2026-11-16. Clock times: no new item 16:00, checkpoint 16:15, finished 16:30, hard limit 16:45.
Lock lines read before each round (`heavy.sh status`, 15:10 after my acquire): round 1 `guild 15:10` (read before the stack, before the seed, before the base run and before each "after" run). Round 2 (the caption rule): see "Round 2" below.

## Done
Files: `apps/web/src/settings/server/server-settings.css` (CSS only), `e2e/tests/p85-guild-shots.spec.ts` (new, local only, `SR_P85_GUILD=1`, needs the sweep's seeded world; header says so), this handoff.

### Item 1 (SEC-AUD-369, D-873): Compare levels on a phone
- **Measured (base, 375 x 812, Lounge Light and Dark identical):** the element that scrolls is the table itself (`table.ss-table.sup-compare`: `.ss-table` is `display:block; overflow-x:auto` in the phone block, `server-settings.css` ~`:283`). `scrollWidth - clientWidth` = **134** (client 309 = the card's content width). The tab's scroller (`main`) and the document: 0. Columns: Perk 160, Level 0 67, Level 1 67, Level 2 74, Level 3 74 (= 442), inline padding 8/8. Nothing marked that it scrolls.
- **Which build and why: the second one (sticky + cue).** The four figure columns alone are 282 px at 8 px padding; with 6 px padding they are about 266 px, and the label column cannot go under 7rem (112 px): 112 + 266 = 378 > 309. It cannot fit, so: sticky label column and a cue.
- **Built (phone block, `.ss-table.sup-compare` only):** first column (`:is(th, td):first-child`) `position: sticky; inset-inline-start: 0; z-index: 1` on `var(--ui-bg-raised)` (the token `.ss-card` draws its ground with; probed: Lounge Light `rgb(242, 243, 245)`, Lounge Dark `rgb(43, 45, 49)`, equal to the card's computed background in both), `min-inline-size: 7rem` (label column is now 128 px, was 160). The cue: a scroll-attached shadow on the table's own background (`background-attachment: scroll` shadow at the inline-end edge under a `local` cover in the card's ground at the same edge): shown at scroll 0, gone at the end. The caption (inside the scrolling table) is sticky at the inline start too (round 2).
- **No `.tsx` needed**: the table is the scroller, so no wrapper.
- **RTL:** the label column and the caption use `inset-inline-start`. Gradients and `background` positions have no logical form: a `:dir(rtl)` copy flips them (`to right`, `0 0`). Not run in an RTL page (no RTL shot); say so.
- **After (375, Light and Dark):** `tableOver` 102 (scrolls, with the cue); tab scroller 0, document 0; at scroll 0 the last visible column is Level 2, cut (not the last); scrolled to the end (`scrollLeft` 102) the last column is whole (right edge 342 = the table's), the label column's left edge 33 = the card's inner inline start, `elementFromPoint` at the label cell's inline-end edge returns the label cell for all 6 rows (no figure on top), and the shot shows no figure through it; label lines: 1, 1, 2, 1, 2, 1 (none over 2); the current level's column (`aria-current`, Level 0) is the same as on the base (same th, same weight/colour). 1280 Editorial: table 794 wide, `tableOver` 0, `position` static, background none: identical to the base; the shot is the same.
- The Leaderboards at 375 (Light, Dark): document 0, all 24 measured boxes identical to the base; the shots are the same.

### Item 2 (SEC-AUD-370): a text field in an Admin form
- **Confirmed (base, 375):** Library catalog's search field is a kit `TextInput`: `div._field_12gqi` (kit, `Input.module.css`) with the input at **241 px** (Invite graph's: 298.5 px; the field's own default width), `flex: 0 1 auto` on the wrapper, `size` attribute none, `max-inline-size` none. Select beside it: 309 + 32 px, wrapper 343. The width is the browser's intrinsic input width: **no rule sets 240 px**; the toolbar child (`.ss-toolbar > *` in `server-settings.css`) only has `min-width: 0`, nothing makes a text field's wrapper take the row.
- **Built (one rule, phone block of `server-settings.css`):** `.ss-toolbar > :has(> input:not(.ss-select, [type=checkbox], [type=radio], [type=file], [type=range])) { flex: 1 1 100% }`. The kit's TextInput wrapper is a direct child of the toolbar only in Admin. `.ss-select` is excluded because Members' two date inputs carry it (my first version matched them, 2 matches; fixed and re-measured: 0).
- **After (375):** Library catalog and Invite graph, Light and Dark: field x 16 to 359 (343 wide), selects' wrapper 16 to 359; every control passes the hit test (all `hitTop` and `hitBottom` true, inView true); document 0. 1280: both fields still 241 and 298.5 px, same boxes as the base. In a server's settings: Members, Bans and Roles put the TextInput in `.ss-search` (a `div` between), so the rule matches 0 elements there; Members/Bans/Roles at 375 are unchanged (search field already 343 wide). 0 px moved.
- Not changed, listed: the FeaturedListsCard's "List ID" field in the same Library page and similar toolbar fields elsewhere in Admin take the same rule when they are a direct kit field in `.ss-toolbar`; that is the same improvement. The cause belongs to the kit's field (no flex on its wrapper); the one-line kit fix would be `flex: 1 1 auto` on `.field` in `Input.module.css` (fenced; not done).

### Who loads `server-settings.css`
`admin/ui.tsx` (Admin: staff chunk, loaded by `import()` in `StaffDoor.tsx`) and `settings/server/ui.tsx` (lazy: the server settings route, the Lounge's edit-channel dialog, a listening room's settings, the Leaderboards). No `.tsx` and no first-paint or public file touched; no web build run. Every importer is lazy.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p85-guild` (untracked). Round 1: 13 base + 19 after = 32 PNG. **Opened:** base Compare 375 Dark and Light, 1280; after Compare 375 start and end Dark and Light, 1280; base Library catalog 375 Dark, Invite graph 375 Dark; after Library catalog 375 Dark and Light and 1280, Invite graph 375 Dark and Light and 1280; Leaderboards base and after Light; after Members (Dark) and Roles (Light). **Not opened one by one (say so):** base Leaderboards Dark, after Leaderboards Dark, base Library and Invite graph 375 Light, base 1280 forms, base compare end shots, after Bans (both), Members Light, Roles Dark. Their boxes are in `measure.json` and equal the opened ones' Light/Dark counterparts.

## Not done / listed
- No RTL shot of Compare levels.
- Round 2 results: see below.

## Tests
Spec run on the base (held-back CSS) and after: both passed (it logs, it does not assert). e2e `tsc` clean, prettier clean, `check-css-tokens` clean, `check-doc-links` ok.

## Proposed inventory rows
- SEC-AUD-369: Compare levels (Server Support) on a phone was cut at the card's right edge with no sign it scrolled. Fixed: label column sticky at the inline start on the card's ground, a scroll cue at the inline-end edge. (D-873)
- SEC-AUD-370: a kit text field in a toolbar form in Admin (Library catalog, Invite graph) was 241 px beside full-width selects at 375. Fixed with one phone rule; the kit's `.field` wrapper has no flex of its own (kit fix listed above).

## Changelog sentences
- Compare levels on a phone keeps its label column in place and shows a shadow at the edge while there is more to scroll.
- A text field in an Admin filter form is as wide as its selects on a phone.

## Owner questions
None.

## Clean-up
See the end of this file.
