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
