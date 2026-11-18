# fix-p103-forum (wave 103, code builder, pod lounge; SEC-AUD-462, 465; D-976)

Started 11:56 (clock). Stack-free: no lock, no stack, no spec, no shots.

## Who imports what (lazy chunk)
`routes/lounge.tsx:6` loads `routes/LoungeRoot` with `lazyNamed(() => import(...))`, so the Lounge is a lazy chunk. `LoungeRoot` -> `LoungeShell` -> `ChannelView` -> `ForumView`; `LoungeShell` -> `LoungeDialogs` -> `CustomStatusDialog` -> `EmojiPicker`; `Composer`, `MessageItem` (via `MessageList`/`ThreadPane`/`VoiceChat`), `msg/Reactions`, `ExpressionPicker` all hang under `ChannelView`/`LoungeShell`. `Gallery.tsx` -> `ShelfPanel`. The only importer outside the Lounge tree is `settings/server/sections/Expressions.tsx` (imports `ExpressionPicker`'s `resetDefaultPacks`, a Settings lazy chunk). Nothing is loaded by first paint or a public page: all three files may change.

## REACH table
| # | file:line | select | opened by | sheet rule now | theme | raw/kit | fit | found by in tests/e2e |
|---|---|---|---|---|---|---|---|---|
| 1 | `lounge/forum/ForumView.tsx:221` | Sort (inside `label.lng-forum-select`, no own aria-label) | a forum channel's toolbar (own forum channel, as `p102-media-shots.spec.ts`) | `forum.css:30-45` `.lng-forum-select` (label, inline-flex; the select styled by element name there, not mine) | Lounge | raw | `fit="content"` | see below |
| 2 | `lounge/components/EmojiPicker.tsx:305` | skin tone (`aria-label` skinTone, class `lng-emoji-tone`) | any emoji picker: composer smiley, reaction add, custom status | none for `.lng-emoji-tone`; row `.lng-emoji-top` (`lounge.css`, flex, gap 6) beside `.lng-emoji-search` | Lounge | raw | `fit="content"` | class `lng-emoji-tone`: only EmojiPicker.tsx itself (grep: no sheet, no test, no spec); picker tests find it by role/label |
| 3 | `lounge/scrapbook/ShelfPanel.tsx:146` | shelf picker (`aria-label` shelfPick, class `lng-scr-input`) | Scrapbook/Gallery panel, shelf scenario of `p102-lounge-shots.spec.ts` | `scrapbook.css:35-43` `.lng-scr-input`, row `.lng-scr-actions` `:98-103` (not mine) | Lounge | raw | `fit="content"` | class `lng-scr-input` is dropped from this select only (other inputs keep it) |

Kit prop `fit` not in my base at 11:56; waiting for `fix/p103-kit (READY)`.
