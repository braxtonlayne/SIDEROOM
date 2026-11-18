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

Kit prop `fit` arrived 11:58 (merged `fix/p103-kit`). All three moved to `NativeSelect fit="content"`: ShelfPanel (class `lng-scr-input` dropped, the select only), ForumView Sort, EmojiPicker tone (class `lng-emoji-tone` kept; nothing in sheets, tests or e2e names it).

## Done (commits on `fix/p103-forum`)
- `SEC-AUD-465 ShelfPanel.tsx (READY)`, `SEC-AUD-465 ForumView.tsx (READY)`, `SEC-AUD-465 EmojiPicker.tsx (READY)`; merge of `fix/p103-kit`. Each file only swaps `<select>` for `<NativeSelect fit="content">` (and the import). No sheet, string or kit file changed. Finished 12:00, well before 13:15; ALL READY.
- Checks on my tree (11:58 to 12:03): prettier --write (ForumView reformatted, now clean), `eslint` on the three files exit 0, vitest single files all pass: `emojiPicker.test.tsx` 9, `p13forumFiles.test.tsx` 1, `scrapbook/__tests__/scrapbook.test.tsx` 20, `externalExpressions.test.tsx` 9, `p10LoungeFe.test.tsx` 12; `node scripts/i18n-audit.mjs --max 0` exit 0 (TOTAL 0); `node scripts/check-doc-links.mjs` exit 0.
- Not run: no stack, shots or spec (loungeshots measures). The `.lng-scr-input` class is still used by the Gallery's other inputs (scrapbook.css untouched).

## TYPECHECK
`bash cli-agents/tc.sh run forum bash -c "cd ./apps/web && npx tsc --noEmit -p ."`, started 11:59, finished 12:06: exit 0.

## STACK-ROUND
none (code builder).

## Text for the inventory
- SEC-AUD-462/465 (forum part): the forum Sort, the emoji picker's skin tone and the scrapbook shelf picker are the kit's `NativeSelect` with `fit="content"` (D-976); `scrapbook.css` is unchanged and the shelf row stays a flex row.
- Changelog sentence: The forum's Sort, the emoji picker's skin tone and the scrapbook's shelf picker now use the kit select, sized to their text.
- Owner questions: none.
