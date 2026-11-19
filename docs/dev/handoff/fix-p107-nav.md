# fix/p107-nav handoff

## Clock times
Started 18:37, both sheets done and checked by about 18:40 (the machine clock). Code builder: no spec, no shots, no job.

## Done (commits)
- `D-1022 user-settings.css (READY)`: `.us-search-input` takes the field contract (D-1024): rest `border-color: color-mix(in srgb, var(--ui-border), var(--ui-text-faint) 65%)` after the existing 1px border, hover `var(--ui-text-faint)`, `:focus-visible` ring with `outline-offset: 1px`, placeholder `var(--ui-text-faint)` and `opacity: 1`; new `@media (max-width: 720px)` block directly after it: `min-block-size: 44px`.
- `D-1022 Settings.module.css (ALL READY)`: in the sheet's own `@media (max-width: 720px)` block, `.navItem { min-block-size: 44px }`, placed after `.sidebar > *`. The list's 2 px gap is not changed (44 px items at a 46 px pitch, so no neighbour's tap area is shared). Padding, type size and the active look are untouched. The text sits centred (a button centres its content).

## Not done
Nothing left. The gzip size of the CSS file is not measured: it needs a build, so ask staffshots for the file name.

## REACH table
| control | rule now (this tree) | page / how opened | cure | what moves |
|---|---|---|---|---|
| `.navItem` (button in the settings drawer's list) | `packages/ui/src/primitives/Settings/Settings.module.css` `.navItem` (line 63): `padding: 6px 10px`, about 32 px tall | User settings, Server settings, Admin: open the picker (`.navToggle`) on a phone, 375 | A.2 (grows, in the 720 px block) | each item 32 to 44 px; the drawer is 12 px longer per item and scrolls (max-height 60vh) |
| `.us-search-input` | `apps/web/src/settings/user/user-settings.css` `.us-search-input` (line 318), 36 px | User settings sidebar head (`SettingsSearch`) | B, and A.2 on a phone | at every width: rest border, hover, ring, placeholder; on a phone 36 to 44 px; the sidebar head is 8 px taller |

Dependants of `.navItem` (`git grep`): only `SettingsLayout.tsx:221`, used by `UserSettings`, `ServerSettings` and `AdminApp`. The danger item shares the class and grows too. `.us-search-input`: only `SettingsSearch.tsx:34`.

## FINDERS
- Classes `navItem`: e2e `p92-probe-shots.spec.ts` (318, 344), `p93-kit-shots.spec.ts` (267, 275), `p93-settings-shots.spec.ts` (150, 354). All retired shots specs finding by `button[class*="navItem"]` and by text: not affected by a height. No CI spec or unit test drives them.
- `us-search`: no e2e or unit test hit besides the component.
- Tests reading the files as text: `Settings.module.css` is named only by `SettingsLayout.tsx`; `user-settings.css` is imported by `parts.tsx`, `support/Leaderboards.tsx`, `support/SupportPage.tsx`. The only `color-mix` tests are `entranceDangerText.test.ts` (danger only, not affected) and `tokens-contrast.test.ts` (65 matches what I used). None edited.

## JOBS
None.

## Figures
Not measured (no job); staffshots measures. May change on a phone: nav items 44, search 44. Everything else at 1024, 1280 and the desktop sidebar is untouched (the new height rules sit inside `max-width: 720px`).

## Shots
None (builder of sheets only).

## Who imports, which chunk
`Settings.module.css` is imported by `SettingsLayout.tsx` only, which is exported from `packages/ui/src/index.ts` and used by the settings and admin surfaces. Whether the settings and admin routes are lazy chunks was not verified by me (my grep for `lazy(` found no hit in `apps/web/src`); staffshots's build will show it. `user-settings.css` is loaded through `parts.tsx` (settings) and by the support and leaderboards pages.

## Checks (on this tree)
- prettier on both files: clean, exit 0.
- `node scripts/check-css-tokens.mjs`: 73 files clean, exit 0.
- vitest `apps/web/src/settings/user/__tests__/p11userset.test.tsx`: 13 passed. `packages/ui/src/primitives/settings-extras.test.tsx`: 8 passed.

## Closing text for rows, new rows, changelog
- Closing: the settings drawer's rows and the settings search are 44 px tall on a phone (D-1022), and the search field has the kit field's manners (D-1024).
- Proposed row: none, unless staffshots finds one.
- Changelog sentence: Settings rows and the settings search field are now 44 px tall on a phone, and the search field shows the kit field's border, hover and focus ring.

## Owner questions
None.

## PROCESSES
strays: none found (strays.sh p107-nav, no job of mine queued or running).
