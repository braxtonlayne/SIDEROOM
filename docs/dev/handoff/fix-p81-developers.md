# Wave 81 handoff: developers (SEC-AUD-335, D-838; findings: the authorize picker, the plan view's Tabs)

T = 09:35 on 2026-11-16. Clock times: no new item from 11:35, checkpoint 11:55, finished 12:20, hard limit 12:35.
Actual: code and spec ready 09:50; lock taken 09:57 (waited behind arcade, tools, library); base runs 09:58 and 09:59; change measured 10:02 and 10:03; stack stopped and lock released 10:03. One lock round, about 6 minutes. Checks and handoff done by 10:10.

## Done
- `apps/web/src/developers/developers.css` (the only source file changed; no `.tsx`, no i18n): three small rules after `.dev-actions`.
  1. `.dev-actions > button, .dev-actions > a { align-self: end }`: a button in a row is its own height (the kit's 36 px), never the row's. Right for a row with no hint under its control, and for a button wrapped onto a line of its own.
  2. `form.dev-actions > button, form.dev-actions > a { margin-block-end: 2px }`: centres the 36 px button on the 40 px control beside it.
  3. `form.dev-actions { container-type: inline-size }` and `@container (min-width: 860px) { .dev-actions > :is(label, div) ~ :is(button, a) { align-self: start; margin-block-start: 23px } }`: in a form wide enough to hold its fields and the button on one line, a field's hint line would pull an `end`-aligned button down under the control (measured: button 46..82 beside controls 21..61). So the button starts under the label instead: 23 px = kit label 15 + field gap 6 + 2 (centring). 860 px is what the widest form (Rich Presence: file, name, type, button, about 845) needs; the row is 950 wide at 1280. Below 860 the button wraps onto its own line and rule 1 applies, with no extra gap.
- Why not one `align-self: end` rule (the contract's first choice): measured on the base with the rule injected (variant A): in Rich Presence and Emojis at 1280 the button is natural height (36) but sits at 46..82, level with the hint line and 25 px below the control (21..61). Variant D (a `margin-block-start` on every button after a field, no container query) put 23 px of extra gap above the Upload button on a phone, where the Emojis button stands alone (row 190 -> 213). Hence the container query.
- `e2e/tests/p81-developers-shots.spec.ts` (local only: `SR_P81_DEVELOPERS=1`, `SR_P81_DEVELOPERS_OUT`; `SR_P81_BASE=1` = base case: logs the rows, injects variants A and D, asserts nothing).
- Commits on `fix/p81-developers`: the spec, then `developers.css` and this handoff, subjects `SEC-AUD-335: ...`.

## What I saw on the base (I ran it: `SR_P81_BASE=1`, developers.css unchanged, Lounge Dark)
- Rich Presence at 1280: row 950x82, "Upload" top 0, height 82 (fault confirmed). Emojis: same, 82. The Installation tab's tester form ("Invite tester" beside "Username"): 61 high (the same fault; no hint, the row is label + control). At 375: Rich Presence button 61 high beside the Type select (169..230); Emojis button already 36 (alone on its line).
- Every other `.dev-actions` row (overview, general x5, oauth2 x4, bot, installation x2, activities x2, review, the emoji list's Rename/Delete spans): the button is already 28 or 36 (small or normal kit button), alone in its row.

## Button boxes before and after (relative to the row's top; 1280 unless noted; Light the same)
| row | before (y, h) | after (y, h) | control beside it (y, h) | level (top / bottom) |
|---|---|---|---|---|
| Rich Presence "Upload" | 0, 82 | 23, 36 | name field 21, 40 | +2 / -2 |
| Emojis "Upload" | 0, 82 | 23, 36 | name field 21, 40 | +2 / -2 |
| Installation "Invite tester" | 0, 61 | 23, 36 | username field 21, 40 | +2 / -2 |
| Rich Presence "Upload" at 375 (beside Type) | 169, 61 | 192, 36 | select 190, 40 | +2 / -2 |
| Emojis "Upload" at 375 (own line) | 154, 36 | 154, 36 | none | unchanged |
| Invite tester at 375 (own line) | 69, 36 | 69, 36 | none | unchanged |

Kit button elsewhere on the page: 36 (the same). Widths unchanged (81.8, 112.8). Every other `.dev-actions` button, at 1280 and 375 (list above): moved by 0 px, kept its height. The two forms' rows on a phone where the button is alone (Emojis, Invite tester) grew by 2 px (the 2 px margin); nothing else moved. The spec asserts this on the change run (button at most 46 px; within 2 px of the control, top and bottom): passed.

## Findings
1. **The authorize page's server picker: reached.** An application (every application has a bot) with the redirect URI `<vite origin>/p81-callback` (PATCH `/applications/:id`), the Owner's servers, `/oauth2/authorize?client_id=<id>&scope=bot&response_type=code&redirect_uri=<uri>`. The picker (`#dev-auth-guild`) against the kit text field read on the same application's General page: Lounge Dark background `rgb(43, 45, 49)`, colour `rgb(219, 222, 225)`, border `rgb(63, 65, 71)`, radius 8, block 40, equal; Lounge Light `rgb(242, 243, 245)`, `rgb(49, 51, 56)`, `rgb(201, 204, 209)`, 8, 40, equal; right edge 870 of 1280 and 334 of 375; no sideways scroll at either size. "Authorize" was not pressed. No fault.
2. **The plan view's Tabs at 375: reached**, with the bridge status, features list, preflight and plan answered by `page.route` (12 roles, 14 channels, a thread: 7 tabs, strip 309 px wide, 1019 px of content). At load: `data-overflow="end"`, "Roles (12)" 33..130 inside the list 33..342, right fade on "Threads". After pressing the last tab: `start`, scrollLeft 710, "Blockers (0)" 228..342 inside, left fade. Dark and Light the same figures; no sideways scroll. No fault, nothing for `Tabs.module.css`.
3. **Not mine, seen on the base and after:** the Activities tab (`?tab=activities`) scrolls sideways by 107 px at 375 (`documentElement.scrollWidth - clientWidth`); the Rich Presence asset list row at 375 wraps the asset name mid-word and "Small image" onto two lines (cramped, not broken). Both are for a later Developers wave (D-817 names the Library, Cinema, Arcade and Study; the Developers pages are not in it).

## Tests
`node scripts/check-css-tokens.mjs` (73 clean), `node scripts/i18n-audit.mjs --max 0` (TOTAL 0), prettier --check (both files), e2e `tsc` clean, `developers/__tests__/portal.test.tsx` as a single file: 45 passed. Shots spec: the base run completed (faults logged: I watched the 82 px button there); the change run: every assertion passed except the Activities sideways scroll (finding 3, present on the base).

## Imports and chunks
`developers.css` is imported only by `DevelopersRoot.tsx` and `Authorize.tsx`, both loaded with `lazyNamed(() => import(...))` in `developers/routes.tsx` (the Developers lazy chunk). Nothing first paint or a public page loads: no web build, no bundle check run.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-developers` (22 PNGs + `rows-after.json`) and `.../p81-developers-base` (14 PNGs: 6 base, 8 with the injected variants, + `rows-base.json`). **I opened the 24 the task lists, all of them**: Rich Presence and Emojis after (Dark and Light, 1280 and 375: 8), the base Dark 1280 pair (2), the General and OAuth2 rows after (2), the authorize picker (4), the plan tabs (4), the Upload click-through (filled and listed, Light, both sizes: 4). The 8 variant shots and the 4 base phone/row shots in the base folder: not opened (their figures are in the log above). Click-through: picked a file and "Small image", pressed Upload, the asset is listed with a toast, at both sizes (asserted in the spec too). Themes were read back from the room's root before each shot (`lounge-dark`, `lounge-light`).

## Proposed inventory row
SEC-AUD-335 (closed): "A button in a Developers form row (Rich Presence and Emojis 'Upload', the tester invite) was stretched to the row's height, about 80 px, by the field's hint line. `developers.css`: a row's button keeps its own height (`align-self: end`), and in a form wide enough to hold its fields and the button on one line it starts under the label so it is level with the controls (D-838)."

## Changelog sentence
The Developers pages' "Upload" and "Invite tester" buttons are a normal button's size and sit level with their fields.

## AGENTS.md
No sentence needed.

## Owner questions
1. The 23 px and 860 px in the new rule are measured for the kit's current label (15 px), field gap (6 px) and the widest form; a change to the kit field's label size needs them re-measured. Default: kept (a kit field that exposes its control row would remove the numbers; the kit is fenced this wave).
2. The Activities tab's 107 px sideways scroll at 375 and the cramped asset list row at 375. Default: left for a Developers wave.

## Clean-up
Stack stopped by the PIDs listening on 5593 and 8593 (metrics 9293 went with the server); `sr_p81_developers_ui` dropped; `p81-developers-bin` deleted (`p80-base-bin` untouched); heavy lock released at 10:03. The scratch folder `SIDEROOM_wt/p81-developers-scratch` (server and Vite logs, scripts; no secrets) is outside the repository. No stash seen or touched. `git status` before the commit: only `developers.css`, the spec and this handoff; nothing under `apps/web/src/developers/data/`.
