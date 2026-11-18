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
