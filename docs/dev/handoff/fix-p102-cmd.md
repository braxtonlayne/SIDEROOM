# Wave 102, cmd: D-968 (support helper) and SEC-AUD-453 (four selects to NativeSelect)

Clock (machine clock, 2026-11-18): work 09:30 to 10:22. Lock `cmd`: acquired 10:17 (queue wait from 09:36, both slots held by others), released 10:21. Files: `e2e/support/p102-app.ts` (new), `apps/web/src/lounge/apps/{CommandForm,InteractionModal}.tsx`, `apps/web/src/lounge/msg/Components.tsx`, `e2e/tests/p102-cmd-shots.spec.ts` (new), this file. No sheet, no string, no kit file, no world/bot change.

## Item 1: the helper (built and proved)
`seedChoiceCommand({ owner, ownerPassword, guildId })` and `startChoiceBot(app)` in `e2e/support/p102-app.ts`. Routes, in order (all worked on a `memory:` stack, no SQL):
1. `POST /applications {name: "p102cmd-app-<random>"}` (session + CSRF) -> 201, `application.id` and `bot_token`.
2. `POST /oauth2/authorize {client_id, scope: "bot applications.commands", guild_id, permissions, response_type: "code", authorize: true}` (session + CSRF) -> installs the bot in the guild.
3. `POST /applications/{id}/commands` with `Authorization: Bot <token>` -> registers `p102cmd` (group `team`: `add` with the choice option `mode`, `drop`; subcommand `status` with `mode`; three choices) -> returns the command id.
`remove()`: `DELETE /applications/{id}/commands/{cid}` (bot token), then `DELETE /applications/{id}` `{password}` (session + CSRF; removes the bot everywhere). Proved: after `remove()` a fresh composer lists 0 options named `p102cmd`.
`startChoiceBot` (in-repo SDK, gateway) answers `status` with a message holding a select menu, `team drop` with a modal holding a select (callback type 9), `team add` with a plain reply; it registers nothing itself.
Three lines a later spec writes:
```
const app = await seedChoiceCommand({ owner: ownerCtx.request, ownerPassword: bootstrapState().owner.password, guildId });
try { /* open the channel, type /p102cmd, take the suggestion: the form opens */ }
finally { await app.remove(); }
```
Side effect: the bot's replies are messages in the world's `#general` and stay (3 per config run that shoots components, 6 in all per run).

## Item 2: the four selects (all reached, all moved)
| file | select | base box (1024 / 375) | after | class |
|---|---|---|---|---|
| CommandForm | subcommand level 1 and 2 | 520x36 / 303x36 | 520x40 / 303x44 | `lng-appform-select` dropped |
| CommandForm | choice option `mode` | 520x36 / 303x36 | 520x40 / 303x44 | same |
| InteractionModal | modal select | 520x36 / 303x36 | 520x40 / 303x44 | same |
| msg/Components | message select menu | 99x32 / 99x32 | 147x40 / 147x44 | `lng-cselect` dropped |
Sheet rules: `.lng-appform-select` (`apps/apps.css:20`, min-block-size 36 px) and `.lng-cselect` (`msg/msg.css:846`) match only by class, so dropping the class removes them; no rule by element name reaches these selects (`lounge.css` `.lng-field select` does not apply: none is in a `.lng-field`). Heights equal the kit's (40; 44 at 375). The message select is wider (the kit's chevron padding), inside its inline-flex wrap; no neighbour moved. Classes are found by no test or spec (`git grep`).
Moves (compare.json, `problems: []`): every control below a select moved by exactly the summed height growth (form 12 px at 1024, 24 at 375; modal 4/8; message row +8/+12); composer box unchanged (196x34 at 1024, 119x56 at 375); no sideways scroll; no overlaps. Hit test (`openCollapsed` + `hitTestAll` over the dialog), pass/fail base -> after: form 1024 Light and Dark 1/5 -> 1/5 (the helper's points are 21 px away, made for a 44 px control; takers are the form, footer, header: same as base); form 375 4/2 -> 6/0; modal 1024 0/4 -> 0/4; modal 375 2/2 -> 3/1 (the remaining fail is the dialog's Close button, taker the body, as in base: kit dialog); message 1024 0/1 -> 0/1, 375 0/1 -> 1/0. Keyboard (all three form configs): focus-visible true, ArrowDown changed "" to "fast", ArrowUp put it back; nothing sent.
Who imports: `CommandForm` and `InteractionModal` via `Composer.tsx`/`LoungeShell.tsx`, `msg/Components.tsx` via `MessageItem.tsx`: all in the Lounge's lazy chunk (not first paint, no public page). The kit's `NativeSelect` CSS is already in a shared lazy chunk. Unit tests run after the last edit, unedited, all pass: `CommandForm.attachment`, `p11DevPlat`, `staffCards`, `testerCards`, `p14AlertCards`.

## STACK-ROUND
One round, slot as `status` showed `cmd 10:17`; stack up 10:17, seed (no FAIL), base 10:19 and 10:20 (first base run failed on my own spec's button name "Run" vs "Run command", fixed), after 10:21, stack down and ports 8787/5787/9487 free, released 10:21. Base was run with the three files put back from copies in `p102-cmd-scratch` (no stash); `git status` afterwards: only the untracked spec.
## TYPECHECK
`tc.sh run cmd bash -c "cd .../p102-cmd/apps/web && npx tsc --noEmit -p ."` 10:21 to 10:22, exit 0 (after the last app edit). Also exit 0: prettier --check on the five files, e2e `tsc` (from `e2e/`), `i18n-audit --max 0`, `check-doc-links`. eslint on the three app files exit 0 (before the spec existed); eslint on the e2e files not run (e2e has no eslint).

## Shots (`visual-sweep-shots/p102-cmd`): 16 made, 9 opened
Opened: all 8 after shots (command-form 1024 Light/Dark, 375 Light; modal 1024, 375; components 1024, 375 ... plus form) and `base-command-form-375-lounge-light`. The other 7 base twins were not opened (their figures are in figures-base.json). Seen: the base shows the browser's own arrow and small boxes; after shows the kit chevron, equal widths, 40/44 px, labels intact; the 1024 components shot has the message hover toolbar over a row (pointer rest from the scroll, not a fault).
Not done: eslint of e2e; the two modal/message shots in Dark (not asked).

## Commander's command (from `e2e/`, stack with the sweep world, API 8787, Vite 5787, `SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL`, `SR_CLI_BIN`)
```
SR_E2E_BASE_URL=http://localhost:5787 SR_P102_CMD_ON=1 SR_P102_CMD_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-cmd \
npx playwright test tests/p102-cmd-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
(Delete `figures-base.json` in the folder first after the merge.) It registers no account.

## Text and proposals
- SEC-AUD-453 (cmd): "The Command form's three selects, the app modal's select and an app message's select menu are kit `NativeSelect`s (40 px, 44 at 375)." Reach by D-968's helper.
- Changelog: "App commands' option lists, app modals and app message select menus use the kit's select look and a full touch size on a phone."
- Proposed row: the app modal's Close button at 375 is taken by the dialog body at its lower point (kit Modal; same in base).
- AGENTS.md: "A spec that needs an app with a choice command calls `seedChoiceCommand` of `e2e/support/p102-app.ts` and removes it in a `finally` (D-968)."
- Owner questions: none.

## Clean-up
`sr_p102_cmd_ui` dropped, `p102-cmd-bin` deleted, test-results removed, no stash touched. `heavy.sh status` before reply: see the commit message of the last commit (free at 10:21 after release).
