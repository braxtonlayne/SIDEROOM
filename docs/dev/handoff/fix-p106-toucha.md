# fix/p106-toucha handoff (wave 106, census: the 44 px fails in the Lounge and User settings)

Measure only; no app file changed. The product is `C:/Users/braxt/Desktop/SIDEROOM_wt/p106-touch-a-report.md` (tables, groups by rule, takers that are controls, passes, not covered). Figures: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-toucha/figures-all.json` (merged) and `figures-part1/3/4/5/12.json` (`figures-part2.json` is the first, superseded run; `old/` holds earlier versions of parts 1, 4, 5 from runs with a wrong root).

## Clock times
Started 16:30; spec first run 16:33; last job ended about 17:44; report and handoff 17:50. The runner was down about 16:40 to 16:55 (mount's cargo build); job 0095 waited 1170 s for it.

## Done (commits)
- `e2e/tests/p106-toucha-shots.spec.ts` (new): 5 parts by `SR_P106_TOUCHA_PART` (1 channel and thread; 2 forum, DM, voice, scrapbook; 3 Home list, Friends, Message Requests, server list, inbox; 4 and 5 User settings 1-10 and 11-20; `12` runs 1 and 2 in one job). Overlays: header More strip, member sheet, message action menu (touch long press, 500 ms touchstart), Add to message, emoji and GIF pickers, phone search sheet, post pane, New Post, DM profile pane, server menu, Add a Server, the Friends tabs, inbox, the Notifications tab, the Sections drawer, and one real dialog per settings section where one opens.
- The report (outside the repo).
Commit subjects start with D-1004 (the jobs decision); the census has no D-number of its own.

## Not done (why)
See the report, section 5. Short: group DM (no second account without sr-cli), scrapbook gallery (feature off in the seeded server), friends and requests with data, attachments and link previews (the sweep's browser-side injection is not copied), calls, 18 of 20 settings dialogs (their buttons act inline or do not exist on the seeded account).

## FINDERS (rule 13)
none. No class, id, label, role or tag of any app file was changed; no spec or test drives anything changed.

## JOBS (id, class, waited, ran, rc; all `shoot base`)
- 0083 part 1: waited 10 s, ran 85 s, rc 0 (first run: wrong member-sheet and thread roots).
- 0095 part 1: 1170 s / 47 s / 0 (runner down while it waited).
- 0105 part 2: 590 / 94 / 0.
- 0119 part 3: 497 / 45 / 0 (final part 3).
- 0132 part 4: 600 / 48 / 0 (the settings "dialog" was the whole layout: wrong root).
- 0133 part 5: 649 / 41 / 0 (same fault).
- 0134 part 12: 695 / 73 / 0 (final forum, DM, voice, scrapbook rows).
- 0140 part 4: 271 / 66 / 0; 0141 part 5: 347 / 53 / 0 (intermediate).
- 0142 part 1: 330 / 44 / 0 (the channel message menu did not open).
- 0147 part 4: 120 / 72 / 0; 0148 part 5: 206 / 58 / 0 (final part 5).
- 0150 part 1: 268 / 46 / 0 (final part 1).
- 0151 part 4: 172 / 64 / 0 (final part 4).
Every job rc 0; the spec records an overlay that did not open as a note, not a failure. Nothing is queued or running.

## Figures against the task's lists
There is no "may change" or "must not change" list (census). Result: 971 rows, 558 pass, 192 fail, 0 not-reached; 43 fails are taken by another control. The wave 105 cure (forum toolbar) passes.

## Shots (each opened after its job ended; what I saw)
Prefix `base-`, suffix `-375.png`, in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-toucha`.
- channel: the general channel at the bottom of the list, composer with + / mic / GIF / image / clock / emoji, bottom tab bar, no sideways scroll.
- channel-header-more-strip: the 6-icon strip under the header (bell, pin, bookmark, members, inbox, info).
- channel-member-list-sheet: Online 1 / Offline 1, two 42 px rows, no header.
- channel-message-action-menu: the long-press menu (Add Reaction highlighted down to Delete Message), 14 items.
- channel-add-to-message-menu: Upload a File, Create Thread, Create Poll, Start an Activity, three greyed.
- channel-emoji-picker: the emoji popup scrolled to Food and Symbols, 6 category buttons at the left.
- channel-gif-picker: tabs GIFs / Stickers / Emoji and the "GIFs are not set up" text.
- channel-phone-search-sheet: the Search page with the field and five filter chips.
- thread: the thread pane over the channel with Following, members, bell, close; the first reply.
- thread-message-action-menu: the same menu inside the pane.
- forum: New Post, search, sort, list and gallery toggles, one post card.
- forum-post-opened: the post pane (Following, members, bell, close), welcome text, one message, composer.
- forum-new-post: the New Post dialog (title, message, Attach files, Cancel, Post).
- dm-request: Juniper's message request (Accept, Ignore, Spam, Block).
- dm: the accepted DM with call, video, search, more; a Block button; two messages.
- dm-header-more-strip: the 6-icon strip under the DM header.
- dm-profile-pane: the profile pane (Juniper, Add friend, View Full Profile).
- dm-message-action-menu: the long-press menu, 11 items.
- voice-channel: the Hangout page with the status line, Screening Room card (3 buttons), Jukebox, Join Voice, Preview camera.
- scrapbook: "The scrapbook is not turned on for this server."
- home-list: rail, Find or start a conversation, Friends, Message Requests (1), ring tray, empty DM list, user panel, tab bar.
- friends, friends-tab-online, friends-tab-pending, friends-tab-add-friend: the Friends header with four pills, the notice, search and the empty state; Add Friend shows the username field.
- message-requests: one row (Juniper) with four buttons.
- server-list: rail plus the channel list of The Back Booth (a "Direct Messages" tooltip shows on the door) and the user panel.
- server-list-server-menu: 12 menu items from the server name; server-list-add-a-server-dialog: the Create a Server dialog with templates.
- inbox-header-inbox: Mentions / Unreads / For You tabs, three checkboxes, "No one has mentioned you".
- inbox-tab-bar-notifications: the "coming soon" Notifications dialog with Got it.
- you-tab-you-tab: the same list pane as server-list (the You tab shows nothing else).
- settings-account, -profile, -privacy, -activity, -apps, -modules, -devices, -connections, -invites, -storage, -support, -appearance, -accessibility, -text, -notifications, -voice, -keybinds, -language, -streamer, -advanced: each the section content scrolled by the probe to its last control (the Sections select and the close button at the top); no sideways scroll on any; none shows a crowded or cut control.
- settings-account-sections-nav-drawer-: the open Sections list (App Settings group) with What's New and Log Out.
- settings-account-dialog: the Change your username dialog; settings-advanced-dialog: the Reset UI state dialog.

## Who imports my files, chunk
`e2e/tests/p106-toucha-shots.spec.ts` is imported by nothing (a Playwright spec); it imports `e2e/support/hit-test.ts`, `fixtures`, `world`, `cli` (read only). No app file touched, so no first-paint question.

## Checks (own tree, last commit)
- e2e `npx tsc --noEmit -p tsconfig.json` (from `e2e/`): exit 0, 17:47.
- `npx prettier --check e2e/tests/p106-toucha-shots.spec.ts`: exit 0, 17:47.
- eslint on the spec: not run as a check: no eslint config covers `e2e/` (the web package's eslint reports the file as outside its base path).
- web tsc, build, vitest: not run (no app file).

## Closing text for my rows and proposed new rows
Closing: "Census done: 192 fails at 375 in the Lounge and User settings, in 13 groups by rule (report, section 2); 43 are taken by another control." Proposed rows (the base had them all): A the composer's five buttons, B the user panel's buttons, C the Friends header, D lone icon buttons in 28-32 px rows, E message-row names, avatars and links, F reactions, G the emoji picker, H the `.lng-expr-tabs` tabs, I inbox checkboxes, J the phone search field, K list rows (channel, nav, member), L the settings drawer's items, M the Notice dismiss, Watch Together and the DM identity button. Takers are in the report.

## Changelog sentence
Tests: a 44 px touch-target census of the Lounge and User settings at 375 px (measure only).

## Owner questions (to Fable; Discord-like defaults)
1. Which groups first? Default: A, B and the header-pattern groups (C, D, the DM identity of M) with the existing 44 px `::after` (no layout change), then K and L with taller rows, E with a `::after`.
2. May the settings drawer grow to 44 px rows (240 px longer)? Default yes (a mobile settings row is about 48 px).
3. Should the emoji cells be 44 px on a phone? Default yes, with more scrolling.

## PROCESSES
`strays.sh p106-toucha`: strays: none found (17:46). `job.sh status`: nothing of mine queued or running.

## Faults of scripts
None. (The runner was down about 15 minutes during mount's build, as announced.)
