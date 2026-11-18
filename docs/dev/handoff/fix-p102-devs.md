# fix-p102-devs: SEC-AUD-453 (D-958), devs: 5 of 6 raw selects moved to the kit's NativeSelect

Code builder of pod pages. Stack-free: no spec, no shots, no lock. Clock: started 09:30, REACH table and four files moved by about 09:45 (real clock).
Files: `apps/web/src/developers/tabs/{Commands,OAuth2,Media}.tsx`, `apps/web/src/developers/{Authorize,Reference}.tsx` (Reference unchanged), this handoff. No sheet, no fragment, no kit file.

## REACH table
The area (Developers portal) has no `sectionThemes` entry in `settings/user/prefs.ts`; it follows the account theme (shots builder: read `data-theme` back). All pages are in the signed-in gated bundle, lazy chunks, none loaded by first paint. Sheet: `apps/web/src/developers/developers.css` (lines 193 to 221): `.dev-select {max-width:100%}` and the kit-look rules `:where(.dev-main,.dev-authorize) select:not([class])` / `select.dev-select` (40 px min, padding 8/12, border, radius). After the move the select carries the kit's module class and no `dev-select`, so these rules no longer match it (the kit look replaces them). The one element-name rule that still matches is `.dev-root select:focus-visible` (line 77, outline 2px/offset 2px, outline only, no box change). No e2e or `__tests__` file finds any of these selects by `dev-select`, by tag or by class (checked with grep over `e2e/` and `apps/web/src`); the specs use ids (`#dev-auth-guild`) or labels.

| # | File:line (base) | Id / label | Page and how to open it | Sheet rule now | Flex label? | Result |
|---|---|---|---|---|---|---|
| 1 | tabs/Commands.tsx:114 | `#cmd-type`, label "Type" (`developers.commands.typeLabel`) | `/developers/applications/<appId>?tab=commands` (any developer application the account owns; earlier spec: `p80-developers-shots.spec.ts` line 268 goes there) | `select.dev-select` (css 199) | no: `.dev-field` is a flex column, the span is a column item (stretches like the select did) | moved, class `dev-select` dropped |
| 2 | tabs/Commands.tsx:179 | `#opt-type-<i>`, label "Option N type" | same page; click "Add option" on a slash command (type 1, the default) so the option fieldset shows | `select.dev-select` | no (`.dev-field` column) | moved, class dropped |
| 3 | tabs/OAuth2.tsx:245 | `#<useId>-redirect`, label "Redirect URI" (`developers.oauth2.redirectUri`) | `/developers/applications/<appId>?tab=oauth2`, the URL generator card (always shown there; needs no redirect URI: the "none" option is always present; for more than one option the app needs `redirect_uris`). `UrlGenerator` is also used with `installOnly` by the Installation tab, where this select is not rendered. Earlier spec: `p83-developers-shots.spec.ts` line 420 | `select.dev-select` | no (`.dev-field` column) | moved, class dropped |
| 4 | tabs/Media.tsx:288 | no id, no class; its `<label className="dev-file">` text "Type" (`developers.media.assetType`) wraps it (implicit label) | `/developers/applications/<appId>?tab=richpresence`, the add-asset form (`RichPresenceTab`, line 215) which is a `form.dev-actions` (flex row, container query at 860 px). Earlier spec: `p83-developers-shots.spec.ts` line 364 | `select:not([class])` (css 198) | label is `display:grid` (css 657), not flex; the form is the flex row and the label its item, unchanged. Watch its inline size: the label sizes to content, and the wrapper is `inline-size:100%` | moved |
| 5 | Authorize.tsx:210 | `#dev-auth-guild`, label "Add to server" | `/oauth2/authorize?client_id=<appId>&scope=bot` (needs a released-or-owned bot app and a signed-in account with at least one guild; earlier specs: `p81-developers-shots.spec.ts` line 379, `developer-gate.spec.ts` lines 77 and 157 use `#dev-auth-guild`) | `select.dev-select` inside `.dev-authorize` | no (`.dev-field` column) | moved, class dropped |
| 6 | Reference.tsx:105 | `aria-label` "Area" (`developers.reference.area`), no id, no class | `/developers/docs/api-reference` (OpenAPI list; needs `/api/v1/openapi.json`). Earlier spec: `p80-developers-shots.spec.ts` line 270 | `select:not([class])` (css 198) | no label, but it is a **flex-wrap toolbar item**: `.pkg-toolbar` (`packages/packages.css` 2 to 11) is `display:flex; flex-wrap:wrap`, the search is `flex:1 1 200px`, the select was content-sized beside it | **left raw ("layout risk, not moved")**: the kit's wrapper span is `inline-size:100%` with `flex-basis:auto`, so it would take a full line and wrap under the search field instead of sitting beside it. No prop or class reaches the wrapper and I may not edit a sheet. If the shots builder wants to measure it anyway, put it in the spec and it can be moved in a second commit. |

Who imports (git grep): `Commands`, `OAuth2`, `Media` are imported only by `developers/AppDetail.tsx`, which is the lazy `AppDetailPage` chunk (`routes.tsx`); `Authorize.tsx` is the lazy `AuthorizePage`/`AuthorizedPage` chunk of `routes.tsx`; `Reference.tsx` is imported by `developers/Docs.tsx` (lazy `DocsIndexPage`/`DocPageView`). **None is loaded by first paint or by a public page.** `Authorize.tsx` is not public: its preview call answers 401 for a signed-out visitor and the page then leaves with `exitToLogin(here)`; it lives in the gated bundle (D-385), so it is moved.

## Status of the work
- All five moves are the diff: import line, `<select` to `<NativeSelect`, `className="dev-select"` removed where there was one, closing tag. Id, value, onChange, children untouched. Diffs read line by line.
- Unit tests (unedited): `authorize.test.tsx`, `media.test.tsx`, `portal.test.tsx` in `apps/web/src/developers/__tests__/`: 64 passed (09:33). prettier: unchanged (clean). eslint on the four files: clean.
- TYPECHECK: `tc.sh run devs bash -c "cd .../p102-devs/apps/web && npx tsc --noEmit -p ."`, 09:34 to 09:37, no output (no errors; the exit shown was the pipe's tail, the output empty). i18n audit `node scripts/i18n-audit.mjs --max 0`: TOTAL 0, exit 0 (09:37). `node scripts/check-doc-links.mjs`: doc links ok, exit 0 (09:37).
- STACK-ROUND: none (code builder). heavy.sh: not used, no hold in my name.

## Still raw (for the SEC-AUD-453 text)
`Reference.tsx` Area filter (flex-wrap toolbar, would wrap under the search field). Everything else in my files moved (pending the shots builder's figures).

## Proposed rows
None yet.

## Changelog sentence
The Developers portal's command type, option type, redirect URI, rich presence asset type and add-to-server pickers now use the kit's select.

## Owner questions
None.
