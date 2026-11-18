# Wave 104 keys (D-995): a focused select keeps ArrowUp / ArrowDown

## REACH table (committed 14:2x; the shell's one `keydown` handler is `useLoungeShortcuts` in `apps/web/src/lounge/LoungeShell.tsx`, a `document` listener live while `LoungeShell` is mounted)

| # | select | file:line | label | how to open it | now styled by | theme |
|---|---|---|---|---|---|---|
| 1 | search results' Sort | `apps/web/src/lounge/SearchBox.tsx:366` (`NativeSelect` in `label.lng-search-sort`) | sr-only "Sort by" (`lounge.searchBox.sortBy`) | in a text channel type a query in the header search box, press Enter; the results panel shows the Sort select; Tab or click to focus it | the kit `NativeSelect` | Lounge theme of the account |
| 2 | forum toolbar's Sort | `apps/web/src/lounge/forum/ForumView.tsx:221` (plain `<select>` in `label.lng-forum-select`) | "Sort" (`lounge.forum.sort`) | open a forum channel; the toolbar above the posts | `.lng-forum-select` in `forum.css` (forum builder changes it to the kit select) | Lounge |
| 3 | Create Event's channel | `apps/web/src/lounge/events/CreateEventDialog.tsx:165` (`NativeSelect id="event-channel"`, label `lounge.createEvent.channel`; shown only for the voice/stage kinds, not for "elsewhere") | "Channel" | channel sidebar guild menu, "Create event" (`ChannelSidebar.tsx:422` opens dialog `createEvent`); needs createEvents / manageEvents / manageGuild | kit `NativeSelect` | Lounge |
| 4 | Server settings selects (Conflict log Kind, `settings/server/bridge/ConflictLog.tsx:109`, and the `Select`s of `sections/*`) | see file | various | `/settings/server/:guildId/:section` | n/a | n/a |

Row 4: **the shell's handler is NOT mounted there.** `routes/settingsServer.tsx:15-27` makes the server settings routes children of the router `root`, siblings of the Lounge route (`routes/lounge.tsx` -> `LoungeRoot` -> `LoungeShell`), so opening Server settings unmounts the shell and its `document` listener. Other global key listeners still live there (`PlayerHost`, `DrawerGate`, `BugReportGate`) but they own only player, Mod+K and bug-report keys, never Alt+Arrow. So row 4 needs no change and loungeshots need not drive it. Other Lounge dialogs with selects (thread create/settings, slowmode, poll templates, guild profile) sit under the same handler and get the same fix.

loungeshots: drive rows 1, 2 and 3.

## Clock
Start 14:13, REACH table committed 14:14, three code commits 14:14-14:15, TYPECHECK done 14:15. Everything finished well before 14:50.

## Done (commits on `fix/p104-keys`)
- `D-995 keys REACH table (handoff)`.
- `D-995 shortcuts.ts (READY)`: new exported `yieldsToSelect(e: Pick<KeyboardEvent,'key'>, target)`: true when the key is `ArrowUp` or `ArrowDown` and `target instanceof HTMLSelectElement` (any modifiers). Nothing else in the file.
- `D-995 LoungeShell.tsx (READY)`: first line of `onKey`: `if (yieldsToSelect(e, e.target)) return;`, plus one import name (`yieldsToSelect` added to the existing `./shortcuts` import, which prettier wraps over six lines; the import line is outside `:86-103` but cannot be avoided). It is placed BEFORE `matchCustomShortcut` because the decision covers a member's own arrow keybind too (that keybind is matched inside `matchCustomShortcut`), and because it is one `key` compare that costs nothing for any other key. Nothing is prevented before it.
- `D-995 keys test (ALL READY)`: `apps/web/src/lounge/__tests__/p104-keys.test.ts`, 4 tests: true for select with ArrowUp and ArrowDown under six modifier sets (none, Alt, Shift, Ctrl, Alt+Shift, Ctrl+Alt); false for select with `k`, Escape, ArrowLeft, ArrowRight, Enter; false for input, textarea, button, `document.body`, `null` with ArrowDown.

Not done: nothing. Behaviour changed is exactly the decision's sentence; the existing `matchShortcut` and `matchCustomShortcut` are untouched, so `logic.test.ts` is unaffected (23 passed).

## Who imports the files, which chunk
- `LoungeShell.tsx`: only `routes/LoungeRoot.tsx`, reached by `routes/lounge.tsx` through `lazyNamed(() => import('./LoungeRoot'))` (lazy Lounge chunk). Not first paint.
- `shortcuts.ts`: imported by `lounge/LoungeShell.tsx`, `lounge/ShortcutSheet.tsx`, `player/PlayerHost.tsx` (lazy via `player/PlayerGate.tsx`), `tools/drawer/DrawerGate.tsx` (lazy in `router.tsx:79`), `bugreport/BugReportGate.tsx` (lazy in `router.tsx:83`). No static import from first paint; the file imports only `../i18n` as before. I added no import to any file, only an export (about 200 B, inside lazy chunks). No web build run.

## Test watched fail
Wrote the test first with `yieldsToSelect` stubbed to `return false`: `vitest run src/lounge/__tests__/p104-keys.test.ts` gave 2 failed (the two ArrowUp/ArrowDown "is true" cases), 2 passed (the false cases). After the body was written: 4 passed.

## Checks (all from the worktree, after my last edit, last commit state)
- `npx prettier --check` on the three files, 14:14, exit 0.
- `pnpm --filter @sideroom/web exec eslint` on the three files, 14:14, exit 0.
- `vitest run src/lounge/__tests__/p104-keys.test.ts` 14:14, exit 0 (4 passed); `.../logic.test.ts` 14:14, exit 0 (23 passed).
- `node scripts/i18n-audit.mjs --max 0` 14:15, exit 0 (TOTAL 0).
- `node scripts/check-doc-links.mjs`: see TYPECHECK below.

## TYPECHECK
`bash cli-agents/tc.sh run keys bash -c "cd .../p104-keys/apps/web && npx tsc --noEmit -p ."`, 14:15, exit 0.

## STACK-ROUND
none (code builder)

## Docs
No doc states this behaviour; none changed.

## Proposed inventory row text (SEC-AUD-470, low)
Alt+Arrow channel shortcuts take the key from a focused select. Closing text: "Fixed in wave 104 (D-995): while focus is on a `select`, ArrowUp and ArrowDown (with any modifiers, and a member's own arrow keybind) are left to the select; `yieldsToSelect` in `lounge/shortcuts.ts`, checked first in the shell's `onKey`. Alt+ArrowDown opens the Sort list instead of moving to the next channel; on the message list, a button, a text field and the body the shortcuts work as before."

## Changelog sentence
"In the Lounge, Alt+ArrowDown and the other arrow-key shortcuts no longer steal the key from a focused select, so the Sort lists open with the keyboard (SEC-AUD-470)."

## Owner questions
None. One note: a member who rebinds a shortcut to an arrow key will find it inert while a select has focus (by D-995).

## PROCESSES
Listing at 14:16 shows three `bash.exe` (19196, 11268, 33508) and the listing's own `powershell.exe` (9564): the bash ones are the harness shells running my commands (command lines contain the worktree path), no server, vitest or tsc of mine remains. `node scripts/check-doc-links.mjs`, 14:16, exit 0 (doc links ok). No ports used, no database, no binaries, no lock held.
