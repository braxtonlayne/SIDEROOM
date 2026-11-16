# Wave 84 handoff: guild (SEC-AUD-360, D-864; SEC-AUD-361, D-865, D-866)

T = 13:20 on 2026-11-16. Clock times: no new item from 15:05, checkpoint 15:25, finished 15:50, hard limit 16:05.
Actual: code, vitest and spec compiled and committed by 13:25 (the commander's clock read 13:17 at launch); round 1: lock 13:27, stack up 13:27, world seeded 13:28, base (my four files put back to the base's with `git show HEAD~1:`, then restored with `git checkout HEAD --`) 13:28 to 13:31, after 13:32 to 13:33, stack stopped and lock released 13:33 (6 minutes). Round 2 (one cosmetic fix): see "Round 2".

## What I saw on the base (and that I ran it)
Ran: the same spec `e2e/tests/p84-guild-shots.spec.ts` on the base (the four files as the base has them) and on my change, on the sweep's seeded world (2 members, 2 invites: the spec makes the second through the API). Base figures at 375 (Lounge Light and Lounge Dark identical): Members table `scrollWidth - clientWidth` **329**, Invites **177**; all 8 controls of Members' two rows (checkbox 13 x 13, "Details for ..." 52 x 17, Roles 60 x 28, the menu 28 x 28) and the 4 of Invites' two rows (Copy link 84 x 28, Revoke 71 x 28) **fail the hit test** (my probe scrolls each control into view first, so "inside the viewport" reads true on the base: the real state is that Roles, the menu, Copy link and Revoke are past the right edge until the table is scrolled sideways, as the wave 83 shot showed). Voice channel's title "# Hangout"; sliders `accent-color: auto`; `.ss-select` 40; Compare levels first column 1, 2, 4, 1, 2, 2 lines. The plan's Admin claim does not hold (see (c)).

## Item 1: Members and Invites as stacked rows (SEC-AUD-360, D-864)
**The rules** (`server-settings.css`, a `max-width: 720px` block at the end, all under `.ss-table-stack`, so they reach only the two tables that carry the class):
- the table (`overflow-x: visible`), body, rows, cells are blocks; the head is hidden to the eye and kept for a screen reader as `bridge.css` does; the caption is a block line (as a table caption inside the block table it shrank to a narrow column);
- a row is a wrapping flex line with a rule between rows; every cell takes a line of its own; a cell with nothing in it (`td:empty`, a signals or roles cell whose `.ss-chips` is empty) takes none;
- the name cell first: Members' checkbox cell and the name cell share the first line, the name in weight 700; Invites' first cell (the inviter) is the strong line with no label;
- each fact cell is "label: value" (`td[data-label]::before { content: attr(data-label) ':' }`, muted token);
- the actions cell last, on a line of its own, its buttons whole at the row's start (`.ss-actions` wraps, `justify-content: flex-start`);
- 44 px: the row checkbox is 20 px with a 12 px margin and an `::after { inset: -12px }`, "Details for ..." (`.ss-link`) `min-block-size: 44px`, the action buttons `min-block-size: 44px` (the kit's `sm` is 28; the rule is on `td .ss-actions button`, so nothing kit-side changed).
`.tsx`: `Members.tsx` and `Invites.tsx`: the class `ss-table-stack` on the table and a `data-label` (the header's own `t()` call, no new key) on Since, Joined via, Signals and Roles (Members) and on Code, Uses, Expires and Channel (Invites).

**The row controls (375, Lounge Dark; Lounge Light is identical)**

| control | before: box, hit test | after: box, inside the viewport, hit test |
|---|---|---|
| Members row checkbox "Select Juniper" | 13 x 13, fail | 20 x 20 (+12 px pseudo-element each side), inside, pass |
| "Details for Juniper" | 52 x 17, fail | 54 x 44, inside, pass |
| Roles | 60 x 28, fail (past the right edge) | 60 x 44, inside, pass |
| "More actions for Juniper" | 28 x 28, fail (past the right edge) | 44 x 44, inside, pass |
| Invites: Copy link | 84 x 28, fail (past the right edge) | 84 x 44, inside, pass |
| Invites: Revoke | 71 x 28, fail (past the right edge) | 71 x 44, inside, pass |
| role chip (`span.ss-role-chip`) | 93 x 19 | 93 x 19: **not a control** (a span, no handler); left as it is, so that a row of wrapped chips does not grow. Wave 83 and the task called it a "button": it is not one in this tree. Owner question 2. |

All 12 controls of the four rows (both lists, both themes) pass; none is "kit". Table and tab scroller `scrollWidth - clientWidth`: **0 and 0** in both themes (base 329 and 177). No text cut (`wide` 0). Shots opened.

**Click-through on the phone (touch; Lounge Light and Lounge Dark; shots `after-click-*`)**: Roles tapped: the "Roles for Juniper" dialog opens inside the viewport (15, 302, 345 x 209), closed with its own button; the row menu (the "..." tapped) opens at (8, 575, 200 x 134), inside (Timeout, Kick, Ban); the row checkbox tapped 18 px below its centre (inside the 44 px target, outside the drawn box) is ticked and the bulk bar appears ("1 member selected", Add role, Remove role, Kick, Ban, Clear selection); Invites: Copy link tapped, the button reads "Copied!" (clipboard permission granted to the context).
**1280 in Editorial: 0 px moved**: the Members and Invites shots are byte-identical to the base's (and the box logs: the only entry that differs is the table's class name in the log key).
Not done in a shot: Members with more than two rows (the world has two; a list longer than the viewport is shown by the scrolled shots of Overview and Compare, not of Members).

## Item 2 (SEC-AUD-361)
- **(a) The voice channel's title.** `Channels.tsx`: one helper `kindMark(c)` (speaker for voice and stage, "# " otherwise) used by the tree and by the editor's title. The tree's "icon" for a voice channel is the speaker emoji in its button text, so the title carries the same character, no new string, the same text a screen reader gets in the tree. Before: "# Hangout"; after: "🔊 Hangout" (375 and 1280); the text channel is "# general" in both, **0 px moved** at 1280 and at 375 (375: +8 px only from the selects, (c)). The voice title is 3 px taller at both sizes (the emoji's line box). Vitest: `channels.test.tsx` had no test expecting "# " before a voice channel; I added one (heading levels, text and voice).
- **(b) The sliders.** `.ss-root input[type='range'] { accent-color: var(--ui-accent) }` (`--ui-accent` is `#c2b08f` in Editorial, `#5865f2` in the Lounge's themes, from `themes.ts`). Probed computed `accent-color`: base `auto`; after Editorial `rgb(194, 176, 143)`, Lounge Dark `rgb(88, 101, 242)` (Lounge Light the same blue). The 44 px phone height stays (44 at 375).
- **(c) `.ss-select` on a phone.** `min-height: 44px` inside a `max-width: 720px` block. 375: every `.ss-select` 40 -> 44 (Members' two, Overview's four, a text channel's two, a voice channel's one); what is under a select moves by 4 px (e.g. Members' toolbar +16 for its four selects/fields rows, Overview +16 over the page), nothing else; 1280: 40 stays, 0 px moved (Members, Invites, Overview, channel tabs: boxes and bytes). **The plan's Admin claim does not hold: Admin's three pages (Accounts, Limits & settings, the platform audit log) use no `.ss-select`** (no native `select` at all on them); I measured every `select` on them at 375 and 1280 in Lounge Dark before and after: none native, no box moved, the 12 shots byte-identical except Limits at 1280 end (visually the same, opened). So (c) reaches no Admin page, and D-865's "Admin alike" has no case to apply to today.
- **(d) Compare levels.** `.ss-table.sup-compare :is(th, td):first-child { min-inline-size: 9rem }` in a phone block of my sheet. First-column lines at 375: 1, 2, 4, 1, 2, 2 before; 1, 1, 1, 1, 2, 1 after (none over 2); the table scrolls inside its card (over 53 -> 134), the tab's scroller 0; 1280 byte-identical.
- **(e) `.ss-root`.** A comment at the rule, nothing else.

## Who loads the sheet, and whom each rule reaches
`server-settings.css` is imported by `admin/ui.tsx` (staff chunk, `import()` in `StaffDoor.tsx`) and `settings/server/ui.tsx` (lazy: the server settings route, the Lounge's edit-channel dialog, a listening room's settings, the Leaderboards): lazy chunks only; no web build, no `.tsx` outside the lazy chunks changed. Rules: `.ss-table-stack` block: only the two tables that carry the class (Members, Invites). `.ss-root input[type='range']`: any `.ss-root` page (server settings and Admin): only a voice channel's Overview has range inputs; Admin has none. `.ss-select` 44 on a phone: every `.ss-select` in server settings, the edit-channel dialog and the Leaderboards if they use it (none in Admin, see (c)). `.ss-table.sup-compare`: Server Support's Compare card only.

## Tests
Single files: `people.test.tsx` 16 passed, `channels.test.tsx` 16 passed (15 + my new test), `content.test.tsx` (renders Invites) 14 passed. I ran them after my change; **I did not run `people.test.tsx` or `content.test.tsx` on the base** (a class and a data-label broke no query). The new channels test was written with the change and not watched failing on the base. web typecheck, eslint and prettier on my files, i18n audit, css tokens, doc links, e2e tsc: see the end of this file.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p84-guild` (`base/`, `after/`, `all/measure.json`; uncommitted). Opened: Members at 375 base (Lounge Light) and after (Lounge Light, Lounge Dark), Members 1280 after, Invites 375 after (Light) and the copied state (Dark), the click-through's roles, menu, checked states, the voice channel at 375 (Dark) and 1280 (Editorial), a text channel 375 (Light), Overview base and after, Compare base and after (end), the Admin Limits 1280 end pair. Round 2 shots (`after2/`, 14): Members 375 Light opened, the others of the 14 are the same views and were not each opened. Not opened one by one: the rest of the 112 (the other theme of each pair, the mid/end shots of Overview and Compare, the other Admin pages: those are byte-identical to the base's, compared by `cmp`).

## Proposed inventory
- **SEC-AUD-360** (D-864): Members and Invites are stacked rows on a phone: table scroll 0, every row control inside the viewport and 44 px by hit test, click-through done. Closes wave 83's SEC-AUD-350 entries "Members, Invites actions off-screen", "Members' checkboxes and link".
- **SEC-AUD-361**: the voice title, the sliders' accent, `.ss-select` 44 px on a phone, the Compare first column, the `.ss-root` comment. Closes SEC-AUD-350's entries "voice title", "sliders' colour", "`.ss-select` at 40". Stays open there: the Modules tab's cut Discard (admin builder, D-867) and the kit's small controls.

## Changelog sentences
- 360: On a phone a server's Members and Invites lists are stacked rows with the name first and every action (Roles, the menu, Copy link, Revoke) on the screen and 44 px tall.
- 361: A voice channel's settings title shows the speaker instead of a hash, its sliders use the accent colour, a server's selects are 44 px tall on a phone and Compare levels keeps its label column readable.

## Owner questions
1. Decided by default: Invites' strong line is the inviter (no label) because it is the first column; the code could lead instead.
2. A role chip is not a control; if chips should become filters later they need 44 px targets.
3. D-865 says Admin selects alike; none of the three pages uses `.ss-select`. Admin's own selects (`admin.css`, `.ss-root .ss-toolbar select:not([class])`) were not measured here: they belong to the admin builder.

## Round 2
After round 1 I opened the Members shot and saw the caption ("2 members") wrap into a narrow column (a table caption inside the block table): fixed with `.ss-table-stack caption { display: block }` (commit "Members caption as a block line"). I re-ran only the people and click parts (`after2/`, 39 s): same figures as round 1 (table over 0, 12 of 12 controls pass, click-through the same, menu at (8, 559)), caption on one line (opened, Light), 1280 Members and Invites byte-identical to the base again.
**A fault of mine, to report: I ran round 2 WITHOUT holding the lock.** My `heavy.sh acquire guild` (started 13:34 in the background) answered 75 (busy: lounge since 13:33); I read the wrong line of its output as "acquired" and started my stack and one Playwright run (about 1.5 minutes, 13:43 to 13:44, ports 8625/5625/9325) while lounge held the lock. I stopped it as soon as I saw the owner file, ran nothing else, and have not taken the lock since (nothing more of mine needs it). Round 1 was inside a real lock (13:27 to 13:33).

## Checks (at the end)
web typecheck clean; eslint clean on Members, Invites, Channels and the channels test; prettier clean on the sheet, the three `.tsx`, the test and the spec; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; `node scripts/check-css-tokens.mjs` clean (73 files); `node scripts/check-doc-links.mjs` ok; e2e `tsc` clean; the spec passed on the base, after, and after2 (`--project=chromium --workers=1 --retries=0 --reporter=line`; 1.2, 1.5 and 0.7 minutes).
No docs changed (nothing in `docs/` states the lists' layout, the title or the sliders). No new i18n key, no fragment. No `git stash` used or touched (the list is empty).

## Cleanup
Stack stopped by PID and by port (8625, 5625, 9325 checked free); scratch database `sr_p84_guild_ui` dropped; `p84-guild-bin` deleted (`p80-base-bin` untouched); lock: not held (released after round 1; round 2 ran without it, see above). Server and Vite ran from the worktree root. `e2e/.state/sweep` is git-ignored and now points at a dropped database. Shots and scratch (`scratch/p84-guild`) uncommitted.
