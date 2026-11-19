# fix-p107-heads (wave 107, lounge B: header search, DM header, phone search sheet, switcher, forum and thread searches)

## Clock
Started 18:37, all three sheets committed READY by 18:40. No job submitted (sheets only).

## Done (commits)
- `D-1024 threads.css (READY)`, `D-1024 forum.css (READY)`, `D-1023 lounge.css (ALL READY)`.
- Header search: `.lng-search:has(input:focus-visible)` draws the 2px `--ui-focus` ring, offset 1, on the wrapper (the input keeps `outline: none`); works folded (28 px icon box) and open. Placeholder faint, opacity 1. No border added.
- Cure B (rest `color-mix` 65, hover, `:focus-visible` ring offset 1, placeholder) on `.lng-forum-search`, `.lng-post-textarea`, `.lng-threads-search`, `.lng-switcher-input`; each new block directly after its rule.
- J: phone search sheet: form `padding-inline: 12px 0`, input `block-size: 100%`, `padding-inline: 0 12px`, `box-sizing: border-box`. Form box unchanged (44 px).
- M: `.lng-header[data-dm] .lng-dm-identity` position relative + `::after` inset-block -8px, inset-inline 0 (inside the existing `max-width: 720px` block).

## Not done
- Thread search (`.lng-threads-search`) and post textarea 44 px on a phone: not added (the task only lists them under "may change"; no phone rule exists for them and no census figure asks for it). Said here for listshots.
- Not built, not shot: no job was run (code builder). Nothing measured; the cures are unverified until listshots shoots them.

## REACH table
| control | rule now | page / how | cure | moves |
|---|---|---|---|---|
| `.lng-search` (header) | lounge.css:876 (24 px pill, `input:focus-visible {outline:none}` :893) | channel header, always shown; folded under 640 px container | D-1023 ring on wrapper | ring only |
| `.lng-dm-identity` | lounge.css:873 + profile-p11.css:298 (28 px = 2+24+2) | DM header on a phone | A.1 `::after` | tap area only |
| phone search sheet input | lounge.css `.lng-root[data-mobile] .lng-search-sheet .lng-search` (44 px form, 12 px padding) | phone: header search button opens the sheet | J: input fills form | input box inside form |
| `.lng-switcher-input` | lounge.css:1513 (44 px, border 0); tools.css:16 overrides inside the Drawer | Ctrl/Cmd+K switcher (Drawer.tsx:159, with `sr-tl-input`) | B | border/ring/hover/placeholder |
| `.lng-forum-search` | forum.css:27 (+ phone 44 px rule) | forum channel toolbar | B | colours, ring |
| `.lng-post-textarea` | forum.css:126 | new post dialog | B | colours, ring |
| `.lng-threads-search` | threads.css:120 | thread list panel (Threads button) | B | colours, ring |

## FINDERS (rule 13)
Classes: no e2e spec or unit test references `lng-dm-identity`, `lng-switcher-input`, `lng-forum-search`, `lng-post-textarea`, `lng-threads-search`, `lng-search-sheet`. `e2e/tests/lounge-gaps.spec.ts:127` and `p71Panels.test.tsx:198` use `.lng-search-context` / `.lng-search-hit` (not touched). File names: `lounge.css` is read as text by `sectionBarVoice.test.tsx` (sectionBar rules only; passes unedited, 3/3). `forum.css` and `threads.css`: no test reads them. No CI spec drives a changed control: none to fix.

## JOBS
none.

## Figures
None measured (no job). Expected: nothing moves at 1024/1280 apart from focus/hover/border colour of the five fields; header height, fold and panel boxes untouched.

## Shots
none (listshots).

## Who imports
lounge.css: LoungeShell.tsx (+ settings/server/ui.tsx, settings/user/parts.tsx), lazy lounge chunk; forum.css: ForumView.tsx; threads.css: ThreadPane.tsx. None is first paint or public.

## Checks
- prettier --check on the 3 files: pass. `node scripts/check-css-tokens.mjs`: clean (exit 0). `npx vitest run src/lounge/__tests__/sectionBarVoice.test.tsx` (apps/web): 3 passed.
- Not run: i18n audit (no strings), tsc (no ts changed).

## Closing text / new rows
Proposed row: tools.css `.sr-tl-drawer .lng-switcher-input` (tools builder) draws the Drawer copy's border without the 65 colour-mix and with a 2 px offset; it overrides my rest border there. Flag to tools.

## Changelog
The header search now shows a focus ring, the DM header name and phone search field are 44 px tall targets on a phone, and the thread, forum and switcher fields share the kit field's border, hover and focus look.

## Owner questions
none.

## PROCESSES
strays: none found. No job of mine queued or running.
