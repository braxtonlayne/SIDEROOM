# Wave 83 handoff: lounge (SEC-AUD-347 and SEC-AUD-348, D-853)

Builder: lounge. Branch `fix/p83-lounge` (base `a9d7f073`). No Rust, no cargo, no `.tsx` changed, no i18n.

## Clock

T = 12:10 on 2026-11-16. No new item from 14:10, checkpoint 14:30, finished 14:55, hard limit 15:10.
Rounds: round 1 took the lock at 12:15 and released at 12:28 (seed of the sweep world, thread item, overlays, three re-runs of the overlays that
needed other selectors); round 2 took it at 12:35 and released at 12:40 (base of the thread header with the old rule, then both items again with the
fixes). Handoff written 12:41. Done with both rounds well inside T+2h20.

## Done (commits)

- `11237119` SEC-AUD-347, SEC-AUD-348: `threads.css` and `profile.css` rules (below) and the local spec `e2e/tests/p83-lounge-shots.spec.ts`.
- This handoff.

## Item 1 (SEC-AUD-347): a thread's member list on a phone: answer (a), plus one fault

**(a) It works for a member.** A thread's header has its own control, **"Thread members (N)"** (`threads/ThreadMembers.tsx`, a popover on the thread header;
it is not named "Member List"). At 375x812 with touch and a mobile user agent a real tap on it opens the list (box 8,75 to 367,174 on the base: inside the
viewport, document `scrollWidth - clientWidth` 0, main scroller 0), a second tap closes it, and focus is back on the control. Same in Lounge Light. At 1280
the same control works with a click. A swipe in the thread (as in the channel) brings up the channel's own member list pane over the thread (it is the
channel's list, `.lng-right`, 0,29 to 375,755, nothing of the thread on top at its centre).

**What wave 82's query had picked.** `getByRole('button', { name: /Member List/ }).first()` matches exactly one node: the **channel header's** "Member List"
toggle (`.lng-header`, 267,39, 28x28, `aria-pressed="true"`), which sits **behind the thread pane**: `elementFromPoint` at its centre is the thread pane's
close-button icon (`path` inside `.lng-thread-pane`). Playwright's click said "intercepts pointer events" for 3 s (5 s in wave 82's spec). At 1280 the same
node is visible and free (the thread is a side pane there), so the click works. The spec never touched the thread's own control, which has a different name.

**The cause of the one fault.** The working control was 28 px tall on the phone, and so were the other three buttons of the thread header (Following: 28,
Thread options: 28, Close thread: 28), against D-846's 44 px.

**What I changed.** `apps/web/src/lounge/threads/threads.css`, one block, phone only (`.lng-root[data-mobile] .lng-thread-head`): every button of the thread
header is 44 px tall (the header is 48), the three icon buttons keep their 28 px width and reach 4 px into the gap on each side with a pseudo-element
(finger target 36 x 44, the title does not move: 121 px before and after). Measured after (375, Lounge Dark and Light): Following 85x44, Thread members
28x44 (hit area 36 wide), Thread options 28x44, Close thread 28x44; the member popover is still whole and a tap opens, a second tap closes it, focus
returns. At 1280 the four boxes of the thread header are identical to the base (checked: same boxes in both runs).
`threads.css` is imported only by `ThreadPane.tsx`, which only `LoungeShell.tsx` imports (the Lounge's lazy chunk via `routes/LoungeRoot.tsx`): not first paint,
no public page.

No `.tsx` changed (the "name the file before you edit it" condition did not arise). No vitest run (no `.tsx`).

## Item 2 (SEC-AUD-348): the overlays

**Measured: 19 overlays at 375x812 (touch) and 19 at 1280x800** (Lounge Dark, opened one at a time by their own controls, closed before the next). Not reached: none
at either size. At 375 (19): emoji picker from the composer, from a reaction pill; GIF picker; sticker picker; profile popover from a message's author, from the member list;
message menu (long press); pins; search with results (the phone's sheet); inbox; channel list drawer (a pane); member list (a pane); channel notification settings; create
channel; create server; invite people; server menu; status menu; tools drawer. At 1280 (19): the same without the channel list drawer (the list is always there) and with
the emoji picker from the message toolbar too ("More" menu for the message menu, the search panel, the member list as the right panel).
Raw lines: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-lounge/measure.json` (round 2, all of them; `measure-round1.json` is round 1, with the base
figures of the profile popover).

For every overlay in both sizes: no edge outside the viewport, document sideways scroll 0, main scroller 0. Sideways scroll inside an overlay: the search panel's
result lines (`span.lng-search-context`, `overflow-x: hidden`, ellipsis: meant), the GIF and sticker popovers at 1280 (14 px, `overflow-x: visible`, no scrollbar:
nothing is cut), the profile popover's head (6 px, visible). None is a fault.

**The table** (D-853; "fixed" = this wave):

| overlay | size | fault and figure | node | file and rule | status |
|---|---|---|---|---|---|
| thread header controls (item 1) | 375 | four buttons 28 px tall | `.lng-thread-head button` | `lounge/threads/threads.css` phone block | **fixed**: 44 px tall (Following 44, icons 28x44 with a 36 px hit area) |
| profile popover (from a message's author and from the member list) | 375 | no close control; 8 px of outside above and 6 px below, 25 px at the left (box 25,8 to 367,806 on the base): a member had a strip a few pixels wide to tap outside | `div.lng-pop` | `lounge/profile/profile.css`, `.lng-pop` `max-height: calc(100vh - 16px)` | **fixed**: phone block `max-block-size: calc(100dvh - 144px)`: box now 25,134 (from a message: 134 px of outside above) and 25,72 (from the member list: 72 above, 70 below), scrolls inside itself. Desktop unchanged (box 376,8 and 698,8, 342x786, as on the base) |
| create channel, create server, invite people, tools drawer ("Drawer") | 375 | close control 32x32 (under 44) | the kit Modal's `button[aria-label="Close"]` (`_close_…`) | **kit:** `packages/ui/src/primitives/Modal/Modal.module.css` `.close { width: 32px; height: 32px }` | **listed, not fixed** (kit is nobody's this wave). Change: add `@media (max-width: 640px) { .close { width: 44px; height: 44px; } }`; the header has room (title wraps first; "Invite people to The Back Booth" already wraps to two lines at 375). Every `Modal` in the app has the same 32 px close, in the Lounge and outside it |
| the same four | 375 | not a fault: box 15,16 to 360,798 at the tallest (create channel, create server: 782 px high, 14 px from the bottom): the dialog fills the screen and its body scrolls inside it (`max-height: calc(100dvh - 32px)`), the primary button sits below the fold in create channel | Modal | kit | **listed**: no change proposed beyond the close control; say if the owner wants the footer pinned |
| message "More" menu (and the right-click menu) | 1280 | **placement, not the viewport**: the menu is 200x547 and opens at 8,4 (the top-left corner), 980 px from the "More" button (989,477) that opened it; the right-click menu opens at 612,8. Whole, but not next to its trigger: the menu is taller than the room above or below the trigger and the clamp pushes it to the corner | `[role=menu]` from `DropdownMenu` | **kit:** `packages/ui/src/primitives/Menu/Menu.tsx` / `Overlay/useFloating.ts` placement when neither side fits | **listed, not fixed**; I did not read the placement code. Change to consider: when neither side fits, clamp the menu to the trigger's x, not to the left edge, and let it scroll (max-height) rather than float 547 px |
| profile popover and message menu at 375 | 375 | both end 6 and 8 px above the bottom edge (profile: 804; message menu 804) | `.lng-pop`, `[role=menu]` | `.lng-pop` fixed above; the menu is the kit's | listed: whole, nothing to do |
| pins, inbox, emoji pickers, GIF/sticker, notification settings, server menu, status menu, search, drawers | both | **no fault** | | | measured, whole; not shot (the brief says no shots of whole overlays) |

Close controls: pins, inbox, pickers, notification settings, profile popovers, menus and the phone's two panes have **none** (they close by Escape or an outside
tap; on a phone: an outside tap, or choosing a channel or a swipe for the panes). The phone's search sheet closes with its back arrow (44 px: `lounge.css`
`.lng-search-sheet-close`). The four kit modals above close with the 32 px button.

Observations outside the contract (for the commander, not fixed): the channel header's seven icon buttons (Threads, Notification Settings, Pinned Messages,
Saved items, Member List, Inbox, Help) and Back are **28 px tall on the phone**, and the Search button is the only one with a 44 px pseudo-element
(`lounge.css:2203`). That is the same fault as the thread header's, in `lounge.css` (a Lounge sheet, which this wave's contract allows me to fix only for
overlays; it is not an overlay). Rule to change: the thread header's block, applied to `.lng-root[data-mobile] .lng-header .lng-icon-btn` (the row is crowded:
use the same 28 px width and the 44 px height with the 4 px pseudo-element). The channel header's "Member List" toggle on the phone changes
`memberListOpen` but the panel sits one pane to the right (x = 375, reached by a swipe); the button does not bring it into view. That is wave 69's design (the
swipe); noting it because it is what wave 82's query clicked.

## Finding about the spec, not the app

After a swipe made through the DevTools touch protocol (as p69's spec does) the next finger tap on a member row produces touch events but **no click** in
Chromium, so no profile opens; a mouse click on the row does. I think that is the synthetic gesture, not the app (other taps on the same page open their
overlays), but I could not prove it with a real device; the spec uses a mouse click on the row at 375 and says so in a comment. If a real phone has the same
fault it is in the member list's row (`MemberList.tsx` wraps it in a `ContextMenu` with touch handlers; `onTouchEnd` only cancels the long-press timer and
nothing calls `preventDefault`).

## Shots

Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-lounge` (uncommitted, with `measure.json`, `measure-round1.json`, `totals.json`):
- thread with its member list open at 375: `thread-phone-lounge-dark-members-open.png`, `thread-phone-lounge-light-members-open.png` (2), its base
  `base-thread/thread-phone-lounge-dark-members-open.png` (1: the base, with the 28 px buttons), and 1280 `thread-desktop-lounge-dark-members-open.png` (1);
- faults at 375: `create-channel-phone`, `create-server-phone`, `invite-people-phone`, `tools-drawer-phone` (4: the 32 px close; these are the "before", the rule is the
  kit's and was not changed), `profile-from-message-author-phone` and `profile-from-member-list-phone` (2: the after; the before shots were overwritten by
  the second run, the before figures are in `measure-round1.json`).
Total kept: 12. **I opened all 12.** Shots of the whole overlays (about 30 more from the `SR_P83_SHOTS=all` runs) are moved to
`visual-sweep-shots/p83-lounge-whole-shots-discarded/`; I opened 4 of them (message menu at 375 and at 1280, inbox and search results at 375); the rest are judged from their measure lines only, so I did not look at every overlay, only at those with a fault and those four.

## Tests and checks

- Spec: `SR_P83_LOUNGE=all SR_P83_SHOTS=all npx playwright test tests/p83-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line`:
  1 passed (it measures and does not assert). The base case: the thread header with the old `threads.css` (from `a9d7f073`) was run (28 px, `base-thread/`), and
  wave 82's click was reproduced on the base and after (it times out with "covered": nothing of mine changes what is behind the thread).
- `node scripts/check-css-tokens.mjs`: 73 files clean. `node scripts/i18n-audit.mjs --max 0`: 0. prettier on `threads.css`, `profile.css` and the spec: clean. e2e `tsc`:
  clean. No eslint, web tsc or vitest (no `.ts`/`.tsx` of the app changed).
- No web build (lazy chunks only; who imports: `threads.css` only `ThreadPane.tsx`, `profile.css` only `ProfilePopout.tsx`; both only `LoungeShell.tsx`, which only
  `routes/LoungeRoot.tsx` (lazy) imports).
- `docs/`: no doc states a size of the thread header or of the profile popout (searched "thread header", "profile popout"): nothing to update.

## Proposed inventory rows

- **SEC-AUD-347** (closed): a thread's member list on a phone: it is reachable ("Thread members (N)" in the thread's own header; wave 82's click picked the channel header's
  "Member List" button behind the thread pane); the thread header's four controls were 28 px tall and are 44 px tall now. Evidence: `measure.json`, the shots above.
- **SEC-AUD-348** (**not** closed: one open row in the kit): the Lounge's overlays measured, 19 at 375 and 19 at 1280, all whole, document 0; fixed the phone profile popout's
  room for an outside tap; open: the kit Modal's 32 px close control (four Lounge dialogs at 375, every other Modal too) and the kit menu's placement when the menu is taller
  than the room (1280 message "More" menu opens at the top-left corner). Wave 84, from this table.

## Changelog sentence

On a phone the buttons in a thread's header are easier to hit, and a member's profile pop-up leaves room above it to tap outside to close it.

## Owner questions (for the Fable agent)

1. Should the channel header's icon buttons (seven of them, plus Back) be 44 px tall on a phone like the thread header's now are? (Default: yes, same rule.)
2. Should a phone's profile popout get a close button (a `.tsx` change) instead of relying on an outside tap? (Default: no, this wave's room above is enough.)

## Cleanup

Stack stopped by PID (ports 8614, 5614, 9314 free), lock released, scratch database `sr_p83_lounge_ui` dropped, `p83-lounge-bin` deleted (`p80-base-bin` untouched). No stash touched or
found by me. The sweep world's state is in `e2e/.state/sweep/` (ignored, untracked).
