# fix-p107-lists (wave 107, lounge B code builder: lists, user panel, members)

## Clock
Started 18:37, all three sheets READY 18:41 (clock of `date`). No jobs (code builder).

## Done (commits)
- `D-1022 apps/web/src/lounge/lounge.css (READY)`, `D-1022 apps/web/src/lounge/shell/shell.css (READY)`, `D-1022 apps/web/src/lounge/rings/rings.css (ALL READY)`.
- Every rule is new, phone only (`.lng-root[data-mobile]`, or the sheet's own `max-width: 720px` block for the Friends header), placed directly after the rule it amends.

## Not done
- "Add a ring" (`.lng-icon-btn` in `.rng-tray-item`, rings.css, 28 px, directly under the 40 px ring avatar with a 4 px gap): not named in my task; no room for A.1 (it would take the ring's point), needs A.2 (`min-block-size: 44px`) in a `.rng-tray-item` rule. Not mine: flagged for the commander.
- Set channel status and Mute the music (voice page): not mine.
- The channel row's own hover actions (`.lng-channel-actions` invite/edit icon buttons, 28 px): not in the task, untouched.

## REACH table
| control | rule now | page / how opened | cure | will move |
|---|---|---|---|---|
| `.lng-channel` | lounge.css `.lng-channel` min-height 32 + margin-top 2 | server channel list, phone list pane | A.2 `min-block-size: 44px` | row 32 -> 44, text and icons not |
| `.lng-category-btn` | `.lng-category-btn` (no height, 16 px) | channel list category head | A.2 `min-block-size: 44px` | head 16 -> 44 (28 px more) |
| category `+` (`.lng-category-head .lng-icon-btn`) | `.lng-icon-btn` 28x28 | same | A.1 `::after` -8 px, inside the 44 px head | nothing |
| `.lng-find` | `.lng-find` height 28 | Home list top (and server list top) | A.2 `height: 44px` | 28 -> 44 |
| `.lng-nav-item` | `.lng-nav-item` height 42 | Home list (Friends, Message Requests) | A.2 `height: 44px` | +2 each |
| Create DM `+` (`.lng-dm-head .lng-icon-btn`) | `.lng-icon-btn` 28 | Home list | A.1 `::after` -8 px, plus `padding-block-end: 8px` on `.lng-dm-head` (was 4) so the area does not take the first conversation's top edge | DM list down 4 px |
| `.rng-btn` | rings.css `.rng-btn` (40 px ring avatar button) | ring tray, Home list, only where interactive | A.1 `::after` -2 px (not A.2: a taller button would turn the round ring into an oval) | nothing |
| user panel Mute, Deafen, Settings, 2 chevrons | `.lng-icon-btn` 28 (chevron `.lng-device-arrow` 14 wide), panel 52 px | bottom of the list pane | A.1 `::after` -8 px, inline 0 (chevron width unchanged) | nothing (panel stays 52) |
| `.lng-user-avatar` | shell.css `.lng-user-avatar` 32 | user panel | A.1 `::after` -6 px | nothing |
| `.lng-user-id` | lounge.css `.lng-user-id` 36 | user panel | A.1 `::after` -4 px | nothing |
| `.lng-member` | `.lng-member` height 42 | member sheet (More actions > members) | A.2 `height: 44px` | +2 each |
| Friends header (Back, New Group DM) | `.lng-friends .lng-header` wrap, row 1 = 28 px button | Friends page on a phone | A.2: the first line is 44 px (`.lng-friends .lng-header-title` min-block-size and line-height 44px, in the existing 720 px block); the headers' own 44 px `::after` (lounge.css, heads') then stays inside it | tabs row and list down about 16 px (row 1 28 -> 44) |

Dependants of each changed rule (`git grep`): `.lng-header-title` in the Friends block is reached only through `.lng-friends`; `.lng-dm-head` has one user (HomeSidebar); `.lng-icon-btn` rules are scoped by `.lng-category-head`, `.lng-dm-head`, `.lng-user-panel`, so no other icon button is touched; `.rng-btn` has one other sheet rule (`.pp-body .rng-btn`, profilepage.css:283, own `::after`, scope differs because it needs `.lng-root[data-mobile]` which the profile page lacks inside).

## FINDERS (rule 13)
Classes touched, searched in `e2e/` and `apps/web/src`: `.lng-channel`, `.lng-category-btn`, `.lng-find`, `.lng-nav-item`, `.lng-member`, `.lng-user-id`, `.lng-user-avatar`, `.rng-btn`, `.lng-dm-head`, `.lng-friends`.
- Only locators and a text-reading test: `e2e/tests/p75-polish-shots.spec.ts:291` (`.lng-user-id`), `p83-lounge-shots.spec.ts:565` (`.lng-find`), `.lng-member` locators in p68/p69/p83/p84/p87/p88/p90/p93/p94/p95 (retired shots specs, not edited), `p106-toucha-shots.spec.ts:557` (`section.lng-friends`). None calls a control I replaced or changes tag/role/label; no control was replaced.
- Tests reading my files as text: `rings/__tests__/rings.test.tsx` (reads rings.css: no `width:`/`height:`/left/right; I used logical properties only; passes), `__tests__/sectionBarVoice.test.tsx` (reads lounge.css, passes), `styles/__tests__/textLink.test.ts` (reads sheet colour tokens, none added). Roving tests (`roving.test.tsx`) pass.
- Fix needed in CI specs: none.

## Figures (not measured: no shots by a code builder; listshots measures)
Expected, phone only: rows 44 tall; category head 44; Friends header first line 44; user panel 52 unchanged; 1024 and 1280 unchanged (no rule applies without `data-mobile` / max-width 720).

## Who imports
lounge.css, shell.css: imported by the Lounge shell (lazy lounge chunk); rings.css by `rings/*.tsx`. No first-paint or public page file changed.

## Checks (own tree, last commit)
- prettier --check on the 3 files: ok. `node scripts/check-css-tokens.mjs`: ok (exit 0).
- vitest single files (18:39-18:40): rings.test.tsx 11 passed, sectionBarVoice.test.tsx 3 passed, roving.test.tsx 3 passed.
- web tsc, build, shots: not run (code builder).

## Closing text / proposed rows
Row K (lists, members), B (user panel), D (the two `+`), C (Friends header): cured on a phone. Proposed new row: "Add a ring" button in the ring tray (28 px, taker the ring avatar above): A.2 in a `.rng-tray-item` rule.

## Changelog sentence
On a phone, channel, Home and member rows, the find box and the Friends header are 44 px tall, and the user panel's buttons have 44 px tap areas.

## Owner questions
None.

## PROCESSES
strays: none found
