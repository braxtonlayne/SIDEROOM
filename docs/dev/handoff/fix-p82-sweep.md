# Wave 82 handoff: sweep (SEC-AUD-342, D-848)

T = 11:10 on 2026-11-16. Clock times: no new item from 13:10, checkpoint 13:30, finished 13:55, hard limit 14:10.
Actual: spec compiled (e2e `tsc`, prettier) 11:10; lock taken 11:42 (waited behind tools, developers, library and arcade); stack up 11:43; world seeded 11:43; walk 11:44 to 11:49 (5.5 minutes of running); stack stopped and lock released 11:50. One lock round; no second round (the table is empty). Cleanup 11:51.

## Done
- `e2e/tests/p82-sweep-shots.spec.ts` (new, local only: `SR_P82_SWEEP=1`, out `SR_P82_SWEEP_OUT`, `SR_P82_ONLY` = regex over route ids). No source file, style sheet or i18n fragment changed. It reuses the visual sweep's seeded world (`e2e/.state/sweep/world.json`, made by running `page-sweep.spec.ts -g "seed the sweep world"`; that spec is untouched) and its route list (copied, filtered), and walks the section lists of the member's Settings, a server's settings and Admin from the page (`[role=dialog] nav ul li > button`), clicking each. The Owner, main theme Lounge Dark (account settings PATCH, localStorage, read back: "lounge-dark"; `data-theme` is logged on every line). Phone = 375x812 with touch and a mobile user agent; desktop = 1280x800. Per route and size one JSON line in `measure.json`: `documentElement.scrollWidth - clientWidth`, `body.scrollWidth - clientWidth`, the same for the page's main scroller (the largest vertical scroller or `main`) and for every other big scroller, and, when any is over 0, the elements past the edge (deepest first) with a viewport shot. States added to the world: a wide table, a long code line and a long link, posted into the channel and the DM (the table renders as plain text, the code block scrolls inside itself; a phone and a desktop shot of the channel were opened to see that they exist).
- Result: **the table is empty.** No page of the signed-in app outside the rooms and the Developers pages scrolls sideways at 375 or at 1280: document and main scroller are 0 on every route measured.

## Count measured (document over 0 on none)
- **375: 109 routes and states. 1280: 110.**
- Lounge (14 each size): home, Friends, Message Requests, a DM (long word, long link, code, wide table), a server's text channel with messages (long unbroken word, code block, wide table text, attached picture and file, link preview, rich embed, thread starter), the channel with the member list open (phone: opened by a real swipe), with it closed, a thread, a thread with the member list, the inbox (Mentions, Unreads, For you), a message link, the scrapbook, a voice channel (no call).
- Member's Settings: all 20 tabs read from the page. Server settings: all 19 tabs the page lists (the sweep's own list has 20 ids), plus a text channel's Permissions tab and a voice channel's tab. Admin: all 34 pages of its navigation (the sweep's list has 26), plus the Review hub's All applications tab.
- Others: app door, OAuth2 authorize and authorized, Support, Support leaderboards, the profile page (`/u/owner`, `/~owner`, a member), the profile page editor, Help (3 pages), Share, a storage mount, an invite and a template inside the app, an unknown channel, an unknown address.
- **Not reached (1 at 375):** a thread's member list on a phone (`lounge-thread-members-open`): the Member List button resolved but could not be clicked in 5 s. Needs a look at how the phone thread header is built. The thread itself, and the channel with the member list open, were measured.
- Not in this walk by design: the Library, Cinema, Arcade, Study (waves 78-81), the Developers pages (the developers builder), the Lounge with a live voice call (its bar was measured by waves 79 to 81; a faked store is not allowed for shots, D-818; a real call was not set up), the Showroom banner, the public pages (signed out). Dialogs and popovers other than the inbox were not opened.

## The table
| route | size | px over | node | file and rule | fixed or proposed |
|---|---|---|---|---|---|
| (none) | | | | | |

Boxes that scroll sideways inside themselves (meant to, not faults; the document and `main` stay at 0), all at 375: Admin's `div.adm-table-wrap` (`admin.css` `.adm-table-wrap { overflow-x: auto }`) on Crash reports (415 px of scroll inside), Accounts (445), Cinema catalog (80), Pricing and perks (392), Platform audit log (290), Access log (181); and Server settings > Server Support's `table.ss-table.sup-compare` (53; the 720px block of `server-settings.css` makes `.ss-table` a block with `overflow-x: auto`). Left as they are.

Side finding (not scroll, not fixed): the Support pages and the in-app invite and template cards carry `data-theme` `lounge-editorial`, and Help carries `sideroom`, while the account's main theme is Lounge Dark (section themes; the visual sweep's O7 says the same for Help).

## Fixes
None: nothing to fix. No style sheet changed, so `check-css-tokens` was not run, no file's laziness was checked and no web build ran. For wave 83: `settings/user/prefs-effects.css` is imported by `settings/user/prefs.ts`, which the app's boot may load: a fix there would be first paint (build and the three checks).

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-sweep`: `measure.json` (220 lines: 219 measured, 1 not reached), `totals.json`, and 2 shots: `admin-31-platform-audit-log-phone.png` and `admin-33-access-log-phone.png`, taken only because the main-scroller pick chose the inner table wrapper. They are inner-wrapper scrolls, not page faults; I opened both: the page is whole and the table is cut at the right edge inside its own wrapper (meant). `measure.json` and the shots stay uncommitted.

## Tests and checks
The spec ran once and passed (`--project=chromium --workers=1 --retries=0`, 5.5 minutes). e2e `tsc` clean, prettier clean, `node scripts/i18n-audit.mjs --max 0` TOTAL 0. Not run: vitest, `check-css-tokens` (no style sheet changed), any build. No base comparison (no change was made).

## Proposed inventory row
SEC-AUD-342 (D-848): every signed-in page outside the rooms and the Developers pages measured for sideways scroll at 375 and 1280 (109 and 110 routes and states, Lounge Dark); none scrolls. **Close.** Not covered, for a later wave if wanted: a thread's member list on a phone, a live voice call's bar in the Lounge, the Showroom banner, popovers and dialogs other than the inbox.

## Changelog sentence
None (nothing changed for members).

## Owner questions
- Support and the in-app invite and template cards show `lounge-editorial`, and Help shows `sideroom`, when the member chose Lounge Dark as the main theme: intended as section themes? (Default taken: yes, left as is.)

## Cleanup
Stack stopped by PID (ports 8605 and 5605); lock released; scratch database `sr_p82_sweep_ui` dropped; `p82-sweep-bin` deleted (`p80-base-bin` untouched). No `git stash` used or touched. Server and Vite ran from the worktree root. The seeded state in `e2e/.state/sweep` is git-ignored and now points at a dropped database.
