# Wave 80 dark handoff (SEC-AUD-319; D-820)

T = 07:30 on 2026-11-16. Clock times: no new item 09:30, checkpoint 09:50, finished 10:15, hard limit 10:30. Mine: started 07:28; lock rounds 07:44-07:59 (base check, first after run), 08:13-08:29 (after runs, spec fixes), 08:49-08:55 (final run, green); finished about 08:57. Web only: no cargo, no Rust, no migration, no fragment, no web build. Binaries copied from `p80-base-bin` (the build succeeded).

## Done (commits on `fix/p80-dark`)
- `44159d79` the CSS; later commits the local-only shots spec `e2e/tests/p80-dark-shots.spec.ts` (`SR_P80_DARK=1`, `SR_P80_DARK_OUT`, `SR_P80_BASE=1`) and this handoff.
- **Tokens added, per block** (`.cin-hero, .cin-title-head, .cin-qa` in `cinema.css`; `.cpl-player` in `player/player.css`; `.tv-crt` in `tv/tv.css`; `.wp-root` in `watchparty/watchparty.css`): `--ui-danger: #b4503f; --ui-danger-ink: #fff6f2; --ui-danger-text: #d97a68; --ui-success: #8c9a6a; --ui-warning: #c2b08f; --ui-info: #9aa8a0;`. A comment in `cinema.css` names the rule.
- **Comparison with the `sideroom` set** (grep of `var(--ui-` in the four files, and the kit parts used there: Button, TextInput, Badge, Tabs, Avatar, EmptyState): `--ui-text-faint`, `--ui-bg-overlay`, `--ui-bg-hover` and the rest were already stated in the `.cin-*`, `.cpl-player` and `.wp-root` blocks. Nothing else added, on purpose: `--ui-backdrop` (Modals portal outside every dark block), `--ui-shadow-overlay` (a shadow), `--ui-status-online/idle/dnd/offline` (the only kit user is Avatar, used in the People panel and the Cinema bar without a status prop, so no dot is drawn; `tv.css:267` reads `--ui-status-online` in `.tv-reception`, which is in the guide's channel panel, not in `.tv-crt`), `--ui-status-ring` (falls back to `--ui-bg`, stated). `.tv-crt` states no `--ui-bg*` or text tokens, before or now: nothing in it reads them. If you want `--ui-status-*` in the blocks too ("every colour token"), it is four lines per block.
- `live.css` and the rest of `player.css`: inside `.cpl-player`, need nothing (confirmed). `live.css:104-111` has literal error colours (`#c0584a`, `#f1b3a9`, `#e2584a`) on the black player: fixed colours, left alone; they could become `--ui-danger*` later.
- **`--sr-olive-accent: var(--ui-success)` does NOT recompute to the Side Room green inside a dark block.** A custom property holding `var()` is resolved where it is declared (`.cin-root:not([data-theme^='sideroom'])`) and inherited as that value: measured in a light Cinema it is `rgb(26, 127, 69)` on `.cin-root` and inside `.cin-hero`. Nothing inside a dark block reads it (its readers are `cinema.css:651-655`, the anime Browse page, outside the blocks), so there is no visible fault and I changed nothing. If a reader ever sits in a dark block, state `--sr-olive-accent: #8c9a6a` there.

## Confirm the fault: the base (RAN: the four CSS files restored from `c8982549` with `git show` into place, spec with `SR_P80_BASE=1`, `sectionThemes.cinema` set and read back)
Host in the theatre, computed:
| | shipped (`sideroom`) | light (`lounge-light`) |
|---|---|---|
| "End party" (filled danger Button): bg / ink | `rgb(180, 80, 63)` / `rgb(255, 246, 242)` | `rgb(209, 45, 53)` / `rgb(255, 255, 255)` |
| tokens inside `.wp-root`, `.cpl-player`, `.cin-hero`, `.cin-qa`, `.cin-title-head`, `.tv-crt`: danger, success, warning, info | `rgb(180,80,63)`, `rgb(140,154,106)`, `rgb(194,176,143)`, `rgb(154,168,160)` | `rgb(209,45,53)`, `rgb(26,127,69)`, `rgb(169,107,0)`, `rgb(0,104,201)` |
| Lounge Dark as the Cinema theme, `.wp-root`: danger, success | | `rgb(218,55,60)`, `rgb(35,165,90)` |
The fault was real. (The light value is Lounge Light's red `rgb(209, 45, 53)`; the plan's `rgb(217, 122, 104)` is the Side Room danger text colour.)
- **After:** every one of those is the Side Room value in light and in Lounge Dark (asserted); "End party" in light is `rgb(180, 80, 63)` / `rgb(255, 246, 242)`.
- **Disabled "Send"**: identical in both themes before and after: `background-color rgb(194, 176, 143)` (the accent wave 79 already stated), `color rgb(27, 26, 21)`, `opacity 0.5`, `cursor not-allowed`; enabled: same colours at `opacity 1`. It is the kit's own rule (`Button.module.css:23`, `opacity: 0.5`), the same as shipped. Changed nothing.
- **Side Room comparison** (`computed-base-*.json` against the after run, asserted by the spec): shipped desktop 12 entries and phone 6 entries equal: "End party", disabled and enabled "Send", the Live TV "Try again", the tokens in every block present (hero, quick actions, title head, CRT, theatre, player). Not compared to a base run: the title player's error "Try again" on the phone (the base phone run did not reach it: a spec timing problem, fixed after by faking `/progress`), and the theatre and Live TV error-text lines (`watchparty.css:425,602`, `tv.css:270,643`): they read `--ui-danger`, whose value inside every block is asserted equal; no element was forced to show them.
- **Turned round:** in light, every logged entry (12 desktop, 7 phone) equals the shipped one: asserted.

## "Try again" and the mini player
- Live TV watch page (tune answered 404 by `page.route`): "Try again" sits in `.cpl-player`, a dark block: gold `rgb(194,176,143)` with ink `rgb(27,26,21)` in shipped, light and Lounge Dark. Clicked in light, desktop and phone: the tune is requested again (1 -> 2). The guide's own "Try again" (`Guide.tsx:236`) is not in a kept-dark surface and was not looked at.
- Title player error state (stream playlist answering 404): reached in all four cases; "Try again" gold.
- **Mini player** (needs real playback: the spec serves a 240 s VP9 HLS clip made with ffmpeg from `scratch-hls/`, untracked, with the playback grant faked): in the Lounge under Lounge Light and Lounge Dark, `.cpl-mini-root` reads `data-theme="sideroom"`; a black card, gold progress line, readable title and three buttons, bottom right: whole in both. I ran no base comparison, so "unchanged by 78 to 80" is by reading (no file of `player/**` or the mini's CSS rules was touched). It takes the Cinema's section theme (shot only with `sideroom`).

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-dark`: 24, all opened and looked at, each after the sideways-scroll measurement (`scrollWidth` equals `clientWidth` at 1280 and 375) and with `data-theme` read back: theatre with disabled and enabled "Send" x shipped/light x desktop/phone (8); Live TV "Try again" x same (4) + Lounge Dark (1); title player error x same (4); theatre in Lounge Dark (1); home and title head, shipped/light desktop (4); mini player in Lounge Light and Dark (2). Also `report-base.txt`, `report-after.txt` and `computed-*.json`. The Lounge Dark theatre shows "This video is not ready yet": the real server has no media for the fixture titles, unrelated. Click-through (light): a chat line sent in the theatre and shown, both sizes; "Try again" pressed, both sizes.

## Checks
prettier clean on the five files; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; `node scripts/check-css-tokens.mjs` clean; e2e `tsc` clean; the shots spec (`--project=chromium --reporter=line`) 1 passed. Lazy chunks (`git grep`): `cinema.css` is imported by CinemaRoot; `player.css` by LiveView, MyList and the player files; `tv.css` by Guide; `watchparty.css` by PartyMessageCard (Lounge chunk), StartPartyDialog, Theatre, the strip. None by first paint or a public page: no web build.

## Owner questions
1. Under a light Cinema the dark surfaces still take the light theme's radius and font on kit parts (light "Send", "Try again", the badge have 8 px corners; shipped 2 px). The contract said no radius or font. State `--ui-radius*` and the fonts in the blocks too?
2. State `--ui-status-*` and `--sr-olive-accent` in the blocks as well (see above)?

## For the commander
- Inventory row SEC-AUD-319: "A Cinema surface kept dark (hero, title head, quick actions, player, CRT, theatre) states `--ui-danger`, `-ink`, `-text`, `--ui-success`, `--ui-warning`, `--ui-info` with the Side Room values; 'End party' is the Side Room red in a light Cinema; 'Try again' and the mini player were looked at (D-820). Closed."
- Changelog: "In a light Cinema, the watch-party theatre's buttons and the other dark Cinema surfaces keep the Side Room's status colours."
- `docs/10-design-system-and-theming.md`: "A surface kept dark states every colour token a kit part reads, the status colours included (D-820), so a button or badge on it is the Side Room's in every theme."
- No stash touched.

## Cleanup
Stack stopped by PID (listeners on 5583/8583/9283 gone), lock released, `sr_p80_dark_ui` dropped, `p80-dark-bin` deleted. Untracked `scratch-run/` and `scratch-hls/` in the worktree, not committed.
