# fix-p106-header (D-1014, SEC-AUD-495)

## Clock
Started 16:30; base figures 16:35; checkpoint commits 16:53 and 17:26; after round passed 17:37; handoff 17:45. The runner was stopped for mount's cargo build, so my first jobs sat queued from about 16:36 to 16:56.

## Done (commits)
- `3ac64b30` checkpoint (CSS, Escape handler, spec); a second commit fixes the KeyboardEvent import clash (tsc job 0096 failed on it) and makes the spec's channel names constant; the last commit adds the "base tools spill" rule to the spec and this handoff.
- `lounge.css`, in the header and search rules (after `.lng-search input:focus-visible`), plus the XC-VIS-002 comment at the tools rule, which now says the search field is the one exception:
  - `@media (min-width: 769px)`: `.lng-header { container: lng-header / inline-size; position: relative; z-index: 3 }`. The z-index is needed: container-type makes the header a stacking context, and without it the search panel (z 20 inside the header) would paint under the messages. The phone (<= 768 px) is not reached.
  - `@container lng-header (max-width: 639.98px)`: the wrap `.lng-search-wrap` is 28 x 24 (24 high so the panel under it keeps its y), the form is a 28 x 28 icon button anchored at the inline end, and the input fills it at opacity 0 (click, Tab and fill all reach it; it keeps a box). `:focus-within` on the wrap (so the panel's own focus counts too) restores the 144 + 12 px field with its background, anchored at the same end, so the open field and the panel keep the base boxes.
  - `.lng-header-tools` gets `--lng-search-folded: 1` in the folded state (read by the Escape handler).
- `ChannelHeader.tsx`: `onKeyDown` on `.lng-header-tools` (`foldSearchOnEscape`): on Escape in the search input, only while folded, focus moves to the tool before the field (SearchBox's own Escape has already closed the panel), so the field folds again and focus is not lost to the page. No new string, class or attribute.
- Threshold 640 px: the unfolded headers at 724 (1280, list open) and 712 (1024, list closed) must stay as they are; a 180 px title with the full 380 px tools needs about 630. 468 (1024, list open) and 588 (900, list closed) fold.

## Not done
- No shortcut that focuses the header search exists in the app (git grep): the spec focuses by `focus()`, a click and typing. Nothing added (not in my files).
- The Friends page, Message Requests pages and the scrapbook gallery also use `.lng-header` (see Imports). Only the Message Request view was measured (the "dm 1024" row, equal to base). They have no search field, so nothing folds there, but they do get the container, `position: relative` and z-index 3 at >= 769 px. The Friends page and the gallery were not measured.

## FINDERS (rule 13)
`git grep -n "searchbox\|lng-search\|Search #" -- e2e/tests`. The one CI spec that types into the header search: `e2e/tests/lounge-gaps.spec.ts:120-127` (`getByRole('searchbox', {name: /^Search/})`, `fill`, `press('Enter')`, then `.lng-search-context`). It runs at Playwright's default 1280 x 720 (header about 724 with the list open, above 640: not folded), and `fill` also works on the folded field (opacity 0 counts as visible; a click on it focuses it). No edit needed. Retired shots specs (not edited): p70:202, p71:167 and :194 (`.lng-header .lng-search` not visible at phone, unchanged), p72:301, p83:498, p85:201. Unit tests rendering the header: `p11ShellDm.test.tsx` 12 passed, `p71Panels.test.tsx` 19 passed (jsdom has no CSS: no change). `search.test.tsx`, `serverFlows.test.tsx`, `p70Lines.test.tsx` render SearchBox alone (not changed). Focus order: unchanged (the spec compares the header's focusable list, base against after).

## JOBS (submitted to started, ran, rc)
| id | class | waited | ran | rc |
|---|---|---|---|---|
| 0082 | shoot base | 18 s | 120 s | 0 |
| 0096 | tsc | 1726 s (runner stopped) | 27 s | 2 (my KeyboardEvent import shadowed the DOM type; fixed) |
| 0099 | build | 789 s | 18 s | 0 |
| 0108 | shoot after | 581 s | 49 s | 1 (spec fault: channel names with a random stamp had different widths in base and after; fixed with constant names) |
| 0122 | shoot base (constant names) | 479 s | 44 s | 0 |
| 0128 | tsc | 620 s | 24 s | 0 |
| 0129 | build | 613 s | 18 s | 0 |
| 0137 | shoot after | 345 s | 46 s | 1 (spec fault: at 900 with the list open the base tools spill past the header, so "x shifted by the fold" is not comparable; the spec now checks they sit inside the header) |
| 0146 | shoot after | 242 s | 46 s | 0, problems [] |
Jobs 0123 and 0145 were cancelled (0123 would have run on a stale build; 0145 had a wrong OUT folder). 0137 and 0146 gave the same figures, so no flakiness seen; the one run that passed is 0146.

## Figures (base job 0122, after job 0146; names `p106h-text` 85 px, `p106h-forum` 102 px)
Base, text channel / forum channel:
- 1280 list open: header 724, title whole, topic 174 / 157. 1280 closed: header 968, whole. 1024 closed: header 712, whole, topic 162 / 145. **1024 list open: header 468, title 4 px (cut), topic 0**, tools 380, search form 156 x 24 (144 + 2 x 6). 900 list open: header 344, title 0 / 8 and the tools (380) spill past the header's end (a base fault); 900 list closed: header 588, whole.
- The layout still has a member list at 900 (the first base run's 900 heading wait timed out because the title was 0 px wide, not visible; the wait is now `attached`).

After:
- 1024 list open: header 468, search 28 x 28 at 676,63, tools 252 wide, **title whole (85 / 102)**, topic 46 / 29 (the topic still gives way first, D-1007). With the longer first-run names (180 / 197 px, job 0108, before the names became constant) the title was cut to 131 px: the fold frees 128 px (156 - 28), which is not enough for a 180 px title at 1024 with the list open. The task accepts that: the title then ends in an ellipsis.
- 900 list open: tools 252, now inside the 344 header (base spilled); title 8 px, cut (no room at all). 900 list closed: folded, title whole.

Must not change, asserted equal to base by the spec (problems: []): unfolded headers (1280 open and closed, 1024 closed): header, title, topic, tools, every tool and the search form; header height 49 everywhere; every other tool's size and y; the tools' right edge; at fold the tools before the search shift right by exactly 128 and the two after it keep x; the 375 header; the Message Request header at 1024; the focus order; no sideways scroll (also with the field focused). Focused at 1024 with the list open: the field is 156 x 24 at 548,65 (the base box), the search panel is 258,97 446 x 47 (the base box); a click focuses it; typing opens the panel; Escape folds it back to 28 x 28 and focus moves to the button before it (in the base, focus stays in the field).
Hit test over the header, box reach at 1024 list open: after 9 pass, 0 fail, 0 not reached; base 8 pass, 0 fail, 1 not reached. 375 touch: 3 pass, 0 fail in both.

## Shots (all opened after job 0146; base twins have the `base-` prefix; I opened the base 1024 list-open twin)
- `header-1024-members-open.png`: title "p106h-text" whole, topic cut to "p106...", the tool icons, the search as a magnifier icon, Inbox and Help after it. Base twin: title 4 px, a full Search field.
- `header-1024-members-open-search-focused.png`: the field opened to full width at the same end, covering the Threads-to-Members icons to its left (the title and a topic stub still show); the filter chips panel below it, above the messages (the z-index works), where the base has it.
- `header-1024-members-closed.png`: full Search field, topic "p106h topic: a long lin...": as the base.
- `header-1280-members-open.png`: full Search field, topic cut: as the base.
- `forum-header-1024-members-open.png`: "p106h-forum" whole, topic "p1...", icon search.
- `header-375.png`: the phone header: back, title, search icon, more: unchanged.
Lounge Light only, as the task says.

## Imports (D-385)
`ChannelHeader.tsx` is imported by `ChannelView.tsx`, `voice/StageView.tsx`, `voice/VoiceChannelView.tsx` (the lazy Lounge chunk). `lounge.css` is the Lounge chunk's sheet. `.lng-header` is also the class of `scrapbook/Gallery.tsx:337`, `shell/FriendsPage.tsx:348`, `shell/MessageRequestView.tsx:85`, `shell/MessageRequestsPage.tsx:126` (all the Lounge chunk): they get container-type, position: relative and z-index 3 at >= 769 px, and none has a `.lng-search-wrap`. The phone is not reached. `.lng-search-wrap > .lng-search` and its input are only SearchBox's; the phone sheet's `.lng-search-sheet .lng-search*` rules are at <= 768 px, where the media query does not apply.

## Checks (on the last commit)
Web tsc: job 0128 rc 0 (the `.tsx` has not changed since). eslint `ChannelHeader.tsx`: exit 0. prettier on the three files: clean. e2e tsc: exit 0. `node scripts/check-css-tokens.mjs`: exit 0. vitest `p11ShellDm.test.tsx` 12 passed, `p71Panels.test.tsx` 19 passed (16:38, before the later `.tsx` change that only renamed the import). i18n audit: not run (no string).

## Closing text proposal for SEC-AUD-495
Fixed (D-1014, after D-1007): when a channel's header is under 640 px wide (a 1024 window with the member list open) the search field folds to its icon; focusing it, or anything in its panel, opens it to full width at the same end, and Escape or leaving folds it. The title, 4 px wide before, shows whole at the seeded names; a title longer than about 150 px at that width still ends in an ellipsis (accepted). The 900 px list-open header no longer spills its tools past its end.
Changelog sentence: The Lounge channel header folds its search field to an icon when it is short of room, so the channel name can be read.
Proposed new row (a base fault, no taker named): at 900 px with the member list open the header is 344 px; even the folded tools (252) leave the title 8 px, unreadable.
No threat-model row.

## Owner questions
None. Default taken (Discord-like): the folded search is an icon; focused, it opens over the tools to its left; Escape leaves the field.

## PROCESSES
strays: none found
