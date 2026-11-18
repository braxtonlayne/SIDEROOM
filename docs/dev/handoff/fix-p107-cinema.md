# fix-p107-cinema (wave 107, pod sections, code builder; D-1022)

## Clock
Started 18:37, all four sheets committed by 18:40. No job submitted (code builder).

## Done (commits)
`D-1022 cinema.css (READY)`, `D-1022 watchparty.css (READY)`, `D-1022 player.css (READY)`, `D-1022 tv.css (ALL READY)`. Every cure is inside a phone rule, so 1024 and 1280 are untouched.
Note: the player sheet is `apps/web/src/cinema/player/player.css` (the task wrote `player/player.css`).

## Not done
Nothing built, no job run: `sectionshots` measures. Not cured, not named: `.cin-ep .cin-qa` (episode rows, 30 px, own rule), `.cin-link-btn` in Search.tsx and the spoiler button (a text button inline in a `p.cin-spoiler`; it shares the 44 px reach rule only if sectionshots finds it failing).

## REACH table
| control | rule now | page / how opened | cure | what moves |
|---|---|---|---|---|
| `.tv-chip` | tv.css:98 min-height 36 | Live TV guide, `/cinema/tv` | A.1, `@media (max-width:768px)` block in tv.css, `::after` -4px | nothing; wrapped rows' 8 px gap holds the reach (areas meet at the midline, no gap change) |
| `.tv-nav-btn` | tv.css:132 min 36 | same, day nav | A.1 -4px | nothing |
| `.tv-fav` | tv.css:364 28x28 | phone agenda head (MobileAgenda) | A.1 -8px | nothing (head padding 10 px) |
| `.tv-agenda-tune` | tv.css:696 min-height 36 | phone agenda head | A.1 -4px | nothing |
| `.wp-btn` | watchparty.css:196 36 high | watch party control bar | A.3 + A.1: `.wp-controls` row-gap 8 px, `::after` -4px, `max-width:600px` (the sheet's own phone rule) | the bar gets 4 px more per wrapped line (about +8 in all) |
| `.wp-vol` | watchparty.css:241 width 80, 16 high | same | A.1 by padding-block 14 + margin-block -14 (an input has no `::after`) | its row does not grow; the box is 44 tall. Probe: if the slider ever sits alone on a line its reach overlaps the next line |
| `.cin-qa` (poster) | cinema.css:519 30x30 | Cinema browse poster overlay | A.2, `.cin-card .cin-qa` 44x44, `max-width:768px` | the overlay only; 2 x 44 + 6 gap in a 144+ px poster; extras are hidden on a phone |
| `.cin-link-btn` | cinema.css:343 padding 4px 0 | title page Comments | A.1 -7.5px, scoped `.cin-post-actions .cin-link-btn` | row-gap 15 px so a wrapped line does not share reach (+3 px a wrapped line); line above is the 6 px grid gap plus body text, not a control |
| `.cpl-btn` | cinema/player/player.css:182 40 high | player and Live TV top bar | A.1 -2px, scoped `.cpl-top > .cpl-btn`, `max-width:768px` | nothing; the control bar is not touched |

## FINDERS
Classes touched, searched in `e2e/` and `apps/web/src`: only retired `p78/p79/p80/p81` shots specs (computed-style reads of `.cin-qa`, `.tv-chip[aria-pressed]`, not driven; not CI). Files read as text: `apps/web/src/tools/__tests__/overlayZoom.test.tsx` reads tv.css for `.tv-pop` z-index only; run once, 12 passed. No CI spec drives any of these. No `outline`/field rules changed (cure B does not apply to this pod's rows).

## JOBS
none.

## Figures
Not measured (no job). Expected: no layout change except wp bar (+8 px), `.cin-qa` 44 on a phone, post-actions row gap 15 px only when wrapped.

## Importers (chunks)
cinema.css: CinemaRoot, MyListPage (lazy cinema chunk); tv.css: Guide, LiveView; watchparty.css: WatchPartyPage, Theatre, strip, dialog (lazy); player.css: MiniPlayer, Player, PlayerFrame. `git grep` of the imports above; none is first paint or a public page.

## Checks (on my last code commit)
prettier --check on the four files: ok (18:39). `node scripts/check-css-tokens.mjs`: clean (18:39). vitest `overlayZoom.test.tsx`: 12 passed, rc 0 (18:40). i18n, tsc: not run (no string, no ts).

## Closing text, changelog, owner questions
Changelog: On a phone, Live TV chips and day arrows, the star and "tune in", the watch party bar and volume, poster quick actions, comment links and the player's top button now have 44 px tap areas.
Owner questions: none.

## PROCESSES
strays: none found (18:41).
