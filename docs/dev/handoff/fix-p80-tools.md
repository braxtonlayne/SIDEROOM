# Wave 80 handoff: tools (SEC-AUD-322, SEC-AUD-323; D-823, D-824; findings for D-826)

T = 07:30 on 2026-11-16. Clock times: no new item from 09:30, checkpoint by 09:50, finished by 10:15, hard limit 10:30. Actual: finished about 07:55 (one lock round, 07:38 to 07:42; the run itself 2.4 min).

## Done
- Commit `9d57bb41` (code and spec); this handoff in a second commit.
- `apps/web/src/tools/utl-dev.css`: `.ud-root > .ud-field, .ud-timer > .ud-field { flex: none; }`, placed after `.ud-field`.
- `voice-p14-e2ee.css:8`, `user-settings.css:512` (`.us-scope-list`), `vouch.css:30`: the three lines exactly as the contract says. Nothing else in those files.
- Spec `e2e/tests/p80-tools-shots.spec.ts` (local only, `SR_P80_TOOLS=1`, call part `SR_P80_VOICE=1`, out `SR_P80_TOOLS_OUT`).

## What I saw on the base (and how: stated plainly)
I did not run the base binaries' CSS from a held-back file. The spec measures AFTER (as built), then deletes the new rule from the live style sheet (CSSOM `deleteRule`) and measures again: that is the base CSS on the same page. For the colours it sets the old declaration back on the live rule. Same stack, same round, no file swap. The server binaries were the p80 base build (`BUILT_FROM` c8982549).

### SEC-AUD-322, cause confirmed: it is `.ud-field { flex: 1 1 140px }` in a column
Label heights, `.ud-field` direct children of `.ud-root`, Lounge Light desktop. Base: every one is 140.0 px (Regex Pattern: caption 15 + input 32, label 140; Diff Compare by: select 32, label 140). After: label = caption + control + 4 within 0.0 to 1.0 px for all 24 fields of all 17 tools (soft assertions passed).

| tool: field | base | after |
|---|---|---|
| Regex: Pattern (input) | 140 | 51 |
| Regex: Text to search (textarea) | 140 | 139 |
| Diff: Compare by (select) | 140 | 51 |
| Loot: Tables from, Table, Name (input/select) | 140 each | 51 each |
| Loot: Rows (textarea) | 140 | 139 |
| JSON/JWT, Base64, URL, Markdown table, Counter, Hash, Wheel: the one textarea | 140 | 139 |
Dice, coin, notes, timers have no `.ud-field` that is a direct child (nothing moved). Fields inside `.ud-row` (Diff Before/After, etc.) keep their basis. No field got taller; the 1 px on a textarea is the 4 px gap rounding. The 120 px textarea minimum is kept.

### SEC-AUD-323: three colours, all reached
Lounge Light and Dark, contrast against the first opaque background above. Elder badge and scope list were reached with the API answer faked (`**/oauth2/tokens` one grant; `**/users/*/vouches` a 640-vouch Elder summary; 500 vouches are not reachable): the element and its CSS are real, the data is not. The shield is a real two-member call (Bob in the call, "Mark as verified" pressed, then the shield showed `data-verified`).

| element | theme | base colour : contrast | after colour : contrast |
|---|---|---|---|
| `.us-scope-list` | Light | rgb(169,164,147) on rgb(242,243,245): 2.25 | rgb(92,94,102): 5.82 |
| `.us-scope-list` | Dark | same grey-cream on rgb(43,45,49): 5.53 | rgb(164,170,179): 5.90 |
| elder badge | Light | rgb(194,176,143) on white: 2.12 | rgb(71,82,196): 6.42 |
| elder badge | Dark | on rgb(49,51,56): 5.96 | rgb(0,168,252): 4.82 |
| verified shield | Light | on rgb(227,229,232): 1.68 (needs 3) | rgb(71,82,196): 5.09 |
| verified shield | Dark | on rgb(30,31,34): 7.77 | rgb(0,168,252): 6.28 |
The Light cases were unreadable on the base (under 4.5 and under 3); Dark was fine on the base and stays fine, now in the theme's blue/grey instead of gold.

Who loads the files (`git grep`): `voice-p14-e2ee.css` by `PrivacyCodes.tsx` (Lounge voice chunk); `user-settings.css` by `settings/user/parts.tsx` and by `support/SupportPage.tsx`, `support/Leaderboards.tsx` (lazy routes in `support/routes.tsx`); `vouch.css` by `profilepage/vouch/VouchPanel.tsx`, imported by `ProfilePageView`, which `profilepage/routes.tsx` loads with `lazyNamed(import())`. The signed-out `/~handle` page is the separate `src/public/profile` bundle, whose ALLOWED.ts says it never imports `profilepage/`. So none is first paint or public: **no web build, no bundle figure** (wave 79's 215 935 bytes unchanged by reading).

## Findings only (nothing changed)
- **The Wheel** (Light and Dark, desktop, opened): the wheel face is a whole disc in the theme's raised/hover colours (light grey-blue in Light, dark grey in Dark), accent ring, pointer in the text colour. No names are set in the default state, so the face is one shade; the segments are not shown with names. Nothing wrong. (Only the top half of the disc fits in the drawer without scrolling at 800 px high; not a fault.)
- **Kit Tabs (D-826) under `dir=rtl`, Saved items, 375x812, Light and Dark:** on open `data-overflow="end"`, selected Bookmarks at the right (238..346 in a list 29..346), the hidden tab "Scheduled" at the left (-26..102), the fade is on the left, the edge that hides a tab. After "Scheduled": scroll -54, `data-overflow="start"`, the fade is on the right where "Bookmarks" is cut, Scheduled 28..156 inside the list (1 px under the list's left edge, not visible). **No fault.**
- **Admin** (`/admin/moderation`, 375, Light): strip 673 wide in a 343 box, `data-overflow="end"`, mask on, the selected tab Cases at 125..197 is in view; Appeals is cut with the fade. **No fault.**
- **A server's settings: not reached.** `<Tabs` is not used in the server settings except in `settings/server/bridge/PlanView.tsx`, which needs a bridge plan; `/settings/server/<id>/bridge` showed the Overview section on a fresh server, no tab list. Not shot with a tab strip.
- Not mine, seen in passing: in `.ud-root input` (`min-block-size: 32px; inline-size: 100%`) also hits checkboxes: the Base64 "URL-safe alphabet" and the Markdown table "Line the columns up" checkboxes are drawn large and above their label (see `base64-...-after.png`, `mdtable-...-after.png`). Pre-existing (not run on the base; the rule is not one I changed). A one-line fix in `utl-dev.css` would be `.ud-root input[type='checkbox'] { inline-size: auto; min-block-size: 0; }`; I did not make it (outside the task). The "Verified" button in the privacy-codes popover in Dark is grey text on a grey fill (low contrast), `vc-e2ee-verify[aria-pressed]`, not mine.
- The "json" drawer query opens the JWT decoder first (search ranking); the JSON tool itself was not shot. It has the same single textarea field; its heights are in the log.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-tools`, 41 PNG plus `measurements.txt` (the REPORT lines). **I opened all 41.** Regex and Diff Light and Dark, desktop and phone, after (8) and the Light desktop base (2); the other tools Light desktop after (14: json query = JWT, base64, url, jwt, mdtable, counter, hash, dice, coin, loot, notes, pomodoro, stopwatch, countdown); each of the three elements Light and Dark (6) plus the two privacy popovers (2); the Wheel (3, one is a duplicate Light shot from the tool loop); Tabs: RTL Light and Dark, open and Scheduled (4), Admin (1), server settings (1, no strip). Clicked through: Regex pattern and text (3 matches listed), Diff two texts (result shown), switching regex to diff to json to stopwatch in one open drawer; the call and the "Mark as verified" click. No page scrolled sideways (asserted in the spec).

## Tests
`check-css-tokens` clean; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok; prettier clean on the four CSS files and the spec; e2e `tsc` clean; spec run `--project=chromium --workers=1`: 4 passed (2.4 min). No vitest reads these files (`git grep` found no test.tsx naming them), none run. No heavy run besides the one round.

## Docs
None state these colours or heights. No doc change.

## Proposed inventory rows
- SEC-AUD-322: "A tool's single field is as tall as what it holds: `.ud-root > .ud-field` and `.ud-timer > .ud-field` take no flex basis (a 140 px height in a column); Regex pattern 140 to 51 px, Diff mode 140 to 51 px, textareas keep 120 px (D-823)." Closes it.
- SEC-AUD-323: "The last three bare `--sr-*` colours read the theme first: the verified privacy shield, the Authorized Apps scope list and the elder vouch badge; Lounge Light contrast 1.68, 2.25 and 2.12 to 5.09, 5.82 and 6.42 (D-824)." Closes it.

## Changelog sentence
The Regex and Diff tools lose their empty gap under the first field, and the verified call shield, the scope list in Authorized Apps and the Elder badge are readable in a light Lounge theme.

## Owner questions
None. (Possible small follow-up, not asked: the checkbox sizing above.)

## Clean-up
Stack stopped by PID (server 32240, Vite 31056 and its child), scratch DB `sr_p80_tools_ui` dropped, `p80-tools-bin` deleted, lock released. No stash touched. `scratch/` in the worktree is untracked.
