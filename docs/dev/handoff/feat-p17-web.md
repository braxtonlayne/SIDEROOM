# Handoff: feat/p17-web

Web and docs only. Items from `docs/dev/answers-followups-p16.md` (owner round M-01 to M-18).

## Done (all ticked in the checklist with an italic note)

- **M-18** Final `/support` copy (D-298): strings replaced in `apps/web/src/i18n/messages/en/p16-uploads-1.ts`, the draft comment removed, the draft note removed from inventory DON-UI-001. `docs/specs/donations.md` had no draft note; `support.test.tsx` asserts headings and buttons only, so it needed no change.
- **M-13** Weekly lineup signpost (web part, D-303): the manual kind notice is reworded; the Now control tab of a manual channel shows the hint and a "Switch to Imported guide" button with a confirm dialog (blocks are kept until the next publish). The switch sends the existing `PUT /admin/tv/channels/{id}` with `kind: imported_guide` (the API is PUT, not PATCH). Strings in `p17-web-1.ts`. Spec kinds table and TV-ADM-003 updated. Test in `admin/__tests__/livetv.test.tsx`.
- **M-04** Closed artwork items: the profile editor already hides the decoration picker when the catalogue is empty (`CatalogPicker`); now tested. Notes added on LNG-PRF-009 (P11-SHL-3, D-290) and LNG-PRF-008 (P11-CLI-1, D-291); DEV-APP-007 ticked (server installs only, D-305); DEV-SDK-003 points at M-16. The LNG-EXP-004 wording is left to M-03.
- **M-09** Composer highlight backdrop (D-299): `lounge/ComposerBackdrop.tsx`, `tokenizeComposer` in `composerText.ts`, CSS in `lounge.css` (logical properties), wired in `Composer.tsx`. Off above 2,000 characters. Tests in `lounge/__tests__/p17ComposerBackdrop.test.tsx` (tokens, aria-hidden, scroll mirror, axe, length cut-off). LNG-CMP-001 reworded and ticked.
- **M-01** `THIRD_PARTY_NOTICES.md`: UnRAR licence text (copied from `unrar_sys` 0.5.8, with the `cargo deny` comment), plus pending sections for JASSUB, libass, FreeType, fribidi, the JASSUB default font and `rvfc-polyfill` (GPL-3.0, D-309). Copy at `apps/web/public/THIRD_PARTY_NOTICES.md` with `scripts/check-notices-copy.mjs` (run in `web.yml`; `--write` refreshes). `/help` has a "Third-party notices" link. The server image copies the file to `/usr/share/doc/sideroom/` through `deploy/docker/server.Dockerfile` (release.yml builds that Dockerfile, so it needed no change). `sw.js` bypasses the path so the link is not answered with the app shell. Doc 03 row added.

## Still open
- Nothing from the assigned list. The JASSUB branch (M-08) must replace the "pending" lines in `THIRD_PARTY_NOTICES.md` with the real versions and licence texts, then run `node scripts/check-notices-copy.mjs --write`.

## Decisions
- The Now control hint uses a button (not a link) for the kind switch; the confirm dialog says blocks are kept.
- Backdrop pills carry no horizontal padding so text never shifts against the textarea.

## Tests run
See the final report of the agent (typecheck, eslint, prettier, vitest, build and bundle budget, doc link check).

## Owner questions
- None. Note for later: the GPL caveat of D-309 applies to the notices entry for `rvfc-polyfill`.
