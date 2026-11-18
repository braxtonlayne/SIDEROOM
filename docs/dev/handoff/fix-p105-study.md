# fix-p105-study (wave 105, solo; D-1010; SEC-AUD-477)

## Clock
Started 15:30, base job finished about 15:33, handoff 15:35. Nothing built.

## Result: the fault is not as the task says
In built mode (the wave's base bundle) the Study's two dialogs already follow the page behind them, in every config. No app file changed, no build, no `shoot after`.

Why: `study/routes.tsx` wraps the Study in `LoungeThemeProvider` (section "lounge", `userGlobal` = the member's MAIN theme from `useAppliedTheme()`). The kit `Modal` portals into the nearest `ThemeProvider` root, which is that same provider, so the dialog gets the page's tokens. The Study follows the Lounge's main theme, NOT `sectionThemes.library` (that belongs to `LibraryRoot`). The p103 note "in the light config the Study dialog stays dark" came from a config with main = lounge-dark and only `sectionThemes` light: the page stayed dark too, so the dialog correctly stayed dark. Wrapping the dialogs in `ThemedOverlayRoot` would be wrong (it always takes the Lounge theme and adds a second provider for nothing).

Open question for Fable (default picked): should the Study follow `sectionThemes.library` instead of the main theme? Default: no, it is a Lounge-family page; leave as is.

## Done (commits)
- D-1010 e2e/tests/p105-study-shots.spec.ts (new), this handoff.

## Not done
Step 2 (wrapper in StudyDialogs.tsx, block in study.css): not needed, figures below.

## JOBS
- 0021-study-shoot: shoot base, submitted 1795033939, started 1795033973, ended 1795034006 (epoch s), rc 0, waited about 34 s, ran about 33 s.

## Figures (base, figures-base.json in the shots folder; page theme = first data-theme in the document, dialog theme = nearest data-theme above the dialog)
| config | page | dialog | panel | title colour | title on panel | fields |
|---|---|---|---|---|---|---|
| 1024 shipped (main lounge-dark) edit/viewer | lounge-dark | lounge-dark | rgb(43,45,49) | rgb(219,222,225) | 10.21 | same as panel |
| 1024 light (main + library lounge-light) | lounge-light | lounge-light | rgb(255,255,255) | rgb(49,51,56) | 12.64 | rgb(242,243,245) |
| 375 light | lounge-light | lounge-light | rgb(255,255,255) | rgb(49,51,56) | 12.64 | rgb(242,243,245) |
| 375 shipped | lounge-dark | lounge-dark | rgb(43,45,49) | rgb(219,222,225) | 10.21 | same as panel |
| 1024 library-only light (main dark, library light) | lounge-dark | lounge-dark | rgb(43,45,49) | rgb(219,222,225) | 10.21 | same as panel |
Focus inside the dialog on open and Escape closes it: true in all 10 cases. No sideways scroll in any.
The viewer shows "Could not open that file" (the fake bucket answers 404): the dialog chrome and the notice are measured, not a player.

## Shots (all opened after the job; base only, no after)
- base-study-edit-1024-light.png: light page behind a light dialog (white panel, grey fields, dark labels in small caps, blue Save). Follows the page.
- base-study-viewer-1024-light.png: light dialog, title "heat", red-edged notice on light grey. Follows.
- base-study-edit-375-light.png: light dialog full form, Cover select full width. Follows.
- base-study-edit-1024-shipped.png: dark page, dark dialog. Follows.
- base-study-viewer-1024-shipped.png: dark dialog with dark notice. Follows.
- base-study-edit-1024-libonly.png and base-study-viewer-1024-libonly.png: page and dialog both dark (the Study ignores the library section theme). Same as shipped.
- base-study-viewer-375-light.png: light dialog over a light page. Follows.

## Who imports my files
Nothing of the app changed. (For the record: `StudyDialogs.tsx` is imported by `StudyFiles.tsx` only, inside the lazy Study chunk.)

## Checks (own tree, last commit)
- e2e tsc (`npx tsc --noEmit -p tsconfig.json`): rc 0. prettier --check on the spec: rc 0. check-css-tokens: clean, rc 0. i18n-audit --max 0: TOTAL 0, rc 0.
- eslint on the spec: not applicable (e2e is outside the web package's eslint base: "File ignored"). Unit tests of StudyDialogs: not run (file unchanged). web tsc job: not run (no app change).

## Closing text for 477
SEC-AUD-477: not reproduced in built mode. The Study sits in the Lounge theme provider and its dialogs portal into it, so they draw the page's theme (shipped dark and Lounge Light, 1024 and 375). The earlier "stays dark" was a page that was dark too (library section theme only). Closed with figures; no code.

## Proposed rows
None from the dialogs. Design note only: the Study ignores `sectionThemes.library` (owner question above).

## Changelog sentence
None (no change).

## PROCESSES
strays: none found
