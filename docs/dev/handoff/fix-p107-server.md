# fix/p107-server handoff (code builder, pod staff)

## Clock
Started 18:38; both files READY by about 18:50. No job submitted (sheets only).

## Done
- `D-1022 admin.css (READY)`, `D-1022 server-settings.css (ALL READY)`.

## Not done
Nothing outstanding. Nothing measured: staffshots measures.

## REACH table (all phone-only, `max-width: 720px`, except B)
| control | sized now | page / how opened | cure | what moves |
|---|---|---|---|---|
| Default permissions `.ss-grow > button.ss-name` | `.ss-row > button.ss-name` phone block, server-settings.css (Roles.tsx:289, inside `.ss-grow`, missed it) | Server settings > Roles | A.2 pattern: selector extended, padding 12 / margin -12 | tap area 20 -> 44; row height unchanged (check) |
| role chip `button.ss-role-chip` | `.ss-role-chip` server-settings.css (padding 1px 8px) | Server settings > Channels > a channel > Permissions | A.2 (chips wrap in 4 px gaps, A.1 would take neighbours' points): min-block-size 44 | that line +24 px per chip row. Members chips are spans: untouched |
| `.ss-card > a` (Open roles, Open channels) | `.ss-card > a` server-settings.css; only WatchLibrary.tsx uses it | Server settings > Watch library | A.1: `::after` inset-block -12 (gap 12 above, padding 16 below hold it) | nothing moves |
| Members bulk bar | `.ss-toolbar` (gap 8) | Server settings > Members, tick a member | A.3: row-gap 16 on `.ss-toolbar[role='toolbar']` (only that bar has the role) | bar grows only when it wraps |
| Bans filter rows | `.adm-actions` (gap 8), Bans.tsx figures | Admin > Bans | A.3: row-gap 16 on `.adm-dash-section > .adm-actions[role='group']` (admin.css, end block). `.adm-dash-section` is also used by moderation DashboardTab.tsx: no `.adm-actions` group inside it was found, staffshots please confirm | rows only when wrapped |
| Live TV daypart select vs "Remove daypart" | no rule; fieldset `.adm-cin-daypart`, RulesEditor.tsx (no sheet lays it out, so admin.css end block) | Admin > Cinema > Live TV > channel editor | gap: `margin-block-end: 8px` on the last `.adm-cin-grid` of the fieldset | +8 px under the last grid of each daypart |
| `.ss-select` | server-settings.css | Members page (sort, etc.), also Admin | B: color-mix 65 rest, hover, ring already existed; no placeholder on a select; 44 px on a phone already existed | border colour at every width |

## FINDERS
`git grep` for ss-select, ss-role-chip, ss-name, adm-cin-daypart, adm-dash-section, server-settings.css, admin.css in `e2e/` and `__tests__`: none (apart from retired shots specs). Both sheets are lazy chunks (settings and admin routes); `.ss-select` also reaches Admin (Accounts, Limits, audit log) via the shared sheet.

## JOBS
none.

## Checks
prettier --check on both files: pass. `node scripts/check-css-tokens.mjs`: pass (73 files clean). Unit tests rendering these (livetv.test.tsx etc.): not run (CSS is not applied in jsdom, no .tsx changed).

## Closing text / changelog
Server settings and Admin: the last small controls reach 44 px on a phone, and the Members selects take the kit field's border and hover.

## Owner questions
none.

## PROCESSES
strays: none found
