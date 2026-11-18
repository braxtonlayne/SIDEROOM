# fix-p104-forum (wave 104, code builder, pod lounge; D-996, SEC-AUD-465 remainder)

Started 14:13. Stack-free: no lock, no stack, no spec, no shots, no binaries, no database.

## REACH table (committed 14:2x)
| # | file:line | element | label text | how to reach | sheet rule now | theme | found by in tests/e2e |
|---|---|---|---|---|---|---|---|
| 1 | `apps/web/src/lounge/forum/ForumView.tsx:219-225` | Sort `<select>` inside `label.lng-forum-select` (no own aria-label; the label's `span.lng-muted` is its accessible name) | `lounge.forum.sort` (Sort) | a forum channel (type 15) in the Lounge: as `e2e/tests/p103-lounge-shots.spec.ts:1019-1028`, `POST /guilds/<id>/channels {name, type: 15}`, then `/channels/<guildId>/<channelId>` | `forum.css:36-40` `.lng-forum-select` (inline-flex, centre, gap 6px); no size set | Lounge | `p103-lounge-shots.spec.ts:259-300` (spec of wave 103, by label/file), `p102-media-shots.spec.ts:857` lists the file; no test or spec finds it by the class or by `select` inside it |
| 2 | `forum.css:36-40` `.lng-forum-select` | the label itself: reaches only `ForumView.tsx:219` (`git grep -n "lng-forum-select" -- apps/web/src e2e` gives exactly those two lines and nothing in e2e) | - | same | - | Lounge | none |
| 3 | `span.lng-muted` in that label (`ForumView.tsx:220`) | label text | Sort | same | `lounge.css:20` `.lng-muted` sets colour only: the span inherits the size. Before: 16px (`.lng-root`, `lounge.css:11`). After: 14px (inherits the label's new size). | Lounge | none |
| 4 | the kit's wrapper `span` of `NativeSelect` | new | - | same | no rule of `forum.css` or `lounge.css` matches `.lng-forum-select > span` or `.lng-forum-select span` (`git grep` empty): the wrapper is not reached | Lounge | none |

Other `lng-forum-*` users, not reached by this change: `NewPostDialog.tsx:141` uses `lng-forum-chips`.
Tests that render `ForumView`: `p13forumFiles.test.tsx`, `p10LoungeFe.test.tsx`, `p69Threads.test.tsx`.

## Done (commits on `fix/p104-forum`)
- `D-996 forum REACH table`; `D-996 ForumView.tsx (READY)`; `D-996 forum.css (ALL READY)`; this handoff. Finished about 14:17.
- Diff in words: `ForumView.tsx` imports `NativeSelect` and the Sort is `<NativeSelect fit="content">` with the same `value`, `onChange` and the two options; the label and its span are untouched. `forum.css`: `.lng-forum-select` gains `font-size: 0.875rem;` and nothing else. No string, kit or other sheet changed.
- Span size: `span.lng-muted` has no own size (`lounge.css:20` sets colour only), so it inherited 16px from `.lng-root` before and is 14px after (the Lounge control size). Not measured in a browser (code builder); loungeshots measures.
- Tests (single files, unedited): `p13forumFiles.test.tsx` 1 passed, `p10LoungeFe.test.tsx` 12 passed, `p69Threads.test.tsx` 21 passed. None finds the select by tag inside the label. Base not run.

## Who imports
`ChannelView.tsx:24` imports `ForumView`; `ForumView.tsx:21` imports `forum.css`. ChannelView sits under `LoungeShell` under `routes/LoungeRoot`, loaded by `routes/lounge.tsx` as a lazy chunk (the Lounge chunk). Nothing first paint or a public page loads statically.

## TYPECHECK
`bash cli-agents/tc.sh run forum bash -c "cd .../p104-forum/apps/web && npx tsc --noEmit -p ."` 14:15 to 14:16, exit 0.
Others (14:14): `prettier --check` on both files exit 0; `eslint src/lounge/forum/ForumView.tsx` exit 0; `node scripts/check-css-tokens.mjs` exit 0; `node scripts/i18n-audit.mjs --max 0` exit 0; `node scripts/check-doc-links.mjs` exit 0.

## STACK-ROUND
none (code builder).

## PROCESSES
Listing run at the end (see the reply); nothing of mine started a server. Vitest and tsc finished.

## Text for the inventory
- SEC-AUD-465 remainder: the forum Sort is the kit's `NativeSelect fit="content"` with the type set to 14px on `.lng-forum-select` (D-996, D-974); the toolbar row may grow to the kit field's height.
- Changelog sentence: The forum's Sort now uses the kit select, sized to its text, at the Lounge's 14 px control size.
- Owner questions: none.
