# Wave 102, settings: SEC-AUD-451, 450, 452 (D-964)

Branch `fix/p102-settings`, three commits (451, 450, 452), 09:30 to 10:20. No Rust, no cargo, no web build, no new string, no migration.

## Verdicts
- **SEC-AUD-451: Result A (real).** At 1024 the content column `<main>` was wider than its box before any scroll call: `scrollWidth` 816 against `clientWidth` 737 (the Members table is 664 px in a 585 px column; with 30 long-named members 837 px, `main` 989 > 737). The chan spec's calls (`openCollapsed`, `hitTestAll`, then `scrollIntoView({block:'start'})`) moved `main.scrollLeft` to 79, which put "Members" and the search field under the sidebar. Not a spec scroll: the column can scroll sideways because the table overflows it. At 900 the column overflowed too (816 > 648) with no call at all. At 1280 nothing overflowed (table 828 in 828).
- **SEC-AUD-450: real.** Base at 1024 and 1280: the names "Everyone", "All channels", "#general" had `.ss-name` boxes 0 px wide (`scrollWidth` 73, 94, 69 against `clientWidth` 0); the select was 748 px (660 px beside Remove), because the kit's wrapper is 100 % wide. At 375 the rows wrap (name 303 px, select 303 px).
- **SEC-AUD-452: real.** Base at 375 touch: "Select all members on this page" fails; its centre and its below point are taken by `td < tr < tbody < table` (the stacked table's head is clipped to 1 px, so the checkbox has no box of its own); the row checkboxes passed.

## 451 figures (base then after; `figures-base.json`, `figures-after.json` in `visual-sweep-shots/p102-settings/`)
Format `scrollLeft/scrollWidth/clientWidth (overflow-x)`. Ancestors above `main` (`div._main`, `div._root`, `div.ss-root`, `div`, `body`, `html`) were 0/w/w (visible) in every run, `html` 0/1024/1024 at 1024.

| Run | Step | `main` base | `main` after | Table / wrapper after |
|---|---|---|---|---|
| 1024 Light, seed (2 members) | 1 navigation only | 0/816/737 (auto) | 0/737/737 | table 662, wrapper 585 (scrollWidth 662, scrollLeft 0) |
| 1024 Light | 2 chan spec's calls | **79**/816/737 | 0/737/737 | wrapper scrollLeft 77 (its own scroll) |
| 1024 Light, large (30 members, 32-character names, five roles each) | 1 | 0/989/737 | 0/737/737 | table 837, wrapper 585 |
| 1024 Light, large | 2 | **54**/989/737 | 0/737/737 | wrapper scrollLeft 252 |
| 1024 Dark | 1 / 2 | 0/816/737 ; **79**/816/737 | 0/737/737 both | as Light |
| 900 Light | 1 / 2 | 0/816/648 both | 0/648/648 both | table 662, wrapper 496 |
| 1280 Light | 1 / 2 | 0/980/980 both | 0/980/980 both | table 828 = wrapper 828 |
| 375 Light (and large) | 1 / 2 | 0/375/375 | 0/375/375 | no wrapper scroll (stacked rows) |

Sidebar after: x 0, w 286.7 (1024), 252 (900), 300 (1280); the content starts at the sidebar's end; "Members", the search field and the Name column start at x 326.7 (1024), 292 (900), 340 (1280): none under the sidebar (the base run of round 1 could not read the sidebar box; the figures above are round 2's and 3's). Filter row at 1024, 900 and 1280: the four filter fields keep their base boxes exactly (role 314x61, dates 169x61, sort 206x61; they already wrapped as whole fields); toolbar `scrollWidth` = `clientWidth` in every run. 375: toolbar box 343x357 at y 156 equal to base; the first row moved from y 553 to 597 (44 px), which is the new head checkbox row (a 44 px head cell); x equal.

**The shot caveat.** The chan spec's `hitTestAll` scrolls controls to the middle of the viewport, and so it scrolls the table's own wrapper (scrollLeft 77 at 1024), which cuts the Name column and the avatar on the left. `members-1024-scrolled.png` shows that state; my named shots reset the wrapper to 0 before they are taken. The re-shoot of `p101-chan-shots` may therefore show the table scrolled by about 77 px, but never the page.

## The diff in words
- `Members.tsx`: the `<table>` sits in `<div className="ss-table-scroll">` (reindent only otherwise).
- `BotCommandPermissions.tsx`: both rule `<li>` get `ss-rule-row` beside `ss-row`.
- `server-settings.css`, three blocks at the end (no rule above edited), each naming its row and D-964:
  - 451: above 720 px `.ss-table-scroll { max-inline-size: 100%; overflow-x: auto }`.
  - 450: above 720 px a `.ss-rule-row` keeps `12ch` for its name column (`flex: 1 1 12ch; min-inline-size: 12ch`) and the select's wrapper takes `flex: 0 1 220px` with a floor of 160 px. 375 is unchanged.
  - 452: at 720 px and under, the stacked table's head with a checkbox in it (`thead:has(th input[type='checkbox'])`) is shown (static, block) as one 44 px line above the rows; its other head cells stay clipped for a screen reader. Only Members' table has a checkbox in its head; Invites and Support compare are untouched.
- `e2e/tests/p102-settings-shots.spec.ts` (new; in the 451 commit).

## 450 after (1024 and 1280 identical)
Names 528 px, 528, 440 (`scrollWidth` = `clientWidth`: whole), meta "Channel" whole, selects 220 px (at least 160), Remove 75.9 px (equal to base). 375: every `name`, `select` and `li` box equals the base within 1 px (the spec asserts it).

## 452 after
375 touch, seed and large: "Select all members on this page" passes (centre, above, below); the row checkboxes still pass; hit test over `main` seed 16 pass / 0 fail (base 15 / 1), large 109 pass / 0 fail (base 108 / 1). At 1024 (not a touch width) the same hit test reports 16 fail in base and after alike (the 44 px rule is a phone rule; the takers are table cells); not a change.

## Who imports `server-settings.css`
`apps/web/src/settings/server/ui.tsx` (the Server settings chunk, route `routes/settingsServer.tsx` lazy) and `apps/web/src/admin/ui.tsx` (the Admin chunk, `StaffDoor.tsx` lazy). Neither is first paint nor a public page. No web build run (lounge's).

## STACK-ROUND
- Round 1: 09:35 to 09:38, slot `settings 09:35`; base files put back from a scratch copy, then after. The after run failed on two spec errors (sidebar not found, base sidebar null); my stack was left up by a wrong kill, I killed it by PID and released.
- Round 2: 10:06 to 10:08, slot 2 (`library 10:06 / settings 10:06`), base then after on one database, fresh seed; the after run found two more spec errors (the sidebar test at 375, the head height), fixed in the spec.
- Round 3: 10:12 to 10:17, slot (`settings 10:12`), fresh seed, `p102-settings-shots` after: **1 passed (34 s), `problems: []`**; then `p101-chan-shots`: 1 passed (1.4 m); `p101-mod-shots`: 7 passed (1.4 m) (both with no base, writing to `p102-settings/p101-chan` and `p101-mod`); stack stopped by port, ports checked free, DB dropped, released.
- The lock was busy from 09:38 to 10:06 (both slots held by others); I waited in foreground `acquire` calls (the harness moved them to the background after 180 s; each ended by itself, rc 75 or 0, none left alive).
- None of the three specs registers an account (owner state, a faked member list and a faked bot).

## TYPECHECK
`bash cli-agents/tc.sh run settings bash -c "cd C:/Users/braxt/Desktop/SIDEROOM_wt/p102-settings/apps/web && npx tsc --noEmit -p ."`, 10:17 to 10:18, exit 0 (no `.tsx` edited since; the spec was edited before it, e2e `tsc` exit 0 at 10:18).

## Other checks (after the last edit)
prettier --check on the 4 files: clean; eslint on the two `.tsx`: clean; e2e `tsc --noEmit`: exit 0; `node scripts/check-css-tokens.mjs`: clean; `node scripts/i18n-audit.mjs --max 0`: TOTAL 0; `node scripts/check-doc-links.mjs`: ok. Unit tests, unedited: `people.test.tsx` (Members, 17 passed), `p14communityMore.test.tsx` (25 passed). The base was run before the change in the same rounds: the faults of 451, 450 and 452 show in `figures-base.json` and `base-*.png`.

## Shots (`visual-sweep-shots/p102-settings/`; I opened every one named in the task)
Opened: `p101-chan/base-members-1024-lounge-light.png`, `p101-mod/base-bot-permissions-1024-lounge-light.png`; `members-1024-lounge-light`, `members-1024-lounge-dark`, `members-900-lounge-light`, `members-375-lounge-light`, `members-1024-lounge-light-large`, `base-members-1024-scrolled`, `base-permissions-1024-lounge-light`, `permissions-1024-lounge-light`, `permissions-375-lounge-light` (11). On disk and not opened: `members-1024-scrolled`, the `-nav` shots, `members-1280`, the base 375/900/1280 shots, the p101-chan and p101-mod after shots (their specs assert them).

## The commander's command
Seed the sweep world on a stack (API 8781, Vite 5781, `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL`, `SR_CLI_BIN` set), then from `e2e/`:
```
SR_E2E_BASE_URL=http://localhost:5781 SR_P102_SETTINGS_ON=1 \
SR_P102_SETTINGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-settings \
npx playwright test tests/p102-settings-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
Delete `figures-base.json` in the out folder first on the merged tree (there is no base then).

## Closing texts
- SEC-AUD-451: closed. At 1024 and 900 the Members table (664 px) was wider than the content column (585 px), so the column scrolled sideways and a scroll call left the heading and search under the sidebar. The table now scrolls in its own wrapper above 720 px; the column's `scrollWidth` equals its `clientWidth` at 900, 1024 and 1280 in Light and Dark and with 30 long-named members; `scrollLeft` is 0 after the chan spec's calls.
- SEC-AUD-450: closed. The rule rows of Command permissions gave the name 0 px at 1024 and 1280; the name keeps 12ch and the select 220 px (at least 160); names and "Channel" are whole; 375 unchanged.
- SEC-AUD-452: closed. The Members head checkbox is a 44 px line of its own at 375; it passes centre, above and below with the row checkboxes still passing.

## Proposed rows
- SEC-AUD-455 (low): at 1024 the Members table is wider than its 585 px column (nowrap role chips, signals and Actions buttons, XC-VIS-002), so the table now scrolls inside its box and the Actions column starts cut off at the right edge ("ACTI..."). A narrower table (the role chips wrapping, or Joined via and Signals merged) would show Actions without a scroll. Not done: it changes rules above my block.
- (low) At 375 the "select all" checkbox has no visible label (its name is the aria-label): a sighted user sees an unlabelled box above the first row.

## Changelog sentence
Server settings: Members sits beside the sidebar at tablet width (its table scrolls in its own box), the Command permissions rules show their names, and the Members "select all" box is a full touch target on a phone.

## Owner questions
- Should the Members table wrap its chips or drop a column at 1024 so the Actions column shows without the table's own scroll? (default: left as is)
- Should the phone's "select all" checkbox get a visible label (the existing `settings.members.selectAll` string; no new string)? (default: no)

## Clean-up and lock
Scratch database `sr_p102_settings_ui` dropped (listing of `sr_p102_settings%` empty); `p102-settings-bin` deleted; the stack stopped (ports 8781, 5781, 9481 free). No `git stash` used. `heavy.sh status` at 10:19 (my last check): `cmd 10:17`, no hold in my name (I released at 10:17); no `acquire` of mine is alive.
