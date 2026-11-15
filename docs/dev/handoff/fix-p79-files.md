# Wave 79, files builder: SEC-AUD-311 (D-813, D-814)

Status: done. One lock round plus re-runs inside the same hold (15:22 to about 15:36). Stack stopped, `sr_p79_files_ui` dropped, `p79-files-bin` deleted, lock released.

## Done (commits on `fix/p79-files`)
- `21745292` and `af59e934`: `e2e/tests/p79-files-shots.spec.ts` (local only, `SR_P79_FILES=1`, `SR_P79_BASE=1` for the base case); `developers/developers.css`: the 11 `var(--sr-radius, N)` are `var(--ui-radius, N)`; `admin/admin.css` (the one Admin CSS file I touched): rules for `label.ss-field` (the file-field labels of Entrance, Subtitles, Guide import).
- No `.tsx`, no i18n fragment, no build (see "Loading").

## The eight inputs (all reached; probe = a span carrying the kit's secondary tokens in the same place)
Seeded through the API as Owner: a draft title with asset id 4242, two TV channels (manual, imported_guide), an application, a server. Faked with `page.route` (said plainly): the channel's command list (`/channels/:id/application-commands`, one command with an attachment option), the ring feed (`GET /rings` answered 404 on this stack so no tray showed: faked as `[]`), and the subtitle track list of asset 4242 (a placeholder id, so the form never rendered: faked as `{tracks: []}`).

| input | reached how | look equals probe | inside panel |
|---|---|---|---|
| Admin > Cinema title, Artwork tab | `/admin/cinema-catalog`, Edit, tab Artwork | yes, 4 of 4 | yes |
| Admin > title, Video tab, subtitles | tab "Video and transcoding", Subtitles button | yes, 4 of 4 | yes after the fix below |
| Admin > Live TV channel logo | `/admin/cinema-live-tv`, Open channel | yes | yes |
| Admin > Live TV guide import | imported_guide channel, tab Guide | yes | yes |
| Developers > Media (Emojis) | `/developers/applications/:id?tab=emojis` | yes | yes |
| Developers > Media (Rich Presence assets) | `?tab=richpresence` | yes | yes |
| App command attachment (`.lng-appform-file`) | composer `/snapshot` + Enter (faked command list) | yes | yes (right edge = panel edge) |
| Rings composer (kit `TextInput type=file`) | Home, "Add a ring", Photo | yes | yes |

Measured on all 36 probes (8 inputs x Lounge Dark/Light x 1280x800/375x812, plus the Entrance's 4): background, colour, border colour, radius, font and cursor equal the probe; no page scrolls sideways; text contrast 9.4 to 12.6 (above 4.5); `data-theme` read back (`lounge-dark` / `lounge-light`). The spec passed.

Click-through (`setInputFiles`, Dark desktop): a PNG on the channel logo ("Logo uploaded." shows), a `.vtt` on subtitles (name shows), a PNG on the command form ("Selected: pic.png"). All read right.

## Faults
- **Fixed (mine): the subtitles input was wider than its panel on a phone.** With my label rule alone (flex, wrap) the input ended at 375 in a panel ending at 306. Fixed in `admin.css`: `label.ss-field { min-inline-size: 0; max-inline-size: 100% }` and `.adm-sub-add > label.ss-field { flex: 1 1 100% }`. After: input right edge 325 = panel right edge. The spec's assertion caught it.
- **Fixed: Admin > Entrance label touches the button.** Base: 0 px between label text and button. The rule: `label.ss-field` in `apps/web/src/admin/admin.css` (the class had no rule anywhere; `admin.css` is imported by `admin/ui.tsx` and `SchemaForm.tsx`, Admin chunk, not a public page). After: 12 px on desktop, the text on its own row above the button on a phone. Subtitles and Guide import use the same class and get the gap too.
- **Fixed: Developers corners.** Base under Lounge Dark and Lounge Light: `.dev-card` radius 2px, a kit button in it 8px. After: 8px and 8px, both themes (asserted).
- **Listed, not changed (not mine):**
  1. Developers > Rich Presence, phone: the "Type" select is the browser's default (white, black frame) on a dark page (`developers-assets-dark-phone.png`); a bare `<select>` in `developers/tabs/Media.tsx`. Proposed rule in `developers.css`: `.dev-actions select { background: var(--ui-bg-raised); color: var(--ui-text); border: 1px solid var(--ui-border); border-radius: var(--ui-radius, 6px); padding: 6px 10px; font: inherit }`, or the kit `Select`.
  2. Developers phone: the top nav clips "Documentation" and "Back to the lounge" (same bar problem as D-817 fixed for the Cinema and Library).
  3. Admin > Live TV: "Remove logo" is a small ghost button, lighter than the picker button beside it. Cosmetic.
- No input is hidden by or overflows its panel after the changes.

## Loading
`developers.css` is imported by `DevelopersRoot.tsx` (lazy via `developers/routes.tsx`) and `Authorize.tsx` (lazyNamed, route `/oauth2/authorize`, an in-app route under the signed-in root, not a public page). `admin.css`: `admin/ui.tsx` and `admin/SchemaForm.tsx` (Admin chunk). Neither is first paint nor public: no build, no three checks. The change names no section and no theme.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-files`: 43 after; `base/`: 30 base)
After: all 43 opened by me (the eight inputs in Dark and Light at both sizes, the Developers home and an application page in Dark and Light at desktop, three file-set shots). `base/` (same spec, the two CSS changes held back with `git apply -R`): I opened only the Entrance light desktop one; the base proof for the radius is the logged computed values (`card 2px, button 8px`), for the label gap the probe (`labelGap 0`). The base run did not reach the title (my first spec lacked the Artwork tab) or the ring composer (feed 404): no CSS of theirs changed, so no base is needed.
Not shot: the disabled state (tail item): no time, and no input disabled on these pages.

## Tests and checks
`p79-files-shots.spec.ts` with `--project=chromium --reporter=line`: the base run passed (3.8 m, faults logged, the Developers radius not asserted in base mode); the final run passed (1.3 m). Also prettier on my files, `check-css-tokens` clean, `i18n-audit --max 0` = 0, e2e `tsc` clean. No vitest.

## Docs
None name any of this. Nothing changed.

## Proposed
- Inventory row **SEC-AUD-311** (closed): the eight remaining visible file inputs (Admin title artwork and subtitles, TV logo and guide import, Developers Media x2, app command attachment, Rings composer) are shot in Dark and Light at both sizes and draw the kit's secondary button; the Developers' corners are the kit's radius; Admin file-field labels have a gap; the subtitles input stays inside its panel on a phone.
- Threat-model rows: none. Fragments: none.
- Changelog sentence: "The Admin's file pickers sit apart from their labels, and the Developers pages use the app's corner radius."
- Owner questions: (1) fix the Developers' bare select and its phone nav next wave? (2) `GET /rings` answered 404 on the p72 binary stack: confirm rings sit behind a switch that is off by default.

## Cleanup
Stack stopped by PID (ports 5576, 8576, 9276 free), `sr_p79_files_ui` dropped, `p79-files-bin` deleted, `e2e/test-results` removed, scratch diff removed, lock released. No stash touched.
