# Wave 81 cinema handoff (SEC-AUD-329, D-831; SEC-AUD-330, D-832)

T = 09:35 on 2026-11-16. Clock times: no new item 11:35, checkpoint 11:55, finished 12:20, hard limit 12:35. Mine: started 09:34; waited for the lock (tools and arcade ahead) 09:37-10:03; lock round 10:03-10:09 (base run 10:04, after run 10:06); handoff written about 10:15. Web only: no cargo, no Rust, no migration, no fragment, no web build. Binaries copied from `p80-base-bin`.

## Done (commits on `fix/p81-cinema`)
- The CSS (`cinema.css`, `player/player.css`, `tv/tv.css`, `watchparty/watchparty.css`) and the local-only spec `e2e/tests/p81-cinema-shots.spec.ts` (`SR_P81_CINEMA=1`, `SR_P81_CINEMA_OUT`, `SR_P81_BASE=1`).
- **329:** each of the four blocks states `--ui-status-online: #34c25f; --ui-status-idle: #d9a441; --ui-status-dnd: #cc5c47; --ui-status-offline: #6f6c60; --text-link: #c2b08f; --sr-olive-accent: #8c9a6a;`. No radius, no font. `cinema.css`'s comment has the extra sentence. **The task's status values (`#23a55a`...) are the Lounge Dark defaults (`loungeShared`), not `sideroom`'s; `themes.ts:281-284` and the probe under `sideroom` read `#34c25f / #d9a441 / #cc5c47 / #6f6c60` (the probe wins; the commander confirmed).**
- **330:** `.cin-hero, .cin-title-head { background-color: var(--sr-bg-0); }` in the rule that sets `color`. Nothing else.

## What I saw on the base (RAN: the four CSS files restored from `12d480b8` with `git show`, `SR_P81_BASE=1`)
Six tokens as computed inside each block (hero, quick actions, title head, CRT, theatre, Live TV player), real stack:

| Cinema theme | online / idle / dnd / offline / link / olive |
|---|---|
| `sideroom` (base = after) | `rgb(52,194,95)` / `rgb(217,164,65)` / `rgb(204,92,71)` / `rgb(111,108,96)` / `rgb(194,176,143)` / `rgb(140,154,106)` |
| `lounge-light`, base | `rgb(22,138,73)` / `rgb(176,120,0)` / `rgb(229,56,59)` / `rgb(128,132,142)` / `rgb(71,82,196)` / `rgb(26,127,69)` |
| `lounge-light`, after | the `sideroom` row, in all six blocks (asserted) |
| `lounge-dark`, after | the `sideroom` row, in all six blocks (asserted; not run on the base) |
| `sideroom-broadcast`, after | the `sideroom` row (asserted). Broadcast spreads `sideroom.tokens`, so it computes the same six values; no scoping needed. The Cinema section took `sideroom-broadcast` as its theme and read it back. |

Under `sideroom` and `sideroom-broadcast` the six colours equal the base's (the spec compares `computed-base-*.json` with the after run, 5 probe sets, asserted). The TV page's `.tv-crt` sits under `sideroom-broadcast` in shipped and under the section theme when a light one is chosen.

Ground (`getComputedStyle(head).backgroundColor`): `rgba(0, 0, 0, 0)` on the base; `rgb(17, 17, 16)` after. Pixels behind the meta line and synopsis (text hidden in a second shot; left/middle/right x top/middle/bottom of each text box), contrast of the text colour `rgb(169,164,147)`:
- Base, light, no picture: worst contrast **1.50** (home 1280), **1.20** (title 1280), **1.33** (home 375), **1.33** (title 375). E.g. title 1280 meta: right samples `rgb(180,180,180)` (1.2:1), middle `rgb(73,73,72)` (3.61:1), left `rgb(25,25,24)` (7.05:1); home 375 synopsis right `rgb(219,219,219)`.
- After, light, no picture: worst **7.57 / 7.51 / 7.57 / 7.57**, every sample `rgb(16..18)`. Picture request answered 404 (a broken image): the same, worst 7.57 / 7.51 / 7.57 / 7.51, all four cases (asserted at least 4.5).
- Shipped theme, no picture: base and after pixels identical in all 18 samples per surface and size (asserted, and the shots opened side by side).
- With a picture: ground dark; picture and shade look exactly as in the shipped theme. Contrast there depends on the picture (my fake is a bright blue disc behind the synopsis: 3.4 to 4.8:1), identical in shipped and light, not asserted (the plan's claim is for no picture); the fix does not change it.

## Findings only: the guide's "Try again" (`Guide.tsx:236`, guide request answered 500 by `page.route`)
Whole, shipped and light, 1280 and 375: "We could not load the guide." and the "Try again" `.tv-btn` are readable on the page's own ground. Shipped: text `rgb(169,164,147)`, button `rgb(236,230,214)` on transparent with a `rgb(42,43,36)` border. Light: text `rgb(92,94,102)`, button `rgb(49,51,56)` with a `rgb(201,204,209)` border. Clicked in all four: the guide is requested again (1 -> 2). Nothing is wrong.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-cinema`: 36 shots (plus 28 "textless" measurement shots with the text hidden), **I opened all 36**: title without a picture shipped/light x 1280/375 after (4) and base (4); the home hero likewise (8); title and hero with a picture shipped/light x both sizes (8); picture answered 404, light, both sizes, home and title (4); the guide's "Try again" shipped/light x both sizes (4); the theatre and the Live TV watch page, light 1280 (2); click-through (open a title from the home, press My list, back), light, both sizes (2). Each after the sideways-scroll measurement (`scrollWidth` equals `clientWidth` at 1280 and 375, asserted) and with `data-theme` read back (asserted). Theatre and Live TV look as in wave 80 (nothing moved). Click-through: title opened, "My list" pressed (check mark at 1280), back on `/cinema`. `report-base.txt`, `report-after.txt`, `computed-*.json`, `pixels-*.json` are in the folder. A first run failed on a spec bug (a raw `PATCH` answered 403); fixed with `apiPatch`; `p81-cinema-run0` holds that run.
Small observation: with the picture answered 404 the hero shows a 1 px broken-image mark at its top-left corner (the `<img alt="">`), shipped and light alike; not part of this task.

## Checks
prettier clean; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; `node scripts/check-css-tokens.mjs` clean; e2e `tsc` clean; the shots spec (`--project=chromium --reporter=line`): base run passed, after run passed. `git grep` finds `tools/__tests__/overlayZoom.test.tsx` reading `tv.css` (z-index of `.tv-pop`): ran it, 12 passed. Lazy chunks (checked by `git grep` of the importers): `cinema.css` by CinemaRoot, `player.css` by the player/LiveView/MyList files, `tv.css` by Guide, `watchparty.css` by the Theatre, StartPartyDialog and the strip (Lounge chunk). None by first paint or a public page: no web build.

## For the commander
- Inventory SEC-AUD-329: "The four dark Cinema blocks state `--ui-status-online/idle/dnd/offline`, `--text-link` and `--sr-olive-accent` with the Side Room values; in a light or Lounge Dark Cinema every probe equals them, `sideroom` and `sideroom-broadcast` unchanged (D-831). Closed." SEC-AUD-330: "`.cin-hero` and `.cin-title-head` paint `--sr-bg-0`; with no or a failed backdrop the text sits on `rgb(17,17,16)` (contrast 7.5:1) in a light Cinema at 375 and 1280 (D-832). Closed."
- Plan correction for D-831's text: the status values are `#34c25f`, `#d9a441`, `#cc5c47`, `#6f6c60` (from `sideroom`), not the Lounge defaults.
- Changelog 329: "In a light or dark Lounge Cinema, the kept-dark surfaces (hero, title page, player, Live TV screen, watch-party theatre) keep the Side Room's status, link and olive colours." 330: "The Cinema's hero and title page keep their dark ground when a picture is missing, so their text stays readable in a light Cinema."
- `docs/10-design-system-and-theming.md`: as the plan says ("A surface kept dark states every colour token (status, presence and link colours included) and paints its own ground; radius and fonts follow the theme (D-831, D-832)").
- Owner questions: none. `live.css`'s literal colours left as answered. No stash touched.

## Cleanup
Stack stopped by PID (listeners on 5594/8594 gone), lock released, `sr_p81_cinema_ui` dropped, `p81-cinema-bin` deleted (never `p80-base-bin`). Untracked `scratch-run/` in the worktree, not committed.
