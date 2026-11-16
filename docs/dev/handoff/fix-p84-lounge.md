# Wave 84 handoff: lounge (SEC-AUD-354 D-858, SEC-AUD-355, SEC-AUD-356 D-859)

Builder: lounge. Branch `fix/p84-lounge` (base `b2f3747b`). No Rust, no cargo, no web build. One new i18n fragment (`p84-lounge-1.ts`).

## Clock

T = 13:20 on 2026-11-16. No new item from 15:05, checkpoint 15:25, finished 15:50, hard limit 16:05.
One lock round: took the lock 13:33, released 13:47 (14 min): stack, seed of the sweep world, base run (my three files held back with
`git show HEAD~1:<file>`, no stash), the probe for item 2, after runs of items 1 and 3, a fix of item 3 and its re-run, stack stopped, database
dropped. Handoff written 13:50.

## Done (commits)

- `3b755f68` SEC-AUD-354, SEC-AUD-356: `lounge.css` block, `profile.css` block, `ProfilePopout.tsx` button, `p84-lounge-1.ts`, vitest case, the spec.
- `bd9e1319` SEC-AUD-356: the button's final CSS (it took room in the first version: see item 3), the probe in the spec.
- This handoff.

## Item 1 (SEC-AUD-354): the channel header on a phone

**Base (ran it, `SR_P84_BASE=1`, 375x812 touch, Lounge Dark and Light, a text channel):** Back, Threads, Notification Settings, Pinned Messages,
Saved items, Member List, Inbox and Help are 28x28 and **fail** the hit test (`elementFromPoint` at centre -21 and +21 is not the button); only Search
(28x28 box, `::after` at -8px) passes. Header box 0,29 375x49, title 84,43 148x20 (identical in Light).

**Build (`lounge.css`, appended at the end):** `.lng-root[data-mobile] .lng-header .lng-icon-btn:not(.lng-phone-search)` is `position: relative`, with an
`::after` `inset-block: -8px; inset-inline: -4px` (a 44 px tall target inside the 48 px header, 4 px into the 4 px gap, never to a neighbour's centre).
The 28 px box stays (the header is not tall enough to give each button a 44 px box without moving the title). Search's own rule is untouched (the
`:not()` keeps mine off it).

| button | box (375, base and after) | hit test base | hit test after |
|---|---|---|---|
| Back to channels | 16,39 28x28 | fail / fail | pass / pass |
| Search this conversation | 107,39 28x28 | pass / pass | pass / pass (its -8/-8 target kept) |
| Threads | 139,39 28x28 | fail | pass |
| Notification Settings | 171,39 | fail | pass |
| Pinned Messages | 203,39 | fail | pass |
| Saved items | 235,39 | fail | pass |
| Member List | 267,39 | fail | pass |
| Inbox | 299,39 | fail | pass |
| Help | 331,39 | fail | pass |

(Dark and Light give the same rows; the full lines, with the centre test true for every button after, are in
`visual-sweep-shots/p84-lounge/measure.json`, the base in `base-measure.json`.) Header height (49) and the title's box (84,43 148x20): 0 px change.
A tap on Pinned Messages and on Inbox opens them (`aria-expanded` true, popover open; shots). At 1280 the header buttons' boxes are identical to the
base (644,63 28x28 ..., no `::after`).
**Not covered:** a DM's header and a voice channel's. The world's DM page showed only Back in both runs (base and after), so the DM header's own buttons
were not measured; they carry the same class and take the rule, but I did not measure them. A voice channel was not opened.
**Observation, not mine:** at 375 the channel title is squeezed to "g." (the eight 28 px buttons take the row); the base is the same.

## Item 2 (SEC-AUD-355): the "More" menu: case 2, the kit's placement; nothing built, listed

The planner's reading (toolbar `display: none` after the pointer leaves, trigger 0x0, menu jumps to 8,4) is **not** what puts the menu at the corner.
Measured on the base at 1280x800, Lounge Dark, message 4 of 7 (box 312,329 724x174), pointer left on the message (`probe-still-*`, no move onto the menu):

| time after the click | trigger box | `.lng-msg-tools` display | menu box |
|---|---|---|---|
| 0 ms (the click) | 989,316 28x28 | flex | **8,4 200x560** |
| 150 ms | 989,316 28x28 | flex | 8,4 200x560 |
| 1000 ms | 989,316 28x28 | flex | 8,4 200x560 |
| pointer moved off the message (400 ms later) | 0,0 0x0 | none | 8,4 200x560 (stays) |

So the trigger **keeps its box and the menu is at the corner from the first frame**: its inline-end edge is 809 px from the trigger's. (My first flow, with
the pointer moved onto the menu's first item, logged the same at the click: 8,4 with the trigger at 989,316; "Add Reaction" is the first item and its
sub-menu then showed at 203,11 200x254, which my first log took for the menu.) The toolbar does hide when the pointer leaves, and the trigger is then
0x0, which would matter to a re-measure. A `:has([aria-expanded='true'])` rule would keep the toolbar shown (the kit's `DropdownMenu` sets
`aria-expanded` on the trigger while open, `Menu.tsx:546`), but it does not touch the cause, so per the contract **I changed nothing**.
The menu is 560 px tall: it fits neither above (316) nor below (456) the trigger in 800 px. `position.ts` flips and then clamps the main axis when
neither side fits (`position.ts:126-128`); the x of 8 comes out of that path or out of the first measure in `MenuSurface`/`useFloating` (not found by
reading; the planner's `0 - 200` arithmetic is for a 0x0 anchor, and the anchor span has a real box). **For the kit's next wave:** the menu's x must be
the trigger's (bottom-end: right edge 1017 - 200 = 817); y pinned to the viewport edge is right. The toolbar's emoji picker (a `Popover`, trigger
849,316) opens at 481,146 360x369 and stays there at 0, 300 ms and 1 s: **no fault**. The right-click menu is not a fault (not touched).
Evidence: `base-measure.json` (`base-more-*`, `base-probe-*`, `base-emoji-*`), shots `base-more-menu-open.png`, `base-more-menu-1s.png`,
`base-probe-pointer-away.png`, `base-emoji-menu-open.png`, `base-emoji-menu-1s.png`.
**SEC-AUD-348's "More" menu half stays open** (wave 85, kit); its Modal-close half is the kit builder's (SEC-AUD-351).

## Item 3 (SEC-AUD-356): a close button on the phone profile pop-up

**Base (ran it):** at 375 the pop-up from a message's author (box 25,134 342x670) and from the member list (25,72) has buttons "Rowan (owner) has a
ring", "Add role", "View Full Profile" and **no "Close"**. First focus after opening: the anchor (`button.lng-avatar-btn` / `button.lng-member`); an
outside tap leaves focus on `body`.

**Build.** `ProfilePopout.tsx`: `<button className="lng-icon-btn lng-pop-close" aria-label={t('lounge.profilePopout.close')} onClick={() => close()}>`
with the `close` icon (size 20), the first child of `.lng-pop` (bridged cards too); `close` is the store's `closeProfile`, what Escape and an outside
tap call. Label: no existing key is the profile's own (the "Close" keys that exist are other features' labels, such as `lounge.followButton.close`), so
the new fragment `apps/web/src/i18n/messages/en/p84-lounge-1.ts` has `lounge.profilePopout.close: 'Close'` (**the commander runs
`staff-keys.mjs --write`**). `profile.css`: `.lng-pop .lng-pop-close { display: none }`, and in the phone block (`@media (max-width: 640px)`, the one
wave 83 added to) it is `sticky` (stays in view when the pop-up scrolls), `display: grid`, 44x44, `margin-block: 8px -52px` (takes no room), at the
inline-end top, `border-radius: 50%`, ground `var(--ui-bg-overlay)` (Lounge Dark rgb(43,45,49), Light rgb(255,255,255), probed), icon `var(--text-normal)`,
a 1 px `--ui-border` ring, `z-index: 2`. A first version used `float`: it made the following blocks 52 px narrower (name 256 instead of 308), caught by
the before/after boxes and replaced.

**Figures (375, final CSS):** button 314,143 44x44 (from a message; 314,81 from the member list), inside the pop-up, hit test pass above and below,
icon contrast on its ground **10.21:1 Dark, 12.64:1 Light** (the banner colour does not matter: the ground is opaque). Avatar, name, handle, badges,
banner and the pop-up: **0 px** against the base (message: avatar 42,155 80x80, name 42,247 308x24, banner 26,135 340x60, pop 25,134 342x670; members:
pop 25,72). Focus after opening: the same as the base (the anchor). A real tap closes it from a message's author; from the member list a **mouse click**
closes it (after the swipe the synthetic touch gives no click, as wave 83 said; the log says "tap sent no click (after the swipe); mouse click"). After
the close, focus is on the anchor (the pop-up's cleanup refocuses it), where the base's outside tap leaves `body`: a close by the button returns to what
opened it. "View Full Profile" still opens the full profile. At 1280 the button is `display: none` (box 0x0) and avatar 393,29, name 393,121, banner
377,9 and the pop-up 376,8 342x786 are identical to the base. Dark and Light, both entry points at 375.
**Vitest:** `lounge/__tests__/profile.test.tsx` (the file that renders `PopoutHost`), one case more ("has a Close button in the pop-up that closes it"):
8 passed. Not run on the base (I wrote the case with the change; the base has no such button, so it fails there by construction).

## Who loads what (lazy?)

- `lounge.css`: `LoungeShell.tsx` (via `routes/LoungeRoot.tsx`, `lazyNamed`), `settings/server/ui.tsx` (ServerSettings, lazy via `routes/settingsServer.tsx`),
  `settings/user/parts.tsx` (the member's Settings sections, behind the lazy `UserSettings`). My rules are all under `.lng-root[data-mobile] .lng-header`:
  they cannot reach a Settings page. No first-paint or public file changed; no web build run.
- `profile.css`: only `ProfilePopout.tsx` (Lounge chunk). Its `.lng-pop-close` rules are scoped by `.lng-pop` (the pop-up is portalled out of `.lng-root`).

## Shots

Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p84-lounge` (25 PNGs, `measure.json`, `base-measure.json`), **all 25 opened**:
header 375 Dark and Light before and after (4), 1280 Dark and Light before and after (4), taps on Pinned Messages and Inbox before and after (4), the
"More" menu before (2) with the probe (1) and the emoji picker (2), profile before (message 375 Dark, members 375 Dark, 1280 Dark: 3) and after (message
and members, 375 Dark and Light: 4; 1280 Dark: 1). The header looks the same before and after (the targets are invisible, as meant); the close button
reads on the blue banner in both themes. No shot of item 2 after (nothing built).

## Tests and checks

web `tsc`: clean. eslint (the package's own) on `ProfilePopout.tsx` and the test: clean. prettier on all my files: clean. `i18n-audit --max 0`: 0.
`check-css-tokens`: clean. e2e `tsc`: clean. Spec `e2e/tests/p84-lounge-shots.spec.ts` (modes `header`, `more`, `profile`, `probe`, `all`; `SR_P84_BASE=1`
for the base): run on the base and after, line reporter, passed each (it measures and writes lines, no assertions). `check-doc-links`: run at commit.
Docs: no doc states the header's target size or the pop-up's controls: nothing to update.

## Proposed inventory rows

- **SEC-AUD-354** (closed): the channel header's eight buttons had a 28 px target on a phone (only Search 44); they have a 44 px tall target now (0 px box
  change). Not measured: a DM's header and a voice channel's (same class).
- **SEC-AUD-355** (**open, kit**): the message "More" menu opens at 8,4 (200x560), 809 px from its trigger, from the first frame, with the trigger present;
  not a toolbar-hover fault. The cause is the kit's placement for a menu taller than both sides of the trigger. Figures above.
- **SEC-AUD-356** (closed): a close button on the phone profile pop-up (44x44, 10.2 to 12.6:1, 0 px moved).
- **SEC-AUD-348**: the Modal half closes with SEC-AUD-351; the "More" menu half stays open (above).

## Changelog sentences

- (354) On a phone the buttons in a channel's header are easier to hit with a finger.
- (355) Not changed: the message "More" menu still opens at the top-left corner at 1280 (the kit's placement; wave 85).
- (356) On a phone a member's profile pop-up has a close button.

## Owner questions (for the Fable agent)

1. The title at 375 is cut to "g." by the header's buttons. A channel name needs more room (for example the Threads, Notification and Saved buttons
   moving into the "More" menu the DM header already has). Default: leave it.
2. The profile pop-up's close button returns focus to the opener, unlike an outside tap (which leaves `body`). Default: keep it.

## Cleanup

Stack stopped by PID (ports 8622, 5622, 9322 free), lock released, `sr_p84_lounge_ui` dropped, `p84-lounge-bin` deleted (`p80-base-bin` untouched). No stash
used or touched. `e2e/.state/sweep` is ignored and untracked.
