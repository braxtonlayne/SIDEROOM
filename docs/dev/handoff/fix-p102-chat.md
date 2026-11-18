# Wave 102 handoff: chat (code builder, pod media; SEC-AUD-453, D-958)

Started 09:30, written 09:40. No stack, no lock, no spec, no shots.

## REACH table
| # | file:line (base) | select | opens at | sheet rule now | theme | flex label | found by class/id in tests | result |
|---|---|---|---|---|---|---|---|---|
| 1 | `apps/web/src/lounge/header/FollowButton.tsx:126` | "Channel" picker (first option `lounge.followButton.choose`, optgroups per category) | Lounge, an announcement channel the member can follow: channel header, Follow button, modal "Follow" opens; needs `groups` loaded (the member's own channels in a guild) | `.lng-follow-select` `header/header.css:123` (min-height 36, padding-inline 8, border, bg) | Lounge Light/Dark | label `.lng-follow-field` is `display:flex; flex-direction:column` (wrapper span is a column item: full width, like the select was) | none | MOVED; class `lng-follow-select` dropped (its padding-inline 8 would sit on the kit's 36 px chevron gutter) |
| 2 | `apps/web/src/lounge/SearchBox.tsx:358` | Sort ("Sort by", sr-only label) | Lounge, channel header search box (or phone search), type a query with results: the bar above the results | `.lng-search-sort select` `msg/msg.css:779` (element selector: height 28, border, radius 4, bg `--bg-base-lowest`) | Lounge Light/Dark | plain `<label class="lng-search-sort">` in `.lng-search-bar` (flex, space-between) | none | MOVED, no class. The element rule still matches the inner select: its `height:28px` loses to the kit's `min-block-size` (40; 44 on coarse), but its border/radius/background override the kit's. MEASURE height (must be 40, 44 at 375) and the look; if not, put back to raw. msg.css is not mine |
| 3 | `apps/web/src/lounge/forum/ForumView.tsx:221` | Forum "Sort" (activity/created) | Lounge, a forum or media channel, toolbar above the posts | none on the select (`.lng-forum-select` `forum/forum.css:36` is the label) | Lounge Light/Dark | **inline-flex label** with a text span (the wave 101 Library ratings case) | none | LEFT RAW: new inline-flex label, the rule says list it |
| 4 | `apps/web/src/lounge/components/EmojiPicker.tsx:305` | skin tone (`aria-label` skinTone) | any emoji picker (composer, reaction, custom status) | none (`.lng-emoji-tone` has no rule); row `.lng-emoji-top` `lounge.css:2056` flex, gap 6, beside `.lng-emoji-search` (height 32, no width) | Lounge | flex ROW, not a label | none | LEFT RAW: the wrapper span has `inline-size:100%` and would be a flex item next to an auto-width input, taking about half the row; the 32 px bar would grow to 40 |

## Chunks
- `FollowButton`: imported only by `ChannelHeader` (`ChannelView`, voice/stage views), all under `LoungeShell`, which is under `routes/lounge.tsx`'s lazy `LoungeRoot`: lazy chunk. Not first paint, not public.
- `SearchBox` (lounge): `ChannelHeader`, `header/PhoneSearch`: same lazy Lounge chunk. (`library/SearchBox` is another file.)
- `ForumView`: `ChannelView`: Lounge chunk. `EmojiPicker`: Composer, MessageItem, ExpressionPicker, Reactions, CustomStatusDialog: Lounge chunk (`app/main.tsx` imports only `lounge/voice/IncomingCall` and `VoiceBridge`, which do not import it).
- `.lng-field`: none of my selects sit under a `.lng-field`.

## Done
- `SEC-AUD-453 FollowButton.tsx, SearchBox.tsx (READY)`: two selects moved.
- Unit tests that render them, unedited: `p71Panels.test.tsx` 19 passed, `p70Lines.test.tsx` 25 passed. eslint on both files exit 0; prettier applied.

## Still raw (SEC-AUD-453 text)
`ForumView.tsx` sort select (inline-flex label) and `EmojiPicker.tsx` tone select (flex row beside an auto-width input); both need a sheet change (lounge.css / forum.css, not mine): e.g. `.lng-emoji-tone`'s wrapper given `inline-size:auto`.

## STACK-ROUND: none (code builder)

## TYPECHECK
`bash tc.sh run chat bash -c "cd ./apps/web && npx tsc --noEmit -p ."` at 09:39: exit 0 (once, after the last code edit). Also: eslint on both files exit 0; `node scripts/i18n-audit.mjs --max 0` exit 0 (TOTAL 0); `node scripts/check-doc-links.mjs` exit 0.

## Proposed rows
None. Changelog sentence: the Follow-channel picker and the search-results sort in the Lounge now use the kit select. Owner questions: none.

Faults from mediashots: none received at time of writing.
