# Wave 79 cinema handoff (SEC-AUD-313, SEC-AUD-314; D-815, D-816, D-817)

Branch `fix/p79-cinema` (base `aa3ff9cf`). Web only: no Rust, no migration, no generated file, no i18n fragment, no web build.

## Done (commits)
- `6b2b0542` SEC-AUD-313, SEC-AUD-314: the CSS, `Shell.tsx` (first version). `b1ce21b6` the shots spec. A third commit holds the section-bar effect's fonts re-run, `scroll-padding-inline`, the spec fixes and this handoff.
- **314, rails.** `cinema.css`: `.cin-rows { grid-template-columns: minmax(0, 1fr) }` plus `min-inline-size: 0` on `.cin-block, .cin-row-wrap`. I put both in at once and did **not** isolate which of the two was needed (no second base variant was run); the grid template is the one the plan read as the cause.
- **314, phone bar.** `cinema.css`: `.cin-frame[data-mobile] .cin-sectionbar` is `overflow-x: auto`, no scrollbar, `scroll-padding-inline: 16px`, children `flex: none`, brand and slash hidden, first nav loses its start margin. (The `max-width: 768px` block already made the bar scroll; what was missing was the brand and slash taking 100 px and nothing bringing the current area into view.) `cinema/Shell.tsx`, `SectionBar` only: a ref and one effect, on mount, on pathname change and once `document.fonts.ready` resolves, calls `scrollIntoView?.({ block: 'nearest', inline: 'nearest' })` on `.cin-sectionbar-nav + .cin-sectionbar-nav [aria-current='page']`. I did not set `scrollLeft` myself; `window.scrollY` stayed 0 on every phone page. The fonts re-run was needed: the first version left Media 2 px past the edge in the light variant (font swap widened the bar after the first scroll).
- **313, fill.** In `.cin-root:not([data-theme^='sideroom'])`: `--sr-gold-ink: var(--ui-accent-ink)`, `--cin-fill: var(--ui-accent)`, `--cin-fill-ink: var(--ui-accent-ink)`; comment corrected. Fills read `var(--cin-fill, var(--sr-gold))` with `var(--cin-fill-ink, var(--sr-gold-ink))`: `.cin-chip-all[aria-pressed='true']` (cinema.css), `.tv-btn-primary` and `.tv-chip[aria-pressed='true']` (tv.css; `.tv-btn-primary:hover` already read `--ui-accent-hover`). Not changed, on purpose: `cinema.css` `.cin-hero-dot[aria-current]::after` (inside the dark hero) and `tv.css` `.tv-rule` (a 1 px decorative rule, gold as a line, not a fill); `live.css` fills sit inside `.cpl-player` (dark block) and need nothing.
- **313, dark surfaces.** `.cin-hero, .cin-title-head, .cin-qa` (cinema.css), `.cpl-player` (player.css), `.tv-crt` (tv.css; it also lacked `--sr-gold-ink`, added), `.wp-root` (watchparty.css) now also state `--ui-accent: #c2b08f; --ui-accent-hover: #d1c1a2; --ui-accent-ink: #1b1a15; --cin-fill: #c2b08f; --cin-fill-ink: #1b1a15`.
- Lazy chunks confirmed by `git grep`: `cinema/Shell.tsx` is imported by `CinemaRoot.tsx` (lazy via `cinema/routes.tsx`) and a test; `cinema.css` by `CinemaRoot`; `player.css` by Player/PlayerFrame/MyList/LiveView and `MiniPlayer` (lazy from `MiniPlayerMount`) and `player/PlayerHost`; `watchparty.css` by `PartyMessageCard` (Lounge chunk) and `StartPartyDialog`; `tv.css` by `Guide.tsx`. No web build, no bundle checks.

## Not done
- Tail (Live TV watch page "Try again" in a light Cinema; the mini player outside the Cinema): not shot.
- Not changed (outside the contract, see questions): the danger button in the dark theatre.

## Confirm-the-fault: the base (ran it: five files restored from `aa3ff9cf` via `git show`, same spec, `SR_P79_BASE=1`; report in `report-base.txt`)
9 titles in the Trending rail (3 real titles shown 9 times through the Discover fake):
| page | base `scrollWidth` / client | after |
|---|---|---|
| `/cinema` 1280 shipped | 1609 / 1280 (past the edge: `.cin-block`, `.cin-row-wrap`, `.cin-scope`, `.cin-row`, rail head) | 1280 / 1280 |
| `/cinema` 1280 light | 1623 / 1280 | 1280 / 1280 |
| `/cinema` 375 shipped and light | 1407 / 375 | 375 / 375 |
| Browse, a title page, `/cinema/tv`, `/cinema/submit`, theatre, 1280 and 375 | 1280 / 1280 and 375 / 375 (already whole) | unchanged |
- Phone "Media" link box on the base: left 423 (shipped), 445 (light): past 375. After: 321..356 (shipped), 321..359 (light), page not moved. Section bar at load after: `scrollLeft` 0 (shipped, it fits with the brand hidden), 18 (light).
- Rail, after: kit scroller `scrollWidth 2086 > clientWidth 1152` at 1280 (1388 > 338 at 375); next button 0 -> 934, previous back; on the phone the kit's arrows are hidden by `cinema.css` (`max-width: 30rem`, by design), so the spec scrolls by `scrollBy` (0 -> 312). First card at 1280: 217.6 px wide with the fix (5 across at exactly 1280, `max-width: 1280px` rule); the base's 296/299 px was the inflated rail (9 titles made the row 1545 wide), and with 3 titles the row is the page width, so the fixed value is the 3-title value (I did not measure a 3-title base run separately).
- Colours, base, light Cinema: theatre badge `rgb(88, 101, 242)` on white ink, "All" chip on `/cinema/tv` `rgb(71, 82, 196)` (the link colour, as the plan said). After: badge `rgb(194, 176, 143)` with `rgb(27, 26, 21)` ink; the chip `rgb(88, 101, 242)` with `rgb(255, 255, 255)` (equals a probe of `var(--ui-accent)` / `var(--ui-accent-ink)`, asserted); Lounge Dark: chip `rgb(88, 101, 242)`, badge gold. Contrast: gold `#c2b08f` on `#1b1a15` far above 4.5; accent `#5865f2` with white 4.61 (the kit's own primary button, same pair) in light and Lounge Dark.
- Under `sideroom` the computed colours (root, section bar, brand, header, logo, nav link, footer, card title, hero, hero title, hero button, quick action, the pressed chip, the primary button, the CRT, the theatre badge) equal the base's JSON at 1280 and 375: asserted by the spec (`shipped-desktop equals the base`, `shipped-phone equals the base`). `sideroom-broadcast` (the Live TV guide's own theme) was not run separately.

## Shots and tests
- Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-cinema`: 33 shots by the after run (home, home with the first rail moved, Browse, a title page, `/cinema/tv`, the theatre: desktop and phone, shipped and light = 24; phone section bar at load and dragged to its start, shipped and light = 4; the phone menu, shipped and light = 2; Lounge Dark home, `/cinema/tv`, theatre = 3), plus `base-pngs/` (home, party, section bar of the base run), `report-base.txt`, `report-after.txt`, `computed-base-*.json`, `computed-after-*.json`. I opened all 33 and looked at each: nothing past the right edge, the filled chip and the Play button read as the kit's primary button in each theme, the badge and the Play button are gold in the dark surfaces, the bar shows Media in view. `data-theme` read back before each page shot, `sectionThemes.cinema` set explicitly to `sideroom` and `lounge-light` (and `lounge-dark` for the three).
- Click-through (in the spec, light): rail next and previous, a genre chip and back to All, the phone bar's Shelves link (goes to `/study`) and back, the phone menu.
- Spec `e2e/tests/p79-cinema-shots.spec.ts` (`SR_P79_CINEMA=1`, `SR_P79_BASE=1` for the base). Run twice before the final one with fixes to the spec and to the effect (listed above); final run: 1 passed.
- Vitest (single files): `cinema/__tests__/roving.test.tsx`, `cinema.test.tsx` 33 passed. Web `tsc`, eslint on `Shell.tsx`, prettier on the changed files, `node scripts/check-css-tokens.mjs` (clean; `--cin-fill` is accepted), `node scripts/i18n-audit.mjs --max 0` (0), e2e `tsc`.

## Docs
`docs/10-design-system-and-theming.md` states only the Side Room token values; nothing it says changed. Proposed sentence (commander): "Under a theme that is not a Side Room one, filled gold in the Library and the Cinema is the theme's accent with its accent ink (`--lib-fill`, `--cin-fill`), and gold as text or a line is the link colour (D-815); a surface kept dark (hero, title backdrop, quick actions, player, CRT, watch-party theatre) restates the Side Room accent too, so a kit badge or button on it is gold (D-816)."

## Proposed inventory rows
- SEC-AUD-313: the Cinema's filled gold is the theme's accent with its ink in every theme, and the surfaces kept dark restate the accent (D-815, D-816). Closed.
- SEC-AUD-314: no Cinema page scrolls sideways at 375 or 1280 with a full rail; the phone section bar scrolls inside itself with the current area in view (D-817). Closed. The Arcade's bar and other rooms with the same bar remain unread.
Changelog sentences: 313 "Buttons and chips in a themed Cinema match the app's buttons, and the watch-party theatre's badges are gold in every theme." 314 "The Cinema's rows scroll inside themselves however many titles they hold, and on a phone the top bar shows the area you are in."
Threat-model rows: none. AGENTS.md: none beyond the commander's D-817 sentence.

## Owner questions
1. In the kept-dark theatre under a light Cinema, "End party" is the Lounge's bright red and the disabled "Send" button is washed out (see `party-light-desktop.png` against `party-shipped-desktop.png`), because the dark blocks restate the accent but not `--ui-danger*`, `--ui-success`, `--ui-warning`. D-816 says "Side Room whole": extend the dark blocks to those tokens? (I did not: outside the contract.)
2. `scrollIntoView` and `document.fonts.ready` are used by the section bar's effect; the Library builder builds the same bar separately, as planned. Say if the two should be unified later (a shared hook would have to live outside both rooms).

## Cleanup
Stack stopped (PIDs, then the two listeners on 5575/8575 by PID), lock released; `sr_p79_cinema_ui` and `p79-cinema-bin`: both done (database dropped, folder deleted). Stash: none touched.
