# Wave 93 lounge: SEC-AUD-408 (D-924)

Builder: lounge. Started 12:54 (before T = 13:15, the lock was free); one stack round 12:58 to 13:02 (lock `lounge`, released 13:02); handoff 13:03. Web only, one CSS file and one spec: no cargo, no web build, no vitest.

## Base (ran it: the sheet as the base has it, same DB, same people)
Seeded world with no `FAIL` line. 375 touch, Lounge Light and Dark (same figures), popout opened from a member's name in the group DM panel and from a guild channel's member list (a second member, `dan`, joined the sweep guild through an invite; role chips are drawn there). `openCollapsed` opened 0; `hitTestAll` on `.lng-pop`:
- Close 44 x 44 pass. Note 308 x 33 FAIL (above: the "Note" heading `h3.lng-pop-h`; below: `div.lng-pop-body`). Message 308 x 33 FAIL (both points: `div.lng-pop-body`). "View Full Profile" 120.7 x 20 FAIL (above: the Message input; below: `div.lng-pop-body`).
- Guild popout also: "Add role" 34 x 22 FAIL (above: the "Roles" heading, below: `div.lng-pop-body`). Not widened: a proposed row.
- Popout box 342 x 316 (group), 342 x 445 (guild); bottom inside the viewport; does not scroll inside itself; no sideways scroll.
- 1280 Editorial (guild popout): 342 x 445, Note 308 x 33, Message 308 x 33, View Full 120.7 x 20, Add role 34 x 22 (the hit test is a phone rule; recorded as the base for the equal-box check).

## Rules as built (`profile-p11.css`, one `@media (max-width: 768px)` block at the end, comment names SEC-AUD-408 and D-924)
- `.lng-pop .lng-pop-input { min-block-size: 44px }`
- `.lng-pop .lng-pop-full { display: inline-flex; align-items: center; min-block-size: 44px }`
No rule on a bare `.lng-pop-input` or `.lng-link-btn`.

## After (375 Lounge Light and Dark)
- Note 308 x 44, Message 308 x 44, View Full 120.7 x 44, Close 44: all pass, 0 fail, 0 not reached.
- Popout height: +46 px (group 316 -> 362, guild 445 -> 491). Bottom inside the viewport (top 129 / 157), so it does not need to scroll; it still scrolls inside itself when taller than the screen (`profile.css`: `max-height: calc(100vh - 16px)`, `overflow: auto`). No sideways scroll (0).
- Flow (group popout, Light): a note typed, a message typed, "View Full Profile" opened the full profile (a dialog).
- 1280 Editorial: every control box and the popout box equal to the base (0 px, asserted by the spec).
- Wave 92's one-to-one card (More actions 44, mutual 273.5 x 44, View Full 120.7 x 44) and the group panel (8 controls): all pass, boxes equal to the base (asserted).
- **The CSS in my commit is the CSS I measured** (applied after the base run, nothing changed after the after run).

## Proposed rows
- SEC-AUD-408: closed (D-924): popout Note and Message fields 33 -> 44 px and "View Full Profile" 20 -> 44 px on a phone, `profile-p11.css`; popout +46 px. Handoff `fix-p93-lounge.md`.
- New fault (number from SEC-AUD-426): the guild popout's role chip "Add role" button is 34 x 22 and fails the hit test at 375 (above: the "Roles" heading, below: `div.lng-pop-body`). A role's remove button (`.lng-pop-role-x`) was not drawn (the member has no roles); not measured. Rules for `.lng-pop-role-add` / `.lng-pop-role-x` are in `lounge/profile/profile.css` (not mine); a fix would be in `profile-p11.css` under `.lng-pop`. The 1280 box is the same (phone rule only).

## Who imports the file
`git grep -n profile-p11`: only `lounge/profile/ProfilePopout.tsx:20` (the Lounge chunk, lazy). No first-paint or public file touched, no import added, no web build.

## Spec and checks
`e2e/tests/p93-lounge-shots.spec.ts` (new; variables `SR_P93_LOUNGE_ON`, `_BASE`, `_OUT`; header has the command). Tests `popout` and `regression`; shared hit test only (`openCollapsed`, `hitTestAll`). After run: 2 passed. Watched fail without the change: the base run's figures above (the assertions are off in the base case, so it "failed" as figures: 4 fails on the group popout, 5 on the guild one). Spec errors fixed in my own round (a first spec looked for "Member List" on the guild header, it is under "More actions"). e2e `tsc`, prettier, `check-css-tokens`, `check-doc-links`: clean.

## Shots (`visual-sweep-shots/p93-lounge`, 5 of the task: all opened by me)
`base-popout-phone-lounge-light`, `popout-phone-lounge-light`, `popout-phone-lounge-dark`, `popout-desktop-lounge-editorial`, `card-phone-lounge-light`. Extra (not opened, figures in `measure.json`): `base-popout-phone-lounge-dark`, `base-popout-desktop-lounge-editorial`, `base-card-phone-lounge-light`, `base-popout-guild-phone-light-failed` (a spec error shot). What I saw: the after popout has a bit more air under the Note and Message and above the link, nothing cut off, the link is an underlined text with a taller box; Dark the same; 1280 unchanged.

## Commander's command
In the header of the spec (`SR_E2E_BASE_URL=http://localhost:5713 SR_P93_LOUNGE_ON=1 SR_P93_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-lounge npx playwright test tests/p93-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`, with `SR_CLI_BIN`, `SR_DATABASE__URL`, `DATABASE_URL`; delete `ids.json` on a fresh database).

## Docs
Searched `docs/` for these classes: no doc states the popout's field sizes. Nothing to update.

## Changelog sentence
On a phone the profile popout's Note and Message fields and "View Full Profile" link now have 44 px touch targets.

## Owner questions
None.

## Cleanup
Stack stopped by PID tree (8713, 5713, 9413 free), `sr_p93_lounge_ui` dropped, lock released 13:02, `p93-lounge-bin` deleted (`p92-base-bin` and `p80-base-bin` untouched), `p93-lounge-scratch` (server and Vite logs) deleted. No stash touched.
