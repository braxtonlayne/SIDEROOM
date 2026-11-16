# Wave 85 lounge: SEC-AUD-368 (D-872), a channel's header on a phone

T = 15:00 on 2026-11-16. No new item from 16:00, checkpoint 16:15, finished 16:30, hard limit 16:45. Done at about 15:40.

## Done
- Commit `SEC-AUD-368` (code, vitest, spec) plus this handoff.
- `lounge/ChannelHeader.tsx`: `compact = onBack && !dmHeader`; `<header data-compact>`; the "More" button (`lng-dm-more`, existing string `lounge.channelHeader.moreActions`) renders for a DM header or a compact one. Same wrapper `div.lng-dm-extra`, same buttons, no new string.
- `lounge/lounge.css`: "More" `display: none` for `[data-compact]` too (outside the media query); inside `@media (max-width: 720px)`, after the DM block: `.lng-header[data-compact]` is `position: relative`, the title is `flex: 1 1 0; min-inline-size: 0`, the tools lose their auto margin, "More" is shown, the wrapper is hidden until `data-open`, then it is the strip under the header with `padding: 12px` (8 px target reach + 4 px) so every 44 px target stays inside the strip. A DM's rules are untouched. The desktop search box stays hidden on a phone (`.lng-root[data-mobile] .lng-search`; measured `searchBoxShown: false`).
- `lounge/__tests__/p71Panels.test.tsx`: one case more (with `onBack`: "More actions" button, `data-compact`, `aria-expanded` true after a click, wrapper `data-open`; without `onBack`: none). 18 of 18 pass (single file). Not watched failing on the base (it needs the new attribute; I did not run it on the base).
- Spec `e2e/tests/p85-lounge-shots.spec.ts` (`SR_P85_LOUNGE=1`, `SR_P85_BASE=1` for the base).

## What I saw on the base (run, `SR_P85_BASE=1`, files held back from base 774d3073)
375x812 touch, Lounge Dark and Light: title "general" box 15x20 (scrollWidth 59 > clientWidth 15), drawn "g."; nine buttons in the row (Back, Search, Threads, Notification Settings, Pinned Messages, Saved items, Member List, Inbox, Help), each 28 px with a 44 px tall `::after` target; all passed the hit test. 30-letter channel: title 15 px as well.

## Table (375, same in both themes)
| Button | Base | After | Hit test (+-21 px) |
|---|---|---|---|
| Back | row x16, 28x28, target 12,31 36x44 | same | pass |
| Search | row x107 | row x307, target 299,31 44x44 | pass |
| More | none | row x335, 28x28, target 331,31 36x44 | pass |
| Threads, Notification Settings, Pinned Messages, Saved items, Member List, Inbox, Help | row x139..335 | strip y89, x12, 44, 76, 108, 140, 176, 208, 28x28, targets from y81 (strip top 77, header bottom 78) | all pass; no target covers a neighbour's centre; none reaches above the strip |

Title box: base 15x20 (scroll 59) -> 215x20 closed (`general` whole, scroll 215/215); the 30-letter name 215x20 with scrollWidth 257 (ellipsis in the shot). Header height 49 closed, same as base (0 px). Header and document `scrollWidth == clientWidth` (doc 0). Strip open: height 53 at y77, absolutely placed, so the page does not move.
Flow (both themes): tap More -> `aria-expanded` true; second tap -> false; Escape -> false. From the strip, Pinned Messages and Inbox each open a popover inside the viewport (box 8,125 359x111 and 359x226; the base's were at y75, the strip pushes them 50 px down, still inside).
Member List: after a tap `aria-pressed` stays "false" and `data-pane` stays "chat" on the base and after (the pane is one swipe away; the button does what it did). Unchanged, not fixed.
1280: header 724x49, button boxes identical to the base, More `display: none`.
DM header at 375: **not measured as a conversation.** The world's only DM (and the one my spec makes with a member) is a pending message request: the page is "Message Requests" with no DM header (shots identical before and after, 0 px moved). The DM CSS and render are unchanged (`dmHeader` is the old condition; `compact` is false for a DM), so a DM's header cannot move, but I did not shoot it. Listed as not done.

## Who loads the files
`ChannelHeader.tsx`: `ChannelView`, `voice/StageView`, `voice/VoiceChannelView` (Lounge chunk: `routes/lounge.tsx` lazy `LoungeRoot` -> `LoungeShell`). `lounge.css`: `LoungeShell` (lazy), `settings/server/ui.tsx` and `settings/user/parts.tsx` (settings routes, lazy; `routes/settingsServer.tsx` lazy). All my rules are under `.lng-header[data-compact]`, which only a header with `onBack` has. No first-paint file touched; no web build.
`p64-phone-shots.spec.ts` would still pass (read): its DM assertions use `.lng-header[data-dm]`; its channel part only logs the title width and shoots.

## Shots (folder `visual-sweep-shots/p85-lounge`, 27 files incl. two measure files = 25 PNG)
Before/after closed, Dark and Light; open, Dark and Light; Pinned and Inbox from the strip (Dark); long name (Dark, plus base); Member List after tap (Dark, Light); 1280 before and after (Dark, Light); DM (Message Requests page) before and after. I opened 22 of the 25 (all measured-relevant ones; not opened: base desktop light, base light dm, after desktop dark dm).

## Checks
web tsc, eslint on my files, prettier, `i18n-audit --max 0` (0), `check-doc-links`, e2e tsc: clean. Spec run 1.2 min, passed (it measures only).

## Proposed
- Inventory row SEC-AUD-368: "On a phone a server channel's header keeps Back, the mark, the name and Search plus a More button; the other seven buttons open in the strip under the header (D-872). The name has 215 px at 375."
- Changelog: "A channel's name now has room on a phone: the header keeps Back, the name, Search and More, and the other buttons open from More."
- Owner questions: which buttons, if any, should stay in the row beside Search (Member List and Inbox are candidates)? Should Member List on a phone open the members pane instead of a hidden toggle?
- Not done: a real DM's header shot (needs an accepted conversation, which the sweep world lacks). Stashes: none touched.

## Cleanup
Stack stopped by port owner PID, lock released, DB `sr_p85_lounge_ui` dropped, `p85-lounge-bin` deleted (never `p80-base-bin`).
