# Wave 70 handoff: lines (SEC-AUD-246..249; D-729, D-730)

Web only: no Rust, no migration (3030-3039 stays empty), no route or response shape touched, nothing for the commander to regenerate. Branch `fix/p70-lines`, base `966e5a42`.

## Done (commits)
- `5ad81ffe` SEC-AUD-246..248: `lounge/msg/lineText.tsx` (`MessageLine`, `messageLineText`, `useHiddenKind`) and its callers; `p70Lines.test.tsx`; the `lounge-chat.md` paragraph.
- `026bce30` the local shots spec `e2e/tests/p70-lines-shots.spec.ts`.
- `a6451ef1` SEC-AUD-249: `e2e/tests/p69-notify-shots.spec.ts` (member list on a phone, scale 2 close-ups).
- `5ad0f26e` pins and the link card name nobody for a blocked author; the link card quotes plain text.
- `2edb9aca` the Inbox shows "no one has mentioned you" when every mention is hidden.

## Item 1 (SEC-AUD-246): plain text in four places
Confirmed on the base first (the test file ran against the unchanged components; each place showed `psst ||the ending|| <@123>` raw). What each place showed before and shows now:
| place | before | now |
|---|---|---|
| reply line (`MessageItem.tsx`) | first 120 characters as typed, `||the ending|| <@123>` | `psst [spoiler] @Dana`; fallback "Click to see attachment" when empty or parser not loaded |
| Inbox mention (`InboxButton.tsx`) | first 140 characters as typed | same plain line; author and place kept |
| DM toast (`DmToasts.tsx`) | first 140 characters as typed | `messageLineText` (empty when the parser is not loaded) |
| delete confirmation (`MessageDialogs.tsx` `Preview`, also the forward dialog) | first 200 characters as typed | plain line; attachment-count fallback kept |
One length everywhere (140, `notificationText`'s). The pin confirmation has no preview of the message (only the delete and forward dialogs call `Preview`): nothing to change there.
A fifth place with the same fault, found by the shots (the quoted text of the message-link card, raw `||the ending||` and `<@id>`): fixed in the same way (`MessageLinkCard.tsx`, 280 -> 140 characters).

## Item 2 (SEC-AUD-247): a blocked or ignored author's text around the list
Which place hid a blocked author before (confirm tests, run on the base): none of the five. Pins, search hits (and the lines before and after a hit) and the link card all showed the words; the reply line and the Inbox row too. So no place was a "guard only".
| place | before | now |
|---|---|---|
| reply line | the words, avatar and `@name` | "1 blocked message" / "1 ignored message" in the muted style; no avatar, no name (the list's hidden run names nobody, so the two agree); still a button that jumps to the message |
| Inbox mention | row with the words | the row is dropped (web guard beside the server's, D-721); when that empties the list, the "no one has mentioned you" line shows |
| pins panel | name and words | list's wording and "Show message" (reveal is that pin's own, then the name and text return); no name while hidden |
| search results | name and snippet | the wording in place of name and snippet; the context lines either side of a hit (`searchContext.tsx`, a file the plan did not list, it is what the results render) show the wording when their author is hidden; no reveal (the compact row) |
| message-link card | author, quoted text, attachment line | the wording in place of name, avatar and text; no attachment line; no reveal |
Surfaces that got the list's row component: none (each has its own compact row); wording-only everywhere except pins, which got the list's "Show message" wording and its own reveal. Blocking while a place is open updates it (every place reads `useHiddenAuthors()`): tested for the reply line, pins and the typing line.
Search is not shown on a phone at all (`.lng-root[data-mobile] .lng-search { display: none }`), so there are no phone shots of search results (see owner questions).

## Item 3 (SEC-AUD-248): the typing line
What the gateway does: `TYPING_START` is a channel-scoped dispatch with no audience filter (`crates/sr-gateway/src/hub.rs:517-529`); only presence is filtered by `blocked_peers` (`:1011`, `:1752`). So a blocked account's typing reaches the viewer and the web is the only filter. `useLiveTypingIds` now drops ids in `useHiddenAuthors()` (blocked and ignored); the store still holds the entry. Confirmed on the base: the test returned the blocked id. "Several people are typing" counts only the shown ones (test).

## Item 4 (SEC-AUD-249): the glyph shots
The helper from wave 68 dispatched a synthetic `TouchEvent` at `[data-pane="chat"]`; the member pane is only rendered when the header's Member List button has turned `memberListOpen` on, and the swipe needs a real touch. The spec now clicks the header's Member List button (if `aria-pressed` is not true) and swipes with CDP `Input.dispatchTouchEvent`. New test "the phone-width member list with the smartphone glyph, and close-ups at scale 2" (`deviceScaleFactor: 2` for the close-ups). Files: `glyph-phone-list-lounge-{editorial,light,dark}.png`, `glyph-closeup2-lounge-{editorial,light,dark}.png`.
What I see: the glyph sits on the avatar's bottom-right corner (not beside the name), an outlined handset 12.8 px square (box measured, 12.8 x 12.8). Light and Dark: it reads as a phone, green, clear of the name and of the status dot (deskman's dot is a green dot, phoner and owner show the glyph). **Editorial: it reads as a phone at scale 2 but is faint** (a muted olive grey on the dark brown avatar; the dot of the desktop person is just as muted), at scale 1 it is close to a smudge. I did not redraw anything (not mine).

## Shots
`visual-sweep-shots/p70-lines`, 87 PNGs from the final run (the whole spec was run twice; the second run was on a fresh database after the last code change). Per theme (Editorial, Lounge Light, Lounge Dark) and viewport (desktop 1280x800, phone 375x812): `reply-before`, `reply-compact-before`, `reply-after`, `inbox-before`, `inbox-after`, `toast`, `delete`, `pins-before`, `pins-after`, `pins-revealed-after`, `linkcard-before`, `linkcard-after` (72), `search-before` and `search-after` on desktop only (6), `typing-before-desktop-lounge-light`, `typing-after-desktop-lounge-light`, `typing-after-phone-lounge-light` (3), and the 6 glyph files. The toast shot has the Owner on another channel with the window focused. I opened every one: individually for the glyph and typing shots, and through contact sheets (the same PNGs shown side by side at natural size for phone, half size for desktop) for the others. The first run's PNGs were all opened; after the last code change I re-opened the states that changed (link card before, Inbox after, desktop and phone). The spec also asserts the recorded texts (reply line, Inbox, pins, link card, search, delete dialog, toast) and prints them (`RECORDED ...`).
Look at in the shots: the pins panel and the search snippet before a block still show the typed text (see owner questions).

## Tests
- `lounge/__tests__/p70Lines.test.tsx`, 25 tests. Watched fail on the base: the first run, against the unchanged components, was 17 failed and 3 passed of 20 (the three that passed are the unchanged-behaviour guards: the delete dialog's attachment count, the link card quoting an unblocked author, the typing line with nobody hidden); some of those 17 failed only because my first harness was wrong (no `ToastProvider` around `MessageItem`, a wrong pins button name, the wrong attachment string). After fixing the harness I re-ran the reply line, pins and DM toast tests on the base: 9 of 9 failed on the faults themselves (raw text, no blocked wording). The three search tests were added later; two of them fail without the `SearchBox.tsx`/`searchContext.tsx` change (checked by stashing those two files). The plain-text link-card test and the "every mention hidden" test were written with the code and not run against the base.
- Old tests run one by one, all green: `p13DmToasts`, `p18restored`, `msgActions`, `p10LoungeFe`, `p35BridgeMembers`, `shell`, `search`, `messages`, `p13Inbox`, `logic.test`.
- Web typecheck, eslint and prettier on the changed files, `node scripts/i18n-audit.mjs --max 0` (TOTAL 0), `node scripts/check-doc-links.mjs`: clean.
- Binaries: `sr-server` and `sr-cli` copied from `p70-base-bin` (ops's build of `966e5a42`, the same commit as my base). No cargo build.
- First paint: grep of importers of `MessageDialogs`, `InboxButton`, `DmToasts`, `lineText`, `SearchBox`, `PinsButton`, `MessageLinkCard`, `searchContext`: only Lounge files (`ChannelHeader`, `Chrome`, `LoungeShell`, `MessageList`, `MessageItem`) and the Library's own `SearchBox`. `lounge/live.tsx` (now imports `shell/hiddenAuthors`) is imported by `lounge/data.ts` and `settings/server/ServerSettings.tsx` (a lazy chunk); nothing under `store/**`, `gateway/**`, `pwa/**`, `i18n/**`, `routes/**` imports it (`routes/lounge.tsx` lazy-loads `LoungeRoot`). No bundle run was made.

## Docs
`docs/specs/lounge-chat.md`: one bullet after the stickers/polls bullet (plain text outside the message; what a blocked or ignored author hides; the typing line; the gateway still sends `TYPING_START`). The social spec the task names does not exist as a file (grep for "blocked message" finds only `lounge-chat.md` lines 50 and 101), so that list lives in the same bullet. Line 101 (thread row) already says the "blocked message" wording: unchanged.
Sentences for others' sections: none wrong. `docs/specs/lounge-chat.md` section 10 (mentions's) says "A notification shows a message as plain text (D-716)": my bullet cross-refers, nothing to change.

## Proposed inventory rows
- SEC-AUD-246: the reply line, the Inbox mention, the DM toast, the delete confirmation and the message-link card show a message as plain text (D-729). Done, `lineText.tsx`.
- SEC-AUD-247: a blocked or ignored author's text is hidden in the reply line, the Inbox, pins, search results (with their context lines) and the link card (D-730). Done.
- SEC-AUD-248: the typing line does not name a blocked or ignored account (D-730). Done; the gateway still sends the event.
- SEC-AUD-249: the phone-width member list with the smartphone glyph and the scale 2 close-ups, shot and looked at. Done; Editorial's glyph is faint (owner question).
Existing rows this closes: none named by number; the wave 69 notify handoff's "Not done" under item 4 (the phone member-list shot) is closed.

## Proposed threat-model rows
- Places where a spoiler was readable: the reply line, the Inbox mention list, the DM toast, the delete/forward confirmation, and the message-link card (five, one more than the plan counted). All five now call D-716's `notificationText`. Still readable: the pins panel (typed text) and the search snippet (server's excerpt).
- Places where a blocked or ignored account's words came back: the reply line, the Inbox, pins, search results with their context lines, the message-link card (five); and its name in the typing line. All hidden in the web; the server still sends `TYPING_START` across a block and the Inbox query has no block rule until SEC-AUD-238 (mentions).
- Nothing new is requested from the server.

## Changelog sentences (ops's file)
- "The text under a reply, in the Inbox, in the direct-message toast, in the delete confirmation and in a message-link card is now plain text: a spoiler stays hidden and a mention reads as a name."
- "A message from someone you blocked or ignore no longer shows in the line above a reply, the Inbox, the pinned messages, search results or a message link; the typing line no longer names them."

## Dead keys
None. No string was added (`lounge.messageList.hiddenRun`, `showHidden`, `lounge.preview.spoiler`, `lounge.messageItem.clickToSeeAttachment` are reused); no `p70-lines-1.ts` fragment exists.

## Not done
Nothing dropped. The plan's "reuse the list's row component where the surface can take it" was not done for any surface (each has its own compact row).

## Owner questions (defaults chosen)
1. The pins panel and the message-link card were the only places that showed typed text without a block rule being involved; I made the link card plain (it is D-729's rule) but left the pins panel's text as typed, since a pin is a whole message and a 140-character cut would lose text. Should a pin render its markdown (spoiler hidden until clicked, mentions named) instead of showing the raw text? Same for the search snippet (the server's excerpt of the stored text).
2. Search is not available on a phone (the header box is hidden at phone width). A phone user cannot search at all; is that intended? (Phone shots of search results could not be made.)
3. Editorial's smartphone glyph is faint on the dark brown (`glyph-closeup2-lounge-editorial.png`, `glyph-phone-list-lounge-editorial.png`): does it need a stronger colour? Not redrawn: the icon is not this wave's.
4. A blocked author's reply line drops the avatar and the name (as the list's hidden run does). Pins and the link card drop the name too while hidden. Say if a name should stay on any of them.
5. Ignored and blocked read as one author at a time ("1 blocked message", blocked wins over ignored); the list's own run says "both" when a run mixes them. For one message that cannot be both, but the same author can be both blocked and ignored: the lines say "blocked".

## Cleanup
`sr_p70_lines_ui` dropped (count of `sr_p70_lines%` databases: 0). `p70-lines-bin` deleted (no cargo target was ever created). Stack stopped by port (sr-server on 8483, Vite on 5483), heavy lock `lines` released. Scratch files under `p70-lines-run/` (untracked, outside the repo) hold the stack scripts, logs and contact sheets.
