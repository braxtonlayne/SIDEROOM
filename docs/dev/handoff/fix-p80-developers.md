# Wave 80 handoff: developers (SEC-AUD-324, D-825)

T = 07:30 on 2026-11-16. Clock times: no new item from 09:30, checkpoint 09:50, finished 10:15, hard limit 10:30.
Actual: code and spec done 07:33; lock round 1 08:09 to 08:13 (base case, change, shots); round 2 08:44 to 08:45 (select pages repeated); the rest without the lock.

## Done
- `apps/web/src/developers/developers.css`: one rule for the Developers' native selects; the phone nav's scrolling, hidden scrollbar, `flex: none` links and 24 px fade (`data-overflow` `start`, `end`, `both`, mirrored under `:dir(rtl)`, inside the 720 px block; desktop unchanged). One small extra: `.dev-file { align-content: start }` (see below).
- `apps/web/src/developers/DevelopersRoot.tsx` (the `.tsx` that renders `.dev-nav`): a ref on the nav, `useCurrentLinkInView(navRef, "[aria-current='page']", pathname)`, and one effect that sets or removes `data-overflow` from `scrollLeft`, `scrollWidth`, `clientWidth` (1 px tolerance) on mount, on `scroll`, on a guarded `ResizeObserver` and when the pathname changes. No new prop, no new string.
- `e2e/tests/p80-developers-shots.spec.ts` (local only: `SR_P80_DEVELOPERS=1`, `SR_P80_DEVELOPERS_OUT`; `SR_P80_BASE=1` is the base case, Dark only).
- Commits `SEC-AUD-324: ...` on `fix/p80-developers`.

## The select rule
- Selector: `:where(.dev-main, .dev-authorize) select:not([class])` and `:where(.dev-main, .dev-authorize) select.dev-select` (plus `:hover:not(:disabled)`, `:focus-visible`, `:disabled`). Bare selects have no class; the four with `className="dev-select"` are named. A kit `Select` carries a module class and is not matched. `.dev-select` kept only `max-width: 100%`; its old `--ui-bg-hover` background and 9px 10px padding are replaced by the kit field's (`--ui-bg-raised`, 8px 12px, `min-block-size: 40px`, 1px `--ui-border`, `--ui-radius`, `font: inherit`, focus ring `2px solid var(--ui-focus)` offset 1px, disabled `opacity .55`).
- Selects found under `developers/**` (6 files, 7 selects): `tabs/Media.tsx` (Rich Presence "Type", bare), `Reference.tsx` (API reference "All areas", bare), `tabs/Commands.tsx` x2 (command type, option type), `tabs/OAuth2.tsx` (redirect URI), `Authorize.tsx` (server picker on `/oauth2/authorize`, inside `.dev-authorize`, not `.dev-main`; **not looked at**: it needs a valid authorize request with a bot scope; covered by the selector and by reading).
- Computed look against a kit text field on the same page (spec `selectLook`, asserted): background, colour, border colour, radius equal in Lounge Dark (`rgb(43, 45, 49)`, `rgb(219, 222, 225)`, `rgb(63, 65, 71)`, 8px), Lounge Light (`rgb(242, 243, 245)`, `rgb(49, 51, 56)`, `rgb(201, 204, 209)`, 8px) and Editorial; block size 40 = the field's 40 on desktop and phone (after the `.dev-file` fix). The option list is the browser's.

## What I saw on the base (I ran it: `SR_P80_BASE=1`, my two files replaced by `c8982549`'s, Lounge Dark)
- Rich Presence "Type": `background-color` `rgb(59, 59, 59)` (the browser's dark-scheme field, not white), border `rgb(133, 133, 133)`, radius 0, font Arial, block 19 on a phone; a kit text field on the same page: `rgb(43, 45, 49)`, `rgb(63, 65, 71)`, radius 8, block 40. The plan's `rgb(255, 255, 255)` is not what Chromium paints here; the fault (default look on a themed page) is as described.
- Phone nav: `scrollWidth` 482 > `clientWidth` 343, no `data-overflow`; "Back to the lounge" right edge 498 (nav client right 359); Documentation page: current link 270..363, partly outside. Home scrolled to the end: no cue.

## The nav figures after (375x812, Dark and Light the same)
- Home at load: `end`, scrollLeft 0, current link 16..74 inside 16..359. Scrolled to the end: `start`, scrollLeft 139, "Back to the lounge" 244..359.
- Application page at load: `end`, current 94..170. Documentation at load: `both`, scrollLeft 20, current 250..343 inside 16..359; "Back to the lounge" 363..478 (inside `.dev-nav`, reached by scrolling; the fade says so).
- Pressing Documentation, Packages, Applications, Overview in turn: the pressed link ends inside the nav each time.
- The document does not scroll sideways on any page at 375 or 1280. At 1280: no attribute (482 = 482).
- "Back to the lounge" pressed after scrolling to it: leaves to `/channels/@me`.

## Not mine / not changed
- The Rich Presence "Upload" button is stretched about 80 px high beside the field row (as on the base). Owner question 1.
- On desktop the select first measured 51 px: `.dev-file` is a grid label whose rows stretch. Fixed with `align-content: start` on `.dev-file` (used by the Emojis and Rich Presence forms only).
- Tail (file inputs in the disabled state): no `type="file"` input is ever disabled in `apps/web/src`, so nothing to shoot. Findings: none.

## Imports and chunks
`DevelopersRoot.tsx` is loaded by `developers/routes.tsx` with `lazyNamed(() => import('./DevelopersRoot'))` (the Developers lazy chunk); `developers.css` is imported by it. The hook's only import is `study/scrollBarTo.ts` (no `RoomsNav`, nothing else of `study/`). No web build (nothing first paint or a public page loads).

## Bars' file
`apps/web/src/study/useCurrentLinkInView.ts` was copied from bars' committed version into this worktree for typecheck and the runs. **It is not committed here** (`git status` shows it untracked; delete the copy if bars is not merged first). Merge bars before developers.

## Rings
Switched on in the spec with `PATCH /admin/features/lounge.rings` `{enabled: true, reason}` as the Owner (as `moderation-gate.spec.ts` does; worked first time; in this scratch DB only). Rings composer, Photo, real feed (no `page.route`): Dark and Light, 1280 and 375 (4 shots). The file input looks as wave 79 measured: picker button in the kit's secondary look (Dark `rgb(53, 55, 60)`, Light `rgb(232, 234, 237)`, radius 8), the input inside the dialog (right edge 900 of 921 desktop, 339 of 360 phone), nothing cut or touching.

## Tests
Web typecheck (bars' file in), eslint and prettier on my files, `i18n-audit --max 0` (TOTAL 0), `check-css-tokens` (73 clean), e2e `tsc`: all clean. `developers/__tests__/portal.test.tsx` (renders the root): 45 passed. Shots spec: base case run (faults logged); the change: passed (rc 0).

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-developers`; base-case shots in `p80-developers-base` (10, not all opened: their figures are in the log above). Final folder: 34 PNGs, **all 34 opened by me**: select closed and focused (Dark/Light x desktop/phone, 8), Editorial desktop (1), chosen value on a phone (1), phone nav at load and scrolled to the end on home, an application page and Documentation (Dark and Light), the pressed-link shots (4), the desktop bar (Dark, Light), the four Rings composers, and the select pages (Commands both selects, OAuth2 redirect, API reference area). A second round (08:44 to 08:45) repeated the select pages because round 1 shot them scrolled away from the select; it passed with no failures. Faults seen in the shots: none of this item (the tall Upload button is owner question 1).

## Proposed inventory row
SEC-AUD-324 (closed): "The Developers pages' native selects took the browser's default look on a themed page, and the phone nav clipped Documentation and Back to the lounge without a cue. `developers.css` gives every Developers select the kit field's look; under 720 px the nav scrolls inside itself without a scrollbar, fades at the edge that hides links (`data-overflow`) and brings the current link into view (`useCurrentLinkInView`, D-821, D-825)."

## Changelog sentence
The Developers pages' dropdowns now match the page, and their phone navigation bar shows where you are and fades at the edge that hides more links.

## AGENTS.md
No sentence needed.

## Owner questions
1. The Rich Presence "Upload" button is about 80 px tall next to the field row (as before). Default: left; `align-self: end` on it in `developers.css` would fix it.
2. The authorize page's server picker is covered by the rule but not looked at. Default: accepted.

## Clean-up
Stacks stopped by the PIDs listening on 5585, 8585 and 9285; scratch database `sr_p80_developers_ui` dropped; `p80-developers-bin` deleted; heavy lock released; my copy of bars' hook deleted from the worktree. No stash seen or touched. Lock rounds: 08:09-08:13 and 08:44-08:45.
