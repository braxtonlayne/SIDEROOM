# Wave 63, phone builder: SEC-AUD-154..156 (D-640)

Branch `fix/p63-phone` (base `integration/phase62` a625b943). No Rust change, no route or response shape change (`openapi.json` untouched). No migration. No static import added to any first-paint file (only a new i18n fragment, which loads in its own chunk).

## Done (commits)

- `4c7a6fe5` SEC-AUD-154..155: Friends page on a phone, Block dialog text.
- `3bef3485` SEC-AUD-156: Message Request rows and the inbox "For You" tab; the shots spec measures the main pane.

### SEC-AUD-154: the Friends page fits 375 px and its row menu can be used
- `shell.css`, `@media (max-width: 720px)`, all under `.lng-friends`: the header wraps (title and plus on line 1, the tab row on its own full-width line); the tab row scrolls inside itself (`[role='tablist']` has `overflow-x: auto`, `min-inline-size: 0`); `.lng-friend-text` shrinks, the name truncates with an ellipsis; the Message button is icon only (`FriendsPage.tsx` wraps its text in `.lng-friend-msg`, visually hidden at <= 720px; the `aria-label` stays); the "..." button stays in view. Logical properties only, existing tokens. Desktop unchanged (compared with wave 62's `p62-web` shot: same header, rows and Active Now column).
- Pending rows (accept / decline icon buttons) fit; they use the same row rules.
- Active Now: nothing to do. The existing rule `.lng-root[data-mobile] .lng-active-now { display: none }` (shell.css) already hides it on a phone, as Discord does. I kept it.
- Row menu: with the page fitting, the `DropdownMenu` opens inside the screen with its default placement; Remove Friend, Ignore and Block are inside 0..375 x 0..812 (asserted, all three themes). No fallback needed, `packages/ui` untouched.
- Watched fail on the base: with the base `shell.css` the spec fails at 375x812 ("main scrolls 450>375", tabs/plus button at x=422/450, and the "All" tab cannot be clicked: the `.lng-body` intercepts the click). Passes with the change.

### SEC-AUD-155 (D-640): the Block dialog text
- Code check: (1) the collapsed "N blocked messages" row is opened by the reader: `MessageList.tsx` renders `lounge.messageList.showHidden` ("Show") that adds the run to `shownRuns`. (2) It applies in a group DM as in a server: `MessageList` calls `useHiddenAuthors()` (`shell/hiddenAuthors.ts`: every blocked relationship plus ignored ids) for every channel, with no guild condition. So the middle sentence of the decided text is true as drafted; no rewording to the code was needed.
- New key `lounge.confirm.blockBodyFull` in `i18n/messages/en/p63-phone-1.ts`; `ConfirmAction.tsx` reads it for the Block body. `p62-web-1.ts` not edited; its `lounge.confirm.blockBody` is now unused. `node scripts/i18n-audit.mjs` reports TOTAL 0 (it does not object to the unused key). Owner/commander may delete the old key at integration if wanted.
- Test: `lounge/__tests__/p63BlockDialog.test.tsx` (vitest, passes; asserts the new sentences, the old text had none of them so it fails without the change). No existing test asserted the old text.
- Shots: the dialog on the Friends page and on the DM welcome, phone and desktop, three themes; the long text fits inside the dialog at 375.

### SEC-AUD-156 (tail): the other Lounge home pages at 375 px
Checked in the same spec at 375x812 (and 1280x800): DM welcome of a friend, Message Requests (list, and one request opened by link), Add Friend tab, Pending tab, inbox For You.
- Fixed in `shell.css`: the Message Requests list row (Accept, Ignore, Spam, Block) ran over the name on a phone; the buttons now wrap under the name. The inbox popover content (`.lng-inbox`, `msg.css`: 420px) was wider than the kit popover's 360px max (336px inner), so the Decline button of a friend request was cut off, also on desktop; `shell.css` now sets `.lng-root .lng-inbox` to 336px (and viewport-limited). It is a CSS override from my file: the cause is `msg.css` (`inline-size: min(420px, 90vw)`), not mine; the commander may prefer to fix it at the source.
- DM welcome, Add Friend, Pending, the request view: fit with no change.

## Not done / found but not mine (not fixed)
1. **On a phone, tapping Friends or Message Requests in the Home sidebar does not reveal the page.** `LoungeShell.tsx:242,281`: the shell switches to the `chat` pane only when a `channelId` (or the scrapbook) is in the route; Friends and Message Requests have none, so the page sits in the off-screen main pane (`lounge.css:1522`, `translateX(100%)`) and can only be reached by swiping. This makes the Friends page on a phone nearly unreachable for a touch user. Suggested fix: in `LoungeShell.tsx` also `setPane('chat')` when the home view is Friends or Requests and the user picked it (needs a callback from `HomeSidebar.goHome` / the Friends link, or an effect on `homeView`). The spec works around it with a synthetic swipe (`swipeToMain`). Owner question 1.
2. **The DM channel header on a phone is crowded**: 11 controls (back, avatar, call, video, bell, pin, bookmark, plus, members, inbox, info) leave the name zero width and the small avatar overlaps the call icon (`dm-welcome-phone-*.png`, `ChannelHeader.tsx` / `lounge.css:790-840`). Not in my files; nothing overflows the page. Owner question 2.

## Visual check
Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p63-phone` (56+ PNGs: `friends-all`, `friends-row-menu`, `friends-pending`, `friends-add`, `confirm-block-friends`, `confirm-block-dm`, `dm-welcome`, `message-requests`, `message-request-one`, `inbox-for-you`, each `-desktop|phone-lounge-editorial|light|dark`). Base-run shots (before the change) were removed. Spec `e2e/tests/p63-phone-shots.spec.ts` (`SR_P63_PHONE=1`, optional `SR_P63_PHONE_OUT`, `SR_P63_PHONE_SEED` to reuse seeded accounts: registration allows 20 per address per day).
Opened and looked at myself: friends-all, friends-row-menu, confirm-block-friends, friends-add, friends-pending, inbox-for-you (phone editorial/light/dark, desktop editorial/dark), dm-welcome (phone dark), message-requests and message-request-one (phone editorial/light/dark, desktop light), friends-all and friends-row-menu desktop editorial against `p62-web/confirm-remove-friend-desktop-lounge-editorial.png` (same header, rows, Active Now column). Clicked through: tab All, row menu, Block (Cancel), Pending, Add Friend, DM welcome Block (Cancel), Message Requests, the request view, inbox For You, and on the phone Remove Friend with a real confirm (row gone).

## Tests
- vitest `p63BlockDialog.test.tsx`: passes (single file).
- Playwright `p63-phone-shots.spec.ts` (1 test, 3 themes x 2 viewports): passes with the change; fails on the base `shell.css` (watched).
- web `tsc --noEmit`, eslint (FriendsPage, ConfirmAction, the new test), prettier on all changed files: clean. `node scripts/i18n-audit.mjs`: TOTAL 0. Not run (CI): bundle and entrance checks (nothing public or first-paint touched), the full vitest suite.

## Proposed inventory rows (after SEC-AUD-153)
- SEC-AUD-154: Friends page fits a 375 px phone (wrapping header, scrolling tab row, truncating rows, icon-only Message button) and its row menu opens inside the screen. `shell.css`, `FriendsPage.tsx`, spec.
- SEC-AUD-155 (D-640): the Block dialog says that in servers and group DMs the blocked person can still write and their messages are hidden until shown (`lounge.confirm.blockBodyFull`).
- SEC-AUD-156: Message Request rows wrap on a phone; the inbox For You content fits its popover; the Home pages checked at 375 px. Open: the Friends and Message Requests pages are not revealed by a tap on a phone (LoungeShell pane), the crowded DM header.
- Threat-model rows: none (no authorization, E2EE or audit behaviour changed).

## Owner questions (for the Fable agent)
1. A tap on Friends / Message Requests on a phone should open the page (Discord does). Fix in `LoungeShell.tsx` / `HomeSidebar.tsx` in the next wave? Proposed default: yes.
2. The DM header on a phone: which controls move into an overflow menu so the name shows? Proposed default: keep back, avatar and name, call, and a "more" menu for the rest.
3. Delete the now unused `lounge.confirm.blockBody` from `p62-web-1.ts`? Proposed default: yes at integration.

## Cleanup
Stack stopped by PID (sr-server, Vite on 5411), heavy lock released, database `sr_p63_phone_ui` dropped, `p63-phone-target`, `p63-phone-bin` and the run folder deleted.
