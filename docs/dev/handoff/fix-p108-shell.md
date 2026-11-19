# Wave 108 shell (pod lounge): D-1022, D-1024; SEC-AUD-527 and the Friends header of 525

## Clock times
Started 20:49; REACH and all three files READY 20:51 (ALL READY). No job of any kind was run (sheets only).

## Done (commits)
- `D-1022 threads.css (READY)`: `@media (max-width: 768px)` after `.lng-threads-search::placeholder`: `.lng-threads-tabs button, .lng-threads-search { min-block-size: 44px }`.
- `D-1022 forum.css (READY)`: `@media (max-width: 768px) { .lng-post-textarea { min-block-size: 44px } }` after the textarea's placeholder rule.
- `D-1022 shell.css (ALL READY)`: in the Friends 720px block, `.lng-friends .lng-header` `padding-block: 0` became `padding-block: 4px 0`.

## Not done
- NewPostDialog has no placeholder attribute: no placeholder: accepted (string and `.tsx`, not mine).

## REACH table
| control | rule that sizes it now | page, how opened | cure | what moves |
|---|---|---|---|---|
| Threads popover tabs (`.lng-threads-tabs button`) | threads.css `.lng-threads-tabs button` (padding 6px, 32 px) | channel header "Threads" button, popover | A.2 min-block-size 44 (phone) | tab row 32 -> 44; popover grows 12 px, scrolls at its 420 cap |
| Threads popover search (`.lng-threads-search`) | threads.css `.lng-threads-search` (padding 6px 8px, 29 px) | same | A.2 min-block-size 44; cure B already complete (rest colour-mix 65, hover, focus ring 2px offset 1px, placeholder faint opacity 1): nothing to add | 29 -> 44 |
| New Post textarea (`.lng-post-textarea`) | forum.css `.lng-post-textarea` (padding 8px, rows height is taller than 44 by default) | forum channel, New Post dialog | A.2 min-block-size 44; keeps its own height if taller (cure B already complete in the rule) | nothing expected; loungeshots to say the figure |
| Friends header Back / New Group DM | shell.css 720px block `.lng-friends .lng-header` (padding-block 0), row 1 44 px from wave 107; lounge.css `.lng-root[data-mobile] .lng-header .lng-icon-btn::after` inset-block -8px | Friends page on a phone, from the tab bar | see below | header +4 px; tabs row and list move down 4 px |

Friends header: by the sheet the first line was already 44 px (title min-block-size 44), with the 28 px buttons centred; the -8px area then spans exactly 0..44, so its top point sat on the header's top edge and the section bar took it. I could not measure (no jobs), so this is the 4 px top padding the task names: area now 4..48, inside the header. If the header was not 44 px on the tab-bar path, loungeshots will show it.
Breakpoint: the popover and the dialog are portaled (kit Overlay), possibly outside `.lng-root[data-mobile]`, so I used `max-width: 768px`, the kit's MOBILE_QUERY which is what sets `data-mobile`.

## FINDERS
- Classes `.lng-threads-tabs`, `.lng-threads-search`, `.lng-post-textarea`, `.lng-friends-tabs`: e2e only `p107-list-shots.spec.ts` (lines 158-166, a retired shots spec, selector lists) and `p104-lounge-shots.spec.ts` (no class hit, only a name). No CI spec or test drives them.
- File names `threads.css`, `forum.css`, `shell.css`: `apps/web/src/styles/__tests__/textLink.test.ts` mentions shell.css in a comment only; no test reads my files as text. Importers: ThreadPane.tsx (threads.css), ForumView.tsx (forum.css), LoungeDialogs.tsx (shell.css), all in the lazy Lounge chunk.

## JOBS
none.

## Figures against the task's lists
Not measured (no jobs). May change: Threads tabs/search 44 on a phone, thread search 44, Friends header +4 px. Pointer widths: every new rule is under a max-width 768 media query or inside the existing 720 block.

## Checks
- `npx prettier --check` on the three files: passed (20:50). `node scripts/check-css-tokens.mjs`: clean (20:50). Vitest, eslint, tsc, i18n audit, doc links: left to the commander.

## Shots
None (code builder).

## Closing text and rows
Rows 527 and the Friends lines of 525: cured in sheets, pending loungeshots figures. One changelog sentence: On a phone the Threads popover's tabs and search, and the New Post box, are 44 px tall, and the Friends header's Back and New Group DM buttons clear the section bar.
Owner questions: none.

## PROCESSES
strays: none found
