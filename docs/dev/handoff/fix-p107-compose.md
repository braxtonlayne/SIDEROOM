# fix-p107-compose (wave 107, pod lounge A; D-1022, D-1023, D-1024)

## Clock times
Started 18:37, sheets and checks done 18:40. No job submitted (code builder).

## Done (commits)
- `D-1022 apps/web/src/lounge/components/emojiPicker.css (READY)`: on a phone (max-width 720px, the sheet's own block) `.lng-emoji .lng-emoji-btn` 44 px tall, `.lng-emoji-cat` block-size 44 px (inline-size stays 32).
- `D-1022 apps/web/src/lounge/lounge.css (ALL READY)`: composer tap areas; composer focus; placeholder colours; emoji search field contract and 44 px; inline-edit ring.

## Not done
- `.lng-inline-textarea` lives in `msg/msg.css:519` (the msgs builder's file), not in lounge.css. I did not touch it: the ring is a two-class rule `.lng-textarea.lng-inline-textarea:focus-visible` in lounge.css after `.lng-textarea:focus-visible`. The textarea draws no border (`border: 0` from `.lng-textarea`), so no border rule.
- Search emoji field: it had no border (the census: 32 fail, F/out 1.26). It now draws the contract's border; `box-sizing: border-box` added so the 32 px height is kept (it was content-box with no vertical padding).

## REACH table
| selector | rule now (this tree) | page / how opened | cure | will move |
|---|---|---|---|---|
| `.lng-composer .lng-icon-btn` (Add, Record, GIF, Sticker, Emoji, and Send) | `.lng-icon-btn` lounge.css:193 (28 x 28) | channel, thread, forum post, DM: the composer | A.1 `.lng-root[data-mobile] .lng-composer .lng-icon-btn::after` inset-block -8px, inset-inline 0 (after `.lng-root[data-mobile] .lng-composer-wrap`) | nothing drawn; tap area 44 tall at 375 only |
| `.lng-emoji-btn` | lounge.css `.lng-emoji-btn` height 34; `.lng-emoji .lng-emoji-btn` in emojiPicker.css | composer, Emoji button; also the Expression picker's emoji tab | A.2, `@media (max-width: 720px)` height 44 | every cell row 10 px taller on a phone; picker scrolls more |
| `.lng-emoji-cat` | emojiPicker.css `.lng-emoji-cat` 32 x 32 | same picker | A.2 block-size 44 on a phone; column keeps scrolling (max-block-size 240 on `.lng-emoji-cats`) | 6 buttons x 12 px |
| `.lng-emoji-search` | lounge.css `.lng-emoji-search` height 32, border 0 | EmojiPicker, ExpressionPicker, GifPicker; MessageDialogs (two inputs: forward and another search, lines 170, 200) | B + A.2 | at every width: now has a 1px border (box kept 32 px), ring, hover; phone 44 px tall. The two MessageDialogs inputs move with it (same class) |
| `.lng-textarea` composer | lounge.css `.lng-textarea` | composer | D-1023 `.lng-composer:has(.lng-textarea:focus-visible)` 1px ring, offset -1px; placeholder `--ui-text-faint`, opacity 1 (also the backdrop variant) | focus look and placeholder colour only |
| `.lng-inline-textarea` | msg.css:519 (read only), `.lng-textarea` in lounge.css | "Edit message" on a message | B: 2px ring, offset 1px | focus look; placeholder colour (shares `.lng-textarea::placeholder`) |
Phone rule choice: the composer buttons use `.lng-root[data-mobile]` (the composer is inside the root); the picker is a popover that may be portaled outside `.lng-root`, so its rules use the sheet's `@media (max-width: 720px)` block. Dependants of the changed rules: placeholders reach every `.lng-textarea` (composer, inline edit; `git grep` finds `lng-textarea` only in Composer.tsx and InlineEdit.tsx).

## FINDERS
`git grep` for `lng-composer|lng-textarea|lng-emoji|emojiPicker.css|lounge.css` in e2e, apps/web/src, packages, scripts: only retired `p<NN>-*-shots` specs, `components/emojiPicker.test.tsx` (ran: 9 passed, not edited), `entranceDangerText.test.ts` (bans `color-mix(...--ui-danger`; it reads other files, not mine). No CI spec drives these controls. Verdict: none to fix.

## JOBS
none.

## Figures against the task lists
Not measured (no shots for a code builder); loungeshots measures. Must-not-change: composer height (no height rule touched; `min-height` unchanged), picker width (`inline-size` untouched), textarea box (untouched).

## Imports / chunk
lounge.css and emojiPicker.css are imported by the lounge chunk (emojiPicker.css by `EmojiPicker.tsx:7`); lazy, not first paint.

## Checks (own tree, last commit)
prettier --check both files: exit 0 (18:38). `node scripts/check-css-tokens.mjs`: clean, exit 0. `npx vitest run src/lounge/components/emojiPicker.test.tsx`: 9 passed (18:39). tsc, build, shoot: not run.

## Closing text / changelog / owner questions
Changelog: the composer's and emoji picker's controls are 44 px targets on a phone, and the composer, inline edit and emoji search show focus.
Owner questions: none (44 px cells taken from the owner's delegate answer).

## PROCESSES
strays: none found
