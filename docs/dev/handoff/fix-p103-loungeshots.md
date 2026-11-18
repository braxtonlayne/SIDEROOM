# fix-p103-loungeshots (wave 103, shots builder of pod lounge)

## Clock
11:55 start. Spec first draft committed 12:00. Binaries copied at once. Slot taken 12:14 (queued from 12:00). Round 1 (base + after in one round, one database) 12:14 to 12:19. Both code branches were ALL READY at 12:03 and were merged into this branch at 12:09 (commander's word); the base phase ran on the pod's six app files put back to `49ca8946`'s text (`git checkout 49ca8946 -- <files>`, then `git checkout HEAD -- <files>`; no stash; `git status` clean afterwards). The spec changed after the round (label-gap check, see "Spec change"): it has not run since.

## STACK-ROUND
- Round 1: `acquire` 12:14 (exit 0; `status` showed `loungeshots 12:14` beside another builder), stack from `p103-loungeshots-bin`, ports 8811/5811/9511, `SR_ADMIN_REQUIRE_REAUTH=0`, `rm -rf e2e/.state`, sweep seed exit 0 with 0 `FAIL` lines, base spec exit 0 (2.0 min, 0 problems), after spec exit 1 (2 min, 3 problems, all one fault, see FAULTS), stack stopped by PID tree, ports 8811/5811/9511 free, `release` 12:19. `status` after release: `toolshots 12:14` (no hold in my name).
- Round 2: not yet (waiting for cures).

## TYPECHECK
`tc.sh run loungeshots bash -c "cd .../p103-loungeshots/apps/web && npx tsc --noEmit -p ."` 12:21, exit 0 (merged tree). e2e `npx tsc --noEmit -p tsconfig.json` exit 0 after the last spec edit (12:20). prettier on the spec: clean. eslint: not applicable (the repo has no eslint config for `e2e/`).

## What was measured (9 stops x 3 configs: 1024 Lounge Light, 1024 Lounge Dark, 375 touch Lounge Light)
Data made through the API: a forum channel `p103l-forum-<digits>`, two applications `p103l-app-<digits>` and `p103l-app2-<digits>`; all removed in a `finally`. The Scrapbook was switched on and off. Fakes by `page.route`: the bridge (status, conflicts, features) and two storage mounts for the shelf picker (the world has none), the same in both phases. No stop was "not reached" in either phase. data-theme read back each time (all as wanted; the public authorize page also showed the main theme).

| stop | file | builder | moved | base box at 1024 (w x h) | after | verdict |
|---|---|---|---|---|---|---|
| forum Sort | lounge/forum/ForumView.tsx | forum | yes, fit content | 109 x 19 (raw, 13.33 px) | 162 x 40 (16 px) | FAULT 1: 53 px wider than base (limit 36) |
| emoji skin tone | lounge/components/EmojiPicker.tsx | forum | yes | 41 x 19 | 72 x 40 (44 at 375) | ok: swatch clear of chevron; search field 185 px before and after at 1024 and 375; 72 px, not the planned 60 |
| shelf picker | lounge/scrapbook/ShelfPanel.tsx | forum | yes | 153 x 32 | 165 x 40 | ok: "Use this shelf" stays on the picker's line (moved 12 px sideways only); controls below moved 4 px down, 8 px of height gained is within the allowance; no 48 px shift |
| Gallery filters (3 selects, same `.lng-scr-actions` family) | lounge/scrapbook/Gallery.tsx | dependant | no | 128 x 40 each | 128 x 40 each, y +4 (below the taller shelf row) | ok, equal |
| Conflict log Kind | settings/server/bridge/ConflictLog.tsx | pages | yes | 246 x 40 (14 px bold) | 222 x 40 (12 px) | numbers pass, but FAULT 2 (label touches the select) |
| API reference Area | developers/Reference.tsx | pages | yes | 182 x 40 | 186 x 40 | ok (+4); one line with the search field at 1024, wraps below at 375 as in base |
| Commands Type, Option type | developers/tabs/Commands.tsx | dependant | no | full width | equal (boxes, fonts, controls) | ok |
| OAuth2 Redirect URI | developers/tabs/OAuth2.tsx | dependant | no | full width | equal | ok |
| Rich Presence asset Type | developers/tabs/Media.tsx | dependant | no | 132 x 40 | equal | ok |
| authorize "Add to server" | developers/Authorize.tsx (public) | dependant | no | 293 x 44 at 375 | equal in every box and font | ok: does not change at all |

Block size 40 at 1024 and 44 at 375 for every moved select; no document scrolled sideways at any size (`scrollWidth` equals `clientWidth`); no overlaps. Hit test: same rows as the base for every stop, none worse. At 1024 the fails (28 to 40 px controls) are the base's own: noted, SEC-AUD-467, not faults; at 375 forum shows 3 fails and emoji 1, equal to the base. Keyboard (1024 Light): Sort activity/created, tone 0/1, shelf ""/mount-one, Kind ""/permission_reverted, Area ""/accounts: ArrowDown changed the value, :focus-visible matched, ArrowUp put it back, the page reacted.

## FAULTS
1. **forum, `apps/web/src/lounge/forum/ForumView.tsx`, the Sort select.** At 1024 Light, 1024 Dark and 375: inline size 162 against the base's 109 (+53, limit +36); height 40 against 19 (the toolbar row grew 12 px at 1024 and 16 px at 375; the two view buttons moved 53 px right). The kit select's font is 16 px (`font: inherit`); the base raw select used the browser's 13.33 px, so it reads larger than its small muted label and the search field (D-974 type-size kind). A `font-size` on `.lng-forum-select` would go in `forum.css`, which is nobody's row. **Cure asked: (a) the Sort goes back to the base's raw `<select>` text** (its `forum.css` rule is untouched, so it is exactly the base). Accepting the wider Sort instead is an owner decision (question below); I edited nothing.
2. **pages, `apps/web/src/settings/server/bridge/ConflictLog.tsx`, the Kind select (seen in the pictures; the spec now measures it).** After the move the label text "SHOW" touches the select (about 1 px by the picture, base 9 px) and the select font fell from 14 px bold to 12 px (the label's small uppercase font is inherited). The base's `.ss-select` rule gave the gap and the size; `.ss-field-label` has no gap. **Cure asked: (a) back to the base's raw select with `className="ss-select"`** (members' sheet is untouched, so it is the base), unless members' block gives `.ss-field-label` a gap and a font size (b; members owns `server-settings.css`: I name it, I do not decide it). Wave 102 put this select back for the same label trouble: the stacking is gone this time, the crowding is not.
No fault of `fit="content"` itself (kit): for every moved select the selected text ends clear of the chevron.

## Notes (not rules)
- Emoji tone is 72 px (12 padding + 22 swatch + 36 padding + borders), not the planned 60.
- Sort and tone font grows 13.33 to 16 px; Kind shrinks 14 to 12 px.
- Area 4 px wider (186 vs 182); Gallery rows sit 4 px lower with the taller shelf row.

## STALE
None known. `p102-pages-shots.spec.ts:534` lists `select.dev-select` beside plain `select`; it matches nothing new. I edited no earlier spec.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-lounge`: 27 base shots, 27 after shots, 27 `cmp-*.png` side-by-side pictures (base left, after right). I opened all 27 comparison pictures, one line each:
- forum-sort 1024 Light, Dark, 375: Sort went from a small raw box to the kit field with a chevron, clearly larger than its label; toolbar taller; view buttons moved right (FAULT 1).
- emoji-tone 1024 Light, Dark, 375: skin tone is a kit field with chevron, 72 px; search field unchanged; picker 8 px taller.
- scrapbook-shelf 1024 Light, Dark, 375: picker and "Use this shelf" on one line in all three; panel 4 px taller; nothing below jumps.
- bridge-conflicts 1024 Light, Dark, 375: Kind narrower, text smaller, "SHOW" touches the field; the "Include resolved" switch moved left 38.6 px at 1024 (follows the narrower select) and stays on its own line at 375 as in base (FAULT 2).
- api-reference-area 1024 Light, Dark, 375: search field and Area on one line at 1024, Area below at 375 as in base; the kit chevron replaces the browser's.
- commands-type, oauth2-redirect, richpresence-type, each at 1024 Light, Dark, 375 (9 pictures): base and after identical.
- authorize-guild 1024 Light, Dark, 375: identical (only the app's stamped name differs).

## The commander's command, whole (both phases; from `e2e/`, on a stack with the sweep's world; `DATABASE_URL` and `SR_CLI_BIN` set)
Base (on a tree where ForumView, EmojiPicker, ShelfPanel, Reference, ConflictLog and developers.css have the base's text):
`SR_E2E_BASE_URL=http://localhost:<vite> SR_P103_LOUNGE_ON=1 SR_P103_LOUNGE_PHASE=base SR_P103_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-lounge npx playwright test tests/p103-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`
After: the same without `SR_P103_LOUNGE_PHASE=base`.

## Spec change after round 1
Added `labelGap` (select left minus the label text's right edge) and the rule "label text touches the select". Round 1's base figures have no `labelGap` (the rule is skipped until the base is re-run). If a cure puts a select back, its `moved` entry in STOPS is set to `[]` (`forum-sort`, `bridge-conflicts`) before the last commit.

## Owner questions
- Forum Sort: is a Sort field 53 px wider and 21 px taller (kit look, 16 px type) acceptable on the forum toolbar, or does it stay raw until `forum.css` can set a type size?
- Conflict log Kind: does `.ss-field-label` get a gap and a font size (members' sheet), or does the select stay raw?

## Proposed rows
Only the two faults above (numbered by the commander from SEC-AUD-470). The 1024 hit-test fails equal to the base are SEC-AUD-467's.

## ROUND 2 (13:25 to 13:30) and FAULTS: final state
- Cures: forum put the Sort back to raw and pages put Kind back to raw with `ss-select` (both files equal the base's text). I merged both branches again (no conflicts) and set `moved: []` for `forum-sort` and `bridge-conflicts` in the spec.
- STACK-ROUND 2: `acquire` 13:25 (queued from 12:26; `status` showed `loungeshots 13:25`), fresh database, seed exit 0 with 0 FAIL, base phase (the pod's four still-changed app files put back to `49ca8946`'s text, then restored; no stash) exit 0, after phase exit 0 with `problems: []`, 39 verdicts and 0 broken, stack stopped by PID tree, ports free, `release` 13:30. `status` after release and cleanup: `free`. Database `sr_p103_loungeshots_ui` dropped, `p103-loungeshots-bin` deleted. e2e `tsc` exit 0 and prettier clean after the last spec edit (12:26).
- The re-run base has `labelGap` figures, so the label-gap rule ran: no select breaks it.
- **FAULT 1 (forum Sort): fixed by the put-back** (not a re-shot cure but an exact return to the base's text; the 375 comparison picture was opened: base and after identical, raw 13.33 px select, toolbar unchanged).
- **FAULT 2 (Kind): fixed by the put-back** (1024 comparison picture opened: base and after identical, "SHOW" label gap as in base).
- Moved selects still passing in round 2 (re-measured): emoji skin tone (72 px, search field 185 px at 1024 and 375, swatch clear of the chevron), shelf picker (165 x 40, "Use this shelf" on its line), API reference Area (186 x 40). Dependants (Gallery filters, Commands, OAuth2, Rich Presence, authorize page) equal to base. No FAULT is open. No kit fault: no select that should fill its column came out narrow.
