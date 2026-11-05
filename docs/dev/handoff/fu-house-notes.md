# Follow-up: house-notes copy (FU-25..FU-28, branch fix/fu-house-notes)

Implements `docs/dev/handoff/fu4-fable-decisions.md` (H4, H6, H7, H11).

## Done
- FU-25/26/27 (one commit): `house_notes_md.rs` and the Vite mirror `houseNotesMd.ts` now keep Unicode letters and digits in heading ids (per-character lower case), join quote lines with `<br>`, and add `nofollow` to `rel`. Fixtures (`basic`, `links`, `headings`) and the `web_gate` assertions updated (`### Über uns` -> `op-über-uns`).
- FU-26 (H7): `EntranceMarkdown` gains `hardBreaks`; `houseNotesHeadingId(text, seen)` is exported beside `defaultHeadingId`; the house-notes block in Admin > Entrance passes both. The hint gains "A line break inside a paragraph stays a line break." Vitest added (entranceCopy, p13entrance).
- Decisions-log rows FU-25..FU-28 and the copied `fu4-fable-decisions.md`.

## Deviation
`headingId` now receives a second argument, a per-render `seen` set (default and runbook ignore it). A factory that kept state itself double-counted ids under React StrictMode (preview showed `op-privacy-2`, `-3`); verified in the browser that the preview ids now equal the served page ids.

## Tests run
cargo fmt --check, clippy -D warnings, `sr-server` lib house_notes tests and `web_gate` (24 passed); web tsc, eslint, prettier on touched files, vitest (vite/, routes/entrance, p13entrance: 105 passed); web build, check-entrance-isolation, check-bundle-budget, i18n-audit (0); `playwright test --list` loads 902 tests. (Prettier reports `src/admin/features/__fixtures__/registry.json`, untouched by this branch.)

## Visual check
Shots in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/fu-hn/`: pub-house-notes and admin-entrance (desktop/phone, light/dark) plus `admin-entrance-preview.png`, with seeded copy (Unicode heading, two-line quote, two equal headings). Served ids: op-welcome-to-the-back-booth, op-ünï-und-café-2, op-privacy, op-privacy-2; the preview gives the same. Stack stopped, DB dropped.

## Owner questions
None new (the `seen`-set signature is the only choice made).

## Resume here
Nothing pending; ready to merge.
