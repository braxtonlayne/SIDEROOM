# Wave 76, dock: SEC-AUD-298 (D-794), one bar per upload in the upload dock

## Done (branch `fix/p76-dock`)
- `apps/web/src/review/UploadDock.tsx` (the `UploadDock` component only): the overall line and bar render only when `tasks.length >= 2`. The line is the existing string `review.upload.total` as visible text (`<span id>` from `useId`); the overall `<progress>` is named by `aria-labelledby` pointing at that span, so there is one announcement (the visible line is not a second one: it is the name). Each row is a text block (name, state, "choose the file again", same order, wrapping) and under it one line with the row's `<progress>` (all `aria-label`s unchanged) and the buttons at the end. A finished row has no button, so its bar takes the whole line.
- `apps/web/src/review/review.css`: new classes only: `rv-upl-total`, `rv-upl-list`, `rv-upl-row`, `rv-upl-text`, `rv-upl-line`, `rv-upl-bar`, `rv-upl-buttons` (logical properties, no colours, `inline-size: 100%` on the bars). `.rv-changes` and every existing rule are untouched; the dock's `<ul>` no longer uses `rv-changes` (the Admin review pages and the Cinema lists still do).
- No string added or changed, no fragment, no Rust, no generated file.
- Commits: `SEC-AUD-298: one bar per upload in the upload dock`, `... prettier on UploadDock`, and the shots-spec commit.

## Confirmed on the base
- Vitest `p76UploadDock.test.tsx`: both tests failed on the base code (one upload drew two `<progress>`; no "All uploads" line was visible text). They pass now.
- Shots spec on the base: NOT run (no time for a second round). Base figures from the CSS and from wave 74's shot `visual-sweep-shots/p74-uploads/ended-no-file-desktop-lounge-light.png` (opened): the overall bar is a default 160px bar under the heading, the row's bar a default 160px bar placed after the long name (left edge at the end of the name, not at the row's start edge).

## After (spec measurements, desktop 1280x800)
- One running upload, long name: 1 `<progress>`, no "All uploads" text; row box left 307, width 907; bar left 307, width 813 (the rest is the buttons); bar top 421 under the name bottom 408.
- Phone 375x812, three uploads: rows left 50, width 275; bars left 50, widths 181 (180 for rows without difference); each bar under its text; no horizontal overflow (asserted for every shot).
- The spec asserts for each shot: no page overflow, bar left edge = row left edge (< 1px), bar top >= name bottom, bar width > 40% of the row.

## Shots
- Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p76-dock`, 30 PNG (24 asked + `back-to-one-*`, 6, the dock after two of three uploads are cancelled: the overall bar goes). States: `one-running`, `three` (running, paused, ended with wave 73's long sentence), `ended-no-file` (D-787 sentence in the same run of text), `finished`, `back-to-one`; each at desktop and phone, Editorial, Lounge Light, Lounge Dark. I opened all 30 (the first batch of the first run was re-shot after I added the account theme; the 30 listed are the final run).
- Finding: the Library submission page draws in its own dark palette whatever the account theme (wave 74's shots are the same), so the three themes look identical. The spec sets the account theme by `PATCH /users/@me/settings` as well as localStorage.
- Click-through (in the spec, every combination): pause, resume, cancel of two rows; the overall bar disappears when one upload is left.
- Cinema draft page (`cinema/submit/DraftPage.tsx:288`): not shot; it draws the same component with the Cinema manager.
- Playwright: run with `--project=chromium` (the config also lists firefox and webkit, which are not installed).

## First paint
`review.css` is imported by `review/shared.tsx`, `NamingGuide`, `RequestsBoard`, `ReviewExtras` and `admin/sections/IngestSources.tsx`; `UploadDock` by `library/Submissions.tsx` and `cinema/submit/DraftPage.tsx`. Their routes are lazy (`library/routes.tsx`, `cinema/routes.tsx` use `lazyNamed`; the Admin sections are the Admin chunk). Nothing first paint loads: no web build run.

## Tests run
- Vitest: `p76UploadDock.test.tsx` (2), `uploadDock.test.tsx` (3), `p74UploadDock.test.tsx` (3): all pass; no existing assertion changed.
- Web `tsc --noEmit` clean; eslint on the changed files clean; prettier check clean; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; `node scripts/check-css-tokens.mjs` covers `review.css` (71 files clean).
- Shots spec on the real stack (own server copy, DB `sr_p76_dock_ui`, ports 8541/5541/9241): passed, 1.1 min. Server copy ran fine from `p72-base-bin`.

## Docs
Searched `docs deploy README.md apps/web/src` for "overall bar" and REV-UP-002: only the historical handoff `docs/dev/handoff/fix-p74-uploads.md` (the "two bars" note; left alone) and the inventory row REV-UP-002 (not mine; still true: "global progress dock"). The code comment in `UploadDock.tsx` header ("one overall bar") now reads "one overall bar" for two or more uploads: left as is. `apps/web/src/storage/browser/UploadPanel.tsx:3` is the storage builder's.

## Proposed inventory row (after SEC-AUD-297)
**SEC-AUD-298** [FE] The upload dock draws one progress bar per upload; the overall bar and its visible line ("All uploads: N%") only when the dock holds two or more uploads; in a row the name and state wrap and the bar sits under them at full width with the buttons at its end *(wave 76, D-794: `review/UploadDock.tsx`, `review/review.css` `rv-upl-*`; tests `p76UploadDock.test.tsx`, local shots spec `e2e/tests/p76-dock-shots.spec.ts`)*. Closes the "two bars" note of `fix-p74-uploads.md`.
Threat-model rows: none.

## Changelog sentence
The upload list shows one progress bar per file, under its name; an "All uploads" bar with its percentage appears only when more than one file is uploading.

## Cleanup
Stack stopped by PID, heavy lock released, scratch DB `sr_p76_dock_ui` dropped, `p76-dock-bin` deleted. No stash touched. Untracked scratch logs removed.

## Not done / owner questions
- Base shots spec not run (above).
- Owner question: the Library page ignores the Lounge Light/Editorial theme (always dark); intended? (Not this wave's item.)
