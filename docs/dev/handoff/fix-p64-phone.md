# Wave 64 phone (SEC-AUD-162..164)

Branch `fix/p64-phone` (base `2646ecd1`). All three items done. Web only; no Rust, route, migration or first-paint change.

## Done (commits)
- `d5d35b34` SEC-AUD-163: `settings/user/storage/types.ts` comment lists `response_type`; `mocks.ts` `PASSING_PROBES` has the seventh row after `put`. `MountsPanel.tsx` shows the server's message as it comes (only the status is translated), so **no label and no visual check were needed**. No test counted six rows.
- `39c301e6` SEC-AUD-162: `HomeSidebar` takes `onOpenPage`, called when the member picks Friends or Message Requests (the pick, not the state); `LoungeShell` passes `showChat` (`setPane('chat')`). Opening Home, or coming back from a server, still shows the list. The pages get the back control: `Main` passes `onBack` (phone only) through `HomeFriends` (Panels.tsx) to `FriendsPage` and `MessageRequestsPage`, which render the same `lng-icon-btn lng-back` button a channel header has (hidden on desktop by the existing rule). **Files outside my list, smallest edits:** `lounge/Panels.tsx`, `lounge/shell/FriendsPage.tsx`, `lounge/shell/MessageRequestsPage.tsx`.
- (item 3 commit) SEC-AUD-164: `ChannelHeader.tsx`, scoped CSS appended at the end of `lounge.css` (`.lng-header[data-dm]`, `max-width: 720px`; shared rules untouched), fragment `p64-phone-1.ts` (`lounge.channelHeader.moreActions`: "More actions").

## SEC-AUD-164: where the controls ended (deviation: a strip, not a DropdownMenu)
Notifications, pins, saved and inbox are popovers with their own state; they cannot be opened from a menu item without rewriting them. So on a phone, in a DM and a group DM only, the row keeps back, avatar + name, Start Call, Start Video Call and a "More actions" button (`aria-expanded`); it opens a strip under the header holding the **same, unchanged buttons**: notifications, pins, saved, add people, profile/member panel, inbox, help (7). Popovers open from the strip (checked: pins). Escape closes the strip. Desktop and server channels: unchanged (`.lng-dm-extra` is `display: contents` there).
- **Search**: the search box is already hidden on a phone by the existing rule `.lng-root[data-mobile] .lng-search` (and is not in the strip). Unchanged by me; a DM has no search on a phone. Owner question below.
- Measured: DM name width at 375 px is 161 px for a 16-character name (spec requires >= 96); the identity box ends before the Start Call button starts. **Server channel header at 375 px: name width 47 px** (reported, not fixed).

## Tests (`e2e/tests/p64-phone-shots.spec.ts`, local only, `SR_P64_PHONE=1`; chromium passes both)
- Friends/Message Requests: list shows on opening Home; tap Friends: header and "All" tab inside the viewport; back; tap Message Requests: inside the viewport; back; desktop: no back button. **Watched fail on the base** (header right edge at 702 px).
- DM header: name width, no overlap, every More control visible and inside the viewport, a popover opens from the strip, desktop has no More button. **Watched fail on the base** (no More button).
- Existing vitest `p11ShellHome`, `p11ShellDm`, `p11ShellActiveNow` pass. No LoungeShell/HomeSidebar vitest harness exists, so none added. tsc, eslint, prettier clean; `node scripts/i18n-audit.mjs` TOTAL 0.

## Shots
`visual-sweep-shots/p64-phone` (outside the repo, next to the worktrees): home-list, friends, requests (phone x 3 themes; desktop Lounge Light), dm-header, dm-header-more, dm-header-pins (phone x 3 themes / light), dm-header desktop, server-header phone. All looked at; the flows were clicked by the spec.

## Not done
Group DM header was not shot (same code path, `dm.group`); Message Requests page was shown empty (no stranger seeded).

## Proposed inventory rows
- SEC-AUD-162: Friends and Message Requests: on a phone a tap on either shows the page (HomeSidebar `onOpenPage`), and both pages carry the back control; desktop unchanged.
- SEC-AUD-163: the web's storage test mock and probe-id comment list the seventh probe `response_type`; the row shows the server's message, no label.
- SEC-AUD-164: DM and group DM header on a phone: back, name, two call buttons and a "More actions" strip with the other seven controls; name keeps >= 96 px.

## Owner questions
- Should a DM on a phone get search (hidden today by an older rule) in the More strip? (Default: left as is.)
- Server channel headers at 375 px leave the name 47 px: fix in a later wave?

## Cleanup
Stack stopped by PID, heavy lock released, DB `sr_p64_phone_ui` dropped, `p64-phone-target` and `p64-phone-bin` deleted.
