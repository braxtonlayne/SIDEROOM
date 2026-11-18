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
