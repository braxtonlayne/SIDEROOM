# Wave 83 handoff: developers (SEC-AUD-343, D-849)

T = 12:10 on 2026-11-16 (the shell clock read 12:06 to 12:12 during the work; times below are the shell's). No new item from 14:10, checkpoint 14:30, finished 14:55, hard limit 15:10.
Actual: spec and CSS written 12:06 to 12:08; lock taken 12:08 (free at once); base run, first after run, fix, base re-run and after run 12:08 to 12:11; stack stopped, `sr_p83_developers_ui` dropped, `p83-developers-bin` deleted, lock released about 12:11. One lock round, about 4 minutes. Web only, no cargo, no `.tsx`.

## Done (`apps/web/src/developers/developers.css`, phone block only)
Rules I ended with (all inside `@media (max-width: 720px)`; wave 82's rules and wave 81's `.dev-actions` numbers untouched):
- `.dev-uri-list li > .dev-media-row:has(> .dev-muted)`: `display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 2px 8px; align-items: center`.
- `.dev-media-row:has(> .dev-muted) > :is(img, .dev-asset)`: `grid-row: 1 / span 2`; `> code`: column 2, row 1; `> .dev-muted`: column 2, row 2.
- `.dev-uri-list li:has(> .dev-media-row > .dev-muted)`: `flex-wrap: nowrap; column-gap: 8px`; its `> :is(button, .dev-actions)`: `flex: none`.
- Selector choice: I limited the grid to rows that hold a `.dev-muted` (`:has(> .dev-muted)`) from the start, so the emoji row (no kind, a kit field while renaming) is untouched by construction. Measured: emoji rows at rest and renaming, and the OAuth2 redirect rows, 0 px moved at 375 and at 1280.
- Two things differ from the task's text, both measured. (1) At the task's 12 px gaps the dashed row did not fit: picture 48 + gap 12 + name 176 + gap 12 + button 66 = 314 against 309, so the action dropped to its own line (my first run). Fix: both gaps 8 px (306, 3 px to spare) and the row stays on one line (`nowrap`, the media block shrinks). (2) Because the row no longer wraps, the 32-letter name breaks beside the action instead of above it, which also makes that row shorter than on the base.

## What I saw on the base (I ran it: the HEAD stylesheet swapped in, twice)
At 375 (Dark and Light the same): dashed name `art-desktop-` / `mwjftvwy` on two lines, "Small image" beside the name (x 217), "Delete" on its own line at the row's end (y 70); 32-letter name 16+16; short name one line. Emoji rows (rest and renaming): the action (Rename, Delete / Save, Cancel) on its own line below. Row heights 108 / 109 / 109.

## Figures at 375, before then after (row 309 px wide)
| row | name lines | kind | action | row height |
|---|---|---|---|---|
| `art-desktop-<8>` | 2 then 1 | beside, x 217 then under, x 56 = name's x, 1 line, muted | own line then on the name's line, right end | 108 then 68 |
| 32 letters | 2 then 2 (20 + 12, breaks inside the word) | beside then under (x 56) | own line then beside, right end | 109 then 81 |
| `gone-<8>` | 1 then 1 | beside then under | own line then on the name's line | 109 then 69 |
After widths for the dashed and the short row: picture 48, gap 8, name column 179 (text 176 / 114), gap 8, button 66, row 309; nothing past the card (17 px to spare). Kind colour: Dark rgb(164,170,179), Light rgb(92,94,102) (the muted token), name Dark rgb(219,222,225), Light rgb(49,51,56). Emoji and OAuth2 rows: 0 px moved at 375 and 1280, Dark and Light. At 1280 Rich Presence, Emojis, OAuth2: 0 px moved (the spec's soft assertions, all 0 failures).

## Tests and checks
`node scripts/check-css-tokens.mjs` clean (73); `node scripts/i18n-audit.mjs --max 0` TOTAL 0; prettier clean; e2e `tsc` clean; `developers/__tests__/portal.test.tsx` single file 45 passed. Shots spec `e2e/tests/p83-developers-shots.spec.ts` (local only: `SR_P83_DEVELOPERS=1`, `SR_P83_BASE=1` for the base case, out `SR_P83_DEVELOPERS_OUT`): base run logs only, after run 1 passed with every soft assertion (one-line dashed and short names, kind's left edge within 1 px of the name's, kind under the name, action on the name's line at the row's end, long name breaks, nothing past the card, 1280 and emoji/OAuth2 boxes within 2 px of the base). The first after run failed the "action on the name's line" assertion (314 against 309), which is what led to the 8 px gaps. `developers.css` is imported only by `DevelopersRoot.tsx` and `Authorize.tsx` (lazy Developers chunk): no web build, no bundle check. No top-navigation change (D-850).

## Shots
`visual-sweep-shots/p83-developers` (10 PNG + boxes json): Rich Presence list at 375 and at 1280, Dark and Light (4); Emojis with one row renaming, 375 and 1280, Dark and Light (4); click-through at 375 Light: emoji renamed and saved, asset deleted (2). `p83-developers-base` holds the base run's 8 PNG. Opened: all 10 after shots and the base Rich Presence Light 375 shot (the other 7 base shots not opened; their figures are in the log). Themes set through the account's user_prefs, `data-theme` read back (`lounge-dark`, `lounge-light`).

## Proposed inventory row
SEC-AUD-343 (closed): "On a phone a Developers asset row put the kind beside the name, so the dashed name broke at its dash and the action dropped to its own line (row 309 px: picture 48 + name 176 + kind 75 + gaps did not fit). `developers.css` phone block: the picture, then the name with the kind under it in the muted colour, the action on the name's line at the row's end; rows are 28 to 40 px shorter (D-849)."

## Changelog sentence
On a phone a Rich Presence asset's kind ("Small image") sits under its name, the name stays whole and Delete stays on the name's line.

## Owner questions
1. The gaps in the asset row are 8 px on a phone (12 px does not fit the dashed 20-letter name by 5 px). Default: kept at 8. A longer dashed name (about 22 letters or more) will break at its dash.

## Clean-up
Stack stopped by PIDs (Vite 18208 with its tree, server 30368, metrics with it); `sr_p83_developers_ui` dropped; `p83-developers-bin` deleted (`p80-base-bin` untouched); lock released. Scratch in `SIDEROOM_wt/scratch/p83-dev`. No stash seen or touched; nothing under `apps/web/src/developers/data/`.
