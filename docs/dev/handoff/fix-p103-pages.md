# fix-p103-pages (wave 103, pod lounge, code builder, stack-free)

## Clock
- 11:56 start. REACH table committed first (before 12:35).

## REACH table
| # | file:line | label / id | page and how to open | sheet rule now | theme | raw / kit | fit | found by class/id in e2e or tests |
|---|---|---|---|---|---|---|---|---|
| 1 | `developers/Reference.tsx:105` | `aria-label` developers.reference.area (Area) | Developer portal > Reference tab (`/developers/reference`), in flex-wrap `.pkg-toolbar` beside the Search `TextInput` | `:where(.dev-main,.dev-authorize) select:not([class])` (developers.css:198-221), plus `.dev-root select:focus-visible` (:76) | Developers page (public-style shell, `sideroom` token set) | raw | content (sits in a flex-wrap row) | p102-pages-shots.spec.ts:534 `section.dev-card select.dev-select, section.dev-card select` (the other cards); Area by aria-label |
| 2 | `settings/server/bridge/ConflictLog.tsx:109` | label "Kind" (`.ss-field-label`) | Server settings > Discord bridge > Conflict log, in `.ss-toolbar` beside the "Include resolved" Switch | `.ss-select` (server-settings.css:222, members' file, read only) | Lounge light/dark | raw (class `ss-select`) | content (shrink-to-fit label in flex toolbar; wave 102 fault 246 to 222 px) | none by class (`git grep ss-select` in e2e/tests: only a selector list in p84/p85 that does not name this select) |
| 3 | `developers/Authorize.tsx:210` | Add to server | public authorize page | `.dev-authorize select:not([class])` does not reach it (kit module class) | public | already kit | n/a | not mine, never changed |
| 4 | `developers/tabs/Commands.tsx:114,178`, `Media.tsx:288`, `OAuth2.tsx:246` | Type, Option type, Asset type, Redirect URI | Developers tabs | none reach them (kit module class) | | already kit | n/a | not mine |
| 5 | `settings/server/bridge/BridgeSection.tsx:342-364`, `PlanView.tsx:452,463` | bridge pickers | Server settings > bridge | none | | already kit | n/a | not mine |

`.dev-select` has no user in any `.tsx` under `apps/web/src` (only the CSS and an e2e selector list that also lists plain `select`). After row 1 moves, no raw `<select>` is left under `apps/web/src/developers`.

## Done (all in the clock window 11:56 to 12:02)
- Merged `fix/p103-kit` (872f2312) into this branch for `fit`.
- `SEC-AUD-465 bridge/ConflictLog (READY)`: Kind is `<NativeSelect fit="content">` inside `label.ss-field-label`; `className="ss-select"` dropped (no test or spec finds it by that class; `git grep ss-select` shows only the sheet, Members, ChannelTabs, admin comment and selector lists in p84/p85).
- `SEC-AUD-465 developers/Reference (READY)`: Area is `<NativeSelect fit="content">`, `aria-label` kept.
- `SEC-AUD-464 developers.css (ALL READY)`: deleted `.dev-select` and the four `:where(.dev-main, .dev-authorize) select...` rules (the former `:193-221`) and `.dev-root select:focus-visible,` from the focus list. Dependants: `git grep "<select" apps/web/src/developers` is empty, `.dev-select` has no `.tsx` user (only the stale e2e selector `p102-pages-shots.spec.ts:534`, which also lists plain `select`, an earlier spec, not edited). Authorize.tsx (public page) already uses the kit's NativeSelect (module class), so it never matched `select:not([class])`: nothing it needs was removed. The other dev-tab selects (Commands, Media, OAuth2) are kit already.
- Not done: nothing. `server-settings.css` `.ss-select` untouched (members'); it still serves Members.tsx and ChannelTabs.tsx.

## Who imports
- `developers/Reference.tsx`: imported by `Docs.tsx` and `gatewayDocs.test.ts`; `developers/routes.tsx` loads `DevelopersRoot` with `lazyNamed(import)`: lazy chunk. `developers.css` is imported by the developers shell (lazy) and the authorize page (public, loads the sheet; only dead rules were removed).
- `bridge/ConflictLog.tsx`: imported by `BridgeSection.tsx`, imported by `ServerSettings.tsx`, loaded by `routes/settingsServer.tsx` lazily: Settings chunk. No first-paint file touched.

## Checks (own tree, last commit; all exit 0)
- prettier --check (3 files), 12:00
- `apps/web/node_modules/.bin/eslint` on Reference.tsx, ConflictLog.tsx: exit 0
- vitest `developers/__tests__/gatewayDocs.test.ts` + `authorize.test.tsx`: 19 passed, 11:58
- `node scripts/check-css-tokens.mjs` 0; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; `node scripts/check-doc-links.mjs` ok
- ## TYPECHECK: `tc.sh run pages bash -c "cd .../p103-pages/apps/web && npx tsc --noEmit -p ."`, finished 12:02, exit 0 (once, after last code edit).
- ## STACK-ROUND: none (code builder). No lock held.

## Proposed row text
- SEC-AUD-462 (pages part): Reference Area and bridge Conflict log Kind use `NativeSelect fit="content"`.
- SEC-AUD-464: `developers.css` no longer styles a `select` by element name; the rules for bare selects and `.dev-select` are removed.
- SEC-AUD-465: the last two raw selects under the Developers portal and the bridge Conflict log are the kit's.
- Changelog: The API reference's Area filter and the Discord bridge conflict log's Kind filter now use the shared select field, and the Developers sheet no longer styles selects by element name.

## Owner questions
None. For loungeshots: measure Kind (was 246 to 222 px, label stacked, Switch moved 78.6 px) and Area in `.pkg-toolbar`; keyboard-test one of them.

## Put back (FAULT from loungeshots)
- `bridge/ConflictLog.tsx` Kind: back to raw `<select className="ss-select">`, file identical to `integration/phase103` (49ca8946). Cause: the label touched the select (about 1 px against 9 px) and the font fell from 14 px bold to 12 px; the base `.ss-select` rule gave the gap and size, and `server-settings.css` is members'. The "Done" and proposed-row text above for ConflictLog (SEC-AUD-462/465) no longer apply; Reference Area and the `developers.css` change stand. Prettier and eslint on the file: exit 0.
