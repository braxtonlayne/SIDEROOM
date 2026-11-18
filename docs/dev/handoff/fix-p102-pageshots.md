# fix-p102-pageshots (shots builder of pod pages; SEC-AUD-453, D-958)

Spec: `e2e/tests/p102-pages-shots.spec.ts` (new, the only file besides this handoff). No app file, sheet or other spec touched.
Clock: started 09:30; spec written 09:38; lock slot `pageshots` 09:44 to 09:59 (status at acquire: `pageshots 09:44`, `shade 09:38`); released 09:59, `status` then `arcadm 09:59` (no hold of mine).
Merged at 09:42 (the commander said to merge at once): `fix/p102-srv`, `fix/p102-reader`, `fix/p102-devs`, no conflicts, 10 app files.

## STACK-ROUND (one round, base and after on one database `sr_p102_pageshots_ui`, ports 8798/5798/9498)
- 09:44 stack start and seed (no FAIL). 09:46 to 09:57 base phase: the ten app files put back to `b308d5e2`'s text with `git checkout b308d5e2 -- <files>` (the list is kept outside the repo), run, then `git checkout HEAD -- <files>` (never a stash). Several partial base re-runs fixed my own spec errors (webhook is under the Webhooks tab; the wizard switch needs a click, not check; the guild GET carries `?with_counts`; a useId in a select id changed the key; scopes picked by ancestor depth, which the wrapper span changes). 09:54 full after run, 09:58 after re-run of the stops whose scope I changed (base re-run for them first). Stack stopped by PID tree (15540, 47676), ports 8798/5798/9498 read free, database dropped. `heavy.sh release pageshots` printed `released`.
- The final after run listed 5 problem lines: 2 faults (below), the others were my rule (a 4 px sideways move of Media's Upload button, now a note) and Reader's 375 line, which is the Reader fault.

## TYPECHECK
`bash cli-agents/tc.sh run pageshots bash -c "cd .../p102-pageshots/apps/web && npx tsc --noEmit -p ."` 10:00:39 to 10:01:08, exit 0 (merged tree). e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: exit 0 (10:00, after the last spec edit); prettier --check on the spec: clean. eslint: not run: `apps/web/eslint.config.js` ignores `e2e/` ("File ignored because outside of base path") and the e2e package has no eslint.

## The table (box = inline x block, px; base -> after)
| builder | file | select | reached | 1024 | 375 touch | verdict |
|---|---|---|---|---|---|---|
| srv | PruneDialog.tsx | #ss-prune-days | yes | 360x40 -> same | 303x44 -> same | pass |
| srv | Integrations.tsx | #ss-webhook-channel | yes | 360x40 -> same | 303x44 -> same | pass |
| srv | CommunityWizard.tsx | #ss-wiz-rules, #ss-wiz-updates | yes (community and guild GET faked) | 520x40 -> same | 303x44 -> same | pass |
| srv | Community.tsx | #ss-rules-channel (and updates, safety) | yes | 560x40 -> same | 343x44 -> same | pass |
| srv | bridge/ConflictLog.tsx | "Show" kind filter | yes (bridge status whole_guild and conflicts faked) | 246x40 -> 222x40 | 246x44 -> 222x44 | **FAIL** |
| reader | reader/Reader.tsx | Chapter picker | yes (series with 2 chapters) | 220x32 -> 220x40 | 218x32 -> 115x44 | **FAIL at 375** |
| reader | SeriesPage.tsx | Edition | yes (2 editions) | 164x36 both | same | left raw by the builder, equal |
| reader | Rankings.tsx | Kind | yes | 108x36 both | same | left raw, equal |
| reader | Pages.tsx | Sort by | yes | 152x36 both | same | left raw, equal |
| reader | Lists.tsx | Who can see it (new and edit forms) | yes | 101x36 both | same | left raw, equal |
| devs | tabs/Commands.tsx | #cmd-type, #opt-type-0 | yes | 934x40 and 904x40 -> same | 309x40 -> 309x44, 279x40 -> 279x44 | pass |
| devs | tabs/OAuth2.tsx | Redirect URI | yes (app with 2 redirect URIs) | 934x40 -> same | 309x40 -> 309x44 | pass |
| devs | tabs/Media.tsx | Rich Presence asset Type | yes | 128x40 -> 132x40 | 128x40 -> 132x44 | pass (note 1) |
| devs | Reference.tsx | Area | yes | 182x40 both | same | left raw, equal |
| devs | Authorize.tsx | #dev-auth-guild | yes | 460x40 -> same | 293x40 -> 293x44 | pass |

Keyboard (1024, one select per file): focus-visible true, ArrowDown changed the value and the page reacted, ArrowUp put it back, for all 15 driven selects. Nothing saved.
No page scrolls sideways in any config. Hit test (`hitTestAll`) ran at the three touch configs and at each area's first desktop config: no row got worse and counts are equal, apart from the two faults' own layout. `reader-chapter` gave 6 "not reached" rows in both phases: a probe limit of my spec on the reader's chrome, not a page fault.

## FAULTS
1. **srv, `settings/server/bridge/ConflictLog.tsx`, the "Show" kind filter: put back to raw.** Base 246x40 beside its label text, with the "Include resolved" switch on the same line. After 222x40 (24 px narrower at 1024 Light, 1024 Dark and 375 Light). The wrapper span turns the shrink-to-fit flex label into a block: the label text stacks above the select (select y 49 -> 64, +15 px), the switch drops 15 px and moves 78.6 px left at 1024, both Dismiss buttons below move 15 px down (allowed 0), and the select text is smaller than in the base. srv's own note predicted it.
2. **reader, `library/reader/Reader.tsx`, Chapter picker: put back to raw.** At 1024 it is 220x32 -> 220x40 (same width, bar items stay put: fine). At 375 base 218x32 -> after 115x44: 103 px narrower (the label's max-width 220 now applies around a shrink-wrapped span), so Bookmark, Comments, Bookmarks and Reader settings all move 103 px left (294 -> 191 and so on). Inline size smaller than the base is the rule's fail.

No fault for srv (Prune, Webhook, Wizard, Community) or devs (all five moved selects). **Builders with entries: srv (1), reader (1). devs: none.** Put-back is the base's text, which needs no new run (no second after round taken).

## Notes
1. Spec rule I changed after the run (compare only): a neighbour moving sideways is now a note, not a failure (vertical moves and a narrower select stay failures). Without it Media's "Upload moved 4 px" at dev 375 was listed: the select is 4 px wider and 4 px taller and Upload follows, allowed.
2. At 375 the base dev selects were 40 px; after, 44 px. The neighbours move by exactly that.
3. `sr-cli staff appoint owner` printed an error (the Owner is already staff): harmless.
4. The four library selects (reader) and Reference's Area (devs) are raw in both phases and measured equal.

## Shots: 45 side-by-side pictures, all 45 opened (`visual-sweep-shots/p102-pages/cmp-<stop>-<config>.png`, base left, after right; the 90 single shots are `base-<stop>-<config>.png` and `<stop>-<config>.png`). One line each by stop (three configs each):
- prune: same dialog; kit chevron replaces the browser arrow, same box, text 4 px further in; reads in Light, Dark and at 375 (44 tall).
- webhook: same dialog and layout, chevron at the end, buttons in the same places.
- community-wizard (step 2): both channel selects same width, kit chevron, nothing moved.
- community: Rules, Updates and Safety lists same boxes, kit chevron, form unchanged.
- bridge-conflicts: base has "SHOW" left of the select with the switch beside it; after, the label is stacked above, the select is narrower and its text smaller, switch and cards 15 px lower (fault 1); at 375 the same.
- series-edition, rankings-kind, mylibrary-sort, lists-new, lists-edit (15 pictures): base and after identical (raw selects, the browser arrow in both).
- reader-chapter: the clip of the reader's top bar is a black strip in all three, so I could not see the chevron or text. **Not seen: the chevron's contrast on the dark chrome.** Only the measured boxes back the verdict. The re-shoot should take the viewport of the reader at 375 and 1024.
- commands-type: Type and Option 1 type same width, kit chevron; at 375 both 4 px taller and the form moves down by that.
- oauth2-redirect: Redirect URI full width, chevron; 375 is 4 px taller.
- richpresence-type: select 4 px wider, kit chevron, Upload follows; at 375 nothing wraps.
- api-reference-area: raw in both, identical.
- authorize-guild: "Select a server" same width, chevron; 375 card 4 px taller.

## Commander's command (whole, from `e2e/`, stack with the sweep's world on API 8798, Vite 5798, metrics 9498, `SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL` and `SR_CLI_BIN` set)
Base phase (only on a tree where the pod's files still have raw selects):

    SR_E2E_BASE_URL=http://localhost:5798 SR_P102_PAGES_ON=1 SR_P102_PAGES_PHASE=base SR_P102_PAGES_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-pages npx playwright test tests/p102-pages-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line

After phase: the same without `SR_P102_PAGES_PHASE=base`. `SR_P102_PAGES_AREAS=settings,library,dev` and `SR_P102_PAGES_STOPS=<stop,...>` split a round (a split run merges into the existing `measure-<phase>.json`). A whole phase takes about 3.5 minutes. The spec makes data under `p102-pages-<digits>` names and removes it in a `finally`.

## Proposed rows
- Reader chrome rows are not reachable by `hitTestAll` (probe): a later spec could open the reader bars first.
- The four library selects and Reference's Area stay raw for the next wave (library.css:737 narrowed; Reference's flex-wrap toolbar needs a wrapper-width rule).

## Owner questions
None.

## Cleanup
Stack stopped by PID tree, ports free, `sr_p102_pageshots_ui` dropped, no stash touched. The helper script and scratch folder are outside the repo.
