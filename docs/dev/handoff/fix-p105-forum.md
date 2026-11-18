# fix-p105-forum (D-1006, SEC-AUD-491)

## Clock
Started 15:30, code done 15:31, committed 15:32. No job (tsc not needed: only a .css file changed).

## Done
- `apps/web/src/lounge/forum/forum.css`: one new block at the end, under `.lng-root[data-mobile]`:
  `.lng-forum-search { min-block-size: 44px; }` and `.lng-forum-views .lng-icon-btn { min-inline-size: 44px; min-block-size: 44px; }`.
- Chose the real 44 px box (the default), not the D-858 pseudo-element: the toolbar wraps on a phone, so growth costs only a little height. `.lng-icon-btn` sets width/height 28; the min-* values win.
- The toolbar's `align-items: center` is not restyled.

## Not done
Nothing. No spec, shots or build (loungeshots measures).

## REACH table
| element | file:line | label/id | reached by | styled by now | found by class in e2e/__tests__ |
|---|---|---|---|---|---|
| search field | ForumView.tsx:213 | `.lng-forum-search` input | open a forum channel in the Lounge (as p104-lounge-shots does) | forum.css `.lng-forum-search` (min-width 160, padding 6px 10px) | p102/p103/p104 lounge specs only (see below); no unit tests |
| two view buttons | ForumView.tsx:230ff | `.lng-forum-views .lng-icon-btn` (aria-pressed) | same | lounge.css `.lng-icon-btn` 28x28 | same |
| Sort select | ForumView.tsx ~:220 | `.lng-forum-select` | same | unchanged, already 44 at 375 | - |
| New Post button | ForumView.tsx | unchanged | same | unchanged | - |
| tag chips, post list | below the toolbar | `.lng-forum-chips`, posts | same | move down by the growth (375 only) | - |
Dependants: `git grep -n "lng-forum-search|lng-forum-views|lng-forum-tools"` finds only ForumView.tsx and forum.css (no other consumer). `.lng-icon-btn` is only restyled via the selector `.lng-forum-views .lng-icon-btn`, so no other icon button is reached. ForumView is imported by ChannelView.tsx (lazy lounge chunk).

## Jobs
none.

## Figures
Not measured by me (loungeshots does it). Expected: 375 only, search 29 -> 44, view buttons 28 -> 44 each; 1024/1280 identical.

## Checks
- prettier --check forum.css: passed (15:31, exit 0).
- node scripts/check-css-tokens.mjs: passed, 73 files clean (15:31, exit 0).
- Unit tests rendering ForumView: none exist (git grep), so not run.
- tsc: not needed.

## Closing text for SEC-AUD-491
On a phone (`.lng-root[data-mobile]`) the forum toolbar's search field is 44 px tall and each view button a 44 x 44 px box (D-1006); pointer widths unchanged. Pending loungeshots' hit test.

## Changelog sentence
The forum toolbar's search field and view buttons are 44 px touch targets on a phone.

## Owner questions
None.

## PROCESSES
strays: none found
