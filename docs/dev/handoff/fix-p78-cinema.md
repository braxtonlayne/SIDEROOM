# Wave 78 cinema handoff (SEC-AUD-304, D-803)

## Done (commits on `fix/p78-cinema`)
- The Cinema is whole under any theme chosen for it; picture surfaces stay dark.
  - `apps/web/src/cinema/cinema.css`: one block `.cin-root:not([data-theme^='sideroom'])` maps `--sr-bg-0/1/2/olive`, `--sr-line`, `--sr-text/2/3`, `--sr-gold`, `--sr-gold-ink`, `--sr-olive-accent` from `--ui-*`. Dark blocks (Side Room values stated, plus the kit's `--ui-*` bg/border/text/focus and `color-scheme: dark`) on `.cin-hero`, `.cin-title-head`, `.cin-qa`. `.cin-card [data-poster-link]` gets `--sr-text` cream (the kit's poster tag). `.cin-root:not(sideroom) .cin-age-badge-card`: lighter danger text on the scrim. `.cin-root .cin-qa { color }`: the play and rate quick actions are anchors and `.cin-root a { color: inherit }` made their icon dark on the scrim in light (found in the first shots, fixed, re-shot).
  - `cinema/player/player.css`: dark block on `.cpl-player` (also covers the Live TV player, `ltv-*`, which lives inside it).
  - `cinema/tv/tv.css`: `.tv-root:not(sideroom)` sets `--sr-guide-cell/-now/-channel/-marker` from `--ui-*` and the teletext font; `.tv-crt` states the CRT frame, radius, scanlines, teletext font and light text (dark under every theme).
  - `watchparty/watchparty.css`: dark block on `.wp-root` (the whole theatre: stage, header, panel, controls; not `.wp-strip` or the message card, which the Lounge shows).
  - `cinema/tv/Guide.tsx` (a .tsx change that is not an inline colour; said here): the guide page is a nested `ThemeProvider section="live-tv"` that always resolved `sideroom-broadcast` (dark, cream text, transparent background) on the room's white page. When the Cinema's theme is not a `sideroom*` one, the guide now gets the Cinema's theme. Side Room themes: unchanged.
- Substitution from D-802's sketch: `--sr-gold-ink` is `var(--ui-bg)`, not `--ui-accent-ink`. Gold is the link colour (readable as text on the page), and the page background reads on that fill as well as the link reads on the page (white ink on Lounge Dark's `#00a8fc` link would be 2.7:1; the page background is 4.8:1; Lounge Light: white on `#4752c4`, 6.4:1). The Library builder chose accent/accent-ink with a `--lib-fill`: same look family, a slightly different fill colour.

## Not done
- Nothing dropped. Tail done: Lounge Dark as the Cinema's theme, home and Browse at desktop (2 shots).

## What I saw on the base (I ran it, in the first lock round: the 5 files put back to `8694067e`, restored with `git checkout HEAD`)
- Light Cinema: `.cin-logo-name` `rgb(236,230,214)` and `.cin-footer` `rgb(169,164,147)` on the white page (the spec asserts this on the base and passed). Nav, brand, section bar, footer, row titles and card titles 1.25 to 2.5:1 (38 elements below 4.5 on home); chips black blocks; the animation banner dark olive; the Live TV guide cream on white with dark `.tv-chan` cells. Base shots: `visual-sweep-shots/p78-cinema-base` (I opened the light home only).
- After: in light, 0 text elements below 4.5 (3 for large) outside the picture surfaces on home, Browse, title and TV (asserted in the spec).

## Computed colours (home)
| | shipped (base equals after) | light before | light after |
|---|---|---|---|
| `.cin-root` bg / text | 17,17,16 / 236,230,214 | 255,255,255 / 49,51,56 | same |
| `.cin-logo-name` | cream | cream (fault) | 49,51,56 |
| `.cin-footer` | 169,164,147 | 169,164,147 (fault) | 92,94,102 |
| `.cin-card h3` | cream | cream (fault) | 49,51,56 |
| `.cin-hero h1` | cream | cream | cream (kept) |

The spec compared a longer list (root, section bar, brand, header, nav link, footer, footer brand, card title, hero, hero title, quick action), shipped base versus after, desktop and phone: equal.

## Surfaces kept dark (selector, file)
`.cin-hero`, `.cin-title-head`, `.cin-qa` (cinema.css); `.cpl-player` incl. live (player/player.css); `.tv-crt` (tv/tv.css); `.wp-root` (watchparty.css).
Literal colours left: scrims `rgb(0 0 0 / x)`, `#000` video, trailer and stage backgrounds, the hero shade (inside the dark hero), danger fallbacks, live.css and the rest of player.css (all inside `.cpl-player`). Changed: the card age badge only. Filled gold (chip, `tv-btn-primary`, the kit's gold button) = link colour with the page background as ink; readable in the light shots.
Kit parts on dark surfaces: `--ui-*` bg/border/text/focus restated (not `--ui-accent`), so hero and theatre buttons are dark. Looks different from Side Room but fine: in a light Cinema the theatre's "1 WATCHING" badge and "Send", and the Live TV watch page's "Try again", use the theme's blurple accent on the dark surface (gold in Side Room). Kit not mine; not changed.

## Who imports my files (`git grep`)
`cinema.css`: `CinemaRoot.tsx` (lazy route). `player.css`: `Player.tsx`, `MyListPage.tsx`, `LiveView.tsx`, `MiniPlayer.tsx` (lazy: `MiniPlayerMount`, which `main.tsx` imports, loads it with `lazy()`), `PlayerFrame.tsx` (unused: `createPlayerRootRoutes` is not mounted). `tv.css`: `Guide.tsx`. `live.css`: `LiveView.tsx`. `watchparty.css`: Theatre, WatchPartyPage, StartPartyDialog (Cinema) and `PartyMessageCard.tsx`, `WatchPartyStrip.tsx` (imported only by Lounge files). None is first paint or a public page: no web build run.
Mini player (by reading): `.cpl-mini-root` has its own `ThemeProvider` mounted at the app root, outside `.cin-root`, so the mapping never applies; it looks as today.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-cinema`)
52 PNG (shipped and light, desktop and phone: home, home again, Browse, Browse filters, search overlay, title, player, submit, My list, Live TV guide, Live TV watch, watch-party theatre, plus the phone menu) + 2 tail (`home-dark-desktop`, `browse-dark-desktop`). Opened all of the final run, except that the 16 light-theme shots of the first run were the ones I looked at: the final run differs only by the `.cin-qa` anchor rule (the light home was not re-opened). `data-theme` read back from `.cin-root` before every shot. The hero, backdrop and posters are SVG fakes injected into the real API answers; titles, the TV channel and the watch party are real. The player and TV watch pages show their own "not ready / off the air" messages (no media on a fresh stack), on black.
Click-through in the spec (shipped and light): Browse filters open/Escape, a genre chip and All, search overlay open and closed with Escape, the 8 header links (desktop), phone menu.

## Tests
- `e2e/tests/p78-cinema-shots.spec.ts` (local only: `SR_P78_CINEMA=1`; `SR_P78_BASE=1` base case; `SR_P78_TAIL=1` tail). Base run passed asserting the fault; after run: 2 passed. Vitest `cinema/tv/__tests__/guide.test.tsx`: 29 passed after the Guide.tsx change. Web and e2e `tsc`, eslint on Guide.tsx, prettier on the changed files, `i18n-audit --max 0`, `check-css-tokens`, `check-doc-links`: clean. After the final `.cin-qa` rule only the spec was re-run (CSS only).

## Docs
Nothing of mine changes. Outside my files: `docs/10-design-system-and-theming.md`, table row "Live TV page | `sideroom-broadcast` (extends `sideroom`)": add "While the Cinema wears a Lounge theme chosen for it (D-803) the guide page follows that theme; only the CRT preview keeps its dark frame."

## Proposed inventory row
`SEC-AUD-304`: the Cinema is whole under any section theme (D-803): its `--sr-*` tokens are mapped from the theme's `--ui-*` under any non-Side-Room theme; hero and backdrop, quick actions, player, CRT screen and watch-party theatre stay dark; the Live TV guide follows the Cinema's theme. Closed. Threat-model rows: none.
Changelog sentence: The Cinema, Live TV included, now looks right in any theme you choose for it, light ones too; the player, the TV screen and text over posters stay dark.

## Owner questions
- The Live TV guide now follows a Lounge theme chosen for the Cinema (it used to wear the dark broadcast look regardless, unreadable on a light page). Say if it should stay a dark page.
- The watch-party theatre stays fully dark (stage, header, chat panel, controls) in a light Cinema; D-803 says "stage", and a half-light theatre looked wrong.

## Housekeeping
Scratch DB `sr_p78_cinema_ui` dropped; `p78-cinema-bin` deleted; stack stopped by PID; lock released. No stash touched. Base shots kept at `visual-sweep-shots/p78-cinema-base`.

## Follow-up: width of the desktop home shots (commander's question)
The final-run home shots were 1633px wide because the Discover rail had 9 cards (the DB had accumulated titles over three runs; the base run had 3). Measured with `SR_P78_OVERFLOW=1` (spec test "page-level overflow", 1280x800, `/cinema`): 3 titles, `scrollWidth` 1280 in shipped and light, base and after. 9 titles: `scrollWidth` 1632 in shipped and light, on the base (my CSS and Guide.tsx put back to `8694067e`) AND after, identical. Cause: `.cin-block`, `.cin-row-wrap` and `.cin-row` grow past the viewport when a rail holds more than about 5 cards (the page, not the rail, scrolls sideways). So it is pre-existing and data-driven, not from this change; not fixed (not in my brief). The dark strip at the right of those shots is the page background beyond the 1280px wrapper. Candidate for a next plan: a rail with 6+ titles widens the Cinema home page on desktop and phone.
