# fix-p105-title (D-1007, SEC-AUD-495)

Clock: started 15:30, finished about 15:40.

## Done
- One commit: `D-1007 lounge.css (ALL READY)`. In `.lng-topic` (lounge.css:837) added `flex: 0 9999 auto`. Nothing else touched: `.lng-header-title` rules unchanged, so the Friends, Message Requests, scrapbook and DM titles are not reached.

## Reading (confirmed by reading, not measured)
Title and topic are both default flex items (shrink 1, `min-width: 0`), so they shrank together in proportion to their widths: the reading in the task is right. Row, in order: back button (phone only), icon or avatar (flex none), title (h1, or the DM identity button holding avatar and title), optional badge, separator, topic button, then `.lng-header-tools` (flex none, margin-inline-start auto; follow, threads, notify, pins, saved, search box 144px, members toggle). With shrink 9999 on the topic against 1 on the title, the topic takes the missing room until it is 0, then the title cuts. If loungeshots finds the topic already 0 at 1024 with the list open and the title still cut, the cause is the tools' width: revert nothing needed, say so with figures (planner decides which tool gives way).

## Not done
No measurement (no spec, no shots, no build for this role); loungeshots measures. No tsc: no .ts/.tsx changed ("not needed").

## REACH table
| file:line | element | where | rule now | found by class in e2e |
|---|---|---|---|---|
| lounge.css:837 | .lng-topic (ChannelHeader.tsx:152) | channel header, any channel with a topic | new flex: 0 9999 auto | none (grep of e2e and __tests__ finds no .lng-topic) |
| (not changed) lounge.css:825, 864 | .lng-header-title users: ChannelHeader.tsx:132,143; scrapbook/Gallery.tsx:349; shell/FriendsPage.tsx:360; MessageRequestView.tsx:97; MessageRequestsPage.tsx:138; shell.css:619 (.lng-friends); lounge.css:2193 (phone [data-compact]) | | untouched | e2e p64:179,225; p84:180; p85:199 (old specs, read only) |
Elements the rule can move: only the topic and the title/separator x when the row is short of room. `.lng-root[data-mobile] .lng-topic` is display none (lounge.css:1592) so the phone header is not reached; the `[data-compact]` title rule is untouched. A topic in a DM header (if any) would also shrink first; DM header is expected unchanged where it fits.

## JOBS
none.

## Checks (on last commit)
prettier --check lounge.css: ok (exit 0). node scripts/check-css-tokens.mjs: clean (exit 0). tsc, vitest: not run (CSS only).

## Imports
lounge.css is loaded by the lazy Lounge chunk (not first paint, not a public page).

## Closing text proposal for SEC-AUD-495
Fixed (D-1007): the channel header's topic now gives way to nothing before the title is cut (topic flex-shrink 9999 against the title's 1); tools unchanged. Pending loungeshots measurement.
Changelog: The channel title in the Lounge header now stays whole while the topic shrinks first.

## Owner questions
none (Discord-like default: topic yields first).

## PROCESSES
strays: none found
