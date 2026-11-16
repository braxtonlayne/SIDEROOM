# Wave 83 handoff: themes (SEC-AUD-345 / D-852, SEC-AUD-346)

T = 12:10 on 2026-11-16. Clock times: no new item 14:10, checkpoint 14:30, finished 14:55, hard limit 15:10. Mine: started 12:06, lock taken 12:28 (lounge and settings got the lock first; I waited from 12:11), released 12:35, handoff 12:38.

## Done (branch `fix/p83-themes`)
- `c3e9dc33` SEC-AUD-345 + 346: the four pages use `LoungeThemeProvider`; two unused keys deleted; shots spec added.
- Second commit: SEC-AUD-345 follow-up, `className="ss-root"` dropped from `SupportPage.tsx` and `Leaderboards.tsx` (see "Fault found").
- This handoff.

## Base (run, SR_P83_BASE=1; 24 measurements, 4 shots opened)
All four pages read `data-theme="lounge-editorial"` in every main theme (Editorial, Lounge Light, Lounge Dark), both sizes, with the main theme read back from the API each time. No page followed the main theme on the base, so none was left alone. Base in Lounge Light (1280): the four shots opened, all dark Editorial.

## Files: changed / put back
All four changed, none put back. Support and Leaderboards use the provider (`<LoungeThemeProvider>`; the import of `ThemeProvider` removed; Leaderboards' import list re-wrapped by prettier). The two `app/` files import it **statically** (the lazy fallback was not needed).

## Fault found (the plan did not know): `.ss-root { display: contents }`
`settings/server/server-settings.css:2` gives `.ss-root` `display: contents`. The Leaderboards import `settings/server/ui`, which loads that sheet, and a loaded sheet stays for the whole session (so Support too, after a visit to the Leaderboards or a server's settings). A `display: contents` root paints no background and ignores the provider's `min-block-size: 100dvh`. First run after the swap: Leaderboards in Lounge Light was dark ground with dark text (not whole). Fix in my own files: Support and Leaderboards no longer carry `ss-root`; nothing else styles `.ss-root` on these pages (`admin/admin.css:575-590` only targets `.ss-toolbar` descendants, which these pages do not have). That is a deviation from "same class", and it is why Editorial's Leaderboards ground is now the theme's `rgb(35,34,29)` instead of the bare page's `rgb(17,17,16)`. Proposed, for the guild builder / commander: `AdminApp.tsx:530` and `ServerSettings` keep `ss-root`; nothing to change there.

## Build (first paint)
`pnpm --filter @sideroom/web build` once (37 s). First-paint chunk `app/app-C0ydrAVQ.js`: **218 309 bytes gzip, room 37 691 bytes** (wave 79: 215 935 / 40 065; +2 374 bytes). `check-bundle-budget`: ok. `check-entrance-isolation`: ok (every public surface clean). `check-staff-delivery`: ok. Static import kept. `apps/web/dist` deleted (ignored by git).

## data-theme table (after; every row equals the main theme)
| page | 1280 Editorial / Light / Dark | 375 Editorial / Light / Dark |
|---|---|---|
| /support | editorial / light / dark | editorial / light / dark |
| /support/leaderboards | editorial / light / dark | editorial / light / dark |
| invite in the app | editorial / light / dark | editorial / light / dark |
| template in the app | editorial / light / dark | editorial / light / dark |
Root ground: Editorial `rgb(35,34,29)`, Light `rgb(255,255,255)`, Dark `rgb(49,51,56)`; reaches the viewport bottom on all 24; `scrollWidth == clientWidth` on all 24. Editorial against the base: **0 boxes moved by more than 2 px** (root height exempt; the Leaderboards root had no box on the base, `display: contents`).

## Light-theme faults seen and not fixed
None. Every one of the 24 shots opened: texts, buttons, tabs, notice, cards whole; accent is the theme's (blurple in Lounge themes, gold in Editorial). The invite card's `components/entrance.css` rules are whole in Light and Dark. Caveat: the template page is shot in its "does not exist" state (`/template/abcd1234`, the sweep's address); the ready state (name box and button) was not reachable from the sweep world. Contrast probe: 0 of 40 / 14 / 4 / 1 sampled texts under 4.5:1 on every page and theme.

## Click-through (Lounge Light, 375 and 1280, passed)
Lounge, Support, Leaderboards (client link), back to Support (with server-settings.css loaded), the invite, its Accept button back into the Lounge: `data-theme` `lounge-light` at every step, the Lounge included. Opened: phone step 4 and step 6; the other 10 click shots are data-theme checked only.

## SEC-AUD-346: grep
`git grep -n "library.browse.listMeta\|library.series.chapterCount" -- apps packages e2e scripts docs`: only `en/library-3.ts:80`, `en/library-4.ts:30`, their successors `...Plural` (p81-library-1.ts, used by `Browse.tsx:329`, `SeriesPage.tsx:343`: different keys) and wave 81's handoff text. No `t()` call, test or doc used the old keys; no `en-staff/` copy. Both lines deleted. Checks: web `tsc`, `i18n-audit --max 0` (TOTAL 0), `src/i18n/catalog.test.ts` (8 passed). Docs: `check-doc-links` ok; no sentence in `docs/` (outside the decisions log and handoffs) names which pages follow the main theme, so nothing to replace (`docs/10` is the commander's: add Support, Leaderboards, in-app invite/template to its list if it has one).

## Tests
`src/settings/server/__tests__/support.test.tsx` (the only test that renders Support/Leaderboards; none for the invite pages): 11 passed after the change, no change needed. Not run on the base (the fault is visible only in a browser). The shots spec `e2e/tests/p83-themes-shots.spec.ts`: base run failed nothing (records only); after run passes, including the Editorial comparison against the base file. Web eslint and prettier on the four files and the two fragments, e2e prettier and `tsc`: clean.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-themes` (24 page shots + 12 click shots, `measure.jsonl`) and `p83-themes-base` (24 shots, `measure.jsonl`; its Leaderboards `rootBox` is 0x0 because of `display: contents`, as above). Opened: all 24 after, the 4 required base shots, 2 click shots. The base folder was run twice; the spec was improved between runs (the later after run uses the `closest('[data-theme]')` root).

## Proposed inventory rows
- SEC-AUD-345: Support, the Leaderboards and the in-app invite and template pages followed the Lounge's default theme whatever the member chose (bare `ThemeProvider`); they use `LoungeThemeProvider` (D-852); `ss-root` dropped from Support/Leaderboards because `server-settings.css` makes it `display: contents`.
- SEC-AUD-346: the two unused chapter-count keys (`library.browse.listMeta`, `library.series.chapterCount`) deleted.
Changelog sentence: "Support, the Leaderboards and an invite or template opened inside the app now follow the theme you chose, and two unused text keys were removed."

## Owner questions
- Should the template "ready" state be shot too? (Needs a real template code in the sweep world.)
- `.ss-root { display: contents }` in `server-settings.css` is a trap for any page that reuses the class with a themed provider; the guild builder may want a more specific selector (not mine).

## Cleanup
Stack stopped by PID (sr-server, Vite), `sr_p83_themes_ui` dropped, `p83-themes-bin` deleted (`p80-base-bin` untouched), `apps/web/dist` deleted, lock released. No stash touched.
