# fix-p107-msgs (D-1022, wave 107, pod lounge A)

## Clock
Started 18:37; REACH table and both files committed by 18:45; sheets only (no job run, by task).

## Done
- `apps/web/src/lounge/profile/profile.css`: `.lng-avatar-btn` tap area (commit `D-1022 profile.css (READY)`).
- `apps/web/src/lounge/msg/msg.css`: link buttons, reaction pills, tab bars, inbox filters (commit `D-1022 msg.css (ALL READY)`).
- `lounge.css` not touched: the `.lng-reaction` rule needed no change (the phone block sits in msg.css next to the burst/add rules; it only adds a `::after` and sets `margin-top`/`row-gap` on `.lng-reactions` through `.lng-root .lng-reactions`, specificity 0,2,0 over lounge.css's 0,1,0).
- All phone rules are `@media (max-width: 720px)` (the block lounge.css already uses at :2229; msg.css had none). Not `.lng-root[data-mobile]`, because the picker/inbox tabs live in popovers that may sit outside `.lng-root`. Nothing changes at 1024/1280.

## REACH table
| control | rule now | page / how opened | cure | what moves (phone only) |
|---|---|---|---|---|
| `.lng-avatar-btn` 40x40 | profile/profile.css:128 (`display:block`, no size) | channel/thread/forum post/DM message, non-grouped | A.1, `::after` inset-block -2px | nothing drawn; 2px tap area above/below; the name button is beside it (inset-inline 0) |
| `.lng-name-btn`, mentions, spoilers, message links, `.lng-embed-title a` | profile.css:120, markdown.css, msg.css:297 | message text/head | accepted (A.4: inside a line of running text) | none; census counts: name x11 (Juniper x4, Rowan x7), mention 1, spoiler 1, links 3, embed title 1 |
| standalone `.lng-link-btn`: "Edit Channel" (`.lng-welcome-actions`, MessageList.tsx:645), "N message requests waiting" (`.lng-dm-empty`, HomeSidebar.tsx:296) | msg/msg.css:10 (text, 20px high) | channel welcome row (needs manage-channels); Home list with no DMs | A.1, `::after` inset-block -12px, scoped to those two containers | nothing drawn; every other `.lng-link-btn` use (Attachments, Embeds, Pins, Saved, Tickets, retry in a message, hidden-run, etc.) is unchanged and not claimed |
| `.lng-reaction` pills (and `.lng-reaction-add`, same class) 24px | lounge.css:1172 (`height:24px`), msg.css:924 | message with reactions | A.1 `::after` inset-block -10px (not while `data-burst`, so the burst ring keeps its own `::after`); row `margin-top` 4 -> 8px; wrapped lines `row-gap` 4 -> 20px (A.3) | +4px per message with reactions; +16px per extra wrapped line of pills. The area reaches 10px above a pill, chip bottom is 8px above the pill, so only the chip's lowest 2px are covered, not its centre line (chip about 26px high, centre 13px up from its bottom) |
| `.lng-expr-tabs button` 32px | msg/msg.css:998 | GIF/emoji/sticker picker tabs and inbox tabs | A.2 `min-block-size:44px` | tab bar +12px |
| `.lng-inbox-filters` label rows | msg/msg.css:1209 (gap 8px 12px) | Inbox popover | A.2: each label `inline-flex`, centre-aligned, `min-block-size:44px`, `gap:4px` (was the space character), `row-gap:0` | each row 44px (was about 17-20px plus 8px gap); input not moved horizontally by more than the old space width |

## FINDERS
- Classes: `git grep` in `e2e/` and tests: `.lng-reactions` (p78-emoji-shots.spec.ts:275, retired shots spec, a locator only), `.lng-reaction, [aria-pressed]` (polish-lounge-flows.spec.ts:82, a visibility check, not touched by a rule). `lng-avatar-btn`, `lng-expr-tabs`, `lng-inbox-filters`: none in e2e or `__tests__`. `lng-link-btn`: used by role/text in tests, no style assertion. Files read as text: `styles/__tests__/textLink.test.ts` (msg.css; 8 passed) and `tools/__tests__/overlayZoom.test.tsx` (profile.css; 12 passed). Importers: msg.css by Composer.tsx and MessageList.tsx (Lounge lazy chunk), profile.css by ProfilePopout.tsx (Lounge lazy chunk). No first-paint file.
- No tag, id, label, role changed.

## Figures vs the task's lists
Not measured by me (no spec, by task); `loungeshots` measures. Expected: see "what moves". Must-not-change: no rule is outside the 720px media; no text line, author line or thread chip box changes. Risk to watch: reactions row `margin-top` also applies in thread/forum views (same class); the `.lng-reaction:not([data-burst])::after` area may cover the lowest 2px of a thread chip above.

## Checks (own tree, last commit)
prettier --check both files: exit 0; `node scripts/check-css-tokens.mjs`: exit 0 (73 clean); vitest textLink.test.ts 8 passed, overlayZoom.test.tsx 12 passed (one file at a time). Not run: tsc, build, shoot (task forbids).

## JOBS
none.

## Closing text / proposed rows
Closing: avatar, standalone link buttons, reaction pills, tab bars and inbox filters have 44px targets on a phone. Proposed new rows (faults base had, not mine): `.lng-name-btn`, mentions, spoilers and message links stay below 44px by A.4; the thread chip (threads.css) and `.lng-msg-tools` are another builder's.
Changelog: On a phone, avatars, reaction pills, picker and inbox tabs and the inbox filters now have 44 px touch targets.
Owner questions: none.

## PROCESSES
strays: none started by me (no process was started).
