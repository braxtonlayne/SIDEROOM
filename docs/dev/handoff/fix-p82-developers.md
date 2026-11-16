# Wave 82 handoff: developers (SEC-AUD-337, D-841; SEC-AUD-338, D-842; finding: the top navigation)

T = 11:10 on 2026-11-16. No new item from 13:10, checkpoint 13:30, finished 13:55, hard limit 14:10.
Actual: spec and CSS written by 11:10; lock taken 11:11 (free at once); base runs (stylesheet = HEAD) and fix runs 11:11 to 11:27; stack stopped, `sr_p82_developers_ui` dropped, lock released about 11:28. One lock round, about 17 minutes. Web only, no cargo.

## Done (`apps/web/src/developers/developers.css` only; `tabs/Activities.tsx` NOT touched)
1. Width (D-841), rules added:
   - `:where(.dev-main) :where(p, li, td, dd) > code { overflow-wrap: anywhere }` (the Activities origin).
   - `.dev-doc-body :is(p, li, dd) { overflow-wrap: anywhere }` (a dotted scope in a doc list item).
   - `.dev-table :is(th, td) { overflow-wrap: anywhere }` (the gateway reference table, which has no other rule).
   - `.dev-bundle { grid-template-columns: minmax(0, 1fr) }` and `.dev-file-list code { overflow-wrap: anywhere }` (preventive: the base did not overflow there; the long file path sits in the file list's own scroller).
   - Phone block: `.dev-uri-list li > div { flex: 1 1 12rem; min-inline-size: 0 }` and `.dev-uri-list li > div ~ button { margin-inline-start: auto }` (mapping rows; phone only so 1280 boxes stay put). No `overflow: hidden` anywhere. Wave 81's `.dev-actions` rules are untouched, numbers included.
2. List rows (D-842), phone block: `.dev-uri-list li > .dev-media-row { flex: 1 1 auto; min-inline-size: 0 }`, `.dev-media-row code { overflow-wrap: break-word; min-inline-size: 0 }`, `.dev-media-row .dev-muted { white-space: nowrap }`, `.dev-uri-list li > .dev-media-row ~ :is(button, .dev-actions) { margin-inline-start: auto }`. The `min-inline-size: 0` on the code is an addition to the task's list: with `break-word` alone the 32-letter name's min-content is the whole word and the row overflowed by 105 px (measured on my first try).
- `e2e/tests/p82-developers-shots.spec.ts` (local only: `SR_P82_DEVELOPERS=1`, `SR_P82_BASE=1` for the base case, out `SR_P82_DEVELOPERS_OUT`).

## What I saw on the base (I ran it: the HEAD stylesheet swapped in, twice)
Width table (375; 1280 was 0 on every page; same figures Dark and Light; "empty" = app with nothing, "content" = two mapping rows (one with a 60-char target), a real uploaded zip with a long file path, files listed):

| page | before | after | node | rule |
|---|---|---|---|---|
| every Activities state (empty, mappings, bundle, files) | 107 | 0 | the `<code>` in `<p>` holding the Activity origin `http://<18-digit id>.activities.localhost:5604`, 449 px wide; not the mapping rows, the bundle rows or the file list | `p > code { overflow-wrap: anywhere }` |
| docs: authentication | 18 | 0 | text of `ul.dev-doc-list > li` ("applications.commands.permissions.update": dots give no break point; the box fits, the text spills) | `.dev-doc-body :is(p, li, dd)` anywhere |
| docs: gateway-events | 194 | 0 | `table.dev-table` (no rule at all), the Notes column | `.dev-table :is(th, td)` anywhere |
| Overview, applications list, Packages, the other 8 tabs, the other 9 docs pages | 0 | 0 | | |

Wave 81's 107 px figure confirmed. Bundle rows and mapping rows were not over (fields 241 px, wrapped).
- At 1280 (Dark and Light): worst box move on the Activities tab (mappings and bundle states), Rich Presence, Emojis and OAuth2: 0 px (`boxes-after.json` against `p82-developers-base/boxes-base.json`).
- Mapping rows at 375 after: path field 309 wide, target field 221 wide, "Remove" at the row's right end (gap 0) beside the target; both fields over 160.

## List rows at 375 (D-842)
Base: dashed name "art-desktop- / mwjftvwy" in two lines, "Small image" in two lines, "Delete" at the row's left (x=0) on its own line; the 32-letter name in two lines.
After: "Small image" and "Large image" on one line in every row; "Delete" (and "Rename, Delete" on emoji rows) at the row's right end (gap 0). The dashed name still breaks, at its dash only: picture 48 + name 160 + kind 75 + gaps does not fit the 309 px row, which D-842 allows (never inside a word); the short "gone-xxxxxxxx" stays on one line. The 32-letter name breaks 16 + 16 and nothing is past the card (17 px to spare). The action sits on its own line at the end because the name and kind fill the row (on the short rows 280 + 66 > 309). At 1280 no row moved (0 px); the redirect URI rows on OAuth2 are unchanged (0 px).

## Finding: the top navigation at 375
Strip `scrollWidth` 482 against `clientWidth` 343 (Overview 16..74, Applications 94..170, Packages 190..250, Documentation 270..363, Back to the lounge 383..498). It scrolls by touch (a real touch-event drag moves scrollLeft from 0 to 139). At load `data-overflow="end"` on Overview, Applications, Packages and an application page: the right 24 px fade is on, so "Documentation" (right edge 363, strip edge 359) shows its last letters faded and 4 px cut ("Documentatio" in the shot). That is the designed hint. The selected item is in view on every page (on Documentation the strip scrolls 20 px by itself, selected 250..343, both fades). Nothing broken, nothing changed; if the owner dislikes the cut letter, the one rule is a smaller fade width or a `padding-inline-end` on `.dev-nav`.

## Tests and checks
`node scripts/check-css-tokens.mjs` clean (73), `node scripts/i18n-audit.mjs --max 0` TOTAL 0, prettier clean on both files, e2e `tsc` clean, `developers/__tests__/portal.test.tsx` as a single file: 45 passed. No `.tsx` touched, so no web typecheck. `developers.css` is imported only by `DevelopersRoot.tsx` and `Authorize.tsx`, both lazy (the Developers chunk): no web build, no bundle check run. Shots spec on the fix run: 1 passed (soft assertions: 0 width on every page and size, 1280 moves at most 2 px, mapping fields at least 160, kind label on one line, action at the end, nothing past the card). The base run asserts nothing and logged the faults above.

## Shots
`visual-sweep-shots/p82-developers` (20 PNG + 2 json): Activities with content (bundle listed, and the mapping rows), Dark and Light, 1280 and 375 (8); Rich Presence and Emojis lists, Dark and Light, 1280 and 375 (8); click-through (mapping added and typed in both fields, then removed; asset deleted; Light 375) (2); the navigation at 375, load and after a touch swipe, on Overview and Documentation (4). `p82-developers-base` holds the base run's 20 PNG. **I opened all 20 after shots, plus 2 base shots (Activities mappings Dark 375, Rich Presence Light 375); the other 18 base shots I did not open (their figures are in the log).** Themes: Lounge Dark and Light set through the account's user_prefs; `data-theme` read back (`lounge-dark`, `lounge-light`).

## Proposed inventory rows
- SEC-AUD-337 (closed): "An application's Activities tab scrolled sideways by 107 px at 375 (the Activity origin `<code>` in a paragraph had no break rule), the documentation page on authentication by 18 px (a dotted scope in a list item) and the gateway reference by 194 px (an unstyled table). `developers.css`: those tokens break inside their box (D-841). No Developers page scrolls sideways at 375 or 1280."
- SEC-AUD-338 (closed): "A Developers list row on a phone (Rich Presence asset, emoji) broke names mid-word, wrapped 'Small image' and dropped the action to the row's start. `developers.css` phone block: a name breaks at a dash, or inside a word only when the word is too long for the row; the kind label is one line; the action sits at the row's end (D-842)."

## Changelog sentences
- The Developers pages no longer slide sideways on a phone: an Activity's address, a scope name and the gateway table wrap inside their box.
- On a phone an application's emoji and Rich Presence lists keep short names whole, keep "Small image" on one line and put Rename and Delete at the row's end.

## Owner questions
1. The dashed asset name still breaks at its dash at 375 because picture, name and kind do not fit one 309 px line. Default: kept (D-842 allows it). The kind label could go under the name on a phone if the owner wants the name whole.
2. The navigation's last link is faded and 4 px cut at load on a phone. Default: kept (the scroll hint).

## Clean-up
Stack stopped by the PIDs listening on 5604 and 8604 (metrics went with the server); `sr_p82_developers_ui` dropped; `p82-developers-bin` deleted (`p80-base-bin` untouched); lock released. Scratch (logs, stylesheet copies, scripts; no secrets) is in `SIDEROOM_wt/scratch/p82-dev`. No stash seen or touched; nothing under `apps/web/src/developers/data/`.
