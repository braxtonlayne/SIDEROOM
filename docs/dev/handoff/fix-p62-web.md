# Handoff: fix/p62-web (SEC-AUD-146..149, wave 62)

Worktree `C:/Users/braxt/Desktop/SIDEROOM_wt/p62-web`, branch `fix/p62-web` (base `integration/phase61` at `3ac7c673`; I cherry-picked `8ef5d3cc`, as the coordinator asked, as the first commit). No Rust, no migration, `openapi.json` untouched. Not pushed.

## Done

| ID | what | files |
|---|---|---|
| SEC-AUD-146 | The map table has its own class `ss-bridge-maps` (`.ss-table` is untouched). At max-width 720px each map is a block: "Channel here", "Channel on Discord" and "Direction" each on a line with the column name in front (`data-label`, the same translated header text), then Test and Remove side by side, then that map's warning, then a separator. The header row is hidden visually but stays in the accessibility tree. The `container-type` / sticky `100cqw` rules and the `ss-bridge-shared-line` wrapper are gone. Logical properties, existing tokens. Desktop is unchanged. | `settings/server/bridge/BridgeSection.tsx`, `bridge.css`, test in `__tests__/bridge.test.tsx` |
| SEC-AUD-147 (D-638) | `ConfirmDialog` (danger, Cancel focused first, Escape cancels) before: Block and Remove Friend on the DM welcome and the Friends page row menu, Block and Remove Friend in the user menu (`userMenu.ts`: member list, DM rows, profile popout, voice menu), End Poll (`MessageList`), End Stage (`StageView`). Not asked, as the task says: cancelling an outgoing request and declining an incoming one (`FriendsPage.tsx:152` and `ForYou.tsx:119` are exactly that), `ReportForm`, `MessageRequestView`/`MessageRequestsPage`, Unblock. Error toasts after a confirmed action are unchanged. | new `lounge/shell/ConfirmAction.tsx`, `DmWelcome.tsx`, `FriendsPage.tsx`, `userMenu.ts`, `MessageList.tsx`, `voice/StageView.tsx`, `i18n/messages/en/p62-web-1.ts` |
| SEC-AUD-148 | Keywords: real in a small way. A failed first load was retried only when a consumer happened to mount again, with no spacing and no sharing of a load in flight. Now: one load in flight, a failure is not remembered, the next use asks again, at most once per 3 s, no toast. `resetKeywordLoad()` for tests. | `lounge/keywords.ts`, `lounge/__tests__/p62Keywords.test.tsx` |
| SEC-AUD-149 | **Not real, dropped.** There is no "Hide cameras" form with a Save in `VoiceVideo`; the only related control is "Show cameras over watch parties" (`watchPartyCameras`), which does `savePrefs({...prefs, ...})` from the current store state; nothing holds an older copy. | none |
| SEC-AUD-148 (stickers half) | **Not real, dropped.** `loadGuildStickers` already deletes a failed promise from its cache, so the next use asks again; the suggestions effect re-runs whenever the box goes from "not a sticker query" to one. | none |

### Two extra files outside the task's list (flag for the integrator)
- `lounge/store.ts`: a new dialog kind `confirmRelationship`, and `lounge/LoungeDialogs.tsx`: one `case`, so the user menu (which closes before the action) can open the confirm through the shell's dialog host, as Add Note does. Both are already in the Lounge chunk; no first-paint change.

### Texts changed after checking what the code does
- **Block**: the task text said "They will not be able to message you or add you as a friend." The server closes a 1:1 DM to writes and refuses friend requests (`routes/social/dms.rs` SEC-AUD-003, `relationships.rs`), but a block does not stop a person writing to you in a server or a group DM. Now: "They will not be able to send you direct messages or friend requests. You can unblock them in Settings."
- **End Stage**: the task text said "Everyone in the stage is disconnected from it." False: `stage_finish` only turns speakers back into the audience; nobody is disconnected. Now: "The stage ends. Everyone in it goes back to the audience and stays in the channel."
- End Poll ("Nobody can vote after this. The results stay.") and Remove Friend are as given.

## Tests

| test | file | failed without the change? |
|---|---|---|
| each map cell carries its `data-label`, the warning row follows its map | `settings/server/bridge/__tests__/bridge.test.tsx` | yes by construction (no class, no attributes); not run against the old code |
| End Poll: click opens the dialog and calls nothing, Cancel calls nothing, Confirm calls `expire` once | `lounge/__tests__/messages.test.tsx` | by construction (old code called at once); not run against the old code |
| Block / Remove Friend / End Stage: confirm needed (existing tests updated to click Confirm; Cancel and Escape checks added) | `homeFlows`, `p11ShellDm`, `shellFlows`, `p54SilentFailures`, `voice/__tests__/stageUi` | **watched fail**: with the change and the old tests, 6 tests failed (they clicked once and expected the call); they pass after I added the confirm clicks |
| a_failed_load_is_asked_again_on_the_next_use_but_not_within_3_seconds | `lounge/__tests__/p62Keywords.test.tsx` | against the old file it fails only because `resetKeywordLoad` does not exist; the 3 s spacing is new behaviour |

Run: the 7 files above plus `keywords.test.ts` and `bridge.test.tsx` together: all pass. Web `tsc --noEmit`, eslint and prettier on all changed files: clean. `node scripts/i18n-audit.mjs`: TOTAL 0. Bundle check not run (paint's).

## Visual check

Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p62-web` (39 PNGs, chromium, spec `e2e/tests/p62-web-shots.spec.ts`, `SR_P62_WEB=1`, local only). Each name ends with `-desktop|phone-lounge-editorial|lounge-light|lounge-dark`. I looked at: `bridge-maps` desktop light and dark, phone light, dark and editorial (Remove visible for both maps, labels in front, warning inside its block, separator, no sideways scroll; the spec asserts scrollWidth <= 375 and that each Remove is inside the viewport, then taps Remove on the phone and checks the DELETE), `confirm-block-desktop-light`, `confirm-end-stage-phone-dark`, `confirm-end-poll-phone-editorial`, `confirm-remove-friend-dm-phone-light`. The remaining confirm shots (other themes and sizes) come from the same code path and I did not open each. The spec clicks Cancel or Escape for every dialog in every theme, Confirm once for End Stage (DELETE faked: no media server here), and at the end for real: End Poll, Remove Friend (DM welcome), Block (Friends page).

Existing issue seen while shooting (not caused by this wave): on a 375 px phone the Friends page is wider than the screen (the tab row scrolls the page sideways) and a friend row's "..." menu opens outside the viewport, so Remove Friend and Block there cannot be reached by tapping. The spec shoots the same two dialogs from the DM welcome on the phone and from the Friends page on desktop.

The dialog text sits tight against the footer separator in `ConfirmDialog` (shared primitive, same on Admin); left alone.

## Proposed inventory rows
- SEC-AUD-146: On a phone the bridge's "Mirrored channels" table scrolled sideways with Remove off-screen. It is now stacked blocks (labelled lines, Test and Remove, the map's warning, a separator) at 720 px and below; the header row stays for screen readers. Fixed.
- SEC-AUD-147: Block, Remove Friend, End Poll and End Stage acted on one click. A danger confirm now asks first (D-638), from the DM welcome, the Friends page, the user menu, the message list and the stage view; cancelling or declining a request, report forms with a block checkbox, message-request blocks and Unblock stay unconfirmed. Fixed.
- SEC-AUD-148: A failed load of the notification keywords was only retried when a consumer remounted, without spacing; now one shared load, not remembered on failure, asked again at most every 3 s. The sticker-suggestion half was not real (the cache already drops a failed load). Fixed (keywords).
- SEC-AUD-149: Not real: "Hide cameras" has no form Save; the watch-party camera switch writes from the current store. Closed, no change.

## Proposed threat-model rows
None: these are accidental-click guards and a retry, not a trust boundary.

## Owner questions
1. The Friends page is wider than a phone screen and its row menu opens off-screen (see above). Make a task for it?
2. A block does not stop messages in servers or group DMs. The dialog text now says only what is true (direct messages and friend requests). Keep as is?

## Cleanup
Stack stopped by PID, `sr_p62_web_ui` dropped, `p62-web-target` deleted, heavy lock released. `scratch/` (untracked, excluded from git) holds the binaries and logs and goes with the worktree.
